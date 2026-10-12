//! Tags, a rating and a comment on a file (spec 12 §3.1), and how each system's record holds
//! them: Gezik's NTFS stream text (§4.2), freedesktop's `user.xdg.tags` and Baloo's rating
//! (§4.3), Finder's FinderInfo label bits (§4.1; its plists are in [`bplist`]).

pub mod bplist;

use crate::pattern::fold_text;

/// Finder's colour numbers, written alike on every system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagColor {
    #[default]
    None,
    Gray,
    Green,
    Purple,
    Blue,
    Yellow,
    Red,
    Orange,
}

impl TagColor {
    const ALL: [TagColor; 8] = [
        TagColor::None,
        TagColor::Gray,
        TagColor::Green,
        TagColor::Purple,
        TagColor::Blue,
        TagColor::Yellow,
        TagColor::Red,
        TagColor::Orange,
    ];

    /// Finder's number for it (0–7).
    pub fn number(self) -> u8 {
        self as u8
    }

    /// The colour numbered `n`; an unknown number is no colour.
    pub fn from_number(n: u8) -> TagColor {
        TagColor::ALL.get(usize::from(n)).copied().unwrap_or(TagColor::None)
    }

    /// The tag Finder shows for a bare FinderInfo label of this colour.
    pub fn default_name(self) -> &'static str {
        match self {
            TagColor::None => "",
            TagColor::Gray => "Gray",
            TagColor::Green => "Green",
            TagColor::Purple => "Purple",
            TagColor::Blue => "Blue",
            TagColor::Yellow => "Yellow",
            TagColor::Red => "Red",
            TagColor::Orange => "Orange",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: Box<str>,
    pub color: TagColor,
}

impl Tag {
    pub fn new(name: &str, color: TagColor) -> Tag {
        Tag { name: name.into(), color }
    }
}

/// The longest comment kept (characters).
pub const MAX_COMMENT: usize = 1000;
/// The longest tag name (characters).
pub const MAX_TAG_NAME: usize = 64;
/// The most tags one file keeps: with a full comment its stream stays under 16 KB.
pub const MAX_TAGS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    /// In the file's own order.
    pub tags: Vec<Tag>,
    /// 0: not rated; 1–5 stars.
    pub rating: u8,
    /// One paragraph.
    pub comment: Box<str>,
}

impl Meta {
    /// No tags, no rating, no comment: writing it removes the record.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.rating == 0 && self.comment.is_empty()
    }

    pub fn has_tag(&self, name: &str) -> bool {
        self.tags.iter().any(|tag| same_name(&tag.name, name))
    }

    /// Whether `other` is the same record as far as every system keeps it: a colour counts
    /// only where both have one (Linux keeps none).
    pub fn matches(&self, other: &Meta) -> bool {
        self.rating == other.rating
            && self.comment == other.comment
            && self.tags.len() == other.tags.len()
            && self.tags.iter().zip(&other.tags).all(|(a, b)| {
                a.name == b.name && (a.color == b.color || a.color == TagColor::None || b.color == TagColor::None)
            })
    }

    /// `edit` done to it.
    pub fn apply(&mut self, edit: &MetaEdit) {
        match edit {
            MetaEdit::AddTags(tags) => {
                for tag in tags {
                    // A name no system can hold is never written.
                    if self.tags.len() < MAX_TAGS && check_tag_name(&tag.name).is_ok() && !self.has_tag(&tag.name) {
                        self.tags.push(tag.clone());
                    }
                }
            }
            MetaEdit::RemoveTags(names) => self.tags.retain(|tag| !names.iter().any(|name| same_name(&tag.name, name))),
            MetaEdit::ClearTags => self.tags.clear(),
            MetaEdit::Rating(stars) => self.rating = (*stars).min(5),
            MetaEdit::Comment(text) => self.comment = one_paragraph(text),
        }
    }
}

/// A change made to every file of a tag job (spec 12 §5, §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaEdit {
    /// Tags it does not have yet, at the end.
    AddTags(Vec<Tag>),
    /// By name (case-insensitive).
    RemoveTags(Vec<Box<str>>),
    /// Every tag; the rating and the comment stay.
    ClearTags,
    Rating(u8),
    Comment(Box<str>),
}

