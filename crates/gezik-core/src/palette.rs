//! The command palette and quick open (spec 7): what an item is, how the typed words match
//! and in which order the items come. Pure; the app builds the items and runs the one chosen.

use crate::pattern::fold_text;
use crate::sort::natural_cmp;

/// What an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Action,
    Command,
    ViewOption,
    Pinned,
    Recent,
    Tab,
    TabSet,
    SavedSearch,
    SavedFilter,
}

/// The tie order of the command palette (spec 7.2's table) and of quick open (places first).
const COMMANDS_ORDER: [Kind; 9] = [
    Kind::Action,
    Kind::Command,
    Kind::ViewOption,
    Kind::Pinned,
    Kind::Recent,
    Kind::Tab,
    Kind::TabSet,
    Kind::SavedSearch,
    Kind::SavedFilter,
];
const QUICK_ORDER: [Kind; 9] = [
    Kind::Pinned,
    Kind::Recent,
    Kind::Tab,
    Kind::TabSet,
    Kind::SavedSearch,
    Kind::SavedFilter,
    Kind::Action,
    Kind::Command,
    Kind::ViewOption,
];

impl Kind {
    /// The label at the right of its line.
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Action => "Action",
            Kind::Command => "Command",
            Kind::ViewOption => "View",
            Kind::Pinned => "Pinned",
            Kind::Recent => "Recent",
            Kind::Tab => "Tab",
            Kind::TabSet => "Tab set",
            Kind::SavedSearch => "Saved search",
            Kind::SavedFilter => "Saved filter",
        }
    }

    /// Its part of an item's key in `[palette] recent` (`action:copy-path`).
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Action => "action",
            Kind::Command => "command",
            Kind::ViewOption => "view",
            Kind::Pinned => "pinned",
            Kind::Recent => "recent",
            Kind::Tab => "tab",
            Kind::TabSet => "tab-set",
            Kind::SavedSearch => "search",
            Kind::SavedFilter => "filter",
        }
    }

    /// A place: quick open's first, never the command palette's.
    pub fn is_place(self) -> bool {
        !matches!(self, Kind::Action | Kind::Command | Kind::ViewOption)
    }

    fn order(self, mode: Mode) -> usize {
        let order = if mode == Mode::Commands { &COMMANDS_ORDER } else { &QUICK_ORDER };
        order.iter().position(|k| *k == self).unwrap_or(order.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `>` in front (Ctrl+Shift+P): actions, commands, view options.
    Commands,
    /// Ctrl+P: everything, places first.
    QuickOpen,
}

/// The mode the field's text puts the box in, and the text to match (spec 7.1).
pub fn mode_of(text: &str) -> (Mode, &str) {
    match text.strip_prefix('>') {
        Some(rest) => (Mode::Commands, rest.trim_start()),
        None => (Mode::QuickOpen, text),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    /// What it is among its kind: an action's name, a path, a saved name.
    pub id: String,
    pub title: String,
    /// The dim line under the title: a path, a pattern, a state.
    pub detail: String,
}

impl Item {
    /// Its key in `[palette] recent`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.prefix(), self.id)
    }
}

/// An item as it is matched: folded once when the box opens.
pub struct Folded {
    title: String,
    detail: String,
    key: String,
}

pub fn fold_items(items: &[Item]) -> Vec<Folded> {
    items.iter().map(|i| Folded { title: fold(&i.title), detail: fold(&i.detail), key: i.key() }).collect()
}

/// Lower case with the Turkish i folded (6a's rule), `/` as `\` (a path is typed with either).
fn fold(text: &str) -> String {
    fold_text(&text.replace('/', "\\"))
}

/// Whether `word` starts a word somewhere in `text` (at its start or after a non-letter).
fn starts_a_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| text[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric()))
}

/// How well `words` match: 0 the title starts with the text, 1 every word starts a word, 2
/// every word is inside; `None` when one is in neither the title nor the detail.
fn tier(f: &Folded, words: &[&str], whole: &str) -> Option<u8> {
    if !words.iter().all(|w| f.title.contains(w) || f.detail.contains(w)) {
        return None;
    }
    if f.title.starts_with(whole) {
        Some(0)
    } else if words.iter().all(|w| starts_a_word(&f.title, w) || starts_a_word(&f.detail, w)) {
        Some(1)
    } else {
        Some(2)
    }
}

/// Keys of the items used last, newest first, at most this many (`[palette] recent`).
pub const RECENT_MAX: usize = 20;

/// The items `text` lets through in `mode`, best first (spec 7.3): the text splits into words,
/// each must be in the title or the detail (`*`, `?`, `;`, `!` are plain characters); a title
/// that starts with the text, then every word at a word's start, then inside; among equals the
/// recently used first, then the kinds in `mode`'s order, then the titles in natural order.
pub fn rank(items: &[Item], folded: &[Folded], text: &str, mode: Mode, recent: &[String]) -> Vec<usize> {
    let text = fold(text.trim());
    let words: Vec<&str> = text.split_whitespace().collect();
    let whole = words.join(" ");
    let mut found: Vec<(u8, usize, usize, usize)> = items
        .iter()
        .zip(folded)
        .enumerate()
        .filter(|(_, (item, _))| mode == Mode::QuickOpen || !item.kind.is_place())
        .filter_map(|(i, (item, f))| {
            let tier = if words.is_empty() { 0 } else { tier(f, &words, &whole)? };
            let used = recent.iter().position(|k| *k == f.key).unwrap_or(usize::MAX);
            Some((tier, used, item.kind.order(mode), i))
        })
        .collect();
    found.sort_by(|a, b| {
        (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)).then_with(|| natural_cmp(&items[a.3].title, &items[b.3].title))
    });
    found.into_iter().map(|f| f.3).collect()
}

