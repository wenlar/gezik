//! A phone as a place (spec 16 §10.1): the session of `gezik_platform::mtp` behind `Vfs`. Its
//! top holds the storages as folders; a folder is asked of the device each time it is listed.
//! The device knows objects by id, Gezik by path: `IdCache` keeps what was listed while the
//! phone is open. Two objects of one name in a folder (MTP allows it) both show, the second as
//! `name (2)`; every change goes by id.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use gezik_core::Entry;
use gezik_core::vfs::{Caps, Edit, VfsPath, VfsRoot};
use gezik_ops::Outcome;
use gezik_platform::mtp::{Object, Session};

use crate::{Cx, Vfs, under};

/// What a phone's changes did: the outcomes (undo), the files and folders on disk put in whole
/// (a move trashes these, and only these), and why it stopped early (a cancel, the phone gone):
/// what was done before that is kept for undo.
#[derive(Debug, Default)]
pub struct Applied {
    pub outcomes: Vec<Outcome>,
    pub sources: Vec<PathBuf>,
    pub ended: Option<io::Error>,
}

/// Inner path → the object listed there.
#[derive(Debug, Default)]
pub(crate) struct IdCache(HashMap<String, Object>);

/// Whether two names are one on the phone's terms (Unicode lower case).
fn same_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

