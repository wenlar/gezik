//! Converting pictures: turning upright by EXIF, resizing, the formats, metadata kept and
//! dropped, location removal without re-encoding, and HEIC/lossy WebP/AVIF through ffmpeg
//! when `GEZIK_TEST_FFMPEG` names an ffmpeg 9 or later.
//!
//! The pictures are made here: a "camera" JPEG with EXIF (orientation 6, GPS, a thumbnail),
//! an ICC profile and XMP holding a location.

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};

use gezik_batch::convert::exifclean;
use gezik_batch::convert::{ImageJob, convert_image, is_needs_ffmpeg, remove_location, strip_location};
use gezik_core::batch::convert::{ImageFormat, ImageOptions, Resize};
use image::metadata::Orientation;
use image::{
    ColorType, DynamicImage, ImageBuffer, ImageDecoder, ImageEncoder, ImageReader, Rgb, RgbImage, Rgba, RgbaImage,
};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-convert-image-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ---- A small TIFF (EXIF) writer --------------------------------------------------------

struct TiffWriter {
    le: bool,
    b: Vec<u8>,
}

/// One IFD entry: tag, type, count, value bytes (in the file's byte order).
type Entry = (u16, u16, u32, Vec<u8>);

impl TiffWriter {
    fn new(le: bool) -> TiffWriter {
        let mut w = TiffWriter { le, b: Vec::new() };
        w.b.extend_from_slice(if le { b"II*\0" } else { b"MM\0*" });
        let first = w.long(8);
        w.b.extend_from_slice(&first);
        w
    }

    fn short(&self, v: u16) -> Vec<u8> {
        (if self.le { v.to_le_bytes() } else { v.to_be_bytes() }).to_vec()
    }

    fn long(&self, v: u32) -> Vec<u8> {
        (if self.le { v.to_le_bytes() } else { v.to_be_bytes() }).to_vec()
    }

    fn rationals(&self, list: &[(u32, u32)]) -> Vec<u8> {
        list.iter().flat_map(|&(n, d)| [self.long(n), self.long(d)].concat()).collect()
    }

    /// Appends an IFD and the values that do not fit in their entries; answers where it
    /// starts, where each entry's value field is and where its next-IFD field is.
    fn ifd(&mut self, entries: &[Entry]) -> (u32, Vec<usize>, usize) {
        let start = self.b.len();
        let mut data_at = start + 2 + entries.len() * 12 + 4;
        let mut data = Vec::new();
        let count = self.short(entries.len() as u16);
        self.b.extend_from_slice(&count);
        let mut values = Vec::new();
        for (tag, typ, n, value) in entries {
            let head = [self.short(*tag), self.short(*typ), self.long(*n)].concat();
            self.b.extend_from_slice(&head);
            values.push(self.b.len());
            if value.len() <= 4 {
                let mut inline = value.clone();
                inline.resize(4, 0);
                self.b.extend_from_slice(&inline);
            } else {
                let at = self.long(data_at as u32);
                self.b.extend_from_slice(&at);
                data.extend_from_slice(value);
                data_at += value.len();
            }
        }
        let next = self.b.len();
        self.b.extend_from_slice(&[0; 4]);
        self.b.extend_from_slice(&data);
        (start as u32, values, next)
    }

    fn patch(&mut self, at: usize, v: u32) {
        let bytes = self.long(v);
        self.b[at..at + 4].copy_from_slice(&bytes);
    }
}

/// An EXIF block as a camera writes it: make, orientation, date, pixel size, a location and
/// a thumbnail.
fn exif_block(le: bool, orientation: u16, w: u32, h: u32, thumb: &[u8]) -> Vec<u8> {
    exif_layout(le, orientation, w, h, thumb).bytes
}

/// An EXIF block and where its parts are.
struct Layout {
    bytes: Vec<u8>,
    /// IFD0's next-IFD field.
    next0: usize,
    /// The value field of IFD0's GPS pointer.
    gps_pointer: usize,
    /// The GPS IFD.
    gps: usize,
}

fn exif_layout(le: bool, orientation: u16, w: u32, h: u32, thumb: &[u8]) -> Layout {
    let mut t = TiffWriter::new(le);
    let ifd0 = [
        (0x010F, 2, 8, b"TESTCAM\0".to_vec()),
        (0x0112, 3, 1, t.short(orientation)),
        (0x8769, 4, 1, t.long(0)),
        (0x8825, 4, 1, t.long(0)),
    ];
    let (_, values0, next0) = t.ifd(&ifd0);
    let exif_ifd =
        [(0x9003, 2, 20, b"2024:07:01 09:30:00\0".to_vec()), (0xA002, 4, 1, t.long(w)), (0xA003, 4, 1, t.long(h))];
    let (exif_at, _, _) = t.ifd(&exif_ifd);
    t.patch(values0[2], exif_at);
    let gps_ifd = [
        (0x0000, 1, 4, vec![2, 3, 0, 0]),
        (0x0001, 2, 2, b"N\0".to_vec()),
        (0x0002, 5, 3, t.rationals(&[(41, 1), (24, 1), (1234, 100)])),
        (0x0003, 2, 2, b"E\0".to_vec()),
        (0x0004, 5, 3, t.rationals(&[(2, 1), (10, 1), (5678, 100)])),
    ];
    let (gps_at, _, _) = t.ifd(&gps_ifd);
    t.patch(values0[3], gps_at);
    let ifd1 = [(0x0103, 3, 1, t.short(6)), (0x0201, 4, 1, t.long(0)), (0x0202, 4, 1, t.long(thumb.len() as u32))];
    let (ifd1_at, values1, _) = t.ifd(&ifd1);
    t.patch(next0, ifd1_at);
    let thumb_at = t.b.len() as u32;
    t.b.extend_from_slice(thumb);
    t.patch(values1[1], thumb_at);
    Layout { bytes: t.b, next0, gps_pointer: values0[3], gps: gps_at as usize }
}

