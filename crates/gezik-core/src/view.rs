//! How a folder is shown: list, grid or columns, sort order, grid size, and the list's columns.

pub use crate::sort::{SortDir, SortKey, SortSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
    /// Miller columns (spec 10 §7); only a folder shows them (`columns::shows`).
    Columns,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::List => "list",
            ViewMode::Grid => "grid",
            ViewMode::Columns => "columns",
        }
    }

    pub fn parse(text: &str) -> Option<ViewMode> {
        [ViewMode::List, ViewMode::Grid, ViewMode::Columns].into_iter().find(|m| m.as_str() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GridSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl GridSize {
    pub fn as_str(self) -> &'static str {
        match self {
            GridSize::Small => "small",
            GridSize::Medium => "medium",
            GridSize::Large => "large",
        }
    }

    pub fn parse(text: &str) -> Option<GridSize> {
        [GridSize::Small, GridSize::Medium, GridSize::Large].into_iter().find(|s| s.as_str() == text)
    }

    /// The picture size in a grid cell, in logical pixels.
    pub fn px(self) -> u32 {
        match self {
            GridSize::Small => 48,
            GridSize::Medium => 96,
            GridSize::Large => 192,
        }
    }

    pub fn bigger(self) -> GridSize {
        match self {
            GridSize::Small => GridSize::Medium,
            GridSize::Medium | GridSize::Large => GridSize::Large,
        }
    }

    pub fn smaller(self) -> GridSize {
        match self {
            GridSize::Large => GridSize::Medium,
            GridSize::Medium | GridSize::Small => GridSize::Small,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IconMode {
    /// The operating system's icons (as in Explorer).
    #[default]
    System,
    /// Gezik's own icons, colored by the theme.
    Gezik,
}

impl IconMode {
    pub fn as_str(self) -> &'static str {
        match self {
            IconMode::System => "system",
            IconMode::Gezik => "gezik",
        }
    }

    pub fn parse(text: &str) -> Option<IconMode> {
        [IconMode::System, IconMode::Gezik].into_iter().find(|m| m.as_str() == text)
    }
}

/// How the date columns and the preview write a time (`[view] date-format`, spec 7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DateFormat {
    /// `5 min ago`, `Today 14:05`, `Yesterday 14:05`, older ones as `System`.
    Relative,
    /// The date only.
    Short,
    /// `YYYY-MM-DD HH:MM` everywhere.
    Iso,
    /// The system's short date and time (Windows), `YYYY-MM-DD HH:MM` elsewhere.
    #[default]
    System,
}

impl DateFormat {
    /// In menu order (the menu ids follow it).
    pub const ALL: [DateFormat; 4] = [DateFormat::Relative, DateFormat::Short, DateFormat::Iso, DateFormat::System];

    pub fn as_str(self) -> &'static str {
        match self {
            DateFormat::Relative => "relative",
            DateFormat::Short => "short",
            DateFormat::Iso => "iso",
            DateFormat::System => "system",
        }
    }

    pub fn parse(text: &str) -> Option<DateFormat> {
        DateFormat::ALL.into_iter().find(|f| f.as_str() == text)
    }

    /// Its line in the Date format menu.
    pub fn label(self) -> &'static str {
        match self {
            DateFormat::Relative => "Relative",
            DateFormat::Short => "Short",
            DateFormat::Iso => "ISO",
            DateFormat::System => "System",
        }
    }
}

/// How sizes are written (`[view] size-format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SizeFormat {
    /// Steps of 1024, `KB`, `MB`, `GB` (as Explorer).
    #[default]
    Binary,
    /// Steps of 1000, `kB`, `MB`, `GB` (as Finder and GNOME).
    Decimal,
}

impl SizeFormat {
    pub const ALL: [SizeFormat; 2] = [SizeFormat::Binary, SizeFormat::Decimal];

    pub fn as_str(self) -> &'static str {
        match self {
            SizeFormat::Binary => "binary",
            SizeFormat::Decimal => "decimal",
        }
    }

    pub fn parse(text: &str) -> Option<SizeFormat> {
        SizeFormat::ALL.into_iter().find(|f| f.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            SizeFormat::Binary => "Binary (1 KB = 1024 bytes)",
            SizeFormat::Decimal => "Decimal (1 kB = 1000 bytes)",
        }
    }
}

