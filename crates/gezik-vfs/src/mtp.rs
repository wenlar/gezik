//! A phone as a place (spec 16 §10.1): the session of `gezik_platform::mtp` behind `Vfs`. Its
//! top holds the storages as folders; a folder is asked of the device each time it is listed.
//! The device knows objects by id, Gezik by path: `IdCache` keeps what was listed while the
//! phone is open. Two objects of one name in a folder (MTP allows it) both show, the second as
//! `name (2)`; every change goes by id.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use gezik_core::Entry;
use gezik_core::vfs::{Caps, Edit, VfsPath, VfsRoot};
use gezik_ops::Outcome;
use gezik_platform::mtp::{Object, Session};

use crate::{Cx, Vfs, under};

/// What a phone's changes did: the outcomes (undo) and the files and folders on disk put in
/// whole (a move trashes these, and only these).
#[derive(Debug, Default)]
pub struct Applied {
    pub outcomes: Vec<Outcome>,
    pub sources: Vec<PathBuf>,
}

/// Inner path → the object listed there.
#[derive(Debug, Default)]
pub(crate) struct IdCache(HashMap<String, Object>);

impl IdCache {
    /// Puts folder `dir`'s new listing in place of its old one; gives the rows as shown, a
    /// repeated name numbered (`a (2).jpg`). Names no path can hold are left out.
    fn fill(&mut self, dir: &str, objects: Vec<Object>) -> Vec<Object> {
        self.0.retain(|path, _| parent(path) != dir);
        let mut shown: Vec<Object> = Vec::with_capacity(objects.len());
        // shortcut: a name with a slash, or `.`/`..`, cannot be a path part and is not shown; rename it on the phone.
        for mut object in objects
            .into_iter()
            .filter(|o| !o.name.is_empty() && !o.name.contains(['/', '\\']) && o.name != "." && o.name != "..")
        {
            let taken = |name: &str| shown.iter().any(|o| o.name.eq_ignore_ascii_case(name));
            if taken(&object.name) {
                object.name = gezik_core::ops::names::next_free(&object.name, object.is_dir, taken);
            }
            self.0.insert(join(dir, &object.name), object.clone());
            shown.push(object);
        }
        shown
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
    session: Box<dyn Session>,
    cache: Mutex<IdCache>,
    /// Its folder in the temporary area (previews, files opened with their app).
    stage: PathBuf,
    writable: bool,
}

impl MtpVfs {
    /// The phone `root` (a `VfsRoot::Device`) through `session`; its storages are read now.
    pub fn open(root: VfsRoot, session: Box<dyn Session>) -> io::Result<Arc<dyn Vfs>> {
        gezik_ops::set_vfs_undo(|root, edits| Arc::new(crate::tasks::VfsEditTask::new(root, edits)));
        let writable = session.writable();
        let vfs = MtpVfs { root, session, cache: Mutex::default(), stage: crate::stage::new_folder()?, writable };
        let storages = vfs.session.storages()?;
        vfs.cache().fill("", storages);
        Ok(Arc::new(vfs))
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, IdCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lists folder `dir` on the device (`""`: the storages) and keeps it.
    fn read_dir(&self, dir: &str) -> io::Result<Vec<Object>> {
        let objects = if dir.is_empty() {
            self.session.storages()?
        } else {
            let id = self.resolve(dir)?.id;
            self.session.list(&id)?
        };
        Ok(self.cache().fill(dir, objects))
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

    /// Whether an error means the phone is gone (then nothing else is tried).
    fn gone(&self, err: &io::Error) -> bool {
        err.kind() == io::ErrorKind::Interrupted || !self.session.connected()
    }

    /// Takes `object` out to `dest` at `path` (a folder with what it holds).
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
            let children = match self.session.list(&object.id) {
                Ok(children) => self.cache().fill(path, children),
                Err(err) if self.gone(&err) => return Err(err),
                Err(err) => {
                    cx.entry_failed(path, &err);
                    return Ok(());
                }
            };
            for child in children {
                self.take(&child, &join(path, &child.name), dest, cx)?;
            }
            return Ok(());
        }
        if let Some(folder) = to.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut file = std::fs::File::create(&to)?;
        let read = self.session.read_to(&object.id, &mut file, &mut |n| {
            cx.add_bytes(n);
            !cx.stopped()
        });
        match read {
            Ok(()) => {
                // The phone's change time (spec 16 §10.1).
                if let Some(modified) = object.modified {
                    let _ = file.set_modified(modified);
                }
                cx.entry_done();
                Ok(())
            }
            Err(err) => {
                drop(file);
                let _ = std::fs::remove_file(&to);
                if self.gone(&err) {
                    return Err(err);
                }
                cx.entry_failed(path, &err);
                Ok(())
            }
        }
    }

