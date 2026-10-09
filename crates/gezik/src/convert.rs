//! Converting in the app: the Convert…, Images to PDF… and Commands ▸ menu items, the Convert
//! layer with its PDF group (pdf.rs), and the offer to download ffmpeg or pdfium (archives.rs's
//! tool box) when a conversion needs it. The work runs as engine jobs (gezik-batch's
//! `ConvertTask`, `CommandTask`, `ImagesToPdfTask` and PDF chain); the UI thread never reads
//! the disk: menus decide by name, and finding programs, ffmpeg and pdfium, counting pages,
//! the trash check and the text detection run on threads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_batch::convert::ffmpeg::{Ffmpeg, find_ffmpeg};
use gezik_batch::convert::text::{Detected, SNIFF, detect, encodings};
use gezik_batch::pdf::client::{Worker, count_pages};
use gezik_batch::tasks::{
    CommandTask, ConvertTask, ConvertTools, ConvertWhat, ImagesToPdfTask, PdfTools, PdfWork, find_program,
    images_pdf_label, is_ffmpeg_needed, pdf_chain, pdf_label, skipped_inputs,
};
use gezik_config::Color;
use gezik_config::settings::{ConvertSettings, ConvertState};
use gezik_config::shortcuts::{Chord, Key, Platform as KeyPlatform};
use gezik_config::store::ConfigStore;
use gezik_core::batch::convert::{
    CommandSpec, Eol, ImageFormat, ImageOptions, Kind, Output, PRESETS, Preset, PresetWhat, Resize, TextOptions,
    command_applies, command_line_len, expand_files_command, image_inputs, media_inputs, needs_ffmpeg_to_read,
    too_many_text, uses_files,
};
use gezik_core::batch::pdf::{
    PageImage, PdfOp, RENDER_DPIS, Split, is_pdf, is_pdf_picture, ops_for, pictures_pdf_name,
};
use gezik_core::batch::tools::Tool;
use gezik_ops::{JobId, Report};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::archives::{Need, resolve_folder};
use crate::context_menu::{
    COMMAND_FIRST, COMMAND_GROUP, COMMAND_MAX, CONVERT, CONVERT_PRESET_FIRST, CONVERT_PRESET_MAX, ENCODING_FROM_FIRST,
    ENCODING_MAX, ENCODING_TO_FIRST, HEADING, IMAGES_TO_PDF,
};
use crate::dialog::Dialogs;
use crate::operations::{After, CANT_UNDO, Operations, items_text};
use crate::pdf::{
    MARGINS, MAX_EVERY, PdfChoices, PdfiumFailed, SIZES, SplitChoice, after_pdfium_failed, choices_from, choices_text,
    extract_note, files_note, move_in_order, pdf_counts, pdf_inputs, pdfium_explained, pictures_note,
    says_pdfium_failed, unknown_note, with_unread,
};
use crate::{AppWindow, ConvertView};

/// The oldest ffmpeg that reads HEIC, HEIF and AVIF right (gezik-batch's image.rs).
const FFMPEG_TO_READ: (u32, u32) = (9, 0);

/// How many text files the layer reads to say which encodings they are in.
const DETECT_MAX: usize = 200;

/// How many PDFs the layer counts the pages of (each count starts the PDF worker).
const COUNT_MAX: usize = 50;

/// Text files, by their ending.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt",
    "text",
    "csv",
    "tsv",
    "md",
    "markdown",
    "rst",
    "srt",
    "sub",
    "ssa",
    "ass",
    "vtt",
    "lrc",
    "log",
    "ini",
    "cfg",
    "conf",
    "config",
    "properties",
    "json",
    "xml",
    "yaml",
    "yml",
    "toml",
    "html",
    "htm",
    "xhtml",
    "css",
    "js",
    "mjs",
    "ts",
    "py",
    "rs",
    "c",
    "h",
    "cc",
    "cpp",
    "hpp",
    "cs",
    "java",
    "kt",
    "go",
    "rb",
    "php",
    "pl",
    "lua",
    "sh",
    "bash",
    "bat",
    "cmd",
    "ps1",
    "sql",
    "tex",
    "nfo",
    "diz",
    "m3u",
    "m3u8",
    "cue",
    "reg",
    "inf",
    "po",
    "asc",
    "rtf",
    "svg",
];

/// The picture formats the layer offers, as its buttons read ("¹ needs ffmpeg").
const FORMATS: [(ImageFormat, &str); 6] = [
    (ImageFormat::Jpeg, "JPEG"),
    (ImageFormat::Png, "PNG"),
    (ImageFormat::WebpLossless, "WebP"),
    (ImageFormat::WebpLossy, "WebP (lossy) ¹"),
    (ImageFormat::Avif, "AVIF ¹"),
    (ImageFormat::Bmp, "BMP"),
];

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn extension_lower(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Whether `name` is a text file by its ending.
pub fn looks_text(name: &str) -> bool {
    extension_lower(name).is_some_and(|ext| TEXT_EXTENSIONS.contains(&ext.as_str()))
}

/// The preset group an item belongs to by its name; `None` for folders and other files.
fn kind_of(name: &str, is_dir: bool) -> Option<Kind> {
    if is_dir {
        None
    } else if image_inputs(name) {
        Some(Kind::Image)
    } else if media_inputs(name) {
        Some(Kind::Media)
    } else if looks_text(name) {
        Some(Kind::Text)
    } else {
        None
    }
}

/// The preset groups for `items` (path, is a folder), the kind most of them are first (on a
/// tie: pictures, text, audio/video). The PDF group comes when there is a PDF operation for
/// the pictures and PDFs: first when PDFs are the most, else after the pictures' group.
pub fn kinds_for(items: &[(PathBuf, bool)]) -> Vec<Kind> {
    let mut counts = [(Kind::Image, 0usize), (Kind::Text, 0), (Kind::Media, 0)];
    for (path, is_dir) in items {
        if let Some(kind) = kind_of(&name_of(path), *is_dir) {
            for (k, n) in &mut counts {
                if *k == kind {
                    *n += 1;
                }
            }
        }
    }
    let most = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
    // A stable sort keeps the tie order.
    counts.sort_by_key(|a| std::cmp::Reverse(a.1));
    let mut kinds: Vec<Kind> = counts.into_iter().filter(|(_, n)| *n > 0).map(|(k, _)| k).collect();
    let (pictures, pdfs) = pdf_counts(items);
    if !ops_for(pictures, pdfs).is_empty() {
        let at =
            if pdfs > most { 0 } else { kinds.iter().position(|k| *k == Kind::Image).map_or(kinds.len(), |i| i + 1) };
        kinds.insert(at, Kind::Pdf);
    }
    kinds
}

/// The PDF group's operations for `items`.
pub fn pdf_ops_for(items: &[(PathBuf, bool)]) -> Vec<PdfOp> {
    let (pictures, pdfs) = pdf_counts(items);
    ops_for(pictures, pdfs)
}

/// How a user command shows for a selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandState {
    /// It takes none of the items.
    Hidden,
    Ready,
    /// Greyed, saying why.
    Off(String),
}

/// The program a command runs (`run[0]`).
fn program_of(spec: &CommandSpec) -> String {
    spec.run.first().map(|p| p.trim().to_owned()).unwrap_or_default()
}

/// The tip for a program not found. On Windows a bare name is looked up with `.exe` and
/// `.com` only, so a batch file needs its ending.
pub fn not_found_tip(program: &str, windows: bool) -> String {
    let bare = Path::new(program).extension().is_none() && !program.contains(['/', '\\']);
    if windows && bare {
        format!("{program} not found (use the full name, e.g. {program}.cmd)")
    } else {
        format!("{program} not found")
    }
}

/// How `spec` shows for `items`: hidden when it takes none, greyed when its program was not
/// found (`found`: `None` while not looked up yet) or when it would change a folder in place
/// (which `CommandTask` refuses).
pub fn command_state(
    spec: &CommandSpec,
    items: &[(PathBuf, bool)],
    found: Option<bool>,
    windows: bool,
) -> CommandState {
    let taken: Vec<&(PathBuf, bool)> =
        items.iter().filter(|(path, is_dir)| command_applies(spec, &name_of(path), *is_dir)).collect();
    if taken.is_empty() {
        return CommandState::Hidden;
    }
    // A `{files}` command does not change the folders in place: it takes them as they are.
    if spec.output.is_none() && !uses_files(spec) && taken.iter().any(|(_, is_dir)| *is_dir) {
        return CommandState::Off("changes items in place: not for folders".to_owned());
    }
    if found == Some(false) {
        return CommandState::Off(not_found_tip(&program_of(spec), windows));
    }
    CommandState::Ready
}

/// The most characters of a command's or a group's name a menu shows.
pub const MENU_NAME_MAX: usize = 60;

/// `name` as a menu shows it: cut to [`MENU_NAME_MAX`] characters, ending in "…".
pub fn shown_name(name: &str) -> String {
    if name.chars().count() <= MENU_NAME_MAX {
        return name.to_owned();
    }
    let mut out: String = name.chars().take(MENU_NAME_MAX - 1).collect();
    out.push('…');
    out
}

/// A command's menu title and whether it can be chosen.
fn command_title(spec: &CommandSpec, state: &CommandState) -> (String, bool) {
    let name = shown_name(&spec.name);
    match state {
        CommandState::Off(tip) => (format!("{name} — {tip}"), false),
        _ => (name, true),
    }
}

/// A line of a commands menu: a group's heading or a command (by its index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line<'a> {
    Heading(&'a str),
    Command(usize),
}

/// The commands `shown` (indices into `commands`) in menu order: those without a `menu`
/// first, then each group under its heading, groups in the order they first appear.
fn grouped<'a>(commands: &'a [CommandSpec], shown: &[usize]) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> =
        shown.iter().filter(|i| commands[**i].menu.is_none()).map(|i| Line::Command(*i)).collect();
    let mut groups: Vec<&str> = Vec::new();
    for i in shown {
        if let Some(group) = commands[*i].menu.as_deref()
            && !groups.contains(&group)
        {
            groups.push(group);
        }
    }
    for group in groups {
        lines.push(Line::Heading(group));
        lines.extend(shown.iter().filter(|i| commands[**i].menu.as_deref() == Some(group)).map(|i| Line::Command(*i)));
    }
    lines
}

/// Why `spec` runs on none of the items it was asked to (spec 7): by its endings, else
/// because they are folders.
pub fn not_for_text(spec: &CommandSpec) -> String {
    let endings: Vec<String> = spec.types.iter().map(|t| format!(".{}", t.to_lowercase())).collect();
    let list = match endings.as_slice() {
        [] => return format!("{} does not run on folders", spec.name),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let what = if spec.folders { "files and folders" } else { "files" };
    format!("{} runs on {list} {what} only", spec.name)
}

/// The question of a command with `ask = true`.
pub fn ask_text(name: &str, count: usize) -> String {
    if count == 1 {
        format!("Run {name} on 1 item?")
    } else {
        format!("Run {name} on {} items?", crate::preview::with_commas(count))
    }
}

/// The macOS menu bar's Commands menu: the commands with a key (spec 8.3), grouped as in
/// "Commands ▸", each "<name>    <key>" by its index; headings greyed with id -1.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn bar_entries(commands: &[CommandSpec], key_of: impl Fn(usize) -> Option<String>) -> Vec<(i32, String, bool)> {
    let shown: Vec<usize> = (0..commands.len()).filter(|i| key_of(*i).is_some()).collect();
    grouped(commands, &shown)
        .into_iter()
        .map(|line| match line {
            Line::Heading(group) => (-1, shown_name(group), false),
            Line::Command(i) => {
                let key = key_of(i).unwrap_or_default();
                (i32::try_from(i).unwrap_or(-1), format!("{}    {key}", shown_name(&commands[i].name)), true)
            }
        })
        .collect()
}

/// A menu's top items (id, title) and its "Commands ▸" items (id, title, enabled), if any.
pub type MenuItems = (Vec<(u32, String)>, Option<Vec<(u32, String, bool)>>);

/// "Convert…" and the "Commands ▸" items for `items`, given the commands and how each shows.
/// No "Convert…" when nothing can be converted or run; no submenu without commands to show.
pub fn menu_entries(items: &[(PathBuf, bool)], commands: &[CommandSpec], states: &[CommandState]) -> MenuItems {
    let shown: Vec<usize> = (0..commands.len().min(states.len()))
        .take(COMMAND_MAX as usize)
        .filter(|i| states[*i] != CommandState::Hidden)
        .collect();
    let sub: Vec<(u32, String, bool)> = grouped(commands, &shown)
        .into_iter()
        .map(|line| match line {
            Line::Heading(group) => (COMMAND_GROUP, shown_name(group), false),
            Line::Command(i) => {
                let (title, enabled) = command_title(&commands[i], &states[i]);
                (COMMAND_FIRST + i as u32, title, enabled)
            }
        })
        .collect();
    let convertible = items.iter().any(|(path, is_dir)| {
        let name = name_of(path);
        kind_of(&name, *is_dir).is_some() || (!is_dir && is_pdf(&name))
    });
    // A greyed command alone gives the layer nothing to run.
    let runnable = sub.iter().any(|(_, _, enabled)| *enabled);
    let mut top = Vec::new();
    if convertible || runnable {
        top.push((CONVERT, "Convert…".to_owned()));
    }
    if items.iter().any(|(path, is_dir)| !is_dir && is_pdf_picture(&name_of(path))) {
        top.push((IMAGES_TO_PDF, "Images to PDF…".to_owned()));
    }
    (top, (!sub.is_empty()).then_some(sub))
}

/// One entry of the layer's preset list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Preset(&'static Preset),
    /// A user command, by its index in the layer's commands.
    Command(usize),
    /// One of the PDF group's operations.
    Pdf(PdfOp),
}

impl Choice {
    fn kind(self) -> Kind {
        match self {
            Choice::Preset(preset) => preset.kind,
            Choice::Command(_) => Kind::Command,
            Choice::Pdf(_) => Kind::Pdf,
        }
    }

    /// How state.toml names it: a preset's id, `command:<name>`, `pdf:<id>`.
    fn key(self, commands: &[CommandSpec]) -> String {
        match self {
            Choice::Preset(preset) => preset.id.to_owned(),
            Choice::Command(i) => format!("command:{}", commands.get(i).map_or("", |c| c.name.as_str())),
            Choice::Pdf(op) => format!("pdf:{}", op.id()),
        }
    }
}

fn group_title(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "Image",
        Kind::Text => "Text",
        Kind::Media => "Audio/Video",
        Kind::Command => "Commands",
        Kind::Pdf => "PDF",
    }
}

/// A line of the layer's preset list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A group's name.
    Heading(&'static str),
    /// The next choice: its title and whether it can be chosen.
    Choice(String, bool),
}

