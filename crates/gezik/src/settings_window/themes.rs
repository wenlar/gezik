//! Themes from GitHub (spec 14 §6), rows of the Appearance page after its fields: Get Theme (an
//! address and Enter), each theme a download found (point at Install to preview it, choose it to
//! install), and each theme installed from an address (Use Now, Check for Updates, Open Source
//! Page, Delete Theme). The download and the zip are read on a thread, in memory; only the
//! themes chosen are written, as `themes/<id>.toml` with their source on the first line.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_config::theme::BUILTIN_THEMES;
use gezik_config::theme_source::{self as source, Candidate, Fetch};
use slint::{ModelRc, SharedString, VecModel};

use crate::SettingItem;

pub(super) const USE_NOW: &str = "Use Now";
pub(super) const CHECK: &str = "Check for Updates";
pub(super) const SOURCE_PAGE: &str = "Open Source Page";
pub(super) const DELETE: &str = "Delete Theme";
pub(super) const INSTALL: &str = "Install";
pub(super) const REPLACE: &str = "Replace";
pub(super) const KEEP_BOTH: &str = "Keep Both";

/// Counts the downloads: one goes on only while it is the last asked for (a new address, or the
/// window closing, stops it).
static TURN: AtomicU64 = AtomicU64::new(0);

/// A theme row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Row {
    Get,
    Found(usize),
    Installed(usize),
}

/// A theme installed from an address.
pub(super) struct Installed {
    pub id: String,
    pub name: String,
    pub address: String,
    pub entry: String,
}

#[derive(Default)]
pub(super) struct Themes {
    /// The Get Theme row's address (the last one downloaded).
    pub address: String,
    pub found: Vec<Candidate>,
    pub installed: Vec<Installed>,
    /// The user themes' texts by id: what a theme found is compared with.
    pub files: HashMap<String, String>,
}

/// What installing a theme found would do.
#[derive(Debug, PartialEq, Eq)]
enum State {
    New,
    Same,
    Differs,
}

impl Themes {
    pub fn read(&mut self, themes: &HashMap<String, String>) {
        self.files.clone_from(themes);
        self.installed = themes
            .iter()
            .filter_map(|(id, text)| {
                let (address, entry) = source::source_of(text)?;
                let name = source::candidate(entry, Some(source::body(text).as_bytes().to_vec())).name;
                Some(Installed { id: id.clone(), name, address: address.to_owned(), entry: entry.to_owned() })
            })
            .collect();
        self.installed.sort_by(|a, b| a.id.cmp(&b.id));
    }

    /// The rows a search for `query` shows (all without one).
    pub fn rows(&self, query: &str) -> Vec<Row> {
        let all = [Row::Get]
            .into_iter()
            .chain((0..self.found.len()).map(Row::Found))
            .chain((0..self.installed.len()).map(Row::Installed));
        let query = query.to_lowercase();
        all.filter(|row| {
            let item = self.item(*row);
            [&item.title, &item.key, &item.description].iter().any(|t| t.to_lowercase().contains(&query))
        })
        .collect()
    }

    /// The file a theme found goes to, and how it compares with what is there: the theme
    /// installed before from the same address and entry, else the file of its name.
    fn target(&self, found: &Candidate) -> (String, State) {
        let id = self
            .installed
            .iter()
            .find(|t| t.address == self.address && t.entry == found.entry)
            .map_or_else(|| found.id.clone(), |t| t.id.clone());
        let state = match self.files.get(&id) {
            None => State::New,
            Some(text) if source::body(text) == found.text => State::Same,
            Some(_) => State::Differs,
        };
        (id, state)
    }

    fn taken(&self, id: &str) -> bool {
        self.files.contains_key(id) || BUILTIN_THEMES.contains(&id)
    }

