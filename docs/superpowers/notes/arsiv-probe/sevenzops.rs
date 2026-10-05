use std::fs::{self, File};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
use sevenz_rust2::{
    Archive, ArchiveEntry, ArchiveReader, ArchiveWriter, BlockDecoder, EncoderConfiguration,
    Error as SzError, NtTime, Password, SourceReader, prepare_block,
};

use crate::{MultiFileReader, fresh_dir, safe_join, sample};

/// Write + Seek that cuts the output into `base.001`, `base.002`, ... of `vol` bytes each.
/// ArchiveWriter seeks back to offset 0 in finish() to write the start header, so a plain
/// "rotate on size" writer is not enough; this one maps every offset to (volume, offset).
pub struct SplitWriter {
    base: PathBuf,
    vol: u64,
    files: Vec<File>,
    pos: u64,
}

impl SplitWriter {
    pub fn new(base: PathBuf, vol: u64) -> Self {
        Self { base, vol, files: Vec::new(), pos: 0 }
    }
    fn file(&mut self, idx: usize) -> io::Result<&mut File> {
        while self.files.len() <= idx {
            let p = PathBuf::from(format!("{}.{:03}", self.base.display(), self.files.len() + 1));
            self.files.push(File::options().read(true).write(true).create(true).truncate(true).open(p)?);
        }
        Ok(&mut self.files[idx])
    }
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let idx = (self.pos / self.vol) as usize;
        let off = self.pos % self.vol;
        let room = (self.vol - off) as usize;
        let n = buf.len().min(room);
        let f = self.file(idx)?;
        f.seek(SeekFrom::Start(off))?;
        f.write_all(&buf[..n])?;
        self.pos += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.files.iter_mut().try_for_each(|f| f.flush())
    }
}

impl Seek for SplitWriter {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.pos = match to {
            SeekFrom::Start(p) => p,
            SeekFrom::Current(d) => (self.pos as i64 + d) as u64,
            SeekFrom::End(_) => return Err(io::Error::other("SeekFrom::End unsupported")),
        };
        Ok(self.pos)
    }
}

fn entry(name: &str, mtime: SystemTime) -> ArchiveEntry {
    let mut e = ArchiveEntry::new_file(name);
    e.last_modified_date = NtTime::try_from(mtime).unwrap();
    e.has_last_modified_date = true;
    e.windows_attributes = 0x20; // FILE_ATTRIBUTE_ARCHIVE
    e.has_windows_attributes = true;
    e
}

/// Extract every entry of an open reader under `dest` with byte progress.
/// The closure MUST drain every reader it is given: entries in a solid block share one
/// decoder stream, and an undrained entry shifts the next entry's bytes.
pub fn extract_all<R: Read + Seek>(
    rd: &mut ArchiveReader<R>,
    dest: &Path,
    mut on_progress: impl FnMut(&str, u64, u64),
) -> Result<u64, SzError> {
    let total: u64 = rd.archive().files.iter().filter(|e| e.has_stream()).map(|e| e.size()).sum();
    let mut done = 0u64;
    rd.for_each_entries(|e, r| {
        let Some(path) = safe_join(dest, e.name()) else {
            io::copy(r, &mut io::sink())?; // drain, then skip
            return Ok(true);
        };
        if e.is_directory() {
            fs::create_dir_all(&path)?;
            return Ok(true);
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let mut f = File::create(&path)?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n])?;
            done += n as u64;
            on_progress(e.name(), done, total);
        }
        if e.has_last_modified_date {
            let _ = f.set_modified(e.last_modified_date().into());
        }
        Ok(true) // Ok(false) stops the current *block* only (see digest gotchas)
    })?;
    Ok(done)
}

