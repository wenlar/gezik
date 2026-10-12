//! A folder's tags read whole (spec 12 §3.4 item 3, §6): only while the view sorts by tags,
//! groups by tag or filters with `tag:`. One low-priority thread reads every name, says how far
//! it got, and hands the result over once; the list is then sorted, grouped and filtered again,
//! once. Rows not read yet sort last, group as `Reading tags…` and pass the filter, drawn faint.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use gezik_core::Entry;
use gezik_core::group::GroupBy;
use gezik_core::meta::Tag;
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
    /// Tagged names only (a folder of 100,000 untagged files keeps nothing per file).
    pub tags: HashMap<Box<str>, Box<[Tag]>>,
    /// Every name was read; until then every row is on its way.
    pub complete: bool,
    /// A `tag:` filter waits for the first read: the rows it lets through meanwhile are faint.
    pub faint: bool,
}

impl FolderTags {
    /// Row `name`'s tags; `None` while on their way.
    pub fn of(&self, name: &str) -> Option<&[Tag]> {
        self.complete.then(|| self.tags.get(name).map_or(&[][..], |tags| &tags[..]))
    }
}

/// The read going on.
pub struct TagRun {
    id: u64,
    dir: PathBuf,
    cancel: Arc<AtomicBool>,
}

/// What the reading thread sends.
pub enum TagArrival {
    Progress { done: usize, total: usize },
    Done(HashMap<Box<str>, Box<[Tag]>>),
}

