//! Settings… (spec 14 §4): settings.toml's single settings in a window of their own. The rows
//! come from `gezik_config::schema`: descriptions and defaults from the template, values from the
//! file's text as last read (the UI thread reads no file). A change is written at once through
//! the settings writer's queue; the reload that follows updates every window and this one.
//! The window is made when it opens and let go when it closes: nothing of it stays.

mod backup;
mod shortcuts;
mod themes;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::schema::{self, Described, FIELDS, Field, Kind, Page};
use gezik_config::settings::Settings;
use gezik_config::settings_writer::SettingsChange;
use gezik_config::shortcuts::Platform;
use gezik_config::store::{ConfigFiles, ConfigStore};
use gezik_config::theme::{BUILTIN_THEMES, ResolvedTheme};
use gezik_core::nav::Location;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{SettingItem, SettingsWindow, Theme};
use shortcuts::Pressed;

/// Slint's modifier keys: pressed alone, a shortcut waits for the rest.
const MODIFIERS: [slint::platform::Key; 9] = {
    use slint::platform::Key as K;
    [K::Shift, K::ShiftR, K::Control, K::ControlR, K::Alt, K::AltGr, K::Meta, K::MetaR, K::CapsLock]
};

/// The row menu's items (spec 14 §10.3).
const RESET: i32 = 2810;
const COPY_KEY: i32 = 2811;
const EDIT_FILE: i32 = 2812;

/// What the window shows of settings.toml.
#[derive(Default)]
struct Shown {
    described: Vec<Described>,
    /// Each field's value in the file (`None`: the default).
    values: Vec<Option<String>>,
    /// The syntax error that makes the rows read-only.
    broken: Option<String>,
    unknown: usize,
    /// The themes folder's ids.
    themes: Vec<String>,
    page: usize,
    query: String,
    /// The fields of the rows, in order.
    rows: Vec<usize>,
    /// The Keyboard page's shortcuts (`shortcuts::rows`), and those shown after the fields.
    keys: Vec<shortcuts::KeyRow>,
    key_rows: Vec<usize>,
    /// settings.toml's text (the shortcut conflicts read `[shortcuts]` from it).
    text: String,
    /// The changes a shortcut's Replace? writes, for that key row.
    pending: Option<(usize, Vec<SettingsChange>)>,
    /// Themes from GitHub, and those of their rows shown (after the key rows).
    got: themes::Themes,
    theme_rows: Vec<themes::Row>,
    /// Export, import and the backups, and those of their rows shown (after the theme rows).
    backup: backup::Backup,
    backup_rows: Vec<backup::Row>,
}

impl Shown {
    fn read(&mut self, files: &ConfigFiles, settings: &Settings) {
        let text = files.settings.as_deref().unwrap_or(gezik_config::store::SETTINGS_TEMPLATE);
        text.clone_into(&mut self.text);
        self.keys = shortcuts::rows(settings, Platform::current());
        match schema::values(text) {
            Ok(values) => (self.values, self.broken) = (values, None),
            Err(warning) => {
                // Read-only, with the values of the last text that read (what was in effect
                // before the file broke), not the built-in defaults.
                let last = LAST_GOOD.with(|l| l.borrow().as_deref().and_then(|text| schema::values(text).ok()));
                self.values = last.unwrap_or_else(|| vec![None; FIELDS.len()]);
                self.broken = Some(warning.to_string());
            }
        }
        self.unknown = schema::unknown_keys(text);
        let mut themes: Vec<String> =
            files.themes.keys().filter(|id| !BUILTIN_THEMES.contains(&id.as_str())).cloned().collect();
        themes.sort();
        self.themes = themes;
        self.got.read(&files.themes);
    }

    /// The fields shown: the page's, or with a search every page's that it finds.
    fn pick_rows(&mut self) {
        let page = Page::ALL[self.page.min(Page::ALL.len() - 1)];
        let query = self.query.trim();
        self.rows = FIELDS
            .iter()
            .enumerate()
            .filter(|(i, field)| match query {
                "" => field.page == page,
                query => schema::matches(field, &self.described[*i], query),
            })
            .map(|(i, _)| i)
            .collect();
        // No search: the Keyboard page's (an empty query matches every row).
        self.key_rows = match query {
            "" if page != Page::Keyboard => Vec::new(),
            _ => (0..self.keys.len()).filter(|&k| shortcuts::matches(&self.keys[k], query)).collect(),
        };
        self.theme_rows = match query {
            "" if page != Page::Appearance => Vec::new(),
            _ => self.got.rows(query),
        };
        self.backup_rows = match query {
            "" if page != Page::SyncAndBackup => Vec::new(),
            _ => self.backup.rows(query),
        };
    }

    /// The rows that edit settings.toml (read-only while it does not read): the fields and keys.
    fn locked_rows(&self) -> usize {
        self.rows.len() + self.key_rows.len()
    }

    /// The window row of backup row `row`, if shown.
    fn backup_index(&self, row: backup::Row) -> Option<usize> {
        let at = self.backup_rows.iter().position(|r| *r == row)?;
        Some(self.locked_rows() + self.theme_rows.len() + at)
    }

    /// The backup row of window row `row` (they come after the theme rows).
    fn backup_of(&self, row: usize) -> Option<backup::Row> {
        self.backup_rows.get(row.checked_sub(self.locked_rows() + self.theme_rows.len())?).copied()
    }

    /// The theme row of window row `row` (they come after the key rows).
    fn theme_of(&self, row: usize) -> Option<themes::Row> {
        self.theme_rows.get(row.checked_sub(self.rows.len() + self.key_rows.len())?).copied()
    }

    /// The key row of window row `row` (they come after the fields), as an index in `keys`.
    fn key_of(&self, row: usize) -> Option<usize> {
        self.key_rows.get(row.checked_sub(self.rows.len())?).copied()
    }

