//! A search or flat view running (spec 4.3): the walk matches on its own threads, and one
//! collector numbers the folders and hands the UI a batch every 100 ms or 2,000 items, then
//! a summary. `max-results` stops it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use gezik_core::Entry;
use gezik_core::search::{HiddenRule, Scope, SearchSpec};
use gezik_platform::fs::DirItem;

use crate::content::Found;
use crate::query::{Query, entry_of};
use crate::results::{Batch, ResultSet};
use crate::walk::{Problems, Visit, Walk, WalkRules, WalkStats, threads_for};

pub const BATCH_ITEMS: usize = 2_000;
pub const BATCH_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum Event {
    Batch(Batch),
    /// Found so far, folders read so far.
    Progress {
        found: usize,
        folders: usize,
    },
    Done(Summary),
}

/// How a search ended (the status bar).
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub found: usize,
    pub folders: usize,
    pub skipped: usize,
    pub problems: Problems,
    /// `max-results` stopped it.
    pub limit_reached: bool,
    /// Stopped by the user (Esc, Stop) or by another search.
    pub cancelled: bool,
    pub elapsed: Duration,
    /// The names came from Everything (Task 4).
    pub everything: bool,
}

/// A search that may still run.
#[derive(Clone, Default)]
pub struct Running(Arc<AtomicBool>);

