//! Archives in the app: the Extract and Compress menu items, the Compress layer, the
//! passwords and questions extraction asks, and the box that offers to download a tool:
//! 7-Zip when an archive needs it, ffmpeg when a conversion does (convert.rs asks). The work
//! runs as engine jobs (gezik-batch's tasks); the UI thread never reads the disk: menus
//! decide by name, and finding 7-Zip or adding up sizes runs on a thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_batch::tasks::{
    AddToArchiveTask, CompressOptions, CompressTask, DownloadTask, ExtractTo, Level, OutFormat, default_name,
    extract_chain, extract_label, says_seven_zip_needed,
};
use gezik_batch::tools::says_damaged;
use gezik_config::settings::{ArchiveState, ArchivesSettings, DoubleClick, ToolsSettings};
use gezik_config::shortcuts::{Chord, Key, Platform as KeyPlatform};
use gezik_config::store::ConfigStore;
use gezik_core::batch::archive::{Format, archive_stem, detect, looks_like_archive, split_sizes};
use gezik_core::batch::tools::{Platform, Tool, ToolBuild, build_for};
use gezik_core::format_size;
use gezik_core::ops::names::{NameRules, validate_name};
use gezik_ops::{JobId, Report};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::AppWindow;
use crate::context_menu::{COMPRESS, COMPRESS_TO, EXTRACT_HERE, EXTRACT_TO, EXTRACT_TO_OWN};
use crate::dialog::Dialogs;
use crate::operations::{After, Operations};

/// Where Gezik's downloads come from, when a build's own release page is not known.
const TOOLS_PAGE: &str = "https://github.com/wenlar/gezik-tools";

/// "Where does it come from?": the release page of the build's download (its notes link the
/// licence and the sources), from `…/releases/download/<tag>/<file>`; the tools page if the
/// address is not one of those.
pub fn release_page(url: &str) -> String {
    let Some((base, rest)) = url.split_once("/releases/download/") else { return TOOLS_PAGE.to_owned() };
    match rest.split_once('/') {
        Some((tag, _)) if !tag.is_empty() => format!("{base}/releases/tag/{tag}"),
        _ => TOOLS_PAGE.to_owned(),
    }
}

/// The Extract and Compress items for the selected `items` (path, is a folder): Extract
/// when every one is an archive by its name, Compress always. `format` and `level` are the
/// ones used last.
pub fn menu_items(items: &[(PathBuf, bool)], format: OutFormat, level: Level) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if items.is_empty() {
        return out;
    }
    if items.iter().all(|(path, is_dir)| !is_dir && looks_like_archive(&name_of(path))) {
        out.push((EXTRACT_HERE, "Extract here".to_owned()));
        match items {
            [(one, _)] => out.push((EXTRACT_TO_OWN, format!("Extract to \"{}{MAIN_SEPARATOR}\"", stem_of(one)))),
            _ => out.push((EXTRACT_TO_OWN, "Extract each to its own folder".to_owned())),
        }
        out.push((EXTRACT_TO, "Extract to…".to_owned()));
    }
    out.push((COMPRESS, "Compress…".to_owned()));
    let format = written_format(compress_format(items, format), level);
    out.push((COMPRESS_TO, format!("Compress to \"{}\"", default_name(items, format))));
    out
}

/// What `format` at `level` writes: Store makes a plain `.tar` of a `.tar.gz` or `.tar.xz`
/// (as `CompressTask` does), so the name shown is the one made.
pub fn written_format(format: OutFormat, level: Level) -> OutFormat {
    match format {
        OutFormat::TarGz | OutFormat::TarXz if level == Level::Store => OutFormat::Tar,
        other => other,
    }
}

/// The layer's note for `format` at `level` (empty: none).
pub fn store_note(format: OutFormat, level: Level) -> &'static str {
    if written_format(format, level) != format { "Store makes a plain .tar" } else { "" }
}

/// The name typed in the layer after the format or level changed from `from` to `to`
/// (each a format and a level): its ending follows what is written.
pub fn renamed_for(name: &str, from: (OutFormat, Level), to: (OutFormat, Level)) -> String {
    let (old, new) = (written_format(from.0, from.1), written_format(to.0, to.1));
    if old == new { name.to_owned() } else { swap_extension(name, old, new) }
}

/// `archives` split into those Gezik opens itself (or all, with 7-Zip there) and those that
/// wait for 7-Zip, known by name.
pub fn split_by_seven_zip(archives: Vec<PathBuf>, have_seven_zip: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
    if have_seven_zip {
        return (archives, Vec::new());
    }
    archives.into_iter().partition(|archive| !needs_seven_zip(&name_of(archive)))
}

/// The formats offered for `items`: `.gz` and `.xz` only for one file.
pub fn formats_for(items: &[(PathBuf, bool)]) -> Vec<OutFormat> {
    let mut out = vec![OutFormat::Zip, OutFormat::SevenZ, OutFormat::TarGz, OutFormat::TarXz, OutFormat::Tar];
    if let [(_, false)] = items {
        out.extend([OutFormat::Gz, OutFormat::Xz]);
    }
    out
}

/// `format` if it can be used for `items`, else zip.
pub fn compress_format(items: &[(PathBuf, bool)], format: OutFormat) -> OutFormat {
    if formats_for(items).contains(&format) { format } else { OutFormat::Zip }
}

/// How a format's button reads.
pub fn format_label(format: OutFormat) -> &'static str {
    match format {
        OutFormat::Gz => ".gz",
        OutFormat::Xz => ".xz",
        other => other.extension(),
    }
}

/// The format saved as `key` (its ending) in state.toml.
pub fn format_from_key(key: &str) -> Option<OutFormat> {
    [
        OutFormat::Zip,
        OutFormat::SevenZ,
        OutFormat::Tar,
        OutFormat::TarGz,
        OutFormat::TarXz,
        OutFormat::Gz,
        OutFormat::Xz,
    ]
    .into_iter()
    .find(|format| format.extension() == key)
}

const LEVELS: [(Level, &str); 4] =
    [(Level::Store, "store"), (Level::Fast, "fast"), (Level::Normal, "normal"), (Level::Best, "best")];

