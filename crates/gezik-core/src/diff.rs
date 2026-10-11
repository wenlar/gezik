//! Two texts' lines lined up side by side (11c, spec 11 §6.2): equal lines, changed pairs,
//! added and removed lines. Lines become numbers (equal key, equal number), the common start and
//! end are trimmed, and the rest goes through Myers' O(ND) algorithm in its linear-space form
//! (the middle snake, divided and conquered): the tracing form would need D² memory at the
//! D = 20,000 cap.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

/// More edits than this: the files are "too different to line up" (spec 11 §6.2).
pub const MAX_EDITS: usize = 20_000;

/// How lines are compared.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Runs of whitespace count as one space; none at the start or end.
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Same,
    /// A removed line and an added one at the same place in their blocks.
    Changed,
    /// Only on the right.
    Added,
    /// Only on the left.
    Removed,
}

/// One line of the side-by-side view: the line index on each side, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub kind: Kind,
}

/// More than `MAX_EDITS` edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooDifferent;

/// What a line is compared by.
pub fn key(line: &str, options: Options) -> Cow<'_, str> {
    let mut out: Cow<str> = Cow::Borrowed(line);
    if options.ignore_whitespace {
        let words: Vec<&str> = line.split_whitespace().collect();
        let joined = words.join(" ");
        if joined != line {
            out = Cow::Owned(joined);
        }
    }
    if options.ignore_case && out.chars().any(char::is_uppercase) {
        out = Cow::Owned(out.to_lowercase());
    }
    out
}

/// Lines `left` and `right` lined up; `TooDifferent` past `MAX_EDITS` edits.
pub fn diff(left: &[&str], right: &[&str], options: Options) -> Result<Vec<Row>, TooDifferent> {
    let (a, b) = numbers(left, right, options);
    let mut matches = Vec::new();
    let mut search = Search::default();
    conquer(&a, 0, &b, 0, &mut search, MAX_EDITS.div_ceil(2), &mut matches)?;
    Ok(rows(&matches, a.len(), b.len()))
}

/// The lines side by side without lining up (too different): row `i` holds line `i` of each.
pub fn unaligned(left: &[&str], right: &[&str], options: Options) -> Vec<Row> {
    (0..left.len().max(right.len()))
        .map(|i| {
            let (l, r) = (left.get(i), right.get(i));
            let kind = match (l, r) {
                (Some(l), Some(r)) if key(l, options) == key(r, options) => Kind::Same,
                (Some(_), Some(_)) => Kind::Changed,
                (Some(_), None) => Kind::Removed,
                _ => Kind::Added,
            };
            Row { left: l.map(|_| index(i)), right: r.map(|_| index(i)), kind }
        })
        .collect()
}

/// Where each change (a run of rows that are not `Same`) starts.
pub fn changes(rows: &[Row]) -> Vec<usize> {
    (0..rows.len()).filter(|&i| rows[i].kind != Kind::Same && (i == 0 || rows[i - 1].kind == Kind::Same)).collect()
}

/// The part of `a` and of `b` (byte ranges on character boundaries) left once their common
/// start and end characters are taken off: the one span a changed line highlights.
pub fn inline_span(a: &str, b: &str) -> (Range<usize>, Range<usize>) {
    let start =
        a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map_or(a.len().min(b.len()), |((i, _), _)| i);
    // `start` is a boundary of both: the characters before it are the same.
    let (rest_a, rest_b) = (&a[start..], &b[start..]);
    let tail = rest_a
        .chars()
        .rev()
        .zip(rest_b.chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum::<usize>();
    (start..a.len() - tail, start..b.len() - tail)
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// The lines as numbers: equal keys, equal numbers.
fn numbers<'a>(left: &[&'a str], right: &[&'a str], options: Options) -> (Vec<u32>, Vec<u32>) {
    let mut ids: HashMap<Cow<'a, str>, u32> = HashMap::with_capacity(left.len() + right.len());
    let mut number = |line: &'a str| {
        let next = index(ids.len());
        *ids.entry(key(line, options)).or_insert(next)
    };
    let a = left.iter().map(|l| number(l)).collect();
    let b = right.iter().map(|l| number(l)).collect();
    (a, b)
}

