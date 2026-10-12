//! Finding duplicate files (spec 11 §9.2). The scanner's walk keeps the regular files the
//! search lets through (no folder, link, empty file, or file only in the cloud); files of a
//! size no other has drop out; the rest are told apart by the SHA-256 of their first 64 KB,
//! then by their whole content (two files compared byte by byte, more by SHA-256); hard
//! links of one file count once. Reads run on low-priority threads; a cancel keeps the
//! groups whose files were read through.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use gezik_core::Entry;
use gezik_core::ops::threads::DiskKind;
use gezik_ops::hash::{self, Algo};
use gezik_platform::fs::DirItem;

use crate::query::Query;
use crate::results::{Batch, relative_key};
use crate::run::{BATCH_EVERY, Event, Running, Summary};
use crate::walk::{Visit, Walk};

/// The first bytes read of every file of a shared size; a file this small is read whole.
pub const PREFIX: u64 = 64 * 1024;

/// What the walk keeps of a file (one allocation, its name).
struct File {
    folder: u32,
    name: Box<str>,
    flags: u8,
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
}

impl File {
    fn entry(&self) -> Entry {
        Entry {
            name: self.name.to_string(),
            is_dir: false,
            flags: self.flags,
            size: self.size,
            modified: self.modified,
            created: self.created,
        }
    }
}

type Sink = dyn Fn(Event) + Send + Sync;

#[derive(Default)]
struct Kept {
    folders: Vec<Box<str>>,
    files: Vec<File>,
    last: Option<Instant>,
}

/// Keeps the files on the walk's threads.
struct Collect {
    query: Query,
    cancel: Arc<AtomicBool>,
    kept: Mutex<Kept>,
    scanned: AtomicUsize,
    online: AtomicUsize,
    read: Arc<AtomicUsize>,
    sink: Arc<Sink>,
}

impl Visit for Collect {
    fn wants_meta(&self, name: &str, is_dir: bool) -> bool {
        !is_dir && self.query.passes_name(name, false)
    }

    fn folder(&self, dir: &Path, relative: &str, items: Vec<DirItem>) {
        let mut mine = Vec::new();
        for item in items {
            if !item.is_file
                || item.is_link
                || item.size == 0
                || !self.query.passes(&item.name, false, item.size, item.modified)
                || !self.query.passes_fields(&item.name, false, |want| {
                    crate::query::field_reader(&dir.join(&item.name), &self.cancel)(want)
                })
            {
                continue;
            }
            // Reading it would download it (spec 11 §3.2): counted, not read.
            if item.offline {
                self.online.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            mine.push(item);
        }
        if mine.is_empty() || self.cancel.load(Ordering::Relaxed) {
            return;
        }
        let scanned = self.scanned.fetch_add(mine.len(), Ordering::Relaxed) + mine.len();
        let mut kept = self.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let folder = kept.folders.len() as u32;
        kept.folders.push(relative.into());
        kept.files.extend(mine.into_iter().map(|item| File {
            folder,
            name: item.name.into_boxed_str(),
            flags: item.flags,
            size: item.size,
            modified: item.modified,
            created: item.created,
        }));
        if kept.last.is_none_or(|last| last.elapsed() >= BATCH_EVERY) {
            kept.last = Some(Instant::now());
            drop(kept);
            (self.sink)(Event::Progress { found: scanned, folders: self.read.load(Ordering::Relaxed) });
        }
    }
}

/// Threads that read files at once on drives of `kinds` (spec 11 §3.2): an SSD 4, a network
/// folder 2, a spinning or unknown disk 1; the slowest decides.
pub fn readers(kinds: &[DiskKind]) -> usize {
    kinds
        .iter()
        .map(|kind| match kind {
            DiskKind::Ssd => 4,
            DiskKind::Network => 2,
            DiskKind::Hdd | DiskKind::Unknown => 1,
        })
        .min()
        .unwrap_or(1)
}

/// Runs `job(i)` for `0..count` on `threads` low-priority threads; `None` where it gave none
/// or `cancel` came first.
fn par_map<T: Send>(
    count: usize,
    threads: usize,
    cancel: &AtomicBool,
    job: &(dyn Fn(usize) -> Option<T> + Sync),
) -> Vec<Option<T>> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, T)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, count.max(1)) {
            scope.spawn(|| {
                gezik_platform::priority::lower_this_thread();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= count || cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(value) = job(i) {
                        done.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((i, value));
                    }
                }
            });
        }
    });
    let mut out: Vec<Option<T>> = (0..count).map(|_| None).collect();
    for (i, value) in done.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
        out[i] = Some(value);
    }
    out
}

