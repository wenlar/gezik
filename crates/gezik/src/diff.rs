//! The file compare (11c, spec 11 §6): `compare` on two files shows them side by side, changed
//! lines and the changed part of each changed line marked. Read only. The files are read and
//! lined up on a thread; the layer exists only while open (`if root.df-open`) and its rows are
//! built only for the lines on screen. Binary files, files over 16 MB or over 1,000,000 lines
//! are compared byte by byte instead. A compare program in `[tools] diff` opens instead of all
//! this.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gezik_config::settings::ToolsSettings;
use gezik_config::shortcuts::{Chord, Key, Platform};
use gezik_config::store::ConfigStore;
use gezik_core::diff::{self, Kind, Options, Row};
use gezik_i18n::{t, tn, tx};
use slint::{ComponentHandle, Model, ModelRc, ModelTracker};

use crate::dialog::Dialogs;
use crate::{AppWindow, DiffRow, DiffView};

pub const SELECT_TWO: &str = tx!("Select two files or two folders, or open the second pane (F3)");
pub const NEEDS_TWO: &str = tx!("Compare needs two files or two folders");
pub const TOO_DIFFERENT: &str = tx!("The files are too different to line up; [tools] diff can name a compare program");

/// Larger files are compared byte by byte (spec 11 §6.1).
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_LINES: usize = 1_000_000;
/// A longer line is cut (with `…`) on screen; it is compared whole.
const SHOWN_CHARS: usize = 4000;
const TAB: &str = "    ";
/// The byte compare's read size, per file.
const CHUNK: usize = 1024 * 1024;

/// What `compare` compares (spec 11 §7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Targets {
    Files(PathBuf, PathBuf),
    Folders(PathBuf, PathBuf),
    /// Nothing to compare: what the status bar says.
    Say(&'static str),
}

/// The two items to compare: two selected in the active pane (in list order), else with two
/// panes one selected in each (the left pane's on the left), else the two panes' folders.
/// `active` and `other` are the panes' selections; `other` is `None` with one pane. `folders`:
/// the folders the two panes show (left, right), when both show a folder.
pub fn targets(
    active: &[(PathBuf, bool)],
    other: Option<&[(PathBuf, bool)]>,
    active_left: bool,
    folders: Option<(PathBuf, PathBuf)>,
) -> Targets {
    let pair = |(a, a_dir): &(PathBuf, bool), (b, b_dir): &(PathBuf, bool)| match (a_dir, b_dir) {
        (false, false) => Targets::Files(a.clone(), b.clone()),
        (true, true) => Targets::Folders(a.clone(), b.clone()),
        _ => Targets::Say(NEEDS_TWO),
    };
    match (active, other) {
        ([a, b], _) => pair(a, b),
        ([mine], Some([theirs])) if active_left => pair(mine, theirs),
        ([mine], Some([theirs])) => pair(theirs, mine),
        (_, Some(_)) => folders.map_or(Targets::Say(NEEDS_TWO), |(left, right)| Targets::Folders(left, right)),
        (_, None) => Targets::Say(SELECT_TWO),
    }
}

/// The compare program and what it gets (`[tools]`).
#[derive(Debug, Clone, Default)]
struct External {
    program: Option<String>,
    args: Vec<String>,
    folders: bool,
}

thread_local! {
    pub(crate) static CURRENT: crate::windows::Slot<Diff> = const { crate::windows::Slot::new() };
    static EXTERNAL: RefCell<External> = RefCell::default();
    /// The last options, one for the process: the first window reads them from state.toml.
    static OPTIONS: Cell<Option<Options>> = const { Cell::new(None) };
}

/// settings.toml changed.
pub fn set_settings(tools: &ToolsSettings) {
    EXTERNAL.with(|e| {
        *e.borrow_mut() =
            External { program: tools.diff.clone(), args: tools.diff_args.clone(), folders: tools.folder_diff }
    });
}

/// Runs `f` with this UI thread's compare layer, if set up.
pub fn with_current(f: impl FnOnce(&Diff)) {
    if let Some(diff) = CURRENT.with(crate::windows::Slot::current) {
        f(&diff);
    }
}

/// What the panes' selections give now.
fn current_targets() -> Targets {
    let view = crate::panes::active_view();
    if view.shows_drives() || view.shows_trash() {
        return Targets::Say(NEEDS_TWO);
    }
    let active = view.selected_items();
    let active_left = crate::panes::active_index() == 0;
    let usable = |v: &crate::view::View| !v.shows_drives() && !v.shows_trash();
    let other = (crate::panes::count() == 2).then(|| crate::panes::at(i32::from(active_left))).flatten();
    let selected = other.as_ref().map(|p| if usable(&p.view) { p.view.selected_items() } else { Vec::new() });
    // A search result, This PC or the trash is no folder to compare.
    let folder =
        |p: &crate::panes::Pane| (usable(&p.view) && !p.view.shows_results()).then(|| p.view.folder()).flatten();
    let folders = match (crate::panes::at(0), crate::panes::at(1)) {
        (Some(l), Some(r)) => folder(&l).zip(folder(&r)),
        _ => None,
    };
    targets(&active, selected.as_deref(), active_left, folders)
}

/// Whether the right-click menu offers `Tools ▸ Compare` (two files or two folders to compare).
pub fn offered() -> bool {
    !matches!(current_targets(), Targets::Say(_))
}

/// `compare`.
pub fn compare() {
    let view = crate::panes::active_view();
    match current_targets() {
        Targets::Files(left, right) => with_current(|d| d.open(left, right)),
        Targets::Folders(left, right) => {
            let external = EXTERNAL.with(|e| e.borrow().clone());
            if !(external.folders && run_external(&external, &left, &right, &view)) {
                crate::compare::open(left, right, String::new());
            }
        }
        Targets::Say(text) => view.note(t("", text).into_owned()),
    }
}

