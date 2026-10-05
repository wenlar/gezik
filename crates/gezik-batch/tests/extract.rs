//! Extracting through the engine: the chain of `ExtractTask` and `PlaceTask`, where things
//! land, conflicts, passwords, cancel, undo and the 7-Zip fallback.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::tasks::{ExtractTo, extract_chain, extract_label};
use gezik_ops::{Answer, Engine, Event, JobId, PendingDeletes, Question, Report, Settings};
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-extract-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending-deletes")), ..Settings::default() }, || {})
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// A zip of (name, contents); a name ending in `/` is a folder.
fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zw = ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in entries {
        if let Some(folder) = name.strip_suffix('/') {
            zw.add_directory(folder, SimpleFileOptions::default()).unwrap();
        } else {
            zw.start_file(*name, SimpleFileOptions::default()).unwrap();
            zw.write_all(data).unwrap();
        }
    }
    zw.finish().unwrap();
}

/// Names in `d` that a staging folder or a temporary copy would have.
fn leftovers(d: &Path) -> Vec<String> {
    std::fs::read_dir(d)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".gezik-"))
        .collect()
}

/// Submits the chain for `archives` and runs it to the end; `on` sees each of its events
/// (conflicts are answered with their defaults unless `on` decided them).
fn extract(
    engine: &Engine,
    archives: Vec<PathBuf>,
    to: ExtractTo,
    seven_zip: Option<PathBuf>,
    on: impl FnMut(&Engine, JobId, &Event) -> bool,
) -> (Report, Vec<Event>) {
    let label = extract_label(&archives);
    let job = engine.submit_chain(extract_chain(archives, to, seven_zip), Some(label));
    finish(engine, job, on)
}

