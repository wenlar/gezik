use std::fs::{self, File};
use std::io::{self, BufReader, Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

use crate::tarops::{Codec, decoder};
use crate::{fresh_dir, sample};

// ---------------------------------------------------------------- CAB
pub fn cab_list_extract(path: &Path, dest: &Path) -> io::Result<Vec<String>> {
    let mut cab = cab::Cabinet::new(File::open(path)?)?;
    // collect names first: folder_entries() borrows the cabinet immutably
    let mut names = Vec::new();
    for folder in cab.folder_entries() {
        for f in folder.file_entries() {
            println!(
                "cab: {:<20} {:>8} B {:?} dt={:?} ro={} hidden={}",
                f.name(), f.uncompressed_size(), folder.compression_type(), f.datetime(), f.is_read_only(), f.is_hidden()
            );
            names.push(f.name().to_owned());
        }
    }
    for n in &names {
        // CAB names use `\`; safe_join treats both separators
        let Some(out) = crate::safe_join(dest, n) else { continue };
        fs::create_dir_all(out.parent().unwrap())?;
        let mut r = cab.read_file(n)?; // decodes its folder from the start (see gotchas)
        io::copy(&mut r, &mut File::create(out)?)?;
    }
    Ok(names)
}

// ---------------------------------------------------------------- ISO (hadris-iso)
pub fn iso_walk_hadris(path: &Path, dest: &Path) -> io::Result<()> {
    use hadris_iso::read::IsoImage;
    let img = IsoImage::open(File::open(path)?).map_err(|e| io::Error::other(format!("{e:?}")))?;
    println!("iso: rrip={} roots={:?}", img.supports_rrip(), img.root_dirs().iter().map(|r| r.entry_type()).collect::<Vec<_>>());
    // walk: (DirectoryRef, relative path)
    let mut stack = vec![(img.root_dir().dir_ref(), std::path::PathBuf::new())];
    while let Some((dref, rel)) = stack.pop() {
        let dir = img.open_dir(dref);
        for e in dir.entries() {
            let e = e.map_err(|e| io::Error::other(format!("{e:?}")))?;
            if e.is_special() {
                continue; // "." and ".."
            }
            // display_name(): RRIP NM > Joliet UCS-2 > ISO name without ";1"
            let name = e.display_name().into_owned();
            let rel = rel.join(&name);
            let (mode, link, mtime) = match &e.rrip {
                Some(r) => (
                    r.posix_attributes.as_ref().map(|p| p.file_mode.read()),
                    r.symlink_target.clone(),
                    r.timestamps.as_ref().and_then(|t| t.modify).map(|m| (m.year, m.month, m.day, m.hour, m.minute, m.second)),
                ),
                None => (None, None, None),
            };
            // ISO 9660 recording time is a private field; bytes 18..25 of the raw record header
            let raw = e.header().to_bytes();
            let rec = (1900 + raw[18] as u16, raw[19], raw[20], raw[21], raw[22], raw[23], raw[24] as i8);
            println!(
                "iso: {:<28} dir={} size={:>7} mode={:?} link={:?} rr_mtime={:?} rec_time={:?}",
                rel.display(), e.is_directory(), e.total_size(), mode.map(|m| format!("{m:o}")), link, mtime, rec
            );
            if e.is_directory() {
                stack.push((e.as_dir_ref(&img).map_err(|e| io::Error::other(format!("{e:?}")))?, rel));
            } else if let Some(out) = crate::safe_join(dest, &rel.to_string_lossy()) {
                fs::create_dir_all(out.parent().unwrap())?;
                // streaming: 64 KiB chunks, handles multi-extent (>4 GiB) files
                let mut chunks = img.read_file_chunked::<65536>(&e).map_err(|e| io::Error::other(format!("{e:?}")))?;
                let mut f = File::create(out)?;
                while let Some(c) = chunks.next_chunk().map_err(|e| io::Error::other(format!("{e:?}")))? {
                    f.write_all(&c)?; // progress: chunks.position() / chunks.total_size()
                }
            }
        }
    }
    Ok(())
}

pub fn iso_walk_isomage(path: &Path) -> Result<(), isomage::Error> {
    let mut f = File::open(path)?;
    let root = isomage::detect_and_parse_filesystem(&mut f, &path.to_string_lossy())?;
    fn walk(n: &isomage::TreeNode, depth: usize, f: &mut File) -> Result<(), isomage::Error> {
        for c in &n.children {
            println!("isomage: {}{} dir={} size={}", "  ".repeat(depth), c.name, c.is_directory, c.size);
            if c.is_directory {
                walk(c, depth + 1, f)?;
            } else {
                let mut sink = Vec::new();
                isomage::cat_node(f, c, &mut sink)?; // stream bytes of one file
                assert_eq!(sink.len() as u64, c.size);
            }
        }
        Ok(())
    }
    walk(&root, 0, &mut f)
}

fn make_iso(path: &Path, big: &[u8]) {
    use hadris_iso::read::PathSeparator;
    use hadris_iso::write::options::{CreationFeatures, IsoFormatOptions};
    use hadris_iso::write::{File as IsoFile, InputFiles, IsoImageWriter};
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
        features: CreationFeatures::extensions(), // Joliet L3 + Rock Ridge
        strict_charset: false,
    };
    let mut out = File::options().read(true).write(true).create(true).truncate(true).open(path).unwrap();
    IsoImageWriter::create(&mut out, files, options).unwrap();
}