/// The program and arguments that open the compare program on `left` and `right`: a macOS
/// `.app` through `open -n -a <app> --args …` (`mac`).
pub fn external_command(
    program: &str,
    args: &[String],
    left: &Path,
    right: &Path,
    mac: bool,
) -> Result<(String, Vec<OsString>), String> {
    let args = gezik_core::batch::convert::expand_diff_args(args, left, right)?;
    let app = program.trim_end_matches('/');
    if mac && app.to_ascii_lowercase().ends_with(".app") {
        let mut all: Vec<OsString> = ["-n", "-a", app, "--args"].into_iter().map(OsString::from).collect();
        all.extend(args);
        return Ok(("open".to_owned(), all));
    }
    Ok((program.to_owned(), args))
}

/// The status bar's text when the compare program did not start; `None`: it was not found.
fn external_error(program: &str, err: &gezik_platform::terminal::TerminalError) -> Option<String> {
    use gezik_platform::terminal::TerminalError;
    match err {
        TerminalError::NotFound => None,
        TerminalError::UnsafeFolder(cmd) => Some(t!("These names can't be passed safely to {}", cmd)),
        // TRANSLATORS: {0} is a program, {1} why it did not start.
        other => Some(t!("Cannot start {0}: {1}", program, other)),
    }
}

/// The text when the compare program is not there: Gezik's own compare opens.
pub fn not_found_text(program: &str) -> String {
    t!("The compare program \"{}\" was not found; using Gezik's own", program)
}

/// A macOS app (`….app`) that is not there: `open -a` would only say so in a dialog of its own.
pub fn app_missing(program: &str, mac: bool) -> bool {
    let app = program.trim_end_matches('/');
    mac && app.to_ascii_lowercase().ends_with(".app") && !Path::new(app).is_dir()
}

/// Starts the compare program on a thread (it looks through PATH); `false` when there is none
/// set. Not found: the status bar says so (files: the own compare opens then).
fn run_external(external: &External, left: &Path, right: &Path, view: &crate::view::View) -> bool {
    let Some(program) = external.program.clone() else { return false };
    let mac = cfg!(target_os = "macos");
    let command = external_command(&program, &external.args, left, right, mac);
    let (name, args) = match command {
        Ok(command) => command,
        Err(err) => {
            // TRANSLATORS: "[tools] diff-args" is a setting's name in settings.toml: keep it.
            view.note(t!("[tools] diff-args: {}", err));
            return true;
        }
    };
    let dir = left.parent().map(Path::to_path_buf).unwrap_or_default();
    let (left, right) = (left.to_path_buf(), right.to_path_buf());
    let window_id = crate::windows::current();
    let shown = program.clone();
    let started = std::thread::Builder::new().name("gezik-compare-program".into()).spawn(move || {
        let started = if app_missing(&program, mac) {
            Err(gezik_platform::terminal::TerminalError::NotFound)
        } else {
            gezik_platform::terminal::start_program(&name, args, &dir)
        };
        let Err(err) = started else { return };
        crate::windows::post(window_id, move || match external_error(&program, &err) {
            Some(text) => crate::panes::active_view().note(text),
            None => {
                let text = not_found_text(&program);
                if left.is_dir() {
                    crate::compare::open(left, right, text);
                } else {
                    // Told where the compare's answer shows: the layer's line or the question.
                    with_current(|d| {
                        *d.0.told.borrow_mut() = text;
                        d.read(left, right, false);
                    });
                }
            }
        });
    });
    if let Err(err) = started {
        view.note(t!("Cannot start {0}: {1}", shown, err));
    }
    true
}

/// One side's text: lines as byte ranges, so the text is held once.
pub struct Text {
    name: String,
    text: String,
    lines: Vec<(u32, u32)>,
    /// `UTF-8 · CRLF`.
    info: String,
}

impl Text {
    #[cfg(test)]
    pub fn new(name: String, text: String, encoding: &str) -> Text {
        let lines = line_ranges(&text);
        Text::with_lines(name, text, lines, encoding)
    }

    fn with_lines(name: String, text: String, lines: Vec<(u32, u32)>, encoding: &str) -> Text {
        let eol = eol_name(&text);
        let info = if eol.is_empty() { encoding.to_owned() } else { format!("{encoding} · {}", t("", eol)) };
        Text { name, text, lines, info }
    }

    fn line(&self, i: u32) -> &str {
        let (start, end) = self.lines[i as usize];
        &self.text[start as usize..end as usize]
    }

    fn all(&self) -> Vec<&str> {
        (0..self.lines.len() as u32).map(|i| self.line(i)).collect()
    }
}

/// Each line's byte range, without its end (`\r\n`, `\n` or `\r`); text after the last end is a
/// line, an end at the very end makes none.
pub fn line_ranges(text: &str) -> Vec<(u32, u32)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                out.push((start as u32, i as u32));
                start = i + 1;
            }
            b'\r' => {
                out.push((start as u32, i as u32));
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push((start as u32, bytes.len() as u32));
    }
    out
}

/// `CRLF`, `LF`, `CR`, `mixed`, or "" with no line end.
pub fn eol_name(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let cr = text.matches('\r').count() - crlf;
    match (crlf > 0, lf > 0, cr > 0) {
        (false, false, false) => "",
        (true, false, false) => "CRLF",
        (false, true, false) => "LF",
        (false, false, true) => "CR",
        // Line endings of more than one kind.
        _ => tx!("mixed"),
    }
}

