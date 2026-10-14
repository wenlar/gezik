//! Highlight rules (spec 12 §9): `[[highlight-rules]]` color and embolden rows, the first rule
//! that matches a row wins (Total Commander's color rules). Pure: the rules are compiled once
//! when the settings load (gezik-config), a row is tried with what its entry already holds
//! and, only for rules with a tag, rating or comment condition, its meta, and only for rules
//! with a `where`, its fields (spec 13 §9.3).

use std::cell::LazyCell;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use crate::Entry;
use crate::field_filter::{self, FieldTerm};
use crate::fields::{FieldSet, Fields};
use crate::file_filter::FileFilter;
use crate::meta::{Meta, TagColor};
use crate::meta_filter::{TagTerm, matches};
use crate::search::KindFilter;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    /// Rules compiled (none while rows are drawn).
    static COMPILED: Cell<usize> = const { Cell::new(0) };
    /// Rules tried (the first match ends the search).
    static TRIED: Cell<usize> = const { Cell::new(0) };
}

/// More than anyone writes by hand; bounds the time a row takes.
pub const MAX_RULES: usize = 64;

/// A rule's color: one of the eight tag colors by name, or `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hue {
    Named(TagColor),
    Rgb([u8; 3]),
}

impl Hue {
    /// `red`, `gray`…, `#2e7d32`.
    pub fn parse(text: &str) -> Option<Hue> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#') {
            if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            return Some(Hue::Rgb([byte(0)?, byte(2)?, byte(4)?]));
        }
        (1..8).map(TagColor::from_number).find(|c| c.default_name().eq_ignore_ascii_case(text)).map(Hue::Named)
    }

    /// The text color it makes: named colors have a deeper shade for light themes and a
    /// lighter one for dark themes (Finder's dot colors are hard to read as text on white).
    pub fn rgb(self, dark: bool) -> [u8; 3] {
        let color = match self {
            Hue::Rgb(rgb) => return rgb,
            Hue::Named(color) => color,
        };
        let (light, darkish) = match color {
            TagColor::None | TagColor::Gray => ([0x8e, 0x8e, 0x93], [0xae, 0xae, 0xb2]),
            TagColor::Green => ([0x24, 0x8a, 0x3d], [0x30, 0xd1, 0x58]),
            TagColor::Purple => ([0x89, 0x44, 0xab], [0xda, 0x8f, 0xff]),
            TagColor::Blue => ([0x00, 0x60, 0xd0], [0x40, 0x9c, 0xff]),
            TagColor::Yellow => ([0x9a, 0x6a, 0x00], [0xff, 0xd6, 0x0a]),
            TagColor::Red => ([0xd7, 0x00, 0x15], [0xff, 0x69, 0x61]),
            TagColor::Orange => ([0xb8, 0x5c, 0x00], [0xff, 0xb3, 0x40]),
        };
        if dark { darkish } else { light }
    }
}

/// What a matching rule does to a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub color: Option<Hue>,
    /// The dark theme's color; else `color`'s.
    pub dark: Option<Hue>,
    pub bold: bool,
}

impl Style {
    /// The text color in a light or dark theme; `None`: the theme's own.
    pub fn rgb(&self, dark: bool) -> Option<[u8; 3]> {
        let hue = if dark { self.dark.or(self.color) } else { self.color };
        hue.map(|h| h.rgb(dark))
    }
}

/// One rule: every condition it has must hold (and); one it lacks holds for every row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rule {
    pub name: Option<FileFilter>,
    pub kind: Option<KindFilter>,
    /// Modified within this long before now.
    pub newer: Option<Duration>,
    /// Modified longer than this before now.
    pub older: Option<Duration>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub hidden: Option<bool>,
    /// `tag`, `rating` and a `comment` text, as the filter bar's words.
    pub terms: Vec<TagTerm>,
    /// `comment = "any"`.
    pub commented: bool,
    /// `where`: property words, as the filter bar's.
    pub fields: Vec<FieldTerm>,
    pub style: Style,
}

impl Rule {
    pub fn needs_meta(&self) -> bool {
        !self.terms.is_empty() || self.commented
    }

    pub fn needs_fields(&self) -> bool {
        !self.fields.is_empty()
    }

