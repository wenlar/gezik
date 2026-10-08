//! What a search looks for (spec 3.1): pure data the history, the session and saved searches
//! carry; `gezik-search` compiles and runs it. Also the text forms of its parts (`500 MB`,
//! `7d`, `videos`) that state.toml and settings.toml share.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::batch::date::DateParts;
use crate::kind::Kind;

/// Where a search looks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    Folder(PathBuf),
    /// Every local fixed drive ("This PC").
    AllDrives,
}

impl Scope {
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Scope::Folder(path) => Some(path),
            Scope::AllDrives => None,
        }
    }
}

/// Bytes, both ends optional and inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SizeRange {
    pub min: Option<u64>,
    pub max: Option<u64>,
}

impl SizeRange {
    pub fn is_any(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }

    pub fn contains(&self, size: u64) -> bool {
        self.min.is_none_or(|min| size >= min) && self.max.is_none_or(|max| size <= max)
    }
}

/// A calendar day (local).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Day {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Day {
    /// `YYYY-MM-DD`, a day that exists.
    pub fn parse(text: &str) -> Option<Day> {
        let mut parts = text.split('-');
        let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || year.len() != 4 || month.len() != 2 || day.len() != 2 {
            return None;
        }
        let day = Day { year: year.parse().ok()?, month: month.parse().ok()?, day: day.parse().ok()? };
        (day.month >= 1 && day.month <= 12 && day.day >= 1 && day.day <= days_in_month(day.year, day.month))
            .then_some(day)
    }

    pub fn text(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Days since 1970-01-01.
    fn number(self) -> i64 {
        crate::view::day_number(&DateParts {
            year: self.year,
            month: self.month,
            day: self.day,
            ..DateParts::default()
        })
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The local day of a local time.
pub fn day_of(local: &DateParts) -> Day {
    Day { year: local.year, month: local.month, day: local.day }
}

/// Local minus UTC, in seconds (rounded to the minute): `local` is what the clock shows at `now`.
pub fn utc_offset(now: SystemTime, local: &DateParts) -> i64 {
    let local_secs = crate::view::day_number(local) * 86_400
        + i64::from(local.hour) * 3600
        + i64::from(local.minute) * 60
        + i64::from(local.second);
    let utc_secs = match now.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    };
    ((local_secs - utc_secs) as f64 / 60.0).round() as i64 * 60
}

/// The most days "last N days" may say.
pub const MAX_LAST_DAYS: u32 = 36_500;

/// When an item was modified (spec 3.1); counted from the moment the search starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DateRange {
    #[default]
    Any,
    Today,
    LastDays(u32),
    ThisYear,
    /// Both whole days, local time.
    Between(Day, Day),
}

/// Times from `from` (inclusive) to `to` (exclusive); `None`: no bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    pub from: Option<SystemTime>,
    pub to: Option<SystemTime>,
}

impl TimeWindow {
    pub const ALL: TimeWindow = TimeWindow { from: None, to: None };

    /// Whether `time` is in; an item without a time only when there are no bounds.
    pub fn contains(&self, time: Option<SystemTime>) -> bool {
        match time {
            None => self.from.is_none() && self.to.is_none(),
            Some(time) => self.from.is_none_or(|from| time >= from) && self.to.is_none_or(|to| time < to),
        }
    }
}

fn at_secs(secs: i64) -> SystemTime {
    if secs >= 0 {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        SystemTime::UNIX_EPOCH - Duration::from_secs(secs.unsigned_abs())
    }
}

impl DateRange {
    /// The times it lets through: `now`, `today` its local day, `offset` local minus UTC.
    pub fn window(&self, now: SystemTime, today: Day, offset: i64) -> TimeWindow {
        let midnight = |day: Day| at_secs(day.number() * 86_400 - offset);
        match *self {
            DateRange::Any => TimeWindow::ALL,
            DateRange::Today => TimeWindow { from: Some(midnight(today)), to: None },
            DateRange::LastDays(n) => TimeWindow {
                from: Some(
                    now.checked_sub(Duration::from_secs(u64::from(n) * 86_400)).unwrap_or(SystemTime::UNIX_EPOCH),
                ),
                to: None,
            },
            DateRange::ThisYear => TimeWindow { from: Some(midnight(Day { month: 1, day: 1, ..today })), to: None },
            DateRange::Between(a, b) => {
                let (first, last) = if a <= b { (a, b) } else { (b, a) };
                TimeWindow { from: Some(midnight(first)), to: Some(at_secs((last.number() + 1) * 86_400 - offset)) }
            }
        }
    }

