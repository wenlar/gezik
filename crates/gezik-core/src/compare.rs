//! The folder compare (spec 11 §7): two folder trees side by side, matched by name and told
//! apart by size, date and (when asked) content. Pure: the scan (`gezik-search::tree_scan`)
//! fills the trees, the layer shows the rows.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::SystemTime;

/// The show buttons in their order (TC): left newer or only left, the same, different, right
/// newer or only right, on one side only, on both sides.
pub const SHOW: [&str; 6] = ["→", "=", "≠", "←", "1×", "2×"];
const ONE_SIDE: usize = 4;
const BOTH_SIDES: usize = 5;

/// A time not known.
pub const NO_TIME: i64 = i64::MIN;
/// FAT and exFAT keep times in 2 second steps: always a tolerance, never a setting.
const FAT_STEP: u64 = 2;
const HOUR: u64 = 3600;

/// The compare's options (`[compare]` in state.toml).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The show buttons, in `SHOW`'s order.
    pub show: [bool; 6],
    pub subfolders: bool,
    /// Same-size pairs whose dates differ (all same-size pairs with `ignore_dates`) are read.
    pub content: bool,
    pub ignore_dates: bool,
    /// Exactly an hour apart (±2 s) is the same time: FAT's daylight saving shift.
    pub ignore_hour: bool,
}

impl Default for Options {
    fn default() -> Options {
        // `=` off: thousands of equal rows would bury the differences.
        Options {
            show: [true, false, true, true, true, true],
            subfolders: true,
            content: false,
            ignore_dates: false,
            ignore_hour: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    LeftOnly,
    RightOnly,
    Equal,
    LeftNewer,
    RightNewer,
    /// The same date (or dates ignored), another size or content.
    Differs,
    /// A file on one side, a folder on the other.
    Mismatch,
    Unreadable,
}

impl State {
    pub fn symbol(self) -> &'static str {
        match self {
            State::LeftOnly | State::LeftNewer => "→",
            State::RightOnly | State::RightNewer => "←",
            State::Equal => "=",
            State::Differs => "≠",
            State::Mismatch | State::Unreadable => "!",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            State::LeftOnly => "Only left",
            State::RightOnly => "Only right",
            State::Equal => "Same",
            State::LeftNewer => "Left newer",
            State::RightNewer => "Right newer",
            State::Differs => "Different",
            State::Mismatch => "File and folder",
            State::Unreadable => "Unreadable",
        }
    }

    /// Its show button among → = ≠ ←.
    pub fn button(self) -> usize {
        match self {
            State::LeftOnly | State::LeftNewer => 0,
            State::Equal => 1,
            State::Differs | State::Mismatch | State::Unreadable => 2,
            State::RightOnly | State::RightNewer => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    /// A file whose data is not on this disk: never read for its content.
    OnlineFile,
    Folder,
    /// A folder that was not read (`Tree::problems` says why).
    Unread,
}

/// A file or folder of one side. The path is not kept: `Tree::names` builds it from `parent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: Box<str>,
    /// The name as names are matched (NFC, folded where case is ignored); `None`: the name.
    pub key: Option<Box<str>>,
    pub size: u64,
    /// Unix seconds; `NO_TIME` when not known.
    pub modified: i64,
    pub parent: u32,
    /// A folder's children are `nodes[first..first + count]`, sorted by key.
    pub first: u32,
    pub count: u32,
    pub kind: Kind,
}

impl Node {
    pub fn key(&self) -> &str {
        self.key.as_deref().unwrap_or(&self.name)
    }
}

/// One side: `nodes[0]` is the folder compared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    pub nodes: Vec<Node>,
    /// The folders not read and why.
    pub problems: Vec<(u32, Box<str>)>,
}

impl Tree {
    fn children(&self, i: u32) -> std::ops::Range<u32> {
        let node = &self.nodes[i as usize];
        node.first..node.first + node.count
    }

    /// The names from under the root down to node `i`.
    pub fn names(&self, mut i: u32) -> Vec<&str> {
        let mut names = Vec::new();
        while i != 0 {
            let node = &self.nodes[i as usize];
            names.push(&*node.name);
            i = node.parent;
        }
        names.reverse();
        names
    }

