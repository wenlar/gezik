//! `gezik-lang`: the translators' tool (spec 15 §11.2), a developer tool not built into Gezik.
//!
//! ```text
//! cargo run -p gezik-i18n --bin gezik-lang -- extract        # writes lang/gezik.pot
//! cargo run -p gezik-i18n --bin gezik-lang -- update         # fits every lang/*.po to it
//! cargo run -p gezik-i18n --bin gezik-lang -- check [file…]  # counts and problems
//! ```
//!
//! `extract` reads `t!`, `tn!` and `tx!` in the crates' `src/` (not this crate's, not after a
//! file's `#[cfg(test)]`, not in comments; a `// TRANSLATORS:` comment above is a note for the
//! translators) and `@tr` in `crates/gezik/ui`, then the settings window's titles and
//! descriptions from the settings template (spec 15 §7). Order: by file path, then first use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gezik_i18n::po::{self, Entry, PoFile};
use gezik_i18n::{blob, check};

/// The settings window's fields and the template's lines, as gezik-config reads them.
#[allow(dead_code)]
#[path = "../../../gezik-config/src/schema/table.rs"]
mod table;

/// The settings template, whose comments are the window's descriptions.
const TEMPLATE: &str = "crates/gezik-config/templates/settings.toml";

const HEADER: &str = "Project-Id-Version: Gezik\nMIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\n\
Content-Transfer-Encoding: 8bit\nPlural-Forms: nplurals=2; plural=(n != 1);\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lang_dir(root: &Path) -> PathBuf {
    root.join("crates/gezik-i18n/lang")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = root();
    let outcome = match args.first().map(String::as_str) {
        Some("extract") => extract(&root).and_then(|pot| write(&lang_dir(&root).join("gezik.pot"), &pot)),
        Some("update") => update_all(&root),
        Some("check") => check_files(&root, &args[1..]),
        _ => Err("usage: gezik-lang extract | update | check [file.po …]".to_owned()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("gezik-lang: {why}");
            ExitCode::FAILURE
        }
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// One use of a text in the code.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    context: String,
    id: String,
    plural: Option<String>,
    note: Option<String>,
}

/// Every `.rs` under the crates' `src/` (not this crate) and `.slint` under `crates/gezik/ui`,
/// as paths relative to `root` with `/`, sorted.
fn sources(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    let Ok(crates) = std::fs::read_dir(root.join("crates")) else { return Vec::new() };
    for krate in crates.filter_map(Result::ok).filter(|c| c.file_name() != "gezik-i18n") {
        walk(&krate.path().join("src"), &mut files);
    }
    walk(&root.join("crates/gezik/ui"), &mut files);
    let mut out: Vec<String> = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "rs" || e == "slint"))
        .filter_map(|p| p.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    out.sort();
    out
}

/// The template (`gezik.pot`) the sources under `root` make.
fn extract(root: &Path) -> Result<String, String> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut files: Vec<(String, Vec<Found>)> = Vec::new();
    for file in sources(root) {
        let text = read(&root.join(&file))?;
        let found = if file.ends_with(".slint") { scan_slint(&text) } else { scan_rust(&text) };
        files.push((file.clone(), found.map_err(|why| format!("{file}: {why}"))?));
    }
    files.push((TEMPLATE.to_owned(), settings_texts(&read(&root.join(TEMPLATE))?)));
    for (file, found) in files {
        for f in found {
            let key = po::key(&f.context, &f.id);
            let i = *at.entry(key).or_insert_with(|| {
                let strs = vec![String::new(); if f.plural.is_some() { 2 } else { 1 }];
                entries.push(Entry {
                    context: f.context.clone(),
                    id: f.id.clone(),
                    plural: f.plural.clone(),
                    strs,
                    ..Entry::default()
                });
                entries.len() - 1
            });
            let entry = &mut entries[i];
            if entry.plural != f.plural {
                return Err(format!(
                    "{file}: \"{}\" is used with two plurals ({:?}, {:?})",
                    f.id, entry.plural, f.plural
                ));
            }
            if !entry.refs.contains(&file) {
                entry.refs.push(file.clone());
            }
            if let Some(note) = f.note.filter(|n| !entry.notes.contains(n)) {
                entry.notes.push(note);
            }
        }
    }
    Ok(po::write(&PoFile { header: HEADER.to_owned(), entries, obsolete: Vec::new() }))
}

