//! Changing an existing zip, 7z or tar archive (spec 16 §6): adding to it, deleting and
//! renaming in it, making folders. The archive is written anew under a temporary name (zip:
//! the entries kept copied as they are; 7z and tar: unpacked into a staging folder and packed
//! again, with each entry's Unix mode, time and owner as the archive had them), read back and
//! checked, then the old one steps aside, the new one takes its name and the old one goes to
//! the trash, so undo brings the old one back. Whatever fails or is cancelled before that
//! leaves the old archive as it was and no temporary file.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use gezik_core::batch::archive::{Codec, Format, detect, volume_set};
use gezik_core::format_size;
use gezik_core::ops::names::{NameRules, next_free, validate_name};
use gezik_core::vfs::Edit;
use gezik_ops::{
    Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after,
};
use zip::ZipArchive;
use zip::result::ZipError;

use super::compress::keep_aside;
use super::{Writing, cancelled, file_name, what, workers};
use crate::archive::write::{self, CompressOptions, Input, InputKind, Level, OutFormat};
use crate::archive::{self, ExtractCx, Header};

/// The archives Gezik can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    SevenZ,
    Tar(Codec),
}

/// A step made to fail, for tests (spec 16 §14 16b): whatever fails, the archive stays as it
/// was and no temporary file is left.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Before anything is written (the space check).
    Space,
    /// While the new archive is written.
    Write,
    /// The new archive is spoiled after writing: the check must catch it.
    Spoil,
    /// The new archive cannot take the old one's name.
    Replace,
    /// The archive changes on disk after it was read, before it is replaced.
    Touch,
    /// The drive has no trash.
    NoTrash,
}

pub struct AddToArchiveTask {
    archive: PathBuf,
    edits: Vec<Edit>,
    /// The archive's password if known; asked for when it is needed and missing or wrong.
    password: Option<String>,
    fault: Option<Fault>,
}

impl AddToArchiveTask {
    /// Adds `sources` (each under its own name, at the top) to `archive`. In an encrypted
    /// archive the new entries are encrypted with its password.
    pub fn new(archive: PathBuf, sources: Vec<PathBuf>, password: Option<String>) -> AddToArchiveTask {
        let edits = sources.into_iter().map(|from| Edit::Put { to: file_name(&from), from }).collect();
        AddToArchiveTask::edit(archive, edits, password)
    }

    /// Applies `edits` to `archive` as one rewrite.
    pub fn edit(archive: PathBuf, edits: Vec<Edit>, password: Option<String>) -> AddToArchiveTask {
        // The archive cannot go into itself.
        let edits =
            edits.into_iter().filter(|edit| !matches!(edit, Edit::Put { from, .. } if *from == archive)).collect();
        AddToArchiveTask { archive, edits, password, fault: None }
    }

    #[doc(hidden)]
    pub fn with_fault(mut self, fault: Fault) -> AddToArchiveTask {
        self.fault = Some(fault);
        self
    }
}

impl Task for AddToArchiveTask {
    fn kind(&self) -> TaskKind {
        TaskKind::AddToArchive
    }

    fn title(&self) -> String {
        let sources = put_sources(&self.edits);
        if sources.is_empty() {
            // TRANSLATORS: {} is an archive's name
            return gezik_i18n::t!("Changing {}", file_name(&self.archive));
        }
        // TRANSLATORS: {0} is a name or "3 items", {1} an archive
        gezik_i18n::t!("Adding {0} to {1}", what(&sources), file_name(&self.archive))
    }

    fn count(&self) -> usize {
        self.edits.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = vec![self.archive.clone()];
        paths.extend(put_sources(&self.edits));
        Resources { paths, work: Work::Cpu, vfs_roots: Vec::new() }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        plan_archive(&self.archive, sink);
    }

    fn run(&self, _item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        let archive = real_archive(&self.archive)?;
        let trash = run.has_trash(&archive) && self.fault != Some(Fault::NoTrash);
        if !trash && !confirm_no_trash(run)? {
            return Ok(Outcome::Nothing);
        }
        if self.fault == Some(Fault::Space) {
            return Err(io::Error::other("injected"));
        }
        check_space(&archive, &self.edits)?;
        let temp = run.temp_file_for(&archive);
        let Some(done) = rewrite(&archive, &self.edits, self.password.clone(), &temp, run, self.fault)? else {
            return Ok(Outcome::Nothing);
        };
        replace(&archive, &temp, trash, done.seen, run, self.fault)
    }
}

/// The one plan item of a task that rewrites `archive` (the entries count instead, once known).
pub fn plan_archive(archive: &Path, sink: &mut dyn ScanSink) {
    match std::fs::metadata(archive) {
        Ok(meta) => {
            let facts = Facts { is_dir: false, size: meta.len(), modified: meta.modified().ok() };
            sink.item(PlanItem::new(Stage::Parallel, facts).source(archive).top(0).uncounted());
        }
        Err(err) => sink.failed(archive, err),
    }
}

/// The files and folders `edits` put in.
pub fn put_sources(edits: &[Edit]) -> Vec<PathBuf> {
    edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Put { from, .. } => Some(from.clone()),
            _ => None,
        })
        .collect()
}

/// The file a rewrite changes: a symbolic link's target. Refused when it is read-only, or has
/// other names (hard links: they would keep the old contents while this one changed).
pub fn real_archive(path: &Path) -> io::Result<PathBuf> {
    let real = if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        plain(std::fs::canonicalize(path)?)
    } else {
        path.to_path_buf()
    };
    let meta = std::fs::metadata(&real)?;
    if meta.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            gezik_i18n::t!("The archive is read-only; it was not changed"),
        ));
    }
    if gezik_platform::fs::link_count(&real)? > 1 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            gezik_i18n::t!("The archive has other names (hard links); it was not changed"),
        ));
    }
    Ok(real)
}

/// A canonical path as Windows users write it (`C:\a`, `\\server\share`), not `\\?\`.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

