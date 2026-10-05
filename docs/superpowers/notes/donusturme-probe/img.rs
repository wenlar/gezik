//! Image conversion with the `image` crate (0.25.10) and fast_image_resize (6.1.0).
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use fast_image_resize::{images::Image as FirImage, FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::bmp::BmpEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader, Rgb, RgbImage};

/// What we read from an input: pixels plus the metadata we may carry over.
pub struct Decoded {
    pub img: DynamicImage,
    pub format: Option<image::ImageFormat>,
    pub icc: Option<Vec<u8>>,
    /// Raw TIFF payload (starts with `II*\0` or `MM\0*`), without the `Exif\0\0` prefix.
    pub exif: Option<Vec<u8>>,
    pub orientation: Orientation,
}

pub fn decode(path: &Path) -> image::ImageResult<Decoded> {
    let reader = ImageReader::open(path)?.with_guessed_format()?; // sniff by content, not extension
    let format = reader.format();
    let mut decoder = reader.into_decoder()?;
    // Metadata must be read BEFORE from_decoder() consumes the decoder.
    let icc = decoder.icc_profile()?;
    let exif = decoder.exif_metadata()?;
    let orientation = decoder.orientation()?;
    let img = DynamicImage::from_decoder(decoder)?; // GIF/WebP/PNG(APNG): first frame
    Ok(Decoded { img, format, icc, exif, orientation })
}

#[derive(Clone, Copy, Debug)]
pub enum Fit {
    LongestSide(u32),
    Width(u32),
    Height(u32),
    Percent(u32),
}

/// Target size for a resize; `None` = keep the size (also when "never enlarge" blocks it).
pub fn target_size(w: u32, h: u32, fit: Fit, never_enlarge: bool) -> Option<(u32, u32)> {
    let scale = match fit {
        Fit::LongestSide(n) => n as f64 / w.max(h) as f64,
        Fit::Width(n) => n as f64 / w as f64,
        Fit::Height(n) => n as f64 / h as f64,
        Fit::Percent(p) => p as f64 / 100.0,
    };
    if never_enlarge && scale >= 1.0 {
        return None;
    }
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    if (nw, nh) == (w, h) { None } else { Some((nw, nh)) }
}

/// Lanczos3 resize that keeps the pixel type (L8, La8, Rgb8, Rgba8, the 16-bit ones, Rgb32F...).
/// Alpha is premultiplied and divided back by fast_image_resize (`use_alpha` defaults to true).
pub fn resize(img: &DynamicImage, nw: u32, nh: u32, resizer: &mut Resizer) -> DynamicImage {
    // A DynamicImage of the same variant as the source is the destination.
    let mut dst = match img {
        DynamicImage::ImageLuma8(_) => DynamicImage::new_luma8(nw, nh),
        DynamicImage::ImageLumaA8(_) => DynamicImage::new_luma_a8(nw, nh),
        DynamicImage::ImageRgb8(_) => DynamicImage::new_rgb8(nw, nh),
        DynamicImage::ImageRgba8(_) => DynamicImage::new_rgba8(nw, nh),
        DynamicImage::ImageLuma16(_) => DynamicImage::new_luma16(nw, nh),
        DynamicImage::ImageLumaA16(_) => DynamicImage::new_luma_a16(nw, nh),
        DynamicImage::ImageRgb16(_) => DynamicImage::new_rgb16(nw, nh),
        DynamicImage::ImageRgba16(_) => DynamicImage::new_rgba16(nw, nh),
        DynamicImage::ImageRgb32F(_) => DynamicImage::new_rgb32f(nw, nh),
        _ => DynamicImage::new_rgba32f(nw, nh), // Rgba32F (non_exhaustive enum)
    };
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    resizer.resize(img, &mut dst, &opts).expect("same pixel type");
    dst
}

/// Same resize through a raw buffer (for when the pixels are not in a DynamicImage).
#[allow(dead_code)]
pub fn resize_rgba8_raw(w: u32, h: u32, rgba: Vec<u8>, nw: u32, nh: u32) -> Vec<u8> {
    let src = FirImage::from_vec_u8(w, h, rgba, fast_image_resize::PixelType::U8x4).unwrap();
    let mut dst = FirImage::new(nw, nh, fast_image_resize::PixelType::U8x4);
    Resizer::new().resize(&src, &mut dst, None).unwrap(); // None = Lanczos3 + use_alpha
    dst.into_vec()
}

/// Composite onto an opaque background colour; result is Rgb8 (what JPEG needs).
pub fn flatten(img: &DynamicImage, bg: [u8; 3]) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8(); // 16-bit -> 8-bit happens here too (value >> 8 with rounding)
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p[3] as u32;
        let mix = |c: u8, b: u8| ((c as u32 * a + b as u32 * (255 - a) + 127) / 255) as u8;
        *o = Rgb([mix(p[0], bg[0]), mix(p[1], bg[1]), mix(p[2], bg[2])]);
    }
    out
}

pub enum OutFormat {
    Jpeg { quality: u8, icc: Option<Vec<u8>>, exif: Option<Vec<u8>> },
    Png,
    WebpLossless,
    Bmp,
}

pub fn encode(img: &DynamicImage, out: &Path, fmt: OutFormat) -> image::ImageResult<()> {
    let mut w = BufWriter::new(File::create(out)?);
    match fmt {
        OutFormat::Jpeg { quality, icc, exif } => {
            let mut enc = JpegEncoder::new_with_quality(&mut w, quality); // 1..=100 (clamped)
            if let Some(icc) = icc {
                enc.set_icc_profile(icc).unwrap(); // APP2 ICC_PROFILE chunks, Ok for JPEG
            }
            if let Some(exif) = exif {
                enc.set_exif_metadata(exif).unwrap(); // APP1, encoder adds "Exif\0\0"
            }
            img.write_with_encoder(enc)?; // Rgba8/16-bit -> Rgb8 by dropping alpha: flatten first!
        }
        OutFormat::Png => {
            let enc = PngEncoder::new_with_quality(&mut w, CompressionType::Default, PngFilter::Adaptive);
            img.write_with_encoder(enc)?; // keeps 16-bit and alpha
        }
        OutFormat::WebpLossless => {
            img.write_with_encoder(WebPEncoder::new_lossless(&mut w))?; // 16-bit -> 8-bit
        }
        OutFormat::Bmp => {
            img.write_with_encoder(BmpEncoder::new(&mut w))?; // 16-bit -> 8-bit, alpha kept (32bpp)
        }
    }
    w.flush()?;
    Ok(())
}

/// Read only the metadata of an encoded file again (for checks).
pub fn read_meta(path: &Path) -> (Option<Vec<u8>>, Option<Vec<u8>>, Orientation) {
    let r = ImageReader::new(BufReader::new(File::open(path).unwrap())).with_guessed_format().unwrap();
    let mut d = r.into_decoder().unwrap();
    (d.icc_profile().unwrap(), d.exif_metadata().unwrap(), d.orientation().unwrap())
}
