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
use gezik_ops::{HashResults, HashRow, HashTask, JobId, NewTask, Report, RowState, SharedResults, Task, Verdict};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::operations::After;
use crate::windows::Slot;
use crate::{AppWindow, ChecksumRow};

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

/// The window's callbacks; they reach the open layer, so nothing is held while it is closed.
pub fn install(window: &AppWindow) {
    window.on_ck_toggle_algo(|i| with_current(|c| c.toggle_algo(i)));
    window.on_ck_expected_edited(|| with_current(Checksums::expected_edited));
    window.on_ck_toggle_only_failed(|| with_current(Checksums::toggle_only_failed));
    window.on_ck_pressed(|i, ctrl, shift| with_current(|c| c.pressed(i, ctrl, shift)));
    window.on_ck_copy(|| with_current(Checksums::copy));
    window.on_ck_save(|| with_current(Checksums::save));
    window.on_ck_export(|| with_current(Checksums::export));
    window.on_ck_close(|| with_current(Checksums::close));
}

/// `checksums`: the selection (or the focused item) of `view`.
pub fn open_for_view(view: &crate::view::View) {
    if view.shows_drives() {
        return view.note("Checksums work on files and folders".to_owned());
    }
    let mut items = view.selected_items();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)));
    }
    if items.is_empty() {
        return view.note("Select files or folders for their checksums".to_owned());
    }
    open(items.into_iter().map(|(path, _)| path).collect());
}