/// A new folder of this user only (0700 on Unix) in the system's temporary folder, removed
/// with what is in it when dropped: what a rewrite unpacks (decrypted) never lands in the
/// archive's folder, which may be synced or shared. Named as gezik-vfs's areas, so one a crash
/// leaves is cleared at the next start.
struct Private(PathBuf);

impl Private {
    fn new() -> io::Result<Private> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("gezik-vfs-{}-edit{n}", std::process::id()));
            #[cfg_attr(not(unix), allow(unused_mut))]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            match builder.create(&path) {
                Ok(()) => return Ok(Private(path)),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err),
            }
        }
    }
}

impl Drop for Private {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// On a drive without a trash the old archive cannot be kept for undo: asks whether to go on.
pub fn confirm_no_trash(run: &RunCx<'_>) -> io::Result<bool> {
    let title = gezik_i18n::t!("The old archive cannot be kept for undo").into_owned();
    let message = if cfg!(windows) {
        gezik_i18n::t!("This drive has no Recycle Bin. Change the archive anyway?")
    } else {
        gezik_i18n::t!("This drive has no trash. Change the archive anyway?")
    }
    .into_owned();
    let buttons = vec![gezik_i18n::t!("Change").into_owned(), gezik_i18n::t!("Cancel").into_owned()];
    match run.ask(Question::Confirm { title, message, buttons }) {
        Answer::Button(0) => Ok(true),
        _ if run.stopped() => Err(cancelled()),
        _ => {
            run.cancel_job();
            Ok(false)
        }
    }
}

/// Step 1: room next to `archive` for the new one (its size and what is put in).
pub fn check_space(archive: &Path, edits: &[Edit]) -> io::Result<()> {
    let mut needed = std::fs::metadata(archive)?.len();
    for from in put_sources(edits) {
        needed = needed.saturating_add(tree_size(&from));
    }
    // Next to the archive (the new one), and in the temporary folder (what is unpacked and read
    // back: at least as much again; a solid 7z or tar.gz unpacks to more).
    room(archive.parent().unwrap_or(Path::new(".")), needed)?;
    room(&std::env::temp_dir(), needed)
}

fn room(folder: &Path, needed: u64) -> io::Result<()> {
    if let Ok(free) = gezik_platform::fs::free_space(folder)
        && needed > free
    {
        let drive = gezik_platform::fs::drive_root(folder).unwrap_or_else(|| folder.to_path_buf());
        // TRANSLATORS: {0} and {1} are sizes, {2} a drive
        let message =
            gezik_i18n::t!("{0} needed, {1} free on {2}", format_size(needed), format_size(free), drive.display());
        return Err(io::Error::new(io::ErrorKind::StorageFull, message));
    }
    Ok(())
}

/// The bytes of the files at `path` and under it.
fn tree_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path).map_or(0, |entries| entries.flatten().map(|entry| tree_size(&entry.path())).sum())
}

/// What a rewrite did.
#[derive(Debug, Default)]
pub struct Rewritten {
    /// The files and folders put in whole (a move trashes these, and only these).
    pub sources: Vec<PathBuf>,
    /// The archive's size and change time when it was read: `replace` refuses if it changed.
    pub seen: (u64, Option<SystemTime>),
}

/// A file's size and change time.
fn facts(path: &Path) -> io::Result<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path)?;
    Ok((meta.len(), meta.modified().ok()))
}

/// Steps 2 and 3: writes `archive` with `edits` applied to `out`, then reads it back and checks
/// it. `None`: nothing to change (all skipped, a question cancelled). On any error or `None`,
/// `out` is gone.
pub fn rewrite(
    archive: &Path,
    edits: &[Edit],
    password: Option<String>,
    out: &Path,
    run: &RunCx<'_>,
    fault: Option<Fault>,
) -> io::Result<Option<Rewritten>> {
    let result = Rewrite { archive, edits, password, out, run, fault }.run();
    if !matches!(result, Ok(Some(_))) {
        let _ = std::fs::remove_file(out);
    }
    result
}

/// Step 4: the new archive (at `temp`) in the old one's place: the old one goes to the trash
/// (`trash`: the drive has one) and the new one takes its name; else the old one steps aside
/// and is deleted once the new one is in. If the new one cannot land, the old one comes back.
pub fn replace(
    archive: &Path,
    temp: &Path,
    trash: bool,
    seen: (u64, Option<SystemTime>),
    run: &RunCx<'_>,
    fault: Option<Fault>,
) -> io::Result<Outcome> {
    if fault == Some(Fault::Touch) {
        std::fs::OpenOptions::new().append(true).open(archive).and_then(|mut f| io::Write::write_all(&mut f, b"!"))?;
    }
    // Changed by another program since it was read: what was written would lose that change.
    if facts(archive).ok() != Some(seen) {
        let _ = std::fs::remove_file(temp);
        return Err(io::Error::other(gezik_i18n::t!("The archive changed while it was written; it was not changed")));
    }
    // The new file keeps what the old one had besides its contents (permissions, owner, tags,
    // the Mark of the Web, the creation time); and Gezik's tags kept in its own store.
    if let Err(err) = gezik_platform::fs::carry_facts(archive, temp) {
        let _ = std::fs::remove_file(temp);
        return Err(err);
    }
    let tags = run.meta().and_then(|store| store.read(archive).ok()).filter(|tags| !tags.is_empty());
    let landing = |temp: &Path| {
        if fault == Some(Fault::Replace) {
            Err(io::Error::other("injected"))
        } else {
            gezik_platform::fs::move_entry(temp, archive)
        }
    };
    let mut outcomes = Vec::new();
    if trash {
        // Trashed from its own place under its own name, so the bin shows it as it was and Put
        // Back returns it there; if the new one cannot land, it comes back from the trash.
        let trashed = match gezik_ops::trash_path(archive) {
            Ok(trashed) => trashed,
            Err(err) => {
                let _ = std::fs::remove_file(temp);
                return Err(err);
            }
        };
        if let Err(err) = landing(temp) {
            let _ = std::fs::remove_file(temp);
            if let Err(back) = gezik_platform::fs::restore(&trashed, archive) {
                // TRANSLATORS: {0} is why it could not be put back, {1} where it is
                let message = gezik_i18n::t!("{0}; it is at {1}", back, trashed.display());
                run.fail(archive, &io::Error::new(back.kind(), message));
            }
            return Err(err);
        }
        outcomes.push(Outcome::Trashed { original: archive.to_path_buf(), trashed });
    } else {
        // No trash: the old one steps aside under a noted name (if Gezik stops meanwhile, the
        // next start puts it back) and is deleted once the new one is in its place.
        let aside = run.aside_name(archive);
        run.note_aside(&[(aside.clone(), archive.to_path_buf())]);
        if let Err(err) = gezik_platform::fs::move_entry(archive, &aside) {
            run.forget_aside(&[&aside]);
            let _ = std::fs::remove_file(temp);
            return Err(err);
        }
        if let Err(err) = landing(temp) {
            let _ = std::fs::remove_file(temp);
            match gezik_platform::fs::move_entry(&aside, archive) {
                Ok(()) => run.forget_aside(&[&aside]),
                Err(back) => {
                    // TRANSLATORS: {0} is why it could not be put back, {1} where it is
                    let message = gezik_i18n::t!("{0}; it is at {1}", back, aside.display());
                    run.fail(archive, &io::Error::new(back.kind(), message));
                }
            }
            return Err(err);
        }
        match gezik_platform::fs::delete(&aside) {
            Ok(()) => {
                outcomes.push(Outcome::Deleted { path: archive.to_path_buf() });
                run.forget_aside(&[&aside]);
            }
            Err(err) => keep_aside(archive, &aside, &err, run),
        }
    }
    if let (Some(store), Some(tags)) = (run.meta(), tags)
        && store.read(archive).ok().as_ref() != Some(&tags)
    {
        let _ = store.write(archive, &tags);
    }
    outcomes.push(Outcome::Created { path: archive.to_path_buf(), facts: facts_after(archive, false), from: None });
    Ok(Outcome::Several(outcomes))
}

