//! Rename rules in TOML: `{ kind = "replace", find = "IMG_", with = "Tatil " }`. Used by
//! `[[rename-presets]]` in settings.toml and the last rules in state.toml.

use gezik_core::batch::case::CaseMode;
use gezik_core::batch::rules::{CleanRule, ExtensionRule, NumberAt, NumberRule, ReplaceRule, Rule, RuleEntry};

fn text(table: &toml::Table, key: &str) -> Result<String, String> {
    match table.get(key) {
        None => Ok(String::new()),
        Some(value) => value.as_str().map(str::to_owned).ok_or_else(|| format!("{key}: expected text, got {value}")),
    }
}

fn flag(table: &toml::Table, key: &str, default: bool) -> Result<bool, String> {
    match table.get(key) {
        None => Ok(default),
        Some(value) => value.as_bool().ok_or_else(|| format!("{key}: expected true or false, got {value}")),
    }
}

fn int(table: &toml::Table, key: &str, default: i64) -> Result<i64, String> {
    match table.get(key) {
        None => Ok(default),
        Some(value) => value.as_integer().ok_or_else(|| format!("{key}: expected a number, got {value}")),
    }
}

pub fn rule_from_toml(table: &toml::Table) -> Result<RuleEntry, String> {
    let kind = text(table, "kind")?;
    let rule = match kind.as_str() {
        "replace" => Rule::Replace(ReplaceRule {
            find: text(table, "find")?,
            with: text(table, "with")?,
            regex: flag(table, "regex", false)?,
            case_sensitive: flag(table, "case-sensitive", false)?,
            all: flag(table, "all", true)?,
        }),
        "number" => {
            let digits = int(table, "digits", 3)?;
            let at = match text(table, "at")?.as_str() {
                "" | "end" => NumberAt::End,
                "start" => NumberAt::Start,
                other => return Err(format!("at: expected \"start\" or \"end\", got \"{other}\"")),
            };
            Rule::Number(NumberRule {
                start: int(table, "start", 1)?,
                step: int(table, "step", 1)?,
                digits: u8::try_from(digits).ok().filter(|d| (1..=9).contains(d)).ok_or("digits: expected 1 to 9")?,
                at,
                separator: if table.contains_key("separator") { text(table, "separator")? } else { " ".to_owned() },
                per_folder: flag(table, "per-folder", false)?,
            })
        }
        "case" => {
            let mode = text(table, "mode")?;
            Rule::Case(CaseMode::parse(&mode).ok_or_else(|| {
                format!("mode: expected \"lower\", \"upper\", \"title\" or \"sentence\", got \"{mode}\"")
            })?)
        }
        "add-text" => Rule::AddText { prefix: text(table, "prefix")?, suffix: text(table, "suffix")? },
        "extension" => match text(table, "mode")?.as_str() {
            "set" => Rule::Extension(ExtensionRule::Set(text(table, "text")?)),
            "lower" => Rule::Extension(ExtensionRule::Lower),
            "upper" => Rule::Extension(ExtensionRule::Upper),
            other => return Err(format!("mode: expected \"set\", \"lower\" or \"upper\", got \"{other}\"")),
        },
        "template" => Rule::Template(text(table, "text")?),
        "clean" => Rule::Clean(CleanRule {
            trim: flag(table, "trim", true)?,
            collapse_spaces: flag(table, "collapse-spaces", true)?,
            separators_to_spaces: flag(table, "separators-to-spaces", false)?,
            spaces_to_underscores: flag(table, "spaces-to-underscores", false)?,
        }),
        "" => return Err("kind is missing".to_owned()),
        other => return Err(format!("kind: unknown rule \"{other}\"")),
    };
    Ok(RuleEntry { enabled: flag(table, "enabled", true)?, rule })
}

pub fn rule_to_toml(entry: &RuleEntry) -> toml::Table {
    let mut t = toml::Table::new();
    let mut put = |key: &str, value: toml::Value| {
        t.insert(key.to_owned(), value);
    };
    let s = |text: &str| toml::Value::String(text.to_owned());
    put("kind", s(entry.rule.kind_name()));
    match &entry.rule {
        Rule::Replace(r) => {
            put("find", s(&r.find));
            put("with", s(&r.with));
            put("regex", toml::Value::Boolean(r.regex));
            put("case-sensitive", toml::Value::Boolean(r.case_sensitive));
            put("all", toml::Value::Boolean(r.all));
        }
        Rule::Number(n) => {
            put("start", toml::Value::Integer(n.start));
            put("step", toml::Value::Integer(n.step));
            put("digits", toml::Value::Integer(n.digits.into()));
            put("at", s(if n.at == NumberAt::Start { "start" } else { "end" }));
            put("separator", s(&n.separator));
            put("per-folder", toml::Value::Boolean(n.per_folder));
        }
        Rule::Case(mode) => put("mode", s(mode.as_str())),
        Rule::AddText { prefix, suffix } => {
            put("prefix", s(prefix));
            put("suffix", s(suffix));
        }
        Rule::Extension(ExtensionRule::Set(ext)) => {
            put("mode", s("set"));
            put("text", s(ext));
        }
        Rule::Extension(ExtensionRule::Lower) => put("mode", s("lower")),
        Rule::Extension(ExtensionRule::Upper) => put("mode", s("upper")),
        Rule::Template(text) => put("text", s(text)),
        Rule::Clean(c) => {
            put("trim", toml::Value::Boolean(c.trim));
            put("collapse-spaces", toml::Value::Boolean(c.collapse_spaces));
            put("separators-to-spaces", toml::Value::Boolean(c.separators_to_spaces));
            put("spaces-to-underscores", toml::Value::Boolean(c.spaces_to_underscores));
        }
    }
    if !entry.enabled {
        put("enabled", toml::Value::Boolean(false));
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::rules::KINDS;

    #[test]
    fn every_kind_survives_a_round_trip() {
        for (kind, _) in KINDS {
            let mut entry = RuleEntry::new(Rule::default_of(kind).unwrap());
            entry.enabled = kind != "case";
            assert_eq!(rule_from_toml(&rule_to_toml(&entry)).unwrap(), entry, "{kind}");
        }
    }

    #[test]
    fn errors_name_the_key() {
        let table: toml::Table = "kind = \"case\"\nmode = \"loud\"".parse().unwrap();
        assert!(rule_from_toml(&table).unwrap_err().starts_with("mode:"));
        let table: toml::Table = "kind = \"nope\"".parse().unwrap();
        assert_eq!(rule_from_toml(&table).unwrap_err(), "kind: unknown rule \"nope\"");
        let table: toml::Table = "kind = \"number\"\ndigits = 12".parse().unwrap();
        assert_eq!(rule_from_toml(&table).unwrap_err(), "digits: expected 1 to 9");
    }
}
