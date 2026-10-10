//! shared-mime-info and the MIME application lists for Linux (spec 9 §8.5): a file's type
//! by its name (`globs2`; no content sniffing, decision 13), its parents (`subclasses`), its
//! icon names, and which apps open it (mimeapps.list in the XDG order, mimeinfo.cache, the
//! legacy defaults.list). Read when needed, never ahead. Compiled everywhere: tested on
//! Windows with files in a temp folder.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// shared-mime-info's files, list files and caches bigger than this are not read.
pub const MAX_LIST_BYTES: u64 = 4 * 1024 * 1024;
/// The most types a type and its parents make.
const MAX_TYPES: usize = 16;

/// The XDG base folders (XDG Base Directory spec 0.8), each made absolute or dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Xdg {
    pub home: PathBuf,
    pub data_home: PathBuf,
    pub data_dirs: Vec<PathBuf>,
    pub config_home: PathBuf,
    pub config_dirs: Vec<PathBuf>,
    /// `$XDG_CURRENT_DESKTOP`'s parts, lower case; ones that could not be a file name part are dropped.
    pub desktops: Vec<String>,
}

impl Xdg {
    /// From the environment's values (`get`) and the home folder, with the spec's defaults.
    pub fn from(get: &dyn Fn(&str) -> Option<String>, home: &Path) -> Xdg {
        let one = |key: &str, default: PathBuf| get(key).map(PathBuf::from).filter(|p| p.has_root()).unwrap_or(default);
        let many = |key: &str, default: &[&str]| {
            let found: Vec<PathBuf> =
                get(key).unwrap_or_default().split(':').map(PathBuf::from).filter(|p| p.has_root()).collect();
            if found.is_empty() { default.iter().map(PathBuf::from).collect() } else { found }
        };
        Xdg {
            home: home.to_path_buf(),
            data_home: one("XDG_DATA_HOME", home.join(".local/share")),
            data_dirs: many("XDG_DATA_DIRS", &["/usr/local/share", "/usr/share"]),
            config_home: one("XDG_CONFIG_HOME", home.join(".config")),
            config_dirs: many("XDG_CONFIG_DIRS", &["/etc/xdg"]),
            desktops: get("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .filter(|d| {
                    !d.is_empty() && d.len() <= 64 && d.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
                })
                .map(str::to_lowercase)
                .collect(),
        }
    }

    /// `$XDG_DATA_HOME/<sub>`, then each `$XDG_DATA_DIRS/<sub>`.
    pub fn data(&self, sub: &str) -> Vec<PathBuf> {
        std::iter::once(&self.data_home).chain(&self.data_dirs).map(|d| d.join(sub)).collect()
    }

    /// The mimeapps.list files, most important first (XDG MIME Applications spec 1.0.1).
    pub fn mimeapps_files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let dirs = std::iter::once(self.config_home.clone())
            .chain(self.config_dirs.iter().cloned())
            .chain(self.data("applications"));
        for dir in dirs {
            out.extend(self.desktops.iter().map(|d| dir.join(format!("{d}-mimeapps.list"))));
            out.push(dir.join("mimeapps.list"));
        }
        out
    }
}

/// This process's XDG folders, read once.
pub fn xdg() -> Option<&'static Xdg> {
    static XDG: OnceLock<Option<Xdg>> = OnceLock::new();
    XDG.get_or_init(|| dirs::home_dir().map(|home| Xdg::from(&|key| std::env::var(key).ok(), &home))).as_ref()
}

/// The MIME database, read on first use (a type name or an icon) and kept.
pub fn db() -> Option<&'static MimeDb> {
    static DB: OnceLock<Option<MimeDb>> = OnceLock::new();
    DB.get_or_init(|| xdg().map(MimeDb::load)).as_ref()
}

/// The UTF-8 text of `path` if it is a regular file of at most `max` bytes.
pub fn read_small(path: &Path, max: u64) -> Option<String> {
    // Checked before opening: opening a FIFO would wait for a writer.
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    let mut text = String::new();
    // The file may grow after the check: never read past the limit.
    std::fs::File::open(path).ok()?.take(max.saturating_add(1)).read_to_string(&mut text).ok()?;
    (text.len() as u64 <= max).then_some(text)
}

/// `media/subtype`, the characters shared-mime-info uses.
pub fn mime_like(text: &str) -> bool {
    let Some((media, sub)) = text.split_once('/') else { return false };
    !media.is_empty()
        && !sub.is_empty()
        && !sub.contains('/')
        && text.len() <= 255
        && text.chars().all(|c| c.is_ascii_alphanumeric() || "/.+-_".contains(c))
}

/// An icon name that is safe as a file name.
pub fn icon_name_like(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 255
        && !text.starts_with('.')
        && text.chars().all(|c| c.is_ascii_alphanumeric() || ".+-_@".contains(c))
}