struct Rewrite<'a, 'r> {
    archive: &'a Path,
    edits: &'a [Edit],
    password: Option<String>,
    out: &'a Path,
    run: &'a RunCx<'r>,
    fault: Option<Fault>,
}

impl Rewrite<'_, '_> {
    fn run(&self) -> io::Result<Option<Rewritten>> {
        for edit in self.edits {
            check_edit(edit)?;
        }
        let kind = kind_of(self.archive)?;
        let seen = facts(self.archive)?;
        // What is put in, every name (a move trashes the folder: nothing may stay behind), and
        // what could not be listed or opened.
        let mut fresh = Vec::new();
        let unread = RefCell::new(Vec::new());
        for edit in self.edits {
            if let Edit::Put { from, to } = edit {
                let mut got = write::collect(std::slice::from_ref(from), false, &mut |path, err| {
                    self.run.fail(path, &err);
                    unread.borrow_mut().push(path.to_path_buf());
                });
                if got.is_empty() {
                    unread.borrow_mut().push(from.clone());
                }
                let top = got.first().map(|input| input.name.len()).unwrap_or_default();
                for input in &mut got {
                    input.name = format!("{to}{}", &input.name[top..]);
                }
                fresh.extend(got);
            }
        }
        // Never the archive itself, nor what is being written.
        fresh.retain(|input| input.path != self.archive && input.path != self.out);
        for input in &fresh {
            if matches!(input.kind, InputKind::File { .. })
                && let Err(err) = File::open(&input.path)
            {
                self.run.fail(&input.path, &err);
                unread.borrow_mut().push(input.path.clone());
            }
        }
        let unread = unread.into_inner();
        fresh.retain(|input| !unread.contains(&input.path));
        let done = match kind {
            Kind::Zip => self.zip(fresh, &unread)?,
            _ => self.repack(kind, fresh, &unread)?,
        };
        Ok(done.map(|sources| Rewritten { sources, seen }))
    }

