//! The folders visited, for the address bar's Recent and Frequent lists (spec 6.2): how often
//! and when each was last visited, scored by "frecency". Pure: the app records the visits and
//! writes them to state.toml.

use std::path::{Path, PathBuf};

use crate::ops::paths::same_path;
use crate::pattern::fold_text;

/// The most folders kept; past it the lowest score goes.
pub const MAX_VISITS: usize = 200;

const HOUR: u64 = 60 * 60;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

/// One folder visited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub path: PathBuf,
    /// How many times (at least 1).
    pub count: u32,
    /// When last, in seconds since 1970.
    pub last: u64,
}

/// A visit's score at `now`: its count times how recent it is, in quarters so that scores
/// are whole numbers (last hour ×4, last day ×2, last week ×1, older ×0.25).
pub fn score(visit: &Visit, now: u64) -> u64 {
    let age = now.saturating_sub(visit.last);
    let quarters = if age < HOUR {
        16
    } else if age < DAY {
        8
    } else if age < WEEK {
        4
    } else {
        1
    };
    u64::from(visit.count) * quarters
}

/// The visits, the most recently visited last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderHistory {
    visits: Vec<Visit>,
}

impl FolderHistory {
    /// From saved visits: one per folder (the first's path, the counts added up, the latest
    /// time), none counted 0, at most `MAX_VISITS` (the lowest scores at `now` go).
    pub fn from_visits(visits: Vec<Visit>, now: u64) -> FolderHistory {
        let mut history = FolderHistory::default();
        // Keep the most recent 2 x MAX_VISITS first, so a huge file costs little to fold.
        let mut visits: Vec<Visit> = visits.into_iter().filter(|visit| visit.count > 0).collect();
        if visits.len() > 2 * MAX_VISITS {
            visits.sort_by_key(|visit| std::cmp::Reverse(visit.last));
            visits.truncate(2 * MAX_VISITS);
        }
        for mut visit in visits {
            visit.last = visit.last.min(now);
            match history.visits.iter_mut().find(|kept| same_path(&kept.path, &visit.path)) {
                Some(kept) => {
                    kept.count = kept.count.saturating_add(visit.count);
                    kept.last = kept.last.max(visit.last);
                }
                None => history.visits.push(visit),
            }
        }
        while history.visits.len() > MAX_VISITS {
            history.drop_lowest(now);
        }
        history
    }

    pub fn visits(&self) -> &[Visit] {
        &self.visits
    }

    pub fn is_empty(&self) -> bool {
        self.visits.is_empty()
    }

    /// Counts a visit to `path` at `now` and makes it the latest. A new folder past
    /// `MAX_VISITS` makes the lowest score go (the oldest of equal ones).
    pub fn visit(&mut self, path: &Path, now: u64) {
        let found = self.visits.iter().position(|visit| same_path(&visit.path, path));
        let visit = match found {
            Some(i) => {
                let mut visit = self.visits.remove(i);
                visit.count = visit.count.saturating_add(1);
                visit.last = now;
                visit
            }
            None => {
                if self.visits.len() >= MAX_VISITS {
                    self.drop_lowest(now);
                }
                Visit { path: path.to_path_buf(), count: 1, last: now }
            }
        };
        self.visits.push(visit);
    }

    fn drop_lowest(&mut self, now: u64) {
        let lowest =
            self.visits.iter().enumerate().min_by_key(|(_, visit)| (score(visit, now), visit.last)).map(|(i, _)| i);
        if let Some(i) = lowest {
            self.visits.remove(i);
        }
    }

    /// Forgets `paths`; returns whether any of them was there.
    pub fn remove(&mut self, paths: &[PathBuf]) -> bool {
        let before = self.visits.len();
        self.visits.retain(|visit| !paths.iter().any(|path| same_path(path, &visit.path)));
        self.visits.len() != before
    }

    pub fn clear(&mut self) {
        self.visits.clear();
    }

    /// The `n` visited last, the latest first (of two in the same second, the later).
    pub fn recent(&self, n: usize) -> Vec<&Visit> {
        let mut out: Vec<&Visit> = self.visits.iter().rev().collect();
        out.sort_by_key(|visit| std::cmp::Reverse(visit.last));
        out.truncate(n);
        out
    }

    /// The `n` best scored at `now`, but those in `skip`; the best first, the latest of equal ones.
    pub fn frequent<'a>(&'a self, n: usize, now: u64, skip: &[&Visit]) -> Vec<&'a Visit> {
        let rest = self.visits.iter().filter(|visit| !skip.iter().any(|s| same_path(&s.path, &visit.path)));
        best(rest, n, now)
    }

    /// The `n` best scored whose path holds `text` (case and the Turkish i ignored), but those
    /// `skip` says yes to.
    pub fn matching(&self, text: &str, n: usize, now: u64, skip: impl Fn(&Path) -> bool) -> Vec<&Visit> {
        let needle = fold_text(text);
        let found = self
            .visits
            .iter()
            .filter(|visit| !skip(&visit.path) && fold_text(&visit.path.to_string_lossy()).contains(&needle));
        best(found, n, now)
    }
}