/// A desktop id from a list file: a plain file name ending in `.desktop`.
pub fn desktop_id_like(id: &str) -> bool {
    id.len() > ".desktop".len()
        && id.len() <= 255
        && id.ends_with(".desktop")
        && !id.starts_with('.')
        && !id.contains(['/', '\\', '\0'])
        && !id.chars().any(char::is_control)
}

/// Types by name, their parents and their icon names.
#[derive(Debug, Default)]
pub struct MimeDb {
    /// `*.suffix` (lower case; `tar.gz` for `*.tar.gz`) → (weight, type).
    suffixes: HashMap<String, (u32, String)>,
    /// Case-sensitive `*.suffix` globs (`:cs`) → (weight, type).
    exact_suffixes: HashMap<String, (u32, String)>,
    /// Whole names (`Makefile`) → type.
    literals: HashMap<String, String>,
    parents: HashMap<String, Vec<String>>,
    icons: HashMap<String, String>,
    generic: HashMap<String, String>,
}

impl MimeDb {
    /// `globs2`, `subclasses`, `icons`, `generic-icons` of each `mime` data folder, the
    /// user's first (decision 13).
    pub fn load(xdg: &Xdg) -> MimeDb {
        let mut db = MimeDb::default();
        for dir in xdg.data("mime") {
            let read = |name: &str| read_small(&dir.join(name), MAX_LIST_BYTES);
            if let Some(text) = read("globs2") {
                db.add_globs(&text);
            }
            if let Some(text) = read("subclasses") {
                db.add_subclasses(&text);
            }
            if let Some(text) = read("icons") {
                db.add_icons(&text, false);
            }
            if let Some(text) = read("generic-icons") {
                db.add_icons(&text, true);
            }
        }
        db
    }

    /// A `globs2` text: `weight:type:glob[:flags]`, highest weight first; a pattern already
    /// known keeps its type unless this one weighs more.
    pub fn add_globs(&mut self, text: &str) {
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let mut parts = line.split(':');
            let (Some(weight), Some(mime), Some(glob)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let case_sensitive = parts.next().is_some_and(|flags| flags.split(',').any(|f| f == "cs"));
            let Ok(weight) = weight.trim().parse::<u32>() else { continue };
            if !mime_like(mime) || glob.is_empty() || glob.len() > 255 {
                continue;
            }
            let wild = |s: &str| s.contains(['*', '?', '[']);
            let keep = |map: &mut HashMap<String, (u32, String)>, key: String| {
                if map.get(&key).is_none_or(|(known, _)| weight > *known) {
                    map.insert(key, (weight, mime.to_owned()));
                }
            };
            if let Some(suffix) = glob.strip_prefix("*.")
                && !wild(suffix)
            {
                if case_sensitive {
                    keep(&mut self.exact_suffixes, suffix.to_owned());
                } else {
                    keep(&mut self.suffixes, suffix.to_lowercase());
                }
            } else if !wild(glob) {
                self.literals.entry(glob.to_owned()).or_insert_with(|| mime.to_owned());
            }
        }
    }

