//! Which Gezik items a context menu shows, and what they do.
//!
//! On Windows, rows and sidebar entries get the Explorer menu with Gezik's items on top;
//! tabs (and everything on macOS/Linux) get a Slint menu.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gezik_config::settings::ViewOption;
use gezik_core::drag::Effect;
use gezik_core::nav::Location;
use gezik_core::path_text::PathFormat;
use gezik_core::search::{DateRange, Scope, SearchSpec};
use gezik_core::templates::{LinkKind, PasteKind, Template};
use gezik_core::view::{
    ColumnKey, ColumnState, DateFormat, GridSize, SizeFormat, SortDir, SortKey, SortSpec, ViewMode, ViewOptions,
    ViewSettings,
};
use gezik_platform::MenuTarget;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::Navigator;
use crate::operations::Operations;
use crate::popup::Anchor;
use crate::preview::Preview;
use crate::sidebar::{SECTION_GROUP, SECTION_PINNED, Sidebar};
use crate::view::View;
use crate::{AppWindow, MenuEntry, MenuSub};

pub const OPEN_IN_NEW_TAB: u32 = 1;
pub const PIN: u32 = 2;
pub const UNPIN: u32 = 3;
pub const MOVE_UP: u32 = 4;
pub const MOVE_DOWN: u32 = 5;
pub const DUPLICATE_TAB: u32 = 6;
pub const CLOSE_TAB: u32 = 7;
pub const CLOSE_OTHER_TABS: u32 = 8;
/// macOS/Linux only.
pub const OPEN: u32 = 9;
/// macOS/Linux only.
pub const OPEN_DEFAULT: u32 = 10;
pub const LOCK_TAB: u32 = 11;
pub const UNLOCK_TAB: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A row in the file list.
    Row {
        is_dir: bool,
        pinned: bool,
    },
    /// Several selected rows.
    Rows,
    /// A sidebar entry; `pinned_section` = it is in the PINNED section.
    Sidebar {
        pinned_section: bool,
        pinned: bool,
        first: bool,
        last: bool,
    },
    Tab {
        only_tab: bool,
        locked: bool,
    },
}

/// Gezik's items for `place`. `native_shell` = the Windows Explorer menu is shown below them.
pub fn items(place: Place, native_shell: bool) -> Vec<(u32, &'static str)> {
    let mut out = Vec::new();
    match place {
        Place::Row { is_dir: true, pinned } => {
            out.push((OPEN_IN_NEW_TAB, "Open in new tab"));
            out.push(pin_toggle(pinned));
        }
        Place::Row { is_dir: false, .. } => {
            // On Windows the Explorer menu already opens files ("Open", "Open with").
            if !native_shell {
                out.push((OPEN, "Open"));
                out.push((OPEN_DEFAULT, "Open with default app"));
            }
        }
        // The Explorer menu acts on all of them; elsewhere Gezik can open them.
        Place::Rows => {
            if !native_shell {
                out.push((OPEN, "Open"));
            }
        }
        Place::Sidebar { pinned_section, pinned, first, last } => {
            out.push((OPEN_IN_NEW_TAB, "Open in new tab"));
            out.push(pin_toggle(pinned));
            if pinned_section && !first {
                out.push((MOVE_UP, "Move up"));
            }
            if pinned_section && !last {
                out.push((MOVE_DOWN, "Move down"));
            }
            if pinned_section {
                out.push((RENAME_PIN, "Rename…"));
            }
        }
        Place::Tab { only_tab, locked } => {
            out.push((DUPLICATE_TAB, "Duplicate"));
            if locked {
                out.push((UNLOCK_TAB, "Unlock tab"));
            } else {
                out.push((LOCK_TAB, "Lock tab"));
                out.push((CLOSE_TAB, "Close"));
            }
            if !only_tab {
                out.push((CLOSE_OTHER_TABS, "Close other tabs"));
            }
            out.push((SAVE_TAB_SET, "Save tabs as…"));
        }
    }
    out
}

/// 8a's ids are 1500-1599 (spec 9.4). 1500: Search in this folder… / Search in "name"…; 1505:
/// View ▸ Flat view; 1506-1508: the search bar's ▾ (in a new tab, Clear, the folders it could
/// not read); 1510-1519: its scope menu by place; 1520-1529: Filters ▸ Modified, sizes, Clear;
/// 1530-1538: Filters ▸ Type in `KindFilter::ALL` order; 1540-1544: Filters' options.
pub const SEARCH_HERE: u32 = 1500;
/// 1501-1504: a search result's rows (spec 4.6, 9.4).
pub const SHOW_IN_FOLDER: u32 = 1501;
pub const SHOW_IN_FOLDER_NEW_TAB: u32 = 1502;
pub const COPY_WITH_FOLDERS: u32 = 1503;
pub const CUT_WITH_FOLDERS: u32 = 1504;
pub const FLAT_VIEW: u32 = 1505;
pub const SEARCH_NEW_TAB: u32 = 1506;
pub const SEARCH_CLEAR: u32 = 1507;
pub const SEARCH_PROBLEMS: u32 = 1508;
pub const SCOPE_FIRST: u32 = 1510;
pub const SCOPE_MAX: u32 = 10;
pub const MODIFIED_FIRST: u32 = 1520;
pub const SIZE_MIN: u32 = 1526;
pub const SIZE_MAX: u32 = 1527;
pub const MODIFIED_BETWEEN: u32 = 1528;
pub const FILTERS_CLEAR: u32 = 1529;
pub const KIND_FIRST: u32 = 1530;
pub const OPTION_FIRST: u32 = 1540;

/// The Modified presets, by id from `MODIFIED_FIRST`.
const MODIFIED_PRESETS: [DateRange; 5] =
    [DateRange::Any, DateRange::Today, DateRange::LastDays(7), DateRange::LastDays(30), DateRange::ThisYear];

/// The date a Modified item sets; `None` for "Between…" (asked in a box) and other ids.
pub fn modified_for(id: u32) -> Option<DateRange> {
    MODIFIED_PRESETS.get(id.checked_sub(MODIFIED_FIRST)? as usize).copied()
}

fn marked(on: bool, title: &str) -> String {
    format!("{}{title}", if on { "• " } else { "    " })
}

/// The search bar's Filters menu (sapma 2): the sizes, the options, Clear; Modified ▸ and Type ▸.
pub fn search_filter_items(spec: &SearchSpec) -> (Vec<(u32, String, bool)>, Vec<Submenu>) {
    let size =
        |bound: Option<u64>| bound.map(|b| format!(" ({})", gezik_core::search::size_text(b))).unwrap_or_default();
    let mut items = vec![
        (SIZE_MIN, format!("Size at least…{}", size(spec.size.min)), true),
        (SIZE_MAX, format!("Size at most…{}", size(spec.size.max)), true),
    ];
    let options = [
        spec.name_regex,
        spec.content_regex,
        spec.match_case,
        spec.hidden == gezik_core::search::HiddenRule::Include,
        spec.skipped,
    ];
    let titles = [
        "Name is a regular expression",
        "Content is a regular expression",
        "Match case",
        "Include hidden items",
        "Include skipped folders",
    ];
    for (i, (on, title)) in options.into_iter().zip(titles).enumerate() {
        items.push((OPTION_FIRST + i as u32, marked(on, title), true));
    }
    items.push((FILTERS_CLEAR, "Clear filters".to_owned(), spec.filter_count() > 0));
    let mut modified: Vec<(u32, String, bool)> = MODIFIED_PRESETS
        .iter()
        .enumerate()
        .map(|(i, range)| (MODIFIED_FIRST + i as u32, marked(spec.modified == *range, &range.label()), true))
        .collect();
    let between = match spec.modified {
        DateRange::Between(..) => marked(true, &spec.modified.label()),
        _ => marked(false, "Between…"),
    };
    modified.push((MODIFIED_BETWEEN, between, true));
    let kinds = gezik_core::search::KindFilter::ALL
        .iter()
        .enumerate()
        .map(|(i, kind)| (KIND_FIRST + i as u32, marked(spec.kind == *kind, kind.label()), true))
        .collect();
    let subs = vec![
        Submenu { title: "Modified".to_owned(), at: 0, items: modified },
        Submenu { title: "Type".to_owned(), at: 0, items: kinds },
    ];
    (items, subs)
}

/// The search bar's ▾ menu (spec 4.2, 8): in a new tab, Clear, the folders the last search
/// could not read, "Save search…", the saved searches, then a Delete item for each (sapma 20).
pub fn search_more_items(problems: usize, saved: &[String], can_save: bool) -> Vec<(u32, String, bool)> {
    let mut items =
        vec![(SEARCH_NEW_TAB, "Search in new tab".to_owned(), true), (SEARCH_CLEAR, "Clear".to_owned(), true)];
    if problems > 0 {
        let what = if problems == 1 { "1 folder".to_owned() } else { format!("{problems} folders") };
        items.push((SEARCH_PROBLEMS, format!("{what} could not be read…"), true));
    }
    items.push((SAVE_SEARCH, "Save search…".to_owned(), can_save));
    let shown = saved.iter().take(SAVED_SEARCH_MAX as usize).enumerate();
    items.extend(shown.clone().map(|(i, name)| (SAVED_SEARCH_FIRST + i as u32, name.clone(), true)));
    items.extend(shown.map(|(i, name)| (SAVED_SEARCH_DELETE_FIRST + i as u32, format!("Delete \"{name}\""), true)));
    items
}

/// The scope menu (spec 4.1): `choices` by place, the current one marked.
pub fn search_scope_items(choices: &[(Scope, String)], current: &Scope) -> Vec<(u32, String, bool)> {
    choices
        .iter()
        .take(SCOPE_MAX as usize)
        .enumerate()
        .map(|(i, (scope, label))| (SCOPE_FIRST + i as u32, marked(scope == current, label), true))
        .collect()
}

/// 1550-1555: the results' header menu shows or hides the columns in `ColumnKey::RESULTS`
/// order; 1556 resets them (sapma 7).
pub const RESULT_COLUMN_FIRST: u32 = 1550;
pub const RESULT_COLUMNS_RESET: u32 = 1556;
/// 8b (spec 9.4): 1600-1699.
pub const CALC_FOLDER_SIZES: u32 = 1600;
pub const SAVE_SEARCH: u32 = 1601;
/// A saved search's sidebar menu.
pub const RUN_SEARCH_NEW_TAB: u32 = 1602;
pub const RENAME_SEARCH: u32 = 1603;
pub const DELETE_SEARCH: u32 = 1604;
/// 1610-1639: the ▾ menu runs saved search N; 1640-1669: deletes it.
pub const SAVED_SEARCH_FIRST: u32 = 1610;
pub const SAVED_SEARCH_DELETE_FIRST: u32 = 1640;
pub const SAVED_SEARCH_MAX: u32 = 30;

/// The results' column header menu.
pub fn result_header_items(columns: &[ColumnState]) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = ColumnKey::RESULTS
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let shown = columns.iter().any(|c| c.key == *key && c.visible);
            (RESULT_COLUMN_FIRST + i as u32, format!("{} {}", if shown { "Hide" } else { "Show" }, key.title()))
        })
        .collect();
    out.push((RESULT_COLUMNS_RESET, "Reset columns".to_owned()));
    out
}

/// Header menu: 20-23 show/hide the columns in `ColumnKey::ALL` order.
pub const TOGGLE_COLUMN_FIRST: u32 = 20;
pub const RESET_COLUMNS: u32 = 24;

