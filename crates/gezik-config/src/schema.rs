//! What the settings window shows of `settings.toml` (spec 14 §3.3): every single setting, its
//! page and its kind of control. The descriptions and defaults come from the template's
//! comments and values (already in the exe), the values from the file's text; the tests bind
//! the three (the template, this list and `Settings::parse`), so a setting added to the template
//! fails them until it is placed here.

use crate::Warning;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    General,
    Appearance,
    FilesAndTools,
    Layouts,
    Tags,
    Keyboard,
    System,
    Advanced,
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::General,
        Page::Appearance,
        Page::FilesAndTools,
        Page::Layouts,
        Page::Tags,
        Page::Keyboard,
        Page::System,
        Page::Advanced,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::FilesAndTools => "Files and Tools",
            Page::Layouts => "Layouts",
            Page::Tags => "Tags",
            Page::Keyboard => "Keyboard",
            Page::System => "System",
            Page::Advanced => "Advanced",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    /// One of these; "true"/"false" are written as booleans, digits as a number.
    Choice(&'static [&'static str]),
    Int,
    Text,
    Path,
    /// A theme's id: the built-ins and the themes folder's (`theme` also "auto").
    Theme,
    /// Written by System Integration (the tray, the global shortcut): shown here, changed there.
    Integration,
    /// A list or tables: counted here, edited in settings.toml (spec 14 decision 4).
    List,
}

/// One setting: `key` in `[table]`, or at the top with no table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub table: Option<&'static str>,
    pub key: &'static str,
    pub page: Page,
    pub kind: Kind,
}

impl Field {
    /// `layout.density`, `theme`: Copy Key's text.
    pub fn path(&self) -> String {
        match self.table {
            Some(table) => format!("{table}.{}", self.key),
            None => self.key.to_owned(),
        }
    }

    /// `reduce-motion` → "Reduce motion".
    pub fn title(&self) -> String {
        let mut title = self.key.replace('-', " ");
        if let Some(first) = title.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        title
    }
}

const fn f(table: Option<&'static str>, key: &'static str, page: Page, kind: Kind) -> Field {
    Field { table, key, page, kind }
}