/// An ICC profile as far as Gezik looks at it: bytes 16-19 name the colour space.
const ICC: &[u8] = b"test ICC profileRGB XYZ only its bytes are compared, nothing reads its colours";

fn icc_of(space: &[u8; 4]) -> Vec<u8> {
    [&ICC[..16], space, &ICC[20..]].concat()
}

/// XMP holding the location (as phones and Lightroom write it).
const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
    xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description \
    xmlns:exif=\"http://ns.adobe.com/exif/1.0/\" exif:GPSLatitude=\"41,24.2N\" \
    exif:GPSLongitude=\"2,10.9E\"/></rdf:RDF></x:xmpmeta>";

fn jpeg_bytes(rgb: &[u8], w: u16, h: u16, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality).encode(rgb, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
    out
}

/// A 64×32 JPEG stored sideways (orientation 6): its left half red, its right half blue, so
/// that upright (32×64) the top is red and the bottom blue.
fn camera_jpeg(path: &Path, le: bool) {
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    camera_jpeg_with(path, &exif_block(le, 6, 64, 32, &thumb), &[], &[]);
}

/// The camera JPEG with this EXIF block, more APP segments (number, payload) and bytes
/// after its end.
fn camera_jpeg_with(path: &Path, exif: &[u8], segments: &[(u8, Vec<u8>)], trailer: &[u8]) {
    let (w, h) = (64u32, 32u32);
    let mut rgb = Vec::new();
    for _ in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(if x < w / 2 { &[220, 20, 20] } else { &[20, 20, 220] });
        }
    }
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, 95);
    encoder.add_exif_metadata(exif).unwrap();
    encoder.add_icc_profile(ICC).unwrap();
    encoder.add_app_segment(1, XMP.to_vec()).unwrap();
    for (number, payload) in segments {
        encoder.add_app_segment(*number, payload.clone()).unwrap();
    }
    encoder.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).unwrap();
    out.extend_from_slice(trailer);
    std::fs::write(path, out).unwrap();
}

// ---- Reading results --------------------------------------------------------------------

struct Read {
    img: DynamicImage,
    format: Option<image::ImageFormat>,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    orientation: Orientation,
}

fn read(path: &Path) -> Read {
    let reader = ImageReader::open(path).unwrap().with_guessed_format().unwrap();
    let format = reader.format();
    let mut decoder = reader.into_decoder().unwrap();
    let icc = decoder.icc_profile().unwrap();
    let exif = decoder.exif_metadata().unwrap();
    let orientation = decoder.orientation().unwrap();
    let img = DynamicImage::from_decoder(decoder).unwrap();
    Read { img, format, icc, exif, orientation }
}

fn exif_of(block: &[u8]) -> exif::Exif {
    exif::Reader::new().read_raw(block.to_vec()).unwrap()
}

fn uint(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    exif.get_field(tag, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0))
}

fn has(exif: &exif::Exif, tag: exif::Tag) -> bool {
    exif.fields().any(|f| f.tag == tag)
}

fn has_thumbnail(exif: &exif::Exif) -> bool {
    exif.fields().any(|f| f.ifd_num == exif::In::THUMBNAIL)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn job<'a>(input: &'a Path, output: &'a Path, options: &'a ImageOptions) -> ImageJob<'a> {
    ImageJob { input, output, options, ffmpeg: None }
}

fn never() -> bool {
    false
}

fn options(format: ImageFormat) -> ImageOptions {
    ImageOptions { format, ..ImageOptions::DEFAULT }
}

fn is_red(p: Rgb<u8>) -> bool {
    p[0] > 180 && p[1] < 70 && p[2] < 70
}

fn is_blue(p: Rgb<u8>) -> bool {
    p[2] > 180 && p[0] < 70 && p[1] < 70
}

// ---- Tests ------------------------------------------------------------------------------

#[test]
fn a_camera_jpeg_comes_out_upright_with_its_exif() {
    let d = dir("upright");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    camera_jpeg(&input, true);
    let o = options(ImageFormat::Jpeg);
    convert_image(&job(&input, &output, &o), &never).unwrap();

    let r = read(&output);
    assert_eq!(r.format, Some(image::ImageFormat::Jpeg));
    assert_eq!((r.img.width(), r.img.height()), (32, 64));
    let rgb = r.img.to_rgb8();
    assert!(is_red(*rgb.get_pixel(16, 8)), "{:?}", rgb.get_pixel(16, 8));
    assert!(is_blue(*rgb.get_pixel(16, 56)), "{:?}", rgb.get_pixel(16, 56));
    assert_eq!(r.orientation, Orientation::NoTransforms);
    assert_eq!(r.icc.as_deref(), Some(ICC));
    let exif = exif_of(&r.exif.expect("EXIF kept"));
    assert_eq!(uint(&exif, exif::Tag::Orientation), Some(1));
    assert!(exif.get_field(exif::Tag::Make, exif::In::PRIMARY).is_some());
    assert!(has(&exif, exif::Tag::DateTimeOriginal));
    assert!(has(&exif, exif::Tag::GPSLatitude), "location stays unless asked");
    assert!(!has_thumbnail(&exif), "the thumbnail goes");
    assert_eq!(uint(&exif, exif::Tag::PixelXDimension), Some(32));
    assert_eq!(uint(&exif, exif::Tag::PixelYDimension), Some(64));
    // XMP is never written.
    assert!(!contains(&std::fs::read(&output).unwrap(), b"ns.adobe.com/xap"));
}

