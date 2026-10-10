//! The texts Gezik writes for the default file manager (spec 6, 10.6, 11.2): the registry
//! command, the restore .reg, the Linux .desktop and D-Bus .service files, and the
//! mimeapps.list editor. Pure and tested on every system; each has a check that reads back
//! only Gezik's own shape (decision 8 of 9b3: exact, not similar).

use gezik_core::system_change::{Change, Kind, RegType, Value};

/// The verb Gezik adds and makes the default (`…\shell\gezik`).
pub const VERB: &str = "gezik";
pub const VERB_TITLE: &str = "Open in Gezik";
/// What the shell puts for the item (decision 5; `%V` if the screen test says so).
pub const VERB_ARG: &str = "%1";
pub const BUNDLE_ID: &str = "com.wenlar.gezik";
/// A mimeapps.list Gezik made because there was none (removed again if still this).
pub const MIMEAPPS_EMPTY: &str = "[Default Applications]\n";

/// The verb's command (decision 1): Gezik itself, as Directory Opus and Total Commander do.
pub fn shell_command(exe: &str, arg: &str) -> String {
    format!(r#""{exe}" --shell "{arg}""#)
}

/// The exe in `data` if `data` is exactly [`shell_command`]'s text for `arg` and the exe is an
/// absolute `…\gezik.exe` (any case): nothing added, nothing missing, no other program.
/// Callers must also require the value to be REG_SZ (an EXPAND_SZ would expand `%…%`).
pub fn command_exe<'a>(data: &'a str, arg: &str) -> Option<&'a str> {
    quoted_gezik_exe(data, &format!(r#"" --shell "{arg}""#))
}

/// The exe in `data` if `data` is `"<exe>` and then exactly `tail`, and the exe is an
/// absolute `…\gezik.exe` (any case) with no quote or control character in it.
fn quoted_gezik_exe<'a>(data: &'a str, tail: &str) -> Option<&'a str> {
    let exe = data.strip_prefix('"')?.strip_suffix(tail)?;
    let bytes = exe.as_bytes();
    let absolute = exe.starts_with(r"\\")
        || (bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\');
    let named = exe.len().checked_sub(r"\gezik.exe".len()).and_then(|at| exe.get(at..))?;
    (absolute && named.eq_ignore_ascii_case(r"\gezik.exe") && !exe.chars().any(|c| c == '"' || c.is_control()))
        .then_some(exe)
}

/// The Run value's name (spec 9 §9.3) and the argument every login entry starts Gezik with.
pub const RUN_NAME: &str = "Gezik";
pub const BACKGROUND_ARG: &str = "--background";
/// `~/Library/LaunchAgents/<this>` (spec 9 §9.3; the label is [`BUNDLE_ID`]).
pub const LAUNCH_AGENT_FILE: &str = "com.wenlar.gezik.plist";

/// The Run value (spec 9 §9.3): Gezik without a window when the tray icon is on.
pub fn login_command(exe: &str) -> String {
    format!(r#""{exe}" {BACKGROUND_ARG}"#)
}

/// The exe of a [`login_command`] text, only if the text is exactly that and the exe is an
/// absolute `…\gezik.exe`. Callers also require REG_SZ (an EXPAND_SZ would expand `%…%`).
pub fn login_exe(data: &str) -> Option<&str> {
    quoted_gezik_exe(data, &format!("\" {BACKGROUND_ARG}"))
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// The inverse of [`xml_escape`] only (`&amp;` last); any other entity stays as it is and the
/// caller's exact comparison refuses the text.
fn xml_unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The LaunchAgent (spec 9 §9.3, decision 25): loaded at the next login, in the user's GUI
/// session only (`Aqua`); no KeepAlive, so Quit stays quit. `launchctl` is never run.
pub fn launch_agent(exe: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>Label</key>\n\t<string>{BUNDLE_ID}</string>\n\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{}</string>\n\t\t<string>{BACKGROUND_ARG}</string>\n\t</array>\n\t<key>RunAtLoad</key>\n\t<true/>\n\t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\t<key>ProcessType</key>\n\t<string>Interactive</string>\n</dict>\n</plist>\n",
        xml_escape(exe)
    )
}

/// The exe of a [`launch_agent`] text, only if the whole text is exactly that and the exe has
/// no control character or noncharacter (XML 1.0 cannot hold them). The caller checks the name.
pub fn launch_agent_exe(text: &str) -> Option<String> {
    const BEFORE: &str = "<key>ProgramArguments</key>\n\t<array>\n\t\t<string>";
    let start = text.find(BEFORE)? + BEFORE.len();
    let end = start + text.get(start..)?.find("</string>")?;
    let exe = xml_unescape(text.get(start..end)?);
    let xml_char = |c: char| !c.is_control() && !matches!(c, '\u{FFFE}' | '\u{FFFF}');
    (exe.chars().all(xml_char) && launch_agent(&exe) == text).then_some(exe)
}

/// `~/.config/autostart/gezik.desktop` (spec 9 §9.3): not shown in menus. The exe is always
/// quoted (reserved characters need it) and every `%` doubled, so no field code is left.
pub fn autostart_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Gezik\nComment=Starts Gezik at login\nExec={} {BACKGROUND_ARG}\nIcon=system-file-manager\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
        exec_quote(exe).replace('%', "%%")
    )
}

