//! The folder compare's scan (spec 11 §7.4): one side's tree, read on the caller's thread (the
//! compare runs the two sides on two low-priority threads at once). Links are never gone into
//! nor compared, only counted; a folder the filter leaves out is never read; a cloud folder is
//! not gone into; each folder's children sit side by side, sorted by their matching key.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gezik_core::compare::{Kind, NO_TIME, Node, Tree, seconds};
use gezik_core::file_filter::FileFilter;
use gezik_core::pattern::fold_text;
use gezik_platform::fs::{describe, read_dir_items};

/// Most items a side may have (spec 11 §7.4).
pub const MAX_ITEMS: usize = 2_000_000;

/// What a side's scan takes and leaves out.
pub struct Rules<'a> {
    pub subfolders: bool,
    pub filter: &'a FileFilter,
    /// Whether the compared folders' own files are taken (a taken folder pattern the folder's
    /// name does not match leaves them out, as 11i's `top_folder` says).
    pub root_taken: bool,
    /// `[view] show-hidden` and `show-system`.
    pub shown: (bool, bool),
    pub case_sensitive: bool,
    pub max_items: usize,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub tree: Tree,
    /// Links and items that are neither files nor folders, left out.
    pub links: usize,
    /// Files and folders whose data is not on this disk (never read).
    pub online: usize,
    /// More than `max_items`: the scan stopped there.
    pub too_many: bool,
    pub stopped: bool,
}

/// The name as names are matched: NFC (macOS's NFD names meet FAT's and NTFS's NFC ones; only
/// names that are not ASCII are looked at), folded as the pattern language folds unless
/// `case_sensitive`.
pub fn name_key(name: &str, case_sensitive: bool) -> String {
    let nfc = if name.is_ascii() {
        Cow::Borrowed(name)
    } else {
        icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(name)
    };
    if case_sensitive { nfc.into_owned() } else { fold_text(&nfc) }
}

/// Node `i`'s path under `root`.
pub fn path_of(root: &Path, tree: &Tree, i: u32) -> PathBuf {
    let mut path = root.to_path_buf();
    path.extend(tree.names(i));
    path
}

