//! The pseudo language (spec 15 §9): every text that goes through the catalogue comes out
//! framed, accented and longer: `⟦Ŝéţţıñĝš ····⟧`. A text without the frame was never
//! translated; a frame cut short is clipped; two frames in one sentence were put together from
//! pieces. Placeholders and line breaks stay as they are.

/// Letters for their accented look-alikes (`i` and `I` as Turkish `ı` and `İ`).
fn accent(c: char) -> char {
    const FROM: &str = "aAcCeEgGiInNoOsStTuUyYzZ";
    const TO: &str = "áÅçÇéÉĝĜıİñÑöÖšŜţŢüÜýÝžŽ";
    FROM.chars().position(|f| f == c).and_then(|i| TO.chars().nth(i)).unwrap_or(c)
}

pub fn pseudo(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2 + 8);
    out.push('⟦');
    let mut letters = 0;
    let mut in_placeholder = false;
    for c in text.chars() {
        match c {
            '{' => in_placeholder = true,
            '}' => in_placeholder = false,
            _ => {}
        }
        if in_placeholder || c == '}' {
            out.push(c);
        } else {
            letters += usize::from(c.is_alphanumeric());
            out.push(accent(c));
        }
    }
    // About 40 % longer, short texts more (German's worst case).
    let pad = (letters * 2 / 5).max(2);
    out.push(' ');
    out.extend(std::iter::repeat_n('·', pad));
    out.push('⟧');
    out
}

#[cfg(test)]
mod tests {
    use super::pseudo;

    #[test]
    fn frames_accents_and_keeps_placeholders() {
        assert_eq!(pseudo("Settings"), "⟦Ŝéţţıñĝš ···⟧");
        assert_eq!(pseudo("Open {} in {name}\nnow {{x}}"), "⟦Öpéñ {} ıñ {name}\nñöw {{x}} ···⟧");
        assert_eq!(pseudo(""), "⟦ ··⟧");
        let long = pseudo(&"word ".repeat(20));
        assert!(long.chars().count() >= 130, "{long}");
        assert!(pseudo("Fix it").contains('ı'), "Turkish dotless i");
    }
}
