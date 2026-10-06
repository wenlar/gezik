//! Converting in the app: the Convert… and Commands ▸ menu items, the Convert layer, and the
//! offer to download ffmpeg (archives.rs's tool box) when a conversion needs it. The work runs
//! as engine jobs (gezik-batch's `ConvertTask` and `CommandTask`); the UI thread never reads
//! the disk: menus decide by name, and finding programs and ffmpeg, the trash check and the
//! text detection run on threads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_batch::convert::ffmpeg::{Ffmpeg, find_ffmpeg};
use gezik_batch::convert::text::{Detected, SNIFF, detect, encodings};
use gezik_batch::tasks::{
    CommandTask, ConvertTask, ConvertTools, ConvertWhat, find_program, is_ffmpeg_needed, skipped_inputs,
};
use gezik_config::Color;
use gezik_config::settings::{ConvertSettings, ConvertState};
use gezik_config::shortcuts::{Chord, Key, Platform as KeyPlatform};
use gezik_config::store::ConfigStore;
use gezik_core::batch::convert::{
    CommandSpec, Eol, ImageFormat, ImageOptions, Kind, Output, PRESETS, Preset, PresetWhat, Resize, TextOptions,
    command_applies, image_inputs, media_inputs, needs_ffmpeg_to_read,
};
use gezik_ops::{JobId, Report};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::archives::{Need, resolve_folder};
use crate::context_menu::{
    COMMAND_FIRST, COMMAND_MAX, CONVERT, CONVERT_PRESET_FIRST, CONVERT_PRESET_MAX, ENCODING_FROM_FIRST, ENCODING_MAX,
    ENCODING_TO_FIRST, HEADING,
};
use crate::dialog::Dialogs;
use crate::operations::{After, Operations, items_text};
use crate::{AppWindow, ConvertView};

/// The oldest ffmpeg that reads HEIC, HEIF and AVIF right (gezik-batch's image.rs).
const FFMPEG_TO_READ: (u32, u32) = (9, 0);

/// How many text files the layer reads to say which encodings they are in.
const DETECT_MAX: usize = 200;

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
/// tie: pictures, text, audio/video).
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
    // A stable sort keeps the tie order.
    counts.sort_by_key(|a| std::cmp::Reverse(a.1));
    counts.into_iter().filter(|(_, n)| *n > 0).map(|(k, _)| k).collect()
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
    if spec.output.is_none() && taken.iter().any(|(_, is_dir)| *is_dir) {
        return CommandState::Off("changes items in place: not for folders".to_owned());
    }
    if found == Some(false) {
        return CommandState::Off(not_found_tip(&program_of(spec), windows));
    }
    CommandState::Ready
}

/// A command's menu title and whether it can be chosen.
fn command_title(spec: &CommandSpec, state: &CommandState) -> (String, bool) {
    match state {
        CommandState::Off(tip) => (format!("{} — {tip}", spec.name), false),
        _ => (spec.name.clone(), true),
    }
}

/// A menu's top items (id, title) and its "Commands ▸" items (id, title, enabled), if any.
pub type MenuItems = (Vec<(u32, String)>, Option<Vec<(u32, String, bool)>>);

/// "Convert…" and the "Commands ▸" items for `items`, given the commands and how each shows.
/// No "Convert…" when nothing can be converted; no submenu without commands to show.
pub fn menu_entries(items: &[(PathBuf, bool)], commands: &[CommandSpec], states: &[CommandState]) -> MenuItems {
    let sub: Vec<(u32, String, bool)> = commands
        .iter()
        .zip(states)
        .enumerate()
        .take(COMMAND_MAX as usize)
        .filter(|(_, (_, state))| **state != CommandState::Hidden)
        .map(|(i, (spec, state))| {
            let (title, enabled) = command_title(spec, state);
            (COMMAND_FIRST + i as u32, title, enabled)
        })
        .collect();
    let convertible = items.iter().any(|(path, is_dir)| kind_of(&name_of(path), *is_dir).is_some());
    let mut top = Vec::new();
    if convertible || !sub.is_empty() {
        top.push((CONVERT, "Convert…".to_owned()));
    }
    (top, (!sub.is_empty()).then_some(sub))
}

