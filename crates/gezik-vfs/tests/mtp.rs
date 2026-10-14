//! A phone through the engine (spec 16 §10, 16f) against a fake device: browsing (repeated
//! names), copying off and onto it, new folder, rename and delete with their undo, the
//! questions for names taken, one queue per phone, a cancel and a phone unplugged half way.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gezik_core::vfs::{Caps, Edit, VfsPath, VfsRoot};
use gezik_ops::{Answer, Engine, Event, Question, Report, Settings, Task};
use gezik_platform::mtp::{Object, Session};
use gezik_vfs::Vfs;
use gezik_vfs::mtp::MtpVfs;
use gezik_vfs::tasks::{VfsEditTask, copy_out};

#[derive(Clone)]
struct Obj {
    id: String,
    parent: String,
    name: String,
    is_dir: bool,
    data: Vec<u8>,
}

/// A phone in memory. `unplug_at`: it goes away once that many bytes were read or written;
/// `slow`: each piece takes this long.
#[derive(Default)]
struct Phone {
    objects: Mutex<Vec<Obj>>,
    next: AtomicU64,
    gone: AtomicBool,
    unplug_at: AtomicU64,
    moved: AtomicU64,
    slow: Mutex<Duration>,
    no_rename: AtomicBool,
    /// Reads give half of each file and say nothing.
    short: AtomicBool,
    /// Deleting these ids fails.
    fail_delete: Mutex<Vec<String>>,
    /// The next this many puts fail.
    fail_puts: AtomicU64,
}

const PIECE: usize = 4;

impl Phone {
    fn new() -> Arc<Phone> {
        let phone = Arc::new(Phone::default());
        phone.unplug_at.store(u64::MAX, Ordering::SeqCst);
        phone
    }

    fn add(&self, parent: &str, name: &str, data: Option<&[u8]>) -> String {
        let id = format!("o{}", self.next.fetch_add(1, Ordering::SeqCst));
        self.objects.lock().unwrap().push(Obj {
            id: id.clone(),
            parent: parent.into(),
            name: name.into(),
            is_dir: data.is_none(),
            data: data.unwrap_or_default().to_vec(),
        });
        id
    }

    /// Names in folder `parent`, sorted.
    fn names(&self, parent: &str) -> Vec<String> {
        let mut names: Vec<String> =
            self.objects.lock().unwrap().iter().filter(|o| o.parent == parent).map(|o| o.name.clone()).collect();
        names.sort();
        names
    }

    fn data(&self, parent: &str, name: &str) -> Vec<u8> {
        let objects = self.objects.lock().unwrap();
        objects.iter().find(|o| o.parent == parent && o.name == name).map(|o| o.data.clone()).unwrap()
    }

    fn id(&self, parent: &str, name: &str) -> String {
        let objects = self.objects.lock().unwrap();
        objects.iter().find(|o| o.parent == parent && o.name == name).map(|o| o.id.clone()).unwrap()
    }

    fn check(&self) -> io::Result<()> {
        if self.gone.load(Ordering::SeqCst) { Err(io::Error::other("The device is not connected")) } else { Ok(()) }
    }

    /// Counts `n` bytes moved; the phone goes once past `unplug_at`.
    fn piece(&self, n: usize) -> io::Result<()> {
        std::thread::sleep(*self.slow.lock().unwrap());
        let moved = self.moved.fetch_add(n as u64, Ordering::SeqCst) + n as u64;
        if moved > self.unplug_at.load(Ordering::SeqCst) {
            self.gone.store(true, Ordering::SeqCst);
        }
        self.check()
    }

    fn object(o: &Obj) -> Object {
        Object { id: o.id.clone(), name: o.name.clone(), is_dir: o.is_dir, size: o.data.len() as u64, modified: None }
    }
}

struct Fake(Arc<Phone>);

impl Session for Fake {
    fn storages(&self) -> io::Result<Vec<Object>> {
        self.list("DEVICE")
    }

    fn list(&self, parent: &str) -> io::Result<Vec<Object>> {
        self.0.check()?;
        Ok(self.0.objects.lock().unwrap().iter().filter(|o| o.parent == parent).map(Phone::object).collect())
    }

