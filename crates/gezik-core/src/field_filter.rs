//! Property words in the filter bar (spec 13 §8.3): `artist:tarkan`, `artist="Sezen Aksu"`,
//! `duration>3m`, `taken>=2024-05`, `camera:any`, `album:none`. Only the registry's keys are
//! words; anything else stays part of the name pattern. Pure: the app gives each row's fields.

use std::cmp::Ordering;

use crate::fields::{self, FieldId, FieldSet, Fields, Value};
use crate::pattern::fold_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `:` the shown value holds the text (case folded).
    Has,
    /// `=`
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

/// One property word: every one must hold (and).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldTerm {
    /// `camera:any`
    Any(FieldId),
    /// `album:none`
    None(FieldId),
    /// `artist:tarkan` (the text folded).
    Has(FieldId, String),
    /// `duration>3m`, `artist="Sezen Aksu"`.
    Cmp(FieldId, Op, Value),
}

impl FieldTerm {
    fn id(&self) -> FieldId {
        match self {
            FieldTerm::Any(id) | FieldTerm::None(id) | FieldTerm::Has(id, _) | FieldTerm::Cmp(id, ..) => *id,
        }
    }
}

/// The fields `terms` need read.
pub fn needed(terms: &[FieldTerm]) -> FieldSet {
    terms.iter().fold(FieldSet::NONE, |set, t| set.with(FieldSet::of(&[t.id()])))
}

