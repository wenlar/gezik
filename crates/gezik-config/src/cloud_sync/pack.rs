//! The bundle the server keeps (spec 17 §5.1). Plain: `GZS1`, the file count (u32), then per
//! file the path's length (u16), the path (UTF-8, `/` between parts), the size (u32) and the
//! bytes, all little-endian. Sealed: `GZE1`, a random 24-byte nonce, then the plain bundle
//! under XChaCha20-Poly1305 with the code's key and its id as associated data.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use super::Keys;
use super::plan::Files;

const PLAIN: &[u8; 4] = b"GZS1";
const SEALED: &[u8; 4] = b"GZE1";
const NONCE: usize = 24;

/// `files` as a plain bundle; an error names a path that may not be in one, or a file too large.
pub fn pack(files: &Files) -> Result<Vec<u8>, String> {
    let mut out = PLAIN.to_vec();
    out.extend(u32::try_from(files.len()).map_err(|_| "Too many files")?.to_le_bytes());
    for (path, bytes) in files {
        check_path(path)?;
        out.extend(u16::try_from(path.len()).map_err(|_| format!("Path too long: {path}"))?.to_le_bytes());
        out.extend(path.as_bytes());
        out.extend(u32::try_from(bytes.len()).map_err(|_| format!("File too large: {path}"))?.to_le_bytes());
        out.extend(bytes);
    }
    Ok(out)
}

/// The files of a plain bundle: refused whole for a wrong start, a path that is unsafe or given
/// twice, a cut-off end or bytes after it.
pub fn unpack(bytes: &[u8]) -> Result<Files, String> {
    let mut rest = bytes.strip_prefix(PLAIN).ok_or("Not a Gezik bundle")?;
    let rest = &mut rest;
    let count = u32::from_le_bytes(take(rest)?);
    let mut files = Files::new();
    for _ in 0..count {
        let len = u16::from_le_bytes(take(rest)?);
        let path = std::str::from_utf8(take_n(rest, len.into())?).map_err(|_| "A path is not UTF-8")?.to_owned();
        check_path(&path)?;
        let size = u32::from_le_bytes(take(rest)?);
        let data = take_n(rest, size as usize)?.to_vec();
        if files.insert(path.clone(), data).is_some() {
            return Err(format!("Path given twice: {path}"));
        }
    }
    if !rest.is_empty() {
        return Err("Bytes after the bundle's end".to_owned());
    }
    Ok(files)
}

/// The next `n` bytes of `rest`.
fn take_n<'a>(rest: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if rest.len() < n {
        return Err("The bundle is cut off".to_owned());
    }
    let (head, tail) = rest.split_at(n);
    *rest = tail;
    Ok(head)
}

/// The next `N` bytes of `rest`, as an array.
fn take<const N: usize>(rest: &mut &[u8]) -> Result<[u8; N], String> {
    Ok(take_n(rest, N)?.try_into().unwrap_or([0; N]))
}

/// `plain` sealed with `keys` under a fresh random nonce.
pub fn seal(plain: &[u8], keys: &Keys) -> std::io::Result<Vec<u8>> {
    let mut nonce = [0u8; NONCE];
    getrandom::fill(&mut nonce).map_err(std::io::Error::other)?;
    let cipher = XChaCha20Poly1305::new(&Key::from(keys.key));
    let sealed = cipher
        .encrypt(&XNonce::from(nonce), Payload { msg: plain, aad: &keys.id })
        .map_err(|_| std::io::Error::other("Could not seal the bundle"))?;
    Ok([SEALED.as_slice(), &nonce, &sealed].concat())
}

/// The plain bundle in `sealed`; an error for another format, another code's keys or any change.
pub fn open(sealed: &[u8], keys: &Keys) -> Result<Vec<u8>, String> {
    let rest = sealed.strip_prefix(SEALED).ok_or("Not a sealed Gezik bundle")?;
    if rest.len() < NONCE {
        return Err("The bundle is cut off".to_owned());
    }
    let (nonce, body) = rest.split_at(NONCE);
    let nonce: [u8; NONCE] = nonce.try_into().map_err(|_| "The bundle is cut off")?;
    let cipher = XChaCha20Poly1305::new(&Key::from(keys.key));
    cipher
        .decrypt(&XNonce::from(nonce), Payload { msg: body, aad: &keys.id })
        .map_err(|_| "The bundle does not open with this code".to_owned())
}

