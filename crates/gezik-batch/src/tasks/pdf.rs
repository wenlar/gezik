//! Merge, split, extract and "PDF to pictures" as one engine job: the PDF worker (a separate
//! process, see [`crate::pdf::client`]) writes into a staging folder next to the first input,
//! then a `PlaceTask` moves what it made there through the conflict list; one undo trashes it.
//!
//! An encrypted PDF's password is asked through the engine and passed only on the worker's
//! input. A pause ends the worker (it would go on using the machine) and the PDF is done again
//! from the start once the job is resumed; cancel ends it at once.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use gezik_core::batch::pdf::{PageImage, Split, page_image_name};
use gezik_core::batch::pdf_worker::{Reply, Request, WorkerJob};
use gezik_core::ops::names::next_free;
use gezik_ops::{Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

use super::place::PlaceTask;
use super::{cancelled, file_name};
use crate::pdf::client::{self, Ended, PdfFailed, Worker};

/// What one PDF job does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfWork {
    Merge,
    Split(Split),
    /// The pages these ranges name, into one PDF.
    Extract(String),
    /// Every page as a picture.
    Render {
        dpi: u32,
        image: PageImage,
    },
}

/// What PDF work runs with: the worker program and the pdfium library it loads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfTools {
    pub worker: Worker,
    pub library: PathBuf,
}

/// The folder in the staging folder that the outputs go into.
const CONTENT: &str = "x";

/// One "Merge/Split/Extract/PDF to images": the worker into a staging folder, then placed
/// next to the first input with the conflict list. Submit with
/// `submit_chain(pdf_chain(..), Some(pdf_label(..)))`.
pub fn pdf_chain(work: PdfWork, inputs: Vec<PathBuf>, tools: PdfTools) -> Vec<Box<dyn Task>> {
    let Some(first) = inputs.first() else { return Vec::new() };
    let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
    let stage = Arc::new(Mutex::new(None));
    let task = PdfTask { work, inputs, tools, dir: dir.clone(), stage: stage.clone(), counts: Mutex::default() };
    let title = task.title();
    vec![Box::new(task), Box::new(PlaceTask::outputs(dir, stage, TaskKind::Pdf, title))]
}

/// What Undo says: "Merge 3 PDFs", "Split a.pdf" / "Split 2 PDFs", "Extract pages from a.pdf",
/// "Save a.pdf as pictures".
pub fn pdf_label(work: &PdfWork, inputs: &[PathBuf]) -> String {
    let what = pdfs(inputs);
    match work {
        PdfWork::Merge => format!("Merge {} PDFs", inputs.len()),
        PdfWork::Split(_) => format!("Split {what}"),
        PdfWork::Extract(_) => format!("Extract pages from {what}"),
        PdfWork::Render { .. } => format!("Save {what} as pictures"),
    }
}

/// `a.pdf`, or `3 PDFs`.
fn pdfs(inputs: &[PathBuf]) -> String {
    match inputs {
        [one] => file_name(one),
        many => format!("{} PDFs", many.len()),
    }
}

