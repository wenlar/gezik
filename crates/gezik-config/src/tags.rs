//! `[tags]` (spec 12 §4.5): the user's tags in order (the first seven have keys), whether tags
//! go into the files at all, and when tag dots show beside names.

use gezik_core::meta::{Tag, TagColor, check_tag_name, same_name};
use gezik_i18n::t;

use crate::Warning;

/// At most this many tags in the list (menu ids 2410–2469, spec 12 §12.3).
pub const TAGS_MAX: usize = 60;

/// The colour names `[tags] list` takes, in Finder's numbers' order.
const COLORS: [(&str, TagColor); 8] = [
    ("none", TagColor::None),
    ("gray", TagColor::Gray),
    ("green", TagColor::Green),
    ("purple", TagColor::Purple),
    ("blue", TagColor::Blue),
    ("yellow", TagColor::Yellow),
    ("red", TagColor::Red),
    ("orange", TagColor::Orange),
];

/// When tag dots show beside names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dots {
    /// macOS: on; elsewhere on once a tag was written (`state.toml` `[tags] used`).
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagsSettings {
    pub list: Vec<Tag>,
    /// false: every tag in Gezik's own store, never in the files.
    pub write_to_files: bool,
    pub dots: Dots,
}

impl Default for TagsSettings {
    fn default() -> Self {
        let list = [
            TagColor::Red,
            TagColor::Orange,
            TagColor::Yellow,
            TagColor::Green,
            TagColor::Blue,
            TagColor::Purple,
            TagColor::Gray,
        ]
        .map(|color| Tag::new(color.default_name(), color))
        .into();
        TagsSettings { list, write_to_files: true, dots: Dots::Auto }
    }
}

/// The name `[tags] list` writes for `color`.
pub fn color_name(color: TagColor) -> &'static str {
    COLORS.iter().find(|(_, c)| *c == color).map_or("none", |(name, _)| name)
}

/// `[tags]`; a bad key or list entry is reported and left out.
pub fn parse(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> TagsSettings {
    let mut tags = TagsSettings::default();
    let mut warn = |text: String| warnings.push(Warning::new(file, format!("tags.{text}")));
    if let Some(value) = table.get("list") {
        match value.as_array() {
            None => warn(t!("{0}: expected a list of {{ name, color }}, got {1}", "list", value)),
            Some(items) => {
                tags.list.clear();
                for (i, item) in items.iter().enumerate() {
                    match parse_tag(item) {
                        Ok(tag) if tags.list.iter().any(|t| same_name(&t.name, &tag.name)) => {
                            warn(t!(
                                "{0}: \"{1}\" is already in the list; left out",
                                format!("list[{}]", i + 1),
                                tag.name
                            ));
                        }
                        Ok(_) if tags.list.len() == TAGS_MAX => {
                            warn(t!("{0}: at most {1} tags; left out", format!("list[{}]", i + 1), TAGS_MAX));
                        }
                        Ok(tag) => tags.list.push(tag),
                        Err(err) => warn(format!("list[{}]: {err}", i + 1)),
                    }
                }
            }
        }
    }
    if let Some(value) = table.get("write-to-files") {
        match value.as_bool() {
            Some(on) => tags.write_to_files = on,
            None => warn(t!("{0}: expected true or false, got {1}", "write-to-files", value)),
        }
    }
    if let Some(value) = table.get("dots") {
        match (value.as_bool(), value.as_str()) {
            (Some(true), _) | (_, Some("on" | "true")) => tags.dots = Dots::On,
            (Some(false), _) | (_, Some("off" | "false")) => tags.dots = Dots::Off,
            (_, Some("auto")) => tags.dots = Dots::Auto,
            _ => warn(t!("{0}: expected \"auto\", true or false, got {1}", "dots", value)),
        }
    }
    tags
}

/// One `{ name = "Red", color = "red" }`; a bare `"Red"` has no colour.
fn parse_tag(value: &toml::Value) -> Result<Tag, String> {
    if let Some(name) = value.as_str() {
        check_tag_name(name).map_err(|err| format!("name: {}", t("", err)))?;
        return Ok(Tag::new(name, TagColor::None));
    }
    let table = value.as_table().ok_or_else(|| t!("expected {{ name, color }}, got {}", value))?;
    if let Some(key) = table.keys().find(|key| !matches!(key.as_str(), "name" | "color")) {
        return Err(t!("unknown key \"{}\"", key));
    }
    let name = table.get("name").ok_or_else(|| t!("{} is missing", "name"))?;
    let name = name.as_str().ok_or_else(|| t!("{0}: expected text, got {1}", "name", name))?;
    check_tag_name(name).map_err(|err| format!("name: {}", t("", err)))?;
    let color = match table.get("color") {
        None => TagColor::None,
        Some(value) => value
            .as_str()
            .and_then(|text| COLORS.iter().find(|(n, _)| text.eq_ignore_ascii_case(n)))
            .map(|(_, color)| *color)
            .ok_or_else(|| {
                t!("{0}: expected red, orange, yellow, green, blue, purple, gray or none, got {1}", "color", value)
            })?,
    };
    Ok(Tag::new(name, color))
}

/// `list` as `[tags] list` writes it: one inline table per tag.
pub fn list_value(list: &[Tag]) -> toml_edit::Value {
    let mut array = toml_edit::Array::new();
    for tag in list {
        let mut entry = toml_edit::InlineTable::new();
        entry.insert("name", (*tag.name).into());
        entry.insert("color", color_name(tag.color).into());
        array.push(entry);
    }
    // One tag a line, as the template has them.
    for item in array.iter_mut() {
        item.decor_mut().set_prefix("\n  ");
    }
    array.set_trailing("\n");
    array.set_trailing_comma(true);
    toml_edit::Value::Array(array)
}

/// A change to `[tags] list` made from the sidebar's menu (spec 12 §7.3); files are not touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagListEdit {
    /// The tag called `.0` is called `.1`.
    Rename(Box<str>, Box<str>),
    Color(Box<str>, TagColor),
    /// One place up (`true`) or down.
    Move(Box<str>, bool),
    Remove(Box<str>),
}

