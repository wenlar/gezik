//! Sorting: natural name order (`file2` before `file10`, letters in Turkish alphabet
//! order) and the column sort keys. Every OS sorts the same, so a synced folder looks the
//! same everywhere.

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
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [SortKey::Name, SortKey::Modified, SortKey::Created, SortKey::Type, SortKey::Size];

    pub fn as_str(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Modified => "modified",
            SortKey::Created => "created",
            SortKey::Type => "type",
            SortKey::Size => "size",
        }
    }

    pub fn parse(text: &str) -> Option<SortKey> {
        SortKey::ALL.into_iter().find(|k| k.as_str() == text)
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

/// A name cut into runs: digits compare by value, everything else letter by letter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    // Declared first: at the same position a number sorts before text (`a1` < `ab`).
    // `Small` is a number up to 38 digits without leading zeros, compared by value; `Big`
    // (longer) compares by `len` then `digits`, which is by value too, and is always larger.
    // `zeros` then puts `7` before `007`.
    Small { value: u128, zeros: usize },
    Big { len: usize, digits: String, zeros: usize },
    Text(Vec<u32>),
}

/// What [`natural_cmp`] compares, computed once per name when sorting many.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NaturalKey(Vec<Part>);

pub fn natural_key(name: &str) -> NaturalKey {
    let mut parts = Vec::new();
    let mut chars = name.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut run = String::new();
            while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                run.push(d);
                chars.next();
            }
            let trimmed = run.trim_start_matches('0');
            let digits = if trimmed.is_empty() { "0" } else { trimmed };
            let zeros = run.len() - digits.len();
            parts.push(match digits.parse::<u128>() {
                Ok(value) if digits.len() <= 38 => Part::Small { value, zeros },
                _ => Part::Big { len: digits.len(), digits: digits.to_owned(), zeros },
            });
        } else {
            let mut weights = Vec::new();
            while let Some(&t) = chars.peek().filter(|t| !t.is_ascii_digit()) {
                weights.push(weight(t));
                chars.next();
            }
            parts.push(Part::Text(weights));
        }
    }
    NaturalKey(parts)
}

/// Natural order, ignoring case; names equal that way are ordered by their exact text, so
/// the order is total and the same on every run.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    natural_key(a).cmp(&natural_key(b)).then_with(|| a.cmp(b))
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

/// The column value an entry sorts by, before its name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Primary {
    None,
    Time(Option<SystemTime>),
    Size(u64),
    Type(NaturalKey, String),
}

/// Sorts `entries` by `spec`: folders first (in either direction), then the column, then
/// natural name order, then the exact name so the order is total. `type_name` gives the
/// Type column's text; it is called once per entry, and only when sorting by type.
pub fn sort_entries(entries: &mut Vec<Entry>, spec: SortSpec, type_name: impl Fn(&Entry) -> String) {
    let mut keyed: Vec<((Primary, NaturalKey), Entry)> = entries
        .drain(..)
        .map(|e| {
            let primary = match spec.key {
                SortKey::Name => Primary::None,
                SortKey::Modified => Primary::Time(e.modified),
                SortKey::Created => Primary::Time(e.created),
                SortKey::Size => Primary::Size(if e.is_dir { 0 } else { e.size }),
                SortKey::Type => Primary::Type(natural_key(&type_name(&e)), e.extension().to_lowercase()),
            };
            ((primary, natural_key(&e.name)), e)
        })
        .collect();
    keyed.sort_by(|(ka, a), (kb, b)| {
        b.is_dir.cmp(&a.is_dir).then_with(|| {
            let order = ka.cmp(kb).then_with(|| a.name.cmp(&b.name));
            if spec.dir == SortDir::Desc { order.reverse() } else { order }
        })
    });
    entries.extend(keyed.into_iter().map(|(_, e)| e));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    fn entry(name: &str, is_dir: bool, size: u64, secs: Option<u64>) -> Entry {
        let time = secs.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        Entry { name: name.to_owned(), is_dir, size, modified: time, created: time }
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
        sort_entries(&mut v, SortSpec::default(), |_| String::new());
        assert_eq!(names(&v), ["alpha", "Zeta", "a9", "a10", "b.txt"]);
        sort_entries(&mut v, SortSpec { key: SortKey::Name, dir: SortDir::Desc }, |_| String::new());
        assert_eq!(names(&v), ["Zeta", "alpha", "b.txt", "a10", "a9"]);
    }

    #[test]
    fn sorts_by_size_and_dates() {
        let mut v =
            vec![entry("big", false, 900, Some(1)), entry("small", false, 5, Some(3)), entry("none", false, 50, None)];
        sort_entries(&mut v, SortSpec { key: SortKey::Size, dir: SortDir::Asc }, |_| String::new());
        assert_eq!(names(&v), ["small", "none", "big"]);
        sort_entries(&mut v, SortSpec { key: SortKey::Modified, dir: SortDir::Asc }, |_| String::new());
        assert_eq!(names(&v), ["none", "big", "small"], "unknown times first");
        sort_entries(&mut v, SortSpec { key: SortKey::Created, dir: SortDir::Desc }, |_| String::new());
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
        sort_entries(&mut v, SortSpec { key: SortKey::Type, dir: SortDir::Asc }, type_name);
        assert_eq!(names(&v), ["a.jpg", "b.png", "c.PNG", "a.txt"]);
    }

    #[test]
    fn equal_keys_keep_a_total_order() {
        let mut v = vec![entry("same", false, 1, None), entry("Same", false, 1, None)];
        sort_entries(&mut v, SortSpec { key: SortKey::Size, dir: SortDir::Asc }, |_| String::new());
        assert_eq!(names(&v), ["Same", "same"]);
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

    /// `cargo test -p gezik-core --release -- --ignored sorting_100k`
    #[test]
    #[ignore]
    fn sorting_100k_names_is_fast() {
        let mut v: Vec<Entry> = (0..100_000)
            .map(|i| entry(&format!("IMG_{:05} kopya şğı {}.jpg", (i * 7919) % 100_000, i), false, i, None))
            .collect();
        let start = std::time::Instant::now();
        sort_entries(&mut v, SortSpec::default(), |_| String::new());
        let took = start.elapsed();
        assert!(took.as_millis() <= 50, "took {took:?}");
    }
}
