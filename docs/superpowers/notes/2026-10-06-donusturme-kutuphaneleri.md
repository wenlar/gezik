# Conversion (5c) API digest for Gezik

Date: 2026-10-06. Toolchain: rustc/cargo 1.99.0, x86_64-pc-windows-msvc, edition 2024.
Probe crate: `scratchpad/convprobe` (functional probes) and `scratchpad/convprobe/sizeprobe` (exe size stages, Gezik's release profile).
ffmpeg tests: `scratchpad/ff` (gyan 9.0.2 / 8.0.1 / 7.1.1 essentials, the local `D:\ffmpeg` 2025-05-19 git full build, HEIC/AVIF samples in `ff/heic`, test videos in `ff/v`).

Every Rust snippet below is copied from code that compiles and runs in the probe. Every command was run, and its output is quoted.

How to rerun:
```
cd convprobe
cargo build --release --bins
target/release/imgprobe      # image decode/orient/resize/flatten/encode/EXIF, output in imgprobe.out.txt
target/release/textprobe     # detection, conversion, EOL, streaming, output in textprobe.out.txt
target/release/resizebench   # fast_image_resize vs image::imageops (needs ../ff/heic/out/inseven_IMG_3857.png)
target/release/jpegcmp       # image JpegEncoder vs jpeg-encoder (needs out/big_1920.jpg from imgprobe)
cd sizeprobe && cargo build --release --no-default-features --features "<stage>"   # see section 7
```
Files: `src/img.rs` (decode, `target_size`, `resize`, `flatten`, `encode`), `src/exifclean.rs` (in-place GPS/thumbnail/pixel-dimension edits, JPEG APP1 finder), `src/text.rs` (detect + streaming convert), `src/bin/*.rs`. Samples: `samples/` (images; `DSCN0010.jpg` and `landscape_6.jpg` are from github.com/ianare/exif-samples) and `tsamples/` (text, made with Python codecs).

---

## 0. Cargo lines (checked)

```toml
# gezik-batch (or wherever conversion lives). Same image line as gezik-platform plus tiff and ico.
image = { version = "0.25.10", default-features = false, features = ["png", "jpeg", "gif", "webp", "bmp", "tiff", "ico"] }
fast_image_resize = { version = "6.1.0", features = ["image"] }   # no "rayon" (see 2)
encoding_rs = "0.8.42"
chardetng = "1.0.0"
# kamadak-exif 0.6 is already a gezik-batch dependency (it reads EXIF; only used to check results here)
```
- MSRVs: image 0.25.10 = 1.88, fast_image_resize 6.1.0 = 1.87, encoding_rs 0.8.42 = 1.88, chardetng 1.0.0 = 1.40.
- Licenses: image MIT/Apache-2.0; fast_image_resize MIT/Apache-2.0; encoding_rs (Apache-2.0 OR MIT) AND BSD-3-Clause (the BSD part is the WHATWG index data, so THIRD-PARTY.md needs it); chardetng Apache-2.0 OR MIT.
- **image features cover both directions.** In image 0.25 there are no separate encoder features: `jpeg`, `png`, `webp`, `bmp` already include the encoders. Gezik already has them in gezik-platform. Only `tiff` and `ico` are new. `ico` = `["bmp", "png"]`, so it adds almost nothing. Encoders are linked only when called (LTO), so their cost shows up only in code that calls them (sizes in 7).
- `fast_image_resize/image` depends on `image ^0.25.6` with `default-features = false`, so Cargo unifies it with Gezik's image. The `rayon` feature also turns on `image/rayon` (harmless; Gezik already locks rayon 1.12).
- Not needed: `little_exif` (pulls brotli 8, quick-xml 0.37, crc 3, miniz_oxide and paste, and rewrites the whole file) and `img-parts`. The in-place EXIF editor in section 3 needs no crate.
- Optional: `jpeg-encoder = "0.7.1"` ((MIT OR Apache-2.0) AND IJG, MSRV 1.87). It gives 18% smaller JPEGs and is 2x faster than image's encoder (see 1.4).

---

## 1. image 0.25.10

### 1.1 Decode with metadata (verified on every input type)
```rust
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};

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
```
The probe prints this (dimensions after `apply_orientation`):
```
all.jpg            fmt=Jpeg color=Rgb8 640x480 orient=Rotate90 -> 480x640 icc=Some(548) exif=Some(11250)
landscape_6.jpg    fmt=Jpeg color=Rgb8 450x600 orient=Rotate90 -> 600x450 icc=Some(1960) exif=Some(120)
DSCN0010.jpg       fmt=Jpeg color=Rgb8 640x480 orient=NoTransforms -> 640x480 icc=None exif=Some(11250)
cmyk.jpg           fmt=Jpeg color=Rgb8 640x480 orient=NoTransforms -> 640x480 icc=None exif=Some(11250)
rgb16.png          fmt=Png color=Rgb16 400x300 ...
rgba16.png         fmt=Png color=Rgba16 256x256 ...
gray.png           fmt=Png color=L8 300x200 ...
rgb16.tif          fmt=Tiff color=Rgb16 400x300 ...
rgb8_orient6.tif   fmt=Tiff color=Rgb8 400x300 orient=Rotate90 -> 300x400 ...
multi.ico          fmt=Ico color=Rgba8 48x48 ...
anim.gif           fmt=Gif color=Rgba8 120x80 ...        gif first frame pixel = Rgba([255, 0, 0, 255]) (red expected)
lossy.webp         fmt=WebP color=Rgb8 640x480 ... exif=Some(11250)
lossless.webp      fmt=WebP color=Rgba8 256x256 ...
in.bmp             fmt=Bmp color=Rgb8 640x480 ...
```
- `exif_metadata()` returns the TIFF payload **without** `Exif\0\0`. JPEG, PNG (eXIf), WebP and TIFF decoders implement `orientation()`. The default trait method derives it from `exif_metadata()`, and TIFF uses its own tag.
- CMYK JPEG decodes to Rgb8 (zune-jpeg converts it).
- **ICO picks the entry with the highest bits-per-pixel first, then the largest size** (`best_entry` scores `(bpp, w*h)`). An ICO whose 256 px entry has a directory bpp of 8 (ImageMagick wrote one) decodes as the 48 px 32-bit entry. That is acceptable, but it is not always "largest".
- The GIF decoder through `from_decoder` gives frame 1 as Rgba8.
- **Limits do not apply on the `into_decoder()` path.** `ImageReader`'s default limits (`max_alloc` 512 MiB) are enforced by `ImageReader::decode()`, but `into_decoder()` + `DynamicImage::from_decoder` allocates whatever the header claims (a 60-byte PNG claiming 100000x100000 asks for 30 GB and aborts the process). After `into_decoder()`, check yourself: `let mut limits = Limits::default(); limits.max_image_width = Some(65535); limits.max_image_height = Some(65535); limits.check_dimensions(w, h)?; decoder.set_limits(limits.clone())?; limits.reserve(decoder.total_bytes())?;` (Gezik: `gezik-batch::convert::image::decode`). Resize targets need the same budget (Gezik: every RGBA buffer ≤ 512 MiB).

### 1.2 Orientation
```rust
let mut im = d.img;
im.apply_orientation(d.orientation);              // DynamicImage::apply_orientation(&mut self, Orientation)
let was = Orientation::remove_from_exif_chunk(&mut exif); // writes 1 into the EXIF tag in place, returns the old value
```
`Orientation::from_exif(u8) -> Option<Orientation>` and `Orientation::from_exif_chunk(&[u8])` also exist. Gotcha: `from_exif_chunk` and `remove_from_exif_chunk` look only at IFD0 entries of type SHORT with count 1. That is fine for real cameras.

### 1.3 Resize size rule ("never enlarge")
```rust
pub enum Fit { LongestSide(u32), Width(u32), Height(u32), Percent(u32) }

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
```
Output: `target_size: Some((1920, 1440)) None Some((1920, 1440)) Some((400, 300)) Some((264, 198))` for (4032x3024, Longest 1920, never), (800x600, Longest 1920, never), (same, enlarge allowed), (800x600, Height 300), (800x600, 33 %).
Apply the orientation **before** computing the size, so that "width" means the displayed width.

### 1.4 Encoders
```rust
use image::codecs::bmp::BmpEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::{ImageEncoder};

let mut w = BufWriter::new(File::create(out)?);
// JPEG
let mut enc = JpegEncoder::new_with_quality(&mut w, quality); // 1..=100, clamped internally
if let Some(icc) = icc { enc.set_icc_profile(icc).unwrap(); }   // Ok for JPEG: APP2 ICC_PROFILE chunks
if let Some(exif) = exif { enc.set_exif_metadata(exif).unwrap(); } // Ok for JPEG: APP1, encoder adds "Exif\0\0"
img.write_with_encoder(enc)?;
// PNG (keeps 16-bit and alpha)
img.write_with_encoder(PngEncoder::new_with_quality(&mut w, CompressionType::Default, PngFilter::Adaptive))?;
// WebP lossless (16-bit -> 8-bit automatically)
img.write_with_encoder(WebPEncoder::new_lossless(&mut w))?;
// BMP (16-bit -> 8-bit, alpha kept as 32 bpp)
img.write_with_encoder(BmpEncoder::new(&mut w))?;
w.flush()?;
```
- `set_icc_profile` and `set_exif_metadata` are `ImageEncoder` trait methods (`use image::ImageEncoder`). JPEG, PNG and WebP accept both. TIFF accepts ICC only. Other encoders return `Err(UnsupportedError)`.
- `DynamicImage::write_with_encoder` calls the encoder's `make_compatible_img`. For JPEG, `Rgba8 | Rgb16 | Rgba16 | Rgb32F | Rgba32F -> to_rgb8()` and `La8 | L16 | La16 -> to_luma8()`. **Alpha is dropped without compositing, so call `flatten` first.** WebP and BMP use `dynimage_conversion_8bit`.
- `PngEncoder::new(w)` defaults to `CompressionType::Fast` (`#[default] Fast`). Use `new_with_quality(.., CompressionType::Default, ..)` for files that are kept.
- **JPEG EXIF length gotcha:** `write_segment` computes `data.len() as u16 + 2` without a check. EXIF longer than 65527 bytes wraps around in release builds, which produces a corrupt JPEG, and overflows (panics) in debug builds. EXIF that came from a JPEG APP1 always fits. Guard anyway with `if exif.len() <= 65527`.
- image's JPEG encoder is baseline only, with fixed 4:2:2 subsampling and standard Huffman tables. Observed results:
```
q50: 78554 bytes   q85: 160957 bytes   q100: 436102 bytes   (DSCN0010 640x480)
1440x1920 photo:
q75: image JpegEncoder 302830 B in 37.0ms | jpeg-encoder optimized 248307 B in 19.2ms
q85: image JpegEncoder 352493 B in 37.6ms | jpeg-encoder optimized 289956 B in 20.6ms
q95: image JpegEncoder 536395 B in 38.9ms | jpeg-encoder optimized 463150 B in 32.5ms
magick (libjpeg-turbo) q85: 334801
```
  The optional `jpeg-encoder` 0.7.1 API (in `src/bin/jpegcmp.rs`): `let mut e = jpeg_encoder::Encoder::new(&mut buf, q); e.set_optimized_huffman_tables(true); e.add_icc_profile(&icc)?; e.add_exif_metadata(&tiff)?; e.encode(rgb.as_raw(), w as u16, h as u16, jpeg_encoder::ColorType::Rgb)?;`. Its default sampling is 4:2:0 below q90 and 4:4:4 at q90 and above. `add_app_segment` rejects more than 65533 bytes with an error, so it has no overflow. The size cost is +68 KB, against +26 KB for image's JpegEncoder. Dimensions are u16 (max 65535).
- Lossless WebP (image-webp 0.2.4): 640x480 photo → 622,828 B (PNG Default 738,451 B, BMP 921,654 B). A 1440x1920 encode takes 17.9 ms. The round trip is bit-exact (`webp lossless: color=Rgba8 identical=true`).

### 1.5 16-bit and alpha
```rust
/// Composite onto an opaque background colour; result is Rgb8 (what JPEG needs).
pub fn flatten(img: &DynamicImage, bg: [u8; 3]) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8(); // 16-bit -> 8-bit happens here too
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
```
Observed:
```
rgba corner Rgba([0, 0, 0, 0]) -> flattened Rgb([255, 255, 255])
rgb16.png -> jpg Rgb8
rgb16.tif -> jpg Rgb8
rgba16 resized png -> Rgba16
webp from 16-bit: Rgba8
bmp: color=Rgba8 identical=true
```

### 1.6 Full JPEG→JPEG flow (verified, `imgprobe` section 2)
```rust
let d = img::decode(&s("all.jpg")).unwrap();       // 640x480, orientation 6, Display P3 ICC, Nikon EXIF with GPS + thumbnail
let mut im = d.img;
im.apply_orientation(d.orientation);
let (nw, nh) = img::target_size(im.width(), im.height(), Fit::LongestSide(320), true).unwrap();
let im = img::resize(&im, nw, nh, &mut resizer);
let mut exif = d.exif.clone().unwrap();
let was = Orientation::remove_from_exif_chunk(&mut exif); // sets tag to 1 in place
let gps = exifclean::strip_gps(&mut exif);
let thumb = exifclean::drop_thumbnail(&mut exif);
let dims = exifclean::set_pixel_dims(&mut exif, nw, nh);
img::encode(
    &DynamicImage::ImageRgb8(img::flatten(&im, [255, 255, 255])),
    &out,
    OutFormat::Jpeg { quality: 85, icc: d.icc.clone(), exif: Some(exif) },
).unwrap();
```
Output (checked with kamadak-exif in the probe and with `magick identify -verbose`):
```
src exif: orient=Some("row 0 at right and column 0 at top") make=Some("\"NIKON\"") dto=Some("2008-10-22 16:28:39") pxdim=Some("640")xSome("480") gps_fields=["GPSLatitudeRef", "GPSLatitude", "GPSLongitudeRef", "GPSLongitude", "GPSAltitudeRef", "GPSTimeStamp", "GPSSatellites", "GPSImgDirectionRef", "GPSMapDatum", "GPSDateStamp"] thumb=true
removed orientation=Some(Rotate90) gps=true thumb=true dims=true
out: 240x320 orient=NoTransforms icc_same=true exif: orient=Some("row 0 at top and column 0 at left") make=Some("\"NIKON\"") dto=Some("2008-10-22 16:28:39") pxdim=Some("240")xSome("320") gps_fields=["GPSVersionID"] thumb=false
lat rational present: src=true out=false          # raw bytes of 43/1 searched in the whole output file
magick: Orientation: TopLeft, exif:GPSVersionID only, exif:Make NIKON, exif:PixelXDimension 240, icc:description Display P3
```
- XMP is never written by image's JpegEncoder, so XMP (which can also hold GPS) is dropped automatically on re-encode.
- For "Remove metadata", pass `icc: None, exif: None`. For non-JPEG outputs the spec says EXIF is not carried. Note that PNG/WebP encoders could carry it (`set_exif_metadata` returns Ok).

### 1.7 Speed (12 MP, 3024x4032 RGB8, release)
```
12MP: png decode 67.4ms, resize rgb8 3024x4032->1440x1920 7.9ms, rgba8 19.8ms, jpeg enc 1920 37.6ms, jpeg enc 12MP 163.2ms, webp-ll enc 1920 17.9ms
```

---

## 2. fast_image_resize 6.1.0

- `ResizeOptions::default()` uses `ResizeAlg::Convolution(FilterType::Lanczos3)`, no cropping, and `mul_div_alpha: true`. **Alpha is premultiplied and divided back automatically.** Pass `.use_alpha(false)` for opaque RGBA to save the work.
- With the `image` feature, `DynamicImage` and every `ImageBuffer` type (`RgbImage`, `RgbaImage`, the 16-bit ones, `Rgb32FImage`, `Rgba32FImage`) implement `IntoImageView` and `IntoImageViewMut`.
- The resizer does no sRGB→linear conversion. `create_srgb_mapper()` exists if wanted.

Generic version (works for every DynamicImage variant, but costs +1.94 MB, see 7):
```rust
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
let mut dst = match img {
    DynamicImage::ImageLuma8(_) => DynamicImage::new_luma8(nw, nh),
    DynamicImage::ImageRgb8(_) => DynamicImage::new_rgb8(nw, nh),
    DynamicImage::ImageRgba8(_) => DynamicImage::new_rgba8(nw, nh),
    DynamicImage::ImageRgb16(_) => DynamicImage::new_rgb16(nw, nh),
    DynamicImage::ImageRgba16(_) => DynamicImage::new_rgba16(nw, nh),
    // ... one arm per variant (src/img.rs has all ten)
    _ => DynamicImage::new_rgba32f(nw, nh),
};
let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
resizer.resize(img, &mut dst, &opts).expect("same pixel type");
```
**Recommended version (size-lean): monomorphise one pixel type with `resize_typed`.** `Resizer::resize()` matches on all 13 `PixelType`s, so every type's SIMD kernels get linked. This is from `sizeprobe/src/main.rs` (`firx4`) and `src/bin/resizebench.rs`:
```rust
use fast_image_resize::{pixels, IntoImageView, IntoImageViewMut, Resizer};
let rgba = img.to_rgba8();                       // everything goes through RGBA8
let mut dst = image::RgbaImage::new(nw, nh);
Resizer::new()
    .resize_typed::<pixels::U8x4>(&rgba.image_view().unwrap(), &mut dst.image_view_mut().unwrap(), None)
    .unwrap();                                    // None = Lanczos3 + premultiplied alpha
let out = DynamicImage::ImageRgba8(dst);
```
(Add the `U8x3` and/or `U16x4` arms the same way if they are worth their size: `pixels::U8x3` with `RgbImage`, `pixels::U16x4` with `ImageBuffer<Rgba<u16>, Vec<u16>>`.) A resized 16-bit PNG then becomes 8-bit. A 16-bit PNG that is **not** resized keeps 16 bits.

Speed (3024x4032 → 1440x1920, AVX2, 32 hardware threads):
```
cpu extensions: Avx2, rayon threads 32
fir U8x4 Lanczos3 3024x4032->1440x1920, rayon pool: 8.4ms
fir U8x4 Lanczos3, 1 thread: 18.9ms
image::imageops::resize Lanczos3 (1 thread): 135.1ms
```
**Leave the `rayon` feature off.** Gezik already runs one image per core (`Work::Cpu`), and the feature gains about 10 ms per photo at a cost of +310 KB (U8x4 only) to +1.28 MB (generic). The cheapest fallback is `img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)` at +40 KB, but it is 7x slower.

---

## 3. Removing GPS while keeping EXIF

Decision: **edit the TIFF payload in place, never moving bytes** (`src/exifclean.rs`, about 170 lines, no dependencies). This works because every offset stays valid, the result has the same length, and it plugs straight into `JpegEncoder::set_exif_metadata` or into an untouched JPEG file.
- `strip_gps(&mut [u8]) -> bool`: finds IFD0 tag 0x8825. It zeroes every GPS entry and its out-of-line data (rationals, strings), then rewrites the GPS IFD as one entry, `GPSVersionID = 2.2.0.0`, so strict readers still see a valid IFD.
- `drop_thumbnail(&mut [u8]) -> bool`: zeroes the IFD1 entries and the JPEG thumbnail bytes (0x0201/0x0202), and sets IFD0's next pointer to 0. A thumbnail goes stale after rotate or resize.
- `set_pixel_dims(&mut [u8], w, h) -> bool`: updates ExifIFD 0xA002/0xA003 (SHORT or LONG, inline).
- `jpeg_exif_range(&[u8]) -> Option<Range<usize>>`: walks JPEG segments to the APP1 `Exif\0\0` payload, stopping at SOS/EOI.

The core of `strip_gps`:
```rust
pub fn strip_gps(b: &mut [u8]) -> bool {
    (|| {
        let t = Tiff::new(b)?;                              // II*\0 or MM\0*
        let (ents, _) = entries(t, b, ifd0(t, b)?)?;
        let &(e, _, _, _) = ents.iter().find(|x| x.1 == 0x8825)?;
        let gps = t.u32(b, e + 8)? as usize;
        let (gents, next) = entries(t, b, gps)?;
        for (ge, _, typ, count) in gents {
            wipe_entry(t, b, ge, typ, count);              // zero out-of-line data (size > 4) + the 12-byte entry
        }
        b[next..next + 4].fill(0);
        t.put_u16(b, gps, 1);                              // one entry: GPSVersionID BYTE[4] = 2,2,0,0
        let e0 = gps + 2;
        t.put_u16(b, e0, 0x0000);
        t.put_u16(b, e0 + 2, 1);
        t.put_u32(b, e0 + 4, 4);
        b[e0 + 8..e0 + 12].copy_from_slice(&[2, 2, 0, 0]);
        if b.get(e0 + 12..e0 + 16).is_some() {
            b[e0 + 12..e0 + 16].fill(0);
        }
        Some(())
    })()
    .is_some()
}
```
Lossless "Remove location data" preset for a JPEG, with no re-encode (verified):
```rust
let mut j = std::fs::read(path)?;
let r = exifclean::jpeg_exif_range(&j).unwrap();
let ok = exifclean::strip_gps(&mut j[r.clone()]);
std::fs::write(tmp_out, &j)?;
```
```
in-place GPS strip ok=true len_same=true bytes_outside_exif_same=true -> orient=Some("row 0 at top and column 0 at left") make=Some("\"NIKON\"") dto=Some("2008-10-22 16:28:39") pxdim=Some("640")xSome("480") gps_fields=["GPSVersionID"] thumb=true
```
Gotchas:
- **XMP can hold GPS too** (`exif:GPSLatitude` in an APP1 segment starting with `http://ns.adobe.com/xap/1.0/\0`, written by Lightroom and phones). The in-place JPEG path must also drop that segment, which needs a segment-copy rewrite: copy every segment except that APP1. The re-encode path drops XMP anyway.
- Gezik's lossless path (`exifclean::jpeg_without_location`) also drops APP13 (Photoshop/IPTC, which can name the city and place) and everything after the main picture's EOI together with the MPF APP2 index: MPF secondary pictures (Ultra HDR gain maps, previews) carry their own EXIF, and a motion photo's video can carry a location; their XMP descriptions are gone anyway. It fails closed: an EXIF block whose IFD0 cannot be read, or whose GPS IFD cannot be stripped, gives `None`, and the caller re-encodes without metadata.
- MakerNotes are kept. Nikon and Apple MakerNotes carry no coordinates, but some vendors store GPS in MakerNotes. To be strict, wipe 0x927C in the Exif IFD the same way as `wipe_entry`.
- For PNG (`eXIf`, needs a CRC32 recompute) and WebP (`EXIF` chunk, no CRC), the same in-place call works on the chunk payload. That was not exercised in the probe.
- The kamadak-exif 0.6.1 reader (`exif::Reader::new().read_raw(Vec<u8>)`) parses the result. kamadak has no writer.

---

## 4. Text: chardetng 1.0.0 + encoding_rs 0.8.42

### 4.1 Detection
```rust
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;

pub const SNIFF: usize = 64 * 1024;

pub enum Detected { Bom(&'static Encoding, usize), Ascii, Guess(&'static Encoding), Binary }

pub fn detect(head: &[u8], whole: bool, tld: Option<&[u8]>) -> Detected {
    if let Some((enc, bom_len)) = Encoding::for_bom(head) {
        return Detected::Bom(enc, bom_len); // UTF-8 / UTF-16LE / UTF-16BE BOMs
    }
    if head[..head.len().min(8192)].contains(&0) {
        return Detected::Binary;
    }
    if head.is_ascii() {
        return Detected::Ascii;
    }
    let mut det = EncodingDetector::new(Iso2022JpDetection::Deny); // 1.0 API: new() takes this enum
    det.feed(head, whole); // last=false when truncated: a split UTF-8 sequence is not an error
    Detected::Guess(det.guess(tld, Utf8Detection::Allow))      // 1.0 API: enum, not bool
}
```
Observed:
```
tr_1254.txt            tld=None: guess windows-1254           tld=tr: guess windows-1254
tr_iso8859_9.txt       tld=None: guess windows-1254           tld=tr: guess windows-1254
tr_utf8.txt            tld=None: guess UTF-8
tr_utf8bom.txt         BOM UTF-8 (3 bytes)
tr_utf16le.txt         BOM UTF-16LE (2 bytes)
tr_utf16be.txt         BOM UTF-16BE (2 bytes)
tr_utf16le_nobom.txt   Binary
ru_1251.txt            guess windows-1251
ru_koi8r.txt           guess KOI8-U
ja_sjis.txt            guess Shift_JIS
zh_gbk.txt             guess GBK
zh_big5.txt            guess Big5
de_1252.txt            guess windows-1252
ascii.txt              Ascii
short_tr_1254.txt      guess windows-1254     ("Işık\n", 5 bytes)
binary.bin             Binary
detect on 64 KB: windows-1254 in 2.3 ms
```
Gotchas:
- Check the BOM **before** the NUL test, because UTF-16 text is full of NULs. **BOM-less UTF-16 is reported as Binary and skipped.** chardetng never guesses UTF-16.
- KOI8-R text is guessed as **KOI8-U**. The two decode Cyrillic letters identically and differ only in a few box-drawing and Ukrainian positions. Show it as detected.
- The `tld` argument must be lower-case ASCII without a dot, or chardetng panics. Deriving `b"tr"` from the UI locale is safe. It changed nothing on these samples.
- `Ascii` means no conversion is needed for UTF-8 or any ASCII-compatible target. Only the EOL and cleanup options apply.

### 4.2 Labels (what the spec's list really maps to)
```
label utf-16le      -> UTF-16LE      output_encoding=UTF-8
label utf-16be      -> UTF-16BE      output_encoding=UTF-8
label iso-8859-9    -> windows-1254
label iso-8859-1    -> windows-1252
label latin5        -> windows-1254
label ascii / us-ascii -> windows-1252
label iso-8859-15   -> ISO-8859-15   koi8-r -> KOI8-R   shift_jis -> Shift_JIS   gbk -> GBK   big5 -> Big5
'€' in 'iso-8859-9' -> [80] (true ISO-8859-9 has no €; windows-1254 has 0x80)
```
**ISO-8859-9 and ISO-8859-1 cannot be offered as separate targets.** encoding_rs follows WHATWG, where those labels are windows-1254 and windows-1252. Bytes 0x80-0x9F then hold €, ‚ etc. instead of C1 controls. Turkish letters are identical (the probe's ISO-8859-9 bytes match Python's `iso8859_9`). Drop those two from the list, or show them as aliases.
**`UTF_16LE.new_encoder()` writes UTF-8** (`output_encoding()`), so UTF-16 output must be encoded by hand (4.3).

### 4.3 Streaming conversion with unmappable/malformed reporting (`src/text.rs::convert`)
One code path for every size: 64 KB reads, decoder and encoder state carried across chunks. A CR at the end of a chunk and a multi-byte character split by a read are handled (verified below).
```rust
let mut decoder = from.new_decoder_with_bom_removal();    // skips a BOM matching `from`
let mut encoder = match to { Target::Enc(e) => Some(e.new_encoder()), _ => None };
match to {
    Target::Utf8Bom => dst.write_all(b"\xEF\xBB\xBF")?,
    Target::Utf16Le => dst.write_all(&[0xFF, 0xFE])?,
    Target::Utf16Be => dst.write_all(&[0xFE, 0xFF])?,
    Target::Enc(_) => {}
}
...
// per 64 KB chunk:
decoded.clear();
// decode_to_string never grows the String: reserve the worst case first.
decoded.reserve(decoder.max_utf8_buffer_length_without_replacement(input.len()).unwrap());
let (res, read) = decoder.decode_to_string_without_replacement(input, &mut decoded, eof);
if let DecoderResult::Malformed(_, _) = res { return Err(TextError::Malformed { line: src_line }); }
lines.push(&decoded, &mut processed, &mut marks);   // EOL + trailing-space logic, records (offset, line) marks
...
let (r, read, written) = enc.encode_from_utf8_without_replacement(s, &mut outbuf, last);
match r {
    EncoderResult::InputEmpty => break,
    EncoderResult::OutputFull => continue,
    EncoderResult::Unmappable(ch) => {
        let at = consumed - ch.len_utf8();
        return Err(TextError::Unmappable { ch, line: line_at(&marks, start_line, at), target: e.name() });
    }
}
// UTF-16 by hand:
let b: Vec<u8> = processed.encode_utf16().flat_map(u16::to_le_bytes).collect();   // or to_be_bytes
```
Observed:
```
1254->utf8 equals python utf8: true
utf16le->utf8 equals: true
utf8->be equals python: true
utf8->le equals python: true
utf8->u8bom equals python: true
utf8bom->utf8 drops BOM: true
utf8->1252: can't encode 'ğ' in windows-1252 (line 2)
1251->1254: can't encode 'С' in windows-1254 (line 1)
bad utf8 source: malformed at line 2
CRLF split across reads -> LF: tail "a\nx\n", len 65538
UTF-8 char split across reads -> 1254 last bytes [FE, 0A]
deep: can't encode 'ş' in windows-1252 (line 200002); expected line 200002
100 MB 1254->utf8+CRLF: 406 ms, out 127514640 bytes
100 MB 1254->utf16le: 755 ms
```
Line endings and cleanup on `"a  \r\nb\t\rc \nd\r\n\r\n\r\n"`:
```
AsIs strip=false final=false: "a  \r\nb\t\rc \nd\r\n\r\n\r\n"
Lf strip=false final=false: "a  \nb\t\nc \nd\n\n\n"
Crlf strip=true final=true: "a\r\nb\r\nc\r\nd\r\n"
Cr strip=true final=false: "a\rb\rc\rd\r\r\r"
Lf strip=false final=true: "a  \nb\t\nc \nd\n"
```
- "Single final newline" holds back newlines until the next non-blank character arrives. At EOF it writes exactly one line ending if the file had any content, so trailing blank lines are dropped.
- Writing goes to a temp file, and any `Err` deletes it, so the original is never touched (spec: "değiştirilmez").
- Use the `*_without_replacement` decoder: a malformed source becomes an error row instead of producing U+FFFD.

---

## 5. ffmpeg

### 5.1 Versions tested on this machine
- `D:\ffmpeg\bin\ffmpeg.exe`: `ffmpeg version 2025-05-19-git-c55d65ac0a-full_build-www.gyan.dev`
- gyan essentials 9.0.2: `ffmpeg version 9.0.2-essentials_build-www.gyan.dev` (gcc 16.2.0). Configure has libx264, libx265, libwebp, libaom, libmp3lame, libopus, libvorbis, libvpx, libzimg, mediafoundation. **It has no libdav1d and no libsvtav1** (the full build has both). Encoders: `libaom-av1`, `libwebp`, `libwebp_anim`, `libx264`, `libx265`, `aac`, `libmp3lame`. Decoders: `hevc`, `av1` (native AV1, used for AVIF), `libaom-av1`, `webp`.
- Also gyan 8.0.1 and 7.1.1 essentials, used only for the HEIC grid test.

### 5.2 HEIC / AVIF → PNG (spec 6.5 verification)
Samples: libheif `examples/example.heic`, `examples/example.avif`, `tests/data/{rainbow-451x461.heic, with-alpha-512x512.heic, clap_cropped_irot_imir.avif}`; nokiatech `autumn_1440x960.heic`, `season_collection_1440x960.heic`; **real iPhone X photos** from github.com/inseven/incontext-test-data (Git LFS, `media.githubusercontent.com/media/inseven/incontext-test-data/HEAD/gallery/IMG_3857.heic` and `IMG_3870.heic`; 48 HEVC tiles of 512x512 in a 4032x3024 `Tile Grid` group, `irot` → display matrix rotation -90, EXIF Orientation 6, Display P3 ICC, GPS).

```
ffmpeg -nostdin -y -hide_banner -loglevel error -i IN.heic -frames:v 1 -update 1 OUT.png
```
| file | 9.0.2 essentials | 8.0.1 essentials | 7.1.1 essentials | 2025-05 git |
|---|---|---|---|---|
| inseven_IMG_3857.heic (grid, rot -90) | **3024x4032** (RMSE vs libheif/ImageMagick 0.003) | 512x512 (one tile) | 512x512 | 512x512 |
| inseven_IMG_3870.heic (grid, edited) | **3540x2654** (libheif: 3540x2655; RMSE 0.003 on the common area) | 675x506 | header error | 675x506 |
| example.heic, autumn, season_collection | correct size (1280x854, 1440x960) | | | |
| rainbow-451x461.heic (clap crop to odd size) | 450x460 | | | |
| with-alpha-512x512.heic | 512x512, **RGB, alpha lost** (libheif gives RGBA) | | | |
| example.avif | 800x533 | | | |
| clap_cropped_irot_imir.avif | 64x64 | | | |

Findings:
- **Only ffmpeg 9.x decodes iPhone grid HEICs correctly.** 7.1.1 and 8.0.1 output a single 512x512 tile with exit code 0, so a silent wrong result. `-map 0:g:0` does not help on 8.0.1; it maps the 48 tiles as separate streams and image2 then fails ("Cannot write more than one file with the same name"). **Pin ffmpeg ≥ 9.0** in gezik-tools.
- The rotation (irot/display matrix) is applied by the CLI by default, and the output PNG has no eXIf chunk. **Gezik must not apply EXIF orientation again for HEIC/AVIF inputs.** It also does not read EXIF from the temp PNG, which has none.
- **The ICC profile is lost for grid images.** The temp PNG has only IHDR, pHYs, IDAT and IEND; for the non-grid `rainbow` it has iCCP. iPhone photos are Display P3, so colours come out slightly desaturated when read as sRGB. Possible mitigation: read the `colr`(`prof`) box from the HEIC (`meta/iprp/ipco`) in Rust and attach it to a JPEG output. That is not done here.
- Odd crop sizes lose 1 px (4:2:0 chroma rounding): 451x461 → 450x460, 3540x2655 → 3540x2654.
- **Alpha in HEIC and AVIF inputs is dropped** (the auxiliary alpha image is not merged).
- `ffprobe -show_entries format=duration` prints `N/A` for HEIC. That is fine, because images do not use progress.
- Speed for the 12 MP iPhone file: default PNG level 0.78 s; with `-c:v png -compression_level 1` 0.24 s (14.8 MB temp file); with `-c:v pam -f image2pipe -` to stdout 0.14 s (36.6 MB raw, needs a small PAM header parser). Recommendation: temp PNG with `-compression_level 1`. The only warning is `[swscaler] deprecated pixel format used, make sure you did set range correctly`, which is harmless (yuvj420p).

### 5.3 Lossy WebP and AVIF output (from a temp PNG)
```
ffmpeg -nostdin -y -hide_banner -loglevel error -i in.png -frames:v 1 -c:v libwebp -lossless 0 -quality 80 -compression_level 4 out.webp
  alpha.png (RGBA 512x512) -> 18678 B, srgba (alpha kept);  big.png 3024x4032 -> 409524 B
ffmpeg -nostdin -y -hide_banner -loglevel error -i in.png -frames:v 1 -c:v libaom-av1 -still-picture 1 -crf 30 -cpu-used 6 -row-mt 1 -pix_fmt yuv420p out.avif
  big.png 3024x4032 -> 268111 B in 1.05 s
```
- Without `-pix_fmt yuv420p`, an RGB input becomes a **`gbrp`** AV1 (identity matrix), which is larger and less compatible. Always pass `-pix_fmt yuv420p`.
- AVIF with alpha needs a second stream (verified: libheif then reads `srgba`, alpha min 0 / max 1):
```
ffmpeg -nostdin -y -hide_banner -loglevel error -i in.png -filter_complex "[0:v]format=yuva420p,split[c][a];[c]format=yuv420p[c2];[a]alphaextract[a2]" -map "[c2]" -map "[a2]" -frames:v 1 -c:v libaom-av1 -still-picture 1 -crf 30 -cpu-used 6 -row-mt 1 out.avif
```
- Quality mapping: libwebp `-quality 0..100` maps directly. For AVIF, `-crf 0..63` (lower is better). A rough map from the UI's 1-100 is `crf = round(63 - q*0.55)` (q85 → 16, q50 → 35). That mapping is a suggestion and was not tuned.

### 5.4 Audio/video presets (all exit 0 on 9.0.2 essentials; inputs: 3840x2160 HEVC 10-bit + AAC .mov, 1080x1920 H.264 + PCM .mkv, 1280x720 H.264 + AAC .mkv, and a 4K .mp4 with `-display_rotation 90`)
Common prefix: `ffmpeg -nostdin -y -hide_banner -loglevel error -progress pipe:1 -nostats -i IN ... TMP_OUT`
| preset | args after `-i IN` | observed |
|---|---|---|
| MP4 (H.264+AAC) | `-c:v libx264 -crf 20 -preset medium -pix_fmt yuv420p -c:a aac -b:a 160k -movflags +faststart out.mp4` | 6 s 4K10bit in 3.1 s |
| Smaller video | `-vf "scale='if(gte(iw,ih),min(1920,iw),min(1080,iw))':'if(gte(iw,ih),min(1080,ih),min(1920,ih))':force_original_aspect_ratio=decrease:force_divisible_by=2" -c:v libx264 -crf 28 -preset medium -pix_fmt yuv420p -c:a aac -b:a 128k -movflags +faststart out.mp4` | 3840x2160 → 1920x1080; 1080x1920 → 1080x1920; 1280x720 → 1280x720 (never enlarged); 4K with rotation 90 → **1080x1920** (autorotate runs before the filter) |
| MP3 | `-vn -c:a libmp3lame -b:a 192k out.mp3` | mp3,44100,1,192000 |
| M4A (AAC) | `-vn -c:a aac -b:a 192k -movflags +faststart out.m4a` | aac,44100 |
| WAV | `-vn -c:a pcm_s16le out.wav` (add `-rf64 auto` for >4 GB) | pcm_s16le,44100,1,705600 |
| Remux to MP4 | `-c copy -movflags +faststart out.mp4` | h264+aac ok; h264+pcm ok (9.x mp4 accepts LPCM); mkv with ASS subtitles ok (default stream selection leaves the subtitle out); **VP8 webm fails**: `Could not find tag for codec vp8 in stream #0, codec not currently supported in container` / `Could not write header (incorrect codec parameters ?): Invalid argument`, non-zero exit |
| GIF from video | `-an -filter_complex "fps=12,scale='if(gte(iw,ih),min(480,iw),-2)':'if(gte(iw,ih),-2,min(480,ih))':flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" -loop 0 out.gif` | 6 s 4K → 480x270, 72 frames, 1.17 MB, 0.8 s; rotated → 270x480 |
- `-pix_fmt yuv420p` is required for 10-bit, 4:2:2 or 4:4:4 sources (iPhone HDR HEVC). Without it, x264 writes High 10 or 4:4:4 profiles that most players reject. The table shows `High,1920,1080,yuv420p` in the output.
- "480 px" is implemented as the longest side, never enlarged. Use `scale=480:-2` if it should mean width.

### 5.5 Progress output (`-progress pipe:1 -nostats`, stdout)
```
frame=0
fps=0.00
stream_0_0_q=0.0
bitrate=   0.2kbits/s
total_size=48
out_time_us=2461315
out_time_ms=2461315
out_time=00:00:02.461315
dup_frames=0
drop_frames=0
speed=4.77x
progress=continue
...
frame=180
fps=65.23
stream_0_0_q=-1.0
bitrate=24904.6kbits/s
total_size=18678475
out_time_us=6000000
out_time_ms=6000000
out_time=00:00:06.000000
dup_frames=0
drop_frames=0
speed=2.17x
progress=end
```
- Blocks are `key=value` lines ending in `progress=continue|end`, sent every 0.5 s (`-stats_period` changes that); 6 blocks for 3 s.
- **`out_time_ms` is also microseconds** (a historical misnomer), so parse `out_time_us`. Early blocks can show `N/A`, and the value can run ahead of the frame count because audio is encoded first. Clamp to [0, duration].
- Duration:
```
ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 IN
6.000000          (src4k10bit.mov)
N/A               (example.heic, exit 0)
```
- Errors go to stderr (with `-loglevel error` it holds only real errors). Keep the last 20 lines.

---

## 6. ffmpeg for gezik-tools

Facts gathered (2026-10-06):

| source | platforms | latest release | per-file size | ffprobe | x264/x265 | libwebp | AV1 enc | AV1 dec | checksum | notes |
|---|---|---|---|---|---|---|---|---|---|---|
| **gyan.dev** (`github.com/GyanD/codexffmpeg/releases`) | win x64 | 9.0.2 (2026-09-20) | essentials `.7z` 35,430,500 B (`.zip` 114,768,076) holding ffmpeg/ffprobe/ffplay 105 MB each | yes | yes | yes | libaom | native av1 + libaom (no dav1d) | GitHub asset `digest` (sha256:4705843c…7071077 for the .7z, matched by local `sha256sum`); also `https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.7z.sha256` | GPLv3; README gives source commit 946fcce07b. No arm64 |
| **BtbN** (`github.com/BtbN/FFmpeg-Builds/releases`) | win64, winarm64, linux64, linuxarm64 | `n9.0.2-22-g46d8f462ee` (9.0 branch head) | winarm64 gpl zip 135,917,512 B (ffmpeg.exe 94.7 MB, ffprobe.exe 94.5 MB); win64 gpl 193,964,226 B (165 MB exes); linux64 gpl tar.xz 151 MB | yes | yes | yes | aom, svt-av1, rav1e | dav1d | GitHub asset `digest` | Linux needs glibc ≥ 2.28. Retention: last 14 daily builds, plus the last build of each month for 2 years. The `latest` tag floats |
| **Martin Riedl** (`ffmpeg.martin-riedl.de`) | macOS arm64/x64, Linux amd64/arm64 | 9.0.2 | ffmpeg.zip / ffprobe.zip separate: mac arm64 28.4/28.3 MB, mac x64 33.8/33.7 MB, linux amd64 33.3/33.2 MB, linux arm64 28.8/28.7 MB | yes (separate zip) | yes | yes (1.6.0) | aom 3.15, svt-av1 4.2, rav1e | dav1d 1.5.4 | `<url>.sha256` beside every zip (linux amd64 ffprobe.zip `3f428c49…` matched) | **macOS: Developer ID signed** ("Developer ID Application: Martin Riedl (KU3N25YGLU)", LC_CODE_SIGNATURE present) and links only to system frameworks. **Linux is dynamic against glibc ≥ 2.35** (PT_INTERP, `GLIBC_2.35`), so Ubuntu 22.04+ / Debian 12+ work and RHEL 9 does not |
| evermeet.cx | macOS x64 only | 9.0.2 | 7z 18 MB / zip 26 MB | separate | yes | yes | aom | dav1d | GPG `.sig` | Intel only, no Apple Silicon |
| osxexperts.net | macOS arm64 9.0, Intel 8.0 | 9.0 | zips | separate | ? | ? | ? | ? | sha256 printed on page | unversioned URLs (`ffmpeg9arm.zip`) |
| johnvansickle.com | linux static | **7.0.2 (Aug 2024)** | 42 MB | yes | | | | | md5 | stale, and 7.x fails the HEIC grid test. Rejected |

Recommendation (one per platform). Gezik downloads only from `wenlar/gezik-tools`; these are the inputs for `scripts/tools/`:
- **Windows x64:** gyan `https://github.com/GyanD/codexffmpeg/releases/download/{ver}/ffmpeg-{ver}-essentials_build.7z`. Verify with the release asset `digest` (GitHub API `assets[].digest`) or `https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-{ver}-essentials_build.7z.sha256`.
- **Windows arm64:** BtbN month-end autobuild `https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-{YYYY-MM-DD-HH-MM}/ffmpeg-n9.0.{x}-{n}-g{hash}-winarm64-gpl-9.0.zip`. Use the month-end tag, which is kept for 2 years. Verify with the asset `digest`.
- **macOS arm64 and x64:** Martin Riedl `https://ffmpeg.martin-riedl.de/download/macos/{arm64|amd64}/{timestamp}_{ver}/{ffmpeg|ffprobe}.zip` plus `.sha256`. They are already signed, so no ad-hoc signing is needed. Re-signing would break his signature; keep it.
- **Linux x64 and arm64:** BtbN `linux64` / `linuxarm64` gpl (glibc 2.28 floor) for the widest reach; the box also offers the package-manager hint. The alternative is Martin Riedl Linux (smaller, glibc 2.35 floor).
- **Repack** each into one archive holding only `ffmpeg`, `ffprobe` and the licence, using **solid** compression. ffmpeg and ffprobe share about 99% of their code. Measured with gyan 9.0.2 (ffmpeg.exe + ffprobe.exe + LICENSE = 210 MB):
  - `7z a -t7z -mx=9 -ms=on`: **30,250,456 B**, 54 s.
  - `tar | xz -9e -T0`: 56,037,856 B, because the threaded blocks lose the cross-file match.
  - `tar | xz -T1 --lzma2=preset=9e,dict=256MiB`: **30,344,072 B**.
  - So the spec's "~35 MB" holds if the repack is solid with a dictionary larger than one binary. 5b's code reads both 7z and tar.xz.
- **Pin ≥ 9.0** because of the HEIC grid finding. The planned tag name `ffmpeg-7.1-1` in the spec must change, for example to `ffmpeg-9.0.2-1`.
- Licence: all chosen builds are GPL (v3 for gyan and BtbN gpl, which use `--enable-version3`). Ship the LICENSE and a source link: the FFmpeg commit or tag plus the builder's script repo.

---

## 7. Release exe size (Gezik profile: opt 3, lto, cgu 1, strip, panic=abort; x86_64-pc-windows-msvc)

The sizeprobe baseline is what Gezik does today, `image::open(...).to_rgba8()` with png/jpeg/gif/webp/bmp. Its exe is 794,624 B, or 844,288 B with rayon linked (Gezik already has rayon).

| stage (features in `sizeprobe/Cargo.toml`) | exe bytes | delta |
|---|---|---|
| baseline | 794,624 | – |
| + `tiff` + `ico` decode | 1,123,328 | **+328,704** (tiff alone +319,488; ico alone +7,168) |
| + encoders (jpeg with ICC/EXIF, png Default, webp lossless, bmp, apply_orientation) | 1,281,024 | **+157,696** |
| image JpegEncoder only | 820,736 | +26,112 |
| jpeg-encoder 0.7.1 instead | 862,720 | +68,096 |
| baseline + rayon | 844,288 | (reference for the rows below) |
| fast_image_resize generic `resize()` with DynamicImage (all 13 pixel types) | 2,786,816 | +1,942,528 |
| … with fir `rayon` feature | 4,064,768 | +3,220,480 |
| fir `resize_typed` U8x3 + U8x4 | 1,701,376 | +857,088 (rayon feature: +1,384,448) |
| fir `resize_typed` U8x3 + U8x4 + U16x4 | 2,252,288 | +1,408,000 |
| **fir `resize_typed` U8x4 only** | 1,224,192 | **+379,904** (rayon feature: +689,664) |
| image `resize_exact(Lanczos3)` instead | 884,224 | +39,936 |
| encoding_rs alone (decode + encode) | 1,038,848 | +244,224 |
| **encoding_rs + chardetng** | 1,119,744 | **+325,120** |
| **recommended set** (tiff/ico + encoders + fir U8x4 + encoding_rs/chardetng) | 2,033,152 | **+1,188,864** over baseline+rayon |
| everything heavy (generic fir + fir rayon + text) | 4,873,728 | +4.03 MB |

For scale, the current `D:\Work\gezik\target\release\gezik.exe` is 17,676,800 B. Gezik already links the PNG encoder through the `gezik` crate, so the real encoder delta is a little lower.

---

## 8. Gotchas (short list for the plan)
1. Read `icc_profile()`, `exif_metadata()` and `orientation()` before `DynamicImage::from_decoder`, which consumes the decoder.
2. JPEG output: `flatten` first, because `write_with_encoder` drops alpha without compositing. Guard EXIF at ≤ 65527 B (no length check in image's JPEG `write_segment`).
3. EXIF for JPEG→JPEG: `Orientation::remove_from_exif_chunk`, then `exifclean::{strip_gps (if requested), drop_thumbnail, set_pixel_dims}`. All edits are in place and keep the length.
4. The lossless "Remove location data" JPEG path must also drop XMP APP1 segments.
5. fast_image_resize: use `resize_typed::<U8x4>`, not `resize()`, unless you accept +1.9 MB. Leave the `rayon` feature off.
6. ISO-8859-1 and ISO-8859-9 are windows-1252 and windows-1254 in encoding_rs. UTF-16 output must be encoded by hand. BOM-less UTF-16 looks binary.
7. `decode_to_string*` never grows the String: reserve `max_utf8_buffer_length_without_replacement` first.
8. ffmpeg must be ≥ 9.0 for iPhone HEIC grids. 7.1 and 8.0 return one 512 px tile with exit 0.
9. ffmpeg HEIC/AVIF decode loses the ICC (grid) and alpha, and crops odd sizes by 1 px. It applies the irot rotation itself, so do not rotate again by EXIF.
10. AVIF encode: always `-pix_fmt yuv420p`, and add an alpha stream via `alphaextract` when there is alpha. gyan essentials has libaom but no SVT-AV1 or dav1d.
11. `-progress`: parse `out_time_us`. `out_time_ms` is microseconds too. Handle `N/A`.
12. Repack ffmpeg + ffprobe as a solid archive (7z `-ms=on` or xz with a 256 MiB dictionary and one thread) to get about 30 MB instead of 56+.
