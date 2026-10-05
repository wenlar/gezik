//! Changing the letter case of names, with Turkish and Azerbaijani i/İ and ı/I.

/// Which letter rules to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// Turkish and Azerbaijani: i ↔ İ, ı ↔ I.
    Turkic,
    #[default]
    Other,
}

impl Lang {
    /// From a language code (`tr`, `tr-TR`, `az_AZ.UTF-8`, `en-US`).
    pub fn from_code(code: &str) -> Lang {
        let lang = code.split(['-', '_', '.']).next().unwrap_or_default().to_ascii_lowercase();
        if lang == "tr" || lang == "az" { Lang::Turkic } else { Lang::Other }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    Lower,
    Upper,
    /// Every word starts with a capital, the rest is small.
    Title,
    /// Only the first letter is a capital.
    Sentence,
}

impl CaseMode {
    pub const ALL: [CaseMode; 4] = [CaseMode::Lower, CaseMode::Upper, CaseMode::Title, CaseMode::Sentence];

    pub fn as_str(self) -> &'static str {
        match self {
            CaseMode::Lower => "lower",
            CaseMode::Upper => "upper",
            CaseMode::Title => "title",
            CaseMode::Sentence => "sentence",
        }
    }

    pub fn parse(text: &str) -> Option<CaseMode> {
        CaseMode::ALL.into_iter().find(|mode| mode.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            CaseMode::Lower => "lower case",
            CaseMode::Upper => "UPPER CASE",
            CaseMode::Title => "Title Case",
            CaseMode::Sentence => "Sentence case",
        }
    }
}

fn lower(c: char, lang: Lang, out: &mut String) {
    match (c, lang) {
        ('I', Lang::Turkic) => out.push('ı'),
        ('İ', Lang::Turkic) => out.push('i'),
        // Outside Turkish, İ lowercases to i plus a combining dot; a plain i reads better.
        ('İ', Lang::Other) => out.push('i'),
        _ => out.extend(c.to_lowercase()),
    }
}

fn upper(c: char, lang: Lang, out: &mut String) {
    match (c, lang) {
        ('i', Lang::Turkic) => out.push('İ'),
        ('ı', Lang::Turkic) => out.push('I'),
        _ => out.extend(c.to_uppercase()),
    }
}

/// Where a new word starts: after a space, `_`, `-`, `.` or `(`.
fn starts_word(previous: Option<char>) -> bool {
    previous.is_none_or(|p| p.is_whitespace() || matches!(p, '_' | '-' | '.' | '(' | '['))
}

pub fn convert(text: &str, mode: CaseMode, lang: Lang) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous: Option<char> = None;
    let mut first_letter_done = false;
    for c in text.chars() {
        let capital = match mode {
            CaseMode::Lower => false,
            CaseMode::Upper => true,
            CaseMode::Title => starts_word(previous),
            CaseMode::Sentence => !first_letter_done && c.is_alphabetic(),
        };
        if capital {
            upper(c, lang, &mut out)
        } else {
            lower(c, lang, &mut out)
        }
        if c.is_alphabetic() {
            first_letter_done = true;
        }
        previous = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_dotted_and_dotless_i() {
        assert_eq!(convert("İSTANBUL ILIK", CaseMode::Lower, Lang::Turkic), "istanbul ılık");
        assert_eq!(convert("istanbul ılık", CaseMode::Upper, Lang::Turkic), "İSTANBUL ILIK");
        assert_eq!(convert("istanbul", CaseMode::Upper, Lang::Other), "ISTANBUL");
        assert_eq!(convert("İzmir", CaseMode::Lower, Lang::Other), "izmir");
    }

    #[test]
    fn title_and_sentence() {
        assert_eq!(convert("tatil_foto-ikinci gün", CaseMode::Title, Lang::Turkic), "Tatil_Foto-İkinci Gün");
        assert_eq!(convert("  BÜYÜK harf", CaseMode::Sentence, Lang::Turkic), "  Büyük harf");
        assert_eq!(convert("2024 yaz", CaseMode::Sentence, Lang::Other), "2024 Yaz");
    }

    #[test]
    fn language_codes() {
        assert_eq!(Lang::from_code("tr-TR"), Lang::Turkic);
        assert_eq!(Lang::from_code("az_AZ.UTF-8"), Lang::Turkic);
        assert_eq!(Lang::from_code("en-US"), Lang::Other);
        assert_eq!(Lang::from_code(""), Lang::Other);
    }
}
