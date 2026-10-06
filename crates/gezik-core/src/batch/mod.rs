//! Batch operations' pure parts: rename rules, name templates, dates, letter case, archive
//! decisions and conversion choices. The engine that applies them is the `gezik-batch` crate.

pub mod archive;
pub mod case;
pub mod convert;
pub mod date;
pub mod pdf;
pub mod pdf_worker;
pub mod rules;
pub mod template;
pub mod tools;