    /// Why folder `i` was not read.
    pub fn problem(&self, i: u32) -> Option<&str> {
        self.problems.iter().find(|(at, _)| *at == i).map(|(_, why)| &**why)
    }
}

/// A row of the compare, in tree order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub state: State,
    pub depth: u16,
    pub folder: bool,
    /// Counted and filtered by the show buttons: a pair, or what one side alone has inside a
    /// folder both have (a folder only one side has is one such row; what is in it is not).
    pub unit: bool,
    /// Its content is still to be read (`Options::content`).
    pub check: bool,
    /// Its content was read and is the same, its dates are not.
    pub dates_differ: bool,
    /// The rows under it end before this one.
    pub end: u32,
}

impl Row {
    pub fn one_sided(&self) -> bool {
        self.left.is_none() || self.right.is_none()
    }
}

/// `time` as Unix seconds (`NO_TIME`: none).
pub fn seconds(time: Option<SystemTime>) -> i64 {
    match time.map(|t| t.duration_since(SystemTime::UNIX_EPOCH)) {
        Some(Ok(after)) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Some(Err(before)) => -i64::try_from(before.duration().as_secs()).unwrap_or(i64::MAX),
        None => NO_TIME,
    }
}

/// Whether two times count as the same: 2 seconds apart (FAT) always, an hour apart (±2 s)
/// with `ignore_hour`. A time not known is the same as any.
pub fn same_time(a: i64, b: i64, ignore_hour: bool) -> bool {
    if a == NO_TIME || b == NO_TIME {
        return true;
    }
    let gap = a.abs_diff(b);
    gap <= FAT_STEP || ignore_hour && gap.abs_diff(HOUR) <= FAT_STEP
}

/// What two items of the same name are, and whether their content is to be read.
pub fn classify(l: &Node, r: &Node, o: &Options) -> (State, bool) {
    match (l.kind, r.kind) {
        (Kind::Unread, _) | (_, Kind::Unread) => return (State::Unreadable, false),
        (Kind::Folder, Kind::Folder) => return (State::Equal, false),
        (Kind::Folder, _) | (_, Kind::Folder) => return (State::Mismatch, false),
        _ => {}
    }
    let state = if !o.ignore_dates && !same_time(l.modified, r.modified, o.ignore_hour) {
        if l.modified > r.modified { State::LeftNewer } else { State::RightNewer }
    } else if l.size == r.size {
        State::Equal
    } else {
        State::Differs
    };
    let online = l.kind == Kind::OnlineFile || r.kind == Kind::OnlineFile;
    let check = o.content && !online && l.size == r.size && (o.ignore_dates || state != State::Equal);
    (state, check)
}

/// The rows of the two trees, in tree order (each folder's children by key), the folders'
/// states from what is under them.
pub fn build(left: &Tree, right: &Tree, o: &Options) -> Vec<Row> {
    let mut rows = Vec::new();
    if !left.nodes.is_empty() && !right.nodes.is_empty() {
        merge(left, right, Some(0), Some(0), 0, o, &mut rows);
    }
    settle_folders(&mut rows);
    rows
}

/// The children of `l` and `r` (either may be missing), merged by key.
fn merge(left: &Tree, right: &Tree, l: Option<u32>, r: Option<u32>, depth: u16, o: &Options, rows: &mut Vec<Row>) {
    let lc = l.map_or(0..0, |i| left.children(i));
    let rc = r.map_or(0..0, |i| right.children(i));
    let (mut a, mut b) = (lc.start, rc.start);
    while a < lc.end || b < rc.end {
        let order = match (a < lc.end, b < rc.end) {
            (true, true) => left.nodes[a as usize].key().cmp(right.nodes[b as usize].key()),
            (true, false) => Ordering::Less,
            _ => Ordering::Greater,
        };
        let (x, y) = match order {
            Ordering::Less => (Some(a), None),
            Ordering::Greater => (None, Some(b)),
            Ordering::Equal => (Some(a), Some(b)),
        };
        a += u32::from(x.is_some());
        b += u32::from(y.is_some());
        pair(left, right, x, y, depth, l.is_some() && r.is_some(), o, rows);
    }
}