/// `[view]`'s options that are the same in every folder (spec 7.1); `views.toml` does not
/// keep them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewOptions {
    pub hide_extensions: bool,
    pub folders_first: bool,
    pub date_format: DateFormat,
    pub size_format: SizeFormat,
    pub single_click_open: bool,
    pub show_hidden: bool,
    /// Windows only: the protected system items.
    pub show_system: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            hide_extensions: false,
            folders_first: true,
            date_format: DateFormat::System,
            size_format: SizeFormat::Binary,
            single_click_open: false,
            // Finder hides them; Explorer users have their dot files in sight today.
            show_hidden: !cfg!(target_os = "macos"),
            show_system: false,
        }
    }
}

/// Days since 1970-01-01 of a calendar date (Howard Hinnant's `days_from_civil`).
pub(crate) fn day_number(date: &crate::batch::date::DateParts) -> i64 {
    let (month, day) = (i64::from(date.month), i64::from(date.day));
    let year = i64::from(date.year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `then` (local) as `relative` writes it, `now` being the local time now and `age_secs` how
/// long ago `then` was: `5 min ago` within the hour (at least 1), `Today 14:05`, `Yesterday
/// 14:05`; `None` (the system's format) for older times and times in the future.
pub fn relative_date(
    now: &crate::batch::date::DateParts,
    then: &crate::batch::date::DateParts,
    age_secs: i64,
) -> Option<String> {
    if age_secs < 0 {
        return None;
    }
    if age_secs < 3600 {
        return Some(format!("{} min ago", (age_secs / 60).max(1)));
    }
    let clock = format!("{:02}:{:02}", then.hour, then.minute);
    match day_number(now) - day_number(then) {
        0 => Some(format!("Today {clock}")),
        1 => Some(format!("Yesterday {clock}")),
        _ => None,
    }
}

/// What a folder remembers: everything the View menu changes for one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ViewSettings {
    pub mode: ViewMode,
    pub sort: SortSpec,
    pub grid_size: GridSize,
    /// Group headers (spec 10 §6); none by default.
    pub group: crate::group::GroupBy,
}

/// The list's columns after Name (which is always shown and takes the remaining width),
/// in display order. Folder and Match are the search results' own (spec 4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnKey {
    Modified,
    Created,
    Type,
    Size,
    /// The folder a result is in, under the search's scope.
    Folder,
    /// A content search's first matching line.
    Match,
}

impl ColumnKey {
    /// A folder's columns.
    pub const ALL: [ColumnKey; 4] = [ColumnKey::Modified, ColumnKey::Created, ColumnKey::Type, ColumnKey::Size];
    /// The search results' and the flat view's columns, in display order.
    pub const RESULTS: [ColumnKey; 6] = [
        ColumnKey::Folder,
        ColumnKey::Match,
        ColumnKey::Modified,
        ColumnKey::Created,
        ColumnKey::Type,
        ColumnKey::Size,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ColumnKey::Modified => "modified",
            ColumnKey::Created => "created",
            ColumnKey::Type => "type",
            ColumnKey::Size => "size",
            ColumnKey::Folder => "folder",
            ColumnKey::Match => "match",
        }
    }

    pub fn parse(text: &str) -> Option<ColumnKey> {
        ColumnKey::RESULTS.into_iter().find(|k| k.as_str() == text)
    }

    pub fn title(self) -> &'static str {
        match self {
            ColumnKey::Modified => "Modified",
            ColumnKey::Created => "Created",
            ColumnKey::Type => "Type",
            ColumnKey::Size => "Size",
            ColumnKey::Folder => "Folder",
            ColumnKey::Match => "Match",
        }
    }

    /// The sort its header click gives; Match does not sort (the view ignores its click).
    pub fn sort_key(self) -> SortKey {
        match self {
            ColumnKey::Modified => SortKey::Modified,
            ColumnKey::Created => SortKey::Created,
            ColumnKey::Type => SortKey::Type,
            ColumnKey::Size => SortKey::Size,
            ColumnKey::Folder => SortKey::Folder,
            ColumnKey::Match => SortKey::Name,
        }
    }

    /// The column's number in the UI: 0 is Name, then 1–4 the folder columns, 5 Folder, 6 Match.
    pub fn index(self) -> i32 {
        match self {
            ColumnKey::Modified => 1,
            ColumnKey::Created => 2,
            ColumnKey::Type => 3,
            ColumnKey::Size => 4,
            ColumnKey::Folder => 5,
            ColumnKey::Match => 6,
        }
    }

    fn default_width(self) -> u32 {
        match self {
            ColumnKey::Modified | ColumnKey::Created => 150,
            ColumnKey::Type => 140,
            ColumnKey::Size => 90,
            ColumnKey::Folder => 220,
            ColumnKey::Match => 320,
        }
    }
}

