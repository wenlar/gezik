//! Sorting: natural name order (`file2` before `file10`, letters in Turkish alphabet
//! order) and the column sort keys. Every OS sorts the same, so a synced folder looks the
//! same everywhere.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::time::SystemTime;

use crate::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortKey {
    Name,
    Modified,
    Created,
    Type,
    Size,
    /// The search results' own sort (a result's folder), not a folder's.
    Folder,
}

impl SortKey {
    /// A folder's sort keys; `Folder` is the search results' own.
    pub const ALL: [SortKey; 5] = [SortKey::Name, SortKey::Modified, SortKey::Created, SortKey::Type, SortKey::Size];

    pub fn as_str(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Modified => "modified",
            SortKey::Created => "created",
            SortKey::Type => "type",
            SortKey::Size => "size",
            SortKey::Folder => "folder",
        }
    }

    /// `ALL` and `Folder` (the search results' own sort, spec 4.5).
    pub fn parse(text: &str) -> Option<SortKey> {
        SortKey::ALL.into_iter().chain([SortKey::Folder]).find(|k| k.as_str() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

impl SortDir {
    pub fn as_str(self) -> &'static str {
        match self {
            SortDir::Asc => "asc",
            SortDir::Desc => "desc",
        }
    }

    pub fn parse(text: &str) -> Option<SortDir> {
        match text {
            "asc" => Some(SortDir::Asc),
            "desc" => Some(SortDir::Desc),
            _ => None,
        }
    }

    pub fn flipped(self) -> SortDir {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SortSpec {
    pub key: SortKey,
    pub dir: SortDir,
}

impl Default for SortSpec {
    fn default() -> Self {
        SortSpec { key: SortKey::Name, dir: SortDir::Asc }
    }
}

// Key tokens. A name becomes a sequence of `u32` tokens, made as they are compared (no
// key is stored). Parts follow each other; a text part is its letter weights plus
// `END`, a number part is `NUMBER`, its digit count, its digits and its leading-zero count.
// The tokens are chosen so that comparing keys token by token gives natural order:
// - `END` is lowest, so a shorter text sorts first (`a` < `a!`) and `a1` < `ab`;
// - `NUMBER` is below every letter weight, so where one name has a number and the other a
//   character, the number sorts first (`1` < `a`, `1` < `_`);
// - digits are written without leading zeros after their count, so a longer number is
//   larger, equal lengths compare digit by digit, and the zero count then puts `7` first
//   before `007`.
const END: u32 = 0;
const NUMBER: u32 = 1;
const TEXT_BASE: u32 = 2;

/// A name's key tokens one at a time, so a sort can compare names without storing keys.
fn tokens(name: &str) -> Tokens<'_> {
    Tokens { name, i: 0, part: Part::Between }
}

struct Tokens<'a> {
    name: &'a str,
    // Where the next part starts (or the text part goes on).
    i: usize,
    part: Part,
}

enum Part {
    Between,
    Text,
    // A digit run: its digits from `next` up to `end` (no leading zeros), then `zeros`;
    // `count` until the digit count is out.
    Number { count: bool, next: usize, end: usize, zeros: usize },
}

impl Iterator for Tokens<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        let bytes = self.name.as_bytes();
        match self.part {
            Part::Between if self.i >= bytes.len() => None,
            Part::Between if bytes[self.i].is_ascii_digit() => {
                let start = self.i;
                while self.i < bytes.len() && bytes[self.i].is_ascii_digit() {
                    self.i += 1;
                }
                let run = &bytes[start..self.i];
                let zeros = run.iter().position(|&b| b != b'0').unwrap_or(run.len() - 1);
                self.part = Part::Number { count: true, next: start + zeros, end: self.i, zeros };
                Some(NUMBER)
            }
            Part::Between | Part::Text => {
                // Digits are ASCII, so they never occur inside a multi-byte character and
                // `i` stays on a character boundary.
                match self.name[self.i..].chars().next().filter(|c| !c.is_ascii_digit()) {
                    Some(c) => {
                        self.part = Part::Text;
                        self.i += c.len_utf8();
                        Some(weight(c) + TEXT_BASE)
                    }
                    None => {
                        self.part = Part::Between;
                        Some(END)
                    }
                }
            }
            Part::Number { ref mut count, ref mut next, end, zeros } => {
                if *count {
                    *count = false;
                    Some(u32::try_from(end - *next).unwrap_or(u32::MAX))
                } else if *next < end {
                    *next += 1;
                    Some(u32::from(bytes[*next - 1] - b'0'))
                } else {
                    self.part = Part::Between;
                    Some(u32::try_from(zeros).unwrap_or(u32::MAX))
                }
            }
        }
    }
}

