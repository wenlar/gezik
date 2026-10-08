//! Everything (spec 3.6): its IPC messages (the version 2 query of `everything_ipc.h`), a
//! search in its syntax, and a search answered by it. Gezik's hidden and skip rules and its
//! own matchers check every answer, so Everything and the walk give the same list (but for
//! Everything's own exclusions). Content is never Everything's: its candidates are read by
//! Gezik's content matcher. Any failure falls back to the walk; nothing is shown as an error.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use gezik_core::Entry;
use gezik_core::pattern::fold_text;
use gezik_core::search::{DateRange, KindFilter, Scope, SearchSpec};
use gezik_platform::everything::{self as ipc, EverythingError};

use crate::content::Found;
use crate::query::Query;
use crate::results::{Batch, ResultSet, folder_text};
use crate::run::{BATCH_ITEMS, Event, Running, Summary, send_whole, start_with};
use crate::walk::{Walk, WalkRules, threads_for};

pub const SEARCH_MATCH_CASE: u32 = 0x0000_0001;
pub const REQUEST_NAME: u32 = 0x0000_0001;
pub const REQUEST_PATH: u32 = 0x0000_0002;
pub const REQUEST_FULL_PATH_AND_NAME: u32 = 0x0000_0004;
pub const REQUEST_EXTENSION: u32 = 0x0000_0008;
pub const REQUEST_SIZE: u32 = 0x0000_0010;
pub const REQUEST_DATE_CREATED: u32 = 0x0000_0020;
pub const REQUEST_DATE_MODIFIED: u32 = 0x0000_0040;
pub const REQUEST_DATE_ACCESSED: u32 = 0x0000_0080;
pub const REQUEST_ATTRIBUTES: u32 = 0x0000_0100;
pub const ITEM_FOLDER: u32 = 0x1;
pub const SORT_NAME_ASCENDING: u32 = 1;
/// What a search asks for: the full path, the size, both dates and the attributes.
pub const REQUEST: u32 =
    REQUEST_FULL_PATH_AND_NAME | REQUEST_SIZE | REQUEST_DATE_CREATED | REQUEST_DATE_MODIFIED | REQUEST_ATTRIBUTES;
/// How long the database check and the scope's probe may take (spec 3.6).
const QUICK: Duration = Duration::from_secs(1);
/// How long the answer itself may take: it can carry 250,000 paths.
const ANSWER: Duration = Duration::from_secs(5);
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// One item of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub attributes: Option<u32>,
}

/// An `EVERYTHING_IPC_QUERY2` with its search text.
pub fn encode_query2(
    reply_window: u32,
    reply_id: u32,
    search: &str,
    flags: u32,
    max_results: u32,
    request: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + search.len() * 2 + 2);
    for word in [reply_window, reply_id, flags, 0, max_results, request, SORT_NAME_ASCENDING] {
        out.extend(word.to_le_bytes());
    }
    out.extend(search.encode_utf16().flat_map(u16::to_le_bytes));
    out.extend([0, 0]);
    out
}

fn word(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes.get(at..at + 4).ok_or("Everything's answer is cut short")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn quad(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from(word(bytes, at)?) | (u64::from(word(bytes, at + 4)?) << 32))
}

/// A counted UTF-16 text at `at` and where the next field starts.
fn text(bytes: &[u8], at: usize) -> Result<(String, usize), String> {
    let len = word(bytes, at)? as usize;
    let start = at + 4;
    let raw = bytes.get(start..start + len * 2).ok_or("Everything's answer is cut short")?;
    let (pairs, _) = raw.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
    Ok((String::from_utf16_lossy(&units), start + (len + 1) * 2))
}

fn file_time(ticks: u64) -> Option<SystemTime> {
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    if ticks == 0 || ticks == u64::MAX {
        return None;
    }
    let since = Duration::from_nanos(ticks.abs_diff(UNIX_EPOCH_TICKS).saturating_mul(100));
    Some(if ticks >= UNIX_EPOCH_TICKS { SystemTime::UNIX_EPOCH + since } else { SystemTime::UNIX_EPOCH - since })
}

