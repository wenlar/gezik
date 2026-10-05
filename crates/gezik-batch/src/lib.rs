//! Batch operations of Gezik. 5a: the engine that turns rename rules into new names, and
//! the EXIF date reader. The renames themselves run as `gezik_ops::RenameTask`.

pub mod exif;
pub mod rename;
