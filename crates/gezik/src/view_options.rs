//! The view options of `[view]` that are the same in every folder (spec 7). settings.toml has
//! them; the View menu and toggle-hidden change them there through the one writer, showing the
//! change at once (as the sidebar does with its pins), and every part of the window reads them
//! here.

use std::cell::RefCell;
use std::time::SystemTime;

use gezik_config::Warning;
use gezik_config::settings::ViewOption;
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
use slint::ComponentHandle;

use crate::AppWindow;

/// What is shown, what settings.toml has, and how many writes are on their way.
#[derive(Debug, Default)]
struct OptionsState {
    shown: ViewOptions,
    in_file: ViewOptions,
    on_their_way: usize,
}

impl OptionsState {
    /// settings.toml was read: the options to show now, if they changed. While writes are on
    /// their way the file is behind them, and what is shown stays.
    fn read(&mut self, options: ViewOptions) -> Option<ViewOptions> {
        self.in_file = options;
        if self.on_their_way > 0 || self.shown == options {
            return None;
        }
        self.shown = options;
        Some(options)
    }

    /// `option` changed from a menu: shown at once, its write on its way.
    fn change(&mut self, option: ViewOption) -> ViewOptions {
        option.apply(&mut self.shown);
        self.on_their_way += 1;
        self.shown
    }

    /// `option` changed with no settings.toml to write: it lives in memory.
    fn change_in_memory(&mut self, option: ViewOption) -> ViewOptions {
        option.apply(&mut self.shown);
        self.in_file = self.shown;
        self.shown
    }

    /// A write ended (`option`: what it wrote). After the last one, what the file has
    /// is shown: the same after a success, the values from before after a failure.
    fn written(&mut self, ok: bool, option: ViewOption) -> Option<ViewOptions> {
        self.on_their_way = self.on_their_way.saturating_sub(1);
        if ok {
            option.apply(&mut self.in_file);
        }
        if self.on_their_way > 0 || self.shown == self.in_file {
            return None;
        }
        self.shown = self.in_file;
        Some(self.shown)
    }
}

thread_local! {
    static STATE: RefCell<OptionsState> = RefCell::new(OptionsState::default());
    /// The window (for the macOS menu bar's marks) and where settings.toml is.
    static CONTEXT: RefCell<Option<(slint::Weak<AppWindow>, Option<ConfigStore>)>> = const { RefCell::new(None) };
}

/// Once, at start, before the settings are first applied.
pub fn install(window: &AppWindow, store: Option<ConfigStore>) {
    CONTEXT.with(|c| *c.borrow_mut() = Some((window.as_weak(), store)));
    sync_window(current());
}

/// The options in effect.
pub fn current() -> ViewOptions {
    STATE.with(|s| s.borrow().shown)
}

/// settings.toml was read (every resolve).
pub fn set_from_file(options: ViewOptions) {
    let shown = STATE.with(|s| s.borrow_mut().read(options));
    if let Some(options) = shown {
        show(options);
    }
}

/// The View menu (or toggle-hidden) changes `option`: shown at once and written into
/// settings.toml; a failure is said in the status bar and the file's values come back.
pub fn change(option: ViewOption) {
    // Already so: nothing to show, nothing to write.
    let mut next = current();
    option.apply(&mut next);
    if next == current() {
        return;
    }
    let store = CONTEXT.with(|c| c.borrow().as_ref().and_then(|(_, store)| store.clone()));
    let Some(store) = store else {
        let shown = STATE.with(|s| s.borrow_mut().change_in_memory(option));
        return show(shown);
    };
    let shown = STATE.with(|s| s.borrow_mut().change(option));
    show(shown);
    store.write_settings(SettingsChange::ViewOption(option), move |result| {
        let _ = slint::invoke_from_event_loop(move || written(option, result));
    });
}

fn written(option: ViewOption, result: Result<(), Warning>) {
    if let Err(warning) = &result {
        crate::view::with_current(|view| view.note(warning.to_string()));
    }
    let back = STATE.with(|s| s.borrow_mut().written(result.is_ok(), option));
    if let Some(options) = back {
        show(options);
    }
}