/// The items of an `EVERYTHING_IPC_LIST2` answer (the fields its `request_flags` says it holds,
/// in bit order; past the attributes nothing is read).
pub fn decode_list2(bytes: &[u8]) -> Result<Vec<Item>, String> {
    let count = word(bytes, 4)? as usize;
    let request = word(bytes, 12)?;
    let mut items = Vec::with_capacity(count.min(1 << 20));
    for k in 0..count {
        let flags = word(bytes, 20 + k * 8)?;
        let mut at = word(bytes, 24 + k * 8)? as usize;
        let mut item = Item {
            path: String::new(),
            is_dir: flags & ITEM_FOLDER != 0,
            size: None,
            created: None,
            modified: None,
            attributes: None,
        };
        let (mut name, mut folder) = (String::new(), String::new());
        for bit in (0..9).map(|shift| 1u32 << shift) {
            if request & bit == 0 {
                continue;
            }
            match bit {
                REQUEST_NAME | REQUEST_PATH | REQUEST_FULL_PATH_AND_NAME | REQUEST_EXTENSION => {
                    let (value, next) = text(bytes, at)?;
                    match bit {
                        REQUEST_NAME => name = value,
                        REQUEST_PATH => folder = value,
                        REQUEST_FULL_PATH_AND_NAME => item.path = value,
                        _ => {}
                    }
                    at = next;
                }
                REQUEST_SIZE => {
                    item.size = Some(quad(bytes, at)?).filter(|size| *size != u64::MAX);
                    at += 8;
                }
                REQUEST_DATE_CREATED => {
                    item.created = file_time(quad(bytes, at)?);
                    at += 8;
                }
                REQUEST_DATE_MODIFIED => {
                    item.modified = file_time(quad(bytes, at)?);
                    at += 8;
                }
                REQUEST_DATE_ACCESSED => at += 8,
                REQUEST_ATTRIBUTES => {
                    item.attributes = Some(word(bytes, at)?);
                    at += 4;
                }
                _ => {}
            }
        }
        if item.path.is_empty() && !name.is_empty() {
            item.path = if folder.is_empty() { name } else { format!("{}\\{name}", folder.trim_end_matches('\\')) };
        }
        items.push(item);
    }
    Ok(items)
}

fn quote(text: &str) -> String {
    format!("\"{text}\"")
}

/// Quoted when it has a space or one of Everything's operators.
fn quote_if(text: &str) -> String {
    if text.contains([' ', '|', '<', '>', '!']) { quote(text) } else { text.to_owned() }
}

fn has_turkish_i(text: &str) -> bool {
    text.contains(['i', 'I', 'İ', 'ı'])
}

/// One part of the pattern language in Everything's syntax: as it is, or with `wfn:` (the whole
/// name) when it has wildcards; the Turkish i letters become `?` (Everything does not take İ
/// for i), and a part without wildcards is then `*…*`.
fn name_term(body: &str) -> String {
    let wild = body.contains(['*', '?']);
    if !wild && !has_turkish_i(body) {
        return quote_if(body);
    }
    let text: String = body.chars().map(|c| if matches!(c, 'i' | 'I' | 'İ' | 'ı') { '?' } else { c }).collect();
    let text = if wild { text } else { format!("*{text}*") };
    format!("wfn:{}", quote_if(&text))
}