/// Whether `name` can be one part of a path.
fn usable(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

impl IdCache {
    /// Puts folder `dir`'s new listing in place of all that was known under it; gives the rows
    /// as shown, a repeated name numbered (`a (2).jpg`), and the names no path can hold (a
    /// slash, `.`, `..`), which are left out.
    fn fill(&mut self, dir: &str, objects: Vec<Object>) -> (Vec<Object>, Vec<String>) {
        let inside = format!("{dir}/");
        self.0.retain(|path, _| !dir.is_empty() && !path.starts_with(&inside));
        let mut shown: Vec<Object> = Vec::with_capacity(objects.len());
        let mut unusable = Vec::new();
        for mut object in objects {
            if !usable(&object.name) {
                unusable.push(object.name);
                continue;
            }
            let taken = |name: &str| shown.iter().any(|o| same_name(&o.name, name));
            if taken(&object.name) {
                object.name = gezik_core::ops::names::next_free(&object.name, object.is_dir, taken);
            }
            self.0.insert(join(dir, &object.name), object.clone());
            shown.push(object);
        }
        (shown, unusable)
    }

    fn get(&self, path: &str) -> Option<&Object> {
        self.0.get(path)
    }

    fn put(&mut self, path: String, object: Object) {
        self.0.insert(path, object);
    }

    /// Forgets `path` and what is under it.
    fn forget(&mut self, path: &str) {
        let inside = format!("{path}/");
        self.0.retain(|p, _| p != path && !p.starts_with(&inside));
    }

    /// `from` (and what is under it) is now `to`.
    fn moved(&mut self, from: &str, to: &str, name: &str) {
        let inside = format!("{from}/");
        let keys: Vec<String> = self.0.keys().filter(|p| *p == from || p.starts_with(&inside)).cloned().collect();
        for key in keys {
            if let Some(mut object) = self.0.remove(&key) {
                if key == from {
                    object.name = name.to_owned();
                }
                self.0.insert(format!("{to}{}", &key[from.len()..]), object);
            }
        }
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

pub struct MtpVfs {
    root: VfsRoot,
    /// Taken when it is let go of (closed on a worker, never on the UI thread).
    session: Option<Box<dyn Session>>,
    cache: Mutex<IdCache>,
    /// Its folder in the temporary area (previews, files opened with their app).
    stage: PathBuf,
    writable: bool,
    /// The phone went away once: this session is done with, even if it comes back (the next
    /// mount opens a new one).
    lost: AtomicBool,
}

/// What a put made: (path, the object's id), in the order made.
type Made = Vec<(String, String)>;

impl MtpVfs {
    /// The phone `root` (a `VfsRoot::Device`) through `session`; its storages are read now.
    pub fn open(root: VfsRoot, session: Box<dyn Session>) -> io::Result<Arc<dyn Vfs>> {
        gezik_ops::set_vfs_undo(|root, edits| Arc::new(crate::tasks::VfsEditTask::new(root, edits)));
        let writable = session.writable();
        let storages = session.storages()?;
        let vfs = MtpVfs {
            root,
            session: Some(session),
            cache: Mutex::default(),
            stage: crate::stage::new_folder()?,
            writable,
            lost: AtomicBool::new(false),
        };
        vfs.cache().fill("", storages);
        Ok(Arc::new(vfs))
    }

    fn session(&self) -> &dyn Session {
        self.session.as_deref().expect("open while it lives")
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, IdCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lists folder `dir` on the device (`""`: the storages) and keeps it; the names no path
    /// can hold come second.
    fn read_dir(&self, dir: &str) -> io::Result<(Vec<Object>, Vec<String>)> {
        let objects = if dir.is_empty() {
            self.session().storages()
        } else {
            let id = self.resolve(dir)?.id;
            self.session().list(&id)
        };
        match objects {
            Ok(objects) => Ok(self.cache().fill(dir, objects)),
            Err(err) => {
                self.gone(&err);
                Err(err)
            }
        }
    }

    /// The object at `path`, its folders listed first if they were not.
    fn resolve(&self, path: &str) -> io::Result<Object> {
        if let Some(object) = self.cache().get(path) {
            return Ok(object.clone());
        }
        if path.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.read_dir(parent(path))?;
        self.cache().get(path).cloned().ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    /// The id of folder `dir` to make things in; the top (the storages) holds none.
    fn folder_id(&self, dir: &str) -> io::Result<String> {
        let object = self.resolve(dir)?;
        if object.is_dir { Ok(object.id) } else { Err(io::ErrorKind::NotADirectory.into()) }
    }

    /// Whether an error ends the job (a stop, the phone gone); the phone gone is remembered.
    fn gone(&self, err: &io::Error) -> bool {
        if !self.session().connected() {
            self.lost.store(true, Ordering::SeqCst);
            return true;
        }
        err.kind() == io::ErrorKind::Interrupted
    }

    /// Takes `object` out to `dest` at `path` (a folder with what it holds). Every entry not
    /// taken out whole is a failure row: a move then deletes nothing of it.
    fn take(&self, object: &Object, path: &str, dest: &Path, cx: &dyn Cx) -> io::Result<()> {
        if cx.stopped() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let Some(to) = gezik_core::batch::archive::safe_join(dest, path) else {
            cx.entry_failed(path, &io::ErrorKind::InvalidInput.into());
            return Ok(());
        };
        if object.is_dir {
            std::fs::create_dir_all(&to)?;
            let (children, unusable) = match self.session().list(&object.id) {
                Ok(children) => self.cache().fill(path, children),
                Err(err) if self.gone(&err) => return Err(err),
                Err(err) => {
                    cx.entry_failed(path, &err);
                    return Ok(());
                }
            };
            for name in unusable {
                let why =
                    io::Error::new(io::ErrorKind::InvalidData, gezik_i18n::t!("A name no path can hold").into_owned());
                cx.entry_failed(&format!("{path}/{name}"), &why);
            }
            for child in children {
                self.take(&child, &join(path, &child.name), dest, cx)?;
            }
            return Ok(());
        }
        if let Some(folder) = to.parent() {
            std::fs::create_dir_all(folder)?;
        }
        match self.read_whole(object, &to, cx) {
            Ok(()) => {
                cx.entry_done();
                Ok(())
            }
            Err(err) => {
                let _ = std::fs::remove_file(&to);
                if self.gone(&err) {
                    return Err(err);
                }
                cx.entry_failed(path, &err);
                Ok(())
            }
        }
    }

    /// Writes file `object` to `to`, checking it came whole (as many bytes as the phone said),
    /// with the phone's change time (spec 16 §10.1).
    fn read_whole(&self, object: &Object, to: &Path, cx: &dyn Cx) -> io::Result<()> {
        let mut file = std::fs::File::create(to)?;
        let mut total = 0u64;
        self.session().read_to(&object.id, &mut file, &mut |n| {
            total += n;
            cx.add_bytes(n);
            !cx.stopped()
        })?;
        if object.size > 0 && total != object.size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                gezik_i18n::t!("The file came short from the phone").into_owned(),
            ));
        }
        if let Some(modified) = object.modified {
            let _ = file.set_modified(modified);
        }
        Ok(())
    }

    /// Puts the file or folder `from` into folder `dir` (id `dir_id`) as `name`. `replace`: an
    /// entry of that name there is replaced, else a file there is left and the put skipped.
    /// Gives whether all of it went in; `made` gets every object made, as it is made.
    #[allow(clippy::too_many_arguments)]
    fn put(
        &self,
        from: &Path,
        dir: &str,
        dir_id: &str,
        name: &str,
        replace: bool,
        made: &mut Made,
        cx: &dyn Cx,
    ) -> io::Result<bool> {
        if cx.stopped() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let path = join(dir, name);
        let meta = std::fs::symlink_metadata(from)?;
        let there = self.cache().get(&path).cloned();
        if meta.is_dir() {
            let id = match there {
                Some(folder) if folder.is_dir => folder.id,
                Some(file) if replace => {
                    self.session().delete(&file.id)?;
                    self.cache().forget(&path);
                    self.mkdir(dir_id, &path, name, made)?
                }
                Some(_) => {
                    cx.entry_skipped(&path, &io::ErrorKind::AlreadyExists.into());
                    return Ok(false);
                }
                None => self.mkdir(dir_id, &path, name, made)?,
            };
            self.read_dir(&path)?;
            let mut whole = true;
            for entry in std::fs::read_dir(from)? {
                let entry = entry?;
                let child = entry.file_name().to_string_lossy().into_owned();
                whole &= self.put(&entry.path(), &path, &id, &child, replace, made, cx)?;
            }
            return Ok(whole);
        }
        if !meta.is_file() {
            // A link is not followed onto a phone.
            cx.entry_skipped(&path, &io::ErrorKind::Unsupported.into());
            return Ok(false);
        }
        let send = |as_name: &str| -> io::Result<Object> {
            let mut file = std::fs::File::open(from)?;
            self.session().put(dir_id, as_name, meta.len(), &mut file, &mut |n| {
                cx.add_bytes(n);
                !cx.stopped()
            })
        };
        let sent = match there {
            None => send(name).inspect(|object| made.push((path.clone(), object.id.clone()))),
            Some(_) if !replace => {
                cx.entry_skipped(&path, &io::ErrorKind::AlreadyExists.into());
                return Ok(false);
            }
            // What it replaced cannot come back: no undo for it.
            Some(old) => self.replace(&old, dir_id, name, &send, cx),
        };
        match sent {
            Ok(object) => {
                self.cache().put(path, object);
                cx.entry_done();
                Ok(true)
            }
            Err(err) if self.gone(&err) => Err(err),
            Err(err) => {
                cx.entry_failed(&path, &err);
                Ok(false)
            }
        }
    }

    /// Replaces `old` in folder `dir_id` by what `send` writes (spec 16 §10.1: no writing in
    /// place on MTP). Written under a free name, the old one deleted, the new one renamed; the
    /// temporary one goes again if the old one stays. Where renaming is refused the old one is
    /// kept on disk first, deleted, and put back if the new one does not go in.
    fn replace(
        &self,
        old: &Object,
        dir_id: &str,
        name: &str,
        send: &dyn Fn(&str) -> io::Result<Object>,
        cx: &dyn Cx,
    ) -> io::Result<Object> {
        if old.is_dir {
            // The job's question said the folder goes with what it holds.
            self.session().delete(&old.id)?;
            return send(name);
        }
        if self.session().can_rename(&old.id) {
            let temp = format!("{name}.gezik-tmp");
            let mut object = send(&temp)?;
            if let Err(err) = self.session().delete(&old.id) {
                let _ = self.session().delete(&object.id);
                return Err(err);
            }
            if let Err(err) = self.session().rename(&object.id, name) {
                // The old one is gone and this is the only copy: kept under its temporary name.
                let why = gezik_i18n::t!("{0}; the new file is kept on the phone as {1}", err, temp);
                return Err(io::Error::new(err.kind(), why));
            }
            object.name = name.to_owned();
            return Ok(object);
        }
        let keep = crate::stage::new_folder()?;
        let kept = keep.join("old");
        let result = (|| {
            self.read_whole(old, &kept, cx)?;
            self.session().delete(&old.id)?;
            send(name).or_else(|err| {
                let mut file = std::fs::File::open(&kept)?;
                match self.session().put(dir_id, name, old.size, &mut file, &mut |_| true) {
                    Ok(_) => Err(err),
                    Err(lost) => Err(io::Error::new(
                        err.kind(),
                        gezik_i18n::t!("{0}; the old file could not be put back: {1}", err, lost),
                    )),
                }
            })
        })();
        let _ = std::fs::remove_dir_all(&keep);
        result
    }

    fn mkdir(&self, dir_id: &str, path: &str, name: &str, made: &mut Made) -> io::Result<String> {
        let folder = self.session().mkdir(dir_id, name)?;
        let id = folder.id.clone();
        made.push((path.to_owned(), id.clone()));
        self.cache().put(path.to_owned(), folder);
        Ok(id)
    }

    fn at(&self, inner: &str) -> VfsPath {
        VfsPath { root: self.root.clone(), inner: inner.to_owned() }
    }

    /// Applies `edits` one by one (spec 16 §4.6: on a phone, each on its own). `replace`: puts
    /// replace entries of their names (the job asked); else those are skipped. A failure is a
    /// row and the next edit goes on; a stop or the phone gone ends it, keeping what was done.
    pub fn apply(&self, edits: &[Edit], replace: bool, cx: &dyn Cx) -> io::Result<Applied> {
        let mut applied = Applied::default();
        let mut read = HashSet::new();
        for edit in edits {
            if cx.stopped() {
                applied.ended = Some(io::ErrorKind::Interrupted.into());
                break;
            }
            let mut made = Made::new();
            let done = self.apply_one(edit, replace, &mut read, &mut made, &mut applied, cx);
            applied.outcomes.extend(made.into_iter().map(|(path, id)| Outcome::VfsCreated { at: self.at(&path), id }));
            if let Err(err) = done {
                if self.gone(&err) {
                    applied.ended = Some(err);
                    break;
                }
                cx.entry_failed(edit_name(edit), &err);
            }
        }
        Ok(applied)
    }

    fn apply_one(
        &self,
        edit: &Edit,
        replace: bool,
        read: &mut HashSet<String>,
        made: &mut Made,
        applied: &mut Applied,
        cx: &dyn Cx,
    ) -> io::Result<()> {
        // A storage is the phone's own: never deleted, renamed or replaced.
        if !edit_name(edit).contains('/') {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                gezik_i18n::t!("A storage of the phone cannot be changed").into_owned(),
            ));
        }
        match edit {
            Edit::Put { from, to } => {
                let dir = parent(to);
                let dir_id = self.folder_id(dir)?;
                // What is there now, once per folder.
                if read.insert(dir.to_owned()) {
                    self.read_dir(dir)?;
                }
                if self.put(from, dir, &dir_id, last(to), replace, made, cx)? {
                    applied.sources.push(from.clone());
                }
                Ok(())
            }
            Edit::MakeDir(path) => {
                let dir_id = self.folder_id(parent(path))?;
                self.mkdir(&dir_id, path, last(path), made).map(drop)
            }
            // Kept nowhere: the job said it cannot be undone.
            Edit::Delete(path) => {
                let object = self.resolve(path)?;
                self.session().delete(&object.id)?;
                self.cache().forget(path);
                Ok(())
            }
            // The undo of something made here: only if it is still that object, a folder only
            // once empty (what went into it went first).
            Edit::Unmake { path, id } => {
                if read.insert(parent(path).to_owned()) {
                    self.read_dir(parent(path))?;
                }
                let Some(object) = self.cache().get(path).cloned().filter(|o| o.id == *id) else {
                    cx.entry_skipped(path, &io::Error::other(gezik_i18n::t!("It changed since").into_owned()));
                    return Ok(());
                };
                if object.is_dir && !self.session().list(&object.id)?.is_empty() {
                    cx.entry_skipped(path, &io::Error::other(gezik_i18n::t!("It is not empty").into_owned()));
                    return Ok(());
                }
                self.session().delete(&object.id)?;
                self.cache().forget(path);
                Ok(())
            }
            Edit::Rename { from, to } => {
                self.rename(from, to)?;
                applied.outcomes.push(Outcome::VfsRenamed { from: self.at(from), to: self.at(to) });
                Ok(())
            }
        }
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        if parent(from) != parent(to) {
            // MTP moves an object only by copying it; a move goes as a copy and a delete.
            return Err(io::ErrorKind::Unsupported.into());
        }
        self.read_dir(parent(from))?;
        let object = self.cache().get(from).cloned().ok_or(io::ErrorKind::NotFound)?;
        if !same_name(from, to) && self.cache().get(to).is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        self.session().rename(&object.id, last(to))?;
        self.cache().moved(from, to, last(to));
        Ok(())
    }
}