/// The two furthest-reaching paths' tables, kept between calls (no allocation per split).
#[derive(Default)]
struct Search {
    forward: Vec<usize>,
    backward: Vec<usize>,
}

/// Adds the matching line pairs of `a` and `b` (at `a0`, `b0` in the whole texts) to `out`, in
/// order. `limit`: the most edits from either end the middle snake may take.
fn conquer(
    a: &[u32],
    a0: usize,
    b: &[u32],
    b0: usize,
    search: &mut Search,
    limit: usize,
    out: &mut Vec<(usize, usize)>,
) -> Result<(), TooDifferent> {
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    out.extend((0..head).map(|i| (a0 + i, b0 + i)));
    let (a, b) = (&a[head..], &b[head..]);
    let tail = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
    let (inner_a, inner_b) = (&a[..a.len() - tail], &b[..b.len() - tail]);
    if !inner_a.is_empty() && !inner_b.is_empty() {
        let (x, y) = middle(inner_a, inner_b, search, limit).ok_or(TooDifferent)?;
        conquer(&inner_a[..x], a0 + head, &inner_b[..y], b0 + head, search, limit, out)?;
        conquer(&inner_a[x..], a0 + head + x, &inner_b[y..], b0 + head + y, search, limit, out)?;
    }
    let (ta, tb) = (a0 + head + inner_a.len(), b0 + head + inner_b.len());
    out.extend((0..tail).map(|i| (ta + i, tb + i)));
    Ok(())
}

/// A point an optimal edit path through `a`, `b` passes, neither end (both are non-empty and
/// differ at both ends); `None` past `limit` edits from either end.
fn middle(a: &[u32], b: &[u32], search: &mut Search, limit: usize) -> Option<(usize, usize)> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = ((n + m + 1) / 2) as usize;
    let d_max = max.min(limit) as isize;
    // Diagonal k (x - y) at `k + offset`, from -(d_max + 1) to d_max + 1.
    let offset = d_max + 1;
    let size = (2 * offset + 1) as usize;
    let (vf, vb) = (&mut search.forward, &mut search.backward);
    vf.clear();
    vf.resize(size, 0);
    vb.clear();
    vb.resize(size, 0);
    let delta = n - m;
    let odd = delta & 1 != 0;
    let at = |k: isize| (k + offset) as usize;
    for d in 0..=d_max {
        // Forward: from the start.
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && vf[at(k - 1)] < vf[at(k + 1)]) {
                vf[at(k + 1)] as isize
            } else {
                vf[at(k - 1)] as isize + 1
            };
            let mut y = x - k;
            let (x0, y0) = (x, y);
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            vf[at(k)] = x as usize;
            let back = delta - k;
            if odd && back.abs() < d && x + vb[at(back)] as isize >= n {
                return Some((x0 as usize, y0 as usize));
            }
        }
        // Backward: from the end, in reversed coordinates.
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && vb[at(k - 1)] < vb[at(k + 1)]) {
                vb[at(k + 1)] as isize
            } else {
                vb[at(k - 1)] as isize + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[(n - x - 1) as usize] == b[(m - y - 1) as usize] {
                x += 1;
                y += 1;
            }
            vb[at(k)] = x as usize;
            let front = delta - k;
            if !odd && front.abs() <= d && x + vf[at(front)] as isize >= n {
                return Some(((n - x) as usize, (m - y) as usize));
            }
        }
    }
    None
}