/// Everything's search for `spec` under `roots` and its flags; `None` when it cannot ask for it
/// rightly (a regular expression whose case folding of the Turkish i matters, a quote in a
/// name, no scope): the walk runs then. Kind, size and date are narrowed only where
/// Everything's syntax is sure; Gezik's own check does the rest.
pub fn translate(spec: &SearchSpec, roots: &[PathBuf]) -> Option<(String, u32)> {
    let scopes: Vec<String> = roots
        .iter()
        .map(|root| {
            let mut text = root.display().to_string();
            if !text.ends_with('\\') {
                text.push('\\');
            }
            quote(&text)
        })
        .collect();
    let mut parts = match scopes.len() {
        0 => return None,
        1 => vec![scopes[0].clone()],
        _ => vec![format!("<{}>", scopes.join("|"))],
    };
    let mut flags = 0;
    if spec.name_regex {
        if !spec.pattern.is_empty() {
            if (!spec.match_case && has_turkish_i(&spec.pattern)) || spec.pattern.contains('"') {
                return None;
            }
            parts.push(format!("regex:{}", quote(&spec.pattern)));
            if spec.match_case {
                flags |= SEARCH_MATCH_CASE;
            }
        }
    } else {
        let mut includes = Vec::new();
        for part in spec.pattern.split(';').map(str::trim).filter(|part| !part.is_empty()) {
            let (leave_out, body) = match part.strip_prefix('!') {
                Some(rest) => (true, rest.trim_start()),
                None => (false, part),
            };
            if body.contains('"') {
                if leave_out {
                    continue;
                }
                return None;
            }
            if leave_out {
                if !has_turkish_i(body) {
                    parts.push(format!("!{}", name_term(body)));
                }
            } else {
                includes.push(name_term(body));
            }
        }
        match includes.len() {
            0 => {}
            1 => parts.push(includes.remove(0)),
            _ => parts.push(format!("<{}>", includes.join("|"))),
        }
    }
    let files_only =
        spec.flat || !spec.content.is_empty() || !matches!(spec.kind, KindFilter::Any | KindFilter::Folders);
    if files_only {
        parts.push("file:".to_owned());
    } else if spec.kind == KindFilter::Folders {
        parts.push("folder:".to_owned());
    }
    if let Some(min) = spec.size.min {
        parts.push(format!("size:>={min}"));
    }
    if let Some(max) = spec.size.max {
        parts.push(format!("size:<={max}"));
    }
    match spec.modified {
        DateRange::Today => parts.push("dm:today".to_owned()),
        DateRange::ThisYear => parts.push("dm:thisyear".to_owned()),
        DateRange::Any | DateRange::LastDays(_) | DateRange::Between(..) => {}
    }
    Some((parts.join(" "), flags))
}

/// `path` (Everything spells it as the disk does) under the first of `roots` it is in, with the
/// root spelled as the scope is (`C:\WINDOWS` for `C:\Windows`): the walk's paths and folder
/// texts start with the scope's spelling too. `None` outside every root.
fn rebased(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let root = roots.iter().find(|root| gezik_core::ops::paths::is_within(path, root))?;
    let depth = root.components().filter(|c| !matches!(c, Component::CurDir)).count();
    let mut out = root.clone();
    out.extend(path.components().filter(|c| !matches!(c, Component::CurDir)).skip(depth));
    Some(out)
}

/// Whether an answer's item stays by Gezik's rules (spec 3.6): under one of `roots`; no folder
/// between the root and it that `skip` names, that the view hides by a dot, or that is in
/// `hidden` (folded whole paths of hidden folders, read from the disk); the item itself shown
/// by the view (`flags`: `Entry::HIDDEN`, `SYSTEM`).
pub fn kept(path: &Path, roots: &[PathBuf], flags: u8, rules: &WalkRules, hidden: &HashSet<String>) -> bool {
    let Some(root) = roots.iter().find(|root| path.starts_with(root)) else { return false };
    let Ok(rest) = path.strip_prefix(root) else { return false };
    let parts: Vec<&std::ffi::OsStr> = rest.iter().collect();
    let Some((own, folders)) = parts.split_last() else { return false };
    let mut walked = root.clone();
    for folder in folders {
        let name = folder.to_string_lossy();
        walked.push(folder);
        if rules.skip.contains(&fold_text(&name)) {
            return false;
        }
        if let Some((show_hidden, _)) = rules.shown
            && ((!show_hidden && name.starts_with('.')) || hidden.contains(&fold_text(&walked.display().to_string())))
        {
            return false;
        }
    }
    match rules.shown {
        Some((show_hidden, show_system)) => {
            gezik_core::is_shown_name(&own.to_string_lossy(), flags, show_hidden, show_system)
        }
        None => true,
    }
}

/// `Entry::HIDDEN` and `SYSTEM` from file attributes.
fn flags_of(attributes: u32) -> u8 {
    let mut flags = 0;
    if attributes & FILE_ATTRIBUTE_HIDDEN != 0 {
        flags |= Entry::HIDDEN;
    }
    if attributes & FILE_ATTRIBUTE_SYSTEM != 0 {
        flags |= Entry::SYSTEM;
    }
    flags
}