const THREADS: &[&str] =
    &["auto", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16"];

use Kind::{Bool, Choice, Int, Integration, List, Path, Text, Theme};
use Page::{Advanced, Appearance, FilesAndTools, General, Keyboard, Layouts, System, Tags};

/// Every setting the window shows, in its order.
pub const FIELDS: &[Field] = &[
    f(None, "start-folder", General, Path),
    f(None, "max-fps", General, Int),
    f(Some("session"), "restore", General, Bool),
    f(Some("history"), "remember", General, Bool),
    f(Some("files"), "confirm-trash", General, Bool),
    f(Some("files"), "copy-threads", General, Choice(THREADS)),
    f(Some("files"), "verify", General, Bool),
    f(Some("files"), "max-speed", General, Text),
    f(None, "theme", Appearance, Theme),
    f(None, "theme-light", Appearance, Theme),
    f(None, "theme-dark", Appearance, Theme),
    f(Some("layout"), "sidebar", Appearance, Choice(&["left", "right", "hidden"])),
    f(Some("layout"), "density", Appearance, Choice(&["compact", "comfortable"])),
    f(Some("layout"), "reduce-motion", Appearance, Bool),
    f(Some("view"), "icons", Appearance, Choice(&["system", "gezik"])),
    f(Some("view"), "thumbnails", Appearance, Bool),
    f(Some("view"), "hide-extensions", Appearance, Bool),
    f(Some("view"), "date-format", Appearance, Choice(&["relative", "short", "iso", "system"])),
    f(Some("view"), "size-format", Appearance, Choice(&["binary", "decimal"])),
    f(Some("view"), "show-hidden", Appearance, Bool),
    f(Some("view"), "show-system", Appearance, Bool),
    f(Some("view"), "mode", Layouts, Choice(&["list", "grid", "columns"])),
    f(Some("view"), "sort", Layouts, Choice(&["name", "modified", "created", "type", "size", "tags", "rating"])),
    f(Some("view"), "sort-dir", Layouts, Choice(&["asc", "desc"])),
    f(Some("view"), "grid-size", Layouts, Choice(&["small", "medium", "large"])),
    f(Some("view"), "group", Layouts, Choice(&["none", "type", "date", "size", "tag", "rating"])),
    f(Some("view"), "folders-first", Layouts, Bool),
    f(Some("view"), "single-click-open", Layouts, Bool),
    f(Some("view"), "folder-sizes", Layouts, Choice(&["off", "local", "all"])),
    f(Some("panes"), "confirm", Layouts, Bool),
    f(Some("sidebar"), "cloud", Layouts, Bool),
    f(Some("sidebar"), "tags", Layouts, Bool),
    f(Some("sidebar"), "tree-follow", Layouts, Bool),
    f(Some("preview"), "properties", Layouts, Bool),
    f(Some("archives"), "double-click", FilesAndTools, Choice(&["system", "extract-here"])),
    f(Some("convert"), "ffmpeg", FilesAndTools, Path),
    f(Some("tools"), "download", FilesAndTools, Bool),
    f(Some("tools"), "seven-zip", FilesAndTools, Path),
    f(Some("tools"), "diff", FilesAndTools, Path),
    f(Some("tools"), "folder-diff", FilesAndTools, Bool),
    f(Some("search"), "everything", FilesAndTools, Choice(&["auto", "off"])),
    f(Some("search"), "max-results", FilesAndTools, Int),
    f(Some("search"), "content-max-size", FilesAndTools, Text),
    f(Some("tags"), "write-to-files", Tags, Bool),
    f(Some("tags"), "dots", Tags, Choice(&["auto", "true", "false"])),
    f(Some("tags"), "list", Tags, List),
    f(Some("keyboard"), "typing", Keyboard, Choice(&["jump", "filter"])),
    f(None, "shortcuts", Keyboard, List),
    f(Some("system"), "single-instance", System, Bool),
    f(Some("system"), "quick-look", System, Choice(&["system", "gezik"])),
    f(Some("system"), "tray", System, Integration),
    f(Some("system"), "hotkey", System, Integration),
    f(None, "pinned", Advanced, List),
    f(Some("tools"), "diff-args", Advanced, List),
    f(Some("terminal"), "command", Advanced, List),
    f(Some("search"), "skip", Advanced, List),
    f(None, "rename-presets", Advanced, List),
    f(None, "filters", Advanced, List),
    f(None, "tab-sets", Advanced, List),
    f(None, "searches", Advanced, List),
    f(None, "sync-profiles", Advanced, List),
    f(None, "view-rules", Advanced, List),
    f(None, "highlight-rules", Advanced, List),
    f(None, "commands", Advanced, List),
];

/// What the template says of a field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Described {
    /// The comment lines above its line and the comment after its value.
    pub description: String,
    /// The default as the window shows it ("" for a list).
    pub default: String,
    /// The default as TOML, written by Reset to Default; `None` for a key the template only
    /// names in a comment (its default is the key's absence: Reset takes it out).
    pub literal: Option<String>,
}

/// One key line of the template: `key = …`, or `# key = …` (a setting left at its default).
struct TemplateLine<'a> {
    table: Option<&'a str>,
    key: &'a str,
    commented: bool,
    /// The line without its `#`.
    body: &'a str,
    /// The comment lines right above it.
    above: Vec<&'a str>,
}

/// `[name]` or `[[name]]`, also commented out: the name.
fn header(body: &str) -> Option<&str> {
    let name = body.strip_prefix('[')?.trim_start_matches('[');
    let name = name.trim_end().strip_suffix(']')?.trim_end_matches(']');
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')).then_some(name)
}

/// `key = …`: the key.
fn key_of(body: &str) -> Option<&str> {
    let (key, _) = body.split_once('=')?;
    let key = key.trim_end();
    (!key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')).then_some(key)
}

/// The template's key lines, outside the commented `[[…]]` examples.
fn template_lines(template: &str) -> Vec<TemplateLine<'_>> {
    let mut out = Vec::new();
    let (mut table, mut in_list, mut above) = (None, false, Vec::new());
    for line in template.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            above.clear();
            continue;
        }
        let (commented, body) = match line.strip_prefix('#') {
            Some(rest) => (true, rest.trim()),
            None => (false, line.trim()),
        };
        if let Some(name) = header(body) {
            (table, in_list) = (Some(name), body.starts_with("[["));
            above.clear();
            continue;
        }
        match key_of(body) {
            // A commented example (`# pinned = […]`) leaves the comment above to the key's own line.
            Some(key) if !in_list && commented => {
                out.push(TemplateLine { table, key, commented, body, above: above.clone() })
            }
            Some(key) if !in_list => {
                out.push(TemplateLine { table, key, commented, body, above: std::mem::take(&mut above) })
            }
            Some(_) => above.clear(),
            None if commented => above.push(body),
            None => above.clear(),
        }
    }
    out
}

