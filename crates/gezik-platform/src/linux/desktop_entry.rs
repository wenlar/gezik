//! Desktop entries (`.desktop` files, Desktop Entry spec 1.5) for Open With on Linux (spec 9
//! §8.5): the keys Gezik needs, and the Exec line turned into argv with no shell. Another
//! program's text: read with limits, never run through a shell; a file name never becomes a
//! field code or a word of its own. Compiled everywhere, so it is tested on Windows.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Desktop files bigger than this are not read (a real one is a few KB).
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024;
pub const BAD_EXEC: &str = "The app's command line cannot be used";

/// The keys of an application's `[Desktop Entry]` that Open With needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub try_exec: Option<String>,
    pub icon: Option<String>,
    pub mime_types: Vec<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
}

/// The first `[Desktop Entry]` group of `text` (later groups, like `[Desktop Action …]`, are
/// not read), the first value of each key; `None` unless it is an application with a name
/// and a command, or hidden (a hidden entry hides its id in lower folders).
pub fn parse(text: &str) -> Option<DesktopEntry> {
    let mut e = DesktopEntry::default();
    let (mut inside, mut app) = (false, false);
    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if inside {
                break;
            }
            inside = group == "Desktop Entry";
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        if !inside || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        match key {
            "Type" => app = value == "Application",
            "Name" => e.name = unescape(value),
            "Exec" => e.exec = unescape(value),
            "TryExec" => e.try_exec = Some(unescape(value)).filter(|t| !t.is_empty()),
            "Icon" => e.icon = Some(unescape(value)).filter(|i| !i.is_empty()),
            "MimeType" => e.mime_types = split_list(value),
            "NoDisplay" => e.no_display = value == "true",
            "Hidden" => e.hidden = value == "true",
            "Terminal" => e.terminal = value == "true",
            "OnlyShowIn" => e.only_show_in = split_list(value),
            "NotShowIn" => e.not_show_in = split_list(value),
            _ => {}
        }
    }
    (e.hidden || (app && !e.name.is_empty() && !e.exec.trim().is_empty())).then_some(e)
}

/// A desktop entry string: `\s \n \t \r \\`; any other escape is kept for its own rule
/// (Exec's quoting, a list's `\;`).
pub fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// A `;`-separated list (`\;` is a `;` inside an item); empty items are dropped.
pub fn split_list(value: &str) -> Vec<String> {
    let (mut items, mut current) = (Vec::new(), String::new());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => current.push(';'),
                Some(next) => {
                    current.push('\\');
                    current.push(next);
                }
                None => current.push('\\'),
            },
            ';' => {
                if !current.is_empty() {
                    items.push(unescape(&current));
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        items.push(unescape(&current));
    }
    items
}

/// Whether `OnlyShowIn`/`NotShowIn` let the entry show on `desktops` (`$XDG_CURRENT_DESKTOP`'s
/// parts, lower case).
pub fn shown_on(entry: &DesktopEntry, desktops: &[String]) -> bool {
    let on = |list: &[String]| list.iter().any(|d| desktops.iter().any(|c| c.eq_ignore_ascii_case(d)));
    (entry.only_show_in.is_empty() || on(&entry.only_show_in)) && !on(&entry.not_show_in)
}

/// One piece of an Exec word: a character, or a field code (outside quotes only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    Char(char),
    Code(char),
}

/// The Exec line's words (the string escapes already undone by `parse`): split at blanks,
/// `"…"` quoted with `\"`, `` \` ``, `\$`, `\\` inside; `%x` outside quotes is a field code,
/// `%%` anywhere a `%`; a field code inside quotes is refused (decision 11).
fn words(exec: &str) -> Result<Vec<Vec<Piece>>, &'static str> {
    let mut out = Vec::new();
    let mut chars = exec.chars().peekable();
    loop {
        while chars.next_if(|c| matches!(c, ' ' | '\t' | '\n')).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut word = Vec::new();
        while let Some(c) = chars.next_if(|c| !matches!(c, ' ' | '\t' | '\n')) {
            match c {
                '"' => loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(e @ ('"' | '`' | '$' | '\\')) => word.push(Piece::Char(e)),
                            _ => return Err(BAD_EXEC),
                        },
                        Some('%') => match chars.next() {
                            Some('%') => word.push(Piece::Char('%')),
                            _ => return Err(BAD_EXEC),
                        },
                        Some(c) => word.push(Piece::Char(c)),
                        None => return Err(BAD_EXEC),
                    }
                },
                '%' => match chars.next() {
                    Some('%') => word.push(Piece::Char('%')),
                    Some(code) => word.push(Piece::Code(code)),
                    None => return Err(BAD_EXEC),
                },
                c => word.push(Piece::Char(c)),
            }
        }
        out.push(word);
    }
    if out.is_empty() { Err(BAD_EXEC) } else { Ok(out) }
}

