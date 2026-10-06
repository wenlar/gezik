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
