//! Icon themes (freedesktop Icon Theme spec 0.13) for Linux's system icons (spec 9 §8.5):
//! the user's theme found once, its index.theme and the themes it inherits read once, icons
//! looked up by name, context and size. PNG only: when the first icon found is not a PNG,
//! Gezik's own icon stays (decision 29; deviation 9). Compiled everywhere: tested on Windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::IconTarget;
use crate::linux::mime::{MimeDb, Xdg};

/// Sizes and scales beyond these are not real (and would overflow).
const MAX_SIZE: u32 = 4096;
const MAX_SCALE: u32 = 16;
/// The most themes in a chain.
const MAX_THEMES: usize = 16;
/// The fallback when nothing names a theme (deviation 8).
pub const FALLBACK_THEME: &str = "Adwaita";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirKind {
    Fixed,
    Scalable,
    Threshold,
}

/// One icon folder of a theme, as its index.theme describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeDir {
    pub path: String,
    pub size: u32,
    pub scale: u32,
    pub kind: DirKind,
    pub min: u32,
    pub max: u32,
    pub threshold: u32,
    /// `MimeTypes`, `Places`, … ("" when the folder does not say).
    pub context: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeIndex {
    pub inherits: Vec<String>,
    pub dirs: Vec<ThemeDir>,
}

/// A theme name that is safe as a folder name.
pub fn theme_name_like(name: &str) -> bool {
    !name.is_empty() && name.len() <= 255 && !name.starts_with('.') && !name.contains(['/', '\\', '\0'])
}

/// A folder of a theme: relative, no `.`/`..`/empty part.
fn safe_rel(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && !path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
}

/// An index.theme: `[Icon Theme]`'s `Inherits`, `Directories` and `ScaledDirectories`, and
/// each listed folder's group. Odd folders and sizes are dropped.
pub fn parse_index(text: &str) -> ThemeIndex {
    let mut groups: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            groups.entry(group.to_owned()).or_default();
            current = Some(group.to_owned());
        } else if let (Some(group), Some((key, value))) = (&current, line.split_once('='))
            && let Some(keys) = groups.get_mut(group)
        {
            keys.entry(key.trim().to_owned()).or_insert_with(|| value.trim().to_owned());
        }
    }
    let Some(head) = groups.get("Icon Theme") else { return ThemeIndex::default() };
    let list = |key: &str| -> Vec<String> {
        head.get(key)
            .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default()
    };
    let mut names = list("Directories");
    for name in list("ScaledDirectories") {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let dirs = names
        .into_iter()
        .filter(|name| safe_rel(name))
        .filter_map(|name| {
            let group = groups.get(&name)?;
            let number = |key: &str| group.get(key).and_then(|v| v.parse::<u32>().ok());
            let size = number("Size").filter(|s| (1..=MAX_SIZE).contains(s))?;
            let kind = match group.get("Type").map(String::as_str) {
                Some("Fixed") => DirKind::Fixed,
                Some("Scalable") => DirKind::Scalable,
                _ => DirKind::Threshold,
            };
            Some(ThemeDir {
                path: name,
                size,
                scale: number("Scale").unwrap_or(1).clamp(1, MAX_SCALE),
                kind,
                min: number("MinSize").unwrap_or(size).min(MAX_SIZE),
                max: number("MaxSize").unwrap_or(size).min(MAX_SIZE),
                threshold: number("Threshold").unwrap_or(2).min(MAX_SIZE),
                context: group.get("Context").cloned().unwrap_or_default(),
            })
        })
        .collect();
    let inherits = list("Inherits").into_iter().filter(|t| theme_name_like(t)).take(MAX_THEMES).collect();
    ThemeIndex { inherits, dirs }
}

/// The spec's DirectoryMatchesSize, with the scale folded into the size.
fn matches(dir: &ThemeDir, px: u32) -> bool {
    let s = dir.scale;
    match dir.kind {
        DirKind::Fixed => dir.size.saturating_mul(s) == px,
        DirKind::Scalable => dir.min.saturating_mul(s) <= px && px <= dir.max.saturating_mul(s),
        DirKind::Threshold => {
            dir.size.saturating_sub(dir.threshold).saturating_mul(s) <= px
                && px <= dir.size.saturating_add(dir.threshold).saturating_mul(s)
        }
    }
}

