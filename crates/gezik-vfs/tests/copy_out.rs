//! Copying out of an archive through the engine: only the chosen entries land in the target
//! folder, folders with what they hold, and no staging folder stays.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_core::vfs::VfsPath;
use gezik_ops::{Answer, Engine, Event, Settings};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-vfs-out-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn zip(path: &Path, files: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn chosen_entries_land_in_the_folder() {
    let d = dir("copy");
    let archive = d.join("a.zip");
    zip(&archive, &[("docs/x.txt", b"x"), ("docs/sub/y.txt", b"yy"), ("docs/other.txt", b"o"), ("top.txt", b"t")]);
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let at = VfsPath::archive(archive).join("docs");
    let (tasks, label) = gezik_vfs::tasks::copy_out(&[at.join("x.txt"), at.join("sub")], out.clone());
    let engine = Engine::new(Settings { pending_deletes: Some(d.join("pending")), ..Settings::default() }, || {});
    let job = engine.submit_chain(tasks, Some(label));
    let deadline = Instant::now() + Duration::from_secs(60);
    let report = 'run: loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, id, .. } if j == job => engine.answer(job, id, Answer::Cancel),
                Event::Finished { job: j, report } if j == job => break 'run report,
                _ => {}
            }
        }
        assert!(Instant::now() < deadline, "the job did not finish");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let mut names: Vec<String> =
        std::fs::read_dir(&out).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["sub", "x.txt"], "nothing else, no staging folder");
    assert_eq!(std::fs::read(out.join("sub/y.txt")).unwrap(), b"yy");
    assert_eq!(engine.undo_label().as_deref(), Some("Copy 2 items"));
    let _ = std::fs::remove_dir_all(&d);
}
