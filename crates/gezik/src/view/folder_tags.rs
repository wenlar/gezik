//! A folder's tags, ratings and comments read whole (spec 12 §3.4 item 3, §6, §8.2): only while
//! the view sorts by tags or rating, groups by them or filters with `tag:` or `rating:`. One low-priority thread reads every name, says how far
//! it got, and hands the result over once; the list is then sorted, grouped and filtered again,
//! once. Rows not read yet sort last, group as `Reading tags…` and pass the filter, drawn faint.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use gezik_core::Entry;
use gezik_core::group::GroupBy;
use gezik_core::meta::Meta;
use gezik_core::meta_filter::{TagKey, TagTerm, matches, tag_key};
use gezik_core::pattern::Pattern;
use gezik_core::sort::SortKey;

use super::View;
use super::listing::{Listing, filtered_with};

/// Progress is said at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// What the view knows of its folder's tags.
pub struct FolderTags {
    pub dir: PathBuf,
    /// Names with a tag, rating or comment only (a folder of 100,000 untagged files keeps
    /// nothing per file).
    pub tags: HashMap<Box<str>, Meta>,
    /// Every name was read; until then every row is on its way.
    pub complete: bool,
    /// A `tag:` filter waits for the first read: the rows it lets through meanwhile are faint.
    pub faint: Cell<bool>,
    /// The files (names, sizes, times) it was read from: a listing of the same files is not
    /// read again.
    stamp: u64,
}

impl FolderTags {
    /// Row `name`'s meta; `None` while on its way.
    pub fn of(&self, name: &str) -> Option<&Meta> {
        static NONE: std::sync::LazyLock<Meta> = std::sync::LazyLock::new(Meta::default);
        self.complete.then(|| self.tags.get(name).unwrap_or(&NONE))
    }

    #[cfg(test)]
    pub fn read(dir: PathBuf, tags: HashMap<Box<str>, Meta>) -> FolderTags {
        FolderTags { dir, tags, complete: true, faint: Cell::new(false), stamp: 0 }
    }
}

/// The read going on; dropped (a read overtaken, a pane or window closed), it stops.
pub struct TagRun {
    id: u64,
    dir: PathBuf,
    cancel: Arc<AtomicBool>,
    stamp: u64,
    /// The files or their tags changed while it went on: read once more when it is done.
    stale: Cell<bool>,
}

impl Drop for TagRun {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// What the reading thread sends.
pub enum TagArrival {
    Progress { done: usize, total: usize },
    Done(HashMap<Box<str>, Meta>),
}

/// Whether a view needs its folder's meta whole (nothing is read otherwise).
pub fn needed(sort: SortKey, group: GroupBy, filter_has_tags: bool) -> bool {
    matches!(sort, SortKey::Tags | SortKey::Rating)
        || matches!(group, GroupBy::Tag | GroupBy::Rating)
        || filter_has_tags
}

/// The same for the same files in any order (names, sizes, modification times).
fn stamp_of(entries: &[Entry]) -> u64 {
    use std::hash::{Hash, Hasher};
    let one = |e: &Entry| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&e.name, e.size, e.modified).hash(&mut h);
        h.finish()
    };
    entries.iter().fold(entries.len() as u64, |sum, e| sum.wrapping_add(one(e)))
}

