//! Quoting for the administrator helper's command line (spec 9 §10.3): Windows' rules (what
//! `CommandLineToArgvW` and the C runtime read back), `sh` single quotes and AppleScript string
//! literals (macOS `do shell script`). Pure; the round trips are tested against real readers.

/// One argument as Windows reads it back: quoted only when it must be, a quote as `\"`, the
/// backslashes before a quote (or before the closing quote) doubled.
pub fn windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut slashes = 0usize;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n('\\', slashes));
        }
        out.push(c);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}

/// The arguments after the program, as one line (`ShellExecuteExW`'s `lpParameters`).
pub fn windows_command_line(args: &[String]) -> String {
    args.iter().map(|arg| windows_arg(arg)).collect::<Vec<_>>().join(" ")
}

/// One `sh` word: in single quotes, a quote inside as `'\''`. Nothing in it is expanded.
pub fn sh_word(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// An AppleScript string literal: `\` and `"` escaped. Control characters never get here
/// (`check` refuses them in every name; `elevate::run` refuses them in the exe's path).
pub fn applescript(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', r"\\").replace('"', "\\\""))
}

/// The AppleScript that runs `exe` with `args` as root behind the system's password prompt:
/// only the exe and its arguments, each a quoted `sh` word (spec §10.5's one exception).
/// `without altering line endings`: the replies come back with `\n`, not `\r`.
pub fn osascript_source(exe: &str, args: &[String], prompt: &str) -> String {
    let command: Vec<String> = std::iter::once(exe).chain(args.iter().map(String::as_str)).map(sh_word).collect();
    format!(
        "do shell script {} with prompt {} with administrator privileges without altering line endings",
        applescript(&command.join(" ")),
        applescript(prompt)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strings from every character that means something to some layer, by a fixed generator.
    fn tricky(count: usize) -> Vec<String> {
        const PARTS: [&str; 26] = [
            "a", "Z", " ", "\t", "\"", "\\", "'", "$", "`", "(", ")", ";", "&", "|", "*", "?", "%", "!", "ğ", "€",
            "😀", "~", "#", "\u{a0}", "\\\\", "$(id)",
        ];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let len = (state % 12) as usize;
                (0..len).map(|i| PARTS[((state >> (i * 5)) % PARTS.len() as u64) as usize]).collect::<String>()
            })
            .collect()
    }

    /// The C runtime's (and CommandLineToArgvW's) reading of the arguments after the program.
    fn split_windows(line: &str) -> Vec<String> {
        let (mut args, mut current) = (Vec::new(), String::new());
        let (mut quoted, mut started, mut slashes) = (false, false, 0usize);
        for c in line.chars() {
            if c == '\\' {
                slashes += 1;
                started = true;
                continue;
            }
            if c == '"' {
                current.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 == 1 {
                    current.push('"');
                } else {
                    quoted = !quoted;
                }
                slashes = 0;
                started = true;
                continue;
            }
            current.extend(std::iter::repeat_n('\\', slashes));
            slashes = 0;
            if (c == ' ' || c == '\t') && !quoted {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            } else {
                current.push(c);
                started = true;
            }
        }
        current.extend(std::iter::repeat_n('\\', slashes));
        if started {
            args.push(current);
        }
        args
    }

    /// `sh`'s reading of words made by `sh_word` (single quotes and `\'` only).
    fn unsh(line: &str) -> Vec<String> {
        let (mut words, mut current) = (Vec::new(), String::new());
        let (mut quoted, mut escaped, mut started) = (false, false, false);
        for c in line.chars() {
            match c {
                _ if escaped => {
                    current.push(c);
                    escaped = false;
                }
                '\'' => {
                    quoted = !quoted;
                    started = true;
                }
                '\\' if !quoted => escaped = true,
                ' ' if !quoted => {
                    if started {
                        words.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                _ => current.push(c),
            }
        }
        if started {
            words.push(current);
        }
        words
    }

    fn unapplescript(literal: &str) -> String {
        let inner = literal.strip_prefix('"').and_then(|l| l.strip_suffix('"')).unwrap();
        let (mut out, mut escaped) = (String::new(), false);
        for c in inner.chars() {
            if escaped || c != '\\' {
                out.push(c);
                escaped = false;
            } else {
                escaped = true;
            }
        }
        out
    }

    #[test]
    fn windows_args_read_back() {
        assert_eq!(windows_arg("plain"), "plain");
        assert_eq!(windows_arg(""), "\"\"");
        assert_eq!(windows_arg(r"C:\Program Files\x"), r#""C:\Program Files\x""#);
        assert_eq!(windows_arg(r"C:\a b\"), r#""C:\a b\\""#, "a trailing backslash is doubled before the quote");
        assert_eq!(windows_arg(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(windows_arg(r#"a\"b"#), r#""a\\\"b""#);
        let mut args = tricky(500);
        args.push(String::new());
        args.push("\\".repeat(5));
        assert_eq!(split_windows(&windows_command_line(&args)), args);
    }

    #[test]
    fn sh_and_applescript_read_back() {
        assert_eq!(sh_word("it's"), r"'it'\''s'");
        assert_eq!(applescript(r#"a "b" \c"#), r#""a \"b\" \\c""#);
        let args = tricky(500);
        let line: Vec<String> = args.iter().map(|a| sh_word(a)).collect();
        assert_eq!(unsh(&line.join(" ")), args);
        for text in args.iter().filter(|t| !t.is_empty()) {
            assert_eq!(&unapplescript(&applescript(text)), text);
        }
        let source = osascript_source("/Apps/gé zik", &["--elevated".into(), "it's".into()], "Gezik: Copy 1 item.");
        assert_eq!(
            source,
            r#"do shell script "'/Apps/gé zik' '--elevated' 'it'\\''s'" with prompt "Gezik: Copy 1 item." with administrator privileges without altering line endings"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn sh_words_survive_a_real_sh() {
        let args = tricky(200);
        let words: Vec<String> = args.iter().map(|a| sh_word(a)).collect();
        let script = format!("printf '%s\\0' {}", words.join(" "));
        let out = std::process::Command::new("/bin/sh").arg("-c").arg(&script).output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let back: Vec<String> = text.split_terminator('\0').map(str::to_owned).collect();
        assert_eq!(back, args);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_survives_a_real_osascript() {
        // `return` only: no privileges are asked for.
        for text in tricky(60).into_iter().filter(|t| !t.is_empty() && !t.contains('\t')) {
            let out = std::process::Command::new("/usr/bin/osascript")
                .arg("-e")
                .arg(format!("return {}", applescript(&text)))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8(out.stdout).unwrap().strip_suffix('\n'), Some(text.as_str()));
        }
    }
}
