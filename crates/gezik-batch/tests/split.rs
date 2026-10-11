//! Split File and Join Files through the engine (spec 11 §12.4–12.5): part names and sizes,
//! the .crc file, a join checked by CRC before anything is placed, missing parts, a name
//! already there meeting the conflict list, and undo.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::tasks::{join_chain, join_label, split_chain, split_label};
use gezik_ops::{Decision, Engine, Event, JobId, Report, Settings};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-split-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    gezik_platform::trash::purge_at_thread_end(&d);
    d
}

fn bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 % 251) as u8).collect()
}

/// Runs `job` to its end, answering conflicts with `decision` (none: their defaults); returns
/// the report and how many conflicts were asked.
fn finish(engine: &Engine, job: JobId, decision: Option<Decision>) -> (Report, usize) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut asked = 0;
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    asked += conflicts.len();
                    engine.decide(job, conflicts.iter().map(|c| decision.unwrap_or(c.decision)).collect());
                }
                Event::Finished { job: j, report } if j == job => return (report, asked),
                _ => {}
            }
        }
        assert!(Instant::now() < deadline, "job {job} did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The names in `d`, sorted.
fn names(d: &Path) -> Vec<String> {
    let mut out: Vec<String> =
        std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    out.sort();
    out
}

fn split(engine: &Engine, source: &Path, into: &Path, size: u64, decision: Option<Decision>) -> (Report, usize) {
    let tasks = split_chain(source.to_path_buf(), into.to_path_buf(), size);
    finish(engine, engine.submit_chain(tasks, Some(split_label(source))), decision)
}

fn join(engine: &Engine, part: &Path, into: &Path, decision: Option<Decision>) -> (Report, usize) {
    let tasks = join_chain(part.to_path_buf(), into.to_path_buf());
    finish(engine, engine.submit_chain(tasks, Some(join_label(part))), decision)
}

#[test]
fn a_file_splits_into_numbered_parts_and_joins_back() {
    let d = dir("round-trip");
    let data = bytes(250);
    std::fs::write(d.join("film.bin"), &data).unwrap();
    let parts = d.join("parts");
    std::fs::create_dir(&parts).unwrap();
    let engine = Engine::new(Settings::default(), || {});
    let (report, asked) = split(&engine, &d.join("film.bin"), &parts, 100, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(asked, 0);
    assert_eq!(names(&parts), ["film.bin.001", "film.bin.002", "film.bin.003", "film.bin.crc"]);
    let sizes: Vec<u64> = ["001", "002", "003"]
        .iter()
        .map(|n| std::fs::metadata(parts.join(format!("film.bin.{n}"))).unwrap().len())
        .collect();
    assert_eq!(sizes, [100, 100, 50]);
    let crc = crc32fast::hash(&data);
    assert_eq!(
        std::fs::read_to_string(parts.join("film.bin.crc")).unwrap(),
        format!("filename=film.bin\r\nsize=250\r\ncrc32={crc:08X}\r\n")
    );
    assert_eq!(engine.undo_label().as_deref(), Some("Split film.bin"));
    // Joined from any part, into another folder, under the .crc's name.
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let (report, _) = join(&engine, &parts.join("film.bin.002"), &out, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(names(&out), ["film.bin"], "only the joined file: no staging left");
    assert_eq!(std::fs::read(out.join("film.bin")).unwrap(), data);
    // Undo takes each job's output away.
    for _ in 0..2 {
        let (undo, _) = finish(&engine, engine.undo().unwrap(), None);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
    }
    assert!(names(&out).is_empty() && names(&parts).is_empty());
    assert_eq!(std::fs::read(d.join("film.bin")).unwrap(), data, "the source stays");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_damaged_part_places_nothing_and_touches_no_existing_file() {
    let d = dir("damaged");
    std::fs::write(d.join("a.dat"), bytes(300)).unwrap();
    let parts = d.join("parts");
    std::fs::create_dir(&parts).unwrap();
    let engine = Engine::new(Settings::default(), || {});
    split(&engine, &d.join("a.dat"), &parts, 128, None);
    let mut second = std::fs::read(parts.join("a.dat.002")).unwrap();
    second[5] ^= 0xFF;
    std::fs::write(parts.join("a.dat.002"), second).unwrap();
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("a.dat"), "old").unwrap();
    let (report, asked) = join(&engine, &parts.join("a.dat.001"), &out, Some(Decision::Replace));
    assert_eq!(asked, 0, "checked before the conflict list: nothing is replaced");
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(report.failures[0].message.contains("does not match a.dat.crc"), "{}", report.failures[0].message);
    assert_eq!(names(&out), ["a.dat"], "no staging left");
    assert_eq!(std::fs::read_to_string(out.join("a.dat")).unwrap(), "old");
    // A size that differs fails the same way.
    std::fs::write(parts.join("a.dat.crc"), "filename=a.dat\r\nsize=999\r\ncrc32=00000000\r\n").unwrap();
    let (report, _) = join(&engine, &parts.join("a.dat.crc"), &out, None);
    assert!(report.failures.iter().any(|f| f.message.contains("is missing")), "{:?}", report.failures);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_missing_part_stops_before_anything_is_written() {
    let d = dir("missing");
    std::fs::write(d.join("x.iso"), bytes(500)).unwrap();
    let engine = Engine::new(Settings::default(), || {});
    split(&engine, &d.join("x.iso"), &d, 100, None);
    std::fs::remove_file(d.join("x.iso.003")).unwrap();
    std::fs::remove_file(d.join("x.iso")).unwrap();
    let (report, _) = join(&engine, &d.join("x.iso.001"), &d, None);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].message.contains("Part x.iso.003 is missing"), "{}", report.failures[0].message);
    // The last part gone: the .crc's size tells.
    std::fs::rename(d.join("x.iso.004"), d.join("x.iso.003")).unwrap();
    std::fs::remove_file(d.join("x.iso.005")).unwrap();
    let (report, _) = join(&engine, &d.join("x.iso.001"), &d, None);
    assert!(report.failures[0].message.contains("Part x.iso.004 is missing"), "{}", report.failures[0].message);
    assert!(!d.join("x.iso").exists());
    assert!(names(&d).iter().all(|n| !n.starts_with(".gezik-")), "{:?}", names(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn names_already_there_meet_the_conflict_list() {
    let d = dir("conflicts");
    let data = bytes(150);
    std::fs::write(d.join("b.bin"), &data).unwrap();
    std::fs::write(d.join("b.bin.001"), "older part").unwrap();
    let engine = Engine::new(Settings::default(), || {});
    let (report, asked) = split(&engine, &d.join("b.bin"), &d, 100, Some(Decision::Skip));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(asked, 1);
    assert_eq!(std::fs::read_to_string(d.join("b.bin.001")).unwrap(), "older part", "skipped: kept");
    std::fs::write(d.join("b.bin.001"), &data[..100]).unwrap();
    // Joined where the original still is: asked, Skip keeps it.
    let (report, asked) = join(&engine, &d.join("b.bin.001"), &d, Some(Decision::Skip));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(asked, 1);
    assert_eq!(std::fs::read(d.join("b.bin")).unwrap(), data);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn without_a_crc_file_the_join_says_it_was_not_checked() {
    let d = dir("no-crc");
    std::fs::write(d.join("n.txt.001"), "abc").unwrap();
    std::fs::write(d.join("n.txt.002"), "def").unwrap();
    let engine = Engine::new(Settings::default(), || {});
    let (report, _) = join(&engine, &d.join("n.txt.002"), &d, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read_to_string(d.join("n.txt")).unwrap(), "abcdef");
    assert!(report.skipped.iter().any(|s| s.message.contains("not checked")), "{:?}", report.skipped);
    let _ = std::fs::remove_dir_all(&d);
}
