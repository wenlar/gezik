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

#[test]
fn tr_follows_the_catalogue_and_comes_back_to_english() {
    i_slint_core::platform::set_platform(Box::new(NoWindows)).unwrap();
    let title = Box::pin(Property::<SharedString>::default());
    title.set_binding(|| translate("Settings — Gezik", "", "", &[] as &[SharedString], 0, ""));
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
