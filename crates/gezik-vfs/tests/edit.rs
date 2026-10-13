//! Changing places through the engine (spec 16 §6): an archive two archives deep, moving out
//! (only what was placed is deleted), moving between archives, putting a changed file back,
//! and archives Gezik only reads.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_core::vfs::{Edit, VfsPath};
use gezik_ops::{Answer, Engine, Event, Question, Report, Settings, Task};
use gezik_vfs::tasks::{VfsEditTask, move_out};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-vfs-edit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    gezik_platform::trash::purge_at_thread_end(&d);
    d
}

fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn zip(path: &Path, files: &[(&str, &[u8])]) {
    std::fs::write(path, zip_bytes(files)).unwrap();
}

/// The entries of the zip in `bytes`: (name, contents), sorted.
fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        out.push((entry.name().to_owned(), data));
    }
    out.sort();
    out
}

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    entries(bytes).into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}")).1
}

fn names(bytes: &[u8]) -> Vec<String> {
    entries(bytes).into_iter().map(|(name, _)| name).collect()
}

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending")), ..Settings::default() }, || {})
}

/// Runs `tasks` as one job to the end: conflicts take their defaults (Skip), questions are
/// cancelled.
fn run(engine: &Engine, tasks: Vec<Box<dyn Task>>) -> (Report, Vec<Question>) {
    let job = engine.submit_chain(tasks, None);
    wait(engine, job)
}

fn wait(engine: &Engine, job: gezik_ops::JobId) -> (Report, Vec<Question>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut asked = Vec::new();
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, id, question } if j == job => {
                    asked.push(question);
                    engine.answer(job, id, Answer::Cancel);
                }
                Event::Finished { job: j, report } if j == job => return (report, asked),
                _ => {}
            }
        }
        assert!(Instant::now() < deadline, "the job did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Names in `d` a temporary file or staging folder would have.
fn leftovers(d: &Path) -> Vec<String> {
    std::fs::read_dir(d)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".gezik-"))
        .collect()
}