/// A value as the window shows it: text bare, a list or table by its count.
fn shown(value: &toml::Value) -> String {
    let count = |n: usize| match n {
        0 => "none".to_owned(),
        1 => "1 entry".to_owned(),
        n => format!("{n} entries"),
    };
    match value {
        toml::Value::String(text) => text.clone(),
        toml::Value::Array(items) => count(items.len()),
        toml::Value::Table(table) => count(table.len()),
        other => other.to_string(),
    }
}

/// The default of a key the template only names in a comment, where it differs by system.
fn platform_default(field: &Field) -> Option<&'static str> {
    (field.path() == "view.show-hidden").then_some(if cfg!(target_os = "macos") { "false" } else { "true" })
}

/// What the template says of each of `FIELDS`, in order. Read when the window opens.
pub fn describe(template: &str) -> Vec<Described> {
    let lines = template_lines(template);
    FIELDS
        .iter()
        .map(|field| {
            // The key's own line wins over a commented example of it (`# pinned = […]`).
            let mine = |l: &&TemplateLine<'_>| l.table == field.table && l.key == field.key;
            let Some(line) = lines.iter().filter(mine).min_by_key(|l| l.commented) else {
                return Described::default();
            };
            let mut description = line.above.join(" ");
            let value = line
                .body
                .parse::<toml_edit::DocumentMut>()
                .ok()
                .and_then(|doc| doc.get(field.key).and_then(|item| item.as_value()).cloned());
            let mut literal = None;
            let mut default = String::new();
            if let Some(mut value) = value {
                let hint = value.decor().suffix().and_then(|s| s.as_str()).unwrap_or_default();
                let hint = hint.trim().trim_start_matches('#').trim();
                // The comment after the value first, as a sentence: it is about this key, the
                // lines above may be about the ones below it too.
                if !hint.is_empty() {
                    let mut first = hint.to_owned();
                    if let Some(letter) = first.get_mut(..1) {
                        letter.make_ascii_uppercase();
                    }
                    if !description.is_empty() {
                        let stop = if first.ends_with(['.', ')', ':']) { " " } else { ". " };
                        first.push_str(stop);
                    }
                    description.insert_str(0, &first);
                }
                value.decor_mut().clear();
                let text = value.to_string();
                default = text.parse::<toml::Value>().map(|v| shown(&v)).unwrap_or_default();
                literal = (!line.commented).then_some(text);
            }
            if let Some(platform) = platform_default(field) {
                default = platform.to_owned();
            }
            if field.kind == Kind::List {
                default.clear();
            }
            Described { description, default, literal }
        })
        .collect()
}

/// The values `text` (settings.toml) gives `FIELDS`, in order (`None`: not in the file, the
/// default); a syntax error with its line.
pub fn values(text: &str) -> Result<Vec<Option<String>>, Warning> {
    let table = text.parse::<toml::Table>().map_err(|err| Warning::from_toml_error("settings.toml", text, &err))?;
    Ok(FIELDS
        .iter()
        .map(|field| {
            let holder = match field.table {
                None => Some(&table),
                Some(name) => table.get(name).and_then(toml::Value::as_table),
            };
            holder?.get(field.key).map(shown)
        })
        .collect())
}

/// How many keys of `text` Gezik does not know (they stay in the file; spec 14 §4.2).
pub fn unknown_keys(text: &str) -> usize {
    let Ok(table) = text.parse::<toml::Table>() else { return 0 };
    let known = |table: Option<&str>, key: &str| FIELDS.iter().any(|f| f.table == table && f.key == key);
    table
        .iter()
        .map(|(name, value)| match value {
            // A table of settings: its keys one by one.
            toml::Value::Table(inner) if FIELDS.iter().any(|f| f.table == Some(name)) => {
                inner.keys().filter(|key| !known(Some(name), key)).count()
            }
            _ => usize::from(!known(None, name)),
        })
        .sum()
}

/// What to write for `input` (a typed or chosen value) as TOML; `None` when it cannot be one
/// of `kind` (a number field with letters).
pub fn literal(kind: Kind, input: &str) -> Option<String> {
    let input = input.trim();
    match kind {
        Kind::Bool => matches!(input, "true" | "false").then(|| input.to_owned()),
        Kind::Int => input.parse::<i64>().ok().map(|n| n.to_string()),
        Kind::Choice(options) if !options.contains(&input) => None,
        Kind::Choice(_) if matches!(input, "true" | "false") || input.bytes().all(|b| b.is_ascii_digit()) => {
            Some(input.to_owned())
        }
        Kind::Choice(_) | Kind::Text | Kind::Path | Kind::Theme => {
            Some(toml::Value::String(input.to_owned()).to_string())
        }
        Kind::Integration | Kind::List => None,
    }
}

