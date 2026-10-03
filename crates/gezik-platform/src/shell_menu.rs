//! Windows: the Explorer context menu for a file/folder or a folder's background, with
//! Gezik's own items inserted at the top.

use std::cell::RefCell;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_NORMAL, CMINVOKECOMMANDINFO, DefSubclassProc, GCS_VERBA, IContextMenu, IContextMenu2, IContextMenu3,
    IShellFolder, RemoveWindowSubclass, SHBindToParent, SHGetDesktopFolder, SHParseDisplayName, SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DeleteMenu, DestroyMenu, GetCursorPos, GetMenuItemCount, GetMenuItemID, HMENU, InsertMenuW,
    MF_BYCOMMAND, MF_BYPOSITION, MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenuEx, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
};
use windows::core::{HSTRING, Interface, PCSTR, PCWSTR, PSTR};

use crate::{MenuOutcome, MenuTarget};

/// Shell command ids start here; Gezik's own ids must be below it.
const FIRST_SHELL_ID: u32 = 1000;
const LAST_SHELL_ID: u32 = 0x7FFF;
const SUBCLASS_ID: usize = 0x6765_7A69;

thread_local! {
    /// The menu being tracked, so the subclass proc can forward submenu/owner-draw messages.
    static ACTIVE: RefCell<Option<IContextMenu>> = const { RefCell::new(None) };
}

/// Shows the Explorer menu for `target` at the mouse cursor, with `extra` (id, label) items
/// on top. Call on the UI thread; blocks until the menu closes.
pub fn show_shell_menu(
    window: &impl HasWindowHandle,
    target: &MenuTarget,
    extra: &[(u32, &str)],
) -> Result<MenuOutcome, String> {
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return Err("not a Win32 window".into()) };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let menu = context_menu_for(hwnd, target).map_err(|e| e.to_string())?;
        let hmenu = CreatePopupMenu().map_err(|e| e.to_string())?;
        let result = track(hwnd, hmenu, &menu, extra).map_err(|e| e.to_string());
        let _ = DestroyMenu(hmenu);
        result
    }
}

unsafe fn context_menu_for(hwnd: HWND, target: &MenuTarget) -> windows::core::Result<IContextMenu> {
    let path = match target {
        MenuTarget::Item(p) | MenuTarget::Background(p) => p,
    };
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    unsafe { SHParseDisplayName(&HSTRING::from(path.as_os_str()), None, &mut pidl, 0, None)? };
    let menu = unsafe {
        match target {
            MenuTarget::Item(_) => {
                let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
                let parent: IShellFolder = SHBindToParent(pidl, Some(&mut child))?;
                parent.GetUIObjectOf::<IContextMenu>(hwnd, &[child as *const ITEMIDLIST], None)
            }
            MenuTarget::Background(_) => {
                let desktop = SHGetDesktopFolder()?;
                let folder: windows::core::Result<IShellFolder> =
                    if (*pidl).mkid.cb == 0 { Ok(desktop) } else { desktop.BindToObject(pidl, None) };
                folder.and_then(|f| f.CreateViewObject::<IContextMenu>(hwnd))
            }
        }
    };
    unsafe { CoTaskMemFree(Some(pidl as *const _)) };
    menu
}

