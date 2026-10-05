//! Batch operations of Gezik. 5a: the engine that turns rename rules into new names, and
//! the EXIF date reader. The renames themselves run as `gezik_ops::RenameTask`. 5b: reading
//! archives into a staging folder, and the engine tasks that extract them.

pub mod archive;
pub mod exif;
pub mod rename;
pub mod tasks;