/// Compares two names' keys token by token (natural order, case ignored).
fn key_cmp(a: &str, b: &str) -> Ordering {
    // The bytes both names start with give the same tokens, unless they end inside a
    // character or a digit run (`file12` and `file13`): start at the last place both agree.
    let mut i = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    while !a.is_char_boundary(i) {
        i -= 1;
    }
    while i > 0 && a.as_bytes()[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let (x, y) = (a.as_bytes(), b.as_bytes());
    let digit = |s: &[u8]| s.get(i).is_some_and(u8::is_ascii_digit);
    if digit(x) && digit(y) {
        // Two numbers here (the usual place names differ): their tokens compared without
        // making them, then the rest.
        fn run(s: &[u8], i: usize) -> (&[u8], usize, usize) {
            let end = i + s[i..].iter().take_while(|b| b.is_ascii_digit()).count();
            let zeros = s[i..end].iter().position(|&b| b != b'0').unwrap_or(end - i - 1);
            (&s[i + zeros..end], zeros, end)
        }
        let ((da, za, ea), (db, zb, eb)) = (run(x, i), run(y, i));
        let rest = |name, i| Tokens { name, i, part: Part::Between };
        return da.len().cmp(&db.len()).then(da.cmp(db)).then(za.cmp(&zb)).then_with(|| rest(a, ea).cmp(rest(b, eb)));
    }
    // After a character (not a digit) both are inside a text part.
    let from = |name| Tokens { name, i, part: if i == 0 { Part::Between } else { Part::Text } };
    from(a).cmp(from(b))
}

/// Natural order, ignoring case; names equal that way are ordered by their exact text, so
/// the order is total and the same on every run.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    key_cmp(a, b).then_with(|| a.cmp(b))
}

/// Letters of the Turkish alphabet (and the English ones it lacks) come after all
/// non-letters; other scripts after them.
const LETTER: u32 = 0x20_0000;
const OTHER_LETTER: u32 = 0x30_0000;

fn weight(c: char) -> u32 {
    let lower = turkish_lowercase(c);
    if let Some((base, accent)) = alphabet_position(lower) {
        return LETTER + base * 8 + accent;
    }
    if lower.is_alphabetic() { OTHER_LETTER + lower as u32 } else { lower as u32 }
}

