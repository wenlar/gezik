//! Export Settings, Import Settings, the backups and Undo (spec 14 §7), run on a worker thread:
//! `gezik_config::bundle` says which files may come in and what they change; this reads and
//! writes them. An import never merges: each file chosen replaces the one here, after every
//! file it touches has gone into a backup zip in `backups/` (the last 10 are kept).

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

/// The file `name` (as in the zip) stands for in config folder `dir`.
fn path_of(dir: &Path, name: &str) -> PathBuf {
    name.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
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

/// The files of config folder `dir` in `groups`. state.toml, the tag store, the system changes,
/// the tools and the backups are never among them (they belong to this computer).
pub fn collect(dir: &Path, groups: &[Group]) -> Result<Vec<File>, String> {
    let mut files = Vec::new();
    for group in groups {
        match group {
            Group::Settings | Group::Views => {
                let name = if *group == Group::Settings { "settings.toml" } else { "views.toml" };
                if let Some(data) = read_if_there(&dir.join(name))? {
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

/// Writes config folder `dir`'s files in `groups` to the zip `out` (which must not be there
/// yet); how many files went in.
pub fn export(dir: &Path, out: &Path, groups: &[Group]) -> Result<usize, String> {
    if out.symlink_metadata().is_ok() {
        return Err(format!("{} is there already: give another name", out.display()));
    }
    let files = collect(dir, groups)?;
    let bytes = zip(&files)?;
    write_atomic_bytes(out, &bytes).map_err(|err| format!("Cannot write {}: {}", out.display(), describe(&err)))?;
    Ok(files.len())
}

/// The files of export `path` that may come in, the OS it was made on, and the entries left
/// out. Read in memory, within the limits (300 MB, 4,096 files, 64 MB each); a zip with a
/// settings.toml or views.toml that does not read is refused.
pub fn read(path: &Path) -> Result<Read, String> {
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
    for (name, group, data) in &files {
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

/// Export `path` compared with config folder `dir`.
pub fn preview(dir: &Path, path: &Path) -> Result<Preview, String> {
    let (os, entries, left_out) = read(path)?;
    let mut files = Vec::new();
    for (name, group, data) in entries {
        let change = match read_if_there(&path_of(dir, &name))? {
            None => Change::New,
            Some(here) if here == data => Change::Same,
            Some(_) => Change::Replaces,
        };
        files.push(Incoming { name, group, change, data });
    }
    let current = read_if_there(&dir.join("settings.toml"))?.unwrap_or_default();
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

/// Backs up config folder `dir`'s settings and themes, and `also`, to `backups/<name>`; the
/// oldest backups beyond 10 go to `discard` (the trash). The backup's path and its files.
pub fn backup(dir: &Path, also: &[Group], name: &str, discard: &dyn Fn(&Path)) -> Result<(PathBuf, Vec<File>), String> {
    let groups: Vec<Group> =
        Group::ALL.into_iter().filter(|g| matches!(g, Group::Settings | Group::Themes) || also.contains(g)).collect();
    let files = collect(dir, &groups).map_err(|why| format!("Nothing imported: the backup failed: {why}"))?;
    let folder = dir.join("backups");
    let path = folder.join(name);
    std::fs::create_dir_all(&folder)
        .and_then(|()| zip(&files).map_err(std::io::Error::other).and_then(|bytes| write_atomic_bytes(&path, &bytes)))
        .map_err(|err| format!("Nothing imported: cannot write the backup {}: {}", path.display(), describe(&err)))?;
    for old in bundle::backups_to_drop(names_in(&folder)) {
        discard(&folder.join(old));
    }
    Ok((path, files))
}

fn names_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()).collect()
}

/// The backups in config folder `dir`, newest first (10 at most).
pub fn backups(dir: &Path) -> Vec<String> {
    let mut names = bundle::newest_first(names_in(&dir.join("backups")));
    names.truncate(bundle::KEEP_BACKUPS);
    names
}

/// Writes the files chosen of an import into config folder `dir`, after the backup `name`.
/// A write that fails puts back what was written before it.
pub fn import(dir: &Path, chosen: Vec<Incoming>, name: &str, discard: &dyn Fn(&Path)) -> Result<Done, String> {
    let chosen: Vec<Incoming> = chosen.into_iter().filter(|f| f.change != Change::Same).collect();
    let groups: Vec<Group> = chosen.iter().map(|f| f.group).collect();
    let (backup, before) = backup(dir, &groups, name, discard)?;
    let mut undo = Undo { backup, replaced: Vec::new(), created: Vec::new() };
    let mut views = None;
    for file in chosen {
        let path = path_of(dir, &file.name);
        let there = path.symlink_metadata().is_ok();
        let written =
            path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| write_atomic_bytes(&path, &file.data));
        if let Err(err) = written {
            let put_back = match put_back(dir, &undo, &before) {
                Ok(()) => "the files written before it are back as they were".to_owned(),
                Err(why) => format!("{why}; Restore Backup has them"),
            };
            return Err(format!("Cannot write {}: {} — {put_back}", file.name, describe(&err)));
        }
        if file.group == Group::Views {
            views = Some(String::from_utf8_lossy(&file.data).into_owned());
        }
        match there {
            true => undo.replaced.push(file.name),
            false => undo.created.push(file.name),
        }
    }
    Ok(Done { undo, views })
}

/// Puts the files `undo` names back as `before` has them: those replaced written, those made
/// removed.
fn put_back(dir: &Path, undo: &Undo, before: &[File]) -> Result<(), String> {
    let mut failed = None;
    for name in &undo.replaced {
        if let Some((_, data)) = before.iter().find(|(n, _)| n == name)
            && let Err(err) = write_atomic_bytes(&path_of(dir, name), data)
        {
            failed = Some(format!("Cannot put back {name}: {}", describe(&err)));
        }
    }
    for name in &undo.created {
        match std::fs::remove_file(path_of(dir, name)) {
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
pub fn undo(dir: &Path, undo: &Undo, name: &str, discard: &dyn Fn(&Path)) -> Result<Done, String> {
    let (_, files, _) = read(&undo.backup)?;
    let before: Vec<File> = files.into_iter().map(|(name, _, data)| (name, data)).collect();
    let touched: Vec<Group> = undo.replaced.iter().chain(&undo.created).filter_map(|n| bundle::group_of(n)).collect();
    let (backup, _) = backup(dir, &touched, name, discard)?;
    put_back(dir, undo, &before)?;
    let views = touched.contains(&Group::Views).then(|| {
        before
            .iter()
            .find(|(n, _)| n == "views.toml")
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
            .unwrap_or_default()
    });
    Ok(Done { undo: Undo { backup, replaced: Vec::new(), created: Vec::new() }, views })
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
        assert_eq!(export(&dir, &out, &[Group::Settings, Group::Themes, Group::Templates]), Ok(3));
        assert!(export(&dir, &out, &[Group::Settings]).unwrap_err().contains("there already"));
        let (os, files, left_out) = read(&out).unwrap();
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
            read(&path)
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
        assert!(read(&path).unwrap_err().contains("does not read as a zip"));
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
        export(&source, &zip, &[Group::Settings, Group::Themes, Group::Templates]).unwrap();

        let shown = preview(&dir, &zip).unwrap();
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

        let done = import(&dir, shown.files, "gezik-settings-1.zip", &|_| panic!("nothing to drop")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("settings.toml")).unwrap(), SETTINGS);
        assert_eq!(std::fs::read_to_string(path_of(&dir, "templates/a/b.txt")).unwrap(), "b");
        assert_eq!(done.undo.replaced, ["settings.toml", "themes/nord.toml"]);
        assert_eq!(done.undo.created, ["themes/frost.toml", "templates/a/b.txt"]);
        let (_, backed_up, _) = read(&done.undo.backup).unwrap();
        let backed_up: Vec<&str> = backed_up.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(backed_up, ["settings.toml", "themes/mine.toml", "themes/nord.toml"], "as they were");

        undo(&dir, &done.undo, "gezik-settings-2.zip", &|_| {}).unwrap();
        assert_eq!(std::fs::read(dir.join("settings.toml")).unwrap(), old_settings.as_bytes());
        assert_eq!(std::fs::read_to_string(dir.join("themes/nord.toml")).unwrap(), "old nord");
        assert!(!dir.join("themes/frost.toml").exists() && !path_of(&dir, "templates/a/b.txt").exists());
        assert_eq!(backups(&dir), ["gezik-settings-2.zip", "gezik-settings-1.zip"]);
    }

    #[test]
    fn eleven_backups_leave_ten() {
        let dir = scratch("rotate");
        put(&dir, "settings.toml", SETTINGS);
        for i in 0..11 {
            backup(&dir, &[], &format!("gezik-settings-{i:02}.zip"), &|old| std::fs::remove_file(old).unwrap())
                .unwrap();
        }
        let kept = backups(&dir);
        assert_eq!(
            (kept.len(), kept[0].as_str(), kept[9].as_str()),
            (10, "gezik-settings-10.zip", "gezik-settings-01.zip")
        );
        assert!(!dir.join("backups/gezik-settings-00.zip").exists());
    }
}
