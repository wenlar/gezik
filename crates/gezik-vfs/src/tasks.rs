//! Jobs on places (spec 16 §4.6). Copying out to a folder on disk: one job of two steps, the
//! chosen entries taken out into a staging folder in the target folder, then placed there as
//! any extraction is (conflicts asked, one undo); a move then deletes in the place what was
//! placed. Changing a place (`VfsEditTask`): an archive is written anew once for all its edits
//! (an archive in an archive, then the one it is in), its old self going to the trash.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use gezik_batch::tasks::{
    check_space, confirm_no_trash, plan_archive, put_sources, read_only, real_archive, replace, rewrite,
};
use gezik_core::vfs::{Caps, Container, Edit, VfsPath, VfsRoot};
use gezik_ops::{Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

use crate::{Cx, Fresh, under};

/// The folder inside the staging folder the entries are written to.
const CONTENT: &str = "x";

/// The tasks that copy `items` (all in one folder of one place) into `dest`, and what Undo
/// says. Submit them as one chain.
pub fn copy_out(items: &[VfsPath], dest: PathBuf) -> (Vec<Box<dyn Task>>, String) {
    let (tasks, label, _, _) = take_out(items, dest, false);
    (tasks, label)
}

/// Like `copy_out`, then the entries placed in `dest` are deleted in the place (Cut, Move To…,
/// a moving drag): what was skipped or failed stays there.
pub fn move_out(items: &[VfsPath], dest: PathBuf) -> (Vec<Box<dyn Task>>, String) {
    let (mut tasks, label, stage, taken) = take_out(items, dest, true);
    if let Some(first) = items.first() {
        let edits = items.iter().map(|item| Edit::Delete(item.inner.clone())).collect();
        let mut delete = VfsEditTask::new(first.root.clone(), edits);
        delete.gate = Some((stage, taken));
        tasks.push(Box::new(delete));
    }
    (tasks, label)
}

fn take_out(items: &[VfsPath], dest: PathBuf, moving: bool) -> (Vec<Box<dyn Task>>, String, Stage_, Taken) {
    let (title, label) = match (items, moving) {
        ([one], false) => (gezik_i18n::t!("Copying {}", one.name()), gezik_i18n::t!("Copy {}", one.name())),
        (many, false) => (
            gezik_i18n::tn!("Copying {n} item" | "Copying {n} items", many.len()),
            gezik_i18n::tn!("Copy {n} item" | "Copy {n} items", many.len()),
        ),
        (many, true) => {
            (gezik_i18n::t!("Moving {0} to {1}", what(items), dest.display()), TaskKind::Move.label(many.len()))
        }
    };
    let stage = Arc::new(Mutex::new(None));
    let taken = Arc::new(Mutex::new(Vec::new()));
    let root = items.first().map(|item| item.root.clone());
    let take = root.map(|root| TakeOutTask {
        root,
        items: items.iter().map(|item| item.inner.clone()).collect(),
        dest: dest.clone(),
        stage: stage.clone(),
        taken: taken.clone(),
        title: title.clone(),
        kind: if moving { TaskKind::Move } else { TaskKind::Copy },
    });
    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
    if let Some(take) = take {
        tasks.push(Box::new(take));
        tasks.push(gezik_batch::tasks::placing(dest, stage.clone(), TaskKind::Copy, title));
    }
    (tasks, label, stage, taken)
}

/// The entries taken out whole and waiting in the staging folder, each under its last name.
type Taken = Arc<Mutex<Vec<String>>>;

/// Where taken-out entries wait to be placed (set once they are all there).
type Stage_ = Arc<Mutex<Option<PathBuf>>>;

/// Takes the entries out into a staging folder in the target folder (so placing them is a
/// rename on one drive).
struct TakeOutTask {
    root: VfsRoot,
    items: Vec<String>,
    dest: PathBuf,
    /// Set to the staged folder holding them; the placing step reads it.
    stage: Stage_,
    /// What went into it (a move deletes only those of them that left it).
    taken: Taken,
    title: String,
    kind: TaskKind,
}

impl Task for TakeOutTask {
    fn kind(&self) -> TaskKind {
        self.kind
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        // The archive's file stands for the place: jobs on it and on the target's drive queue.
        let mut paths = vec![self.dest.clone()];
        paths.extend(disk_of(&self.root).map(Path::to_path_buf));
        Resources { paths, work: Work::Disk, vfs_roots: crate::queue_key(&self.root).into_iter().collect() }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let facts = Facts { is_dir: false, size: 0, modified: None };
        sink.item(PlanItem::new(Stage::Parallel, facts).source(&self.dest).top(0).uncounted());
    }

    fn run(&self, _item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        let cx = Engine::new(run, &self.root);
        let vfs = crate::mount(&self.root, &cx)?;
        // Copy To… may name a folder that is not there yet. shortcut: undo leaves it.
        std::fs::create_dir_all(&self.dest)?;
        let staging = run.staging_dir(&self.dest.join(CONTENT))?;
        let content = staging.join(CONTENT);
        std::fs::create_dir(&content)?;
        // Every entry whole, or none: a folder missing a child (a link left out, a damaged
        // entry) must not count as taken out, or a move would delete what did not come out.
        crate::stage_into(vfs.as_ref(), &self.items, &content, &cx)?;
        // Side by side for the placing step, whatever folders they were in (a search's results);
        // the second of two with one name fails.
        let held = staging.join("y");
        std::fs::create_dir(&held)?;
        for item in &self.items {
            let from = under(&content, item);
            let to = held.join(last(item));
            if to.exists() {
                run.fail(&from, &io::Error::from(io::ErrorKind::AlreadyExists));
                continue;
            }
            std::fs::rename(&from, &to)?;
            self.taken.lock().unwrap_or_else(PoisonError::into_inner).push(item.clone());
        }
        // Leaving Gezik: they carry the archive's Mark of the Web.
        crate::motw::mark_like(disk_of(&self.root), &held);
        *self.stage.lock().unwrap_or_else(PoisonError::into_inner) = Some(held);
        // What lands is reported by the placing step.
        Ok(Outcome::Nothing)
    }
}

/// Changes a place (spec 16 §6): files put in, entries deleted, renamed, folders made, as one
/// rewrite of the archive (and of each archive it is in). Undo brings the old archive back.
pub struct VfsEditTask {
    root: VfsRoot,
    edits: Vec<Edit>,
    /// Entries of a place (this one or another) taken out first and put into folder `into`.
    take: Option<(VfsRoot, Vec<String>)>,
    into: String,
    /// What was put in goes from where it came: files on disk to the trash, a place's entries
    /// deleted there.
    moving: bool,
    /// A put replaces the entry of its name without asking ("Update in Archive").
    replace: bool,
    /// Deletes only the entries taken out and no longer in this folder (a move out: those placed).
    gate: Option<(Stage_, Taken)>,
    kind: TaskKind,
    title: String,
    count: usize,
}

impl VfsEditTask {
    /// `edits` in `root`: deletes, renames or new folders (puts come with `put`).
    pub fn new(root: VfsRoot, edits: Vec<Edit>) -> VfsEditTask {
        let place = root.name();
        let names: Vec<String> = edits
            .iter()
            .map(|edit| match edit {
                Edit::Put { to: name, .. }
                | Edit::MakeDir(name)
                | Edit::Delete(name)
                | Edit::Unmake { path: name, .. }
                | Edit::Rename { from: name, .. } => last(name).to_owned(),
            })
            .collect();
        let what = match names.as_slice() {
            [one] => one.clone(),
            many => gezik_i18n::tn!("{n} item" | "{n} items", many.len()),
        };
        let (kind, title) = match edits.first() {
            Some(Edit::Rename { to, .. }) if edits.len() == 1 => {
                (TaskKind::Rename, gezik_i18n::t!("Renaming {0} to {1}", what, last(to)))
            }
            Some(Edit::Rename { .. }) => (TaskKind::Rename, gezik_i18n::t!("Renaming {}", what)),
            Some(Edit::MakeDir(_)) => (TaskKind::NewFolder, gezik_i18n::t!("Creating {}", what)),
            // TRANSLATORS: {0} is a name or "3 items", {1} an archive or a folder in one
            _ => (TaskKind::Trash, gezik_i18n::t!("Deleting {0} from {1}", what, place)),
        };
        let count = edits.len();
        VfsEditTask {
            root,
            edits,
            take: None,
            into: String::new(),
            moving: false,
            replace: false,
            gate: None,
            kind,
            title,
            count,
        }
    }

    /// Files and folders on disk put into folder `dir`; `moving`: trashed once in.
    pub fn put(dir: &VfsPath, sources: Vec<PathBuf>, moving: bool) -> VfsEditTask {
        let edits: Vec<Edit> = sources
            .iter()
            .filter_map(|from| {
                let name = from.file_name()?.to_string_lossy().into_owned();
                Some(Edit::Put { from: from.clone(), to: dir.join(&name).inner })
            })
            .collect();
        let names: Vec<String> = edits.iter().map(|edit| last(edit_name(edit)).to_owned()).collect();
        let mut task = VfsEditTask::new(dir.root.clone(), edits);
        task.describe_put(&names, dir, moving);
        task.moving = moving;
        task
    }

    /// Entries of a place copied (`moving`: moved) into folder `dir`; within one archive a
    /// move is a rename. A move leaves out what is `dir` itself or holds it, and what is in
    /// `dir` already (`movable`).
    pub fn copy_in(items: &[VfsPath], dir: &VfsPath, moving: bool) -> VfsEditTask {
        let kept;
        let items = if moving {
            kept = movable(items, dir);
            kept.as_slice()
        } else {
            items
        };
        let names: Vec<String> = items.iter().map(|item| last(&item.inner).to_owned()).collect();
        let from = items.first().map(|item| item.root.clone());
        let mut task = match from {
            // On a phone a move between folders is a copy and a delete (MTP has no cheap move).
            Some(from) if moving && crate::same_root(&from, &dir.root) && crate::queue_key(&from).is_none() => {
                let edits = items
                    .iter()
                    .map(|item| Edit::Rename { from: item.inner.clone(), to: dir.join(last(&item.inner)).inner })
                    .filter(|edit| !matches!(edit, Edit::Rename { from, to } if from == to))
                    .collect();
                VfsEditTask::new(dir.root.clone(), edits)
            }
            from => {
                let mut task = VfsEditTask::new(dir.root.clone(), Vec::new());
                task.take = from.map(|from| (from, items.iter().map(|item| item.inner.clone()).collect()));
                task.into = dir.inner.clone();
                task.moving = moving;
                task
            }
        };
        task.describe_put(&names, dir, moving);
        task
    }

    /// `file` (taken out, opened with its app and changed) back into the archive as `at`.
    pub fn update(at: &VfsPath, file: PathBuf) -> VfsEditTask {
        let mut task = VfsEditTask::new(at.root.clone(), vec![Edit::Put { from: file, to: at.inner.clone() }]);
        task.replace = true;
        task.kind = TaskKind::AddToArchive;
        // TRANSLATORS: {} is a file's name; it is put back into the archive it was opened from
        task.title = gezik_i18n::t!("Updating {} in the archive", at.name());
        task
    }

    fn describe_put(&mut self, names: &[String], dir: &VfsPath, moving: bool) {
        let what = match names {
            [one] => one.clone(),
            many => gezik_i18n::tn!("{n} item" | "{n} items", many.len()),
        };
        (self.kind, self.title) = if moving {
            (TaskKind::Move, gezik_i18n::t!("Moving {0} to {1}", what, dir.name()))
        } else {
            (TaskKind::Copy, gezik_i18n::t!("Copying {0} to {1}", what, dir.name()))
        };
        self.count = names.len();
    }

    /// Writes `edits` into the archive `root` and each archive it is in: the outcome (the old
    /// archive on disk trashed, the new one made) and what went in. `None`: nothing changed.
    fn write_at(
        &self,
        root: &VfsRoot,
        edits: Vec<Edit>,
        trash: bool,
        run: &RunCx<'_>,
    ) -> io::Result<Option<(Outcome, gezik_batch::tasks::Rewritten)>> {
        let cx = Engine::new(run, root);
        let vfs = crate::mount(root, &cx)?;
        if !vfs.caps().has(Caps::WRITE) {
            return Err(read_only());
        }
        if vfs.device() {
            // A phone: each edit on its own, after one question for names taken.
            let Some((edits, replace)) = settle_on_device(vfs.as_ref(), edits, self.replace, run)? else {
                return Ok(None);
            };
            let applied = vfs.apply(&edits, replace, &cx)?;
            // Stopped early: what was done stays undoable; a phone gone is a failure row.
            if let Some(err) = &applied.ended
                && err.kind() != io::ErrorKind::Interrupted
            {
                run.fail(Path::new(&root.text()), err);
            }
            let done = gezik_batch::tasks::Rewritten { sources: applied.sources, seen: (0, None) };
            return Ok(Some((Outcome::Several(applied.outcomes), done)));
        }
        // Held while writing: an archive in an archive is a file in that one's folder, which
        // goes when it is let go of.
        let (file, password) = vfs.archive().ok_or_else(read_only)?;
        match root {
            VfsRoot::Archive(Container::Disk(_)) => {
                let file = real_archive(&file)?;
                check_space(&file, &edits)?;
                let temp = run.temp_file_for(&file);
                let Some(done) = rewrite(&file, &edits, password, &temp, run, None)? else { return Ok(None) };
                Ok(Some((replace(&file, &temp, trash, done.seen, run, None)?, done)))
            }
            VfsRoot::Archive(Container::Inside(at)) => {
                // Written in a private folder, then put back into the one it is in in its place.
                let folder = Fresh(crate::stage::new_folder()?);
                let name = file.file_name().map_or_else(|| "archive".into(), |name| name.to_os_string());
                let out = folder.0.join(name);
                let Some(done) = rewrite(&file, &edits, password, &out, run, None)? else { return Ok(None) };
                let outer = vec![Edit::Delete(at.inner.clone()), Edit::Put { from: out, to: at.inner.clone() }];
                Ok(self.write_at(&at.root, outer, trash, run)?.map(|(outcome, _)| (outcome, done)))
            }
            VfsRoot::Device { .. } => Err(io::ErrorKind::Unsupported.into()),
        }
    }
}

impl Task for VfsEditTask {
    fn kind(&self) -> TaskKind {
        self.kind
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn count(&self) -> usize {
        self.count
    }

    fn resources(&self) -> Resources {
        // The archive's file stands for the place: two jobs on one archive queue.
        let mut paths: Vec<PathBuf> = disk_of(&self.root).map(Path::to_path_buf).into_iter().collect();
        paths.extend(put_sources(&self.edits));
        paths.extend(self.take.as_ref().and_then(|(from, _)| disk_of(from)).map(Path::to_path_buf));
        // A phone stands for itself: two jobs on one phone queue (one session, one worker).
        let vfs_roots: Vec<String> = crate::queue_key(&self.root)
            .into_iter()
            .chain(self.take.as_ref().and_then(|(from, _)| crate::queue_key(from)))
            .collect();
        let work = if vfs_roots.is_empty() { Work::Cpu } else { Work::Disk };
        Resources { paths, work, vfs_roots }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        match disk_of(&self.root) {
            Some(disk) => plan_archive(disk, sink),
            // A phone: one item, the edits done one by one in it.
            None => {
                let facts = Facts { is_dir: false, size: 0, modified: None };
                sink.item(PlanItem::new(Stage::Parallel, facts).source(self.root.text()).top(0).uncounted());
            }
        }
    }

    fn run(&self, _item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        let mut edits = self.edits.clone();
        if let Some((stage, taken)) = &self.gate {
            // Moved out: only what was taken out whole and placed (it left the staging folder)
            // is deleted here; a second entry of one name was never taken.
            let held = stage.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let Some(held) = held else { return Ok(Outcome::Nothing) };
            let taken = taken.lock().unwrap_or_else(PoisonError::into_inner).clone();
            edits.retain(|edit| match edit {
                Edit::Delete(name) => taken.contains(name) && held.join(last(name)).symlink_metadata().is_err(),
                _ => true,
            });
            if edits.is_empty() {
                return Ok(Outcome::Nothing);
            }
        }
        // A phone has no trash, nor needs one: no old archive is kept there.
        let trash = disk_of(&self.root).is_none_or(|disk| run.has_trash(disk));
        let source_trash =
            self.take.as_ref().and_then(|(from, _)| disk_of(from)).is_none_or(|from| run.has_trash(from));
        if (!trash || (self.moving && !source_trash)) && !confirm_no_trash(run)? {
            return Ok(Outcome::Nothing);
        }
        // Entries of a place: taken out into a private folder first, each under its own name.
        let taken = match &self.take {
            Some((from, names)) => {
                let cx = Engine::new(run, from);
                let source = crate::mount(from, &cx)?;
                let folder = Fresh(crate::stage::new_folder()?);
                crate::stage_into(source.as_ref(), names, &folder.0, &cx)?;
                let mut used = HashSet::new();
                let mut puts = Vec::new();
                for name in names {
                    if !used.insert(last(name).to_lowercase()) {
                        run.fail(Path::new(&from.text()).join(name).as_path(), &io::ErrorKind::AlreadyExists.into());
                        continue;
                    }
                    let to = if self.into.is_empty() {
                        last(name).to_owned()
                    } else {
                        format!("{}/{}", self.into, last(name))
                    };
                    edits.push(Edit::Put { from: under(&folder.0, name), to });
                    puts.push((under(&folder.0, name), name.clone()));
                }
                Some((folder, puts))
            }
            None => None,
        };
        if self.replace && crate::queue_key(&self.root).is_none() {
            let cx = Engine::new(run, &self.root);
            let vfs = crate::mount(&self.root, &cx)?;
            edits = edits
                .into_iter()
                .flat_map(|edit| {
                    let delete = match &edit {
                        Edit::Put { to, .. } if vfs.is_file(to) || vfs.is_dir(to) => Some(Edit::Delete(to.clone())),
                        _ => None,
                    };
                    delete.into_iter().chain([edit])
                })
                .collect();
        }
        let Some((outcome, done)) = self.write_at(&self.root, edits, trash, run)? else { return Ok(Outcome::Nothing) };
        let mut outcomes = Vec::new();
        flatten(outcome, &mut outcomes);
        if self.moving {
            match (&self.take, &taken) {
                (Some((from, _)), Some((_, puts))) => {
                    // Deleted in the place they came from: only those that went in whole.
                    let gone: Vec<Edit> = puts
                        .iter()
                        .filter(|(staged, _)| done.sources.contains(staged))
                        .map(|(_, name)| Edit::Delete(name.clone()))
                        .collect();
                    run.touched(disk_of(from));
                    if !gone.is_empty()
                        && let Some((outcome, _)) = self.write_at(from, gone, source_trash, run)?
                    {
                        flatten(outcome, &mut outcomes);
                    }
                }
                _ => {
                    // The folders they leave change: panes showing them are read again.
                    run.touched(done.sources.iter().map(PathBuf::as_path));
                    for source in done.sources {
                        match gezik_ops::trash_path(&source) {
                            Ok(trashed) => outcomes.push(Outcome::Trashed { original: source, trashed }),
                            Err(err) => run.fail(&source, &err),
                        }
                    }
                }
            }
        }
        drop(taken);
        Ok(Outcome::Several(outcomes))
    }
}

/// On a phone, the puts whose names are taken there (a folder onto a folder merges) are asked
/// about once (Replace / Skip / Keep both); replacing where the phone cannot rename asks again,
/// since the old file then goes first. `replace` (Update): replaced without asking. The edits
/// to apply and whether they replace; `None`: cancelled.
fn settle_on_device(
    vfs: &dyn crate::Vfs,
    mut edits: Vec<Edit>,
    replace: bool,
    run: &RunCx<'_>,
) -> io::Result<Option<(Vec<Edit>, bool)>> {
    let mut read = HashSet::new();
    let mut taken = |to: &str, is_dir: bool| {
        let dir = to.rsplit_once('/').map_or("", |(up, _)| up).to_owned();
        if read.insert(dir.clone()) {
            let _ = vfs.list(&dir);
        }
        vfs.is_file(to) || (vfs.is_dir(to) && !is_dir)
    };
    let clashes: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, edit)| matches!(edit, Edit::Put { from, to } if taken(to, from.is_dir())))
        .map(|(i, _)| i)
        .collect();
    if clashes.is_empty() || replace {
        return Ok(Some((edits, replace)));
    }
    let names: Vec<&str> = clashes.iter().map(|&i| last(edit_name(&edits[i]))).collect();
    let title = gezik_i18n::tn!("{n} item already on the phone" | "{n} items already on the phone", names.len());
    let buttons = [t("Replace"), t("Skip"), t("Keep both"), t("Cancel")].to_vec();
    let mut message = names.join("\n");
    // A file put where a folder is: Replace deletes the folder with what it holds.
    if clashes.iter().any(|&i| vfs.is_dir(edit_name(&edits[i]))) {
        message.push_str("\n\n");
        message.push_str(&gezik_i18n::t!("Replace deletes a folder of that name with everything in it."));
    }
    match run.ask(Question::Confirm { title, message, buttons }) {
        Answer::Button(0) => {
            if clashes.iter().any(|&i| !vfs.can_rename(edit_name(&edits[i]))) {
                let title = gezik_i18n::t!("Replace on the phone?").into_owned();
                let message = gezik_i18n::t!("The old file is deleted before the new one is written").into_owned();
                let buttons = vec![t("Replace"), t("Cancel")];
                if run.ask(Question::Confirm { title, message, buttons }) != Answer::Button(0) {
                    return Ok(None);
                }
            }
            Ok(Some((edits, true)))
        }
        Answer::Button(1) => {
            let kept = edits.into_iter().enumerate().filter(|(i, _)| !clashes.contains(i)).map(|(_, e)| e).collect();
            Ok(Some((kept, false)))
        }
        Answer::Button(2) => {
            for &i in &clashes {
                if let Edit::Put { from, to } = &mut edits[i] {
                    let dir = to.rsplit_once('/').map_or(String::new(), |(up, _)| format!("{up}/"));
                    let free = gezik_core::ops::names::next_free(last(to), from.is_dir(), |name| {
                        let path = format!("{dir}{name}");
                        vfs.is_file(&path) || vfs.is_dir(&path)
                    });
                    *to = format!("{dir}{free}");
                }
            }
            Ok(Some((edits, false)))
        }
        _ => Ok(None),
    }
}