/// A folder's `Entry::HIDDEN` and `SYSTEM`, from the disk (0 where it cannot be read).
fn folder_flags(path: &Path) -> u8 {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        std::fs::symlink_metadata(path).map_or(0, |meta| flags_of(meta.file_attributes()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        0
    }
}

/// The folders between a root and `paths` that the view (`shown`: `show-hidden`,
/// `show-system`) hides by their attributes, folded whole, for [`kept`]. Each folder is read
/// once; asking Everything for `attrib:H` instead takes seconds (attributes are not indexed).
fn hidden_folders<'a>(
    paths: impl Iterator<Item = &'a Path>,
    roots: &[PathBuf],
    (show_hidden, show_system): (bool, bool),
    cancel: &AtomicBool,
) -> HashSet<String> {
    let mut hidden = HashSet::new();
    if show_hidden && show_system {
        return hidden;
    }
    let mut seen: HashSet<&Path> = HashSet::new();
    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Some(root) = roots.iter().find(|root| path.starts_with(root)) else { continue };
        for folder in path.ancestors().skip(1) {
            if folder == root.as_path() || !folder.starts_with(root) || !seen.insert(folder) {
                break;
            }
            let Some(name) = folder.file_name() else { break };
            if !gezik_core::is_shown_name(&name.to_string_lossy(), folder_flags(folder), show_hidden, show_system) {
                hidden.insert(fold_text(&folder.display().to_string()));
            }
        }
    }
    hidden
}

/// Why Everything did not answer: the walk runs then (spec 3.6: never shown as an error).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    Off,
    /// Not a local fixed drive (a network folder, a USB stick), or not Windows.
    NotFixed,
    Untranslatable,
    NotRunning,
    NotReady,
    /// Everything has nothing under the scope (excluded, or a volume it does not index).
    NotIndexed,
    NoAnswer,
    Cancelled,
}

impl From<EverythingError> for Fallback {
    fn from(err: EverythingError) -> Fallback {
        match err {
            EverythingError::NotRunning => Fallback::NotRunning,
            EverythingError::NotReady => Fallback::NotReady,
            EverythingError::NoAnswer | EverythingError::Failed(_) => Fallback::NoAnswer,
        }
    }
}

/// Whether every root is on a local fixed drive (Windows only).
fn fixed(roots: &[PathBuf]) -> bool {
    if !cfg!(windows) || roots.is_empty() {
        return false;
    }
    let drives = gezik_platform::drives();
    roots.iter().all(|root| {
        let Some(drive) = gezik_platform::fs::drive_root(root) else { return false };
        drives
            .iter()
            .any(|d| d.kind == gezik_platform::DriveKind::Fixed && gezik_core::ops::paths::same_path(&d.path, &drive))
    })
}

fn ask(search: &str, flags: u32, max: u32, request: u32, timeout: Duration) -> Result<Vec<Item>, Fallback> {
    let bytes = ipc::query(&|window, id| encode_query2(window, id, search, flags, max, request), timeout)?;
    decode_list2(&bytes).map_err(|_| Fallback::NoAnswer)
}

