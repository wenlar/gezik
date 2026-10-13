# l10n overflow: the 15b screen test's findings — notes

Stacked on `feat/i18n-15c-messages` (PR #126). Findings from the pseudo-language screen test
(Windows) of 15b; English draws as before except where noted.

## What changed

- **Label columns** (`Labeled` in compress, convert, info, rename-batch): the label's width is
  `max(<old px>, its text's preferred width)`. A row whose translated label is longer grows on
  its own (limit: its field then starts further right than the rows above it; a shared column
  would need every label measured in each sheet).
- **Compress level buttons:** `min-width: max(64px, text + 2 spacings)` (`TextButton`'s
  `content-width` is now an `out` property). English stays at 64 px.
- **Info window permission grid:** the label column is the widest of Owner/Group/Everyone (at
  least 80 px), each box column the widest of Read/Write/Execute (and Hidden/Locked on macOS)
  plus the box (at least 84 px), measured by hidden `Text`s. The panel grows past 400 px when
  the grid needs it.
- **Conflict list Action column:** at least 110 px, wider for the longest translated decision
  (Replace, Skip, Keep both, If newer, Merge; hidden `Text`s with the same msgids as
  `Decision::label`).
- **Settings rows:** the control column is at least 240 px, wider when a translated button
  (Set Key…, Edit in settings.toml, System Integration…) needs it. The key text of a shortcut
  row (and a list's count) starts at the column's left edge: without `x` it was centred, so a
  long key ran under the button. This moves it left in English too (it was ~40 px in).
- **Window title on a language change:** `apply_config` calls `update_chrome` on every pane
  when the language changed, so `{} — Gezik` and the tabs' names follow at once (15c already
  routed the title through `t!`).
- **Already fixed in 15c:** the settings window's search hint is `t!`; an empty text is never
  looked up (the shortcuts row's `— « ··»`); a test now checks the empty case.

## Left as they were

- Conflict list Source/Target (200 px) and the tool table's Status (110 px): data columns that
  elide (sizes, dates, `(newer)`, statuses with error text); English widths kept.
- Convert's Quality (72 px) and Background (96 px) and other fixed fields hold values, not
  labels.

## Checks

fmt, workspace clippy `-D warnings` and tests on Windows; `cargo clippy -p gezik --target
aarch64-apple-darwin --no-default-features -D warnings`. Pot regenerated (two Slint
references added, no new texts). Screen re-check exe: `%TEMP%\gezik-gui-15\bin\tl10nfix.exe`.
