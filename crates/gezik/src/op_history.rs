//! The operations panel's History (spec 9.4): each finished job (undo and redo too) as one
//! record, newest first, at most `HISTORY_MAX`, in memory only.

use std::collections::VecDeque;
use std::path::PathBuf;

use gezik_ops::{Failure, Report};

/// The most records kept; the oldest goes first.
pub const HISTORY_MAX: usize = 200;
/// The most results "Show in folder" selects.
pub const SHOW_MAX: usize = 1000;
/// "Details" lists at most this many lines, the "…and N more" line included.
pub const MAX_DETAILS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: u64,
    /// When it ended: `14:05:09`.
    pub time: String,
    pub title: String,
    pub result: String,
    pub failed: bool,
    /// Where "Show in folder" goes and what it selects there.
    pub show: Option<(PathBuf, Vec<String>)>,
    pub details: Option<String>,
}

#[derive(Debug, Default)]
pub struct History {
    records: VecDeque<Record>,
    next: u64,
}

impl History {
    /// Records a job that ended with `report`; its id.
    pub fn push(&mut self, time: String, title: String, report: &Report) -> u64 {
        self.next += 1;
        let (result, failed) = result_text(report);
        self.records.push_front(Record {
            id: self.next,
            time,
            title,
            result,
            failed,
            show: show_target(report),
            details: details_text(report),
        });
        self.records.truncate(HISTORY_MAX);
        self.next
    }

    /// Newest first.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    pub fn get(&self, id: u64) -> Option<&Record> {
        self.records.iter().find(|record| record.id == id)
    }
}

/// `Done`, `Done · 2 skipped`, `3 failed`, `Cancelled`; and whether it failed.
pub fn result_text(report: &Report) -> (String, bool) {
    if report.cancelled {
        return ("Cancelled".to_owned(), false);
    }
    match (report.failures.len(), report.skipped.len()) {
        (0, 0) => ("Done".to_owned(), false),
        (0, skipped) => (format!("Done · {skipped} skipped"), false),
        (failed, _) => (format!("{failed} failed"), true),
    }
}

/// The folder of the first result and the results there (at most `SHOW_MAX`); with no
/// results, the first folder the job changed.
pub fn show_target(report: &Report) -> Option<(PathBuf, Vec<String>)> {
    if let Some(dir) = report.results.first().and_then(|first| first.parent()) {
        let names = crate::operations::result_names(&report.results, dir);
        return Some((dir.to_path_buf(), names.into_iter().take(SHOW_MAX).collect()));
    }
    report.changed_dirs.first().map(|dir| (dir.clone(), Vec::new()))
}

