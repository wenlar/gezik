//! The trash view (spec 7.1, part 9b2): every bin of this user in one list. Its rows are the
//! entries in the bins, shown by the names and places they had (`TrashLabel`). Read only when
//! shown; watched only while shown; nothing kept after.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gezik_config::shortcuts::Action;
use gezik_core::Entry;
use gezik_platform::trash::TrashItem;
use gezik_search::results::{Batch, ResultSet, TrashLabel};

use crate::folder_watch::FolderWatch;
use crate::view::View;

/// The status bar's word when macOS keeps the home Trash from Gezik.
const DENIED: &str = "Gezik needs Full Disk Access to show the Trash";

/// What a load of the trash gives the UI thread.
#[derive(Debug)]
pub struct Loaded {
    pub set: ResultSet,
    pub bins: Vec<PathBuf>,
    pub denied: bool,
}

/// Reads every bin (blocking: the navigator's loading thread).
pub fn load() -> Loaded {
    let list = gezik_platform::trash::list();
    Loaded { set: to_set(list.items), bins: list.bins, denied: list.denied }
}

/// The rows: each entry in its bin's folder (every folder once), labelled with what it was.
/// A row's place is the record's checked one or blank (unknown), never anything made up.
pub fn to_set(items: Vec<TrashItem>) -> ResultSet {
    let mut batch = Batch::default();
    let mut labels = Vec::with_capacity(items.len());
    let mut folders: HashMap<PathBuf, u32> = HashMap::new();
    for item in items {
        // shortcut: an entry whose name is not UTF-8 (Linux) is left out; read it lossless if users miss one.
        let (Some(dir), Some(entry_name)) = (item.trashed.parent(), item.trashed.file_name().and_then(|n| n.to_str()))
        else {
            continue;
        };
        // What a delete is still working on under a hidden name.
        if gezik_ops::pending::is_internal_name(entry_name) {
            continue;
        }
        let next = u32::try_from(folders.len()).unwrap_or(u32::MAX);
        let parent = *folders.entry(dir.to_path_buf()).or_insert_with(|| {
            batch.folders.push(dir.display().to_string().into());
            next
        });
        batch.entries.push(Entry {
            name: entry_name.to_owned(),
            is_dir: item.is_dir,
            // A folder's whole size is in its `$I` record on Windows; elsewhere it is not known.
            flags: if item.is_dir && cfg!(windows) { Entry::SIZED } else { 0 },
            size: item.size,
            modified: item.deleted,
            created: None,
        });
        batch.parent.push(parent);
        labels.push(TrashLabel {
            name: item.name.into(),
            folder: item
                .original
                .as_deref()
                .and_then(Path::parent)
                .map(|p| p.display().to_string())
                .unwrap_or_default()
                .into(),
            original: item.original,
            info: item.info,
        });
    }
    let mut set = ResultSet::trash();
    set.append_trash(batch, labels);
    set
}

/// Bins watched while the trash is shown (spec 7.1); nothing otherwise.
struct Watching {
    bins: Vec<PathBuf>,
    watches: Vec<FolderWatch>,
}

thread_local! {
    static WATCHING: RefCell<Option<Watching>> = const { RefCell::new(None) };
    /// One reload, a moment after the last change.
    static RELOAD: slint::Timer = slint::Timer::default();
    static ASKED: Cell<bool> = const { Cell::new(false) };
}

/// The trash was read and is going on screen: watch its bins (Linux: their `info/`, which
/// changes with every item), and on macOS ask once for Full Disk Access. Returns the status
/// bar's note when the home Trash could not be read.
pub fn shown(bins: &[PathBuf], denied: bool) -> Option<String> {
    WATCHING.with(|w| {
        let mut w = w.borrow_mut();
        if w.as_ref().is_some_and(|w| w.bins == bins) {
            return;
        }
        let watches = bins
            .iter()
            .map(|bin| {
                let watch = FolderWatch::new(|| {
                    let _ = slint::invoke_from_event_loop(bin_changed);
                });
                let dir = if cfg!(target_os = "linux") { bin.join("info") } else { bin.clone() };
                watch.watch(Some(&dir));
                watch
            })
            .collect();
        *w = Some(Watching { bins: bins.to_vec(), watches });
    });
    if !denied {
        return None;
    }
    if ASKED.with(first_denial) {
        ask_for_full_disk_access();
    }
    Some(DENIED.to_owned())
}

