//! Splitting a file into raw parts (`film.mkv.001`…, with Total Commander's `film.mkv.crc`)
//! and joining them back (spec 11 §12.4–12.5). Each writes into a staging folder in the target
//! folder and a `PlaceTask` moves what it made into place: a name already there meets the
//! conflict list, a cancel or a failure leaves nothing in the target folder, undo trashes what
//! landed. A join checks the `.crc`'s CRC32 and size before anything is placed.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use gezik_core::ops::names::{NameRules, validate_name};
use gezik_core::ops::split::{MAX_PARTS, crc_name, crc_text, first_gap, parse_crc, part_count, part_name, split_base};
use gezik_ops::{Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

use super::file_name;
use super::place::PlaceTask;
use crate::archive::cancelled;

/// Read and written at a time.
const BUFFER: usize = 1 << 20;

type Staged = Arc<Mutex<Option<PathBuf>>>;

/// The tasks of "Split File…": `source` into parts of `size` bytes in `dir`. Submit them with
/// `submit_chain(tasks, Some(split_label(&source)))`.
pub fn split_chain(source: PathBuf, dir: PathBuf, size: u64) -> Vec<Box<dyn Task>> {
    let stage: Staged = Arc::default();
    let title = gezik_i18n::t!("Splitting {}", file_name(&source));
    let place = PlaceTask::outputs(dir.clone(), stage.clone(), TaskKind::Copy, title);
    vec![Box::new(SplitTask { source, dir, size, stage }), Box::new(place)]
}

/// What Undo says: "Split film.mkv".
pub fn split_label(source: &Path) -> String {
    gezik_i18n::t!("Split {}", file_name(source))
}

/// The tasks of "Join Files": the set `part` (any part, or its `.crc`) belongs to, joined in
/// `dir`. Submit them with `submit_chain(tasks, Some(join_label(&part)))`.
pub fn join_chain(part: PathBuf, dir: PathBuf) -> Vec<Box<dyn Task>> {
    let stage: Staged = Arc::default();
    let title = gezik_i18n::t!("Joining {}", join_name(&part));
    let place = PlaceTask::outputs(dir.clone(), stage.clone(), TaskKind::Copy, title);
    vec![Box::new(JoinTask { part, dir, stage, set: OnceLock::new() }), Box::new(place)]
}

/// What Undo says: "Join film.mkv".
pub fn join_label(part: &Path) -> String {
    gezik_i18n::t!("Join {}", join_name(part))
}

/// The name the set of `part` was split from (`film.mkv`).
fn join_name(part: &Path) -> String {
    split_base(&file_name(part)).unwrap_or_else(|| file_name(part))
}

/// Whether `path` names a part (`.001`…) or a `.crc` that Join Files works on.
pub fn is_part(path: &Path) -> bool {
    split_base(&file_name(path)).is_some()
}

/// Copies `len` bytes (all that is left: `None`) from `from` to `to`, adding them to `crc`;
/// stops when the job is cancelled. The bytes read count in the job's progress.
fn pour(
    from: &mut File,
    to: &mut File,
    len: Option<u64>,
    crc: &mut crc32fast::Hasher,
    cx: &RunCx<'_>,
) -> io::Result<u64> {
    let mut buffer = vec![0u8; BUFFER];
    let mut left = len.unwrap_or(u64::MAX);
    let mut done = 0;
    while left > 0 {
        if cx.stopped() {
            return Err(cancelled());
        }
        let want = usize::try_from(left).map_or(BUFFER, |left| left.min(BUFFER));
        let read = from.read(&mut buffer[..want])?;
        if read == 0 {
            break;
        }
        to.write_all(&buffer[..read])?;
        crc.update(&buffer[..read]);
        cx.add_bytes(read as u64);
        done += read as u64;
        left -= read as u64;
    }
    Ok(done)
}

struct SplitTask {
    source: PathBuf,
    dir: PathBuf,
    size: u64,
    /// Where the parts were written, once all are; the `PlaceTask` after it reads it.
    stage: Staged,
}

impl Task for SplitTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        gezik_i18n::t!("Splitting {}", file_name(&self.source))
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.source.clone(), self.dir.clone()], work: Work::Disk, vfs_roots: Vec::new() }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        match std::fs::metadata(&self.source) {
            Ok(meta) if meta.is_file() && part_count(meta.len(), self.size) > MAX_PARTS => {
                let message = gezik_i18n::t!("That makes more than {} parts; choose a bigger size", MAX_PARTS);
                sink.failed(&self.source, io::Error::new(io::ErrorKind::InvalidInput, message));
            }
            Ok(meta) if meta.is_file() => {
                let facts = Facts { is_dir: false, size: meta.len(), modified: meta.modified().ok() };
                sink.item(PlanItem::new(Stage::Parallel, facts).source(&self.source).top(0));
            }
            Ok(_) => {
                let why = gezik_i18n::t!("Only a file can be split");
                sink.failed(&self.source, io::Error::new(io::ErrorKind::InvalidInput, why))
            }
            Err(err) => sink.failed(&self.source, err),
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let name = file_name(&self.source);
        // A set of that name there already is one unit (asked once): replaced whole, or left
        // whole. Never a mix of old and new parts, nor old parts past the new last one.
        let (old_parts, old_crc) = set_files(&self.dir, &name).unwrap_or_default();
        let olds: Vec<PathBuf> = old_parts.into_iter().map(|(_, path, _)| path).chain(old_crc).collect();
        if !olds.is_empty() {
            // TRANSLATORS: {0} is a folder, {1} a file name
            let message = gezik_i18n::tn!(
                "{0} holds {n} file of a split {1} already. Replace that whole set? The old files go to the trash."
                    | "{0} holds {n} files of a split {1} already. Replace that whole set? The old files go to the trash.",
                olds.len(),
                self.dir.display(),
                name
            );
            let buttons = vec![gezik_i18n::t!("Replace All").into_owned(), gezik_i18n::t!("Cancel").into_owned()];
            let title = gezik_i18n::t!("Parts are there already").into_owned();
            match cx.ask(Question::Confirm { title, message, buttons }) {
                Answer::Button(0) => {}
                _ if cx.stopped() => return Err(cancelled()),
                _ => {
                    cx.skip(
                        &self.source,
                        &io::Error::other(gezik_i18n::t!("Left as it was: its parts are there already")),
                    );
                    return Ok(Outcome::Nothing);
                }
            }
        }
        let staging = cx.staging_dir(&self.dir.join(&name))?;
        let mut from = File::open(&self.source)?;
        let mut whole = crc32fast::Hasher::new();
        let parts = part_count(item.facts.size, self.size);
        for n in 1..=parts {
            let mut to = File::create(staging.join(part_name(&name, n as u32)))?;
            pour(&mut from, &mut to, Some(self.size), &mut whole, cx)?;
        }
        std::fs::write(staging.join(crc_name(&name)), crc_text(&name, item.facts.size, whole.finalize()))?;
        // The new set is complete: the old one makes way (to the trash: undo brings it back).
        let mut outcomes = Vec::new();
        for old in &olds {
            match super::compress::out_of_the_way(old, old, cx) {
                Ok(outcome) => outcomes.push(outcome),
                Err(err) => {
                    let message = gezik_i18n::t!("{}; it could not make way, so nothing was placed", err);
                    cx.fail(old, &io::Error::new(err.kind(), message));
                    return Ok(Outcome::Several(outcomes));
                }
            }
        }
        *self.stage.lock().unwrap_or_else(|e| e.into_inner()) = Some(staging);
        Ok(Outcome::Several(outcomes))
    }
}