/// Runs `job` to the end; `on` returns true when it handled an event itself.
fn finish(engine: &Engine, job: JobId, mut on: impl FnMut(&Engine, JobId, &Event) -> bool) -> (Report, Vec<Event>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = Vec::new();
    loop {
        for event in engine.drain() {
            let handled = on(engine, job, &event);
            match &event {
                Event::Conflicts { job: j, conflicts } if *j == job && !handled => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, .. } if *j == job && !handled => engine.answer(job, Answer::Cancel),
                Event::Finished { job: j, report } if *j == job => {
                    let report = report.clone();
                    seen.push(event);
                    return (report, seen);
                }
                _ => {}
            }
            seen.push(event);
        }
        assert!(Instant::now() < deadline, "job {job} did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn nothing(_: &Engine, _: JobId, _: &Event) -> bool {
    false
}

fn undo(engine: &Engine) -> Report {
    let job = engine.undo().expect("something to undo");
    finish(engine, job, nothing).0
}

#[test]
fn extract_here_single_root_goes_in_directly() {
    let d = dir("single-root");
    let archive = d.join("photos.zip");
    zip(&archive, &[("Fotolar/", b""), ("Fotolar/a.jpg", b"jpeg")]);
    let engine = engine(&d);
    let (report, _) = extract(&engine, vec![archive.clone()], ExtractTo::Smart(d.clone()), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("Fotolar/a.jpg")), "jpeg");
    assert!(!d.join("photos").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    assert_eq!(report.results, [d.join("Fotolar")]);
    assert_eq!(engine.undo_label().as_deref(), Some("Extract photos.zip"));
    let report = undo(&engine);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!d.join("Fotolar").exists(), "the placed folder went to the trash");
    assert!(archive.is_file());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn extract_here_many_roots_go_into_a_folder() {
    let d = dir("many-roots");
    let archive = d.join("bundle.zip");
    zip(&archive, &[("a.txt", b"a"), ("b/", b""), ("b/c.txt", b"c")]);
    let engine = engine(&d);
    let (report, _) = extract(&engine, vec![archive.clone()], ExtractTo::Smart(d.clone()), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("bundle/a.txt")), "a");
    assert_eq!(read(&d.join("bundle/b/c.txt")), "c");
    assert!(!d.join("a.txt").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    undo(&engine);
    assert!(!d.join("bundle").exists(), "the folder made for it goes with undo");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn folder_and_into_go_where_they_say() {
    let d = dir("folder-into");
    let archive = d.join("one.zip");
    zip(&archive, &[("Fotolar/a.jpg", b"jpeg")]);
    let engine = engine(&d);
    let (report, _) = extract(&engine, vec![archive.clone()], ExtractTo::Folder(d.clone()), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("one/Fotolar/a.jpg")), "jpeg");
    std::fs::create_dir(d.join("into")).unwrap();
    let (report, _) = extract(&engine, vec![archive], ExtractTo::Into(d.join("into")), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("into/Fotolar/a.jpg")), "jpeg");
    assert!(leftovers(&d).is_empty() && leftovers(&d.join("into")).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn several_archives_are_one_action() {
    let d = dir("several");
    zip(&d.join("a.zip"), &[("a.txt", b"a")]);
    zip(&d.join("b.zip"), &[("b.txt", b"b")]);
    let engine = engine(&d);
    let archives = vec![d.join("a.zip"), d.join("b.zip")];
    let (report, _) = extract(&engine, archives, ExtractTo::Smart(d.clone()), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("a.txt")), "a");
    assert_eq!(read(&d.join("b.txt")), "b");
    assert_eq!(engine.undo_label().as_deref(), Some("Extract 2 archives"));
    undo(&engine);
    assert!(!d.join("a.txt").exists() && !d.join("b.txt").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn extract_into_an_existing_folder_merges_and_undoes() {
    let d = dir("merge");
    let archive = d.join("photos.zip");
    zip(&archive, &[("Fotolar/a.jpg", b"new")]);
    std::fs::create_dir(d.join("Fotolar")).unwrap();
    std::fs::write(d.join("Fotolar/old.jpg"), "old").unwrap();
    let engine = engine(&d);
    let (report, _) = extract(&engine, vec![archive], ExtractTo::Smart(d.clone()), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("Fotolar/a.jpg")), "new");
    assert_eq!(read(&d.join("Fotolar/old.jpg")), "old");
    undo(&engine);
    assert!(!d.join("Fotolar/a.jpg").exists(), "the extracted file went to the trash");
    assert_eq!(read(&d.join("Fotolar/old.jpg")), "old", "what was there stays");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn extract_conflict_asks() {
    let d = dir("conflict");
    let archive = d.join("one.zip");
    zip(&archive, &[("a.txt", b"from the archive")]);
    std::fs::write(d.join("a.txt"), "already here").unwrap();
    let engine = engine(&d);
    let (report, events) = extract(&engine, vec![archive], ExtractTo::Smart(d.clone()), None, nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let conflicts = events
        .iter()
        .find_map(|event| match event {
            Event::Conflicts { conflicts, .. } => Some(conflicts.clone()),
            _ => None,
        })
        .expect("the conflict is asked");
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].target, d.join("a.txt"));
    assert_eq!(read(&d.join("a.txt")), "already here", "skipped by default");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn password_question_flow() {
    let d = dir("password");
    let archive = d.join("secret.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let opts = SimpleFileOptions::default().with_aes_encryption(AesMode::Aes256, "Passw0rd");
        zw.start_file("a.txt", opts).unwrap();
        zw.write_all(b"first secret").unwrap();
        zw.start_file("b.txt", opts).unwrap();
        zw.write_all(b"second secret").unwrap();
        zw.finish().unwrap();
    }
    let engine = engine(&d);
    let mut questions = Vec::new();
    let (report, _) =
        extract(&engine, vec![archive.clone()], ExtractTo::Smart(d.clone()), None, |engine, job, event| {
            let Event::Question { question, .. } = event else { return false };
            questions.push(question.clone());
            let password = if questions.len() == 1 { "wrong" } else { "Passw0rd" };
            engine.answer(job, Answer::Text(password.into()));
            true
        });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        questions,
        [
            Question::Password { archive: archive.clone(), retry: false },
            Question::Password { archive: archive.clone(), retry: true },
        ]
    );
    assert_eq!(read(&d.join("secret/a.txt")), "first secret");
    assert_eq!(read(&d.join("secret/b.txt")), "second secret");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_mid_extract_leaves_nothing() {
    let d = dir("cancel");
    let archive = d.join("big.zip");
    {
        // A large entry, then an encrypted one: the job is cancelled at its first progress,
        // or at the latest when the password is asked; either way in the middle.
        let mut zw = ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("big.bin", stored.large_file(true)).unwrap();
        let piece = vec![7u8; 1 << 20];
        for _ in 0..64 {
            zw.write_all(&piece).unwrap();
        }
        let secret = SimpleFileOptions::default().with_aes_encryption(AesMode::Aes256, "pw");
        zw.start_file("z.txt", secret).unwrap();
        zw.write_all(b"z").unwrap();
        zw.finish().unwrap();
    }
    let engine = engine(&d);
    let (report, _) =
        extract(&engine, vec![archive.clone()], ExtractTo::Smart(d.clone()), None, |engine, job, event| match event {
            Event::Progress { job: j, .. } | Event::Question { job: j, .. } if *j == job => {
                engine.cancel(job);
                true
            }
            _ => false,
        });
    assert!(report.cancelled);
    let mut left: Vec<String> =
        std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    left.retain(|name| name != "pending-deletes");
    assert_eq!(left, ["big.zip"], "nothing in the target, no staging folder");
    let pending = PendingDeletes::new(d.join("pending-deletes"));
    assert!(pending.load().is_empty() && pending.copies().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn rare_format_without_seven_zip_says_so() {
    let d = dir("needs-7z");
    let archive = d.join("old.lzh");
    std::fs::write(&archive, b"not something Gezik reads").unwrap();
    let engine = engine(&d);
    let (report, _) = extract(&engine, vec![archive.clone()], ExtractTo::Smart(d.clone()), None, nothing);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, archive);
    assert!(report.failures[0].message.contains("7-Zip needed to open this kind of archive"), "{:?}", report.failures);
    assert!(leftovers(&d).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

/// The 7-Zip installed on this machine, if any.
fn installed_seven_zip() -> Option<PathBuf> {
    let candidates = [r"C:\Program Files\7-Zip\7z.exe", "/usr/bin/7z", "/usr/bin/7zz", "/opt/homebrew/bin/7zz"];
    candidates.into_iter().map(PathBuf::from).find(|path| path.is_file())
}

#[test]
fn seven_zip_fallback() {
    let Some(seven_zip) = installed_seven_zip() else {
        eprintln!("seven_zip_fallback: no 7-Zip installed; skipped");
        return;
    };
    let d = dir("seven-zip");
    let src = d.join("src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.txt"), "a").unwrap();
    std::fs::write(src.join("sub/b.txt"), "b").unwrap();
    // A WIM image: a format only 7-Zip reads.
    let status = std::process::Command::new(&seven_zip)
        .args(["a", "-twim", "-bso0", "-bsp0"])
        .arg(d.join("image.wim"))
        .arg(src.join("a.txt"))
        .arg(src.join("sub"))
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::create_dir(d.join("out")).unwrap();
    let engine = engine(&d);
    let archive = d.join("image.wim");
    let (report, events) = extract(&engine, vec![archive], ExtractTo::Smart(d.join("out")), Some(seven_zip), nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&d.join("out/image/a.txt")), "a");
    assert_eq!(read(&d.join("out/image/sub/b.txt")), "b");
    assert!(leftovers(&d.join("out")).is_empty());
    let last = events.iter().rev().find_map(|event| match event {
        Event::Progress { progress, .. } => Some(progress.clone()),
        _ => None,
    });
    if let Some(progress) = last {
        assert!(progress.bytes_done <= progress.bytes_total, "{progress:?}");
    }
    undo(&engine);
    assert!(!d.join("out/image").exists());
    let _ = std::fs::remove_dir_all(&d);
}
