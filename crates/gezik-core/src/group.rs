//! Grouping the file list (spec 10 §6): by type, date or size, with Explorer's buckets. Pure: the
//! local time comes in as a function, and nothing here touches the file system. A grouped sort
//! puts each row's group first (`Grouping::ranks`); the groups are then the runs of rows with the
//! same key (`spans`), a header line each (`layout::Lines`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::Entry;
use crate::batch::date::DateParts;
use crate::layout::Span;
use crate::meta_filter::TagKey;
use crate::sort::{SortDir, SortKey, SortSpec, natural_cmp};

/// What the list is grouped by (`views.toml` and `[view] group`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GroupBy {
    #[default]
    None,
    Type,
    Date,
    Size,
    /// Each row's front tag (spec 12 §6.2): one group a row.
    Tag,
    /// Each row's stars (spec 12 §8.2): five first, then not rated.
    Rating,
    /// Duplicate files' groups (spec 11 §9.3): only a duplicates search's results, never a
    /// setting or a menu item (not in `ALL`).
    Duplicates,
}

impl GroupBy {
    pub const ALL: [GroupBy; 6] =
        [GroupBy::None, GroupBy::Type, GroupBy::Date, GroupBy::Size, GroupBy::Tag, GroupBy::Rating];

    pub fn as_str(self) -> &'static str {
        match self {
            GroupBy::None => "none",
            GroupBy::Type => "type",
            GroupBy::Date => "date",
            GroupBy::Size => "size",
            GroupBy::Tag => "tag",
            GroupBy::Rating => "rating",
            GroupBy::Duplicates => "duplicates",
        }
    }

    pub fn parse(text: &str) -> Option<GroupBy> {
        GroupBy::ALL.into_iter().find(|g| g.as_str() == text)
    }

    /// Its item in Group by ▸.
    pub fn title(self) -> &'static str {
        match self {
            GroupBy::None => "None",
            GroupBy::Type => "Type",
            GroupBy::Date => "Date",
            GroupBy::Size => "Size",
            GroupBy::Tag => "Tag",
            GroupBy::Rating => "Rating",
            GroupBy::Duplicates => "Duplicates",
        }
    }
}

/// Explorer's date buckets, newest first; `Later` holds times after today (a clock set ahead).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateGroup {
    Later,
    Today,
    Yesterday,
    EarlierThisWeek,
    LastWeek,
    EarlierThisMonth,
    LastMonth,
    EarlierThisYear,
    Year(i32),
    LongAgo,
    NoDate,
}

/// Explorer's size buckets, biggest first; `Folders`: a folder whose size is not worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeGroup {
    Gigantic,
    Huge,
    Large,
    Medium,
    Small,
    Tiny,
    Empty,
    Folders,
}

/// A row's group: what its header says.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GroupKey {
    /// The Type column's text.
    Type(String),
    Date(DateGroup),
    Size(SizeGroup),
    Tag(TagKey),
    /// Stars (0: not rated); `None` while on their way.
    Rating(Option<u8>),
    /// A group of duplicate files: its place (most bytes to free first) and each file's size.
    Duplicates {
        id: u32,
        size: u64,
    },
}

impl GroupKey {
    pub fn label(&self) -> String {
        match self {
            GroupKey::Type(name) => name.clone(),
            GroupKey::Date(DateGroup::Year(year)) => year.to_string(),
            GroupKey::Date(g) => match g {
                DateGroup::Later => "Later",
                DateGroup::Today => "Today",
                DateGroup::Yesterday => "Yesterday",
                DateGroup::EarlierThisWeek => "Earlier this week",
                DateGroup::LastWeek => "Last week",
                DateGroup::EarlierThisMonth => "Earlier this month",
                DateGroup::LastMonth => "Last month",
                DateGroup::EarlierThisYear => "Earlier this year",
                DateGroup::Year(_) | DateGroup::LongAgo => "A long time ago",
                DateGroup::NoDate => "No date",
            }
            .to_owned(),
            GroupKey::Size(g) => match g {
                SizeGroup::Gigantic => "Gigantic",
                SizeGroup::Huge => "Huge",
                SizeGroup::Large => "Large",
                SizeGroup::Medium => "Medium",
                SizeGroup::Small => "Small",
                SizeGroup::Tiny => "Tiny",
                SizeGroup::Empty => "Empty",
                SizeGroup::Folders => "Folders",
            }
            .to_owned(),
            GroupKey::Tag(key) => key.label(),
            GroupKey::Rating(Some(0)) => "Not Rated".to_owned(),
            GroupKey::Rating(Some(stars)) => "★".repeat(usize::from(*stars)),
            GroupKey::Rating(None) => TagKey::Pending.label(),
            GroupKey::Duplicates { size, .. } => crate::format_size(*size),
        }
    }

    /// Its header line for `len` rows: `Today (12)`; duplicates
    /// `3 files × 12.4 MB · 24.8 MB could be freed`.
    pub fn header(&self, len: usize) -> String {
        match self {
            GroupKey::Duplicates { size, .. } => {
                let freed = size.saturating_mul(len.saturating_sub(1) as u64);
                format!("{len} files × {} · {} could be freed", crate::format_size(*size), crate::format_size(freed))
            }
            _ => format!("{} ({len})", self.label()),
        }
    }
}

/// Years shown each as a group (this one's in the buckets above, then nine more); older ones
/// are `LongAgo`.
const YEARS: i32 = 10;

