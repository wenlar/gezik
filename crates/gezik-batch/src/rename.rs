//! Applies rename rules to names and checks the result.

use regex::{Regex, RegexBuilder};

use gezik_core::batch::case::{Lang, convert};
use gezik_core::batch::date::{DateParts, format as format_date};
use gezik_core::batch::rules::{CleanRule, ExtensionRule, NumberAt, NumberRule, Rule, RuleEntry};
use gezik_core::batch::template::{Piece, parse as parse_template, uses_taken};
use gezik_core::format_size;
use gezik_core::ops::names::{NameError, NameRules, split_name, validate_name};

/// One item to rename, as the layer knows it.
#[derive(Debug, Clone, Default)]
pub struct Item {
    pub name: String,
    pub is_dir: bool,
    /// The name of the folder it is in.
    pub parent: String,
    /// Which folder it is in (items in one folder share it): counting per folder, duplicates.
    pub folder: usize,
    pub size: u64,
    pub modified: Option<DateParts>,
    /// From EXIF, once read.
    pub taken: Option<DateParts>,
}

enum Ready {
    Replace { regex: Regex, with: String, all: bool },
    Number(NumberRule),
    Case(gezik_core::batch::case::CaseMode),
    AddText { prefix: Vec<Piece>, suffix: Vec<Piece> },
    Extension(ExtensionRule),
    Template(Vec<Piece>),
    Clean(CleanRule),
}

/// Rules made ready to apply; a rule that cannot be (a bad regex) has an error and is skipped.
pub struct Compiled {
    ready: Vec<Ready>,
    errors: Vec<Option<String>>,
    taken: bool,
}

impl Compiled {
    /// Per rule given to [`compile`]: what is wrong with it.
    pub fn errors(&self) -> &[Option<String>] {
        &self.errors
    }

    pub fn ok(&self) -> bool {
        self.errors.iter().all(Option::is_none)
    }

    /// Some rule uses `{taken}`: the EXIF dates must be read.
    pub fn needs_taken(&self) -> bool {
        self.taken
    }
}

pub fn compile(rules: &[RuleEntry]) -> Compiled {
    let mut compiled = Compiled { ready: Vec::new(), errors: Vec::new(), taken: false };
    for entry in rules {
        let ready = if entry.enabled { ready(&entry.rule) } else { Ok(None) };
        match ready {
            Ok(Some((ready, taken))) => {
                compiled.taken |= taken;
                compiled.ready.push(ready);
                compiled.errors.push(None);
            }
            Ok(None) => compiled.errors.push(None),
            Err(error) => compiled.errors.push(Some(error)),
        }
    }
    compiled
}

fn ready(rule: &Rule) -> Result<Option<(Ready, bool)>, String> {
    Ok(Some(match rule {
        Rule::Replace(r) => {
            if r.find.is_empty() {
                return Ok(None);
            }
            let pattern = if r.regex { r.find.clone() } else { regex::escape(&r.find) };
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!r.case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|err| last_line(&err.to_string()))?;
            // Plain text is put in as written: `$` means nothing there.
            let with = if r.regex { r.with.clone() } else { r.with.replace('$', "$$") };
            (Ready::Replace { regex, with, all: r.all }, false)
        }
        Rule::Number(n) => (Ready::Number(n.clone()), false),
        Rule::Case(mode) => (Ready::Case(*mode), false),
        Rule::AddText { prefix, suffix } => {
            let (prefix, suffix) = (parse_template(prefix)?, parse_template(suffix)?);
            let taken = uses_taken(&prefix) || uses_taken(&suffix);
            (Ready::AddText { prefix, suffix }, taken)
        }
        Rule::Extension(e) => (Ready::Extension(e.clone()), false),
        Rule::Template(text) => {
            let pieces = parse_template(text)?;
            let taken = uses_taken(&pieces);
            (Ready::Template(pieces), taken)
        }
        Rule::Clean(c) => (Ready::Clean(*c), false),
    }))
}

