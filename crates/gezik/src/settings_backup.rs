//! Export Settings, Import Settings, the backups and Undo (spec 14 §7), run on a worker thread:
//! `gezik_config::bundle` says which files may come in and what they change; this reads and
//! writes them. An import never merges: each file chosen replaces the one here, after every
//! file it touches has gone into a backup zip in `backups/` (the last 10 are kept). Also the
//! sync folder's setting up and leaving (spec 14 §8.1), which back up first the same way.
//! settings.toml, the themes and the templates are in the shared folder (a sync folder, when one
//! is used); views.toml and `backups/` in this computer's own.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use gezik_config::bundle::{self, Group};
use gezik_config::paths::write_atomic_bytes;

/// A file of an export: its name in the zip (`themes/nord.toml`) and contents.
pub type File = (String, Vec<u8>);

/// An export read: the OS it was made on, its files that may come in, the entries left out.
pub type Read = (String, Vec<(String, Group, Vec<u8>)>, Vec<String>);

/// What a file an import brings does here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    New,
    Replaces,
    Same,
}

#[derive(Debug)]
pub struct Incoming {
    pub name: String,
    pub group: Group,
    pub change: Change,
    pub data: Vec<u8>,
}

/// A zip read, compared with the config folder; nothing written yet.
#[derive(Debug)]
pub struct Preview {
    pub source: PathBuf,
    /// The OS it was exported on.
    pub os: String,
    pub files: Vec<Incoming>,
    /// Entries that may not come in (`..`, an absolute path, `state.toml`…).
    pub left_out: Vec<String>,
    /// settings.toml's changes (`bundle::changed_keys`) and its settings that run programs.
    pub settings: Vec<String>,
    pub programs: Vec<String>,
}

/// What undoes an import: the backup made before it, the files it replaced and those it made.
#[derive(Debug, Clone)]
pub struct Undo {
    pub backup: PathBuf,
    pub replaced: Vec<String>,
    pub created: Vec<String>,
}

/// An import's or undo's outcome: views.toml's new text when it changed ("" when it went), for
/// the folder views in memory.
#[derive(Debug)]
pub struct Done {
    pub undo: Undo,
    pub views: Option<String>,
}

/// The file `name` (as in the zip) stands for in folder `dir`.
fn path_of(dir: &Path, name: &str) -> PathBuf {
    name.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The config folders (spec 14 §3.1): views.toml and `backups/` are in `local`, settings.toml,
/// the themes and the templates in `shared`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folders {
    pub shared: PathBuf,
    pub local: PathBuf,
}

impl Folders {
    /// Both in `dir` (no sync folder).
    pub fn one(dir: &Path) -> Folders {
        Folders { shared: dir.to_path_buf(), local: dir.to_path_buf() }
    }

    pub fn of(store: &gezik_config::store::ConfigStore) -> Folders {
        Folders { shared: store.shared_dir(), local: store.local_dir().to_path_buf() }
    }

    /// The folder file `name` (as in the zip) is in.
    fn dir_of(&self, name: &str) -> &Path {
        if name == "views.toml" { &self.local } else { &self.shared }
    }

    fn path_of(&self, name: &str) -> PathBuf {
        path_of(self.dir_of(name), name)
    }

    fn backups(&self) -> PathBuf {
        self.local.join("backups")
    }
}

fn describe(err: &std::io::Error) -> String {
    gezik_platform::fs::describe(err)
}

/// The file's contents, `None` when it is not there.
fn read_if_there(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(data) => Ok(Some(data)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("Cannot read {}: {}", path.display(), describe(&err))),
    }
}

/// The files of the config folders in `groups`. state.toml, the tag store, the system changes,
/// the tools and the backups are never among them (they belong to this computer).
pub fn collect(folders: &Folders, groups: &[Group]) -> Result<Vec<File>, String> {
    let dir = &folders.shared;
    let mut files = Vec::new();
    for group in groups {
        match group {
            Group::Settings | Group::Views => {
                let name = if *group == Group::Settings { "settings.toml" } else { "views.toml" };
                if let Some(data) = read_if_there(&folders.path_of(name))? {
                    files.push((name.to_owned(), data));
                }
            }
            Group::Themes => {
                let mut names: Vec<String> = match std::fs::read_dir(dir.join("themes")) {
                    Ok(entries) => entries
                        .filter_map(Result::ok)
                        .filter(|e| e.file_type().is_ok_and(|t| t.is_file() || t.is_symlink()))
                        .filter_map(|e| e.file_name().into_string().ok())
                        .map(|name| format!("themes/{name}"))
                        .filter(|name| bundle::group_of(name) == Some(Group::Themes))
                        .collect(),
                    Err(err) if err.kind() == ErrorKind::NotFound => Vec::new(),
                    Err(err) => return Err(format!("Cannot read the themes folder: {}", describe(&err))),
                };
                names.sort();
                for name in names {
                    if let Some(data) = read_if_there(&path_of(dir, &name))? {
                        files.push((name, data));
                    }
                }
            }
            Group::Templates => {
                let mut total = 0;
                templates(&dir.join("templates"), "templates", &mut files, &mut total)?;
            }
        }
    }
    Ok(files)
}