#[allow(clippy::too_many_arguments)]
fn pair(
    left: &Tree,
    right: &Tree,
    x: Option<u32>,
    y: Option<u32>,
    depth: u16,
    in_both: bool,
    o: &Options,
    rows: &mut Vec<Row>,
) {
    let at = rows.len();
    let mut row = Row {
        left: x,
        right: y,
        state: State::Equal,
        depth,
        folder: false,
        unit: in_both,
        check: false,
        dates_differ: false,
        end: 0,
    };
    let side = |node: &Node, only: State| match node.kind {
        Kind::Unread => (State::Unreadable, false),
        Kind::Folder => (only, true),
        _ => (only, false),
    };
    match (x.map(|i| &left.nodes[i as usize]), y.map(|i| &right.nodes[i as usize])) {
        (Some(a), Some(b)) if a.kind == Kind::Folder && b.kind == Kind::Folder => {
            row.folder = true;
            row.unit = false;
        }
        (Some(a), Some(b)) => (row.state, row.check) = classify(a, b, o),
        (Some(a), None) => (row.state, row.folder) = side(a, State::LeftOnly),
        (None, Some(b)) => (row.state, row.folder) = side(b, State::RightOnly),
        (None, None) => return,
    }
    rows.push(row);
    if row.folder {
        merge(left, right, x, y, depth.saturating_add(1), o, rows);
    }
    rows[at].end = u32::try_from(rows.len()).unwrap_or(u32::MAX);
}

/// A folder both sides have is `=` when everything under it is, else `≠`.
pub fn settle_folders(rows: &mut [Row]) {
    for i in (0..rows.len()).rev() {
        if !rows[i].folder || rows[i].one_sided() {
            continue;
        }
        let (mut same, mut j, end) = (true, i + 1, rows[i].end as usize);
        while j < end {
            same &= rows[j].state == State::Equal;
            j = rows[j].end as usize;
        }
        rows[i].state = if same { State::Equal } else { State::Differs };
    }
}

/// A content compare's answer for `row`: the same content is `=` (its dates noted), another
/// keeps its date's state (`≠` if dates did not tell them apart).
pub fn content_result(row: &mut Row, same: bool) {
    row.check = false;
    if same {
        row.dates_differ = row.state != State::Equal;
        row.state = State::Equal;
    } else if row.state == State::Equal {
        row.state = State::Differs;
    }
}

/// The show buttons' numbers.
pub fn counts(rows: &[Row]) -> [usize; 6] {
    let mut n = [0; 6];
    for row in rows.iter().filter(|r| r.unit) {
        n[row.state.button()] += 1;
        n[if row.one_sided() { ONE_SIDE } else { BOTH_SIDES }] += 1;
    }
    n
}

/// The rows shown, as indices: a counted row when its state's and its sides' buttons are on, a
/// folder both sides have when something under it shows; nothing under a closed folder
/// (`open[i]`).
pub fn visible(rows: &[Row], show: &[bool; 6], open: &[bool]) -> Vec<u32> {
    let mut passes = vec![false; rows.len()];
    for i in (0..rows.len()).rev() {
        let row = &rows[i];
        passes[i] = if row.unit {
            show[row.state.button()] && show[if row.one_sided() { ONE_SIDE } else { BOTH_SIDES }]
        } else if row.folder && !row.one_sided() {
            let (mut any, mut j) = (false, i + 1);
            while j < row.end as usize {
                any |= passes[j];
                j = rows[j].end as usize;
            }
            any
        } else {
            // Inside a folder one side alone has: shown with it.
            true
        };
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let row = &rows[i];
        if !passes[i] {
            i = row.end as usize;
            continue;
        }
        out.push(u32::try_from(i).unwrap_or(u32::MAX));
        i = if row.folder && !open.get(i).copied().unwrap_or(false) { row.end as usize } else { i + 1 };
    }
    out
}

/// One item of a pane's list for `differences`: its matching key, whether it is a folder, its
/// time (Unix seconds).
pub type Item = (String, bool, i64);

