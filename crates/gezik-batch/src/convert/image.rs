//! Converting one picture: read it with its metadata, turn it upright by its EXIF
//! orientation, resize it, and write it as JPEG (jpeg-encoder), PNG, lossless WebP or BMP
//! (image), or as lossy WebP or AVIF through ffmpeg. HEIC, HEIF and AVIF are read through
//! ffmpeg 9 or later. "Remove location data" leaves a JPEG's picture untouched.
//!
//! Metadata: the ICC profile is kept (the colours stay right); a JPEG made from a JPEG keeps
//! its EXIF, with the orientation reset once the pixels are turned, the thumbnail dropped and
//! the pixel size updated; `strip_metadata` drops EXIF (location included) and XMP. Other
//! formats carry no EXIF, and nothing written here carries XMP.

use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use ::image::codecs::bmp::BmpEncoder;
use ::image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use ::image::codecs::webp::WebPEncoder;
use ::image::metadata::Orientation;
use ::image::{ColorType, DynamicImage, ImageDecoder, ImageEncoder, ImageReader, Limits, Rgb, RgbImage, RgbaImage};
use fast_image_resize::pixels::U8x4;
use fast_image_resize::{FilterType, IntoImageView, IntoImageViewMut, ResizeAlg, ResizeOptions, Resizer};
use gezik_core::batch::convert::{
    FfmpegImage, ImageFormat, ImageOptions, MAX_SIDE, ffmpeg_image_args, needs_ffmpeg_to_read, target_size,
};

use super::exifclean;
use super::ffmpeg::{run_ffmpeg, version};

/// One picture to convert.
pub struct ImageJob<'a> {
    pub input: &'a Path,
    /// Where the result goes: a temporary name, renamed by the caller. Gone again on failure.
    pub output: &'a Path,
    pub options: &'a ImageOptions,
    /// The ffmpeg to use for HEIC, HEIF and AVIF inputs (9.0 or later) and for lossy WebP and
    /// AVIF outputs.
    pub ffmpeg: Option<&'a Path>,
}

/// Why a picture could not be converted, carried in an `io::Error` (`io::Error::other`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// A picture Gezik cannot read, or cannot write in the format asked for.
    NotSupported(String),
    /// It takes ffmpeg (9.0 or later to read HEIC, HEIF and AVIF), and there is none.
    NeedsFfmpeg,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::NotSupported(why) => write!(f, "{why}"),
            ImageError::NeedsFfmpeg => write!(f, "ffmpeg needed"),
        }
    }
}

impl std::error::Error for ImageError {}

/// The oldest ffmpeg that reads HEIC: older ones turn an iPhone photo (a grid of tiles) into
/// its first 512×512 tile without an error.
pub const FFMPEG_TO_READ: (u32, u32) = (9, 0);

/// Whether a conversion failed for want of ffmpeg.
pub fn is_needs_ffmpeg(err: &io::Error) -> bool {
    err.get_ref().and_then(|inner| inner.downcast_ref::<ImageError>()) == Some(&ImageError::NeedsFfmpeg)
}

fn needs_ffmpeg() -> io::Error {
    io::Error::other(ImageError::NeedsFfmpeg)
}

fn not_supported(why: impl Into<String>) -> io::Error {
    io::Error::other(ImageError::NotSupported(why.into()))
}

fn too_large() -> io::Error {
    not_supported("too large to convert")
}

/// The most bytes one picture buffer may take: image's default allocation limit (512 MiB).
const MAX_BUFFER: u64 = 512 << 20;

/// Whether a `w`×`h` buffer of `bytes_per_pixel` fits in [`MAX_BUFFER`].
fn fits(w: u32, h: u32, bytes_per_pixel: u64) -> bool {
    u64::from(w) * u64::from(h) * bytes_per_pixel <= MAX_BUFFER
}

/// The ICC profile, if it is for the output's colours: grey or RGB (bytes 16-19 name the
/// colour space). A grey picture written in colour, or a CMYK one decoded to RGB, goes
/// without it.
fn icc_for(icc: Option<&[u8]>, grey: bool) -> Option<&[u8]> {
    let wanted: &[u8] = if grey { b"GRAY" } else { b"RGB " };
    icc.filter(|icc| icc.get(16..20) == Some(wanted))
}