    /// A put that replaces an entry, or comes with a delete (Update in Archive, an archive
    /// written back into its own), must go in whole: else what it replaces would be lost.
    fn guard(&self, failed: &[PathBuf], replaced: &[String]) -> io::Result<()> {
        let risky = !replaced.is_empty() || self.edits.iter().any(|edit| matches!(edit, Edit::Delete(_)));
        let lost = put_sources(self.edits).iter().any(|from| failed.iter().any(|path| path.starts_with(from)));
        if risky && lost {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                gezik_i18n::t!("Some items to put in could not be read; the archive was not changed"),
            ));
        }
        Ok(())
    }

    fn fault(&self, fault: Fault) -> io::Result<()> {
        if self.fault == Some(fault) { Err(io::Error::other("injected")) } else { Ok(()) }
    }

    /// The zip with the edits: the entries kept are copied as they are (renamed ones too).
    fn zip(&self, mut fresh: Vec<Input>, unread: &[PathBuf]) -> io::Result<Option<Vec<PathBuf>>> {
        let mut zip = ZipArchive::new(BufReader::new(File::open(self.archive)?))?;
        // (name, is a folder) of each entry, and the smallest encrypted one: it is read
        // through to check a password.
        let mut old = Vec::new();
        let mut encrypted: Option<(u64, usize)> = None;
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i)?;
            if entry.encrypted() && !entry.is_dir() && encrypted.is_none_or(|(size, _)| entry.compressed_size() < size)
            {
                encrypted = Some((entry.compressed_size(), i));
            }
            old.push((entry.name().to_owned(), entry.is_dir()));
        }
        let password = match encrypted {
            Some((_, i)) => match self.zip_password(&mut zip, i)? {
                Some(password) => Some(password),
                None => return Ok(None),
            },
            None => None,
        };
        let changes = Changes::new(self.edits, old.iter().map(|(name, is_dir)| (trimmed(name), *is_dir)))?;
        let Some(replaced) = settle(&mut fresh, &changes.names, self.run)? else { return Ok(None) };
        self.guard(unread, &replaced)?;
        if fresh.is_empty() && changes.none() {
            return Ok(None);
        }
        let putting = fresh.clone();
        fresh.extend(changes.new_dirs());
        self.run.found(fresh.len() as u64, fresh.iter().map(Input::size).sum());
        let keep = |name: &str| {
            changes.map(name).filter(|new| !covered(&key(new), &replaced)).map(|new| {
                // A folder keeps its ending slash.
                if name.ends_with('/') { format!("{new}/") } else { new }
            })
        };
        let cx = Writing::new(self.run);
        self.fault(Fault::Write)?;
        write::rewrite_zip(&mut zip, &keep, &fresh, self.out, password.as_deref(), workers(), &cx)?;
        restore_flags(self.archive, &mut zip, &keep, self.out)?;
        let mut failed = cx.failed.into_inner();
        self.guard(&failed, &replaced)?;
        if changes.none() && failed.len() >= putting.len() {
            // Nothing new could be read: the archive stays as it is.
            return Ok(None);
        }
        let mut expected = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i)?;
            if let Some(name) = keep(entry.name()) {
                expected.push(Expected::new(&name, entry.is_dir(), Some(entry.size())));
            }
        }
        let written: Vec<&Input> = fresh.iter().filter(|input| went_in(input, &failed, false)).collect();
        expected.extend(written.iter().map(|input| Expected::of(input)));
        // Entries copied as they are kept their checksums; only what is new is read through.
        let checked: Vec<&Input> =
            written.into_iter().filter(|input| matches!(input.kind, InputKind::File { .. })).collect();
        self.spoil()?;
        // Entries copied as they are must be as they were: checksum, method, encryption, sizes.
        if !kept_as_they_were(&mut zip, &keep, self.out)? {
            return Err(not_read_back());
        }
        verify(self.out, self.archive, expected, &checked, password, self.run)?;
        failed.extend_from_slice(unread);
        Ok(Some(whole(self.edits, &putting, &failed)))
    }

    /// The password that opens entry `i`: the one given, else asked for until it fits.
    /// `None`: the user gave none.
    fn zip_password(&self, zip: &mut ZipArchive<BufReader<File>>, i: usize) -> io::Result<Option<String>> {
        let mut given = self.password.clone();
        let mut retry = false;
        loop {
            let password = match given.take() {
                Some(password) => password,
                None => match self.run.ask(Question::Password { archive: self.archive.to_path_buf(), retry }) {
                    Answer::Text(password) => password,
                    _ if self.run.stopped() => return Err(cancelled()),
                    _ => {
                        self.run.fail(self.archive, &no_password());
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

    /// Unpacks the 7z or tar into a staging folder and packs it again with the edits.
    fn repack(&self, kind: Kind, mut fresh: Vec<Input>, unread: &[PathBuf]) -> io::Result<Option<Vec<PathBuf>>> {
        let names_encrypted = kind == Kind::SevenZ
            && matches!(sevenz_rust2::Archive::open(self.archive), Err(sevenz_rust2::Error::PasswordRequired));
        let stage = Private::new()?;
        self.repack_in(kind, &mut fresh, unread, names_encrypted, &stage.0)
    }

    fn repack_in(
        &self,
        kind: Kind,
        fresh: &mut Vec<Input>,
        unread: &[PathBuf],
        names_encrypted: bool,
        stage: &Path,
    ) -> io::Result<Option<Vec<PathBuf>>> {
        let content = stage.join("x");
        std::fs::create_dir(&content)?;
        let unpack = Unpack {
            run: self.run,
            archive: self.archive,
            given: RefCell::new(self.password.clone()),
            used: RefCell::new(None),
            declined: Cell::new(false),
            failed: Cell::new(false),
            headers: RefCell::new(HashMap::new()),
        };
        archive::open(self.archive)?.extract(&content, &unpack)?;
        if unpack.declined.get() {
            return Ok(None);
        }
        if unpack.failed.get() {
            // Packing what could be read would lose the rest.
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                gezik_i18n::t!("Some items of the archive could not be read; nothing was changed"),
            ));
        }
        let mut children: Vec<PathBuf> =
            std::fs::read_dir(&content)?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
        children.sort();
        let mut old = write::collect(&children, false, &mut |path, err| self.run.fail(path, &err));
        // What the disk could not keep (a Unix mode on Windows, the owner) comes from the archive.
        let headers = unpack.headers.take();
        for input in &mut old {
            if let Some(header) = headers.get(&input.path) {
                if let Some(mode) = header.mode {
                    input.mode = mode;
                    input.keep_mode = true;
                }
                input.modified = header.modified.or(input.modified);
                input.owner = header.owner.clone();
            }
        }
        let changes =
            Changes::new(self.edits, old.iter().map(|input| (input.name.as_str(), input.kind == InputKind::Dir)))?;
        let Some(replaced) = settle(fresh, &changes.names, self.run)? else { return Ok(None) };
        self.guard(unread, &replaced)?;
        if fresh.is_empty() && changes.none() {
            return Ok(None);
        }
        old.retain_mut(|input| match changes.map(&input.name) {
            Some(new) if covered(&key(&new), &replaced) => false,
            Some(new) => {
                input.name = new;
                true
            }
            None => false,
        });
        let putting = std::mem::take(fresh);
        old.extend(changes.new_dirs());
        old.extend(putting.iter().cloned());
        self.run.found(old.len() as u64, old.iter().map(Input::size).sum());
        let cx = Writing::new(self.run);
        let password = unpack.used.into_inner();
        self.fault(Fault::Write)?;
        match kind {
            Kind::Tar(codec) => {
                write::write_tar(&old, self.out, codec, Level::Normal, &cx)?;
            }
            _ => {
                let options = CompressOptions {
                    format: OutFormat::SevenZ,
                    level: Level::Normal,
                    password: password.clone(),
                    encrypt_names: names_encrypted,
                    split: None,
                };
                write::write(&old, self.out, &options, workers(), &cx)?;
            }
        };
        let mut failed = cx.failed.into_inner();
        self.guard(&failed, &replaced)?;
        if failed.iter().any(|path| path.starts_with(&content)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                gezik_i18n::t!("Some items of the archive could not be packed again; nothing was changed"),
            ));
        }
        if changes.none() && failed.len() >= putting.len() {
            return Ok(None);
        }
        let tar = matches!(kind, Kind::Tar(_));
        let written: Vec<&Input> = old.iter().filter(|input| went_in(input, &failed, tar)).collect();
        let expected = written.iter().map(|input| Expected::of(input)).collect();
        // Every entry was packed again: each is read back.
        let checked: Vec<&Input> =
            written.into_iter().filter(|input| matches!(input.kind, InputKind::File { .. })).collect();
        self.spoil()?;
        verify(self.out, self.archive, expected, &checked, password, self.run)?;
        failed.extend_from_slice(unread);
        Ok(Some(whole(self.edits, &putting, &failed)))
    }

    /// The test hook that spoils what was written (one byte in the middle).
    fn spoil(&self) -> io::Result<()> {
        if self.fault != Some(Fault::Spoil) {
            return Ok(());
        }
        let mut bytes = std::fs::read(self.out)?;
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0x5A;
        std::fs::write(self.out, bytes)
    }
}