#[cfg(test)]
thread_local! {
    /// Reads started (tests: none while nothing needs them).
    pub static STARTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl View {
    fn filter_has_tags(&self) -> bool {
        self.0.filter.borrow().as_ref().is_some_and(|f| !f.tags.is_empty())
    }

    /// Reads the folder shown whole if its sort, grouping or filter needs it; `fresh`: a listing
    /// was shown, read again if its files differ from those read (one read at a time: one going
    /// on reads once more when done). Tags a job changed (`reread_folder_tags`) are read again
    /// even when the files look the same. Not needed: what was read goes.
    pub(super) fn sync_folder_tags(&self, fresh: bool) {
        let dir = match &self.0.data.borrow().listing {
            Listing::Files(dir, _) if !dir.as_os_str().is_empty() => Some(dir.clone()),
            _ => None,
        };
        let filter = self.filter_has_tags();
        let wanted = needed(self.sort().key, self.group_by(), filter);
        let Some(dir) = dir.filter(|_| wanted) else {
            self.stop_tag_read();
            self.0.data.borrow_mut().folder_tags = None;
            self.0.tags_dirty.set(false);
            return;
        };
        let dirty = self.0.tags_dirty.replace(false);
        let (same, complete, read_stamp) = match &self.0.data.borrow().folder_tags {
            Some(f) if f.dir == dir => {
                f.faint.set(filter && !f.complete);
                (true, f.complete, f.stamp)
            }
            _ => (false, false, 0),
        };
        if !same {
            let tags = FolderTags {
                dir: dir.clone(),
                tags: HashMap::new(),
                complete: false,
                faint: Cell::new(filter),
                stamp: 0,
            };
            self.0.data.borrow_mut().folder_tags = Some(Rc::new(tags));
        }
        let stamp = stamp_of(&self.0.data.borrow().full);
        let run_stamp = self.0.tag_run.borrow().as_ref().filter(|run| run.dir == dir).map(|run| run.stamp);
        match run_stamp {
            Some(run_stamp) => {
                if (dirty || (fresh && stamp != run_stamp))
                    && let Some(run) = self.0.tag_run.borrow().as_ref()
                {
                    run.stale.set(true);
                }
            }
            None if !same || !complete || dirty || (fresh && stamp != read_stamp) => self.start_tag_read(dir, stamp),
            None => {}
        }
    }

    /// A tag job ended: a folder read whole is read again.
    pub fn reread_folder_tags(&self) {
        if self.0.data.borrow().folder_tags.is_some() {
            self.0.tags_dirty.set(true);
            self.sync_folder_tags(true);
        }
    }

    /// The same, for a pane of another window: read when it next shows, sorts or filters.
    pub fn folder_tags_changed(&self) {
        if self.0.data.borrow().folder_tags.is_some() {
            self.0.tags_dirty.set(true);
        }
    }

    fn stop_tag_read(&self) {
        // Dropped, it stops.
        self.0.tag_run.borrow_mut().take();
        if self.0.tag_progress.borrow_mut().take().is_some() {
            self.update_status();
        }
    }

    fn start_tag_read(&self, dir: PathBuf, stamp: u64) {
        self.stop_tag_read();
        #[cfg(test)]
        STARTED.with(|s| s.set(s.get() + 1));
        let id = self.0.tag_runs.get() + 1;
        self.0.tag_runs.set(id);
        let cancel = Arc::new(AtomicBool::new(false));
        let run = TagRun { id, dir: dir.clone(), cancel: cancel.clone(), stamp, stale: Cell::new(false) };
        *self.0.tag_run.borrow_mut() = Some(run);
        let names: Vec<(String, bool, u64, i64)> =
            self.0.data.borrow().full.iter().map(|e| (e.name.clone(), e.is_dir, e.size, mtime_ns(e))).collect();
        let (pane, window) = (self.0.id, self.0.window.clone());
        let send = move |arrival: TagArrival| {
            let _ = window.upgrade_in_event_loop(move |_| {
                crate::panes::with_id(pane, |p| p.view.tags_arrived(id, arrival));
            });
        };
        let spawned = std::thread::Builder::new().name("gezik-tags".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            read_whole(&dir, &names, &cancel, &send);
        });
        if spawned.is_err() {
            // Nothing reads: the next sort, filter or listing tries again.
            self.0.tag_run.borrow_mut().take();
            self.note(gezik_i18n::t!("Tags could not be read (no thread to read them)").into_owned());
        }
    }

    /// The reading thread's news, on the UI thread.
    pub fn tags_arrived(&self, id: u64, arrival: TagArrival) {
        if !self.0.tag_run.borrow().as_ref().is_some_and(|run| run.id == id) {
            return;
        }
        match arrival {
            TagArrival::Progress { done, total } => {
                let (done, total) = (gezik_core::group_digits(done as u64), gezik_core::group_digits(total as u64));
                *self.0.tag_progress.borrow_mut() = Some(gezik_i18n::t!("Reading tags… {} of {}", done, total));
                self.update_status();
            }
            TagArrival::Done(tags) => {
                let Some(run) = self.0.tag_run.borrow_mut().take() else { return };
                self.0.tag_progress.borrow_mut().take();
                let (dir, stamp, stale) = (run.dir.clone(), run.stamp, run.stale.get());
                let done = FolderTags { dir: dir.clone(), tags, complete: true, faint: Cell::new(false), stamp };
                self.0.data.borrow_mut().folder_tags = Some(Rc::new(done));
                // Once: the rows take their places, groups and filter.
                self.resort(false);
                self.update_status();
                // Changed meanwhile: once more, from the files shown now.
                if stale && self.0.data.borrow().folder_tags.is_some() {
                    let stamp = stamp_of(&self.0.data.borrow().full);
                    self.start_tag_read(dir, stamp);
                }
            }
        }
    }

    /// The keys rows of `dir` sort and group by: as read, else on their way.
    pub(super) fn tag_keys(&self, dir: &Path) -> impl Fn(&Entry) -> TagKey + use<> {
        let known = self.0.data.borrow().folder_tags.clone().filter(|f| f.dir == dir);
        let list = crate::tags::list();
        move |e: &Entry| tag_key(known.as_ref().and_then(|f| f.of(&e.name)).map(|m| &m.tags[..]), &list)
    }

    /// The stars rows of `dir` sort and group by: as read, else on their way (`None`).
    pub(super) fn rating_keys(&self, dir: &Path) -> impl Fn(&Entry) -> Option<u8> + use<> {
        let known = self.0.data.borrow().folder_tags.clone().filter(|f| f.dir == dir);
        move |e: &Entry| known.as_ref().and_then(|f| f.of(&e.name)).map(|m| m.rating)
    }

    /// What `pattern`, the `tag:` words `terms`, the property words `fields` (spec 13 §8.3) and
    /// the `git:` words `git` (spec 16 §8.4) let through of `full` (the entries of `shape`'s
    /// folder); rows whose tags, fields or Git marks are on their way pass. Inside an archive
    /// only the name counts (no tags, no fields, no Git there).
    pub(super) fn filter_files(
        &self,
        shape: &Listing,
        full: &Rc<Vec<Entry>>,
        pattern: &Pattern,
        terms: &[TagTerm],
        fields: &[gezik_core::field_filter::FieldTerm],
        git: &[gezik_core::git_filter::GitTerm],
    ) -> (Listing, Option<Vec<usize>>) {
        let dir = match shape.folder() {
            Some(dir) if !terms.is_empty() || !fields.is_empty() || !git.is_empty() => dir,
            _ => return filtered_with(shape, full, pattern, None),
        };
        let known = self.0.data.borrow().folder_tags.clone().filter(|f| f.dir == dir && !terms.is_empty());
        let by_fields = self.field_filter(dir, fields);
        let by_git = self.git_filter(dir, git);
        let keep = |e: &Entry| {
            known.as_ref().and_then(|f| f.of(&e.name)).is_none_or(|meta| matches(terms, meta))
                && by_fields(e)
                && by_git(e)
        };
        filtered_with(shape, full, pattern, Some(&keep))
    }
}

