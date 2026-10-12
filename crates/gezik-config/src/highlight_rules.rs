//! `[[highlight-rules]]` in settings.toml (spec 12 §9.1), compiled once as they load. A rule
//! with a mistake is left out with a warning (`highlight-rules[2]: …`); the others stay.

use gezik_core::file_filter::FileFilter;
use gezik_core::highlight::{Highlights, Hue, MAX_RULES, Rule, Style, parse_age};
use gezik_core::meta_filter::TagTerm;
use gezik_core::search::{KindFilter, parse_size};

use crate::Warning;

const KEYS: [&str; 13] = [
    "name",
    "kind",
    "newer-than",
    "older-than",
    "min-size",
    "max-size",
    "hidden",
    "tag",
    "rating",
    "comment",
    "color",
    "dark-color",
    "bold",
];

pub(crate) fn parse_highlight_rules(value: &toml::Value, file: &str, warnings: &mut Vec<Warning>) -> Highlights {
    let Some(items) = value.as_array() else {
        warnings.push(Warning::new(file, format!("highlight-rules: expected [[highlight-rules]] tables, got {value}")));
        return Highlights::default();
    };
    if items.len() > MAX_RULES {
        warnings.push(Warning::new(file, format!("highlight-rules: only the first {MAX_RULES} rules are used")));
    }
    let mut rules = Vec::new();
    for (i, item) in items.iter().enumerate().take(MAX_RULES) {
        match parse_rule(item) {
            Ok(rule) => rules.push(rule),
            Err(err) => warnings.push(Warning::new(file, format!("highlight-rules[{}]: {err}", i + 1))),
        }
    }
    Highlights::new(rules)
}

fn parse_rule(value: &toml::Value) -> Result<Rule, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    if let Some(key) = table.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("unknown key \"{key}\" (known: {})", KEYS.join(", ")));
    }
    let text = |key: &str| -> Result<Option<&str>, String> {
        table.get(key).map(|v| v.as_str().ok_or_else(|| format!("{key}: expected text, got {v}"))).transpose()
    };
    let field = |key: &str, e: String| format!("{key}: {e}");
    let hue = |key: &str| {
        text(key)?
            .map(|t| {
                Hue::parse(t).ok_or_else(|| {
                    format!("{key}: expected red, orange, yellow, green, blue, purple, gray or #rrggbb, got \"{t}\"")
                })
            })
            .transpose()
    };
    let mut rule = Rule {
        name: text("name")?.map(|t| FileFilter::parse(t, false).map_err(|e| field("name", e))).transpose()?,
        kind: text("kind")?.map(kind_of).transpose()?,
        newer: text("newer-than")?.map(|t| parse_age(t).map_err(|e| field("newer-than", e))).transpose()?,
        older: text("older-than")?.map(|t| parse_age(t).map_err(|e| field("older-than", e))).transpose()?,
        min_size: text("min-size")?.map(|t| parse_size(t).map_err(|e| field("min-size", e))).transpose()?,
        max_size: text("max-size")?.map(|t| parse_size(t).map_err(|e| field("max-size", e))).transpose()?,
        hidden: table
            .get("hidden")
            .map(|v| v.as_bool().ok_or_else(|| format!("hidden: expected true or false, got {v}")))
            .transpose()?,
        style: Style {
            color: hue("color")?,
            dark: hue("dark-color")?,
            bold: table
                .get("bold")
                .map(|v| v.as_bool().ok_or_else(|| format!("bold: expected true or false, got {v}")))
                .transpose()?
                .unwrap_or(false),
        },
        ..Rule::default()
    };
    if let Some(tag) = text("tag")? {
        rule.terms.push(match tag.trim() {
            "" => return Err("tag: empty (a tag's name, \"any\" or \"none\")".to_owned()),
            t if t.eq_ignore_ascii_case("any") => TagTerm::Any,
            t if t.eq_ignore_ascii_case("none") => TagTerm::None,
            t => TagTerm::Named(t.into()),
        });
    }
    if let Some(v) = table.get("rating") {
        rule.terms.push(match v.as_integer() {
            Some(0) => TagTerm::Unrated,
            Some(n @ 1..=5) => TagTerm::MinRating(n as u8),
            _ => return Err(format!("rating: expected 0 (not rated) to 5 (at least that many stars), got {v}")),
        });
    }
    match text("comment")?.map(str::trim) {
        None => {}
        Some("") => return Err("comment: empty (\"any\" or a text it holds)".to_owned()),
        Some(t) if t.eq_ignore_ascii_case("any") => rule.commented = true,
        Some(t) => rule.terms.push(TagTerm::Comment(t.into())),
    }
    if rule.style.color.is_none() && rule.style.dark.is_none() && !rule.style.bold {
        return Err("sets nothing (color, dark-color or bold = true)".to_owned());
    }
    Ok(rule)
}