// ---------------------------------------------------------------- AR / DEB
/// Lists a .deb: ar members, then the tar inside data.tar.* / control.tar.*.
pub fn deb_list(path: &Path) -> io::Result<()> {
    let mut ar = ar::Archive::new(File::open(path)?);
    while let Some(entry) = ar.next_entry() {
        let mut entry = entry?;
        let id = String::from_utf8_lossy(entry.header().identifier()).into_owned();
        println!("ar: {id:<16} size={} mtime={} mode={:o}", entry.header().size(), entry.header().mtime(), entry.header().mode());
        let codec = if id.ends_with(".gz") {
            Codec::Gz
        } else if id.ends_with(".xz") {
            Codec::Xz
        } else if id.ends_with(".zst") {
            Codec::Zst
        } else if id.ends_with(".bz2") {
            Codec::Bz2
        } else if id.ends_with(".tar") {
            Codec::None
        } else {
            let mut s = String::new();
            entry.read_to_string(&mut s)?;
            println!("ar:   content {s:?}");
            continue;
        };
        // stream: ar entry -> decompressor -> tar, no temp file
        let mut t = tar::Archive::new(decoder(codec, BufReader::new(&mut entry)));
        for te in t.entries()? {
            let te = te?;
            println!("ar:   {} -> {} ({} B)", id, te.path()?.display(), te.size());
        }
    }
    Ok(())
}