/// The column header's menu: show or hide each column, reset all.
pub fn header_items(columns: &[ColumnState]) -> Vec<(u32, &'static str)> {
    let mut out: Vec<(u32, &'static str)> = columns
        .iter()
        .map(|c| {
            let id = TOGGLE_COLUMN_FIRST + u32::try_from(c.key.index() - 1).unwrap_or(0);
            let title = match (c.key, c.visible) {
                (ColumnKey::Modified, true) => "Hide Modified",
                (ColumnKey::Modified, false) => "Show Modified",
                (ColumnKey::Created, true) => "Hide Created",
                (ColumnKey::Created, false) => "Show Created",
                (ColumnKey::Type, true) => "Hide Type",
                (ColumnKey::Type, false) => "Show Type",
                (ColumnKey::Size, true) => "Hide Size",
                (ColumnKey::Size, false) => "Show Size",
                (ColumnKey::Folder, true) => "Hide Folder",
                (ColumnKey::Folder, false) => "Show Folder",
                (ColumnKey::Match, true) => "Hide Match",
                (ColumnKey::Match, false) => "Show Match",
            };
            (id, title)
        })
        .collect();
    out.push((RESET_COLUMNS, "Reset columns"));
    out
}

pub const VIEW_LIST: u32 = 30;
pub const VIEW_GRID: u32 = 31;
pub const GRID_SMALL: u32 = 32;
pub const GRID_MEDIUM: u32 = 33;
pub const GRID_LARGE: u32 = 34;
/// 35–39: sort by the keys in `SortKey::ALL` order.
pub const SORT_BY_NAME: u32 = 35;
pub const SORT_BY_MODIFIED: u32 = 36;
pub const SORT_BY_CREATED: u32 = 37;
pub const SORT_BY_TYPE: u32 = 38;
pub const SORT_BY_SIZE: u32 = 39;
pub const SORT_ASC: u32 = 40;
pub const SORT_DESC: u32 = 41;
pub const PREVIEW_PANE: u32 = 42;
/// 1408: View ▸ Drop stack.
pub const TOGGLE_STACK: u32 = 1408;
/// 1409: View ▸ Operation history.
pub const SHOW_HISTORY: u32 = 1409;
pub const APPLY_TO_ALL: u32 = 43;
pub const RESET_FOLDER: u32 = 44;

pub const UNDO: u32 = 50;
pub const REDO: u32 = 51;
pub const PASTE: u32 = 52;
pub const NEW_FOLDER: u32 = 53;
pub const NEW_FILE: u32 = 54;
pub const REFRESH: u32 = 55;
pub const CUT: u32 = 56;
pub const COPY: u32 = 57;
pub const DUPLICATE: u32 = 58;
pub const RENAME: u32 = 59;
pub const TRASH: u32 = 60;
pub const DELETE_PERMANENTLY: u32 = 61;
pub const PASTE_INTO: u32 = 62;
/// Windows only: "Rename N items…" (Explorer's menu has no Rename for several).
pub const BATCH_RENAME: u32 = 63;

/// 70–73: the conflict row menu, in `conflicts::DECISIONS` order.
pub const CONFLICT_FIRST: u32 = 70;

/// The menu after a drag with the right button (drag.rs).
pub const COPY_HERE: u32 = 80;
pub const MOVE_HERE: u32 = 81;
pub const CANCEL_DROP: u32 = 82;

/// 90-99: "Add rule" in the batch rename layer, in `gezik_core::batch::rules::KINDS` order.
pub const ADD_RULE_FIRST: u32 = 90;
/// 100-299: saved rule sets; 300 "Save current rules as…"; 400-599 delete one (ids stay
/// below the Shell's, which start at `GEZIK_IDS_END`).
pub const PRESET_FIRST: u32 = 100;
pub const PRESET_SAVE: u32 = 300;
pub const PRESET_DELETE_FIRST: u32 = 400;
/// How many saved sets the menu lists (and can delete).
pub const PRESET_MAX: u32 = 200;

/// 600-609: archives (archives.rs builds the items).
pub const EXTRACT_HERE: u32 = 600;
pub const EXTRACT_TO_OWN: u32 = 601;
pub const EXTRACT_TO: u32 = 602;
pub const COMPRESS: u32 = 603;
pub const COMPRESS_TO: u32 = 604;
/// After a drag with the right button onto an archive.
pub const ADD_TO_ARCHIVE: u32 = 605;

/// "Convert…" (convert.rs builds the conversion items).
pub const CONVERT: u32 = 610;
/// "Images to PDF…": the Convert layer opened on its PDF choice.
pub const IMAGES_TO_PDF: u32 = 611;
/// 620-659: the Convert layer's "From" encodings (the first: detect); 660-699: its "To"
/// encodings (the first: keep each file's), in `gezik_batch::convert::text::encodings` order.
pub const ENCODING_FROM_FIRST: u32 = 620;
pub const ENCODING_TO_FIRST: u32 = 660;
pub const ENCODING_MAX: u32 = 40;
/// 700-799: "Commands ▸", settings.toml's `[[commands]]` by index.
pub const COMMAND_FIRST: u32 = 700;
pub const COMMAND_MAX: u32 = 100;
/// 800-899: the Convert layer's preset list, by index.
pub const CONVERT_PRESET_FIRST: u32 = 800;
pub const CONVERT_PRESET_MAX: u32 = 100;
/// 900-929: the filter bar's saved filters, by index; 930 "Save as…"; 940-969 delete one
/// (ids stay below the Shell's, which start at `GEZIK_IDS_END`).
pub const FILTER_FIRST: u32 = 900;
pub const FILTER_MAX: u32 = 30;
pub const FILTER_SAVE: u32 = 930;
pub const FILTER_DELETE_FIRST: u32 = 940;
/// 970: a heading inside "Commands ▸" (`menu = "…"`), greyed, never chosen.
pub const COMMAND_GROUP: u32 = 970;
/// 1000-1001: Open terminal here / as administrator. 1010-1016: Copy path as ▸, in
/// `PathFormat::ALL` order.
pub const OPEN_TERMINAL: u32 = 1000;
pub const OPEN_TERMINAL_ADMIN: u32 = 1001;
pub const COPY_PATH_FIRST: u32 = 1010;
/// 1020: Save tabs as… (tab menu). 1100-1129: open tab set N; 1130-1159: replace the tabs with
/// set N; 1160-1189: delete set N.
pub const SAVE_TAB_SET: u32 = 1020;
pub const TAB_SET_OPEN_FIRST: u32 = 1100;
pub const TAB_SET_REPLACE_FIRST: u32 = 1130;
pub const TAB_SET_DELETE_FIRST: u32 = 1160;
pub const TAB_SET_MAX: u32 = 30;
/// 1200: Rename… (a pinned folder's alias). 1201: Move to group ▸ New group…; 1202: No group;
/// 1210-1259: Move to group ▸ group N (the groups shown, by place). 1260-1263: a group
/// heading's menu.
pub const RENAME_PIN: u32 = 1200;
pub const GROUP_NEW: u32 = 1201;
pub const GROUP_NONE: u32 = 1202;
pub const GROUP_MOVE_FIRST: u32 = 1210;
pub const GROUP_MAX: u32 = 50;
pub const GROUP_UP: u32 = 1260;
pub const GROUP_DOWN: u32 = 1261;
pub const GROUP_RENAME: u32 = 1262;
pub const UNGROUP: u32 = 1263;
/// 1300-1304: the View menu's options (Hide extensions, Folders first, Single-click to open,
/// Show hidden items, Show system items); 1310-1313 Date format ▸ in `DateFormat::ALL` order;
/// 1320-1321 Size format ▸ in `SizeFormat::ALL` order.
pub const HIDE_EXTENSIONS: u32 = 1300;
pub const FOLDERS_FIRST: u32 = 1301;
pub const SINGLE_CLICK_OPEN: u32 = 1302;
pub const SHOW_HIDDEN: u32 = 1303;
pub const SHOW_SYSTEM: u32 = 1304;
pub const DATE_FORMAT_FIRST: u32 = 1310;
pub const SIZE_FORMAT_FIRST: u32 = 1320;
/// 7c's ids are 1400-1459 (spec 11.1). 1407: "Create link here" after a drag with the right
/// button.
pub const CREATE_LINK_HERE: u32 = 1407;
/// 1400: New ▸ Markdown file; 1401: Open templates folder; 1402: New folder with selection;
/// 1403: Paste image/text as file; 1404-1406: Create link ▸ Shortcut, Junction, Symbolic link
/// (elsewhere the one "Create link" is 1406); 1410-1459: the user's templates by place.
pub const NEW_MARKDOWN: u32 = 1400;
pub const OPEN_TEMPLATES: u32 = 1401;
pub const NEW_FOLDER_WITH_SELECTION: u32 = 1402;
pub const PASTE_AS_FILE: u32 = 1403;
pub const LINK_SHORTCUT: u32 = 1404;
pub const LINK_JUNCTION: u32 = 1405;
pub const LINK_SYMLINK: u32 = 1406;
pub const TEMPLATE_FIRST: u32 = 1410;
/// 9a's ids are 1700-1799 (spec 9 §13.3); 9a1 has these two; 9a2: Open With ▸ by place, Other…,
/// Share… 1741, Quick Actions ▸ 1750-1779 by place.
pub const MAKE_ALIAS: u32 = 1743;
pub const SHOW_PACKAGE: u32 = 1744;
pub const OPEN_WITH_FIRST: u32 = 1700;
pub const OPEN_WITH_MAX: u32 = gezik_platform::open_with::MAX_APPS as u32;
pub const OPEN_WITH_OTHER: u32 = 1740;
pub const SHARE: u32 = 1741;
pub const QUICK_ACTION_FIRST: u32 = 1750;
pub const QUICK_ACTION_MAX: u32 = gezik_platform::services::MAX_SERVICES as u32;
pub const TEMPLATE_MAX: u32 = gezik_core::templates::TEMPLATE_MAX as u32;
/// 9b's ids are 1800-1899 (1800 kept for a "Show Trash" item). The trash's rows and background.
pub const PUT_BACK: u32 = 1801;
pub const TRASH_DELETE: u32 = 1802;
pub const EMPTY_TRASH: u32 = 1803;
/// 1881: the View menu's last item (spec 13.3).
pub const SYSTEM_INTEGRATION: u32 = 1881;

/// A trash row's menu: only what the trash does (no Explorer menu, nothing that acts on a
/// `$R…` name).
pub fn trash_row_items() -> [(u32, &'static str); 2] {
    [(PUT_BACK, "Put Back"), (TRASH_DELETE, "Delete Permanently…")]
}

/// "Empty Recycle Bin…" on Windows, "Empty Trash…" elsewhere.
fn empty_title() -> String {
    format!("Empty {}…", gezik_core::nav::TRASH_NAME)
}
/// Group headings in a Slint menu: shown greyed, never chosen.
pub const HEADING: u32 = 0;
/// Gezik's menu ids are below this; the Explorer menu's start here (gezik_platform's
/// `FIRST_SHELL_ID` on Windows). 1000-4095 are new ranges (spec 11.1).
#[cfg_attr(not(windows), allow(dead_code))]
pub const GEZIK_IDS_END: u32 = 4096;
#[cfg(windows)]
const _: () = assert!(gezik_platform::FIRST_SHELL_ID == GEZIK_IDS_END, "Gezik's ids end where Explorer's begin");
/// Most submenus a menu shows: app.slint and popup-menu.slint have this many places.
pub const MAX_SUBMENUS: usize = 4;

/// A submenu among a menu's items ("Commands ▸"): its title, its place among the items (0:
/// first), and its items (id, title, enabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submenu {
    pub title: String,
    pub at: usize,
    pub items: Vec<(u32, String, bool)>,
}

/// What a search result's row menu adds.
pub fn result_row_items() -> Vec<(u32, &'static str)> {
    vec![
        (SHOW_IN_FOLDER, "Show in folder"),
        (SHOW_IN_FOLDER_NEW_TAB, "Show in folder in new tab"),
        (COPY_WITH_FOLDERS, "Copy with folders"),
        (CUT_WITH_FOLDERS, "Cut with folders"),
    ]
}

/// Whether `paths` share one folder: Explorer's menu takes only that (`CDefFolderMenu`, spec 4.6).
pub fn one_folder(paths: &[PathBuf]) -> bool {
    paths.windows(2).all(|pair| pair[0].parent() == pair[1].parent())
}

/// Gezik's file items for rows on macOS and Linux; Windows has them in its own menu (and
/// Gezik takes them over, see `Menus::run_verb`).
pub fn file_items(single: bool, folder: bool, can_paste: bool) -> Vec<(u32, &'static str)> {
    let mut out = vec![(CUT, "Cut"), (COPY, "Copy")];
    if folder && can_paste {
        out.push((PASTE_INTO, "Paste into folder"));
    }
    out.push((DUPLICATE, "Duplicate"));
    out.push((RENAME, if single { "Rename" } else { "Rename items…" }));
    out.push((TRASH, "Move to Trash"));
    out.push((DELETE_PERMANENTLY, "Delete permanently"));
    out
}

/// The items for empty space in a folder: Undo/Redo say what they would do; Paste, or what
/// paste would write as a file; New folder and New file on Windows (elsewhere in New ▸).
pub fn background_items(
    undo: Option<&str>,
    redo: Option<&str>,
    can_paste: bool,
    paste_as: Option<PasteKind>,
    windows: bool,
) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if let Some(label) = undo {
        out.push((UNDO, format!("Undo {label}")));
    }
    if let Some(label) = redo {
        out.push((REDO, format!("Redo {label}")));
    }
    match (can_paste, paste_as) {
        (true, _) => out.push((PASTE, "Paste".to_owned())),
        (false, Some(PasteKind::Image)) => out.push((PASTE_AS_FILE, "Paste image as file".to_owned())),
        (false, Some(PasteKind::Text)) => out.push((PASTE_AS_FILE, "Paste text as file".to_owned())),
        (false, None) => {}
    }
    if windows {
        out.push((NEW_FOLDER, "New folder".to_owned()));
        out.push((NEW_FILE, "New file".to_owned()));
    }
    out.push((REFRESH, "Refresh".to_owned()));
    out
}

/// New ▸ (macOS, Linux: Folder, Text file, Markdown file) or New from template ▸ (Windows,
/// where Explorer's New ▸ is in the same menu: Markdown file), then the user's templates,
/// then Open templates folder; at place `at` (spec 8.1). No separator: submenus have none.
pub fn new_sub(templates: &[Template], windows: bool, at: usize) -> Submenu {
    let mut items = Vec::new();
    if !windows {
        items.push((NEW_FOLDER, "Folder".to_owned(), true));
        items.push((NEW_FILE, "Text file".to_owned(), true));
    }
    items.push((NEW_MARKDOWN, "Markdown file".to_owned(), true));
    items.extend(
        templates
            .iter()
            .take(TEMPLATE_MAX as usize)
            .enumerate()
            .map(|(i, template)| (TEMPLATE_FIRST + i as u32, template.label.clone(), true)),
    );
    items.push((OPEN_TEMPLATES, "Open templates folder".to_owned(), true));
    let title = if windows { "New from template" } else { "New" };
    Submenu { title: title.to_owned(), at, items }
}

/// Create link ▸ on Windows (Shortcut; Junction for local folders; Symbolic link when it can
/// be made); elsewhere the one "Create link" (a symbolic link, spec 9.2), after Make Alias on
/// macOS.
pub fn link_items(windows: bool, mac: bool, junction: bool, symlink: bool) -> Vec<(u32, String, bool)> {
    if !windows {
        let mut out = Vec::new();
        if mac {
            out.push((MAKE_ALIAS, "Make Alias".to_owned(), true));
        }
        out.push((LINK_SYMLINK, "Create link".to_owned(), true));
        return out;
    }
    let mut out = vec![(LINK_SHORTCUT, "Shortcut".to_owned(), true)];
    if junction {
        out.push((LINK_JUNCTION, "Junction".to_owned(), true));
    }
    if symlink {
        out.push((LINK_SYMLINK, "Symbolic link".to_owned(), true));
    }
    out
}

/// Show Package Contents for a package's row on macOS: by the name alone (a menu does not
/// read the disk).
pub fn package_item(path: &Path, is_dir: bool, mac: bool) -> Option<(u32, String)> {
    let package =
        mac && is_dir && path.file_name().is_some_and(|n| gezik_core::kind::is_package_name(&n.to_string_lossy()));
    package.then(|| (SHOW_PACKAGE, "Show Package Contents".to_owned()))
}

/// Open With ▸ (macOS): `apps` by place, or a greyed Loading… while they are not known, then
/// Other…; at place `at`.
pub fn open_with_sub(apps: Option<&[gezik_platform::open_with::AppChoice]>, at: usize) -> Submenu {
    let mut items: Vec<(u32, String, bool)> = match apps {
        None => vec![(HEADING, "Loading…".to_owned(), false)],
        Some(apps) => apps
            .iter()
            .take(OPEN_WITH_MAX as usize)
            .enumerate()
            .map(|(i, app)| {
                let title = if app.default { format!("{} (default)", app.name) } else { app.name.clone() };
                (OPEN_WITH_FIRST + i as u32, title, true)
            })
            .collect(),
    };
    items.push((OPEN_WITH_OTHER, "Other…".to_owned(), true));
    Submenu { title: "Open With".to_owned(), at, items }
}

/// Share… and Quick Actions ▸ (macOS) for rows, the submenu right after Share… (`at` is where
/// Share… goes); nothing off macOS, no submenu while the services are not known or none fits.
pub fn finder_items(
    services: Option<&[gezik_platform::services::Service]>,
    mac: bool,
    at: usize,
) -> (Vec<(u32, String)>, Option<Submenu>) {
    if !mac {
        return (Vec::new(), None);
    }
    let sub = services.filter(|s| !s.is_empty()).map(|services| Submenu {
        title: "Quick Actions".to_owned(),
        at: at + 1,
        items: services
            .iter()
            .take(QUICK_ACTION_MAX as usize)
            .enumerate()
            .map(|(i, s)| (QUICK_ACTION_FIRST + i as u32, s.title.clone(), true))
            .collect(),
    });
    (vec![(SHARE, "Share…".to_owned())], sub)
}

/// Whether Junction is offered for `rows`: all folders, none on a share or on one of
/// `network_drives` (mapped drives). The file system (NTFS) is checked by the job: the menu
/// reads no disk.
pub fn junction_offered(rows: &[(PathBuf, bool)], network_drives: &[PathBuf]) -> bool {
    !rows.is_empty()
        && rows.iter().all(|(path, is_dir)| {
            *is_dir
                && !path.to_string_lossy().starts_with(r"\\")
                && !network_drives.iter().any(|drive| gezik_core::ops::paths::is_within(path, drive))
        })
}

/// "Open terminal here", and on Windows "Open terminal as administrator" under it.
pub fn terminal_items(windows: bool) -> Vec<(u32, &'static str)> {
    let mut out = vec![(OPEN_TERMINAL, "Open terminal here")];
    if windows {
        out.push((OPEN_TERMINAL_ADMIN, "Open terminal as administrator"));
    }
    out
}

/// "Copy path as ▸": the formats offered here (`PathFormat::offered`), each by its place in
/// `PathFormat::ALL`.
pub fn copy_path_items(windows: bool, unc: bool) -> Vec<(u32, String, bool)> {
    PathFormat::ALL
        .iter()
        .enumerate()
        .filter(|(_, kind)| kind.offered(windows, unc))
        .map(|(i, kind)| (COPY_PATH_FIRST + i as u32, kind.label().to_owned(), true))
        .collect()
}

