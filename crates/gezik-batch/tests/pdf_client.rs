//! Running the PDF worker as a separate process: replies as they come, passwords on its input,
//! crashes, damage, stopping, and the bounds on what goes in and comes out; against
//! `examples/fake_pdf_worker.rs`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gezik_batch::pdf::client::{Ended, PdfFailed, WORKER_ARG, Worker, count_pages, run};
use gezik_core::batch::pdf::{PageImage, Split};
use gezik_core::batch::pdf_worker::{Failure, Reply, Request, WorkerJob};

/// The folder all tests share, with the fake worker copied in.
///
/// It is made, and the fake copied, the first time any test asks, so before any test starts a
/// program: on Linux a copy made on one thread while another thread starts a process can have
/// its open handle inherited by that process for a moment, and starting the copy then fails
/// with "Text file busy" (ETXTBSY).
fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("gezik-pdf-client-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(fake(), root.join(format!("fake_pdf_worker{}", std::env::consts::EXE_SUFFIX))).unwrap();
        root
    })
}

/// The fake worker that `cargo test` builds with the examples (as a program, not a test:
/// `target/<profile>/examples/fake_pdf_worker[.exe]`).
fn fake() -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let name = format!("fake_pdf_worker{}", std::env::consts::EXE_SUFFIX);
    let path = deps.parent().unwrap().join("examples").join(name);
    assert!(
        path.is_file(),
        "{} is missing: run `cargo test -p gezik-batch` (it builds the examples; `--test pdf_client` alone does not)",
        path.display()
    );
    path
}

fn worker() -> Worker {
    let program = root().join(format!("fake_pdf_worker{}", std::env::consts::EXE_SUFFIX));
    Worker { program, args: vec![WORKER_ARG.into()] }
}

