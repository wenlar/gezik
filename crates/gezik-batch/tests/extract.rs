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
    // Every test starts here: a crash or an abort in this binary (the Shell's code runs in it
    // when undo trashes) must end it, never wait on a dialog.
    static QUIET: std::sync::Once = std::sync::Once::new();
    QUIET.call_once(gezik_platform::process::quiet_crashes);
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
    let _turn = one_job_at_a_time();
    let job = engine.submit_chain(extract_chain(archives, to, seven_zip), Some(label));
    finish(engine, job, on)
}

/// Held from submitting a job until it ended: the tests' jobs run one at a time. Their undos
/// and replaces trash, and the Shell runs third-party copy hooks in this process for each
/// trash; WinSCP's DragExt64 aborted the binary (an "Abnormal program termination" box) when
/// several trashed at once. Gezik itself trashes in parallel.
fn one_job_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Runs `job` to the end; `on` returns true when it handled an event itself. A pause (a
/// full disk, many failures) cancels the job; one that does not end in time fails the test
/// with the last events it sent.
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
                Event::Question { job: j, id, .. } if *j == job && !handled => engine.answer(job, *id, Answer::Cancel),
                Event::Paused { job: j, .. } if *j == job && !handled => engine.cancel(job),
                Event::Finished { job: j, report } if *j == job => {
                    let report = report.clone();
                    seen.push(event);
                    return (report, seen);
                }
                _ => {}
            }
            seen.push(event);
        }
        if Instant::now() > deadline {
            let last: Vec<&Event> = seen.iter().rev().take(20).collect();
            panic!("job {job} did not finish in 60 s; its last events, newest first: {last:#?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn nothing(_: &Engine, _: JobId, _: &Event) -> bool {
    false
}

fn undo(engine: &Engine) -> Report {
    let _turn = one_job_at_a_time();
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
            let Event::Question { id, question, .. } = event else { return false };
            questions.push(question.clone());
            let password = if questions.len() == 1 { "wrong" } else { "Passw0rd" };
            engine.answer(job, *id, Answer::Text(password.into()));
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

/// Makes `archive` from `files` (name, contents) with the installed 7-Zip and `switches`.
fn seven_zip_archive(seven_zip: &Path, archive: &Path, files: &[(&str, &str)], switches: &[&str]) {
    let src = archive.with_extension("src");
    std::fs::create_dir_all(&src).unwrap();
    for (name, data) in files {
        std::fs::write(src.join(name), data).unwrap();
    }
    let status = std::process::Command::new(seven_zip)
        .arg("a")
        .args(switches)
        .args(["-bso0", "-bsp0"])
        .arg(archive)
        .args(files.iter().map(|(name, _)| src.join(name)))
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_dir_all(&src).unwrap();
}

/// Listing without questions (no archive here is header-encrypted).
struct Lister;

impl gezik_batch::archive::ExtractCx for Lister {
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

#[test]
fn methods_gezik_cannot_decode_go_to_seven_zip() {
    let Some(seven_zip) = installed_seven_zip() else {
        eprintln!("methods_gezik_cannot_decode_go_to_seven_zip: no 7-Zip installed; skipped");
        return;
    };
    let d = dir("methods");
    // Long enough that 7-Zip does not just store them.
    let (deflated, lzma_text) = ("deflated in a 7z ".repeat(100), "lzma in a zip ".repeat(100));
    let deflate = d.join("deflate.7z");
    seven_zip_archive(&seven_zip, &deflate, &[("d.txt", &deflated)], &["-t7z", "-m0=Deflate"]);
    let lzma = d.join("lzma.zip");
    seven_zip_archive(&seven_zip, &lzma, &[("l.txt", &lzma_text)], &["-tzip", "-mm=LZMA"]);
    for archive in [&deflate, &lzma] {
        // The reader alone says so when it opens or lists, before writing anything.
        let listed = gezik_batch::archive::open(archive).and_then(|mut source| source.list(&Lister));
        let err = listed.err().unwrap_or_else(|| panic!("{} was listed", archive.display()));
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported, "{err}");
        // Without 7-Zip the job says it is needed.
        let engine = engine(&d);
        let (report, _) = extract(&engine, vec![archive.clone()], ExtractTo::Into(d.join("none")), None, nothing);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(report.failures[0].message.contains("7-Zip needed to open this kind of archive"), "{report:?}");
    }
    let engine = engine(&d);
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let (report, _) =
        extract(&engine, vec![deflate, lzma], ExtractTo::Into(out.clone()), Some(seven_zip.clone()), nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&out.join("d.txt")), deflated);
    assert_eq!(read(&out.join("l.txt")), lzma_text);
    assert!(leftovers(&out).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn seven_zip_gets_the_password_on_its_input() {
    let Some(seven_zip) = installed_seven_zip() else {
        eprintln!("seven_zip_gets_the_password_on_its_input: no 7-Zip installed; skipped");
        return;
    };
    let d = dir("seven-zip-password");
    // AES with LZMA: only 7-Zip reads it.
    let archive = d.join("secret.zip");
    let secret = "secret data ".repeat(100);
    seven_zip_archive(&seven_zip, &archive, &[("s.txt", &secret)], &["-tzip", "-mm=LZMA", "-mem=AES256", "-ppw"]);
    let engine = engine(&d);
    let mut questions = Vec::new();
    let (report, _) = extract(
        &engine,
        vec![archive.clone()],
        ExtractTo::Smart(d.clone()),
        Some(seven_zip.clone()),
        |engine, job, event| {
            let Event::Question { id, question, .. } = event else { return false };
            questions.push(question.clone());
            let password = if questions.len() == 1 { "wrong" } else { "pw" };
            engine.answer(job, *id, Answer::Text(password.into()));
            true
        },
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        questions,
        [
            Question::Password { archive: archive.clone(), retry: false },
            Question::Password { archive: archive.clone(), retry: true },
        ]
    );
    assert_eq!(read(&d.join("s.txt")), secret);

    // Skip: a note, not a failure, and nothing lands.
    std::fs::remove_file(d.join("s.txt")).unwrap();
    let (report, _) =
        extract(&engine, vec![archive.clone()], ExtractTo::Smart(d.clone()), Some(seven_zip), |engine, job, event| {
            let Event::Question { id, .. } = event else { return false };
            engine.answer(job, *id, Answer::Button(1));
            true
        });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{report:?}");
    assert_eq!(report.skipped[0].path, archive);
    assert!(!d.join("s.txt").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}