/// `items` with owned labels, to add items whose labels are made at run time.
fn owned(items: Vec<(u32, &'static str)>) -> Vec<(u32, String)> {
    items.into_iter().map(|(id, title)| (id, title.to_owned())).collect()
}

/// The View menu; the current choices are marked with a bullet. `options`: `[view]`'s options
/// (spec 7.2), "Show system items" only on Windows; Date format ▸ and Size format ▸ go before
/// "Apply to all folders" (`format_subs`).
pub fn view_items(
    view: ViewSettings,
    preview_open: bool,
    stack_open: bool,
    options: ViewOptions,
    windows: bool,
) -> Vec<(u32, String)> {
    let mark = |on: bool, title: &str| format!("{}{title}", if on { "• " } else { "    " });
    let grid = view.mode == ViewMode::Grid;
    let mut out = vec![(VIEW_LIST, mark(!grid, "List")), (VIEW_GRID, mark(grid, "Grid"))];
    if grid {
        out.push((GRID_SMALL, mark(view.grid_size == GridSize::Small, "Small icons")));
        out.push((GRID_MEDIUM, mark(view.grid_size == GridSize::Medium, "Medium icons")));
        out.push((GRID_LARGE, mark(view.grid_size == GridSize::Large, "Large icons")));
    }
    let sorts = [
        (SORT_BY_NAME, SortKey::Name, "Sort by name"),
        (SORT_BY_MODIFIED, SortKey::Modified, "Sort by date modified"),
        (SORT_BY_CREATED, SortKey::Created, "Sort by date created"),
        (SORT_BY_TYPE, SortKey::Type, "Sort by type"),
        (SORT_BY_SIZE, SortKey::Size, "Sort by size"),
    ];
    for (id, key, title) in sorts {
        out.push((id, mark(view.sort.key == key, title)));
    }
    out.push((SORT_ASC, mark(view.sort.dir == SortDir::Asc, "Ascending")));
    out.push((SORT_DESC, mark(view.sort.dir == SortDir::Desc, "Descending")));
    out.push((PREVIEW_PANE, mark(preview_open, "Preview pane")));
    out.push((TOGGLE_STACK, mark(stack_open, "Drop stack")));
    out.push((SHOW_HISTORY, "    Operation history".to_owned()));
    out.push((HIDE_EXTENSIONS, mark(options.hide_extensions, "Hide extensions")));
    out.push((FOLDERS_FIRST, mark(options.folders_first, "Folders first")));
    out.push((SINGLE_CLICK_OPEN, mark(options.single_click_open, "Single-click to open")));
    out.push((SHOW_HIDDEN, mark(options.show_hidden, "Show hidden items")));
    if windows {
        out.push((SHOW_SYSTEM, mark(options.show_system, "Show system items")));
    }
    out.push((APPLY_TO_ALL, "Apply to all folders".to_owned()));
    out.push((RESET_FOLDER, "Reset this folder".to_owned()));
    out.push((SYSTEM_INTEGRATION, "    System Integration…".to_owned()));
    out
}

/// Date format ▸ and Size format ▸ of the View menu, at place `at`, the current one marked.
pub fn format_subs(options: ViewOptions, at: usize) -> Vec<Submenu> {
    let mark = |on: bool, title: &str| format!("{}{title}", if on { "• " } else { "    " });
    let dates = DateFormat::ALL
        .iter()
        .enumerate()
        .map(|(i, f)| (DATE_FORMAT_FIRST + i as u32, mark(*f == options.date_format, f.label()), true))
        .collect();
    let sizes = SizeFormat::ALL
        .iter()
        .enumerate()
        .map(|(i, f)| (SIZE_FORMAT_FIRST + i as u32, mark(*f == options.size_format, f.label()), true))
        .collect();
    vec![
        Submenu { title: "Date format".to_owned(), at, items: dates },
        Submenu { title: "Size format".to_owned(), at, items: sizes },
    ]
}

/// What View menu item `id` changes, from `options` as they are now.
pub fn view_option_for(id: u32, options: ViewOptions) -> Option<ViewOption> {
    Some(match id {
        HIDE_EXTENSIONS => ViewOption::HideExtensions(!options.hide_extensions),
        FOLDERS_FIRST => ViewOption::FoldersFirst(!options.folders_first),
        SINGLE_CLICK_OPEN => ViewOption::SingleClickOpen(!options.single_click_open),
        SHOW_HIDDEN => ViewOption::ShowHidden(!options.show_hidden),
        SHOW_SYSTEM => ViewOption::ShowSystem(!options.show_system),
        id if (DATE_FORMAT_FIRST..DATE_FORMAT_FIRST + DateFormat::ALL.len() as u32).contains(&id) => {
            ViewOption::DateFormat(DateFormat::ALL[(id - DATE_FORMAT_FIRST) as usize])
        }
        id if (SIZE_FORMAT_FIRST..SIZE_FORMAT_FIRST + SizeFormat::ALL.len() as u32).contains(&id) => {
            ViewOption::SizeFormat(SizeFormat::ALL[(id - SIZE_FORMAT_FIRST) as usize])
        }
        _ => return None,
    })
}

/// "Presets ▾": each saved set, Save, then a Delete item for each (up to `PRESET_MAX` each).
pub fn preset_items(names: &[String]) -> Vec<(u32, String)> {
    let mut list: Vec<(u32, String)> =
        names.iter().enumerate().take(PRESET_MAX as usize).map(|(i, n)| (PRESET_FIRST + i as u32, n.clone())).collect();
    list.push((PRESET_SAVE, "Save current rules as…".to_owned()));
    list.extend(
        names
            .iter()
            .enumerate()
            .take(PRESET_MAX as usize)
            .map(|(i, n)| (PRESET_DELETE_FIRST + i as u32, format!("Delete \"{n}\""))),
    );
    list
}

/// The filter bar's ▾ menu: each saved filter, "Save as…" (only with a good, non-empty text),
/// then a Delete item for each (up to `FILTER_MAX` each).
pub fn filter_items(names: &[String], can_save: bool) -> Vec<(u32, String, bool)> {
    let shown = names.iter().take(FILTER_MAX as usize).enumerate();
    let mut list: Vec<(u32, String, bool)> =
        shown.clone().map(|(i, name)| (FILTER_FIRST + i as u32, name.clone(), true)).collect();
    list.push((FILTER_SAVE, "Save as…".to_owned(), can_save));
    list.extend(shown.map(|(i, name)| (FILTER_DELETE_FIRST + i as u32, format!("Delete \"{name}\""), true)));
    list
}

/// "Open tab set ▸": each set (opened after the tabs), then "Replace tabs with" each, then
/// "Delete" each (up to `TAB_SET_MAX` each), as the filter menu lists its own.
pub fn tab_set_items(names: &[String]) -> Vec<(u32, String, bool)> {
    let shown = names.iter().take(TAB_SET_MAX as usize).enumerate();
    let mut list: Vec<(u32, String, bool)> =
        shown.clone().map(|(i, name)| (TAB_SET_OPEN_FIRST + i as u32, name.clone(), true)).collect();
    list.extend(
        shown
            .clone()
            .map(|(i, name)| (TAB_SET_REPLACE_FIRST + i as u32, format!("Replace tabs with \"{name}\""), true)),
    );
    list.extend(shown.map(|(i, name)| (TAB_SET_DELETE_FIRST + i as u32, format!("Delete \"{name}\""), true)));
    list
}

fn pin_toggle(pinned: bool) -> (u32, &'static str) {
    if pinned { (UNPIN, "Unpin from sidebar") } else { (PIN, "Pin to sidebar") }
}

/// "Move to group ▸" for a pin in group `own`: the groups shown but its own (by their place
/// among them, up to `GROUP_MAX`), "New group…", and "No group" when it has one.
pub fn group_items(groups: &[String], own: Option<&str>) -> Vec<(u32, String, bool)> {
    let mut out: Vec<(u32, String, bool)> = groups
        .iter()
        .enumerate()
        .take(GROUP_MAX as usize)
        .filter(|(_, group)| own.is_none_or(|own| !gezik_config::pins::same_group(own, group)))
        .map(|(i, group)| (GROUP_MOVE_FIRST + i as u32, group.clone(), true))
        .collect();
    out.push((GROUP_NEW, "New group…".to_owned(), true));
    if own.is_some() {
        out.push((GROUP_NONE, "No group".to_owned(), true));
    }
    out
}

/// A group heading's menu: move it (not past the first or the last), rename it, ungroup it.
pub fn group_heading_items(first: bool, last: bool) -> Vec<(u32, &'static str)> {
    let mut out = Vec::new();
    if !first {
        out.push((GROUP_UP, "Move group up"));
    }
    if !last {
        out.push((GROUP_DOWN, "Move group down"));
    }
    out.push((GROUP_RENAME, "Rename group…"));
    out.push((UNGROUP, "Ungroup"));
    out
}

/// What a menu was opened for, captured when it opens. Items run later (the Slint menu
/// stays open while other things happen), so nothing here is an index that could point
/// elsewhere by then: rows and sidebar entries are kept by path, tabs by id.
#[derive(Debug, Clone)]
enum Subject {
    Row(PathBuf),
    Rows(Vec<PathBuf>),
    SidebarEntry(PathBuf),
    /// A group heading of the sidebar, by name.
    PinGroup(String),
    /// A tab by id, and the tab set names its menu listed.
    Tab(u64, Vec<String>),
    /// Empty space in this folder.
    Background(PathBuf),
    Header,
    View,
    Conflict(usize),
    /// Files dropped with the right button, the folder they were dropped on, and the
    /// archive if they were dropped on one.
    Drop(Vec<PathBuf>, PathBuf, Option<PathBuf>),
    /// The batch rename layer's menus, with the preset names shown (items are by index).
    BatchRename(Vec<String>),
    /// The Convert layer's menus (presets, encodings).
    Convert,
    /// The filter bar's ▾ menu, with the saved filters' names shown (items are by index).
    Filter(Vec<String>),
    /// The search bar's menus (items by id; the scope menu's by place in `search::Searches`).
    Search,
    /// A saved search in the sidebar, by name.
    SavedSearch(String),
}

/// Lets one native menu be pending or open at a time, so two right-clicks in quick
/// succession (within the delay before it opens, or queued behind its modal loop) do not
/// show two menus one after the other.
#[derive(Clone, Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct MenuGate(Rc<Cell<bool>>);

#[cfg_attr(not(windows), allow(dead_code))]
impl MenuGate {
    /// Claims the gate until the returned claim is dropped; `None` while a menu already
    /// holds it.
    fn claim(&self) -> Option<MenuClaim> {
        (!self.0.replace(true)).then(|| MenuClaim(self.0.clone()))
    }
}

struct MenuClaim(Rc<Cell<bool>>);

impl Drop for MenuClaim {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Opens context menus and runs their Gezik items.
#[derive(Clone)]
pub struct Menus {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    view: View,
    preview: Preview,
    sidebar: Sidebar,
    ops: Operations,
    /// What the open Slint menu is for.
    subject: Rc<RefCell<Option<Subject>>>,
    /// The rows a row menu was opened for (path, is a folder), for its archive items.
    rows: Rc<RefCell<Vec<(PathBuf, bool)>>>,
    /// The groups the last sidebar menu listed in Move to group ▸ (items are by place).
    pin_groups: Rc<RefCell<Vec<String>>>,
    /// The templates the last New ▸ listed (items are by place).
    menu_templates: Rc<RefCell<Vec<Template>>>,
    /// The apps the last Open With ▸ listed (items are by place).
    menu_apps: Rc<RefCell<Vec<gezik_platform::open_with::AppChoice>>>,
    /// The Quick Actions the last row menu listed (items are by place).
    menu_services: Rc<RefCell<Vec<gezik_platform::services::Service>>>,
    /// Where the last row menu was opened (window position), for Share…'s picker.
    menu_at: Rc<Cell<(f32, f32)>>,
    /// An Open With list that came after its menu had opened (macOS).
    late: crate::finder_menu::Late,
    #[cfg_attr(not(windows), allow(dead_code))]
    native_menu: MenuGate,
}

impl Menus {
    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        view: View,
        preview: Preview,
        sidebar: Sidebar,
        ops: Operations,
    ) -> Menus {
        let menus = Menus {
            window: window.as_weak(),
            nav,
            view,
            preview,
            sidebar,
            ops,
            subject: Rc::default(),
            rows: Rc::default(),
            pin_groups: Rc::default(),
            menu_templates: Rc::default(),
            menu_apps: Rc::default(),
            menu_services: Rc::default(),
            menu_at: Rc::default(),
            late: crate::finder_menu::Late::default(),
            native_menu: MenuGate::default(),
        };
        window.on_menu_closed(|| {
            crate::drag::with_current(|drags| drags.menu_shown());
        });
        window.on_menu_activated({
            let menus = menus.clone();
            move |id| {
                let subject = menus.subject.borrow_mut().take();
                if let (Ok(id), Some(subject)) = (u32::try_from(id), subject) {
                    menus.run(id, subject);
                    // An item of a submenu leaves the keyboard nowhere once the menus close
                    // (Slint gives it back to the parent menu, which is gone): the list (or
                    // the layer over it) takes it, unless a question came up.
                    if from_submenu(id) {
                        let weak = menus.window.clone();
                        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                            if let Some(window) = weak.upgrade()
                                && !window.get_dialog_open()
                            {
                                window.invoke_focus_list();
                            }
                        });
                    }
                }
            }
        });
        menus
    }

    /// Right-click on file list row `index` (already selected), at window position `x`, `y`.
    pub fn row(&self, index: i32, x: f32, y: f32) {
        self.row_menu(index, x, y, false);
    }

    /// Right-click on empty space in the file list, at window position `x`, `y`.
    pub fn background(&self, x: f32, y: f32) {
        self.background_menu(None, x, y);
    }

    /// Shift+F10 or the Menu key on the file list: the menu of row `index` (selected and
    /// scrolled into view), or of the folder's background if it is -1, at window position
    /// `x`, `y` (also for the Windows menu, which a right-click opens at the cursor).
    pub fn keyboard(&self, index: i32, x: f32, y: f32) {
        if index >= 0 {
            self.row_menu(index, x, y, true);
        } else {
            self.background_menu(Some((x, y)), x, y);
        }
    }

    fn row_menu(&self, index: i32, x: f32, y: f32, at_position: bool) {
        let Ok(i) = usize::try_from(index) else { return };
        if self.view.shows_trash() {
            *self.subject.borrow_mut() = Some(Subject::Background(PathBuf::new()));
            return self.open_slint(&trash_row_items(), Anchor::point(x, y));
        }
        let at = at_position.then_some((x, y));
        let native = cfg!(windows);
        self.menu_at.set((x, y));
        self.ops.clipboard_check();
        if self.view.is_selected(i) && self.view.selection_count() > 1 {
            let rows = self.view.selected_items();
            let paths: Vec<PathBuf> = rows.iter().map(|(path, _)| path.clone()).collect();
            let results = self.view.shows_results();
            // Results from several folders get Gezik's own menu (spec 4.6).
            let native = native && (!results || one_folder(&paths));
            let mut list = owned(items(Place::Rows, native));
            list.extend(owned(terminal_items(native)));
            let mut subs = vec![self.copy_path_sub(&paths, list.len())];
            let services = self.add_finder_extras(&list, &mut subs, &rows);
            self.add_links(&mut list, &mut subs, &rows, native);
            self.add_file_tools(&mut list, &mut subs, rows, native);
            list.extend(self.file_extras(false, false, native));
            self.add_finder_items(&mut list, &mut subs, services);
            if results {
                list.extend(owned(result_row_items()).into_iter().filter(|(id, _)| *id != SHOW_IN_FOLDER_NEW_TAB));
            } else if !self.view.shows_drives() {
                list.push((NEW_FOLDER_WITH_SELECTION, "New folder with selection".to_owned()));
            }
            if native && !self.view.shows_drives() {
                list.push((BATCH_RENAME, format!("Rename {} items…", paths.len())));
            }
            return self.open_as(native, Subject::Rows(paths.clone()), list, subs, MenuTarget::Items(paths), x, y, at);
        }
        let Some((path, is_dir)) = self.view.entry_path(i) else { return };
        let place = Place::Row { is_dir, pinned: is_dir && self.sidebar.is_pinned(&path) };
        let mut list = owned(items(place, native));
        if let Some(item) = package_item(&path, is_dir, cfg!(target_os = "macos")) {
            list.insert(list.len().min(1), item); // after Open, as in Finder
        }
        if is_dir {
            list.push((
                SEARCH_HERE,
                format!("Search in \"{}\"…", crate::operations::items_text(std::slice::from_ref(&path))),
            ));
            if matches!(self.nav.active_location(), Location::Path(_)) {
                list.push((CALC_FOLDER_SIZES, "Calculate folder sizes".to_owned()));
            }
        }
        list.extend(owned(terminal_items(native)));
        let mut subs = vec![self.copy_path_sub(std::slice::from_ref(&path), list.len())];
        let services = self.add_finder_extras(&list, &mut subs, &[(path.clone(), is_dir)]);
        self.add_links(&mut list, &mut subs, &[(path.clone(), is_dir)], native);
        self.add_file_tools(&mut list, &mut subs, vec![(path.clone(), is_dir)], native);
        list.extend(self.file_extras(true, is_dir, native));
        self.add_finder_items(&mut list, &mut subs, services);
        if self.view.shows_results() {
            list.extend(owned(result_row_items()));
        }
        self.open(Subject::Row(path.clone()), list, subs, MenuTarget::Item(path), x, y, at);
    }

    /// Extract, Compress, Convert and Commands ▸ for `rows`: first on Windows (above the
    /// Explorer menu's own items, moving the places of `subs` down), after the row's other
    /// items elsewhere. Not for drives.
    fn add_file_tools(
        &self,
        list: &mut Vec<(u32, String)>,
        subs: &mut Vec<Submenu>,
        rows: Vec<(PathBuf, bool)>,
        native: bool,
    ) {
        if self.view.shows_drives() {
            return;
        }
        let mut extra = crate::archives::menu_items(&rows);
        let (convert, commands) = crate::convert::menu_items(&rows);
        extra.extend(convert);
        *self.rows.borrow_mut() = rows;
        let start = if native { 0 } else { list.len() };
        if native {
            for sub in subs.iter_mut() {
                sub.at += extra.len();
            }
        }
        subs.extend(commands.map(|items| Submenu { title: "Commands".to_owned(), at: start + extra.len(), items }));
        if native {
            list.splice(0..0, extra);
        } else {
            list.extend(extra);
        }
    }

    /// File items after the row's own: Windows already has Cut, Copy, Delete... (taken over in
    /// `run_verb`), so only Duplicate is added there.
    fn file_extras(&self, single: bool, folder: bool, native: bool) -> Vec<(u32, String)> {
        // Drives (This PC) are not files: no copying, deleting or renaming them.
        if self.view.shows_drives() {
            Vec::new()
        } else if native {
            vec![(DUPLICATE, "Duplicate".to_owned())]
        } else {
            owned(file_items(single, folder, self.ops.can_paste()))
        }
    }

    /// macOS: asks Open With's apps and the Quick Actions for `rows` (at most `WAIT`), adds
    /// Open With ▸ after the Open items; Share… and Quick Actions ▸ come with `add_finder_items`.
    /// Asked for exactly the rows the menu acts on, so never for more than `MAX_ITEMS`.
    fn add_finder_extras(
        &self,
        list: &[(u32, String)],
        subs: &mut Vec<Submenu>,
        rows: &[(PathBuf, bool)],
    ) -> Option<Vec<gezik_platform::services::Service>> {
        self.menu_apps.borrow_mut().clear();
        self.menu_services.borrow_mut().clear();
        if !cfg!(target_os = "macos")
            || self.view.shows_drives()
            || rows.is_empty()
            || rows.len() > gezik_platform::open_with::MAX_ITEMS
        {
            return None;
        }
        let items: Vec<PathBuf> = rows.iter().map(|(path, _)| path.clone()).collect();
        let extras = crate::finder_menu::fetch(&self.late, items);
        if crate::finder_menu::offers_open_with(rows, true) {
            let apps = extras.as_ref().map(|e| e.apps.as_slice());
            subs.push(open_with_sub(apps, crate::finder_menu::open_with_place(list)));
            *self.menu_apps.borrow_mut() = extras.as_ref().map(|e| e.apps.clone()).unwrap_or_default();
        }
        extras.map(|e| e.services)
    }

    /// Share… and Quick Actions ▸ at the end of `list` (macOS rows, not drives).
    fn add_finder_items(
        &self,
        list: &mut Vec<(u32, String)>,
        subs: &mut Vec<Submenu>,
        services: Option<Vec<gezik_platform::services::Service>>,
    ) {
        if self.view.shows_drives() {
            return;
        }
        let (items, sub) = finder_items(services.as_deref(), cfg!(target_os = "macos"), list.len());
        list.extend(items);
        subs.extend(sub);
        *self.menu_services.borrow_mut() = services.unwrap_or_default();
    }

    /// Share… from a row menu, at the right-click's place once the menu is done.
    fn share(&self, paths: Vec<PathBuf>) {
        let (window, at) = (self.window.clone(), self.menu_at.get());
        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
            if let Some(window) = window.upgrade() {
                crate::finder_menu::share(&window, paths, Some(at));
            }
        });
    }

    /// Create link ▸ (Windows) or Create link at the end of the items so far, for `rows`;
    /// nothing for drives.
    fn add_links(
        &self,
        list: &mut Vec<(u32, String)>,
        subs: &mut Vec<Submenu>,
        rows: &[(PathBuf, bool)],
        native: bool,
    ) {
        if self.view.shows_drives() {
            return;
        }
        let network: Vec<PathBuf> = self
            .nav
            .places()
            .drives
            .into_iter()
            .filter(|drive| drive.kind == gezik_platform::DriveKind::Network)
            .map(|drive| drive.path)
            .collect();
        let items = link_items(
            native,
            cfg!(target_os = "macos"),
            junction_offered(rows, &network),
            gezik_platform::link::symlinks_allowed(),
        );
        if native {
            subs.push(Submenu { title: "Create link".to_owned(), at: list.len(), items });
        } else {
            list.extend(items.into_iter().map(|(id, title, _)| (id, title)));
        }
    }

    fn background_menu(&self, at: Option<(f32, f32)>, x: f32, y: f32) {
        // Search results have no folder: Undo, Redo and Refresh (spec 4.6).
        if self.view.shows_results() {
            let list: Vec<(u32, String, bool)> = background_items(
                self.ops.undo_label().as_deref(),
                self.ops.redo_label().as_deref(),
                false,
                None,
                false,
            )
            .into_iter()
            .map(|(id, title)| (id, title, true))
            .collect();
            let mut list = list;
            if self.view.shows_trash() {
                list.push((EMPTY_TRASH, empty_title(), true));
            }
            *self.subject.borrow_mut() = Some(Subject::Background(PathBuf::new()));
            return self.open_slint_entries(&list, Vec::new(), Anchor::point(x, y));
        }
        let Location::Path(dir) = self.nav.active_location() else { return };
        self.ops.clipboard_check();
        // One clipboard query each, shared by the menu and its Paste item.
        let can_paste = self.ops.can_paste();
        let paste_as = self.ops.paste_as(can_paste);
        let windows = cfg!(windows);
        let mut list = background_items(
            self.ops.undo_label().as_deref(),
            self.ops.redo_label().as_deref(),
            can_paste,
            paste_as,
            windows,
        );
        let templates = crate::templates::current();
        let new_at = list.iter().position(|(id, _)| *id == REFRESH).unwrap_or(list.len());
        let mut subs = vec![new_sub(&templates, windows, new_at)];
        *self.menu_templates.borrow_mut() = templates;
        list.push((SEARCH_HERE, "Search in this folder…".to_owned()));
        list.extend(owned(terminal_items(windows)));
        subs.push(self.copy_path_sub(std::slice::from_ref(&dir), list.len()));
        self.open(Subject::Background(dir.clone()), list, subs, MenuTarget::Background(dir), x, y, at);
    }

    /// "Copy path as ▸" for `paths`, at place `at` among the items.
    fn copy_path_sub(&self, paths: &[PathBuf], at: usize) -> Submenu {
        let unc = crate::copy_path::unc_offered(paths.first().map(PathBuf::as_path));
        Submenu { title: "Copy path as".to_owned(), at, items: copy_path_items(cfg!(windows), unc) }
    }

    /// Right-click on sidebar entry (`section`, `index`), at window position `x`, `y`; on a
    /// group's heading, its menu.
    pub fn sidebar_entry(&self, section: i32, index: i32, x: f32, y: f32) {
        if section == SECTION_GROUP {
            return self.group_heading(index, x, y);
        }
        if section == crate::sidebar::SECTION_SEARCHES {
            let names = crate::saved_searches::names();
            let Some(name) = usize::try_from(index).ok().and_then(|i| names.get(i).cloned()) else { return };
            *self.subject.borrow_mut() = Some(Subject::SavedSearch(name));
            let list = [(RUN_SEARCH_NEW_TAB, "Run in new tab"), (RENAME_SEARCH, "Rename…"), (DELETE_SEARCH, "Delete")];
            return self.open_slint(&list, Anchor::point(x, y));
        }
        if section == crate::sidebar::SECTION_TRASH {
            *self.subject.borrow_mut() = Some(Subject::Background(PathBuf::new()));
            return self.open_slint(&[(EMPTY_TRASH, empty_title())], Anchor::point(x, y));
        }
        let Some(Location::Path(path)) = self.sidebar.location_of(section, index) else { return };
        let pinned_section = section == SECTION_PINNED;
        let (first, last) = if pinned_section { self.sidebar.group_ends(&path) } else { (true, true) };
        let place = Place::Sidebar { pinned_section, pinned: self.sidebar.is_pinned(&path), first, last };
        let mut list = owned(items(place, cfg!(windows)));
        let mut subs = Vec::new();
        if pinned_section {
            let groups = self.sidebar.shown_groups();
            let own = self.sidebar.group_of(&path);
            subs.push(Submenu {
                title: "Move to group".to_owned(),
                at: list.len(),
                items: group_items(&groups, own.as_deref()),
            });
            *self.pin_groups.borrow_mut() = groups;
        }
        list.push((SEARCH_HERE, "Search in this folder…".to_owned()));
        list.extend(owned(terminal_items(cfg!(windows))));
        subs.push(self.copy_path_sub(std::slice::from_ref(&path), list.len()));
        self.open(Subject::SidebarEntry(path.clone()), list, subs, MenuTarget::Item(path), x, y, None);
    }

    /// Right-click on the heading of group `index` (its place among the groups shown).
    fn group_heading(&self, index: i32, x: f32, y: f32) {
        let groups = self.sidebar.shown_groups();
        let Some(i) = usize::try_from(index).ok().filter(|i| *i < groups.len()) else { return };
        *self.subject.borrow_mut() = Some(Subject::PinGroup(groups[i].clone()));
        self.open_slint(&group_heading_items(i == 0, i + 1 == groups.len()), Anchor::point(x, y));
    }

    /// Right-click on tab `index`, at window position `x`, `y`. Tabs get Gezik's own menu
    /// everywhere, with "Open tab set ▸" when there are sets.
    pub fn tab(&self, index: usize, x: f32, y: f32) {
        let Some(id) = self.nav.tab_id(index) else { return };
        let place = Place::Tab { only_tab: self.nav.tab_count() == 1, locked: self.nav.is_tab_locked(index) };
        let list: Vec<(u32, String, bool)> =
            items(place, false).into_iter().map(|(id, title)| (id, title.to_owned(), true)).collect();
        let names = crate::tab_sets::names();
        let subs = vec![Submenu { title: "Open tab set".to_owned(), at: list.len(), items: tab_set_items(&names) }];
        *self.subject.borrow_mut() = Some(Subject::Tab(id, names));
        self.open_slint_entries(&list, subs, Anchor::point(x, y));
    }

    /// Right-click on the column header, at window position `x`, `y`.
    pub fn header(&self, x: f32, y: f32) {
        *self.subject.borrow_mut() = Some(Subject::Header);
        if self.view.shows_results() {
            self.open_slint(&result_header_items(&self.view.result_columns()), Anchor::point(x, y));
        } else {
            self.open_slint(&header_items(&self.view.columns()), Anchor::point(x, y));
        }
    }

    /// The View button's menu, under it.
    pub fn view_menu(&self, at: Anchor) {
        *self.subject.borrow_mut() = Some(Subject::View);
        let options = crate::view_options::current();
        let mut stack_open = false;
        crate::stack::with_current(|stack| stack_open = stack.is_open());
        let items =
            view_items(self.view.view_settings(), self.preview.is_pane_open(), stack_open, options, cfg!(windows));
        let place = items.iter().position(|(id, _)| *id == APPLY_TO_ALL).unwrap_or(items.len());
        let mut entries: Vec<(u32, String, bool)> = items.into_iter().map(|(id, title)| (id, title, true)).collect();
        // Flat view (spec 5): marked in it, off in This PC.
        let location = self.nav.active_location();
        let flat = matches!(location, Location::Flat(_));
        let at_flat = entries.iter().position(|(id, _, _)| *id == PREVIEW_PANE).unwrap_or(entries.len());
        entries.insert(
            at_flat,
            (FLAT_VIEW, format!("{}Flat view", if flat { "• " } else { "    " }), location.folder().is_some()),
        );
        // After "Apply to all folders" (the formats' place stays): only in a folder.
        if matches!(location, Location::Path(_)) {
            let at_calc = entries.iter().position(|(id, _, _)| *id == RESET_FOLDER).unwrap_or(entries.len());
            entries.insert(at_calc, (CALC_FOLDER_SIZES, "    Calculate folder sizes".to_owned(), true));
        }
        self.open_slint_entries(&entries, format_subs(options, place + 1), at);
    }

    /// `subs`: submenus among `items`. `at`: where the Windows menu opens (window position),
    /// else at the cursor.
    #[allow(clippy::too_many_arguments, reason = "what the menu is for, what it shows, and where")]
    fn open(
        &self,
        subject: Subject,
        items: Vec<(u32, String)>,
        subs: Vec<Submenu>,
        target: MenuTarget,
        x: f32,
        y: f32,
        at: Option<(f32, f32)>,
    ) {
        self.open_as(cfg!(windows), subject, items, subs, target, x, y, at);
    }

    /// Like `open`; `native`: the Explorer menu (Windows), else Gezik's own.
    #[allow(clippy::too_many_arguments, reason = "what the menu is for, what it shows, and where")]
    fn open_as(
        &self,
        native: bool,
        subject: Subject,
        items: Vec<(u32, String)>,
        subs: Vec<Submenu>,
        target: MenuTarget,
        x: f32,
        y: f32,
        at: Option<(f32, f32)>,
    ) {
        if native {
            self.open_native(Some(subject), target, items, subs, at);
        } else {
            *self.subject.borrow_mut() = Some(subject);
            let entries: Vec<(u32, String, bool)> = items.into_iter().map(|(id, title)| (id, title, true)).collect();
            self.open_slint_entries(&entries, subs, Anchor::point(x, y));
        }
    }

    /// Shows the Explorer menu at window position `at` (else at the cursor), with `items` on
    /// top. Its modal loop blocks the UI thread, so it opens once the click is fully handled
    /// and the selection is drawn. A request while another menu is pending or open is dropped.
    #[cfg(windows)]
    fn open_native(
        &self,
        subject: Option<Subject>,
        target: MenuTarget,
        items: Vec<(u32, String)>,
        subs: Vec<Submenu>,
        at: Option<(f32, f32)>,
    ) {
        let Some(claim) = self.native_menu.claim() else { return };
        let menus = self.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(16), move || {
            let Some(window) = menus.window.upgrade() else { return };
            let handle = window.window().window_handle();
            let scale = window.window().scale_factor();
            let at = at.map(|(x, y)| ((x * scale).round() as i32, (y * scale).round() as i32));
            let items: Vec<(u32, &str)> = items.iter().map(|(id, title)| (*id, title.as_str())).collect();
            let sub_items: Vec<Vec<(u32, &str, bool)>> = subs
                .iter()
                .map(|sub| sub.items.iter().map(|(id, title, on)| (*id, title.as_str(), *on)).collect())
                .collect();
            let shell_subs: Vec<gezik_platform::ShellSubmenu> = subs
                .iter()
                .zip(&sub_items)
                .map(|(sub, items)| gezik_platform::ShellSubmenu { title: sub.title.as_str(), at: sub.at, items })
                .collect();
            // Gezik renames in place, only a single row of a folder listing (see run_verb).
            let can_rename = matches!(subject, Some(Subject::Row(_))) && !menus.view.shows_drives();
            crate::drag::with_current(|drags| drags.menu_shown());
            let outcome = gezik_platform::show_shell_menu(&handle, &target, &items, &shell_subs, at, can_rename);
            release_stale_modifiers(&window);
            after_native_menu(&window);
            drop(claim);
            match outcome {
                Ok(gezik_platform::MenuOutcome::Gezik(id)) => {
                    if let Some(subject) = subject {
                        menus.run(id, subject);
                    }
                }
                Ok(gezik_platform::MenuOutcome::SystemCommandRan) => {
                    // The command may have created, renamed or deleted anything, pinned
                    // folders included. Results are not read again for it: that would run the
                    // whole search and read its name cache anew (F5 does, spec 4.7).
                    if !menus.nav.active_location().is_results() {
                        menus.nav.reload();
                    }
                    menus.sidebar.refresh();
                }
                Ok(gezik_platform::MenuOutcome::Verb(verb)) => menus.run_verb(verb, subject),
                Ok(gezik_platform::MenuOutcome::Dismissed) => {}
                Err(err) => window.set_status(format!("Cannot show the menu: {err}").into()),
            }
        });
    }

    #[cfg(not(windows))]
    fn open_native(
        &self,
        _subject: Option<Subject>,
        _target: MenuTarget,
        _items: Vec<(u32, String)>,
        _subs: Vec<Submenu>,
        _at: Option<(f32, f32)>,
    ) {
    }

    fn open_slint<S: AsRef<str>>(&self, items: &[(u32, S)], at: Anchor) {
        let entries: Vec<(u32, String, bool)> =
            items.iter().map(|(id, title)| (*id, title.as_ref().to_owned(), true)).collect();
        self.open_slint_entries(&entries, Vec::new(), at);
    }

    /// Shows `items` (id, title, enabled), with `subs` among them, at `anchor`.
    /// (Slint shows these as native menus on Windows, where `&` marks the access key, and on
    /// macOS; elsewhere Gezik draws its own, which popup.rs keeps inside the window.)
    fn open_slint_entries(&self, items: &[(u32, String, bool)], subs: Vec<Submenu>, anchor: Anchor) {
        let Some(window) = self.window.upgrade() else { return };
        if items.is_empty() && subs.iter().all(|sub| sub.items.is_empty()) {
            return;
        }
        let entry = |(id, title, enabled): &(u32, String, bool)| MenuEntry {
            id: i32::try_from(*id).unwrap_or(0),
            title: menu_title(title).into(),
            enabled: *enabled,
        };
        let model = |entries: Vec<MenuEntry>| ModelRc::new(VecModel::from(entries));
        let (before, parts) = split_menu(items, &subs);
        let before: Vec<MenuEntry> = before.iter().map(entry).collect();
        let parts: Vec<(String, Vec<MenuEntry>, Vec<MenuEntry>)> = parts
            .into_iter()
            .map(|(sub, after)| {
                (menu_title(&sub.title), sub.items.iter().map(entry).collect(), after.iter().map(entry).collect())
            })
            .collect();
        let slots: Vec<MenuSub> = parts
            .iter()
            .map(|(title, inner, after)| MenuSub {
                title: title.as_str().into(),
                entries: model(inner.clone()),
                after: model(after.clone()),
            })
            .collect();
        window.set_menu_entries(model(before.clone()));
        window.set_menu_subs(ModelRc::new(VecModel::from(slots)));
        if !window.get_native_menus() {
            let lines = parts.into_iter().map(|(title, _, after)| (title, after)).collect();
            window.set_menu_lines(model(menu_lines(before, lines)));
        }
        crate::drag::with_current(|drags| drags.menu_shown());
        window.invoke_show_menu(anchor.x, anchor.y, anchor.flip_x, anchor.flip_y);
        // A system menu (Windows, macOS) is closed again by now; Gezik's own tells
        // `menu-closed`.
        if window.get_native_menus() {
            after_native_menu(&window);
        }
    }

    /// One of the Convert layer's menus (convert.rs builds it): `items` (id, title, enabled)
    /// under its button.
    pub fn convert_menu(&self, items: Vec<(u32, String, bool)>, at: Anchor) {
        *self.subject.borrow_mut() = Some(Subject::Convert);
        self.open_slint_entries(&items, Vec::new(), at);
    }

    /// Copy here / Move here / Create link here / Cancel for files dropped with the right
    /// button on `dir`, at window position `x`, `y`; only the effects that make sense there are offered.
    /// `archive`: they were dropped on one, which "Add to archive" adds them to.
    #[allow(clippy::too_many_arguments, reason = "what was dropped where, and what it may do")]
    pub fn drop_menu(
        &self,
        paths: Vec<PathBuf>,
        dir: PathBuf,
        archive: Option<PathBuf>,
        can_copy: bool,
        can_move: bool,
        can_link: bool,
        x: f32,
        y: f32,
    ) {
        let mut list = Vec::new();
        if archive.is_some() {
            list.push((ADD_TO_ARCHIVE, "Add to archive"));
        }
        if can_copy {
            list.push((COPY_HERE, "Copy here"));
        }
        if can_move {
            list.push((MOVE_HERE, "Move here"));
        }
        if can_link {
            list.push((CREATE_LINK_HERE, "Create link here"));
        }
        if list.is_empty() {
            return;
        }
        list.push((CANCEL_DROP, "Cancel"));
        *self.subject.borrow_mut() = Some(Subject::Drop(paths, dir, archive));
        self.open_slint(&list, Anchor::point(x, y));
    }

    /// The decision menu of conflict row `row`, at window position `x`, `y`.
    pub fn conflict(&self, row: i32, x: f32, y: f32) {
        let Ok(row) = usize::try_from(row) else { return };
        let conflicts = self.ops.conflicts();
        let list: Vec<(u32, &'static str)> = crate::conflicts::DECISIONS
            .iter()
            .enumerate()
            .filter(|(_, d)| conflicts.choices_for(row).contains(d))
            .map(|(i, d)| (CONFLICT_FIRST + i as u32, d.label()))
            .collect();
        if list.is_empty() {
            return;
        }
        *self.subject.borrow_mut() = Some(Subject::Conflict(row));
        self.open_slint(&list, Anchor::point(x, y));
    }
    /// "Add rule ▾" of the batch rename layer, under it.
    pub fn add_rule(&self, at: Anchor) {
        let list: Vec<(u32, &str)> = gezik_core::batch::rules::KINDS
            .iter()
            .enumerate()
            .map(|(i, (_, label))| (ADD_RULE_FIRST + i as u32, *label))
            .collect();
        *self.subject.borrow_mut() = Some(Subject::BatchRename(Vec::new()));
        self.open_slint(&list, at);
    }

    /// "Presets ▾": the saved sets, save, delete.
    pub fn presets(&self, at: Anchor) {
        let names = crate::batch_rename::preset_names();
        let list = preset_items(&names);
        *self.subject.borrow_mut() = Some(Subject::BatchRename(names));
        self.open_slint(&list, at);
    }

    /// The filter bar's ▾ menu, under its button: the saved filters, "Save as…", Delete.
    pub fn filter_menu(&self, at: Anchor) {
        let names: Vec<String> = crate::filter::saved().into_iter().map(|f| f.name).collect();
        let mut can_save = false;
        crate::filter::with_current(|filter| can_save = filter.can_save());
        let items = filter_items(&names, can_save);
        *self.subject.borrow_mut() = Some(Subject::Filter(names));
        self.open_slint_entries(&items, Vec::new(), at);
    }

    /// One of the search bar's menus, under its button.
    pub fn search_menu(&self, which: crate::search::SearchMenu, at: Anchor) {
        let mut built = (Vec::new(), Vec::new());
        crate::search::with_current(|s| built = s.menu(which));
        *self.subject.borrow_mut() = Some(Subject::Search);
        self.open_slint_entries(&built.0, built.1, at);
    }

    fn run(&self, id: u32, subject: Subject) {
        match (id, subject) {
            (id, Subject::Search) => crate::search::with_current(|s| s.menu_chosen(id)),
            (PUT_BACK, _) => crate::trash_view::put_back(&self.view),
            (TRASH_DELETE, _) => crate::trash_view::delete_selection(&self.view),
            (EMPTY_TRASH, _) => crate::trash_view::empty(),
            (RUN_SEARCH_NEW_TAB, Subject::SavedSearch(name)) => {
                crate::saved_searches::with_current(|s| s.run(&name, true));
            }
            (RENAME_SEARCH, Subject::SavedSearch(name)) => crate::saved_searches::with_current(|s| s.ask_rename(&name)),
            (DELETE_SEARCH, Subject::SavedSearch(name)) => crate::saved_searches::with_current(|s| s.delete(&name)),
            (SEARCH_HERE, Subject::Background(dir) | Subject::SidebarEntry(dir) | Subject::Row(dir)) => {
                crate::search::with_current(|s| s.open_in(dir));
            }
            (SHOW_IN_FOLDER | SHOW_IN_FOLDER_NEW_TAB, Subject::Row(path)) => {
                self.ops.show_path_in_folder(&path, id == SHOW_IN_FOLDER_NEW_TAB);
            }
            (SHOW_IN_FOLDER, Subject::Rows(_)) => self.ops.show_in_folder(false),
            (COPY_WITH_FOLDERS | CUT_WITH_FOLDERS, Subject::Row(_) | Subject::Rows(_)) => {
                self.ops.copy_with_folders(id == CUT_WITH_FOLDERS);
            }
            (FLAT_VIEW, Subject::View) => crate::search::with_current(crate::search::Searches::flat_view),
            // By name: settings.toml may have been reloaded since the menu opened.
            (id, Subject::Filter(names)) if (FILTER_FIRST..FILTER_FIRST + FILTER_MAX).contains(&id) => {
                if let Some(name) = names.get((id - FILTER_FIRST) as usize) {
                    crate::filter::with_current(|filter| filter.apply_saved(name));
                }
            }
            (FILTER_SAVE, Subject::Filter(_)) => crate::filter::with_current(crate::filter::Filter::ask_save),
            (id, Subject::Filter(names)) if (FILTER_DELETE_FIRST..FILTER_DELETE_FIRST + FILTER_MAX).contains(&id) => {
                if let Some(name) = names.get((id - FILTER_DELETE_FIRST) as usize) {
                    crate::filter::with_current(|filter| filter.delete_saved(name));
                }
            }
            (id, Subject::BatchRename(_)) if (ADD_RULE_FIRST..ADD_RULE_FIRST + 10).contains(&id) => {
                if let Some((kind, _)) = gezik_core::batch::rules::KINDS.get((id - ADD_RULE_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.add_rule(kind));
                }
            }
            // By name: settings.toml may have been reloaded since the menu opened.
            (id, Subject::BatchRename(names)) if (PRESET_FIRST..PRESET_FIRST + PRESET_MAX).contains(&id) => {
                if let Some(name) = names.get((id - PRESET_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.apply_preset(name));
                }
            }
            (PRESET_SAVE, Subject::BatchRename(_)) => {
                crate::batch_rename::with_current(|layer| layer.ask_preset_name());
            }
            (id, Subject::BatchRename(names))
                if (PRESET_DELETE_FIRST..PRESET_DELETE_FIRST + PRESET_MAX).contains(&id) =>
            {
                if let Some(name) = names.get((id - PRESET_DELETE_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.delete_preset(name));
                }
            }
            (id, Subject::Conflict(row)) if (CONFLICT_FIRST..CONFLICT_FIRST + 4).contains(&id) => {
                if let Some(decision) = crate::conflicts::DECISIONS.get((id - CONFLICT_FIRST) as usize) {
                    self.ops.conflicts().decide_row(row, *decision);
                }
            }
            (COPY_HERE, Subject::Drop(paths, dir, _)) => self.ops.transfer(paths, dir, Effect::Copy),
            (MOVE_HERE, Subject::Drop(paths, dir, _)) => self.ops.transfer(paths, dir, Effect::Move),
            (CREATE_LINK_HERE, Subject::Drop(paths, dir, _)) => self.ops.transfer(paths, dir, Effect::Link),
            (ADD_TO_ARCHIVE, Subject::Drop(paths, _, Some(archive))) => {
                crate::archives::with_current(|archives| archives.add_to(archive, paths, None));
            }
            (id, Subject::Convert) => crate::convert::with_current(|convert| convert.menu_chosen(id)),
            (CONVERT, Subject::Row(_) | Subject::Rows(_)) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                crate::convert::with_current(|convert| convert.open(rows));
            }
            (IMAGES_TO_PDF, Subject::Row(_) | Subject::Rows(_)) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                let choice = crate::convert::Choice::Pdf(gezik_core::batch::pdf::PdfOp::ImagesToPdf);
                crate::convert::with_current(|convert| convert.open_with(rows, Some(choice)));
            }
            (id, Subject::Row(_) | Subject::Rows(_)) if (COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX).contains(&id) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                crate::convert::run_menu_command((id - COMMAND_FIRST) as usize, rows);
            }
            (EXTRACT_HERE..=COMPRESS_TO, Subject::Row(_) | Subject::Rows(_)) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                let paths: Vec<PathBuf> = rows.iter().map(|(path, _)| path.clone()).collect();
                crate::archives::with_current(|archives| match id {
                    EXTRACT_HERE => archives.extract_here(paths),
                    EXTRACT_TO_OWN => archives.extract_to_own(paths),
                    EXTRACT_TO => archives.extract_to_asked(paths),
                    COMPRESS => archives.open_compress(rows),
                    _ => archives.compress_to(rows),
                });
            }
            (OPEN_IN_NEW_TAB, Subject::Row(path) | Subject::SidebarEntry(path)) => {
                self.nav.open_tab(Location::Path(path), false);
            }
            (PIN, Subject::Row(path) | Subject::SidebarEntry(path)) => self.sidebar.pin(path),
            (UNPIN, Subject::Row(path) | Subject::SidebarEntry(path)) => self.sidebar.unpin_path(&path),
            (MOVE_UP | MOVE_DOWN, Subject::SidebarEntry(path)) => self.sidebar.move_in_group(&path, id == MOVE_UP),
            (RENAME_PIN, Subject::SidebarEntry(path)) => self.sidebar.ask_alias(&path),
            (GROUP_NEW, Subject::SidebarEntry(path)) => self.sidebar.ask_new_group(&path),
            (GROUP_NONE, Subject::SidebarEntry(path)) => self.sidebar.set_group(&path, None),
            (id, Subject::SidebarEntry(path)) if (GROUP_MOVE_FIRST..GROUP_MOVE_FIRST + GROUP_MAX).contains(&id) => {
                let group = self.pin_groups.borrow().get((id - GROUP_MOVE_FIRST) as usize).cloned();
                if let Some(group) = group {
                    self.sidebar.set_group(&path, Some(&group));
                }
            }
            (GROUP_UP | GROUP_DOWN, Subject::PinGroup(group)) => self.sidebar.move_group(&group, id == GROUP_UP),
            (GROUP_RENAME, Subject::PinGroup(group)) => self.sidebar.ask_group_name(&group),
            (UNGROUP, Subject::PinGroup(group)) => self.sidebar.ungroup(&group),
            (SAVE_TAB_SET, Subject::Tab(..)) => crate::tab_sets::with_current(crate::tab_sets::TabSets::ask_save),
            (id, Subject::Tab(_, names)) if crate::tab_sets::set_item(id).is_some() => {
                crate::tab_sets::with_current(|sets| sets.chosen(id, &names));
            }
            (DUPLICATE_TAB, Subject::Tab(id, _)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.duplicate_tab(i);
                }
            }
            (CLOSE_TAB, Subject::Tab(id, _)) => {
                // After the menu is fully done: closing the last tab closes the window.
                let nav = self.nav.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || nav.close_tab_by_id(id));
            }
            (LOCK_TAB | UNLOCK_TAB, Subject::Tab(id, _)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.toggle_tab_lock(i);
                }
            }
            (CLOSE_OTHER_TABS, Subject::Tab(id, _)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.close_other_tabs(i);
                }
            }
            (OPEN | OPEN_DEFAULT, Subject::Row(path)) => {
                if let Err(err) = open::that_detached(&path)
                    && let Some(window) = self.window.upgrade()
                {
                    let why = gezik_platform::fs::describe(&err);
                    window.set_status(format!("Cannot open {}: {why}", path.display()).into());
                }
            }
            (OPEN, Subject::Rows(paths)) => {
                let paths = match crate::view::limit_open(paths) {
                    Ok(paths) => paths,
                    Err(message) => {
                        if let Some(window) = self.window.upgrade() {
                            window.set_status(message.into());
                        }
                        return;
                    }
                };
                for path in paths {
                    if let Err(err) = open::that_detached(&path)
                        && let Some(window) = self.window.upgrade()
                    {
                        let why = gezik_platform::fs::describe(&err);
                        window.set_status(format!("Cannot open {}: {why}", path.display()).into());
                    }
                }
            }
            (id, Subject::Header) if (TOGGLE_COLUMN_FIRST..TOGGLE_COLUMN_FIRST + 4).contains(&id) => {
                if let Some(key) = ColumnKey::ALL.get((id - TOGGLE_COLUMN_FIRST) as usize) {
                    self.view.toggle_column(*key);
                }
            }
            (RESET_COLUMNS, Subject::Header) => self.view.reset_columns(),
            (id, Subject::Header) if (RESULT_COLUMN_FIRST..RESULT_COLUMN_FIRST + 6).contains(&id) => {
                self.view.toggle_column(ColumnKey::RESULTS[(id - RESULT_COLUMN_FIRST) as usize]);
            }
            (RESULT_COLUMNS_RESET, Subject::Header) => self.view.reset_columns(),
            (VIEW_LIST, Subject::View) => self.view.set_mode(ViewMode::List),
            (VIEW_GRID, Subject::View) => self.view.set_mode(ViewMode::Grid),
            (GRID_SMALL, Subject::View) => self.view.set_grid_size(GridSize::Small),
            (GRID_MEDIUM, Subject::View) => self.view.set_grid_size(GridSize::Medium),
            (GRID_LARGE, Subject::View) => self.view.set_grid_size(GridSize::Large),
            (id, Subject::View) if (SORT_BY_NAME..=SORT_BY_SIZE).contains(&id) => {
                let key = SortKey::ALL[(id - SORT_BY_NAME) as usize];
                self.view.set_sort(SortSpec { key, dir: self.view.sort().dir });
            }
            (SORT_ASC, Subject::View) => self.view.set_sort(SortSpec { dir: SortDir::Asc, ..self.view.sort() }),
            (SORT_DESC, Subject::View) => self.view.set_sort(SortSpec { dir: SortDir::Desc, ..self.view.sort() }),
            (PREVIEW_PANE, Subject::View) => self.preview.toggle_pane(),
            (TOGGLE_STACK, Subject::View) => crate::stack::with_current(crate::stack::Stack::toggle),
            (SHOW_HISTORY, Subject::View) => self.ops.show_history(),
            (APPLY_TO_ALL, Subject::View) => self.view.apply_to_all(),
            (RESET_FOLDER, Subject::View) => self.view.reset_folder(),
            (SYSTEM_INTEGRATION, Subject::View) => {
                crate::integration::with_current(crate::integration::Integration::open)
            }
            (CALC_FOLDER_SIZES, _) => crate::folder_sizes::with_current(crate::folder_sizes::FolderSizes::calculate),
            (id, Subject::View) => {
                if let Some(option) = view_option_for(id, crate::view_options::current()) {
                    crate::view_options::change(option);
                }
            }
            (CUT | COPY, Subject::Row(path)) => self.ops.copy_paths(vec![path], id == CUT),
            (CUT | COPY, Subject::Rows(paths)) => self.ops.copy_paths(paths, id == CUT),
            (PASTE_INTO, Subject::Row(path)) => self.ops.paste(Some(path), false),
            (DUPLICATE, Subject::Row(_) | Subject::Rows(_)) => self.ops.duplicate(),
            (RENAME, Subject::Row(_) | Subject::Rows(_)) => self.ops.rename_start(),
            (BATCH_RENAME, Subject::Rows(_)) => self.ops.batch_rename(),
            (TRASH | DELETE_PERMANENTLY, Subject::Row(path)) => {
                self.ops.trash_paths(vec![path], id == DELETE_PERMANENTLY)
            }
            (TRASH | DELETE_PERMANENTLY, Subject::Rows(paths)) => self.ops.trash_paths(paths, id == DELETE_PERMANENTLY),
            (UNDO, Subject::Background(_)) => self.ops.undo(),
            (REDO, Subject::Background(_)) => self.ops.redo(),
            (PASTE, Subject::Background(dir)) => self.ops.paste(Some(dir), false),
            (NEW_FOLDER, Subject::Background(dir)) => self.ops.new_folder(Some(dir)),
            (NEW_FILE, Subject::Background(dir)) => self.ops.new_file(Some(dir)),
            (NEW_MARKDOWN, Subject::Background(dir)) => self.ops.new_markdown(dir),
            (OPEN_TEMPLATES, Subject::Background(_)) => crate::templates::open_folder(),
            (id, Subject::Background(dir)) if (TEMPLATE_FIRST..TEMPLATE_FIRST + TEMPLATE_MAX).contains(&id) => {
                let template = self.menu_templates.borrow().get((id - TEMPLATE_FIRST) as usize).cloned();
                if let Some(template) = template {
                    self.ops.new_from_template(dir, &template);
                }
            }
            (PASTE_AS_FILE, Subject::Background(dir)) => self.ops.paste_as_file(dir),
            (NEW_FOLDER_WITH_SELECTION, Subject::Rows(paths)) => self.ops.new_folder_with(paths),
            (LINK_SHORTCUT | LINK_JUNCTION | LINK_SYMLINK | MAKE_ALIAS, Subject::Row(path)) => {
                self.ops.create_links(vec![path], link_kind(id))
            }
            (LINK_SHORTCUT | LINK_JUNCTION | LINK_SYMLINK | MAKE_ALIAS, Subject::Rows(paths)) => {
                self.ops.create_links(paths, link_kind(id))
            }
            (SHOW_PACKAGE, Subject::Row(path)) => self.nav.go(Location::Path(path)),
            (id, subject @ (Subject::Row(_) | Subject::Rows(_)))
                if (OPEN_WITH_FIRST..OPEN_WITH_FIRST + OPEN_WITH_MAX).contains(&id) || id == OPEN_WITH_OTHER =>
            {
                let paths = match subject {
                    Subject::Row(path) => vec![path],
                    Subject::Rows(paths) => paths,
                    _ => Vec::new(),
                };
                let app = match id {
                    OPEN_WITH_OTHER => None,
                    // An app id the last menu did not list: nothing (never the Other… panel).
                    _ => match self.menu_apps.borrow().get((id - OPEN_WITH_FIRST) as usize) {
                        Some(app) => Some(app.path.clone()),
                        None => return,
                    },
                };
                crate::finder_menu::open_with(&self.window, paths, app);
            }
            (SHARE, Subject::Row(path)) => self.share(vec![path]),
            (SHARE, Subject::Rows(paths)) => self.share(paths),
            (id, subject @ (Subject::Row(_) | Subject::Rows(_)))
                if (QUICK_ACTION_FIRST..QUICK_ACTION_FIRST + QUICK_ACTION_MAX).contains(&id) =>
            {
                let paths = match subject {
                    Subject::Row(path) => vec![path],
                    Subject::Rows(paths) => paths,
                    _ => Vec::new(),
                };
                // Only a Quick Action the last menu listed, on the items it was listed for.
                let Some(title) =
                    self.menu_services.borrow().get((id - QUICK_ACTION_FIRST) as usize).map(|s| s.title.clone())
                else {
                    return;
                };
                if let Err(why) = gezik_platform::services::perform(&title, &paths)
                    && let Some(window) = self.window.upgrade()
                {
                    window.set_status(format!("Cannot run {title}: {why}").into());
                }
            }
            (REFRESH, Subject::Background(_)) => self.nav.reload(),
            (OPEN_TERMINAL | OPEN_TERMINAL_ADMIN, subject) => {
                let dir = match subject {
                    Subject::Row(path) if self.view.is_folder_row(&path) => Some(path),
                    Subject::Row(path) => path.parent().map(Path::to_path_buf),
                    Subject::Rows(_) => self.view.folder(),
                    Subject::SidebarEntry(path) | Subject::Background(path) => Some(path),
                    _ => None,
                };
                if let Some(dir) = dir {
                    crate::terminal::open_in(dir, id == OPEN_TERMINAL_ADMIN);
                }
            }
            (id, subject) if (COPY_PATH_FIRST..COPY_PATH_FIRST + PathFormat::ALL.len() as u32).contains(&id) => {
                let paths = match subject {
                    Subject::Row(path) | Subject::SidebarEntry(path) | Subject::Background(path) => vec![path],
                    Subject::Rows(paths) => paths,
                    _ => Vec::new(),
                };
                if let Some(kind) = PathFormat::ALL.get((id - COPY_PATH_FIRST) as usize) {
                    crate::copy_path::copy(&self.view, &paths, *kind);
                }
            }
            _ => {}
        }
    }

    /// Explorer's own Cut, Copy, Paste, Delete and Rename, done by Gezik (its engine, its
    /// conflict list, its undo).
    #[cfg(windows)]
    fn run_verb(&self, verb: gezik_platform::ShellVerb, subject: Option<Subject>) {
        use gezik_platform::ShellVerb;
        let paths = match &subject {
            Some(Subject::Row(path) | Subject::SidebarEntry(path)) => vec![path.clone()],
            Some(Subject::Rows(paths)) => paths.clone(),
            _ => Vec::new(),
        };
        match verb {
            ShellVerb::Cut => self.ops.copy_paths(paths, true),
            ShellVerb::Copy => self.ops.copy_paths(paths, false),
            ShellVerb::Paste => {
                let into = match subject {
                    // A row is a target only if it is a folder, else the shown folder gets it.
                    Some(Subject::Row(path)) => self.view.is_folder_row(&path).then_some(path),
                    Some(Subject::SidebarEntry(path) | Subject::Background(path)) => Some(path),
                    _ => None,
                };
                self.ops.paste(into, false);
            }
            ShellVerb::Delete => {
                let keys = gezik_platform::modifier_keys_down();
                let permanent = keys.left_shift || keys.right_shift;
                if matches!(subject, Some(Subject::SidebarEntry(_))) {
                    // A pinned folder is not what is selected in the list: always ask first.
                    self.ops.trash_asking(paths, permanent);
                } else {
                    self.ops.trash_paths(paths, permanent);
                }
            }
            ShellVerb::Link => self.ops.create_links(paths, LinkKind::Shortcut),
            ShellVerb::Rename => {
                if matches!(subject, Some(Subject::Row(_))) {
                    self.ops.rename_start();
                }
            }
        }
    }
}