#[test]
fn without_rotating_the_orientation_stays() {
    let d = dir("sideways");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    camera_jpeg(&input, false);
    let o = ImageOptions { rotate_by_exif: false, resize: Resize::Percent(50), ..options(ImageFormat::Jpeg) };
    convert_image(&job(&input, &output, &o), &never).unwrap();
    let r = read(&output);
    assert_eq!((r.img.width(), r.img.height()), (32, 16));
    assert_eq!(r.orientation, Orientation::Rotate90);
    let exif = exif_of(&r.exif.unwrap());
    assert_eq!(uint(&exif, exif::Tag::PixelXDimension), Some(32));
    assert_eq!(uint(&exif, exif::Tag::PixelYDimension), Some(16));
}

#[test]
fn strip_metadata_drops_exif_and_keeps_the_colour_profile() {
    let d = dir("strip");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    camera_jpeg(&input, true);
    let o = ImageOptions { strip_metadata: true, ..options(ImageFormat::Jpeg) };
    convert_image(&job(&input, &output, &o), &never).unwrap();
    let r = read(&output);
    assert_eq!((r.img.width(), r.img.height()), (32, 64), "turned upright before the EXIF goes");
    assert!(r.exif.is_none());
    assert_eq!(r.icc.as_deref(), Some(ICC));
    let bytes = std::fs::read(&output).unwrap();
    assert!(!contains(&bytes, b"TESTCAM") && !contains(&bytes, b"GPSLatitude"));
}

/// Everything from the start of scan on: the coded picture.
fn scan_data(jpeg: &[u8]) -> &[u8] {
    let (mut p, mut last) = (2, 0);
    while p + 4 <= jpeg.len() && jpeg[p] == 0xFF && jpeg[p + 1] != 0xDA {
        last = p + 2 + u16::from_be_bytes([jpeg[p + 2], jpeg[p + 3]]) as usize;
        p = last;
    }
    assert!(last > 0);
    &jpeg[p..]
}

#[test]
fn location_removal_leaves_the_picture_as_it_was() {
    for le in [true, false] {
        let d = dir(if le { "location-le" } else { "location-be" });
        let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
        camera_jpeg(&input, le);
        strip_location(&input, &output).unwrap();

        let (before, after) = (std::fs::read(&input).unwrap(), std::fs::read(&output).unwrap());
        assert_eq!(scan_data(&before), scan_data(&after), "not re-encoded");
        assert_eq!(read(&input).img.to_rgb8(), read(&output).img.to_rgb8());
        assert!(!contains(&after, b"ns.adobe.com/xap") && !contains(&after, b"GPSLatitude"), "XMP goes");
        assert_eq!(after.len(), before.len() - (XMP.len() + 4));

        let r = read(&output);
        assert_eq!(r.icc.as_deref(), Some(ICC));
        assert_eq!(r.orientation, Orientation::Rotate90, "the orientation still applies");
        let exif = exif_of(&r.exif.unwrap());
        let gps: Vec<exif::Tag> = exif.fields().filter(|f| f.ifd_num == exif::In::PRIMARY).map(|f| f.tag).collect();
        for tag in
            [exif::Tag::GPSLatitude, exif::Tag::GPSLongitude, exif::Tag::GPSLatitudeRef, exif::Tag::GPSLongitudeRef]
        {
            assert!(!gps.contains(&tag), "{tag} left in {gps:?}");
        }
        assert!(has(&exif, exif::Tag::GPSVersionID));
        assert!(has(&exif, exif::Tag::Make) && has(&exif, exif::Tag::DateTimeOriginal));
        assert!(has_thumbnail(&exif), "the picture did not change, so its thumbnail stays");
        // The location's numbers themselves are gone from the file (41/1 as a rational).
        let t = TiffWriter::new(le);
        assert!(contains(&before, &t.rationals(&[(1234, 100)])));
        assert!(!contains(&after, &t.rationals(&[(1234, 100)])));
    }
}

#[test]
fn the_remove_location_preset_keeps_jpegs_and_re_encodes_the_rest() {
    let d = dir("preset");
    let preset = gezik_core::batch::convert::preset(gezik_core::batch::convert::REMOVE_LOCATION).unwrap();
    let gezik_core::batch::convert::PresetWhat::Image(base) = preset.what else { panic!() };

    let (input, output, direct) = (d.join("camera.jpg"), d.join("out.tmp"), d.join("direct.tmp"));
    camera_jpeg(&input, true);
    let o = ImageOptions { format: preset.format_for("camera.jpg").unwrap(), ..base };
    remove_location(&job(&input, &output, &o), &never).unwrap();
    strip_location(&input, &direct).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), std::fs::read(&direct).unwrap());

    // A PNG with EXIF (and a location in it): written again as a PNG, without EXIF.
    let png = d.join("photo.png");
    {
        let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
        let mut file = std::fs::File::create(&png).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut file);
        encoder.set_exif_metadata(exif_block(true, 1, 4, 4, &thumb)).unwrap();
        encoder.write_image(&[200; 4 * 4 * 3], 4, 4, image::ExtendedColorType::Rgb8).unwrap();
    }
    assert!(read(&png).exif.is_some());
    let out = d.join("png.tmp");
    let o = ImageOptions { format: preset.format_for("photo.png").unwrap(), ..base };
    remove_location(&job(&png, &out, &o), &never).unwrap();
    let r = read(&out);
    assert_eq!(r.format, Some(image::ImageFormat::Png));
    assert!(r.exif.is_none());
    assert_eq!(r.img.to_rgb8(), read(&png).img.to_rgb8());
}

