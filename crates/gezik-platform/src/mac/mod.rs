//! macOS: AppKit, CoreGraphics and Quick Look calls (spec 9 §4.1-§4.5). Only the glue
//! is here; what can be decided without the system lives in `icons.rs`, `picture.rs` and
//! `finder.rs`, tested on every system.

pub(crate) mod finder;
pub(crate) mod icons;
mod image;
pub(crate) mod info;
pub mod open_urls;
pub(crate) mod open_with;
pub(crate) mod ql_panel;
pub mod reopen;
pub(crate) mod services;
pub(crate) mod share;
pub mod terminate;
pub(crate) mod thumbs;
