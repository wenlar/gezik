//! Type names, icons and thumbnails: loaded off the UI thread, cached on it.
//!
//! Two worker threads, each started with its first request: one for type names and icons
//! (fast), one for thumbnails (a video's can take seconds). Requests are made while Slint builds a line on screen. The newest
//! are served first and only the newest [`MAX_QUEUED`] are kept, so lines scrolled past
//! long ago are dropped. Showing another folder starts a new generation: older requests
//! are dropped and their results only fill the caches. On macOS a running Quick Look
//! request is cancelled when another listing is shown.
//!
//! One per process (spec 10 §3.3): caches, queues and workers are shared, while each view
//! holds its own client (`client`) with its entries waiting, its listener and its generations.
//! A request is dropped only once no client waits for it any more.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// A folder whose icon may be its own (`desktop.ini`; on macOS a custom icon or a volume's
    /// root); the worker checks.
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
fn run(key: &MediaKey, wanted: &dyn Fn() -> bool) -> Outcome {
    let picture = |rgba: Option<Rgba>| rgba.and_then(buffer).map_or(Outcome::Nothing, Outcome::Picture);
    match key {
        MediaKey::TypeName { ext, is_dir } => {
            gezik_platform::type_name(ext, *is_dir).map_or(Outcome::Nothing, Outcome::Text)
        }
        MediaKey::ExtIcon { ext, px } => picture(gezik_platform::icon(&IconTarget::Extension(ext.clone()), *px)),
        MediaKey::GenericFolder { px } => picture(gezik_platform::icon(&IconTarget::Folder, *px)),
        MediaKey::FolderIcon { path, px } => {
            if gezik_platform::folder_has_own_icon(path) {
                picture(gezik_platform::icon(&IconTarget::Path(path.clone()), *px))
            } else {
                Outcome::PlainFolder
            }
        }
        MediaKey::PathIcon { path, px } => picture(gezik_platform::icon(&IconTarget::Path(path.clone()), *px)),
        MediaKey::Thumbnail { path, px, .. } => picture(gezik_platform::thumbnail_while(path, *px, wanted)),
    }
}

/// Whether a job is still wanted: cleared once no client waits for it (a new generation).
type Wanted = Arc<AtomicBool>;

/// A worker's jobs: newest first, at most [`MAX_QUEUED`].
#[derive(Default)]
pub struct Queue {
    jobs: Mutex<VecDeque<(Wanted, MediaKey)>>,
    ready: Condvar,
}

impl Queue {
    /// Adds a job; returns the oldest one if it had to make room.
    pub fn push(&self, wanted: Wanted, key: MediaKey) -> Option<MediaKey> {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        jobs.push_back((wanted, key));
        let dropped = (jobs.len() > MAX_QUEUED).then(|| jobs.pop_front()).flatten().map(|(_, key)| key);
        self.ready.notify_one();
        dropped
    }

    /// Drops the jobs no longer wanted.
    pub fn retain_wanted(&self) {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner).retain(|(wanted, _)| wanted.load(Ordering::SeqCst));
    }

    /// The newest job, waiting for one if there is none.
    pub fn pop(&self) -> (Wanted, MediaKey) {
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
    /// Each worker is started on its first request (`idle` never starts them: tests).
    fast_started: Cell<bool>,
    slow_started: Cell<bool>,
    /// `None`: the system has no name for it.
    type_names: RefCell<HashMap<(String, bool), Option<String>>>,
    /// Type and generic folder icons: few, so kept for good. `None`: no icon.
    shared: RefCell<HashMap<MediaKey, Option<Image>>>,
    paths: RefCell<ByteLru<MediaKey, Option<Image>>>,
    thumbnails: RefCell<ByteLru<MediaKey, Option<Image>>>,
    plain_folders: RefCell<HashSet<PathBuf>>,
    /// The keys queued or loading, each with its job's flag.
    pending: RefCell<HashMap<MediaKey, Wanted>>,
    clients: RefCell<Vec<Weak<Client>>>,
}

/// What belongs to one view.
#[derive(Default)]
struct Client {
    /// The keys it asked for, with its entries (indexes in its current listing) to redraw
    /// when one arrives.
    waiting: RefCell<HashMap<MediaKey, Vec<usize>>>,
    on_ready: RefCell<Option<ReadyListener>>,
}