/// The operator at the start of `text` and its length (longest first).
fn op_at(text: &str) -> Option<(Op, usize)> {
    [("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt), ("=", Op::Eq), (":", Op::Has)]
        .into_iter()
        .find(|(s, _)| text.starts_with(s))
        .map(|(s, op)| (op, s.len()))
}

/// `text` without its property words, and those words; the first word whose value does not
/// read is the error (`duration>3x: "3x" is not a duration (3m, 1:30)`). A word starts the text
/// or follows a space or `;`; its value is quoted or runs to the next space or `;`.
pub fn split(text: &str) -> Result<(String, Vec<FieldTerm>), String> {
    let (mut rest, mut terms) = (String::with_capacity(text.len()), Vec::new());
    let mut i = 0;
    while i < text.len() {
        let here = text.get(i..).unwrap_or_default();
        let starts = i == 0 || text.get(..i).is_some_and(|before| before.ends_with([' ', ';']));
        let key_len = here.find(|c: char| !(c.is_ascii_lowercase() || c == '-')).unwrap_or(here.len());
        let key = here.get(..key_len).unwrap_or_default();
        let word = starts.then(|| fields::by_key(key)).flatten().zip(here.get(key_len..).and_then(op_at));
        let Some((id, (op, op_len))) = word else {
            let c = here.chars().next().unwrap_or(' ');
            rest.push(c);
            i += c.len_utf8();
            continue;
        };
        let value = here.get(key_len + op_len..).unwrap_or_default();
        let (raw, used) = match value.strip_prefix('"') {
            Some(quoted) => match quoted.find('"') {
                Some(end) => (quoted.get(..end).unwrap_or_default(), end + 2),
                None => (quoted, quoted.len() + 1),
            },
            None => {
                let end = value.find([' ', ';']).unwrap_or(value.len());
                (value.get(..end).unwrap_or_default(), end)
            }
        };
        i += key_len + op_len + used;
        let raw = raw.trim();
        if raw.is_empty() {
            // Half typed (`artist:`): nothing yet.
            continue;
        }
        let kind = id.def().kind;
        terms.push(match op {
            Op::Has if raw.eq_ignore_ascii_case("any") => FieldTerm::Any(id),
            Op::Has if raw.eq_ignore_ascii_case("none") => FieldTerm::None(id),
            Op::Has => FieldTerm::Has(id, fold_text(raw)),
            op => {
                let value = fields::parse_value(kind, raw).map_err(|e| {
                    let word = here.get(..key_len + op_len + used).unwrap_or(here);
                    format!("{}: {e}", word.trim())
                })?;
                FieldTerm::Cmp(id, op, value)
            }
        });
    }
    Ok((rest, terms))
}

/// Whether a row with `values` passes every term.
pub fn matches(terms: &[FieldTerm], values: &Fields) -> bool {
    terms.iter().all(|term| match term {
        FieldTerm::Any(id) => values.get(*id).is_some(),
        FieldTerm::None(id) => values.get(*id).is_none(),
        FieldTerm::Has(id, text) => values.text(*id).is_some_and(|shown| fold_text(&shown).contains(text.as_str())),
        FieldTerm::Cmp(id, op, wanted) => values.get(*id).is_some_and(|v| holds(*op, v, wanted)),
    })
}

fn holds(op: Op, value: &Value, wanted: &Value) -> bool {
    // A date typed in part (`2024`, `2024-05`) is equal to every day it covers.
    if let (Op::Eq, Value::Date(v), Value::Date(w)) = (op, value, wanted) {
        return v.year == w.year && (w.month == 0 || v.month == w.month) && (w.day == 0 || v.day == w.day);
    }
    let order = fields::compare(Some(value), Some(wanted));
    match op {
        Op::Has | Op::Eq => order == Ordering::Equal,
        Op::Lt => order == Ordering::Less,
        Op::Le => order != Ordering::Greater,
        Op::Gt => order == Ordering::Greater,
        Op::Ge => order != Ordering::Less,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::date::DateParts;
    use crate::fields::{ALBUM, ARTIST, CAMERA, DURATION, TAKEN, WIDTH, YEAR};

    fn song() -> Fields {
        Fields::new(vec![
            (ARTIST, Value::Text("Sezen Aksu".into())),
            (DURATION, Value::Duration(205_000)),
            (YEAR, Value::Int(1991)),
            (TAKEN, Value::Date(DateParts { year: 2024, month: 5, day: 3, ..DateParts::default() })),
        ])
    }

    fn pass(text: &str) -> bool {
        let (_, terms) = split(text).unwrap();
        matches(&terms, &song())
    }

    #[test]
    fn words_leave_the_name_pattern() {
        let (rest, terms) = split("*.mp3 artist:tarkan duration>3m").unwrap();
        assert_eq!(rest, "*.mp3  ");
        assert_eq!(
            terms,
            [FieldTerm::Has(ARTIST, "tarkan".into()), FieldTerm::Cmp(DURATION, Op::Gt, Value::Duration(180_000))]
        );
        assert_eq!(needed(&terms), FieldSet::of(&[ARTIST, DURATION]));
        let (rest, terms) = split("foo:bar artist").unwrap();
        assert_eq!((rest.as_str(), terms.len()), ("foo:bar artist", 0), "not a key: a name");
        let (rest, terms) = split("xartist:a").unwrap();
        assert_eq!((rest.as_str(), terms.len()), ("xartist:a", 0), "a word starts the text or follows a space");
        let (_, terms) = split("artist=\"Sezen Aksu\";camera:any album:none").unwrap();
        assert_eq!(
            terms,
            [
                FieldTerm::Cmp(ARTIST, Op::Eq, Value::Text("Sezen Aksu".into())),
                FieldTerm::Any(CAMERA),
                FieldTerm::None(ALBUM)
            ]
        );
        assert_eq!(split("artist:").unwrap().1, [], "half typed");
    }

    #[test]
    fn bad_values_say_so() {
        let err = split("duration>3x").unwrap_err();
        assert!(err.starts_with("duration>3x: \"3x\" is not a duration"), "{err}");
        assert!(split("width>=wide").is_err());
        assert!(split("taken>=2024-13").is_err());
    }

    #[test]
    fn each_operator_holds() {
        assert!(pass("artist:aksu") && pass("artist:SEZEN") && !pass("artist:tarkan"));
        assert!(pass("artist=\"sezen aksu\"") && !pass("artist=sezen"));
        assert!(pass("duration>3m") && !pass("duration<3m") && pass("duration<=3:25") && pass("duration>=205"));
        assert!(pass("year<2000") && !pass("year>2000") && pass("year=1991") && pass("year:199"));
        assert!(pass("taken>=2024-05") && !pass("taken<2024") && pass("taken=2024") && pass("taken=2024-05"));
        assert!(!pass("taken=2024-06"));
        assert!(pass("artist:any") && pass("album:none") && !pass("album:any") && !pass("artist:none"));
        assert!(!pass("width>0"), "no value: not more, not less");
        assert!(pass("duration>3m artist:sezen") && !pass("duration>3m artist:x"), "and");
    }
}