/// Whether a view needs its folder's tags whole (nothing is read otherwise).
pub fn needed(sort: SortKey, group: GroupBy, filter_has_tags: bool) -> bool {
    sort == SortKey::Tags || group == GroupBy::Tag || filter_has_tags
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

    /// Reads the folder shown whole if its sort, grouping or filter needs it; `fresh`: its files
    /// may have changed (a listing shown, a tag job ended), so it is read again, the old tags
    /// used until then. Not needed: what was read goes.
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
            return;
        };
        let (same, complete) = match &self.0.data.borrow().folder_tags {
            Some(f) if f.dir == dir => (true, f.complete),
            _ => (false, false),
        };
        if !same {
            let tags = FolderTags { dir: dir.clone(), tags: HashMap::new(), complete: false, faint: filter };
            self.0.data.borrow_mut().folder_tags = Some(Rc::new(tags));
        }
        let running = self.0.tag_run.borrow().as_ref().is_some_and(|run| run.dir == dir);
        if !same || fresh || (!running && !complete) {
            self.start_tag_read(dir);
        }
    }

    /// A tag job ended: a folder read whole is read again.
    pub fn reread_folder_tags(&self) {
        if self.0.data.borrow().folder_tags.is_some() {
            self.sync_folder_tags(true);
        }
    }

    fn stop_tag_read(&self) {
        if let Some(run) = self.0.tag_run.borrow_mut().take() {
            run.cancel.store(true, Ordering::Relaxed);
        }
        if self.0.tag_progress.borrow_mut().take().is_some() {
            self.update_status();
        }
    }

    fn start_tag_read(&self, dir: PathBuf) {
        self.stop_tag_read();
        #[cfg(test)]
        STARTED.with(|s| s.set(s.get() + 1));
        let id = self.0.tag_runs.get() + 1;
        self.0.tag_runs.set(id);
        let cancel = Arc::new(AtomicBool::new(false));
        *self.0.tag_run.borrow_mut() = Some(TagRun { id, dir: dir.clone(), cancel: cancel.clone() });
        let names: Vec<(String, bool, u64, i64)> =
            self.0.data.borrow().full.iter().map(|e| (e.name.clone(), e.is_dir, e.size, mtime_ns(e))).collect();
        let (pane, window) = (self.0.id, self.0.window.clone());
        let send = move |arrival: TagArrival| {
            let _ = window.upgrade_in_event_loop(move |_| {
                crate::panes::with_id(pane, |p| p.view.tags_arrived(id, arrival));
            });
        };
        let _ = std::thread::Builder::new().name("gezik-tags".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            read_whole(&dir, &names, &cancel, &send);
        });
    }

    /// The reading thread's news, on the UI thread.
    pub fn tags_arrived(&self, id: u64, arrival: TagArrival) {
        let Some(dir) = self.0.tag_run.borrow().as_ref().filter(|run| run.id == id).map(|run| run.dir.clone()) else {
            return;
        };
        match arrival {
            TagArrival::Progress { done, total } => {
                let (done, total) = (gezik_core::group_digits(done as u64), gezik_core::group_digits(total as u64));
                *self.0.tag_progress.borrow_mut() = Some(format!("Reading tags… {done} of {total}"));
                self.update_status();
            }
            TagArrival::Done(tags) => {
                self.0.tag_run.borrow_mut().take();
                self.0.tag_progress.borrow_mut().take();
                self.0.data.borrow_mut().folder_tags =
                    Some(Rc::new(FolderTags { dir, tags, complete: true, faint: false }));
                // Once: the rows take their places, groups and filter.
                self.resort(false);
                self.update_status();
            }
        }
    }

    /// The keys rows of `dir` sort and group by: as read, else on their way.
    pub(super) fn tag_keys(&self, dir: &Path) -> impl Fn(&Entry) -> TagKey + use<> {
        let known = self.0.data.borrow().folder_tags.clone().filter(|f| f.dir == dir);
        let list = crate::tags::list();
        move |e: &Entry| tag_key(known.as_ref().and_then(|f| f.of(&e.name)), &list)
    }

    /// What `pattern` and the `tag:` words `terms` let through of `full` (the entries of `dir`);
    /// rows whose tags are on their way pass.
    pub(super) fn filter_files(
        &self,
        dir: &Path,
        full: &Rc<Vec<Entry>>,
        pattern: &Pattern,
        terms: &[TagTerm],
    ) -> (Listing, Option<Vec<usize>>) {
        if terms.is_empty() {
            return filtered_with(dir, full, pattern, None);
        }
        let known = self.0.data.borrow().folder_tags.clone().filter(|f| f.dir == dir);
        let keep = |e: &Entry| known.as_ref().and_then(|f| f.of(&e.name)).is_none_or(|tags| matches(terms, tags));
        filtered_with(dir, full, pattern, Some(&keep))
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
        let found = crate::tags::read_tags(&dir.join(name));
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

    fn red() -> Box<[Tag]> {
        vec![Tag::new("Red", TagColor::Red)].into_boxed_slice()
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

        // A listing shown again reads again; the old tags hold until then.
        view.sync_folder_tags(true);
        assert_eq!(started(), before + 2);
        assert!(view.0.data.borrow().folder_tags.as_ref().unwrap().complete, "no jump meanwhile");

        view.0.current.set(gezik_core::view::ViewSettings { sort: SortSpec::default(), ..view.0.current.get() });
        view.sync_folder_tags(false);
        assert!(view.0.data.borrow().folder_tags.is_none(), "not needed any more: dropped");
        assert!(view.0.tag_run.borrow().is_none(), "and the read stopped");
    }

    #[test]
    fn a_tag_filter_shows_rows_faint_until_read_then_filters() {
        let view = view_of(&["b.txt", "a.txt"]);
        *view.0.filter.borrow_mut() = Some(super::super::FilterState::new("tag:Red", None));
        view.sync_folder_tags(false);
        let (listing, _) = view.filter_files(
            Path::new("/x"),
            &view.0.data.borrow().full.clone(),
            &Pattern::default(),
            &[TagTerm::Named("Red".into())],
        );
        assert_eq!(listing.len(), 2, "not read yet: shown");
        assert!(view.0.data.borrow().folder_tags.as_ref().unwrap().faint);
        let id = view.0.tag_runs.get();
        view.tags_arrived(id, TagArrival::Done(HashMap::from([(Box::from("a.txt"), red())])));
        assert_eq!(shown(&view), ["a.txt"]);
    }
}
