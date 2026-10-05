//! Archive decisions without touching files: what a file is (by magic bytes and name), where an
//! entry may go, what an archive's folder is called, and how volumes are named.

use std::path::{Component, Path, PathBuf};

use crate::ops::names::{self, NameError, NameRules};

/// How a stream is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    None,
    Gz,
    Bz2,
    Xz,
    Zst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Format {
    Zip,
    SevenZ,
    Rar,
    /// A tar, plain or compressed.
    Tar(Codec),
    /// One compressed file (`notes.txt.gz`).
    Single(Codec),
    Cab,
    Iso,
    Udf,
    Cpio,
    Ar,
    Deb,
    /// A format only 7-Zip reads, by its extension (lowercase).
    Other(String),
}

/// Extensions of formats the downloaded 7-Zip handles and Gezik does not.
const OTHER_EXTENSIONS: [&str; 24] = [
    "lzh", "lha", "arj", "wim", "swm", "esd", "dmg", "hfs", "msi", "rpm", "z01", "vhd", "vhdx", "vmdk", "xar",
    "squashfs", "chm", "cramfs", "ext", "ext4", "fat", "ntfs", "qcow2", "uefi",
];

const TAR_ENDINGS: [&str; 9] = [".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tbz", ".tar.zst", ".tzst"];

/// What `head` (the file's first 0x9010 bytes, or all of a shorter file) is, with `name`
/// deciding tar against a single compressed file and naming the rare formats.
pub fn detect(head: &[u8], name: &str) -> Option<Format> {
    let at = |off: usize, m: &[u8]| head.len() >= off + m.len() && &head[off..off + m.len()] == m;
    let lower = name.to_ascii_lowercase();
    let tar_or_single = |codec| {
        if TAR_ENDINGS.iter().any(|e| lower.ends_with(e)) { Format::Tar(codec) } else { Format::Single(codec) }
    };
    if at(0, b"PK\x03\x04") || at(0, b"PK\x05\x06") || at(0, b"PK\x07\x08") {
        return Some(Format::Zip);
    }
    if at(0, b"7z\xBC\xAF\x27\x1C") {
        return Some(Format::SevenZ);
    }
    if at(0, b"Rar!\x1A\x07\x01\x00") || at(0, b"Rar!\x1A\x07\x00") {
        return Some(Format::Rar);
    }
    if at(0, b"\x1F\x8B") {
        return Some(tar_or_single(Codec::Gz));
    }
    if at(0, b"\xFD7zXZ\x00") {
        return Some(tar_or_single(Codec::Xz));
    }
    if at(0, b"BZh") && head.len() > 3 && (b'1'..=b'9').contains(&head[3]) {
        return Some(tar_or_single(Codec::Bz2));
    }
    if at(0, b"\x28\xB5\x2F\xFD") {
        return Some(tar_or_single(Codec::Zst));
    }
    if at(0, b"MSCF\0\0\0\0") {
        return Some(Format::Cab);
    }
    if at(0, b"!<arch>\n") {
        return Some(if at(8, b"debian-binary") { Format::Deb } else { Format::Ar });
    }
    if at(0, b"070701") || at(0, b"070702") || at(0, b"070707") || at(0, &[0xC7, 0x71]) || at(0, &[0x71, 0xC7]) {
        return Some(Format::Cpio);
    }
    if at(257, b"ustar") {
        return Some(Format::Tar(Codec::None));
    }
    if at(0x8001, b"CD001") {
        return Some(Format::Iso);
    }
    let nsr = |off: usize| at(off, b"NSR02") || at(off, b"NSR03");
    if at(0x8001, b"BEA01") && (nsr(0x8801) || nsr(0x9001)) {
        return Some(Format::Udf);
    }
    if head.len() >= 512 && v7_tar_checksum_ok(&head[..512]) {
        return Some(Format::Tar(Codec::None));
    }
    let ext = lower.rsplit_once('.')?.1;
    OTHER_EXTENSIONS.contains(&ext).then(|| Format::Other(ext.to_string()))
}

/// Old v7 tars have no magic: the octal checksum at 148..156 equals the header's byte sum
/// (the checksum field counted as spaces).
fn v7_tar_checksum_ok(block: &[u8]) -> bool {
    let field = std::str::from_utf8(&block[148..156]).unwrap_or("").trim_matches(|c: char| c == '\0' || c == ' ');
    let Ok(want) = u32::from_str_radix(field, 8) else { return false };
    let sum: u32 =
        block.iter().enumerate().map(|(i, &b)| if (148..156).contains(&i) { 32 } else { u32::from(b) }).sum();
    sum == want && block[0] != 0
}

