//! Conversion decisions without touching files: the built-in presets and their options, where
//! an output goes, how big a resized picture gets, which names are pictures or media, the
//! arguments for ffmpeg and its progress lines, and the user commands' placeholders.
//! The work itself is in `gezik-batch::convert`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::ops::paths::same_path;

/// The groups of the Convert layer's preset list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
    Media,
    Command,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebpLossless,
    /// Written by ffmpeg.
    WebpLossy,
    /// Written by ffmpeg.
    Avif,
    Bmp,
}

impl ImageFormat {
    /// The output's extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::WebpLossless | ImageFormat::WebpLossy => "webp",
            ImageFormat::Avif => "avif",
            ImageFormat::Bmp => "bmp",
        }
    }

    /// Whether writing this format needs ffmpeg.
    pub fn needs_ffmpeg(self) -> bool {
        matches!(self, ImageFormat::WebpLossy | ImageFormat::Avif)
    }

    /// The format a picture named `name` is in, by its extension: for "Remove location data",
    /// which keeps each file's format. `None` for formats Gezik does not write (gif, tiff,
    /// heic…); WebP counts as lossy.
    pub fn of_name(name: &str) -> Option<ImageFormat> {
        match extension_lower(name).as_deref() {
            Some("jpg" | "jpeg" | "jpe" | "jfif") => Some(ImageFormat::Jpeg),
            Some("png") => Some(ImageFormat::Png),
            Some("webp") => Some(ImageFormat::WebpLossy),
            Some("avif") => Some(ImageFormat::Avif),
            Some("bmp") => Some(ImageFormat::Bmp),
            _ => None,
        }
    }
}

/// How a picture is resized (the sizes are in pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    None,
    Longest(u32),
    Width(u32),
    Height(u32),
    Percent(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageOptions {
    pub format: ImageFormat,
    /// 1-100, for JPEG, lossy WebP and AVIF.
    pub quality: u8,
    pub resize: Resize,
    pub never_enlarge: bool,
    pub rotate_by_exif: bool,
    /// Drops EXIF (location included) and XMP.
    pub strip_metadata: bool,
    /// What transparency becomes in a JPEG.
    pub background: [u8; 3],
}

impl ImageOptions {
    /// The layer's defaults: JPEG 85, no resize, never enlarge, rotate by EXIF, white.
    pub const DEFAULT: ImageOptions = ImageOptions {
        format: ImageFormat::Jpeg,
        quality: 85,
        resize: Resize::None,
        never_enlarge: true,
        rotate_by_exif: true,
        strip_metadata: false,
        background: [255, 255, 255],
    };
}

impl Default for ImageOptions {
    fn default() -> Self {
        ImageOptions::DEFAULT
    }
}

/// Line endings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Keep,
    Lf,
    Crlf,
    Cr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOptions {
    /// The source encoding's label; `None` detects it per file.
    pub from: Option<String>,
    /// The target encoding's label; empty keeps each file's own encoding (line ending presets).
    pub to: String,
    pub bom: bool,
    pub eol: Eol,
    pub trim_trailing: bool,
    pub final_newline: bool,
}

impl TextOptions {
    /// Whether each file stays in its own encoding.
    pub fn keeps_encoding(&self) -> bool {
        self.to.is_empty()
    }
}

/// The built-in audio and video conversions (ffmpeg).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPreset {
    Mp4,
    Smaller,
    Mp3,
    M4a,
    Wav,
    Remux,
    Gif,
}

impl MediaPreset {
    /// The output's extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            MediaPreset::Mp4 | MediaPreset::Smaller | MediaPreset::Remux => "mp4",
            MediaPreset::Mp3 => "mp3",
            MediaPreset::M4a => "m4a",
            MediaPreset::Wav => "wav",
            MediaPreset::Gif => "gif",
        }
    }

    /// The ffmpeg muxer, given with `-f` because outputs are written under a temporary name
    /// without the extension ffmpeg would guess it from.
    fn muxer(self) -> &'static str {
        match self {
            MediaPreset::Mp4 | MediaPreset::Smaller | MediaPreset::Remux => "mp4",
            MediaPreset::Mp3 => "mp3",
            MediaPreset::M4a => "ipod",
            MediaPreset::Wav => "wav",
            MediaPreset::Gif => "gif",
        }
    }
}

/// Where outputs go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Next to the input, `photo.png` → `photo.webp`, or `photo (converted).jpg` when the
    /// extension stays.
    SameFolder,
    /// In a `converted` folder next to the input.
    Subfolder,
    /// In this folder.
    Folder(PathBuf),
    /// Under the input's name (with the new extension); the input goes to the trash.
    ReplaceOriginal,
}

/// The name of the folder [`Output::Subfolder`] writes into.
pub const SUBFOLDER: &str = "converted";

/// A text preset: target encoding (`None` keeps each file's own) and line endings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPreset {
    pub to: Option<&'static str>,
    pub eol: Eol,
}

impl TextPreset {
    pub fn options(&self) -> TextOptions {
        TextOptions {
            from: None,
            to: self.to.unwrap_or_default().to_owned(),
            bom: false,
            eol: self.eol,
            trim_trailing: false,
            final_newline: false,
        }
    }
}

/// What a preset does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetWhat {
    Image(ImageOptions),
    Text(TextPreset),
    Media(MediaPreset),
}

/// A built-in preset of the Convert layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// Stable, for `state.toml`.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub what: PresetWhat,
}

impl Preset {
    /// Where its outputs go unless the user picks otherwise: text replaces the original
    /// (spec 6.3), everything else goes next to it.
    pub fn default_output(&self) -> Output {
        match self.kind {
            Kind::Text => Output::ReplaceOriginal,
            _ => Output::SameFolder,
        }
    }

    /// Whether it writes each picture in the format it already has ("Remove location data")
    /// instead of its options' `format`.
    pub fn keeps_format(&self) -> bool {
        self.id == REMOVE_LOCATION
    }

    /// The format it writes the picture `name` in: its options' format, or for a preset that
    /// [keeps the format](Self::keeps_format) the picture's own, with WebP written lossless (no
    /// ffmpeg, no generation loss). `None` for a format Gezik cannot write (gif, tiff, heic…)
    /// and for presets that are not about pictures.
    pub fn format_for(&self, name: &str) -> Option<ImageFormat> {
        let PresetWhat::Image(options) = self.what else { return None };
        if !self.keeps_format() {
            return Some(options.format);
        }
        match ImageFormat::of_name(name)? {
            ImageFormat::WebpLossy => Some(ImageFormat::WebpLossless),
            format => Some(format),
        }
    }

