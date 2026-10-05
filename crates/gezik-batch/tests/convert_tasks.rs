//! Converting and running user commands as engine jobs: outputs next to the inputs, in a
//! subfolder or in place of the originals, conflicts, inputs left out, ffmpeg (the fake one),
//! user commands with and without an output (`examples/fake_tool.rs`), and one undo that
//! brings every original back byte for byte.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::convert::ffmpeg::find_ffmpeg;
use gezik_batch::tasks::{CommandTask, ConvertTask, ConvertTools, ConvertWhat, is_ffmpeg_needed};
use gezik_core::batch::convert::{CommandSpec, Eol, ImageFormat, ImageOptions, MediaPreset, Output, TextOptions};
use gezik_ops::{Answer, Decision, Engine, Event, JobId, Report, Settings, Task};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-convert-task-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending-deletes")), ..Settings::default() }, || {})
}

/// Runs `job` to the end; `decision` settles any conflicts (default: their defaults).
fn finish_with(engine: &Engine, job: JobId, decision: Option<Decision>) -> (Report, Vec<Event>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = Vec::new();
    loop {
        for event in engine.drain() {
            match &event {
                Event::Conflicts { job: j, conflicts } if *j == job => {
                    let decisions = conflicts.iter().map(|c| decision.unwrap_or(c.decision)).collect();
                    engine.decide(job, decisions);
                }
                Event::Question { job: j, id, .. } if *j == job => engine.answer(job, *id, Answer::Cancel),
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

fn run(engine: &Engine, task: impl Task + 'static) -> Report {
    finish_with(engine, engine.submit(Box::new(task)), None).0
}

fn undo(engine: &Engine) -> Report {
    let report = finish_with(engine, engine.undo().expect("something to undo"), None).0;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    report
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

/// A small picture at `path`, in the format its extension says.
fn picture(path: &Path) {
    let img = image::RgbImage::from_fn(8, 6, |x, y| image::Rgb([(x * 30) as u8, (y * 40) as u8, 128]));
    img.save(path).unwrap();
}

fn image_options(format: ImageFormat) -> ImageOptions {
    ImageOptions { format, ..ImageOptions::DEFAULT }
}

fn text_options(eol: Eol) -> TextOptions {
    TextOptions { from: None, to: String::new(), bom: false, eol, trim_trailing: false, final_newline: false }
}

fn convert(inputs: Vec<PathBuf>, what: ConvertWhat, output: Output) -> ConvertTask {
    ConvertTask::new(inputs, what, output, ConvertTools::default())
}

/// A program `cargo test` builds with the examples (`target/<profile>/examples/<name>`).
fn example(name: &str) -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let path = deps.parent().unwrap().join("examples").join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(path.is_file(), "{} is missing: run `cargo test -p gezik-batch` (it builds the examples)", path.display());
    path
}

fn command(run: &[&str], output: Option<&str>) -> CommandSpec {
    let mut list = vec![example("fake_tool").to_string_lossy().into_owned()];
    list.extend(run.iter().map(|s| s.to_string()));
    CommandSpec {
        name: "Fake".into(),
        run: list,
        output: output.map(str::to_owned),
        types: Vec::new(),
        folders: false,
        parallel: 2,
    }
}

#[test]
fn a_jpeg_converts_to_png_next_to_it_and_undo_trashes_the_png() {
    let d = dir("jpeg-png");
    let jpg = d.join("photo.jpg");
    picture(&jpg);
    let before = std::fs::read(&jpg).unwrap();
    let engine = engine(&d);
    let task = convert(vec![jpg.clone()], ConvertWhat::Image(image_options(ImageFormat::Png)), Output::SameFolder);
    let report = run(&engine, task);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let png = d.join("photo.png");
    assert_eq!(report.results, std::slice::from_ref(&png));
    assert_eq!(image::open(&png).unwrap().into_rgb8().dimensions(), (8, 6));
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    assert_eq!(engine.undo_label().as_deref(), Some("Convert 1 item"));
    undo(&engine);
    assert_eq!(names(&d), ["photo.jpg"]);
    assert_eq!(std::fs::read(&jpg).unwrap(), before);

    // The same extension: a new name beside it.
    let task = convert(vec![jpg.clone()], ConvertWhat::Image(image_options(ImageFormat::Jpeg)), Output::SameFolder);
    let report = run(&engine, task);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(names(&d), ["photo (converted).jpg", "photo.jpg"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn replacing_originals_trashes_them_and_undo_brings_back_their_bytes() {
    let d = dir("replace");
    let (png, jpg) = (d.join("a.png"), d.join("b.jpg"));
    picture(&png);
    picture(&jpg);
    let (png_bytes, jpg_bytes) = (std::fs::read(&png).unwrap(), std::fs::read(&jpg).unwrap());
    let engine = engine(&d);
    // a.png becomes a.jpg (the original goes); b.jpg is rewritten under its own name.
    let what = ConvertWhat::Image(ImageOptions { quality: 40, ..image_options(ImageFormat::Jpeg) });
    let report = run(&engine, convert(vec![png.clone(), jpg.clone()], what, Output::ReplaceOriginal));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(names(&d), ["a.jpg", "b.jpg"]);
    assert_ne!(std::fs::read(&jpg).unwrap(), jpg_bytes, "re-encoded");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let notes = gezik_ops::PendingDeletes::new(d.join("pending-deletes"));
    assert!(notes.restores().is_empty(), "nothing is left noted aside");
    undo(&engine);
    assert_eq!(names(&d), ["a.png", "b.jpg"]);
    assert_eq!(std::fs::read(&png).unwrap(), png_bytes);
    assert_eq!(std::fs::read(&jpg).unwrap(), jpg_bytes);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn remove_location_keeps_each_format_in_place() {
    let d = dir("location");
    let (jpg, png) = (d.join("a.jpg"), d.join("b.png"));
    picture(&jpg);
    picture(&png);
    let jpg_bytes = std::fs::read(&jpg).unwrap();
    let engine = engine(&d);
    let what = ConvertWhat::RemoveLocation(ImageOptions { strip_metadata: true, ..ImageOptions::DEFAULT });
    let report = run(&engine, convert(vec![jpg.clone(), png.clone()], what, Output::ReplaceOriginal));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(names(&d), ["a.jpg", "b.png"]);
    // A JPEG without location data is not re-encoded: the same bytes.
    assert_eq!(std::fs::read(&jpg).unwrap(), jpg_bytes);
    assert_eq!(image::open(&png).unwrap().into_rgb8().dimensions(), (8, 6));
    undo(&engine);
    assert_eq!(std::fs::read(&jpg).unwrap(), jpg_bytes);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn text_replaces_its_original_and_undo_brings_it_back() {
    let d = dir("text");
    let txt = d.join("notes.txt");
    let original = b"one\r\ntwo\r\n".to_vec();
    std::fs::write(&txt, &original).unwrap();
    let binary = d.join("data.txt");
    std::fs::write(&binary, [0u8, 1, 2, 0, 0, 0, 7, 0]).unwrap();
    let engine = engine(&d);
    let task =
        convert(vec![txt.clone(), binary.clone()], ConvertWhat::Text(text_options(Eol::Lf)), Output::ReplaceOriginal);
    let report = run(&engine, task);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(&txt).unwrap(), b"one\ntwo\n");
    // A binary file is left out, not failed, and stays as it was.
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].path, binary);
    assert_eq!(report.skipped[0].message, "looks binary");
    assert_eq!(std::fs::read(&binary).unwrap(), [0u8, 1, 2, 0, 0, 0, 7, 0]);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    undo(&engine);
    assert_eq!(std::fs::read(&txt).unwrap(), original);
    assert_eq!(names(&d), ["data.txt", "notes.txt"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_subfolder_is_made_and_undo_removes_it() {
    let d = dir("subfolder");
    let inputs = vec![d.join("a.png"), d.join("b.bmp")];
    for input in &inputs {
        picture(input);
    }
    let engine = engine(&d);
    let what = ConvertWhat::Image(image_options(ImageFormat::WebpLossless));
    let report = run(&engine, convert(inputs, what, Output::Subfolder));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(names(&d.join("converted")), ["a.webp", "b.webp"]);
    undo(&engine);
    assert_eq!(names(&d), ["a.png", "b.bmp"]);

    // An existing subfolder stays when undone.
    std::fs::create_dir(d.join("converted")).unwrap();
    let what = ConvertWhat::Image(image_options(ImageFormat::Png));
    let report = run(&engine, convert(vec![d.join("b.bmp")], what, Output::Subfolder));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    undo(&engine);
    assert!(d.join("converted").is_dir());
    assert!(names(&d.join("converted")).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_existing_output_goes_through_the_conflict_list() {
    let d = dir("conflict");
    let jpg = d.join("a.jpg");
    picture(&jpg);
    std::fs::write(d.join("a.png"), b"keep me").unwrap();
    let engine = engine(&d);
    let what = || ConvertWhat::Image(image_options(ImageFormat::Png));
    let (report, events) = finish_with(
        &engine,
        engine.submit(Box::new(convert(vec![jpg.clone()], what(), Output::SameFolder))),
        Some(Decision::Skip),
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let asked = events
        .iter()
        .any(|e| matches!(e, Event::Conflicts { conflicts, .. } if conflicts[0].target == d.join("a.png")));
    assert!(asked, "the existing a.png was in the conflict list");
    assert_eq!(std::fs::read(d.join("a.png")).unwrap(), b"keep me");

    // Keep both: a free name.
    let job = engine.submit(Box::new(convert(vec![jpg.clone()], what(), Output::SameFolder)));
    let report = finish_with(&engine, job, Some(Decision::KeepBoth)).0;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(names(&d), ["a (2).png", "a.jpg", "a.png"]);
    assert_eq!(std::fs::read(d.join("a.png")).unwrap(), b"keep me");

    // Replace: the old one goes to the trash, undo brings it back.
    let job = engine.submit(Box::new(convert(vec![jpg.clone()], what(), Output::SameFolder)));
    let report = finish_with(&engine, job, Some(Decision::Replace)).0;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(image::open(d.join("a.png")).is_ok());
    undo(&engine);
    assert_eq!(std::fs::read(d.join("a.png")).unwrap(), b"keep me");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn two_inputs_with_one_output_do_not_overwrite_each_other() {
    let d = dir("same-output");
    let (jpg, png) = (d.join("a.jpg"), d.join("a.png"));
    picture(&jpg);
    picture(&png);
    let engine = engine(&d);
    let what = ConvertWhat::Image(image_options(ImageFormat::WebpLossless));
    let report = run(&engine, convert(vec![jpg.clone(), png.clone()], what, Output::SameFolder));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, png);
    assert!(report.failures[0].message.contains("a.webp"), "{:?}", report.failures);
    assert_eq!(names(&d), ["a.jpg", "a.png", "a.webp"]);

    // One input's output is another input: refused rather than raced.
    let what = ConvertWhat::Image(image_options(ImageFormat::Png));
    let report = run(&engine, convert(vec![jpg.clone(), png.clone()], what, Output::ReplaceOriginal));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].path, jpg);
    assert!(jpg.is_file());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn inputs_of_another_kind_are_left_out() {
    let d = dir("left-out");
    picture(&d.join("a.png"));
    std::fs::write(d.join("notes.txt"), b"text").unwrap();
    let engine = engine(&d);
    let what = ConvertWhat::Image(image_options(ImageFormat::Jpeg));
    let report = run(&engine, convert(vec![d.join("a.png"), d.join("notes.txt")], what, Output::SameFolder));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "{:?}", report.skipped);
    assert_eq!(report.skipped[0].message, "not a picture");
    assert_eq!(names(&d), ["a.jpg", "a.png", "notes.txt"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn media_runs_through_ffmpeg_or_says_it_is_needed() {
    let d = dir("media");
    let input = d.join("song.wav");
    std::fs::write(&input, vec![7u8; 10_000]).unwrap();
    let engine = engine(&d);
    let report = run(&engine, convert(vec![input.clone()], ConvertWhat::Media(MediaPreset::Mp3), Output::SameFolder));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(is_ffmpeg_needed(&report.failures[0].message), "{:?}", report.failures);

    let tools = d.join("tools");
    std::fs::create_dir(&tools).unwrap();
    let ffmpeg = tools.join(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(example("fake_ffmpeg"), &ffmpeg).unwrap();
    let tools = ConvertTools { ffmpeg: find_ffmpeg(&d.join("data"), Some(&ffmpeg)) };
    assert!(tools.ffmpeg.is_some());
    let task = ConvertTask::new(vec![input.clone()], ConvertWhat::Media(MediaPreset::Mp3), Output::SameFolder, tools);
    let report = run(&engine, task);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(d.join("song.mp3")).unwrap(), b"fake output");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    undo(&engine);
    assert!(!d.join("song.mp3").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_command_with_an_output_writes_it_and_undo_trashes_it() {
    let d = dir("command-output");
    let input = d.join("a.txt");
    std::fs::write(&input, b"hello").unwrap();
    let engine = engine(&d);
    let spec = command(&["copy", "{in}", "{out}"], Some("{name}-copy.{ext}"));
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], spec));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.results, [d.join("a-copy.txt")]);
    assert_eq!(std::fs::read(d.join("a-copy.txt")).unwrap(), b"hello");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    assert_eq!(engine.undo_label().as_deref(), Some("Run command on 1 item"));
    undo(&engine);
    assert_eq!(names(&d), ["a.txt"]);

    // Exit 0 without the output is a failure.
    let spec = command(&["nothing"], Some("{name}-copy.{ext}"));
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], spec));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(report.failures[0].message.contains("a-copy.txt"), "{:?}", report.failures);
    assert_eq!(names(&d), ["a.txt"]);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_command_in_place_is_undone_from_a_trashed_copy() {
    let d = dir("command-in-place");
    let input = d.join("a.txt");
    std::fs::write(&input, b"hello").unwrap();
    let engine = engine(&d);
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], command(&["append", "{in}"], None)));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(&input).unwrap(), b"hello changed");
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    undo(&engine);
    assert_eq!(std::fs::read(&input).unwrap(), b"hello");
    assert_eq!(names(&d), ["a.txt"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn special_characters_in_names_pass_through_intact() {
    let d = dir("command-names");
    let name = if cfg!(windows) { "a b; $c & 'd' ğüşİ.txt" } else { "a \"b\"; $c & 'd' ğüşİ `x`.txt" };
    let input = d.join(name);
    std::fs::write(&input, b"x").unwrap();
    let engine = engine(&d);
    let spec = command(&["args", "{out}", "{in}", "{name}", "{ext}", "{dir}"], Some("{name}.log"));
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], spec));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let log = d.join(format!("{}.log", input.file_stem().unwrap().to_string_lossy()));
    let text = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(lines[0], input.to_string_lossy());
    assert_eq!(lines[1], input.file_stem().unwrap().to_string_lossy());
    assert_eq!(lines[2], "txt");
    assert_eq!(lines[3], d.to_string_lossy());
    // It ran in the input's folder.
    assert_eq!(std::fs::canonicalize(lines[4]).unwrap(), std::fs::canonicalize(&d).unwrap());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_failing_command_reports_its_errors_and_leaves_nothing() {
    let d = dir("command-fail");
    let input = d.join("a.txt");
    std::fs::write(&input, b"hello").unwrap();
    let engine = engine(&d);
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], command(&["fail"], Some("{name}.out"))));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    let message = &report.failures[0].message;
    assert!(message.contains("exit code 3") && message.contains("fake tool error 3"), "{message}");
    assert_eq!(names(&d), ["a.txt"]);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));

    // In place: the file is untouched and nothing is recorded to undo.
    let label = engine.undo_label();
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], command(&["fail"], None)));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(std::fs::read(&input).unwrap(), b"hello");
    assert_eq!(engine.undo_label(), label);

    // A program that is not there.
    let mut spec = command(&["copy", "{in}", "{out}"], Some("{name}.out"));
    spec.run[0] = "gezik-missing-tool".into();
    let report = run(&engine, CommandTask::new(vec![(input.clone(), false)], spec));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].message, "gezik-missing-tool not found");
    assert_eq!(names(&d), ["a.txt"]);

    // Cancelled while it runs: it is ended and its unfinished output goes.
    let spec = command(&["hang", "{out}"], Some("{name}.out"));
    let job = engine.submit(Box::new(CommandTask::new(vec![(input.clone(), false)], spec)));
    std::thread::sleep(Duration::from_millis(300));
    engine.cancel(job);
    let report = finish_with(&engine, job, None).0;
    assert!(report.cancelled);
    assert_eq!(names(&d), ["a.txt"]);
    assert!(leftovers(&d).is_empty(), "{:?}", leftovers(&d));
    let _ = std::fs::remove_dir_all(&d);
}