/// A button's text.
fn t(text: &'static str) -> String {
    gezik_i18n::t("", text).into_owned()
}

fn flatten(outcome: Outcome, into: &mut Vec<Outcome>) {
    match outcome {
        Outcome::Several(all) => into.extend(all),
        one => into.push(one),
    }
}

/// The items a move into folder `dir` may take: in one place, not `dir` nor a folder holding
/// it (it would go into itself), nor what is in `dir` already (it would replace itself).
pub fn movable(items: &[VfsPath], dir: &VfsPath) -> Vec<VfsPath> {
    items
        .iter()
        .filter(|item| {
            if !crate::same_root(&item.root, &dir.root) {
                return true;
            }
            let fold = |s: &str| {
                if cfg!(windows) || crate::queue_key(&item.root).is_some() { s.to_lowercase() } else { s.to_owned() }
            };
            let (item_key, dir_key) = (fold(&item.inner), fold(&dir.inner));
            let into_itself = dir_key == item_key || dir_key.starts_with(&format!("{item_key}/"));
            let parent = item_key.rsplit_once('/').map_or("", |(up, _)| up);
            !into_itself && parent != dir_key
        })
        .cloned()
        .collect()
}

/// The name an edit acts on.
fn edit_name(edit: &Edit) -> &str {
    match edit {
        Edit::Put { to: name, .. }
        | Edit::MakeDir(name)
        | Edit::Delete(name)
        | Edit::Unmake { path: name, .. }
        | Edit::Rename { from: name, .. } => name,
    }
}

