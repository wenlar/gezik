use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Kind {
    Zip,
    SevenZ,
    Rar4,
    Rar5,
    Gzip,
    Xz,
    Bzip2,
    Zstd,
    Tar,
    Cab,
    Iso,
    CpioNewc,
    CpioOdc,
    CpioBin,
    Ar,
    Deb,
    Unknown,
}

/// Sniff from the first 64 KiB (ISO needs bytes at 0x8001; 32 KiB + 6 is enough).
pub fn sniff(h: &[u8]) -> Kind {
    let at = |off: usize, m: &[u8]| h.len() >= off + m.len() && &h[off..off + m.len()] == m;
    if at(0, b"PK\x03\x04") || at(0, b"PK\x05\x06") /* empty zip */ || at(0, b"PK\x07\x08") /* spanned marker */ {
        return Kind::Zip;
    }
    if at(0, b"7z\xBC\xAF\x27\x1C") {
        return Kind::SevenZ;
    }
    if at(0, b"Rar!\x1A\x07\x01\x00") {
        return Kind::Rar5;
    }
    if at(0, b"Rar!\x1A\x07\x00") {
        return Kind::Rar4;
    }
    if at(0, b"\x1F\x8B") {
        return Kind::Gzip;
    }
    if at(0, b"\xFD7zXZ\x00") {
        return Kind::Xz;
    }
    if at(0, b"BZh") && h.len() > 3 && (b'1'..=b'9').contains(&h[3]) {
        return Kind::Bzip2;
    }
    if at(0, b"\x28\xB5\x2F\xFD") {
        return Kind::Zstd;
    }
    if at(0, b"MSCF\0\0\0\0") {
        return Kind::Cab;
    }
    if at(0, b"!<arch>\n") {
        return if at(8, b"debian-binary") { Kind::Deb } else { Kind::Ar };
    }
    if at(0, b"070701") || at(0, b"070702") {
        return Kind::CpioNewc;
    }
    if at(0, b"070707") {
        return Kind::CpioOdc;
    }
    if at(0, &[0xC7, 0x71]) || at(0, &[0x71, 0xC7]) {
        return Kind::CpioBin;
    }
    // ustar: "ustar\0" + "00" (POSIX) or "ustar  \0" (GNU) at 257
    if at(257, b"ustar") {
        return Kind::Tar;
    }
    // ISO 9660: volume descriptor at sector 16 (0x8000): type byte, then "CD001"
    if at(0x8001, b"CD001") {
        return Kind::Iso;
    }
    // UDF-only images: "BEA01"/"NSR02"/"NSR03" at 0x8001/0x8801/0x9001
    if at(0x8001, b"BEA01") || at(0x8801, b"NSR0") {
        return Kind::Iso;
    }
    // old v7 tar has no magic: header checksum check at 148..156
    if h.len() >= 512 && v7_tar_checksum_ok(&h[..512]) {
        return Kind::Tar;
    }
    Kind::Unknown
}

fn v7_tar_checksum_ok(b: &[u8]) -> bool {
    let field = std::str::from_utf8(&b[148..156]).unwrap_or("").trim_matches(|c: char| c == '\0' || c == ' ');
    let Ok(want) = u32::from_str_radix(field, 8) else { return false };
    let sum: u32 = b.iter().enumerate().map(|(i, &x)| if (148..156).contains(&i) { 32 } else { x as u32 }).sum();
    sum == want && b[0] != 0
}

pub fn sniff_file(p: &Path) -> std::io::Result<Kind> {
    let mut f = File::open(p)?;
    let mut buf = vec![0u8; 0x9010];
    let mut n = 0;
    while n < buf.len() {
        let k = f.read(&mut buf[n..])?;
        if k == 0 {
            break;
        }
        n += k;
    }
    buf.truncate(n);
    let k = sniff(&buf);
    // zip with prepended data (SFX .exe, or a zip whose first entry is not at 0): look for EOCD
    if k == Kind::Unknown && f.seek(SeekFrom::End(-22)).is_ok() {
        let mut t = [0u8; 4];
        if f.read_exact(&mut t).is_ok() && &t == b"PK\x05\x06" {
            return Ok(Kind::Zip);
        }
    }
    Ok(k)
}

pub fn run(base: &Path) {
    let rar = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/rar");
    let cases: Vec<(std::path::PathBuf, Kind)> = vec![
        (base.join("zip/probe.zip"), Kind::Zip),
        (base.join("7z/enc.7z"), Kind::SevenZ),
        (base.join("7z/vol.7z.001"), Kind::SevenZ),
        (rar.join("solid.rar"), Kind::Rar5),
        (rar.join("crypted.rar"), Kind::Rar4),
        (base.join("tar/probe.tar"), Kind::Tar),
        (base.join("tar/probe.tar.gz"), Kind::Gzip),
        (base.join("tar/probe.tar.xz"), Kind::Xz),
        (base.join("tar/probe.tar.bz2"), Kind::Bzip2),
        (base.join("tar/probe.tar.zst"), Kind::Zstd),
        (base.join("misc/probe.cab"), Kind::Cab),
        (base.join("misc/probe.iso"), Kind::Iso),
        (base.join("misc/probe.cpio"), Kind::CpioNewc),
        (base.join("misc/probe.deb"), Kind::Deb),
    ];
    for (p, want) in cases {
        let got = sniff_file(&p).unwrap();
        println!("detect: {:<24} {:?}", p.file_name().unwrap().to_string_lossy(), got);
        assert_eq!(got, want, "{}", p.display());
    }
    // RAR5 signature check from bytes
    assert_eq!(sniff(b"Rar!\x1A\x07\x01\x00rest"), Kind::Rar5);
    println!("detect: OK");
}