/// What reading a file found.
enum Loaded {
    Text(String, Vec<(u32, u32)>, &'static str),
    /// Binary, too large, or too many lines: compared byte by byte.
    Bytes,
}

/// `path` opened, if it is a plain file (not a device or a pipe, which may never end), with
/// its size.
fn open_file(path: &Path) -> io::Result<(std::fs::File, u64)> {
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::other(t!("It is not a file").into_owned()));
    }
    Ok((file, meta.len()))
}

fn load(path: &Path) -> io::Result<Loaded> {
    let (file, len) = open_file(path)?;
    if len > MAX_BYTES {
        return Ok(Loaded::Bytes);
    }
    // At most one byte past the cap: a file that grew since is compared by bytes.
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Ok(Loaded::Bytes);
    }
    let Some((text, encoding)) = gezik_batch::convert::text::decode(bytes) else { return Ok(Loaded::Bytes) };
    let lines = line_ranges(&text);
    Ok(if lines.len() > MAX_LINES { Loaded::Bytes } else { Loaded::Text(text, lines, encoding) })
}

/// Where `a` and `b` first differ (a byte offset; the shorter one's end if one is the start of
/// the other), `None` when they are the same. Read up to the larger of their sizes when opened:
/// a file growing meanwhile does not keep it reading.
// shortcut: no progress while two large files are read; 11a's `hash::same_content` (with
// progress) replaces this once merged.
pub fn first_difference(a: &Path, b: &Path) -> io::Result<Option<u64>> {
    let ((fa, la), (fb, lb)) = (open_file(a)?, open_file(b)?);
    let limit = la.max(lb);
    first_difference_in(fa.take(limit), fb.take(limit))
}

/// `first_difference` on two readers.
fn first_difference_in(mut fa: impl Read, mut fb: impl Read) -> io::Result<Option<u64>> {
    let (mut ba, mut bb) = (vec![0u8; CHUNK], vec![0u8; CHUNK]);
    let mut offset = 0u64;
    loop {
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        let n = na.min(nb);
        if let Some(i) = (0..n).find(|&i| ba[i] != bb[i]) {
            return Ok(Some(offset + i as u64));
        }
        if na != nb {
            return Ok(Some(offset + n as u64));
        }
        if na == 0 {
            return Ok(None);
        }
        offset += n as u64;
    }
}

/// Reads until `buf` is full or the file ends; how much was read.
fn read_full(file: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(filled)
}

/// The byte compare's answer.
pub fn bytes_text(first: Option<u64>, size: u64) -> String {
    match first {
        None => t!("The files are identical").into_owned(),
        // TRANSLATORS: {0} is a byte's place, {1} the file's size.
        Some(at) => t!(
            "The files differ from byte {0} (of {1})",
            crate::preview::with_commas(usize::try_from(at + 1).unwrap_or(usize::MAX)),
            crate::view_options::size_text(size)
        ),
    }
}

/// Both sides and their rows, as the layer shows them.
pub struct Compared {
    texts: Arc<(Text, Text)>,
    rows: Vec<Row>,
    aligned: bool,
}

/// Lines `texts` up (on a thread).
pub fn line_up(texts: Arc<(Text, Text)>, options: Options) -> Compared {
    let (left, right) = (texts.0.all(), texts.1.all());
    let (rows, aligned) = match diff::diff(&left, &right, options) {
        Ok(rows) => (rows, true),
        Err(diff::TooDifferent) => (diff::unaligned(&left, &right, options), false),
    };
    drop((left, right));
    Compared { texts, rows, aligned }
}

/// A line as shown: tabs as four spaces, cut at `SHOWN_CHARS` characters.
pub fn shown(line: &str) -> String {
    let mut out = String::with_capacity(line.len().min(SHOWN_CHARS));
    for (count, c) in line.chars().enumerate() {
        if count == SHOWN_CHARS {
            out.push('…');
            break;
        }
        if c == '\t' {
            out.push_str(TAB);
        } else {
            out.push(c);
        }
    }
    out
}

/// A side as the layer draws it: `number` and `text` with the number in front, right-aligned
/// to `digits` (monospace columns), and the text before the changed part (`span`, bytes of
/// `text`) and the changed part itself. The layer measures those two with the font, so the
/// mark sits on the characters whatever their widths.
pub fn side_parts(number: u32, digits: usize, text: &str, span: Option<std::ops::Range<usize>>) -> [String; 3] {
    let front = format!("{number:>digits$}  ");
    let (before, changed) = match span {
        Some(span) if !span.is_empty() => (format!("{front}{}", &text[..span.start]), text[span].to_owned()),
        _ => (String::new(), String::new()),
    };
    [format!("{front}{text}"), before, changed]
}

/// What a screen reader says for a row.
pub fn row_label(kind: Kind, left: &str, right: &str) -> String {
    match kind {
        Kind::Same => left.to_owned(),
        Kind::Added => t!("Added: {}", right),
        Kind::Removed => t!("Removed: {}", left),
        // TRANSLATORS: a screen reader's text for a changed line: {0} the old line, {1} the new.
        Kind::Changed => t!("Changed: {0} to {1}", left, right),
    }
}

/// The rows the layer's list shows, each built when asked for (only those on screen).
struct RowsModel(Compared);

impl Model for RowsModel {
    type Data = DiffRow;

    fn row_count(&self) -> usize {
        self.0.rows.len()
    }

