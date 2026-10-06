//! Windows: the Explorer context menu for a file/folder or a folder's background, with
//! Gezik's own items inserted at the top.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{E_INVALIDARG, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, ClientToScreen, EndPaint, PAINTSTRUCT};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_CANRENAME, CMF_NORMAL, CMINVOKECOMMANDINFO, DefSubclassProc, GCS_VERBA, IContextMenu, IContextMenu2,
    IContextMenu3, IShellFolder, RemoveWindowSubclass, SHBindToParent, SHGetDesktopFolder, SHParseDisplayName,
    SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HMENU, InsertMenuW, MF_BYPOSITION, MF_GRAYED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_DRAWITEM,
    WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_PAINT,
};
use windows::core::{HSTRING, Interface, PCSTR, PCWSTR, PSTR};

use crate::{MenuOutcome, MenuTarget, ShellVerb};

/// Shell command ids start here; Gezik's own ids must be below it.
const FIRST_SHELL_ID: u32 = 1000;
const LAST_SHELL_ID: u32 = 0x7FFF;
const SUBCLASS_ID: usize = 0x6765_7A69;

thread_local! {
    /// The menu being tracked, so the subclass proc can forward submenu/owner-draw messages.
    static ACTIVE: RefCell<Option<IContextMenu>> = const { RefCell::new(None) };
}

/// A submenu among Gezik's own items ("Commands ▸"): its title, where it goes among them
/// (0: first), and its items (id, label, enabled).
#[derive(Debug, Clone, Copy)]
pub struct ShellSubmenu<'a> {
    pub title: &'a str,
    pub at: usize,
    pub items: &'a [(u32, &'a str, bool)],
}

/// Shows the Explorer menu for `target` with `extra` (id, label) items on top, and `sub` among
/// them: at `at`, a point in the window's client area in physical pixels (a menu opened from
/// the keyboard), else at the mouse cursor. `can_rename`: the menu has the Shell's "Rename" (returned as
/// `ShellVerb::Rename`, the caller renames). Call on the UI thread; blocks until the menu
/// closes.
pub fn show_shell_menu(
    window: &impl HasWindowHandle,
    target: &MenuTarget,
    extra: &[(u32, &str)],
    sub: Option<ShellSubmenu<'_>>,
    at: Option<(i32, i32)>,
    can_rename: bool,
) -> Result<MenuOutcome, String> {
    validate_ids(extra)?;
    if let Some(sub) = &sub {
        let ids: Vec<(u32, &str)> = sub.items.iter().map(|(id, label, _)| (*id, *label)).collect();
        validate_ids(&ids)?;
    }
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return Err("not a Win32 window".into()) };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let menu = context_menu_for(hwnd, target).map_err(|e| e.to_string())?;
        let hmenu = CreatePopupMenu().map_err(|e| e.to_string())?;
        let result = track(hwnd, hmenu, &menu, extra, sub, at, can_rename).map_err(|e| e.to_string());
        let _ = DestroyMenu(hmenu);
        result
    }
}

/// Without CMF_CANRENAME the Shell leaves its "Rename" out.
fn query_flags(can_rename: bool) -> u32 {
    if can_rename { CMF_NORMAL | CMF_CANRENAME } else { CMF_NORMAL }
}

/// Gezik's own item ids must be in 1..FIRST_SHELL_ID (0 means "dismissed", 1000+ are Shell ids).
fn validate_ids(extra: &[(u32, &str)]) -> Result<(), String> {
    if extra.iter().all(|(id, _)| (1..FIRST_SHELL_ID).contains(id)) {
        Ok(())
    } else {
        Err("menu item ids must be in 1..1000".into())
    }
}

unsafe fn context_menu_for(hwnd: HWND, target: &MenuTarget) -> windows::core::Result<IContextMenu> {
    match target {
        MenuTarget::Item(path) => unsafe { items_menu(hwnd, std::slice::from_ref(path)) },
        MenuTarget::Items(paths) => match shared_parent(paths) {
            Some((parent, names)) => unsafe { children_menu(hwnd, parent, &names) },
            None => unsafe { items_menu(hwnd, paths) },
        },
        MenuTarget::Background(path) => unsafe { background_menu(hwnd, path) },
    }
}

/// The folder all of `paths` are in and their names; `None` if there is no such folder
/// (drives, or paths from different folders).
pub(crate) fn shared_parent(paths: &[PathBuf]) -> Option<(&Path, Vec<&std::ffi::OsStr>)> {
    let parent = paths.first()?.parent()?;
    let names: Option<Vec<_>> =
        paths.iter().map(|path| path.file_name().filter(|_| path.parent() == Some(parent))).collect();
    Some((parent, names?))
}