/// The layer's preset list for the groups `kinds`, the PDF operations `pdf_ops` (the PDF
/// group's lines) and the commands with their states: the choices in order, and the lines of
/// its menu.
pub fn preset_list(
    kinds: &[Kind],
    pdf_ops: &[PdfOp],
    commands: &[CommandSpec],
    states: &[CommandState],
) -> (Vec<Choice>, Vec<Row>) {
    let mut choices = Vec::new();
    let mut rows = Vec::new();
    for kind in kinds {
        if *kind == Kind::Pdf {
            if !pdf_ops.is_empty() {
                rows.push(Row::Heading(group_title(Kind::Pdf)));
            }
            for op in pdf_ops {
                choices.push(Choice::Pdf(*op));
                rows.push(Row::Choice(op.label().to_owned(), true));
            }
            continue;
        }
        rows.push(Row::Heading(group_title(*kind)));
        for preset in PRESETS.iter().filter(|p| p.kind == *kind) {
            choices.push(Choice::Preset(preset));
            rows.push(Row::Choice(preset.label.to_owned(), true));
        }
    }
    // The commands that fit after the other choices: each choice has its menu id.
    let room = (CONVERT_PRESET_MAX as usize).saturating_sub(choices.len());
    let shown: Vec<usize> =
        (0..commands.len().min(states.len())).filter(|i| states[*i] != CommandState::Hidden).take(room).collect();
    if !shown.is_empty() {
        rows.push(Row::Heading(group_title(Kind::Command)));
        for i in shown {
            let (title, enabled) = command_title(&commands[i], &states[i]);
            choices.push(Choice::Command(i));
            rows.push(Row::Choice(title, enabled));
        }
    }
    (choices, rows)
}

/// The menu of `rows` (from [`preset_list`]): headings greyed with [`HEADING`], choices with
/// their index after [`CONVERT_PRESET_FIRST`]; the current choice is marked.
pub fn preset_menu(rows: &[Row], current: usize) -> Vec<(u32, String, bool)> {
    let mut index = 0;
    let mut out = Vec::new();
    for row in rows {
        match row {
            Row::Heading(title) => out.push((HEADING, (*title).to_owned(), false)),
            Row::Choice(title, enabled) => {
                if index < CONVERT_PRESET_MAX as usize {
                    let mark = if index == current { "• " } else { "    " };
                    out.push((CONVERT_PRESET_FIRST + index as u32, format!("{mark}{title}"), *enabled));
                }
                index += 1;
            }
        }
    }
    out
}

/// Whether each choice of `rows` can be chosen.
fn enabled_of(rows: &[Row]) -> Vec<bool> {
    rows.iter()
        .filter_map(|row| match row {
            Row::Choice(_, enabled) => Some(*enabled),
            Row::Heading(_) => None,
        })
        .collect()
}

/// The choice to start with: the one used last if it is there and can be chosen, else the
/// first that can.
fn first_choice(
    choices: &[Choice],
    enabled: &[bool],
    commands: &[CommandSpec],
    wanted: &[impl AsRef<str>],
) -> Option<usize> {
    let usable = |i: &usize| enabled.get(*i).copied().unwrap_or(false);
    wanted
        .iter()
        .find_map(|want| (0..choices.len()).filter(usable).find(|i| choices[*i].key(commands) == want.as_ref()))
        .or_else(|| (0..choices.len()).find(usable))
}

/// The preset a kind's saved options were for (`preset=<id>` in them).
pub fn saved_preset(text: &str) -> Option<&str> {
    text.split_whitespace().find_map(|pair| pair.strip_prefix("preset="))
}

/// The presets to start with, best first: the one used last, then the one used last for
/// each kind of the selection (`kinds`, the most common first), so that a picture after a
/// text conversion gets the last picture preset.
pub fn wanted_presets(state: &ConvertState, kinds: &[Kind]) -> Vec<String> {
    let mut out: Vec<String> = state.last_preset.iter().cloned().collect();
    for kind in kinds {
        let remembered = match kind {
            Kind::Image => state.image.as_deref().and_then(saved_preset).map(str::to_owned),
            Kind::Text => state.text.as_deref().and_then(saved_preset).map(str::to_owned),
            Kind::Media => state.media.clone(),
            Kind::Pdf => state.pdf.as_deref().map(|text| Choice::Pdf(saved_pdf(text).op).key(&[])),
            Kind::Command => None,
        };
        out.extend(remembered);
    }
    out
}

/// The PDF group's choices saved in state.toml (the defaults for what is not saved).
fn saved_pdf(text: &str) -> PdfChoices {
    choices_from(text, PdfChoices::DEFAULT)
}

/// Whether the options differ from those `preset` starts with (the Preset button then says so).
pub fn options_changed(preset: &Preset, image: &ImageOptions, text: &TextOptions) -> bool {
    match preset.what {
        PresetWhat::Image(_) if preset.keeps_format() => false,
        PresetWhat::Image(own) => {
            let shown_quality = matches!(image.format, ImageFormat::Jpeg | ImageFormat::WebpLossy | ImageFormat::Avif);
            image.format != own.format
                || (shown_quality && image.quality != own.quality)
                || image.resize != own.resize
                || (image.resize != Resize::None && image.never_enlarge != own.never_enlarge)
                || image.rotate_by_exif != own.rotate_by_exif
                || image.strip_metadata != own.strip_metadata
                || (image.format == ImageFormat::Jpeg && image.background != own.background)
        }
        PresetWhat::Text(own) => *text != own.options(),
        PresetWhat::Media(_) => false,
    }
}

/// What a preset converts, with the layer's options.
pub fn convert_what(preset: &Preset, image: ImageOptions, text: &TextOptions) -> ConvertWhat {
    match preset.what {
        PresetWhat::Image(options) if preset.keeps_format() => ConvertWhat::RemoveLocation(options),
        PresetWhat::Image(_) => ConvertWhat::Image(image),
        PresetWhat::Text(_) => ConvertWhat::Text(text.clone()),
        PresetWhat::Media(media) => ConvertWhat::Media(media),
    }
}

/// The items `what` converts, and how many it leaves out. Text takes text files by their
/// ending (others would be passed over as binary anyway).
pub fn split_inputs(items: &[(PathBuf, bool)], what: &ConvertWhat) -> (Vec<PathBuf>, usize) {
    if let ConvertWhat::Text(_) = what {
        let taken: Vec<PathBuf> = items
            .iter()
            .filter(|(path, is_dir)| !is_dir && looks_text(&name_of(path)))
            .map(|(p, _)| p.clone())
            .collect();
        let left = items.len() - taken.len();
        return (taken, left);
    }
    let skipped = skipped_inputs(items, what);
    let taken: Vec<PathBuf> =
        items.iter().filter(|(path, _)| !skipped.contains(path)).map(|(path, _)| path.clone()).collect();
    (taken, skipped.len())
}

/// The footer's "3 items skipped: not images" (`what`: none for a command); empty for none.
pub fn skipped_text(count: usize, what: Option<&ConvertWhat>) -> String {
    let (many, one) = match what {
        Some(ConvertWhat::Image(_)) => ("not images", "not an image"),
        Some(ConvertWhat::RemoveLocation(_)) => ("not pictures Gezik can write", "not a picture Gezik can write"),
        Some(ConvertWhat::Text(_)) => ("not text files", "not a text file"),
        Some(ConvertWhat::Media(_)) => ("not audio or video", "not audio or video"),
        None => ("not for this command", "not for this command"),
    };
    match count {
        0 => String::new(),
        1 => format!("1 item skipped: {one}"),
        n => format!("{n} items skipped: {many}"),
    }
}

/// What the layer knows about ffmpeg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Not looked for yet.
    Unknown,
    Missing,
    /// There, with its version (`None`: a build from git, version unknown).
    Version(Option<(u32, u32)>),
}

/// The footer's ffmpeg note: "¹ needs ffmpeg" under the picture formats it marks, "Needs
/// ffmpeg" for audio and video; none once an ffmpeg was found.
pub fn ffmpeg_note(kind: Kind, keeps_format: bool, found: Found) -> &'static str {
    if matches!(found, Found::Version(_)) {
        return "";
    }
    match kind {
        Kind::Image if !keeps_format => "¹ needs ffmpeg",
        Kind::Media if found == Found::Missing => "Needs ffmpeg",
        _ => "",
    }
}

/// An ffmpeg that was found: its version (`None`: a build from git, version unknown) and
/// whether it is the one set in settings.toml (`[convert] ffmpeg`, used before any other).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Have {
    pub version: Option<(u32, u32)>,
    pub configured: bool,
}

/// Whether converting `name` as `what` says needs ffmpeg: audio and video always, pictures
/// read through it (HEIC, HEIF, AVIF) or written by it (lossy WebP, AVIF). It goes by the
/// options chosen, not by the preset they started from.
pub fn needs_ffmpeg(what: &ConvertWhat, name: &str) -> bool {
    match what {
        ConvertWhat::Image(options) => needs_ffmpeg_to_read(name) || options.format.needs_ffmpeg(),
        // Each picture keeps its format, a WebP written lossless.
        ConvertWhat::RemoveLocation(_) => {
            needs_ffmpeg_to_read(name) || ImageFormat::of_name(name) == Some(ImageFormat::Avif)
        }
        ConvertWhat::Text(_) => false,
        ConvertWhat::Media(_) => true,
    }
}

/// Whether `what` can run on `name` with ffmpeg as `found` (`None`: there is none).
fn runs_with(what: &ConvertWhat, name: &str, found: Option<Have>) -> bool {
    if !needs_ffmpeg(what, name) {
        return true;
    }
    match found {
        None => false,
        Some(have) => !needs_ffmpeg_to_read(name) || have.version.is_some_and(|v| v >= FFMPEG_TO_READ),
    }
}

/// What the box says `what` needs for `inputs`, with ffmpeg as `found`.
pub fn need_for(what: &ConvertWhat, inputs: &[PathBuf], found: Option<Have>) -> Need {
    if matches!(what, ConvertWhat::Media(_)) {
        Need::Media
    } else if inputs.iter().any(|p| needs_ffmpeg_to_read(&name_of(p))) {
        match found {
            None => Need::Heic,
            // Recent enough, and it still failed to read them (after it was tried again).
            Some(Have { version: Some(v), .. }) if v >= FFMPEG_TO_READ => Need::FfmpegFailed,
            // A download would not be used: the configured one comes first.
            Some(Have { configured: true, version: Some(_) }) => Need::ConfiguredTooOld,
            Some(Have { configured: true, version: None }) => Need::ConfiguredUnknown,
            Some(Have { version: Some(_), .. }) => Need::NewerFfmpeg,
            // A build from git: its version is unknown, not older.
            Some(Have { version: None, .. }) => Need::UnknownFfmpeg,
        }
    } else {
        Need::Pictures
    }
}

/// `inputs` split into those that run now and those that wait for ffmpeg (none, or older
/// than 9 for HEIC, HEIF and AVIF), with what those need.
pub fn split_by_ffmpeg(
    what: &ConvertWhat,
    inputs: Vec<PathBuf>,
    found: Option<Have>,
) -> (Vec<PathBuf>, Vec<PathBuf>, Option<Need>) {
    let (now, waiting): (Vec<PathBuf>, Vec<PathBuf>) =
        inputs.into_iter().partition(|path| runs_with(what, &name_of(path), found));
    let need = (!waiting.is_empty()).then(|| need_for(what, &waiting, found));
    (now, waiting, need)
}

/// "Detected: Windows-1254 (11 files), UTF-8 (1 file)", the most common first; empty for none.
pub fn detected_text(counts: &[(&str, usize)]) -> String {
    let mut counts: Vec<(&str, usize)> = counts.iter().filter(|(_, n)| *n > 0).copied().collect();
    if counts.is_empty() {
        return String::new();
    }
    counts.sort_by_key(|a| std::cmp::Reverse(a.1));
    let parts: Vec<String> = counts
        .iter()
        .map(|(name, n)| if *n == 1 { format!("{name} (1 file)") } else { format!("{name} ({n} files)") })
        .collect();
    format!("Detected: {}", parts.join(", "))
}

/// Where outputs go, as the layer offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputChoice {
    Same,
    Subfolder,
    Folder,
    Replace,
}

const OUTPUTS: [(OutputChoice, &str); 4] = [
    (OutputChoice::Same, "same"),
    (OutputChoice::Subfolder, "subfolder"),
    (OutputChoice::Folder, "folder"),
    (OutputChoice::Replace, "replace"),
];

impl OutputChoice {
    fn index(self) -> i32 {
        OUTPUTS.iter().position(|(o, _)| *o == self).unwrap_or(0) as i32
    }

    fn key(self) -> &'static str {
        OUTPUTS.iter().find(|(o, _)| *o == self).map_or("same", |(_, key)| key)
    }

    fn from_key(key: &str) -> Option<OutputChoice> {
        OUTPUTS.iter().find(|(_, k)| *k == key).map(|(o, _)| *o)
    }

    fn of(output: &Output) -> OutputChoice {
        match output {
            Output::SameFolder => OutputChoice::Same,
            Output::Subfolder => OutputChoice::Subfolder,
            Output::Folder(_) => OutputChoice::Folder,
            Output::ReplaceOriginal => OutputChoice::Replace,
        }
    }
}

fn format_key(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Png => "png",
        ImageFormat::WebpLossless => "webp",
        ImageFormat::WebpLossy => "webp-lossy",
        ImageFormat::Avif => "avif",
        ImageFormat::Bmp => "bmp",
    }
}

fn yes(on: bool) -> &'static str {
    if on { "yes" } else { "no" }
}

/// The picture options as state.toml keeps them: `format=jpeg quality=85 resize=longest:1920
/// never-enlarge=yes rotate=yes strip=no background=#ffffff`.
pub fn image_options_text(o: &ImageOptions) -> String {
    let resize = match o.resize {
        Resize::None => "none".to_owned(),
        Resize::Longest(n) => format!("longest:{n}"),
        Resize::Width(n) => format!("width:{n}"),
        Resize::Height(n) => format!("height:{n}"),
        Resize::Percent(n) => format!("percent:{n}"),
    };
    let [r, g, b] = o.background;
    format!(
        "format={} quality={} resize={resize} never-enlarge={} rotate={} strip={} background=#{r:02x}{g:02x}{b:02x}",
        format_key(o.format),
        o.quality,
        yes(o.never_enlarge),
        yes(o.rotate_by_exif),
        yes(o.strip_metadata),
    )
}

