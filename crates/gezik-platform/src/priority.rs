//! Lower priority for the search's own threads (spec 3.4): copies, the list and typing come
//! first. Best effort: a refusal changes nothing else.

/// Lowers the calling thread's CPU and disk priority.
pub fn lower_this_thread() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        // SAFETY: the pseudo handle of this thread; background mode lowers its I/O too.
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
        }
    }
    #[cfg(target_os = "linux")]
    {
        // ioprio_set(IOPRIO_WHO_PROCESS, this thread, IOPRIO_CLASS_IDLE << 13), then nice 10.
        const IOPRIO_WHO_PROCESS: libc::c_long = 1;
        const IDLE: libc::c_long = 3 << 13;
        // SAFETY: plain system calls on this thread.
        unsafe {
            let _ = libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, 0 as libc::c_long, IDLE);
            let _ = libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as libc::id_t, 10);
        }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn setiopolicy_np(iotype: libc::c_int, scope: libc::c_int, policy: libc::c_int) -> libc::c_int;
        }
        const IOPOL_TYPE_DISK: libc::c_int = 0;
        const IOPOL_SCOPE_THREAD: libc::c_int = 1;
        const IOPOL_THROTTLE: libc::c_int = 3;
        // SAFETY: plain calls on this thread.
        unsafe {
            let _ = setiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_THREAD, IOPOL_THROTTLE);
            let _ = libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_thread_can_lower_itself() {
        std::thread::spawn(super::lower_this_thread).join().unwrap();
    }
}
