//! A small table-driven syntax colorer for the preview (spec 13 §4.3): per language a
//! [`LangDef`] of comment and string markers and word lists, a one-byte state carried from
//! line to line (inside a block comment, a multi-line or raw string, a Markdown code block),
//! and per line the places where the color changes. Good enough to tell keywords, strings,
//! numbers and comments apart; no grammar. File contents are untrusted: nothing here indexes.

#![deny(clippy::indexing_slicing)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Class {
    Plain,
    Keyword,
    Type,
    String,
    Number,
    Comment,
    /// Markup tags, keys (JSON, TOML, YAML, INI), section headers.
    Tag,
}

/// A string: `open` … `close`, `escape` (0: none) skipping the next byte; `multi_line`
/// strings carry on to the next line; with `max` (0: any) a string that does not close
/// within that many bytes is no string (a Rust lifetime `'a` is no character).
pub struct StringRule {
    pub open: &'static str,
    pub close: &'static str,
    pub escape: u8,
    pub multi_line: bool,
    pub max: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Code,
    /// `<tag attr="…">`: strings only inside tags.
    Markup,
    /// A first word followed by `=` or `:` is a key; `[section]` lines.
    Keys,
    /// Headings, quotes, list markers, code spans and fenced blocks.
    Markdown,
    /// `+` and `-` lines.
    Diff,
}

pub struct LangDef {
    /// Lowercase extensions and whole file names (`makefile`, `dockerfile`).
    pub names: &'static [&'static str],
    pub line_comments: &'static [&'static str],
    pub block_comments: &'static [(&'static str, &'static str)],
    /// Longer openings first (`"""` before `"`).
    pub strings: &'static [StringRule],
    /// Space-separated.
    pub keywords: &'static str,
    /// Space-separated.
    pub types: &'static str,
    pub mode: Mode,
    /// Words starting with a capital letter are types.
    pub caps_types: bool,
    /// Keywords and comment markers ignore case.
    pub nocase: bool,
    /// Rust raw strings: `r"…"`, `r#"…"#`, `br"…"`.
    pub raw: bool,
}

/// The most parts a line is colored in; the rest of a longer line is plain.
pub const MAX_PARTS: usize = 64;

const fn string(open: &'static str, close: &'static str, escape: u8, multi_line: bool) -> StringRule {
    StringRule { open, close, escape, multi_line, max: 0 }
}

const DQ: StringRule = string("\"", "\"", b'\\', false);
const SQ: StringRule = string("'", "'", b'\\', false);
const SQ_RAW: StringRule = string("'", "'", 0, false);
const DQ_RAW: StringRule = string("\"", "\"", 0, false);
const CHAR: StringRule = StringRule { open: "'", close: "'", escape: b'\\', multi_line: false, max: 10 };
const BACKTICK: StringRule = string("`", "`", b'\\', true);
const TRIPLE_DQ: StringRule = string("\"\"\"", "\"\"\"", b'\\', true);
const TRIPLE_SQ: StringRule = string("'''", "'''", b'\\', true);
const C_BLOCK: (&str, &str) = ("/*", "*/");

const fn code(names: &'static [&'static str], keywords: &'static str, types: &'static str) -> LangDef {
    LangDef {
        names,
        line_comments: &["//"],
        block_comments: &[C_BLOCK],
        strings: &[DQ, CHAR],
        keywords,
        types,
        mode: Mode::Code,
        caps_types: true,
        nocase: false,
        raw: false,
    }
}

const fn keys(
    names: &'static [&'static str],
    line_comments: &'static [&'static str],
    keywords: &'static str,
) -> LangDef {
    LangDef {
        names,
        line_comments,
        block_comments: &[],
        strings: &[DQ, SQ],
        keywords,
        types: "",
        mode: Mode::Keys,
        caps_types: false,
        nocase: false,
        raw: false,
    }
}

