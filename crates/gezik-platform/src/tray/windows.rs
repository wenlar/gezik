//! Windows: Shell_NotifyIconW (NOTIFYICON_VERSION_4) on a thread of its own, with a hidden
//! top-level window (deviation 6: a message-only window would miss TaskbarCreated, and the
//! icon would not come back when Explorer restarts). Left click toggles, right click shows
//! the menu (deviation 8). The icon is the system's folder icon (deviation 7).

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    DefSubclassProc, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, SHGSI_ICON, SHGSI_SMALLICON, SHGetStockIconInfo, SHSTOCKICONINFO,
    SIID_FOLDER, SetWindowSubclass, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, EndMenu,
    GetMessageW, HICON, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterWindowMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_DESTROY, WM_NULL,
};
use windows::core::{PCWSTR, w};

use crate::tray::{Click, MenuLine, OnEvent, OnReady, TIP, TrayError, TrayEvent, event_for, menu, windows_click};

/// The icon's callback message.
const CALLBACK: u32 = WM_APP + 1;
/// Ends an open menu (Drop posts it before WM_CLOSE, so the thread is not held in TrackPopupMenu).
const END_MENU: u32 = WM_APP + 2;
const SUBCLASS_ID: usize = 1;

/// What the window's messages need; lives on the tray thread for as long as the window.
struct Shared {
    on_event: OnEvent,
    pins: Arc<Mutex<Vec<String>>>,
    taskbar_created: u32,
    icon: HICON,
}

pub struct Tray {
    hwnd: Arc<AtomicIsize>,
    stop: Arc<AtomicBool>,
    pins: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<()>>,
}

pub fn start(pins: Vec<String>, on_event: OnEvent, on_ready: OnReady) -> Tray {
    let hwnd = Arc::new(AtomicIsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let pins = Arc::new(Mutex::new(pins));
    let (window, halt, labels) = (hwnd.clone(), stop.clone(), pins.clone());
    // shortcut: if no thread can be started, on_ready is never called (as in hotkey/windows.rs).
    let thread = std::thread::Builder::new()
        .name("gezik-tray".into())
        .spawn(move || run(&window, &halt, labels, on_event, on_ready))
        .ok();
    Tray { hwnd, stop, pins, thread }
}

impl Tray {
    /// Tests: a left click without a mouse (the icon's callback, NIN_SELECT).
    #[cfg(test)]
    pub fn post_click(&self) {
        // SAFETY: a plain message to our window.
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd.load(SeqCst) as *mut _)), CALLBACK, WPARAM(0), LPARAM(0x400));
        }
    }

    pub fn set_pins(&self, pins: Vec<String>) {
        if let Ok(mut labels) = self.pins.lock() {
            *labels = pins;
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn icon_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: CALLBACK,
        hIcon: icon,
        ..Default::default()
    };
    for (slot, unit) in data.szTip.iter_mut().zip(TIP.encode_utf16()) {
        *slot = unit;
    }
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    data
}

fn add_icon(hwnd: HWND, icon: HICON) -> bool {
    let data = icon_data(hwnd, icon);
    // SAFETY: `data` is a filled NOTIFYICONDATAW of our window.
    unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() && Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool() }
}

fn run(hwnd_out: &AtomicIsize, stop: &AtomicBool, pins: Arc<Mutex<Vec<String>>>, on_event: OnEvent, on_ready: OnReady) {
    // SAFETY: a system class ("STATIC"); no parent and no WS_VISIBLE: a hidden top-level window.
    let created = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Gezik tray"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
    };
    let hwnd = match created {
        Ok(hwnd) => hwnd,
        Err(err) => return on_ready(Err(TrayError::Failed(err.message()))),
    };
    let mut info = SHSTOCKICONINFO { cbSize: size_of::<SHSTOCKICONINFO>() as u32, ..Default::default() };
    // SAFETY: `info` has its size set; the icon is destroyed below.
    let icon = unsafe { SHGetStockIconInfo(SIID_FOLDER, SHGSI_ICON | SHGSI_SMALLICON, &mut info) }
        .map(|()| info.hIcon)
        .unwrap_or_default();
    // SAFETY: a constant name.
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let shared = Box::new(Shared { on_event, pins, taskbar_created, icon });
    // SAFETY: `shared` outlives the window: the window is destroyed before this function ends.
    let subclassed =
        unsafe { SetWindowSubclass(hwnd, Some(messages), SUBCLASS_ID, &*shared as *const Shared as usize) }.as_bool();
    if !subclassed || !add_icon(hwnd, icon) {
        // SAFETY: our window and icon.
        unsafe {
            let _ = DestroyWindow(hwnd);
            let _ = DestroyIcon(icon);
        }
        return on_ready(Err(TrayError::Failed("the taskbar did not take the icon".to_owned())));
    }
    hwnd_out.store(hwnd.0 as isize, SeqCst);
    on_ready(Ok(()));
    if stop.load(SeqCst) {
        // SAFETY: our window; WM_DESTROY takes the icon away.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    } else {
        let mut msg = MSG::default();
        // SAFETY: this thread's own queue; WM_DESTROY posts WM_QUIT, GetMessageW then returns 0.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    // SAFETY: the stock icon's copy SHGetStockIconInfo made for us.
    unsafe {
        let _ = DestroyIcon(icon);
    }
    drop(shared);
}

