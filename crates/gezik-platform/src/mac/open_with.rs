//! macOS: the apps LaunchServices offers for a file, opening with one, and choosing another
//! (spec 9 §4.3).

use std::path::{Path, PathBuf};

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSError, NSFileManager, NSString, NSURL};

fn url_of(path: &Path) -> Option<Retained<NSURL>> {
    Some(NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?)))
}

fn path_of(url: &NSURL) -> Option<PathBuf> {
    url.path().map(|p| PathBuf::from(p.to_string()))
}

/// `path`'s default app and every app that opens it (macOS 12+).
pub fn apps_of(path: &Path) -> (Option<PathBuf>, Vec<PathBuf>) {
    autoreleasepool(|_| {
        let Some(url) = url_of(path) else { return (None, Vec::new()) };
        let workspace = NSWorkspace::sharedWorkspace();
        let default = workspace.URLForApplicationToOpenURL(&url).and_then(|u| path_of(&u));
        let all = workspace.URLsForApplicationsToOpenURL(&url).iter().filter_map(|u| path_of(&u)).collect();
        (default, all)
    })
}

/// Finder's name for an app ("Preview"), without `.app`.
pub fn app_name(app: &Path) -> String {
    let shown = app.to_str().map(|text| {
        autoreleasepool(|_| NSFileManager::defaultManager().displayNameAtPath(&NSString::from_str(text)).to_string())
    });
    let shown = shown.filter(|s| !s.is_empty()).unwrap_or_else(|| app.to_string_lossy().into_owned());
    shown.strip_suffix(".app").map(str::to_owned).unwrap_or(shown)
}

pub fn open(paths: &[PathBuf], app: &Path, on_error: Box<dyn FnOnce(String) + Send>) -> Result<(), String> {
    autoreleasepool(|_| {
        let app_url = url_of(app).ok_or("The app's path is not valid UTF-8")?;
        let urls: Vec<Retained<NSURL>> = paths.iter().filter_map(|p| url_of(p)).collect();
        if urls.is_empty() {
            return Err("No item can be opened".to_owned());
        }
        // The block may be called once, on any queue: the callback is taken out of a cell.
        let on_error = std::sync::Mutex::new(Some(on_error));
        let done = RcBlock::new(move |_app: *mut NSRunningApplication, error: *mut NSError| {
            // SAFETY: AppKit passes a valid NSError or null.
            if let Some(error) = unsafe { error.as_ref() }
                && let Some(report) = on_error.lock().ok().and_then(|mut f| f.take())
            {
                report(error.localizedDescription().to_string());
            }
        });
        NSWorkspace::sharedWorkspace().openURLs_withApplicationAtURL_configuration_completionHandler(
            &NSArray::from_retained_slice(&urls),
            &app_url,
            &NSWorkspaceOpenConfiguration::configuration(),
            Some(&done),
        );
        Ok(())
    })
}

#[allow(deprecated, reason = "setAllowedContentTypes needs objc2-uniform-type-identifiers 0.3")]
pub fn choose_app() -> Option<PathBuf> {
    let mtm = MainThreadMarker::new()?;
    autoreleasepool(|_| {
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str("/Applications"))));
        panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[NSString::from_str("app")])));
        (panel.runModal() == NSModalResponseOK).then(|| panel.URL()).flatten().and_then(|u| path_of(&u))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_files_open_in_textedit() {
        let file = std::env::temp_dir().join(format!("gezik-open-with-{}.txt", std::process::id()));
        std::fs::write(&file, "a").unwrap();
        let (default, all) = apps_of(&file);
        assert!(default.as_ref().is_some_and(|d| all.contains(d)), "{default:?} in {all:?}");
        assert!(all.iter().any(|a| app_name(a) == "TextEdit"), "{all:?}");
        let _ = std::fs::remove_file(&file);
    }
}