/// Each settings field's title and description (the template's comment) in its `setting:`
/// context, as the settings window shows them.
fn settings_texts(template: &str) -> Vec<Found> {
    let lines = table::template_lines(template);
    let mut out = Vec::new();
    for field in table::FIELDS {
        let context = field.context();
        out.push(Found { context: context.clone(), id: field.title(), plural: None, note: None });
        let description = table::line_of(&lines, field).map(table::description).unwrap_or_default();
        if !description.is_empty() {
            out.push(Found { context, id: description, plural: None, note: None });
        }
    }
    out
}

/// A string literal at the start of `s` (after spaces): its text and the rest after it.
/// Rust's (`"…"`, `r"…"`, `r#"…"#`) or, with `slint`, Slint's.
fn literal(s: &str, slint: bool) -> Option<(String, &str)> {
    let s = s.trim_start();
    if !slint && let Some(raw) = s.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let body = raw[hashes..].strip_prefix('"')?;
        let close = format!("\"{}", "#".repeat(hashes));
        let end = body.find(&close)?;
        return Some((body[..end].to_owned(), &body[end + close.len()..]));
    }
    let body = s.strip_prefix('"')?;
    let mut out = String::new();
    let mut j = 0;
    while let Some(c) = body[j..].chars().next() {
        j += c.len_utf8();
        match c {
            '"' => return Some((out, &body[j..])),
            '\\' => {
                let e = body[j..].chars().next()?;
                j += e.len_utf8();
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '0' => out.push('\0'),
                    '\\' | '"' | '\'' => out.push(e),
                    'x' if !slint => {
                        out.push(char::from(u8::from_str_radix(body.get(j..j + 2)?, 16).ok()?));
                        j += 2;
                    }
                    'u' => {
                        let end = j + body[j..].find('}')?;
                        out.push(char::from_u32(u32::from_str_radix(body.get(j + 1..end)?, 16).ok()?)?);
                        j = end + 1;
                    }
                    // Slint's `\{expr}`: to its closing brace, the strings in it skipped.
                    '{' if slint => {
                        let mut depth = 1;
                        while depth > 0 {
                            let c = body[j..].chars().next()?;
                            if c == '"' {
                                let (_, after) = literal(&body[j..], true)?;
                                j = body.len() - after.len();
                                continue;
                            }
                            depth += i32::from(c == '{') - i32::from(c == '}');
                            j += c.len_utf8();
                        }
                    }
                    // A line continuation: the line break and the next line's indent go.
                    '\n' if !slint => j = body.len() - body[j..].trim_start().len(),
                    _ => return None,
                }
            }
            c => out.push(c),
        }
    }
    None
}

/// The `t!`, `tn!` and `tx!` of a Rust file, until its `#[cfg(test)]` test module.
fn scan_rust(text: &str) -> Result<Vec<Found>, String> {
    scan(&text[..test_module(text)], false, &["t!(", "tn!(", "tx!("])
}

/// Where a file's `#[cfg(test)] mod …` starts (other attributes may come between); its length
/// if it has none. A `#[cfg(test)]` on a single item or on `mod tests;` (a file of its own)
/// does not end the file's texts.
fn test_module(text: &str) -> usize {
    let mut from = 0;
    while let Some(found) = text[from..].find("\n#[cfg(test)]") {
        let at = from + found;
        let mut lines = text[at + 1..].lines().skip(1).map(str::trim_start);
        let module = |l: &str| {
            let l = l.strip_prefix("pub(crate) ").or_else(|| l.strip_prefix("pub ")).unwrap_or(l);
            l.starts_with("mod ") && !l.trim_end().ends_with(';')
        };
        if lines.find(|l| !l.starts_with("#[")).is_some_and(module) {
            return at;
        }
        from = at + 1;
    }
    text.len()
}

/// The `@tr(…)` of a Slint file.
fn scan_slint(text: &str) -> Result<Vec<Found>, String> {
    scan(text, true, &["@tr("])
}