impl Vfs for MtpVfs {
    fn root(&self) -> &VfsRoot {
        &self.root
    }

    fn caps(&self) -> Caps {
        if self.writable { Caps::EDIT } else { Caps::default() }
    }

    fn list(&self, dir: &str) -> Option<Vec<Entry>> {
        let (objects, _) = self.read_dir(dir).ok()?;
        Some(
            objects
                .into_iter()
                .map(|o| Entry {
                    name: o.name,
                    is_dir: o.is_dir,
                    flags: 0,
                    size: if o.is_dir { 0 } else { o.size },
                    modified: o.modified,
                    created: None,
                })
                .collect(),
        )
    }

    fn is_file(&self, path: &str) -> bool {
        self.cache().get(path).is_some_and(|o| !o.is_dir)
    }

    fn is_dir(&self, path: &str) -> bool {
        path.is_empty() || self.cache().get(path).is_some_and(|o| o.is_dir)
    }

    fn packed(&self, _path: &str) -> Option<(Option<u64>, bool)> {
        None
    }

    fn stage(&self, items: &[String], dest: &Path, cx: &dyn Cx) -> io::Result<()> {
        for item in items {
            let object = self.resolve(item)?;
            self.take(&object, item, dest, cx)?;
        }
        Ok(())
    }