/// A part: its number, path and size.
type Part = (u32, PathBuf, u64);

/// The parts of `base` in `folder` (`base.001`… with three or more digits), sorted, with
/// their sizes, and its `.crc` (the ending in any case: `X.CRC` from Windows on Linux too).
fn set_files(folder: &Path, base: &str) -> io::Result<(Vec<Part>, Option<PathBuf>)> {
    let fold = |s: &str| if cfg!(any(windows, target_os = "macos")) { s.to_lowercase() } else { s.to_owned() };
    let prefix = fold(&format!("{base}."));
    let mut parts: Vec<Part> = Vec::new();
    let mut crc_file = None;
    for entry in std::fs::read_dir(folder)?.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(tail) = fold(&name).strip_prefix(&prefix).map(str::to_owned) else { continue };
        if tail.eq_ignore_ascii_case("crc") {
            crc_file = Some(entry.path());
        } else if tail.len() >= 3
            && tail.bytes().all(|b| b.is_ascii_digit())
            && let Ok(n) = tail.parse()
        {
            parts.push((n, entry.path(), meta.len()));
        }
    }
    parts.sort();
    Ok((parts, crc_file))
}

/// The parts found and what the `.crc` says.
struct Set {
    parts: Vec<PathBuf>,
    name: String,
    /// The `.crc` file and its (size, CRC32); `None`: no `.crc`.
    check: Option<(String, Option<u64>, Option<u32>)>,
}