fn is_grey(img: &DynamicImage) -> bool {
    matches!(img.color(), ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16)
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

fn check(stop: &dyn Fn() -> bool) -> io::Result<()> {
    if stop() { Err(cancelled()) } else { Ok(()) }
}

/// Converts `job.input` into `job.output` as `job.options` say. `stop` is asked between the
/// steps (and while ffmpeg runs); then the answer is `Interrupted`. On any failure nothing
/// is left at `job.output`, nor any temporary file.
pub fn convert_image(job: &ImageJob, stop: &dyn Fn() -> bool) -> io::Result<()> {
    let result = convert(job, stop);
    if result.is_err() {
        let _ = std::fs::remove_file(job.output);
    }
    result
}

/// "Remove location data" for one picture: a JPEG written as a JPEG loses its location
/// without being re-encoded ([`strip_location`]); anything else (or a JPEG whose location
/// cannot be removed that way) is converted with `job.options`, which drop the metadata.
pub fn remove_location(job: &ImageJob, stop: &dyn Fn() -> bool) -> io::Result<()> {
    if job.options.format == ImageFormat::Jpeg && starts_like_jpeg(job.input)? {
        let bytes = std::fs::read(job.input)?;
        check(stop)?;
        if let Some(clean) = exifclean::jpeg_without_location(&bytes) {
            return write_or_remove(job.output, &clean);
        }
    }
    convert_image(job, stop)
}

/// Writes `jpeg_in` to `jpeg_out` without its location and without re-encoding it: the GPS
/// data of its EXIF is removed in place, its XMP and IPTC are left out, and so is whatever
/// follows the main picture (MPF secondary pictures, a motion photo's video); the picture's
/// bytes stay as they are. Fails (`InvalidData`) on a JPEG it cannot walk or whose EXIF
/// location it cannot remove. On failure nothing is left at `jpeg_out`.
pub fn strip_location(jpeg_in: &Path, jpeg_out: &Path) -> io::Result<()> {
    let bytes = std::fs::read(jpeg_in)?;
    let clean = exifclean::jpeg_without_location(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a JPEG file Gezik can edit"))?;
    write_or_remove(jpeg_out, &clean)
}

fn write_or_remove(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let result = std::fs::write(path, bytes);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn starts_like_jpeg(path: &Path) -> io::Result<bool> {
    let mut head = [0u8; 3];
    let mut file = File::open(path)?;
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..])? {
            0 => return Ok(false),
            n => read += n,
        }
    }
    Ok(head == [0xFF, 0xD8, 0xFF])
}

/// Temporary files next to the output, removed when the conversion ends (whatever way).
#[derive(Default)]
struct Temps(Vec<PathBuf>);

impl Temps {
    /// `<output's name>.<suffix>`: in the output's folder (same volume, and the caller's
    /// temporary names keep `%` out of what ffmpeg sees).
    fn next(&mut self, output: &Path, suffix: &str) -> PathBuf {
        let mut name = output.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(suffix);
        let path = output.with_file_name(name);
        self.0.push(path.clone());
        path
    }
}