/// `I` is the capital of `ı`, and `İ` of `i`, as in Turkish.
fn turkish_lowercase(c: char) -> char {
    match c {
        'I' => 'ı',
        'İ' => 'i',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

/// Position in `a b c ç d e f g ğ h ı i j k l m n o ö p q r s ş t u ü v w x y z`, and an
/// accent rank so that accented letters other languages use (`é`, `ñ`…) sort right after
/// their base letter.
fn alphabet_position(c: char) -> Option<(u32, u32)> {
    Some(match c {
        'a' => (0, 0),
        'á' => (0, 1),
        'à' => (0, 2),
        'â' => (0, 3),
        'ä' => (0, 4),
        'ã' => (0, 5),
        'å' => (0, 6),
        'æ' => (0, 7),
        'b' => (1, 0),
        'c' => (2, 0),
        'ć' => (2, 1),
        'č' => (2, 2),
        'ç' => (3, 0),
        'd' => (4, 0),
        'e' => (5, 0),
        'é' => (5, 1),
        'è' => (5, 2),
        'ê' => (5, 3),
        'ë' => (5, 4),
        'f' => (6, 0),
        'g' => (7, 0),
        'ğ' => (8, 0),
        'h' => (9, 0),
        'ı' => (10, 0),
        'i' => (11, 0),
        'í' => (11, 1),
        'ì' => (11, 2),
        'î' => (11, 3),
        'ï' => (11, 4),
        'j' => (12, 0),
        'k' => (13, 0),
        'l' => (14, 0),
        'm' => (15, 0),
        'n' => (16, 0),
        'ñ' => (16, 1),
        'o' => (17, 0),
        'ó' => (17, 1),
        'ò' => (17, 2),
        'ô' => (17, 3),
        'õ' => (17, 4),
        'ø' => (17, 5),
        'œ' => (17, 6),
        'ö' => (18, 0),
        'p' => (19, 0),
        'q' => (20, 0),
        'r' => (21, 0),
        's' => (22, 0),
        'ß' => (22, 1),
        'š' => (22, 2),
        'ş' => (23, 0),
        't' => (24, 0),
        'u' => (25, 0),
        'ú' => (25, 1),
        'ù' => (25, 2),
        'û' => (25, 3),
        'ü' => (26, 0),
        'v' => (27, 0),
        'w' => (28, 0),
        'x' => (29, 0),
        'y' => (30, 0),
        'ý' => (30, 1),
        'ÿ' => (30, 2),
        'z' => (31, 0),
        'ž' => (31, 1),
        _ => return None,
    })
}

/// The column value an entry sorts by, before its name (not kept for Name and Folder: those
/// compare the row's own text when the sort asks).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Primary {
    Time(Option<SystemTime>),
    Size(u64),
    // Type name key, then the lowercase extension (spans of the key buffer: no string a row).
    Type((u32, u32), (u32, u32)),
}

/// Sorts `entries` by `spec` (see [`sort_order`]). Returns where each entry came from: entry
/// `k` now was entry `order[k]` before.
pub fn sort_entries(
    entries: &mut [Entry],
    spec: SortSpec,
    folders_first: bool,
    type_name: impl Fn(&Entry) -> String,
) -> Vec<usize> {
    let order = sort_order(entries, spec, folders_first, type_name, &|_| "");
    apply_order(entries, &order);
    order
}

/// The order `entries` sort in by `spec`, without moving them: folders first (in either
/// direction) unless `folders_first` is off, then the column, then natural name order, then
/// the exact name so the order is total. `type_name` gives the Type column's text (called once
/// per entry, only when sorting by type); `folder` the folder of entry `i` (only by Folder).
pub fn sort_order<'a>(
    entries: &[Entry],
    spec: SortSpec,
    folders_first: bool,
    type_name: impl Fn(&Entry) -> String,
    folder: &dyn Fn(usize) -> &'a str,
) -> Vec<usize> {
    sort_rows(
        entries.len(),
        &|i| Cow::Borrowed(&entries[i]),
        &|i| &entries[i].name,
        spec,
        folders_first,
        type_name,
        folder,
    )
}

