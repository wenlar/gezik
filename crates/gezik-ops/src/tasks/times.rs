//! Set Dates (16k): the chosen times of each item written from one source; a file without a
//! date taken is skipped. Undo writes back the times each item had, the whole job at once.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use gezik_core::ops::conflict::Facts;
use gezik_core::times::{Fields, Source, Times, resolve};

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

/// Reads a photo's date taken (EXIF lives in gezik-batch, which depends on this crate).
pub type TakenReader = Arc<dyn Fn(&Path) -> Option<SystemTime> + Send + Sync>;

enum Origin {
    Same(Source),
    Taken(TakenReader),
    /// Undo: each item's own old times.
    Each(Vec<Times>),
}

pub struct SetTimesTask {
    paths: Vec<PathBuf>,
    fields: Fields,
    from: Origin,
}

impl SetTimesTask {
    /// The chosen `fields` of each of `paths` from `source` (`Source::At` or `Source::From`).
    pub fn new(paths: Vec<PathBuf>, fields: Fields, source: Source) -> SetTimesTask {
        SetTimesTask { paths, fields, from: Origin::Same(source) }
    }

    /// The chosen `fields` of each of `paths` from its own date taken, read by `taken`.
    pub fn taken(paths: Vec<PathBuf>, fields: Fields, taken: TakenReader) -> SetTimesTask {
        SetTimesTask { paths, fields, from: Origin::Taken(taken) }
    }

    /// Undo: each path's times back to those of its `Times` that are there.
    pub fn restore(items: Vec<(PathBuf, Times)>) -> SetTimesTask {
        let (paths, times) = items.into_iter().unzip();
        let fields = Fields { modified: true, created: true, accessed: true };
        SetTimesTask { paths, fields, from: Origin::Each(times) }
    }
}

impl Task for SetTimesTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Times
    }

    fn title(&self) -> String {
        gezik_i18n::t!("Setting the dates of {}", what(&self.paths))
    }

    fn count(&self) -> usize {
        self.paths.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths.clone(), work: Work::Disk, vfs_roots: Vec::new() }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, path) in self.paths.iter().enumerate() {
            if !sink.item(PlanItem::new(Stage::Parallel, Facts::default()).source(path).top(root)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        let source = match &self.from {
            Origin::Same(source) => *source,
            Origin::Taken(read) => Source::Taken(read(path)),
            Origin::Each(times) => Source::From(times.get(item.root).copied().unwrap_or_default()),
        };
        let current = gezik_platform::fs::times(path)?;
        let can_create = gezik_platform::fs::CAN_SET_CREATED;
        let Some(new) = resolve(&source, self.fields, &current, can_create) else {
            cx.skip(path, &io::Error::other(gezik_i18n::t!("No date taken")));
            return Ok(Outcome::Nothing);
        };
        if new.is_empty() {
            return Ok(Outcome::Nothing);
        }
        gezik_platform::fs::set_times(path, &new)?;
        // Only what was written: undo puts those back and leaves the rest.
        let old = Times {
            modified: new.modified.and(current.modified),
            created: new.created.and(current.created),
            accessed: new.accessed.and(current.accessed),
        };
        Ok(Outcome::TimesChanged { path: path.clone(), old })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    const ALL: Fields = Fields { modified: true, created: true, accessed: true };

    #[test]
    fn dates_are_set_through_the_engine_and_one_undo_puts_them_back() {
        let dir = test_dir("times-undo");
        let (file, folder) = (dir.join("a.txt"), dir.join("sub"));
        write(&file, "x");
        std::fs::create_dir_all(&folder).unwrap();
        let before = [gezik_platform::fs::times(&file).unwrap(), gezik_platform::fs::times(&folder).unwrap()];
        let engine = engine();
        let task = SetTimesTask::new(vec![file.clone(), folder.clone()], ALL, Source::At(at(1_500_000_000)));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        for path in [&file, &folder] {
            let now = gezik_platform::fs::times(path).unwrap();
            assert_eq!(now.modified, Some(at(1_500_000_000)), "{path:?}");
            assert_eq!(now.accessed, Some(at(1_500_000_000)), "{path:?}");
            if gezik_platform::fs::CAN_SET_CREATED {
                assert_eq!(now.created, Some(at(1_500_000_000)), "{path:?}");
            }
        }
        assert_eq!(engine.undo_label().as_deref(), Some("Set dates of 2 items"));
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        for (path, before) in [&file, &folder].into_iter().zip(before) {
            let now = gezik_platform::fs::times(path).unwrap();
            assert_eq!((now.modified, now.created), (before.modified, before.created), "{path:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_without_a_date_taken_is_skipped() {
        let dir = test_dir("times-taken");
        let (a, b) = (dir.join("a.jpg"), dir.join("b.jpg"));
        write(&a, "x");
        write(&b, "x");
        let a_key = a.clone();
        let reader: TakenReader = Arc::new(move |path| (path == a_key).then(|| at(1_000_000_000)));
        let fields = Fields { modified: true, ..Fields::default() };
        let engine = engine();
        let task = SetTimesTask::taken(vec![a.clone(), b.clone()], fields, reader);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].path, b);
        assert_eq!(gezik_platform::fs::times(&a).unwrap().modified, Some(at(1_000_000_000)));
        assert_ne!(gezik_platform::fs::times(&b).unwrap().modified, Some(at(1_000_000_000)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