/// `toggle-hidden` (Ctrl+H, Cmd+Shift+.): writes `show-hidden` (spec 7.2).
pub fn toggle_hidden() {
    change(ViewOption::ShowHidden(!current().show_hidden));
}

/// Applies `options` to the list; the folder is read again when hidden or system items
/// come or go.
fn show(options: ViewOptions) {
    let mut reload = false;
    crate::view::with_current(|view| reload = view.set_options(options));
    if reload {
        crate::navigation::with_current(|nav| nav.reload());
        // Hidden or system folders come or go in the sidebar tree's open branches too.
        crate::sidebar::with_current(crate::sidebar::Sidebar::options_changed);
    }
    sync_window(options);
}

/// Puts the macOS menu bar's View marks as `options` are (elsewhere the window has the
/// properties too, unused). Also after a choice that changed nothing: Slint flipped that
/// item's mark itself.
pub fn sync_window(options: ViewOptions) {
    let window = CONTEXT.with(|c| c.borrow().as_ref().and_then(|(window, _)| window.upgrade()));
    let Some(window) = window else { return };
    window.set_view_hide_extensions(options.hide_extensions);
    window.set_view_folders_first(options.folders_first);
    window.set_view_single_click(options.single_click_open);
    window.set_view_show_hidden(options.show_hidden);
    window.set_view_date_relative(options.date_format == DateFormat::Relative);
    window.set_view_date_short(options.date_format == DateFormat::Short);
    window.set_view_date_iso(options.date_format == DateFormat::Iso);
    window.set_view_date_system(options.date_format == DateFormat::System);
    window.set_view_size_binary(options.size_format == SizeFormat::Binary);
    window.set_view_size_decimal(options.size_format == SizeFormat::Decimal);
}

/// A size as `[view] size-format` writes it (the list, the status bar, the preview, the
/// operations panel, the conflict list).
pub fn size_text(bytes: u64) -> String {
    gezik_core::format_size_in(bytes, current().size_format)
}

/// A time as `[view] date-format` writes it.
pub fn date_text(time: SystemTime) -> String {
    gezik_platform::format_date(time, current().date_format, SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::view::{DateFormat, SizeFormat};

    #[test]
    fn a_reload_while_writes_are_on_their_way_is_ignored() {
        let mut state = OptionsState::default();
        let first = state.change(ViewOption::HideExtensions(true));
        let second = state.change(ViewOption::DateFormat(DateFormat::Iso));
        assert!(second.hide_extensions && second.date_format == DateFormat::Iso);
        // The reload after the first write: the file has only that one yet.
        assert_eq!(state.read(first), None, "what is shown stays");
        assert_eq!(state.written(true, ViewOption::HideExtensions(true)), None);
        assert_eq!(
            state.written(true, ViewOption::DateFormat(DateFormat::Iso)),
            None,
            "the last one: the file is what is shown"
        );
        assert_eq!(state.shown, second);
        assert_eq!(state.read(second), None, "its own reload changes nothing");
        let by_hand = ViewOptions { size_format: SizeFormat::Decimal, ..second };
        assert_eq!(state.read(by_hand), Some(by_hand), "a hand edit shows at once");
    }

    #[test]
    fn a_failed_write_goes_back_to_the_file() {
        let mut state = OptionsState::default();
        let before = state.shown;
        let changed = state.change(ViewOption::ShowHidden(!before.show_hidden));
        assert_ne!(changed, before);
        assert_eq!(
            state.written(false, ViewOption::ShowHidden(!before.show_hidden)),
            Some(before),
            "back to what the file has"
        );
        assert_eq!(state.shown, before);
    }

    #[test]
    fn a_failed_write_does_not_leave_its_value_in_the_file() {
        let mut state = OptionsState::default();
        let a = ViewOption::HideExtensions(true);
        let b = ViewOption::FoldersFirst(false);
        state.change(a);
        state.change(b);
        assert_eq!(state.written(false, a), None, "another write is still on its way");
        let back = state.written(true, b).expect("what the file has is shown");
        assert!(!back.hide_extensions, "A never reached the file");
        assert!(!back.folders_first);
    }
}
