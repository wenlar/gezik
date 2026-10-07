//! PDF work as engine jobs: the worker (`examples/fake_pdf_worker.rs`) writes into a staging
//! folder, what it made is placed next to the first input through the conflict list, passwords
//! are asked (and go only to the worker's input), crashes, cancel and pause end the worker, and
//! one undo trashes what was made. Pictures to PDF runs in the job itself.
//!
//! `split_and_render_with_real_pdfium` needs `GEZIK_TEST_PDFIUM` to name a pdfium library;
//! without it it returns at once.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gezik_batch::pdf::client::{WORKER_ARG, Worker, count_pages};
use gezik_batch::tasks::{ImagesToPdfTask, PdfTools, PdfWork, images_pdf_label, pdf_chain, pdf_label};
use gezik_core::batch::pdf::{PageImage, PageOptions, Split};
use gezik_ops::{Answer, Decision, Engine, Event, JobId, Question, Report, Settings, Task};

/// The folder all tests share, with the fake worker (and the real one) copied in first: on
/// Linux a copy made on one thread while another thread starts a process can fail to start
/// with "Text file busy" (see `tests/pdf_client.rs`).
fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("gezik-pdf-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for name in ["fake_pdf_worker", "pdf_worker"] {
            let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
            std::fs::copy(example(&name), root.join(&name)).unwrap();
        }
        root
    })
}

/// A program `cargo test` builds with the examples (`target/<profile>/examples/<name>`).
fn example(name: &str) -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let path = deps.parent().unwrap().join("examples").join(name);
    assert!(
        path.is_file(),
        "{} is missing: run `cargo test -p gezik-batch` (it builds the examples; `--test pdf_tasks` alone does not)",
        path.display()
    );
    path
}

fn fake() -> PathBuf {
    root().join(format!("fake_pdf_worker{}", std::env::consts::EXE_SUFFIX))
}

fn fake_tools() -> PdfTools {
    PdfTools { worker: Worker { program: fake(), args: vec![] }, library: "lib".into() }
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

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending-deletes")), ..Settings::default() }, || {})
}