    /// `today`, `7d`, `year`, `2026-01-01..2026-06-30`; `""` and `any`: any time.
    pub fn parse(text: &str) -> Result<DateRange, String> {
        let text = text.trim();
        let bad = || format!("expected \"today\", \"7d\", \"year\" or \"2026-01-01..2026-06-30\", got \"{text}\"");
        match text {
            "" | "any" => return Ok(DateRange::Any),
            "today" => return Ok(DateRange::Today),
            "year" => return Ok(DateRange::ThisYear),
            _ => {}
        }
        if let Some(days) = text.strip_suffix('d') {
            let days: u32 = days.parse().map_err(|_| bad())?;
            return if (1..=MAX_LAST_DAYS).contains(&days) { Ok(DateRange::LastDays(days)) } else { Err(bad()) };
        }
        let (first, last) = text.split_once("..").ok_or_else(bad)?;
        match (Day::parse(first), Day::parse(last)) {
            (Some(first), Some(last)) => Ok(DateRange::Between(first, last)),
            _ => Err(bad()),
        }
    }

    /// As `parse` reads it.
    pub fn text(&self) -> String {
        match self {
            DateRange::Any => "any".to_owned(),
            DateRange::Today => "today".to_owned(),
            DateRange::LastDays(n) => format!("{n}d"),
            DateRange::ThisYear => "year".to_owned(),
            DateRange::Between(a, b) => format!("{}..{}", a.text(), b.text()),
        }
    }

    /// Its line in the Filters menu.
    pub fn label(&self) -> String {
        match self {
            DateRange::Any => "Any time".to_owned(),
            DateRange::Today => "Today".to_owned(),
            DateRange::LastDays(n) => format!("Last {n} days"),
            DateRange::ThisYear => "This year".to_owned(),
            DateRange::Between(a, b) => format!("{} – {}", a.text(), b.text()),
        }
    }
}

/// What kind of item (spec 3.1), mapped to the icon kinds (`Kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KindFilter {
    #[default]
    Any,
    Files,
    Folders,
    Pictures,
    Videos,
    Audio,
    Documents,
    Archives,
    Code,
}

impl KindFilter {
    /// In menu order (the menu ids follow it).
    pub const ALL: [KindFilter; 9] = [
        KindFilter::Any,
        KindFilter::Files,
        KindFilter::Folders,
        KindFilter::Pictures,
        KindFilter::Videos,
        KindFilter::Audio,
        KindFilter::Documents,
        KindFilter::Archives,
        KindFilter::Code,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            KindFilter::Any => "any",
            KindFilter::Files => "files",
            KindFilter::Folders => "folders",
            KindFilter::Pictures => "pictures",
            KindFilter::Videos => "videos",
            KindFilter::Audio => "audio",
            KindFilter::Documents => "documents",
            KindFilter::Archives => "archives",
            KindFilter::Code => "code",
        }
    }

    pub fn parse(text: &str) -> Option<KindFilter> {
        KindFilter::ALL.into_iter().find(|k| k.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            KindFilter::Any => "Any type",
            KindFilter::Files => "Files",
            KindFilter::Folders => "Folders",
            KindFilter::Pictures => "Pictures",
            KindFilter::Videos => "Videos",
            KindFilter::Audio => "Audio",
            KindFilter::Documents => "Documents",
            KindFilter::Archives => "Archives",
            KindFilter::Code => "Code",
        }
    }

    /// Whether an item called `name` is of this kind. The file kinds take no folder.
    pub fn matches(self, name: &str, is_dir: bool) -> bool {
        match self {
            KindFilter::Any => true,
            KindFilter::Files => !is_dir,
            KindFilter::Folders => is_dir,
            _ if is_dir => false,
            other => {
                let kind = Kind::of(name, false);
                match other {
                    KindFilter::Pictures => kind == Kind::Image,
                    KindFilter::Videos => kind == Kind::Video,
                    KindFilter::Audio => kind == Kind::Audio,
                    KindFilter::Documents => {
                        matches!(kind, Kind::Document | Kind::Spreadsheet | Kind::Presentation | Kind::Pdf | Kind::Text)
                    }
                    KindFilter::Archives => matches!(kind, Kind::Archive | Kind::DiskImage),
                    KindFilter::Code => kind == Kind::Code,
                    KindFilter::Any | KindFilter::Files | KindFilter::Folders => true,
                }
            }
        }
    }
}