/// Where each date bucket starts, as times: local midnights worked out once (a sort), so each
/// row costs only comparisons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DateBounds {
    year: i32,
    /// Tried in this order: the first start at or before a time is its bucket.
    starts: Vec<(DateGroup, SystemTime)>,
}

/// Seconds since 1970 (negative before), whole seconds down.
fn secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => {
            -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX) - i64::from(e.duration().subsec_nanos() > 0)
        }
    }
}

fn at(secs: i64) -> SystemTime {
    let d = Duration::from_secs(secs.unsigned_abs());
    if secs >= 0 { UNIX_EPOCH + d } else { UNIX_EPOCH - d }
}

fn clock_secs(p: &DateParts) -> i64 {
    i64::from(p.hour) * 3600 + i64::from(p.minute) * 60 + i64::from(p.second)
}

fn first_of(year: i32, month: u8) -> i64 {
    crate::view::day_number(&DateParts { year, month, day: 1, ..Default::default() })
}

impl DateBounds {
    /// The buckets as seen at `now`; `local` gives a time's local date and clock
    /// (`gezik_platform::local_date_parts`). If it cannot, there are no buckets: every dated
    /// row is `LongAgo`.
    pub fn new(now: SystemTime, local: &dyn Fn(SystemTime) -> Option<DateParts>) -> DateBounds {
        let Some(today) = local(now) else { return DateBounds::default() };
        let day = crate::view::day_number(&today);
        let midnight = secs(now) - clock_secs(&today);
        // The start of local day `d`: today's midnight moved by whole days, then put right
        // where a daylight-saving change in between moved the clock.
        let start = |d: i64| {
            let guess = midnight + (d - day) * 86_400;
            match local(at(guess)) {
                Some(p) => at(guess - (crate::view::day_number(&p) - d) * 86_400 - clock_secs(&p)),
                None => at(guess),
            }
        };
        // 0: Monday (1970-01-01 was a Thursday). shortcut: weeks start on Monday everywhere;
        // read the locale's first day if users ask for Sunday weeks.
        let weekday = (day + 3).rem_euclid(7);
        let (y, m) = (today.year, today.month);
        let (last_y, last_m) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
        let mut starts = vec![
            (DateGroup::Later, start(day + 1)),
            (DateGroup::Today, start(day)),
            (DateGroup::Yesterday, start(day - 1)),
            (DateGroup::EarlierThisWeek, start(day - weekday)),
            (DateGroup::LastWeek, start(day - weekday - 7)),
            (DateGroup::EarlierThisMonth, start(first_of(y, m))),
            (DateGroup::LastMonth, start(first_of(last_y, last_m))),
            (DateGroup::EarlierThisYear, start(first_of(y, 1))),
        ];
        starts.extend((1..YEARS).map(|back| (DateGroup::Year(y - back), start(first_of(y - back, 1)))));
        DateBounds { year: y, starts }
    }

    /// The bucket of a row's time; `None`: `NoDate`.
    pub fn group(&self, time: Option<SystemTime>) -> DateGroup {
        let Some(time) = time else { return DateGroup::NoDate };
        self.starts.iter().find(|(_, start)| time >= *start).map_or(DateGroup::LongAgo, |(g, _)| *g)
    }

    /// Newest first.
    fn place(&self, g: DateGroup) -> u32 {
        match g {
            DateGroup::Later => 0,
            DateGroup::Today => 1,
            DateGroup::Yesterday => 2,
            DateGroup::EarlierThisWeek => 3,
            DateGroup::LastWeek => 4,
            DateGroup::EarlierThisMonth => 5,
            DateGroup::LastMonth => 6,
            DateGroup::EarlierThisYear => 7,
            DateGroup::Year(y) => 8 + u32::try_from(self.year - 1 - y).unwrap_or(0),
            DateGroup::LongAgo => 100,
            DateGroup::NoDate => 101,
        }
    }
}

/// A row's size bucket: a file's size, a folder's worked-out total, else `Folders`.
pub fn size_group(e: &Entry) -> SizeGroup {
    const KB: u64 = 1024;
    const MB: u64 = KB * KB;
    const GB: u64 = MB * KB;
    match e.known_size() {
        None => SizeGroup::Folders,
        Some(0) => SizeGroup::Empty,
        Some(b) if b < 16 * KB => SizeGroup::Tiny,
        Some(b) if b < MB => SizeGroup::Small,
        Some(b) if b < 128 * MB => SizeGroup::Medium,
        Some(b) if b < GB => SizeGroup::Large,
        Some(b) if b < 4 * GB => SizeGroup::Huge,
        Some(_) => SizeGroup::Gigantic,
    }
}

/// How a list is grouped: the grouping, the sort it goes with, and what keys need.
pub struct Grouping<'a> {
    pub by: GroupBy,
    pub spec: SortSpec,
    pub folders_first: bool,
    pub dates: &'a DateBounds,
    /// The Type column's text (the same function the sort uses).
    pub type_name: &'a dyn Fn(&Entry) -> String,
    /// Row `i`'s group when the row, not its entry, knows it (duplicates); `None` for others.
    pub row: Option<&'a dyn Fn(usize) -> Option<GroupKey>>,
    /// A row's front tag (spec 12 §6), for `GroupBy::Tag` and `SortKey::Tags`; `None`: none
    /// known (every row untagged). A sort by tags that is not grouped passes `by: None` with it.
    pub tags: Option<&'a dyn Fn(&Entry) -> TagKey>,
    /// A row's stars (0: not rated; `None`: not read yet), for `GroupBy::Rating` and
    /// `SortKey::Rating` (spec 12 §8.2); `None`: none known (every row not rated).
    pub rating: Option<&'a RatingKey>,
}

