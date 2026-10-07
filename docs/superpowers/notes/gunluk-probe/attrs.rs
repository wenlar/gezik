//! Q5: hidden and system attributes in the listing path, and what each costs.
//! `attrs [dir...]`
use std::path::Path;
use std::time::Instant;

/// Hidden/system as gezik-core's `list_dir` could fill them: from the DirEntry's metadata,
/// which on Windows comes from FindNextFileW's WIN32_FIND_DATAW (no extra call).
#[derive(Debug, Default, Clone, Copy)]
pub struct Flags {
    pub hidden: bool,
    pub system: bool,
}

#[cfg(windows)]
pub fn flags(meta: &std::fs::Metadata) -> Flags {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2; // FILE_ATTRIBUTE_HIDDEN
    const SYSTEM: u32 = 0x4; // FILE_ATTRIBUTE_SYSTEM
    let a = meta.file_attributes();
    Flags { hidden: a & HIDDEN != 0, system: a & SYSTEM != 0 }
}

/// macOS: UF_HIDDEN (chflags hidden; Finder hides these), from the lstat Gezik already does.
#[cfg(target_os = "macos")]
pub fn flags(meta: &std::fs::Metadata) -> Flags {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    Flags { hidden: meta.st_flags() & UF_HIDDEN != 0, system: false }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn flags(_: &std::fs::Metadata) -> Flags {
    Flags::default()
}

fn list(dir: &Path, with_flags: bool) -> (usize, usize, usize, usize) {
    let (mut n, mut h, mut s, mut hs) = (0, 0, 0, 0);
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let Ok(meta) = e.metadata() else { continue };
        n += 1;
        std::hint::black_box(meta.len());
        if with_flags {
            let f = flags(&meta);
            h += f.hidden as usize;
            s += f.system as usize;
            hs += (f.hidden && f.system) as usize;
        }
    }
    (n, h, s, hs)
}

#[cfg(windows)]
fn list_with_extra_call(dir: &Path) -> usize {
    use windows::Win32::Storage::FileSystem::GetFileAttributesW;
    use windows::core::HSTRING;
    let mut hidden = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let _ = e.metadata();
        let a = unsafe { GetFileAttributesW(&HSTRING::from(e.path().as_os_str())) };
        hidden += (a != u32::MAX && a & 0x2 != 0) as usize;
    }
    hidden
}

/// Explorer's own choice: "Hidden items" (fShowAllObjects) and "Hide protected operating
/// system files" (fShowSuperHidden off), through SHGetSetSettings (Win32_UI_Shell).
#[cfg(windows)]
fn explorer_settings() -> (bool, bool) {
    use windows::Win32::UI::Shell::{SHELLSTATEA, SHGetSetSettings, SSF_SHOWALLOBJECTS, SSF_SHOWSUPERHIDDEN};
    let mut state = SHELLSTATEA::default();
    unsafe { SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS | SSF_SHOWSUPERHIDDEN, false) };
    let bits = state._bitfield1;
    // SHELLSTATE bitfield order: fShowAllObjects is bit 0, fShowSuperHidden bit 15.
    (bits & 1 != 0, bits & (1 << 15) != 0)
}

fn main() {
    #[cfg(windows)]
    {
        let (all, superhidden) = explorer_settings();
        println!("Explorer: show hidden items = {all}, show protected OS files = {superhidden}");
    }
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    let dirs = if dirs.is_empty() {
        if cfg!(windows) {
            vec![r"C:\".into(), r"C:\Windows\System32".into(), std::env::var("USERPROFILE").unwrap_or_default()]
        } else {
            vec!["/usr/lib".into(), std::env::var("HOME").unwrap_or_default()]
        }
    } else {
        dirs
    };
    for dir in &dirs {
        let dir = Path::new(dir);
        let _ = list(dir, true); // warm the cache
        let best = |f: &dyn Fn()| (0..5).map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        }).min().unwrap();
        let plain = best(&|| {
            list(dir, false);
        });
        let with = best(&|| {
            list(dir, true);
        });
        let core = best(&|| {
            gezik_core::list_dir(dir).unwrap();
        });
        let (n, h, s, hs) = list(dir, true);
        println!("{}: {n} entries, hidden {h}, system {s}, both {hs}", dir.display());
        println!("  read_dir+metadata {plain:?}; + attributes {with:?}; gezik_core::list_dir {core:?}");
        #[cfg(windows)]
        {
            let extra = best(&|| {
                list_with_extra_call(dir);
            });
            println!("  + GetFileAttributesW per entry {extra:?}");
        }
        // The hidden + system ones by name, a few.
        let names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.metadata().map(|m| { let f = flags(&m); f.hidden || f.system }).unwrap_or(false))
            .take(8)
            .map(|e| {
                let f = flags(&e.metadata().unwrap());
                format!("{}{}{}", e.file_name().to_string_lossy(), if f.hidden { " [H]" } else { "" }, if f.system { "[S]" } else { "" })
            })
            .collect();
        println!("  e.g. {names:?}");
    }
}
