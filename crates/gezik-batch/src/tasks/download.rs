//! Downloading a tool: the system's own HTTP client (`gezik_platform::http`) into a temporary
//! file, the SHA-256 checked, then unpacked by Gezik's own archive reader into a staging
//! folder and moved to `<data>/tools/<name>-<version>/` in one step. Older versions go after.
//! Not undone and not in the history: the folder may be deleted by hand.

use std::cell::RefCell;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use gezik_core::batch::tools::ToolBuild;
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};
use gezik_platform::http;
use sha2::{Digest, Sha256};

use super::cancelled;
use crate::archive::{self, ExtractCx};
use crate::tools::{damaged, display_name, folder_name, install_dir};

/// How much larger than the build's size a download may get before it is stopped (the hash
/// check fails it anyway; this only stops an endless one early).
const SLACK: u64 = 1 << 20;

/// The folder in the staging folder that the download is unpacked into.
const CONTENT: &str = "x";

/// Downloads, checks and unpacks `build` into `install_dir`; then removes older versions.
pub struct DownloadTask {
    build: &'static ToolBuild,
    data_dir: PathBuf,
    url: String,
    /// Plain http allowed: only for a test server's address given by `with_url`.
    allow_http: bool,
}

impl DownloadTask {
    pub fn new(build: &'static ToolBuild, data_dir: PathBuf) -> DownloadTask {
        DownloadTask { build, data_dir, url: build.url.to_owned(), allow_http: false }
    }

    /// Downloads from `url` instead of the build's own address (tests: a local server, so
    /// plain http is allowed).
    pub fn with_url(mut self, url: String) -> DownloadTask {
        self.url = url;
        self.allow_http = true;
        self
    }

    /// Downloads into `temp` with the system's HTTP client, counting what arrives.
    fn download(&self, temp: &Path, cx: &RunCx<'_>) -> io::Result<()> {
        let size = self.build.size;
        let mut counted = 0u64;
        let mut progress = |bytes: u64| {
            let now = bytes.min(size);
            if now > counted {
                cx.add_bytes(now - counted);
                counted = now;
            }
        };
        let result =
            http::download(&self.url, temp, size.saturating_add(SLACK), self.allow_http, &mut progress, &|| {
                cx.stopped()
            });
        match result {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => return Err(cancelled()),
            result => result?,
        }
        cx.one_done(size.saturating_sub(counted));
        Ok(())
    }

    /// Unpacks the checked download at `file` into a staging folder and moves it in place.
    fn install(&self, file: &Path, target: &Path, cx: &RunCx<'_>) -> io::Result<()> {
        let stage = cx.staging_dir(target)?;
        // The reader knows a `.tar.xz` by its name: the download gets one in the staging folder.
        let archive_path = stage.join(format!("{}.{}", folder_name(self.build.tool), self.build.kind));
        std::fs::rename(file, &archive_path)?;
        let content = stage.join(CONTENT);
        std::fs::create_dir(&content)?;
        let unpacking = Unpacking { run: cx, failed: RefCell::new(None) };
        archive::open(&archive_path)?.extract(&content, &unpacking)?;
        if let Some(err) = unpacking.failed.into_inner() {
            return Err(err);
        }
        for program in self.build.programs {
            let path = program.split('/').fold(content.clone(), |path, part| path.join(part));
            if !path.is_file() {
                return Err(io::Error::new(io::ErrorKind::NotFound, format!("{program} is not in the download")));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
        }
        // What is there under this version (a broken install) makes way.
        if std::fs::symlink_metadata(target).is_ok() {
            std::fs::remove_dir_all(target)?;
        }
        gezik_platform::fs::move_entry(&content, target)
    }

    /// Removes `<data>/tools/<name>-*` folders other than `keep`; one in use stays.
    fn remove_older(&self, tools: &Path, keep: &Path) {
        let Ok(listing) = std::fs::read_dir(tools) else { return };
        let prefix = format!("{}-", folder_name(self.build.tool));
        for entry in listing.flatten() {
            let path = entry.path();
            let older = entry.file_name().to_string_lossy().starts_with(&prefix)
                && path != keep
                && entry.file_type().is_ok_and(|kind| kind.is_dir());
            if older {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

impl Task for DownloadTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Download
    }

    fn title(&self) -> String {
        format!("Downloading {}", display_name(self.build.tool))
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.data_dir.clone()], work: Work::External }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let facts = Facts { is_dir: true, size: 0, modified: None };
        // The download counts instead, once it starts.
        sink.item(PlanItem::new(Stage::Parallel, facts).target(install_dir(self.build, &self.data_dir)).uncounted());
    }

    fn run(&self, _item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let target = install_dir(self.build, &self.data_dir);
        let tools = target.parent().unwrap_or(&self.data_dir).to_path_buf();
        std::fs::create_dir_all(&tools)?;
        cx.found(1, self.build.size);
        let temp = TempFile(cx.temp_file_for(&target));
        self.download(&temp.0, cx)?;
        if sha256(&temp.0, cx)? != self.build.sha256.to_ascii_lowercase() {
            return Err(damaged());
        }
        self.install(&temp.0, &target, cx)?;
        self.remove_older(&tools, &target);
        Ok(Outcome::Nothing)
    }
}

/// The downloaded file, removed when it goes out of scope (a cancel, a failure, or after
/// it was moved into the staging folder).
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The SHA-256 of `path` in lowercase hex; a cancel stops it.
fn sha256(path: &Path, cx: &RunCx<'_>) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        if cx.stopped() {
            return Err(cancelled());
        }
        match file.read(&mut buf)? {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Unpacking's needs: cancel, and the first entry that failed (a tool missing a file is
/// broken: the whole install fails). The download already counted the progress.
struct Unpacking<'a, 'r> {
    run: &'a RunCx<'r>,
    failed: RefCell<Option<io::Error>>,
}

impl ExtractCx for Unpacking<'_, '_> {
    fn add_bytes(&self, _: u64) {}

    fn entry_done(&self) {}

    fn stopped(&self) -> bool {
        self.run.stopped()
    }

    fn password(&self, _: bool) -> Option<String> {
        None
    }

    fn entry_failed(&self, name: &str, error: &io::Error) {
        let mut failed = self.failed.borrow_mut();
        if failed.is_none() {
            *failed = Some(io::Error::new(error.kind(), format!("{name}: {error}")));
        }
    }
}