    fn read_to(&self, id: &str, out: &mut dyn io::Write, more: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
        self.0.check()?;
        let mut data = self.0.objects.lock().unwrap().iter().find(|o| o.id == id).map(|o| o.data.clone()).unwrap();
        if self.0.short.load(Ordering::SeqCst) {
            data.truncate(data.len() / 2);
        }
        for piece in data.chunks(PIECE) {
            self.0.piece(piece.len())?;
            out.write_all(piece)?;
            if !more(piece.len() as u64) {
                return Err(io::ErrorKind::Interrupted.into());
            }
        }
        Ok(())
    }

    fn put(
        &self,
        parent: &str,
        name: &str,
        _size: u64,
        from: &mut dyn io::Read,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> io::Result<Object> {
        self.0.check()?;
        if self.0.fail_puts.load(Ordering::SeqCst) > 0 {
            self.0.fail_puts.fetch_sub(1, Ordering::SeqCst);
            return Err(io::Error::other("the phone refused the file"));
        }
        let mut data = Vec::new();
        let mut piece = [0u8; PIECE];
        loop {
            let n = from.read(&mut piece)?;
            if n == 0 {
                break;
            }
            // Nothing is made until the whole file is there.
            self.0.piece(n)?;
            data.extend_from_slice(&piece[..n]);
            if !more(n as u64) {
                return Err(io::ErrorKind::Interrupted.into());
            }
        }
        let id = self.0.add(parent, name, Some(&data));
        Ok(Object { id, name: name.into(), is_dir: false, size: data.len() as u64, modified: None })
    }

    fn mkdir(&self, parent: &str, name: &str) -> io::Result<Object> {
        self.0.check()?;
        let id = self.0.add(parent, name, None);
        Ok(Object { id, name: name.into(), is_dir: true, size: 0, modified: None })
    }

    fn delete(&self, id: &str) -> io::Result<()> {
        self.0.check()?;
        if self.0.fail_delete.lock().unwrap().iter().any(|f| f == id) {
            return Err(io::Error::other("the phone refused to delete it"));
        }
        let mut objects = self.0.objects.lock().unwrap();
        let mut gone = vec![id.to_owned()];
        while let Some(at) = gone.pop() {
            gone.extend(objects.iter().filter(|o| o.parent == at).map(|o| o.id.clone()));
            objects.retain(|o| o.id != at);
        }
        Ok(())
    }

    fn rename(&self, id: &str, name: &str) -> io::Result<()> {
        self.0.check()?;
        let mut objects = self.0.objects.lock().unwrap();
        objects.iter_mut().find(|o| o.id == id).ok_or(io::ErrorKind::NotFound)?.name = name.into();
        Ok(())
    }

    fn can_rename(&self, _id: &str) -> bool {
        !self.0.no_rename.load(Ordering::SeqCst)
    }

    fn connected(&self) -> bool {
        !self.0.gone.load(Ordering::SeqCst)
    }
}

/// A phone with storage "Internal" holding `DCIM` (two `a.jpg`, a `b.txt`).
fn phone(test: &str) -> (Arc<Phone>, Arc<dyn Vfs>, VfsPath) {
    let phone = Phone::new();
    let storage = phone.add("DEVICE", "Internal", None);
    let dcim = phone.add(&storage, "DCIM", None);
    phone.add(&dcim, "a.jpg", Some(b"first"));
    phone.add(&dcim, "a.jpg", Some(b"second"));
    phone.add(&dcim, "b.txt", Some(b"bee"));
    let root = VfsRoot::Device { id: format!("usb#{test}"), name: "Pixel".into() };
    let vfs = MtpVfs::open(root.clone(), Box::new(Fake(phone.clone()))).unwrap();
    gezik_vfs::hold(&vfs);
    (phone, vfs, VfsPath { root, inner: String::new() })
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-vfs-mtp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    gezik_platform::trash::purge_at_thread_end(&d);
    d
}

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending")), ..Settings::default() }, || {})
}

/// Runs `job` to the end, answering each question with the next of `answers` (then Cancel).
fn wait(engine: &Engine, job: gezik_ops::JobId, answers: &[Answer]) -> (Report, Vec<Question>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut asked = Vec::new();
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, id, question } if j == job => {
                    let answer = answers.get(asked.len()).cloned().unwrap_or(Answer::Cancel);
                    asked.push(question);
                    engine.answer(job, id, answer);
                }
                Event::Finished { job: j, report } if j == job => return (report, asked),
                _ => {}
            }
        }
        assert!(Instant::now() < deadline, "the job did not finish (a hang)");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn run(engine: &Engine, task: impl Task + 'static, answers: &[Answer]) -> (Report, Vec<Question>) {
    let job = engine.submit_chain(vec![Box::new(task)], None);
    wait(engine, job, answers)
}

