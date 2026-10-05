# Archive crate API digest for Gezik 5b

Date: 2026-10-05. Toolchain: rustc 1.99.0, x86_64-pc-windows-msvc, edition 2024.
Probe crate: `scratchpad/archprobe` (functional probes) and `scratchpad/archprobe/sizeprobe` (size stages).
Every snippet below was copied from code that compiles and runs in the probe crate. The probes write archives, read them back, assert byte equality, and cross-check with 7-Zip 25 (`7z t`) and GNU tar/xz/gzip/bzip2.

How to rerun the probes:
```
cd archprobe
cargo build --release --bins
target/release/probe            # zip, 7z, tar, rar, misc, detect: each prints "...: OK"
./make-external.sh              # makes archives with 7-Zip, GNU tar and makecab (run it after probe, which wipes work/)
target/release/zipext; target/release/szext; target/release/tarext; target/release/miscext
```
Files: `src/zipops.rs`, `src/sevenzops.rs`, `src/tarops.rs`, `src/rarops.rs`, `src/miscops.rs`, `src/detect.rs`, `src/lib.rs` (`safe_join`, `Progress`, `MultiFileReader`).

---

## 0. Recommended Cargo.toml (checked in the probe and sizeprobe crates)

```toml
zip = { version = "8.6.0", default-features = false, features = ["aes-crypto", "deflate-flate2", "deflate64", "bzip2"] }
sevenz-rust2 = { version = "0.23.0", default-features = false, features = ["aes256", "compress", "bzip2", "ppmd"] }
tar = { version = "0.4.46", default-features = false }
flate2 = "1.1.10"                       # default = rust_backend (miniz_oxide); see the flate2 section
lzma-rust2 = { version = "0.21.0", default-features = false, features = ["std", "encoder", "optimization", "xz"] }
bzip2 = "0.6.1"                         # default backend = libbz2-rs-sys (pure Rust)
ruzstd = { version = "0.9.0", default-features = false, features = ["std"] }
unrar-ng = { version = "0.7.7", default-features = false }   # C++ (UnRAR 7.21), see section 4
cab = "0.6.0"
hadris-iso = { version = "2.5.0", default-features = false, features = ["std", "sync", "read"] }
cpio = "0.4.1"
ar = "0.9.0"
```

**MSRV:** `sevenz-rust2` 0.21.0 and later all declare `rust-version = 1.93`. Gezik declares `rust-version = "1.92"` in one crate, so that crate must move to 1.93. The installed toolchain is 1.99. The last version with MSRV 1.85 is sevenz-rust2 0.20.2 (Feb 2026, depends on lzma-rust2 ^0.16). Other MSRVs: zip 8.6 = 1.88, hadris-iso = 1.88, ruzstd = 1.87, lzma-rust2 = 1.85, unrar-ng = 1.85, bzip2 = 1.82, flate2 = 1.67, tar = 1.63.

**Duplicate lzma-rust2:** zip 8.6.0 depends on `lzma-rust2 ^0.16.1` (zip 9.0.0-pre3 uses ^0.18), while sevenz-rust2 0.23 uses ^0.21. They cannot share it. Keep zip's `lzma` and `xz` features off: LZMA/XZ-in-zip is rare, and turning them on adds 150 KB for a second lzma-rust2 copy. If zip-LZMA support is needed, extract that entry raw (`by_index_raw` + `take`) and decode it with lzma-rust2 0.21 ourselves.

**zip's `zstd` feature is C** (`zstd` 0.13 → `zstd-sys`, built with cc). It adds 448 KB. zip has no ruzstd option, so leave it off.

`zip` 9.0.0-pre3 is the newest on crates.io but is a pre-release. Use 8.6.0, the latest stable (2026-04-25).

---

## 1. zip 8.6.0

Features: default = aes-crypto, bzip2, deflate64, deflate (= zopfli + flate2/zlib-rs), lzma, ppmd, time, zstd, xz. The minimal set is above. Notes on each:
- `deflate-flate2` uses whatever flate2 backend the build turns on. Gezik already gets flate2 with `rust_backend` (miniz) through resvg/usvg. Standalone builds need `deflate-flate2-zlib-rs` or their own flate2 dependency.
- `deflate64` = `deflate64` crate (pure Rust, decode only).
- `bzip2` = bzip2 0.6 with libbz2-rs-sys (pure Rust, read and write).
- `aes-crypto` = RustCrypto aes/hmac/pbkdf2/sha1 + getrandom.
- `time` only changes `DateTime::default_for_write()` (without it the default is 1980-01-01). We always set the time ourselves, so leave it off.
- `ppmd` (pure Rust) is optional. 7-Zip can write PPMd zips (`-mm=PPMd`).

Methods and directions in 8.6: Stored, Deflated, Bzip2, Zstd and Xz can be read and written. **Lzma is read-only** (writing fails with `"LZMA isn't supported for compression"`). **Deflate64 is read-only.** A compression level on `Stored` fails with `UnsupportedArchive("Unsupported compression level")`, so set `.compression_level(None)` for it.

### Imports
```rust
use zip::result::ZipError;
use zip::unstable::write::FileOptionsExt; // with_deprecated_encryption (ZipCrypto write)
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, DateTime, ExtraField, ZipArchive, ZipWriter};
```

### List entries without a password or decompression
```rust
pub fn list<R: Read + Seek>(ar: &mut ZipArchive<R>) -> zip::result::ZipResult<Vec<ZipEntryInfo>> {
    let mut out = Vec::with_capacity(ar.len());
    for i in 0..ar.len() {
        // by_index_raw: no password needed, nothing decompressed -> fine for listing.
        let f = ar.by_index_raw(i)?;
        let mut ntfs_mtime = None;
        let mut unix_mtime = None;
        for x in f.extra_data_fields() {
            match x {
                ExtraField::Ntfs(n) => ntfs_mtime = Some(n.mtime()),          // u64 FILETIME (100ns since 1601)
                ExtraField::ExtendedTimestamp(t) => unix_mtime = t.mod_time(), // Option<u32> unix secs
            }
        }
        out.push(ZipEntryInfo {
            name: f.name().to_owned(),          // raw name (may be "../evil.txt")
            enclosed: f.enclosed_name(),        // Option<PathBuf>: None for "..", absolute, NUL
            is_dir: f.is_dir(),                 // name ends with '/'
            is_symlink: f.is_symlink(),         // unix_mode has S_IFLNK
            size: f.size(),
            compressed: f.compressed_size(),
            method: f.compression(),            // AES entries report the real codec (Deflated)
            encrypted: f.encrypted(),
            dos_mtime: f.last_modified(),       // Option<DateTime>, local time, 2 s resolution
            ntfs_mtime,
            unix_mtime,
            unix_mode: f.unix_mode(),           // Some(0o100755) etc. (full st_mode incl. type bits)
        });
    }
    Ok(out)
}
```
`DateTime` has the getters `year() month() day() hour() minute() second()`. Construct one with `DateTime::from_date_and_time(y, mo, d, h, mi, s) -> Result<DateTime, DateTimeRangeError>` (years 1980..=2107).
Other helpers: `ar.index_for_name(&str) -> Option<usize>`, `ar.name_for_index(i)`, `ar.file_names()`, `ar.by_name(..)`.