/// An entry the new archive must have.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Expected {
    name: String,
    is_dir: bool,
    /// A file's size, when known.
    size: Option<u64>,
}

impl Expected {
    fn new(name: &str, is_dir: bool, size: Option<u64>) -> Expected {
        Expected { name: trimmed(&name.replace('\\', "/")).to_owned(), is_dir, size: size.filter(|_| !is_dir) }
    }

    fn of(input: &Input) -> Expected {
        let size = match input.kind {
            InputKind::File { size } => Some(size),
            _ => None,
        };
        Expected::new(&input.name, input.kind == InputKind::Dir, size)
    }
}

/// Whether `input` is in the archive written: not failed, and no link where links are left out.
fn went_in(input: &Input, failed: &[PathBuf], links: bool) -> bool {
    !failed.contains(&input.path) && (links || !matches!(input.kind, InputKind::Link { .. }))
}

/// The Put sources that went in whole: their top was not left out, nothing under them failed.
fn whole(edits: &[Edit], putting: &[Input], failed: &[PathBuf]) -> Vec<PathBuf> {
    put_sources(edits)
        .into_iter()
        .filter(|from| putting.iter().any(|input| input.path == *from))
        .filter(|from| !failed.iter().any(|path| path.starts_with(from)))
        .collect()
}

/// Step 3: the archive at `out` (of `archive`'s kind) is read back: its entries must be
/// `expected` (names, folders, sizes), and each of `checked` must come out as its source's
/// bytes (read through, so its checksum is checked too). A zip's entries copied as they are
/// are not read; a repacked archive's are all checked.
fn verify(
    out: &Path,
    archive: &Path,
    mut expected: Vec<Expected>,
    checked: &[&Input],
    password: Option<String>,
    run: &RunCx<'_>,
) -> io::Result<()> {
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            gezik_i18n::t!("The new archive did not read back as written; the archive was not changed"),
        )
    };
    let name = file_name(archive);
    let cx = Check { run, password, failed: Cell::new(false) };
    let listed = match archive::open_as(out, &name).and_then(|mut source| archive::list_all(source.as_mut(), &cx)) {
        Ok(listed) => listed,
        Err(_) if run.stopped() => return Err(cancelled()),
        Err(_) => return Err(bad()),
    };
    let mut got: Vec<Expected> =
        listed.iter().map(|entry| Expected::new(&entry.name, entry.is_dir, entry.size)).collect();
    expected.sort_by(|a, b| (&a.name, a.is_dir).cmp(&(&b.name, b.is_dir)));
    got.sort_by(|a, b| (&a.name, a.is_dir).cmp(&(&b.name, b.is_dir)));
    let same = |a: &Expected, b: &Expected| {
        a.name == b.name && a.is_dir == b.is_dir && (a.size.is_none() || b.size.is_none() || a.size == b.size)
    };
    if cx.failed.get() || got.len() != expected.len() || !got.iter().zip(&expected).all(|(a, b)| same(a, b)) {
        return Err(bad());
    }
    if checked.is_empty() {
        return Ok(());
    }
    let private = Private::new()?;
    let dir = private.0.clone();
    let result = (|| {
        let names: Vec<String> = checked.iter().map(|input| input.name.clone()).collect();
        archive::open_as(out, &name)?.extract_some(&names, &dir, &cx)?;
        for input in checked {
            let back = input.name.split('/').fold(dir.clone(), |at, part| at.join(part));
            if !same_bytes(&back, &input.path)? {
                return Err(bad());
            }
        }
        if cx.failed.get() { Err(bad()) } else { Ok(()) }
    })();
    drop(private);
    match result {
        Err(_) if run.stopped() => Err(cancelled()),
        Err(_) => Err(bad()),
        ok => ok,
    }
}

/// The error when the new archive does not read back as written.
fn not_read_back() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        gezik_i18n::t!("The new archive did not read back as written; the archive was not changed"),
    )
}

/// zip's writer rebuilds the headers of the entries it copies raw, without ZipCrypto's
/// encryption flag (bit 0) or the data-descriptor flag (bit 3) its password check reads: both
/// go back into the local and the central header of each copied entry, as the old archive had
/// them. (AES entries keep theirs through the writer.)
fn restore_flags<R: Read + Seek>(
    archive: &Path,
    zip: &mut ZipArchive<R>,
    keep: &dyn Fn(&str) -> Option<String>,
    out: &Path,
) -> io::Result<()> {
    let mut source = File::open(archive)?;
    let mut wanted: HashMap<String, u16> = HashMap::new();
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i)?;
        let Some(name) = keep(entry.name()) else { continue };
        let flags = u16_at(&mut source, entry.central_header_start() + 8)?;
        if flags & 1 != 0 {
            wanted.insert(name, flags & 0x0009);
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let mut places = Vec::new();
    {
        let mut new = ZipArchive::new(BufReader::new(File::open(out)?))?;
        for i in 0..new.len() {
            let entry = new.by_index_raw(i)?;
            if let Some(bits) = wanted.get(entry.name()) {
                places.push((entry.header_start() + 6, *bits));
                places.push((entry.central_header_start() + 8, *bits));
            }
        }
    }
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(out)?;
    for (at, bits) in places {
        let flags = u16_at(&mut file, at)?;
        file.seek(SeekFrom::Start(at))?;
        file.write_all(&(flags | bits).to_le_bytes())?;
    }
    file.flush()
}

