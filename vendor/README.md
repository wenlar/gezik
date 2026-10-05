# Vendored crates

## i-slint-core 1.18.1 (patched)

The published `i-slint-core` 1.18.1, unchanged except for one fix in `properties.rs`
(`impl Drop for PropertyTracker`, marked "Gezik patch"). The root `Cargo.toml` uses it
through `[patch.crates-io]`.

**The bug.** The software renderer keeps each `Text`'s shaped paragraphs in a cache
(`textlayout/sharedparley/cache.rs`). Drawing a text from the cache makes the item's
rendering tracker depend on the cache entry's tracker, which depends on the `text`
property. Once the cache holds more than 1024 entries, a sweep drops the entries that were
not used in the last two frames, trackers included. Dropping a tracker unlinked its
dependents without marking them dirty, so the rendering tracker no longer depended on the
text at all: a later change of the text was never repainted until something redrew the
whole window. In Gezik, scrolling a large folder fills the cache, and then the status bar
kept showing an old selection count (any rarely redrawn text whose size stays the same is
affected).

**The fix.** A dropped `PropertyTracker` marks the bindings that depend on it dirty, as a
changed dependency would.

**Test.** `crates/gezik/tests/slint_patch.rs` fails without the fix. The crate's own tests
cannot be built outside the Slint repository.

**Updating Slint.** When upgrading, check whether the new release fixes this (drop of a
`PropertyTracker` with dependents). If it does, remove this folder, the
`[patch.crates-io]` section and the `exclude` entry in the root `Cargo.toml`, and the
`i-slint-core` dev-dependency and test in `crates/gezik`. If not, copy the new release here
and apply the same change to `properties.rs`.

## unrar-ng 0.7.7 (patched)

The published `unrar-ng` 0.7.7, unchanged except for one line in `src/open_archive.rs`
(the `UCM_CHANGEVOLUMEW` callback, marked "Gezik patch"). The root `Cargo.toml` uses it
through `[patch.crates-io]`.

**The bug.** When UnRAR moves to the next volume it calls back with the volume's name. For
`RAR_VOL_NOTIFY` that name is the `data()` of a `std::wstring`, sized to the name, but the
callback copied a fixed 2048 wide characters from it: a heap read past the end of the
string on every volume change (8 KB on Linux, 4 KB on Windows). Debug builds on Linux
caught it now and then as "unsafe precondition(s) violated: ptr::copy_nonoverlapping" in
`rar_missing_volume_is_a_clear_error`; a release build could crash at the end of a heap
mapping.

**The fix.** The name is read up to its NUL (`WideCString::from_ptr_str`); UnRAR always
ends it with one.

**Updating unrar-ng.** Check whether the new release still reads a fixed length there. If
it does not, remove this folder and its `[patch.crates-io]` line; if it does, copy the new
release here and change the same line.