/// The trash is no longer shown: the watches go.
pub fn left() {
    if WATCHING.with(|w| w.borrow_mut().take()).is_some() {
        RELOAD.with(slint::Timer::stop);
    }
}

/// Whether to ask about Full Disk Access now: the first time in this session only.
pub fn first_denial(asked: &Cell<bool>) -> bool {
    !asked.replace(true)
}

/// A watched bin changed: take the change (so the next one says so again), then reload.
fn bin_changed() {
    WATCHING.with(|w| {
        if let Some(w) = w.borrow().as_ref() {
            for watch in &w.watches {
                watch.take_change();
            }
        }
    });
    changed();
}

/// The trash may have changed (a bin, or a job of Gezik's): read it again a second after the
/// last change, if it still shows. Many changes in a row (a long job) give one read.
pub fn changed() {
    RELOAD.with(|timer| {
        timer.start(slint::TimerMode::SingleShot, Duration::from_secs(1), || {
            crate::panes::with_active(|p| p.nav.refresh_trash());
        });
    });
}

fn ask_for_full_disk_access() {
    crate::operations::with_current(|ops| {
        ops.dialogs().ask(
            format!("{DENIED}."),
            "Allow it in System Settings ▸ Privacy & Security ▸ Full Disk Access, then open the Trash again.",
            &["Open Privacy Settings", "Not Now"],
            |choice| {
                if choice == Some(0) {
                    // To be checked on a Mac (spec 7.1).
                    let _ = std::process::Command::new("/usr/bin/open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
                        .spawn();
                }
            },
        );
    });
}

/// What an action that is not the trash's own says there.
pub fn not_here() -> String {
    format!("Not available in the {}", gezik_core::nav::TRASH_NAME)
}

/// Whether `action` may run while the trash is shown: what only looks, moves around, undoes or
/// works on the trash itself. Anything that would act on a `$R…` name is out (decision 7).
/// Listed one by one: a new action is a compile error here until it is placed.
pub fn allowed(action: Action) -> bool {
    match action {
        Action::NewTab
        | Action::NewWindow
        | Action::CloseTab
        | Action::NextTab
        | Action::PrevTab
        | Action::Back
        | Action::Forward
        | Action::Up
        | Action::FocusPath
        | Action::Refresh
        | Action::SelectAll
        | Action::ViewList
        | Action::ViewGrid
        | Action::TogglePreview
        | Action::QuickLook
        | Action::Trash
        | Action::DeletePermanently
        | Action::Undo
        | Action::Redo
        | Action::CommandPalette
        | Action::QuickOpen
        | Action::Filter
        | Action::InvertSelection
        | Action::SelectPattern
        | Action::DeselectPattern
        | Action::SelectSameType
        | Action::RestoreSelection
        | Action::Tab1
        | Action::Tab2
        | Action::Tab3
        | Action::Tab4
        | Action::Tab5
        | Action::Tab6
        | Action::Tab7
        | Action::Tab8
        | Action::TabLast
        | Action::ReopenTab
        | Action::TabPicker
        | Action::ToggleTabLock
        | Action::ClearHistory
        | Action::ToggleHidden
        | Action::Pin1
        | Action::Pin2
        | Action::Pin3
        | Action::Pin4
        | Action::Pin5
        | Action::Pin6
        | Action::Pin7
        | Action::Pin8
        | Action::Pin9
        | Action::ShowHistory
        | Action::ToggleStack
        | Action::SaveTabSet
        | Action::ShowTrash
        | Action::PutBack
        | Action::EmptyTrash
        | Action::SystemIntegration
        | Action::ConnectToServer
        | Action::GroupNone
        | Action::GroupType
        | Action::GroupDate
        | Action::GroupSize
        | Action::CollapseGroups
        | Action::ExpandGroups
        | Action::RevealInTree
        | Action::ToggleDualPane
        | Action::FocusOtherPane => true,
        // No drive to eject in the trash.
        Action::Eject
        | Action::Rename
        | Action::NewFolder
        | Action::Copy
        | Action::Cut
        | Action::Paste
        | Action::PasteMove
        | Action::Duplicate
        | Action::BatchRename
        | Action::OpenTerminal
        | Action::OpenTerminalAdmin
        | Action::CopyPath
        | Action::NewFolderWithSelection
        | Action::AddToStack
        | Action::Search
        | Action::FlatView
        | Action::ShowInFolder
        | Action::KeepOffline
        | Action::FreeUpSpace
        | Action::GetInfo
        | Action::MakeAlias
        | Action::ShowPackageContents
        | Action::Share
        | Action::CopyWithFolders
        | Action::CutWithFolders
        | Action::CalculateFolderSizes
        | Action::SaveSearch => false,
    }
}

/// What an action does while the trash is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InTrash {
    /// As anywhere.
    Run,
    /// Delete and Delete Permanently both: Delete Permanently, asked first.
    DeleteForGood,
    /// Not here (`not_here`).
    Refuse,
}

