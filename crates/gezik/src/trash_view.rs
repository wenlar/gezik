//! The trash view (spec 7.1, part 9b2): every bin of this user in one list. Its rows are the
//! entries in the bins, shown by the names and places they had (`TrashLabel`). Read only when
//! shown; watched only while shown; nothing kept after.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gezik_core::Entry;
use gezik_platform::trash::TrashItem;
use gezik_search::results::{Batch, ResultSet, TrashLabel};

use crate::folder_watch::FolderWatch;

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
            flags: 0,
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
            crate::navigation::with_current(crate::navigation::Navigator::refresh_trash);
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
    fn gezik_s_own_half_deleted_names_are_not_rows() {
        let hidden = gezik_ops::pending::hidden_name();
        let set = to_set(vec![item("/bin", &hidden, "x", None)]);
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn denied_is_said_once() {
        let asked = Cell::new(false);
        assert!(first_denial(&asked));
        assert!(!first_denial(&asked));
    }
}