/// A file named `.jpg` too big to read whole is refused before it is read, with a reason.
#[test]
fn removing_location_from_a_huge_jpeg_is_refused_before_reading_it() {
    let d = dir("huge-jpeg");
    let (input, output) = (d.join("huge.jpg"), d.join("out.tmp"));
    {
        let file = std::fs::File::create(&input).unwrap();
        std::io::Write::write_all(&mut &file, &[0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        // Sparse where the file system can: nothing is written past the header.
        file.set_len(300 << 20).unwrap();
    }
    let o = ImageOptions { format: ImageFormat::Jpeg, strip_metadata: true, ..ImageOptions::DEFAULT };
    let err = remove_location(&job(&input, &output, &o), &never).unwrap_err();
    assert!(err.to_string().contains("too large to remove location data"), "{err}");
    assert!(strip_location(&input, &output).unwrap_err().to_string().contains("too large"));
    assert!(!output.exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// 64×32, its left half fully transparent black, its right half opaque green.
fn half_transparent(path: &Path) -> RgbaImage {
    let img = RgbaImage::from_fn(64, 32, |x, _| if x < 32 { Rgba([0, 0, 0, 0]) } else { Rgba([0, 200, 0, 255]) });
    img.save(path).unwrap();
    img
}

#[test]
fn transparency_becomes_the_background_in_a_jpeg() {
    let d = dir("flatten");
    let (input, output) = (d.join("alpha.png"), d.join("out.tmp"));
    half_transparent(&input);
    convert_image(&job(&input, &output, &options(ImageFormat::Jpeg)), &never).unwrap();
    let rgb = read(&output).img.to_rgb8();
    let p = rgb.get_pixel(8, 16);
    assert!(p.0.iter().all(|&c| c > 245), "white, not black: {p:?}");
    let q = rgb.get_pixel(56, 16);
    assert!(q[1] > 170 && q[0] < 60, "{q:?}");

    let o = ImageOptions { background: [0, 0, 255], ..options(ImageFormat::Jpeg) };
    convert_image(&job(&input, &output, &o), &never).unwrap();
    assert!(is_blue(*read(&output).img.to_rgb8().get_pixel(8, 16)));
}

#[test]
fn sizes_follow_the_resize_and_never_enlarge() {
    let d = dir("sizes");
    let (input, output) = (d.join("alpha.png"), d.join("out.tmp"));
    half_transparent(&input);
    let size = |resize: Resize, never_enlarge: bool| {
        let o = ImageOptions { resize, never_enlarge, ..options(ImageFormat::Png) };
        convert_image(&job(&input, &output, &o), &never).unwrap();
        let img = read(&output).img;
        (img.width(), img.height(), img.color())
    };
    assert_eq!(size(Resize::Longest(200), true), (64, 32, ColorType::Rgba8));
    assert_eq!(size(Resize::Longest(200), false), (200, 100, ColorType::Rgba8));
    assert_eq!(size(Resize::Width(32), true), (32, 16, ColorType::Rgba8));
    assert_eq!(size(Resize::Height(8), true), (16, 8, ColorType::Rgba8));
    assert_eq!(size(Resize::Percent(50), true), (32, 16, ColorType::Rgba8));
    // Resized with premultiplied alpha: the transparent black does not darken the green.
    let o = ImageOptions { resize: Resize::Percent(50), ..options(ImageFormat::Png) };
    convert_image(&job(&input, &output, &o), &never).unwrap();
    let img = read(&output).img.to_rgba8();
    for x in 0..32 {
        let p = img.get_pixel(x, 8);
        if p[3] > 20 {
            assert!(p[1] > 170, "x {x}: {p:?}");
        }
    }
}

#[test]
fn opaque_and_grey_pictures_stay_so_when_resized() {
    let d = dir("shapes");
    let (rgb, grey, output) = (d.join("rgb.png"), d.join("grey.png"), d.join("out.tmp"));
    RgbImage::from_fn(40, 20, |x, y| Rgb([x as u8 * 6, y as u8 * 12, 90])).save(&rgb).unwrap();
    image::GrayImage::from_fn(40, 20, |x, _| image::Luma([x as u8 * 6])).save(&grey).unwrap();
    let o = ImageOptions { resize: Resize::Width(20), ..options(ImageFormat::Png) };
    convert_image(&job(&rgb, &output, &o), &never).unwrap();
    assert_eq!(read(&output).img.color(), ColorType::Rgb8);
    convert_image(&job(&grey, &output, &o), &never).unwrap();
    assert_eq!(read(&output).img.color(), ColorType::L8);
    let o = options(ImageFormat::Jpeg);
    convert_image(&job(&grey, &output, &o), &never).unwrap();
    assert_eq!(read(&output).img.color(), ColorType::L8);
}

/// Noise with every alpha value, transparent pixels included.
fn noise_rgba(w: u32, h: u32) -> RgbaImage {
    let mut x = 2_654_435_761u32;
    RgbaImage::from_fn(w, h, |_, _| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        Rgba(x.to_le_bytes())
    })
}

#[test]
fn lossless_webp_and_bmp_give_back_the_same_pixels() {
    let d = dir("lossless");
    let (input, output) = (d.join("noise.png"), d.join("out.tmp"));
    let original = noise_rgba(37, 23);
    original.save(&input).unwrap();
    convert_image(&job(&input, &output, &options(ImageFormat::WebpLossless)), &never).unwrap();
    let r = read(&output);
    assert_eq!(r.format, Some(image::ImageFormat::WebP));
    assert_eq!(r.img.to_rgba8(), original);

    convert_image(&job(&input, &output, &options(ImageFormat::Bmp)), &never).unwrap();
    let r = read(&output);
    assert_eq!(r.format, Some(image::ImageFormat::Bmp));
    assert_eq!(r.img.to_rgba8(), original);
}

#[test]
fn sixteen_bit_pictures() {
    let d = dir("sixteen");
    let (input, output) = (d.join("deep.png"), d.join("out.tmp"));
    let deep: ImageBuffer<Rgb<u16>, Vec<u16>> =
        ImageBuffer::from_fn(30, 20, |x, y| Rgb([x as u16 * 2000, y as u16 * 3000, 40_000]));
    deep.save(&input).unwrap();
    assert_eq!(read(&input).img.color(), ColorType::Rgb16);

    convert_image(&job(&input, &output, &options(ImageFormat::Png)), &never).unwrap();
    let r = read(&output);
    assert_eq!(r.img.color(), ColorType::Rgb16, "not resized: 16 bits kept");
    assert_eq!(r.img.to_rgb16(), deep);

    let o = ImageOptions { resize: Resize::Width(15), ..options(ImageFormat::Png) };
    convert_image(&job(&input, &output, &o), &never).unwrap();
    let r = read(&output);
    assert_eq!((r.img.width(), r.img.height(), r.img.color()), (15, 10, ColorType::Rgb8));

    convert_image(&job(&input, &output, &options(ImageFormat::Jpeg)), &never).unwrap();
    let r = read(&output);
    assert_eq!((r.img.width(), r.img.color()), (30, ColorType::Rgb8));
}

#[test]
fn a_gif_gives_its_first_frame() {
    let d = dir("gif");
    let (input, output) = (d.join("anim.gif"), d.join("out.tmp"));
    {
        let file = std::fs::File::create(&input).unwrap();
        let mut encoder = image::codecs::gif::GifEncoder::new(file);
        let frame = |c: Rgba<u8>| image::Frame::new(RgbaImage::from_pixel(10, 10, c));
        encoder.encode_frames([frame(Rgba([255, 0, 0, 255])), frame(Rgba([0, 0, 255, 255]))]).unwrap();
    }
    convert_image(&job(&input, &output, &options(ImageFormat::Png)), &never).unwrap();
    assert_eq!(read(&output).img.to_rgba8().get_pixel(5, 5), &Rgba([255, 0, 0, 255]));
}

#[test]
fn without_ffmpeg_heic_and_lossy_outputs_say_so() {
    let d = dir("no-ffmpeg");
    let (heic, png, output) = (d.join("photo.heic"), d.join("alpha.png"), d.join("out.tmp"));
    std::fs::write(&heic, b"not read without ffmpeg").unwrap();
    half_transparent(&png);
    for (input, format) in [(&heic, ImageFormat::Jpeg), (&png, ImageFormat::Avif), (&png, ImageFormat::WebpLossy)] {
        let o = options(format);
        let err = convert_image(&job(input, &output, &o), &never).unwrap_err();
        assert!(is_needs_ffmpeg(&err), "{format:?}: {err}");
        assert_eq!(err.to_string(), "ffmpeg needed");
        assert!(!output.exists());
    }
    // A program that is no ffmpeg 9 does not read HEIC either.
    let not_ffmpeg = d.join("missing-ffmpeg.exe");
    let o = options(ImageFormat::Jpeg);
    let err = convert_image(&ImageJob { ffmpeg: Some(&not_ffmpeg), ..job(&heic, &output, &o) }, &never).unwrap_err();
    assert!(is_needs_ffmpeg(&err), "{err}");
}

#[test]
fn not_a_picture_is_not_supported_and_a_damaged_one_says_so() {
    let d = dir("unsupported");
    let output = d.join("out.tmp");
    let unknown = d.join("notes.txt");
    std::fs::write(&unknown, b"plain text, not a picture").unwrap();
    let err = convert_image(&job(&unknown, &output, &options(ImageFormat::Jpeg)), &never).unwrap_err();
    let inner = err.get_ref().and_then(|e| e.downcast_ref::<gezik_batch::convert::ImageError>());
    assert!(matches!(inner, Some(gezik_batch::convert::ImageError::NotSupported(_))), "{err:?}");
    assert!(!output.exists());

    let damaged = d.join("damaged.png");
    std::fs::write(&damaged, b"plain text, not a picture").unwrap();
    let err = convert_image(&job(&damaged, &output, &options(ImageFormat::Jpeg)), &never).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err:?}");
    assert!(!output.exists());
}

/// Gezik's own viewer reads JPEGs with image's decoder (zune-jpeg): what is written must come
/// back in the right colours at every quality.
#[test]
fn jpegs_read_back_in_their_colours_at_every_quality() {
    let d = dir("colours");
    let (input, output) = (d.join("gradient.png"), d.join("out.tmp"));
    let original = RgbImage::from_fn(160, 90, |x, y| Rgb([(x * 255 / 160) as u8, (y * 255 / 90) as u8, 128]));
    original.save(&input).unwrap();
    for quality in [1, 50, 85, 89, 90, 95, 100] {
        let o = ImageOptions { quality, ..options(ImageFormat::Jpeg) };
        convert_image(&job(&input, &output, &o), &never).unwrap();
        let back = read(&output).img.to_rgb8();
        let error: u64 = back.as_raw().iter().zip(original.as_raw()).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
        let mean = error as f64 / original.as_raw().len() as f64;
        assert!(mean < if quality < 50 { 20.0 } else { 4.0 }, "quality {quality}: mean error {mean}");
    }
}

#[test]
fn a_stop_leaves_nothing_behind() {
    let d = dir("stop");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    camera_jpeg(&input, true);
    let o = ImageOptions { resize: Resize::Percent(50), ..options(ImageFormat::Jpeg) };
    for after in 0..4 {
        let asked = Cell::new(0);
        let stop = || {
            asked.set(asked.get() + 1);
            asked.get() > after
        };
        match convert_image(&job(&input, &output, &o), &stop) {
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::Interrupted);
                assert!(!output.exists());
            }
            Ok(()) => assert!(asked.get() <= after, "finished although asked to stop"),
        }
    }
    let left: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(left.len() <= 2, "{left:?}");
}