pub fn in_trash(action: Action) -> InTrash {
    match action {
        Action::Trash | Action::DeletePermanently => InTrash::DeleteForGood,
        action if allowed(action) => InTrash::Run,
        _ => InTrash::Refuse,
    }
}

/// Does what `action` is in the trash instead, when that is not what it does anywhere;
/// returns whether it did (the caller then does nothing more).
pub fn instead(action: Action, view: &View) -> bool {
    if !view.shows_trash() {
        return false;
    }
    match in_trash(action) {
        InTrash::Run => false,
        InTrash::DeleteForGood => {
            delete_selection(view);
            true
        }
        InTrash::Refuse => {
            view.note(not_here());
            true
        }
    }
}

/// Opening an entry in the trash: it is not where it was, so it is put back first.
pub fn open_note() -> String {
    format!("Put it back to open it: nothing in the {} is opened", gezik_core::nav::TRASH_NAME)
}

/// Entries in the bins without a known place, with the names they had.
type Unknown = Vec<(PathBuf, String)>;

/// Put Back's two halves: rows whose place is known (entry, where it goes), and the others
/// (entry, the name they had), for which a folder is asked.
pub fn put_back_plan(rows: Vec<(PathBuf, TrashLabel)>) -> (Vec<(PathBuf, PathBuf)>, Unknown) {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for (trashed, label) in rows {
        match label.original {
            Some(original) => known.push((trashed, original)),
            None => unknown.push((trashed, label.name.into())),
        }
    }
    (known, unknown)
}

/// The rows without a known place, put into `folder` under the names they had. A name that is
/// not one plain part gives the entry's own name: never a step out of `folder`.
pub fn into_folder(folder: &Path, unknown: Unknown) -> Vec<(PathBuf, PathBuf)> {
    unknown
        .into_iter()
        .map(|(trashed, name)| {
            let mut parts = Path::new(&name).components();
            let plain = matches!((parts.next(), parts.next()), (Some(std::path::Component::Normal(_)), None));
            let name =
                if plain { std::ffi::OsString::from(name) } else { trashed.file_name().unwrap_or_default().to_owned() };
            (trashed, folder.join(name))
        })
        .collect()
}

/// Put Back on the selection (spec 7.1). Known places go back at once; for the others a folder
/// is asked (never a made-up place). A name taken there goes to the conflict list.
pub fn put_back(view: &View) {
    if !view.shows_trash() {
        return view.note(format!("Put Back works in the {}", gezik_core::nav::TRASH_NAME));
    }
    let (known, unknown) = put_back_plan(view.selected_trash());
    crate::operations::with_current(|ops| {
        if !known.is_empty() {
            ops.restore_from_trash(known);
        }
        if unknown.is_empty() {
            return;
        }
        let (what, them) = if unknown.len() == 1 {
            (format!("\"{}\"", unknown[0].1), "it")
        } else {
            (format!("{} items", group_digits(unknown.len())), "them")
        };
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(|h| PathBuf::from(h).display().to_string())
            .unwrap_or_default();
        ops.dialogs().ask_text(
            "Put Back",
            format!("Where {what} came from is not known. Put {them} into this folder:"),
            home,
            &["Put Back", "Cancel"],
            move |typed| {
                let Some(typed) = typed else { return };
                let folder = PathBuf::from(typed.trim());
                // shortcut: one stat on the UI thread for a typed folder (a dead network path
                // blocks); check it off the thread if that bites.
                if !folder.is_absolute() || !folder.is_dir() {
                    crate::panes::with_active(|p| p.view.note(format!("{} is not a folder", folder.display())));
                    return;
                }
                crate::operations::with_current(|ops| ops.restore_from_trash(into_folder(&folder, unknown)));
            },
        );
    });
}