    fn staged(&self, path: &str) -> PathBuf {
        under(&self.stage, path)
    }

    fn unchanged(&self) -> bool {
        if !self.session().connected() {
            self.lost.store(true, Ordering::SeqCst);
        }
        !self.lost.load(Ordering::SeqCst)
    }

    fn lost(&self) {
        self.lost.store(true, Ordering::SeqCst);
    }

    fn len(&self) -> usize {
        self.cache().len()
    }

    fn can_rename(&self, path: &str) -> bool {
        self.cache().get(path).is_none_or(|o| self.session().can_rename(&o.id))
    }

    fn apply(&self, edits: &[Edit], replace: bool, cx: &dyn Cx) -> io::Result<Applied> {
        MtpVfs::apply(self, edits, replace, cx)
    }

    fn device(&self) -> bool {
        true
    }
}

impl Drop for MtpVfs {
    fn drop(&mut self) {
        // Closing the device may wait for it: on a worker, with the temporary files. Without
        // a thread to be had the closure is dropped here, which closes it here.
        let (session, stage) = (self.session.take(), std::mem::take(&mut self.stage));
        let _ = std::thread::Builder::new().name("gezik-mtp-close".into()).spawn(move || {
            drop(session);
            let _ = std::fs::remove_dir_all(&stage);
        });
    }
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(up, _)| up)
}

fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_owned() } else { format!("{dir}/{name}") }
}

fn edit_name(edit: &Edit) -> &str {
    match edit {
        Edit::Put { to: name, .. }
        | Edit::MakeDir(name)
        | Edit::Delete(name)
        | Edit::Unmake { path: name, .. }
        | Edit::Rename { from: name, .. } => name,
    }
}
