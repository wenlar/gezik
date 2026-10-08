//! Reading and writing helpers the archive formats share: volumes read as one stream, a
//! writer that cuts its output into volumes, and the stream decoders of tar and single files.

use std::fs::File;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use gezik_core::batch::archive::Codec;
use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};

/// Read + Seek over byte-split volumes (`x.7z.001`, `x.7z.002`… or 7-Zip's `x.zip.001`): such
/// volumes are plain cuts, so the files one after another are the archive.
pub struct MultiFileReader {
    /// (file, where it starts in the whole, its length)
    files: Vec<(File, u64, u64)>,
    pos: u64,
    total: u64,
}

impl MultiFileReader {
    pub fn open(paths: &[PathBuf]) -> io::Result<Self> {
        let mut files = Vec::new();
        let mut start = 0;
        for p in paths {
            let f = File::open(p)?;
            let len = f.metadata()?.len();
            files.push((f, start, len));
            start += len;
        }
        Ok(Self { files, pos: 0, total: start })
    }
}

impl Read for MultiFileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos = self.pos;
        let Some((f, start, len)) = self.files.iter_mut().find(|(_, s, l)| pos >= *s && pos < *s + *l) else {
            return Ok(0);
        };
        f.seek(SeekFrom::Start(pos - *start))?;
        let max = ((*start + *len - pos) as usize).min(buf.len());
        let n = f.read(&mut buf[..max])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for MultiFileReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let p = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.total as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

/// Write + Seek that cuts its output into `base.001`, `base.002`… of `vol` bytes each. A 7z
/// writer seeks back to the start to write its header when it finishes, so every offset is
/// mapped to (volume, offset) instead of rotating files by size.
pub struct SplitWriter {
    base: PathBuf,
    vol: u64,
    files: Vec<File>,
    pos: u64,
    /// The furthest byte written: where `SeekFrom::End` counts from.
    len: u64,
}

impl SplitWriter {
    pub fn new(base: PathBuf, vol: u64) -> Self {
        Self { base, vol: vol.max(1), files: Vec::new(), pos: 0, len: 0 }
    }

    /// The volumes written so far.
    pub fn paths(&self) -> Vec<PathBuf> {
        (1..=self.files.len()).map(|i| self.volume(i)).collect()
    }

    fn volume(&self, n: usize) -> PathBuf {
        let mut name = self.base.as_os_str().to_owned();
        name.push(format!(".{n:03}"));
        PathBuf::from(name)
    }

    fn file(&mut self, idx: usize) -> io::Result<&mut File> {
        while self.files.len() <= idx {
            let p = self.volume(self.files.len() + 1);
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
        self.len = self.len.max(self.pos);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.files.iter_mut().try_for_each(|f| f.flush())
    }
}

impl Seek for SplitWriter {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let p = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

/// Reads up to `buf.len()` bytes, fewer only at the end of the stream.
pub(crate) fn read_full(src: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match src.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// `r` (a compressed stream) decoded by `codec`. Concatenated members (pigz, pbzip2, `cat
/// a.gz b.gz`) decode in full.
pub fn decoder<'a, R: BufRead + 'a>(codec: Codec, r: R) -> Box<dyn Read + 'a> {
    match codec {
        Codec::None => Box::new(r),
        Codec::Gz => Box::new(flate2::bufread::MultiGzDecoder::new(r)),
        Codec::Bz2 => Box::new(bzip2::bufread::MultiBzDecoder::new(r)),
        // A tiny file may ask for a 4 GiB dictionary: 1.5 GiB (xz -9 needs 64 MiB) at most.
        Codec::Xz => Box::new(lzma_rust2::XzReader::new_mem_limit(r, true, 1_572_864)),
        Codec::Zst => Box::new(ZstdReader::new(r)),
    }
}

/// A zstd stream of any number of frames (ruzstd's `StreamingDecoder` stops after one;
/// pzstd and `cat a.zst b.zst` write several), skipping skippable frames.
pub struct ZstdReader<R: BufRead> {
    src: R,
    dec: FrameDecoder,
    in_frame: bool,
}

impl<R: BufRead> ZstdReader<R> {
    pub fn new(src: R) -> Self {
        let mut dec = FrameDecoder::new();
        // `zstd --long=31` streams need a 2 GiB window.
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
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                }
                let n = self.dec.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                self.in_frame = false;
            }
            if self.src.fill_buf()?.is_empty() {
                return Ok(0);
            }
            match self.dec.reset(&mut self.src) {
                Ok(()) => self.in_frame = true,
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame { length, .. })) => {
                    io::copy(&mut (&mut self.src).take(u64::from(length)), &mut io::sink())?;
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gezik-batch-io-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn multi_file_reader_reads_parts_as_one_stream_and_seeks() {
        let d = dir("multi");
        let data: Vec<u8> = (0..2500u32).map(|i| (i % 251) as u8).collect();
        let parts: Vec<PathBuf> = data
            .chunks(1000)
            .enumerate()
            .map(|(i, chunk)| {
                let p = d.join(format!("a.7z.{:03}", i + 1));
                std::fs::write(&p, chunk).unwrap();
                p
            })
            .collect();
        let mut r = MultiFileReader::open(&parts).unwrap();
        let mut all = Vec::new();
        r.read_to_end(&mut all).unwrap();
        assert_eq!(all, data);

        // Across a part boundary, from the end and relative.
        r.seek(SeekFrom::Start(995)).unwrap();
        let mut ten = [0u8; 10];
        r.read_exact(&mut ten).unwrap();
        assert_eq!(ten[..], data[995..1005]);
        assert_eq!(r.seek(SeekFrom::End(-3)).unwrap(), 2497);
        let mut tail = Vec::new();
        r.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, data[2497..]);
        assert_eq!(r.seek(SeekFrom::Current(-1500)).unwrap(), 1000);
        r.read_exact(&mut ten).unwrap();
        assert_eq!(ten[..], data[1000..1010]);
        assert!(r.seek(SeekFrom::Current(-5000)).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn split_writer_cuts_volumes_and_writes_where_it_seeks() {
        let d = dir("split");
        let base = d.join("b.7z");
        let mut w = SplitWriter::new(base.clone(), 100);
        w.write_all(&[1u8; 250]).unwrap();
        // Back to the start (a header), then to the end to go on.
        w.seek(SeekFrom::Start(0)).unwrap();
        w.write_all(b"HEAD").unwrap();
        w.seek(SeekFrom::Start(98)).unwrap();
        w.write_all(b"xyzw").unwrap();
        assert_eq!(w.seek(SeekFrom::End(0)).unwrap(), 250);
        w.write_all(&[2u8; 60]).unwrap();
        w.flush().unwrap();
        let paths = w.paths();
        drop(w);
        assert_eq!(paths.len(), 4);
        let sizes: Vec<u64> = paths.iter().map(|p| std::fs::metadata(p).unwrap().len()).collect();
        assert_eq!(sizes, [100, 100, 100, 10]);

        let mut want = vec![1u8; 250];
        want[..4].copy_from_slice(b"HEAD");
        want[98..102].copy_from_slice(b"xyzw");
        want.extend([2u8; 60]);
        let mut back = Vec::new();
        MultiFileReader::open(&paths).unwrap().read_to_end(&mut back).unwrap();
        assert_eq!(back, want);
        let _ = std::fs::remove_dir_all(&d);
    }
}