pub fn run(base: &Path) {
    let dir = fresh_dir(base, "misc");
    let big = sample(300_000, 77);

    // CAB: write MSZIP with the crate, LZX comes from makecab.exe in the probe bin
    let cab_path = dir.join("probe.cab");
    {
        let mut b = cab::CabinetBuilder::new();
        {
            let folder = b.add_folder(cab::CompressionType::MsZip);
            folder.add_file("docs\\big.bin");
            folder.add_file("small.txt").set_is_read_only(true);
        }
        let mut w = b.build(File::create(&cab_path).unwrap()).unwrap();
        while let Some(mut fw) = w.next_file().unwrap() {
            let data: &[u8] = if fw.file_name().ends_with("big.bin") { &big } else { b"small" };
            fw.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    let names = cab_list_extract(&cab_path, &dir.join("cab-out")).unwrap();
    assert_eq!(fs::read(dir.join("cab-out/docs/big.bin")).unwrap(), big);
    println!("cab: {names:?}");

    // ISO
    let iso_path = dir.join("probe.iso");
    make_iso(&iso_path, &big);
    iso_walk_hadris(&iso_path, &dir.join("iso-out")).unwrap();
    assert_eq!(fs::read(dir.join("iso-out/Docs/Big File.bin")).unwrap(), big);
    iso_walk_isomage(&iso_path).unwrap();

    // CPIO newc
    let cpio_path = dir.join("probe.cpio");
    {
        let inputs = vec![
            (cpio::NewcBuilder::new("etc").mode(0o040755).mtime(1_700_000_000), Cursor::new(Vec::new())),
            (cpio::NewcBuilder::new("etc/big.bin").mode(0o100644).mtime(1_700_000_000), Cursor::new(big.clone())),
            (cpio::NewcBuilder::new("../evil").mode(0o100644), Cursor::new(b"x".to_vec())),
        ];
        cpio::write_cpio(inputs.into_iter(), File::create(&cpio_path).unwrap()).unwrap();
    }
    let names = cpio_extract(&cpio_path, &dir.join("cpio-out")).unwrap();
    assert_eq!(fs::read(dir.join("cpio-out/etc/big.bin")).unwrap(), big);
    println!("cpio: {names:?}");

    // DEB = ar { debian-binary, control.tar.xz, data.tar.gz }
    let deb_path = dir.join("probe.deb");
    {
        let mut data_tar = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(big.len() as u64);
        h.set_mode(0o644);
        data_tar.append_data(&mut h, "./usr/share/gezik/big.bin", big.as_slice()).unwrap();
        let data_tar = data_tar.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&data_tar).unwrap();
        let data_gz = gz.finish().unwrap();

        let mut ctl = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(14);
        h.set_mode(0o644);
        ctl.append_data(&mut h, "./control", &b"Package: gezik"[..]).unwrap();
        let ctl = ctl.into_inner().unwrap();
        let mut xz = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(6)).unwrap();
        xz.write_all(&ctl).unwrap();
        let ctl_xz = xz.finish().unwrap();

        let mut a = ar::Builder::new(File::create(&deb_path).unwrap());
        for (name, bytes) in [("debian-binary", b"2.0\n".to_vec()), ("control.tar.xz", ctl_xz), ("data.tar.gz", data_gz)] {
            let mut h = ar::Header::new(name.as_bytes().to_vec(), bytes.len() as u64);
            h.set_mode(0o100644);
            h.set_mtime(1_700_000_000);
            a.append(&h, bytes.as_slice()).unwrap();
        }
    }
    deb_list(&deb_path).unwrap();
    println!("misc: OK");
}

/// cpio: newc (070701) / newc+crc (070702) only. Each NewcReader owns the stream; finish()
/// or to_writer() hands it back for the next header.
pub fn cpio_extract(path: &Path, dest: &Path) -> io::Result<Vec<String>> {
    let mut r = BufReader::new(File::open(path)?);
    let mut names = Vec::new();
    loop {
        let e = cpio::NewcReader::new(r)?;
        if e.entry().is_trailer() {
            break;
        }
        let name = e.entry().name().to_owned();
        let mode = e.entry().mode();
        println!("cpio: {name:<16} mode={mode:o} mtime={} size={}", e.entry().mtime(), e.entry().file_size());
        r = match (mode & 0o170000, crate::safe_join(dest, &name)) {
            (0o040000, Some(p)) => {
                fs::create_dir_all(p)?;
                e.finish()?
            }
            (0o100000, Some(p)) => {
                fs::create_dir_all(p.parent().unwrap())?;
                e.to_writer(File::create(p)?)? // copies the data and skips padding
            }
            _ => e.finish()?, // unsafe name, symlink (data = target), device, fifo
        };
        names.push(name);
    }
    Ok(names)
}