    /// Puts the file or folder `from` into folder `dir` (id `dir_id`) as `name`. `replace`: an
    /// entry of that name there is replaced, else a file there is left and the put skipped.
    /// Gives whether all of it went in; `made` gets the place made (none when it merged into a
    /// folder there).
    #[allow(clippy::too_many_arguments)]
    fn put(
        &self,
        from: &Path,
        dir: &str,
        dir_id: &str,
        name: &str,
        replace: bool,
        made: &mut Vec<String>,
        cx: &dyn Cx,
    ) -> io::Result<bool> {
        if cx.stopped() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let path = join(dir, name);
        let meta = std::fs::symlink_metadata(from)?;
        let there = self.cache().get(&path).cloned();
        if meta.is_dir() {
            let (id, new) = match there {
                Some(folder) if folder.is_dir => (folder.id, false),
                Some(file) if replace => {
                    self.session.delete(&file.id)?;
                    self.cache().forget(&path);
                    (self.mkdir(dir_id, &path, name)?, true)
                }
                Some(_) => {
                    cx.entry_skipped(&path, &io::ErrorKind::AlreadyExists.into());
                    return Ok(false);
                }
                None => (self.mkdir(dir_id, &path, name)?, true),
            };
            if new {
                made.push(path.clone());
            }
            self.read_dir(&path)?;
            let mut whole = true;
            let mut inner = Vec::new();
            for entry in std::fs::read_dir(from)? {
                let entry = entry?;
                let child = entry.file_name().to_string_lossy().into_owned();
                whole &= self.put(&entry.path(), &path, &id, &child, replace, &mut inner, cx)?;
            }
            // What was made inside a new folder goes with it on undo.
            if !new {
                made.extend(inner);
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
            self.session.put(dir_id, as_name, meta.len(), &mut file, &mut |n| {
                cx.add_bytes(n);
                !cx.stopped()
            })
        };
        let sent = match there {
            None => send(name).inspect(|_| made.push(path.clone())),
            Some(_) if !replace => {
                cx.entry_skipped(&path, &io::ErrorKind::AlreadyExists.into());
                return Ok(false);
            }
            // No writing in place on MTP (spec 16 §10.1): written under a free name, the old one
            // deleted, the new one renamed; where renaming is refused the old one goes first
            // (the job asked). What it replaced cannot come back: no undo for it.
            Some(old) if self.session.can_rename(&old.id) => (|| {
                let mut object = send(&format!("{name}.gezik-tmp"))?;
                self.session.delete(&old.id)?;
                self.session.rename(&object.id, name)?;
                object.name = name.to_owned();
                Ok(object)
            })(),
            Some(old) => self.session.delete(&old.id).and_then(|()| send(name)),
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

    fn mkdir(&self, dir_id: &str, path: &str, name: &str) -> io::Result<String> {
        let folder = self.session.mkdir(dir_id, name)?;
        let id = folder.id.clone();
        self.cache().put(path.to_owned(), folder);
        Ok(id)
    }

    fn at(&self, inner: &str) -> VfsPath {
        VfsPath { root: self.root.clone(), inner: inner.to_owned() }
    }

    /// Applies `edits` one by one (spec 16 §4.6: on a phone, each on its own). `replace`: puts
    /// replace entries of their names (the job asked); else those are skipped.
    pub fn apply(&self, edits: &[Edit], replace: bool, cx: &dyn Cx) -> io::Result<Applied> {
        let mut applied = Applied::default();
        let mut read = std::collections::HashSet::new();
        for edit in edits {
            if cx.stopped() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let done = match edit {
                Edit::Put { from, to } => {
                    let dir = parent(to);
                    let dir_id = self.folder_id(dir)?;
                    // What is there now, once per folder.
                    if read.insert(dir.to_owned()) {
                        self.read_dir(dir)?;
                    }
                    let mut made = Vec::new();
                    let whole = self.put(from, dir, &dir_id, last(to), replace, &mut made, cx);
                    applied.outcomes.extend(made.iter().map(|path| Outcome::VfsCreated { at: self.at(path) }));
                    if whole? {
                        applied.sources.push(from.clone());
                    }
                    Ok(())
                }
                Edit::MakeDir(path) => self.folder_id(parent(path)).and_then(|dir_id| {
                    self.mkdir(&dir_id, path, last(path))?;
                    applied.outcomes.push(Outcome::VfsCreated { at: self.at(path) });
                    Ok(())
                }),
                // Kept nowhere: the job said it cannot be undone.
                Edit::Delete(path) => self.resolve(path).and_then(|object| {
                    self.session.delete(&object.id)?;
                    self.cache().forget(path);
                    Ok(())
                }),
                Edit::Rename { from, to } => self.rename(from, to).map(|()| {
                    applied.outcomes.push(Outcome::VfsRenamed { from: self.at(from), to: self.at(to) });
                }),
            };
            if let Err(err) = done {
                if self.gone(&err) {
                    return Err(err);
                }
                cx.entry_failed(edit_name(edit), &err);
            }
        }
        Ok(applied)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        if parent(from) != parent(to) {
            // MTP moves an object only by copying it; a move goes as a copy and a delete.
            return Err(io::ErrorKind::Unsupported.into());
        }
        let object = self.resolve(from)?;
        self.read_dir(parent(to))?;
        if !from.eq_ignore_ascii_case(to) && self.cache().get(to).is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        self.session.rename(&object.id, last(to))?;
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
        let objects = self.read_dir(dir).ok()?;
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
        self.session.connected()
    }

    fn len(&self) -> usize {
        self.cache().len()
    }

    fn can_rename(&self, path: &str) -> bool {
        self.cache().get(path).is_none_or(|o| self.session.can_rename(&o.id))
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
        let _ = std::fs::remove_dir_all(&self.stage);
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
        Edit::Put { to: name, .. } | Edit::MakeDir(name) | Edit::Delete(name) | Edit::Rename { from: name, .. } => name,
    }
}