    /// Whether running it on `name` needs ffmpeg: media always, pictures when the picture is
    /// read through ffmpeg (HEIC, HEIF, AVIF) or written by it (lossy WebP, AVIF).
    pub fn needs_ffmpeg_for(&self, name: &str) -> bool {
        match self.what {
            PresetWhat::Image(_) => {
                needs_ffmpeg_to_read(name) || self.format_for(name).is_some_and(ImageFormat::needs_ffmpeg)
            }
            PresetWhat::Text(_) => false,
            PresetWhat::Media(_) => true,
        }
    }

    /// Whether running it needs ffmpeg whatever the inputs are (for a preset that keeps the
    /// format, that depends on the inputs: [`Self::needs_ffmpeg_for`]).
    pub fn needs_ffmpeg(&self) -> bool {
        if self.keeps_format() {
            return false;
        }
        match self.what {
            PresetWhat::Image(options) => options.format.needs_ffmpeg(),
            PresetWhat::Text(_) => false,
            PresetWhat::Media(_) => true,
        }
    }
}

/// The id of "Remove location data": it keeps each picture's format ([`Preset::format_for`])
/// and, for a JPEG, does not re-encode (its `format` field is only a placeholder). It rotates
/// by EXIF when it re-encodes, since the orientation tag goes with the EXIF.
pub const REMOVE_LOCATION: &str = "remove-location";

const fn image(format: ImageFormat, quality: u8, resize: Resize, strip_metadata: bool) -> PresetWhat {
    PresetWhat::Image(ImageOptions { format, quality, resize, strip_metadata, ..ImageOptions::DEFAULT })
}

/// The built-in presets (spec 6.2-6.4), grouped by kind in this order.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "resize-photos",
        label: "Resize photos (JPEG 1920 px)",
        kind: Kind::Image,
        what: image(ImageFormat::Jpeg, 85, Resize::Longest(1920), false),
    },
    Preset {
        id: "to-jpeg",
        label: "Convert to JPEG",
        kind: Kind::Image,
        what: image(ImageFormat::Jpeg, 85, Resize::None, false),
    },
    Preset {
        id: "to-png",
        label: "Convert to PNG",
        kind: Kind::Image,
        what: image(ImageFormat::Png, 85, Resize::None, false),
    },
    Preset {
        id: "to-webp",
        label: "Convert to WebP",
        kind: Kind::Image,
        what: image(ImageFormat::WebpLossless, 85, Resize::None, false),
    },
    Preset {
        id: "to-avif",
        label: "Convert to AVIF",
        kind: Kind::Image,
        // crf 30, what the library notes measured.
        what: image(ImageFormat::Avif, 60, Resize::None, false),
    },
    Preset {
        id: REMOVE_LOCATION,
        label: "Remove location data",
        kind: Kind::Image,
        what: PresetWhat::Image(ImageOptions {
            format: ImageFormat::Jpeg,
            quality: 85,
            resize: Resize::None,
            never_enlarge: true,
            rotate_by_exif: true,
            strip_metadata: true,
            background: [255, 255, 255],
        }),
    },
    Preset {
        id: "to-utf8",
        label: "Convert to UTF-8",
        kind: Kind::Text,
        what: PresetWhat::Text(TextPreset { to: Some("UTF-8"), eol: Eol::Keep }),
    },
    Preset {
        id: "eol-crlf",
        label: "Windows line endings (CRLF)",
        kind: Kind::Text,
        what: PresetWhat::Text(TextPreset { to: None, eol: Eol::Crlf }),
    },
    Preset {
        id: "eol-lf",
        label: "Unix line endings (LF)",
        kind: Kind::Text,
        what: PresetWhat::Text(TextPreset { to: None, eol: Eol::Lf }),
    },
    Preset { id: "mp4", label: "MP4 (H.264 + AAC)", kind: Kind::Media, what: PresetWhat::Media(MediaPreset::Mp4) },
    Preset {
        id: "smaller-video",
        label: "Smaller video",
        kind: Kind::Media,
        what: PresetWhat::Media(MediaPreset::Smaller),
    },
    Preset { id: "mp3", label: "MP3", kind: Kind::Media, what: PresetWhat::Media(MediaPreset::Mp3) },
    Preset { id: "m4a", label: "M4A (AAC)", kind: Kind::Media, what: PresetWhat::Media(MediaPreset::M4a) },
    Preset { id: "wav", label: "WAV", kind: Kind::Media, what: PresetWhat::Media(MediaPreset::Wav) },
    Preset { id: "remux-mp4", label: "Remux to MP4", kind: Kind::Media, what: PresetWhat::Media(MediaPreset::Remux) },
    Preset { id: "gif", label: "GIF from video", kind: Kind::Media, what: PresetWhat::Media(MediaPreset::Gif) },
];

/// The built-in preset with this id.
pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// The largest width or height [`target_size`] gives.
pub const MAX_SIDE: u32 = 65535;

/// The size a `w`×`h` picture is resized to, or `None` when it stays as it is (no resize, the
/// same size, or larger while `never_enlarge`). Call it after the EXIF rotation, so that
/// "width" is the width as shown. Neither side gets larger than [`MAX_SIDE`] (jpeg-encoder's
/// limit): a larger result is scaled down further, keeping the aspect ratio.
pub fn target_size(w: u32, h: u32, resize: &Resize, never_enlarge: bool) -> Option<(u32, u32)> {
    if w == 0 || h == 0 {
        return None;
    }
    let scale = match *resize {
        Resize::None => return None,
        Resize::Longest(n) => n as f64 / w.max(h) as f64,
        Resize::Width(n) => n as f64 / w as f64,
        Resize::Height(n) => n as f64 / h as f64,
        Resize::Percent(p) => p as f64 / 100.0,
    };
    if scale <= 0.0 || (never_enlarge && scale >= 1.0) {
        return None;
    }
    let scale = scale.min(MAX_SIDE as f64 / w.max(h) as f64);
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    if (nw, nh) == (w, h) { None } else { Some((nw, nh)) }
}

/// The final path of `input` converted to a file ending in `ext` (without the dot; empty for
/// no extension). It is the input itself for [`Output::ReplaceOriginal`] when the extension
/// stays. When the output would be the input itself in another mode, it gets ` (converted)`.
pub fn output_path(input: &Path, ext: &str, output: &Output) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new(""));
    let stem = input.file_stem().unwrap_or(OsStr::new(""));
    let same_ext = input.extension().and_then(OsStr::to_str).unwrap_or("").eq_ignore_ascii_case(ext)
        && (input.extension().is_some() == !ext.is_empty());
    let named = |suffix: &str| {
        let mut name = stem.to_os_string();
        name.push(suffix);
        if !ext.is_empty() {
            name.push(".");
            name.push(ext);
        }
        name
    };
    match output {
        Output::SameFolder if same_ext => dir.join(named(" (converted)")),
        Output::SameFolder | Output::ReplaceOriginal => dir.join(named("")),
        Output::Subfolder => dir.join(SUBFOLDER).join(named("")),
        Output::Folder(folder) => {
            let path = folder.join(named(""));
            if same_path(&path, input) { folder.join(named(" (converted)")) } else { path }
        }
    }
}

