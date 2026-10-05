//! Running ffmpeg: finding it and its version, ffprobe's duration, progress, failures and
//! cancel, against `examples/fake_ffmpeg.rs`; and a real MP3 when `GEZIK_TEST_FFMPEG` names an
//! ffmpeg.

use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::convert::ffmpeg::{Ffmpeg, duration_us, find_ffmpeg, run_preset};
use gezik_core::batch::convert::{MediaPreset, ffmpeg_args};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-ffmpeg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The fake ffmpeg that `cargo test` builds with the examples (as a program, not a test:
/// `target/<profile>/examples/fake_ffmpeg[.exe]`).
fn fake() -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let name = format!("fake_ffmpeg{}", std::env::consts::EXE_SUFFIX);
    let path = deps.parent().unwrap().join("examples").join(name);
    assert!(
        path.is_file(),
        "{} is missing: run `cargo test -p gezik-batch` (it builds the examples; `--test ffmpeg` alone does not)",
        path.display()
    );
    path
}

/// Copies of the fake as `ffmpeg` (and `ffprobe` with `probe`) in a folder of their own, with
/// `banner` as what `-version` prints.
fn install(d: &Path, banner: Option<&str>, probe: bool) -> PathBuf {
    let exe = |name: &str| d.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(fake(), exe("ffmpeg")).unwrap();
    if probe {
        std::fs::copy(fake(), exe("ffprobe")).unwrap();
    }
    if let Some(banner) = banner {
        std::fs::write(d.join("banner.txt"), banner).unwrap();
    }
    exe("ffmpeg")
}

fn ff(d: &Path) -> Ffmpeg {
    let path = install(d, None, true);
    find_ffmpeg(&d.join("data"), Some(&path)).expect("the fake is found")
}

fn args(list: &[&str], output: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = list.iter().map(OsString::from).collect();
    args.push(output.as_os_str().to_os_string());
    args
}

fn never() -> bool {
    false
}

