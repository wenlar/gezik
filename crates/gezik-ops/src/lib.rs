//! The file operations engine: runs [`Task`]s (copy, move, trash…) off the UI thread, one
//! queue per drive, with conflicts asked up front, progress, pause and cancel.

mod control;
mod engine;
mod run;
mod task;
#[cfg(test)]
mod testing;
pub mod walk;

pub use engine::{ConflictItem, Engine, Event, Failure, JobId, JobState, PauseReason, Progress, Report, Settings};
pub use gezik_core::ops::conflict::{ConflictKind, Decision, Facts};
pub use task::{
    ChangedSince, NoTrash, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since,
    facts_after, no_trash, unchanged,
};