/// Catches up after a system menu closed: no press from before it goes on as a drag, and
/// (Windows) the window learns where the pointer went while the menu's modal loop took its
/// moves, so the press that closed the menu by clicking elsewhere lands there and not where
/// the menu opened (from where its first move would start a drag).
fn after_native_menu(window: &AppWindow) {
    crate::drag::with_current(|drags| drags.menu_shown());
    #[cfg(windows)]
    gezik_platform::catch_up_pointer(&window.window().window_handle());
    #[cfg(not(windows))]
    let _ = window;
}

/// Tells Slint that the modifier keys not down now were released. The native menu's modal
/// loop takes the key releases made while it is open (Shift after Shift+F10, or a modifier
/// held for a right-click), and Slint knows modifiers only from key events, so it would go
/// on treating plain clicks as Shift+clicks. Releasing a key Slint already counts as up
/// changes nothing.
#[cfg(windows)]
fn release_stale_modifiers(window: &AppWindow) {
    for key in released_modifiers(gezik_platform::modifier_keys_down()) {
        window.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key.into() });
    }
}

/// The Slint modifier keys that are up, given the keys `down`.
#[cfg(windows)]
fn released_modifiers(down: gezik_platform::ModifierKeys) -> Vec<slint::platform::Key> {
    use slint::platform::Key;
    [
        (down.left_shift, Key::Shift),
        (down.right_shift, Key::ShiftR),
        (down.left_control, Key::Control),
        (down.right_control, Key::ControlR),
        // Slint reports either Alt as `Alt`, and right Alt as `AltGr` on layouts that have it.
        (down.alt, Key::Alt),
        (down.right_alt, Key::AltGr),
        (down.left_meta, Key::Meta),
        (down.right_meta, Key::MetaR),
    ]
    .into_iter()
    .filter_map(|(is_down, key)| (!is_down).then_some(key))
    .collect()
}

