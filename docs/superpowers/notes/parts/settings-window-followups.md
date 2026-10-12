# Settings window follow-ups — notes

Follow-ups to 14d–14f (branch `fix/settings-window-followups`, on top of `feat/portable-14f`).

## Slint "long changed callback chain" warnings

- **Cause**: the rows' title `Text` had `wrap: word-wrap` and `overflow: elide`. Slint measures such a text unwrapped for its layout width and wrapped for its height, and its text cache keeps one wrap mode per item: each measure drops the other's entry. Our vendored `i-slint-core` patch (a dropped `PropertyTracker` dirties its dependents) turns each drop into a dirty binding, so the two measures dirty each other for ever. Nothing re-reads the layout between frames until a text field has the keyboard: Slint then tracks the focused field's position, and that tracker re-ran the layout every round. Import, Export and Keep Settings in a Folder focus their path field (and Get Theme its address field), so the window logged on every event (~700 an import, thousands with a sync folder); 14e's screen test used buttons and saw two.
- **Fix**: the title keeps `word-wrap` without `elide` (it wraps, so the elision never showed). No other text in a layout has both (the grid tiles' names have fixed sizes). `vendor/README.md` notes the side effect.
- **Check**: `windows::tests::a_reload_with_settings_open_settles` builds a whole window and Settings headless (the test platform now has an event loop proxy that a test can drain, and logs what Slint says), opens Settings at Import, then reloads after an import, a sync folder set up and the folder going away. Each step may log at most two warnings; with `elide` back, opening alone logged 106.
- Not the watcher: the warnings came without any reload. The Linux reload storm (13 reloads for one change, CI) is the separate `watcher.rs` fix on `ci/github-actions`.

## A theme named `Dracula 2.toml`

- Its id is "dracula 2" (ids are lowercase); choosing it resolves from the files read, so no path is built from the id there. Delete, Replace and Check for Updates already find the file by `theme_path` (14c).
- **The sync folder's last copy** wrote each theme as `<id>.toml`: `dracula 2.toml`, and leaving the folder while it was away copied that name into the local folder. It now keeps each theme under its real name, and an older copy named by the id goes (on a case-insensitive system it is the same file, written again). Test: `store::tests::a_theme_with_capitals_is_kept_under_its_real_name`.

## Delete Theme asking behind Settings

Dropped here: fixed on `fix/macos-run3` (aba75f4).

## Rows after a sync folder is set up

- `settings_window::same` compared neither `place` nor `default`, so the rows kept their old place (no "this computer" mark, no Only on This Computer in the menu) until the page changed. It compares both now. Test: `a_sync_folder_set_up_changes_the_rows_in_place`.
