//! macOS: tries the system calls parts 9a1, 9a2 and 9a3 rely on (spec 9 §4.1-§4.5) through Gezik's
//! own code, and prints what they did. On a Mac:
//! `cargo run --release -p gezik-platform --example mac_probe -- [--service "<Quick Action>"] [paths…]`
//! and paste the whole output into docs/superpowers/notes/macos-test-results.md.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("mac_probe runs on macOS only");
}

#[cfg(target_os = "macos")]
fn main() {
    use std::path::PathBuf;
    use std::time::Instant;

    use gezik_platform::finder::{self, AliasTarget};
    use gezik_platform::{IconTarget, Rgba, folder_has_own_icon, icon, only_in_cloud, thumbnail_while};

    fn describe(image: &Option<Rgba>) -> String {
        image.as_ref().map_or("none".to_owned(), |i| format!("{}x{}", i.width, i.height))
    }
    fn ms(start: Instant) -> f64 {
        start.elapsed().as_secs_f64() * 1000.0
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let service = args.iter().position(|a| a == "--service").map(|i| {
        let title = args.get(i + 1).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
        args.drain(i..(i + 2).min(args.len()));
        title
    });
    let mut paths: Vec<PathBuf> = args.into_iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        paths = vec![
            "/Applications/Safari.app".into(),
            "/Applications".into(),
            "/Users".into(),
            home.join("Documents"),
            home.join("Downloads"),
        ];
    }

    println!("== 1. Icons: the main thread and a worker give the same pixels (spec 4.1) ==");
    let mut targets: Vec<IconTarget> = paths.iter().cloned().map(IconTarget::Path).collect();
    targets.extend([IconTarget::Folder, IconTarget::Extension("pdf".into()), IconTarget::Extension(String::new())]);
    for target in targets {
        let start = Instant::now();
        let main = icon(&target, 64);
        let main_ms = ms(start);
        let worker_target = target.clone();
        let worker = std::thread::spawn(move || icon(&worker_target, 64)).join().expect("the worker panicked");
        let verdict = if main.is_some() && main == worker { "PASS" } else { "FAIL" };
        println!("{verdict} {target:?}: main {} in {main_ms:.1} ms, worker {}", describe(&main), describe(&worker));
    }
    let start = Instant::now();
    let worker = std::thread::spawn(|| {
        (0..1000)
            .filter(|i| icon(&IconTarget::Extension(["txt", "pdf", "png", "zip", "md"][i % 5].into()), 32).is_some())
            .count()
    });
    println!("1000 type icons on a worker: {} found in {:.0} ms", worker.join().unwrap_or(0), ms(start));
    // Gezik has one icon worker, but a crash here would show NSWorkspace is not thread safe.
    let start = Instant::now();
    let found: usize = (0..4)
        .map(|_| std::thread::spawn(|| (0..250).filter(|_| icon(&IconTarget::Folder, 32).is_some()).count()))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|worker| worker.join().unwrap_or(0))
        .sum();
    println!(
        "{} 4 workers at once, 1000 folder icons: {found} in {:.0} ms",
        if found == 1000 { "PASS" } else { "FAIL" },
        ms(start)
    );
    for path in paths.iter().filter(|p| p.is_dir()) {
        let start = Instant::now();
        let own = folder_has_own_icon(path);
        println!("folder_has_own_icon {}: {own} in {:.1} ms", path.display(), ms(start));
    }

    println!("== 2. Quick Look thumbnails on a worker while the main thread waits (spec 4.5) ==");
    for path in paths.iter().filter(|p| p.is_file()) {
        let shown = path.display().to_string();
        if only_in_cloud(path) {
            println!("SKIP {shown}: only in iCloud, not read (Finder should still show the cloud icon)");
            continue;
        }
        let path = path.clone();
        let start = Instant::now();
        let image = std::thread::spawn(move || thumbnail_while(&path, 256, &|| true)).join().ok().flatten();
        println!(
            "{} {shown}: {} in {:.0} ms",
            if image.is_some() { "PASS" } else { "NONE" },
            describe(&image),
            ms(start)
        );
    }
    if let Some(big) = paths.iter().find(|p| p.is_file() && !only_in_cloud(p)).cloned() {
        let start = Instant::now();
        let given_up = thumbnail_while(&big, 1024, &|| start.elapsed().as_millis() < 5);
        println!(
            "cancel after 5 ms: {} in {:.0} ms (expect none, under ~100 ms; a PNG/JPEG may come at once)",
            describe(&given_up),
            ms(start)
        );
    } else {
        println!("(give a PDF, a HEIC, a MOV and a .pages file as arguments to try thumbnails)");
    }

    println!("== 3. Finder names, packages, aliases (spec 4.2) ==");
    for path in &paths {
        println!(
            "{}: finder name {:?}, package {}, alias {:?}",
            path.display(),
            finder::finder_name(path),
            finder::is_package(path),
            finder::resolve_alias(path)
        );
    }
    let dir = std::env::temp_dir().join(format!("gezik-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("folder")).expect("a temp folder");
    std::fs::write(dir.join("a.txt"), "a").expect("a temp file");
    for (target, alias) in
        [("a.txt", "a.txt alias"), ("folder", "folder alias"), ("/Applications/Safari.app", "Safari alias")]
    {
        let made = finder::make_alias(&dir.join(target), &dir.join(alias));
        let resolved = finder::resolve_alias(&dir.join(alias));
        let ok = made.is_ok() && matches!(resolved, AliasTarget::Target { .. });
        println!("{} make + resolve {alias}: {made:?} → {resolved:?}", if ok { "PASS" } else { "FAIL" });
    }
    let again = finder::make_alias(&dir.join("a.txt"), &dir.join("a.txt alias"));
    println!("{} a second alias of the same name is refused: {again:?}", if again.is_err() { "PASS" } else { "FAIL" });
    std::fs::remove_file(dir.join("a.txt")).ok();
    let gone = finder::resolve_alias(&dir.join("a.txt alias"));
    println!("{} original gone: {gone:?}", if gone == AliasTarget::Missing { "PASS" } else { "FAIL" });
    println!("== 4. Open With: LaunchServices on the main thread and a worker (spec 4.3) ==");
    use gezik_platform::open_with;
    let files: Vec<PathBuf> = paths.iter().filter(|p| p.is_file() || finder::is_package(p)).cloned().collect();
    for path in &files {
        let start = Instant::now();
        let main = open_with::apps(std::slice::from_ref(path));
        let main_ms = ms(start);
        let worker_path = path.clone();
        let start = Instant::now();
        let worker = std::thread::spawn(move || open_with::apps(&[worker_path])).join().unwrap_or_default();
        let verdict = if !main.is_empty() && main == worker { "PASS" } else { "FAIL" };
        let names: Vec<String> =
            main.iter().map(|a| if a.default { format!("{} (default)", a.name) } else { a.name.clone() }).collect();
        println!(
            "{verdict} {}: {main_ms:.1} ms main, {:.1} ms worker: {}",
            path.display(),
            ms(start),
            names.join(", ")
        );
    }
    if files.len() > 1 {
        let start = Instant::now();
        let shared = open_with::apps(&files);
        println!("apps for all {} files: {} in {:.1} ms", files.len(), shared.len(), ms(start));
        let fifty: Vec<PathBuf> = files.iter().cycle().take(open_with::MAX_ITEMS).cloned().collect();
        let start = Instant::now();
        let count = open_with::apps(&fifty).len();
        println!("50 items (the menu waits 50 ms): {count} apps in {:.1} ms", ms(start));
    } else {
        println!("(give two or more files, e.g. a .txt and a .pdf, to try Open With for several items)");
    }
    println!("== 5. Quick Actions: installed bundles, types, running one (spec 4.3, decision 26) ==");
    use gezik_platform::services;
    for path in &paths {
        let start = Instant::now();
        let offered: Vec<String> =
            services::for_items(std::slice::from_ref(path)).into_iter().map(|s| s.title).collect();
        println!("{}: {} in {:.1} ms: {}", path.display(), offered.len(), ms(start), offered.join(", "));
    }
    let start = Instant::now();
    match std::process::Command::new("/System/Library/CoreServices/pbs").arg("-dump_pboard").output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            println!(
                "pbs -dump_pboard: {} bytes, {} NSSendFileTypes, {} NSMenuItem in {:.0} ms (all services, apps' too)",
                text.len(),
                text.matches("NSSendFileTypes").count(),
                text.matches("NSMenuItem").count(),
                ms(start)
            );
        }
        Err(err) => println!("pbs -dump_pboard: not run ({err})"),
    }
    match service {
        Some(title) => {
            let file = dir.join("service-test.txt");
            std::fs::write(&file, "gezik").ok();
            let ran = services::perform(&title, std::slice::from_ref(&file));
            println!(
                "{} perform \"{title}\" on {}: {ran:?} (check that it did its work)",
                if ran.is_ok() { "PASS" } else { "FAIL" },
                file.display()
            );
        }
        None => println!(
            "(make a Quick Action in Automator or Shortcuts that takes files, then rerun with --service \"<its name>\")"
        ),
    }
    println!("== 6. Get Info: attributes without following links, names, ACLs (spec 4.4) ==");
    {
        use gezik_core::attrs::{Change, HIDDEN, LOCKED, PERMS};
        use gezik_platform::attrs;
        let check = |label: &str, ok: bool| println!("{} {label}", if ok { "PASS" } else { "FAIL" });
        let start = Instant::now();
        let (users, groups, mine) = (attrs::users(), attrs::groups(), attrs::my_groups());
        println!("{} users, {} groups, mine {mine:?} in {:.1} ms", users.len(), groups.len(), ms(start));
        let file = dir.join("info-test.txt");
        std::fs::write(&file, "gezik").ok();
        let link = dir.join("info-link");
        let _ = std::os::unix::fs::symlink(&file, &link);
        match attrs::read(&file) {
            Ok(entry) => {
                let a = entry.attrs;
                let shut = Change::flag(LOCKED, true).apply(Change::flag(HIDDEN, true).apply(a));
                check(
                    "hide and lock",
                    attrs::write(&file, entry.id, a, shut).is_ok()
                        && attrs::read(&file).is_ok_and(|e| e.attrs.flags == HIDDEN | LOCKED),
                );
                let chmod = Change::bit(0o020, true).apply(shut);
                check("a locked file's permissions are refused", attrs::write(&file, entry.id, shut, chmod).is_err());
                let open = Change::flag(LOCKED, false).apply(chmod);
                check(
                    "unlock and chmod in one write",
                    attrs::write(&file, entry.id, shut, open).is_ok()
                        && attrs::read(&file)
                            .is_ok_and(|e| e.attrs.mode & PERMS == (a.mode | 0o020) & PERMS && e.attrs.flags == HIDDEN),
                );
                check("unhide", attrs::write(&file, entry.id, open, Change::flag(HIDDEN, false).apply(open)).is_ok());
                if let Ok(l) = attrs::read(&link) {
                    check(
                        "a link's permissions are refused",
                        attrs::write(&link, l.id, l.attrs, Change::bit(0o002, true).apply(l.attrs)).is_err(),
                    );
                    check(
                        "what the link leads to is untouched",
                        attrs::read(&file).is_ok_and(|e| e.attrs.mode & 0o002 == 0),
                    );
                    if let Some(gid) = mine.first() {
                        check(
                            "a link's own group (lchown)",
                            attrs::write(&link, l.id, l.attrs, Change::group(*gid).apply(l.attrs)).is_ok(),
                        );
                    }
                }
                // chown clears setuid: the write puts it back and adds none.
                let suid = dir.join("info-suid");
                std::fs::write(&suid, "x").ok();
                let _ = std::process::Command::new("/bin/chmod").arg("4755").arg(&suid).status();
                if let (Ok(s), Some(gid)) = (attrs::read(&suid), mine.first()) {
                    let to = Change::group(*gid).apply(s.attrs);
                    let forced = gezik_core::attrs::Attrs { gid: !*gid, ..s.attrs };
                    check(
                        "setuid kept across a group change",
                        attrs::write(&suid, s.id, forced, to).is_ok()
                            && attrs::read(&suid).is_ok_and(|e| e.attrs.mode == 0o4755),
                    );
                }
                check("no ACL at first", !attrs::has_acl(&file));
                let acl = |args: &[&str]| {
                    std::process::Command::new("/bin/chmod").args(args).arg(&file).status().is_ok_and(|s| s.success())
                };
                check("an access control entry is seen", acl(&["+a", "everyone deny delete"]) && attrs::has_acl(&file));
                acl(&["-N"]);
            }
            Err(err) => println!("FAIL read {}: {err}", file.display()),
        }
    }
    println!("(leave {} for Finder: its aliases should show the arrow badge; delete it afterwards)", dir.display());
}
