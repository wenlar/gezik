//! The Git marks of the folder shown (spec 16 §8.2–8.4): one `git status` at a time on a
//! thread, only while a repository folder shows; again when the folder or the repository's
//! `.git` changes, events within 300 ms making one run and events during a run at most one more.
//! Nothing runs or waits while idle (no timer but the 300 ms after an event).

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gezik_core::Entry;
use gezik_core::git::{FolderMarks, GitMark};
use gezik_core::git_filter::GitTerm;
use gezik_core::group::GroupBy;
use gezik_core::sort::SortKey;
use gezik_i18n::t;

use super::View;
use super::listing::Listing;
use crate::git::{Outcome, Repo};

/// Events this close together make one run.
const SETTLE: Duration = Duration::from_millis(300);

/// The marks of a folder as the last run gave them.
pub struct FolderGit {
    pub dir: PathBuf,
    pub marks: FolderMarks,
    /// The status bar's Git part (`⎇ main ↑2 · 5 changed`).
    pub text: String,
}

/// When to run: an event starts the 300 ms wait unless one waits or a run goes on (then one
/// more run once it ends, however many events came).
#[derive(Debug, Default)]
struct Pacer {
    waiting: bool,
    running: bool,
    again: bool,
}

impl Pacer {
    /// An event: whether to start the wait.
    fn event(&mut self) -> bool {
        if self.running {
            self.again = true;
        }
        let start = !self.running && !self.waiting;
        self.waiting |= start;
        start
    }

    /// The wait ended (or a run is wanted at once): whether to run now.
    fn start(&mut self) -> bool {
        self.waiting = false;
        if self.running {
            self.again = true;
            return false;
        }
        self.running = true;
        true
    }

    /// The run ended: whether events came meanwhile.
    fn done(&mut self) -> bool {
        self.running = false;
        std::mem::take(&mut self.again)
    }
}

/// The run going on; dropped (another folder, a pane closed), git is stopped.
struct GitRun {
    id: u64,
    cancel: Arc<AtomicBool>,
}

