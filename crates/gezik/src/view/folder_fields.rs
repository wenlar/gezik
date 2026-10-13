//! A folder's metadata fields read whole (spec 13 §7.5): only while the view sorts or groups by
//! a field or filters with a property word. One low-priority thread reads every file the fields'
//! readers know (by extension; no other file is opened), says how far it got, and hands the
//! result over once; the list is then sorted, grouped and filtered again, once. Rows not read yet
//! sort last, group as `Reading properties…` and pass the filter, drawn faint.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gezik_core::Entry;
use gezik_core::field_filter::{FieldTerm, matches};
use gezik_core::fields::{self, FieldId, FieldSet, Fields, Value};
use gezik_core::group::GroupBy;
use gezik_core::sort::SortKey;

use super::View;
use super::listing::Listing;

/// Progress is said at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// What the view knows of its folder's fields.
pub struct FolderFields {
    pub dir: PathBuf,
    /// The fields read.
    pub read: FieldSet,
    /// Files with a value only (a folder of 100,000 text files keeps nothing per file).
    pub values: HashMap<Box<str>, Rc<Fields>>,
    /// Every name was read; until then every row is on its way.
    pub complete: bool,
    /// A property filter waits for the first read: the rows it lets through meanwhile are faint.
    pub faint: Cell<bool>,
    /// The files it was read from: a listing of the same files is not read again.
    stamp: u64,
}

impl FolderFields {
    /// Row `name`'s fields; `None` while on their way.
    pub fn of(&self, name: &str) -> Option<Rc<Fields>> {
        self.complete.then(|| self.values.get(name).cloned().unwrap_or_default())
    }
}

/// The read going on; dropped (a read overtaken, a pane or window closed), it stops.
pub struct FieldRun {
    id: u64,
    dir: PathBuf,
    want: FieldSet,
    cancel: Arc<AtomicBool>,
    stamp: u64,
    /// The files changed while it went on: read once more when it is done.
    stale: Cell<bool>,
}