fn u16_at(file: &mut File, at: u64) -> io::Result<u16> {
    let mut bytes = [0u8; 2];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

/// Whether each entry of `zip` that `keep` names is in the zip at `out` as it was: its
/// checksum, method, encryption and sizes (entries of one name twice are not compared).
fn kept_as_they_were<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    keep: &dyn Fn(&str) -> Option<String>,
    out: &Path,
) -> io::Result<bool> {
    let Ok(mut new) = ZipArchive::new(BufReader::new(File::open(out)?)) else { return Ok(false) };
    let mut seen = HashMap::new();
    for i in 0..zip.len() {
        if let Some(name) = keep(zip.by_index_raw(i)?.name()) {
            *seen.entry(name).or_insert(0) += 1;
        }
    }
    for i in 0..zip.len() {
        let old = zip.by_index_raw(i)?;
        let Some(name) = keep(old.name()) else { continue };
        if seen.get(&name) != Some(&1) {
            continue;
        }
        let Some(at) = new.index_for_name(&name) else { return Ok(false) };
        let was = entry_facts(&old);
        drop(old);
        let now = entry_facts(&new.by_index_raw(at)?);
        if was != now {
            return Ok(false);
        }
    }
    Ok(true)
}

/// What `kept_as_they_were` compares.
fn entry_facts<R: Read>(e: &zip::read::ZipFile<'_, R>) -> (u32, zip::CompressionMethod, bool, u64, u64, bool) {
    (e.crc32(), e.compression(), e.encrypted(), e.compressed_size(), e.size(), e.is_dir())
}

/// Whether the files `a` and `b` hold the same bytes.
fn same_bytes(a: &Path, b: &Path) -> io::Result<bool> {
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (BufReader::new(File::open(a)?), BufReader::new(File::open(b)?));
    let (mut x, mut y) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let n = a.read(&mut x)?;
        if n == 0 {
            return Ok(true);
        }
        b.read_exact(&mut y[..n])?;
        if x[..n] != y[..n] {
            return Ok(false);
        }
    }
}

/// Reading back: the password known, any failure noted.
struct Check<'a, 'r> {
    run: &'a RunCx<'r>,
    password: Option<String>,
    failed: Cell<bool>,
}

impl ExtractCx for Check<'_, '_> {
    fn add_bytes(&self, _n: u64) {}
    fn entry_done(&self) {}
    fn stopped(&self) -> bool {
        self.run.stopped()
    }
    fn password(&self, retry: bool) -> Option<String> {
        if retry { None } else { self.password.clone() }
    }
    fn entry_failed(&self, _name: &str, _error: &io::Error) {
        self.failed.set(true);
    }
    fn entry_skipped(&self, _name: &str, _why: &io::Error) {
        self.failed.set(true);
    }
}

/// An edit's names must be plain paths inside the archive: no empty, `.` or `..` part, no
/// leading `/`, no `\`.
fn check_edit(edit: &Edit) -> io::Result<()> {
    let names: Vec<&str> = match edit {
        Edit::Put { to, .. } => vec![to],
        Edit::MakeDir(name) | Edit::Delete(name) => vec![name],
        Edit::Rename { from, to } => vec![from, to],
    };
    for name in names {
        // Every part as extracting would take it (no control character; on Windows no `:`, no
        // device name, no ending dot or space) and as a name on this system.
        let bad = name.is_empty()
            || name.contains('\\')
            || gezik_core::batch::archive::safe_join(Path::new("x"), name).is_none()
            || name.split('/').any(|part| {
                part.is_empty()
                    || part.chars().any(char::is_control)
                    || validate_name(part, NameRules::current()).is_err()
                    || (cfg!(windows) && (part.contains(':') || part.ends_with(['.', ' '])))
            });
        if bad {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, gezik_i18n::t!("Not a usable name: {}", name)));
        }
    }
    if let Edit::Rename { from, to } = edit
        && key(from) != key(to)
        && covered(&key(to), &[key(from)])
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, gezik_i18n::t!("Cannot move a folder into itself")));
    }
    Ok(())
}

/// What `kind_of` refuses: the archive can only be read.
pub fn read_only() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, gezik_i18n::t!("This kind of archive can only be read"))
}

/// Whether Gezik can write the archive at `path`: zip, 7z, tar plain, gz, bz2 or xz, whole.
pub fn writable(path: &Path) -> bool {
    kind_of(path).is_ok()
}