/// Whether Gezik reads `literal` for `field` without a warning (the parser decides, so the
/// window never writes what the next reload would refuse); the warning's text if not.
pub fn check(field: &Field, literal: &str) -> Result<(), String> {
    let text = match field.table {
        Some(table) => format!("[{table}]\n{} = {literal}\n", field.key),
        None => format!("{} = {literal}\n", field.key),
    };
    let mut warnings = Vec::new();
    Settings::parse("settings.toml", &text, &mut warnings);
    match warnings.into_iter().next() {
        None => Ok(()),
        Some(warning) => Err(warning.message),
    }
}

/// Whether the search box's `query` finds `field`: in its title, its key or its description
/// (case ignored; every word somewhere).
pub fn matches(field: &Field, described: &Described, query: &str) -> bool {
    let haystack = format!("{} {} {}", field.title(), field.path(), described.description).to_lowercase();
    query.to_lowercase().split_whitespace().all(|word| haystack.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SETTINGS_TEMPLATE;

    fn fields() -> impl Iterator<Item = (usize, &'static Field)> {
        FIELDS.iter().enumerate()
    }

    #[test]
    fn every_template_setting_is_in_the_window() {
        for line in template_lines(SETTINGS_TEMPLATE) {
            let mine = FIELDS.iter().any(|f| f.table == line.table && f.key == line.key);
            // A key in a table that is itself a list setting (`[shortcuts]`'s actions).
            let in_list = FIELDS.iter().any(|f| f.kind == Kind::List && f.table.is_none() && Some(f.key) == line.table);
            assert!(mine || in_list, "{:?}.{} is not in FIELDS", line.table, line.key);
        }
    }

    #[test]
    fn every_field_is_in_the_template_with_a_description() {
        let described = describe(SETTINGS_TEMPLATE);
        let lines = template_lines(SETTINGS_TEMPLATE);
        for (i, field) in fields() {
            let path = field.path();
            // The `[[…]]` lists are commented examples in the template: their header is there.
            let listed = SETTINGS_TEMPLATE.contains(&format!("# [[{}]]", field.key))
                || SETTINGS_TEMPLATE.contains(&format!(
                    "
[{}]
",
                    field.key
                ));
            let line = lines.iter().any(|l| l.table == field.table && l.key == field.key);
            assert!(line || listed, "{path} is not in the template");
            if line {
                assert!(!described[i].description.is_empty(), "{path} has no comment in the template");
            }
            if matches!(field.kind, Kind::Bool | Kind::Choice(_) | Kind::Int | Kind::Theme) {
                assert!(!described[i].default.is_empty(), "{path} has no default");
            }
        }
        let paths: Vec<String> = fields().map(|(_, f)| f.path()).collect();
        for (i, path) in paths.iter().enumerate() {
            assert!(!paths[..i].contains(path), "{path} twice");
        }
    }

    #[test]
    fn every_choice_and_default_reads_back_without_a_warning() {
        let described = describe(SETTINGS_TEMPLATE);
        for (i, field) in fields() {
            if let Kind::Choice(options) = field.kind {
                for option in options {
                    let text = literal(field.kind, option).unwrap();
                    assert_eq!(check(field, &text), Ok(()), "{} = {text}", field.path());
                }
                assert!(options.contains(&described[i].default.as_str()), "{}: {:?}", field.path(), described[i]);
            }
            if let Some(default) = &described[i].literal {
                assert_eq!(check(field, default), Ok(()), "{} = {default}", field.path());
            }
            if !matches!(field.kind, Kind::List | Kind::Integration) {
                let text = literal(field.kind, &described[i].default).unwrap();
                assert_eq!(check(field, &text), Ok(()), "{}'s default as the window writes it", field.path());
            }
        }
    }

    #[test]
    fn wrong_values_are_refused_by_the_parser() {
        let field = |path: &str| fields().map(|(_, f)| f).find(|f| f.path() == path).unwrap();
        assert!(check(field("max-fps"), "5").unwrap_err().contains("max-fps"), "out of range");
        assert!(check(field("search.max-results"), "10").is_err());
        assert!(check(field("files.max-speed"), "\"fast\"").is_err());
        assert_eq!(check(field("files.max-speed"), "\"20 MB/s\""), Ok(()));
        assert!(check(field("start-folder"), "\"{home}/../x\"").is_err());
        assert_eq!(literal(Kind::Int, "12a"), None);
        assert_eq!(literal(Kind::Int, " 60 "), Some("60".to_owned()));
        assert_eq!(literal(Kind::Bool, "yes"), None);
        assert_eq!(literal(field("files.copy-threads").kind, "4"), Some("4".to_owned()), "a number");
        assert_eq!(literal(field("tags.dots").kind, "false"), Some("false".to_owned()), "a boolean");
        assert_eq!(literal(field("layout.density").kind, "tight"), None);
        assert_eq!(literal(Kind::Path, "C:\\Tools \"x\""), Some("'C:\\Tools \"x\"'".to_owned()));
        assert_eq!(literal(Kind::List, "x"), None);
    }

    #[test]
    fn descriptions_and_defaults_come_from_the_template() {
        let described = describe(SETTINGS_TEMPLATE);
        let get = |path: &str| {
            let i = FIELDS.iter().position(|f| f.path() == path).unwrap();
            described[i].clone()
        };
        let density = get("layout.density");
        assert_eq!((density.description.as_str(), density.default.as_str()), ("compact | comfortable", "comfortable"));
        assert!(!described.iter().any(|d| d.description.contains('\u{22ef}')), "the UI font has no ⋯");
        assert_eq!(density.literal.as_deref(), Some("\"comfortable\""));
        let sort = get("view.sort");
        assert!(sort.description == "Name | modified | created | type | size | tags | rating", "{sort:?}");
        let hide = get("view.hide-extensions").description;
        assert_eq!(
            hide,
            "Show names without their extension. The same in every folder; the View menu changes these here too."
        );
        let hidden = get("view.show-hidden");
        assert_eq!(hidden.literal, None, "commented: Reset takes it out");
        assert_eq!(hidden.default, if cfg!(target_os = "macos") { "false" } else { "true" });
        assert!(hidden.description.starts_with("Dot names"), "{hidden:?}");
        assert_eq!(get("max-fps").default, "120");
        assert_eq!(get("pinned").default, "", "a list shows its count only");
        assert!(get("pinned").description.starts_with("Folders pinned to the sidebar"), "{:?}", get("pinned"));
    }

    #[test]
    fn values_are_what_the_file_says() {
        let text = "theme = \"nord\"\npinned = [\"/a\", \"/b\"]\n[layout]\ndensity = \"compact\"\n[shortcuts]\nnew-tab = \"ctrl+t\"\n";
        let values = values(text).unwrap();
        let get = |path: &str| values[FIELDS.iter().position(|f| f.path() == path).unwrap()].clone();
        assert_eq!(get("theme").as_deref(), Some("nord"));
        assert_eq!(get("layout.density").as_deref(), Some("compact"));
        assert_eq!(get("layout.sidebar"), None, "not in the file: the default");
        assert_eq!(get("pinned").as_deref(), Some("2 entries"));
        assert_eq!(get("shortcuts").as_deref(), Some("1 entry"));
        assert_eq!(get("max-fps"), None);
        let broken = super::values("theme = \"a\"\n[layout\n").unwrap_err();
        assert_eq!(broken.line, Some(2), "{broken}");
        assert!(broken.to_string().starts_with("settings.toml line 2: "), "{broken}");
    }

    #[test]
    fn unknown_keys_are_counted() {
        assert_eq!(unknown_keys(SETTINGS_TEMPLATE), 0);
        let text = "colour = 1\n[layout]\ndensity = \"compact\"\nwidth = 3\n[mine]\na = 1\nb = 2\n";
        assert_eq!(unknown_keys(text), 3, "colour, layout.width and the table mine");
        assert_eq!(unknown_keys("[broken\n"), 0);
    }

    #[test]
    fn the_search_finds_titles_keys_and_descriptions() {
        let described = describe(SETTINGS_TEMPLATE);
        let found = |query: &str| -> Vec<String> {
            fields().filter(|(i, f)| matches(f, &described[*i], query)).map(|(_, f)| f.path()).collect()
        };
        assert_eq!(found("density"), ["layout.density"]);
        assert!(found("REDUCE motion").contains(&"layout.reduce-motion".to_owned()));
        assert!(found("7-zip").contains(&"tools.seven-zip".to_owned()), "by its description");
        assert!(found("").len() > 60, "an empty search shows everything");
        assert!(found("no such setting").is_empty());
    }

    #[test]
    fn titles_and_paths() {
        let field = Field { table: Some("layout"), key: "reduce-motion", page: Page::Appearance, kind: Kind::Bool };
        assert_eq!((field.title(), field.path()), ("Reduce motion".to_owned(), "layout.reduce-motion".to_owned()));
        assert_eq!(Page::ALL.map(Page::title)[2], "Files and Tools");
    }
}
