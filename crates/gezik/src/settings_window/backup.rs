//! The Sync and Backup page's rows (spec 14 §7): Export Settings (a zip's path and Enter, two
//! boxes for what goes in), Import Settings (a zip's path and Enter: what it would change, the
//! settings in it that run programs in a red row of their own, a box for each kind of file,
//! then Import or Cancel), Restore Backup (the last 10) and Undo Import. Before them the sync
//! folder's (spec 14 §8): Keep Settings in a Folder (a path and Enter; Use or Replace when it
//! has settings already), or the folder in use, its conflicting copies and their Review (a row
//! per key that differs, This copy or Other copy, then Merge or Cancel). The files are read and
//! written on a thread (`crate::settings_backup`); these rows only show its outcome.

use std::cell::RefCell;

use std::path::PathBuf;

use gezik_config::bundle::{self, Group};
use gezik_config::sync_folder::Difference;
use slint::{ModelRc, SharedString, VecModel};

use crate::SettingItem;
use crate::settings_backup::{Change, Incoming, Preview, Undo};

pub(super) const IMPORT: &str = "Import";
pub(super) const UNDO: &str = "Undo";
pub(super) const OPEN_DATA: &str = "Open Data Folder";
pub(super) const OPEN_SYNC: &str = "Open Settings Folder";
pub(super) const CHECK_AGAIN: &str = "Check Again";
pub(super) const STOP: &str = "Stop Keeping Settings There";
pub(super) const USE: &str = "Use the Settings in This Folder";
pub(super) const REPLACE: &str = "Replace Them with This Computer's";
pub(super) const REVIEW: &str = "Review…";
pub(super) const THIS_COPY: &str = "This copy";
pub(super) const OTHER_COPY: &str = "Other copy";
pub(super) const MERGE: &str = "Merge";

thread_local! {
    /// The last import of this run, for Undo Import (it outlives the window).
    static LAST: RefCell<Option<Undo>> = const { RefCell::new(None) };
}

pub(super) fn last() -> Option<Undo> {
    LAST.with(|l| l.borrow().clone())
}

