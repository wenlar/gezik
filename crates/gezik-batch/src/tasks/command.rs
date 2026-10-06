//! Running a user command (`settings.toml` `[[commands]]`) on each chosen item as one engine
//! job: no shell, the item's folder as the working folder, no input, no window. With an
//! `output` the command writes to a temporary name that is renamed when it succeeded; without
//! one it changes the item in place, after a copy of the item went to the trash so that undo
//! can bring it back.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::batch::convert::{CommandSpec, check_command, command_applies, expand_command, expand_output_name};
use gezik_core::ops::paths::{path_key, same_path};
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use gezik_platform::ChildProcess;

use super::convert::needs_trash;
use super::{cancelled, file_name, what};
use crate::convert::ffmpeg::stderr_tail;

/// The plan item's note: an item to run on, an item the command does not take.
const RUN: u8 = 0;
const LEFT_OUT: u8 = 1;

pub struct CommandTask {
    inputs: Vec<(PathBuf, bool)>,
    spec: CommandSpec,
}

impl CommandTask {
    /// Runs `spec` on each of `inputs` (absolute paths, each with whether it is a folder).
    pub fn new(inputs: Vec<(PathBuf, bool)>, spec: CommandSpec) -> CommandTask {
        CommandTask { inputs, spec }
    }

    /// Runs the command (`args` from `expand_command`) in `input`'s folder; an error unless
    /// it ended with 0. A pause ends it (it would go on using the machine while the job is
    /// paused) and the answer is `restart`; a cancel ends it too.
    fn execute(&self, input: &Path, args: &[std::ffi::OsString], run: &RunCx<'_>) -> io::Result<()> {
        let Some((program, rest)) = args.split_first() else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "run has no program"));
        };
        let name = self.spec.run.first().map_or_else(String::new, |p| p.trim().to_owned());
        let program = find_program(program)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{name} not found")))?;
        let dir = input.parent().filter(|dir| !dir.as_os_str().is_empty());
        let mut child = ChildProcess::spawn(&program, rest, dir)?;
        // Its output is read (and dropped) so that it never waits on a full pipe.
        let lines = child.stdout_lines();
        let status = child.wait_or_stop(|| {
            lines.ready().for_each(drop);
            run.cancelled() || run.paused()
        })?;
        let Some(status) = status else {
            return Err(if run.cancelled() { cancelled() } else { gezik_ops::restart() });
        };
        if status.success() {
            return Ok(());
        }
        let tail = stderr_tail(&child.stderr_text());
        let code = status.code().map_or_else(|| status.to_string(), |code| format!("exit code {code}"));
        Err(io::Error::other(if tail.is_empty() {
            format!("{name} failed ({code})")
        } else {
            format!("{name} failed ({code}): {tail}")
        }))
    }

    /// With an output: the command writes into a staging folder of its own next to the
    /// target. `{out}` is the output's name in it and `{outdir}` the folder itself, so a tool
    /// that names its output itself (LibreOffice's `--outdir`) writes there too, never over a
    /// file next to the target. Once it ended with 0, the file the `output` template names is
    /// moved from there to the target (which the conflict list may have renamed: "Keep
    /// both"). The folder goes when the item ends, or at the next start if Gezik stops first.
    fn with_output(&self, input: &Path, is_dir: bool, target: &Path, run: &RunCx<'_>) -> io::Result<Outcome> {
        let template = self.spec.output.as_deref().unwrap_or_default();
        let name = expand_output_name(template, input, is_dir).map_err(invalid)?;
        let staging = run.staging_dir(target)?;
        let made = staging.join(&name);
        let result = expand_command(&self.spec, input, is_dir, Some(&made))
            .map_err(invalid)
            .and_then(|args| self.execute(input, &args, run))
            .and_then(|()| {
                if std::fs::symlink_metadata(&made).is_err() {
                    return Err(io::Error::other(format!("the command made no {}", name.to_string_lossy())));
                }
                gezik_platform::fs::move_entry(&made, target)
            });
        // With whatever the command left in it (the job removes it too, if this fails).
        let _ = std::fs::remove_dir_all(&staging);
        result?;
        let made_dir = target.is_dir();
        Ok(Outcome::Created { path: target.to_path_buf(), facts: facts_after(target, made_dir), from: None })
    }

    /// In place: a copy of the file goes to the trash first (under the file's name for undo),
    /// then the command changes the file. A pause ends the command and puts the file back as
    /// it was (from the trashed copy); once resumed the command runs on it again.
    fn in_place(&self, input: &Path, is_dir: bool, size: u64, run: &RunCx<'_>) -> io::Result<Outcome> {
        if is_dir {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "a command without an output runs only on files"));
        }
        if !run.has_trash(input) {
            return Err(needs_trash());
        }
        let args = expand_command(&self.spec, input, false, None).map_err(invalid)?;
        let before = facts_after(input, false);
        let copy = run.temp_file_for(input);
        if let Err(err) = run.copy_file(input, &copy, size) {
            let _ = std::fs::remove_file(&copy);
            return Err(err);
        }
        let trashed = match gezik_ops::trash_path(&copy) {
            Ok(trashed) => trashed,
            Err(err) => {
                let _ = std::fs::remove_file(&copy);
                return Err(err);
            }
        };
        let result = loop {
            match self.execute(input, &args, run) {
                Err(err) if gezik_ops::is_restart(&err) => {
                    if let Err(err) = std::fs::copy(&trashed, input) {
                        break Err(err);
                    }
                    if run.stopped() {
                        break Err(cancelled());
                    }
                }
                ended => break ended,
            }
        };
        // The file as the command left it: undo trashes it only if it is still so.
        let outcome = Outcome::Several(vec![
            Outcome::Created { path: input.to_path_buf(), facts: facts_after(input, false), from: None },
            Outcome::Trashed { original: input.to_path_buf(), trashed },
        ]);
        match result {
            Ok(()) => Ok(outcome),
            // Unchanged: nothing to undo (the copy stays in the trash).
            Err(err) if facts_after(input, false) == before => Err(err),
            // Changed before it failed or was cancelled: undo brings the copy back.
            Err(err) => {
                if !(err.kind() == io::ErrorKind::Interrupted && run.cancelled()) {
                    run.fail(input, &err);
                }
                Ok(outcome)
            }
        }
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// The program a command's `run[0]` names: an absolute path as it is, a bare name in an
/// absolute folder of PATH (on Windows with `.exe` or `.com` added when it has no extension).
/// `None`: not found (a relative path with folders is not looked up). It touches the disk.
pub fn find_program(program: &OsStr) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return executable(path).then(|| path.to_path_buf());
    }
    if path.components().count() != 1 || program.is_empty() {
        return None;
    }
    let names: Vec<std::ffi::OsString> = if cfg!(windows) && path.extension().is_none() {
        [".exe", ".com"]
            .iter()
            .map(|ext| {
                let mut name = program.to_os_string();
                name.push(ext);
                name
            })
            .collect()
    } else {
        vec![program.to_os_string()]
    };
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| names.iter().map(|name| dir.join(name)).find(|path| executable(path)))
}