fn undo(engine: &Engine) -> Report {
    let job = engine.undo().expect("something to undo");
    wait(engine, job, &[]).0
}

#[test]
fn browsing_shows_storages_and_tells_repeated_names_apart() {
    let (_phone, vfs, _) = phone("browse");
    assert!(vfs.caps().has(Caps::WRITE));
    let top: Vec<String> = vfs.list("").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(top, ["Internal"]);
    let names: Vec<(String, bool, u64)> =
        vfs.list("Internal/DCIM").unwrap().into_iter().map(|e| (e.name, e.is_dir, e.size)).collect();
    assert_eq!(names, [("a.jpg".into(), false, 5), ("a (2).jpg".into(), false, 6), ("b.txt".into(), false, 3)]);
    assert!(vfs.is_dir("Internal/DCIM") && vfs.is_file("Internal/DCIM/a (2).jpg"));
    assert!(vfs.list("Internal/nothing").is_none());
}

#[test]
fn files_copied_off_the_phone_are_whole_and_each_the_right_one() {
    let (_phone, vfs, top) = phone("copy-out");
    let d = dir("copy-out");
    let dcim = top.join("Internal/DCIM");
    vfs.list(&dcim.inner);
    let (tasks, _) = copy_out(&[dcim.join("a.jpg"), dcim.join("a (2).jpg"), top.join("Internal")], d.clone());
    let engine = engine(&d);
    let job = engine.submit_chain(tasks, None);
    let (report, _) = wait(&engine, job, &[]);
    assert!(report.failures.is_empty() && !report.cancelled, "{:?}", report.failures);
    assert_eq!(std::fs::read(d.join("a.jpg")).unwrap(), b"first");
    assert_eq!(std::fs::read(d.join("a (2).jpg")).unwrap(), b"second", "the second object, not the first");
    assert_eq!(std::fs::read(d.join("Internal/DCIM/b.txt")).unwrap(), b"bee", "a folder with what it holds");
    drop(vfs);
}

#[test]
fn files_put_on_the_phone_go_away_again_with_undo() {
    let (phone, vfs, top) = phone("put");
    let d = dir("put");
    std::fs::write(d.join("new.txt"), b"new file").unwrap();
    std::fs::create_dir_all(d.join("album/inner")).unwrap();
    std::fs::write(d.join("album/inner/p.jpg"), b"pic").unwrap();
    let dcim = top.join("Internal/DCIM");
    let engine = engine(&d);
    let task = VfsEditTask::put(&dcim, vec![d.join("new.txt"), d.join("album")], false);
    assert_eq!(task.resources().vfs_roots, ["usb#put"], "queued by the phone");
    let (report, asked) = run(&engine, task, &[]);
    assert!(report.failures.is_empty() && asked.is_empty(), "{:?} {asked:?}", report.failures);
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "album", "b.txt", "new.txt"]);
    assert_eq!(phone.data(&dcim_id, "new.txt"), b"new file");
    let inner = phone.id(&phone.id(&dcim_id, "album"), "inner");
    assert_eq!(phone.data(&inner, "p.jpg"), b"pic");
    let report = undo(&engine);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "b.txt"], "what was put went");
    drop(vfs);
}

#[test]
fn a_new_folder_and_a_rename_are_undone_and_a_delete_stays() {
    let (phone, vfs, top) = phone("edit");
    let d = dir("edit");
    let engine = engine(&d);
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    let root = top.root.clone();
    let (report, _) =
        run(&engine, VfsEditTask::new(root.clone(), vec![Edit::MakeDir("Internal/DCIM/New".into())]), &[]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(phone.names(&dcim_id).contains(&"New".to_owned()));
    undo(&engine);
    assert!(!phone.names(&dcim_id).contains(&"New".to_owned()));
    vfs.list("Internal/DCIM");
    let rename = Edit::Rename { from: "Internal/DCIM/b.txt".into(), to: "Internal/DCIM/c.txt".into() };
    let (report, _) = run(&engine, VfsEditTask::new(root.clone(), vec![rename]), &[]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "c.txt"]);
    undo(&engine);
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "b.txt"], "renamed back");
    let (report, _) = run(&engine, VfsEditTask::new(root, vec![Edit::Delete("Internal/DCIM/a (2).jpg".into())]), &[]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "b.txt"]);
    assert_eq!(phone.data(&dcim_id, "a.jpg"), b"first", "the second one went, by its id");
    drop(vfs);
}