    fn item(&self, i: usize) -> SettingItem {
        let field = &FIELDS[i];
        let described = &self.described[i];
        let options: Vec<SharedString> = match field.kind {
            Kind::Choice(options) => options.iter().map(|o| SharedString::from(*o)).collect(),
            Kind::Theme => {
                let auto = (field.key == "theme").then_some("auto");
                auto.into_iter()
                    .chain(BUILTIN_THEMES)
                    .chain(self.themes.iter().map(String::as_str))
                    .map(Into::into)
                    .collect()
            }
            _ => Vec::new(),
        };
        SettingItem {
            kind: match field.kind {
                Kind::Bool => 0,
                Kind::Choice(_) | Kind::Theme => 1,
                Kind::Int | Kind::Text | Kind::Path => 2,
                Kind::List => 3,
                Kind::Integration => 4,
            },
            title: field.title().into(),
            key: field.path().into(),
            // The bundled font has no ▸ (the template's "Group by ▸ Type").
            description: described.description.replace('▸', ">").into(),
            value: self.values[i].as_deref().unwrap_or(&described.default).into(),
            is_default: self.values[i].is_none(),
            default: described.default.as_str().into(),
            options: ModelRc::new(VecModel::from(options)),
        }
    }

    /// What the status line says, and whether it is a problem: settings.toml's error, a search
    /// that finds nothing, the unknown keys on the Advanced page.
    fn status(&self) -> (String, bool) {
        if let Some(broken) = &self.broken {
            return (format!("{broken} — fix it in settings.toml; until then these are read-only"), true);
        }
        if self.locked_rows() + self.theme_rows.len() + self.backup_rows.len() == 0 {
            return ("No setting matches".to_owned(), false);
        }
        let advanced = Page::ALL[self.page.min(Page::ALL.len() - 1)] == Page::Advanced && self.query.trim().is_empty();
        let text = match self.unknown {
            n if !advanced || n == 0 => "Type in the box above to search every page".to_owned(),
            1 => "1 key Gezik does not know is in settings.toml (it stays there; see the notice)".to_owned(),
            n => format!("{n} keys Gezik does not know are in settings.toml (they stay there; see the notice)"),
        };
        (text, false)
    }

    /// The change `input` makes to row `row`'s field, or why it makes none.
    fn change(&self, row: usize, input: &str) -> Result<Option<SettingsChange>, String> {
        let i = *self.rows.get(row).ok_or_else(String::new)?;
        if self.broken.is_some() {
            return Err("Fix settings.toml first".to_owned());
        }
        let field = &FIELDS[i];
        let shown = self.values[i].as_deref().unwrap_or(&self.described[i].default);
        if input.trim() == shown && self.values[i].is_some() {
            return Ok(None);
        }
        let literal = schema::literal(field.kind, input).ok_or_else(|| match field.kind {
            Kind::Int => "Type a whole number".to_owned(),
            _ => "Not one of the choices".to_owned(),
        })?;
        schema::check(field, &literal)?;
        Ok(Some(value_change(field, Some(literal))))
    }
}

fn value_change(field: &Field, literal: Option<String>) -> SettingsChange {
    SettingsChange::Value { table: field.table.map(str::to_owned), key: field.key.to_owned(), value: literal }
}

struct Open {
    window: SettingsWindow,
    shown: RefCell<Shown>,
    rows: Rc<VecModel<SettingItem>>,
    /// Counts the theme previews: a revert waits and goes only if no newer one came.
    previews: Cell<u64>,
    /// The status the rows last asked for (to tell it from an outcome said since).
    auto_status: RefCell<String>,
}