#[test]
fn configured_ffmpeg_is_found_with_ffprobe_and_version() {
    let d = dir("find");
    let path = install(&d, None, true);
    let found = find_ffmpeg(&d.join("data"), Some(&path)).unwrap();
    assert_eq!(found.ffmpeg, path);
    assert_eq!(found.ffprobe, Some(d.join(format!("ffprobe{}", std::env::consts::EXE_SUFFIX))));
    assert_eq!(found.version, Some((9, 0)));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn versions_are_read_from_real_banners() {
    let banners: [(&str, Option<(u32, u32)>); 4] = [
        (
            "ffmpeg version 9.0.2-essentials_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers\n",
            Some((9, 0)),
        ),
        ("ffmpeg version n9.0.2-22-g1a2b3c4d5e-20261001 Copyright (c) 2000-2026 the FFmpeg developers\n", Some((9, 0))),
        ("ffmpeg version 7.1.1 Copyright (c) 2000-2025 the FFmpeg developers\nbuilt with Apple clang\n", Some((7, 1))),
        // A build from git is still ffmpeg (fine for audio and video); its version is unknown.
        ("ffmpeg version 2025-05-19-git-c55d65ac0a-full_build-www.gyan.dev Copyright (c) 2000-2025\n", None),
    ];
    for (n, (banner, version)) in banners.into_iter().enumerate() {
        let d = dir(&format!("banner{n}"));
        let path = install(&d, Some(banner), false);
        let found = find_ffmpeg(&d.join("data"), Some(&path)).unwrap_or_else(|| panic!("{banner}"));
        assert_eq!(found.version, version, "{banner}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn a_program_that_is_no_ffmpeg_is_passed_over() {
    let d = dir("not-ffmpeg");
    let path = install(&d, Some("usage: something else 1.2\n"), false);
    // Whatever PATH holds, the configured program is not what is found.
    let found = find_ffmpeg(&d.join("data"), Some(&path));
    assert!(found.is_none_or(|f| f.ffmpeg != path));
    // A relative path in settings is not taken either.
    let found = find_ffmpeg(&d.join("data"), Some(Path::new("ffmpeg")));
    assert!(found.is_none_or(|f| f.ffmpeg.is_absolute()));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn duration_comes_from_ffprobe() {
    let d = dir("duration");
    let ff = ff(&d);
    let input = d.join("in.mp4");
    std::fs::write(&input, "12.345678").unwrap();
    assert_eq!(duration_us(&ff, &input), Some(12_345_678));
    std::fs::write(&input, "N/A").unwrap();
    assert_eq!(duration_us(&ff, &input), None);
    std::fs::write(&input, "").unwrap();
    assert_eq!(duration_us(&ff, &input), None);
    let without = Ffmpeg { ffprobe: None, ..ff };
    std::fs::write(&input, "3.0").unwrap();
    assert_eq!(duration_us(&without, &input), None);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn progress_rises_and_reaches_one() {
    let d = dir("progress");
    let ff = ff(&d);
    let output = d.join("out.tmp");
    let mut seen = Vec::new();
    // 6 blocks of 0.25 s against 1 s: the last ones go past the end and are held at 1.0.
    let list = ["--fake-blocks", "6", "--fake-na", "-f", "mp4"];
    run_preset(&ff, args(&list, &output), Some(1_000_000), &mut |p| seen.push(p), &never).unwrap();
    assert!(output.is_file());
    assert!(!seen.is_empty());
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert!(seen.iter().all(|p| (0.0..=1.0).contains(p)), "{seen:?}");
    assert_eq!(seen.last(), Some(&1.0));
    assert!(seen.contains(&0.5), "{seen:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn without_a_duration_there_is_no_progress() {
    let d = dir("no-duration");
    let ff = ff(&d);
    let output = d.join("out.tmp");
    let mut calls = 0;
    run_preset(&ff, args(&["--fake-blocks", "3"], &output), None, &mut |_| calls += 1, &never).unwrap();
    assert_eq!(calls, 0);
    assert!(output.is_file());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_ends_ffmpeg_within_a_second() {
    let d = dir("cancel");
    let ff = ff(&d);
    let output = d.join("out.tmp");
    let progressed = Cell::new(false);
    let asked = Cell::new(None::<Instant>);
    let stop = || {
        if progressed.get() && asked.get().is_none() {
            asked.set(Some(Instant::now()));
        }
        asked.get().is_some()
    };
    let list = ["--fake-blocks", "2", "--fake-hang"];
    let started = Instant::now();
    let err =
        run_preset(&ff, args(&list, &output), Some(10_000_000), &mut |_| progressed.set(true), &stop).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    let asked = asked.get().expect("stop was asked after progress");
    assert!(asked.elapsed() < Duration::from_secs(1), "{:?}", asked.elapsed());
    assert!(started.elapsed() < Duration::from_secs(10));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_failure_carries_the_last_twenty_error_lines() {
    let d = dir("failure");
    let ff = ff(&d);
    let output = d.join("out.tmp");
    let list = ["--fake-blocks", "1", "--fake-stderr-lines", "30", "--fake-exit", "3"];
    let err = run_preset(&ff, args(&list, &output), Some(1_000_000), &mut |_| {}, &never).unwrap_err();
    let text = err.to_string();
    assert!(text.starts_with("ffmpeg failed (exit code 3): error line 11\n"), "{text}");
    assert!(text.ends_with("error line 30"), "{text}");
    assert!(!text.contains("error line 10\n"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn progress_is_asked_for_when_the_arguments_lack_it() {
    let d = dir("adds-progress");
    let ff = ff(&d);
    let output = d.join("out.tmp");
    // The fake fails (exit 9) without `-progress`; `ffmpeg_args` always has it.
    let list = ffmpeg_args(MediaPreset::Mp3, &d.join("in.wav"), &output);
    assert!(list.iter().any(|a| a == "-progress"));
    run_preset(&ff, list, None, &mut |_| {}, &never).unwrap();
    run_preset(&ff, args(&["--fake-blocks", "1"], &output), None, &mut |_| {}, &never).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

// ---- A real ffmpeg (GEZIK_TEST_FFMPEG) ---------------------------------------------------

#[test]
fn real_ffmpeg_makes_an_mp3_with_progress() {
    let Some(path) = std::env::var_os("GEZIK_TEST_FFMPEG").map(PathBuf::from) else { return };
    let d = dir("real");
    let ff = find_ffmpeg(&d.join("data"), Some(&path)).expect("GEZIK_TEST_FFMPEG is an ffmpeg");
    let wav = d.join("sine.wav");
    let sine =
        ["-nostdin", "-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"];
    run_preset(&ff, args(&sine, &wav), None, &mut |_| {}, &never).unwrap();
    let duration = duration_us(&ff, &wav);
    if ff.ffprobe.is_some() {
        let us = duration.expect("ffprobe reads the WAV's duration");
        assert!((900_000..=1_100_000).contains(&us), "{us}");
    }
    let output = d.join("sine.mp3.tmp");
    let mut seen = Vec::new();
    run_preset(&ff, ffmpeg_args(MediaPreset::Mp3, &wav, &output), duration, &mut |p| seen.push(p), &never).unwrap();
    let bytes = std::fs::read(&output).unwrap();
    assert!(bytes.len() > 1000);
    assert!(bytes.starts_with(b"ID3") || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0));
    if duration.is_some() {
        assert_eq!(seen.last(), Some(&1.0), "{seen:?}");
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    }
    // A missing input fails with ffmpeg's own words.
    let err =
        run_preset(&ff, ffmpeg_args(MediaPreset::Mp3, &d.join("missing.wav"), &output), None, &mut |_| {}, &never)
            .unwrap_err();
    assert!(err.to_string().contains("missing.wav"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}