/// `items` cut at `subs`' places: the items before the first submenu, then each submenu (by
/// place, empty ones left out, at most `MAX_SUBMENUS`) with the items after it.
fn split_menu<'a, T: Clone>(items: &[T], subs: &'a [Submenu]) -> (Vec<T>, Vec<(&'a Submenu, Vec<T>)>) {
    let mut kept: Vec<&Submenu> = subs.iter().filter(|sub| !sub.items.is_empty()).collect();
    kept.sort_by_key(|sub| sub.at);
    kept.truncate(MAX_SUBMENUS);
    let place = |sub: &Submenu| sub.at.min(items.len());
    let first = kept.first().map_or(items.len(), |sub| place(sub));
    let parts = kept
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            let end = kept.get(i + 1).map_or(items.len(), |next| place(next));
            (*sub, items[place(sub)..end].to_vec())
        })
        .collect();
    (items[..first].to_vec(), parts)
}

/// Gezik's own menu in one list (widgets/popup-menu.slint): `before`, then for each submenu
/// its opener (id -1 for the first, -2 for the second…, titled as it) and the items after it.
fn menu_lines(before: Vec<MenuEntry>, subs: Vec<(String, Vec<MenuEntry>)>) -> Vec<MenuEntry> {
    let mut lines = before;
    for (k, (title, after)) in subs.into_iter().enumerate() {
        lines.push(MenuEntry { id: -1 - k as i32, title: title.into(), enabled: true });
        lines.extend(after);
    }
    lines
}

