//! Making an archive of the chosen items: written under a temporary name next to the target
//! and renamed when complete (a 7z in parts: each part), so a half-written archive never
//! looks finished.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::names::split_name;
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

use super::{Writing, file_name, what, workers};
use crate::archive::write::{self, CompressOptions, Level, OutFormat, volume};

pub struct CompressTask {
    sources: Vec<PathBuf>,
    target: PathBuf,
    options: CompressOptions,
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
        CompressTask { sources, target, options }
    }

    /// The first file it makes: the target, or its first part.
    fn first(&self) -> PathBuf {
        if self.options.split.is_some() { volume(&self.target, 1) } else { self.target.clone() }
    }
}

/// The archive name for `sources` with `format`'s extension: one item gives its name (a
/// file's without its extension, but whole before `.gz` and `.xz`), several their folder's
/// name, "Archive" for a drive. Looks whether a single item is a folder.
pub fn default_name(sources: &[PathBuf], format: OutFormat) -> String {
    let stem = match sources {
        [one] => match one.file_name() {
            Some(name) => {
                let name = name.to_string_lossy().into_owned();
                let whole = matches!(format, OutFormat::Gz | OutFormat::Xz) || one.is_dir();
                let stem = if whole { name.as_str() } else { split_name(&name, false).0 };
                if stem.is_empty() { name.clone() } else { stem.to_owned() }
            }
            None => String::new(),
        },
        many => write::common_parent(many).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
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
        let item = PlanItem::new(Stage::Parallel, Facts::default()).target(self.first()).checked().top(0).uncounted();
        sink.item(item);
    }

    fn run(&self, item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        // Renamed by "Keep both" perhaps: `x (2).zip`, `x.7z (2).001`.
        let first = item.target.clone().unwrap_or_else(|| self.first());
        let target = match self.options.split {
            Some(_) => {
                let name = file_name(&first);
                first.with_file_name(name.strip_suffix(".001").unwrap_or(&name))
            }
            None => first,
        };
        let inputs = write::inputs(&self.sources, &mut |path, err| run.fail(path, &err));
        run.found(inputs.len() as u64, inputs.iter().map(write::Input::size).sum());
        let temp = run.temp_file_for(&target);
        let written = write::write(&inputs, &temp, &self.options, workers(), &Writing(run))?;
        if self.options.split.is_none() {
            if let Err(err) = gezik_platform::fs::move_entry(&temp, &target) {
                let _ = std::fs::remove_file(&temp);
                return Err(err);
            }
            return Ok(Outcome::Created { path: target.clone(), facts: facts_after(&target, false), from: None });
        }
        // Each part to its name; one that cannot go there fails alone, what landed stays undoable.
        let mut made = Vec::new();
        for (n, part) in written.iter().enumerate() {
            let dest = volume(&target, n + 1);
            match gezik_platform::fs::move_entry(part, &dest) {
                Ok(()) => made.push(Outcome::Created { facts: facts_after(&dest, false), path: dest, from: None }),
                Err(err) => {
                    let _ = std::fs::remove_file(part);
                    run.fail(&dest, &err);
                }
            }
        }
        Ok(Outcome::Several(made))
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
    }
}
