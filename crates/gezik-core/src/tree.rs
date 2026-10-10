//! The sidebar's folder tree (spec 10 §5): branches under the sidebar's places, read only when
//! opened, never watched, forgotten when closed. Pure: the app reads a folder on a worker for a
//! `Read` and hands back what it found (`Listed`); nothing here touches the disk.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::sort::natural_cmp;

/// The most sub-folders one branch shows (spec 10 §5.2); the rest is one "… n more" line.
pub const BRANCH_CAP: usize = 20_000;
/// A folder this deep is not opened: a loop the read cannot see (a bind mount) stops here.
pub const MAX_DEPTH: u16 = 64;

pub type NodeId = u32;

/// A place a root stands for: its sidebar section and its folder. The same folder in two
/// sections is two roots, each opened on its own (as in Explorer).
pub type RootKey = (i32, PathBuf);

/// Whether two paths name one folder (sidebar.rs `same_path`: case-blind on Windows).
pub type Same<'a> = &'a dyn Fn(&Path, &Path) -> bool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not read (or closed again): it may have sub-folders, nothing is kept.
    Closed,
    /// Its read is on its way.
    Loading,
    Open,
    /// Read and found without sub-folders.
    Empty,
    /// Leads back to a folder above it (a link or junction), or too deep: never opened.
    Loop,
}

impl State {
    /// The arrow sidebar.slint draws: 0 none, 1 closed, 2 open, 3 a read on its way.
    pub fn arrow(self) -> i32 {
        match self {
            State::Closed => 1,
            State::Open => 2,
            State::Loading => 3,
            State::Empty | State::Loop => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    /// `None` for a root (a sidebar place).
    pub parent: Option<NodeId>,
    /// 0 for a root.
    pub depth: u16,
    pub state: State,
    /// In natural name order, at most `BRANCH_CAP`; empty unless open.
    pub children: Vec<NodeId>,
    /// Sub-folders left out past the cap.
    pub more: usize,
    /// The newest read asked for; a result with another ticket is dropped.
    ticket: u64,
    /// Where the folder really is (links resolved), once read: the loop guard.
    real: Option<PathBuf>,
}

impl Node {
    /// The folder's name (a drive or server root: its whole path).
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map_or_else(|| self.path.display().to_string(), |name| name.to_string_lossy().into_owned())
    }
}

/// A folder the app reads for the tree, off the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub node: NodeId,
    pub ticket: u64,
    pub path: PathBuf,
}

/// What a read found, made by [`prepare`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listed {
    /// The sub-folders shown, in natural order, at most `BRANCH_CAP`.
    pub names: Vec<String>,
    /// How many were left out past the cap.
    pub more: usize,
    pub real: Option<PathBuf>,
}

/// Sorts `names` as the list sorts names (natural order), drops repeats and keeps the first
/// `BRANCH_CAP`. Run on the worker that read them: a branch of 50,000 folders costs the UI
/// thread no sort.
pub fn prepare(mut names: Vec<String>, real: Option<PathBuf>) -> Listed {
    names.sort_by(|a, b| natural_cmp(a, b));
    names.dedup();
    let more = names.len().saturating_sub(BRANCH_CAP);
    names.truncate(BRANCH_CAP);
    names.shrink_to_fit();
    Listed { names, more, real }
}

/// A line under an open root, in the order the sidebar shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Node(NodeId),
    /// The "… n more" line closing a capped branch (the branch's node).
    More(NodeId),
}

/// Where [`Tree::reveal`] got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reveal {
    /// The folder is the place itself.
    Place,
    /// The folder's line is shown (its ancestors are open).
    Shown(NodeId),
    /// An ancestor is being read (the read to start, if this call opened it); call again when
    /// it is in.
    Wait(Option<Read>),
    /// Not among the shown sub-folders (hidden, gone, past the cap) or under a loop.
    Missing,
}

/// The names from `base` down to `target`, if `target` is `base` or under it.
pub fn rest_under(base: &Path, target: &Path, same: Same) -> Option<Vec<OsString>> {
    let parts: Vec<Component> = target.components().collect();
    let n = base.components().count();
    if parts.len() < n {
        return None;
    }
    let head: PathBuf = parts[..n].iter().collect();
    same(&head, base).then(|| parts[n..].iter().map(|part| part.as_os_str().to_os_string()).collect())
}

