//! The checksum core (spec 11 §3.2): several digests of a file in one read, the first bytes'
//! SHA-256, and comparing two files byte by byte. Checksums, copy verification, content
//! comparison, duplicates and joining files use it.

use std::cell::Cell;
use std::io::{self, Read};
use std::path::Path;

pub use gezik_core::checksum_file::Algo;
use gezik_platform::fs::{DIRECT_ALIGN, open_sequential, open_uncached, read_full};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

/// How much one read takes.
pub const CHUNK: usize = 1 << 20;

thread_local! {
    /// This thread's read buffers (`CHUNK` plus room to align them), made on its first file and
    /// gone with the thread: the engine's workers end with their task.
    static SPARE: Cell<Vec<Vec<u8>>> = const { Cell::new(Vec::new()) };
}

/// One of this thread's buffers, taken out (no borrow is held while it is used).
fn take_buffer() -> Vec<u8> {
    let mut spare = SPARE.take();
    let buffer = spare.pop();
    SPARE.set(spare);
    buffer.unwrap_or_else(|| vec![0; CHUNK + DIRECT_ALIGN])
}

fn give_back(buffer: Vec<u8>) {
    let mut spare = SPARE.take();
    spare.push(buffer);
    SPARE.set(spare);
}

/// The `CHUNK` bytes of `buffer` that start on a `DIRECT_ALIGN` boundary.
fn aligned(buffer: &mut [u8]) -> &mut [u8] {
    let start = buffer.as_ptr().align_offset(DIRECT_ALIGN).min(DIRECT_ALIGN);
    &mut buffer[start..start + CHUNK]
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "Cancelled")
}

/// The digests of one file, as lowercase hex.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Digests([Option<String>; 5]);