/// [`sort_order`] for `len` rows kept some other way (the search results' compact rows):
/// `entry` gives row `i` (called once per row), `name` its name (whenever two rows compare).
pub fn sort_rows<'e, 'a>(
    len: usize,
    entry: &dyn Fn(usize) -> Cow<'e, Entry>,
    name: &dyn Fn(usize) -> &'e str,
    spec: SortSpec,
    folders_first: bool,
    type_name: impl Fn(&Entry) -> String,
    folder: &dyn Fn(usize) -> &'a str,
) -> Vec<usize> {
    // Names and folders are compared token by token from their text, not from stored keys:
    // keys took ~4 B a character and 32 B a row (21 MB at 250,000 results); the compares cost
    // a little time.
    // The Type column's keys live in one buffer (its texts are short and few per row).
    let mut buf: Vec<u32> = Vec::new();
    // `raw`: the code points as they are (they compare as the text's bytes did).
    let mut push = |text: &str, raw: bool| {
        let start = buf.len() as u32;
        if raw {
            buf.extend(text.chars().map(u32::from));
        } else {
            buf.extend(tokens(text));
        }
        (start, buf.len() as u32)
    };
    // Folders whose size is on their way go last in either direction (spec 6.3).
    let last = |e: &Entry| spec.key == SortKey::Size && e.size_pending();
    let keyed = !matches!(spec.key, SortKey::Name | SortKey::Folder);
    // Per row: whether it is a folder and goes last (the comparison needs no entry), and the
    // column's value when it is not the row's text.
    let mut kinds: Vec<(bool, bool)> = Vec::with_capacity(len);
    let mut keys: Vec<Primary> = Vec::with_capacity(if keyed { len } else { 0 });
    for i in 0..len {
        let e = entry(i);
        kinds.push((e.is_dir, last(&e)));
        keys.extend(match spec.key {
            SortKey::Name | SortKey::Folder => None,
            SortKey::Modified => Some(Primary::Time(e.modified)),
            SortKey::Created => Some(Primary::Time(e.created)),
            // A file's size, a folder's worked-out total; a folder without one as 0 (as before).
            SortKey::Size => Some(Primary::Size(e.known_size().unwrap_or(0))),
            SortKey::Type => {
                Some(Primary::Type(push(&type_name(&e), false), push(&e.extension().to_lowercase(), true)))
            }
        });
    }
    let span = |(start, end): (u32, u32)| &buf[start as usize..end as usize];
    // Sorting indices moves 8 bytes per swap instead of a whole `Entry`.
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_unstable_by(|&i, &j| {
        let ((a_dir, a_last), (b_dir, b_last)) = (kinds[i], kinds[j]);
        let folders = if folders_first { b_dir.cmp(&a_dir) } else { Ordering::Equal };
        folders.then_with(|| a_last.cmp(&b_last)).then_with(|| {
            let primary = match spec.key {
                SortKey::Name => Ordering::Equal,
                SortKey::Folder => key_cmp(folder(i), folder(j)),
                _ => match (&keys[i], &keys[j]) {
                    (Primary::Type(ta, xa), Primary::Type(tb, xb)) => {
                        span(*ta).cmp(span(*tb)).then_with(|| span(*xa).cmp(span(*xb)))
                    }
                    (pa, pb) => pa.cmp(pb),
                },
            };
            let order = primary
                .then_with(|| key_cmp(name(i), name(j)))
                .then_with(|| name(i).cmp(name(j)))
                .then_with(|| folder(i).cmp(folder(j)))
                .then_with(|| i.cmp(&j));
            if spec.dir == SortDir::Desc { order.reverse() } else { order }
        })
    });
    order
}

