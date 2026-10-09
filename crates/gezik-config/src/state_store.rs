//! `state.toml` kept in memory and written by one thread of its own: the UI changes the
//! state without touching the disk, and two changes made close together (by different parts
//! of the UI) are never lost or written over each other.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::lock;
use crate::paths::write_atomic;
use crate::settings::State;

/// What the writer is woken for: a write, or a flush (answered once all is written).
type Wake = Option<Sender<()>>;

pub(crate) struct StateCell {
    path: PathBuf,
    /// The state as it is now; `None` until it was read.
    memory: Mutex<Option<State>>,
    /// Held while the file is written: one write at a time (they share the temporary name).
    writing: Mutex<()>,
    /// Wakes the writer thread, once it was started.
    writer: Mutex<Option<Sender<Wake>>>,
    /// Set by [`StateCell::keep_unwritten`]: nothing more is written.
    read_only: std::sync::atomic::AtomicBool,
}

impl fmt::Debug for StateCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateCell").field("path", &self.path).finish_non_exhaustive()
    }
}

/// How long a flush waits for the writer.
const FLUSH_WAIT: Duration = Duration::from_secs(5);

impl StateCell {
    pub fn new(path: PathBuf) -> StateCell {
        StateCell {
            path,
            memory: Mutex::new(None),
            writing: Mutex::new(()),
            writer: Mutex::new(None),
            read_only: Default::default(),
        }
    }

    /// The state: read from the file the first time (at start), from memory after that.
    pub fn get(&self) -> State {
        self.memory().as_ref().cloned().unwrap_or_default()
    }

    fn memory(&self) -> MutexGuard<'_, Option<State>> {
        let mut memory = lock(&self.memory);
        if memory.is_none() {
            *memory = Some(read(&self.path));
        }
        memory
    }

    /// Changes the state in memory and has the writer thread write it soon (a burst of
    /// changes is written once).
    pub fn update(self: &Arc<Self>, change: impl FnOnce(&mut State)) {
        if let Some(state) = self.memory().as_mut() {
            change(state);
        }
        self.wake(None);
    }

    /// Sets the state and writes it now, on this thread.
    pub fn save(&self, state: &State) -> io::Result<()> {
        *lock(&self.memory) = Some(state.clone());
        self.write()
    }

    /// Waits (up to 5 s) until every change made so far is written.
    pub fn flush(self: &Arc<Self>) {
        if lock(&self.writer).is_none() {
            return;
        }
        let (done, written) = mpsc::channel();
        self.wake(Some(done));
        let _ = written.recv_timeout(FLUSH_WAIT);
    }

    /// From now on nothing is written: a `--new-window` Gezik (spec 5.3) must not put the
    /// tabs it read at start over those of the window that keeps them.
    pub fn keep_unwritten(&self) {
        self.read_only.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Writes what is in memory now.
    fn write(&self) -> io::Result<()> {
        if self.read_only.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let _turn = lock(&self.writing);
        let Some(state) = lock(&self.memory).clone() else { return Ok(()) };
        write_atomic(&self.path, &state.to_toml())
    }

    fn write_or_say(&self) {
        if let Err(err) = self.write() {
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
                self.write_or_say();
                if let Some(done) = wake {
                    let _ = done.send(());
                }
            }
        }
    }

    /// The writer thread: each time it is woken it writes the state as it is in memory then,
    /// once for all the wakes waiting. It ends with the cell (whose sender is the only one).
    fn start(self: &Arc<Self>) -> Option<Sender<Wake>> {
        let (sender, wakes) = mpsc::channel::<Wake>();
        let cell = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("gezik-state".into())
            .spawn(move || {
                while let Ok(first) = wakes.recv() {
                    let mut flushes: Vec<Sender<()>> = first.into_iter().collect();
                    flushes.extend(wakes.try_iter().flatten());
                    if let Some(cell) = cell.upgrade() {
                        cell.write_or_say();
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

fn read(path: &std::path::Path) -> State {
    crate::store::read_text(path).map(|text| State::parse(&text)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_from_two_places_close_together_are_all_written() {
        let dir = crate::test_dir("state-store");
        let path = dir.join("state.toml");
        std::fs::write(&path, "[preview]\nopen = true\n").unwrap();
        let cell = Arc::new(StateCell::new(path.clone()));
        assert!(cell.get().preview_open, "read at first");
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let cell = cell.clone();
                std::thread::spawn(move || {
                    for i in 0..50 {
                        if n % 2 == 0 {
                            cell.update(|state| state.sidebar_width = Some(200 + i));
                        } else {
                            cell.update(|state| state.preview_width = Some(300 + i));
                        }
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        cell.update(|state| state.operations_collapsed = true);
        cell.flush();
        let written = State::parse(&std::fs::read_to_string(&path).unwrap());
        assert!(written.preview_open && written.operations_collapsed);
        assert_eq!((written.sidebar_width, written.preview_width), (Some(249), Some(349)));
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"], "no temporary file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_kept_unwritten_never_writes() {
        let dir = crate::test_dir("state-unwritten");
        let path = dir.join("state.toml");
        std::fs::write(&path, "[preview]\nopen = true\n").unwrap();
        let cell = Arc::new(StateCell::new(path.clone()));
        cell.keep_unwritten();
        cell.update(|state| state.preview_open = false);
        cell.flush();
        cell.save(&State::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[preview]\nopen = true\n");
        assert!(!cell.get().preview_open, "memory still follows");
    }
}