pub static LANGS: [LangDef; 27] = [
    LangDef {
        strings: &[DQ, CHAR],
        raw: true,
        ..code(
            &["rs"],
            "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while",
            "bool char f32 f64 i8 i16 i32 i64 i128 isize str u8 u16 u32 u64 u128 usize",
        )
    },
    LangDef {
        caps_types: false,
        ..code(
            &["c", "h", "cpp", "cc", "cxx", "hpp", "hh", "hxx", "ino", "m", "mm"],
            "auto break case catch class const constexpr continue default define defined delete do else elif endif enum explicit extern false for friend goto if ifdef ifndef include inline namespace new noexcept nullptr operator pragma private protected public return sizeof static struct switch template this throw true try typedef typename union using virtual volatile while",
            "bool char double float int long short signed unsigned void size_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t",
        )
    },
    code(
        &["cs"],
        "abstract as async await base break case catch class const continue default delegate do else enum event explicit false finally for foreach get if implicit in interface internal is lock namespace new null operator out override params private protected public readonly record ref return sealed set static struct switch this throw true try typeof using var virtual void volatile while yield",
        "bool byte char decimal double float int long object sbyte short string uint ulong ushort",
    ),
    code(
        &["java"],
        "abstract assert break case catch class const continue default do else enum extends false final finally for if implements import instanceof interface native new null package private protected public record return static super switch synchronized this throw throws transient true try var void volatile while",
        "boolean byte char double float int long short",
    ),
    LangDef {
        strings: &[TRIPLE_DQ, DQ, CHAR],
        ..code(
            &["kt", "kts"],
            "as break class companion continue data do else enum false for fun if import in interface is null object override package private protected public return sealed super this throw true try typealias val var when while",
            "",
        )
    },
    LangDef {
        strings: &[DQ, string("`", "`", 0, true), CHAR],
        caps_types: false,
        ..code(
            &["go"],
            "break case chan const continue default defer else fallthrough false for func go goto if import interface map nil package range return select struct switch true type var",
            "bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune string uint uint8 uint16 uint32 uint64 uintptr",
        )
    },
    LangDef {
        strings: &[TRIPLE_DQ, DQ],
        ..code(
            &["swift"],
            "as break case catch class continue default defer do else enum extension false fileprivate for func guard if import in init inout internal is let nil open operator private protocol public repeat return self static struct subscript super switch throw throws true try var where while",
            "",
        )
    },
    LangDef {
        strings: &[DQ, SQ, BACKTICK],
        ..code(
            &["js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts"],
            "abstract as async await break case catch class const continue debugger declare default delete do else enum export extends false finally for from function if implements import in instanceof interface let namespace new null of private protected public readonly return static super switch this throw true try type typeof undefined var void while yield",
            "any bigint boolean never number object string symbol unknown",
        )
    },
    LangDef {
        line_comments: &["#"],
        block_comments: &[],
        strings: &[TRIPLE_DQ, TRIPLE_SQ, DQ, SQ],
        ..code(
            &["py", "pyw", "pyi"],
            "False None True and as assert async await break case class continue def del elif else except finally for from global if import in is lambda match nonlocal not or pass raise return self try while with yield",
            "bool bytes dict float int list object set str tuple",
        )
    },
    LangDef {
        line_comments: &["#"],
        block_comments: &[("=begin", "=end")],
        strings: &[DQ, SQ],
        ..code(
            &["rb", "rake", "gemspec", "rakefile", "gemfile"],
            "alias and begin break case class def do else elsif end ensure false for if in module next nil not or redo require rescue retry return self super then true undef unless until when while yield",
            "",
        )
    },
    LangDef {
        line_comments: &["//", "#"],
        strings: &[DQ, SQ],
        caps_types: false,
        ..code(
            &["php"],
            "abstract and array as break case catch class clone const continue declare default do echo else elseif empty extends false final finally fn for foreach function global if implements include instanceof interface isset list match namespace new null or print private protected public require require_once return static switch throw trait true try unset use var while yield",
            "",
        )
    },
    LangDef {
        line_comments: &["--"],
        block_comments: &[("--[[", "]]")],
        strings: &[DQ, SQ, string("[[", "]]", 0, true)],
        caps_types: false,
        ..code(
            &["lua"],
            "and break do else elseif end false for function goto if in local nil not or repeat return then true until while",
            "",
        )
    },
    LangDef {
        line_comments: &["#"],
        block_comments: &[],
        strings: &[DQ, SQ_RAW],
        caps_types: false,
        ..code(
            &["sh", "bash", "zsh", "ksh", "fish", ".bashrc", ".zshrc", ".profile", ".bash_profile"],
            "alias break case continue declare do done echo elif else esac exit export fi for function if in local readonly return set shift source then unset until while",
            "",
        )
    },
    LangDef {
        line_comments: &["#"],
        block_comments: &[("<#", "#>")],
        strings: &[string("\"", "\"", b'`', false), SQ_RAW],
        caps_types: false,
        nocase: true,
        ..code(
            &["ps1", "psm1", "psd1"],
            "begin break catch class continue data do dynamicparam else elseif end exit filter finally for foreach function if in param process return switch throw trap try until using while",
            "",
        )
    },
    LangDef {
        line_comments: &["rem ", "::", "@rem "],
        block_comments: &[],
        strings: &[DQ_RAW],
        caps_types: false,
        nocase: true,
        ..code(
            &["bat", "cmd"],
            "call cd cls copy del dir do echo else endlocal errorlevel exist exit for goto if in md mkdir move not off on pause popd pushd rd ren set setlocal shift start",
            "",
        )
    },
    LangDef {
        line_comments: &["--"],
        strings: &[SQ_RAW, DQ_RAW],
        caps_types: false,
        nocase: true,
        ..code(
            &["sql"],
            "add all alter and as asc begin between by case check column commit constraint create cross database default delete desc distinct drop else end exists foreign from full group having if in index inner insert into is join key left like limit not null on or order outer primary references right rollback select set table then transaction union unique update values view when where with",
            "bigint blob boolean char date datetime decimal float int integer numeric real smallint text timestamp varchar",
        )
    },
    LangDef {
        names: &[
            "html", "htm", "xhtml", "xml", "svg", "xsd", "xsl", "xslt", "plist", "csproj", "vcxproj", "props",
            "targets", "resx", "xaml", "vue", "manifest",
        ],
        line_comments: &[],
        block_comments: &[("<!--", "-->")],
        strings: &[DQ_RAW, SQ_RAW],
        keywords: "",
        types: "",
        mode: Mode::Markup,
        caps_types: false,
        nocase: false,
        raw: false,
    },
    LangDef {
        block_comments: &[C_BLOCK],
        ..keys(&["css", "scss", "sass", "less"], &["//"], "important inherit initial none unset")
    },
    keys(&["json", "jsonc", "json5", "geojson", "webmanifest", ".babelrc", ".eslintrc"], &["//"], "false null true"),
    LangDef {
        strings: &[TRIPLE_DQ, string("'''", "'''", 0, true), DQ, SQ_RAW],
        ..keys(&["toml"], &["#"], "false true")
    },
    keys(&["yaml", "yml"], &["#"], "false no null off on true yes"),
    keys(
        &[
            "ini",
            "cfg",
            "conf",
            "properties",
            "inf",
            "reg",
            "desktop",
            "service",
            "env",
            ".env",
            ".editorconfig",
            ".gitconfig",
            ".gitattributes",
            ".npmrc",
        ],
        &[";", "#"],
        "false no off on true yes",
    ),
    LangDef { mode: Mode::Markdown, ..keys(&["md", "markdown", "mdx"], &[], "") },
    LangDef { mode: Mode::Diff, ..keys(&["diff", "patch"], &[], "") },
    code(
        &["slint"],
        "animate callback component export false for from function global if import in in-out inherits out parent private property public pure return root self states struct transitions true",
        "bool brush color duration easing float image int length percent string",
    ),
    keys(
        &["makefile", "gnumakefile", "mk", "mak"],
        &["#"],
        "define else endef endif export ifdef ifeq ifndef ifneq include override",
    ),
    LangDef {
        nocase: true,
        ..keys(
            &["dockerfile", "containerfile"],
            &["#"],
            "add arg as cmd copy entrypoint env expose from healthcheck label maintainer onbuild run shell stopsignal user volume workdir",
        )
    },
];