fn mtime_ns(e: &Entry) -> i64 {
    let since = e.modified.and_then(|t| t.duration_since(UNIX_EPOCH).ok());
    since.map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

/// Off the UI thread: store entries of files moved here by other programs follow them, then every
/// name's tags, progress every `PROGRESS_EVERY`. Stops as soon as `cancel` is set.
fn read_whole(dir: &Path, names: &[(String, bool, u64, i64)], cancel: &AtomicBool, send: &dyn Fn(TagArrival)) {
    let files: Vec<(&str, u64, i64)> =
        names.iter().filter(|n| !n.1).map(|(name, _, size, mtime)| (name.as_str(), *size, *mtime)).collect();
    crate::tags::refind_in(dir, &files);
    let mut tags = HashMap::new();
    let mut said = Instant::now();
    for (i, (name, ..)) in names.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let found = crate::tags::read_meta(&dir.join(name));
        if !found.is_empty() {
            tags.insert(Box::from(name.as_str()), found);
        }
        if said.elapsed() >= PROGRESS_EVERY {
            send(TagArrival::Progress { done: i + 1, total: names.len() });
            said = Instant::now();
        }
    }
    send(TagArrival::Done(tags));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Media;
    use crate::view::listing::files;
    use gezik_core::meta::TagColor;
    use gezik_core::sort::{SortDir, SortSpec};

    #[test]
    fn nothing_is_read_unless_a_sort_group_or_filter_needs_it() {
        assert!(!needed(SortKey::Name, GroupBy::Type, false));
        assert!(needed(SortKey::Tags, GroupBy::None, false));
        assert!(needed(SortKey::Name, GroupBy::Tag, false));
        assert!(needed(SortKey::Size, GroupBy::None, true));
        assert!(needed(SortKey::Rating, GroupBy::None, false) && needed(SortKey::Name, GroupBy::Rating, false));
    }

    fn view_of(names: &[&str]) -> View {
        let view = View::new(crate::panes::next_id(), slint::Weak::default(), Media::idle(), Default::default(), None);
        let listing = files("/x", names);
        let Listing::Files(_, full) = &listing else { unreachable!() };
        let mut data = view.0.data.borrow_mut();
        data.full = full.clone();
        data.listing = listing;
        drop(data);
        view
    }

    fn shown(view: &View) -> Vec<String> {
        let data = view.0.data.borrow();
        (0..data.listing.len()).map(|i| data.listing.entry(i).unwrap().name.clone()).collect()
    }

    fn red() -> Meta {
        Meta { tags: vec![gezik_core::meta::Tag::new("Red", TagColor::Red)], ..Meta::default() }
    }

    #[test]
    fn a_tag_sort_reads_the_folder_once_and_sorts_when_it_is_in() {
        let view = view_of(&["b.txt", "a.txt", "c.txt"]);
        let started = || STARTED.with(std::cell::Cell::get);
        let before = started();
        view.sync_folder_tags(false);
        assert_eq!(started(), before, "sorted by name: nothing read");
        assert!(view.0.data.borrow().folder_tags.is_none());

        view.0.current.set(gezik_core::view::ViewSettings {
            sort: SortSpec { key: SortKey::Tags, dir: SortDir::Asc },
            ..view.0.current.get()
        });
        view.sync_folder_tags(false);
        assert_eq!(started(), before + 1);
        view.sync_folder_tags(false);
        assert_eq!(started(), before + 1, "one read while it goes on");
        let id = view.0.tag_runs.get();
        // An older read's news is dropped.
        view.tags_arrived(id - 1, TagArrival::Done(HashMap::from([(Box::from("c.txt"), red())])));
        assert!(!view.0.data.borrow().folder_tags.as_ref().unwrap().complete);
        view.tags_arrived(id, TagArrival::Progress { done: 1, total: 3 });
        assert_eq!(view.0.tag_progress.borrow().as_deref(), Some("Reading tags… 1 of 3"));
        view.tags_arrived(id, TagArrival::Done(HashMap::from([(Box::from("c.txt"), red())])));
        assert_eq!(shown(&view), ["c.txt", "a.txt", "b.txt"], "sorted again once: tagged first");
        assert!(view.0.tag_progress.borrow().is_none());
        view.sync_folder_tags(false);
        assert_eq!(started(), before + 1, "read and complete: nothing more");

        // The same files shown again: not read again; a job's tag change: read again, the old
        // tags holding until then.
        view.sync_folder_tags(true);
        assert_eq!(started(), before + 1, "same files");
        view.reread_folder_tags();
        assert_eq!(started(), before + 2);
        assert!(view.0.data.borrow().folder_tags.as_ref().unwrap().complete, "no jump meanwhile");

        view.0.current.set(gezik_core::view::ViewSettings { sort: SortSpec::default(), ..view.0.current.get() });
        view.sync_folder_tags(false);
        assert!(view.0.data.borrow().folder_tags.is_none(), "not needed any more: dropped");
        assert!(view.0.tag_run.borrow().is_none(), "and the read stopped");
    }

    #[test]
    fn a_rating_sort_and_filter_read_the_folder_and_use_the_stars() {
        let view = view_of(&["a.txt", "b.txt", "c.txt"]);
        view.0.current.set(gezik_core::view::ViewSettings {
            sort: SortSpec { key: SortKey::Rating, dir: SortDir::Desc },
            ..view.0.current.get()
        });
        view.sync_folder_tags(false);
        let id = view.0.tag_runs.get();
        let stars = |n| Meta { rating: n, ..Meta::default() };
        view.tags_arrived(
            id,
            TagArrival::Done(HashMap::from([(Box::from("a.txt"), stars(2)), (Box::from("c.txt"), stars(5))])),
        );
        assert_eq!(shown(&view), ["c.txt", "a.txt", "b.txt"], "most stars first, not rated last");
        let terms = gezik_core::meta_filter::split("rating:>=2").1;
        let full = view.0.data.borrow().full.clone();
        let x = Listing::Files("/x".into(), Rc::default());
        let (listing, _) = view.filter_files(&x, &full, &Pattern::default(), &terms, &[], &[]);
        assert_eq!(listing.len(), 2, "b.txt has no stars");
        let (none, _) = view.filter_files(&x, &full, &Pattern::default(), &[TagTerm::Unrated], &[], &[]);
        assert_eq!(none.len(), 1);
    }

    #[test]
    fn a_tag_filter_shows_rows_faint_until_read_then_filters() {
        let view = view_of(&["b.txt", "a.txt"]);
        *view.0.filter.borrow_mut() = Some(super::super::FilterState::new("tag:Red", None));
        view.sync_folder_tags(false);
        let (listing, _) = view.filter_files(
            &Listing::Files("/x".into(), Rc::default()),
            &view.0.data.borrow().full.clone(),
            &Pattern::default(),
            &[TagTerm::Named("Red".into())],
            &[],
            &[],
        );
        assert_eq!(listing.len(), 2, "not read yet: shown");
        assert!(view.0.data.borrow().folder_tags.as_ref().unwrap().faint.get());
        let id = view.0.tag_runs.get();
        view.tags_arrived(id, TagArrival::Done(HashMap::from([(Box::from("a.txt"), red())])));
        assert_eq!(shown(&view), ["a.txt"]);
    }

    #[test]
    fn a_busy_folder_is_read_once_more_after_the_read_going_on() {
        let view = view_of(&["a.txt", "b.txt"]);
        let started = || STARTED.with(std::cell::Cell::get);
        view.0.current.set(gezik_core::view::ViewSettings { group: GroupBy::Tag, ..view.0.current.get() });
        let before = started();
        view.sync_folder_tags(true);
        assert_eq!(started(), before + 1);
        // Shown again unchanged, then changed, while it reads: no restart.
        view.sync_folder_tags(true);
        assert!(!view.0.tag_run.borrow().as_ref().unwrap().stale.get(), "the same files");
        let Listing::Files(_, more) = files("/x", &["a.txt", "b.txt", "c.txt"]) else { unreachable!() };
        view.0.data.borrow_mut().full = more;
        view.sync_folder_tags(true);
        view.sync_folder_tags(true);
        assert_eq!(started(), before + 1, "never restarted, so it ends");
        assert!(view.0.tag_run.borrow().as_ref().unwrap().stale.get());
        let id = view.0.tag_runs.get();
        view.tags_arrived(id, TagArrival::Done(HashMap::new()));
        assert!(view.0.data.borrow().folder_tags.as_ref().unwrap().complete, "its result is used");
        assert_eq!(started(), before + 2, "then once more");
        let id = view.0.tag_runs.get();
        view.tags_arrived(id, TagArrival::Done(HashMap::new()));
        assert_eq!(started(), before + 2, "done");
    }

    #[test]
    fn closing_a_view_stops_its_read() {
        let view = view_of(&["a.txt"]);
        view.0.current.set(gezik_core::view::ViewSettings {
            sort: SortSpec { key: SortKey::Tags, dir: SortDir::Asc },
            ..view.0.current.get()
        });
        view.sync_folder_tags(false);
        let cancel = view.0.tag_run.borrow().as_ref().unwrap().cancel.clone();
        assert!(!cancel.load(Ordering::Relaxed));
        drop(view);
        assert!(cancel.load(Ordering::Relaxed), "the thread sees it before its next file");
    }

    #[test]
    fn the_faint_rows_follow_the_filter() {
        let view = view_of(&["a.txt"]);
        view.0.current.set(gezik_core::view::ViewSettings {
            sort: SortSpec { key: SortKey::Tags, dir: SortDir::Asc },
            ..view.0.current.get()
        });
        view.sync_folder_tags(false);
        let faint = || view.0.data.borrow().folder_tags.as_ref().unwrap().faint.get();
        assert!(!faint(), "a sort does not wait with faint rows");
        *view.0.filter.borrow_mut() = Some(super::super::FilterState::new("tag:Red", None));
        view.sync_folder_tags(false);
        assert!(faint(), "a tag filter added while it reads");
        *view.0.filter.borrow_mut() = None;
        view.sync_folder_tags(false);
        assert!(!faint());
    }

    #[test]
    fn sorting_by_a_hidden_column_shows_it() {
        use gezik_core::view::ColumnKey;
        let view = view_of(&["a.txt"]);
        let tags_shown = || view.columns_shown().iter().any(|c| c.key == ColumnKey::Tags && c.visible);
        assert!(!tags_shown());
        view.set_sort(SortSpec { key: SortKey::Tags, dir: SortDir::Asc });
        assert!(tags_shown(), "its header carries the arrow");
        assert_eq!(view.sort().key, SortKey::Tags);
    }
}