/// Hidden and system items (spec 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HiddenRule {
    /// As `[view] show-hidden` / `show-system` show them in a folder.
    #[default]
    FollowView,
    /// Every item.
    Include,
}

/// A search, or the flat view (spec 3.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchSpec {
    pub scope: Scope,
    /// The pattern language of the filter, or a regular expression (`name_regex`).
    pub pattern: String,
    pub name_regex: bool,
    /// Text in files; empty: no content search.
    pub content: String,
    pub content_regex: bool,
    /// Content and regular expressions only: a pattern always ignores case.
    pub match_case: bool,
    pub size: SizeRange,
    pub modified: DateRange,
    pub kind: KindFilter,
    pub hidden: HiddenRule,
    /// Goes into the folders `[search] skip` names too.
    pub skipped: bool,
    /// The flat view: every file under the scope, no folders.
    pub flat: bool,
}

impl SearchSpec {
    pub fn new(scope: Scope) -> SearchSpec {
        SearchSpec {
            scope,
            pattern: String::new(),
            name_regex: false,
            content: String::new(),
            content_regex: false,
            match_case: false,
            size: SizeRange::default(),
            modified: DateRange::Any,
            kind: KindFilter::Any,
            hidden: HiddenRule::FollowView,
            skipped: false,
            flat: false,
        }
    }

    /// The flat view of `folder` (spec 5).
    pub fn flat_view(folder: PathBuf) -> SearchSpec {
        SearchSpec { flat: true, ..SearchSpec::new(Scope::Folder(folder)) }
    }

    /// Whether there is something to look for: a name, a text, a criterion, or the flat view.
    /// An empty search says "Type something to search" (spec 3.1).
    pub fn is_query(&self) -> bool {
        self.flat
            || !self.pattern.trim().is_empty()
            || !self.content.is_empty()
            || !self.size.is_any()
            || self.modified != DateRange::Any
            || self.kind != KindFilter::Any
    }

    /// The number on the Filters button: the criteria and options that are not as they start.
    pub fn filter_count(&self) -> usize {
        [
            !self.size.is_any(),
            self.modified != DateRange::Any,
            self.kind != KindFilter::Any,
            self.name_regex,
            self.content_regex,
            self.match_case,
            self.hidden == HiddenRule::Include,
            self.skipped,
        ]
        .into_iter()
        .filter(|on| *on)
        .count()
    }

    /// The tab title: `Search: *.pdf`, `Search: "fatura"` (the text in files first).
    pub fn title(&self) -> String {
        if !self.content.is_empty() {
            format!("Search: \"{}\"", self.content)
        } else if self.pattern.trim().is_empty() {
            "Search".to_owned()
        } else {
            format!("Search: {}", self.pattern.trim())
        }
    }

    /// The last address bar part: `Search "*.pdf"`.
    pub fn crumb(&self) -> String {
        if !self.content.is_empty() {
            format!("Search \"{}\"", self.content)
        } else if self.pattern.trim().is_empty() {
            "Search".to_owned()
        } else {
            format!("Search \"{}\"", self.pattern.trim())
        }
    }
}

/// `500 MB`, `1.5 GB`, `10 kB`: B, KB, MB, GB, TB are steps of 1024 (any case), `kB` of 1000;
/// no unit is bytes. Independent of `[view] size-format`.
pub fn parse_size(text: &str) -> Result<u64, String> {
    let bad = || format!("expected a size like \"500 MB\", got \"{}\"", text.trim());
    let trimmed = text.trim();
    let split = trimmed.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(trimmed.len());
    let (number, unit) = (&trimmed[..split], trimmed[split..].trim());
    if number.is_empty() || number.matches('.').count() > 1 {
        return Err(bad());
    }
    let number: f64 = number.parse().map_err(|_| bad())?;
    let step: u64 = if unit == "kB" {
        1000
    } else {
        match unit.to_ascii_lowercase().as_str() {
            "" | "b" => 1,
            "kb" => 1024,
            "mb" => 1024 * 1024,
            "gb" => 1024 * 1024 * 1024,
            "tb" => 1024u64.pow(4),
            _ => return Err(bad()),
        }
    };
    let bytes = (number * step as f64).round();
    if !bytes.is_finite() || bytes < 0.0 || bytes > u64::MAX as f64 / 2.0 {
        return Err(bad());
    }
    Ok(bytes as u64)
}

