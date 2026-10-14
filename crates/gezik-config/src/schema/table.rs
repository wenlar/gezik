//! The settings window's fields and the template's key lines (spec 14 §3.3), apart from the
//! rest of `schema` because `gezik-lang` (gezik-i18n) includes this file to put every title,
//! description and choice in the translations' template (spec 15 §7): std and `tx!` only.

use gezik_i18n::tx;

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
    /// Export, import and the backups (no fields: `settings_window`'s rows).
    SyncAndBackup,
}

impl Page {
    pub const ALL: [Page; 9] = [
        Page::General,
        Page::Appearance,
        Page::FilesAndTools,
        Page::Layouts,
        Page::Tags,
        Page::Keyboard,
        Page::System,
        Page::Advanced,
        Page::SyncAndBackup,
    ];

    /// Its title in English (shown through `gezik_i18n::t`).
    pub fn title(self) -> &'static str {
        match self {
            Page::General => tx!("General"),
            Page::Appearance => tx!("Appearance"),
            Page::FilesAndTools => tx!("Files and Tools"),
            Page::Layouts => tx!("Layouts"),
            Page::Tags => tx!("Tags"),
            Page::Keyboard => tx!("Keyboard"),
            Page::System => tx!("System"),
            Page::Advanced => tx!("Advanced"),
            Page::SyncAndBackup => tx!("Sync and Backup"),
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
    /// A language's code: "auto", the built-ins, the lang folder's, "pseudo" (spec 15 §5.1).
    Language,
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
    /// Its title's and description's translation context: `setting:layout.density`.
    pub fn context(&self) -> String {
        format!("setting:{}", self.path())
    }

    /// Its choices' translation context: `choice:layout.density`.
    pub fn choice_context(&self) -> String {
        format!("choice:{}", self.path())
    }

    /// `layout.density`, `theme`: Copy Key's text.
    pub fn path(&self) -> String {
        match self.table {
            Some(table) => format!("{table}.{}", self.key),
            None => self.key.to_owned(),
        }
    }

    /// `reduce-motion` → "Reduce motion": the English title, shown through
    /// `gezik_i18n::t(&self.context(), …)` (the pot has it, `gezik-lang`).
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

const THREADS: &[&str] = &[
    tx!("choice:files.copy-threads" => "auto"),
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
    "11",
    "12",
    "13",
    "14",
    "15",
    "16",
];

use Kind::{Bool, Choice, Int, Integration, Language, List, Path, Text, Theme};
use Page::{Advanced, Appearance, FilesAndTools, General, Keyboard, Layouts, System, Tags};

/// Every setting the window shows, in its order.
pub const FIELDS: &[Field] = &[
    f(None, "language", General, Language),
    f(None, "start-folder", General, Path),
    f(None, "max-fps", General, Int),
    f(Some("session"), "restore", General, Bool),
    f(Some("history"), "remember", General, Bool),
    f(Some("drag"), "spring-delay", General, Int),
    f(Some("files"), "confirm-trash", General, Bool),
    f(Some("files"), "copy-threads", General, Choice(THREADS)),
    f(Some("files"), "verify", General, Bool),
    f(Some("files"), "max-speed", General, Text),
    f(Some("update"), "check", General, Bool),
    f(
        Some("update"),
        "channel",
        General,
        Choice(&[
            tx!("choice:update.channel" => "auto"),
            tx!("choice:update.channel" => "stable"),
            tx!("choice:update.channel" => "pre"),
        ]),
    ),
    f(None, "theme", Appearance, Theme),
    f(None, "theme-light", Appearance, Theme),
    f(None, "theme-dark", Appearance, Theme),
    f(
        Some("layout"),
        "sidebar",
        Appearance,
        Choice(&[
            tx!("choice:layout.sidebar" => "left"),
            tx!("choice:layout.sidebar" => "right"),
            tx!("choice:layout.sidebar" => "hidden"),
        ]),
    ),
    f(
        Some("layout"),
        "density",
        Appearance,
        Choice(&[tx!("choice:layout.density" => "compact"), tx!("choice:layout.density" => "comfortable")]),
    ),
    f(Some("layout"), "reduce-motion", Appearance, Bool),
    f(
        Some("view"),
        "icons",
        Appearance,
        Choice(&[tx!("choice:view.icons" => "system"), tx!("choice:view.icons" => "gezik")]),
    ),
    f(Some("view"), "thumbnails", Appearance, Bool),
    f(Some("view"), "hide-extensions", Appearance, Bool),
    f(
        Some("view"),
        "date-format",
        Appearance,
        Choice(&[
            tx!("choice:view.date-format" => "relative"),
            tx!("choice:view.date-format" => "short"),
            tx!("choice:view.date-format" => "iso"),
            tx!("choice:view.date-format" => "system"),
        ]),
    ),
    f(
        Some("view"),
        "size-format",
        Appearance,
        Choice(&[tx!("choice:view.size-format" => "binary"), tx!("choice:view.size-format" => "decimal")]),
    ),
    f(Some("view"), "show-hidden", Appearance, Bool),
    f(Some("view"), "show-system", Appearance, Bool),
    f(
        Some("view"),
        "mode",
        Layouts,
        Choice(&[
            tx!("choice:view.mode" => "list"),
            tx!("choice:view.mode" => "grid"),
            tx!("choice:view.mode" => "columns"),
        ]),
    ),
    f(
        Some("view"),
        "sort",
        Layouts,
        Choice(&[
            tx!("choice:view.sort" => "name"),
            tx!("choice:view.sort" => "modified"),
            tx!("choice:view.sort" => "created"),
            tx!("choice:view.sort" => "type"),
            tx!("choice:view.sort" => "size"),
            tx!("choice:view.sort" => "tags"),
            tx!("choice:view.sort" => "rating"),
            tx!("choice:view.sort" => "git"),
        ]),
    ),
    f(
        Some("view"),
        "sort-dir",
        Layouts,
        Choice(&[tx!("choice:view.sort-dir" => "asc"), tx!("choice:view.sort-dir" => "desc")]),
    ),
    f(
        Some("view"),
        "grid-size",
        Layouts,
        Choice(&[
            tx!("choice:view.grid-size" => "small"),
            tx!("choice:view.grid-size" => "medium"),
            tx!("choice:view.grid-size" => "large"),
        ]),
    ),
    f(
        Some("view"),
        "group",
        Layouts,
        Choice(&[
            tx!("choice:view.group" => "none"),
            tx!("choice:view.group" => "type"),
            tx!("choice:view.group" => "date"),
            tx!("choice:view.group" => "size"),
            tx!("choice:view.group" => "tag"),
            tx!("choice:view.group" => "rating"),
            tx!("choice:view.group" => "git"),
        ]),
    ),
    f(Some("view"), "folders-first", Layouts, Bool),
    f(Some("view"), "single-click-open", Layouts, Bool),
    f(
        Some("view"),
        "folder-sizes",
        Layouts,
        Choice(&[
            tx!("choice:view.folder-sizes" => "off"),
            tx!("choice:view.folder-sizes" => "local"),
            tx!("choice:view.folder-sizes" => "all"),
        ]),
    ),
    f(Some("panes"), "confirm", Layouts, Bool),
    f(Some("sidebar"), "cloud", Layouts, Bool),
    f(Some("sidebar"), "tags", Layouts, Bool),
    f(Some("sidebar"), "tree-follow", Layouts, Bool),
    f(Some("preview"), "properties", Layouts, Bool),
    f(
        Some("archives"),
        "double-click",
        FilesAndTools,
        Choice(&[
            tx!("choice:archives.double-click" => "browse"),
            tx!("choice:archives.double-click" => "system"),
            tx!("choice:archives.double-click" => "extract-here"),
        ]),
    ),
    f(Some("archives"), "preview-limit", FilesAndTools, Int),
    f(Some("command-line"), "shell", FilesAndTools, Text),
    f(Some("git"), "status", FilesAndTools, Bool),
    f(Some("git"), "program", FilesAndTools, Path),
    f(Some("git"), "ignored", FilesAndTools, Bool),
    f(Some("git"), "network", FilesAndTools, Bool),
    f(Some("git"), "timeout", FilesAndTools, Int),
    f(Some("devices"), "phones", FilesAndTools, Bool),
    f(Some("devices"), "libmtp", FilesAndTools, Path),
    f(Some("convert"), "ffmpeg", FilesAndTools, Path),
    f(Some("tools"), "download", FilesAndTools, Bool),
    f(Some("tools"), "seven-zip", FilesAndTools, Path),
    f(Some("tools"), "diff", FilesAndTools, Path),
    f(Some("tools"), "folder-diff", FilesAndTools, Bool),
    f(
        Some("search"),
        "everything",
        FilesAndTools,
        Choice(&[tx!("choice:search.everything" => "auto"), tx!("choice:search.everything" => "off")]),
    ),
    f(Some("search"), "max-results", FilesAndTools, Int),
    f(Some("search"), "content-max-size", FilesAndTools, Text),
    f(Some("tags"), "write-to-files", Tags, Bool),
    f(
        Some("tags"),
        "dots",
        Tags,
        Choice(&[
            tx!("choice:tags.dots" => "auto"),
            tx!("choice:tags.dots" => "true"),
            tx!("choice:tags.dots" => "false"),
        ]),
    ),
    f(Some("tags"), "list", Tags, List),
    f(
        Some("keyboard"),
        "typing",
        Keyboard,
        Choice(&[tx!("choice:keyboard.typing" => "jump"), tx!("choice:keyboard.typing" => "filter")]),
    ),
    f(None, "shortcuts", Keyboard, List),
    f(Some("system"), "single-instance", System, Bool),
    f(
        Some("system"),
        "quick-look",
        System,
        Choice(&[tx!("choice:system.quick-look" => "system"), tx!("choice:system.quick-look" => "gezik")]),
    ),
    f(Some("system"), "tray", System, Integration),
    f(Some("system"), "hotkey", System, Integration),
    f(None, "pinned", Advanced, List),
    f(Some("tools"), "diff-args", Advanced, List),
    f(Some("terminal"), "command", Advanced, List),
    f(Some("command-line"), "shell-args", Advanced, List),
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

/// One key line of the template: `key = …`, or `# key = …` (a setting left at its default).
pub struct TemplateLine<'a> {
    pub table: Option<&'a str>,
    pub key: &'a str,
    pub commented: bool,
    /// The line without its `#`.
    pub body: &'a str,
    /// The comment lines right above it.
    pub above: Vec<&'a str>,
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
pub fn template_lines(template: &str) -> Vec<TemplateLine<'_>> {
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

/// The template line of `field`: its own line wins over a commented example of it
/// (`# pinned = […]`).
pub fn line_of<'a, 'b>(lines: &'b [TemplateLine<'a>], field: &Field) -> Option<&'b TemplateLine<'a>> {
    lines.iter().filter(|l| l.table == field.table && l.key == field.key).min_by_key(|l| l.commented)
}

/// The comment after a line's value (`key = "a#b" # hint` → `hint`): the first `#` outside a
/// string.
fn hint(body: &str) -> &str {
    let Some((_, value)) = body.split_once('=') else { return "" };
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in value.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return value[i + 1..].trim(),
            None => {}
        }
    }
    ""
}

/// A field's description: the comment after its value first, as a sentence (it is about this
/// key; the lines above may be about the ones below it too), then the comment lines above.
pub fn description(line: &TemplateLine<'_>) -> String {
    let mut description = line.above.join(" ");
    let hint = hint(line.body).trim_start_matches('#').trim();
    if !hint.is_empty() {
        let mut first = hint.to_owned();
        // A list of the values first (`list | grid | columns`) stays as they are written.
        let values = hint.split_once(" | ").is_some_and(|(before, _)| !before.contains(':'));
        if let Some(letter) = first.get_mut(..1).filter(|_| !values) {
            letter.make_ascii_uppercase();
        }
        if !description.is_empty() {
            let stop = if first.ends_with(['.', ')', ':']) { " " } else { ". " };
            first.push_str(stop);
        }
        description.insert_str(0, &first);
    }
    description
}