impl Drop for FieldRun {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// What the reading thread sends.
pub enum FieldArrival {
    Progress { done: usize, total: usize },
    Done(HashMap<Box<str>, Rc<Fields>>),
}

// `Rc` is not `Send`; the thread sends `Fields` and the UI thread wraps them.
enum Sent {
    Progress { done: usize, total: usize },
    Done(Vec<(Box<str>, Fields)>),
}

/// The fields a view needs whole (none: nothing is read): its sort's, its grouping's and its
/// filter's.
pub fn needed(sort: SortKey, group: GroupBy, terms: &[FieldTerm]) -> FieldSet {
    let mut want = gezik_core::field_filter::needed(terms);
    if let SortKey::Field(id) = sort {
        want = want.with(FieldSet::of(&[id]));
    }
    if let GroupBy::Field(id) = group {
        want = want.with(FieldSet::of(&[id]));
    }
    want
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
    fn filter_terms(&self) -> Vec<FieldTerm> {
        self.0.filter.borrow().as_ref().map(|f| f.fields.clone()).unwrap_or_default()
    }

    /// Reads the folder shown whole if its sort, grouping or filter needs fields; `fresh`: a
    /// listing was shown, read again if its files differ from those read (one read at a time:
    /// one going on reads once more when done). Not needed: what was read goes.
    pub(super) fn sync_folder_fields(&self, fresh: bool) {
        let dir = match &self.0.data.borrow().listing {
            Listing::Files(dir, _) if !dir.as_os_str().is_empty() => Some(dir.clone()),
            _ => None,
        };
        let terms = self.filter_terms();
        let filtering = !terms.is_empty();
        let want = needed(self.sort().key, self.group_by(), &terms);
        let Some(dir) = dir.filter(|_| !want.is_empty()) else {
            self.stop_field_read();
            self.0.data.borrow_mut().folder_fields = None;
            return;
        };
        let (same, complete, read_stamp) = match &self.0.data.borrow().folder_fields {
            Some(f) if f.dir == dir && f.read.covers(want) => {
                f.faint.set(filtering && !f.complete);
                (true, f.complete, f.stamp)
            }
            _ => (false, false, 0),
        };
        if !same {
            let fields = FolderFields {
                dir: dir.clone(),
                read: want,
                values: HashMap::new(),
                complete: false,
                faint: Cell::new(filtering),
                stamp: 0,
            };
            self.0.data.borrow_mut().folder_fields = Some(Rc::new(fields));
        }
        let stamp = stamp_of(&self.0.data.borrow().full);
        let run = self.0.field_run.borrow().as_ref().filter(|r| r.dir == dir && r.want.covers(want)).map(|r| r.stamp);
        match run {
            Some(run_stamp) => {
                if fresh
                    && stamp != run_stamp
                    && let Some(run) = self.0.field_run.borrow().as_ref()
                {
                    run.stale.set(true);
                }
            }
            None if !same || !complete || (fresh && stamp != read_stamp) => self.start_field_read(dir, want, stamp),
            None => {}
        }
    }

    fn stop_field_read(&self) {
        // Dropped, it stops.
        self.0.field_run.borrow_mut().take();
        if self.0.field_progress.borrow_mut().take().is_some() {
            self.update_status();
        }
    }

    fn start_field_read(&self, dir: PathBuf, want: FieldSet, stamp: u64) {
        self.stop_field_read();
        #[cfg(test)]
        STARTED.with(|s| s.set(s.get() + 1));
        let id = self.0.field_runs.get() + 1;
        self.0.field_runs.set(id);
        let cancel = Arc::new(AtomicBool::new(false));
        let run = FieldRun { id, dir: dir.clone(), want, cancel: cancel.clone(), stamp, stale: Cell::new(false) };
        *self.0.field_run.borrow_mut() = Some(run);
        // Only the files a reader of `want` knows (by extension): the others have no value.
        let names: Vec<String> = self
            .0
            .data
            .borrow()
            .full
            .iter()
            .filter(|e| !e.is_dir && !fields::provided(&e.extension().to_ascii_lowercase()).and(want).is_empty())
            .map(|e| e.name.clone())
            .collect();
        let (pane, window) = (self.0.id, self.0.window.clone());
        let send = move |sent: Sent| {
            let _ = window.upgrade_in_event_loop(move |_| {
                let arrival = match sent {
                    Sent::Progress { done, total } => FieldArrival::Progress { done, total },
                    Sent::Done(values) => {
                        FieldArrival::Done(values.into_iter().map(|(name, v)| (name, Rc::new(v))).collect())
                    }
                };
                crate::panes::with_id(pane, |p| p.view.fields_arrived(id, arrival));
            });
        };
        let spawned = std::thread::Builder::new().name("gezik-fields".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            read_whole(&dir, &names, want, &cancel, &send);
        });
        if spawned.is_err() {
            // Nothing reads: the next sort, filter or listing tries again.
            self.0.field_run.borrow_mut().take();
            self.note(gezik_i18n::t!("Properties could not be read (no thread to read them)").into_owned());
        }
    }

    /// The reading thread's news, on the UI thread.
    pub fn fields_arrived(&self, id: u64, arrival: FieldArrival) {
        if !self.0.field_run.borrow().as_ref().is_some_and(|run| run.id == id) {
            return;
        }
        match arrival {
            FieldArrival::Progress { done, total } => {
                let (done, total) = (gezik_core::group_digits(done as u64), gezik_core::group_digits(total as u64));
                *self.0.field_progress.borrow_mut() = Some(gezik_i18n::t!("Reading properties… {} of {}", done, total));
                self.update_status();
            }
            FieldArrival::Done(values) => {
                let Some(run) = self.0.field_run.borrow_mut().take() else { return };
                self.0.field_progress.borrow_mut().take();
                let (dir, want, stamp, stale) = (run.dir.clone(), run.want, run.stamp, run.stale.get());
                let done = FolderFields {
                    dir: dir.clone(),
                    read: want,
                    values,
                    complete: true,
                    faint: Cell::new(false),
                    stamp,
                };
                self.0.data.borrow_mut().folder_fields = Some(Rc::new(done));
                // Once: the rows take their places, groups and filter.
                self.resort(false);
                self.update_status();
                // Changed meanwhile: once more, from the files shown now.
                if stale && self.0.data.borrow().folder_fields.is_some() {
                    let stamp = stamp_of(&self.0.data.borrow().full);
                    self.start_field_read(dir, want, stamp);
                }
            }
        }
    }

