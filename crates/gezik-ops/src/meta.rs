//! Where a file's tags live (spec 12 §3.3, §4.4): its own record (`gezik_platform::meta`) or,
//! where it cannot hold one, Gezik's central store, which also wins when both have one. The
//! store file is read the first time something needs it, and only if it is there; it is
//! written whole, atomically, once a job that changed it ends.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use gezik_core::meta::Meta;
use gezik_core::meta_store::{Record, Store};
use gezik_platform::meta::{self as own, Volume};

use crate::engine::lock;
use crate::task::Outcome;

/// Where a write went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    File,
    /// Gezik's store: always (`[tags] write-to-files = false`), or because the file or its
    /// volume refused (why, then).
    Store(Option<String>),
}

/// Folders whose volume is known; past this the list starts over.
const MAX_FOLDERS: usize = 1024;
/// How long a folder's volume is trusted: a USB stick swapped for another under the same
/// letter or mount point is asked about again.
const VOLUME_FRESH: Duration = Duration::from_secs(5);

pub struct MetaStore {
    /// `tags-store.tsv` in the config folder (`None`: no store; files only).
    file: Option<PathBuf>,
    /// Never write into files (`[tags] write-to-files = false`).
    store_only: bool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// `None` until something needs it.
    store: Option<Loaded>,
    /// Each folder's volume, and where it is mounted.
    volumes: HashMap<PathBuf, (Instant, Arc<(Volume, PathBuf)>)>,
    /// Volumes that refused a record this session.
    refused: HashSet<String>,
    /// Sources whose tags a copy could not carry: a move does not drop their entries.
    kept: HashSet<PathBuf>,
    #[cfg(test)]
    loads: usize,
    #[cfg(test)]
    own_reads: usize,
    /// Tests: whether a path's volume holds records, instead of asking it.
    #[cfg(test)]
    holds: Option<fn(&Path) -> bool>,
}

struct Loaded {
    store: Store,
    dirty: bool,
    /// Why the file cannot be written (it could not be read): it is never written over.
    broken: Option<String>,
}

impl MetaStore {
    /// Reads nothing until something asks.
    pub fn new(file: Option<PathBuf>, store_only: bool) -> MetaStore {
        MetaStore { file, store_only, state: Mutex::default() }
    }