#[test]
fn damaged_exif_and_jpegs_never_panic() {
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    for le in [true, false] {
        let block = exif_block(le, 6, 64, 32, &thumb);
        for len in 0..block.len() {
            let mut b = block[..len].to_vec();
            exifclean::strip_gps(&mut b);
            exifclean::drop_thumbnail(&mut b);
            exifclean::set_pixel_dims(&mut b, 1, 1);
        }
        // Every byte of the IFDs set to 0xFF in turn (offsets and counts pointing anywhere).
        for at in 0..block.len().min(300) {
            let mut b = block.clone();
            b[at] = 0xFF;
            exifclean::strip_gps(&mut b);
            exifclean::drop_thumbnail(&mut b);
            exifclean::set_pixel_dims(&mut b, 1, 1);
            assert_eq!(b.len(), block.len());
        }
    }
    let d = dir("damaged");
    let input = d.join("camera.jpg");
    camera_jpeg(&input, true);
    let jpeg = std::fs::read(&input).unwrap();
    for len in 0..jpeg.len().min(2000) {
        let _ = exifclean::jpeg_without_location(&jpeg[..len]);
    }
    assert!(exifclean::jpeg_without_location(b"not a jpeg").is_none());
}

// ---- Through ffmpeg (GEZIK_TEST_FFMPEG) --------------------------------------------------

