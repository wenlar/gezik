//! Placeholders in texts, with Slint's syntax so `@tr` and `t!` read alike (spec 15 §4.1): `{}`
//! takes the next argument, `{0}` a given one, `{n}` the count of a plural, `{{` and `}}` are
//! braces. A translation may put the arguments in another order. The same steps as
//! `i_slint_core::translations::formatter`; the tests are its own.

use std::collections::BTreeSet;
use std::fmt::{Display, Write};

/// `template` with its placeholders filled; one that has no argument stays as written.
pub fn format(template: &str, args: &[&dyn Display], n: Option<u64>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut next = 0;
    let mut pos = 0;
    while let Some(found) = template[pos..].find(['{', '}']) {
        let p = pos + found;
        if template[p..].starts_with('}') {
            // `}}` is one brace; a lone `}` stays.
            out.push_str(&template[pos..=p]);
            pos = if template[p + 1..].starts_with('}') { p + 2 } else { p + 1 };
            continue;
        }
        if template[p + 1..].starts_with('{') {
            out.push_str(&template[pos..=p]);
            pos = p + 2;
            continue;
        }
        // No `}` after it: no placeholder follows either (stopping keeps this linear; Slint's
        // own formatter would look for one from every later `{`).
        let Some(end) = template[p..].find('}').map(|e| p + e) else { break };
        out.push_str(&template[pos..p]);
        let name = template[p + 1..end].trim();
        let arg = if end == p + 1 {
            next += 1;
            args.get(next - 1)
        } else if let Ok(i) = name.parse::<usize>() {
            args.get(i)
        } else {
            None
        };
        // Written in place: no text per argument. `{n}` grouped as the language does.
        match (arg, n) {
            (Some(arg), _) => {
                let _ = write!(out, "{arg}");
            }
            (None, Some(n)) if name == "n" => crate::number().write_grouped(&mut out, n),
            _ => out.push_str(&template[p..=end]),
        }
        pos = end + 1;
    }
    out.push_str(&template[pos..]);
    out
}

/// The placeholders of `text`, each `{}` numbered by its place (`{}`, `{}` → `{0}`, `{1}`): two
/// texts that take the same arguments have the same set, in any order.
pub fn placeholders(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut next = 0;
    let mut rest = text;
    while let Some(p) = rest.find('{') {
        if rest[p + 1..].starts_with('{') {
            rest = &rest[p + 2..];
            continue;
        }
        let Some(end) = rest[p..].find('}').map(|e| p + e) else { break };
        let name = rest[p + 1..end].trim();
        let mut item = String::from("{");
        if name.is_empty() {
            let _ = write!(item, "{next}");
            next += 1;
        } else {
            item.push_str(name);
        }
        item.push('}');
        out.insert(item);
        rest = &rest[end + 1..];
    }
    out
}

/// Whether `text` has a `{` that no `}` closes (`{{` is a brace): a translation with one is
/// refused, as Slint's formatter is slow on it.
pub fn unclosed(text: &str) -> bool {
    let mut rest = text;
    while let Some(p) = rest.find('{') {
        if rest[p + 1..].starts_with('{') {
            rest = &rest[p + 2..];
            continue;
        }
        let Some(end) = rest[p..].find('}') else { return true };
        rest = &rest[p + end + 1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unclosed_braces_are_linear() {
        let long = "{a".repeat(500_000);
        let start = std::time::Instant::now();
        assert_eq!(format(&long, &[], None), long);
        assert!(unclosed(&long));
        // Quadratic would be ~10^11 steps here; linear is milliseconds even in a debug build.
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "{:?}", start.elapsed());
        assert_eq!(format("{} and {", &[&1], None), "1 and {");
        assert!(!unclosed("{} {{ x }} {n}") && unclosed("ok {} {") && !unclosed(""));
    }

    #[test]
    fn as_slint_formats() {
        // i_slint_core::translations::formatter's tests.
        assert_eq!(format("Hello", &[], None), "Hello");
        assert_eq!(format("Hello {}!", &[&"world"], None), "Hello world!");
        assert_eq!(format("Hello {0}!", &[&"world"], None), "Hello world!");
        assert_eq!(format("Hello -{1}- -{0}-", &[&(40 + 5), &"World"], None), "Hello -World- -45-");
        let inner = format("Hello {{}}!", &[], None);
        assert_eq!(format(&inner, &[&format("{}", &[&"world"], None)], None), "Hello world!");
        assert_eq!(format("Hello -{}- -{}-", &[&(40 + 5), &"World"], None), "Hello -45- -World-");
        assert_eq!(format("Hello {{0}} {}", &[&"world"], None), "Hello {0} world");
        // And Gezik's: the count, a missing argument, a lone brace, text after a multi-byte letter.
        assert_eq!(format("{n} öğe, {}", &[&"x"], Some(3)), "3 öğe, x");
        assert_eq!(format("{n} files", &[], Some(1234)), "1,234 files", "grouped as the language does");
        assert_eq!(format("{n} {2} {name}", &[], None), "{n} {2} {name}");
        assert_eq!(format("a } b { c", &[], None), "a } b { c");
        assert_eq!(format("ş{}ş", &[&1], None), "ş1ş");
    }

    #[test]
    fn placeholder_sets() {
        let set = |s: &str| placeholders(s).into_iter().collect::<Vec<_>>();
        assert_eq!(set("Moved {} to {}"), ["{0}", "{1}"]);
        assert_eq!(placeholders("Moved {} to {}"), placeholders("{1} ← {0}"));
        assert_eq!(set("{n} files in {}"), ["{0}", "{n}"]);
        assert_eq!(set("{{literal}} {"), Vec::<String>::new());
        assert_ne!(placeholders("{} and {}"), placeholders("{}"));
    }
}
