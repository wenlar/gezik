//! Checksums as an engine job (spec 11 §4.3): the drive's queue, pause, cancel and the panel's
//! progress, but nothing to undo (the job changes nothing, so it never reaches the history).
//! The rows go to a list the Checksums layer shares; its items carry no paths, so the engine
//! neither reloads their folders nor selects them.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gezik_core::checksum_file::{self, Algo};
use gezik_core::ops::conflict::Facts;

use crate::engine::lock;
use crate::hash::hash_file;
use crate::task::{Answer, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

/// A checksum file larger than this is not read (its lines would not fit the layer anyway).
pub const MAX_CHECKSUM_FILE: u64 = 64 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    Waiting,
    Done,
    /// It could not be read: why.
    Unreadable(String),
    /// Verify: the checksum file names it, but it is not there.
    Missing,
    /// Its data is in the cloud and the user chose not to download it.
    OnlineOnly,
}

/// What a verified row came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Waiting,
    Ok,
    Failed,
    Missing,
    Unreadable,
    OnlineOnly,
}

impl Verdict {
    /// As `sha256sum -c` says it.
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Waiting => "",
            Verdict::Ok => "OK",
            Verdict::Failed => "FAILED",
            Verdict::Missing => "MISSING",
            Verdict::Unreadable => "UNREADABLE",
            Verdict::OnlineOnly => "ONLINE ONLY",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HashRow {
    pub path: PathBuf,
    /// What the layer shows and the files write: relative to the base folder, with `/` (verify:
    /// as the checksum file writes it).
    pub name: String,
    pub size: u64,
    /// Lowercase hex, by `Algo::index`.
    pub digests: [Option<String>; 5],
    /// Verify: the value the checksum file gives, and the size a `.crc` gives.
    pub expected: Option<(Algo, String)>,
    pub expected_size: Option<u64>,
    pub state: RowState,
    /// Its data is in the cloud (reading it downloads it).
    pub online: bool,
}

impl HashRow {
    fn new(path: PathBuf, name: String, size: u64) -> HashRow {
        HashRow {
            path,
            name,
            size,
            digests: Default::default(),
            expected: None,
            expected_size: None,
            state: RowState::Waiting,
            online: false,
        }
    }

    pub fn digest(&self, algo: Algo) -> Option<&str> {
        self.digests[algo.index()].as_deref()
    }

    /// Verify: whether its value (and a `.crc`'s size) is what the checksum file says.
    pub fn verdict(&self) -> Verdict {
        match &self.state {
            RowState::Waiting => Verdict::Waiting,
            RowState::Missing => Verdict::Missing,
            RowState::Unreadable(_) => Verdict::Unreadable,
            RowState::OnlineOnly => Verdict::OnlineOnly,
            RowState::Done => {
                let Some((algo, hex)) = &self.expected else { return Verdict::Ok };
                let size_ok = self.expected_size.is_none_or(|size| size == self.size);
                if size_ok && self.digest(*algo) == Some(hex.as_str()) { Verdict::Ok } else { Verdict::Failed }
            }
        }
    }
}

/// What the job finds, shared with the layer.
#[derive(Debug, Default)]
pub struct HashResults {
    pub rows: Vec<HashRow>,
    /// Rows added or changed since the layer last took them (it takes them away).
    pub dirty: Vec<usize>,
    /// Verify: lines of the checksum file that could not be read.
    pub bad_lines: usize,
    /// Verify: why the checksum file itself could not be read.
    pub error: Option<String>,
    /// Whether online-only files are downloaded: asked once for the layer (every job of it).
    pub download: Option<bool>,
}

impl HashResults {
    fn push(&mut self, row: HashRow) -> usize {
        self.rows.push(row);
        let index = self.rows.len() - 1;
        self.dirty.push(index);
        index
    }
}

pub type SharedResults = Arc<Mutex<HashResults>>;

enum HashWork {
    /// The files of `roots` (folders walked), named relative to `base`; hidden items as the
    /// view shows them (`[view] show-hidden`, `show-system`).
    Items { roots: Vec<PathBuf>, base: PathBuf, shown: (bool, bool) },
    /// The rows listed already, for the algorithms they lack.
    Again,
    /// The files a checksum file names.
    Verify { file: PathBuf },
}

/// Whether a file's data is in the cloud: the system's word (`said`) unless a test says.
type OnlineCheck = fn(&Path, bool) -> bool;

pub struct HashTask {
    work: HashWork,
    algos: Vec<Algo>,
    results: SharedResults,
    online: AtomicUsize,
    online_check: OnlineCheck,
}

impl HashTask {
    fn new(work: HashWork, algos: Vec<Algo>, results: SharedResults) -> HashTask {
        HashTask { work, algos, results, online: AtomicUsize::new(0), online_check: |_, said| said }
    }