/// The ffmpeg 9 (or later) to test with, from `GEZIK_TEST_FFMPEG`.
fn test_ffmpeg() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("GEZIK_TEST_FFMPEG")?);
    match gezik_batch::convert::ffmpeg::version(&path) {
        Some(version) if version >= (9, 0) => Some(path),
        other => {
            eprintln!("GEZIK_TEST_FFMPEG is not ffmpeg 9 or later ({other:?}); skipped");
            None
        }
    }
}

#[test]
fn heic_is_read_through_ffmpeg() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let d = dir("heic");
    let input = d.join("example.heic");
    // libheif's example picture (1280×854); not kept in the repository.
    let url = "https://raw.githubusercontent.com/strukturag/libheif/master/examples/example.heic";
    if let Err(err) = gezik_platform::http::download(url, &input, 10 << 20, false, &mut |_| {}, &|| false) {
        eprintln!("could not download {url} ({err}); skipped");
        return;
    }
    let output = d.join("out.tmp");
    let o = ImageOptions { resize: Resize::Longest(640), ..options(ImageFormat::Jpeg) };
    convert_image(&ImageJob { ffmpeg: Some(&ffmpeg), ..job(&input, &output, &o) }, &never).unwrap();
    let r = read(&output);
    assert_eq!(r.format, Some(image::ImageFormat::Jpeg));
    assert_eq!((r.img.width(), r.img.height()), (640, 427));
    assert!(r.exif.is_none());
    let left: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left.len(), 2, "the temporary PNG is gone: {left:?}");
}

#[test]
fn lossy_webp_and_avif_are_written_by_ffmpeg() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let d = dir("ffmpeg-out");
    let (input, output) = (d.join("alpha.png"), d.join("out.tmp"));
    half_transparent(&input);

    let o = ImageOptions { quality: 80, ..options(ImageFormat::WebpLossy) };
    convert_image(&ImageJob { ffmpeg: Some(&ffmpeg), ..job(&input, &output, &o) }, &never).unwrap();
    let r = read(&output);
    assert_eq!(r.format, Some(image::ImageFormat::WebP));
    assert_eq!((r.img.width(), r.img.height()), (64, 32));
    let rgba = r.img.to_rgba8();
    assert!(rgba.get_pixel(8, 16)[3] < 10 && rgba.get_pixel(56, 16)[3] > 245, "alpha kept");

    for path in [input.clone(), {
        let opaque = d.join("opaque.png");
        RgbImage::from_pixel(64, 32, Rgb([10, 120, 200])).save(&opaque).unwrap();
        opaque
    }] {
        let o = options(ImageFormat::Avif);
        convert_image(&ImageJob { ffmpeg: Some(&ffmpeg), ..job(&path, &output, &o) }, &never).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[4..12], b"ftypavif", "{}", path.display());
    }
    let left: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left.len(), 3, "only the inputs and the output: {left:?}");
}

#[test]
fn a_failing_ffmpeg_reports_its_errors() {
    let Some(ffmpeg) = test_ffmpeg() else { return };
    let d = dir("ffmpeg-fail");
    let output = d.join("out.png");
    let args = vec!["-hide_banner".into(), "-i".into(), d.join("missing.mov").into_os_string(), output.into()];
    let err = gezik_batch::convert::ffmpeg::run_ffmpeg(&ffmpeg, args, &never).unwrap_err();
    let text = err.to_string();
    assert!(text.starts_with("ffmpeg failed (exit code") && text.contains("missing.mov"), "{text}");
}

// ---- Limits, failing closed, trailers, colour profiles -------------------------------------

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let body = [&kind[..], data].concat();
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