/// Runs `spec` through Everything, sending batches and a summary to `sink` as the walk does;
/// `Err` (nothing sent) when Everything cannot answer it.
pub fn search(
    spec: &SearchSpec,
    walk: &Walk,
    query: &Query,
    on: bool,
    cancel: &AtomicBool,
    sink: &dyn Fn(Event),
) -> Result<(), Fallback> {
    let started = Instant::now();
    if !on {
        return Err(Fallback::Off);
    }
    if !fixed(&walk.roots) {
        return Err(Fallback::NotFixed);
    }
    let (search, flags) = translate(spec, &walk.roots).ok_or(Fallback::Untranslatable)?;
    ipc::ready(QUICK)?;
    for root in &walk.roots {
        let probe = translate(&SearchSpec::new(Scope::Folder(root.clone())), std::slice::from_ref(root))
            .ok_or(Fallback::Untranslatable)?
            .0;
        if ask(&probe, 0, 1, REQUEST_FULL_PATH_AND_NAME, QUICK)?.is_empty() {
            return Err(Fallback::NotIndexed);
        }
    }
    let max = u32::try_from(query.max_results().saturating_add(1)).unwrap_or(u32::MAX);
    let answer = ask(&search, flags, max, REQUEST, ANSWER)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(Fallback::Cancelled);
    }
    // The answer under the scope's spelling, then the folders above it that the view hides
    // (the walk does not go into them).
    let answer: Vec<(PathBuf, Item)> =
        answer.into_iter().filter_map(|item| Some((rebased(Path::new(&item.path), &walk.roots)?, item))).collect();
    let hidden = match walk.rules.shown {
        Some(shown) => hidden_folders(answer.iter().map(|(path, _)| path.as_path()), &walk.roots, shown, cancel),
        None => HashSet::new(),
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(Fallback::Cancelled);
    }
    let root = if walk.absolute { PathBuf::new() } else { walk.roots[0].clone() };
    let mut candidates: Vec<(PathBuf, Entry)> = Vec::new();
    for (path, item) in answer {
        let flags = flags_of(item.attributes.unwrap_or(0));
        if !kept(&path, &walk.roots, flags, &walk.rules, &hidden) {
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let mut entry = Entry {
            name,
            is_dir: item.is_dir,
            flags,
            size: if item.is_dir { 0 } else { item.size.unwrap_or(0) },
            modified: item.modified,
            created: item.created,
        };
        // A field Everything does not keep is read from the disk (spec 3.6).
        if ((!entry.is_dir && item.size.is_none()) || entry.modified.is_none())
            && let Ok(meta) = std::fs::symlink_metadata(&path)
        {
            entry.size = if meta.is_file() { meta.len() } else { 0 };
            entry.modified = meta.modified().ok();
            entry.created = entry.created.or_else(|| meta.created().ok());
        }
        if query.passes(&entry.name, entry.is_dir, entry.size, entry.modified) {
            candidates.push((path, entry));
        }
    }
    match query.content() {
        None => {
            let limit_reached = candidates.len() > query.max_results();
            candidates.truncate(query.max_results());
            let set = ResultSet::collect(root, false, candidates.into_iter().map(|(path, entry)| (path, entry, None)));
            send_whole(set, limit_reached, started, true, sink);
        }
        Some(_) => read_contents(&root, candidates, query, cancel, started, sink),
    }
    Ok(())
}

