//! Copying or moving only what a filter picks, under a name mask (spec 11 §12.2–12.3). The
//! walk reads each folder once (its files' names are all the related-file rule needs), never
//! enters a folder the filter leaves out, and plans a folder only once a file goes into it:
//! a filtered copy makes no empty folders.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gezik_core::file_filter::FileFilter;
use gezik_core::ops::conflict::Facts;
use gezik_core::ops::names::apply_mask;

use crate::task::ScanSink;
use crate::walk::facts_of;

/// What a filtered copy or move takes: the filter (none: everything) and a name mask for the
/// files (`*.bak`; folders keep their names).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pick {
    pub filter: Option<FileFilter>,
    pub mask: Option<String>,
}

impl Pick {
    /// Whether it changes nothing: then the task plans as it always did.
    pub fn is_empty(&self) -> bool {
        self.filter.as_ref().is_none_or(FileFilter::is_empty) && self.mask.is_none()
    }
}

/// One entry the walk picked; a folder comes before the first file going into it.
pub(crate) struct Picked<'a> {
    pub source: &'a Path,
    pub target: &'a Path,
    pub facts: Facts,
    /// One of the chosen items itself.
    pub top: bool,
}

/// Plans an entry; false once the job is cancelled.
pub(crate) type Emit<'e> = dyn FnMut(&mut dyn ScanSink, Picked<'_>) -> bool + 'e;

/// Names in a folder with their facts.
type Entries = Vec<(OsString, Facts)>;

/// Folders entered but not planned yet: (source, target, facts, top).
type Pending = Vec<(PathBuf, PathBuf, Facts, bool)>;

pub(crate) struct Picker<'p> {
    pick: &'p Pick,
    /// Chosen files and those going along with them: each is planned once.
    seen: std::collections::HashSet<PathBuf>,
}

impl<'p> Picker<'p> {
    pub fn new(pick: &'p Pick) -> Picker<'p> {
        Picker { pick, seen: Default::default() }
    }

    fn filter(&self) -> Option<&'p FileFilter> {
        self.pick.filter.as_ref()
    }

    fn masked(&self, name: &OsStr) -> OsString {
        match (&self.pick.mask, name.to_str()) {
            (Some(mask), Some(name)) => apply_mask(name, mask).into(),
            _ => name.to_os_string(),
        }
    }

    /// Plans chosen item `source` (going to `target`); false once the job is cancelled.
    pub fn root(
        &mut self,
        sink: &mut dyn ScanSink,
        source: &Path,
        target: &Path,
        facts: Facts,
        emit: &mut Emit<'_>,
    ) -> bool {
        let name = source.file_name().unwrap_or_default();
        if facts.is_dir {
            let inside = match self.filter() {
                None => true,
                Some(filter) => match filter.top_folder(&name.to_string_lossy()) {
                    None => return true,
                    Some(inside) => inside,
                },
            };
            return self.folder(sink, source, target, facts, true, inside, &mut Vec::new(), emit);
        }
        let text = name.to_string_lossy();
        if self.filter().is_some_and(|filter| !filter.takes_file(&text)) {
            return true;
        }
        let folder = target.parent().unwrap_or(Path::new(""));
        if self.seen.insert(source.to_path_buf()) {
            let target = folder.join(self.masked(name));
            if !emit(sink, Picked { source, target: &target, facts, top: true }) {
                return false;
            }
        }
        // The files going along with it, from its own folder.
        let Some(filter) = self.filter().filter(|filter| filter.has_related()) else { return true };
        let Some(dir) = source.parent() else { return true };
        let (files, _) = list(sink, dir);
        let names: Vec<String> = files.iter().map(|(name, _)| name.to_string_lossy().into_owned()).collect();
        let others: Vec<&str> = names.iter().map(String::as_str).filter(|other| *other != text).collect();
        for along in filter.related(&[text.as_ref()], &others) {
            let Some((name, facts)) = files.iter().find(|(name, _)| name.to_string_lossy() == along) else { continue };
            let path = dir.join(name);
            if self.seen.insert(path.clone()) {
                let target = folder.join(self.masked(name));
                if !emit(sink, Picked { source: &path, target: &target, facts: *facts, top: false }) {
                    return false;
                }
            }
        }
        true
    }