/// The menu of the entries `names` in `folder`. The folder is bound once and each name is
/// parsed relative to it, so a selection of 100 000 files opens quickly.
unsafe fn children_menu(hwnd: HWND, folder: &Path, names: &[&std::ffi::OsStr]) -> windows::core::Result<IContextMenu> {
    unsafe { children_object(hwnd, folder, names) }
}

/// The Shell's `T` (a menu, a data object) for the entries `names` in `folder`, as Explorer
/// gets it for a selection.
pub(crate) unsafe fn children_object<T: Interface>(
    hwnd: HWND,
    folder: &Path,
    names: &[&std::ffi::OsStr],
) -> windows::core::Result<T> {
    let shell_folder = unsafe { bind_folder(folder)? };
    let mut children: Vec<*mut ITEMIDLIST> = Vec::with_capacity(names.len());
    let parsed = names.iter().try_for_each(|name| {
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        unsafe {
            shell_folder.ParseDisplayName(hwnd, None, &HSTRING::from(*name), None, &mut child, std::ptr::null_mut())?
        };
        children.push(child);
        Ok(())
    });
    // Separate from the parsing, so `?` cannot skip the CoTaskMemFree below.
    let object = parsed.and_then(|()| unsafe {
        let children: Vec<*const ITEMIDLIST> = children.iter().map(|&c| c as *const ITEMIDLIST).collect();
        shell_folder.GetUIObjectOf::<T>(hwnd, &children, None)
    });
    for child in children {
        unsafe { CoTaskMemFree(Some(child as *const _)) };
    }
    object
}

/// `folder` as a shell folder (the desktop for the namespace root).
unsafe fn bind_folder(folder: &Path) -> windows::core::Result<IShellFolder> {
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    unsafe { SHParseDisplayName(&HSTRING::from(folder.as_os_str()), None, &mut pidl, 0, None)? };
    // The closure keeps `?` from skipping the CoTaskMemFree below.
    let bound = (|| unsafe {
        let desktop = SHGetDesktopFolder()?;
        if (*pidl).mkid.cb == 0 { Ok(desktop) } else { desktop.BindToObject(pidl, None) }
    })();
    unsafe { CoTaskMemFree(Some(pidl as *const _)) };
    bound
}

/// The menu of `paths` (one, or several from different folders), each parsed on its own.
unsafe fn items_menu(hwnd: HWND, paths: &[PathBuf]) -> windows::core::Result<IContextMenu> {
    let mut pidls: Vec<*mut ITEMIDLIST> = Vec::with_capacity(paths.len());
    let parsed = paths.iter().try_for_each(|path| {
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        unsafe { SHParseDisplayName(&HSTRING::from(path.as_os_str()), None, &mut pidl, 0, None)? };
        pidls.push(pidl);
        Ok(())
    });
    // Separate from the parsing, so `?` cannot skip the CoTaskMemFree below.
    let menu = parsed.and_then(|()| unsafe {
        let mut parent: Option<IShellFolder> = None;
        let mut children: Vec<*const ITEMIDLIST> = Vec::with_capacity(pidls.len());
        for &pidl in &pidls {
            let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
            let folder: IShellFolder = SHBindToParent(pidl, Some(&mut child))?;
            parent.get_or_insert(folder);
            children.push(child as *const ITEMIDLIST);
        }
        let parent = parent.ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        parent.GetUIObjectOf::<IContextMenu>(hwnd, &children, None)
    });
    for pidl in pidls {
        unsafe { CoTaskMemFree(Some(pidl as *const _)) };
    }
    menu
}

/// The menu of empty space in `folder`'s listing.
unsafe fn background_menu(hwnd: HWND, folder: &Path) -> windows::core::Result<IContextMenu> {
    unsafe { bind_folder(folder)?.CreateViewObject::<IContextMenu>(hwnd) }
}