/// `select-differences` (spec 11 §7.5): on each side, the items the other side lacks, and the
/// files newer than the other side's file of the same name.
pub fn differences(left: &[Item], right: &[Item]) -> (Vec<usize>, Vec<usize>) {
    fn pick(mine: &[Item], theirs: &[Item]) -> Vec<usize> {
        let index: HashMap<&str, (bool, i64)> = theirs.iter().map(|(k, dir, t)| (k.as_str(), (*dir, *t))).collect();
        let picked = |(key, dir, time): &Item| match index.get(key.as_str()) {
            None => true,
            Some(&(other_dir, other_time)) => {
                !dir && !other_dir && !same_time(*time, other_time, false) && *time > other_time
            }
        };
        mine.iter().enumerate().filter(|(_, item)| picked(item)).map(|(i, _)| i).collect()
    }
    (pick(left, right), pick(right, left))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree from `(path, size, time)`; a path ending in `/` is a folder, in `!` unread, `~`
    /// online only. Children sorted by name as the scan sorts them.
    fn tree(items: &[(&str, u64, i64)]) -> Tree {
        let mut tree = Tree::default();
        tree.nodes.push(node("", Kind::Folder, 0, 0, 0));
        add(&mut tree, 0, "", items);
        tree
    }

    fn node(name: &str, kind: Kind, size: u64, modified: i64, parent: u32) -> Node {
        Node { name: name.into(), key: None, size, modified, parent, first: 0, count: 0, kind }
    }

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
                Some('~') => (&rest[..rest.len() - 1], Kind::OnlineFile),
                _ => (rest, Kind::File),
            };
            mine.push((name.to_owned(), kind, *size, *time));
        }
        mine.sort_by(|a, b| a.0.cmp(&b.0));
        let first = tree.nodes.len() as u32;
        for (name, kind, size, time) in &mine {
            tree.nodes.push(node(name, *kind, *size, *time, at));
        }
        tree.nodes[at as usize].first = first;
        tree.nodes[at as usize].count = mine.len() as u32;
        for (k, (name, kind, ..)) in mine.iter().enumerate() {
            if *kind == Kind::Folder {
                add(tree, first + k as u32, &format!("{prefix}{name}/"), items);
            }
        }
    }

    fn states(rows: &[Row], left: &Tree, right: &Tree) -> Vec<(String, State)> {
        rows.iter()
            .map(|r| {
                let names = match (r.left, r.right) {
                    (Some(i), _) => left.names(i),
                    (None, Some(i)) => right.names(i),
                    _ => Vec::new(),
                };
                (names.join("/"), r.state)
            })
            .collect()
    }

    const T: i64 = 1_700_000_000;

    #[test]
    fn every_state_of_a_pair() {
        let left = tree(&[
            ("a.txt", 5, T),
            ("newer.txt", 5, T + 60),
            ("older.txt", 5, T),
            ("size.txt", 5, T),
            ("both", 1, T),
            ("lonely.txt", 1, T),
            ("gone!", 0, T),
        ]);
        let right = tree(&[
            ("a.txt", 5, T + 1),
            ("newer.txt", 5, T),
            ("older.txt", 5, T + 60),
            ("size.txt", 6, T),
            ("both/", 0, T),
            ("extra.txt", 1, T),
            ("gone/", 0, T),
        ]);
        let rows = build(&left, &right, &Options::default());
        assert_eq!(
            states(&rows, &left, &right),
            [
                ("a.txt".to_owned(), State::Equal),
                ("both".to_owned(), State::Mismatch),
                ("extra.txt".to_owned(), State::RightOnly),
                ("gone".to_owned(), State::Unreadable),
                ("lonely.txt".to_owned(), State::LeftOnly),
                ("newer.txt".to_owned(), State::LeftNewer),
                ("older.txt".to_owned(), State::RightNewer),
                ("size.txt".to_owned(), State::Differs),
            ]
        );
        assert!(rows.iter().all(|r| r.unit && !r.check));
        assert_eq!(counts(&rows), [2, 1, 3, 2, 2, 6]);
    }

    #[test]
    fn fat_two_seconds_always_an_hour_when_asked() {
        let file = |t: i64| node("f", Kind::File, 1, t, 0);
        let o = Options::default();
        assert_eq!(classify(&file(T), &file(T + 2), &o).0, State::Equal, "FAT's 2 s step");
        assert_eq!(classify(&file(T + 2), &file(T), &o).0, State::Equal);
        assert_eq!(classify(&file(T), &file(T + 3), &o).0, State::RightNewer);
        assert_eq!(classify(&file(T), &file(T + 3600), &o).0, State::RightNewer, "the hour only when asked");
        let hour = Options { ignore_hour: true, ..o };
        for gap in [3598, 3600, 3602, -3600] {
            assert_eq!(classify(&file(T), &file(T + gap), &hour).0, State::Equal, "{gap}");
        }
        assert_eq!(classify(&file(T), &file(T + 3603), &hour).0, State::RightNewer);
        assert_eq!(classify(&file(T), &file(T + 1800), &hour).0, State::RightNewer);
        assert_eq!(classify(&file(NO_TIME), &file(T), &o).0, State::Equal, "no time: by size");
        let dates = Options { ignore_dates: true, ..o };
        assert_eq!(classify(&file(T), &file(T + 99), &dates).0, State::Equal);
        let mut big = file(T + 99);
        big.size = 2;
        assert_eq!(classify(&file(T), &big, &dates).0, State::Differs);
        assert!(same_time(seconds(None), T, false));
        assert_eq!(seconds(Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1500))), 1);
    }

    #[test]
    fn content_is_read_only_where_it_can_tell() {
        let file = |size: u64, t: i64, kind: Kind| node("f", kind, size, t, 0);
        let o = Options { content: true, ..Options::default() };
        assert!(classify(&file(1, T, Kind::File), &file(1, T + 60, Kind::File), &o).1, "same size, dates apart");
        assert!(!classify(&file(1, T, Kind::File), &file(1, T, Kind::File), &o).1, "same size and date: not read");
        assert!(!classify(&file(1, T, Kind::File), &file(2, T + 60, Kind::File), &o).1, "sizes apart");
        assert!(!classify(&file(1, T, Kind::OnlineFile), &file(1, T + 60, Kind::File), &o).1, "online only");
        let dates = Options { ignore_dates: true, ..o };
        assert!(classify(&file(1, T, Kind::File), &file(1, T, Kind::File), &dates).1);
        assert!(!classify(&file(1, T, Kind::File), &file(1, T + 60, Kind::File), &Options::default()).1);

        let left = tree(&[("d/", 0, T), ("d/a", 1, T + 60), ("d/b", 1, T), ("d/c", 1, T)]);
        let right = tree(&[("d/", 0, T), ("d/a", 1, T), ("d/b", 1, T + 60), ("d/c", 1, T)]);
        let mut rows = build(&left, &right, &o);
        assert_eq!(rows[0].state, State::Differs, "the folder from what is under it");
        assert_eq!([rows[1].check, rows[2].check, rows[3].check], [true, true, false]);
        content_result(&mut rows[1], true);
        content_result(&mut rows[2], false);
        settle_folders(&mut rows);
        assert_eq!((rows[1].state, rows[1].dates_differ), (State::Equal, true));
        assert_eq!(rows[2].state, State::RightNewer, "other content keeps the date's state");
        assert_eq!(rows[0].state, State::Differs);
        content_result(&mut rows[2], true);
        settle_folders(&mut rows);
        assert_eq!(rows[0].state, State::Equal, "everything under it the same");
        let mut equal = rows[3];
        equal.state = State::Equal;
        content_result(&mut equal, false);
        assert_eq!(equal.state, State::Differs, "dates ignored: other content is ≠");
    }

    #[test]
    fn a_folder_one_side_has_is_one_counted_row() {
        let left = tree(&[("new/", 0, T), ("new/a", 1, T), ("new/sub/", 0, T), ("new/sub/b", 1, T), ("x", 1, T)]);
        let right = tree(&[("x", 1, T)]);
        let rows = build(&left, &right, &Options::default());
        let shown = states(&rows, &left, &right);
        assert_eq!(shown[0], ("new".to_owned(), State::LeftOnly));
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].end, 4, "its rows under it");
        assert!(rows[0].unit && rows[0].folder && !rows[1].unit && !rows[3].unit);
        assert_eq!(rows[2].depth, 1);
        assert_eq!(counts(&rows), [1, 1, 0, 0, 1, 1]);
        // Closed (as a one-sided folder starts): only its own row.
        let all = Options { show: [true; 6], ..Options::default() };
        assert_eq!(visible(&rows, &all.show, &[false; 5]), [0, 4]);
        let open = [true, false, false, false, false];
        assert_eq!(visible(&rows, &all.show, &open), [0, 1, 2, 4], "sub closed");
    }

    #[test]
    fn the_show_buttons_filter_and_folders_follow() {
        let left = tree(&[("d/", 0, T), ("d/same", 1, T), ("d/new", 1, T + 60), ("e/", 0, T), ("e/same", 1, T)]);
        let right = tree(&[("d/", 0, T), ("d/same", 1, T), ("d/new", 1, T), ("e/", 0, T), ("e/same", 1, T)]);
        let rows = build(&left, &right, &Options::default());
        let open = vec![true; rows.len()];
        let names =
            |shown: Vec<u32>| states(&shown.iter().map(|i| rows[*i as usize]).collect::<Vec<_>>(), &left, &right);
        // Default: `=` off; the folder of only equal files is gone.
        let shown = names(visible(&rows, &Options::default().show, &open));
        assert_eq!(shown, [("d".to_owned(), State::Differs), ("d/new".to_owned(), State::LeftNewer)]);
        let mut show = [true; 6];
        assert_eq!(visible(&rows, &show, &open).len(), 5);
        show[BOTH_SIDES] = false;
        assert!(visible(&rows, &show, &open).is_empty(), "2× off: nothing on both sides");
        show = [false, true, false, false, true, true];
        assert_eq!(names(visible(&rows, &show, &open)).len(), 4, "= only: both folders, two files");
        assert_eq!(visible(&rows, &[true; 6], &[false; 5]), [0, 3], "closed folders");
    }

    #[test]
    fn a_hundred_thousand_pairs_compare_quickly() {
        // 100 folders of 1,000 files on each side, every tenth file newer on the left.
        let side = |newer: bool| {
            let mut tree = Tree::default();
            tree.nodes.push(Node { first: 1, count: 100, ..node("", Kind::Folder, 0, 0, 0) });
            for f in 0..100u32 {
                tree.nodes.push(Node {
                    first: 101 + f * 1000,
                    count: 1000,
                    ..node(&format!("d{f:03}"), Kind::Folder, 0, T, 0)
                });
            }
            for f in 0..100u32 {
                for i in 0..1000u32 {
                    let t = if newer && i % 10 == 0 { T + 60 } else { T };
                    tree.nodes.push(node(&format!("f{i:04}"), Kind::File, u64::from(i), t, 1 + f));
                }
            }
            tree
        };
        let (left, right) = (side(true), side(false));
        let started = std::time::Instant::now();
        let rows = build(&left, &right, &Options::default());
        let shown = visible(&rows, &Options::default().show, &vec![true; rows.len()]);
        let took = started.elapsed();
        assert_eq!(counts(&rows)[0], 10_000);
        assert_eq!(shown.len(), 10_100);
        // Spec: ≤ 200 ms in a release build; a debug build is slower.
        assert!(took.as_millis() < if cfg!(debug_assertions) { 2000 } else { 200 }, "{took:?}");
    }

    #[test]
    fn select_differences_takes_what_is_missing_or_newer() {
        let item = |k: &str, dir: bool, t: i64| (k.to_owned(), dir, t);
        let left = [item("a", false, T), item("b", false, T + 60), item("d", true, T), item("e", false, T)];
        let right = [item("a", false, T + 1), item("b", false, T), item("c", false, T), item("d", true, T + 99)];
        assert_eq!(differences(&left, &right), (vec![1, 3], vec![2]));
        assert_eq!(differences(&[], &right).1, [0, 1, 2, 3]);
    }
}