    /// A `subclasses` text: `child parent` per line.
    pub fn add_subclasses(&mut self, text: &str) {
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            if let (Some(child), Some(parent)) = (parts.next(), parts.next())
                && mime_like(child)
                && mime_like(parent)
            {
                let parents = self.parents.entry(child.to_owned()).or_default();
                if !parents.iter().any(|p| p == parent) {
                    parents.push(parent.to_owned());
                }
            }
        }
    }

    /// An `icons` or `generic-icons` text: `type:icon-name` per line; the first folder's wins.
    pub fn add_icons(&mut self, text: &str, generic: bool) {
        let map = if generic { &mut self.generic } else { &mut self.icons };
        for line in text.lines() {
            if let Some((mime, icon)) = line.split_once(':')
                && mime_like(mime)
                && icon_name_like(icon)
            {
                map.entry(mime.to_owned()).or_insert_with(|| icon.to_owned());
            }
        }
    }

    /// The type of a file named `name`, by its name alone (decision 13): a whole-name glob,
    /// else the heaviest suffix glob, the longest at equal weight; a case-sensitive glob
    /// before a case-insensitive one for the same suffix.
    pub fn type_of_name(&self, name: &str) -> Option<String> {
        if let Some(mime) = self.literals.get(name) {
            return Some(mime.clone());
        }
        let mut best: Option<(u32, usize, &String)> = None;
        for (dot, _) in name.match_indices('.') {
            // `.` is ASCII: `dot + 1` is always a character boundary.
            let suffix = &name[dot + 1..];
            let found = self.exact_suffixes.get(suffix).or_else(|| self.suffixes.get(&suffix.to_lowercase()));
            if let Some((weight, mime)) = found
                && best.is_none_or(|(w, len, _)| (*weight, suffix.len()) > (w, len))
            {
                best = Some((*weight, suffix.len(), mime));
            }
        }
        best.map(|(_, _, mime)| mime.clone())
    }

    /// `mime`, then its parents breadth first, `text/*` also under `text/plain`; at most 16.
    /// No `application/octet-stream`: it would offer every binary editor.
    pub fn with_parents(&self, mime: &str) -> Vec<String> {
        let mut out = vec![mime.to_owned()];
        let mut i = 0;
        while i < out.len() && out.len() < MAX_TYPES {
            for parent in self.parents.get(&out[i]).cloned().unwrap_or_default() {
                if !out.contains(&parent) {
                    out.push(parent);
                }
            }
            i += 1;
        }
        if mime.starts_with("text/") && !out.iter().any(|m| m == "text/plain") {
            out.push("text/plain".to_owned());
        }
        out.truncate(MAX_TYPES);
        out
    }

    /// The icon names for `mime` (shared-mime-info spec): its `icons` entry, the type with
    /// `-` for `/`, its `generic-icons` entry, `<media>-x-generic`.
    pub fn icon_names(&self, mime: &str) -> Vec<String> {
        let media = mime.split('/').next().unwrap_or_default();
        let mut names: Vec<String> = Vec::new();
        let candidates = [
            self.icons.get(mime).cloned(),
            Some(mime.replace('/', "-")),
            self.generic.get(mime).cloned(),
            Some(format!("{media}-x-generic")),
        ];
        for name in candidates.into_iter().flatten() {
            if icon_name_like(&name) && !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}

/// The system's name for a type (the Type column), from the type's XML comment.
pub fn type_name(ext: &str, is_dir: bool) -> Option<String> {
    let mime = if is_dir { "inode/directory".to_owned() } else { db()?.type_of_name(&format!("x.{ext}"))? };
    // `mime` passed `mime_like`: safe as a path part.
    let xml = xdg()?.data("mime").iter().find_map(|dir| read_small(&dir.join(format!("{mime}.xml")), 1024 * 1024))?;
    comment(&xml)
}

/// The first `<comment>` without a language: the English description.
fn comment(xml: &str) -> Option<String> {
    let start = xml.find("<comment>")? + "<comment>".len();
    let end = start + xml[start..].find("</comment>")?;
    let text = xml[start..end]
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    Some(text)
}

/// `type → desktop ids` of one section.
pub type Lists = HashMap<String, Vec<String>>;

/// The `[section]` of an INI-like text as lists: `type=a.desktop;b.desktop;`. Keys that are
/// not types and ids that are not desktop ids are dropped; the first line of a key counts.
pub fn section_lists(text: &str, section: &str) -> Lists {
    let mut lists = Lists::new();
    let mut inside = false;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name == section;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        if !mime_like(key) || lists.contains_key(key) {
            continue;
        }
        let ids: Vec<String> =
            value.split(';').map(str::trim).filter(|id| desktop_id_like(id)).map(str::to_owned).collect();
        lists.insert(key.to_owned(), ids);
    }
    lists
}

/// One mimeapps.list.
#[derive(Debug, Default)]
pub struct MimeAppsFile {
    pub default: Lists,
    pub added: Lists,
    pub removed: Lists,
}

/// The three sections of a mimeapps.list text.
pub fn parse_mimeapps(text: &str) -> MimeAppsFile {
    MimeAppsFile {
        default: section_lists(text, "Default Applications"),
        added: section_lists(text, "Added Associations"),
        removed: section_lists(text, "Removed Associations"),
    }
}

/// Every list that says which apps open which type.
#[derive(Debug, Default)]
pub struct Associations {
    /// mimeapps.list files, most important first.
    pub files: Vec<MimeAppsFile>,
    /// Each applications folder's mimeinfo.cache (or its desktop files' MimeType=), in order.
    pub caches: Vec<Lists>,
    /// The legacy defaults.list files.
    pub legacy: Vec<Lists>,
}

