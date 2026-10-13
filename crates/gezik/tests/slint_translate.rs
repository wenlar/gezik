//! Slint's `@tr` through Gezik's catalogue (spec 15 §4.4): the generated code calls
//! `i_slint_core::translations::translate`, which asks the external translator `translator.rs`
//! installs. If a Slint upgrade changes that path, this fails (see vendor/README.md).

#[path = "../src/translator.rs"]
mod translator;

use std::rc::Rc;
use std::sync::Arc;

use gezik_i18n::Catalog;
use gezik_i18n::pseudo::pseudo;
use i_slint_core::SharedString;
use i_slint_core::platform::{Platform, PlatformError, WindowAdapter};
use i_slint_core::properties::Property;
use i_slint_core::translations::translate;

struct NoWindows;

impl Platform for NoWindows {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Err(PlatformError::NoPlatform)
    }
}

const PL: &str = "msgid \"\"\nmsgstr \"Plural-Forms: nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);\\n\"\n\nmsgid \"Settings — Gezik\"\nmsgstr \"Ustawienia — Gezik\"\n\nmsgctxt \"menu\"\nmsgid \"{n} file\"\nmsgid_plural \"{n} files\"\nmsgstr[0] \"{n} plik\"\nmsgstr[1] \"{n} pliki\"\nmsgstr[2] \"{n} plików\"\n";

/// The (context, text) of every `@tr` as the build compiled it (`build.rs`'s config): the first
/// two arguments of each `translate` call in the generated code.
fn compiled_tr() -> Vec<(String, String)> {
    let code = std::fs::read_to_string(concat!(env!("OUT_DIR"), "/app.rs")).unwrap();
    const CALL: &str = "private_unstable_api :: translate (";
    const ARG: &str = "(sp :: SharedString :: from (";
    // A Rust string literal as `quote!` writes it: its text, and the code after it.
    fn literal(s: &str) -> (String, &str) {
        let body = s.strip_prefix('"').unwrap();
        let (mut out, mut chars) = (String::new(), body.char_indices());
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return (out, &body[i + 1..]),
                '\\' => out.push(match chars.next().unwrap().1 {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                }),
                c => out.push(c),
            }
        }
        panic!("an unclosed literal in app.rs");
    }
    let mut out = Vec::new();
    for call in code.split(CALL).skip(1) {
        let Some(rest) = call.strip_prefix(ARG) else { continue };
        let (id, rest) = literal(rest);
        let Some(rest) = rest.split_once(ARG).map(|(_, r)| r) else { continue };
        let (context, _) = literal(rest);
        out.push((context, id));
    }
    out
}

/// What `gezik-lang extract` wrote is what the code asks for: the same contexts.
#[test]
fn every_compiled_tr_is_in_the_template() {
    let pot = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../gezik-i18n/lang/gezik.pot")).unwrap();
    let pot = gezik_i18n::po::parse(&pot).unwrap();
    let calls = compiled_tr();
    assert!(calls.iter().any(|(_, id)| id == "Settings — Gezik"), "{calls:?}");
    for (context, id) in calls {
        let found = pot.entries.iter().any(|e| e.context == context && e.id == id);
        assert!(found, "@tr({context:?} => {id:?}) is not in gezik.pot as Slint calls it");
    }
}

#[test]
fn tr_follows_the_catalogue_and_comes_back_to_english() {
    i_slint_core::platform::set_platform(Box::new(NoWindows)).unwrap();
    // The title's call as compiled (its context included), not one written here.
    let (context, id) = compiled_tr().into_iter().find(|(_, id)| id == "Settings — Gezik").unwrap();
    let title = Box::pin(Property::<SharedString>::default());
    title.set_binding(move || translate(&id, &context, "gezik", &[] as &[SharedString], 1, ""));
    let files = Box::pin(Property::<SharedString>::default());
    files.set_binding(|| translate("{n} file", "menu", "", &[] as &[SharedString], 5, "{n} files"));
    assert_eq!(title.as_ref().get(), "Settings — Gezik", "no translator: the text as written");
    assert_eq!(files.as_ref().get(), "5 files");

    // A binding already evaluated is evaluated again when the language changes.
    let layer = gezik_i18n::check::load(PL, "pl", &[]).0.unwrap();
    translator::install(Catalog::open("pl", Some(Arc::new(layer))).map(Arc::new));
    assert_eq!(title.as_ref().get(), "Ustawienia — Gezik");
    assert_eq!(files.as_ref().get(), "5 plików", "the language's plural, Slint's {{n}}");

    translator::install(Some(Arc::new(Catalog::pseudo())));
    assert_eq!(title.as_ref().get(), pseudo("Settings — Gezik"));
    assert_eq!(files.as_ref().get(), pseudo("{n} files").replace("{n}", "5"));

    translator::install(None);
    assert_eq!(title.as_ref().get(), "Settings — Gezik");
}
