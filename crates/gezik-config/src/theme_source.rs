//! Themes from GitHub (spec 14 §6), the pure part: which address is downloaded for what the user
//! pasted, which zip entries may be themes, whether a file is one, the file name Gezik picks, and
//! the source line written on top of an installed theme (what Check for Updates reads back).
//! A theme is data only (colors, sizes, a font's name): nothing in it is run or names a file.

use crate::theme::parse_theme;
use gezik_i18n::{t, tx};

/// The most downloaded (a repository's zip).
pub const MAX_DOWNLOAD: u64 = 5 << 20;
/// The most one theme file may have, unpacked.
pub const MAX_THEME: u64 = 64 << 10;
/// The most themes taken from one download.
pub const MAX_THEMES: usize = 32;
/// The longest file name (without `.toml`) Gezik gives a theme.
const MAX_ID: usize = 48;

const SOURCE_PREFIX: &str = "# Installed from ";

/// What is downloaded for an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub url: String,
    /// A repository's zip (else one `.toml`).
    pub zip: bool,
    /// In the zip, under GitHub's top folder: the folder the themes are in (`""`, `"sub/dir/"`).
    pub folder: String,
}

/// The download for a pasted address: `github.com/o/r` (or `…/tree/<branch>[/<folder>]`) is the
/// repository's zip, `…/blob/<branch>/<path>.toml` its raw file, any other `https://…/….toml` that
/// file. Query and fragment are dropped. Anything else, plain http among it, is refused (why:
/// marked with `tx!`, translated where shown).
pub fn resolve(address: &str) -> Result<Fetch, &'static str> {
    const WHAT: &str = tx!("Paste a GitHub repository or an https address of a .toml file");
    let address = address.trim();
    if address.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(WHAT);
    }
    let rest = match address.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => rest,
        Some(_) => return Err(tx!("Only https addresses are downloaded")),
        None => address,
    };
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host_ok = host.contains('.')
        && !host.starts_with('.')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    // `..` (written plainly or as `%2e`) would leave the folder asked for once the server
    // resolves it.
    let dots = |p: &&str| matches!(p.to_ascii_lowercase().replace("%2e", ".").as_str(), "." | "..");
    if !host_ok || parts.iter().any(dots) {
        return Err(WHAT);
    }
    let toml = parts.last().is_some_and(|p| p.to_ascii_lowercase().ends_with(".toml"));
    if host == "github.com" || host == "www.github.com" {
        let name_ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
        let (owner, repo) = match parts.as_slice() {
            [owner, repo, ..] => (*owner, repo.strip_suffix(".git").unwrap_or(repo)),
            _ => return Err(WHAT),
        };
        // No escapes in a branch or a path: what GitHub decodes is not what was checked.
        if !name_ok(owner) || !name_ok(repo) || parts.iter().any(|p| p.contains('%')) {
            return Err(WHAT);
        }
        let zipball = format!("https://api.github.com/repos/{owner}/{repo}/zipball");
        return match &parts[2..] {
            [] => Ok(Fetch { url: zipball, zip: true, folder: String::new() }),
            ["tree", branch, folder @ ..] => Ok(Fetch {
                url: format!("{zipball}/{branch}"),
                zip: true,
                folder: folder.iter().map(|p| format!("{p}/")).collect(),
            }),
            ["blob", branch, file @ ..] if !file.is_empty() && toml => Ok(Fetch {
                url: format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{}", file.join("/")),
                zip: false,
                folder: String::new(),
            }),
            _ => Err(WHAT),
        };
    }
    if !toml {
        return Err(WHAT);
    }
    Ok(Fetch { url: format!("https://{host}/{}", parts.join("/")), zip: false, folder: String::new() })
}

/// The path in the repository of zip entry `name` when it may be a theme: a `.toml` in `folder`
/// (under GitHub's top folder) or in its `themes/`; deeper folders are not looked at.
pub fn theme_entry<'a>(folder: &str, name: &'a str) -> Option<&'a str> {
    let (_, path) = name.split_once('/')?;
    let file = path.strip_prefix(folder)?;
    let file = file.strip_prefix("themes/").unwrap_or(file);
    (!file.contains('/') && file.len() > ".toml".len() && file.to_ascii_lowercase().ends_with(".toml")).then_some(path)
}

