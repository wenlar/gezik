//! macOS's Spotlight for a search of every drive that asks for tags, a rating or a comment
//! (spec 12 §7.1-7.2): Finder's own answer, from the index macOS keeps anyway. The query text
//! is built here on every system (tested everywhere); `mdfind` runs on macOS only. Gezik checks
//! every answer again (name, type, size, date and all the meta terms), so a term Spotlight
//! cannot say (`tag:none`, `rating:0`) still holds.

use gezik_core::meta_filter::TagTerm;
use gezik_core::search::{Scope, SearchSpec};

/// `text` for a quoted Spotlight value: `"`, `\` and `*` (a wildcard there) escaped.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '"' | '\\' | '*') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Spotlight's query for `spec`: the terms it can say, joined by `&&` (`cd`: ignoring case and
/// accents, as Finder). `None` when Spotlight is not asked: not every drive, text in files,
/// duplicates, the flat view, or no term it can say.
pub fn query_text(spec: &SearchSpec) -> Option<String> {
    if spec.scope != Scope::AllDrives || !spec.content.is_empty() || spec.duplicates || spec.flat {
        return None;
    }
    let parts: Vec<String> = spec
        .meta_terms()
        .iter()
        .filter_map(|term| match term {
            TagTerm::Named(name) => Some(format!("kMDItemUserTags == \"{}\"cd", quoted(name))),
            TagTerm::Any => Some("kMDItemUserTags == \"*\"".to_owned()),
            TagTerm::MinRating(n) => Some(format!("kMDItemStarRating >= {n}")),
            TagTerm::Rated => Some("kMDItemStarRating >= 1".to_owned()),
            TagTerm::Comment(text) => Some(format!("kMDItemFinderComment == \"*{}*\"cd", quoted(text))),
            // No value is no attribute: Spotlight cannot find what lacks one.
            TagTerm::None | TagTerm::Unrated => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" && "))
}

/// Runs `text` through `mdfind` and sends what `query` keeps as one batch and a summary;
/// false (nothing sent) when `mdfind` cannot answer: the walk runs then.
#[cfg(target_os = "macos")]
pub fn search(
    text: &str,
    query: &crate::query::Query,
    cancel: &std::sync::atomic::AtomicBool,
    sink: &dyn Fn(crate::run::Event),
) -> bool {
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    use crate::run::{Event, Summary};
    let started = std::time::Instant::now();
    // shortcut: mdfind runs to its end before a cancel is seen (a tag's answer comes in well
    // under a second); spawn and kill it if a slow index shows up.
    let output = std::process::Command::new("/usr/bin/mdfind").arg("-0").arg(text).output();
    let Some(output) = output.ok().filter(|o| o.status.success()) else { return false };
    let mut found = Vec::new();
    let mut limit_reached = false;
    for raw in output.stdout.split(|b| *b == 0).filter(|raw| !raw.is_empty()) {
        if cancel.load(Ordering::Relaxed) {
            sink(Event::Done(Summary { cancelled: true, ..Summary::default() }));
            return true;
        }
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(raw));
        let (Some(name), Ok(meta)) = (path.file_name(), std::fs::symlink_metadata(&path)) else { continue };
        let name = name.to_string_lossy().into_owned();
        let is_dir = meta.is_dir();
        let size = if meta.is_file() { meta.len() } else { 0 };
        if !query.passes(&name, is_dir, size, meta.modified().ok()) || !query.passes_meta(|| path.clone()) {
            continue;
        }
        if found.len() >= query.max_results() {
            limit_reached = true;
            break;
        }
        let entry = gezik_core::Entry {
            name,
            is_dir,
            flags: 0,
            size,
            modified: meta.modified().ok(),
            created: meta.created().ok(),
        };
        found.push((path, entry, None));
    }
    let set = crate::results::ResultSet::collect(PathBuf::new(), false, found);
    let count = set.len();
    sink(Event::Batch(set.into_batch()));
    sink(Event::Done(Summary {
        found: count,
        limit_reached,
        elapsed: started.elapsed(),
        spotlight: true,
        ..Summary::default()
    }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn drives(pattern: &str) -> SearchSpec {
        SearchSpec { pattern: pattern.to_owned(), ..SearchSpec::new(Scope::AllDrives) }
    }

    #[test]
    fn tags_ratings_and_comments_become_spotlights_query() {
        assert_eq!(query_text(&drives("tag:Red")).as_deref(), Some("kMDItemUserTags == \"Red\"cd"));
        let mut spec = drives("*.jpg tag:\"Project X\" rating:4");
        spec.comment = "review".into();
        assert_eq!(
            query_text(&spec).as_deref(),
            Some(
                "kMDItemUserTags == \"Project X\"cd && kMDItemStarRating >= 4 && kMDItemFinderComment == \"*review*\"cd"
            ),
            "the name is Gezik's to check"
        );
        assert_eq!(
            query_text(&drives("tag:any rating:any")).as_deref(),
            Some("kMDItemUserTags == \"*\" && kMDItemStarRating >= 1")
        );
        assert_eq!(
            query_text(&drives("tag:none tag:Red")).as_deref(),
            Some("kMDItemUserTags == \"Red\"cd"),
            "what Spotlight cannot say Gezik checks"
        );
    }

    #[test]
    fn quotes_backslashes_and_stars_are_escaped() {
        let mut spec = drives("");
        spec.tags = vec![r#"a "b" \c*"#.to_owned()];
        assert_eq!(query_text(&spec).as_deref(), Some(r#"kMDItemUserTags == "a \"b\" \\c\*"cd"#));
    }

    #[test]
    fn spotlight_is_asked_only_for_every_drive_and_a_term_it_can_say() {
        assert_eq!(query_text(&drives("*.jpg")), None, "no meta term");
        assert_eq!(query_text(&drives("tag:none rating:0")), None, "nothing it can say");
        let folder = SearchSpec { pattern: "tag:Red".into(), ..SearchSpec::new(Scope::Folder(PathBuf::from("/w"))) };
        assert_eq!(query_text(&folder), None, "a folder: the walk (also where Spotlight indexes nothing)");
        let mut content = drives("tag:Red");
        content.content = "x".into();
        assert_eq!(query_text(&content), None);
        let mut regex = drives("tag:Red");
        regex.name_regex = true;
        assert_eq!(query_text(&regex), None, "a regular expression has no words");
    }
}