    /// Whether `e` passes the conditions its entry answers.
    fn entry_matches(&self, e: &Entry, now: SystemTime) -> bool {
        let age = |time: Option<SystemTime>| time.map(|t| now.duration_since(t).unwrap_or_default());
        self.name.as_ref().is_none_or(|f| f.takes(&e.name, e.is_dir))
            && self.kind.is_none_or(|k| k.matches(&e.name, e.is_dir))
            && self.newer.is_none_or(|d| age(e.modified).is_some_and(|a| a <= d))
            && self.older.is_none_or(|d| age(e.modified).is_some_and(|a| a > d))
            && self.min_size.is_none_or(|n| e.known_size().is_some_and(|s| s >= n))
            && self.max_size.is_none_or(|n| e.known_size().is_some_and(|s| s <= n))
            && self.hidden.is_none_or(|h| h == (e.name.starts_with('.') || e.flags & Entry::HIDDEN != 0))
    }

    fn meta_matches(&self, meta: &Meta) -> bool {
        matches(&self.terms, meta) && (!self.commented || !meta.comment.is_empty())
    }
}

/// What is known of a row's meta.
#[derive(Debug, Clone, Copy)]
pub enum RowMeta<'a> {
    Known(&'a Meta),
    /// On its way: a rule asking for it waits (the row is drawn again when it comes), so the
    /// color of a later rule never flashes.
    Pending,
    /// Never read for this row (a cloud folder, the drives): rules asking for it do not match.
    Never,
}

/// The rules in effect, compiled.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Highlights {
    rules: Vec<Rule>,
}

