//! File operations: the pure rules (names, conflicts, paths, undo history, speed). The
//! engine that runs them is the `gezik-ops` crate.

pub mod conflict;
pub mod history;
pub mod names;
pub mod paths;
pub mod rate;
pub mod renames;
pub mod threads;