/// Rows from the matching pairs: between two matches the removed lines and the added ones pair
/// up as `Changed`, the rest are `Removed` or `Added`.
fn rows(matches: &[(usize, usize)], n: usize, m: usize) -> Vec<Row> {
    let mut out = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (0, 0);
    for &(x, y) in matches.iter().chain(std::iter::once(&(n, m))) {
        let (removed, added) = (x - i, y - j);
        for k in 0..removed.max(added) {
            let left = (k < removed).then(|| index(i + k));
            let right = (k < added).then(|| index(j + k));
            let kind = match (left, right) {
                (Some(_), Some(_)) => Kind::Changed,
                (Some(_), None) => Kind::Removed,
                _ => Kind::Added,
            };
            out.push(Row { left, right, kind });
        }
        if x < n {
            out.push(Row { left: Some(index(x)), right: Some(index(y)), kind: Kind::Same });
        }
        (i, j) = (x + 1, y + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<&str> {
        text.split(' ').filter(|s| !s.is_empty()).collect()
    }

    fn run(a: &str, b: &str) -> Vec<Row> {
        diff(&lines(a), &lines(b), Options::default()).unwrap()
    }

    /// Edits (removed + added lines, a changed pair counting two) and the rows' own check: every
    /// line of each side once, in order, and equal lines only on `Same` rows.
    fn edits(a: &[&str], b: &[&str], rows: &[Row]) -> usize {
        let left: Vec<u32> = rows.iter().filter_map(|r| r.left).collect();
        let right: Vec<u32> = rows.iter().filter_map(|r| r.right).collect();
        assert_eq!(left, (0..a.len() as u32).collect::<Vec<_>>());
        assert_eq!(right, (0..b.len() as u32).collect::<Vec<_>>());
        let mut count = 0;
        for row in rows {
            match row.kind {
                Kind::Same => assert_eq!(a[row.left.unwrap() as usize], b[row.right.unwrap() as usize]),
                Kind::Changed => count += 2,
                Kind::Added | Kind::Removed => count += 1,
            }
        }
        count
    }

    /// The longest common subsequence's length, the slow way.
    fn lcs(a: &[&str], b: &[&str]) -> usize {
        let mut table = vec![vec![0; b.len() + 1]; a.len() + 1];
        for i in 0..a.len() {
            for j in 0..b.len() {
                table[i + 1][j + 1] = if a[i] == b[j] { table[i][j] + 1 } else { table[i][j + 1].max(table[i + 1][j]) };
            }
        }
        table[a.len()][b.len()]
    }

    #[test]
    fn empty_and_equal_texts() {
        assert_eq!(run("", ""), []);
        assert_eq!(
            run("", "a b"),
            [
                Row { left: None, right: Some(0), kind: Kind::Added },
                Row { left: None, right: Some(1), kind: Kind::Added }
            ]
        );
        assert_eq!(run("a", ""), [Row { left: Some(0), right: None, kind: Kind::Removed }]);
        let same = run("a b c", "a b c");
        assert!(same.iter().all(|r| r.kind == Kind::Same) && same.len() == 3);
        assert!(changes(&same).is_empty());
    }

    #[test]
    fn all_different_lines_pair_up_as_changed() {
        let rows = run("a b c", "x y");
        let kinds: Vec<Kind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [Kind::Changed, Kind::Changed, Kind::Removed]);
        assert_eq!(changes(&rows), [0], "one block of changes");
    }

    #[test]
    fn myers_paper_example_is_shortest() {
        // Myers 1986: ABCABBA → CBABAC takes D = 5.
        let (a, b) = (lines("A B C A B B A"), lines("C B A B A C"));
        let rows = diff(&a, &b, Options::default()).unwrap();
        let same = rows.iter().filter(|r| r.kind == Kind::Same).count();
        assert_eq!(same, 4);
        assert_eq!(a.len() + b.len() - 2 * same, 5);
        edits(&a, &b, &rows);
    }

    #[test]
    fn known_cases_keep_the_common_start_and_end() {
        let rows = run("a b c d e", "a b x d e");
        assert_eq!(rows[2], Row { left: Some(2), right: Some(2), kind: Kind::Changed });
        assert_eq!(rows.iter().filter(|r| r.kind == Kind::Same).count(), 4);
        let rows = run("a b c", "a z b c");
        assert_eq!(rows[1], Row { left: None, right: Some(1), kind: Kind::Added });
        assert_eq!(changes(&run("a x b y c", "a b c")), [1, 3], "two changes");
    }

    #[test]
    fn random_texts_get_a_shortest_script() {
        // A small xorshift: the same cases every run.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        let words = ["a", "b", "c", "d"];
        for _ in 0..400 {
            let a: Vec<&str> = (0..next(12)).map(|_| words[next(4) as usize]).collect();
            let b: Vec<&str> = (0..next(12)).map(|_| words[next(4) as usize]).collect();
            let rows = diff(&a, &b, Options::default()).unwrap();
            let same = rows.iter().filter(|r| r.kind == Kind::Same).count();
            assert_eq!(same, lcs(&a, &b), "{a:?} {b:?}");
            edits(&a, &b, &rows);
        }
    }

    #[test]
    fn too_many_edits_stop_the_lining_up() {
        let a: Vec<String> = (0..MAX_EDITS / 2 + 1).map(|i| format!("a{i}")).collect();
        let b: Vec<String> = (0..MAX_EDITS / 2 + 1).map(|i| format!("b{i}")).collect();
        let (a, b): (Vec<&str>, Vec<&str>) =
            (a.iter().map(|s| s.as_str()).collect(), b.iter().map(|s| s.as_str()).collect());
        assert_eq!(diff(&a, &b, Options::default()), Err(TooDifferent), "D = 20,002");
        let rows = unaligned(&a[..3], &b[..1], Options::default());
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0].kind, rows[2].kind), (Kind::Changed, Kind::Removed));
        // Exactly at the cap it still lines up.
        let fit = MAX_EDITS / 2;
        assert!(diff(&a[..fit], &b[..fit], Options::default()).is_ok(), "D = 20,000");
    }

    #[test]
    fn whitespace_and_case_can_be_ignored() {
        let (a, b) = (["  Foo   bar ", "x"], ["foo bar", "x"]);
        assert_eq!(diff(&a, &b, Options::default()).unwrap()[0].kind, Kind::Changed);
        let both = Options { ignore_whitespace: true, ignore_case: true };
        assert_eq!(diff(&a, &b, both).unwrap()[0].kind, Kind::Same);
        let ws = Options { ignore_whitespace: true, ignore_case: false };
        assert_eq!(diff(&a, &b, ws).unwrap()[0].kind, Kind::Changed, "case still counts");
        assert_eq!(key("a\t b", ws), "a b");
        assert!(matches!(key("plain", both), Cow::Borrowed(_)), "nothing to change: no copy");
    }

    #[test]
    fn inline_span_is_between_the_common_ends() {
        assert_eq!(inline_span("jumps over the lazy dog", "jumps over the lazy cat"), (20..23, 20..23));
        assert_eq!(inline_span("same", "same"), (4..4, 4..4));
        assert_eq!(inline_span("", "new"), (0..0, 0..3));
        assert_eq!(inline_span("çay ılık", "çay sıcak"), (5..10, 5..10), "on character boundaries");
        assert_eq!(inline_span("aXa", "aa"), (1..2, 1..1), "the end is not counted twice");
    }

    #[test]
    fn ten_thousand_lines_line_up_fast() {
        let a: Vec<String> = (0..10_000).map(|i| format!("line {i} of the report")).collect();
        let mut b = a.clone();
        for i in (0..10_000).step_by(37) {
            b[i] = format!("changed {i}");
        }
        b.insert(5000, "inserted".to_owned());
        b.remove(9000);
        let (a, b): (Vec<&str>, Vec<&str>) =
            (a.iter().map(|s| s.as_str()).collect(), b.iter().map(|s| s.as_str()).collect());
        let started = std::time::Instant::now();
        let rows = diff(&a, &b, Options::default()).unwrap();
        let took = started.elapsed();
        // Spec: ≤ 150 ms in a release build; a debug build is several times slower.
        let cap = if cfg!(debug_assertions) { 1500 } else { 150 };
        assert!(took.as_millis() <= cap, "{took:?}");
        assert_eq!(edits(&a, &b, &rows), 2 * 271 + 2, "{} changes", changes(&rows).len());
    }
}
