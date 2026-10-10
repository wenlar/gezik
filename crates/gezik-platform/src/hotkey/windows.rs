//! Windows: `RegisterHotKey` on a thread of its own, with no window (WM_HOTKEY goes to the
//! thread's queue; deviation 6); dropped, WM_QUIT ends the thread and the key is released.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
};

use crate::hotkey::{Combo, HotkeyError, OnPress, OnReady, windows_modifiers, windows_vk};

const ID: i32 = 1;

pub struct Hotkey {
    thread_id: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub fn register(combo: Combo, on_press: OnPress, on_ready: OnReady) -> Hotkey {
    let thread_id = Arc::new(AtomicU32::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (id, halt) = (thread_id.clone(), stop.clone());
    // shortcut: if no thread can be started, on_ready is never called and the row stays
    // "turning on"; report it once a system shows that happens.
    let thread = std::thread::Builder::new()
        .name("gezik-hotkey".into())
        .spawn(move || run(&id, &halt, combo, on_press, on_ready))
        .ok();
    Hotkey { thread_id, stop, thread }
}

fn run(thread_id: &AtomicU32, stop: &AtomicBool, combo: Combo, on_press: OnPress, on_ready: OnReady) {
    let mut msg = MSG::default();
    // SAFETY: makes this thread's queue before its id is handed out, so WM_QUIT can be posted.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        thread_id.store(GetCurrentThreadId(), SeqCst);
    }
    // SAFETY: a hotkey of this thread's queue (no window), released below.
    let registered =
        unsafe { RegisterHotKey(None, ID, HOT_KEY_MODIFIERS(windows_modifiers(&combo)), windows_vk(combo.key)) };
    if let Err(err) = registered {
        let taken = err.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult();
        return on_ready(Err(if taken { HotkeyError::Taken } else { HotkeyError::Failed(err.message()) }));
    }
    on_ready(Ok(()));
    if !stop.load(SeqCst) {
        // SAFETY: this thread's own queue; it ends with WM_QUIT (GetMessageW returns 0).
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
            if msg.message == WM_HOTKEY && msg.wParam.0 == ID as usize {
                on_press();
            }
        }
    }
    // SAFETY: the hotkey this thread registered.
    unsafe {
        let _ = UnregisterHotKey(None, ID);
    }
}

impl Hotkey {
    /// Tests: a press without a key (WM_HOTKEY on the thread's queue).
    #[cfg(test)]
    pub fn post_press(&self) {
        // SAFETY: a plain message to our own thread.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id.load(SeqCst), WM_HOTKEY, WPARAM(ID as usize), LPARAM(0));
        }
    }
}

/// The thread WM_QUIT goes to: only one that is still ours (a finished thread's id may
/// already be another thread's).
fn quit_to(id: u32, running: bool) -> Option<u32> {
    (id != 0 && running).then_some(id)
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        // The thread stores its id before it looks at `stop`: either it sees `stop`, or WM_QUIT
        // is posted to a queue that exists.
        self.stop.store(true, SeqCst);
        let running = self.thread.as_ref().map(JoinHandle::is_finished) == Some(false);
        if let Some(id) = quit_to(self.thread_id.load(SeqCst), running) {
            // SAFETY: a plain message to the hotkey thread (fails harmlessly if it has ended).
            unsafe {
                let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        // Dropped on its own thread (inside on_press): it ends by itself after WM_QUIT.
        if let Some(thread) = self.thread.take().filter(|t| t.thread().id() != std::thread::current().id()) {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn quit_goes_only_to_a_thread_still_running() {
        assert_eq!(super::quit_to(42, true), Some(42));
        assert_eq!(super::quit_to(42, false), None, "its id may be another thread's now");
        assert_eq!(super::quit_to(0, true), None, "no queue yet: it sees stop");
    }
}
