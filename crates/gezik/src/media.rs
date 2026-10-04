//! Type names, icons and thumbnails: loaded off the UI thread, cached on it.
//!
//! Two worker threads: one for type names and icons (fast), one for thumbnails (a video's
//! can take seconds). Requests are made while Slint builds a line on screen. The newest
//! are served first and only the newest [`MAX_QUEUED`] are kept, so lines scrolled past
//! long ago are dropped. Showing another folder starts a new generation: older requests
//! are dropped and their results only fill the caches.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::SystemTime;

use gezik_platform::{IconTarget, Rgba};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

pub const MAX_QUEUED: usize = 256;
pub const THUMBNAIL_BUDGET: usize = 32 * 1024 * 1024;
pub const PATH_ICON_BUDGET: usize = 8 * 1024 * 1024;
pub const MAX_PATH_ICONS: usize = 512;
/// What a cached "nothing here" counts as, so failures cannot pile up for free.
const MISSING_COST: usize = 64;
/// Folders found to have no custom icon; forgotten wholesale beyond this many.
const MAX_PLAIN_FOLDERS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MediaKey {
    /// `ext` lowercase; folders have their own name.
    TypeName {
        ext: String,
        is_dir: bool,
    },
    ExtIcon {
        ext: String,
        px: u32,
    },
    GenericFolder {
        px: u32,
    },
    /// A folder whose icon may be customized (`desktop.ini`); the worker checks.
    FolderIcon {
        path: PathBuf,
        px: u32,
    },
    /// A file with its own icon (programs, shortcuts), or a drive.
    PathIcon {
        path: PathBuf,
        px: u32,
    },
    Thumbnail {
        path: PathBuf,
        modified: Option<SystemTime>,
        px: u32,
    },
}

impl MediaKey {
    fn slow(&self) -> bool {
        matches!(self, MediaKey::Thumbnail { .. })
    }
}

/// What a worker found. Pictures are converted on the worker; the UI thread only wraps them.
pub enum Outcome {
    Text(String),
    Picture(SharedPixelBuffer<Rgba8Pixel>),
    /// A folder without a custom icon: the generic folder icon applies.
    PlainFolder,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ready {
    Picture,
    TypeName,
}

fn buffer(rgba: Rgba) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let expected = rgba.width as usize * rgba.height as usize * 4;
    (rgba.width > 0 && rgba.pixels.len() == expected)
        .then(|| SharedPixelBuffer::clone_from_slice(&rgba.pixels, rgba.width, rgba.height))
}

/// Runs on a worker thread.
fn run(key: &MediaKey) -> Outcome {
    let picture = |rgba: Option<Rgba>| rgba.and_then(buffer).map_or(Outcome::Nothing, Outcome::Picture);
    match key {
        MediaKey::TypeName { ext, is_dir } => {
            gezik_platform::type_name(ext, *is_dir).map_or(Outcome::Nothing, Outcome::Text)
        }
        MediaKey::ExtIcon { ext, px } => picture(gezik_platform::icon(&IconTarget::Extension(ext.clone()), *px)),
        MediaKey::GenericFolder { px } => picture(gezik_platform::icon(&IconTarget::Folder, *px)),
        MediaKey::FolderIcon { path, px } => {
            if path.join("desktop.ini").is_file() {
                picture(gezik_platform::icon(&IconTarget::Path(path.clone()), *px))
            } else {
                Outcome::PlainFolder
            }
        }
        MediaKey::PathIcon { path, px } => picture(gezik_platform::icon(&IconTarget::Path(path.clone()), *px)),
        MediaKey::Thumbnail { path, px, .. } => picture(gezik_platform::thumbnail(path, *px)),
    }
}

/// A worker's jobs: newest first, at most [`MAX_QUEUED`].
#[derive(Default)]
pub struct Queue {
    jobs: Mutex<VecDeque<(u64, MediaKey)>>,
    ready: Condvar,
}

impl Queue {
    /// Adds a job; returns the oldest one if it had to make room.
    pub fn push(&self, generation: u64, key: MediaKey) -> Option<MediaKey> {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        jobs.push_back((generation, key));
        let dropped = (jobs.len() > MAX_QUEUED).then(|| jobs.pop_front()).flatten().map(|(_, key)| key);
        self.ready.notify_one();
        dropped
    }

