//! The view controls right of each pane's address bar (design 3a, split toolbar): the Layout
//! popup, the Sort menu and the ⋯ menu (widgets/view-popups.slint). Their lines carry the View
//! menu's ids (context_menu.rs), and a choice runs as that menu's item did.

use gezik_config::shortcuts::{Action, Platform};
use gezik_core::group::GroupBy;
use gezik_core::nav::Location;
use gezik_core::view::{GridSize, SortDir, SortKey, ViewMode, ViewOptions, ViewSettings};
use gezik_i18n::{t, tx};
use slint::{ModelRc, VecModel};

use crate::context_menu::*;
use crate::{AppWindow, MenuEntry, ViewLine};

/// The popups, as the toolbar's buttons name them.
pub const LAYOUT: i32 = 0;
pub const SORT: i32 = 1;

fn line(id: u32, title: &str, kind: i32, on: bool, keys: String, enabled: bool) -> ViewLine {
    ViewLine { id: i32::try_from(id).unwrap_or(0), title: title.into(), kind, on, keys: keys.into(), enabled }
}

fn item(id: u32, title: &str, on: bool) -> ViewLine {
    line(id, title, 0, on, String::new(), true)
}

fn divider() -> ViewLine {
    line(HEADING, "", 2, false, String::new(), false)
}

/// The sort's name on the Sort button and in its menu.
pub fn sort_name(key: SortKey) -> String {
    let title = match key {
        SortKey::Name => tx!("Name"),
        SortKey::Modified => tx!("Date modified"),
        SortKey::Created => tx!("Date created"),
        SortKey::Type => tx!("Type"),
        SortKey::Size => tx!("Size"),
        SortKey::Folder => tx!("Folder"),
        SortKey::Tags => tx!("Tags"),
        SortKey::Rating => tx!("Rating"),
        SortKey::Git => tx!("Git"),
        SortKey::Field(id) => id.def().label,
    };
    t("", title).into_owned()
}

/// The Layout popup: the layouts, the icon sizes (only the grid has them) and Flat view
/// (`flat`: whether it is on; `None` where there is no folder to flatten).
pub fn layout_lines(view: ViewSettings, flat: Option<bool>) -> [Vec<ViewLine>; 3] {
    let grid = view.mode == ViewMode::Grid;
    let size = |id, title: &str, size| line(id, title, 0, grid && view.grid_size == size, String::new(), grid);
    [
        vec![
            item(VIEW_LIST, &t!("List"), view.mode == ViewMode::List),
            item(VIEW_GRID, &t!("Grid"), grid),
            item(VIEW_COLUMNS, &t!("Columns"), view.mode == ViewMode::Columns),
        ],
        vec![
            size(GRID_SMALL, &t!("Small"), GridSize::Small),
            size(GRID_MEDIUM, &t!("Medium"), GridSize::Medium),
            size(GRID_LARGE, &t!("Large"), GridSize::Large),
        ],
        vec![line(FLAT_VIEW, &t!("Flat view"), 0, flat == Some(true), String::new(), flat.is_some())],
    ]
}

/// The Sort menu: the keys, the direction, then Group by ▸ (what it is now) and Folders first.
pub fn sort_lines(view: ViewSettings, folders_first: bool) -> [Vec<ViewLine>; 3] {
    let key = view.sort.key;
    let mut keys = vec![line(HEADING, &t!("Sort by"), 1, false, String::new(), false)];
    let named = [
        (SORT_BY_NAME, SortKey::Name),
        (SORT_BY_MODIFIED, SortKey::Modified),
        (SORT_BY_CREATED, SortKey::Created),
        (SORT_BY_TYPE, SortKey::Type),
        (SORT_BY_SIZE, SortKey::Size),
        (SORT_BY_TAGS, SortKey::Tags),
        (SORT_BY_RATING, SortKey::Rating),
    ];
    keys.extend(named.into_iter().map(|(id, k)| item(id, &sort_name(k), k == key)));
    let property = match key {
        SortKey::Field(_) => format!("{}…", sort_name(key)),
        _ => t!("Property…").into_owned(),
    };
    keys.push(item(SORT_BY_MORE, &property, matches!(key, SortKey::Field(_))));
    let group = match view.group {
        GroupBy::Field(id) => t("", id.def().label),
        by => t("", by.title()),
    };
    [
        keys,
        vec![
            item(SORT_ASC, &t!("Ascending"), view.sort.dir == SortDir::Asc),
            item(SORT_DESC, &t!("Descending"), view.sort.dir == SortDir::Desc),
        ],
        // Its id is none: the line opens Group by ▸ (`Menus::group_menu`).
        vec![
            line(HEADING, &t!("Group by"), 3, false, group.into_owned(), true),
            item(FOLDERS_FIRST, &t!("Folders first"), folders_first),
        ],
    ]
}

