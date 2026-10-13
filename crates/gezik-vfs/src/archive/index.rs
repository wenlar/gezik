//! An archive's entries as a folder tree, read once when it is opened (spec 16 §5.2): every
//! name in one text, the nodes in one list where a folder's children sit side by side in
//! name order (found by halving). Folders the archive has no entry for are made from the
//! names; names that lead out (`..`, a root, a drive) are left out, and of two entries with
//! one name the later counts (as extracting writes it last).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gezik_batch::archive::Entry as Listed;
use gezik_core::Entry;

const DIR: u8 = 1;
const ENCRYPTED: u8 = 2;
/// The archive says how much an entry takes packed.
const PACKED: u8 = 4;

/// A date not given.
const NO_TIME: i64 = i64::MIN;

#[derive(Default)]
pub struct Index {
    names: String,
    nodes: Vec<Node>,
}

#[derive(Clone, Copy)]
struct Node {
    /// Its name in `names`.
    start: u32,
    len: u32,
    /// Its children: `count` nodes from `first`.
    first: u32,
    count: u32,
    /// A folder's: everything under it.
    size: u64,
    packed: u64,
    /// Seconds since 1970 (`NO_TIME`: none).
    modified: i64,
    flags: u8,
}

/// The node of the top.
const TOP: u32 = 0;

impl Index {
    pub fn build(entries: Vec<Listed>) -> Index {
        // (parts, its entry): sorted part by part, a folder comes right before what it holds.
        let mut paths: Vec<(Vec<String>, Listed)> = entries
            .into_iter()
            .filter_map(|entry| {
                let parts = parts(&entry.name)?;
                (!parts.is_empty()).then_some((parts, entry))
            })
            .collect();
        paths.sort_by(|(a, _), (b, _)| a.cmp(b));
        // One pass: a node per path part, in the order met (a tree in pre-order).
        let mut names = String::new();
        let mut flat: Vec<(u32, Node)> = vec![(u32::MAX, Node::folder(0, 0))];
        // The open folders: (its parts, its node).
        let mut open: Vec<(Vec<String>, u32)> = vec![(Vec::new(), TOP)];
        for (parts, entry) in paths {
            // The innermost open folder this path is under (never the path itself).
            while let Some((folder, _)) = open.last()
                && open.len() > 1
                && !(folder.len() < parts.len() && parts.starts_with(folder))
            {
                open.pop();
            }
            // Folders on the way the archive has no entry for.
            let mut at = open.last().map_or(0, |(p, _)| p.len());
            while at + 1 < parts.len() {
                let parent = open.last().map_or(TOP, |(_, n)| *n);
                let id = push(&mut names, &mut flat, parent, &parts[at], None);
                open.push((parts[..=at].to_vec(), id));
                at += 1;
            }
            let parent = open.last().map_or(TOP, |(_, n)| *n);
            let last = parts.last().map_or("", String::as_str);
            let id = match flat.last() {
                // A second entry of one name: the later one counts.
                Some((p, node)) if *p == parent && name_of(&names, node) == last && flat.len() > 1 => {
                    let id = flat.len() as u32 - 1;
                    flat[id as usize].1 = Node::of(flat[id as usize].1.start, last.len(), &entry);
                    id
                }
                _ => push(&mut names, &mut flat, parent, last, Some(&entry)),
            };
            if entry.is_dir {
                open.push((parts, id));
            }
        }
        Index { names, nodes: arrange(flat) }
    }