impl Drop for Temps {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// A picture as read, with what of its metadata may be carried over.
struct Source {
    img: DynamicImage,
    icc: Option<Vec<u8>>,
    /// The TIFF payload, without `Exif\0\0`.
    exif: Option<Vec<u8>>,
    orientation: Orientation,
    /// Whether it was a JPEG (only a JPEG passes its EXIF on).
    jpeg: bool,
}

fn convert(job: &ImageJob, stop: &dyn Fn() -> bool) -> io::Result<()> {
    let options = job.options;
    if options.format.needs_ffmpeg() && job.ffmpeg.is_none() {
        return Err(needs_ffmpeg());
    }
    let mut temps = Temps::default();
    check(stop)?;
    let source = read(job, &mut temps, stop)?;
    check(stop)?;
    let mut img = source.img;
    if options.rotate_by_exif {
        img.apply_orientation(source.orientation);
    }
    // After the rotation, so that a width is the width as shown.
    if let Some((w, h)) = target_size(img.width(), img.height(), &options.resize, options.never_enlarge) {
        img = resize(&img, w, h)?;
        check(stop)?;
    }
    let icc = source.icc.as_deref();
    let output = job.output;
    match options.format {
        ImageFormat::Jpeg => {
            let exif = source.exif.filter(|_| source.jpeg && !options.strip_metadata).map(|mut exif| {
                if options.rotate_by_exif {
                    let _ = Orientation::remove_from_exif_chunk(&mut exif);
                }
                exifclean::drop_thumbnail(&mut exif);
                exifclean::set_pixel_dims(&mut exif, img.width(), img.height());
                exif
            });
            write_jpeg(&img, output, options.quality, options.background, icc, exif.as_deref())
        }
        ImageFormat::Png => write_png(&img, output, icc_for(icc, is_grey(&img)), CompressionType::Default),
        ImageFormat::WebpLossless => {
            let mut file = BufWriter::new(File::create(output)?);
            let mut encoder = WebPEncoder::new_lossless(&mut file);
            // WebP is always RGB.
            if let Some(icc) = icc_for(icc, false) {
                let _ = encoder.set_icc_profile(icc.to_vec());
            }
            img.write_with_encoder(encoder).map_err(image_error)?;
            file.flush()
        }
        ImageFormat::Bmp => {
            let mut file = BufWriter::new(File::create(output)?);
            img.write_with_encoder(BmpEncoder::new(&mut file)).map_err(image_error)?;
            file.flush()
        }
        ImageFormat::WebpLossy | ImageFormat::Avif => {
            let ffmpeg = job.ffmpeg.ok_or_else(needs_ffmpeg)?;
            let alpha = has_transparency(&img);
            let img =
                if alpha { DynamicImage::ImageRgba8(img.to_rgba8()) } else { DynamicImage::ImageRgb8(img.to_rgb8()) };
            let png = temps.next(output, "enc.png");
            write_png(&img, &png, icc_for(icc, false), CompressionType::Fast)?;
            drop(img);
            check(stop)?;
            let step = match options.format {
                ImageFormat::Avif => FfmpegImage::Avif { quality: options.quality, alpha },
                _ => FfmpegImage::WebpLossy { quality: options.quality },
            };
            run_ffmpeg(ffmpeg, ffmpeg_image_args(step, &png, output), stop)
        }
    }
}

/// Reads the input: by itself, or for HEIC, HEIF and AVIF through ffmpeg into a temporary
/// PNG. ffmpeg turns those upright itself and passes no EXIF on, so nothing turns them again.
fn read(job: &ImageJob, temps: &mut Temps, stop: &dyn Fn() -> bool) -> io::Result<Source> {
    let name = job.input.file_name().unwrap_or_default().to_string_lossy();
    if !needs_ffmpeg_to_read(&name) {
        return decode(job.input);
    }
    let ffmpeg = job.ffmpeg.filter(|ffmpeg| version(ffmpeg).is_some_and(|v| v >= FFMPEG_TO_READ));
    let ffmpeg = ffmpeg.ok_or_else(needs_ffmpeg)?;
    let png = temps.next(job.output, "dec.png");
    run_ffmpeg(ffmpeg, ffmpeg_image_args(FfmpegImage::DecodeToPng, job.input, &png), stop)?;
    check(stop)?;
    let source = decode(&png)?;
    let _ = std::fs::remove_file(&png);
    Ok(Source { exif: None, orientation: Orientation::NoTransforms, jpeg: false, ..source })
}

/// Reads a picture (by its content, not its name) with its ICC profile, EXIF and
/// orientation; GIF, WebP and APNG give their first frame.
fn decode(path: &Path) -> io::Result<Source> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let jpeg = reader.format() == Some(::image::ImageFormat::Jpeg);
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    // `from_decoder` allocates what the header claims without asking any limit: a few bytes
    // claiming 100000×100000 would take 30 GB. So the size is checked here, against image's
    // default allocation limit.
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    let (w, h) = decoder.dimensions();
    limits.check_dimensions(w, h).map_err(|_| too_large())?;
    decoder.set_limits(limits.clone()).map_err(|_| too_large())?;
    limits.reserve(decoder.total_bytes()).map_err(|_| too_large())?;
    // The conversion makes 8-bit RGBA copies (flatten, transparency, ffmpeg's PNG, resize):
    // a grey picture takes four times its own size there.
    if !fits(w, h, 4) {
        return Err(too_large());
    }
    // Before from_decoder, which takes the decoder. Metadata that cannot be read is left out.
    let icc = decoder.icc_profile().ok().flatten();
    let exif = decoder.exif_metadata().ok().flatten();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let img = DynamicImage::from_decoder(decoder).map_err(image_error)?;
    Ok(Source { img, icc, exif, orientation, jpeg })
}

fn image_error(err: ::image::ImageError) -> io::Error {
    use ::image::ImageError as E;
    match err {
        E::IoError(err) => err,
        E::Unsupported(err) => not_supported(err.to_string()),
        E::Limits(_) => too_large(),
        E::Decoding(err) => io::Error::new(io::ErrorKind::InvalidData, format!("the picture is damaged ({err})")),
        err => io::Error::other(err.to_string()),
    }
}

