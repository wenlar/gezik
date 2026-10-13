//! The text language in `vendor/i-slint-core` (see vendor/README.md, spec 15 §3.4): Slint's text
//! is shaped in the language Gezik sets, so Han characters fall back to a Chinese font for
//! Chinese and a Japanese one for Japanese. Without it Windows draws `这` with Yu Gothic, which
//! has no Simplified forms. A missing font (Linux without Noto Sans CJK) skips the font checks.

use i_slint_core::properties::PropertyTracker;
use i_slint_core::textlayout::sharedparley::{fallback_families_for_tests, parley, set_text_locale};

/// The families `text` is drawn with in `locale`, in a fresh font context: fontique fills its
/// fallback slots on the first query, and Simplified Chinese shares its slot with no language.
fn families(locale: Option<&str>, text: &str) -> Vec<String> {
    set_text_locale(locale);
    fallback_families_for_tests(&mut parley::FontContext::default(), text)
}

#[test]
fn han_characters_follow_the_language() {
    // `这` is Simplified only (Japanese fonts lack it); `日` is in both.
    let zh = families(Some("zh-Hans"), "这");
    let zh_shared = families(Some("zh-Hans"), "日");
    let ja = families(Some("ja"), "日");
    let has = |families: &[String], name: &str| families.iter().any(|f| f.contains(name));
    if cfg!(windows) {
        assert!(has(&zh, "YaHei") && has(&zh_shared, "YaHei"), "{zh:?} {zh_shared:?}");
        assert!(has(&ja, "Yu Gothic"), "{ja:?}");
    } else if cfg!(target_os = "macos") {
        assert!(has(&zh, "PingFang") || has(&zh, "Heiti"), "{zh:?}");
        assert!(has(&ja, "Hiragino"), "{ja:?}");
    } else {
        // fontconfig: Noto Sans CJK may serve both from one file (its `locl` picks the forms).
        eprintln!("zh-Hans: {zh:?} {zh_shared:?}, ja: {ja:?}");
    }
}

#[test]
fn latin_text_ignores_the_language() {
    let none = families(None, "Gezik abc");
    assert_eq!(families(Some("zh-Hans"), "Gezik abc"), none);
    assert_eq!(families(Some("ja"), "Gezik abc"), none);
}

#[test]
fn a_new_language_reshapes_the_text() {
    // Shaped text is cached per item and dropped when what the shaping read changes.
    let mut context = parley::FontContext::default();
    let tracker = Box::pin(<PropertyTracker>::default());
    set_text_locale(Some("zh-Hans"));
    tracker.as_ref().evaluate(|| fallback_families_for_tests(&mut context, "a"));
    set_text_locale(Some("zh-Hans"));
    assert!(!tracker.is_dirty(), "the same language keeps the cache");
    set_text_locale(Some("ja"));
    assert!(tracker.is_dirty(), "a cached shaping would keep the old language");
}
