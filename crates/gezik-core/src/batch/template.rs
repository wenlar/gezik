//! Name templates: text with fields in braces, `{name} {n:03}` or `{taken:%Y-%m-%d}`, and the
//! metadata fields (spec 13 §9.2): `{artist|Unknown}`, `{track:02}`, `{duration}`.

use gezik_i18n::t;

use super::date::check_format;
use crate::fields::{self, FieldSet, ValueKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// The name so far (without its extension unless the extension is included).
    Name,
    /// The extension without its dot.
    Ext,
    /// The counter, at least `width` digits (zero-padded).
    Number {
        width: u8,
    },
    /// The name of the folder the item is in.
    Parent,
    /// The modified date, in this format.
    Modified(String),
    /// When the photo was taken (EXIF), else the modified date.
    Taken(String),
    /// The size, readable (`2.1 MB`).
    Size,
    /// A metadata field (`{track:02}`), its format and the text when it has no value
    /// (`{artist|Unknown}`).
    Field {
        id: fields::FieldId,
        arg: Option<String>,
        fallback: Option<String>,
    },
}

const DEFAULT_DATE: &str = "%Y-%m-%d";

/// The pieces of `text`; `{{` and `}}` are literal braces. The error names what is wrong.
pub fn parse(text: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '}' => return Err(t!("a {0} without its {1}", '}', '{')),
            '{' => {
                let mut field = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => field.push(c),
                        None => return Err(t!("{{{} is not closed with }}", field)),
                    }
                }
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                pieces.push(field_piece(&field)?);
            }
            c => literal.push(c),
        }
    }
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    Ok(pieces)
}

fn field_piece(field: &str) -> Result<Piece, String> {
    // `|` is not allowed in Windows names, so it never clashes with the text after it.
    let (field, fallback) = match field.split_once('|') {
        Some((field, fallback)) => (field, Some(fallback.to_owned())),
        None => (field, None),
    };
    let (key, arg) = match field.split_once(':') {
        Some((key, arg)) => (key.trim(), Some(arg)),
        None => (field.trim(), None),
    };
    let date = |arg: Option<&str>| -> Result<String, String> {
        let format = arg.unwrap_or(DEFAULT_DATE).to_owned();
        check_format(&format)?;
        Ok(format)
    };
    let no_arg = |piece: Piece| match arg {
        Some(_) => Err(t!("{{{}}} takes no format", key)),
        None => Ok(piece),
    };
    let builtin = matches!(key, "name" | "ext" | "parent" | "size" | "n" | "date" | "taken");
    if builtin && fallback.is_some() {
        return Err(t!("{{{}}} takes no | text", key));
    }
    match key {
        "name" => no_arg(Piece::Name),
        "ext" => no_arg(Piece::Ext),
        "parent" => no_arg(Piece::Parent),
        "size" => no_arg(Piece::Size),
        "n" => {
            let width = match arg {
                None => 1,
                Some(digits) => match digits.trim().parse::<u8>() {
                    Ok(width) if (1..=9).contains(&width) => width,
                    _ => return Err(t!("{{n:{}}}: give 1 to 9 digits, like {{n:03}}", digits)),
                },
            };
            Ok(Piece::Number { width })
        }
        "date" => Ok(Piece::Modified(date(arg)?)),
        "taken" => Ok(Piece::Taken(date(arg)?)),
        other => {
            let id = fields::by_key(other).ok_or_else(|| t!("unknown field {{{}}}", other))?;
            let arg = arg.map(str::trim);
            let ok = match (id.def().kind, arg) {
                (_, None) => true,
                (ValueKind::Int | ValueKind::Real, Some(digits)) => digits.parse::<u8>().is_ok_and(|n| n <= 9),
                (ValueKind::Date, Some(format)) => check_format(format).map(|()| true)?,
                (ValueKind::Text | ValueKind::Duration | ValueKind::Geo, Some(_)) => false,
            };
            if !ok {
                let arg = arg.unwrap_or_default();
                return Err(match id.def().kind {
                    // TRANSLATORS: {0} is a field's key in a name template ({artist}), {1} what was typed.
                    ValueKind::Int => t!("{{{0}:{1}}}: give 0 to 9 digits, like {{{0}:02}}", other, arg),
                    ValueKind::Real => t!("{{{0}:{1}}}: give 0 to 9 decimals, like {{{0}:1}}", other, arg),
                    _ => t!("{{{}}} takes no format", other),
                });
            }
            Ok(Piece::Field { id, arg: arg.map(str::to_owned), fallback })
        }
    }
}