impl TagListEdit {
    /// The name of the tag it changes.
    pub fn name(&self) -> &str {
        match self {
            TagListEdit::Rename(name, _)
            | TagListEdit::Color(name, _)
            | TagListEdit::Move(name, _)
            | TagListEdit::Remove(name) => name,
        }
    }
}

/// `list` with `edit` made; the same list when it names no tag there (or moves the first up).
pub fn edited(list: &[Tag], edit: &TagListEdit) -> Vec<Tag> {
    let mut list = list.to_vec();
    let Some(at) = list.iter().position(|t| same_name(&t.name, edit.name())) else { return list };
    match edit {
        TagListEdit::Rename(_, to) => list[at].name = to.clone(),
        TagListEdit::Color(_, color) => list[at].color = *color,
        TagListEdit::Move(_, up) => {
            let other = if *up { at.checked_sub(1) } else { Some(at + 1).filter(|n| *n < list.len()) };
            if let Some(other) = other {
                list.swap(at, other);
            }
        }
        TagListEdit::Remove(_) => {
            list.remove(at);
        }
    }
    list
}

/// The tag name of a `[tags] list` entry as written (`"Red"` or `{ name = "Red", … }`).
pub fn entry_name(value: &toml_edit::Value) -> Option<&str> {
    match value {
        toml_edit::Value::String(name) => Some(name.value()),
        toml_edit::Value::InlineTable(table) => table.get("name")?.as_str(),
        _ => None,
    }
}