/// The marked texts of `text`, skipping strings and comments.
fn scan(text: &str, slint: bool, marks: &[&str]) -> Result<Vec<Found>, String> {
    let mut out = Vec::new();
    let mut note: Option<String> = None;
    let mut i = 0;
    let line = |at: usize| text[..at].matches('\n').count() + 1;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap_or_default();
        if rest.starts_with("//") {
            let end = rest.find('\n').unwrap_or(rest.len());
            if let Some(n) = rest[2..end].trim().strip_prefix("TRANSLATORS:") {
                note = Some(n.trim().to_owned());
            }
            i += end;
            continue;
        }
        if rest.starts_with("/*") {
            i += rest.find("*/").map_or(rest.len(), |e| e + 2);
            continue;
        }
        // A raw string (`r"…"`, `br#"…"#`), not a name ending in r.
        let raw = !slint && (rest.starts_with("r\"") || rest.starts_with("r#")) && {
            let before = text[..i].strip_suffix('b').unwrap_or(&text[..i]);
            !ident_before(before, before.len())
        };
        if c == '"' || raw {
            // One that does not read (a byte string's escape) is stepped over.
            i = literal(rest, slint).map_or(i + 1, |(_, after)| text.len() - after.len());
            continue;
        }
        if c == '\'' && !slint {
            // A char literal ('"', '\'', '\u{2026}'); a lifetime ('a) is left alone.
            let inner = &rest[1..];
            let len = match inner.strip_prefix('\\') {
                Some(escaped) => escaped.get(1..).and_then(|e| e.find('\'')).map(|e| e + 2),
                None => {
                    let mut chars = inner.chars();
                    chars.next().filter(|_| chars.next() == Some('\'')).map(char::len_utf8)
                }
            };
            i += 1 + len.map_or(0, |l| l + 1);
            continue;
        }
        // A macro's pattern (`tx!($label:literal)`) is not a use.
        let pattern = |m: &str| !slint && rest[m.len()..].starts_with('$');
        if let Some(mark) =
            marks.iter().find(|m| rest.starts_with(**m)).filter(|m| slint || !(ident_before(text, i) || pattern(m)))
        {
            let start = line(i);
            let at = &rest[mark.len()..];
            let bad = || format!("line {start}: {} needs string literals", mark.trim_end_matches('('));
            let (first, mut after) = literal(at, slint).ok_or_else(bad)?;
            let (mut context, mut id) = (String::new(), first);
            if let Some(next) = after.trim_start().strip_prefix("=>") {
                let (text, rest) = literal(next, slint).ok_or_else(bad)?;
                (context, id, after) = (id, text, rest);
            }
            let mut plural = None;
            if *mark == "tn!(" || slint {
                if let Some(next) = after.trim_start().strip_prefix('|') {
                    let (text, rest) = literal(next, slint).ok_or_else(bad)?;
                    (plural, after) = (Some(text), rest);
                } else if *mark == "tn!(" {
                    return Err(bad());
                }
            }
            out.push(Found { context, id, plural, note: note.take() });
            i = text.len() - after.len();
            continue;
        }
        i += c.len_utf8();
    }
    Ok(out)
}

/// Whether the character before `i` is part of a name (`format!(` is not `t!(`).
fn ident_before(text: &str, i: usize) -> bool {
    text[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Fits every `lang/*.po` to the template.
fn update_all(root: &Path) -> Result<(), String> {
    let dir = lang_dir(root);
    let pot = po::parse(&read(&dir.join("gezik.pot"))?).map_err(|e| format!("gezik.pot {e}"))?;
    for path in po_files(&dir) {
        let old = po::parse(&read(&path)?).map_err(|e| format!("{} {e}", path.display()))?;
        write(&path, &po::write(&update(&pot, &old)))?;
    }
    Ok(())
}

/// `old` fitted to `pot`: its order, notes and references; a new text empty; a removed one at
/// the end as `#~`. A translation whose plural changed is dropped (English rather than wrong).
fn update(pot: &PoFile, old: &PoFile) -> PoFile {
    let mut kept: HashMap<String, &Entry> = old.entries.iter().map(|e| (e.key(), e)).collect();
    let entries = pot
        .entries
        .iter()
        .map(|new| {
            let mut entry = Entry { line: 0, ..new.clone() };
            if let Some(was) = kept.remove(&new.key()).filter(|was| was.plural == new.plural && was.translated()) {
                entry.strs.clone_from(&was.strs);
                entry.fuzzy = was.fuzzy;
            }
            entry
        })
        .collect();
    let mut gone: Vec<&Entry> = kept.into_values().filter(|e| e.translated()).collect();
    gone.sort_by_key(|e| e.line);
    let mut obsolete = Vec::new();
    for e in gone {
        let mut one = PoFile::default();
        one.entries.push(Entry { notes: Vec::new(), refs: Vec::new(), ..e.clone() });
        let text = po::write(&one);
        let body = text.split_once("\n\n").map_or("", |(_, b)| b);
        obsolete.extend(body.lines().map(|l| format!("#~ {l}")));
    }
    obsolete.extend(old.obsolete.iter().cloned());
    PoFile { header: old.header.clone(), entries, obsolete }
}

fn po_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "po"));
    files.sort();
    files
}

