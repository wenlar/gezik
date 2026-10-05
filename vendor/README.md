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
