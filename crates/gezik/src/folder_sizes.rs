//! Folder sizes (spec 6): the folders of the folder shown get their totals from memory, from
//! Everything or from a low-priority walk (2 threads, 1 on a network folder), those on screen
//! first; a list sorted by size sorts again as they come, at most once a second.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use gezik_config::settings::FolderSizeMode;
use gezik_core::Entry;
use gezik_core::nav::Location;
use gezik_core::ops::paths::is_within;
use gezik_core::pattern::fold_text;
use gezik_platform::fs::DirItem;
use gezik_search::size::{CACHE_MAX, FolderTotal, Known, SizeCache, measure};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::view::View;

/// Results are sent at most this often (one view update each).
const BATCH_EVERY: Duration = Duration::from_millis(100);
/// Sorting again by size: at most this often, and not this soon after a scroll (spec 6.3).
const RESORT_EVERY: Duration = Duration::from_secs(1);
/// Larger folders sort again only once every size is in (sapma 9).
const RESORT_MAX_ENTRIES: usize = 50_000;

thread_local! {
    static CURRENT: RefCell<Option<FolderSizes>> = const { RefCell::new(None) };
}

pub fn with_current(f: impl FnOnce(&FolderSizes)) {
    if let Some(sizes) = CURRENT.with(|c| c.borrow().clone()) {
        f(&sizes);
    }
}

/// The files and folders under `path`, if its size is known (the preview).
pub fn counts(path: &Path) -> Option<(u64, u64)> {
    CURRENT.with(|c| c.borrow().as_ref().and_then(|s| s.0.cache.borrow().get(path)).and_then(|t| t.counts))
}

/// Whether the folder shown gets its folders' sizes (spec 6.1); `explicit`: Calculate folder sizes.
pub fn sizes_wanted(mode: FolderSizeMode, network: bool, explicit: bool) -> bool {
    explicit
        || match mode {
            FolderSizeMode::Off => false,
            FolderSizeMode::Local => !network,
            FolderSizeMode::All => true,
        }
}

/// Whether a folder of the folder shown is added up: never a link (its target is elsewhere) nor
/// a cloud folder (listing it would fill it from the cloud), sapma 13.
pub fn sizable(item: &DirItem) -> bool {
    item.is_dir && !item.is_link && !item.offline
}

/// Whether results of run `run` still count (`now`: the run going on).
fn current_run(run: u64, now: u64) -> bool {
    run == now
}

/// When to sort a size-sorted list again (spec 6.3, sapma 9).
#[derive(Default)]
pub struct ResortClock {
    last: Option<Instant>,
    scroll: Option<f32>,
    scrolled: Option<Instant>,
}

impl ResortClock {
    /// Sizes came at `now` with the list at `scroll`; `busy`: a drag or rubber band is on;
    /// `done`: every size is in (the last sort, not held back by the rate). How long to wait.
    pub fn due(&mut self, now: Instant, scroll: f32, busy: bool, done: bool) -> Duration {
        if self.scroll.is_some_and(|s| s != scroll) {
            self.scrolled = Some(now);
        }
        self.scroll = Some(scroll);
        let after =
            |at: Option<Instant>| at.map_or(Duration::ZERO, |at| (at + RESORT_EVERY).saturating_duration_since(now));
        let rate = if done { Duration::ZERO } else { after(self.last) };
        let busy = if busy { RESORT_EVERY } else { Duration::ZERO };
        rate.max(after(self.scrolled)).max(busy)
    }

    pub fn sorted(&mut self, now: Instant) {
        self.last = Some(now);
    }
}

/// What the worker sends: sizes by name (`None`: not sized, a link, a cloud folder or a folder
/// not worked out here), whether it is done, and the network root it found the folder on (sapma 14).
struct Arrival {
    run: u64,
    folder: PathBuf,
    sizes: Vec<(String, Option<SystemTime>, Option<FolderTotal>)>,
    done: bool,
    network_root: Option<PathBuf>,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    view: View,
    cache: RefCell<SizeCache>,
    mode: Cell<FolderSizeMode>,
    everything: Cell<bool>,
    /// The run going on: its number, folder and flag.
    run: Cell<u64>,
    running: RefCell<Option<(PathBuf, Arc<AtomicBool>)>>,
    /// More was marked while a run for the same folder went on: run again after it.
    again: Cell<bool>,
    /// Roots found to be network drives (`folder-sizes = "local"` leaves them alone).
    network_roots: RefCell<Vec<PathBuf>>,
    clock: RefCell<ResortClock>,
    resort: slint::Timer,
    /// Every size of the current run is in.
    done: Cell<bool>,
}