/// The failures, then what was skipped under its own heading; at most `MAX_DETAILS` lines
/// in all (the blank line, the heading and the "…and N more" line count); None when there
/// are neither.
pub fn details_text(report: &Report) -> Option<String> {
    let (failed, skipped) = (report.failures.len(), report.skipped.len());
    if failed == 0 && skipped == 0 {
        return None;
    }
    let named = |f: &Failure| {
        format!(
            "{}: {}",
            f.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            crate::pdf::failure_shown(&f.message)
        )
    };
    // The blank line between the lists, and the heading.
    let overhead = if failed > 0 { 2 } else { 1 };
    let everything = failed + if skipped > 0 { overhead + skipped } else { 0 };
    // When it does not all fit, the last line is the "…and N more".
    let room = if everything <= MAX_DETAILS { MAX_DETAILS } else { MAX_DETAILS - 1 };
    let shown_failures = failed.min(room);
    let left = room - shown_failures;
    let shown_skipped = if skipped > 0 && left > overhead { skipped.min(left - overhead) } else { 0 };
    let mut lines: Vec<String> = report.failures.iter().take(shown_failures).map(named).collect();
    if shown_skipped > 0 {
        if failed > 0 {
            lines.push(String::new());
        }
        lines.push("Skipped:".to_owned());
        lines.extend(report.skipped.iter().take(shown_skipped).map(named));
    }
    let more = failed + skipped - shown_failures - shown_skipped;
    if more > 0 {
        lines.push(format!("…and {more} more"));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_ops::{Failure, TaskKind};

    fn report() -> Report {
        Report {
            kind: TaskKind::Copy,
            cancelled: false,
            failures: Vec::new(),
            skipped: Vec::new(),
            skipped_changed: 0,
            no_trash: Vec::new(),
            unchecked: Vec::new(),
            results: Vec::new(),
            changed_dirs: Vec::new(),
            moved: Vec::new(),
            as_admin: Vec::new(),
        }
    }

    fn failures(n: usize) -> Vec<Failure> {
        (0..n)
            .map(|i| Failure {
                path: PathBuf::from(format!("f{i}.txt")),
                message: "It is open".to_owned(),
                denied: false,
            })
            .collect()
    }

    #[test]
    fn results_read_as_the_spec_says() {
        assert_eq!(result_text(&report()), ("Done".to_owned(), false));
        assert_eq!(result_text(&Report { skipped: failures(2), ..report() }), ("Done · 2 skipped".to_owned(), false));
        assert_eq!(result_text(&Report { failures: failures(3), ..report() }), ("3 failed".to_owned(), true));
        assert_eq!(result_text(&Report { failures: failures(1), ..report() }), ("1 failed".to_owned(), true));
        assert_eq!(
            result_text(&Report { cancelled: true, failures: failures(1), ..report() }),
            ("Cancelled".to_owned(), false)
        );
    }

    #[test]
    fn the_history_keeps_the_newest_two_hundred() {
        let mut history = History::default();
        let first = history.push("14:05:09".to_owned(), "1".to_owned(), &report());
        for i in 2..=HISTORY_MAX + 5 {
            history.push("14:05:09".to_owned(), i.to_string(), &report());
        }
        let titles: Vec<&str> = history.records().map(|r| r.title.as_str()).collect();
        assert_eq!(titles.len(), HISTORY_MAX);
        assert_eq!(titles[0], (HISTORY_MAX + 5).to_string(), "the newest on top");
        assert!(history.get(first).is_none(), "the oldest went");
    }

    #[test]
    fn show_in_folder_goes_to_the_first_results_folder() {
        let (d, e) = (PathBuf::from("d"), PathBuf::from("e"));
        let results = vec![d.join("a"), d.join("b"), e.join("c")];
        assert_eq!(
            show_target(&Report { results, ..report() }),
            Some((d.clone(), vec!["a".to_owned(), "b".to_owned()]))
        );
        let changed = Report { changed_dirs: vec![e.clone()], ..report() };
        assert_eq!(show_target(&changed), Some((e, Vec::new())), "no results: the first folder it changed");
        assert_eq!(show_target(&report()), None);
        let many = Report { results: (0..1500).map(|i| d.join(format!("f{i}"))).collect(), ..report() };
        assert_eq!(show_target(&many).unwrap().1.len(), SHOW_MAX);
    }

    #[test]
    fn details_list_at_most_fifty_lines() {
        assert_eq!(details_text(&report()), None);
        let text = details_text(&Report { failures: failures(60), ..report() }).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_DETAILS, "the more line counts");
        assert_eq!(lines[0], "f0.txt: It is open");
        assert_eq!(lines[MAX_DETAILS - 2], "f48.txt: It is open");
        assert_eq!(lines.last().copied(), Some("…and 11 more"));
        let skipped = details_text(&Report { skipped: failures(1), ..report() }).unwrap();
        assert_eq!(skipped, "Skipped:\nf0.txt: It is open");
        let both = details_text(&Report { failures: failures(1), skipped: failures(1), ..report() }).unwrap();
        assert_eq!(both, "f0.txt: It is open\n\nSkipped:\nf0.txt: It is open");
        let exactly = details_text(&Report { failures: failures(MAX_DETAILS), ..report() }).unwrap();
        assert_eq!(exactly.lines().count(), MAX_DETAILS, "all fit: no more line");
        assert!(!exactly.contains("more"));
    }

    #[test]
    fn details_with_skipped_items_stay_within_fifty_lines() {
        // 49 failures leave no room for the skipped heading: they are only counted.
        let text = details_text(&Report { failures: failures(49), skipped: failures(10), ..report() }).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_DETAILS);
        assert_eq!(lines.last().copied(), Some("…and 10 more"));
        assert!(!text.contains("Skipped:"));
        // Room for some: the blank line and the heading count.
        let text = details_text(&Report { failures: failures(30), skipped: failures(40), ..report() }).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_DETAILS);
        assert_eq!(lines[30], "");
        assert_eq!(lines[31], "Skipped:");
        assert_eq!(lines.last().copied(), Some("…and 23 more"));
        // Many skipped and no failures.
        let text = details_text(&Report { skipped: failures(80), ..report() }).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_DETAILS);
        assert_eq!(lines[0], "Skipped:");
        assert_eq!(lines.last().copied(), Some("…and 32 more"));
    }
}