impl Highlights {
    /// At most `MAX_RULES` (the settings warn about the rest).
    pub fn new(mut rules: Vec<Rule>) -> Highlights {
        rules.truncate(MAX_RULES);
        #[cfg(test)]
        COMPILED.with(|c| c.set(c.get() + rules.len()));
        Highlights { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Whether some rule needs rows' meta: only then is it read for them (spec 12 §3.4).
    pub fn needs_meta(&self) -> bool {
        self.rules.iter().any(Rule::needs_meta)
    }

    /// The fields some rule's `where` needs: only these are read for rows, and none when no
    /// rule has one.
    pub fn fields_needed(&self) -> FieldSet {
        self.rules.iter().fold(FieldSet::NONE, |set, r| set.with(field_filter::needed(&r.fields)))
    }

    /// The first rule matching `e` (its index), `now` the time the row is drawn, `fields` the
    /// row's fields (`None`: not read yet), asked for only when a `where` rule's other
    /// conditions hold; `None` when none does or one waits for the meta or the fields.
    pub fn first(
        &self,
        e: &Entry,
        now: SystemTime,
        meta: RowMeta,
        fields: impl FnOnce() -> Option<Rc<Fields>>,
    ) -> Option<usize> {
        let fields = LazyCell::new(fields);
        for (i, rule) in self.rules.iter().enumerate() {
            #[cfg(test)]
            TRIED.with(|c| c.set(c.get() + 1));
            if !rule.entry_matches(e, now) {
                continue;
            }
            if rule.needs_fields() {
                match &*fields {
                    None => return None,
                    Some(values) if !field_filter::matches(&rule.fields, values) => continue,
                    Some(_) => {}
                }
            }
            if !rule.needs_meta() {
                return Some(i);
            }
            match meta {
                RowMeta::Known(meta) if rule.meta_matches(meta) => return Some(i),
                RowMeta::Pending => return None,
                _ => {}
            }
        }
        None
    }
}

/// `7d`, `2w`, `3m` (30 days), `1y` (365 days), `12h`.
pub fn parse_age(text: &str) -> Result<Duration, String> {
    let text = text.trim();
    let bad = || gezik_i18n::t!("expected an age like \"7d\" (h, d, w, m, y), got \"{}\"", text);
    let unit = text.chars().last().ok_or_else(bad)?;
    let hours: u64 = match unit {
        'h' => 1,
        'd' => 24,
        'w' => 24 * 7,
        'm' => 24 * 30,
        'y' => 24 * 365,
        _ => return Err(bad()),
    };
    let n: u64 = text[..text.len() - 1].trim().parse().map_err(|_| bad())?;
    if n == 0 || n > 100 * 365 * 24 / hours {
        return Err(bad());
    }
    Ok(Duration::from_secs(n * hours * 3600))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Tag;

    const DAY: Duration = Duration::from_secs(86_400);

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + DAY * 20_000
    }

    fn file(name: &str, size: u64, days_old: u32) -> Entry {
        Entry {
            name: name.into(),
            is_dir: false,
            flags: 0,
            size,
            modified: Some(now() - DAY * days_old),
            created: None,
        }
    }

    fn folder(name: &str) -> Entry {
        Entry { is_dir: true, ..file(name, 0, 0) }
    }

    fn red(bold: bool) -> Style {
        Style { color: Some(Hue::Named(TagColor::Red)), dark: None, bold }
    }

    fn one(rule: Rule) -> Highlights {
        Highlights::new(vec![Rule { style: red(false), ..rule }])
    }

    fn hit(rules: &Highlights, e: &Entry) -> bool {
        rules.first(e, now(), RowMeta::Never, || None).is_some()
    }

    #[test]
    fn each_entry_condition() {
        let named = one(Rule { name: Some(FileFilter::parse("*.tmp ~* | ~keep*", false).unwrap()), ..Rule::default() });
        assert!(hit(&named, &file("a.TMP", 1, 0)) && hit(&named, &file("~x", 1, 0)));
        assert!(!hit(&named, &file("~keep.txt", 1, 0)), "| leaves out");
        assert!(!hit(&named, &folder("x.tmp")), "file patterns take no folder");
        let dirs = one(Rule { name: Some(FileFilter::parse("node_modules/", false).unwrap()), ..Rule::default() });
        assert!(hit(&dirs, &folder("node_modules")) && !hit(&dirs, &file("node_modules", 1, 0)));
        let not = one(Rule { name: Some(FileFilter::parse("| *.bak", false).unwrap()), ..Rule::default() });
        assert!(hit(&not, &folder("x")) && hit(&not, &file("a", 1, 0)) && !hit(&not, &file("a.bak", 1, 0)));

        let pictures = one(Rule { kind: Some(KindFilter::Pictures), ..Rule::default() });
        assert!(hit(&pictures, &file("a.jpg", 1, 0)) && !hit(&pictures, &file("a.txt", 1, 0)));
        let folders = one(Rule { kind: Some(KindFilter::Folders), ..Rule::default() });
        assert!(hit(&folders, &folder("d")) && !hit(&folders, &file("d", 1, 0)));

        let newer = one(Rule { newer: Some(DAY), ..Rule::default() });
        assert!(hit(&newer, &file("a", 1, 0)) && !hit(&newer, &file("a", 1, 2)));
        assert!(!hit(&newer, &Entry { modified: None, ..file("a", 1, 0) }), "no time: no match");
        let older = one(Rule { older: Some(DAY * 7), ..Rule::default() });
        assert!(hit(&older, &file("a", 1, 30)) && !hit(&older, &file("a", 1, 3)));

        let big = one(Rule { min_size: Some(100), max_size: Some(200), ..Rule::default() });
        assert!(hit(&big, &file("a", 150, 0)) && !hit(&big, &file("a", 99, 0)) && !hit(&big, &file("a", 201, 0)));
        assert!(!hit(&one(Rule { min_size: Some(0), ..Rule::default() }), &folder("d")), "size not known");

        let hidden = one(Rule { hidden: Some(true), ..Rule::default() });
        assert!(hit(&hidden, &file(".git", 1, 0)) && hit(&hidden, &Entry { flags: Entry::HIDDEN, ..file("a", 1, 0) }));
        assert!(!hit(&hidden, &file("a", 1, 0)));
        assert!(hit(&one(Rule { hidden: Some(false), ..Rule::default() }), &file("a", 1, 0)));
    }

    #[test]
    fn meta_conditions() {
        let tagged = Meta { tags: vec![Tag::new("Red", TagColor::Red)], rating: 3, comment: "Needs REVIEW".into() };
        let none = Meta::default();
        let check =
            |rule: Rule, meta: &Meta| one(rule).first(&file("a", 1, 0), now(), RowMeta::Known(meta), || None).is_some();
        let terms = |t: Vec<TagTerm>| Rule { terms: t, ..Rule::default() };
        assert!(check(terms(vec![TagTerm::Named("red".into())]), &tagged));
        assert!(!check(terms(vec![TagTerm::Named("Blue".into())]), &tagged));
        assert!(check(terms(vec![TagTerm::Any]), &tagged) && !check(terms(vec![TagTerm::Any]), &none));
        assert!(check(terms(vec![TagTerm::None]), &none) && !check(terms(vec![TagTerm::None]), &tagged));
        assert!(check(terms(vec![TagTerm::Unrated]), &none) && !check(terms(vec![TagTerm::Unrated]), &tagged));
        assert!(
            check(terms(vec![TagTerm::MinRating(3)]), &tagged) && !check(terms(vec![TagTerm::MinRating(4)]), &tagged)
        );
        assert!(check(terms(vec![TagTerm::Comment("review".into())]), &tagged));
        let any = Rule { commented: true, ..Rule::default() };
        assert!(check(any.clone(), &tagged) && !check(any, &none));
    }

    #[test]
    fn the_first_match_wins_with_its_color_and_boldness() {
        let rules = Highlights::new(vec![
            Rule { kind: Some(KindFilter::Pictures), style: red(true), ..Rule::default() },
            Rule { style: Style { color: Some(Hue::Rgb([1, 2, 3])), ..Style::default() }, ..Rule::default() },
        ]);
        assert_eq!(rules.first(&file("a.png", 1, 0), now(), RowMeta::Never, || None), Some(0));
        assert_eq!(rules.first(&file("a.txt", 1, 0), now(), RowMeta::Never, || None), Some(1));
        let style = rules.rules()[0].style;
        assert!(style.bold && style.rgb(false).is_some(), "both from the one rule");
    }

    #[test]
    fn a_meta_rule_waits_for_the_meta_and_is_passed_where_none_comes() {
        let rules = Highlights::new(vec![
            Rule { terms: vec![TagTerm::Any], style: red(false), ..Rule::default() },
            Rule { style: red(true), ..Rule::default() },
        ]);
        assert!(rules.needs_meta());
        let e = file("a", 1, 0);
        assert_eq!(rules.first(&e, now(), RowMeta::Pending, || None), None, "no later color meanwhile");
        assert_eq!(rules.first(&e, now(), RowMeta::Never, || None), Some(1));
        assert_eq!(rules.first(&e, now(), RowMeta::Known(&Meta::default()), || None), Some(1));
        let no_meta = Highlights::new(vec![Rule { style: red(false), ..Rule::default() }]);
        assert!(!no_meta.needs_meta() && !Highlights::default().needs_meta());
        assert_eq!(no_meta.first(&e, now(), RowMeta::Pending, || None), Some(0), "a rule without meta does not wait");
    }

    #[test]
    fn a_where_rule_matches_its_fields_and_waits_for_them() {
        use crate::fields::{ARTIST, DURATION, Value};
        let rules = Highlights::new(vec![
            Rule { fields: field_filter::parse_where("duration>3m").unwrap(), style: red(false), ..Rule::default() },
            Rule { style: red(true), ..Rule::default() },
        ]);
        assert_eq!(rules.fields_needed(), FieldSet::of(&[DURATION]));
        assert!(!rules.needs_meta(), "fields are not meta");
        let e = file("a.mp3", 1, 0);
        let long = Fields::new(vec![(DURATION, Value::Duration(200_000)), (ARTIST, Value::Text("x".into()))]);
        let short = Fields::new(vec![(DURATION, Value::Duration(60_000))]);
        assert_eq!(rules.first(&e, now(), RowMeta::Never, || Some(Rc::new(long.clone()))), Some(0));
        assert_eq!(rules.first(&e, now(), RowMeta::Never, || Some(Rc::new(short.clone()))), Some(1));
        assert_eq!(rules.first(&e, now(), RowMeta::Never, || Some(Rc::default())), Some(1), "no value: no match");
        assert_eq!(rules.first(&e, now(), RowMeta::Never, || None), None, "not read yet: no later color meanwhile");
        // Anded with the other conditions: a name that fails is passed without waiting.
        let named = Highlights::new(vec![Rule {
            name: Some(FileFilter::parse("*.mp3", false).unwrap()),
            fields: field_filter::parse_where("duration>3m").unwrap(),
            style: red(false),
            ..Rule::default()
        }]);
        let asked = Cell::new(false);
        let ask = || {
            asked.set(true);
            None
        };
        assert_eq!(named.first(&file("a.txt", 1, 0), now(), RowMeta::Never, ask), None);
        assert!(!asked.get(), "a row no where rule can take is not read");
        assert_eq!(named.first(&e, now(), RowMeta::Never, || Some(Rc::new(long.clone()))), Some(0));
        assert!(Highlights::new(vec![Rule { style: red(false), ..Rule::default() }]).fields_needed().is_empty());
        assert!(Highlights::default().fields_needed().is_empty(), "no where: nothing read");
    }

    #[test]
    fn compiled_once_tried_until_the_first_match() {
        let rules: Vec<Rule> = (0..70)
            .map(|i| Rule {
                name: Some(FileFilter::parse(&format!("x{i}"), false).unwrap()),
                style: red(false),
                ..Rule::default()
            })
            .collect();
        let rules = Highlights::new(rules);
        assert_eq!(rules.rules().len(), MAX_RULES);
        let compiled = COMPILED.with(Cell::get);
        TRIED.with(|c| c.set(0));
        assert_eq!(rules.first(&file("x2", 1, 0), now(), RowMeta::Never, || None), Some(2));
        assert_eq!(TRIED.with(Cell::get), 3, "stops at the first match");
        assert_eq!(COMPILED.with(Cell::get), compiled, "nothing compiled while drawing");
    }

    #[test]
    fn a_row_takes_well_under_a_microsecond_per_rule_set() {
        let rules: Vec<Rule> = (0..MAX_RULES)
            .map(|i| Rule {
                // Half the rules take the row by its name and fail on a `where` (spec 13 §9.3).
                name: Some(FileFilter::parse(if i % 2 == 0 { "*.txt" } else { "*.x" }, false).unwrap()),
                kind: Some(KindFilter::Files),
                min_size: Some(1),
                fields: if i % 2 == 0 {
                    field_filter::parse_where("duration>1h year<2000").unwrap()
                } else {
                    Vec::new()
                },
                style: red(false),
                ..Rule::default()
            })
            .collect();
        let rules = Highlights::new(rules);
        let e = file("some_long_file_name_2026.txt", 10, 0);
        let values = Rc::new(Fields::new(vec![(crate::fields::DURATION, crate::fields::Value::Duration(60_000))]));
        // The best of five batches: a shared CI runner stalls now and then.
        let per_row = (0..5)
            .map(|_| {
                let start = std::time::Instant::now();
                for _ in 0..200 {
                    assert_eq!(rules.first(&e, now(), RowMeta::Never, || Some(values.clone())), None);
                }
                start.elapsed() / 200
            })
            .min()
            .unwrap();
        // 1 µs a row in a release build (spec 12 §9.2); a debug build is many times slower, and
        // only a row that compiled or allocated per rule would reach a millisecond.
        assert!(per_row < Duration::from_micros(if cfg!(debug_assertions) { 1000 } else { 5 }), "{per_row:?}");
    }

    #[test]
    fn colors_and_ages_parse() {
        assert_eq!(Hue::parse("Red"), Some(Hue::Named(TagColor::Red)));
        assert_eq!(Hue::parse("#2e7d32"), Some(Hue::Rgb([0x2e, 0x7d, 0x32])));
        assert_eq!(Hue::parse("none"), None);
        assert_eq!(Hue::parse("#2e7d3"), None);
        assert_ne!(Hue::Named(TagColor::Yellow).rgb(false), Hue::Named(TagColor::Yellow).rgb(true), "a dark shade");
        assert_eq!(Hue::Rgb([1, 2, 3]).rgb(true), [1, 2, 3]);
        let style = Style { color: Some(Hue::Rgb([1, 1, 1])), dark: Some(Hue::Rgb([9, 9, 9])), bold: false };
        assert_eq!((style.rgb(false), style.rgb(true)), (Some([1, 1, 1]), Some([9, 9, 9])));
        assert_eq!(Style { bold: true, ..Style::default() }.rgb(true), None);
        assert_eq!(parse_age("1d"), Ok(DAY));
        assert_eq!(parse_age("2w"), Ok(DAY * 14));
        assert_eq!(parse_age("3m"), Ok(DAY * 90));
        assert_eq!(parse_age("1y"), Ok(DAY * 365));
        assert_eq!(parse_age("12h"), Ok(DAY / 2));
        assert!(parse_age("0d").is_err() && parse_age("d").is_err() && parse_age("5x").is_err());
        assert!(parse_age("").is_err() && parse_age("1000y").is_err());
    }
}