/// One view's handle on the process's media.
#[derive(Clone)]
pub struct Media {
    shared: Rc<Inner>,
    client: Rc<Client>,
}

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
        let media = Media {
            shared: Rc::new(Inner {
                fast: Arc::default(),
                slow: Arc::default(),
                fast_started: Cell::new(false),
                slow_started: Cell::new(false),
                type_names: RefCell::default(),
                shared: RefCell::default(),
                paths: RefCell::new(ByteLru::new(PATH_ICON_BUDGET, MAX_PATH_ICONS)),
                thumbnails: RefCell::new(ByteLru::new(THUMBNAIL_BUDGET, usize::MAX)),
                plain_folders: RefCell::default(),
                pending: RefCell::default(),
                clients: RefCell::default(),
            }),
            client: Rc::default(),
        };
        media.shared.clients.borrow_mut().push(Rc::downgrade(&media.client));
        media
    }

    /// A handle for a view: the same caches and workers, its own entries and listener.
    pub fn client(&self) -> Media {
        let client = Rc::<Client>::default();
        let mut clients = self.shared.clients.borrow_mut();
        clients.retain(|c| c.strong_count() > 0);
        clients.push(Rc::downgrade(&client));
        Media { shared: self.shared.clone(), client }
    }

    /// Never starts workers: requests just queue up (for tests).
    #[cfg(test)]
    pub fn idle() -> Media {
        let media = Media::new();
        media.shared.fast_started.set(true);
        media.shared.slow_started.set(true);
        media
    }

    /// Makes worker results reach this media. Call once, on the UI thread.
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
    }

    /// Called with the entries to redraw (and what arrived) when results come in. A type
    /// name also calls it with no entries, so a list sorted by type can sort again.
    pub fn on_ready(&self, f: impl Fn(&[usize], Ready) + 'static) {
        *self.client.on_ready.borrow_mut() = Some(Rc::new(f));
    }

    fn clients(&self) -> Vec<Rc<Client>> {
        self.shared.clients.borrow().iter().filter_map(Weak::upgrade).collect()
    }

    /// Another listing is shown: drops the requests made for the old one, unless another
    /// client waits for them too.
    pub fn new_generation(&self) {
        let keys: Vec<MediaKey> = self.client.waiting.borrow_mut().drain().map(|(key, _)| key).collect();
        let clients = self.clients();
        {
            let mut pending = self.shared.pending.borrow_mut();
            for key in keys {
                if !clients.iter().any(|c| c.waiting.borrow().contains_key(&key))
                    && let Some(wanted) = pending.remove(&key)
                {
                    wanted.store(false, Ordering::SeqCst);
                }
            }
        }
        self.shared.fast.retain_wanted();
        self.shared.slow.retain_wanted();
    }

    /// The system's name for a type (`ext` lowercase), once known. While it loads, `None`
    /// (entry `entry`, if any, is redrawn when it arrives); also `None` if there is none.
    pub fn type_name(&self, ext: &str, is_dir: bool, entry: Option<usize>) -> Option<String> {
        if let Some(found) = self.shared.type_names.borrow().get(&(ext.to_owned(), is_dir)) {
            return found.clone();
        }
        self.request(MediaKey::TypeName { ext: ext.to_owned(), is_dir }, entry);
        None
    }

    /// Only what is already known; asks for nothing.
    pub fn known_type_name(&self, ext: &str, is_dir: bool) -> Option<String> {
        self.shared.type_names.borrow().get(&(ext.to_owned(), is_dir)).cloned().flatten()
    }

    /// The picture for `key`, if loaded; otherwise it is requested and entry `entry` is
    /// redrawn when it arrives.
    pub fn picture(&self, key: MediaKey, entry: usize) -> Option<Image> {
        if let MediaKey::FolderIcon { path, px } = &key
            && self.shared.plain_folders.borrow().contains(path)
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
            MediaKey::ExtIcon { .. } | MediaKey::GenericFolder { .. } => self.shared.shared.borrow().get(key).cloned(),
            MediaKey::FolderIcon { .. } | MediaKey::PathIcon { .. } => self.shared.paths.borrow_mut().get(key),
            MediaKey::Thumbnail { .. } => self.shared.thumbnails.borrow_mut().get(key),
            MediaKey::TypeName { .. } => None,
        }
    }

    fn store(&self, key: MediaKey, picture: Option<Image>, bytes: usize) {
        match key {
            MediaKey::ExtIcon { .. } | MediaKey::GenericFolder { .. } => {
                self.shared.shared.borrow_mut().insert(key, picture);
            }
            MediaKey::FolderIcon { .. } | MediaKey::PathIcon { .. } => {
                self.shared.paths.borrow_mut().insert(key, picture, bytes)
            }
            MediaKey::Thumbnail { .. } => self.shared.thumbnails.borrow_mut().insert(key, picture, bytes),
            MediaKey::TypeName { .. } => {}
        }
    }

    fn request(&self, key: MediaKey, entry: Option<usize>) {
        {
            // Kept even without an entry: the key is this client's until its next generation.
            let mut waiting = self.client.waiting.borrow_mut();
            let entries = waiting.entry(key.clone()).or_default();
            if let Some(entry) = entry
                && !entries.contains(&entry)
            {
                entries.push(entry);
            }
        }
        let wanted = Wanted::new(AtomicBool::new(true));
        {
            let mut pending = self.shared.pending.borrow_mut();
            if pending.contains_key(&key) {
                return;
            }
            pending.insert(key.clone(), wanted.clone());
        }
        self.start(key.slow());
        let queue = if key.slow() { &self.shared.slow } else { &self.shared.fast };
        if let Some(dropped) = queue.push(wanted, key) {
            self.shared.pending.borrow_mut().remove(&dropped);
            for client in self.clients() {
                client.waiting.borrow_mut().remove(&dropped);
            }
        }
    }

    /// Starts the thumbnail worker (`slow`) or the icon worker, unless it runs already.
    fn start(&self, slow: bool) {
        let (started, name, queue) = if slow {
            (&self.shared.slow_started, "gezik-thumbnails", &self.shared.slow)
        } else {
            (&self.shared.fast_started, "gezik-icons", &self.shared.fast)
        };
        if started.replace(true) {
            return;
        }
        let queue = queue.clone();
        let spawned = std::thread::Builder::new().name(name.to_owned()).spawn(move || {
            gezik_platform::init_thread();
            loop {
                let (wanted, key) = queue.pop();
                if !wanted.load(Ordering::SeqCst) {
                    continue;
                }
                // Only a wanted request is waited for: another folder drops a Quick Look request.
                let outcome = run(&key, &|| wanted.load(Ordering::SeqCst));
                let _ = slint::invoke_from_event_loop(move || {
                    with_current(|media| media.finish(&wanted, key, outcome));
                });
            }
        });
        if let Err(err) = spawned {
            eprintln!("gezik: cannot start the {name} thread: {err}");
        }
    }

    /// A worker's result, on the UI thread. Results no longer wanted (an older generation)
    /// only fill the caches, and not with a miss.
    pub fn finish(&self, wanted: &AtomicBool, key: MediaKey, outcome: Outcome) {
        let current = wanted.load(Ordering::SeqCst);
        if current {
            self.shared.pending.borrow_mut().remove(&key);
        }
        let ready = match (&key, outcome) {
            (MediaKey::TypeName { ext, is_dir }, outcome) => {
                let name = match outcome {
                    Outcome::Text(name) => Some(name),
                    _ => None,
                };
                self.shared.type_names.borrow_mut().insert((ext.clone(), *is_dir), name);
                Ready::TypeName
            }
            (MediaKey::FolderIcon { path, .. }, Outcome::PlainFolder) => {
                let mut plain = self.shared.plain_folders.borrow_mut();
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
                // An older generation's miss may be a cancelled Quick Look request, and an
                // iCloud-only file keeps its mtime once downloaded: both are asked again.
                let in_cloud = matches!(&key, MediaKey::Thumbnail { path, .. } if gezik_platform::only_in_cloud(path));
                if current && !in_cloud {
                    self.store(key.clone(), None, MISSING_COST);
                }
                Ready::Picture
            }
        };
        if !current {
            return;
        }
        // Each client that asked, with its own entries; the listeners run with no borrow held.
        let asked: Vec<_> = self
            .clients()
            .into_iter()
            .filter_map(|c| {
                let entries = c.waiting.borrow_mut().remove(&key)?;
                Some((entries, c.on_ready.borrow().clone()))
            })
            .collect();
        for (entries, listener) in asked {
            if entries.is_empty() && ready != Ready::TypeName {
                continue;
            }
            if let Some(f) = listener {
                f(&entries, ready);
            }
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

    /// The flag of `key`'s job; a wanted one if it was not asked for.
    fn job(media: &Media, key: &MediaKey) -> Wanted {
        media.shared.pending.borrow().get(key).cloned().unwrap_or_else(|| Wanted::new(AtomicBool::new(true)))
    }

    /// `key`'s job is done.
    fn done(media: &Media, key: MediaKey, outcome: Outcome) {
        media.finish(&job(media, &key), key, outcome);
    }

    fn queued(media: &Media) -> usize {
        media.shared.fast.jobs.lock().unwrap().len()
    }

    #[test]
    fn the_thumbnail_worker_starts_with_the_first_thumbnail() {
        let media = Media::new();
        assert_eq!(media.picture(icon("txt"), 0), None);
        assert!(media.shared.fast_started.get() && !media.shared.slow_started.get(), "icons only: no thumbnail thread");
        let thumbnail = MediaKey::Thumbnail { path: PathBuf::from("/x/missing.png"), px: 64, modified: None };
        assert_eq!(media.picture(thumbnail, 1), None);
        assert!(media.shared.slow_started.get());
    }

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
            assert_eq!(queue.push(Wanted::default(), icon(&i.to_string())), None);
        }
        assert_eq!(queue.push(Wanted::default(), icon("new")), Some(icon("0")));
        assert_eq!(queue.pop().1, icon("new"));
        assert_eq!(queue.pop().1, icon(&(MAX_QUEUED - 1).to_string()));
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
        done(&media, icon("txt"), pixels());
        assert_eq!(*seen.borrow(), [(vec![3, 7], Ready::Picture)]);
        assert!(media.picture(icon("txt"), 9).is_some(), "now cached");
    }

    #[test]
    fn stale_generation_results_are_dropped() {
        let media = Media::idle();
        let seen = recorder(&media);
        media.picture(icon("png"), 4);
        let old = job(&media, &icon("png"));
        media.new_generation();
        assert_eq!(queued(&media), 0, "its job is dropped");
        media.finish(&old, icon("png"), pixels());
        assert!(seen.borrow().is_empty(), "the old listing's entries are not redrawn");
        assert!(media.picture(icon("png"), 1).is_some(), "but the icon is kept");
    }

    #[test]
    fn an_older_generations_miss_is_asked_again() {
        let media = Media::idle();
        media.picture(icon("zzz"), 0);
        let old = job(&media, &icon("zzz"));
        media.new_generation();
        media.finish(&old, icon("zzz"), Outcome::Nothing);
        assert!(media.picture(icon("zzz"), 0).is_none());
        assert!(media.shared.pending.borrow().contains_key(&icon("zzz")), "requested again, not remembered as missing");
    }

    #[test]
    fn missing_pictures_are_not_asked_again() {
        let media = Media::idle();
        media.picture(icon("zzz"), 0);
        done(&media, icon("zzz"), Outcome::Nothing);
        assert!(media.picture(icon("zzz"), 0).is_none());
        assert!(media.shared.pending.borrow().is_empty());
    }

    #[test]
    fn plain_folders_use_the_generic_folder_icon() {
        let media = Media::idle();
        let folder = MediaKey::FolderIcon { path: PathBuf::from("/x/sub"), px: 16 };
        media.picture(folder.clone(), 2);
        done(&media, folder.clone(), Outcome::PlainFolder);
        done(&media, MediaKey::GenericFolder { px: 16 }, pixels());
        assert!(media.picture(folder, 2).is_some());
    }

    #[test]
    fn type_names_are_cached_and_announced() {
        let media = Media::idle();
        let seen = recorder(&media);
        assert_eq!(media.type_name("txt", false, None), None);
        done(&media, MediaKey::TypeName { ext: "txt".into(), is_dir: false }, Outcome::Text("Text Document".into()));
        assert_eq!(*seen.borrow(), [(vec![], Ready::TypeName)]);
        assert_eq!(media.type_name("txt", false, Some(1)).as_deref(), Some("Text Document"));
        assert_eq!(media.known_type_name("txt", false).as_deref(), Some("Text Document"));
        assert_eq!(media.known_type_name("md", false), None);
    }

    #[test]
    fn a_new_generation_keeps_the_other_clients_requests() {
        let left = Media::idle();
        let right = left.client();
        let (seen_left, seen_right) = (recorder(&left), recorder(&right));
        left.picture(icon("png"), 4);
        right.picture(icon("jpg"), 2);
        let png = job(&left, &icon("png"));
        left.new_generation();
        assert!(!png.load(Ordering::SeqCst), "its own request is dropped");
        assert_eq!(queued(&left), 1, "the other's is kept");
        done(&right, icon("jpg"), pixels());
        assert_eq!(*seen_right.borrow(), [(vec![2], Ready::Picture)]);
        assert!(seen_left.borrow().is_empty());
    }

    #[test]
    fn a_key_asked_by_two_clients_is_loaded_once_for_both() {
        let left = Media::idle();
        let right = left.client();
        let (seen_left, seen_right) = (recorder(&left), recorder(&right));
        left.picture(icon("txt"), 3);
        right.picture(icon("txt"), 7);
        assert_eq!(queued(&left), 1);
        done(&left, icon("txt"), pixels());
        assert_eq!(*seen_left.borrow(), [(vec![3], Ready::Picture)]);
        assert_eq!(*seen_right.borrow(), [(vec![7], Ready::Picture)]);
    }

    #[test]
    fn a_shared_key_survives_one_clients_new_generation() {
        let left = Media::idle();
        let right = left.client();
        let (seen_left, seen_right) = (recorder(&left), recorder(&right));
        left.picture(icon("txt"), 3);
        right.picture(icon("txt"), 7);
        left.new_generation();
        assert!(job(&left, &icon("txt")).load(Ordering::SeqCst), "the right one still waits for it");
        assert_eq!(queued(&left), 1);
        done(&right, icon("txt"), pixels());
        assert!(seen_left.borrow().is_empty());
        assert_eq!(*seen_right.borrow(), [(vec![7], Ready::Picture)]);
    }
}