unsafe fn track(
    hwnd: HWND,
    hmenu: HMENU,
    menu: &IContextMenu,
    extra: &[(u32, &str)],
) -> windows::core::Result<MenuOutcome> {
    unsafe {
        menu.QueryContextMenu(hmenu, 0, FIRST_SHELL_ID, LAST_SHELL_ID, CMF_NORMAL).ok()?;
        remove_explorer_only_items(hmenu, menu);
        if !extra.is_empty() {
            for (position, (id, label)) in extra.iter().enumerate() {
                InsertMenuW(hmenu, position as u32, MF_BYPOSITION | MF_STRING, *id as usize, &HSTRING::from(*label))?;
            }
            InsertMenuW(hmenu, extra.len() as u32, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null())?;
        }

        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor)?;
        ACTIVE.with(|active| *active.borrow_mut() = Some(menu.clone()));
        let _ = SetWindowSubclass(hwnd, Some(forward_menu_messages), SUBCLASS_ID, 0);
        let chosen =
            TrackPopupMenuEx(hmenu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, cursor.x, cursor.y, hwnd, None).0 as u32;
        let _ = RemoveWindowSubclass(hwnd, Some(forward_menu_messages), SUBCLASS_ID);
        ACTIVE.with(|active| *active.borrow_mut() = None);

        Ok(match chosen {
            0 => MenuOutcome::Dismissed,
            id if id < FIRST_SHELL_ID => MenuOutcome::Gezik(id),
            id => {
                let info = CMINVOKECOMMANDINFO {
                    cbSize: size_of::<CMINVOKECOMMANDINFO>() as u32,
                    hwnd,
                    // MAKEINTRESOURCE: the verb is the command's offset.
                    lpVerb: PCSTR((id - FIRST_SHELL_ID) as usize as *const u8),
                    nShow: SW_SHOWNORMAL.0,
                    ..Default::default()
                };
                menu.InvokeCommand(&info)?;
                MenuOutcome::SystemCommandRan
            }
        })
    }
}

/// Removes verbs that only work inside Explorer itself (they do nothing elsewhere).
unsafe fn remove_explorer_only_items(hmenu: HMENU, menu: &IContextMenu) {
    unsafe {
        let count = GetMenuItemCount(Some(hmenu)).max(0);
        let mut doomed = Vec::new();
        for i in 0..count {
            let id = GetMenuItemID(hmenu, i);
            if id == u32::MAX || id < FIRST_SHELL_ID {
                continue;
            }
            let mut verb = [0u8; 64];
            let found = menu.GetCommandString(
                (id - FIRST_SHELL_ID) as usize,
                GCS_VERBA,
                None,
                PSTR(verb.as_mut_ptr()),
                verb.len() as u32,
            );
            let end = verb.iter().position(|&b| b == 0).unwrap_or(verb.len());
            if found.is_ok() && verb[..end].eq_ignore_ascii_case(b"rename") {
                doomed.push(id);
            }
        }
        for id in doomed {
            let _ = DeleteMenu(hmenu, id, MF_BYCOMMAND);
        }
    }
}

/// Forwards owner-draw and submenu messages ("Send to", "Open with") while the menu is open.
unsafe extern "system" fn forward_menu_messages(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        let handled = ACTIVE.with(|active| {
            let active = active.borrow();
            let menu = active.as_ref()?;
            unsafe {
                if let Ok(menu3) = menu.cast::<IContextMenu3>() {
                    let mut result = LRESULT(0);
                    menu3.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result)).ok()?;
                    return Some(result);
                }
                if let Ok(menu2) = menu.cast::<IContextMenu2>() {
                    menu2.HandleMenuMsg(msg, wparam, lparam).ok()?;
                    return Some(LRESULT(0));
                }
            }
            None
        });
        if let Some(result) = handled {
            return result;
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Number of entries the Shell menu for `target` has (for tests; nothing is shown).
#[cfg(test)]
fn count_items(target: &MenuTarget) -> windows::core::Result<i32> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let menu = context_menu_for(HWND::default(), target)?;
        let hmenu = CreatePopupMenu()?;
        menu.QueryContextMenu(hmenu, 0, FIRST_SHELL_ID, LAST_SHELL_ID, CMF_NORMAL).ok()?;
        let count = GetMenuItemCount(Some(hmenu));
        let _ = DestroyMenu(hmenu);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_folder_has_a_shell_menu() {
        let home = dirs::home_dir().unwrap();
        let item = count_items(&crate::MenuTarget::Item(home.clone())).unwrap();
        let background = count_items(&crate::MenuTarget::Background(home)).unwrap();
        assert!(item > 5, "item menu had {item} entries");
        assert!(background > 2, "background menu had {background} entries");
    }
}
