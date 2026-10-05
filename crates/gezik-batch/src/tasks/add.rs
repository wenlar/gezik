//! Adding files to an existing zip, 7z or tar archive. The archive is written anew under a
//! temporary name (zip: the entries there copied as they are; 7z and tar: unpacked into a
//! staging folder and packed again), then the old one goes to the trash and the new one takes
//! its name, so undo brings the old one back.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use gezik_core::batch::archive::{Codec, Format, detect, volume_set};
use gezik_core::ops::names::next_free;
use gezik_ops::{
    Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after,
};
use zip::ZipArchive;
use zip::result::ZipError;

use super::{Writing, cancelled, file_name, what, workers};
use crate::archive::write::{self, CompressOptions, Input, InputKind, Level, OutFormat};
use crate::archive::{self, ExtractCx};

/// The archives Gezik can add to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    SevenZ,
    Tar(Codec),
}

pub struct AddToArchiveTask {
    archive: PathBuf,
    sources: Vec<PathBuf>,
    /// The archive's password if known; asked for when it is needed and missing or wrong.
    password: Option<String>,
}

impl AddToArchiveTask {
    /// Adds `sources` (named relative to their common folder) to `archive`. In an encrypted
    /// archive the new entries are encrypted with its password.
    pub fn new(archive: PathBuf, sources: Vec<PathBuf>, password: Option<String>) -> AddToArchiveTask {
        // The archive cannot go into itself.
        let sources = sources.into_iter().filter(|s| *s != archive).collect();
        AddToArchiveTask { archive, sources, password }
    }