fn extension_lower(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.to_ascii_lowercase())
}

fn has_extension(name: &str, list: &[&str]) -> bool {
    extension_lower(name).is_some_and(|ext| list.contains(&ext.as_str()))
}

/// Pictures Gezik reads itself.
const IMAGE_EXTENSIONS: [&str; 12] =
    ["png", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "tif", "tiff", "ico", "dib"];

/// Pictures read through ffmpeg (decoded to a temporary PNG first).
const FFMPEG_IMAGE_EXTENSIONS: [&str; 4] = ["heic", "heif", "hif", "avif"];

const MEDIA_EXTENSIONS: [&str; 36] = [
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "m2v", "ts", "mts", "m2ts", "3gp", "3g2",
    "ogv", "vob", "mxf", "asf", "mp3", "m4a", "m4b", "aac", "wav", "flac", "ogg", "oga", "opus", "wma", "aif", "aiff",
    "ac3", "amr", "mka", "caf",
];

/// Whether `name` is a picture the image presets accept (including the ffmpeg ones).
pub fn image_inputs(name: &str) -> bool {
    has_extension(name, &IMAGE_EXTENSIONS) || needs_ffmpeg_to_read(name)
}

/// Whether reading the picture `name` needs ffmpeg (≥ 9.0): HEIC, HEIF and AVIF.
pub fn needs_ffmpeg_to_read(name: &str) -> bool {
    has_extension(name, &FFMPEG_IMAGE_EXTENSIONS)
}

/// Whether `name` is audio or video the media presets accept.
pub fn media_inputs(name: &str) -> bool {
    has_extension(name, &MEDIA_EXTENSIONS)
}

/// The AVIF `-crf` (0-63, lower is better) for a quality of 1-100: `63 - q × 0.55` rounded,
/// halves going to the better quality (q85 → 16, q50 → 35).
pub fn avif_crf(quality: u8) -> u8 {
    let q = u32::from(quality.min(100));
    // In hundredths: 6300 - 55q is never negative for q ≤ 100.
    let hundredths = 6300 - 55 * q;
    ((hundredths + 49) / 100).min(63) as u8
}

fn args<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(items: I) -> Vec<OsString> {
    items.into_iter().map(|s| s.as_ref().to_os_string()).collect()
}

/// The arguments in `text`, which are separated by single spaces.
fn words(text: &'static str) -> impl Iterator<Item = &'static str> {
    text.split(' ')
}

const COMMON: [&str; 5] = ["-nostdin", "-y", "-hide_banner", "-loglevel", "error"];

const SMALLER_SCALE: &str = "scale='if(gte(iw,ih),min(1920,iw),min(1080,iw))':'if(gte(iw,ih),min(1080,ih),min(1920,ih))'\
                             :force_original_aspect_ratio=decrease:force_divisible_by=2";

const GIF_FILTER: &str = "fps=12,scale='if(gte(iw,ih),min(480,iw),-2)':'if(gte(iw,ih),-2,min(480,ih))':flags=lanczos,\
                          split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5\
                          :diff_mode=rectangle";

const AVIF_ALPHA_FILTER: &str = "[0:v]format=yuva420p,split[c][a];[c]format=yuv420p[c2];[a]alphaextract[a2]";

/// ffmpeg's arguments (without the program) for a media preset, progress on stdout
/// (`-progress pipe:1`). `output` may be a temporary name: the muxer is given with `-f`.
pub fn ffmpeg_args(preset: MediaPreset, input: &Path, output: &Path) -> Vec<OsString> {
    let mut list = args(COMMON);
    list.extend(args(["-progress", "pipe:1", "-nostats", "-i"]));
    list.push(input.as_os_str().to_os_string());
    let middle: Vec<&str> = match preset {
        MediaPreset::Mp4 => {
            words("-c:v libx264 -crf 20 -preset medium -pix_fmt yuv420p -c:a aac -b:a 160k -movflags +faststart")
                .collect()
        }
        MediaPreset::Smaller => ["-vf", SMALLER_SCALE]
            .into_iter()
            .chain(words(
                "-c:v libx264 -crf 28 -preset medium -pix_fmt yuv420p -c:a aac -b:a 128k -movflags +faststart",
            ))
            .collect(),
        MediaPreset::Mp3 => words("-vn -c:a libmp3lame -b:a 192k").collect(),
        MediaPreset::M4a => words("-vn -c:a aac -b:a 192k -movflags +faststart").collect(),
        MediaPreset::Wav => words("-vn -c:a pcm_s16le -rf64 auto").collect(),
        MediaPreset::Remux => words("-c copy -movflags +faststart").collect(),
        MediaPreset::Gif => vec!["-an", "-filter_complex", GIF_FILTER, "-loop", "0"],
    };
    list.extend(args(middle));
    list.extend(args(["-f", preset.muxer()]));
    list.push(output.as_os_str().to_os_string());
    list
}

/// A picture step done by ffmpeg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegImage {
    /// HEIC, HEIF or AVIF to a temporary PNG (fast compression; ffmpeg applies the rotation).
    DecodeToPng,
    /// A PNG to lossy WebP, quality 1-100 (alpha is kept).
    WebpLossy { quality: u8 },
    /// A PNG to AVIF, quality 1-100 (see [`avif_crf`]); `alpha` adds the alpha stream.
    Avif { quality: u8, alpha: bool },
}

/// ffmpeg's arguments (without the program) for a picture step. `output` may be a temporary
/// name: the muxer is given with `-f`.
pub fn ffmpeg_image_args(step: FfmpegImage, input: &Path, output: &Path) -> Vec<OsString> {
    let mut list = args(COMMON);
    list.push("-i".into());
    list.push(input.as_os_str().to_os_string());
    match step {
        FfmpegImage::DecodeToPng => {
            list.extend(args(words("-frames:v 1 -update 1 -c:v png -compression_level 1 -f image2")))
        }
        FfmpegImage::WebpLossy { quality } => {
            list.extend(args(words("-frames:v 1 -c:v libwebp -lossless 0 -quality")));
            list.push(quality.clamp(1, 100).to_string().into());
            list.extend(args(words("-compression_level 4 -f webp")));
        }
        FfmpegImage::Avif { quality, alpha } => {
            if alpha {
                list.extend(args(["-filter_complex", AVIF_ALPHA_FILTER, "-map", "[c2]", "-map", "[a2]"]));
            }
            list.extend(args(words("-frames:v 1 -c:v libaom-av1 -still-picture 1 -crf")));
            list.push(avif_crf(quality).to_string().into());
            list.extend(args(words("-cpu-used 6 -row-mt 1")));
            if !alpha {
                list.extend(args(["-pix_fmt", "yuv420p"]));
            }
            list.extend(args(["-f", "avif"]));
        }
    }
    list.push(output.as_os_str().to_os_string());
    list
}