/// Delete / Shift+Delete / Delete Permanently in the trash: asks, then deletes for good.
pub fn delete_selection(view: &View) {
    let rows = view.selected_trash();
    if rows.is_empty() {
        return;
    }
    let what =
        if rows.len() == 1 { format!("\"{}\"", rows[0].1.name) } else { format!("{} items", group_digits(rows.len())) };
    let items: Vec<(PathBuf, Option<PathBuf>)> =
        rows.into_iter().map(|(trashed, label)| (trashed, label.info)).collect();
    crate::operations::with_current(|ops| {
        ops.dialogs().ask(
            format!("Delete {what} permanently?"),
            "This cannot be undone.",
            &["Delete", "Cancel"],
            |choice| {
                if choice == Some(0) {
                    crate::operations::with_current(|ops| ops.delete_from_trash(items));
                }
            },
        );
    });
}

/// Empty Trash (spec 7.1): reads every bin afresh (off the UI thread), asks with the count and
/// size, then deletes what that read found.
pub fn empty() {
    std::thread::spawn(|| {
        let list = gezik_platform::trash::list();
        let _ = slint::invoke_from_event_loop(move || confirm_empty(list.items));
    });
}

fn confirm_empty(items: Vec<TrashItem>) {
    let bin = gezik_core::nav::TRASH_NAME;
    // What a delete is still working on under a hidden name is not the user's (as in the list).
    let items: Vec<TrashItem> = items
        .into_iter()
        .filter(|i| !i.trashed.file_name().and_then(|n| n.to_str()).is_some_and(gezik_ops::pending::is_internal_name))
        .collect();
    if items.is_empty() {
        crate::panes::with_active(|p| p.view.note(format!("The {bin} is empty")));
        return;
    }
    // Sizes are known on Windows, and elsewhere when no folder is in it (decision 5).
    let size = (cfg!(windows) || items.iter().all(|i| !i.is_dir)).then(|| items.iter().map(|i| i.size).sum());
    let question = empty_question(items.len(), size);
    let items: Vec<(PathBuf, Option<PathBuf>)> = items.into_iter().map(|i| (i.trashed, i.info)).collect();
    crate::operations::with_current(|ops| {
        ops.dialogs().ask(format!("Empty the {bin}?"), question, &["Empty", "Cancel"], |choice| {
            if choice == Some(0) {
                crate::operations::with_current(|ops| ops.delete_from_trash(items));
            }
        });
    });
}

/// `Permanently delete 1,234 items (3.2 GB)? This cannot be undone.`
pub fn empty_question(count: usize, size: Option<u64>) -> String {
    let items = if count == 1 { "1 item".to_owned() } else { format!("{} items", group_digits(count)) };
    let size = size.map(|s| format!(" ({})", crate::view_options::size_text(s))).unwrap_or_default();
    format!("Permanently delete {items}{size}? This cannot be undone.")
}