/// What `archive` is, if Gezik can write it.
fn kind_of(archive: &Path) -> io::Result<Kind> {
    let name = file_name(archive);
    if volume_set(&name).is_some() {
        return Err(read_only());
    }
    let mut head = Vec::new();
    File::open(archive)?.take(0x9010).read_to_end(&mut head)?;
    match detect(&head, &name) {
        Some(Format::Zip) => Ok(Kind::Zip),
        Some(Format::SevenZ) => Ok(Kind::SevenZ),
        Some(Format::Tar(codec)) if codec != Codec::Zst => Ok(Kind::Tar(codec)),
        _ => Err(read_only()),
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
    io::Error::new(io::ErrorKind::PermissionDenied, gezik_i18n::t!("no password"))
}

/// `name` without a folder's ending slash.
fn trimmed(name: &str) -> &str {
    name.trim_end_matches('/')
}

/// The names an archive has, as `key`s.
#[derive(Default)]
struct Names {
    files: HashSet<String>,
    dirs: HashSet<String>,
    /// Every folder a name is in, whether or not it has an entry of its own.
    parents: HashSet<String>,
}

impl Names {
    fn add(&mut self, name: String, is_dir: bool) {
        let mut at = name.as_str();
        while let Some((parent, _)) = at.rsplit_once('/') {
            if !self.parents.insert(parent.to_owned()) {
                break;
            }
            at = parent;
        }
        if is_dir {
            self.dirs.insert(name)
        } else {
            self.files.insert(name)
        };
    }

    /// Whether `name` (a key) is there: a file, a folder, or a folder only its entries make.
    fn has(&self, name: &str) -> bool {
        self.files.contains(name) || self.dirs.contains(name) || self.parents.contains(name)
    }
}

/// The deletes, renames and new folders of a rewrite, checked against the archive's names.
struct Changes {
    deleted: Vec<String>,
    /// (old name, new name) as given; matched by key.
    renamed: Vec<(String, String)>,
    /// Folders to make: asked for, and those a delete or rename would leave with nothing (a
    /// folder only its entries made would vanish with them).
    dirs: Vec<String>,
    /// The archive's names once deleted and renamed, with the new folders.
    names: Names,
}

impl Changes {
    fn new<'n>(edits: &[Edit], old: impl Iterator<Item = (&'n str, bool)>) -> io::Result<Changes> {
        let old: Vec<(&str, bool)> = old.collect();
        let mut changes =
            Changes { deleted: Vec::new(), renamed: Vec::new(), dirs: Vec::new(), names: Names::default() };
        let mut before = Names::default();
        for (name, is_dir) in &old {
            before.add(key(name), *is_dir);
        }
        let missing =
            |name: &str| io::Error::new(io::ErrorKind::NotFound, gezik_i18n::t!("{} is not in the archive", name));
        let taken = |name: &str| {
            io::Error::new(io::ErrorKind::AlreadyExists, gezik_i18n::t!("{} is already in the archive", name))
        };
        for edit in edits {
            match edit {
                Edit::Delete(name) if before.has(&key(name)) => changes.deleted.push(key(name)),
                Edit::Rename { from, to } if before.has(&key(from)) => {
                    // Onto another entry's name would make two of one name (a case change is fine).
                    if key(from) != key(to) && before.has(&key(to)) {
                        return Err(taken(to));
                    }
                    changes.renamed.push((from.clone(), to.clone()));
                }
                Edit::Delete(name) | Edit::Rename { from: name, .. } => return Err(missing(name)),
                _ => {}
            }
        }
        for (name, is_dir) in &old {
            if let Some(new) = changes.map(name) {
                changes.names.add(key(&new), *is_dir);
            }
        }
        for edit in edits {
            if let Edit::MakeDir(name) = edit {
                if changes.names.has(&key(name)) {
                    return Err(taken(name));
                }
                changes.dirs.push(name.clone());
                changes.names.add(key(name), true);
            }
        }
        // Where puts go: a folder they go into does not vanish.
        let puts: Vec<String> = edits
            .iter()
            .filter_map(|edit| match edit {
                Edit::Put { to, .. } => Some(key(to)),
                _ => None,
            })
            .collect();
        let vacated: Vec<String> =
            changes.deleted.iter().cloned().chain(changes.renamed.iter().map(|(from, _)| key(from))).collect();
        for gone in vacated {
            let Some((parent, _)) = gone.rsplit_once('/') else { continue };
            let filled = puts.iter().any(|to| to.strip_prefix(parent).is_some_and(|rest| rest.starts_with('/')));
            if !filled && !changes.names.has(parent) && !covered(parent, &changes.deleted) {
                // The folder as the archive spells it (part by part, as `map`).
                let depth = parent.split('/').count();
                let spelled = old.iter().find_map(|(name, _)| {
                    let parts: Vec<&str> = name.split('/').take(depth).collect();
                    (parts.len() == depth && key(&parts.join("/")) == parent).then(|| parts.join("/"))
                });
                let spelled = spelled.unwrap_or_else(|| parent.to_owned());
                changes.names.add(key(&spelled), true);
                changes.dirs.push(spelled);
            }
        }
        Ok(changes)
    }

    /// Nothing but puts.
    fn none(&self) -> bool {
        self.deleted.is_empty() && self.renamed.is_empty() && self.dirs.is_empty()
    }

    /// The name entry `name` gets (a folder's ending slash dropped); `None` when it is deleted.
    fn map(&self, name: &str) -> Option<String> {
        let name = trimmed(name);
        if covered(&key(name), &self.deleted) {
            return None;
        }
        // Part by part: a part folded to lower case may not be as long as it was.
        let parts: Vec<&str> = name.split('/').collect();
        for (from, to) in &self.renamed {
            let head: Vec<&str> = from.split('/').collect();
            if parts.len() >= head.len() && parts.iter().zip(&head).all(|(a, b)| key(a) == key(b)) {
                let rest = &parts[head.len()..];
                return Some(if rest.is_empty() { to.clone() } else { format!("{to}/{}", rest.join("/")) });
            }
        }
        Some(name.to_owned())
    }

    /// The folders to make, as inputs (dated now, as a new folder on disk is).
    fn new_dirs(&self) -> impl Iterator<Item = Input> + '_ {
        let now = SystemTime::now();
        self.dirs.iter().map(move |name| Input {
            path: PathBuf::new(),
            name: name.clone(),
            kind: InputKind::Dir,
            modified: Some(now),
            mode: 0o755,
            read_only: false,
            keep_mode: false,
            owner: None,
        })
    }
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
    let is_dir = |k: &String| names.dirs.contains(k) || names.parents.contains(k);
    fresh.retain(|input| input.kind != InputKind::Dir || !is_dir(&key(&input.name)));
    let clashes: Vec<(String, bool)> = fresh
        .iter()
        .filter(|input| {
            let k = key(&input.name);
            names.files.contains(&k) || (input.kind != InputKind::Dir && is_dir(&k))
        })
        .map(|input| (input.name.clone(), input.kind == InputKind::Dir))
        .collect();
    if clashes.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let title = gezik_i18n::tn!("{n} item already in the archive" | "{n} items already in the archive", clashes.len());
    let mut message = clashes.iter().take(10).map(|(name, _)| name.clone()).collect::<Vec<_>>().join("\n");
    if clashes.len() > 10 {
        message.push('\n');
        message.push_str(&gezik_i18n::tn!("…and {n} more" | "…and {n} more", clashes.len() - 10));
    }
    let buttons =
        [gezik_i18n::t!("Replace"), gezik_i18n::t!("Skip"), gezik_i18n::t!("Keep both"), gezik_i18n::t!("Cancel")]
            .map(std::borrow::Cow::into_owned)
            .to_vec();
    let roots: Vec<String> = clashes.iter().map(|(name, _)| key(name)).collect();
    match run.ask(Question::Confirm { title, message, buttons }) {
        Answer::Button(0) => Ok(Some(roots)),
        Answer::Button(1) => {
            fresh.retain(|input| !covered(&key(&input.name), &roots));
            Ok(Some(Vec::new()))
        }
        Answer::Button(2) => {
            let mut taken: HashSet<String> = names
                .files
                .iter()
                .chain(&names.dirs)
                .chain(&names.parents)
                .cloned()
                .chain(fresh.iter().map(|i| key(&i.name)))
                .collect();
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
        // Cancel: the job ends as cancelled, not as done.
        _ => {
            run.cancel_job();
            Ok(None)
        }
    }
}