    pub fn item(&self, row: Row) -> SettingItem {
        let (kind, title, key, value, description, options): (i32, String, String, String, String, &[&str]) = match row
        {
            Row::Get => (
                2,
                "Get Theme".to_owned(),
                "install-theme".to_owned(),
                self.address.clone(),
                "Paste a GitHub repository (github.com/owner/repo, …/tree/<branch>/<folder>, …/blob/…/x.toml) \
                 or an https address of a .toml file and press Enter. A theme is data only: colors, sizes and \
                 a font's name."
                    .to_owned(),
                &[],
            ),
            Row::Found(i) => {
                let found = &self.found[i];
                let skipped = match found.skipped {
                    0 => String::new(),
                    n => format!(" {n} of its values are wrong and are skipped."),
                };
                let (id, state) = self.target(found);
                match (&found.problem, state) {
                    (Some(problem), _) => {
                        (6, found.name.clone(), found.entry.clone(), "Cannot install".into(), problem.clone(), &[])
                    }
                    (None, State::Same) => (
                        6,
                        found.name.clone(),
                        found.entry.clone(),
                        "Installed".into(),
                        format!("The same as themes/{id}.toml"),
                        &[],
                    ),
                    (None, State::New) => (
                        1,
                        found.name.clone(),
                        found.entry.clone(),
                        "Not installed".into(),
                        format!("Installs as themes/{id}.toml; point at Install to preview it.{skipped}"),
                        &[INSTALL],
                    ),
                    (None, State::Differs) => (
                        1,
                        found.name.clone(),
                        found.entry.clone(),
                        "Differs".into(),
                        format!(
                            "themes/{id}.toml is there and differs: Replace writes over it, Keep Both installs \
                             this as themes/{}.toml; point at either to preview it.{skipped}",
                            source::free_id(&found.id, |id| self.taken(id))
                        ),
                        &[REPLACE, KEEP_BOTH],
                    ),
                }
            }
            Row::Installed(i) => {
                let theme = &self.installed[i];
                (
                    1,
                    theme.name.clone(),
                    format!("themes/{}.toml", theme.id),
                    "Installed".into(),
                    format!("From {} ({})", theme.address, theme.entry),
                    &[USE_NOW, CHECK, SOURCE_PAGE, DELETE],
                )
            }
        };
        let options: Vec<SharedString> = options.iter().map(|o| SharedString::from(*o)).collect();
        SettingItem {
            kind,
            title: title.into(),
            key: key.into(),
            description: description.into(),
            value: value.into(),
            is_default: false,
            default: SharedString::new(),
            options: ModelRc::new(VecModel::from(options)),
        }
    }

    /// The file theme `i` found goes to (`keep_both`: a free name) and its text.
    pub fn install(&self, i: usize, keep_both: bool, date: &str) -> Option<(String, String)> {
        let found = self.found.get(i).filter(|f| f.problem.is_none())?;
        let id = match keep_both {
            true => source::free_id(&found.id, |id| self.taken(id)),
            false => self.target(found).0,
        };
        Some((id, source::with_source(&found.text, &self.address, &found.entry, date)))
    }

    /// The theme `option` previews on row `row`: its id and, for one found, its text.
    pub fn preview(&self, row: Row) -> Option<(String, Option<String>)> {
        match row {
            Row::Get => None,
            Row::Found(i) => {
                let found = self.found.get(i).filter(|f| f.problem.is_none())?;
                Some((found.id.clone(), Some(found.text.clone())))
            }
            Row::Installed(i) => Some((self.installed.get(i)?.id.clone(), None)),
        }
    }
}

/// The page an installed theme's address shows in a browser, when it is one Gezik downloads from.
pub(super) fn source_page(address: &str) -> Option<String> {
    source::resolve(address).ok()?;
    let address = address.trim();
    Some(if address.contains("://") { address.to_owned() } else { format!("https://{address}") })
}

/// Theme `id`'s file in `dir`: the one the store reads it from (`Nord.toml` for `nord`, which
/// matters where names are case-sensitive), else `<id>.toml`. Reads the folder: not on the UI
/// thread.
pub(super) fn theme_path(dir: &std::path::Path, id: &str) -> std::path::PathBuf {
    let found = std::fs::read_dir(dir).ok().and_then(|entries| {
        entries.filter_map(Result::ok).map(|e| e.path()).find(|path| {
            path.extension().and_then(|e| e.to_str()) == Some("toml")
                && path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.to_lowercase() == id)
        })
    });
    found.unwrap_or_else(|| dir.join(format!("{id}.toml")))
}

/// Stops the download under way, if any.
pub(super) fn stop() {
    TURN.fetch_add(1, Ordering::SeqCst);
}

/// Downloads `fetch` on a thread and finds its themes; `progress(bytes)` and `done` are called
/// on the UI thread, and only while this is the last download asked for.
pub(super) fn start(fetch: Fetch, progress: fn(u64), done: fn(Result<Vec<Candidate>, String>)) {
    let turn = TURN.fetch_add(1, Ordering::SeqCst) + 1;
    let current = move || TURN.load(Ordering::SeqCst) == turn;
    let spawned = std::thread::Builder::new().name("theme download".into()).spawn(move || {
        let mut told = |bytes| {
            let _ = slint::invoke_from_event_loop(move || {
                if current() {
                    progress(bytes);
                }
            });
        };
        let result = download(&fetch, turn, &mut told, &|| !current());
        let _ = slint::invoke_from_event_loop(move || {
            if current() {
                done(result);
            }
        });
    });
    if spawned.is_err() {
        done(Err("Cannot start the download".to_owned()));
    }
}

