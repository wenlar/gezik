// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Delegate the rendering to the [`i_slint_renderer_software::SoftwareRenderer`]

use core::num::NonZeroU32;
use core::ops::DerefMut;
use i_slint_core::graphics::Rgb8Pixel;
use i_slint_core::platform::PlatformError;
use i_slint_core::renderer::DrawOutcome;
pub use i_slint_renderer_software::SoftwareRenderer;
use i_slint_renderer_software::{PremultipliedRgbaColor, RepaintBufferType, TargetPixel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;

use super::WinitCompatibleRenderer;

pub struct WinitSoftwareRenderer {
    renderer: SoftwareRenderer,
    _context: RefCell<Option<softbuffer::Context<Arc<winit::window::Window>>>>,
    surface: RefCell<
        Option<softbuffer::Surface<Arc<winit::window::Window>, Arc<winit::window::Window>>>,
    >,
    /// Gezik patch: the size of the last frame drawn straight to the window (0 = none yet,
    /// the next frame is drawn whole).
    #[cfg(target_os = "windows")]
    drawn_size: core::cell::Cell<(u32, u32)>,
}

#[repr(transparent)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SoftBufferPixel(pub u32);

impl From<SoftBufferPixel> for PremultipliedRgbaColor {
    #[inline]
    fn from(pixel: SoftBufferPixel) -> Self {
        let v = pixel.0;
        PremultipliedRgbaColor {
            red: (v >> 16) as u8,
            green: (v >> 8) as u8,
            blue: v as u8,
            alpha: (v >> 24) as u8,
        }
    }
}

impl From<PremultipliedRgbaColor> for SoftBufferPixel {
    #[inline]
    fn from(pixel: PremultipliedRgbaColor) -> Self {
        Self(
            (pixel.alpha as u32) << 24
                | ((pixel.red as u32) << 16)
                | ((pixel.green as u32) << 8)
                | (pixel.blue as u32),
        )
    }
}

impl TargetPixel for SoftBufferPixel {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let mut x = PremultipliedRgbaColor::from(*self);
        x.blend(color);
        *self = x.into();
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(0xff000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    fn background() -> Self {
        Self(0)
    }
}

impl WinitSoftwareRenderer {
    pub fn new_suspended(
        _shared_backend_data: &Rc<crate::SharedBackendData>,
    ) -> Result<Box<dyn WinitCompatibleRenderer>, PlatformError> {
        Ok(Box::new(Self {
            renderer: SoftwareRenderer::new(),
            _context: RefCell::new(None),
            surface: RefCell::new(None),
            #[cfg(target_os = "windows")]
            drawn_size: Default::default(),
        }))
    }
}

/// Gezik patch: renders with `render_by_line` and copies bands of up to `BAND` lines to the
/// window with GDI. The window's content persists between frames (DWM keeps it), so only the
/// dirty region is drawn, as with softbuffer's reused buffer.
#[cfg(target_os = "windows")]
impl WinitSoftwareRenderer {
    fn render_to_window(&self, window: &winit::window::Window, width: u32, height: u32) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        const BAND: usize = 16;

        let Ok(RawWindowHandle::Win32(handle)) = window.window_handle().map(|h| h.as_raw()) else {
            return;
        };
        let hwnd = HWND(handle.hwnd.get() as _);

        let full = self.drawn_size.replace((width, height)) != (width, height);
        self.renderer.set_repaint_buffer_type(if full {
            RepaintBufferType::NewBuffer
        } else {
            RepaintBufferType::ReusedBuffer
        });

        // Lines of one range, `y` being the first; a dirty region has at most 3 ranges per line.
        struct Band {
            range: core::ops::Range<usize>,
            y: usize,
            lines: usize,
            pixels: Vec<SoftBufferPixel>,
        }
        struct Window {
            dc: Gdi::HDC,
            bands: Vec<Band>,
        }
        impl Window {
            fn flush(dc: Gdi::HDC, band: &mut Band) {
                if band.lines == 0 {
                    return;
                }
                let (w, h) = (band.range.len() as i32, band.lines as i32);
                let info = Gdi::BITMAPINFO {
                    bmiHeader: Gdi::BITMAPINFOHEADER {
                        biSize: core::mem::size_of::<Gdi::BITMAPINFOHEADER>() as u32,
                        biWidth: w,
                        biHeight: -h, // top-down
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: Gdi::BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                unsafe {
                    Gdi::StretchDIBits(
                        dc,
                        band.range.start as i32,
                        band.y as i32,
                        w,
                        h,
                        0,
                        0,
                        w,
                        h,
                        Some(band.pixels.as_ptr().cast()),
                        &info,
                        Gdi::DIB_RGB_COLORS,
                        Gdi::SRCCOPY,
                    );
                }
                band.lines = 0;
            }
        }
        impl i_slint_renderer_software::LineBufferProvider for &mut Window {
            type TargetPixel = SoftBufferPixel;
            fn process_line(
                &mut self,
                line: usize,
                range: core::ops::Range<usize>,
                render_fn: impl FnOnce(&mut [SoftBufferPixel]),
            ) {
                let dc = self.dc;
                let i = match self.bands.iter().position(|b| b.range == range) {
                    Some(i) => i,
                    None => {
                        let pixels = vec![SoftBufferPixel(0); range.len() * BAND];
                        self.bands.push(Band { range: range.clone(), y: line, lines: 0, pixels });
                        self.bands.len() - 1
                    }
                };
                let band = &mut self.bands[i];
                if band.lines == BAND || band.y + band.lines != line {
                    Window::flush(dc, band);
                }
                if band.lines == 0 {
                    band.y = line;
                }
                let w = range.len();
                render_fn(&mut band.pixels[band.lines * w..][..w]);
                band.lines += 1;
            }
        }

        let dc = unsafe { Gdi::GetDC(Some(hwnd)) };
        if dc.is_invalid() {
            self.drawn_size.set((0, 0));
            return;
        }
        let mut target = Window { dc, bands: Vec::new() };
        window.pre_present_notify();
        self.renderer.render_by_line(&mut target);
        for band in &mut target.bands {
            Window::flush(dc, band);
        }
        unsafe {
            Gdi::ReleaseDC(Some(hwnd), dc);
            let _ = Gdi::ValidateRect(Some(hwnd), None);
        }
    }
}

impl super::WinitCompatibleRenderer for WinitSoftwareRenderer {
    fn render(&self, window: &i_slint_core::api::Window) -> Result<DrawOutcome, PlatformError> {
        let size = window.size();

        let Some((width, height)) = size.width.try_into().ok().zip(size.height.try_into().ok())
        else {
            // Nothing to render
            return Ok(DrawOutcome::Success);
        };

        let mut borrowed_surface = self.surface.borrow_mut();
        let Some(surface) = borrowed_surface.as_mut() else {
            // Nothing to render
            return Ok(DrawOutcome::Success);
        };

        let winit_window = surface.window().clone();

        // Gezik patch: Windows may still ask a minimized window to draw; drawing would take
        // the frame buffer back that `occluded` gave up. The next frame after the restore is
        // drawn whole.
        #[cfg(target_os = "windows")]
        if winit_window.is_minimized() == Some(true) {
            return Ok(DrawOutcome::Success);
        }

        // Gezik patch: draw line by line straight to the window, so no frame buffer is kept.
        #[cfg(target_os = "windows")]
        if std::env::var_os("GEZIK_SOFTBUFFER").is_none() {
            self.render_to_window(&winit_window, NonZeroU32::get(width), NonZeroU32::get(height));
            return Ok(DrawOutcome::Success);
        }

        surface
            .resize(width, height)
            .map_err(|e| format!("Error resizing softbuffer surface: {e}"))?;

        let mut target_buffer = surface
            .buffer_mut()
            .map_err(|e| format!("Error retrieving softbuffer rendering buffer: {e}"))?;

        let age = target_buffer.age();
        self.renderer.set_repaint_buffer_type(match age {
            1 => RepaintBufferType::ReusedBuffer,
            2 => RepaintBufferType::SwappedBuffers,
            _ => RepaintBufferType::NewBuffer,
        });

        let region = if std::env::var_os("SLINT_LINE_BY_LINE").is_none() {
            let buffer: &mut [SoftBufferPixel] =
                bytemuck::cast_slice_mut(target_buffer.deref_mut());
            self.renderer.render(buffer, width.get() as usize)
        } else {
            // SLINT_LINE_BY_LINE is set and this is a debug mode where we also render in a Rgb565Pixel
            struct FrameBuffer<'a> {
                buffer: &'a mut [u32],
                line: Vec<i_slint_renderer_software::Rgb565Pixel>,
            }
            impl i_slint_renderer_software::LineBufferProvider for FrameBuffer<'_> {
                type TargetPixel = i_slint_renderer_software::Rgb565Pixel;
                fn process_line(
                    &mut self,
                    line: usize,
                    range: core::ops::Range<usize>,
                    render_fn: impl FnOnce(&mut [Self::TargetPixel]),
                ) {
                    let line_begin = line * self.line.len();
                    let sub = &mut self.line[..range.len()];
                    render_fn(sub);
                    for (dst, src) in self.buffer[line_begin..][range].iter_mut().zip(sub) {
                        let p = Rgb8Pixel::from(*src);
                        *dst =
                            0xff000000 | ((p.r as u32) << 16) | ((p.g as u32) << 8) | (p.b as u32);
                    }
                }
            }
            self.renderer.render_by_line(FrameBuffer {
                buffer: &mut target_buffer,
                line: vec![Default::default(); width.get() as usize],
            })
        };

        let damage = region
            .iter()
            .filter_map(|(pos, size)| {
                Some(softbuffer::Rect {
                    x: pos.x as u32,
                    y: pos.y as u32,
                    width: NonZeroU32::new(size.width)?,
                    height: NonZeroU32::new(size.height)?,
                })
            })
            .collect::<Vec<_>>();
        if !damage.is_empty() {
            winit_window.pre_present_notify();
            target_buffer
                .present_with_damage(&damage)
                .map_err(|e| format!("Error presenting softbuffer buffer: {e}"))?;
        }
        Ok(DrawOutcome::Success)
    }

    fn as_core_renderer(&self) -> &dyn i_slint_core::renderer::Renderer {
        &self.renderer
    }

    fn occluded(&self, _occluded: bool) {
        // On X11 and Windows, the buffer is completely cleared when the window is hidden
        // and the buffer age doesn't respect that, so clean the partial rendering cache
        self.renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        #[cfg(target_os = "windows")]
        self.drawn_size.set((0, 0));
        // Gezik patch: a hidden or minimized window gives its frame buffer (a DIB of
        // width x height x 4 bytes) back; a fresh surface on the same window and context has
        // none until the next frame, which is drawn whole (NewBuffer above).
        #[cfg(target_os = "windows")]
        if _occluded {
            let mut surface = self.surface.borrow_mut();
            let fresh = surface.as_ref().zip(self._context.borrow().as_ref()).and_then(
                |(surface, context)| softbuffer::Surface::new(context, surface.window().clone()).ok(),
            );
            if fresh.is_some() {
                *surface = fresh;
            }
        }
    }

    fn resume(
        &self,
        active_event_loop: &ActiveEventLoop,
        window_attributes: winit::window::WindowAttributes,
        _window_adapter_weak: std::rc::Weak<crate::winitwindowadapter::WinitWindowAdapter>,
    ) -> Result<Arc<winit::window::Window>, PlatformError> {
        let winit_window =
            active_event_loop.create_window(window_attributes).map_err(|winit_os_error| {
                PlatformError::from(format!(
                    "Error creating native window for software rendering: {winit_os_error}"
                ))
            })?;
        let winit_window = Arc::new(winit_window);

        let context = softbuffer::Context::new(winit_window.clone())
            .map_err(|e| format!("Error creating softbuffer context: {e}"))?;

        let surface = softbuffer::Surface::new(&context, winit_window.clone()).map_err(
            |softbuffer_error| format!("Error creating softbuffer surface: {softbuffer_error}"),
        )?;

        *self._context.borrow_mut() = Some(context);
        *self.surface.borrow_mut() = Some(surface);

        Ok(winit_window)
    }

    fn suspend(&self) -> Result<(), PlatformError> {
        drop(self.surface.borrow_mut().take());
        drop(self._context.borrow_mut().take());
        Ok(())
    }
}