/// The text of a word with no field code.
fn plain(word: &[Piece]) -> Option<String> {
    word.iter().map(|p| if let Piece::Char(c) = p { Some(*c) } else { None }).collect()
}

/// The program of an Exec line: its first word, if it has no field code.
pub fn program(exec: &str) -> Option<String> {
    plain(words(exec).ok()?.first()?).filter(|t| !t.is_empty())
}

/// Whether `program` is a file: an absolute path, or a plain name in one of `path_var`'s
/// folders (a relative path with a slash is refused).
pub fn program_found(program: &str, path_var: Option<&str>, exists: &dyn Fn(&Path) -> bool) -> bool {
    program_path(program, path_var, exists).is_some()
}

/// The absolute file `program` names: itself if absolute, else the first in one of
/// `path_var`'s absolute folders (a relative folder like `.` would be the launch's own folder).
pub fn program_path(program: &str, path_var: Option<&str>, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    if program.contains('/') {
        let path = Path::new(program);
        return (path.has_root() && exists(path)).then(|| path.to_path_buf());
    }
    path_var.unwrap_or_default().split(':').map(|d| Path::new(d).join(program)).find(|p| p.has_root() && exists(p))
}

/// Whether Open With may offer the entry (decisions 10, 12): not hidden, not a terminal app,
/// shown on this desktop, and its TryExec and its program are there (`found`).
pub fn usable(entry: &DesktopEntry, desktops: &[String], found: &dyn Fn(&str) -> bool) -> bool {
    !entry.hidden
        && !entry.terminal
        && shown_on(entry, desktops)
        && entry.try_exec.as_deref().is_none_or(found)
        && program(&entry.exec).is_some_and(|p| found(&p))
}

/// What one Exec word stands for.
enum Slot {
    Text(String),
    OneFile { uri: bool },
    AllFiles { uri: bool },
    Icon,
}

fn slot_of(word: &[Piece], entry: &DesktopEntry, desktop: &Path) -> Result<Option<Slot>, &'static str> {
    match word {
        [Piece::Code('f')] => return Ok(Some(Slot::OneFile { uri: false })),
        [Piece::Code('u')] => return Ok(Some(Slot::OneFile { uri: true })),
        [Piece::Code('F')] => return Ok(Some(Slot::AllFiles { uri: false })),
        [Piece::Code('U')] => return Ok(Some(Slot::AllFiles { uri: true })),
        [Piece::Code('i')] => return Ok(Some(Slot::Icon)),
        _ => {}
    }
    let mut text = String::new();
    let mut only_dropped = true;
    for piece in word {
        match *piece {
            Piece::Char(c) => {
                text.push(c);
                only_dropped = false;
            }
            Piece::Code('c') => {
                text.push_str(&entry.name);
                only_dropped = false;
            }
            Piece::Code('k') => {
                text.push_str(&desktop.to_string_lossy());
                only_dropped = false;
            }
            Piece::Code('d' | 'D' | 'n' | 'N' | 'v' | 'm') => {}
            // %f %u %F %U %i inside a word, or an unknown code (decision 11).
            Piece::Code(_) => return Err(BAD_EXEC),
        }
    }
    // A word that was only deprecated codes goes; `""` stays an empty argument.
    Ok((!(only_dropped && !word.is_empty())).then_some(Slot::Text(text)))
}

