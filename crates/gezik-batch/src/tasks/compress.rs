//! Making an archive of the chosen items: written under a temporary name next to the target
//! and renamed when complete (a 7z in parts: each part), so a half-written archive never
//! looks finished.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gezik_core::ops::names::{next_free, split_name};
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

use super::{Writing, file_name, what, workers};
use crate::archive::write::{self, CompressOptions, Level, OutFormat, volume};

pub struct CompressTask {
    sources: Vec<PathBuf>,
    target: PathBuf,
    options: CompressOptions,
    /// The target the plan checked for a conflict (a 7z in parts: the first part there).
    planned: Mutex<Option<PathBuf>>,
}

impl CompressTask {
    /// Compresses `sources` (named relative to their common folder) into `target`. `Store`
    /// makes a plain `.tar` of a `.tar.gz` or `.tar.xz` (and names it so).
    pub fn new(sources: Vec<PathBuf>, target: PathBuf, mut options: CompressOptions) -> CompressTask {
        let mut target = target;
        if options.level == Level::Store && matches!(options.format, OutFormat::TarGz | OutFormat::TarXz) {
            options.format = OutFormat::Tar;
            target = plain_tar(&target);
        }
        if options.format != OutFormat::SevenZ {
            options.split = None;
        }
        CompressTask { sources, target, options, planned: Mutex::new(None) }
    }

    /// The file whose conflict stands for the whole archive: the target; in parts, the
    /// lowest part of an older set of that name if there is one, else the first part.
    fn first(&self) -> PathBuf {
        if self.options.split.is_none() {
            return self.target.clone();
        }
        parts_of(&self.target).into_iter().next().map_or_else(|| volume(&self.target, 1), |(_, path)| path)
    }

    /// Writes the parts (at `temp.001`…) to their names on `base`. An older set there goes
    /// first (Replace was chosen for it); "Keep both" moves the whole set to a free name. All
    /// parts land or none: undo then brings back the older set.
    fn place_parts(&self, written: &[PathBuf], first: &Path, run: &RunCx<'_>) -> io::Result<Outcome> {
        let planned = self.planned.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let base = strip_part(first);
        let mut outcomes = Vec::new();
        let base = if let Some(planned) = planned.filter(|planned| planned != first) {
            // "Keep both" named it `x.7z (2).001`, which is no volume name: `x (2).7z.001`.
            free_base(&strip_part(&planned))
        } else {
            for (_, old) in parts_of(&base) {
                match out_of_the_way(&old, run) {
                    Ok(outcome) => outcomes.push(outcome),
                    Err(err) => return fail_parts(written, &[], &old, err, outcomes, run),
                }
            }
            base
        };
        let mut landed = Vec::new();
        for (n, part) in written.iter().enumerate() {
            let dest = volume(&base, n + 1);
            if let Err(err) = gezik_platform::fs::move_entry(part, &dest) {
                return fail_parts(&written[n..], &landed, &dest, err, outcomes, run);
            }
            landed.push(dest);
        }
        outcomes.extend(landed.into_iter().map(|path| Outcome::Created {
            facts: facts_after(&path, false),
            path,
            from: None,
        }));
        Ok(Outcome::Several(outcomes))
    }
}

/// Undoes a half-placed set: removes the parts written and those that landed. With older parts
/// already in the trash the item reports the failure and gives those (undo brings them back).
fn fail_parts(
    written: &[PathBuf],
    landed: &[PathBuf],
    at: &Path,
    err: io::Error,
    outcomes: Vec<Outcome>,
    run: &RunCx<'_>,
) -> io::Result<Outcome> {
    for path in written.iter().chain(landed) {
        let _ = std::fs::remove_file(path);
    }
    if outcomes.is_empty() {
        return Err(err);
    }
    run.fail(at, &err);
    Ok(Outcome::Several(outcomes))
}

/// Moves an older part out of the way: to the trash, or deleted on a drive without one (as
/// the engine's "Replace" does).
fn out_of_the_way(path: &Path, run: &RunCx<'_>) -> io::Result<Outcome> {
    if run.has_trash(path) {
        let trashed = gezik_ops::trash_path(path)?;
        return Ok(Outcome::Trashed { original: path.to_path_buf(), trashed });
    }
    gezik_platform::fs::delete(path)?;
    Ok(Outcome::Deleted { path: path.to_path_buf() })
}

/// The existing parts `base.001`, `base.002`… (any number of at least three digits), in order.
fn parts_of(base: &Path) -> Vec<(u32, PathBuf)> {
    let folder = base.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let prefix = format!("{}.", file_name(base));
    let fold = |s: &str| if cfg!(any(windows, target_os = "macos")) { s.to_lowercase() } else { s.to_owned() };
    let prefix = fold(&prefix);
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut parts: Vec<(u32, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = fold(&entry.file_name().to_string_lossy());
            let digits = name.strip_prefix(&prefix)?;
            let number =
                (digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.parse().ok())??;
            Some((number, base.parent().unwrap_or(Path::new("")).join(entry.file_name())))
        })
        .collect();
    parts.sort();
    parts
}

