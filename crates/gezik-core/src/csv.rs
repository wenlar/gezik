//! CSV rows (RFC 4180): the Checksums layer's Export CSV (spec 11 §4.1) and list export (§11.2).

use std::borrow::Cow;

/// `text` as one field: a leading `=`, `+`, `-`, `@`, tab or CR gets a `'` first (a
/// spreadsheet would run a file named `=HYPERLINK(…)` as a formula); quoted (with `"` doubled)
/// when it holds the separator, a quote or a line break, or starts or ends with a space
/// (spreadsheets trim those otherwise).
pub fn field(text: &str, separator: char) -> Cow<'_, str> {
    let text: Cow<'_, str> = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        Cow::Owned(format!("'{text}"))
    } else {
        Cow::Borrowed(text)
    };
    let quoted = text.contains([separator, '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ');
    if quoted { Cow::Owned(format!("\"{}\"", text.replace('"', "\"\""))) } else { text }
}

/// Appends one row ending in CRLF.
pub fn push_row<S: AsRef<str>>(out: &mut String, fields: &[S], separator: char) {
    for (i, text) in fields.iter().enumerate() {
        if i > 0 {
            out.push(separator);
        }
        out.push_str(&field(text.as_ref(), separator));
    }
    out.push_str("\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_quoted_only_when_they_must_be() {
        assert_eq!(field("plain.txt", ','), "plain.txt");
        assert_eq!(field("a,b", ','), "\"a,b\"");
        assert_eq!(field("a;b", ','), "a;b");
        assert_eq!(field("a;b", ';'), "\"a;b\"");
        assert_eq!(field("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines", ','), "\"two\nlines\"");
        assert_eq!(field("cr\rhere", ','), "\"cr\rhere\"");
        assert_eq!(field(" lead", ','), "\" lead\"");
        assert_eq!(field("", ','), "");
    }

    #[test]
    fn a_field_never_starts_a_formula() {
        assert_eq!(field("=HYPERLINK(\"x\")", ','), "\"'=HYPERLINK(\"\"x\"\")\"");
        for (text, want) in [("+1", "'+1"), ("-x.txt", "'-x.txt"), ("@sum", "'@sum"), ("\tx", "'\tx")] {
            assert_eq!(field(text, ','), want);
        }
        assert_eq!(field("\rx", ','), "\"'\rx\"", "a CR is still quoted");
        assert_eq!(field("a=b", ','), "a=b", "only at the start");
    }

    #[test]
    fn rows_end_in_crlf() {
        let mut out = String::new();
        push_row(&mut out, &["Path", "Size"], ',');
        push_row(&mut out, &["a,b.txt".to_owned(), "3".to_owned()], ',');
        assert_eq!(out, "Path,Size\r\n\"a,b.txt\",3\r\n");
    }
}