/// The templates folder's files, links left out, within the size limits.
fn templates(folder: &Path, name: &str, files: &mut Vec<File>, total: &mut u64) -> Result<(), String> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("Cannot read {}: {}", folder.display(), describe(&err))),
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let Ok(file_name) = entry.file_name().into_string() else { continue };
        let inner = format!("{name}/{file_name}");
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            templates(&entry.path(), &inner, files, total)?;
        } else if kind.is_file() && bundle::group_of(&inner) == Some(Group::Templates) {
            let data = std::fs::read(entry.path()).map_err(|err| format!("Cannot read {inner}: {}", describe(&err)))?;
            *total += data.len() as u64;
            if data.len() as u64 > bundle::MAX_FILE || *total > bundle::MAX_TEMPLATES {
                return Err("The templates are larger than 256 MB (or one is over 64 MB): leave them out".to_owned());
            }
            if files.len() >= bundle::MAX_ENTRIES - 1 {
                return Err("More than 4,096 files: leave the templates out".to_owned());
            }
            files.push((inner, data));
        }
    }
    Ok(())
}

/// The manifest of an export made now.
fn manifest() -> String {
    let date = gezik_platform::local_date_parts(std::time::SystemTime::now())
        .map(|p| format!("{:04}-{:02}-{:02}", p.year, p.month, p.day))
        .unwrap_or_default();
    bundle::manifest(env!("CARGO_PKG_VERSION"), std::env::consts::OS, &date)
}

/// `files` with the manifest, as a zip.
fn zip(files: &[File]) -> Result<Vec<u8>, String> {
    let all: Vec<File> =
        std::iter::once((bundle::MANIFEST.to_owned(), manifest().into_bytes())).chain(files.iter().cloned()).collect();
    gezik_batch::archive::write_zip_entries(&all).map_err(|err| format!("Cannot make the zip: {}", describe(&err)))
}

/// Writes the config folders' files in `groups` to the zip `out` (which must not be there yet);
/// how many files went in.
pub fn export(folders: &Folders, out: &Path, groups: &[Group]) -> Result<usize, String> {
    if out.symlink_metadata().is_ok() {
        return Err(format!("{} is there already: give another name", out.display()));
    }
    let files = collect(folders, groups)?;
    let bytes = zip(&files)?;
    write_atomic_bytes(out, &bytes).map_err(|err| format!("Cannot write {}: {}", out.display(), describe(&err)))?;
    Ok(files.len())
}

/// The files of export `path` that may come in, the OS it was made on, and the entries left
/// out. Read in memory, within the limits (300 MB, 4,096 files, 64 MB each). `check`: a zip
/// from elsewhere, refused when its settings.toml or views.toml does not read; one of Gezik's
/// own backups is not checked (it holds what was there, broken or not, byte for byte).
pub fn read(path: &Path, check: bool) -> Result<Read, String> {
    let cannot = |err: std::io::Error| format!("Cannot read {}: {}", path.display(), describe(&err));
    let size = std::fs::metadata(path).map_err(cannot)?.len();
    if size > bundle::MAX_ZIP {
        return Err(format!("{} is larger than 300 MB: not a settings export", path.display()));
    }
    let bytes = std::fs::read(path).map_err(cannot)?;
    let offered = std::cell::RefCell::new((0usize, Vec::new()));
    let wanted = |name: &str| {
        let mut offered = offered.borrow_mut();
        offered.0 += 1;
        let ok = name == bundle::MANIFEST || bundle::group_of(name).is_some();
        if !ok {
            offered.1.push(name.to_owned());
        }
        ok
    };
    let entries = gezik_batch::archive::read_zip_entries(
        &bytes,
        &wanted,
        bundle::MAX_FILE,
        bundle::MAX_ENTRIES + 1,
        bundle::MAX_ZIP,
    )
    .map_err(|err| format!("{} does not read as a zip: {}", path.display(), describe(&err)))?;
    let (count, left_out) = offered.into_inner();
    if count > bundle::MAX_ENTRIES {
        return Err("It has more than 4,096 files: not a settings export".to_owned());
    }
    let mut manifest = None;
    let mut files: Vec<(String, Group, Vec<u8>)> = Vec::new();
    for (name, data) in entries {
        let data = data.ok_or_else(|| format!("{name} is larger than 64 MB"))?;
        if files.iter().any(|(n, _, _)| n.eq_ignore_ascii_case(&name))
            || (manifest.is_some() && name == bundle::MANIFEST)
        {
            return Err(format!("{name} is in it twice"));
        }
        match bundle::group_of(&name) {
            Some(group) => files.push((name, group, data)),
            None => manifest = Some(String::from_utf8_lossy(&data).into_owned()),
        }
    }
    let os = bundle::read_manifest(manifest.as_deref())?;
    for (name, group, data) in files.iter().filter(|_| check) {
        let text = || std::str::from_utf8(data).map_err(|_| format!("{name} is not text"));
        match group {
            Group::Settings => {
                gezik_config::schema::values(text()?).map(drop).map_err(|w| format!("Not imported: {w}"))?
            }
            Group::Views => gezik_config::views_file::parse_views(text()?)
                .map(drop)
                .map_err(|err| format!("Not imported: views.toml: {err}"))?,
            Group::Themes => drop(text()?),
            Group::Templates => {}
        }
    }
    Ok((os, files, left_out))
}