/// The path of a file kept under `root` (empty: folders are whole paths).
fn path_of(root: &Path, folders: &[Box<str>], file: &File) -> PathBuf {
    let folder = &*folders[file.folder as usize];
    if folder.is_empty() { root.join(&*file.name) } else { root.join(folder).join(&*file.name) }
}

/// A file's drive and id (`fs::file_id`), to tell hard links of one file.
type FileId = (u64, u128);

/// The groups of `files` (indices) whose content is the same, read on `threads` threads;
/// `id_of` tells hard links of one file. Returns the groups, most bytes to free first, and the
/// hard links left out.
fn confirm(
    root: &Path,
    folders: &[Box<str>],
    files: &[File],
    threads: usize,
    cancel: &AtomicBool,
    id_of: &(dyn Fn(&Path) -> Option<FileId> + Sync),
) -> (Vec<Vec<usize>>, usize) {
    let path = |i: usize| path_of(root, folders, &files[i]);
    let go_on = |_: u64| !cancel.load(Ordering::Relaxed);
    // The first bytes of each.
    let prefixes = par_map(files.len(), threads, cancel, &|i| hash::hash_prefix(&path(i), PREFIX).ok());
    let mut by_prefix: HashMap<(u64, [u8; 32]), Vec<usize>> = HashMap::new();
    for (i, prefix) in prefixes.into_iter().enumerate() {
        // As many bytes as its size says (all of a small file): one that changed since the walk
        // is left out, so a small file's first bytes are surely all of it.
        if let Some((prefix, read)) = prefix
            && read == files[i].size.min(PREFIX)
        {
            by_prefix.entry((files[i].size, prefix)).or_default().push(i);
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    // The rest: a pair compared byte by byte, more each by its SHA-256.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut whole: Vec<Vec<usize>> = Vec::new();
    for (_, group) in by_prefix.into_iter().filter(|(_, g)| g.len() > 1) {
        match group.as_slice() {
            _ if files[group[0]].size <= PREFIX => groups.push(group),
            [a, b] => pairs.push((*a, *b)),
            _ => whole.push(group),
        }
    }
    let same = par_map(pairs.len(), threads, cancel, &|p| {
        let (a, b) = pairs[p];
        hash::same_content(&path(a), &path(b), false, &mut go_on.clone()).ok()
    });
    groups.extend(pairs.iter().zip(same).filter(|(_, same)| *same == Some(None)).map(|((a, b), _)| vec![*a, *b]));
    let members: Vec<usize> = whole.iter().flatten().copied().collect();
    let digests = par_map(members.len(), threads, cancel, &|m| {
        let digests = hash::hash_file(&path(members[m]), &[Algo::Sha256], &mut go_on.clone()).ok()?;
        digests.get(Algo::Sha256).map(str::to_owned)
    });
    let mut by_digest: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, digest) in members.into_iter().zip(digests) {
        if let Some(digest) = digest {
            by_digest.entry(digest).or_default().push(i);
        }
    }
    groups.extend(by_digest.into_values().filter(|g| g.len() > 1));
    // Hard links of one file: the first stays. A file whose id cannot be read could be another
    // name of a file in the group (a subst drive, a share of this disk): left out, never offered.
    let mut links = 0;
    for group in &mut groups {
        let mut seen = Vec::new();
        group.retain(|&i| match id_of(&path(i)) {
            Some(id) if seen.contains(&id) => {
                links += 1;
                false
            }
            Some(id) => {
                seen.push(id);
                true
            }
            None => false,
        });
    }
    groups.retain(|g| g.len() > 1);
    let freed = |g: &Vec<usize>| files[g[0]].size.saturating_mul(g.len() as u64 - 1);
    let key = |i: usize| relative_key(&folders[files[i].folder as usize], &files[i].name);
    for group in &mut groups {
        group.sort_by_cached_key(|&i| key(i));
    }
    groups.sort_by(|a, b| freed(b).cmp(&freed(a)).then(files[b[0]].size.cmp(&files[a[0]].size)).then(a.cmp(b)));
    (groups, links)
}

/// Finds the duplicates under `walk` that `query` lets through, reading on `readers`
/// threads; `root`: the results' scope (empty for every drive). `sink` hears the walk's
/// progress, one `Comparing`, the groups as one batch, and the summary. Returns when done.
pub fn find(walk: Walk, query: Query, root: &Path, readers: usize, running: &Running, sink: Arc<Sink>) {
    let started = Instant::now();
    let cancel = running.flag();
    let collect = Arc::new(Collect {
        query,
        cancel: cancel.clone(),
        kept: Mutex::default(),
        scanned: AtomicUsize::new(0),
        online: AtomicUsize::new(0),
        read: walk.read.clone(),
        sink: sink.clone(),
    });
    let stats = crate::walk::run(walk, cancel.clone(), collect.clone());
    let online = collect.online.load(Ordering::Relaxed);
    let scanned = collect.scanned.load(Ordering::Relaxed);
    let kept = std::mem::take(&mut *collect.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    drop(collect);
    let summary = Summary {
        folders: stats.folders,
        skipped: stats.skipped,
        problems: stats.problems,
        online,
        ..Summary::default()
    };
    if cancel.load(Ordering::Relaxed) {
        return sink(Event::Done(Summary { cancelled: true, elapsed: started.elapsed(), ..summary }));
    }
    // Files of a size no other has go: most do, and their memory with them.
    let Kept { folders, files, .. } = kept;
    let mut count: HashMap<u64, u32> = HashMap::new();
    for file in &files {
        *count.entry(file.size).or_default() += 1;
    }
    let files: Vec<File> = files.into_iter().filter(|f| count[&f.size] > 1).collect();
    drop(count);
    let bytes = files.iter().map(|f| f.size).sum();
    sink(Event::Comparing { scanned, files: files.len(), bytes });
    let (groups, hard_links) = confirm(root, &folders, &files, readers, &cancel, &gezik_platform::fs::file_id);
    // The batch: only the folders its rows are in.
    let mut batch = Batch::default();
    let mut renumbered: HashMap<u32, u32> = HashMap::new();
    for (id, group) in groups.iter().enumerate() {
        for &i in group {
            let file = &files[i];
            let next = renumbered.len() as u32;
            let parent = *renumbered.entry(file.folder).or_insert_with(|| {
                batch.folders.push(folders[file.folder as usize].clone());
                next
            });
            batch.entries.push(file.entry());
            batch.parent.push(parent);
            batch.matches.push(None);
            batch.dups.push(id as u32);
        }
    }
    let found = batch.entries.len();
    if !batch.is_empty() {
        sink(Event::Batch(batch));
    }
    let cancelled = cancel.load(Ordering::Relaxed);
    sink(Event::Done(Summary { found, cancelled, hard_links, elapsed: started.elapsed(), ..summary }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::QueryOptions;
    use crate::results::ResultSet;
    use crate::walk::WalkRules;
    use gezik_core::search::{Scope, SearchSpec};

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-dups-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, name: &str, data: &[u8]) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    /// `len` bytes: `seed` repeated, then `tail` at the end.
    fn data(len: usize, seed: u8, tail: u8) -> Vec<u8> {
        let mut out = vec![seed; len];
        if let Some(last) = out.last_mut() {
            *last = tail;
        }
        out
    }

    fn run(root: &Path, spec: SearchSpec, cancel: bool) -> (ResultSet, Summary, Vec<Event>) {
        let query = Query::compile(&spec, &QueryOptions::local(1 << 20, 1000)).unwrap();
        let walk = Walk::new(vec![root.to_path_buf()], false, 2, WalkRules::new(None, &[], Vec::new()));
        let running = Running::default();
        if cancel {
            running.cancel();
        }
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        find(walk, query, root, 2, &running, Arc::new(move |e| sink.lock().unwrap().push(e)));
        let events = std::mem::take(&mut *events.lock().unwrap());
        let mut set = ResultSet::duplicates(root.to_path_buf());
        let mut summary = None;
        let mut others = Vec::new();
        for event in events {
            match event {
                Event::Batch(batch) => set.append(batch),
                Event::Done(s) => summary = Some(s),
                other => others.push(other),
            }
        }
        (set, summary.expect("a summary"), others)
    }

    fn spec(root: &Path) -> SearchSpec {
        SearchSpec { duplicates: true, ..SearchSpec::new(Scope::Folder(root.to_path_buf())) }
    }

    /// The groups as sorted lists of paths under the root, in their order.
    fn groups(set: &ResultSet) -> Vec<Vec<String>> {
        let mut out: Vec<(u32, Vec<String>)> = Vec::new();
        for i in 0..set.len() {
            let id = set.dup_group(i).unwrap();
            let key = set.key_at(i).unwrap().replace('\\', "/");
            match out.iter_mut().find(|(g, _)| *g == id) {
                Some((_, keys)) => keys.push(key),
                None => out.push((id, vec![key])),
            }
        }
        out.sort_by_key(|(id, _)| *id);
        out.into_iter().map(|(_, keys)| keys).collect()
    }

    #[test]
    fn size_then_first_bytes_then_content() {
        let root = tree("stages");
        let big = 3 * PREFIX as usize;
        // Three the same (SHA-256 of the whole), two the same (byte by byte).
        for name in ["a/big1", "b/big2", "c/big3"] {
            write(&root, name, &data(big, 1, 9));
        }
        write(&root, "pair1", &data(big + 1, 2, 2));
        write(&root, "x/pair2", &data(big + 1, 2, 2));
        // Same size, different first bytes; same first 64 KB, different after it.
        write(&root, "first-differs", &data(big, 7, 9));
        write(&root, "tail-differs", &data(big, 1, 8));
        // Small ones: the first bytes are all of them.
        write(&root, "s1.txt", b"hello");
        write(&root, "s2.txt", b"hello");
        write(&root, "s3.txt", b"world");
        // A size no other has.
        write(&root, "alone", &data(10_000, 3, 3));
        let (set, summary, events) = run(&root, spec(&root), false);
        assert_eq!(
            groups(&set),
            [vec!["a/big1", "b/big2", "c/big3"], vec!["pair1", "x/pair2"], vec!["s1.txt", "s2.txt"],],
            "most bytes to free first; the near misses are left out"
        );
        assert_eq!(set.dup_counts(), (3, 7, 2 * big as u64 + big as u64 + 1 + 5));
        assert_eq!(summary.found, 7);
        assert!(!summary.cancelled && summary.hard_links == 0);
        let comparing = events.iter().find_map(|e| match e {
            Event::Comparing { scanned, files, .. } => Some((*scanned, *files)),
            _ => None,
        });
        assert_eq!(comparing, Some((11, 10)), "only the one of its size is not read");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_files_hard_links_and_filters() {
        let root = tree("links");
        write(&root, "empty1", b"");
        write(&root, "empty2", b"");
        write(&root, "file.bin", b"same bytes");
        std::fs::hard_link(root.join("file.bin"), root.join("link.bin")).unwrap();
        write(&root, "photo.jpg", b"picture");
        write(&root, "copy.jpg", b"picture");
        write(&root, "copy.txt", b"picture");
        let (set, summary, _) = run(&root, spec(&root), false);
        assert_eq!(
            groups(&set),
            [vec!["copy.jpg", "copy.txt", "photo.jpg"]],
            "empty files are no duplicates; a hard link is the same file"
        );
        assert_eq!(summary.hard_links, 1);
        let mut jpg = spec(&root);
        jpg.pattern = "*.jpg".into();
        let (set, _, _) = run(&root, jpg, false);
        assert_eq!(groups(&set), [vec!["copy.jpg", "photo.jpg"]], "the name filter applies");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn links_are_not_followed() {
        let root = tree("symlinks");
        let outside = tree("symlinks-outside");
        write(&outside, "target.txt", b"one content");
        write(&root, "real.txt", b"one content");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(outside.join("target.txt"), root.join("link.txt")).is_ok()
            && std::os::unix::fs::symlink(&outside, root.join("dir")).is_ok();
        // Windows needs developer mode or administrator rights for symbolic links.
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(outside.join("target.txt"), root.join("link.txt")).is_ok()
            && std::os::windows::fs::symlink_dir(&outside, root.join("dir")).is_ok();
        if made {
            let (set, _, _) = run(&root, spec(&root), false);
            assert!(groups(&set).is_empty(), "neither link is read: {:?}", groups(&set));
        }
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn a_cancel_stops_it_without_groups() {
        let root = tree("cancel");
        write(&root, "a", b"same");
        write(&root, "b", b"same");
        let (set, summary, events) = run(&root, spec(&root), true);
        assert!(set.is_empty() && summary.cancelled);
        assert!(!events.iter().any(|e| matches!(e, Event::Comparing { .. })), "nothing is read");
        // A cancel while reading: no read is started, so no group is sure.
        let files = [
            File { folder: 0, name: "a".into(), flags: 0, size: 4, modified: None, created: None },
            File { folder: 0, name: "b".into(), flags: 0, size: 4, modified: None, created: None },
        ];
        let id = &gezik_platform::fs::file_id;
        let (groups, _) = confirm(&root, &["".into()], &files, 2, &AtomicBool::new(true), id);
        assert!(groups.is_empty());
        let (groups, _) = confirm(&root, &["".into()], &files, 2, &AtomicBool::new(false), id);
        assert_eq!(groups, [vec![0, 1]]);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn file(name: &str, size: u64) -> File {
        File { folder: 0, name: name.into(), flags: 0, size, modified: None, created: None }
    }

    #[test]
    fn a_file_that_changed_since_the_walk_is_left_out() {
        let root = tree("changed");
        write(&root, "a", b"hello");
        write(&root, "b", b"hello");
        // The walk saw 10 bytes; 5 are there now: their first bytes are not all of them.
        let files = [file("a", 10), file("b", 10)];
        let (groups, _) =
            confirm(&root, &["".into()], &files, 2, &AtomicBool::new(false), &gezik_platform::fs::file_id);
        assert!(groups.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_without_an_id_is_never_offered() {
        let root = tree("no-id");
        for name in ["a", "b", "c"] {
            write(&root, name, b"same");
        }
        let files = [file("a", 4), file("b", 4), file("c", 4)];
        let no_id_for_c = |path: &Path| {
            (path.file_name() != Some(std::ffi::OsStr::new("c"))).then(|| gezik_platform::fs::file_id(path)).flatten()
        };
        let (groups, _) = confirm(&root, &["".into()], &files, 2, &AtomicBool::new(false), &no_id_for_c);
        assert_eq!(groups, [vec![0, 1]], "c could be another name of a or b");
        let only_a = |path: &Path| {
            (path.file_name() == Some(std::ffi::OsStr::new("a"))).then(|| gezik_platform::fs::file_id(path)).flatten()
        };
        let (groups, _) = confirm(&root, &["".into()], &files, 2, &AtomicBool::new(false), &only_a);
        assert!(groups.is_empty(), "one sure file is no group");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn readers_follow_the_slowest_disk() {
        assert_eq!(readers(&[DiskKind::Ssd]), 4);
        assert_eq!(readers(&[DiskKind::Ssd, DiskKind::Network]), 2);
        assert_eq!(readers(&[DiskKind::Hdd, DiskKind::Ssd]), 1);
        assert_eq!(readers(&[]), 1);
    }
}