/// Unpacking for a repack: the password given is tried first, failures are reported and
/// remembered (nothing is then changed).
struct Unpack<'a, 'r> {
    run: &'a RunCx<'r>,
    archive: &'a Path,
    given: RefCell<Option<String>>,
    /// The password that opened it (the new archive gets it too).
    used: RefCell<Option<String>>,
    declined: Cell<bool>,
    failed: Cell<bool>,
    /// Each unpacked entry's header, by where it was written.
    headers: RefCell<HashMap<PathBuf, Header>>,
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

    fn entry_header(&self, path: &Path, header: &Header) {
        self.headers.borrow_mut().insert(path.to_path_buf(), header.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changes(edits: &[Edit], old: &[(&str, bool)]) -> io::Result<Changes> {
        Changes::new(edits, old.iter().copied())
    }

    #[test]
    fn deletes_and_renames_map_names_and_keep_folders() {
        let old = [("docs/a.txt", false), ("docs/sub/b.txt", false), ("top.txt", false)];
        let edits =
            [Edit::Delete("docs/a.txt".into()), Edit::Rename { from: "docs/sub".into(), to: "docs/inner".into() }];
        let c = changes(&edits, &old).unwrap();
        assert_eq!(c.map("docs/a.txt"), None);
        assert_eq!(c.map("docs/sub/b.txt").as_deref(), Some("docs/inner/b.txt"));
        assert_eq!(c.map("docs/sub/").as_deref(), Some("docs/inner"));
        assert_eq!(c.map("top.txt").as_deref(), Some("top.txt"));
        assert!(c.dirs.is_empty(), "docs still holds something");
        // The last file of a folder only its entries made: the folder stays.
        let c = changes(&[Edit::Delete("docs/sub/b.txt".into())], &old).unwrap();
        assert_eq!(c.dirs, ["docs/sub"]);
        let c = changes(&[Edit::Rename { from: "docs/sub/b.txt".into(), to: "b2.txt".into() }], &old).unwrap();
        assert_eq!(c.dirs, ["docs/sub"]);
    }

    #[test]
    fn edits_on_missing_or_taken_names_are_refused() {
        let old = [("a.txt", false), ("dir/", true), ("dir/x", false), ("implied/y", false)];
        let missing = changes(&[Edit::Delete("nope".into())], &old).err().unwrap();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        let taken = changes(&[Edit::Rename { from: "a.txt".into(), to: "dir".into() }], &old).err().unwrap();
        assert_eq!(taken.kind(), io::ErrorKind::AlreadyExists);
        assert!(changes(&[Edit::MakeDir("dir".into())], &old).is_err());
        assert!(changes(&[Edit::MakeDir("implied".into())], &old).is_err(), "a folder only entries make is there");
        assert_eq!(changes(&[Edit::MakeDir("dir/new".into())], &old).unwrap().dirs, ["dir/new"]);
        assert!(changes(&[Edit::Delete("implied".into())], &old).unwrap().map("implied/y").is_none());
        if cfg!(windows) {
            // A case change of one entry is no clash.
            assert!(changes(&[Edit::Rename { from: "a.txt".into(), to: "A.txt".into() }], &old).is_ok());
        }
    }

    #[test]
    fn a_rename_matches_part_by_part_whatever_lower_case_does_to_lengths() {
        // `İ` (2 bytes) folds to `i̇` (3 bytes) where case is ignored.
        let old = [("İ/x.txt", false)];
        let from = if cfg!(any(windows, target_os = "macos")) { "i\u{307}" } else { "İ" };
        let c = changes(&[Edit::Rename { from: from.into(), to: "J".into() }], &old).unwrap();
        assert_eq!(c.map("İ/x.txt").as_deref(), Some("J/x.txt"));
        let c = changes(&[Edit::Delete("İ/x.txt".into())], &old).unwrap();
        assert_eq!(c.dirs, ["İ"], "the folder keeps its spelling");
    }

    #[test]
    fn unsafe_names_are_refused() {
        let hostile: &[&str] = if cfg!(windows) { &["a:b", "con", "NUL.txt", "x.", "x ", "a/com1/b"] } else { &[] };
        for name in hostile.iter().copied().chain(["\u{1}x", "a/\u{7f}"]) {
            assert!(check_edit(&Edit::Put { from: "x".into(), to: name.into() }).is_err(), "{name:?}");
            assert!(check_edit(&Edit::Rename { from: "a".into(), to: name.into() }).is_err(), "{name:?}");
        }
        for name in ["", "../x", "a/../b", "/abs", "a//b", "a\\b", "./a", "a/"] {
            assert!(check_edit(&Edit::MakeDir(name.into())).is_err(), "{name:?}");
        }
        assert!(check_edit(&Edit::Put { from: "x".into(), to: "a/b c.txt".into() }).is_ok());
        assert!(check_edit(&Edit::Rename { from: "a".into(), to: "a/b".into() }).is_err(), "into itself");
        assert!(check_edit(&Edit::Rename { from: "a".into(), to: "ab".into() }).is_ok());
    }
}