/// The exe of an [`autostart_entry`] text, only if the whole text is exactly that and the exe
/// has no control character (one would split the entry's lines). The caller checks the name.
pub fn autostart_exe(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let (exe, rest) = exec_unquote(line)?;
    let exe = exe.replace("%%", "%");
    (rest == format!(" {BACKGROUND_ARG}") && !exe.chars().any(char::is_control) && autostart_entry(&exe) == text)
        .then_some(exe)
}

/// A `shell` key's default: one verb name or a list of them (what Windows allows there).
pub fn verb_like(data: &str) -> bool {
    data.len() <= 64 && data.chars().all(|c| c.is_ascii_alphanumeric() || " ,._-".contains(c))
}

pub fn bundle_id_like(data: &str) -> bool {
    (1..=255).contains(&data.len())
        && data.contains('.')
        && data.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
}

/// A mimeapps.list value: `a.desktop;b.desktop;`.
pub fn desktop_ids(data: &str) -> bool {
    let mut ids = data.split(';').filter(|id| !id.is_empty()).peekable();
    ids.peek().is_some()
        && ids.all(|id| {
            id.strip_suffix(".desktop").is_some_and(|stem| {
                !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            })
        })
}

/// A .reg string; one with a line break or other control character as `hex(1)` so a
/// foreign value cannot add lines to the file.
fn reg_string(data: &str) -> String {
    if data.chars().any(char::is_control) {
        reg_hex(1, data)
    } else {
        format!("\"{}\"", data.replace('\\', r"\\").replace('"', "\\\""))
    }
}

/// `hex(n):` with the UTF-16LE text and its closing zero, as regedit exports it.
fn reg_hex(n: u8, data: &str) -> String {
    let bytes: Vec<String> =
        data.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).map(|b| format!("{b:02x}")).collect();
    format!("hex({n}):{}", bytes.join(","))
}

/// Text that cannot break a .reg line or section: no control characters, no `[` or `]`.
fn reg_safe(text: &str) -> bool {
    !text.chars().any(|c| c.is_control() || c == '[' || c == ']')
}

/// A key the restore file may name: under `HKCU\Software\Classes\` (where the default file
/// manager's changes are) and [`reg_safe`]. The caller also keeps only Gezik's allowed places.
fn restorable_key(place: &str) -> bool {
    const ROOT: &str = r"HKCU\Software\Classes\";
    place.get(..ROOT.len()).is_some_and(|root| root.eq_ignore_ascii_case(ROOT))
        && place.len() > ROOT.len()
        && reg_safe(place)
}

