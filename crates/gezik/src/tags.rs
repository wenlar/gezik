//! Tags in the window (spec 12 §5): `[tags]` as in effect, the tag picker (the tab picker's
//! box, `widgets/picker.slint`), `tags`, `tag-1…7` and `clear-tags`, the rows' tag dots, and
//! the reads they need. Files are read only off the UI thread: the rows on screen through
//! `crate::media` while dots show, a selection when the picker opens or a tag key toggles.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use gezik_config::settings_writer::SettingsChange;
use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_config::store::ConfigStore;
use gezik_config::tags::{Dots, TagListEdit, TagsSettings};
use gezik_core::meta::{Meta, MetaEdit, Tag, TagColor, check_tag_name, same_name};
use gezik_core::pattern::fold_text;
use gezik_i18n::{t, tn};
use gezik_ops::meta::MetaStore;
use slint::{Color, ComponentHandle, ModelRc, VecModel};

use crate::view::View;
use crate::{AppWindow, PickRow};

/// The engine's tag store: what every read goes through (files and Gezik's own store).
static STORE: OnceLock<Arc<MetaStore>> = OnceLock::new();

#[derive(Default)]
struct State {
    settings: TagsSettings,
    /// A tag was written once (`state.toml` `[tags] used`).
    used: bool,
    config: Option<ConfigStore>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    pub(crate) static CURRENT: crate::windows::Slot<TagPicker> = const { crate::windows::Slot::new() };
}

/// Once, at start: the engine's store, where settings.toml and state.toml are, and whether a
/// tag was written before.
pub fn install(store: Arc<MetaStore>, config: Option<ConfigStore>, used: bool) {
    let _ = STORE.set(store);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.config = config;
        s.used = used;
    });
}

/// `[tags]` read (every resolve): rows are drawn again when the dots come or go.
pub fn set_settings(settings: TagsSettings) {
    let before = dots_on();
    let list_changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let changed = s.settings.list != settings.list;
        FORCED.store(settings.dots == Dots::On, Ordering::Relaxed);
        s.settings = settings;
        changed
    });
    if before != dots_on() || (list_changed && dots_on()) {
        redraw_all();
    }
    if list_changed {
        crate::windows::for_each(|_| crate::sidebar::with_current(crate::sidebar::Sidebar::tags_changed));
    }
    sync_window();
}

/// The list in effect.
pub fn list() -> Vec<Tag> {
    STATE.with(|s| s.borrow().settings.list.clone())
}

/// `f` with the list in effect, not copied (each row drawn).
pub fn with_list<R>(f: impl FnOnce(&[Tag]) -> R) -> R {
    STATE.with(|s| f(&s.borrow().settings.list))
}

/// Whether rows show tag dots now (`[tags] dots`; `auto`: macOS, or a tag was written).
pub fn dots_on() -> bool {
    STATE.with(|s| {
        let s = s.borrow();
        dots_shown(s.settings.dots, s.used, cfg!(target_os = "macos"))
    })
}

pub fn dots_shown(dots: Dots, used: bool, mac: bool) -> bool {
    match dots {
        Dots::On => true,
        Dots::Off => false,
        Dots::Auto => mac || used,
    }
}

/// `[tags] dots = true` (not `auto`): network and cloud folders are read too.
static FORCED: AtomicBool = AtomicBool::new(false);

pub fn dots_forced() -> bool {
    FORCED.load(Ordering::Relaxed)
}

thread_local! {
    /// The icon worker's last folder and whether it is on a network drive (asked once a folder).
    static NETWORK: RefCell<Option<(PathBuf, bool)>> = const { RefCell::new(None) };
}

/// A row's meta as drawn (its own record and Gezik's store).
#[derive(Debug, Default)]
pub struct RowMeta {
    pub meta: Meta,
}

/// What Explorer keeps in a file (`System.Keywords`, `System.Rating`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Explorer {
    /// Its Tags, parted by ", ".
    pub keywords: Box<str>,
    pub stars: u8,
}

/// Explorer's Tags and rating of `path`; none where Windows keeps none (or the handler hangs).
pub fn read_explorer(path: &Path) -> Explorer {
    #[cfg(windows)]
    if let Some((keywords, rating)) = gezik_platform::meta::explorer(path) {
        return Explorer { keywords: keywords.join(", ").into(), stars: gezik_core::meta::stars_from_windows(rating) };
    }
    let _ = path;
    Explorer::default()
}

/// A row's meta, on the icon worker (media.rs); a file that cannot be read has none, nor one on
/// a network drive unless dots are forced on (each read would cross the network).
pub fn read_for_rows(path: &Path) -> RowMeta {
    if !dots_forced() && on_network(path) {
        return RowMeta::default();
    }
    RowMeta { meta: read_meta(path) }
}

/// Explorer's reads (Windows, spec 12 §8.3): their own worker and small queue, so a slow
/// property handler never holds up icons and dots. Answers are kept by path until F5 or a tag
/// job; rows are drawn again as they come.
struct ExplorerReads {
    queue: std::collections::VecDeque<PathBuf>,
    known: std::collections::HashMap<PathBuf, Explorer>,
    started: bool,
}

/// The most queued: the newest win (rows scrolled past are not read).
const EXPLORER_QUEUE: usize = 64;
/// Answers kept; forgotten wholesale beyond this.
const EXPLORER_KNOWN: usize = 10_000;

static EXPLORER: std::sync::Mutex<Option<ExplorerReads>> = std::sync::Mutex::new(None);
static EXPLORER_WAKE: std::sync::Condvar = std::sync::Condvar::new();
static EXPLORER_REDRAW: AtomicBool = AtomicBool::new(false);

fn explorer_reads() -> std::sync::MutexGuard<'static, Option<ExplorerReads>> {
    EXPLORER.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Explorer's Tags and stars of `path` if read; otherwise queued for its worker.
pub fn explorer_of(path: &Path) -> Option<Explorer> {
    let mut guard = explorer_reads();
    let reads = guard.get_or_insert_with(|| ExplorerReads {
        queue: std::collections::VecDeque::new(),
        known: std::collections::HashMap::new(),
        started: false,
    });
    if let Some(found) = reads.known.get(path) {
        return Some(found.clone());
    }
    if !reads.queue.iter().any(|p| p == path) {
        if reads.queue.len() >= EXPLORER_QUEUE {
            reads.queue.pop_front();
        }
        reads.queue.push_back(path.to_path_buf());
    }
    if !reads.started {
        reads.started = std::thread::Builder::new().name("gezik-explorer".into()).spawn(explorer_worker).is_ok();
    }
    EXPLORER_WAKE.notify_one();
    None
}