### Extract one entry with progress (AES and ZipCrypto use the same call)
```rust
pub struct Progress<W, F: FnMut(u64)> { pub inner: W, pub done: u64, pub on_progress: F }
impl<W: Write, F: FnMut(u64)> Write for Progress<W, F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?; self.done += n as u64; (self.on_progress)(self.done); Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> { self.inner.flush() }
}

pub fn extract_one<R: Read + Seek, W: Write>(
    ar: &mut ZipArchive<R>, i: usize, password: Option<&[u8]>, out: W, mut on_progress: impl FnMut(u64),
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
```
Password errors (verified):
- `by_index` on an encrypted entry returns `Err(ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED))`.
- AES with a wrong password returns `Err(ZipError::InvalidPassword)` from `by_index_decrypt`.
- ZipCrypto with a wrong password usually returns `InvalidPassword` (1-byte check, about 255/256 of the time). Otherwise it fails as an `io::Error` on the CRC at the end of the stream.
- Options form: `ar.by_index_with_options(i, ZipReadOptions::new().password(Some(pw)))`.

### Safe extraction loop (the zip-slip check is `enclosed_name`)
```rust
for i in 0..ar.len() {
    let (name, enclosed, is_dir, encrypted, is_symlink) = {
        let f = ar.by_index_raw(i).unwrap();
        (f.name().to_owned(), f.enclosed_name(), f.is_dir(), f.encrypted(), f.is_symlink())
    };
    let Some(rel) = enclosed else { continue };        // "../evil.txt" -> None
    let target = out.join(rel);
    if is_dir { fs::create_dir_all(&target).unwrap(); continue; }
    if is_symlink {
        let mut t = String::new();                    // body = link target
        ar.by_index(i).unwrap().read_to_string(&mut t).unwrap();
        continue;                                      // Gezik decides: recreate / skip on Windows
    }
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    extract_one(&mut ar, i, pw, File::create(&target).unwrap(), |_| {}).unwrap();
}
```
`ZipArchive::extract(dir)` also exists. It sanitizes through `enclosed_name`, creates symlinks only when their target stays inside the destination, and has no progress callback. On Windows without the symlink privilege it fails (see the tar section).

### Write: Deflate level, AES-256, ZipCrypto, zip64, time, permissions, directory, symlink
```rust
let mut zw = ZipWriter::new(File::create(&path).unwrap());
let base_opts = SimpleFileOptions::default()
    .compression_method(CompressionMethod::Deflated)
    .compression_level(Some(6)) // 0..=9 for flate2 (10..=264 means zopfli)
    .last_modified_time(dt(2024, 5, 17, 13, 45, 30))
    .unix_permissions(0o644);

zw.add_directory("docs/", base_opts.unix_permissions(0o755)).unwrap();

zw.start_file("docs/big.bin", base_opts.large_file(true)).unwrap(); // zip64 header forced
zw.write_all(&big).unwrap();

zw.start_file("secret-aes.txt", base_opts.with_aes_encryption(AesMode::Aes256, "Passw0rd")).unwrap();
zw.write_all(&small).unwrap();

let zc = base_opts.with_deprecated_encryption(b"legacy").unwrap(); // ZipCrypto (FileOptionsExt)
zw.start_file("secret-zipcrypto.txt", zc).unwrap();
zw.write_all(&small).unwrap();

zw.start_file("bz.bin", base_opts.compression_method(CompressionMethod::Bzip2)).unwrap();
zw.write_all(&small).unwrap();
zw.start_file("stored.bin", base_opts.compression_method(CompressionMethod::Stored).compression_level(None)).unwrap();
zw.write_all(&small).unwrap();

zw.add_symlink("link-to-tool", "tool.sh", base_opts).unwrap();
zw.finish().unwrap();
```
- `large_file(false)` (the default) makes an entry over 4 GiB fail with an io error. Either set `large_file(true)` for big inputs or build the writer with `ZipWriter::new(w).set_auto_large_file()`.
- `unix_permissions` keeps only `& 0o777`. File type bits come from the call you use (`add_directory`, `add_symlink`).
- 7-Zip verified the output: Deflate, zip64, AES-256 and ZipCrypto all pass `7z t`.

### "Add to existing archive"
Two APIs exist, and both were probed:
```rust
// (a) in place: append + rewrite central directory
let f = OpenOptions::new().read(true).write(true).open(&path).unwrap();
let mut zw = ZipWriter::new_append(f).unwrap();
zw.start_file("added-later.txt", SimpleFileOptions::default()).unwrap();
zw.write_all(b"appended").unwrap();
zw.finish().unwrap();

// (b) rebuild into a new file, copying existing entries without recompressing
let mut zw = ZipWriter::new(File::create(&copy_path).unwrap());
for i in 0..ar.len() {
    let f = ar.by_index_raw(i).unwrap();
    zw.raw_copy_file(f).unwrap(); // or raw_copy_file_rename(f, "new/name"), raw_copy_file_touch(f, DateTime, Some(mode))
}
zw.start_file("new.txt", SimpleFileOptions::default()).unwrap();
zw.write_all(b"new").unwrap();
zw.finish().unwrap();
```
**BUG in 8.6.0 `new_append`:** for every existing AES entry it rewrites the central-directory method from 99 to 8. The local header keeps 99. 7-Zip then reports `Headers Error : secret-aes.txt`. The zip crate itself still reads the result. `raw_copy_file` (b) keeps 99 and passes `7z t`. **Use (b): write to a temp file, then rename.** Also: `deep_copy_file`, `shallow_copy_file`, `merge_archive(ZipArchive)`.

### Parallel compression
zip has no parallel deflate. There is also **no public API to write a pre-compressed entry from raw bytes plus crc/size**: `start_entry` with raw values is private. The working pattern is to have each worker build a one-entry zip in memory (or in a temp file) and let the main thread `merge_archive` it. `merge_archive` copies the compressed bytes and headers as they are:
```rust
let parts: Vec<Vec<u8>> = std::thread::scope(|s| {
    let hs: Vec<_> = inputs.iter().map(|(name, data)| s.spawn(move || {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        let o = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(dt(2024, 1, 2, 3, 4, 6));
        w.start_file(name.as_str(), o).unwrap();
        w.write_all(data).unwrap();
        w.finish().unwrap().into_inner()
    })).collect();
    hs.into_iter().map(|h| h.join().unwrap()).collect()
});
let mut zw = ZipWriter::new(File::create(&par_path).unwrap());
for p in parts {
    zw.merge_archive(ZipArchive::new(Cursor::new(p)).unwrap()).unwrap();
}
zw.finish().unwrap();
```
7-Zip verified the output (`Everything is Ok`). `raw_copy_file(ar.by_index_raw(0))` from each mini-archive works the same way and also allows renaming.