    pub fn clear(&self) {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    /// The newest job, waiting for one if there is none.
    pub fn pop(&self) -> (u64, MediaKey) {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(job) = jobs.pop_back() {
                return job;
            }
            jobs = self.ready.wait(jobs).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A cache bounded by bytes and entries; the least recently used goes first.
pub struct ByteLru<K, V> {
    map: HashMap<K, (V, usize, u64)>,
    bytes: usize,
    budget: usize,
    max_entries: usize,
    clock: u64,
}

impl<K: Eq + Hash + Clone, V: Clone> ByteLru<K, V> {
    pub fn new(budget: usize, max_entries: usize) -> Self {
        ByteLru { map: HashMap::new(), bytes: 0, budget, max_entries, clock: 0 }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        self.clock += 1;
        let clock = self.clock;
        let (value, _, used) = self.map.get_mut(key)?;
        *used = clock;
        Some(value.clone())
    }

    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        self.clock += 1;
        if let Some((_, old, _)) = self.map.insert(key, (value, bytes, self.clock)) {
            self.bytes -= old;
        }
        self.bytes += bytes;
        while (self.bytes > self.budget || self.map.len() > self.max_entries) && self.map.len() > 1 {
            let Some(oldest) = self.map.iter().min_by_key(|(_, (_, _, used))| *used).map(|(k, _)| k.clone()) else {
                break;
            };
            if let Some((_, bytes, _)) = self.map.remove(&oldest) {
                self.bytes -= bytes;
            }
        }
    }

    #[allow(dead_code, reason = "inspected by tests")]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    #[allow(dead_code, reason = "inspected by tests")]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[allow(dead_code, reason = "inspected by tests")]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

type ReadyListener = Rc<dyn Fn(&[usize], Ready)>;

struct Inner {
    fast: Arc<Queue>,
    slow: Arc<Queue>,
    /// Workers are started on the first request (`idle` never starts them: tests).
    started: Cell<bool>,
    generation: Arc<AtomicU64>,
    /// `None`: the system has no name for it.
    type_names: RefCell<HashMap<(String, bool), Option<String>>>,
    /// Type and generic folder icons: few, so kept for good. `None`: no icon.
    shared: RefCell<HashMap<MediaKey, Option<Image>>>,
    paths: RefCell<ByteLru<MediaKey, Option<Image>>>,
    thumbnails: RefCell<ByteLru<MediaKey, Option<Image>>>,
    plain_folders: RefCell<HashSet<PathBuf>>,
    pending: RefCell<HashSet<MediaKey>>,
    /// Entries (indexes in the current listing) to redraw when a key arrives.
    waiting: RefCell<HashMap<MediaKey, Vec<usize>>>,
    on_ready: RefCell<Option<ReadyListener>>,
}

#[derive(Clone)]
pub struct Media(Rc<Inner>);

thread_local! {
    /// The media of this (UI) thread, so worker results can reach it.
    static CURRENT: RefCell<Option<Media>> = const { RefCell::new(None) };
}

pub fn with_current(f: impl FnOnce(&Media)) {
    if let Some(media) = CURRENT.with(|c| c.borrow().clone()) {
        f(&media);
    }
}

impl Default for Media {
    fn default() -> Self {
        Media::new()
    }
}

impl Media {
    pub fn new() -> Media {
        Media(Rc::new(Inner {
            fast: Arc::default(),
            slow: Arc::default(),
            started: Cell::new(false),
            generation: Arc::default(),
            type_names: RefCell::default(),
            shared: RefCell::default(),
            paths: RefCell::new(ByteLru::new(PATH_ICON_BUDGET, MAX_PATH_ICONS)),
            thumbnails: RefCell::new(ByteLru::new(THUMBNAIL_BUDGET, usize::MAX)),
            plain_folders: RefCell::default(),
            pending: RefCell::default(),
            waiting: RefCell::default(),
            on_ready: RefCell::new(None),
        }))
    }

    /// Never starts workers: requests just queue up (for tests).
    #[cfg(test)]
    pub fn idle() -> Media {
        let media = Media::new();
        media.0.started.set(true);
        media
    }

    /// Makes worker results reach this media. Call once, on the UI thread.
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
    }

    /// Called with the entries to redraw (and what arrived) when results come in. A type
    /// name also calls it with no entries, so a list sorted by type can sort again.
    pub fn on_ready(&self, f: impl Fn(&[usize], Ready) + 'static) {
        *self.0.on_ready.borrow_mut() = Some(Rc::new(f));
    }

    pub fn generation(&self) -> u64 {
        self.0.generation.load(Ordering::SeqCst)
    }

    /// Another listing is shown: drops the requests made for the old one.
    pub fn new_generation(&self) {
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        self.0.fast.clear();
        self.0.slow.clear();
        self.0.pending.borrow_mut().clear();
        self.0.waiting.borrow_mut().clear();
    }

    /// The system's name for a type (`ext` lowercase), once known. While it loads, `None`
    /// (entry `entry`, if any, is redrawn when it arrives); also `None` if there is none.
    pub fn type_name(&self, ext: &str, is_dir: bool, entry: Option<usize>) -> Option<String> {
        if let Some(found) = self.0.type_names.borrow().get(&(ext.to_owned(), is_dir)) {
            return found.clone();
        }
        self.request(MediaKey::TypeName { ext: ext.to_owned(), is_dir }, entry);
        None
    }

    /// Only what is already known; asks for nothing.
    pub fn known_type_name(&self, ext: &str, is_dir: bool) -> Option<String> {
        self.0.type_names.borrow().get(&(ext.to_owned(), is_dir)).cloned().flatten()
    }

    /// The picture for `key`, if loaded; otherwise it is requested and entry `entry` is
    /// redrawn when it arrives.
    pub fn picture(&self, key: MediaKey, entry: usize) -> Option<Image> {
        if let MediaKey::FolderIcon { path, px } = &key
            && self.0.plain_folders.borrow().contains(path)
        {
            return self.picture(MediaKey::GenericFolder { px: *px }, entry);
        }
        if let Some(found) = self.cached(&key) {
            return found;
        }
        self.request(key, Some(entry));
        None
    }

    fn cached(&self, key: &MediaKey) -> Option<Option<Image>> {
        match key {
            MediaKey::ExtIcon { .. } | MediaKey::GenericFolder { .. } => self.0.shared.borrow().get(key).cloned(),
            MediaKey::FolderIcon { .. } | MediaKey::PathIcon { .. } => self.0.paths.borrow_mut().get(key),
            MediaKey::Thumbnail { .. } => self.0.thumbnails.borrow_mut().get(key),
            MediaKey::TypeName { .. } => None,
        }
    }

    fn store(&self, key: MediaKey, picture: Option<Image>, bytes: usize) {
        match key {
            MediaKey::ExtIcon { .. } | MediaKey::GenericFolder { .. } => {
                self.0.shared.borrow_mut().insert(key, picture);
            }
            MediaKey::FolderIcon { .. } | MediaKey::PathIcon { .. } => {
                self.0.paths.borrow_mut().insert(key, picture, bytes)
            }
            MediaKey::Thumbnail { .. } => self.0.thumbnails.borrow_mut().insert(key, picture, bytes),
            MediaKey::TypeName { .. } => {}
        }
    }

    fn request(&self, key: MediaKey, entry: Option<usize>) {
        if let Some(entry) = entry {
            let mut waiting = self.0.waiting.borrow_mut();
            let entries = waiting.entry(key.clone()).or_default();
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
        if !self.0.pending.borrow_mut().insert(key.clone()) {
            return;
        }
        self.start();
        let queue = if key.slow() { &self.0.slow } else { &self.0.fast };
        if let Some(dropped) = queue.push(self.generation(), key) {
            self.0.pending.borrow_mut().remove(&dropped);
            self.0.waiting.borrow_mut().remove(&dropped);
        }
    }

    fn start(&self) {
        if self.0.started.replace(true) {
            return;
        }
        for (name, queue) in [("gezik-icons", &self.0.fast), ("gezik-thumbnails", &self.0.slow)] {
            let (queue, current) = (queue.clone(), self.0.generation.clone());
            let spawned = std::thread::Builder::new().name(name.to_owned()).spawn(move || {
                gezik_platform::init_thread();
                loop {
                    let (generation, key) = queue.pop();
                    if generation != current.load(Ordering::SeqCst) {
                        continue;
                    }
                    let outcome = run(&key);
                    let _ = slint::invoke_from_event_loop(move || {
                        with_current(|media| media.finish(generation, key, outcome));
                    });
                }
            });
            if let Err(err) = spawned {
                eprintln!("gezik: cannot start the {name} thread: {err}");
            }
        }
    }

    /// A worker's result, on the UI thread. Results of an older generation only fill the
    /// caches.
    pub fn finish(&self, generation: u64, key: MediaKey, outcome: Outcome) {
        let current = generation == self.generation();
        if current {
            self.0.pending.borrow_mut().remove(&key);
        }
        let ready = match (&key, outcome) {
            (MediaKey::TypeName { ext, is_dir }, outcome) => {
                let name = match outcome {
                    Outcome::Text(name) => Some(name),
                    _ => None,
                };
                self.0.type_names.borrow_mut().insert((ext.clone(), *is_dir), name);
                Ready::TypeName
            }
            (MediaKey::FolderIcon { path, .. }, Outcome::PlainFolder) => {
                let mut plain = self.0.plain_folders.borrow_mut();
                if plain.len() >= MAX_PLAIN_FOLDERS {
                    plain.clear();
                }
                plain.insert(path.clone());
                Ready::Picture
            }
            (_, Outcome::Picture(buffer)) => {
                let bytes = buffer.width() as usize * buffer.height() as usize * 4;
                self.store(key.clone(), Some(Image::from_rgba8(buffer)), bytes);
                Ready::Picture
            }
            (_, _) => {
                self.store(key.clone(), None, MISSING_COST);
                Ready::Picture
            }
        };
        if !current {
            return;
        }
        let entries = self.0.waiting.borrow_mut().remove(&key).unwrap_or_default();
        if entries.is_empty() && ready != Ready::TypeName {
            return;
        }
        let listener = self.0.on_ready.borrow().clone();
        if let Some(f) = listener {
            f(&entries, ready);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(ext: &str) -> MediaKey {
        MediaKey::ExtIcon { ext: ext.to_owned(), px: 16 }
    }

    fn pixels() -> Outcome {
        Outcome::Picture(SharedPixelBuffer::new(2, 2))
    }

    type Seen = Vec<(Vec<usize>, Ready)>;

    /// Entries the listener was told about, in order.
    fn recorder(media: &Media) -> Rc<RefCell<Seen>> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        media.on_ready(move |entries, ready| sink.borrow_mut().push((entries.to_vec(), ready)));
        seen
    }

    #[test]
    fn queue_serves_newest_first_and_drops_the_oldest() {
        let queue = Queue::default();
        for i in 0..MAX_QUEUED {
            assert_eq!(queue.push(1, icon(&i.to_string())), None);
        }
        assert_eq!(queue.push(1, icon("new")), Some(icon("0")));
        assert_eq!(queue.pop(), (1, icon("new")));
        assert_eq!(queue.pop(), (1, icon(&(MAX_QUEUED - 1).to_string())));
    }

    #[test]
    fn byte_lru_evicts_least_recently_used() {
        let mut lru = ByteLru::new(100, 10);
        lru.insert("a", 1, 40);
        lru.insert("b", 2, 40);
        assert_eq!(lru.get(&"a"), Some(1));
        lru.insert("c", 3, 40);
        assert_eq!((lru.get(&"b"), lru.get(&"a"), lru.get(&"c")), (None, Some(1), Some(3)));
        assert_eq!(lru.bytes(), 80);
        let mut few = ByteLru::new(usize::MAX, 2);
        few.insert(1, (), 1);
        few.insert(2, (), 1);
        few.insert(3, (), 1);
        assert_eq!(few.len(), 2);
    }

    #[test]
    fn results_redraw_the_entries_that_asked() {
        let media = Media::idle();
        let seen = recorder(&media);
        assert!(media.picture(icon("txt"), 3).is_none());
        assert!(media.picture(icon("txt"), 7).is_none());
        media.finish(media.generation(), icon("txt"), pixels());
        assert_eq!(*seen.borrow(), [(vec![3, 7], Ready::Picture)]);
        assert!(media.picture(icon("txt"), 9).is_some(), "now cached");
    }

    #[test]
    fn stale_generation_results_are_dropped() {
        let media = Media::idle();
        let seen = recorder(&media);
        let old = media.generation();
        media.picture(icon("png"), 4);
        media.new_generation();
        media.finish(old, icon("png"), pixels());
        assert!(seen.borrow().is_empty(), "the old listing's entries are not redrawn");
        assert!(media.picture(icon("png"), 1).is_some(), "but the icon is kept");
    }

    #[test]
    fn missing_pictures_are_not_asked_again() {
        let media = Media::idle();
        media.picture(icon("zzz"), 0);
        media.finish(media.generation(), icon("zzz"), Outcome::Nothing);
        assert!(media.picture(icon("zzz"), 0).is_none());
        assert!(media.0.pending.borrow().is_empty());
    }

    #[test]
    fn plain_folders_use_the_generic_folder_icon() {
        let media = Media::idle();
        let folder = MediaKey::FolderIcon { path: PathBuf::from("/x/sub"), px: 16 };
        media.picture(folder.clone(), 2);
        media.finish(media.generation(), folder.clone(), Outcome::PlainFolder);
        media.finish(media.generation(), MediaKey::GenericFolder { px: 16 }, pixels());
        assert!(media.picture(folder, 2).is_some());
    }

    #[test]
    fn type_names_are_cached_and_announced() {
        let media = Media::idle();
        let seen = recorder(&media);
        assert_eq!(media.type_name("txt", false, None), None);
        media.finish(
            media.generation(),
            MediaKey::TypeName { ext: "txt".into(), is_dir: false },
            Outcome::Text("Text Document".into()),
        );
        assert_eq!(*seen.borrow(), [(vec![], Ready::TypeName)]);
        assert_eq!(media.type_name("txt", false, Some(1)).as_deref(), Some("Text Document"));
        assert_eq!(media.known_type_name("txt", false).as_deref(), Some("Text Document"));
        assert_eq!(media.known_type_name("md", false), None);
    }
}