/// `key` was used now: first, once, at most [`RECENT_MAX`] kept.
pub fn remember(recent: &mut Vec<String>, key: String) {
    recent.retain(|k| *k != key);
    recent.insert(0, key);
    recent.truncate(RECENT_MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: Kind, title: &str, detail: &str) -> Item {
        Item { kind, id: title.to_lowercase(), title: title.into(), detail: detail.into() }
    }

    fn titles(items: &[Item], text: &str, mode: Mode, recent: &[String]) -> Vec<String> {
        let folded = fold_items(items);
        rank(items, &folded, text, mode, recent).into_iter().map(|i| items[i].title.clone()).collect()
    }

    fn sample() -> Vec<Item> {
        vec![
            item(Kind::Action, "Copy Path", ""),
            item(Kind::Action, "Copy", ""),
            item(Kind::Action, "Undo Copy", ""),
            item(Kind::Action, "Recopy", ""),
            item(Kind::Pinned, "İndirilenler", r"C:\Users\a\Downloads"),
            item(Kind::Recent, "Work", "D:/Work"),
            item(Kind::SavedFilter, "Pictures", "*.jpg;*.png"),
        ]
    }

    #[test]
    fn every_word_must_be_in_the_title_or_the_detail() {
        let items = sample();
        assert_eq!(titles(&items, "copy path", Mode::QuickOpen, &[]), ["Copy Path"]);
        assert_eq!(titles(&items, "users downloads", Mode::QuickOpen, &[]), ["İndirilenler"], "by its detail");
        assert_eq!(titles(&items, "indir", Mode::QuickOpen, &[]), ["İndirilenler"], "Turkish İ folds");
        assert_eq!(titles(&items, "d:/work", Mode::QuickOpen, &[]), ["Work"], "either slash");
        assert!(titles(&items, "zzz", Mode::QuickOpen, &[]).is_empty());
    }

    #[test]
    fn the_pattern_language_is_plain_text_here() {
        let items = sample();
        assert_eq!(titles(&items, "*.jpg", Mode::QuickOpen, &[]), ["Pictures"], "* is a character");
        assert!(titles(&items, "!copy", Mode::QuickOpen, &[]).is_empty(), "! leaves nothing out");
    }

    #[test]
    fn a_title_start_beats_a_word_start_beats_inside() {
        let items = sample();
        assert_eq!(titles(&items, "copy", Mode::Commands, &[]), ["Copy", "Copy Path", "Undo Copy", "Recopy"]);
    }

    #[test]
    fn the_recently_used_come_first_among_equals() {
        let items = sample();
        let recent = vec!["action:copy path".to_owned()];
        assert_eq!(titles(&items, "copy", Mode::Commands, &recent), ["Copy Path", "Copy", "Undo Copy", "Recopy"]);
    }

    #[test]
    fn quick_open_lists_places_first_and_the_palette_none() {
        let items = sample();
        assert_eq!(
            titles(&items, "", Mode::QuickOpen, &[]),
            ["İndirilenler", "Work", "Pictures", "Copy", "Copy Path", "Recopy", "Undo Copy"]
        );
        assert_eq!(titles(&items, "", Mode::Commands, &[]), ["Copy", "Copy Path", "Recopy", "Undo Copy"]);
    }

    #[test]
    fn a_leading_angle_bracket_is_the_command_palette() {
        assert_eq!(mode_of(">copy"), (Mode::Commands, "copy"));
        assert_eq!(mode_of(">  copy"), (Mode::Commands, "copy"));
        assert_eq!(mode_of(">"), (Mode::Commands, ""));
        assert_eq!(mode_of("work"), (Mode::QuickOpen, "work"));
    }

    #[test]
    fn remembering_keeps_twenty_once_each_newest_first() {
        let mut recent: Vec<String> = (0..RECENT_MAX).map(|i| format!("action:{i}")).collect();
        remember(&mut recent, "action:5".into());
        assert_eq!((recent[0].as_str(), recent.len()), ("action:5", RECENT_MAX));
        remember(&mut recent, "tab:new".into());
        assert_eq!((recent[0].as_str(), recent.len()), ("tab:new", RECENT_MAX));
        assert!(!recent.contains(&format!("action:{}", RECENT_MAX - 1)), "the oldest goes");
    }

    #[test]
    #[ignore = "timing; run with --ignored on a quiet machine"]
    fn two_thousand_items_rank_within_five_milliseconds() {
        let items: Vec<Item> = (0..2000)
            .map(|i| item(Kind::Recent, &format!("Folder {i} İstanbul"), &format!(r"D:\Projects\client-{i}\src")))
            .collect();
        let folded = fold_items(&items);
        let started = std::time::Instant::now();
        let found = rank(&items, &folded, "ist src", Mode::QuickOpen, &[]);
        assert!(started.elapsed() < std::time::Duration::from_millis(5), "{:?}", started.elapsed());
        assert_eq!(found.len(), 2000);
    }
}