struct JoinTask {
    part: PathBuf,
    dir: PathBuf,
    /// Where the joined file was written, once it is checked; the `PlaceTask` reads it.
    stage: Staged,
    set: OnceLock<Set>,
}

impl JoinTask {
    /// The set `self.part` belongs to, every part there: `Err` names the first one missing.
    fn find(&self) -> io::Result<Set> {
        let invalid = |message: String| io::Error::new(io::ErrorKind::InvalidInput, message);
        let base = split_base(&file_name(&self.part))
            .ok_or_else(|| invalid(gezik_i18n::t!("Not a part of a split file").into_owned()))?;
        let folder = self.part.parent().unwrap_or(Path::new("."));
        let (parts, crc_file) = set_files(folder, &base)?;
        let missing = |n: u32| invalid(gezik_i18n::t!("Part {} is missing", part_name(&base, n)));
        if let Some(gap) = first_gap(&parts.iter().map(|(n, ..)| *n).collect::<Vec<_>>()) {
            return Err(missing(gap));
        }
        if parts.is_empty() {
            return Err(missing(1));
        }
        let mut name = base.clone();
        let check = match crc_file {
            Some(path) => {
                let info = parse_crc(&String::from_utf8_lossy(&std::fs::read(&path)?));
                let total: u64 = parts.iter().map(|(.., len)| len).sum();
                if info.size.is_some_and(|size| total < size) {
                    return Err(missing(parts.len() as u32 + 1));
                }
                if let Some(plain) = info.filename.filter(|n| validate_name(n, NameRules::current()).is_ok()) {
                    name = plain;
                }
                Some((file_name(&path), info.size, info.crc))
            }
            None => None,
        };
        Ok(Set { parts: parts.into_iter().map(|(_, path, _)| path).collect(), name, check })
    }
}

impl Task for JoinTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        gezik_i18n::t!("Joining {}", join_name(&self.part))
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.part.clone(), self.dir.clone()], work: Work::Disk, vfs_roots: Vec::new() }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        match self.find() {
            Ok(set) => {
                let size = set.parts.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
                let item = PlanItem::new(Stage::Parallel, Facts { is_dir: false, size, modified: None })
                    .source(&set.parts[0])
                    .top(0);
                let _ = self.set.set(set);
                sink.item(item);
            }
            Err(err) => sink.failed(&self.part, err),
        }
    }

    fn run(&self, _item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(set) = self.set.get() else { return Ok(Outcome::Nothing) };
        let staging = cx.staging_dir(&self.dir.join(&set.name))?;
        let joined = staging.join(&set.name);
        let mut to = File::create(&joined)?;
        let mut crc = crc32fast::Hasher::new();
        let mut size = 0;
        for part in &set.parts {
            size += pour(&mut File::open(part)?, &mut to, None, &mut crc, cx)?;
        }
        to.sync_all()?;
        drop(to);
        let crc = crc.finalize();
        match &set.check {
            Some((crc_file, want_size, want_crc))
                if want_size.is_some_and(|want| want != size) || want_crc.is_none_or(|want| want != crc) =>
            {
                // Nothing is placed: the staging folder goes with the job.
                let _ = std::fs::remove_file(&joined);
                let message =
                    gezik_i18n::t!("The joined file does not match {} (CRC or size); a part may be damaged", crc_file);
                return Err(io::Error::new(io::ErrorKind::InvalidData, message));
            }
            Some(_) => {}
            None => cx.skip(&self.part, &io::Error::other(gezik_i18n::t!("No .crc file: the result was not checked"))),
        }
        *self.stage.lock().unwrap_or_else(|e| e.into_inner()) = Some(staging);
        Ok(Outcome::Nothing)
    }
}
