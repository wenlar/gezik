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
        match extension_lowercase(name).as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "svg" | "ico" | "heic" | "heif"
            | "avif" | "raw" | "cr2" | "nef" | "psd" => Kind::Image,
            "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "flv" | "mpg" | "mpeg" => Kind::Video,
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" | "opus" => Kind::Audio,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" => Kind::Archive,
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

/// The Type column until (or unless) the system names the type: `PNG File`.
/// On macOS, Finder's words: the system names nearly every type there.
pub fn fallback_type_name(name: &str, is_dir: bool) -> String {
    if cfg!(target_os = "macos") {
        return if is_dir { "Folder" } else { "Document" }.to_owned();
    }
    if is_dir {
        return "File folder".to_owned();
    }
    match extension_lowercase(name).as_str() {
        "" => "File".to_owned(),
        ext => format!("{} File", ext.to_uppercase()),
    }
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

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn fallback_type_names() {
        assert_eq!(fallback_type_name("a.png", false), "PNG File");
        assert_eq!(fallback_type_name("Makefile", false), "File");
        assert_eq!(fallback_type_name("x", true), "File folder");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fallback_type_names_are_finders_on_macos() {
        assert_eq!(fallback_type_name("a.png", false), "Document");
        assert_eq!(fallback_type_name("x", true), "Folder");
    }
}