pub fn level_from_key(key: &str) -> Option<Level> {
    LEVELS.iter().find(|(_, k)| *k == key).map(|(level, _)| *level)
}

pub fn level_key(level: Level) -> &'static str {
    LEVELS.iter().find(|(l, _)| *l == level).map_or("normal", |(_, key)| key)
}

/// `name` with `from`'s ending changed to `to`'s (`Fotolar.zip` → `Fotolar.7z`); a name that
/// does not end in it gets `to`'s added.
pub fn swap_extension(name: &str, from: OutFormat, to: OutFormat) -> String {
    let old = format!(".{}", from.extension());
    let stem = match name.len().checked_sub(old.len()) {
        Some(at) if name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(&old) => &name[..at],
        _ => name,
    };
    format!("{stem}.{}", to.extension())
}

/// The folder typed in a field: relative to `base`; `None` if empty.
pub fn resolve_folder(typed: &str, base: &Path) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let path = PathBuf::from(typed);
    Some(if path.is_absolute() { path } else { base.join(path) })
}

/// The password question's title and message.
pub fn password_text(archive: &Path, retry: bool) -> (String, String) {
    let message = if retry {
        "Wrong password. Try again:".to_owned()
    } else {
        format!("{} is encrypted. Password:", name_of(archive))
    };
    ("Password".to_owned(), message)
}

/// What a tool is needed for, which its box says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    /// 7-Zip, to open archives with this ending (`.lzh`).
    Archive(String),
    /// ffmpeg, for audio or video.
    Media,
    /// ffmpeg, to write pictures (lossy WebP, AVIF): any version.
    Pictures,
    /// ffmpeg 9 or newer, to read HEIC, HEIF or AVIF pictures; there is none.
    Heic,
    /// The same, and the ffmpeg there is older (or of unknown version).
    NewerFfmpeg,
    /// The same, and the older one is set in settings.toml (`[convert] ffmpeg`): a download
    /// would not be used.
    ConfiguredTooOld,
}

impl Need {
    fn tool(&self) -> Tool {
        if matches!(self, Need::Archive(_)) { Tool::SevenZip } else { Tool::Ffmpeg }
    }
}

/// The box offering a tool for `need`: its title, message and buttons. `size`: the download
/// for this system (none: Gezik has no build for it yet); `download`: `[tools] download`;
/// `hint`: what to install first for downloading to work at all (Linux without curl or wget).
pub fn tool_offer(
    need: &Need,
    size: Option<u64>,
    download: bool,
    linux: bool,
    hint: Option<&str>,
) -> (&'static str, String, Vec<&'static str>) {
    let lead = match need {
        Need::Archive(ext) => {
            let (message, buttons) = seven_zip_offer(ext, size, download, linux);
            return ("7-Zip needed", message, buttons);
        }
        Need::ConfiguredTooOld => {
            let message = "Reading HEIC and AVIF pictures needs ffmpeg 9 or newer. The ffmpeg set under [convert]                            in settings.toml is older: set it to ffmpeg 9 or newer, or remove it to use Gezik's                            download.";
            return ("ffmpeg needed", message.to_owned(), vec!["OK"]);
        }
        Need::Media => "Video conversion needs ffmpeg",
        Need::Pictures => "Converting to this format needs ffmpeg",
        Need::Heic => "Reading HEIC and AVIF pictures needs ffmpeg 9 or newer",
        Need::NewerFfmpeg => "Reading HEIC and AVIF pictures needs ffmpeg 9 or newer (the one found is older)",
    };
    // Linux packages are often older than 9, which reads HEIC wrongly: no package hint then.
    let nine = matches!(need, Need::Heic | Need::NewerFfmpeg);
    let yourself =
        if nine { "install ffmpeg 9 or newer yourself" } else { "install it yourself (sudo apt install ffmpeg)" };
    let message = match size {
        Some(size) if download => {
            let mut message = format!("{lead} (~{}, free).", format_size(size));
            match hint {
                Some(hint) => {
                    message.push_str(&format!(" {hint} to download it, or {yourself}."));
                    return ("ffmpeg needed", message, vec!["OK"]);
                }
                None if linux => message.push_str(&format!(" Download it, or {yourself}.")),
                None => {}
            }
            return ("ffmpeg needed", message, vec!["Download", "Where does it come from?", "Cancel"]);
        }
        _ if download => {
            let how = if linux { yourself } else { "install it yourself" };
            format!("{lead}. Gezik cannot download it for this system yet: {how}.")
        }
        _ => format!("{lead}: install it yourself, or set ffmpeg under [convert] in settings.toml."),
    };
    ("ffmpeg needed", message, vec!["OK"])
}

/// The "needs 7-Zip" box: its message and buttons. `size`: the download for this system
/// (none: Gezik has no build for it yet); `download`: `[tools] download`.
pub fn seven_zip_offer(ext: &str, size: Option<u64>, download: bool, linux: bool) -> (String, Vec<&'static str>) {
    match size {
        Some(size) if download => {
            let mut message = format!("Opening {ext} archives needs 7-Zip (~{}, free).", format_size(size));
            if linux {
                message.push_str(" Download it, or install 7-Zip 25 or newer yourself.");
            }
            (message, vec!["Download", "Where does it come from?", "Cancel"])
        }
        _ => {
            // An older one is not used (Gezik passes over it).
            let how = "install 7-Zip 25 or newer yourself";
            let message = if download {
                format!("Opening {ext} archives needs 7-Zip. Gezik cannot download it for this system yet: {how}.")
            } else {
                format!("Opening {ext} archives needs 7-Zip: {how}, or set seven-zip under [tools] in settings.toml.")
            };
            (message, vec!["OK"])
        }
    }
}

/// The split choice (0 none, 1-3 `split_sizes`, 4 custom) and the custom size in MB for a
/// saved part size.
pub fn split_choice(split: Option<u64>) -> (usize, String) {
    match split {
        None => (0, String::new()),
        Some(bytes) => match split_sizes().iter().position(|(_, size)| *size == bytes) {
            Some(i) => (i + 1, String::new()),
            None => (4, (bytes / 1_000_000).max(1).to_string()),
        },
    }
}

