//! The Checksums layer (spec 11 §4): the selection's files with their CRC32, MD5, SHA-1,
//! SHA-256 or SHA-512, an expected value to compare with (from the clipboard when it holds
//! one), Copy, Save (checksum files) and Export CSV; and Verify Checksums, the files a checksum
//! file names with their results. The work is an engine job (`HashTask`); nothing of the layer
//! exists until it opens, and all of it goes when it closes.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use gezik_config::shortcuts::{Chord, Key, Platform};
use gezik_core::checksum_file::{self, Algo};
use gezik_core::export;
use gezik_i18n::{t, tn, tx};
use gezik_ops::{HashResults, HashRow, HashTask, JobId, NewTask, Report, RowState, SharedResults, Task, Verdict};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::operations::After;
use crate::windows::Slot;
use crate::{AppWindow, ToolRow, ToolToggle};

thread_local! {
    /// The open layer of each window (none while closed).
    pub(crate) static CURRENT: Slot<Rc<Checksums>> = const { Slot::new() };
}

/// Runs `f` with the current window's open layer.
pub fn with_current(f: impl FnOnce(&Checksums)) {
    if let Some(layer) = CURRENT.with(Slot::current) {
        f(&layer);
    }
}

/// Whether `path` is a file Verify Checksums reads (by its name).
pub fn is_checksum_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| checksum_file::recognize(&name.to_string_lossy()).is_some())
}

fn lock(results: &Mutex<HashResults>) -> std::sync::MutexGuard<'_, HashResults> {
    results.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The window's tool table callbacks; they reach the tool that has the table (Checksums or the
/// folder compare: one at a time), so nothing is held while it is closed.
pub fn install(window: &AppWindow) {
    use crate::compare::{FolderCompare, with_shown};
    window.on_tool_toggle(|i| {
        with_current(|c| c.toggle(i));
        with_shown(|c| c.toggle(i));
    });
    window.on_tool_field_edited(|| {
        with_current(Checksums::expected_edited);
        with_shown(FolderCompare::field_edited);
    });
    window.on_tool_pressed(|i, ctrl, shift| {
        with_current(|c| c.pressed(i, ctrl, shift));
        with_shown(|c| c.pressed(i, ctrl, shift));
    });
    window.on_tool_button(|i| {
        with_current(|c| c.button(i));
        with_shown(|c| c.button(i));
    });
}

/// The buttons under the table; the last closes.
fn buttons(verify: bool) -> &'static [&'static str] {
    if verify {
        &[tx!("Copy"), tx!("Export CSV…"), tx!("Close")]
    } else {
        &[tx!("Copy"), tx!("Save…"), tx!("Export CSV…"), tx!("Close")]
    }
}

/// Where a row's size ends in its fixed-width text: the sizes line up right.
const SIZE_WIDTH: usize = 10;

/// `checksums`: the selection (or the focused item) of `view`.
pub fn open_for_view(view: &crate::view::View) {
    if view.shows_drives() {
        return view.note(t!("Checksums work on files and folders").into_owned());
    }
    let mut items = view.selected_items();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)));
    }
    if items.is_empty() {
        return view.note(t!("Select files or folders for their checksums").into_owned());
    }
    open(items.into_iter().map(|(path, _)| path).collect());
}

/// `verify-checksums`: the selected (or focused) checksum file of `view`.
pub fn verify_for_view(view: &crate::view::View) {
    let item = view.single_selected().or_else(|| view.focus()).and_then(|i| view.entry_path(i));
    match item {
        Some((path, false)) if is_checksum_file(&path) => verify(path),
        _ => view.note(t!("Select a checksum file (.sha256, .md5, .sfv, SHA256SUMS…)").into_owned()),
    }
}

/// The folder the items' names are relative to: the deepest one holding them all.
fn common_folder(paths: &[PathBuf]) -> PathBuf {
    let mut base = paths.first().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_default();
    for path in paths.iter().skip(1) {
        while !path.starts_with(&base) {
            match base.parent() {
                Some(up) => base = up.to_path_buf(),
                None => return paths[0].parent().map(Path::to_path_buf).unwrap_or_default(),
            }
        }
    }
    base
}