    fn row_data(&self, i: usize) -> Option<DiffRow> {
        let row = self.0.rows.get(i)?;
        let (left, right) = (&self.0.texts.0, &self.0.texts.1);
        let l = row.left.map(|n| shown(left.line(n))).unwrap_or_default();
        let r = row.right.map(|n| shown(right.line(n))).unwrap_or_default();
        let (ls, rs) = if row.kind == Kind::Changed {
            let (a, b) = diff::inline_span(&l, &r);
            (Some(a), Some(b))
        } else {
            (None, None)
        };
        let digits = left.lines.len().max(right.lines.len()).to_string().len();
        let side =
            |n: Option<u32>, text: &str, span| n.map(|n| side_parts(n + 1, digits, text, span)).unwrap_or_default();
        let [l_all, l_before, l_changed] = side(row.left, &l, ls);
        let [r_all, r_before, r_changed] = side(row.right, &r, rs);
        Some(DiffRow {
            label: row_label(row.kind, &l, &r).into(),
            l: l_all.into(),
            lp: l_before.into(),
            lc: l_changed.into(),
            r: r_all.into(),
            rp: r_before.into(),
            rc: r_changed.into(),
            kind: kind_index(row.kind),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        // Never changes: new rows are a new model.
        &()
    }
}

fn kind_index(kind: Kind) -> i32 {
    match kind {
        Kind::Same => 0,
        Kind::Changed => 1,
        Kind::Added => 2,
        Kind::Removed => 3,
    }
}

/// "3 changes".
pub fn changes_text(count: usize) -> String {
    match count {
        0 => t!("No changes").into_owned(),
        n => tn!("{} change" | "{} changes", n, crate::preview::with_commas(n)),
    }
}

/// The change after (`forward`) or before row `current`: the next or previous start.
pub fn step(changes: &[usize], current: usize, forward: bool) -> Option<usize> {
    if forward {
        changes.iter().copied().find(|&c| c > current)
    } else {
        changes.iter().copied().rev().find(|&c| c < current)
    }
}

/// What a key does in the layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Close,
    Reload,
    /// The next (true) or previous change.
    Step(bool),
    Copy,
    Move(Key),
}

pub fn key_action(chord: &Chord, platform: Platform) -> Option<KeyAction> {
    let primary = crate::keys::is_primary(chord, platform);
    let other = if platform == Platform::Mac { chord.ctrl } else { chord.meta };
    let plain = !primary && !other && !chord.alt && !chord.shift;
    let alt_only = chord.alt && !primary && !other && !chord.shift;
    let primary_only = primary && !other && !chord.alt && !chord.shift;
    match chord.key {
        Key::Escape if plain => Some(KeyAction::Close),
        Key::F(5) if plain => Some(KeyAction::Reload),
        Key::Char('r') if primary_only => Some(KeyAction::Reload),
        Key::Char('c') if primary_only => Some(KeyAction::Copy),
        Key::Down if alt_only => Some(KeyAction::Step(true)),
        Key::Up if alt_only => Some(KeyAction::Step(false)),
        Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End if plain => {
            Some(KeyAction::Move(chord.key))
        }
        _ => None,
    }
}

/// Row `current` moved by `key`, among `count` rows, `page` to a page.
pub fn moved(current: usize, key: Key, count: usize, page: usize) -> usize {
    let last = count.saturating_sub(1);
    match key {
        Key::Up => current.saturating_sub(1),
        Key::Down => (current + 1).min(last),
        Key::PageUp => current.saturating_sub(page.max(1)),
        Key::PageDown => (current + page.max(1)).min(last),
        Key::Home => 0,
        _ => last,
    }
}

/// Ctrl+C on a row: `< left` and `> right` (the sides it has).
pub fn row_text(left: Option<&str>, right: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(l) = left {
        out.push_str("< ");
        out.push_str(l);
    }
    if let Some(r) = right {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("> ");
        out.push_str(r);
    }
    out
}

struct State {
    paths: (PathBuf, PathBuf),
    compared: Rc<RowsModel>,
    changes: Vec<usize>,
    note: String,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    dialogs: Dialogs,
    store: Option<ConfigStore>,
    state: RefCell<Option<State>>,
    /// Bumped by each read and each close: a late answer is dropped.
    generation: Cell<u64>,
    /// The files are being read (the layer's reload): an option change reads them again too,
    /// so the reload's new text is not dropped.
    reading: Cell<bool>,
    /// Said with the next answer ("The compare program … was not found; using Gezik's own"):
    /// the status bar's note would go when the answer comes.
    told: RefCell<String>,
}

#[derive(Clone)]
pub struct Diff(Rc<Inner>);

impl Diff {
    pub fn new(window: &AppWindow, dialogs: Dialogs, store: Option<ConfigStore>, options: Options) -> Diff {
        if OPTIONS.with(Cell::get).is_none() {
            OPTIONS.with(|o| o.set(Some(options)));
        }
        let diff = Diff(Rc::new(Inner {
            window: window.as_weak(),
            dialogs,
            store,
            state: RefCell::default(),
            generation: Cell::new(0),
            reading: Cell::new(false),
            told: RefCell::default(),
        }));
        let d = diff.clone();
        window.on_df_button(move |k| if k == 2 { d.close() } else { d.toggle(k) });
        CURRENT.with(|c| c.set(crate::windows::current(), diff.clone()));
        diff
    }

    fn options() -> Options {
        OPTIONS.with(Cell::get).unwrap_or_default()
    }

    /// Opens the compare program if one is set, else this layer.
    pub fn open(&self, left: PathBuf, right: PathBuf) {
        let external = EXTERNAL.with(|e| e.borrow().clone());
        if run_external(&external, &left, &right, &crate::panes::active_view()) {
            return;
        }
        self.read(left, right, false);
    }