impl Drop for GitRun {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// A view's Git state: the folder shown and its repository, the run, the `.git` watch.
#[derive(Default)]
pub struct GitState {
    at: Option<(PathBuf, Repo)>,
    run: Option<GitRun>,
    runs: u64,
    pacer: Pacer,
    timer: slint::Timer,
    /// The watch of `.git` (its folder): kept while the same repository shows.
    watch: Option<(PathBuf, notify::RecommendedWatcher)>,
    /// Why the folder shown has no marks although in a repository (no git, too old, refused,
    /// too slow): said in the status bar's Git part, beside the item count. Meanwhile it counts
    /// as no repository (no column, no Git groups).
    problem: Option<String>,
}

#[cfg(test)]
thread_local! {
    /// Runs started (tests: none outside repositories).
    pub static STARTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl View {
    /// The folder `dir` shows, in `repo` (`None`: not a repository, or not a folder): a new one
    /// runs at once, the same one again after the wait (it changed); outside a repository
    /// everything stops.
    pub fn git_listed(&self, shown: Option<(PathBuf, Repo)>) {
        let Some((dir, repo)) = shown else { return self.git_off() };
        if self.0.git.borrow().at.as_ref().is_some_and(|(d, r)| *d == dir && *r == repo) {
            return self.git_soon();
        }
        let was_repo = {
            let mut git = self.0.git.borrow_mut();
            git.run = None;
            git.pacer = Pacer::default();
            git.timer.stop();
            if git.watch.as_ref().is_some_and(|(d, _)| *d != repo.git_dir) {
                git.watch = None;
            }
            let was = git.problem.take().is_none() && git.at.is_some();
            git.at = Some((dir, repo));
            was
        };
        let had = self.0.data.borrow_mut().folder_git.take().is_some();
        if !was_repo {
            // The Git column shows in repositories.
            self.sync_columns();
        }
        if had {
            self.git_rows_changed();
        }
        self.git_start();
    }

    /// Not in a repository: no run, no watch, no marks.
    fn git_off(&self) {
        let was = self.in_repo();
        {
            let mut git = self.0.git.borrow_mut();
            git.run = None;
            git.watch = None;
            git.timer.stop();
            git.pacer = Pacer::default();
            git.at = None;
            git.problem = None;
        }
        let had = self.0.data.borrow_mut().folder_git.take().is_some();
        self.git_repo_changed(was, had);
    }

    /// After a change that may end the marks: the column and the Git groups go when the folder
    /// no longer counts as a repository (`was` it did), rows are drawn again when marks
    /// went (`had`).
    fn git_repo_changed(&self, was: bool, had: bool) {
        if was != self.in_repo() {
            self.sync_columns();
            // A grouping by Git falls back to none (and comes back).
            self.resort(false);
            self.update_status();
        } else if had {
            self.git_rows_changed();
        } else {
            self.update_status();
        }
    }

    /// Whether a repository folder shows with its marks to come (its Git column, its groups):
    /// not when git is missing or refused.
    pub fn in_repo(&self) -> bool {
        let git = self.0.git.borrow();
        git.at.is_some() && git.problem.is_none()
    }

    /// The folder or its `.git` changed: a run after the wait.
    pub fn git_soon(&self) {
        let mut git = self.0.git.borrow_mut();
        if git.at.is_none() || !git.pacer.event() {
            return;
        }
        let pane = self.0.id;
        git.timer.start(
            slint::TimerMode::SingleShot,
            SETTLE,
            crate::windows::bind_mut(move || {
                crate::panes::with_id(pane, |p| p.view.git_start());
            }),
        );
    }

    fn git_start(&self) {
        let (repo, id, cancel, watch) = {
            let mut git = self.0.git.borrow_mut();
            let Some((_, repo)) = git.at.clone() else { return };
            if !git.pacer.start() {
                return;
            }
            git.runs += 1;
            let (id, cancel) = (git.runs, Arc::new(AtomicBool::new(false)));
            git.run = Some(GitRun { id, cancel: cancel.clone() });
            (repo, id, cancel, git.watch.is_none())
        };
        #[cfg(test)]
        STARTED.with(|s| s.set(s.get() + 1));
        let settings = crate::git::settings();
        let (pane, window) = (self.0.id, self.0.window.clone());
        let spawned = std::thread::Builder::new().name("gezik-git".into()).spawn(move || {
            let timeout = Duration::from_secs(settings.timeout.into());
            let outcome = crate::git::status(&repo.real, settings.ignored, timeout, &cancel);
            let watch = watch
                .then(|| {
                    let window = window.clone();
                    crate::git::watch(&repo, move || {
                        let _ = window.upgrade_in_event_loop(move |_| {
                            crate::panes::with_id(pane, |p| p.view.git_soon());
                        });
                    })
                })
                .flatten();
            let _ = window.upgrade_in_event_loop(move |_| {
                crate::panes::with_id(pane, |p| p.view.git_arrived(id, outcome, watch));
            });
        });
        if spawned.is_err() {
            // Nothing runs: the next change tries again.
            let mut git = self.0.git.borrow_mut();
            git.run = None;
            git.pacer.done();
        }
    }

    /// A run's result, on the UI thread (an older run's is dropped).
    pub fn git_arrived(&self, id: u64, outcome: Outcome, watch: Option<notify::RecommendedWatcher>) {
        let (dir, root, real, again) = {
            let mut git = self.0.git.borrow_mut();
            if !git.run.as_ref().is_some_and(|run| run.id == id) {
                return;
            }
            git.run = None;
            let Some((dir, repo)) = git.at.clone() else { return };
            if let Some(watch) = watch
                && git.watch.is_none()
            {
                git.watch = Some((repo.git_dir.clone(), watch));
            }
            (dir, repo.root, repo.real, git.pacer.done())
        };
        let was = self.in_repo();
        let problem = match outcome {
            Outcome::Done(status) => {
                let marks = gezik_core::git::folder_marks(&status, &crate::git::prefix(&root, &real));
                let text = gezik_core::git::status_text(&status, &marks);
                self.0.data.borrow_mut().folder_git = Some(Rc::new(FolderGit { dir, marks, text }));
                None
            }
            Outcome::Missing => Some(gezik_core::git::missing_text()),
            // TRANSLATORS: {} is git's own message (not translated).
            Outcome::Failed(why) => Some(t!("Git: {}", why)),
            Outcome::TimedOut => Some(t!("Git status took too long ([git] timeout)").into_owned()),
            Outcome::Stopped => return,
        };
        // No status: no marks, never the last run's (they would be wrong now).
        let had = problem.is_some() && self.0.data.borrow_mut().folder_git.take().is_some();
        let done = problem.is_none();
        self.0.git.borrow_mut().problem = problem;
        if done {
            self.git_rows_changed();
            if was != self.in_repo() {
                self.sync_columns();
            }
        } else {
            self.git_repo_changed(was, had);
        }
        if again {
            self.git_soon();
        }
    }

    /// The marks changed: rows sorted, grouped or filtered by them take their places, the
    /// others on screen are drawn again; the status bar says the branch.
    fn git_rows_changed(&self) {
        if self.git_needed() {
            self.resort(false);
        } else {
            self.redraw_visible();
        }
        self.update_status();
    }

    /// Whether the sort, the grouping or the filter use the marks.
    fn git_needed(&self) -> bool {
        self.sort().key == SortKey::Git
            || self.group_by() == GroupBy::Git
            || self.0.filter.borrow().as_ref().is_some_and(|f| !f.git.is_empty())
    }

    /// The status bar's Git part for the folder shown ("" elsewhere): the branch and changes,
    /// or why there are none.
    pub(super) fn git_text(&self) -> String {
        if let Some(problem) = self.0.git.borrow().problem.clone() {
            return problem;
        }
        let data = self.0.data.borrow();
        match (&data.listing, &data.folder_git) {
            (Listing::Files(dir, _), Some(g)) if g.dir == *dir => g.text.clone(),
            _ => String::new(),
        }
    }

    /// The marks rows of `dir` sort and group by (none known: unchanged).
    pub(super) fn git_keys(&self, dir: &std::path::Path) -> impl Fn(&Entry) -> Option<GitMark> + use<> {
        let known = self.0.data.borrow().folder_git.clone().filter(|g| g.dir == dir);
        move |e: &Entry| known.as_ref().and_then(|g| g.marks.of(&e.name))
    }

    /// Whether a row of `dir` passes the `git:` words `terms`: all do while the first run of a
    /// repository goes on; outside one rows have no mark.
    pub(super) fn git_filter(&self, dir: &std::path::Path, terms: &[GitTerm]) -> impl Fn(&Entry) -> bool + use<> {
        let known = self.0.data.borrow().folder_git.clone().filter(|g| g.dir == dir);
        let waiting = known.is_none() && self.in_repo();
        let terms = terms.to_vec();
        move |e: &Entry| {
            terms.is_empty()
                || waiting
                || gezik_core::git_filter::matches(&terms, known.as_ref().and_then(|g| g.marks.of(&e.name)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Media;
    use crate::view::listing::files;
    use gezik_core::git::GitStatus;

    #[test]
    fn ten_events_make_one_run_and_events_during_it_one_more() {
        let mut p = Pacer::default();
        let waits = (0..10).filter(|_| p.event()).count();
        assert_eq!(waits, 1, "one wait for ten events");
        assert!(p.start());
        assert!((0..10).all(|_| !p.event()), "no wait while it runs");
        assert!(!p.start(), "never two at once");
        assert!(p.done(), "events came: once more");
        assert!(p.event() && p.start());
        assert!(!p.done(), "quiet: done");
        assert!(p.event(), "the next event waits again");
    }

    fn view_of(names: &[&str]) -> View {
        let view = View::new(crate::panes::next_id(), slint::Weak::default(), Media::idle(), Default::default(), None);
        let listing = files("/r/src", names);
        let Listing::Files(_, full) = &listing else { unreachable!() };
        let mut data = view.0.data.borrow_mut();
        data.full = full.clone();
        data.listing = listing;
        drop(data);
        view
    }

    fn repo() -> Repo {
        Repo { root: "/r".into(), git_dir: "/r/.git".into(), common: "/r/.git".into(), real: "/r/src".into() }
    }

    #[test]
    fn outside_a_repository_nothing_runs() {
        let view = view_of(&["a.txt"]);
        let started = || STARTED.with(std::cell::Cell::get);
        let before = started();
        view.git_listed(None);
        view.git_soon();
        assert_eq!(started(), before);
        assert!(!view.in_repo());
        view.git_listed(Some(("/r/src".into(), repo())));
        assert_eq!(started(), before + 1, "a repository folder runs at once");
        let id = view.0.git.borrow().runs;
        view.git_listed(Some(("/r/src".into(), repo())));
        assert_eq!(started(), before + 1, "the same folder again waits (and one runs)");
        // An older run's result is dropped; this one's marks show.
        view.git_arrived(id - 1, Outcome::Done(GitStatus::default()), None);
        assert!(view.0.data.borrow().folder_git.is_none());
        let status = GitStatus { entries: vec![("src/a.txt".into(), GitMark::Modified)], ..GitStatus::default() };
        view.git_arrived(id, Outcome::Done(status), None);
        assert_eq!(
            view.git_keys(std::path::Path::new("/r/src"))(&view.0.data.borrow().full[0]),
            Some(GitMark::Modified)
        );
        view.git_listed(None);
        assert!(view.0.data.borrow().folder_git.is_none() && !view.in_repo(), "left: forgotten");
    }

    #[test]
    fn no_git_drops_the_marks_column_and_branch() {
        let view = view_of(&["a.txt", "new.txt"]);
        view.git_listed(Some(("/r/src".into(), repo())));
        let id = view.0.git.borrow().runs;
        let status = GitStatus {
            branch: Some("main".into()),
            entries: vec![("src/a.txt".into(), GitMark::Modified)],
            ..GitStatus::default()
        };
        view.git_arrived(id, Outcome::Done(status), None);
        assert!(view.git_text().starts_with("⎇ main"));
        // `[git] program` now names no git: the same folder again.
        view.0.git.borrow_mut().pacer = Pacer::default();
        view.git_start();
        let id = view.0.git.borrow().runs;
        view.git_arrived(id, Outcome::Missing, None);
        assert!(view.0.data.borrow().folder_git.is_none(), "no stale marks");
        assert!(!view.in_repo(), "no Git column, no Git groups");
        assert_eq!(view.git_text(), gezik_core::git::missing_text(), "said beside the item count");
        let a = view.0.data.borrow().full[0].clone();
        assert_eq!(view.git_keys(std::path::Path::new("/r/src"))(&a), None);
        // Group by Git falls back to none.
        view.0.current.set(gezik_core::view::ViewSettings { group: GroupBy::Git, ..view.0.current.get() });
        assert_eq!(view.group_by(), GroupBy::None);
    }

    #[test]
    fn leaving_or_closing_stops_the_run() {
        let view = view_of(&["a.txt"]);
        view.git_listed(Some(("/r/src".into(), repo())));
        let cancel = view.0.git.borrow().run.as_ref().unwrap().cancel.clone();
        view.git_listed(Some(("/elsewhere".into(), Repo { root: "/elsewhere".into(), ..repo() })));
        assert!(cancel.load(Ordering::Relaxed), "another folder: stopped");
        let cancel = view.0.git.borrow().run.as_ref().unwrap().cancel.clone();
        drop(view);
        assert!(cancel.load(Ordering::Relaxed), "the pane closed: stopped");
    }

    #[test]
    fn git_words_pass_everything_until_the_first_run_ends() {
        let view = view_of(&["a.txt", "b.txt"]);
        let terms = gezik_core::git_filter::split("git:modified").1;
        let dir = std::path::Path::new("/r/src");
        let a = view.0.data.borrow().full[0].clone();
        assert!(!view.git_filter(dir, &terms)(&a), "not a repository: no marks");
        view.git_listed(Some(("/r/src".into(), repo())));
        assert!(view.git_filter(dir, &terms)(&a), "on its way");
        let id = view.0.git.borrow().runs;
        let status = GitStatus { entries: vec![("src/b.txt".into(), GitMark::Modified)], ..GitStatus::default() };
        view.git_arrived(id, Outcome::Done(status), None);
        let keep = view.git_filter(dir, &terms);
        assert!(!keep(&a) && keep(&view.0.data.borrow().full[1].clone()));
    }
}