fn explorer_worker() {
    gezik_platform::priority::lower_this_thread();
    loop {
        let path = {
            let mut guard = explorer_reads();
            loop {
                if let Some(path) = guard.as_mut().and_then(|r| r.queue.pop_back()) {
                    break path;
                }
                guard = EXPLORER_WAKE.wait(guard).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        let found = read_explorer(&path);
        if let Some(reads) = explorer_reads().as_mut() {
            if reads.known.len() >= EXPLORER_KNOWN {
                reads.known.clear();
            }
            reads.known.insert(path, found);
        }
        // One redraw for answers that come together.
        if !EXPLORER_REDRAW.swap(true, Ordering::SeqCst) {
            let _ = slint::invoke_from_event_loop(|| {
                EXPLORER_REDRAW.store(false, Ordering::SeqCst);
                redraw_all();
            });
        }
    }
}

/// F5, a tag job: Explorer's answers are asked for again.
fn forget_explorer() {
    if let Some(reads) = explorer_reads().as_mut() {
        reads.known.clear();
    }
}

/// Tests: an answer as if read.
#[cfg(test)]
pub fn set_explorer_for_tests(path: &Path, found: Explorer) {
    drop(explorer_of(path));
    if let Some(reads) = explorer_reads().as_mut() {
        reads.queue.retain(|p| p != path);
        reads.known.insert(path.to_path_buf(), found);
    }
}

/// A file's meta for a folder read whole (12c, `view/folder_tags.rs`): network drives too, the
/// view asked for it; a file that cannot be read has none.
pub fn read_meta(path: &Path) -> Meta {
    STORE.get().and_then(|store| store.read(path).ok()).unwrap_or_default()
}

/// Store entries of files other programs moved into `folder` follow them (12c).
pub fn refind_in(folder: &Path, files: &[(&str, u64, i64)]) {
    if let Some(store) = STORE.get() {
        store.refind_in(folder, files);
    }
}

fn on_network(path: &Path) -> bool {
    let folder = path.parent().unwrap_or(path);
    NETWORK.with(|n| {
        let mut n = n.borrow_mut();
        match &*n {
            Some((known, network)) if known == folder => *network,
            _ => {
                let network = gezik_platform::fs::is_network(folder).unwrap_or(false);
                *n = Some((folder.to_path_buf(), network));
                network
            }
        }
    })
}

/// Finder's colours, the same in light and dark themes; no colour: transparent (a ring).
pub fn color_of(color: TagColor) -> Color {
    let rgb = |r, g, b| Color::from_rgb_u8(r, g, b);
    match color {
        TagColor::None => Color::from_argb_u8(0, 0, 0, 0),
        TagColor::Gray => rgb(0x8e, 0x8e, 0x93),
        TagColor::Green => rgb(0x34, 0xc7, 0x59),
        TagColor::Purple => rgb(0xaf, 0x52, 0xde),
        TagColor::Blue => rgb(0x00, 0x7a, 0xff),
        TagColor::Yellow => rgb(0xff, 0xcc, 0x00),
        TagColor::Red => rgb(0xff, 0x3b, 0x30),
        TagColor::Orange => rgb(0xff, 0x95, 0x00),
    }
}

/// The colour a tag shows: the list's for a listed name, else the file's (on Linux, where files
/// keep none, gray).
pub fn shown_color(tag: &Tag, list: &[Tag], linux: bool) -> TagColor {
    match list.iter().find(|t| same_name(&t.name, &tag.name)) {
        Some(listed) => listed.color,
        None if linux && tag.color == TagColor::None => TagColor::Gray,
        None => tag.color,
    }
}

/// The most dots a row draws; more are a count (`+2`).
pub const MAX_DOTS: usize = 3;

/// A row's dots (at most three), how many more, and the names for a screen reader.
pub fn dots_of(tags: &[Tag], list: &[Tag]) -> (Vec<Color>, i32, String) {
    let linux = cfg!(target_os = "linux");
    let colors = tags.iter().take(MAX_DOTS).map(|t| color_of(shown_color(t, list, linux))).collect();
    let more = i32::try_from(tags.len().saturating_sub(MAX_DOTS)).unwrap_or(i32::MAX);
    let names = tags.iter().map(|t| &*t.name).collect::<Vec<_>>().join(", ");
    (colors, more, names)
}

/// Drops what is known of every row's tags and draws the rows again (a tag job ended, F5).
pub fn forget_rows() {
    crate::media::with_current(crate::media::Media::forget_tags);
    forget_explorer();
    // This window's panes read again now; other windows' when they next show or sort.
    let here: Vec<_> = crate::panes::all().iter().map(|p| p.id).collect();
    for p in crate::panes::everywhere() {
        if here.contains(&p.id) {
            p.view.reread_folder_tags();
        } else {
            p.view.folder_tags_changed();
        }
    }
    // Dots, or a highlight rule asking for tags, ratings or comments, show what changed.
    if dots_on() || highlight_needs_meta() {
        redraw_all();
    }
}

/// The highlight rules in effect (spec 12 §9) and, per rule, its text color in the theme
/// shown and whether that color stays on a selected row (contrast to the selection ≥ 3:1).
#[derive(Default)]
struct Highlighting {
    rules: gezik_core::highlight::Highlights,
    tints: Vec<(Color, bool)>,
    /// The theme's background and selection (kept when a theme fails to load).
    theme: Option<(gezik_config::Color, gezik_config::Color)>,
}

thread_local! {
    static HIGHLIGHT: RefCell<Highlighting> = RefCell::new(Highlighting::default());
}

/// `[[highlight-rules]]` and the theme, at every resolve: rows are drawn again when either
/// changes what they show.
pub fn set_highlights(rules: gezik_core::highlight::Highlights, theme: Option<&gezik_config::theme::ThemeColors>) {
    let changed = HIGHLIGHT.with(|h| {
        let mut h = h.borrow_mut();
        let theme = theme.map(|c| (c.background, c.selection)).or(h.theme);
        let tints = tints_of(&rules, theme);
        let changed = h.rules != rules || h.tints != tints;
        *h = Highlighting { rules, tints, theme };
        changed
    });
    if changed {
        redraw_all();
    }
}

/// Each rule's color for a theme with this background and selection.
fn tints_of(
    rules: &gezik_core::highlight::Highlights,
    theme: Option<(gezik_config::Color, gezik_config::Color)>,
) -> Vec<(Color, bool)> {
    use gezik_config::theme_rules::{contrast, flatten, is_dark};
    let (background, selection) = theme.unwrap_or_default();
    let dark = is_dark(background);
    let selection = flatten(selection, background);
    rules
        .rules()
        .iter()
        .map(|rule| match rule.style.rgb(dark) {
            None => (Color::from_argb_u8(0, 0, 0, 0), false),
            Some([r, g, b]) => {
                (Color::from_rgb_u8(r, g, b), contrast(gezik_config::Color { r, g, b, a: 255 }, selection) >= 3.0)
            }
        })
        .collect()
}

/// Whether a highlight rule asks for rows' tags, ratings or comments.
pub fn highlight_needs_meta() -> bool {
    HIGHLIGHT.with(|h| h.borrow().rules.needs_meta())
}

/// A row's text color (transparent: the theme's) and boldness by the first rule matching it. A
/// selected row keeps the color only where it reads on the selection.
pub fn highlight_of(
    e: &gezik_core::Entry,
    now: std::time::SystemTime,
    meta: gezik_core::highlight::RowMeta,
    selected: bool,
) -> (Color, bool) {
    HIGHLIGHT.with(|h| {
        let h = h.borrow();
        let none = (Color::from_argb_u8(0, 0, 0, 0), false);
        if h.rules.is_empty() {
            return none;
        }
        let Some(i) = h.rules.first(e, now, meta) else { return none };
        let (tint, keeps) = h.tints[i];
        let bold = h.rules.rules()[i].style.bold;
        (if selected && !keeps { none.0 } else { tint }, bold)
    })
}

fn redraw_all() {
    for p in crate::panes::everywhere() {
        p.view.redraw();
    }
}

/// The macOS View menu's Tag Dots mark.
fn sync_window() {
    let on = dots_on();
    crate::windows::for_each(|window| window.set_view_tag_dots(on));
}

/// View ▸ Tag dots: shows or hides them, and writes `[tags] dots`.
pub fn toggle_dots() {
    let dots = if dots_on() { Dots::Off } else { Dots::On };
    let config = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.settings.dots = dots;
        FORCED.store(dots == Dots::On, Ordering::Relaxed);
        s.config.clone()
    });
    redraw_all();
    sync_window();
    if let Some(config) = config {
        let id = crate::windows::current();
        config.write_settings(gezik_config::settings_writer::SettingsChange::TagDots(dots), move |result| {
            if let Err(warning) = result {
                crate::windows::post(id, move || crate::panes::active_view().note(warning.to_string()));
            }
        });
    }
}

/// What the tag actions act on: the selection, else the focused item; `None` (said in the
/// status bar) for drives and the trash.
fn targets(view: &View) -> Option<Vec<PathBuf>> {
    if view.shows_drives() || view.shows_trash() {
        view.note(t!("Tags work on files and folders").into_owned());
        return None;
    }
    let mut items: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path));
    }
    (!items.is_empty()).then_some(items)
}

/// Reads `paths`' tags on a thread, then runs `then` with them in this window.
/// The tag jobs sent and not finished yet, with what they do: a read before one of them writes
/// sees its edits too (a tag key pressed twice quickly takes the tag off again).
type InFlight = Vec<(gezik_ops::JobId, Vec<PathBuf>, Vec<MetaEdit>)>;

thread_local! {
    static IN_FLIGHT: RefCell<InFlight> = const { RefCell::new(Vec::new()) };
}

/// `metas` (of `paths`) as they will be once the jobs in `jobs` have written: each job's edits
/// on the paths it has, in the order the jobs were sent. The edits are idempotent, so a job that
/// wrote before the read counts once all the same.
pub fn with_in_flight(paths: &[PathBuf], metas: &mut [Meta], jobs: &InFlight) {
    for (_, job_paths, edits) in jobs {
        for (path, meta) in paths.iter().zip(metas.iter_mut()) {
            if job_paths.contains(path) {
                edits.iter().for_each(|edit| meta.apply(edit));
            }
        }
    }
}

fn read_then(paths: Vec<PathBuf>, then: impl FnOnce(Vec<PathBuf>, Vec<Meta>) + Send + 'static) {
    let Some(store) = STORE.get().cloned() else { return };
    let id = crate::windows::current();
    // The jobs in flight now may finish before the read: their edits count either way.
    let mut jobs = IN_FLIGHT.with(|f| f.borrow().clone());
    let spawned = std::thread::Builder::new().name("gezik-tags".into()).spawn(move || {
        let metas = paths.iter().map(|path| store.read(path).unwrap_or_default()).collect();
        crate::windows::post(id, move || {
            let mut metas: Vec<Meta> = metas;
            IN_FLIGHT.with(|f| {
                for job in f.borrow().iter() {
                    if !jobs.iter().any(|(id, _, _)| *id == job.0) {
                        jobs.push(job.clone());
                    }
                }
            });
            with_in_flight(&paths, &mut metas, &jobs);
            then(paths, metas)
        });
    });
    if let Err(err) = spawned {
        eprintln!("gezik: cannot start the tags thread: {err}");
    }
}