    /// `algos` of the files of `roots`, folders walked; names relative to `base`.
    pub fn items(
        roots: Vec<PathBuf>,
        base: PathBuf,
        shown: (bool, bool),
        algos: Vec<Algo>,
        results: SharedResults,
    ) -> HashTask {
        HashTask::new(HashWork::Items { roots, base, shown }, algos, results)
    }

    /// The rows already in `results`, for those of `algos` they lack (a box ticked later).
    pub fn again(algos: Vec<Algo>, results: SharedResults) -> HashTask {
        HashTask::new(HashWork::Again, algos, results)
    }

    /// The files checksum file `file` names, each against its value.
    pub fn verify(file: PathBuf, results: SharedResults) -> HashTask {
        HashTask::new(HashWork::Verify { file }, Vec::new(), results)
    }

    #[cfg(test)]
    fn online_check(mut self, check: OnlineCheck) -> HashTask {
        self.online_check = check;
        self
    }

    /// Adds `row` and plans it (online-only files after the rest, when the count is known).
    fn plan_row(&self, sink: &mut dyn ScanSink, mut row: HashRow, online: bool) -> bool {
        row.online = (self.online_check)(&row.path, online);
        if row.online {
            self.online.fetch_add(1, Ordering::Relaxed);
        }
        let stage = if row.online { Stage::After } else { Stage::Parallel };
        let size = row.size;
        let index = lock(&self.results).push(row);
        sink.item(PlanItem::new(stage, Facts { size, ..Facts::default() }).under(index))
    }

    fn plan_items(&self, sink: &mut dyn ScanSink, roots: &[PathBuf], base: &Path, shown: (bool, bool)) {
        let mut files: Vec<(PathBuf, u64, bool)> = Vec::new();
        for root in roots {
            // A chosen link is followed (it was chosen); links inside folders are not. Only
            // regular files are read: never a device or a pipe (`CON`, `/dev/zero`, a FIFO).
            match std::fs::metadata(root) {
                Ok(meta) if meta.is_dir() => walk(root, shown, sink, &mut files),
                Ok(meta) if meta.is_file() => files.push((root.clone(), meta.len(), is_online(&meta))),
                Ok(_) => sink.failed(root, not_a_file()),
                Err(err) => sink.failed(root, err),
            }
            if sink.cancelled() {
                return;
            }
        }
        for (path, size, online) in files {
            let name = checksum_file::relative_slash(&path, base);
            if !self.plan_row(sink, HashRow::new(path, name, size), online) {
                return;
            }
        }
    }

    fn plan_again(&self, sink: &mut dyn ScanSink) {
        let wanted: Vec<(usize, u64, bool)> = {
            let mut results = lock(&self.results);
            let mut wanted = Vec::new();
            for (i, row) in results.rows.iter_mut().enumerate() {
                let lacks = self.algos.iter().any(|a| row.digest(*a).is_none());
                if lacks && row.state != RowState::Missing {
                    row.state = RowState::Waiting;
                    wanted.push((i, row.size, row.online));
                }
            }
            results.dirty.extend(wanted.iter().map(|(i, ..)| *i));
            wanted
        };
        for (index, size, online) in wanted {
            if online {
                self.online.fetch_add(1, Ordering::Relaxed);
            }
            let stage = if online { Stage::After } else { Stage::Parallel };
            if !sink.item(PlanItem::new(stage, Facts { size, ..Facts::default() }).under(index)) {
                return;
            }
        }
    }