/// Runs `job` to the end; `decision` settles any conflicts (default: their defaults), and
/// questions are answered from `answers` in turn (then `Cancel`). Returns every event seen and
/// the questions asked.
fn finish_all(
    engine: &Engine,
    job: JobId,
    decision: Option<Decision>,
    answers: Vec<Answer>,
) -> (Report, Vec<Event>, Vec<Question>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut answers = answers.into_iter();
    let mut seen = Vec::new();
    let mut asked = Vec::new();
    loop {
        for event in engine.drain() {
            match &event {
                Event::Conflicts { job: j, conflicts } if *j == job => {
                    let decisions = conflicts.iter().map(|c| decision.unwrap_or(c.decision)).collect();
                    engine.decide(job, decisions);
                }
                Event::Question { job: j, id, question } if *j == job => {
                    asked.push(question.clone());
                    engine.answer(job, *id, answers.next().unwrap_or(Answer::Cancel));
                }
                Event::Finished { job: j, report } if *j == job => {
                    let report = report.clone();
                    seen.push(event);
                    return (report, seen, asked);
                }
                _ => {}
            }
            seen.push(event);
        }
        assert!(Instant::now() < deadline, "job {job} did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn finish_with(engine: &Engine, job: JobId, decision: Option<Decision>) -> (Report, Vec<Event>) {
    let (report, events, _) = finish_all(engine, job, decision, Vec::new());
    (report, events)
}

fn finish_answering(engine: &Engine, job: JobId, answers: Vec<Answer>) -> (Report, Vec<Question>) {
    let (report, _, asked) = finish_all(engine, job, None, answers);
    (report, asked)
}

fn run(engine: &Engine, task: impl Task + 'static) -> Report {
    finish_with(engine, engine.submit(Box::new(task)), None).0
}

fn undo(engine: &Engine) -> Report {
    let report = finish_with(engine, engine.undo().expect("something to undo"), None).0;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    report
}

fn wait(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "waited too long for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
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

/// The names in `d` (Gezik's own and the engine's notes left out), sorted.
fn names(d: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(d)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with(".gezik-") && name != "pending-deletes" && name != "copying")
        .collect();
    names.sort();
    names
}

fn jpeg(path: &Path, w: u16, h: u16, color: [u8; 3]) -> PathBuf {
    let rgb: Vec<u8> = (0..u32::from(w) * u32::from(h)).flat_map(|_| color).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90).encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
    std::fs::write(path, out).unwrap();
    path.to_path_buf()
}

#[test]
fn split_each_page_places_the_parts_and_one_undo_trashes_them() {
    let d = dir("split");
    let a = script(&d, "a.pdf", "pages 3\n");
    let engine = engine(&d);
    let job = engine.submit_chain(
        pdf_chain(PdfWork::Split(Split::EachPage), vec![a.clone()], fake_tools()),
        Some(pdf_label(&PdfWork::Split(Split::EachPage), &[a])),
    );
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // (`.args` and `.log` are the fake worker's own notes.)
    assert_eq!(names(&d), ["a - page 1.pdf", "a - page 2.pdf", "a - page 3.pdf", "a.pdf", "a.pdf.args", "a.pdf.log"]);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    assert_eq!(engine.undo_label().as_deref(), Some("Split a.pdf"));
    undo(&engine);
    assert_eq!(names(&d), ["a.pdf", "a.pdf.args", "a.pdf.log"]);
}

#[test]
fn a_wrong_password_is_asked_again_and_never_put_on_the_command_line() {
    let d = dir("password");
    let s = script(&d, "s.pdf", "password gizli\npages 2\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Extract("2".into()), vec![s.clone()], fake_tools()), None);
    let (report, asked) =
        finish_answering(&engine, job, vec![Answer::Text("yanlış".into()), Answer::Text("gizli".into())]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        asked,
        [
            Question::Password { archive: s.clone(), retry: false },
            Question::Password { archive: s.clone(), retry: true }
        ]
    );
    assert!(d.join("s - page 2.pdf").is_file());
    let args = std::fs::read_to_string(d.join("s.pdf.args")).unwrap();
    assert!(!args.contains("gizli") && !args.contains("yanlış"), "{args}");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn no_password_skips_the_pdf_and_a_merge_entirely() {
    let d = dir("no-password");
    let locked = script(&d, "locked.pdf", "password gizli\npages 2\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Extract("1".into()), vec![locked.clone()], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].path, locked);
    assert_eq!(report.skipped[0].message, "no password given");
    assert!(!d.join("locked - page 1.pdf").exists());

    let plain = script(&d, "plain.pdf", "pages 1\n");
    let job = engine.submit_chain(pdf_chain(PdfWork::Merge, vec![plain, locked.clone()], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].path, locked);
    assert_eq!(report.skipped[0].message, "no password given; nothing was merged");
    assert!(!d.join("plain (merged).pdf").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn a_merge_places_one_pdf_named_after_the_first() {
    let d = dir("merge");
    let a = script(&d, "a.pdf", "pages 2\n");
    let b = script(&d, "b.pdf", "pages 3\n");
    let engine = engine(&d);
    let inputs = vec![a, b];
    let job = engine.submit_chain(
        pdf_chain(PdfWork::Merge, inputs.clone(), fake_tools()),
        Some(pdf_label(&PdfWork::Merge, &inputs)),
    );
    let (report, events) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read_to_string(d.join("a (merged).pdf")).unwrap(), "fake merge 5");
    // The pages counted, and never more done than found.
    let last = events.iter().rev().find_map(|e| match e {
        Event::Progress { job: j, progress } if *j == job => Some(progress.clone()),
        _ => None,
    });
    if let Some(progress) = last {
        assert!(progress.items_done <= progress.items_total, "{progress:?}");
    }
    undo(&engine);
    assert!(!d.join("a (merged).pdf").exists());
}

#[test]
fn a_crashing_worker_fails_its_pdf_and_the_others_still_land() {
    let d = dir("crash");
    let ok = script(&d, "ok.pdf", "pages 2\n");
    let bad = script(&d, "bad.pdf", "crash\n");
    let engine = engine(&d);
    let job =
        engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![bad.clone(), ok], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, bad);
    assert!(report.failures[0].message.starts_with("PDF engine stopped"), "{}", report.failures[0].message);
    assert!(d.join("ok - page 2.pdf").is_file() && !d.join("bad - page 1.pdf").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn outputs_of_a_pdf_that_fails_never_land_and_the_worker_writes_beside_what_is_placed() {
    let d = dir("late-failure");
    let a = script(
        &d,
        "a.pdf",
        "pages 2
stall
late-failure
",
    );
    let hold = d.join("a.pdf.hold");
    std::fs::write(&hold, "").unwrap();
    let ok = script(
        &d, "ok.pdf", "pages 1
",
    );
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![a.clone(), ok], fake_tools()), None);
    // While the worker waits with its outputs written: they are in a folder of their own in
    // the staging folder, and the folder that gets placed holds none of them.
    let mut staging = None;
    wait("the outputs to be written", || {
        staging = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.join(".0").join("a - page 2.pdf").exists());
        staging.is_some()
    });
    let staging = staging.unwrap();
    assert!(staging.file_name().unwrap().to_string_lossy().starts_with(".gezik-"), "{}", staging.display());
    let placed: Vec<_> = std::fs::read_dir(staging.join("x")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(placed.is_empty(), "{placed:?}");
    std::fs::remove_file(&hold).unwrap();
    let (report, _) = finish_with(&engine, job, None);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, a);
    assert_eq!(
        names(&d),
        ["a.pdf", "a.pdf.args", "a.pdf.log", "ok - page 1.pdf", "ok.pdf", "ok.pdf.args", "ok.pdf.log"]
    );
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn cancel_ends_the_worker_and_leaves_nothing() {
    let d = dir("cancel");
    let h = script(&d, "h.pdf", "pages 5\nhang\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![h], fake_tools()), None);
    wait("the worker to start", || d.join("h.pdf.log").exists());
    let started = Instant::now();
    engine.cancel(job);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.cancelled);
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    let pid: u32 = std::fs::read_to_string(d.join("h.pdf.log")).unwrap().lines().next().unwrap().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
    assert_eq!(names(&d), ["h.pdf", "h.pdf.args", "h.pdf.log"]);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn a_hanging_worker_is_ended_by_cancel_at_once() {
    // A worker that never answers (it waits before its first reply): cancel still ends it.
    let d = dir("hang-early");
    let h = script(&d, "h.pdf", "pages 2\nhold\n");
    std::fs::write(d.join("h.pdf.hold"), "").unwrap();
    let engine = engine(&d);
    let job =
        engine.submit_chain(pdf_chain(PdfWork::Merge, vec![h.clone(), script(&d, "i.pdf", "")], fake_tools()), None);
    wait("the worker to start", || d.join("h.pdf.log").exists());
    let started = Instant::now();
    engine.cancel(job);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.cancelled && started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    let pid: u32 = std::fs::read_to_string(d.join("h.pdf.log")).unwrap().lines().next().unwrap().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn a_pause_ends_the_worker_and_the_pdf_is_done_once_after_resume() {
    let d = dir("pause");
    let h = script(&d, "h.pdf", "pages 2\nhold\n");
    let hold = d.join("h.pdf.hold");
    std::fs::write(&hold, "").unwrap();
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![h], fake_tools()), None);
    wait("the worker to start", || d.join("h.pdf.log").exists());
    engine.pause(job);
    let pid: u32 = std::fs::read_to_string(d.join("h.pdf.log")).unwrap().lines().next().unwrap().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
    std::fs::remove_file(&hold).unwrap();
    engine.resume(job);
    let (report, events) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!report.cancelled);
    assert_eq!(names(&d), ["h - page 1.pdf", "h - page 2.pdf", "h.pdf", "h.pdf.args", "h.pdf.log"]);
    let log = std::fs::read_to_string(d.join("h.pdf.log")).unwrap();
    assert_eq!(log.lines().count(), 2, "{log}");
    for event in &events {
        if let Event::Progress { job: j, progress } = event
            && *j == job
        {
            assert!(progress.items_done <= progress.items_total, "{progress:?}");
        }
    }
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn pages_done_before_a_pause_are_not_counted_twice() {
    let d = dir("pause-counted");
    let s = script(&d, "s.pdf", "pages 3\nslow 400\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![s], fake_tools()), None);
    let mut events = Vec::new();
    wait("a page to be done", || {
        let new = engine.drain();
        let done = new.iter().any(|e| matches!(e, Event::Progress { progress, .. } if progress.items_done >= 1));
        events.extend(new);
        done
    });
    engine.pause(job);
    let pid: u32 = std::fs::read_to_string(d.join("s.pdf.log")).unwrap().lines().next().unwrap().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
    engine.resume(job);
    let (report, rest) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    events.extend(rest);
    let progress: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Progress { job: j, progress } if *j == job => Some(progress.clone()),
            _ => None,
        })
        .collect();
    // The progress reporter samples the job ten times a second, over the whole chain: a sample
    // may fall in the place stage after the pages, whose 3 files moved out of staging are items
    // of their own (3 of 6, with their bytes). The pages are counted with no bytes, so the
    // samples without bytes are the page stage's: a try after the pause that counted its pages
    // again would show more than 3 there for its whole run (3 slow pages).
    assert!(progress.iter().all(|p| p.items_done <= p.items_total && p.items_total <= 6), "{progress:?}");
    let pages: Vec<_> = progress.iter().filter(|p| p.bytes_total == 0).collect();
    assert!(!pages.is_empty() && pages.iter().all(|p| p.items_total <= 3), "{progress:?}");
    assert_eq!(std::fs::read_to_string(d.join("s.pdf.log")).unwrap().lines().count(), 2);
    assert_eq!(names(&d), ["s - page 1.pdf", "s - page 2.pdf", "s - page 3.pdf", "s.pdf", "s.pdf.args", "s.pdf.log"]);
}

#[test]
fn existing_outputs_go_through_the_conflict_list() {
    let d = dir("conflict");
    let a = script(&d, "a.pdf", "pages 2\n");
    std::fs::write(d.join("a - page 1.pdf"), b"old").unwrap();
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![a], fake_tools()), None);
    let (report, events) = finish_with(&engine, job, Some(Decision::Skip));
    assert!(events.iter().any(|e| matches!(e, Event::Conflicts { .. })));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(d.join("a - page 1.pdf")).unwrap(), b"old");
    assert!(d.join("a - page 2.pdf").is_file());
    undo(&engine);
    assert_eq!(std::fs::read(d.join("a - page 1.pdf")).unwrap(), b"old");
    assert!(!d.join("a - page 2.pdf").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn a_lowered_page_is_a_note_not_a_failure() {
    let d = dir("lowered");
    let a = script(&d, "a.pdf", "pages 2\nlowered 2 41\n");
    let engine = engine(&d);
    let work = PdfWork::Render { dpi: 300, image: PageImage::Png };
    let job = engine.submit_chain(pdf_chain(work, vec![a], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].path, d.join("a - page 2.png"));
    assert_eq!(report.skipped[0].message, "page 2 was made at 41 dpi: at 300 dpi it would be too large");
    assert!(d.join("a - page 1.png").is_file() && d.join("a - page 2.png").is_file());
    // A render that fails after all made no pictures, so nothing is said of their dpi.
    let b = script(
        &d,
        "b.pdf",
        "pages 2
lowered 2 41
late-failure
",
    );
    let work = PdfWork::Render { dpi: 300, image: PageImage::Png };
    let job = engine.submit_chain(pdf_chain(work, vec![b.clone()], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert!(!d.join("b - page 2.png").exists());
}

#[test]
fn a_one_page_render_lowered_keeps_its_note() {
    // As `huge.pdf` in the GUI run: one page, JPEG, lowered to 40 dpi.
    let d = dir("lowered-one");
    let huge = script(&d, "huge.pdf", "pages 1\nlowered 1 40\n");
    let engine = engine(&d);
    let work = PdfWork::Render { dpi: 300, image: PageImage::Jpeg };
    let job = engine.submit_chain(pdf_chain(work, vec![huge], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert!(!report.cancelled && report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].path, d.join("huge - page 1.jpg"));
    assert_eq!(report.skipped[0].message, "page 1 was made at 40 dpi: at 300 dpi it would be too large");
    assert!(d.join("huge - page 1.jpg").is_file());
}

#[test]
fn damaged_pdfs_fail_with_their_reason() {
    let d = dir("damaged");
    let a = script(&d, "a.pdf", "damaged\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![a.clone()], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, a);
    assert!(report.failures[0].message.starts_with("the PDF is damaged"), "{}", report.failures[0].message);
    // In a merge the damaged one is named, not the first.
    let ok = script(&d, "ok.pdf", "pages 1\n");
    let job = engine.submit_chain(pdf_chain(PdfWork::Merge, vec![ok, a.clone()], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, a);
    assert!(!d.join("ok (merged).pdf").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn pictures_make_one_pdf_that_one_undo_trashes() {
    let d = dir("pictures");
    let pics: Vec<PathBuf> = (0..3).map(|i| jpeg(&d.join(format!("{i}.jpg")), 16, 12, [9, 9, 9])).collect();
    let bad = d.join("x.png");
    std::fs::write(&bad, b"nope").unwrap();
    let mut all = pics.clone();
    all.push(bad.clone());
    let out = d.join("d.pdf");
    let engine = engine(&d);
    let task = ImagesToPdfTask::new(all, out.clone(), PageOptions::DEFAULT);
    assert_eq!(task.title(), "Making d.pdf from 4 pictures");
    let report = run(&engine, task);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].path, bad);
    assert!(out.is_file());
    assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF-"));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    undo(&engine);
    assert!(!out.exists());
    // An existing output asks first.
    std::fs::write(&out, b"old").unwrap();
    let job = engine.submit(Box::new(ImagesToPdfTask::new(pics, out.clone(), PageOptions::DEFAULT)));
    let (_, events) = finish_with(&engine, job, Some(Decision::Skip));
    assert!(events.iter().any(|e| matches!(e, Event::Conflicts { .. })));
    assert_eq!(std::fs::read(&out).unwrap(), b"old");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn split_and_render_with_real_pdfium() {
    let Some(lib) = std::env::var_os("GEZIK_TEST_PDFIUM").map(PathBuf::from) else { return };
    let d = dir("real");
    let pics: Vec<PathBuf> = (0..3).map(|i| jpeg(&d.join(format!("{i}.jpg")), 40, 30, [200, 40, 9])).collect();
    let pdf = d.join("p.pdf");
    let engine = engine(&d);
    let report = run(&engine, ImagesToPdfTask::new(pics, pdf.clone(), PageOptions::DEFAULT));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let worker = Worker {
        program: root().join(format!("pdf_worker{}", std::env::consts::EXE_SUFFIX)),
        args: vec![WORKER_ARG.into()],
    };
    let tools = PdfTools { worker: worker.clone(), library: lib.clone() };
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![pdf.clone()], tools.clone()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    for page in 1..=3 {
        let part = d.join(format!("p - page {page}.pdf"));
        assert_eq!(count_pages(&worker, &lib, &part, &|| false).unwrap(), Some(1), "{}", part.display());
    }
    let work = PdfWork::Render { dpi: 150, image: PageImage::Png };
    let job = engine.submit_chain(pdf_chain(work, vec![pdf.clone()], tools), None);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    for page in 1..=3 {
        let png = d.join(format!("p - page {page}.png"));
        assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"), "{}", png.display());
    }
    undo(&engine);
    assert!(!d.join("p - page 1.png").exists());
    undo(&engine);
    assert!(!d.join("p - page 1.pdf").exists());
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
}

#[test]
fn labels_read_well() {
    let a = PathBuf::from("d/a.pdf");
    assert_eq!(pdf_label(&PdfWork::Merge, &[a.clone(), "d/b.pdf".into(), "d/c.pdf".into()]), "Merge 3 PDFs");
    assert_eq!(pdf_label(&PdfWork::Split(Split::EachPage), std::slice::from_ref(&a)), "Split a.pdf");
    assert_eq!(pdf_label(&PdfWork::Split(Split::Every(2)), &[a.clone(), "d/b.pdf".into()]), "Split 2 PDFs");
    assert_eq!(pdf_label(&PdfWork::Extract("1".into()), std::slice::from_ref(&a)), "Extract pages from a.pdf");
    assert_eq!(pdf_label(&PdfWork::Render { dpi: 72, image: PageImage::Png }, &[a]), "Save a.pdf as pictures");
    assert_eq!(images_pdf_label(1), "Make PDF from 1 picture");
    assert_eq!(images_pdf_label(12), "Make PDF from 12 pictures");
}
