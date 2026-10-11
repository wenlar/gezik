//! Export List… (spec 11 §11): the list the active pane shows, as text, CSV or HTML, saved as a
//! new file (an engine job: the conflict list, undo, the file selected) or put on the
//! clipboard. Nothing is kept between two exports but the last choices in state.toml.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use gezik_config::settings::ExportState;
use gezik_core::export::{self, Column, Format, Sheet};
use gezik_core::nav::Location;
use gezik_core::view::ColumnKey;
use gezik_platform::clipboard::{self, ClipboardError};

use crate::navigation::Navigator;
use crate::operations::{After, Operations};
use crate::preview::with_commas;
use crate::view::View;

/// What This PC says (the trash never gets here: `trash_view::allowed`).
const NOT_HERE: &str = "Export List works on a folder or search results";

/// The format a name without a known extension is told to end with.
const BAD_EXTENSION: &str = "End the name with .txt, .csv or .html";

/// Where an export goes by default and what heads it: the folder, the file's stem and the
/// title (spec 11 §11.1, plan deviation 13). `None` where there is no list of files.
fn place(location: &Location, home: &Path) -> Option<(PathBuf, String, String)> {
    match location {
        Location::Path(dir) | Location::Flat(dir) => {
            let stem = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned());
            Some((dir.clone(), export::file_stem(&stem), dir.display().to_string()))
        }
        Location::Search(spec) => {
            let stem = spec.name.clone().unwrap_or_else(|| "Search results".to_owned());
            let (folder, title) = match spec.scope.folder() {
                Some(folder) => (folder.to_path_buf(), format!("{} in {}", spec.title(), folder.display())),
                None => (home.to_path_buf(), spec.title()),
            };
            Some((folder, export::file_stem(&stem), title))
        }
        Location::Drives | Location::Trash => None,
    }
}

/// The file `typed` names: `~` and variables put in, a relative path under `base`. `None` for
/// an empty field or one naming no file.
fn typed_target(base: &Path, typed: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let path = PathBuf::from(crate::path_box::expand(typed));
    let path = if path.is_absolute() { path } else { base.join(path) };
    path.file_name().is_some().then_some(path)
}

/// The line under the field: the format and the folder, or what is wrong.
fn field_note(base: &Path, typed: &str) -> (String, bool) {
    match typed_target(base, typed) {
        None => ("Type a file name".to_owned(), true),
        Some(target) => match Format::from_path(&target) {
            None => (BAD_EXTENSION.to_owned(), true),
            Some(format) => {
                let folder = target.parent().map(|p| p.display().to_string()).unwrap_or_default();
                (format!("{} into {folder}", format.title()), false)
            }
        },
    }
}

/// How `format` is written; `file`: into a file (only a Windows CSV file starts with a BOM).
fn sheet(format: Format, columns: &[Column], title: String, count: usize, file: bool) -> Sheet {
    let now = std::time::SystemTime::now();
    let at = gezik_platform::format_date(now, gezik_core::view::DateFormat::Iso, now);
    let windows = cfg!(windows);
    Sheet {
        format,
        headers: columns.iter().map(|c| c.title().to_owned()).collect(),
        right: columns.iter().map(|&c| c == Column::Shown(ColumnKey::Size)).collect(),
        separator: if format == Format::Csv { gezik_platform::list_separator() } else { ',' },
        bom: file && windows && format == Format::Csv,
        eol: if windows && format != Format::Html { "\r\n" } else { "\n" },
        title,
        note: format!("{} · {at}", items_text(count)),
    }
}

/// The question's boxes: with a selection (a row's menu has one: the row) "Only the N items
/// selected", ticked, so Tools ▸ Export list… on rows exports them (spec 11 §11.1); then "Full
/// paths in text" as last time.
fn form_checks(selected: usize, full_paths: bool) -> Vec<(String, bool)> {
    let mut checks = Vec::new();
    if selected > 0 {
        checks.push((format!("Only the {} selected", items_text(selected)), true));
    }
    checks.push(("Full paths in text".to_owned(), full_paths));
    checks
}

fn items_text(count: usize) -> String {
    if count == 1 { "1 item".to_owned() } else { format!("{} items", with_commas(count)) }
}

/// Writes the view's rows as `sheet` says into `out`; returns how many.
fn write_list(
    view: &View,
    sheet: &Sheet,
    columns: &[Column],
    selected_only: bool,
    out: &mut dyn Write,
) -> io::Result<usize> {
    export::begin(out, sheet)?;
    let raw = sheet.format == Format::Csv;
    let count = view.export_rows(columns, raw, selected_only, &mut |cells| export::row(out, sheet, cells));
    let count = count.unwrap_or(Ok(0))?;
    export::end(out, sheet)?;
    out.flush()?;
    Ok(count)
}