/// The picture options read back from [`image_options_text`]; what is missing or wrong stays
/// as in `base`.
pub fn image_options_from(text: &str, base: ImageOptions) -> ImageOptions {
    let mut o = base;
    for (key, value) in text.split_whitespace().filter_map(|pair| pair.split_once('=')) {
        match key {
            "format" => {
                if let Some((format, _)) = FORMATS.iter().find(|(f, _)| format_key(*f) == value) {
                    o.format = *format;
                }
            }
            "quality" => {
                if let Some(q) = value.parse::<u8>().ok().filter(|q| (1..=100).contains(q)) {
                    o.quality = q;
                }
            }
            "resize" => {
                let (mode, n) = value.split_once(':').unwrap_or((value, ""));
                let n = n.parse::<u32>().ok().filter(|n| *n > 0);
                o.resize = match (mode, n) {
                    ("none", _) => Resize::None,
                    ("longest", Some(n)) => Resize::Longest(n),
                    ("width", Some(n)) => Resize::Width(n),
                    ("height", Some(n)) => Resize::Height(n),
                    ("percent", Some(n)) => Resize::Percent(n),
                    _ => o.resize,
                };
            }
            "never-enlarge" => o.never_enlarge = value == "yes",
            "rotate" => o.rotate_by_exif = value == "yes",
            "strip" => o.strip_metadata = value == "yes",
            "background" => {
                if let Some(c) = Color::parse(value) {
                    o.background = [c.r, c.g, c.b];
                }
            }
            _ => {}
        }
    }
    o
}

const EOLS: [(Eol, &str); 4] = [(Eol::Keep, "keep"), (Eol::Lf, "lf"), (Eol::Crlf, "crlf"), (Eol::Cr, "cr")];

/// The text options as state.toml keeps them: `from=detect to=UTF-8 bom=no eol=keep trim=no
/// final-newline=no` (`to=keep`: each file's own).
pub fn text_options_text(o: &TextOptions) -> String {
    let eol = EOLS.iter().find(|(e, _)| *e == o.eol).map_or("keep", |(_, k)| k);
    format!(
        "from={} to={} bom={} eol={eol} trim={} final-newline={}",
        o.from.as_deref().unwrap_or("detect"),
        if o.to.is_empty() { "keep" } else { &o.to },
        yes(o.bom),
        yes(o.trim_trailing),
        yes(o.final_newline),
    )
}

/// The text options read back from [`text_options_text`]; unknown encodings and what is
/// missing stay as in `base`.
pub fn text_options_from(text: &str, base: TextOptions) -> TextOptions {
    let mut o = base;
    let known = |label: &str| encodings().iter().any(|(l, _)| *l == label);
    for (key, value) in text.split_whitespace().filter_map(|pair| pair.split_once('=')) {
        match key {
            "from" if value == "detect" => o.from = None,
            "from" if known(value) => o.from = Some(value.to_owned()),
            "to" if value == "keep" => o.to = String::new(),
            "to" if known(value) => o.to = value.to_owned(),
            "bom" => o.bom = value == "yes",
            "eol" => {
                if let Some((eol, _)) = EOLS.iter().find(|(_, k)| *k == value) {
                    o.eol = *eol;
                }
            }
            "trim" => o.trim_trailing = value == "yes",
            "final-newline" => o.final_newline = value == "yes",
            _ => {}
        }
    }
    o
}

const RESIZE_MODES: usize = 5;

/// The resize buttons' index (0 none, 1 longest side, 2 width, 3 height, 4 percent) and the
/// field's text for `resize`.
fn resize_choice(resize: Resize) -> (usize, String) {
    match resize {
        Resize::None => (0, String::new()),
        Resize::Longest(n) => (1, n.to_string()),
        Resize::Width(n) => (2, n.to_string()),
        Resize::Height(n) => (3, n.to_string()),
        Resize::Percent(n) => (4, n.to_string()),
    }
}

/// The resize for button `mode` and the typed `value`.
pub fn resize_from(mode: usize, value: &str) -> Result<Resize, String> {
    if mode == 0 {
        return Ok(Resize::None);
    }
    let n = value.trim().parse::<u32>().ok().filter(|n| *n > 0 && *n <= gezik_core::batch::convert::MAX_SIDE);
    let Some(n) = n else {
        return Err(if mode == 4 { "Type the size in percent" } else { "Type the size in pixels" }.to_owned());
    };
    Ok(match mode {
        1 => Resize::Longest(n),
        2 => Resize::Width(n),
        3 => Resize::Height(n),
        _ => Resize::Percent(n),
    })
}

/// The quality typed (1-100).
pub fn quality_from(value: &str) -> Result<u8, String> {
    value
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|q| (1..=100).contains(q))
        .ok_or_else(|| "Type a quality from 1 to 100".to_owned())
}

/// The background colour typed (`#rrggbb`).
pub fn background_from(value: &str) -> Result<[u8; 3], String> {
    let value = value.trim();
    match Color::parse(value) {
        Some(c) if value.len() == 7 => Ok([c.r, c.g, c.b]),
        _ => Err("Type the background as #rrggbb".to_owned()),
    }
}

fn background_text([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The picture options a preset starts with.
fn preset_image(preset: &Preset) -> ImageOptions {
    match preset.what {
        PresetWhat::Image(options) => options,
        _ => ImageOptions::DEFAULT,
    }
}

/// The text options a preset starts with.
fn preset_text(preset: &Preset) -> TextOptions {
    match preset.what {
        PresetWhat::Text(text) => text.options(),
        _ => TextOptions {
            from: None,
            to: "UTF-8".to_owned(),
            bom: false,
            eol: Eol::Keep,
            trim_trailing: false,
            final_newline: false,
        },
    }
}

/// An encoding's name in the layer ("Windows-1254 (ISO-8859-9)").
fn encoding_shown(label: &str) -> String {
    encodings().into_iter().find(|(l, _)| *l == label).map_or_else(|| label.to_owned(), |(_, shown)| shown.to_owned())
}

/// What the worker said of a PDF's pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Count {
    Pages(u32),
    /// It needs a password (the count asks none).
    Encrypted,
    /// It could not be opened (damaged, or pdfium did not load).
    Failed,
}

/// A PDF job to start: pictures into one PDF (in this process), or PDF work (the worker).
#[derive(Clone)]
enum PdfJob {
    Pictures {
        pictures: Vec<PathBuf>,
        output: PathBuf,
        page: gezik_core::batch::pdf::PageOptions,
    },
    /// `resubmitted`: started again after pdfium was downloaded again because it did not
    /// load; a second such failure gets no more downloads.
    Pdfs {
        work: PdfWork,
        inputs: Vec<PathBuf>,
        resubmitted: bool,
    },
}

/// "Every [N] pages" as typed.
pub fn every_from(value: &str) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=MAX_EVERY).contains(n))
        .ok_or_else(|| "Type how many pages go in each file".to_owned())
}

/// The split the buttons and the typed texts say.
pub fn split_of(choice: SplitChoice, every: &str, ranges: &str) -> Result<Split, String> {
    match choice {
        SplitChoice::EachPage => Ok(Split::EachPage),
        SplitChoice::Every => every_from(every).map(Split::Every),
        SplitChoice::Ranges => Ok(Split::Ranges(ranges.trim().to_owned())),
    }
}

const SPLIT_CHOICES: [SplitChoice; 3] = [SplitChoice::EachPage, SplitChoice::Every, SplitChoice::Ranges];
const PAGE_IMAGES: [PageImage; 2] = [PageImage::Png, PageImage::Jpeg];

/// A conversion to start: what, on which inputs, to where.
#[derive(Clone)]
struct Job {
    inputs: Vec<PathBuf>,
    preset: &'static Preset,
    what: ConvertWhat,
    output: Output,
    /// Started again after it failed for want of ffmpeg: a second such failure gets the box.
    resubmitted: bool,
}

/// What the layer knows of the ffmpeg `ff`, found with `configured` set in settings.toml.
fn have_of(ff: &Ffmpeg, configured: Option<&Path>) -> Have {
    Have { version: ff.version, configured: configured.is_some_and(|c| c == ff.ffmpeg.as_path()) }
}

/// The Convert layer's choices (its texts live in the window).
struct Layer {
    items: Vec<(PathBuf, bool)>,
    /// The folder shown when it opened: a typed folder is relative to it.
    folder: PathBuf,
    commands: Vec<CommandSpec>,
    states: Vec<CommandState>,
    choices: Vec<Choice>,
    rows: Vec<Row>,
    choice: usize,
    image: ImageOptions,
    resize_mode: usize,
    text: TextOptions,
    output: OutputChoice,
    chosen: Option<PathBuf>,
    /// Whether the items' drive has a trash (`None`: not known yet).
    can_replace: Option<bool>,
    found: Found,
    detected: String,
    error: String,
    /// The PDF group's choices (the typed "every" and ranges live in the window).
    pdf: PdfChoices,
    /// "Images to PDF"'s pictures and the PDFs, in the order they are used (dragged).
    pictures: Vec<PathBuf>,
    pdfs: Vec<PathBuf>,
    /// Where pdfium is (`None`: not looked for yet; `Some(None)`: not there).
    pdfium: Option<Option<PathBuf>>,
    /// Each PDF's pages, as counted on a thread.
    pages: HashMap<PathBuf, Count>,
}

impl Layer {
    fn current(&self) -> Choice {
        self.choices[self.choice]
    }

    fn preset(&self) -> Option<&'static Preset> {
        match self.current() {
            Choice::Preset(preset) => Some(preset),
            Choice::Command(_) | Choice::Pdf(_) => None,
        }
    }

    fn pdf_op(&self) -> Option<PdfOp> {
        match self.current() {
            Choice::Pdf(op) => Some(op),
            _ => None,
        }
    }

    /// The order `op` uses its inputs in (the list dragged in the layer).
    fn order_mut(&mut self, op: PdfOp) -> &mut Vec<PathBuf> {
        if op == PdfOp::ImagesToPdf { &mut self.pictures } else { &mut self.pdfs }
    }

    fn pdfium_found(&self) -> bool {
        matches!(self.pdfium, Some(Some(_)))
    }

    /// Whether page counts are still coming.
    fn counting(&self) -> bool {
        self.pdfium_found() && self.pdfs.len() <= COUNT_MAX && self.pdfs.iter().any(|p| !self.pages.contains_key(p))
    }

    /// The PDFs' page counts (`None`: one that needs a password); empty when not counted.
    fn counts(&self) -> Vec<Option<u32>> {
        if self.counting() {
            return Vec::new();
        }
        self.pdfs
            .iter()
            .filter_map(|p| match self.pages.get(p) {
                Some(Count::Pages(n)) => Some(Some(*n)),
                Some(Count::Encrypted) => Some(None),
                _ => None,
            })
            .collect()
    }

    /// How many PDFs could not be counted (damaged, not PDFs); none while counting.
    fn unread(&self) -> usize {
        if self.counting() {
            return 0;
        }
        self.pdfs.iter().filter(|p| self.pages.get(*p) == Some(&Count::Failed)).count()
    }

    /// The note under a split, an extract or "PDF to images" and the mistake in what is typed (live), for the
    /// typed "every" and ranges.
    fn pdf_note(&self, op: PdfOp, every: &str, ranges: &str) -> (String, String) {
        let counts = self.counts();
        let result = match op {
            PdfOp::Split => split_of(self.pdf.split, every, ranges).and_then(|split| files_note(&split, &counts)),
            PdfOp::Extract => extract_note(ranges, &counts),
            PdfOp::ToImages => Ok(pictures_note(&counts)),
            _ => return (String::new(), String::new()),
        };
        // Nothing typed is not a mistake yet: Convert says what to type.
        let wants_ranges = op == PdfOp::Extract || self.pdf.split == SplitChoice::Ranges;
        let blank = wants_ranges && ranges.trim().is_empty();
        match result {
            Ok(note) if note.is_empty() && self.counting() => ("Counting pages…".to_owned(), String::new()),
            Ok(note) if note.is_empty() => (with_unread(unknown_note(&counts), self.unread()), String::new()),
            Ok(note) => (with_unread(&note, self.unread()), String::new()),
            Err(_) if blank => (String::new(), String::new()),
            Err(err) => (String::new(), err),
        }
    }

    /// Whether each choice can be chosen.
    fn enabled(&self) -> Vec<bool> {
        enabled_of(&self.rows)
    }

    /// What the current preset converts with the layer's options (the picture's numbers as
    /// they were last read).
    fn what(&self) -> Option<ConvertWhat> {
        self.preset().map(|preset| convert_what(preset, self.image, &self.text))
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    ops: Operations,
    dialogs: Dialogs,
    store: Option<ConfigStore>,
    state: RefCell<ConvertState>,
    layer: RefCell<Option<Layer>>,
    /// Counts openings of the layer: what a thread found for an earlier one is dropped.
    opening: Arc<AtomicU64>,
    /// Conversions running, for an ffmpeg they turn out to need.
    jobs: RefCell<HashMap<JobId, Job>>,
    /// PDF jobs running, for a pdfium that turns out not to load.
    pdf_jobs: RefCell<HashMap<JobId, PdfJob>>,
    /// The order list's names: one model, changed only when the order does, so that a row
    /// held for a drag is not rebuilt under the pointer (page counts arriving redraw the
    /// layer).
    pdf_order: Rc<VecModel<SharedString>>,
}

#[derive(Clone)]
pub struct Convert(Rc<Inner>);

thread_local! {
    static CURRENT: RefCell<Option<Convert>> = const { RefCell::new(None) };
    static SETTINGS: RefCell<(ConvertSettings, Vec<CommandSpec>)> = RefCell::default();
    /// Whether each command's program was found (by `run[0]`), looked up on a thread.
    static FOUND: RefCell<HashMap<String, bool>> = RefCell::default();
    /// The commands the last "Commands ▸" was made from, by index.
    static MENU_COMMANDS: RefCell<Vec<CommandSpec>> = RefCell::default();
}

/// settings.toml changed: `[convert]` and `[[commands]]`. The commands' programs are looked
/// up again on a thread.
pub fn set_settings(convert: ConvertSettings, commands: Vec<CommandSpec>) {
    SETTINGS.with(|s| *s.borrow_mut() = (convert, commands));
    look_up_programs();
}

/// The [[commands]] names, by index (the palette).
pub fn command_names() -> Vec<String> {
    commands().iter().map(|c| c.name.clone()).collect()
}

fn commands() -> Vec<CommandSpec> {
    SETTINGS.with(|s| s.borrow().1.clone())
}

fn configured_ffmpeg() -> Option<PathBuf> {
    SETTINGS.with(|s| s.borrow().0.ffmpeg.clone()).map(PathBuf::from)
}

/// Finds the commands' programs on a thread, for the menus shown next.
fn look_up_programs() {
    let mut programs: Vec<String> = commands().iter().map(program_of).filter(|p| !p.is_empty()).collect();
    programs.sort();
    programs.dedup();
    if programs.is_empty() {
        FOUND.with(|f| f.borrow_mut().clear());
        return;
    }
    std::thread::spawn(move || {
        let found: HashMap<String, bool> =
            programs.into_iter().map(|p| (p.clone(), find_program(std::ffi::OsStr::new(&p)).is_some())).collect();
        let _ = slint::invoke_from_event_loop(move || FOUND.with(|f| *f.borrow_mut() = found));
    });
}