/// A size as `parse_size` reads it back exactly: the largest binary unit that writes it with
/// at most two decimals, else bytes.
pub fn size_text(bytes: u64) -> String {
    for (unit, step) in [("TB", 1024u64.pow(4)), ("GB", 1024u64.pow(3)), ("MB", 1024u64.pow(2)), ("KB", 1024)] {
        if bytes < step {
            continue;
        }
        let value = (bytes as f64 / step as f64 * 100.0).round() / 100.0;
        if (value * step as f64).round() as u64 == bytes {
            let text = format!("{value:.2}");
            let text = text.trim_end_matches('0').trim_end_matches('.');
            return format!("{text} {unit}");
        }
    }
    format!("{bytes} B")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn day(year: i32, month: u8, day: u8) -> Day {
        Day { year, month, day }
    }

    #[test]
    fn sizes_read_in_both_unit_systems() {
        assert_eq!(parse_size("500 MB"), Ok(500 * 1024 * 1024));
        assert_eq!(parse_size("1.5 GB"), Ok(1_610_612_736));
        assert_eq!(parse_size("1.5gb"), Ok(1_610_612_736), "case and the space do not matter");
        assert_eq!(parse_size("10 kB"), Ok(10_000), "kB is decimal");
        assert_eq!(parse_size("10 KB"), Ok(10_240));
        assert_eq!(parse_size("12"), Ok(12), "bytes without a unit");
        assert_eq!(parse_size("3 B"), Ok(3));
        assert_eq!(parse_size(" 2 TB "), Ok(2 * 1024u64.pow(4)));
        for bad in ["", "MB", "1..5 MB", "5 XB", "-1 MB", "1e3 MB", "99999999999 TB"] {
            assert!(parse_size(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_size("5 XB").unwrap_err(), "expected a size like \"500 MB\", got \"5 XB\"");
    }

    #[test]
    fn sizes_write_back_as_they_read() {
        assert_eq!(size_text(500 * 1024 * 1024), "500 MB");
        assert_eq!(size_text(1_610_612_736), "1.5 GB");
        assert_eq!(size_text(1536), "1.5 KB");
        assert_eq!(size_text(1000), "1000 B");
        assert_eq!(size_text(0), "0 B");
        for bytes in [1u64, 1023, 1024, 10_000, 64 * 1024 * 1024, 1_610_612_736, 5 * 1024u64.pow(4)] {
            assert_eq!(parse_size(&size_text(bytes)), Ok(bytes), "{bytes}");
        }
    }

    #[test]
    fn dates_read_and_write() {
        assert_eq!(DateRange::parse("today"), Ok(DateRange::Today));
        assert_eq!(DateRange::parse("7d"), Ok(DateRange::LastDays(7)));
        assert_eq!(DateRange::parse("year"), Ok(DateRange::ThisYear));
        assert_eq!(DateRange::parse(""), Ok(DateRange::Any));
        assert_eq!(DateRange::parse("any"), Ok(DateRange::Any));
        assert_eq!(
            DateRange::parse("2026-01-01..2026-06-30"),
            Ok(DateRange::Between(day(2026, 1, 1), day(2026, 6, 30)))
        );
        for bad in ["0d", "36501d", "yesterday", "2026-02-30..2026-03-01", "2026-01-01", "2026-1-1..2026-01-02"] {
            assert!(DateRange::parse(bad).is_err(), "{bad}");
        }
        for range in [
            DateRange::Any,
            DateRange::Today,
            DateRange::LastDays(30),
            DateRange::ThisYear,
            DateRange::Between(day(2024, 2, 29), day(2024, 3, 1)),
        ] {
            assert_eq!(DateRange::parse(&range.text()), Ok(range), "{}", range.text());
        }
        assert_eq!(DateRange::LastDays(7).label(), "Last 7 days");
        assert_eq!(DateRange::Between(day(2026, 1, 1), day(2026, 6, 30)).label(), "2026-01-01 – 2026-06-30");
    }

    /// 2026-10-09 14:30 local, the clock three hours ahead of UTC (Istanbul).
    fn istanbul() -> (SystemTime, Day, i64) {
        let local = DateParts { year: 2026, month: 10, day: 9, hour: 14, minute: 30, second: 0 };
        let utc_secs = crate::view::day_number(&local) * 86_400 + 14 * 3600 + 30 * 60 - 3 * 3600;
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(utc_secs as u64);
        assert_eq!(utc_offset(now, &local), 3 * 3600);
        (now, day_of(&local), 3 * 3600)
    }

    #[test]
    fn date_windows_follow_the_local_day() {
        let (now, today, offset) = istanbul();
        let local = |y, mo, d, h, mi| {
            let parts = DateParts { year: y, month: mo, day: d, hour: h, minute: mi, second: 0 };
            let secs = crate::view::day_number(&parts) * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60 - offset;
            SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
        };
        let today_window = DateRange::Today.window(now, today, offset);
        assert!(today_window.contains(Some(local(2026, 10, 9, 0, 0))), "local midnight is in");
        assert!(!today_window.contains(Some(local(2026, 10, 8, 23, 59))));
        let between = DateRange::Between(day(2026, 10, 1), day(2026, 10, 2)).window(now, today, offset);
        assert!(between.contains(Some(local(2026, 10, 2, 23, 59))), "both ends are whole days");
        assert!(!between.contains(Some(local(2026, 10, 3, 0, 0))));
        assert!(!between.contains(Some(local(2026, 9, 30, 23, 59))));
        let swapped = DateRange::Between(day(2026, 10, 2), day(2026, 10, 1)).window(now, today, offset);
        assert_eq!(swapped, between, "the ends may come in either order");
        let week = DateRange::LastDays(7).window(now, today, offset);
        assert!(week.contains(Some(now - Duration::from_secs(6 * 86_400))));
        assert!(!week.contains(Some(now - Duration::from_secs(8 * 86_400))));
        let year = DateRange::ThisYear.window(now, today, offset);
        assert!(year.contains(Some(local(2026, 1, 1, 0, 0))) && !year.contains(Some(local(2025, 12, 31, 23, 59))));
        assert!(TimeWindow::ALL.contains(None), "no date: only without bounds");
        assert!(!week.contains(None));
    }

    #[test]
    fn kinds_map_to_the_icon_kinds() {
        assert!(KindFilter::Pictures.matches("a.JPG", false) && !KindFilter::Pictures.matches("a.jpg", true));
        assert!(KindFilter::Documents.matches("r.pdf", false) && KindFilter::Documents.matches("n.txt", false));
        assert!(KindFilter::Documents.matches("t.xlsx", false) && KindFilter::Documents.matches("s.pptx", false));
        assert!(KindFilter::Archives.matches("x.iso", false) && KindFilter::Archives.matches("x.7z", false));
        assert!(KindFilter::Code.matches("main.rs", false) && !KindFilter::Code.matches("n.txt", false));
        assert!(KindFilter::Folders.matches("x", true) && !KindFilter::Folders.matches("x", false));
        assert!(KindFilter::Files.matches("x", false) && !KindFilter::Files.matches("x", true));
        assert!(KindFilter::Any.matches("x", true));
        for kind in KindFilter::ALL {
            assert_eq!(KindFilter::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(KindFilter::parse("photos"), None);
    }

    #[test]
    fn an_empty_search_is_no_query_but_a_flat_view_is() {
        let mut spec = SearchSpec::new(Scope::Folder("/w".into()));
        assert!(!spec.is_query());
        spec.pattern = "  ".into();
        assert!(!spec.is_query(), "blanks only");
        spec.size.min = Some(1);
        assert!(spec.is_query(), "a size alone searches");
        assert!(SearchSpec::flat_view("/w".into()).is_query());
        assert!(SearchSpec::flat_view("/w".into()).flat);
    }

    #[test]
    fn titles_and_filter_counts() {
        let mut spec = SearchSpec::new(Scope::AllDrives);
        spec.pattern = " *.pdf ".into();
        assert_eq!(spec.title(), "Search: *.pdf");
        assert_eq!(spec.crumb(), "Search \"*.pdf\"");
        spec.content = "fatura".into();
        assert_eq!(spec.title(), "Search: \"fatura\"");
        assert_eq!(spec.filter_count(), 0);
        spec.modified = DateRange::Today;
        spec.match_case = true;
        spec.hidden = HiddenRule::Include;
        assert_eq!(spec.filter_count(), 3);
    }

    #[test]
    fn days_are_checked() {
        assert_eq!(Day::parse("2024-02-29"), Some(day(2024, 2, 29)));
        assert_eq!(Day::parse("2023-02-29"), None, "no leap day");
        assert_eq!(Day::parse("2026-13-01"), None);
        assert_eq!(day(2026, 1, 5).text(), "2026-01-05");
    }
}
