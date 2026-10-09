//! Quick Actions ▸ (spec 9 §4.3, §17 decision 26): the Services and Quick Actions installed as
//! bundles that take files, offered when every selected item's type fits. What fits is decided
//! here (tested on every system); macOS reads the bundles and runs them (`mac::services`).

use std::path::PathBuf;

/// Most Quick Actions listed (`QUICK_ACTION_FIRST` 1750-1779).
pub const MAX_SERVICES: usize = 30;

/// A Service or Quick Action that takes files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// Its menu title (`NSMenuItem.default`), also the name `NSPerformService` runs it by.
    pub title: String,
    /// The types it takes (UTIs); a file fits when its type conforms to one.
    pub file_types: Vec<String>,
}

/// Pasteboard types that carry file paths: a service that sends them takes any file.
const FILE_PASTEBOARD_TYPES: &[&str] = &["NSFilenamesPboardType", "public.file-url", "NSURLPboardType"];

/// The types a service takes, from its `NSSendFileTypes`, else any item when its `NSSendTypes`
/// carry file paths; empty when it takes no files.
pub fn file_types(send_file_types: Vec<String>, send_types: &[String]) -> Vec<String> {
    if !send_file_types.is_empty() {
        send_file_types
    } else if send_types.iter().any(|t| FILE_PASTEBOARD_TYPES.contains(&t.as_str())) {
        vec!["public.item".to_owned()]
    } else {
        Vec::new()
    }
}

/// The services every item (by its type) fits, by title, each title once, at most `MAX_SERVICES`.
pub fn offered(services: Vec<Service>, item_types: &[String], conforms: impl Fn(&str, &str) -> bool) -> Vec<Service> {
    if item_types.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Service> = services
        .into_iter()
        .filter(|s| item_types.iter().all(|item| s.file_types.iter().any(|to| conforms(item, to))))
        .collect();
    out.sort_by_cached_key(|s| s.title.to_lowercase());
    out.dedup_by(|a, b| a.title == b.title);
    out.truncate(MAX_SERVICES);
    out
}

/// The services offered for `items`. Any thread; empty off macOS.
pub fn for_items(items: &[PathBuf]) -> Vec<Service> {
    #[cfg(target_os = "macos")]
    {
        let types: Option<Vec<String>> = items.iter().map(|item| crate::mac::services::type_of(item)).collect();
        // shortcut: an item whose type cannot be read hides every Quick Action; fine for local files.
        let Some(types) = types else { return Vec::new() };
        offered(crate::mac::services::installed(), &types, crate::mac::services::conforms)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = items;
        Vec::new()
    }
}

/// Runs the service titled `title` on `paths`. Main thread.
pub fn perform(title: &str, paths: &[PathBuf]) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::mac::services::perform(title, paths);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, paths);
        Err("Quick Actions are on macOS only".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small conformance table: image types are images, everything is an item.
    fn conforms(item: &str, to: &str) -> bool {
        item == to || to == "public.item" || (to == "public.image" && ["public.png", "public.jpeg"].contains(&item))
    }

    fn service(title: &str, types: &[&str]) -> Service {
        Service { title: title.to_owned(), file_types: types.iter().map(|t| (*t).to_owned()).collect() }
    }

    #[test]
    fn service_file_types_come_from_either_key() {
        let s = |v: &[&str]| v.iter().map(|t| (*t).to_owned()).collect::<Vec<String>>();
        assert_eq!(file_types(s(&["public.image"]), &[]), ["public.image"]);
        assert_eq!(file_types(Vec::new(), &s(&["NSFilenamesPboardType"])), ["public.item"]);
        assert_eq!(file_types(Vec::new(), &s(&["public.file-url"])), ["public.item"]);
        assert!(file_types(Vec::new(), &s(&["NSStringPboardType"])).is_empty(), "text only: takes no files");
    }

    #[test]
    fn quick_actions_match_every_selected_item() {
        let all = vec![
            service("Resize Images", &["public.image"]),
            service("Encode Folder", &["public.folder"]),
            service("Upload", &["public.item"]),
            service("Upload", &["public.item"]),
            service("Text Only", &[]),
        ];
        let titles = |items: &[&str]| -> Vec<String> {
            let types: Vec<String> = items.iter().map(|t| (*t).to_owned()).collect();
            offered(all.clone(), &types, conforms).into_iter().map(|s| s.title).collect()
        };
        assert_eq!(titles(&["public.png"]), ["Resize Images", "Upload"], "by title, once each");
        assert_eq!(titles(&["com.adobe.pdf"]), ["Upload"], "not an image");
        assert_eq!(titles(&["public.png", "com.adobe.pdf"]), ["Upload"], "every item must fit");
        assert_eq!(titles(&["public.folder"]), ["Encode Folder", "Upload"]);
        assert!(titles(&[]).is_empty(), "no items: nothing");
        let many: Vec<Service> = (0..40).map(|i| service(&format!("S{i:02}"), &["public.item"])).collect();
        assert_eq!(offered(many, &["x".to_owned()], conforms).len(), MAX_SERVICES);
    }
}