/// `edits` on `paths` as one job (one undo); the first tag written turns `dots = "auto"` on.
fn submit(paths: Vec<PathBuf>, edits: Vec<MetaEdit>) {
    if paths.is_empty() || edits.is_empty() {
        return;
    }
    let adds = edits.iter().any(|e| matches!(e, MetaEdit::AddTags(_)));
    let task = Box::new(gezik_ops::TagsTask::new(paths.clone(), edits.clone()));
    crate::operations::with_current(|ops| {
        let job = ops.submit(task, None, crate::operations::After::Nothing);
        IN_FLIGHT.with(|f| f.borrow_mut().push((job, paths, edits)));
    });
    if adds && !STATE.with(|s| s.borrow().used) {
        let before = dots_on();
        let config = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.used = true;
            s.config.clone()
        });
        if let Some(config) = config {
            config.update_state(|state| state.tags_used = true);
        }
        if before != dots_on() {
            redraw_all();
            sync_window();
        }
    }
}

/// Whether `tag-n` takes `name` off (all have it) rather than putting it on.
pub fn all_have(metas: &[Meta], name: &str) -> bool {
    !metas.is_empty() && metas.iter().all(|m| m.has_tag(name))
}

/// `tag-n` (1-based): the list's n-th tag on the selection, or off it when all have it.
pub fn toggle_nth(n: usize, view: &View) {
    let Some(tag) = list().get(n.wrapping_sub(1)).cloned() else {
        return view.note(t!("No tag {} in your list ([tags] list)", n));
    };
    let Some(paths) = targets(view) else { return };
    toggle_on(tag, paths);
}

/// `tag` on `paths`, or off them when all have it; the status bar says which.
pub fn toggle_on(tag: Tag, paths: Vec<PathBuf>) {
    read_then(paths, move |paths, metas| {
        let off = all_have(&metas, &tag.name);
        let n = paths.len();
        let note = if off {
            tn!("{} removed from {n} item" | "{} removed from {n} items", n, tag.name)
        } else {
            tn!("{} added to {n} item" | "{} added to {n} items", n, tag.name)
        };
        let edit = if off { MetaEdit::RemoveTags(vec![tag.name.clone()]) } else { MetaEdit::AddTags(vec![tag]) };
        submit(paths, vec![edit]);
        crate::panes::active_view().note(note);
    });
}

/// `clear-tags`: every tag off the selection (rating and comment stay); nothing to read first.
pub fn clear(view: &View) {
    if let Some(paths) = targets(view) {
        clear_paths(paths);
    }
}

pub fn clear_paths(paths: Vec<PathBuf>) {
    let note = tn!("Tags cleared from {n} item" | "Tags cleared from {n} items", paths.len());
    submit(paths, vec![MetaEdit::ClearTags]);
    crate::panes::active_view().note(note);
}

/// The selection's stars (0: none), from the macOS menu bar (spec 12 §8.1).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn rate(stars: u8, view: &View) {
    if let Some(paths) = targets(view) {
        rate_paths(stars, paths);
    }
}

pub fn rate_paths(stars: u8, paths: Vec<PathBuf>) {
    let count = paths.len();
    let note = match stars.min(5) {
        0 => tn!("Rating removed from {n} item" | "Rating removed from {n} items", count),
        // TRANSLATORS: {} is the stars given ("★★★").
        n => tn!("{n} item rated {}" | "{n} items rated {}", count, "★".repeat(usize::from(n))),
    };
    submit(paths, vec![MetaEdit::Rating(stars.min(5))]);
    crate::panes::active_view().note(note);
}

/// `edit-comment`: the selection's comment asked for in a question (spec 12 §8.1).
pub fn edit_comment(view: &View) {
    if let Some(paths) = targets(view) {
        edit_comment_paths(paths);
    }
}

/// The comment the field starts with: the items' own when all have the same (0), else none and
/// how many items there are (their comments differ).
pub fn shared_comment(metas: &[Meta]) -> (String, usize) {
    match metas.split_first() {
        Some((first, rest)) if rest.iter().all(|m| m.comment == first.comment) => (first.comment.to_string(), 0),
        _ => (String::new(), metas.len()),
    }
}

/// The line under the comment field for `text`, and whether it stops Save: over 1,000
/// characters it does; line breaks and tabs become spaces; `differing` items' comments differ.
pub fn comment_note(text: &str, differing: usize) -> (String, bool) {
    let count = text.chars().count();
    if count > gezik_core::meta::MAX_COMMENT {
        let count = gezik_core::group_digits(count as u64);
        (t!("A comment is at most 1,000 characters; this one has {}", count), true)
    } else if text.chars().any(char::is_control) {
        (t!("Line breaks and tabs become spaces").into_owned(), false)
    } else if text.is_empty() && differing > 0 {
        (tn!("Comments differ across {n} item" | "Comments differ across {n} items", differing), false)
    } else {
        (String::new(), false)
    }
}

/// The comment a question's answer writes: Save with a changed field writes it (empty: removes
/// it); `Remove Comments` (button 1, only when they differ) removes them all; an untouched field
/// does nothing, so different comments are never cleared by a Save.
pub fn comment_answer(choice: usize, text: &str, initial: &str, differing: usize) -> Option<String> {
    match choice {
        1 if differing > 0 => Some(String::new()),
        0 if text.trim() != initial.trim() => Some(text.to_owned()),
        _ => None,
    }
}

pub fn edit_comment_paths(paths: Vec<PathBuf>) {
    read_then(paths, |paths, metas| {
        let (initial, differing) = shared_comment(&metas);
        let message = t!("Comment for {}:", crate::operations::items_text(&paths));
        let note = move |text: &str| comment_note(text, differing);
        let unchanged = initial.clone();
        let (save, remove, cancel) = (t!("Save"), t!("Remove Comments"), t!("Cancel"));
        let buttons: &[&str] = if differing > 0 { &[&save, &remove, &cancel] } else { &[&save, &cancel] };
        crate::operations::with_current(|ops| {
            ops.dialogs().ask_form(t!("Comment"), message, (initial, Vec::new()), buttons, note, move |answer| {
                let Some((choice, text, _)) = answer else { return };
                if let Some(comment) = comment_answer(choice, &text, &unchanged, differing) {
                    submit(paths, vec![MetaEdit::Comment(comment.into())]);
                }
            });
        });
    });
}

/// `tags`: the picker for the selection.
pub fn open_picker(view: &View) {
    if let Some(paths) = targets(view) {
        with_current(|p| p.open(paths));
    }
}

/// A tag job (or its undo) ended: rows read their tags again.
pub fn job_done(job: gezik_ops::JobId) {
    IN_FLIGHT.with(|f| f.borrow_mut().retain(|(id, _, _)| *id != job));
    forget_rows();
}

// The menus (spec 12 §5.3, §12.3: 2400–2599).

/// `Tags ▸` for `paths` at place `at`: the list's tags, marked `• ` when every row (whose tags
/// are known) has it, `– ` when some do; then Tags… and Clear tags.
pub fn menu_sub(paths: &[PathBuf], at: usize) -> crate::context_menu::Submenu {
    use crate::context_menu::{TAGS_CLEAR, TAGS_COMMENT, TAGS_PICKER, TAGS_RATING_FIRST, TAGS_TAG_FIRST};
    let mut known: Vec<Arc<RowMeta>> = Vec::new();
    crate::media::with_current(|m| known = paths.iter().filter_map(|p| m.known_tags(p)).collect());
    // Marks only when every row is known: nothing is read for a menu.
    let all_known = known.len() == paths.len() && !paths.is_empty();
    let mut items: Vec<(u32, String, bool)> = list()
        .iter()
        .enumerate()
        .map(|(i, tag)| {
            let count = known.iter().filter(|row| row.meta.has_tag(&tag.name)).count();
            let mark = match (all_known, count) {
                (false, _) | (_, 0) => "    ",
                (true, n) if n == paths.len() => "• ",
                _ => "– ",
            };
            (TAGS_TAG_FIRST + i as u32, format!("{mark}{}", tag.name), true)
        })
        .collect();
    items.push((TAGS_PICKER, gezik_i18n::t!("Tags…").into_owned(), true));
    items.push((TAGS_CLEAR, gezik_i18n::t!("Clear tags").into_owned(), true));
    // The stars, marked when every row is known to have them (12d).
    let same = known
        .split_first()
        .filter(|_| all_known)
        .and_then(|(first, rest)| rest.iter().all(|r| r.meta.rating == first.meta.rating).then_some(first.meta.rating));
    for stars in 0..=5u8 {
        let mark = if same == Some(stars) { "• " } else { "    " };
        let title =
            if stars == 0 { gezik_i18n::t!("No rating").into_owned() } else { "★".repeat(usize::from(stars)) };
        items.push((TAGS_RATING_FIRST + u32::from(stars), format!("{mark}{title}"), true));
    }
    items.push((TAGS_COMMENT, gezik_i18n::t!("Comment…").into_owned(), true));
    crate::context_menu::Submenu { title: gezik_i18n::t!("Tags").into_owned(), at, items }
}