/// Export `path` compared with the config folders (`check`: see [`read`]).
pub fn preview(folders: &Folders, path: &Path, check: bool) -> Result<Preview, String> {
    let (os, entries, left_out) = read(path, check)?;
    let mut files = Vec::new();
    for (name, group, data) in entries {
        let change = match read_if_there(&folders.path_of(&name))? {
            None => Change::New,
            Some(here) if here == data => Change::Same,
            Some(_) => Change::Replaces,
        };
        files.push(Incoming { name, group, change, data });
    }
    let current = read_if_there(&folders.path_of("settings.toml"))?.unwrap_or_default();
    let current = String::from_utf8_lossy(&current);
    let (settings, programs) = match files.iter().find(|f| f.group == Group::Settings) {
        Some(file) => {
            let imported = String::from_utf8_lossy(&file.data);
            (bundle::changed_keys(&current, &imported), bundle::programs(&current, &imported))
        }
        None => (Vec::new(), Vec::new()),
    };
    Ok(Preview { source: path.to_owned(), os, files, left_out, settings, programs })
}

/// A backup's name for now: the time down to the millisecond, so they sort by it.
pub fn stamp() -> String {
    let now = std::time::SystemTime::now();
    let millis = now.duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_millis()).unwrap_or_default();
    let p = gezik_platform::local_date_parts(now).unwrap_or_default();
    format!(
        "{}{:04}-{:02}-{:02}-{:02}{:02}{:02}-{millis:03}.zip",
        bundle::BACKUP_PREFIX,
        p.year,
        p.month,
        p.day,
        p.hour,
        p.minute,
        p.second
    )
}

/// Backs up the config folders' settings and themes, and `also`, to `backups/<name>`; the
/// oldest backups beyond 10 go to `discard` (the trash). The backup's path and its files.
pub fn backup(
    folders: &Folders,
    also: &[Group],
    name: &str,
    discard: &dyn Fn(&Path),
) -> Result<(PathBuf, Vec<File>), String> {
    let groups: Vec<Group> =
        Group::ALL.into_iter().filter(|g| matches!(g, Group::Settings | Group::Themes) || also.contains(g)).collect();
    let files = collect(folders, &groups).map_err(|why| format!("Nothing changed: the backup failed: {why}"))?;
    let folder = folders.backups();
    let path = folder.join(name);
    std::fs::create_dir_all(&folder)
        .and_then(|()| zip(&files).map_err(std::io::Error::other).and_then(|bytes| write_atomic_bytes(&path, &bytes)))
        .map_err(|err| format!("Nothing changed: cannot write the backup {}: {}", path.display(), describe(&err)))?;
    for old in bundle::backups_to_drop(names_in(&folder)) {
        discard(&folder.join(old));
    }
    Ok((path, files))
}

fn names_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()).collect()
}

/// The backups, newest first (10 at most).
pub fn backups(folders: &Folders) -> Vec<String> {
    let mut names = bundle::newest_first(names_in(&folders.backups()));
    names.truncate(bundle::KEEP_BACKUPS);
    names
}

