//! The freedesktop.org trash format: `files/NAME` holds the entry, `info/NAME.trashinfo` says
//! where it came from.

/// `path` as the `Path=` value: bytes outside `A-Z a-z 0-9 - _ . ~ /` percent-encoded.
pub(crate) fn encode_path(path: &[u8]) -> String {
    let mut out = String::with_capacity(path.len());
    for &b in path {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The `.trashinfo` file for `path` (absolute) deleted at `deleted_at` (local time,
/// `YYYY-MM-DDThh:mm:ss`).
pub(crate) fn info_text(path: &[u8], deleted_at: &str) -> String {
    format!("[Trash Info]\nPath={}\nDeletionDate={deleted_at}\n", encode_path(path))
}

pub(crate) fn format_date(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

/// `name`, else `name.2`, `name.3`… the first `taken` does not claim.
pub(crate) fn free_name(name: &[u8], taken: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    if !taken(name) {
        return name.to_vec();
    }
    (2u32..)
        .map(|n| {
            let mut candidate = name.to_vec();
            candidate.extend_from_slice(format!(".{n}").as_bytes());
            candidate
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| name.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_percent_encoded() {
        assert_eq!(encode_path(b"/home/a/My File.txt"), "/home/a/My%20File.txt");
        assert_eq!(encode_path("/home/a/şarkı".as_bytes()), "/home/a/%C5%9Fark%C4%B1");
    }

    #[test]
    fn info_file_layout() {
        let date = format_date(2026, 10, 4, 9, 5, 0);
        assert_eq!(date, "2026-10-04T09:05:00");
        assert_eq!(info_text(b"/a/b", &date), "[Trash Info]\nPath=/a/b\nDeletionDate=2026-10-04T09:05:00\n");
    }

    #[test]
    fn free_names_count_up() {
        let taken: [&[u8]; 2] = [b"a.txt", b"a.txt.2"];
        assert_eq!(free_name(b"a.txt", |n| taken.contains(&n)), b"a.txt.3");
        assert_eq!(free_name(b"b", |_| false), b"b");
    }
}
