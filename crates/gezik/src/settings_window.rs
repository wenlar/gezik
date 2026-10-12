//! Settings… (spec 14 §4): settings.toml's single settings in a window of their own. The rows
//! come from `gezik_config::schema`: descriptions and defaults from the template, values from the
//! file's text as last read (the UI thread reads no file). A change is written at once through
//! the settings writer's queue; the reload that follows updates every window and this one.
//! The window is made when it opens and let go when it closes: nothing of it stays.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::schema::{self, Described, FIELDS, Field, Kind, Page};
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigFiles;
use gezik_config::theme::{BUILTIN_THEMES, ResolvedTheme};
use gezik_core::nav::Location;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{SettingItem, SettingsWindow, Theme};

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
}

impl Shown {
    fn read(&mut self, files: &ConfigFiles) {
        let text = files.settings.as_deref().unwrap_or(gezik_config::store::SETTINGS_TEMPLATE);
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
            description: described.description.as_str().into(),
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
        if self.rows.is_empty() {
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
    let mut shown = Shown { described: schema::describe(gezik_config::store::SETTINGS_TEMPLATE), ..Shown::default() };
    if let Some(files) = crate::windows::files() {
        shown.read(&gezik_config::lock(&files));
    } else {
        shown.read(&ConfigFiles::default());
    }
    shown.pick_rows();
    let links = ["Open settings.toml", "Open Config Folder"];
    let pages: Vec<SharedString> = Page::ALL.iter().map(|p| p.title()).chain(links).map(Into::into).collect();
    window.set_pages(ModelRc::new(VecModel::from(pages)));
    let open = Rc::new(Open {
        window,
        shown: RefCell::new(shown),
        rows: Rc::new(VecModel::default()),
        previews: Cell::new(0),
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

/// Lets the window go (its close button, the end of main).
pub fn close() {
    let Some(open) = OPEN.with(|o| o.borrow_mut().take()) else { return };
    let _ = open.window.hide();
    end_preview_now();
}

/// The config files were read again (settings.toml or a theme changed): the rows follow.
pub fn refresh(files: &ConfigFiles) {
    let Some(open) = current() else { return };
    open.shown.borrow_mut().read(files);
    show_rows(&open, false);
}

/// The theme shown changed: this window too.
pub fn retheme(theme: &ResolvedTheme) {
    if let Some(open) = current() {
        crate::theme_bridge::apply_global(&open.window.global::<Theme>(), theme);
    }
}

/// Puts the rows in the window: all new (a page or a search) or row by row, so a field being
/// typed in keeps its text.
fn show_rows(open: &Open, new: bool) {
    let shown = open.shown.borrow();
    let items: Vec<SettingItem> = shown.rows.iter().map(|&i| shown.item(i)).collect();
    if new || items.len() != open.rows.row_count() {
        open.rows.set_vec(items);
    } else {
        for (row, item) in items.into_iter().enumerate() {
            if open.rows.row_data(row).is_none_or(|old| !same(&old, &item)) {
                open.rows.set_row_data(row, item);
            }
        }
    }
    open.window.set_page(i32::try_from(shown.page).unwrap_or(0));
    open.window.set_read_only(shown.broken.is_some());
    let (status, bad) = shown.status();
    open.window.set_status(status.into());
    open.window.set_status_bad(bad);
}

/// Whether two rows show the same (their choices are new models each time).
fn same(a: &SettingItem, b: &SettingItem) -> bool {
    (&a.key, &a.value, a.is_default, &a.description) == (&b.key, &b.value, b.is_default, &b.description)
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
    window.on_hovered({
        let weak = weak.clone();
        move |row, option| {
            let Some(open) = weak.upgrade() else { return };
            if field_of(&open, row).is_some_and(|i| FIELDS[i].kind == Kind::Theme) {
                drop(open);
                preview(&option);
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
            let Some(open) = weak.upgrade() else { return };
            let Some(i) = field_of(&open, row) else { return };
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
        }
    });
    window.on_escape(|| slint::Timer::single_shot(Duration::ZERO, close));
    window.window().on_close_requested(|| {
        // Not inside the window's own event: on the next turn of the event loop.
        slint::Timer::single_shot(Duration::ZERO, close);
        slint::CloseRequestResponse::HideWindow
    });
}

/// Opens the config folder in a new tab of the window last used.
fn open_folder() {
    let Some(dir) = crate::windows::config().map(|store| store.dir().to_owned()) else { return };
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
    let Some(path) = crate::windows::config().map(|store| store.dir().join("settings.toml")) else { return };
    if let Err(err) = open::that_detached(&path) {
        let text = format!("Cannot open {}: {}", path.display(), gezik_platform::fs::describe(&err));
        match current() {
            Some(open) => say_bad(&open, &text),
            None => note(text),
        }
    }
}

/// Shows theme `id` in every window without writing it ("" or "auto": back to the theme in
/// effect, a moment later so that moving from one choice to the next does not flash).
fn preview(id: &str) {
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
    let resolved = gezik_config::theme::resolve_theme(id, &gezik_config::lock(&files).themes, &mut Vec::new());
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
        shown.read(&files);
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
}