pub const MIN_COLUMN_WIDTH: u32 = 50;
pub const MAX_COLUMN_WIDTH: u32 = 600;
pub const MIN_NAME_WIDTH: u32 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnState {
    pub key: ColumnKey,
    pub visible: bool,
    /// Logical pixels.
    pub width: u32,
}

/// Modified, Type and Size shown; Created hidden.
pub fn default_columns() -> Vec<ColumnState> {
    ColumnKey::ALL
        .into_iter()
        .map(|key| ColumnState { key, visible: key != ColumnKey::Created, width: key.default_width() })
        .collect()
}

/// `saved` (from `state.toml`, possibly edited by hand) made complete: every column once,
/// in display order, widths within range; missing columns get their defaults.
pub fn normalize_columns(saved: &[ColumnState]) -> Vec<ColumnState> {
    default_columns()
        .into_iter()
        .map(|default| match saved.iter().find(|c| c.key == default.key) {
            Some(c) => ColumnState { width: c.width.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH), ..*c },
            None => default,
        })
        .collect()
}

/// The search results' columns: Folder, Match, Modified and Size shown (spec 4.5).
pub fn default_result_columns() -> Vec<ColumnState> {
    ColumnKey::RESULTS
        .into_iter()
        .map(|key| ColumnState {
            key,
            visible: matches!(key, ColumnKey::Folder | ColumnKey::Match | ColumnKey::Modified | ColumnKey::Size),
            width: key.default_width(),
        })
        .collect()
}