/// The last part of an inner path.
fn last(inner: &str) -> &str {
    inner.rsplit('/').next().unwrap_or(inner)
}

/// `rapor.pdf`, or `3 items`.
fn what(items: &[VfsPath]) -> String {
    match items {
        [one] => one.name(),
        many => gezik_i18n::tn!("{n} item" | "{n} items", many.len()),
    }
}

/// The archive on disk a root is in (the outermost one).
fn disk_of(root: &VfsRoot) -> Option<&Path> {
    match root {
        VfsRoot::Archive(Container::Disk(file)) => Some(file),
        VfsRoot::Archive(Container::Inside(at)) => disk_of(&at.root),
        VfsRoot::Device { .. } => None,
    }
}

/// Taking out's needs, met by the engine: progress, cancel, the password, failure rows.
struct Engine<'a, 'r> {
    run: &'a RunCx<'r>,
    /// The place's text, for the password question and failure rows.
    place: String,
}

impl<'a, 'r> Engine<'a, 'r> {
    fn new(run: &'a RunCx<'r>, root: &VfsRoot) -> Self {
        Engine { run, place: root.text() }
    }
}

impl Cx for Engine<'_, '_> {
    fn add_bytes(&self, n: u64) {
        self.run.add_bytes(n);
    }

    fn entry_done(&self) {}