/// The language of a file by its name: its lowercase extension, else its whole name.
pub fn lang_for(file_name: &str) -> Option<&'static LangDef> {
    let name = file_name.to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(stem, ext)| if stem.is_empty() { name.as_str() } else { ext });
    let find = |key: &str| LANGS.iter().find(|lang| lang.names.contains(&key));
    ext.and_then(find).or_else(|| find(&name))
}

impl LangDef {
    /// Code is shown unwrapped by default; prose (Markdown, INI-like settings) wrapped.
    pub fn is_code(&self) -> bool {
        !matches!(self.mode, Mode::Markdown | Mode::Diff) && self.names.first() != Some(&"ini")
    }
}

/// States carried from line to line; 0 is none.
const IN_BLOCK: u8 = 1; // + block comment index (≤ 7)
const IN_STRING: u8 = 8; // + string rule index (≤ 7)
const IN_RAW: u8 = 16; // + the raw string's number of `#`
const MAX_HASHES: usize = 200;

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

/// Colors one line: pushes `(byte offset, class)` where the color changes (the line starts
/// plain) into `out` (nothing when `None`: only the state is wanted) and returns the state the
/// next line starts in.
pub fn line(lang: &LangDef, state: u8, text: &str, out: Option<&mut Vec<(usize, Class)>>) -> u8 {
    let mut lexer = Lexer { lang, b: text.as_bytes(), out, parts: 0 };
    match lang.mode {
        Mode::Markdown => lexer.markdown(state),
        Mode::Diff => {
            lexer.diff();
            0
        }
        _ => lexer.code(state),
    }
}