/// [`normalize_columns`] for the results' columns.
pub fn normalize_result_columns(saved: &[ColumnState]) -> Vec<ColumnState> {
    default_result_columns()
        .into_iter()
        .map(|default| match saved.iter().find(|c| c.key == default.key) {
            Some(c) => ColumnState { width: c.width.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH), ..*c },
            None => default,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for mode in [ViewMode::List, ViewMode::Grid, ViewMode::Columns] {
            assert_eq!(ViewMode::parse(mode.as_str()), Some(mode));
        }
        for size in [GridSize::Small, GridSize::Medium, GridSize::Large] {
            assert_eq!(GridSize::parse(size.as_str()), Some(size));
        }
        for icons in [IconMode::System, IconMode::Gezik] {
            assert_eq!(IconMode::parse(icons.as_str()), Some(icons));
        }
        for key in ColumnKey::RESULTS {
            assert_eq!(ColumnKey::parse(key.as_str()), Some(key));
        }
        assert_eq!(ViewMode::parse("tiles"), None);
    }

    use crate::batch::date::DateParts;

    fn at(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> DateParts {
        DateParts { year, month, day, hour, minute, second: 0 }
    }

    #[test]
    fn relative_dates_go_by_the_minute_the_day_and_yesterday() {
        let now = at(2026, 10, 8, 14, 30);
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 14, 25), 300).as_deref(), Some("5 min ago"));
        assert_eq!(relative_date(&now, &now, 20).as_deref(), Some("1 min ago"), "under a minute");
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 13, 31), 3599).as_deref(), Some("59 min ago"));
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 13, 30), 3600).as_deref(), Some("Today 13:30"));
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 0, 5), 52_500).as_deref(), Some("Today 00:05"));
        assert_eq!(relative_date(&now, &at(2026, 10, 7, 23, 59), 52_260).as_deref(), Some("Yesterday 23:59"));
        assert_eq!(relative_date(&now, &at(2026, 10, 6, 9, 0), 192_600), None, "older: the system's");
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 15, 0), -1800), None, "in the future: the system's");
        // Across a month, a year and a leap day.
        let new_year = at(2027, 1, 1, 0, 10);
        assert_eq!(relative_date(&new_year, &at(2026, 12, 31, 22, 0), 7800).as_deref(), Some("Yesterday 22:00"));
        let march = at(2024, 3, 1, 9, 0);
        assert_eq!(relative_date(&march, &at(2024, 2, 29, 8, 0), 90_000).as_deref(), Some("Yesterday 08:00"));
    }

    #[test]
    fn format_names_round_trip() {
        for format in DateFormat::ALL {
            assert_eq!(DateFormat::parse(format.as_str()), Some(format));
        }
        for format in SizeFormat::ALL {
            assert_eq!(SizeFormat::parse(format.as_str()), Some(format));
        }
        assert_eq!(DateFormat::parse("weekday"), None);
        assert_eq!(SizeFormat::parse("Binary"), None, "as written in settings.toml: lower case");
        let options = ViewOptions::default();
        assert!(options.folders_first && !options.hide_extensions && !options.single_click_open);
        assert_eq!((options.date_format, options.size_format), (DateFormat::System, SizeFormat::Binary));
        assert_eq!(options.show_hidden, !cfg!(target_os = "macos"));
        assert!(!options.show_system);
    }

    #[test]
    fn grid_sizes_step_and_stop_at_the_ends() {
        assert_eq!(GridSize::Small.bigger(), GridSize::Medium);
        assert_eq!(GridSize::Large.bigger(), GridSize::Large);
        assert_eq!(GridSize::Small.smaller(), GridSize::Small);
        assert_eq!((GridSize::Small.px(), GridSize::Medium.px(), GridSize::Large.px()), (48, 96, 192));
    }

    #[test]
    fn default_view_is_a_name_sorted_list() {
        let v = ViewSettings::default();
        assert_eq!((v.mode, v.sort, v.grid_size), (ViewMode::List, SortSpec::default(), GridSize::Medium));
    }

    #[test]
    fn columns_are_normalized() {
        let saved = [
            ColumnState { key: ColumnKey::Size, visible: false, width: 5 },
            ColumnState { key: ColumnKey::Modified, visible: true, width: 9000 },
            ColumnState { key: ColumnKey::Size, visible: true, width: 80 },
        ];
        let columns = normalize_columns(&saved);
        assert_eq!(columns.iter().map(|c| c.key).collect::<Vec<_>>(), ColumnKey::ALL);
        assert_eq!(columns[0], ColumnState { key: ColumnKey::Modified, visible: true, width: MAX_COLUMN_WIDTH });
        assert_eq!(columns[1], ColumnState { key: ColumnKey::Created, visible: false, width: 150 }, "missing: default");
        assert_eq!(
            columns[3],
            ColumnState { key: ColumnKey::Size, visible: false, width: MIN_COLUMN_WIDTH },
            "first wins"
        );
    }

    #[test]
    fn result_columns_have_folder_and_match() {
        let columns = default_result_columns();
        assert_eq!(columns.iter().map(|c| c.key).collect::<Vec<_>>(), ColumnKey::RESULTS);
        let shown: Vec<ColumnKey> = columns.iter().filter(|c| c.visible).map(|c| c.key).collect();
        assert_eq!(shown, [ColumnKey::Folder, ColumnKey::Match, ColumnKey::Modified, ColumnKey::Size]);
        for key in ColumnKey::RESULTS {
            assert_eq!(ColumnKey::parse(key.as_str()), Some(key));
        }
        assert_eq!((ColumnKey::Folder.index(), ColumnKey::Match.index()), (5, 6));
        assert_eq!(ColumnKey::Folder.sort_key(), SortKey::Folder);
        let saved = [ColumnState { key: ColumnKey::Folder, visible: false, width: 9 }];
        let normal = normalize_result_columns(&saved);
        assert_eq!(normal[0], ColumnState { key: ColumnKey::Folder, visible: false, width: MIN_COLUMN_WIDTH });
        assert_eq!(normal.len(), 6);
        assert_eq!(normalize_columns(&saved), default_columns(), "a folder list never gets them");
    }
}
