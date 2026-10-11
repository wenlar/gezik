//! The folder sync's side of the compare (11e, spec 11 §8): the rows' directions turned into
//! the job's steps, the summary question's lines and the trash rule, and the saved profiles
//! (`[[sync-profiles]]`) with what writes them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gezik_config::settings_writer::SettingsChange;
use gezik_config::sync_profiles::{SYNC_PROFILES_MAX, SyncProfile};
use gezik_core::compare::{self, Act, Dir, Kind, Node, Row, Tree};
use gezik_ops::{Report, Seen, SyncStep};

/// The job's steps and, for each, its row and whether it acts on the right side.
#[derive(Debug, Default, Clone)]
pub struct Planned {
    pub steps: Vec<SyncStep>,
    pub rows: Vec<(u32, bool)>,
    /// Folders inside a copied folder that could not be read: left out.
    pub unread: usize,
}

fn seen(node: &Node) -> Seen {
    Seen { is_dir: node.kind == Kind::Folder, size: node.size, modified: node.modified }
}

/// The steps for `rows` as `dirs` say: copies to the right, then to the left, then what goes
/// to the trash. A folder only one side has is copied as the compare read it (its files one by
/// one, the filter's rules kept) and trashed as one item.
pub fn plan(left: &Tree, right: &Tree, rows: &[Row], dirs: &[Dir], roots: (&Path, &Path)) -> Planned {
    let mut copies = [Planned::default(), Planned::default()];
    let mut trash = Planned::default();
    // The folder holding the rows at each depth, on each side.
    let mut parents: Vec<(PathBuf, PathBuf)> = vec![(roots.0.to_path_buf(), roots.1.to_path_buf())];
    for (i, row) in rows.iter().enumerate() {
        let depth = usize::from(row.depth);
        parents.truncate(depth + 1);
        let Some((lp, rp)) = parents.get(depth).cloned() else { continue };
        let l = row.left.map(|n| &left.nodes[n as usize]);
        let r = row.right.map(|n| &right.nodes[n as usize]);
        if let (true, Some(a), Some(b)) = (row.folder, l, r) {
            parents.push((lp.join(&*a.name), rp.join(&*b.name)));
        }
        let at = u32::try_from(i).unwrap_or(u32::MAX);
        let act = compare::act(row, dirs.get(i).copied().unwrap_or_default());
        match (act, l, r) {
            (Some(Act::CopyRight), Some(a), _) => {
                let to = r.map_or_else(|| rp.join(&*a.name), |b| rp.join(&*b.name));
                copy(left, a, (lp.join(&*a.name), to), r.map(seen), (at, true), &mut copies[0]);
            }
            (Some(Act::CopyLeft), _, Some(b)) => {
                let to = l.map_or_else(|| lp.join(&*b.name), |a| lp.join(&*a.name));
                copy(right, b, (rp.join(&*b.name), to), l.map(seen), (at, false), &mut copies[1]);
            }
            (Some(Act::TrashRight), _, Some(b)) => {
                trash.steps.push(SyncStep::Trash { path: rp.join(&*b.name), seen: seen(b) });
                trash.rows.push((at, true));
            }
            (Some(Act::TrashLeft), Some(a), _) => {
                trash.steps.push(SyncStep::Trash { path: lp.join(&*a.name), seen: seen(a) });
                trash.rows.push((at, false));
            }
            _ => {}
        }
    }
    let [mut all, to_left] = copies;
    for part in [to_left, trash] {
        all.steps.extend(part.steps);
        all.rows.extend(part.rows);
        all.unread += part.unread;
    }
    all
}

/// Node `node` of `tree` copied from `from` to `to` (`replaces`: the file there as seen); a
/// folder with what the compare read in it.
fn copy(
    tree: &Tree,
    node: &Node,
    (from, to): (PathBuf, PathBuf),
    replaces: Option<Seen>,
    row: (u32, bool),
    out: &mut Planned,
) {
    match node.kind {
        Kind::File | Kind::OnlineFile => {
            out.steps.push(SyncStep::Copy { from, to, seen: seen(node), replaces });
            out.rows.push(row);
        }
        Kind::Folder => {
            out.steps.push(SyncStep::Folder { path: to.clone() });
            out.rows.push(row);
            for child in &tree.nodes[node.first as usize..(node.first + node.count) as usize] {
                let name = &*child.name;
                copy(tree, child, (from.join(name), to.join(name)), None, row, out);
            }
        }
        Kind::Unread => out.unread += 1,
    }
}