    /// Plans what folder `source` holds that the filter picks; `inside`: its files are taken.
    #[allow(clippy::too_many_arguments)]
    fn folder(
        &mut self,
        sink: &mut dyn ScanSink,
        source: &Path,
        target: &Path,
        facts: Facts,
        top: bool,
        inside: bool,
        pending: &mut Pending,
        emit: &mut Emit<'_>,
    ) -> bool {
        let mark = pending.len();
        pending.push((source.to_path_buf(), target.to_path_buf(), facts, top));
        // Without a filter every folder is copied, empty or not.
        if self.filter().is_none() && !flush(sink, pending, emit) {
            return false;
        }
        let (files, folders) = list(sink, source);
        if inside {
            let names: Vec<String> = files.iter().map(|(name, _)| name.to_string_lossy().into_owned()).collect();
            let mut take: Vec<bool> =
                names.iter().map(|name| self.filter().is_none_or(|filter| filter.takes_file(name))).collect();
            if let Some(filter) = self.filter().filter(|filter| filter.has_related()) {
                let side = |taken: bool| -> Vec<&str> {
                    names.iter().zip(&take).filter(|(_, t)| **t == taken).map(|(name, _)| name.as_str()).collect()
                };
                let along: Vec<String> =
                    filter.related(&side(true), &side(false)).into_iter().map(str::to_owned).collect();
                for (name, take) in names.iter().zip(&mut take) {
                    *take |= along.contains(name);
                }
            }
            for ((name, facts), take) in files.iter().zip(take) {
                if !take {
                    continue;
                }
                if !flush(sink, pending, emit) {
                    return false;
                }
                let (path, to) = (source.join(name), target.join(self.masked(name)));
                if !emit(sink, Picked { source: &path, target: &to, facts: *facts, top: false }) {
                    return false;
                }
            }
        }
        for (name, facts) in folders {
            let next = match self.filter() {
                None => Some(true),
                Some(filter) => filter.enters(&name.to_string_lossy(), inside),
            };
            let Some(inside) = next else { continue };
            if !self.folder(sink, &source.join(&name), &target.join(&name), facts, false, inside, pending, emit) {
                return false;
            }
        }
        // Nothing went into it: it is not made.
        pending.truncate(mark);
        true
    }
}

/// Plans the folders entered so far that are not planned yet, shallowest first.
fn flush(sink: &mut dyn ScanSink, pending: &mut Pending, emit: &mut Emit<'_>) -> bool {
    pending
        .drain(..)
        .all(|(source, target, facts, top)| emit(sink, Picked { source: &source, target: &target, facts, top }))
}

