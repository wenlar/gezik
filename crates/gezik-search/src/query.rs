//! A search compiled once (spec 3.1-3.3): the name matcher, the kind, the size, the time
//! window, the tags, rating and comment (spec 12 §7.2) and the content matcher, checked from
//! the cheapest on.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use gezik_core::Entry;
use gezik_core::meta::Meta;
use gezik_core::meta_filter::{TagTerm, matches};
use gezik_core::search::{Day, KindFilter, SearchSpec, SizeRange, TimeWindow, day_of, utc_offset};
use gezik_platform::fs::DirItem;

use crate::content::ContentMatcher;
use crate::name::NameMatcher;

/// What a search counts from.
#[derive(Debug, Clone, Copy)]
pub struct QueryOptions {
    pub now: SystemTime,
    pub today: Day,
    /// Local minus UTC, in seconds.
    pub utc_offset: i64,
    /// `[search] content-max-size`.
    pub content_max_size: u64,
    /// `[search] max-results`.
    pub max_results: usize,
}

impl QueryOptions {
    /// Now, on this computer's clock.
    pub fn local(content_max_size: u64, max_results: usize) -> QueryOptions {
        let now = SystemTime::now();
        let local = gezik_platform::local_date_parts(now).unwrap_or_default();
        QueryOptions { now, today: day_of(&local), utc_offset: utc_offset(now, &local), content_max_size, max_results }
    }
}

/// Which field's text cannot be used, and why (shown under it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    Name(String),
    Content(String),
}

/// Reads an item's tags, rating and comment: the app's reader (the file's own record and
/// Gezik's store, `gezik_ops::meta::MetaStore::read`).
#[derive(Clone)]
pub struct MetaReader(pub Arc<dyn Fn(&Path) -> Meta + Send + Sync>);

impl std::fmt::Debug for MetaReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MetaReader")
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    name: NameMatcher,
    kind: KindFilter,
    size: SizeRange,
    window: TimeWindow,
    /// Every one must hold; read only for an item the rest let through.
    meta: Vec<TagTerm>,
    reader: Option<MetaReader>,
    content: Option<ContentMatcher>,
    flat: bool,
    max_results: usize,
}

impl Query {
    pub fn compile(spec: &SearchSpec, options: &QueryOptions) -> Result<Query, QueryError> {
        let name =
            NameMatcher::compile(&spec.name_pattern(), spec.name_regex, spec.match_case).map_err(QueryError::Name)?;
        let content = (!spec.content.is_empty())
            .then(|| {
                ContentMatcher::compile(&spec.content, spec.content_regex, spec.match_case, options.content_max_size)
            })
            .transpose()
            .map_err(QueryError::Content)?
            .filter(|content| !content.matches_empty());
        Ok(Query {
            name,
            kind: spec.kind,
            size: spec.size,
            window: spec.modified.window(options.now, options.today, options.utc_offset),
            meta: spec.meta_terms(),
            reader: None,
            content,
            flat: spec.flat,
            max_results: options.max_results,
        })
    }

    /// The cheap part, from the name alone: the flat view and a content search take files
    /// only; the kind; the name matcher.
    pub fn passes_name(&self, name: &str, is_dir: bool) -> bool {
        if is_dir && (self.flat || self.content.is_some()) {
            return false;
        }
        self.kind.matches(name, is_dir) && self.name.matches(name)
    }

    /// Everything but the content. A folder has no size: a size criterion leaves folders out.
    pub fn passes(&self, name: &str, is_dir: bool, size: u64, modified: Option<SystemTime>) -> bool {
        self.passes_name(name, is_dir)
            && (self.size.is_any() || (!is_dir && self.size.contains(size)))
            && self.window.contains(modified)
    }

    /// Reads the items' meta with `reader` (without one, every item has none).
    pub fn read_meta_with(&mut self, reader: MetaReader) {
        self.reader = Some(reader);
    }

    /// Whether it asks for tags, a rating or a comment.
    pub fn has_meta(&self) -> bool {
        !self.meta.is_empty()
    }

    /// The tags, rating and comment of the item at `path` (asked only when there are such
    /// criteria): last of the cheap checks, before the content.
    pub fn passes_meta(&self, path: impl FnOnce() -> PathBuf) -> bool {
        if self.meta.is_empty() {
            return true;
        }
        let meta = self.reader.as_ref().map_or_else(Meta::default, |read| (read.0)(&path()));
        matches(&self.meta, &meta)
    }

    /// The tag, rating and comment criteria (Spotlight asks for them).
    pub fn meta_terms(&self) -> &[TagTerm] {
        &self.meta
    }