#[test]
fn a_name_taken_on_the_phone_is_asked_about() {
    let (phone, vfs, top) = phone("clash");
    let d = dir("clash");
    std::fs::write(d.join("b.txt"), b"newer").unwrap();
    let dcim = top.join("Internal/DCIM");
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    let engine = engine(&d);
    let put = || VfsEditTask::put(&dcim, vec![d.join("b.txt")], false);
    // Cancel: nothing changes.
    let (_, asked) = run(&engine, put(), &[Answer::Cancel]);
    assert_eq!(asked.len(), 1);
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"bee");
    // Keep both.
    run(&engine, put(), &[Answer::Button(2)]);
    assert_eq!(phone.data(&dcim_id, "b (2).txt"), b"newer");
    // Replace: written beside it, the old one deleted, the new one renamed.
    let (report, asked) = run(&engine, put(), &[Answer::Button(0)]);
    assert!(report.failures.is_empty() && asked.len() == 1, "{:?}", report.failures);
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"newer");
    assert!(!phone.names(&dcim_id).iter().any(|n| n.contains("gezik-tmp")));
    // Where the phone cannot rename, the old one goes first: asked again.
    phone.no_rename.store(true, Ordering::SeqCst);
    std::fs::write(d.join("b.txt"), b"newest").unwrap();
    let (_, asked) = run(&engine, put(), &[Answer::Button(0), Answer::Cancel]);
    assert_eq!(asked.len(), 2);
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"newer", "cancelled at the second question");
    run(&engine, put(), &[Answer::Button(0), Answer::Button(0)]);
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"newest");
    drop(vfs);
}

#[test]
fn a_phone_unplugged_half_way_ends_the_job_and_leaves_no_half_file() {
    let (phone, vfs, top) = phone("unplug");
    let d = dir("unplug");
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    phone.add(&dcim_id, "big.bin", Some(&[7u8; 64]));
    vfs.list("Internal/DCIM");
    phone.unplug_at.store(20, Ordering::SeqCst);
    let started = Instant::now();
    let (tasks, _) = copy_out(&[top.join("Internal/DCIM/big.bin")], d.clone());
    let engine = engine(&d);
    let job = engine.submit_chain(tasks, None);
    let (report, _) = wait(&engine, job, &[]);
    assert!(!report.failures.is_empty(), "it failed");
    assert!(started.elapsed() < Duration::from_secs(10));
    let left: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(left.iter().all(|n| n == "pending"), "nothing half copied: {left:?}");
    assert!(!vfs.unchanged(), "the place knows it is gone");
    // Onto the phone: a gone phone fails at once.
    std::fs::write(d.join("x.txt"), b"x").unwrap();
    let (report, _) = run(&engine, VfsEditTask::put(&top.join("Internal/DCIM"), vec![d.join("x.txt")], false), &[]);
    assert!(!report.failures.is_empty());
    assert!(d.join("x.txt").exists(), "the source stays");
    drop(vfs);
}

#[test]
fn a_cancelled_put_makes_nothing_on_the_phone() {
    let (phone, vfs, top) = phone("cancel");
    let d = dir("cancel");
    std::fs::write(d.join("slow.bin"), [1u8; 400]).unwrap();
    *phone.slow.lock().unwrap() = Duration::from_millis(20);
    let engine = engine(&d);
    let task = VfsEditTask::put(&top.join("Internal/DCIM"), vec![d.join("slow.bin")], true);
    let job = engine.submit_chain(vec![Box::new(task)], None);
    std::thread::sleep(Duration::from_millis(300));
    engine.cancel(job);
    let (report, _) = wait(&engine, job, &[]);
    assert!(report.cancelled);
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "b.txt"]);
    assert!(d.join("slow.bin").exists(), "a cancelled move keeps its source");
    drop(vfs);
}

