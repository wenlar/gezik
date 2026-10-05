//! Helpers for the engine's tests.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::engine::{ConflictItem, Engine, Event, JobId, Report, Settings, lock};
use crate::task::{Answer, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

/// A fresh, empty folder.
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gezik-ops-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Writes `text` to `path`, making its folders.
pub(crate) fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub(crate) fn engine() -> Engine {
    Engine::new(Settings::default(), || {})
}

/// Each conflict's starting decision.
pub(crate) fn defaults(conflicts: &[ConflictItem]) -> Vec<Decision> {
    conflicts.iter().map(|c| c.decision).collect()
}

/// Waits until `job` finishes, answering its conflicts with `decide`; returns the report and
/// every event seen for it on the way.
pub(crate) fn finish(
    engine: &Engine,
    job: JobId,
    decide: impl Fn(&[ConflictItem]) -> Vec<Decision>,
) -> (Report, Vec<Event>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut seen = Vec::new();
    // Other jobs' events go back to the queue for their own `finish`.
    let mut others = Vec::new();
    loop {
        let mut batch = engine.drain().into_iter();
        while let Some(event) = batch.next() {
            match &event {
                Event::Conflicts { job: j, conflicts } if *j == job => engine.decide(job, decide(conflicts)),
                Event::Finished { job: j, report } if *j == job => {
                    let report = report.clone();
                    seen.push(event);
                    others.extend(batch);
                    engine.requeue(others);
                    return (report, seen);
                }
                _ => {}
            }
            if event_job(&event).is_some_and(|j| j != job) {
                others.push(event);
            } else {
                seen.push(event);
            }
        }
        assert!(Instant::now() < deadline, "job {job} did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn event_job(event: &Event) -> Option<JobId> {
    match event {
        Event::Added { job, .. }
        | Event::Progress { job, .. }
        | Event::Conflicts { job, .. }
        | Event::Paused { job, .. }
        | Event::Question { job, .. }
        | Event::Finished { job, .. } => Some(*job),
        Event::Changed { .. } | Event::History => None,
    }
}

/// A gate the fake task's items wait at until it opens.
#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    pub fn open(&self) {
        *lock(&self.0.0) = true;
        self.0.1.notify_all();
    }

    pub fn wait(&self) {
        let mut open = lock(&self.0.0);
        while !*open {
            open = self.0.1.wait(open).unwrap();
        }
    }
}

/// A task for testing the engine itself: `items` items that log, wait at a gate, fail, or
/// find the disk full once.
pub(crate) struct FakeTask {
    pub name: &'static str,
    pub items: usize,
    /// Drive paths it claims (none: it shares no drive with anything).
    pub paths: Vec<PathBuf>,
    pub gate: Option<Gate>,
    pub fail: Vec<usize>,
    pub disk_full_once: Mutex<Vec<usize>>,
    pub log: Arc<Mutex<Vec<String>>>,
    pub running: Arc<AtomicUsize>,
}

impl FakeTask {
    pub fn new(name: &'static str, items: usize, log: &Arc<Mutex<Vec<String>>>) -> FakeTask {
        FakeTask {
            name,
            items,
            paths: Vec::new(),
            gate: None,
            fail: Vec::new(),
            disk_full_once: Mutex::default(),
            log: log.clone(),
            running: Arc::default(),
        }
    }
}

impl Task for FakeTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        format!("fake {}", self.name)
    }

    fn count(&self) -> usize {
        self.items
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths.clone(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for i in 0..self.items {
            let facts = Facts { is_dir: false, size: 10, modified: None };
            if !sink.item(PlanItem::new(Stage::Parallel, facts).top(i)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        self.running.fetch_add(1, Ordering::SeqCst);
        lock(&self.log).push(format!("{}+{}", self.name, item.root));
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        lock(&self.log).push(format!("{}-{}", self.name, item.root));
        self.running.fetch_sub(1, Ordering::SeqCst);
        if self.fail.contains(&item.root) {
            return Err(io::Error::other("boom"));
        }
        let mut full = lock(&self.disk_full_once);
        if let Some(at) = full.iter().position(|&i| i == item.root) {
            full.remove(at);
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        Ok(Outcome::Nothing)
    }
}

/// Keeps a task's plan, for testing a task without the engine.
#[derive(Default)]
pub(crate) struct CollectSink {
    pub items: Vec<PlanItem>,
    pub failed: Vec<PathBuf>,
}

impl ScanSink for CollectSink {
    fn item(&mut self, item: PlanItem) -> bool {
        self.items.push(item);
        true
    }

    fn failed(&mut self, path: &Path, _error: io::Error) {
        self.failed.push(path.to_path_buf());
    }
}

/// Asks for a password once; its result is the answer's text.
#[derive(Default)]
pub(crate) struct AskingTask;

impl Task for AskingTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Extract
    }

    fn title(&self) -> String {
        "Asking".into()
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: Vec::new(), work: Work::Cpu }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        sink.item(PlanItem::new(Stage::Parallel, Facts::default()).top(0));
    }

    fn run(&self, _: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        match cx.ask(Question::Password { archive: "a.zip".into(), retry: false }) {
            Answer::Text(text) => Ok(Outcome::Created { path: text.into(), facts: Facts::default(), from: None }),
            _ => Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled")),
        }
    }
}

/// Makes a staging folder with a file in it.
pub(crate) struct StagingTask(PathBuf);

impl StagingTask {
    pub fn new(dir: PathBuf) -> StagingTask {
        StagingTask(dir)
    }
}

impl Task for StagingTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Extract
    }

    fn title(&self) -> String {
        "Staging".into()
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.0.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        sink.item(PlanItem::new(Stage::Parallel, Facts::default()).target(self.0.join("x")).top(0));
    }

    fn run(&self, _: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let staging = cx.staging_dir(&self.0.join("x"))?;
        std::fs::write(staging.join("f.txt"), "f")?;
        Ok(Outcome::Nothing)
    }
}