impl Digests {
    pub fn get(&self, algo: Algo) -> Option<&str> {
        self.0[algo.index()].as_deref()
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

/// Every chosen digest, each fed every chunk.
#[derive(Default)]
struct Hashers {
    crc32: Option<crc32fast::Hasher>,
    md5: Option<Md5>,
    sha1: Option<Sha1>,
    sha256: Option<Sha256>,
    sha512: Option<Sha512>,
}

impl Hashers {
    fn new(algos: &[Algo]) -> Hashers {
        let mut h = Hashers::default();
        for algo in algos {
            match algo {
                Algo::Crc32 => h.crc32 = Some(crc32fast::Hasher::new()),
                Algo::Md5 => h.md5 = Some(Md5::new()),
                Algo::Sha1 => h.sha1 = Some(Sha1::new()),
                Algo::Sha256 => h.sha256 = Some(Sha256::new()),
                Algo::Sha512 => h.sha512 = Some(Sha512::new()),
            }
        }
        h
    }

    fn update(&mut self, data: &[u8]) {
        if let Some(h) = &mut self.crc32 {
            h.update(data);
        }
        if let Some(h) = &mut self.md5 {
            h.update(data);
        }
        if let Some(h) = &mut self.sha1 {
            h.update(data);
        }
        if let Some(h) = &mut self.sha256 {
            h.update(data);
        }
        if let Some(h) = &mut self.sha512 {
            h.update(data);
        }
    }

    fn finish(self) -> Digests {
        let mut out = Digests::default();
        out.0[Algo::Crc32.index()] = self.crc32.map(|h| hex(&h.finalize().to_be_bytes()));
        out.0[Algo::Md5.index()] = self.md5.map(|h| hex(&h.finalize()));
        out.0[Algo::Sha1.index()] = self.sha1.map(|h| hex(&h.finalize()));
        out.0[Algo::Sha256.index()] = self.sha256.map(|h| hex(&h.finalize()));
        out.0[Algo::Sha512.index()] = self.sha512.map(|h| hex(&h.finalize()));
        out
    }
}

/// `algos`' digests of the file at `path`, read once from start to end. `progress` hears the
/// bytes of each chunk read; false stops it (an `Interrupted` error).
pub fn hash_file(path: &Path, algos: &[Algo], progress: &mut dyn FnMut(u64) -> bool) -> io::Result<Digests> {
    hash_reader(&mut open_sequential(path)?, algos, progress)
}

/// [`hash_file`] for what `reader` gives.
pub fn hash_reader(
    reader: &mut dyn Read,
    algos: &[Algo],
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<Digests> {
    let mut buffer = take_buffer();
    let result = (|| {
        let mut hashers = Hashers::new(algos);
        let chunk = &mut buffer[..CHUNK];
        loop {
            let n = read_full(reader, chunk, false)?;
            hashers.update(&chunk[..n]);
            if n > 0 && !progress(n as u64) {
                return Err(cancelled());
            }
            if n < CHUNK {
                return Ok(hashers.finish());
            }
        }
    })();
    give_back(buffer);
    result
}

/// The SHA-256 of the first `n` bytes of `path` (all of it if shorter): duplicates' first sieve.
pub fn hash_prefix(path: &Path, n: u64) -> io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    let mut buffer = take_buffer();
    let result = (|| {
        let mut file = open_sequential(path)?.take(n);
        let chunk = &mut buffer[..CHUNK];
        loop {
            let read = read_full(&mut file, chunk, false)?;
            hasher.update(&chunk[..read]);
            if read < CHUNK {
                return Ok(());
            }
        }
    })();
    give_back(buffer);
    result.map(|()| hasher.finalize().into())
}

/// Reads `a` and `b` side by side and returns where they first differ (`None`: the same bytes;
/// a file that is a beginning of the other differs where it ends). `uncached_b`: `b` is read
/// past the system's cache (a copy's check, spec 11 §5.1). `progress` hears the bytes of `a`
/// compared; false stops it.
pub fn same_content(
    a: &Path,
    b: &Path,
    uncached_b: bool,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<Option<u64>> {
    let mut first = open_sequential(a)?;
    if !uncached_b {
        return same_readers(&mut first, &mut open_sequential(b)?, false, progress);
    }
    let second = open_uncached(b)?;
    let direct = second.direct;
    let mut file = second.file;
    match same_readers(&mut first, &mut file, direct, progress) {
        // The drive took the uncached open but not its reads (a sector larger than the
        // alignment): a plain read then.
        Err(err) if direct && err.kind() == io::ErrorKind::InvalidInput => {
            let mut first = open_sequential(a)?;
            same_readers(&mut first, &mut open_sequential(b)?, false, progress)
        }
        other => other,
    }
}

/// [`same_content`] for two readers; `direct`: `b` needs aligned whole-sector reads.
pub fn same_readers(
    a: &mut dyn Read,
    b: &mut dyn Read,
    direct: bool,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<Option<u64>> {
    let (mut first, mut second) = (take_buffer(), take_buffer());
    let result = (|| {
        let mut at = 0u64;
        let (x, y) = (&mut first[..CHUNK], aligned(&mut second));
        loop {
            let n = read_full(a, x, false)?;
            let m = read_full(b, y, direct)?;
            let common = n.min(m);
            // Compared whole first (a memcmp); the place only when they differ.
            if x[..common] != y[..common]
                && let Some(i) = x[..common].iter().zip(&y[..common]).position(|(p, q)| p != q)
            {
                return Ok(Some(at + i as u64));
            }
            if n != m {
                return Ok(Some(at + common as u64));
            }
            at += n as u64;
            if n > 0 && !progress(n as u64) {
                return Err(cancelled());
            }
            if n < CHUNK {
                return Ok(None);
            }
        }
    })();
    give_back(first);
    give_back(second);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_dir;

    fn digests(data: &[u8]) -> Digests {
        hash_reader(&mut &data[..], &Algo::ALL, &mut |_| true).unwrap()
    }

    #[test]
    fn known_answers() {
        // RFC 1321's suite and FIPS 180-4's examples; CRC32's check value.
        let md5 = |s: &str| digests(s.as_bytes()).get(Algo::Md5).unwrap().to_owned();
        assert_eq!(md5(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5("a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(md5("abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5("message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        assert_eq!(md5("abcdefghijklmnopqrstuvwxyz"), "c3fcd3d76192e4007dfb496cca67e13b");
        assert_eq!(
            md5("12345678901234567890123456789012345678901234567890123456789012345678901234567890"),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
        let abc = digests(b"abc");
        assert_eq!(abc.get(Algo::Sha1), Some("a9993e364706816aba3e25717850c26c9cd0d89d"));
        assert_eq!(abc.get(Algo::Sha256), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        assert_eq!(
            abc.get(Algo::Sha512),
            Some(
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
            )
        );
        let two_blocks = digests(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        assert_eq!(two_blocks.get(Algo::Sha1), Some("84983e441c3bd26ebaae4aa1f95129e5e54670f1"));
        assert_eq!(
            two_blocks.get(Algo::Sha256),
            Some("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")
        );
        assert_eq!(digests(b"123456789").get(Algo::Crc32), Some("cbf43926"));
        let empty = digests(b"");
        assert_eq!(empty.get(Algo::Crc32), Some("00000000"));
        assert_eq!(empty.get(Algo::Sha256), Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    }

    #[test]
    fn one_read_gives_each_digest_as_a_read_of_its_own_and_only_those_asked() {
        let data: Vec<u8> = (0..3 * CHUNK as u32 + 17).map(|i| (i * 7 % 251) as u8).collect();
        let all = digests(&data);
        for algo in Algo::ALL {
            let alone = hash_reader(&mut &data[..], &[algo], &mut |_| true).unwrap();
            assert_eq!(alone.get(algo), all.get(algo), "{algo:?}");
            assert_eq!(Algo::ALL.iter().filter(|a| alone.get(**a).is_some()).count(), 1);
        }
    }

    #[test]
    fn files_on_both_sides_of_a_chunk() {
        let dir = test_dir("hash-chunks");
        for len in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK] {
            let data: Vec<u8> = (0..len).map(|i| (i % 253) as u8).collect();
            let path = dir.join(format!("{len}.bin"));
            std::fs::write(&path, &data).unwrap();
            let mut seen = 0;
            let got = hash_file(&path, &[Algo::Sha256, Algo::Crc32], &mut |n| {
                seen += n;
                true
            })
            .unwrap();
            assert_eq!(seen, len as u64, "{len}");
            assert_eq!(got.get(Algo::Sha256).map(str::to_owned), digests(&data).get(Algo::Sha256).map(str::to_owned));
            assert_eq!(got.get(Algo::Crc32), Some(format!("{:08x}", crc32fast::hash(&data)).as_str()));
            assert!(got.get(Algo::Md5).is_none());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `len` zero bytes, given without writing them (the buffer starts zeroed).
    struct Zeros(u64);

    impl Read for Zeros {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = (buf.len() as u64).min(self.0) as usize;
            self.0 -= n as u64;
            Ok(n)
        }
    }

    #[test]
    fn past_four_gib_offsets_and_counts_do_not_wrap() {
        let len = (4u64 << 30) + 3;
        let mut seen = 0u64;
        hash_reader(&mut Zeros(len), &[], &mut |n| {
            seen += n;
            true
        })
        .unwrap();
        assert_eq!(seen, len);
        // The second ends 2 bytes early: they differ where it ends.
        let differ = same_readers(&mut Zeros(len), &mut Zeros(len - 2), false, &mut |_| true).unwrap();
        assert_eq!(differ, Some(len - 2));
    }

    #[test]
    fn a_stop_ends_the_read_at_once() {
        let mut calls = 0;
        let err = hash_reader(&mut Zeros(10 * CHUNK as u64), &Algo::ALL, &mut |_| {
            calls += 1;
            false
        })
        .unwrap_err();
        assert_eq!((err.kind(), calls), (io::ErrorKind::Interrupted, 1));
        let err = same_readers(&mut Zeros(5 * CHUNK as u64), &mut Zeros(5 * CHUNK as u64), false, &mut |_| false);
        assert_eq!(err.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn same_content_finds_the_first_difference() {
        let dir = test_dir("same-content");
        let data: Vec<u8> = (0..CHUNK as u32 + 4099).map(|i| (i % 249) as u8).collect();
        std::fs::write(dir.join("a"), &data).unwrap();
        std::fs::write(dir.join("same"), &data).unwrap();
        let mut changed = data.clone();
        changed[CHUNK + 5] ^= 1;
        std::fs::write(dir.join("changed"), &changed).unwrap();
        std::fs::write(dir.join("short"), &data[..100]).unwrap();
        std::fs::write(dir.join("empty"), b"").unwrap();
        for uncached in [false, true] {
            let same = |b: &str| same_content(&dir.join("a"), &dir.join(b), uncached, &mut |_| true).unwrap();
            assert_eq!(same("same"), None, "uncached {uncached}");
            assert_eq!(same("changed"), Some(CHUNK as u64 + 5));
            assert_eq!(same("short"), Some(100));
            assert_eq!(same("empty"), Some(0));
        }
        assert_eq!(same_content(&dir.join("empty"), &dir.join("empty"), true, &mut |_| true).unwrap(), None);
        assert!(same_content(&dir.join("a"), &dir.join("missing"), false, &mut |_| true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_prefix_is_the_first_bytes_digest() {
        let dir = test_dir("hash-prefix");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 241) as u8).collect();
        std::fs::write(dir.join("f"), &data).unwrap();
        let want: [u8; 32] = Sha256::digest(&data[..65536]).into();
        assert_eq!(hash_prefix(&dir.join("f"), 65536).unwrap(), want);
        let whole: [u8; 32] = Sha256::digest(&data).into();
        assert_eq!(hash_prefix(&dir.join("f"), 1 << 30).unwrap(), whole, "shorter than n: all of it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