#[test]
fn a_move_between_folders_on_the_phone_is_a_copy_and_a_delete() {
    let (phone, vfs, top) = phone("move");
    let d = dir("move");
    let engine = engine(&d);
    let storage = phone.id("DEVICE", "Internal");
    phone.add(&storage, "Music", None);
    vfs.list("Internal");
    vfs.list("Internal/DCIM");
    let task = VfsEditTask::copy_in(&[top.join("Internal/DCIM/b.txt")], &top.join("Internal/Music"), true);
    let (report, _) = run(&engine, task, &[]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(phone.names(&phone.id(&storage, "Music")), ["b.txt"]);
    assert_eq!(phone.names(&phone.id(&storage, "DCIM")), ["a.jpg", "a.jpg"]);
    drop(vfs);
}

// ---- Regressions found in review (each failed before its fix) ----

/// Names in folder `path` (parts by name) on the phone.
fn names_at(phone: &Phone, path: &[&str]) -> Vec<String> {
    let mut id = "DEVICE".to_owned();
    for part in path {
        id = phone.id(&id, part);
    }
    phone.names(&id)
}

#[test]
fn a_storage_is_never_deleted_or_renamed() {
    let (phone, vfs, top) = phone("storage");
    let d = dir("storage");
    let engine = engine(&d);
    let root = top.root.clone();
    let (report, _) = run(&engine, VfsEditTask::new(root.clone(), vec![Edit::Delete("Internal".into())]), &[]);
    assert!(!report.failures.is_empty());
    let rename = Edit::Rename { from: "Internal".into(), to: "Other".into() };
    let (report, _) = run(&engine, VfsEditTask::new(root, vec![rename]), &[]);
    assert!(!report.failures.is_empty());
    assert_eq!(phone.names("DEVICE"), ["Internal"]);
    assert_eq!(names_at(&phone, &["Internal"]), ["DCIM"], "the phone keeps what it holds");
    drop(vfs);
}

#[test]
fn a_moved_folder_with_a_name_no_path_holds_stays_on_the_phone() {
    let (phone, vfs, top) = phone("odd-name");
    let d = dir("odd-name");
    let dcim = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    phone.add(&dcim, "a/b.txt", Some(b"odd"));
    vfs.list("Internal");
    let (tasks, _) = gezik_vfs::tasks::move_out(&[top.join("Internal/DCIM")], d.clone());
    let engine = engine(&d);
    let job = engine.submit_chain(tasks, None);
    let (report, _) = wait(&engine, job, &[]);
    assert!(!report.failures.is_empty(), "the odd name is a failure");
    assert!(names_at(&phone, &["Internal", "DCIM"]).contains(&"a/b.txt".to_owned()), "nothing deleted");
    drop(vfs);
}

#[test]
fn a_move_into_itself_or_onto_itself_deletes_nothing() {
    let (phone, vfs, top) = phone("itself");
    let d = dir("itself");
    let engine = engine(&d);
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    phone.add(&dcim_id, "sub", None);
    vfs.list("Internal/DCIM");
    let dcim = top.join("Internal/DCIM");
    run(&engine, VfsEditTask::copy_in(std::slice::from_ref(&dcim), &dcim.join("sub"), true), &[Answer::Button(0)]);
    run(&engine, VfsEditTask::copy_in(&[dcim.join("b.txt")], &dcim, true), &[Answer::Button(0)]);
    assert_eq!(names_at(&phone, &["Internal"]), ["DCIM"]);
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "b.txt", "sub"]);
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"bee");
    // The same guard holds in archives.
    let zip = VfsPath::archive("/w/a.zip".into());
    assert!(gezik_vfs::tasks::movable(&[zip.join("a")], &zip.join("a/b")).is_empty());
    assert!(gezik_vfs::tasks::movable(&[zip.join("a/x")], &zip.join("a")).is_empty());
    assert_eq!(gezik_vfs::tasks::movable(&[zip.join("a/x")], &zip.join("b")).len(), 1);
    drop(vfs);
}

#[test]
fn undo_leaves_what_took_a_made_things_place_and_a_folder_with_more_in_it() {
    let (phone, vfs, top) = phone("undo-id");
    let d = dir("undo-id");
    std::fs::write(d.join("new.txt"), b"mine").unwrap();
    std::fs::create_dir(d.join("album")).unwrap();
    std::fs::write(d.join("album/p.jpg"), b"pic").unwrap();
    let engine = engine(&d);
    let dcim = top.join("Internal/DCIM");
    run(&engine, VfsEditTask::put(&dcim, vec![d.join("new.txt"), d.join("album")], false), &[]);
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    // Another program replaces new.txt and adds a file to the album.
    let old = phone.id(&dcim_id, "new.txt");
    phone.objects.lock().unwrap().retain(|o| o.id != old);
    phone.add(&dcim_id, "new.txt", Some(b"theirs"));
    phone.add(&phone.id(&dcim_id, "album"), "theirs.jpg", Some(b"t"));
    undo(&engine);
    assert_eq!(phone.data(&dcim_id, "new.txt"), b"theirs", "not the object undo made");
    assert_eq!(names_at(&phone, &["Internal", "DCIM", "album"]), ["theirs.jpg"], "only what undo made went");
    drop(vfs);
}