/// The spec's DirectorySizeDistance.
fn distance(dir: &ThemeDir, px: u32) -> u32 {
    let s = dir.scale;
    match dir.kind {
        DirKind::Fixed => dir.size.saturating_mul(s).abs_diff(px),
        DirKind::Scalable | DirKind::Threshold => {
            let (low, high) = match dir.kind {
                DirKind::Scalable => (dir.min, dir.max),
                _ => (dir.size.saturating_sub(dir.threshold), dir.size.saturating_add(dir.threshold)),
            };
            if px < low.saturating_mul(s) {
                dir.min.saturating_mul(s).saturating_sub(px)
            } else if px > high.saturating_mul(s) {
                px.saturating_sub(dir.max.saturating_mul(s))
            } else {
                0
            }
        }
    }
}

/// A file found for an icon name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    pub png: bool,
}

/// `name` in one theme (its folder under each base in `roots`), only in folders of
/// `context` (or that say none): an exact size first, else the closest (LookupIcon).
pub fn lookup(
    roots: &[PathBuf],
    index: &ThemeIndex,
    name: &str,
    context: &str,
    px: u32,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<Hit> {
    let mut closest: Option<(u32, Hit)> = None;
    for dir in &index.dirs {
        if !dir.context.is_empty() && !dir.context.eq_ignore_ascii_case(context) {
            continue;
        }
        let exact = matches(dir, px);
        let away = distance(dir, px);
        if !exact && closest.as_ref().is_some_and(|(best, _)| *best <= away) {
            continue;
        }
        for root in roots {
            for (ext, png) in [("png", true), ("svg", false)] {
                let path = root.join(&dir.path).join(format!("{name}.{ext}"));
                if !exists(&path) {
                    continue;
                }
                let hit = Hit { path, png };
                if exact {
                    return Some(hit);
                }
                if closest.as_ref().is_none_or(|(best, _)| away < *best) {
                    closest = Some((away, hit));
                }
            }
        }
    }
    closest.map(|(_, hit)| hit)
}

/// The themes to look in, in order, and the pixmaps folders.
#[derive(Debug, Default)]
pub struct Chain {
    /// Each theme's existing folders under the bases, and its index.
    pub themes: Vec<(Vec<PathBuf>, ThemeIndex)>,
    pub pixmaps: Vec<PathBuf>,
}

/// `name`, the themes it inherits depth first (each once), then hicolor; a theme whose
/// index.theme is under no base is skipped; at most 16.
pub fn load_chain(
    name: &str,
    bases: &[PathBuf],
    pixmaps: Vec<PathBuf>,
    read: &dyn Fn(&Path) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> Chain {
    let mut themes = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut stack = vec!["hicolor".to_owned(), name.to_owned()];
    while let Some(theme) = stack.pop() {
        // hicolor is at the bottom of the stack: it always gets in, even past the limit.
        if seen.contains(&theme) || !theme_name_like(&theme) || (seen.len() >= MAX_THEMES && theme != "hicolor") {
            continue;
        }
        seen.push(theme.clone());
        let roots: Vec<PathBuf> = bases.iter().map(|base| base.join(&theme)).filter(|root| exists(root)).collect();
        let Some(text) = roots.iter().find_map(|root| read(&root.join("index.theme"))) else { continue };
        let index = parse_index(&text);
        // hicolor waits at the bottom of the stack: listed early, it still comes last.
        stack.extend(index.inherits.iter().rev().filter(|t| *t != "hicolor").cloned());
        themes.push((roots, index));
    }
    Chain { themes, pixmaps }
}

/// The first of `names` (name, context) any theme of `chain` has, each name through the
/// whole chain; then the pixmaps folders. A PNG's path, or `None` when the first found is
/// not a PNG (deviation 9).
pub fn find(chain: &Chain, names: &[(String, &str)], px: u32, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    for (name, context) in names {
        for (roots, index) in &chain.themes {
            if let Some(hit) = lookup(roots, index, name, context, px, exists) {
                return hit.png.then_some(hit.path);
            }
        }
    }
    names
        .iter()
        .find_map(|(name, _)| chain.pixmaps.iter().map(|dir| dir.join(format!("{name}.png"))).find(|p| exists(p)))
}

/// Where themes are: `~/.icons`, `$XDG_DATA_HOME/icons`, each `$XDG_DATA_DIRS/icons`.
pub fn icon_bases(xdg: &Xdg) -> Vec<PathBuf> {
    std::iter::once(xdg.home.join(".icons")).chain(xdg.data("icons")).collect()
}

/// Where the user's icon theme is set, by desktop (deviation 8).
#[derive(Debug, PartialEq, Eq)]
pub enum ThemeSource {
    Kde,
    Gsettings(&'static str),
}

pub fn source_for(desktops: &[String]) -> ThemeSource {
    let has = |name: &str| desktops.iter().any(|d| d == name);
    if has("kde") {
        ThemeSource::Kde
    } else if has("x-cinnamon") || has("cinnamon") {
        ThemeSource::Gsettings("org.cinnamon.desktop.interface")
    } else if has("mate") {
        ThemeSource::Gsettings("org.mate.interface")
    } else {
        ThemeSource::Gsettings("org.gnome.desktop.interface")
    }
}

/// `gsettings get … icon-theme`'s answer (`'Adwaita'`).
pub fn from_gsettings(out: &str) -> Option<String> {
    let text = out.trim();
    let text = text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')).unwrap_or(text);
    Some(text.to_owned()).filter(|t| theme_name_like(t))
}

/// kdeglobals' `[Icons] Theme`, else KDE's default.
pub fn from_kdeglobals(text: Option<&str>) -> String {
    text.and_then(|t| ini_value(t, "Icons", "Theme"))
        .filter(|t| theme_name_like(t))
        .unwrap_or_else(|| "breeze".to_owned())
}

/// GTK's settings.ini `[Settings] gtk-icon-theme-name`.
pub fn from_gtk_settings(text: &str) -> Option<String> {
    ini_value(text, "Settings", "gtk-icon-theme-name").filter(|t| theme_name_like(t))
}

fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name == section;
        } else if inside
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().trim_matches('"').to_owned());
        }
    }
    None
}

