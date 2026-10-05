use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};
use tar::{Archive, Builder, EntryType, Header};

use crate::{fresh_dir, sample};

/// Read wrapper that reports bytes read: progress for Builder::append_data (the builder
/// pulls from the reader) and for decompression (wrap the *compressed file* and compare
/// against its length: the uncompressed size of a .tar.gz/.xz/.zst is not known up front).
pub struct ReadProgress<R, F: FnMut(u64)> {
    pub inner: R,
    pub done: u64,
    pub on_progress: F,
}
impl<R: Read, F: FnMut(u64)> Read for ReadProgress<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.done += n as u64;
        (self.on_progress)(self.done);
        Ok(n)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Codec {
    None,
    Gz,
    Xz,
    Bz2,
    Zst,
}

/// Wrap a raw compressed stream in the right decoder. All are pure Rust.
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

/// Multi-frame zstd reader on ruzstd. `StreamingDecoder` stops after one frame; files from
/// pzstd / `cat a.zst b.zst` / seekable-zstd have many, and may contain skippable frames.
pub struct ZstdReader<R: BufRead> {
    src: R,
    dec: FrameDecoder,
    in_frame: bool,
}
impl<R: BufRead> ZstdReader<R> {
    pub fn new(src: R) -> Self {
        let mut dec = FrameDecoder::new();
        // Default max window is ruzstd::decoding::DEFAULT_MAX_WINDOW_SIZE; `zstd --long=31`
        // archives need 2 GiB. Raise deliberately (memory!) if Gezik wants them.
        dec.set_max_window_size(1 << 31);
        Self { src, dec, in_frame: false }
    }
}
impl<R: BufRead> Read for ZstdReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.in_frame {
                while self.dec.can_collect() < buf.len() && !self.dec.is_finished() {
                    let need = buf.len() - self.dec.can_collect();
                    self.dec
                        .decode_blocks(&mut self.src, BlockDecodingStrategy::UptoBytes(need))
                        .map_err(io::Error::other)?;
                }
                let n = self.dec.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                self.in_frame = false; // frame exhausted
            }
            if self.src.fill_buf()?.is_empty() {
                return Ok(0);
            }
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

pub fn codec_for(name: &str) -> Codec {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
        Codec::Gz
    } else if n.ends_with(".tar.xz") || n.ends_with(".txz") {
        Codec::Xz
    } else if n.ends_with(".tar.bz2") || n.ends_with(".tbz2") || n.ends_with(".tbz") {
        Codec::Bz2
    } else if n.ends_with(".tar.zst") || n.ends_with(".tzst") {
        Codec::Zst
    } else {
        Codec::None
    }
}

/// List entries: path (lossy), type, size, mode, mtime, link target.
pub fn list(path: &Path) -> io::Result<Vec<String>> {
    let f = BufReader::new(File::open(path)?);
    let mut ar = Archive::new(decoder(codec_for(&path.to_string_lossy()), f));
    let mut out = Vec::new();
    for e in ar.entries()? {
        let e = e?;
        let h = e.header();
        out.push(format!(
            "{:?} {:?} size={} mode={:o} mtime={:?} link={:?}",
            e.path()?, // PAX/GNU long names already applied
            h.entry_type(),
            e.size(),
            h.mode().unwrap_or(0), // a blank numeric field is an Err, not 0: don't `?` it
            h.mtime().ok(),
            e.link_name()?.map(|l| l.into_owned()),
        ));
    }
    Ok(out)
}

