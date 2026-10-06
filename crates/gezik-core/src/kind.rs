//! What kind of file an entry is, from its name: picks Gezik's own icon and its color,
//! and decides which files need a per-file icon lookup.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Kind {
    Folder = 0,
    File = 1,
    Image = 2,
    Video = 3,
    Audio = 4,
    Archive = 5,
    Document = 6,
    Spreadsheet = 7,
    Presentation = 8,
    Pdf = 9,
    Code = 10,
    Text = 11,
    Executable = 12,
    Font = 13,
    DiskImage = 14,
    Link = 15,
}

impl Kind {
    pub fn of(name: &str, is_dir: bool) -> Kind {
        if is_dir {
            return Kind::Folder;
        }
        if split_part(name).is_some() {
            return Kind::Archive;
        }
        match extension_lowercase(name).as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "svg" | "ico" | "heic" | "heif"
            | "avif" | "raw" | "cr2" | "nef" | "psd" => Kind::Image,
            "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "flv" | "mpg" | "mpeg" => Kind::Video,
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" | "opus" => Kind::Audio,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "txz" | "tbz" | "tbz2" | "tzst"
            | "cab" | "cpio" | "wim" | "swm" | "esd" | "lzh" | "lha" | "arj" | "xar" => Kind::Archive,
            "doc" | "docx" | "odt" | "rtf" | "pages" => Kind::Document,
            "xls" | "xlsx" | "ods" | "csv" | "numbers" => Kind::Spreadsheet,
            "ppt" | "pptx" | "odp" | "key" => Kind::Presentation,
            "pdf" => Kind::Pdf,
            "rs" | "c" | "h" | "cpp" | "hpp" | "cs" | "java" | "kt" | "go" | "py" | "js" | "ts" | "tsx" | "jsx"
            | "html" | "css" | "scss" | "json" | "toml" | "yaml" | "yml" | "xml" | "sh" | "ps1" | "bat" | "cmd"
            | "sql" | "swift" | "rb" | "php" | "lua" | "slint" => Kind::Code,
            "txt" | "md" | "log" | "ini" | "cfg" | "conf" => Kind::Text,
            "exe" | "msi" | "apk" | "deb" | "rpm" | "appimage" | "com" | "scr" | "app" => Kind::Executable,
            "ttf" | "otf" | "woff" | "woff2" => Kind::Font,
            "iso" | "img" | "dmg" | "vhd" | "vhdx" => Kind::DiskImage,
            "lnk" | "url" | "desktop" | "webloc" => Kind::Link,
            _ => Kind::File,
        }
    }

    /// The number the UI gets (`FileRow.kind`); see `file-icon.slint`.
    pub fn index(self) -> i32 {
        self as i32
    }
}

/// Whether a file's icon is stored in the file itself rather than given by its type, so it
/// must be looked up by path (programs, shortcuts, icon files).
pub fn has_own_icon(name: &str) -> bool {
    matches!(
        extension_lowercase(name).as_str(),
        "exe" | "lnk" | "ico" | "url" | "cur" | "ani" | "scr" | "msc" | "appref-ms"
    )
}

/// The Type column until (or unless) the system names the type: `PNG File`, `WIM archive`.
pub fn fallback_type_name(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "File folder".to_owned();
    }
    if let Some(own) = own_type_name(name, is_dir) {
        return own;
    }
    match extension_lowercase(name).as_str() {
        "" => "File".to_owned(),
        _ if Kind::of(name, false) == Kind::Archive => format!("{} archive", archive_ending(name).to_uppercase()),
        ext => format!("{} File", ext.to_uppercase()),
    }
}

/// Gezik's name for a type the system cannot know from the last ending alone, which comes
/// before the system's: a part of a split archive (`big.7z.001`: "Split 7Z archive",
/// `big.part2.rar`: "Split RAR archive", `film.001`: "Split archive").
pub fn own_type_name(name: &str, is_dir: bool) -> Option<String> {
    if is_dir {
        return None;
    }
    split_part(name).map(|ending| match ending {
        "" => "Split archive".to_owned(),
        ending => format!("Split {} archive", ending.to_uppercase()),
    })
}

/// The archive endings a split part's name tells (longest first, so `tar.gz` wins over `gz`).
const SPLIT_BASES: [&str; 12] =
    ["tar.gz", "tar.xz", "tar.bz2", "7z", "zip", "rar", "tar", "gz", "xz", "bz2", "zst", "wim"];