thread_local! {
    static OPEN: RefCell<Option<Rc<Open>>> = const { RefCell::new(None) };
    /// settings.toml's last text that read, for the rows while the file is broken.
    static LAST_GOOD: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The config files are about to be replaced by a fresh read: their settings.toml text is kept
/// if it reads (the rows show it while the new one is broken). Only on a reload, never idle.
pub fn keep_last_good(files: &ConfigFiles) {
    let Some(text) = files.settings.as_deref() else { return };
    if schema::values(text).is_ok() {
        LAST_GOOD.with(|l| *l.borrow_mut() = Some(text.to_owned()));
    }
}

fn current() -> Option<Rc<Open>> {
    OPEN.with(|o| o.borrow().clone())
}

/// Opens the window, or brings it forward when it is open.
pub fn open() {
    if let Some(open) = current() {
        let _ = open.window.show();
        crate::single_instance::raise_native(open.window.window());
        #[cfg(target_os = "macos")]
        gezik_platform::app::activate();
        return;
    }
    let window = match SettingsWindow::new() {
        Ok(window) => window,
        Err(err) => return note(format!("Cannot open Settings: {err}")),
    };
    if let Some(theme) = crate::theme_bridge::current() {
        crate::theme_bridge::apply_global(&window.global::<Theme>(), &theme);
    }
    // Where the shortcuts get the key being pressed (keys.rs `Press`), as the main windows do.
    window.window().on_winit_window_event(|_, event| {
        match event {
            winit::event::WindowEvent::KeyboardInput { event, .. } => crate::keys::note_key(event),
            winit::event::WindowEvent::Focused(false) => crate::keys::forget_altgr(),
            _ => {}
        }
        EventResult::Propagate
    });
    let mut shown = Shown { described: schema::describe(gezik_config::store::SETTINGS_TEMPLATE), ..Shown::default() };
    with_settings(|settings| match crate::windows::files() {
        Some(files) => shown.read(&gezik_config::lock(&files), settings),
        None => shown.read(&ConfigFiles::default(), settings),
    });
    let date = gezik_platform::local_date_parts(std::time::SystemTime::now())
        .map(|p| format!("{:04}-{:02}-{:02}", p.year, p.month, p.day))
        .unwrap_or_default();
    let zip = gezik_config::paths::KnownDirs::system().expand(&format!("{{documents}}/gezik-settings-{date}.zip"));
    shown.backup.export_to = zip.display().to_string();
    shown.pick_rows();
    let links = ["Open settings.toml", "Open Config Folder"];
    let pages: Vec<SharedString> = Page::ALL.iter().map(|p| p.title()).chain(links).map(Into::into).collect();
    window.set_pages(ModelRc::new(VecModel::from(pages)));
    let open = Rc::new(Open {
        window,
        shown: RefCell::new(shown),
        rows: Rc::new(VecModel::default()),
        previews: Cell::new(0),
        auto_status: RefCell::default(),
    });
    open.window.set_rows(ModelRc::from(open.rows.clone()));
    show_rows(&open, true);
    bind(&open);
    OPEN.with(|o| *o.borrow_mut() = Some(open.clone()));
    if let Err(err) = open.window.show() {
        close();
        note(format!("Cannot open Settings: {err}"));
    }
}

/// Opens the window at its Keyboard page (`edit-shortcuts`).
pub fn open_shortcuts() {
    open_at(Page::Keyboard);
}

/// Opens the window at its Appearance page, where Get Theme is (`install-theme`).
pub fn open_themes() {
    if let Some(open) = open_at(Page::Appearance) {
        say(&open, "Paste the address in Get Theme and press Enter");
        let get = open.shown.borrow().theme_rows.iter().position(|r| *r == themes::Row::Get);
        let before = open.shown.borrow().locked_rows();
        if let Some(get) = get {
            focus_row(&open, before + get);
        }
    }
}

/// Opens the window at its Sync and Backup page (`export-settings`, `import-settings`).
pub fn open_backup(import: bool) {
    if let Some(open) = open_at(Page::SyncAndBackup) {
        let row = if import { "Import Settings" } else { "Export Settings" };
        say(&open, &format!("Type the zip's path in {row} and press Enter"));
        let wanted = if import { backup::Row::Import } else { backup::Row::Export };
        let at = open.shown.borrow().backup_index(wanted);
        if let Some(at) = at {
            focus_row(&open, at);
        }
    }
}

/// Scrolls row `row` into view and gives its control the keyboard (once the rows are laid out).
fn focus_row(open: &Open, row: usize) {
    open.window.set_focus_row(-1);
    let row = i32::try_from(row).unwrap_or(-1);
    // After a frame, not on the next turn: the -1 must be seen (the same row asked for again
    // is a change then), and a row just added must exist and be laid out.
    slint::Timer::single_shot(Duration::from_millis(50), move || {
        if let Some(open) = current() {
            open.window.set_focus_row(row);
        }
    });
}

fn open_at(page: Page) -> Option<Rc<Open>> {
    open();
    let open = current()?;
    open.shown.borrow_mut().page = Page::ALL.iter().position(|p| *p == page).unwrap_or(0);
    open.shown.borrow_mut().pick_rows();
    show_rows(&open, true);
    Some(open)
}

/// `f` with the settings in effect (their shortcuts and commands).
fn with_settings<R>(f: impl FnOnce(&Settings) -> R) -> R {
    match crate::windows::loaded() {
        Some(loaded) => f(&loaded.settings),
        None => f(&Settings::default()),
    }
}

/// Lets the window go (its close button, the end of main).
pub fn close() {
    let Some(open) = OPEN.with(|o| o.borrow_mut().take()) else { return };
    let _ = open.window.hide();
    themes::stop();
    end_preview_now();
}

/// The config files were read again (settings.toml or a theme changed): the rows follow.
pub fn refresh(files: &ConfigFiles) {
    let Some(open) = current() else { return };
    with_settings(|settings| open.shown.borrow_mut().read(files, settings));
    open.shown.borrow_mut().pick_rows();
    show_rows(&open, false);
}

/// The theme shown changed: this window too.
pub fn retheme(theme: &ResolvedTheme) {
    if let Some(open) = current() {
        crate::theme_bridge::apply_global(&open.window.global::<Theme>(), theme);
    }
}

/// Puts the rows in the window: all new (a page or a search) or row by row, so a field being
/// typed in keeps its text and the keyboard (rows that come or go are added or taken at the end).
fn show_rows(open: &Open, new: bool) {
    let shown = open.shown.borrow();
    let keys = shown.key_rows.iter().map(|&k| shortcuts::item(&shown.keys[k], Platform::current()));
    let found = shown.theme_rows.iter().map(|&row| shown.got.item(row));
    let backups = shown.backup_rows.iter().map(|&row| shown.backup.item(row));
    let items: Vec<SettingItem> =
        shown.rows.iter().map(|&i| shown.item(i)).chain(keys).chain(found).chain(backups).collect();
    if new {
        open.rows.set_vec(items);
    } else {
        let count = items.len();
        for (row, item) in items.into_iter().enumerate() {
            match open.rows.row_data(row) {
                None => open.rows.push(item),
                Some(old) if !same(&old, &item) => open.rows.set_row_data(row, item),
                Some(_) => {}
            }
        }
        while open.rows.row_count() > count {
            open.rows.remove(open.rows.row_count() - 1);
        }
    }
    open.window.set_page(i32::try_from(shown.page).unwrap_or(0));
    open.window.set_read_only(shown.broken.is_some());
    open.window.set_locked_rows(i32::try_from(shown.locked_rows()).unwrap_or(i32::MAX));
    let (status, bad) = shown.status();
    let list = shown.backup_rows.contains(&backup::Row::Restore) && shown.backup.backups.is_none();
    drop(shown);
    // A reload keeps what the status line says (an import's outcome) unless it is a new
    // problem; the rows' own earlier status (a problem now fixed) is replaced.
    let before = open.auto_status.replace(status.clone());
    let ours = open.window.get_status().as_str() == before;
    if new || ours || (bad && status != before) {
        open.window.set_status(status.into());
        open.window.set_status_bad(bad);
    }
    if list {
        list_backups(open);
    }
}

/// Whether two rows show the same (their choices are new models each time).
fn same(a: &SettingItem, b: &SettingItem) -> bool {
    (a.kind, &a.title, &a.key, &a.value, a.is_default, &a.description)
        == (b.kind, &b.title, &b.key, &b.value, b.is_default, &b.description)
        && a.options.iter().eq(b.options.iter())
}

fn bind(open: &Rc<Open>) {
    let window = &open.window;
    let weak = Rc::downgrade(open);
    let field_of =
        |open: &Open, row: i32| -> Option<usize> { open.shown.borrow().rows.get(usize::try_from(row).ok()?).copied() };
    let rows_from = |weak: &std::rc::Weak<Open>, change: &dyn Fn(&mut Shown)| {
        if let Some(open) = weak.upgrade() {
            change(&mut open.shown.borrow_mut());
            open.shown.borrow_mut().pick_rows();
            show_rows(&open, true);
        }
    };
    window.on_page_chosen({
        let weak = weak.clone();
        move |page| match usize::try_from(page).unwrap_or(0).checked_sub(Page::ALL.len()) {
            None => rows_from(&weak, &|shown| shown.page = usize::try_from(page).unwrap_or(0)),
            Some(0) => edit_file(),
            Some(_) => open_folder(),
        }
    });
    window.on_search({
        let weak = weak.clone();
        move |text| rows_from(&weak, &|shown| shown.query = text.to_string())
    });
    window.on_commit({
        let weak = weak.clone();
        move |row, text| {
            let Some(open) = weak.upgrade() else { return };
            let row = usize::try_from(row).unwrap_or(usize::MAX);
            let theme_row = open.shown.borrow().theme_of(row);
            if let Some(theme_row) = theme_row {
                return theme_chosen(&open, theme_row, &text);
            }
            let backup_row = open.shown.borrow().backup_of(row);
            if let Some(backup_row) = backup_row {
                return backup_chosen(&open, backup_row, &text);
            }
            // A shortcut's Replace? answered.
            // Bound first: the borrow must end before `pending` is taken below.
            let key = open.shown.borrow().key_of(row);
            if let Some(key) = key {
                let pending = open.shown.borrow_mut().pending.take();
                say(&open, "");
                if let Some((_, changes)) = pending.filter(|(of, _)| *of == key && text == "Replace") {
                    changes.into_iter().for_each(write);
                }
                return;
            }
            let change = open.shown.borrow().change(row, &text);
            match change {
                Ok(Some(change)) => {
                    say(&open, "");
                    write(change);
                }
                Ok(None) => {}
                Err(why) => {
                    let title = open.shown.borrow().rows.get(row).map(|&i| FIELDS[i].title()).unwrap_or_default();
                    say_bad(&open, &format!("{title}: {why} — not written"));
                }
            }
        }
    });
    window.on_key({
        let weak = weak.clone();
        move |row, text, control, alt, shift, meta| {
            let Some(open) = weak.upgrade() else { return 1 };
            let key = open.shown.borrow().key_of(usize::try_from(row).unwrap_or(usize::MAX));
            let Some(key) = key else { return 1 };
            if open.shown.borrow().broken.is_some() {
                say_bad(&open, "Fix settings.toml first");
                return 1;
            }
            // As the main windows read a key (windows.rs `on_key_event`).
            let press = crate::keys::take_pressed();
            let (control, alt) = press.modifiers(control, alt);
            let text = crate::keys::shortcut_text(&text, control);
            let platform = Platform::current();
            let chord = crate::keys::chord_from_press(&text, press.physical, control, alt, shift, meta, platform);
            let modifier = MODIFIERS.iter().any(|m| text.chars().eq([char::from(*m)]));
            let outcome = with_settings(|settings| {
                let shown = open.shown.borrow();
                shortcuts::pressed(&shown.keys[key], chord, modifier, settings, &shown.text, platform)
            });
            match outcome {
                Pressed::Wait(why) => {
                    if !why.is_empty() {
                        say_bad(&open, &why);
                    }
                    0
                }
                Pressed::Done(word) => {
                    say(&open, &word);
                    1
                }
                Pressed::Write(changes) => {
                    say(&open, "");
                    changes.into_iter().for_each(write);
                    1
                }
                Pressed::Ask(question, changes) => {
                    open.shown.borrow_mut().pending = Some((key, changes));
                    say(&open, &question);
                    2
                }
            }
        }
    });
    window.on_hovered({
        let weak = weak.clone();
        move |row, option| {
            let Some(open) = weak.upgrade() else { return };
            if field_of(&open, row).is_some_and(|i| FIELDS[i].kind == Kind::Theme) {
                drop(open);
                preview(&option);
                return;
            }
            let theme_row = open.shown.borrow().theme_of(usize::try_from(row).unwrap_or(usize::MAX));
            let shown = theme_row.and_then(|row| open.shown.borrow().got.preview(row));
            drop(open);
            match shown {
                Some((id, text)) if !option.is_empty() => preview_theme(&id, text.as_deref()),
                Some(_) => preview(""),
                None => {}
            }
        }
    });
    window.on_button({
        let weak = weak.clone();
        move |row| {
            let Some(open) = weak.upgrade() else { return };
            match field_of(&open, row).map(|i| FIELDS[i].kind) {
                Some(Kind::Integration) => {
                    crate::windows::with_focused(|_| {
                        crate::integration::with_current(crate::integration::Integration::open)
                    });
                }
                Some(_) => edit_file(),
                None => {}
            }
        }
    });
    window.on_row_action({
        let weak = weak.clone();
        move |row, id| {
            let Some(open) = weak.upgrade() else { return 1 };
            // A theme or backup row: its key is what Copy Key copies.
            if usize::try_from(row).is_ok_and(|row| row >= open.shown.borrow().locked_rows()) {
                let key = usize::try_from(row).ok().and_then(|row| open.rows.row_data(row)).map(|i| i.key);
                let key = key.unwrap_or_default();
                match id {
                    COPY_KEY if gezik_platform::clipboard::write_text(&key).is_err() => {
                        say(&open, "Cannot use the clipboard");
                    }
                    EDIT_FILE => edit_file(),
                    _ => {}
                }
                return 1;
            }
            let key = open.shown.borrow().key_of(usize::try_from(row).unwrap_or(usize::MAX));
            if let Some(index) = key {
                let shown = open.shown.borrow();
                let key = &shown.keys[index];
                match id {
                    RESET if shown.broken.is_some() => say(&open, "Fix settings.toml first"),
                    RESET => {
                        let reset = with_settings(|s| shortcuts::reset(key, s, &shown.text, Platform::current()));
                        drop(shown);
                        match reset {
                            Ok(Pressed::Ask(question, changes)) => {
                                open.shown.borrow_mut().pending = Some((index, changes));
                                say(&open, &question);
                                return 2;
                            }
                            Ok(Pressed::Write(changes)) => changes.into_iter().for_each(write),
                            Ok(Pressed::Wait(why) | Pressed::Done(why)) => say(&open, &why),
                            Err(why) => say(&open, why),
                        }
                    }
                    COPY_KEY if gezik_platform::clipboard::write_text(&key.path).is_err() => {
                        say(&open, "Cannot use the clipboard");
                    }
                    EDIT_FILE => edit_file(),
                    _ => {}
                }
                return 1;
            }
            let Some(i) = field_of(&open, row) else { return 1 };
            let field = &FIELDS[i];
            match id {
                RESET if matches!(field.kind, Kind::List | Kind::Integration) => {
                    say(&open, "Change it in settings.toml or System Integration");
                }
                RESET if open.shown.borrow().broken.is_some() => say(&open, "Fix settings.toml first"),
                RESET => write(value_change(field, open.shown.borrow().described[i].literal.clone())),
                COPY_KEY => {
                    if gezik_platform::clipboard::write_text(&field.path()).is_err() {
                        say(&open, "Cannot use the clipboard");
                    }
                }
                EDIT_FILE => edit_file(),
                _ => {}
            }
            1
        }
    });
    window.on_escape(|| slint::Timer::single_shot(Duration::ZERO, close));
    window.window().on_close_requested(|| {
        // Not inside the window's own event: on the next turn of the event loop.
        slint::Timer::single_shot(Duration::ZERO, close);
        slint::CloseRequestResponse::HideWindow
    });
}

/// Opens the config folder (settings.toml's) in a new tab of the window last used.
fn open_folder() {
    open_folder_of(ConfigStore::shared_dir);
}

/// Opens one of the store's folders in a new tab of the window last used.
fn open_folder_of(which: fn(&ConfigStore) -> &std::path::Path) {
    let Some(dir) = crate::windows::config().map(|store| which(&store).to_owned()) else { return };
    crate::windows::with_focused(|_| {
        crate::panes::with_active(|p| p.nav.open_tab(Location::Path(dir), true));
    });
}

/// Hands `change` to the writer; a failure shows in the status line.
fn write(change: SettingsChange) {
    let Some(config) = crate::windows::config() else { return };
    config.write_settings(change, |result| {
        if let Err(warning) = result {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(open) = current() {
                    say_bad(&open, &warning.to_string());
                }
            });
        }
    });
}

