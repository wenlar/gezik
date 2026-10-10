//! macOS: settings of the app as a whole.

use objc2::MainThreadMarker;
use objc2_app_kit::NSWindow;

/// Turns off macOS's window tabs: Gezik has tabs of its own, and the system would add "Show
/// Tab Bar" and "Show All Tabs" to the View menu for its windows. Call it before the window
/// opens, on the main thread.
pub fn no_window_tabs() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
    }
}

/// Makes Gezik the active app. Un-minimizing a window and asking winit to focus it leaves the
/// app behind the one in front on macOS 14 and later (cooperative activation); asking the
/// app itself to activate brings it forward.
pub fn activate() {
    if let Some(mtm) = MainThreadMarker::new() {
        // Deprecated in macOS 14, but the newer `activate` is only a request the system may
        // turn down when the one asking (another Gezik process) is not in front itself.
        #[allow(deprecated)]
        objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}
