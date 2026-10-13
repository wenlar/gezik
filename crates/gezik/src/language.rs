//! The interface language (spec 15 §5): picked from the `language` setting and the system's
//! languages, and made the one `t!` and Slint's `@tr` show when it changes. The lang folder's
//! files were read off this thread (`ConfigStore::read_files`); here a built-in language is
//! only inflated. English loads nothing.

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use gezik_config::store::ConfigFiles;
use gezik_i18n::{Catalog, Layer, lang, t};

/// The system's languages, read once: a change shows at the next start (spec 15 §5.3).
fn system() -> &'static [String] {
    static SYSTEM: OnceLock<Vec<String>> = OnceLock::new();
    SYSTEM.get_or_init(gezik_platform::preferred_languages)
}

/// The built-in languages' codes and the lang folder's.
fn available(files: &ConfigFiles) -> Vec<&str> {
    let builtin = gezik_i18n::builtins().iter().map(|b| b.code);
    builtin.chain(files.languages.iter().map(|(code, _)| code.as_str())).collect()
}

/// The lang folder's file of `code`.
fn file<'a>(code: &str, files: &'a ConfigFiles) -> Option<&'a Arc<Layer>> {
    files.languages.iter().find(|(c, _)| c.eq_ignore_ascii_case(code)).map(|(_, layer)| layer)
}

thread_local! {
    /// The language shown and its file (by identity: a reload that read it again shows it again).
    static SHOWN: RefCell<(String, Option<Arc<Layer>>)> = RefCell::new(("en".to_owned(), None));
}

/// Makes the language `setting` names the one in use, if it or its file changed: whether it did.
pub fn apply(setting: &str, files: &ConfigFiles) -> bool {
    let code = lang::resolve(setting, system(), &available(files));
    // Before the check below, which skips English at the start: Han characters in the language
    // the reader expects (spec 15 §3.4), set before the first one is drawn.
    i_slint_core::textlayout::sharedparley::set_text_locale(Some(&lang::text_locale(&code, system())));
    let file = file(&code, files).cloned();
    let same = SHOWN.with(|shown| {
        let shown = shown.borrow();
        shown.0 == code && shown.1.as_ref().map(Arc::as_ptr) == file.as_ref().map(Arc::as_ptr)
    });
    if same {
        return false;
    }
    SHOWN.with(|shown| *shown.borrow_mut() = (code.clone(), file.clone()));
    let catalog = Catalog::open(&code, file).map(Arc::new);
    gezik_i18n::set(catalog.clone());
    crate::translator::install(catalog);
    // What Rust holds of the texts: the windows' sidebars, menus and Settings follow the
    // reload that brought the change (`apply_config`); the tray's menu (macOS keeps it built).
    crate::resident::refresh_pins();
    // The app menu Slint asks muda for (About, Hide, Quit Gezik …): made with the first
    // window, so a later change shows at the next start, as AppKit's own items do.
    #[cfg(target_os = "macos")]
    i_slint_backend_winit::set_app_menu_texts(
        [t!("About Gezik"), t!("Services"), t!("Hide Gezik"), t!("Hide Others"), t!("Show All"), t!("Quit Gezik")]
            .map(std::borrow::Cow::into_owned),
    );
    true
}

/// The language row's choices (spec 15 §5.1): `code — name`, the first part is written.
pub fn options(files: &ConfigFiles) -> Vec<String> {
    let auto = lang::resolve(lang::AUTO, system(), &available(files));
    let name = |code: &str| {
        let builtin = gezik_i18n::builtins().iter().find(|b| b.code == code).map(|b| b.name);
        let named = file(code, files).map(|layer| layer.name.as_str()).or(builtin).or(lang::endonym(code));
        named.unwrap_or(code).to_owned()
    };
    let total = gezik_i18n::keys().len();
    let mut out = vec![format!("{} — {}", lang::AUTO, t!("System: {}", name(&auto))), "en — English".to_owned()];
    for b in gezik_i18n::builtins().iter().filter(|b| file(b.code, files).is_none()) {
        out.push(format!("{} — {}{}", b.code, b.name, state(b.human, b.translated, b.fuzzy, total)));
    }
    for (code, layer) in &files.languages {
        out.push(format!("{code} — {}{}", layer.name, state(layer.human, layer.translated, layer.fuzzy, total)));
    }
    out.push(format!("{} — {}", lang::PSEUDO, t!("Pseudo (for translators)")));
    out
}

/// How far a language is: machine translated, partly reviewed, partly translated.
fn state(human: bool, translated: usize, fuzzy: usize, total: usize) -> String {
    let percent = |part: usize, of: usize| part * 100 / of.max(1);
    if !human && fuzzy == translated && translated > 0 {
        format!(" — {}", t!("machine translated"))
    } else if fuzzy > 0 {
        format!(" — {}", t!("{} % reviewed", percent(translated - fuzzy, translated)))
    } else if translated < total {
        format!(" — {}", t!("{} % translated", percent(translated, total)))
    } else {
        String::new()
    }
}

/// The code of a row's choice (`pl — Polski` → `pl`).
pub fn code_of(choice: &str) -> &str {
    choice.split(" — ").next().unwrap_or(choice).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_and_their_codes() {
        let mut files = ConfigFiles::default();
        let po = "msgid \"\"\nmsgstr \"X-Gezik-Name: Polski\\n\"\n";
        let layer = gezik_i18n::check::load(po, "pl", &[]).0.unwrap();
        files.languages.push(("pl".to_owned(), Arc::new(layer)));
        let options = options(&files);
        assert!(options[0].starts_with("auto — System: "), "{options:?}");
        let pl = "pl — Polski — 0 % translated";
        // The built-in languages sit between English and pseudo; a dropped file joins them.
        assert_eq!(options[1], "en — English");
        assert!(options.contains(&pl.to_owned()), "{options:?}");
        assert_eq!(options.last().unwrap(), "pseudo — Pseudo (for translators)");
        let codes: Vec<_> = options.iter().map(|o| code_of(o)).collect();
        assert_eq!(codes.first(), Some(&"auto"));
        for code in ["tr", "de", "es", "fr", "zh-Hans", "pl"] {
            assert!(codes.contains(&code), "{code}: {codes:?}");
        }
        assert_eq!(state(false, 10, 10, 10), " — machine translated");
        assert_eq!(state(false, 10, 6, 10), " — 40 % reviewed");
        assert_eq!(state(true, 62, 0, 100), " — 62 % translated");
        assert_eq!(state(true, 100, 0, 100), "");
    }
}