/// A `Tags ▸` item chosen for `paths`; returns whether `id` was one.
pub fn menu_chosen(id: u32, paths: Vec<PathBuf>) -> bool {
    use crate::context_menu::{
        TAGS_CLEAR, TAGS_COMMENT, TAGS_PICKER, TAGS_RATING_FIRST, TAGS_RATING_LAST, TAGS_TAG_FIRST, TAGS_TAG_MAX,
    };
    match id {
        TAGS_PICKER => with_current(|p| p.open(paths)),
        TAGS_CLEAR => clear_paths(paths),
        TAGS_COMMENT => edit_comment_paths(paths),
        TAGS_RATING_FIRST..=TAGS_RATING_LAST => rate_paths((id - TAGS_RATING_FIRST) as u8, paths),
        id if (TAGS_TAG_FIRST..TAGS_TAG_FIRST + TAGS_TAG_MAX).contains(&id) => {
            if let Some(tag) = list().get((id - TAGS_TAG_FIRST) as usize).cloned() {
                toggle_on(tag, paths);
            }
        }
        _ => return false,
    }
    true
}

/// The listed tag `name` on the selection, or off it (the palette's Toggle Tag items).
pub fn toggle_named(name: &str, view: &View) {
    let Some(tag) = list().into_iter().find(|t| same_name(&t.name, name)) else { return };
    if let Some(paths) = targets(view) {
        toggle_on(tag, paths);
    }
}

// The picker (spec 12 §5.1).

/// A line of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A tag, and whether the list has it.
    Tag(Tag, bool),
    /// `Create "…"`: the typed name.
    Create(String),
    /// The stars (12d).
    Rating,
    /// `Comment…`.
    Comment,
}

/// What the picker changes when it closes: tags put on every item, and tags taken off all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    pub adds: Vec<Tag>,
    pub removes: Vec<Box<str>>,
    /// Stars given in the box (12d).
    pub rating: Option<u8>,
}

impl Edits {
    /// One job's edits: the removals, then the additions.
    pub fn to_edits(&self) -> Vec<MetaEdit> {
        let mut out = Vec::new();
        if !self.removes.is_empty() {
            out.push(MetaEdit::RemoveTags(self.removes.clone()));
        }
        if !self.adds.is_empty() {
            out.push(MetaEdit::AddTags(self.adds.clone()));
        }
        out.extend(self.rating.map(MetaEdit::Rating));
        out
    }

    /// The stars the box shows: those given, else the items' own when all have the same (none
    /// while read); 6 when they differ (drawn faint).
    pub fn stars(&self, metas: Option<&[Meta]>) -> i32 {
        if let Some(stars) = self.rating {
            return i32::from(stars);
        }
        match metas.and_then(|m| m.split_first()) {
            Some((first, rest)) if rest.iter().any(|m| m.rating != first.rating) => 6,
            Some((first, _)) => i32::from(first.rating),
            None => 0,
        }
    }

    /// A click on star `n`: those stars, or none when the box shows exactly them.
    pub fn star(&mut self, n: u8, metas: Option<&[Meta]>) {
        self.rating = Some(if self.stars(metas) == i32::from(n) { 0 } else { n.min(5) });
    }

    /// 2 on every item, 1 on some, 0 on none, as the picker shows `name` now; `metas` `None`
    /// while they are read (nothing is known to be on any item yet).
    pub fn check(&self, name: &str, metas: Option<&[Meta]>) -> i32 {
        if self.adds.iter().any(|t| same_name(&t.name, name)) {
            return 2;
        }
        if self.removes.iter().any(|n| same_name(n, name)) {
            return 0;
        }
        original(name, metas)
    }

    /// A click on `tag`: on every item when it is not on all, off all when it is.
    pub fn toggle(&mut self, tag: &Tag, metas: Option<&[Meta]>) {
        let name = &tag.name;
        let was = original(name, metas);
        if self.check(name, metas) == 2 {
            self.adds.retain(|t| !same_name(&t.name, name));
            if metas.is_some() && was != 0 {
                self.removes.push(name.clone());
            }
        } else {
            self.removes.retain(|n| !same_name(n, name));
            if was != 2 {
                self.adds.push(tag.clone());
            }
        }
    }
}

/// How many of `metas` have `name`, as 0, 1 (some) or 2 (all).
fn original(name: &str, metas: Option<&[Meta]>) -> i32 {
    let Some(metas) = metas else { return 0 };
    match metas.iter().filter(|m| m.has_tag(name)).count() {
        0 => 0,
        n if n == metas.len() => 2,
        _ => 1,
    }
}

/// The picker's lines for `query`: the list's tags, then those the items have that the list
/// has not, then `Create "…"` when the typed name is no tag's.
pub fn picker_lines(list: &[Tag], metas: Option<&[Meta]>, edits: &Edits, query: &str) -> Vec<Line> {
    let mut tags: Vec<Line> = list.iter().map(|t| Line::Tag(t.clone(), true)).collect();
    let found = metas.into_iter().flatten().flat_map(|m| &m.tags).chain(&edits.adds);
    for tag in found {
        if !tags.iter().any(|line| matches!(line, Line::Tag(t, _) if same_name(&t.name, &tag.name))) {
            tags.push(Line::Tag(tag.clone(), false));
        }
    }
    let typed = query.trim();
    let folded = fold_text(typed);
    let exact = tags.iter().any(|line| matches!(line, Line::Tag(t, _) if same_name(&t.name, typed)));
    let mut lines: Vec<Line> = tags
        .into_iter()
        .filter(|line| matches!(line, Line::Tag(t, _) if fold_text(&t.name).contains(&folded)))
        .collect();
    if !typed.is_empty() && !exact {
        lines.push(Line::Create(typed.to_owned()));
    }
    lines.extend([Line::Rating, Line::Comment]);
    lines
}

pub fn with_current(f: impl FnOnce(&TagPicker)) {
    if let Some(picker) = CURRENT.with(crate::windows::Slot::current) {
        f(&picker);
    }
}

pub fn is_open() -> bool {
    CURRENT.with(crate::windows::Slot::current).is_some_and(|p| p.0.open.get())
}

struct Inner {
    window: slint::Weak<AppWindow>,
    open: Cell<bool>,
    /// Which opening the reads belong to: a late answer for an earlier one is dropped.
    opening: Cell<u64>,
    paths: RefCell<Vec<PathBuf>>,
    /// The items' tags; `None` while they are read.
    metas: RefCell<Option<Vec<Meta>>>,
    edits: RefCell<Edits>,
    query: RefCell<String>,
    lines: RefCell<Vec<Line>>,
    current: Cell<usize>,
    model: Rc<VecModel<PickRow>>,
}

#[derive(Clone)]
pub struct TagPicker(Rc<Inner>);

impl TagPicker {
    pub fn new(window: &AppWindow) -> TagPicker {
        let picker = TagPicker(Rc::new(Inner {
            window: window.as_weak(),
            open: Cell::new(false),
            opening: Cell::new(0),
            paths: RefCell::default(),
            metas: RefCell::default(),
            edits: RefCell::default(),
            query: RefCell::default(),
            lines: RefCell::default(),
            current: Cell::new(0),
            model: Rc::new(VecModel::default()),
        }));
        CURRENT.with(|c| c.set(crate::windows::current(), picker.clone()));
        picker
    }

    /// Opens on `paths`; their tags are read meanwhile.
    pub fn open(&self, paths: Vec<PathBuf>) {
        let Some(window) = self.0.window.upgrade() else { return };
        if paths.is_empty() || window.get_tp_open() || crate::tab_tools::over_another_layer(&window) {
            return;
        }
        let opening = self.0.opening.get() + 1;
        self.0.opening.set(opening);
        *self.0.paths.borrow_mut() = paths.clone();
        *self.0.metas.borrow_mut() = None;
        // Search and Retag… on its results: the old tag off, the new one on (spec 12 §7.3).
        *self.0.edits.borrow_mut() = retag_edits().unwrap_or_default();
        self.0.query.borrow_mut().clear();
        self.0.current.set(0);
        self.0.open.set(true);
        window.set_tp_rows(ModelRc::from(self.0.model.clone()));
        window.set_tp_empty(gezik_i18n::t!("No tags match").as_ref().into());
        window.set_tp_query("".into());
        self.show();
        window.set_tp_open(true);
        window.invoke_focus_list();
        read_then(paths, move |_, metas| {
            with_current(|p| {
                if p.0.open.get() && p.0.opening.get() == opening {
                    *p.0.metas.borrow_mut() = Some(metas);
                    p.show();
                }
            });
        });
    }