#[test]
fn a_header_claiming_a_huge_picture_is_refused_before_allocating() {
    let d = dir("bomb");
    let output = d.join("out.tmp");
    let too_large = |input: &Path| {
        let err = convert_image(&job(input, &output, &options(ImageFormat::Jpeg)), &never).unwrap_err();
        let inner = err.get_ref().and_then(|e| e.downcast_ref::<gezik_batch::convert::ImageError>());
        assert_eq!(
            inner,
            Some(&gezik_batch::convert::ImageError::NotSupported("too large to convert".into())),
            "{}",
            input.display()
        );
        assert!(!output.exists());
    };
    // A PNG of a few dozen bytes claiming 100000×100000, and one at the 65535 limit (12 GB).
    for side in [100_000u32, 65_535] {
        let png = d.join(format!("bomb{side}.png"));
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        let ihdr = [&side.to_be_bytes()[..], &side.to_be_bytes(), &[8, 2, 0, 0, 0]].concat();
        png_chunk(&mut bytes, b"IHDR", &ihdr);
        png_chunk(&mut bytes, b"IDAT", &[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
        png_chunk(&mut bytes, b"IEND", &[]);
        std::fs::write(&png, bytes).unwrap();
        too_large(&png);
    }
    // A JPEG whose frame header says 65535×65535.
    let jpeg = d.join("bomb.jpg");
    camera_jpeg(&jpeg, true);
    let mut bytes = std::fs::read(&jpeg).unwrap();
    let sof = bytes.windows(2).rposition(|w| w == [0xFF, 0xC0]).unwrap();
    bytes[sof + 5..sof + 9].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    std::fs::write(&jpeg, bytes).unwrap();
    too_large(&jpeg);
}

#[test]
fn a_resize_too_large_to_hold_is_refused() {
    let d = dir("big-resize");
    let (input, output) = (d.join("plain.png"), d.join("out.tmp"));
    RgbImage::from_pixel(1200, 1200, Rgb([10, 20, 30])).save(&input).unwrap();
    // 12000×12000 RGBA: 576 MB, over the 512 MiB a buffer may take.
    let o = ImageOptions { resize: Resize::Percent(1000), never_enlarge: false, ..options(ImageFormat::Png) };
    let err = convert_image(&job(&input, &output, &o), &never).unwrap_err();
    assert_eq!(err.to_string(), "too large to convert");
    assert!(!output.exists());
}

#[test]
fn location_removal_fails_closed_on_gps_it_cannot_remove() {
    let d = dir("gps-damaged");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    let mut layout = exif_layout(true, 6, 64, 32, &thumb);
    // The GPS IFD claims more entries than the block holds.
    layout.bytes[layout.gps..layout.gps + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());
    let mut copy = layout.bytes.clone();
    assert!(!exifclean::strip_gps(&mut copy));
    assert_eq!(copy, layout.bytes, "nothing changed by a failed strip");
    assert_eq!(exifclean::has_gps(&layout.bytes), Some(true));
    camera_jpeg_with(&input, &layout.bytes, &[], &[]);

    let err = strip_location(&input, &output).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(!output.exists());

    // The preset re-encodes it instead, without EXIF.
    let preset = gezik_core::batch::convert::preset(gezik_core::batch::convert::REMOVE_LOCATION).unwrap();
    let gezik_core::batch::convert::PresetWhat::Image(base) = preset.what else { panic!() };
    let o = ImageOptions { format: ImageFormat::Jpeg, ..base };
    remove_location(&job(&input, &output, &o), &never).unwrap();
    let r = read(&output);
    assert!(r.exif.is_none());
    assert_eq!((r.img.width(), r.img.height()), (32, 64));
    assert!(!contains(&std::fs::read(&output).unwrap(), &TiffWriter::new(true).rationals(&[(1234, 100)])));
}

#[test]
fn location_removal_leaves_out_iptc_and_what_follows_the_picture() {
    let d = dir("trailer");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    let exif = exif_block(true, 6, 64, 32, &thumb);
    let iptc = b"Photoshop 3.0\x008BIM\x04\x04\x00\x00\x00\x00\x00\x10\x1c\x02\x5a\x00\x09Barcelona".to_vec();
    let mpf = b"MPF\x00II*\x00\x08\x00\x00\x00".to_vec();
    // A secondary picture with its own location after the main one, as MPF files and motion
    // photos append them.
    let mut second = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut second, 80);
    encoder.add_exif_metadata(&exif).unwrap();
    encoder.encode(&[50; 8 * 8 * 3], 8, 8, jpeg_encoder::ColorType::Rgb).unwrap();
    camera_jpeg_with(&input, &exif, &[(13, iptc), (2, mpf)], &second);

    strip_location(&input, &output).unwrap();
    let (before, after) = (std::fs::read(&input).unwrap(), std::fs::read(&output).unwrap());
    assert!(contains(&before, b"Barcelona") && !contains(&after, b"Barcelona"), "IPTC goes");
    assert!(contains(&before, b"MPF\x00") && !contains(&after, b"MPF\x00"));
    let main = &before[..before.len() - second.len()];
    assert_eq!(scan_data(&after), scan_data(main), "the main picture unchanged, nothing after it");
    assert_eq!(read(&input).img.to_rgb8(), read(&output).img.to_rgb8());
    let lat = TiffWriter::new(true).rationals(&[(1234, 100)]);
    assert!(contains(&before[main.len()..], &lat));
    assert!(!contains(&after, &lat), "no location left anywhere");
}

#[test]
fn a_colour_profile_goes_only_with_matching_colours() {
    let d = dir("icc-space");
    let output = d.join("out.tmp");
    let png_with = |name: &str, img: &DynamicImage, space: &[u8; 4]| {
        let path = d.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut file);
        encoder.set_icc_profile(icc_of(space)).unwrap();
        img.write_with_encoder(encoder).unwrap();
        path
    };
    let grey = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(8, 8, image::Luma([100])));
    let colour = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([10, 100, 200])));
    let cases = [
        (png_with("grey-gray.png", &grey, b"GRAY"), ImageFormat::Jpeg, Some(icc_of(b"GRAY"))),
        (png_with("grey-gray2.png", &grey, b"GRAY"), ImageFormat::Png, Some(icc_of(b"GRAY"))),
        // WebP is always colour.
        (png_with("grey-gray3.png", &grey, b"GRAY"), ImageFormat::WebpLossless, None),
        (png_with("grey-rgb.png", &grey, b"RGB "), ImageFormat::Jpeg, None),
        (png_with("rgb-gray.png", &colour, b"GRAY"), ImageFormat::Jpeg, None),
        (png_with("rgb-cmyk.png", &colour, b"CMYK"), ImageFormat::Png, None),
        (png_with("rgb-rgb.png", &colour, b"RGB "), ImageFormat::WebpLossless, Some(icc_of(b"RGB "))),
    ];
    for (input, format, icc) in cases {
        convert_image(&job(&input, &output, &options(format)), &never).unwrap();
        assert_eq!(read(&output).icc, icc, "{} as {format:?}", input.display());
    }
}

