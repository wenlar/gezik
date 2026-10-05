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
    ChangedSince, NoTrash, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since,
    facts_after, no_trash, unchanged,
};
pub use tasks::{CopyTask, DeleteTask, MoveTask, NewTask, RenameTask, RestoreTask, TrashTask};