pub fn run(base: &Path) {
    let dir = fresh_dir(base, "7z");
    let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1_715_953_530);
    let files: Vec<(String, Vec<u8>)> =
        (0..6).map(|k| (format!("dir/file{k}.bin"), sample(400_000 + k as usize * 1000, 30 + k))).collect();

    // ---------- 1. non-solid, LZMA2 level 7, AES-256 + encrypted header ----------
    let enc_path = dir.join("enc.7z");
    {
        let mut w = ArchiveWriter::create(&enc_path).unwrap();
        let mut aes = AesEncoderOptions::new(Password::new("Passw0rd"));
        aes.num_cycles_power = 19; // crate default is 8 (2^8 SHA-256 rounds); 7-Zip writes 19
        w.set_content_methods(vec![
            aes.into(), // AES first, then codec
            Lzma2Options::from_level(7).into(),
        ]);
        w.set_encrypt_header(true); // default is already true; only effective when AES is in the methods
        w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("dir"), None).unwrap();
        for (name, data) in &files {
            let e = w.push_archive_entry(entry(name, mtime), Some(data.as_slice())).unwrap();
            assert_eq!(e.size, data.len() as u64);
        }
        w.finish().unwrap();
    }

    // listing an encrypted-header archive needs the password; without it -> PasswordRequired
    match Archive::open(&enc_path) {
        Err(SzError::PasswordRequired) => println!("7z: header-encrypted, no pw -> PasswordRequired"),
        other => panic!("expected PasswordRequired, got {:?}", other.map(|a| a.files.len())),
    }
    match ArchiveReader::open(&enc_path, Password::new("wrong")) {
        Err(e) => println!("7z: wrong pw on open -> {e:?}"),
        Ok(_) => panic!("wrong password opened"),
    }
    let ar = Archive::open_with_password(&enc_path, &Password::new("Passw0rd")).unwrap();
    println!("7z: solid={} blocks={} files={}", ar.is_solid, ar.blocks.len(), ar.files.len());
    for e in &ar.files {
        let mt: SystemTime = e.last_modified_date().into();
        println!(
            "7z: {:<16} dir={} size={:>7} packed={:>7} attrs={:#x} has_mtime={} mtime={:?}",
            e.name(), e.is_directory(), e.size(), e.compressed_size, e.windows_attributes(),
            e.has_last_modified_date, mt.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
        );
    }

    let mut rd = ArchiveReader::open(&enc_path, Password::new("Passw0rd")).unwrap();
    let out = dir.join("out-enc");
    let mut ticks = 0;
    let n = extract_all(&mut rd, &out, |_, _, _| ticks += 1).unwrap();
    assert_eq!(n, files.iter().map(|f| f.1.len() as u64).sum::<u64>());
    for (name, data) in &files {
        assert_eq!(&fs::read(out.join(name)).unwrap(), data);
    }
    println!("7z: enc.7z extracted, {ticks} progress ticks");
    // single file by name
    let one = rd.read_file("dir/file3.bin").unwrap();
    assert_eq!(one, files[3].1);

    // data-only encryption (header readable without password); wrong pw shows at extract time
    let enc2 = dir.join("enc-data-only.7z");
    {
        let mut w = ArchiveWriter::create(&enc2).unwrap();
        w.set_content_methods(vec![AesEncoderOptions::new("pw".into()).into(), Lzma2Options::from_level(5).into()]);
        w.set_encrypt_header(false);
        w.push_archive_entry(entry("a.bin", mtime), Some(files[0].1.as_slice())).unwrap();
        w.finish().unwrap();
    }
    let listed = Archive::open(&enc2).unwrap(); // works without password
    assert_eq!(listed.files[0].name(), "a.bin");
    let mut rd = ArchiveReader::open(&enc2, Password::empty()).unwrap();
    let r = rd.for_each_entries(|_, r| { io::copy(r, &mut io::sink())?; Ok(true) });
    println!("7z: data-only enc, no pw, extract -> {:?}", r.err());
    let mut rd = ArchiveReader::open(&enc2, "bad".into()).unwrap();
    let r = rd.for_each_entries(|_, r| { io::copy(r, &mut io::sink())?; Ok(true) });
    println!("7z: data-only enc, bad pw, extract -> {:?}", r.err());

    // ---------- 2. solid block, multithreaded LZMA2 ----------
    let solid_path = dir.join("solid.7z");
    {
        let mut w = ArchiveWriter::create(&solid_path).unwrap();
        // 4 threads; chunk (independent LZMA2 stream) is clamped to >= dict size.
        w.set_content_methods(vec![Lzma2Options::from_level_mt(6, 4, 1 << 20).into()]);
        let entries: Vec<ArchiveEntry> = files.iter().map(|(n, _)| entry(n, mtime)).collect();
        let readers: Vec<SourceReader<&[u8]>> = files.iter().map(|(_, d)| SourceReader::new(d.as_slice())).collect();
        w.push_archive_entries(entries, readers).unwrap();
        w.finish().unwrap();
    }
    let ar = Archive::open(&solid_path).unwrap();
    assert!(ar.is_solid && ar.blocks.len() == 1);
    let mut rd = ArchiveReader::open(&solid_path, Password::empty()).unwrap();
    rd.set_thread_count(4);
    let out = dir.join("out-solid");
    extract_all(&mut rd, &out, |_, _, _| {}).unwrap();
    assert_eq!(fs::read(out.join("dir/file5.bin")).unwrap(), files[5].1);

    // ---------- 3. several solid blocks compressed on worker threads ----------
    let par_path = dir.join("parallel-blocks.7z");
    {
        let methods: Arc<Vec<EncoderConfiguration>> = Arc::new(vec![Lzma2Options::from_level(6).into()]);
        let blocks = std::thread::scope(|s| {
            let hs: Vec<_> = files
                .chunks(2)
                .map(|chunk| {
                    let methods = methods.clone();
                    s.spawn(move || {
                        let entries = chunk.iter().map(|(n, _)| entry(n, mtime)).collect();
                        let readers = chunk.iter().map(|(_, d)| SourceReader::new(Cursor::new(d.clone()))).collect();
                        prepare_block(methods, entries, readers).unwrap()
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
        });
        let mut w = ArchiveWriter::create(&par_path).unwrap();
        w.set_content_methods(methods.as_ref().clone());
        for b in blocks {
            w.push_prepared_block(b).unwrap(); // order of pushes = order in the archive
        }
        w.finish().unwrap();
    }
    let ar = Archive::open(&par_path).unwrap();
    println!("7z: parallel-blocks.7z blocks={} solid={}", ar.blocks.len(), ar.is_solid);
    // decode blocks in parallel: one BlockDecoder (own file handle) per block
    let ar = Arc::new(ar);
    let pw = Password::empty();
    std::thread::scope(|s| {
        for bi in 0..ar.blocks.len() {
            let (ar, pw, par_path, out) = (ar.clone(), &pw, &par_path, dir.join("out-par"));
            s.spawn(move || {
                let mut src = File::open(par_path).unwrap();
                BlockDecoder::new(1, bi, &ar, pw, &mut src)
                    .for_each_entries(&mut |e, r| {
                        let p = safe_join(&out, e.name()).unwrap();
                        fs::create_dir_all(p.parent().unwrap())?;
                        io::copy(r, &mut File::create(p)?)?;
                        Ok(true)
                    })
                    .unwrap();
            });
        }
    });
    assert_eq!(fs::read(dir.join("out-par/dir/file4.bin")).unwrap(), files[4].1);

    // ---------- 4. volumes: write .7z.001.. with SplitWriter, read back via MultiFileReader ----------
    let vol_base = dir.join("vol.7z");
    {
        let mut w = ArchiveWriter::new(SplitWriter::new(vol_base.clone(), 300_000)).unwrap();
        w.set_content_methods(vec![
            AesEncoderOptions::new("Passw0rd".into()).into(),
            Lzma2Options::from_level(1).into(),
        ]);
        for (name, data) in &files {
            w.push_archive_entry(entry(name, mtime), Some(data.as_slice())).unwrap();
        }
        w.finish().unwrap().flush().unwrap();
    }
    let first = dir.join("vol.7z.001");
    let vols = MultiFileReader::volumes(&first);
    let mut rd = ArchiveReader::new(MultiFileReader::open(&vols).unwrap(), "Passw0rd".into()).unwrap();
    let out = dir.join("out-vol");
    extract_all(&mut rd, &out, |_, _, _| {}).unwrap();
    assert_eq!(fs::read(out.join("dir/file2.bin")).unwrap(), files[2].1);
    println!("7z: volumes written+read: {}", vols.len());
    println!("7z: OK");
}

/// Reads an external .7z (e.g. made by 7-Zip) and returns (name, size) after a full decode.
pub fn read_external(path: &Path, pw: &str) -> Result<Vec<(String, u64)>, SzError> {
    let mut rd = if path.to_string_lossy().ends_with(".001") {
        let v = MultiFileReader::volumes(path);
        let a = Archive::read(&mut MultiFileReader::open(&v)?, &Password::new(pw))?;
        let mut out = Vec::new();
        let mut r = ArchiveReader::from_archive(a, MultiFileReader::open(&v)?, Password::new(pw));
        r.for_each_entries(|e, rd| {
            out.push((e.name().to_owned(), io::copy(rd, &mut io::sink())?));
            Ok(true)
        })?;
        return Ok(out);
    } else {
        ArchiveReader::open(path, Password::new(pw))?
    };
    let mut out = Vec::new();
    rd.for_each_entries(|e, r| {
        out.push((e.name().to_owned(), io::copy(r, &mut io::sink())?));
        Ok(true)
    })?;
    Ok(out)
}
