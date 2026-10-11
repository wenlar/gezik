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

## i-slint-backend-winit 1.18.1 (patched)

The published `i-slint-backend-winit` 1.18.1, unchanged except for four changes marked
"Gezik patch", all for Windows only (`cfg(target_os = "windows")`). The root `Cargo.toml`
uses it through `[patch.crates-io]`.

**The problem.** The software renderer draws into softbuffer's surface, which keeps one DIB of
width x height x 4 bytes (4 MB at 1280x800). On Windows, Slint keeps the native window and
this surface while the window is hidden (in the tray) or minimized, so the buffer stays
committed although nothing is drawn. `SLINT_DESTROY_WINDOW_ON_HIDE` would free it but destroys
the native window itself (position, drop target, the tray's window).

**The fix.**
1. `renderer/sw.rs`, `occluded(true)`: the surface is replaced by a fresh one on the same
   window and context, which has no buffer until the next frame (that frame is drawn whole;
   `occluded` already asked for that).
2. `renderer/sw.rs`, `render`: a minimized window does not draw (Windows still asks it to;
   drawing would allocate the buffer again).
3. `winitwindowadapter.rs`, `set_visible(false)`: calls `occluded(true)`, since Windows sends
   no `Occluded` event for a hidden window (a minimized one gets it through the zero-size
   resize).

**Check.** With the window in the tray or minimized, private memory drops by the buffer and
showing or restoring draws the whole window again (Gezik's own restore redraw is in
`windows.rs`); `scripts/perf/hidden.ps1` measures both.

4. `renderer/sw.rs`, `render_to_window`: the window is drawn line by line
   (`SoftwareRenderer::render_by_line`) and copied to the window in bands of up to 16 lines with
   GDI (`StretchDIBits` on the window's DC), so no frame buffer exists while the window is shown
   either: only the band buffers (16 lines x the dirty width x 4 bytes each, one per distinct
   line range), kept between frames and freed by `occluded(true)`. DWM keeps the window's
   content, so as with softbuffer's reused buffer only the dirty region is drawn; the first
   frame, a frame after a size or scale factor change, one after `occluded`, and one while
   Windows has an update region of its own (`GetUpdateRect`, e.g. after a display change) are
   drawn whole. A guard releases the DC even if rendering panics, and the next frame is then
   drawn whole. The environment variable `GEZIK_SOFTBUFFER` (any value) goes back to softbuffer,
   for comparisons or if something draws wrong. Line rendering needs paths, see
   `i-slint-renderer-software` below.

**Check.** With the window in the tray or minimized, private memory drops by the buffer and
showing or restoring draws the whole window again (Gezik's own restore redraw is in
`windows.rs`); `scripts/perf/hidden.ps1` measures both. For 4: screenshots (`PrintWindow`) with
and without `GEZIK_SOFTBUFFER` are identical; idle memory and `scripts/perf/stress.ps1` with both.

**Updating Slint.** If the new release frees the software renderer's surface while hidden or
minimized on Windows, drop 1-3; if not, copy the new release here and apply the changes again.
4 stays as long as softbuffer keeps a full-window buffer on Windows.

## i-slint-renderer-software 1.18.1 (patched)

The published `i-slint-renderer-software` 1.18.1, unchanged except for path drawing in
line-by-line mode (`lib.rs`, `PrepareScene::process_filled_path`, `process_stroked_path` and
`process_path`, marked "Gezik patch"). The root `Cargo.toml` uses it through
`[patch.crates-io]`.

**The problem.** `render_by_line` (used by the winit patch 4 above) skipped `Path` items: every
icon drawn with a path (sidebar, toolbar, breadcrumb arrows, tab close buttons) was missing.

**The fix.** A path is rendered, clipped, with the buffer renderer's own code
(`path::render_filled_path` / `render_stroked_path`) into a premultiplied RGBA buffer of the clip's
size (straight into the texture's own buffer), which goes into the scene as a texture. The pixels
are the same as with `render`. The `path` feature comes with the default `std` feature, as
before.

**Updating Slint.** If the new release draws paths in `render_by_line`, remove this folder and its
`[patch.crates-io]` line; otherwise copy the new release here and apply the same change.

## C++ exceptions on MSVC (`.cargo/config.toml`)

UnRAR reports errors and a user break by throwing. The `cc` crate does not pass `/EHsc` to
MSVC, and without it destructors do not run while unwinding: a cancelled or failed entry
kept its file open (Windows then refused to delete it) and its memory allocated.
`.cargo/config.toml` sets `CXXFLAGS_<target>=/EHsc` for the MSVC targets.