#[derive(Clone)]
pub struct FolderSizes(Rc<Inner>);

impl FolderSizes {
    pub fn new(window: &AppWindow, view: View) -> FolderSizes {
        let sizes = FolderSizes(Rc::new(Inner {
            window: window.as_weak(),
            view,
            cache: RefCell::new(SizeCache::new(CACHE_MAX)),
            mode: Cell::new(FolderSizeMode::default()),
            everything: Cell::new(true),
            run: Cell::new(0),
            running: RefCell::default(),
            again: Cell::new(false),
            network_roots: RefCell::default(),
            clock: RefCell::default(),
            resort: slint::Timer::default(),
            done: Cell::new(true),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(sizes.clone()));
        sizes
    }

    /// settings.toml was read: `[view] folder-sizes` and `[search] everything`.
    pub fn set_settings(&self, mode: FolderSizeMode, everything: bool) {
        self.0.mode.set(mode);
        self.0.everything.set(everything);
    }

    fn known_network(&self, folder: &Path) -> bool {
        self.0.network_roots.borrow().iter().any(|root| is_within(folder, root))
    }

    /// `folder` was just listed: its folders get what is known of them, and the others are
    /// marked as on their way where sizes come by themselves (before the first sort, so they
    /// sort last). No file system access.
    pub fn apply_known(&self, folder: &Path, entries: &mut [Entry]) {
        let auto = sizes_wanted(self.0.mode.get(), self.known_network(folder), false);
        let now = Instant::now();
        let mut cache = self.0.cache.borrow_mut();
        for entry in entries.iter_mut().filter(|e| e.is_dir) {
            entry.flags &= !Entry::SIZE_FLAGS;
            match cache.look(&folder.join(&entry.name), entry.modified, now) {
                Known::Fresh(total) => set_total(entry, total, 0),
                // Not worked out again where sizes do not come by themselves: shown as it is.
                Known::Stale(total) => set_total(entry, total, if auto { Entry::SIZE_STALE } else { 0 }),
                Known::Unknown if auto => entry.flags |= Entry::SIZE_PENDING,
                Known::Unknown => {}
            }
        }
    }

    /// A location is on screen: a run for another folder stops (its finished sizes stay in
    /// the cache); the folders on their way here are worked out.
    pub fn shown(&self, location: &Location) {
        let folder = match location {
            Location::Path(path) => Some(path.clone()),
            _ => None,
        };
        let same = matches!((&*self.0.running.borrow(), &folder), (Some((running, _)), Some(f)) if running == f);
        if same {
            self.0.again.set(true);
            return;
        }
        self.stop();
        if folder.is_some() {
            self.start(false);
        }
    }

    fn stop(&self) {
        if let Some((_, cancel)) = self.0.running.borrow_mut().take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.0.run.set(self.0.run.get() + 1);
        self.0.resort.stop();
    }

    /// `calculate-folder-sizes`: the selected folders, else every folder shown, on any drive.
    pub fn calculate(&self) {
        let selected = self.0.view.selected_folder_names();
        let names = (!selected.is_empty()).then_some(selected.as_slice());
        if !self.0.view.mark_size_pending(names) {
            return self.0.view.note("Folder sizes are worked out in folders".to_owned());
        }
        self.stop();
        self.start(true);
    }

    /// A job changed `paths`: their folders' totals and every folder's above are out of date.
    pub fn forget(&self, paths: &[PathBuf]) {
        self.0.cache.borrow_mut().forget_with_ancestors(paths);
    }

    /// F5 on `folder`.
    pub fn forget_children(&self, folder: &Path) {
        self.0.cache.borrow_mut().forget_children(folder);
    }

    /// Starts a run on the folders the view has on their way; nothing if none.
    fn start(&self, explicit: bool) {
        let Some((folder, names)) = self.0.view.folders_to_size() else { return };
        if names.is_empty() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let run = self.0.run.get() + 1;
        self.0.run.set(run);
        self.0.again.set(false);
        self.0.done.set(false);
        *self.0.running.borrow_mut() = Some((folder.clone(), cancel.clone()));
        let (mode, everything, window) = (self.0.mode.get(), self.0.everything.get(), self.0.window.clone());
        let spawned = std::thread::Builder::new().name("gezik-sizes".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            work(run, folder, names, mode, everything, explicit, &cancel, &|arrival| {
                let _ = window.upgrade_in_event_loop(move |_| with_current(|s| s.arrived(arrival)));
            });
            // What the walk read and freed (up to ~30 MB on a home folder) goes back (spec 12).
            gezik_platform::priority::give_back_memory();
        });
        if spawned.is_err() {
            self.0.running.borrow_mut().take();
        }
    }

    fn arrived(&self, arrival: Arrival) {
        if !current_run(arrival.run, self.0.run.get()) {
            return;
        }
        if let Some(root) = arrival.network_root {
            self.0.network_roots.borrow_mut().push(root);
        }
        let now = Instant::now();
        let updates: Vec<(String, u64, u8)> = {
            let mut cache = self.0.cache.borrow_mut();
            arrival
                .sizes
                .into_iter()
                .map(|(name, stamp, total)| {
                    let path = arrival.folder.join(&name);
                    match total {
                        Some(total) => {
                            cache.put(&path, total, stamp, now);
                            (name, total.bytes, size_bits(total))
                        }
                        // Not worked out here (a network folder under "local"): an old total stays shown.
                        None => match cache.get(&path) {
                            Some(total) => (name, total.bytes, size_bits(total)),
                            None => (name, 0, 0),
                        },
                    }
                })
                .collect()
        };
        let changed = self.0.view.set_folder_sizes(&arrival.folder, &updates);
        if arrival.done {
            self.0.running.borrow_mut().take();
            self.0.done.set(true);
        }
        if changed && self.0.view.sorted_by_size() {
            self.resort_soon();
        }
        if arrival.done && self.0.again.replace(false) {
            self.start(false);
        }
    }

    /// Sorts again when the clock allows (spec 6.3): never under a drag, a rubber band or a
    /// press (`list_busy`); large folders only once every size is in.
    fn resort_soon(&self) {
        let done = self.0.done.get();
        if !done && self.0.view.len_full() > RESORT_MAX_ENTRIES {
            return;
        }
        let view = &self.0.view;
        let wait = self.0.clock.borrow_mut().due(Instant::now(), view.list_scroll(), view.list_busy(), done);
        if wait.is_zero() {
            self.0.resort.stop();
            view.resort_in_place();
            self.0.clock.borrow_mut().sorted(Instant::now());
        } else if !self.0.resort.running() {
            let this = self.clone();
            self.0.resort.start(slint::TimerMode::SingleShot, wait, move || this.resort_soon());
        }
    }
}

fn size_bits(total: FolderTotal) -> u8 {
    Entry::SIZED | if total.partial { Entry::SIZE_PARTIAL } else { 0 }
}

fn set_total(entry: &mut Entry, total: FolderTotal, extra: u8) {
    entry.size = total.bytes;
    entry.flags |= size_bits(total) | extra;
}

/// The run's work, off the UI thread: Everything's sizes where it keeps them, the other folders
/// added up in turn, results sent every `BATCH_EVERY`. Stops as soon as `cancel` is set.
#[allow(clippy::too_many_arguments)]
fn work(
    run: u64,
    folder: PathBuf,
    names: Vec<(String, Option<SystemTime>)>,
    mode: FolderSizeMode,
    everything: bool,
    explicit: bool,
    cancel: &Arc<AtomicBool>,
    send: &dyn Fn(Arrival),
) {
    let arrival = |sizes, done, network_root| Arrival { run, folder: folder.clone(), sizes, done, network_root };
    let network = gezik_platform::fs::is_network(&folder).unwrap_or(false);
    if !sizes_wanted(mode, network, explicit) {
        // A network folder under "local": nothing is pending there any more (sapma 14).
        let root = gezik_platform::fs::drive_root(&folder);
        return send(arrival(names.into_iter().map(|(n, s)| (n, s, None)).collect(), true, root));
    }
    // Which folders are links or cloud folders: one read of the folder itself.
    let items = gezik_platform::fs::read_dir_items(&folder, &|_, _| false).unwrap_or_default();
    let kind: HashMap<&str, bool> = items.iter().filter(|i| i.is_dir).map(|i| (i.name.as_str(), sizable(i))).collect();
    // A folder Everything gave no size for is added up by the walk.
    let indexed: HashMap<String, u64> = gezik_search::everything::folder_sizes(&folder, everything, cancel)
        .map(|found| found.into_iter().map(|(n, b)| (fold_text(&n), b)).collect())
        .unwrap_or_default();
    let threads = if network { 1 } else { 2 };
    let mut batch = Vec::new();
    let mut sent = Instant::now();
    for (name, stamp) in names {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let total = if !kind.get(name.as_str()).copied().unwrap_or(false) {
            None
        } else if let Some(&bytes) = indexed.get(&fold_text(&name)) {
            Some(FolderTotal { bytes, counts: None, partial: false })
        } else {
            // The run's own flag: the walk looks at it before every folder it reads.
            match measure(&folder.join(&name), threads, cancel) {
                Some(total) => Some(total),
                None => return,
            }
        };
        batch.push((name, stamp, total));
        if sent.elapsed() >= BATCH_EVERY {
            send(arrival(std::mem::take(&mut batch), false, None));
            sent = Instant::now();
        }
    }
    send(arrival(batch, true, None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_come_by_themselves_only_where_the_setting_says() {
        use FolderSizeMode::*;
        assert!(sizes_wanted(Local, false, false));
        assert!(!sizes_wanted(Local, true, false), "not on a network folder");
        assert!(sizes_wanted(All, true, false));
        assert!(!sizes_wanted(Off, false, false));
        assert!(sizes_wanted(Off, true, true), "Calculate folder sizes works everywhere");
    }

    fn dir_item(is_link: bool, offline: bool) -> DirItem {
        DirItem {
            name: "d".into(),
            is_dir: true,
            is_link,
            is_file: false,
            offline,
            flags: 0,
            size: 0,
            modified: None,
            created: None,
            device: 0,
            has_meta: false,
        }
    }

    #[test]
    fn links_and_cloud_folders_are_not_sized() {
        assert!(sizable(&dir_item(false, false)));
        assert!(!sizable(&dir_item(true, false)), "a junction or symlink: its target is not added up");
        assert!(!sizable(&dir_item(false, true)), "a OneDrive folder in the cloud is not filled");
    }

    #[test]
    fn sorting_again_waits_a_second_and_for_the_scrolling_to_stop() {
        let start = Instant::now();
        let mut clock = ResortClock::default();
        assert_eq!(clock.due(start, 0.0, false, false), Duration::ZERO, "the first sizes sort at once");
        clock.sorted(start);
        let soon = start + Duration::from_millis(300);
        assert_eq!(clock.due(soon, 0.0, false, false), Duration::from_millis(700), "at most once a second");
        let later = start + Duration::from_millis(1500);
        assert_eq!(clock.due(later, -52.0, false, false), Duration::from_secs(1), "just scrolled: a second more");
        assert_eq!(clock.due(later + Duration::from_secs(1), -52.0, false, false), Duration::ZERO);
        assert_eq!(clock.due(later, -52.0, true, true), Duration::from_secs(1), "never under a drag");
    }

    #[test]
    fn the_last_sort_still_waits_for_the_scrolling() {
        let start = Instant::now();
        let mut clock = ResortClock::default();
        clock.sorted(start);
        let soon = start + Duration::from_millis(200);
        assert_eq!(clock.due(soon, 0.0, false, true), Duration::ZERO, "all sizes in: no rate limit");
        assert_eq!(clock.due(soon, -26.0, false, true), Duration::from_secs(1));
    }

    #[test]
    fn an_overtaken_run_is_dropped() {
        assert!(current_run(3, 3));
        assert!(!current_run(2, 3), "sizes of the folder left before");
    }
}