    pub fn edited(&self, query: &str) {
        *self.0.query.borrow_mut() = query.to_owned();
        self.0.current.set(0);
        self.show();
    }

    /// Esc closes (and applies), Enter toggles the current line, Up/Down move; Alt+0…5 gives
    /// the stars (Windows and Linux: ⌥ types a character on macOS).
    pub fn chord(&self, chord: &Chord) -> bool {
        if let Key::Char(c @ '0'..='5') = chord.key
            && chord.alt
            && !(chord.ctrl || chord.meta || chord.shift)
        {
            self.0.edits.borrow_mut().rating = Some(c as u8 - b'0');
            self.show();
            return true;
        }
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        match chord.key {
            Key::Escape => self.close(),
            Key::Enter => self.choose(self.0.current.get()),
            Key::Up | Key::Down => {
                let len = self.0.lines.borrow().len();
                self.0.current.set(crate::tab_tools::step(self.0.current.get(), len, chord.key == Key::Down));
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_current(i32::try_from(self.0.current.get()).unwrap_or(0));
                }
            }
            _ => return false,
        }
        true
    }

    /// A click on line `line`.
    pub fn chosen(&self, line: usize) {
        self.choose(line);
    }

    /// A click on star `n` of the Rating line.
    pub fn starred(&self, n: u8) {
        {
            let metas = self.0.metas.borrow();
            self.0.edits.borrow_mut().star(n, metas.as_deref());
        }
        self.show();
    }

    /// Toggles a tag on the items (in memory; the box stays open), or creates the typed one.
    fn choose(&self, line: usize) {
        let Some(line) = self.0.lines.borrow().get(line).cloned() else { return };
        self.0.current.set(self.0.lines.borrow().iter().position(|l| *l == line).unwrap_or(0));
        let tag = match line {
            Line::Tag(tag, _) => tag,
            // Enter on the stars: they take clicks and Alt+digits.
            Line::Rating => return,
            Line::Comment => {
                let paths = self.0.paths.borrow().clone();
                self.close();
                return edit_comment_paths(paths);
            }
            Line::Create(name) => {
                if let Err(err) = check_tag_name(&name) {
                    return crate::panes::active_view().note(format!("\"{name}\": {}", gezik_i18n::t("", err)));
                }
                let tag = Tag::new(&name, TagColor::None);
                add_to_list(tag.clone());
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_query("".into());
                }
                self.0.query.borrow_mut().clear();
                tag
            }
        };
        {
            let metas = self.0.metas.borrow();
            self.0.edits.borrow_mut().toggle(&tag, metas.as_deref());
        }
        self.show();
    }

    /// Closes the box; what changed becomes one job.
    fn close(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.open.set(false);
        window.set_tp_open(false);
        self.0.model.set_vec(Vec::new());
        let edits = std::mem::take(&mut *self.0.edits.borrow_mut()).to_edits();
        let paths = std::mem::take(&mut *self.0.paths.borrow_mut());
        *self.0.metas.borrow_mut() = None;
        self.0.lines.borrow_mut().clear();
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
        submit(paths, edits);
    }

    fn show(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let list = list();
        let metas = self.0.metas.borrow();
        let edits = self.0.edits.borrow();
        let lines = picker_lines(&list, metas.as_deref(), &edits, &self.0.query.borrow());
        let paths = self.0.paths.borrow();
        let label = match (metas.is_some(), paths.as_slice()) {
            (false, _) => gezik_i18n::t!("Reading tags…").into_owned(),
            (true, many) => gezik_i18n::t!("Tags for {}", crate::operations::items_text(many)),
        };
        let rows: Vec<PickRow> = lines
            .iter()
            .map(|line| match line {
                Line::Tag(tag, listed) => {
                    let keys = key_label(&list, &tag.name);
                    let linux = cfg!(target_os = "linux");
                    PickRow {
                        title: (*tag.name).into(),
                        detail: if *listed { "".into() } else { gezik_i18n::t!("Not in your list").as_ref().into() },
                        tag: "".into(),
                        keys: keys.into(),
                        bold: false,
                        dot: color_of(shown_color(tag, &list, linux)),
                        check: edits.check(&tag.name, metas.as_deref()),
                        stars: -1,
                    }
                }
                Line::Create(name) => PickRow {
                    title: gezik_i18n::t!("Create \"{}\"", name).into(),
                    tag: gezik_i18n::t!("New tag").as_ref().into(),
                    ..plain_row()
                },
                Line::Rating => PickRow {
                    title: gezik_i18n::t!("Rating").as_ref().into(),
                    keys: if cfg!(target_os = "macos") { "" } else { "Alt+0…5" }.into(),
                    tag: " ".into(),
                    stars: edits.stars(metas.as_deref()),
                    ..plain_row()
                },
                Line::Comment => PickRow { title: gezik_i18n::t!("Comment…").as_ref().into(), ..plain_row() },
            })
            .collect();
        drop((metas, edits, paths));
        *self.0.lines.borrow_mut() = lines;
        let current = self.0.current.get().min(rows.len().saturating_sub(1));
        self.0.current.set(current);
        self.0.model.set_vec(rows);
        window.set_tp_label(label.into());
        window.set_tp_current(i32::try_from(current).unwrap_or(0));
    }
}

/// A picker line without a tag's dot, mark or stars.
fn plain_row() -> PickRow {
    PickRow {
        title: "".into(),
        detail: "".into(),
        tag: "".into(),
        keys: "".into(),
        bold: false,
        dot: Color::default(),
        check: -1,
        stars: -1,
    }
}

/// The key of the `tag-N` action for `name`'s place in `list` (the first seven), else "".
pub fn key_label(list: &[Tag], name: &str) -> String {
    list.iter()
        .position(|t| same_name(&t.name, name))
        .and_then(|i| Action::ALL.into_iter().find(|a| a.tag_number() == Some(i + 1)))
        .and_then(crate::keys::chord_for)
        .map(|chord| crate::keys::chord_label(&chord, Platform::current()))
        .unwrap_or_default()
}

/// Puts `tag` at the end of `[tags] list` (shown at once, written to settings.toml).
fn add_to_list(tag: Tag) {
    let added = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let new = !s.settings.list.iter().any(|t| same_name(&t.name, &tag.name))
            && s.settings.list.len() < gezik_config::tags::TAGS_MAX;
        if new {
            s.settings.list.push(tag.clone());
        }
        new.then(|| (s.settings.list.clone(), s.config.clone()))
    });
    if added.is_some() {
        list_shown();
    }
    if let Some((list, Some(config))) = added {
        write_list(config, SettingsChange::TagAdded(tag, list));
    }
}

/// `change` to settings.toml; a warning goes to the status bar.
fn write_list(config: ConfigStore, change: SettingsChange) {
    let id = crate::windows::current();
    config.write_settings(change, move |result| {
        if let Err(warning) = result {
            crate::windows::post(id, move || crate::panes::active_view().note(warning.to_string()));
        }
    });
}

/// The list in memory changed: the sidebars' TAGS and the dots' colours follow.
fn list_shown() {
    crate::windows::for_each(|_| crate::sidebar::with_current(crate::sidebar::Sidebar::tags_changed));
    if dots_on() {
        redraw_all();
    }
}

// The sidebar's TAGS and the list (spec 12 §7).

/// What a search reads tags with: the engine's store (the file's own record, else Gezik's).
pub fn meta_reader() -> Option<gezik_search::query::MetaReader> {
    let store = STORE.get()?.clone();
    Some(gezik_search::query::MetaReader(Arc::new(move |path: &Path| store.read(path).unwrap_or_default())))
}

/// The tag store's files that pass `terms`, on the drives mounted now (any thread).
pub fn store_files(terms: &[gezik_core::meta_filter::TagTerm]) -> Vec<PathBuf> {
    STORE.get().map(|store| store.find(|meta| gezik_core::meta_filter::matches(terms, meta))).unwrap_or_default()
}

/// The search for the tag `name` (spec 12 §7.1): under `folder` with its subfolders, or on
/// every drive (`None`; macOS: Spotlight).
pub fn tag_search(name: &str, folder: Option<PathBuf>) -> gezik_core::search::SearchSpec {
    use gezik_core::search::{Scope, SearchSpec};
    let scope = folder.map_or(Scope::AllDrives, Scope::Folder);
    SearchSpec { tags: vec![name.to_owned()], ..SearchSpec::new(scope) }
}

/// The folder the active pane shows (a search's own folder), if any.
fn shown_folder() -> Option<PathBuf> {
    use gezik_core::nav::Location;
    match crate::panes::active_nav().active_location() {
        Location::Path(folder) | Location::Flat(folder) => Some(folder),
        Location::Search(spec) => spec.scope.folder().map(Path::to_path_buf),
        Location::Drives | Location::Trash | Location::Vfs(_) => None,
    }
}