unsafe fn track(
    hwnd: HWND,
    hmenu: HMENU,
    menu: &IContextMenu,
    extra: &[(u32, &str)],
    sub: Option<ShellSubmenu<'_>>,
    at: Option<(i32, i32)>,
    can_rename: bool,
) -> windows::core::Result<MenuOutcome> {
    unsafe {
        menu.QueryContextMenu(hmenu, 0, FIRST_SHELL_ID, LAST_SHELL_ID, query_flags(can_rename)).ok()?;
        let mut ours = extra.len();
        for (position, (id, label)) in extra.iter().enumerate() {
            InsertMenuW(hmenu, position as u32, MF_BYPOSITION | MF_STRING, *id as usize, &HSTRING::from(*label))?;
        }
        if let Some(sub) = sub.filter(|sub| !sub.items.is_empty()) {
            // Once in `hmenu`, destroyed with it (DestroyMenu takes its submenus along).
            let popup = CreatePopupMenu()?;
            let filled = sub.items.iter().try_for_each(|(id, label, enabled)| {
                let flags = if *enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
                AppendMenuW(popup, flags, *id as usize, &HSTRING::from(*label))
            });
            let at = sub.at.min(extra.len()) as u32;
            let inserted = filled.and_then(|()| {
                InsertMenuW(hmenu, at, MF_BYPOSITION | MF_POPUP, popup.0 as usize, &HSTRING::from(sub.title))
            });
            if let Err(err) = inserted {
                let _ = DestroyMenu(popup);
                return Err(err);
            }
            ours += 1;
        }
        if ours > 0 {
            InsertMenuW(hmenu, ours as u32, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null())?;
        }

        let mut cursor = POINT::default();
        match at {
            Some((x, y)) => {
                cursor = POINT { x, y };
                let _ = ClientToScreen(hwnd, &mut cursor);
            }
            None => GetCursorPos(&mut cursor)?,
        }
        ACTIVE.with(|active| *active.borrow_mut() = Some(menu.clone()));
        // Stays installed until the chosen command has run too, so Shell confirmation dialogs
        // shown by InvokeCommand are covered; the guard removes it on every path.
        let _subclass = Subclass::install(hwnd);
        let chosen =
            TrackPopupMenuEx(hmenu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, cursor.x, cursor.y, hwnd, None).0 as u32;
        ACTIVE.with(|active| *active.borrow_mut() = None);

        Ok(match chosen {
            0 => MenuOutcome::Dismissed,
            id if id < FIRST_SHELL_ID => MenuOutcome::Gezik(id),
            id => {
                if let Some(verb) = gezik_verb(menu, id - FIRST_SHELL_ID) {
                    return Ok(MenuOutcome::Verb(verb));
                }
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

/// `forward_menu_messages` installed on the window for as long as this lives.
struct Subclass {
    hwnd: HWND,
    installed: bool,
}

impl Subclass {
    unsafe fn install(hwnd: HWND) -> Self {
        let installed = unsafe { SetWindowSubclass(hwnd, Some(forward_menu_messages), SUBCLASS_ID, 0) }.as_bool();
        if !installed {
            eprintln!("gezik: SetWindowSubclass failed; submenus and repaint suppression may not work");
        }
        Subclass { hwnd, installed }
    }
}

impl Drop for Subclass {
    fn drop(&mut self) {
        if self.installed {
            let _ = unsafe { RemoveWindowSubclass(self.hwnd, Some(forward_menu_messages), SUBCLASS_ID) };
        }
    }
}

/// The command at `offset` if Gezik does it itself (cut, copy, paste, delete, rename).
unsafe fn gezik_verb(menu: &IContextMenu, offset: u32) -> Option<ShellVerb> {
    let mut verb = [0u8; 64];
    unsafe { menu.GetCommandString(offset as usize, GCS_VERBA, None, PSTR(verb.as_mut_ptr()), verb.len() as u32) }
        .ok()?;
    let end = verb.iter().position(|&b| b == 0).unwrap_or(verb.len());
    ShellVerb::from_name(&verb[..end])
}

/// Forwards owner-draw and submenu messages ("Send to", "Open with") while the menu is open,
/// and swallows WM_PAINT while the menu or a Shell dialog runs its modal loop.
///
/// winit (0.30) cannot run its handler during that modal loop; for each WM_PAINT it sees then,
/// it calls `RedrawWindow(RDW_INTERNALPAINT)`, which posts another WM_PAINT, so the UI thread
/// would spin at 100%. Validating the window here keeps the paint away from winit.
unsafe extern "system" fn forward_menu_messages(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        unsafe {
            BeginPaint(hwnd, &mut paint);
            let _ = EndPaint(hwnd, &paint);
        }
        return LRESULT(0);
    }
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
    Ok(menu_verbs(target, false)?.len() as i32)
}

/// The verbs Gezik does itself found in the top level of the Shell menu for `target`, one
/// entry per menu item (`None` for the others); for tests, nothing is shown.
#[cfg(test)]
fn menu_verbs(target: &MenuTarget, can_rename: bool) -> windows::core::Result<Vec<Option<ShellVerb>>> {
    use windows::Win32::UI::WindowsAndMessaging::{GetMenuItemCount, GetMenuItemID};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let menu = context_menu_for(HWND::default(), target)?;
        let hmenu = CreatePopupMenu()?;
        menu.QueryContextMenu(hmenu, 0, FIRST_SHELL_ID, LAST_SHELL_ID, query_flags(can_rename)).ok()?;
        let count = GetMenuItemCount(Some(hmenu));
        let verbs = (0..count.max(0))
            .map(|position| {
                let id = GetMenuItemID(hmenu, position);
                // Separators and submenus give 0 or u32::MAX.
                if (FIRST_SHELL_ID..=LAST_SHELL_ID).contains(&id) {
                    gezik_verb(&menu, id - FIRST_SHELL_ID)
                } else {
                    None
                }
            })
            .collect();
        let _ = DestroyMenu(hmenu);
        Ok(verbs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_menus_have_entries() {
        let dir = std::env::temp_dir().join(format!("gezik-shell-menu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let sub = dir.join("sub");
        std::fs::create_dir(&sub).unwrap();

        let file_items = count_items(&crate::MenuTarget::Item(file));
        let folder_items = count_items(&crate::MenuTarget::Item(sub));
        let background = count_items(&crate::MenuTarget::Background(dir.clone()));
        let file2 = dir.join("b.txt");
        std::fs::write(&file2, "y").unwrap();
        let several = count_items(&crate::MenuTarget::Items(vec![dir.join("a.txt"), file2]));
        let _ = std::fs::remove_dir_all(&dir);

        let (file_items, folder_items, background) = (file_items.unwrap(), folder_items.unwrap(), background.unwrap());
        assert!(file_items > 3, "file menu had {file_items} entries");
        assert!(folder_items > 3, "folder menu had {folder_items} entries");
        assert!(background > 0, "background menu had {background} entries");
        let several = several.unwrap();
        assert!(several > 3, "multi-item menu had {several} entries");
    }

    #[test]
    fn rename_is_offered_only_when_asked() {
        let dir = std::env::temp_dir().join(format!("gezik-shell-menu-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();

        let with = menu_verbs(&crate::MenuTarget::Item(file.clone()), true);
        let without = menu_verbs(&crate::MenuTarget::Item(file), false);
        let _ = std::fs::remove_dir_all(&dir);

        let (with, without) = (with.unwrap(), without.unwrap());
        assert!(with.contains(&Some(ShellVerb::Rename)), "menu with CMF_CANRENAME: {with:?}");
        assert!(!without.contains(&Some(ShellVerb::Rename)), "menu without CMF_CANRENAME: {without:?}");
    }

    #[test]
    fn large_selections_open_quickly() {
        let dir = std::env::temp_dir().join(format!("gezik-shell-menu-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = (0..2000)
            .map(|i| {
                let path = dir.join(format!("f{i}.txt"));
                std::fs::write(&path, "x").unwrap();
                path
            })
            .collect();
        let started = std::time::Instant::now();
        let items = count_items(&crate::MenuTarget::Items(paths));
        let took = started.elapsed();
        let missing = count_items(&crate::MenuTarget::Items(vec![dir.join("f1.txt"), dir.join("gone.txt")]));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(items.unwrap() > 3);
        assert!(missing.is_err(), "a missing entry fails the menu");
        eprintln!("2000-item menu: {took:?}");
    }

    #[test]
    fn finds_the_shared_folder() {
        let a = PathBuf::from(r"C:\x\a.txt");
        let b = PathBuf::from(r"C:\x\b");
        let both = [a.clone(), b];
        let (parent, names) = shared_parent(&both).unwrap();
        assert_eq!(parent, Path::new(r"C:\x"));
        assert_eq!(names, [std::ffi::OsStr::new("a.txt"), std::ffi::OsStr::new("b")]);
        assert!(shared_parent(&[a, PathBuf::from(r"C:\y\c")]).is_none());
        assert!(shared_parent(&[PathBuf::from(r"C:\"), PathBuf::from(r"D:\")]).is_none());
        assert!(shared_parent(&[]).is_none());
    }

    #[test]
    fn extra_ids_must_be_below_the_shell_range() {
        assert!(validate_ids(&[]).is_ok());
        assert!(validate_ids(&[(1, "a"), (999, "b")]).is_ok());
        assert!(validate_ids(&[(0, "a")]).is_err());
        assert!(validate_ids(&[(1000, "a")]).is_err());
        assert!(validate_ids(&[(1, "a"), (1000, "b")]).is_err());
    }

    #[test]
    fn submenu_ids_are_checked_too() {
        let window = NoWindow;
        let target = MenuTarget::Background(PathBuf::from(r"C:\"));
        let items = [(700, "Resize", true), (1000, "Bad", false)];
        let sub = ShellSubmenu { title: "Commands", at: 0, items: &items };
        let err = show_shell_menu(&window, &target, &[(1, "a")], Some(sub), None, false).unwrap_err();
        assert!(err.contains("1..1000"), "{err}");
    }

    /// A window that is never reached: the ids are checked first.
    struct NoWindow;

    impl HasWindowHandle for NoWindow {
        fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }
}