/// The search's kinds, and `folder`/`file` as well as their plurals.
fn kind_of(text: &str) -> Result<KindFilter, String> {
    let text = text.trim();
    let plural = match text {
        "folder" => "folders",
        "file" => "files",
        t => t,
    };
    KindFilter::parse(plural).filter(|k| *k != KindFilter::Any).ok_or_else(|| {
        format!("kind: expected folder, file, pictures, videos, audio, documents, archives or code, got \"{text}\"")
    })
}

#[cfg(test)]
mod tests {
    use crate::Warning;
    use crate::settings::Settings;
    use gezik_core::Entry;
    use gezik_core::highlight::{Highlights, RowMeta};
    use gezik_core::meta::{Meta, TagColor};
    use std::time::{Duration, SystemTime};

    fn parse(text: &str) -> (Highlights, Vec<String>) {
        let mut warnings: Vec<Warning> = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        (settings.highlight_rules, warnings.into_iter().map(|w| w.message).collect())
    }

    #[test]
    fn reads_the_rules_of_the_spec() {
        let (rules, warnings) = parse(
            "[[highlight-rules]]\nname = \"*.tmp *.bak ~*\"\ncolor = \"gray\"\n\n\
             [[highlight-rules]]\ntag = \"Red\"\nbold = true\n\n\
             [[highlight-rules]]\nkind = \"folder\"\nnewer-than = \"1d\"\ncolor = \"#2e7d32\"\ndark-color = \"#81c784\"\n\n\
             [[highlight-rules]]\nmin-size = \"1 GB\"\nrating = 0\ncolor = \"orange\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(rules.rules().len(), 4);
        assert!(rules.needs_meta());
        let now = SystemTime::now();
        let entry = |name: &str, is_dir: bool, size: u64| Entry {
            name: name.into(),
            is_dir,
            flags: 0,
            size,
            modified: Some(now - Duration::from_secs(60)),
            created: None,
        };
        let none = Meta::default();
        let first = |e: &Entry| rules.first(e, now, RowMeta::Known(&none));
        assert_eq!(first(&entry("a.bak", false, 1)), Some(0));
        assert_eq!(first(&entry("new", true, 0)), Some(2));
        assert_eq!(first(&entry("big.iso", false, 2 << 30)), Some(3));
        assert_eq!(first(&entry("small.txt", false, 1)), None);
        let s = rules.rules()[2].style;
        assert_eq!((s.rgb(false), s.rgb(true)), (Some([0x2e, 0x7d, 0x32]), Some([0x81, 0xc7, 0x84])));
        assert_eq!(rules.rules()[0].style.color, Some(gezik_core::highlight::Hue::Named(TagColor::Gray)));
    }

    #[test]
    fn a_wrong_rule_is_left_out_with_a_warning_and_the_others_stay() {
        let (rules, warnings) = parse(
            "[[highlight-rules]]\nmin-size = \"lots\"\ncolor = \"red\"\n\n\
             [[highlight-rules]]\ncolor = \"red\"\nkind = \"pictures\"\n\n\
             [[highlight-rules]]\nkind = \"pictures\"\n\n\
             [[highlight-rules]]\ncolour = \"red\"\n\n\
             [[highlight-rules]]\nrating = 9\nbold = true\n\n\
             [[highlight-rules]]\ncolor = \"pink\"\n",
        );
        assert_eq!(rules.rules().len(), 1);
        assert!(warnings[0].starts_with("highlight-rules[1]: min-size: expected a size like"), "{warnings:?}");
        assert!(warnings[1].starts_with("highlight-rules[3]: sets nothing"), "{warnings:?}");
        assert!(warnings[2].starts_with("highlight-rules[4]: unknown key \"colour\""), "{warnings:?}");
        assert!(warnings[3].starts_with("highlight-rules[5]: rating:"), "{warnings:?}");
        assert!(warnings[4].starts_with("highlight-rules[6]: color:"), "{warnings:?}");
        assert_eq!(warnings.len(), 5);
    }

    #[test]
    fn at_most_64_rules() {
        let text = "[[highlight-rules]]\nbold = true\n".repeat(70);
        let (rules, warnings) = parse(&text);
        assert_eq!(rules.rules().len(), 64);
        assert_eq!(warnings, ["highlight-rules: only the first 64 rules are used"]);
        let (rules, warnings) = parse("");
        assert!(rules.is_empty() && warnings.is_empty(), "none by default");
    }
}
