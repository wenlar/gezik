//! Language codes (spec 15 §5): the system's names (`tr-TR`, `zh_CN.UTF-8`, `zh-Hans-CN`)
//! matched to the languages Gezik has, and the language the `language` setting picks.

/// The built-in languages' codes and names in their own language (once their `.po` is in).
pub const NAMES: &[(&str, &str)] = &[
    ("en", "English"),
    ("tr", "Türkçe"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fr", "Français"),
    ("zh-Hans", "简体中文"),
];

/// The setting's value that follows the system.
pub const AUTO: &str = "auto";
/// The setting's value that shows the pseudo language.
pub const PSEUDO: &str = "pseudo";

/// A language's name in its own language, for the codes Gezik knows.
pub fn endonym(code: &str) -> Option<&'static str> {
    NAMES.iter().find(|(c, _)| c.eq_ignore_ascii_case(code)).map(|(_, name)| *name)
}

/// Whether `code` can name a language (and a file in the lang folder): a letter, then
/// letters, digits, `-` or `_`, at most 32.
pub fn valid_code(code: &str) -> bool {
    code.len() <= 32
        && code.starts_with(|c: char| c.is_ascii_alphabetic())
        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Whether a lang folder file may be named `code`: a code, but not `auto` or `pseudo` (which
/// the setting's own values would then mean).
pub fn file_code(code: &str) -> bool {
    valid_code(code) && !code.eq_ignore_ascii_case(AUTO) && !code.eq_ignore_ascii_case(PSEUDO)
}

/// A system's name for a language without its encoding and variant, with `-`:
/// `zh_CN.UTF-8@x` → `zh-CN`; `C.UTF-8` → `C`.
pub fn normalize(tag: &str) -> String {
    let tag = tag.split(['.', '@']).next().unwrap_or_default().trim();
    tag.replace('_', "-")
}

/// The first of the user's languages (`preferred`, most wanted first) that one of `available`
/// serves: the same code, then the script (`zh-CN` → `zh-Hans`; Traditional Chinese never
/// gets Simplified), then the language alone (`de-AT` → `de`); `en` if none does.
pub fn choose(preferred: &[String], available: &[&str]) -> String {
    // English is always there: the source texts.
    let have = |code: &str| {
        let found = ["en"].iter().chain(available).find(|a| a.eq_ignore_ascii_case(code));
        found.map(|a| (*a).to_owned())
    };
    for tag in preferred {
        let tag = normalize(tag);
        if tag.is_empty() {
            continue;
        }
        if tag == "C" || tag == "POSIX" {
            return "en".to_owned();
        }
        if let Some(code) = have(&tag) {
            return code;
        }
        let lower = tag.to_ascii_lowercase();
        let mut parts = lower.split('-');
        let base = parts.next().unwrap_or_default();
        if base == "zh" {
            let traditional = parts.any(|p| matches!(p, "hant" | "tw" | "hk" | "mo"));
            if let Some(code) = have(if traditional { "zh-Hant" } else { "zh-Hans" }) {
                return code;
            }
            continue;
        }
        if let Some(code) = have(base) {
            return code;
        }
    }
    "en".to_owned()
}

/// The language `setting` (the `language` value) shows: `auto`, and a code Gezik does not
/// have, follow the system; `pseudo` is the pseudo language.
pub fn resolve(setting: &str, preferred: &[String], available: &[&str]) -> String {
    if setting.eq_ignore_ascii_case(PSEUDO) {
        return PSEUDO.to_owned();
    }
    if setting.eq_ignore_ascii_case(AUTO) {
        return choose(preferred, available);
    }
    if setting.eq_ignore_ascii_case("en") {
        return "en".to_owned();
    }
    match available.iter().find(|a| a.eq_ignore_ascii_case(setting)) {
        Some(code) => (*code).to_owned(),
        None => choose(preferred, available),
    }
}

/// Whether `setting` names a language Gezik has (else a warning: it follows the system).
pub fn known(setting: &str, available: &[&str]) -> bool {
    [AUTO, PSEUDO, "en"].iter().chain(available).any(|code| code.eq_ignore_ascii_case(setting))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(preferred: &[&str], available: &[&str]) -> String {
        choose(&preferred.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(), available)
    }

    const ALL: &[&str] = &["tr", "de", "es", "fr", "zh-Hans"];

    #[test]
    fn the_systems_names_find_the_language() {
        assert_eq!(pick(&["tr-TR"], ALL), "tr");
        assert_eq!(pick(&["tr_TR.UTF-8"], ALL), "tr");
        assert_eq!(pick(&["de-AT", "en-US"], ALL), "de");
        assert_eq!(pick(&["zh-CN"], ALL), "zh-Hans");
        assert_eq!(pick(&["zh_CN.UTF-8"], ALL), "zh-Hans");
        assert_eq!(pick(&["zh-SG"], ALL), "zh-Hans");
        assert_eq!(pick(&["zh-Hans-CN"], ALL), "zh-Hans", "macOS");
        assert_eq!(pick(&["zh"], ALL), "zh-Hans");
        for traditional in ["zh-TW", "zh_TW.UTF-8", "zh-HK", "zh-MO", "zh-Hant", "zh-Hant-TW"] {
            assert_eq!(pick(&[traditional], ALL), "en", "{traditional} never gets Simplified");
            assert_eq!(pick(&[traditional], &["zh-Hant", "zh-Hans"]), "zh-Hant", "a dropped file serves it");
        }
        assert_eq!(pick(&["pt-BR"], &["pt"]), "pt", "a dropped file");
        assert_eq!(pick(&["pt-BR"], &["pt", "pt-BR"]), "pt-BR", "the exact one first");
        assert_eq!(pick(&["fr@euro"], ALL), "fr");
    }

    #[test]
    fn order_c_and_nothing() {
        assert_eq!(pick(&["pl-PL", "de-DE", "tr-TR"], ALL), "de", "the first one served");
        assert_eq!(pick(&["en-US", "tr-TR"], ALL), "en", "English first stays English");
        assert_eq!(pick(&["C"], ALL), "en");
        assert_eq!(pick(&["C.UTF-8", "tr"], ALL), "en");
        assert_eq!(pick(&["POSIX"], ALL), "en");
        assert_eq!(pick(&[], ALL), "en");
        assert_eq!(pick(&["", "ja-JP"], ALL), "en");
        assert_eq!(pick(&["tr-TR"], &[]), "en", "no languages yet (15a)");
    }

    #[test]
    fn the_setting() {
        let system = vec!["tr-TR".to_owned()];
        assert_eq!(resolve("auto", &system, ALL), "tr");
        assert_eq!(resolve("de", &system, ALL), "de");
        assert_eq!(resolve("DE", &system, ALL), "de");
        assert_eq!(resolve("en", &system, ALL), "en");
        assert_eq!(resolve("pseudo", &system, ALL), "pseudo");
        assert_eq!(resolve("xx", &system, ALL), "tr", "unknown: the system's");
        assert_eq!(resolve("auto", &system, &["auto", "tr"]), "tr", "a file named auto never wins");
        assert!(known("auto", &[]) && known("pseudo", &[]) && known("en", &[]) && known("pl", &["pl"]));
        assert!(!known("xx", ALL));
        assert!(valid_code("zh-Hans") && valid_code("pt_BR") && !valid_code("../x") && !valid_code("1a"));
        assert!(!valid_code(&"a".repeat(33)) && !valid_code(""));
        assert!(file_code("pl") && file_code("en") && !file_code("auto") && !file_code("PSEUDO"));
        assert_eq!(endonym("TR"), Some("Türkçe"));
    }
}