    /// What rows of `dir` sort and group by: as read (`Some(None)`: no value), else on their way.
    pub(super) fn field_look(&self, dir: &Path) -> impl Fn(&Entry, FieldId) -> Option<Option<Value>> + use<> {
        let known = self.0.data.borrow().folder_fields.clone().filter(|f| f.dir == dir);
        move |e: &Entry, id: FieldId| {
            let values = known.as_ref()?.of(&e.name)?;
            Some(values.get(id).cloned())
        }
    }

    /// Whether a row of `dir` passes the property words `terms` (`filter_files`); rows whose
    /// fields are on their way pass.
    pub(super) fn field_filter(&self, dir: &Path, terms: &[FieldTerm]) -> impl Fn(&Entry) -> bool + use<> {
        let known = self.0.data.borrow().folder_fields.clone().filter(|f| f.dir == dir && !terms.is_empty());
        let terms = terms.to_vec();
        move |e: &Entry| known.as_ref().and_then(|f| f.of(&e.name)).is_none_or(|v| matches(&terms, &v))
    }
}

/// Off the UI thread: every name's fields, progress every `PROGRESS_EVERY`. Stops as soon as
/// `cancel` is set.
fn read_whole(dir: &Path, names: &[String], want: FieldSet, cancel: &AtomicBool, send: &dyn Fn(Sent)) {
    let mut values = Vec::new();
    let mut said = Instant::now();
    let stop = || cancel.load(Ordering::Relaxed);
    for (i, name) in names.iter().enumerate() {
        if stop() {
            return;
        }
        let found = gezik_batch::fields::read(&dir.join(name), want, &stop);
        if !found.is_empty() {
            values.push((Box::from(name.as_str()), found));
        }
        if said.elapsed() >= PROGRESS_EVERY {
            send(Sent::Progress { done: i + 1, total: names.len() });
            said = Instant::now();
        }
    }
    if !stop() {
        send(Sent::Done(values));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Media;
    use crate::view::listing::files;
    use gezik_core::fields::{ARTIST, DURATION};
    use gezik_core::sort::{SortDir, SortSpec};

    #[test]
    fn nothing_is_read_unless_a_sort_group_or_filter_needs_it() {
        assert!(needed(SortKey::Name, GroupBy::Type, &[]).is_empty());
        assert_eq!(needed(SortKey::Field(DURATION), GroupBy::None, &[]), FieldSet::of(&[DURATION]));
        assert_eq!(needed(SortKey::Name, GroupBy::Field(ARTIST), &[]), FieldSet::of(&[ARTIST]));
        let (_, terms) = gezik_core::field_filter::split("artist:x").unwrap();
        assert_eq!(needed(SortKey::Size, GroupBy::None, &terms), FieldSet::of(&[ARTIST]));
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

    fn song(ms: u64) -> Rc<Fields> {
        Rc::new(Fields::new(vec![(DURATION, Value::Duration(ms)), (ARTIST, Value::Text("Tarkan".into()))]))
    }

    #[test]
    fn a_field_sort_reads_the_folder_once_and_sorts_when_it_is_in() {
        let view = view_of(&["b.mp3", "a.mp3", "c.mp3", "notes.txt"]);
        let started = || STARTED.with(std::cell::Cell::get);
        let before = started();
        view.sync_folder_fields(false);
        assert_eq!(started(), before, "sorted by name: nothing read");
        assert!(view.0.data.borrow().folder_fields.is_none());

        view.0.current.set(gezik_core::view::ViewSettings {
            sort: SortSpec { key: SortKey::Field(DURATION), dir: SortDir::Asc },
            ..view.0.current.get()
        });
        view.sync_folder_fields(false);
        assert_eq!(started(), before + 1);
        view.sync_folder_fields(false);
        assert_eq!(started(), before + 1, "one read while it goes on");
        let id = view.0.field_runs.get();
        view.fields_arrived(id - 1, FieldArrival::Done(HashMap::from([(Box::from("c.mp3"), song(1))])));
        assert!(!view.0.data.borrow().folder_fields.as_ref().unwrap().complete, "an older read's news is dropped");
        view.fields_arrived(id, FieldArrival::Progress { done: 1, total: 3 });
        assert_eq!(view.0.field_progress.borrow().as_deref(), Some("Reading properties… 1 of 3"));
        let values = HashMap::from([(Box::from("c.mp3"), song(1000)), (Box::from("a.mp3"), song(5000))]);
        view.fields_arrived(id, FieldArrival::Done(values));
        assert_eq!(shown(&view), ["c.mp3", "a.mp3", "b.mp3", "notes.txt"], "sorted once: no value last");
        assert!(view.0.field_progress.borrow().is_none());
        view.sync_folder_fields(true);
        assert_eq!(started(), before + 1, "read, complete, the same files: nothing more");

        view.0.current.set(gezik_core::view::ViewSettings { sort: SortSpec::default(), ..view.0.current.get() });
        view.sync_folder_fields(false);
        assert!(view.0.data.borrow().folder_fields.is_none(), "not needed any more: dropped");
        assert!(view.0.field_run.borrow().is_none(), "and the read stopped");
    }

    #[test]
    fn a_property_filter_shows_rows_faint_until_read_then_filters() {
        let view = view_of(&["b.mp3", "a.mp3"]);
        *view.0.filter.borrow_mut() = Some(super::super::FilterState::new("duration>3m", None));
        view.sync_folder_fields(false);
        let terms = view.filter_terms();
        let full = view.0.data.borrow().full.clone();
        let (listing, _) = view.filter_files(
            &Listing::Files("/x".into(), Rc::default()),
            &full,
            &gezik_core::pattern::Pattern::default(),
            &[],
            &terms,
        );
        assert_eq!(listing.len(), 2, "not read yet: shown");
        assert!(view.0.data.borrow().folder_fields.as_ref().unwrap().faint.get());
        let id = view.0.field_runs.get();
        view.fields_arrived(id, FieldArrival::Done(HashMap::from([(Box::from("a.mp3"), song(200_000))])));
        assert_eq!(shown(&view), ["a.mp3"]);
    }

    #[test]
    fn a_broken_word_is_said_and_left_out() {
        let state = super::super::FilterState::new("*.mp3 duration>3x artist:tarkan", None);
        assert!(state.error.as_deref().is_some_and(|e| e.starts_with("duration>3x:")));
        assert_eq!(state.fields, [gezik_core::field_filter::FieldTerm::Has(ARTIST, "tarkan".into())]);
        assert!(state.pattern.matches("a.mp3") && !state.pattern.matches("a.txt"), "the name part filters");
        let alone = super::super::FilterState::new("duration>3x", None);
        assert!(alone.error.is_some() && alone.fields.is_empty() && alone.pattern.is_empty(), "all show");
    }

    #[test]
    fn closing_a_view_stops_its_read() {
        let view = view_of(&["a.mp3"]);
        view.0.current.set(gezik_core::view::ViewSettings { group: GroupBy::Field(ARTIST), ..view.0.current.get() });
        view.sync_folder_fields(false);
        let cancel = view.0.field_run.borrow().as_ref().unwrap().cancel.clone();
        assert!(!cancel.load(Ordering::Relaxed));
        drop(view);
        assert!(cancel.load(Ordering::Relaxed), "the thread sees it before its next file");
    }
}