fn found_program(program: &str) -> Option<bool> {
    FOUND.with(|f| f.borrow().get(program).copied())
}

fn states_for(commands: &[CommandSpec], items: &[(PathBuf, bool)]) -> Vec<CommandState> {
    commands.iter().map(|spec| command_state(spec, items, found_program(&program_of(spec)), cfg!(windows))).collect()
}

/// "Convert…" and "Commands ▸" for the rows a menu opens on. The programs are looked up
/// again for the next menu (PATH may have changed).
pub fn menu_items(rows: &[(PathBuf, bool)]) -> MenuItems {
    let commands = commands();
    let states = states_for(&commands, rows);
    let out = menu_entries(rows, &commands, &states);
    MENU_COMMANDS.with(|m| *m.borrow_mut() = commands);
    look_up_programs();
    out
}

/// A command chosen from "Commands ▸" (by its index in the menu's list), run on `rows`.
pub fn run_menu_command(index: usize, rows: Vec<(PathBuf, bool)>) {
    let Some(spec) = MENU_COMMANDS.with(|m| m.borrow().get(index).cloned()) else { return };
    with_current(|convert| convert.run_asked(spec, rows));
}

/// A command run by its key or the macOS menu bar (`index` in `[[commands]]`) on `items`.
pub fn run_by_index(index: usize, items: Vec<(PathBuf, bool)>) {
    let Some(spec) = commands().get(index).cloned() else { return };
    with_current(|convert| convert.run_checked(spec, items));
}

/// Whether `items` fit one command line of `spec` (a `{files}` command) here. One that
/// cannot be made at all is left to the task, which says why.
fn fits_one_run(spec: &CommandSpec, items: &[(PathBuf, bool)]) -> bool {
    let Some((first, _)) = items.first() else { return true };
    let dir = first.parent().unwrap_or(Path::new(""));
    let files: Vec<PathBuf> = items.iter().map(|(path, _)| path.clone()).collect();
    match expand_files_command(spec, dir, &files) {
        Err(_) => true,
        Ok(args) => {
            command_line_len(&args, cfg!(windows))
                <= gezik_platform::process::command_line_limit(&args[0].to_string_lossy())
        }
    }
}

/// Runs `f` with this UI thread's converter, if set up.
pub fn with_current(f: impl FnOnce(&Convert)) {
    if let Some(convert) = CURRENT.with(|c| c.borrow().clone()) {
        f(&convert);
    }
}

impl Convert {
    pub fn new(
        window: &AppWindow,
        ops: Operations,
        dialogs: Dialogs,
        store: Option<ConfigStore>,
        state: ConvertState,
    ) -> Convert {
        let this = Convert(Rc::new(Inner {
            window: window.as_weak(),
            ops,
            dialogs,
            store,
            state: RefCell::new(state),
            layer: RefCell::default(),
            opening: Arc::default(),
            jobs: RefCell::default(),
            pdf_jobs: RefCell::default(),
            pdf_order: Rc::new(VecModel::default()),
        }));
        this.install(window);
        CURRENT.with(|c| *c.borrow_mut() = Some(this.clone()));
        this
    }

    fn install(&self, window: &AppWindow) {
        let t = self.clone();
        window.on_cv_set_format(move |i| {
            if let Some((format, _)) = usize::try_from(i).ok().and_then(|i| FORMATS.get(i)) {
                t.edit(|layer| layer.image.format = *format);
            }
        });
        let t = self.clone();
        window.on_cv_set_resize(move |i| {
            if let Some(i) = usize::try_from(i).ok().filter(|i| *i < RESIZE_MODES) {
                t.edit(|layer| layer.resize_mode = i);
            }
        });
        let t = self.clone();
        window.on_cv_toggle(move |key| {
            t.edit(|layer| match key.as_str() {
                "enlarge" => layer.image.never_enlarge = !layer.image.never_enlarge,
                "rotate" => layer.image.rotate_by_exif = !layer.image.rotate_by_exif,
                "strip" => layer.image.strip_metadata = !layer.image.strip_metadata,
                "bom" => layer.text.bom = !layer.text.bom,
                "trim" => layer.text.trim_trailing = !layer.text.trim_trailing,
                "final" => layer.text.final_newline = !layer.text.final_newline,
                _ => {}
            });
        });
        let t = self.clone();
        window.on_cv_set_eol(move |i| {
            if let Some((eol, _)) = usize::try_from(i).ok().and_then(|i| EOLS.get(i)) {
                t.edit(|layer| layer.text.eol = *eol);
            }
        });
        let t = self.clone();
        window.on_cv_set_output(move |i| t.set_output(i));
        let t = self.clone();
        window.on_cv_edited(move || t.set_error(""));
        let t = self.clone();
        window.on_cv_convert(move || t.convert());
        let t = self.clone();
        window.on_cv_cancel(move || t.close());
        let t = self.clone();
        window.on_cv_pdf_set(move |key, i| t.pdf_set(key.as_str(), i));
        let t = self.clone();
        window.on_cv_pdf_reorder(move |from, to| {
            let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else { return };
            t.edit(|layer| {
                if let Some(op) = layer.pdf_op() {
                    move_in_order(layer.order_mut(op), from, to);
                }
            });
        });
        window.on_cv_pdf_drop_row(|y, row_height, count| {
            crate::batch_rename::drop_row(y, row_height, usize::try_from(count).unwrap_or(0)) as i32
        });
    }

    /// A button of the PDF group: `key` "page", "margin", "split", "dpi" or "image", `index`
    /// its position.
    fn pdf_set(&self, key: &str, index: i32) {
        let Ok(i) = usize::try_from(index) else { return };
        self.edit(|layer| {
            let c = &mut layer.pdf;
            match key {
                "page" => c.page.size = SIZES.get(i).map_or(c.page.size, |(size, _)| *size),
                "margin" => c.page.margin = MARGINS.get(i).map_or(c.page.margin, |(margin, _)| *margin),
                "split" => c.split = SPLIT_CHOICES.get(i).copied().unwrap_or(c.split),
                "dpi" => c.dpi = RENDER_DPIS.get(i).copied().unwrap_or(c.dpi),
                "image" => c.image = PAGE_IMAGES.get(i).copied().unwrap_or(c.image),
                _ => {}
            }
        });
    }

    /// Where downloaded tools are: `<config dir>/tools/` (as archives.rs has it).
    fn data_dir(&self) -> PathBuf {
        self.0.store.as_ref().map_or_else(|| std::env::temp_dir().join("gezik"), |store| store.dir().to_path_buf())
    }

    /// Writes the last choices to state.toml.
    fn save_state(&self, f: impl FnOnce(&mut ConvertState)) {
        f(&mut self.0.state.borrow_mut());
        if let Some(store) = &self.0.store {
            let convert = self.0.state.borrow().clone();
            store.update_state(|saved| saved.convert = convert);
        }
    }

    pub fn is_open(&self) -> bool {
        self.0.layer.borrow().is_some()
    }

    /// "Convert…": the layer for `items` (path, is a folder).
    pub fn open(&self, items: Vec<(PathBuf, bool)>) {
        self.open_with(items, None);
    }

    /// The layer for `items`, on `wanted` when it is offered for them ("Images to PDF…").
    pub fn open_with(&self, items: Vec<(PathBuf, bool)>, wanted: Option<Choice>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let Some(folder) = items.first().and_then(|(p, _)| p.parent()).map(Path::to_path_buf) else { return };
        if self.is_open() {
            return;
        }
        let commands = commands();
        let states = states_for(&commands, &items);
        let kinds = kinds_for(&items);
        let (choices, rows) = preset_list(&kinds, &pdf_ops_for(&items), &commands, &states);
        let state = self.0.state.borrow().clone();
        let pictures = pdf_inputs(&items, PdfOp::ImagesToPdf).0;
        let pdfs = pdf_inputs(&items, PdfOp::Split).0;
        let mut layer = Layer {
            items,
            folder,
            commands,
            states,
            choices,
            rows,
            choice: 0,
            image: ImageOptions::DEFAULT,
            resize_mode: 0,
            text: preset_text(&PRESETS[0]),
            output: OutputChoice::Same,
            chosen: None,
            can_replace: None,
            found: Found::Unknown,
            detected: String::new(),
            error: String::new(),
            pdf: state.pdf.as_deref().map_or(PdfChoices::DEFAULT, saved_pdf),
            pictures,
            pdfs,
            pdfium: None,
            pages: HashMap::new(),
        };
        let mut wanted_keys = wanted_presets(&state, &kinds);
        if let Some(choice) = wanted {
            wanted_keys.insert(0, choice.key(&layer.commands));
        }
        let Some(choice) = first_choice(&layer.choices, &layer.enabled(), &layer.commands, &wanted_keys) else {
            return;
        };
        layer.choice = choice;
        self.load_choice(&mut layer, None);
        // The options used last with this preset (saved per kind), and the output when it was
        // the last one used.
        if let Some(preset) = layer.preset() {
            let last = state.last_preset.as_deref() == Some(preset.id);
            // Saved before options named their preset: the last one's.
            let was_for = |text: &str| saved_preset(text).map_or(last, |id| id == preset.id);
            if preset.kind == Kind::Image
                && !preset.keeps_format()
                && let Some(text) = state.image.as_deref().filter(|t| was_for(t))
            {
                layer.image = image_options_from(text, layer.image);
                layer.resize_mode = resize_choice(layer.image.resize).0;
            }
            if preset.kind == Kind::Text
                && let Some(text) = state.text.as_deref().filter(|t| was_for(t))
            {
                layer.text = text_options_from(text, layer.text.clone());
            }
            if last && let Some(output) = OutputChoice::from_key(&state.last_output) {
                layer.output = output;
            }
        }
        if layer.output == OutputChoice::Folder {
            layer.chosen = state.last_folder.as_deref().and_then(|text| resolve_folder(text, &layer.folder));
            if layer.chosen.is_none() {
                layer.output = OutputChoice::Same;
            }
        }
        let (_, value) = resize_choice(layer.image.resize);
        window.set_cv_quality(layer.image.quality.to_string().into());
        window.set_cv_resize_value(value.into());
        window.set_cv_background(background_text(layer.image.background).into());
        window.set_cv_pdf_every(layer.pdf.every.to_string().into());
        // Page ranges are for one document: never kept.
        window.set_cv_pdf_ranges("".into());
        let probe = (layer.items.clone(), layer.folder.clone(), kinds, layer.pdfs.clone());
        *self.0.layer.borrow_mut() = Some(layer);
        self.show();
        window.set_cv_open(true);
        self.probe(probe.0, probe.1, &probe.2, probe.3);
    }

