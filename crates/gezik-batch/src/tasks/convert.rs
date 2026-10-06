//! Converting files as one engine job: each picture, text or media file is written under a
//! temporary name next to its output and renamed when complete. "Replace original" trashes
//! the original once the new file is in place; one undo trashes what was made and brings
//! the originals back.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::batch::convert::{
    ImageFormat, ImageOptions, MediaPreset, Output, TextOptions, image_inputs, media_inputs, output_path,
};
use gezik_core::ops::paths::{path_key, same_path};
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

use super::compress::keep_aside;
use super::{file_name, what};
use crate::convert::ffmpeg::{Ffmpeg, duration_us, run_preset};
use crate::convert::image::{ImageError, ImageJob, convert_image, remove_location};
use crate::convert::text::{convert_text, is_binary};

/// What a conversion does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertWhat {
    Image(ImageOptions),
    /// "Remove location data" (a preset that [keeps the format]): each picture is written in
    /// the format it has, a JPEG without being re-encoded. `format` is not used.
    ///
    /// [keeps the format]: gezik_core::batch::convert::Preset::keeps_format
    RemoveLocation(ImageOptions),
    Text(TextOptions),
    Media(MediaPreset),
}

impl ConvertWhat {
    /// Whether it takes the file `name` (a folder never).
    fn takes(&self, name: &str, is_dir: bool) -> bool {
        if is_dir {
            return false;
        }
        match self {
            ConvertWhat::Image(_) => image_inputs(name),
            ConvertWhat::RemoveLocation(_) => kept_format(name).is_some(),
            ConvertWhat::Text(_) => true,
            ConvertWhat::Media(_) => media_inputs(name),
        }
    }

    /// Why a file it does not take is left out.
    fn not_taken(&self, is_dir: bool) -> &'static str {
        match self {
            _ if is_dir => "a folder",
            ConvertWhat::Image(_) => "not a picture",
            ConvertWhat::RemoveLocation(_) => "not a picture Gezik can write",
            ConvertWhat::Text(_) => "not a text file",
            ConvertWhat::Media(_) => "not audio or video",
        }
    }

    /// The output's extension for `input`.
    fn extension(&self, input: &Path) -> String {
        match self {
            ConvertWhat::Image(options) => options.format.extension().to_owned(),
            // Its own extension, as it is spelled (`.jpeg` stays `.jpeg`).
            ConvertWhat::RemoveLocation(_) | ConvertWhat::Text(_) => {
                input.extension().map(|ext| ext.to_string_lossy().into_owned()).unwrap_or_default()
            }
            ConvertWhat::Media(preset) => preset.extension().to_owned(),
        }
    }

    /// What it converts to, for the panel: "WebP", "UTF-8", "MP3".
    fn to(&self) -> String {
        match self {
            ConvertWhat::Image(options) => match options.format {
                ImageFormat::Jpeg => "JPEG",
                ImageFormat::Png => "PNG",
                ImageFormat::WebpLossless | ImageFormat::WebpLossy => "WebP",
                ImageFormat::Avif => "AVIF",
                ImageFormat::Bmp => "BMP",
            }
            .to_owned(),
            ConvertWhat::RemoveLocation(_) => String::new(),
            // As the layer names it ("Windows-1252", not the label "windows-1252").
            ConvertWhat::Text(options) if !options.keeps_encoding() => {
                encoding_rs::Encoding::for_label(options.to.as_bytes())
                    .map_or_else(|| options.to.clone(), |encoding| crate::convert::text::name(encoding).to_owned())
            }
            ConvertWhat::Text(_) => "new line endings".to_owned(),
            ConvertWhat::Media(preset) => preset.extension().to_ascii_uppercase(),
        }
    }
}

/// The format "Remove location data" writes the picture `name` in: its own, WebP lossless.
fn kept_format(name: &str) -> Option<ImageFormat> {
    match ImageFormat::of_name(name)? {
        ImageFormat::WebpLossy => Some(ImageFormat::WebpLossless),
        format => Some(format),
    }
}

/// The programs a conversion may need.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConvertTools {
    pub ffmpeg: Option<Ffmpeg>,
}

/// The inputs `what` leaves out (folders, and files of other kinds), for "3 items skipped:
/// not images". Touches no disk.
pub fn skipped_inputs(inputs: &[(PathBuf, bool)], what: &ConvertWhat) -> Vec<PathBuf> {
    inputs
        .iter()
        .filter(|(path, is_dir)| !what.takes(&file_name(path), *is_dir))
        .map(|(path, _)| path.clone())
        .collect()
}