pub(super) struct PdfTask {
    work: PdfWork,
    inputs: Vec<PathBuf>,
    tools: PdfTools,
    /// Where the outputs go (the first input's folder); the staging folder is made in it.
    dir: PathBuf,
    /// The staging folder's content folder, once made; the `PlaceTask` after it reads it.
    stage: Arc<Mutex<Option<PathBuf>>>,
    /// By item: the most page steps found and done in any try, so that a try after a pause
    /// (or a password) counts only what goes past the earlier ones.
    counts: Mutex<HashMap<usize, (u64, u64)>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl PdfTask {
    fn merging(&self) -> bool {
        self.work == PdfWork::Merge
    }

    fn job(&self) -> WorkerJob {
        match &self.work {
            PdfWork::Merge => WorkerJob::Merge,
            PdfWork::Split(split) => WorkerJob::Split(split.clone()),
            PdfWork::Extract(ranges) => WorkerJob::Extract(ranges.clone()),
            PdfWork::Render { dpi, image } => WorkerJob::Render { dpi: *dpi, image: *image },
        }
    }

    /// The content folder of the job's staging folder, made the first time.
    fn content(&self, run: &RunCx<'_>) -> io::Result<PathBuf> {
        let mut stage = lock(&self.stage);
        if let Some(content) = &*stage {
            return Ok(content.clone());
        }
        let staging = run.staging_dir(&self.dir.join(CONTENT))?;
        let content = staging.join(CONTENT);
        std::fs::create_dir(&content)?;
        *stage = Some(content.clone());
        Ok(content)
    }

    /// Counts a reply's steps against what the item counted in earlier tries.
    fn tally(&self, item: usize, this: &mut (u64, u64), reply: &Reply, run: &RunCx<'_>) {
        let mut counts = lock(&self.counts);
        let most = counts.entry(item).or_default();
        match reply {
            Reply::Steps(n) => {
                this.0 = this.0.saturating_add(*n);
                if this.0 > most.0 {
                    run.found(this.0 - most.0, 0);
                    most.0 = this.0;
                }
            }
            Reply::Step => {
                this.1 += 1;
                // Never more done than found.
                if this.1 > most.1 && most.1 < most.0 {
                    run.one_done(0);
                    most.1 += 1;
                }
            }
            _ => {}
        }
    }
}

/// Moves every file the worker wrote in `out` into `content` (the same drive); a name another
/// input's outputs already took there gets a number.
fn gather(out: &Path, content: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(out)? {
        let entry = entry?;
        let name = entry.file_name();
        let mut target = content.join(&name);
        if std::fs::symlink_metadata(&target).is_ok() {
            let text = name.to_string_lossy();
            let free = next_free(&text, false, |candidate| std::fs::symlink_metadata(content.join(candidate)).is_ok());
            target = content.join(OsString::from(free));
        }
        std::fs::rename(entry.path(), target)?;
    }
    Ok(())
}

impl Task for PdfTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Pdf
    }

    fn title(&self) -> String {
        let what = pdfs(&self.inputs);
        match &self.work {
            PdfWork::Merge => format!("Merging {} PDFs", self.inputs.len()),
            PdfWork::Split(_) => format!("Splitting {what}"),
            PdfWork::Extract(_) => format!("Extracting pages from {what}"),
            PdfWork::Render { image: PageImage::Png, .. } => format!("Saving {what} as PNG pictures"),
            PdfWork::Render { image: PageImage::Jpeg, .. } => format!("Saving {what} as JPEG pictures"),
        }
    }

    fn count(&self) -> usize {
        self.inputs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.inputs.clone();
        paths.push(self.dir.clone());
        Resources { paths, work: Work::Cpu }
    }

    fn workers(&self) -> Option<usize> {
        // One worker at a time: pdfium is single-threaded, and each worker may use a lot of
        // memory.
        Some(1)
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let mut facts = Vec::new();
        for input in &self.inputs {
            match std::fs::metadata(input) {
                Ok(meta) => facts.push(Some(Facts { is_dir: false, size: meta.len(), modified: meta.modified().ok() })),
                Err(err) => {
                    sink.failed(input, err);
                    facts.push(None);
                }
            }
        }
        // Its pages count instead, once the worker has opened it.
        let item = |root: usize, facts: Facts| {
            PlanItem::new(Stage::Parallel, facts).source(&self.inputs[root]).top(root).uncounted()
        };
        if self.merging() {
            // All of them or nothing.
            let Some(all) = facts.into_iter().collect::<Option<Vec<Facts>>>() else { return };
            let size = all.iter().map(|f| f.size).sum();
            sink.item(item(0, Facts { size, ..all[0] }));
            return;
        }
        for (root, facts) in facts.into_iter().enumerate() {
            if let Some(facts) = facts
                && !sink.item(item(root, facts))
            {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        let index = item.root;
        let inputs: Vec<PathBuf> = if self.merging() { self.inputs.clone() } else { vec![self.inputs[index].clone()] };
        let content = self.content(run)?;
        let out = content.join(format!(".{index}"));
        let mut passwords: Vec<(usize, String)> = Vec::new();
        loop {
            let _ = std::fs::remove_dir_all(&out);
            std::fs::create_dir(&out)?;
            let request = Request {
                library: self.tools.library.clone(),
                dir: out.clone(),
                job: self.job(),
                inputs: inputs.clone(),
                passwords: passwords.clone(),
            };
            let mut this = (0u64, 0u64);
            let mut on_reply = |reply: &Reply| match reply {
                Reply::Steps(_) | Reply::Step => self.tally(index, &mut this, reply, run),
                Reply::Lowered { page, dpi } => {
                    let PdfWork::Render { dpi: asked, image } = &self.work else { return };
                    let name = page_image_name(&inputs[0], page.saturating_sub(1), *image);
                    let why = io::Error::other(format!(
                        "page {page} was made at {dpi} dpi: at {asked} dpi it would be too large"
                    ));
                    run.skip(&self.dir.join(name), &why);
                }
                _ => {}
            };
            // Whether a pause ended the worker: by the time it is reaped the job may be resumed.
            let paused = std::cell::Cell::new(false);
            let stop = || {
                if run.cancelled() {
                    return true;
                }
                let now = run.paused();
                paused.set(paused.get() || now);
                now
            };
            let result = client::run(&self.tools.worker, &request, &mut on_reply, &stop);
            let (input, retry) = match result {
                Ok(Ended::Done) => {
                    let gathered = gather(&out, &content);
                    let _ = std::fs::remove_dir_all(&out);
                    gathered?;
                    // What lands is reported by the PlaceTask.
                    return Ok(Outcome::Nothing);
                }
                Ok(Ended::NeedsPassword(i)) => (i, false),
                Ok(Ended::WrongPassword(i)) => (i, true),
                Err(err) => {
                    let _ = std::fs::remove_dir_all(&out);
                    if run.cancelled() {
                        return Err(cancelled());
                    }
                    if paused.get() || run.paused() {
                        return Err(gezik_ops::restart());
                    }
                    // In a merge, the input that failed is named, not the first.
                    if let Some(failed) = err.get_ref().and_then(|e| e.downcast_ref::<PdfFailed>())
                        && let Some(i) = failed.input
                        && i > 0
                        && let Some(path) = inputs.get(i)
                    {
                        run.fail(path, &err);
                        return Ok(Outcome::Nothing);
                    }
                    return Err(err);
                }
            };
            let _ = std::fs::remove_dir_all(&out);
            let Some(path) = inputs.get(input) else {
                return Err(io::Error::other("the PDF engine asked for the password of a PDF it was not given"));
            };
            match run.ask(Question::Password { archive: path.clone(), retry }) {
                Answer::Text(password) => {
                    passwords.retain(|(i, _)| *i != input);
                    passwords.push((input, password));
                }
                _ if run.cancelled() => return Err(cancelled()),
                _ => {
                    let why =
                        if self.merging() { "no password given; nothing was merged" } else { "no password given" };
                    run.skip(path, &io::Error::other(why));
                    return Ok(Outcome::Nothing);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(work: PdfWork, inputs: &[&str]) -> PdfTask {
        let tools = PdfTools { worker: Worker { program: "w".into(), args: vec![] }, library: "lib".into() };
        let inputs: Vec<PathBuf> = inputs.iter().map(PathBuf::from).collect();
        PdfTask { work, inputs, tools, dir: "d".into(), stage: Arc::default(), counts: Mutex::default() }
    }

    #[test]
    fn titles_and_resources() {
        assert_eq!(task(PdfWork::Merge, &["d/a.pdf", "d/b.pdf", "d/c.pdf"]).title(), "Merging 3 PDFs");
        assert_eq!(task(PdfWork::Split(Split::EachPage), &["d/a.pdf"]).title(), "Splitting a.pdf");
        assert_eq!(task(PdfWork::Extract("1".into()), &["d/a.pdf"]).title(), "Extracting pages from a.pdf");
        let png = PdfWork::Render { dpi: 72, image: PageImage::Png };
        assert_eq!(task(png, &["d/a.pdf"]).title(), "Saving a.pdf as PNG pictures");
        let jpeg = task(PdfWork::Render { dpi: 72, image: PageImage::Jpeg }, &["d/a.pdf", "d/b.pdf"]);
        assert_eq!(jpeg.title(), "Saving 2 PDFs as JPEG pictures");
        assert_eq!(jpeg.count(), 2);
        assert_eq!(jpeg.kind(), TaskKind::Pdf);
        assert_eq!(jpeg.workers(), Some(1));
        let resources = jpeg.resources();
        assert_eq!(resources.work, Work::Cpu);
        assert_eq!(resources.paths, [PathBuf::from("d/a.pdf"), "d/b.pdf".into(), "d".into()]);
        let chain = pdf_chain(PdfWork::Merge, vec!["d/a.pdf".into(), "d/b.pdf".into()], jpeg.tools.clone());
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[1].kind(), TaskKind::Pdf);
        assert_eq!(chain[1].title(), "Merging 2 PDFs");
        assert!(pdf_chain(PdfWork::Merge, Vec::new(), jpeg.tools.clone()).is_empty());
    }

    #[test]
    fn same_names_from_two_inputs_get_numbers() {
        let d = std::env::temp_dir().join(format!("gezik-pdf-gather-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (out, content) = (d.join("out"), d.join("content"));
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("a - page 1.pdf"), "first").unwrap();
        std::fs::write(out.join("a - page 1.pdf"), "second").unwrap();
        std::fs::write(out.join("a - page 2.pdf"), "two").unwrap();
        gather(&out, &content).unwrap();
        assert_eq!(std::fs::read_to_string(content.join("a - page 1.pdf")).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(content.join("a - page 1 (2).pdf")).unwrap(), "second");
        assert_eq!(std::fs::read_to_string(content.join("a - page 2.pdf")).unwrap(), "two");
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }
}
