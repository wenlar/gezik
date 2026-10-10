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

pub fn install(window: &AppWindow, ops: Operations) {
    window.set_native_menu_bar(true);
    let weak = window.as_weak();
    window.on_menu_command(move |name| {
        let Some(window) = weak.upgrade() else { return };
        match name.as_str() {
            "minimize" => window.window().set_minimized(true),
            "zoom" => window.window().set_maximized(!window.window().is_maximized()),
            name => {
                if let Some(index) = name.strip_prefix("command:").and_then(|i| i.parse::<usize>().ok()) {
                    return crate::actions::run_command(index, &crate::panes::active_view());
                }
                if let Some(id) = name.strip_prefix("tab-set:").and_then(|i| i.parse::<u32>().ok()) {
                    let names = crate::tab_sets::names();
                    return crate::tab_sets::with_current(|sets| sets.chosen(id, &names));
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
                        let weak = window.as_weak();
                        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                            if let Some(window) = weak.upgrade() {
                                play(&window, &chord);
                            }
                        });
                    }
                    // No shortcut (none by default, or turned off in settings.toml), or only
                    // the keypad's.
                    None if crate::actions::run(action, &nav, &view) => {}
                    None => {
                        if action == Action::BatchRename {
                            ops.batch_rename();
                        }
                    }
                }
            }
        }
    });
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