/// The template's key hashes, sorted.
fn pot_keys(pot: &PoFile) -> Vec<u64> {
    let mut keys: Vec<u64> = pot.entries.iter().map(|e| blob::hash(&e.key())).collect();
    keys.sort_unstable();
    keys
}

/// What `check` says of one file, and whether it fails.
fn report(name: &str, text: &str, keys: &[u64]) -> (Vec<String>, bool) {
    let code = Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (_, r) = check::load(text, &code, keys);
    let mut lines = vec![format!(
        "{name}: {} of {} translated, {} not reviewed, {} missing, {} unused ({})",
        r.translated,
        r.total,
        r.fuzzy,
        r.missing,
        r.unused,
        if r.human { "human" } else { "machine" }
    )];
    lines.extend(r.errors.iter().map(|i| format!("{name} line {}: error: {}", i.line, i.message)));
    lines.extend(r.warnings.iter().map(|i| format!("{name} line {}: warning: {}", i.line, i.message)));
    // Spec 15 §11.2: a reviewed language is complete; a machine one may fall back to English.
    let fails = !r.errors.is_empty() || (r.human && r.missing > 0);
    if r.human && r.missing > 0 {
        lines.push(format!("{name}: error: a reviewed language must translate every text"));
    }
    (lines, fails)
}

