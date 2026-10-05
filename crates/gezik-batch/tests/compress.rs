//! Writing archives: every format read back with Gezik's own readers (and tested with 7-Zip
//! when it is installed), zip on several threads, 7z in parts, passwords, cancel, and adding
//! to an existing archive through the engine with its undo.

use std::cell::{Cell, RefCell};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::archive::write::{self, Input, WriteCx};
use gezik_batch::archive::{self, ExtractCx};
use gezik_batch::tasks::{AddToArchiveTask, CompressOptions, CompressTask, Level, OutFormat, default_name};
use gezik_ops::{Answer, Decision, Engine, Event, JobId, PendingDeletes, Question, Report, Settings};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-compress-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Poorly compressible bytes.
fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

/// Writes `files` (relative path with '/', contents) under `root`.
fn make(root: &Path, files: &[(&str, &[u8])]) {
    for (name, data) in files {
        let path = name.split('/').fold(root.to_path_buf(), |p, part| p.join(part));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }
}

/// The files under `root` as (relative path with '/', contents), Gezik's own left out.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().unwrap().to_string_lossy().starts_with(".gezik-") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let rel: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                out.push((rel.join("/"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// What writing reported.
#[derive(Default)]
struct W {
    done: Cell<u64>,
    failed: RefCell<Vec<PathBuf>>,
    bytes: Cell<u64>,
    cancel_after: Option<u64>,
}

impl WriteCx for W {
    fn add_bytes(&self, n: u64) {
        self.bytes.set(self.bytes.get() + n);
    }
    fn entry_done(&self) {
        self.done.set(self.done.get() + 1);
    }
    fn entry_failed(&self, path: &Path, _: &io::Error) {
        self.failed.borrow_mut().push(path.to_path_buf());
    }
    fn stopped(&self) -> bool {
        self.cancel_after.is_some_and(|limit| self.bytes.get() >= limit)
    }
}

/// Reading back, with a password.
struct R {
    password: Option<String>,
    failed: RefCell<Vec<String>>,
}

impl R {
    fn new(password: Option<&str>) -> R {
        R { password: password.map(str::to_owned), failed: Default::default() }
    }
}

impl ExtractCx for R {
    fn add_bytes(&self, _: u64) {}
    fn entry_done(&self) {}
    fn stopped(&self) -> bool {
        false
    }
    fn password(&self, retry: bool) -> Option<String> {
        if retry { None } else { self.password.clone() }
    }
    fn entry_failed(&self, name: &str, _: &io::Error) {
        self.failed.borrow_mut().push(name.to_owned());
    }
}

/// `archive` unpacked by Gezik's reader into a fresh `d/<name>`, as a tree.
fn unpacked(archive: &Path, d: &Path, name: &str, password: Option<&str>) -> Vec<(String, Vec<u8>)> {
    let stage = d.join(name);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).unwrap();
    let cx = R::new(password);
    archive::open(archive).unwrap().extract(&stage, &cx).unwrap();
    assert!(cx.failed.borrow().is_empty(), "{:?}", cx.failed.borrow());
    tree(&stage)
}

fn options(format: OutFormat, level: Level) -> CompressOptions {
    CompressOptions { format, level, password: None, encrypt_names: false, split: None }
}

/// The children of `dir`, sorted.
fn children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    v.sort();
    v
}

fn inputs(sources: &[PathBuf]) -> Vec<Input> {
    write::inputs(sources, &mut |path, err| panic!("{}: {err}", path.display()))
}

/// The 7-Zip installed on this machine, if any.
fn installed_seven_zip() -> Option<PathBuf> {
    let candidates = [r"C:\Program Files\7-Zip\7z.exe", "/usr/bin/7z", "/usr/bin/7zz", "/opt/homebrew/bin/7zz"];
    candidates.into_iter().map(PathBuf::from).find(|path| path.is_file())
}

/// `7z t` passes on `archive`, when 7-Zip is installed.
fn seven_zip_tests(archive: &Path, password: Option<&str>) {
    let Some(seven_zip) = installed_seven_zip() else {
        eprintln!("no 7-Zip installed; `7z t` skipped");
        return;
    };
    let mut command = std::process::Command::new(seven_zip);
    command.args(["t", "-bso0", "-bsp0"]).arg(format!("-p{}", password.unwrap_or(""))).arg(archive);
    let out = command.stdin(std::process::Stdio::null()).output().unwrap();
    assert!(out.status.success(), "7z t {}: {}", archive.display(), String::from_utf8_lossy(&out.stderr));
}

