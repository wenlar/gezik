use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Seek, Write};
use std::path::Path;

use zip::result::ZipError;
use zip::unstable::write::FileOptionsExt; // with_deprecated_encryption (ZipCrypto write)
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, DateTime, ExtraField, ZipArchive, ZipWriter};

use crate::{Progress, fresh_dir, safe_join, sample};

/// One listed entry, read without decrypting or decompressing.
#[derive(Debug)]
pub struct ZipEntryInfo {
    pub name: String,
    pub enclosed: Option<std::path::PathBuf>,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub compressed: u64,
    pub method: CompressionMethod,
    pub encrypted: bool,
    pub dos_mtime: Option<DateTime>,
    pub ntfs_mtime: Option<u64>,
    pub unix_mtime: Option<u32>,
    pub unix_mode: Option<u32>,
}

pub fn list<R: Read + Seek>(ar: &mut ZipArchive<R>) -> zip::result::ZipResult<Vec<ZipEntryInfo>> {
    let mut out = Vec::with_capacity(ar.len());
    for i in 0..ar.len() {
        // by_index_raw: no password needed, nothing decompressed -> fine for listing.
        let f = ar.by_index_raw(i)?;
        let mut ntfs_mtime = None;
        let mut unix_mtime = None;
        for x in f.extra_data_fields() {
            match x {
                ExtraField::Ntfs(n) => ntfs_mtime = Some(n.mtime()),
                ExtraField::ExtendedTimestamp(t) => unix_mtime = t.mod_time(),
            }
        }
        out.push(ZipEntryInfo {
            name: f.name().to_owned(),
            enclosed: f.enclosed_name(),
            is_dir: f.is_dir(),
            is_symlink: f.is_symlink(),
            size: f.size(),
            compressed: f.compressed_size(),
            method: f.compression(),
            encrypted: f.encrypted(),
            dos_mtime: f.last_modified(),
            ntfs_mtime,
            unix_mtime,
            unix_mode: f.unix_mode(),
        });
    }
    Ok(out)
}

/// Extract entry `i` into `out`, reporting bytes written. `password` is used only if the
/// entry is encrypted (AES or ZipCrypto); both go through the same call.
pub fn extract_one<R: Read + Seek, W: Write>(
    ar: &mut ZipArchive<R>,
    i: usize,
    password: Option<&[u8]>,
    out: W,
    mut on_progress: impl FnMut(u64),
) -> zip::result::ZipResult<u64> {
    let mut f = match password {
        Some(pw) => ar.by_index_decrypt(i, pw)?,
        None => ar.by_index(i)?,
    };
    let mut w = Progress { inner: out, done: 0, on_progress: &mut on_progress };
    // Reading to the end also verifies CRC32 (and the AES HMAC); a bad one is an io::Error.
    let n = io::copy(&mut f, &mut w)?;
    Ok(n)
}