    /// Reads both files on a thread and lines them up (`reload`: the layer is open on them).
    fn read(&self, left: PathBuf, right: PathBuf, reload: bool) {
        let generation = self.0.generation.get() + 1;
        self.0.generation.set(generation);
        self.0.reading.set(true);
        if !reload {
            crate::panes::active_view().note(t!("Comparing…").into_owned());
        }
        let options = Diff::options();
        let window_id = crate::windows::current();
        let spawned = std::thread::Builder::new().name("gezik-compare".into()).spawn(move || {
            let answer = read_both(&left, &right, options);
            crate::windows::post(window_id, move || {
                with_current(|d| d.answer(generation, (left, right), answer));
            });
        });
        if let Err(err) = spawned {
            crate::panes::active_view().note(t!("Cannot compare: {}", err));
        }
    }

    fn answer(&self, generation: u64, paths: (PathBuf, PathBuf), answer: Answer) {
        if generation != self.0.generation.get() {
            return;
        }
        self.0.reading.set(false);
        let view = crate::panes::active_view();
        let told = std::mem::take(&mut *self.0.told.borrow_mut());
        // TRANSLATORS: {0} is what was said first (the compare program was not found), {1} the answer.
        let with_told = |text: String| if told.is_empty() { text } else { t!("{0}.\n{1}", told, text) };
        match answer {
            Answer::Failed(text) => view.note(with_told(text).replace('\n', " ")),
            Answer::Bytes(first, size) => {
                view.clear_note();
                // TRANSLATORS: {0} and {1} are file names, {2} the answer ("The files are identical").
                let text = t!("{0} and {1}\n{2}", file_name(&paths.0), file_name(&paths.1), bytes_text(first, size));
                self.0.dialogs.ask(t!("Compare"), with_told(text), &[&t!("OK")], |_| {});
            }
            Answer::Lines(compared) => {
                view.clear_note();
                self.show(paths, compared, told);
            }
        }
    }

    /// Shows `compared`; `told` (if any) on the layer's line with the encodings.
    fn show(&self, paths: (PathBuf, PathBuf), compared: Compared, told: String) {
        let Some(window) = self.0.window.upgrade() else { return };
        let changes = gezik_core::diff::changes(&compared.rows);
        let note = match (compared.aligned, told.is_empty()) {
            (true, _) => told,
            (false, true) => t("", TOO_DIFFERENT).into_owned(),
            (false, false) => format!("{told}  ·  {}", t("", TOO_DIFFERENT)),
        };
        let opening = self.0.state.borrow().is_none();
        let model = Rc::new(RowsModel(compared));
        window.set_df_rows(ModelRc::from(model.clone()));
        *self.0.state.borrow_mut() = Some(State { paths, compared: model, changes, note });
        self.show_view(&window);
        if opening {
            window.set_df_current(0);
            window.set_df_open(true);
        } else {
            let last = self.0.state.borrow().as_ref().map_or(0, |s| s.compared.row_count().saturating_sub(1));
            window.set_df_current(window.get_df_current().clamp(0, i32::try_from(last).unwrap_or(i32::MAX)));
        }
        window.invoke_df_reveal();
    }

    fn show_view(&self, window: &AppWindow) {
        let state = self.0.state.borrow();
        let Some(state) = state.as_ref() else { return };
        let texts = &state.compared.0.texts;
        let options = Diff::options();
        let mut info = format!("{}  ↔  {}", texts.0.info, texts.1.info);
        if !state.note.is_empty() {
            info = format!("{info}  ·  {}", state.note);
        }
        window.set_df_view(DiffView {
            title: t!("Compare  {}  ↔  {}  ·  {}", texts.0.name, texts.1.name, changes_text(state.changes.len()))
                .into(),
            info: info.into(),
            marked: texts.0.info != texts.1.info || !state.note.is_empty(),
            ignore_whitespace: options.ignore_whitespace,
            ignore_case: options.ignore_case,
        });
    }

    /// Ignore whitespace (0) or case (1) turned on or off: the lines are lined up again.
    fn toggle(&self, which: i32) {
        let mut options = Diff::options();
        if which == 0 {
            options.ignore_whitespace = !options.ignore_whitespace;
        } else {
            options.ignore_case = !options.ignore_case;
        }
        OPTIONS.with(|o| o.set(Some(options)));
        if let Some(store) = &self.0.store {
            store.update_state(move |state| state.diff = options);
        }
        let Some((texts, paths)) =
            self.0.state.borrow().as_ref().map(|s| (s.compared.0.texts.clone(), s.paths.clone()))
        else {
            return;
        };
        if let Some(window) = self.0.window.upgrade() {
            self.show_view(&window);
        }
        if self.0.reading.get() {
            // A reload is on its way: read again with the new options.
            return self.read(paths.0, paths.1, true);
        }
        let generation = self.0.generation.get() + 1;
        self.0.generation.set(generation);
        let window_id = crate::windows::current();
        let _ = std::thread::Builder::new().name("gezik-compare".into()).spawn(move || {
            let compared = line_up(texts, options);
            crate::windows::post(window_id, move || {
                with_current(|d| d.answer(generation, paths, Answer::Lines(compared)));
            });
        });
    }

    fn step(&self, forward: bool) {
        let Some(window) = self.0.window.upgrade() else { return };
        let current = usize::try_from(window.get_df_current()).unwrap_or(0);
        let next = self.0.state.borrow().as_ref().and_then(|s| step(&s.changes, current, forward));
        if let Some(next) = next {
            window.set_df_current(i32::try_from(next).unwrap_or(0));
            window.invoke_df_reveal();
        }
    }

