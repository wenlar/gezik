//! macOS: the menu bar at the top of the screen (`MenuBar` in app.slint, shown only here).
//! An item stands for an action: choosing it, by its shortcut or with the mouse, plays the
//! action's shortcut to the window, so it does just what the keys would do where the focus is
//! (⌘C copies the text of a name being edited, and the files otherwise). A keypad key cannot be
//! played (its text is the main key's): such an action, or one without a shortcut, is run here.

use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use slint::ComponentHandle;
use slint::platform::{Key as SlintKey, WindowEvent};

use crate::AppWindow;
use crate::keys;
use crate::navigation::Navigator;
use crate::operations::Operations;
use crate::view::View;

pub fn install(window: &AppWindow, view: View, nav: Navigator, ops: Operations) {
    window.set_native_menu_bar(true);
    let weak = window.as_weak();
    window.on_menu_command(move |name| {
        let Some(window) = weak.upgrade() else { return };
        match name.as_str() {
            "minimize" => window.window().set_minimized(true),
            "zoom" => window.window().set_maximized(!window.window().is_maximized()),
            name => {
                let Some(action) = Action::from_name(name) else { return };
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
                    None => match action {
                        Action::BatchRename => ops.batch_rename(),
                        Action::ToggleHidden => {
                            view.toggle_hidden();
                            nav.reload();
                        }
                        _ => {}
                    },
                }
            }
        }
    });
}

/// Presses and releases `chord`'s keys in the window, as the keyboard would.
fn play(window: &AppWindow, chord: &Chord) {
    let (modifiers, text) = keys::slint_keys(chord, Platform::Mac);
    let send = |event| window.window().dispatch_event(event);
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
