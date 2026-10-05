//! Batch operations' pure parts: rename rules, name templates, dates and letter case. The
//! engine that applies them is the `gezik-batch` crate.

pub mod case;
pub mod date;
pub mod rules;
pub mod template;