/// Extract with per-entry progress. unpack_in() refuses `..`/absolute paths (returns false)
/// and checks symlink targets stay inside `dest`.
pub fn extract(path: &Path, dest: &Path, mut on_entry: impl FnMut(&Path, bool)) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    let len = fs::metadata(path)?.len();
    let f = ReadProgress {
        inner: File::open(path)?,
        done: 0,
        on_progress: move |d: u64| {
            let _pct = d * 100 / len.max(1); // compressed-bytes progress
        },
    };
    let mut ar = Archive::new(decoder(codec_for(&path.to_string_lossy()), BufReader::new(f)));
    ar.set_preserve_mtime(true); // default true
    ar.set_preserve_permissions(false); // default false; only meaningful on Unix
    ar.set_overwrite(true); // default true
    for e in ar.entries()? {
        let mut e = e?;
        let p = e.path()?.into_owned();
        let is_link = matches!(e.header().entry_type(), EntryType::Symlink | EntryType::Link);
        match e.unpack_in(dest) {
            Ok(written) => on_entry(&p, written),
            // Windows without Developer Mode / SeCreateSymbolicLinkPrivilege: ERROR_PRIVILEGE_NOT_HELD.
            // tar wraps the io::Error in its TarError (Display shows only "failed to unpack"),
            // so raw_os_error() is None; the code is only visible in the Debug text.
            Err(err) if is_link && cfg!(windows) && format!("{err:?}").contains("os error 1314") => on_entry(&p, false),
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

pub fn run(base: &Path) {
    let dir = fresh_dir(base, "tar");
    let big = sample(2 << 20, 7);
    let mtime = 1_715_953_530u64;

    // ---------- build a tar in memory with Builder (progress = bytes pulled from the source) ----------
    let mut tar_bytes = Vec::new();
    {
        let mut b = Builder::new(&mut tar_bytes);
        b.mode(tar::HeaderMode::Complete); // keep mtime/mode as given (Deterministic zeroes them)

        let mut h = Header::new_gnu();
        h.set_entry_type(EntryType::Directory);
        h.set_mode(0o755);
        h.set_mtime(mtime);
        h.set_size(0);
        b.append_data(&mut h, "docs/", io::empty()).unwrap(); // append_data sets path + cksum

        let mut h = Header::new_gnu();
        h.set_size(big.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(mtime);
        let mut ticks = 0;
        let src = ReadProgress { inner: big.as_slice(), done: 0, on_progress: |_| ticks += 1 };
        // long names (>100 bytes) get a GNU long-name record automatically
        let long = format!("docs/{}/big.bin", "x".repeat(120));
        b.append_data(&mut h, &long, src).unwrap();

        let mut h = Header::new_gnu();
        h.set_size(5);
        h.set_mode(0o755);
        h.set_mtime(mtime);
        b.append_data(&mut h, "run.sh", &b"echo\n"[..]).unwrap();

        let mut h = Header::new_gnu();
        h.set_entry_type(EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        h.set_mtime(mtime);
        b.append_link(&mut h, "link-to-run", "run.sh").unwrap();

        // A symlink pointing outside, then a file through it: unpack_in must not escape.
        let mut h = Header::new_gnu();
        h.set_entry_type(EntryType::Symlink);
        h.set_size(0);
        b.append_link(&mut h, "escape", "../../outside").unwrap();

        // `..` in a name: set_path refuses it, so write the raw name bytes.
        let mut h = Header::new_gnu();
        h.set_size(4);
        h.set_mtime(mtime);
        h.as_old_mut().name[..10].copy_from_slice(b"../evil.tx");
        h.set_cksum();
        b.append(&h, &b"evil"[..]).unwrap();

        b.finish().unwrap();
        println!("tar: builder progress ticks {ticks}");
    }
    fs::write(dir.join("probe.tar"), &tar_bytes).unwrap();

    // ---------- compress to each codec ----------
    {
        let mut gz = flate2::write::GzEncoder::new(File::create(dir.join("probe.tar.gz")).unwrap(), flate2::Compression::new(6));
        gz.write_all(&tar_bytes).unwrap();
        gz.finish().unwrap();

        let mut xz = lzma_rust2::XzWriter::new(File::create(dir.join("probe.tar.xz")).unwrap(), lzma_rust2::XzOptions::with_preset(6)).unwrap();
        xz.write_all(&tar_bytes).unwrap();
        xz.finish().unwrap();

        // multithreaded xz: needs a block size; each block compresses independently
        let mut o = lzma_rust2::XzOptions::with_preset(6);
        o.set_block_size(std::num::NonZeroU64::new(1 << 20));
        let mut xzmt = lzma_rust2::XzWriterMt::new(File::create(dir.join("probe-mt.tar.xz")).unwrap(), o, 4).unwrap();
        xzmt.write_all(&tar_bytes).unwrap();
        xzmt.finish().unwrap();

        let mut bz = bzip2::write::BzEncoder::new(File::create(dir.join("probe.tar.bz2")).unwrap(), bzip2::Compression::new(9));
        bz.write_all(&tar_bytes).unwrap();
        bz.finish().unwrap();

        // ruzstd's encoder pulls from a Read; two frames back to back to test multi-frame decode
        let mut zst = ruzstd::encoding::compress_to_vec(&tar_bytes[..10240], ruzstd::encoding::CompressionLevel::Fastest);
        zst.extend(ruzstd::encoding::compress_to_vec(&tar_bytes[10240..], ruzstd::encoding::CompressionLevel::Fastest));
        fs::write(dir.join("probe.tar.zst"), zst).unwrap();
    }

    for name in ["probe.tar", "probe.tar.gz", "probe.tar.xz", "probe-mt.tar.xz", "probe.tar.bz2", "probe.tar.zst"] {
        let p = dir.join(name);
        let l = list(&p).unwrap();
        if name == "probe.tar" {
            for x in &l {
                println!("tar: {x}");
            }
        }
        let out = dir.join(format!("out-{name}"));
        let mut skipped = Vec::new();
        extract(&p, &out, |p, w| if !w { skipped.push(p.to_path_buf()) }).unwrap();
        let bigp = out.join(format!("docs/{}/big.bin", "x".repeat(120)));
        assert_eq!(fs::read(&bigp).unwrap(), big, "{name}");
        let mt = fs::metadata(&bigp).unwrap().modified().unwrap();
        assert_eq!(mt.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), mtime);
        assert!(!dir.join("evil.tx").exists() && !base.join("outside").exists());
        println!("tar: {name}: {} entries, skipped {:?}", l.len(), skipped);
    }
    println!("tar: OK");
}
