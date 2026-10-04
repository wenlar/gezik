//! `views.toml` (this machine only): the view of each folder the user changed. Gezik
//! writes it itself, so single bad entries are skipped without warnings.

use gezik_core::view::{GridSize, SortDir, SortKey, SortSpec, ViewMode, ViewSettings};
use gezik_core::view_memory::FolderView;

/// The saved folder views. A syntax error is an error: the caller starts over.
pub fn parse_views(text: &str) -> Result<Vec<FolderView>, String> {
    let table = text.parse::<toml::Table>().map_err(|err| err.message().to_owned())?;
    let Some(items) = table.get("folder").and_then(|v| v.as_array()) else { return Ok(Vec::new()) };
    let defaults = ViewSettings::default();
    Ok(items
        .iter()
        .filter_map(|item| {
            let item = item.as_table()?;
            let text = |key: &str| item.get(key).and_then(|v| v.as_str());
            Some(FolderView {
                path: text("path")?.to_owned(),
                view: ViewSettings {
                    mode: text("mode").and_then(ViewMode::parse).unwrap_or(defaults.mode),
                    sort: SortSpec {
                        key: text("sort").and_then(SortKey::parse).unwrap_or(defaults.sort.key),
                        dir: text("sort-dir").and_then(SortDir::parse).unwrap_or(defaults.sort.dir),
                    },
                    grid_size: text("grid-size").and_then(GridSize::parse).unwrap_or(defaults.grid_size),
                },
                used: item.get("used").and_then(|v| v.as_integer()).and_then(|u| u64::try_from(u).ok()).unwrap_or(0),
            })
        })
        .collect())
}

pub fn views_to_toml(folders: &[FolderView]) -> String {
    let items = folders
        .iter()
        .map(|f| {
            let mut folder = toml::Table::new();
            let text = |s: &str| toml::Value::String(s.to_owned());
            folder.insert("path".into(), text(&f.path));
            folder.insert("mode".into(), text(f.view.mode.as_str()));
            folder.insert("sort".into(), text(f.view.sort.key.as_str()));
            folder.insert("sort-dir".into(), text(f.view.sort.dir.as_str()));
            folder.insert("grid-size".into(), text(f.view.grid_size.as_str()));
            folder.insert("used".into(), toml::Value::Integer(i64::try_from(f.used).unwrap_or(i64::MAX)));
            toml::Value::Table(folder)
        })
        .collect();
    let mut root = toml::Table::new();
    root.insert("folder".into(), toml::Value::Array(items));
    root.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str, mode: ViewMode, used: u64) -> FolderView {
        FolderView { path: path.to_owned(), view: ViewSettings { mode, ..ViewSettings::default() }, used }
    }

    #[test]
    fn round_trips() {
        let folders = vec![
            folder(r"C:\Pictures", ViewMode::Grid, 7),
            FolderView {
                view: ViewSettings {
                    mode: ViewMode::List,
                    sort: SortSpec { key: SortKey::Modified, dir: SortDir::Desc },
                    grid_size: GridSize::Small,
                },
                ..folder("/home/me/Belgeler", ViewMode::List, 3)
            },
        ];
        assert_eq!(parse_views(&views_to_toml(&folders)).unwrap(), folders);
        assert_eq!(parse_views(&views_to_toml(&[])).unwrap(), []);
    }

    #[test]
    fn skips_bad_entries_and_fills_bad_values() {
        let text = "[[folder]]\nmode = \"grid\"\n\n[[folder]]\npath = \"/a\"\nmode = \"tiles\"\nused = -4\n";
        let folders = parse_views(text).unwrap();
        assert_eq!(folders, [folder("/a", ViewMode::List, 0)]);
    }

    #[test]
    fn syntax_errors_are_errors() {
        assert!(parse_views("[[folder]\npath = ").is_err());
        assert_eq!(parse_views("").unwrap(), []);
    }
}
