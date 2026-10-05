//! Reading archives into a staging folder: archives are made here with the crates' writers
//! (and with 7-Zip when it is installed), opened with `archive::open` and extracted.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gezik_batch::archive::io::SplitWriter;
use gezik_batch::archive::{self, ExtractCx};
use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
use sevenz_rust2::{ArchiveEntry, ArchiveWriter, NtTime, Password, SourceReader};
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, DateTime, ZipWriter};

struct Cx {
    password: Option<String>,
    asked: std::cell::Cell<u32>,
    failed: std::cell::RefCell<Vec<String>>,
    cancel_after: Option<u64>,
    bytes: std::cell::Cell<u64>,
}

impl Cx {
    fn new(password: Option<&str>) -> Cx {
        Cx {
            password: password.map(str::to_owned),
            asked: 0.into(),
            failed: Default::default(),
            cancel_after: None,
            bytes: 0.into(),
        }
    }

    fn failed(&self) -> Vec<String> {
        self.failed.borrow().clone()
    }
}

impl ExtractCx for Cx {
    fn add_bytes(&self, n: u64) {
        self.bytes.set(self.bytes.get() + n);
    }
    fn entry_done(&self) {}
    fn stopped(&self) -> bool {
        self.cancel_after.is_some_and(|limit| self.bytes.get() >= limit)
    }
    fn password(&self, retry: bool) -> Option<String> {
        self.asked.set(self.asked.get() + 1);
        if retry { None } else { self.password.clone() }
    }
    fn entry_failed(&self, name: &str, _: &std::io::Error) {
        self.failed.borrow_mut().push(name.to_owned());
    }
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-batch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The files under `root` as (relative path with '/', contents).
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let rel: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                out.push((rel.join("/"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// A staging folder inside `d`, made empty.
fn stage(d: &Path) -> PathBuf {
    let s = d.join("stage");
    let _ = std::fs::remove_dir_all(&s);
    std::fs::create_dir_all(&s).unwrap();
    s
}

/// Poorly compressible bytes.
fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn files(list: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = list.iter().map(|(n, d)| (n.to_string(), d.to_vec())).collect();
    v.sort();
    v
}

fn extract(path: &Path, stage: &Path, cx: &Cx) -> std::io::Result<()> {
    archive::open(path)?.extract(stage, cx)
}

fn dt(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> DateTime {
    DateTime::from_date_and_time(y, mo, d, h, mi, s).unwrap()
}

#[test]
fn zip_round_trip_with_folders_dates_and_big_file() {
    let d = dir("zip-round");
    let path = d.join("a.zip");
    let text = "Merhaba dünya\n".repeat(100);
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(1))
            .last_modified_time(dt(2024, 5, 17, 13, 45, 30));
        zw.add_directory("klasör/", opts).unwrap();
        zw.add_directory("boş/", opts).unwrap();
        zw.start_file("klasör/Türkçe ağaç.txt", opts).unwrap();
        zw.write_all(text.as_bytes()).unwrap();
        zw.start_file("big.bin", opts.large_file(true)).unwrap();
        let zeros = vec![0u8; 1 << 20];
        for _ in 0..70 {
            zw.write_all(&zeros).unwrap();
        }
        zw.finish().unwrap();
    }
    let stage = stage(&d);
    let cx = Cx::new(None);
    let mut source = archive::open(&path).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    assert_eq!(listed.len(), 4);
    assert!(listed.iter().any(|e| e.name == "big.bin" && e.size == Some(70 << 20) && !e.is_dir));
    source.extract(&stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(cx.asked.get(), 0);
    assert_eq!(cx.bytes.get(), (70 << 20) + text.len() as u64);
    assert!(stage.join("boş").is_dir());
    let got = tree(&stage);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0, "big.bin");
    assert!(got[0].1.len() == 70 << 20 && got[0].1.iter().all(|&b| b == 0));
    assert_eq!(got[1], ("klasör/Türkçe ağaç.txt".to_string(), text.into_bytes()));

    // The DOS time is local time; it reads back as the same local time (2 s steps).
    let modified = std::fs::metadata(stage.join("klasör/Türkçe ağaç.txt")).unwrap().modified().unwrap();
    let want = gezik_batch_local(2024, 5, 17, 13, 45, 30);
    let diff = modified.duration_since(want).unwrap_or_else(|e| e.duration());
    assert!(diff <= Duration::from_secs(2), "{modified:?} vs {want:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// The time whose local reading is the given date and time (searched around the UTC reading).
fn gezik_batch_local(y: i32, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> SystemTime {
    let want = gezik_core::batch::date::DateParts { year: y, month: mo, day: d, hour: h, minute: mi, second: s };
    // 2024-05-17 13:45:30 UTC, then every quarter hour within ±14 hours.
    let utc = SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530);
    (-56i64..=56)
        .map(|q| {
            if q >= 0 {
                utc + Duration::from_secs(q as u64 * 900)
            } else {
                utc - Duration::from_secs(q.unsigned_abs() * 900)
            }
        })
        .find(|t| gezik_platform::local_date_parts(*t) == Some(want))
        .expect("a local time for the date")
}

fn aes_zip(path: &Path) {
    let mut zw = ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default().with_aes_encryption(AesMode::Aes256, "Passw0rd");
    zw.start_file("a.txt", opts).unwrap();
    zw.write_all(b"first secret").unwrap();
    zw.start_file("b/c.txt", opts).unwrap();
    zw.write_all(&noise(100_000, 3)).unwrap();
    zw.finish().unwrap();
}

#[test]
fn zip_aes_asks_once_and_wrong_password_skips() {
    let d = dir("zip-aes");
    let path = d.join("aes.zip");
    aes_zip(&path);

    let stage = stage(&d);
    let cx = Cx::new(Some("Passw0rd"));
    extract(&path, &stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 1);
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), files(&[("a.txt", b"first secret"), ("b/c.txt", &noise(100_000, 3))]));

    let stage = self::stage(&d);
    let cx = Cx::new(Some("wrong"));
    extract(&path, &stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 2);
    assert_eq!(cx.failed(), ["aes.zip"]);
    assert!(tree(&stage).is_empty());
    assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn zip_slip_entries_are_refused() {
    let d = dir("zip-slip");
    let path = d.join("slip.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        zw.start_file("ok.txt", opts).unwrap();
        zw.write_all(b"ok").unwrap();
        // The writer takes these names as they are; the reader must refuse them.
        zw.start_file("../evil.txt", opts).unwrap();
        zw.write_all(b"evil").unwrap();
        zw.start_file("/abs.txt", opts).unwrap();
        zw.write_all(b"abs").unwrap();
        zw.start_file("CON.txt", opts).unwrap();
        zw.write_all(b"device").unwrap();
        zw.finish().unwrap();
    }
    let names: Vec<String> =
        archive::open(&path).unwrap().list(&Cx::new(None)).unwrap().unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["ok.txt", "../evil.txt", "/abs.txt", "CON.txt"]);

    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    assert_eq!(cx.failed(), ["../evil.txt", "/abs.txt", "CON.txt"]);
    assert_eq!(tree(&stage), files(&[("ok.txt", b"ok")]));
    assert!(!d.join("evil.txt").exists());
    assert!(!Path::new("/abs.txt").exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// A 7z entry for a file with a fixed time.
fn sz_entry(name: &str) -> ArchiveEntry {
    let mut e = ArchiveEntry::new_file(name);
    e.last_modified_date = NtTime::try_from(SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530)).unwrap();
    e.has_last_modified_date = true;
    e
}

#[test]
fn sevenz_header_encrypted_and_solid() {
    let d = dir("7z-enc");
    let path = d.join("enc.7z");
    let content: Vec<(String, Vec<u8>)> =
        (0..5).map(|k| (format!("dir/file{k}.bin"), noise(50_000 + k * 1000, 10 + k as u32))).collect();
    {
        let mut w = ArchiveWriter::create(&path).unwrap();
        let mut aes = AesEncoderOptions::new(Password::new("Passw0rd"));
        aes.num_cycles_power = 19;
        w.set_content_methods(vec![aes.into(), Lzma2Options::from_level(5).into()]);
        w.set_encrypt_header(true);
        w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("dir"), None).unwrap();
        w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("empty"), None).unwrap();
        let entries = content.iter().map(|(n, _)| sz_entry(n)).collect();
        let readers = content.iter().map(|(_, data)| SourceReader::new(data.as_slice())).collect();
        w.push_archive_entries(entries, readers).unwrap();
        w.finish().unwrap();
    }

    // The header needs the password: listing asks once, extracting does not ask again.
    let stage = stage(&d);
    let cx = Cx::new(Some("Passw0rd"));
    let mut source = archive::open(&path).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    assert_eq!(listed.len(), 7);
    assert!(listed.iter().any(|e| e.name == "dir/file2.bin" && e.encrypted && e.size == Some(52_000)));
    source.extract(&stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 1);
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), content);
    assert!(stage.join("empty").is_dir());
    let modified = std::fs::metadata(stage.join("dir/file0.bin")).unwrap().modified().unwrap();
    assert_eq!(modified, SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530));

    let stage = self::stage(&d);
    let cx = Cx::new(Some("wrong"));
    extract(&path, &stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 2);
    assert_eq!(cx.failed(), ["enc.7z"]);
    assert!(tree(&stage).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn sevenz_data_encrypted_wrong_password_asks_again() {
    let d = dir("7z-data");
    let path = d.join("data.7z");
    let content = noise(200_000, 5);
    {
        let mut w = ArchiveWriter::create(&path).unwrap();
        w.set_content_methods(vec![AesEncoderOptions::new("pw".into()).into(), Lzma2Options::from_level(1).into()]);
        w.set_encrypt_header(false);
        w.push_archive_entry(sz_entry("a.bin"), Some(content.as_slice())).unwrap();
        w.finish().unwrap();
    }
    struct Twice(std::cell::Cell<u32>);
    impl ExtractCx for Twice {
        fn add_bytes(&self, _: u64) {}
        fn entry_done(&self) {}
        fn stopped(&self) -> bool {
            false
        }
        fn password(&self, retry: bool) -> Option<String> {
            self.0.set(self.0.get() + 1);
            Some(if retry { "pw" } else { "bad" }.to_string())
        }
        fn entry_failed(&self, name: &str, e: &std::io::Error) {
            panic!("{name}: {e}");
        }
    }
    let stage = stage(&d);
    let cx = Twice(0.into());
    archive::open(&path).unwrap().extract(&stage, &cx).unwrap();
    assert_eq!(cx.0.get(), 2);
    assert_eq!(tree(&stage), [("a.bin".to_string(), content)]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn sevenz_volumes_open_from_any_part() {
    let d = dir("7z-vol");
    let content: Vec<(String, Vec<u8>)> = (0..4).map(|k| (format!("f{k}.bin"), noise(200_000, 20 + k))).collect();
    {
        let mut w = ArchiveWriter::new(SplitWriter::new(d.join("v.7z"), 300_000)).unwrap();
        w.set_content_methods(vec![Lzma2Options::from_level(1).into()]);
        for (name, data) in &content {
            w.push_archive_entry(sz_entry(name), Some(data.as_slice())).unwrap();
        }
        w.finish().unwrap().flush().unwrap();
    }
    assert!(d.join("v.7z.003").exists() && !d.join("v.7z.004").exists());

    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&d.join("v.7z.002"), &stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), content);

    // A hole in the set: `.002` missing while `.003` is there.
    let middle = d.join("v.7z.002");
    let kept = std::fs::read(&middle).unwrap();
    std::fs::remove_file(&middle).unwrap();
    for part in ["v.7z.001", "v.7z.003"] {
        let err = archive::open(&d.join(part)).err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(err.to_string(), "part v.7z.002 is missing");
    }
    std::fs::write(&middle, kept).unwrap();

    std::fs::remove_file(d.join("v.7z.001")).unwrap();
    let err = archive::open(&d.join("v.7z.003")).err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert!(err.to_string().contains("v.7z.001"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// A tar with a folder, a name over 100 bytes, a small file and an entry named `../evil.tx`.
fn tar_bytes(long: &str, big: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut b = tar::Builder::new(&mut out);
        b.mode(tar::HeaderMode::Complete);
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_mode(0o755);
        h.set_mtime(1_715_953_530);
        h.set_size(0);
        b.append_data(&mut h, "docs/", std::io::empty()).unwrap();

        let mut h = tar::Header::new_gnu();
        h.set_size(big.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(1_715_953_530);
        b.append_data(&mut h, long, big).unwrap();

        let mut h = tar::Header::new_gnu();
        h.set_size(5);
        h.set_mode(0o755);
        h.set_mtime(1_715_953_530);
        b.append_data(&mut h, "run.sh", &b"echo\n"[..]).unwrap();

        // `set_path` refuses `..`, so the raw name bytes are written.
        let mut h = tar::Header::new_gnu();
        h.set_size(4);
        h.set_mtime(1_715_953_530);
        h.as_old_mut().name[..10].copy_from_slice(b"../evil.tx");
        h.set_cksum();
        b.append(&h, &b"evil"[..]).unwrap();
        b.finish().unwrap();
    }
    out
}

#[test]
fn tar_families() {
    let d = dir("tar");
    let long = format!("docs/{}/big.bin", "x".repeat(120));
    let big = noise(300_000, 7);
    let tar = tar_bytes(&long, &big);

    std::fs::write(d.join("t.tar"), &tar).unwrap();
    let mut gz =
        flate2::write::GzEncoder::new(std::fs::File::create(d.join("t.tar.gz")).unwrap(), flate2::Compression::new(6));
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap();
    let mut xz = lzma_rust2::XzWriter::new(
        std::fs::File::create(d.join("t.tar.xz")).unwrap(),
        lzma_rust2::XzOptions::with_preset(1),
    )
    .unwrap();
    xz.write_all(&tar).unwrap();
    xz.finish().unwrap();
    let mut bz =
        bzip2::write::BzEncoder::new(std::fs::File::create(d.join("t.tar.bz2")).unwrap(), bzip2::Compression::new(9));
    bz.write_all(&tar).unwrap();
    bz.finish().unwrap();
    // Two zstd frames back to back.
    let mut zst = ruzstd::encoding::compress_to_vec(&tar[..10240], ruzstd::encoding::CompressionLevel::Fastest);
    zst.extend(ruzstd::encoding::compress_to_vec(&tar[10240..], ruzstd::encoding::CompressionLevel::Fastest));
    std::fs::write(d.join("t.tar.zst"), zst).unwrap();

    for name in ["t.tar", "t.tar.gz", "t.tar.xz", "t.tar.bz2", "t.tar.zst"] {
        let stage = stage(&d);
        let cx = Cx::new(None);
        let mut source = archive::open(&d.join(name)).unwrap();
        assert_eq!(source.list(&cx).unwrap(), None, "{name}");
        source.extract(&stage, &cx).unwrap();
        assert_eq!(cx.failed(), ["../evil.tx"], "{name}");
        assert_eq!(tree(&stage), files(&[(long.as_str(), &big), ("run.sh", b"echo\n")]), "{name}");
        assert!(!d.join("evil.tx").exists(), "{name}");
        let modified = std::fs::metadata(stage.join(&long)).unwrap().modified().unwrap();
        assert_eq!(modified, SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530), "{name}");
    }

    // Cut short: the archive ends with an error, and nothing half written stays.
    std::fs::write(d.join("cut.tar"), &tar[..200_000]).unwrap();
    let stage = stage(&d);
    let cx = Cx::new(None);
    assert!(extract(&d.join("cut.tar"), &stage, &cx).is_err());
    assert_eq!(cx.failed(), [long.as_str()]);
    assert!(tree(&stage).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn single_file_streams_use_their_inner_name() {
    let d = dir("single");
    let text = b"notes\n".repeat(1000);
    let mut gz = flate2::write::GzEncoder::new(
        std::fs::File::create(d.join("notes.txt.gz")).unwrap(),
        flate2::Compression::new(6),
    );
    gz.write_all(&text).unwrap();
    gz.finish().unwrap();
    // A gzip header that keeps the original name wins over the archive's.
    let mut gz = flate2::GzBuilder::new()
        .filename("original.txt")
        .mtime(1_715_953_530)
        .write(std::fs::File::create(d.join("renamed.gz")).unwrap(), flate2::Compression::new(6));
    gz.write_all(&text).unwrap();
    gz.finish().unwrap();
    let mut xz = lzma_rust2::XzWriter::new(
        std::fs::File::create(d.join("a.log.xz")).unwrap(),
        lzma_rust2::XzOptions::with_preset(1),
    )
    .unwrap();
    xz.write_all(&text).unwrap();
    xz.finish().unwrap();
    let mut bz =
        bzip2::write::BzEncoder::new(std::fs::File::create(d.join("b.csv.bz2")).unwrap(), bzip2::Compression::new(9));
    bz.write_all(&text).unwrap();
    bz.finish().unwrap();
    std::fs::write(
        d.join("c.json.zst"),
        ruzstd::encoding::compress_to_vec(&text[..], ruzstd::encoding::CompressionLevel::Fastest),
    )
    .unwrap();

    for (archive_name, inner) in [
        ("notes.txt.gz", "notes.txt"),
        ("renamed.gz", "original.txt"),
        ("a.log.xz", "a.log"),
        ("b.csv.bz2", "b.csv"),
        ("c.json.zst", "c.json"),
    ] {
        let stage = stage(&d);
        let cx = Cx::new(None);
        extract(&d.join(archive_name), &stage, &cx).unwrap();
        assert!(cx.failed().is_empty(), "{archive_name}");
        assert_eq!(tree(&stage), [(inner.to_string(), text.clone())], "{archive_name}");
        assert_eq!(cx.bytes.get(), text.len() as u64);
    }
    let stage = stage(&d);
    extract(&d.join("renamed.gz"), &stage, &Cx::new(None)).unwrap();
    let modified = std::fs::metadata(stage.join("original.txt")).unwrap().modified().unwrap();
    assert_eq!(modified, SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_leaves_no_half_file() {
    let d = dir("cancel");
    let path = d.join("big.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("big.bin", opts).unwrap();
        zw.write_all(&noise(5 << 20, 9)).unwrap();
        zw.finish().unwrap();
    }
    let stage = stage(&d);
    let mut cx = Cx::new(None);
    cx.cancel_after = Some(1 << 20);
    let err = extract(&path, &stage, &cx).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(tree(&stage).is_empty());
    assert!(cx.bytes.get() < 2 << 20);
    let _ = std::fs::remove_dir_all(&d);
}

/// Where `needle` starts in `hay`, from `from`.
fn find(hay: &[u8], needle: &[u8], from: usize) -> usize {
    from + hay[from..].windows(needle.len()).position(|w| w == needle).unwrap()
}

#[test]
fn oversized_entry_is_stopped() {
    let d = dir("oversized");
    let path = d.join("liar.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("big.bin", opts).unwrap();
        zw.write_all(&noise(3 << 20, 4)).unwrap();
        zw.start_file("ok.txt", opts).unwrap();
        zw.write_all(b"ok").unwrap();
        zw.finish().unwrap();
    }
    // "Uncompressed size" is at 22 in the local header and at 24 in the central one.
    let mut bytes = std::fs::read(&path).unwrap();
    let local = find(&bytes, b"PK\x03\x04", 0);
    bytes[local + 22..local + 26].copy_from_slice(&10u32.to_le_bytes());
    let central = find(&bytes, b"PK\x01\x02", 0);
    assert_eq!(&bytes[central + 46..central + 53], b"big.bin");
    bytes[central + 24..central + 28].copy_from_slice(&10u32.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();

    let stage = stage(&d);
    let cx = Cx::new(None);
    let mut source = archive::open(&path).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    assert_eq!(listed[0].size, Some(10));
    source.extract(&stage, &cx).unwrap();
    assert_eq!(cx.failed(), ["big.bin"]);
    assert_eq!(tree(&stage), files(&[("ok.txt", b"ok")]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_damaged_zip_entry_fails_alone() {
    let d = dir("crc");
    let path = d.join("bad.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("bad.bin", opts).unwrap();
        zw.write_all(&[7u8; 1000]).unwrap();
        zw.start_file("ok.txt", opts).unwrap();
        zw.write_all(b"ok").unwrap();
        zw.finish().unwrap();
    }
    // One byte of the stored data changed: the CRC fails.
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    let at = find(&bytes, &[7u8; 16], 0) + 500;
    file.seek(SeekFrom::Start(at as u64)).unwrap();
    file.write_all(&[8]).unwrap();
    drop(file);

    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    assert_eq!(cx.failed(), ["bad.bin"]);
    assert_eq!(tree(&stage), files(&[("ok.txt", b"ok")]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn no_entry_is_written_through_a_link() {
    let d = dir("links");
    let path = d.join("links.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        // A link to the parent, then a file "inside" it: the file must land in the stage.
        zw.add_symlink("up", "..", opts).unwrap();
        zw.start_file("up/evil.txt", opts).unwrap();
        zw.write_all(b"evil").unwrap();
        zw.start_file("real.txt", opts).unwrap();
        zw.write_all(b"real").unwrap();
        zw.add_symlink("good", "real.txt", opts).unwrap();
        zw.add_symlink("away", "../../outside", opts).unwrap();
        zw.finish().unwrap();
    }
    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    assert!(!d.join("evil.txt").exists());
    assert!(stage.join("up").symlink_metadata().unwrap().is_dir());
    assert_eq!(std::fs::read(stage.join("up/evil.txt")).unwrap(), b"evil");
    if cfg!(windows) {
        // Windows skips every link.
        assert_eq!(cx.failed(), ["up", "good", "away"]);
    } else {
        // `up` cannot be made (a folder is there now); `away` points outside.
        assert_eq!(cx.failed(), ["up", "away"]);
        assert_eq!(std::fs::read(stage.join("good")).unwrap(), b"real");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 7-Zip's own output, when it is installed: an encrypted-header 7z in volumes and a
/// Deflate64 zip.
#[test]
fn reads_7zip_output() {
    let seven = Path::new(r"C:\Program Files\7-Zip\7z.exe");
    if !seven.exists() {
        return;
    }
    let d = dir("7zip");
    let src = d.join("src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    let a = noise(400_000, 1);
    std::fs::write(src.join("sub").join("a.bin"), &a).unwrap();
    std::fs::write(src.join("b.txt"), b"bee").unwrap();
    let run = |args: &[&str]| {
        let status = std::process::Command::new(seven)
            .args(args)
            .current_dir(&src)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "{args:?}");
    };
    let out = |name: &str| d.join(name).to_string_lossy().into_owned();
    run(&["a", "-t7z", "-mhe=on", "-pPw", "-v150k", &out("s.7z"), "sub", "b.txt"]);
    run(&["a", "-tzip", "-mm=Deflate64", &out("d64.zip"), "sub", "b.txt"]);
    run(&["a", "-tzip", "-v150k", &out("split.zip"), "sub", "b.txt"]);
    let want = files(&[("b.txt", b"bee"), ("sub/a.bin", &a)]);
    for (name, password) in [("s.7z.002", Some("Pw")), ("d64.zip", None), ("split.zip.003", None)] {
        let stage = stage(&d);
        let cx = Cx::new(password);
        extract(&d.join(name), &stage, &cx).unwrap();
        assert!(cx.failed().is_empty(), "{name}: {:?}", cx.failed());
        assert_eq!(tree(&stage), want, "{name}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn zip_keeps_read_only() {
    let d = dir("readonly");
    let path = d.join("ro.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        zw.start_file("ro.txt", SimpleFileOptions::default().unix_permissions(0o444)).unwrap();
        zw.write_all(b"ro").unwrap();
        zw.start_file("rw.txt", SimpleFileOptions::default().unix_permissions(0o644)).unwrap();
        zw.write_all(b"rw").unwrap();
        zw.finish().unwrap();
    }
    let stage = stage(&d);
    extract(&path, &stage, &Cx::new(None)).unwrap();
    assert!(std::fs::metadata(stage.join("ro.txt")).unwrap().permissions().readonly());
    assert!(!std::fs::metadata(stage.join("rw.txt")).unwrap().permissions().readonly());
    // A staging folder with read-only files in it can still be removed.
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_huge_tar_date_is_dropped_without_a_panic() {
    let d = dir("tar-date");
    let mut out = Vec::new();
    {
        let mut b = tar::Builder::new(&mut out);
        b.mode(tar::HeaderMode::Complete);
        // Base-256 dates: past u64 seconds as i64, and past what a SystemTime holds.
        for (name, mtime) in [("max.txt", u64::MAX), ("far.txt", 1u64 << 62)] {
            let mut h = tar::Header::new_gnu();
            h.set_size(2);
            h.set_mtime(mtime);
            b.append_data(&mut h, name, &b"ok"[..]).unwrap();
        }
        b.finish().unwrap();
    }
    std::fs::write(d.join("dates.tar"), out).unwrap();
    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&d.join("dates.tar"), &stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), files(&[("far.txt", b"ok"), ("max.txt", b"ok")]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn duplicate_names_the_last_one_wins() {
    let d = dir("dup");
    let path = d.join("dup.zip");
    {
        // The writer refuses a name twice, so the second is renamed in the bytes afterwards
        // (a name is not part of the CRC). The first is read-only, the second must replace it.
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        zw.start_file("one.txt", SimpleFileOptions::default().unix_permissions(0o444)).unwrap();
        zw.write_all(b"first").unwrap();
        zw.start_file("two.txt", SimpleFileOptions::default()).unwrap();
        zw.write_all(b"second").unwrap();
        zw.finish().unwrap();
    }
    let mut bytes = std::fs::read(&path).unwrap();
    let mut from = 0;
    while let Some(at) = bytes[from..].windows(7).position(|w| w == b"two.txt") {
        bytes[from + at..from + at + 7].copy_from_slice(b"one.txt");
        from += at + 7;
    }
    std::fs::write(&path, &bytes).unwrap();

    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), files(&[("one.txt", b"second")]));
    std::fs::remove_dir_all(&d).unwrap();
}

/// Cancels after 1 MiB of a 5 MB entry: `Interrupted`, and nothing half written stays.
fn assert_cancels(path: &Path, d: &Path) {
    let stage = stage(d);
    let mut cx = Cx::new(None);
    cx.cancel_after = Some(1 << 20);
    let started = std::time::Instant::now();
    let err = extract(path, &stage, &cx).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted, "{}", path.display());
    assert!(tree(&stage).is_empty(), "{}", path.display());
    assert!(cx.bytes.get() < 2 << 20, "{}", path.display());
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn cancel_stops_7z_tar_gz_and_xz() {
    let d = dir("cancel-more");
    let data = noise(5 << 20, 11);
    // A solid 7z: the cancelled entry is not decoded to its end, nor is the one after it.
    let seven = d.join("big.7z");
    {
        let mut w = ArchiveWriter::create(&seven).unwrap();
        w.set_content_methods(vec![Lzma2Options::from_level(1).into()]);
        let entries = vec![sz_entry("big.bin"), sz_entry("next.bin")];
        let readers = vec![SourceReader::new(data.as_slice()), SourceReader::new(data.as_slice())];
        w.push_archive_entries(entries, readers).unwrap();
        w.finish().unwrap();
    }
    assert_cancels(&seven, &d);

    let tar = {
        let mut out = Vec::new();
        let mut b = tar::Builder::new(&mut out);
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        b.append_data(&mut h, "big.bin", data.as_slice()).unwrap();
        b.finish().unwrap();
        drop(b);
        out
    };
    let mut gz = flate2::write::GzEncoder::new(
        std::fs::File::create(d.join("big.tar.gz")).unwrap(),
        flate2::Compression::fast(),
    );
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap();
    assert_cancels(&d.join("big.tar.gz"), &d);

    let mut xz = lzma_rust2::XzWriter::new(
        std::fs::File::create(d.join("big.bin.xz")).unwrap(),
        lzma_rust2::XzOptions::with_preset(0),
    )
    .unwrap();
    xz.write_all(&data).unwrap();
    xz.finish().unwrap();
    assert_cancels(&d.join("big.bin.xz"), &d);
    let _ = std::fs::remove_dir_all(&d);
}

/// Unix only: links that could lead out of the stage through another link are skipped, and
/// no folder is made for a link.
#[cfg(unix)]
#[test]
fn link_chains_cannot_escape() {
    let d = dir("link-chain");
    let path = d.join("chain.zip");
    {
        let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        zw.add_directory("d/", opts).unwrap();
        // `..` after a name: with B → "." below, A would point outside.
        zw.add_symlink("d/A", "B/../../x", opts).unwrap();
        zw.add_symlink("d/B", ".", opts).unwrap();
        // Above the stage.
        zw.add_symlink("up", "..", opts).unwrap();
        // Through a link: its folder is not a real one.
        zw.add_symlink("L", "d", opts).unwrap();
        zw.add_symlink("L/foo/c", "x", opts).unwrap();
        zw.add_symlink("d/sib", "../d/B", opts).unwrap();
        zw.finish().unwrap();
    }
    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    assert_eq!(cx.failed(), ["d/A", "up", "L/foo/c"]);
    assert!(stage.join("d/B").symlink_metadata().unwrap().file_type().is_symlink());
    assert!(stage.join("L").symlink_metadata().unwrap().file_type().is_symlink());
    assert!(stage.join("d/sib").symlink_metadata().unwrap().file_type().is_symlink());
    assert!(!stage.join("d/foo").exists() && !d.join("x").exists());
    let _ = std::fs::remove_dir_all(&d);
}
