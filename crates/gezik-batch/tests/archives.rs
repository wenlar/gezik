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
    // A device name only on Windows.
    if cfg!(windows) {
        assert_eq!(cx.failed(), ["../evil.txt", "/abs.txt", "CON.txt"]);
        assert_eq!(tree(&stage), files(&[("ok.txt", b"ok")]));
    } else {
        assert_eq!(cx.failed(), ["../evil.txt", "/abs.txt"]);
        assert_eq!(tree(&stage), files(&[("CON.txt", b"device"), ("ok.txt", b"ok")]));
    }
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

/// A damaged first entry looks like a wrong password; the same password again is right.
#[test]
fn sevenz_damaged_first_encrypted_entry_fails_once_the_password_repeats() {
    let d = dir("7z-damaged");
    let path = d.join("data.7z");
    let (a, b) = (noise(200_000, 6), noise(1000, 7));
    {
        let mut w = ArchiveWriter::create(&path).unwrap();
        w.set_content_methods(vec![AesEncoderOptions::new("pw".into()).into(), Lzma2Options::from_level(1).into()]);
        w.set_encrypt_header(false);
        w.push_archive_entry(sz_entry("a.bin"), Some(a.as_slice())).unwrap();
        w.push_archive_entry(sz_entry("b.bin"), Some(b.as_slice())).unwrap();
        w.finish().unwrap();
    }
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[32 + 100_000] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();
    struct Same(std::cell::Cell<u32>, std::cell::RefCell<Vec<String>>);
    impl ExtractCx for Same {
        fn add_bytes(&self, _: u64) {}
        fn entry_done(&self) {}
        fn stopped(&self) -> bool {
            false
        }
        fn password(&self, _: bool) -> Option<String> {
            self.0.set(self.0.get() + 1);
            (self.0.get() < 5).then(|| "pw".to_string())
        }
        fn entry_failed(&self, name: &str, _: &std::io::Error) {
            self.1.borrow_mut().push(name.to_owned());
        }
    }
    let stage = stage(&d);
    let cx = Same(0.into(), Default::default());
    archive::open(&path).unwrap().extract(&stage, &cx).unwrap();
    assert_eq!((cx.0.get(), cx.1.borrow().clone()), (2, vec!["a.bin".to_owned()]));
    assert_eq!(tree(&stage), [("b.bin".to_string(), b)]);
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

/// On a case-insensitive disk `readme` would replace `README`: it is left out (a repack is
/// then refused) instead.
#[test]
fn names_differing_only_in_case_do_not_replace_each_other() {
    let d = dir("case");
    let path = d.join("case.zip");
    let mut zw = ZipWriter::new(std::fs::File::create(&path).unwrap());
    zw.start_file("README", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"upper").unwrap();
    zw.start_file("readme", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"lower").unwrap();
    zw.finish().unwrap();

    let stage = stage(&d);
    std::fs::write(stage.join("Probe"), b"").unwrap();
    let insensitive = stage.join("PROBE").exists();
    std::fs::remove_file(stage.join("Probe")).unwrap();
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    if insensitive {
        assert_eq!(cx.failed(), ["readme"]);
        assert_eq!(tree(&stage), files(&[("README", b"upper")]));
    } else {
        assert!(cx.failed().is_empty(), "{:?}", cx.failed());
        assert_eq!(tree(&stage), files(&[("README", b"upper"), ("readme", b"lower")]));
    }
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

#[cfg(feature = "rar")]
/// A file of the test data (`tests/data/…`).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

#[cfg(feature = "rar")]
/// FNV-1a: checks the content of the downloaded test archives without keeping a copy.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

#[cfg(feature = "rar")]
/// The files under `root` as (path, length, FNV-1a).
fn hashed(root: &Path) -> Vec<(String, usize, u64)> {
    tree(root).into_iter().map(|(name, bytes)| (name, bytes.len(), fnv(&bytes))).collect()
}

#[cfg(feature = "rar")]
#[test]
fn rar4_and_rar5_extract() {
    let d = dir("rar");
    // RAR4 packed on Unix: two files, a symbolic link, a folder and an empty folder.
    let stage = stage(&d);
    let cx = Cx::new(None);
    let mut source = archive::open(&data("rar/test_read_format_rar.rar")).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    let names: Vec<&str> = listed.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["test.txt", "testlink", "testdir/test.txt", "testdir", "testemptydir"]);
    assert!(listed[3].is_dir && listed[0].size == Some(20) && !listed[0].encrypted);
    source.extract(&stage, &cx).unwrap();
    let text = b"test text document\r\n";
    assert!(stage.join("testemptydir").is_dir());
    assert_eq!(cx.bytes.get(), 40);
    if cfg!(windows) {
        assert_eq!(cx.failed(), ["testlink"]);
        assert_eq!(tree(&stage), files(&[("test.txt", text), ("testdir/test.txt", text)]));
    } else {
        assert!(cx.failed().is_empty(), "{:?}", cx.failed());
        assert_eq!(std::fs::read_link(stage.join("testlink")).unwrap(), Path::new("test.txt"));
    }
    let modified = std::fs::metadata(stage.join("test.txt")).unwrap().modified().unwrap();
    assert_eq!(gezik_platform::local_date_parts(modified).map(|p| (p.year, p.month, p.day)), Some((2011, 6, 26)));

    // RAR5.
    let stage = self::stage(&d);
    let cx = Cx::new(None);
    extract(&data("rar/test_read_format_rar5_multiple_files.rar"), &stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(cx.asked.get(), 0);
    assert_eq!(
        hashed(&stage),
        [
            ("test1.bin".to_string(), 4096, 0x990a_8e41_3bc9_0e83),
            ("test2.bin".to_string(), 4096, 0xd02d_b38e_fa4d_5ce3),
            ("test3.bin".to_string(), 4096, 0x998f_c865_54aa_8d0b),
            ("test4.bin".to_string(), 4096, 0x65fc_b635_5daa_e8e3),
        ]
    );
    let modified = std::fs::metadata(stage.join("test1.bin")).unwrap().modified().unwrap();
    // RAR5 keeps nanoseconds; NTFS holds 100 ns steps.
    let want = SystemTime::UNIX_EPOCH + Duration::new(1_538_023_271, 278_813_210);
    let diff = modified.duration_since(want).unwrap_or_else(|e| e.duration());
    assert!(diff < Duration::from_nanos(100), "{modified:?}");

    // UnRAR's data callback counts every byte.
    assert_eq!(cx.bytes.get(), 4 * 4096);

    // A cancel stops inside the first entry, and its file is removed.
    let stage = self::stage(&d);
    let mut cx = Cx::new(None);
    cx.cancel_after = Some(1);
    let err = extract(&data("rar/test_read_format_rar5_multiple_files.rar"), &stage, &cx).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(tree(&stage).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(feature = "rar")]
#[test]
fn rar_hard_links_and_copies_are_skipped_and_symlinks_follow_the_link_rules() {
    let d = dir("rar-links");
    // UnRAR would resolve a hard link's source itself; it is never asked to.
    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&data("rar/test_read_format_rar5_hardlink.rar"), &stage, &cx).unwrap();
    assert_eq!(cx.failed(), ["hardlink.txt"]);
    assert_eq!(tree(&stage), files(&[("file.txt", b"1234\n")]));

    // RAR5 symbolic links carry their target in the header.
    let stage = self::stage(&d);
    let cx = Cx::new(None);
    extract(&data("rar/test_read_format_rar5_symlink.rar"), &stage, &cx).unwrap();
    assert!(stage.join("dir").is_dir());
    if cfg!(windows) {
        assert_eq!(cx.failed(), ["symlink.txt", "dirlink"]);
    } else {
        assert!(cx.failed().is_empty(), "{:?}", cx.failed());
        assert_eq!(std::fs::read_link(stage.join("symlink.txt")).unwrap(), Path::new("file.txt"));
        assert_eq!(std::fs::read_link(stage.join("dirlink")).unwrap(), Path::new("dir"));
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(feature = "rar")]
#[test]
fn rar_cancel_inside_a_large_entry() {
    // The first entry of this set unpacks to 241 MB; the cancel comes after 1 MiB.
    let d = dir("rar-big");
    let stage = stage(&d);
    let mut cx = Cx::new(None);
    cx.cancel_after = Some(1 << 20);
    let started = std::time::Instant::now();
    let err = extract(&data("rar/test_read_format_rar_multivolume.part0001.rar"), &stage, &cx).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(tree(&stage).is_empty());
    assert!(cx.bytes.get() < 16 << 20, "{}", cx.bytes.get());
    assert!(started.elapsed() < Duration::from_secs(20));
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(feature = "rar")]
#[test]
fn rar_wrong_password_asks_again_then_skips() {
    let d = dir("rar-pw");
    // RAR4, encrypted data, plain headers: no password check, so a wrong one is bad data.
    let rar4 = data("rar/test_read_format_rar_encryption_data.rar");
    let stage = stage(&d);
    let cx = Cx::new(Some("12345678"));
    let mut source = archive::open(&rar4).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    assert_eq!(cx.asked.get(), 0);
    assert!(listed.iter().all(|e| e.encrypted));
    source.extract(&stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 1);
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), files(&[("bar.txt", b"data of bar.txt\n"), ("foo.txt", b"data of foo.txt\n")]));

    let stage = self::stage(&d);
    let cx = Cx::new(Some("wrong"));
    extract(&rar4, &stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 2);
    assert_eq!(cx.failed(), ["test_read_format_rar_encryption_data.rar"]);
    assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);

    // RAR5 with encrypted headers (`rar -hp`): the listing needs the password already.
    let rar5 = data("rar/test_read_format_rar5_encrypted_filenames.rar");
    let stage = self::stage(&d);
    let cx = Cx::new(Some("password"));
    let mut source = archive::open(&rar5).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    assert_eq!(listed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a.txt", "b.txt", "c.txt", "d.txt"]);
    source.extract(&stage, &cx).unwrap();
    assert_eq!(cx.asked.get(), 1);
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    let want: Vec<(String, Vec<u8>)> = ["a", "b", "c", "d"]
        .iter()
        .map(|n| (format!("{n}.txt"), format!("This is from {n}.txt").into_bytes()))
        .collect();
    assert_eq!(tree(&stage), want);

    for listing in [true, false] {
        let stage = self::stage(&d);
        let cx = Cx::new(Some("wrong"));
        let mut source = archive::open(&rar5).unwrap();
        if listing {
            assert_eq!(source.list(&cx).unwrap(), Some(Vec::new()));
        }
        source.extract(&stage, &cx).unwrap();
        assert_eq!(cx.asked.get(), 2);
        assert_eq!(cx.failed(), ["test_read_format_rar5_encrypted_filenames.rar"]);
        assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(feature = "rar")]
#[test]
fn rar_missing_volume_is_a_clear_error() {
    let d = dir("rar-vol");
    let parts: Vec<PathBuf> = (1..=3)
        .map(|n| {
            let name = format!("test_rar_multivolume_single_file.part{n}.rar");
            let copy = d.join(&name);
            std::fs::copy(data(&format!("rar/{name}")), &copy).unwrap();
            copy
        })
        .collect();
    // One file across three volumes, opened from the middle one.
    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&parts[1], &stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(hashed(&stage), [("LibarchiveAddingTest.html".to_string(), 20111, 0xa46d_d6be_0077_5a17)]);

    std::fs::remove_file(&parts[2]).unwrap();
    let stage = self::stage(&d);
    let cx = Cx::new(None);
    let err = extract(&parts[0], &stage, &cx).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(err.to_string(), "a later part (test_rar_multivolume_single_file.part3.rar) is missing");
    assert_eq!(cx.failed(), ["LibarchiveAddingTest.html"]);
    assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
    let err = archive::open(&parts[0]).unwrap().list(&cx).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);

    std::fs::remove_file(&parts[0]).unwrap();
    let err = archive::open(&parts[1]).err().unwrap();
    assert_eq!(err.to_string(), "the first part (test_rar_multivolume_single_file.part1.rar) is missing");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cab_mszip_and_lzx() {
    let d = dir("cab");
    let big = noise(300_000, 31);
    let path = d.join("a.cab");
    {
        let mut b = cab::CabinetBuilder::new();
        let folder = b.add_folder(cab::CompressionType::MsZip);
        folder.add_file("docs\\big.bin");
        folder.add_file("small.txt").set_is_read_only(true);
        let folder = b.add_folder(cab::CompressionType::None);
        folder.add_file("plain.txt");
        folder.add_file("..\\evil.txt");
        let mut w = b.build(std::fs::File::create(&path).unwrap()).unwrap();
        while let Some(mut fw) = w.next_file().unwrap() {
            let bytes: &[u8] = match fw.file_name() {
                "docs\\big.bin" => &big,
                "small.txt" => b"small",
                "plain.txt" => b"plain",
                _ => b"evil",
            };
            fw.write_all(bytes).unwrap();
        }
        w.finish().unwrap();
    }
    let stage = stage(&d);
    let cx = Cx::new(None);
    let mut source = archive::open(&path).unwrap();
    let listed = source.list(&cx).unwrap().unwrap();
    assert_eq!(listed.len(), 4);
    assert!(listed.iter().any(|e| e.name == "docs/big.bin" && e.size == Some(300_000)));
    source.extract(&stage, &cx).unwrap();
    assert_eq!(cx.failed(), ["../evil.txt"]);
    assert_eq!(tree(&stage), files(&[("docs/big.bin", &big), ("plain.txt", b"plain"), ("small.txt", b"small")]));
    assert!(std::fs::metadata(stage.join("small.txt")).unwrap().permissions().readonly());

    // A compressed folder of many files is 7-Zip's.
    let many = d.join("many.cab");
    {
        let mut b = cab::CabinetBuilder::new();
        let folder = b.add_folder(cab::CompressionType::MsZip);
        for i in 0..201 {
            folder.add_file(format!("f{i}.txt"));
        }
        let mut w = b.build(std::fs::File::create(&many).unwrap()).unwrap();
        while let Some(mut fw) = w.next_file().unwrap() {
            fw.write_all(b"x").unwrap();
        }
        w.finish().unwrap();
    }
    assert_eq!(archive::open(&many).err().unwrap().kind(), std::io::ErrorKind::Unsupported);

    // LZX: only makecab writes it.
    #[cfg(windows)]
    if Path::new(r"C:\Windows\System32\makecab.exe").exists() {
        std::fs::write(d.join("big.bin"), &big).unwrap();
        let status = std::process::Command::new("makecab")
            .args(["/D", "CompressionType=LZX", "/D", "CompressionMemory=21", "big.bin", "lzx.cab"])
            .current_dir(&d)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let stage = self::stage(&d);
        let cx = Cx::new(None);
        extract(&d.join("lzx.cab"), &stage, &cx).unwrap();
        assert!(cx.failed().is_empty(), "{:?}", cx.failed());
        assert_eq!(tree(&stage), files(&[("big.bin", &big)]));
    }
    std::fs::remove_dir_all(&d).unwrap();
}

/// Writes an ISO image with `features` holding a file, and a folder with a big file and a
/// Unicode name.
fn make_iso(path: &Path, big: &[u8], features: hadris_iso::write::options::CreationFeatures) {
    use hadris_iso::read::PathSeparator;
    use hadris_iso::write::options::IsoFormatOptions;
    use hadris_iso::write::{File as IsoFile, InputFiles, IsoImageWriter};
    use std::sync::Arc;
    let files = InputFiles {
        path_separator: PathSeparator::ForwardSlash,
        files: vec![
            IsoFile::File { name: Arc::new("readme.txt".into()), contents: b"Hello ISO".to_vec() },
            IsoFile::Directory {
                name: Arc::new("Docs".into()),
                children: vec![
                    IsoFile::File { name: Arc::new("Big File.bin".into()), contents: big.to_vec() },
                    IsoFile::File { name: Arc::new("Türkçe ağaç.txt".into()), contents: b"unicode".to_vec() },
                ],
            },
        ],
    };
    let options = IsoFormatOptions {
        volume_name: "GEZIK".into(),
        system_id: None,
        volume_set_id: None,
        publisher_id: None,
        preparer_id: None,
        application_id: None,
        sector_size: 2048,
        path_separator: PathSeparator::ForwardSlash,
        features,
        strict_charset: false,
    };
    let mut out = std::fs::File::options().read(true).write(true).create(true).truncate(true).open(path).unwrap();
    IsoImageWriter::create(&mut out, files, options).unwrap();
}

#[test]
fn iso_with_joliet_and_rock_ridge() {
    use hadris_iso::joliet::JolietLevel;
    use hadris_iso::write::options::CreationFeatures;
    let d = dir("iso");
    let big = noise(300_000, 41);
    let want =
        files(&[("Docs/Big File.bin", &big), ("Docs/Türkçe ağaç.txt", b"unicode"), ("readme.txt", b"Hello ISO")]);
    for (name, features) in
        [("both.iso", CreationFeatures::extensions()), ("joliet.iso", CreationFeatures::joliet(JolietLevel::Level3))]
    {
        let path = d.join(name);
        make_iso(&path, &big, features);
        let stage = stage(&d);
        let cx = Cx::new(None);
        let mut source = archive::open(&path).unwrap();
        let listed = source.list(&cx).unwrap().unwrap();
        assert_eq!(listed.len(), 4, "{name}");
        assert!(listed.iter().any(|e| e.name == "Docs" && e.is_dir), "{name}");
        assert!(listed.iter().any(|e| e.name == "Docs/Big File.bin" && e.size == Some(300_000)), "{name}");
        source.extract(&stage, &cx).unwrap();
        assert!(cx.failed().is_empty(), "{name}: {:?}", cx.failed());
        assert_eq!(tree(&stage), want, "{name}");
        assert_eq!(cx.bytes.get(), 300_000 + 7 + 9, "{name}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// An odc cpio header (76 octal bytes) and name for `name` with `size` bytes of data.
fn odc_header(name: &str, mode: u32, size: usize) -> Vec<u8> {
    let fields = [(0u64, 6), (7, 6), (u64::from(mode), 6), (0, 6), (0, 6), (1, 6), (0, 6), (1_715_953_530, 11)];
    let mut h = b"070707".to_vec();
    for (value, width) in fields.into_iter().chain([((name.len() + 1) as u64, 6), (size as u64, 11)]) {
        h.extend(format!("{value:0width$o}").into_bytes());
    }
    h.extend(name.as_bytes());
    h.push(0);
    h
}

#[test]
fn cpio_newc_and_odc() {
    let d = dir("cpio");
    let big = noise(100_001, 51);
    let newc = d.join("a.cpio");
    {
        let entry = |name: &str, mode: u32, bytes: &[u8]| {
            (cpio::NewcBuilder::new(name).mode(mode).mtime(1_715_953_530), std::io::Cursor::new(bytes.to_vec()))
        };
        let inputs = vec![
            entry(".", 0o040755, b""),
            entry("etc", 0o040755, b""),
            entry("etc/big.bin", 0o100644, &big),
            entry("../evil", 0o100644, b"x"),
            entry("dev/null", 0o020666, b""),
            entry("run.sh", 0o100755, b"echo\n"),
        ];
        cpio::write_cpio(inputs.into_iter(), std::fs::File::create(&newc).unwrap()).unwrap();
    }
    // odc by hand: the header, the name with its NUL, then the data.
    let odc = d.join("b.cpio");
    {
        let mut bytes = odc_header("etc", 0o040755, 0);
        bytes.extend(odc_header("etc/big.bin", 0o100644, big.len()));
        bytes.extend(&big);
        bytes.extend(odc_header("../evil", 0o100644, 1));
        bytes.extend(b"x");
        bytes.extend(odc_header("run.sh", 0o100755, 5));
        bytes.extend(b"echo\n");
        bytes.extend(odc_header("TRAILER!!!", 0, 0));
        std::fs::write(&odc, bytes).unwrap();
    }
    for path in [&newc, &odc] {
        let stage = stage(&d);
        let cx = Cx::new(None);
        let mut source = archive::open(path).unwrap();
        assert_eq!(source.list(&cx).unwrap(), None);
        source.extract(&stage, &cx).unwrap();
        let failed = if path == &newc { vec!["../evil", "dev/null"] } else { vec!["../evil"] };
        assert_eq!(cx.failed(), failed, "{}", path.display());
        assert_eq!(tree(&stage), files(&[("etc/big.bin", &big), ("run.sh", b"echo\n")]), "{}", path.display());
        let modified = std::fs::metadata(stage.join("etc/big.bin")).unwrap().modified().unwrap();
        assert_eq!(modified, SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530));
    }
    // Binary cpio is 7-Zip's.
    let mut binary = vec![0xC7, 0x71];
    binary.resize(600, 0);
    std::fs::write(d.join("c.cpio"), binary).unwrap();
    assert_eq!(archive::open(&d.join("c.cpio")).err().unwrap().kind(), std::io::ErrorKind::Unsupported);
    let _ = std::fs::remove_dir_all(&d);
}

/// A tar of `entries` (names, contents), with a `./` folder first as dpkg-deb writes it.
fn deb_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Directory);
    h.set_mode(0o755);
    h.set_size(0);
    b.append_data(&mut h, "./", std::io::empty()).unwrap();
    for (name, bytes) in entries {
        let mut h = tar::Header::new_gnu();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        b.append_data(&mut h, name, *bytes).unwrap();
    }
    b.into_inner().unwrap()
}

#[test]
fn deb_layout() {
    let d = dir("deb");
    let big = noise(200_000, 61);
    let path = d.join("gezik_1.0_amd64.deb");
    {
        let data = deb_tar(&[("./usr/share/gezik/big.bin", &big)]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&data).unwrap();
        let data_gz = gz.finish().unwrap();
        let control = deb_tar(&[("./control", b"Package: gezik\n")]);
        let mut xz = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(1)).unwrap();
        xz.write_all(&control).unwrap();
        let control_xz = xz.finish().unwrap();
        let mut a = ar::Builder::new(std::fs::File::create(&path).unwrap());
        let members = [("debian-binary", b"2.0\n".to_vec()), ("control.tar.xz", control_xz), ("data.tar.gz", data_gz)];
        for (name, bytes) in members {
            let mut h = ar::Header::new(name.as_bytes().to_vec(), bytes.len() as u64);
            h.set_mode(0o100644);
            a.append(&h, bytes.as_slice()).unwrap();
        }
    }
    let stage = stage(&d);
    let cx = Cx::new(None);
    extract(&path, &stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), files(&[("DEBIAN/control", b"Package: gezik\n"), ("usr/share/gezik/big.bin", &big)]));

    // A plain ar: its members are the files.
    let plain = d.join("lib.a");
    {
        let mut a = ar::Builder::new(std::fs::File::create(&plain).unwrap());
        for (name, bytes) in [("one.o", &b"one"[..]), ("two.o", &b"second"[..])] {
            let mut h = ar::Header::new(name.as_bytes().to_vec(), bytes.len() as u64);
            h.set_mode(0o100644);
            h.set_mtime(1_715_953_530);
            a.append(&h, bytes).unwrap();
        }
    }
    let stage = self::stage(&d);
    let cx = Cx::new(None);
    let mut source = archive::open(&plain).unwrap();
    assert_eq!(source.list(&cx).unwrap().unwrap().len(), 2);
    source.extract(&stage, &cx).unwrap();
    assert!(cx.failed().is_empty(), "{:?}", cx.failed());
    assert_eq!(tree(&stage), files(&[("one.o", b"one"), ("two.o", b"second")]));
    let modified = std::fs::metadata(stage.join("one.o")).unwrap().modified().unwrap();
    assert_eq!(modified, SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn udf_and_rare_formats_need_seven_zip() {
    use gezik_core::batch::archive::Format;
    assert!(!archive::supported(&Format::Udf));
    assert!(!archive::supported(&Format::Other("lzh".into())));
    for format in [Format::Cab, Format::Iso, Format::Cpio, Format::Ar, Format::Deb] {
        assert!(archive::supported(&format), "{format:?}");
    }
    // Without the `rar` feature (no C++ compiler for the target) RAR goes to 7-Zip.
    assert_eq!(archive::supported(&Format::Rar), cfg!(feature = "rar"));
    let d = dir("rare");
    std::fs::write(d.join("a.lzh"), b"\x1a\x00-lh5-\x10\x00\x00\x00").unwrap();
    // A UDF-only image: the UDF descriptors without an ISO 9660 one.
    let mut udf = vec![0u8; 0x9800];
    udf[0x8001..0x8006].copy_from_slice(b"BEA01");
    udf[0x8801..0x8806].copy_from_slice(b"NSR02");
    std::fs::write(d.join("win.iso"), udf).unwrap();
    for name in ["a.lzh", "win.iso"] {
        let err = archive::open(&d.join(name)).err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported, "{name}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

fn crc(bytes: &[u8]) -> [u8; 4] {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum().to_le_bytes()
}

/// A few bytes asking for a 4 GiB dictionary allocate nothing: xz refuses, 7z goes to 7-Zip.
#[test]
fn a_huge_dictionary_is_not_allocated() {
    let d = dir("dict");
    // xz: stream header (CRC32 check), then a block header whose LZMA2 filter says 4 GiB.
    let mut xz = b"\xFD7zXZ\0\0\x01".to_vec();
    xz.extend(crc(b"\0\x01"));
    let block = [0x02, 0x00, 0x21, 0x01, 40, 0, 0, 0];
    xz.extend(block);
    xz.extend(crc(&block));
    xz.extend([0; 16]);
    std::fs::write(d.join("a.xz"), xz).unwrap();
    let cx = Cx::new(None);
    let err = extract(&d.join("a.xz"), &stage(&d), &cx).err().unwrap();
    assert!(err.to_string().contains("memory"), "{err}");

    // 7z: one byte packed by LZMA2 with a 4 GiB dictionary, a plain header naming file `a`.
    let header = [
        0x01, 0x04, 0x06, 0x00, 0x01, 0x09, 0x01, 0x00, 0x07, 0x0B, 0x01, 0x00, 0x01, 0x21, 0x21, 0x01, 40, 0x0C, 0x01,
        0x00, 0x08, 0x00, 0x00, 0x05, 0x01, 0x11, 0x05, 0x00, b'a', 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let mut start = 1u64.to_le_bytes().to_vec();
    start.extend((header.len() as u64).to_le_bytes());
    start.extend(crc(&header));
    let mut sz = b"7z\xBC\xAF\x27\x1C\0\x04".to_vec();
    sz.extend(crc(&start));
    sz.extend(start);
    sz.push(0);
    sz.extend(header);
    std::fs::write(d.join("a.7z"), sz).unwrap();
    let err = archive::open(&d.join("a.7z")).err().unwrap();
    assert_eq!(err.to_string(), "7-Zip needed");
    let _ = std::fs::remove_dir_all(&d);
}