/// `export-list`: asks where to and how, then saves or copies.
pub fn ask(nav: &Navigator, view: &View) {
    if view.shows_drives() || view.shows_trash() {
        return view.note(NOT_HERE.to_owned());
    }
    let Some((folder, stem, title)) = place(&nav.active_location(), &crate::path_box::home()) else {
        return view.note(NOT_HERE.to_owned());
    };
    crate::operations::with_current(|ops| {
        let last = ops.store().map(|store| store.load_state().export).unwrap_or_default();
        let format = last.format.as_deref().and_then(Format::parse).unwrap_or(Format::Csv);
        let (shown, selected) = (view.len(), view.selection_count());
        let checks = form_checks(selected, last.full_paths);
        let message = format!(
            "The {} shown, as sorted and filtered. The name's extension sets the format: .txt (one name or path per line), .csv or .html.",
            items_text(shown)
        );
        let field = folder.join(format!("{stem}.{}", format.extension())).display().to_string();
        let base = folder.clone();
        let (ops2, view) = (ops.clone(), view.clone());
        ops.dialogs().ask_form(
            "Export List",
            message,
            (field, checks),
            &["Save", "Copy", "Cancel"],
            move |typed| field_note(&folder, typed),
            move |answer| {
                let Some((button, typed, marks)) = answer else { return };
                let only_selected = selected > 0 && marks.first().copied().unwrap_or(false);
                let full_paths = marks.last().copied().unwrap_or(false);
                let Some(target) = typed_target(&base, &typed) else { return view.note("Type a file name".to_owned()) };
                let Some(format) = Format::from_path(&target) else { return view.note(BAD_EXTENSION.to_owned()) };
                if let Some(store) = ops2.store() {
                    let state = ExportState { format: Some(format.as_str().to_owned()), full_paths };
                    store.update_state(|saved| saved.export = state);
                }
                let columns = export::columns(format, &view.shown_column_keys(), full_paths);
                let count = if only_selected { selected } else { view.len() };
                let sheet = sheet(format, &columns, title, count, button == 0);
                if button == 0 {
                    save(&ops2, &view, &sheet, &columns, only_selected, &target);
                } else {
                    copy(&view, &sheet, &columns, only_selected);
                }
            },
        );
    });
}

/// The list written into a temporary file on this thread (64 KB at a time; the list is the
/// view's), then a job streams it to `target` (plan deviation 5).
fn save(ops: &Operations, view: &View, sheet: &Sheet, columns: &[Column], only_selected: bool, target: &Path) {
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else { return };
    let dir_of_temps = temp_dir();
    // Left by a Gezik that stopped mid-export: swept off the UI thread.
    let sweep_dir = dir_of_temps.clone();
    let _ = std::thread::Builder::new()
        .name("gezik-export-sweep".into())
        .spawn(move || sweep(&sweep_dir, std::time::SystemTime::now()));
    let written = TempFile::create_in(&dir_of_temps).and_then(|(temp, file)| {
        let mut out = io::BufWriter::with_capacity(64 * 1024, file);
        write_list(view, sheet, columns, only_selected, &mut out).map(|_| temp)
    });
    let temp = match written {
        Ok(temp) => temp,
        Err(err) => return view.note(format!("Cannot export the list: {}", gezik_platform::fs::describe(&err))),
    };
    let task = gezik_ops::NewTask::written(dir, &name.to_string_lossy(), move |out| {
        io::copy(&mut std::fs::File::open(&temp.0)?, out).map(|_| ())
    });
    ops.submit(Box::new(task), None, After::Select);
}

/// The list as text on the clipboard (HTML as its source; no BOM).
fn copy(view: &View, sheet: &Sheet, columns: &[Column], only_selected: bool) {
    let mut out = Vec::new();
    let Ok(count) = write_list(view, sheet, columns, only_selected, &mut out) else { return };
    view.note(match clipboard::write_text(&String::from_utf8_lossy(&out)) {
        Ok(()) => format!("Copied {} as {}", items_text(count), sheet.format.title()),
        Err(ClipboardError::Unsupported) => "No system clipboard here".to_owned(),
        Err(ClipboardError::Failed(why)) => format!("Cannot use the clipboard: {why}"),
    });
}

/// The temporary files' names start so (`gezik-export-<pid>-<n>.tmp`).
const TEMP_PREFIX: &str = "gezik-export-";

/// Where the temporary files go: on Linux the user's own runtime folder when there is one, else
/// the system's temporary folder.
fn temp_dir() -> PathBuf {
    if cfg!(all(unix, not(target_os = "macos")))
        && let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir())
    {
        return dir;
    }
    std::env::temp_dir()
}

