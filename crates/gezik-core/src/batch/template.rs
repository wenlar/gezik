//! Name templates: text with fields in braces, `{name} {n:03}` or `{taken:%Y-%m-%d}`.

use super::date::check_format;

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
            '}' => return Err("a } without its {".to_owned()),
            '{' => {
                let mut field = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => field.push(c),
                        None => return Err(format!("{{{field} is not closed with }}")),
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
        Some(_) => Err(format!("{{{key}}} takes no format")),
        None => Ok(piece),
    };
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
                    _ => return Err(format!("{{n:{digits}}}: give 1 to 9 digits, like {{n:03}}")),
                },
            };
            Ok(Piece::Number { width })
        }
        "date" => Ok(Piece::Modified(date(arg)?)),
        "taken" => Ok(Piece::Taken(date(arg)?)),
        other => Err(format!("unknown field {{{other}}}")),
    }
}

/// Whether the pieces use the EXIF date (so it must be read).
pub fn uses_taken(pieces: &[Piece]) -> bool {
    pieces.iter().any(|piece| matches!(piece, Piece::Taken(_)))
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
    fn knows_when_exif_is_needed() {
        assert!(uses_taken(&parse("{taken}").unwrap()));
        assert!(!uses_taken(&parse("{date} {name}").unwrap()));
    }
}