    /// Looks on a thread at what the layer cannot know by names: whether the drive has a
    /// trash, which ffmpeg there is, what encodings the text files are in; on another,
    /// whether pdfium is there and how many pages the PDFs have.
    fn probe(&self, items: Vec<(PathBuf, bool)>, folder: PathBuf, kinds: &[Kind], pdfs: Vec<PathBuf>) {
        let opening = self.0.opening.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.0.opening.clone();
        if !pdfs.is_empty() {
            self.probe_pdfs(opening, pdfs);
        }
        let wants_ffmpeg = kinds.iter().any(|k| matches!(k, Kind::Image | Kind::Media));
        let wants_text = kinds.contains(&Kind::Text);
        let (data, configured) = (self.data_dir(), configured_ffmpeg());
        std::thread::spawn(move || {
            let trash = gezik_platform::fs::drive_facts(&folder).is_ok_and(|facts| facts.trash);
            let found = if wants_ffmpeg {
                find_ffmpeg(&data, configured.as_deref()).map_or(Found::Missing, |ff| Found::Version(ff.version))
            } else {
                Found::Unknown
            };
            let mut counts: Vec<(&'static str, usize)> = Vec::new();
            if wants_text {
                let texts = items.iter().filter(|(p, d)| !d && looks_text(&name_of(p))).take(DETECT_MAX);
                for (path, _) in texts {
                    if current.load(Ordering::SeqCst) != opening {
                        return;
                    }
                    let mut head = Vec::new();
                    let read = std::fs::File::open(path).and_then(|f| f.take(SNIFF as u64).read_to_end(&mut head));
                    if read.is_err() {
                        continue;
                    }
                    if let Detected::Text(encoding, _) = detect(&head) {
                        let name = gezik_batch::convert::text::name(encoding);
                        match counts.iter_mut().find(|(n, _)| *n == name) {
                            Some((_, n)) => *n += 1,
                            None => counts.push((name, 1)),
                        }
                    }
                }
            }
            let detected = detected_text(&counts);
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| this.probed(opening, trash, found, detected));
            });
        });
    }

    /// Finds pdfium, then counts the pages of up to [`COUNT_MAX`] PDFs with it, one worker
    /// at a time; a newer opening (or the layer closing) stops it.
    fn probe_pdfs(&self, opening: u64, pdfs: Vec<PathBuf>) {
        let current = self.0.opening.clone();
        let data = self.data_dir();
        std::thread::spawn(move || {
            let stale = || current.load(Ordering::SeqCst) != opening;
            let library = gezik_batch::tools::find(Tool::Pdfium, &data, None);
            let found = library.clone();
            let _ = slint::invoke_from_event_loop(move || with_current(|this| this.pdfium_found(opening, found)));
            let Some(library) = library.filter(|_| pdfs.len() <= COUNT_MAX) else { return };
            let worker = Worker::this_exe();
            for pdf in pdfs {
                if stale() {
                    return;
                }
                let count = match worker.as_ref().map(|worker| count_pages(worker, &library, &pdf, &stale)) {
                    Ok(Ok(Some(pages))) => Count::Pages(pages),
                    Ok(Ok(None)) => Count::Encrypted,
                    _ => Count::Failed,
                };
                let _ = slint::invoke_from_event_loop(move || with_current(|this| this.counted(opening, pdf, count)));
            }
        });
    }

    fn pdfium_found(&self, opening: u64, library: Option<PathBuf>) {
        if self.0.opening.load(Ordering::SeqCst) != opening {
            return;
        }
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            layer.pdfium = Some(library);
        }
        self.show();
    }

    fn counted(&self, opening: u64, pdf: PathBuf, count: Count) {
        if self.0.opening.load(Ordering::SeqCst) != opening {
            return;
        }
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            layer.pages.insert(pdf, count);
        }
        self.show();
    }

    fn probed(&self, opening: u64, trash: bool, found: Found, detected: String) {
        if self.0.opening.load(Ordering::SeqCst) != opening {
            return;
        }
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            layer.can_replace = Some(trash);
            if !trash && layer.output == OutputChoice::Replace {
                layer.output = OutputChoice::Same;
            }
            layer.found = found;
            layer.detected = detected;
        }
        self.show();
    }

    /// Sets the options and output for the layer's current choice; `before`: the kind chosen
    /// before (the output follows a new kind's default).
    fn load_choice(&self, layer: &mut Layer, before: Option<Kind>) {
        if let Choice::Pdf(op) = layer.current() {
            layer.pdf.op = op;
            return;
        }
        let Choice::Preset(preset) = layer.current() else { return };
        layer.image = preset_image(preset);
        layer.resize_mode = resize_choice(layer.image.resize).0;
        layer.text = preset_text(preset);
        if before != Some(preset.kind) {
            layer.output = OutputChoice::of(&preset.default_output());
        }
        if layer.output == OutputChoice::Replace && layer.can_replace == Some(false) {
            layer.output = OutputChoice::Same;
        }
    }

    /// A preset from the list (an index into the layer's choices).
    fn choose(&self, index: usize) {
        let Some(window) = self.0.window.upgrade() else { return };
        {
            let mut layer = self.0.layer.borrow_mut();
            let Some(layer) = layer.as_mut() else { return };
            if index >= layer.choices.len() || !layer.enabled().get(index).copied().unwrap_or(false) {
                return;
            }
            let before = layer.current().kind();
            layer.choice = index;
            self.load_choice(layer, Some(before));
            layer.error.clear();
            let (_, value) = resize_choice(layer.image.resize);
            window.set_cv_quality(layer.image.quality.to_string().into());
            window.set_cv_resize_value(value.into());
            window.set_cv_background(background_text(layer.image.background).into());
        }
        self.show();
    }

    /// The preset list, for the menu under the Preset button.
    pub fn preset_menu(&self) -> Vec<(u32, String, bool)> {
        let layer = self.0.layer.borrow();
        let Some(layer) = layer.as_ref() else { return Vec::new() };
        preset_menu(&layer.rows, layer.choice)
    }

    /// The "From" (`from`) or "To" encodings, for the menu under their button.
    pub fn encoding_menu(&self, from: bool) -> Vec<(u32, String, bool)> {
        let layer = self.0.layer.borrow();
        let Some(layer) = layer.as_ref() else { return Vec::new() };
        let (first, current, label) = if from {
            (ENCODING_FROM_FIRST, layer.text.from.clone().unwrap_or_default(), "Detect")
        } else {
            (ENCODING_TO_FIRST, layer.text.to.clone(), "Keep each file's")
        };
        let mark = |on: bool, title: &str| format!("{}{title}", if on { "• " } else { "    " });
        let mut out = vec![(first, mark(current.is_empty(), label), true)];
        for (i, (code, shown)) in encodings().into_iter().enumerate().take(ENCODING_MAX as usize - 1) {
            out.push((first + 1 + i as u32, mark(code == current, shown), true));
        }
        out
    }

    /// An item of one of the layer's menus.
    pub fn menu_chosen(&self, id: u32) {
        if (CONVERT_PRESET_FIRST..CONVERT_PRESET_FIRST + CONVERT_PRESET_MAX).contains(&id) {
            self.choose((id - CONVERT_PRESET_FIRST) as usize);
        } else if (ENCODING_FROM_FIRST..ENCODING_FROM_FIRST + ENCODING_MAX).contains(&id) {
            let label = Self::encoding_at((id - ENCODING_FROM_FIRST) as usize);
            self.edit(|layer| layer.text.from = label);
        } else if (ENCODING_TO_FIRST..ENCODING_TO_FIRST + ENCODING_MAX).contains(&id) {
            let label = Self::encoding_at((id - ENCODING_TO_FIRST) as usize);
            self.edit(|layer| layer.text.to = label.unwrap_or_default());
        }
    }

    /// The encoding at a menu position: the first is none (detect, keep).
    fn encoding_at(index: usize) -> Option<String> {
        index.checked_sub(1).and_then(|i| encodings().get(i).map(|(label, _)| (*label).to_owned()))
    }

    fn edit(&self, f: impl FnOnce(&mut Layer)) {
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            f(layer);
            layer.error.clear();
        }
        self.show();
    }

    fn set_error(&self, error: &str) {
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            layer.error = error.to_owned();
        }
        self.show();
    }

    /// An output button: "Choose…" asks for the folder first.
    fn set_output(&self, index: i32) {
        let Some(output) = usize::try_from(index).ok().and_then(|i| OUTPUTS.get(i)).map(|(o, _)| *o) else { return };
        if output == OutputChoice::Replace
            && self.0.layer.borrow().as_ref().is_some_and(|layer| layer.can_replace == Some(false))
        {
            return;
        }
        if output != OutputChoice::Folder {
            return self.edit(|layer| layer.output = output);
        }
        let Some((base, shown)) = self.0.layer.borrow().as_ref().map(|layer| {
            let shown = layer.chosen.as_ref().map(|p| p.display().to_string());
            (layer.folder.clone(), shown)
        }) else {
            return;
        };
        let initial =
            shown.or_else(|| self.0.state.borrow().last_folder.clone()).unwrap_or_else(|| base.display().to_string());
        let this = self.clone();
        self.0.dialogs.ask_text("Convert to", "Folder:", initial, &["Choose", "Cancel"], move |text| {
            let Some(dir) = text.and_then(|text| resolve_folder(&text, &base)) else { return };
            this.edit(|layer| {
                layer.output = OutputChoice::Folder;
                layer.chosen = Some(dir);
            });
        });
    }

    /// Shows the layer's choices.
    fn show(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let layer = self.0.layer.borrow();
        let Some(layer) = layer.as_ref() else { return };
        let choice = layer.current();
        let preset = layer.preset();
        let keeps_format = preset.is_some_and(Preset::keeps_format);
        let (title, kind, note) = match choice {
            Choice::Preset(preset) => {
                let kind = match preset.kind {
                    Kind::Image if keeps_format => 4,
                    Kind::Image => 1,
                    Kind::Text => 2,
                    _ => 3,
                };
                let note = if keeps_format {
                    "Each picture keeps its format; a JPEG is not re-encoded.".to_owned()
                } else {
                    String::new()
                };
                // The typed numbers count as they read now.
                let mut image = layer.image;
                if let Ok(q) = quality_from(&window.get_cv_quality()) {
                    image.quality = q;
                }
                if let Ok(resize) = resize_from(layer.resize_mode, &window.get_cv_resize_value()) {
                    image.resize = resize;
                }
                if let Ok(background) = background_from(&window.get_cv_background()) {
                    image.background = background;
                }
                let title = if options_changed(preset, &image, &layer.text) {
                    format!("{} (changed)", preset.label)
                } else {
                    preset.label.to_owned()
                };
                (title, kind, note)
            }
            Choice::Command(i) => {
                let spec = &layer.commands[i];
                (spec.name.clone(), 0, format!("Runs {}", spec.run.join(" ")))
            }
            Choice::Pdf(op) if op.needs_pdfium() && !layer.pdfium_found() => {
                (format!("{} ¹", op.label()), 5, String::new())
            }
            Choice::Pdf(op) => (op.label().to_owned(), 5, String::new()),
        };
        let what = layer.what();
        let skipped = match (&what, choice) {
            (Some(what), _) => split_inputs(&layer.items, what).1,
            (None, Choice::Command(i)) => {
                let spec = &layer.commands[i];
                layer.items.iter().filter(|(p, d)| !command_applies(spec, &name_of(p), *d)).count()
            }
            (None, Choice::Pdf(op)) => pdf_inputs(&layer.items, op).1,
            (None, _) => 0,
        };
        let pdf_op = layer.pdf_op();
        let skipped = match pdf_op {
            Some(op) => crate::pdf::skipped_text(skipped, op),
            None => skipped_text(skipped, what.as_ref()),
        };
        let tool_note = match pdf_op {
            Some(op) if op.needs_pdfium() && !layer.pdfium_found() => "¹ needs pdfium",
            Some(_) => "",
            None => ffmpeg_note(choice.kind(), keeps_format, layer.found),
        };
        let (pdf_note, pdf_error) = pdf_op.map_or_else(Default::default, |op| {
            layer.pdf_note(op, &window.get_cv_pdf_every(), &window.get_cv_pdf_ranges())
        });
        let order: Vec<SharedString> = match pdf_op {
            Some(PdfOp::ImagesToPdf) => layer.pictures.iter().map(|p| name_of(p).into()).collect(),
            Some(PdfOp::Merge) => layer.pdfs.iter().map(|p| name_of(p).into()).collect(),
            _ => Vec::new(),
        };
        let error = if layer.error.is_empty() { pdf_error } else { layer.error.clone() };
        crate::batch_rename::update_rows(&self.0.pdf_order, order);
        let format = layer.image.format;
        let replace_note = match layer.can_replace {
            Some(true) if cfg!(windows) => "Originals go to the Recycle Bin",
            Some(true) => "Originals go to the trash",
            Some(false) if cfg!(windows) => "This drive has no Recycle Bin",
            Some(false) => "This drive has no trash",
            None => "",
        };
        let formats: Vec<SharedString> = FORMATS.iter().map(|(_, label)| (*label).into()).collect();
        let from = layer.text.from.as_deref().map_or_else(|| "Detect".to_owned(), encoding_shown);
        let to = if layer.text.to.is_empty() { "Keep each file's".to_owned() } else { encoding_shown(&layer.text.to) };
        let view = ConvertView {
            title: match layer.items.as_slice() {
                [(one, _)] => format!("Convert {}", name_of(one)),
                many => format!("Convert {} items", many.len()),
            }
            .into(),
            preset: title.into(),
            kind,
            note: note.into(),
            formats: ModelRc::new(VecModel::from(formats)),
            format: FORMATS.iter().position(|(f, _)| *f == format).unwrap_or(0) as i32,
            show_quality: matches!(format, ImageFormat::Jpeg | ImageFormat::WebpLossy | ImageFormat::Avif),
            resize: layer.resize_mode as i32,
            never_enlarge: layer.image.never_enlarge,
            rotate: layer.image.rotate_by_exif,
            strip: layer.image.strip_metadata,
            show_background: format == ImageFormat::Jpeg,
            from: from.into(),
            detected: layer.detected.as_str().into(),
            to: to.into(),
            bom: layer.text.bom,
            eol: EOLS.iter().position(|(e, _)| *e == layer.text.eol).unwrap_or(0) as i32,
            trim: layer.text.trim_trailing,
            final_newline: layer.text.final_newline,
            show_output: preset.is_some(),
            output: layer.output.index(),
            folder: layer.chosen.as_ref().map(|p| p.display().to_string()).unwrap_or_default().into(),
            can_replace: layer.can_replace != Some(false),
            replace_note: replace_note.into(),
            skipped: skipped.into(),
            ffmpeg_note: tool_note.into(),
            error: error.into(),
            pdf_op: pdf_op.and_then(|op| PdfOp::ALL.iter().position(|o| *o == op)).map_or(-1, |i| i as i32),
            pdf_order: ModelRc::from(self.0.pdf_order.clone()),
            pdf_page: SIZES.iter().position(|(s, _)| *s == layer.pdf.page.size).unwrap_or(0) as i32,
            pdf_margin: MARGINS.iter().position(|(m, _)| *m == layer.pdf.page.margin).unwrap_or(0) as i32,
            pdf_split: SPLIT_CHOICES.iter().position(|s| *s == layer.pdf.split).unwrap_or(0) as i32,
            pdf_dpi: RENDER_DPIS.iter().position(|d| *d == layer.pdf.dpi).unwrap_or(0) as i32,
            pdf_image: PAGE_IMAGES.iter().position(|i| *i == layer.pdf.image).unwrap_or(0) as i32,
            pdf_note: pdf_note.into(),
            pdf_losses: if pdf_op.is_some_and(PdfOp::loses_extras) { crate::pdf::LOSSES } else { "" }.into(),
        };
        window.set_cv_view(view);
    }

    /// Esc closes the layer and Ctrl+Enter (Cmd on macOS) converts, wherever its focus is;
    /// returns whether the key was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        let primary = crate::keys::is_primary(chord, KeyPlatform::current());
        match chord.key {
            Key::Escape if !primary && !chord.shift => self.close(),
            Key::Enter if primary => self.convert(),
            _ => return false,
        }
        true
    }

    pub fn close(&self) {
        self.0.layer.borrow_mut().take();
        self.0.opening.fetch_add(1, Ordering::SeqCst);
        if let Some(window) = self.0.window.upgrade() {
            window.set_cv_open(false);
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }

    /// Convert: reads the typed numbers, then starts the job (or runs the command).
    fn convert(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let checked = {
            let layer = self.0.layer.borrow();
            let Some(layer) = layer.as_ref() else { return };
            self.check(&window, layer)
        };
        match checked {
            Err(error) => self.set_error(&error),
            Ok(Ready::Command(spec, items)) => {
                let key = format!("command:{}", spec.name);
                self.save_state(|state| state.last_preset = Some(key));
                self.close();
                self.run_command(spec, items);
            }
            Ok(Ready::Convert(job)) => {
                // Each kind remembers its own preset (and options) for the next time.
                let id = job.preset.id;
                let image = match &job.what {
                    ConvertWhat::Image(options) => Some(format!("preset={id} {}", image_options_text(options))),
                    ConvertWhat::RemoveLocation(_) => Some(format!("preset={id}")),
                    _ => None,
                };
                let text = match &job.what {
                    ConvertWhat::Text(options) => Some(format!("preset={id} {}", text_options_text(options))),
                    _ => None,
                };
                let output = OutputChoice::of(&job.output);
                let folder = match &job.output {
                    Output::Folder(dir) => Some(dir.display().to_string()),
                    _ => None,
                };
                let preset = job.preset;
                self.save_state(|state| {
                    state.last_preset = Some(preset.id.to_owned());
                    if image.is_some() {
                        state.image = image;
                    }
                    if text.is_some() {
                        state.text = text;
                    }
                    if preset.kind == Kind::Media {
                        state.media = Some(preset.id.to_owned());
                    }
                    state.last_output = output.key().to_owned();
                    if folder.is_some() {
                        state.last_folder = folder;
                    }
                });
                self.close();
                self.start(job);
            }
            Ok(Ready::Pdf(job, choices)) => {
                let (key, text) = (Choice::Pdf(choices.op).key(&[]), choices_text(&choices));
                self.save_state(|state| {
                    state.last_preset = Some(key);
                    state.pdf = Some(text);
                });
                self.close();
                self.start_pdf(job);
            }
        }
    }

    /// The PDF group's Convert: what the choices and the typed texts say, checked against the
    /// page counts known.
    fn check_pdf(&self, window: &AppWindow, layer: &Layer, op: PdfOp) -> Result<Ready, String> {
        let mut choices = PdfChoices { op, ..layer.pdf.clone() };
        let every = window.get_cv_pdf_every();
        if let Ok(n) = every_from(&every) {
            choices.every = n;
        }
        let ranges = window.get_cv_pdf_ranges().trim().to_owned();
        let counts = layer.counts();
        if op == PdfOp::ImagesToPdf {
            let pictures = layer.pictures.clone();
            let Some(output) = pictures_pdf_name(&pictures) else {
                return Err("No pictures here that Gezik reads itself".to_owned());
            };
            return Ok(Ready::Pdf(PdfJob::Pictures { pictures, output, page: choices.page }, choices));
        }
        let inputs = layer.pdfs.clone();
        if inputs.is_empty() {
            return Err("No PDFs here".to_owned());
        }
        let work = match op {
            PdfOp::Split => {
                let split = split_of(choices.split, &every, &ranges)?;
                files_note(&split, &counts)?;
                PdfWork::Split(split)
            }
            PdfOp::Extract => {
                extract_note(&ranges, &counts)?;
                PdfWork::Extract(ranges)
            }
            PdfOp::ToImages => PdfWork::Render { dpi: choices.dpi, image: choices.image },
            PdfOp::Merge | PdfOp::ImagesToPdf => PdfWork::Merge,
        };
        Ok(Ready::Pdf(PdfJob::Pdfs { work, inputs, resubmitted: false }, choices))
    }

    /// Starts a PDF job: pictures at once; PDF work once pdfium is found (on a thread), else
    /// the box offers it and starts the job after the download.
    fn start_pdf(&self, job: PdfJob) {
        match job {
            PdfJob::Pictures { pictures, output, page } => {
                let again =
                    self.pdf_again(PdfJob::Pictures { pictures: pictures.clone(), output: output.clone(), page });
                let label = images_pdf_label(pictures.len());
                self.0.ops.remember_for(&pictures);
                let task: Box<dyn gezik_ops::Task> = Box::new(ImagesToPdfTask::new(pictures, output, page));
                self.0.ops.submit_chain(vec![task], Some(label), Some(again), After::Select);
            }
            PdfJob::Pdfs { work, inputs, resubmitted } => {
                let data = self.data_dir();
                std::thread::spawn(move || {
                    let library = gezik_batch::tools::find(Tool::Pdfium, &data, None);
                    let hint = if library.is_none() { gezik_platform::http::tool_missing_hint() } else { None };
                    let worker = Worker::this_exe().map_err(|err| err.to_string());
                    let _ = slint::invoke_from_event_loop(move || {
                        with_current(|this| this.pdf_found(work, inputs, resubmitted, library, worker, hint));
                    });
                });
            }
        }
    }

    fn pdf_found(
        &self,
        work: PdfWork,
        inputs: Vec<PathBuf>,
        resubmitted: bool,
        library: Option<PathBuf>,
        worker: Result<Worker, String>,
        hint: Option<String>,
    ) {
        // From its row it starts afresh: a load failure then offers the download again.
        let again = self.pdf_again(PdfJob::Pdfs { work: work.clone(), inputs: inputs.clone(), resubmitted: false });
        let job = PdfJob::Pdfs { work: work.clone(), inputs: inputs.clone(), resubmitted };
        let Some(library) = library else {
            crate::archives::with_current(|archives| archives.offer_pdfium(false, hint, again, None));
            return;
        };
        let worker = match worker {
            Ok(worker) => worker,
            Err(err) => {
                let message = format!("Gezik could not start its PDF engine: {err}");
                self.0.dialogs.ask("PDF tools", message, &["OK"], |_| {});
                return;
            }
        };
        let label = pdf_label(&work, &inputs);
        self.0.ops.remember_for(&inputs);
        let tasks = pdf_chain(work, inputs, PdfTools { worker, library });
        let id = self.0.ops.submit_chain(tasks, Some(label), Some(again), After::Select);
        self.0.pdf_jobs.borrow_mut().insert(id, job);
    }

    /// Starts `job` again (from its row, or once pdfium is there).
    fn pdf_again(&self, job: PdfJob) -> Rc<dyn Fn()> {
        Rc::new(move || {
            let job = job.clone();
            with_current(|this| this.start_pdf(job));
        })
    }

    /// PDF job `id` ended with pdfium failing to load (a broken download): the box offers to
    /// get it again, then starts the job again in place of its row. When that fresh download
    /// fails to load too, a plain box says why: another download would not help.
    fn pdf_finished(&self, id: JobId, job: PdfJob, report: &Report) {
        if report.cancelled {
            return;
        }
        let Some(failed) = report.failures.iter().find(|f| says_pdfium_failed(&f.message)) else { return };
        let resubmitted = matches!(job, PdfJob::Pdfs { resubmitted: true, .. });
        if after_pdfium_failed(resubmitted) == PdfiumFailed::Explain {
            self.0.dialogs.ask("pdfium failed to load", pdfium_explained(&failed.message), &["OK"], |_| {});
            return;
        }
        let job = match job {
            PdfJob::Pdfs { work, inputs, .. } => PdfJob::Pdfs { work, inputs, resubmitted: true },
            other => other,
        };
        std::thread::spawn(move || {
            let hint = gezik_platform::http::tool_missing_hint();
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| {
                    let again = this.pdf_again(job);
                    crate::archives::with_current(|archives| archives.offer_pdfium(true, hint, again, Some(id)));
                });
            });
        });
    }

    fn check(&self, window: &AppWindow, layer: &Layer) -> Result<Ready, String> {
        let choice = layer.current();
        if let Choice::Pdf(op) = choice {
            return self.check_pdf(window, layer, op);
        }
        if let Choice::Command(i) = choice {
            let spec = layer.commands[i].clone();
            if let Some(CommandState::Off(tip)) = layer.states.get(i) {
                return Err(format!("{}: {tip}", spec.name));
            }
            let items: Vec<(PathBuf, bool)> =
                layer.items.iter().filter(|(p, d)| command_applies(&spec, &name_of(p), *d)).cloned().collect();
            if items.is_empty() {
                return Err(format!("Nothing here for {}", spec.name));
            }
            return Ok(Ready::Command(spec, items));
        }
        let Some(preset) = layer.preset() else { return Err("Choose a preset".to_owned()) };
        let mut image = layer.image;
        if preset.kind == Kind::Image && !preset.keeps_format() {
            if matches!(image.format, ImageFormat::Jpeg | ImageFormat::WebpLossy | ImageFormat::Avif) {
                image.quality = quality_from(&window.get_cv_quality())?;
            }
            image.resize = resize_from(layer.resize_mode, &window.get_cv_resize_value())?;
            if image.format == ImageFormat::Jpeg {
                image.background = background_from(&window.get_cv_background())?;
            }
        }
        let what = convert_what(preset, image, &layer.text);
        let output = match layer.output {
            OutputChoice::Same => Output::SameFolder,
            OutputChoice::Subfolder => Output::Subfolder,
            OutputChoice::Folder => match &layer.chosen {
                Some(dir) => Output::Folder(dir.clone()),
                None => return Err("Choose the folder".to_owned()),
            },
            // Not known yet: the job refuses to replace without a trash.
            OutputChoice::Replace if layer.can_replace != Some(false) => Output::ReplaceOriginal,
            OutputChoice::Replace => return Err("This drive has no trash to put the originals in".to_owned()),
        };
        let (inputs, _) = split_inputs(&layer.items, &what);
        if inputs.is_empty() {
            return Err("Nothing here to convert with this preset".to_owned());
        }
        Ok(Ready::Convert(Job { inputs, preset, what, output, resubmitted: false }))
    }

    /// Finds ffmpeg on a thread if the job needs it, then starts what can run and offers
    /// ffmpeg for the rest.
    fn start(&self, job: Job) {
        let wants = job.inputs.iter().any(|p| needs_ffmpeg(&job.what, &name_of(p)));
        let (data, configured) = (self.data_dir(), configured_ffmpeg());
        std::thread::spawn(move || {
            let ffmpeg = if wants { find_ffmpeg(&data, configured.as_deref()) } else { None };
            let hint = if wants { gezik_platform::http::tool_missing_hint() } else { None };
            let found = ffmpeg.as_ref().map(|ff| have_of(ff, configured.as_deref()));
            // A chosen folder that is not there is made by the job (with its missing parents),
            // undone with it.
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| this.found(job, ffmpeg, found, hint));
            });
        });
    }

    fn found(&self, job: Job, ffmpeg: Option<Ffmpeg>, found: Option<Have>, hint: Option<String>) {
        let (now, waiting, need) = split_by_ffmpeg(&job.what, job.inputs.clone(), found);
        if !now.is_empty() {
            let task = ConvertTask::new(now.clone(), job.what.clone(), job.output.clone(), ConvertTools { ffmpeg });
            // Retry goes through `start` again, so its job is followed too (and ffmpeg found anew).
            let again = self.again(Job { inputs: now.clone(), resubmitted: false, ..job.clone() });
            self.0.ops.remember_for(&now);
            let id = self.0.ops.submit_chain(vec![Box::new(task)], None, Some(again), After::Select);
            self.0.jobs.borrow_mut().insert(id, Job { inputs: now, ..job.clone() });
        }
        if let Some(need) = need {
            let again = self.again(Job { inputs: waiting, resubmitted: false, ..job });
            crate::archives::with_current(|archives| archives.offer_ffmpeg(need, hint, again, None));
        }
    }

    /// Starts `job` again (once ffmpeg is there).
    fn again(&self, job: Job) -> Rc<dyn Fn()> {
        Rc::new(move || {
            let job = job.clone();
            with_current(|this| this.start(job));
        })
    }

    /// A job ended (operations.rs tells every one): pictures or media that turned out to
    /// need ffmpeg run again when a good enough one is there now, else get the box, which
    /// starts them again after a download.
    pub fn job_finished(&self, id: JobId, report: &Report) {
        let pdf = self.0.pdf_jobs.borrow_mut().remove(&id);
        if let Some(job) = pdf {
            return self.pdf_finished(id, job, report);
        }
        let Some(job) = self.0.jobs.borrow_mut().remove(&id) else { return };
        if report.cancelled {
            return;
        }
        let needing: Vec<PathBuf> = job
            .inputs
            .iter()
            .filter(|input| report.failures.iter().any(|f| f.path == **input && is_ffmpeg_needed(&f.message)))
            .cloned()
            .collect();
        if needing.is_empty() {
            return;
        }
        let job = Job { inputs: needing, ..job };
        let (data, configured) = (self.data_dir(), configured_ffmpeg());
        std::thread::spawn(move || {
            let found = find_ffmpeg(&data, configured.as_deref()).map(|ff| have_of(&ff, configured.as_deref()));
            let hint = gezik_platform::http::tool_missing_hint();
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| this.offer_or_again(id, job, found, hint));
            });
        });
    }

    /// After job `id` failed for want of ffmpeg on `job`'s inputs: run them again once if the
    /// ffmpeg there now does for them (taking the failed row away), else offer the box.
    fn offer_or_again(&self, id: JobId, job: Job, found: Option<Have>, hint: Option<String>) {
        let all_run = job.inputs.iter().all(|p| runs_with(&job.what, &name_of(p), found));
        if all_run && !job.resubmitted {
            self.0.ops.forget(id);
            self.start(Job { resubmitted: true, ..job });
            return;
        }
        let need = need_for(&job.what, &job.inputs, found);
        let again = self.again(Job { resubmitted: false, ..job });
        crate::archives::with_current(|archives| archives.offer_ffmpeg(need, hint, again, Some(id)));
    }

    /// Says `text` in the status bar until the selection changes.
    fn note(&self, text: String) {
        crate::view::with_current(|view| view.note(text));
    }

    /// A command asked for by its key: on the items it takes; when it takes none or cannot
    /// run, the status bar says why (spec 7).
    fn run_checked(&self, spec: CommandSpec, items: Vec<(PathBuf, bool)>) {
        if items.is_empty() {
            return self.note(format!("Select the items to run {} on", spec.name));
        }
        match command_state(&spec, &items, found_program(&program_of(&spec)), cfg!(windows)) {
            CommandState::Hidden => self.note(not_for_text(&spec)),
            CommandState::Off(tip) => self.note(format!("{} — {tip}", spec.name)),
            CommandState::Ready => self.run_asked(spec, items),
        }
    }

    /// Runs `spec` on the items it takes, asking first when it says so (`ask`). A `{files}`
    /// run that does not fit one command line is refused before the question.
    fn run_asked(&self, spec: CommandSpec, items: Vec<(PathBuf, bool)>) {
        let taken: Vec<(PathBuf, bool)> =
            items.into_iter().filter(|(path, is_dir)| command_applies(&spec, &name_of(path), *is_dir)).collect();
        if taken.is_empty() {
            return;
        }
        if uses_files(&spec) && !fits_one_run(&spec, &taken) {
            return self.note(too_many_text(&spec.name));
        }
        if !spec.ask {
            return self.run_command(spec, taken);
        }
        let this = self.clone();
        let message = ask_text(&spec.name, taken.len());
        self.0.dialogs.ask(spec.name.clone(), message, &["Run", "Cancel"], move |answer| {
            if answer == Some(0) {
                this.run_command(spec, taken);
            }
        });
    }

    /// Runs a user command on the `items` it takes, as one job undone as one action (a
    /// `{files}` run: nothing to undo, which the panel says).
    fn run_command(&self, spec: CommandSpec, items: Vec<(PathBuf, bool)>) {
        let items: Vec<(PathBuf, bool)> =
            items.into_iter().filter(|(p, d)| command_applies(&spec, &name_of(p), *d)).collect();
        if items.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = items.iter().map(|(p, _)| p.clone()).collect();
        let label = format!("{} on {}", spec.name, items_text(&paths));
        let again: Rc<dyn Fn()> = {
            let (spec, items) = (spec.clone(), items.clone());
            Rc::new(move || {
                let (spec, items) = (spec.clone(), items.clone());
                with_current(|this| this.run_command(spec, items));
            })
        };
        self.0.ops.remember_for(&paths);
        let once = uses_files(&spec);
        let task: Box<dyn gezik_ops::Task> = Box::new(CommandTask::new(items, spec));
        if once {
            // What it did is not known: nothing to undo (spec 7), and the panel says so.
            let id = self.0.ops.submit_chain(vec![task], None, Some(again), After::Nothing);
            self.0.ops.set_note(id, CANT_UNDO);
        } else {
            self.0.ops.submit_chain(vec![task], Some(label), Some(again), After::Select);
        }
    }
}