/// One entry of the layer's preset list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Preset(&'static Preset),
    /// A user command, by its index in the layer's commands.
    Command(usize),
}

impl Choice {
    fn kind(self) -> Kind {
        match self {
            Choice::Preset(preset) => preset.kind,
            Choice::Command(_) => Kind::Command,
        }
    }

    /// How state.toml names it: a preset's id, `command:<name>`.
    fn key(self, commands: &[CommandSpec]) -> String {
        match self {
            Choice::Preset(preset) => preset.id.to_owned(),
            Choice::Command(i) => format!("command:{}", commands.get(i).map_or("", |c| c.name.as_str())),
        }
    }
}

fn group_title(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "Image",
        Kind::Text => "Text",
        Kind::Media => "Audio/Video",
        Kind::Command => "Commands",
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

/// The layer's preset list for the groups `kinds` and the commands with their states: the
/// choices in order, and the lines of its menu.
pub fn preset_list(kinds: &[Kind], commands: &[CommandSpec], states: &[CommandState]) -> (Vec<Choice>, Vec<Row>) {
    let mut choices = Vec::new();
    let mut rows = Vec::new();
    for kind in kinds {
        rows.push(Row::Heading(group_title(*kind)));
        for preset in PRESETS.iter().filter(|p| p.kind == *kind) {
            choices.push(Choice::Preset(preset));
            rows.push(Row::Choice(preset.label.to_owned(), true));
        }
    }
    let shown: Vec<usize> =
        (0..commands.len().min(states.len())).filter(|i| states[*i] != CommandState::Hidden).collect();
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
fn first_choice(choices: &[Choice], enabled: &[bool], commands: &[CommandSpec], last: Option<&str>) -> Option<usize> {
    let usable = |i: &usize| enabled.get(*i).copied().unwrap_or(false);
    last.and_then(|last| (0..choices.len()).filter(usable).find(|i| choices[*i].key(commands) == last))
        .or_else(|| (0..choices.len()).find(usable))
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

/// Whether `preset` can run on `name` with ffmpeg as `found` (`None`: there is none).
fn runs_with(preset: &Preset, name: &str, found: Option<Option<(u32, u32)>>) -> bool {
    if !preset.needs_ffmpeg_for(name) {
        return true;
    }
    match found {
        None => false,
        Some(version) => !needs_ffmpeg_to_read(name) || version.is_some_and(|v| v >= FFMPEG_TO_READ),
    }
}

/// What the box says `preset` needs for `inputs`, with ffmpeg as `found`.
pub fn need_for(preset: &Preset, inputs: &[PathBuf], found: Option<Option<(u32, u32)>>) -> Need {
    if preset.kind == Kind::Media {
        Need::Media
    } else if inputs.iter().any(|p| needs_ffmpeg_to_read(&name_of(p))) {
        if found.is_some() { Need::NewerFfmpeg } else { Need::Heic }
    } else {
        Need::Pictures
    }
}

/// `inputs` split into those that run now and those that wait for ffmpeg (none, or older
/// than 9 for HEIC, HEIF and AVIF), with what those need.
pub fn split_by_ffmpeg(
    preset: &Preset,
    inputs: Vec<PathBuf>,
    found: Option<Option<(u32, u32)>>,
) -> (Vec<PathBuf>, Vec<PathBuf>, Option<Need>) {
    let (now, waiting): (Vec<PathBuf>, Vec<PathBuf>) =
        inputs.into_iter().partition(|path| runs_with(preset, &name_of(path), found));
    let need = (!waiting.is_empty()).then(|| need_for(preset, &waiting, found));
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

/// A conversion to start: what, on which inputs, to where.
#[derive(Clone)]
struct Job {
    inputs: Vec<PathBuf>,
    preset: &'static Preset,
    what: ConvertWhat,
    output: Output,
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
}

impl Layer {
    fn current(&self) -> Choice {
        self.choices[self.choice]
    }

    fn preset(&self) -> Option<&'static Preset> {
        match self.current() {
            Choice::Preset(preset) => Some(preset),
            Choice::Command(_) => None,
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
    with_current(|convert| convert.run_command(spec, rows));
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
    }

    /// Where downloaded tools are: `<config dir>/tools/` (as archives.rs has it).
    fn data_dir(&self) -> PathBuf {
        self.0.store.as_ref().map_or_else(|| std::env::temp_dir().join("gezik"), |store| store.dir().to_path_buf())
    }

    /// Writes the last choices to state.toml.
    fn save_state(&self, f: impl FnOnce(&mut ConvertState)) {
        f(&mut self.0.state.borrow_mut());
        if let Some(store) = &self.0.store {
            let mut saved = store.load_state();
            saved.convert = self.0.state.borrow().clone();
            if let Err(err) = store.save_state(&saved) {
                eprintln!("gezik: cannot save the conversion choices: {err}");
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.0.layer.borrow().is_some()
    }

    /// "Convert…": the layer for `items` (path, is a folder).
    pub fn open(&self, items: Vec<(PathBuf, bool)>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let Some(folder) = items.first().and_then(|(p, _)| p.parent()).map(Path::to_path_buf) else { return };
        if self.is_open() {
            return;
        }
        let commands = commands();
        let states = states_for(&commands, &items);
        let kinds = kinds_for(&items);
        let (choices, rows) = preset_list(&kinds, &commands, &states);
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
        };
        let state = self.0.state.borrow().clone();
        let Some(choice) =
            first_choice(&layer.choices, &layer.enabled(), &layer.commands, state.last_preset.as_deref())
        else {
            return;
        };
        layer.choice = choice;
        self.load_choice(&mut layer, None);
        // What was used last, when it was this preset.
        if let Some(preset) = layer.preset()
            && state.last_preset.as_deref() == Some(preset.id)
        {
            if let Some(text) = &state.image {
                layer.image = image_options_from(text, layer.image);
                layer.resize_mode = resize_choice(layer.image.resize).0;
            }
            if let Some(text) = &state.text {
                layer.text = text_options_from(text, layer.text.clone());
            }
            if let Some(output) = OutputChoice::from_key(&state.last_output) {
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
        let probe = (layer.items.clone(), layer.folder.clone(), kinds);
        *self.0.layer.borrow_mut() = Some(layer);
        self.show();
        window.set_cv_open(true);
        self.probe(probe.0, probe.1, &probe.2);
    }

    /// Looks on a thread at what the layer cannot know by names: whether the drive has a
    /// trash, which ffmpeg there is, what encodings the text files are in.
    fn probe(&self, items: Vec<(PathBuf, bool)>, folder: PathBuf, kinds: &[Kind]) {
        let opening = self.0.opening.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.0.opening.clone();
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
                (preset.label.to_owned(), kind, note)
            }
            Choice::Command(i) => {
                let spec = &layer.commands[i];
                (spec.name.clone(), 0, format!("Runs {}", spec.run.join(" ")))
            }
        };
        let what = layer.what();
        let skipped = match (&what, choice) {
            (Some(what), _) => split_inputs(&layer.items, what).1,
            (None, Choice::Command(i)) => {
                let spec = &layer.commands[i];
                layer.items.iter().filter(|(p, d)| !command_applies(spec, &name_of(p), *d)).count()
            }
            (None, _) => 0,
        };
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
            skipped: skipped_text(skipped, what.as_ref()).into(),
            ffmpeg_note: ffmpeg_note(choice.kind(), keeps_format, layer.found).into(),
            error: layer.error.as_str().into(),
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
                let image = match &job.what {
                    ConvertWhat::Image(options) => Some(image_options_text(options)),
                    _ => None,
                };
                let text = match &job.what {
                    ConvertWhat::Text(options) => Some(text_options_text(options)),
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
        }
    }

    fn check(&self, window: &AppWindow, layer: &Layer) -> Result<Ready, String> {
        let choice = layer.current();
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
        Ok(Ready::Convert(Job { inputs, preset, what, output }))
    }

    /// Finds ffmpeg on a thread if the job needs it, then starts what can run and offers
    /// ffmpeg for the rest.
    fn start(&self, job: Job) {
        let wants = job.inputs.iter().any(|p| job.preset.needs_ffmpeg_for(&name_of(p)));
        let (data, configured) = (self.data_dir(), configured_ffmpeg());
        std::thread::spawn(move || {
            let ffmpeg = if wants { find_ffmpeg(&data, configured.as_deref()) } else { None };
            let hint = if wants { gezik_platform::http::tool_missing_hint() } else { None };
            let found = ffmpeg.as_ref().map(|ff| ff.version);
            let runs_now = job.inputs.iter().any(|p| runs_with(job.preset, &name_of(p), found));
            if runs_now && let Output::Folder(dir) = &job.output {
                // A failure shows when the job cannot use the folder.
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| this.found(job, ffmpeg, hint));
            });
        });
    }

    fn found(&self, job: Job, ffmpeg: Option<Ffmpeg>, hint: Option<String>) {
        let found = ffmpeg.as_ref().map(|ff| ff.version);
        let (now, waiting, need) = split_by_ffmpeg(job.preset, job.inputs.clone(), found);
        if !now.is_empty() {
            let tools = ConvertTools { ffmpeg };
            let (inputs, what, output) = (now.clone(), job.what.clone(), job.output.clone());
            let retry: Rc<dyn Fn() -> Box<dyn gezik_ops::Task>> = Rc::new(move || {
                Box::new(ConvertTask::new(inputs.clone(), what.clone(), output.clone(), tools.clone()))
            });
            let id = self.0.ops.submit(retry(), Some(retry), After::Select);
            self.0.jobs.borrow_mut().insert(id, Job { inputs: now, ..job.clone() });
        }
        if let Some(need) = need {
            let again = self.again(Job { inputs: waiting, ..job });
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
    /// need ffmpeg get the box, which starts them again after a download.
    pub fn job_finished(&self, id: JobId, report: &Report) {
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
            let found = find_ffmpeg(&data, configured.as_deref()).map(|ff| ff.version);
            let hint = gezik_platform::http::tool_missing_hint();
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| {
                    let need = need_for(job.preset, &job.inputs, found);
                    let again = this.again(job);
                    crate::archives::with_current(|archives| archives.offer_ffmpeg(need, hint, again, Some(id)));
                });
            });
        });
    }

    /// Runs a user command on the `items` it takes, as one job undone as one action.
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
        let task: Box<dyn gezik_ops::Task> = Box::new(CommandTask::new(items, spec));
        self.0.ops.submit_chain(vec![task], Some(label), Some(again), After::Select);
    }
}

