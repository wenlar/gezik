//! The administrator helper (`gezik --elevated 1 <channel> <operations>`, spec 9 §10): this exe,
//! started by the system's prompt. It does the one list on its command line and exits: no
//! window, no Slint, no settings, no environment, nothing read from stdin, nothing written but
//! its replies. The whole list is checked before anything is done; one bad item and nothing is.

use std::io::{self, Write};
use std::path::PathBuf;

use gezik_core::elevated::{self, Op, System};
use gezik_platform::fs::secure::{self, Guard};

/// The helper's whole life; the exit code (0 once the channel was opened: macOS's
/// `do shell script` drops the output of a command that fails).
pub fn main() -> i32 {
    // First of all (security review): system DLLs only, before anything can load one.
    if gezik_platform::elevate::harden().is_err() {
        return 2;
    }
    gezik_platform::process::quiet_crashes();
    let Some(args) = std::env::args_os().skip(1).map(|arg| arg.into_string().ok()).collect::<Option<Vec<String>>>()
    else {
        return 2;
    };
    // The channel is checked and opened before anything else of the list is read.
    let Some(channel) = elevated::channel_of(&args) else { return 2 };
    let Ok(mut out) = gezik_platform::elevate::open_channel(channel) else { return 2 };
    let protected = gezik_platform::elevate::protected();
    serve(&args, &mut out, gezik_platform::elevate::is_elevated(), &protected);
    let _ = out.flush();
    0
}

/// Checks everything first, then does each operation in order with a reply each, then `done`.
pub(crate) fn serve(args: &[String], out: &mut dyn Write, elevated: bool, protected: &[PathBuf]) {
    let mut say = |line: String| {
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    };
    if !elevated {
        return say(elevated::refused_line(elevated::NOT_ELEVATED));
    }
    let ops = match elevated::decode(args) {
        Ok((_, ops)) => ops,
        Err(why) => return say(elevated::refused_line(why)),
    };
    let system = System { windows: cfg!(windows), macos: cfg!(target_os = "macos"), protected };
    if let Err((index, why)) = elevated::check(&ops, &system) {
        return say(elevated::refused_line(&format!("item {}: {why}", index + 1)));
    }
    // The identity check: other spellings of a system folder (case, short name, firmlink).
    let guard = Guard::new(protected);
    for (index, op) in ops.iter().enumerate() {
        match run(op, &guard) {
            Ok(()) => say(elevated::ok_line(index)),
            Err(err) => {
                say(elevated::err_line(index, err.raw_os_error().unwrap_or(0), &gezik_platform::fs::describe(&err)))
            }
        }
    }
    say(elevated::DONE.to_owned());
}