/// `dots` as written: `"auto"`, true or false.
pub fn dots_value(dots: Dots) -> toml_edit::Value {
    match dots {
        Dots::Auto => "auto".into(),
        Dots::On => true.into(),
        Dots::Off => false.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> (TagsSettings, Vec<String>) {
        let table: toml::Table = text.parse().unwrap();
        let mut warnings = Vec::new();
        let tags = parse(&table, "settings.toml", &mut warnings);
        (tags, warnings.into_iter().map(|w| w.to_string()).collect())
    }

    #[test]
    fn list_edits_rename_recolour_move_and_remove() {
        let list = TagsSettings::default().list;
        let names = |list: &[Tag]| list.iter().map(|t| t.name.to_string()).collect::<Vec<_>>().join(",");
        let renamed = edited(&list, &TagListEdit::Rename("red".into(), "Urgent".into()));
        assert_eq!((&*renamed[0].name, renamed[0].color), ("Urgent", TagColor::Red), "the colour stays");
        assert_eq!(edited(&list, &TagListEdit::Color("Gray".into(), TagColor::None))[6].color, TagColor::None);
        assert_eq!(names(&edited(&list, &TagListEdit::Move("Orange".into(), true)))[..10], *"Orange,Red");
        assert_eq!(edited(&list, &TagListEdit::Move("Red".into(), true)), list, "the first stays first");
        assert_eq!(edited(&list, &TagListEdit::Move("Gray".into(), false)), list, "the last stays last");
        assert_eq!(edited(&list, &TagListEdit::Remove("Blue".into())).len(), 6);
        assert_eq!(edited(&list, &TagListEdit::Remove("Nope".into())), list);
    }

    #[test]
    fn the_default_list_is_finders_seven_colours() {
        let tags = TagsSettings::default();
        let names: Vec<&str> = tags.list.iter().map(|t| &*t.name).collect();
        assert_eq!(names, ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"]);
        assert!(tags.list.iter().all(|t| t.color != TagColor::None));
        assert_eq!((tags.write_to_files, tags.dots), (true, Dots::Auto));
        assert_eq!(read("").0, tags, "no [tags]: the defaults");
    }

    #[test]
    fn a_list_reads_and_bad_entries_are_left_out() {
        let (tags, warnings) = read(
            r#"list = [
                { name = "Work", color = "Blue" },
                { name = "a,b", color = "red" },
                { name = "work", color = "red" },
                "Project X",
                { name = "Late", color = "pink" },
                { name = "Odd", colour = "red" },
            ]
            write-to-files = false
            dots = true"#,
        );
        let got: Vec<(&str, TagColor)> = tags.list.iter().map(|t| (&*t.name, t.color)).collect();
        assert_eq!(got, [("Work", TagColor::Blue), ("Project X", TagColor::None)]);
        assert!(!tags.write_to_files);
        assert_eq!(tags.dots, Dots::On);
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        assert!(warnings[0].contains("tags.list[2]: name: a tag name cannot hold a comma"), "{warnings:?}");
        assert!(warnings[1].contains("tags.list[3]: \"work\" is already in the list"), "{warnings:?}");
        assert!(warnings[2].contains("tags.list[5]: color: expected red"), "{warnings:?}");
        assert!(warnings[3].contains("unknown key \"colour\""), "{warnings:?}");
    }

    #[test]
    fn dots_take_auto_true_false_on_and_off() {
        for (text, want) in [
            ("\"auto\"", Dots::Auto),
            ("true", Dots::On),
            ("false", Dots::Off),
            ("\"on\"", Dots::On),
            ("\"off\"", Dots::Off),
        ] {
            assert_eq!(read(&format!("dots = {text}")).0.dots, want, "{text}");
        }
        let (tags, warnings) = read("dots = 3\nwrite-to-files = \"no\"");
        assert_eq!((tags.dots, tags.write_to_files), (Dots::Auto, true));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn the_list_stops_at_sixty() {
        let entries: Vec<String> = (0..62).map(|i| format!("\"T{i}\"")).collect();
        let (tags, warnings) = read(&format!("list = [{}]", entries.join(", ")));
        assert_eq!(tags.list.len(), TAGS_MAX);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn a_written_list_reads_back() {
        let list = vec![Tag::new("Öncelikli", TagColor::Red), Tag::new("Project X", TagColor::None)];
        let text = format!("list = {}\ndots = {}\n", list_value(&list), dots_value(Dots::Off));
        let (tags, warnings) = read(&text);
        assert!(warnings.is_empty(), "{warnings:?}\n{text}");
        assert_eq!(tags.list, list);
        assert_eq!(tags.dots, Dots::Off);
        assert!(text.contains("\n  { name = \"Öncelikli\", color = \"red\" },\n"), "{text}");
    }
}
