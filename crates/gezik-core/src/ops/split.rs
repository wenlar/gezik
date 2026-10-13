//! Splitting a file into raw parts and joining them (spec 11 §12.4–12.5): the part size typed,
//! the parts' names (`film.mkv.001`…), and the `.crc` file Total Commander writes beside them.

use crate::batch::archive::{VolumeKind, volume_name, volume_set};

/// Most parts one file is split into (three digits).
pub const MAX_PARTS: u64 = 999;

/// A typed part size in bytes: `100 MB`, `4.7 GB`, `4,095 MB`, `650` (MB), `500 KB`, `1024 B`;
/// units are decimal (1 MB = 1,000,000 bytes), as disks and networks count them.
pub fn parse_size(text: &str) -> Result<u64, String> {
    let text = text.trim().to_ascii_lowercase();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')).unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let unit = match unit.trim() {
        "" | "m" | "mb" => 1_000_000.0,
        "b" => 1.0,
        "k" | "kb" => 1_000.0,
        "g" | "gb" => 1_000_000_000.0,
        _ => return Err(gezik_i18n::t!("Type a size like 100 MB, 4.7 GB or 500 KB").into_owned()),
    };
    // `4,095`: a thousands comma; `1,5`: a decimal one.
    let number = match number.split_once(',') {
        Some((_, after)) if after.len() == 3 && !after.contains(['.', ',']) => number.replace(',', ""),
        _ => number.replace(',', "."),
    };
    let value: f64 =
        number.parse().map_err(|_| gezik_i18n::t!("Type a size like 100 MB, 4.7 GB or 500 KB").into_owned())?;
    let bytes = (value * unit).round();
    if !(1.0..=u64::MAX as f64).contains(&bytes) {
        return Err(gezik_i18n::t!("The parts need a size above 0").into_owned());
    }
    Ok(bytes as u64)
}

/// How many parts of `size` bytes a file of `len` bytes makes (an empty file: one).
pub fn part_count(len: u64, size: u64) -> u64 {
    len.div_ceil(size.max(1)).max(1)
}

/// Part `n` (from 1) of `name`: `film.mkv.001`.
pub fn part_name(name: &str, n: u32) -> String {
    volume_name(name, VolumeKind::Numbered { width: 3 }, n)
}

/// The `.crc` file of `name`: `film.mkv.crc`.
pub fn crc_name(name: &str) -> String {
    format!("{name}.crc")
}

/// What the `.crc` file says (Total Commander's form), CRLF.
pub fn crc_text(name: &str, size: u64, crc: u32) -> String {
    format!("filename={name}\r\nsize={size}\r\ncrc32={crc:08X}\r\n")
}

/// What a `.crc` file holds; a line it does not know is passed over.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CrcInfo {
    /// The joined file's name, only if it is a plain name (it comes from a file anyone could
    /// have written: no folder, `..` or drive in it).
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub crc: Option<u32>,
}

pub fn parse_crc(text: &str) -> CrcInfo {
    let mut info = CrcInfo::default();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "filename" => {
                let plain =
                    !value.is_empty() && value != "." && value != ".." && !value.contains(['/', '\\', ':', '\0']);
                info.filename = plain.then(|| value.to_owned());
            }
            "size" => info.size = value.parse().ok(),
            "crc32" => info.crc = u32::from_str_radix(value, 16).ok(),
            _ => {}
        }
    }
    info
}

/// The name a part or `.crc` file was split from: `film.mkv.003` and `film.mkv.crc` give
/// `film.mkv`; anything else (a RAR part, any other file) gives `None`.
pub fn split_base(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    if lower.len() > 4 && lower.ends_with(".crc") {
        return Some(name[..name.len() - 4].to_owned());
    }
    match volume_set(name)? {
        set if matches!(set.kind, VolumeKind::Numbered { .. }) => Some(set.base),
        _ => None,
    }
}

/// The first part number missing from `found` (sorted, from 1): `[1, 2, 4]` → 3; `None` when
/// they run on without a gap.
pub fn first_gap(found: &[u32]) -> Option<u32> {
    (1u32..).zip(found).find(|(n, got)| n != *got).map(|(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_read_in_decimal_units() {
        assert_eq!(parse_size("100 MB"), Ok(100_000_000));
        assert_eq!(parse_size("650"), Ok(650_000_000), "a bare number is MB");
        assert_eq!(parse_size("4,095 MB"), Ok(4_095_000_000));
        assert_eq!(parse_size("4.7 GB"), Ok(4_700_000_000));
        assert_eq!(parse_size("1,5 gb"), Ok(1_500_000_000), "a decimal comma");
        assert_eq!(parse_size(" 500kb "), Ok(500_000));
        assert_eq!(parse_size("1024 B"), Ok(1024));
        assert!(parse_size("0 MB").is_err());
        assert!(parse_size("ten MB").is_err());
        assert!(parse_size("5 TB").is_err());
        assert!(parse_size("").is_err());
    }

    #[test]
    fn parts_are_counted_and_named() {
        assert_eq!(part_count(250, 100), 3);
        assert_eq!(part_count(200, 100), 2);
        assert_eq!(part_count(0, 100), 1, "an empty file: one empty part");
        assert_eq!(part_name("film.mkv", 1), "film.mkv.001");
        assert_eq!(part_name("film.mkv", 12), "film.mkv.012");
        assert_eq!(crc_name("film.mkv"), "film.mkv.crc");
    }

    #[test]
    fn the_crc_file_goes_both_ways() {
        let text = crc_text("film.mkv", 1234, 0xCBF4_3926);
        assert_eq!(text, "filename=film.mkv\r\nsize=1234\r\ncrc32=CBF43926\r\n");
        let info = parse_crc(&text);
        assert_eq!(info, CrcInfo { filename: Some("film.mkv".into()), size: Some(1234), crc: Some(0xCBF4_3926) });
        assert_eq!(parse_crc("\u{feff}FileName = a b.txt\nCRC32=0000abcd\nnote").crc, Some(0xabcd));
        for bad in ["../x", "a/b", "C:x", "..", "", "a\\b"] {
            assert_eq!(parse_crc(&format!("filename={bad}")).filename, None, "{bad}");
        }
        assert_eq!(parse_crc("size=lots").size, None);
    }

    #[test]
    fn a_part_or_crc_names_its_set() {
        assert_eq!(split_base("film.mkv.003").as_deref(), Some("film.mkv"));
        assert_eq!(split_base("film.mkv.CRC").as_deref(), Some("film.mkv"));
        assert_eq!(split_base("a.7z.001").as_deref(), Some("a.7z"));
        assert_eq!(split_base("x.part2.rar"), None);
        assert_eq!(split_base("film.mkv"), None);
        assert_eq!(split_base(".crc"), None);
        assert_eq!(first_gap(&[1, 2, 3]), None);
        assert_eq!(first_gap(&[1, 2, 4]), Some(3));
        assert_eq!(first_gap(&[2]), Some(1));
        assert_eq!(first_gap(&[]), None);
    }
}