/// What the summary question counts, per side (0 left, 1 right).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub files: [usize; 2],
    pub bytes: [u64; 2],
    pub replacing: [usize; 2],
    pub trashed: [usize; 2],
}

pub fn summary(planned: &Planned) -> Summary {
    let mut s = Summary::default();
    for (step, &(_, right)) in planned.steps.iter().zip(&planned.rows) {
        let side = usize::from(right);
        match step {
            SyncStep::Copy { seen, replaces, .. } => {
                s.files[side] += 1;
                s.bytes[side] += seen.size;
                s.replacing[side] += usize::from(replaces.is_some());
            }
            SyncStep::Trash { .. } => s.trashed[side] += 1,
            SyncStep::Folder { .. } => {}
        }
    }
    s
}

/// The steps a side without a trash cannot take: its trash steps always (deletes go only to the
/// trash), its replacing copies unless `anyway` (the old file would be gone for good).
pub fn keep_safe(planned: Planned, bins: [bool; 2], anyway: bool) -> Planned {
    let mut out = Planned { unread: planned.unread, ..Planned::default() };
    for (step, row) in planned.steps.into_iter().zip(planned.rows) {
        let bin = bins[usize::from(row.1)];
        let refused = match &step {
            SyncStep::Trash { .. } => !bin,
            SyncStep::Copy { replaces: Some(_), .. } => !bin && !anyway,
            _ => false,
        };
        if !refused {
            out.steps.push(step);
            out.rows.push(row);
        }
    }
    out
}

/// What became of each row's steps once the job ended: `None` done, else why not (the first
/// reason of the row's steps).
pub fn outcomes(planned: &Planned, report: &Report) -> HashMap<u32, Option<String>> {
    let results: HashSet<&Path> = report.results.iter().map(PathBuf::as_path).collect();
    let mut why: HashMap<&Path, &str> = HashMap::new();
    for f in report.failures.iter().chain(&report.skipped) {
        why.entry(f.path.as_path()).or_insert(f.message.as_str());
    }
    let mut out: HashMap<u32, Option<String>> = HashMap::new();
    for (step, &(row, _)) in planned.steps.iter().zip(&planned.rows) {
        let failed = match step {
            SyncStep::Folder { path } => (!results.contains(path.as_path())).then(|| why.get(path.as_path()).copied()),
            SyncStep::Copy { from, to, .. } => (!results.contains(to.as_path()))
                .then(|| why.get(to.as_path()).or_else(|| why.get(from.as_path())).copied()),
            SyncStep::Trash { path, .. } => {
                if report.cancelled && !why.contains_key(path.as_path()) && path.exists() {
                    Some(Some("Cancelled"))
                } else {
                    why.get(path.as_path()).map(|w| Some(*w))
                }
            }
        };
        let entry = out.entry(row).or_insert(None);
        if let Some(reason) = failed
            && entry.is_none()
        {
            let reason = reason.unwrap_or(if report.cancelled { "Cancelled" } else { "Skipped" });
            *entry = Some(reason.to_owned());
        }
    }
    out
}

/// `1 file`, `12 files`.
pub fn files_text(n: usize) -> String {
    if n == 1 { "1 file".to_owned() } else { format!("{} files", crate::preview::with_commas(n)) }
}

fn items_text(n: usize) -> String {
    if n == 1 { "1 item".to_owned() } else { format!("{} items", crate::preview::with_commas(n)) }
}

pub fn bin_name() -> &'static str {
    if cfg!(windows) { "the Recycle Bin" } else { "the Trash" }
}

