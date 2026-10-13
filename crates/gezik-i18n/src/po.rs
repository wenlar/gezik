//! gettext `.po` files (spec 15 §11.1): read with their line numbers, written back by
//! `gezik-lang`. A reader of the subset translation tools write: `msgctxt`, `msgid`,
//! `msgid_plural`, `msgstr`, `msgstr[n]`, multi-line strings, the `#,` flags, `#.` notes, `#:`
//! references and `#~` obsolete entries. Also used by `build.rs`, so only `std`.

use std::collections::HashMap;

/// A larger file is refused (spec 15 §15: a dropped file may be anything).
pub const MAX_BYTES: usize = 4 << 20;
/// More entries than this are refused.
pub const MAX_ENTRIES: usize = 20_000;

/// One text and its translation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// `msgctxt`, empty when none.
    pub context: String,
    pub id: String,
    pub plural: Option<String>,
    /// `msgstr`, or `msgstr[0]`, `msgstr[1]` … in order.
    pub strs: Vec<String>,
    /// `#, fuzzy`: machine translated, not reviewed (spec 15 §11.3).
    pub fuzzy: bool,
    /// `#.` lines: notes for translators.
    pub notes: Vec<String>,
    /// `#:` lines: where the text is used.
    pub refs: Vec<String>,
    /// The line of its `msgid`.
    pub line: usize,
}

impl Entry {
    pub fn key(&self) -> String {
        key(&self.context, &self.id)
    }

    /// Every form has a translation.
    pub fn translated(&self) -> bool {
        !self.strs.is_empty() && self.strs.iter().all(|s| !s.is_empty())
    }
}

/// The catalogue key of a text: its context, `\x04` and the source (gettext's).
pub fn key(context: &str, id: &str) -> String {
    if context.is_empty() { id.to_owned() } else { format!("{context}\u{4}{id}") }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoFile {
    /// The header entry's text (`Language: tr\n…`).
    pub header: String,
    pub entries: Vec<Entry>,
    /// The `#~` lines, as written.
    pub obsolete: Vec<String>,
}

impl PoFile {
    /// A header field (`Plural-Forms`), trimmed; `None` when missing or empty.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.header.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim().eq_ignore_ascii_case(name) && !value.trim().is_empty()).then(|| value.trim())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for PoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

fn error<T>(line: usize, message: impl Into<String>) -> Result<T, PoError> {
    Err(PoError { line, message: message.into() })
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    None,
    Context,
    Id,
    Plural,
    Str(usize),
}

#[derive(Default)]
struct Parser {
    file: PoFile,
    entry: Entry,
    has_context: bool,
    has_id: bool,
    /// The line the entry began on (its first keyword).
    start: usize,
    header_seen: bool,
    seen: HashMap<String, usize>,
}

impl Parser {
    fn started(&self) -> bool {
        self.has_context || self.has_id
    }

    /// The entry so far is complete: kept (or taken as the header).
    fn finish(&mut self) -> Result<(), PoError> {
        if !self.started() {
            // Comments with no entry after them (at the end of the file) are dropped.
            self.entry = Entry::default();
            return Ok(());
        }
        let entry = std::mem::take(&mut self.entry);
        let (start, has_id) = (self.start, self.has_id);
        (self.has_context, self.has_id) = (false, false);
        if !has_id {
            return error(start, "msgctxt without msgid");
        }
        if entry.strs.is_empty() {
            return error(entry.line, "msgid without msgstr");
        }
        if entry.id.is_empty() && entry.context.is_empty() {
            if self.header_seen {
                return error(entry.line, "a second header (empty msgid)");
            }
            self.header_seen = true;
            self.file.header = entry.strs.into_iter().next().unwrap_or_default();
            return Ok(());
        }
        if let Some(first) = self.seen.insert(entry.key(), entry.line) {
            return error(entry.line, format!("\"{}\" is here twice (first on line {first})", entry.id));
        }
        if self.file.entries.len() >= MAX_ENTRIES {
            return error(entry.line, format!("more than {MAX_ENTRIES} entries"));
        }
        self.file.entries.push(entry);
        Ok(())
    }
}

/// Reads a `.po` (or `.pot`) file; the first problem with its line.
pub fn parse(text: &str) -> Result<PoFile, PoError> {
    if text.len() > MAX_BYTES {
        return error(1, format!("larger than {} MB", MAX_BYTES >> 20));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p = Parser::default();
    let mut field = Field::None;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let l = raw.trim();
        if l.is_empty() {
            p.finish()?;
            field = Field::None;
            continue;
        }
        if l.starts_with("#~") {
            p.finish()?;
            field = Field::None;
            p.file.obsolete.push(raw.to_owned());
            continue;
        }
        if let Some(comment) = l.strip_prefix('#') {
            // Comments belong to the next entry.
            if !p.entry.strs.is_empty() {
                p.finish()?;
            }
            field = Field::None;
            let rest = comment.get(1..).unwrap_or_default().trim();
            match comment.chars().next() {
                Some(',') => p.entry.fuzzy |= rest.split(',').any(|flag| flag.trim() == "fuzzy"),
                Some('.') => p.entry.notes.push(rest.to_owned()),
                Some(':') => p.entry.refs.push(rest.to_owned()),
                _ => {}
            }
            continue;
        }
        if l.starts_with('"') {
            let more = unquote(l, line)?;
            let target = match field {
                Field::None => return error(line, "a string without a keyword before it"),
                Field::Context => &mut p.entry.context,
                Field::Id => &mut p.entry.id,
                Field::Plural => p.entry.plural.get_or_insert_default(),
                Field::Str(n) => &mut p.entry.strs[n],
            };
            target.push_str(&more);
            continue;
        }
        let (keyword, rest) = l.split_once([' ', '\t']).unwrap_or((l, ""));
        let value = unquote(rest.trim(), line)?;
        match keyword {
            "msgctxt" => {
                if p.has_id {
                    p.finish()?;
                }
                if p.has_context {
                    return error(line, "msgctxt twice");
                }
                if !p.started() {
                    p.start = line;
                }
                (p.entry.context, p.has_context, field) = (value, true, Field::Context);
            }
            "msgid" => {
                if p.has_id {
                    p.finish()?;
                }
                if !p.started() {
                    p.start = line;
                }
                (p.entry.id, p.entry.line, p.has_id, field) = (value, line, true, Field::Id);
            }
            "msgid_plural" => {
                if !p.has_id || p.entry.plural.is_some() || !p.entry.strs.is_empty() {
                    return error(line, "msgid_plural out of place");
                }
                (p.entry.plural, field) = (Some(value), Field::Plural);
            }
            "msgstr" => {
                if !p.has_id || p.entry.plural.is_some() || !p.entry.strs.is_empty() {
                    return error(line, "msgstr out of place");
                }
                p.entry.strs.push(value);
                field = Field::Str(0);
            }
            _ => {
                let index = keyword.strip_prefix("msgstr[").and_then(|k| k.strip_suffix(']'));
                let Some(index) = index else { return error(line, format!("unknown keyword \"{keyword}\"")) };
                let n = p.entry.strs.len();
                if !p.has_id || p.entry.plural.is_none() || index.parse::<usize>().ok() != Some(n) {
                    return error(line, format!("{keyword} out of place (msgstr[{n}] expected)"));
                }
                p.entry.strs.push(value);
                field = Field::Str(n);
            }
        }
    }
    p.finish()?;
    Ok(p.file)
}

/// `"text"` without its quotes and escapes.
fn unquote(s: &str, line: usize) -> Result<String, PoError> {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).filter(|_| s.len() >= 2) else {
        return error(line, "expected a string in double quotes");
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(match chars.next() {
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('\\') => '\\',
                Some('"') => '"',
                Some(other) => return error(line, format!("unknown escape \\{other}")),
                None => return error(line, "a string ending in \\"),
            }),
            '"' => return error(line, "a \" inside the string must be written \\\""),
            c => out.push(c),
        }
    }
    Ok(out)
}