/// Whether a failure's text, as the engine's report keeps it, says ffmpeg is needed (the UI
/// then offers to download it).
pub fn is_ffmpeg_needed(err_text: &str) -> bool {
    err_text == ImageError::NeedsFfmpeg.to_string()
}

fn ffmpeg_needed() -> io::Error {
    io::Error::other(ImageError::NeedsFfmpeg)
}

/// Replacing an original (or changing it in place) is undone from the trash: without one it
/// is refused.
pub(super) fn needs_trash() -> io::Error {
    let what = if cfg!(windows) { "needs a recycle bin to undo" } else { "needs a trash to undo" };
    io::Error::new(io::ErrorKind::Unsupported, what)
}

/// The plan item's note: a file to convert, a folder to make first, a file left out.
const CONVERT: u8 = 0;
const FOLDER: u8 = 1;
const LEFT_OUT: u8 = 2;

pub struct ConvertTask {
    inputs: Vec<PathBuf>,
    what: ConvertWhat,
    output: Output,
    tools: ConvertTools,
}

impl ConvertTask {
    pub fn new(inputs: Vec<PathBuf>, what: ConvertWhat, output: Output, tools: ConvertTools) -> ConvertTask {
        ConvertTask { inputs, what, output, tools }
    }

    fn replacing(&self) -> bool {
        self.output == Output::ReplaceOriginal
    }

    /// Converts `input` into `temp`.
    fn convert(&self, input: &Path, temp: &Path, run: &RunCx<'_>) -> io::Result<()> {
        let stop = || run.stopped();
        let ffmpeg = self.tools.ffmpeg.as_ref();
        match &self.what {
            ConvertWhat::Image(options) => {
                let job = ImageJob { input, output: temp, options, ffmpeg: ffmpeg.map(|f| f.ffmpeg.as_path()) };
                convert_image(&job, &stop)
            }
            ConvertWhat::RemoveLocation(options) => {
                let format = kept_format(&file_name(input))
                    .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "not a picture Gezik can write"))?;
                let options = ImageOptions { format, ..*options };
                let job =
                    ImageJob { input, output: temp, options: &options, ffmpeg: ffmpeg.map(|f| f.ffmpeg.as_path()) };
                remove_location(&job, &stop)
            }
            ConvertWhat::Text(options) => convert_text(input, temp, options, &stop),
            ConvertWhat::Media(preset) => {
                let ff = ffmpeg.ok_or_else(ffmpeg_needed)?;
                let size = std::fs::metadata(input).map_or(0, |meta| meta.len());
                // Against what the item counted in earlier tries (a full disk, a pause): a try
                // that starts over counts only what goes past them.
                let mut on_progress = |fraction: f64| {
                    let now = ((size as f64 * fraction) as u64).min(size);
                    let counted = run.counted();
                    if now > counted {
                        run.add_bytes(now - counted);
                    }
                };
                let args = gezik_core::batch::convert::ffmpeg_args(*preset, input, temp);
                // A pause ends ffmpeg (it would go on using every core) without waiting: the
                // file is done again from the start once the job is resumed.
                let interrupted = || run.cancelled() || run.paused();
                let result = run_preset(ff, args, duration_us(ff, input), &mut on_progress, &interrupted);
                if result.is_err() {
                    let _ = std::fs::remove_file(temp);
                }
                match result {
                    Err(err) if err.kind() == io::ErrorKind::Interrupted && !run.cancelled() => {
                        Err(gezik_ops::restart())
                    }
                    result => result,
                }
            }
        }
    }

    /// Puts the finished `temp` at `target`; replacing, the original goes to the trash once
    /// the new file is in place (set aside under a noted name meanwhile, so it is never lost).
    fn place(&self, input: &Path, temp: &Path, target: &Path, run: &RunCx<'_>) -> io::Result<Outcome> {
        let created =
            |path: &Path| Outcome::Created { path: path.to_path_buf(), facts: facts_after(path, false), from: None };
        if !self.replacing() {
            if let Err(err) = gezik_platform::fs::move_entry(temp, target) {
                let _ = std::fs::remove_file(temp);
                return Err(err);
            }
            return Ok(created(target));
        }
        let aside = run.aside_name(input);
        run.note_aside(&[(aside.clone(), input.to_path_buf())]);
        if let Err(err) = gezik_platform::fs::move_entry(input, &aside) {
            run.forget_aside(&[&aside]);
            let _ = std::fs::remove_file(temp);
            return Err(err);
        }
        if let Err(err) = gezik_platform::fs::move_entry(temp, target) {
            let _ = std::fs::remove_file(temp);
            match gezik_platform::fs::move_entry(&aside, input) {
                Ok(()) => run.forget_aside(&[&aside]),
                Err(back) => {
                    return Err(io::Error::new(
                        err.kind(),
                        format!("{err}; the original is at {} ({back})", aside.display()),
                    ));
                }
            }
            return Err(err);
        }
        match gezik_ops::trash_path(&aside) {
            Ok(trashed) => {
                run.forget_aside(&[&aside]);
                Ok(Outcome::Several(vec![Outcome::Trashed { original: input.to_path_buf(), trashed }, created(target)]))
            }
            Err(err) => {
                // The new file stays; the original is kept next to it under a free name.
                keep_aside(input, &aside, &err, run);
                Ok(created(target))
            }
        }
    }
}