/// The download's themes (on the download's thread).
fn download(
    fetch: &Fetch,
    turn: u64,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Candidate>, String> {
    // A folder of its own, made new (another user's planted one makes this fail), private on Unix.
    let folder = std::env::temp_dir().join(format!("gezik-theme-{}-{turn}", std::process::id()));
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let temp = folder.join("download");
    let downloaded = builder
        .create(&folder)
        .and_then(|()| gezik_platform::http::download(&fetch.url, &temp, source::MAX_DOWNLOAD, false, progress, stop));
    let bytes = downloaded.and_then(|()| std::fs::read(&temp));
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_dir(&folder);
    let bytes = bytes.map_err(|err| download_error(fetch, &err))?;
    candidates(fetch, &bytes)
}

/// The themes in a download's bytes.
fn candidates(fetch: &Fetch, bytes: &[u8]) -> Result<Vec<Candidate>, String> {
    if !fetch.zip {
        let name = fetch.url.rsplit('/').next().unwrap_or_default();
        let fits = bytes.len() as u64 <= source::MAX_THEME;
        return Ok(vec![source::candidate(name, fits.then(|| bytes.to_vec()))]);
    }
    let wanted = |name: &str| source::theme_entry(&fetch.folder, name).is_some();
    let entries = gezik_batch::archive::read_zip_entries(bytes, &wanted, source::MAX_THEME, source::MAX_THEMES)
        .map_err(|err| format!("The download is not a zip: {err}"))?;
    let found: Vec<Candidate> = entries
        .into_iter()
        .filter_map(|(name, data)| Some(source::candidate(source::theme_entry(&fetch.folder, &name)?, data)))
        .collect();
    match found.is_empty() {
        true => Err(format!("No .toml file in {}the repository or its themes folder", fetch.folder)),
        false => Ok(found),
    }
}

/// A failed download in words.
fn download_error(fetch: &Fetch, err: &std::io::Error) -> String {
    let text = gezik_platform::fs::describe(err);
    let hint = match err.kind() {
        std::io::ErrorKind::FileTooLarge => " (more than 5 MB: paste the address of a theme's .toml file instead)",
        _ if fetch.url.starts_with("https://api.github.com/") && (text.contains("403") || text.contains("429")) => {
            " (GitHub allows 60 downloads an hour without signing in: try again later, or paste the address of a \
             theme's .toml file)"
        }
        _ => "",
    };
    let missing = gezik_platform::http::tool_missing_hint().map(|h| format!(" — {h}")).unwrap_or_default();
    format!("Cannot download {}: {text}{hint}{missing}", fetch.url)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn repo_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const NORD: &str = "name = \"Nord\"\n[colors]\naccent = \"#88c0d0\"\n";

    fn fetched(address: &str, files: &[(&str, &str)], installed: &[(&str, &str)]) -> Themes {
        let fetch = source::resolve(address).unwrap();
        let mut themes = Themes { address: address.to_owned(), ..Themes::default() };
        themes.found = candidates(&fetch, &repo_zip(files)).unwrap();
        themes.read(&installed.iter().map(|(id, t)| (id.to_string(), t.to_string())).collect());
        themes
    }

    #[test]
    fn a_repository_zip_lists_its_themes_without_writing_anything() {
        let big = format!("[colors]\n# {}\n", "x".repeat(70 << 10));
        let themes = fetched(
            "https://github.com/arc/nord",
            &[
                ("arc-nord-1a2/nord.toml", NORD),
                ("arc-nord-1a2/themes/big.toml", &big),
                ("arc-nord-1a2/Cargo.toml", "[package]\nname = \"x\"\n"),
                ("arc-nord-1a2/docs/deep.toml", NORD),
                ("arc-nord-1a2/../../evil.toml", NORD),
            ],
            &[],
        );
        let rows: Vec<(String, String)> = themes
            .rows("")
            .into_iter()
            .map(|r| themes.item(r))
            .map(|i| (i.key.to_string(), i.value.to_string()))
            .collect();
        assert_eq!(
            rows,
            [
                ("install-theme", "https://github.com/arc/nord"),
                ("nord.toml", "Not installed"),
                ("themes/big.toml", "Cannot install"),
                ("Cargo.toml", "Cannot install"),
            ]
            .map(|(a, b)| (a.to_owned(), b.to_owned())),
            "deeper folders and names outside are not looked at"
        );
        assert!(themes.item(Row::Found(1)).description.contains("64 KB"));
        let (id, text) = themes.install(0, false, "2026-10-12").unwrap();
        assert_eq!(id, "nord");
        assert_eq!(text, format!("# Installed from https://github.com/arc/nord (nord.toml) on 2026-10-12\n{NORD}"));
        assert_eq!(themes.install(1, false, "2026-10-12"), None, "a file with a problem is not installed");
        assert_eq!(themes.preview(Row::Found(0)), Some(("nord".to_owned(), Some(NORD.to_owned()))));
    }

    #[test]
    fn an_installed_theme_is_compared_and_kept_both() {
        let address = "https://github.com/arc/nord";
        let installed = source::with_source(NORD, address, "nord.toml", "2026-10-01");
        let same = fetched(address, &[("r/nord.toml", NORD)], &[("nord", &installed)]);
        assert_eq!(same.item(Row::Found(0)).value, "Installed");
        assert_eq!(same.item(Row::Installed(0)).key, "themes/nord.toml");
        assert_eq!(same.item(Row::Installed(0)).title, "Nord");

        let newer = "name = \"Nord\"\n[colors]\naccent = \"#5e81ac\"\n";
        let update = fetched(address, &[("r/nord.toml", newer)], &[("nord", &installed), ("nord-2", "")]);
        assert_eq!(update.item(Row::Found(0)).value, "Differs");
        assert_eq!(update.install(0, false, "d").unwrap().0, "nord", "Replace: the same file");
        assert_eq!(update.install(0, true, "d").unwrap().0, "nord-3", "Keep Both: a free name");

        // Installed by hand as `frost`: an update of it goes there, not to `nord`.
        let moved = fetched(address, &[("r/nord.toml", newer)], &[("frost", &installed)]);
        assert_eq!(moved.install(0, false, "d").unwrap().0, "frost");
        let elsewhere = fetched("https://github.com/other/nord", &[("r/nord.toml", newer)], &[("frost", &installed)]);
        assert_eq!(elsewhere.install(0, false, "d").unwrap().0, "nord");
    }

    #[test]
    fn a_search_finds_theme_rows() {
        let themes = fetched("https://github.com/arc/nord", &[("r/nord.toml", NORD)], &[]);
        assert_eq!(themes.rows("github"), [Row::Get]);
        assert_eq!(themes.rows("NORD.toml"), [Row::Found(0)]);
        assert!(themes.rows("no such thing").is_empty());
    }

    #[test]
    fn a_single_file_and_empty_repositories() {
        let fetch = source::resolve("https://example.org/t/nord.toml").unwrap();
        let found = candidates(&fetch, NORD.as_bytes()).unwrap();
        assert_eq!((found[0].entry.as_str(), found[0].problem.is_none()), ("nord.toml", true));
        assert!(candidates(&fetch, &vec![b' '; 70 << 10]).unwrap()[0].problem.is_some());
        let repo = source::resolve("https://github.com/a/b").unwrap();
        assert!(candidates(&repo, &repo_zip(&[("r/readme.md", "")])).unwrap_err().starts_with("No .toml file"));
        assert!(candidates(&repo, b"<html>").is_err());
    }

    #[test]
    fn a_theme_file_is_found_whatever_its_case() {
        let dir = std::env::temp_dir().join(format!("gezik-theme-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Nord.toml"), "").unwrap();
        std::fs::write(dir.join("frost.TOML"), "").unwrap();
        let name = |id| theme_path(&dir, id).file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name("nord"), "Nord.toml", "the file the store read, not a second one beside it");
        assert_eq!(name("frost"), "frost.toml", "the store does not read .TOML");
        assert_eq!(name("new"), "new.toml");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_https_source_pages_open() {
        assert_eq!(source_page("github.com/a/b").as_deref(), Some("https://github.com/a/b"));
        assert_eq!(source_page("https://github.com/a/b").as_deref(), Some("https://github.com/a/b"));
        assert_eq!(source_page("file:///C:/Windows/System32/calc.exe"), None);
        assert_eq!(source_page("C:\\x.toml"), None);
    }
}