impl Running {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn flag(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

enum Msg {
    Found(String, Vec<(Entry, Option<Found>)>),
    Done(WalkStats),
}

/// Matches each folder's items on the walk's threads.
struct Matcher {
    query: Query,
    sender: Mutex<mpsc::Sender<Msg>>,
    cancel: Arc<AtomicBool>,
}

impl Visit for Matcher {
    fn wants_meta(&self, name: &str, is_dir: bool) -> bool {
        self.query.passes_name(name, is_dir)
    }

    fn folder(&self, dir: &Path, relative: &str, items: Vec<DirItem>) {
        let mut found = Vec::new();
        for item in &items {
            if !self.query.passes(&item.name, item.is_dir, item.size, item.modified) {
                continue;
            }
            let line = match self.query.content() {
                None => None,
                Some(content) => {
                    if self.cancel.load(Ordering::Relaxed) || !content.reads(&item.name, item.size) {
                        continue;
                    }
                    match content.find_in_file(&dir.join(&item.name), &self.cancel, &gezik_platform::decode_ansi) {
                        Ok(Some(line)) => Some(line),
                        _ => continue,
                    }
                }
            };
            found.push((entry_of(item), line));
        }
        if !found.is_empty() && !self.cancel.load(Ordering::Relaxed) {
            let sender = self.sender.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let _ = sender.send(Msg::Found(relative.to_owned(), found));
        }
    }
}

/// Starts `walk` matched by `query`; `sink` gets the batches, the progress and the summary,
/// on the collector's thread.
pub fn start(walk: Walk, query: Query, sink: impl Fn(Event) + Send + 'static) -> Running {
    let running = Running::default();
    let cancel = running.flag();
    let (sender, receiver) = mpsc::channel();
    let read = walk.read.clone();
    let max = query.max_results();
    let matcher = Arc::new(Matcher { query, sender: Mutex::new(sender.clone()), cancel: cancel.clone() });
    let walk_cancel = cancel.clone();
    let _ = std::thread::Builder::new().name("gezik-search-walk".into()).spawn(move || {
        let stats = crate::walk::run(walk, walk_cancel, matcher);
        let _ = sender.send(Msg::Done(stats));
    });
    let _ = std::thread::Builder::new().name("gezik-search-collect".into()).spawn(move || {
        collect(receiver, &cancel, max, &read, &sink);
    });
    running
}

fn collect(
    receiver: mpsc::Receiver<Msg>,
    cancel: &AtomicBool,
    max: usize,
    read: &std::sync::atomic::AtomicUsize,
    sink: &dyn Fn(Event),
) {
    let started = Instant::now();
    let mut folders: HashMap<String, u32> = HashMap::new();
    let mut batch = Batch::default();
    let mut found = 0usize;
    let mut limit_reached = false;
    let mut last = Instant::now();
    let flush = |batch: &mut Batch| {
        if !batch.is_empty() {
            sink(Event::Batch(std::mem::take(batch)));
        }
    };
    loop {
        match receiver.recv_timeout(BATCH_EVERY) {
            Ok(Msg::Found(relative, items)) if !limit_reached => {
                let next = folders.len() as u32;
                let parent = *folders.entry(relative).or_insert_with_key(|relative| {
                    batch.folders.push(relative.as_str().into());
                    next
                });
                for (entry, line) in items {
                    if found >= max {
                        limit_reached = true;
                        cancel.store(true, Ordering::SeqCst);
                        break;
                    }
                    batch.entries.push(entry);
                    batch.parent.push(parent);
                    batch.matches.push(line);
                    found += 1;
                    if batch.entries.len() >= BATCH_ITEMS {
                        flush(&mut batch);
                    }
                }
            }
            Ok(Msg::Found(..)) => {}
            Ok(Msg::Done(stats)) => {
                flush(&mut batch);
                sink(Event::Done(Summary {
                    found,
                    folders: stats.folders,
                    skipped: stats.skipped,
                    problems: stats.problems,
                    limit_reached,
                    cancelled: stats.cancelled && !limit_reached,
                    elapsed: started.elapsed(),
                    everything: false,
                }));
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush(&mut batch);
                sink(Event::Done(Summary { found, cancelled: true, elapsed: started.elapsed(), ..Summary::default() }));
                return;
            }
        }
        if last.elapsed() >= BATCH_EVERY {
            last = Instant::now();
            flush(&mut batch);
            sink(Event::Progress { found, folders: read.load(Ordering::Relaxed) });
        }
    }
}

/// The walk `spec` needs: its roots (the scope, or every local fixed drive), threads (2 on a
/// network folder), the hidden rule (`view_shown`: `[view] show-hidden`, `show-system`), the
/// skipped folders (none with "Include skipped folders") and, on Unix, the devices it may enter.
/// Asks the file system: call it off the UI thread.
pub fn plan_walk(spec: &SearchSpec, skip: &[String], view_shown: (bool, bool)) -> Walk {
    let (roots, absolute): (Vec<PathBuf>, bool) = match &spec.scope {
        Scope::Folder(folder) => (vec![folder.clone()], false),
        Scope::AllDrives => (
            gezik_platform::drives()
                .into_iter()
                .filter(|drive| drive.kind == gezik_platform::DriveKind::Fixed)
                .map(|drive| drive.path)
                .collect(),
            true,
        ),
    };
    let network = roots.iter().any(|root| gezik_platform::fs::is_network(root).unwrap_or(false));
    let shown = (spec.hidden == HiddenRule::FollowView).then_some(view_shown);
    let skip: &[String] = if spec.skipped { &[] } else { skip };
    let mut devices: Vec<u64> = Vec::new();
    if cfg!(unix) {
        devices.extend(roots.iter().filter_map(|root| gezik_platform::fs::device_of(root).ok()));
        if cfg!(target_os = "macos")
            && let Ok(data) = gezik_platform::fs::device_of(Path::new("/System/Volumes/Data"))
        {
            devices.push(data);
        }
    }
    Walk::new(roots, absolute, threads_for(network), WalkRules::new(shown, skip, devices))
}

/// Results of a cache selection or an Everything answer, sent as one batch and a summary
/// (the UI takes both paths the same way).
pub fn send_whole(set: ResultSet, limit_reached: bool, started: Instant, everything: bool, sink: &dyn Fn(Event)) {
    let found = set.len();
    sink(Event::Batch(set.into_batch()));
    sink(Event::Done(Summary { found, limit_reached, elapsed: started.elapsed(), everything, ..Summary::default() }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::QueryOptions;
    use gezik_core::search::{Scope, SearchSpec};
    use std::sync::mpsc;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn options(max: usize) -> QueryOptions {
        QueryOptions { max_results: max, ..QueryOptions::local(64 * 1024 * 1024, max) }
    }

    /// Runs `spec` under `root` to the end: the results and the summary.
    fn search(root: &Path, spec: SearchSpec, max: usize) -> (ResultSet, Summary, Vec<usize>) {
        let query = Query::compile(&spec, &options(max)).unwrap();
        let walk = Walk::new(vec![root.to_path_buf()], false, 4, WalkRules::new(None, &[], Vec::new()));
        let (tx, rx) = mpsc::channel();
        start(walk, query, move |event| {
            let _ = tx.send(event);
        });
        let mut set = ResultSet::new(root.to_path_buf(), !spec.content.is_empty());
        let mut sizes = Vec::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::Batch(batch) => {
                    sizes.push(batch.entries.len());
                    set.append(batch);
                }
                Event::Progress { .. } => {}
                Event::Done(summary) => return (set, summary, sizes),
            }
        }
    }

    fn keys(set: &ResultSet) -> Vec<String> {
        let mut keys: Vec<String> =
            (0..set.len()).filter_map(|i| set.key_at(i)).map(|k| k.replace('\\', "/")).collect();
        keys.sort();
        keys
    }

    #[test]
    fn a_search_finds_names_under_the_scope() {
        let root = tree("names");
        for path in ["a/rapor.pdf", "a/b/Rapor 2.PDF", "notes.txt", "rapor/x.txt"] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let mut spec = SearchSpec::new(Scope::Folder(root.clone()));
        spec.pattern = "rapor".into();
        let (set, summary, _) = search(&root, spec, 100);
        assert_eq!(
            keys(&set),
            ["a/b/Rapor 2.PDF", "a/rapor.pdf", "rapor"],
            "a folder's name matches, not what is in it"
        );
        assert_eq!(summary.found, 3);
        assert!(!summary.cancelled && !summary.limit_reached);
        assert!(summary.folders >= 4);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn content_finds_the_line() {
        let root = tree("content");
        std::fs::write(root.join("a.txt"), "one\ntwo fatura\n").unwrap();
        std::fs::write(root.join("b.txt"), "nothing").unwrap();
        std::fs::write(root.join("c.jpg"), "fatura").unwrap();
        let mut spec = SearchSpec::new(Scope::Folder(root.clone()));
        spec.content = "FATURA".into();
        let (set, _, _) = search(&root, spec, 100);
        assert_eq!(keys(&set), ["a.txt"], "a picture is not read");
        assert_eq!(set.found(0).map(|f| (f.0, f.1.to_string())), Some((2, "two fatura".to_owned())));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_limit_stops_the_search() {
        let root = tree("limit");
        for i in 0..50 {
            std::fs::write(root.join(format!("f{i}.txt")), "x").unwrap();
        }
        let (set, summary, _) = search(&root, SearchSpec::flat_view(root.clone()), 20);
        assert_eq!(set.len(), 20);
        assert!(summary.limit_reached && !summary.cancelled);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn batches_hold_at_most_two_thousand() {
        let root = tree("batches");
        for i in 0..5000 {
            std::fs::write(root.join(format!("f{i}.txt")), "").unwrap();
        }
        let (set, _, sizes) = search(&root, SearchSpec::flat_view(root.clone()), 250_000);
        assert_eq!(set.len(), 5000);
        assert!(sizes.iter().all(|n| *n <= BATCH_ITEMS), "{sizes:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_walk_is_planned_from_the_spec() {
        let root = tree("plan");
        let mut spec = SearchSpec::new(Scope::Folder(root.clone()));
        let walk = plan_walk(&spec, &[".git".into()], (false, false));
        assert_eq!(walk.roots, std::slice::from_ref(&root));
        assert!(!walk.absolute && walk.threads >= 2);
        assert_eq!(walk.rules.shown, Some((false, false)));
        assert_eq!(walk.rules.skip, [".git"]);
        spec.skipped = true;
        spec.hidden = gezik_core::search::HiddenRule::Include;
        let walk = plan_walk(&spec, &[".git".into()], (false, false));
        assert!(walk.rules.skip.is_empty() && walk.rules.shown.is_none());
        let drives = plan_walk(&SearchSpec::new(Scope::AllDrives), &[], (true, false));
        assert!(drives.absolute && !drives.roots.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