/// A theme found in a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Its path in the repository (`themes/nord.toml`), or the file's name.
    pub entry: String,
    /// Its `name`, else its file name's.
    pub name: String,
    /// The file name Gezik would give it (`nord`; see `free_id`).
    pub id: String,
    pub text: String,
    /// Why it cannot be installed.
    pub problem: Option<String>,
    /// How many of its values Gezik skips (wrong type, out of range).
    pub skipped: usize,
}

/// Checks a downloaded file (`None`: larger than `MAX_THEME`).
pub fn candidate(entry: &str, bytes: Option<Vec<u8>>) -> Candidate {
    let stem = entry.rsplit('/').next().unwrap_or(entry);
    let stem = stem.get(..stem.len().saturating_sub(".toml".len())).unwrap_or(stem);
    let mut found = Candidate {
        entry: entry.chars().map(|c| if c.is_control() { '_' } else { c }).collect(),
        name: stem.to_owned(),
        id: file_id(stem),
        text: String::new(),
        problem: None,
        skipped: 0,
    };
    let Some(bytes) = bytes else {
        found.problem = Some(t!("larger than {} KB", MAX_THEME >> 10));
        return found;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        found.problem = Some(t!("not UTF-8 text").into_owned());
        return found;
    };
    found.text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();
    let mut warnings = Vec::new();
    let theme = parse_theme(entry, &found.text, &mut warnings);
    let table = found.text.parse::<toml::Table>().ok();
    match (theme, table) {
        (Some(theme), Some(table)) if ["colors", "metrics", "base"].iter().any(|k| table.contains_key(*k)) => {
            if let Some(name) = theme.display_name.filter(|n| !n.trim().is_empty()) {
                found.name = name.chars().filter(|c| !c.is_control()).take(80).collect();
            }
            found.skipped = warnings.len();
        }
        (Some(_), _) => found.problem = Some(t!("not a theme: no [colors], [metrics] or base").into_owned()),
        _ => {
            found.problem = Some(warnings.first().map_or_else(|| t!("not valid TOML").into_owned(), |w| w.to_string()))
        }
    }
    found
}

/// The file name for a theme called `stem`: lowercase `a-z`, `0-9` and `-`, at most 48 long;
/// never a built-in's id or `auto` (those get `-theme`).
pub fn file_id(stem: &str) -> String {
    let mut id = String::new();
    for c in stem.to_lowercase().chars() {
        match c {
            'a'..='z' | '0'..='9' => id.push(c),
            _ if !id.is_empty() && !id.ends_with('-') => id.push('-'),
            _ => {}
        }
    }
    let mut id: String = id.chars().take(MAX_ID).collect();
    while id.ends_with('-') {
        id.pop();
    }
    if id.is_empty() {
        id = "theme".to_owned();
    }
    // Windows cannot make a file of a device's name (`nul.toml` is the device).
    let device = matches!(id.as_str(), "con" | "nul" | "aux" | "prn")
        || ["com", "lpt"].iter().any(|d| {
            id.strip_prefix(d).is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        });
    if id == "auto" || device || crate::theme::BUILTIN_THEMES.contains(&id.as_str()) {
        id.push_str("-theme");
    }
    id
}

/// `id`, or the first of `id-2`, `id-3`, … that `taken` does not have (Keep Both).
pub fn free_id(id: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(id) {
        return id.to_owned();
    }
    (2..).map(|n| format!("{id}-{n}")).find(|id| !taken(id)).unwrap_or_default()
}

/// The installed file: the source line (address, entry, date `YYYY-MM-DD`), then the theme.
pub fn with_source(text: &str, address: &str, entry: &str, date: &str) -> String {
    format!("{SOURCE_PREFIX}{address} ({entry}) on {date}\n{}", body(text))
}

