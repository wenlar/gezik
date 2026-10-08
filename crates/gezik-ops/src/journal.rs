//! A copy's journal: one line per file it copies under its own name (`source<TAB>target`),
//! written before that file is copied. The system makes a copy its full size at once and
//! gives it the source's modification time last, so if Gezik is killed, the next start
//! deletes each target that is not a finished copy of its source. A target changed after the
//! journal was last written is left alone (someone used it since).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use gezik_platform::fs;

use crate::engine::lock;

/// FAT and exFAT keep times in 2 second steps.
const FAT_STEP: Duration = Duration::from_secs(2);

pub(crate) struct Journal {
    /// `None`: nowhere to keep one (no config folder).
    path: Option<PathBuf>,
    file: Mutex<Option<File>>,
}

impl Journal {
    /// A journal named `name` in `dir`; nothing is written until the first file.
    pub fn new(dir: Option<PathBuf>, name: &str) -> Journal {
        Journal { path: dir.map(|dir| dir.join(format!("{}-{name}.log", std::process::id()))), file: Mutex::default() }
    }

    /// Whether a line holds `source` and `target` as they are: Unicode, no tab or line break.
    /// Read back, another name could be paths of unrelated files, which recovery would delete.
    pub fn can_note(source: &Path, target: &Path) -> bool {
        crate::pending::can_hold(source) && crate::pending::can_hold(target)
    }

    /// `target` is about to be written as a copy of `source` (see `can_note`).
    pub fn note(&self, source: &Path, target: &Path) {
        let Some(path) = &self.path else { return };
        if !Journal::can_note(source, target) {
            return;
        }
        let mut file = lock(&self.file);
        if file.is_none() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            *file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
        }
        if let Some(file) = file.as_mut() {
            // One write per line, straight to the system: a kill loses nothing written.
            let _ = file.write_all(format!("{}\t{}\n", source.display(), target.display()).as_bytes());
        }
    }

    /// The copy ended: every file it wrote is finished or removed.
    pub fn done(&self) {
        if lock(&self.file).take().is_some()
            && let Some(path) = &self.path
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Deletes the unfinished targets in the journals of copies that no longer run, then the
/// journals.
pub(crate) fn recover(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_suffix(".log")?.split('-').next()?.parse::<u32>().ok())
        else {
            continue;
        };
        if gezik_platform::process_alive(pid) {
            continue;
        }
        let (Ok(text), Ok(written)) = (std::fs::read_to_string(&path), entry.metadata().and_then(|m| m.modified()))
        else {
            continue;
        };
        for line in text.lines() {
            if let Some((source, target)) = line.split_once('\t')
                && let (source, target) = (Path::new(source), Path::new(target))
                && source.is_absolute()
                && target.is_absolute()
                && unfinished(source, target, written)
            {
                let _ = fs::delete(target);
            }
        }
        let _ = std::fs::remove_file(&path);
    }
}

/// Whether `target` is a copy of `source` cut short. `written`: when its journal last grew.
fn unfinished(source: &Path, target: &Path, written: SystemTime) -> bool {
    let Ok(target_meta) = std::fs::symlink_metadata(target) else { return false };
    if !target_meta.is_file() {
        return false;
    }
    // A move removes its source only once the copy is finished.
    let Ok(source_meta) = std::fs::metadata(source) else { return false };
    let (Ok(target_time), Ok(source_time)) = (target_meta.modified(), source_meta.modified()) else { return false };
    if target_time > written + FAT_STEP {
        // Changed after Gezik stopped: someone used it, it is not ours to judge.
        return false;
    }
    !(target_meta.len() == source_meta.len() && same_time(source_time, target_time))
}