/// One `-progress` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// How far the output is, in microseconds (`out_time_us`); `None` for `N/A`.
    pub out_us: Option<u64>,
    /// `progress=end`.
    pub end: bool,
}

/// Reads a block of `key=value` lines that ends with `progress=continue` or `progress=end`.
pub fn parse_progress_block(lines: &[&str]) -> Progress {
    let mut progress = Progress::default();
    for line in lines {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        match key.trim() {
            "out_time_us" => progress.out_us = value.trim().parse().ok(),
            "progress" => progress.end = value.trim() == "end",
            _ => {}
        }
    }
    progress
}

/// The (major, minor) version from the first line of `ffmpeg -version`: `ffmpeg version
/// 9.0.2-essentials_build…` → (9, 0), BtbN's `n9.0.2-22-g…` → (9, 0). Builds from git
/// (`2025-05-19-git-…`, `N-…`) give `None`.
pub fn parse_ffmpeg_version(first_line: &str) -> Option<(u32, u32)> {
    let rest = first_line.trim_start().strip_prefix("ffmpeg version ")?.trim_start();
    let rest = rest.strip_prefix('n').unwrap_or(rest);
    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let major_len = digits(rest);
    let after_major = rest[major_len..].strip_prefix('.')?;
    let minor_len = digits(after_major);
    if major_len == 0 || minor_len == 0 {
        return None;
    }
    Some((rest[..major_len].parse().ok()?, after_major[..minor_len].parse().ok()?))
}

/// A user command from `settings.toml` `[[commands]]` (spec 7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: String,
    /// The program and its arguments; no shell.
    pub run: Vec<String>,
    /// The output's name template (`{name}-small.{ext}`); `None`: the command changes the
    /// input in place.
    pub output: Option<String>,
    /// Extensions it runs on (any case, with or without the dot); empty: every file.
    pub types: Vec<String>,
    /// Whether it also runs on folders.
    pub folders: bool,
    /// 1-16 at a time.
    pub parallel: u8,
}

const PLACEHOLDERS: [&str; 6] = ["in", "out", "dir", "name", "ext", "outdir"];

/// Splits `text` into literal pieces and placeholders; `{{` and `}}` are literal braces.
fn pieces(text: &str) -> Result<Vec<Result<String, &str>>, String> {
    let mut out = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        literal.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(after) = tail.strip_prefix("{{") {
            literal.push('{');
            rest = after;
        } else if let Some(after) = tail.strip_prefix("}}") {
            literal.push('}');
            rest = after;
        } else if tail.starts_with('}') {
            return Err(format!("a }} without its {{ in \"{text}\""));
        } else {
            let Some(end) = tail.find('}') else { return Err(format!("a {{ is not closed in \"{text}\"")) };
            let field = &tail[1..end];
            if !PLACEHOLDERS.contains(&field) {
                return Err(format!("unknown placeholder {{{field}}}"));
            }
            if !literal.is_empty() {
                out.push(Ok(std::mem::take(&mut literal)));
            }
            out.push(Err(field));
            rest = &tail[end + 1..];
        }
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        out.push(Ok(literal));
    }
    Ok(out)
}

/// Checks a command without a file: `run` not empty, placeholders known, `{out}` only with
/// `output`, `output` a file name (no `/` or `\`, not `.` or `..`) with only `{name}` and
/// `{ext}`, `parallel` 1-16.
pub fn check_command(spec: &CommandSpec) -> Result<(), String> {
    if spec.run.first().is_none_or(|program| program.trim().is_empty()) {
        return Err("run has no program".to_owned());
    }
    for arg in &spec.run {
        for piece in pieces(arg)? {
            if piece == Err("out") && spec.output.is_none() {
                return Err("{out} needs an output".to_owned());
            }
        }
    }
    if let Some(output) = &spec.output {
        for piece in pieces(output)? {
            match piece {
                Ok(text) if text.contains(['/', '\\']) => {
                    return Err("output must be a file name, without / or \\".to_owned());
                }
                Err(field) if field != "name" && field != "ext" => {
                    return Err(format!("output can only use {{name}} and {{ext}}, not {{{field}}}"));
                }
                _ => {}
            }
        }
        if matches!(output.trim(), "" | "." | "..") {
            return Err(format!("output \"{output}\" is not a file name"));
        }
    }
    if !(1..=16).contains(&spec.parallel) {
        return Err(format!("parallel must be 1-16, not {}", spec.parallel));
    }
    Ok(())
}

/// The program and arguments for running `spec` on `input` (absolute), with `out` the
/// (temporary) output path. Placeholders are replaced inside each argument, which stays one
/// argument whatever the name holds: `{in}`, `{out}`, `{dir}` (the input's folder), `{name}`
/// (its name without the extension), `{ext}` (without the dot), `{outdir}` (the output's
/// folder, else the input's). For a folder (`is_dir`), `{name}` is its whole name and `{ext}`
/// is empty. An unknown placeholder, or `{out}` without `out`, is an error.
pub fn expand_command(
    spec: &CommandSpec,
    input: &Path,
    is_dir: bool,
    out: Option<&Path>,
) -> Result<Vec<OsString>, String> {
    debug_assert!(input.is_absolute(), "{} is not absolute", input.display());
    if spec.run.is_empty() {
        return Err("run has no program".to_owned());
    }
    let dir = input.parent().unwrap_or(Path::new(""));
    let outdir = out.and_then(Path::parent).unwrap_or(dir);
    spec.run
        .iter()
        .map(|arg| {
            let mut expanded = OsString::new();
            for piece in pieces(arg)? {
                match piece {
                    Ok(text) => expanded.push(text),
                    Err("in") => expanded.push(input),
                    Err("out") => expanded.push(out.ok_or("{out} needs an output")?),
                    Err("dir") => expanded.push(dir),
                    Err("name") => expanded.push(name_of(input, is_dir)),
                    Err("ext") => expanded.push(ext_of(input, is_dir)),
                    Err("outdir") => expanded.push(outdir),
                    Err(field) => unreachable!("pieces lets through only known placeholders, not {field}"),
                }
            }
            Ok(expanded)
        })
        .collect()
}

/// `{name}`: the name without its extension, or a folder's whole name.
fn name_of(input: &Path, is_dir: bool) -> &OsStr {
    if is_dir { input.file_name() } else { input.file_stem() }.unwrap_or_default()
}

/// `{ext}`: the extension without its dot; empty for a folder.
fn ext_of(input: &Path, is_dir: bool) -> &OsStr {
    if is_dir { None } else { input.extension() }.unwrap_or_default()
}