fn search_tag(name: &str, folder: Option<PathBuf>, new_tab: bool) {
    let spec = tag_search(name, folder);
    crate::panes::with_active(|p| p.search.run_saved(spec, new_tab));
}

/// A click on the sidebar's tag `index` (a middle click: in a new tab): its files, on macOS
/// everywhere (Spotlight, as Finder), elsewhere in the folder shown (spec 12 §7.1, decision 20).
pub fn sidebar_clicked(index: i32, new_tab: bool) {
    let Some(tag) = usize::try_from(index).ok().and_then(|i| list().get(i).cloned()) else { return };
    let folder = if cfg!(target_os = "macos") { None } else { shown_folder() };
    if folder.is_some() {
        crate::panes::active_view().note(t!("Searching {} here; Search everywhere is in its menu", tag.name));
    }
    search_tag(&tag.name, folder, new_tab);
}

/// `tag:Name` for the filter bar, quoted when the name has a space or `;`.
pub fn tag_word(name: &str) -> String {
    if name.contains([' ', ';']) { format!("tag:\"{name}\"") } else { format!("tag:{name}") }
}

/// An item of the sidebar tag `name`'s menu.
pub fn row_menu_chosen(id: u32, name: &str) {
    use crate::context_menu::{
        TAG_COLOR_FIRST, TAG_DOWN, TAG_FILTER, TAG_REMOVE, TAG_RENAME, TAG_SEARCH_EVERYWHERE, TAG_SEARCH_HERE, TAG_UP,
        TAGS_NEW,
    };
    match id {
        TAG_SEARCH_HERE => match shown_folder() {
            Some(folder) => search_tag(name, Some(folder), false),
            None => crate::panes::active_view()
                .note(t!("No folder is shown here; Search everywhere looks on every drive").into_owned()),
        },
        TAG_SEARCH_EVERYWHERE => search_tag(name, None, false),
        TAG_FILTER => {
            let word = tag_word(name);
            crate::panes::with_active(|p| p.filter.apply_text(&word));
        }
        TAG_RENAME => ask_rename(name),
        TAG_UP | TAG_DOWN => edit_list(TagListEdit::Move(name.into(), id == TAG_UP)),
        TAG_REMOVE => {
            edit_list(TagListEdit::Remove(name.into()));
            crate::panes::active_view().note(t!("{} left your list; files keep it", name));
        }
        TAGS_NEW => ask_new_tag(),
        id if (TAG_COLOR_FIRST..TAG_COLOR_FIRST + 8).contains(&id) => {
            edit_list(TagListEdit::Color(name.into(), TagColor::from_number((id - TAG_COLOR_FIRST) as u8)));
        }
        _ => {}
    }
}

/// `edit` on the list in memory now and in settings.toml (comments kept); files are not touched.
fn edit_list(edit: TagListEdit) {
    let (before, config) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let before = s.settings.list.clone();
        s.settings.list = gezik_config::tags::edited(&before, &edit);
        (before, s.config.clone())
    });
    list_shown();
    if let Some(config) = config {
        write_list(config, SettingsChange::TagList(edit, before));
    }
}

/// The line under a tag name typed for `old` (`""`: a new tag), and whether it cannot be used.
pub fn name_note(old: &str, text: &str, list: &[Tag]) -> (String, bool) {
    let name = text.trim();
    if name.is_empty() {
        return (t!("Type a name").into_owned(), true);
    }
    if let Err(err) = check_tag_name(name) {
        return (gezik_i18n::t("", err).into_owned(), true);
    }
    if !same_name(name, old) && list.iter().any(|t| same_name(&t.name, name)) {
        return (t!("{} is in your list already", name), true);
    }
    (String::new(), false)
}

/// Rename… (spec 12 §7.3): the list's name only, and the store's entries; files keep the old
/// name. Search and Retag… also looks for the files with the old one.
fn ask_rename(old: &str) {
    let old = old.to_owned();
    let message = t!(
        "New name for {0}. Files keep the tag they have ({0}); Search and Retag… finds them so you can give them the new one.",
        old
    );
    let note = {
        let old = old.clone();
        move |text: &str| name_note(&old, text, &list())
    };
    crate::operations::with_current(|ops| {
        ops.dialogs().ask_text_choice(
            t!("Rename Tag"),
            message,
            old.clone(),
            &[&t!("Rename"), &t!("Search and Retag…"), &t!("Cancel")],
            note,
            move |answer| {
                let Some((button, text)) = answer else { return };
                let new = text.trim().to_owned();
                if name_note(&old, &new, &list()).1 {
                    return;
                }
                let color = list().iter().find(|t| same_name(&t.name, &old)).map_or(TagColor::None, |t| t.color);
                if new != old {
                    edit_list(TagListEdit::Rename(old.as_str().into(), new.as_str().into()));
                    if let Some(store) = STORE.get().cloned() {
                        let (from, to) = (old.clone(), new.clone());
                        let _ = std::thread::Builder::new()
                            .name("gezik-tags".into())
                            .spawn(move || store.rename_tag(&from, &to));
                    }
                }
                if button == 1 {
                    retag(&old, Tag::new(&new, color));
                }
            },
        );
    });
}

thread_local! {
    /// Search and Retag…: the old name and the new tag the next picker on that search's
    /// results comes with.
    static RETAG: RefCell<Option<(Box<str>, Tag)>> = const { RefCell::new(None) };
}

fn retag(old: &str, new: Tag) {
    // TRANSLATORS: {0} is the Tags… shortcut, {1} the old tag, {2} the new one.
    let note = t!(
        "Select the files, then Tags… ({0}): {1} comes off and {2} goes on, as one job",
        key_label_of(Action::Tags),
        old,
        new.name
    );
    RETAG.with(|r| *r.borrow_mut() = Some((old.into(), new)));
    search_tag(old, shown_folder(), false);
    crate::panes::active_view().note(note);
}

fn key_label_of(action: Action) -> String {
    crate::keys::chord_for(action).map(|c| crate::keys::chord_label(&c, Platform::current())).unwrap_or_default()
}

/// What the status bar adds while a Search and Retag… search shows (plan sapma 7).
pub fn retag_hint() -> Option<String> {
    let gezik_core::nav::Location::Search(spec) = crate::panes::active_nav().active_location() else { return None };
    RETAG.with(|r| {
        let r = r.borrow();
        let (old, new) = r.as_ref()?;
        spec.tags.iter().any(|t| same_name(t, old)).then(|| {
            // TRANSLATORS: {0} is the Tags… shortcut, {1} the old tag, {2} the new one.
            t!(
                "Select them, then Tags… ({0}): {1} comes off and {2} goes on",
                key_label_of(Action::Tags),
                old,
                new.name
            )
        })
    })
}

/// The picker's edits when it opens on the results of a Search and Retag… search (taken once).
fn retag_edits() -> Option<Edits> {
    let gezik_core::nav::Location::Search(spec) = crate::panes::active_nav().active_location() else { return None };
    RETAG.with(|r| {
        let mut r = r.borrow_mut();
        let (old, _) = r.as_ref()?;
        if !spec.tags.iter().any(|t| same_name(t, old)) {
            return None;
        }
        let (old, new) = r.take()?;
        Some(Edits { adds: vec![new], removes: vec![old], rating: None })
    })
}

/// New tag… (the sidebar): a name for the end of the list, no colour yet.
fn ask_new_tag() {
    crate::operations::with_current(|ops| {
        ops.dialogs().ask_text_noted(
            t!("New Tag"),
            t!("Name of the new tag (its colour is in its menu):"),
            "",
            &[&t!("Add"), &t!("Cancel")],
            |text| name_note("", text, &list()),
            |text| {
                let Some(name) = text.map(|t| t.trim().to_owned()) else { return };
                if !name_note("", &name, &list()).1 {
                    add_to_list(Tag::new(&name, TagColor::None));
                }
            },
        );
    });
}

/// What files dropped on the sidebar's tag `index` get: that tag (spec 12 §7.1); `None` for no
/// such tag.
pub fn drop_edits(list: &[Tag], index: usize) -> Option<(Tag, Vec<MetaEdit>)> {
    let tag = list.get(index)?.clone();
    Some((tag.clone(), vec![MetaEdit::AddTags(vec![tag])]))
}

/// Files dropped on the sidebar's tag `index`: one job, one undo.
pub fn drop_on_tag(index: usize, paths: Vec<PathBuf>) {
    let Some((tag, edits)) = drop_edits(&list(), index) else { return };
    let note = tn!("{} added to {n} item" | "{} added to {n} items", paths.len(), tag.name);
    submit(paths, edits);
    crate::panes::active_view().note(note);
}