/// Reads `root`'s tree. `read` counts the folders read; `stop` ends the scan soon.
pub fn scan(root: &Path, rules: &Rules, stop: &AtomicBool, read: &AtomicUsize) -> Scan {
    let mut scan = Scan::default();
    let tree = &mut scan.tree;
    tree.nodes.push(Node {
        name: "".into(),
        key: None,
        size: 0,
        modified: NO_TIME,
        parent: 0,
        first: 0,
        count: 0,
        kind: Kind::Folder,
    });
    // (node, path, whether its files are taken, depth)
    let mut pending = vec![(0u32, root.to_path_buf(), rules.root_taken, 0usize)];
    while let Some((at, dir, taken, depth)) = pending.pop() {
        if stop.load(Ordering::Relaxed) {
            scan.stopped = true;
            break;
        }
        let items = match read_dir_items(&dir, &|_, _| true) {
            Ok(items) => items,
            Err(err) => {
                tree.nodes[at as usize].kind = Kind::Unread;
                tree.problems.push((at, describe(&err).into()));
                continue;
            }
        };
        read.fetch_add(1, Ordering::Relaxed);
        let mut children = Vec::new();
        for item in items {
            if !gezik_core::is_shown_name(&item.name, item.flags, rules.shown.0, rules.shown.1) {
                continue;
            }
            if item.is_link || !(item.is_dir || item.is_file) {
                scan.links += 1;
                continue;
            }
            let kind = if item.is_dir {
                if !rules.subfolders || rules.filter.enters(&item.name, taken).is_none() {
                    continue;
                }
                if item.offline { Kind::Unread } else { Kind::Folder }
            } else {
                if !taken || !rules.filter.takes_file(&item.name) {
                    continue;
                }
                if item.offline { Kind::OnlineFile } else { Kind::File }
            };
            scan.online += usize::from(item.offline);
            let key = name_key(&item.name, rules.case_sensitive);
            children.push(Node {
                key: (key != item.name).then(|| key.into()),
                name: item.name.into(),
                size: if item.is_dir { 0 } else { item.size },
                modified: seconds(item.modified),
                parent: at,
                first: 0,
                count: 0,
                kind,
            });
        }
        if tree.nodes.len() + children.len() > rules.max_items {
            scan.too_many = true;
            break;
        }
        children.sort_unstable_by(|a, b| a.key().cmp(b.key()));
        let first = u32::try_from(tree.nodes.len()).unwrap_or(u32::MAX);
        let count = u32::try_from(children.len()).unwrap_or(u32::MAX);
        for (i, mut child) in (first..).zip(children) {
            if child.kind == Kind::Folder && depth + 1 >= crate::walk::MAX_DEPTH {
                child.kind = Kind::Unread;
                tree.problems.push((i, "Too deep".into()));
            } else if child.kind == Kind::Folder {
                pending.push((i, dir.join(&*child.name), true, depth + 1));
            } else if child.kind == Kind::Unread {
                tree.problems.push((i, "Online only".into()));
            }
            tree.nodes.push(child);
        }
        let node = &mut tree.nodes[at as usize];
        node.first = first;
        node.count = count;
    }
    scan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-tree-scan-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, files: &[&str]) {
        for file in files {
            let path = root.join(file);
            if file.ends_with('/') {
                std::fs::create_dir_all(path).unwrap();
            } else {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, file.as_bytes()).unwrap();
            }
        }
    }

    fn rules(filter: &FileFilter) -> Rules<'_> {
        Rules {
            subfolders: true,
            filter,
            root_taken: true,
            shown: (false, false),
            case_sensitive: true,
            max_items: MAX_ITEMS,
        }
    }

    fn run(root: &Path, rules: &Rules) -> (Scan, usize) {
        let read = AtomicUsize::new(0);
        let scan = scan(root, rules, &AtomicBool::new(false), &read);
        (scan, read.into_inner())
    }

    /// Every path of the tree, folders with `/`.
    fn paths(tree: &Tree) -> Vec<String> {
        let mut all: Vec<String> = (1..tree.nodes.len() as u32)
            .map(|i| {
                let slash = if tree.nodes[i as usize].kind == Kind::Folder { "/" } else { "" };
                format!("{}{slash}", tree.names(i).join("/"))
            })
            .collect();
        all.sort();
        all
    }

    #[test]
    fn a_tree_with_its_children_side_by_side() {
        let root = dir("tree");
        write(&root, &["b.txt", "a/x.txt", "a/sub/y.txt", "c/", ".hidden"]);
        let none = FileFilter::default();
        let (scan, read) = run(&root, &rules(&none));
        assert_eq!(paths(&scan.tree), ["a/", "a/sub/", "a/sub/y.txt", "a/x.txt", "b.txt", "c/"]);
        assert_eq!(read, 4, "the root, a, a/sub and c");
        let names: Vec<&str> = (1..4).map(|i| &*scan.tree.nodes[i].name).collect();
        assert_eq!(names, ["a", "b.txt", "c"], "the root's children first, sorted");
        let file = scan.tree.nodes.iter().position(|n| &*n.name == "y.txt").unwrap() as u32;
        assert_eq!(path_of(&root, &scan.tree, file), root.join("a").join("sub").join("y.txt"));
        assert_eq!(scan.tree.nodes[file as usize].size, 11);
        assert!(scan.tree.nodes[file as usize].modified > 1_600_000_000);
        // Hidden ones as the view says.
        let shown = Rules { shown: (true, true), ..rules(&none) };
        assert!(paths(&run(&root, &shown).0.tree).contains(&".hidden".to_owned()));
        // Without subfolders: the root's files only.
        let flat = Rules { subfolders: false, ..rules(&none) };
        let (scan, read) = run(&root, &flat);
        assert_eq!((paths(&scan.tree), read), (vec!["b.txt".to_owned()], 1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_the_filter_leaves_out_is_never_read() {
        let root = dir("filter");
        write(&root, &["a.jpg", "a.txt", "thumbs/t.jpg", "thumbs/deep/u.jpg", "keep/k.jpg", "keep/k.tmp"]);
        let filter = FileFilter::parse("*.jpg | thumbs\\ *.tmp", false).unwrap();
        let (scan, read) = run(&root, &rules(&filter));
        assert_eq!(paths(&scan.tree), ["a.jpg", "keep/", "keep/k.jpg"]);
        assert_eq!(read, 2, "thumbs and what is under it not read");
        // Taken folder patterns: only into those; the root's own files by its name.
        let filter = FileFilter::parse("keep\\", false).unwrap();
        let only = Rules { root_taken: false, ..rules(&filter) };
        assert_eq!(paths(&run(&root, &only).0.tree), ["keep/", "keep/k.jpg", "keep/k.tmp"]);
        assert!(FileFilter::parse("*.jpg>*.cr2", false).is_err(), "no related files in a compare");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn links_are_counted_not_followed() {
        let root = dir("links");
        write(&root, &["real/a.txt", "f.txt"]);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(root.join("real"), root.join("link")).is_ok()
            && std::os::unix::fs::symlink(root.join("f.txt"), root.join("flink")).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(root.join("real"), root.join("link")).is_ok()
            && std::os::windows::fs::symlink_file(root.join("f.txt"), root.join("flink")).is_ok();
        if !made {
            // Windows without the right to make links: nothing to look at.
            let _ = std::fs::remove_dir_all(&root);
            return;
        }
        let none = FileFilter::default();
        let (scan, read) = run(&root, &rules(&none));
        assert_eq!(paths(&scan.tree), ["f.txt", "real/", "real/a.txt"]);
        assert_eq!((scan.links, read), (2, 2), "the linked folder is not read again");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_meet_in_nfc_and_by_the_case_rule() {
        let nfd = "Cafe\u{301}.txt";
        let nfc = "Caf\u{e9}.txt";
        assert_eq!(name_key(nfd, true), name_key(nfc, true));
        assert_eq!(name_key(nfd, true), nfc);
        assert_eq!(name_key("Docs", false), name_key("docs", false));
        assert_ne!(name_key("Docs", true), name_key("docs", true), "Linux tells them apart");
        assert_eq!(name_key("IRMAK", false), name_key("ırmak", false), "6a's folding");
        // The scan keeps a key only where it is not the name.
        let root = dir("keys");
        write(&root, &["B.txt", nfd]);
        let none = FileFilter::default();
        let folded = Rules { case_sensitive: false, ..rules(&none) };
        let (scan, _) = run(&root, &folded);
        let keys: Vec<&str> = scan.tree.nodes[1..].iter().map(Node::key).collect();
        assert_eq!(keys, [name_key("b.txt", false), name_key(nfc, false)]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_limit_and_the_stop() {
        let root = dir("limit");
        write(&root, &["a", "b", "c", "d/e"]);
        let none = FileFilter::default();
        let small = Rules { max_items: 3, ..rules(&none) };
        assert!(run(&root, &small).0.too_many);
        let read = AtomicUsize::new(0);
        let stopped = scan(&root, &rules(&none), &AtomicBool::new(true), &read);
        assert!(stopped.stopped && read.into_inner() == 0 && stopped.tree.nodes.len() == 1);
        // A folder that cannot be read is said, not dropped.
        let gone = scan(&root.join("missing"), &rules(&none), &AtomicBool::new(false), &AtomicUsize::new(0));
        assert_eq!(gone.tree.nodes[0].kind, Kind::Unread);
        assert!(gone.tree.problem(0).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}