/// The layer for the files of `paths` (folders walked).
pub fn open(paths: Vec<PathBuf>) {
    if paths.is_empty() || CURRENT.with(Slot::current).is_some() {
        return;
    }
    let base = common_folder(&paths);
    let mut algos = [false; 5];
    let saved = crate::windows::config().map(|c| c.load_state().checksum_algorithms).unwrap_or_default();
    for algo in saved.iter().filter_map(|key| Algo::from_key(key)) {
        algos[algo.index()] = true;
    }
    if !algos.contains(&true) {
        algos[Algo::Sha256.index()] = true;
    }
    // The clipboard's value (a download page's SHA-256) is the expected one at once.
    let expected =
        gezik_platform::clipboard::read_text().ok().flatten().and_then(|text| checksum_file::clipboard_hex(&text));
    if let Some((algo, _)) = &expected {
        algos[algo.index()] = true;
    }
    // TRANSLATORS: {} is a file name or a count of items ("3 items").
    let title = t!("Checksums of {}", crate::operations::items_text(&paths));
    let save_name = match paths.as_slice() {
        [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()),
        _ => base.file_name().map(|n| n.to_string_lossy().into_owned()),
    }
    .unwrap_or_else(|| "checksums".to_owned());
    let Some(layer) = Checksums::new(title, false, base.clone(), save_name, algos, expected) else { return };
    let (shown, system) = {
        let options = crate::view_options::current();
        (options.show_hidden, options.show_system)
    };
    let task = HashTask::items(paths, base, (shown, system), layer.ticked(), layer.results.clone());
    layer.submit(Box::new(task));
}