/// Writes the files chosen of an import into the config folders, after the backup `name`.
/// A write that fails puts back what was written before it.
pub fn import(folders: &Folders, chosen: Vec<Incoming>, name: &str, discard: &dyn Fn(&Path)) -> Result<Done, String> {
    let chosen: Vec<Incoming> = chosen.into_iter().filter(|f| f.change != Change::Same).collect();
    let groups: Vec<Group> = chosen.iter().map(|f| f.group).collect();
    let (backup, before) = backup(folders, &groups, name, discard)?;
    // Each file written over goes by its name on disk (`nord.toml` for the zip's `Nord.toml` on
    // Windows and macOS), and must be in the backup: else nothing is written.
    let mut targets = Vec::new();
    for file in &chosen {
        let there = folders.path_of(&file.name).symlink_metadata().is_ok();
        let real = if there { real_name(folders.dir_of(&file.name), &file.name) } else { None };
        if there && real.as_ref().is_none_or(|real| !before.iter().any(|(n, _)| n == real)) {
            return Err(format!(
                "Nothing imported: {} here is a link or has a name the backup cannot hold; move it away or leave \
                 its kind of file out",
                file.name
            ));
        }
        targets.push(real);
    }
    let mut undo = Undo { backup, replaced: Vec::new(), created: Vec::new() };
    let mut views = None;
    for (file, real) in chosen.into_iter().zip(targets) {
        let path = folders.path_of(real.as_deref().unwrap_or(&file.name));
        let written =
            path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| write_atomic_bytes(&path, &file.data));
        if let Err(err) = written {
            let put_back = match put_back(folders, &undo, &before) {
                Ok(()) => "the files written before it are back as they were".to_owned(),
                Err(why) => format!("{why}; Restore Backup has them"),
            };
            return Err(format!("Cannot write {}: {} — {put_back}", file.name, describe(&err)));
        }
        if file.group == Group::Views {
            views = Some(String::from_utf8_lossy(&file.data).into_owned());
        }
        match real {
            Some(real) => undo.replaced.push(real),
            None => undo.created.push(file.name),
        }
    }
    Ok(Done { undo, views })
}

/// `name`'s spelling on disk, folder by folder (the exact one first, else one that differs only
/// in case), or `None` when a part is not there or not UTF-8.
fn real_name(dir: &Path, name: &str) -> Option<String> {
    let mut folder = dir.to_path_buf();
    let mut parts = Vec::new();
    for part in name.split('/') {
        let names: Vec<String> = std::fs::read_dir(&folder)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        let found = names
            .iter()
            .find(|n| *n == part)
            .or_else(|| names.iter().find(|n| n.to_lowercase() == part.to_lowercase()))?;
        folder.push(found);
        parts.push(found.clone());
    }
    Some(parts.join("/"))
}

/// Puts the files `undo` names back as `before` has them: those replaced written, those made
/// removed.
fn put_back(folders: &Folders, undo: &Undo, before: &[File]) -> Result<(), String> {
    let mut failed = None;
    for name in &undo.replaced {
        let Some((_, data)) = before.iter().find(|(n, _)| n == name) else {
            failed = Some(format!("Cannot put back {name}: it is not in the backup"));
            continue;
        };
        if let Err(err) = write_atomic_bytes(&folders.path_of(name), data) {
            failed = Some(format!("Cannot put back {name}: {}", describe(&err)));
        }
    }
    for name in &undo.created {
        match std::fs::remove_file(folders.path_of(name)) {
            Err(err) if err.kind() != ErrorKind::NotFound => {
                failed = Some(format!("Cannot remove {name}: {}", describe(&err)));
            }
            _ => {}
        }
    }
    failed.map_or(Ok(()), Err)
}

/// Undoes an import: the files it replaced come back from its backup, byte for byte, and those
/// it made go. What is there now is backed up first (Restore Backup has it).
pub fn undo(folders: &Folders, undo: &Undo, name: &str, discard: &dyn Fn(&Path)) -> Result<Done, String> {
    let (_, files, _) = read(&undo.backup, false)?;
    let before: Vec<File> = files.into_iter().map(|(name, _, data)| (name, data)).collect();
    let touched: Vec<Group> = undo.replaced.iter().chain(&undo.created).filter_map(|n| bundle::group_of(n)).collect();
    let (backup, _) = backup(folders, &touched, name, discard)?;
    put_back(folders, undo, &before)?;
    let views = touched.contains(&Group::Views).then(|| {
        before
            .iter()
            .find(|(n, _)| n == "views.toml")
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
            .unwrap_or_default()
    });
    Ok(Done { undo: Undo { backup, replaced: Vec::new(), created: Vec::new() }, views })
}

/// What a folder chosen to keep the settings in holds (spec 14 §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    Empty,
    /// A settings.toml: another computer set it up first.
    Settings,
}

/// How a sync folder is taken up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// No settings.toml there: this computer's files are copied in.
    Copy,
    /// "Use the Settings in This Folder": this computer's are only backed up.
    Use,
    /// "Replace Them with This Computer's": the folder's are backed up, then written over.
    Replace,
}

/// Whether `folder` holds Gezik's settings; made when it is not there. The config folders
/// themselves are refused.
pub fn look(folders: &Folders, folder: &Path) -> Result<Found, String> {
    if !folder.is_absolute() {
        return Err("Type the whole path of a folder".to_owned());
    }
    let same =
        |a: &Path, b: &Path| a == b || std::fs::canonicalize(a).ok().is_some_and(|a| b.canonicalize().ok() == Some(a));
    if same(folder, &folders.shared) || same(folder, &folders.local) {
        return Err("The settings are in that folder already".to_owned());
    }
    std::fs::create_dir_all(folder).map_err(|err| format!("Cannot make {}: {}", folder.display(), describe(&err)))?;
    Ok(if folder.join("settings.toml").is_file() { Found::Settings } else { Found::Empty })
}

