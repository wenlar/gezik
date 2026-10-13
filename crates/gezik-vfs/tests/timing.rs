//! How long opening a large zip takes (spec 16 §13: a 10,000-entry zip shows its first rows
//! in 150 ms at most). Run by hand, in release: `cargo test -p gezik-vfs --release --test
//! timing -- --ignored --nocapture`.

use std::io::Write;
use std::time::Instant;

use gezik_core::vfs::VfsPath;

struct Quiet;

impl gezik_vfs::Cx for Quiet {
    fn add_bytes(&self, _: u64) {}
    fn entry_done(&self) {}
    fn stopped(&self) -> bool {
        false
    }
    fn password(&self, _: bool) -> Option<String> {
        None
    }
    fn entry_failed(&self, _: &str, _: &std::io::Error) {}
}

#[test]
#[ignore = "a measurement, run by hand"]
fn opening_large_zips() {
    let dir = std::env::temp_dir().join(format!("gezik-vfs-timing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for count in [10_000usize, 100_000] {
        let file = dir.join(format!("z{count}.zip"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
        for i in 0..count {
            let name = format!("project/src/module{:03}/file_{i:06}.rs", i % 500);
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"fn main() {}").unwrap();
        }
        zip.finish().unwrap();
        let at = VfsPath::archive(file.clone()).join("project/src/module007");
        let start = Instant::now();
        let vfs = gezik_vfs::mount(&at.root, &Quiet).unwrap();
        let rows = vfs.list(&at.inner).unwrap().len();
        println!("{count} entries: opened and listed in {:?} ({rows} rows shown)", start.elapsed());
        drop(vfs);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