/// What Convert starts.
enum Ready {
    Command(CommandSpec, Vec<(PathBuf, bool)>),
    Convert(Job),
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
        }
    }

    #[test]
    fn the_most_common_kind_comes_first() {
        let items = [file("a.txt"), file("b.jpg"), file("c.mp4"), file("d.MP3"), folder("e"), file("f.exe")];
        assert_eq!(kinds_for(&items), [Kind::Media, Kind::Image, Kind::Text]);
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
        assert_eq!(top, [(CONVERT, "Convert…".to_owned())]);
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
    fn the_preset_list_follows_the_selection() {
        let kinds = kinds_for(&[file("a.txt"), file("b.txt"), file("c.png")]);
        let commands = [spec("Lint", &["lint", "{in}"], None, &["txt"], false)];
        let (choices, rows) = preset_list(&kinds, &commands, &[CommandState::Off("lint not found".to_owned())]);
        assert_eq!(rows[0], Row::Heading("Text"));
        assert_eq!(choices[0], Choice::Preset(preset("to-utf8").unwrap()));
        let images = PRESETS.iter().filter(|p| p.kind == Kind::Image).count();
        assert_eq!(choices.len(), 3 + images + 1);
        assert_eq!(*choices.last().unwrap(), Choice::Command(0));
        assert!(!rows.contains(&Row::Heading("Audio/Video")));

        let menu = preset_menu(&rows, 1);
        assert_eq!(menu[0], (HEADING, "Text".to_owned(), false));
        assert_eq!(menu[1], (CONVERT_PRESET_FIRST, "    Convert to UTF-8".to_owned(), true));
        assert_eq!(menu[2].1, "• Windows line endings (CRLF)");
        let last = menu.last().unwrap();
        assert_eq!(last.0, CONVERT_PRESET_FIRST + choices.len() as u32 - 1);
        assert!(!last.2 && last.1.contains("lint not found"));
        assert_eq!(menu.iter().filter(|(id, _, _)| *id == HEADING).count(), 3);

        // The last choice when it is there and can be chosen, else the first that can.
        let enabled = enabled_of(&rows);
        let png = choices.iter().position(|c| *c == Choice::Preset(preset("to-png").unwrap()));
        assert_eq!(first_choice(&choices, &enabled, &commands, Some("to-png")), png);
        assert_eq!(first_choice(&choices, &enabled, &commands, Some("command:Lint")), Some(0));
        assert_eq!(first_choice(&choices, &enabled, &commands, Some("mp3")), Some(0));
        assert_eq!(first_choice(&choices, &[false; 3], &commands, None), None);
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

    #[test]
    fn what_needs_ffmpeg_waits_for_it() {
        let (jpg, heic) = (PathBuf::from("d/a.jpg"), PathBuf::from("d/b.HEIC"));
        let to_jpeg = preset("to-jpeg").unwrap();
        let both = vec![jpg.clone(), heic.clone()];
        let none = split_by_ffmpeg(to_jpeg, both.clone(), None);
        assert_eq!(none, (vec![jpg.clone()], vec![heic.clone()], Some(Need::Heic)));
        // An ffmpeg older than 9 (or of unknown version) does not read HEIC.
        assert_eq!(split_by_ffmpeg(to_jpeg, both.clone(), Some(Some((6, 1)))).2, Some(Need::NewerFfmpeg));
        assert_eq!(split_by_ffmpeg(to_jpeg, both.clone(), Some(None)).2, Some(Need::NewerFfmpeg));
        assert_eq!(split_by_ffmpeg(to_jpeg, both.clone(), Some(Some((9, 0)))), (both, Vec::new(), None));
        // Writing AVIF takes any ffmpeg.
        let avif = preset("to-avif").unwrap();
        assert_eq!(split_by_ffmpeg(avif, vec![jpg.clone()], None).2, Some(Need::Pictures));
        assert!(split_by_ffmpeg(avif, vec![jpg], Some(Some((6, 0)))).1.is_empty());
        let mp3 = preset("mp3").unwrap();
        let video = vec![PathBuf::from("d/v.mkv")];
        assert_eq!(split_by_ffmpeg(mp3, video.clone(), None), (Vec::new(), video.clone(), Some(Need::Media)));
        assert_eq!(split_by_ffmpeg(mp3, video.clone(), Some(None)).0, video);
        assert_eq!(split_by_ffmpeg(preset("to-utf8").unwrap(), vec![PathBuf::from("a.txt")], None).2, None);
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
}
