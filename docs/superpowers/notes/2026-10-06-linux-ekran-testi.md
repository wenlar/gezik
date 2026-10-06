# Gezik Linux GUI test, 2026-10-05/06

Branch feat/batch-ops-5c (current master features plus scripts), debug build, container `gezik-linux` (rust:1-trixie).
- X11: Xvfb 1600x900 with no window manager, real XTEST input through xdotool.
- Wayland: headless sway (1280x720 output), wtype for keys and `examples/vpointer` for the pointer.

New script: `scripts/linux/gui.sh [x11|wayland|all]`, with helpers in `scripts/linux/gui-lib.sh`. It is committed as 81e85f3.

Final runs:
- `gui.sh all`: X11 40 ok and 1 FAIL; Wayland 20 ok and 1 FAIL. Both failures are the same focus bug (F1 below).
- `test.sh all`: 0 failures.

Screenshots are in `D:\Work\gezik\.superpowers\linux-shots\gui-*.png`.

7-Zip: Debian trixie's `7zip` package is now **25.01**, which is at least 25, so Gezik would use it. The script uses it only to make the test archives. It then renames `/usr/bin/7z*` so that Gezik shows its download offer.

## Results

| # | Item | X11 | Wayland |
|---|------|-----|---------|
| A1 | Window opens | PASS | PASS |
| A2 | Open folder (double click / Enter), Alt+Left back, Alt+Up up | PASS | PASS |
| A3 | Copy+paste, cut+paste within Gezik | PASS | PASS (copy) |
| A4 | Delete → `~/.local/share/Trash/files` + `.trashinfo` Path; Ctrl+Z restores | PASS | PASS |
| A5 | Ctrl+Shift+N → folder named in place | PASS | PASS |
| A6 | F2 rename in place (stem preselected) | PASS | PASS |
| A7 | Ctrl+Z / Ctrl+Y (move, rename) | PASS | PASS (undo) |
| A8 | Keyboard reaches the list after window focus loss during F2 / batch layer | **FAIL** (F1) | **FAIL** (F1) |
| B1 | F2 on 3 items opens "Rename 3 items" | PASS | PASS |
| B2 | Find/Replace rule → preview → Rename on disk | PASS | PASS |
| B3 | Ctrl+Z restores the batch | PASS | PASS |
| B4 | Swap a.txt↔b.txt via manual names (contents checked) + undo | PASS | NOT TESTED (X11 only) |
| B5 | Esc closes the layer | PASS | PASS |
| C1 | Right click → Gezik menu → Extract here, single-root zip (→ `one/` as is) | PASS | PASS (Menu key) |
| C2 | Extract here, multi-root zip (→ `multi/`) | PASS | PASS |
| C3 | Extract here, tar.gz | PASS | NOT TESTED (X11 only) |
| C4 | AES zip: password dialog, field shows dots (Show reveals), wrong → "Wrong password. Try again:", right → extracted | PASS | PASS |
| C5 | Compress… → zip | PASS | NOT TESTED |
| C6 | Compress… → 7z with password + Encrypt file names (`7z l -pbad` fails, `7z t -ppw7` ok) | PASS | NOT TESTED |
| C7 | Compress to "a.zip" (follows the last format; was "a.7z" right after the 7z) | PASS | NOT TESTED |
| C8 | Drag z1.txt onto zz.zip → "Add to zz.zip" badge → conflict box → Keep both → "z1 (2).txt" in zip | PASS | NOT TESTED (vpointer drag possible, not scripted) |
| C9 | .wim → "7-Zip needed" (~1014.6 KB) → Download → `<config>/tools/7zip-26.03/7zz` (runs: "7-Zip (z) 26.03") → extraction continues into `w/` | PASS | PASS |
| C10 | Cancel (×) at ~49% of a 1.8 GB stored zip → no `big/`, no `.gezik-*` | PASS | NOT TESTED |
| D | test.sh (GNOME/URI clipboard formats, xclip/wl-copy paste, XDND, Wayland DnD both ways) | PASS (8/8) | PASS (6/6) |