/// What Convert starts.
enum Ready {
    Command(CommandSpec, Vec<(PathBuf, bool)>),
    Convert(Job),
    Pdf(PdfJob, PdfChoices),
}

#[cfg(test)]
mod tests {
    use gezik_core::batch::convert::{MediaPreset, preset};

    use super::*;

    fn file(name: &str) -> (PathBuf, bool) {
        (PathBuf::from("d").join(name), false)
    }

    fn folder(name: &str) -> (PathBuf, bool) {
        (PathBuf::from("d").join(name), true)
    }

    fn spec(name: &str, run: &[&str], output: Option<&str>, types: &[&str], folders: bool) -> CommandSpec {
        CommandSpec {
            name: name.to_owned(),
            run: run.iter().map(|s| (*s).to_owned()).collect(),
            output: output.map(str::to_owned),
            types: types.iter().map(|s| (*s).to_owned()).collect(),
            folders,
            parallel: 1,
            shortcut: None,
            menu: None,
            ask: false,
        }
    }

    #[test]
    fn the_most_common_kind_comes_first() {
        let items = [file("a.txt"), file("b.jpg"), file("c.mp4"), file("d.MP3"), folder("e"), file("f.exe")];
        // A picture makes a PDF too: its group comes after the pictures'.
        assert_eq!(kinds_for(&items), [Kind::Media, Kind::Image, Kind::Pdf, Kind::Text]);
        // A tie keeps pictures, text, audio/video.
        assert_eq!(kinds_for(&[file("a.csv"), file("b.heic")]), [Kind::Image, Kind::Text]);
        assert!(kinds_for(&[folder("x"), file("setup.exe"), file("README")]).is_empty());
        assert!(looks_text("Notes.TXT") && looks_text("sub.srt") && !looks_text(".txt") && !looks_text("a.docx"));
    }

