//! Batch operations of Gezik. 5a: the engine that turns rename rules into new names, and
//! the EXIF date reader. The renames themselves run as `gezik_ops::RenameTask`. 5b: reading
//! archives into a staging folder, the engine tasks that extract and make them, and
//! downloading the tools Gezik runs (7-Zip). 5c: converting pictures and text.

pub mod archive;
pub mod convert;
pub mod exif;
pub mod rename;
pub mod tasks;
pub mod tools;