fn say(open: &Open, text: &str) {
    open.window.set_status(text.into());
    open.window.set_status_bad(false);
}

fn say_bad(open: &Open, text: &str) {
    open.window.set_status(text.into());
    open.window.set_status_bad(true);
}

/// The status bar of the window last used, when this window cannot show it.
fn note(text: String) {
    crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.note(text)));
}

/// Opens settings.toml with the system's editor for it.
fn edit_file() {
    let Some(path) = crate::windows::config().map(|store| store.shared_dir().join("settings.toml")) else { return };
    if let Err(err) = open::that_detached(&path) {
        let text = format!("Cannot open {}: {}", path.display(), gezik_platform::fs::describe(&err));
        match current() {
            Some(open) => say_bad(&open, &text),
            None => note(text),
        }
    }
}

/// A theme row's choice: Get Theme's address, a theme found's Install, Replace or Keep Both, an
/// installed theme's Use Now, Check for Updates, Open Source Page or Delete Theme.
fn theme_chosen(open: &Open, row: themes::Row, text: &str) {
    match row {
        themes::Row::Get => fetch(open, text.trim()),
        themes::Row::Found(i) => {
            let date = gezik_platform::local_date_parts(std::time::SystemTime::now())
                .map(|p| format!("{:04}-{:02}-{:02}", p.year, p.month, p.day))
                .unwrap_or_default();
            let shown = open.shown.borrow();
            let Some((id, contents)) = shown.got.install(i, text == themes::KEEP_BOTH, &date) else { return };
            let name = shown.got.found[i].name.clone();
            drop(shown);
            let Some(dir) = crate::windows::config().map(|store| store.shared_dir().join("themes")) else { return };
            say(open, &format!("Installing {name}…"));
            // Off the UI thread; the config watcher's reload then shows it installed.
            std::thread::spawn(move || {
                let written = std::fs::create_dir_all(&dir)
                    .and_then(|()| gezik_config::paths::write_atomic(&themes::theme_path(&dir, &id), &contents));
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(open) = current() else { return };
                    match written {
                        Ok(()) => {
                            let mut themes = open.shown.borrow().got.files.clone();
                            themes.insert(id.clone(), contents);
                            open.shown.borrow_mut().got.read(&themes);
                            open.shown.borrow_mut().pick_rows();
                            show_rows(&open, false);
                            say(&open, &format!("Installed {name} as themes/{id}.toml: Use Now is in its row"));
                        }
                        Err(err) => say_bad(
                            &open,
                            &format!("Cannot write themes/{id}.toml: {}", gezik_platform::fs::describe(&err)),
                        ),
                    }
                });
            });
        }
        themes::Row::Installed(i) => {
            let shown = open.shown.borrow();
            let Some(theme) = shown.got.installed.get(i) else { return };
            let (id, address) = (theme.id.clone(), theme.address.clone());
            drop(shown);
            match text {
                themes::USE_NOW => write(SettingsChange::Value {
                    table: None,
                    key: "theme".to_owned(),
                    value: schema::literal(Kind::Theme, &id),
                }),
                themes::CHECK => fetch(open, &address),
                themes::SOURCE_PAGE => match themes::source_page(&address) {
                    Some(url) if open::that_detached(&url).is_ok() => {}
                    _ => say_bad(open, &format!("Cannot open {address}")),
                },
                themes::DELETE => {
                    let Some(dir) = crate::windows::config().map(|s| s.shared_dir().join("themes")) else { return };
                    // The real file name is read off the UI thread.
                    std::thread::spawn(move || {
                        let path = themes::theme_path(&dir, &id);
                        let _ = slint::invoke_from_event_loop(move || {
                            crate::windows::with_focused(|_| {
                                crate::operations::with_current(|ops| ops.trash_paths(vec![path], false));
                            });
                        });
                    });
                }
                _ => {}
            }
        }
    }
}

