//! Probes for the archive crates Gezik 5b will use. Each module has `run(dir)`, which
//! writes and reads real archives under `dir` and panics on any mismatch.

pub mod detect;
pub mod miscops;
pub mod rarops;
pub mod sevenzops;
pub mod tarops;
pub mod zipops;

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

/// A writer wrapper that reports bytes written: the progress hook used by every probe.
pub struct Progress<W, F: FnMut(u64)> {
    pub inner: W,
    pub done: u64,
    pub on_progress: F,
}

impl<W: Write, F: FnMut(u64)> Write for Progress<W, F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.done += n as u64;
        (self.on_progress)(self.done);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Joins an untrusted entry name under `dest`, refusing `..`, roots and drive prefixes.
/// Treats `\` as a separator too. Returns None for a name that would escape.
pub fn safe_join(dest: &Path, entry_name: &str) -> Option<PathBuf> {
    let normalized = entry_name.replace('\\', "/");
    let mut out = dest.to_path_buf();
    let mut any = false;
    for c in Path::new(&normalized).components() {
        match c {
            Component::Normal(p) => {
                out.push(p);
                any = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    any.then_some(out)
}

/// Deterministic, poorly compressible-ish sample data.
pub fn sample(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    (0..len)
        .map(|i| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            // Mix text-like runs with noise so codecs have something to do.
            if (i / 64) % 2 == 0 { b"gezik archive "[i % 14] } else { x as u8 }
        })
        .collect()
}

pub fn fresh_dir(base: &Path, name: &str) -> PathBuf {
    let d = base.join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Read + Seek over byte-split volumes (`x.7z.001`, `x.7z.002`, ... or 7-Zip's `x.zip.001`).
/// 7z volumes and 7-Zip's zip "split" are plain byte splits, so concatenation is the archive.
pub struct MultiFileReader {
    files: Vec<(std::fs::File, u64, u64)>, // (file, start offset, len)
    pos: u64,
    total: u64,
}

impl MultiFileReader {
    pub fn open(paths: &[PathBuf]) -> io::Result<Self> {
        let mut files = Vec::new();
        let mut start = 0;
        for p in paths {
            let f = std::fs::File::open(p)?;
            let len = f.metadata()?.len();
            files.push((f, start, len));
            start += len;
        }
        Ok(Self { files, pos: 0, total: start })
    }

    /// `base.001`, `base.002`, ... while they exist.
    pub fn volumes(first: &Path) -> Vec<PathBuf> {
        let s = first.to_string_lossy();
        let stem = s.strip_suffix(".001").expect("first volume ends in .001");
        (1..)
            .map(|i| PathBuf::from(format!("{stem}.{i:03}")))
            .take_while(|p| p.exists())
            .collect()
    }
}

impl io::Read for MultiFileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use io::Seek;
        let pos = self.pos;
        let Some((f, start, len)) = self.files.iter_mut().find(|(_, s, l)| pos >= *s && pos < *s + *l) else {
            return Ok(0);
        };
        f.seek(io::SeekFrom::Start(pos - *start))?;
        let max = ((*start + *len - pos) as usize).min(buf.len());
        let n = f.read(&mut buf[..max])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl io::Seek for MultiFileReader {
    fn seek(&mut self, to: io::SeekFrom) -> io::Result<u64> {
        let p = match to {
            io::SeekFrom::Start(p) => p as i128,
            io::SeekFrom::End(d) => self.total as i128 + d as i128,
            io::SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}
