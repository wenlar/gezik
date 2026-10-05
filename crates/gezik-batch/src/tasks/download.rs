//! Downloading a tool: the system's `curl` into a temporary file (its growing size is the
//! progress), the SHA-256 checked, then unpacked by Gezik's own archive reader into a staging
//! folder and moved to `<data>/tools/<name>-<version>/` in one step. Older versions go after.
//! Not undone and not in the history: the folder may be deleted by hand.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_core::batch::tools::ToolBuild;
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};
use gezik_platform::ChildProcess;
use sha2::{Digest, Sha256};

use super::cancelled;
use crate::archive::{self, ExtractCx};
use crate::tools::{damaged, display_name, folder_name, install_dir, on_path};

/// How often the temporary file's size is looked at.
const POLL: Duration = Duration::from_millis(200);

/// The folder in the staging folder that the download is unpacked into.
const CONTENT: &str = "x";

/// Downloads, checks and unpacks `build` into `install_dir`; then removes older versions.
pub struct DownloadTask {
    build: &'static ToolBuild,
    data_dir: PathBuf,
    url: String,
}

impl DownloadTask {
    pub fn new(build: &'static ToolBuild, data_dir: PathBuf) -> DownloadTask {
        DownloadTask { build, data_dir, url: build.url.to_owned() }
    }

    /// Downloads from `url` instead of the build's own address (tests).
    pub fn with_url(mut self, url: String) -> DownloadTask {
        self.url = url;
        self
    }

    /// Runs curl into `temp` (in `folder`, its working folder), counting what arrives.
    fn download(&self, curl: &Path, folder: &Path, temp: &Path, cx: &RunCx<'_>) -> io::Result<()> {
        let size = self.build.size;
        let mut child = ChildProcess::spawn(curl, arguments(&self.url, temp, size), Some(folder))?;
        let counted = Cell::new(0u64);
        let looked = Cell::new(Instant::now());
        let status = child.wait_or_stop(|| {
            if looked.get().elapsed() >= POLL {
                looked.set(Instant::now());
                let now = std::fs::metadata(temp).map_or(0, |meta| meta.len()).min(size);
                if now > counted.get() {
                    cx.add_bytes(now - counted.get());
                    counted.set(now);
                }
            }
            cx.stopped()
        })?;
        let Some(status) = status else { return Err(cancelled()) };
        if !status.success() {
            return Err(io::Error::other(message(&child.stderr_text())));
        }
        cx.one_done(size.saturating_sub(counted.get()));
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
        let curl = curl().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "curl not found"))?;
        let target = install_dir(self.build, &self.data_dir);
        let tools = target.parent().unwrap_or(&self.data_dir).to_path_buf();
        std::fs::create_dir_all(&tools)?;
        cx.found(1, self.build.size);
        let temp = TempFile(cx.temp_file_for(&target));
        self.download(&curl, &tools, &temp.0, cx)?;
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

/// The system's curl (Windows 10 1803+ has one in System32).
fn curl() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        let system = PathBuf::from(root).join("System32").join("curl.exe");
        if system.is_file() {
            return Some(system);
        }
    }
    on_path(&["curl"], &std::env::var_os("PATH")?)
}

/// curl's arguments: no `.curlrc` (`--disable`, which must come first), HTTPS only for the
/// address and every redirect (plain HTTP too for an `http://` address, which only tests
/// give), at most `size` + 1 MiB, into `temp` given by its name (curl runs in its folder: no
/// non-ASCII path in the arguments).
fn arguments(url: &str, temp: &Path, size: u64) -> Vec<OsString> {
    let protocols = if url.starts_with("http://") { "=http,https" } else { "=https" };
    let max = size.saturating_add(1 << 20).to_string();
    let mut args: Vec<OsString> = [
        "--disable",
        "--fail",
        "--location",
        "--proto",
        protocols,
        "--proto-redir",
        protocols,
        "--max-redirs",
        "5",
        "--max-filesize",
        &max,
        "--connect-timeout",
        "30",
        "--silent",
        "--show-error",
        "--output",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(temp.file_name().unwrap_or(temp.as_os_str()).to_owned());
    args.push(url.into());
    args
}

/// What curl said went wrong, in a line.
fn message(errors: &str) -> String {
    let lines: Vec<&str> = errors.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.is_empty() { "the download failed".to_owned() } else { lines.join(" ") }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_arguments() {
        let temp = Path::new("tools").join(".gezik-copying-1-0");
        let args = arguments("https://example.com/a.zip", &temp, 1000);
        let expected = [
            "--disable",
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-redirs",
            "5",
            "--max-filesize",
            "1049576",
            "--connect-timeout",
            "30",
            "--silent",
            "--show-error",
            "--output",
            ".gezik-copying-1-0",
            "https://example.com/a.zip",
        ];
        assert_eq!(args, expected);
        let http = arguments("http://127.0.0.1:1/a.zip", &temp, 1000);
        assert_eq!((&http[4], &http[6]), (&OsString::from("=http,https"), &OsString::from("=http,https")));
        assert_eq!(
            message("curl: (22) The requested URL returned error: 404\n"),
            "curl: (22) The requested URL returned error: 404"
        );
        assert_eq!(message(""), "the download failed");
    }
}