/// The fields the pieces need read from the files (`{taken}` is the photo's date).
pub fn needed(pieces: &[Piece]) -> FieldSet {
    pieces.iter().fold(FieldSet::NONE, |set, piece| match piece {
        Piece::Taken(_) => set.with(FieldSet::of(&[fields::TAKEN])),
        Piece::Field { id, .. } => set.with(FieldSet::of(&[*id])),
        _ => set,
    })
}

/// The fields used with no `|` text: an item without one of them gets a warning.
pub fn bare(pieces: &[Piece]) -> FieldSet {
    pieces.iter().fold(FieldSet::NONE, |set, piece| match piece {
        Piece::Field { id, fallback: None, .. } => set.with(FieldSet::of(&[*id])),
        _ => set,
    })
}

/// A value from a file made safe in a name, the same on every system (spec 13 §9.2):
/// `/ \ : * ? " < > |` and control characters become `_`, at most 100 characters, no spaces or
/// dots at the ends, a value of only those (`..`) is `_`. A tag like `../../x` stays a name.
pub fn safe_part(value: &str) -> String {
    let unsafe_char = |c: char| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
    let cleaned: String = value.chars().take(100).map(|c| if unsafe_char(c) { '_' } else { c }).collect();
    let trimmed = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if trimmed.is_empty() && !cleaned.is_empty() { "_".to_owned() } else { trimmed.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_text() {
        assert_eq!(
            parse("{taken:%Y} - {name} {n:03}").unwrap(),
            [
                Piece::Taken("%Y".into()),
                Piece::Text(" - ".into()),
                Piece::Name,
                Piece::Text(" ".into()),
                Piece::Number { width: 3 },
            ]
        );
        assert_eq!(parse("{date}").unwrap(), [Piece::Modified("%Y-%m-%d".into())]);
        assert_eq!(parse("a{{b}}").unwrap(), [Piece::Text("a{b}".into())]);
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert_eq!(parse("{nmae}").unwrap_err(), "unknown field {nmae}");
        assert_eq!(parse("{n:0}").unwrap_err(), "{n:0}: give 1 to 9 digits, like {n:03}");
        assert_eq!(parse("{name").unwrap_err(), "{name is not closed with }");
        assert_eq!(parse("{date:%Q}").unwrap_err(), "unknown date part %Q");
        assert_eq!(parse("{ext:x}").unwrap_err(), "{ext} takes no format");
        assert!(parse("a}").is_err());
    }

    #[test]
    fn knows_which_fields_to_read() {
        use crate::fields::{ARTIST, TAKEN, TITLE, TRACK};
        assert_eq!(needed(&parse("{taken}").unwrap()), FieldSet::of(&[TAKEN]));
        assert!(needed(&parse("{date} {name}").unwrap()).is_empty());
        let pieces = parse("{track:02} {artist|Unknown} - {title}").unwrap();
        assert_eq!(needed(&pieces), FieldSet::of(&[TRACK, ARTIST, TITLE]));
        assert_eq!(bare(&pieces), FieldSet::of(&[TRACK, TITLE]));
    }

    #[test]
    fn metadata_fields_take_a_format_by_kind_and_a_fallback() {
        use crate::fields::{APERTURE, ARTIST, TRACK};
        assert_eq!(
            parse("{track:02}{artist|Unknown}").unwrap(),
            [
                Piece::Field { id: TRACK, arg: Some("02".into()), fallback: None },
                Piece::Field { id: ARTIST, arg: None, fallback: Some("Unknown".into()) },
            ]
        );
        assert_eq!(
            parse("{aperture:1}").unwrap(),
            [Piece::Field { id: APERTURE, arg: Some("1".into()), fallback: None }]
        );
        assert_eq!(parse("{taken:%Y}").unwrap(), [Piece::Taken("%Y".into())], "the photo date keeps its meaning");
        assert_eq!(parse("{artist:2}").unwrap_err(), "{artist} takes no format");
        assert_eq!(parse("{duration:2}").unwrap_err(), "{duration} takes no format");
        assert_eq!(parse("{track:x}").unwrap_err(), "{track:x}: give 0 to 9 digits, like {track:02}");
        assert_eq!(parse("{name|x}").unwrap_err(), "{name} takes no | text");
        assert_eq!(parse("{artst}").unwrap_err(), "unknown field {artst}");
    }

    #[test]
    fn values_are_made_safe_in_names() {
        assert_eq!(safe_part("../../x"), "_.._x");
        assert_eq!(safe_part("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j");
        assert_eq!(safe_part("..."), "_");
        assert_eq!(safe_part(" .Tarkan. "), "Tarkan");
        assert_eq!(safe_part("a\nb\u{7}"), "a_b_");
        assert_eq!(safe_part(""), "");
        assert_eq!(safe_part(&"ş".repeat(150)).chars().count(), 100);
    }
}