impl Task for ConvertTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Convert
    }

    fn title(&self) -> String {
        let to = self.what.to();
        match &self.what {
            ConvertWhat::RemoveLocation(_) => format!("Removing location data from {}", what(&self.inputs)),
            _ => format!("Converting {} to {to}", what(&self.inputs)),
        }
    }

    fn count(&self) -> usize {
        self.inputs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.inputs.clone();
        if let Output::Folder(folder) = &self.output {
            paths.push(folder.clone());
        }
        let work = match self.what {
            ConvertWhat::Image(_) | ConvertWhat::RemoveLocation(_) => Work::Cpu,
            ConvertWhat::Text(_) => Work::Disk,
            ConvertWhat::Media(_) => Work::External,
        };
        Resources { paths, work }
    }

    fn workers(&self) -> Option<usize> {
        // ffmpeg uses every core itself.
        matches!(self.what, ConvertWhat::Media(_)).then_some(1)
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let inputs: HashSet<Vec<String>> = self.inputs.iter().map(|input| path_key(input)).collect();
        // Outputs planned so far: two inputs must not write the same file.
        let mut outputs: HashSet<Vec<String>> = HashSet::new();
        let mut folders: HashSet<Vec<String>> = HashSet::new();
        for (root, input) in self.inputs.iter().enumerate() {
            let meta = match std::fs::metadata(input) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(input, err);
                    continue;
                }
            };
            let facts = Facts { is_dir: meta.is_dir(), size: meta.len(), modified: meta.modified().ok() };
            let item = PlanItem::new(Stage::Parallel, facts).source(input).top(root);
            if !self.what.takes(&file_name(input), meta.is_dir()) {
                if !sink.item(item.tag(LEFT_OUT)) {
                    return;
                }
                continue;
            }
            let target = output_path(input, &self.what.extension(input), &self.output);
            let key = path_key(&target);
            let in_place = same_path(&target, input);
            if !in_place && inputs.contains(&key) {
                let message = format!("it would be converted to {}, which is also selected", file_name(&target));
                sink.failed(input, io::Error::new(io::ErrorKind::AlreadyExists, message));
                continue;
            }
            if !outputs.insert(key) {
                let message = format!("another selected file is also converted to {}", file_name(&target));
                sink.failed(input, io::Error::new(io::ErrorKind::AlreadyExists, message));
                continue;
            }
            // The output's folder ("converted"), made first if it is not there: undo removes it.
            if let Some(folder) = target.parent()
                && !folder.as_os_str().is_empty()
                && folders.insert(path_key(folder))
                && std::fs::symlink_metadata(folder).is_err()
            {
                let make = PlanItem::new(Stage::Before, Facts { is_dir: true, ..Facts::default() })
                    .target(folder)
                    .tag(FOLDER)
                    .uncounted();
                if !sink.item(make) {
                    return;
                }
            }
            let item = item.target(target).tag(CONVERT);
            // Replacing under the same name: the original is the target, no conflict.
            let item = if in_place { item } else { item.checked() };
            if !sink.item(item) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        match item.tag {
            FOLDER => {
                let Some(folder) = &item.target else { return Ok(Outcome::Nothing) };
                return match std::fs::create_dir(folder) {
                    Ok(()) => {
                        Ok(Outcome::Created { path: folder.clone(), facts: facts_after(folder, true), from: None })
                    }
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists && folder.is_dir() => Ok(Outcome::Nothing),
                    Err(err) => Err(err),
                };
            }
            LEFT_OUT => {
                let why = io::Error::new(io::ErrorKind::Unsupported, self.what.not_taken(item.facts.is_dir));
                run.skip(item.path(), &why);
                return Ok(Outcome::Nothing);
            }
            _ => {}
        }
        let (Some(input), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        if self.replacing() && !run.has_trash(input) {
            return Err(needs_trash());
        }
        let temp = run.temp_file_for(target);
        match self.convert(input, &temp, run) {
            Ok(()) => {}
            // A file that looks binary is left out, not failed.
            Err(err) if is_binary(&err) => {
                run.skip(input, &err);
                return Ok(Outcome::Nothing);
            }
            Err(err) => return Err(err),
        }
        self.place(input, &temp, target, run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_inputs_by_kind() {
        let inputs: Vec<(PathBuf, bool)> = vec![
            ("d/a.jpg".into(), false),
            ("d/b.PNG".into(), false),
            ("d/c.heic".into(), false),
            ("d/notes.txt".into(), false),
            ("d/film.mkv".into(), false),
            ("d/sub".into(), true),
        ];
        let paths = |list: &[&str]| list.iter().map(PathBuf::from).collect::<Vec<_>>();
        let image = ConvertWhat::Image(ImageOptions::DEFAULT);
        assert_eq!(skipped_inputs(&inputs, &image), paths(&["d/notes.txt", "d/film.mkv", "d/sub"]));
        // HEIC is read through ffmpeg but cannot be written back.
        let location = ConvertWhat::RemoveLocation(ImageOptions::DEFAULT);
        assert_eq!(skipped_inputs(&inputs, &location), paths(&["d/c.heic", "d/notes.txt", "d/film.mkv", "d/sub"]));
        let text = ConvertWhat::Text(
            gezik_core::batch::convert::TextPreset { to: Some("UTF-8"), eol: gezik_core::batch::convert::Eol::Keep }
                .options(),
        );
        assert_eq!(skipped_inputs(&inputs, &text), paths(&["d/sub"]));
        let media = ConvertWhat::Media(MediaPreset::Mp3);
        assert_eq!(skipped_inputs(&inputs, &media), paths(&["d/a.jpg", "d/b.PNG", "d/c.heic", "d/notes.txt", "d/sub"]));
    }

    #[test]
    fn ffmpeg_needed_is_recognized_in_the_report() {
        assert!(is_ffmpeg_needed(&gezik_platform::fs::describe(&ffmpeg_needed())));
        assert!(!is_ffmpeg_needed("ffmpeg failed (exit code 1)"));
    }

    #[test]
    fn kept_formats_and_titles() {
        assert_eq!(kept_format("a.webp"), Some(ImageFormat::WebpLossless));
        assert_eq!(kept_format("a.JPEG"), Some(ImageFormat::Jpeg));
        assert_eq!(kept_format("a.gif"), None);
        let what = ConvertWhat::RemoveLocation(ImageOptions::DEFAULT);
        assert_eq!(what.extension(Path::new("d/a.JPEG")), "JPEG");
        let task = ConvertTask::new(
            vec!["d/a.png".into(), "d/b.png".into()],
            ConvertWhat::Image(ImageOptions { format: ImageFormat::WebpLossless, ..ImageOptions::DEFAULT }),
            Output::SameFolder,
            ConvertTools::default(),
        );
        assert_eq!(task.title(), "Converting 2 items to WebP");
        assert_eq!(task.workers(), None);
        let task = ConvertTask::new(
            vec!["d/a.mkv".into()],
            ConvertWhat::Media(MediaPreset::Mp4),
            Output::SameFolder,
            ConvertTools::default(),
        );
        assert_eq!(task.title(), "Converting a.mkv to MP4");
        assert_eq!(task.workers(), Some(1));
        assert_eq!(task.resources().work, Work::External);
        // Encodings by the name the layer shows.
        let text = |to: &str| {
            ConvertWhat::Text(TextOptions {
                from: None,
                to: to.to_owned(),
                bom: false,
                eol: gezik_core::batch::convert::Eol::Keep,
                trim_trailing: false,
                final_newline: false,
            })
        };
        assert_eq!(text("windows-1252").to(), "Windows-1252");
        assert_eq!(text("UTF-16LE").to(), "UTF-16 LE");
        assert_eq!(text("").to(), "new line endings");
    }
}
