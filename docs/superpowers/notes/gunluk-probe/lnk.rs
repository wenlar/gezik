//! Q1: .lnk shortcuts through IShellLinkW + IPersistFile, on a worker thread.
#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    let base = std::env::temp_dir().join("probe7 lnk Ğüşİ 日本 🙂");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("hedef klasör ş")).unwrap();
    std::fs::write(base.join("belge ç 日本.txt"), "merhaba").unwrap();
    // A target past MAX_PATH.
    let mut long = base.clone();
    for i in 0..6 {
        long.push(format!("uzun-klasör-adı-{i}-{}", "x".repeat(40)));
    }
    std::fs::create_dir_all(&long).unwrap();
    let long_file = long.join("dosya.txt");
    std::fs::write(&long_file, "x").unwrap();
    println!("long target length: {} chars", long_file.as_os_str().len());

    let jobs: Vec<(PathBuf, PathBuf)> = vec![
        (base.join("belge ç 日本.txt"), base.join("belge ç 日本.txt - Kısayol.lnk")),
        (base.join("hedef klasör ş"), base.join("hedef klasör ş - Kısayol.lnk")),
        (long_file.clone(), base.join("uzun.lnk")),
        (base.join("yok.txt"), base.join("yok.lnk")),
        (PathBuf::from(format!(r"\?\{}", long_file.display())), base.join("uzun-verbatim.lnk")),
    ];
    // The worker thread: its own COM apartment.
    let handle = std::thread::spawn(move || {
        let _com = lnk::Com::init().expect("CoInitializeEx");
        for (target, link) in &jobs {
            let t = std::time::Instant::now();
            let made = lnk::create(target, link, Some("probe7 açıklama"));
            println!("create {:?} -> {:?} ({:?})", link.file_name().unwrap(), made, t.elapsed());
            if made.is_ok() {
                let read = lnk::read(link);
                println!("  read back: {read:?}");
                if let Ok(info) = read {
                    println!("  same target: {}", info.target == *target);
                }
            }
        }
    });
    handle.join().unwrap();
    println!("left in {}", base.display());
}

#[cfg(windows)]
mod lnk {
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};

    use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, SLR_NO_UI, SLR_NOSEARCH, SLR_NOTRACK, SLR_NOUPDATE, ShellLink};
    use windows::core::{HSTRING, Interface};

    /// COM for this thread until dropped (only if this call started it).
    pub struct Com(bool);
    impl Com {
        pub fn init() -> windows::core::Result<Com> {
            // S_OK: started here; S_FALSE: already on (same model); RPC_E_CHANGED_MODE: an MTA thread.
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
            println!("CoInitializeEx: {hr:?}");
            hr.ok()?;
            Ok(Com(true))
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub fn create(target: &Path, link: &Path, description: Option<&str>) -> windows::core::Result<()> {
        unsafe {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            shell_link.SetPath(&HSTRING::from(target.as_os_str()))?;
            if let Some(dir) = target.parent() {
                shell_link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
            }
            if let Some(text) = description {
                shell_link.SetDescription(&HSTRING::from(text))?;
            }
            let file: IPersistFile = shell_link.cast()?;
            file.Save(&HSTRING::from(link.as_os_str()), true)
        }
    }

    #[derive(Debug)]
    pub struct LinkInfo {
        pub target: PathBuf,
        pub working_dir: PathBuf,
        pub arguments: String,
        pub resolved: bool,
    }

    pub fn read(link: &Path) -> windows::core::Result<LinkInfo> {
        unsafe {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let file: IPersistFile = shell_link.cast()?;
            file.Load(&HSTRING::from(link.as_os_str()), STGM_READ)?;
            // No UI, no search, no tracking, no rewrite of the .lnk: only whether it resolves.
            // SLR_NO_UI's high word is the timeout in ms (here 0 = default 3 s); a missing
            // target still gives S_OK/S_FALSE, so Gezik checks the target itself.
            let resolve = shell_link.Resolve(
                windows::Win32::Foundation::HWND::default(),
                (SLR_NO_UI.0 | SLR_NOSEARCH.0 | SLR_NOTRACK.0 | SLR_NOUPDATE.0) as u32,
            );
            let resolved = resolve.is_ok();
            let mut buf = vec![0u16; 32768];
            let mut find = WIN32_FIND_DATAW::default();
            shell_link.GetPath(&mut buf, &mut find, SLGP_RAWPATH.0 as u32)?;
            let target = PathBuf::from(std::ffi::OsString::from_wide(until_nul(&buf)));
            println!("  resolve: {resolve:?}, find attrs: {:#x} (0x10 = folder)", find.dwFileAttributes);
            let mut dir = vec![0u16; 32768];
            shell_link.GetWorkingDirectory(&mut dir)?;
            let mut args = vec![0u16; 32768];
            shell_link.GetArguments(&mut args)?;
            Ok(LinkInfo {
                target,
                working_dir: PathBuf::from(std::ffi::OsString::from_wide(until_nul(&dir))),
                arguments: String::from_utf16_lossy(until_nul(&args)),
                resolved,
            })
        }
    }

    fn until_nul(buf: &[u16]) -> &[u16] {
        &buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]
    }
}