/// Downloads `address` and lists its themes ("": stops the download and clears the list).
fn fetch(open: &Open, address: &str) {
    let fetch = match address {
        "" => None,
        address => match gezik_config::theme_source::resolve(address) {
            Ok(fetch) => Some(fetch),
            Err(why) => return say_bad(open, why),
        },
    };
    themes::stop();
    {
        let mut shown = open.shown.borrow_mut();
        shown.got.address = address.to_owned();
        shown.got.found.clear();
        shown.pick_rows();
    }
    show_rows(open, false);
    let Some(fetch) = fetch else { return };
    say(open, &format!("Downloading {}…", fetch.url));
    themes::start(
        fetch,
        |bytes| {
            if let Some(open) = current() {
                say(&open, &format!("Downloading… {} KB", bytes >> 10));
            }
        },
        |result| {
            let Some(open) = current() else { return };
            match result {
                Ok(found) => {
                    let count = found.len();
                    open.shown.borrow_mut().got.found = found;
                    open.shown.borrow_mut().pick_rows();
                    show_rows(&open, false);
                    say(&open, &format!("{count} found: point at a choice in a row to preview the theme"));
                }
                Err(why) => say_bad(&open, &why),
            }
        },
    );
}

/// A Sync and Backup row's change: an export's or import's path, a box, Import or Cancel, a
/// backup to restore, Undo.
fn backup_chosen(open: &Open, row: backup::Row, text: &str) {
    use backup::Row;
    use gezik_config::bundle::Group;
    let path = || gezik_config::paths::KnownDirs::system().expand(text.trim().trim_matches('"'));
    let on = text == "true";
    let mut shown = open.shown.borrow_mut();
    let state = &mut shown.backup;
    match row {
        Row::Portable => {
            if text == backup::OPEN_DATA {
                drop(shown);
                return open_folder_of(ConfigStore::local_dir);
            }
        }
        Row::Templates => state.without_templates = !on,
        Row::Views => state.with_views = on,
        Row::Group(group) => {
            state.skipped.retain(|g| *g != group);
            if !on {
                state.skipped.push(group);
            }
        }
        Row::Programs => {}
        Row::Export => {
            text.trim().clone_into(&mut state.export_to);
            if text.trim().is_empty() {
                return;
            }
            let (out, groups) = (path(), state.export_groups());
            drop(shown);
            if groups.contains(&Group::Views) {
                flush_views();
            }
            let saying = format!("Exporting to {}…", out.display());
            let job = move |store: &ConfigStore| {
                store.flush_settings();
                store.flush_views();
                crate::settings_backup::export(store.shared_dir(), &out, &groups).map(|n| (n, out))
            };
            return work(open, &saying, job, |open, result| match (open, result) {
                (Some(open), Ok((n, out))) => {
                    say(open, &format!("Exported {n} {} to {}", if n == 1 { "file" } else { "files" }, out.display()))
                }
                (Some(open), Err(why)) => say_bad(open, &why),
                (None, _) => {}
            });
        }
        Row::Import | Row::Restore => {
            let from = match row {
                Row::Import => path(),
                _ => match crate::windows::config() {
                    Some(store) => store.shared_dir().join("backups").join(text),
                    None => return,
                },
            };
            state.import_from = if row == Row::Import { text.trim().to_owned() } else { from.display().to_string() };
            state.preview = None;
            state.skipped.clear();
            shown.pick_rows();
            drop(shown);
            show_rows(open, false);
            if text.trim().is_empty() {
                return;
            }
            let saying = format!("Reading {}…", from.display());
            // Gezik's own backups are not checked: a broken settings.toml comes back as it was.
            let check = row == Row::Import;
            let job = move |store: &ConfigStore| crate::settings_backup::preview(store.shared_dir(), &from, check);
            return work(open, &saying, job, |open, result| {
                let Some(open) = open else { return };
                let programs = match result {
                    Ok(preview) => {
                        let programs = preview.programs.len();
                        open.shown.borrow_mut().backup.preview = Some(preview);
                        programs
                    }
                    Err(why) => return say_bad(open, &why),
                };
                open.shown.borrow_mut().pick_rows();
                show_rows(open, false);
                // The Import or Cancel row is below the fold at the window's first size.
                let confirm = open.shown.borrow().backup_index(backup::Row::Confirm);
                if let Some(confirm) = confirm {
                    focus_row(open, confirm);
                }
                match programs {
                    0 => say(open, "Untick what not to import, then choose Import in the last of these rows"),
                    1 => say_bad(open, "1 setting in it runs programs (the red row): import only files you trust"),
                    n => say_bad(
                        open,
                        &format!("{n} settings in it run programs (the red row): import only files you trust"),
                    ),
                }
            });
        }
        Row::Confirm => {
            if text != backup::IMPORT {
                state.preview = None;
                shown.pick_rows();
                drop(shown);
                show_rows(open, false);
                return say(open, "Nothing imported");
            }
            if state.busy {
                drop(shown);
                return say(open, BUSY);
            }
            let chosen = state.chosen();
            shown.pick_rows();
            drop(shown);
            if chosen.iter().any(|f| f.group == Group::Views) {
                flush_views();
            }
            show_rows(open, false);
            // The flushes are channel round trips to the writer threads (no UI, no watcher): safe here.
            let job = move |store: &ConfigStore| {
                store.flush_settings();
                store.flush_views();
                crate::settings_backup::import(store.shared_dir(), chosen, &crate::settings_backup::stamp(), &discard)
            };
            return work(open, "Backing up, then importing…", job, |open, result| {
                imported(open, result, "Imported: Undo Import puts the files back");
            });
        }
        Row::Undo => {
            let Some(last) = backup::last() else { return };
            drop(shown);
            if text != backup::UNDO {
                return;
            }
            flush_views();
            let job = move |store: &ConfigStore| {
                store.flush_settings();
                store.flush_views();
                crate::settings_backup::undo(store.shared_dir(), &last, &crate::settings_backup::stamp(), &discard)
            };
            return work(open, "Undoing the import…", job, |open, result| {
                imported(open, result, "Undone: the files are as they were before the import");
            });
        }
    }
    drop(shown);
    show_rows(open, false);
}