struct Lexer<'a, 'o> {
    lang: &'a LangDef,
    b: &'a [u8],
    out: Option<&'o mut Vec<(usize, Class)>>,
    parts: usize,
}

impl Lexer<'_, '_> {
    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(0)
    }

    fn starts(&self, i: usize, pat: &str) -> bool {
        let Some(head) = self.b.get(i..).and_then(|rest| rest.get(..pat.len())) else { return false };
        if self.lang.nocase { head.eq_ignore_ascii_case(pat.as_bytes()) } else { head == pat.as_bytes() }
    }

    /// Where `pat` starts at or after `i`.
    fn find(&self, i: usize, pat: &str) -> Option<usize> {
        (i..self.b.len()).find(|&at| self.starts(at, pat))
    }

    fn emit(&mut self, at: usize, class: Class) {
        let Some(out) = self.out.as_deref_mut() else { return };
        let last = out.last().map_or(Class::Plain, |&(_, c)| c);
        if last == class || self.parts >= MAX_PARTS {
            return;
        }
        if self.parts == MAX_PARTS - 1 {
            // Out of parts: the rest is plain.
            if last != Class::Plain {
                out.push((at, Class::Plain));
            }
            self.parts = MAX_PARTS;
            return;
        }
        out.push((at, class));
        self.parts += 1;
    }

    fn skip_spaces(&self, mut i: usize) -> usize {
        while matches!(self.at(i), b' ' | b'\t') {
            i += 1;
        }
        i
    }

    /// The end of the string of `rule` whose body starts at `i`.
    fn string_end(&self, i: usize, rule: &StringRule) -> Option<usize> {
        let mut at = i;
        while at < self.b.len() {
            // A character without an escape is one character: at most 4 bytes.
            let max = if self.at(i) == rule.escape { usize::from(rule.max) } else { 5 };
            if rule.max > 0 && at - i > max {
                return None;
            }
            if rule.escape != 0 && self.at(at) == rule.escape {
                at += 2;
                continue;
            }
            if self.starts(at, rule.close) {
                return Some(at + rule.close.len());
            }
            at += 1;
        }
        None
    }

    fn raw_end(&self, i: usize, hashes: usize) -> Option<usize> {
        let mut at = i;
        while let Some(quote) = self.find(at, "\"") {
            let end = quote + 1;
            if (end..end + hashes).all(|h| self.at(h) == b'#') {
                return Some(end + hashes);
            }
            at = end;
        }
        None
    }

    /// Finishes what the line starts inside; returns where to go on, or the state the line
    /// ends in.
    fn resume(&mut self, state: u8) -> Result<usize, u8> {
        let end = match state {
            0 => return Ok(0),
            s if s < IN_STRING => {
                let close = self.lang.block_comments.get(usize::from(s - IN_BLOCK)).map(|&(_, close)| close);
                self.emit(0, Class::Comment);
                close.and_then(|close| self.find(0, close).map(|at| at + close.len()))
            }
            s if s < IN_RAW => {
                self.emit(0, Class::String);
                self.lang.strings.get(usize::from(s - IN_STRING)).and_then(|rule| self.string_end(0, rule))
            }
            s => {
                self.emit(0, Class::String);
                self.raw_end(0, usize::from(s - IN_RAW))
            }
        };
        end.ok_or(state)
    }

    fn code(&mut self, state: u8) -> u8 {
        let mut i = match self.resume(state) {
            Ok(i) => i,
            Err(state) => return state,
        };
        let lang = self.lang;
        let markup = lang.mode == Mode::Markup;
        let keys = lang.mode == Mode::Keys;
        let mut first = true;
        let mut in_tag = false;
        if keys && self.section() {
            return 0;
        }
        while i < self.b.len() {
            let c = self.at(i);
            if c == b' ' || c == b'\t' {
                i += 1;
                continue;
            }
            if let Some((k, &(open, close))) =
                lang.block_comments.iter().enumerate().find(|(_, (open, _))| self.starts(i, open))
            {
                self.emit(i, Class::Comment);
                match self.find(i + open.len(), close) {
                    Some(at) => i = at + close.len(),
                    None => return IN_BLOCK + k as u8,
                }
                continue;
            }
            if markup && !in_tag {
                if c == b'<' && (self.at(i + 1).is_ascii_alphabetic() || matches!(self.at(i + 1), b'/' | b'!' | b'?')) {
                    self.emit(i, Class::Tag);
                    i += 2;
                    while self.at(i).is_ascii_alphanumeric() || matches!(self.at(i), b'-' | b':' | b'.' | b'_') {
                        i += 1;
                    }
                    in_tag = true;
                } else {
                    self.emit(i, Class::Plain);
                    i += 1;
                }
                continue;
            }
            if markup && (c == b'>' || (c == b'/' && self.at(i + 1) == b'>') || (c == b'?' && self.at(i + 1) == b'>')) {
                self.emit(i, Class::Tag);
                i += if c == b'>' { 1 } else { 2 };
                in_tag = false;
                continue;
            }
            let after_word = i > 0 && ident_byte(self.at(i - 1));
            if let Some(&comment) = lang.line_comments.iter().find(|&&comment| self.starts(i, comment))
                && !(comment == "#" && i > 0 && !matches!(self.at(i - 1), b' ' | b'\t'))
            {
                self.emit(i, Class::Comment);
                return 0;
            }
            if let Some((j, rule)) = lang.strings.iter().enumerate().find(|(_, rule)| self.starts(i, rule.open)) {
                match self.string_end(i + rule.open.len(), rule) {
                    Some(end) => {
                        let key = keys && self.at(self.skip_spaces(end)) == b':';
                        self.emit(i, if key { Class::Tag } else { Class::String });
                        i = end;
                    }
                    None if rule.max > 0 => {
                        self.emit(i, Class::Plain);
                        i += rule.open.len();
                    }
                    None => {
                        self.emit(i, Class::String);
                        return if rule.multi_line { IN_STRING + j as u8 } else { 0 };
                    }
                }
                first = false;
                continue;
            }
            if c.is_ascii_digit() || (c == b'.' && self.at(i + 1).is_ascii_digit() && !after_word) {
                self.emit(i, Class::Number);
                i += 1;
                while ident_byte(self.at(i)) || (self.at(i) == b'.' && self.at(i + 1).is_ascii_digit()) {
                    i += 1;
                }
                first = false;
                continue;
            }
            if ident_byte(c) {
                if lang.raw
                    && let Some(next) = self.raw_string(i)
                {
                    match next {
                        Ok(end) => i = end,
                        Err(state) => return state,
                    }
                    continue;
                }
                let start = i;
                while ident_byte(self.at(i)) || ((keys || in_tag) && matches!(self.at(i), b'-' | b'.')) {
                    i += 1;
                }
                let word = std::str::from_utf8(self.b.get(start..i).unwrap_or_default()).unwrap_or_default();
                let class = if keys && first && matches!(self.at(self.skip_spaces(i)), b':' | b'=') {
                    Class::Tag
                } else if has(lang.keywords, word, lang.nocase) {
                    Class::Keyword
                } else if has(lang.types, word, false) || (lang.caps_types && c.is_ascii_uppercase()) {
                    Class::Type
                } else {
                    Class::Plain
                };
                self.emit(start, class);
                first = false;
                continue;
            }
            self.emit(i, Class::Plain);
            // A YAML list item's key still comes first.
            first &= c == b'-';
            i += 1;
        }
        0
    }

    /// At a Rust raw string's `r`/`br`: its end, or the state of one going on.
    fn raw_string(&mut self, i: usize) -> Option<Result<usize, u8>> {
        let prefix = if self.starts(i, "br") { 2 } else { usize::from(self.at(i) == b'r') };
        if prefix == 0 || (i > 0 && ident_byte(self.at(i - 1))) {
            return None;
        }
        let mut quote = i + prefix;
        while self.at(quote) == b'#' {
            quote += 1;
        }
        let hashes = quote - i - prefix;
        if self.at(quote) != b'"' || hashes > MAX_HASHES {
            return None;
        }
        self.emit(i, Class::String);
        Some(self.raw_end(quote + 1, hashes).ok_or(IN_RAW + hashes as u8))
    }

    /// A `[section]` line (TOML, INI): all of it a tag.
    fn section(&mut self) -> bool {
        let start = self.skip_spaces(0);
        let trimmed = String::from_utf8_lossy(self.b.get(start..).unwrap_or_default());
        let trimmed = trimmed.trim_end();
        let inner = trimmed.trim_start_matches('[');
        let is = trimmed.starts_with('[')
            && trimmed.ends_with(']')
            && inner.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && !trimmed.contains(',');
        if is {
            self.emit(start, Class::Tag);
        }
        is
    }

    fn markdown(&mut self, state: u8) -> u8 {
        let start = self.skip_spaces(0);
        let fence = self.starts(start, "```") || self.starts(start, "~~~");
        if fence {
            self.emit(0, Class::Comment);
            return u8::from(state == 0);
        }
        if state != 0 {
            self.emit(0, Class::String);
            return state;
        }
        match self.at(start) {
            b'#' => self.emit(0, Class::Keyword),
            b'>' => self.emit(0, Class::Comment),
            b'-' | b'*' | b'+' if self.at(start + 1) == b' ' => {
                self.emit(start, Class::Tag);
                self.emit(start + 1, Class::Plain);
            }
            _ => {}
        }
        if self.at(start) != b'#' {
            // Code spans.
            let mut i = start;
            while let Some(open) = self.find(i, "`") {
                let Some(close) = self.find(open + 1, "`") else { break };
                self.emit(open, Class::String);
                self.emit(close + 1, Class::Plain);
                i = close + 1;
            }
        }
        0
    }

    fn diff(&mut self) {
        let class = match self.at(0) {
            _ if self.starts(0, "+++")
                || self.starts(0, "---")
                || self.starts(0, "diff ")
                || self.starts(0, "index ") =>
            {
                Class::Keyword
            }
            b'+' => Class::String,
            b'-' => Class::Tag,
            b'@' => Class::Type,
            _ => Class::Plain,
        };
        self.emit(0, class);
    }
}