/// `restore-explorer.reg` (decision 12): the default file manager's changes undone, newest
/// first, as regedit imports them without Gezik. A change it cannot write safely (another
/// place, a broken name, a value type it does not know) becomes a comment with no journal text.
pub fn restore_reg(changes: &[Change]) -> String {
    const SKIPPED: &str = "; skipped a change this file cannot undo safely.\r\n";
    const WIN_E: &str = "; Win+E: the next lines take back Gezik's Win+E command; if another program answers Win+E now, delete them first.\r\n";
    let key = |place: &str| format!(r"HKEY_CURRENT_USER\{}", &place[r"HKCU\".len()..]);
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    for change in changes.iter().rev() {
        match (change.kind, &change.before) {
            (Kind::RegistryValue, before) if restorable_key(&change.place) && reg_safe(&change.name) => {
                let name = if change.name.is_empty() { "@".to_owned() } else { reg_string(&change.name) };
                let value = match before {
                    Value::Reg { ty: RegType::Sz, data } => reg_string(data),
                    Value::Reg { ty: RegType::ExpandSz, data } => reg_hex(2, data),
                    Value::Absent => "-".to_owned(),
                    // A DWORD or anything else of someone's: never deleted from here.
                    _ => {
                        out.push_str(SKIPPED);
                        continue;
                    }
                };
                if change.place.to_lowercase().ends_with(r"\opennewwindow\command") {
                    out.push_str(WIN_E);
                }
                out.push_str(&format!("[{}]\r\n{name}={value}\r\n\r\n", key(&change.place)));
            }
            (Kind::RegistryKey, Value::Absent) if restorable_key(&change.place) => {
                let place = change.place.to_lowercase();
                // `[-key]` deletes a whole tree: only Gezik's own verb key; its subkeys go with it.
                if place.ends_with(r"\shell\gezik") {
                    out.push_str(&format!("[-{}]\r\n\r\n", key(&change.place)));
                } else if !place.contains(r"\shell\gezik\") {
                    out.push_str(&format!(
                        "; Gezik also made {}; delete it by hand if it is empty.\r\n",
                        key(&change.place)
                    ));
                }
            }
            (Kind::File | Kind::Folder, Value::Absent) if reg_safe(&change.place) => {
                out.push_str(&format!("; Gezik also made {}; delete it by hand if you like.\r\n", change.place));
            }
            (Kind::RegistryValue | Kind::RegistryKey | Kind::File | Kind::Folder, _) => out.push_str(SKIPPED),
            _ => {}
        }
    }
    out
}

/// What regedit reads: UTF-16LE with a BOM.
pub fn utf16_file(text: &str) -> Vec<u8> {
    [0xFF, 0xFE].into_iter().chain(text.encode_utf16().flat_map(u16::to_le_bytes)).collect()
}

/// An argument quoted for a desktop entry's Exec (the spec's rule), then escaped again as
/// a desktop entry string (`\` doubled).
fn exec_quote(arg: &str) -> String {
    let mut out = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out.replace('\\', r"\\")
}

/// The inverse of [`exec_quote`]: one quoted argument at the start of `text`, and what follows it.
fn exec_unquote(text: &str) -> Option<(String, String)> {
    let text = text.replace(r"\\", "\\");
    let mut out = String::new();
    let mut chars = text.strip_prefix('"')?.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            '"' => return Some((out, chars.collect())),
            _ => out.push(c),
        }
    }
    None
}

/// `gezik.desktop` (spec 6.4; decision 17: a theme icon, none shipped).
pub fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Gezik\nComment=File manager\nExec={} %U\nIcon=system-file-manager\nTerminal=false\nCategories=System;FileTools;FileManager;\nMimeType=inode/directory;\nDBusActivatable=false\n",
        exec_quote(exe).replace('%', "%%")
    )
}

/// The exe of a [`desktop_entry`] text, only if the whole text is exactly that.
pub fn desktop_exe(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let (exe, rest) = exec_unquote(line)?;
    let exe = exe.replace("%%", "%");
    (rest == " %U" && desktop_entry(&exe) == text).then_some(exe)
}

/// The FileManager1 activation file (spec 6.4): the bus starts `gezik --dbus`.
pub fn dbus_service(exe: &str) -> String {
    format!("[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec={} --dbus\n", exec_quote(exe))
}

/// The exe of a [`dbus_service`] text, only if the whole text is exactly that.
pub fn service_exe(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let (exe, rest) = exec_unquote(line)?;
    (rest == " --dbus" && dbus_service(&exe) == text).then_some(exe)
}

fn section_of(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.strip_suffix(']')
}

fn key_of(line: &str) -> Option<&str> {
    let (key, _) = line.split_once('=')?;
    Some(key.trim())
}

/// `key`'s value in `[Default Applications]`.
pub fn mimeapps_get(text: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        if let Some(section) = section_of(line) {
            inside = section == "Default Applications";
        } else if inside && key_of(line) == Some(key) {
            return line.split_once('=').map(|(_, v)| v.trim().to_owned());
        }
    }
    None
}