/// A row's stars (`None`: not read yet).
pub type RatingKey = dyn Fn(&Entry) -> Option<u8>;

/// Ranks below this go first in either direction (the folders' group with folders-first).
const HALF: u32 = 1 << 31;
/// Tag groups' places: the list's tags by their place in it, then unlisted tags by name, then
/// no tags, then those not read yet.
const TAG_UNLISTED: u32 = 1 << 16;
const TAG_UNTAGGED: u32 = 1 << 30;

impl Grouping<'_> {
    /// Row `e`'s group; `None` when not grouped.
    pub fn key(&self, e: &Entry) -> Option<GroupKey> {
        Some(match self.by {
            GroupBy::None => return None,
            GroupBy::Type => GroupKey::Type((self.type_name)(e)),
            GroupBy::Date => {
                GroupKey::Date(self.dates.group(if self.spec.key == SortKey::Created { e.created } else { e.modified }))
            }
            GroupBy::Size => GroupKey::Size(size_group(e)),
            GroupBy::Tag => GroupKey::Tag(self.tags.map_or(TagKey::Untagged, |key| key(e))),
            GroupBy::Rating => GroupKey::Rating(self.rating.map_or(Some(0), |key| key(e))),
            GroupBy::Duplicates => return None,
        })
    }

    /// Row `i`'s group (its entry `e`).
    fn key_at(&self, i: usize, e: &Entry) -> Option<GroupKey> {
        match self.row {
            Some(row) => row(i),
            None => self.key(e),
        }
    }

    /// Whether the groups go against their canonical order (sapma 1): a grouping sorted by its
    /// own column follows that column's direction, any other sort turns them round when
    /// descending.
    fn reversed(&self) -> bool {
        // Duplicates: the most bytes to free first, whatever the sort.
        if self.by == GroupBy::Duplicates {
            return false;
        }
        let own = matches!(
            (self.by, self.spec.key),
            (GroupBy::Date, SortKey::Modified | SortKey::Created)
                | (GroupBy::Size, SortKey::Size)
                | (GroupBy::Rating, SortKey::Rating)
        );
        if own { self.spec.dir == SortDir::Asc } else { self.spec.dir == SortDir::Desc }
    }

    /// The folders' group goes first whatever the direction (folders-first; sapma 2).
    fn pinned(&self, e: &Entry, key: &GroupKey) -> bool {
        self.folders_first
            && match key {
                GroupKey::Type(_) => e.is_dir,
                GroupKey::Size(g) => *g == SizeGroup::Folders,
                GroupKey::Date(_) | GroupKey::Tag(_) | GroupKey::Rating(_) | GroupKey::Duplicates { .. } => false,
            }
    }

    /// Each row's place among the groups, smaller first: the sort's first key. Empty when not
    /// grouped. Type names are put in natural order once all are known.
    pub fn ranks<'e>(&self, len: usize, entry: &dyn Fn(usize) -> Cow<'e, Entry>) -> Vec<u32> {
        // Type names and unlisted tags are put in natural order once all are known: their rows
        // hold a name's number until then (`named`).
        let mut names: HashMap<String, u32> = HashMap::new();
        let mut rows: Vec<(bool, u32, bool)> = Vec::with_capacity(len);
        let mut name_id = |name: String| {
            let next = u32::try_from(names.len()).unwrap_or(u32::MAX);
            *names.entry(name).or_insert(next)
        };
        for i in 0..len {
            let e = entry(i);
            let Some(key) = self.key_at(i, &e) else { return Vec::new() };
            let pinned = self.pinned(&e, &key);
            let (place, named) = match key {
                GroupKey::Type(name) => (name_id(name), true),
                GroupKey::Date(g) => (self.dates.place(g), false),
                GroupKey::Size(g) => (g as u32, false),
                GroupKey::Tag(TagKey::Listed { rank, .. }) => (rank, false),
                GroupKey::Tag(TagKey::Unlisted(name)) => (name_id(name.into()), true),
                GroupKey::Tag(TagKey::Untagged) => (TAG_UNTAGGED, false),
                GroupKey::Tag(TagKey::Pending) => (TAG_UNTAGGED + 1, false),
                // Five stars first, then fewer, then not rated, then not read yet.
                GroupKey::Rating(stars) => (stars.map_or(6, |n| 5 - u32::from(n.min(5))), false),
                GroupKey::Duplicates { id, .. } => (id, false),
            };
            rows.push((pinned, place, named));
        }
        if !names.is_empty() {
            let mut sorted: Vec<(&String, &u32)> = names.iter().collect();
            sorted.sort_by(|a, b| natural_cmp(a.0, b.0));
            let mut by_id = vec![0u32; names.len()];
            for (rank, (_, id)) in sorted.iter().enumerate() {
                by_id[**id as usize] = u32::try_from(rank).unwrap_or(u32::MAX);
            }
            // Unlisted tags after the list's (at most 60).
            let offset = if self.by == GroupBy::Tag { TAG_UNLISTED } else { 0 };
            for row in rows.iter_mut().filter(|row| row.2) {
                row.1 = offset + by_id[row.1 as usize].min(TAG_UNTAGGED - TAG_UNLISTED - 1);
            }
        }
        let reversed = self.reversed();
        rows.into_iter()
            .map(|(pinned, place, _)| match (pinned, reversed) {
                (true, _) => place.min(HALF - 1),
                (false, false) => HALF + place.min(HALF - 1),
                (false, true) => u32::MAX - place.min(HALF - 1),
            })
            .collect()
    }
}