/// The ⋯ menu: what the folder shows, its view for every folder, the two panes' commands
/// (`dual`: open, and whether they browse together), then what has no better place. `windows`:
/// "Show system items" there only; `folder`: a folder (Calculate folder sizes); `rule`: a view
/// rule's number and whether it set this folder's view (spec 10 §8.2); `keys`: an action's
/// shortcut as shown.
pub fn more_lines(
    options: ViewOptions,
    windows: bool,
    folder: bool,
    rule: Option<(usize, bool)>,
    dual: Option<bool>,
    keys: &dyn Fn(Action) -> String,
) -> Vec<ViewLine> {
    let keyed = |id, title: &str, on, action| line(id, title, 0, on, keys(action), true);
    let mut out = vec![keyed(SHOW_HIDDEN, &t!("Show hidden items"), options.show_hidden, Action::ToggleHidden)];
    if windows {
        out.push(item(SHOW_SYSTEM, &t!("Show system items"), options.show_system));
    }
    out.push(item(HIDE_EXTENSIONS, &t!("Show file extensions"), !options.hide_extensions));
    if folder {
        out.push(item(CALC_FOLDER_SIZES, &t!("Calculate folder sizes"), false));
    }
    out.push(divider());
    out.push(item(APPLY_TO_ALL, &t!("Use this view everywhere"), false));
    match rule {
        // The rule set it: which one, greyed, then Reset this folder.
        Some((number, true)) => {
            out.push(line(VIEW_RULE_APPLIES, &t!("View rule {} applies", number), 0, false, String::new(), false));
            out.push(item(RESET_FOLDER, &t!("Reset this folder"), false));
        }
        // The folder has its own view and the rule would apply without it.
        Some((number, false)) => out.push(item(RESET_TO_RULE, &t!("Reset to rule {}", number), false)),
        None => out.push(item(RESET_FOLDER, &t!("Reset this folder"), false)),
    }
    if let Some(synced) = dual {
        out.push(divider());
        out.push(item(SYNC_BROWSING, &t!("Sync browsing"), synced));
        out.push(keyed(SWAP_PANES, &t!("Swap panes"), false, Action::SwapPanes));
        out.push(item(SAME_FOLDER, &t!("Same folder in other pane"), false));
    }
    out.push(divider());
    out.push(item(REVEAL_IN_TREE, &t!("Show in sidebar tree"), false));
    out.push(keyed(SETTINGS, &t!("Settings…"), false, Action::Settings));
    out.push(item(SYSTEM_INTEGRATION, &t!("System Integration…"), false));
    out
}

/// A panel toggle's right-click menu: `first` or `second` (`on`: the second is in use), the
/// one in use marked and greyed; choosing the other flips it (`id`).
pub fn layout_choices(id: u32, on: bool, first: &str, second: &str) -> Vec<(u32, String, bool)> {
    let mark = |on: bool, title: &str| format!("{}{title}", if on { "• " } else { "    " });
    vec![(id, mark(!on, first), on), (id, mark(on, second), !on)]
}

/// `action`'s shortcut as the settings write it on this platform ("" none).
fn shortcut(action: Action) -> String {
    crate::keys::chord_for(action)
        .map(|chord| crate::keys::chord_label(&chord, Platform::current()))
        .unwrap_or_default()
}