/// The summary question's lines for `s` (what will be done) and what a side without a trash
/// leaves out (`refused`: its trash steps and replacing copies, per side).
pub fn lines(s: &Summary, roots: [&Path; 2], bins: [bool; 2], refused: &Summary) -> Vec<String> {
    let mut lines = Vec::new();
    // To the right first, as the job does.
    for side in [1, 0] {
        if s.files[side] > 0 {
            let mut line = format!(
                "Copy {} ({}) to {}",
                files_text(s.files[side]),
                crate::view_options::size_text(s.bytes[side]),
                roots[side].display()
            );
            if s.replacing[side] > 0 {
                line.push_str(&format!(", replacing {}", crate::preview::with_commas(s.replacing[side])));
            }
            lines.push(line);
        }
    }
    for side in [1, 0] {
        if s.trashed[side] > 0 {
            lines.push(format!("Move {} on {} to {}", items_text(s.trashed[side]), roots[side].display(), bin_name()));
        }
    }
    for side in [1, 0] {
        if bins[side] {
            continue;
        }
        let (drive, none) = (roots[side].display(), no_bin());
        if refused.trashed[side] > 0 {
            let n = refused.trashed[side];
            let what = if n == 1 {
                "1 deletion there is".to_owned()
            } else {
                format!("{} deletions there are", crate::preview::with_commas(n))
            };
            lines.push(format!("{drive} {none}: {what} left out"));
        }
        if refused.replacing[side] > 0 {
            lines.push(format!(
                "{} on {drive} would be replaced without a way back",
                files_text(refused.replacing[side])
            ));
        }
    }
    lines
}

fn no_bin() -> &'static str {
    if cfg!(windows) { "has no Recycle Bin" } else { "has no trash" }
}

// The saved profiles.

thread_local! {
    static PROFILES: RefCell<Vec<SyncProfile>> = const { RefCell::new(Vec::new()) };
}

/// settings.toml changed.
pub fn set_profiles(profiles: Vec<SyncProfile>) {
    PROFILES.with(|p| *p.borrow_mut() = profiles);
}

pub fn profiles() -> Vec<SyncProfile> {
    PROFILES.with(|p| p.borrow().clone())
}

/// `list` with `profile` in it: one of the same name (ignoring case) replaced, else added;
/// `None` when the list is full.
pub fn with_profile(mut list: Vec<SyncProfile>, profile: SyncProfile) -> Option<Vec<SyncProfile>> {
    match list.iter().position(|p| p.name.to_lowercase() == profile.name.to_lowercase()) {
        Some(i) => list[i] = profile,
        None if list.len() >= SYNC_PROFILES_MAX => return None,
        None => list.push(profile),
    }
    Some(list)
}