/// Writes `files` into folder `dir`; a template there already is never written over.
fn write_into(dir: &Path, files: Vec<File>) -> Result<(), String> {
    for (name, data) in files {
        let path = path_of(dir, &name);
        if bundle::group_of(&name) == Some(Group::Templates) && path.symlink_metadata().is_ok() {
            continue;
        }
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| write_atomic_bytes(&path, &data))
            .map_err(|err| format!("Cannot write {}: {}", path.display(), describe(&err)))?;
    }
    Ok(())
}

/// Takes up sync folder `folder` as `start` says (spec 14 §8.1): what would be written over is
/// backed up first (`backups/`); this computer's `[system]` goes to settings.local.toml (a tray
/// here is not the others'; decision 12) unless that has one; the computer's name goes in the
/// folder's `machines/` (OneDrive names conflicting copies by it); `location.toml` is written
/// last. The caller then moves the store there. The backup's path.
pub fn start_sync(
    folders: &Folders,
    folder: &Path,
    start: Start,
    machine: Option<&str>,
    discard: &dyn Fn(&Path),
) -> Result<PathBuf, String> {
    use gezik_config::sync_folder::{self, LOCAL_SETTINGS, LOCATION_FILE};
    let theirs = Folders { shared: folder.to_path_buf(), local: folders.local.clone() };
    let (backup, _) = backup(if start == Start::Replace { &theirs } else { folders }, &[], &stamp(), discard)?;
    if start != Start::Use {
        let mut mine = collect(folders, &[Group::Settings, Group::Themes, Group::Templates])?;
        if start == Start::Copy {
            // Nothing there is written over.
            mine.retain(|(name, _)| path_of(folder, name).symlink_metadata().is_err());
        }
        write_into(folder, mine)?;
    }
    let local = folders.local.join(LOCAL_SETTINGS);
    let mine = std::fs::read_to_string(folders.shared.join("settings.toml")).unwrap_or_default();
    let here = std::fs::read_to_string(&local).unwrap_or_default();
    if let Some(text) = sync_folder::local_with_system(&mine, &here) {
        write_atomic_bytes(&local, text.as_bytes())
            .map_err(|err| format!("Cannot write {}: {}", local.display(), describe(&err)))?;
    }
    if let Some(machine) = machine {
        let _ = sync_folder::add_machine(folder, machine);
    }
    let text = sync_folder::location_text(folder, &gezik_config::paths::KnownDirs::system());
    let location = folders.local.join(LOCATION_FILE);
    write_atomic_bytes(&location, text.as_bytes())
        .map_err(|err| format!("Cannot write {}: {}", location.display(), describe(&err)))?;
    Ok(backup)
}