    /// Writes the zip with the new entries to `temp`; false: nothing to add.
    fn add_zip(&self, mut fresh: Vec<Input>, temp: &Path, run: &RunCx<'_>) -> io::Result<bool> {
        let mut zip = ZipArchive::new(BufReader::new(File::open(&self.archive)?))?;
        let mut names = Names::default();
        // The smallest encrypted entry: it is read through to check a password.
        let mut encrypted: Option<(u64, usize)> = None;
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i)?;
            let name = key(entry.name().trim_end_matches('/'));
            if entry.is_dir() {
                names.dirs.insert(name);
            } else {
                if entry.encrypted() && encrypted.is_none_or(|(size, _)| entry.compressed_size() < size) {
                    encrypted = Some((entry.compressed_size(), i));
                }
                names.files.insert(name);
            }
        }
        let password = match encrypted {
            Some((_, i)) => match self.zip_password(&mut zip, i, run)? {
                Some(password) => Some(password),
                None => return Ok(false),
            },
            None => None,
        };
        let Some(replaced) = settle(&mut fresh, &names, run)? else { return Ok(false) };
        if fresh.is_empty() {
            return Ok(false);
        }
        run.found(fresh.len() as u64, fresh.iter().map(Input::size).sum());
        let keep = |name: &str| !covered(&key(name.trim_end_matches('/')), &replaced);
        let cx = Writing::new(run);
        write::rewrite_zip(&mut zip, &keep, &fresh, temp, password.as_deref(), workers(), &cx)?;
        if cx.failed.borrow().len() >= fresh.len() {
            // Nothing new could be read: the archive stays as it is.
            let _ = std::fs::remove_file(temp);
            return Ok(false);
        }
        Ok(true)
    }

    /// The password that opens entry `i`: the one given, else asked for until it fits.
    /// `None`: the user gave none.
    fn zip_password(
        &self,
        zip: &mut ZipArchive<BufReader<File>>,
        i: usize,
        run: &RunCx<'_>,
    ) -> io::Result<Option<String>> {
        let mut given = self.password.clone();
        let mut retry = false;
        loop {
            let password = match given.take() {
                Some(password) => password,
                None => match run.ask(Question::Password { archive: self.archive.clone(), retry }) {
                    Answer::Text(password) => password,
                    _ if run.stopped() => return Err(cancelled()),
                    _ => {
                        run.fail(&self.archive, &no_password());
                        return Ok(None);
                    }
                },
            };
            if opens(zip, i, &password)? {
                return Ok(Some(password));
            }
            retry = true;
        }
    }

    /// Unpacks the 7z or tar into a staging folder and packs it again with the new items into
    /// `temp`; false: nothing to add.
    fn repack(&self, kind: Kind, mut fresh: Vec<Input>, temp: &Path, run: &RunCx<'_>) -> io::Result<bool> {
        let names_encrypted = kind == Kind::SevenZ
            && matches!(sevenz_rust2::Archive::open(&self.archive), Err(sevenz_rust2::Error::PasswordRequired));
        let stage = run.staging_dir(&self.archive)?;
        let content = stage.join("x");
        std::fs::create_dir(&content)?;
        let unpack = Unpack {
            run,
            archive: &self.archive,
            given: RefCell::new(self.password.clone()),
            used: RefCell::new(None),
            declined: Cell::new(false),
            failed: Cell::new(false),
        };
        archive::open(&self.archive)?.extract(&content, &unpack)?;
        if unpack.declined.get() {
            return Ok(false);
        }
        if unpack.failed.get() {
            // Packing what could be read would lose the rest.
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Some items of the archive could not be read; nothing was added",
            ));
        }
        let mut children: Vec<PathBuf> =
            std::fs::read_dir(&content)?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
        children.sort();
        let mut old = write::collect(&children, false, &mut |path, err| run.fail(path, &err));
        let mut names = Names::default();
        for input in &old {
            let set = if input.kind == InputKind::Dir { &mut names.dirs } else { &mut names.files };
            set.insert(key(&input.name));
        }
        let Some(replaced) = settle(&mut fresh, &names, run)? else { return Ok(false) };
        if fresh.is_empty() {
            return Ok(false);
        }
        let added = fresh.len();
        old.retain(|input| !covered(&key(&input.name), &replaced));
        old.extend(fresh);
        run.found(old.len() as u64, old.iter().map(Input::size).sum());
        let cx = Writing::new(run);
        let password = unpack.used.into_inner();
        match kind {
            Kind::Tar(codec) => write::write_tar(&old, temp, codec, Level::Normal, &cx)?,
            _ => {
                let options = CompressOptions {
                    format: OutFormat::SevenZ,
                    level: Level::Normal,
                    password,
                    encrypt_names: names_encrypted,
                    split: None,
                };
                write::write(&old, temp, &options, workers(), &cx)?
            }
        };
        let failed = cx.failed.into_inner();
        if failed.iter().any(|path| path.starts_with(&content)) {
            let _ = std::fs::remove_file(temp);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Some items of the archive could not be packed again; nothing was added",
            ));
        }
        if failed.len() >= added {
            let _ = std::fs::remove_file(temp);
            return Ok(false);
        }
        Ok(true)
    }

    /// The old archive to the trash, the new one (at `temp`) in its place.
    fn replace(&self, temp: &Path, run: &RunCx<'_>) -> io::Result<Outcome> {
        let archive = &self.archive;
        let mut outcomes = Vec::new();
        match gezik_ops::trash_path(archive) {
            Ok(trashed) => outcomes.push(Outcome::Trashed { original: archive.clone(), trashed }),
            // Deleted for good instead (too large for the trash): the new one still goes there.
            Err(_) if std::fs::symlink_metadata(archive).is_err() => {
                outcomes.push(Outcome::Deleted { path: archive.clone() });
            }
            Err(err) => {
                let _ = std::fs::remove_file(temp);
                return Err(err);
            }
        }
        if let Err(err) = gezik_platform::fs::move_entry(temp, archive) {
            // The old one is in the trash (undo brings it back); the new one is kept beside it.
            let folder = archive.parent().unwrap_or(Path::new(""));
            let free = next_free(&file_name(archive), false, |name| folder.join(name).symlink_metadata().is_ok());
            let kept = folder.join(free);
            match gezik_platform::fs::move_entry(temp, &kept) {
                Ok(()) => {
                    let message = format!("{err}; the new archive was kept as {}", file_name(&kept));
                    run.fail(archive, &io::Error::new(err.kind(), message));
                    outcomes.push(Outcome::Created { facts: facts_after(&kept, false), path: kept, from: None });
                }
                Err(_) => {
                    let _ = std::fs::remove_file(temp);
                    run.fail(archive, &err);
                }
            }
            return Ok(Outcome::Several(outcomes));
        }
        outcomes.push(Outcome::Created { path: archive.clone(), facts: facts_after(archive, false), from: None });
        Ok(Outcome::Several(outcomes))
    }
}

