//! The file operations engine: runs [`Task`]s (copy, move, trash…) off the UI thread, one
//! queue per drive, with conflicts asked up front, progress, pause and cancel.

mod control;
mod engine;
pub mod hash;
mod inverse;
mod journal;
pub mod meta;
pub mod pending;
mod run;
mod task;
mod tasks;
#[cfg(test)]
mod testing;
pub mod walk;

pub use engine::{
    ConflictItem, Engine, Event, Failure, JobId, JobOptions, JobState, PauseReason, Progress, Report, Settings,
    move_next_to,
};
pub use gezik_core::ops::conflict::{ConflictKind, Decision, Facts};
pub use pending::PendingDeletes;
pub use task::{
    Answer, ChangedSince, NoTrash, Outcome, PlanItem, Question, Resources, Restart, RunCx, ScanSink, Stage, Task,
    TaskKind, Work, changed_since, facts_after, is_restart, no_trash, restart, unchanged,
};
pub use tasks::{
    CANCELLED, CloudPinTask, CopyTask, DeleteTask, ElevatedTask, Elevator, GroupTask, HashResults, HashRow, HashTask,
    LinkTask, MAX_CHECKSUM_FILE, MaterializeTask, MoveTask, NEEDS_ADMIN, NOT_AVAILABLE, NOT_SO, NewTask, Pick,
    RenameTask, RestoreTask, RowState, Seen, SetAttributesTask, SharedResults, SyncStep, SyncTask, TagsTask, TrashTask,
    UNCHECKED, Verdict, in_a_bin_folder, sync_label, trash_path,
};