/// The output's file name for `input` (a folder when `is_dir`), from the `output` template
/// (`{name}`, `{ext}`). It must come out a plain file name: not empty, not `.` or `..`,
/// without `/` or `\`.
pub fn expand_output_name(template: &str, input: &Path, is_dir: bool) -> Result<OsString, String> {
    let mut name = OsString::new();
    for piece in pieces(template)? {
        match piece {
            Ok(text) => name.push(text),
            Err("name") => name.push(name_of(input, is_dir)),
            Err("ext") => name.push(ext_of(input, is_dir)),
            Err(field) => return Err(format!("output can only use {{name}} and {{ext}}, not {{{field}}}")),
        }
    }
    let text = name.to_string_lossy();
    if matches!(text.trim(), "" | "." | "..") || text.contains(['/', '\\']) {
        return Err(format!("output \"{text}\" is not a file name"));
    }
    Ok(name)
}

/// Whether `spec` runs on an item named `name`: folders when `folders`, files when `types`
/// is empty or holds the name's ending (any case).
pub fn command_applies(spec: &CommandSpec, name: &str, is_dir: bool) -> bool {
    if is_dir {
        return spec.folders;
    }
    if spec.types.is_empty() {
        return true;
    }
    let lower = name.to_lowercase();
    spec.types.iter().any(|t| {
        let t = t.trim().trim_start_matches('.').to_lowercase();
        !t.is_empty() && lower.len() > t.len() + 1 && lower.ends_with(&format!(".{t}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[OsString]) -> Vec<&str> {
        list.iter().map(|s| s.to_str().unwrap()).collect()
    }

    /// An absolute root on any platform, for paths the commands get.
    fn root() -> PathBuf {
        if cfg!(windows) { PathBuf::from("C:\\") } else { PathBuf::from("/") }
    }

    fn split(s: &str) -> Vec<&str> {
        s.split(' ').collect()
    }

    #[test]
    fn presets_are_the_spec_ones() {
        let labels: Vec<&str> = PRESETS.iter().map(|p| p.label).collect();
        assert_eq!(
            labels,
            [
                "Resize photos (JPEG 1920 px)",
                "Convert to JPEG",
                "Convert to PNG",
                "Convert to WebP",
                "Convert to AVIF",
                "Remove location data",
                "Convert to UTF-8",
                "Windows line endings (CRLF)",
                "Unix line endings (LF)",
                "MP4 (H.264 + AAC)",
                "Smaller video",
                "MP3",
                "M4A (AAC)",
                "WAV",
                "Remux to MP4",
                "GIF from video",
            ]
        );
        let mut ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len());
        for p in PRESETS {
            let kind = match p.what {
                PresetWhat::Image(_) => Kind::Image,
                PresetWhat::Text(_) => Kind::Text,
                PresetWhat::Media(_) => Kind::Media,
            };
            assert_eq!(p.kind, kind, "{}", p.id);
        }
    }

    #[test]
    fn preset_options() {
        let PresetWhat::Image(resize) = preset("resize-photos").unwrap().what else { panic!() };
        assert_eq!(resize.format, ImageFormat::Jpeg);
        assert_eq!(resize.quality, 85);
        assert_eq!(resize.resize, Resize::Longest(1920));
        assert!(resize.never_enlarge && resize.rotate_by_exif && !resize.strip_metadata);
        assert_eq!(resize.background, [255, 255, 255]);

        let PresetWhat::Image(location) = preset(REMOVE_LOCATION).unwrap().what else { panic!() };
        assert!(location.strip_metadata);
        assert_eq!(location.resize, Resize::None);
        assert!(location.rotate_by_exif);
        let remove = preset(REMOVE_LOCATION).unwrap();
        assert!(remove.keeps_format());
        assert_eq!(remove.format_for("a.JPG"), Some(ImageFormat::Jpeg));
        assert_eq!(remove.format_for("a.png"), Some(ImageFormat::Png));
        assert_eq!(remove.format_for("a.webp"), Some(ImageFormat::WebpLossless));
        assert_eq!(remove.format_for("a.avif"), Some(ImageFormat::Avif));
        assert_eq!(remove.format_for("a.gif"), None);
        assert!(!remove.needs_ffmpeg());
        assert!(!remove.needs_ffmpeg_for("a.jpg"));
        assert!(!remove.needs_ffmpeg_for("a.webp"));
        assert!(remove.needs_ffmpeg_for("a.avif"));
        assert!(remove.needs_ffmpeg_for("a.heic"));
        let png = preset("to-png").unwrap();
        assert!(!png.keeps_format());
        assert_eq!(png.format_for("a.webp"), Some(ImageFormat::Png));
        assert!(!png.needs_ffmpeg_for("a.jpg"));
        assert!(png.needs_ffmpeg_for("IMG.HEIC"));
        assert!(preset("to-avif").unwrap().needs_ffmpeg_for("a.png"));
        assert!(preset("mp3").unwrap().needs_ffmpeg_for("a.mp4"));
        assert!(!preset("to-utf8").unwrap().needs_ffmpeg_for("a.txt"));
        assert_eq!(preset("mp3").unwrap().format_for("a.png"), None);

        assert!(preset("to-avif").unwrap().needs_ffmpeg());
        assert!(!preset("to-webp").unwrap().needs_ffmpeg());
        assert!(preset("gif").unwrap().needs_ffmpeg());
        assert!(!preset("to-utf8").unwrap().needs_ffmpeg());

        let PresetWhat::Text(utf8) = preset("to-utf8").unwrap().what else { panic!() };
        let options = utf8.options();
        assert_eq!(options.to, "UTF-8");
        assert_eq!(options.from, None);
        assert_eq!(options.eol, Eol::Keep);
        assert!(!options.bom && !options.keeps_encoding());
        let PresetWhat::Text(crlf) = preset("eol-crlf").unwrap().what else { panic!() };
        assert!(crlf.options().keeps_encoding());
        assert_eq!(crlf.options().eol, Eol::Crlf);
        let PresetWhat::Text(lf) = preset("eol-lf").unwrap().what else { panic!() };
        assert_eq!(lf.options().eol, Eol::Lf);

        assert_eq!(preset("to-utf8").unwrap().default_output(), Output::ReplaceOriginal);
        assert_eq!(preset("to-png").unwrap().default_output(), Output::SameFolder);
        assert_eq!(preset("mp3").unwrap().default_output(), Output::SameFolder);
        assert_eq!(preset("nope"), None);
    }

    #[test]
    fn extensions() {
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::WebpLossy.extension(), "webp");
        assert_eq!(ImageFormat::WebpLossless.extension(), "webp");
        assert_eq!(ImageFormat::Avif.extension(), "avif");
        assert_eq!(MediaPreset::M4a.extension(), "m4a");
        assert_eq!(MediaPreset::Remux.extension(), "mp4");
        assert_eq!(MediaPreset::Gif.extension(), "gif");
        assert_eq!(ImageFormat::of_name("A.JPEG"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::of_name("a.png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::of_name("a.gif"), None);
        assert_eq!(ImageFormat::of_name("a.heic"), None);
    }

    #[test]
    fn target_sizes_from_the_notes() {
        let longest = Resize::Longest(1920);
        assert_eq!(target_size(4032, 3024, &longest, true), Some((1920, 1440)));
        assert_eq!(target_size(800, 600, &longest, true), None);
        assert_eq!(target_size(800, 600, &longest, false), Some((1920, 1440)));
        assert_eq!(target_size(800, 600, &Resize::Height(300), true), Some((400, 300)));
        assert_eq!(target_size(800, 600, &Resize::Percent(33), true), Some((264, 198)));
        assert_eq!(target_size(3024, 4032, &Resize::Width(1000), true), Some((1000, 1333)));
        // Portrait after rotation: the longest side is the height.
        assert_eq!(target_size(3024, 4032, &longest, true), Some((1440, 1920)));
    }

    #[test]
    fn target_size_edges() {
        assert_eq!(target_size(800, 600, &Resize::None, true), None);
        assert_eq!(target_size(800, 600, &Resize::Width(800), false), None);
        assert_eq!(target_size(800, 600, &Resize::Percent(100), false), None);
        assert_eq!(target_size(800, 600, &Resize::Percent(0), false), None);
        assert_eq!(target_size(800, 600, &Resize::Width(0), false), None);
        assert_eq!(target_size(0, 600, &Resize::Width(10), false), None);
        assert_eq!(target_size(10000, 1, &Resize::Width(100), true), Some((100, 1)));
        assert_eq!(target_size(800, 600, &Resize::Percent(200), true), None);
        assert_eq!(target_size(800, 600, &Resize::Percent(200), false), Some((1600, 1200)));
        // Capped at 65535 a side, keeping the aspect ratio.
        assert_eq!(target_size(40000, 20000, &Resize::Percent(200), false), Some((65535, 32768)));
        assert_eq!(target_size(1000, 10, &Resize::Height(1000), false), Some((65535, 655)));
        assert_eq!(target_size(65535, 100, &Resize::Width(70000), false), None);
        assert_eq!(MAX_SIDE, 65535);
    }

    #[test]
    fn output_names() {
        let dir = &root().join("photos");
        let png = dir.join("foto.png");
        let jpg = dir.join("foto.jpg");
        assert_eq!(output_path(&png, "webp", &Output::SameFolder), dir.join("foto.webp"));
        assert_eq!(output_path(&jpg, "jpg", &Output::SameFolder), dir.join("foto (converted).jpg"));
        assert_eq!(output_path(&dir.join("foto.JPG"), "jpg", &Output::SameFolder), dir.join("foto (converted).jpg"));
        assert_eq!(output_path(&jpg, "jpg", &Output::Subfolder), dir.join("converted").join("foto.jpg"));
        assert_eq!(output_path(&png, "webp", &Output::Subfolder), dir.join("converted").join("foto.webp"));
        let other = &root().join("out");
        assert_eq!(output_path(&jpg, "jpg", &Output::Folder(other.to_path_buf())), other.join("foto.jpg"));
        assert_eq!(output_path(&png, "jpg", &Output::Folder(other.to_path_buf())), other.join("foto.jpg"));
        assert_eq!(
            output_path(&jpg, "jpg", &Output::Folder(dir.to_path_buf())),
            dir.join("foto (converted).jpg"),
            "the input's own folder chosen"
        );
        let upper = root().join("PHOTOS");
        let expected = if cfg!(any(windows, target_os = "macos")) {
            upper.join("foto (converted).jpg")
        } else {
            upper.join("foto.jpg")
        };
        assert_eq!(output_path(&jpg, "jpg", &Output::Folder(upper.clone())), expected, "the same folder in other case");
        assert_eq!(
            output_path(&dir.join("foto.JPG"), "jpg", &Output::Folder(dir.to_path_buf())),
            if cfg!(any(windows, target_os = "macos")) {
                dir.join("foto (converted).jpg")
            } else {
                dir.join("foto.jpg")
            }
        );
        assert_eq!(output_path(&jpg, "jpg", &Output::ReplaceOriginal), jpg);
        assert_eq!(output_path(&png, "jpg", &Output::ReplaceOriginal), dir.join("foto.jpg"));
        // Names with more dots and without an extension.
        assert_eq!(output_path(&dir.join("a.b.png"), "jpg", &Output::SameFolder), dir.join("a.b.jpg"));
        assert_eq!(output_path(&dir.join("README"), "", &Output::SameFolder), dir.join("README (converted)"));
        assert_eq!(output_path(&dir.join("README"), "", &Output::ReplaceOriginal), dir.join("README"));
        assert_eq!(output_path(&dir.join("README"), "txt", &Output::SameFolder), dir.join("README.txt"));
    }

    #[test]
    fn input_kinds() {
        for name in ["a.png", "B.JPG", "c.jpeg", "d.gif", "e.webp", "f.bmp", "g.tif", "h.TIFF", "i.ico"] {
            assert!(image_inputs(name), "{name}");
            assert!(!needs_ffmpeg_to_read(name), "{name}");
        }
        for name in ["IMG_1.HEIC", "a.heif", "b.avif"] {
            assert!(image_inputs(name), "{name}");
            assert!(needs_ffmpeg_to_read(name), "{name}");
        }
        for name in ["a.txt", "png", ".png", "a.mp4", "a.pdf"] {
            assert!(!image_inputs(name), "{name}");
        }
        for name in ["a.mp4", "b.MOV", "c.mkv", "d.webm", "e.mp3", "f.flac", "g.wav", "h.m4a", "i.avi"] {
            assert!(media_inputs(name), "{name}");
        }
        for name in ["a.jpg", "b.txt", "mp4", "a.gif"] {
            assert!(!media_inputs(name), "{name}");
        }
    }

    #[test]
    fn avif_quality_map() {
        assert_eq!(avif_crf(85), 16);
        assert_eq!(avif_crf(50), 35);
        assert_eq!(avif_crf(60), 30);
        assert_eq!(avif_crf(100), 8);
        assert_eq!(avif_crf(1), 62);
        assert_eq!(avif_crf(0), 63);
        assert_eq!(avif_crf(11), 57); // 56.95
        assert_eq!(avif_crf(255), 8);
    }

    const IN: &str = "C:\\v\\in put.mov";
    const OUT: &str = "C:\\v\\.gezik-tmp-1";

    fn media(preset: MediaPreset) -> Vec<OsString> {
        ffmpeg_args(preset, Path::new(IN), Path::new(OUT))
    }

    fn expected(middle: &[&str], muxer: &str) -> Vec<String> {
        let mut list: Vec<String> = split("-nostdin -y -hide_banner -loglevel error -progress pipe:1 -nostats -i")
            .into_iter()
            .map(String::from)
            .collect();
        list.push(IN.to_owned());
        list.extend(middle.iter().map(|s| s.to_string()));
        list.extend(["-f".to_owned(), muxer.to_owned(), OUT.to_owned()]);
        list
    }

    #[test]
    fn media_preset_arguments() {
        let check = |preset, middle: &[&str], muxer| {
            assert_eq!(strings(&media(preset)), expected(middle, muxer), "{preset:?}");
        };
        check(
            MediaPreset::Mp4,
            &split("-c:v libx264 -crf 20 -preset medium -pix_fmt yuv420p -c:a aac -b:a 160k -movflags +faststart"),
            "mp4",
        );
        check(MediaPreset::Mp3, &split("-vn -c:a libmp3lame -b:a 192k"), "mp3");
        check(MediaPreset::M4a, &split("-vn -c:a aac -b:a 192k -movflags +faststart"), "ipod");
        check(MediaPreset::Wav, &split("-vn -c:a pcm_s16le -rf64 auto"), "wav");
        check(MediaPreset::Remux, &split("-c copy -movflags +faststart"), "mp4");
    }

    #[test]
    fn smaller_video_filter() {
        let mut middle = vec![
            "-vf",
            "scale='if(gte(iw,ih),min(1920,iw),min(1080,iw))':'if(gte(iw,ih),min(1080,ih),min(1920,ih))':force_original_aspect_ratio=decrease:force_divisible_by=2",
        ];
        middle.extend(split(
            "-c:v libx264 -crf 28 -preset medium -pix_fmt yuv420p -c:a aac -b:a 128k -movflags +faststart",
        ));
        assert_eq!(strings(&media(MediaPreset::Smaller)), expected(&middle, "mp4"));
    }

    #[test]
    fn gif_filter() {
        let middle = [
            "-an",
            "-filter_complex",
            "fps=12,scale='if(gte(iw,ih),min(480,iw),-2)':'if(gte(iw,ih),-2,min(480,ih))':flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
            "-loop",
            "0",
        ];
        assert_eq!(strings(&media(MediaPreset::Gif)), expected(&middle, "gif"));
    }

    #[test]
    fn special_paths_pass_whole() {
        let input = Path::new("C:\\v\\a \"b\"; $c & ş.mov");
        let list = ffmpeg_args(MediaPreset::Mp3, input, Path::new(OUT));
        assert!(list.iter().any(|a| a == input.as_os_str()));
        assert_eq!(list.last().unwrap(), OUT);
    }

    #[test]
    fn picture_arguments() {
        let (i, o) = (Path::new("C:\\p\\IMG.HEIC"), Path::new("C:\\p\\.gezik-tmp-2"));
        let head = "-nostdin -y -hide_banner -loglevel error -i C:\\p\\IMG.HEIC";
        let joined = |step| strings(&ffmpeg_image_args(step, i, o)).join(" ");
        assert_eq!(
            joined(FfmpegImage::DecodeToPng),
            format!("{head} -frames:v 1 -update 1 -c:v png -compression_level 1 -f image2 C:\\p\\.gezik-tmp-2")
        );
        assert_eq!(
            joined(FfmpegImage::WebpLossy { quality: 80 }),
            format!(
                "{head} -frames:v 1 -c:v libwebp -lossless 0 -quality 80 -compression_level 4 -f webp C:\\p\\.gezik-tmp-2"
            )
        );
        assert_eq!(
            joined(FfmpegImage::Avif { quality: 60, alpha: false }),
            format!(
                "{head} -frames:v 1 -c:v libaom-av1 -still-picture 1 -crf 30 -cpu-used 6 -row-mt 1 -pix_fmt yuv420p \
                 -f avif C:\\p\\.gezik-tmp-2"
            )
        );
        let alpha = ffmpeg_image_args(FfmpegImage::Avif { quality: 85, alpha: true }, i, o);
        let alpha = strings(&alpha);
        let at = alpha.iter().position(|a| *a == "-filter_complex").unwrap();
        assert_eq!(
            alpha[at + 1..at + 6],
            [
                "[0:v]format=yuva420p,split[c][a];[c]format=yuv420p[c2];[a]alphaextract[a2]",
                "-map",
                "[c2]",
                "-map",
                "[a2]"
            ]
        );
        assert_eq!(
            alpha[at + 6..].join(" "),
            "-frames:v 1 -c:v libaom-av1 -still-picture 1 -crf 16 -cpu-used 6 -row-mt 1 -f avif C:\\p\\.gezik-tmp-2"
        );
    }

    #[test]
    fn progress_blocks() {
        let block = [
            "frame=0",
            "fps=0.00",
            "bitrate=   0.2kbits/s",
            "total_size=48",
            "out_time_us=2461315",
            "out_time_ms=2461315",
            "out_time=00:00:02.461315",
            "speed=4.77x",
            "progress=continue",
        ];
        assert_eq!(parse_progress_block(&block), Progress { out_us: Some(2461315), end: false });
        let last = ["frame=180", "out_time_us=6000000", "out_time_ms=6000000", "progress=end"];
        assert_eq!(parse_progress_block(&last), Progress { out_us: Some(6000000), end: true });
        let early = ["out_time_us=N/A", "out_time_ms=N/A", "out_time=N/A", "progress=continue"];
        assert_eq!(parse_progress_block(&early), Progress { out_us: None, end: false });
        assert_eq!(parse_progress_block(&["out_time_us=-9223372036854775807"]).out_us, None);
        assert_eq!(
            parse_progress_block(&["out_time_us=5\r", "progress=end\r"]),
            Progress { out_us: Some(5), end: true }
        );
        assert_eq!(parse_progress_block(&[]), Progress::default());
    }

    #[test]
    fn ffmpeg_versions() {
        assert_eq!(
            parse_ffmpeg_version("ffmpeg version 9.0.2-essentials_build-www.gyan.dev Copyright (c) 2000-2026"),
            Some((9, 0))
        );
        assert_eq!(parse_ffmpeg_version("ffmpeg version 8.0.1-essentials_build-www.gyan.dev"), Some((8, 0)));
        assert_eq!(parse_ffmpeg_version("ffmpeg version 7.1.1 Copyright"), Some((7, 1)));
        assert_eq!(parse_ffmpeg_version("ffmpeg version n9.0.2-22-g46d8f462ee-20261001 Copyright"), Some((9, 0)));
        assert_eq!(parse_ffmpeg_version("ffmpeg version 6.1.1-3ubuntu5 Copyright"), Some((6, 1)));
        assert_eq!(parse_ffmpeg_version("ffmpeg version 10.12"), Some((10, 12)));
        assert_eq!(parse_ffmpeg_version("ffmpeg version 2025-05-19-git-c55d65ac0a-full_build-www.gyan.dev"), None);
        assert_eq!(parse_ffmpeg_version("ffmpeg version N-119621-g1a2b3c Copyright"), None);
        assert_eq!(parse_ffmpeg_version("ffprobe version 9.0.2"), None);
        assert_eq!(parse_ffmpeg_version("ffmpeg version 9"), None);
        assert_eq!(parse_ffmpeg_version(""), None);
    }

    fn command(run: &[&str], output: Option<&str>, types: &[&str]) -> CommandSpec {
        CommandSpec {
            name: "test".to_owned(),
            run: run.iter().map(|s| s.to_string()).collect(),
            output: output.map(str::to_owned),
            types: types.iter().map(|s| s.to_string()).collect(),
            folders: false,
            parallel: 1,
        }
    }

    #[test]
    fn special_names_stay_one_argument() {
        let spec = command(&["magick", "{in}", "-resize", "50%", "{out}"], Some("{name}-small.{ext}"), &[]);
        let input = root().join("pics").join("a \"b\"; $c & d ş.jpg");
        let out = root().join("pics").join(".gezik-tmp-3");
        let list = expand_command(&spec, &input, false, Some(&out)).unwrap();
        assert_eq!(list.len(), 5);
        assert_eq!(list[0], "magick");
        assert_eq!(list[1], input.as_os_str());
        assert!(list[1].to_str().unwrap().ends_with("a \"b\"; $c & d ş.jpg"));
        assert_eq!(list[3], "50%");
        assert_eq!(list[4], out.as_os_str());
    }

    #[test]
    fn all_placeholders() {
        let spec = command(
            &["tool", "--dir={dir}", "{name}.{ext}", "{outdir}", "x{{y}}z", "{in}{in}"],
            Some("{name}.pdf"),
            &[],
        );
        let docs = root().join("docs");
        let input = docs.join("report.final.docx");
        let out = root().join("out").join(".gezik-tmp-4");
        let list = expand_command(&spec, &input, false, Some(&out)).unwrap();
        let mut both = input.as_os_str().to_os_string();
        both.push(input.as_os_str());
        let mut dir_arg = OsString::from("--dir=");
        dir_arg.push(&docs);
        assert_eq!(list[1], dir_arg);
        assert_eq!(list[2], "report.final.docx");
        assert_eq!(list[3], root().join("out").as_os_str());
        assert_eq!(list[4], "x{y}z");
        assert_eq!(list[5], both);
        // Without an output, {outdir} is the input's folder.
        let in_place = command(&["tool", "{outdir}"], None, &[]);
        assert_eq!(expand_command(&in_place, &input, false, None).unwrap()[1], docs.as_os_str());
        assert_eq!(expand_output_name("{name}.pdf", &input, false).unwrap(), "report.final.pdf");
        assert_eq!(expand_output_name("{name}-small.{ext}", &input, false).unwrap(), "report.final-small.docx");
        assert!(expand_output_name("{in}.pdf", &input, false).is_err());
        for bad in ["", " ", ".", "..", "out/{name}.pdf", "..\\{name}.pdf", "{name}/x"] {
            assert!(expand_output_name(bad, &input, false).is_err(), "{bad}");
        }
        // An empty name from the input itself.
        assert!(expand_output_name("{ext}", &docs.join("noext"), false).is_err());
    }

    #[test]
    fn folder_placeholders() {
        let folder = root().join("work").join("site.v2");
        let spec = CommandSpec { folders: true, ..command(&["zip", "{name}|{ext}|{in}"], Some("{name}.zip"), &[]) };
        let list = expand_command(&spec, &folder, true, None).unwrap();
        let mut expected = OsString::from("site.v2||");
        expected.push(folder.as_os_str());
        assert_eq!(list[1], expected);
        assert_eq!(expand_output_name("{name}.zip", &folder, true).unwrap(), "site.v2.zip");
        assert_eq!(expand_output_name("{name}.{ext}", &folder, false).unwrap(), "site.v2");
    }

    #[test]
    fn bad_placeholders() {
        let input = &root().join("a.jpg");
        assert!(expand_command(&command(&["t", "{x}"], None, &[]), input, false, None).unwrap_err().contains("{x}"));
        assert!(expand_command(&command(&["t", "{in"], None, &[]), input, false, None).is_err());
        assert!(expand_command(&command(&["t", "a}b"], None, &[]), input, false, None).is_err());
        assert!(expand_command(&command(&["t", "{IN}"], None, &[]), input, false, None).is_err());
        assert!(expand_command(&command(&["t", "{out}"], None, &[]), input, false, None).is_err());
        assert!(expand_command(&command(&[], None, &[]), input, false, None).is_err());
    }

    #[test]
    fn command_checks() {
        assert_eq!(check_command(&command(&["magick", "{in}", "{out}"], Some("{name}-small.{ext}"), &[])), Ok(()));
        assert_eq!(check_command(&command(&["sh", "-c", "awk '{{print $1}}' {in}"], None, &[])), Ok(()));
        assert!(check_command(&command(&[], None, &[])).is_err());
        assert!(check_command(&command(&[" "], None, &[])).is_err());
        assert!(check_command(&command(&["t", "{out}"], None, &[])).is_err());
        assert!(check_command(&command(&["t", "{nope}"], None, &[])).is_err());
        assert!(check_command(&command(&["t"], Some("{dir}.pdf"), &[])).is_err());
        assert!(check_command(&command(&["t"], Some(""), &[])).is_err());
        for bad in [".", "..", " ", "sub/{name}.pdf", "..\\{name}.pdf"] {
            assert!(check_command(&command(&["t"], Some(bad), &[])).is_err(), "{bad}");
        }
        let mut spec = command(&["t"], None, &[]);
        spec.parallel = 0;
        assert!(check_command(&spec).is_err());
        spec.parallel = 17;
        assert!(check_command(&spec).is_err());
        spec.parallel = 16;
        assert_eq!(check_command(&spec), Ok(()));
    }

    #[test]
    fn which_items_a_command_takes() {
        let spec = command(&["t"], None, &["jpg", "JPEG", ".png", "tar.gz"]);
        assert!(command_applies(&spec, "a.jpg", false));
        assert!(command_applies(&spec, "A.JPG", false));
        assert!(command_applies(&spec, "b.jpeg", false));
        assert!(command_applies(&spec, "c.PNG", false));
        assert!(command_applies(&spec, "d.tar.gz", false));
        assert!(!command_applies(&spec, "e.gif", false));
        assert!(!command_applies(&spec, "jpg", false));
        assert!(!command_applies(&spec, ".jpg", false));
        assert!(!command_applies(&spec, "photos.jpg", true), "a folder needs folders = true");
        let any = command(&["t"], None, &[]);
        assert!(command_applies(&any, "anything", false));
        assert!(!command_applies(&any, "dir", true));
        let folders = CommandSpec { folders: true, ..any };
        assert!(command_applies(&folders, "dir", true));
        let unicode = command(&["t"], None, &["ŞEY"]);
        assert!(command_applies(&unicode, "a.şey", false));
    }
}