/// Whether `name` is a part of a split archive, and the ending of what was split (`7z` for
/// `big.7z.001`, `rar` for `big.part2.rar`, empty for `film.001`): a number of three or
/// more digits after an archive's name, or of exactly three after any name (a year such
/// as `report.2024` is not one).
fn split_part(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if let Some(head) = lower.strip_suffix(".rar")
        && let Some(at) = head.rfind(".part")
        && at > 0
    {
        let digits = &head[at + 5..];
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return Some("rar");
        }
    }
    let (base, digits) = lower.rsplit_once('.')?;
    if base.is_empty() || digits.len() < 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match SPLIT_BASES.iter().find(|ending| base.len() > ending.len() + 1 && base.ends_with(&format!(".{ending}"))) {
        Some(ending) => Some(ending),
        None if digits.len() == 3 => Some(""),
        None => None,
    }
}

/// The ending an archive's type is named by: `tar.gz` for `a.tar.gz`, else the last one.
fn archive_ending(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for two in ["tar.gz", "tar.xz", "tar.bz2", "tar.zst"] {
        if lower.len() > two.len() + 1 && lower.ends_with(&format!(".{two}")) {
            return two.to_owned();
        }
    }
    extension_lowercase(name)
}

fn extension_lowercase(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_come_from_the_extension_ignoring_case() {
        assert_eq!(Kind::of("Tatil.JPG", false), Kind::Image);
        assert_eq!(Kind::of("film.mkv", false), Kind::Video);
        assert_eq!(Kind::of("a.tar.gz", false), Kind::Archive);
        assert_eq!(Kind::of("main.rs", false), Kind::Code);
        assert_eq!(Kind::of("rapor.pdf", false), Kind::Pdf);
        assert_eq!(Kind::of("README", false), Kind::File);
        assert_eq!(Kind::of(".gitignore", false), Kind::File);
        assert_eq!(Kind::of("photos.jpg", true), Kind::Folder);
    }

    #[test]
    fn indexes_are_stable() {
        assert_eq!(Kind::Folder.index(), 0);
        assert_eq!(Kind::Link.index(), 15);
    }

    #[test]
    fn programs_and_shortcuts_have_their_own_icon() {
        assert!(has_own_icon("setup.EXE"));
        assert!(has_own_icon("Gezik.lnk"));
        assert!(!has_own_icon("notes.txt"));
        assert!(!has_own_icon("exe"));
    }

    #[test]
    fn fallback_type_names() {
        assert_eq!(fallback_type_name("a.png", false), "PNG File");
        assert_eq!(fallback_type_name("Makefile", false), "File");
        assert_eq!(fallback_type_name("x", true), "File folder");
        // Archives are named as such.
        assert_eq!(fallback_type_name("test.wim", false), "WIM archive");
        assert_eq!(fallback_type_name("a.7z", false), "7Z archive");
        assert_eq!(fallback_type_name("a.TAR.GZ", false), "TAR.GZ archive");
        assert_eq!(fallback_type_name("notes.txt.gz", false), "GZ archive");
    }

    #[test]
    fn windows_images_and_split_archives_are_archives() {
        for name in [
            "test.wim",
            "install.esd",
            "buyuk.7z.001",
            "buyuk.7z.002",
            "a.zip.001",
            "a.tar.gz.010",
            "b.part1.rar",
            "b.PART02.RAR",
            "film.001",
            "film.999",
            "big.7z.0001",
        ] {
            assert_eq!(Kind::of(name, false), Kind::Archive, "{name}");
        }
        // Numbers that are not parts.
        assert_eq!(Kind::of("report.2024", false), Kind::File);
        assert_eq!(Kind::of("a.01", false), Kind::File);
        assert_eq!(Kind::of(".001", false), Kind::File);
        assert_eq!(Kind::of("buyuk.7z.001", true), Kind::Folder);
    }

    #[test]
    fn split_parts_have_their_own_type_name() {
        assert_eq!(own_type_name("buyuk.7z.001", false).as_deref(), Some("Split 7Z archive"));
        assert_eq!(own_type_name("A.ZIP.002", false).as_deref(), Some("Split ZIP archive"));
        assert_eq!(own_type_name("a.tar.gz.003", false).as_deref(), Some("Split TAR.GZ archive"));
        assert_eq!(own_type_name("b.part1.rar", false).as_deref(), Some("Split RAR archive"));
        assert_eq!(own_type_name("film.001", false).as_deref(), Some("Split archive"));
        assert_eq!(own_type_name("b.rar", false), None);
        assert_eq!(own_type_name("test.wim", false), None);
        assert_eq!(own_type_name("report.2024", false), None);
        assert_eq!(own_type_name("x.7z.001", true), None);
        assert_eq!(fallback_type_name("buyuk.7z.003", false), "Split 7Z archive");
    }
}