    fn stopped(&self) -> bool {
        self.run.stopped()
    }

    fn password(&self, retry: bool) -> Option<String> {
        match self.run.ask(Question::Password { archive: PathBuf::from(&self.place), retry }) {
            Answer::Text(password) => Some(password),
            _ => None,
        }
    }

    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.run.fail(&Path::new(&self.place).join(name), error);
    }

    fn entry_skipped(&self, name: &str, why: &io::Error) {
        self.run.skip(&Path::new(&self.place).join(name), why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_out_takes_out_then_places_in_one_job() {
        let at = VfsPath::archive("/w/a.zip".into()).join("docs");
        let (tasks, label) = copy_out(&[at.join("x.txt"), at.join("sub")], "/out".into());
        assert_eq!((tasks.len(), label.as_str()), (2, "Copy 2 items"));
        assert_eq!(tasks[0].title(), "Copying 2 items");
        let paths = tasks[0].resources().paths;
        assert_eq!(paths, [PathBuf::from("/out"), PathBuf::from("/w/a.zip")]);
        let (_, one) = copy_out(&[at.join("x.txt")], "/out".into());
        assert_eq!(one, "Copy x.txt");
        let (tasks, label) = move_out(&[at.join("x.txt")], "/out".into());
        assert_eq!((tasks.len(), label.as_str()), (3, "Move 1 item"));
        assert_eq!(tasks[2].kind(), TaskKind::Trash);
    }

    #[test]
    fn edits_say_what_they_do_and_queue_on_the_archive_file() {
        let top = VfsPath::archive("/w/a.zip".into());
        let nested = VfsPath::inside(top.join("in/b.tar.gz")).join("src");
        let delete = VfsEditTask::new(nested.root.clone(), vec![Edit::Delete("src/x.txt".into())]);
        assert_eq!((delete.kind(), delete.title().as_str()), (TaskKind::Trash, "Deleting x.txt from b.tar.gz"));
        assert_eq!(delete.resources().paths, [PathBuf::from("/w/a.zip")], "the outermost archive is the key");
        let rename = VfsEditTask::new(top.root.clone(), vec![Edit::Rename { from: "a".into(), to: "b".into() }]);
        assert_eq!(rename.title(), "Renaming a to b");
        let put = VfsEditTask::put(&top.join("docs"), vec!["/d/new.txt".into()], true);
        assert_eq!((put.kind(), put.title().as_str()), (TaskKind::Move, "Moving new.txt to docs"));
        assert_eq!(put.edits, [Edit::Put { from: "/d/new.txt".into(), to: "docs/new.txt".into() }]);
        // Within one archive a move is a rename.
        let moved = VfsEditTask::copy_in(&[top.join("a/x.txt")], &top.join("b"), true);
        assert_eq!(moved.edits, [Edit::Rename { from: "a/x.txt".into(), to: "b/x.txt".into() }]);
        let copied = VfsEditTask::copy_in(&[top.join("a/x.txt")], &top.join("b"), false);
        assert!(copied.edits.is_empty() && copied.take.is_some());
    }
}