/// The place `target` is under with the fewest folders between (the first such in `places`'
/// order), and the names from it down to `target`.
pub fn nearest_place(places: &[RootKey], target: &Path, same: Same) -> Option<(RootKey, Vec<OsString>)> {
    places
        .iter()
        .filter_map(|key| Some((key, rest_under(&key.1, target, same)?)))
        .min_by_key(|(_, rest)| rest.len())
        .map(|(key, rest)| (key.clone(), rest))
}

#[derive(Debug, Default)]
pub struct Tree {
    nodes: HashMap<NodeId, Node>,
    roots: Vec<(RootKey, NodeId)>,
    next_id: NodeId,
    next_ticket: u64,
    /// Bumped whenever the lines may have changed, so the sidebar lays its rows out again only
    /// then (not on every navigation).
    version: u64,
}

impl Tree {
    /// Nothing open, nothing kept.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Changes whenever [`Tree::lines`] or a node's state may have changed.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn root_of(&self, key: &RootKey) -> Option<NodeId> {
        self.roots.iter().find(|(k, _)| k == key).map(|(_, id)| *id)
    }

    fn add(&mut self, path: PathBuf, parent: Option<NodeId>, depth: u16) -> NodeId {
        self.next_id += 1;
        let node =
            Node { path, parent, depth, state: State::Closed, children: Vec::new(), more: 0, ticket: 0, real: None };
        self.nodes.insert(self.next_id, node);
        self.next_id
    }

    fn add_root(&mut self, key: RootKey) -> NodeId {
        let id = self.add(key.1.clone(), None, 0);
        self.roots.push((key, id));
        id
    }

    fn drop_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if let Some(node) = self.nodes.remove(&id) {
                stack.extend(node.children);
            }
        }
    }

    /// A new read of `id`; the one before it, if any, is dropped when it comes.
    fn ticket(&mut self, id: NodeId) -> Option<Read> {
        self.next_ticket += 1;
        let node = self.nodes.get_mut(&id)?;
        node.ticket = self.next_ticket;
        Some(Read { node: id, ticket: self.next_ticket, path: node.path.clone() })
    }

    fn ancestors(&self, id: NodeId) -> impl Iterator<Item = &Node> + '_ {
        std::iter::successors(self.nodes.get(&id).and_then(|n| n.parent), |p| self.nodes.get(p).and_then(|n| n.parent))
            .filter_map(|id| self.nodes.get(&id))
    }

    /// Opens or closes place `key`'s branch: the read to start when it opens.
    pub fn toggle_root(&mut self, key: RootKey) -> Option<Read> {
        match self.root_of(&key) {
            Some(id) => self.toggle(id),
            None => {
                let id = self.add_root(key);
                self.expand(id)
            }
        }
    }

    pub fn toggle(&mut self, id: NodeId) -> Option<Read> {
        match self.nodes.get(&id)?.state {
            State::Open | State::Loading => {
                self.collapse(id);
                None
            }
            State::Closed => self.expand(id),
            State::Empty | State::Loop => None,
        }
    }

    /// Opens a closed folder: the read to start.
    pub fn expand(&mut self, id: NodeId) -> Option<Read> {
        let node = self.nodes.get_mut(&id)?;
        if node.state != State::Closed {
            return None;
        }
        self.version += 1;
        if node.depth >= MAX_DEPTH {
            node.state = State::Loop;
            return None;
        }
        node.state = State::Loading;
        self.ticket(id)
    }

    /// Closes `id`: its sub-folders are let go (opening it again reads it again), a read on its
    /// way is dropped, and a closed root is forgotten.
    pub fn collapse(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get_mut(&id) else { return };
        if !matches!(node.state, State::Open | State::Loading) {
            return;
        }
        node.state = State::Closed;
        node.more = 0;
        node.ticket = 0;
        self.version += 1;
        for child in std::mem::take(&mut node.children) {
            self.drop_subtree(child);
        }
        if let Some(at) = self.roots.iter().position(|(_, root)| *root == id) {
            self.roots.remove(at);
            self.nodes.remove(&id);
        }
    }

    /// Lets go of the roots whose places are no longer in the sidebar, with their branches.
    pub fn keep_roots(&mut self, keys: &[RootKey]) {
        let gone: Vec<NodeId> = self.roots.iter().filter(|(k, _)| !keys.contains(k)).map(|(_, id)| *id).collect();
        if gone.is_empty() {
            return;
        }
        self.version += 1;
        self.roots.retain(|(k, _)| keys.contains(k));
        for id in gone {
            self.drop_subtree(id);
        }
    }

    /// `read` came back: `Some` shows what it found (sub-folders still there keep their own open
    /// branches), `None` (it failed) closes a branch being opened and leaves an open one as it
    /// was. Returns whether anything changed; a read of a branch closed or read again since is
    /// dropped.
    pub fn loaded(&mut self, read: &Read, found: Option<Listed>) -> bool {
        let Some(node) = self.nodes.get(&read.node) else { return false };
        if node.ticket != read.ticket || !matches!(node.state, State::Loading | State::Open) {
            return false;
        }
        match found {
            Some(listed) => {
                self.fill(read.node, listed);
                true
            }
            None if node.state == State::Loading => {
                self.collapse(read.node);
                true
            }
            None => false,
        }
    }

    fn fill(&mut self, id: NodeId, listed: Listed) {
        self.version += 1;
        let looped = listed.real.is_some() && self.ancestors(id).any(|a| a.real == listed.real);
        let Some(node) = self.nodes.get_mut(&id) else { return };
        node.real = listed.real;
        let (path, depth) = (node.path.clone(), node.depth);
        let old = std::mem::take(&mut node.children);
        if looped {
            node.state = State::Loop;
            node.more = 0;
            for child in old {
                self.drop_subtree(child);
            }
            return;
        }
        let mut kept: HashMap<String, NodeId> =
            old.into_iter().filter_map(|c| Some((self.nodes.get(&c)?.name(), c))).collect();
        let children: Vec<NodeId> = listed
            .names
            .into_iter()
            .map(|name| match kept.remove(&name) {
                Some(child) => child,
                None => self.add(path.join(&name), Some(id), depth + 1),
            })
            .collect();
        for (_, gone) in kept {
            self.drop_subtree(gone);
        }
        let Some(node) = self.nodes.get_mut(&id) else { return };
        node.state = if children.is_empty() && listed.more == 0 { State::Empty } else { State::Open };
        node.children = children;
        node.more = listed.more;
    }

    /// Whether a pane's listing of `folder` has something to give: an open branch there, or one
    /// found empty.
    pub fn has_branch_at(&self, folder: &Path, same: Same) -> bool {
        self.nodes.values().any(|n| matches!(n.state, State::Open | State::Empty) && same(&n.path, folder))
    }

    /// A pane listed `folder` (spec 10 §5.2): its open branches show `found` (no read of their
    /// own), a branch found empty gets its arrow back if it has sub-folders now. Returns the
    /// nodes changed.
    pub fn listed(&mut self, folder: &Path, found: &Listed, same: Same) -> Vec<NodeId> {
        let ids: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.state, State::Open | State::Empty) && same(&n.path, folder))
            .map(|(id, _)| *id)
            .collect();
        let mut changed = Vec::new();
        for id in ids {
            let Some(node) = self.nodes.get_mut(&id) else { continue };
            if node.state == State::Empty {
                if !found.names.is_empty() {
                    node.state = State::Closed;
                    self.version += 1;
                    changed.push(id);
                }
                continue;
            }
            let real = node.real.clone();
            // A read already on its way is older than this listing: dropped when it comes.
            self.ticket(id);
            self.fill(id, Listed { names: found.names.clone(), more: found.more, real });
            changed.push(id);
        }
        changed
    }

    /// Gezik's own job changed `folders`: their open branches are read again, those found empty
    /// get their arrow back. Returns the reads and the nodes whose arrow came back.
    pub fn rereads(&mut self, folders: &[PathBuf], same: Same) -> (Vec<Read>, Vec<NodeId>) {
        let hit: Vec<(NodeId, State)> = self
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.state, State::Open | State::Empty) && folders.iter().any(|f| same(&n.path, f)))
            .map(|(id, n)| (*id, n.state))
            .collect();
        let mut reads = Vec::new();
        let mut rearmed = Vec::new();
        for (id, state) in hit {
            if state == State::Empty {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.state = State::Closed;
                    self.version += 1;
                }
                rearmed.push(id);
            } else {
                reads.extend(self.ticket(id));
            }
        }
        (reads, rearmed)
    }

    /// Every open branch read again (the hidden-items setting changed).
    pub fn reread_all(&mut self) -> Vec<Read> {
        let open: Vec<NodeId> = self.nodes.iter().filter(|(_, n)| n.state == State::Open).map(|(id, _)| *id).collect();
        open.into_iter().filter_map(|id| self.ticket(id)).collect()
    }

    /// The lines under `root`, top to bottom: each shown sub-folder, the open ones followed by
    /// their own lines, a capped branch closed by its "… n more" line.
    pub fn lines(&self, root: NodeId) -> Vec<Line> {
        let mut out = Vec::new();
        self.push_lines(root, &mut out);
        out
    }

    fn push_lines(&self, id: NodeId, out: &mut Vec<Line>) {
        let Some(node) = self.nodes.get(&id) else { return };
        if node.state != State::Open {
            return;
        }
        for child in &node.children {
            out.push(Line::Node(*child));
            self.push_lines(*child, out);
        }
        if node.more > 0 {
            out.push(Line::More(id));
        }
    }

    /// One step of opening place `key`'s tree down to the folder `rest` names under it (spec 10
    /// §5.3): the first closed ancestor is opened (its read returned), an ancestor being read
    /// waits; the folder itself stays closed.
    pub fn reveal(&mut self, key: RootKey, rest: &[OsString], same: Same) -> Reveal {
        if rest.is_empty() {
            return Reveal::Place;
        }
        let mut id = match self.root_of(&key) {
            Some(id) => id,
            None => self.add_root(key),
        };
        for name in rest {
            let Some(node) = self.nodes.get(&id) else { return Reveal::Missing };
            match node.state {
                // `expand` refuses only at the depth cap (now a `Loop`): nothing to wait for.
                State::Closed => return self.expand(id).map_or(Reveal::Missing, |read| Reveal::Wait(Some(read))),
                State::Loading => return Reveal::Wait(None),
                State::Empty | State::Loop => return Reveal::Missing,
                State::Open => {}
            }
            // By name first (no allocation per sub-folder); `same` (case-blind on Windows) only
            // when no name matches exactly.
            let exact = node
                .children
                .iter()
                .find(|c| self.nodes.get(c).is_some_and(|n| n.path.file_name() == Some(name.as_os_str())));
            let found = exact.or_else(|| {
                let want = node.path.join(name);
                node.children.iter().find(|c| self.nodes.get(c).is_some_and(|n| same(&n.path, &want)))
            });
            match found {
                Some(child) => id = *child,
                None => return Reveal::Missing,
            }
        }
        Reveal::Shown(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &Path, b: &Path) -> bool {
        a == b
    }

    fn key(path: &str) -> RootKey {
        (2, PathBuf::from(path))
    }

    fn found(names: &[&str]) -> Option<Listed> {
        Some(prepare(names.iter().map(|s| (*s).to_owned()).collect(), None))
    }

    fn names(tree: &Tree, lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| match line {
                Line::Node(id) => tree.node(*id).unwrap().name(),
                Line::More(id) => format!("+{}", tree.node(*id).unwrap().more),
            })
            .collect()
    }

    fn child(tree: &Tree, parent: NodeId, name: &str) -> NodeId {
        *tree.node(parent).unwrap().children.iter().find(|c| tree.node(**c).unwrap().name() == name).unwrap()
    }

    #[test]
    fn nothing_is_read_or_kept_until_a_branch_opens() {
        let mut tree = Tree::default();
        assert!(tree.is_empty());
        assert_eq!(tree.root_of(&key("/r")), None);
        assert!(tree.reread_all().is_empty());
        assert_eq!(tree.rereads(&[PathBuf::from("/r")], &same), (Vec::new(), Vec::new()));
        assert!(!tree.has_branch_at(Path::new("/r"), &same));
        assert!(tree.is_empty(), "asking reads nothing and keeps nothing (spec 10 §5.4)");
        let read = tree.toggle_root(key("/r")).expect("opening reads");
        assert_eq!(read.path, PathBuf::from("/r"));
        let root = tree.root_of(&key("/r")).unwrap();
        assert_eq!((tree.node(root).unwrap().state, State::Loading.arrow()), (State::Loading, 3));
        assert!(tree.lines(root).is_empty(), "nothing shown while it reads");
        assert!(tree.loaded(&read, found(&["b", "a"])));
        assert_eq!(names(&tree, &tree.lines(root)), ["a", "b"]);
        assert_eq!(tree.toggle_root(key("/r")), None, "closing reads nothing");
        assert!(tree.is_empty(), "a closed root is forgotten with its branch");
    }

    #[test]
    fn a_branch_is_in_natural_order_and_capped() {
        let listed = prepare(vec!["a10".into(), "a2".into(), "B".into(), "a2".into()], None);
        assert_eq!(listed.names, ["a2", "a10", "B"]);
        assert_eq!(listed.more, 0);
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        let many: Vec<String> = (0..BRANCH_CAP + 5).map(|i| format!("d{i}")).collect();
        assert!(tree.loaded(&read, Some(prepare(many, None))));
        let root = tree.root_of(&key("/r")).unwrap();
        let lines = tree.lines(root);
        assert_eq!(lines.len(), BRANCH_CAP + 1);
        assert_eq!(lines.last(), Some(&Line::More(root)));
        assert_eq!(tree.node(root).unwrap().more, 5);
        assert_eq!(names(&tree, &lines[..3]), ["d0", "d1", "d2"]);
    }

    #[test]
    fn a_late_read_of_a_closed_branch_is_dropped() {
        let mut tree = Tree::default();
        let slow = tree.toggle_root(key("/net")).unwrap();
        assert_eq!(tree.toggle_root(key("/net")), None, "closed before the share answered");
        assert!(!tree.loaded(&slow, found(&["x"])));
        assert!(tree.is_empty());
        let again = tree.toggle_root(key("/net")).unwrap();
        assert_ne!(again.ticket, slow.ticket);
        assert!(!tree.loaded(&slow, found(&["old"])), "the first read is still not the one asked for");
        assert!(tree.loaded(&again, found(&["new"])));
        let root = tree.root_of(&key("/net")).unwrap();
        assert_eq!(names(&tree, &tree.lines(root)), ["new"]);
        assert_eq!(tree.expand(root), None, "an open branch is not read twice");
    }

    #[test]
    fn a_reread_keeps_open_subbranches_by_name() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["a", "b"]));
        let root = tree.root_of(&key("/r")).unwrap();
        let a = child(&tree, root, "a");
        let b = child(&tree, root, "b");
        let read_a = tree.expand(a).unwrap();
        tree.loaded(&read_a, found(&["a1"]));
        let (reads, _) = tree.rereads(&[PathBuf::from("/r")], &same);
        assert_eq!(reads.len(), 1, "only the open branch at /r");
        assert!(tree.loaded(&reads[0], found(&["a", "c"])));
        assert_eq!(names(&tree, &tree.lines(root)), ["a", "a1", "c"], "b gone, c new, a still open");
        assert_eq!(child(&tree, root, "a"), a);
        assert!(tree.node(b).is_none(), "the gone folder's node is let go");
    }

    #[test]
    fn a_failed_open_closes_and_a_failed_reread_keeps_the_branch() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/gone")).unwrap();
        assert!(tree.loaded(&read, None));
        assert!(tree.is_empty(), "the arrow is back, nothing kept");
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["a"]));
        let again = tree.rereads(&[PathBuf::from("/r")], &same).0.remove(0);
        assert!(!tree.loaded(&again, None));
        let root = tree.root_of(&key("/r")).unwrap();
        assert_eq!(names(&tree, &tree.lines(root)), ["a"]);
    }

    #[test]
    fn a_loop_shows_but_does_not_open() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, Some(prepare(vec!["link".into()], Some(PathBuf::from("/real/r")))));
        let root = tree.root_of(&key("/r")).unwrap();
        let link = child(&tree, root, "link");
        let read = tree.expand(link).unwrap();
        // /r/link resolves to /real/r, the folder above it.
        assert!(tree.loaded(&read, Some(prepare(vec!["link".into()], Some(PathBuf::from("/real/r"))))));
        let node = tree.node(link).unwrap();
        assert_eq!((node.state, node.state.arrow()), (State::Loop, 0));
        assert!(node.children.is_empty());
        assert_eq!(tree.expand(link), None, "never read again");
        assert_eq!(names(&tree, &tree.lines(root)), ["link"], "the link itself is shown");
    }

    #[test]
    fn the_depth_cap_stops_a_loop_the_read_cannot_see() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["d"]));
        let mut id = tree.root_of(&key("/r")).unwrap();
        for _ in 1..MAX_DEPTH {
            id = child(&tree, id, "d");
            let read = tree.expand(id).unwrap();
            tree.loaded(&read, found(&["d"]));
        }
        let deepest = child(&tree, id, "d");
        assert_eq!(tree.node(deepest).unwrap().depth, MAX_DEPTH);
        assert_eq!(tree.expand(deepest), None);
        assert_eq!(tree.node(deepest).unwrap().state, State::Loop);
    }

    #[test]
    fn a_pane_listing_fills_open_branches_and_gives_empty_ones_their_arrow() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["a", "e"]));
        let root = tree.root_of(&key("/r")).unwrap();
        let e = child(&tree, root, "e");
        let read = tree.expand(e).unwrap();
        tree.loaded(&read, found(&[]));
        assert_eq!((tree.node(e).unwrap().state, State::Empty.arrow()), (State::Empty, 0));
        assert!(tree.has_branch_at(Path::new("/r"), &same));
        assert!(!tree.has_branch_at(Path::new("/r/a"), &same), "closed: not kept up");
        let listing = prepare(vec!["n".into(), "a".into(), "e".into()], None);
        assert_eq!(tree.listed(Path::new("/r"), &listing, &same), vec![root]);
        assert_eq!(names(&tree, &tree.lines(root)), ["a", "e", "n"]);
        assert_eq!(tree.listed(Path::new("/r/e"), &prepare(vec!["sub".into()], None), &same), vec![e]);
        assert_eq!(tree.node(e).unwrap().state, State::Closed, "an arrow again, nothing read");
        assert!(tree.listed(Path::new("/r/a"), &prepare(vec!["x".into()], None), &same).is_empty());
    }

    #[test]
    fn a_read_older_than_a_pane_listing_is_dropped() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["a"]));
        let root = tree.root_of(&key("/r")).unwrap();
        let (reads, _) = tree.rereads(&[PathBuf::from("/r")], &same);
        tree.listed(Path::new("/r"), &prepare(vec!["a".into(), "new".into()], None), &same);
        assert!(!tree.loaded(&reads[0], found(&["a"])), "the listing is newer than the read");
        assert_eq!(names(&tree, &tree.lines(root)), ["a", "new"]);
    }

    #[test]
    fn the_version_moves_only_when_the_lines_may_have() {
        let mut tree = Tree::default();
        let v = tree.version();
        let read = tree.toggle_root(key("/r")).unwrap();
        let opened = tree.version();
        assert_ne!(opened, v);
        assert!(tree.reread_all().is_empty());
        tree.keep_roots(&[key("/r")]);
        assert!(!tree.has_branch_at(Path::new("/r"), &same));
        assert_eq!(tree.version(), opened, "asking and keeping change nothing");
        tree.loaded(&read, found(&["a"]));
        assert_ne!(tree.version(), opened);
        let filled = tree.version();
        tree.reread_all();
        assert_eq!(tree.version(), filled, "a read on its way shows nothing new yet");
        tree.keep_roots(&[]);
        assert_ne!(tree.version(), filled);
    }

    #[test]
    fn geziks_jobs_reread_only_open_branches() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["a", "e"]));
        let root = tree.root_of(&key("/r")).unwrap();
        let e = child(&tree, root, "e");
        let read = tree.expand(e).unwrap();
        tree.loaded(&read, found(&[]));
        let (reads, rearmed) = tree.rereads(&[PathBuf::from("/r/e"), PathBuf::from("/elsewhere")], &same);
        assert!(reads.is_empty(), "/r/e is not open; /elsewhere is not in the tree");
        assert_eq!(rearmed, [e]);
        assert_eq!(tree.node(e).unwrap().state, State::Closed, "a new folder may be there: the arrow comes back");
        let all: Vec<PathBuf> = tree.reread_all().into_iter().map(|r| r.path).collect();
        assert_eq!(all, [PathBuf::from("/r")]);
    }

    #[test]
    fn reveal_opens_each_ancestor_then_shows_the_folder() {
        let mut tree = Tree::default();
        let rest = rest_under(Path::new("/r"), Path::new("/r/a/b"), &same).unwrap();
        assert_eq!(rest, ["a", "b"].map(OsString::from));
        let Reveal::Wait(Some(read)) = tree.reveal(key("/r"), &rest, &same) else { panic!("the root is read first") };
        assert_eq!(read.path, PathBuf::from("/r"));
        assert_eq!(tree.reveal(key("/r"), &rest, &same), Reveal::Wait(None), "the read is on its way");
        tree.loaded(&read, found(&["a", "z"]));
        let Reveal::Wait(Some(read)) = tree.reveal(key("/r"), &rest, &same) else { panic!("then /r/a") };
        assert_eq!(read.path, PathBuf::from("/r/a"));
        tree.loaded(&read, found(&["b"]));
        let root = tree.root_of(&key("/r")).unwrap();
        let b = child(&tree, child(&tree, root, "a"), "b");
        assert_eq!(tree.reveal(key("/r"), &rest, &same), Reveal::Shown(b));
        assert_eq!(tree.node(b).unwrap().state, State::Closed, "the folder itself stays closed");
        assert_eq!(tree.reveal(key("/r"), &[], &same), Reveal::Place);
        let hidden = rest_under(Path::new("/r"), Path::new("/r/.git"), &same).unwrap();
        assert_eq!(tree.reveal(key("/r"), &hidden, &same), Reveal::Missing, "not among the shown sub-folders");
    }

    #[test]
    fn the_nearest_place_is_the_deepest_one_holding_the_folder() {
        let places = [(0, PathBuf::from("/home/u")), (0, PathBuf::from("/home/u/Documents")), (2, PathBuf::from("/"))];
        let (key, rest) = nearest_place(&places, Path::new("/home/u/Documents/x/y"), &same).unwrap();
        assert_eq!((key, rest.len()), (places[1].clone(), 2));
        assert_eq!(nearest_place(&places, Path::new("/etc"), &same).unwrap().0, places[2]);
        assert_eq!(nearest_place(&places[..2], Path::new("/etc"), &same), None);
        assert!(
            rest_under(Path::new("/home/u"), Path::new("/home/user"), &same).is_none(),
            "a name's start is no folder"
        );
        let twice = [(1, PathBuf::from("/w")), (0, PathBuf::from("/w"))];
        assert_eq!(nearest_place(&twice, Path::new("/w/x"), &same).unwrap().0, twice[0], "the first in the sidebar");
    }

    #[test]
    fn a_place_that_leaves_the_sidebar_takes_its_branch() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/usb")).unwrap();
        tree.loaded(&read, found(&["a"]));
        let slow = tree.toggle_root(key("/net")).unwrap();
        tree.keep_roots(&[key("/other")]);
        assert!(tree.is_empty());
        assert!(!tree.loaded(&slow, found(&["x"])), "a read for a place gone is dropped");
    }

    #[test]
    fn an_empty_folder_and_one_with_every_sub_folder_hidden_lose_their_arrow() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        // The worker leaves hidden folders out before `prepare`: all hidden is an empty read.
        assert!(tree.loaded(&read, found(&[])));
        let root = tree.root_of(&key("/r")).unwrap();
        assert_eq!(tree.node(root).unwrap().state.arrow(), 0);
        assert!(tree.lines(root).is_empty());
        assert_eq!(tree.toggle_root(key("/r")), None, "nothing to open or close");
        assert_eq!(tree.root_of(&key("/r")), Some(root));
        // An open branch whose sub-folders all became hidden (a pane's filtered listing).
        let read = tree.toggle_root(key("/s")).unwrap();
        tree.loaded(&read, found(&["a"]));
        let s = tree.root_of(&key("/s")).unwrap();
        let a = child(&tree, s, "a");
        assert_eq!(tree.listed(Path::new("/s"), &prepare(Vec::new(), None), &same), vec![s]);
        assert_eq!(tree.node(s).unwrap().state, State::Empty);
        assert!(tree.node(a).is_none());
    }

    #[test]
    fn the_cap_holds_exactly_twenty_thousand() {
        let at = |n: usize| prepare((0..n).map(|i| format!("d{i}")).collect(), None);
        let full = at(BRANCH_CAP);
        assert_eq!((full.names.len(), full.more), (BRANCH_CAP, 0));
        let over = at(BRANCH_CAP + 1);
        assert_eq!((over.names.len(), over.more), (BRANCH_CAP, 1));
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, Some(full));
        let root = tree.root_of(&key("/r")).unwrap();
        assert!(tree.lines(root).iter().all(|line| matches!(line, Line::Node(_))), "no \"… more\" line");
        let read = tree.rereads(&[PathBuf::from("/r")], &same).0.remove(0);
        tree.loaded(&read, Some(over));
        let lines = tree.lines(root);
        assert_eq!((lines.len(), lines.last()), (BRANCH_CAP + 1, Some(&Line::More(root))));
        // Repeats count once, before the cap.
        let mut twice: Vec<String> = (0..BRANCH_CAP).map(|i| format!("d{i}")).collect();
        twice.push("d0".into());
        assert_eq!(prepare(twice, None).more, 0);
    }

    #[test]
    fn unicode_names_sort_as_the_list_does() {
        let names = ["şey", "Zeki", "sabun", "çay", "ılık", "cam", "iğne", "日本", "Ölçü", "ok"];
        let listed = prepare(names.iter().map(|s| (*s).to_owned()).collect(), None);
        assert_eq!(listed.names, ["cam", "çay", "ılık", "iğne", "ok", "Ölçü", "sabun", "şey", "Zeki", "日本"]);
        let case = prepare(vec!["b".into(), "B".into(), "a".into(), "b".into()], None);
        assert_eq!(case.names, ["a", "B", "b"], "names differing only by case are both kept");
    }

    #[test]
    fn a_link_back_to_a_grandparent_or_its_own_parent_is_a_loop() {
        let real = |p: &str| Some(PathBuf::from(p));
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("C:/r")).unwrap();
        tree.loaded(&read, Some(prepare(vec!["a".into()], real("D:/r"))));
        let root = tree.root_of(&key("C:/r")).unwrap();
        let a = child(&tree, root, "a");
        let read = tree.expand(a).unwrap();
        tree.loaded(&read, Some(prepare(vec!["j".into(), "same".into(), "ok".into()], real("D:/r/a"))));
        // `j` is a junction to C:/r (two up), `same` a link to `.` (its parent).
        for (name, to) in [("j", "D:/r"), ("same", "D:/r/a")] {
            let id = child(&tree, a, name);
            let read = tree.expand(id).unwrap();
            tree.loaded(&read, Some(prepare(vec!["a".into()], real(to))));
            assert_eq!(tree.node(id).unwrap().state, State::Loop, "{name}");
        }
        // A link to a sibling folder is no loop.
        let ok = child(&tree, a, "ok");
        let read = tree.expand(ok).unwrap();
        tree.loaded(&read, Some(prepare(vec!["x".into()], real("D:/elsewhere"))));
        assert_eq!(tree.node(ok).unwrap().state, State::Open);
        // A reread of a looped folder's parent keeps it a loop (no arrow comes back).
        let read = tree.rereads(&[PathBuf::from("C:/r/a")], &same).0.remove(0);
        tree.loaded(&read, Some(prepare(vec!["j".into(), "ok".into()], real("D:/r/a"))));
        assert_eq!(tree.node(child(&tree, a, "j")).unwrap().state, State::Loop);
        assert_eq!(names(&tree, &tree.lines(root)), ["a", "j", "ok", "x"]);
    }

    #[test]
    fn reveal_stops_at_the_depth_cap_instead_of_waiting() {
        let mut tree = Tree::default();
        let rest: Vec<OsString> = (0..=MAX_DEPTH).map(|_| OsString::from("d")).collect();
        let mut steps = 0;
        loop {
            match tree.reveal(key("/r"), &rest, &same) {
                Reveal::Wait(Some(read)) => {
                    tree.loaded(&read, found(&["d"]));
                    steps += 1;
                }
                other => {
                    assert_eq!(other, Reveal::Missing);
                    break;
                }
            }
        }
        assert_eq!(steps, MAX_DEPTH, "the root and each folder above the cap were read once");
    }

    #[test]
    fn reopening_a_closed_sub_branch_reads_it_again() {
        let mut tree = Tree::default();
        let read = tree.toggle_root(key("/r")).unwrap();
        tree.loaded(&read, found(&["a"]));
        let root = tree.root_of(&key("/r")).unwrap();
        let a = child(&tree, root, "a");
        let read = tree.toggle(a).unwrap();
        tree.loaded(&read, found(&["a1", "a2"]));
        let a1 = child(&tree, a, "a1");
        assert_eq!(tree.toggle(a), None, "closing");
        assert!(tree.node(a1).is_none(), "a closed branch's nodes are let go");
        assert_eq!(names(&tree, &tree.lines(root)), ["a"]);
        let again = tree.toggle(a).expect("a new read");
        assert!(!tree.loaded(&read, found(&["stale"])), "the first read's ticket is old");
        assert!(tree.loaded(&again, found(&["a3"])));
        assert_eq!(names(&tree, &tree.lines(root)), ["a", "a3"]);
    }
}