/// Writes `list` to settings.toml; the palette shows it once written.
pub fn write_profiles(list: Vec<SyncProfile>) {
    let Some(store) = crate::windows::config() else { return set_profiles(list) };
    let window = crate::windows::current();
    store.write_settings(SettingsChange::SyncProfiles(list.clone()), move |result| {
        crate::windows::post(window, move || match result {
            Ok(()) => set_profiles(list),
            Err(warning) => crate::panes::active_view().note(warning.to_string()),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::compare::{Mode, Options, build, default_dir};
    use gezik_ops::Failure;

    /// A tree from `(path, size, time)`; a path ending in `/` is a folder, in `!` unread.
    fn tree(items: &[(&str, u64, i64)]) -> Tree {
        fn add(tree: &mut Tree, at: u32, prefix: &str, items: &[(&str, u64, i64)]) {
            let mut mine: Vec<(String, Kind, u64, i64)> = Vec::new();
            for (path, size, time) in items {
                let Some(rest) = path.strip_prefix(prefix) else { continue };
                if rest.is_empty() || rest[..rest.len() - 1].contains('/') {
                    continue;
                }
                let (name, kind) = match rest.chars().last() {
                    Some('/') => (&rest[..rest.len() - 1], Kind::Folder),
                    Some('!') => (&rest[..rest.len() - 1], Kind::Unread),
                    _ => (rest, Kind::File),
                };
                mine.push((name.to_owned(), kind, *size, *time));
            }
            mine.sort_by(|a, b| a.0.cmp(&b.0));
            let first = tree.nodes.len() as u32;
            for (name, kind, size, time) in &mine {
                tree.nodes.push(Node {
                    name: name.as_str().into(),
                    key: None,
                    size: *size,
                    modified: *time,
                    parent: at,
                    first: 0,
                    count: 0,
                    kind: *kind,
                });
            }
            tree.nodes[at as usize].first = first;
            tree.nodes[at as usize].count = mine.len() as u32;
            for (k, (name, kind, ..)) in mine.iter().enumerate() {
                if *kind == Kind::Folder {
                    add(tree, first + k as u32, &format!("{prefix}{name}/"), items);
                }
            }
        }
        let mut tree = Tree::default();
        tree.nodes.push(Node {
            name: "".into(),
            key: None,
            size: 0,
            modified: 0,
            parent: 0,
            first: 0,
            count: 0,
            kind: Kind::Folder,
        });
        add(&mut tree, 0, "", items);
        tree
    }

    const T: i64 = 1_700_000_000;

    fn roots() -> (PathBuf, PathBuf) {
        (PathBuf::from("L"), PathBuf::from("R"))
    }

    fn planned(mode: Mode) -> (Planned, Vec<Row>) {
        let left = tree(&[
            ("a.txt", 10, T + 60),
            ("d/", 0, T),
            ("d/only.txt", 3, T),
            ("new/", 0, T),
            ("new/x.txt", 5, T),
            ("new/sub/", 0, T),
            ("new/sub/y.txt", 7, T),
            ("new/locked!", 0, T),
        ]);
        let right = tree(&[("a.txt", 4, T), ("d/", 0, T), ("d/stale.txt", 2, T), ("b.txt", 1, T)]);
        let rows = build(&left, &right, &Options::default());
        let dirs: Vec<Dir> = rows.iter().map(|r| default_dir(r, mode)).collect();
        let (l, r) = roots();
        (plan(&left, &right, &rows, &dirs, (&l, &r)), rows)
    }

    #[test]
    fn mirror_copies_right_then_trashes_and_counts() {
        let (p, _) = planned(Mode::Mirror);
        let (l, r) = roots();
        let s = |a: u64| Seen { is_dir: false, size: a, modified: T };
        assert_eq!(
            p.steps,
            [
                SyncStep::Copy {
                    from: l.join("a.txt"),
                    to: r.join("a.txt"),
                    seen: Seen { modified: T + 60, ..s(10) },
                    replaces: Some(s(4))
                },
                SyncStep::Copy {
                    from: l.join("d").join("only.txt"),
                    to: r.join("d").join("only.txt"),
                    seen: s(3),
                    replaces: None
                },
                SyncStep::Folder { path: r.join("new") },
                SyncStep::Folder { path: r.join("new").join("sub") },
                SyncStep::Copy {
                    from: l.join("new").join("sub").join("y.txt"),
                    to: r.join("new").join("sub").join("y.txt"),
                    seen: s(7),
                    replaces: None
                },
                SyncStep::Copy {
                    from: l.join("new").join("x.txt"),
                    to: r.join("new").join("x.txt"),
                    seen: s(5),
                    replaces: None
                },
                SyncStep::Trash { path: r.join("b.txt"), seen: s(1) },
                SyncStep::Trash { path: r.join("d").join("stale.txt"), seen: s(2) },
            ]
        );
        assert_eq!(p.unread, 1, "the unreadable folder inside the copied one is left out");
        assert_eq!(summary(&p), Summary { files: [0, 4], bytes: [0, 25], replacing: [0, 1], trashed: [0, 2] });
    }

    #[test]
    fn two_way_copies_both_ways_and_trashes_nothing() {
        let (p, _) = planned(Mode::TwoWay);
        let s = summary(&p);
        assert_eq!((s.files, s.replacing, s.trashed), ([2, 4], [0, 1], [0, 0]));
        let (_, r) = roots();
        assert!(matches!(&p.steps[0], SyncStep::Copy { to, .. } if *to == r.join("a.txt")), "to the right first");
    }

    #[test]
    fn a_side_without_a_trash_loses_its_deletions_and_replaces_only_when_allowed() {
        let (p, _) = planned(Mode::Mirror);
        let all = summary(&p);
        let kept = keep_safe(planned(Mode::Mirror).0, [true, false], false);
        let s = summary(&kept);
        assert_eq!((s.trashed, s.replacing, s.files), ([0, 0], [0, 0], [0, 3]));
        let refused = Summary {
            files: [0, all.files[1] - s.files[1]],
            replacing: [0, all.replacing[1] - s.replacing[1]],
            trashed: [0, all.trashed[1] - s.trashed[1]],
            bytes: [0, 0],
        };
        let roots = [Path::new("C:/L"), Path::new("E:/R")];
        let said = lines(&s, roots, [true, false], &refused);
        assert!(said[0].starts_with("Copy 3 files (") && said[0].ends_with("to E:/R"), "{said:?}");
        assert!(said.contains(&format!("E:/R {}: 2 deletions there are left out", no_bin())), "{said:?}");
        assert!(said.contains(&"1 file on E:/R would be replaced without a way back".to_owned()), "{said:?}");
        let anyway = summary(&keep_safe(planned(Mode::Mirror).0, [true, false], true));
        assert_eq!((anyway.replacing, anyway.trashed), ([0, 1], [0, 0]));
    }

    #[test]
    fn a_user_direction_turns_a_skip_into_a_delete_and_folders_hand_theirs_down() {
        let left = tree(&[("d/", 0, T), ("d/a.txt", 1, T), ("d/b.txt", 1, T)]);
        let right = tree(&[("d/", 0, T), ("d/a.txt", 1, T + 60), ("d/c.txt", 1, T)]);
        let rows = build(&left, &right, &Options::default());
        let mut dirs: Vec<Dir> = rows.iter().map(|r| default_dir(r, Mode::LeftToRight)).collect();
        // The folder's ← goes to every row under it: a.txt copied left, b.txt trashed on the left,
        // c.txt copied left.
        for at in compare::units_under(&rows, 0) {
            dirs[at as usize] = Dir::ToLeft;
        }
        let (l, r) = roots();
        let p = plan(&left, &right, &rows, &dirs, (&l, &r));
        let s = summary(&p);
        assert_eq!((s.files, s.replacing, s.trashed), ([2, 0], [1, 0], [1, 0]));
    }

    #[test]
    fn the_report_tells_each_row_what_became_of_it() {
        let (p, _) = planned(Mode::Mirror);
        let (l, r) = roots();
        let mut report = Report {
            kind: gezik_ops::TaskKind::Sync,
            cancelled: false,
            failures: vec![Failure {
                path: l.join("new").join("x.txt"),
                message: "Access denied".into(),
                denied: true,
            }],
            skipped: vec![Failure {
                path: r.join("b.txt"),
                message: "Changed after the compare; left alone".into(),
                denied: false,
            }],
            skipped_changed: 0,
            no_trash: Vec::new(),
            unchecked: Vec::new(),
            results: vec![
                r.join("a.txt"),
                r.join("d").join("only.txt"),
                r.join("new"),
                r.join("new").join("sub"),
                r.join("new").join("sub").join("y.txt"),
            ],
            changed_dirs: Vec::new(),
            moved: Vec::new(),
            as_admin: Vec::new(),
        };
        let rows = outcomes(&p, &report);
        let row_of = |i: usize| p.rows[i].0;
        assert_eq!(rows[&row_of(0)], None, "a.txt");
        assert_eq!(rows[&row_of(2)], Some("Access denied".to_owned()), "the folder: one of its files failed");
        assert_eq!(rows[&row_of(6)], Some("Changed after the compare; left alone".to_owned()), "b.txt");
        assert_eq!(rows[&row_of(7)], None, "stale.txt went");
        report.failures.clear();
        report.results.push(r.join("new").join("x.txt"));
        assert_eq!(outcomes(&p, &report)[&row_of(2)], None);
    }

    #[test]
    fn a_profile_replaces_one_of_its_name_and_the_list_has_a_limit() {
        let profile = |name: &str| SyncProfile {
            name: name.into(),
            left: "/a".into(),
            right: "/b".into(),
            options: Options::default(),
            filter: String::new(),
        };
        let list =
            with_profile(vec![profile("Photos")], SyncProfile { left: "/c".into(), ..profile("photos") }).unwrap();
        assert_eq!((list.len(), list[0].left.as_str()), (1, "/c"));
        let full: Vec<SyncProfile> = (0..SYNC_PROFILES_MAX).map(|i| profile(&i.to_string())).collect();
        assert!(with_profile(full.clone(), profile("new")).is_none());
        assert!(with_profile(full, profile("3")).is_some());
    }

    #[test]
    fn a_mirror_of_two_real_folders_runs_as_one_job_and_one_undo_takes_it_back() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        let root = std::env::temp_dir().join(format!("gezik-sync-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        gezik_platform::trash::purge_at_thread_end(&root);
        let (l, r) = (root.join("L"), root.join("R"));
        let write = |path: &Path, text: &str, secs: u64| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
            let file = std::fs::File::options().write(true).open(path).unwrap();
            file.set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)).unwrap();
        };
        let t = 1_700_000_000;
        write(&l.join("same.txt"), "s", t);
        write(&r.join("same.txt"), "s", t);
        write(&l.join("newer.txt"), "new", t + 600);
        write(&r.join("newer.txt"), "old", t);
        write(&l.join("pics/a.jpg"), "a", t);
        write(&l.join("pics/skip.tmp"), "x", t);
        write(&r.join("stale/z.txt"), "z", t);
        let filter = gezik_core::file_filter::FileFilter::parse("| *.tmp", false).unwrap();
        let rules = gezik_search::tree_scan::Rules {
            subfolders: true,
            filter: &filter,
            root_taken: true,
            shown: (true, true),
            case_sensitive: true,
            max_items: 1000,
        };
        let scan =
            |dir: &Path| gezik_search::tree_scan::scan(dir, &rules, &AtomicBool::new(false), &AtomicUsize::new(0)).tree;
        let (left, right) = (scan(&l), scan(&r));
        let rows = build(&left, &right, &Options::default());
        let dirs: Vec<Dir> = rows.iter().map(|row| default_dir(row, Mode::Mirror)).collect();
        let planned = plan(&left, &right, &rows, &dirs, (&l, &r));
        let task = gezik_ops::SyncTask::new(l.clone(), r.clone(), planned.steps.clone(), false);
        let engine = gezik_ops::Engine::new(gezik_ops::Settings::default(), || {});
        let job = engine.submit_chain(vec![Box::new(task)], Some(gezik_ops::sync_label(&l)));
        let wait = |job: gezik_ops::JobId| loop {
            let report = engine.drain().into_iter().find_map(|e| match e {
                gezik_ops::Event::Finished { job: j, report } if j == job => Some(report),
                gezik_ops::Event::Conflicts { job: j, .. } => panic!("no conflicts expected in job {j}"),
                _ => None,
            });
            if let Some(report) = report {
                break report;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let report = wait(job);
        assert!(report.failures.is_empty() && report.skipped.is_empty(), "{report:?}");
        let read = |p: &Path| std::fs::read_to_string(p).unwrap();
        assert_eq!(read(&r.join("newer.txt")), "new");
        assert_eq!(read(&r.join("pics/a.jpg")), "a");
        assert!(!r.join("pics/skip.tmp").exists(), "the filter's file is not copied");
        assert!(!r.join("stale").exists());
        let marked = outcomes(&planned, &report);
        assert!(!marked.is_empty() && marked.values().all(Option::is_none), "{marked:?}");
        let undo = wait(engine.undo().unwrap());
        assert!(undo.failures.is_empty(), "{undo:?}");
        assert_eq!(read(&r.join("newer.txt")), "old");
        assert_eq!(read(&r.join("stale/z.txt")), "z");
        assert!(!r.join("pics").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