#[test]
fn an_archive_two_archives_deep_is_written_back_through_both() {
    let d = dir("nested");
    let deep = zip_bytes(&[("x.txt", b"x")]);
    let mid = zip_bytes(&[("deep/in.zip", &deep), ("m.txt", b"m")]);
    let outer = d.join("outer.zip");
    zip(&outer, &[("a/mid.zip", &mid), ("o.txt", b"o")]);
    let before = std::fs::read(&outer).unwrap();
    let new = d.join("new.txt");
    std::fs::write(&new, b"new").unwrap();
    let mid_at = VfsPath::archive(outer.clone()).join("a/mid.zip");
    let in_at = VfsPath::inside(mid_at).join("deep/in.zip");
    let inner = VfsPath::inside(in_at);
    let engine = engine(&d);
    let (report, _) = run(&engine, vec![Box::new(VfsEditTask::put(&inner, vec![new.clone()], false))]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let now = std::fs::read(&outer).unwrap();
    assert_eq!(names(&now), ["a/mid.zip", "o.txt"]);
    let mid = entry(&now, "a/mid.zip");
    assert_eq!(names(&mid), ["deep/in.zip", "m.txt"]);
    assert_eq!(
        entries(&entry(&mid, "deep/in.zip")),
        [("new.txt".into(), b"new".to_vec()), ("x.txt".into(), b"x".to_vec())]
    );
    assert!(new.exists(), "a copy leaves the file");
    // The place, opened again, shows it (the outer archive changed).
    let cx = Quiet;
    let vfs = gezik_vfs::mount(&inner.root, &cx).unwrap();
    assert!(vfs.is_file("new.txt"));
    drop(vfs);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    // One undo: the outer archive as it was.
    let (report, _) = wait(&engine, engine.undo().unwrap());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(&outer).unwrap(), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn moving_out_deletes_only_what_was_placed() {
    let d = dir("move-out");
    let archive = d.join("a.zip");
    zip(&archive, &[("docs/x.txt", b"x"), ("docs/y.txt", b"y"), ("keep.txt", b"k")]);
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    // y.txt is there already: its conflict is skipped, so it stays in the archive.
    std::fs::write(out.join("y.txt"), b"older").unwrap();
    let at = VfsPath::archive(archive.clone()).join("docs");
    let (tasks, label) = move_out(&[at.join("x.txt"), at.join("y.txt")], out.clone());
    assert_eq!(label, "Move 2 items");
    let (report, _) = run(&engine(&d), tasks);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(out.join("x.txt")).unwrap(), b"x");
    assert_eq!(std::fs::read(out.join("y.txt")).unwrap(), b"older");
    assert_eq!(names(&std::fs::read(&archive).unwrap()), ["docs/y.txt", "keep.txt"]);
    assert!(leftovers(&d).is_empty() && leftovers(&out).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn entries_move_between_archives_and_files_move_in() {
    let d = dir("between");
    let a = d.join("a.zip");
    let b = d.join("b.zip");
    zip(&a, &[("x.txt", b"x"), ("stay.txt", b"s")]);
    zip(&b, &[("d/old.txt", b"o")]);
    let engine = engine(&d);
    let into = VfsPath::archive(b.clone()).join("d");
    let task = VfsEditTask::copy_in(&[VfsPath::archive(a.clone()).join("x.txt")], &into, true);
    let (report, _) = run(&engine, vec![Box::new(task)]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(names(&std::fs::read(&b).unwrap()), ["d/old.txt", "d/x.txt"]);
    assert_eq!(names(&std::fs::read(&a).unwrap()), ["stay.txt"]);
    // A file on disk moved in goes to the trash once it is in.
    let file = d.join("f.txt");
    std::fs::write(&file, b"f").unwrap();
    let (report, _) = run(&engine, vec![Box::new(VfsEditTask::put(&into, vec![file.clone()], true))]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!file.exists());
    assert_eq!(entry(&std::fs::read(&b).unwrap(), "d/f.txt"), b"f");
    // Undo brings the file back and the archive as it was.
    let (report, _) = wait(&engine, engine.undo().unwrap());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(file.exists());
    assert_eq!(names(&std::fs::read(&b).unwrap()), ["d/old.txt", "d/x.txt"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_changed_file_is_put_back_without_a_question() {
    let d = dir("update");
    let archive = d.join("a.zip");
    zip(&archive, &[("docs/report.txt", b"draft"), ("other.txt", b"o")]);
    let changed = d.join("report.txt");
    std::fs::write(&changed, b"final").unwrap();
    let at = VfsPath::archive(archive.clone()).join("docs/report.txt");
    let (report, asked) = run(&engine(&d), vec![Box::new(VfsEditTask::update(&at, changed))]);
    assert!(report.failures.is_empty() && asked.is_empty(), "{:?} {asked:?}", report.failures);
    let now = std::fs::read(&archive).unwrap();
    assert_eq!(names(&now), ["docs/report.txt", "other.txt"]);
    assert_eq!(entry(&now, "docs/report.txt"), b"final");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_archive_gezik_only_reads_is_not_changed() {
    let d = dir("read-only");
    let single = d.join("x.txt.gz");
    {
        let file = std::fs::File::create(&single).unwrap();
        let mut gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        gz.write_all(b"x").unwrap();
        gz.finish().unwrap();
    }
    let root = VfsPath::archive(single.clone());
    let vfs = gezik_vfs::mount(&root.root, &Quiet).unwrap();
    assert!(!vfs.caps().has(gezik_core::vfs::Caps::WRITE));
    drop(vfs);
    let before = std::fs::read(&single).unwrap();
    let task = VfsEditTask::new(root.root.clone(), vec![Edit::MakeDir("new".into())]);
    let (report, _) = run(&engine(&d), vec![Box::new(task)]);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].message.contains("can only be read"), "{}", report.failures[0].message);
    assert_eq!(std::fs::read(&single).unwrap(), before);
    let _ = std::fs::remove_dir_all(&d);
}

struct Quiet;

impl gezik_vfs::Cx for Quiet {
    fn add_bytes(&self, _: u64) {}
    fn entry_done(&self) {}
    fn stopped(&self) -> bool {
        false
    }
    fn password(&self, _: bool) -> Option<String> {
        None
    }
    fn entry_failed(&self, name: &str, error: &std::io::Error) {
        panic!("{name}: {error}");
    }
}