const BUSY: &str = "Wait: Gezik is still reading or writing a zip";

/// An import's or undo's outcome: Undo Import follows it, the folder views in memory too.
fn imported(open: Option<&Open>, result: Result<crate::settings_backup::Done, String>, done: &str) {
    let ok = result.is_ok();
    match result {
        Ok(outcome) => {
            let undone = outcome.undo.replaced.is_empty() && outcome.undo.created.is_empty();
            backup::set_last((!undone).then_some(outcome.undo));
            if let Some(text) = outcome.views {
                let folders = gezik_config::views_file::parse_views(&text).unwrap_or_default();
                let memory = gezik_core::view_memory::ViewMemory::from_folders(folders);
                crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.replace_memory(memory)));
            }
        }
        Err(why) => {
            if let Some(open) = open {
                say_bad(open, &why);
            }
        }
    }
    let Some(open) = open else { return };
    open.shown.borrow_mut().backup.backups = None;
    open.shown.borrow_mut().pick_rows();
    show_rows(open, false);
    if ok {
        say(open, done);
    }
}

/// An old backup goes to the trash (kept when it cannot).
fn discard(path: &std::path::Path) {
    let _ = gezik_platform::fs::trash(path);
}

/// The folder views changed but not yet handed to their writer go now (an export or import
/// of views.toml reads the file).
fn flush_views() {
    crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.flush_memory()));
}