/// The layer checking the files checksum file `file` names.
pub fn verify(file: PathBuf) {
    if CURRENT.with(Slot::current).is_some() {
        return;
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let Some(layer) = Checksums::new(t!("Verify {}", name), true, base, name, [false; 5], None) else { return };
    layer.submit(Box::new(HashTask::verify(file, layer.results.clone())));
}

/// Window `id` closes: its layer's job stops and the layer goes.
pub fn forget(id: crate::windows::WindowId) {
    if let Some(layer) = CURRENT.with(|s| s.get(id)) {
        layer.cancel_job();
    }
    CURRENT.with(|s| s.forget(id));
}

/// The engine said something: the open layers take the rows that changed (no timer of their own).
pub fn engine_progress() {
    for (id, layer) in CURRENT.with(Slot::all) {
        crate::windows::enter(id, || layer.refresh());
    }
}

/// A job ended: the layer whose it was shows where it ended, and reads what a box ticked
/// meanwhile wants.
pub fn job_finished(id: JobId, _report: &Report) {
    for (window, layer) in CURRENT.with(Slot::all) {
        if layer.job.get() == Some(id) {
            layer.job.set(None);
            crate::windows::enter(window, || {
                if layer.pending_again.take() {
                    layer.read_missing();
                }
                layer.refresh();
            });
        }
    }
}

/// What a newly ticked box does: read the rows' missing values now, or once the running job
/// ends. A running job is never cancelled for it: one still listing the files would leave the
/// rest of them out.
fn when_ticked(running: bool) -> Tick {
    if running { Tick::AfterTheJob } else { Tick::Now }
}

#[derive(Debug, PartialEq, Eq)]
enum Tick {
    Now,
    AfterTheJob,
}

/// A verdict's colour: 1 good, 2 bad, 0 neither.
fn tone(verdict: Verdict) -> i32 {
    match verdict {
        Verdict::Ok => 1,
        Verdict::Failed | Verdict::Missing | Verdict::Unreadable => 2,
        Verdict::Waiting | Verdict::OnlineOnly => 0,
    }
}

pub struct Checksums {
    window: slint::Weak<AppWindow>,
    verify: bool,
    /// The folder the names are relative to: where Save and Export CSV write.
    base: PathBuf,
    /// The name Save offers (`report.pdf`, the folder's).
    save_name: String,
    results: SharedResults,
    algos: Cell<[bool; 5]>,
    job: Cell<Option<JobId>>,
    model: Rc<VecModel<ToolRow>>,
    /// The result row each shown row is (in order; fewer with "Failed and missing only").
    shown: RefCell<Vec<usize>>,
    /// Selected result rows, and the one a Shift+click spans from.
    selected: RefCell<BTreeSet<usize>>,
    anchor: Cell<Option<usize>>,
    only_failed: Cell<bool>,
    expected: RefCell<Option<(Algo, String)>>,
    /// A box was ticked while the job ran: its values are read when the job ends.
    pending_again: Cell<bool>,
}

impl Checksums {
    fn new(
        title: String,
        verify: bool,
        base: PathBuf,
        save_name: String,
        algos: [bool; 5],
        expected: Option<(Algo, String)>,
    ) -> Option<Rc<Checksums>> {
        let window = crate::windows::with_window(crate::windows::current(), |w| w.as_weak())?;
        let window_ref = window.upgrade()?;
        // The folder compare has the table.
        if window_ref.get_tool_open() {
            return None;
        }
        let layer = Rc::new(Checksums {
            window,
            verify,
            base,
            save_name,
            results: Arc::new(Mutex::new(HashResults::default())),
            algos: Cell::new(algos),
            job: Cell::new(None),
            model: Rc::new(VecModel::default()),
            shown: RefCell::default(),
            selected: RefCell::default(),
            anchor: Cell::new(None),
            only_failed: Cell::new(false),
            expected: RefCell::new(expected.clone()),
            pending_again: Cell::new(false),
        });
        CURRENT.with(|s| s.set(crate::windows::current(), layer.clone()));
        window_ref.set_tool_title(title.into());
        window_ref.set_tool_field_label(if verify { SharedString::new() } else { t!("Expected").as_ref().into() });
        window_ref.set_tool_field_text(expected.map(|(_, hex)| hex).unwrap_or_default().into());
        window_ref.set_tool_field_focused(false);
        window_ref.set_tool_scroll(0.0);
        window_ref.set_tool_cursor(-1);
        window_ref.set_tool_rows(ModelRc::from(layer.model.clone()));
        let labels: Vec<SharedString> =
            buttons(verify).iter().map(|b| SharedString::from(&*gezik_i18n::t("", b))).collect();
        window_ref.set_tool_buttons(ModelRc::new(VecModel::from(labels)));
        layer.show_columns();
        layer.show_summary();
        window_ref.set_tool_open(true);
        Some(layer)
    }

    fn submit(&self, task: Box<dyn Task>) {
        let mut job = None;
        crate::operations::with_current(|ops| job = Some(ops.submit(task, None, After::Nothing)));
        self.job.set(job);
    }

    /// The ticked algorithms, in their order.
    fn ticked(&self) -> Vec<Algo> {
        let on = self.algos.get();
        Algo::ALL.into_iter().filter(|a| on[a.index()]).collect()
    }

    /// The algorithms whose values a row shows (verify: none; each row has its own).
    fn columns(&self) -> Vec<Algo> {
        if self.verify { Vec::new() } else { self.ticked() }
    }

    /// The column titles and the boxes (verify: the filter).
    fn show_columns(&self) {
        let Some(window) = self.window.upgrade() else { return };
        let mut title = vec![format!("{:>SIZE_WIDTH$}", t!("column" => "Size"))];
        if self.verify {
            title.push(t!("Checksum").into_owned());
        } else {
            title.extend(self.columns().iter().map(|a| format!("{:<1$}", a.name(), a.hex_len())));
        }
        window.set_tool_columns(title.join("  ").trim_end().into());
        let toggles: Vec<ToolToggle> = if self.verify {
            vec![ToolToggle { label: t!("Failed and missing only").as_ref().into(), checked: self.only_failed.get() }]
        } else {
            let on = self.algos.get();
            Algo::ALL.iter().map(|a| ToolToggle { label: a.name().into(), checked: on[a.index()] }).collect()
        };
        // Changed in place: a new model would rebuild the boxes and drop the keyboard's one.
        let current = window.get_tool_toggles();
        if current.row_count() == toggles.len() {
            for (i, toggle) in toggles.into_iter().enumerate() {
                if current.row_data(i).as_ref() != Some(&toggle) {
                    current.set_row_data(i, toggle);
                }
            }
        } else {
            window.set_tool_toggles(ModelRc::new(VecModel::from(toggles)));
        }
    }

    /// Box `i`: an algorithm, or verify's filter.
    fn toggle(&self, i: i32) {
        if self.verify { self.toggle_only_failed() } else { self.toggle_algo(i) }
    }

    /// Button `i` of `buttons`.
    fn button(&self, i: i32) {
        match usize::try_from(i).ok().and_then(|i| buttons(self.verify).get(i)) {
            Some(&"Copy") => self.copy(),
            Some(&"Save…") => self.save(),
            Some(&"Export CSV…") => self.export(),
            Some(_) => self.close(),
            None => {}
        }
    }

    /// How row `row` shows: its values padded to their lengths, so they line up in columns.
    fn row(&self, row: &HashRow, selected: bool) -> ToolRow {
        let (values, status, tone) = if self.verify {
            let verdict = row.verdict();
            let value = row.expected.as_ref().and_then(|(algo, _)| row.digest(*algo)).unwrap_or_default();
            (value.to_owned(), gezik_i18n::t("checksum", verdict.label()).into_owned(), tone(verdict))
        } else {
            let expected = self.expected.borrow();
            let mut compared = None;
            let mut values = Vec::new();
            for algo in self.columns() {
                let value = row.digest(algo);
                values.push(format!("{:<1$}", value.unwrap_or_default(), algo.hex_len()));
                if let (Some((want, hex)), Some(value)) = (&*expected, value)
                    && *want == algo
                {
                    compared = Some(value == hex);
                }
            }
            let (status, tone) = match (&row.state, compared) {
                (RowState::Unreadable(why), _) => (t!("Unreadable: {}", why), 2),
                (RowState::OnlineOnly, _) => (t!("Online only").into_owned(), 0),
                (RowState::Missing, _) => (t!("Missing").into_owned(), 2),
                (RowState::Waiting, _) => (t!("Waiting").into_owned(), 0),
                (RowState::Done, Some(true)) => (t!("Matches").into_owned(), 1),
                (RowState::Done, Some(false)) => (t!("No match").into_owned(), 2),
                (RowState::Done, None) => (String::new(), 0),
            };
            (values.join("  ").trim_end().to_owned(), status, tone)
        };
        let size =
            // A missing file, or an entry outside the folder (never looked at), has no size.
            if row.state == RowState::Missing || row.path.as_os_str().is_empty() {
                String::new()
            } else {
                crate::view_options::size_text(row.size)
            };
        let values = format!("{size:>SIZE_WIDTH$}  {values}");
        ToolRow {
            name: row.name.as_str().into(),
            values: values.trim_end().into(),
            status: status.into(),
            tone,
            selected,
        }
    }

    /// Whether `row` shows with "Failed and missing only".
    fn passes(&self, row: &HashRow) -> bool {
        !self.only_failed.get() || matches!(row.verdict(), Verdict::Failed | Verdict::Missing | Verdict::Unreadable)
    }

    /// Takes the rows the job added or changed since the last look.
    fn refresh(&self) {
        let changed: Vec<(usize, HashRow)> = {
            let mut results = lock(&self.results);
            let mut dirty = std::mem::take(&mut results.dirty);
            dirty.sort_unstable();
            dirty.dedup();
            dirty.into_iter().filter_map(|i| results.rows.get(i).map(|row| (i, row.clone()))).collect()
        };
        for (index, row) in changed {
            self.place(index, &row);
        }
        self.show_summary();
    }

    /// Shows, changes or hides result row `index` where it belongs.
    fn place(&self, index: usize, row: &HashRow) {
        let passes = self.passes(row);
        let selected = self.selected.borrow().contains(&index);
        let at = self.shown.borrow().binary_search(&index);
        match (at, passes) {
            (Ok(at), true) => self.model.set_row_data(at, self.row(row, selected)),
            (Ok(at), false) => {
                self.shown.borrow_mut().remove(at);
                self.model.remove(at);
            }
            (Err(at), true) => {
                self.shown.borrow_mut().insert(at, index);
                self.model.insert(at, self.row(row, selected));
            }
            (Err(_), false) => {}
        }
    }

    /// Every row again (columns, the expected value or the filter changed).
    fn rebuild(&self) {
        let rows: Vec<ToolRow>;
        let shown: Vec<usize>;
        {
            let results = lock(&self.results);
            let selected = self.selected.borrow();
            let kept: Vec<(usize, &HashRow)> =
                results.rows.iter().enumerate().filter(|(_, r)| self.passes(r)).collect();
            shown = kept.iter().map(|(i, _)| *i).collect();
            rows = kept.iter().map(|(i, r)| self.row(r, selected.contains(i))).collect();
        }
        *self.shown.borrow_mut() = shown;
        self.model.set_vec(rows);
        // The shown rows changed: the cursor starts again from the first.
        if let Some(window) = self.window.upgrade() {
            window.set_tool_cursor(-1);
        }
        self.show_summary();
    }

    fn show_summary(&self) {
        let Some(window) = self.window.upgrade() else { return };
        let (text, tone) = {
            let results = lock(&self.results);
            if self.verify { self.verify_summary(&results) } else { self.compute_summary(&results) }
        };
        window.set_tool_summary(text.into());
        window.set_tool_summary_tone(tone);
    }

    fn compute_summary(&self, results: &HashResults) -> (String, i32) {
        let rows = &results.rows;
        let done = rows.iter().filter(|r| r.state != RowState::Waiting).count();
        let bytes: u64 = rows.iter().map(|r| r.size).sum();
        let running = self.job.get().is_some();
        let expected = self.expected.borrow();
        let typed = window_expected(&self.window);
        match &*expected {
            _ if running => {
                let (count, total) = (group(done), group(rows.len()));
                (tn!("Reading… {} of {} file" | "Reading… {} of {} files", rows.len(), count, total), 0)
            }
            None if !typed.trim().is_empty() => (t!("Not a checksum: 8, 32, 40, 64 or 128 hex digits").into_owned(), 2),
            None => {
                let size = crate::view_options::size_text(bytes);
                (tn!("{} file · {}" | "{} files · {}", rows.len(), group(rows.len()), size), 0)
            }
            // TRANSLATORS: {} is an algorithm's name (SHA-256), a box to tick.
            Some((algo, _)) if !self.algos.get()[algo.index()] => (t!("Check {} to compare", algo.name()), 0),
            Some((algo, hex)) => {
                let matches: Vec<&HashRow> = rows.iter().filter(|r| r.digest(*algo) == Some(hex.as_str())).collect();
                match (rows.len(), matches.as_slice()) {
                    (1, [_]) => (t!("Matches the expected {}", algo.name()), 1),
                    (1, _) => (t!("Does not match the expected {}", algo.name()), 2),
                    (_, []) => (t!("No file matches the expected {}", algo.name()), 2),
                    // TRANSLATORS: {0} is a file name, {1} an algorithm's name (SHA-256).
                    (_, [one]) => (t!("{0} matches the expected {1}", one.name, algo.name()), 1),
                    (_, many) => {
                        let n = many.len();
                        (tn!("{n} file matches the expected {}" | "{n} files match the expected {}", n, algo.name()), 1)
                    }
                }
            }
        }
    }

    fn verify_summary(&self, results: &HashResults) -> (String, i32) {
        if let Some(error) = &results.error {
            return (t!("Cannot read the checksum file: {}", error), 2);
        }
        let count = |v: Verdict| results.rows.iter().filter(|r| r.verdict() == v).count();
        let ok = count(Verdict::Ok);
        let mut parts = vec![tn!("{} OK" | "{} OK", ok, group(ok))];
        let failed = count(Verdict::Failed);
        let missing = count(Verdict::Missing);
        let unreadable = count(Verdict::Unreadable);
        let online = count(Verdict::OnlineOnly);
        let counts = [
            (failed, tn!("{} failed" | "{} failed", failed, group(failed))),
            (missing, tn!("{} missing" | "{} missing", missing, group(missing))),
            (unreadable, tn!("{} unreadable" | "{} unreadable", unreadable, group(unreadable))),
            (online, tn!("{} online only" | "{} online only", online, group(online))),
        ];
        parts.extend(counts.into_iter().filter(|(n, _)| *n > 0).map(|(_, text)| text));
        let bad_lines = results.bad_lines;
        if bad_lines > 0 {
            parts.push(tn!("{} line could not be read" | "{} lines could not be read", bad_lines, group(bad_lines)));
        }
        let text = parts.join(" · ");
        if self.job.get().is_some() {
            // TRANSLATORS: {} is a list of counts ("3 OK · 1 failed").
            return (t!("Verifying… {}", text), 0);
        }
        let bad = failed + missing + unreadable > 0 || results.bad_lines > 0;
        let good = !results.rows.is_empty();
        (text, if bad { 2 } else { i32::from(good) })
    }

    fn toggle_algo(&self, i: i32) {
        let Some(algo) = usize::try_from(i).ok().and_then(|i| Algo::ALL.get(i).copied()) else { return };
        let mut on = self.algos.get();
        on[algo.index()] = !on[algo.index()];
        self.algos.set(on);
        let keys: Vec<String> = self.ticked().iter().map(|a| a.key().to_owned()).collect();
        if let Some(config) = crate::windows::config() {
            config.update_state(move |state| state.checksum_algorithms = keys);
        }
        self.show_columns();
        self.rebuild();
        if on[algo.index()] {
            match when_ticked(self.job.get().is_some()) {
                Tick::Now => self.read_missing(),
                Tick::AfterTheJob => self.pending_again.set(true),
            }
        }
    }

    /// Reads the values the rows lack for the ticked boxes.
    fn read_missing(&self) {
        self.submit(Box::new(HashTask::again(self.ticked(), self.results.clone())));
    }

    fn cancel_job(&self) {
        if let Some(job) = self.job.take() {
            crate::operations::with_current(|ops| ops.cancel(i32::try_from(job).unwrap_or(-1)));
        }
    }

    fn expected_edited(&self) {
        let typed = window_expected(&self.window);
        *self.expected.borrow_mut() = checksum_file::typed_hex(&typed);
        self.rebuild();
    }

    fn toggle_only_failed(&self) {
        self.only_failed.set(!self.only_failed.get());
        self.show_columns();
        self.rebuild();
    }

    /// A click on shown row `i`: it alone, with Ctrl added or taken away, with Shift the span.
    fn pressed(&self, i: i32, ctrl: bool, shift: bool) {
        let shown = self.shown.borrow().clone();
        let Some(&index) = usize::try_from(i).ok().and_then(|i| shown.get(i)) else { return };
        {
            let mut selected = self.selected.borrow_mut();
            match (ctrl, shift, self.anchor.get().and_then(|a| shown.iter().position(|s| *s == a))) {
                (_, true, Some(from)) => {
                    let to = usize::try_from(i).unwrap_or(0);
                    if !ctrl {
                        selected.clear();
                    }
                    selected.extend(shown[from.min(to)..=from.max(to)].iter().copied());
                }
                (true, false, _) => {
                    if !selected.remove(&index) {
                        selected.insert(index);
                    }
                    self.anchor.set(Some(index));
                }
                _ => {
                    selected.clear();
                    selected.insert(index);
                    self.anchor.set(Some(index));
                }
            }
        }
        if let Some(window) = self.window.upgrade() {
            window.set_tool_cursor(i);
        }
        self.show_selection();
    }

    /// An arrow, page or Home/End key on the rows: the row it reaches alone, with Shift the
    /// span from the anchor.
    fn step(&self, key: Key, shift: bool) {
        let Some(window) = self.window.upgrade() else { return };
        let len = self.shown.borrow().len();
        if let Some(to) = stepped(key, usize::try_from(window.get_tool_cursor()).ok(), len) {
            self.pressed(i32::try_from(to).unwrap_or(0), false, shift);
        }
    }

    fn select_all(&self) {
        self.selected.borrow_mut().extend(self.shown.borrow().iter().copied());
        self.show_selection();
    }

    /// Sets the shown rows' `selected` where it changed.
    fn show_selection(&self) {
        let shown = self.shown.borrow().clone();
        let selected = self.selected.borrow().clone();
        for (at, index) in shown.iter().enumerate() {
            if let Some(mut row) = self.model.row_data(at) {
                let on = selected.contains(index);
                if row.selected != on {
                    row.selected = on;
                    self.model.set_row_data(at, row);
                }
            }
        }
    }

    /// The rows Copy, Save and Export take: the selected shown ones, else every shown one.
    fn chosen(&self) -> Vec<HashRow> {
        let shown = self.shown.borrow().clone();
        let selected = self.selected.borrow().clone();
        let picked: Vec<usize> = shown.iter().copied().filter(|i| selected.contains(i)).collect();
        let picked = if picked.is_empty() { shown } else { picked };
        let results = lock(&self.results);
        picked.into_iter().filter_map(|i| results.rows.get(i).cloned()).collect()
    }

    fn say(&self, text: &str) {
        if let Some(window) = self.window.upgrade() {
            window.set_tool_summary(text.into());
            window.set_tool_summary_tone(0);
        }
    }

    /// Copy: GNU lines for one algorithm, BSD lines for several, the value alone for one row and
    /// one algorithm; verify: `<name>: OK` lines.
    fn copy(&self) {
        let rows = self.chosen();
        let text = if self.verify {
            rows.iter().map(|r| format!("{}: {}\n", r.name, r.verdict().label())).collect()
        } else {
            copy_text(&rows, &self.ticked())
        };
        if text.is_empty() {
            return self.say(&t!("Nothing to copy yet"));
        }
        match gezik_platform::clipboard::write_text(&text) {
            Ok(()) => {
                let n = text.trim_end().lines().count();
                self.say(&tn!("Copied {} line" | "Copied {} lines", n, group(n)));
            }
            Err(_) => self.say(&t!("Cannot use the clipboard")),
        }
    }

    /// Save: a checksum file per ticked algorithm in the base folder (spec 11 §4.2).
    fn save(&self) {
        if self.verify {
            return;
        }
        let algos = self.ticked();
        let rows: Vec<HashRow> =
            self.chosen().into_iter().filter(|r| algos.iter().all(|a| r.digest(*a).is_some())).collect();
        if algos.is_empty() || rows.is_empty() {
            return self.say(&t!("No checksums to save yet"));
        }
        let files: Vec<String> = algos.iter().map(|a| format!("{}.{}", self.save_name, a.extension())).collect();
        // TRANSLATORS: {0} is a list of file names, {1} a folder.
        let message = t!("Writes {0} in {1}", files.join(", "), self.base.display());
        let texts: Vec<(String, Vec<u8>)> = algos
            .iter()
            .map(|algo| {
                let lines = rows.iter().filter_map(|r| Some((r.name.as_str(), r.digest(*algo)?)));
                let text = if *algo == Algo::Crc32 {
                    checksum_file::write_sfv(lines)
                } else {
                    checksum_file::write_gnu(lines)
                };
                (format!(".{}", algo.extension()), text.into_bytes())
            })
            .collect();
        self.ask_and_write(&t!("Save Checksums"), message, self.save_name.clone(), texts);
    }

    /// Asks for a name (`initial` to start), then writes each of `files` (an ending after the
    /// name, its bytes) into the base folder as one undoable job; a taken name goes to the
    /// conflict list.
    fn ask_and_write(&self, title: &str, message: String, initial: String, files: Vec<(String, Vec<u8>)>) {
        let dir = self.base.clone();
        let label = title.to_owned();
        crate::operations::with_current(|ops| {
            let ops2 = ops.clone();
            ops.dialogs().ask_text(title, message, initial, &[&t!("Save"), &t!("Cancel")], move |name| {
                let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
                let mut tasks: Vec<Box<dyn Task>> = Vec::new();
                for (ending, bytes) in files {
                    let file = format!("{name}{ending}");
                    if let Err(err) =
                        gezik_core::ops::names::validate_name(&file, gezik_core::ops::names::NameRules::current())
                    {
                        return with_current(|c| c.say(&err.to_string()));
                    }
                    tasks.push(Box::new(NewTask::written(&dir, &file, move |out| out.write_all(&bytes))));
                }
                ops2.submit_chain(tasks, Some(label), None, After::Select);
            });
        });
    }

    /// Export CSV: the rows shown (or selected) with their values (spec 11 §4.1).
    fn export(&self) {
        let rows = self.chosen();
        if rows.is_empty() {
            return self.say(&t!("Nothing to export yet"));
        }
        // Verify: each row's own algorithm, value and result; else the ticked columns.
        let algos = self.ticked();
        let mut header = vec!["Path".to_owned(), "Size".to_owned()];
        if self.verify {
            header.extend(["Algorithm", "Expected", "Value", "Status"].map(str::to_owned));
        } else {
            header.extend(algos.iter().map(|a| a.name().to_owned()));
        }
        // The list export's CSV writer (11h): the system's list separator, a BOM and CRLF on
        // Windows, formula-safe fields.
        let windows = cfg!(windows);
        let sheet = export::Sheet {
            format: export::Format::Csv,
            headers: header,
            right: Vec::new(),
            separator: gezik_platform::list_separator(),
            bom: windows,
            eol: if windows { "\r\n" } else { "\n" },
            title: String::new(),
            note: String::new(),
        };
        let mut bytes = Vec::new();
        // Writing into memory cannot fail.
        let _ = export::begin(&mut bytes, &sheet);
        for r in &rows {
            let mut fields = vec![r.name.clone(), r.size.to_string()];
            match &r.expected {
                Some((algo, hex)) if self.verify => fields.extend([
                    algo.name().to_owned(),
                    hex.clone(),
                    r.digest(*algo).unwrap_or_default().to_owned(),
                    r.verdict().label().to_owned(),
                ]),
                _ => fields.extend(algos.iter().map(|a| r.digest(*a).unwrap_or_default().to_owned())),
            }
            let _ = export::row(&mut bytes, &sheet, &fields);
        }
        let _ = export::end(&mut bytes, &sheet);
        let what = if self.verify { "results" } else { "checksums" };
        let message = t!("Writes the list in {}", self.base.display());
        let initial = format!("{} {what}.csv", self.save_name);
        self.ask_and_write(&t!("Export CSV"), message, initial, vec![(String::new(), bytes)]);
    }

    /// Esc closes; Ctrl+C / Ctrl+A (⌘ on macOS) copy and select and the arrow, page and Home/End
    /// keys move the row cursor, unless the Expected field has the keyboard. Whether it was used.
    pub fn chord(&self, chord: &Chord, field_focused: bool) -> bool {
        let primary = crate::keys::is_primary(chord, Platform::current());
        match chord.key {
            Key::Escape if !primary && !chord.shift && !chord.alt => self.close(),
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End
                if !field_focused && !primary && !chord.alt =>
            {
                self.step(chord.key, chord.shift)
            }
            Key::Char('c') if primary && !chord.shift && !field_focused => self.copy(),
            Key::Char('a') if primary && !chord.shift && !field_focused => self.select_all(),
            _ => return false,
        }
        true
    }

    /// Closes the layer: its job stops, its rows and models go.
    pub fn close(&self) {
        self.cancel_job();
        CURRENT.with(|s| s.forget(crate::windows::current()));
        if let Some(window) = self.window.upgrade() {
            window.set_tool_open(false);
            window.set_tool_rows(ModelRc::default());
            window.set_tool_toggles(ModelRc::default());
            window.set_tool_buttons(ModelRc::default());
            window.set_tool_field_text(SharedString::new());
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }
}

/// The shown row a key moves the cursor to (`at`: where it is; none yet: the first), within
/// `len` rows; a page is 10 rows.
pub(crate) fn stepped(key: Key, at: Option<usize>, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    let Some(at) = at else { return Some(if key == Key::End { last } else { 0 }) };
    Some(match key {
        Key::Up => at.saturating_sub(1),
        Key::Down => (at + 1).min(last),
        Key::PageUp => at.saturating_sub(10),
        Key::PageDown => (at + 10).min(last),
        Key::Home => 0,
        Key::End => last,
        _ => return None,
    })
}

fn window_expected(window: &slint::Weak<AppWindow>) -> String {
    window.upgrade().map(|w| w.get_tool_field_text().to_string()).unwrap_or_default()
}

/// `1,234`.
fn group(n: usize) -> String {
    crate::preview::with_commas(n)
}

/// What Copy puts on the clipboard for `rows` (only the values they have).
fn copy_text(rows: &[HashRow], algos: &[Algo]) -> String {
    match (rows, algos) {
        ([row], [algo]) => row.digest(*algo).unwrap_or_default().to_owned(),
        (_, [algo]) => checksum_file::write_gnu(rows.iter().filter_map(|r| Some((r.name.as_str(), r.digest(*algo)?)))),
        _ => checksum_file::write_bsd(
            rows.iter().flat_map(|r| algos.iter().filter_map(move |a| Some((*a, r.name.as_str(), r.digest(*a)?)))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, values: &[(Algo, &str)]) -> HashRow {
        let mut digests: [Option<String>; 5] = Default::default();
        for (algo, hex) in values {
            digests[algo.index()] = Some((*hex).to_owned());
        }
        HashRow {
            path: PathBuf::from(name),
            name: name.to_owned(),
            size: 3,
            digests,
            expected: None,
            expected_size: None,
            state: RowState::Done,
            online: false,
        }
    }

    #[test]
    fn keys_move_the_cursor_within_the_rows() {
        assert_eq!(stepped(Key::Down, None, 5), Some(0), "the first row first");
        assert_eq!(stepped(Key::End, None, 5), Some(4));
        assert_eq!(stepped(Key::Down, Some(4), 5), Some(4));
        assert_eq!(stepped(Key::Up, Some(0), 5), Some(0));
        assert_eq!(stepped(Key::PageDown, Some(1), 30), Some(11));
        assert_eq!(stepped(Key::PageDown, Some(25), 30), Some(29));
        assert_eq!(stepped(Key::PageUp, Some(5), 30), Some(0));
        assert_eq!(stepped(Key::Home, Some(7), 30), Some(0));
        assert_eq!(stepped(Key::Down, None, 0), None, "no rows");
        assert_eq!(stepped(Key::Left, Some(1), 5), None);
    }

    #[test]
    fn a_box_ticked_mid_job_waits_for_the_job() {
        assert_eq!(when_ticked(true), Tick::AfterTheJob, "never cancels a job still listing files");
        assert_eq!(when_ticked(false), Tick::Now);
        assert_eq!([Verdict::Ok, Verdict::Failed, Verdict::OnlineOnly].map(tone), [1, 2, 0]);
    }

    #[test]
    fn copy_writes_gnu_for_one_algorithm_and_bsd_for_several() {
        let a = row("a.txt", &[(Algo::Md5, "aa"), (Algo::Sha256, "bb")]);
        let b = row("sub/b.txt", &[(Algo::Md5, "cc")]);
        assert_eq!(copy_text(std::slice::from_ref(&a), &[Algo::Md5]), "aa", "one value alone");
        assert_eq!(copy_text(&[a.clone(), b.clone()], &[Algo::Md5]), "aa *a.txt\ncc *sub/b.txt\n");
        assert_eq!(
            copy_text(&[a, b], &[Algo::Md5, Algo::Sha256]),
            "MD5 (a.txt) = aa\nSHA256 (a.txt) = bb\nMD5 (sub/b.txt) = cc\n",
            "what a row lacks is left out"
        );
    }

    #[test]
    fn names_are_relative_to_the_deepest_common_folder() {
        let root = std::env::temp_dir().join("gezik-common");
        let paths = [root.join("a").join("x.txt"), root.join("a").join("b").join("y.txt")];
        assert_eq!(common_folder(&paths), root.join("a"));
        let paths = [root.join("a").join("x.txt"), root.join("c").join("y.txt")];
        assert_eq!(common_folder(&paths), root);
        assert_eq!(common_folder(&[root.join("f.txt")]), root);
        assert!(is_checksum_file(Path::new("x/SHA256SUMS")) && !is_checksum_file(Path::new("x/a.txt")));
    }
}
