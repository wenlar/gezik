//! The file operations engine: runs [`Task`]s (copy, move, trash…) off the UI thread, one
//! queue per drive, with conflicts asked up front, progress, pause and cancel.

mod control;
mod engine;
mod inverse;
mod journal;
pub mod pending;
mod run;
mod task;
mod tasks;
#[cfg(test)]
mod testing;
pub mod walk;

pub use engine::{ConflictItem, Engine, Event, Failure, JobId, JobState, PauseReason, Progress, Report, Settings};
pub use gezik_core::ops::conflict::{ConflictKind, Decision, Facts};
pub use pending::PendingDeletes;
pub use task::{
    Answer, ChangedSince, NoTrash, Outcome, PlanItem, Question, Resources, Restart, RunCx, ScanSink, Stage, Task,
    TaskKind, Work, changed_since, facts_after, is_restart, no_trash, restart, unchanged,
};
pub use tasks::{
    CopyTask, DeleteTask, GroupTask, LinkTask, MoveTask, NEEDS_ADMIN, NewTask, RenameTask, RestoreTask,
    SetAttributesTask, TrashTask, trash_path,
};