fn run(op: &Op, guard: &Guard) -> io::Result<()> {
    match op {
        Op::Copy { from, to, replace } => secure::copy(from, to, *replace),
        Op::Move { from, to, replace } => secure::move_to(from, to, *replace, guard),
        Op::Delete(path) => secure::delete(path, guard),
        Op::Rename { path, name } => secure::rename(path, name, guard),
        Op::Mkdir(path) => secure::mkdir(path),
        Op::Rmdir(path) => secure::rmdir(path, guard),
        #[cfg(unix)]
        Op::Chmod { path, mode } => secure::chmod(path, *mode, guard),
        #[cfg(unix)]
        Op::Chown { path, uid, gid } => secure::chown(path, *uid, *gid, guard),
        #[cfg(target_os = "macos")]
        Op::Chflags { path, set, clear } => secure::chflags(path, *set, *clear, guard),
        // `check` refuses these here before anything runs.
        #[allow(unreachable_patterns)]
        _ => Err(io::ErrorKind::Unsupported.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::elevated::{STDOUT, encode};
    use std::fs;

    /// A fresh folder with its real spelling (see `secure`'s tests).
    fn base(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-helper-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let real = fs::canonicalize(&dir).unwrap();
        match real.to_str().and_then(|t| t.strip_prefix(r"\\?\")) {
            Some(plain) => PathBuf::from(plain),
            None => real,
        }
    }

    /// What the helper writes for `ops`, run (without rights) as if it had them.
    fn answer(ops: &[Op], elevated: bool, protected: &[PathBuf]) -> Vec<String> {
        let args = encode(STDOUT, ops).unwrap();
        let mut out = Vec::new();
        serve(&args, &mut out, elevated, protected);
        String::from_utf8(out).unwrap().lines().map(str::to_owned).collect()
    }

    /// The first statement of the body after `head` in `source`, comments skipped.
    fn first_statement<'a>(source: &'a str, head: &str) -> &'a str {
        let body = source.split(head).nth(1).unwrap();
        body.lines().skip(1).map(str::trim).find(|line| !line.is_empty() && !line.starts_with("//")).unwrap()
    }

    #[test]
    fn the_helper_hardens_before_anything_else() {
        // Nothing of Gezik (a window, a delay-loaded DLL, the PDF worker) runs before the helper,
        // and the helper's first step is the DLL search hardening.
        let main_rs = include_str!("main.rs").replace("\r\n", "\n");
        assert!(first_statement(&main_rs, "\nfn main()").contains("gezik_core::elevated::ARG"));
        let this = include_str!("elevated.rs").replace("\r\n", "\n");
        assert!(first_statement(&this, "\npub fn main() -> i32 {").contains("elevate::harden()"));
    }

    #[test]
    fn a_whole_list_is_done_with_a_reply_each() {
        let dir = base("list");
        fs::write(dir.join("a.txt"), "a").unwrap();
        let ops = [
            Op::Mkdir(dir.join("d")),
            Op::Copy { from: dir.join("a.txt"), to: dir.join("d").join("a.txt"), replace: false },
            Op::Rename { path: dir.join("d").join("a.txt"), name: "b.txt".into() },
            Op::Delete(dir.join("a.txt")),
        ];
        assert_eq!(answer(&ops, true, &[]), ["ok 0", "ok 1", "ok 2", "ok 3", "done"]);
        assert_eq!(fs::read_to_string(dir.join("d").join("b.txt")).unwrap(), "a");
        assert!(!dir.join("a.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_runs_unless_elevated() {
        let dir = base("not-elevated");
        let lines = answer(&[Op::Mkdir(dir.join("d"))], false, &[]);
        assert_eq!(lines, [elevated::refused_line(elevated::NOT_ELEVATED)]);
        assert!(!dir.join("d").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_item_stops_the_whole_list() {
        let dir = base("bad-item");
        let ops = [Op::Mkdir(dir.join("d")), Op::Delete(PathBuf::from("relative"))];
        let lines = answer(&ops, true, &[]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("refused item 2: "), "{lines:?}");
        assert!(!dir.join("d").exists(), "the good first item did not run either");
        let mut out = Vec::new();
        serve(&["--elevated".into(), "9".into(), "-".into()], &mut out, true, &[]);
        assert!(String::from_utf8(out).unwrap().starts_with("refused "), "another version: nothing");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_system_folder_is_refused_even_spelled_otherwise() {
        let dir = base("guarded");
        fs::create_dir_all(dir.join("Sys").join("inside")).unwrap();
        let protected = [dir.join("Sys")];
        let refused = answer(&[Op::Delete(dir.join("Sys"))], true, &protected);
        assert!(refused[0].starts_with("refused item 1: "), "{refused:?}");
        if cfg!(any(windows, target_os = "macos")) {
            // The words differ, the folder does not: the identity check catches it.
            let lines = answer(&[Op::Delete(dir.join("SYS"))], true, &protected);
            assert!(lines.concat().contains("refused") || lines[0].starts_with("err 0"), "{lines:?}");
        }
        assert!(dir.join("Sys").join("inside").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_is_never_followed() {
        let dir = base("links");
        fs::create_dir_all(dir.join("outside")).unwrap();
        fs::write(dir.join("outside").join("keep.txt"), "keep").unwrap();
        fs::create_dir_all(dir.join("tree")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("outside"), dir.join("tree").join("link")).unwrap();
        #[cfg(windows)]
        gezik_platform::link::create(
            gezik_core::templates::LinkKind::Junction,
            &dir.join("outside"),
            &dir.join("tree").join("link"),
            true,
        )
        .unwrap();
        let through = dir.join("tree").join("link").join("keep.txt");
        let lines = answer(&[Op::Delete(through), Op::Delete(dir.join("tree"))], true, &[]);
        assert!(lines[0].starts_with("err 0 ") && lines[0].contains("link"), "{lines:?}");
        assert_eq!(lines[1..], ["ok 1", "done"]);
        assert!(!dir.join("tree").exists());
        assert_eq!(fs::read_to_string(dir.join("outside").join("keep.txt")).unwrap(), "keep");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replies_carry_the_systems_words() {
        let dir = base("words");
        let lines = answer(&[Op::Delete(dir.join("missing"))], true, &[]);
        assert_eq!(lines, ["err 0 2 It no longer exists", "done"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
