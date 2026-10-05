//! Unpacking one archive into a staging folder: the questions first (not enough space, a
//! suspected bomb), then every entry with progress, passwords asked through the engine.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gezik_core::batch::archive::bomb_suspect;
use gezik_core::format_size;
use gezik_ops::{Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

use super::{cancelled, external, file_name, seven_zip_needed};
use crate::archive::{self, ArchiveSource, ExtractCx};

/// The folder inside the staging folder that the entries go into: placing it whole gives
/// `dir/<stem>/` in one move.
const CONTENT: &str = "x";

pub(super) struct ExtractTask {
    archive: PathBuf,
    /// Where it is extracted to (the staging folder is made in it, on the same drive).
    dir: PathBuf,
    seven_zip: Option<PathBuf>,
    /// Set to the unpacked contents once done; the `PlaceTask` after it reads it.
    stage: Arc<Mutex<Option<PathBuf>>>,
    title: String,
}

impl ExtractTask {
    pub fn new(
        archive: PathBuf,
        dir: PathBuf,
        seven_zip: Option<PathBuf>,
        stage: Arc<Mutex<Option<PathBuf>>>,
        title: String,
    ) -> ExtractTask {
        ExtractTask { archive, dir, seven_zip, stage, title }
    }

    /// A choice between "go on" and "Cancel"; true to go on. A cancelled job is an error.
    fn confirm(&self, cx: &RunCx<'_>, title: &str, message: String, go: &str) -> io::Result<bool> {
        let question =
            Question::Confirm { title: title.to_owned(), message, buttons: vec![go.to_owned(), "Cancel".to_owned()] };
        match cx.ask(question) {
            Answer::Button(0) => Ok(true),
            _ if cx.stopped() => Err(cancelled()),
            _ => Ok(false),
        }
    }

    /// Lists the archive and asks what the sizes call for; false: the user said no.
    fn check(&self, source: &mut dyn ArchiveSource, run: &RunCx<'_>, cx: &Adapter<'_, '_>) -> io::Result<bool> {
        let Some(entries) = source.list(cx)? else {
            // A stream (tar.*, .gz…): no sizes to ask about; a full disk pauses the job.
            run.found(0, 0);
            return Ok(true);
        };
        let sizes: Option<Vec<u64>> = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).collect();
        let total = sizes.as_ref().map_or(0, |sizes| sizes.iter().sum());
        cx.counting.set(true);
        run.found(entries.len() as u64, total);
        if sizes.is_none() {
            return Ok(true);
        }
        let needed = total.saturating_add(total / 20);
        if let Ok(free) = gezik_platform::fs::free_space(&self.dir)
            && needed > free
        {
            let drive = gezik_platform::fs::drive_root(&self.dir).unwrap_or_else(|| self.dir.clone());
            let message = format!("{} needed, {} free on {}", format_size(needed), format_size(free), drive.display());
            if !self.confirm(run, "Not enough space", message, "Extract anyway")? {
                return Ok(false);
            }
        }
        let packed = std::fs::metadata(&self.archive).map_or(0, |meta| meta.len());
        if bomb_suspect(total, packed) {
            let ratio = total / packed.max(1);
            let message = format!(
                "This archive says it unpacks to {} ({ratio}× its size). It may be made to fill the disk.",
                format_size(total)
            );
            if !self.confirm(run, "Very large archive", message, "Extract")? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Task for ExtractTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Extract
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.archive.clone(), self.dir.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        match std::fs::metadata(&self.archive) {
            Ok(meta) => {
                let facts = Facts { is_dir: false, size: meta.len(), modified: meta.modified().ok() };
                // Its entries count instead, once listed.
                sink.item(PlanItem::new(Stage::Parallel, facts).source(&self.archive).top(0).uncounted());
            }
            Err(err) => sink.failed(&self.archive, err),
        }
    }

    fn run(&self, _item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        // A format, or a method inside a known format, that only 7-Zip reads: found when the
        // archive is opened or listed, before anything is written.
        let only_seven_zip = |err: &io::Error| err.kind() == io::ErrorKind::Unsupported;
        let content = match archive::open(&self.archive) {
            Ok(mut source) => {
                let cx = Adapter { run, archive: &self.archive, counting: false.into() };
                match self.check(source.as_mut(), run, &cx) {
                    Ok(true) => {
                        let content = self.content_dir(run)?;
                        source.extract(&content, &cx)?;
                        content
                    }
                    Ok(false) => return Ok(Outcome::Nothing),
                    Err(err) if only_seven_zip(&err) => self.with_seven_zip(run)?,
                    Err(err) => return Err(err),
                }
            }
            Err(err) if only_seven_zip(&err) => self.with_seven_zip(run)?,
            Err(err) => return Err(err),
        };
        *self.stage.lock().unwrap_or_else(|e| e.into_inner()) = Some(content);
        // What lands is reported by the PlaceTask.
        Ok(Outcome::Nothing)
    }
}

impl ExtractTask {
    /// Extracts with 7-Zip into a fresh staging folder; "7-Zip needed" when there is none.
    fn with_seven_zip(&self, run: &RunCx<'_>) -> io::Result<PathBuf> {
        let Some(seven_zip) = &self.seven_zip else { return Err(seven_zip_needed()) };
        let content = self.content_dir(run)?;
        external::extract(seven_zip, &self.archive, &content, run)?;
        Ok(content)
    }

    /// A fresh staging folder in the target folder, and the folder for the entries in it.
    fn content_dir(&self, run: &RunCx<'_>) -> io::Result<PathBuf> {
        let stage = run.staging_dir(&self.dir.join(CONTENT))?;
        let content = stage.join(CONTENT);
        std::fs::create_dir(&content)?;
        Ok(content)
    }
}

/// Extraction's needs, met by the engine: progress, cancel, questions, failure rows.
struct Adapter<'a, 'r> {
    run: &'a RunCx<'r>,
    archive: &'a Path,
    /// The entries were listed and counted in the job's totals: each one done counts.
    counting: std::cell::Cell<bool>,
}

impl ExtractCx for Adapter<'_, '_> {
    fn add_bytes(&self, n: u64) {
        self.run.add_bytes(n);
    }

    fn entry_done(&self) {
        if self.counting.get() {
            self.run.one_done(0);
        }
    }

    fn stopped(&self) -> bool {
        self.run.stopped()
    }

    fn password(&self, retry: bool) -> Option<String> {
        match self.run.ask(Question::Password { archive: self.archive.to_path_buf(), retry }) {
            Answer::Text(password) => Some(password),
            _ => None,
        }
    }

    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.run.fail(&self.path(name), error);
        self.entry_done();
    }

    fn entry_skipped(&self, name: &str, why: &io::Error) {
        self.run.skip(&self.path(name), why);
        self.entry_done();
    }
}

impl Adapter<'_, '_> {
    /// The archive itself (skipped for want of a password), or an entry inside it.
    fn path(&self, name: &str) -> PathBuf {
        if name == file_name(self.archive) { self.archive.to_path_buf() } else { self.archive.join(name) }
    }
}
