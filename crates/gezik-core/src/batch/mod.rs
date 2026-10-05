//! Batch operations' pure parts: rename rules, name templates, dates, letter case and archive
//! decisions. The engine that applies them is the `gezik-batch` crate.

pub mod archive;
pub mod case;
pub mod date;
pub mod rules;
pub mod template;
pub mod tools;