    #[test]
    fn menu_offers_convert_and_the_commands_that_apply() {
        let resize = spec(
            "Resize to 50%",
            &["magick", "{in}", "-resize", "50%", "{out}"],
            Some("{name}-small.{ext}"),
            &["jpg"],
            false,
        );
        let pdf = spec("Office to PDF", &["soffice", "{in}"], Some("{name}.pdf"), &["docx"], false);
        let touch = spec("Touch", &["touch", "{in}"], None, &[], true);
        let commands = [resize.clone(), pdf, touch.clone()];
        let items = [file("a.jpg"), folder("b")];
        let states: Vec<CommandState> = commands.iter().map(|c| command_state(c, &items, Some(true), false)).collect();
        assert_eq!(states[0], CommandState::Ready);
        assert_eq!(states[1], CommandState::Hidden);
        // In place on a folder is refused by the task: greyed.
        assert_eq!(states[2], CommandState::Off("changes items in place: not for folders".to_owned()));
        let (top, sub) = menu_entries(&items, &commands, &states);
        // A picture can also become a PDF.
        assert_eq!(top, [(CONVERT, "Convert…".to_owned()), (IMAGES_TO_PDF, "Images to PDF…".to_owned())]);
        let sub = sub.unwrap();
        // By index in settings.toml: the hidden one keeps its id free.
        assert_eq!(sub[0], (COMMAND_FIRST, "Resize to 50%".to_owned(), true));
        assert_eq!(sub[1].0, COMMAND_FIRST + 2);
        assert!(!sub[1].2 && sub[1].1.starts_with("Touch — "), "{:?}", sub[1]);

        // Nothing convertible and no command: no items at all.
        let hidden = [CommandState::Hidden, CommandState::Hidden];
        let (top, sub) = menu_entries(&[file("setup.exe")], &commands[..2], &hidden);
        assert!(top.is_empty() && sub.is_none());
        // Only a command applies: Convert… opens its group.
        let states = [CommandState::Hidden, CommandState::Ready];
        assert_eq!(menu_entries(&[file("a.docx")], &commands[..2], &states).0.len(), 1);

        // A program that was not found is greyed with a tip; one not looked up yet is not.
        let missing = command_state(&resize, &[file("a.jpg")], Some(false), false);
        assert_eq!(missing, CommandState::Off("magick not found".to_owned()));
        assert_eq!(command_state(&resize, &[file("a.jpg")], None, false), CommandState::Ready);
        assert_eq!(command_state(&touch, &[file("a.jpg")], Some(true), false), CommandState::Ready);
    }

    #[test]
    fn a_missing_program_says_how_to_name_it() {
        assert_eq!(not_found_tip("magick", false), "magick not found");
        assert_eq!(not_found_tip("magick", true), "magick not found (use the full name, e.g. magick.cmd)");
        assert_eq!(not_found_tip("tool.bat", true), "tool.bat not found");
        assert_eq!(not_found_tip(r"C:\bin\tool", true), r"C:\bin\tool not found");
    }

    #[test]
    fn the_preset_list_never_outgrows_its_ids() {
        let kinds = [Kind::Image, Kind::Text, Kind::Media, Kind::Pdf];
        let commands: Vec<CommandSpec> =
            (0..COMMAND_MAX as usize).map(|i| spec(&format!("C{i}"), &["c", "{in}"], None, &[], false)).collect();
        let states = vec![CommandState::Ready; commands.len()];
        let (choices, rows) = preset_list(&kinds, &PdfOp::ALL, &commands, &states);
        assert_eq!(choices.len(), CONVERT_PRESET_MAX as usize, "the commands fill it up to the last id");
        let menu = preset_menu(&rows, 0);
        let last = menu.iter().map(|(id, _, _)| *id).max().unwrap();
        assert!(last < CONVERT_PRESET_FIRST + CONVERT_PRESET_MAX);
        assert_eq!(menu.iter().filter(|(id, _, _)| *id != HEADING).count(), choices.len(), "every choice has an id");
    }

    #[test]
    fn the_preset_list_follows_the_selection() {
        let items = [file("a.txt"), file("b.txt"), file("c.png")];
        let kinds = kinds_for(&items);
        let commands = [spec("Lint", &["lint", "{in}"], None, &["txt"], false)];
        let off = [CommandState::Off("lint not found".to_owned())];
        let (choices, rows) = preset_list(&kinds, &pdf_ops_for(&items), &commands, &off);
        assert_eq!(rows[0], Row::Heading("Text"));
        assert_eq!(choices[0], Choice::Preset(preset("to-utf8").unwrap()));
        let images = PRESETS.iter().filter(|p| p.kind == Kind::Image).count();
        // Text, Image, PDF ("Images to PDF") and the command.
        assert_eq!(choices.len(), 3 + images + 1 + 1);
        assert_eq!(*choices.last().unwrap(), Choice::Command(0));
        assert!(!rows.contains(&Row::Heading("Audio/Video")));

        let menu = preset_menu(&rows, 1);
        assert_eq!(menu[0], (HEADING, "Text".to_owned(), false));
        assert_eq!(menu[1], (CONVERT_PRESET_FIRST, "    Convert to UTF-8".to_owned(), true));
        assert_eq!(menu[2].1, "• Windows line endings (CRLF)");
        let last = menu.last().unwrap();
        assert_eq!(last.0, CONVERT_PRESET_FIRST + choices.len() as u32 - 1);
        assert!(!last.2 && last.1.contains("lint not found"));
        assert_eq!(menu.iter().filter(|(id, _, _)| *id == HEADING).count(), 4);

        // The last choice when it is there and can be chosen, else the first that can.
        let enabled = enabled_of(&rows);
        let png = choices.iter().position(|c| *c == Choice::Preset(preset("to-png").unwrap()));
        assert_eq!(first_choice(&choices, &enabled, &commands, &["to-png"]), png);
        assert_eq!(first_choice(&choices, &enabled, &commands, &["command:Lint"]), Some(0));
        assert_eq!(first_choice(&choices, &enabled, &commands, &["mp3"]), Some(0));
        assert_eq!(first_choice(&choices, &[false; 3], &commands, &[] as &[&str]), None);
    }

    #[test]
    fn skipped_items_are_counted_and_named() {
        let items = [file("a.jpg"), file("b.txt"), file("c.mp4"), folder("d"), file("e.gif")];
        let utf8 = preset("to-utf8").unwrap();
        let image = convert_what(preset("to-png").unwrap(), ImageOptions::DEFAULT, &preset_text(utf8));
        let (taken, skipped) = split_inputs(&items, &image);
        assert_eq!(taken, [items[0].0.clone(), items[4].0.clone()]);
        assert_eq!(skipped_text(skipped, Some(&image)), "3 items skipped: not images");
        // Remove location data keeps the format: a GIF cannot be written.
        let location = convert_what(preset("remove-location").unwrap(), ImageOptions::DEFAULT, &preset_text(utf8));
        assert!(matches!(location, ConvertWhat::RemoveLocation(_)));
        assert_eq!(split_inputs(&items, &location).0, [items[0].0.clone()]);
        let text = convert_what(utf8, ImageOptions::DEFAULT, &preset_text(utf8));
        let (taken, skipped) = split_inputs(&items, &text);
        assert_eq!(taken, [items[1].0.clone()]);
        assert_eq!(skipped_text(skipped, Some(&text)), "4 items skipped: not text files");
        let media = ConvertWhat::Media(MediaPreset::Mp3);
        assert_eq!(skipped_text(split_inputs(&items, &media).1, Some(&media)), "4 items skipped: not audio or video");
        assert_eq!(skipped_text(1, Some(&image)), "1 item skipped: not an image");
        assert_eq!(skipped_text(0, Some(&image)), "");
        assert_eq!(skipped_text(2, None), "2 items skipped: not for this command");
    }

    #[test]
    fn footer_says_when_ffmpeg_is_needed() {
        assert_eq!(ffmpeg_note(Kind::Image, false, Found::Unknown), "¹ needs ffmpeg");
        assert_eq!(ffmpeg_note(Kind::Image, false, Found::Missing), "¹ needs ffmpeg");
        assert_eq!(ffmpeg_note(Kind::Image, false, Found::Version(Some((9, 0)))), "");
        assert_eq!(ffmpeg_note(Kind::Image, true, Found::Missing), "");
        assert_eq!(ffmpeg_note(Kind::Media, false, Found::Missing), "Needs ffmpeg");
        assert_eq!(ffmpeg_note(Kind::Media, false, Found::Unknown), "");
        assert_eq!(ffmpeg_note(Kind::Text, false, Found::Missing), "");
        assert_eq!(ffmpeg_note(Kind::Command, false, Found::Missing), "");
    }

    /// What a preset converts with its own options.
    fn what_of(id: &str) -> ConvertWhat {
        let preset = preset(id).unwrap();
        convert_what(preset, preset_image(preset), &preset_text(preset))
    }

    fn have(version: Option<(u32, u32)>) -> Option<Have> {
        Some(Have { version, configured: false })
    }

    #[test]
    fn what_needs_ffmpeg_waits_for_it() {
        let (jpg, heic) = (PathBuf::from("d/a.jpg"), PathBuf::from("d/b.HEIC"));
        let to_jpeg = what_of("to-jpeg");
        let both = vec![jpg.clone(), heic.clone()];
        let none = split_by_ffmpeg(&to_jpeg, both.clone(), None);
        assert_eq!(none, (vec![jpg.clone()], vec![heic.clone()], Some(Need::Heic)));
        // An ffmpeg older than 9 (or of unknown version) does not read HEIC.
        assert_eq!(split_by_ffmpeg(&to_jpeg, both.clone(), have(Some((6, 1)))).2, Some(Need::NewerFfmpeg));
        // A build from git: its version is unknown, not older.
        assert_eq!(split_by_ffmpeg(&to_jpeg, both.clone(), have(None)).2, Some(Need::UnknownFfmpeg));
        // One of 9 or newer that failed to read them anyway (tried again): not "older".
        assert_eq!(need_for(&to_jpeg, &both, have(Some((9, 0)))), Need::FfmpegFailed);
        let unknown_configured = Some(Have { version: None, configured: true });
        assert_eq!(need_for(&to_jpeg, &both, unknown_configured), Need::ConfiguredUnknown);
        // One set in settings.toml comes before a download: the box says so.
        let configured = Some(Have { version: Some((7, 1)), configured: true });
        assert_eq!(split_by_ffmpeg(&to_jpeg, both.clone(), configured).2, Some(Need::ConfiguredTooOld));
        assert_eq!(split_by_ffmpeg(&to_jpeg, both.clone(), have(Some((9, 0)))), (both, Vec::new(), None));
        // Writing AVIF takes any ffmpeg.
        let avif = what_of("to-avif");
        assert_eq!(split_by_ffmpeg(&avif, vec![jpg.clone()], None).2, Some(Need::Pictures));
        assert!(split_by_ffmpeg(&avif, vec![jpg.clone()], have(Some((6, 0)))).1.is_empty());
        let mp3 = what_of("mp3");
        let video = vec![PathBuf::from("d/v.mkv")];
        assert_eq!(split_by_ffmpeg(&mp3, video.clone(), None), (Vec::new(), video.clone(), Some(Need::Media)));
        assert_eq!(split_by_ffmpeg(&mp3, video.clone(), have(None)).0, video);
        assert_eq!(split_by_ffmpeg(&what_of("to-utf8"), vec![PathBuf::from("a.txt")], None).2, None);
        // Remove location data: only an AVIF (kept as AVIF) or a HEIC needs it.
        let location = what_of("remove-location");
        assert!(!needs_ffmpeg(&location, "a.webp") && needs_ffmpeg(&location, "a.avif"));
    }

    #[test]
    fn the_chosen_format_decides_on_ffmpeg_not_the_preset() {
        // "Resize photos" writes JPEG; the format button changed it to lossy WebP.
        let resize = preset("resize-photos").unwrap();
        let lossy = ImageOptions { format: ImageFormat::WebpLossy, ..preset_image(resize) };
        let what = convert_what(resize, lossy, &preset_text(resize));
        let png = vec![PathBuf::from("d/alpha.png")];
        assert!(needs_ffmpeg(&what, "alpha.png"));
        assert_eq!(split_by_ffmpeg(&what, png.clone(), None), (Vec::new(), png.clone(), Some(Need::Pictures)));
        assert_eq!(split_by_ffmpeg(&what, png.clone(), have(Some((9, 0)))).0, png);
        // And the other way: "Convert to AVIF" switched to PNG needs none.
        let avif = preset("to-avif").unwrap();
        let what =
            convert_what(avif, ImageOptions { format: ImageFormat::Png, ..preset_image(avif) }, &preset_text(avif));
        assert!(!needs_ffmpeg(&what, "alpha.png"));
    }

    #[test]
    fn convert_needs_something_to_run() {
        let lint = spec("Lint", &["lint", "{in}"], None, &["exe"], false);
        let off = [CommandState::Off("lint not found".to_owned())];
        // A greyed command alone: the submenu shows it, but no Convert….
        let (top, sub) = menu_entries(&[file("setup.exe")], std::slice::from_ref(&lint), &off);
        assert!(top.is_empty(), "{top:?}");
        assert_eq!(sub.map(|s| s.len()), Some(1));
        let (top, _) = menu_entries(&[file("setup.exe")], &[lint], &[CommandState::Ready]);
        assert_eq!(top.len(), 1);
    }