/// The Layout popup's footer: the keys that switch the layout.
fn layout_hint() -> String {
    let keys: Vec<String> = [Action::ViewList, Action::ViewGrid, Action::ViewColumns]
        .into_iter()
        .map(shortcut)
        .filter(|k| !k.is_empty())
        .collect();
    if keys.is_empty() { String::new() } else { t!("{} switch layout", keys.join(" / ")) }
}

/// Puts popup `which` of the active pane in the window (`show-view-popup` opens it); again
/// after a choice, so the Layout popup shows it.
pub fn fill(window: &AppWindow, which: i32) {
    let view = crate::panes::active_view();
    let settings = view.view_settings();
    let options = crate::view_options::current();
    let location = crate::panes::active_nav().active_location();
    let parts = match which {
        LAYOUT => layout_lines(settings, location.folder().is_some().then_some(matches!(location, Location::Flat(_)))),
        SORT => sort_lines(settings, options.folders_first),
        _ => {
            let folder = matches!(location, Location::Path(_));
            let dual = crate::dual::is_open().then(crate::dual::is_synced);
            [more_lines(options, cfg!(windows), folder, view.rule_state(), dual, &shortcut), Vec::new(), Vec::new()]
        }
    };
    let keys: Vec<MenuEntry> = parts
        .iter()
        .flatten()
        .map(|l| MenuEntry { id: l.id, title: l.title.clone(), enabled: l.enabled && (l.kind == 0 || l.kind == 3) })
        .collect();
    let model = |lines: &Vec<ViewLine>| ModelRc::new(VecModel::from(lines.clone()));
    window.set_vp_kind(i32::from(which != LAYOUT));
    window.set_vp_hint(if which == LAYOUT { layout_hint() } else { String::new() }.into());
    window.set_vp_lines(model(&parts[0]));
    window.set_vp_segments(model(&parts[1]));
    window.set_vp_after(model(&parts[2]));
    window.set_vp_keys(ModelRc::new(VecModel::from(keys)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::view::SortSpec;

    fn ids(lines: &[ViewLine]) -> Vec<u32> {
        lines.iter().map(|l| u32::try_from(l.id).unwrap_or(0)).collect()
    }

    fn marked(lines: &[ViewLine]) -> Vec<&str> {
        lines.iter().filter(|l| l.on).map(|l| l.title.as_str()).collect()
    }

    #[test]
    fn the_layout_popup_marks_the_layout_and_offers_sizes_in_the_grid_only() {
        let [layouts, sizes, flat] = layout_lines(ViewSettings::default(), Some(false));
        assert_eq!(ids(&layouts), [VIEW_LIST, VIEW_GRID, VIEW_COLUMNS]);
        assert_eq!(marked(&layouts), ["List"]);
        assert!(sizes.iter().all(|l| !l.enabled && !l.on), "the list has no icon sizes");
        assert_eq!((ids(&flat), flat[0].on, flat[0].enabled), (vec![FLAT_VIEW], false, true));
        let grid = ViewSettings { mode: ViewMode::Grid, grid_size: GridSize::Large, ..ViewSettings::default() };
        let [layouts, sizes, flat] = layout_lines(grid, None);
        assert_eq!(marked(&layouts), ["Grid"]);
        assert_eq!(ids(&sizes), [GRID_SMALL, GRID_MEDIUM, GRID_LARGE]);
        assert_eq!(marked(&sizes), ["Large"]);
        assert!(!flat[0].enabled, "nothing to flatten here");
    }

    #[test]
    fn the_sort_menu_lists_every_key_and_marks_the_current_one() {
        let view = ViewSettings {
            sort: SortSpec { key: SortKey::Size, dir: SortDir::Desc },
            group: GroupBy::Type,
            ..ViewSettings::default()
        };
        let [keys, dirs, after] = sort_lines(view, true);
        assert_eq!(keys[0].kind, 1, "a heading first");
        assert_eq!(
            ids(&keys[1..]),
            [
                SORT_BY_NAME,
                SORT_BY_MODIFIED,
                SORT_BY_CREATED,
                SORT_BY_TYPE,
                SORT_BY_SIZE,
                SORT_BY_TAGS,
                SORT_BY_RATING,
                SORT_BY_MORE
            ]
        );
        assert_eq!(marked(&keys), ["Size"]);
        assert_eq!(ids(&dirs), [SORT_ASC, SORT_DESC]);
        assert_eq!(marked(&dirs), ["Descending"]);
        assert_eq!((after[0].kind, after[0].keys.as_str()), (3, "Type"), "Group by ▸ says what it is");
        assert_eq!((ids(&after[1..]), after[1].on), (vec![FOLDERS_FIRST], true));
        let by_duration = ViewSettings {
            sort: SortSpec { key: SortKey::Field(gezik_core::fields::DURATION), dir: SortDir::Asc },
            ..ViewSettings::default()
        };
        let [keys, ..] = sort_lines(by_duration, false);
        assert_eq!(marked(&keys), ["Duration…"]);
        assert_eq!(sort_name(SortKey::Field(gezik_core::fields::DURATION)), "Duration");
    }

    #[test]
    fn the_more_menu_keeps_every_view_command_the_view_menu_had() {
        let none = |_| String::new();
        let options = ViewOptions { hide_extensions: true, show_hidden: true, ..ViewOptions::default() };
        let lines = more_lines(options, true, true, None, Some(true), &none);
        let chosen: Vec<u32> = lines.iter().filter(|l| l.kind == 0).map(|l| u32::try_from(l.id).unwrap_or(0)).collect();
        assert_eq!(
            chosen,
            [
                SHOW_HIDDEN,
                SHOW_SYSTEM,
                HIDE_EXTENSIONS,
                CALC_FOLDER_SIZES,
                APPLY_TO_ALL,
                RESET_FOLDER,
                SYNC_BROWSING,
                SWAP_PANES,
                SAME_FOLDER,
                REVEAL_IN_TREE,
                SETTINGS,
                SYSTEM_INTEGRATION
            ]
        );
        assert_eq!(marked(&lines), ["Show hidden items", "Sync browsing"], "extensions hidden: not shown");
        let plain = more_lines(options, false, false, None, None, &none);
        let plain = ids(&plain);
        assert!(!plain.contains(&SHOW_SYSTEM) && !plain.contains(&CALC_FOLDER_SIZES) && !plain.contains(&SWAP_PANES));
        let keyed = more_lines(options, false, false, None, None, &|a| {
            if a == Action::Settings { "Ctrl+,".into() } else { String::new() }
        });
        assert!(keyed.iter().any(|l| u32::try_from(l.id) == Ok(SETTINGS) && l.keys == "Ctrl+,"));
    }

    #[test]
    fn the_more_menu_says_which_rule_applies_or_offers_it_back() {
        let none = |_| String::new();
        let applies = more_lines(ViewOptions::default(), false, false, Some((2, true)), None, &none);
        let at = applies.iter().position(|l| u32::try_from(l.id) == Ok(VIEW_RULE_APPLIES)).unwrap();
        assert_eq!((applies[at].title.as_str(), applies[at].enabled), ("View rule 2 applies", false), "greyed");
        assert_eq!(u32::try_from(applies[at + 1].id), Ok(RESET_FOLDER));
        let own = ids(&more_lines(ViewOptions::default(), false, false, Some((3, false)), None, &none));
        assert!(own.contains(&RESET_TO_RULE) && !own.contains(&RESET_FOLDER) && !own.contains(&VIEW_RULE_APPLIES));
    }

    #[test]
    fn a_panel_toggle_offers_the_other_place_only() {
        let right = layout_choices(PREVIEW_BOTTOM, false, "Right", "Bottom");
        assert_eq!(
            right,
            [(PREVIEW_BOTTOM, "• Right".to_owned(), false), (PREVIEW_BOTTOM, "    Bottom".to_owned(), true)]
        );
        let bottom = layout_choices(PREVIEW_BOTTOM, true, "Right", "Bottom");
        assert_eq!((bottom[0].2, bottom[1].1.as_str()), (true, "• Bottom"));
    }
}