/// Puts `items` in `order` (item `k` becomes the one that was at `order[k]`); `order` holds
/// every index once.
pub fn apply_order<T>(items: &mut [T], order: &[usize]) {
    debug_assert_eq!(order.len(), items.len());
    // Follows each cycle with swaps: a flag per item, not a second copy of the items (250,000
    // results would need 16 MB more for a moment).
    let mut done = vec![false; order.len()];
    for start in 0..order.len() {
        let mut at = start;
        while !done[at] {
            done[at] = true;
            let from = order[at];
            if from == start {
                break;
            }
            items.swap(at, from);
            at = from;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn size_order(entries: &[Entry], dir: SortDir, folders_first: bool) -> Vec<String> {
        let spec = SortSpec { key: SortKey::Size, dir };
        sort_order(entries, spec, folders_first, |_| String::new(), &|_| "")
            .into_iter()
            .map(|i| entries[i].name.clone())
            .collect()
    }

    fn folder(name: &str, size: u64, flags: u8) -> Entry {
        Entry { name: name.into(), is_dir: true, flags, size, modified: None, created: None }
    }

    fn sized_file(name: &str, size: u64) -> Entry {
        Entry { name: name.into(), is_dir: false, flags: 0, size, modified: None, created: None }
    }

    #[test]
    fn folders_without_a_size_yet_sort_last_both_ways() {
        let entries = vec![
            folder("pending", 0, Entry::SIZE_PENDING),
            folder("big", 500, Entry::SIZED),
            folder("small", 5, Entry::SIZED),
            folder("old", 50, Entry::SIZED | Entry::SIZE_STALE),
            sized_file("f", 60),
        ];
        assert_eq!(size_order(&entries, SortDir::Asc, true), ["small", "old", "big", "pending", "f"]);
        assert_eq!(size_order(&entries, SortDir::Desc, true), ["big", "old", "small", "pending", "f"]);
        assert_eq!(size_order(&entries, SortDir::Asc, false), ["small", "old", "f", "big", "pending"]);
        assert_eq!(size_order(&entries, SortDir::Desc, false), ["big", "f", "old", "small", "pending"]);
    }

    #[test]
    fn folders_gezik_does_not_size_sort_as_before() {
        // folder-sizes = "off", results, a folder's own record size (4096 on Unix): as 0.
        let entries = vec![sized_file("f", 1), folder("d", 4096, 0)];
        assert_eq!(size_order(&entries, SortDir::Asc, false), ["d", "f"]);
        assert_eq!(size_order(&entries, SortDir::Desc, false), ["f", "d"]);
    }

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    fn entry(name: &str, is_dir: bool, size: u64, secs: Option<u64>) -> Entry {
        let time = secs.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        Entry { name: name.to_owned(), is_dir, flags: 0, size, modified: time, created: time }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(sorted(&["file10", "file2", "file1", "file20"]), ["file1", "file2", "file10", "file20"]);
        assert_eq!(sorted(&["v1.10", "v1.9", "v1.2"]), ["v1.2", "v1.9", "v1.10"]);
    }

    #[test]
    fn leading_zeros_break_ties_shortest_first() {
        assert_eq!(natural_cmp("7", "007"), Ordering::Less);
        assert_eq!(natural_cmp("007", "8"), Ordering::Less);
        assert_eq!(natural_cmp("0", "00"), Ordering::Less);
    }

    #[test]
    fn turkish_letters_are_in_alphabet_order() {
        let input =
            ["zil", "üzüm", "uçak", "şeker", "sabun", "öykü", "oda", "inek", "ılık", "çay", "cam", "ğol", "gol"];
        let expected =
            ["cam", "çay", "gol", "ğol", "ılık", "inek", "oda", "öykü", "sabun", "şeker", "uçak", "üzüm", "zil"];
        assert_eq!(sorted(&input), expected);
    }

    #[test]
    fn dotted_and_dotless_capitals_follow_turkish_rules() {
        assert_eq!(natural_cmp("ılık", "Irmak"), Ordering::Less, "I is the capital of ı");
        assert_eq!(natural_cmp("Irmak", "inek"), Ordering::Less, "ı comes before i");
        assert_eq!(natural_cmp("ilk", "İnek"), Ordering::Less, "İ is the capital of i");
    }

    #[test]
    fn case_is_ignored_then_exact_text_decides() {
        assert_eq!(natural_cmp("a", "B"), Ordering::Less);
        assert_eq!(natural_cmp("Apple", "apple"), Ordering::Less);
        assert_eq!(natural_cmp("apple", "apple"), Ordering::Equal);
    }

    #[test]
    fn non_letters_and_numbers_come_before_letters() {
        assert_eq!(natural_cmp("_x", "a"), Ordering::Less);
        assert_eq!(natural_cmp("1", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "ab"), Ordering::Less);
        assert_eq!(natural_cmp("a", "a.txt"), Ordering::Less);
    }

    #[test]
    fn accented_letters_follow_their_base_letter() {
        assert_eq!(sorted(&["f", "é", "e"]), ["e", "é", "f"]);
        assert_eq!(sorted(&["o", "ö", "ó"]), ["o", "ó", "ö"]);
    }

    #[test]
    fn other_scripts_come_after_latin_letters() {
        assert_eq!(natural_cmp("zebra", "яблоко"), Ordering::Less);
    }

    #[test]
    fn sorts_by_name_with_folders_first_in_both_directions() {
        let mut v = vec![entry("b.txt", false, 0, None), entry("Zeta", true, 0, None), entry("a10", false, 0, None)];
        v.push(entry("a9", false, 0, None));
        v.push(entry("alpha", true, 0, None));
        let order = sort_entries(&mut v, SortSpec::default(), true, |_| String::new());
        assert_eq!(names(&v), ["alpha", "Zeta", "a9", "a10", "b.txt"]);
        assert_eq!(order, [4, 1, 3, 2, 0], "where each entry was before");
        sort_entries(&mut v, SortSpec { key: SortKey::Name, dir: SortDir::Desc }, true, |_| String::new());
        assert_eq!(names(&v), ["Zeta", "alpha", "b.txt", "a10", "a9"]);
    }

    #[test]
    fn sorts_by_size_and_dates() {
        let mut v =
            vec![entry("big", false, 900, Some(1)), entry("small", false, 5, Some(3)), entry("none", false, 50, None)];
        sort_entries(&mut v, SortSpec { key: SortKey::Size, dir: SortDir::Asc }, true, |_| String::new());
        assert_eq!(names(&v), ["small", "none", "big"]);
        sort_entries(&mut v, SortSpec { key: SortKey::Modified, dir: SortDir::Asc }, true, |_| String::new());
        assert_eq!(names(&v), ["none", "big", "small"], "unknown times first");
        sort_entries(&mut v, SortSpec { key: SortKey::Created, dir: SortDir::Desc }, true, |_| String::new());
        assert_eq!(names(&v), ["small", "big", "none"]);
    }

    #[test]
    fn sorts_by_type_name_then_extension_then_name() {
        let mut v =
            vec![entry("b.png", false, 0, None), entry("a.txt", false, 0, None), entry("c.PNG", false, 0, None)];
        v.push(entry("a.jpg", false, 0, None));
        let type_name = |e: &Entry| match e.extension().to_lowercase().as_str() {
            "png" | "jpg" => "Picture".to_owned(),
            _ => "Text".to_owned(),
        };
        sort_entries(&mut v, SortSpec { key: SortKey::Type, dir: SortDir::Asc }, true, type_name);
        assert_eq!(names(&v), ["a.jpg", "b.png", "c.PNG", "a.txt"]);
    }

    #[test]
    fn equal_keys_keep_a_total_order() {
        let mut v = vec![entry("same", false, 1, None), entry("Same", false, 1, None)];
        sort_entries(&mut v, SortSpec { key: SortKey::Size, dir: SortDir::Asc }, true, |_| String::new());
        assert_eq!(names(&v), ["Same", "same"]);
    }

    #[test]
    fn long_numbers_compare_by_value() {
        let n = |digits: usize, first: char| format!("{first}{}", "0".repeat(digits - 1));
        // Lengths around where a 128-bit integer would stop fitting.
        for digits in [37, 38, 39, 40, 45] {
            let smaller = format!("f{}", "9".repeat(digits - 1));
            let larger = format!("f{}", n(digits + 1, '1'));
            assert_eq!(natural_cmp(&smaller, &larger), Ordering::Less, "{digits} digits");
            assert_eq!(natural_cmp(&larger, &smaller), Ordering::Greater, "{digits} digits");
        }
        let max = u128::MAX.to_string();
        let above = "340282366920938463463374607431768211456"; // u128::MAX + 1
        let below = "340282366920938463463374607431768211454";
        assert_eq!(natural_cmp(&format!("x{below}"), &format!("x{max}")), Ordering::Less);
        assert_eq!(natural_cmp(&format!("x{max}"), &format!("x{above}")), Ordering::Less);
        assert_eq!(natural_cmp(&format!("x{above}"), &format!("x{max}")), Ordering::Greater);
        // A long run against a short one, then the text after it.
        let long = "1".repeat(45);
        assert_eq!(natural_cmp(&format!("x{}", "9".repeat(10)), &format!("x{long}")), Ordering::Less);
        assert_eq!(natural_cmp(&format!("x{long}a"), &format!("x{long}b")), Ordering::Less);
        // Same leading digits, different length.
        assert_eq!(natural_cmp(&"1".repeat(39), &"1".repeat(40)), Ordering::Less);
        assert_eq!(natural_cmp(&"1".repeat(38), &"1".repeat(39)), Ordering::Less);
        // Same value, same length differing only in the last digit.
        assert_eq!(natural_cmp(&format!("{}1", "1".repeat(44)), &format!("{}2", "1".repeat(44))), Ordering::Less);
    }

    #[test]
    fn leading_zeros_break_ties_for_long_numbers() {
        let long = "7".repeat(45);
        assert_eq!(natural_cmp(&long, &format!("0{long}")), Ordering::Less);
        assert_eq!(natural_cmp(&format!("0{long}"), &format!("00{long}")), Ordering::Less);
        // The value still beats the zeros.
        assert_eq!(natural_cmp(&format!("00{long}"), &format!("{long}1")), Ordering::Less);
        assert_eq!(natural_cmp(&format!("{long}0"), &format!("0{long}")), Ordering::Greater);
        let max = u128::MAX.to_string();
        assert_eq!(natural_cmp(&max, &format!("0{max}")), Ordering::Less);
    }

    #[test]
    fn a_number_sorts_before_any_character_at_the_same_place() {
        assert_eq!(natural_cmp("1", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a!"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a_"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a"), Ordering::Greater, "a prefix is shorter");
        assert_eq!(natural_cmp("1", "!"), Ordering::Less);
    }

    #[test]
    fn descending_type_reverses_ties_by_name() {
        let mut v =
            vec![entry("a.txt", false, 0, None), entry("b.txt", false, 0, None), entry("c.png", false, 0, None)];
        let type_name = |e: &Entry| if e.extension() == "png" { "Picture".to_owned() } else { "Text".to_owned() };
        sort_entries(&mut v, SortSpec { key: SortKey::Type, dir: SortDir::Desc }, true, type_name);
        assert_eq!(names(&v), ["b.txt", "a.txt", "c.png"]);
    }
    #[test]
    fn sort_names_round_trip() {
        for key in SortKey::ALL {
            assert_eq!(SortKey::parse(key.as_str()), Some(key));
        }
        assert_eq!(SortDir::parse("desc"), Some(SortDir::Desc));
        assert_eq!(SortDir::parse("down"), None);
        assert_eq!(SortDir::Asc.flipped(), SortDir::Desc);
    }

    #[test]
    fn folders_mix_with_files_when_not_first() {
        let mut v = vec![
            entry("b.txt", false, 3, None),
            entry("Zeta", true, 0, None),
            entry("alpha", true, 0, None),
            entry("a9", false, 7, None),
        ];
        sort_entries(&mut v, SortSpec::default(), false, |_| String::new());
        assert_eq!(names(&v), ["a9", "alpha", "b.txt", "Zeta"]);
        sort_entries(&mut v, SortSpec { key: SortKey::Size, dir: SortDir::Desc }, false, |_| String::new());
        assert_eq!(names(&v), ["a9", "b.txt", "Zeta", "alpha"], "a folder's size counts as 0");
        sort_entries(&mut v, SortSpec::default(), true, |_| String::new());
        assert_eq!(names(&v), ["alpha", "Zeta", "a9", "b.txt"], "folders first again");
    }

    /// `cargo test -p gezik-core --release -- --ignored sorting_100k`
    #[test]
    #[ignore]
    fn sorting_100k_names_is_fast() {
        let mut v: Vec<Entry> = (0..100_000)
            .map(|i| entry(&format!("IMG_{:05} kopya şğı {}.jpg", (i * 7919) % 100_000, i), false, i, None))
            .collect();
        let start = std::time::Instant::now();
        sort_entries(&mut v, SortSpec::default(), true, |_| String::new());
        let took = start.elapsed();
        eprintln!("sorted 100k in {took:?}");
        assert!(took.as_millis() <= 50, "took {took:?}");
    }

    #[test]
    fn folders_sort_naturally_then_by_name() {
        let entries = vec![
            entry("b.txt", false, 0, None),
            entry("a.txt", false, 0, None),
            entry("c.txt", false, 0, None),
            entry("b.txt", false, 0, None),
        ];
        let folders = ["sub10", "sub2", "", "sub2"];
        let spec = SortSpec { key: SortKey::Folder, dir: SortDir::Asc };
        let order = sort_order(&entries, spec, true, |_| String::new(), &|i| folders[i]);
        assert_eq!(order, [2, 1, 3, 0], "root first, sub2 (a, b) before sub10");
        let desc =
            sort_order(&entries, SortSpec { dir: SortDir::Desc, ..spec }, true, |_| String::new(), &|i| folders[i]);
        assert_eq!(desc, [0, 3, 1, 2]);
        let mut names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        apply_order(&mut names, &order);
        assert_eq!(names, ["c.txt", "a.txt", "b.txt", "b.txt"]);
        assert_eq!(SortKey::parse("folder"), Some(SortKey::Folder));
        let same = vec![entry("a.txt", false, 0, None), entry("a.txt", false, 0, None), entry("a.txt", false, 0, None)];
        let dirs = ["sub2", "sub1", "sub3"];
        let by_name =
            |dir| sort_order(&same, SortSpec { key: SortKey::Name, dir }, true, |_| String::new(), &|i| dirs[i]);
        assert_eq!(by_name(SortDir::Asc), [1, 0, 2], "equal names: by folder");
        assert_eq!(by_name(SortDir::Desc), [2, 0, 1]);
        assert!(!SortKey::ALL.contains(&SortKey::Folder), "not a folder's sort");
    }

    /// The key builder sorts used before tokens were made lazily (the reference they match).
    fn stored_key(name: &str) -> Vec<u32> {
        let (bytes, mut key, mut i) = (name.as_bytes(), Vec::new(), 0);
        while i < bytes.len() {
            if bytes[i].is_ascii_digit() {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let run = &bytes[start..i];
                let first = run.iter().position(|&b| b != b'0').unwrap_or(run.len() - 1);
                key.push(NUMBER);
                key.push(u32::try_from(run.len() - first).unwrap());
                key.extend(run[first..].iter().map(|&b| u32::from(b - b'0')));
                key.push(u32::try_from(first).unwrap());
            } else {
                for c in name[i..].chars().take_while(|c| !c.is_ascii_digit()) {
                    key.push(weight(c) + TEXT_BASE);
                    i += c.len_utf8();
                }
                key.push(END);
            }
        }
        key
    }

    const MIXED: [&str; 37] = [
        "",
        "a9",
        "a10",
        "x1.5",
        "x1.50",
        "çx",
        "öx",
        "0",
        "00",
        "007",
        "7",
        "a",
        "A",
        "a1",
        "a01",
        "a!",
        "ab",
        "file10",
        "File2",
        "file2.txt",
        "ılık",
        "Irmak",
        "İnek",
        "inek",
        "şeker",
        "Şeker",
        "çay10b",
        "Çay9",
        "x340282366920938463463374607431768211456y",
        "яблоко",
        "çay10a",
        "çay1",
        "file10.txt",
        "file_12.dat",
        "file_13.dat",
        "file_123.dat",
        "ılık2",
    ];

    #[test]
    fn lazy_tokens_match_the_stored_keys() {
        for name in MIXED {
            assert_eq!(tokens(name).collect::<Vec<_>>(), stored_key(name), "{name:?}");
        }
        for a in MIXED {
            for b in MIXED {
                assert_eq!(key_cmp(a, b), stored_key(a).cmp(&stored_key(b)), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn rows_sort_by_lazy_names_and_folders() {
        let entries: Vec<Entry> = MIXED.iter().map(|n| entry(n, false, 0, None)).collect();
        let folders: Vec<&str> = MIXED.iter().rev().copied().collect();
        // The old keys' order: the column (none for Name), the name's key, then the exact texts.
        let want = |primary: &dyn Fn(usize) -> Vec<u32>| {
            let mut v: Vec<usize> = (0..MIXED.len()).collect();
            v.sort_by(|&i, &j| {
                (primary(i), stored_key(MIXED[i]), MIXED[i], folders[i]).cmp(&(
                    primary(j),
                    stored_key(MIXED[j]),
                    MIXED[j],
                    folders[j],
                ))
            });
            v
        };
        let by_name = sort_order(&entries, SortSpec::default(), true, |_| String::new(), &|i| folders[i]);
        assert_eq!(by_name, want(&|_| Vec::new()), "Turkish letters, numbers, case ties");
        let spec = SortSpec { key: SortKey::Folder, dir: SortDir::Desc };
        let by_folder = sort_order(&entries, spec, true, |_| String::new(), &|i| folders[i]);
        let mut desc = want(&|i| stored_key(folders[i]));
        desc.reverse();
        assert_eq!(by_folder, desc);
    }

    #[test]
    fn apply_order_moves_each_item_to_its_place() {
        // Cycles of several lengths, fixed points and a reversal.
        for order in
            [vec![], vec![0], vec![2, 0, 1], vec![1, 0, 3, 2, 4], vec![5, 4, 3, 2, 1, 0], vec![3, 0, 4, 1, 2, 5]]
        {
            let mut items: Vec<String> = (0..order.len()).map(|i| format!("item {i}")).collect();
            let want: Vec<String> = order.iter().map(|&i| items[i].clone()).collect();
            apply_order(&mut items, &order);
            assert_eq!(items, want, "order {order:?}");
        }
    }
}
