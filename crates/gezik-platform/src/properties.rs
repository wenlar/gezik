//! Windows: the system's Properties window for files, folders and drives (Alt+Enter, spec 9
//! §4.4). Gezik has no Info window here: Explorer's has the permissions, attributes and
//! security already, and asks for administrator rights itself.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, IDataObject};
use windows::Win32::UI::Shell::{SHMultiFileProperties, SHOP_FILEPATH, SHObjectProperties};
use windows::core::PCWSTR;

use crate::shell_menu::{children_object, parse_name, shared_parent};

/// Several items of one folder show in one window; one item, or items of different folders
/// (search results, drives), show the first one's.
fn together(paths: &[PathBuf]) -> Option<(&Path, Vec<&OsStr>)> {
    if paths.len() < 2 {
        return None;
    }
    shared_parent(paths)
}

/// Opens the Properties window of `paths`. It is modeless (the Shell runs it on its own
/// thread): this returns once it is up.
pub fn show_properties(window: &impl HasWindowHandle, paths: &[PathBuf]) -> Result<(), String> {
    let Some(first) = paths.first() else { return Ok(()) };
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return Err("not a Win32 window".into()) };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    // SAFETY: COM is set up for this (the UI) thread; every pointer and string lives through its call.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Some((folder, names)) = together(paths) {
            let data: IDataObject = children_object(hwnd, folder, &names).map_err(|e| e.to_string())?;
            return SHMultiFileProperties(&data, 0).map_err(|e| e.to_string());
        }
        let name = parse_name(first);
        if SHObjectProperties(Some(hwnd), SHOP_FILEPATH, &name, PCWSTR::null()).as_bool() {
            Ok(())
        } else {
            Err("Windows did not open its Properties window".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_of_one_folder_show_together() {
        let p = |s: &str| PathBuf::from(s);
        assert!(together(&[p(r"C:\d\a.txt")]).is_none(), "one item alone");
        let two = [p(r"C:\d\a.txt"), p(r"C:\d\b")];
        let (folder, names) = together(&two).unwrap();
        assert_eq!((folder, names.len()), (Path::new(r"C:\d"), 2));
        assert!(together(&[p(r"C:\d\a.txt"), p(r"C:\e\b.txt")]).is_none(), "other folders: the first alone");
        assert!(together(&[p(r"C:\"), p(r"D:\")]).is_none(), "drives: the first alone");
    }
}
