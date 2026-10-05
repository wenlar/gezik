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

The published `unrar-ng` 0.7.7 with the changes below, all in `src/open_archive.rs` (plus
the `Redirect` export in `src/lib.rs`) and marked "Gezik patch". The root `Cargo.toml` uses
it through `[patch.crates-io]`; `cargo test --manifest-path vendor/unrar-ng/Cargo.toml --lib`
runs its own tests.

1. **Volume names read past their end.** When UnRAR moves to the next volume it calls back
   with the volume's name. For `RAR_VOL_NOTIFY` that name is the `data()` of a
   `std::wstring`, sized to the name, but the callback copied a fixed 2048 wide characters:
   a heap read past the end of the string on every volume change (8 KB on Linux, 4 KB on
   Windows). Debug builds on Linux caught it now and then as "unsafe precondition(s)
   violated: ptr::copy_nonoverlapping". Fix: read up to the NUL (`from_ptr_str`).
2. **Redirect entries.** `FileHeader` gains `redirect` (UnRAR's `RedirType`: symbolic link,
   junction, hard link, file copy) and `redirect_target` (the header's `RedirName`, read into
   a buffer `read_header` now passes). Gezik skips hard links and file copies, whose source
   UnRAR would resolve itself, and makes symbolic links through its own link rules.
3. **`extract_into(base, file, progress)`.** Passes both DestPath (`base`, the staging
   folder) and DestName. With only DestName, UnRAR's extraction folder is empty and names it
   resolves itself (a hard link's or a copy's source) are taken from the process's current
   folder. Its callback hands each piece of unpacked data (`UCM_PROCESSDATA`) to `progress`
   and returns -1 (UnRAR's user break) when `progress` returns `false`: exact progress, and a
   cancel inside an entry. Test: `extract_into_passes_the_base_folder`.

**Updating unrar-ng.** Check each point against the new release. Remove this folder and its
`[patch.crates-io]` line once all three are covered upstream; otherwise copy the new release
here and apply the remaining changes.

## C++ exceptions on MSVC (`.cargo/config.toml`)

UnRAR reports errors and a user break by throwing. The `cc` crate does not pass `/EHsc` to
MSVC, and without it destructors do not run while unwinding: a cancelled or failed entry
kept its file open (Windows then refused to delete it) and its memory allocated.
`.cargo/config.toml` sets `CXXFLAGS_<target>=/EHsc` for the MSVC targets.