/// The link a Create link item makes.
fn link_kind(id: u32) -> LinkKind {
    match id {
        LINK_SHORTCUT => LinkKind::Shortcut,
        LINK_JUNCTION => LinkKind::Junction,
        MAKE_ALIAS => LinkKind::Alias,
        _ => LinkKind::Symlink,
    }
}

/// Whether `id` is an item of a submenu: once one is chosen, Slint leaves the keyboard
/// nowhere (it gives it to the parent menu, which is gone).
fn from_submenu(id: u32) -> bool {
    (COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX).contains(&id)
        || (COPY_PATH_FIRST..COPY_PATH_FIRST + PathFormat::ALL.len() as u32).contains(&id)
        || crate::tab_sets::set_item(id).is_some()
        || (DATE_FORMAT_FIRST..DATE_FORMAT_FIRST + DateFormat::ALL.len() as u32).contains(&id)
        || (SIZE_FORMAT_FIRST..SIZE_FORMAT_FIRST + SizeFormat::ALL.len() as u32).contains(&id)
        || (GROUP_MOVE_FIRST..GROUP_MOVE_FIRST + GROUP_MAX).contains(&id)
        || id == GROUP_NEW
        || id == GROUP_NONE
        || (TEMPLATE_FIRST..TEMPLATE_FIRST + TEMPLATE_MAX).contains(&id)
        || (OPEN_WITH_FIRST..OPEN_WITH_FIRST + OPEN_WITH_MAX).contains(&id)
        || id == OPEN_WITH_OTHER
        || (QUICK_ACTION_FIRST..QUICK_ACTION_FIRST + QUICK_ACTION_MAX).contains(&id)
        || (MODIFIED_FIRST..=MODIFIED_BETWEEN).contains(&id)
        || (KIND_FIRST..KIND_FIRST + gezik_core::search::KindFilter::ALL.len() as u32).contains(&id)
        || matches!(
            id,
            NEW_FOLDER | NEW_FILE | NEW_MARKDOWN | OPEN_TEMPLATES | LINK_SHORTCUT | LINK_JUNCTION | LINK_SYMLINK
        )
}

/// Whether Slint shows its menus as the system's: on Windows and macOS (through muda),
/// unless `SLINT_NO_MUDA` turns that off. Elsewhere Gezik draws its own.
pub fn native_menus() -> bool {
    cfg!(any(windows, target_os = "macos")) && std::env::var_os("SLINT_NO_MUDA").is_none()
}

