//! Tags in a folder's view (spec 12 §6): the `tag:` words of the filter bar, and the key a row
//! sorts and groups by. Pure: the app gives each row's tags and the `[tags] list`.

use crate::meta::{Tag, same_name};
use crate::sort::natural_cmp;

/// One `tag:` word: every one must hold (and).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagTerm {
    /// `tag:any`: some tag.
    Any,
    /// `tag:none`: no tag.
    None,
    /// `tag:Red`, `tag:"Project X"`.
    Named(Box<str>),
}

/// `text` without its `tag:` words, and those words. A word starts the text or follows a space
/// or `;`; its value is quoted (`tag:"Project X"`) or runs to the next space or `;`. A quoted
/// word (`"tag:x"`) stays a name to look for, without its quotes.
pub fn split(text: &str) -> (String, Vec<TagTerm>) {
    let (mut rest, mut terms) = (String::with_capacity(text.len()), Vec::new());
    let mut i = 0;
    while i < text.len() {
        let here = &text[i..];
        let starts = i == 0 || text[..i].ends_with([' ', ';']);
        let prefixed = |s: &str, p: &str| s.get(..p.len()).is_some_and(|head| head.eq_ignore_ascii_case(p));
        if starts && prefixed(here, "tag:") {
            let value = &here[4..];
            let (name, used) = match value.strip_prefix('"') {
                Some(quoted) => match quoted.find('"') {
                    Some(end) => (&quoted[..end], end + 2),
                    None => (quoted, quoted.len() + 1),
                },
                None => {
                    let end = value.find([' ', ';']).unwrap_or(value.len());
                    (&value[..end], end)
                }
            };
            match name.trim() {
                "" => {}
                n if n.eq_ignore_ascii_case("any") => terms.push(TagTerm::Any),
                n if n.eq_ignore_ascii_case("none") => terms.push(TagTerm::None),
                n => terms.push(TagTerm::Named(n.into())),
            }
            i += 4 + used;
        } else if starts && prefixed(here, "\"tag:") {
            let inner = &here[1..];
            let end = inner.find('"').unwrap_or(inner.len());
            rest.push_str(&inner[..end]);
            i += 1 + (end + 1).min(inner.len());
        } else {
            let c = here.chars().next().unwrap_or(' ');
            rest.push(c);
            i += c.len_utf8();
        }
    }
    (rest, terms)
}

/// Whether a row with `tags` passes every term.
pub fn matches(terms: &[TagTerm], tags: &[Tag]) -> bool {
    terms.iter().all(|term| match term {
        TagTerm::Any => !tags.is_empty(),
        TagTerm::None => tags.is_empty(),
        TagTerm::Named(name) => tags.iter().any(|t| same_name(&t.name, name)),
    })
}

/// What a row sorts and groups by: its front tag (spec 12 §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TagKey {
    /// A tag of the list: its place there, and its name as the list writes it.
    Listed {
        rank: u32,
        name: Box<str>,
    },
    /// Only tags not in the list: the first of them by name.
    Unlisted(Box<str>),
    Untagged,
    /// Not read yet.
    Pending,
}

impl TagKey {
    /// A group header's text (without the count).
    pub fn label(&self) -> String {
        match self {
            TagKey::Listed { name, .. } | TagKey::Unlisted(name) => format!("● {name}"),
            TagKey::Untagged => "No Tags".to_owned(),
            TagKey::Pending => "Reading tags…".to_owned(),
        }
    }
}

/// The front tag of a row with `tags` (`None`: not read yet): the one earliest in `list`, else
/// the smallest name in natural order.
pub fn tag_key(tags: Option<&[Tag]>, list: &[Tag]) -> TagKey {
    let Some(tags) = tags else { return TagKey::Pending };
    let listed =
        tags.iter().filter_map(|t| list.iter().position(|l| same_name(&l.name, &t.name))).min().map(|rank| {
            TagKey::Listed { rank: u32::try_from(rank).unwrap_or(u32::MAX), name: list[rank].name.clone() }
        });
    listed
        .or_else(|| tags.iter().map(|t| &t.name).min_by(|a, b| natural_cmp(a, b)).map(|n| TagKey::Unlisted(n.clone())))
        .unwrap_or(TagKey::Untagged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::TagColor;

    fn named(n: &str) -> TagTerm {
        TagTerm::Named(n.into())
    }

    #[test]
    fn tag_words_come_out_of_the_filter() {
        assert_eq!(split("*.jpg tag:Red"), ("*.jpg ".to_owned(), vec![named("Red")]));
        assert_eq!(split("tag:\"Project X\" report"), (" report".to_owned(), vec![named("Project X")]));
        assert_eq!(split("TAG:none;tag:any"), (";".to_owned(), vec![TagTerm::None, TagTerm::Any]));
        assert_eq!(split("tag:Red tag:Blue").1, [named("Red"), named("Blue")], "both must hold");
        assert_eq!(split("\"tag:x\""), ("tag:x".to_owned(), vec![]), "a quoted word is a name");
        assert_eq!(split("mytag:Red"), ("mytag:Red".to_owned(), vec![]), "only a word's start");
        assert_eq!(split("tag:"), (String::new(), vec![]), "half typed");
        assert_eq!(split("tag:\"Open"), (String::new(), vec![named("Open")]), "an open quote to the end");
        assert_eq!(split("ünïcode tag:Öncelikli"), ("ünïcode ".to_owned(), vec![named("Öncelikli")]));
    }

    #[test]
    fn terms_match_ignoring_case() {
        let tags = [Tag::new("Red", TagColor::Red), Tag::new("İş", TagColor::None)];
        assert!(matches(&[named("red"), named("iş")], &tags), "Turkish İ/i");
        assert!(!matches(&[named("Red"), named("Blue")], &tags), "and");
        assert!(matches(&[TagTerm::Any], &tags) && !matches(&[TagTerm::None], &tags));
        assert!(matches(&[TagTerm::None], &[]) && !matches(&[TagTerm::Any], &[]));
        assert!(matches(&[], &[]), "no terms: all pass");
    }

    #[test]
    fn the_front_tag_is_the_lists_earliest_else_the_smallest_name() {
        let list = [Tag::new("Red", TagColor::Red), Tag::new("Green", TagColor::Green)];
        let row =
            [Tag::new("zeta", TagColor::None), Tag::new("green", TagColor::None), Tag::new("RED", TagColor::None)];
        assert_eq!(tag_key(Some(&row), &list), TagKey::Listed { rank: 0, name: "Red".into() });
        let unlisted = [Tag::new("x10", TagColor::None), Tag::new("x9", TagColor::None)];
        assert_eq!(tag_key(Some(&unlisted), &list), TagKey::Unlisted("x9".into()), "natural order");
        assert_eq!(tag_key(Some(&[]), &list), TagKey::Untagged);
        assert_eq!(tag_key(None, &list), TagKey::Pending);
        assert_eq!(TagKey::Listed { rank: 0, name: "Red".into() }.label(), "● Red");
        assert_eq!(TagKey::Untagged.label(), "No Tags");
    }
}