    fn plan_verify(&self, sink: &mut dyn ScanSink, file: &Path) {
        let read = std::fs::metadata(file).and_then(|meta| {
            if meta.len() > MAX_CHECKSUM_FILE {
                Err(io::Error::new(io::ErrorKind::InvalidData, "The checksum file is too large"))
            } else {
                std::fs::read(file)
            }
        });
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(err) => {
                lock(&self.results).error = Some(err.to_string());
                return sink.failed(file, err);
            }
        };
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let parsed = checksum_file::parse_file(&name, &bytes);
        lock(&self.results).bad_lines = parsed.bad;
        let base = file.parent().unwrap_or(Path::new(""));
        for entry in parsed.entries {
            // Nothing outside the checksum file's folder is touched (spec 11 §4.4).
            let Some(path) = checksum_file::resolve(base, &entry.path) else {
                let mut row = HashRow::new(PathBuf::new(), entry.path, 0);
                row.expected = Some((entry.algo, entry.hex));
                row.state = RowState::Unreadable("Outside the folder".to_owned());
                lock(&self.results).push(row);
                continue;
            };
            let mut row = HashRow::new(path.clone(), entry.path, 0);
            row.expected = Some((entry.algo, entry.hex));
            row.expected_size = entry.size;
            let why = match std::fs::metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    row.size = meta.len();
                    if !self.plan_row(sink, row, is_online(&meta)) {
                        return;
                    }
                    continue;
                }
                Ok(meta) if meta.is_dir() => RowState::Unreadable("It is a folder".to_owned()),
                Ok(_) => RowState::Unreadable(not_a_file().to_string()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => RowState::Missing,
                Err(err) => RowState::Unreadable(err.to_string()),
            };
            row.state = why;
            lock(&self.results).push(row);
        }
    }

    /// Whether online-only files are read: asked once for the layer, when the first comes up
    /// (the online-only rows run one by one after the rest, so one asks at a time).
    fn downloads(&self, cx: &RunCx<'_>) -> bool {
        if let Some(yes) = lock(&self.results).download {
            return yes;
        }
        let count = self.online.load(Ordering::Relaxed);
        let files = if count == 1 { "1 file is".to_owned() } else { format!("{count} files are") };
        let question = Question::Confirm {
            title: "Online-only files".to_owned(),
            message: format!("{files} online-only. Download them to compute checksums?"),
            buttons: vec!["Download".to_owned(), "Skip".to_owned()],
        };
        let yes = cx.ask(question) == Answer::Button(0);
        lock(&self.results).download = Some(yes);
        yes
    }

    fn set_row(&self, index: usize, change: impl FnOnce(&mut HashRow)) {
        let mut results = lock(&self.results);
        if let Some(row) = results.rows.get_mut(index) {
            change(row);
            results.dirty.push(index);
        }
    }
}

/// Whether the system says `meta`'s data is in the cloud.
fn is_online(meta: &std::fs::Metadata) -> bool {
    gezik_core::attribute_flags(meta) & gezik_core::Entry::CLOUD_ONLY != 0
}

fn not_a_file() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Not a file (a device or a pipe)")
}

/// Every file under `dir` (not links; hidden ones as `shown` says), folders read in name order.
fn walk(dir: &Path, shown: (bool, bool), sink: &mut dyn ScanSink, files: &mut Vec<(PathBuf, u64, bool)>) {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        if sink.cancelled() {
            return;
        }
        let mut items = match gezik_platform::fs::read_dir_items(&folder, &|_, is_dir| !is_dir) {
            Ok(items) => items,
            Err(err) => {
                sink.failed(&folder, err);
                continue;
            }
        };
        items.sort_by(|a, b| a.name.cmp(&b.name));
        let mut folders = Vec::new();
        for item in items {
            if item.is_link || !gezik_core::is_shown_name(&item.name, item.flags, shown.0, shown.1) {
                continue;
            }
            let path = folder.join(&item.name);
            if item.is_dir {
                folders.push(path);
            } else if item.is_file {
                files.push((path, item.size, item.offline));
            }
        }
        // Depth first, the first folder next.
        pending.extend(folders.into_iter().rev());
    }
}