/// The `n` best scored of `visits` at `now`, the latest of equal ones first.
fn best<'a>(visits: impl Iterator<Item = &'a Visit>, n: usize, now: u64) -> Vec<&'a Visit> {
    let mut out: Vec<&Visit> = visits.collect();
    out.sort_by(|a, b| score(b, now).cmp(&score(a, now)).then(b.last.cmp(&a.last)));
    out.truncate(n);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn v(path: &str, count: u32, last: u64) -> Visit {
        Visit { path: p(path), count, last }
    }

    fn paths(list: &[&Visit]) -> Vec<PathBuf> {
        list.iter().map(|visit| visit.path.clone()).collect()
    }

    #[test]
    fn the_score_weighs_how_recent() {
        assert_eq!(score(&v("/a", 3, NOW - 10), NOW), 48);
        assert_eq!(score(&v("/a", 3, NOW - 2 * HOUR), NOW), 24);
        assert_eq!(score(&v("/a", 3, NOW - 2 * DAY), NOW), 12);
        assert_eq!(score(&v("/a", 3, NOW - 30 * DAY), NOW), 3);
        assert_eq!(score(&v("/a", 1, NOW + 50), NOW), 16, "a clock set back counts as now");
    }

    #[test]
    fn a_visit_counts_once_per_folder_and_moves_it_last() {
        let mut h = FolderHistory::default();
        h.visit(&p("/a"), NOW - 100);
        h.visit(&p("/b"), NOW - 50);
        h.visit(&p("/a"), NOW);
        assert_eq!(h.visits(), [v("/b", 1, NOW - 50), v("/a", 2, NOW)]);
        // A trailing separator is the same folder.
        h.visit(&p("/b/"), NOW);
        assert_eq!(h.visits(), [v("/a", 2, NOW), v("/b", 2, NOW)]);
    }

    #[test]
    fn of_two_visits_in_one_second_the_later_is_more_recent() {
        let mut h = FolderHistory::default();
        h.visit(&p("/a"), NOW);
        h.visit(&p("/b"), NOW);
        h.visit(&p("/a"), NOW);
        assert_eq!(paths(&h.recent(2)), [p("/a"), p("/b")]);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_makes_no_other_folder_where_the_system_ignores_it() {
        let mut h = FolderHistory::default();
        h.visit(&p("/Users/A/Belgeler"), NOW);
        h.visit(&p("/users/a/BELGELER"), NOW);
        assert_eq!(h.visits().len(), 1);
        assert_eq!(h.visits()[0].count, 2);
    }

    #[test]
    fn past_the_limit_the_lowest_score_goes() {
        let mut h = FolderHistory::default();
        // All older than a week: a score of 1 each.
        for i in 0..MAX_VISITS as u64 {
            h.visit(&p(&format!("/f{i}")), NOW - 2 * WEEK + i);
        }
        h.visit(&p("/f7"), NOW);
        h.visit(&p("/new"), NOW);
        assert_eq!(h.visits().len(), MAX_VISITS);
        assert!(!h.visits().iter().any(|x| x.path == p("/f0")), "the oldest of the lowest went");
        assert!(h.visits().iter().any(|x| x.path == p("/f7")) && h.visits().iter().any(|x| x.path == p("/new")));
    }

    #[test]
    fn recent_and_frequent_lists() {
        let h = FolderHistory::from_visits(
            vec![
                v("/old-often", 50, NOW - 30 * DAY),
                v("/today", 2, NOW - 3 * HOUR),
                v("/now", 1, NOW - 60),
                v("/week", 3, NOW - 3 * DAY),
            ],
            NOW,
        );
        let recent = h.recent(2);
        assert_eq!(paths(&recent), [p("/now"), p("/today")]);
        assert_eq!(paths(&h.frequent(7, NOW, &recent)), [p("/old-often"), p("/week")]);
    }

    #[test]
    fn matching_ignores_case_and_turkish_i() {
        let h = FolderHistory::from_visits(
            vec![v("/home/ali/İndirilenler", 1, NOW), v("/home/ali/Belgeler", 5, NOW), v("/srv/indir", 9, NOW)],
            NOW,
        );
        assert_eq!(paths(&h.matching("indir", 5, NOW, |_| false)), [p("/srv/indir"), p("/home/ali/İndirilenler")]);
        assert_eq!(h.matching("INDIR", 5, NOW, |path| path == Path::new("/srv/indir")).len(), 1);
        assert_eq!(h.matching("belge", 1, NOW, |_| false).len(), 1);
        assert!(h.matching("zzz", 5, NOW, |_| false).is_empty());
    }

    #[test]
    fn saved_visits_are_cleaned_up() {
        let h = FolderHistory::from_visits(vec![v("/a", 2, NOW - 10), v("/b", 0, NOW), v("/a/", 3, NOW)], NOW);
        assert_eq!(h.visits(), [v("/a", 5, NOW)]);
        let many: Vec<Visit> = (0..250u32).map(|i| v(&format!("/m{i}"), 1 + i, NOW)).collect();
        let h = FolderHistory::from_visits(many, NOW);
        assert_eq!(h.visits().len(), MAX_VISITS);
        assert!(h.visits().iter().all(|x| x.count > 50), "the lowest scores went");
    }

    #[test]
    fn forgetting_and_clearing() {
        let mut h = FolderHistory::from_visits(vec![v("/a", 1, NOW), v("/b", 1, NOW)], NOW);
        assert!(h.remove(&[p("/a"), p("/zzz")]));
        assert!(!h.remove(&[p("/zzz")]));
        assert_eq!(h.visits(), [v("/b", 1, NOW)]);
        h.clear();
        assert!(h.is_empty());
    }

    #[test]
    fn a_future_time_is_clamped_and_a_huge_file_is_cut() {
        let h = FolderHistory::from_visits(vec![v("/a", 1, NOW + 5000)], NOW);
        assert_eq!(h.visits()[0].last, NOW);
        let many: Vec<Visit> = (0..10_000u64).map(|i| v(&format!("/m{i}"), 1, NOW - 20_000 + i)).collect();
        let h = FolderHistory::from_visits(many, NOW);
        assert_eq!(h.visits().len(), MAX_VISITS);
        assert!(h.visits().iter().all(|x| x.last >= NOW - 20_000 + 9_000), "the most recent kept");
    }
}