impl Task for AddToArchiveTask {
    fn kind(&self) -> TaskKind {
        TaskKind::AddToArchive
    }

    fn title(&self) -> String {
        format!("Adding {} to {}", what(&self.sources), file_name(&self.archive))
    }

    fn count(&self) -> usize {
        self.sources.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = vec![self.archive.clone()];
        paths.extend(self.sources.iter().cloned());
        Resources { paths, work: Work::Cpu }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        match std::fs::metadata(&self.archive) {
            Ok(meta) => {
                let facts = Facts { is_dir: false, size: meta.len(), modified: meta.modified().ok() };
                // The entries count instead, once known.
                sink.item(PlanItem::new(Stage::Parallel, facts).source(&self.archive).top(0).uncounted());
            }
            Err(err) => sink.failed(&self.archive, err),
        }
    }

    fn run(&self, _item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        // Undo needs the old archive in the trash.
        if !run.has_trash(&self.archive) {
            let bin = if cfg!(windows) { "a Recycle Bin" } else { "a trash" };
            return Err(io::Error::new(io::ErrorKind::Unsupported, format!("Adding needs {bin} to undo")));
        }
        let kind = kind_of(&self.archive)?;
        let fresh = write::inputs(&self.sources, &mut |path, err| run.fail(path, &err));
        let temp = run.temp_file_for(&self.archive);
        let written = match kind {
            Kind::Zip => self.add_zip(fresh, &temp, run)?,
            _ => self.repack(kind, fresh, &temp, run)?,
        };
        if !written {
            return Ok(Outcome::Nothing);
        }
        self.replace(&temp, run)
    }
}

/// What `archive` is, if Gezik can add to it.
fn kind_of(archive: &Path) -> io::Result<Kind> {
    let name = file_name(archive);
    if volume_set(&name).is_some() {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "Cannot add to an archive in parts"));
    }
    let mut head = Vec::new();
    File::open(archive)?.take(0x9010).read_to_end(&mut head)?;
    match detect(&head, &name) {
        Some(Format::Zip) => Ok(Kind::Zip),
        Some(Format::SevenZ) => Ok(Kind::SevenZ),
        Some(Format::Tar(codec)) if codec != Codec::Zst => Ok(Kind::Tar(codec)),
        _ => Err(io::Error::new(io::ErrorKind::Unsupported, "Only zip, 7z and tar archives can be added to")),
    }
}

/// Whether `password` opens zip entry `i`, read to its end: its AES check value or ZipCrypto
/// byte lets some wrong passwords through, the HMAC or CRC at the end catches them.
fn opens(zip: &mut ZipArchive<BufReader<File>>, i: usize, password: &str) -> io::Result<bool> {
    match zip.by_index_decrypt(i, password.as_bytes()) {
        Ok(mut entry) => Ok(io::copy(&mut entry, &mut io::sink()).is_ok()),
        Err(ZipError::InvalidPassword) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn no_password() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "no password")
}

/// The names an archive has, as `key`s.
#[derive(Default)]
struct Names {
    files: HashSet<String>,
    dirs: HashSet<String>,
}

/// A name as names are compared: case folded where the file system ignores case.
fn key(name: &str) -> String {
    if cfg!(any(windows, target_os = "macos")) { name.to_lowercase() } else { name.to_owned() }
}

/// Whether `name` (a key) is one of `roots` or inside one.
fn covered(name: &str, roots: &[String]) -> bool {
    roots.iter().any(|root| name == root || name.strip_prefix(root.as_str()).is_some_and(|rest| rest.starts_with('/')))
}