/// Everything's candidates read by Gezik's content matcher, a batch at a time on a few
/// low-priority threads.
fn read_contents(
    root: &Path,
    candidates: Vec<(PathBuf, Entry)>,
    query: &Query,
    cancel: &AtomicBool,
    started: Instant,
    sink: &dyn Fn(Event),
) {
    let Some(content) = query.content() else { return };
    let threads = threads_for(false);
    let mut numbers: HashMap<String, u32> = HashMap::new();
    let (mut found, mut limit_reached) = (0usize, false);
    for chunk in candidates.chunks(BATCH_ITEMS) {
        if cancel.load(Ordering::Relaxed) || limit_reached {
            break;
        }
        let per = chunk.len().div_ceil(threads).max(1);
        let lines: Vec<Option<Found>> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .chunks(per)
                .map(|part| {
                    scope.spawn(move || {
                        gezik_platform::priority::lower_this_thread();
                        part.iter()
                            .map(|(path, entry)| {
                                if cancel.load(Ordering::Relaxed) || !content.reads(&entry.name, entry.size) {
                                    return None;
                                }
                                content.find_in_file(path, cancel, &gezik_platform::decode_ansi).ok().flatten()
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().flat_map(|handle| handle.join().unwrap_or_default()).collect()
        });
        let mut batch = Batch::default();
        for ((path, entry), line) in chunk.iter().zip(lines) {
            let Some(line) = line else { continue };
            if found >= query.max_results() {
                limit_reached = true;
                break;
            }
            let Some(folder) = folder_text(root, path) else { continue };
            let next = numbers.len() as u32;
            let parent = *numbers.entry(folder).or_insert_with_key(|folder| {
                batch.folders.push(folder.as_str().into());
                next
            });
            batch.entries.push(entry.clone());
            batch.parent.push(parent);
            batch.matches.push(Some(line));
            found += 1;
        }
        if !batch.is_empty() {
            sink(Event::Batch(batch));
        }
        sink(Event::Progress { found, folders: 0 });
    }
    sink(Event::Done(Summary {
        found,
        limit_reached,
        cancelled: cancel.load(Ordering::Relaxed) && !limit_reached,
        elapsed: started.elapsed(),
        everything: true,
        ..Summary::default()
    }));
}

/// Starts `spec`: through Everything when it can answer (spec 3.6), else the walk, on the same
/// flag. `on`: `[search] everything = "auto"`.
pub fn start(
    spec: SearchSpec,
    walk: Walk,
    query: Query,
    on: bool,
    sink: impl Fn(Event) + Send + Sync + 'static,
) -> Running {
    let running = Running::default();
    let flag = running.flag();
    let again = running.clone();
    let _ = std::thread::Builder::new().name("gezik-search-everything".into()).spawn(move || {
        let sink = Arc::new(sink);
        match search(&spec, &walk, &query, on, &flag, &*sink) {
            Ok(()) => {}
            Err(Fallback::Cancelled) => sink(Event::Done(Summary { cancelled: true, ..Summary::default() })),
            Err(_) => {
                let sink = sink.clone();
                start_with(walk, query, again, move |event| sink(event));
            }
        }
    });
    running
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::search::{DateRange, KindFilter, Scope, SearchSpec};

    /// A LIST2 answer as Everything builds it, for `items` (path, is a folder, size).
    fn answer(items: &[(&str, bool, u64)], request: u32) -> Vec<u8> {
        let header = 20 + items.len() * 8;
        let mut data = Vec::new();
        let mut index = Vec::new();
        for (path, folder, size) in items {
            index.push((if *folder { ITEM_FOLDER } else { 0 }, (header + data.len()) as u32));
            if request & REQUEST_FULL_PATH_AND_NAME != 0 {
                let units: Vec<u16> = path.encode_utf16().collect();
                data.extend((units.len() as u32).to_le_bytes());
                data.extend(units.iter().flat_map(|u| u.to_le_bytes()));
                data.extend([0, 0]);
            }
            if request & REQUEST_SIZE != 0 {
                data.extend(size.to_le_bytes());
            }
            if request & REQUEST_DATE_CREATED != 0 {
                data.extend(0u64.to_le_bytes());
            }
            if request & REQUEST_DATE_MODIFIED != 0 {
                // 2026-01-01 00:00:00 UTC.
                data.extend((116_444_736_000_000_000u64 + 1_767_225_600 * 10_000_000).to_le_bytes());
            }
            if request & REQUEST_ATTRIBUTES != 0 {
                data.extend(0x22u32.to_le_bytes());
            }
        }
        let mut out = Vec::new();
        for value in [items.len() as u32, items.len() as u32, 0, request, SORT_NAME_ASCENDING] {
            out.extend(value.to_le_bytes());
        }
        for (flags, offset) in index {
            out.extend(flags.to_le_bytes());
            out.extend(offset.to_le_bytes());
        }
        out.extend(data);
        out
    }

    #[test]
    fn a_query_is_seven_words_and_the_text() {
        let bytes = encode_query2(0x1234, 77, "a b", SEARCH_MATCH_CASE, 500, REQUEST);
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        let words: Vec<u32> = (0..7).map(word).collect();
        assert_eq!(words, [0x1234, 77, SEARCH_MATCH_CASE, 0, 500, REQUEST, SORT_NAME_ASCENDING]);
        assert_eq!(&bytes[28..], &[b'a', 0, b' ', 0, b'b', 0, 0, 0]);
    }

    #[test]
    fn an_answer_is_read_back() {
        let bytes = answer(&[(r"D:\Work\a.txt", false, 5), (r"D:\Work\sub", true, u64::MAX)], REQUEST);
        let items = decode_list2(&bytes).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].path.as_str(), items[0].is_dir, items[0].size), (r"D:\Work\a.txt", false, Some(5)));
        assert_eq!((items[1].is_dir, items[1].size), (true, None), "a size it does not know");
        assert_eq!(items[0].created, None, "a zero time is none");
        assert_eq!(items[0].modified, Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_767_225_600)));
        assert_eq!(items[0].attributes, Some(0x22));
        assert!(decode_list2(&bytes[..bytes.len() - 3]).is_err(), "cut short");
        assert!(decode_list2(&[1, 2]).is_err());
    }

    fn spec(pattern: &str) -> SearchSpec {
        let mut spec = SearchSpec::new(Scope::Folder(r"D:\Work".into()));
        spec.pattern = pattern.into();
        spec
    }

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from(r"D:\Work")]
    }

    #[test]
    fn a_pattern_becomes_everythings_syntax() {
        assert_eq!(translate(&spec("*.pdf"), &roots()), Some((r#""D:\Work\" wfn:*.pdf"#.to_owned(), 0)));
        assert_eq!(translate(&spec("rapor"), &roots()).unwrap().0, r#""D:\Work\" rapor"#);
        assert_eq!(
            translate(&spec("*.jpg;*.png;!*thumb*"), &roots()).unwrap().0,
            r#""D:\Work\" !wfn:*thumb* <wfn:*.jpg|wfn:*.png>"#
        );
        assert_eq!(translate(&spec("my doc"), &roots()).unwrap().0, r#""D:\Work\" "my doc""#);
        assert_eq!(translate(&spec("my file"), &roots()).unwrap().0, r#""D:\Work\" wfn:"*my f?le*""#);
        // Everything does not take İ for i: the Turkish letters become wildcards, Gezik checks after.
        assert_eq!(translate(&spec("istanbul"), &roots()).unwrap().0, r#""D:\Work\" wfn:*?stanbul*"#);
        assert_eq!(translate(&spec("!ılık"), &roots()).unwrap().0, r#""D:\Work\""#, "a Turkish exclusion is Gezik's");
    }

    #[test]
    fn criteria_and_scopes() {
        let mut s = spec("");
        s.kind = KindFilter::Folders;
        s.size.min = Some(500);
        s.modified = DateRange::Today;
        assert_eq!(translate(&s, &roots()).unwrap().0, r#""D:\Work\" folder: size:>=500 dm:today"#);
        let flat = SearchSpec::flat_view(r"D:\Work".into());
        assert_eq!(translate(&flat, &roots()).unwrap().0, r#""D:\Work\" file:"#);
        let drives = [PathBuf::from(r"C:\"), PathBuf::from(r"D:\")];
        assert_eq!(translate(&spec("x"), &drives).unwrap().0, r#"<"C:\"|"D:\"> x"#);
        s.modified = DateRange::LastDays(7);
        assert!(!translate(&s, &roots()).unwrap().0.contains("dm:"), "left to Gezik's check");
    }

    #[test]
    fn regexes_go_as_they_are_unless_the_turkish_i_matters() {
        let mut s = spec("^rapor.*\\.pdf$");
        s.name_regex = true;
        assert_eq!(translate(&s, &roots()), Some((r#""D:\Work\" regex:"^rapor.*\.pdf$""#.to_owned(), 0)));
        s.pattern = "^ist".into();
        assert_eq!(translate(&s, &roots()), None, "the walk folds İ; Everything would miss it");
        s.match_case = true;
        assert_eq!(translate(&s, &roots()).map(|t| t.1), Some(SEARCH_MATCH_CASE));
        assert_eq!(translate(&spec("x"), &[]), None, "no scope");
    }

    // Windows paths: their parts are only parts there.
    #[cfg(windows)]
    #[test]
    fn gezik_rules_apply_to_everythings_answers() {
        let rules = WalkRules::new(Some((false, false)), &[".git".to_owned()], Vec::new());
        let roots = roots();
        let none = HashSet::new();
        let path = |p: &str| PathBuf::from(p);
        assert!(kept(&path(r"D:\Work\a\x.txt"), &roots, 0, &rules, &none));
        assert!(!kept(&path(r"D:\Work\.git\x.txt"), &roots, 0, &rules, &none), "under a skipped folder");
        assert!(!kept(&path(r"D:\Work\.git"), &roots, 0, &rules, &none), "a dot name the view hides");
        assert!(!kept(&path(r"D:\Work\.cache\x.txt"), &roots, 0, &rules, &none), "under a dot folder");
        assert!(!kept(&path(r"D:\Work\x.txt"), &roots, gezik_core::Entry::HIDDEN, &rules, &none));
        let hidden: HashSet<String> = [gezik_core::pattern::fold_text(r"D:\Work\Secret")].into();
        assert!(!kept(&path(r"D:\Work\secret\x.txt"), &roots, 0, &rules, &hidden), "under a hidden folder");
        assert!(!kept(&path(r"E:\x.txt"), &roots, 0, &rules, &none), "outside the scope");
        let all = WalkRules::new(None, &[], Vec::new());
        assert!(kept(&path(r"D:\Work\.git\x.txt"), &roots, gezik_core::Entry::HIDDEN, &all, &none));
    }

    #[cfg(windows)]
    #[test]
    fn answers_take_the_scopes_spelling() {
        let roots = [PathBuf::from(r"C:\WINDOWS")];
        let path = |p: &str| PathBuf::from(p);
        assert_eq!(rebased(&path(r"C:\Windows\System32\x.dll"), &roots), Some(path(r"C:\WINDOWS\System32\x.dll")));
        assert_eq!(rebased(&path(r"c:\windows"), &roots), Some(path(r"C:\WINDOWS")));
        assert_eq!(rebased(&path(r"C:\Windowsx\a"), &roots), None);
        let drive = [PathBuf::from(r"D:\")];
        assert_eq!(rebased(&path(r"d:\a\b"), &drive), Some(path(r"D:\a\b")));
    }

    #[cfg(windows)]
    #[test]
    fn folders_the_view_hides_are_read_from_the_disk() {
        let root = std::env::temp_dir().join(format!("gezik-everything-hidden-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["h/in", "v"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        gezik_platform::fs::set_hidden(&root.join("h")).unwrap();
        let paths = [root.join("h").join("in").join("x.txt"), root.join("v").join("y.txt"), root.join("z.txt")];
        let roots = [root.clone()];
        let cancel = AtomicBool::new(false);
        let hidden = hidden_folders(paths.iter().map(PathBuf::as_path), &roots, (false, false), &cancel);
        assert_eq!(hidden, [fold_text(&root.join("h").display().to_string())].into());
        assert!(hidden_folders(paths.iter().map(PathBuf::as_path), &roots, (true, true), &cancel).is_empty());
        let rules = WalkRules::new(Some((false, false)), &[], Vec::new());
        assert!(!kept(&paths[0], &roots, 0, &rules, &hidden));
        assert!(kept(&paths[1], &roots, 0, &rules, &hidden));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Needs Everything running with its database loaded (Windows): `cargo test -p gezik-search
    /// everything_answers -- --ignored`. Task 9 records the result for 1.4.1 and 1.5a.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs Everything"]
    fn everything_answers_a_real_query() {
        gezik_platform::everything::ready(Duration::from_secs(1)).expect("Everything runs with its database");
        let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        let search = format!("\"{windows}\\\" wfn:notepad.exe");
        let bytes = gezik_platform::everything::query(
            &|window, id| encode_query2(window, id, &search, 0, 100, REQUEST),
            Duration::from_secs(5),
        )
        .unwrap();
        let items = decode_list2(&bytes).unwrap();
        assert!(items.iter().any(|item| item.path.to_lowercase().ends_with("notepad.exe")), "{items:?}");
        assert!(items.iter().all(|item| item.modified.is_some()));
    }

    /// A whole search answered by Everything, with and without the view's hidden rule (the
    /// second asks for the hidden folders too): `cargo test -p gezik-search everything_runs
    /// -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs Everything"]
    fn everything_runs_a_whole_search() {
        let windows = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()));
        let mut spec = SearchSpec::new(Scope::Folder(windows.clone()));
        spec.pattern = "notepad.exe".into();
        let options = crate::query::QueryOptions::local(64 * 1024 * 1024, 1000);
        for shown in [None, Some((false, false))] {
            let query = Query::compile(&spec, &options).unwrap();
            let walk = Walk::new(vec![windows.clone()], false, 2, WalkRules::new(shown, &[], Vec::new()));
            let events = std::sync::Mutex::new(Vec::new());
            let cancel = AtomicBool::new(false);
            search(&spec, &walk, &query, true, &cancel, &|event| events.lock().unwrap().push(event)).unwrap();
            let mut set = ResultSet::new(windows.clone(), false);
            let mut summary = None;
            for event in events.into_inner().unwrap() {
                match event {
                    Event::Batch(batch) => set.append(batch),
                    Event::Done(done) => summary = Some(done),
                    Event::Progress { .. } => {}
                }
            }
            let summary = summary.expect("a summary");
            assert!(summary.everything && summary.found == set.len() && !summary.cancelled);
            assert_eq!(
                set.path_at(set.index_of_path(&windows.join("notepad.exe")).unwrap()),
                Some(windows.join("notepad.exe"))
            );
        }
        assert_eq!(
            search(
                &spec,
                &Walk::new(vec![windows], false, 2, WalkRules::new(None, &[], Vec::new())),
                &Query::compile(&spec, &options).unwrap(),
                false,
                &AtomicBool::new(false),
                &|_| {}
            ),
            Err(Fallback::Off)
        );
    }
}
