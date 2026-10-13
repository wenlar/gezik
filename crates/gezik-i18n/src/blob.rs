//! The built-in languages' packed form (spec 15 §4.3), written by `build.rs` and read by
//! `catalog`: the template's keys once, as sorted 64-bit FNV-1a hashes, and per language the
//! translations in the same order, forms split by `\x1F`, entries by `\0` (then deflated).
//! Only `std`: `build.rs` includes this file.

use std::collections::HashMap;

/// Between the forms of a plural.
pub const FORM: char = '\u{1f}';
/// Between entries.
pub const END: char = '\0';

/// 64-bit FNV-1a of a catalogue key (`po::key`).
pub fn hash(key: &str) -> u64 {
    key.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The translations of `keys` (sorted hashes) in their order; a key with none is empty.
pub fn pack(keys: &[u64], translations: &HashMap<u64, Vec<String>>) -> String {
    let clean = |s: &String| s.replace([FORM, END], "");
    let entries: Vec<String> = keys
        .iter()
        .map(|k| translations.get(k).map(|forms| forms.iter().map(clean).collect::<Vec<_>>().join("\u{1f}")))
        .map(Option::unwrap_or_default)
        .collect();
    entries.join("\0")
}