/// The regex crate's errors span lines with a caret drawing: the last line says what is wrong.
fn last_line(text: &str) -> String {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text)
        .trim()
        .trim_start_matches("error: ")
        .to_owned()
}

struct Context<'a> {
    item: &'a Item,
    counter: i64,
}

fn render(pieces: &[Piece], name: &str, ext: &str, cx: &Context<'_>) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Name => out.push_str(name),
            Piece::Ext => out.push_str(ext.trim_start_matches('.')),
            Piece::Number { width } => out.push_str(&number(cx.counter, *width)),
            Piece::Parent => out.push_str(&cx.item.parent),
            Piece::Modified(format) => {
                if let Some(date) = &cx.item.modified {
                    out.push_str(&format_date(format, date));
                }
            }
            Piece::Taken(format) => {
                if let Some(date) = cx.item.taken.as_ref().or(cx.item.modified.as_ref()) {
                    out.push_str(&format_date(format, date));
                }
            }
            Piece::Size => out.push_str(&format_size(cx.item.size)),
        }
    }
    out
}

fn number(n: i64, width: u8) -> String {
    let width = usize::from(width.clamp(1, 9));
    if n < 0 { format!("-{:0width$}", n.unsigned_abs()) } else { format!("{n:0width$}") }
}

fn clean(text: &str, rule: &CleanRule) -> String {
    let mut text: String = if rule.separators_to_spaces { text.replace(['_', '.', '-'], " ") } else { text.to_owned() };
    if rule.collapse_spaces {
        let mut out = String::with_capacity(text.len());
        let mut space = false;
        for c in text.chars() {
            if c == ' ' {
                if !space {
                    out.push(c);
                }
                space = true;
            } else {
                out.push(c);
                space = false;
            }
        }
        text = out;
    }
    if rule.trim {
        text = text.trim().to_owned();
    }
    if rule.spaces_to_underscores {
        text = text.replace(' ', "_");
    }
    text
}

/// Each item's new name, in order: `items` in the order the layer shows them.
pub fn new_names(items: &[Item], compiled: &Compiled, include_extension: bool, lang: Lang) -> Vec<String> {
    let mut seen: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    // `{n}` in templates counts like the first Number rule, else from 1 in each folder
    // (search results span folders; spec 2026-10-09-arama §batch rename).
    let first_number = compiled.ready.iter().find_map(|r| match r {
        Ready::Number(rule) => Some(rule),
        _ => None,
    });
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let count = seen.entry(item.folder).or_default();
        let in_folder = *count;
        *count += 1;
        let mut name = item.name.clone();
        for ready in &compiled.ready {
            let position = |rule: &NumberRule| if rule.per_folder { in_folder } else { index } as i64;
            let counter_of = |rule: &NumberRule| rule.start.saturating_add(rule.step.saturating_mul(position(rule)));
            let counter = match ready {
                Ready::Number(rule) => counter_of(rule),
                _ => first_number.map_or(1 + in_folder as i64, counter_of),
            };
            let cx = Context { item, counter };
            name = apply(ready, &name, item.is_dir, include_extension, lang, &cx);
        }
        out.push(name);
    }
    out
}