/// The argv of each launch of `entry` (its desktop file at `desktop`) for `files` (decision
/// 11): one launch, or one per file for `%f`/`%u`. Files must be rooted paths.
pub fn expand(entry: &DesktopEntry, desktop: &Path, files: &[PathBuf]) -> Result<Vec<Vec<OsString>>, &'static str> {
    if files.iter().any(|f| !f.has_root()) {
        return Err(BAD_EXEC);
    }
    let words = words(&entry.exec)?;
    // The program word carries no field code (not even `%c`, `%k` or a dropped one).
    if words.first().and_then(|w| plain(w)).is_none_or(|p| p.is_empty()) {
        return Err(BAD_EXEC);
    }
    let mut slots = Vec::new();
    for word in &words {
        slots.extend(slot_of(word, entry, desktop)?);
    }
    let single = slots.iter().any(|s| matches!(s, Slot::OneFile { .. }));
    let multi = slots.iter().any(|s| matches!(s, Slot::AllFiles { .. }));
    if single && multi {
        return Err(BAD_EXEC);
    }
    let arg = |file: &PathBuf, uri: bool| {
        if uri { OsString::from(crate::linux::uri::file_uri(file)) } else { file.clone().into_os_string() }
    };
    let build = |one: Option<&PathBuf>| {
        let mut argv: Vec<OsString> = Vec::new();
        for slot in &slots {
            match slot {
                Slot::Text(text) => argv.push(OsString::from(text)),
                Slot::OneFile { uri } => argv.extend(one.map(|f| arg(f, *uri))),
                Slot::AllFiles { uri } => argv.extend(files.iter().map(|f| arg(f, *uri))),
                Slot::Icon => {
                    if let Some(icon) = &entry.icon {
                        argv.push("--icon".into());
                        argv.push(icon.into());
                    }
                }
            }
        }
        if !single && !multi {
            argv.extend(files.iter().map(|f| f.clone().into_os_string()));
        }
        argv
    };
    Ok(if single && files.len() > 1 {
        files.iter().map(|f| build(Some(f))).collect()
    } else {
        vec![build(files.first())]
    })
}