/// The label beside files dragged over the sidebar's tag `name`.
pub fn drop_label(name: &str, count: usize) -> String {
    // TRANSLATORS: beside files dragged over a tag in the sidebar; {} is the tag.
    tn!("Tag {n} item {}" | "Tag {n} items {}", count, name)
}

/// The question of Clean Up Tag Store… for what `orphans` found; `None` when nothing is gone
/// (the status bar says it then).
pub fn clean_up_text(orphans: &gezik_ops::meta::Orphans) -> (Option<String>, String) {
    let n = orphans.gone.len();
    let digits = |n: usize| gezik_core::group_digits(n as u64);
    // Whole sentences, each after a space.
    let mut away = String::new();
    if orphans.unmounted > 0 {
        let m = orphans.unmounted;
        away.push(' ');
        away.push_str(&tn!(
            "{} entry on a drive that is not connected was left alone."
                | "{} entries on drives that are not connected were left alone.",
            m,
            digits(m)
        ));
    }
    if orphans.unreachable > 0 {
        let m = orphans.unreachable;
        away.push(' ');
        away.push_str(&tn!(
            "{} entry Gezik cannot look at (no access, or the drive does not answer) was left alone."
                | "{} entries Gezik cannot look at (no access, or the drive does not answer) were left alone.",
            m,
            digits(m)
        ));
    }
    match n {
        0 => (None, format!("{}{away}", t!("No entries point to files that are gone."))),
        n => {
            let question = tn!(
                "{} entry points to a file that is gone. Remove it?"
                    | "{} entries point to files that are gone. Remove them?",
                n,
                digits(n)
            );
            (Some(format!("{question}{away}")), String::new())
        }
    }
}