fn dir(name: &str) -> PathBuf {
    let d = root().join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// An input file holding the fake worker's script.
fn script(d: &Path, name: &str, text: &str) -> PathBuf {
    let path = d.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn request(out: &Path, job: WorkerJob, inputs: &[&Path]) -> Request {
    Request {
        library: PathBuf::from("lib"),
        dir: out.to_path_buf(),
        job,
        inputs: inputs.iter().map(|p| p.to_path_buf()).collect(),
        passwords: Vec::new(),
    }
}

fn names(d: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn never() -> bool {
    false
}

fn wait(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_split_reports_steps_and_writes_its_parts() {
    let d = dir("split");
    let input = script(&d, "a.pdf", "pages 4\n");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut replies = Vec::new();
    let ended = run(
        &worker(),
        &request(&out, WorkerJob::Split(Split::Every(3)), &[&input]),
        &mut |r| replies.push(r.clone()),
        &never,
    )
    .unwrap();
    assert_eq!(ended, Ended::Done);
    assert_eq!(replies.iter().filter(|r| **r == Reply::Step).count(), 4);
    assert!(replies.contains(&Reply::Steps(4)));
    assert_eq!(names(&out), ["a - page 4.pdf", "a - pages 1-3.pdf"]);
}

#[test]
fn passwords_go_through_the_input_not_the_command_line() {
    let d = dir("password");
    let input = script(&d, "s.pdf", "password gizli\n");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut req = request(&out, WorkerJob::Count, &[&input]);
    assert_eq!(run(&worker(), &req, &mut |_| {}, &never).unwrap(), Ended::NeedsPassword(0));
    req.passwords = vec![(0, "yanlış".into())];
    assert_eq!(run(&worker(), &req, &mut |_| {}, &never).unwrap(), Ended::WrongPassword(0));
    req.passwords = vec![(0, "gizli".into())];
    assert_eq!(run(&worker(), &req, &mut |_| {}, &never).unwrap(), Ended::Done);
    let args = std::fs::read_to_string(d.join("s.pdf.args")).unwrap();
    assert!(!args.contains("gizli") && !args.contains("yanlış"), "{args}");
    assert_eq!(count_pages(&worker(), Path::new("lib"), &input, &never).unwrap(), None);
}

#[test]
fn a_crash_is_an_engine_stop_and_damage_is_named() {
    let d = dir("crash");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let crash = script(&d, "c.pdf", "crash\n");
    let err =
        run(&worker(), &request(&out, WorkerJob::Split(Split::EachPage), &[&crash]), &mut |_| {}, &never).unwrap_err();
    assert!(err.to_string().starts_with("PDF engine stopped (exit code 101)"), "{err}");
    let bad = script(&d, "b.pdf", "damaged\n");
    let err = run(&worker(), &request(&out, WorkerJob::Count, &[&bad]), &mut |_| {}, &never).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().starts_with("the PDF is damaged"), "{err}");
}

#[test]
fn stop_ends_a_hanging_worker_at_once() {
    let d = dir("hang");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let input = script(&d, "h.pdf", "hang\n");
    let started = Instant::now();
    let err = run(&worker(), &request(&out, WorkerJob::Split(Split::EachPage), &[&input]), &mut |_| {}, &|| {
        started.elapsed() > Duration::from_millis(300)
    })
    .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(started.elapsed() < Duration::from_secs(2));
    let pid: u32 = std::fs::read_to_string(d.join("h.pdf.log")).unwrap().trim().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
}

#[test]
fn this_exe_runs_itself_in_worker_mode() {
    let w = Worker::this_exe().unwrap();
    assert_eq!(w.args, [std::ffi::OsString::from(WORKER_ARG)]);
    assert!(w.program.is_absolute());
}

// ---- Beyond the plan's five: every job of the fake, and the bounds -----------------------

#[test]
fn merge_extract_render_and_count_go_through() {
    let d = dir("jobs");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let a = script(&d, "a.pdf", "pages 2\n");
    let b = script(&d, "b.pdf", "pages 3\nlowered 2 41\n");
    let mut replies = Vec::new();
    let ended =
        run(&worker(), &request(&out, WorkerJob::Merge, &[&a, &b]), &mut |r| replies.push(r.clone()), &never).unwrap();
    assert_eq!(ended, Ended::Done);
    assert!(replies.contains(&Reply::Steps(5)), "{replies:?}");
    assert_eq!(std::fs::read_to_string(out.join("a (merged).pdf")).unwrap(), "fake merge 5");

    let out = d.join("extract");
    std::fs::create_dir(&out).unwrap();
    run(&worker(), &request(&out, WorkerJob::Extract("3, 1".into()), &[&b]), &mut |_| {}, &never).unwrap();
    assert_eq!(names(&out), ["b - pages 3, 1.pdf"]);

    let out = d.join("render");
    std::fs::create_dir(&out).unwrap();
    let mut replies = Vec::new();
    let job = WorkerJob::Render { dpi: 300, image: PageImage::Png };
    run(&worker(), &request(&out, job, &[&b]), &mut |r| replies.push(r.clone()), &never).unwrap();
    assert!(replies.contains(&Reply::Lowered { page: 2, dpi: 41 }), "{replies:?}");
    assert_eq!(names(&out), ["b - page 1.png", "b - page 2.png", "b - page 3.png"]);

    assert_eq!(count_pages(&worker(), Path::new("lib"), &b, &never).unwrap(), Some(3));
}

#[test]
fn a_bad_page_range_is_invalid_input() {
    let d = dir("ranges");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let a = script(&d, "a.pdf", "pages 2\n");
    let err = run(&worker(), &request(&out, WorkerJob::Extract("9".into()), &[&a]), &mut |_| {}, &never).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    let failed = err.get_ref().and_then(|e| e.downcast_ref::<PdfFailed>()).expect("a PdfFailed");
    assert_eq!((failed.input, failed.why), (Some(0), Failure::Ranges));
}

#[test]
fn a_reply_line_too_long_ends_the_worker_as_a_protocol_failure() {
    let d = dir("long");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let input = script(&d, "l.pdf", "long\nhang\n");
    let started = Instant::now();
    let err =
        run(&worker(), &request(&out, WorkerJob::Split(Split::EachPage), &[&input]), &mut |_| {}, &never).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("too long"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
    let pid: u32 = std::fs::read_to_string(d.join("l.pdf.log")).unwrap().trim().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
}

#[test]
fn a_request_too_large_is_refused_before_the_worker_starts() {
    let d = dir("large");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let input = script(&d, "a.pdf", "pages 1\n");
    let mut req = request(&out, WorkerJob::Count, &[&input]);
    req.passwords = vec![(0, "x".repeat(5 * 1024 * 1024))];
    let err = run(&worker(), &req, &mut |_| {}, &never).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(!err.to_string().contains("xxxx"), "{err}");
    assert!(!d.join("a.pdf.log").exists(), "the worker started");
}

#[test]
fn worker_messages_reach_the_caller_without_control_characters() {
    let d = dir("clean");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let bad = script(&d, "b.pdf", "damaged\n");
    let mut replies = Vec::new();
    let err = run(&worker(), &request(&out, WorkerJob::Count, &[&bad]), &mut |r| replies.push(r.clone()), &never)
        .unwrap_err();
    let shown = err.to_string();
    assert!(!shown.chars().any(char::is_control), "{shown:?}");
    assert!(shown.contains("bad xref"), "{shown:?}");
    let Some(Reply::Failed { message, .. }) = replies.iter().find(|r| matches!(r, Reply::Failed { .. })) else {
        panic!("no failed reply: {replies:?}")
    };
    assert!(!message.chars().any(char::is_control), "{message:?}");
    // The error output of a crash keeps its lines but loses every other control character.
    let crash = script(&d, "c.pdf", "crash\n");
    let err = run(&worker(), &request(&out, WorkerJob::Count, &[&crash]), &mut |_| {}, &never).unwrap_err();
    let shown = err.to_string();
    assert!(!shown.chars().any(|c| c.is_control() && c != '\n'), "{shown:?}");
    assert!(shown.contains("fake worker crashed"), "{shown:?}");
}
