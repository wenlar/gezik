//! Gezik's own tag store, for files whose file system keeps no tags (spec 12 §4.4): one line a
//! file, keyed by volume and the path in it; size, modification time and file id only find a
//! file moved elsewhere and keep a reused path from showing another file's tags.

use std::collections::BTreeMap;

use crate::meta::{Meta, meta_from_lines, meta_lines, same_name};

pub const MAX_RECORDS: usize = 200_000;
const HEADER: &str = "# gezik tags store";
pub const FULL: &str = "The tag store is full (200,000 files); clean it up or tag files on NTFS, APFS or ext4";
pub const LATER: &str = "The tag store was written by a later Gezik; it is left as it is";

/// Paths compare as the system compares them (case-insensitive on Windows and macOS).
const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    /// In the volume, from its root, with `/`.
    pub path: Box<str>,
    pub size: u64,
    /// Modification time, nanoseconds since 1970.
    pub mtime_ns: i64,
    /// Where the system gives a stable one (NTFS, ext4, APFS): its file number.
    pub file_id: Option<u64>,
    pub meta: Meta,
    /// Meta lines this version does not know, written back as they were.
    pub unknown: Vec<Box<str>>,
}

impl Record {
    /// Whether the file now at its path is, as far as can be told, still this one. Where both
    /// file ids are known they decide (a file made anew under the old name has another);
    /// otherwise a path reused by another file differs in size and time.
    pub fn fits(&self, size: u64, mtime_ns: i64, file_id: Option<u64>) -> bool {
        match (self.file_id, file_id) {
            (Some(a), Some(b)) => a == b,
            _ => self.size == size || self.mtime_ns == mtime_ns,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Store {
    records: BTreeMap<(Box<str>, Box<str>), Record>,
    /// Lines that are not records: written back as they were.
    kept: Vec<String>,
    /// Written by a later version: read, never written.
    pub read_only: bool,
}

fn key_path(path: &str) -> Box<str> {
    if IGNORE_CASE { path.to_lowercase().into() } else { path.into() }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\x1f' => out.push_str("\\u"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('u') => out.push('\x1f'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

impl Store {
    pub fn parse(text: &str) -> Store {
        let mut store = Store::default();
        for (i, line) in text.lines().enumerate() {
            if i == 0
                && let Some(version) = line.strip_prefix(HEADER)
            {
                store.read_only = version.trim() != "1";
                continue;
            }
            match parse_line(line) {
                Some((volume, record)) => {
                    store.records.insert((volume, key_path(&record.path)), record);
                }
                None if line.is_empty() => {}
                None => store.kept.push(line.to_owned()),
            }
        }
        store
    }

    pub fn to_text(&self) -> String {
        let mut text = format!("{HEADER} 1\n");
        for ((volume, _), record) in &self.records {
            let mut lines = meta_lines(&record.meta);
            lines.extend(record.unknown.iter().map(|line| line.to_string()));
            let id = record.file_id.map_or_else(|| "-".to_owned(), |id| id.to_string());
            let fields = [
                escape(volume),
                escape(&record.path),
                record.size.to_string(),
                record.mtime_ns.to_string(),
                id,
                escape(&lines.join("\x1f")),
            ];
            text.push_str(&fields.join("\t"));
            text.push('\n');
        }
        for line in &self.kept {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn get(&self, volume: &str, path: &str) -> Option<&Record> {
        self.records.get(&(volume.into(), key_path(path)))
    }

    /// Puts `record` in (one with nothing in it goes); refused when full or written by a later
    /// version.
    pub fn set(&mut self, volume: &str, record: Record) -> Result<(), &'static str> {
        if self.read_only {
            return Err(LATER);
        }
        let key = (Box::<str>::from(volume), key_path(&record.path));
        if record.meta.is_empty() && record.unknown.is_empty() {
            self.records.remove(&key);
            return Ok(());
        }
        if !self.records.contains_key(&key) && self.records.len() >= MAX_RECORDS {
            return Err(FULL);
        }
        self.records.insert(key, record);
        Ok(())
    }

    /// The keys of `path` and of everything inside it.
    fn tree_keys(&self, volume: &str, path: &str) -> Vec<(Box<str>, Box<str>)> {
        let exact = key_path(path);
        let inside = format!("{}/", exact.trim_end_matches('/'));
        let start = (Box::<str>::from(volume), exact.clone());
        let mut keys: Vec<_> = self.records.get_key_value(&start).map(|(k, _)| k.clone()).into_iter().collect();
        let from = (Box::<str>::from(volume), Box::<str>::from(inside.as_str()));
        keys.extend(
            self.records
                .range(from..)
                .take_while(|((v, p), _)| &**v == volume && p.starts_with(&inside))
                .map(|(k, _)| k.clone()),
        );
        keys
    }

    /// Drops `path` and everything inside it (deleted for good); how many went.
    pub fn remove_tree(&mut self, volume: &str, path: &str) -> usize {
        if self.read_only {
            return 0;
        }
        let keys = self.tree_keys(volume, path);
        for key in &keys {
            self.records.remove(key);
        }
        keys.len()
    }

    /// `from` and everything inside it now at `to` (a rename or a move); how many moved.
    pub fn rename(&mut self, volume: &str, from: &str, to_volume: &str, to: &str) -> usize {
        self.rename_all(&[(volume, from, to_volume, to)])
    }

    /// Several renames done together (volume, from, to volume, to): every source's entries are
    /// taken out first, then put in at their new places, so swapped names (`a` ↔ `b`) and chains
    /// (`1` → `2`, `2` → `3`) keep each file's own tags.
    pub fn rename_all(&mut self, moves: &[(&str, &str, &str, &str)]) -> usize {
        if self.read_only {
            return 0;
        }
        let mut taken = Vec::new();
        for &(volume, from, to_volume, to) in moves {
            let from_len = from.trim_end_matches('/').len();
            for key in self.tree_keys(volume, from) {
                let Some(mut record) = self.records.remove(&key) else { continue };
                let rest = record.path.get(from_len..).unwrap_or("").to_owned();
                record.path = format!("{}{rest}", to.trim_end_matches('/')).into();
                taken.push((to_volume, record));
            }
        }
        let count = taken.len();
        for (volume, record) in taken {
            self.records.insert((volume.into(), key_path(&record.path)), record);
        }
        count
    }

    /// Drops the entry of `path` alone (what is inside a folder stays).
    pub fn remove(&mut self, volume: &str, path: &str) -> bool {
        !self.read_only && self.records.remove(&(volume.into(), key_path(path))).is_some()
    }

    /// A record whose path is gone (`gone` says so; at most 64 are asked) that is the file
    /// named `name` found on `volume`: the same name with the same size and modification time
    /// (FAT keeps it to 2 s), or the same file id. Finds a file renamed or moved on its volume
    /// by another program.
    pub fn refind(
        &self,
        volume: &str,
        name: &str,
        size: u64,
        mtime_ns: i64,
        file_id: Option<u64>,
        mut gone: impl FnMut(&str) -> bool,
    ) -> Option<&Record> {
        const TOLERANCE_NS: i64 = 2_000_000_000;
        let name = key_path(name);
        self.records
            .iter()
            .filter(|((v, _), _)| &**v == volume)
            .map(|(_, record)| record)
            .filter(|record| {
                let same_id = matches!((record.file_id, file_id), (Some(a), Some(b)) if a == b);
                let base = record.path.rsplit('/').next().unwrap_or(&record.path);
                // The same size and time always; then the same name, or the same file id (a rename).
                record.size == size
                    && (record.mtime_ns - mtime_ns).abs() <= TOLERANCE_NS
                    && (same_id || *key_path(base) == *name)
            })
            .take(64)
            .find(|record| gone(&record.path))
    }

    /// Every record with its volume, in volume and path order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Record)> {
        self.records.iter().map(|((volume, _), record)| (&**volume, record))
    }

    /// The tag `from` is called `to` in every record now (the list's Rename…, spec 12 §7.3);
    /// a record that has both keeps one. How many records changed.
    pub fn rename_tag(&mut self, from: &str, to: &str) -> usize {
        if self.read_only {
            return 0;
        }
        let mut changed = 0;
        for record in self.records.values_mut() {
            let tags = &mut record.meta.tags;
            let Some(at) = tags.iter().position(|t| same_name(&t.name, from)) else { continue };
            if tags.iter().any(|t| same_name(&t.name, to)) {
                tags.remove(at);
            } else {
                tags[at].name = to.into();
            }
            changed += 1;
        }
        changed
    }

    /// What [`refind_in`] needs of the records on `volume`: copied out, so the search for them
    /// runs without the store's lock.
    pub fn traces(&self, volume: &str) -> Vec<Trace> {
        let from = (Box::<str>::from(volume), Box::<str>::from(""));
        self.records
            .range(from..)
            .take_while(|((v, _), _)| &**v == volume)
            .map(|(_, r)| Trace { path: r.path.clone(), size: r.size, mtime_ns: r.mtime_ns, file_id: r.file_id })
            .collect()
    }
}

/// A record's path, size, time and id ([`Store::traces`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    pub path: Box<str>,
    pub size: u64,
    pub mtime_ns: i64,
    pub file_id: Option<u64>,
}

/// [`Store::refind`] for a whole folder listed at once (spec 12 §4.4): which of `files` (name,
/// size, modification time) in `folder` are records of `traces` (one volume's) whose path is
/// gone. Only files without a record of their own count. Files are indexed once by size and
/// name, so each record costs one lookup; a record that has a file id also looks at the files
/// of its size within 2 s (sorted by time; at most 64 of them), asking `id_of(i)` once per file at most. At most 64
/// paths are asked about (`gone`). Each found record's path, with the file it is.
pub fn refind_in(
    traces: &[Trace],
    folder: &str,
    files: &[(&str, u64, i64)],
    mut id_of: impl FnMut(usize) -> Option<u64>,
    mut gone: impl FnMut(&str) -> bool,
) -> Vec<(Box<str>, usize)> {
    use std::collections::{HashMap, HashSet};
    const TOLERANCE_NS: i64 = 2_000_000_000;
    let own: HashSet<Box<str>> = traces.iter().map(|t| key_path(&t.path)).collect();
    let mut by_name: HashMap<(u64, Box<str>), Vec<usize>> = HashMap::new();
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, (name, size, _)) in files.iter().enumerate() {
        if !own.contains(&key_path(&join(folder, name))) {
            by_name.entry((*size, key_path(name))).or_default().push(i);
            by_size.entry(*size).or_default().push(i);
        }
    }
    let mut found: Vec<(Box<str>, usize)> = Vec::new();
    if by_name.is_empty() {
        return found;
    }
    for list in by_size.values_mut() {
        list.sort_unstable_by_key(|&i| files[i].2);
    }
    let mut ids: HashMap<usize, Option<u64>> = HashMap::new();
    let mut taken: HashSet<usize> = HashSet::new();
    let mut asked = 0;
    for t in traces {
        let near = |i: &usize| (t.mtime_ns - files[*i].2).abs() <= TOLERANCE_NS;
        let base = key_path(t.path.rsplit('/').next().unwrap_or(&t.path));
        let mut hit =
            by_name.get(&(t.size, base)).and_then(|list| list.iter().copied().find(|i| near(i) && !taken.contains(i)));
        if hit.is_none()
            && let (Some(id), Some(list)) = (t.file_id, by_size.get(&t.size))
        {
            let from = list.partition_point(|&i| files[i].2 < t.mtime_ns.saturating_sub(TOLERANCE_NS));
            let to = list.partition_point(|&i| files[i].2 <= t.mtime_ns.saturating_add(TOLERANCE_NS));
            // shortcut: more than 64 files of one size and time (empty files made at once) are
            // not told apart by id; a renamed one among them is not found.
            if to - from <= 64 {
                hit = list[from..to]
                    .iter()
                    .copied()
                    .filter(|i| !taken.contains(i))
                    .find(|&i| *ids.entry(i).or_insert_with(|| id_of(i)) == Some(id));
            }
        }
        let Some(i) = hit else { continue };
        if asked == 64 {
            break;
        }
        asked += 1;
        if gone(&t.path) {
            taken.insert(i);
            found.push((t.path.clone(), i));
        }
    }
    found
}

/// `name` in `folder` (both in a volume, with `/`).
pub fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_owned() } else { format!("{folder}/{name}") }
}

