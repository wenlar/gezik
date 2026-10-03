//! Copies a resolved theme into the Slint `Theme` global.
//!
//! Slint's own `Palette` is read-only and is left to follow the system light/dark mode;
//! only std-widgets we still use (the ListView scrollbar) take their colors from it.

use gezik_config::Color;
use gezik_config::theme::ResolvedTheme;
use slint::ComponentHandle;

use crate::{AppWindow, Theme};

fn color(c: Color) -> slint::Color {
    slint::Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

/// No validation here: `gezik-config` guarantees every value is present and in range.
pub fn apply(window: &AppWindow, theme: &ResolvedTheme) {
    let global = window.global::<Theme>();
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
    global.set_folder_icon(color(c.folder_icon));
    global.set_file_icon(color(c.file_icon));
    global.set_danger(color(c.danger));

    let m = &theme.metrics;
    global.set_font_family(m.font_family.as_str().into());
    global.set_font_size(m.font_size);
    global.set_row_height(m.row_height);
    global.set_icon_size(m.icon_size);
    global.set_radius(m.radius);
    global.set_spacing(m.spacing);
}