impl Task for HashTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Checksum
    }

    fn title(&self) -> String {
        match &self.work {
            HashWork::Items { roots, .. } => format!("Computing checksums of {}", super::what(roots)),
            HashWork::Again => "Computing checksums".to_owned(),
            HashWork::Verify { file } => format!("Verifying {}", super::name(file)),
        }
    }

    fn count(&self) -> usize {
        match &self.work {
            HashWork::Items { roots, .. } => roots.len(),
            _ => 1,
        }
    }

    fn resources(&self) -> Resources {
        let paths = match &self.work {
            HashWork::Items { roots, .. } => roots.clone(),
            HashWork::Again => lock(&self.results).rows.first().map(|row| row.path.clone()).into_iter().collect(),
            HashWork::Verify { file } => vec![file.clone()],
        };
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        match &self.work {
            HashWork::Items { roots, base, shown } => self.plan_items(sink, roots, base, *shown),
            HashWork::Again => self.plan_again(sink),
            HashWork::Verify { file } => self.plan_verify(sink, file),
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let index = item.root;
        let Some((path, online, algos)) = ({
            let results = lock(&self.results);
            results.rows.get(index).map(|row| {
                let algos: Vec<Algo> = match &row.expected {
                    Some((algo, _)) => vec![*algo],
                    None => self.algos.iter().copied().filter(|a| row.digest(*a).is_none()).collect(),
                };
                (row.path.clone(), row.online, algos)
            })
        }) else {
            return Ok(Outcome::Nothing);
        };
        if online && !self.downloads(cx) {
            self.set_row(index, |row| row.state = RowState::OnlineOnly);
            return Ok(Outcome::Nothing);
        }
        let read = hash_file(&path, &algos, &mut |bytes| {
            cx.add_bytes(bytes);
            !cx.stopped()
        });
        match read {
            Ok(digests) => self.set_row(index, |row| {
                for algo in &algos {
                    row.digests[algo.index()] = digests.get(*algo).map(str::to_owned);
                }
                row.state = RowState::Done;
            }),
            // Cancelled: the row stays as it was; the engine ends the job.
            Err(err) if err.kind() == io::ErrorKind::Interrupted && cx.cancelled() => return Err(err),
            Err(err) => self.set_row(index, |row| row.state = RowState::Unreadable(err.to_string())),
        }
        Ok(Outcome::Nothing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Event;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const MD5_ABC: &str = "900150983cd24fb0d6963f7d28e17f72";

    fn shared_results() -> SharedResults {
        Arc::new(Mutex::new(HashResults::default()))
    }

    fn names(results: &SharedResults) -> Vec<String> {
        lock(results).rows.iter().map(|row| row.name.clone()).collect()
    }

    #[test]
    fn the_files_of_the_chosen_items_with_relative_names_and_nothing_to_undo() {
        let dir = test_dir("hash-items");
        write(&dir.join("a.txt"), "abc");
        write(&dir.join("sub/b.txt"), "abc");
        write(&dir.join("sub/deep/c.txt"), "");
        write(&dir.join("sub/.hidden"), "x");
        let engine = engine();
        let shared = shared_results();
        let roots = vec![dir.join("a.txt"), dir.join("sub")];
        let task = HashTask::items(roots, dir.clone(), (false, false), vec![Algo::Sha256, Algo::Md5], shared.clone());
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(report.results.is_empty() && report.changed_dirs.is_empty(), "nothing is selected or reloaded");
        assert_eq!(names(&shared), ["a.txt", "sub/b.txt", "sub/deep/c.txt"]);
        let results = lock(&shared);
        assert!(results.rows.iter().all(|row| row.state == RowState::Done));
        assert_eq!(results.rows[1].digest(Algo::Sha256), Some(SHA256_ABC));
        assert_eq!(results.rows[1].digest(Algo::Md5), Some(MD5_ABC));
        assert_eq!(results.rows[1].digest(Algo::Sha1), None);
        assert_eq!(results.rows[0].size, 3);
        drop(results);
        assert_eq!(engine.undo_label(), None, "a checksum job never reaches the history");
        let shown = shared_results();
        let task = HashTask::items(vec![dir.join("sub")], dir.clone(), (true, false), vec![Algo::Crc32], shown.clone());
        finish(&engine, engine.submit(Box::new(task)), defaults);
        assert_eq!(names(&shown), ["sub/.hidden", "sub/b.txt", "sub/deep/c.txt"], "hidden ones as the view shows them");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_box_ticked_later_reads_only_the_rows_that_lack_it() {
        let dir = test_dir("hash-again");
        write(&dir.join("a.txt"), "abc");
        write(&dir.join("b.txt"), "abc");
        let engine = engine();
        let shared = shared_results();
        let roots = vec![dir.join("a.txt"), dir.join("b.txt")];
        let task = HashTask::items(roots, dir.clone(), (false, false), vec![Algo::Sha256], shared.clone());
        finish(&engine, engine.submit(Box::new(task)), defaults);
        // One row already has MD5 (as if computed earlier): only the other is read.
        lock(&shared).rows[0].digests[Algo::Md5.index()] = Some("kept".into());
        lock(&shared).dirty.clear();
        let again = HashTask::again(vec![Algo::Sha256, Algo::Md5], shared.clone());
        finish(&engine, engine.submit(Box::new(again)), defaults);
        let results = lock(&shared);
        assert_eq!(results.rows[0].digest(Algo::Md5), Some("kept"));
        assert_eq!(results.rows[1].digest(Algo::Md5), Some(MD5_ABC));
        assert_eq!(results.dirty, [1, 1], "only the other row was planned and read");
        drop(results);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_says_ok_failed_missing_and_counts_broken_lines() {
        let dir = test_dir("hash-verify");
        write(&dir.join("ok.txt"), "abc");
        write(&dir.join("bad.txt"), "abd");
        write(&dir.join("sub/up.txt"), "abc");
        let sums = format!(
            "{SHA256_ABC} *ok.txt\n{SHA256_ABC}  bad.txt\n{SHA256_ABC}  gone.txt\nnonsense\nMD5 (sub/up.txt) = {MD5_ABC}\n"
        );
        write(&dir.join("SHA256SUMS"), &sums);
        let engine = engine();
        let shared = shared_results();
        finish(&engine, engine.submit(Box::new(HashTask::verify(dir.join("SHA256SUMS"), shared.clone()))), defaults);
        let results = lock(&shared);
        let verdicts: Vec<(&str, Verdict)> = results.rows.iter().map(|r| (r.name.as_str(), r.verdict())).collect();
        assert_eq!(
            verdicts,
            [
                ("ok.txt", Verdict::Ok),
                ("bad.txt", Verdict::Failed),
                ("gone.txt", Verdict::Missing),
                ("sub/up.txt", Verdict::Ok)
            ]
        );
        assert_eq!(results.bad_lines, 1);
        drop(results);
        // Total Commander's .crc: the size counts too.
        write(&dir.join("x.bin"), "123456789");
        write(&dir.join("x.crc"), "filename=x.bin\nsize=10\ncrc32=CBF43926\n");
        let crc = shared_results();
        finish(&engine, engine.submit(Box::new(HashTask::verify(dir.join("x.crc"), crc.clone()))), defaults);
        assert_eq!(lock(&crc).rows[0].verdict(), Verdict::Failed, "the right CRC, the wrong size");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn online_only_files_are_asked_about_once_and_skipped() {
        let dir = test_dir("hash-online");
        for name in ["a.cloud", "b.cloud", "c.txt"] {
            write(&dir.join(name), "abc");
        }
        let engine = engine();
        let shared = shared_results();
        let task = HashTask::items(vec![dir.clone()], dir.clone(), (false, false), vec![Algo::Sha256], shared.clone())
            .online_check(|path, _| path.extension().is_some_and(|e| e == "cloud"));
        let asked = skipping(&engine, engine.submit(Box::new(task)));
        assert_eq!(asked, ["2 files are online-only. Download them to compute checksums?"]);
        let states =
            |shared: &SharedResults| -> Vec<RowState> { lock(shared).rows.iter().map(|r| r.state.clone()).collect() };
        assert_eq!(states(&shared), [RowState::OnlineOnly, RowState::OnlineOnly, RowState::Done]);
        // A box ticked later: the layer's answer stands, nothing is asked again.
        let again = HashTask::again(vec![Algo::Sha256, Algo::Md5], shared.clone());
        assert!(skipping(&engine, engine.submit(Box::new(again))).is_empty());
        assert_eq!(states(&shared), [RowState::OnlineOnly, RowState::OnlineOnly, RowState::Done]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runs `job` to its end, answering Skip to every question; the questions asked.
    fn skipping(engine: &crate::Engine, job: crate::JobId) -> Vec<String> {
        let mut asked = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            for event in engine.drain() {
                match event {
                    Event::Question { id, question: Question::Confirm { message, .. }, .. } => {
                        asked.push(message);
                        engine.answer(job, id, Answer::Button(1));
                    }
                    Event::Finished { job: done, .. } if done == job => return asked,
                    _ => {}
                }
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn verify_touches_nothing_outside_the_files_folder() {
        let dir = test_dir("hash-outside");
        write(&dir.join("in/sums.md5"), "");
        write(&dir.join("secret.txt"), "abc");
        let mut lines = format!("{MD5_ABC}  ../secret.txt\n{MD5_ABC}  {}\n", dir.join("secret.txt").display());
        if cfg!(windows) {
            lines.push_str(&format!("{MD5_ABC}  \\\\127.0.0.1\\c$\\x.txt\n{MD5_ABC}  NUL\n{MD5_ABC}  CON\n"));
        }
        write(&dir.join("in/sums.md5"), &lines);
        let engine = engine();
        let shared = shared_results();
        let (report, _) = finish(
            &engine,
            engine.submit(Box::new(HashTask::verify(dir.join("in/sums.md5"), shared.clone()))),
            defaults,
        );
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let results = lock(&shared);
        assert_eq!(results.rows.len(), if cfg!(windows) { 5 } else { 2 });
        for row in &results.rows {
            // A device name (`NUL`, `CON`) is a device or not found: never read.
            assert!(matches!(row.verdict(), Verdict::Unreadable | Verdict::Missing), "{}", row.name);
            assert!(row.digest(Algo::Md5).is_none(), "{} was read", row.name);
        }
        assert_eq!(results.rows[0].state, RowState::Unreadable("Outside the folder".into()));
        assert_eq!(results.rows[1].state, RowState::Unreadable("Outside the folder".into()));
        drop(results);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_chosen_device_is_not_read() {
        let device = PathBuf::from(if cfg!(windows) { "\\\\.\\NUL" } else { "/dev/zero" });
        let engine = engine();
        let shared = shared_results();
        let task =
            HashTask::items(vec![device.clone()], PathBuf::new(), (false, false), vec![Algo::Crc32], shared.clone());
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert!(lock(&shared).rows.is_empty());
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    }

    #[test]
    fn an_unreadable_file_says_why_in_its_row() {
        let dir = test_dir("hash-unreadable");
        let shared = shared_results();
        lock(&shared).push(HashRow::new(dir.join("gone.txt"), "gone.txt".into(), 3));
        let engine = engine();
        let (report, _) =
            finish(&engine, engine.submit(Box::new(HashTask::again(vec![Algo::Md5], shared.clone()))), defaults);
        assert!(report.failures.is_empty());
        assert!(matches!(lock(&shared).rows[0].state, RowState::Unreadable(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