    /// The node at `path` (`/` between parts, `""` the top).
    fn find(&self, path: &str) -> Option<u32> {
        let mut at = TOP;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let node = self.nodes.get(at as usize)?;
            let children = &self.nodes[node.first as usize..(node.first + node.count) as usize];
            let found = children.binary_search_by(|child| name_of(&self.names, child).cmp(part)).ok()?;
            at = node.first + found as u32;
        }
        Some(at)
    }

    /// What folder `path` holds; `None` if it is no folder.
    pub fn list(&self, path: &str) -> Option<Vec<Entry>> {
        let node = self.nodes[self.find(path)? as usize];
        if node.flags & DIR == 0 {
            return None;
        }
        let children = &self.nodes[node.first as usize..(node.first + node.count) as usize];
        Some(children.iter().map(|child| self.entry(child)).collect())
    }

    /// Everything under folder `path`, each with its folder under `path` (`/` between parts,
    /// `""` for `path` itself), folders before what they hold. `each` returns false to stop.
    pub fn walk(&self, path: &str, each: &mut dyn FnMut(&str, Entry) -> bool) {
        let Some(top) = self.find(path) else { return };
        let mut queue = std::collections::VecDeque::from([(top, String::new())]);
        while let Some((at, folder)) = queue.pop_front() {
            let node = self.nodes[at as usize];
            for child in node.first..node.first + node.count {
                let entry = self.entry(&self.nodes[child as usize]);
                if entry.is_dir {
                    let under = if folder.is_empty() { entry.name.clone() } else { format!("{folder}/{}", entry.name) };
                    queue.push_back((child, under));
                }
                if !each(&folder, entry) {
                    return;
                }
            }
        }
    }

    fn entry(&self, node: &Node) -> Entry {
        let is_dir = node.flags & DIR != 0;
        Entry {
            name: name_of(&self.names, node).to_owned(),
            is_dir,
            // A folder's size is known at once (spec 16 §4.7).
            flags: if is_dir { Entry::SIZED } else { 0 },
            size: node.size,
            modified: (node.modified != NO_TIME).then(|| time(node.modified)).flatten(),
            created: None,
        }
    }

    pub fn is_file(&self, path: &str) -> bool {
        self.find(path).is_some_and(|n| self.nodes[n as usize].flags & DIR == 0)
    }

    pub fn is_dir(&self, path: &str) -> bool {
        self.find(path).is_some_and(|n| self.nodes[n as usize].flags & DIR != 0)
    }

    /// The bytes it takes packed (when the archive says) and whether it is encrypted.
    pub fn packed(&self, path: &str) -> Option<(Option<u64>, bool)> {
        let node = self.nodes[self.find(path)? as usize];
        Some(((node.flags & PACKED != 0).then_some(node.packed), node.flags & ENCRYPTED != 0))
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len() - 1
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.names.capacity() + self.nodes.capacity() * std::mem::size_of::<Node>()
    }
}

impl Node {
    fn folder(start: usize, len: usize) -> Node {
        Node {
            start: start as u32,
            len: len as u32,
            first: 0,
            count: 0,
            size: 0,
            packed: 0,
            modified: NO_TIME,
            flags: DIR,
        }
    }

    fn of(start: u32, len: usize, entry: &Listed) -> Node {
        let mut flags = if entry.is_dir { DIR } else { 0 };
        if entry.encrypted {
            flags |= ENCRYPTED;
        }
        if entry.packed.is_some() && !entry.is_dir {
            flags |= PACKED;
        }
        Node {
            start,
            len: len as u32,
            first: 0,
            count: 0,
            size: if entry.is_dir { 0 } else { entry.size.unwrap_or(0) },
            packed: entry.packed.unwrap_or(0),
            modified: entry.modified.map_or(NO_TIME, seconds),
            flags,
        }
    }
}

/// Adds a node named `name` under `parent` (a folder made from a name when `entry` is none).
fn push(names: &mut String, flat: &mut Vec<(u32, Node)>, parent: u32, name: &str, entry: Option<&Listed>) -> u32 {
    let start = names.len();
    names.push_str(name);
    let node = match entry {
        Some(entry) => Node::of(start as u32, name.len(), entry),
        None => Node::folder(start, name.len()),
    };
    flat.push((parent, node));
    flat.len() as u32 - 1
}

fn name_of<'a>(names: &'a str, node: &Node) -> &'a str {
    &names[node.start as usize..(node.start + node.len) as usize]
}

/// The nodes made in pre-order (each with its parent) laid out breadth first, so that every
/// folder's children are side by side (still in name order), with folder sizes added up.
fn arrange(flat: Vec<(u32, Node)>) -> Vec<Node> {
    let n = flat.len();
    // Each node's children, in the order met: by parent, kept stable.
    let mut starts = vec![0u32; n + 1];
    for (parent, _) in flat.iter().skip(1) {
        starts[*parent as usize + 1] += 1;
    }
    for i in 0..n {
        starts[i + 1] += starts[i];
    }
    let mut kids = vec![0u32; n.saturating_sub(1)];
    let mut fill = starts.clone();
    for (id, (parent, _)) in flat.iter().enumerate().skip(1) {
        kids[fill[*parent as usize] as usize] = id as u32;
        fill[*parent as usize] += 1;
    }
    // Breadth first: a node's children are placed together when it is reached.
    let mut order = Vec::with_capacity(n);
    order.push(0u32);
    let mut nodes: Vec<Node> = Vec::with_capacity(n);
    let mut at = 0;
    while at < order.len() {
        let old = order[at] as usize;
        let mut node = flat[old].1;
        let (from, to) = (starts[old] as usize, starts[old + 1] as usize);
        node.first = order.len() as u32;
        node.count = (to - from) as u32;
        order.extend_from_slice(&kids[from..to]);
        nodes.push(node);
        at += 1;
    }
    // Children come after their folder: added up from the end.
    for i in (0..nodes.len()).rev() {
        let node = nodes[i];
        if node.flags & DIR != 0 && node.count > 0 {
            let total: u64 =
                nodes[node.first as usize..(node.first + node.count) as usize].iter().map(|c| c.size).sum();
            nodes[i].size = total;
        }
    }
    nodes
}