/// Runs `job` on a thread with the config store, then `done` on the UI thread with the window
/// if it is still open. One at a time.
fn work<R: Send + 'static>(
    open: &Open,
    saying: &str,
    job: impl FnOnce(&ConfigStore) -> R + Send + 'static,
    done: fn(Option<&Open>, R),
) {
    let Some(store) = crate::windows::config() else { return };
    if std::mem::replace(&mut open.shown.borrow_mut().backup.busy, true) {
        return say(open, BUSY);
    }
    say(open, saying);
    let spawned = std::thread::Builder::new().name("settings backup".into()).spawn(move || {
        let result = job(&store);
        let _ = slint::invoke_from_event_loop(move || {
            let open = current();
            if let Some(open) = &open {
                open.shown.borrow_mut().backup.busy = false;
            }
            done(open.as_deref(), result);
        });
    });
    if spawned.is_err() {
        open.shown.borrow_mut().backup.busy = false;
        say_bad(open, "Cannot start a thread");
    }
}

/// Lists the backups on a thread for Restore Backup.
fn list_backups(open: &Open) {
    let Some(store) = crate::windows::config() else { return };
    if std::mem::replace(&mut open.shown.borrow_mut().backup.listing, true) {
        return;
    }
    std::thread::spawn(move || {
        let names = crate::settings_backup::backups(store.shared_dir());
        let _ = slint::invoke_from_event_loop(move || {
            let Some(open) = current() else { return };
            open.shown.borrow_mut().backup.listing = false;
            open.shown.borrow_mut().backup.backups = Some(names);
            open.shown.borrow_mut().pick_rows();
            show_rows(&open, false);
        });
    });
}

/// Shows theme `id` in every window without writing it ("" or "auto": back to the theme in
/// effect, a moment later so that moving from one choice to the next does not flash).
fn preview(id: &str) {
    preview_theme(id, None);
}

/// `preview`, with `text` (a theme not installed) as theme `id`'s file.
fn preview_theme(id: &str, text: Option<&str>) {
    let Some(open) = current() else { return };
    let turn = open.previews.get() + 1;
    open.previews.set(turn);
    if id.is_empty() || id == "auto" {
        slint::Timer::single_shot(Duration::from_millis(150), move || {
            if current().is_some_and(|open| open.previews.get() == turn) {
                end_preview_now();
            }
        });
        return;
    }
    let Some(files) = crate::windows::files() else { return };
    let files = gezik_config::lock(&files);
    let resolved = match text {
        Some(text) => {
            let mut themes = files.themes.clone();
            themes.insert(id.to_owned(), text.to_owned());
            gezik_config::theme::resolve_theme(id, &themes, &mut Vec::new())
        }
        None => gezik_config::theme::resolve_theme(id, &files.themes, &mut Vec::new()),
    };
    drop(files);
    let Ok(mut theme) = resolved else { return };
    let compact =
        crate::windows::loaded().is_some_and(|l| l.settings.density == gezik_config::settings::Density::Compact);
    if compact {
        theme.metrics = theme.metrics.compact();
    }
    crate::windows::for_each(|window| crate::theme_bridge::apply(window, &theme));
}

