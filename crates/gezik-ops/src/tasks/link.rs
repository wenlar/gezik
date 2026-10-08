//! Links (spec 9.2): a shortcut, junction or symbolic link to each item. Undo trashes the
//! link itself (the trash takes the link, not what it leads to), redo brings it back.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_core::templates::{LinkKind, link_name};
use gezik_platform::link;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

/// The item's target is a folder.
const DIR: u8 = 1;

pub struct LinkTask {
    /// (what it leads to, where the link goes) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    kind: LinkKind,
}

/// Where a link to `source` goes in `dir`.
fn link_in(source: &Path, dir: &Path, kind: LinkKind) -> PathBuf {
    dir.join(link_name(&super::name(source), kind))
}

impl LinkTask {
    /// Links to `sources` in `dir` (a drop with the link keys, "Create link here").
    pub fn into(sources: Vec<PathBuf>, dir: &Path, kind: LinkKind) -> LinkTask {
        let pairs = sources
            .into_iter()
            .map(|source| {
                let at = link_in(&source, dir, kind);
                (source, at)
            })
            .collect();
        LinkTask { pairs, kind }
    }

    /// A link next to each of `sources` ("Create link ▸", Explorer's "Create shortcut").
    pub fn beside(sources: Vec<PathBuf>, kind: LinkKind) -> LinkTask {
        let pairs = sources
            .into_iter()
            .map(|source| {
                let dir = source.parent().map(Path::to_path_buf).unwrap_or_default();
                let at = link_in(&source, &dir, kind);
                (source, at)
            })
            .collect();
        LinkTask { pairs, kind }
    }

    fn sources(&self) -> Vec<PathBuf> {
        self.pairs.iter().map(|(source, _)| source.clone()).collect()
    }
}

impl Task for LinkTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Link
    }

    fn title(&self) -> String {
        match self.pairs.as_slice() {
            [_] => format!("Creating a link to {}", what(&self.sources())),
            _ => format!("Creating links to {}", what(&self.sources())),
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.sources();
        paths.extend(self.pairs.iter().filter_map(|(_, at)| at.parent().map(Path::to_path_buf)));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // Junctions need a local NTFS drive: asked once, for where the first link goes.
        let junctions = self.kind != LinkKind::Junction
            || self.pairs.first().and_then(|(_, at)| at.parent()).is_some_and(link::junctions_supported);
        for (root, (source, at)) in self.pairs.iter().enumerate() {
            if super::refuse_root(sink, source, "link to") {
                continue;
            }
            // What it leads to decides a folder link (a link to a link to a folder too).
            let meta = match std::fs::metadata(source).or_else(|_| std::fs::symlink_metadata(source)) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let refused = match self.kind {
                LinkKind::Junction if !meta.is_dir() => Some("A junction can only point to a folder"),
                LinkKind::Junction if !junctions => Some("Junctions need a local NTFS drive"),
                _ => None,
            };
            if let Some(why) = refused {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, why));
                continue;
            }
            // The link itself is neither a folder nor sized (as `walk` sees links); a taken
            // name gets a number without asking.
            let item = PlanItem::new(Stage::Parallel, Facts::default())
                .source(source)
                .target(at)
                .checked()
                .top(root)
                .preset(Some(Decision::KeepBoth))
                .tag(if meta.is_dir() { DIR } else { 0 });
            if !sink.item(item) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        link::create(self.kind, source, target, item.tag == DIR)?;
        // `symlink_metadata`: the link's own facts, not its target's.
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, false), from: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};

    /// Makes a link of `kind` to a folder with a file in it and undoes it (the link goes to the
    /// trash): the folder keeps its file. Redo brings the link back from the trash.
    fn trash_keeps_the_target(kind: LinkKind, name: &str) {
        let dir = test_dir(name);
        write(&dir.join("target/inside.txt"), "kept");
        std::fs::create_dir(dir.join("links")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(LinkTask::into(vec![dir.join("target")], &dir.join("links"), kind)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{kind:?}: {:?}", report.failures);
        let link = report.results[0].clone();
        assert_eq!(engine.undo_label().as_deref(), Some("Create link"));
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty() && undo.no_trash.is_empty(), "{kind:?}: {:?}", undo.failures);
        assert!(std::fs::symlink_metadata(&link).is_err(), "{kind:?}: the link went to the trash");
        assert_eq!(read(&dir.join("target/inside.txt")), "kept", "{kind:?}: the folder's contents stay");
        let (redo, _) = finish(&engine, engine.redo().unwrap(), defaults);
        assert!(redo.failures.is_empty(), "{kind:?}: {:?}", redo.failures);
        match kind {
            #[cfg(windows)]
            LinkKind::Shortcut => {
                assert_eq!(gezik_platform::link::read_shortcut(&link).unwrap(), dir.join("target"))
            }
            _ => assert_eq!(read(&link.join("inside.txt")), "kept", "{kind:?}: back and leading there"),
        }
        // Removed as a link, never through it.
        gezik_platform::fs::delete(&link).unwrap();
        assert_eq!(read(&dir.join("target/inside.txt")), "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn trashing_a_junction_leaves_its_folder() {
        trash_keeps_the_target(LinkKind::Junction, "link-junction");
    }

    #[cfg(windows)]
    #[test]
    fn trashing_a_shortcut_leaves_its_folder() {
        trash_keeps_the_target(LinkKind::Shortcut, "link-shortcut");
    }

    #[test]
    fn trashing_a_symbolic_link_leaves_its_folder() {
        gezik_platform::link::probe_symlinks();
        if !gezik_platform::link::symlinks_allowed() {
            eprintln!("symbolic links need Developer Mode here: skipped");
            return;
        }
        trash_keeps_the_target(LinkKind::Symlink, "link-symlink");
    }

    /// A kind every test machine can make: a junction on Windows, a symbolic link elsewhere.
    fn any_kind() -> LinkKind {
        if cfg!(windows) { LinkKind::Junction } else { LinkKind::Symlink }
    }

    #[test]
    fn links_beside_their_items_are_named_and_numbered() {
        let dir = test_dir("link-beside");
        write(&dir.join("Docs/a.txt"), "a");
        let engine = engine();
        for _ in 0..2 {
            let job = engine.submit(Box::new(LinkTask::beside(vec![dir.join("Docs")], any_kind())));
            let (report, _) = finish(&engine, job, defaults);
            assert!(report.failures.is_empty(), "{:?}", report.failures);
        }
        assert_eq!(read(&dir.join("Link to Docs/a.txt")), "a");
        assert_eq!(read(&dir.join("Link to Docs (2)/a.txt")), "a");
        for name in ["Link to Docs", "Link to Docs (2)"] {
            gezik_platform::fs::delete(&dir.join(name)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_to_a_file_fails_with_a_reason() {
        let dir = test_dir("link-junction-file");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let job = engine.submit(Box::new(LinkTask::beside(vec![dir.join("a.txt")], LinkKind::Junction)));
        let (report, _) = finish(&engine, job, defaults);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.contains("only point to a folder"), "{}", report.failures[0].message);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