/// Where an installed theme came from: its address and entry, from its first line.
pub fn source_of(text: &str) -> Option<(&str, &str)> {
    let line = text.lines().next()?.strip_prefix(SOURCE_PREFIX)?;
    let (rest, _date) = line.rsplit_once(") on ")?;
    let (address, entry) = rest.split_once(" (")?;
    Some((address, entry))
}

/// A theme's text without its source line.
pub fn body(text: &str) -> &str {
    match text.strip_prefix(SOURCE_PREFIX) {
        Some(_) => text.split_once('\n').map_or("", |(_, rest)| rest),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip(url: &str, folder: &str) -> Result<Fetch, &'static str> {
        Ok(Fetch { url: url.to_owned(), zip: true, folder: folder.to_owned() })
    }

    fn file(url: &str) -> Result<Fetch, &'static str> {
        Ok(Fetch { url: url.to_owned(), zip: false, folder: String::new() })
    }

    #[test]
    fn addresses_resolve_to_a_download() {
        let api = "https://api.github.com/repos/arc/nord-gezik/zipball";
        assert_eq!(resolve("https://github.com/arc/nord-gezik"), zip(api, ""));
        assert_eq!(resolve("  github.com/arc/nord-gezik.git/  "), zip(api, ""), "no scheme, .git, slash");
        assert_eq!(resolve("HTTPS://GitHub.com/arc/nord-gezik?tab=readme#top"), zip(api, ""), "query dropped");
        assert_eq!(resolve("https://github.com/arc/nord-gezik/tree/main"), zip(&format!("{api}/main"), ""));
        assert_eq!(
            resolve("https://github.com/arc/nord-gezik/tree/dev/extra/dark"),
            zip(&format!("{api}/dev"), "extra/dark/")
        );
        assert_eq!(
            resolve("https://github.com/arc/nord-gezik/blob/main/themes/nord.toml"),
            file("https://raw.githubusercontent.com/arc/nord-gezik/main/themes/nord.toml")
        );
        assert_eq!(
            resolve("https://raw.githubusercontent.com/arc/nord-gezik/main/nord.toml"),
            file("https://raw.githubusercontent.com/arc/nord-gezik/main/nord.toml")
        );
        assert_eq!(resolve("https://example.org/t/Nord.TOML?x=1"), file("https://example.org/t/Nord.TOML"));
    }

    #[test]
    fn other_addresses_are_refused() {
        for bad in [
            "http://github.com/arc/nord",
            "http://example.org/nord.toml",
            "file:///C:/nord.toml",
            "ftp://example.org/nord.toml",
            "https://example.org/nord.json",
            "https://example.org/",
            "https://github.com/arc",
            "https://github.com/arc/nord/blob/main/readme.md",
            "https://github.com/arc/nord/issues",
            "https://github.com/a%2Fb/nord",
            "https://user@example.org/nord.toml",
            "https://example.org:8443/nord.toml",
            "https://example.org/../nord.toml",
            "https://github.com/arc/nord/tree/main/../x",
            "https://github.com/arc/nord/blob/%2e%2e/%2E%2e/x.toml",
            "https://github.com/arc/nord/tree/ma%2Fin",
            "https://example.org/a/%2e%2E/x.toml",
            "https://example.org/.%2e/x.toml",
            "https://example.org/a b.toml",
            "https://example.org/a\n.toml",
            "C:\\themes\\nord.toml",
            "nord.toml",
            "",
        ] {
            assert!(resolve(bad).is_err(), "{bad:?}");
        }
        assert_eq!(resolve("http://github.com/a/b"), Err("Only https addresses are downloaded"));
    }

    #[test]
    fn themes_are_looked_for_at_the_top_and_in_themes() {
        let entry = |name| theme_entry("", name);
        assert_eq!(entry("arc-nord-1a2b/nord.toml"), Some("nord.toml"));
        assert_eq!(entry("arc-nord-1a2b/themes/frost.TOML"), Some("themes/frost.TOML"));
        assert_eq!(entry("arc-nord-1a2b/extra/themes/x.toml"), None, "deeper folders are not looked at");
        assert_eq!(entry("arc-nord-1a2b/themes/"), None);
        assert_eq!(entry("arc-nord-1a2b/.toml"), None);
        assert_eq!(entry("arc-nord-1a2b/readme.md"), None);
        assert_eq!(entry("nord.toml"), None, "outside GitHub's top folder");
        assert_eq!(theme_entry("extra/dark/", "x-1/extra/dark/themes/a.toml"), Some("extra/dark/themes/a.toml"));
        assert_eq!(theme_entry("extra/dark/", "x-1/nord.toml"), None, "outside the folder asked for");
    }

    #[test]
    fn a_file_is_checked() {
        let theme = |text: &str| candidate("themes/Nord Frost.toml", Some(text.as_bytes().to_vec()));
        let nord = theme("\u{feff}name = \"Nord\"\n[colors]\naccent = \"#88c0d0\"\nbackground = \"blue\"\n");
        assert_eq!((nord.name.as_str(), nord.id.as_str(), nord.problem, nord.skipped), ("Nord", "nord-frost", None, 1));
        assert!(nord.text.starts_with("name"), "the BOM goes");
        assert_eq!(theme("base = \"dark\"\n").name, "Nord Frost", "no name: the file's");
        assert_eq!(
            theme("[package]\nname = \"x\"\n").problem.as_deref(),
            Some("not a theme: no [colors], [metrics] or base")
        );
        assert!(theme("[colors\n").problem.unwrap().contains("line 1"));
        assert_eq!(candidate("x.toml", Some(vec![0xff, 0xfe])).problem.as_deref(), Some("not UTF-8 text"));
        assert_eq!(candidate("x.toml", None).problem.as_deref(), Some("larger than 64 KB"));
        assert_eq!(candidate("a\nb.toml", None).entry, "a_b.toml", "no line break in the source line");
    }

    #[test]
    fn gezik_picks_the_file_name() {
        assert_eq!(file_id("Nord Frost (v2)"), "nord-frost-v2");
        assert_eq!(file_id("../../evil"), "evil");
        assert_eq!(file_id("C:\\Windows\\x"), "c-windows-x");
        assert_eq!(file_id("çok güzel"), "ok-g-zel");
        assert_eq!(file_id("..."), "theme");
        assert_eq!(file_id("Dark"), "dark-theme", "a built-in is not shadowed");
        assert_eq!(file_id("auto"), "auto-theme", "reserved");
        for device in ["con", "NUL", "Aux.toml", "prn", "com1", "LPT9"] {
            let id = file_id(device.strip_suffix(".toml").unwrap_or(device));
            assert!(id.ends_with("-theme"), "{device}: {id}");
        }
        assert_eq!(file_id("aux toml"), "aux-toml", "with something after it, not the device");
        assert_eq!(file_id("com10"), "com10");
        assert_eq!(file_id(&"a".repeat(100)).len(), 48);
        assert_eq!(free_id("nord", |id| id == "nord" || id == "nord-2"), "nord-3");
        assert_eq!(free_id("nord", |_| false), "nord");
    }

    #[test]
    fn the_source_line_reads_back() {
        let address = "https://github.com/arc/nord-gezik";
        let text = with_source("[colors]\n", address, "themes/nord (x).toml", "2026-10-12");
        assert_eq!(text, format!("# Installed from {address} (themes/nord (x).toml) on 2026-10-12\n[colors]\n"));
        assert_eq!(source_of(&text), Some((address, "themes/nord (x).toml")));
        assert_eq!(body(&text), "[colors]\n");
        assert_eq!(with_source(&text, address, "a.toml", "2026-10-13").lines().count(), 2, "an update: one line");
        assert_eq!(source_of("[colors]\n"), None);
        assert_eq!(body("[colors]\n"), "[colors]\n");
        assert!(resolve(source_of(&text).unwrap().0).is_ok(), "Check for Updates downloads the same");
    }
}