/// A title for Gezik's own menus: on Windows and macOS they are native menus (muda), which
/// take `&` as the access key mark (and drop it on macOS), so it is doubled to show as itself.
pub fn menu_title(title: &str) -> String {
    if native_menus() { title.replace('&', "&&") } else { title.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_rows_offer_their_folder_and_copies_that_keep_folders() {
        let ids: Vec<u32> = result_row_items().iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [SHOW_IN_FOLDER, SHOW_IN_FOLDER_NEW_TAB, COPY_WITH_FOLDERS, CUT_WITH_FOLDERS]);
        assert!(ids.iter().all(|id| (1500..1600).contains(id)));
        assert!(one_folder(&[PathBuf::from("/w/a/x"), PathBuf::from("/w/a/y")]));
        assert!(!one_folder(&[PathBuf::from("/w/a/x"), PathBuf::from("/w/b/y")]), "the Explorer menu takes one folder");
    }

    #[test]
    fn the_results_header_menu_has_its_own_ids() {
        let items = result_header_items(&gezik_core::view::default_result_columns());
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [1550, 1551, 1552, 1553, 1554, 1555, RESULT_COLUMNS_RESET]);
        assert_eq!(items[0].1, "Hide Folder");
        assert_eq!(items[3].1, "Show Created");
        assert!(ids.iter().all(|id| (1500..1600).contains(id)), "8a's range");
    }

    #[test]
    fn gezik_menus_list_the_submenus_among_the_items() {
        let entry = |id: i32, title: &str| MenuEntry { id, title: title.into(), enabled: true };
        let lines = menu_lines(
            vec![entry(1, "Open")],
            vec![("Copy path as".to_owned(), vec![entry(2, "Cut")]), ("Commands".to_owned(), Vec::new())],
        );
        let shown: Vec<(i32, &str)> = lines.iter().map(|l| (l.id, l.title.as_str())).collect();
        assert_eq!(shown, [(1, "Open"), (-1, "Copy path as"), (2, "Cut"), (-2, "Commands")]);
    }

    #[test]
    fn a_menu_is_cut_at_its_submenus() {
        let sub = |title: &str, at: usize, n: usize| Submenu {
            title: title.into(),
            at,
            items: vec![(1, "x".to_owned(), true); n],
        };
        let items = ["a", "b", "c", "d"];
        let subs = [sub("Late", 9, 1), sub("Empty", 1, 0), sub("Mid", 2, 1), sub("First", 0, 2)];
        let (before, parts) = split_menu(&items, &subs);
        assert!(before.is_empty());
        let shown: Vec<(&str, Vec<&str>)> = parts.iter().map(|(s, after)| (s.title.as_str(), after.clone())).collect();
        assert_eq!(shown, [("First", vec!["a", "b"]), ("Mid", vec!["c", "d"]), ("Late", vec![])]);
        let many: Vec<Submenu> = (0..6).map(|i| sub("S", i, 1)).collect();
        let (_, parts) = split_menu(&items, &many);
        assert_eq!(parts.len(), MAX_SUBMENUS);
        assert_eq!(parts.last().unwrap().1, ["d"], "the items after one left out stay");
        let (before, parts) = split_menu(&items, &[]);
        assert_eq!((before.len(), parts.len()), (4, 0));
    }

    #[cfg(windows)]
    #[test]
    fn gezik_ids_end_where_explorers_begin() {
        assert_eq!(gezik_platform::FIRST_SHELL_ID, GEZIK_IDS_END);
    }

    #[test]
    fn trash_rows_offer_only_put_back_and_delete_for_good() {
        assert_eq!(trash_row_items().map(|(id, _)| id), [PUT_BACK, TRASH_DELETE]);
        assert!(empty_title().starts_with("Empty ") && empty_title().ends_with('…'));
    }

    #[test]
    fn an_ampersand_shows_as_itself() {
        let shown = if native_menus() { "Copy && keep" } else { "Copy & keep" };
        assert_eq!(menu_title("Copy & keep"), shown);
    }

    #[test]
    fn drop_menu_ids_are_their_own() {
        let others = [
            OPEN_IN_NEW_TAB,
            PIN,
            UNPIN,
            MOVE_UP,
            MOVE_DOWN,
            DUPLICATE_TAB,
            CLOSE_TAB,
            CLOSE_OTHER_TABS,
            OPEN,
            OPEN_DEFAULT,
            RESET_COLUMNS,
            VIEW_LIST,
            VIEW_GRID,
            GRID_SMALL,
            GRID_MEDIUM,
            GRID_LARGE,
            SORT_BY_NAME,
            SORT_BY_MODIFIED,
            SORT_BY_CREATED,
            SORT_BY_TYPE,
            SORT_BY_SIZE,
            SORT_ASC,
            SORT_DESC,
            PREVIEW_PANE,
            TOGGLE_STACK,
            SHOW_HISTORY,
            APPLY_TO_ALL,
            RESET_FOLDER,
            SYSTEM_INTEGRATION,
            UNDO,
            REDO,
            PASTE,
            NEW_FOLDER,
            NEW_FILE,
            REFRESH,
            CUT,
            COPY,
            DUPLICATE,
            RENAME,
            TRASH,
            DELETE_PERMANENTLY,
            PASTE_INTO,
            BATCH_RENAME,
            LOCK_TAB,
            UNLOCK_TAB,
            COMMAND_GROUP,
        ];
        let ranges = [TOGGLE_COLUMN_FIRST..RESET_COLUMNS, CONFLICT_FIRST..CONFLICT_FIRST + 4];
        let archives = [EXTRACT_HERE, EXTRACT_TO_OWN, EXTRACT_TO, COMPRESS, COMPRESS_TO, ADD_TO_ARCHIVE];
        for id in [
            COPY_HERE,
            MOVE_HERE,
            CREATE_LINK_HERE,
            CANCEL_DROP,
            MAKE_ALIAS,
            SHOW_PACKAGE,
            OPEN_WITH_FIRST,
            OPEN_WITH_FIRST + OPEN_WITH_MAX - 1,
            OPEN_WITH_OTHER,
            SHARE,
            QUICK_ACTION_FIRST,
            QUICK_ACTION_FIRST + QUICK_ACTION_MAX - 1,
            ADD_RULE_FIRST,
            ADD_RULE_FIRST + 9,
            PRESET_FIRST,
            PRESET_SAVE,
        ] {
            assert!(!others.contains(&id) && !ranges.iter().any(|r| r.contains(&id)), "{id} is taken");
        }
        // The archive items are their own, distinct, and meet no other range.
        for (i, id) in archives.iter().enumerate() {
            assert!(!archives[..i].contains(id), "{id} twice");
            assert!(!others.contains(id) && !ranges.iter().any(|r| r.contains(id)), "{id} is taken");
            assert!(![COPY_HERE, MOVE_HERE, CANCEL_DROP, PRESET_SAVE].contains(id), "{id} is taken");
            assert!(!(ADD_RULE_FIRST..ADD_RULE_FIRST + 10).contains(id), "{id} is a rule id");
        }
        // The preset ranges meet nothing else, nor each other, and stay below the Shell's ids.
        let presets = [PRESET_FIRST..PRESET_FIRST + PRESET_MAX, PRESET_DELETE_FIRST..PRESET_DELETE_FIRST + PRESET_MAX];
        let singles = others.iter().chain(&[COPY_HERE, MOVE_HERE, CANCEL_DROP, PRESET_SAVE]).chain(&archives);
        for id in singles.copied().chain(ADD_RULE_FIRST..ADD_RULE_FIRST + 10).chain(CONFLICT_FIRST..CONFLICT_FIRST + 4)
        {
            assert!(!presets.iter().any(|r| r.contains(&id)), "{id} is in a preset range");
        }
        assert!(presets[0].end <= PRESET_DELETE_FIRST && presets[1].end < GEZIK_IDS_END);
        assert!(archives.iter().all(|id| (presets[1].end..GEZIK_IDS_END).contains(id)), "below the Shell's ids");
    }

    #[test]
    fn open_with_lists_apps_then_other() {
        use gezik_platform::open_with::AppChoice;
        let apps = [
            AppChoice { path: "/A/Preview.app".into(), name: "Preview".into(), default: true },
            AppChoice { path: "/A/Safari.app".into(), name: "Safari".into(), default: false },
        ];
        let sub = open_with_sub(Some(&apps), 2);
        assert_eq!((sub.title.as_str(), sub.at), ("Open With", 2));
        assert_eq!(
            sub.items,
            [
                (OPEN_WITH_FIRST, "Preview (default)".to_owned(), true),
                (OPEN_WITH_FIRST + 1, "Safari".to_owned(), true),
                (OPEN_WITH_OTHER, "Other…".to_owned(), true),
            ]
        );
        assert_eq!(open_with_sub(Some(&[]), 0).items, [(OPEN_WITH_OTHER, "Other…".to_owned(), true)]);
        assert_eq!(
            open_with_sub(None, 0).items,
            [(HEADING, "Loading…".to_owned(), false), (OPEN_WITH_OTHER, "Other…".to_owned(), true)]
        );
        assert!(from_submenu(OPEN_WITH_FIRST + 39) && from_submenu(OPEN_WITH_OTHER));
    }

    #[test]
    fn the_9a2_ids_stay_in_their_range() {
        assert_eq!(OPEN_WITH_MAX as usize, gezik_platform::open_with::MAX_APPS);
        const { assert!(OPEN_WITH_FIRST >= 1700 && OPEN_WITH_FIRST + OPEN_WITH_MAX <= OPEN_WITH_OTHER) };
        for id in [OPEN_WITH_FIRST, OPEN_WITH_FIRST + OPEN_WITH_MAX - 1, OPEN_WITH_OTHER] {
            assert!((1700..1800).contains(&id) && ![1742, MAKE_ALIAS, SHOW_PACKAGE].contains(&id), "{id}");
        }
    }

    #[test]
    fn finder_items_are_macos_only() {
        use gezik_platform::services::Service;
        let services = [Service { title: "Resize Images".into(), file_types: vec!["public.image".into()] }];
        let (items, sub) = finder_items(Some(&services), true, 7);
        assert_eq!(items, [(SHARE, "Share…".to_owned())]);
        let sub = sub.unwrap();
        assert_eq!((sub.title.as_str(), sub.at), ("Quick Actions", 8), "after Share…");
        assert_eq!(sub.items, [(QUICK_ACTION_FIRST, "Resize Images".to_owned(), true)]);
        assert!(finder_items(Some(&[]), true, 0).1.is_none(), "no Quick Actions: no submenu");
        assert!(finder_items(None, true, 0).1.is_none(), "not known in time: none");
        assert_eq!(finder_items(Some(&services), false, 0), (Vec::new(), None), "off macOS: nothing");
        assert!(from_submenu(QUICK_ACTION_FIRST + 29) && !from_submenu(SHARE));
        assert_eq!(QUICK_ACTION_MAX as usize, gezik_platform::services::MAX_SERVICES);
        for id in [SHARE, QUICK_ACTION_FIRST, QUICK_ACTION_FIRST + QUICK_ACTION_MAX - 1] {
            assert!((1700..1780).contains(&id) && ![1742, MAKE_ALIAS, SHOW_PACKAGE, OPEN_WITH_OTHER].contains(&id));
        }
    }

    #[test]
    fn a_macos_row_keeps_all_four_submenus() {
        let list: Vec<(u32, String)> = (0..10).map(|i| (i + 100, String::new())).collect();
        let sub = |title: &str, at: usize| Submenu { title: title.into(), at, items: vec![(1, String::new(), true)] };
        let subs = [sub("Open With", 2), sub("Copy path as", 4), sub("Commands", 6), sub("Quick Actions", 10)];
        let (_, parts) = split_menu(&list, &subs);
        assert_eq!(parts.len(), MAX_SUBMENUS, "none is dropped");
    }

    #[test]
    fn conversion_ids_meet_no_others() {
        // Every range of ids, with the single ids as ranges of one.
        let singles = [
            OPEN_IN_NEW_TAB,
            PIN,
            UNPIN,
            MOVE_UP,
            MOVE_DOWN,
            DUPLICATE_TAB,
            CLOSE_TAB,
            CLOSE_OTHER_TABS,
            OPEN,
            OPEN_DEFAULT,
            RESET_COLUMNS,
            VIEW_LIST,
            VIEW_GRID,
            GRID_SMALL,
            GRID_MEDIUM,
            GRID_LARGE,
            SORT_BY_NAME,
            SORT_BY_MODIFIED,
            SORT_BY_CREATED,
            SORT_BY_TYPE,
            SORT_BY_SIZE,
            SORT_ASC,
            SORT_DESC,
            PREVIEW_PANE,
            APPLY_TO_ALL,
            RESET_FOLDER,
            SYSTEM_INTEGRATION,
            UNDO,
            REDO,
            PASTE,
            NEW_FOLDER,
            NEW_FILE,
            REFRESH,
            CUT,
            COPY,
            DUPLICATE,
            RENAME,
            TRASH,
            DELETE_PERMANENTLY,
            PASTE_INTO,
            BATCH_RENAME,
            COPY_HERE,
            MOVE_HERE,
            CANCEL_DROP,
            CREATE_LINK_HERE,
            NEW_MARKDOWN,
            OPEN_TEMPLATES,
            NEW_FOLDER_WITH_SELECTION,
            PASTE_AS_FILE,
            LINK_SHORTCUT,
            LINK_JUNCTION,
            LINK_SYMLINK,
            MAKE_ALIAS,
            SHOW_PACKAGE,
            OPEN_WITH_FIRST,
            OPEN_WITH_FIRST + OPEN_WITH_MAX - 1,
            OPEN_WITH_OTHER,
            SHARE,
            QUICK_ACTION_FIRST,
            QUICK_ACTION_FIRST + QUICK_ACTION_MAX - 1,
            PRESET_SAVE,
            EXTRACT_HERE,
            EXTRACT_TO_OWN,
            EXTRACT_TO,
            COMPRESS,
            COMPRESS_TO,
            ADD_TO_ARCHIVE,
            CONVERT,
            IMAGES_TO_PDF,
            LOCK_TAB,
            UNLOCK_TAB,
            SAVE_TAB_SET,
            FILTER_SAVE,
            COMMAND_GROUP,
            OPEN_TERMINAL,
            OPEN_TERMINAL_ADMIN,
            HIDE_EXTENSIONS,
            FOLDERS_FIRST,
            SINGLE_CLICK_OPEN,
            SHOW_HIDDEN,
            SHOW_SYSTEM,
            RENAME_PIN,
            GROUP_NEW,
            GROUP_NONE,
            GROUP_UP,
            GROUP_DOWN,
            GROUP_RENAME,
            UNGROUP,
            CALC_FOLDER_SIZES,
            SAVE_SEARCH,
            RUN_SEARCH_NEW_TAB,
            RENAME_SEARCH,
            DELETE_SEARCH,
            PUT_BACK,
            TRASH_DELETE,
            EMPTY_TRASH,
        ];
        let mut ranges: Vec<std::ops::Range<u32>> = singles.iter().map(|id| *id..id + 1).collect();
        ranges.extend([
            TOGGLE_COLUMN_FIRST..RESET_COLUMNS,
            CONFLICT_FIRST..CONFLICT_FIRST + 4,
            ADD_RULE_FIRST..ADD_RULE_FIRST + 10,
            PRESET_FIRST..PRESET_FIRST + PRESET_MAX,
            PRESET_DELETE_FIRST..PRESET_DELETE_FIRST + PRESET_MAX,
            ENCODING_FROM_FIRST..ENCODING_FROM_FIRST + ENCODING_MAX,
            ENCODING_TO_FIRST..ENCODING_TO_FIRST + ENCODING_MAX,
            COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX,
            CONVERT_PRESET_FIRST..CONVERT_PRESET_FIRST + CONVERT_PRESET_MAX,
            FILTER_FIRST..FILTER_FIRST + FILTER_MAX,
            FILTER_DELETE_FIRST..FILTER_DELETE_FIRST + FILTER_MAX,
            COPY_PATH_FIRST..COPY_PATH_FIRST + 7,
            TAB_SET_OPEN_FIRST..TAB_SET_OPEN_FIRST + TAB_SET_MAX,
            TAB_SET_REPLACE_FIRST..TAB_SET_REPLACE_FIRST + TAB_SET_MAX,
            TAB_SET_DELETE_FIRST..TAB_SET_DELETE_FIRST + TAB_SET_MAX,
            DATE_FORMAT_FIRST..DATE_FORMAT_FIRST + DateFormat::ALL.len() as u32,
            SIZE_FORMAT_FIRST..SIZE_FORMAT_FIRST + SizeFormat::ALL.len() as u32,
            GROUP_MOVE_FIRST..GROUP_MOVE_FIRST + GROUP_MAX,
            TEMPLATE_FIRST..TEMPLATE_FIRST + TEMPLATE_MAX,
            SAVED_SEARCH_FIRST..SAVED_SEARCH_FIRST + SAVED_SEARCH_MAX,
            SAVED_SEARCH_DELETE_FIRST..SAVED_SEARCH_DELETE_FIRST + SAVED_SEARCH_MAX,
        ]);
        for (i, a) in ranges.iter().enumerate() {
            assert!(a.start >= 1 && a.end <= GEZIK_IDS_END, "{a:?}: 1..4096 (0 is a heading, 4096 on the Shell's)");
            for b in &ranges[i + 1..] {
                assert!(a.end <= b.start || b.end <= a.start, "{a:?} meets {b:?}");
            }
        }
        assert!(!ranges.iter().any(|r| r.contains(&HEADING)), "a heading is never an item");
        let encodings = gezik_batch::convert::text::encodings().len() as u32;
        assert!(encodings < ENCODING_MAX, "{encodings} encodings and the first choice fit");
    }

    fn ids(v: Vec<(u32, &str)>) -> Vec<u32> {
        v.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn terminal_and_copy_path_items() {
        assert_eq!(ids(terminal_items(false)), [OPEN_TERMINAL]);
        assert_eq!(ids(terminal_items(true)), [OPEN_TERMINAL, OPEN_TERMINAL_ADMIN]);
        let titles = |items: Vec<(u32, String, bool)>| items.into_iter().map(|(_, t, _)| t).collect::<Vec<_>>();
        assert_eq!(
            titles(copy_path_items(false, false)),
            ["Full path", "Quoted", "Name", "Folder path", "file:// URL"]
        );
        let windows = copy_path_items(true, true);
        assert_eq!(windows.len(), 7);
        assert_eq!(windows[6], (COPY_PATH_FIRST + 6, "UNC path".to_owned(), true));
        assert_eq!(copy_path_items(false, false)[2].0, COPY_PATH_FIRST + 3, "ids follow PathFormat::ALL, not the menu");
    }

    #[test]
    fn file_items_depend_on_the_selection() {
        assert_eq!(ids(file_items(true, false, true)), [CUT, COPY, DUPLICATE, RENAME, TRASH, DELETE_PERMANENTLY]);
        let several = ids(file_items(false, true, true));
        assert_eq!(several, [CUT, COPY, PASTE_INTO, DUPLICATE, RENAME, TRASH, DELETE_PERMANENTLY]);
        assert!(!ids(file_items(true, true, false)).contains(&PASTE_INTO));
        assert!(file_items(false, false, false).contains(&(RENAME, "Rename items…")));
    }

    #[test]
    fn the_filter_menu_lists_saves_and_deletes() {
        let names = vec!["Resimler".to_owned(), "Belgeler".to_owned()];
        let items = filter_items(&names, true);
        let ids: Vec<u32> = items.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(ids, [FILTER_FIRST, FILTER_FIRST + 1, FILTER_SAVE, FILTER_DELETE_FIRST, FILTER_DELETE_FIRST + 1]);
        assert_eq!(items[2], (FILTER_SAVE, "Save as…".to_owned(), true));
        assert_eq!(items[3].1, "Delete \"Resimler\"");
        assert!(!filter_items(&[], false)[0].2, "nothing to save: greyed");
    }

    #[test]
    fn preset_menu_lists_saves_and_deletes() {
        let names = ["Photos".to_owned(), "Music".to_owned()];
        let list = preset_items(&names);
        let ids: Vec<u32> = list.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [PRESET_FIRST, PRESET_FIRST + 1, PRESET_SAVE, PRESET_DELETE_FIRST, PRESET_DELETE_FIRST + 1]);
        assert_eq!(list[3].1, "Delete \"Photos\"");
        assert_eq!(preset_items(&[]), [(PRESET_SAVE, "Save current rules as…".to_owned())]);
        let many: Vec<String> = (0..PRESET_MAX + 5).map(|i| format!("Set {i}")).collect();
        let list = preset_items(&many);
        assert_eq!(list.len(), 2 * PRESET_MAX as usize + 1);
        assert_eq!(list.last().unwrap().0, PRESET_DELETE_FIRST + PRESET_MAX - 1);
        assert!(gezik_core::batch::rules::KINDS.len() <= 10, "Add rule has ids 90-99");
    }

    #[test]
    fn background_items_say_what_undo_does() {
        let items = background_items(Some("Copy 3 items"), None, true, None, true);
        assert_eq!(items[0], (UNDO, "Undo Copy 3 items".to_owned()));
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [UNDO, PASTE, NEW_FOLDER, NEW_FILE, REFRESH]);
        let bare: Vec<u32> = background_items(None, None, false, None, true).iter().map(|(id, _)| *id).collect();
        assert_eq!(bare, [NEW_FOLDER, NEW_FILE, REFRESH]);
    }

    #[test]
    fn the_background_menu_says_what_paste_does() {
        use gezik_core::templates::PasteKind;
        let ids = |items: Vec<(u32, String)>| items.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(
            ids(background_items(None, None, true, Some(PasteKind::Image), true)),
            [PASTE, NEW_FOLDER, NEW_FILE, REFRESH],
            "files paste as files"
        );
        assert_eq!(
            background_items(None, None, false, Some(PasteKind::Image), false),
            [(PASTE_AS_FILE, "Paste image as file".to_owned()), (REFRESH, "Refresh".to_owned())],
            "macOS and Linux: New folder and New file are in New ▸"
        );
        assert_eq!(background_items(None, None, false, Some(PasteKind::Text), true)[0].1, "Paste text as file");
    }

    #[test]
    fn new_lists_the_built_ins_the_templates_and_the_folder() {
        let templates = gezik_core::templates::template_list([
            ("Report.docx".to_owned(), false, false),
            ("Project".to_owned(), true, false),
        ]);
        let titles = |sub: &Submenu| sub.items.iter().map(|(id, t, _)| (*id, t.clone())).collect::<Vec<_>>();
        let elsewhere = new_sub(&templates, false, 3);
        assert_eq!((elsewhere.title.as_str(), elsewhere.at), ("New", 3));
        let expected: Vec<(u32, String)> = [
            (NEW_FOLDER, "Folder"),
            (NEW_FILE, "Text file"),
            (NEW_MARKDOWN, "Markdown file"),
            (TEMPLATE_FIRST, "Project"),
            (TEMPLATE_FIRST + 1, "Report"),
            (OPEN_TEMPLATES, "Open templates folder"),
        ]
        .iter()
        .map(|(id, t)| (*id, (*t).to_owned()))
        .collect();
        assert_eq!(titles(&elsewhere), expected);
        let windows = new_sub(&templates, true, 5);
        assert_eq!(windows.title, "New from template", "Explorer's own New ▸ is in the same menu");
        assert_eq!(titles(&windows)[0], (NEW_MARKDOWN, "Markdown file".to_owned()));
        assert!(from_submenu(TEMPLATE_FIRST + 49) && from_submenu(NEW_MARKDOWN) && from_submenu(OPEN_TEMPLATES));
        assert_eq!(TEMPLATE_MAX as usize, gezik_core::templates::TEMPLATE_MAX);
    }

    #[test]
    fn link_items_follow_the_system_and_what_can_be_made() {
        let ids = |v: Vec<(u32, String, bool)>| v.into_iter().map(|(id, _, _)| id).collect::<Vec<_>>();
        assert_eq!(ids(link_items(true, false, true, true)), [LINK_SHORTCUT, LINK_JUNCTION, LINK_SYMLINK]);
        assert_eq!(ids(link_items(true, false, false, false)), [LINK_SHORTCUT]);
        assert_eq!(link_items(false, false, false, true), [(LINK_SYMLINK, "Create link".to_owned(), true)]);
    }

    #[test]
    fn macos_rows_offer_aliases_and_package_contents() {
        let ids = |v: Vec<(u32, String, bool)>| v.into_iter().map(|(id, _, _)| id).collect::<Vec<_>>();
        assert_eq!(ids(link_items(false, true, false, true)), [MAKE_ALIAS, LINK_SYMLINK], "macOS: Make Alias first");
        let app = Path::new("/Applications/Safari.app");
        assert_eq!(package_item(app, true, true), Some((SHOW_PACKAGE, "Show Package Contents".to_owned())));
        assert_eq!(package_item(app, true, false), None, "not off macOS");
        assert_eq!(package_item(Path::new("/x/Docs"), true, true), None);
        assert_eq!(package_item(Path::new("/x/Rapor.pages"), false, true), None, "a one-file document");
        assert_eq!(link_kind(MAKE_ALIAS), LinkKind::Alias);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_is_offered_for_local_folders_only() {
        let row = |path: &str, is_dir| (PathBuf::from(path), is_dir);
        let network = [PathBuf::from(r"Z:\")];
        assert!(junction_offered(&[row(r"C:\a", true), row(r"C:\b", true)], &network));
        assert!(!junction_offered(&[row(r"C:\a", true), row(r"C:\f.txt", false)], &network), "a file");
        assert!(!junction_offered(&[row(r"\\srv\share\a", true)], &network), "a share");
        assert!(!junction_offered(&[row(r"Z:\a", true)], &network), "a mapped drive");
        assert!(!junction_offered(&[], &network));
    }

    #[cfg(windows)]
    #[test]
    fn keys_up_after_a_native_menu_are_released() {
        use gezik_platform::ModifierKeys;
        use slint::platform::Key;
        let all_up = released_modifiers(ModifierKeys::default());
        assert_eq!(all_up.len(), 8);
        assert!(all_up.contains(&Key::Shift) && all_up.contains(&Key::ShiftR) && all_up.contains(&Key::AltGr));
        // Shift still held when the menu closes: its own release comes later as usual.
        let held = released_modifiers(ModifierKeys { left_shift: true, alt: true, ..Default::default() });
        assert!(!held.contains(&Key::Shift) && !held.contains(&Key::Alt));
        assert!(held.contains(&Key::ShiftR) && held.contains(&Key::Control));
    }

    #[test]
    fn folder_rows_offer_new_tab_and_pin_toggle() {
        assert_eq!(ids(items(Place::Row { is_dir: true, pinned: false }, true)), [OPEN_IN_NEW_TAB, PIN]);
        assert_eq!(ids(items(Place::Row { is_dir: true, pinned: true }, true)), [OPEN_IN_NEW_TAB, UNPIN]);
    }

    #[test]
    fn file_rows_rely_on_the_system_menu_on_windows() {
        assert!(items(Place::Row { is_dir: false, pinned: false }, true).is_empty());
        assert_eq!(ids(items(Place::Row { is_dir: false, pinned: false }, false)), [OPEN, OPEN_DEFAULT]);
    }

    #[test]
    fn several_rows_get_the_system_menu_or_open() {
        assert!(items(Place::Rows, true).is_empty());
        assert_eq!(ids(items(Place::Rows, false)), [OPEN]);
    }

    #[test]
    fn pinned_sidebar_rows_can_move_within_bounds() {
        let first = items(Place::Sidebar { pinned_section: true, pinned: true, first: true, last: false }, true);
        assert_eq!(ids(first), [OPEN_IN_NEW_TAB, UNPIN, MOVE_DOWN, RENAME_PIN]);
        let middle = items(Place::Sidebar { pinned_section: true, pinned: true, first: false, last: false }, true);
        assert_eq!(ids(middle), [OPEN_IN_NEW_TAB, UNPIN, MOVE_UP, MOVE_DOWN, RENAME_PIN]);
        let folder = items(Place::Sidebar { pinned_section: false, pinned: false, first: false, last: false }, true);
        assert_eq!(ids(folder), [OPEN_IN_NEW_TAB, PIN]);
    }

    #[test]
    fn move_to_group_lists_the_other_groups_new_and_none() {
        let groups = vec!["Work".to_owned(), "Media".to_owned()];
        let titles = |items: Vec<(u32, String, bool)>| items.into_iter().map(|(id, t, _)| (id, t)).collect::<Vec<_>>();
        assert_eq!(
            titles(group_items(&groups, None)),
            [
                (GROUP_MOVE_FIRST, "Work".to_owned()),
                (GROUP_MOVE_FIRST + 1, "Media".to_owned()),
                (GROUP_NEW, "New group…".to_owned())
            ]
        );
        assert_eq!(
            titles(group_items(&groups, Some("work"))),
            [
                (GROUP_MOVE_FIRST + 1, "Media".to_owned()),
                (GROUP_NEW, "New group…".to_owned()),
                (GROUP_NONE, "No group".to_owned())
            ],
            "its own group left out; the ids go by place"
        );
        let many: Vec<String> = (0..GROUP_MAX + 5).map(|i| format!("G{i}")).collect();
        assert_eq!(group_items(&many, None).len(), GROUP_MAX as usize + 1);
        assert!(from_submenu(GROUP_MOVE_FIRST + 3) && from_submenu(GROUP_NEW) && from_submenu(GROUP_NONE));
        assert!(!from_submenu(RENAME_PIN));
    }

    #[test]
    fn a_group_heading_moves_renames_and_ungroups() {
        assert_eq!(ids(group_heading_items(true, false)), [GROUP_DOWN, GROUP_RENAME, UNGROUP]);
        assert_eq!(ids(group_heading_items(false, true)), [GROUP_UP, GROUP_RENAME, UNGROUP]);
        assert_eq!(ids(group_heading_items(false, false)), [GROUP_UP, GROUP_DOWN, GROUP_RENAME, UNGROUP]);
        assert_eq!(ids(group_heading_items(true, true)), [GROUP_RENAME, UNGROUP]);
    }

    #[test]
    fn one_native_menu_at_a_time() {
        let gate = MenuGate::default();
        let first = gate.claim();
        assert!(first.is_some());
        assert!(gate.claim().is_none(), "a second right-click while one is pending is dropped");
        drop(first);
        assert!(gate.claim().is_some(), "the next menu opens once the first is closed");
    }

    #[test]
    fn header_menu_toggles_each_column_and_resets() {
        use gezik_core::view::default_columns;
        let got = header_items(&default_columns());
        assert_eq!(
            got,
            [
                (TOGGLE_COLUMN_FIRST, "Hide Modified"),
                (TOGGLE_COLUMN_FIRST + 1, "Show Created"),
                (TOGGLE_COLUMN_FIRST + 2, "Hide Type"),
                (TOGGLE_COLUMN_FIRST + 3, "Hide Size"),
                (RESET_COLUMNS, "Reset columns"),
            ]
        );
    }

    #[test]
    fn the_tab_menu_offers_the_lock_and_no_close_on_a_locked_tab() {
        let tab = |only_tab, locked| ids(items(Place::Tab { only_tab, locked }, false));
        assert_eq!(tab(false, false), [DUPLICATE_TAB, LOCK_TAB, CLOSE_TAB, CLOSE_OTHER_TABS, SAVE_TAB_SET]);
        assert_eq!(tab(true, false), [DUPLICATE_TAB, LOCK_TAB, CLOSE_TAB, SAVE_TAB_SET]);
        assert_eq!(tab(false, true), [DUPLICATE_TAB, UNLOCK_TAB, CLOSE_OTHER_TABS, SAVE_TAB_SET]);
        assert_eq!(tab(true, true), [DUPLICATE_TAB, UNLOCK_TAB, SAVE_TAB_SET]);
    }

    #[test]
    fn the_tab_set_menu_opens_replaces_and_deletes() {
        let names = vec!["Work".to_owned(), "Media".to_owned()];
        let items = tab_set_items(&names);
        let ids: Vec<u32> = items.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(
            ids,
            [
                TAB_SET_OPEN_FIRST,
                TAB_SET_OPEN_FIRST + 1,
                TAB_SET_REPLACE_FIRST,
                TAB_SET_REPLACE_FIRST + 1,
                TAB_SET_DELETE_FIRST,
                TAB_SET_DELETE_FIRST + 1
            ]
        );
        assert_eq!(items[2].1, "Replace tabs with \"Work\"");
        assert_eq!(items[5].1, "Delete \"Media\"");
        assert!(tab_set_items(&[]).is_empty());
        let many: Vec<String> = (0..TAB_SET_MAX + 3).map(|i| format!("S{i}")).collect();
        assert_eq!(tab_set_items(&many).len(), 3 * TAB_SET_MAX as usize);
    }

    #[test]
    fn view_menu_marks_the_current_choices() {
        use gezik_core::view::{GridSize, SortDir, SortKey, SortSpec, ViewMode, ViewOptions, ViewSettings};
        let list = view_items(ViewSettings::default(), false, false, ViewOptions::default(), false);
        let ids: Vec<u32> = list.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            [
                VIEW_LIST,
                VIEW_GRID,
                SORT_BY_NAME,
                SORT_BY_MODIFIED,
                SORT_BY_CREATED,
                SORT_BY_TYPE,
                SORT_BY_SIZE,
                SORT_ASC,
                SORT_DESC,
                PREVIEW_PANE,
                TOGGLE_STACK,
                SHOW_HISTORY,
                HIDE_EXTENSIONS,
                FOLDERS_FIRST,
                SINGLE_CLICK_OPEN,
                SHOW_HIDDEN,
                APPLY_TO_ALL,
                RESET_FOLDER,
                SYSTEM_INTEGRATION
            ]
        );
        assert!(list[0].1.starts_with("• ") && !list[1].1.starts_with("• "));
        assert_eq!(list.last().map(|(id, _)| *id), Some(SYSTEM_INTEGRATION));
        let grid = ViewSettings {
            mode: ViewMode::Grid,
            sort: SortSpec { key: SortKey::Size, dir: SortDir::Desc },
            grid_size: GridSize::Large,
        };
        let items = view_items(grid, false, false, ViewOptions::default(), false);
        let marked: Vec<&str> =
            items.iter().filter(|(_, t)| t.starts_with("• ")).map(|(_, t)| t.trim_start_matches("• ")).collect();
        let mut expected = vec!["Grid", "Large icons", "Sort by size", "Descending", "Folders first"];
        if !cfg!(target_os = "macos") {
            expected.push("Show hidden items");
        }
        assert_eq!(marked, expected);
        assert!(
            view_items(ViewSettings::default(), true, false, ViewOptions::default(), false)
                .iter()
                .any(|(id, t)| *id == PREVIEW_PANE && t.starts_with("• "))
        );
    }

    #[test]
    fn the_view_menu_shows_and_hides_the_drop_stack() {
        use gezik_core::view::{ViewOptions, ViewSettings};
        let shown = view_items(ViewSettings::default(), false, true, ViewOptions::default(), false);
        assert!(shown.contains(&(TOGGLE_STACK, "• Drop stack".to_owned())));
        let hidden = view_items(ViewSettings::default(), false, false, ViewOptions::default(), false);
        assert!(hidden.contains(&(TOGGLE_STACK, "    Drop stack".to_owned())));
    }

    #[test]
    fn view_menu_lists_the_options_and_their_marks() {
        use gezik_core::view::{DateFormat, ViewOptions};
        let options = ViewOptions { hide_extensions: true, ..ViewOptions::default() };
        let items = view_items(ViewSettings::default(), false, false, options, true);
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            &ids[12..],
            [
                HIDE_EXTENSIONS,
                FOLDERS_FIRST,
                SINGLE_CLICK_OPEN,
                SHOW_HIDDEN,
                SHOW_SYSTEM,
                APPLY_TO_ALL,
                RESET_FOLDER,
                SYSTEM_INTEGRATION
            ]
        );
        assert!(items[12].1.starts_with("• ") && items[13].1.starts_with("• "), "extensions hidden, folders first");
        assert!(!items[14].1.starts_with("• "));
        assert_eq!(items[15].1.starts_with("• "), options.show_hidden);
        let elsewhere: Vec<u32> =
            view_items(ViewSettings::default(), false, false, options, false).iter().map(|(id, _)| *id).collect();
        assert!(!elsewhere.contains(&SHOW_SYSTEM), "Show system items: Windows only");
        let subs = format_subs(ViewOptions { date_format: DateFormat::Iso, ..options }, 17);
        let places: Vec<(&str, usize)> = subs.iter().map(|s| (s.title.as_str(), s.at)).collect();
        assert_eq!(places, [("Date format", 17), ("Size format", 17)]);
        let dates: Vec<(u32, &str)> = subs[0].items.iter().map(|(id, t, _)| (*id, t.as_str())).collect();
        assert_eq!(
            dates,
            [
                (DATE_FORMAT_FIRST, "    Relative"),
                (DATE_FORMAT_FIRST + 1, "    Short"),
                (DATE_FORMAT_FIRST + 2, "• ISO"),
                (DATE_FORMAT_FIRST + 3, "    System")
            ]
        );
        assert_eq!(subs[1].items.len(), 2);
    }

    #[test]
    fn the_mac_menu_bar_names_the_formats_as_the_view_menu_does() {
        use gezik_core::view::{DateFormat, SizeFormat};
        let bar = include_str!("../ui/app.slint");
        let dates = DateFormat::ALL.iter().enumerate().map(|(k, f)| (f.label(), DATE_FORMAT_FIRST + k as u32));
        let sizes = SizeFormat::ALL.iter().enumerate().map(|(k, f)| (f.label(), SIZE_FORMAT_FIRST + k as u32));
        for (label, id) in dates.chain(sizes) {
            let title = format!("title: \"{label}\";");
            let command = format!("\"view-option:{id}\"");
            assert!(bar.lines().any(|line| line.contains(&title) && line.contains(&command)), "{label} ({id})");
        }
    }

    #[test]
    fn view_menu_ids_say_which_option_changes() {
        use gezik_config::settings::ViewOption;
        use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
        let options = ViewOptions::default();
        assert_eq!(view_option_for(HIDE_EXTENSIONS, options), Some(ViewOption::HideExtensions(true)));
        assert_eq!(view_option_for(FOLDERS_FIRST, options), Some(ViewOption::FoldersFirst(false)));
        assert_eq!(view_option_for(SINGLE_CLICK_OPEN, options), Some(ViewOption::SingleClickOpen(true)));
        assert_eq!(view_option_for(SHOW_HIDDEN, options), Some(ViewOption::ShowHidden(!options.show_hidden)));
        assert_eq!(view_option_for(SHOW_SYSTEM, options), Some(ViewOption::ShowSystem(true)));
        assert_eq!(view_option_for(DATE_FORMAT_FIRST, options), Some(ViewOption::DateFormat(DateFormat::Relative)));
        assert_eq!(view_option_for(SIZE_FORMAT_FIRST + 1, options), Some(ViewOption::SizeFormat(SizeFormat::Decimal)));
        assert_eq!(view_option_for(SIZE_FORMAT_FIRST + 2, options), None);
        assert_eq!(view_option_for(VIEW_LIST, options), None);
        assert!(
            from_submenu(DATE_FORMAT_FIRST + 3) && from_submenu(SIZE_FORMAT_FIRST) && !from_submenu(HIDE_EXTENSIONS)
        );
    }

    #[test]
    fn the_search_menus_mark_what_is_chosen_and_stay_in_their_range() {
        use gezik_core::search::{DateRange, KindFilter, Scope, SearchSpec};
        let mut spec = SearchSpec::new(Scope::Folder("/w".into()));
        spec.modified = DateRange::LastDays(7);
        spec.kind = KindFilter::Videos;
        spec.match_case = true;
        spec.size.min = Some(500 * 1024 * 1024);
        let (items, subs) = search_filter_items(&spec);
        assert!(items.iter().any(|(id, title, _)| *id == SIZE_MIN && title == "Size at least… (500 MB)"));
        assert!(items.iter().any(|(id, title, _)| *id == OPTION_FIRST + 2 && title.starts_with("• Match case")));
        assert!(items.iter().any(|(id, _, on)| *id == FILTERS_CLEAR && *on));
        let modified = subs.iter().find(|s| s.title == "Modified").unwrap();
        assert!(
            modified.items.iter().any(|(id, title, _)| *id == MODIFIED_FIRST + 2 && title.starts_with("• Last 7 days"))
        );
        let kinds = subs.iter().find(|s| s.title == "Type").unwrap();
        assert_eq!(kinds.items.len(), 9);
        assert!(kinds.items[4].1.starts_with("• Videos"));
        let ids: Vec<u32> = items
            .iter()
            .map(|i| i.0)
            .chain(subs.iter().flat_map(|s| s.items.iter().map(|i| i.0)))
            .chain(search_more_items(3, &[], true).iter().map(|i| i.0).filter(|id| *id != SAVE_SEARCH))
            .collect();
        assert!(ids.iter().all(|id| (1500..1600).contains(id)), "{ids:?}");
        assert!(search_more_items(0, &[], true).iter().all(|(id, _, _)| *id != SEARCH_PROBLEMS));
        assert_eq!(modified_for(MODIFIED_FIRST + 3), Some(DateRange::LastDays(30)));
        assert_eq!(modified_for(MODIFIED_BETWEEN), None, "asked for in a box");
        let choices =
            vec![(Scope::Folder("/w".into()), "This folder (w)".to_owned()), (Scope::AllDrives, "This PC".to_owned())];
        let scope = search_scope_items(&choices, &Scope::AllDrives);
        assert_eq!(scope[1], (SCOPE_FIRST + 1, "• This PC".to_owned(), true));
    }
}