/// Home and the XDG user folders with their icon names (deviation 15); `others` in the order
/// desktop, documents, download, music, pictures, public, templates, videos.
pub fn special_folders(home: Option<PathBuf>, others: [Option<PathBuf>; 8]) -> Vec<(PathBuf, &'static str)> {
    const NAMES: [&str; 8] = [
        "user-desktop",
        "folder-documents",
        "folder-download",
        "folder-music",
        "folder-pictures",
        "folder-publicshare",
        "folder-templates",
        "folder-videos",
    ];
    let mut out: Vec<(PathBuf, &'static str)> = home.into_iter().map(|h| (h, "user-home")).collect();
    for (path, name) in others.into_iter().zip(NAMES) {
        if let Some(path) = path
            && !out.iter().any(|(p, _)| *p == path)
        {
            out.push((path, name));
        }
    }
    out
}

/// The icon names (with their contexts) for `target` (deviation 15).
pub fn names_for(
    target: &IconTarget,
    db: &MimeDb,
    specials: &[(PathBuf, &'static str)],
) -> Vec<(String, &'static str)> {
    let folder = ("folder".to_owned(), "Places");
    match target {
        IconTarget::Folder => vec![folder],
        IconTarget::Extension(ext) => {
            let mut names = match db.type_of_name(&format!("x.{ext}")) {
                Some(mime) => db.icon_names(&mime),
                // Unknown: a document of no known type.
                None => vec!["text-x-generic".to_owned()],
            };
            if !names.iter().any(|n| n == "application-x-generic") {
                names.push("application-x-generic".to_owned());
            }
            names.into_iter().map(|n| (n, "MimeTypes")).collect()
        }
        IconTarget::Path(path) => {
            if let Some((_, name)) = specials.iter().find(|(p, _)| p == path) {
                return vec![((*name).to_owned(), "Places"), folder];
            }
            let text = path.to_string_lossy().replace('\\', "/");
            let (name, context) = if text.contains("/gvfs/") {
                ("folder-remote", "Places")
            } else if text.starts_with("/media/") || text.starts_with("/run/media/") {
                ("drive-removable-media", "Devices")
            } else {
                ("drive-harddisk", "Devices")
            };
            vec![(name.to_owned(), context), folder]
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod live {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use super::*;
    use crate::Rgba;
    use crate::linux::mime;

    /// How long `gsettings` may take to name the theme.
    const GSETTINGS_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

    /// The user's theme name (deviation 8), asked once.
    fn user_theme(xdg: &Xdg) -> String {
        let read = |p: PathBuf| mime::read_small(&p, mime::MAX_LIST_BYTES);
        let named = match source_for(&xdg.desktops) {
            ThemeSource::Kde => Some(from_kdeglobals(read(xdg.config_home.join("kdeglobals")).as_deref())),
            // Asked inside the chain's OnceLock: a hung session bus must not stall every icon
            // thread for long. Past the limit, settings.ini and then Adwaita decide.
            ThemeSource::Gsettings(schema) => {
                crate::process::run_until(&["gsettings", "get", schema, "icon-theme"], "", GSETTINGS_LIMIT)
                    .ok()
                    .filter(|ran| ran.ok)
                    .and_then(|ran| from_gsettings(&ran.stdout))
            }
        };
        named
            .or_else(|| {
                ["gtk-3.0", "gtk-4.0"].iter().find_map(|gtk| {
                    read(xdg.config_home.join(gtk).join("settings.ini")).and_then(|t| from_gtk_settings(&t))
                })
            })
            .unwrap_or_else(|| FALLBACK_THEME.to_owned())
    }

    /// The theme chain, built on first use and kept (shortcut: a theme changed while Gezik runs
    /// shows after a restart).
    fn chain() -> Option<&'static Chain> {
        static CHAIN: OnceLock<Option<Chain>> = OnceLock::new();
        CHAIN
            .get_or_init(|| {
                let xdg = mime::xdg()?;
                let pixmaps = xdg.data_dirs.iter().map(|d| d.join("pixmaps")).collect();
                Some(load_chain(
                    &user_theme(xdg),
                    &icon_bases(xdg),
                    pixmaps,
                    &|p: &Path| mime::read_small(p, mime::MAX_LIST_BYTES),
                    &|p: &Path| p.is_dir(),
                ))
            })
            .as_ref()
    }

    fn specials() -> &'static [(PathBuf, &'static str)] {
        static SPECIALS: OnceLock<Vec<(PathBuf, &'static str)>> = OnceLock::new();
        SPECIALS.get_or_init(|| {
            special_folders(
                dirs::home_dir(),
                [
                    dirs::desktop_dir(),
                    dirs::document_dir(),
                    dirs::download_dir(),
                    dirs::audio_dir(),
                    dirs::picture_dir(),
                    dirs::public_dir(),
                    dirs::template_dir(),
                    dirs::video_dir(),
                ],
            )
        })
    }

    /// Whether `path` is home or an XDG user folder (it has a theme icon of its own).
    pub fn is_special(path: &Path) -> bool {
        specials().iter().any(|(p, _)| p == path)
    }

    /// The theme's PNG icon for `target`, at most `px`; `None` keeps Gezik's own icon.
    pub fn icon(target: &crate::IconTarget, px: u32) -> Option<Rgba> {
        let names = names_for(target, mime::db()?, specials());
        let path = find(chain()?, &names, px, &|p: &Path| p.is_file())?;
        crate::picture::decode_image(&path, px).ok().map(|d| d.image)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use live::{icon, is_special};

#[cfg(test)]
mod tests {
    use super::*;

    const ADWAITA: &str = "[Icon Theme]\nName=Adwaita\nInherits=hicolor\nDirectories=16x16/mimetypes,48x48/mimetypes,scalable/mimetypes,16x16/places,../evil,/abs,96x96@2/mimetypes,huge\n\
        [16x16/mimetypes]\nSize=16\nContext=MimeTypes\nType=Fixed\n\
        [48x48/mimetypes]\nSize=48\nContext=MimeTypes\n\
        [scalable/mimetypes]\nSize=128\nMinSize=8\nMaxSize=512\nType=Scalable\nContext=MimeTypes\n\
        [16x16/places]\nSize=16\nContext=Places\nType=Fixed\n\
        [../evil]\nSize=16\n[/abs]\nSize=16\n\
        [96x96@2/mimetypes]\nSize=48\nScale=2\nType=Fixed\nContext=MimeTypes\n\
        [huge]\nSize=99999\n";

    fn set(paths: &[&str]) -> impl Fn(&Path) -> bool {
        let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        move |p: &Path| paths.iter().any(|q| *q == p.to_string_lossy().replace('\\', "/"))
    }

    #[test]
    fn index_theme_parses_and_refuses_odd_dirs() {
        let index = parse_index(ADWAITA);
        assert_eq!(index.inherits, ["hicolor"]);
        let names: Vec<&str> = index.dirs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(
            names,
            ["16x16/mimetypes", "48x48/mimetypes", "scalable/mimetypes", "16x16/places", "96x96@2/mimetypes"]
        );
        let threshold = &index.dirs[1];
        assert_eq!((threshold.kind, threshold.threshold, threshold.scale), (DirKind::Threshold, 2, 1));
        assert_eq!(index.dirs[4].scale, 2);
        assert_eq!(parse_index("[Icon Theme]\nInherits=../x,ok,a/b\n").inherits, ["ok"]);
        assert_eq!(parse_index("garbage"), ThemeIndex::default());
        let zero = parse_index("[Icon Theme]\nDirectories=d\n[d]\nSize=16\nScale=0\n");
        assert_eq!(zero.dirs[0].scale, 1, "a zero scale is one");
    }

    #[test]
    fn sizes_match_exactly_then_closest() {
        let index = parse_index(ADWAITA);
        let roots = [PathBuf::from("/usr/share/icons/Adwaita")];
        let exists = set(&[
            "/usr/share/icons/Adwaita/16x16/mimetypes/text-plain.png",
            "/usr/share/icons/Adwaita/48x48/mimetypes/text-plain.png",
            "/usr/share/icons/Adwaita/96x96@2/mimetypes/image-png.png",
            "/usr/share/icons/Adwaita/16x16/places/folder.png",
        ]);
        let at = |name: &str, ctx: &str, px: u32| {
            lookup(&roots, &index, name, ctx, px, &exists).map(|h| h.path.to_string_lossy().replace('\\', "/"))
        };
        assert_eq!(
            at("text-plain", "MimeTypes", 16).as_deref(),
            Some("/usr/share/icons/Adwaita/16x16/mimetypes/text-plain.png")
        );
        assert_eq!(
            at("text-plain", "MimeTypes", 50).as_deref(),
            Some("/usr/share/icons/Adwaita/48x48/mimetypes/text-plain.png"),
            "within the threshold"
        );
        assert_eq!(
            at("text-plain", "MimeTypes", 40).as_deref(),
            Some("/usr/share/icons/Adwaita/48x48/mimetypes/text-plain.png"),
            "the closest"
        );
        assert_eq!(
            at("text-plain", "MimeTypes", 32).as_deref(),
            Some("/usr/share/icons/Adwaita/16x16/mimetypes/text-plain.png"),
            "a tie: the first folder"
        );
        assert_eq!(
            at("image-png", "MimeTypes", 96).as_deref(),
            Some("/usr/share/icons/Adwaita/96x96@2/mimetypes/image-png.png"),
            "size times scale"
        );
        assert_eq!(at("folder", "MimeTypes", 16), None, "another context's folder is not looked in");
        assert_eq!(at("folder", "Places", 16).as_deref(), Some("/usr/share/icons/Adwaita/16x16/places/folder.png"));
        let svg = set(&["/usr/share/icons/Adwaita/scalable/mimetypes/text-plain.svg"]);
        assert_eq!(lookup(&roots, &index, "text-plain", "MimeTypes", 64, &svg).map(|h| h.png), Some(false));
    }

    #[test]
    fn the_chain_follows_inherits_once_and_ends_with_hicolor() {
        let texts: HashMap<&str, &str> = [
            ("/h/.icons/Mine/index.theme", "[Icon Theme]\nInherits=Papirus,Adwaita\n"),
            ("/usr/share/icons/Papirus/index.theme", "[Icon Theme]\nInherits=Mine,breeze\n"),
            ("/usr/share/icons/Adwaita/index.theme", "[Icon Theme]\nInherits=hicolor\n"),
            ("/usr/share/icons/hicolor/index.theme", "[Icon Theme]\n"),
            ("/usr/share/icons/Early/index.theme", "[Icon Theme]\nInherits=hicolor,Adwaita\n"),
        ]
        .into_iter()
        .collect();
        let read = |p: &Path| texts.get(p.to_string_lossy().replace('\\', "/").as_str()).map(|t| (*t).to_owned());
        let exists = |p: &Path| {
            let p = p.to_string_lossy().replace('\\', "/");
            [
                "/h/.icons/Mine",
                "/usr/share/icons/Papirus",
                "/usr/share/icons/Adwaita",
                "/usr/share/icons/hicolor",
                "/usr/share/icons/Early",
            ]
            .contains(&p.as_str())
        };
        let bases = [PathBuf::from("/h/.icons"), PathBuf::from("/usr/share/icons")];
        let chain = load_chain("Mine", &bases, Vec::new(), &read, &exists);
        let roots: Vec<String> = chain.themes.iter().map(|(r, _)| r[0].to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(
            roots,
            ["/h/.icons/Mine", "/usr/share/icons/Papirus", "/usr/share/icons/Adwaita", "/usr/share/icons/hicolor"],
            "breeze has no index: skipped; Mine only once"
        );
        assert_eq!(
            load_chain("../etc", &bases, Vec::new(), &read, &exists).themes.len(),
            1,
            "a bad name: only hicolor"
        );
        let early: Vec<String> = load_chain("Early", &bases, Vec::new(), &read, &exists)
            .themes
            .iter()
            .map(|(r, _)| r[0].to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            early,
            ["/usr/share/icons/Early", "/usr/share/icons/Adwaita", "/usr/share/icons/hicolor"],
            "hicolor listed first still comes last"
        );
    }

    #[test]
    fn the_first_hit_decides_png_or_own_icon() {
        let svg_theme = parse_index(
            "[Icon Theme]\nDirectories=s\n[s]\nSize=64\nType=Scalable\nMinSize=8\nMaxSize=512\nContext=MimeTypes\n",
        );
        let hicolor = parse_index("[Icon Theme]\nDirectories=48\n[48]\nSize=48\nContext=MimeTypes\n");
        let chain = Chain {
            themes: vec![(vec![PathBuf::from("/t")], svg_theme), (vec![PathBuf::from("/hc")], hicolor)],
            pixmaps: vec![PathBuf::from("/px")],
        };
        let exists =
            set(&["/t/s/text-plain.svg", "/hc/48/text-plain.png", "/hc/48/text-x-generic.png", "/px/gimp.png"]);
        let names = |list: &[&str]| list.iter().map(|n| ((*n).to_owned(), "MimeTypes")).collect::<Vec<_>>();
        assert_eq!(
            find(&chain, &names(&["text-plain", "text-x-generic"]), 48, &exists),
            None,
            "an SVG found first: Gezik's icon, not hicolor's PNG"
        );
        assert_eq!(
            find(&chain, &names(&["image-png", "text-x-generic"]), 48, &exists)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .as_deref(),
            Some("/hc/48/text-x-generic.png"),
            "the next name through the whole chain"
        );
        assert_eq!(
            find(&chain, &names(&["gimp"]), 48, &exists).map(|p| p.to_string_lossy().replace('\\', "/")).as_deref(),
            Some("/px/gimp.png")
        );
        assert_eq!(find(&chain, &names(&["nothing"]), 48, &exists), None);
    }

    #[test]
    fn the_theme_name_comes_from_the_desktop() {
        assert_eq!(source_for(&["kde".into()]), ThemeSource::Kde);
        assert_eq!(source_for(&["x-cinnamon".into()]), ThemeSource::Gsettings("org.cinnamon.desktop.interface"));
        assert_eq!(source_for(&["mate".into()]), ThemeSource::Gsettings("org.mate.interface"));
        assert_eq!(
            source_for(&["ubuntu".into(), "gnome".into()]),
            ThemeSource::Gsettings("org.gnome.desktop.interface")
        );
        assert_eq!(source_for(&[]), ThemeSource::Gsettings("org.gnome.desktop.interface"));
        assert_eq!(from_gsettings("'Yaru-dark'\n").as_deref(), Some("Yaru-dark"));
        assert_eq!(from_gsettings("'../../etc'\n"), None);
        assert_eq!(from_gsettings(""), None);
        assert_eq!(from_kdeglobals(Some("[General]\nTheme=x\n[Icons]\nTheme=Papirus-Dark\n")), "Papirus-Dark");
        assert_eq!(from_kdeglobals(Some("[Icons]\n")), "breeze");
        assert_eq!(from_kdeglobals(None), "breeze");
        assert_eq!(from_gtk_settings("[Settings]\ngtk-icon-theme-name = Tela\n").as_deref(), Some("Tela"));
        assert!(
            theme_name_like("Adwaita")
                && !theme_name_like(".hidden")
                && !theme_name_like("a/b")
                && !theme_name_like("")
        );
    }

    #[test]
    fn names_for_types_folders_and_drives() {
        let mut db = MimeDb::default();
        db.add_globs("50:text/plain:*.txt\n");
        let specials = special_folders(
            Some("/home/u".into()),
            [Some("/home/u/Masaüstü".into()), Some("/home/u/Belgeler".into()), None, None, None, None, None, None],
        );
        let names = |t: IconTarget| {
            names_for(&t, &db, &specials).into_iter().map(|(n, c)| format!("{c}:{n}")).collect::<Vec<_>>()
        };
        assert_eq!(
            names(IconTarget::Extension("txt".into())),
            ["MimeTypes:text-plain", "MimeTypes:text-x-generic", "MimeTypes:application-x-generic"]
        );
        assert_eq!(
            names(IconTarget::Extension("".into())),
            ["MimeTypes:text-x-generic", "MimeTypes:application-x-generic"]
        );
        assert_eq!(names(IconTarget::Folder), ["Places:folder"]);
        assert_eq!(names(IconTarget::Path("/home/u".into())), ["Places:user-home", "Places:folder"]);
        assert_eq!(names(IconTarget::Path("/home/u/Belgeler".into())), ["Places:folder-documents", "Places:folder"]);
        assert_eq!(names(IconTarget::Path("/".into())), ["Devices:drive-harddisk", "Places:folder"]);
        assert_eq!(
            names(IconTarget::Path("/run/media/u/USB".into())),
            ["Devices:drive-removable-media", "Places:folder"]
        );
        assert_eq!(
            names(IconTarget::Path("/run/user/1000/gvfs/smb-share:server=nas,share=x".into())),
            ["Places:folder-remote", "Places:folder"]
        );
        assert_eq!(names(IconTarget::Path("/mnt/data".into())), ["Devices:drive-harddisk", "Places:folder"]);
    }

    #[test]
    fn a_real_tree_in_a_temp_folder() {
        use crate::linux::mime;
        let root = std::env::temp_dir().join(format!("gezik-icon-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        // Built by hand: `Xdg::from` splits on ':', which a Windows temp path has.
        let xdg = Xdg {
            home: home.clone(),
            data_home: home.join(".local/share"),
            data_dirs: vec![root.join("share")],
            ..Xdg::default()
        };
        let write = |rel: &Path, text: &str| {
            std::fs::create_dir_all(rel.parent().unwrap()).unwrap();
            std::fs::write(rel, text).unwrap();
        };
        let bases = icon_bases(&xdg);
        assert_eq!(bases[0], home.join(".icons"));
        // A loop of 20 themes each inheriting the next (the last the first): the chain stops
        // at 16 and still ends with hicolor.
        for i in 0..20 {
            write(
                &bases[0].join(format!("t{i}/index.theme")),
                &format!(
                    "[Icon Theme]
Inherits=t{}
",
                    (i + 1) % 20
                ),
            );
        }
        let shared = root.join("share/icons");
        write(
            &shared.join("hicolor/index.theme"),
            "[Icon Theme]
Directories=48x48/apps
[48x48/apps]
Size=48
Context=Applications
",
        );
        write(&shared.join("hicolor/48x48/apps/gimp.png"), "png");
        let read = |p: &Path| mime::read_small(p, mime::MAX_LIST_BYTES);
        let chain = load_chain("t0", &bases, vec![root.join("share/pixmaps")], &read, &|p: &Path| p.is_dir());
        assert_eq!(chain.themes.len(), MAX_THEMES + 1);
        assert_eq!(chain.themes.last().map(|(r, _)| r[0].clone()), Some(shared.join("hicolor")));
        let found = find(&chain, &[("gimp".to_owned(), "Applications")], 48, &|p: &Path| p.is_file());
        assert_eq!(found, Some(shared.join("hicolor/48x48/apps/gimp.png")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
