//! A CGImage (an NSImage's best rendition, a Quick Look thumbnail) as straight RGBA.

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGInterpolationQuality,
    kCGColorSpaceSRGB,
};

use crate::Rgba;
use crate::icons::{fit_within, unpremultiply};

/// `image` drawn at most `px` on its longer side (never enlarged), top row first. CoreGraphics
/// is not tied to a thread: any worker may call this.
pub fn cg_to_rgba(image: &CGImage, px: u32) -> Option<Rgba> {
    let (width, height) = fit_within(CGImage::width(Some(image)), CGImage::height(Some(image)), px)?;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: an extern static of CoreGraphics.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: `pixels` holds `height` rows of `width * 4` bytes and outlives the context,
    // which is dropped before `pixels` is read.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            width as usize * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: f64::from(width), height: f64::from(height) },
    };
    CGContext::draw_image(Some(&context), rect, Some(image));
    drop(context);
    unpremultiply(&mut pixels);
    Some(Rgba { width, height, pixels })
}
