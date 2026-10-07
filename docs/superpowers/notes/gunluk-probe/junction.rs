//! Q2: junctions without admin (FSCTL_SET_REPARSE_POINT), symlink permission, safe removal.
#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::path::Path;
    println!("elevated: {}", junction::elevated());
    println!("SeCreateSymbolicLinkPrivilege in token: {:?}", junction::has_symlink_privilege());

    let base = std::env::temp_dir().join("probe7 junction Ğş 日本");
    let _ = std::fs::remove_dir_all(&base);
    let target = base.join("hedef ç");
    std::fs::create_dir_all(target.join("alt")).unwrap();
    std::fs::write(target.join("alt").join("dosya ü.txt"), "içerik").unwrap();

    // 1. Junction on the same volume.
    let link = base.join("bağlantı İ");
    let t = std::time::Instant::now();
    junction::create(&link, &target).unwrap();
    println!("junction created in {:?}", t.elapsed());
    println!("read back: {:?}", junction::read(&link));
    println!("through it: {:?}", std::fs::read_to_string(link.join("alt").join("dosya ü.txt")));
    let meta = std::fs::symlink_metadata(&link).unwrap();
    println!("symlink_metadata: is_symlink={} is_dir={}", meta.file_type().is_symlink(), meta.is_dir());
    {
        use std::os::windows::fs::MetadataExt;
        println!("attrs {:#x} (0x400 = reparse point)", meta.file_attributes());
    }

    // 2. Safe removal: only if it is still the junction Gezik made.
    let t = std::time::Instant::now();
    junction::remove_if_points_to(&link, &target).unwrap();
    println!("removed in {:?}; link gone: {}, target file kept: {}", t.elapsed(), !link.exists(),
        target.join("alt").join("dosya ü.txt").exists());

    // 3. gezik_platform::fs::delete on a junction (FILE_FLAG_OPEN_REPARSE_POINT): link only.
    junction::create(&link, &target).unwrap();
    gezik_platform::fs::delete(&link).unwrap();
    println!("gezik delete: link gone: {}, target kept: {}", !link.exists(), target.join("alt").join("dosya ü.txt").exists());

    // 4. std::fs::remove_dir_all on a folder holding a junction: does not walk into it.
    let holder = base.join("tutucu");
    std::fs::create_dir(&holder).unwrap();
    junction::create(&holder.join("j"), &target).unwrap();
    std::fs::remove_dir_all(&holder).unwrap();
    println!("remove_dir_all(holder): target kept: {}", target.join("alt").join("dosya ü.txt").exists());

    // 5. Recycle Bin (gezik_platform::fs::trash) of a junction.
    junction::create(&link, &target).unwrap();
    match gezik_platform::fs::trash(&link) {
        Ok(at) => println!("trash: ok {:?}; target kept: {}", at, target.join("alt").join("dosya ü.txt").exists()),
        Err(e) => println!("trash: {e}"),
    }

    // 6. Removal refuses a junction that now points elsewhere, and a plain folder.
    let other = base.join("başka");
    std::fs::create_dir(&other).unwrap();
    junction::create(&link.with_extension("2"), &other).unwrap();
    println!("refuse other target: {:?}", junction::remove_if_points_to(&link.with_extension("2"), &target));
    println!("refuse plain folder: {:?}", junction::remove_if_points_to(&other, &other));

    // 7. Cross-volume target (D:), a missing target, a relative target, a file target, a UNC target.
    let d = Path::new(r"D:\tmp\probe7-junction-target");
    let _ = std::fs::create_dir_all(d);
    let cross = base.join("cross");
    println!("cross-volume: {:?}; readable: {:?}", junction::create(&cross, d), std::fs::read_dir(&cross).map(|r| r.count()));
    let _ = std::fs::remove_dir(&cross);
    let _ = std::fs::remove_dir(d);
    println!("missing target: {:?}", junction::create(&base.join("missing"), &base.join("yok")));
    println!("  exists() through it: {}", base.join("missing").exists());
    let _ = std::fs::remove_dir(base.join("missing"));
    println!("relative target: {:?}", junction::create(&base.join("rel"), Path::new("hedef ç")));
    std::fs::write(base.join("f.txt"), "x").unwrap();
    println!("file target: {:?}", junction::create(&base.join("tofile"), &base.join("f.txt")));
    println!("  read_dir through it: {:?}", std::fs::read_dir(base.join("tofile")).map(|r| r.count()));
    let _ = std::fs::remove_dir(base.join("tofile"));
    println!("UNC target: {:?}", junction::create(&base.join("unc"), Path::new(r"\\localhost\C$\Windows")));
    println!("  read_dir through it: {:?}", std::fs::read_dir(base.join("unc")).map(|r| r.count()));
    let _ = std::fs::remove_dir(base.join("unc"));
    // Long target (> MAX_PATH).
    let mut long = base.clone();
    for i in 0..6 {
        long.push(format!("uzun-{i}-{}", "y".repeat(45)));
    }
    std::fs::create_dir_all(&long).unwrap();
    println!("long target ({} chars): {:?}; readable: {:?}", long.as_os_str().len(),
        junction::create(&base.join("long"), &long), std::fs::read_dir(base.join("long")).map(|r| r.count()));

    // 8. Symlinks: std uses SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE itself.
    let r = std::os::windows::fs::symlink_dir(&target, base.join("sym-dir"));
    println!("std symlink_dir: {:?}", r.as_ref().map_err(|e| (e.raw_os_error(), e.to_string())));
    let r = std::os::windows::fs::symlink_file(base.join("f.txt"), base.join("sym-file"));
    println!("std symlink_file: {:?}", r.as_ref().map_err(|e| (e.raw_os_error(), e.to_string())));
    println!("can_symlink() trial: {}", junction::can_symlink());
    let _ = std::fs::remove_dir_all(&base);
}