/// `entry` (an untrusted name from inside an archive; `\` also separates) under `dest`, or
/// `None` if it would escape or cannot be a portable name: `..`, roots, drive prefixes, UNC,
/// `:` (Windows alternate streams), Windows device names, empty.
pub fn safe_join(dest: &Path, entry: &str) -> Option<PathBuf> {
    let normalized = entry.replace('\\', "/");
    let mut out = dest.to_path_buf();
    let mut any = false;
    for c in Path::new(&normalized).components() {
        match c {
            Component::Normal(part) => {
                let part = part.to_str()?;
                if part.contains(':') || part.contains('\0') {
                    return None;
                }
                if let Err(NameError::Reserved(_)) = names::validate_name(part, NameRules::Windows) {
                    return None;
                }
                out.push(part);
                any = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    any.then_some(out)
}

/// How a set of volumes is numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    /// `big.7z.001`, `big.7z.002`…: the number is appended to the whole name.
    Numbered { width: usize },
    /// `big.part01.rar`, `big.part02.rar`…: the number goes between the name and `.rar`.
    RarPart { width: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSet {
    /// The name `volume_name` takes: `big.7z` for `big.7z.003`, `big` for `big.part03.rar`.
    pub base: String,
    pub kind: VolumeKind,
    /// The first volume, the one to open.
    pub first: String,
}

/// The volume set `name` belongs to, if it is one volume of one (`x.7z.003`, `x.zip.001`,
/// `x.part2.rar`).
pub fn volume_set(name: &str) -> Option<VolumeSet> {
    let (base, kind) = split_volume(name)?;
    let first = volume_name(base, kind, 1);
    Some(VolumeSet { base: base.to_string(), kind, first })
}

/// `name` split into its set's base and numbering.
fn split_volume(name: &str) -> Option<(&str, VolumeKind)> {
    let lower = name.to_ascii_lowercase();
    if let Some(head) = lower.strip_suffix(".rar")
        && let Some(at) = head.rfind(".part")
    {
        let digits = &head[at + 5..];
        if at > 0 && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return Some((&name[..at], VolumeKind::RarPart { width: digits.len() }));
        }
    }
    let (base, digits) = name.rsplit_once('.')?;
    if digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit()) && !base.is_empty() {
        return Some((base, VolumeKind::Numbered { width: digits.len() }));
    }
    None
}

/// Volume `n` of the set with `base` and `kind`: `("big.7z", Numbered 3, 12)` → `big.7z.012`.
pub fn volume_name(base: &str, kind: VolumeKind, n: u32) -> String {
    match kind {
        VolumeKind::Numbered { width } => format!("{base}.{n:0width$}"),
        VolumeKind::RarPart { width } => format!("{base}.part{n:0width$}.rar"),
    }
}

/// What an extracted archive's folder is called: `a.tar.gz`, `a.part1.rar`, `a.7z.001` and
/// `a.zip` all give `a`.
pub fn archive_stem(name: &str) -> &str {
    let name = match split_volume(name) {
        Some((base, VolumeKind::RarPart { .. })) => return base,
        Some((base, VolumeKind::Numbered { .. })) => base,
        None => name,
    };
    names::split_name(name, false).0
}

/// What a single compressed file is called once unpacked: `notes.txt.gz` gives `notes.txt`.
pub fn single_stem(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for ext in [".gz", ".xz", ".bz2", ".zst"] {
        if lower.len() > ext.len() && lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Whether what an archive holds at its root (name, is a folder) is one item, so it can be
/// placed as is instead of inside a folder of its own.
pub fn single_root(names: &[(String, bool)]) -> bool {
    names.len() == 1
}

/// Whether an archive claiming `declared` bytes from `packed` bytes looks like a bomb.
pub fn bomb_suspect(declared: u64, packed: u64) -> bool {
    declared > 10 << 30 && declared / packed.max(1) > 1000
}

/// The sizes offered for splitting an archive into volumes.
pub fn split_sizes() -> [(&'static str, u64); 3] {
    [("100 MB", 100_000_000), ("700 MB", 700_000_000), ("4 GB (FAT32)", 4_294_967_295)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(bytes: &[u8], at: usize) -> Vec<u8> {
        let mut h = vec![0u8; at + bytes.len()];
        h[at..].copy_from_slice(bytes);
        h
    }

    #[test]
    fn formats_by_magic() {
        assert_eq!(detect(b"PK\x03\x04rest", "a.bin"), Some(Format::Zip));
        assert_eq!(detect(b"7z\xBC\xAF\x27\x1C", "x"), Some(Format::SevenZ));
        assert_eq!(detect(b"Rar!\x1A\x07\x00", "x"), Some(Format::Rar));
        assert_eq!(detect(b"Rar!\x1A\x07\x01\x00", "x"), Some(Format::Rar));
        assert_eq!(detect(b"\x1F\x8B\x08", "a.tar.gz"), Some(Format::Tar(Codec::Gz)));
        assert_eq!(detect(b"\x1F\x8B\x08", "a.tgz"), Some(Format::Tar(Codec::Gz)));
        assert_eq!(detect(b"\x1F\x8B\x08", "notes.txt.gz"), Some(Format::Single(Codec::Gz)));
        assert_eq!(detect(b"\xFD7zXZ\x00", "a.tar.xz"), Some(Format::Tar(Codec::Xz)));
        assert_eq!(detect(b"BZh9", "a.bz2"), Some(Format::Single(Codec::Bz2)));
        assert_eq!(detect(b"\x28\xB5\x2F\xFD", "a.tar.zst"), Some(Format::Tar(Codec::Zst)));
        assert_eq!(detect(&head(b"ustar\x0000", 257), "a.tar"), Some(Format::Tar(Codec::None)));
        assert_eq!(detect(b"MSCF\0\0\0\0", "a.cab"), Some(Format::Cab));
        assert_eq!(detect(&head(b"CD001", 0x8001), "a.iso"), Some(Format::Iso));
        assert_eq!(detect(b"070701", "a.cpio"), Some(Format::Cpio));
        assert_eq!(detect(b"070707", "a.cpio"), Some(Format::Cpio));
        assert_eq!(detect(b"!<arch>\ndebian-binary   ", "a.deb"), Some(Format::Deb));
        assert_eq!(detect(b"!<arch>\nfoo.o/          ", "a.a"), Some(Format::Ar));
        assert_eq!(detect(b"hello", "a.txt"), None);
    }

    #[test]
    fn udf_and_v7_tar() {
        let mut udf = head(b"BEA01", 0x8001);
        udf.resize(0x9010, 0);
        udf[0x8801..0x8801 + 5].copy_from_slice(b"NSR02");
        assert_eq!(detect(&udf, "a.iso"), Some(Format::Udf));

        let mut tar = vec![0u8; 512];
        tar[..5].copy_from_slice(b"a.txt");
        let sum: u32 = tar.iter().map(|&b| u32::from(b)).sum::<u32>() + 8 * 32;
        tar[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        tar[155] = b' ';
        assert_eq!(detect(&tar, "a.tar"), Some(Format::Tar(Codec::None)));
        tar[0] = b'b';
        assert_eq!(detect(&tar, "a.tar"), None);
    }

    #[test]
    fn rare_formats_go_to_seven_zip_by_extension() {
        for name in ["a.lzh", "a.lha", "a.arj", "a.wim", "a.dmg", "a.msi", "a.rpm", "a.z01", "a.vhd", "a.xar"] {
            assert!(matches!(detect(b"\0\0\0\0", name), Some(Format::Other(_))), "{name}");
        }
    }

    #[test]
    fn unsafe_names_are_refused() {
        let dest = Path::new("/stage");
        assert_eq!(safe_join(dest, "a/b.txt"), Some(dest.join("a").join("b.txt")));
        assert_eq!(safe_join(dest, "a\\b.txt"), Some(dest.join("a").join("b.txt")));
        assert_eq!(safe_join(dest, "./a"), Some(dest.join("a")));
        for bad in ["../x", "a/../../x", "/etc/passwd", "\\x", "C:\\x", "C:x", "\\\\srv\\s\\x", "a:stream", "", ".."] {
            assert_eq!(safe_join(dest, bad), None, "{bad}");
        }
        for reserved in ["CON", "con.txt", "a/NUL", "LPT1.log"] {
            assert_eq!(safe_join(dest, reserved), None, "{reserved}");
        }
    }

    #[test]
    fn stems_and_volumes() {
        assert_eq!(archive_stem("arsiv.tar.gz"), "arsiv");
        assert_eq!(archive_stem("Fotolar.zip"), "Fotolar");
        assert_eq!(archive_stem("big.part01.rar"), "big");
        assert_eq!(archive_stem("big.7z.003"), "big");
        assert_eq!(archive_stem("a.tgz"), "a");
        assert_eq!(single_stem("notes.txt.gz"), "notes.txt");
        assert_eq!(single_stem("notes.txt"), "notes.txt");
        let set = volume_set("big.7z.003").unwrap();
        assert_eq!(set.first, "big.7z.001");
        assert_eq!(volume_name("big.7z", set.kind, 12), "big.7z.012");
        assert_eq!(volume_set("x.part3.rar").unwrap().first, "x.part1.rar");
        assert_eq!(volume_set("x.part03.rar").unwrap().first, "x.part01.rar");
        assert!(volume_set("x.zip").is_none());
    }

    #[test]
    fn single_root_and_bombs() {
        assert!(single_root(&[("Fotolar".into(), true)]));
        assert!(single_root(&[("readme.txt".into(), false)]));
        assert!(!single_root(&[("a".into(), true), ("b.txt".into(), false)]));
        assert!(!single_root(&[]));
        assert!(bomb_suspect(11 << 30, 1 << 20));
        assert!(!bomb_suspect(11 << 30, 1 << 30));
        assert!(!bomb_suspect(1 << 30, 1));
    }
}