/// The default app and the apps for a file of `types` (its type, then its parents), by the
/// XDG MIME Applications rules (decision 14). `usable` says whether a desktop id is installed
/// and may be offered. The default comes first in the apps.
pub fn resolve(
    a: &Associations,
    types: &[String],
    usable: &mut dyn FnMut(&str) -> bool,
) -> (Option<String>, Vec<String>) {
    let mut default = None;
    'types: for mime in types {
        let defaults = a.files.iter().map(|f| &f.default).chain(&a.legacy);
        for id in defaults.filter_map(|lists| lists.get(mime)).flatten() {
            if usable(id) {
                default = Some(id.clone());
                break 'types;
            }
        }
    }
    let mut apps: Vec<String> = Vec::new();
    for mime in types {
        // A file's removals hide its own additions and those of the files below it.
        // shortcut: every file's removals also hide every mimeinfo.cache, even a more important
        // folder's; exact interleaving only if a user reports a missing app.
        let mut removed: Vec<&String> = Vec::new();
        for file in &a.files {
            removed.extend(file.removed.get(mime).into_iter().flatten());
            for id in file.added.get(mime).into_iter().flatten() {
                if !removed.contains(&id) && !apps.contains(id) && usable(id) {
                    apps.push(id.clone());
                }
            }
        }
        for id in a.caches.iter().filter_map(|cache| cache.get(mime)).flatten() {
            if !removed.contains(&id) && !apps.contains(id) && usable(id) {
                apps.push(id.clone());
            }
        }
    }
    let default = default.or_else(|| apps.first().cloned());
    if let Some(id) = &default {
        apps.retain(|app| app != id);
        apps.insert(0, id.clone());
    }
    (default, apps)
}

// ---- Loaders: read the files the lists name (each Open With menu reads them again) ----

use crate::linux::desktop_entry::{self as de, DesktopEntry};

/// The most desktop files read from one folder without a mimeinfo.cache, and by `all_apps`.
pub const MAX_DESKTOP_FILES: usize = 2000;

/// A desktop file's entry: at most `MAX_ENTRY_BYTES`, checked before reading; not UTF-8 → None.
pub fn read_entry(path: &Path) -> Option<DesktopEntry> {
    read_small(path, de::MAX_ENTRY_BYTES).and_then(|text| de::parse(&text))
}

/// `(desktop id, path)` of the desktop files in `dir` and one folder below it
/// (`kde4/a.desktop` is `kde4-a.desktop`), sorted by id, at most `MAX_DESKTOP_FILES`.
pub fn desktop_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let names = |dir: &Path| -> Vec<(String, PathBuf, bool)> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .take(MAX_DESKTOP_FILES)
            .filter_map(|e| {
                let kind = e.file_type().ok()?;
                Some((e.file_name().to_str()?.to_owned(), e.path(), kind.is_dir()))
            })
            .collect()
    };
    for (name, path, is_dir) in names(dir) {
        if is_dir {
            for (inner, inner_path, inner_dir) in names(&path) {
                let id = format!("{name}-{inner}");
                if !inner_dir && desktop_id_like(&id) {
                    out.push((id, inner_path));
                }
            }
        } else if desktop_id_like(&name) {
            out.push((name, path));
        }
    }
    out.sort();
    out.truncate(MAX_DESKTOP_FILES);
    out
}

/// A folder without a mimeinfo.cache: its desktop files' `MimeType=`.
fn scan_mime_types(dir: &Path) -> Lists {
    let mut lists = Lists::new();
    for (id, path) in desktop_files(dir) {
        for mime in read_entry(&path).map(|e| e.mime_types).unwrap_or_default() {
            if mime_like(&mime) {
                lists.entry(mime).or_default().push(id.clone());
            }
        }
    }
    lists
}

/// Every list as the files say now (decision 14).
pub fn associations(xdg: &Xdg) -> Associations {
    let files =
        xdg.mimeapps_files().iter().filter_map(|p| read_small(p, MAX_LIST_BYTES)).map(|t| parse_mimeapps(&t)).collect();
    let (mut caches, mut legacy) = (Vec::new(), Vec::new());
    for dir in xdg.data("applications") {
        caches.push(match read_small(&dir.join("mimeinfo.cache"), MAX_LIST_BYTES) {
            Some(text) => section_lists(&text, "MIME Cache"),
            None => scan_mime_types(&dir),
        });
        if let Some(text) = read_small(&dir.join("defaults.list"), MAX_LIST_BYTES) {
            legacy.push(section_lists(&text, "Default Applications"));
        }
    }
    Associations { files, caches, legacy }
}

/// The file of desktop id `id`: the first applications folder that has it, as `id` or
/// with its first `-` as `/` (shortcut: deeper folders are not tried; add them if an app is missed).
pub fn find_desktop(xdg: &Xdg, id: &str) -> Option<PathBuf> {
    if !desktop_id_like(id) {
        return None;
    }
    for dir in xdg.data("applications") {
        let direct = dir.join(id);
        if direct.is_file() {
            return Some(direct);
        }
        if let Some((sub, rest)) = id.split_once('-')
            && desktop_id_like(rest)
            && dir.join(sub).join(rest).is_file()
        {
            return Some(dir.join(sub).join(rest));
        }
    }
    None
}

/// Whether `program` is on this system's `PATH` (or an existing absolute path).
pub fn on_path(program: &str) -> bool {
    de::program_found(program, std::env::var("PATH").ok().as_deref(), &|p| p.is_file())
}