/// `text` on one line, at most [`MAX_COMMENT`] characters.
fn one_paragraph(text: &str) -> Box<str> {
    // Line breaks, tabs and the store's field separator (U+001F) are all control characters.
    // A Windows line break is one break: one space.
    let line: String =
        text.trim().replace("\r\n", "\n").chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    line.chars().take(MAX_COMMENT).collect::<String>().into()
}

/// Why `name` cannot be a tag's name, if it cannot: commas part Linux's list, a line break
/// parts Finder's name from its colour.
pub fn check_tag_name(name: &str) -> Result<(), &'static str> {
    let count = name.chars().count();
    if count == 0 {
        Err("a tag name cannot be empty")
    } else if count > MAX_TAG_NAME {
        Err("a tag name is at most 64 characters")
    } else if name.trim() != name {
        Err("a tag name cannot start or end with a space")
    } else if name.contains(',') {
        Err("a tag name cannot hold a comma")
    } else if name.contains(['\n', '\r']) {
        Err("a tag name cannot hold a line break")
    } else if name.chars().any(char::is_control) {
        Err("a tag name cannot hold a control character")
    } else {
        Ok(())
    }
}

/// Whether two tag names are one tag (case-insensitive, Turkish İ/ı too).
pub fn same_name(a: &str, b: &str) -> bool {
    a == b || fold_text(a) == fold_text(b)
}

/// What Gezik's NTFS stream held (spec 12 §4.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamRecord {
    pub meta: Meta,
    /// Lines this version does not know: written back as they were.
    pub unknown: Vec<Box<str>>,
    /// False for a later version's record: shown, never written over.
    pub writable: bool,
}

const STREAM_HEADER: &str = "gezik-meta";

/// Reads the stream's text; one without a version line is shown but not written over.
pub fn decode_stream(text: &str) -> StreamRecord {
    let mut lines = text.lines();
    let version = lines.next().and_then(|first| first.strip_prefix(STREAM_HEADER)).map(str::trim);
    // A blank stream is Gezik's to write: nothing in it can be lost.
    let blank = text.trim().is_empty();
    let mut record = StreamRecord { writable: blank || version == Some("1"), ..StreamRecord::default() };
    for line in lines {
        if !decode_line(line, &mut record.meta) && !line.is_empty() {
            record.unknown.push(line.into());
        }
    }
    record
}

/// One known line into `meta`; false if it is not one.
fn decode_line(line: &str, meta: &mut Meta) -> bool {
    let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
    match key {
        "tag" => {
            let Some((color, name)) = rest.split_once(' ') else { return false };
            let Ok(color) = color.parse::<u8>() else { return false };
            if check_tag_name(name).is_err() {
                return false;
            }
            if !meta.has_tag(name) {
                meta.tags.push(Tag::new(name, TagColor::from_number(color)));
            }
            true
        }
        "rating" => match rest.parse::<u8>() {
            Ok(stars) if stars <= 5 => {
                meta.rating = stars;
                true
            }
            _ => false,
        },
        "comment" => {
            meta.comment = one_paragraph(rest);
            true
        }
        _ => false,
    }
}

/// The lines of `meta` without the version line (also what the tag store keeps).
pub fn meta_lines(meta: &Meta) -> Vec<String> {
    let mut lines: Vec<String> =
        meta.tags.iter().map(|tag| format!("tag {} {}", tag.color.number(), tag.name)).collect();
    if meta.rating > 0 {
        lines.push(format!("rating {}", meta.rating.min(5)));
    }
    if !meta.comment.is_empty() {
        lines.push(format!("comment {}", one_paragraph(&meta.comment)));
    }
    lines
}

/// Reads [`meta_lines`]' lines back; unknown ones go to `unknown`.
pub fn meta_from_lines<'a>(lines: impl IntoIterator<Item = &'a str>, unknown: &mut Vec<Box<str>>) -> Meta {
    let mut meta = Meta::default();
    for line in lines {
        if !decode_line(line, &mut meta) && !line.is_empty() {
            unknown.push(line.into());
        }
    }
    meta
}