fn apply(ready: &Ready, name: &str, is_dir: bool, include_extension: bool, lang: Lang, cx: &Context<'_>) -> String {
    if let Ready::Extension(rule) = ready {
        let (stem, ext) = split_name(name, is_dir);
        if is_dir {
            return name.to_owned();
        }
        return match rule {
            ExtensionRule::Set(new) if new.trim_start_matches('.').is_empty() => stem.to_owned(),
            ExtensionRule::Set(new) => format!("{stem}.{}", new.trim_start_matches('.')),
            ExtensionRule::Lower => format!("{stem}{}", ext.to_lowercase()),
            ExtensionRule::Upper => format!("{stem}{}", ext.to_uppercase()),
        };
    }
    let (stem, ext) = if include_extension { (name, "") } else { split_name(name, is_dir) };
    let (_, real_ext) = split_name(name, is_dir);
    let changed = match ready {
        Ready::Replace { regex, with, all } => {
            if *all {
                regex.replace_all(stem, with.as_str()).into_owned()
            } else {
                regex.replace(stem, with.as_str()).into_owned()
            }
        }
        Ready::Number(rule) => {
            let n = number(cx.counter, rule.digits);
            match rule.at {
                NumberAt::Start => format!("{n}{}{stem}", rule.separator),
                NumberAt::End => format!("{stem}{}{n}", rule.separator),
            }
        }
        Ready::Case(mode) => convert(stem, *mode, lang),
        Ready::AddText { prefix, suffix } => {
            format!("{}{stem}{}", render(prefix, stem, real_ext, cx), render(suffix, stem, real_ext, cx))
        }
        Ready::Template(pieces) => render(pieces, stem, real_ext, cx),
        Ready::Clean(rule) => clean(stem, rule),
        Ready::Extension(_) => unreachable!("handled above"),
    };
    format!("{changed}{ext}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Changed,
    /// Another item gets the same new name.
    Duplicate,
    /// Something not being renamed already has this name.
    Exists,
    Invalid(NameError),
}

impl Status {
    /// Blocks the Rename button.
    pub fn blocks(&self) -> bool {
        matches!(self, Status::Duplicate | Status::Exists | Status::Invalid(_))
    }
}

/// What each new name means. `existing(folder, name)`: whether something outside the
/// selection in that folder has the name. `ignore_case`: the file system ignores case.
pub fn check(
    items: &[Item],
    new: &[String],
    existing: &dyn Fn(usize, &str) -> bool,
    rules: NameRules,
    ignore_case: bool,
) -> Vec<Status> {
    let key = |folder: usize, name: &str| (folder, if ignore_case { name.to_lowercase() } else { name.to_owned() });
    let mut counts: std::collections::HashMap<(usize, String), usize> = std::collections::HashMap::new();
    for (item, name) in items.iter().zip(new) {
        *counts.entry(key(item.folder, name)).or_default() += 1;
    }
    items
        .iter()
        .zip(new)
        .map(|(item, name)| {
            let duplicate = counts.get(&key(item.folder, name)).copied().unwrap_or(0) > 1;
            if *name == item.name {
                // Not renamed: its current name is not judged, but others may collide with it.
                return if duplicate { Status::Duplicate } else { Status::Unchanged };
            }
            if let Err(err) = validate_name(name, rules) {
                return Status::Invalid(err);
            }
            if duplicate {
                return Status::Duplicate;
            }
            let only_case = ignore_case && name.to_lowercase() == item.name.to_lowercase();
            if !only_case && existing(item.folder, name) {
                return Status::Exists;
            }
            Status::Changed
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::case::CaseMode;
    use gezik_core::batch::rules::ReplaceRule;

    fn items(names: &[&str]) -> Vec<Item> {
        names.iter().map(|n| Item { name: n.to_string(), parent: "Tatil".into(), ..Item::default() }).collect()
    }

    fn run(names: &[&str], rules: Vec<Rule>, include_extension: bool) -> Vec<String> {
        let entries: Vec<RuleEntry> = rules.into_iter().map(RuleEntry::new).collect();
        let compiled = compile(&entries);
        assert!(compiled.ok(), "{:?}", compiled.errors());
        new_names(&items(names), &compiled, include_extension, Lang::Turkic)
    }

    fn replace(find: &str, with: &str, regex: bool) -> Rule {
        Rule::Replace(ReplaceRule { find: find.into(), with: with.into(), regex, case_sensitive: false, all: true })
    }

    #[test]
    fn replace_then_number_then_case() {
        let rules = vec![
            replace("IMG_", "Tatil ", false),
            Rule::Number(NumberRule { digits: 2, separator: " - ".into(), ..NumberRule::default() }),
            Rule::Case(CaseMode::Upper),
        ];
        assert_eq!(
            run(&["IMG_0012.jpg", "img_0013.JPG"], rules, false),
            ["TATİL 0012 - 01.jpg", "TATİL 0013 - 02.JPG"]
        );
    }

    #[test]
    fn regex_groups_and_plain_dollars() {
        assert_eq!(
            run(&["2024_05_foto.png"], vec![replace(r"(\d+)_(\d+)", "$2-$1", true)], false),
            ["05-2024_foto.png"]
        );
        assert_eq!(run(&["a.txt"], vec![replace("a", "$1", false)], false), ["$1.txt"]);
    }

    #[test]
    fn the_extension_stays_unless_included() {
        assert_eq!(run(&["a.b.txt"], vec![replace(".", "_", false)], false), ["a_b.txt"]);
        assert_eq!(run(&["a.b.txt"], vec![replace(".", "_", false)], true), ["a_b_txt"]);
        assert_eq!(run(&["arsiv.tar.gz"], vec![Rule::Case(CaseMode::Upper)], false), ["ARSİV.tar.gz"]);
    }

    #[test]
    fn extension_rules() {
        let set = |e: &str| Rule::Extension(ExtensionRule::Set(e.into()));
        assert_eq!(run(&["a.JPEG"], vec![set("jpg")], false), ["a.jpg"]);
        assert_eq!(run(&["a.JPEG"], vec![set("")], false), ["a"]);
        assert_eq!(run(&["a.JPEG"], vec![Rule::Extension(ExtensionRule::Lower)], false), ["a.jpeg"]);
    }

    #[test]
    fn templates_and_add_text() {
        let date = DateParts { year: 2024, month: 7, day: 1, hour: 9, minute: 30, second: 0 };
        let mut list = items(&["IMG_1.jpg", "IMG_2.jpg"]);
        list[0].taken = Some(date);
        list[1].modified = Some(DateParts { day: 2, ..date });
        let entries = vec![
            RuleEntry::new(Rule::Template("{taken:%Y-%m-%d} {parent} {n:02}".into())),
            RuleEntry::new(Rule::AddText { prefix: "[".into(), suffix: "] {ext}".into() }),
        ];
        let compiled = compile(&entries);
        assert!(compiled.needs_taken());
        assert_eq!(
            new_names(&list, &compiled, false, Lang::Other),
            ["[2024-07-01 Tatil 01] jpg.jpg", "[2024-07-02 Tatil 02] jpg.jpg"]
        );
    }

    #[test]
    fn numbers_restart_per_folder_when_asked() {
        let mut list = items(&["a", "b", "c"]);
        list[2].folder = 1;
        let rule = NumberRule { per_folder: true, digits: 1, separator: "".into(), ..NumberRule::default() };
        let compiled = compile(&[RuleEntry::new(Rule::Number(rule))]);
        assert_eq!(new_names(&list, &compiled, false, Lang::Other), ["a1", "b2", "c1"]);
    }

    #[test]
    fn template_numbers_without_a_number_rule_restart_per_folder() {
        let mut list = items(&["a", "b", "c"]);
        list[1].folder = 1;
        let compiled = compile(&[RuleEntry::new(Rule::Template("doc {n}".into()))]);
        assert_eq!(new_names(&list, &compiled, false, Lang::Other), ["doc 1", "doc 1", "doc 2"]);
    }

    #[test]
    fn clean_up() {
        let rule = Rule::Clean(CleanRule {
            trim: true,
            collapse_spaces: true,
            separators_to_spaces: true,
            spaces_to_underscores: false,
        });
        assert_eq!(run(&["  my__holiday-photo .jpg"], vec![rule], false), ["my holiday photo.jpg"]);
    }

    #[test]
    fn unchecked_rules_are_skipped() {
        let mut entry = RuleEntry::new(Rule::Case(CaseMode::Upper));
        entry.enabled = false;
        let compiled = compile(&[entry]);
        assert_eq!(new_names(&items(&["a.txt"]), &compiled, false, Lang::Other), ["a.txt"]);
    }

    #[test]
    fn a_bad_rule_reports_and_changes_nothing() {
        let entries = vec![
            RuleEntry::new(replace("(", "x", true)),
            RuleEntry::new(Rule::Template("{nope}".into())),
            RuleEntry::new(Rule::Case(CaseMode::Upper)),
        ];
        let compiled = compile(&entries);
        assert!(!compiled.ok());
        assert!(compiled.errors()[0].as_deref().is_some_and(|e| !e.contains('\n')), "{:?}", compiled.errors());
        assert_eq!(compiled.errors()[1].as_deref(), Some("unknown field {nope}"));
        assert_eq!(compiled.errors()[2], None);
        // The good rules still show what they would do.
        assert_eq!(new_names(&items(&["a.txt"]), &compiled, false, Lang::Other), ["A.txt"]);
    }

    #[test]
    fn statuses() {
        let list = items(&["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]);
        let new: Vec<String> =
            ["x.txt", "x.txt", "taken.txt", "d.txt", "bad/name"].iter().map(|s| s.to_string()).collect();
        let statuses = check(&list, &new, &|_, name| name == "taken.txt", NameRules::Unix, false);
        assert_eq!(statuses[0], Status::Duplicate);
        assert_eq!(statuses[1], Status::Duplicate);
        assert_eq!(statuses[2], Status::Exists);
        assert_eq!(statuses[3], Status::Unchanged);
        assert!(matches!(statuses[4], Status::Invalid(_)));
        assert!(statuses[2].blocks() && !statuses[3].blocks());
    }

    #[test]
    fn unchanged_items_are_not_judged_but_can_collide() {
        let list = items(&["a.txt", "b.txt", "bad/name"]);
        let new: Vec<String> = ["a.txt", "a.txt", "bad/name"].iter().map(|s| s.to_string()).collect();
        let statuses = check(&list, &new, &|_, _| false, NameRules::Unix, false);
        assert_eq!(statuses, [Status::Duplicate, Status::Duplicate, Status::Unchanged]);
    }

    #[test]
    fn huge_numbers_saturate() {
        let rule =
            NumberRule { start: i64::MAX, step: i64::MAX, digits: 1, separator: "".into(), ..NumberRule::default() };
        let compiled = compile(&[RuleEntry::new(Rule::Number(rule))]);
        assert_eq!(new_names(&items(&["a", "b"]), &compiled, false, Lang::Other).len(), 2);
    }

    #[test]
    fn duplicates_ignore_case_where_the_system_does() {
        let list = items(&["a.jpg", "b.jpg", "c.jpg"]);
        let new: Vec<String> = ["Foto.jpg", "foto.jpg", "C.jpg"].iter().map(|s| s.to_string()).collect();
        // C.jpg exists as c.jpg: that is the item itself, only its case changes.
        let existing = |_: usize, name: &str| name.eq_ignore_ascii_case("c.jpg");
        let statuses = check(&list, &new, &existing, NameRules::Windows, true);
        assert_eq!(statuses, [Status::Duplicate, Status::Duplicate, Status::Changed]);
        let statuses = check(&list, &new, &|_, _| false, NameRules::Unix, false);
        assert_eq!(statuses, [Status::Changed, Status::Changed, Status::Changed]);
    }

    #[test]
    fn a_thousand_names_are_quick() {
        let names: Vec<String> = (0..1000).map(|n| format!("IMG_{n:04}.jpg")).collect();
        let list: Vec<Item> = names.iter().map(|n| Item { name: n.clone(), ..Item::default() }).collect();
        let entries = vec![
            RuleEntry::new(replace(r"IMG_(\d+)", "Tatil $1", true)),
            RuleEntry::new(Rule::Number(NumberRule::default())),
            RuleEntry::new(Rule::Case(CaseMode::Title)),
        ];
        let start = std::time::Instant::now();
        let compiled = compile(&entries);
        let new = new_names(&list, &compiled, false, Lang::Turkic);
        let _ = check(&list, &new, &|_, _| false, NameRules::Windows, true);
        // Debug builds are several times slower than release; 50 ms is the release target.
        assert!(start.elapsed() < std::time::Duration::from_millis(500), "{:?}", start.elapsed());
    }
}
