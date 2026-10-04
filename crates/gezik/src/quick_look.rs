//! The quick look window (Space): the selection, big, in its own window over Gezik's.

use gezik_config::theme::ResolvedTheme;
use slint::ComponentHandle;

use crate::{AppWindow, PreviewInfo, QuickLookWindow, Theme};

/// The share of the main window's size it takes.
const SHARE: f32 = 0.7;

pub struct QuickLook {
    window: QuickLookWindow,
}

impl QuickLook {
    /// Opens centered over `main`, showing `info`. `on_key` gets every key press (text and
    /// Ctrl, Alt, Shift, Meta) and returns whether it used it; `on_close` runs when the
    /// window's close button is pressed.
    pub fn open(
        main: &AppWindow,
        info: PreviewInfo,
        mono_font: slint::SharedString,
        on_key: impl Fn(&str, bool, bool, bool, bool) -> bool + 'static,
        on_close: impl Fn() + 'static,
    ) -> Result<QuickLook, slint::PlatformError> {
        let window = QuickLookWindow::new()?;
        if let Some(theme) = crate::theme_bridge::current() {
            crate::theme_bridge::apply_global(&window.global::<Theme>(), &theme);
        }
        window.set_info(info);
        window.set_mono_font(mono_font);
        window.on_key(move |event| {
            let m = event.modifiers;
            on_key(&event.text, m.control, m.alt, m.shift, m.meta)
        });
        window.window().on_close_requested(move || {
            on_close();
            slint::CloseRequestResponse::HideWindow
        });
        let main_window = main.window();
        let scale = main_window.scale_factor();
        let size = main_window.size().to_logical(scale);
        let (width, height) = (size.width * SHARE, size.height * SHARE);
        window.window().set_size(slint::LogicalSize::new(width, height));
        let origin = main_window.position();
        window.window().set_position(slint::PhysicalPosition::new(
            origin.x + ((size.width - width) * scale / 2.0) as i32,
            origin.y + ((size.height - height) * scale / 2.0) as i32,
        ));
        window.show()?;
        Ok(QuickLook { window })
    }

    pub fn set_info(&self, info: PreviewInfo) {
        self.window.set_info(info);
    }

    pub fn retheme(&self, theme: &ResolvedTheme) {
        crate::theme_bridge::apply_global(&self.window.global::<Theme>(), theme);
    }

    /// Width in logical pixels.
    pub fn width(&self) -> f32 {
        let native = self.window.window();
        native.size().to_logical(native.scale_factor()).width
    }

    pub fn close(self) {
        let _ = self.window.hide();
    }
}