/// The default app and the apps for the file `path`, as desktop file paths (decision 10:
/// a NoDisplay app only as the default). `found` checks programs (`on_path`).
pub fn apps_of(
    xdg: &Xdg,
    db: &MimeDb,
    assoc: &Associations,
    path: &Path,
    found: &dyn Fn(&str) -> bool,
) -> (Option<PathBuf>, Vec<PathBuf>) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(mime) = db.type_of_name(&name) else { return (None, Vec::new()) };
    let types = db.with_parents(&mime);
    let mut known: HashMap<String, Option<(PathBuf, bool)>> = HashMap::new();
    let mut look = |id: &str| -> Option<(PathBuf, bool)> {
        known
            .entry(id.to_owned())
            .or_insert_with(|| {
                let file = find_desktop(xdg, id)?;
                let entry = read_entry(&file)?;
                de::usable(&entry, &xdg.desktops, found).then_some((file, !entry.no_display))
            })
            .clone()
    };
    let (default, ids) = resolve(assoc, &types, &mut |id| look(id).is_some());
    let default = default.and_then(|id| look(&id)).map(|(file, _)| file);
    let apps = ids
        .iter()
        .filter_map(|id| look(id))
        .filter(|(file, shown)| *shown || Some(file) == default.as_ref())
        .map(|(file, _)| file)
        .collect();
    (default, apps)
}

/// Every app Other… may offer (decision 2): each id once (the first folder's), usable, not
/// NoDisplay; by name.
pub fn all_apps(xdg: &Xdg, found: &dyn Fn(&str) -> bool) -> Vec<(PathBuf, DesktopEntry)> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in xdg.data("applications") {
        for (id, path) in desktop_files(&dir) {
            if out.len() >= MAX_DESKTOP_FILES || !seen.insert(id) {
                continue;
            }
            if let Some(entry) = read_entry(&path)
                && !entry.no_display
                && de::usable(&entry, &xdg.desktops, found)
            {
                out.push((path, entry));
            }
        }
    }
    out.sort_by_cached_key(|(path, e)| (e.name.to_lowercase(), path.clone()));
    out
}