/// `verify-checksums`: the selected (or focused) checksum file of `view`.
pub fn verify_for_view(view: &crate::view::View) {
    let item = view.single_selected().or_else(|| view.focus()).and_then(|i| view.entry_path(i));
    match item {
        Some((path, false)) if is_checksum_file(&path) => verify(path),
        _ => view.note("Select a checksum file (.sha256, .md5, .sfv, SHA256SUMS…)".to_owned()),
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
    let title = format!("Checksums of {}", crate::operations::items_text(&paths));
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
    let Some(layer) = Checksums::new(format!("Verify {name}"), true, base, name, [false; 5], None) else { return };
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

/// A job ended: the layer whose it was shows where it ended.
pub fn job_finished(id: JobId, _report: &Report) {
    for (window, layer) in CURRENT.with(Slot::all) {
        if layer.job.get() == Some(id) {
            layer.job.set(None);
            crate::windows::enter(window, || layer.refresh());
        }
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
    model: Rc<VecModel<ChecksumRow>>,
    /// The result row each shown row is (in order; fewer with "Failed and missing only").
    shown: RefCell<Vec<usize>>,
    /// Selected result rows, and the one a Shift+click spans from.
    selected: RefCell<BTreeSet<usize>>,
    anchor: Cell<Option<usize>>,
    only_failed: Cell<bool>,
    expected: RefCell<Option<(Algo, String)>>,
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
        });
        CURRENT.with(|s| s.set(crate::windows::current(), layer.clone()));
        window_ref.set_ck_title(title.into());
        window_ref.set_ck_verify(verify);
        window_ref.set_ck_expected(expected.map(|(_, hex)| hex).unwrap_or_default().into());
        window_ref.set_ck_only_failed(false);
        window_ref.set_ck_field_focused(false);
        window_ref.set_ck_scroll(0.0);
        window_ref.set_ck_rows(ModelRc::from(layer.model.clone()));
        layer.show_columns();
        layer.show_summary();
        window_ref.set_ck_open(true);
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

    fn show_columns(&self) {
        let Some(window) = self.window.upgrade() else { return };
        let title = if self.verify {
            "Checksum".to_owned()
        } else {
            self.columns().iter().map(|a| format!("{:<1$}", a.name(), a.hex_len())).collect::<Vec<_>>().join("  ")
        };
        window.set_ck_values_title(title.trim_end().into());
        window.set_ck_algos(ModelRc::new(VecModel::from(self.algos.get().to_vec())));
    }

    /// How row `row` shows: its values padded to their lengths, so they line up in columns.
    fn row(&self, row: &HashRow, selected: bool) -> ChecksumRow {
        let (values, status, tone) = if self.verify {
            let verdict = row.verdict();
            let value = row.expected.as_ref().and_then(|(algo, _)| row.digest(*algo)).unwrap_or_default();
            let tone = match verdict {
                Verdict::Ok => 1,
                Verdict::Failed | Verdict::Missing | Verdict::Unreadable => 2,
                _ => 0,
            };
            (value.to_owned(), verdict.label().to_owned(), tone)
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
                (RowState::Unreadable(why), _) => (format!("Unreadable: {why}"), 2),
                (RowState::OnlineOnly, _) => ("Online only".to_owned(), 0),
                (RowState::Missing, _) => ("Missing".to_owned(), 2),
                (RowState::Waiting, _) => ("Waiting".to_owned(), 0),
                (RowState::Done, Some(true)) => ("Matches".to_owned(), 1),
                (RowState::Done, Some(false)) => ("No match".to_owned(), 2),
                (RowState::Done, None) => (String::new(), 0),
            };
            (values.join("  ").trim_end().to_owned(), status, tone)
        };
        let size =
            if row.state == RowState::Missing { String::new() } else { crate::view_options::size_text(row.size) };
        ChecksumRow {
            name: row.name.as_str().into(),
            size: size.into(),
            values: values.into(),
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
        let rows: Vec<ChecksumRow>;
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
        self.show_summary();
    }

    fn show_summary(&self) {
        let Some(window) = self.window.upgrade() else { return };
        let (text, tone) = {
            let results = lock(&self.results);
            if self.verify { self.verify_summary(&results) } else { self.compute_summary(&results) }
        };
        window.set_ck_summary(text.into());
        window.set_ck_summary_tone(tone);
    }

    fn compute_summary(&self, results: &HashResults) -> (String, i32) {
        let rows = &results.rows;
        let done = rows.iter().filter(|r| r.state != RowState::Waiting).count();
        let files = if rows.len() == 1 { "1 file".to_owned() } else { format!("{} files", group(rows.len())) };
        let bytes: u64 = rows.iter().map(|r| r.size).sum();
        let running = self.job.get().is_some();
        let expected = self.expected.borrow();
        let typed = window_expected(&self.window);
        match &*expected {
            _ if running => (format!("Reading… {} of {files}", group(done)), 0),
            None if !typed.trim().is_empty() => ("Not a checksum: 8, 32, 40, 64 or 128 hex digits".to_owned(), 2),
            None => (format!("{files} · {}", crate::view_options::size_text(bytes)), 0),
            Some((algo, _)) if !self.algos.get()[algo.index()] => (format!("Check {} to compare", algo.name()), 0),
            Some((algo, hex)) => {
                let matches: Vec<&HashRow> = rows.iter().filter(|r| r.digest(*algo) == Some(hex.as_str())).collect();
                match (rows.len(), matches.as_slice()) {
                    (1, [_]) => (format!("Matches the expected {}", algo.name()), 1),
                    (1, _) => (format!("Does not match the expected {}", algo.name()), 2),
                    (_, []) => (format!("No file matches the expected {}", algo.name()), 2),
                    (_, [one]) => (format!("{} matches the expected {}", one.name, algo.name()), 1),
                    (_, many) => (format!("{} files match the expected {}", many.len(), algo.name()), 1),
                }
            }
        }
    }

    fn verify_summary(&self, results: &HashResults) -> (String, i32) {
        if let Some(error) = &results.error {
            return (format!("Cannot read the checksum file: {error}"), 2);
        }
        let count = |v: Verdict| results.rows.iter().filter(|r| r.verdict() == v).count();
        let mut parts = vec![format!("{} OK", group(count(Verdict::Ok)))];
        let failed = count(Verdict::Failed);
        let missing = count(Verdict::Missing);
        let unreadable = count(Verdict::Unreadable);
        let online = count(Verdict::OnlineOnly);
        for (n, what) in [(failed, "failed"), (missing, "missing"), (unreadable, "unreadable"), (online, "online only")]
        {
            if n > 0 {
                parts.push(format!("{} {what}", group(n)));
            }
        }
        match results.bad_lines {
            0 => {}
            1 => parts.push("1 line could not be read".to_owned()),
            n => parts.push(format!("{} lines could not be read", group(n))),
        }
        let text = parts.join(" · ");
        if self.job.get().is_some() {
            return (format!("Verifying… {text}"), 0);
        }
        let bad = failed + missing + unreadable > 0 || results.bad_lines > 0;
        (
            text,
            if bad {
                2
            } else if results.rows.is_empty() {
                0
            } else {
                1
            },
        )
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
            // Only what the rows lack is read: a running job stops and one for the rest starts.
            self.cancel_job();
            self.submit(Box::new(HashTask::again(self.ticked(), self.results.clone())));
        }
    }

    fn cancel_job(&self) {
        if let Some(job) = self.job.take() {
            crate::operations::with_current(|ops| ops.cancel(i32::try_from(job).unwrap_or(-1)));
        }
    }

    fn expected_edited(&self) {
        let typed = window_expected(&self.window);
        *self.expected.borrow_mut() = checksum_file::clipboard_hex(&typed);
        self.rebuild();
    }

    fn toggle_only_failed(&self) {
        self.only_failed.set(!self.only_failed.get());
        if let Some(window) = self.window.upgrade() {
            window.set_ck_only_failed(self.only_failed.get());
        }
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
        self.show_selection();
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
            window.set_ck_summary(text.into());
            window.set_ck_summary_tone(0);
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
            return self.say("Nothing to copy yet");
        }
        match gezik_platform::clipboard::write_text(&text) {
            Ok(()) => self.say(&format!("Copied {}", lines_text(text.trim_end().lines().count()))),
            Err(_) => self.say("Cannot use the clipboard"),
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
            return self.say("No checksums to save yet");
        }
        let files: Vec<String> = algos.iter().map(|a| format!("{}.{}", self.save_name, a.extension())).collect();
        let message = format!("Writes {} in {}", files.join(", "), self.base.display());
        let dir = self.base.clone();
        let texts: Vec<(String, Vec<u8>)> = algos
            .iter()
            .map(|algo| {
                let lines = rows.iter().filter_map(|r| Some((r.name.as_str(), r.digest(*algo)?)));
                let text = if *algo == Algo::Crc32 {
                    checksum_file::write_sfv(lines)
                } else {
                    checksum_file::write_gnu(lines)
                };
                (algo.extension().to_owned(), text.into_bytes())
            })
            .collect();
        crate::operations::with_current(|ops| {
            let ops2 = ops.clone();
            ops.dialogs().ask_text(
                "Save Checksums",
                message,
                self.save_name.clone(),
                &["Save", "Cancel"],
                move |name| {
                    let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
                    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
                    for (ext, bytes) in texts {
                        let file = format!("{name}.{ext}");
                        if let Err(err) =
                            gezik_core::ops::names::validate_name(&file, gezik_core::ops::names::NameRules::current())
                        {
                            return with_current(|c| c.say(&err.to_string()));
                        }
                        tasks.push(Box::new(NewTask::asking(&dir, &file, move || Ok(bytes.clone()))));
                    }
                    ops2.submit_chain(tasks, Some("Save checksums".to_owned()), None, After::Select);
                },
            );
        });
    }

    /// Export CSV: the rows shown (or selected) with their values (spec 11 §4.1).
    fn export(&self) {
        let rows = self.chosen();
        if rows.is_empty() {
            return self.say("Nothing to export yet");
        }
        let mut text = String::new();
        if cfg!(windows) {
            // Excel opens UTF-8 names right only with the BOM (spec 11 §11.2).
            text.push('\u{feff}');
        }
        if self.verify {
            gezik_core::csv::push_row(&mut text, &["Path", "Size", "Algorithm", "Expected", "Value", "Status"], ',');
            for r in &rows {
                let (algo, hex) = r.expected.clone().unwrap_or((Algo::Sha256, String::new()));
                let value = r.digest(algo).unwrap_or_default().to_owned();
                let fields = [
                    r.name.clone(),
                    r.size.to_string(),
                    algo.name().to_owned(),
                    hex,
                    value,
                    r.verdict().label().to_owned(),
                ];
                gezik_core::csv::push_row(&mut text, &fields, ',');
            }
        } else {
            let algos = self.ticked();
            let mut header = vec!["Path".to_owned(), "Size".to_owned()];
            header.extend(algos.iter().map(|a| a.name().to_owned()));
            gezik_core::csv::push_row(&mut text, &header, ',');
            for r in &rows {
                let mut fields = vec![r.name.clone(), r.size.to_string()];
                fields.extend(algos.iter().map(|a| r.digest(*a).unwrap_or_default().to_owned()));
                gezik_core::csv::push_row(&mut text, &fields, ',');
            }
        }
        let initial = if self.verify {
            format!("{} results.csv", self.save_name)
        } else {
            format!("{} checksums.csv", self.save_name)
        };
        let message = format!("Writes the list in {}", self.base.display());
        let dir = self.base.clone();
        let bytes = text.into_bytes();
        crate::operations::with_current(|ops| {
            let ops2 = ops.clone();
            ops.dialogs().ask_text("Export CSV", message, initial, &["Save", "Cancel"], move |name| {
                let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
                if let Err(err) =
                    gezik_core::ops::names::validate_name(&name, gezik_core::ops::names::NameRules::current())
                {
                    return with_current(|c| c.say(&err.to_string()));
                }
                let task = NewTask::asking(&dir, &name, move || Ok(bytes.clone()));
                ops2.submit(Box::new(task), None, After::Select);
            });
        });
    }

    /// Esc closes and Ctrl+C / Ctrl+A (⌘ on macOS) copy and select, unless the Expected field
    /// has the keyboard; returns whether the key was used.
    pub fn chord(&self, chord: &Chord, field_focused: bool) -> bool {
        let primary = crate::keys::is_primary(chord, Platform::current());
        match chord.key {
            Key::Escape if !primary && !chord.shift && !chord.alt => self.close(),
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
            window.set_ck_open(false);
            window.set_ck_rows(ModelRc::default());
            window.set_ck_algos(ModelRc::default());
            window.set_ck_expected(SharedString::new());
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }
}

fn window_expected(window: &slint::Weak<AppWindow>) -> String {
    window.upgrade().map(|w| w.get_ck_expected().to_string()).unwrap_or_default()
}

/// `1,234`.
fn group(n: usize) -> String {
    crate::preview::with_commas(n)
}

fn lines_text(n: usize) -> String {
    if n == 1 { "1 line".to_owned() } else { format!("{} lines", group(n)) }
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