/// `x.7z.001` → `x.7z`.
fn strip_part(part: &Path) -> PathBuf {
    let name = file_name(part);
    match name.rsplit_once('.') {
        Some((base, digits)) if digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit()) => {
            part.with_file_name(base)
        }
        _ => part.to_path_buf(),
    }
}

/// `x (2).7z`, `x (3).7z`… for `base` `x.7z`: the first name no part has.
fn free_base(base: &Path) -> PathBuf {
    let free = next_free(&file_name(base), false, |candidate| !parts_of(&base.with_file_name(candidate)).is_empty());
    base.with_file_name(free)
}

/// The archive name for `sources` (each with whether it is a folder) with `format`'s
/// extension: one item gives its name (a file's without its extension, but whole before `.gz`
/// and `.xz`), several their folder's name, "Archive" for a drive. Touches no disk.
pub fn default_name(sources: &[(PathBuf, bool)], format: OutFormat) -> String {
    let stem = match sources {
        [(one, is_dir)] => match one.file_name() {
            Some(name) => {
                let name = name.to_string_lossy().into_owned();
                let whole = matches!(format, OutFormat::Gz | OutFormat::Xz) || *is_dir;
                let stem = if whole { name.as_str() } else { split_name(&name, false).0 };
                if stem.is_empty() { name.clone() } else { stem.to_owned() }
            }
            None => String::new(),
        },
        many => write::common_parent(&many.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let stem = if stem.is_empty() { "Archive" } else { stem.as_str() };
    format!("{stem}.{}", format.extension())
}

/// `a.tar.gz` → `a.tar`, `a.txz` → `a.tar`.
fn plain_tar(target: &Path) -> PathBuf {
    let name = file_name(target);
    let lower = name.to_ascii_lowercase();
    let plain = if lower.ends_with(".tar.gz") || lower.ends_with(".tar.xz") {
        name[..name.len() - 3].to_owned()
    } else if lower.ends_with(".tgz") || lower.ends_with(".txz") {
        format!("{}.tar", &name[..name.len() - 4])
    } else {
        name
    };
    target.with_file_name(plain)
}

impl Task for CompressTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Compress
    }

    fn title(&self) -> String {
        format!("Compressing {} to {}", what(&self.sources), file_name(&self.target))
    }

    fn count(&self) -> usize {
        self.sources.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.sources.clone();
        paths.push(self.target.clone());
        Resources { paths, work: Work::Cpu }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // An archive already there goes through the conflict list; the entries count once found.
        let first = self.first();
        *self.planned.lock().unwrap_or_else(|e| e.into_inner()) = Some(first.clone());
        let item = PlanItem::new(Stage::Parallel, Facts::default()).target(first).checked().top(0).uncounted();
        sink.item(item);
    }

    fn run(&self, item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        // Renamed by "Keep both" perhaps: `x (2).zip` (parts get a free name below).
        let first = item.target.clone().unwrap_or_else(|| self.first());
        let target = if self.options.split.is_some() { strip_part(&first) } else { first.clone() };
        let inputs = write::inputs(&self.sources, &mut |path, err| run.fail(path, &err));
        run.found(inputs.len() as u64, inputs.iter().map(write::Input::size).sum());
        let temp = run.temp_file_for(&target);
        let written = write::write(&inputs, &temp, &self.options, workers(), &Writing::new(run))?;
        if self.options.split.is_some() {
            return self.place_parts(&written, &first, run);
        }
        if let Err(err) = gezik_platform::fs::move_entry(&temp, &target) {
            let _ = std::fs::remove_file(&temp);
            return Err(err);
        }
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(&target, false), from: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_makes_a_plain_tar() {
        assert_eq!(plain_tar(Path::new("d/a.tar.gz")), Path::new("d/a.tar"));
        assert_eq!(plain_tar(Path::new("d/a.TXZ")), Path::new("d/a.tar"));
        assert_eq!(plain_tar(Path::new("d/a.bin")), Path::new("d/a.bin"));
        let options = CompressOptions {
            format: OutFormat::TarXz,
            level: Level::Store,
            password: None,
            encrypt_names: false,
            split: Some(9),
        };
        let task = CompressTask::new(vec!["d/x".into()], "d/x.tar.xz".into(), options);
        assert_eq!(task.target, Path::new("d/x.tar"));
        assert_eq!(task.options.format, OutFormat::Tar);
        assert_eq!(task.first(), Path::new("d/x.tar"), "only 7z is split");
        assert_eq!(strip_part(Path::new("d/x.7z (2).001")), Path::new("d/x.7z (2)"));
        assert_eq!(strip_part(Path::new("d/x.7z")), Path::new("d/x.7z"));
    }
}