fn sample(src: &Path) {
    make(
        src,
        &[
            ("Docs/a.txt", b"alpha alpha alpha alpha"),
            ("Docs/sub/b.bin", &noise(300_000, 1)),
            ("Docs/sub/zeros.bin", &[0u8; 200_000]),
            ("Docs/.gezik-copying-1-0", b"a leftover; never archived"),
            ("Docs/Türkçe ğüşiöç.txt", b"unicode"),
            ("top.txt", b"top"),
            ("empty.txt", b""),
        ],
    );
    std::fs::create_dir_all(src.join("Docs/empty dir")).unwrap();
}

#[test]
fn every_format_round_trips() {
    let d = dir("formats");
    let src = d.join("src");
    sample(&src);
    let want = tree(&src);
    let sources = children(&src);
    let cases = [
        (OutFormat::Zip, Level::Normal),
        (OutFormat::Zip, Level::Store),
        (OutFormat::SevenZ, Level::Normal),
        (OutFormat::SevenZ, Level::Store),
        (OutFormat::Tar, Level::Normal),
        (OutFormat::TarGz, Level::Fast),
        (OutFormat::TarXz, Level::Normal),
    ];
    for (format, level) in cases {
        let out = d.join(format!("out-{level:?}.{}", format.extension()));
        let cx = W::default();
        let all = inputs(&sources);
        let written = write::write(&all, &out, &options(format, level), 2, &cx).unwrap();
        assert_eq!(written, std::slice::from_ref(&out));
        assert!(cx.failed.borrow().is_empty(), "{format:?}: {:?}", cx.failed.borrow());
        assert_eq!(cx.done.get(), all.len() as u64, "{format:?}: every input counted");
        assert_eq!(unpacked(&out, &d, "back", None), want, "{format:?} {level:?}");
        assert!(d.join("back/Docs/empty dir").is_dir(), "{format:?}: the empty folder is there");
        if matches!(format, OutFormat::Zip | OutFormat::SevenZ) {
            seven_zip_tests(&out, None);
        }
    }
    for format in [OutFormat::Gz, OutFormat::Xz] {
        let out = d.join(format!("top.txt.{}", format.extension()));
        write::write(&inputs(&[src.join("top.txt")]), &out, &options(format, Level::Best), 1, &W::default()).unwrap();
        assert_eq!(unpacked(&out, &d, "single", None), [("top.txt".to_owned(), b"top".to_vec())], "{format:?}");
    }
    let err =
        write::write(&inputs(&sources), &d.join("x.gz"), &options(OutFormat::Gz, Level::Normal), 1, &W::default());
    assert_eq!(err.unwrap_err().kind(), io::ErrorKind::InvalidInput, "a .gz holds one file");
    assert!(!d.join("x.gz").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn zip_parallel_matches_serial_content() {
    let d = dir("parallel");
    let src = d.join("src");
    let files: Vec<(String, Vec<u8>)> = (0..100u32)
        .map(|i| {
            let data = match i % 4 {
                0 => Vec::new(),
                1 => noise(1000 + i as usize * 997, i),
                2 => vec![i as u8; 50_000],
                _ => format!("file {i}").repeat(i as usize).into_bytes(),
            };
            (format!("f{}/n{i:03}.dat", i % 7), data)
        })
        .collect();
    let list: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    make(&src, &list);
    let want = tree(&src);
    let all = inputs(&children(&src));
    let (serial, parallel) = (d.join("serial.zip"), d.join("parallel.zip"));
    let password = CompressOptions { password: Some("pw".into()), ..options(OutFormat::Zip, Level::Normal) };
    for (out, workers) in [(&serial, 1), (&parallel, 4)] {
        let cx = W::default();
        write::write(&all, out, &password, workers, &cx).unwrap();
        assert_eq!(cx.done.get(), all.len() as u64);
        assert_eq!(cx.bytes.get(), files.iter().map(|(_, d)| d.len() as u64).sum::<u64>(), "progress is every byte");
    }
    assert_eq!(unpacked(&serial, &d, "s", Some("pw")), want);
    assert_eq!(unpacked(&parallel, &d, "p", Some("pw")), want);
    seven_zip_tests(&parallel, Some("pw"));
    // In the inputs' order: the same input gives the same bytes (no password: AES salts differ).
    let plain = options(OutFormat::Zip, Level::Normal);
    let (one, two) = (d.join("one.zip"), d.join("two.zip"));
    write::write(&all, &one, &plain, 4, &W::default()).unwrap();
    write::write(&all, &two, &plain, 3, &W::default()).unwrap();
    assert_eq!(std::fs::read(&one).unwrap(), std::fs::read(&two).unwrap());
    let names: Vec<String> =
        zip::ZipArchive::new(std::fs::File::open(&one).unwrap()).unwrap().file_names().map(str::to_owned).collect();
    let listed: Vec<String> = all
        .iter()
        .map(|i| if i.kind == write::InputKind::Dir { format!("{}/", i.name) } else { i.name.clone() })
        .collect();
    let mut dirs_first: Vec<String> = listed.iter().filter(|n| n.ends_with('/')).cloned().collect();
    dirs_first.extend(listed.iter().filter(|n| !n.ends_with('/')).cloned());
    assert_eq!(names, dirs_first);
    let leftovers: Vec<PathBuf> = children(&d).into_iter().filter(|p| p.to_string_lossy().contains(".zip.")).collect();
    assert!(leftovers.is_empty(), "no worker part is left: {leftovers:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn sevenz_split_parts_open() {
    let d = dir("split");
    let src = d.join("src");
    make(&src, &[("big.bin", &noise(250_000, 7)), ("small.txt", b"small")]);
    let want = tree(&src);
    let out = d.join("parts.7z");
    let split = CompressOptions { split: Some(100_000), ..options(OutFormat::SevenZ, Level::Fast) };
    let written = write::write(&inputs(&children(&src)), &out, &split, 1, &W::default()).unwrap();
    assert_eq!(written, [d.join("parts.7z.001"), d.join("parts.7z.002"), d.join("parts.7z.003")]);
    assert!(!out.exists());
    // Any part opens the set.
    assert_eq!(unpacked(&written[1], &d, "back", None), want);
    seven_zip_tests(&written[0], None);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn aes_zip_and_7z_need_the_password() {
    let d = dir("aes");
    let src = d.join("src");
    make(&src, &[("secret.txt", b"the secret"), ("dir/more.txt", b"more")]);
    let want = tree(&src);
    let all = inputs(&children(&src));
    for (format, names) in [(OutFormat::Zip, false), (OutFormat::SevenZ, false), (OutFormat::SevenZ, true)] {
        let out = d.join(format!("s-{names}.{}", format.extension()));
        let opts = CompressOptions {
            password: Some("Passw0rd".into()),
            encrypt_names: names,
            ..options(format, Level::Normal)
        };
        write::write(&all, &out, &opts, 2, &W::default()).unwrap();
        // Without the password nothing comes out.
        let stage = d.join("none");
        let _ = std::fs::remove_dir_all(&stage);
        std::fs::create_dir_all(&stage).unwrap();
        let cx = R::new(None);
        let _ = archive::open(&out).and_then(|mut a| a.extract(&stage, &cx));
        assert!(tree(&stage).is_empty(), "{format:?} {names}");
        assert_eq!(unpacked(&out, &d, "back", Some("Passw0rd")), want, "{format:?} {names}");
        seven_zip_tests(&out, Some("Passw0rd"));
        match format {
            OutFormat::Zip => {
                let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
                for i in 0..zip.len() {
                    let entry = zip.by_index_raw(i).unwrap();
                    assert!(entry.is_dir() || entry.encrypted(), "{}", entry.name());
                }
            }
            _ => {
                let header = sevenz_rust2::Archive::open(&out);
                assert_eq!(matches!(header, Err(sevenz_rust2::Error::PasswordRequired)), names, "names hidden");
            }
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending-deletes")), ..Settings::default() }, || {})
}

/// Runs `job` to the end; `on` returns true when it handled an event itself (questions are
/// cancelled and conflicts take their defaults otherwise).
fn finish(engine: &Engine, job: JobId, mut on: impl FnMut(&Engine, JobId, &Event) -> bool) -> (Report, Vec<Event>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = Vec::new();
    loop {
        for event in engine.drain() {
            let handled = on(engine, job, &event);
            match &event {
                Event::Conflicts { job: j, conflicts } if *j == job && !handled => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, id, .. } if *j == job && !handled => engine.answer(job, *id, Answer::Cancel),
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

/// Names in `d` that a temporary file or staging folder would have.
fn leftovers(d: &Path) -> Vec<String> {
    std::fs::read_dir(d)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".gezik-"))
        .collect()
}

#[test]
fn compress_task_makes_the_archive_and_undo_trashes_it() {
    let d = dir("task");
    let src = d.join("src");
    sample(&src);
    let target = d.join("out.zip");
    let engine = engine(&d);
    let task = CompressTask::new(children(&src), target.clone(), options(OutFormat::Zip, Level::Normal));
    let (report, _) = finish(&engine, engine.submit(Box::new(task)), nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.results, std::slice::from_ref(&target));
    assert_eq!(unpacked(&target, &d, "back", None), tree(&src));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    assert_eq!(engine.undo_label().as_deref(), Some("Compress 3 items"));
    let job = engine.undo().unwrap();
    finish(&engine, job, nothing);
    assert!(!target.exists());

    // In parts: each part lands under its name.
    let split = CompressOptions { split: Some(200_000), ..options(OutFormat::SevenZ, Level::Store) };
    let task = CompressTask::new(children(&src), d.join("parts.7z"), split);
    let (report, _) = finish(&engine, engine.submit(Box::new(task)), nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(d.join("parts.7z.001").is_file() && d.join("parts.7z.003").is_file());
    assert_eq!(unpacked(&d.join("parts.7z.002"), &d, "parts", None), tree(&src));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_removes_the_temp_file() {
    let d = dir("cancel");
    // Through the writer: a stop in the middle leaves nothing.
    let src = d.join("src");
    let big: Vec<(String, Vec<u8>)> = (0..8).map(|i| (format!("n{i}.bin"), noise(2_000_000, i))).collect();
    let list: Vec<(&str, &[u8])> = big.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    make(&src, &list);
    let all = inputs(&children(&src));
    for (format, workers) in [(OutFormat::Zip, 4), (OutFormat::Zip, 1), (OutFormat::SevenZ, 1), (OutFormat::TarGz, 1)] {
        let out = d.join(format!("cut.{}", format.extension()));
        let cx = W { cancel_after: Some(3_000_000), ..W::default() };
        let err = write::write(&all, &out, &options(format, Level::Normal), workers, &cx).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted, "{format:?}: {err}");
        assert!(!out.exists(), "{format:?}");
        let parts: Vec<PathBuf> = children(&d).into_iter().filter(|p| p.to_string_lossy().contains("cut.")).collect();
        assert!(parts.is_empty(), "{format:?}: {parts:?}");
    }

    // Through the engine: the temporary file goes and nothing is noted any more.
    let engine = engine(&d);
    let task = CompressTask::new(vec![src.clone()], d.join("slow.7z"), options(OutFormat::SevenZ, Level::Best));
    let mut temp_seen = false;
    let (report, _) = finish(&engine, engine.submit(Box::new(task)), |engine, job, event| match event {
        // Cancelled once it is writing: its temporary file is there then.
        Event::Progress { job: j, progress } if *j == job && progress.bytes_done > 0 => {
            temp_seen |= !leftovers(&d).is_empty();
            engine.cancel(job);
            true
        }
        _ => false,
    });
    assert!(report.cancelled);
    assert!(temp_seen, "cancelled in the middle");
    assert!(!d.join("slow.7z").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let pending = PendingDeletes::new(d.join("pending-deletes"));
    assert!(pending.load().is_empty() && pending.copies().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

/// Runs an `AddToArchiveTask`; `on` answers its questions.
fn add(
    engine: &Engine,
    archive: &Path,
    sources: Vec<PathBuf>,
    password: Option<&str>,
    on: impl FnMut(&Engine, JobId, &Event) -> bool,
) -> (Report, Vec<Event>) {
    let task = AddToArchiveTask::new(archive.to_path_buf(), sources, password.map(str::to_owned));
    finish(engine, engine.submit(Box::new(task)), on)
}

#[test]
fn add_to_zip_keeps_aes_entries() {
    let d = dir("add-zip");
    let src = d.join("src");
    make(&src, &[("a.txt", b"first"), ("dir/b.txt", b"second")]);
    let archive = d.join("s.zip");
    let opts = CompressOptions { password: Some("Passw0rd".into()), ..options(OutFormat::Zip, Level::Normal) };
    write::write(&inputs(&children(&src)), &archive, &opts, 1, &W::default()).unwrap();
    let more = d.join("more");
    make(&more, &[("new.txt", b"added"), ("dir/c.txt", b"third")]);
    let engine = engine(&d);
    // No password given: it is asked, a wrong one again.
    let mut asked = Vec::new();
    let (report, _) = add(&engine, &archive, children(&more), None, |engine, job, event| {
        let Event::Question { id, question, .. } = event else { return false };
        asked.push(question.clone());
        engine.answer(job, *id, Answer::Text(if asked.len() == 1 { "wrong" } else { "Passw0rd" }.into()));
        true
    });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        asked,
        [
            Question::Password { archive: archive.clone(), retry: false },
            Question::Password { archive: archive.clone(), retry: true }
        ]
    );
    let mut want = tree(&src);
    want.extend(tree(&more));
    want.sort();
    assert_eq!(unpacked(&archive, &d, "back", Some("Passw0rd")), want);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).unwrap();
        assert!(entry.is_dir() || entry.encrypted(), "{} is encrypted", entry.name());
    }
    // The folder `dir/` is in it once.
    let dirs = zip.file_names().filter(|n| *n == "dir/").count();
    assert_eq!(dirs, 1);
    drop(zip);
    seven_zip_tests(&archive, Some("Passw0rd"));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn add_to_tar_gz_repacks() {
    let d = dir("add-tgz");
    let src = d.join("src");
    make(&src, &[("Docs/a.txt", b"old a"), ("Docs/keep.txt", b"keep")]);
    let archive = d.join("docs.tar.gz");
    write::write(&inputs(&children(&src)), &archive, &options(OutFormat::TarGz, Level::Normal), 1, &W::default())
        .unwrap();
    let more = d.join("more");
    make(&more, &[("Docs/a.txt", b"new a"), ("Docs/b.txt", b"b")]);
    let engine = engine(&d);
    let mut questions = Vec::new();
    let (report, _) = add(&engine, &archive, children(&more), None, |engine, job, event| {
        let Event::Question { id, question, .. } = event else { return false };
        questions.push(question.clone());
        engine.answer(job, *id, Answer::Button(2));
        true
    });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let [Question::Confirm { title, message, buttons }] = questions.as_slice() else { panic!("{questions:?}") };
    assert_eq!(title, "1 item already in the archive");
    assert_eq!(message, "Docs/a.txt");
    assert_eq!(buttons, &["Replace", "Skip", "Keep both", "Cancel"]);
    let back = unpacked(&archive, &d, "back", None);
    let names: Vec<(&str, &[u8])> = back.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    assert_eq!(
        names,
        [
            ("Docs/a (2).txt", b"new a".as_slice()),
            ("Docs/a.txt", b"old a"),
            ("Docs/b.txt", b"b"),
            ("Docs/keep.txt", b"keep")
        ]
    );
    // Still a tar.gz.
    let mut head = [0u8; 2];
    std::fs::File::open(&archive).unwrap().read_exact(&mut head).unwrap();
    assert_eq!(head, [0x1F, 0x8B]);

    // Replace this time.
    make(&more, &[("Docs/a.txt", b"newer a")]);
    let (report, _) = add(&engine, &archive, vec![more.join("Docs")], None, |engine, job, event| {
        let Event::Question { id, .. } = event else { return false };
        engine.answer(job, *id, Answer::Button(0));
        true
    });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let back = unpacked(&archive, &d, "back", None);
    assert!(back.contains(&("Docs/a.txt".to_owned(), b"newer a".to_vec())), "{back:?}");
    assert_eq!(back.len(), 4);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn add_keeps_the_unix_facts_of_entries_packed_again() {
    let d = dir("add-unix");
    // A tar.gz made on Unix: a 0755 script with its owner and time.
    let archive = d.join("tools.tar.gz");
    {
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(10);
        header.set_mode(0o755);
        header.set_mtime(1_700_000_000);
        header.set_uid(1000);
        header.set_gid(100);
        header.set_username("teo").unwrap();
        header.set_groupname("staff").unwrap();
        builder.append_data(&mut header, "bin/run.sh", &b"#!/bin/sh\n"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
    }
    // A 7z whose entry carries a Unix mode (7-Zip's 0x8000 flag), on every system.
    let sevenz = d.join("tools.7z");
    {
        let src = d.join("src");
        make(&src, &[("run.sh", b"#!/bin/sh\n")]);
        let mut items = inputs(&children(&src));
        items[0].mode = 0o755;
        items[0].keep_mode = true;
        write::write(&items, &sevenz, &options(OutFormat::SevenZ, Level::Normal), 1, &W::default()).unwrap();
    }
    let more = d.join("more");
    make(&more, &[("new.txt", b"new")]);
    let engine = engine(&d);
    for target in [&archive, &sevenz] {
        let (report, _) = add(&engine, target, children(&more), None, nothing);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
    }
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(&archive).unwrap());
    let mut tar = tar::Archive::new(gz);
    let mut found = false;
    for entry in tar.entries().unwrap() {
        let entry = entry.unwrap();
        let header = entry.header();
        if entry.path().unwrap() == Path::new("bin/run.sh") {
            found = true;
            assert_eq!(header.mode().unwrap() & 0o7777, 0o755);
            assert_eq!(header.mtime().unwrap(), 1_700_000_000);
            assert_eq!((header.uid().unwrap(), header.gid().unwrap()), (1000, 100));
            assert_eq!(header.username().unwrap(), Some("teo"));
            assert_eq!(header.groupname().unwrap(), Some("staff"));
        }
    }
    assert!(found, "bin/run.sh is kept");
    let packed = sevenz_rust2::Archive::open(&sevenz).unwrap();
    let entry = packed.files.iter().find(|e| e.name() == "run.sh").expect("run.sh is kept");
    let attributes = entry.windows_attributes();
    assert!(attributes & 0x8000 != 0, "{attributes:#x}");
    assert_eq!((attributes >> 16) & 0o7777, 0o755);
    assert!(packed.files.iter().any(|e| e.name() == "new.txt"));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn add_undo_restores_the_old_archive() {
    let d = dir("add-undo");
    let src = d.join("src");
    make(&src, &[("a.txt", b"a")]);
    let more = d.join("more");
    make(&more, &[("b.txt", b"b")]);
    let engine = engine(&d);
    for format in [OutFormat::Zip, OutFormat::SevenZ] {
        let archive = d.join(format!("a.{}", format.extension()));
        write::write(&inputs(&children(&src)), &archive, &options(format, Level::Normal), 1, &W::default()).unwrap();
        let before = std::fs::read(&archive).unwrap();
        let (report, _) = add(&engine, &archive, children(&more), None, nothing);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.results, std::slice::from_ref(&archive));
        let both = vec![("a.txt".to_owned(), b"a".to_vec()), ("b.txt".to_owned(), b"b".to_vec())];
        assert_eq!(unpacked(&archive, &d, "back", None), both, "{format:?}");
        assert_eq!(engine.undo_label().as_deref(), Some("Add to archive 1 item"));
        let job = engine.undo().unwrap();
        let (report, _) = finish(&engine, job, nothing);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(std::fs::read(&archive).unwrap(), before, "{format:?}: the old archive is back");
        assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn default_names() {
    let file = |name: &str| (PathBuf::from("d").join(name), false);
    let folder = |name: &str| (PathBuf::from("d").join(name), true);
    assert_eq!(default_name(&[file("report.pdf")], OutFormat::Zip), "report.zip");
    assert_eq!(default_name(&[folder("Photos.2024")], OutFormat::SevenZ), "Photos.2024.7z");
    assert_eq!(default_name(&[file("notes.txt")], OutFormat::Gz), "notes.txt.gz");
    assert_eq!(default_name(&[file(".bashrc")], OutFormat::Zip), ".bashrc.zip");
    assert_eq!(default_name(&[file("report.pdf"), file("notes.txt")], OutFormat::TarGz), "d.tar.gz");
    assert_eq!(default_name(&[file("x/a"), folder("y/b")], OutFormat::Zip), "d.zip");
    let root = gezik_platform::fs::drive_root(&std::env::temp_dir()).unwrap();
    assert_eq!(default_name(&[(root.join("a"), false), (root.join("b"), true)], OutFormat::Zip), "Archive.zip");
    assert_eq!(default_name(&[(root, true)], OutFormat::Zip), "Archive.zip");
    assert_eq!(default_name(&[], OutFormat::Zip), "Archive.zip");
}

#[test]
fn sevenz_parts_replace_and_keep_both_cover_the_whole_set() {
    let d = dir("parts-conflict");
    let (old, new) = (d.join("old"), d.join("new"));
    make(&old, &[("big.bin", &noise(250_000, 3))]);
    make(&new, &[("small.bin", &noise(150_000, 4))]);
    let engine = engine(&d);
    let parts = || CompressOptions { split: Some(100_000), ..options(OutFormat::SevenZ, Level::Store) };
    let compress = |source: &Path, decision: Decision| {
        let task = CompressTask::new(children(source), d.join("set.7z"), parts());
        finish(&engine, engine.submit(Box::new(task)), |engine, job, event| match event {
            Event::Conflicts { job: j, conflicts } if *j == job => {
                engine.decide(job, vec![decision; conflicts.len()]);
                true
            }
            _ => false,
        })
        .0
    };
    let set = |base: &str| -> Vec<String> {
        let mut names: Vec<String> = children(&d)
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(base))
            .collect();
        names.sort();
        names
    };
    let report = compress(&old, Decision::Replace);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(set("set.7z."), ["set.7z.001", "set.7z.002", "set.7z.003"]);

    // Keep both: the whole new set goes to a free name, the old set stays whole.
    let report = compress(&new, Decision::KeepBoth);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(set("set.7z."), ["set.7z.001", "set.7z.002", "set.7z.003"]);
    assert_eq!(unpacked(&d.join("set.7z.001"), &d, "back", None), tree(&old));
    let kept: Vec<PathBuf> = children(&d).into_iter().filter(|p| p.to_string_lossy().contains("set (2).7z.")).collect();
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_eq!(unpacked(&kept[0], &d, "back", None), tree(&new));

    // Replace: every old part goes, only the new two remain and open.
    let report = compress(&new, Decision::Replace);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(set("set.7z."), ["set.7z.001", "set.7z.002"]);
    assert_eq!(unpacked(&d.join("set.7z.002"), &d, "back", None), tree(&new));
    assert!(PendingDeletes::new(d.join("pending-deletes")).restores().is_empty(), "trashed: no note left");
    // Undo brings the old set back whole.
    let report = finish(&engine, engine.undo().unwrap(), nothing).0;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(set("set.7z."), ["set.7z.001", "set.7z.002", "set.7z.003"]);
    assert_eq!(unpacked(&d.join("set.7z.001"), &d, "back", None), tree(&old));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn unreadable_inputs_are_left_out() {
    let d = dir("unreadable");
    let src = d.join("src");
    make(&src, &[("a.txt", b"a"), ("gone.txt", b"gone"), ("z.txt", b"z")]);
    let all = inputs(&children(&src));
    // Removed after it was listed, before it is read.
    std::fs::remove_file(src.join("gone.txt")).unwrap();
    let want = tree(&src);
    for format in [OutFormat::Zip, OutFormat::SevenZ, OutFormat::TarGz] {
        for workers in [1, 4] {
            let out = d.join(format!("out-{workers}.{}", format.extension()));
            let cx = W::default();
            write::write(&all, &out, &options(format, Level::Normal), workers, &cx).unwrap();
            assert_eq!(*cx.failed.borrow(), [src.join("gone.txt")], "{format:?}");
            assert_eq!(cx.done.get(), 2, "{format:?}");
            assert_eq!(unpacked(&out, &d, "back", None), want, "{format:?}: left out, not stored empty");
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn add_clashes_ignore_case_and_see_files_against_folders() {
    let d = dir("add-clash");
    let src = d.join("src");
    make(&src, &[("Notes.txt", b"old notes"), ("pics/a.jpg", b"jpeg")]);
    let archive = d.join("a.zip");
    write::write(&inputs(&children(&src)), &archive, &options(OutFormat::Zip, Level::Normal), 1, &W::default())
        .unwrap();
    let more = d.join("more");
    // `pics` is a file here: it clashes with the folder in the archive.
    make(&more, &[("pics", b"a file"), ("notes.TXT", b"new notes")]);
    let engine = engine(&d);
    let mut titles = Vec::new();
    let (report, _) = add(&engine, &archive, children(&more), None, |engine, job, event| {
        let Event::Question { id, question: Question::Confirm { title, .. }, .. } = event else { return false };
        titles.push(title.clone());
        engine.answer(job, *id, Answer::Button(2));
        true
    });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let case_blind = cfg!(any(windows, target_os = "macos"));
    assert_eq!(titles, [if case_blind { "2 items already in the archive" } else { "1 item already in the archive" }]);
    let names: Vec<String> = unpacked(&archive, &d, "back", None).into_iter().map(|(n, _)| n).collect();
    let mut want = vec!["Notes.txt", "pics (2)", "pics/a.jpg", if case_blind { "notes (2).TXT" } else { "notes.TXT" }];
    want.sort();
    assert_eq!(names, want);

    // Replace: the folder goes with what it holds.
    let file = d.join("file");
    make(&file, &[("pics", b"now a file")]);
    let (report, _) = add(&engine, &archive, vec![file.join("pics")], None, |engine, job, event| {
        let Event::Question { id, .. } = event else { return false };
        engine.answer(job, *id, Answer::Button(0));
        true
    });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let back = unpacked(&archive, &d, "back", None);
    assert!(back.contains(&("pics".to_owned(), b"now a file".to_vec())), "{back:?}");
    assert!(!back.iter().any(|(n, _)| n.starts_with("pics/")), "{back:?}");

    // Skip everything: nothing to add, the archive is left as it is and there is nothing to undo.
    let before = std::fs::read(&archive).unwrap();
    let label = engine.undo_label();
    let (report, _) = add(&engine, &archive, vec![file.join("pics")], None, |engine, job, event| {
        let Event::Question { id, .. } = event else { return false };
        engine.answer(job, *id, Answer::Button(1));
        true
    });
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(&archive).unwrap(), before);
    assert_eq!(engine.undo_label(), label);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_part_that_cannot_land_brings_the_old_set_back() {
    let d = dir("parts-rollback");
    let (old, new) = (d.join("old"), d.join("new"));
    make(&old, &[("old.bin", &noise(150_000, 5))]);
    make(&new, &[("new.bin", &noise(250_000, 6))]);
    let engine = engine(&d);
    let parts = || CompressOptions { split: Some(100_000), ..options(OutFormat::SevenZ, Level::Store) };
    let task = CompressTask::new(children(&old), d.join("set.7z"), parts());
    let (report, _) = finish(&engine, engine.submit(Box::new(task)), nothing);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let old_bytes: Vec<Vec<u8>> = (1..=2).map(|n| std::fs::read(d.join(format!("set.7z.{n:03}"))).unwrap()).collect();
    // The new set has three parts; the third cannot land (a folder has its name).
    std::fs::create_dir(d.join("set.7z.003")).unwrap();
    let task = CompressTask::new(children(&new), d.join("set.7z"), parts());
    let (report, _) = finish(&engine, engine.submit(Box::new(task)), |engine, job, event| match event {
        Event::Conflicts { job: j, conflicts } if *j == job => {
            engine.decide(job, vec![Decision::Replace; conflicts.len()]);
            true
        }
        _ => false,
    });
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(report.failures[0].message.contains("set.7z.003"), "{:?}", report.failures);
    // No new part stays; the old part set aside is back. The first old part was trashed by
    // the engine for "Replace" before the run: undo brings it back.
    assert!(!d.join("set.7z.001").exists());
    assert_eq!(std::fs::read(d.join("set.7z.002")).unwrap(), old_bytes[1]);
    assert!(d.join("set.7z.003").is_dir());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    assert!(PendingDeletes::new(d.join("pending-deletes")).restores().is_empty(), "put back: no note left");
    let report = finish(&engine, engine.undo().unwrap(), nothing).0;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(d.join("set.7z.001")).unwrap(), old_bytes[0]);
    std::fs::remove_dir(d.join("set.7z.003")).unwrap();
    assert_eq!(unpacked(&d.join("set.7z.001"), &d, "back", None), tree(&old));
    let _ = std::fs::remove_dir_all(&d);
}