/// Deletes the temporary files in `dir` older than a day (an export cut short by a crash).
fn sweep(dir: &Path, now: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with(TEMP_PREFIX) && name.ends_with(".tmp")) {
            continue;
        }
        let old =
            entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| now.duration_since(t).is_ok_and(|age| age > day));
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// A new file only Gezik made, never one already there: `create_new`, owner-only on Unix.
fn create_new_at(path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// A temporary file, deleted when dropped (when the job that copies it ends).
struct TempFile(PathBuf);

impl TempFile {
    /// A new one in `dir`, opened for writing; a name someone else took is passed over.
    fn create_in(dir: &Path) -> io::Result<(TempFile, std::fs::File)> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
        for _ in 0..100 {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!("{TEMP_PREFIX}{}-{n}.tmp", std::process::id()));
            match create_new_at(&path) {
                Ok(file) => return Ok((TempFile(path), file)),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last = err,
                Err(err) => return Err(err),
            }
        }
        Err(last)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::search::{Scope, SearchSpec};

    fn root() -> PathBuf {
        PathBuf::from(if cfg!(windows) { r"C:\Work" } else { "/Work" })
    }

    #[test]
    fn a_folder_exports_into_itself_under_its_name() {
        let home = PathBuf::from("/home/me");
        let (folder, stem, title) = place(&Location::Path(root()), &home).unwrap();
        assert_eq!((folder, stem.as_str()), (root(), "Work"));
        assert_eq!(title, root().display().to_string());
        let (_, stem, _) = place(&Location::Flat(root()), &home).unwrap();
        assert_eq!(stem, "Work");
        assert_eq!(place(&Location::Drives, &home), None);
        assert_eq!(place(&Location::Trash, &home), None);
    }

    #[test]
    fn search_results_export_into_the_searched_folder() {
        let home = PathBuf::from("/home/me");
        let mut spec = SearchSpec::new(Scope::Folder(root()));
        spec.pattern = "*.pdf".into();
        let (folder, stem, title) = place(&Location::Search(Box::new(spec.clone())), &home).unwrap();
        assert_eq!((folder, stem.as_str()), (root(), "Search results"));
        assert_eq!(title, format!("Search: *.pdf in {}", root().display()));
        spec.name = Some("Invoices: 2024".into());
        let (_, stem, _) = place(&Location::Search(Box::new(spec)), &home).unwrap();
        assert_eq!(stem, "Invoices_ 2024");
        let everywhere = SearchSpec::new(Scope::AllDrives);
        let (folder, _, _) = place(&Location::Search(Box::new(everywhere)), &home).unwrap();
        assert_eq!(folder, home, "every drive: the home folder");
    }

    #[test]
    fn the_field_names_the_file_and_its_format() {
        let base = root();
        assert_eq!(typed_target(&base, "  list.csv "), Some(base.join("list.csv")));
        assert_eq!(typed_target(&base, ""), None);
        assert_eq!(field_note(&base, "list.csv"), (format!("CSV into {}", base.display()), false));
        assert_eq!(field_note(&base, "sub/list.HTM").0, format!("HTML into {}", base.join("sub").display()));
        assert_eq!(field_note(&base, "list.doc"), (BAD_EXTENSION.to_owned(), true));
        assert_eq!(field_note(&base, " "), ("Type a file name".to_owned(), true));
    }

    #[test]
    fn a_selection_is_what_goes_unless_unticked() {
        assert_eq!(
            form_checks(1, false),
            [("Only the 1 item selected".to_owned(), true), ("Full paths in text".to_owned(), false)]
        );
        assert_eq!(form_checks(0, true), [("Full paths in text".to_owned(), true)], "no selection: the whole list");
    }

    #[test]
    fn sheets_follow_the_system() {
        let columns = export::columns(Format::Csv, &[ColumnKey::Size], false);
        let csv = sheet(Format::Csv, &columns, "t".into(), 2, true);
        assert_eq!(csv.headers, ["Name", "Size", "Path"]);
        assert_eq!(csv.right, [false, true, false]);
        assert_eq!(csv.bom, cfg!(windows), "a BOM only in a Windows CSV file");
        assert_eq!(csv.eol, if cfg!(windows) { "\r\n" } else { "\n" });
        assert_eq!(csv.separator, gezik_platform::list_separator());
        assert!(!sheet(Format::Csv, &columns, "t".into(), 2, false).bom, "never on the clipboard");
        assert_eq!(sheet(Format::Html, &columns, "t".into(), 1, true).eol, "\n");
        assert!(sheet(Format::Text, &columns, "t".into(), 1, true).note.starts_with("1 item ·"));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-test-export-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_temporary_file_is_new_its_own_and_goes_with_its_guard() {
        let dir = scratch("temp");
        let (temp, _file) = TempFile::create_in(&dir).unwrap();
        let path = temp.0.clone();
        assert!(path.file_name().unwrap().to_string_lossy().starts_with(TEMP_PREFIX));
        let err = create_new_at(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "never opens one already there");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let (other, _) = TempFile::create_in(&dir).unwrap();
        assert_ne!(other.0, path, "each is its own");
        drop(temp);
        assert!(!path.exists());
        drop(other);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_old_export_files_are_swept() {
        let dir = scratch("sweep");
        let old = dir.join(format!("{TEMP_PREFIX}1-0.tmp"));
        let fresh = dir.join(format!("{TEMP_PREFIX}1-1.tmp"));
        let other = dir.join("keep.tmp");
        for path in [&old, &fresh, &other] {
            std::fs::write(path, "x").unwrap();
        }
        let two_days = std::time::Duration::from_secs(2 * 24 * 60 * 60);
        let now = std::time::SystemTime::now();
        for path in [&old, &other] {
            std::fs::File::options().write(true).open(path).unwrap().set_modified(now - two_days).unwrap();
        }
        sweep(&dir, now);
        assert!(!old.exists());
        assert!(fresh.exists() && other.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