#[test]
fn self_pointing_ifds_are_left_alone() {
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    let layout = exif_layout(true, 6, 64, 32, &thumb);
    let t = TiffWriter::new(true);
    // IFD1 is IFD0 itself.
    let mut b = layout.bytes.clone();
    b[layout.next0..layout.next0 + 4].copy_from_slice(&t.long(8));
    let before = b.clone();
    assert!(!exifclean::drop_thumbnail(&mut b));
    assert_eq!(b, before);
    // The GPS IFD is IFD0 itself.
    let mut b = layout.bytes.clone();
    b[layout.gps_pointer..layout.gps_pointer + 4].copy_from_slice(&t.long(8));
    let before = b.clone();
    assert!(!exifclean::strip_gps(&mut b));
    assert_eq!(b, before);
}

#[test]
fn a_grey_picture_too_large_as_rgba_is_refused_before_decoding() {
    let d = dir("grey-bomb");
    let (png, output) = (d.join("grey.png"), d.join("out.tmp"));
    // 16384² grey: 256 MiB decoded (within image's limit), 1 GiB as RGBA. The image data is
    // a tiny stream that does not hold the picture: refused before it is read.
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let side = 16_384u32.to_be_bytes();
    png_chunk(&mut bytes, b"IHDR", &[&side[..], &side, &[8, 0, 0, 0, 0]].concat());
    png_chunk(&mut bytes, b"IDAT", &[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    png_chunk(&mut bytes, b"IEND", &[]);
    std::fs::write(&png, bytes).unwrap();
    for format in [ImageFormat::Jpeg, ImageFormat::Png] {
        let err = convert_image(&job(&png, &output, &options(format)), &never).unwrap_err();
        assert_eq!(err.to_string(), "too large to convert", "{format:?}");
        assert!(!output.exists());
    }
}

/// The camera JPEG with `insert` put just before its EOI and `after` after it; with `cut`,
/// that many bytes of image data and the EOI are left out (a truncated picture).
fn camera_jpeg_spliced(path: &Path, insert: &[u8], cut: usize, after: &[u8]) {
    camera_jpeg(path, true);
    let bytes = std::fs::read(path).unwrap();
    let end = bytes.len() - 2 - cut;
    let eoi: &[u8] = if cut == 0 { &[0xFF, 0xD9] } else { &[] };
    std::fs::write(path, [&bytes[..end], insert, eoi, after].concat()).unwrap();
}

#[test]
fn location_removal_fails_closed_on_what_it_does_not_walk() {
    let d = dir("walk");
    let output = d.join("out.tmp");
    let lat = TiffWriter::new(true).rationals(&[(1234, 100)]);
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    let exif = exif_block(true, 1, 8, 8, &thumb);
    let mut second = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut second, 80);
    encoder.add_exif_metadata(&exif).unwrap();
    encoder.encode(&[50; 8 * 8 * 3], 8, 8, jpeg_encoder::ColorType::Rgb).unwrap();

    let app = |marker: u8, payload: &[u8]| {
        [&[0xFF, marker][..], &((payload.len() + 2) as u16).to_be_bytes(), payload].concat()
    };
    let exif_segment = app(0xE1, &[b"Exif\x00\x00", &exif[..]].concat());
    let cases: Vec<(&str, Vec<u8>, usize, Vec<u8>)> = vec![
        // APPn segments after the first scan (between progressive scans).
        ("app1 after the scan", exif_segment.clone(), 0, vec![]),
        ("app13 after the scan", app(0xED, b"Photoshop 3.0\x00Barcelona"), 0, vec![]),
        ("mpf after the scan", app(0xE2, b"MPF\x00II*\x00\x08\x00\x00\x00"), 0, vec![]),
        // A truncated main picture (no EOI) followed by another picture.
        ("picture after a truncated one", vec![], 6, second.clone()),
        ("TEM in the image data", vec![0xFF, 0x01], 0, vec![]),
    ];
    for (name, insert, cut, after) in cases {
        let input = d.join("camera.jpg");
        camera_jpeg_spliced(&input, &insert, cut, &after);
        let err = strip_location(&input, &output).expect_err(name);
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{name}");
        assert!(!output.exists(), "{name}");
        // The preset re-encodes it instead (when the picture can be read at all).
        let preset = gezik_core::batch::convert::preset(gezik_core::batch::convert::REMOVE_LOCATION).unwrap();
        let gezik_core::batch::convert::PresetWhat::Image(base) = preset.what else { panic!() };
        let o = ImageOptions { format: ImageFormat::Jpeg, ..base };
        if remove_location(&job(&input, &output, &o), &never).is_ok() {
            assert!(!contains(&std::fs::read(&output).unwrap(), &lat), "{name}");
            std::fs::remove_file(&output).unwrap();
        }
    }
}

#[test]
fn two_gps_pointers_fail_closed() {
    let d = dir("two-gps");
    let (input, output) = (d.join("camera.jpg"), d.join("out.tmp"));
    let thumb = jpeg_bytes(&[128; 8 * 8 * 3], 8, 8, 50);
    let mut block = exif_block(true, 6, 64, 32, &thumb);
    // IFD0's first entry (Make, at 8 + 2) becomes a second GPS pointer.
    block[10..12].copy_from_slice(&0x8825u16.to_le_bytes());
    assert_eq!(exifclean::has_gps(&block), None);
    let mut copy = block.clone();
    assert!(!exifclean::strip_gps(&mut copy));
    assert_eq!(copy, block);
    camera_jpeg_with(&input, &block, &[], &[]);
    assert_eq!(strip_location(&input, &output).unwrap_err().kind(), io::ErrorKind::InvalidData);
    assert!(!output.exists());
}