/// An app's name for menus and messages: its entry's Name, else its file name.
pub fn app_name(path: &Path) -> String {
    read_entry(path)
        .map(|e| e.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn xdg_folders_and_their_defaults() {
        let x = Xdg::from(&env(&[]), Path::new("/home/u"));
        assert_eq!(x.data_home, Path::new("/home/u/.local/share"));
        assert_eq!(x.data_dirs, [PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")]);
        assert_eq!(x.config_home, Path::new("/home/u/.config"));
        assert_eq!(x.config_dirs, [PathBuf::from("/etc/xdg")]);
        let set = Xdg::from(
            &env(&[
                ("XDG_DATA_DIRS", "/nix/share::relative:/usr/share"),
                ("XDG_CONFIG_HOME", "rel"),
                ("XDG_CURRENT_DESKTOP", "ubuntu:GNOME:../x"),
            ]),
            Path::new("/home/u"),
        );
        assert_eq!(
            set.data_dirs,
            [PathBuf::from("/nix/share"), PathBuf::from("/usr/share")],
            "relative and empty parts dropped"
        );
        assert_eq!(set.config_home, Path::new("/home/u/.config"), "a relative XDG_CONFIG_HOME is ignored");
        assert_eq!(set.desktops, ["ubuntu", "gnome"], "a desktop that is not a name part is dropped");
        assert_eq!(set.data("applications")[0], Path::new("/home/u/.local/share/applications"));
    }

    #[test]
    fn the_xdg_order_decides() {
        let x = Xdg::from(&env(&[("XDG_CURRENT_DESKTOP", "KDE")]), Path::new("/h"));
        let files: Vec<String> = x.mimeapps_files().iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(
            files,
            [
                "/h/.config/kde-mimeapps.list",
                "/h/.config/mimeapps.list",
                "/etc/xdg/kde-mimeapps.list",
                "/etc/xdg/mimeapps.list",
                "/h/.local/share/applications/kde-mimeapps.list",
                "/h/.local/share/applications/mimeapps.list",
                "/usr/local/share/applications/kde-mimeapps.list",
                "/usr/local/share/applications/mimeapps.list",
                "/usr/share/applications/kde-mimeapps.list",
                "/usr/share/applications/mimeapps.list",
            ]
        );
    }

    fn db() -> MimeDb {
        let mut db = MimeDb::default();
        // The user's globs2 first (it wins at equal weight), then the system's.
        db.add_globs("# user\n50:application/x-mine:*.txt\n");
        db.add_globs(
            "# system\n80:application/x-compressed-tar:*.tar.gz\n50:application/gzip:*.gz\n50:text/plain:*.txt\n\
             50:text/x-c++src:*.C:cs\n50:text/x-csrc:*.c\n10:text/x-makefile:Makefile\n50:bad mime:*.bad\n\
             50:image/png:*.png\n40:image/x-weird:*.p?g\nnot-a-weight:a/b:*.ab\n::\n\u{0}\n",
        );
        db.add_subclasses(
            "text/x-csrc text/plain\napplication/x-loop application/x-pool\napplication/x-pool application/x-loop\n",
        );
        db.add_icons("application/x-compressed-tar:package-x-generic\nbad:../../etc\n", false);
        db.add_icons("text/x-csrc:text-x-script\n", true);
        db
    }

    #[test]
    fn types_come_from_names() {
        let db = db();
        assert_eq!(db.type_of_name("a.txt").as_deref(), Some("application/x-mine"), "the user's wins at equal weight");
        assert_eq!(
            db.type_of_name("Yedek.TAR.GZ").as_deref(),
            Some("application/x-compressed-tar"),
            "the longest suffix, any case"
        );
        assert_eq!(db.type_of_name("a.gz").as_deref(), Some("application/gzip"));
        assert_eq!(db.type_of_name("a.C").as_deref(), Some("text/x-c++src"), "a case-sensitive glob first");
        assert_eq!(db.type_of_name("a.c").as_deref(), Some("text/x-csrc"));
        assert_eq!(db.type_of_name("Makefile").as_deref(), Some("text/x-makefile"));
        assert_eq!(db.type_of_name("README"), None);
        assert_eq!(db.type_of_name("x.bad"), None, "a malformed type is dropped");
        assert_eq!(db.type_of_name("x.ab"), None, "a malformed weight is dropped");
        assert_eq!(db.type_of_name("x.pxg"), None, "wildcard globs are not used");
        assert_eq!(db.type_of_name("İ.PNG").as_deref(), Some("image/png"), "lower-casing never splits a character");
        assert_eq!(db.type_of_name(""), None);
        assert_eq!(db.type_of_name("..."), None);
    }

    #[test]
    fn parents_stop_at_a_loop() {
        let db = db();
        assert_eq!(db.with_parents("text/x-csrc"), ["text/x-csrc", "text/plain"]);
        assert_eq!(
            db.with_parents("text/x-makefile"),
            ["text/x-makefile", "text/plain"],
            "text/* falls back to text/plain"
        );
        assert_eq!(db.with_parents("application/x-loop"), ["application/x-loop", "application/x-pool"]);
        assert_eq!(db.with_parents("image/png"), ["image/png"], "no octet-stream");
        let mut wide = MimeDb::default();
        wide.add_subclasses(&(0..40).map(|i| format!("a/x a/p{i}\n")).collect::<String>());
        assert_eq!(wide.with_parents("a/x").len(), MAX_TYPES);
    }

    #[test]
    fn icon_names_follow_shared_mime_info() {
        let db = db();
        assert_eq!(
            db.icon_names("application/x-compressed-tar"),
            ["package-x-generic", "application-x-compressed-tar", "application-x-generic"]
        );
        assert_eq!(db.icon_names("text/x-csrc"), ["text-x-csrc", "text-x-script", "text-x-generic"]);
        assert!(
            icon_name_like("folder-documents")
                && !icon_name_like("../x")
                && !icon_name_like("a/b")
                && !icon_name_like("")
        );
        assert!(
            mime_like("image/svg+xml")
                && !mime_like("image")
                && !mime_like("a/b/c")
                && !mime_like("a/b\n")
                && !mime_like("a=b/c")
        );
        assert!(
            desktop_id_like("org.gnome.Nautilus.desktop")
                && !desktop_id_like("../x.desktop")
                && !desktop_id_like(".desktop")
                && !desktop_id_like("x.txt")
        );
    }

    #[test]
    fn lists_read_and_bad_keys_are_dropped() {
        let text = "\u{feff}[Default Applications]\r\nimage/png=eog.desktop;gimp.desktop;\nimage/png=later.desktop\nbad\nx=y.desktop\n\
                    [Added Associations]\nimage/png = krita.desktop ; ; ../evil.desktop;\n[Removed Associations]\nimage/png=gimp.desktop;\n";
        let f = parse_mimeapps(text);
        assert_eq!(
            f.default["image/png"],
            ["eog.desktop", "gimp.desktop"],
            "the first line of a key; BOM and CRLF read"
        );
        assert!(!f.default.contains_key("x"), "not a type");
        assert_eq!(f.added["image/png"], ["krita.desktop"], "spaces, empty items and paths dropped");
        assert_eq!(f.removed["image/png"], ["gimp.desktop"]);
        let cache = section_lists("[MIME Cache]\ntext/plain=gedit.desktop;vim.desktop;\n", "MIME Cache");
        assert_eq!(cache["text/plain"], ["gedit.desktop", "vim.desktop"]);
        assert!(section_lists("garbage\u{0}\n[[\n=\n", "MIME Cache").is_empty());
    }

    #[test]
    fn apps_resolve_by_the_spec() {
        let user = parse_mimeapps(
            "[Default Applications]\nimage/png=gone.desktop;eog.desktop;\n[Removed Associations]\nimage/png=shotwell.desktop;\n",
        );
        let system = parse_mimeapps(
            "[Added Associations]\nimage/png=shotwell.desktop;krita.desktop;\n[Default Applications]\ntext/plain=gedit.desktop;\n",
        );
        let cache = section_lists(
            "[MIME Cache]\nimage/png=gimp.desktop;eog.desktop;shotwell.desktop;\ntext/plain=kate.desktop;\n",
            "MIME Cache",
        );
        let legacy = section_lists("[Default Applications]\nimage/x-xcf=gimp.desktop\n", "Default Applications");
        let a = Associations { files: vec![user, system], caches: vec![cache], legacy: vec![legacy] };
        let mut installed = |id: &str| id != "gone.desktop";
        assert_eq!(
            resolve(&a, &["image/png".into()], &mut installed),
            (Some("eog.desktop".into()), vec!["eog.desktop".into(), "krita.desktop".into(), "gimp.desktop".into()]),
            "the first installed default; shotwell removed by a file above"
        );
        assert_eq!(
            resolve(&a, &["text/x-csrc".into(), "text/plain".into()], &mut installed),
            (Some("gedit.desktop".into()), vec!["gedit.desktop".into(), "kate.desktop".into()]),
            "a parent's default and apps"
        );
        assert_eq!(
            resolve(&a, &["image/x-xcf".into()], &mut installed),
            (Some("gimp.desktop".into()), vec!["gimp.desktop".into()]),
            "defaults.list last"
        );
        assert_eq!(resolve(&a, &["video/x-none".into()], &mut installed), (None, Vec::new()));
        let only_cache = Associations {
            caches: vec![section_lists("[MIME Cache]\na/b=x.desktop;y.desktop;\n", "MIME Cache")],
            ..Default::default()
        };
        assert_eq!(
            resolve(&only_cache, &["a/b".into()], &mut |_| true).0.as_deref(),
            Some("x.desktop"),
            "no default: the first app"
        );
        let same_file = Associations {
            files: vec![parse_mimeapps(
                "[Added Associations]\na/b=x.desktop;\n[Removed Associations]\na/b=x.desktop;\n",
            )],
            ..Default::default()
        };
        assert_eq!(
            resolve(&same_file, &["a/b".into()], &mut |_| true),
            (None, Vec::new()),
            "a file's removal hides its own addition"
        );
    }

    #[test]
    fn reads_the_unlocalized_comment() {
        let xml =
            "<mime-type><comment xml:lang=\"tr\">Metin</comment><comment>Plain &amp; simple</comment></mime-type>";
        assert_eq!(super::comment(xml).as_deref(), Some("Plain & simple"));
        assert_eq!(super::comment("<comment>open"), None);
    }

    /// A process-unique folder under the temp folder, removed on drop.
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_db_loads_from_a_fake_tree_the_users_first() {
        let root = Temp(std::env::temp_dir().join(format!("gezik-mime-{}", std::process::id())));
        let (home, system) = (root.0.join("home"), root.0.join("sys"));
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(home.join(".local/share/mime/globs2"), "50:application/x-mine:*.txt\n");
        write(system.join("mime/globs2"), "50:text/plain:*.txt\n50:image/png:*.png\n");
        write(system.join("mime/subclasses"), "image/png image/x-any\n");
        write(system.join("mime/generic-icons"), "image/png:image-x-generic\n");
        write(system.join("mime/icons"), "image/png:my-png\n");
        // Built by hand: a Windows temp path has a `:` that XDG_DATA_DIRS would split at.
        let x = Xdg { data_home: home.join(".local/share"), data_dirs: vec![system], ..Xdg::default() };
        let db = MimeDb::load(&x);
        assert_eq!(db.type_of_name("a.txt").as_deref(), Some("application/x-mine"));
        assert_eq!(db.with_parents("image/png"), ["image/png", "image/x-any"]);
        assert_eq!(db.icon_names("image/png"), ["my-png", "image-png", "image-x-generic"]);
        let big = root.0.join("big");
        write(big.clone(), "x");
        assert_eq!(read_small(&big, 1).as_deref(), Some("x"));
        assert_eq!(read_small(&big, 0), None, "over the limit");
        assert_eq!(read_small(&root.0, MAX_LIST_BYTES), None, "a folder");
        assert_eq!(read_small(&root.0.join("none"), MAX_LIST_BYTES), None);
    }

    fn tree(name: &str, files: &[(&str, &str)]) -> (PathBuf, Xdg) {
        let root = std::env::temp_dir().join(format!("gezik-9b10-mime-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, text) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
        }
        // Built directly: a Windows temp path has a drive colon, which `from` would split at.
        let xdg = Xdg {
            home: root.join("home"),
            data_home: root.join("home/.local/share"),
            data_dirs: vec![root.join("usr/share")],
            config_home: root.join("home/.config"),
            config_dirs: vec![root.join("etc/xdg")],
            desktops: vec!["gnome".into()],
        };
        (root, xdg)
    }

    fn app(name: &str, exec: &str, extra: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\n{extra}")
    }

    #[test]
    fn apps_come_from_the_tree_in_order() {
        let (root, xdg) = tree(
            "apps",
            &[
                ("usr/share/mime/globs2", "50:image/png:*.png\n50:text/plain:*.txt\n"),
                (
                    "usr/share/applications/mimeinfo.cache",
                    "[MIME Cache]\nimage/png=eog.desktop;gimp.desktop;kde4-paint.desktop;hidden.desktop;nodisplay.desktop;broken.desktop;big.desktop;latin.desktop;\n",
                ),
                ("usr/share/applications/eog.desktop", &app("Image Viewer", "eog %U", "MimeType=image/png;\n")),
                ("usr/share/applications/gimp.desktop", &app("GIMP", "gimp %U", "")),
                ("usr/share/applications/kde4/paint.desktop", &app("Paint", "kpaint %f", "OnlyShowIn=KDE;\n")),
                ("usr/share/applications/hidden.desktop", &app("Hidden", "hidden %f", "")),
                ("home/.local/share/applications/hidden.desktop", "[Desktop Entry]\nHidden=true\n"),
                ("usr/share/applications/nodisplay.desktop", &app("Helper", "helper %f", "NoDisplay=true\n")),
                ("usr/share/applications/broken.desktop", "[Desktop Entry]\nType=Application\nName=No command\n"),
                // No mimeinfo.cache here: the files themselves are read.
                (
                    "home/.local/share/applications/mine.desktop",
                    &app("My Viewer", "myview %f", "MimeType=image/png;\n"),
                ),
                ("home/.config/mimeapps.list", "[Default Applications]\nimage/png=gimp.desktop;\n"),
            ],
        );
        // Over MAX_ENTRY_BYTES, and not UTF-8: skipped, no panic.
        let big = app("Big", "big %f", &"X-Pad=x\n".repeat(40_000));
        std::fs::write(root.join("usr/share/applications/big.desktop"), big).unwrap();
        let mut latin = app("Latin", "latin %f", "").into_bytes();
        latin.extend_from_slice(b"Comment=\xe7\xff\n");
        std::fs::write(root.join("usr/share/applications/latin.desktop"), latin).unwrap();
        let db = MimeDb::load(&xdg);
        let assoc = associations(&xdg);
        let found = |p: &str| !p.is_empty();
        let (default, apps) = apps_of(&xdg, &db, &assoc, Path::new("/home/u/a.png"), &found);
        let id = |p: &PathBuf| super::super::desktop_entry::desktop_id_of(p).unwrap_or_default();
        assert_eq!(default.as_ref().map(id).as_deref(), Some("gimp.desktop"));
        assert_eq!(
            apps.iter().map(id).collect::<Vec<_>>(),
            ["gimp.desktop", "mine.desktop", "eog.desktop"],
            "the default first; the user's folder (no cache) before the system's; hidden, KDE-only, NoDisplay, broken, big and non-UTF-8 left out"
        );
        assert_eq!(
            apps_of(&xdg, &db, &assoc, Path::new("/home/u/README"), &found),
            (None, Vec::new()),
            "no type, no apps"
        );
        let all: Vec<String> = all_apps(&xdg, &found).into_iter().map(|(_, e)| e.name).collect();
        assert_eq!(
            all,
            ["GIMP", "Image Viewer", "My Viewer"],
            "by name; no NoDisplay, hidden, other desktops' or broken ones"
        );
        assert_eq!(app_name(&root.join("usr/share/applications/eog.desktop")), "Image Viewer");
        assert_eq!(app_name(Path::new("/nowhere/x.desktop")), "x");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn desktop_files_and_ids_stay_inside_their_folder() {
        let (root, xdg) = tree(
            "ids",
            &[
                ("usr/share/applications/a.desktop", &app("A", "a", "")),
                ("usr/share/applications/kde4/b.desktop", &app("B", "b", "")),
                ("usr/share/applications/notes.txt", "x"),
            ],
        );
        let dir = &xdg.data("applications")[1];
        let ids: Vec<String> = desktop_files(dir).into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, ["a.desktop", "kde4-b.desktop"]);
        assert!(find_desktop(&xdg, "kde4-b.desktop").is_some());
        assert!(find_desktop(&xdg, "../a.desktop").is_none());
        assert!(find_desktop(&xdg, "missing.desktop").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