#[cfg(windows)]
mod junction {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
        TokenElevation, TokenPrivileges,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING, RemoveDirectoryW,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{FSCTL_GET_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};
    use windows::Win32::System::SystemServices::{IO_REPARSE_TAG_MOUNT_POINT, IO_REPARSE_TAG_SYMLINK};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::core::{HSTRING, w};

    const HEADER: usize = 8; // ReparseTag u32, ReparseDataLength u16, Reserved u16
    const MOUNT_POINT_HEADER: usize = 8; // four u16 offsets/lengths
    const MAX_REPARSE: usize = 16 * 1024; // MAXIMUM_REPARSE_DATA_BUFFER_SIZE

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().collect()
    }

    fn open(path: &Path, access: u32) -> io::Result<HANDLE> {
        unsafe {
            CreateFileW(
                &HSTRING::from(path.as_os_str()),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
    }

    /// The mount-point reparse buffer: SubstituteName `\??\C:\x` then PrintName `C:\x`, each
    /// NUL-terminated (the lengths leave the NULs out).
    pub fn mount_point_buffer(target: &Path) -> io::Result<Vec<u8>> {
        // Absolute, no `\\?\` and not a UNC path: junctions point at local volumes only.
        let text = target.as_os_str().to_string_lossy();
        let plain = text.strip_prefix(r"\\?\").unwrap_or(&text);
        if plain.starts_with(r"\\") || !target.is_absolute() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a junction needs an absolute local path"));
        }
        let print: Vec<u16> = plain.encode_utf16().collect();
        let substitute: Vec<u16> = format!(r"\??\{plain}").encode_utf16().collect();
        let (sub_bytes, print_bytes) = (substitute.len() * 2, print.len() * 2);
        let data_len = MOUNT_POINT_HEADER + sub_bytes + 2 + print_bytes + 2;
        if HEADER + data_len > MAX_REPARSE {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "target path too long for a junction"));
        }
        let mut b = Vec::with_capacity(HEADER + data_len);
        b.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        b.extend_from_slice(&(data_len as u16).to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // SubstituteNameOffset
        b.extend_from_slice(&(sub_bytes as u16).to_le_bytes());
        b.extend_from_slice(&((sub_bytes + 2) as u16).to_le_bytes()); // PrintNameOffset
        b.extend_from_slice(&(print_bytes as u16).to_le_bytes());
        for u in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) {
            b.extend_from_slice(&u.to_le_bytes());
        }
        Ok(b)
    }

    /// Makes `link` (must not exist) a junction to `target`. No admin, no Developer Mode.
    pub fn create(link: &Path, target: &Path) -> io::Result<()> {
        let buffer = mount_point_buffer(target)?;
        std::fs::create_dir(link)?;
        let result = (|| {
            let handle = open(link, GENERIC_WRITE.0)?;
            let done = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_SET_REPARSE_POINT,
                    Some(buffer.as_ptr() as *const c_void),
                    buffer.len() as u32,
                    None,
                    0,
                    None,
                    None,
                )
            };
            let _ = unsafe { CloseHandle(handle) };
            done.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir(link);
        }
        result
    }

    #[derive(Debug, PartialEq)]
    pub enum Reparse {
        Junction(PathBuf),
        Symlink(PathBuf),
        Other(u32),
    }

    pub fn read(path: &Path) -> io::Result<Reparse> {
        let handle = open(path, GENERIC_READ.0)?;
        let mut buf = vec![0u8; MAX_REPARSE];
        let mut got = 0u32;
        let done = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_GET_REPARSE_POINT,
                None,
                0,
                Some(buf.as_mut_ptr() as *mut c_void),
                buf.len() as u32,
                Some(&mut got),
                None,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        done.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
        let u16_at = |i: usize| u16::from_le_bytes([buf[i], buf[i + 1]]) as usize;
        let tag = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        // Symlinks have a u32 Flags after the four u16s.
        let (names_at, kind) = match tag {
            IO_REPARSE_TAG_MOUNT_POINT => (HEADER + 8, 0),
            IO_REPARSE_TAG_SYMLINK => (HEADER + 12, 1),
            other => return Ok(Reparse::Other(other)),
        };
        let (off, len) = (u16_at(HEADER), u16_at(HEADER + 2));
        let units: Vec<u16> =
            buf[names_at + off..names_at + off + len].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let mut name = std::ffi::OsString::from_wide(&units).to_string_lossy().into_owned();
        if let Some(rest) = name.strip_prefix(r"\??\") {
            name = rest.to_owned();
        }
        Ok(if kind == 0 { Reparse::Junction(name.into()) } else { Reparse::Symlink(name.into()) })
    }

    /// Undo of "make junction": removes `link` only if it is still a junction to `target`.
    /// RemoveDirectoryW on a reparse point removes the point, never what it points to.
    pub fn remove_if_points_to(link: &Path, target: &Path) -> io::Result<()> {
        match read(link)? {
            Reparse::Junction(to) if same(&to, target) => {}
            other => return Err(io::Error::other(format!("not the junction made: {other:?}"))),
        }
        unsafe { RemoveDirectoryW(&HSTRING::from(link.as_os_str())) }
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
    }

    fn same(a: &Path, b: &Path) -> bool {
        let strip = |p: &Path| {
            let s = p.as_os_str().to_string_lossy().into_owned();
            s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s).to_lowercase()
        };
        strip(a) == strip(b)
    }

    pub fn elevated() -> bool {
        unsafe {
            let mut token = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
                return false;
            }
            let mut e = TOKEN_ELEVATION::default();
            let mut len = 0;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut e as *mut _ as *mut c_void),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(token);
            ok && e.TokenIsElevated != 0
        }
    }

    /// Whether the token holds SeCreateSymbolicLinkPrivilege (elevated admins, or users given it
    /// by policy). Developer Mode does not add it; it lets the unprivileged flag through instead.
    pub fn has_symlink_privilege() -> io::Result<bool> {
        unsafe {
            let mut luid = Default::default();
            LookupPrivilegeValueW(None, w!("SeCreateSymbolicLinkPrivilege"), &mut luid).map_err(io::Error::other)?;
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(io::Error::other)?;
            let mut buf = vec![0u8; 4096];
            let mut len = 0;
            let r = GetTokenInformation(
                token,
                TokenPrivileges,
                Some(buf.as_mut_ptr() as *mut c_void),
                buf.len() as u32,
                &mut len,
            );
            let _ = CloseHandle(token);
            r.map_err(io::Error::other)?;
            let privs = &*(buf.as_ptr() as *const TOKEN_PRIVILEGES);
            let all = std::slice::from_raw_parts(
                privs.Privileges.as_ptr() as *const LUID_AND_ATTRIBUTES,
                privs.PrivilegeCount as usize,
            );
            Ok(all.iter().any(|p| p.Luid.LowPart == luid.LowPart && p.Luid.HighPart == luid.HighPart))
        }
    }

    /// The honest answer: try once in the temp folder (std passes
    /// SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE, so Developer Mode counts too).
    pub fn can_symlink() -> bool {
        let dir = std::env::temp_dir();
        let link = dir.join(format!("gezik-symlink-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&link);
        let ok = std::os::windows::fs::symlink_file(dir.join("gezik-nonexistent-target"), &link).is_ok();
        let _ = std::fs::remove_file(&link);
        ok
    }
}