/// A file that may be run (on Unix: with an execute bit).
fn executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    meta.is_file()
}

impl Task for CommandTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Command
    }

    fn title(&self) -> String {
        let paths: Vec<PathBuf> = self.inputs.iter().map(|(path, _)| path.clone()).collect();
        format!("Running {} on {}", self.spec.name, what(&paths))
    }

    fn count(&self) -> usize {
        self.inputs.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.inputs.iter().map(|(path, _)| path.clone()).collect(), work: Work::External }
    }

    fn workers(&self) -> Option<usize> {
        Some(usize::from(self.spec.parallel.clamp(1, 16)))
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        if let Err(message) = check_command(&self.spec) {
            for (input, _) in &self.inputs {
                sink.failed(input, invalid(message.clone()));
            }
            return;
        }
        let inputs: HashSet<Vec<String>> = self.inputs.iter().map(|(input, _)| path_key(input)).collect();
        let mut outputs: HashSet<Vec<String>> = HashSet::new();
        for (root, (input, is_dir)) in self.inputs.iter().enumerate() {
            let meta = match std::fs::metadata(input) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(input, err);
                    continue;
                }
            };
            let facts = Facts { is_dir: meta.is_dir(), size: meta.len(), modified: meta.modified().ok() };
            let item = PlanItem::new(Stage::Parallel, facts).source(input).top(root);
            if !command_applies(&self.spec, &file_name(input), *is_dir) {
                if !sink.item(item.tag(LEFT_OUT)) {
                    return;
                }
                continue;
            }
            let Some(template) = &self.spec.output else {
                if !sink.item(item.tag(RUN)) {
                    return;
                }
                continue;
            };
            let target = match expand_output_name(template, input, *is_dir) {
                Ok(name) => input.parent().unwrap_or(Path::new("")).join(name),
                Err(message) => {
                    sink.failed(input, invalid(message));
                    continue;
                }
            };
            let key = path_key(&target);
            if same_path(&target, input) || inputs.contains(&key) || !outputs.insert(key) {
                let message = format!("its output {} is a selected item or another one's output", file_name(&target));
                sink.failed(input, io::Error::new(io::ErrorKind::AlreadyExists, message));
                continue;
            }
            if !sink.item(item.target(target).checked().tag(RUN)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(input) = &item.source else { return Ok(Outcome::Nothing) };
        if item.tag == LEFT_OUT {
            let why = io::Error::new(io::ErrorKind::Unsupported, format!("not for {}", self.spec.name));
            run.skip(input, &why);
            return Ok(Outcome::Nothing);
        }
        let is_dir = item.facts.is_dir;
        match &item.target {
            Some(target) => self.with_output(input, is_dir, target, run),
            None => self.in_place(input, is_dir, item.facts.size, run),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(parallel: u8) -> CommandSpec {
        CommandSpec {
            name: "Resize to 50%".into(),
            run: vec!["magick".into(), "{in}".into()],
            output: None,
            types: Vec::new(),
            folders: false,
            parallel,
        }
    }

    #[test]
    fn title_and_workers() {
        let task = CommandTask::new(vec![("d/a.jpg".into(), false), ("d/b.jpg".into(), false)], spec(4));
        assert_eq!(task.title(), "Running Resize to 50% on 2 items");
        assert_eq!(task.workers(), Some(4));
        assert_eq!(task.resources().work, Work::External);
        assert_eq!(CommandTask::new(Vec::new(), spec(0)).workers(), Some(1));
    }

    #[test]
    fn programs_are_found_by_absolute_path_only_or_on_path() {
        let exe = std::env::current_exe().unwrap();
        assert_eq!(find_program(exe.as_os_str()), Some(exe.clone()));
        assert_eq!(find_program(OsStr::new("gezik-no-such-program-here")), None);
        assert_eq!(find_program(OsStr::new("")), None);
        let relative = Path::new("deps").join(exe.file_name().unwrap());
        assert_eq!(find_program(relative.as_os_str()), None);
    }
}
