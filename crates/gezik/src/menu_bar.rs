//! macOS: the menu bar at the top of the screen (`MenuBar` in app.slint, shown only here).
//! An item stands for an action: choosing it, by its shortcut or with the mouse, plays the
//! action's shortcut to the window, so it does just what the keys would do where the focus is
//! (⌘C copies the text of a name being edited, and the files otherwise). A keypad key cannot be
//! played (its text is the main key's): such an action, or one without a shortcut, is run here.

use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_core::batch::convert::CommandSpec;
use slint::platform::{Key as SlintKey, WindowEvent};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::keys;
use crate::operations::Operations;
use crate::{AppWindow, MenuEntry};

/// The menu bar of the window being opened: its items run in that window (muda's events are
/// no winit events, so the router has not made it the current one).
pub fn install(window: &AppWindow, ops: Operations) {
    window.set_native_menu_bar(true);
    let id = crate::windows::current();
    window.on_menu_command(move |name| {
        crate::windows::with_window(id, |window| chosen(window, &name, &ops));
    });
}

fn chosen(window: &AppWindow, name: &str, ops: &Operations) {
    // The window the item was chosen in, for the keys played after it.
    let id = crate::windows::current();
    match name {
        "minimize" => window.window().set_minimized(true),
        "zoom" => window.window().set_maximized(!window.window().is_maximized()),
        name => {
            if let Some(index) = name.strip_prefix("command:").and_then(|i| i.parse::<usize>().ok()) {
                return crate::actions::run_command(index, &crate::panes::active_view());
            }
            if let Some(number) = name.strip_prefix("window:").and_then(|i| i.parse::<i32>().ok()) {
                return crate::windows::raise_number(number);
            }
            if let Some(id) = name.strip_prefix("tab-set:").and_then(|i| i.parse::<u32>().ok()) {
                let names = crate::tab_sets::names();
                return crate::tab_sets::with_current(|sets| sets.chosen(id, &names));
            }
            // Sorting or grouping by a property: the picker of fields (spec 13 §8.2).
            if let Some(purpose) = match name {
                "sort-property" => Some(crate::field_picker::Purpose::Sort),
                "group-property" => Some(crate::field_picker::Purpose::Group),
                _ => None,
            } {
                return crate::field_picker::with_current(|p| p.open(purpose));
            }
            if let Some(index) = name.strip_prefix("tag:").and_then(|i| i.parse::<usize>().ok()) {
                return crate::tags::toggle_nth(index + 1, &crate::panes::active_view());
            }
            if name == "tag-dots" {
                return crate::tags::toggle_dots();
            }
            if name == "group-tag" {
                return crate::panes::active_view().set_group(gezik_core::group::GroupBy::Tag);
            }
            if name == "group-rating" {
                return crate::panes::active_view().set_group(gezik_core::group::GroupBy::Rating);
            }
            if let Some(stars) = name.strip_prefix("rating:").and_then(|n| n.parse::<u8>().ok()) {
                return crate::tags::rate(stars, &crate::panes::active_view());
            }
            if let Some(id) = name.strip_prefix("view-option:").and_then(|i| i.parse::<u32>().ok()) {
                let options = crate::view_options::current();
                if let Some(option) = crate::context_menu::view_option_for(id, options) {
                    crate::view_options::change(option);
                }
                return crate::view_options::sync_window(crate::view_options::current());
            }
            let Some(action) = Action::from_name(name) else { return };
            let (nav, view) = (crate::panes::active_nav(), crate::panes::active_view());
            if crate::trash_view::instead(action, &view) {
                return;
            }
            match keys::chord_for(action).filter(|c| !matches!(c.key, Key::Num(_))) {
                // After the menu is done with this item: playing the keys changes the
                // menu's shortcuts, which Slint must not rebuild while it activates one.
                Some(chord) => {
                    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                        crate::windows::with_window(id, |window| play(window, &chord));
                    });
                }
                // No shortcut (none by default, or turned off in settings.toml), or only
                // the keypad's: run as the palette runs it (Show Preview as Hex has no key).
                None if crate::actions::run(action, &nav, &view) => {}
                None => {
                    if let Some(preview) = crate::preview::with_current(crate::preview::Preview::clone) {
                        crate::perform(action, window, &nav, &view, &preview, ops);
                    }
                }
            }
        }
    }
}

/// The Commands menu: the commands that have a key, with the key in the title.
pub fn set_commands(window: &AppWindow, commands: &[CommandSpec]) {
    let entries = crate::convert::bar_entries(commands, |i| {
        keys::command_chord(i).map(|chord| keys::chord_label(&chord, Platform::Mac))
    });
    let entries: Vec<MenuEntry> = entries
        .into_iter()
        .map(|(id, title, enabled)| MenuEntry { id, title: crate::context_menu::menu_title(&title).into(), enabled })
        .collect();
    window.set_bar_commands(ModelRc::new(VecModel::from(entries)));
}

/// The Window menu's "Open Tab Set": the sets to open, to replace the tabs with, to delete.
pub fn set_tab_sets(window: &AppWindow, names: &[String]) {
    let entries: Vec<MenuEntry> = crate::context_menu::tab_set_items(names)
        .into_iter()
        .map(|(id, title, enabled)| MenuEntry {
            id: i32::try_from(id).unwrap_or(-1),
            title: crate::context_menu::menu_title(&title).into(),
            enabled,
        })
        .collect();
    window.set_bar_tab_sets(ModelRc::new(VecModel::from(entries)));
}

/// File ▸ Tags: the list's tags; the first seven say their `tag-N` keys (⌃1…⌃7 by default).
pub fn set_tags(window: &AppWindow, list: &[gezik_core::meta::Tag]) {
    let entries: Vec<MenuEntry> = list
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let keys = crate::tags::key_label(list, &t.name);
            let title = if keys.is_empty() { t.name.to_string() } else { format!("{}    {keys}", t.name) };
            MenuEntry {
                id: i32::try_from(i).unwrap_or(-1),
                title: crate::context_menu::menu_title(&title).into(),
                enabled: true,
            }
        })
        .collect();
    window.set_bar_tags(ModelRc::new(VecModel::from(entries)));
}

/// Presses and releases `chord`'s keys in the window, as the keyboard would.
fn play(window: &AppWindow, chord: &Chord) {
    let (modifiers, text) = keys::slint_keys(chord, Platform::Mac);
    let send = |event| window.window().dispatch_event(event);
    // The real press the menu bar took never reached `key-event`: its noted physical key
    // (keys.rs `Physical`) must not pass to these made-up ones.
    let _ = keys::take_pressed();
    // Slint matches the menu bar's shortcuts first (as macOS does): it would take these keys
    // for the item again instead of passing them on.
    window.set_menu_keys(false);
    for key in &modifiers {
        send(WindowEvent::KeyPressed { text: (*key).into() });
    }
    send(WindowEvent::KeyPressed { text: text.clone().into() });
    send(WindowEvent::KeyReleased { text: text.into() });
    for key in modifiers.iter().rev() {
        send(WindowEvent::KeyReleased { text: (*key).into() });
    }
    window.set_menu_keys(true);
    // Back in step with the keys still held (the shortcut's own ⌘, for one): their release
    // reaches the window as usual.
    let held = gezik_platform::key_place::modifiers_held();
    for (down, key) in [
        (held.command, SlintKey::Control),
        (held.control, SlintKey::Meta),
        (held.option, SlintKey::Alt),
        (held.shift, SlintKey::Shift),
    ] {
        if down {
            send(WindowEvent::KeyPressed { text: key.into() });
        }
    }
}