fn check_files(root: &Path, files: &[String]) -> Result<(), String> {
    let dir = lang_dir(root);
    let pot = po::parse(&read(&dir.join("gezik.pot"))?).map_err(|e| format!("gezik.pot {e}"))?;
    let keys = pot_keys(&pot);
    let paths: Vec<PathBuf> = if files.is_empty() { po_files(&dir) } else { files.iter().map(PathBuf::from).collect() };
    let mut failed = false;
    for path in paths {
        let (lines, fails) = report(&path.file_name().unwrap_or_default().to_string_lossy(), &read(&path)?, &keys);
        lines.iter().for_each(|l| println!("{l}"));
        failed |= fails;
    }
    if failed { Err("problems found".to_owned()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(context: &str, id: &str, plural: Option<&str>, note: Option<&str>) -> Found {
        Found { context: context.into(), id: id.into(), plural: plural.map(Into::into), note: note.map(Into::into) }
    }

    #[test]
    fn rust_marks() {
        let code = r##"
            // t!("in a comment") is not a text
            /* nor t!("this") */
            let a = format!("t!(\"in a string\")");
            let c = '"'; let q = '\''; fn x<'a>(_: &'a str) {}
            // TRANSLATORS: the File menu
            let b = t!("menu" => "Open");
            let d = t!("Moved {} to {}", a,
                b);
            let e = tn!("{n} file" | "{n} files", n);
            const F: &str = tx!(r#"Raw "quoted""#);
            let g = t!("Line \
                       joined\n\u{2026}");
            assert!(true); print!("x"); let ok = it!("not ours"); macro_rules! m { (tx!($l:literal)) => {} }
        "##;
        let test_item = "\n#[cfg(test)]\nfn helper() {}\n#[cfg(test)]\nmod tests;\n";
        assert_eq!(
            scan_rust(&format!(
                "{test_item}{code}\n#[cfg(test)]\n#[allow(x)]\npub(crate) mod tests {{ t!(\"test only\"); }}"
            ))
            .unwrap(),
            [
                found("menu", "Open", None, Some("the File menu")),
                found("", "Moved {} to {}", None, None),
                found("", "{n} file", Some("{n} files"), None),
                found("", "Raw \"quoted\"", None, None),
                found("", "Line joined\n…", None, None),
            ]
        );
        assert!(scan_rust("t!(name)").unwrap_err().contains("line 1"));
        assert!(scan_rust("\n\ntn!(\"a\", n)").unwrap_err().contains("line 3"));
    }

    #[test]
    fn slint_marks() {
        let code = r#"
            // @tr("comment")
            title: @tr("Settings — Gezik");
            text: @tr("menu" => "Open \"{}\"", name);
            text: @tr("{n} file" | "{n} files" % count);
            text: "@tr(\"plain\")";
        "#;
        assert_eq!(
            scan_slint(code).unwrap(),
            [
                found("", "Settings — Gezik", None, None),
                found("menu", "Open \"{}\"", None, None),
                found("", "{n} file", Some("{n} files"), None),
            ]
        );
    }

    #[test]
    fn update_keeps_drops_and_retires() {
        let pot = po::parse("msgid \"Open\"\nmsgstr \"\"\n\n#: a.rs\nmsgid \"New\"\nmsgstr \"\"\n\nmsgid \"{n} file\"\nmsgid_plural \"{n} files\"\nmsgstr[0] \"\"\nmsgstr[1] \"\"\n").unwrap();
        let old = po::parse("msgid \"\"\nmsgstr \"Language: tr\\n\"\n\n#, fuzzy\nmsgid \"Open\"\nmsgstr \"Aç\"\n\nmsgid \"Gone\"\nmsgstr \"Gitti\"\n\nmsgid \"{n} file\"\nmsgstr \"{n} dosya\"\n").unwrap();
        let new = update(&pot, &old);
        assert_eq!(new.header, "Language: tr\n");
        let shown: Vec<_> = new.entries.iter().map(|e| (e.id.as_str(), e.strs.clone(), e.fuzzy)).collect();
        assert_eq!(
            shown,
            [
                ("Open", vec!["Aç".to_owned()], true),
                ("New", vec![String::new()], false),
                ("{n} file", vec![String::new(), String::new()], false),
            ],
            "the plural changed: dropped"
        );
        assert_eq!(new.entries[1].refs, ["a.rs"]);
        assert_eq!(new.obsolete, ["#~ msgid \"Gone\"", "#~ msgstr \"Gitti\""], "{new:?}");
        assert_eq!(po::parse(&po::write(&new)).unwrap().obsolete, new.obsolete);
    }

    #[test]
    fn check_says_what_is_wrong() {
        let keys = pot_keys(&po::parse("msgid \"Open {}\"\nmsgstr \"\"\n\nmsgid \"Close\"\nmsgstr \"\"\n").unwrap());
        let (lines, fails) = report("tr.po", "msgid \"Open {}\"\nmsgstr \"Aç\"\n", &keys);
        assert!(fails);
        assert_eq!(lines[1], "tr.po line 1: error: \"{0}\" missing in the translation");
        assert!(lines.last().unwrap().contains("every text"), "human and incomplete");
        let machine =
            "msgid \"\"\nmsgstr \"X-Gezik-Review: machine\\n\"\n\n#, fuzzy\nmsgid \"Open {}\"\nmsgstr \"Ouvrir {}\"\n";
        let (lines, fails) = report("fr.po", machine, &keys);
        assert!(!fails, "{lines:?}");
        assert_eq!(lines, ["fr.po: 1 of 2 translated, 1 not reviewed, 1 missing, 0 unused (machine)"]);
    }

    /// The string literals of a Slint file that are not `@tr`'s, `@image-url`'s or an import's,
    /// with their lines.
    fn plain_slint_literals(text: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut i = 0;
        let line = |at: usize| text[..at].matches('\n').count() + 1;
        while i < text.len() {
            let rest = &text[i..];
            let c = rest.chars().next().unwrap_or_default();
            let at_line_start = i == 0 || text[..i].trim_end_matches([' ', '\t']).ends_with('\n');
            if rest.starts_with("//") || (at_line_start && rest.starts_with("import ")) {
                i += rest.find(if rest.starts_with("//") { '\n' } else { ';' }).unwrap_or(rest.len());
            } else if rest.starts_with("/*") {
                i += rest.find("*/").map_or(rest.len(), |e| e + 2);
            } else if let Some(mark) = ["@tr(", "@image-url("].iter().find(|m| rest.starts_with(**m)) {
                // The marked texts (with a context and a plural); the arguments after are code.
                let mut after = &rest[mark.len()..];
                while let Some((_, next)) = literal(after, true) {
                    let next = next.trim_start();
                    after = next.strip_prefix("=>").or_else(|| next.strip_prefix('|')).unwrap_or(next);
                    if after.len() == next.len() {
                        break;
                    }
                }
                i = text.len() - after.len();
            } else if c == '"' {
                let (found, after) = literal(rest, true).expect("a Slint string literal");
                out.push((line(i), found));
                i = text.len() - after.len();
            } else {
                i += c.len_utf8();
            }
        }
        out
    }

    /// A literal that is no text for the user: no letter, an id (`new-window`, `tag:`, `true`),
    /// an SVG path, `{token}`s only, or a name that stays as it is.
    fn not_a_text(literal: &str) -> bool {
        const KEPT: [&str; 7] = ["Gezik", "PNG", "JPEG", "LF", "CRLF", "CR", "A4"];
        let mut tokenless = String::new();
        let mut depth = 0;
        for c in literal.chars() {
            depth += i32::from(c == '{');
            if depth == 0 {
                tokenless.push(c);
            }
            depth -= i32::from(c == '}' && depth > 0);
        }
        let svg = literal.starts_with(['M', 'm'])
            && literal.chars().all(|c| "MmLlHhVvCcSsQqTtAaZz0123456789 .,-".contains(c));
        !tokenless.chars().any(char::is_alphabetic)
            || literal.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_:./%".contains(c))
            || svg
            || KEPT.contains(&literal)
    }

    /// Spec 15 §6: a text the user sees in a `.slint` file goes through `@tr`.
    #[test]
    fn slint_texts_are_marked() {
        let found = plain_slint_literals(
            r#"import { A } from "a.slint";
// "comment"
text: @tr("ctx" => "{n} a" | "{n} b" % n);
icon: @image-url("x.svg"); t: "Plain" + @tr("{}", "arg"); u: "Page \{n}";"#,
        );
        assert_eq!(found, [(4, "Plain".to_owned()), (4, "arg".to_owned()), (4, "Page ".to_owned())]);
        assert!(
            not_a_text("new-window") && not_a_text("M 1 2 L 3 4") && not_a_text("  ·  ") && not_a_text("{name} {ext}")
        );
        assert!(!not_a_text("Cancel") && !not_a_text("sorted ascending") && !not_a_text("Page "));
        let root = root();
        let mut plain = Vec::new();
        for file in sources(&root).into_iter().filter(|f| f.ends_with(".slint")) {
            for (line, literal) in plain_slint_literals(&read(&root.join(&file)).unwrap()) {
                if !not_a_text(&literal) {
                    plain.push(format!("{file}:{line}: {literal:?}"));
                }
            }
        }
        assert!(plain.is_empty(), "texts without @tr (spec 15 §6):\n{}", plain.join("\n"));
    }

    /// Whether a function or method of this name shows its text arguments to the user: status
    /// lines, questions, notices and the window's text properties.
    fn is_message_sink(name: &str) -> bool {
        const ENDS: [&str; 10] =
            ["text", "label", "title", "status", "note", "error", "message", "hint", "tip", "empty"];
        matches!(name, "status" | "tell" | "notify" | "warn" | "note")
            || name.starts_with("ask")
            || name.starts_with("confirm")
            || (name.starts_with("set_") && ENDS.iter().any(|e| name.ends_with(e)))
    }

    /// The top-level arguments of the call whose `(` is at the start of `s`.
    fn call_args(s: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let (mut depth, mut start, mut i) = (0, 1, 0);
        while i < s.len() {
            let rest = &s[i..];
            let c = rest.chars().next().unwrap_or_default();
            if c == '"' || ((rest.starts_with("r\"") || rest.starts_with("r#\"")) && !ident_before(s, i)) {
                i = literal(rest, false).map_or(i + 1, |(_, after)| s.len() - after.len());
                continue;
            }
            if c == '\'' && rest[1..].chars().nth(1) == Some('\'') {
                i += rest[1..].chars().next().map_or(1, char::len_utf8) + 2;
                continue;
            }
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        out.push(&s[start..i]);
                        return out;
                    }
                }
                ',' if depth == 1 => {
                    out.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
            i += c.len_utf8();
        }
        out
    }

    /// The plain texts given to a message sink in `text` (before its test module), with their
    /// lines: an argument that is a string literal, a `format!`'s, or one in a `&[…]` of them.
    fn unmarked_messages(text: &str) -> Vec<(usize, String)> {
        let text = &text[..test_module(text)];
        let mut out = Vec::new();
        let line = |at: usize| text[..at].matches('\n').count() + 1;
        let mut i = 0;
        while i < text.len() {
            let rest = &text[i..];
            let c = rest.chars().next().unwrap_or_default();
            if rest.starts_with("//") {
                i += rest.find('\n').unwrap_or(rest.len());
                continue;
            }
            if rest.starts_with("/*") {
                i += rest.find("*/").map_or(rest.len(), |e| e + 2);
                continue;
            }
            if c == '"' || ((rest.starts_with("r\"") || rest.starts_with("r#\"")) && !ident_before(text, i)) {
                i = literal(rest, false).map_or(i + 1, |(_, after)| text.len() - after.len());
                continue;
            }
            if c == '\'' && rest[1..].chars().nth(1) == Some('\'') {
                i += rest[1..].chars().next().map_or(1, char::len_utf8) + 2;
                continue;
            }
            if c == '(' {
                let start = text[..i].rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(0, |p| p + 1);
                if is_message_sink(&text[start..i]) && !text[..start].ends_with("fn ") {
                    for arg in call_args(rest) {
                        let arg = arg.trim_start();
                        let items = match arg.strip_prefix('&').filter(|list| list.starts_with('[')) {
                            Some(list) => call_args(list),
                            None => vec![arg.strip_prefix("format!(").unwrap_or(arg)],
                        };
                        for item in items {
                            if let Some((found, _)) = literal(item, false).filter(|(l, _)| !not_a_text(l)) {
                                out.push((line(i), found));
                            }
                        }
                    }
                }
            }
            i += c.len_utf8();
        }
        out
    }

    /// Spec 15 §6 for Rust, as far as a scan can tell: a text given to a status line, a question,
    /// a notice or a window's text goes through `t!`, `tn!` or `tx!`. A text kept as it is
    /// (a name, a format) goes in `KEPT` as `file: text`.
    #[test]
    fn rust_messages_are_marked() {
        // A key path and an inner message, nothing to translate.
        const KEPT: &[&str] = &[
            "crates/gezik-config/src/settings.rs: search.content-max-size: {err}",
            "crates/gezik-config/src/tags.rs: list[{}]: {err}",
        ];
        let found = unmarked_messages(
            r#"fn status(x: &str) {}
nav.status(format!("Copied {}", n)); nav.status(t!("Fine"));
dialogs.ask(t!("Title"), "Plain message", &["Delete", "x-id"], |_| status("Nested call"));
window.set_cp_note("Hint".into()); window.set_mode("list".into()); "status(\"no\")";
#[cfg(test)]
mod tests { fn f() { status("Test only"); } }"#,
        );
        let want = [(2, "Copied {}"), (3, "Plain message"), (3, "Delete"), (3, "Nested call"), (4, "Hint")];
        assert_eq!(found, want.map(|(l, s)| (l, s.to_owned())), "ids pass");
        let root = root();
        let mut plain = Vec::new();
        for file in sources(&root).into_iter().filter(|f| f.ends_with(".rs")) {
            for (line, literal) in unmarked_messages(&read(&root.join(&file)).unwrap()) {
                if !KEPT.contains(&format!("{file}: {literal}").as_str()) {
                    plain.push(format!("{file}:{line}: {literal:?}"));
                }
            }
        }
        assert!(plain.is_empty(), "texts without t! (spec 15 §6), or add them to KEPT:\n{}", plain.join("\n"));
    }

    /// The CI's tie (spec 15 §11.2): the template is the code's, and the built-in languages hold.
    #[test]
    fn the_template_is_current_and_the_languages_hold() {
        let root = root();
        let pot_path = lang_dir(&root).join("gezik.pot");
        let written = read(&pot_path).unwrap().replace("\r\n", "\n");
        assert!(
            extract(&root).unwrap() == written,
            "lang/gezik.pot is out of date: run `cargo run -p gezik-i18n --bin gezik-lang -- extract`"
        );
        let pot = po::parse(&written).unwrap();
        let mut keys = pot_keys(&pot);
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count, "two texts of the template have the same hash");
        assert_eq!(keys, gezik_i18n::keys(), "the build packed this template");
        for path in po_files(&lang_dir(&root)) {
            let (lines, fails) = report(&path.display().to_string(), &read(&path).unwrap(), &keys);
            assert!(!fails, "{}", lines.join("\n"));
        }
    }
}
