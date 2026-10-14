//! Terminal escape sequences taken out of a program's output line (spec 16 §7.3): the
//! command panel is no terminal, so colors, cursor moves and window titles are dropped.

/// `line` without its escape sequences (CSI `ESC [ … final`, OSC/DCS/APC/PM/SOS strings up
/// to BEL or `ESC \`, two-byte escapes, an escape cut off at the end) and control characters
/// other than tab.
pub fn strip(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => skip_csi(&mut chars),
                Some(']' | 'P' | 'X' | '^' | '_') => skip_string(&mut chars),
                // Intermediates (` `–`/`), then one final character.
                Some(' '..='/') => {
                    while chars.next_if(|c| matches!(c, ' '..='/')).is_some() {}
                    chars.next();
                }
                _ => {}
            },
            '\u{9b}' => skip_csi(&mut chars),
            '\u{9d}' => skip_string(&mut chars),
            '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Parameters and intermediates up to the final character (`@`–`~`).
fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for c in chars.by_ref() {
        if matches!(c, '@'..='~') {
            break;
        }
    }
}

/// Up to BEL or ST (`ESC \` or U+009C).
fn skip_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(c) = chars.next() {
        match c {
            '\u{7}' | '\u{9c}' => break,
            '\u{1b}' => {
                chars.next_if_eq(&'\\');
                break;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::strip;

    #[test]
    fn colors_and_cursor_moves_go() {
        assert_eq!(strip("\x1b[1;31merror\x1b[0m: x"), "error: x");
        assert_eq!(strip("a\x1b[2K\x1b[1Gb"), "ab");
        assert_eq!(strip("\x1b[?25lhidden cursor\x1b[?25h"), "hidden cursor");
        assert_eq!(strip("\u{9b}32mgreen"), "green");
    }

    #[test]
    fn titles_and_links_go() {
        assert_eq!(strip("\x1b]0;title\x07text"), "text");
        assert_eq!(strip("\x1b]8;;https://x.org\x1b\\link\x1b]8;;\x1b\\"), "link");
        assert_eq!(strip("\x1bPq#0;2;0;0;0\x1b\\after"), "after");
    }

    #[test]
    fn short_and_cut_off_escapes_go() {
        assert_eq!(strip("\x1b(Bplain\x1b="), "plain");
        assert_eq!(strip("end\x1b"), "end");
        assert_eq!(strip("end\x1b[31"), "end");
        assert_eq!(strip("\x1b]0;never ended"), "");
    }

    #[test]
    fn controls_but_tab_go_and_text_stays() {
        assert_eq!(strip("a\tb\x07c\x08d"), "a\tbcd");
        assert_eq!(strip("Çalışma klasörü ğüşiöç 漢字 [1] ~"), "Çalışma klasörü ğüşiöç 漢字 [1] ~");
    }
}