/// `1234` → `1,234`.
fn group_digits(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn item(bin: &str, entry: &str, name: &str, original: Option<&str>) -> TrashItem {
        TrashItem {
            trashed: PathBuf::from(bin).join(entry),
            info: Some(PathBuf::from(bin).join(format!("{entry}.info"))),
            name: name.into(),
            original: original.map(PathBuf::from),
            deleted: Some(SystemTime::UNIX_EPOCH),
            is_dir: false,
            size: 3,
        }
    }

    #[test]
    fn rows_are_the_entries_in_their_bins() {
        let set = to_set(vec![
            item("/bin1", "$R1.txt", "a.txt", Some("/w/a.txt")),
            item("/bin2", "$R2", "Old", None),
            item("/bin1", "$R3.txt", "b.txt", Some("/w/sub/b.txt")),
        ]);
        assert!(set.is_trash());
        assert_eq!(set.len(), 3);
        assert_eq!(set.path_at(0), Some(PathBuf::from("/bin1").join("$R1.txt")));
        assert_eq!(set.path_at(2), Some(PathBuf::from("/bin1").join("$R3.txt")), "one folder, shared");
        assert_eq!((set.shown_name(0), set.shown_folder(0)), (Some("a.txt"), Some(Path::new("/w").to_str().unwrap())));
        assert_eq!(set.shown_folder(1), Some(""), "unknown place: blank");
        assert_eq!(set.label(1).unwrap().original, None, "unknown stays unknown");
        assert_eq!(set.entry(0).unwrap().modified, Some(SystemTime::UNIX_EPOCH), "Date deleted");
        assert_eq!(set.label(1).unwrap().info, Some(PathBuf::from("/bin2").join("$R2.info")));
    }

    #[test]
    fn a_binned_folder_shows_its_recorded_size_on_windows() {
        let set = to_set(vec![TrashItem { is_dir: true, size: 4, ..item("/bin", "$R1", "f", None) }]);
        let expected = if cfg!(windows) { Some(4) } else { None };
        assert_eq!(set.entry(0).unwrap().known_size(), expected, "the $I record holds the folder's total");
    }

    #[test]
    fn gezik_s_own_half_deleted_names_are_not_rows() {
        let hidden = gezik_ops::pending::hidden_name();
        let set = to_set(vec![item("/bin", &hidden, "x", None)]);
        assert_eq!(set.len(), 0);
    }

    fn label(name: &str, original: Option<&str>) -> TrashLabel {
        TrashLabel { name: name.into(), folder: "".into(), original: original.map(PathBuf::from), info: None }
    }

    #[test]
    fn put_back_splits_known_and_unknown_places() {
        let rows = vec![
            (PathBuf::from("/bin/$R1"), label("a.txt", Some("/w/a.txt"))),
            (PathBuf::from("/bin/$R2"), label("b.txt", None)),
            (PathBuf::from("/bin/$R3"), label("a.txt", Some("/w/a.txt"))),
        ];
        let (known, unknown) = put_back_plan(rows);
        assert_eq!(
            known,
            [
                (PathBuf::from("/bin/$R1"), PathBuf::from("/w/a.txt")),
                (PathBuf::from("/bin/$R3"), PathBuf::from("/w/a.txt")),
            ],
            "the same place twice: the job's conflict list decides the second"
        );
        assert_eq!(unknown, [(PathBuf::from("/bin/$R2"), "b.txt".to_owned())]);
        assert_eq!(
            into_folder(Path::new("/x"), unknown),
            [(PathBuf::from("/bin/$R2"), PathBuf::from("/x").join("b.txt"))]
        );
    }

    #[test]
    fn a_name_that_climbs_falls_back_to_the_entry_s_own() {
        for bad in ["..", "../up", "a/b", "", "."] {
            assert_eq!(
                into_folder(Path::new("/x"), vec![(PathBuf::from("/bin/$R9"), bad.to_owned())]),
                [(PathBuf::from("/bin/$R9"), PathBuf::from("/x").join("$R9"))],
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_empty_question_says_it_cannot_be_undone() {
        assert_eq!(
            empty_question(1234, Some(3_435_973_837)),
            "Permanently delete 1,234 items (3.2 GB)? This cannot be undone."
        );
        assert_eq!(empty_question(1, None), "Permanently delete 1 item? This cannot be undone.");
    }

    #[test]
    fn only_safe_actions_run_in_the_trash() {
        for blocked in [
            Action::Rename,
            Action::NewFolder,
            Action::Copy,
            Action::Cut,
            Action::Paste,
            Action::PasteMove,
            Action::Duplicate,
            Action::BatchRename,
            Action::CopyPath,
            Action::OpenTerminal,
            Action::OpenTerminalAdmin,
            Action::AddToStack,
            Action::Search,
            Action::FlatView,
            Action::ShowInFolder,
            Action::CopyWithFolders,
            Action::CutWithFolders,
            Action::NewFolderWithSelection,
            Action::CalculateFolderSizes,
            Action::SaveSearch,
        ] {
            assert!(!allowed(blocked), "{blocked:?}");
        }
        for fine in [
            Action::Undo,
            Action::Redo,
            Action::Refresh,
            Action::QuickLook,
            Action::TogglePreview,
            Action::SelectAll,
            Action::Filter,
            Action::Back,
            Action::Up,
            Action::NewTab,
            Action::PutBack,
            Action::EmptyTrash,
            Action::Trash,
            Action::DeletePermanently,
        ] {
            assert!(allowed(fine), "{fine:?}");
        }
    }

    #[test]
    fn delete_in_the_trash_is_for_good() {
        for action in Action::ALL {
            assert_eq!(
                in_trash(action),
                match action {
                    Action::Trash | Action::DeletePermanently => InTrash::DeleteForGood,
                    a if allowed(a) => InTrash::Run,
                    _ => InTrash::Refuse,
                },
                "{action:?}"
            );
        }
    }

    #[test]
    fn denied_is_said_once() {
        let asked = Cell::new(false);
        assert!(first_denial(&asked));
        assert!(!first_denial(&asked));
    }
}