/// The stream's text for `meta`, with lines an earlier read did not know.
pub fn encode_stream(meta: &Meta, unknown: &[Box<str>]) -> String {
    let mut text = format!("{STREAM_HEADER} 1\n");
    for line in meta_lines(meta).iter().map(String::as_str).chain(unknown.iter().map(|line| &**line)) {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// `user.xdg.tags`: names parted by commas.
pub fn decode_xdg_tags(text: &str) -> Vec<Box<str>> {
    let mut names: Vec<Box<str>> = Vec::new();
    for name in text.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        if !names.iter().any(|known| same_name(known, name)) {
            names.push(name.into());
        }
    }
    names
}

pub fn encode_xdg_tags(tags: &[Tag]) -> String {
    tags.iter().map(|tag| &*tag.name).collect::<Vec<_>>().join(",")
}

/// Baloo's `user.baloo.rating` (0–10, half stars) as stars: an odd number rounds up.
pub fn decode_baloo_rating(text: &str) -> u8 {
    text.trim().parse::<u8>().map(|n| n.min(10).div_ceil(2)).unwrap_or(0)
}

pub fn encode_baloo_rating(stars: u8) -> String {
    (stars.min(5) * 2).to_string()
}

/// The legacy label colour in a FinderInfo (byte 9, bits 1–3).
pub fn finder_info_color(info: &[u8]) -> TagColor {
    info.get(9).map_or(TagColor::None, |byte| TagColor::from_number((byte >> 1) & 7))
}

/// `info` (32 bytes, or none yet) with its label colour set to `color`; `None` when the
/// FinderInfo would be all zeros (the attribute goes).
pub fn with_finder_info_color(info: Option<&[u8]>, color: TagColor) -> Option<Vec<u8>> {
    let mut bytes = info.filter(|info| info.len() == 32).map(<[u8]>::to_vec).unwrap_or_else(|| vec![0; 32]);
    bytes[9] = (bytes[9] & !0x0E) | (color.number() << 1);
    bytes.iter().any(|&b| b != 0).then_some(bytes)
}

/// The colour Finder puts in the FinderInfo for `tags`: the last one with a colour.
pub fn label_color(tags: &[Tag]) -> TagColor {
    tags.iter().rev().map(|tag| tag.color).find(|color| *color != TagColor::None).unwrap_or(TagColor::None)
}

/// `stars` as the Rating column draws them: `★★★☆☆`; nothing when not rated.
pub fn stars_text(stars: u8) -> String {
    let n = usize::from(stars.min(5));
    if n == 0 { String::new() } else { format!("{}{}", "★".repeat(n), "☆".repeat(5 - n)) }
}

/// Whether Windows has a property handler that keeps Explorer's Tags (`System.Keywords`) and
/// rating in files named so (spec 12 §8.3): a fixed list, as trying any file loads a handler.
pub fn explorer_kind(name: &str) -> bool {
    const KINDS: [&str; 13] =
        ["jpg", "jpeg", "tif", "tiff", "heic", "mp3", "m4a", "wma", "mp4", "wmv", "docx", "xlsx", "pptx"];
    name.rsplit_once('.').is_some_and(|(_, ext)| KINDS.iter().any(|k| k.eq_ignore_ascii_case(ext)))
}

/// `System.Rating` (1–99) as stars, by Explorer's ranges (it writes 1, 25, 50, 75, 99).
pub fn stars_from_windows(rating: u32) -> u8 {
    match rating {
        0 => 0,
        1..=12 => 1,
        13..=37 => 2,
        38..=62 => 3,
        63..=87 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(tags: &[(&str, TagColor)], rating: u8, comment: &str) -> Meta {
        Meta { tags: tags.iter().map(|(n, c)| Tag::new(n, *c)).collect(), rating, comment: comment.into() }
    }

    #[test]
    fn names_follow_the_rules() {
        assert!(check_tag_name("Project X").is_ok());
        assert!(check_tag_name(&"x".repeat(64)).is_ok());
        assert_eq!(check_tag_name(&"x".repeat(65)), Err("a tag name is at most 64 characters"));
        assert_eq!(check_tag_name("a,b"), Err("a tag name cannot hold a comma"));
        assert_eq!(check_tag_name("a\nb"), Err("a tag name cannot hold a line break"));
        assert_eq!(check_tag_name(" a"), Err("a tag name cannot start or end with a space"));
        assert_eq!(check_tag_name(""), Err("a tag name cannot be empty"));
        assert_eq!(check_tag_name("a\u{1f}b"), Err("a tag name cannot hold a control character"));
        assert_eq!(check_tag_name("a\tb"), Err("a tag name cannot hold a control character"));
        assert!(same_name("red", "RED"));
        assert!(same_name("İzmir", "izmir"));
        assert!(same_name("ılık", "ILIK"));
        assert!(!same_name("Red", "Reds"));
    }

    #[test]
    fn edits_add_remove_and_clear() {
        let mut m = meta(&[("Red", TagColor::Red)], 3, "x");
        m.apply(&MetaEdit::AddTags(vec![Tag::new("red", TagColor::None), Tag::new("Blue", TagColor::Blue)]));
        assert_eq!(m.tags.len(), 2, "a name it has is not added twice");
        m.apply(&MetaEdit::RemoveTags(vec!["RED".into()]));
        assert_eq!(m.tags, [Tag::new("Blue", TagColor::Blue)]);
        m.apply(&MetaEdit::ClearTags);
        assert!(m.tags.is_empty());
        assert_eq!((m.rating, &*m.comment), (3, "x"), "clearing tags keeps the rest");
        m.apply(&MetaEdit::Rating(9));
        assert_eq!(m.rating, 5);
        m.apply(&MetaEdit::Comment("one\ntwo".into()));
        assert_eq!(&*m.comment, "one two");
        m.apply(&MetaEdit::Comment("y".repeat(2000).into()));
        assert_eq!(m.comment.chars().count(), MAX_COMMENT);
        m.apply(&MetaEdit::Comment("a\u{1f}b\tc".into()));
        assert_eq!(&*m.comment, "a b c", "no control characters: the store parts fields with U+001F");
        m.apply(&MetaEdit::Comment("one\r\ntwo\rthree".into()));
        assert_eq!(&*m.comment, "one two three", "CRLF is one space");
        m.apply(&MetaEdit::AddTags(vec![Tag::new("bad\u{1f}", TagColor::Red)]));
        assert!(m.tags.is_empty(), "a name no system holds is not added");
        let many: Vec<Tag> = (0..100).map(|i| Tag::new(&format!("tag {i}"), TagColor::Red)).collect();
        m.apply(&MetaEdit::AddTags(many));
        assert_eq!(m.tags.len(), MAX_TAGS);
        let longest: Vec<Tag> =
            (0..MAX_TAGS).map(|i| Tag::new(&format!("{i:02}{}", "ğ".repeat(62)), TagColor::Red)).collect();
        let full = Meta { tags: longest, rating: 5, comment: "ğ".repeat(MAX_COMMENT).into() };
        assert!(encode_stream(&full, &[]).len() < 16 * 1024, "the fullest record fits the stream");
        m.apply(&MetaEdit::ClearTags);
        m.apply(&MetaEdit::Rating(0));
        m.apply(&MetaEdit::Comment("".into()));
        assert!(m.is_empty());
    }

    #[test]
    fn records_match_where_a_system_keeps_no_colour() {
        let red = meta(&[("Red", TagColor::Red)], 2, "x");
        assert!(red.matches(&meta(&[("Red", TagColor::None)], 2, "x")));
        assert!(!red.matches(&meta(&[("Red", TagColor::Blue)], 2, "x")));
        assert!(!red.matches(&meta(&[("Red", TagColor::Red)], 3, "x")));
        assert!(!red.matches(&meta(&[], 2, "x")));
    }

    #[test]
    fn the_stream_round_trips_and_keeps_unknown_lines() {
        let m = meta(&[("Red", TagColor::Red), ("Project X", TagColor::None)], 4, "Ready; ask Ayşe");
        let text = encode_stream(&m, &["future line".into()]);
        assert_eq!(text, "gezik-meta 1\ntag 6 Red\ntag 0 Project X\nrating 4\ncomment Ready; ask Ayşe\nfuture line\n");
        let back = decode_stream(&text);
        assert_eq!(back.meta, m);
        assert_eq!(back.unknown, ["future line".into()]);
        assert!(back.writable);
        assert_eq!(decode_stream(&text.replace("\n", "\r\n")).meta, m, "CRLF reads too");
    }

    #[test]
    fn a_later_or_missing_version_is_read_but_not_written() {
        let later = decode_stream("gezik-meta 2\ntag 6 Red\n");
        assert_eq!(later.meta.tags, [Tag::new("Red", TagColor::Red)]);
        assert!(!later.writable);
        assert!(!decode_stream("tag 6 Red\n").writable);
        assert!(decode_stream("").writable && decode_stream(" \n\n").writable, "a blank stream is writable");
        let bad = decode_stream("gezik-meta 1\ntag x Red\nrating 9\ntag 1 a,b\n");
        assert!(bad.meta.is_empty());
        assert_eq!(bad.unknown.len(), 3, "bad lines are kept, not lost");
    }

    #[test]
    fn stars_and_explorers_ratings() {
        assert_eq!((stars_text(0), stars_text(3), stars_text(9)), (String::new(), "★★★☆☆".into(), "★★★★★".into()));
        let written = [1, 25, 50, 75, 99].map(stars_from_windows);
        assert_eq!(written, [1, 2, 3, 4, 5], "what Explorer writes reads back");
        assert_eq!([0, 12, 13, 37, 38, 62, 63, 87, 88].map(stars_from_windows), [0, 1, 2, 2, 3, 3, 4, 4, 5]);
        assert!(explorer_kind("a.JPG") && explorer_kind("x.tar.docx") && !explorer_kind("a.png"));
        assert!(!explorer_kind("jpg") && !explorer_kind("a.jpgx"));
    }

    #[test]
    fn xdg_tags_and_baloo_ratings() {
        assert_eq!(decode_xdg_tags("Red, Project X,,red"), ["Red".into(), "Project X".into()] as [Box<str>; 2]);
        assert_eq!(encode_xdg_tags(&[Tag::new("a", TagColor::Red), Tag::new("b c", TagColor::None)]), "a,b c");
        assert_eq!(decode_baloo_rating("7"), 4, "an odd number rounds up");
        assert_eq!(decode_baloo_rating("10"), 5);
        assert_eq!(decode_baloo_rating("0"), 0);
        assert_eq!(decode_baloo_rating("x"), 0);
        assert_eq!(decode_baloo_rating("99"), 5);
        assert_eq!(encode_baloo_rating(4), "8");
    }

    #[test]
    fn finder_info_keeps_its_other_bytes() {
        let mut info = vec![0u8; 32];
        info[0] = b'T';
        info[9] = 0x41; // other flag bits around the label
        let red = with_finder_info_color(Some(&info), TagColor::Red).unwrap();
        assert_eq!(red[9], 0x41 | 0x0C);
        assert_eq!(red[0], b'T');
        assert_eq!(finder_info_color(&red), TagColor::Red);
        let none = with_finder_info_color(Some(&red), TagColor::None).unwrap();
        assert_eq!(none, info, "only the label bits change");
        assert_eq!(with_finder_info_color(None, TagColor::None), None, "all zeros: the attribute goes");
        assert_eq!(with_finder_info_color(None, TagColor::Green).unwrap()[9], 0x04);
        assert_eq!(label_color(&[Tag::new("a", TagColor::Red), Tag::new("b", TagColor::None)]), TagColor::Red);
        assert_eq!(TagColor::from_number(9), TagColor::None);
    }
}