### Reading archives from other tools (verified on 7-Zip output)
Deflate64, LZMA (method 14), BZip2, AES-256 (7-Zip `-mem=AES256`) and ZipCrypto all read correctly.

### Split / multi-disk
- **Real spanned zips (`.z01 .z02 ... .zip`, WinZip/Info-ZIP `zip -s`) are not supported.** `zip_archive.rs` returns `UnsupportedArchive("Support for multi-disk files is not implemented")` when `disk_number != disk_with_central_directory`. Offsets in those zips are relative to each disk, so concatenation alone does not work. A reader would have to map (disk, offset) itself.
- **7-Zip's `-v` split (`x.zip.001`, `.002`)** is a plain byte split. `ZipArchive::new(File(001))` gives `Could not find EOCD`, but `ZipArchive::new(MultiFileReader::open(&volumes))` reads it (verified, 4 volumes). `MultiFileReader` is in `src/lib.rs`: a ~60-line Read+Seek over the concatenated files.

---

## 2. sevenz-rust2 0.23.0 (pure Rust)

Features: default = aes256, bzip2, compress, ppmd, util. Use `aes256, compress, bzip2, ppmd`:
- `compress` gives the writer and `lzma-rust2/encoder`.
- `util` gives only the convenience helpers `decompress_file*`, `compress_to_path*` and `default_entry_extract_fn`. Skip it.
- `deflate` reads 7z Deflate, which is rare. **The `deflate` feature turns on `flate2/zlib-rs`, which switches the whole binary's flate2 to zlib-rs.** That costs about 68 KB. Leave it off unless wanted.
- `zstd`, `brotli` and `lz4` are optional. `zstd` pulls in C.

Decode support (verified on 7-Zip 25 archives): LZMA2 (incl. `-mx=9 -mmt`), LZMA, BCJ2+LZMA (exe filter), PPMd, BZip2, Deflate, AES + header encryption (`-mhe=on`), solid, and volumes through `MultiFileReader`. **Deflate64 is not supported** (`UnsupportedCompressionMethod("DEFLATE64")`). PPMd decode is slow: 2.3 s for 4.4 MB in release.

### Imports
```rust
use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
use sevenz_rust2::{
    Archive, ArchiveEntry, ArchiveReader, ArchiveWriter, BlockDecoder, EncoderConfiguration,
    Error as SzError, NtTime, Password, SourceReader, prepare_block,
};
```

### Open, list, and handle passwords
```rust
// header-encrypted archive, no password:
match Archive::open(&enc_path) { Err(SzError::PasswordRequired) => {}, _ => unreachable!() }
// wrong password at open (header-encrypted):
//   Err(MaybeBadPassword(Custom { kind: InvalidInput, error: "range decoder first byte is not zero" }))
let ar = Archive::open_with_password(&enc_path, &Password::new("Passw0rd")).unwrap();
println!("solid={} blocks={} files={}", ar.is_solid, ar.blocks.len(), ar.files.len());
for e in &ar.files {
    let mt: SystemTime = e.last_modified_date().into();   // NtTime -> SystemTime
    // e.name(), e.is_directory(), e.size(), e.compressed_size, e.windows_attributes(),
    // e.has_last_modified_date, e.has_windows_attributes, e.has_stream(), e.creation_date(), e.access_date()
}
```
Also available: `Archive::read(&mut (impl Read+Seek), &Password)` and `ArchiveReader::from_archive(archive, source, pw)`.
When only the data is encrypted (header in clear), listing works without a password and the error comes at extract time:
- no password: `PasswordRequired`
- wrong password: `MaybeBadPassword(... "invalid LZMA2 control byte" ...)`

### Extract every entry with byte progress
```rust
pub fn extract_all<R: Read + Seek>(
    rd: &mut ArchiveReader<R>, dest: &Path, mut on_progress: impl FnMut(&str, u64, u64),
) -> Result<u64, SzError> {
    let total: u64 = rd.archive().files.iter().filter(|e| e.has_stream()).map(|e| e.size()).sum();
    let mut done = 0u64;
    rd.for_each_entries(|e, r| {
        let Some(path) = safe_join(dest, e.name()) else {
            io::copy(r, &mut io::sink())?; // drain, then skip
            return Ok(true);
        };
        if e.is_directory() { fs::create_dir_all(&path)?; return Ok(true); }
        fs::create_dir_all(path.parent().unwrap())?;
        let mut f = File::create(&path)?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 { break; }
            f.write_all(&buf[..n])?;
            done += n as u64;
            on_progress(e.name(), done, total);
        }
        if e.has_last_modified_date { let _ = f.set_modified(e.last_modified_date().into()); }
        Ok(true)
    })?;
    Ok(done)
}
// usage
let mut rd = ArchiveReader::open(&enc_path, Password::new("Passw0rd")).unwrap();
extract_all(&mut rd, &out, |name, done, total| { /* ... */ }).unwrap();
let one: Vec<u8> = rd.read_file("dir/file3.bin").unwrap();   // single entry by name, into memory
```
The `for_each_entries` callback signature is `FnMut(&ArchiveEntry, &mut dyn Read) -> Result<bool, sevenz_rust2::Error>`. `?` on `io::Error` works there.

### Write: non-solid, LZMA2 level, AES-256, encrypted header
```rust
fn entry(name: &str, mtime: SystemTime) -> ArchiveEntry {
    let mut e = ArchiveEntry::new_file(name);
    e.last_modified_date = NtTime::try_from(mtime).unwrap();
    e.has_last_modified_date = true;
    e.windows_attributes = 0x20; // FILE_ATTRIBUTE_ARCHIVE
    e.has_windows_attributes = true;
    e
}

let mut w = ArchiveWriter::create(&enc_path).unwrap();          // or ArchiveWriter::new(impl Write+Seek)
let mut aes = AesEncoderOptions::new(Password::new("Passw0rd"));
aes.num_cycles_power = 19; // crate default is 8 (2^8 SHA-256 rounds); 7-Zip writes 19
w.set_content_methods(vec![
    aes.into(), // AES first, then codec
    Lzma2Options::from_level(7).into(),
]);
w.set_encrypt_header(true); // default is already true; only takes effect when AES is in the methods
w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("dir"), None).unwrap();
for (name, data) in &files {
    let e = w.push_archive_entry(entry(name, mtime), Some(data.as_slice())).unwrap();
    assert_eq!(e.size, data.len() as u64);   // returns &ArchiveEntry with size/compressed_size filled
}
w.finish().unwrap();
```
`7z l -slt` on the result shows `Method = LZMA2:24 7zAES:19`, `Encrypted = +`, and the header is encrypted. `ArchiveEntry::from_path(path, name)` fills times from the filesystem but **not** `windows_attributes`.