/// Leaves the sync folder (spec 14 §8.1): its settings.toml, themes and templates (its last
/// copy when it is not there) are copied into this computer's folder after a backup of what is
/// there, and `location.toml` goes. Nothing in the sync folder is touched. The backup's path.
pub fn stop_sync(folders: &Folders, discard: &dyn Fn(&Path)) -> Result<PathBuf, String> {
    use gezik_config::sync_folder::{CACHE, LOCATION_FILE};
    let there = folders.shared.join("settings.toml").is_file();
    let from = if there { folders.shared.clone() } else { folders.local.join(CACHE) };
    let source = Folders { shared: from, local: folders.local.clone() };
    let groups: &[Group] = if there { &[Group::Settings, Group::Themes, Group::Templates] } else { &Group::ALL[..2] };
    let files = collect(&source, groups)?;
    if !files.iter().any(|(name, _)| name == "settings.toml") {
        return Err("The settings folder is not available and no copy of it is here: nothing changed".to_owned());
    }
    let mine = Folders::one(&folders.local);
    let (backup, _) = backup(&mine, &[], &stamp(), discard)?;
    write_into(&folders.local, files)?;
    match std::fs::remove_file(folders.local.join(LOCATION_FILE)) {
        Err(err) if err.kind() != ErrorKind::NotFound => {
            Err(format!("Cannot remove {LOCATION_FILE}: {}", describe(&err)))
        }
        _ => Ok(backup),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-backup-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(dir: &Path, name: &str, text: &str) {
        let path = path_of(dir, name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const SETTINGS: &str = "theme = \"nord\"\npinned = [\"{home}/Projects\"]\n";

    #[test]
    fn an_export_holds_the_shared_files_only_and_reads_back() {
        let dir = scratch("export");
        put(&dir, "settings.toml", SETTINGS);
        put(&dir, "themes/nord.toml", "[colors]\n");
        put(&dir, "templates/work/Report.txt", "R");
        put(&dir, "views.toml", "");
        for local in ["state.toml", "system-changes.toml", "tags-store.tsv", "backups/gezik-settings-1.zip"] {
            put(&dir, local, "x");
        }
        let out = dir.join("out.zip");
        assert_eq!(export(&Folders::one(&dir), &out, &[Group::Settings, Group::Themes, Group::Templates]), Ok(3));
        assert!(export(&Folders::one(&dir), &out, &[Group::Settings]).unwrap_err().contains("there already"));
        let (os, files, left_out) = read(&out, true).unwrap();
        assert_eq!(os, std::env::consts::OS);
        let names: Vec<&str> = files.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(names, ["settings.toml", "themes/nord.toml", "templates/work/Report.txt"]);
        assert_eq!(files[0].2, SETTINGS.as_bytes(), "{{home}} stays a token: the other system's home");
        assert!(left_out.is_empty());
    }

    #[test]
    fn a_zip_is_checked_before_anything_is_written() {
        let dir = scratch("check");
        let manifest = bundle::manifest("0.1.0", "linux", "2026-10-12");
        let path = dir.join("in.zip");
        let read_zip = |entries: &[(&str, &[u8])]| {
            std::fs::write(&path, zip_of(entries)).unwrap();
            read(&path, true)
        };
        let (os, files, left_out) = read_zip(&[
            (bundle::MANIFEST, manifest.as_bytes()),
            ("settings.toml", SETTINGS.as_bytes()),
            ("../evil.toml", b"x"),
            ("/etc/passwd", b"x"),
            ("templates/../../x.exe", b"x"),
            ("state.toml", b"x"),
        ])
        .unwrap();
        assert_eq!((os.as_str(), files.len()), ("linux", 1));
        assert_eq!(left_out, ["../evil.toml", "/etc/passwd", "templates/../../x.exe", "state.toml"]);
        assert!(
            read_zip(&[(bundle::MANIFEST, manifest.as_bytes()), ("settings.toml", b"[layout\n")])
                .unwrap_err()
                .starts_with("Not imported: settings.toml line 1")
        );
        assert!(read_zip(&[("settings.toml", SETTINGS.as_bytes())]).unwrap_err().contains("no gezik-export.toml"));
        assert!(
            read_zip(&[(bundle::MANIFEST, manifest.as_bytes()), ("themes/a.toml", b"1"), ("themes/A.toml", b"2")])
                .unwrap_err()
                .contains("twice")
        );
        let many: Vec<(String, Vec<u8>)> = (0..=bundle::MAX_ENTRIES).map(|i| (format!("x{i}"), Vec::new())).collect();
        let refs: Vec<(&str, &[u8])> = many.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        assert!(read_zip(&refs).unwrap_err().contains("4,096"));
        std::fs::write(&path, b"not a zip").unwrap();
        assert!(read(&path, true).unwrap_err().contains("does not read as a zip"));
    }

    #[test]
    fn an_import_backs_up_first_and_undo_puts_back_every_byte() {
        let dir = scratch("import");
        let old_settings = "theme = \"dark\"   # mine\n";
        put(&dir, "settings.toml", old_settings);
        put(&dir, "themes/nord.toml", "old nord");
        put(&dir, "themes/mine.toml", "mine");
        let source = scratch("import-source");
        put(&source, "settings.toml", SETTINGS);
        put(&source, "themes/nord.toml", "new nord");
        put(&source, "themes/mine.toml", "mine");
        put(&source, "themes/frost.toml", "frost");
        put(&source, "templates/a/b.txt", "b");
        let zip = source.join("out.zip");
        export(&Folders::one(&source), &zip, &[Group::Settings, Group::Themes, Group::Templates]).unwrap();

        let shown = preview(&Folders::one(&dir), &zip, true).unwrap();
        let changes: Vec<(&str, Change)> = shown.files.iter().map(|f| (f.name.as_str(), f.change)).collect();
        assert_eq!(
            changes,
            [
                ("settings.toml", Change::Replaces),
                ("themes/frost.toml", Change::New),
                ("themes/mine.toml", Change::Same),
                ("themes/nord.toml", Change::Replaces),
                ("templates/a/b.txt", Change::New),
            ]
        );
        assert_eq!(shown.settings, ["theme: dark → nord", "pinned: (default) → {home}/Projects"]);

        let done =
            import(&Folders::one(&dir), shown.files, "gezik-settings-1.zip", &|_| panic!("nothing to drop")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("settings.toml")).unwrap(), SETTINGS);
        assert_eq!(std::fs::read_to_string(path_of(&dir, "templates/a/b.txt")).unwrap(), "b");
        assert_eq!(done.undo.replaced, ["settings.toml", "themes/nord.toml"]);
        assert_eq!(done.undo.created, ["themes/frost.toml", "templates/a/b.txt"]);
        let (_, backed_up, _) = read(&done.undo.backup, false).unwrap();
        let backed_up: Vec<&str> = backed_up.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(backed_up, ["settings.toml", "themes/mine.toml", "themes/nord.toml"], "as they were");

        undo(&Folders::one(&dir), &done.undo, "gezik-settings-2.zip", &|_| {}).unwrap();
        assert_eq!(std::fs::read(dir.join("settings.toml")).unwrap(), old_settings.as_bytes());
        assert_eq!(std::fs::read_to_string(dir.join("themes/nord.toml")).unwrap(), "old nord");
        assert!(!dir.join("themes/frost.toml").exists() && !path_of(&dir, "templates/a/b.txt").exists());
        assert_eq!(backups(&Folders::one(&dir)), ["gezik-settings-2.zip", "gezik-settings-1.zip"]);
    }

    #[test]
    fn a_broken_settings_file_comes_back_on_undo() {
        let dir = scratch("broken");
        let broken = "theme = \"dark\"\n[layout\n";
        put(&dir, "settings.toml", broken);
        let source = scratch("broken-source");
        put(&source, "settings.toml", SETTINGS);
        let zip = source.join("out.zip");
        export(&Folders::one(&source), &zip, &[Group::Settings]).unwrap();
        let shown = preview(&Folders::one(&dir), &zip, true).unwrap();
        let done = import(&Folders::one(&dir), shown.files, "gezik-settings-1.zip", &|_| {}).unwrap();
        assert!(read(&done.undo.backup, true).is_err(), "checked, the backup would be refused");
        assert!(preview(&Folders::one(&dir), &done.undo.backup, false).is_ok(), "Restore Backup reads it");
        undo(&Folders::one(&dir), &done.undo, "gezik-settings-2.zip", &|_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("settings.toml")).unwrap(), broken);
    }

    #[test]
    fn a_file_is_put_back_under_its_name_on_disk() {
        let dir = scratch("case");
        put(&dir, "themes/nord.toml", "old nord");
        let source = scratch("case-source");
        put(&source, "themes/Nord.toml", "new nord");
        let zip = source.join("out.zip");
        export(&Folders::one(&source), &zip, &[Group::Themes]).unwrap();
        let shown = preview(&Folders::one(&dir), &zip, true).unwrap();
        let done = import(&Folders::one(&dir), shown.files, "gezik-settings-1.zip", &|_| {}).unwrap();
        let insensitive = dir.join("themes/NORD.toml").exists();
        if insensitive {
            assert_eq!(done.undo.replaced, ["themes/nord.toml"], "the name on disk, as the backup has it");
            assert_eq!(std::fs::read_to_string(dir.join("themes/nord.toml")).unwrap(), "new nord");
        } else {
            assert_eq!(done.undo.created, ["themes/Nord.toml"], "another file where case counts");
        }
        undo(&Folders::one(&dir), &done.undo, "gezik-settings-2.zip", &|_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("themes/nord.toml")).unwrap(), "old nord");

        let missing =
            Undo { backup: PathBuf::new(), replaced: vec!["themes/gone.toml".to_owned()], created: Vec::new() };
        assert_eq!(
            put_back(&Folders::one(&dir), &missing, &[]),
            Err("Cannot put back themes/gone.toml: it is not in the backup".to_owned())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_template_stops_the_import() {
        let dir = scratch("link");
        put(&dir, "elsewhere.txt", "mine");
        std::fs::create_dir_all(dir.join("templates")).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere.txt"), dir.join("templates/a.txt")).unwrap();
        let source = scratch("link-source");
        put(&source, "templates/a.txt", "new");
        let zip = source.join("out.zip");
        export(&Folders::one(&source), &zip, &[Group::Templates]).unwrap();
        let shown = preview(&Folders::one(&dir), &zip, true).unwrap();
        let refused = import(&Folders::one(&dir), shown.files, "gezik-settings-1.zip", &|_| {}).unwrap_err();
        assert!(refused.starts_with("Nothing imported: templates/a.txt"), "{refused}");
        assert_eq!(std::fs::read_to_string(dir.join("elsewhere.txt")).unwrap(), "mine");
    }

    #[test]
    fn eleven_backups_leave_ten() {
        let dir = scratch("rotate");
        put(&dir, "settings.toml", SETTINGS);
        for i in 0..11 {
            backup(&Folders::one(&dir), &[], &format!("gezik-settings-{i:02}.zip"), &|old| {
                std::fs::remove_file(old).unwrap()
            })
            .unwrap();
        }
        let kept = backups(&Folders::one(&dir));
        assert_eq!(
            (kept.len(), kept[0].as_str(), kept[9].as_str()),
            (10, "gezik-settings-10.zip", "gezik-settings-01.zip")
        );
        assert!(!dir.join("backups/gezik-settings-00.zip").exists());
    }

    #[test]
    fn a_sync_folder_is_set_up_each_way_with_a_backup_first_and_left_again() {
        use gezik_config::sync_folder::{CACHE, LOCAL_SETTINGS, LOCATION_FILE, MACHINES};
        let root = scratch("sync");
        let local = root.join("local");
        let mine = Folders::one(&local);
        put(&local, "settings.toml", "theme = \"nord\"\n[system]\ntray = true\n");
        put(&local, "themes/nord.toml", "mine");
        put(&local, "templates/a.txt", "a");
        let folder = root.join("Dropbox").join("Gezik");
        assert!(look(&mine, Path::new("relative/x")).is_err());
        assert!(look(&mine, &local).unwrap_err().contains("already"));
        // An empty folder: this computer's files are copied in.
        assert_eq!(look(&mine, &folder), Ok(Found::Empty));
        put(&folder, "templates/a.txt", "theirs");
        let backup = start_sync(&mine, &folder, Start::Copy, Some("DESKTOP-AB12"), &|_| {}).unwrap();
        assert!(backup.starts_with(local.join("backups")));
        assert_eq!(
            std::fs::read_to_string(folder.join("settings.toml")).unwrap(),
            "theme = \"nord\"\n[system]\ntray = true\n"
        );
        assert_eq!(std::fs::read_to_string(folder.join("themes/nord.toml")).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(folder.join("templates/a.txt")).unwrap(), "theirs", "never written over");
        assert!(folder.join(MACHINES).join("DESKTOP-AB12").is_file());
        assert!(std::fs::read_to_string(local.join(LOCAL_SETTINGS)).unwrap().contains("tray = true"));
        let known = gezik_config::paths::KnownDirs::system();
        assert_eq!(gezik_config::sync_folder::read_location(&local, &known), Some(folder.clone()));

        // A second computer: Use keeps the folder's, Replace backs them up and writes over.
        let other = root.join("other");
        let theirs = Folders::one(&other);
        put(&other, "settings.toml", "theme = \"dark\"\n");
        assert_eq!(look(&theirs, &folder), Ok(Found::Settings));
        start_sync(&theirs, &folder, Start::Use, None, &|_| {}).unwrap();
        assert!(std::fs::read_to_string(folder.join("settings.toml")).unwrap().contains("nord"));
        assert!(!other.join(LOCAL_SETTINGS).exists(), "no [system] here: none copied");
        let backup = start_sync(&theirs, &folder, Start::Replace, None, &|_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(folder.join("settings.toml")).unwrap(), "theme = \"dark\"\n");
        let (_, kept, _) = read(&backup, false).unwrap();
        assert!(
            kept.iter().any(|(n, _, d)| n == "settings.toml" && d.starts_with(b"theme = \"nord\"")),
            "the folder's"
        );

        // Leaving: the folder's files come here, after a backup; the folder is left as it is.
        let synced = Folders { shared: folder.clone(), local: local.clone() };
        put(&folder, "themes/frost.toml", "frost");
        stop_sync(&synced, &|_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(local.join("settings.toml")).unwrap(), "theme = \"dark\"\n");
        assert_eq!(std::fs::read_to_string(local.join("themes/frost.toml")).unwrap(), "frost");
        assert!(!local.join(LOCATION_FILE).exists() && folder.join("settings.toml").is_file());
        // The folder gone: its last copy; no copy either: nothing changes.
        put(&local, &format!("{CACHE}/settings.toml"), "theme = \"light\"\n");
        let gone = Folders { shared: root.join("gone"), local: local.clone() };
        stop_sync(&gone, &|_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(local.join("settings.toml")).unwrap(), "theme = \"light\"\n");
        std::fs::remove_dir_all(local.join(CACHE)).unwrap();
        assert!(stop_sync(&gone, &|_| {}).unwrap_err().contains("nothing changed"));
    }

    #[test]
    fn views_and_backups_are_this_computers_and_the_rest_is_shared() {
        let root = scratch("split");
        let folders = Folders { shared: root.join("shared"), local: root.join("local") };
        put(&folders.shared, "settings.toml", SETTINGS);
        put(&folders.local, "views.toml", "");
        put(&folders.shared, "views.toml", "not this one");
        let out = root.join("out.zip");
        assert_eq!(export(&folders, &out, &[Group::Settings, Group::Views]), Ok(2));
        let (_, files, _) = read(&out, true).unwrap();
        assert_eq!(files[1].2, b"", "views.toml from this computer's folder");
        backup(&folders, &[], "gezik-settings-1.zip", &|_| {}).unwrap();
        assert_eq!(backups(&folders), ["gezik-settings-1.zip"]);
        assert!(
            folders.local.join("backups/gezik-settings-1.zip").is_file() && !folders.shared.join("backups").exists()
        );
    }
}