unsafe extern "system" fn messages(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: `data` is the Shared that `run` keeps alive while the window is.
    let shared = unsafe { &*(data as *const Shared) };
    if msg == CALLBACK {
        match windows_click((lparam.0 as u32) & 0xFFFF) {
            Some(Click::Toggle) => (shared.on_event)(TrayEvent::Toggle),
            Some(Click::Menu) => {
                // NOTIFYICON_VERSION_4: where the menu goes, in wParam (signed screen points).
                let x = i32::from((wparam.0 & 0xFFFF) as u16 as i16);
                let y = i32::from(((wparam.0 >> 16) & 0xFFFF) as u16 as i16);
                if let Some(event) = show_menu(hwnd, shared, x, y) {
                    (shared.on_event)(event);
                }
            }
            None => {}
        }
        return LRESULT(0);
    }
    if msg != 0 && msg == shared.taskbar_created {
        // Explorer started again: the icon is put back.
        add_icon(hwnd, shared.icon);
        return LRESULT(0);
    }
    if msg == END_MENU {
        // SAFETY: ends the menu of this thread, if one is open.
        unsafe {
            let _ = EndMenu();
        }
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        let data = icon_data(hwnd, shared.icon);
        // SAFETY: our icon; the thread's loop ends with the WM_QUIT this posts.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            PostQuitMessage(0);
        }
    }
    // SAFETY: the default for every other message.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// The menu at (x, y); the chosen item's event.
fn show_menu(hwnd: HWND, shared: &Shared, x: i32, y: i32) -> Option<TrayEvent> {
    let pins = shared.pins.lock().map(|labels| labels.clone()).unwrap_or_default();
    // SAFETY: a menu made, shown and destroyed here, on the window's thread; the texts live
    // until the menu is gone.
    unsafe {
        let popup = CreatePopupMenu().ok()?;
        let lines = menu(&pins, true);
        let texts: Vec<Vec<u16>> = lines
            .iter()
            .map(|line| if let MenuLine::Item(_, label) = line { wide(label) } else { Vec::new() })
            .collect();
        for (line, text) in lines.iter().zip(&texts) {
            let _ = match line {
                MenuLine::Item(id, _) => AppendMenuW(popup, MF_STRING, *id as usize, PCWSTR(text.as_ptr())),
                MenuLine::Separator => AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null()),
            };
        }
        // The menu closes on a click elsewhere only if its window is in front (MS docs).
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(popup, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, x, y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(popup);
        event_for(chosen.0 as u32)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // The thread stores the window before it looks at `stop` (as hotkey/windows.rs).
        self.stop.store(true, SeqCst);
        let hwnd = self.hwnd.load(SeqCst);
        if hwnd != 0 {
            // SAFETY: a plain message to our window (fails harmlessly if it is gone).
            unsafe {
                let window = Some(HWND(hwnd as *mut _));
                let _ = PostMessageW(window, END_MENU, WPARAM(0), LPARAM(0));
                let _ = PostMessageW(window, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        // Dropped on its own thread (inside on_event): it ends by itself after WM_CLOSE.
        if let Some(thread) = self.thread.take().filter(|t| t.thread().id() != std::thread::current().id()) {
            let _ = thread.join();
        }
    }
}