/// The files and the folders in `dir` (a link is a file: it is not followed); what cannot be
/// read is reported.
fn list(sink: &mut dyn ScanSink, dir: &Path) -> (Entries, Entries) {
    let (mut files, mut folders) = (Vec::new(), Vec::new());
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            sink.failed(dir, err);
            return (files, folders);
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                sink.failed(dir, err);
                continue;
            }
        };
        // DirEntry::metadata does not follow links.
        match entry.metadata() {
            Ok(meta) => {
                let facts = facts_of(&meta);
                if facts.is_dir { &mut folders } else { &mut files }.push((entry.file_name(), facts));
            }
            Err(err) => sink.failed(&entry.path(), err),
        }
    }
    (files, folders)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ConflictItem;
    use crate::tasks::CopyTask;
    use crate::testing::{CollectSink, engine, finish, read, test_dir, write};
    use crate::{Decision, Task};

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    fn pick(filter: &str, mask: Option<&str>) -> Pick {
        Pick { filter: Some(FileFilter::parse(filter, true).unwrap()), mask: mask.map(str::to_owned) }
    }

    /// Every file under `dir`, relative, with `/`.
    fn files(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        crate::walk::walk(dir, &mut |step| {
            if let crate::walk::Step::Entry { relative, facts, .. } = step {
                out.push(
                    format!("{}{}", relative.display(), if facts.is_dir { "/" } else { "" })
                        .replace(std::path::MAIN_SEPARATOR, "/"),
                );
            }
            true
        });
        out.sort();
        out
    }

    #[test]
    fn a_filtered_copy_takes_only_what_matches_and_makes_no_empty_folders() {
        let dir = test_dir("pick-copy");
        let src = dir.join("src");
        for file in ["a/x.jpg", "a/y.txt", "a/thumbs/t.jpg", "a/only/z.txt", "a/deep/d/e.JPG"] {
            write(&src.join(file), file);
        }
        std::fs::create_dir_all(src.join("a/empty")).unwrap();
        let dst = dir.join("dst");
        let task = CopyTask::with_folders(vec![(src.join("a"), PathBuf::from("a"))], &dst)
            .picking(pick("*.jpg | thumbs\\", None));
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(files(&dst), ["a/", "a/deep/", "a/deep/d/", "a/deep/d/e.JPG", "a/x.jpg"]);
        assert_eq!(read(&dst.join("a/x.jpg")), "a/x.jpg");
        let (undo, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert!(!dst.exists(), "one undo takes it all");
        assert_eq!(files(&src).len(), 11, "the sources stay");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_left_out_folder_is_not_entered() {
        let dir = test_dir("pick-skip");
        write(&dir.join("a/keep.txt"), "k");
        write(&dir.join("a/.git/objects/x"), "x");
        let skipped = dir.join("a/.git");
        // Unreadable there: a walk into it would report it as failed.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&skipped, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let task = CopyTask::with_folders(vec![(dir.join("a"), PathBuf::from("a"))], &dir.join("dst"))
            .picking(pick("| .git\\", None));
        let mut sink = CollectSink::default();
        task.plan(&mut sink);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&skipped, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(sink.failed.is_empty(), "{:?}", sink.failed);
        let sources: Vec<PathBuf> = sink.items.iter().filter_map(|item| item.source.clone()).collect();
        assert!(sources.iter().all(|s| !s.starts_with(&skipped)), "{sources:?}");
        assert!(sources.contains(&dir.join("a/keep.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn related_files_come_along_once() {
        let dir = test_dir("pick-related");
        let src = dir.join("src");
        for file in ["IMG_1.jpg", "IMG_1.cr2", "IMG_1.xmp", "IMG_2.cr2", "IMG_3.png", "sub/IMG_4.jpg", "sub/IMG_4.cr2"]
        {
            write(&src.join(file), file);
        }
        let dst = dir.join("dst");
        // The .cr2 chosen too, before its .jpg: still once.
        let items = ["IMG_1.cr2", "IMG_1.jpg", "IMG_3.png", "sub"].map(|n| (src.join(n), PathBuf::from(n))).to_vec();
        let task = CopyTask::with_folders(items, &dst).picking(pick("*.jpg>*.cr2 *.xmp", None));
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(files(&dst), ["IMG_1.cr2", "IMG_1.jpg", "IMG_1.xmp", "sub/", "sub/IMG_4.cr2", "sub/IMG_4.jpg"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mask_names_the_copies_and_two_on_one_name_meet_the_conflict_list() {
        let dir = test_dir("pick-mask");
        let src = dir.join("src");
        for file in ["a.txt", "b.txt", "a.log", "f/c.txt"] {
            write(&src.join(file), file);
        }
        let dst = dir.join("dst");
        let items = ["a.txt", "b.txt", "f"].map(|n| (src.join(n), PathBuf::from(n))).to_vec();
        let task = CopyTask::with_folders(items, &dst).picking(Pick { filter: None, mask: Some("*.bak".into()) });
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(files(&dst), ["a.bak", "b.bak", "f/", "f/c.bak"], "folders keep their names");
        // a.txt and a.log both become a.bak: the second is asked about, not written over.
        let dst2 = dir.join("dst2");
        let items = ["a.txt", "a.log"].map(|n| (src.join(n), PathBuf::from(n))).to_vec();
        let task = CopyTask::with_folders(items, &dst2).picking(Pick { filter: None, mask: Some("*.bak".into()) });
        let asked = std::cell::Cell::new(0);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), |c| {
            asked.set(c.len());
            vec![Decision::Skip; c.len()]
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(asked.get(), 1);
        assert_eq!(files(&dst2), ["a.bak"]);
        assert_eq!(
            read(&dst2.join("a.bak")),
            "a.log",
            "the first planned (in path order) stays, the other was skipped"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_name_from_two_folders_meets_the_conflict_list() {
        let dir = test_dir("pick-same-name");
        write(&dir.join("a/x.txt"), "a");
        write(&dir.join("b/x.txt"), "b");
        let dst = dir.join("dst");
        std::fs::create_dir(&dst).unwrap();
        let engine = engine();
        let asked = std::cell::Cell::new(0);
        let task = CopyTask::into(vec![dir.join("a/x.txt"), dir.join("b/x.txt")], &dst);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), |c| {
            asked.set(c.len());
            vec![Decision::Skip; c.len()]
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(asked.get(), 1, "the second x.txt is asked about, not written over the first");
        assert_eq!(read(&dst.join("x.txt")), "a");
        let task = crate::tasks::MoveTask::into(vec![dir.join("a/x.txt"), dir.join("b/x.txt")], &dir.join("dst2"));
        assert!(task.same_targets());
        assert!(!CopyTask::into(vec![dir.join("a/x.txt")], &dst).same_targets());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_pick_changes_nothing() {
        assert!(Pick::default().is_empty());
        assert!(Pick { filter: Some(FileFilter::parse(" | ", true).unwrap()), mask: None }.is_empty());
        assert!(!Pick { filter: None, mask: Some("*.bak".into()) }.is_empty());
    }
}