/// Starts `files` with the app of the desktop file `desktop` (decision 11): no shell, the
/// program by its absolute path, each launch its own process group with stdin/stdout/stderr
/// null, waited for on a small thread (`terminal::start`). Blocking (reads the entry, looks
/// through PATH): not on the UI thread.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn launch(desktop: &Path, files: &[PathBuf]) -> Result<(), String> {
    let entry = crate::linux::mime::read_entry(desktop)
        .filter(|e| !e.hidden && !e.terminal)
        .ok_or_else(|| "The app's desktop file cannot be read".to_owned())?;
    let dir = files
        .first()
        .and_then(|f| f.parent())
        .map(Path::to_path_buf)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"));
    let path_var = std::env::var("PATH").ok();
    for argv in expand(&entry, desktop, files)? {
        let Some((program, args)) = argv.split_first() else { continue };
        let program = program
            .to_str()
            .and_then(|p| program_path(p, path_var.as_deref(), &|p| p.is_file()))
            .ok_or_else(|| "The app's program was not found".to_owned())?;
        let launch = crate::terminal::Launch {
            program: program.into_os_string(),
            args: args.to_vec(),
            dir: dir.clone(),
            elevated: false,
        };
        crate::terminal::start(&launch).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A desktop file's id: its path below the last `applications` folder, `/` as `-`
/// (`…/applications/kde4/a.desktop` is `kde4-a.desktop`).
pub fn desktop_id_of(path: &Path) -> Option<String> {
    let parts: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let at = parts.iter().rposition(|p| p == "applications")?;
    let id = parts.get(at + 1..)?.join("-");
    (id.len() > ".desktop".len() && id.ends_with(".desktop")).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIMP: &str = "\u{feff}[Desktop Entry]\r\nType=Application\r\nName=GIMP\r\nName[tr]=GIMP Resim\r\nExec=gimp-2.10 %U\r\nTryExec=gimp-2.10\r\nIcon=gimp\r\nMimeType=image/png;image/x-xcf;\r\nTerminal=false\r\n\r\n[Desktop Action new]\r\nName=New\r\nExec=rm -rf %f\r\n";

    fn entry(exec: &str) -> DesktopEntry {
        DesktopEntry { name: "Viewer".into(), exec: exec.into(), icon: Some("viewer".into()), ..Default::default() }
    }

    fn os(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn entries_parse_only_their_first_group() {
        let e = parse(GIMP).unwrap();
        assert_eq!(e.name, "GIMP", "the unlocalized name");
        assert_eq!(e.exec, "gimp-2.10 %U", "the action's Exec does not win");
        assert_eq!(e.try_exec.as_deref(), Some("gimp-2.10"));
        assert_eq!(e.mime_types, ["image/png", "image/x-xcf"]);
        assert!(!e.terminal && !e.hidden && !e.no_display);
        assert_eq!(parse("[Desktop Entry]\nType=Application\nName=X\n"), None, "no Exec: not an app");
        assert_eq!(parse("[Desktop Entry]\nType=Link\nName=X\nExec=x\n"), None, "not an application");
        assert_eq!(parse("[Other]\nExec=x\n"), None);
        let hidden = parse("[Desktop Entry]\nHidden=true\n").unwrap();
        assert!(hidden.hidden, "a hidden entry hides the id even with nothing else");
        let first = parse("[Desktop Entry]\nType=Application\nName=A\nName=B\nExec=a\nExec=b\n").unwrap();
        assert_eq!((first.name.as_str(), first.exec.as_str()), ("A", "a"), "the first value of a key");
        let lists = parse(
            "[Desktop Entry]\nType=Application\nName=A\nExec=a\nOnlyShowIn=GNOME;KDE;\nNotShowIn=X\\;Y;\nNoDisplay=true\n",
        )
        .unwrap();
        assert_eq!(lists.only_show_in, ["GNOME", "KDE"]);
        assert_eq!(lists.not_show_in, ["X;Y"], "an escaped separator");
        assert!(lists.no_display);
    }

    #[test]
    fn strings_unescape() {
        assert_eq!(unescape(r"a\sb\tc\\d"), "a b\tc\\d");
        assert_eq!(unescape(r#"say \"x\""#), r#"say \"x\""#, "Exec's own escapes are left for its rule");
        assert_eq!(unescape("ends\\"), "ends\\");
        assert_eq!(split_list(r"a;b\;c;;\\;d"), ["a", "b;c", "\\", "d"]);
    }

    #[test]
    fn usable_entries() {
        let found = |p: &str| matches!(p, "gimp-2.10" | "/usr/bin/viewer");
        let gimp = parse(GIMP).unwrap();
        assert!(usable(&gimp, &["gnome".into()], &found));
        assert!(!usable(&DesktopEntry { try_exec: Some("missing".into()), ..gimp.clone() }, &[], &found));
        assert!(!usable(&DesktopEntry { exec: "missing %f".into(), try_exec: None, ..gimp.clone() }, &[], &found));
        assert!(!usable(&DesktopEntry { terminal: true, ..gimp.clone() }, &[], &found), "decision 12");
        assert!(!usable(&DesktopEntry { hidden: true, ..gimp.clone() }, &[], &found));
        let kde_only = DesktopEntry { only_show_in: vec!["KDE".into()], ..gimp.clone() };
        assert!(usable(&kde_only, &["kde".into()], &found), "case does not matter");
        assert!(!usable(&kde_only, &["gnome".into()], &found));
        assert!(!usable(
            &DesktopEntry { not_show_in: vec!["GNOME".into()], ..gimp },
            &["ubuntu".into(), "gnome".into()],
            &found
        ));
        let exists = |p: &Path| {
            p.to_string_lossy().replace('\\', "/") == "/usr/bin/viewer"
                || p.to_string_lossy().replace('\\', "/") == "/opt/bin/tool"
        };
        assert!(program_found("/usr/bin/viewer", None, &exists));
        assert!(program_found("tool", Some("/bin::/opt/bin"), &exists));
        assert!(!program_found("bin/viewer", Some("/usr"), &exists), "a relative path with a slash");
        assert!(!program_found("", Some("/usr/bin"), &exists));
        let here = |p: &Path| p.to_string_lossy().replace('\\', "/") == "./tool";
        assert!(!program_found("tool", Some(".:"), &here), "relative PATH folders are not searched");
        assert_eq!(
            program_path("tool", Some("rel:/bin:/opt/bin"), &exists).map(|p| p.to_string_lossy().replace('\\', "/")),
            Some("/opt/bin/tool".into())
        );
        assert_eq!(program(r#""/opt/My App/app" --x %F"#).as_deref(), Some("/opt/My App/app"));
        assert_eq!(program("%f"), None);
    }

    #[test]
    fn codes_expand_by_the_spec() {
        let desk = Path::new("/usr/share/applications/viewer.desktop");
        let (a, b) = (PathBuf::from("/home/u/a b.png"), PathBuf::from("/home/u/ağaç.png"));
        let both = [a.clone(), b.clone()];
        assert_eq!(
            expand(&entry("viewer %F"), desk, &both),
            Ok(vec![os(&["viewer", "/home/u/a b.png", "/home/u/ağaç.png"])])
        );
        assert_eq!(
            expand(&entry("viewer --one %f"), desk, &both),
            Ok(vec![os(&["viewer", "--one", "/home/u/a b.png"]), os(&["viewer", "--one", "/home/u/ağaç.png"])]),
            "%f with two files: two launches"
        );
        assert_eq!(
            expand(&entry("viewer %U"), desk, &both),
            Ok(vec![os(&["viewer", "file:///home/u/a%20b.png", "file:///home/u/a%C4%9Fa%C3%A7.png"])])
        );
        assert_eq!(
            expand(&entry("viewer"), desk, &both),
            Ok(vec![os(&["viewer", "/home/u/a b.png", "/home/u/ağaç.png"])]),
            "no code: appended"
        );
        assert_eq!(expand(&entry("viewer %f"), desk, &[]), Ok(vec![os(&["viewer"])]), "no file: the code goes");
        assert_eq!(
            expand(&entry("viewer %i --name %c --from %k 100%% %d %m"), desk, std::slice::from_ref(&a)),
            Ok(vec![os(&[
                "viewer",
                "--icon",
                "viewer",
                "--name",
                "Viewer",
                "--from",
                "/usr/share/applications/viewer.desktop",
                "100%",
                "/home/u/a b.png"
            ])]),
            "deprecated codes vanish; no file code: the file is appended"
        );
        assert_eq!(
            expand(&DesktopEntry { icon: None, ..entry("viewer %i %f") }, desk, std::slice::from_ref(&a)),
            Ok(vec![os(&["viewer", "/home/u/a b.png"])])
        );
        assert_eq!(
            expand(&entry(r#""/opt/My App/app" "say \"hi\" \$HOME \`x\` \\" %F"#), desk, std::slice::from_ref(&a)),
            Ok(vec![os(&["/opt/My App/app", r#"say "hi" $HOME `x` \"#, "/home/u/a b.png"])])
        );
        assert_eq!(
            expand(&entry("env A=1 viewer \"\" %u"), desk, &[]),
            Ok(vec![os(&["env", "A=1", "viewer", ""])]),
            "an empty quoted word stays"
        );
        assert_eq!(
            expand(&DesktopEntry { name: "My \"App\" %f".into(), ..entry("viewer --title=%c %F") }, desk, &[]),
            Ok(vec![os(&["viewer", "--title=My \"App\" %f"])]),
            "a name is text in its own word, never a code or more words"
        );
    }

    #[test]
    fn field_codes_in_quotes_or_inside_words_are_refused() {
        let desk = Path::new("/a/x.desktop");
        let a = [PathBuf::from("/home/u/a.txt")];
        for bad in [
            r#"sh -c "cat %f""#,
            r#"sh -c "cat %c""#,
            "bash -c cat%f",
            "viewer --file=%f",
            "viewer --icon=%i",
            "viewer %F %f",
            "viewer %u %U",
            "viewer %x",
            "viewer %",
            "viewer 100%",
            "%f",
            "%c --x",
            "%d viewer",
            "x%k",
            r#""" %f"#,
            r#"viewer "unclosed"#,
            r#"viewer "bad \q escape""#,
            "",
            "   ",
        ] {
            assert_eq!(expand(&entry(bad), desk, &a), Err(BAD_EXEC), "{bad:?}");
        }
        assert!(expand(&entry("viewer"), desk, &[PathBuf::from("relative.txt")]).is_err(), "never a relative file");
        assert!(expand(&entry("viewer"), desk, &[PathBuf::from("-rf")]).is_err(), "never an option-like word");
    }

    #[test]
    fn file_names_reach_argv_untouched() {
        let desk = Path::new("/a/x.desktop");
        let nasty = [
            "/tmp/$(touch pwned) `id` ;rm -rf ~;.txt",
            "/tmp/it's \"quoted\".txt",
            "/tmp/%f %U %%.txt",
            "/tmp/line\nbreak.txt",
            "/tmp/-rf",
            "/tmp/ \t lead and trail \\ ",
        ];
        for name in nasty {
            let file = PathBuf::from(name);
            let one = std::slice::from_ref(&file);
            assert_eq!(expand(&entry("viewer %F"), desk, one), Ok(vec![os(&["viewer", name])]), "{name:?}");
            assert_eq!(expand(&entry("viewer --one %f"), desk, one), Ok(vec![os(&["viewer", "--one", name])]));
            assert_eq!(expand(&entry("viewer"), desk, one), Ok(vec![os(&["viewer", name])]));
            let uri = crate::linux::uri::file_uri(&file);
            assert!(!uri.contains([' ', '"', '`', '\n', '\t', '\\']), "{uri:?}");
            assert_eq!(expand(&entry("viewer %u"), desk, one), Ok(vec![os(&["viewer", &uri])]));
        }
    }

    /// Fuzz-ish: random Exec lines from tricky pieces, run once with plain marker files and
    /// once with hostile names. Never a panic; both runs agree on success and on every word
    /// except the files, which are whole words, and file names never change the shape.
    #[test]
    fn hostile_exec_lines_and_names_never_add_words() {
        const PIECES: [&str; 24] = [
            "viewer", " ", " ", "\"", "\\", "%", "f", "F", "u", "U", "i", "c", "k", "d", "%%", "\t", "\n", "$", "`",
            "x", "=", "--a", "%f", "%U",
        ];
        let desk = Path::new("/a/x.desktop");
        let marks = [PathBuf::from("/m0"), PathBuf::from("/m1")];
        let hostile = [PathBuf::from("/t/%f \"a\" %U\n-x"), PathBuf::from("/t/-rf ;$(id)`id`'\\")];
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |n: usize| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(seed >> 33).unwrap_or(0) % n
        };
        let mut ok_runs = 0;
        for _ in 0..5000 {
            let len = 1 + next(10);
            let exec: String = (0..len).map(|_| PIECES[next(PIECES.len())]).collect();
            let e = entry(&format!("viewer {exec}"));
            for count in [0, 1, 2] {
                let plain_run = expand(&e, desk, &marks[..count]);
                let hostile_run = expand(&e, desk, &hostile[..count]);
                assert_eq!(plain_run.is_ok(), hostile_run.is_ok(), "{exec:?}");
                let (Ok(plain_run), Ok(hostile_run)) = (plain_run, hostile_run) else { continue };
                ok_runs += 1;
                assert_eq!(plain_run.len(), hostile_run.len(), "{exec:?}");
                for (p, h) in plain_run.iter().zip(&hostile_run) {
                    assert_eq!(p.len(), h.len(), "a file name added or removed words: {exec:?}");
                    for (pw, hw) in p.iter().zip(h) {
                        let at = (0..count)
                            .find(|&i| *pw == *marks[i].as_os_str() || *pw == *crate::linux::uri::file_uri(&marks[i]));
                        match at {
                            Some(i) => assert!(
                                *hw == *hostile[i].as_os_str() || *hw == *crate::linux::uri::file_uri(&hostile[i]),
                                "{exec:?}"
                            ),
                            None => assert_eq!(pw, hw, "a file name leaked into another word: {exec:?}"),
                        }
                    }
                }
            }
        }
        assert!(ok_runs > 500, "the generator reaches the expander ({ok_runs})");
    }

    #[test]
    fn gezik_s_own_desktop_entry_reads_back() {
        // 9b4 writes it with its own quoting: both rules must agree.
        let exe = r#"/home/u/gé zik/"q"$x\gezik"#;
        let text = crate::system::text::desktop_entry(exe);
        let e = parse(&text).unwrap();
        let argv = expand(&e, Path::new("/x/gezik.desktop"), &[PathBuf::from("/home/u/d")]).unwrap();
        assert_eq!(argv, vec![os(&[exe, "file:///home/u/d"])]);
    }

    #[test]
    fn desktop_ids_come_from_the_applications_folder() {
        assert_eq!(desktop_id_of(Path::new("/usr/share/applications/gimp.desktop")).as_deref(), Some("gimp.desktop"));
        assert_eq!(
            desktop_id_of(Path::new("/usr/share/applications/kde4/kate.desktop")).as_deref(),
            Some("kde4-kate.desktop")
        );
        assert_eq!(desktop_id_of(Path::new("/home/u/x/gimp.desktop")), None);
        assert_eq!(desktop_id_of(Path::new("/usr/share/applications/.desktop")), None);
    }
}