    #[test]
    fn each_kind_remembers_its_preset() {
        let state = ConvertState {
            last_preset: Some("to-utf8".to_owned()),
            image: Some("preset=to-jpeg format=jpeg strip=yes".to_owned()),
            text: Some("preset=to-utf8 from=detect".to_owned()),
            media: Some("mp3".to_owned()),
            ..ConvertState::default()
        };
        assert_eq!(saved_preset("preset=to-jpeg format=jpeg"), Some("to-jpeg"));
        assert_eq!(saved_preset("format=jpeg"), None);
        assert_eq!(wanted_presets(&state, &[Kind::Media, Kind::Image]), ["to-utf8", "mp3", "to-jpeg"]);
        // Pictures after a text conversion: the last picture preset, not the first one.
        let kinds = [Kind::Image];
        let (choices, rows) = preset_list(&kinds, &[], &[], &[]);
        let at = first_choice(&choices, &enabled_of(&rows), &[], &wanted_presets(&state, &kinds));
        assert_eq!(at.map(|i| choices[i]), Some(Choice::Preset(preset("to-jpeg").unwrap())));
    }

    /// The rows' titles, headings as `# PDF`.
    fn titles(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Heading(title) => format!("# {title}"),
                Row::Choice(title, _) => title.clone(),
            })
            .collect()
    }

    fn pdf_rows(items: &[(PathBuf, bool)]) -> (Vec<Choice>, Vec<String>) {
        let (choices, rows) = preset_list(&kinds_for(items), &pdf_ops_for(items), &[], &[]);
        (choices, titles(&rows))
    }

    #[test]
    fn pdfs_get_the_pdf_group() {
        let one = [file("a.pdf")];
        assert_eq!(kinds_for(&one), [Kind::Pdf]);
        let (top, sub) = menu_entries(&one, &[], &[]);
        assert_eq!(top, [(CONVERT, "Convert…".to_owned())]);
        assert!(sub.is_none());
        let (choices, rows) = pdf_rows(&one);
        assert_eq!(rows, ["# PDF", "Split PDF", "PDF to images", "Extract pages"]);
        assert_eq!(choices[0], Choice::Pdf(PdfOp::Split));
        // Two PDFs can be merged too.
        let (_, rows) = pdf_rows(&[file("a.pdf"), file("b.PDF")]);
        assert_eq!(rows, ["# PDF", "Merge PDFs", "Split PDF", "PDF to images", "Extract pages"]);
        // A folder named like a PDF is not one.
        assert!(kinds_for(&[folder("x.pdf")]).is_empty());
        assert!(menu_entries(&[folder("x.pdf")], &[], &[]).0.is_empty());
    }

    #[test]
    fn mixed_selections_get_both_groups() {
        // More pictures: the pictures' group first, the PDF group right after it.
        let items = [file("a.jpg"), file("b.png"), file("c.pdf"), file("d.txt")];
        assert_eq!(kinds_for(&items), [Kind::Image, Kind::Pdf, Kind::Text]);
        let (choices, rows) = pdf_rows(&items);
        let at = rows.iter().position(|r| r == "# PDF").unwrap();
        assert_eq!(rows[at + 1..at + 5], ["Images to PDF", "Split PDF", "PDF to images", "Extract pages"]);
        assert!(choices.contains(&Choice::Pdf(PdfOp::ImagesToPdf)));
        // More PDFs: the PDF group first.
        let items = [file("a.jpg"), file("b.pdf"), file("c.pdf")];
        assert_eq!(kinds_for(&items), [Kind::Pdf, Kind::Image]);
        assert_eq!(pdf_rows(&items).1[..3], ["# PDF", "Images to PDF", "Merge PDFs"]);
        // A tie keeps the pictures first.
        assert_eq!(kinds_for(&[file("a.jpg"), file("b.pdf")]), [Kind::Image, Kind::Pdf]);
    }

    #[test]
    fn pictures_offer_images_to_pdf_in_the_menu() {
        let (top, _) = menu_entries(&[file("a.jpg"), file("b.heic"), folder("c")], &[], &[]);
        assert_eq!(top, [(CONVERT, "Convert…".to_owned()), (IMAGES_TO_PDF, "Images to PDF…".to_owned())]);
        // HEIC is read through ffmpeg only: no PDF from it, no PDF group.
        let heic = [file("b.HEIC")];
        assert_eq!(menu_entries(&heic, &[], &[]).0, [(CONVERT, "Convert…".to_owned())]);
        assert_eq!(kinds_for(&heic), [Kind::Image]);
        assert!(pdf_ops_for(&heic).is_empty());
        // A PDF alone: no "Images to PDF…".
        assert!(!menu_entries(&[file("a.pdf")], &[], &[]).0.iter().any(|(id, _)| *id == IMAGES_TO_PDF));
    }

    #[test]
    fn the_pdf_group_remembers_its_operation() {
        let items = [file("a.pdf"), file("b.pdf")];
        let kinds = kinds_for(&items);
        let (choices, rows) = preset_list(&kinds, &pdf_ops_for(&items), &[], &[]);
        let state = ConvertState { pdf: Some("op=merge dpi=300".to_owned()), ..ConvertState::default() };
        assert_eq!(wanted_presets(&state, &kinds), ["pdf:merge-pdfs"]);
        let at = first_choice(&choices, &enabled_of(&rows), &[], &wanted_presets(&state, &kinds));
        assert_eq!(at.map(|i| choices[i]), Some(Choice::Pdf(PdfOp::Merge)));
        // The last one used comes first; "Images to PDF…" asks for its own.
        let last = ConvertState { last_preset: Some("pdf:extract-pages".to_owned()), ..state };
        let at = first_choice(&choices, &enabled_of(&rows), &[], &wanted_presets(&last, &kinds));
        assert_eq!(at.map(|i| choices[i]), Some(Choice::Pdf(PdfOp::Extract)));
        assert_eq!(Choice::Pdf(PdfOp::ImagesToPdf).key(&[]), "pdf:images-to-pdf");
    }

    #[test]
    fn the_split_is_read_from_the_buttons_and_fields() {
        assert_eq!(split_of(SplitChoice::EachPage, "x", ""), Ok(Split::EachPage));
        assert_eq!(split_of(SplitChoice::Every, " 5 ", ""), Ok(Split::Every(5)));
        assert_eq!(split_of(SplitChoice::Every, "0", ""), Err("Type how many pages go in each file".to_owned()));
        assert_eq!(split_of(SplitChoice::Ranges, "", " 1-3, 5 "), Ok(Split::Ranges("1-3, 5".to_owned())));
        assert!(every_from("abc").is_err() && every_from("1000001").is_err());
    }

    #[test]
    fn the_preset_says_when_its_options_changed() {
        let resize = preset("resize-photos").unwrap();
        let own = preset_image(resize);
        let text = preset_text(resize);
        assert!(!options_changed(resize, &own, &text));
        assert!(options_changed(resize, &ImageOptions { format: ImageFormat::WebpLossy, ..own }, &text));
        assert!(options_changed(resize, &ImageOptions { strip_metadata: true, ..own }, &text));
        // Quality does not show for PNG: changing it there changes nothing.
        let png = preset("to-png").unwrap();
        assert!(!options_changed(png, &ImageOptions { quality: 10, ..preset_image(png) }, &text));
        let utf8 = preset("to-utf8").unwrap();
        let bom = TextOptions { bom: true, ..preset_text(utf8) };
        assert!(options_changed(utf8, &preset_image(utf8), &bom));
        assert!(!options_changed(preset("mp3").unwrap(), &own, &text));
    }

    #[test]
    fn detected_encodings_read_well() {
        let text = detected_text(&[("UTF-8", 1), ("Windows-1254", 11)]);
        assert_eq!(text, "Detected: Windows-1254 (11 files), UTF-8 (1 file)");
        assert_eq!(detected_text(&[("UTF-8", 0)]), "");
    }

    #[test]
    fn options_round_trip_through_state() {
        let options = ImageOptions {
            format: ImageFormat::WebpLossy,
            quality: 70,
            resize: Resize::Percent(50),
            never_enlarge: false,
            rotate_by_exif: false,
            strip_metadata: true,
            background: [0x12, 0xab, 0xff],
        };
        let text = image_options_text(&options);
        assert_eq!(
            text,
            "format=webp-lossy quality=70 resize=percent:50 never-enlarge=no rotate=no strip=yes background=#12abff"
        );
        assert_eq!(image_options_from(&text, ImageOptions::DEFAULT), options);
        let junk = image_options_from("format=gif quality=0 resize=width:x junk", ImageOptions::DEFAULT);
        assert_eq!(junk, ImageOptions::DEFAULT);

        let base = preset_text(&PRESETS[0]);
        let options = TextOptions {
            from: Some("windows-1254".to_owned()),
            to: String::new(),
            bom: true,
            eol: Eol::Crlf,
            trim_trailing: true,
            final_newline: true,
        };
        let text = text_options_text(&options);
        assert_eq!(text, "from=windows-1254 to=keep bom=yes eol=crlf trim=yes final-newline=yes");
        assert_eq!(text_options_from(&text, base.clone()), options);
        assert_eq!(text_options_from("from=klingon to=nope eol=x", base.clone()), base);

        assert_eq!(OutputChoice::from_key(OutputChoice::Replace.key()), Some(OutputChoice::Replace));
        assert_eq!(OutputChoice::from_key(""), None);
    }

    #[test]
    fn typed_numbers_are_checked() {
        assert_eq!(resize_from(0, "x"), Ok(Resize::None));
        assert_eq!(resize_from(1, " 1920 "), Ok(Resize::Longest(1920)));
        assert_eq!(resize_from(4, "50"), Ok(Resize::Percent(50)));
        assert_eq!(resize_from(2, "0"), Err("Type the size in pixels".to_owned()));
        assert_eq!(resize_from(4, ""), Err("Type the size in percent".to_owned()));
        assert_eq!(quality_from("85"), Ok(85));
        assert!(quality_from("0").is_err() && quality_from("101").is_err());
        assert_eq!(background_from("#FFffff"), Ok([255, 255, 255]));
        assert!(background_from("#ffffff80").is_err() && background_from("white").is_err());
        assert_eq!(resize_choice(Resize::Height(600)), (3, "600".to_owned()));
        assert_eq!(background_text([255, 0, 16]), "#ff0010");
    }

    #[test]
    fn commands_with_a_menu_name_are_grouped_under_it() {
        let grouped = |name: &str, run: &[&str], group: &str| CommandSpec {
            menu: Some(group.to_owned()),
            ..spec(name, run, None, &[], true)
        };
        let commands = vec![
            grouped("Zip", &["7z", "{files}"], "Archives"),
            CommandSpec {
                menu: Some("Images".into()),
                ..spec("Small", &["m", "{in}", "{out}"], Some("{name}-s.{ext}"), &["jpg"], false)
            },
            spec("Plain", &["x", "{in}"], None, &[], false),
            grouped("Tar", &["tar", "{files}"], "Archives"),
        ];
        let states = vec![CommandState::Ready; 4];
        let (_, sub) = menu_entries(&[file("a.jpg")], &commands, &states);
        let lines: Vec<(u32, String, bool)> = sub.unwrap();
        assert_eq!(
            lines,
            [
                (COMMAND_FIRST + 2, "Plain".to_owned(), true),
                (COMMAND_GROUP, "Archives".to_owned(), false),
                (COMMAND_FIRST, "Zip".to_owned(), true),
                (COMMAND_FIRST + 3, "Tar".to_owned(), true),
                (COMMAND_GROUP, "Images".to_owned(), false),
                (COMMAND_FIRST + 1, "Small".to_owned(), true),
            ]
        );
    }

    #[test]
    fn a_files_command_is_not_greyed_for_folders() {
        let zip = spec("Zip", &["7z", "a", "x.zip", "{files}"], None, &[], true);
        assert_eq!(command_state(&zip, &[folder("site"), file("a.txt")], Some(true), false), CommandState::Ready);
        let in_place = spec("Touch", &["touch", "{in}"], None, &[], true);
        assert!(matches!(command_state(&in_place, &[folder("site")], Some(true), false), CommandState::Off(_)));
    }

    #[test]
    fn why_a_command_does_not_run_and_what_it_asks() {
        assert_eq!(
            not_for_text(&spec("Small", &["x"], None, &["jpg", "PNG"], false)),
            "Small runs on .jpg and .png files only"
        );
        assert_eq!(
            not_for_text(&spec("Small", &["x"], None, &["jpg", "png", "gif"], false)),
            "Small runs on .jpg, .png and .gif files only"
        );
        assert_eq!(
            not_for_text(&spec("Small", &["x"], None, &["jpg"], true)),
            "Small runs on .jpg files and folders only"
        );
        assert_eq!(not_for_text(&spec("Touch", &["x"], None, &[], false)), "Touch does not run on folders");
        assert_eq!(ask_text("Zip", 1), "Run Zip on 1 item?");
        assert_eq!(ask_text("Zip", 1234), "Run Zip on 1,234 items?");
    }

    #[test]
    fn the_menu_bar_lists_the_commands_with_a_key() {
        let zip = CommandSpec { menu: Some("Archives".into()), ..spec("Zip", &["7z", "{files}"], None, &[], true) };
        let commands = vec![spec("A", &["x"], None, &[], false), zip, spec("B", &["x"], None, &[], false)];
        let key = |i: usize| (i != 0).then(|| format!("⌃⌥{i}"));
        assert_eq!(
            bar_entries(&commands, key),
            [(2, "B    ⌃⌥2".to_owned(), true), (-1, "Archives".to_owned(), false), (1, "Zip    ⌃⌥1".to_owned(), true)]
        );
    }

    #[test]
    fn very_long_command_and_group_names_are_cut_in_menus() {
        let long = "x".repeat(200);
        let shown = shown_name(&long);
        assert_eq!(shown.chars().count(), MENU_NAME_MAX);
        assert!(shown.ends_with('…') && shown.starts_with("xxx"));
        assert_eq!(shown_name("Zip"), "Zip");
        assert_eq!(shown_name(&"ş".repeat(MENU_NAME_MAX)), "ş".repeat(MENU_NAME_MAX), "counted in characters");
        let commands = vec![CommandSpec { menu: Some(long.clone()), ..spec(&long, &["x", "{in}"], None, &[], false) }];
        let (_, sub) = menu_entries(&[file("a.txt")], &commands, &[CommandState::Ready]);
        let sub = sub.unwrap();
        assert_eq!((sub[0].1.chars().count(), sub[1].1.chars().count()), (MENU_NAME_MAX, MENU_NAME_MAX));
        let bar = bar_entries(&commands, |_| Some("F5".to_owned()));
        assert_eq!(bar[1].1, format!("{shown}    F5"), "the key stays in view");
    }
}