/// Back to the theme the settings say.
fn end_preview_now() {
    let Some(theme) = crate::windows::loaded().and_then(|l| l.theme.clone()) else { return };
    if crate::theme_bridge::current().is_some_and(|shown| shown == theme) {
        return;
    }
    crate::windows::for_each(|window| crate::theme_bridge::apply(window, &theme));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str) -> Shown {
        let mut shown =
            Shown { described: schema::describe(gezik_config::store::SETTINGS_TEMPLATE), ..Shown::default() };
        let mut files = ConfigFiles { settings: Some(text.to_owned()), ..ConfigFiles::default() };
        files.themes.insert("nord".to_owned(), String::new());
        files.themes.insert("dark".to_owned(), String::new());
        shown.read(&files, &Settings::default());
        shown.pick_rows();
        shown
    }

    fn row_of(shown: &Shown, path: &str) -> usize {
        shown.rows.iter().position(|&i| FIELDS[i].path() == path).unwrap()
    }

    #[test]
    fn a_page_shows_its_fields_and_a_search_every_page() {
        let mut s = shown("");
        let general: Vec<String> = s.rows.iter().map(|&i| FIELDS[i].path()).collect();
        assert_eq!(general[..2], ["start-folder", "max-fps"]);
        s.page = 1;
        s.pick_rows();
        assert!(s.rows.iter().all(|&i| FIELDS[i].page == Page::Appearance));
        s.query = "confirm".to_owned();
        s.pick_rows();
        let found: Vec<String> = s.rows.iter().map(|&i| FIELDS[i].path()).collect();
        assert!(
            found.contains(&"files.confirm-trash".to_owned()) && found.contains(&"panes.confirm".to_owned()),
            "{found:?}"
        );
    }

    #[test]
    fn rows_say_the_value_or_the_default() {
        let mut s = shown("[layout]\ndensity = \"compact\"\n");
        s.page = 1;
        s.pick_rows();
        let density = s.item(s.rows[row_of(&s, "layout.density")]);
        assert_eq!((density.value.as_str(), density.is_default, density.kind), ("compact", false, 1));
        assert_eq!(density.options.row_count(), 2);
        let sidebar = s.item(s.rows[row_of(&s, "layout.sidebar")]);
        assert_eq!((sidebar.value.as_str(), sidebar.is_default), ("left", true));
        let theme = s.item(s.rows[row_of(&s, "theme")]);
        let options: Vec<String> = theme.options.iter().map(|o| o.to_string()).collect();
        assert_eq!(
            options,
            ["auto", "light", "dark", "classic-light", "classic-dark", "nord"],
            "a file shadowing a built-in once"
        );
        let light = s.item(s.rows[row_of(&s, "theme-light")]);
        assert!(!light.options.iter().any(|o| o == "auto"));
    }

    #[test]
    fn a_change_is_checked_before_it_is_written() {
        let s = shown("max-fps = 120\n");
        let fps = row_of(&s, "max-fps");
        assert_eq!(
            s.change(fps, "60"),
            Ok(Some(SettingsChange::Value { table: None, key: "max-fps".into(), value: Some("60".into()) }))
        );
        assert_eq!(s.change(fps, "120"), Ok(None), "the same value: nothing written");
        assert_eq!(s.change(fps, "sixty"), Err("Type a whole number".to_owned()));
        assert!(s.change(fps, "5").unwrap_err().contains("max-fps"), "out of the parser's range: not written");
        let restore = row_of(&s, "session.restore");
        assert_eq!(
            s.change(restore, "true"),
            Ok(Some(SettingsChange::Value {
                table: Some("session".into()),
                key: "restore".into(),
                value: Some("true".into())
            })),
            "the default, written when it is not in the file"
        );
    }

    #[test]
    fn a_broken_file_makes_the_rows_read_only() {
        let s = shown("theme = \"auto\"\n[layout\n");
        assert!(s.broken.as_deref().is_some_and(|b| b.starts_with("settings.toml line 2: ")), "{:?}", s.broken);
        assert_eq!(s.change(0, "{home}/x"), Err("Fix settings.toml first".to_owned()));
        assert!(s.item(s.rows[0]).is_default, "no good text before: the defaults show");
        assert!(s.status().1 && s.status().0.starts_with("settings.toml line 2: "), "{:?}", s.status());
    }

    #[test]
    fn a_broken_file_shows_the_last_values_that_read() {
        let good =
            ConfigFiles { settings: Some("[files]\nconfirm-trash = true\n".to_owned()), ..ConfigFiles::default() };
        keep_last_good(&good);
        keep_last_good(&ConfigFiles { settings: Some("[files\n".to_owned()), ..ConfigFiles::default() });
        let s = shown("[files]\nconfirm-trash = true\n[layout\n");
        assert!(s.broken.is_some());
        let trash = s.item(s.rows[row_of(&s, "files.confirm-trash")]);
        assert_eq!((trash.value.as_str(), trash.is_default, trash.default.as_str()), ("true", false, "false"));
        let verify = s.item(s.rows[row_of(&s, "files.verify")]);
        assert_eq!((verify.is_default, verify.default.as_str()), (true, "false"), "the default text stays");
        LAST_GOOD.with(|l| l.borrow_mut().take());
    }

    #[test]
    fn the_advanced_page_counts_unknown_keys() {
        let mut s = shown("colour = 1\n");
        assert_eq!(s.status(), ("Type in the box above to search every page".to_owned(), false));
        s.page = Page::ALL.iter().position(|p| *p == Page::Advanced).unwrap();
        s.pick_rows();
        assert!(s.status().0.starts_with("1 key Gezik does not know"), "{:?}", s.status());
        assert!(s.rows.iter().all(|&i| FIELDS[i].kind == Kind::List));
    }

    #[test]
    fn the_keyboard_page_and_a_search_show_the_shortcuts_after_the_fields() {
        let mut s = shown("");
        assert!(s.key_rows.is_empty(), "General has none");
        s.page = Page::ALL.iter().position(|p| *p == Page::Keyboard).unwrap();
        s.pick_rows();
        assert_eq!(s.key_rows.len(), s.keys.len());
        assert_eq!((s.key_of(s.rows.len() - 1), s.key_of(s.rows.len())), (None, Some(s.key_rows[0])));
        s.query = "new-tab".to_owned();
        s.pick_rows();
        let titles: Vec<&str> = s.key_rows.iter().map(|&k| s.keys[k].title.as_str()).collect();
        assert_eq!(titles, ["New Tab"]);
        s.query = "no such thing".to_owned();
        s.pick_rows();
        assert_eq!(s.status().0, "No setting matches");
    }
}