/// The copy's time is the source's: exactly, or rounded to 2 seconds on FAT.
fn same_time(source: SystemTime, target: SystemTime) -> bool {
    if source == target {
        return true;
    }
    let gap = source.duration_since(target).or_else(|_| target.duration_since(source)).unwrap_or(Duration::MAX);
    let fat = target
        .duration_since(SystemTime::UNIX_EPOCH)
        .is_ok_and(|since| since.subsec_nanos() == 0 && since.as_secs() % 2 == 0);
    fat && gap <= FAT_STEP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{test_dir, write};

    fn set_time(path: &Path, time: SystemTime) {
        std::fs::File::options().write(true).open(path).unwrap().set_modified(time).unwrap();
    }

    fn journal_with(dir: &Path, pid: u32, pairs: &[(&Path, &Path)]) -> PathBuf {
        let journals = dir.join("copying");
        std::fs::create_dir_all(&journals).unwrap();
        let path = journals.join(format!("{pid}-abc.log"));
        let text: String = pairs.iter().map(|(s, t)| format!("{}\t{}\n", s.display(), t.display())).collect();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn finished_pid() -> u32 {
        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "exit"]).spawn().unwrap()
        } else {
            std::process::Command::new("true").spawn().unwrap()
        };
        child.wait().unwrap();
        child.id()
    }

    #[test]
    fn recovery_deletes_only_unfinished_copies() {
        let dir = test_dir("journal-recover");
        let old = SystemTime::now() - Duration::from_secs(3600);
        let src = |name: &str| {
            let path = dir.join("src").join(name);
            write(&path, "0123456789");
            set_time(&path, old);
            path
        };
        let (a, b, c, d) = (src("a"), src("b"), src("c"), src("d"));
        // a: finished (same size, the source's time). b: full size, but cut short (its own
        // time). c: shorter. d: its source is gone (a move that finished).
        let (ta, tb, tc, td) = (dir.join("dst/a"), dir.join("dst/b"), dir.join("dst/c"), dir.join("dst/d"));
        write(&ta, "0123456789");
        set_time(&ta, old);
        write(&tb, "0123\0\0\0\0\0\0");
        write(&tc, "0123");
        write(&td, "0123");
        std::fs::remove_file(&d).unwrap();
        let journal = journal_with(&dir, finished_pid(), &[(&a, &ta), (&b, &tb), (&c, &tc), (&d, &td)]);
        recover(&dir.join("copying"));
        assert!(ta.exists(), "a finished copy stays");
        assert!(!tb.exists() && !tc.exists(), "copies cut short go");
        assert!(td.exists(), "nothing to compare with: it stays");
        assert!(!journal.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_target_changed_after_the_journal_stays() {
        let dir = test_dir("journal-edited");
        let source = dir.join("src/a");
        write(&source, "0123456789");
        set_time(&source, SystemTime::now() - Duration::from_secs(3600));
        let target = dir.join("dst/a");
        write(&target, "edited after the crash");
        let journal = journal_with(&dir, finished_pid(), &[(&source, &target)]);
        set_time(&journal, SystemTime::now() - Duration::from_secs(60));
        recover(&dir.join("copying"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "edited after the crash");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_journal_of_a_running_copy_stays() {
        let dir = test_dir("journal-running");
        let (source, target) = (dir.join("src/a"), dir.join("dst/a"));
        write(&source, "0123456789");
        write(&target, "0123");
        let mut running = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "ping -n 30 127.0.0.1 >nul"]).spawn().unwrap()
        } else {
            std::process::Command::new("sleep").arg("30").spawn().unwrap()
        };
        let journal = journal_with(&dir, running.id(), &[(&source, &target)]);
        recover(&dir.join("copying"));
        let _ = running.kill();
        let _ = running.wait();
        assert!(target.exists() && journal.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_journal_is_written_line_by_line_and_removed_when_done() {
        let dir = test_dir("journal-write");
        let journal = Journal::new(Some(dir.join("copying")), "abc");
        journal.note(Path::new("/s/a"), Path::new("/t/a"));
        journal.note(Path::new("/s/b"), Path::new("/t/b"));
        let path = dir.join("copying").join(format!("{}-abc.log", std::process::id()));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        journal.done();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_a_line_cannot_hold_is_not_noted() {
        let dir = test_dir("journal-names");
        let journal = Journal::new(Some(dir.join("copying")), "abc");
        journal.note(Path::new("/s/a\n/s/b\t/t/victim"), Path::new("/t/a\n/s/b\t/t/victim"));
        journal.note(Path::new("/s/a\tb"), Path::new("/t/a\tb"));
        journal.note(Path::new("/s/a\r"), Path::new("/t/a\r"));
        assert!(!dir.join("copying").join(format!("{}-abc.log", std::process::id())).exists());
        assert!(Journal::can_note(Path::new("/s/a b"), Path::new("/t/a b")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fat_times_match_within_two_seconds_only_when_rounded() {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(same_time(base + Duration::from_millis(1500), base));
        assert!(!same_time(base, base + Duration::from_millis(1500)), "not a FAT time");
        assert!(!same_time(base + Duration::from_secs(4), base));
    }
}