    /// The layer's keys, wherever its focus is; returns whether the key was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        let Some(action) = key_action(chord, Platform::current()) else { return false };
        let Some(window) = self.0.window.upgrade() else { return false };
        match action {
            KeyAction::Close => self.close(),
            KeyAction::Reload => {
                if let Some(paths) = self.0.state.borrow().as_ref().map(|s| s.paths.clone()) {
                    self.read(paths.0, paths.1, true);
                }
            }
            KeyAction::Step(forward) => self.step(forward),
            KeyAction::Copy => self.copy_row(&window),
            KeyAction::Move(key) => {
                let count = self.0.state.borrow().as_ref().map_or(0, |s| s.compared.row_count());
                let row = window.window().scale_factor() * window.global::<crate::Theme>().get_row_height().max(1.0);
                let page = (window.window().size().height as f32 / row) as usize;
                let current = usize::try_from(window.get_df_current()).unwrap_or(0);
                // The header and the window's edges take about four rows.
                let next = moved(current, key, count, page.saturating_sub(4));
                window.set_df_current(i32::try_from(next).unwrap_or(0));
                window.invoke_df_reveal();
            }
        }
        true
    }

    fn copy_row(&self, window: &AppWindow) {
        let state = self.0.state.borrow();
        let Some(state) = state.as_ref() else { return };
        let current = usize::try_from(window.get_df_current()).unwrap_or(0);
        let Some(row) = state.compared.0.rows.get(current) else { return };
        let texts = &state.compared.0.texts;
        let text = row_text(row.left.map(|n| texts.0.line(n)), row.right.map(|n| texts.1.line(n)));
        if let Err(err) = gezik_platform::clipboard::write_text(&text) {
            crate::panes::active_view().note(t!("Cannot copy: {}", err));
        }
    }