pub(super) fn set_last(undo: Option<Undo>) {
    LAST.with(|l| *l.borrow_mut() = undo);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Row {
    /// Portable mode's data folder (spec 14 §9.1).
    Portable,
    /// Keep Settings in a Folder, or the sync folder in use (spec 14 §8).
    Sync,
    /// The folder chosen has settings already: Use or Replace.
    Existing,
    /// A conflicting copy in the sync folder.
    Conflict(usize),
    /// A key the copies under review differ in.
    Difference(usize),
    Merge,
    Export,
    Templates,
    Views,
    Import,
    Group(Group),
    Programs,
    Confirm,
    Restore,
    Undo,
}

#[derive(Default)]
pub(super) struct Backup {
    /// Export Settings' zip, as typed.
    pub export_to: String,
    pub without_templates: bool,
    pub with_views: bool,
    /// Import Settings' zip, as typed, and what it would do.
    pub import_from: String,
    pub preview: Option<Preview>,
    /// The kinds of file unticked for the import.
    pub skipped: Vec<Group>,
    /// The backups, newest first (`None`: not looked at yet).
    pub backups: Option<Vec<String>>,
    pub listing: bool,
    /// A zip is being read or written.
    pub busy: bool,
    /// Keep Settings in a Folder's folder, as typed.
    pub sync_to: String,
    /// The sync folder in use (`None`: none) and whether it could not be read.
    pub synced: Option<PathBuf>,
    pub offline: bool,
    /// Settings changes waiting for the sync folder.
    pub unsaved: usize,
    /// A cloud folder to suggest (OneDrive's, Dropbox's…).
    pub suggest: Option<PathBuf>,
    /// The folder chosen that has settings already, while Use or Replace is asked.
    pub found: Option<PathBuf>,
    /// The sync folder's conflicting copies (`settings (1).toml`, `themes/nord (1).toml`).
    pub conflicts: Vec<String>,
    pub review: Option<Review>,
}

/// A conflicting copy under review (spec 14 §8.3): the keys it differs in, and those taken from it.
#[derive(Debug, Clone)]
pub struct Review {
    /// The copy and the file it is of, as named in the sync folder (`themes/nord (1).toml`).
    pub copy: String,
    pub original: String,
    /// The copy's text.
    pub other: String,
    pub differences: Vec<Difference>,
    /// Per difference: Other copy chosen.
    pub take: Vec<bool>,
}

impl Backup {
    /// The kinds of file Export Settings writes.
    pub fn export_groups(&self) -> Vec<Group> {
        Group::ALL
            .into_iter()
            .filter(|g| match g {
                Group::Templates => !self.without_templates,
                Group::Views => self.with_views,
                _ => true,
            })
            .collect()
    }

    /// The rows a search for `query` shows (all without one).
    pub fn rows(&self, query: &str) -> Vec<Row> {
        let portable = gezik_config::paths::location().dirs.as_ref().is_some_and(|d| d.portable);
        let mut all = if portable { vec![Row::Portable] } else { Vec::new() };
        all.push(Row::Sync);
        if self.found.is_some() {
            all.push(Row::Existing);
        }
        if self.synced.is_some() {
            all.extend((0..self.conflicts.len()).map(Row::Conflict));
        }
        if let Some(review) = &self.review {
            all.extend((0..review.differences.len()).map(Row::Difference));
            all.push(Row::Merge);
        }
        all.extend([Row::Export, Row::Templates, Row::Views, Row::Import]);
        if let Some(preview) = &self.preview {
            all.extend(Group::ALL.into_iter().filter(|g| preview.files.iter().any(|f| f.group == *g)).map(Row::Group));
            if !preview.programs.is_empty() {
                all.push(Row::Programs);
            }
            all.push(Row::Confirm);
        }
        all.push(Row::Restore);
        if last().is_some() {
            all.push(Row::Undo);
        }
        let query = query.to_lowercase();
        all.into_iter()
            .filter(|row| {
                let item = self.item(*row);
                [&item.title, &item.key, &item.description].iter().any(|t| t.to_lowercase().contains(&query))
            })
            .collect()
    }

    /// The files the import writes: those ticked that are not the same here.
    pub fn chosen(&mut self) -> Vec<Incoming> {
        let Some(preview) = self.preview.take() else { return Vec::new() };
        preview.files.into_iter().filter(|f| f.change != Change::Same && !self.skipped.contains(&f.group)).collect()
    }

    pub fn item(&self, row: Row) -> SettingItem {
        let check = |on: bool| if on { "true" } else { "false" }.to_owned();
        let (kind, title, key, value, description, options): (i32, String, String, String, String, Vec<&str>) =
            match row {
                Row::Portable => {
                    let location = gezik_config::paths::location();
                    let data = location.dirs.as_ref().map(|d| d.local.display().to_string()).unwrap_or_default();
                    let read_only = if location.read_only { " It is read-only: changes are not saved." } else { "" };
                    (
                        1,
                        format!("Portable: settings are kept in {data}"),
                        "gezik-portable".to_owned(),
                        "On".to_owned(),
                        format!(
                            "A gezik-portable file next to Gezik keeps everything in this folder.{read_only} Delete \
                             that file to use this computer's settings folder again (the data stays); Export and \
                             Import move settings between the two."
                        ),
                        vec![OPEN_DATA],
                    )
                }
                Row::Sync => self.sync_item(),
                Row::Existing => (
                    1,
                    "This folder has Gezik settings already".to_owned(),
                    self.found.as_ref().map(|f| f.display().to_string()).unwrap_or_default(),
                    "Choose".to_owned(),
                    "Use them: this computer's settings are backed up to backups/ and the folder's are used. Replace \
                     them: the folder's are backed up to backups/, then this computer's are written there. Nothing \
                     is merged."
                        .to_owned(),
                    vec![USE, REPLACE, "Cancel"],
                ),
                Row::Conflict(i) => {
                    let name = self.conflicts.get(i).cloned().unwrap_or_default();
                    let of = if name.starts_with("themes/") { "a theme" } else { "settings.toml" };
                    (
                        1,
                        format!("Conflicting copy: {name}"),
                        name,
                        "Not reviewed".to_owned(),
                        format!(
                            "A sync service made it when two computers changed {of} at once. Review lists each key \
                             the two copies differ in and you choose which to keep; nothing is merged on its own."
                        ),
                        vec![REVIEW],
                    )
                }
                Row::Difference(i) => {
                    let review = self.review.as_ref();
                    let difference = review.and_then(|r| r.differences.get(i));
                    let take = review.and_then(|r| r.take.get(i)).copied().unwrap_or_default();
                    let not_set = || "(not set)".to_owned();
                    let (this, other) = difference
                        .map(|d| (d.this.clone().unwrap_or_else(not_set), d.other.clone().unwrap_or_else(not_set)))
                        .unwrap_or_default();
                    (
                        1,
                        difference.map(Difference::key).unwrap_or_default(),
                        review.map(|r| r.original.clone()).unwrap_or_default(),
                        if take { OTHER_COPY } else { THIS_COPY }.to_owned(),
                        format!("This copy: {this}  —  other copy: {other}"),
                        vec![THIS_COPY, OTHER_COPY],
                    )
                }
                Row::Merge => {
                    let review = self.review.as_ref();
                    let taken = review.map_or(0, |r| r.take.iter().filter(|t| **t).count());
                    let (copy, original) = review.map(|r| (r.copy.clone(), r.original.clone())).unwrap_or_default();
                    (
                        1,
                        format!("Merge {copy}"),
                        original.clone(),
                        match taken {
                            1 => "1 key from the other copy".to_owned(),
                            n => format!("{n} keys from the other copy"),
                        },
                        format!(
                            "Merge writes the keys set to Other copy above into {original} (the rest stays as it \
                             is) and moves the conflicting copy to the trash. Cancel leaves both as they are."
                        ),
                        vec![MERGE, "Cancel"],
                    )
                }
                Row::Export => (
                    2,
                    "Export Settings".to_owned(),
                    "export-settings".to_owned(),
                    self.export_to.clone(),
                    "Writes settings.toml and the themes (and what is ticked below) to this zip: change the path if \
                     you like and press Enter. state.toml, the tag store, the system changes, the tools and the \
                     backups stay on this computer."
                        .to_owned(),
                    Vec::new(),
                ),
                Row::Templates => (
                    0,
                    "Export Templates".to_owned(),
                    "export-settings.templates".to_owned(),
                    check(!self.without_templates),
                    "The New > templates (at most 256 MB)".to_owned(),
                    Vec::new(),
                ),
                Row::Views => (
                    0,
                    "Export Folder Views".to_owned(),
                    "export-settings.views".to_owned(),
                    check(self.with_views),
                    "views.toml: the view each folder was left in (their paths are this computer's)".to_owned(),
                    Vec::new(),
                ),
                Row::Import => (
                    2,
                    "Import Settings".to_owned(),
                    "import-settings".to_owned(),
                    self.import_from.clone(),
                    "Type or paste the path of a zip made by Export Settings and press Enter: the rows below show \
                     what it would change, and nothing is written until you choose Import. Each file replaces the \
                     one here (nothing is merged), after a backup."
                        .to_owned(),
                    Vec::new(),
                ),
                Row::Group(group) => self.group_item(group),
                Row::Programs => {
                    let programs = self.preview.as_ref().map(|p| p.programs.as_slice()).unwrap_or_default();
                    (
                        7,
                        "These run programs on this computer".to_owned(),
                        "import-settings.programs".to_owned(),
                        format!("{} in settings.toml", programs.len()),
                        format!("{}. Import only files you trust: nothing runs while importing.", programs.join("; ")),
                        Vec::new(),
                    )
                }
                Row::Confirm => self.confirm_item(),
                Row::Restore => {
                    let backups = self.backups.as_deref().unwrap_or_default();
                    let value = match (&self.backups, backups.len()) {
                        (None, _) => "Looking…".to_owned(),
                        (_, 0) => "No backups yet".to_owned(),
                        (_, 1) => "1 backup".to_owned(),
                        (_, n) => format!("{n} backups"),
                    };
                    (
                        if backups.is_empty() { 6 } else { 1 },
                        "Restore Backup".to_owned(),
                        "backups/".to_owned(),
                        value,
                        "Before each import Gezik backs up the files it replaces here (the last 10 are kept): choose \
                         one to see what it would put back, then Import."
                            .to_owned(),
                        backups.iter().map(String::as_str).collect(),
                    )
                }
                Row::Undo => {
                    let backup = last().and_then(|u| u.backup.file_name().map(|n| n.to_string_lossy().into_owned()));
                    (
                        1,
                        "Undo Import".to_owned(),
                        "import-settings.undo".to_owned(),
                        "Imported".to_owned(),
                        format!(
                            "Puts back the files the last import replaced, byte for byte, from backups/{}, and \
                             removes those it added",
                            backup.unwrap_or_default()
                        ),
                        vec![UNDO],
                    )
                }
            };
        let options: Vec<SharedString> = options.into_iter().map(SharedString::from).collect();
        SettingItem {
            kind,
            title: title.into(),
            key: key.into(),
            description: description.into(),
            value: value.into(),
            is_default: false,
            default: SharedString::new(),
            options: ModelRc::new(VecModel::from(options)),
            place: 0,
        }
    }

    /// Keep Settings in a Folder (a path and Enter), or the sync folder in use.
    fn sync_item(&self) -> (i32, String, String, String, String, Vec<&'static str>) {
        let key = gezik_config::sync_folder::LOCATION_FILE.to_owned();
        let Some(folder) = &self.synced else {
            let suggest = match &self.suggest {
                Some(cloud) => format!(" (say, {})", cloud.join("Gezik").display()),
                None => String::new(),
            };
            return (
                2,
                "Keep Settings in a Folder".to_owned(),
                key,
                self.sync_to.clone(),
                format!(
                    "Type a folder in OneDrive, Dropbox, iCloud Drive or Syncthing{suggest} and press Enter: \
                     settings.toml, the themes and the templates are kept there for every computer that uses it. \
                     State, folder views, tools, backups and the tag store stay on this computer, and so do \
                     [system] and what you mark Only on This Computer (right-click a setting)."
                ),
                Vec::new(),
            );
        };
        let waiting = match self.unsaved {
            0 => String::new(),
            1 => " 1 change waits for it.".to_owned(),
            n => format!(" {n} changes wait for it."),
        };
        let (value, description) = if self.offline {
            (
                "Not available",
                format!(
                    "The folder cannot be read (a drive not plugged in, the sync app not running): the last copy \
                     is used and the settings are read-only.{waiting} Check Again looks once more."
                ),
            )
        } else {
            (
                "On",
                "Shared by the computers that use this folder; this computer's own values are in \
                 settings.local.toml. Stop copies the folder's settings back here (after a backup) and leaves the \
                 folder as it is."
                    .to_owned(),
            )
        };
        let mut options = vec![OPEN_SYNC, STOP];
        if self.offline {
            options.insert(1, CHECK_AGAIN);
        }
        (1, format!("Settings are kept in {}", folder.display()), key, value.to_owned(), description, options)
    }

    /// A kind of file in the zip: a box, or "The same" when every file of it is.
    fn group_item(&self, group: Group) -> (i32, String, String, String, String, Vec<&'static str>) {
        let preview = self.preview.as_ref();
        let files: Vec<&Incoming> = preview.iter().flat_map(|p| &p.files).filter(|f| f.group == group).collect();
        let same = files.iter().filter(|f| f.change == Change::Same).count();
        let changing: Vec<String> = files
            .iter()
            .filter(|f| f.change != Change::Same)
            .map(|f| {
                let name = f.name.split_once('/').map_or(f.name.as_str(), |(_, rest)| rest);
                format!("{name} {}", if f.change == Change::New { "new" } else { "replaces" })
            })
            .collect();
        let mut description = match group {
            Group::Settings => preview.map(|p| bundle::summary(&p.settings)).unwrap_or_default(),
            _ if changing.len() > 8 => format!("{} and {} more", changing[..8].join(", "), changing.len() - 8),
            _ => changing.join(", "),
        };
        if same > 0 && group != Group::Settings {
            description.push_str(&format!("{}{same} the same here", if changing.is_empty() { "" } else { "; " }));
        }
        let title = format!("Import {}", group.title());
        let key = format!("import-settings.{}", group.title().to_lowercase().replace([' ', '.'], "-"));
        match changing.is_empty() {
            true => (6, title, key, "The same".to_owned(), description, Vec::new()),
            false => (0, title, key, (!self.skipped.contains(&group)).to_string(), description, Vec::new()),
        }
    }

    fn confirm_item(&self) -> (i32, String, String, String, String, Vec<&'static str>) {
        let Some(preview) = &self.preview else {
            return (6, String::new(), String::new(), String::new(), String::new(), Vec::new());
        };
        let name = preview.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let count =
            preview.files.iter().filter(|f| f.change != Change::Same && !self.skipped.contains(&f.group)).count();
        let mut description =
            "The files it replaces are backed up to backups/ first; Undo Import puts them back.".to_owned();
        if !preview.left_out.is_empty() {
            let shown: Vec<&str> = preview.left_out.iter().take(5).map(String::as_str).collect();
            description.push_str(&format!(
                " {} entries that may not come in are left out: {}.",
                preview.left_out.len(),
                shown.join(", ")
            ));
        }
        if !preview.os.is_empty() && preview.os != std::env::consts::OS {
            description.push_str(&format!(
                " Exported on {}: paths written without a {{token}} may not be on this computer.",
                preview.os
            ));
        }
        let key = preview.source.display().to_string();
        match count {
            0 => (6, format!("Import from {name}"), key, "Nothing to import".to_owned(), description, Vec::new()),
            n => (
                1,
                format!("Import from {name}"),
                key,
                format!("{n} file{} to write", if n == 1 { "" } else { "s" }),
                description,
                vec![IMPORT, "Cancel"],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use slint::Model;

    fn incoming(name: &str, change: Change) -> Incoming {
        Incoming { name: name.to_owned(), group: bundle::group_of(name).unwrap(), change, data: Vec::new() }
    }

    fn previewed() -> Backup {
        Backup {
            preview: Some(Preview {
                source: PathBuf::from("/x/gezik-settings.zip"),
                os: "plan9".to_owned(),
                files: vec![
                    incoming("settings.toml", Change::Replaces),
                    incoming("themes/nord.toml", Change::New),
                    incoming("themes/dark.toml", Change::Same),
                    incoming("templates/a.txt", Change::Same),
                ],
                left_out: vec!["../evil".to_owned()],
                settings: vec!["theme: dark → nord".to_owned()],
                programs: vec!["Command \"Wipe\": cmd /c del {in}".to_owned()],
            }),
            backups: Some(vec!["gezik-settings-2.zip".to_owned()]),
            ..Backup::default()
        }
    }

    #[test]
    fn a_preview_shows_its_files_its_programs_and_a_choice() {
        let mut backup = previewed();
        let rows = backup.rows("");
        assert_eq!(
            rows,
            [
                Row::Sync,
                Row::Export,
                Row::Templates,
                Row::Views,
                Row::Import,
                Row::Group(Group::Settings),
                Row::Group(Group::Themes),
                Row::Group(Group::Templates),
                Row::Programs,
                Row::Confirm,
                Row::Restore,
            ]
        );
        let settings = backup.item(Row::Group(Group::Settings));
        assert_eq!((settings.kind, settings.value.as_str()), (0, "true"));
        assert_eq!(settings.description, "1 setting changes: theme: dark → nord");
        assert_eq!(backup.item(Row::Group(Group::Themes)).description, "nord.toml new; 1 the same here");
        assert_eq!(backup.item(Row::Group(Group::Templates)).kind, 6, "all the same: nothing to tick");
        let programs = backup.item(Row::Programs);
        assert_eq!(programs.kind, 7, "the red row");
        assert!(programs.description.starts_with("Command \"Wipe\": cmd /c del {in}. Import only files you trust"));
        let confirm = backup.item(Row::Confirm);
        assert_eq!((confirm.kind, confirm.value.as_str()), (1, "2 files to write"));
        assert!(confirm.description.contains("1 entries that may not come in are left out: ../evil."));
        assert!(confirm.description.contains("Exported on plan9"));

        backup.skipped.push(Group::Themes);
        assert_eq!(backup.item(Row::Confirm).value, "1 file to write");
        let chosen: Vec<String> = backup.chosen().into_iter().map(|f| f.name).collect();
        assert_eq!(chosen, ["settings.toml"], "unticked and the same are not written");
        assert!(backup.preview.is_none());
        assert_eq!(backup.rows("restore"), [Row::Restore]);
    }

    #[test]
    fn the_export_boxes_choose_its_files() {
        let mut backup = Backup::default();
        assert_eq!(backup.export_groups(), [Group::Settings, Group::Themes, Group::Templates]);
        (backup.without_templates, backup.with_views) = (true, true);
        assert_eq!(backup.export_groups(), [Group::Settings, Group::Themes, Group::Views]);
        assert_eq!(backup.item(Row::Templates).value, "false");
        assert_eq!(backup.item(Row::Restore).value, "Looking…");
    }

    #[test]
    fn the_sync_rows_follow_the_folder_its_copies_and_a_review() {
        let mut backup = Backup { suggest: Some(PathBuf::from("/u/Dropbox")), ..Backup::default() };
        assert_eq!(backup.rows("")[0], Row::Sync);
        let sync = backup.item(Row::Sync);
        assert_eq!((sync.kind, sync.title.as_str()), (2, "Keep Settings in a Folder"));
        assert!(sync.description.contains(&PathBuf::from("/u/Dropbox").join("Gezik").display().to_string()));
        backup.found = Some(PathBuf::from("/u/Dropbox/Gezik"));
        assert_eq!(backup.rows("")[..2], [Row::Sync, Row::Existing]);
        let options: Vec<String> = backup.item(Row::Existing).options.iter().map(|o| o.to_string()).collect();
        assert_eq!(options, [USE, REPLACE, "Cancel"]);

        backup.found = None;
        backup.synced = Some(PathBuf::from("/u/Dropbox/Gezik"));
        backup.conflicts = vec!["settings (1).toml".to_owned()];
        let sync = backup.item(Row::Sync);
        assert_eq!((sync.kind, sync.value.as_str()), (1, "On"));
        assert!(!sync.options.iter().any(|o| o == CHECK_AGAIN));
        (backup.offline, backup.unsaved) = (true, 2);
        let sync = backup.item(Row::Sync);
        assert_eq!(sync.value, "Not available");
        assert!(sync.options.iter().any(|o| o == CHECK_AGAIN) && sync.description.contains("2 changes wait"));
        assert_eq!(backup.rows("")[..2], [Row::Sync, Row::Conflict(0)]);
        assert_eq!(backup.item(Row::Conflict(0)).title, "Conflicting copy: settings (1).toml");

        let differences = gezik_config::sync_folder::differences(
            "theme = \"nord\"
",
            "theme = \"dark\"
",
        )
        .unwrap();
        backup.review = Some(Review {
            copy: "settings (1).toml".to_owned(),
            original: "settings.toml".to_owned(),
            other: String::new(),
            take: vec![false; differences.len()],
            differences,
        });
        assert_eq!(backup.rows("")[..4], [Row::Sync, Row::Conflict(0), Row::Difference(0), Row::Merge]);
        let key = backup.item(Row::Difference(0));
        assert_eq!((key.title.as_str(), key.value.as_str()), ("theme", THIS_COPY));
        assert_eq!(key.description, "This copy: nord  —  other copy: dark");
        assert_eq!(backup.item(Row::Merge).value, "0 keys from the other copy");
        backup.review.as_mut().unwrap().take[0] = true;
        assert_eq!(backup.item(Row::Difference(0)).value, OTHER_COPY);
        assert_eq!(backup.item(Row::Merge).value, "1 key from the other copy");
    }
}
