//! How the language in use writes numbers (spec 15 §8.1): its decimal and digit group
//! separators. Gezik's own numbers follow the interface language, not the OS region.

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

/// A language's number separators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Number {
    pub decimal: char,
    pub group: char,
    /// Digits a number needs before it is grouped: 4 (`1,234`), or 5 where CLDR's minimum
    /// grouping digits is 2 (Spanish `1234`, `12.345`).
    pub group_from: u8,
}

impl Number {
    pub const ENGLISH: Number = Number { decimal: '.', group: ',', group_from: 4 };

    /// A built-in language's (CLDR); `None` for others.
    pub fn builtin(code: &str) -> Option<Number> {
        let (decimal, group, group_from) = match code {
            "en" | "zh-Hans" => ('.', ',', 4),
            "tr" | "de" => (',', '.', 4),
            "es" => (',', '.', 5),
            "fr" => (',', '\u{202f}', 4),
            _ => return None,
        };
        Some(Number { decimal, group, group_from })
    }

    /// A dropped file's `X-Gezik-Number: decimal=",", group="."` (both needed, one character
    /// each, quoted).
    pub fn parse(header: &str) -> Option<Number> {
        let (mut decimal, mut group) = (None, None);
        // Not split at `,`: it is a value too.
        let mut rest = header.trim();
        while !rest.is_empty() {
            let (name, value) = rest.split_once('=')?;
            let mut chars = value.trim_start().strip_prefix('"')?.chars();
            let c = chars.next()?;
            rest = chars.as_str().strip_prefix('"')?.trim_start();
            rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
            match name.trim() {
                "decimal" => decimal = Some(c),
                "group" => group = Some(c),
                _ => return None,
            }
        }
        Some(Number { decimal: decimal?, group: group?, group_from: 4 })
    }

    /// `n` with its digits grouped by three (`1,234,567`), written to `out` without allocating
    /// more than `out` needs.
    pub fn write_grouped(self, out: &mut String, n: u64) {
        let mut buf = [0u8; 20];
        let mut at = buf.len();
        let mut rest = n;
        loop {
            at -= 1;
            buf[at] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        let digits = &buf[at..];
        let grouped = digits.len() >= usize::from(self.group_from);
        for (i, &d) in digits.iter().enumerate() {
            if grouped && i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(self.group);
            }
            out.push(char::from(d));
        }
    }

    /// `n` grouped (`1,234,567`).
    pub fn grouped(self, n: u64) -> String {
        let mut out = String::with_capacity(26);
        self.write_grouped(&mut out, n);
        out
    }

    /// `x` with `places` decimals (`1.5`, `1,5`), written to `out`; no grouping.
    pub fn write_decimal(self, out: &mut String, x: f64, places: usize) {
        let start = out.len();
        let _ = write!(out, "{x:.places$}");
        if self.decimal != '.'
            && let Some(dot) = out[start..].find('.')
        {
            let mut c = [0u8; 4];
            out.replace_range(start + dot..=start + dot, self.decimal.encode_utf8(&mut c));
        }
    }

    fn pack(self) -> u64 {
        u64::from(self.decimal) | u64::from(self.group) << 21 | u64::from(self.group_from) << 42
    }

    fn unpack(bits: u64) -> Number {
        let c = |b: u64| char::from_u32((b & 0x1f_ffff) as u32).unwrap_or('?');
        Number { decimal: c(bits), group: c(bits >> 21), group_from: (bits >> 42) as u8 }
    }
}

/// The language in use's separators, read per row: one atomic, no lock.
static CURRENT: AtomicU64 = AtomicU64::new(
    '.' as u64 | (',' as u64) << 21 | 4 << 42, // Number::ENGLISH
);

pub(crate) fn set(number: Number) {
    CURRENT.store(number.pack(), Ordering::Relaxed);
}

/// How the language in use writes numbers.
pub fn number() -> Number {
    Number::unpack(CURRENT.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grouped(code: &str, n: u64) -> String {
        Number::builtin(code).unwrap().grouped(n)
    }

    fn decimal(code: &str, x: f64) -> String {
        let mut out = String::new();
        Number::builtin(code).unwrap().write_decimal(&mut out, x, 1);
        out
    }

    #[test]
    fn every_built_in_language_writes_its_numbers() {
        assert_eq!(Number::unpack(Number::ENGLISH.pack()), Number::ENGLISH);
        assert_eq!(number(), Number::ENGLISH, "English until a language is set");
        for (code, big, four, small, dec) in [
            ("en", "1,234,567", "1,234", "999", "1.5"),
            ("zh-Hans", "1,234,567", "1,234", "999", "1.5"),
            ("tr", "1.234.567", "1.234", "999", "1,5"),
            ("de", "1.234.567", "1.234", "999", "1,5"),
            ("es", "1.234.567", "1234", "999", "1,5"),
            ("fr", "1\u{202f}234\u{202f}567", "1\u{202f}234", "999", "1,5"),
        ] {
            assert_eq!(grouped(code, 1_234_567), big, "{code}");
            assert_eq!(grouped(code, 1234), four, "{code}");
            assert_eq!(grouped(code, 999), small, "{code}");
            assert_eq!(decimal(code, 1.5), dec, "{code}");
            assert_eq!(Number::unpack(Number::builtin(code).unwrap().pack()), Number::builtin(code).unwrap());
        }
        assert_eq!(grouped("es", 12_345), "12.345");
        assert_eq!(grouped("en", 0), "0");
        assert_eq!(grouped("en", u64::MAX), "18,446,744,073,709,551,615");
        assert_eq!(Number::builtin("pl"), None);
    }

    #[test]
    fn a_dropped_file_names_its_separators() {
        let pl = Number::parse("decimal=\",\", group=\"\u{a0}\"").unwrap();
        assert_eq!((pl.decimal, pl.group), (',', '\u{a0}'));
        assert_eq!(Number::parse("group=\".\", decimal=\",\"").map(|n| n.decimal), Some(','));
        for bad in ["", "decimal=\",\"", "decimal=,, group=.", "decimal=\",,\", group=\".\"", "x=\"1\""] {
            assert_eq!(Number::parse(bad), None, "{bad}");
        }
    }
}