Counts: X11 has 22 PASS and 1 FAIL. Wayland has 15 PASS, 1 FAIL and 7 NOT TESTED. test.sh passes on both.

## Failures

### F1. When the window loses keyboard focus while an inline editor or the batch layer has it, the file list gets no keys afterwards

This happens on both X11 and Wayland.

Steps on X11:
1. Run `gui.sh x11`.
2. In /tmp/t, click `a.txt` and press F2. The name editor opens.
3. Give focus to another X window (`xev`), then give it back to Gezik with `xdotool windowfocus`.
4. The editor is gone and the name is unchanged, which is fine.
5. Press F2 or Down. Nothing happens: no editor and no change of selection. Keys work again only after a click in the list.

Control case: the same focus switch with only a row selected (no editor open) keeps the keyboard in the list, and Down works.

On Wayland, every wtype run is a new virtual keyboard that goes away at its end, so it hits the same path:
- Ctrl+Shift+N or F2 followed by typing in a *separate* wtype loses the editor. Afterwards End, Up and F2 do nothing until a click.
- After Rename in the batch layer, the next F2 does nothing until a click.

Screenshots: `gui-x11-focus-away.png`, `gui-x11-focus-back.png`, `gui-wl-f2-after-batch.png`.

Likely cause: when the focused text field is destroyed, or the window is blurred, Slint focus goes nowhere, and nothing gives it back to the list when the window is focused again.

Impact: alt-tab during a rename leaves keyboard navigation dead until a click.

## Observations

- **Test-harness pitfall, not a Gezik bug.** xdotool `type` without `windowfocus` loses keys under Xvfb with no window manager. This happened after a mouse-only menu flow: the password field stayed empty and Enter (which was sent with focus) submitted an empty password. `gui-lib.sh`'s `typ` now focuses first. Under sway, any text typed into an inline editor must come in the same wtype run as the key that opened it (F1).
- **"This PC" in the breadcrumb on Linux.** The root crumb reads "This PC › / › tmp", which is a Windows term. Sidebar headings are FOLDERS (Home) and DRIVES (File System).
- **Self-referential "Compress to" label.** On a single archive, the menu offers `Compress to "one.zip"`, which is the archive's own name. It was not run, so whether it would collide or pick "one (2).zip" was not checked.
- **Batch rename layer size.** At the 900x600 default window, the layer fills almost the whole window (24 px margins). It is usable, and the preview column has plenty of room. The rules are remembered between opens in a session: the second open showed `Replace "photo" → "img"`.
- **Compress layer growth.** It grows when 7z is chosen (Encrypt file names, Split rows) and moves up, so fixed coordinates change. The split buttons fit their text.
- **Download size text.** The 7-Zip box says "~1014.6 KB" where "~1.0 MB" might read better. The download was fast and was verified to work (`License.txt` ships next to `7zz`).
- **Operations panel.** Skipped password prompts (Esc) leave "Extracting aes.zip · Done · 1 item skipped" rows, which stack until dismissed. A cancelled extract disappears from the panel without a "Cancelled" line. Extract throughput was about 525 MB/s in the debug build.
- **Transient staging folder.** During the first big extract, a transient `.gezik-deleting-x-*/x/big/...` folder showed up in the target folder for a moment and was cleaned up by the end. After a cancel, none was left.
- **Other dialogs.** The password dialog is titled "Password", with OK/Skip; Esc means Skip. The "7-Zip needed" dialog has Download as its default button (Enter works on Wayland).
- **Log noise.** Every start logs "Error watching for xdg desktop settings: … /run/user/0/bus" because there is no D-Bus session in the container. It is harmless.
- **Fonts and rendering.** DejaVu Sans renders cleanly. The bullet glyph for passwords is present, and the arrows (← → ↑ ↓ ×) render. HiDPI is not testable (scale 1 on Xvfb and headless sway).
- **System 7-Zip 25.01 not exercised.** With Debian's 7-Zip 25.01 visible, Gezik should use it without offering a download. This was NOT TESTED because the binaries were hidden to test the offer.