### Solid block + multithreaded LZMA2
```rust
let mut w = ArchiveWriter::create(&solid_path).unwrap();
// 4 threads; chunk (independent LZMA2 stream) is clamped to >= dict size.
w.set_content_methods(vec![Lzma2Options::from_level_mt(6, 4, 1 << 20).into()]);
let entries: Vec<ArchiveEntry> = files.iter().map(|(n, _)| entry(n, mtime)).collect();
let readers: Vec<SourceReader<&[u8]>> = files.iter().map(|(_, d)| SourceReader::new(d.as_slice())).collect();
w.push_archive_entries(entries, readers).unwrap();   // one solid block
w.finish().unwrap();

let mut rd = ArchiveReader::open(&solid_path, Password::empty()).unwrap();
rd.set_thread_count(4);  // defaults to available_parallelism(); only MT-encoded LZMA2 benefits
```

### Several solid blocks compressed on worker threads (supported by the crate)
```rust
let methods: Arc<Vec<EncoderConfiguration>> = Arc::new(vec![Lzma2Options::from_level(6).into()]);
let blocks = std::thread::scope(|s| {
    let hs: Vec<_> = files.chunks(2).map(|chunk| {
        let methods = methods.clone();
        s.spawn(move || {
            let entries = chunk.iter().map(|(n, _)| entry(n, mtime)).collect();
            let readers = chunk.iter().map(|(_, d)| SourceReader::new(Cursor::new(d.clone()))).collect();
            prepare_block(methods, entries, readers).unwrap()   // whole compressed block in memory
        })
    }).collect();
    hs.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
});
let mut w = ArchiveWriter::create(&par_path).unwrap();
w.set_content_methods(methods.as_ref().clone());
for b in blocks { w.push_prepared_block(b).unwrap(); } // push order = archive order
w.finish().unwrap();
```
For parallel decode, give each block its own `BlockDecoder` and its own file handle:
```rust
BlockDecoder::new(1 /*threads*/, block_index, &archive, &password, &mut File::open(p)?)
    .for_each_entries(&mut |e, r| { /* drain r */ Ok(true) })?;
```

### Volumes `.7z.001`, `.002`, ...
The crate has no volume support (no "volume" in the source). 7z volumes are pure byte splits, so:
- **Read:** `ArchiveReader::new(MultiFileReader::open(&MultiFileReader::volumes(first_001))?, pw)`. Verified on 7-Zip's `-v700k -mhe=on -p` output.
- **Write:** `ArchiveWriter::new(SplitWriter::new(base, vol_bytes))`. **A rotate-on-size writer is not enough**, because `finish()` seeks back to offset 0 to write the start header. `SplitWriter` (in `src/sevenzops.rs`, about 50 lines) maps every absolute offset to (volume, offset). Verified: 5 volumes with AES, and `7z t vol.7z.001` reports `Volumes = 5, Everything is Ok`.
```rust
let mut w = ArchiveWriter::new(SplitWriter::new(vol_base.clone(), 300_000)).unwrap();
w.set_content_methods(vec![AesEncoderOptions::new("Passw0rd".into()).into(), Lzma2Options::from_level(1).into()]);
for (name, data) in &files { w.push_archive_entry(entry(name, mtime), Some(data.as_slice())).unwrap(); }
w.finish().unwrap().flush().unwrap();
```