#[test]
fn one_put_failing_does_not_stop_the_others_or_lose_their_undo() {
    let (phone, vfs, top) = phone("partial");
    let d = dir("partial");
    std::fs::write(d.join("real.txt"), b"real").unwrap();
    let engine = engine(&d);
    let dcim = top.join("Internal/DCIM");
    let (report, _) =
        run(&engine, VfsEditTask::put(&dcim, vec![d.join("missing.txt"), d.join("real.txt")], false), &[]);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    assert!(phone.names(&dcim_id).contains(&"real.txt".to_owned()));
    undo(&engine);
    assert!(!phone.names(&dcim_id).contains(&"real.txt".to_owned()));
    drop(vfs);
}

#[test]
fn a_replace_that_fails_leaves_no_temporary_file_and_keeps_the_old_one() {
    let (phone, vfs, top) = phone("replace-fail");
    let d = dir("replace-fail");
    std::fs::write(d.join("b.txt"), b"newer").unwrap();
    let dcim = top.join("Internal/DCIM");
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    let engine = engine(&d);
    // The old one cannot be deleted: the temporary copy goes again.
    phone.fail_delete.lock().unwrap().push(phone.id(&dcim_id, "b.txt"));
    let (report, _) = run(&engine, VfsEditTask::put(&dcim, vec![d.join("b.txt")], false), &[Answer::Button(0)]);
    assert!(!report.failures.is_empty());
    assert_eq!(phone.names(&dcim_id), ["a.jpg", "a.jpg", "b.txt"]);
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"bee");
    phone.fail_delete.lock().unwrap().clear();
    // No renaming, and the new one does not go in: the old one is put back.
    phone.no_rename.store(true, Ordering::SeqCst);
    phone.fail_puts.store(1, Ordering::SeqCst);
    let answers = [Answer::Button(0), Answer::Button(0)];
    let (report, _) = run(&engine, VfsEditTask::put(&dcim, vec![d.join("b.txt")], false), &answers);
    assert!(!report.failures.is_empty());
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"bee", "the old file is back");
    drop(vfs);
}

#[test]
fn a_file_that_comes_short_is_a_failure_and_a_move_keeps_it() {
    let (phone, vfs, top) = phone("short");
    let d = dir("short");
    vfs.list("Internal/DCIM");
    phone.short.store(true, Ordering::SeqCst);
    let (tasks, _) = gezik_vfs::tasks::move_out(&[top.join("Internal/DCIM/b.txt")], d.clone());
    let engine = engine(&d);
    let job = engine.submit_chain(tasks, None);
    let (report, _) = wait(&engine, job, &[]);
    assert!(!report.failures.is_empty());
    assert!(!d.join("b.txt").exists());
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    assert_eq!(phone.data(&dcim_id, "b.txt"), b"bee", "the original stays");
    drop(vfs);
}

#[test]
fn a_phone_plugged_in_again_gets_a_new_session() {
    let (phone, vfs, _) = phone("replug");
    assert!(vfs.unchanged());
    phone.gone.store(true, Ordering::SeqCst);
    assert!(!vfs.unchanged());
    phone.gone.store(false, Ordering::SeqCst);
    assert!(!vfs.unchanged(), "the old session is not used again");
    drop(vfs);
}

#[test]
fn a_folder_read_again_forgets_what_was_under_it() {
    let (phone, vfs, _) = phone("subtree");
    let dcim_id = phone.id(&phone.id("DEVICE", "Internal"), "DCIM");
    let sub = phone.add(&dcim_id, "sub", None);
    phone.add(&sub, "x.txt", Some(b"x"));
    vfs.list("Internal/DCIM/sub");
    assert!(vfs.is_file("Internal/DCIM/sub/x.txt"));
    phone.objects.lock().unwrap().retain(|o| o.id != sub && o.parent != sub);
    vfs.list("Internal/DCIM");
    assert!(!vfs.is_file("Internal/DCIM/sub/x.txt"), "gone with its folder");
    drop(vfs);
}