/// Lanczos3 to `w`×`h`, alpha premultiplied. Everything goes through 8-bit RGBA (one pixel
/// type keeps the resizer small), then back to grey or RGB when the picture had no colour or
/// no alpha; a 16-bit picture becomes 8-bit.
fn resize(img: &DynamicImage, w: u32, h: u32) -> io::Result<DynamicImage> {
    // The RGBA copy of the source and the RGBA result.
    if !fits(img.width(), img.height(), 4) || !fits(w, h, 4) {
        return Err(too_large());
    }
    let color = img.color();
    let src = img.to_rgba8();
    let mut dst = RgbaImage::new(w, h);
    let options =
        ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)).use_alpha(color.has_alpha());
    let (Some(from), Some(mut to)) = (src.image_view::<U8x4>(), dst.image_view_mut::<U8x4>()) else {
        return Err(io::Error::other("the resizer does not take this picture"));
    };
    Resizer::new().resize_typed(&from, &mut to, &options).map_err(io::Error::other)?;
    drop((from, to));
    let out = DynamicImage::ImageRgba8(dst);
    Ok(match color {
        ColorType::L8 | ColorType::L16 => DynamicImage::ImageLuma8(out.to_luma8()),
        ColorType::La8 | ColorType::La16 => DynamicImage::ImageLumaA8(out.to_luma_alpha8()),
        color if !color.has_alpha() => DynamicImage::ImageRgb8(out.to_rgb8()),
        _ => out,
    })
}

/// Whether any pixel is not fully opaque.
fn has_transparency(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageRgba8(buffer) => buffer.pixels().any(|p| p.0[3] != u8::MAX),
        DynamicImage::ImageLumaA8(buffer) => buffer.pixels().any(|p| p.0[1] != u8::MAX),
        img if img.color().has_alpha() => img.to_rgba8().pixels().any(|p| p.0[3] != u8::MAX),
        _ => false,
    }
}

/// Composites onto an opaque background; the result is 8-bit RGB (what JPEG takes).
fn flatten(img: &DynamicImage, background: [u8; 3]) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u32::from(p[3]);
        let mix = |c: u8, b: u8| ((u32::from(c) * a + u32::from(b) * (255 - a) + 127) / 255) as u8;
        *o = Rgb([mix(p[0], background[0]), mix(p[1], background[1]), mix(p[2], background[2])]);
    }
    out
}

/// The largest EXIF block a JPEG APP1 segment holds (65533 bytes less `Exif\0\0`); a larger
/// one is left out.
const MAX_JPEG_EXIF: usize = 65527;

fn write_jpeg(
    img: &DynamicImage,
    out: &Path,
    quality: u8,
    background: [u8; 3],
    icc: Option<&[u8]>,
    exif: Option<&[u8]>,
) -> io::Result<()> {
    let (Ok(w), Ok(h)) = (u16::try_from(img.width()), u16::try_from(img.height())) else {
        return Err(not_supported("a JPEG is at most 65535 pixels wide and high"));
    };
    let grey = matches!(img.color(), ColorType::L8 | ColorType::L16);
    let (pixels, color) = if grey {
        (img.to_luma8().into_raw(), jpeg_encoder::ColorType::Luma)
    } else {
        (flatten(img, background).into_raw(), jpeg_encoder::ColorType::Rgb)
    };
    let mut file = BufWriter::new(File::create(out)?);
    let quality = quality.clamp(1, 100);
    let mut encoder = jpeg_encoder::Encoder::new(&mut file, quality);
    // Colour at full resolution from 90 on, else at half in both directions (jpeg-encoder's
    // own choice, made explicit). Optimized Huffman tables only at full resolution: with
    // 4:2:0 they make jpeg-encoder 0.7.1 write one scan per component, which zune-jpeg 0.5
    // (image's decoder, so Gezik's own viewer) decodes into wrong colours. Standard tables
    // cost 1-4% there.
    if quality >= 90 {
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
        encoder.set_optimized_huffman_tables(true);
    } else {
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    }
    if let Some(exif) = exif.filter(|exif| exif.len() <= MAX_JPEG_EXIF) {
        encoder.add_exif_metadata(exif).map_err(jpeg_error)?;
    }
    if let Some(icc) = icc_for(icc, grey) {
        // Too large for a JPEG (over 16 MB): left out.
        let _ = encoder.add_icc_profile(icc);
    }
    encoder.encode(&pixels, w, h, color).map_err(jpeg_error)?;
    file.flush()
}

fn jpeg_error(err: jpeg_encoder::EncodingError) -> io::Error {
    match err {
        jpeg_encoder::EncodingError::IoError(err) => err,
        err => io::Error::other(err.to_string()),
    }
}

fn write_png(img: &DynamicImage, out: &Path, icc: Option<&[u8]>, compression: CompressionType) -> io::Result<()> {
    let mut file = BufWriter::new(File::create(out)?);
    let mut encoder = PngEncoder::new_with_quality(&mut file, compression, PngFilter::Adaptive);
    if let Some(icc) = icc {
        let _ = encoder.set_icc_profile(icc.to_vec());
    }
    img.write_with_encoder(encoder).map_err(image_error)?;
    file.flush()
}