/// Settles the new items whose names the archive has already, with one question for all of
/// them (Replace / Skip / Keep both). A folder already there merges; a file against a folder
/// (or the other way) clashes, and the folder goes or stays with all it holds. Gives the
/// archive's names to leave out (Replace), as keys; `None`: the user said Cancel.
fn settle(fresh: &mut Vec<Input>, names: &Names, run: &RunCx<'_>) -> io::Result<Option<Vec<String>>> {
    fresh.retain(|input| input.kind != InputKind::Dir || !names.dirs.contains(&key(&input.name)));
    let clashes: Vec<(String, bool)> = fresh
        .iter()
        .filter(|input| {
            let k = key(&input.name);
            names.files.contains(&k) || (input.kind != InputKind::Dir && names.dirs.contains(&k))
        })
        .map(|input| (input.name.clone(), input.kind == InputKind::Dir))
        .collect();
    if clashes.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let title = match clashes.len() {
        1 => "1 item already in the archive".to_owned(),
        n => format!("{n} items already in the archive"),
    };
    let mut message = clashes.iter().take(10).map(|(name, _)| name.clone()).collect::<Vec<_>>().join("\n");
    if clashes.len() > 10 {
        message.push_str(&format!("\n…and {} more", clashes.len() - 10));
    }
    let buttons = ["Replace", "Skip", "Keep both", "Cancel"].map(str::to_owned).to_vec();
    let roots: Vec<String> = clashes.iter().map(|(name, _)| key(name)).collect();
    match run.ask(Question::Confirm { title, message, buttons }) {
        Answer::Button(0) => Ok(Some(roots)),
        Answer::Button(1) => {
            fresh.retain(|input| !covered(&key(&input.name), &roots));
            Ok(Some(Vec::new()))
        }
        Answer::Button(2) => {
            let mut taken: HashSet<String> =
                names.files.iter().chain(&names.dirs).cloned().chain(fresh.iter().map(|i| key(&i.name))).collect();
            for (name, is_dir) in clashes {
                let (folder, last) = match name.rsplit_once('/') {
                    Some((folder, last)) => (format!("{folder}/"), last.to_owned()),
                    None => (String::new(), name.clone()),
                };
                let free = next_free(&last, is_dir, |candidate| taken.contains(&key(&format!("{folder}{candidate}"))));
                let renamed = format!("{folder}{free}");
                taken.insert(key(&renamed));
                // The item and, for a folder, all it holds.
                let root = [key(&name)];
                for input in fresh.iter_mut().filter(|input| covered(&key(&input.name), &root)) {
                    input.name = format!("{renamed}{}", &input.name[name.len()..]);
                }
            }
            Ok(Some(Vec::new()))
        }
        _ if run.stopped() => Err(cancelled()),
        _ => Ok(None),
    }
}

/// Unpacking for a repack: the password given is tried first, failures are reported and
/// remembered (nothing is then added).
struct Unpack<'a, 'r> {
    run: &'a RunCx<'r>,
    archive: &'a Path,
    given: RefCell<Option<String>>,
    /// The password that opened it (the new archive gets it too).
    used: RefCell<Option<String>>,
    declined: Cell<bool>,
    failed: Cell<bool>,
}

impl ExtractCx for Unpack<'_, '_> {
    fn add_bytes(&self, _n: u64) {}

    fn entry_done(&self) {}

    fn stopped(&self) -> bool {
        self.run.stopped()
    }

    fn password(&self, retry: bool) -> Option<String> {
        let given = if retry { None } else { self.given.borrow_mut().take() };
        let password =
            given.or_else(|| match self.run.ask(Question::Password { archive: self.archive.to_path_buf(), retry }) {
                Answer::Text(password) => Some(password),
                _ => None,
            });
        match &password {
            Some(password) => *self.used.borrow_mut() = Some(password.clone()),
            None => self.declined.set(true),
        }
        password
    }

    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.failed.set(true);
        let path = if name == file_name(self.archive) { self.archive.to_path_buf() } else { self.archive.join(name) };
        self.run.fail(&path, error);
    }
}