    fn load<'a>(&self, state: &'a mut State) -> &'a mut Loaded {
        state.store.get_or_insert_with(|| {
            #[cfg(test)]
            {
                state.loads += 1;
            }
            let Some(file) = &self.file else {
                let broken = Some("Gezik has no config folder to keep tags in".to_owned());
                return Loaded { store: Store::default(), dirty: false, broken };
            };
            match std::fs::read(file) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => Loaded { store: Store::parse(&text), dirty: false, broken: None },
                    Err(_) => broken_store(file, "it is not UTF-8"),
                },
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    Loaded { store: Store::default(), dirty: false, broken: None }
                }
                Err(err) => broken_store(file, &gezik_platform::fs::describe(&err)),
            }
        })
    }

    /// The volume `path` is on, and its root.
    fn volume(&self, state: &mut State, path: &Path) -> io::Result<Arc<(Volume, PathBuf)>> {
        let folder = path.parent().unwrap_or(path);
        if let Some((when, known)) = state.volumes.get(folder)
            && when.elapsed() < VOLUME_FRESH
        {
            return Ok(known.clone());
        }
        let root = gezik_platform::fs::drive_root(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no drive for this path"))?;
        #[allow(unused_mut)]
        let mut volume = own::volume(&root)?;
        #[cfg(test)]
        if let Some(holds) = state.holds {
            volume.holds = holds(path);
        }
        if state.volumes.len() >= MAX_FOLDERS {
            state.volumes.clear();
        }
        let known = Arc::new((volume, root));
        state.volumes.insert(folder.to_path_buf(), (Instant::now(), known.clone()));
        Ok(known)
    }

    /// Whether files on `volume` take their own record now.
    fn takes_own(&self, state: &State, volume: &Volume) -> bool {
        !self.store_only && volume.holds && !state.refused.contains(&volume.key)
    }

    /// `path`'s store entry, if it has one that still fits the file there.
    fn stored(&self, state: &mut State, path: &Path) -> io::Result<Option<Meta>> {
        if self.load(state).store.is_empty() {
            return Ok(None);
        }
        let known = self.volume(state, path)?;
        let (volume, root) = &*known;
        let Some(record) = self.load(state).store.get(&volume.key, &in_volume(path, root)) else { return Ok(None) };
        let (size, mtime_ns) = facts(path)?;
        let id = record.file_id.and_then(|_| own::file_id(path));
        Ok(record.fits(size, mtime_ns, id).then(|| record.meta.clone()))
    }

    /// `path`'s tags: its store entry if it has one, else its own record.
    pub fn read(&self, path: &Path) -> io::Result<Meta> {
        let holds = self.volume(&mut lock(&self.state), path)?.0.holds;
        // A volume without records (vfat, exFAT, NFS) has nothing of its own to read.
        #[cfg(test)]
        if holds {
            lock(&self.state).own_reads += 1;
        }
        let own = if holds { own::read(path)?.meta } else { Meta::default() };
        let mut state = lock(&self.state);
        Ok(self.stored(&mut state, path)?.unwrap_or(own))
    }

    /// Writes `meta` for `path` (an empty one removes it): into the file's own record where it
    /// takes one, else into the store. Never changes the file's contents.
    pub fn write(&self, path: &Path, meta: &Meta) -> io::Result<Where> {
        std::fs::symlink_metadata(path)?;
        let known = self.volume(&mut lock(&self.state), path)?;
        let volume = &known.0;
        let mut why = None;
        if self.takes_own(&lock(&self.state), volume) {
            match own::write(path, meta) {
                Ok(()) => {
                    // An older store entry would win over what was just written.
                    let mut state = lock(&self.state);
                    let loaded = self.load(&mut state);
                    if !loaded.store.is_empty() && loaded.broken.is_none() {
                        // Its own entry only: a folder's files keep theirs.
                        let gone = loaded.store.remove(&volume.key, &in_volume(path, &known.1));
                        loaded.dirty |= gone;
                    }
                    return Ok(Where::File);
                }
                Err(err) if own::refused_by_volume(&err) => {
                    lock(&self.state).refused.insert(volume.key.clone());
                    why = Some(err);
                }
                Err(err) if own::refused_by_file(&err) => why = Some(err),
                Err(err) => return Err(err),
            }
        }
        self.write_store(path, &known, meta, why)
    }

    fn write_store(
        &self,
        path: &Path,
        known: &(Volume, PathBuf),
        meta: &Meta,
        why: Option<io::Error>,
    ) -> io::Result<Where> {
        let (volume, root) = known;
        // Removing the entry would bring the file's own tags back: those only go by writing it.
        if meta.is_empty() && volume.holds && !own::read(path)?.meta.is_empty() {
            return Err(why.unwrap_or_else(|| {
                io::Error::other("Its tags are in the file itself; Gezik is set not to write into files")
            }));
        }
        let (size, mtime_ns) = facts(path)?;
        let file_id = if volume.holds { own::file_id(path) } else { None };
        let mut state = lock(&self.state);
        let loaded = self.load(&mut state);
        if let Some(broken) = &loaded.broken {
            return Err(io::Error::other(broken.clone()));
        }
        let at = in_volume(path, root);
        let unknown = loaded.store.get(&volume.key, &at).map(|record| record.unknown.clone()).unwrap_or_default();
        let record = Record { path: at.into(), size, mtime_ns, file_id, meta: meta.clone(), unknown };
        loaded.store.set(&volume.key, record).map_err(io::Error::other)?;
        loaded.dirty = true;
        Ok(Where::Store(why.map(|err| gezik_platform::fs::describe(&err))))
    }

    /// After `from` was copied to `to` (spec 12 §10): a store entry of `from` is written for
    /// `to`; and a copy to a volume that holds no records keeps `from`'s own record in the
    /// store. Between volumes alike, the system took the record along and nothing is read.
    pub fn carry(&self, from: &Path, to: &Path) -> io::Result<()> {
        let result = self.carry_now(from, to);
        if result.is_err() {
            lock(&self.state).kept.insert(from.to_path_buf());
        }
        result
    }

    fn carry_now(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut state = lock(&self.state);
        // `to` is a new file (a copy, maybe over a file it replaced): an entry left at its path
        // is another file's.
        if !self.load(&mut state).store.is_empty() {
            let known = self.volume(&mut state, to)?;
            let at = in_volume(to, &known.1);
            let loaded = self.load(&mut state);
            if loaded.broken.is_none() && loaded.store.remove(&known.0.key, &at) {
                loaded.dirty = true;
            }
        }
        if let Some(meta) = self.stored(&mut state, from)? {
            drop(state);
            return self.write(to, &meta).map(drop);
        }
        let target = self.volume(&mut state, to)?;
        if target.0.holds && !state.refused.contains(&target.0.key) {
            return Ok(());
        }
        if !self.volume(&mut state, from)?.0.holds {
            return Ok(());
        }
        #[cfg(test)]
        {
            state.own_reads += 1;
        }
        drop(state);
        let source = own::read(from)?;
        if source.meta.is_empty() {
            return Ok(());
        }
        self.write_store(to, &target, &source.meta, None).map(drop)
    }

    /// The store follows what a job of Gezik's did: renames and moves take their entries along
    /// (a folder's with everything inside), a move to another drive and a permanent delete
    /// drop them (the copy carried them over). A trashed file keeps its entry at its own path,
    /// where restoring puts it back.
    pub(crate) fn follow(&self, outcomes: &[&Outcome]) {
        let relevant = |outcome: &&&Outcome| {
            matches!(outcome, Outcome::Moved { .. } | Outcome::Deleted { .. } | Outcome::Created { from: Some(_), .. })
        };
        if !outcomes.iter().any(|outcome| relevant(&outcome)) {
            return;
        }
        let mut state = lock(&self.state);
        let loaded = self.load(&mut state);
        if loaded.store.is_empty() || loaded.broken.is_some() {
            return;
        }
        // Renames are done together: in one job `a` <-> `b` or `1` -> `2`, `2` -> `3` name each
        // file as it was before the job.
        let mut moves: Vec<(String, String, String)> = Vec::new();
        for outcome in outcomes.iter().filter(relevant) {
            let (from, to, tree) = match outcome {
                Outcome::Moved { from, to, .. } => (from, Some(to), true),
                // A move to another drive: each item says so itself, a folder only for itself
                // (its files left behind by a cancel keep their entries).
                Outcome::Created { from: Some(from), .. } if !state.kept.contains(from) => (from, None, false),
                Outcome::Deleted { path } => (path, None, true),
                _ => continue,
            };
            // The volume of a path that is gone is its folder's, or the place it went to.
            let Ok(source) = self.volume(&mut state, to.map_or(from.as_path(), |to| to.as_path())) else { continue };
            let at = in_volume(from, &source.1);
            if let Some(to) = to {
                moves.push((source.0.key.clone(), at, in_volume(to, &source.1)));
                continue;
            }
            let loaded = self.load(&mut state);
            let changed = if tree {
                loaded.store.remove_tree(&source.0.key, &at) > 0
            } else {
                loaded.store.remove(&source.0.key, &at)
            };
            loaded.dirty |= changed;
        }
        let moves: Vec<(&str, &str, &str, &str)> =
            moves.iter().map(|(key, from, to)| (key.as_str(), from.as_str(), key.as_str(), to.as_str())).collect();
        let loaded = self.load(&mut state);
        loaded.dirty |= loaded.store.rename_all(&moves) > 0;
    }

    /// Gezik lists `folder` whole for a view that needs tags (spec 12 §4.4, plan 12a sapma 4):
    /// store entries of files another program moved or renamed into it (`files`: name, size,
    /// modification time in ns) follow them, and the store is written. Nothing is read when
    /// the store has no entries. How many were found.
    pub fn refind_in(&self, folder: &Path, files: &[(&str, u64, i64)]) -> usize {
        let Some(first) = files.first() else { return 0 };
        let mut state = lock(&self.state);
        let loaded = self.load(&mut state);
        if loaded.store.is_empty() || loaded.broken.is_some() {
            return 0;
        }
        let Ok(known) = self.volume(&mut state, &folder.join(first.0)) else { return 0 };
        let (volume, root) = &*known;
        let at = in_volume(folder, root);
        let found = self.load(&mut state).store.refind_in(
            &volume.key,
            &at,
            files,
            |i| if volume.holds { own::file_id(&folder.join(files[i].0)) } else { None },
            |path| std::fs::symlink_metadata(root.join(path)).is_err(),
        );
        if found.is_empty() {
            return 0;
        }
        let moves: Vec<(String, String)> =
            found.iter().map(|(from, i)| (from.to_string(), gezik_core::meta_store::join(&at, files[*i].0))).collect();
        let moves: Vec<(&str, &str, &str, &str)> = moves
            .iter()
            .map(|(from, to)| (volume.key.as_str(), from.as_str(), volume.key.as_str(), to.as_str()))
            .collect();
        let loaded = self.load(&mut state);
        loaded.dirty |= loaded.store.rename_all(&moves) > 0;
        drop(state);
        // Written now: no job ends to write it.
        let _ = self.flush();
        found.len()
    }

    /// Writes the store if a job changed it: whole, to a temporary file, then renamed over
    /// the old one, so a crash leaves the old or the new store, never half of one. Left dirty
    /// if it cannot be written (the next job tries again).
    pub fn flush(&self) -> io::Result<()> {
        let mut state = lock(&self.state);
        let Some(loaded) = state.store.as_mut().filter(|loaded| loaded.dirty) else { return Ok(()) };
        let Some(file) = &self.file else { return Ok(()) };
        write_atomic(file, loaded.store.to_text().as_bytes())?;
        loaded.dirty = false;
        Ok(())
    }

    /// Where the store is kept.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}

fn broken_store(file: &Path, why: &str) -> Loaded {
    let broken = Some(format!("Gezik's tag store {} cannot be read ({why}); it is left as it is", file.display()));
    Loaded { store: Store::default(), dirty: false, broken }
}

/// `path` from its volume's root, with `/`.
fn in_volume(path: &Path, root: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .skip(root.components().count())
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

/// Size and modification time (ns since 1970) of `path` itself.
fn facts(path: &Path) -> io::Result<(u64, i64)> {
    let meta = std::fs::symlink_metadata(path)?;
    let mtime = meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok());
    Ok((meta.len(), mtime.map_or(0, |since| i64::try_from(since.as_nanos()).unwrap_or(i64::MAX))))
}

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    // The process id keeps two Gezik processes out of each other's temporary file.
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents)?;
        file.sync_all()
    })();
    let done = written.and_then(|()| std::fs::rename(&temp, path));
    if done.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    done
}

