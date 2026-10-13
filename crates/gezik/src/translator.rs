//! Slint's `@tr` through Gezik's catalogue (spec 15 §4.4): Slint asks its external translator
//! (`i-slint-core`'s `tr` feature) for every `@tr` text, then fills the placeholders itself.
//! English sets none: Slint shows the texts as written. `tests/slint_translate.rs` uses this
//! file too.

use std::borrow::Cow;
use std::sync::Arc;

use gezik_i18n::Catalog;
use i_slint_core::translations::Translator as SlintTranslator;

struct Translator(Arc<Catalog>);

impl SlintTranslator for Translator {
    fn translate<'a>(&'a self, string: &'a str, context: Option<&'a str>) -> Cow<'a, str> {
        self.0.get(context.unwrap_or_default(), string)
    }

    fn ntranslate<'a>(&'a self, n: u64, singular: &'a str, plural: &'a str, context: Option<&'a str>) -> Cow<'a, str> {
        self.0.nget(context.unwrap_or_default(), singular, plural, n)
    }
}

/// Hands `catalog` (`None`: English) to Slint, which evaluates every `@tr` again.
pub fn install(catalog: Option<Arc<Catalog>>) {
    let translator = catalog.map(|catalog| Box::new(Translator(catalog)) as Box<dyn SlintTranslator>);
    let no_platform = || Err(i_slint_core::platform::PlatformError::NoPlatform);
    let _ = i_slint_core::context::with_global_context(no_platform, |ctx| ctx.set_external_translator(translator));
}