/// A path as a bundle may have it: relative, `/` between parts, none empty, `.` or `..`, no
/// `\`, and one Windows takes as it is (`safe_relative` would change nothing in it), so the
/// same files land on every system.
fn check_path(path: &str) -> Result<(), String> {
    let refused = || Err(format!("Unsafe path in the bundle: {path}"));
    if path.contains('\\') {
        return refused();
    }
    match gezik_core::drop_names::safe_relative(path, true) {
        Ok(safe) if safe.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/") == path => Ok(()),
        _ => refused(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(entries: &[(&str, &[u8])]) -> Files {
        entries.iter().map(|(path, bytes)| (path.to_string(), bytes.to_vec())).collect()
    }

    fn keys(code: &str) -> Keys {
        Keys::derive(code)
    }

    #[test]
    fn a_bundle_goes_and_comes_back() {
        let set = files(&[("settings.toml", b"theme = \"nord\"\n"), ("themes/nord.toml", b"x"), ("templates/e", b"")]);
        let plain = pack(&set).unwrap();
        assert_eq!(&plain[..8], b"GZS1\x03\0\0\0");
        assert_eq!(unpack(&plain).unwrap(), set);
        assert_eq!(unpack(&pack(&Files::new()).unwrap()).unwrap(), Files::new());
        let k = keys("A");
        let sealed = seal(&plain, &k).unwrap();
        assert_eq!(&sealed[..4], b"GZE1");
        assert_eq!(sealed.len(), 4 + 24 + plain.len() + 16);
        assert_ne!(seal(&plain, &k).unwrap(), sealed, "a new nonce each time");
        assert_eq!(open(&sealed, &k).unwrap(), plain);
    }

    #[test]
    fn a_bad_or_foreign_bundle_is_refused() {
        let plain = pack(&files(&[("settings.toml", b"a = 1\n")])).unwrap();
        let k = keys("A");
        let sealed = seal(&plain, &k).unwrap();
        assert!(open(&sealed, &keys("B")).is_err(), "another code");
        // The same key under another id: the id is bound in.
        let other_id = Keys { id: [7; 16], auth: k.auth, key: k.key };
        assert!(open(&sealed, &other_id).is_err());
        for at in [4, 30, sealed.len() - 1] {
            let mut bent = sealed.clone();
            bent[at] ^= 1;
            assert!(open(&bent, &k).is_err(), "byte {at}");
        }
        assert!(open(&sealed[..sealed.len() - 1], &k).is_err());
        assert!(open(&sealed[..10], &k).is_err());
        assert!(open(&plain, &k).is_err(), "not sealed");
        let mut magic = sealed.clone();
        magic[3] = b'2';
        assert!(open(&magic, &k).is_err());

        assert!(unpack(b"GZS2\0\0\0\0").is_err());
        assert!(unpack(&plain[..plain.len() - 1]).is_err(), "cut off");
        assert!(unpack(&[plain.as_slice(), b"x"].concat()).is_err(), "extra bytes");
        let mut twice = b"GZS1\x02\0\0\0".to_vec();
        for _ in 0..2 {
            twice.extend(b"\x01\0a\x00\0\0\0");
        }
        assert_eq!(unpack(&twice), Err("Path given twice: a".to_owned()));
    }

    #[test]
    fn unsafe_paths_are_refused() {
        for path in [
            "../settings.toml",
            "themes/../../x",
            "/etc/passwd",
            "C:/x",
            "themes\\nord.toml",
            "themes//nord.toml",
            "themes/",
            "",
            "./a",
            "a\nb",
            "CON",
            "a.",
        ] {
            assert!(pack(&files(&[(path, b"x")])).is_err(), "pack {path:?}");
            let mut raw = b"GZS1\x01\0\0\0".to_vec();
            raw.extend((path.len() as u16).to_le_bytes());
            raw.extend(path.as_bytes());
            raw.extend(b"\x01\0\0\0x");
            assert!(unpack(&raw).is_err(), "unpack {path:?}");
        }
        assert!(pack(&files(&[("themes/My Theme.toml", b"x"), ("ünlü.toml", b"y")])).is_ok());
    }
}
