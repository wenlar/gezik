//! Copies a resolved theme into the Slint `Theme` global.
//!
//! Slint's own `Palette` is read-only and is left to follow the system light/dark mode.
//! Gezik draws its own scroll bars, so no std widget takes its colors from it any more.

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

/// Makes the next frame redraw the whole window: the window's background is set to another
/// color and back, which marks all of it as changed (Slint has no direct call for this).
pub fn repaint_all(window: &AppWindow) {
    let global = window.global::<Theme>();
    let background = global.get_background();
    global.set_background(background.with_alpha(if background.alpha() == 255 { 0.99 } else { 1.0 }));
    global.set_background(background);
    window.window().request_redraw();
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
    global.set_progress(color(c.progress));
    global.set_progress_paused(color(c.progress_paused));
    global.set_progress_error(color(c.progress_error));
    global.set_drop_target(color(c.drop_target));
    global.set_surface_raised(color(c.surface_raised));
    global.set_chrome(color(c.chrome));
    global.set_border_strong(color(c.border_strong));
    global.set_accent_hover(color(c.accent_hover));
    global.set_accent_pressed(color(c.accent_pressed));
    global.set_selection_foreground_muted(color(c.selection_foreground_muted));
    global.set_selection_inactive(color(c.selection_inactive));
    global.set_pressed(color(c.pressed));
    global.set_tab_active(color(c.tab_active));
    global.set_tab_inactive(color(c.tab_inactive));
    global.set_input_background(color(c.input_background));
    global.set_danger_background(color(c.danger_background));
    global.set_success(color(c.success));
    global.set_warning(color(c.warning));
    global.set_shadow(color(c.shadow));
    global.set_overlay(color(c.overlay));
    global.set_scrollbar(color(c.scrollbar));
    global.set_scrollbar_hover(color(c.scrollbar_hover));

    let m = &theme.metrics;
    global.set_font_family(m.font_family.as_str().into());
    global.set_font_size(m.font_size);
    global.set_row_height(m.row_height);
    global.set_icon_size(m.icon_size);
    global.set_radius(m.radius);
    global.set_spacing(m.spacing);
    global.set_inset(m.inset);
}

/// `[layout] reduce-motion`: hover and popup fades take no time.
pub fn set_reduce_motion(window: &AppWindow, on: bool) {
    window.global::<Theme>().set_reduce_motion(on);
}