#[cfg(test)]
impl MetaStore {
    pub(crate) fn fake_holds(&self, holds: fn(&Path) -> bool) {
        let mut state = lock(&self.state);
        state.holds = Some(holds);
        state.volumes.clear();
    }

    pub(crate) fn loads(&self) -> usize {
        lock(&self.state).loads
    }

    pub(crate) fn own_reads(&self) -> usize {
        lock(&self.state).own_reads
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_dir;
    use gezik_core::meta::{Tag, TagColor};

    fn red() -> Meta {
        Meta { tags: vec![Tag::new("Red", TagColor::Red)], ..Meta::default() }
    }

    #[cfg(windows)]
    fn blue() -> Meta {
        Meta { tags: vec![Tag::new("Blue", TagColor::Blue)], rating: 2, ..Meta::default() }
    }

    /// Paths with `fat` in them are on a volume that holds no records.
    fn fat(path: &Path) -> bool {
        !path.to_string_lossy().contains("fat")
    }

    fn store(dir: &Path) -> MetaStore {
        MetaStore::new(Some(dir.join("config/tags-store.tsv")), false)
    }

    fn file(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_folder_read_whole_takes_back_tags_moved_by_another_program() {
        let dir = test_dir("meta-refind");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let a = file(&dir, "fat/old/a.txt", "x");
        tags.write(&a, &red()).unwrap();
        tags.flush().unwrap();
        // Explorer moves it: same name, size and time.
        std::fs::create_dir_all(dir.join("fat/new")).unwrap();
        let moved = dir.join("fat/new/a.txt");
        std::fs::rename(&a, &moved).unwrap();
        let (size, mtime) = facts(&moved).unwrap();
        assert!(tags.read(&moved).unwrap().is_empty(), "not found yet");
        let files = [("a.txt", size, mtime), ("other.txt", 1, 2)];
        assert_eq!(tags.refind_in(&dir.join("fat/new"), &files), 1);
        assert_eq!(tags.read(&moved).unwrap(), red());
        assert_eq!(tags.refind_in(&dir.join("fat/new"), &files), 0, "its own entry now");
        let again = store(&dir);
        again.fake_holds(fat);
        assert_eq!(again.read(&moved).unwrap(), red(), "the store file was written");
        assert_eq!(store(&dir.join("empty")).refind_in(&dir, &files), 0);
    }

    /// `cargo test -p gezik-ops --release -- --ignored reading_10k` (spec 12 §14: a folder of
    /// 10,000 read whole in ≤ 1 s on Windows, ≤ 300 ms on Linux and macOS).
    #[test]
    #[ignore]
    fn reading_10k_files_whole_is_within_budget() {
        let dir = test_dir("meta-10k");
        let tags = store(&dir);
        let paths: Vec<PathBuf> = (0..10_000).map(|i| file(&dir, &format!("f/{i}.txt"), "x")).collect();
        for path in paths.iter().step_by(10) {
            tags.write(path, &red()).unwrap();
        }
        let start = Instant::now();
        let tagged = paths.iter().filter(|p| !tags.read(p).unwrap().tags.is_empty()).count();
        let took = start.elapsed();
        eprintln!("read 10,000 files' tags in {took:?}");
        assert_eq!(tagged, 1000);
        assert!(took <= Duration::from_secs(1), "took {took:?}");
    }

    #[test]
    fn nothing_is_read_until_asked() {
        let dir = test_dir("meta-lazy");
        let tags = store(&dir);
        let a = file(&dir, "a.txt", "x");
        tags.follow(&[&Outcome::Nothing, &Outcome::Placed { path: a.clone() }]);
        tags.flush().unwrap();
        assert_eq!(tags.loads(), 0, "no tag work: the store file is never looked at");
        assert!(!dir.join("config").exists());
        tags.follow(&[&Outcome::Deleted { path: a.clone() }]);
        tags.read(&a).unwrap();
        tags.read(&a).unwrap();
        assert_eq!(tags.loads(), 1, "once, and only once");
        assert!(!dir.join("config/tags-store.tsv").exists(), "made only by the first store write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refusing_volume_sends_tags_to_the_store_and_says_so() {
        let dir = test_dir("meta-fat");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let a = file(&dir, "fat/a.txt", "contents");
        let before = std::fs::metadata(&a).unwrap().modified().unwrap();
        assert_eq!(tags.write(&a, &red()).unwrap(), Where::Store(None));
        assert_eq!(tags.read(&a).unwrap(), red());
        assert_eq!(std::fs::read(&a).unwrap(), b"contents");
        assert_eq!(std::fs::metadata(&a).unwrap().modified().unwrap(), before);
        tags.flush().unwrap();
        let text = std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap();
        assert!(text.contains("fat/a.txt\t8\t") && text.ends_with("\t-\ttag 6 Red\n"), "{text}");
        // A fresh start reads it back.
        assert_eq!(store(&dir).read(&a).unwrap(), red());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_read_only_file_goes_to_the_store_with_its_attribute_kept() {
        let dir = test_dir("meta-readonly");
        let tags = store(&dir);
        let a = file(&dir, "a.txt", "x");
        let mut permissions = std::fs::metadata(&a).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&a, permissions.clone()).unwrap();
        let Where::Store(Some(why)) = tags.write(&a, &red()).unwrap() else { panic!("not the store") };
        assert!(why.contains("denied"), "{why}");
        assert!(std::fs::metadata(&a).unwrap().permissions().readonly());
        assert_eq!(tags.read(&a).unwrap(), red());
        // The volume still takes records: another file gets its own.
        let b = file(&dir, "b.txt", "x");
        assert_eq!(tags.write(&b, &red()).unwrap(), Where::File);
        #[allow(clippy::permissions_set_readonly_false)] // Windows only: it clears the attribute
        permissions.set_readonly(false);
        std::fs::set_permissions(&a, permissions).unwrap();
        // Writable again: the file takes it and the store entry goes.
        assert_eq!(tags.write(&a, &blue()).unwrap(), Where::File);
        assert_eq!(tags.read(&a).unwrap(), blue());
        tags.flush().unwrap();
        let text = std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap();
        assert_eq!(text.lines().count(), 1, "only the header: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_that_cannot_be_read_is_never_written_over() {
        let dir = test_dir("meta-broken");
        let tags = store(&dir);
        tags.fake_holds(fat);
        file(&dir, "config/tags-store.tsv", "");
        std::fs::write(dir.join("config/tags-store.tsv"), [0xFF, 0xFE, 0x00]).unwrap();
        let a = file(&dir, "fat/a.txt", "x");
        let err = tags.write(&a, &red()).unwrap_err();
        assert!(err.to_string().contains("cannot be read"), "{err}");
        tags.flush().unwrap();
        assert_eq!(std::fs::read(dir.join("config/tags-store.tsv")).unwrap(), [0xFF, 0xFE, 0x00]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writing_the_store_is_atomic_and_keeps_lines_it_does_not_know() {
        let dir = test_dir("meta-atomic");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let old = "# gezik tags store 1\nsomething from a later version\n";
        file(&dir, "config/tags-store.tsv", old);
        let a = file(&dir, "fat/a.txt", "x");
        tags.write(&a, &red()).unwrap();
        // Until the job ends, the file is as it was (a crash now loses only this change).
        assert_eq!(std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap(), old);
        tags.flush().unwrap();
        let text = std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap();
        assert!(text.contains("something from a later version\n") && text.contains("tag 6 Red"), "{text}");
        let leftovers: Vec<_> =
            std::fs::read_dir(dir.join("config")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers.len(), 1, "no temporary file left: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_store_follows_renames_folders_moves_and_deletes() {
        let dir = test_dir("meta-follow");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let a = file(&dir, "fat/a.txt", "a");
        let b = file(&dir, "fat/sub/b.txt", "b");
        let c = file(&dir, "fat/c.txt", "c");
        for path in [&a, &b, &c] {
            tags.write(path, &red()).unwrap();
        }
        // A rename and a folder moved, as the engine reports them once done.
        let a2 = dir.join("fat/a2.txt");
        std::fs::rename(&a, &a2).unwrap();
        std::fs::rename(dir.join("fat/sub"), dir.join("fat/moved")).unwrap();
        let facts = gezik_core::ops::conflict::Facts::default();
        tags.follow(&[
            &Outcome::Moved { from: a.clone(), to: a2.clone(), facts },
            &Outcome::Moved { from: dir.join("fat/sub"), to: dir.join("fat/moved"), facts },
        ]);
        assert_eq!(tags.read(&a2).unwrap(), red());
        assert_eq!(tags.read(&dir.join("fat/moved/b.txt")).unwrap(), red());
        // Trashed: the entry stays where restoring brings the file back.
        tags.follow(&[&Outcome::Trashed { original: c.clone(), trashed: dir.join("bin/c.txt") }]);
        assert_eq!(tags.read(&c).unwrap(), red());
        tags.follow(&[&Outcome::Deleted { path: c.clone() }]);
        assert_eq!(tags.read(&c).unwrap(), Meta::default());
        tags.flush().unwrap();
        let text = std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap();
        assert_eq!(text.lines().count(), 3, "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn entries_whose_files_are_gone_are_kept() {
        let dir = test_dir("meta-gone");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let a = file(&dir, "fat/a.txt", "a");
        tags.write(&a, &red()).unwrap();
        tags.flush().unwrap();
        // Gone behind Gezik's back (a USB stick unplugged looks the same).
        std::fs::remove_file(&a).unwrap();
        let again = store(&dir);
        assert!(again.read(&a).is_err());
        again.follow(&[&Outcome::Placed { path: dir.join("fat/x") }]);
        again.flush().unwrap();
        assert!(std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap().contains("fat/a.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reused_path_does_not_show_the_old_files_tags() {
        let dir = test_dir("meta-reused");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let a = file(&dir, "fat/a.txt", "short");
        tags.write(&a, &red()).unwrap();
        // Edited in place (same size): still its tags.
        std::fs::write(&a, "SHORT").unwrap();
        assert_eq!(tags.read(&a).unwrap(), red());
        // Another file under the old name: other size, other time.
        std::fs::remove_file(&a).unwrap();
        std::fs::write(&a, "a much longer and different file").unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        std::fs::File::options().write(true).open(&a).unwrap().set_modified(old).unwrap();
        assert_eq!(tags.read(&a).unwrap(), Meta::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_carries_tags_between_records_and_the_store() {
        let dir = test_dir("meta-carry");
        let tags = store(&dir);
        tags.fake_holds(fat);
        // Own record → a volume without records: into the store.
        let a = file(&dir, "a.txt", "a");
        assert_eq!(tags.write(&a, &red()).unwrap(), Where::File);
        let on_fat = file(&dir, "fat/a.txt", "a");
        tags.carry(&a, &on_fat).unwrap();
        assert_eq!(tags.own_reads(), 1);
        assert_eq!(tags.read(&on_fat).unwrap(), red());
        // Store → a volume with records: into the copy's own record.
        let back = file(&dir, "back.txt", "a");
        tags.carry(&on_fat, &back).unwrap();
        assert_eq!(gezik_platform::meta::read(&back).unwrap().meta, red());
        // Between volumes alike, nothing is read: the system took the record along.
        let plain = file(&dir, "plain.txt", "p");
        tags.carry(&plain, &dir.join("back.txt")).unwrap();
        let on_fat_too = file(&dir, "fat/b.txt", "b");
        tags.carry(&on_fat_too, &dir.join("fat/c.txt")).unwrap();
        assert_eq!(tags.own_reads(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_only_never_writes_into_files() {
        let dir = test_dir("meta-store-only");
        let tags = MetaStore::new(Some(dir.join("config/tags-store.tsv")), true);
        let a = file(&dir, "a.txt", "x");
        assert_eq!(tags.write(&a, &red()).unwrap(), Where::Store(None));
        assert_eq!(gezik_platform::meta::read(&a).unwrap().meta, Meta::default());
        assert_eq!(tags.read(&a).unwrap(), red());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real drive without records (Linux checklist 251: a vfat USB stick):
    /// `GEZIK_TEST_TAG_DIR=/media/$USER/STICK cargo test -p gezik-ops meta -- --ignored`. The
    /// store is made, holds the file, and follows Gezik's rename.
    #[test]
    #[ignore]
    fn the_store_on_a_drive_without_records() {
        let Some(target) = std::env::var_os("GEZIK_TEST_TAG_DIR").map(PathBuf::from) else { return };
        let dir = test_dir("meta-real-fat");
        let tags = store(&dir);
        let (a, b) = (target.join("gezik-tag-a.txt"), target.join("gezik-tag-b.txt"));
        let _ = std::fs::remove_file(&b);
        std::fs::write(&a, "x").unwrap();
        assert!(matches!(tags.write(&a, &red()).unwrap(), Where::Store(_)), "this drive holds records");
        std::fs::rename(&a, &b).unwrap();
        let facts = gezik_core::ops::conflict::Facts::default();
        tags.follow(&[&Outcome::Moved { from: a.clone(), to: b.clone(), facts }]);
        tags.flush().unwrap();
        assert!(std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap().contains("gezik-tag-b.txt"));
        assert_eq!(store(&dir).read(&b).unwrap(), red());
        let _ = std::fs::remove_file(&b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drive_without_records_is_not_asked_for_them() {
        let dir = test_dir("meta-no-ask");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let a = file(&dir, "fat/a.txt", "x");
        assert_eq!(tags.read(&a).unwrap(), Meta::default());
        tags.write(&a, &red()).unwrap();
        assert_eq!(tags.read(&a).unwrap(), red());
        assert_eq!(tags.own_reads(), 0, "vfat answers ENOTSUP: nothing is read there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swapped_names_keep_each_files_tags() {
        let dir = test_dir("meta-swap");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let (a, b) = (file(&dir, "fat/a.txt", "a"), file(&dir, "fat/b.txt", "bb"));
        tags.write(&a, &red()).unwrap();
        tags.write(&b, &Meta { rating: 5, ..Meta::default() }).unwrap();
        let t = dir.join("fat/t");
        std::fs::rename(&a, &t).unwrap();
        std::fs::rename(&b, &a).unwrap();
        std::fs::rename(&t, &b).unwrap();
        let facts = gezik_core::ops::conflict::Facts::default();
        // As a rename job reports a swap: each item from its old name to its new one.
        tags.follow(&[
            &Outcome::Moved { from: a.clone(), to: b.clone(), facts },
            &Outcome::Moved { from: b.clone(), to: a.clone(), facts },
        ]);
        assert_eq!(tags.read(&b).unwrap(), red());
        assert_eq!(tags.read(&a).unwrap().rating, 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_moved_to_another_drive_drops_only_its_own_entry() {
        let dir = test_dir("meta-cross");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let folder = dir.join("fat/box");
        let left = file(&dir, "fat/box/left.txt", "x");
        tags.write(&folder, &red()).unwrap();
        tags.write(&left, &red()).unwrap();
        // The folder was made on the other drive, then the move was cancelled: `left.txt` stays.
        tags.follow(&[&Outcome::Created {
            path: dir.join("other/box"),
            facts: Default::default(),
            from: Some(folder.clone()),
        }]);
        assert_eq!(tags.read(&left).unwrap(), red());
        assert_eq!(tags.read(&folder).unwrap(), Meta::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folders_own_record_keeps_the_entries_inside_it() {
        let dir = test_dir("meta-folder");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let folder = dir.join("box");
        let inside = file(&dir, "box/fat.txt", "x");
        assert!(matches!(tags.write(&inside, &red()).unwrap(), Where::Store(_)));
        assert_eq!(tags.write(&folder, &red()).unwrap(), Where::File);
        assert_eq!(tags.read(&inside).unwrap(), red());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_over_a_file_drops_that_files_entry() {
        let dir = test_dir("meta-over");
        let tags = store(&dir);
        tags.fake_holds(fat);
        let (plain, old) = (file(&dir, "plain.txt", "p"), file(&dir, "fat/old.txt", "o"));
        tags.write(&old, &red()).unwrap();
        // `plain.txt` (no tags) copied over `old.txt`.
        std::fs::copy(&plain, &old).unwrap();
        tags.carry(&plain, &old).unwrap();
        assert_eq!(tags.read(&old).unwrap(), Meta::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_paths_in_a_volume_use_slashes() {
        let root = if cfg!(windows) { Path::new(r"C:\") } else { Path::new("/mnt/usb") };
        let path = root.join("a").join("b.txt");
        assert_eq!(in_volume(&path, root), "a/b.txt");
    }
}