    pub fn content(&self) -> Option<&ContentMatcher> {
        self.content.as_ref()
    }

    pub fn max_results(&self) -> usize {
        self.max_results
    }
}

/// A folder item as a list entry.
pub fn entry_of(item: &DirItem) -> Entry {
    Entry {
        name: item.name.clone(),
        is_dir: item.is_dir,
        flags: item.flags,
        size: item.size,
        modified: item.modified,
        created: item.created,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::search::{KindFilter, Scope, SearchSpec};

    fn options() -> QueryOptions {
        QueryOptions {
            now: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000),
            today: Day { year: 2027, month: 1, day: 15 },
            utc_offset: 0,
            content_max_size: 1000,
            max_results: 10,
        }
    }

    fn spec() -> SearchSpec {
        SearchSpec::new(Scope::Folder("/w".into()))
    }

    #[test]
    fn names_kinds_sizes_and_dates_all_have_to_pass() {
        let mut s = spec();
        s.pattern = "*.pdf".into();
        s.size.min = Some(10);
        s.modified = gezik_core::search::DateRange::LastDays(7);
        let q = Query::compile(&s, &options()).unwrap();
        let recent = Some(options().now - std::time::Duration::from_secs(3600));
        assert!(q.passes("a.pdf", false, 10, recent));
        assert!(!q.passes("a.txt", false, 10, recent), "the name");
        assert!(!q.passes("a.pdf", false, 9, recent), "the size");
        assert!(!q.passes("a.pdf", false, 10, Some(SystemTime::UNIX_EPOCH)), "the date");
        assert!(!q.passes("x.pdf", true, 10, recent), "a size leaves folders out");
    }

    #[test]
    fn the_flat_view_and_content_take_files_only() {
        let q = Query::compile(&SearchSpec::flat_view("/w".into()), &options()).unwrap();
        assert!(q.passes("a", false, 0, None) && !q.passes("a", true, 0, None));
        let mut s = spec();
        s.content = "x".into();
        let q = Query::compile(&s, &options()).unwrap();
        assert!(!q.passes_name("folder", true) && q.content().is_some());
        s.content = "x*".into();
        s.content_regex = true;
        let q = Query::compile(&s, &options()).unwrap();
        assert!(
            q.content().is_none() && q.passes_name("folder", true),
            "a regex that matches an empty text is no criterion"
        );
        let mut folders = spec();
        folders.kind = KindFilter::Folders;
        assert!(Query::compile(&folders, &options()).unwrap().passes("d", true, 0, None));
    }

    #[test]
    fn meta_is_read_last_and_only_when_asked_for() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let reads = Arc::new(AtomicUsize::new(0));
        let counted = reads.clone();
        let reader = MetaReader(Arc::new(move |path: &Path| {
            counted.fetch_add(1, Ordering::SeqCst);
            let red = path.ends_with("red.jpg");
            Meta {
                tags: if red { vec![gezik_core::meta::Tag::new("Red", Default::default())] } else { vec![] },
                ..Meta::default()
            }
        }));
        let mut s = spec();
        s.pattern = "*.jpg".into();
        let mut plain = Query::compile(&s, &options()).unwrap();
        plain.read_meta_with(reader.clone());
        assert!(plain.passes_meta(|| "/w/a.jpg".into()) && !plain.has_meta());
        assert_eq!(reads.load(Ordering::SeqCst), 0, "no criterion: nothing read");
        s.pattern = "*.jpg tag:red".into();
        let mut q = Query::compile(&s, &options()).unwrap();
        q.read_meta_with(reader);
        assert!(q.has_meta() && q.passes("x.jpg", false, 0, None), "the word is no name");
        assert!(q.passes_meta(|| "/w/red.jpg".into()) && !q.passes_meta(|| "/w/blue.jpg".into()));
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        let mut unread = spec();
        unread.tags = vec!["Red".into()];
        let q = Query::compile(&unread, &options()).unwrap();
        assert!(!q.passes_meta(|| "/w/red.jpg".into()), "no reader: no tags");
    }

    #[test]
    fn errors_say_which_field() {
        let mut s = spec();
        s.pattern = "!".into();
        assert_eq!(Query::compile(&s, &options()).err(), Some(QueryError::Name("Type a name after \"!\"".into())));
        let mut s = spec();
        s.content = "(".into();
        s.content_regex = true;
        assert!(matches!(Query::compile(&s, &options()), Err(QueryError::Content(_))));
        assert_eq!(Query::compile(&spec(), &options()).unwrap().max_results(), 10);
    }
}
