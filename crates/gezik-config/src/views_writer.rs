//! `views.toml` written by one thread of its own, as `state.toml` is: the UI hands over the
//! folder views as they are now and never touches the disk; a burst of changes is written
//! once, with the last of them.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gezik_core::view_memory::FolderView;

use crate::lock;
use crate::paths::write_atomic;
use crate::views_file::views_to_toml;

/// What the writer is woken for: a write, or a flush (answered once all is written).
type Wake = Option<Sender<()>>;

/// How long a flush waits for the writer.
const FLUSH_WAIT: Duration = Duration::from_secs(5);

pub(crate) struct ViewsWriter {
    path: PathBuf,
    /// The folder views handed over and not written yet.
    pending: Mutex<Option<Vec<FolderView>>>,
    /// Held while the file is written: one write at a time (they share the temporary name).
    writing: Mutex<()>,
    /// Wakes the writer thread, once it was started.
    writer: Mutex<Option<Sender<Wake>>>,
    /// Set by [`ViewsWriter::keep_unwritten`]: nothing more is written.
    read_only: AtomicBool,
}

impl fmt::Debug for ViewsWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewsWriter").field("path", &self.path).finish_non_exhaustive()
    }
}

impl ViewsWriter {
    pub fn new(path: PathBuf) -> ViewsWriter {
        ViewsWriter {
            path,
            pending: Mutex::new(None),
            writing: Mutex::new(()),
            writer: Mutex::new(None),
            read_only: AtomicBool::new(false),
        }
    }

    /// Has the writer thread write `folders` soon (a newer hand-over replaces one not written
    /// yet).
    pub fn send(self: &Arc<Self>, folders: Vec<FolderView>) {
        if self.read_only.load(Ordering::Relaxed) {
            return;
        }
        *lock(&self.pending) = Some(folders);
        self.wake(None);
    }

    /// Writes `folders` now, on this thread.
    pub fn save(&self, folders: &[FolderView]) -> io::Result<()> {
        if self.read_only.load(Ordering::Relaxed) {
            return Ok(());
        }
        let _turn = lock(&self.writing);
        // What was waiting is older than this.
        lock(&self.pending).take();
        write(&self.path, folders)
    }

    /// From now on `views.toml` is left as it is (a second window's, spec 5.3).
    pub fn keep_unwritten(&self) {
        self.read_only.store(true, Ordering::Relaxed);
    }

    /// Waits (up to 5 s) until everything handed over so far is written.
    pub fn flush(self: &Arc<Self>) {
        if lock(&self.writer).is_none() {
            return;
        }
        let (done, written) = mpsc::channel();
        self.wake(Some(done));
        let _ = written.recv_timeout(FLUSH_WAIT);
    }

    /// Writes what is waiting, if anything.
    fn write_pending(&self) {
        let _turn = lock(&self.writing);
        let Some(folders) = lock(&self.pending).take() else { return };
        if let Err(err) = write(&self.path, &folders) {
            eprintln!("gezik: cannot save {}: {err}", self.path.display());
        }
    }

    fn wake(self: &Arc<Self>, wake: Wake) {
        let mut writer = lock(&self.writer);
        if writer.is_none() {
            *writer = self.start();
        }
        match writer.as_ref() {
            Some(sender) => {
                let _ = sender.send(wake);
            }
            // No thread could be started: written here.
            None => {
                drop(writer);
                self.write_pending();
                if let Some(done) = wake {
                    let _ = done.send(());
                }
            }
        }
    }

    /// The writer thread: each time it is woken it writes what is waiting then, once for all
    /// the wakes waiting. It ends with the writer (whose sender is the only one).
    fn start(self: &Arc<Self>) -> Option<Sender<Wake>> {
        let (sender, wakes) = mpsc::channel::<Wake>();
        let cell = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("gezik-views".into())
            .spawn(move || {
                while let Ok(first) = wakes.recv() {
                    let mut flushes: Vec<Sender<()>> = first.into_iter().collect();
                    flushes.extend(wakes.try_iter().flatten());
                    if let Some(cell) = cell.upgrade() {
                        cell.write_pending();
                    }
                    for done in flushes {
                        let _ = done.send(());
                    }
                }
            })
            .ok()
            .map(|_| sender)
    }
}

fn write(path: &std::path::Path, folders: &[FolderView]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_atomic(path, &views_to_toml(folders))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views_file::parse_views;
    use gezik_core::view::ViewSettings;

    fn folder(path: &str, used: u64) -> FolderView {
        FolderView { path: path.to_owned(), view: ViewSettings::default(), used }
    }

    #[test]
    fn a_burst_is_written_with_its_last_hand_over_and_a_flush_waits_for_it() {
        let dir = crate::test_dir("views-writer");
        // Not made yet: the writer makes the folder.
        let path = dir.join("sub").join("views.toml");
        let writer = Arc::new(ViewsWriter::new(path.clone()));
        for i in 0..50 {
            writer.send(vec![folder(&format!("/f{i}"), i)]);
        }
        writer.flush();
        let folders = parse_views(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(folders, [folder("/f49", 49)], "the last of the burst wins");
        let names: Vec<_> =
            std::fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["views.toml"], "no temporary file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_flush_with_nothing_sent_returns_at_once_and_writes_nothing() {
        let dir = crate::test_dir("views-writer-idle");
        let path = dir.join("views.toml");
        let writer = Arc::new(ViewsWriter::new(path.clone()));
        writer.flush();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
