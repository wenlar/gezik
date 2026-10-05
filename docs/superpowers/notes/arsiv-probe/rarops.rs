use std::path::Path;

use unrar_ng::error::{Code, UnrarError};
use unrar_ng::{Archive, ExtractEvent, ExtractStatus};

use crate::fresh_dir;

/// List entries. With header encryption the listing itself needs the password.
pub fn list(path: &Path, password: Option<&str>) -> Result<Vec<String>, UnrarError> {
    let ar = match password {
        Some(pw) => Archive::with_password(path, pw),
        None => Archive::new(path),
    };
    let mut out = Vec::new();
    let open = ar.open_for_listing()?; // Iterator<Item = Result<FileHeader, UnrarError>>
    println!(
        "rar: {} solid={} enc_headers={} volume={:?}",
        path.file_name().unwrap().to_string_lossy(),
        open.is_solid(),
        open.has_encrypted_headers(),
        open.volume_info()
    );
    for h in open {
        let h = h?;
        out.push(format!(
            "{} size={} dir={} enc={} split={} attr={:#x} dos_time={:#x} crc={:08x}",
            h.filename.display(), h.unpacked_size, h.is_directory(), h.is_encrypted(), h.is_split(),
            h.file_attr, h.file_time, h.file_crc
        ));
    }
    Ok(out)
}

/// Extract entry by entry under `dest` (one progress tick per entry; UnRAR's DLL API has no
/// per-byte callback in the safe wrapper). Returns the extracted names.
pub fn extract(path: &Path, password: Option<&str>, dest: &Path) -> Result<Vec<String>, UnrarError> {
    let ar = match password {
        Some(pw) => Archive::with_password(path, pw),
        None => Archive::new(path),
    };
    let mut done = Vec::new();
    // as_first_part(): if the user picked part3.rar, start from part1 (volumes are found by name)
    let mut open = ar.as_first_part().open_for_processing()?;
    while let Some(header) = open.read_header()? {
        let e = header.entry();
        let name = e.filename.display().to_string();
        open = if e.is_file() {
            done.push(name);
            // UnRAR builds dest/<entry path> itself and strips `..` / drive letters.
            header.extract_with_base(dest)?
        } else {
            header.skip()?
        };
    }
    Ok(done)
}

pub fn run(base: &Path) {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/rar");
    let dir = fresh_dir(base, "rar");

    for f in ["solid.rar", "unicode.rar", "crypted.rar"] {
        for l in list(&data.join(f), None).unwrap() {
            println!("rar:   {l}");
        }
    }
    // Normal listing walks all volumes; with part2 missing it fails with EOpen.
    println!("rar: list part1 w/o part2 -> {:?}", list(&data.join("archive.part1.rar"), None).map_err(|e| e.code));
    // open_for_listing_split lists only this volume's headers (split entries reported per part).
    let one = Archive::new(&data.join("archive.part1.rar")).open_for_listing_split().unwrap();
    for h in one {
        // even the split listing ends with Err(EOpen) when the next part is missing
        let h = match h {
            Ok(h) => h,
            Err(e) => {
                println!("rar:   split-list ends with {:?}", e.code);
                break;
            }
        };
        println!("rar:   split-list {} size={} split_after={}", h.filename.display(), h.unpacked_size, h.is_split_after());
    }
    // header-encrypted (rar -hp): no password -> MissingPassword at open
    match list(&data.join("comment-hpw-password.rar"), None) {
        Err(UnrarError { code, .. }) => println!("rar: -hp archive without pw -> {code:?}"),
        Ok(v) => println!("rar: -hp archive listed without pw?? {v:?}"),
    }
    println!("rar: -hp (RAR5) wrong pw -> {:?}", list(&data.join("comment-hpw-password.rar"), Some("wrong")).map_err(|e| e.code));
    for l in list(&data.join("comment-hpw-password.rar"), Some("password")).unwrap() {
        println!("rar:   {l}");
    }

    // extract with and without password
    let out = dir.join("crypted");
    match extract(&data.join("crypted.rar"), None, &out) {
        Err(e) => println!("rar: crypted.rar no pw -> {:?}", e.code),
        Ok(v) => println!("rar: crypted.rar no pw extracted?? {v:?}"),
    }
    match extract(&data.join("crypted.rar"), Some("wrong"), &out) {
        Err(e) => println!("rar: crypted.rar wrong pw -> {:?}", e.code),
        Ok(v) => println!("rar: crypted.rar wrong pw extracted?? {v:?}"),
    }
    let got = extract(&data.join("crypted.rar"), Some("unrar"), &out).unwrap();
    println!("rar: crypted.rar ok -> {got:?}");
    // RAR5 (-hp) data with a wrong password
    println!("rar: RAR5 wrong pw extract -> {:?}", extract(&data.join("comment-hpw-password.rar"), Some("wrong"), &dir.join("x")).map_err(|e| e.code));
    let got = extract(&data.join("solid.rar"), None, &dir.join("solid")).unwrap();
    println!("rar: solid.rar -> {got:?}");

    // multi-volume with a missing part2: the split entry fails, earlier ones succeed
    match extract(&data.join("archive.part1.rar"), None, &dir.join("multi")) {
        Err(e) => println!("rar: part1 without part2 -> {:?} ({:?})", e.code, e.when),
        Ok(v) => println!("rar: part1 -> {v:?}"),
    }

    // batch API with callbacks (unrar-ng only): Start/Ok/Err per file, can cancel
    let status = Archive::with_password(&data.join("crypted.rar"), "unrar")
        .open_for_processing()
        .unwrap()
        .extract_all_with_callback(dir.join("batch"), |ev| {
            match ev {
                ExtractEvent::Start { filename, size } => println!("rar: start {} ({size} B)", filename.display()),
                ExtractEvent::Ok { filename, .. } => println!("rar: ok {}", filename.display()),
                ExtractEvent::Err { filename, error_code } => println!("rar: err {} {error_code}", filename.display()),
                _ => {}
            }
            true // false = cancel -> Ok(ExtractStatus::Cancelled)
        })
        .unwrap();
    assert_eq!(status, ExtractStatus::Completed);
    let _ = Code::Success;
    println!("rar: OK");
}
