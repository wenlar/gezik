//! Links (spec 9.2): a shortcut, junction or symbolic link to each item. Undo trashes the
//! link itself (the trash takes the link, not what it leads to), redo brings it back.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_core::templates::{LinkKind, free_link_name, link_name};
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
        let mut planned = std::collections::HashSet::new();
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
            // A junction needs a folder on a local NTFS drive, and its link on one too: asked
            // per item, for both places.
            let refused = match self.kind {
                LinkKind::Junction if !meta.is_dir() => Some("A junction can only point to a folder"),
                LinkKind::Junction
                    if !(link::junctions_supported(source) && at.parent().is_some_and(link::junctions_supported)) =>
                {
                    Some("Junctions need a local NTFS drive")
                }
                _ => None,
            };
            if let Some(why) = refused {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, why));
                continue;
            }
            // Two sources of one name get their numbers here: nothing is on disk yet to meet.
            // A shortcut and an alias are files even when they lead to a folder.
            let named_dir = meta.is_dir() && !matches!(self.kind, LinkKind::Shortcut | LinkKind::Alias);
            let mut at = at.clone();
            // Numbered against the names planned in this job and against the disk.
            let taken = |path: &Path| planned.contains(path) || std::fs::symlink_metadata(path).is_ok();
            if taken(&at) {
                let parent = at.parent().map(Path::to_path_buf).unwrap_or_default();
                let name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let free = free_link_name(&name, self.kind, meta.is_dir(), |candidate| taken(&parent.join(candidate)));
                at = parent.join(free);
            }
            planned.insert(at.clone());
            // The link is not sized (as `walk` sees links); a folder link is numbered as a
            // folder (`v1.2 (2)`), and a taken name gets a number without asking.
            let facts = Facts { is_dir: named_dir, ..Facts::default() };
            let item = PlanItem::new(Stage::Parallel, facts)
                .source(source)
                .target(&at)
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

    #[test]
    fn same_named_sources_next_to_a_taken_name_get_two_numbers() {
        let dir = test_dir("link-same-name-taken");
        write(&dir.join("a/X/x.txt"), "x");
        write(&dir.join("b/X/y.txt"), "y");
        std::fs::create_dir_all(dir.join("links/Link to X")).unwrap();
        let engine = engine();
        let sources = vec![dir.join("a/X"), dir.join("b/X")];
        let job = engine.submit(Box::new(LinkTask::into(sources, &dir.join("links"), any_kind())));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("links/Link to X (2)/x.txt")), "x");
        assert_eq!(read(&dir.join("links/Link to X (3)/y.txt")), "y");
        for name in ["Link to X (2)", "Link to X (3)"] {
            gezik_platform::fs::delete(&dir.join("links").join(name)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_named_sources_and_dotted_folders_are_numbered() {
        let dir = test_dir("link-same-name");
        write(&dir.join("a/v1.2/x.txt"), "x");
        write(&dir.join("b/v1.2/y.txt"), "y");
        std::fs::create_dir(dir.join("links")).unwrap();
        let engine = engine();
        let sources = vec![dir.join("a/v1.2"), dir.join("b/v1.2")];
        let job = engine.submit(Box::new(LinkTask::into(sources, &dir.join("links"), any_kind())));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("links/Link to v1.2/x.txt")), "x");
        assert_eq!(read(&dir.join("links/Link to v1.2 (2)/y.txt")), "y");
        for name in ["Link to v1.2", "Link to v1.2 (2)"] {
            gezik_platform::fs::delete(&dir.join("links").join(name)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn shortcuts_to_folders_are_numbered_as_files() {
        let dir = test_dir("link-shortcut-number");
        write(&dir.join("Docs/a.txt"), "a");
        let engine = engine();
        for _ in 0..2 {
            let job = engine.submit(Box::new(LinkTask::beside(vec![dir.join("Docs")], LinkKind::Shortcut)));
            let (report, _) = finish(&engine, job, defaults);
            assert!(report.failures.is_empty(), "{:?}", report.failures);
        }
        assert!(dir.join("Docs - Shortcut.lnk").exists());
        assert!(dir.join("Docs - Shortcut (2).lnk").exists());
        // Two sources of one name in one job go through the planner's numbering.
        write(&dir.join("x/Docs/b.txt"), "b");
        let job = engine.submit(Box::new(LinkTask::into(
            vec![dir.join("Docs"), dir.join("x/Docs")],
            &dir.join("x"),
            LinkKind::Shortcut,
        )));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(dir.join("x/Docs - Shortcut (2).lnk").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