/// The part size for split choice `choice`, `mb` typed for a custom one.
pub fn split_bytes(choice: usize, mb: &str) -> Result<Option<u64>, String> {
    match choice {
        0 => Ok(None),
        1..=3 => Ok(Some(split_sizes()[choice - 1].1)),
        _ => match mb.trim().parse::<u64>() {
            Ok(mb) if mb > 0 => Ok(Some(mb.saturating_mul(1_000_000))),
            _ => Err("Type the size of a part in MB".to_owned()),
        },
    }
}

/// `.lzh`: how the box names an archive's kind.
fn extension_of(path: &Path) -> String {
    let name = name_of(path).to_ascii_lowercase();
    name.rsplit_once('.').map_or_else(|| "these".to_owned(), |(_, ext)| format!(".{ext}"))
}

/// Whether Gezik knows by `name` alone that only 7-Zip opens it.
fn needs_seven_zip(name: &str) -> bool {
    let rar = name.to_ascii_lowercase().ends_with(".rar");
    matches!(detect(&[], name), Some(Format::Other(_))) || (rar && !cfg!(feature = "rar"))
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// The folder an archive goes into for "Extract to "<stem>\"".
fn stem_of(archive: &Path) -> String {
    let name = name_of(archive);
    let stem = archive_stem(&name);
    if stem.is_empty() { "Archive".to_owned() } else { stem.to_owned() }
}

/// "Size: 340 MB in 12 files".
fn size_text(files: u64, bytes: u64) -> String {
    let files = if files == 1 { "1 file".to_owned() } else { format!("{files} files") };
    format!("Size: {} in {files}", format_size(bytes))
}

/// The files under `paths` and their size; stops early once `stop` says so.
fn total_size(paths: &[PathBuf], stop: impl Fn() -> bool) -> (u64, u64) {
    let (mut files, mut bytes) = (0u64, 0u64);
    let mut todo: Vec<PathBuf> = paths.to_vec();
    while let Some(path) = todo.pop() {
        if stop() {
            break;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.is_dir() {
            if let Ok(listing) = std::fs::read_dir(&path) {
                todo.extend(listing.flatten().map(|entry| entry.path()));
            }
        } else {
            files += 1;
            bytes += meta.len();
        }
    }
    (files, bytes)
}

/// What a job of ours waits to do when it ends.
enum Pending {
    Extract { archives: Vec<PathBuf>, to: ExtractTo },
    Download { build: &'static ToolBuild, then: Vec<Then> },
}

/// What runs once a downloaded tool is there.
#[derive(Clone)]
enum Then {
    /// Extracting, and the failed row it replaces.
    Extract {
        archives: Vec<PathBuf>,
        to: ExtractTo,
        row: Option<JobId>,
        /// "Extract to…": the folder is made first.
        make_dir: bool,
    },
    /// Anything else started again (a conversion), and the failed row it replaces.
    Again { again: Rc<dyn Fn()>, row: Option<JobId> },
}

/// The Compress layer's choices (its texts live in the window).
struct Layer {
    sources: Vec<PathBuf>,
    /// The folder shown when it opened: a typed folder is relative to it.
    folder: PathBuf,
    formats: Vec<OutFormat>,
    format: usize,
    level: Level,
    encrypt_names: bool,
    split: usize,
}

impl Layer {
    fn format(&self) -> OutFormat {
        self.formats[self.format]
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    ops: Operations,
    dialogs: Dialogs,
    store: Option<ConfigStore>,
    state: RefCell<ArchiveState>,
    jobs: RefCell<HashMap<JobId, Pending>>,
    /// The tools whose box is on screen (several things that need one ask once).
    offering: RefCell<Vec<Tool>>,
    /// What runs once each tool is there, gathered while its box is on screen.
    waiting: RefCell<Vec<(Tool, Then)>>,
    layer: RefCell<Option<Layer>>,
    /// Counts openings of the layer: a size added up for an earlier one is dropped.
    sizing: Arc<AtomicU64>,
}

#[derive(Clone)]
pub struct Archives(Rc<Inner>);

thread_local! {
    static CURRENT: RefCell<Option<Archives>> = const { RefCell::new(None) };
    static SETTINGS: RefCell<(ToolsSettings, ArchivesSettings)> = RefCell::default();
}

/// settings.toml changed: `[tools]` and `[archives]`.
pub fn set_settings(tools: ToolsSettings, archives: ArchivesSettings) {
    SETTINGS.with(|s| *s.borrow_mut() = (tools, archives));
}

pub fn tools_settings() -> ToolsSettings {
    SETTINGS.with(|s| s.borrow().0.clone())
}

/// Whether double-clicking an archive extracts it (`[archives] double-click`).
pub fn extracts_on_double_click() -> bool {
    SETTINGS.with(|s| s.borrow().1.double_click == DoubleClick::ExtractHere)
}

/// Runs `f` with this UI thread's archives, if set up.
pub fn with_current(f: impl FnOnce(&Archives)) {
    if let Some(archives) = CURRENT.with(|c| c.borrow().clone()) {
        f(&archives);
    }
}

impl Archives {
    pub fn new(
        window: &AppWindow,
        ops: Operations,
        dialogs: Dialogs,
        store: Option<ConfigStore>,
        state: ArchiveState,
    ) -> Archives {
        let this = Archives(Rc::new(Inner {
            window: window.as_weak(),
            ops,
            dialogs,
            store,
            state: RefCell::new(state),
            jobs: RefCell::default(),
            offering: RefCell::default(),
            waiting: RefCell::default(),
            layer: RefCell::default(),
            sizing: Arc::default(),
        }));
        this.install(window);
        CURRENT.with(|c| *c.borrow_mut() = Some(this.clone()));
        this
    }

    fn install(&self, window: &AppWindow) {
        let t = self.clone();
        window.on_cp_set_format(move |i| {
            if let Ok(i) = usize::try_from(i) {
                t.choose(Some(i), None);
            }
        });
        let t = self.clone();
        window.on_cp_set_level(move |i| {
            if let Some(level) = usize::try_from(i).ok().and_then(|i| LEVELS.get(i)) {
                t.choose(None, Some(level.0));
            }
        });
        let t = self.clone();
        window.on_cp_toggle_encrypt_names(move || t.edit(|layer| layer.encrypt_names = !layer.encrypt_names));
        let t = self.clone();
        window.on_cp_set_split(move |i| {
            if let Ok(i) = usize::try_from(i) {
                t.edit(|layer| layer.split = i.min(4));
            }
        });
        let t = self.clone();
        window.on_cp_edited(move || t.set_error(""));
        let t = self.clone();
        window.on_cp_compress(move || t.compress());
        let t = self.clone();
        window.on_cp_cancel(move || t.close());
        let t = self.clone();
        window.on_cp_add_existing(move || t.ask_existing());
    }

    /// Where downloaded tools go: `<config dir>/tools/` (next to settings.toml and the
    /// pending deletes); without a config folder, a folder in the temporary one.
    fn data_dir(&self) -> PathBuf {
        self.0.store.as_ref().map_or_else(|| std::env::temp_dir().join("gezik"), |store| store.dir().to_path_buf())
    }

    /// The format used last (zip at first).
    pub fn last_format(&self) -> OutFormat {
        self.0.state.borrow().format.as_deref().and_then(format_from_key).unwrap_or(OutFormat::Zip)
    }

    pub fn last_level(&self) -> Level {
        self.0.state.borrow().level.as_deref().and_then(level_from_key).unwrap_or(Level::Normal)
    }

    /// Writes the last choices to state.toml (never a password).
    fn save_state(&self, f: impl FnOnce(&mut ArchiveState)) {
        f(&mut self.0.state.borrow_mut());
        if let Some(store) = &self.0.store {
            let mut saved = store.load_state();
            saved.archive = self.0.state.borrow().clone();
            if let Err(err) = store.save_state(&saved) {
                eprintln!("gezik: cannot save the archive choices: {err}");
            }
        }
    }

    // Extracting.

    /// "Extract here": into the archives' folder, one item as it is, several in a folder.
    pub fn extract_here(&self, archives: Vec<PathBuf>) {
        let Some(dir) = archives.first().and_then(|a| a.parent()).map(Path::to_path_buf) else { return };
        self.extract(archives, ExtractTo::Smart(dir), false);
    }

    /// "Extract to "<stem>\"": each into a folder named after it.
    pub fn extract_to_own(&self, archives: Vec<PathBuf>) {
        let Some(dir) = archives.first().and_then(|a| a.parent()).map(Path::to_path_buf) else { return };
        self.extract(archives, ExtractTo::Folder(dir), false);
    }

    /// "Extract to…": asks for the folder (made if it is not there).
    pub fn extract_to_asked(&self, archives: Vec<PathBuf>) {
        let Some(base) = archives.first().and_then(|a| a.parent()).map(Path::to_path_buf) else { return };
        let initial = self.0.state.borrow().last_extract_to.clone().unwrap_or_else(|| base.display().to_string());
        let this = self.clone();
        self.0.dialogs.ask_text("Extract to", "Folder:", initial, &["Extract", "Cancel"], move |text| {
            let Some(dir) = text.and_then(|text| resolve_folder(&text, &base)) else { return };
            this.save_state(|state| state.last_extract_to = Some(dir.display().to_string()));
            this.extract(archives, ExtractTo::Smart(dir), true);
        });
    }

    /// Finds 7-Zip on a thread (it looks at the disk), then starts the job for what can be
    /// opened; what needs 7-Zip and lacks it waits for the box. `make_dir`: the folder is made
    /// first (only when something is extracted now).
    fn extract(&self, archives: Vec<PathBuf>, to: ExtractTo, make_dir: bool) {
        let configured = tools_settings().seven_zip.map(PathBuf::from);
        let data = self.data_dir();
        std::thread::spawn(move || {
            let seven_zip = gezik_batch::tools::find(Tool::SevenZip, &data, configured.as_deref());
            let (now, waiting) = split_by_seven_zip(archives, seven_zip.is_some());
            if make_dir && !now.is_empty() {
                // A failure shows when the job cannot use the folder.
                let _ = std::fs::create_dir_all(to.dir());
            }
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| this.extract_found(now, waiting, to, seven_zip, make_dir));
            });
        });
    }

    fn extract_found(
        &self,
        now: Vec<PathBuf>,
        waiting: Vec<PathBuf>,
        to: ExtractTo,
        seven_zip: Option<PathBuf>,
        make_dir: bool,
    ) {
        if !now.is_empty() {
            let tasks = extract_chain(now.clone(), to.clone(), seven_zip);
            let again: Rc<dyn Fn()> = {
                let (archives, to) = (now.clone(), to.clone());
                Rc::new(move || {
                    let (archives, to) = (archives.clone(), to.clone());
                    with_current(|this| this.extract(archives, to, false));
                })
            };
            let label = extract_label(&now);
            let id = self.0.ops.submit_chain(tasks, Some(label), Some(again), After::Select);
            self.0.jobs.borrow_mut().insert(id, Pending::Extract { archives: now, to: to.clone() });
        }
        if let Some(first) = waiting.first() {
            // Known by its name: offered before a job tries it.
            let ext = extension_of(first);
            self.offer(Need::Archive(ext), None, Then::Extract { archives: waiting, to, row: None, make_dir });
        }
    }

    /// A job ended (operations.rs tells every one): 7-Zip was needed, or a download is done.
    pub fn job_finished(&self, id: JobId, report: &Report) {
        let Some(pending) = self.0.jobs.borrow_mut().remove(&id) else { return };
        if report.cancelled {
            return;
        }
        match pending {
            Pending::Extract { archives, to } => {
                let needing: Vec<PathBuf> = archives
                    .into_iter()
                    .filter(|a| report.failures.iter().any(|f| f.path == *a && says_seven_zip_needed(&f.message)))
                    .collect();
                if let Some(first) = needing.first() {
                    let ext = extension_of(first);
                    let then = Then::Extract { archives: needing, to, row: Some(id), make_dir: false };
                    self.offer(Need::Archive(ext), None, then);
                }
            }
            Pending::Download { build, then } => {
                if report.failures.is_empty() {
                    for then in then {
                        match then {
                            Then::Extract { archives, to, row, make_dir } => {
                                if let Some(row) = row {
                                    self.0.ops.forget(row);
                                }
                                self.extract(archives, to, make_dir);
                            }
                            Then::Again { again, row } => {
                                if let Some(row) = row {
                                    self.0.ops.forget(row);
                                }
                                again();
                            }
                        }
                    }
                } else if report.failures.iter().any(|f| says_damaged(&f.message)) {
                    let this = self.clone();
                    self.0.dialogs.ask(
                        "The download was damaged",
                        "What arrived is not the file Gezik expected. Try again?",
                        &["Try again", "Cancel"],
                        move |choice| {
                            if choice == Some(0) {
                                this.0.ops.forget(id);
                                this.download(build, then);
                            }
                        },
                    );
                }
            }
        }
    }

    /// The box for ffmpeg (convert.rs): Download fetches it and then calls `again`, after
    /// taking away the failed row `row` if there is one. `hint`: what to install first for
    /// downloading to work (found on a thread). While the box is on screen, more that need
    /// ffmpeg join it.
    pub fn offer_ffmpeg(&self, need: Need, hint: Option<String>, again: Rc<dyn Fn()>, row: Option<JobId>) {
        self.offer(need, hint, Then::Again { again, row });
    }

    /// The box for `need`'s tool; Download fetches it and then runs `then`. While it is on
    /// screen, more that need the same tool join it.
    fn offer(&self, need: Need, hint: Option<String>, then: Then) {
        let tool = need.tool();
        self.0.waiting.borrow_mut().push((tool, then));
        if !self.0.offering.borrow().contains(&tool) {
            self.0.offering.borrow_mut().push(tool);
            self.show_offer(need, hint);
        }
    }

    /// Takes what waits for `tool`: its box closed.
    fn take_waiting(&self, tool: Tool) -> Vec<Then> {
        self.0.offering.borrow_mut().retain(|t| *t != tool);
        let mut waiting = self.0.waiting.borrow_mut();
        let (mine, others): (Vec<_>, Vec<_>) = std::mem::take(&mut *waiting).into_iter().partition(|(t, _)| *t == tool);
        *waiting = others;
        mine.into_iter().map(|(_, then)| then).collect()
    }

    fn show_offer(&self, need: Need, hint: Option<String>) {
        let tools = tools_settings();
        let tool = need.tool();
        let build = Platform::current().and_then(|platform| build_for(tool, platform));
        let (title, message, buttons) =
            tool_offer(&need, build.map(|b| b.size), tools.download, cfg!(target_os = "linux"), hint.as_deref());
        let offers = buttons.len() > 1;
        let escape = buttons.len() - 1;
        let this = self.clone();
        self.0.dialogs.ask_escape(title, message, &buttons, escape, move |choice| {
            match (choice, build) {
                (Some(1), _) if offers => {
                    // One page: the build's release, whose notes link the licence and sources.
                    let page = build.map_or_else(|| TOOLS_PAGE.to_owned(), |build| release_page(build.url));
                    let _ = open::that_detached(page);
                    // The same box again, with what waits for it.
                    this.show_offer(need, hint);
                }
                (Some(0), Some(build)) if offers => {
                    let then = this.take_waiting(tool);
                    this.download(build, then);
                }
                _ => {
                    this.take_waiting(tool);
                }
            }
        });
    }

    fn download(&self, build: &'static ToolBuild, then: Vec<Then>) {
        let task = DownloadTask::new(build, self.data_dir());
        let id = self.0.ops.submit(Box::new(task), None, After::Nothing);
        self.0.jobs.borrow_mut().insert(id, Pending::Download { build, then });
    }

    // Compressing.

    /// "Compress to "<name>"": at once, next to the items, with the format and level used
    /// last.
    pub fn compress_to(&self, items: Vec<(PathBuf, bool)>) {
        let Some(folder) = items.first().and_then(|(p, _)| p.parent()).map(Path::to_path_buf) else { return };
        let level = self.last_level();
        let format = written_format(compress_format(&items, self.last_format()), level);
        let target = folder.join(default_name(&items, format));
        let options = CompressOptions { format, level, password: None, encrypt_names: false, split: None };
        let sources: Vec<PathBuf> = items.into_iter().map(|(p, _)| p).collect();
        self.submit_compress(sources, target, options);
    }

    fn submit_compress(&self, sources: Vec<PathBuf>, target: PathBuf, options: CompressOptions) {
        let retry: Rc<dyn Fn() -> Box<dyn gezik_ops::Task>> =
            Rc::new(move || Box::new(CompressTask::new(sources.clone(), target.clone(), options.clone())));
        self.0.ops.submit(retry(), Some(retry), After::Select);
    }

    /// Adds `sources` to `archive` (`password`: the archive's, if known).
    pub fn add_to(&self, archive: PathBuf, sources: Vec<PathBuf>, password: Option<String>) {
        let retry: Rc<dyn Fn() -> Box<dyn gezik_ops::Task>> =
            Rc::new(move || Box::new(AddToArchiveTask::new(archive.clone(), sources.clone(), password.clone())));
        self.0.ops.submit(retry(), Some(retry), After::Select);
    }

    pub fn is_open(&self) -> bool {
        self.0.layer.borrow().is_some()
    }

    /// "Compress…": the layer for `items` (path, is a folder).
    pub fn open_compress(&self, items: Vec<(PathBuf, bool)>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let Some(folder) = items.first().and_then(|(p, _)| p.parent()).map(Path::to_path_buf) else { return };
        if self.is_open() {
            return;
        }
        let formats = formats_for(&items);
        let format = compress_format(&items, self.last_format());
        let (split, split_mb) = split_choice(self.0.state.borrow().split);
        let title = match items.as_slice() {
            [(one, _)] => format!("Compress {}", name_of(one)),
            many => format!("Compress {} items", many.len()),
        };
        window.set_cp_title(title.into());
        window.set_cp_name(default_name(&items, written_format(format, self.last_level())).into());
        window.set_cp_folder(folder.display().to_string().into());
        let labels: Vec<SharedString> = formats.iter().map(|f| format_label(*f).into()).collect();
        window.set_cp_formats(ModelRc::new(VecModel::from(labels)));
        window.set_cp_password("".into());
        window.set_cp_split_mb(split_mb.into());
        window.set_cp_error("".into());
        window.set_cp_size_text("Calculating…".into());
        let sources: Vec<PathBuf> = items.into_iter().map(|(p, _)| p).collect();
        *self.0.layer.borrow_mut() = Some(Layer {
            sources: sources.clone(),
            folder,
            format: formats.iter().position(|f| *f == format).unwrap_or(0),
            formats,
            level: self.last_level(),
            encrypt_names: false,
            split,
        });
        self.show_layer();
        window.set_cp_open(true);
        // The size is added up on a thread; a newer opening drops it.
        let opening = self.0.sizing.fetch_add(1, Ordering::SeqCst) + 1;
        let sizing = self.0.sizing.clone();
        std::thread::spawn(move || {
            let (files, bytes) = total_size(&sources, || sizing.load(Ordering::SeqCst) != opening);
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| this.sized(opening, files, bytes));
            });
        });
    }

    fn sized(&self, opening: u64, files: u64, bytes: u64) {
        if self.0.sizing.load(Ordering::SeqCst) != opening || !self.is_open() {
            return;
        }
        if let Some(window) = self.0.window.upgrade() {
            window.set_cp_size_text(size_text(files, bytes).into());
        }
    }

    /// Shows the layer's choices.
    fn show_layer(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let layer = self.0.layer.borrow();
        let Some(layer) = layer.as_ref() else { return };
        let format = layer.format();
        window.set_cp_format(i32::try_from(layer.format).unwrap_or(0));
        window.set_cp_level(LEVELS.iter().position(|(l, _)| *l == layer.level).unwrap_or(2) as i32);
        window.set_cp_can_encrypt(matches!(format, OutFormat::Zip | OutFormat::SevenZ));
        window.set_cp_is_7z(format == OutFormat::SevenZ);
        window.set_cp_encrypt_names(layer.encrypt_names);
        window.set_cp_split(i32::try_from(layer.split).unwrap_or(0));
        window.set_cp_note(store_note(format, layer.level).into());
    }

    fn edit(&self, f: impl FnOnce(&mut Layer)) {
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            f(layer);
        }
        self.set_error("");
        self.show_layer();
    }

    /// Another format (an index into the layer's formats) or level: the name's ending follows
    /// what is written.
    fn choose(&self, format: Option<usize>, level: Option<Level>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let mut renamed = None;
        if let Some(layer) = self.0.layer.borrow_mut().as_mut() {
            let from = (layer.format(), layer.level);
            if let Some(index) = format.filter(|i| *i < layer.formats.len()) {
                layer.format = index;
            }
            if let Some(level) = level {
                layer.level = level;
            }
            renamed = Some(renamed_for(&window.get_cp_name(), from, (layer.format(), layer.level)));
        }
        if let Some(name) = renamed {
            window.set_cp_name(name.into());
        }
        self.set_error("");
        self.show_layer();
    }

    fn set_error(&self, error: &str) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_cp_error(error.into());
        }
    }

    /// Esc closes the layer and Ctrl+Enter (Cmd on macOS) compresses, wherever its focus is;
    /// returns whether the key was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        let primary = crate::keys::is_primary(chord, KeyPlatform::current());
        match chord.key {
            Key::Escape if !primary && !chord.shift => self.close(),
            Key::Enter if primary => self.compress(),
            _ => return false,
        }
        true
    }

    pub fn close(&self) {
        self.0.layer.borrow_mut().take();
        self.0.sizing.fetch_add(1, Ordering::SeqCst);
        if let Some(window) = self.0.window.upgrade() {
            window.set_cp_open(false);
            // The password does not stay in the window.
            window.set_cp_password("".into());
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }

    /// Compress: checks the name, folder and part size, then starts the job.
    fn compress(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let checked = {
            let layer = self.0.layer.borrow();
            let Some(layer) = layer.as_ref() else { return };
            self.check(&window, layer)
        };
        let (sources, target, options) = match checked {
            Ok(job) => job,
            Err(error) => return self.set_error(&error),
        };
        let split = options.split;
        let (format, level) = (options.format, options.level);
        self.save_state(|state| {
            state.format = Some(format.extension().to_owned());
            state.level = Some(level_key(level).to_owned());
            if format == OutFormat::SevenZ {
                state.split = split;
            }
        });
        self.close();
        self.submit_compress(sources, target, options);
    }

    fn check(&self, window: &AppWindow, layer: &Layer) -> Result<(Vec<PathBuf>, PathBuf, CompressOptions), String> {
        let name = window.get_cp_name().trim().to_owned();
        validate_name(&name, NameRules::current()).map_err(|err| err.to_string())?;
        let folder = resolve_folder(&window.get_cp_folder(), &layer.folder).unwrap_or_else(|| layer.folder.clone());
        let format = layer.format();
        let split =
            if format == OutFormat::SevenZ { split_bytes(layer.split, &window.get_cp_split_mb())? } else { None };
        let password = window.get_cp_password().to_string();
        let password =
            (matches!(format, OutFormat::Zip | OutFormat::SevenZ) && !password.is_empty()).then_some(password);
        let options = CompressOptions {
            format,
            level: layer.level,
            encrypt_names: format == OutFormat::SevenZ && password.is_some() && layer.encrypt_names,
            password,
            split,
        };
        Ok((layer.sources.clone(), folder.join(name), options))
    }

    /// "Add to existing archive…": asks for the archive, then adds the layer's items to it.
    fn ask_existing(&self) {
        let Some(base) = self.0.layer.borrow().as_ref().map(|layer| layer.folder.clone()) else { return };
        let this = self.clone();
        self.0.dialogs.ask_text("Add to archive", "Archive:", "", &["Add", "Cancel"], move |text| {
            let Some(archive) = text.and_then(|text| resolve_folder(&text, &base)) else { return };
            let Some(sources) = this.0.layer.borrow().as_ref().map(|layer| layer.sources.clone()) else { return };
            this.close();
            // The password typed is for a new archive; the task asks for this one's if needed.
            this.add_to(archive, sources, None);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[(u32, String)]) -> Vec<u32> {
        items.iter().map(|(id, _)| *id).collect()
    }

    #[test]
    fn menu_items_follow_the_selection() {
        let zip = (PathBuf::from("d").join("Fotolar.zip"), false);
        let rar = (PathBuf::from("d").join("b.part1.rar"), false);
        let text = (PathBuf::from("d").join("notes.txt"), false);
        let folder = (PathBuf::from("d").join("Belgeler"), true);

        let one = menu_items(std::slice::from_ref(&zip), OutFormat::Zip, Level::Normal);
        assert_eq!(ids(&one), [EXTRACT_HERE, EXTRACT_TO_OWN, EXTRACT_TO, COMPRESS, COMPRESS_TO]);
        assert_eq!(one[1].1, format!("Extract to \"Fotolar{MAIN_SEPARATOR}\""));
        assert_eq!(one[4].1, "Compress to \"Fotolar.zip\"");

        let two = menu_items(&[zip.clone(), rar], OutFormat::SevenZ, Level::Normal);
        assert_eq!(two[1].1, "Extract each to its own folder");
        assert_eq!(two[4].1, "Compress to \"d.7z\"");

        // An archive with something else: only Compress.
        assert_eq!(ids(&menu_items(&[zip, text.clone()], OutFormat::Zip, Level::Normal)), [COMPRESS, COMPRESS_TO]);
        assert_eq!(
            menu_items(std::slice::from_ref(&folder), OutFormat::TarGz, Level::Normal)[1].1,
            "Compress to \"Belgeler.tar.gz\""
        );
        // .gz is for one file; a folder gets zip.
        assert_eq!(
            menu_items(std::slice::from_ref(&text), OutFormat::Gz, Level::Best)[1].1,
            "Compress to \"notes.txt.gz\""
        );
        assert_eq!(
            menu_items(std::slice::from_ref(&folder), OutFormat::Gz, Level::Normal)[1].1,
            "Compress to \"Belgeler.zip\""
        );
        // Store makes a plain tar of a tar.gz or tar.xz: the item names what is made.
        assert_eq!(
            menu_items(std::slice::from_ref(&folder), OutFormat::TarXz, Level::Store)[1].1,
            "Compress to \"Belgeler.tar\""
        );
        assert_eq!(menu_items(&[folder], OutFormat::Zip, Level::Store)[1].1, "Compress to \"Belgeler.zip\"");
        assert!(menu_items(&[], OutFormat::Zip, Level::Normal).is_empty());
    }

    #[test]
    fn formats_and_names() {
        let file = [(PathBuf::from("a.txt"), false)];
        assert_eq!(formats_for(&file).len(), 7);
        assert_eq!(formats_for(&[(PathBuf::from("a"), true)]).len(), 5);
        assert_eq!(format_label(OutFormat::Gz), ".gz");
        assert_eq!(format_label(OutFormat::TarXz), "tar.xz");
        assert_eq!(format_from_key("tar.gz"), Some(OutFormat::TarGz));
        assert_eq!(format_from_key("rar"), None);
        assert_eq!(level_from_key("best"), Some(Level::Best));
        assert_eq!(level_key(Level::Fast), "fast");
        assert_eq!(swap_extension("Fotolar.zip", OutFormat::Zip, OutFormat::SevenZ), "Fotolar.7z");
        assert_eq!(swap_extension("Fotolar.ZIP", OutFormat::Zip, OutFormat::TarGz), "Fotolar.tar.gz");
        assert_eq!(swap_extension("Fotolar.tar.gz", OutFormat::TarGz, OutFormat::Tar), "Fotolar.tar");
        assert_eq!(swap_extension("my name", OutFormat::Zip, OutFormat::SevenZ), "my name.7z");
        assert_eq!(default_name(&file, OutFormat::Zip), "a.zip");
    }

    #[test]
    fn store_makes_a_plain_tar() {
        assert_eq!(written_format(OutFormat::TarGz, Level::Store), OutFormat::Tar);
        assert_eq!(written_format(OutFormat::TarXz, Level::Store), OutFormat::Tar);
        assert_eq!(written_format(OutFormat::TarGz, Level::Fast), OutFormat::TarGz);
        assert_eq!(written_format(OutFormat::Zip, Level::Store), OutFormat::Zip);
        assert_eq!(store_note(OutFormat::TarGz, Level::Store), "Store makes a plain .tar");
        assert_eq!(store_note(OutFormat::SevenZ, Level::Store), "");
        let (gz, xz, zip) =
            ((OutFormat::TarGz, Level::Normal), (OutFormat::TarXz, Level::Store), (OutFormat::Zip, Level::Store));
        // Store on a tar.gz: the name becomes .tar; back to Normal: .tar.gz again.
        assert_eq!(renamed_for("x.tar.gz", gz, (OutFormat::TarGz, Level::Store)), "x.tar");
        assert_eq!(renamed_for("x.tar", (OutFormat::TarGz, Level::Store), gz), "x.tar.gz");
        assert_eq!(renamed_for("x.tar", xz, zip), "x.zip");
        assert_eq!(renamed_for("x.zip", zip, (OutFormat::Zip, Level::Best)), "x.zip");
        assert_eq!(renamed_for("x.zip", zip, xz), "x.tar");
    }

    #[test]
    fn only_what_needs_seven_zip_waits_for_it() {
        let (zip, lzh, arj) = (PathBuf::from("d/a.zip"), PathBuf::from("d/b.lzh"), PathBuf::from("d/c.arj"));
        let all = vec![zip.clone(), lzh.clone(), arj.clone()];
        assert_eq!(split_by_seven_zip(all.clone(), false), (vec![zip.clone()], vec![lzh.clone(), arj.clone()]));
        assert_eq!(split_by_seven_zip(all.clone(), true), (all, Vec::new()));
        assert_eq!(split_by_seven_zip(vec![lzh.clone()], false), (Vec::new(), vec![lzh]));
        assert_eq!(split_by_seven_zip(vec![zip.clone()], false), (vec![zip], Vec::new()));
    }

    #[test]
    fn extract_to_resolves_against_the_folder() {
        let base = std::env::temp_dir();
        assert_eq!(resolve_folder("  ", &base), None);
        assert_eq!(resolve_folder("out", &base), Some(base.join("out")));
        let absolute = base.join("abs");
        assert_eq!(resolve_folder(&absolute.display().to_string(), Path::new("elsewhere")), Some(absolute));
    }

    #[test]
    fn questions_read_well() {
        let archive = PathBuf::from("d").join("gizli.zip");
        assert_eq!(password_text(&archive, false), ("Password".into(), "gizli.zip is encrypted. Password:".into()));
        assert_eq!(password_text(&archive, true).1, "Wrong password. Try again:");

        let (message, buttons) = seven_zip_offer(".lzh", Some(1_700_000), true, false);
        assert!(message.starts_with("Opening .lzh archives needs 7-Zip (~1.6 MB, free)."), "{message}");
        assert_eq!(buttons, ["Download", "Where does it come from?", "Cancel"]);
        let (linux, _) = seven_zip_offer(".lzh", Some(1_700_000), true, true);
        assert!(linux.ends_with("Download it, or install 7-Zip 25 or newer yourself."), "{linux}");
        assert!(!linux.contains("p7zip"), "{linux}");
        let (off, buttons) = seven_zip_offer(".lzh", Some(1_700_000), false, false);
        assert_eq!(buttons, ["OK"]);
        assert!(off.contains("[tools]"), "{off}");
        let (none, buttons) = seven_zip_offer(".arj", None, true, false);
        assert_eq!(buttons, ["OK"]);
        assert!(none.contains("cannot download it for this system yet"), "{none}");
        assert_eq!(extension_of(Path::new("d/A.LZH")), ".lzh");
        assert!(needs_seven_zip("a.lzh") && !needs_seven_zip("a.zip"));
    }

    #[test]
    fn where_it_comes_from_is_the_release_page() {
        let url = "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-linux-arm64.7z";
        assert_eq!(release_page(url), "https://github.com/wenlar/gezik-tools/releases/tag/ffmpeg-9.0.2-1");
        assert_eq!(release_page("https://example.com/a.zip"), TOOLS_PAGE);
        // Every build of this system has one.
        for tool in [Tool::SevenZip, Tool::Ffmpeg] {
            if let Some(build) = Platform::current().and_then(|p| build_for(tool, p)) {
                assert!(release_page(build.url).contains("/releases/tag/"), "{}", build.url);
            }
        }
    }

    #[test]
    fn one_box_offers_each_tool() {
        // 7-Zip's box is as before.
        let (title, message, buttons) = tool_offer(&Need::Archive(".lzh".into()), Some(1_700_000), true, false, None);
        assert_eq!((title, buttons.len()), ("7-Zip needed", 3));
        assert_eq!(message, seven_zip_offer(".lzh", Some(1_700_000), true, false).0);

        let size = Some(31_000_000);
        let (title, message, buttons) = tool_offer(&Need::Media, size, true, false, None);
        assert_eq!(title, "ffmpeg needed");
        assert_eq!(message, "Video conversion needs ffmpeg (~29.6 MB, free).");
        assert_eq!(buttons, ["Download", "Where does it come from?", "Cancel"]);
        // Linux: a package does for video, not for HEIC (packages are often older than 9).
        let linux = tool_offer(&Need::Media, size, true, true, None).1;
        assert!(linux.ends_with("Download it, or install it yourself (sudo apt install ffmpeg)."), "{linux}");
        let heic = tool_offer(&Need::Heic, size, true, true, None).1;
        assert!(heic.contains("ffmpeg 9 or newer") && !heic.contains("apt"), "{heic}");
        assert!(tool_offer(&Need::NewerFfmpeg, size, true, false, None).1.contains("older"));
        // An older one set in settings.toml would come before a download: no Download then.
        let (_, configured, buttons) = tool_offer(&Need::ConfiguredTooOld, size, true, false, None);
        assert!(configured.contains("[convert]") && configured.contains("9 or newer"), "{configured}");
        assert_eq!(buttons, ["OK"]);
        // Nothing that downloads (Linux without curl or wget): the box says what to install.
        let hint = "Install curl with your package manager (sudo apt install curl)";
        let (_, message, buttons) = tool_offer(&Need::Pictures, size, true, true, Some(hint));
        assert!(message.contains("(sudo apt install curl) to download it"), "{message}");
        assert_eq!(buttons, ["OK"]);
        // [tools] download = false: only says so.
        let (_, off, buttons) = tool_offer(&Need::Media, size, false, false, None);
        assert!(off.contains("[convert]") && buttons == ["OK"], "{off}");
        let (_, none, buttons) = tool_offer(&Need::Media, None, true, false, None);
        assert!(none.contains("cannot download it for this system yet") && buttons == ["OK"], "{none}");
    }

    #[test]
    fn split_sizes_round_trip() {
        assert_eq!(split_choice(None), (0, String::new()));
        assert_eq!(split_choice(Some(700_000_000)), (2, String::new()));
        assert_eq!(split_choice(Some(250_000_000)), (4, "250".to_owned()));
        assert_eq!(split_bytes(0, ""), Ok(None));
        assert_eq!(split_bytes(3, ""), Ok(Some(4_294_967_295)));
        assert_eq!(split_bytes(4, " 250 "), Ok(Some(250_000_000)));
        assert!(split_bytes(4, "0").is_err() && split_bytes(4, "x").is_err());
    }

    #[test]
    fn sizes_add_up() {
        let dir = std::env::temp_dir().join(format!("gezik-archives-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"12345").unwrap();
        std::fs::write(dir.join("sub").join("b.txt"), b"123").unwrap();
        assert_eq!(total_size(std::slice::from_ref(&dir), || false), (2, 8));
        assert_eq!(size_text(2, 8), "Size: 8 B in 2 files");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