### sevenz gotchas
1. **Drain every reader in `for_each_entries`.** Entries in a solid block share one decoder stream through `BoundedReader`, which does not skip unread bytes. If one entry is left undrained, the next entry gets its leftover bytes and the CRC fails.
2. **`Ok(false)` stops only the current block.** `ArchiveReader::for_each_entries` discards the block's bool (`forder_dec.for_each_entries(&mut each)?;`) and moves on to the next block, then to the empty files. To cancel, return `Err(SzError::other("cancelled"))`.
3. Empty files and directories (no stream) are reported **after** all blocks.
4. Directory entries from `new_directory` have no mtime or attributes. Set them yourself.
5. Entry names are not sanitized (the crate's `safe_join` is private). Use our `safe_join`, which rejects `..`, roots and drive prefixes and treats `\` as a separator.
6. The crate caches derived AES keys, so a second open with the same password and salt is instant.

---

## 3. tar + compressors (all pure Rust)

### Pieces and choices
- `tar` 0.4.46: the default feature is only `xattr` (Unix). Use `default-features = false`. Depends on `filetime`.
- `flate2` 1.1.10: **default = `rust_backend` = miniz_oxide**. `zlib-rs` (pure Rust port of zlib-ng) is faster, mainly when compressing, and 64.5 KB bigger in the zip stage. If any crate turns on `flate2/zlib-rs`, every flate2 user in the binary uses zlib-rs (`any_zlib` takes precedence over miniz). sevenz-rust2's `deflate` feature does that. cab forces `rust_backend`, which is harmless. Gezik today compiles flate2 with miniz only (through resvg/usvg).
- **xz: `lzma-rust2` 0.21 (pure Rust, the same crate sevenz-rust2 0.23 uses, so it is shared).** `liblzma` and `xz2` are C bindings; avoid them.
- **bzip2: `bzip2` 0.6.1, whose default backend is `libbz2-rs-sys` (pure Rust, Trifecta), for both read and write.** It is the same crate zip and sevenz use. `bzip2-rs` 0.1.2 also exists (pure Rust, decoder only) but is not needed. The `bzip2-sys` feature would switch to C.
- **zstd decode: `ruzstd` 0.9.0.** Its encoder (`ruzstd::encoding::compress`) only has the levels `Uncompressed` and `Fastest`; the others are "UNIMPLEMENTED". Use it for reading.

### Decoder selection (verified on our output and on GNU tar/gzip/xz/bzip2 output)
```rust
pub fn decoder<'a, R: BufRead + 'a>(codec: Codec, r: R) -> Box<dyn Read + 'a> {
    match codec {
        Codec::None => Box::new(r),
        // Multi*: concatenated members (pigz / pbzip2 output, `cat a.gz b.gz`) decode fully.
        Codec::Gz => Box::new(flate2::bufread::MultiGzDecoder::new(r)),
        Codec::Bz2 => Box::new(bzip2::bufread::MultiBzDecoder::new(r)),
        // second arg: accept concatenated .xz streams
        Codec::Xz => Box::new(lzma_rust2::XzReader::new(r, true)),
        Codec::Zst => Box::new(ZstdReader::new(r)),
    }
}
```
`XzReader::new_mem_limit(r, true, mem_limit_kb)` caps decoder memory. Multithreaded xz decode needs Seek: `XzReaderMt::new(inner: Read+Seek, allow_multi, workers)`.

### ruzstd: multi-frame streaming reader
`StreamingDecoder` decodes **one frame** only. pzstd output, `cat a.zst b.zst` and skippable frames need this loop (verified with a 2-frame file):
```rust
use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};

pub struct ZstdReader<R: BufRead> { src: R, dec: FrameDecoder, in_frame: bool }
impl<R: BufRead> ZstdReader<R> {
    pub fn new(src: R) -> Self {
        let mut dec = FrameDecoder::new();
        dec.set_max_window_size(1 << 31); // default DEFAULT_MAX_WINDOW_SIZE; --long=31 needs 2 GiB
        Self { src, dec, in_frame: false }
    }
}
impl<R: BufRead> Read for ZstdReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() { return Ok(0); }
        loop {
            if self.in_frame {
                while self.dec.can_collect() < buf.len() && !self.dec.is_finished() {
                    let need = buf.len() - self.dec.can_collect();
                    self.dec.decode_blocks(&mut self.src, BlockDecodingStrategy::UptoBytes(need))
                        .map_err(io::Error::other)?;
                }
                let n = self.dec.read(buf)?;
                if n > 0 { return Ok(n); }
                self.in_frame = false; // frame exhausted
            }
            if self.src.fill_buf()?.is_empty() { return Ok(0); }
            match self.dec.reset(&mut self.src) {
                Ok(()) => self.in_frame = true,
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame { length, .. })) => {
                    io::copy(&mut (&mut self.src).take(length as u64), &mut io::sink())?;
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
            }
        }
    }
}
```

### List
```rust
let mut ar = tar::Archive::new(decoder(codec_for(&name), BufReader::new(File::open(path)?)));
for e in ar.entries()? {
    let e = e?;
    let h = e.header();
    let _ = (
        e.path()?,                 // Cow<Path>; GNU long-name / PAX path already applied; "./x" -> "x"
        h.entry_type(),            // EntryType::{Regular, Directory, Symlink, Link, ...}
        e.size(),
        h.mode().unwrap_or(0),     // a blank numeric field is an Err, not 0: don't `?` it
        h.mtime().ok(),            // u64 unix secs
        e.link_name()?,            // Option<Cow<Path>> for symlinks/hardlinks
    );
}
```

### Extract with sanitizing and progress
```rust
let len = fs::metadata(path)?.len();
let f = ReadProgress { inner: File::open(path)?, done: 0, on_progress: move |d: u64| {
    let _pct = d * 100 / len.max(1); // compressed-bytes progress
}};
let mut ar = tar::Archive::new(decoder(codec_for(&name), BufReader::new(f)));
ar.set_preserve_mtime(true);        // default true
ar.set_preserve_permissions(false); // default false; only meaningful on Unix
ar.set_overwrite(true);             // default true
for e in ar.entries()? {
    let mut e = e?;
    let p = e.path()?.into_owned();
    let is_link = matches!(e.header().entry_type(), EntryType::Symlink | EntryType::Link);
    match e.unpack_in(dest) {
        Ok(written) => on_entry(&p, written),   // false = refused (".." / absolute path)
        Err(err) if is_link && cfg!(windows) && format!("{err:?}").contains("os error 1314") => on_entry(&p, false),
        Err(err) => return Err(err),
    }
}
```
`unpack_in` behavior (verified):
- A `../evil.tx` entry returns `Ok(false)` and nothing is written.
- Paths with `..` are rejected.
- Symlink targets are validated before use (lib.rs security notes; TOCTOU races are out of scope).
- mtime is restored (verified 1715953530).
- **Windows:** creating a symlink without Developer Mode fails with `ERROR_PRIVILEGE_NOT_HELD` (1314). tar wraps it in `TarError`, whose `Display` is just "failed to unpack ..." and `raw_os_error()` is None. The code only appears in the `Debug` text, hence the `format!("{err:?}")` check. Gezik should decide its policy up front: skip links on Windows and report them.
- The ReadProgress wrapper gives progress against the compressed file. Uncompressed tar.* size is unknown until the end.

### Write (Builder with progress)
```rust
let mut b = tar::Builder::new(&mut tar_bytes);   // any Write; wrap in GzEncoder/XzWriter for .tar.gz/.tar.xz
b.mode(tar::HeaderMode::Complete); // keep mtime/mode as given (Deterministic zeroes them)

let mut h = Header::new_gnu();
h.set_entry_type(EntryType::Directory); h.set_mode(0o755); h.set_mtime(mtime); h.set_size(0);
b.append_data(&mut h, "docs/", io::empty()).unwrap(); // append_data sets path + cksum

let mut h = Header::new_gnu();
h.set_size(big.len() as u64); h.set_mode(0o644); h.set_mtime(mtime);
let src = ReadProgress { inner: big.as_slice(), done: 0, on_progress: |_| ticks += 1 };
b.append_data(&mut h, &long_name_over_100_bytes, src).unwrap(); // GNU long-name record added automatically

let mut h = Header::new_gnu();
h.set_entry_type(EntryType::Symlink); h.set_size(0); h.set_mode(0o777); h.set_mtime(mtime);
b.append_link(&mut h, "link-to-run", "run.sh").unwrap();
b.finish().unwrap();
```
`Header::set_path` refuses `..`. The probe forges such an entry through `h.as_old_mut().name` + `set_cksum()` + `append` to test the reader. Other calls: `append_path_with_name(src, name)`, `append_file(name, &mut File)`, `append_dir_all(name, dir)` (no progress), `append_writer(&mut h, path) -> EntryWriter` (streaming without a known size).

### Compressing
```rust
let mut gz = flate2::write::GzEncoder::new(File::create(p)?, flate2::Compression::new(6));
gz.write_all(&tar_bytes)?; gz.finish()?;

let mut xz = lzma_rust2::XzWriter::new(File::create(p)?, lzma_rust2::XzOptions::with_preset(6))?;
xz.write_all(&tar_bytes)?; xz.finish()?;

// multithreaded xz: needs a block size; each block compresses independently
let mut o = lzma_rust2::XzOptions::with_preset(6);
o.set_block_size(std::num::NonZeroU64::new(1 << 20));
let mut xzmt = lzma_rust2::XzWriterMt::new(File::create(p)?, o, 4)?;
xzmt.write_all(&tar_bytes)?; xzmt.finish()?;

let mut bz = bzip2::write::BzEncoder::new(File::create(p)?, bzip2::Compression::new(9));
bz.write_all(&tar_bytes)?; bz.finish()?;
```
- `XzWriterMt` returns `"block size must be set"` without a block size.
- The block size is clamped to at least the dictionary size (8 MiB at preset 6), so a 1 MiB block gave 1 block (`xz -lv`). MT only helps for inputs larger than the dictionary.
- All outputs pass `xz -t` and `tar -tvf`.

---

## 4. RAR: `unrar-ng` 0.7.7 (pick this over `unrar` 0.5.8)

|            | unrar 0.5.8 | unrar-ng 0.7.7 |
|------------|-------------|----------------|
| Last update | 2025-02-19 | 2026-05-07 |
| Bundled UnRAR | 7.1 (2024) | 7.21 (2026) |
| Edition / MSRV | 2021 / none | 2024 / 1.85 |
| Extra API | none | `extract_all_with_callback`, `ExtractStatus::Cancelled`, `LargeDictWarning`, `open_for_listing_split` |

**Do not link both:** they compile the same UnRAR C++ symbols.

**Build on MSVC:** `unrar-ng-sys`'s build.rs compiles 45 UnRAR `.cpp` files with `cc` (C++14, `-O2`). It links `powrprof`, `shell32` and `advapi32`. It needs `cl.exe`, which cc finds through vswhere; no CMake or other setup. **It compiled here without problems.** Measured with cargo `--timings` on a clean target, 32 threads:
- build-script run (C++): 23.0 s
- regex/aho-corasick deps: about 6 s
- whole rar-only release build: 26 s

Binary cost: +278 KB on top of stage 3, or 400,896 B for a rar-only exe against 120,320 B empty.
Dependencies: `regex` (glob for volume names), `widestring`, `bitflags`, `winapi`, `libc`.
**License:** the UnRAR license is freeware, not OSI. It allows use in any software to extract RAR, but forbids re-creating RAR compression. Its paragraph 2 must be included in the docs. Check this against Gezik's PolyForm-Noncommercial licensing.

### List (password needed for `-hp` archives)
```rust
use unrar_ng::error::{Code, UnrarError};
use unrar_ng::{Archive, ExtractEvent, ExtractStatus};

let ar = match password { Some(pw) => Archive::with_password(path, pw), None => Archive::new(path) };
let open = ar.open_for_listing()?; // Iterator<Item = Result<FileHeader, UnrarError>>
// open.is_solid(), open.has_encrypted_headers(), open.volume_info() -> VolumeInfo::{None, First, Subsequent}
for h in open {
    let h = h?;
    // h.filename: PathBuf (Windows: '\' separators), h.unpacked_size: u64, h.is_directory(),
    // h.is_encrypted(), h.is_split(), h.file_attr: u32 (unix mode if packed on unix: 0x81a4),
    // h.file_time: u32 (MS-DOS date/time), h.file_crc: u32, h.method
}
```

### Extract entry by entry (password, volumes)
```rust
let mut open = ar.as_first_part().open_for_processing()?;   // part3.rar -> starts at part1
while let Some(header) = open.read_header()? {
    let e = header.entry();
    open = if e.is_file() {
        header.extract_with_base(dest)?      // or extract_to(file_path), read() -> (Vec<u8>, next), test()
    } else {
        header.skip()?
    };
}
```

### Batch extraction with a per-file callback (cancellable)
```rust
let status = Archive::with_password(&p, "unrar").open_for_processing()?
    .extract_all_with_callback(dest, |ev| {
        match ev {
            ExtractEvent::Start { filename, size } => { /* file begins */ }
            ExtractEvent::Ok { filename, .. } => { /* file done */ }
            ExtractEvent::Err { filename, error_code } => {}
            _ => {}                                   // LargeDictWarning{..}: return false to refuse
        }
        true // false = cancel -> Ok(ExtractStatus::Cancelled)
    })?;
assert_eq!(status, ExtractStatus::Completed);
```

### Verified behavior (test RARs from the unrar crate: RAR4 = crypted/part1, RAR5 = solid/unicode/-hp)
| Case | Result |
|------|--------|
| `-hp` (RAR5) without a password | `MissingPassword` |
| `-hp` (RAR5) with a wrong password | `BadPassword` |
| RAR4 encrypted data, no password | `MissingPassword` |
| RAR4 encrypted data, wrong password | **`BadData`** (RAR4 has no password check value) |
| `part1.rar` with part2 missing, `open_for_listing` | ends with `Err(EOpen)` |
| `part1.rar` with part2 missing, extract | `EOpen@Process (Could not open next volume)` |
| `open_for_listing_split()` | lists only that volume's headers, still ends with `Err(EOpen)` |

Treat a trailing `Err` as "next volume missing". Unicode names (`te…―st✌`) decode correctly.

### RAR gotchas
- **No per-byte progress** in the safe API. UCM_PROCESSDATA is only used internally by `read()`/`test()`, which buffer the whole entry in memory. For large entries, run the extraction on a worker thread and poll the size of the growing output file, or use `ExtractEvent::Start{size}` and `Ok` for per-file steps.
- Path sanitizing is left to UnRAR (`extract_with_base` joins `dest` and the entry name inside the DLL). The bundled UnRAR strips `..`, but this was **not tested** here because there was no malicious RAR to try. To be safe, check `header.entry().filename` with `safe_join` before extracting and skip anything it refuses.

---

## 5. cab 0.6.0 (pure Rust; depends on flate2 `rust_backend`, lzxd, time, byteorder)

Read support: None, MSZIP and LZX (verified on a `makecab /D CompressionType=LZX` cab, 3 MB in 20 ms). Quantum cannot be read. The writer supports None and MSZIP.
```rust
let mut cab = cab::Cabinet::new(File::open(path)?)?;
let mut names = Vec::new();
for folder in cab.folder_entries() {               // borrows cab immutably -> collect names first
    for f in folder.file_entries() {
        // f.name() (uses '\'), f.uncompressed_size(): u32, folder.compression_type(),
        // f.datetime(): Option<time::PrimitiveDateTime>, f.is_read_only(), f.is_hidden(),
        // f.is_system(), f.is_archive(), f.is_exec(), f.is_name_utf()
        names.push(f.name().to_owned());
    }
}
for n in &names {
    let Some(out) = safe_join(dest, n) else { continue };
    fs::create_dir_all(out.parent().unwrap())?;
    let mut r = cab.read_file(n)?;                  // FileReader: Read (+ Seek)
    io::copy(&mut r, &mut File::create(out)?)?;
}
```
Writing:
```rust
let mut b = cab::CabinetBuilder::new();
let folder = b.add_folder(cab::CompressionType::MsZip);
folder.add_file("docs\\big.bin");
folder.add_file("small.txt").set_is_read_only(true);
let mut w = b.build(File::create(&cab_path)?)?;
while let Some(mut fw) = w.next_file()? { fw.write_all(data_for(fw.file_name()))?; }
w.finish()?;
```

Gotchas:
- **`read_file` is O(folder)**: each call opens a new folder reader and decodes from the folder start (`seek_to_uncompressed_offset`). Extracting n files of one big LZX folder is therefore O(n²). Name lookup is linear, and the first duplicate name wins.
- Cabinets that span several `.cab` files are not supported (only `cabinet_set_id()` and `cabinet_set_index()` are exposed).

---

## 6. ISO: pick `hadris-iso` 2.5.0 (and say UDF is not supported); `isomage` 2.1.0 is the fallback

| | hadris-iso 2.5.0 | isomage 2.1.0 |
|---|---|---|
| updated | 2026-10-03; 17.6k downloads | 2026-05-14; 1.8k downloads |
| deps (read: `std,sync,read`) | hadris-{io,common,fixed,path,part,macros}, bytemuck, bitflags, spin, chrono, thiserror, tracing, crc, heapless... | **none** |
| ISO 9660 / Joliet / Rock Ridge | yes / yes / yes (PX mode, TF times, SL symlinks, NM names, CL/PL/RE) | yes / yes (preferred) / NM names only |
| UDF | **no** | yes (`udf::parse_udf`) |
| multi-extent (>4 GiB files) | yes (`extents()`, `read_file_chunked`) | no (single `file_location`) |
| metadata | mode, symlink target, RR mtimes, raw record time | name, size, is_directory only |
| streaming | `read_file_chunked::<N>` → `next_chunk()` | `cat_node(&mut file, node, &mut writer)` |
| also | write ISOs (used to make the test image), El Torito | many other formats behind features |

**Choice: hadris-iso** for 9660/Joliet/RR: real metadata, multi-extent, active maintenance. **Gap: UDF-only images.** Windows 10/11 install ISOs keep their real content only in UDF; the ISO 9660 tree there holds just a README. For those, either use isomage's `udf::parse_udf` (zero dependencies, ~1 MB of source with every format off) or report the image as unsupported. `isomage::detect_and_parse_filesystem` tries ISO 9660 **first**, so on such images it returns the README-only tree. Call `parse_udf` directly when the image has an NSR descriptor.

### hadris-iso read (verified; 7-Zip also reads the ISO hadris wrote)
```rust
use hadris_iso::read::IsoImage;
let img = IsoImage::open(File::open(path)?).map_err(|e| io::Error::other(format!("{e:?}")))?;
// img.supports_rrip(), img.root_dirs().iter().map(|r| r.entry_type())  (Level1{..}, Joliet{..})
let mut stack = vec![(img.root_dir().dir_ref(), PathBuf::new())];  // root_dir() = best_choice()
while let Some((dref, rel)) = stack.pop() {
    for e in img.open_dir(dref).entries() {
        let e = e.map_err(|e| io::Error::other(format!("{e:?}")))?;
        if e.is_special() { continue; }                         // "." and ".."
        let name = e.display_name().into_owned();               // RRIP NM > Joliet > ISO name w/o ";1"
        let rel = rel.join(&name);
        if let Some(r) = &e.rrip {
            let _mode = r.posix_attributes.as_ref().map(|p| p.file_mode.read());   // 0o100644
            let _link = r.symlink_target.clone();
            let _mtime = r.timestamps.as_ref().and_then(|t| t.modify);             // RripDateTime{year,..,gmt_offset}
        }
        // ISO 9660 recording time: DirDateTime fields are private -> raw header bytes 18..25
        let raw = e.header().to_bytes();
        let _rec = (1900 + raw[18] as u16, raw[19], raw[20], raw[21], raw[22], raw[23], raw[24] as i8);
        if e.is_directory() {
            stack.push((e.as_dir_ref(&img).map_err(|e| io::Error::other(format!("{e:?}")))?, rel));
        } else if let Some(out) = safe_join(dest, &rel.to_string_lossy()) {
            fs::create_dir_all(out.parent().unwrap())?;
            let mut chunks = img.read_file_chunked::<65536>(&e).map_err(|e| io::Error::other(format!("{e:?}")))?;
            let mut f = File::create(out)?;
            while let Some(c) = chunks.next_chunk().map_err(|e| io::Error::other(format!("{e:?}")))? {
                f.write_all(&c)?;           // progress: chunks.position() / chunks.total_size()
            }
        }
    }
}
```
Other calls: `img.read_file(&e) -> io::Result<Vec<u8>>` (whole file) and `img.find_path("a/b")`.

Gotchas:
- `root_dir()` uses usefulness scoring. A Level-1+RRIP root can win over Joliet; RR names are UTF-8, so Unicode still worked (`Türkçe ağaç.txt`). If you need a specific namespace, pick it with `root_dirs().get(EntryType)`.
- Interleaved files return `Unsupported`.
- `IsoImage` needs 2048-byte logical blocks.
- Its error type is not `std::error::Error`-friendly, hence `format!("{e:?}")`.

### isomage (fallback / UDF)
```rust
let mut f = File::open(path)?;
let root = isomage::detect_and_parse_filesystem(&mut f, &path.to_string_lossy())?;  // TreeNode "/"
// node.name, node.size, node.is_directory, node.children, root.find_node("a/b")
isomage::cat_node(&mut f, node, &mut writer)?;   // stream one file
// isomage::udf::parse_udf(&mut f)? for UDF; avoid extract_node (prints to stderr)
```

---

## 7. cpio 0.4.1 and ar 0.9.0 (.deb)

**cpio crate = newc only** (`070701` and `070702`). `070707` (odc) and binary (`0x71C7`) fail at `NewcReader::new` with "Invalid magic number". Sizes are u32 (4 GiB). For odc support we would need our own parser of about 60 lines (76-byte octal header).
```rust
// newc: each NewcReader owns the stream; finish()/to_writer() hands it back.
let mut r = BufReader::new(File::open(path)?);
loop {
    let e = cpio::NewcReader::new(r)?;
    if e.entry().is_trailer() { break; }                    // "TRAILER!!!"
    let name = e.entry().name().to_owned();                  // String (UTF-8 required)
    let mode = e.entry().mode();                             // e.entry().mtime(), file_size(), uid(), gid(), nlink()
    r = match (mode & 0o170000, safe_join(dest, &name)) {
        (0o040000, Some(p)) => { fs::create_dir_all(p)?; e.finish()? }
        (0o100000, Some(p)) => {
            fs::create_dir_all(p.parent().unwrap())?;
            e.to_writer(File::create(p)?)?                   // copies data, skips padding
        }
        _ => e.finish()?,   // unsafe name, symlink (data = target), device, fifo
    };
}
```
`7z t` passes on the cpio written by `cpio::write_cpio(iter_of_(NewcBuilder, Read+Seek), out)`.

**ar** (works for .deb; `next_entry()` is not an Iterator because of the borrow):
```rust
let mut ar = ar::Archive::new(File::open(path)?);
while let Some(entry) = ar.next_entry() {
    let mut entry = entry?;
    let id = String::from_utf8_lossy(entry.header().identifier()).into_owned(); // GNU/BSD long names handled
    // entry.header().size(), .mtime(), .mode(), .uid(), .gid(); entry: Read
    let codec = /* by suffix: .gz .xz .zst .bz2 .tar */;
    let mut t = tar::Archive::new(decoder(codec, BufReader::new(&mut entry)));   // stream, no temp file
    for te in t.entries()? { let te = te?; /* te.path()? ... */ }
}
```
Verified on a .deb built with ar::Builder (`debian-binary`, `control.tar.xz`, `data.tar.gz`); 7-Zip also opens it.

---

## 8. Magic numbers (implemented in `src/detect.rs`, verified on every probe file)

| format | offset | bytes |
|---|---|---|
| zip | 0 | `50 4B 03 04` (`PK\3\4`); empty zip `PK\5\6`; spanned first disk `PK\7\8`; SFX/prefixed: EOCD `PK\5\6` at EOF-22 (no comment) |
| 7z | 0 | `37 7A BC AF 27 1C` |
| RAR 1.5–4.x | 0 | `52 61 72 21 1A 07 00` |
| RAR 5 | 0 | `52 61 72 21 1A 07 01 00` (test both; RAR4 is the 7-byte prefix) |
| gzip | 0 | `1F 8B` (then `08` = deflate) |
| xz | 0 | `FD 37 7A 58 5A 00` |
| bzip2 | 0 | `42 5A 68` `'1'..'9'` (`BZh9`) |
| zstd | 0 | `28 B5 2F FD` (skippable frames: `5? 2A 4D 18`) |
| tar | 257 | `ustar\0` + `00` (POSIX) or `ustar  \0` (GNU); v7 tar has no magic, so validate the header checksum at 148..156 |
| cab | 0 | `4D 53 43 46 00 00 00 00` (`MSCF` + reserved) |
| ISO 9660 | 0x8001 | `CD001` (byte 0x8000 = descriptor type); UDF: `BEA01` at 0x8001, `NSR02`/`NSR03` at 0x8801/0x9001 |
| cpio newc / crc | 0 | ASCII `070701` / `070702` |
| cpio odc | 0 | ASCII `070707` |
| cpio binary | 0 | `C7 71` (LE) / `71 C7` (BE) |
| ar | 0 | `!<arch>\n` |
| deb | 0 + 8 | `!<arch>\n` then first member name `debian-binary` |

Read at least 0x9010 bytes to cover ISO and UDF. Check order: specific signatures first, then `ustar` at 257, then ISO at 0x8001, then the v7 tar checksum.

---

## 9. Release binary size (opt-level=3, lto=true, codegen-units=1, strip=true, panic="abort")

Crate: `archprobe/sizeprobe`. Run `measure.sh`, or build with `--no-default-features --features <list>`. Every stage does a real write and read, and archive paths come from argv so LTO keeps all decoders.

| stage | exe bytes | Δ vs previous |
|---|---:|---:|
| s0 empty main | 120,320 | — |
| s1 zip (deflate via miniz + AES-256, read+write) | 380,416 | **+260,096** |
| s1 same with flate2/zlib-rs | 444,928 | (+64,512 vs miniz) |
| s1b + zip deflate64 + bzip2 (libbz2-rs) | 485,888 | +105,472 |
| (s1c + zip lzma/xz, lzma-rust2 0.16) | 671,744 | (+185,856) |
| (s1d + zip zstd, C zstd-sys) | 1,119,744 | (+448,000) |
| s2 s1b + sevenz-rust2 (aes256, compress = LZMA/LZMA2 enc+dec) | 894,464 | **+408,576** |
| s2c + 7z bzip2 + ppmd | 932,864 | +38,400 |
| s2b + 7z deflate (switches all flate2 to zlib-rs) | 1,002,496 | +69,632 |
| s3 + tar + gz r/w + xz r/w (shared lzma-rust2 0.21) + bz2 + ruzstd decode | 1,249,792 | **+247,296** |
| s4 + unrar-ng (UnRAR 7.21 C++) | 1,527,808 | **+278,016** |
| s5 + cab + hadris-iso (read) + cpio + ar | 1,609,728 | **+81,920** |
| s5 variant without 7z `deflate` (miniz stays) | 1,541,632 | (recommended set) |
| s5 variant with zip lzma/xz on (2nd lzma-rust2) | 1,761,280 | (+151,552) |

So the recommended set adds about **1.42 MB** to an empty exe. In Gezik it will be somewhat less, because flate2/miniz_oxide, crc32fast and time are already linked through resvg and cab.
The s1d build crashed rustc once with `STATUS_ILLEGAL_INSTRUCTION` (LTO plus zstd-sys); the retry succeeded. That is one more reason to leave zip's `zstd` feature off.

---

## 10. Gotcha checklist for the plan

1. Move the crate with `rust-version = 1.92` to 1.93, which sevenz-rust2 ≥ 0.21 requires.
2. zip: do not use `new_append` on archives with AES entries (central-directory method bug). Rebuild with `raw_copy_file` into a temp file, then rename.
3. zip: leave `zstd` off (C, +448 KB) and `lzma`/`xz` off (second lzma-rust2 copy). LZMA writing and Deflate64 writing are unsupported anyway.
4. zip: `by_index_raw` lists encrypted entries without a password. Give `Stored` `compression_level(None)`. Set `large_file(true)`, or `set_auto_large_file()` on the writer, for files that can exceed 4 GiB.
5. zip: there is no raw pre-compressed write API. Parallel writing works through mini-zips plus `merge_archive` or `raw_copy_file`.
6. zip: real `.z01` spanned archives are unsupported. 7-Zip's `.zip.001` byte split works through `MultiFileReader`.
7. 7z: drain every entry reader. `Ok(false)` does not stop extraction; return `Err` to cancel. Directories and empty files come last.
8. 7z: set `AesEncoderOptions.num_cycles_power = 19`; the crate default of 8 is far weaker than 7-Zip's.
9. 7z: there is no volume support in the crate. Use `MultiFileReader` for reading and a seekable `SplitWriter` for writing (verified with 7-Zip).
10. 7z: the `deflate` feature switches the whole binary's flate2 to zlib-rs. There is no Deflate64. PPMd decode is slow (~2 MB/s).
11. flate2: miniz_oxide is the default and is already in Gezik; zlib-rs is faster and +64 KB. Any crate turning on `zlib-rs` flips the backend everywhere.
12. ruzstd: `StreamingDecoder` is single-frame; use the `ZstdReader` loop. The window limit is configurable. The encoder only has Fastest.
13. tar: a blank header numeric field gives `Err` from `mode()`/`mtime()`, so do not `?` it in listings. On Windows, symlink entries fail with os error 1314, which is visible only in `Debug` output. `unpack_in` returns `Ok(false)` for `..`.
14. RAR: unrar-ng only (never alongside unrar). C++ build adds about 23 s; check the UnRAR license. There is no per-byte progress. RAR4 with a wrong password gives `BadData`, not `BadPassword`. A missing volume gives `Err(EOpen)` at the end of the iteration.
15. cab: `read_file` decodes from the folder start, so it is quadratic for many files in one LZX folder. Names use `\`. No multi-cab sets.
16. ISO: hadris-iso has no UDF. For UDF-only images, use isomage `udf::parse_udf` or refuse the image. The 9660 record time is only available from raw header bytes 18..25.
17. cpio crate reads newc/crc only; odc and binary need a small own parser.
18. Every format: route entry names through `safe_join` (rejects `..`, root and drive prefixes, treats `\` as a separator). zip has `enclosed_name`, tar has `unpack_in`, and isomage's `extract_node` validates names. sevenz, cab, cpio, ar and hadris do no sanitizing, and RAR's DLL-side sanitizing was not tested.
