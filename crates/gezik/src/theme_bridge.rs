//! Copies a resolved theme into the Slint `Theme` global.
//!
//! Slint's own `Palette` is read-only and is left to follow the system light/dark mode;
//! only std-widgets we still use (the ListView scrollbar) take their colors from it.

use std::cell::RefCell;

use gezik_config::Color;
use gezik_config::theme::ResolvedTheme;
use slint::ComponentHandle;

use crate::{AppWindow, Theme};

fn color(c: Color) -> slint::Color {
    slint::Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

thread_local! {
    /// The theme on screen, for windows opened later (quick look).
    static CURRENT: RefCell<Option<ResolvedTheme>> = const { RefCell::new(None) };
}

/// The theme on screen now.
pub fn current() -> Option<ResolvedTheme> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Shows `theme` in the main window and every other open Gezik window.
pub fn apply(window: &AppWindow, theme: &ResolvedTheme) {
    apply_global(&window.global::<Theme>(), theme);
    CURRENT.with(|c| *c.borrow_mut() = Some(theme.clone()));
    crate::preview::with_current(|p| p.retheme(theme));
}

/// No validation here: `gezik-config` guarantees every value is present and in range.
pub fn apply_global(global: &Theme<'_>, theme: &ResolvedTheme) {
    let c = &theme.colors;
    global.set_background(color(c.background));
    global.set_surface(color(c.surface));
    global.set_foreground(color(c.foreground));
    global.set_foreground_muted(color(c.foreground_muted));
    global.set_border(color(c.border));
    global.set_accent(color(c.accent));
    global.set_accent_foreground(color(c.accent_foreground));
    global.set_selection(color(c.selection));
    global.set_selection_foreground(color(c.selection_foreground));
    global.set_hover(color(c.hover));
    global.set_icon_folder(color(c.icon_folder));
    global.set_icon_image(color(c.icon_image));
    global.set_icon_video(color(c.icon_video));
    global.set_icon_audio(color(c.icon_audio));
    global.set_icon_archive(color(c.icon_archive));
    global.set_icon_document(color(c.icon_document));
    global.set_icon_code(color(c.icon_code));
    global.set_icon_other(color(c.icon_other));
    global.set_focus_ring(color(c.focus_ring));
    global.set_marquee(color(c.marquee));
    global.set_danger(color(c.danger));

    let m = &theme.metrics;
    global.set_font_family(m.font_family.as_str().into());
    global.set_font_size(m.font_size);
    global.set_row_height(m.row_height);
    global.set_icon_size(m.icon_size);
    global.set_radius(m.radius);
    global.set_spacing(m.spacing);
}