/// An entry's name as parts; `None` if it leads out (`..`, a root, a drive) or holds a
/// control character.
fn parts(name: &str) -> Option<Vec<String>> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') || name.chars().any(char::is_control) {
        return None;
    }
    let mut out = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            // `C:` at the start: a drive.
            _ if out.is_empty() && part.len() == 2 && part.ends_with(':') => return None,
            _ => out.push(part.to_owned()),
        }
    }
    Some(out)
}

fn seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

fn time(secs: i64) -> Option<SystemTime> {
    if secs >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(name: &str, is_dir: bool, size: u64) -> Listed {
        Listed { name: name.into(), is_dir, size: Some(size), packed: Some(size / 2), encrypted: false, modified: None }
    }

    fn names(index: &Index, path: &str) -> Vec<(String, bool, u64)> {
        index.list(path).unwrap().into_iter().map(|e| (e.name, e.is_dir, e.size)).collect()
    }

    #[test]
    fn folders_are_made_from_names_and_add_up_their_sizes() {
        let index = Index::build(vec![
            listed("docs/2024/b.txt", false, 5),
            listed("a.txt", false, 1),
            listed("docs/", true, 0),
            listed("docs/2024/a.txt", false, 7),
            listed("./x.bin", false, 3),
        ]);
        assert_eq!(
            names(&index, ""),
            [("a.txt".into(), false, 1), ("docs".into(), true, 12), ("x.bin".into(), false, 3)]
        );
        assert_eq!(names(&index, "docs"), [("2024".into(), true, 12)], "a folder with no entry of its own");
        assert_eq!(names(&index, "docs/2024"), [("a.txt".into(), false, 7), ("b.txt".into(), false, 5)]);
        assert!(index.list("a.txt").is_none() && index.list("nope").is_none());
        assert!(index.is_file("docs/2024/a.txt") && index.is_dir("docs/2024") && !index.is_file("docs"));
        assert_eq!(index.packed("docs/2024/a.txt"), Some((Some(3), false)));
        assert_eq!(index.list("docs").unwrap()[0].flags, Entry::SIZED);
        let mut found = Vec::new();
        index.walk("docs", &mut |folder, entry| {
            found.push(format!("{folder}|{}", entry.name));
            true
        });
        assert_eq!(found, ["|2024", "2024|a.txt", "2024|b.txt"]);
        let mut first = 0;
        index.walk("", &mut |_, _| {
            first += 1;
            false
        });
        assert_eq!(first, 1, "stopped at once");
    }

    #[test]
    fn names_that_lead_out_are_left_out_and_the_later_of_two_counts() {
        let index = Index::build(vec![
            listed("../evil", false, 1),
            listed("/etc/passwd", false, 1),
            listed("C:/x", false, 1),
            listed("a/../../b", false, 1),
            listed("ok\\win.txt", false, 2),
            listed("dup.txt", false, 1),
            listed("dup.txt", false, 9),
        ]);
        assert_eq!(names(&index, ""), [("dup.txt".into(), false, 9), ("ok".into(), true, 2)]);
        assert_eq!(names(&index, "ok"), [("win.txt".into(), false, 2)]);
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn a_hundred_thousand_entries_stay_under_8_mb() {
        let entries: Vec<Listed> = (0..100_000)
            .map(|i| listed(&format!("project/src/module{:03}/file_{i:06}.rs", i % 500), false, 100))
            .collect();
        let index = Index::build(entries);
        assert_eq!(index.len(), 100_000 + 500 + 2);
        assert!(index.bytes() < 8 << 20, "{} bytes", index.bytes());
        assert_eq!(index.list("project/src/module007").unwrap().len(), 200);
        assert!(index.is_file("project/src/module499/file_099999.rs"));
    }
}