/// `text` with `key` in `[Default Applications]` set to `value` (or taken out); every other
/// line, comment and section as it was (spec 6.4).
pub fn mimeapps_set(text: &str, key: &str, value: Option<&str>) -> String {
    let new_line = value.map(|v| format!("{key}={v}"));
    let mut lines: Vec<String> = Vec::new();
    let (mut inside, mut done, mut seen) = (false, false, false);
    for line in text.lines() {
        if let Some(section) = section_of(line) {
            if inside
                && !done
                && let Some(l) = &new_line
            {
                // Added at the end of the section, before its trailing blank lines.
                let at = lines.iter().rposition(|l| !l.trim().is_empty()).map_or(0, |i| i + 1);
                lines.insert(at, l.clone());
                done = true;
            }
            inside = section == "Default Applications";
            seen |= inside;
        } else if inside && key_of(line) == Some(key) {
            if !done && let Some(l) = &new_line {
                lines.push(l.clone());
            }
            done = true;
            continue;
        }
        lines.push(line.to_owned());
    }
    if !done && let Some(l) = new_line {
        if !seen {
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(String::new());
            }
            lines.push("[Default Applications]".to_owned());
        }
        lines.push(l);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') || text.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::system_change::{Change, Kind, RegType, Value};

    #[test]
    fn shell_command_round_trips_unicode_and_spaces() {
        let exe = r"D:\Araçlar\Gezik Dev\gezik.exe";
        let line = shell_command(exe, VERB_ARG);
        assert_eq!(line, format!(r#""{exe}" --shell "%1""#));
        assert_eq!(command_exe(&line, VERB_ARG), Some(exe));
        assert_eq!(command_exe(&shell_command(exe, ""), ""), Some(exe));
        // Another argument, an injected one, a quote in the exe, no quotes: not Gezik's.
        assert_eq!(command_exe(&line, ""), None);
        assert_eq!(command_exe(&format!(r#""{exe}" --shell "%1" --evil"#), VERB_ARG), None);
        assert_eq!(command_exe(r#""a" "b" --shell "%1""#, VERB_ARG), None);
        assert_eq!(command_exe(&format!(r#"{exe} --shell "%1""#), VERB_ARG), None);
        assert_eq!(command_exe(r#""" --shell "%1""#, VERB_ARG), None);
    }

    #[test]
    fn command_exe_takes_only_an_absolute_gezik_exe_in_the_exact_form() {
        let ok = |exe: &str| assert_eq!(command_exe(&shell_command(exe, VERB_ARG), VERB_ARG), Some(exe), "{exe}");
        ok(r"C:\Program Files\Gezik\gezik.exe");
        ok(r"C:\Users\Ömer Şen\AppData\Local\日本\GEZIK.EXE");
        ok(r"\\server\share\Gezik.Exe");
        let no = |data: &str| assert_eq!(command_exe(data, VERB_ARG), None, "{data}");
        // Arguments injected before, after or instead of `--shell`.
        no(r#""C:\G\gezik.exe" --evil --shell "%1""#);
        no(r#""C:\G\gezik.exe" --shell "%1" "%2""#);
        no(r#""C:\G\gezik.exe" --shell "%1" "#);
        no(r#""C:\G\gezik.exe"  --shell "%1""#);
        no(r#""C:\G\gezik.exe" --SHELL "%1""#);
        no(r#""C:\G\gezik.exe" --shell "%V""#);
        no(r#""C:\G\gezik.exe" --shell %1"#);
        no(r#" "C:\G\gezik.exe" --shell "%1""#);
        // Missing or broken quotes.
        no(r#"C:\G\gezik.exe --shell "%1""#);
        no(r#""C:\G\gezik.exe --shell "%1""#);
        no(r#"C:\G\gezik.exe" --shell "%1""#);
        // Another program, a relative path, a line break.
        no(r#""C:\G\notgezik.exe" --shell "%1""#);
        no(r#""C:\G\gezik.exe.cmd" --shell "%1""#);
        no(r#""C:\Windows\System32\cmd.exe" --shell "%1""#);
        no(r#""gezik.exe" --shell "%1""#);
        no(r#""C:gezik.exe" --shell "%1""#);
        no(r#""G\gezik.exe" --shell "%1""#);
        no("\"C:\\G\nx\\gezik.exe\" --shell \"%1\"");
    }

    #[test]
    fn verb_names_bundle_ids_and_desktop_ids() {
        for ok in ["gezik", "none", "", "open", "openinxyplorer", "explore,open"] {
            assert!(verb_like(ok), "{ok}");
        }
        for bad in [r"C:\x.exe", "\"x\"", "a%1", &"x".repeat(65)] {
            assert!(!verb_like(bad), "{bad}");
        }
        assert!(bundle_id_like("com.wenlar.gezik") && bundle_id_like("com.apple.finder"));
        assert!(!bundle_id_like("finder") && !bundle_id_like("/Applications/X.app") && !bundle_id_like(""));
        assert!(desktop_ids("gezik.desktop;") && desktop_ids("org.gnome.Nautilus.desktop;nemo.desktop"));
        assert!(
            !desktop_ids("") && !desktop_ids("/usr/bin/x;") && !desktop_ids("x.sh;") && !desktop_ids("a b.desktop")
        );
    }

    fn change(kind: Kind, place: &str, name: &str, before: Value, after: Value) -> Change {
        Change {
            feature: "default-file-manager".into(),
            kind,
            place: place.into(),
            name: name.into(),
            entry: String::new(),
            before,
            after,
            done: true,
        }
    }

    #[test]
    fn restore_reg_undoes_newest_first() {
        let sz = |d: &str| Value::Reg { ty: RegType::Sz, data: d.into() };
        let changes = [
            change(Kind::File, r"C:\L\Gezik\x.txt", "", Value::Absent, Value::Text("x".into())),
            change(
                Kind::RegistryKey,
                r"HKCU\Software\Classes\Directory\shell\gezik",
                "",
                Value::Absent,
                Value::Present,
            ),
            change(
                Kind::RegistryValue,
                r"HKCU\Software\Classes\CLSID\{x}\shell\opennewwindow\command",
                "DelegateExecute",
                Value::Absent,
                sz(""),
            ),
            change(Kind::RegistryValue, r"HKCU\Software\Classes\Directory\shell", "", sz(r#"o"p\n"#), sz("gezik")),
            change(
                Kind::RegistryValue,
                r"HKCU\Software\Classes\Drive\shell",
                "",
                Value::Reg { ty: RegType::ExpandSz, data: "a".into() },
                sz("gezik"),
            ),
        ];
        let text = restore_reg(&changes);
        let expected = "Windows Registry Editor Version 5.00\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\Classes\\Drive\\shell]\r\n@=hex(2):61,00,00,00\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell]\r\n@=\"o\\\"p\\\\n\"\r\n\r\n\
            ; Win+E: the next lines take back Gezik's Win+E command; if another program answers Win+E now, delete them first.\r\n\
            [HKEY_CURRENT_USER\\Software\\Classes\\CLSID\\{x}\\shell\\opennewwindow\\command]\r\n\"DelegateExecute\"=-\r\n\r\n\
            [-HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell\\gezik]\r\n\r\n\
            ; Gezik also made C:\\L\\Gezik\\x.txt; delete it by hand if you like.\r\n";
        assert_eq!(text, expected);
        let bytes = utf16_file(&text);
        assert_eq!(&bytes[..2], &[0xFF, 0xFE], "UTF-16LE with a BOM, as regedit writes it");
    }

    #[test]
    fn restore_reg_keeps_a_foreign_line_break_inside_its_value() {
        let before = Value::Reg { ty: RegType::Sz, data: "x\r\n[HKEY_CURRENT_USER\\Evil]".into() };
        let place = r"HKCU\Software\Classes\Directory\shell";
        let text = restore_reg(&[change(Kind::RegistryValue, place, "", before, Value::Absent)]);
        assert!(!text.contains("[HKEY_CURRENT_USER\\Evil]"), "{text}");
        assert!(text.contains("@=hex(1):78,00,0d,00,0a,00,"), "{text}");
    }

    #[test]
    fn restore_reg_writes_nothing_from_a_tampered_journal() {
        let sz = |d: &str| Value::Reg { ty: RegType::Sz, data: d.into() };
        let classes = r"HKCU\Software\Classes\Directory\shell";
        let evil = "x]\r\n[HKEY_LOCAL_MACHINE\\Evil]";
        let changes = [
            change(Kind::RegistryValue, r"HKLM\Software\Classes\Directory\shell", "", sz("x"), sz("gezik")),
            change(Kind::RegistryKey, r"HKLM\Software\Evil", "", Value::Absent, Value::Present),
            change(Kind::RegistryKey, r"HKCU\Environment", "", Value::Absent, Value::Present),
            change(Kind::RegistryValue, r"HKCU\Software\Classes\", "", sz("x"), sz("gezik")),
            change(Kind::RegistryValue, &format!(r"HKCU\Software\Classes\{evil}"), "", sz("x"), sz("gezik")),
            change(Kind::RegistryKey, &format!(r"HKCU\Software\Classes\{evil}"), "", Value::Absent, Value::Present),
            change(Kind::RegistryValue, classes, evil, Value::Absent, sz("gezik")),
            change(Kind::RegistryValue, classes, "a\tb", Value::Absent, sz("gezik")),
            change(Kind::RegistryValue, classes, "", Value::Other, sz("gezik")),
            change(Kind::File, &format!(r"C:\{evil}"), "", Value::Absent, Value::Text("x".into())),
        ];
        let text = restore_reg(&changes);
        let skipped = "; skipped a change this file cannot undo safely.\r\n";
        assert_eq!(text, format!("Windows Registry Editor Version 5.00\r\n\r\n{}", skipped.repeat(changes.len())));
    }

    #[test]
    fn desktop_exec_doubles_a_percent_sign() {
        let exe = "/home/u/100% sure/gezik";
        let entry = desktop_entry(exe);
        assert!(entry.contains("Exec=\"/home/u/100%% sure/gezik\" %U\n"), "{entry}");
        assert_eq!(desktop_exe(&entry).as_deref(), Some(exe));
        assert_eq!(desktop_exe(&entry.replace("100%%", "100%")), None);
    }

    #[test]
    fn desktop_and_service_texts_quote_the_exe() {
        let exe = r#"/home/u/my apps/"q"$x`y\z/gezik"#;
        let entry = desktop_entry(exe);
        assert!(entry.contains(r#"Exec="/home/u/my apps/\\"q\\"\\$x\\`y\\\\z/gezik" %U"#), "{entry}");
        assert!(entry.contains("MimeType=inode/directory;\n"));
        assert_eq!(desktop_exe(&entry).as_deref(), Some(exe));
        let service = dbus_service(exe);
        assert!(service.starts_with("[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec="));
        assert_eq!(service_exe(&service).as_deref(), Some(exe));
        assert_eq!(desktop_exe(&entry.replace("%U", "%U --evil")), None);
        assert_eq!(service_exe("[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec=/bin/sh -c x\n"), None);
    }

    #[test]
    fn login_texts_read_back_only_exactly() {
        let exe = r"D:\Araçlar\Gezik Dev\gezik.exe";
        let command = login_command(exe);
        assert_eq!(command, format!(r#""{exe}" --background"#));
        assert_eq!(login_exe(&command), Some(exe));
        for bad in [
            format!(r#""{exe}" --background --evil"#),
            format!(r#""{exe}"  --background"#),
            format!(r#""{exe}" --background "#),
            format!(r#""{exe}" --BACKGROUND"#),
            format!(r#"{exe} --background"#),
            format!(r#""{exe}" --shell "%1""#),
            r#""C:\G\evil.exe" --background"#.to_owned(),
            r#""gezik.exe" --background"#.to_owned(),
            r#""C:\G\a" "b\gezik.exe" --background"#.to_owned(),
            "\"C:\\G\nx\\gezik.exe\" --background".to_owned(),
        ] {
            assert_eq!(login_exe(&bad), None, "{bad}");
        }
        // The folder verb's command is not a login command, nor the other way round.
        assert_eq!(command_exe(&command, VERB_ARG), None);
        assert_eq!(login_exe(&shell_command(exe, VERB_ARG)), None);

        let unix = r#"/Users/ü/My Apps/"q"&<x>'y'$z/gezik"#;
        let plist = launch_agent(unix);
        assert!(plist.contains("<string>com.wenlar.gezik</string>"));
        assert!(plist.contains("&quot;q&quot;&amp;&lt;x&gt;&apos;y&apos;$z/gezik</string>"), "{plist}");
        assert!(plist.contains("<string>--background</string>") && plist.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(plist.contains("<string>Aqua</string>") && !plist.contains("KeepAlive"));
        assert_eq!(launch_agent_exe(&plist).as_deref(), Some(unix));
        assert_eq!(launch_agent_exe(&plist.replace("</dict>", "\t<key>KeepAlive</key>\n\t<true/>\n</dict>")), None);
        assert_eq!(launch_agent_exe(&plist.replace("--background", "--evil")), None);
        assert_eq!(
            launch_agent_exe(&launch_agent("/bin/sh")).as_deref(),
            Some("/bin/sh"),
            "the caller checks the name"
        );
        assert_eq!(launch_agent_exe("<plist/>"), None);

        let entry = autostart_entry("/home/u/100% sure/gezik");
        assert!(entry.contains("Exec=\"/home/u/100%% sure/gezik\" --background\n"), "{entry}");
        assert!(entry.contains("NoDisplay=true\n") && entry.starts_with("[Desktop Entry]\nType=Application\n"));
        assert_eq!(autostart_exe(&entry).as_deref(), Some("/home/u/100% sure/gezik"));
        assert_eq!(autostart_exe(&entry.replace("--background", "--background --evil")), None);
        assert_eq!(autostart_exe(&desktop_entry("/home/u/gezik")), None, "the default's entry is not the login one");
        assert_eq!(desktop_exe(&autostart_entry("/home/u/gezik")), None);
    }

    #[test]
    fn a_control_character_never_makes_a_login_text() {
        assert_eq!(launch_agent_exe(&launch_agent("/a\u{1}b/gezik")), None, "XML 1.0 cannot hold it");
        assert_eq!(launch_agent_exe(&launch_agent("/a\tb/gezik")), None);
        assert_eq!(launch_agent_exe(&launch_agent("/a\u{FFFE}b/gezik")), None, "not an XML character either");
        assert_eq!(autostart_exe(&autostart_entry("/a\nExec=/bin/sh\n/gezik")), None);
        assert_eq!(autostart_exe(&autostart_entry("/a\rb/gezik")), None);
        assert_eq!(autostart_exe(&autostart_entry("/a\u{85}b/gezik")), None);
    }

    #[test]
    fn hostile_launch_agents_are_not_gezik_s() {
        let plist = launch_agent("/Applications/Gezik.app/Contents/MacOS/gezik");
        for bad in [
            String::new(),
            plist.replace('\n', "\r\n"),
            format!("\u{FEFF}{plist}"),
            format!("{plist}\n"),
            format!("{plist}<!-- -->"),
            // An entity or a CDATA section that means the same exe to a plist reader.
            plist.replace("/gezik</string>", "/&#103;ezik</string>"),
            plist
                .replace("<string>/Applications", "<string><![CDATA[/Applications")
                .replace("gezik</string>\n\t\t<string>--", "gezik]]></string>\n\t\t<string>--"),
            plist.replace("<string>--background</string>", "<string>--background</string>\n\t\t<string>-c</string>"),
            plist.replace("<string>Aqua</string>", "<string>Background</string>"),
            plist.replace(
                "<key>ProgramArguments</key>",
                "<key>Program</key>\n\t<string>/bin/sh</string>\n\t<key>ProgramArguments</key>",
            ),
            // Two ProgramArguments: the reader must not take the first and ignore the second.
            plist.replace(
                "</dict>",
                "\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>/bin/sh</string>\n\t</array>\n</dict>",
            ),
            "<key>ProgramArguments</key>\n\t<array>\n\t\t<string>".to_owned(),
            "<key>ProgramArguments</key>\n\t<array>\n\t\t<string>&amp;".to_owned(),
        ] {
            assert_eq!(launch_agent_exe(&bad), None, "{bad}");
        }
    }

    #[test]
    fn hostile_autostart_entries_are_not_gezik_s() {
        let entry = autostart_entry("/opt/gezik/gezik");
        for bad in [
            String::new(),
            "Exec=".to_owned(),
            "Exec=\"".to_owned(),
            "Exec=\"\\".to_owned(),
            "Exec=\"\\\\".to_owned(),
            entry.replace('\n', "\r\n"),
            format!("{entry}Exec=/bin/sh\n"),
            format!("Exec=/bin/sh\n{entry}"),
            entry.replace("NoDisplay=true", "NoDisplay=false"),
            entry.replace("Type=Application", "Type=Application\nTryExec=/bin/sh"),
            // Field codes and unquoted or single-quoted programs.
            entry.replace(" --background", " --background %U"),
            entry.replace(" --background", " %f --background"),
            entry.replace("Exec=\"/opt/gezik/gezik\"", "Exec=/opt/gezik/gezik"),
            entry.replace("Exec=\"/opt/gezik/gezik\"", "Exec='/opt/gezik/gezik'"),
            entry.replace("/opt/gezik/gezik", "/opt/%U/gezik"),
            entry.replace("Exec=\"/opt/gezik/gezik\"", "Exec=\"/bin/sh\" -c \"/opt/gezik/gezik\""),
            entry.replace("Exec=", "Exec[tr]=\"/bin/sh\" --background\nExec="),
        ] {
            assert_eq!(autostart_exe(&bad), None, "{bad}");
        }
        // Reserved characters and escapes survive the round trip.
        for exe in [
            r#"/h/a "b"/gezik"#,
            r"/h/a\b/gezik",
            "/h/$HOME`id`/gezik",
            "/h/%%u %U/gezik",
            "/h/a;b|c&d<e>f~g*h?i#j(k)l'm/gezik",
        ] {
            let entry = autostart_entry(exe);
            assert_eq!(autostart_exe(&entry).as_deref(), Some(exe), "{entry}");
            let exec = entry.lines().find_map(|l| l.strip_prefix("Exec=")).unwrap();
            // Every `%` is doubled, so no field code is left for the launcher to expand.
            assert!(exec.replace("%%", "").find('%').is_none(), "{exec}");
        }
    }

    #[test]
    fn login_readers_take_no_cut_text() {
        let texts = [
            login_command(r"C:\G\gezik.exe"),
            launch_agent("/Applications/Gezik.app/Contents/MacOS/gezik"),
            autostart_entry("/opt/gezik/gezik"),
        ];
        for text in &texts {
            for (at, _) in text.char_indices().skip(1) {
                let cut = &text[..at];
                assert_eq!(login_exe(cut), None, "{cut}");
                assert_eq!(launch_agent_exe(cut), None, "{cut}");
                assert_eq!(autostart_exe(cut), None, "{cut}");
            }
        }
    }

    #[test]
    fn mimeapps_edits_keep_everything_else() {
        let text = "# mine\n[Added Associations]\ninode/directory=a.desktop;\n\n[Default Applications]\ntext/plain=gedit.desktop;\ninode/directory = org.gnome.Nautilus.desktop;\n";
        assert_eq!(mimeapps_get(text, "inode/directory").as_deref(), Some("org.gnome.Nautilus.desktop;"));
        let set = mimeapps_set(text, "inode/directory", Some("gezik.desktop;"));
        assert_eq!(
            set,
            text.replace("inode/directory = org.gnome.Nautilus.desktop;", "inode/directory=gezik.desktop;")
        );
        assert_eq!(
            mimeapps_set(&set, "inode/directory", Some("org.gnome.Nautilus.desktop;"))
                .replace("inode/directory=org", "inode/directory = org"),
            text
        );
        let gone = mimeapps_set(text, "inode/directory", None);
        assert!(gone.contains("[Added Associations]\ninode/directory=a.desktop;"), "only [Default Applications]");
        assert_eq!(mimeapps_get(&gone, "inode/directory"), None);
        // No section: added at the end; no file text at all: Gezik's own.
        assert_eq!(
            mimeapps_set("[Added Associations]\n", "inode/directory", Some("gezik.desktop;")),
            "[Added Associations]\n\n[Default Applications]\ninode/directory=gezik.desktop;\n"
        );
        assert_eq!(
            mimeapps_set(MIMEAPPS_EMPTY, "inode/directory", Some("gezik.desktop;")),
            "[Default Applications]\ninode/directory=gezik.desktop;\n"
        );
        assert_eq!(
            mimeapps_set("[Default Applications]\ninode/directory=gezik.desktop;\n", "inode/directory", None),
            MIMEAPPS_EMPTY
        );
        // No trailing newline kept as it was.
        assert_eq!(
            mimeapps_set("[Default Applications]\nx/y=a.desktop;", "inode/directory", Some("g.desktop;")),
            "[Default Applications]\nx/y=a.desktop;\ninode/directory=g.desktop;"
        );
    }
}
