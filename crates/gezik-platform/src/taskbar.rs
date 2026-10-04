//! Windows: the taskbar button shows the progress of all running file operations. Elsewhere
//! this does nothing.

use raw_window_handle::HasWindowHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarState {
    Off,
    Normal,
    Paused,
    Error,
}

pub struct Taskbar {
    #[cfg(windows)]
    inner: Option<(windows::Win32::UI::Shell::ITaskbarList3, windows::Win32::Foundation::HWND)>,
}

impl Taskbar {
    /// Call on the UI thread once the window is shown.
    pub fn new(window: &impl HasWindowHandle) -> Taskbar {
        #[cfg(windows)]
        {
            use raw_window_handle::RawWindowHandle;
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
            use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
            let hwnd = match window.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(win32)) => HWND(win32.hwnd.get() as *mut _),
                _ => return Taskbar { inner: None },
            };
            let list: Option<ITaskbarList3> = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) }
                .ok()
                .filter(|list: &ITaskbarList3| unsafe { list.HrInit() }.is_ok());
            Taskbar { inner: list.map(|list| (list, hwnd)) }
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            Taskbar {}
        }
    }

    /// `done` of `total` (any unit); `Off` removes the bar.
    pub fn set(&self, state: TaskbarState, done: u64, total: u64) {
        #[cfg(windows)]
        if let Some((list, hwnd)) = &self.inner {
            use windows::Win32::UI::Shell::{TBPF_ERROR, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED};
            let flag = match state {
                TaskbarState::Off => TBPF_NOPROGRESS,
                TaskbarState::Normal => TBPF_NORMAL,
                TaskbarState::Paused => TBPF_PAUSED,
                TaskbarState::Error => TBPF_ERROR,
            };
            unsafe {
                let _ = list.SetProgressState(*hwnd, flag);
                if state != TaskbarState::Off {
                    let _ = list.SetProgressValue(*hwnd, done.min(total), total.max(1));
                }
            }
        }
        #[cfg(not(windows))]
        let _ = (state, done, total);
    }
}
