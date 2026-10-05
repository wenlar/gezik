//! Converting files (5c): pictures (`image`, with in-place EXIF edits in `exifclean`), and
//! the ffmpeg steps they need. The decisions (presets, sizes, names, ffmpeg's arguments) are
//! in `gezik_core::batch::convert`.

pub mod exifclean;
pub mod ffmpeg;
pub mod image;

pub use self::image::{ImageError, ImageJob, convert_image, is_needs_ffmpeg, remove_location, strip_location};