/// Clean Up Tag Store… (spec 12 §7.4): counts on a thread, asks, removes only what the answer
/// says (and only what is still gone), a copy of the store kept as tags-store.tsv.bak.
pub fn clean_up_store() {
    let Some(store) = STORE.get().cloned() else { return };
    let id = crate::windows::current();
    let spawned = std::thread::Builder::new().name("gezik-tags".into()).spawn(move || {
        let found = store.orphans();
        crate::windows::post(id, move || {
            let orphans = match found {
                Ok(orphans) => orphans,
                Err(err) => {
                    return crate::panes::active_view().note(t!("The tag store cannot be cleaned up: {}", err));
                }
            };
            let (question, note) = clean_up_text(&orphans);
            let Some(question) = question else { return crate::panes::active_view().note(note) };
            crate::operations::with_current(|ops| {
                ops.dialogs().ask(t!("Clean Up Tag Store"), question, &[&t!("Remove"), &t!("Cancel")], move |answer| {
                    if answer != Some(0) {
                        return;
                    }
                    let Some(store) = STORE.get().cloned() else { return };
                    let id = crate::windows::current();
                    let _ = std::thread::Builder::new().name("gezik-tags".into()).spawn(move || {
                        let note = match store.remove_orphans(&orphans.gone) {
                            Ok(n) => tn!(
                                "{} entry removed from the tag store; the old one is tags-store.tsv.bak"
                                    | "{} entries removed from the tag store; the old one is tags-store.tsv.bak",
                                n,
                                gezik_core::group_digits(n as u64)
                            ),
                            Err(err) => t!("The tag store cannot be cleaned up: {}", err),
                        };
                        crate::windows::post(id, move || crate::panes::active_view().note(note));
                    });
                });
            });
        });
    });
    if let Err(err) = spawned {
        eprintln!("gezik: cannot start the tags thread: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_highlight_colors_read_on_a_light_background() {
        use gezik_config::theme_rules::contrast;
        use gezik_core::highlight::Hue;
        let white = gezik_config::Color { r: 255, g: 255, b: 255, a: 255 };
        for n in 1..8 {
            let [r, g, b] = Hue::Named(TagColor::from_number(n)).rgb(false);
            let ratio = contrast(gezik_config::Color { r, g, b, a: 255 }, white);
            assert!(ratio >= 3.0, "{:?}: {ratio:.2}", TagColor::from_number(n));
        }
    }

    fn tag(name: &str, color: TagColor) -> Tag {
        Tag::new(name, color)
    }

    fn with(names: &[&str]) -> Meta {
        Meta { tags: names.iter().map(|n| tag(n, TagColor::None)).collect(), ..Meta::default() }
    }

    fn list() -> Vec<Tag> {
        vec![tag("Red", TagColor::Red), tag("Orange", TagColor::Orange), tag("Green", TagColor::Green)]
    }

    #[test]
    fn the_picker_lists_the_list_then_the_items_own_then_create() {
        let metas = [with(&["Project X", "red"]), with(&["Red"])];
        let lines = picker_lines(&list(), Some(&metas), &Edits::default(), "");
        let names: Vec<String> = lines
            .iter()
            .map(|l| match l {
                Line::Tag(t, listed) => format!("{}{}", t.name, if *listed { "" } else { "*" }),
                Line::Create(n) => format!("+{n}"),
                Line::Rating => "★".to_owned(),
                Line::Comment => "Comment…".to_owned(),
            })
            .collect();
        assert_eq!(names, ["Red", "Orange", "Green", "Project X*", "★", "Comment…"], "red is Red: one line");
        let typed = picker_lines(&list(), Some(&metas), &Edits::default(), "pro");
        assert_eq!(
            typed,
            [
                Line::Tag(tag("Project X", TagColor::None), false),
                Line::Create("pro".into()),
                Line::Rating,
                Line::Comment
            ]
        );
        let exact = picker_lines(&list(), Some(&metas), &Edits::default(), " GREEN ");
        assert_eq!(exact[0], Line::Tag(tag("Green", TagColor::Green), true));
        assert_eq!(exact.len(), 3, "a tag's own name: no Create");
        let turkish = picker_lines(&[tag("İş", TagColor::Blue)], None, &Edits::default(), "iş");
        assert_eq!(turkish.len(), 3, "Turkish İ folds");
    }

    #[test]
    fn a_drop_on_a_sidebar_tag_adds_it_as_one_edit() {
        let list = list();
        let (tag, edits) = drop_edits(&list, 2).unwrap();
        assert_eq!(&*tag.name, "Green");
        assert_eq!(edits, [MetaEdit::AddTags(vec![tag.clone()])], "one job, undone as one");
        let mut meta = Meta::default();
        edits.iter().for_each(|e| meta.apply(e));
        assert!(meta.has_tag("green"));
        assert_eq!(drop_edits(&list, 9), None, "no such tag");
        assert_eq!(drop_label("Red", 3), "Tag 3 items Red");
    }

    #[test]
    fn names_typed_for_the_list_keep_its_rules() {
        let list = list();
        assert!(name_note("Red", "Crimson", &list).0.is_empty());
        assert!(name_note("Red", "RED", &list).0.is_empty(), "its own name in another case");
        assert_eq!(name_note("Red", "green", &list), ("green is in your list already".to_owned(), true));
        assert!(name_note("", "a,b", &list).1 && name_note("", "  ", &list).1);
    }

    #[test]
    fn search_and_retag_takes_the_old_tag_off_and_puts_the_new_one_on() {
        let edits = Edits { adds: vec![tag("Crimson", TagColor::Red)], removes: vec!["Red".into()], rating: None };
        assert_eq!(
            edits.to_edits(),
            [MetaEdit::RemoveTags(vec!["Red".into()]), MetaEdit::AddTags(vec![tag("Crimson", TagColor::Red)])]
        );
        let metas = [with(&["Red"])];
        assert_eq!((edits.check("Red", Some(&metas)), edits.check("Crimson", Some(&metas))), (0, 2));
    }

    #[test]
    fn a_tag_search_and_filter_word() {
        let spec = tag_search("Red", Some(PathBuf::from("/w")));
        assert_eq!((spec.tags.as_slice(), spec.scope.folder()), (&["Red".to_owned()][..], Some(Path::new("/w"))));
        assert_eq!(tag_search("Red", None).scope, gezik_core::search::Scope::AllDrives);
        assert_eq!(tag_word("Red"), "tag:Red");
        assert_eq!(tag_word("Project X"), "tag:\"Project X\"");
        assert_eq!(gezik_core::meta_filter::split(&tag_word("a;b")).1.len(), 1);
    }

    #[test]
    fn the_clean_up_asks_only_when_something_is_gone() {
        use gezik_ops::meta::Orphans;
        let gone = |n: usize, unmounted: usize| Orphans {
            gone: vec![("v".to_owned(), "x".into()); n],
            unmounted,
            unreachable: 0,
        };
        assert_eq!(clean_up_text(&gone(0, 0)), (None, "No entries point to files that are gone.".to_owned()));
        assert_eq!(clean_up_text(&gone(1, 0)).0.as_deref(), Some("1 entry points to a file that is gone. Remove it?"));
        assert_eq!(
            clean_up_text(&gone(142, 318)).0.as_deref(),
            Some(
                "142 entries point to files that are gone. Remove them? 318 entries on drives that are not connected were left alone."
            )
        );
        assert!(clean_up_text(&gone(0, 1)).1.ends_with("1 entry on a drive that is not connected was left alone."));
        let locked = Orphans { unreachable: 2, ..gone(1, 0) };
        assert!(
            clean_up_text(&locked)
                .0
                .unwrap()
                .ends_with("2 entries Gezik cannot look at (no access, or the drive does not answer) were left alone.")
        );
    }

    fn rated(stars: u8, comment: &str) -> Meta {
        Meta { rating: stars, comment: comment.into(), ..Meta::default() }
    }

    #[test]
    fn the_stars_show_the_items_rating_and_a_click_sets_or_clears_it() {
        let same = [rated(3, ""), rated(3, "")];
        let mixed = [rated(3, ""), rated(0, "")];
        let mut edits = Edits::default();
        assert_eq!(edits.stars(None), 0, "while read: none");
        assert_eq!((edits.stars(Some(&same)), edits.stars(Some(&mixed))), (3, 6), "6: they differ (faint)");
        edits.star(3, Some(&same));
        assert_eq!(edits.rating, Some(0), "the stars shown again: none");
        edits.star(4, Some(&same));
        assert_eq!((edits.rating, edits.stars(Some(&mixed))), (Some(4), 4));
        edits.star(5, Some(&mixed));
        edits.toggle(&tag("Red", TagColor::Red), Some(&same));
        assert_eq!(
            edits.to_edits(),
            [MetaEdit::AddTags(vec![tag("Red", TagColor::Red)]), MetaEdit::Rating(5)],
            "tags and stars: one job"
        );
        assert_eq!(Edits::default().to_edits(), [], "no stars given: the rating is left alone");
    }

    #[test]
    fn a_comment_starts_from_the_shared_one_and_stays_within_its_rules() {
        assert_eq!(shared_comment(&[rated(0, "draft"), rated(2, "draft")]), ("draft".to_owned(), 0));
        assert_eq!(shared_comment(&[rated(0, "a"), rated(0, ""), rated(0, "b")]), (String::new(), 3));
        assert_eq!(shared_comment(&[rated(0, "a"), rated(0, "")]), (String::new(), 2), "1 has one: 2 differ");
        assert_eq!(shared_comment(&[rated(0, ""), rated(0, "")]), (String::new(), 0));
        assert_eq!(comment_note("ok", 0), (String::new(), false));
        assert_eq!(comment_note("", 2), ("Comments differ across 2 items".to_owned(), false));
        assert_eq!(comment_note("a\tb", 0), ("Line breaks and tabs become spaces".to_owned(), false));
        let (line, stops) = comment_note(&"ğ".repeat(1001), 0);
        assert!(stops && line.ends_with("1,001"), "{line}");
        assert!(!comment_note(&"ğ".repeat(1000), 0).1, "1,000 characters (not bytes) fit");
        // Different comments: an untouched Save leaves them; Remove Comments clears them.
        assert_eq!(comment_answer(0, "", "", 2), None);
        assert_eq!(comment_answer(0, "new", "", 2), Some("new".to_owned()));
        assert_eq!(comment_answer(1, "", "", 2), Some(String::new()));
        // One shared comment: Save as it was does nothing, emptied removes it.
        assert_eq!(comment_answer(0, " draft ", "draft", 0), None);
        assert_eq!(comment_answer(0, "", "draft", 0), Some(String::new()));
        assert_eq!(comment_answer(1, "x", "draft", 0), None, "Cancel");
    }

    #[test]
    fn the_menu_has_the_stars_and_comment_marked_from_what_is_known() {
        use crate::context_menu::{TAGS_COMMENT, TAGS_RATING_FIRST};
        let sub = menu_sub(&[PathBuf::from("a")], 2);
        let ids: Vec<u32> = sub.items.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(&ids[ids.len() - 7..], [2470, 2471, 2472, 2473, 2474, 2475, TAGS_COMMENT]);
        assert_eq!(sub.items[ids.len() - 6].1, "    ★", "nothing known: no mark");
        assert_eq!(TAGS_RATING_FIRST, 2470);
    }

    #[test]
    fn checks_say_all_some_or_none_and_a_click_flips_all() {
        let metas = [with(&["Red", "Orange"]), with(&["Red"])];
        let m = Some(&metas[..]);
        let mut edits = Edits::default();
        assert_eq!((edits.check("Red", m), edits.check("orange", m), edits.check("Green", m)), (2, 1, 0));
        // ✓ → off all; – → on all; none → on all.
        edits.toggle(&tag("Red", TagColor::Red), m);
        edits.toggle(&tag("Orange", TagColor::Orange), m);
        edits.toggle(&tag("Green", TagColor::Green), m);
        assert_eq!((edits.check("Red", m), edits.check("Orange", m), edits.check("Green", m)), (0, 2, 2));
        assert_eq!(
            edits.to_edits(),
            [
                MetaEdit::RemoveTags(vec!["Red".into()]),
                MetaEdit::AddTags(vec![tag("Orange", TagColor::Orange), tag("Green", TagColor::Green)]),
            ],
            "one job: removals, then additions"
        );
        // Back where it was: nothing to do.
        edits.toggle(&tag("Red", TagColor::Red), m);
        edits.toggle(&tag("Green", TagColor::Green), m);
        assert_eq!(edits.check("Red", m), 2);
        assert_eq!(edits.check("Green", m), 0);
        assert_eq!(edits.to_edits(), [MetaEdit::AddTags(vec![tag("Orange", TagColor::Orange)])]);
    }

    #[test]
    fn while_tags_are_read_a_click_only_adds() {
        let mut edits = Edits::default();
        assert_eq!(edits.check("Red", None), 0, "no – before the reads");
        edits.toggle(&tag("Red", TagColor::Red), None);
        assert_eq!(edits.to_edits(), [MetaEdit::AddTags(vec![tag("Red", TagColor::Red)])]);
        edits.toggle(&tag("Red", TagColor::Red), None);
        assert_eq!(edits.to_edits(), [], "a second click takes the addition back, removes nothing");
    }

    #[test]
    fn a_second_press_sees_the_first_job_not_yet_written() {
        let paths = [PathBuf::from("a"), PathBuf::from("b")];
        let red = tag("Red", TagColor::Red);
        let mut metas = [with(&[]), with(&["Blue"])];
        let jobs: InFlight = vec![(7, paths.to_vec(), vec![MetaEdit::AddTags(vec![red.clone()])])];
        with_in_flight(&paths, &mut metas, &jobs);
        assert!(all_have(&metas, "Red"), "the second press takes Red off");
        // The read came after the job wrote: the same once applied again.
        with_in_flight(&paths, &mut metas, &jobs);
        assert_eq!(metas[1].tags.iter().filter(|t| &*t.name == "Red").count(), 1);
        let mut other = [with(&[])];
        with_in_flight(&[PathBuf::from("c")], &mut other, &jobs);
        assert!(other[0].tags.is_empty(), "another file is not in that job");
    }

    #[test]
    fn a_tag_key_takes_off_only_what_all_have() {
        let metas = [with(&["Red"]), with(&["red", "Blue"])];
        assert!(all_have(&metas, "Red"));
        assert!(!all_have(&metas, "Blue"));
        assert!(!all_have(&[], "Red"));
    }

    #[test]
    fn dots_show_three_and_count_the_rest_in_the_lists_colours() {
        let tags = [
            tag("red", TagColor::Blue),
            tag("Project X", TagColor::None),
            tag("Mine", TagColor::Purple),
            tag("A", TagColor::None),
            tag("B", TagColor::None),
        ];
        let (colors, more, names) = dots_of(&tags, &list());
        assert_eq!(colors.len(), MAX_DOTS);
        assert_eq!(colors[0], color_of(TagColor::Red), "the list's colour wins");
        let unlisted =
            if cfg!(target_os = "linux") { color_of(TagColor::Gray) } else { Color::from_argb_u8(0, 0, 0, 0) };
        assert_eq!(colors[1], unlisted, "no colour: a ring (gray on Linux)");
        assert_eq!(colors[2], color_of(TagColor::Purple), "the file's own colour");
        assert_eq!(more, 2);
        assert_eq!(names, "red, Project X, Mine, A, B");
        assert_eq!(dots_of(&[], &list()), (Vec::new(), 0, String::new()));
    }

    #[test]
    fn auto_dots_wait_for_the_first_tag_off_macos() {
        assert!(!dots_shown(Dots::Auto, false, false));
        assert!(dots_shown(Dots::Auto, true, false));
        assert!(dots_shown(Dots::Auto, false, true), "macOS: Finder's users expect them");
        assert!(dots_shown(Dots::On, false, false) && !dots_shown(Dots::Off, true, true));
    }
}