fn parse_line(line: &str) -> Option<(Box<str>, Record)> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [volume, path, size, mtime, id, meta] = fields[..] else { return None };
    if line.starts_with('#') {
        return None;
    }
    let file_id = if id == "-" { None } else { Some(id.parse().ok()?) };
    let meta_text = unescape(meta);
    let mut unknown = Vec::new();
    let meta = meta_from_lines(meta_text.split('\x1f'), &mut unknown);
    let record = Record {
        path: unescape(path).into(),
        size: size.parse().ok()?,
        mtime_ns: mtime.parse().ok()?,
        file_id,
        meta,
        unknown,
    };
    Some((unescape(volume).into(), record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{Tag, TagColor};

    fn record(path: &str, tag: &str) -> Record {
        Record {
            path: path.into(),
            size: 10,
            mtime_ns: 1_000_000_000_000,
            file_id: None,
            meta: Meta { tags: vec![Tag::new(tag, TagColor::Red)], ..Meta::default() },
            unknown: Vec::new(),
        }
    }

    #[test]
    fn lines_round_trip_with_escapes_and_keep_what_they_do_not_know() {
        let mut store = Store::default();
        let mut odd = record("dir/a\tb\\c\nd.txt", "Red");
        odd.meta.comment = r"back\slash".into();
        odd.meta.rating = 3;
        odd.file_id = Some(42);
        odd.unknown = vec!["future 1".into()];
        store.set("1a2b3c4d", odd.clone()).unwrap();
        store.set(r"\\server\share", record("x.txt", "Blue")).unwrap();
        let text = store.to_text();
        assert!(text.starts_with("# gezik tags store 1\n"));
        assert_eq!(text.lines().count(), 3, "one line a record: {text}");
        let mut with_junk = text.clone();
        with_junk.push_str("not a record\n");
        let back = Store::parse(&with_junk);
        assert_eq!(back.get("1a2b3c4d", "dir/a\tb\\c\nd.txt"), Some(&odd));
        assert_eq!(back.len(), 2);
        assert!(back.to_text().ends_with("not a record\n"), "a line it cannot read is kept");
        assert!(!back.read_only);
    }

    #[test]
    fn a_tag_is_renamed_in_every_record() {
        let mut store = Store::default();
        store.set("v", record("a.txt", "Red")).unwrap();
        let mut both = record("b.txt", "red");
        both.meta.tags.push(Tag::new("Crimson", TagColor::Red));
        store.set("v", both).unwrap();
        store.set("w", record("c.txt", "Blue")).unwrap();
        assert_eq!(store.rename_tag("RED", "Crimson"), 2);
        let names = |path: &str, volume: &str| -> Vec<String> {
            store.get(volume, path).unwrap().meta.tags.iter().map(|t| t.name.to_string()).collect()
        };
        assert_eq!(names("a.txt", "v"), ["Crimson"], "the colour stays");
        assert_eq!(names("b.txt", "v"), ["Crimson"], "one of two");
        assert_eq!(names("c.txt", "w"), ["Blue"]);
        assert_eq!(
            store.iter().map(|(v, r)| (v, &*r.path)).collect::<Vec<_>>(),
            [("v", "a.txt"), ("v", "b.txt"), ("w", "c.txt")]
        );
    }

    #[test]
    fn a_later_version_is_read_only() {
        let mut store = Store::parse("# gezik tags store 2\nv\ta.txt\t1\t2\t-\ttag 6 Red\n");
        assert!(store.read_only);
        assert_eq!(store.get("v", "a.txt").unwrap().meta.tags[0].name.as_ref(), "Red");
        assert_eq!(store.set("v", record("b", "x")), Err(LATER));
        assert_eq!(store.remove_tree("v", "a.txt"), 0);
    }

    #[test]
    fn an_empty_record_goes() {
        let mut store = Store::default();
        store.set("v", record("a", "Red")).unwrap();
        store.set("v", Record { path: "a".into(), ..Record::default() }).unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn renames_move_a_folder_and_what_is_inside_only() {
        let mut store = Store::default();
        for path in ["p", "p/a.txt", "p/sub/b.txt", "pq/c.txt", "o.txt"] {
            store.set("v", record(path, "Red")).unwrap();
        }
        assert_eq!(store.rename("v", "p", "v", "new/p2"), 3);
        assert!(store.get("v", "new/p2/sub/b.txt").is_some());
        assert!(store.get("v", "new/p2").is_some());
        assert!(store.get("v", "pq/c.txt").is_some(), "a sibling sharing the prefix stays");
        assert!(store.get("v", "p/a.txt").is_none());
        assert_eq!(store.rename("v", "o.txt", "w", "o.txt"), 1, "to another volume");
        assert!(store.get("w", "o.txt").is_some());
        assert_eq!(store.remove_tree("v", "new"), 3);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn swapped_and_chained_renames_keep_each_files_tags() {
        let mut store = Store::default();
        store.set("v", record("a", "A")).unwrap();
        store.set("v", record("b", "B")).unwrap();
        store.set("v", record("1", "One")).unwrap();
        store.set("v", record("2", "Two")).unwrap();
        // a <-> b through a temporary name, and 2 -> 3, 1 -> 2, reported in the order done.
        let moves = [("v", "a", "v", "t"), ("v", "b", "v", "a"), ("v", "t", "v", "b")];
        for (vol, from, to_vol, to) in moves {
            store.rename(vol, from, to_vol, to);
        }
        assert_eq!(&*store.get("v", "a").unwrap().meta.tags[0].name, "B");
        assert_eq!(&*store.get("v", "b").unwrap().meta.tags[0].name, "A");
        assert_eq!(store.rename_all(&[("v", "1", "v", "2"), ("v", "2", "v", "3")]), 2);
        assert_eq!(&*store.get("v", "2").unwrap().meta.tags[0].name, "One");
        assert_eq!(&*store.get("v", "3").unwrap().meta.tags[0].name, "Two");
        assert_eq!(store.rename_all(&[("v", "a", "v", "b"), ("v", "b", "v", "a")]), 2, "a direct swap");
        assert_eq!(&*store.get("v", "a").unwrap().meta.tags[0].name, "A");
    }

    #[test]
    fn removing_a_folders_own_entry_keeps_what_is_inside() {
        let mut store = Store::default();
        store.set("v", record("d", "Red")).unwrap();
        store.set("v", record("d/x", "Red")).unwrap();
        assert!(store.remove("v", "d"));
        assert!(store.get("v", "d/x").is_some());
        assert!(!store.remove("v", "d"));
    }

    #[test]
    fn paths_compare_as_the_system_does() {
        let mut store = Store::default();
        store.set("v", record("Dir/A.txt", "Red")).unwrap();
        assert_eq!(store.get("v", "dir/a.txt").is_some(), IGNORE_CASE);
        assert_eq!(&*store.get("v", "Dir/A.txt").unwrap().path, "Dir/A.txt", "written as given");
    }

    #[test]
    fn a_reused_path_does_not_fit() {
        let mut r = record("a", "Red");
        r.file_id = Some(7);
        assert!(r.fits(10, 5, None), "same size: the file was edited in place");
        assert!(r.fits(11, 1_000_000_000_000, None));
        assert!(r.fits(11, 5, Some(7)));
        assert!(!r.fits(11, 5, Some(8)), "all three differ: another file");
        assert!(!r.fits(10, 1_000_000_000_000, Some(8)), "known ids that differ decide: a file made anew");
        assert!(!r.fits(11, 5, None));
    }

    #[test]
    fn a_file_moved_elsewhere_is_found_again() {
        let mut store = Store::default();
        store.set("v", record("old/a.txt", "Red")).unwrap();
        store.set("v", record("here/a.txt", "Blue")).unwrap();
        let mut by_id = record("old/b.bin", "Green");
        by_id.file_id = Some(99);
        store.set("v", by_id).unwrap();
        let gone = |path: &str| path.starts_with("old/");
        let found = store.refind("v", "A.TXT", 10, 1_000_000_000_000 + 1_500_000_000, None, gone);
        if IGNORE_CASE {
            assert_eq!(&*found.unwrap().path, "old/a.txt", "within FAT's 2 s; the one still there is not taken");
        }
        let found = store.refind("v", "a.txt", 10, 1_000_000_000_000 + 1_500_000_000, None, gone);
        assert_eq!(&*found.unwrap().path, "old/a.txt");
        assert!(store.refind("v", "a.txt", 10, 1_000_000_000_000 + 3_000_000_000, None, gone).is_none());
        assert!(store.refind("v", "a.txt", 11, 1_000_000_000_000, None, gone).is_none());
        assert!(store.refind("w", "a.txt", 10, 1_000_000_000_000, None, gone).is_none(), "only on its volume");
        assert!(store.refind("v", "renamed.bin", 1, 2, Some(99), gone).is_none(), "an id alone is not enough");
        let renamed = store.refind("v", "renamed.bin", 10, 1_000_000_000_000, Some(99), gone);
        assert_eq!(&*renamed.unwrap().path, "old/b.bin");
    }

    #[test]
    fn a_folder_listed_whole_finds_files_moved_into_it() {
        let mut store = Store::default();
        store.set("v", record("old/a.txt", "Red")).unwrap();
        store.set("v", record("still/b.txt", "Blue")).unwrap();
        let mut by_id = record("old/c.bin", "Green");
        by_id.file_id = Some(99);
        store.set("v", by_id).unwrap();
        store.set("v", record("new/own.txt", "Gray")).unwrap();
        let t = 1_000_000_000_000;
        let files = [("a.txt", 10, t + 1_500_000_000), ("b.txt", 10, t), ("renamed.bin", 10, t), ("own.txt", 10, t)];
        let ids = [None, None, Some(99), None];
        let gone = |path: &str| path.starts_with("old/");
        let found = refind_in(&store.traces("v"), "new", &files, |i| ids[i], gone);
        assert_eq!(found, [(Box::from("old/a.txt"), 0), (Box::from("old/c.bin"), 2)], "b.txt is still in its place");
        assert!(refind_in(&store.traces("w"), "new", &files, |i| ids[i], gone).is_empty(), "only on its volume");
        let late = [("a.txt", 10, t + 3_000_000_000)];
        assert!(refind_in(&store.traces("v"), "new", &late, |_| None, gone).is_empty(), "past FAT's 2 s");
        let mut asked = 0;
        for i in 0..100 {
            store.set("v", record(&format!("x/{i}/a.txt"), "Red")).unwrap();
        }
        refind_in(
            &store.traces("v"),
            "new",
            &[("a.txt", 10, t)],
            |_| None,
            |_| {
                asked += 1;
                false
            },
        );
        assert_eq!(asked, 64, "at most 64 paths are asked about");
    }

    #[test]
    fn a_big_folder_against_many_records_is_one_pass() {
        let mut store = Store::default();
        for i in 0..20_000 {
            let mut r = record(&format!("old/{i}.txt"), "Red");
            r.size = 0;
            r.file_id = (i % 2 == 0).then_some(i);
            store.set("v", r).unwrap();
        }
        let names: Vec<String> = (0..100_000).map(|i| format!("new{i}.txt")).collect();
        let files: Vec<(&str, u64, i64)> = names.iter().map(|n| (n.as_str(), 0, 1_000_000_000_000)).collect();
        let (mut ids, mut asked) = (0, 0);
        let start = std::time::Instant::now();
        let found = refind_in(
            &store.traces("v"),
            "new",
            &files,
            |_| {
                ids += 1;
                None
            },
            |_| {
                asked += 1;
                false
            },
        );
        let took = start.elapsed();
        assert!(found.is_empty() && asked == 0);
        assert!(ids <= files.len(), "each file's id asked once at most: {ids}");
        assert!(took.as_secs() < 5, "took {took:?}");
    }

    #[test]
    fn a_full_store_takes_no_new_file() {
        let mut store = Store::default();
        for i in 0..MAX_RECORDS {
            store.records.insert(("v".into(), i.to_string().into()), record(&i.to_string(), "x"));
        }
        assert_eq!(store.set("v", record("new", "Red")), Err(FULL));
        assert_eq!(store.set("v", record("5", "Blue")), Ok(()), "one already there is updated");
        store.set("v", Record { path: "6".into(), ..Record::default() }).unwrap();
        assert_eq!(store.set("v", record("new", "Red")), Ok(()), "room again");
    }
}
