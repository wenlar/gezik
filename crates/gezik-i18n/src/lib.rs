//! Gezik's translations (spec 15). The source texts are English and are the keys (gettext's
//! way): `t!("Copy Path")` gives the text in the language in use. English loads nothing: no
//! catalogue is set and `t!` returns its text as written. Any crate may use it (no
//! dependencies but `miniz_oxide`).
//!
//! ```
//! use gezik_i18n::{t, tn, tx};
//! let name = "a.txt";
//! assert_eq!(t!("Copy Path"), "Copy Path");
//! assert_eq!(t!("Moved {0} to {1}", name, "b"), "Moved a.txt to b");
//! assert_eq!(t!("menu" => "Open"), "Open");
//! assert_eq!(tn!("{n} item" | "{n} items", 3), "3 items");
//! const TITLE: &str = tx!("New Tab"); // only marked for `gezik-lang extract`
//! # let _ = TITLE;
//! ```

pub mod blob;
pub mod catalog;
pub mod check;
pub mod format;
pub mod lang;
pub mod plural;
pub mod po;
pub mod pseudo;

use std::borrow::Cow;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

pub use catalog::{Builtin, Catalog, Layer};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

/// The built-in languages (their `.po` files in `lang/`).
pub fn builtins() -> &'static [Builtin] {
    embedded::BUILTINS
}

/// The template's (`lang/gezik.pot`) key hashes, sorted.
pub fn keys() -> &'static [u64] {
    embedded::KEYS
}

/// The language in use; `None` is English.
static CURRENT: RwLock<Option<Arc<Catalog>>> = RwLock::new(None);

/// No catalogue is set: `t!` gives its text back without taking the lock or allocating.
static ENGLISH: AtomicBool = AtomicBool::new(true);

/// Makes `catalog` the language in use (`None`: English).
pub fn set(catalog: Option<Arc<Catalog>>) {
    let mut current = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
    ENGLISH.store(catalog.is_none(), Ordering::Release);
    *current = catalog;
}

/// `found` as `'static`: borrowed when it is the source text itself.
fn keep(found: Cow<'_, str>, source: &'static str) -> Cow<'static, str> {
    match found {
        Cow::Borrowed(s) if std::ptr::eq(s, source) => Cow::Borrowed(source),
        other => Cow::Owned(other.into_owned()),
    }
}

/// The language in use; `None` is English.
pub fn current() -> Option<Arc<Catalog>> {
    CURRENT.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// `id` (in `context`, `""` for none) in the language in use. `t!` comes here.
/// English costs one atomic load: the text itself, borrowed.
pub fn t(context: &str, id: &'static str) -> Cow<'static, str> {
    if ENGLISH.load(Ordering::Acquire) {
        return Cow::Borrowed(id);
    }
    match &*CURRENT.read().unwrap_or_else(PoisonError::into_inner) {
        None => Cow::Borrowed(id),
        Some(catalog) => keep(catalog.get(context, id), id),
    }
}

/// The form of `singular`/`plural` that `n` takes in the language in use. `tn!` comes here.
pub fn tn(context: &str, singular: &'static str, plural: &'static str, n: u64) -> Cow<'static, str> {
    let english = if n == 1 { singular } else { plural };
    if ENGLISH.load(Ordering::Acquire) {
        return Cow::Borrowed(english);
    }
    match &*CURRENT.read().unwrap_or_else(PoisonError::into_inner) {
        None => Cow::Borrowed(english),
        Some(catalog) => keep(catalog.nget(context, singular, plural, n), english),
    }
}

/// A count as `tn!` takes it (any integer; a negative one as the largest).
pub fn count(n: impl TryInto<u64>) -> u64 {
    n.try_into().unwrap_or(u64::MAX)
}

/// `template` with its placeholders (`{}`, `{0}`, `{n}`) filled.
pub fn format(template: &str, args: &[&dyn Display], n: Option<u64>) -> String {
    format::format(template, args, n)
}

/// A text in the language in use: `t!("Open")`, `t!("Moved {} to {}", a, b)`, with a context
/// `t!("menu" => "Open")`. Placeholders as Slint's `@tr`.
#[macro_export]
macro_rules! t {
    ($context:literal => $id:literal) => {
        $crate::t($context, $id)
    };
    ($context:literal => $id:literal, $($arg:expr),+ $(,)?) => {
        $crate::format(&$crate::t($context, $id), &[$(&$arg as &dyn ::core::fmt::Display),+], None)
    };
    ($id:literal) => {
        $crate::t("", $id)
    };
    ($id:literal, $($arg:expr),+ $(,)?) => {
        $crate::format(&$crate::t("", $id), &[$(&$arg as &dyn ::core::fmt::Display),+], None)
    };
}

/// A text with a count: `tn!("{n} item" | "{n} items", count)`, more arguments after it.
#[macro_export]
macro_rules! tn {
    ($singular:literal | $plural:literal, $n:expr $(, $arg:expr)* $(,)?) => {{
        let n = $crate::count($n);
        $crate::format(&$crate::tn("", $singular, $plural, n), &[$(&$arg as &dyn ::core::fmt::Display),*], Some(n))
    }};
}

/// Marks a text for `gezik-lang extract` and gives it back as written (gettext's `N_`): for
/// tables of `&'static str` translated where shown, `t(title)`.
#[macro_export]
macro_rules! tx {
    ($id:literal) => {
        $id
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test touches the language in use: tests run in parallel.
    #[test]
    fn english_loads_nothing_and_a_catalogue_changes_every_text() {
        assert!(current().is_none(), "English: no catalogue");
        assert_eq!(t!("Settings"), "Settings");
        assert!(matches!(t!("Settings"), Cow::Borrowed(_)), "English allocates nothing");
        assert_eq!(tn!("{n} file" | "{n} files", 1), "1 file");
        assert_eq!(tn!("{n} file in {}" | "{n} files in {}", 2, "Docs"), "2 files in Docs");
        set(Some(Arc::new(Catalog::pseudo())));
        assert_eq!(t!("Settings"), "«Ŝéţţıñĝš ···»");
        assert_eq!(t!("menu" => "Open {}", "x"), "«Öpéñ x ··»");
        assert_eq!(tn!("{n} file" | "{n} files", 2usize), "«2 fıléš ··»");
        set(None);
        assert_eq!(t!("Settings"), "Settings");
        assert!(keys().is_sorted());
        assert!(
            builtins().iter().all(|b| lang::endonym(b.code).is_some()),
            "a built-in language has its name in lang.rs"
        );
    }
}
