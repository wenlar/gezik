//! The Sync and Backup page's rows (spec 14 §7): Export Settings (a zip's path and Enter, two
//! boxes for what goes in), Import Settings (a zip's path and Enter: what it would change, the
//! settings in it that run programs in a red row of their own, a box for each kind of file,
//! then Import or Cancel), Restore Backup (the last 10) and Undo Import. The zip is read and
//! written on a thread (`crate::settings_backup`); these rows only show its outcome.

use std::cell::RefCell;

use gezik_config::bundle::{self, Group};
use slint::{ModelRc, SharedString, VecModel};

use crate::SettingItem;
use crate::settings_backup::{Change, Incoming, Preview, Undo};

pub(super) const IMPORT: &str = "Import";
pub(super) const UNDO: &str = "Undo";
pub(super) const OPEN_DATA: &str = "Open Data Folder";

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
                            "A gezik-portable file next to Gezik keeps everything in this folder.{read_only} Delete                              that file to use this computer's settings folder again (the data stays); Export and                              Import move settings between the two."
                        ),
                        vec![OPEN_DATA],
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
                    "The New ▸ templates (at most 256 MB)".to_owned(),
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
        }
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
}