    /// Closes the layer: its texts and rows go.
    pub fn close(&self) {
        self.0.generation.set(self.0.generation.get() + 1);
        self.0.reading.set(false);
        let state = self.0.state.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            window.set_df_open(false);
            window.set_df_rows(ModelRc::default());
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
        drop(state);
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// What the reading thread found.
enum Answer {
    Lines(Compared),
    /// Compared byte by byte: where they first differ, and the left one's size.
    Bytes(Option<u64>, u64),
    Failed(String),
}

fn read_both(left: &Path, right: &Path, options: Options) -> Answer {
    let failed = |path: &Path, err: io::Error| {
        Answer::Failed(t!("Cannot read {}: {}", file_name(path), gezik_platform::fs::describe(&err)))
    };
    let a = match load(left) {
        Ok(loaded) => loaded,
        Err(err) => return failed(left, err),
    };
    // The other one is read only if this one is text.
    let b = match &a {
        Loaded::Bytes => Loaded::Bytes,
        Loaded::Text(..) => match load(right) {
            Ok(loaded) => loaded,
            Err(err) => return failed(right, err),
        },
    };
    match (a, b) {
        (Loaded::Text(a, la, ea), Loaded::Text(b, lb, eb)) => {
            let texts =
                Arc::new((Text::with_lines(file_name(left), a, la, ea), Text::with_lines(file_name(right), b, lb, eb)));
            Answer::Lines(line_up(texts, options))
        }
        _ => {
            let size = std::fs::metadata(left).map(|m| m.len()).unwrap_or(0);
            match first_difference(left, right) {
                Ok(first) => Answer::Bytes(first, size),
                Err(err) => Answer::Failed(t!("Cannot compare: {}", gezik_platform::fs::describe(&err))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, bool)]) -> Vec<(PathBuf, bool)> {
        list.iter().map(|(p, d)| (PathBuf::from(p), *d)).collect()
    }

    #[test]
    fn the_selection_says_what_to_compare() {
        let files = Targets::Files("/a".into(), "/b".into());
        assert_eq!(targets(&items(&[("/a", false), ("/b", false)]), None, true, None), files);
        assert_eq!(
            targets(&items(&[("/a", true), ("/b", true)]), None, true, None),
            Targets::Folders("/a".into(), "/b".into())
        );
        assert_eq!(targets(&items(&[("/a", false), ("/b", true)]), None, true, None), Targets::Say(NEEDS_TWO));
        assert_eq!(targets(&items(&[("/a", false)]), None, true, None), Targets::Say(SELECT_TWO));
        assert_eq!(
            targets(&items(&[("/a", false), ("/b", false), ("/c", false)]), None, true, None),
            Targets::Say(SELECT_TWO)
        );
        // Two panes: one in each, the left pane's on the left.
        let theirs = items(&[("/b", false)]);
        assert_eq!(targets(&items(&[("/a", false)]), Some(&theirs), true, None), files);
        assert_eq!(
            targets(&items(&[("/a", false)]), Some(&theirs), false, None),
            Targets::Files("/b".into(), "/a".into())
        );
        assert_eq!(targets(&items(&[("/a", true)]), Some(&theirs), true, None), Targets::Say(NEEDS_TWO));
        // Two panes, anything else: the two folders shown (spec 11 §7.1), the left pane's first.
        let shown = Some((PathBuf::from("/l"), PathBuf::from("/r")));
        let folders = Targets::Folders("/l".into(), "/r".into());
        assert_eq!(targets(&items(&[]), Some(&theirs), true, shown.clone()), folders);
        assert_eq!(targets(&items(&[]), Some(&[]), false, shown.clone()), folders);
        let three = items(&[("/a", false), ("/b", false), ("/c", false)]);
        assert_eq!(targets(&three, Some(&theirs), true, shown.clone()), folders);
        assert_eq!(
            targets(&items(&[("/a", true)]), Some(&theirs), true, shown),
            Targets::Say(NEEDS_TWO),
            "file and folder"
        );
        // A pane on search results, This PC or the trash: no folders.
        assert_eq!(targets(&items(&[]), Some(&theirs), true, None), Targets::Say(NEEDS_TWO));
        // Two in the active pane win over the other pane's.
        assert_eq!(targets(&items(&[("/a", false), ("/b", false)]), Some(&items(&[("/c", false)])), true, None), files);
    }

    #[test]
    fn lines_split_at_every_kind_of_end() {
        let text = "a\r\nb\nc\rd";
        let lines: Vec<&str> = line_ranges(text).iter().map(|&(s, e)| &text[s as usize..e as usize]).collect();
        assert_eq!(lines, ["a", "b", "c", "d"]);
        assert_eq!(line_ranges("a\n").len(), 1, "an end at the end makes no line");
        assert_eq!(line_ranges("\n\n").len(), 2);
        assert!(line_ranges("").is_empty());
        assert_eq!(
            (eol_name("a\r\nb\r\n"), eol_name("a\nb"), eol_name("a\rb"), eol_name("a"), eol_name("a\r\nb\n")),
            ("CRLF", "LF", "CR", "", "mixed")
        );
    }

    #[test]
    fn line_ends_are_not_differences_but_are_told() {
        let left = Text::new("a.txt".into(), "one\r\ntwo\r\n".into(), "UTF-8");
        let right = Text::new("b.txt".into(), "one\ntwo".into(), "UTF-8");
        assert_eq!((left.info.as_str(), right.info.as_str()), ("UTF-8 · CRLF", "UTF-8 · LF"));
        let compared = line_up(Arc::new((left, right)), Options::default());
        assert!(compared.aligned && compared.rows.iter().all(|r| r.kind == Kind::Same) && compared.rows.len() == 2);
        let model = RowsModel(compared);
        let row = model.row_data(1).unwrap();
        assert_eq!((row.l.as_str(), row.r.as_str(), row.kind), ("2  two", "2  two", 0));
        assert_eq!(row.label.as_str(), "two");
    }

    #[test]
    fn a_changed_row_shows_its_changed_part() {
        let left = Text::new("a".into(), "the lazy dog\nsame\n".into(), "UTF-8");
        let right = Text::new("b".into(), "the lazy cat\nsame\nnew\n".into(), "UTF-8");
        let model = RowsModel(line_up(Arc::new((left, right)), Options::default()));
        let row = model.row_data(0).unwrap();
        assert_eq!((row.l.as_str(), row.lp.as_str(), row.lc.as_str()), ("1  the lazy dog", "1  the lazy ", "dog"));
        assert_eq!((row.rp.as_str(), row.rc.as_str(), row.kind), ("1  the lazy ", "cat", 1));
        assert!(row.l.starts_with(&format!("{}{}", row.lp, row.lc)), "the measured parts are the drawn text's start");
        assert_eq!(row.label.as_str(), "Changed: the lazy dog to the lazy cat");
        let added = model.row_data(2).unwrap();
        assert_eq!((added.l.as_str(), added.r.as_str(), added.kind), ("", "3  new", 2));
        assert_eq!(added.label.as_str(), "Added: new");
        let [all, before, changed] = side_parts(7, 3, "çay ılık", Some(5..11));
        assert_eq!((all.as_str(), before.as_str(), changed.as_str()), ("  7  çay ılık", "  7  çay ", "ılık"));
        assert_eq!(side_parts(7, 1, "same", None), ["7  same".to_owned(), String::new(), String::new()]);
        assert_eq!(
            side_parts(7, 1, "x", Some(1..1)),
            ["7  x".to_owned(), String::new(), String::new()],
            "nothing changed: no mark"
        );
        assert_eq!(gezik_core::diff::changes(&model.0.rows), [0, 2]);
    }

    #[test]
    fn lines_are_shown_with_tabs_opened_and_cut() {
        assert_eq!(shown("\ta\tb"), "    a    b");
        let long = "x".repeat(SHOWN_CHARS + 10);
        let cut = shown(&long);
        assert_eq!(cut.chars().count(), SHOWN_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(shown(&"ş".repeat(SHOWN_CHARS)), "ş".repeat(SHOWN_CHARS), "exactly the limit: whole");
    }

    #[test]
    fn binary_and_large_files_are_compared_byte_by_byte() {
        let dir = std::env::temp_dir().join(format!("gezik-diff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b, c) = (dir.join("a.bin"), dir.join("b.bin"), dir.join("c.bin"));
        let mut bytes = vec![0u8; CHUNK + 100];
        std::fs::write(&a, &bytes).unwrap();
        std::fs::write(&c, &bytes).unwrap();
        bytes[CHUNK + 7] = 1;
        std::fs::write(&b, &bytes).unwrap();
        assert_eq!(first_difference(&a, &c).unwrap(), None);
        assert_eq!(first_difference(&a, &b).unwrap(), Some(CHUNK as u64 + 7), "past the first read");
        std::fs::write(&c, &bytes[..50]).unwrap();
        assert_eq!(first_difference(&a, &c).unwrap(), Some(50), "one is the other's start");
        match read_both(&a, &b, Options::default()) {
            Answer::Bytes(first, size) => assert_eq!((first, size), (Some(CHUNK as u64 + 7), CHUNK as u64 + 100)),
            _ => panic!("NUL bytes: binary"),
        }
        assert_eq!(bytes_text(None, 10), "The files are identical");
        assert_eq!(
            bytes_text(Some(1_234_566), 2_100_000),
            format!("The files differ from byte 1,234,567 (of {})", crate::view_options::size_text(2_100_000))
        );
        // Over 16 MB: bytes, without reading it as text.
        let big = dir.join("big.txt");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        drop(file);
        assert!(matches!(load(&big).unwrap(), Loaded::Bytes));
        // Text in other encodings is text.
        std::fs::write(&a, [0xFF, 0xFE, b'a', 0, b'\n', 0]).unwrap();
        assert!(matches!(load(&a).unwrap(), Loaded::Text(ref t, _, "UTF-16 LE") if t == "a\n"));
        // Over the line cap with lone CRs only: bytes.
        std::fs::write(&a, "a\r".repeat(MAX_LINES + 1)).unwrap();
        assert!(matches!(load(&a).unwrap(), Loaded::Bytes));
        std::fs::write(&a, "a\r".repeat(10)).unwrap();
        assert!(matches!(load(&a).unwrap(), Loaded::Text(_, ref lines, _) if lines.len() == 10));
        // A folder (a device or a pipe likewise) is no file: never read on and on.
        assert!(load(&dir).is_err() && first_difference(&dir, &a).is_err());
        // Endless input stops at the bound it is given.
        let endless = || std::io::repeat(7).take(3 * CHUNK as u64 + 5);
        assert_eq!(first_difference_in(endless(), endless()).unwrap(), None);
        assert_eq!(first_difference_in(endless(), std::io::repeat(7).take(10)).unwrap(), Some(10));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_compare_program_gets_both_paths() {
        let args = vec!["{left}".to_owned(), "{right}".to_owned()];
        let (left, right) = (Path::new("/a b/x.txt"), Path::new("/c/y.txt"));
        let (program, got) = external_command("meld", &args, left, right, false).unwrap();
        assert_eq!((program.as_str(), got), ("meld", vec![OsString::from("/a b/x.txt"), "/c/y.txt".into()]));
        let (program, got) = external_command("/Applications/Beyond Compare.app/", &args, left, right, true).unwrap();
        assert_eq!(program, "open");
        assert_eq!(got, ["-n", "-a", "/Applications/Beyond Compare.app", "--args", "/a b/x.txt", "/c/y.txt"]);
        let (program, _) = external_command("opendiff", &args, left, right, true).unwrap();
        assert_eq!(program, "opendiff", "a program on PATH stays itself on macOS");
        assert!(external_command("x", &["{in}".to_owned()], left, right, false).is_err());
        use gezik_platform::terminal::TerminalError;
        assert_eq!(external_error("meld", &TerminalError::NotFound), None);
        assert_eq!(not_found_text("meld"), "The compare program \"meld\" was not found; using Gezik's own");
        assert!(app_missing("/Applications/Gezik No Such Compare.app/", true));
        assert!(!app_missing("/Applications/Gezik No Such Compare.app", false), "not macOS");
        assert!(!app_missing("opendiff", true), "a name on PATH is looked up as usual");
        assert_eq!(
            external_error("x", &TerminalError::UnsafeFolder("diff.cmd".into())).as_deref(),
            Some("These names can't be passed safely to diff.cmd")
        );
    }

    #[test]
    fn keys_move_step_copy_and_close() {
        let chord = |key, ctrl, alt, meta| Chord { ctrl, alt, shift: false, meta, key };
        let other = Platform::Other;
        assert_eq!(key_action(&chord(Key::Escape, false, false, false), other), Some(KeyAction::Close));
        assert_eq!(key_action(&chord(Key::F(5), false, false, false), other), Some(KeyAction::Reload));
        assert_eq!(key_action(&chord(Key::Char('r'), true, false, false), other), Some(KeyAction::Reload));
        assert_eq!(
            key_action(&chord(Key::Char('r'), false, false, true), Platform::Mac),
            Some(KeyAction::Reload),
            "⌘R"
        );
        assert_eq!(key_action(&chord(Key::Down, false, true, false), other), Some(KeyAction::Step(true)));
        assert_eq!(key_action(&chord(Key::Up, false, true, false), Platform::Mac), Some(KeyAction::Step(false)), "⌥↑");
        assert_eq!(key_action(&chord(Key::Char('c'), true, false, false), other), Some(KeyAction::Copy));
        assert_eq!(key_action(&chord(Key::PageDown, false, false, false), other), Some(KeyAction::Move(Key::PageDown)));
        assert_eq!(key_action(&chord(Key::Char('a'), false, false, false), other), None);
        assert_eq!(key_action(&chord(Key::Char('c'), false, false, true), other), None, "Win+C is not Ctrl+C");
        assert_eq!(moved(5, Key::Up, 10, 3), 4);
        assert_eq!(moved(9, Key::Down, 10, 3), 9);
        assert_eq!((moved(5, Key::PageUp, 10, 3), moved(5, Key::PageDown, 10, 3)), (2, 8));
        assert_eq!((moved(5, Key::Home, 10, 3), moved(5, Key::End, 10, 3), moved(0, Key::End, 0, 3)), (0, 9, 0));
        assert_eq!((step(&[2, 8], 0, true), step(&[2, 8], 2, true), step(&[2, 8], 8, true)), (Some(2), Some(8), None));
        assert_eq!((step(&[2, 8], 8, false), step(&[2, 8], 2, false)), (Some(2), None));
        assert_eq!(row_text(Some("a"), Some("b")), "< a\n> b");
        assert_eq!(row_text(None, Some("b")), "> b");
        assert_eq!(
            (changes_text(0), changes_text(1), changes_text(1200)),
            ("No changes".into(), "1 change".into(), "1,200 changes".into())
        );
    }
}