fn dt(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> DateTime {
    DateTime::from_date_and_time(y, mo, d, h, mi, s).unwrap()
}

pub fn run(base: &Path) {
    let dir = fresh_dir(base, "zip");
    let big = sample(3 << 20, 1);
    let small = sample(10_000, 2);
    let path = dir.join("probe.zip");

    // ---------- write ----------
    {
        let mut zw = ZipWriter::new(File::create(&path).unwrap());
        let base_opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6)) // 0..=9 for flate2 (10..=264 means zopfli)
            .last_modified_time(dt(2024, 5, 17, 13, 45, 30))
            .unix_permissions(0o644);

        zw.add_directory("docs/", base_opts.unix_permissions(0o755)).unwrap();

        zw.start_file("docs/big.bin", base_opts.large_file(true)).unwrap(); // zip64 header forced
        zw.write_all(&big).unwrap();

        zw.start_file("tool.sh", base_opts.unix_permissions(0o755)).unwrap();
        zw.write_all(b"#!/bin/sh\necho hi\n").unwrap();

        zw.start_file("secret-aes.txt", base_opts.with_aes_encryption(AesMode::Aes256, "Passw0rd"))
            .unwrap();
        zw.write_all(&small).unwrap();

        let zc = base_opts.with_deprecated_encryption(b"legacy").unwrap();
        zw.start_file("secret-zipcrypto.txt", zc).unwrap();
        zw.write_all(&small).unwrap();

        zw.start_file("bz.bin", base_opts.compression_method(CompressionMethod::Bzip2)).unwrap();
        zw.write_all(&small).unwrap();
        zw.start_file("zstd.bin", base_opts.compression_method(CompressionMethod::Zstd)).unwrap();
        zw.write_all(&small).unwrap();
        zw.start_file("xz.bin", base_opts.compression_method(CompressionMethod::Xz)).unwrap();
        zw.write_all(&small).unwrap();
        // Lzma: read-only in zip 8.6 ("LZMA isn't supported for compression").
        zw.start_file("stored.bin", base_opts.compression_method(CompressionMethod::Stored).compression_level(None)).unwrap();
        zw.write_all(&small).unwrap();

        zw.add_symlink("link-to-tool", "tool.sh", base_opts).unwrap();
        zw.start_file("../evil.txt", base_opts).unwrap(); // writer accepts it; reader must refuse
        zw.write_all(b"x").unwrap();
        zw.finish().unwrap();
    }

    // ---------- list ----------
    let mut ar = ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let entries = list(&mut ar).unwrap();
    for e in &entries {
        println!(
            "zip: {:<22} dir={} link={} {:>8}->{:>8} {:?} enc={} mode={:?} dos={:?} enclosed={:?}",
            e.name, e.is_dir, e.is_symlink, e.size, e.compressed, e.method, e.encrypted,
            e.unix_mode.map(|m| format!("{m:o}")), e.dos_mtime.map(|d| (d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second())), e.enclosed
        );
    }
    let evil = entries.iter().find(|e| e.name == "../evil.txt").unwrap();
    assert!(evil.enclosed.is_none(), "enclosed_name must refuse ..");
    assert!(entries.iter().find(|e| e.name == "link-to-tool").unwrap().is_symlink);
    assert_eq!(entries.iter().find(|e| e.name == "tool.sh").unwrap().unix_mode, Some(0o100755));
    // AES entries: compression() already reports the real codec (Deflated), encrypted() is true.
    assert!(entries.iter().find(|e| e.name == "secret-aes.txt").unwrap().encrypted);

    // ---------- extract with progress ----------
    let i_big = ar.index_for_name("docs/big.bin").unwrap();
    let mut buf = Vec::new();
    let mut ticks = 0u32;
    let n = extract_one(&mut ar, i_big, None, &mut buf, |_| ticks += 1).unwrap();
    assert_eq!(n, big.len() as u64);
    assert_eq!(buf, big);
    println!("zip: big.bin extracted, {ticks} progress ticks");

    for name in ["bz.bin", "zstd.bin", "xz.bin", "stored.bin"] {
        let i = ar.index_for_name(name).unwrap();
        let mut v = Vec::new();
        extract_one(&mut ar, i, None, &mut v, |_| {}).unwrap();
        assert_eq!(v, small, "{name}");
    }

    // ---------- passwords ----------
    let i_aes = ar.index_for_name("secret-aes.txt").unwrap();
    match ar.by_index(i_aes) {
        Err(ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED)) => {}
        other => panic!("expected PASSWORD_REQUIRED, got {:?}", other.err()),
    }
    match ar.by_index_decrypt(i_aes, b"wrong") {
        Err(ZipError::InvalidPassword) => {}
        other => panic!("expected InvalidPassword, got {:?}", other.err()),
    }
    let mut v = Vec::new();
    extract_one(&mut ar, i_aes, Some("Passw0rd".as_bytes()), &mut v, |_| {}).unwrap();
    assert_eq!(v, small);

    let i_zc = ar.index_for_name("secret-zipcrypto.txt").unwrap();
    let mut v = Vec::new();
    extract_one(&mut ar, i_zc, Some(b"legacy"), &mut v, |_| {}).unwrap();
    assert_eq!(v, small);
    // ZipCrypto: a wrong password is caught by the 1-byte check ~255/256 of the time,
    // otherwise by the CRC at end of stream.
    let wrong = ar.by_index_decrypt(i_zc, b"nope").map(|mut f| {
        let mut s = Vec::new();
        f.read_to_end(&mut s).map(|_| ())
    });
    println!("zip: zipcrypto wrong pw -> {:?}", wrong.as_ref().map(|r| r.as_ref().map_err(|e| e.to_string())).map_err(|e| e.to_string()));
    assert!(matches!(wrong, Err(_) | Ok(Err(_))));

    // ---------- safe extraction of the whole archive ----------
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    for i in 0..ar.len() {
        let (name, enclosed, is_dir, encrypted, is_symlink) = {
            let f = ar.by_index_raw(i).unwrap();
            (f.name().to_owned(), f.enclosed_name(), f.is_dir(), f.encrypted(), f.is_symlink())
        };
        let Some(rel) = enclosed else {
            println!("zip: skip unsafe {name}");
            continue;
        };
        let target = out.join(rel);
        if is_dir {
            fs::create_dir_all(&target).unwrap();
            continue;
        }
        if is_symlink {
            // Body is the link target. Gezik would recreate it (or skip on Windows w/o privilege).
            let mut t = String::new();
            ar.by_index(i).unwrap().read_to_string(&mut t).unwrap();
            println!("zip: symlink {name} -> {t}");
            continue;
        }
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let pw: Option<&[u8]> = if !encrypted {
            None
        } else if name.contains("aes") {
            Some("Passw0rd".as_bytes())
        } else {
            Some(b"legacy")
        };
        extract_one(&mut ar, i, pw, File::create(&target).unwrap(), |_| {}).unwrap();
    }
    // The crate's own extract() also sanitizes (enclosed_name) and refuses escaping symlinks.
    let out2 = dir.join("out2");
    let mut plain = ZipWriter::new(Cursor::new(Vec::new()));
    plain.start_file("a/b.txt", SimpleFileOptions::default()).unwrap();
    plain.write_all(b"b").unwrap();
    let mut plain = ZipArchive::new(plain.finish().unwrap()).unwrap();
    plain.extract(&out2).unwrap();
    assert_eq!(fs::read(out2.join("a/b.txt")).unwrap(), b"b");
    drop(ar);

    fs::copy(&path, dir.join("before-append.zip")).unwrap();
    // ---------- add to an existing archive in place ----------
    {
        let f = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        let mut zw = ZipWriter::new_append(f).unwrap(); // keeps existing entries, rewrites the central dir
        zw.start_file("added-later.txt", SimpleFileOptions::default()).unwrap();
        zw.write_all(b"appended").unwrap();
        zw.finish().unwrap();
    }
    let mut ar = ZipArchive::new(File::open(&path).unwrap()).unwrap();
    assert!(ar.index_for_name("added-later.txt").is_some());
    assert!(ar.index_for_name("secret-aes.txt").is_some());

    // ---------- copy entries raw into a new archive (no recompress, keeps encryption) ----------
    let copy_path = dir.join("copy.zip");
    {
        let mut zw = ZipWriter::new(File::create(&copy_path).unwrap());
        for i in 0..ar.len() {
            let f = ar.by_index_raw(i).unwrap();
            if f.name() == "../evil.txt" {
                continue;
            }
            zw.raw_copy_file(f).unwrap(); // or raw_copy_file_rename(f, "new/name")
        }
        zw.start_file("new.txt", SimpleFileOptions::default()).unwrap();
        zw.write_all(b"new").unwrap();
        zw.finish().unwrap();
    }
    let mut copy = ZipArchive::new(File::open(&copy_path).unwrap()).unwrap();
    let i = copy.index_for_name("secret-aes.txt").unwrap();
    let mut v = Vec::new();
    extract_one(&mut copy, i, Some("Passw0rd".as_bytes()), &mut v, |_| {}).unwrap();
    assert_eq!(v, small);

    // ---------- parallel deflate: each worker builds a one-entry zip in memory, main merges ----------
    let par_path = dir.join("parallel.zip");
    {
        let inputs: Vec<(String, Vec<u8>)> =
            (0..8).map(|k| (format!("part{k}.bin"), sample(1 << 20, 10 + k))).collect();
        let parts: Vec<Vec<u8>> = std::thread::scope(|s| {
            let hs: Vec<_> = inputs
                .iter()
                .map(|(name, data)| {
                    s.spawn(move || {
                        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
                        let o = SimpleFileOptions::default()
                            .compression_method(CompressionMethod::Deflated)
                            .compression_level(Some(6))
                            .last_modified_time(dt(2024, 1, 2, 3, 4, 6));
                        w.start_file(name.as_str(), o).unwrap();
                        w.write_all(data).unwrap();
                        w.finish().unwrap().into_inner()
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut zw = ZipWriter::new(File::create(&par_path).unwrap());
        for p in parts {
            // merge_archive copies the compressed bytes and the headers as-is.
            zw.merge_archive(ZipArchive::new(Cursor::new(p)).unwrap()).unwrap();
        }
        zw.finish().unwrap();
        let mut back = ZipArchive::new(File::open(&par_path).unwrap()).unwrap();
        for (name, data) in &inputs {
            let mut v = Vec::new();
            back.by_name(name).unwrap().read_to_end(&mut v).unwrap();
            assert_eq!(&v, data);
        }
    }
    println!("zip: OK ({})", path.display());
}

/// Read archives made by 7-Zip (Deflate64, split .zip.001) to check what the crate accepts.
pub fn read_external(path: &Path, password: Option<&[u8]>) -> Result<Vec<(String, u64)>, String> {
    let mut ar = ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for i in 0..ar.len() {
        let mut v = Vec::new();
        let name = ar.by_index_raw(i).map_err(|e| e.to_string())?.name().to_owned();
        let n = extract_one(&mut ar, i, password, &mut v, |_| {}).map_err(|e| format!("{name}: {e}"))?;
        out.push((name, n));
    }
    Ok(out)
}

#[allow(dead_code)]
fn _safe(dest: &Path, n: &str) -> Option<std::path::PathBuf> {
    safe_join(dest, n)
}