fn has(list: &str, word: &str, nocase: bool) -> bool {
    !word.is_empty() && list.split(' ').any(|k| if nocase { k.eq_ignore_ascii_case(word) } else { k == word })
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// The line's words with their classes (whitespace-separated pieces of each part).
    fn colors(file: &str, lines: &[&str]) -> Vec<Vec<(String, Class)>> {
        let lang = lang_for(file).unwrap();
        let mut state = 0;
        lines
            .iter()
            .map(|text| {
                let mut out = Vec::new();
                state = line(lang, state, text, Some(&mut out));
                let mut parts = Vec::new();
                let mut bounds: Vec<(usize, Class)> = vec![(0, Class::Plain)];
                bounds.extend(out);
                for (k, &(start, class)) in bounds.iter().enumerate() {
                    let end = bounds.get(k + 1).map_or(text.len(), |b| b.0);
                    let piece = text[start..end].trim();
                    if !piece.is_empty() {
                        parts.push((piece.to_owned(), class));
                    }
                }
                parts
            })
            .collect()
    }

    fn class_of(file: &str, text: &str, word: &str) -> Class {
        let parts = colors(file, &[text]).remove(0);
        parts
            .iter()
            .find(|(p, _)| p.starts_with(word))
            .map(|(_, c)| *c)
            .unwrap_or_else(|| panic!("{word} in {parts:?}"))
    }

    #[test]
    fn languages_are_found_by_extension_or_name() {
        assert_eq!(lang_for("main.RS").unwrap().names, &["rs"]);
        assert_eq!(lang_for("Makefile").unwrap().names[0], "makefile");
        assert_eq!(lang_for("Dockerfile").unwrap().names[0], "dockerfile");
        assert_eq!(lang_for(".bashrc").unwrap().names[0], "sh");
        assert!(lang_for("notes.txt").is_none());
        assert!(lang_for("README").is_none());
        assert!(lang_for("x.py").unwrap().is_code());
        assert!(!lang_for("x.md").unwrap().is_code());
        assert!(!lang_for("x.ini").unwrap().is_code());
        let mut names: Vec<_> = LANGS.iter().flat_map(|l| l.names.iter()).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "a name belongs to one language");
        for lang in &LANGS {
            assert!(lang.block_comments.len() < 8 && lang.strings.len() < 8);
        }
    }

    #[test]
    fn each_language_colors_its_basics() {
        use Class::*;
        let cases: &[(&str, &str, &str, Class)] = &[
            ("a.rs", "fn main() { let x = 42; } // done", "fn", Keyword),
            ("a.rs", "fn main() { let x = 42; } // done", "42", Number),
            ("a.rs", "fn main() { let x = 42; } // done", "// done", Comment),
            ("a.rs", "let s: String = \"hi\";", "String", Type),
            ("a.rs", "let s: String = \"hi\";", "\"hi\"", String),
            ("a.rs", "fn f<'a>(x: &'a str) {}", "str", Type),
            ("a.c", "#include <stdio.h>", "include", Keyword),
            ("a.c", "int x = 0x1F;", "int", Type),
            ("a.c", "int x = 0x1F;", "0x1F", Number),
            ("a.cs", "public class A { string s = \"x\"; }", "public", Keyword),
            ("a.java", "import java.util.List;", "import", Keyword),
            ("a.kt", "val x = \"\"\"a\"\"\"", "val", Keyword),
            ("a.go", "func main() { s := `raw` }", "`raw`", String),
            ("a.swift", "guard let x else { return }", "guard", Keyword),
            ("a.ts", "const t = `${a}`; // c", "`${a}`", String),
            ("a.ts", "let n: number = 1.5;", "number", Type),
            ("a.py", "def f(): return None  # c", "def", Keyword),
            ("a.py", "def f(): return None  # c", "# c", Comment),
            ("a.rb", "def x; end # c", "end", Keyword),
            ("a.php", "<?php echo $x; # c", "echo", Keyword),
            ("a.lua", "local x = 1 -- c", "-- c", Comment),
            ("a.sh", "echo \"$HOME\" # c", "# c", Comment),
            ("a.sh", "echo ${#arr}", "echo", Keyword),
            ("a.ps1", "Function Get-X { param($a) } # c", "Function", Keyword),
            ("a.bat", "REM a comment", "REM", Comment),
            ("a.bat", "@echo off", "echo", Keyword),
            ("a.sql", "SELECT * FROM t -- c", "SELECT", Keyword),
            ("a.sql", "select 'it''s' from t", "'it'", String),
            ("a.html", "<div class=\"a\">text</div>", "<div", Tag),
            ("a.html", "<div class=\"a\">text</div>", "\"a\"", String),
            ("a.html", "<p>don't</p>", "don't", Plain),
            ("a.css", "a { color: red; } /* c */", "/* c */", Comment),
            ("a.json", "{ \"key\": \"value\", \"n\": 1 }", "\"key\"", Tag),
            ("a.json", "{ \"key\": \"value\", \"n\": 1 }", "\"value\"", String),
            ("a.json", "  \"on\": true", "true", Keyword),
            ("a.toml", "[package]", "[package]", Tag),
            ("a.toml", "name = \"gezik\" # c", "name", Tag),
            ("a.toml", "name = \"gezik\" # c", "# c", Comment),
            ("a.yaml", "- key: value", "key", Tag),
            ("a.yaml", "url: http://x", ": http", Plain),
            ("a.ini", "; comment", "; comment", Comment),
            ("a.ini", "[section]", "[section]", Tag),
            ("a.md", "# Heading", "# Heading", Keyword),
            ("a.md", "- item with `code` here", "-", Tag),
            ("a.md", "- item with `code` here", "`code`", String),
            ("a.diff", "+added", "+added", String),
            ("a.diff", "-removed", "-removed", Tag),
            ("a.diff", "--- a/x", "--- a/x", Keyword),
            ("a.slint", "export component A inherits Window {}", "export component", Keyword),
            ("Makefile", "CC = gcc", "CC", Tag),
            ("Dockerfile", "FROM alpine AS build", "FROM", Keyword),
        ];
        for &(file, text, word, class) in cases {
            assert_eq!(class_of(file, text, word), class, "{word} in {file}: {text}");
        }
    }

    #[test]
    fn block_comments_and_strings_go_on_to_the_next_lines() {
        use Class::*;
        let lines = colors("a.c", &["int a; /* one", "two", "three */ int b;"]);
        assert_eq!(lines[1], [("two".into(), Comment)]);
        assert_eq!(lines[2][0], ("three */".into(), Comment));
        assert_eq!(lines[2][1], ("int".into(), Type));
        let lines = colors("a.py", &["x = \"\"\"doc", "more", "end\"\"\" + 1"]);
        assert_eq!(lines[1], [("more".into(), String)]);
        assert_eq!(lines[2][0], ("end\"\"\"".into(), String));
        let lines = colors("a.md", &["```rust", "fn x", "```", "text"]);
        assert_eq!(lines[1], [("fn x".into(), String)]);
        assert_eq!(lines[3], [("text".into(), Plain)]);
    }

    #[test]
    fn rust_raw_strings() {
        use Class::*;
        let lines = colors("a.rs", &["let s = r#\"a \" b", "c\"# ; let t = br\"x\";"]);
        assert_eq!(lines[0][2], ("r#\"a \" b".into(), String));
        assert_eq!(lines[1][0], ("c\"#".into(), String));
        assert!(lines[1].contains(&("br\"x\"".into(), String)), "{:?}", lines[1]);
        assert_eq!(class_of("a.rs", "let r = 1;", "r"), Plain, "a variable named r");
    }

    #[test]
    fn a_line_has_at_most_64_parts() {
        let lang = lang_for("a.rs").unwrap();
        let text = "fn x 1 ".repeat(100);
        let mut out = Vec::new();
        line(lang, 0, &text, Some(&mut out));
        assert!(out.len() <= MAX_PARTS, "{}", out.len());
        assert_eq!(out.last().unwrap().1, Class::Plain, "the rest is plain");
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let alphabet = b"\"'`#/*-<>![]{}:=;rb$\\@ \tx1.\n\xC3\xA7";
        for round in 0..10_000 {
            let len = (next() % 80) as usize;
            let bytes: Vec<u8> = (0..len).map(|_| alphabet[(next() % alphabet.len() as u64) as usize]).collect();
            let text = String::from_utf8_lossy(&bytes);
            let lang = &LANGS[round % LANGS.len()];
            let state = (next() % 256) as u8;
            let mut out = Vec::new();
            line(lang, state, &text, Some(&mut out));
            assert!(out.windows(2).all(|w| w[0].0 <= w[1].0), "ordered parts");
            assert!(out.iter().all(|&(at, _)| at <= text.len()));
        }
    }
}