/// `text` as a `.po` string: one line, or after each `\n` a line of its own (as gettext writes).
pub fn quote(text: &str) -> String {
    let escape = |part: &str| {
        let mut out = String::from("\"");
        for c in part.chars() {
            match c {
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    };
    let body = text.strip_suffix('\n').unwrap_or(text);
    if !body.contains('\n') {
        return escape(text);
    }
    let mut out = String::from("\"\"");
    for part in text.split_inclusive('\n') {
        out.push('\n');
        out.push_str(&escape(part));
    }
    out
}

/// Writes `file` as a `.po` file.
pub fn write(file: &PoFile) -> String {
    let mut out = format!("msgid \"\"\nmsgstr {}\n", quote(&file.header));
    for e in &file.entries {
        out.push('\n');
        for note in &e.notes {
            out.push_str(&format!("#. {note}\n"));
        }
        for r in &e.refs {
            out.push_str(&format!("#: {r}\n"));
        }
        if e.fuzzy {
            out.push_str("#, fuzzy\n");
        }
        if !e.context.is_empty() {
            out.push_str(&format!("msgctxt {}\n", quote(&e.context)));
        }
        out.push_str(&format!("msgid {}\n", quote(&e.id)));
        match &e.plural {
            Some(plural) => {
                out.push_str(&format!("msgid_plural {}\n", quote(plural)));
                for (i, s) in e.strs.iter().enumerate() {
                    out.push_str(&format!("msgstr[{i}] {}\n", quote(s)));
                }
            }
            None => out.push_str(&format!("msgstr {}\n", quote(e.strs.first().map_or("", String::as_str)))),
        }
    }
    if !file.obsolete.is_empty() {
        out.push('\n');
        for line in &file.obsolete {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}# A translator's comment\r\nmsgid \"\"\r\nmsgstr \"\"\r\n\"Language: tr\\n\"\r\n\"Plural-Forms: nplurals=1; plural=0;\\n\"\r\n\r\n#. TRANSLATORS: a button\r\n#: crates/gezik/src/a.rs\r\n#, fuzzy, c-format\r\nmsgctxt \"menu\"\r\nmsgid \"Open\"\r\nmsgstr \"Aç\"\r\n\r\nmsgid \"{n} file\"\r\nmsgid_plural \"{n} files\"\r\nmsgstr[0] \"{n} dosya\"\r\n\r\nmsgid \"\"\r\n\"Two\\n\"\r\n\"lines \\\"quoted\\\" \\\\ \\t\"\r\nmsgstr \"İki\\nsatır\"\r\n\r\n#~ msgid \"Gone\"\r\n#~ msgstr \"Gitti\"\r\n";

    #[test]
    fn reads_headers_contexts_plurals_escapes_flags_bom_and_crlf() {
        let file = parse(SAMPLE).unwrap();
        assert_eq!(file.field("language"), Some("tr"));
        assert_eq!(file.field("Plural-Forms"), Some("nplurals=1; plural=0;"));
        assert_eq!(file.field("X-Gezik-Name"), None);
        let [open, files, two] = &file.entries[..] else { panic!("{file:?}") };
        assert_eq!(
            (open.context.as_str(), open.id.as_str(), open.strs.as_slice()),
            ("menu", "Open", &["Aç".to_owned()][..])
        );
        assert!(open.fuzzy && open.translated());
        assert_eq!(open.notes, ["TRANSLATORS: a button"]);
        assert_eq!(open.refs, ["crates/gezik/src/a.rs"]);
        assert_eq!(open.key(), "menu\u{4}Open");
        assert_eq!(open.line, 11);
        assert_eq!(files.plural.as_deref(), Some("{n} files"));
        assert_eq!(files.strs, ["{n} dosya"]);
        assert!(!files.fuzzy);
        assert_eq!(two.id, "Two\nlines \"quoted\" \\ \t");
        assert_eq!(two.strs, ["İki\nsatır"]);
        assert_eq!(file.obsolete, ["#~ msgid \"Gone\"", "#~ msgstr \"Gitti\""]);
    }

    #[test]
    fn writes_what_it_reads() {
        let file = parse(SAMPLE).unwrap();
        let again = parse(&write(&file)).unwrap();
        assert_eq!(again.header, file.header);
        let strip = |f: &PoFile| f.entries.iter().map(|e| Entry { line: 0, ..e.clone() }).collect::<Vec<_>>();
        assert_eq!(strip(&again), strip(&file));
        assert_eq!(again.obsolete, file.obsolete);
        assert_eq!(quote("a\nb\n"), "\"\"\n\"a\\n\"\n\"b\\n\"");
        assert_eq!(quote("one\n"), "\"one\\n\"");
    }

    #[test]
    fn problems_come_with_their_line() {
        let cases = [
            ("msgid \"a\"\nmsgstr \"b\n", 2, "double quotes"),
            ("msgid \"a\"\nmsgstr \"b\\q\"\n", 2, "unknown escape"),
            ("msgid \"a\"\nmsgstr \"b\"c\"\n", 2, "inside the string"),
            ("msgid \"a\"\n\nmsgid \"b\"\nmsgstr \"\"\n", 1, "without msgstr"),
            ("msgctxt \"x\"\n\n", 1, "msgctxt without msgid"),
            ("msgid \"a\"\nmsgstr[0] \"b\"\n", 2, "out of place"),
            ("msgid \"a\"\nmsgid_plural \"as\"\nmsgstr[1] \"b\"\n", 3, "msgstr[0] expected"),
            ("\"stray\"\n", 1, "without a keyword"),
            ("msgid \"a\"\nmsgstr \"b\"\nmsgfoo \"c\"\n", 3, "unknown keyword"),
            ("msgid \"a\"\nmsgstr \"b\"\n\nmsgid \"a\"\nmsgstr \"c\"\n", 4, "twice (first on line 1)"),
            ("msgid \"\"\nmsgstr \"\"\n\nmsgid \"\"\nmsgstr \"\"\n", 4, "second header"),
        ];
        for (text, line, words) in cases {
            let err = parse(text).unwrap_err();
            assert_eq!(err.line, line, "{text:?}: {err}");
            assert!(err.message.contains(words), "{text:?}: {err}");
        }
        assert!(parse(&"x".repeat(MAX_BYTES + 1)).unwrap_err().message.contains("4 MB"));
        let many: String = (0..=MAX_ENTRIES).map(|i| format!("msgid \"{i}\"\nmsgstr \"\"\n\n")).collect();
        assert!(parse(&many).unwrap_err().message.contains("more than"));
    }

    #[test]
    fn mangled_files_never_panic() {
        // Every cut and every byte swapped for a troublesome one: an error or a file, no panic.
        let bytes = SAMPLE.as_bytes();
        for cut in 0..bytes.len() {
            if let Ok(text) = std::str::from_utf8(&bytes[..cut]) {
                let _ = parse(text);
            }
            for &bad in b"\"\\\n#[ " {
                let mut copy = bytes.to_vec();
                copy[cut] = bad;
                if let Ok(text) = std::str::from_utf8(&copy) {
                    let _ = parse(text);
                }
            }
        }
        assert_eq!(parse("").unwrap(), PoFile::default());
        assert_eq!(parse("# only a comment\n").unwrap(), PoFile::default());
    }
}
