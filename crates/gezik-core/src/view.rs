//! How a folder is shown: list or grid, sort order, grid size, and the list's columns.

pub use crate::sort::{SortDir, SortKey, SortSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::List => "list",
            ViewMode::Grid => "grid",
        }
    }

    pub fn parse(text: &str) -> Option<ViewMode> {
        [ViewMode::List, ViewMode::Grid].into_iter().find(|m| m.as_str() == text)
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

/// What a folder remembers: everything the View menu changes for one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ViewSettings {
    pub mode: ViewMode,
    pub sort: SortSpec,
    pub grid_size: GridSize,
}

/// The list's columns after Name (which is always shown and takes the remaining width),
/// in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnKey {
    Modified,
    Created,
    Type,
    Size,
}

impl ColumnKey {
    pub const ALL: [ColumnKey; 4] = [ColumnKey::Modified, ColumnKey::Created, ColumnKey::Type, ColumnKey::Size];

    pub fn as_str(self) -> &'static str {
        match self {
            ColumnKey::Modified => "modified",
            ColumnKey::Created => "created",
            ColumnKey::Type => "type",
            ColumnKey::Size => "size",
        }
    }

    pub fn parse(text: &str) -> Option<ColumnKey> {
        ColumnKey::ALL.into_iter().find(|k| k.as_str() == text)
    }

    pub fn title(self) -> &'static str {
        match self {
            ColumnKey::Modified => "Modified",
            ColumnKey::Created => "Created",
            ColumnKey::Type => "Type",
            ColumnKey::Size => "Size",
        }
    }

    pub fn sort_key(self) -> SortKey {
        match self {
            ColumnKey::Modified => SortKey::Modified,
            ColumnKey::Created => SortKey::Created,
            ColumnKey::Type => SortKey::Type,
            ColumnKey::Size => SortKey::Size,
        }
    }

    /// The column's number in the UI: 0 is Name, then 1–4 in display order.
    pub fn index(self) -> i32 {
        match self {
            ColumnKey::Modified => 1,
            ColumnKey::Created => 2,
            ColumnKey::Type => 3,
            ColumnKey::Size => 4,
        }
    }

    fn default_width(self) -> u32 {
        match self {
            ColumnKey::Modified | ColumnKey::Created => 150,
            ColumnKey::Type => 140,
            ColumnKey::Size => 90,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for mode in [ViewMode::List, ViewMode::Grid] {
            assert_eq!(ViewMode::parse(mode.as_str()), Some(mode));
        }
        for size in [GridSize::Small, GridSize::Medium, GridSize::Large] {
            assert_eq!(GridSize::parse(size.as_str()), Some(size));
        }
        for icons in [IconMode::System, IconMode::Gezik] {
            assert_eq!(IconMode::parse(icons.as_str()), Some(icons));
        }
        for key in ColumnKey::ALL {
            assert_eq!(ColumnKey::parse(key.as_str()), Some(key));
        }
        assert_eq!(ViewMode::parse("tiles"), None);
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
}