/// The groups of rows sorted with `grouping`: each run of rows with the same key, closed if
/// `collapsed` says so, and its key. Rows past what `entry` gives end it. Not grouped: none.
pub fn spans<'e>(
    len: usize,
    entry: &dyn Fn(usize) -> Option<Cow<'e, Entry>>,
    grouping: &Grouping<'_>,
    collapsed: &dyn Fn(&GroupKey) -> bool,
) -> (Vec<Span>, Vec<GroupKey>) {
    let (mut spans, mut keys): (Vec<Span>, Vec<GroupKey>) = (Vec::new(), Vec::new());
    for i in 0..len {
        let Some(e) = entry(i) else { break };
        let Some(key) = grouping.key_at(i, &e) else { return (Vec::new(), Vec::new()) };
        match (spans.last_mut(), keys.last()) {
            (Some(span), Some(last)) if *last == key => span.len += 1,
            _ => {
                spans.push(Span { start: i, len: 1, collapsed: collapsed(&key) });
                keys.push(key);
            }
        }
    }
    (spans, keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Days since 1970 → (year, month, day) (Howard Hinnant's `civil_from_days`).
    fn civil(days: i64) -> (i32, u8, u8) {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
        let y = (yoe + era * 400) as i32;
        (if m <= 2 { y + 1 } else { y }, m, d)
    }

    /// A clock `offset` seconds ahead of UTC at `t` (`offset_at`), as `local_date_parts` answers.
    fn zone(offset_at: impl Fn(i64) -> i64) -> impl Fn(SystemTime) -> Option<DateParts> {
        move |t| {
            let utc = secs(t);
            let local = utc + offset_at(utc);
            let (days, rest) = (local.div_euclid(86_400), local.rem_euclid(86_400));
            let (year, month, day) = civil(days);
            Some(DateParts {
                year,
                month,
                day,
                hour: (rest / 3600) as u8,
                minute: (rest % 3600 / 60) as u8,
                second: (rest % 60) as u8,
            })
        }
    }

    /// `y-m-d h:mi` UTC.
    fn utc(y: i32, m: u8, d: u8, h: i64, mi: i64) -> SystemTime {
        let day = crate::view::day_number(&DateParts { year: y, month: m, day: d, ..Default::default() });
        at(day * 86_400 + h * 3600 + mi * 60)
    }

    fn file(name: &str, size: u64, modified: Option<SystemTime>) -> Entry {
        Entry { name: name.into(), is_dir: false, flags: 0, size, modified, created: None }
    }

    fn folder(name: &str) -> Entry {
        Entry { name: name.into(), is_dir: true, flags: 0, size: 0, modified: None, created: None }
    }

    #[test]
    fn dates_fall_in_explorers_buckets() {
        // Wednesday 2026-10-14 15:00, UTC.
        let b = DateBounds::new(utc(2026, 10, 14, 15, 0), &zone(|_| 0));
        let g = |y, m, d, h, mi| b.group(Some(utc(y, m, d, h, mi)));
        assert_eq!(g(2026, 10, 15, 0, 0), DateGroup::Later, "tomorrow");
        assert_eq!(g(2026, 10, 14, 0, 0), DateGroup::Today);
        assert_eq!(g(2026, 10, 13, 23, 59), DateGroup::Yesterday);
        assert_eq!(g(2026, 10, 13, 0, 0), DateGroup::Yesterday);
        assert_eq!(g(2026, 10, 12, 0, 0), DateGroup::EarlierThisWeek, "Monday");
        assert_eq!(g(2026, 10, 11, 23, 59), DateGroup::LastWeek, "Sunday before");
        assert_eq!(g(2026, 10, 5, 0, 0), DateGroup::LastWeek);
        assert_eq!(g(2026, 10, 4, 23, 59), DateGroup::EarlierThisMonth);
        assert_eq!(g(2026, 10, 1, 0, 0), DateGroup::EarlierThisMonth);
        assert_eq!(g(2026, 9, 30, 23, 59), DateGroup::LastMonth);
        assert_eq!(g(2026, 9, 1, 0, 0), DateGroup::LastMonth);
        assert_eq!(g(2026, 8, 31, 23, 59), DateGroup::EarlierThisYear);
        assert_eq!(g(2026, 1, 1, 0, 0), DateGroup::EarlierThisYear);
        assert_eq!(g(2025, 12, 31, 23, 59), DateGroup::Year(2025));
        assert_eq!(g(2017, 1, 1, 0, 0), DateGroup::Year(2017), "nine years back");
        assert_eq!(g(2016, 12, 31, 23, 59), DateGroup::LongAgo, "ten years back");
        assert_eq!(g(1960, 1, 1, 0, 0), DateGroup::LongAgo, "before 1970");
        assert_eq!(b.group(None), DateGroup::NoDate);
    }

    #[test]
    fn a_monday_on_new_years_week() {
        // Monday 2027-01-04: yesterday was last week and last year.
        let b = DateBounds::new(utc(2027, 1, 4, 9, 0), &zone(|_| 0));
        let g = |y, m, d| b.group(Some(utc(y, m, d, 12, 0)));
        assert_eq!(g(2027, 1, 3), DateGroup::Yesterday);
        assert_eq!(g(2027, 1, 2), DateGroup::LastWeek);
        assert_eq!(g(2026, 12, 28), DateGroup::LastWeek, "last week's Monday, last year");
        assert_eq!(g(2026, 12, 27), DateGroup::LastMonth);
        assert_eq!(g(2026, 11, 30), DateGroup::Year(2026));
    }

    #[test]
    fn the_buckets_are_local_midnights() {
        // UTC+3: 00:30 on the 14th there is 21:30 UTC on the 13th.
        let b = DateBounds::new(utc(2026, 10, 13, 21, 30), &zone(|_| 3 * 3600));
        assert_eq!(b.group(Some(utc(2026, 10, 13, 22, 0))), DateGroup::Today, "01:00 local");
        assert_eq!(b.group(Some(utc(2026, 10, 13, 20, 0))), DateGroup::Yesterday, "23:00 local the day before");
    }

    #[test]
    fn a_daylight_saving_change_moves_the_midnight() {
        // +3 h until 2026-10-25 01:00 UTC, +2 h after (as Europe's clocks go back). Monday 26th.
        let change = secs(utc(2026, 10, 25, 1, 0));
        let b = DateBounds::new(utc(2026, 10, 26, 12, 0), &zone(move |t| if t < change { 3 * 3600 } else { 2 * 3600 }));
        assert_eq!(b.group(Some(utc(2026, 10, 25, 22, 0))), DateGroup::Today, "00:00 local on the 26th");
        assert_eq!(b.group(Some(utc(2026, 10, 25, 21, 59))), DateGroup::Yesterday);
        assert_eq!(b.group(Some(utc(2026, 10, 24, 21, 0))), DateGroup::Yesterday, "00:00 local on the 25th (+3)");
        assert_eq!(b.group(Some(utc(2026, 10, 24, 20, 59))), DateGroup::LastWeek);
    }

    #[test]
    fn size_buckets_have_explorers_limits() {
        const KB: u64 = 1024;
        const MB: u64 = KB * KB;
        const GB: u64 = MB * KB;
        let s = |size| size_group(&file("a", size, None));
        assert_eq!(s(0), SizeGroup::Empty);
        assert_eq!((s(1), s(16 * KB - 1)), (SizeGroup::Tiny, SizeGroup::Tiny));
        assert_eq!((s(16 * KB), s(MB - 1)), (SizeGroup::Small, SizeGroup::Small));
        assert_eq!((s(MB), s(128 * MB - 1)), (SizeGroup::Medium, SizeGroup::Medium));
        assert_eq!((s(128 * MB), s(GB - 1)), (SizeGroup::Large, SizeGroup::Large));
        assert_eq!((s(GB), s(4 * GB - 1)), (SizeGroup::Huge, SizeGroup::Huge));
        assert_eq!(s(4 * GB), SizeGroup::Gigantic);
        assert_eq!(size_group(&folder("d")), SizeGroup::Folders, "not sized");
        let sized = Entry { flags: Entry::SIZED, size: 2 * MB, ..folder("d") };
        assert_eq!(size_group(&sized), SizeGroup::Medium, "a worked-out size has its bucket");
        let pending = Entry { flags: Entry::SIZE_PENDING, ..folder("d") };
        assert_eq!(size_group(&pending), SizeGroup::Folders);
    }

    fn by(group: GroupBy, key: SortKey, dir: SortDir, folders_first: bool, entries: &[Entry]) -> Vec<String> {
        let dates = DateBounds::new(utc(2026, 10, 14, 15, 0), &zone(|_| 0));
        let type_name = |e: &Entry| {
            if e.is_dir { "File folder".to_owned() } else { format!("{} File", e.extension().to_uppercase()) }
        };
        let spec = SortSpec { key, dir };
        let grouping = Grouping {
            by: group,
            spec,
            folders_first,
            dates: &dates,
            type_name: &type_name,
            row: None,
            tags: None,
            rating: None,
        };
        let mut v = entries.to_vec();
        crate::sort::sort_entries_grouped(&mut v, spec, folders_first, type_name, Some(&grouping));
        v.into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn size_groups_go_biggest_first_and_follow_their_own_column() {
        let v =
            [file("e", 0, None), file("t", 10, None), file("g", 5 << 30, None), folder("d"), file("m", 2 << 20, None)];
        let asc = SortDir::Asc;
        assert_eq!(
            by(GroupBy::Size, SortKey::Name, asc, true, &v),
            ["d", "g", "m", "t", "e"],
            "Folders first, then biggest"
        );
        assert_eq!(
            by(GroupBy::Size, SortKey::Name, SortDir::Desc, true, &v),
            ["d", "e", "t", "m", "g"],
            "reversed, Folders stay first"
        );
        assert_eq!(
            by(GroupBy::Size, SortKey::Size, asc, true, &v),
            ["d", "e", "t", "m", "g"],
            "Size asc: smallest first"
        );
        assert_eq!(
            by(GroupBy::Size, SortKey::Name, asc, false, &v),
            ["g", "m", "t", "e", "d"],
            "Folders last without folders-first"
        );
    }

    #[test]
    fn date_groups_go_newest_first_and_follow_their_own_column() {
        let v = [
            file("old", 1, Some(utc(2020, 5, 1, 0, 0))),
            file("today", 1, Some(utc(2026, 10, 14, 9, 0))),
            file("none", 1, None),
            file("sept", 1, Some(utc(2026, 9, 3, 0, 0))),
        ];
        assert_eq!(by(GroupBy::Date, SortKey::Name, SortDir::Asc, true, &v), ["today", "sept", "old", "none"]);
        assert_eq!(by(GroupBy::Date, SortKey::Modified, SortDir::Desc, true, &v), ["today", "sept", "old", "none"]);
        assert_eq!(by(GroupBy::Date, SortKey::Modified, SortDir::Asc, true, &v), ["none", "old", "sept", "today"]);
        assert_eq!(by(GroupBy::Date, SortKey::Name, SortDir::Desc, true, &v), ["none", "old", "sept", "today"]);
    }

    #[test]
    fn type_groups_go_by_name_with_folders_first() {
        let v = [
            file("b.txt", 1, None),
            file("a.png", 1, None),
            folder("z"),
            file("c.txt", 1, None),
            file("x10.doc", 1, None),
        ];
        assert_eq!(
            by(GroupBy::Type, SortKey::Name, SortDir::Asc, true, &v),
            ["z", "x10.doc", "a.png", "b.txt", "c.txt"]
        );
        assert_eq!(
            by(GroupBy::Type, SortKey::Name, SortDir::Desc, true, &v),
            ["z", "c.txt", "b.txt", "a.png", "x10.doc"]
        );
        assert_eq!(
            by(GroupBy::Type, SortKey::Name, SortDir::Asc, false, &v),
            ["x10.doc", "z", "a.png", "b.txt", "c.txt"],
            "File folder by its name"
        );
    }

    #[test]
    fn spans_are_the_runs_of_one_group() {
        let dates = DateBounds::new(utc(2026, 10, 14, 15, 0), &zone(|_| 0));
        let type_name = |e: &Entry| format!("{} File", e.extension().to_uppercase());
        let spec = SortSpec::default();
        let grouping = Grouping {
            by: GroupBy::Type,
            spec,
            folders_first: true,
            dates: &dates,
            type_name: &type_name,
            row: None,
            tags: None,
            rating: None,
        };
        let rows = [file("a.png", 1, None), file("b.png", 1, None), file("c.txt", 1, None)];
        let closed = GroupKey::Type("TXT File".into());
        let (spans, keys) = spans(3, &|i| rows.get(i).map(Cow::Borrowed), &grouping, &|k| *k == closed);
        assert_eq!(
            spans,
            [
                crate::layout::Span { start: 0, len: 2, collapsed: false },
                crate::layout::Span { start: 2, len: 1, collapsed: true }
            ]
        );
        assert_eq!(keys, [GroupKey::Type("PNG File".into()), closed]);
        let none = Grouping { by: GroupBy::None, ..grouping };
        assert_eq!(super::spans(3, &|i| rows.get(i).map(Cow::Borrowed), &none, &|_| false), (vec![], vec![]));
        assert_eq!(GroupKey::Date(DateGroup::Year(2024)).label(), "2024");
        assert_eq!(GroupKey::Date(DateGroup::EarlierThisWeek).label(), "Earlier this week");
        assert_eq!(GroupKey::Size(SizeGroup::Gigantic).label(), "Gigantic");
    }

    #[test]
    fn duplicates_group_by_row_most_to_free_first_whatever_the_sort() {
        let dates = DateBounds::default();
        let type_name = |_: &Entry| String::new();
        // Rows 0 and 2 are group 1, rows 1 and 3 group 0 (the one with more to free).
        let ids = [1u32, 0, 1, 0];
        let row = |i: usize| Some(GroupKey::Duplicates { id: ids[i], size: 5 });
        let rows = [file("b", 5, None), file("d", 5, None), file("a", 5, None), file("c", 5, None)];
        for dir in [SortDir::Asc, SortDir::Desc] {
            let spec = SortSpec { key: SortKey::Name, dir };
            let grouping = Grouping {
                by: GroupBy::Duplicates,
                spec,
                folders_first: true,
                dates: &dates,
                type_name: &type_name,
                row: Some(&row),
                tags: None,
                rating: None,
            };
            let order = crate::sort::sort_rows_grouped(
                4,
                &|i| Cow::Borrowed(&rows[i]),
                &|i| &rows[i].name,
                spec,
                true,
                type_name,
                &|_| "",
                Some(&grouping),
            );
            let names: Vec<&str> = order.iter().map(|&i| rows[i].name.as_str()).collect();
            let want = if dir == SortDir::Asc { ["c", "d", "a", "b"] } else { ["d", "c", "b", "a"] };
            assert_eq!(names, want, "group 0 first, the sort inside: {dir:?}");
        }
        let sorted = [1usize, 3, 0, 2];
        let spec = SortSpec::default();
        let in_order = |i: usize| Some(GroupKey::Duplicates { id: ids[sorted[i]], size: 5 });
        let grouping = Grouping {
            by: GroupBy::Duplicates,
            spec,
            folders_first: true,
            dates: &dates,
            type_name: &type_name,
            row: Some(&in_order),
            tags: None,
            rating: None,
        };
        let (spans, keys) = spans(4, &|i| rows.get(i).map(Cow::Borrowed), &grouping, &|_| false);
        assert_eq!(spans.iter().map(|s| (s.start, s.len)).collect::<Vec<_>>(), [(0, 2), (2, 2)]);
        assert_eq!(keys[0], GroupKey::Duplicates { id: 0, size: 5 });
        let key = GroupKey::Duplicates { id: 0, size: 13 * 1024 * 1024 };
        assert_eq!(key.header(3), "3 files × 13.0 MB · 26.0 MB could be freed");
        assert_eq!(GroupKey::Size(SizeGroup::Tiny).header(12), "Tiny (12)");
        assert_eq!(GroupBy::parse("duplicates"), None, "never a setting");
    }

    #[test]
    fn group_names_read_back() {
        for g in GroupBy::ALL {
            assert_eq!(GroupBy::parse(g.as_str()), Some(g));
        }
        assert_eq!(GroupBy::parse("tag"), Some(GroupBy::Tag));
        assert_eq!(GroupBy::default(), GroupBy::None);
    }

    /// `cargo test -p gezik-core --release -- --ignored grouped_sort`
    #[test]
    #[ignore]
    fn grouped_sort_of_100k_is_within_budget() {
        let now = SystemTime::now();
        let exts = ["txt", "png", "jpg", "pdf", "docx", "zip", "mp4", "rs"];
        let rows: Vec<Entry> = (0..100_000u64)
            .map(|i| Entry {
                name: format!("IMG_{:05} kopya şğı {}.{}", (i * 7919) % 100_000, i, exts[(i % 8) as usize]),
                is_dir: i % 50 == 0,
                flags: 0,
                size: (i * 104_729) % (3 << 30),
                modified: now.checked_sub(Duration::from_secs((i * 7_777) % (12 * 365 * 86_400))),
                created: None,
            })
            .collect();
        let dates = DateBounds::new(now, &zone(|_| 0));
        let type_name = |e: &Entry| {
            if e.is_dir { "File folder".to_owned() } else { format!("{} File", e.extension().to_uppercase()) }
        };
        // The best of three runs, so a busy machine does not decide.
        let time = |group: GroupBy, key: SortKey| {
            (0..3)
                .map(|_| {
                    let mut v = rows.clone();
                    let spec = SortSpec { key, dir: SortDir::Asc };
                    let grouping = Grouping {
                        by: group,
                        spec,
                        folders_first: true,
                        dates: &dates,
                        type_name: &type_name,
                        row: None,
                        tags: None,
                        rating: None,
                    };
                    let start = std::time::Instant::now();
                    crate::sort::sort_entries_grouped(
                        &mut v,
                        spec,
                        true,
                        type_name,
                        (group != GroupBy::None).then_some(&grouping),
                    );
                    let _ = spans(v.len(), &|i| v.get(i).map(Cow::Borrowed), &grouping, &|_| false);
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let plain = time(GroupBy::None, SortKey::Name);
        let by_type = time(GroupBy::None, SortKey::Type);
        let date = time(GroupBy::Date, SortKey::Name);
        let size = time(GroupBy::Size, SortKey::Name);
        let types = time(GroupBy::Type, SortKey::Name);
        eprintln!("name {plain:?}, type {by_type:?}; grouped by date {date:?}, size {size:?}, type {types:?}");
        assert!(date.as_secs_f64() <= plain.as_secs_f64() * 1.2, "date {date:?} vs {plain:?}");
        assert!(size.as_secs_f64() <= plain.as_secs_f64() * 1.2, "size {size:?} vs {plain:?}");
        assert!(types.as_secs_f64() <= by_type.as_secs_f64() * 1.2, "type {types:?} vs {by_type:?} (sapma 14)");
    }

    /// Rows tagged by name (`""`: untagged, absent: not read), sorted by `key` and grouped by `by`.
    fn by_tags(
        by: GroupBy,
        key: SortKey,
        dir: SortDir,
        folders_first: bool,
        rows: &[Entry],
    ) -> (Vec<String>, Vec<String>) {
        use crate::meta::{Tag, TagColor};
        use crate::meta_filter::tag_key;
        let list = [Tag::new("Red", TagColor::Red), Tag::new("Green", TagColor::Green)];
        let tags_of = |e: &Entry| -> Option<Vec<Tag>> {
            let stem = e.name.split('.').next().unwrap_or_default();
            (!stem.starts_with('?'))
                .then(|| stem.split('+').filter(|t| !t.is_empty()).map(|t| Tag::new(t, TagColor::None)).collect())
        };
        let tag_fn = |e: &Entry| tag_key(tags_of(e).as_deref(), &list);
        let dates = DateBounds::default();
        let type_name = |_: &Entry| String::new();
        let spec = SortSpec { key, dir };
        let grouping = Grouping {
            by,
            spec,
            folders_first,
            dates: &dates,
            type_name: &type_name,
            row: None,
            tags: Some(&tag_fn),
            rating: None,
        };
        let mut v = rows.to_vec();
        crate::sort::sort_entries_grouped(&mut v, spec, folders_first, type_name, Some(&grouping));
        let (spans, keys) = spans(v.len(), &|i| v.get(i).map(Cow::Borrowed), &grouping, &|_| false);
        let headers = keys.iter().zip(&spans).map(|(k, s)| k.header(s.len)).collect();
        (v.into_iter().map(|e| e.name).collect(), headers)
    }

    #[test]
    fn rows_sort_by_their_front_tag() {
        // Names are the tags: "Green+Red.txt" has both, ".txt" none, "?x" is not read yet.
        let rows = [
            file("zeta+alpha.txt", 0, None),
            file(".b", 0, None),
            file("Green.txt", 0, None),
            file("?x", 0, None),
            file("Green+Red.txt", 0, None),
            file(".a", 0, None),
            file("Alpha.txt", 0, None),
            folder("Red"),
        ];
        let (asc, _) = by_tags(GroupBy::None, SortKey::Tags, SortDir::Asc, true, &rows);
        assert_eq!(
            asc,
            ["Red", "Green+Red.txt", "Green.txt", "Alpha.txt", "zeta+alpha.txt", ".a", ".b", "?x"],
            "folders first; the list's order, unlisted by name, untagged by name, unread last"
        );
        let (desc, _) = by_tags(GroupBy::None, SortKey::Tags, SortDir::Desc, false, &rows);
        assert_eq!(
            desc,
            [".b", ".a", "zeta+alpha.txt", "Alpha.txt", "Green.txt", "Red", "Green+Red.txt", "?x"],
            "turned round, unread still last"
        );
        let (name, _) = by_tags(GroupBy::None, SortKey::Name, SortDir::Asc, false, &rows);
        assert_eq!(name[0], ".a", "another sort ignores the tags");
    }

    #[test]
    fn rows_group_by_their_front_tag_once() {
        let rows = [
            file("Green+Red.txt", 0, None),
            file(".x", 0, None),
            file("Green.txt", 0, None),
            file("beta+Zed.txt", 0, None),
            folder("Red"),
            file("alpha.txt", 0, None),
        ];
        let (names, headers) = by_tags(GroupBy::Tag, SortKey::Name, SortDir::Asc, true, &rows);
        assert_eq!(names, ["Red", "Green+Red.txt", "Green.txt", "alpha.txt", "beta+Zed.txt", ".x"]);
        assert_eq!(
            headers,
            ["● Red (2)", "● Green (1)", "● alpha (1)", "● beta (1)", "No Tags (1)"],
            "one group a row"
        );
        let (_, headers) = by_tags(GroupBy::Tag, SortKey::Tags, SortDir::Desc, true, &rows);
        assert_eq!(headers.first().map(String::as_str), Some("No Tags (1)"), "its own sort's direction");
        let pending = [file("?a", 0, None), file("?b", 0, None)];
        let (_, headers) = by_tags(GroupBy::Tag, SortKey::Name, SortDir::Asc, true, &pending);
        assert_eq!(headers, ["Reading tags… (2)"], "not a false No Tags");
    }

    /// Rows rated by name (`r3…`: three stars, `u…`: not rated, `p…`: not read yet), sorted by
    /// `key` and grouped by `by`.
    fn by_rating(by: GroupBy, key: SortKey, dir: SortDir, rows: &[Entry]) -> (Vec<String>, Vec<String>) {
        let rate = |e: &Entry| match e.name.as_bytes()[0] {
            b'r' => Some(e.name.as_bytes()[1] - b'0'),
            b'u' => Some(0),
            _ => None,
        };
        let dates = DateBounds::default();
        let type_name = |_: &Entry| String::new();
        let spec = SortSpec { key, dir };
        let grouping = Grouping {
            by,
            spec,
            folders_first: false,
            dates: &dates,
            type_name: &type_name,
            row: None,
            tags: None,
            rating: Some(&rate),
        };
        let mut v = rows.to_vec();
        crate::sort::sort_entries_grouped(&mut v, spec, false, type_name, Some(&grouping));
        let (spans, keys) = spans(v.len(), &|i| v.get(i).map(Cow::Borrowed), &grouping, &|_| false);
        let headers = keys.iter().zip(&spans).map(|(k, s)| k.header(s.len)).collect();
        (v.into_iter().map(|e| e.name).collect(), headers)
    }

    #[test]
    fn rows_sort_and_group_by_their_stars() {
        let rows = [
            file("r3a", 0, None),
            file("ub", 0, None),
            file("r5", 0, None),
            file("pz", 0, None),
            file("r3b", 0, None),
            file("ua", 0, None),
            file("r1", 0, None),
        ];
        let (desc, _) = by_rating(GroupBy::None, SortKey::Rating, SortDir::Desc, &rows);
        assert_eq!(desc, ["r5", "r3b", "r3a", "r1", "ub", "ua", "pz"], "most stars first, unread last");
        let (asc, _) = by_rating(GroupBy::None, SortKey::Rating, SortDir::Asc, &rows);
        assert_eq!(asc, ["ua", "ub", "r1", "r3a", "r3b", "r5", "pz"], "by number: not rated is 0; unread still last");
        let (names, headers) = by_rating(GroupBy::Rating, SortKey::Name, SortDir::Asc, &rows);
        assert_eq!(names, ["r5", "r3a", "r3b", "r1", "ua", "ub", "pz"]);
        assert_eq!(headers, ["★★★★★ (1)", "★★★ (2)", "★ (1)", "Not Rated (2)", "Reading tags… (1)"]);
        let (_, own) = by_rating(GroupBy::Rating, SortKey::Rating, SortDir::Desc, &rows);
        assert_eq!(own[0], "★★★★★ (1)", "its own sort descending keeps five stars first");
        let none = Grouping {
            by: GroupBy::Rating,
            spec: SortSpec::default(),
            folders_first: false,
            dates: &DateBounds::default(),
            type_name: &|_: &Entry| String::new(),
            row: None,
            tags: None,
            rating: None,
        };
        assert_eq!(none.key(&rows[0]), Some(GroupKey::Rating(Some(0))), "nothing known: not rated");
    }
}
