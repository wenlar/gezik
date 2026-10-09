use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownFolder {
    pub path: PathBuf,
    /// Name in the OS language (`Belgeler` on Turkish Windows); `Home` for the home folder.
    pub name: String,
}

/// Home, Desktop, Documents, Downloads, Pictures, Music, Videos — those that exist.
pub fn known_folders() -> Vec<KnownFolder> {
    let candidates = [
        dirs::home_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::picture_dir(),
        dirs::audio_dir(),
        dirs::video_dir(),
    ];
    let mut out: Vec<KnownFolder> = Vec::new();
    for (index, path) in candidates.into_iter().enumerate() {
        let Some(path) = path.filter(|p| p.is_dir()) else { continue };
        if out.iter().any(|f| f.path == path) {
            continue; // e.g. Desktop == Home on some Linux setups
        }
        let name = if index == 0 {
            "Home".to_owned()
        } else {
            display_name(&path)
                .unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        };
        out.push(KnownFolder { path, name });
    }
    out
}

/// The name the OS shows for `path` (localized on Windows), if available.
#[cfg(windows)]
pub(crate) fn display_name(path: &std::path::Path) -> Option<String> {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SIGDN_NORMALDISPLAY};
    use windows::core::HSTRING;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?;
        let text = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        text
    }
}

/// macOS: Finder's name ("Belgeler" for Documents in Turkish).
#[cfg(target_os = "macos")]
pub(crate) fn display_name(path: &std::path::Path) -> Option<String> {
    crate::mac::finder::display_name(path)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn display_name(path: &std::path::Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_comes_first_and_everything_exists() {
        let folders = known_folders();
        assert_eq!(folders[0].name, "Home");
        assert_eq!(Some(folders[0].path.clone()), dirs::home_dir());
        assert!(folders.iter().all(|f| f.path.is_dir() && !f.name.is_empty()));
    }
}
