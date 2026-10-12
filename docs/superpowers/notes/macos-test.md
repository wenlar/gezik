# Gezik on macOS: build and manual test

Run 1 (2026-10-06, `feat/batch-ops-5c`) covered items 1-22; its fixes are on master. Everything since (items 23-152: 5c, 5d, 6a, 7a-7c, 8a-8b, the design rounds and step 9 parts 9a1-9a3, 9b1-9b6) has never run on a Mac.

**Run 2 is one run on `master`** (all of these are merged). Write it as a new section **"Run 2 (master)"** in `macos-test-results.md`, below Run 1, and don't change Run 1. Where an item compares two branches (exe size items 93, 100, 109), just write today's `ls -l target/release/gezik`.

Suggested order, so the cheap checks find build or API problems first:
1. Build, `cargo test --workspace` (macOS-only tests run only there), then every probe the items name (`mac_probe`, `cloud_probe`, ...) with their whole output pasted into the results.
2. Command-line items (9b1, 9b3: `--help`, `--unregister`, single instance) from Terminal.
3. Window items, part by part, in the checklist's order.

If Claude Code runs this on the Mac, it does steps 1-2 itself and walks the person through step 3 item by item (it can't click in Gezik's window).

This file is for a person, or Claude Code on the Mac. Write the results into `docs/superpowers/notes/macos-test-results.md`.
- Give PASS, FAIL or NOT TESTED for every item.
- For a FAIL, give the steps, what you expected, what happened, and a screenshot path. Don't commit screenshots.
- Start the section with the date, the branch and short SHA, the machine, the macOS version and the keyboard layout.
- Add a summary table like Run 1's.

## Build

```sh
xcode-select --install            # C/C++ compiler (UnRAR is C++); skip if already installed
curl https://sh.rustup.rs -sSf | sh   # Rust; then open a new terminal
git clone git@github.com:wenlar/gezik.git && cd gezik   # or: git fetch in the existing clone
git checkout master && git pull
git log -1 --oneline              # write this SHA into the results
cargo build --release -p gezik
rm -rf /tmp/gezik-cfg && mkdir -p /tmp/gezik-cfg /tmp/gezik-test
GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
```

- Start from an empty `/tmp/gezik-cfg`, so that the 7-Zip, ffmpeg and pdfium download offers appear. Gezik downloads them into `/tmp/gezik-cfg/tools/`.
- Hide any ffmpeg on PATH while testing the ffmpeg download. Homebrew's is at `/opt/homebrew/bin/ffmpeg`. Start Gezik with a PATH that leaves it out:
  ```sh
  PATH=/usr/bin:/bin:/usr/sbin:/sbin GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
  ```
- Run the automated tests once with `cargo test --workspace`, and report any failures with their output.
- After the pdfium download (item 42), also run the real-pdfium tests:
  ```sh
  GEZIK_TEST_PDFIUM=/tmp/gezik-cfg/tools/pdfium-8086/libpdfium.dylib \
      cargo test -p gezik-batch --test pdf_worker --test pdf_tasks
  ```
  Use the dylib's real path from `ls /tmp/gezik-cfg/tools/`.

### Test data

Put a few files and folders in `/tmp/gezik-test`. Also make:
- **Archives** with `zip`, `tar czf`, or 7-Zip if it is installed. A RAR file is in `crates/gezik-batch/tests/data/` (see SOURCES.md there).
- **A second volume** for cross-volume copies, because copies on the same APFS volume are clones and finish at once:
  ```sh
  hdiutil create -size 4g -fs HFS+ -volname GezikHedef -type SPARSE /tmp/hedef && hdiutil attach /tmp/hedef.sparseimage
  ```
- **Pictures:**
  - A few JPEGs from the camera or the web, one of them with EXIF orientation 6 (a portrait phone photo).
  - A PNG with transparency.
  - **A real iPhone HEIC** (AirDrop one from a phone; these are tiled, in Display P3). If there is no iPhone, use `sips -s format heic photo.jpg --out photo.heic`, and say in the results that it was made with sips.
- **Text** in Windows-1254: `printf 'ılık İstanbul\r\n' | iconv -f UTF-8 -t WINDOWS-1254 > tr1254.txt`.
- **Audio and video:** any .mov from the screen recorder (⇧⌘5) or the phone, and an .m4a or .mp3.
- **PDFs:**
  - Print any page to PDF from Safari, a few times.
  - Make one with 200+ pages (for example, join prints in Preview), for cancel and memory.
  - Make an encrypted one: in Preview, File ▸ Export…, then "Encrypt" with a password.

## Checklist

Items 1-22 are Run 1's, updated for what has been fixed since. Items 23-34 re-check the fixes. Items 35-46 are new (5c and 5d).

Basics
1. The window opens. It follows the system light/dark mode, and switching the mode while Gezik runs changes the theme.
2. Navigation:
   - Double-click a folder; ⌘↓ opens, ⌘[ goes back, ⌘] goes forward and ⌘↑ goes up.
   - ⌘T opens a new tab and ⌘W closes it.
   - The sidebar shows Home and the drives.
3. Enter renames (Finder style). Escape cancels. A name with `/` shows a red tip.
4. ⌘⇧N creates a new folder and opens it for renaming.
5. ⌘C / ⌘X / ⌘V work inside Gezik. ⌘⌥V pastes as a move.
6. **Finder interop:**
   - Copy a file in Gezik, then paste in Finder (⌘V).
   - Copy a file in Finder, then paste in Gezik.
   - Cut in Gezik shows the item faded.
7. **Trash:**
   - ⌘⌫ moves the item to the Trash and it appears there in Finder.
   - ⌘Z brings it back.
   - ⌘⌥⌫ deletes permanently, after a question.
8. **Drag and drop:**
   - Drag from Gezik to a Finder window: the file is copied, or moved on the same volume.
   - Drag from Finder into Gezik, onto the list and onto a folder row: the row is highlighted and the label says "Move to …". With ⌥ held it says "Copy to …" and copies.
   - Drag between two Gezik tabs.
   - Drag onto a sidebar folder.
9. Space opens quick look. ⌘A selects all. ⌘2 is grid view and ⌘1 is list view.
10. Copy a large file (1-2 GB) to GezikHedef. The operations panel shows progress, and pause, resume and cancel all work.
11. Pasting over existing files opens a conflict list. Replace, Skip and Keep both work, and ⌘Z undoes them.

5a, batch rename
12. Select 3 or more files and press Enter. The "Rename N items" layer opens.
13. Replace, Number and Case update the preview live. Rename applies the names, and ⌘Z undoes them.
    - With Turkish first in System Settings ▸ General ▸ Language & Region, Case gives Turkish i/İ in a normal launch, with no `LC_ALL`.
14. Swap two names (a.txt ↔ b.txt) by typing them, then undo.
15. Presets:
    - Save, reopen the layer, apply the preset, then Delete it.
    - Check that `settings.toml` in /tmp/gezik-cfg has `[[rename-presets]]`.
    - The Presets menu stays fully on screen (see 31).

5b, archives
16. Right-click a zip, then Extract here. A single-root zip goes in as it is; a multi-root zip goes into a `<name>/` folder. ⌘Z removes what was extracted.
17. tar.gz, 7z and rar (from the test data) extract.
18. An AES zip asks for a password. The field shows dots and "Show" reveals the text. A wrong password asks again; the right one extracts.
19. In the Compress… layer, make:
    - a zip;
    - a 7z with a password and "Encrypt file names";
    - a 7z split into parts.
    Also use "Compress to "x.zip"" from the menu, without the layer. Item 32 has the details of the name fixes.
20. Drag files onto a zip row. The label says "Add to x.zip" and the files are added. ⌘Z restores the old archive.
21. **7-Zip download:** open a `.wim` or `.dmg` archive (make a .dmg with `hdiutil create -format UDZO`).
    - The "needs 7-Zip" box appears. Choose Download.
    - The tool lands in `/tmp/gezik-cfg/tools/7zip-26.03/7zz` and runs. Note any Gatekeeper prompt.
    - The archive opens.
22. Cancel a big extraction. No `.gezik-*` folders are left in the target folder (`ls -a`).

Re-checks of the fix/macos fixes (see "Fixes on fix/macos" in the results)

23. **Live refresh through a symlink:** with Gezik on `/tmp/gezik-test`, `touch /tmp/gezik-test/canli.txt` shows up at once.
24. **⌘[ / ⌘] by key place:**
    - On the layout in use, ⌘ + the key right of P goes back, and ⌘ + the next key goes forward. On Turkish-QWERTY-PC these are the keys that type `ğ` and `ü`.
    - Go ▸ Back/Forward in the menu bar work too.
25. **Finder → Gezik drops:**
    - Onto a folder row: the row is highlighted and the file goes into the folder.
    - Onto a zip row: the file is added to the zip.
    - The label follows ⌥ while dragging, and ⌥ copies.
26. **Tab-rest drag:** drag a file onto another tab and rest until the tab opens.
    - Release on the tab: the file moves.
    - Do it again and release over the new list: the file moves.
    - A quick drop before the tab switches also moves it.
    - After a rest, drag out of the window into Finder and drop there. Finder gets the file, and no Gezik ghost is left when the pointer comes back over Gezik.
    - With a mouse that has a right button: right-drag, rest on a tab, release. Only the Copy here / Move here menu appears, with no second context menu after it.
27. **Sidebar drop onto another volume:** dropping a file from `/tmp` onto GezikHedef says "Copy to GezikHedef" and copies.
28. **Language:** with Turkish first in AppleLanguages, Case UPPER previews `İ` when Gezik is started from Terminal and when it is started with no locale variables, as an app from the Dock would be:
    ```sh
    env -u LANG -u LC_ALL -u LC_CTYPE GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./target/release/gezik /tmp/gezik-test
    ```
    A bare binary double-clicked in Finder runs inside Terminal, so that is no test of this.
29. **Menu bar:**
    - There are File, Edit, View, Go and Window menus. The app menu is titled "Gezik", but "About/Hide/Quit gezik" is known (see "Known gaps").
    - ⌘M minimizes.
    - View ▸ as Grid / as List work with the mouse.
    - ⌘C in a name being edited copies text; in the list it copies the file.
    - No crash when menu items are used quickly one after another.
30. **Wording and dotfiles:**
    - The address bar starts with "Computer", and the sidebar section is "LOCATIONS".
    - The Type column shows Finder-style kinds ("Folder", "Plain Text Document").
    - Dotfiles are hidden by default. ⇧⌘. and View ▸ Show Hidden Files show them.
    - Ctrl+H does **not** toggle them on macOS (it is the Windows/Linux default); ⌘H hides the app.

Re-checks of the shared UI fixes (fix/shared-ui, on master)

31. **Menus at the window edge.**
    - Put the window in the middle of the screen, not maximized. Open each of these near the window's bottom-right corner:
      - the context menu on a row at the very bottom right;
      - the Commands ▸ submenu near the right edge;
      - Presets in the rename layer;
      - Preset, From and To in the Convert layer;
      - the View button's menu.
    - Take a **full-screen** screenshot (`screencapture -x ~/Desktop/menu-edge.png`). Don't use a window capture (`-l`, ⇧⌘4 + Space), which cuts everything at the window's frame.
    - On macOS these menus are native and may hang past the window's edge. That is a PASS.
    - It is a FAIL only if items are cut off or off the screen in the full-screen shot, or a list taller than the screen cannot scroll.
    - Run 1's "Presets menu runs past the right edge" and "context menu cut off at the bottom" were probably window captures. Say which they were.
32. **Compress name:**
    - The layer selects only the stem when it opens.
    - With 7z chosen, typing `sifreli` makes `sifreli.7z`.
    - Typing only `.zip` makes `<item name>.zip`, not `.zip.zip`.
    - After a 7z in the layer, the menu still says `Compress to "x.zip"` and makes a plain zip.
    - The layer opens with Split off every time, and `state.toml` keeps no split size afterwards.
33. **⌘ / Ctrl shortcuts:**
    - ⌘Enter starts the rename, Compress and Convert layers, from any field in them.
    - Ctrl+Enter is not needed; note what it does.
    - Esc closes each layer.
    - ⌘Z / ⇧⌘Z undo and redo after each layer's job.
    - Up and Down in a Gezik menu skip greyed lines. (A native macOS menu does this itself; note it if Gezik's menus are native here.)
34. **Archive kinds and icons:**
    - `x.wim` and `big.7z.001`/`.002` show the archive icon, with the types "WIM archive" and "Split 7Z archive".
    - Extract is offered on `.001`, and on a bare `film.001` that is the first part of a 7z set.
    - `clip.264` keeps its own name and isn't taken as an archive part.

5c, convert and user commands (never run on a Mac)

35. **Pictures:** right-click a JPEG, then Convert…
    - The layer opens. The right-click menu also has Commands ▸, and a command whose program is missing is grey.
    - Preset "Resize photos (JPEG 1920 px)" with output "Subfolder", then ⌘Enter. The portrait photo with EXIF orientation 6 comes out upright.
    - With "Remove metadata (incl. location)" on, `mdls -name kMDItemLatitude out.jpg` is empty. With it off, JPEG → JPEG keeps EXIF and location.
    - PNG with transparency → JPEG has a white background.
    - "Replace originals": the originals go to the Trash, and ⌘Z brings them back.
    - "Remove location data" on a phone JPEG drops the GPS tags. The picture isn't re-encoded: the size changes by only a few KB.
36. **Text:**
    - `tr1254.txt`: the layer says "Detected: Windows-1254". Converting to UTF-8 gives `ılık İstanbul` in TextEdit.
    - With "Replace originals", ⌘Z gives the old bytes back (`xxd`).
    - A UTF-8 file with `ı` → Windows-1252 shows an error line ("can't encode 'ı' in Windows-1252 (line N)"), and the file is unchanged.
37. **HEIC and the ffmpeg download (Gatekeeper, quarantine).** Start Gezik with ffmpeg off PATH.
    - Convert the HEIC to JPEG. The "ffmpeg needed" box appears. "Where does it come from?" opens the gezik-tools release page.
    - Choose Download. It lands in `/tmp/gezik-cfg/tools/ffmpeg-9.0.2/` (`ffmpeg`, `ffprobe`, LICENSE, SOURCE.txt), and the conversion continues by itself.
    - Record what these say about the downloaded `ffmpeg`:
      - `file` (arm64 or x86_64?);
      - `xattr -l` (is there a `com.apple.quarantine`?);
      - `codesign -dv`;
      - `spctl -a -vv`.
    - Note any Gatekeeper prompt.
    - **Quarantine test:** run `xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" /tmp/gezik-cfg/tools/ffmpeg-9.0.2/ffmpeg`, restart Gezik and convert again.
      - Record whether it runs, Gatekeeper blocks it, or Gezik shows an error, and the exact text.
      - Then `xattr -d com.apple.quarantine …` and confirm that it works again.
    - **The HEIC result:**
      - right size and upright;
      - colours compared with Preview (a Display P3 → sRGB shift is a known limit);
      - a tiled iPhone HEIC must not come out as one 512 px tile.
    - Also try "Convert to WebP" with WebP (lossy), and "Convert to AVIF": both go through ffmpeg.
38. **Audio and video:**
    - MP3 and M4A (AAC) from the .mov, "Smaller video", "Remux to MP4" and "GIF from video".
    - Progress moves smoothly, and the outputs play in QuickTime.
    - Cancel a "Smaller video" halfway. No half file is left, and `pgrep -l ffmpeg` is empty.
39. **Pause and resume:** pause a long "Smaller video".
    - `pgrep -l ffmpeg` shows that ffmpeg has ended.
    - Resume starts that item again, and the output is complete and plays.
    - Do the same with a user command from 40.
40. **User commands.** Add these to `/tmp/gezik-cfg/settings.toml`. Gezik picks the change up live; check that it does.
    ```toml
    [[commands]]
    name = "Half size (sips)"
    run = ["sips", "-Z", "800", "{in}", "--out", "{out}"]
    output = "{name}-small.{ext}"
    types = ["jpg", "jpeg", "png"]
    parallel = 4

    [[commands]]
    name = "Thumbnail (qlmanage, outdir)"
    run = ["qlmanage", "-t", "-s", "256", "-o", "{outdir}", "{in}"]
    output = "{name}.{ext}.png"

    [[commands]]
    name = "Rotate in place (sips)"
    run = ["sips", "-r", "90", "{in}"]
    types = ["jpg", "png"]

    [[commands]]
    name = "Missing program"
    run = ["no-such-tool", "{in}"]
    ```
    - Commands ▸ in the right-click menu, and Commands in the Convert layer, list the ones for the file's type. "Missing program" is grey.
    - The `{out}` and `{outdir}` commands put their output next to the input, under the `output` name.
      - While a `{outdir}` command runs, nothing half-made shows in the folder; it runs in Gezik's staging folder.
      - A second run opens the conflict list.
    - "Rotate in place": ⌘Z gives the original back, from the copy Gezik put in the Trash.
    - A name with spaces and quotes (`it's a "test".jpg`) works.
    - An invalid command (a typo in a key) is reported and left out.

5d, PDF (never run on a Mac)

41. **Images to PDF** (needs no pdfium):
    - Select 3 JPEGs (one with orientation 6), a PNG with transparency and a HEIC. Right-click, then "Images to PDF…".
    - The HEIC is left out with a note.
    - Change the order by dragging in the list. Choose A4 and margin Small, then ⌘Enter.
    - Preview shows the pages in that order, with the portrait one upright.
42. **pdfium download (does the dylib load?)**
    - Select 2 PDFs, Convert…, then "Merge PDFs". The "pdfium needed" box appears. Choose Download.
    - It lands in `/tmp/gezik-cfg/tools/pdfium-8086/` (`libpdfium.dylib`, LICENSE, licenses/, SOURCE.txt). The merge continues by itself, and the result opens in Preview.
    - Record what `file`, `xattr -l`, `codesign -dv` and `spctl -a -vv -t open --context context:primary-signature` say about the dylib.
    - **Quarantine test:**
      - Add `com.apple.quarantine` to the dylib as in 37. Quit Gezik, start it again, and run a Split.
      - Record whether it loads, or whether the row says "pdfium does not load on this system: …" (with the text).
      - Remove the attribute and check that it loads again.
    - **Note:** a `cargo build` binary has no hardened runtime, so library validation doesn't apply. Whether a signed and notarized Gezik.app can load this dylib is still open. Write down what `codesign -dv --verbose=4 target/release/gezik` shows (flags, TeamIdentifier).
    - Run the `GEZIK_TEST_PDFIUM` tests from "Build".
43. **Split, extract, to images:**
    - Split PDF, Each page: the layer says "Makes N files" and makes N files.
    - Ranges `1-2, 5`: one file per range.
    - A range past the end shows the error live, before you start.
    - Extract pages to a Turkish file name (`çıktı ğüşö.pdf`).
    - PDF to images, PNG at 150 dpi and JPEG at 72 dpi: the page count and sizes are right, and the text is sharp.
    - Running a job again opens the conflict list. ⌘Z removes a job's outputs in one step.
44. **Encrypted PDF:**
    - Split or merge the encrypted PDF. The password box shows dots, and Show reveals the text.
    - A wrong password asks again. The right one works.
    - The output is not encrypted, and the layer said so.
    - In a merge, choosing Skip at the password leaves the whole merge out.
45. **No worker left behind:**
    - After every PDF job above, `pgrep -fl -- --pdf-worker` is empty.
    - Start "PDF to images" (PNG, 300 dpi) on the 200+ page PDF. Pause, resume, then Cancel halfway. No worker is left, and no half files or `.gezik-*` folders remain.
    - Afterwards, `lsof -p $(pgrep -x gezik) | grep -i pdfium` is empty: the library is only loaded in the worker.
    - Write down Gezik's RSS (`ps -o rss= -p $(pgrep -x gezik)`) before and after.
46. **Last choices and keys:**
    - Close and reopen the Convert layer on a PDF and on a picture. Each kind comes back with its own last choices; the range text does not come back.
    - Esc closes the layer and ⌘Enter starts it.
    - `state.toml` has `[convert]`.

6a, keyboard (filter, selection, tabs; never run on a Mac)

47. **Filter:**
    - ⌘F opens a bar above the list with the field focused. `jpg` leaves only names with `jpg`, and the counter says shown / all ("2 / 8").
    - ↓ gives the list the keyboard and the bar stays. Enter on a filtered folder opens it.
    - Esc on the list closes the filter, and a second Esc clears the selection. Esc in the field closes it too.
    - `/` on the list opens it as well.
    - A tab switch keeps each tab's filter. Going to another folder (or Back) opens it without one.
    - With a file named `İSTANBUL.txt`, `istanbul` finds it, and so does `ılık` for `ILIK.doc`.
48. **Pattern box and selection keys:**
    - ⌘= and ⌘- open "Select by pattern" / "Deselect by pattern", starting with the last pattern. "N items match" follows the text, and a bad pattern (`!` alone) is said in red.
    - ⌘⇧I inverts the selection.
    - With a numeric keypad (an external keyboard): keypad + / - open the box, and keypad / brings back the selection of the last delete (Delete, ⌘Z, keypad /). ⌥+keypad + on a `.jpg` adds every `.jpg`.
    - In the menu bar, Edit ▸ Filter…, Select by Pattern…, Deselect by Pattern…, Invert Selection, Select Same Type and Restore Selection work with the mouse, and show ⌘F / ⌘= / ⌘- / ⌘⇧I. Window ▸ Reopen Closed Tab shows ⌘⇧T.
    - On a layout where `=` needs Shift (Turkish Q), write down what ⌘= does, and whether `select-pattern = ["num+", "mod+shift+0"]` under `[shortcuts]` makes it work.
49. **Tabs:**
    - With four tabs, ⌘1…⌘4 switch, ⌘7 does nothing, and ⌘9 shows the last.
    - ⌘⇧1 shows the list and ⌘⇧2 the grid, from the keys and from View ▸ as List / as Grid (which show ⌘⇧1 / ⌘⇧2). Also on Turkish Q.
    - Go two folders deep in a tab, filter it, ⌘W, then ⌘⇧T: the tab comes back in its place with its filter, and ⌘[ goes to the folder before.
    - ⌘⇧A opens the tab picker. Typing filters it, ↑/↓ and Enter switch, and Esc closes it.
    - Right-click a tab, then Lock tab: a lock shows and the × goes. ⌘W leaves it open and the status bar says so. Unlock tab.
50. **Typing mode:** add `[keyboard]` `typing = "filter"` to `/tmp/gezik-cfg/settings.toml`. Gezik picks it up live, and a letter on the list opens the filter with it. Back to `"jump"`, a letter jumps to a name again.
51. **Saved filters:** ▾ in the bar, then Save as… "Resimler". `settings.toml` gets a `[[filters]]` entry. ▾ then Resimler fills the bar, and ▾, Delete "Resimler" removes it.

6b, keyboard (path completion, folder history, command keys; never run on a Mac)

52. **Path suggestions:**
    - ⌘L, then type `/Us`. After a short pause, a list under the field shows the sub-folders that match (`Users`), never files.
    - ↓ marks a row, ↑ goes back up (from the first row back to the text). Tab writes the marked row (or the first) into the field with a `/` at the end, and the list shows that folder's sub-folders. Enter on a marked row goes there; with none marked, it goes where the text says.
    - Esc closes the list and typing goes on; a second Esc ends typing and the path parts come back. A click on a row goes there.
    - `~/Desktop` and `$HOME/Downloads` (also `${HOME}/Downloads`) go there. `$GEZIK_NOPE/x` stays as it is and opens nothing; `~veli` is not expanded.
    - Typing doesn't stutter while the list is shown. On a mounted share that doesn't answer (unplug the network after mounting), typing stays smooth and the list is empty after about 1 s.
53. **Folder history:**
    - Go to a few folders. ⌘L, then delete the text: the empty field lists "Recent" (the last 5) and "Frequent" (the most visited).
    - In another folder, type a part of a visited folder's name: under the sub-folders, a "History" heading lists the matches.
    - Delete a visited folder in Finder, then open the empty list: it goes from the list within 1 s.
    - Go ▸ Clear Folder History empties the list, and the status bar says "Folder history cleared". `state.toml` has no `[history]` any more.
54. **Command keys.** Add to `/tmp/gezik-cfg/settings.toml`:
    ```toml
    [[commands]]
    name = "Zip together"
    run = ["zip", "-r", "together.zip", "{files}"]
    folders = true
    shortcut = "ctrl+alt+z"
    menu = "Archives"
    ask = true

    [[commands]]
    name = "Clash"
    run = ["true"]
    shortcut = "mod+f"
    ```
    - Select a few files and a folder, then ⌃⌥Z: "Run Zip together on N items?". Cancel runs nothing. Run makes one `together.zip` with all of them, and the panel row says "Done · can't be undone". ⌘Z doesn't undo it.
    - The menu bar has a Commands menu with a greyed "Archives" heading and "Zip together" under it, with ⌃⌥Z in the title. Choosing it asks the same question.
    - Right-click, then Commands ▸: the same greyed heading over its group. The heading can't be chosen.
    - The notice says `commands[2]: shortcut "mod+f" is already used by filter; the command has no key`, and ⌘F still opens the filter.
    - ⌃⌥Z while the path field or the filter field has the keyboard does nothing.
55. **No history:** add `[history]` `remember = false`. `state.toml` loses its `[history]`, and new visits are not kept. Take the line out again.

### 7a, daily

56. **Open terminal.** In a folder with a space in its name, ⌘⌥T (and File ▸ Open Terminal): Terminal.app opens a window in that folder. Right-click a folder ▸ "Open terminal here": the same, in that folder. Right-click empty space ▸ "Open terminal here": the folder shown. Quit Gezik: the Terminal window stays.
57. **Copy path.** Select a file named `it's ş #1.txt` and press ⌘⌥C (and Edit ▸ Copy Path). Right-click ▸ "Copy path as ▸" ▸ Quoted: pasted into Terminal, the shell reads the same file (`'…'\''…'`). file:// URL: pasted into Safari's address bar, it opens the file. With nothing selected, ⌘⌥C copies the folder shown. Copy a file with ⌘C, then ⌘⌥C: ⌘V in another folder pastes nothing.
58. **Session.** Open three tabs, lock one, put another in front, press ⌘Q, open Gezik again: the same tabs, the same one in front, the lock in place. Force quit, open again: the same. `open -a Gezik --args <folder>` (or `gezik <folder>` from Terminal) opens that folder in a new tab after the saved ones, in front. `[session]` `restore = false` in `settings.toml`: `state.toml` loses `[session]`, and one tab opens in `start-folder`.
59. **Tab sets.** Window ▸ Save Tabs As…, name it "Work": `settings.toml` has `[[tab-sets]]`. Window ▸ Open Tab Set ▸ Work opens the set after the tabs, its first tab in front. With a tab locked, "Replace tabs with…" (in the tab's right-click menu) keeps it and says so. Note whether the Window menu shows "Open Tab Set" (and the sets) as expected, since it is an `if … : Menu` inside the menu bar.
60. **Custom terminal.** `[terminal]` `command = ["open", "-a", "iTerm", "{dir}"]`: ⌘⌥T opens iTerm in the folder.

### 7b, daily

61. **Pinned groups.** Write by hand in `settings.toml`: `pinned = ["~/Documents", { path = "~/Downloads", name = "DL", group = "Work" }, { path = "~/Desktop", group = "Media" }, { path = "~/Pictures", group = "work" }]` (use your own full paths or `{documents}`-style tokens). The sidebar shows "PINNED", then "Work" (with DL and Pictures) and "Media", as written. Right-click a group heading: "Move group up/down", "Rename group…", "Ungroup", and each does what it says in the file. Drag a pin between two pins of another group: a line shows where, and it lands in that group. Right-click a pin ▸ "Rename…" (empty: the folder's own name again) and "Move to group ▸" (the other groups, "New group…", "No group").
62. **Pins 1-9.** ⌘⌥1…⌘⌥9 go to the pins in the sidebar's order, and Go ▸ Pinned 1…9 too. Try it with the US, Turkish Q and French AZERTY layouts: note whether the menu's shortcut fires or the key goes to the window, and whether ⌘⌥ plus a digit types a character anywhere (path field) instead.
63. **View menu.** View ▸ Hide Extensions, Folders First, Single-Click to Open, Show Hidden Items (⌘⇧.) are check marks; Date Format ▸ and Size Format ▸ have one checked choice each. Choosing one moves the check and writes `settings.toml` (`[view]`); change the file by hand and the checks follow. ⌘⇧. writes `show-hidden` and the "Show Hidden Items" check follows it.
64. **View options.** Hide Extensions: names lose their extension (folders keep theirs; F2 shows the whole name). Folders First off: folders sort among the files. Date Format ▸ Relative: a file saved just now says "1 min ago", one from today "Today 14:05". Size Format ▸ Decimal: a 1,500-byte file is `1.5 kB` (the same as Finder's Get Info). Single-Click to Open: one click opens a folder, ⌘-click and ⇧-click only select.
65. **Hidden by default.** With no `show-hidden` line, `.DS_Store` and other dot files are not shown (macOS default `show-hidden = false`), and the template in a new `settings.toml` has `# show-hidden = true …` as a comment.

### 7c, daily

66. **New ▸ and templates.** Right-click empty space: "New ▸" holds Folder, Text file, Markdown file, the templates, then "Open templates folder". "Open templates folder" opens `~/Library/Application Support/gezik/templates/` in a new Gezik tab. Put `Report.pages` (or any file) and a folder `Project/` with a file in it there: within a second "Project" and "Report" are in the menu (no extension, alphabetical); Finder's `.DS_Store` in that folder is not listed. Each item is made in the folder shown, numbered `(2)` when the name is taken (`Project` comes with its file), and its name is being edited; ⌘Z takes it away (to the Trash).
67. **New folder with selection.** Select three files, ⌃⌘N (and File ▸ New Folder with Selection, and the right-click item "New folder with selection"): they are in "New folder", which is selected and being renamed. ⌘Z puts them back and the folder goes to the Trash; ⌘⇧Z moves them in again. With "New folder" already there, the new one is `New folder (2)`.
68. **Paste as file.** A screenshot to the clipboard (⌃⇧⌘4) and ⌘V on the list: `Pasted image <date> <time>.png`, opens in Preview and looks right (Retina size; it may take a moment, no beach ball). Safari ▸ right-click an image ▸ "Copy Image" ▸ ⌘V: an image, not text. Text from TextEdit: right-click empty space says "Paste text as file", ⌘V makes `Pasted text … .txt` (UTF-8). Files copied in Finder (⌘C): ⌘V pastes the files, not a text file. All of this without Terminal.
69. **Links.** Right-click a file ▸ "Create link": `Link to <name>`, a symbolic link, not a Finder alias (Finder shows the arrow badge). The same on a folder. Drag a file inside Gezik with ⌘⌥ held: the cursor shows the link arrow, the label says "Create link in …", and dropping makes the link. Drag from Finder with ⌘⌥ onto Gezik: a link too. Right-drag (or ⌃-drag) ▸ "Create link here". ⌘Z on a link to a folder: the link goes to the Trash, the folder and its files stay (check in Finder, without Terminal).
70. **Drop stack.** ⌘⇧S and Edit ▸ Add to Drop Stack add the selection; the strip opens above the status bar; View ▸ Drop Stack shows and hides it. Drag files from Finder onto the strip (Finder keeps them). Go to another folder: "Copy here" copies them (one ⌘Z), "Move here" moves them and they leave the strip. Delete a stacked file in Finder: it fades and is left out. With many items, a two-finger swipe scrolls the strip. Drag an item from the strip to Finder or the Desktop. "Clear" empties it.
71. **History.** After a few jobs (one failing: copy onto a locked file, one cancelled), View ▸ Operation History and the status bar's "History" open it: newest first, with time and result; "Show in folder" goes there with the results selected; "Details" lists what failed.

### 8a, search

72. **Search.** ⌘⇧F and Edit ▸ Find… open the search bar above the list ("in <folder> ▾", the name field, Content, Filters, Search). In your home folder type `*.pdf`: results come while you type (the name cache), in a tab titled "Search: *.pdf" whose address bar ends in `Search "*.pdf"`; Back returns to the folder, Forward to the results without searching again. Folder, Modified and Size columns; sort by Folder; ⌘F filters the results. The status bar says "N results in X s" and "Skipped N folders (search.skip)". "Content" ▸ a word from a few text files (one saved as UTF-16 by TextEdit) ▸ Return: those files, with the line in the Match column.
73. **Privacy prompts.** A search in your home folder (first run of this build): macOS asks for Desktop, Documents and Downloads (TCC). Allow some and deny one: the denied folder and the `~/Library` folders macOS keeps to itself count as "N folders could not be read" in the status bar, and the bar's ▾ ▸ "N folders could not be read…" lists them. The search still ends, and no prompt comes twice in one run.
74. **Whole drive.** Scope menu ▸ "Whole drive (/)" with a name that exists under `/Users`: `/Users` is searched (it lives on the Data volume), `/System/Volumes/Data` is not walked a second time (no file appears twice in the results), and other volumes (a USB stick, a disk image under `/Volumes`) are not walked. Esc stops a long search within a moment; there is no beach ball, and scrolling and switching tabs stay smooth while it runs.
75. **Flat view.** ⌘B and View ▸ Flat View in a project folder: every file under it in one list (no folders), the Folder column; ⌘B again on a file goes to its folder with it selected. A symlink to a folder is a row and is not walked into.
76. **Show in folder.** ⌘⇧E and Go ▸ Show in Folder on a result: its folder opens with the file selected; Back returns to the results. The row menu's "Show in folder in new tab" opens it in a new tab.
77. **Results like a folder.** In the results: ⌘C, then ⌘V in another folder (flat). Right-click ▸ "Copy with folders" and ⌘V: the folders under the scope are made; one ⌘Z takes the copies and the made folders away. Move a result to the Trash (⌘⌫): it leaves the list; ⌘Z brings it back. Select three results from two folders ▸ the rename layer (`{n}`): numbered per folder; a name already in that folder says "already in the folder" on its row. Close and open Gezik with a search tab open: the tab comes back and searches again.

### Design round 1 (Graphite)

78. **Themes.** The four built-in themes (`light`, `dark`, `classic-light`, `classic-dark`) and `auto` following the system's light/dark switch (System Settings ▸ Appearance); `reduce-motion = true` stops the hover and popup fades; `density = "compact"`; the classic themes are flat (rows edge to edge, no rounded sheet).
79. **Drawing at 200 %.** On a Retina screen: 1 px lines and the sheet's rounded corner are crisp; the 11 px small text (column header, status bar, sidebar section labels) is readable in the font the system picks; the glyphs (Refresh and History arcs, the dot on Drive), the sidebar icons, the LOCATIONS section; View ▾ and a long popup menu.
80. **Thin scroll bar.** In a folder with thousands of files: a 6 px bar, no track, the theme's color (never the system's), darker on hover and drag, at least 24 px tall; dragging it follows the pointer to both ends; a click above or below scrolls a page; touchpad two-finger scrolling stays smooth (natural direction as the system setting says); the sidebar, a long popup menu, the conflict list and the rename layer have the same bar.

### 8b, folder sizes, command palette, saved searches

81. **Folder sizes.** In your home folder the Size column fills in for folders: `…` while a folder is worked out, the ones on screen first, then a size. Folders macOS asks about (Desktop, Documents, Downloads: TCC) and the parts of `~/Library` it keeps to itself show `≥ …` when they could not be read in full; the preview says "Some folders could not be read". A folder's preview shows its size and `N files, M folders`; three folders selected: the status bar sums them, with `+` while one is still `…`. Sort by Size (both ways): the `…` folders stay last, the list sorts again at most once a second, and the focused row stays where it is on screen. A symlink to a folder has no size. View ▸ Calculate Folder Sizes (and the folder row's menu) works on a network share with `folder-sizes = "local"`; with `folder-sizes = "off"` no folder gets a size. Leaving a big folder (`/System`) at once stops the walk: no beach ball, coming back shows the sizes that were done.
82. **Command palette.** ⌘⇧P opens the picker with `>` and the caret after it; typing keeps the `>` (`copy` lists Copy, Copy Path, Copy with Folders…, shortcuts on the right). Return runs the command, Esc closes it and the list has the keyboard back. Deleting the `>` turns it into Quick Open.
83. **Quick Open.** ⌘P lists places first (pinned, recent folders, tabs, tab sets, saved searches and filters); typing `down` finds Downloads; Alt+Return opens it in a new tab; the last line `Search for "x" in <folder>` starts a search. Go ▸ Quick Open… and Go ▸ Command Palette… do the same as the keys.
84. **Saved searches.** Search for something, then the bar's ▾ ▸ Save search…: a name, then "Save with this folder" or "Save for any folder ({here})". `~/Library/Application Support/gezik/settings.toml` gets a `[[searches]]` entry; comments written there by hand stay. The sidebar's SEARCHES section lists it (magnifier icon): a click runs it, the tab is titled with its name, right-click ▸ Run in new tab / Rename… / Delete. A `{here}` search run in another folder searches there.

### 9a1, system icons, thumbnails, Finder names, aliases, packages

85. **Probe.** `cargo run --release -p gezik-platform --example mac_probe -- ~/Desktop/some.pdf ~/Pictures/photo.heic ~/Movies/clip.mov ~/Documents/report.pages` (any files of these types; add a file that is only in iCloud and a symlink to it if you have one). Paste the whole output into the results. Every line is PASS (NONE is fine for a type Quick Look has no thumbnail for; an iCloud-only file says SKIP); "4 workers at once" must not crash (if it does, or icons come back blank on a worker, NSWorkspace icons have to be serialised); "cancel after 5 ms" says none for a PDF or video. Note the `folder_has_own_icon` times. Then open the folder it names in Finder: "a.txt alias" (now broken), "folder alias" and "Safari alias" show the alias arrow, "folder alias" opens the folder in Finder. Delete the folder. Also run `cargo test -p gezik-platform` and report failures.
86. **System icons.** With `icons = "system"` (the default): `/Applications` in the list and the grid shows each app's own icon; `~` shows Desktop, Documents, Downloads with Finder's special folder icons; `/` shows Applications, Library, System, Users as Finder does; a folder with a custom icon (Finder ▸ Get Info ▸ paste an image on the icon) shows it; a mounted disk image under `/Volumes` shows its volume icon; `.pdf`, `.txt`, `.zip`, `.md` and a file with no extension have Finder's document icons. Icons are the right way up, have clean transparent edges (no dark fringe) and are crisp at 200 %. `icons = "gezik"` brings Gezik's own icons back at once.
87. **Speed and memory.** A folder with 1,000 files of mixed types: the list draws without a visible pause (≤ 150 ms), fast scrolling is as smooth as with `icons = "gezik"`. Scroll `/Applications` in the grid at the largest size: Activity Monitor's memory for gezik stays within ~40 MB of what it was before (the icon and thumbnail caches are bounded). Nothing uses CPU once scrolling stops. A folder with 200 subfolders on a network share (SMB) lists without a pause: each folder is checked for its own icon; note if it is slow.
88. **Quick Look thumbnails.** Grid view with `thumbnails = true`: a PDF shows its first page, a HEIC photo, a MOV a frame, a Pages/Keynote/Numbers file its first page, a PSD (if any) its picture, a `.txt` its text; PNG/JPEG still come at once (Gezik's own); none is upside down. The preview panel shows the same for a PDF and a MOV. Scroll a folder of 500 PDFs fast to the end: the ones on screen come first. Leave a folder of big videos while thumbnails load: CPU drops within a moment; go back: the thumbnails that were cancelled come now (no blank tiles). iCloud Drive with "Optimize Mac Storage": a file that is only in iCloud (cloud icon in Finder) gets no thumbnail and no preview (its icon only) and is **not** downloaded (Finder still shows the cloud icon afterwards); the same for a symbolic link to such a file in another folder.
89. **Finder names.** Put Türkçe first in System Settings ▸ General ▸ Language & Region, log out and in (or restart), then start Gezik: the sidebar says Masaüstü, Belgeler, İndirilenler…; `~` lists Belgeler, Masaüstü, Müzik…; `/` lists Uygulamalar, Kullanıcılar, Sistem, Kitaplık; the address bar's parts and the tab title say Kullanıcılar › <you> › Belgeler; ⌘L shows the real path `/Users/<you>/Documents`. Sort by name: Documents sorts under D. Type "Doc" in the filter: Belgeler is found. F2/Enter on Belgeler edits "Documents" (the real name; Esc, don't rename it). Files in `~` keep their extensions. A pinned `~/Documents` keeps its own label. Put English back first afterwards.
90. **Opening aliases.** In Finder make aliases (⌃⌘A) of a file, a folder, a folder on another volume and `/Applications/Safari.app`. In Gezik double-click and Enter: the folder alias goes into the original folder (Back returns); the file alias opens the original in its app; the Safari alias starts Safari (Gezik does not go into Safari.app). Delete the original file, open the alias: "The original item can't be found" with Delete Alias (the alias goes to the Trash at once, with no second question; ⌘Z brings it back) and OK (nothing happens). A symbolic link to a file still opens the file; a broken one asks the same question. A symbolic link to a folder is entered as before.
91. **Make Alias.** ⌃⌘A, File ▸ Make Alias and right-click ▸ Make Alias (before "Create link") on a file, a folder and three items: `<name> alias` next to each (`rapor.pdf alias`), selected. Finder shows the arrow badge and opens them. Again on the same file: `rapor.pdf alias (2)`; the first alias is untouched. ⌘Z moves the aliases to the Trash; the originals are untouched. Move the original elsewhere in Finder: the alias still opens it.
92. **Packages.** In `/Applications` double-click Safari.app: Safari starts, Gezik does not go in; Enter does the same. Right-click Safari.app ▸ Show Package Contents (right after Open) and File ▸ Show Package Contents: Gezik goes into `Safari.app` (Contents). A Keynote/Pages document that is a folder (`.key`/`.pages` package, e.g. from an older version, or `.rtfd` from TextEdit) opens in its app. A plain folder you name `x.app` (`mkdir /tmp/gezik-test/x.app`) is entered on double-click. The command palette lists Make Alias and Show Package Contents.
93. **Exe size.** `ls -l target/release/gezik` before (on `master`) and after (on `feat/system-9a1`), both `cargo build --release -p gezik`: write both numbers; the difference should be under 256 KB plus the Quick Look and CoreGraphics bindings.

### 9a2, Open With, Share, Quick Actions, Quick Look panel

94. **Probe.** Make a Quick Action in Automator (File ▸ New ▸ Quick Action, "Workflow receives current files or folders in Finder", one "Reveal Finder Items" action, save as "Gezik Test"), then `cargo run --release -p gezik-platform --example mac_probe -- --service "Gezik Test" ~/Desktop/some.txt ~/Desktop/some.pdf ~/Pictures/photo.jpg /Applications/Safari.app`. Paste the whole output into the results. Section 4: every line PASS, each file lists its default app first, and note the "50 items" time (above 50 ms means the menu shows "Loading…" for 50 items). Section 5: the `.txt` offers "Gezik Test"; note the `pbs -dump_pboard` line; `perform "Gezik Test"` is PASS and a Finder window shows the test file.
95. **Open With.** Right-click a `.txt`: "Open With ▸" right after "Open with default app", TextEdit first with "(default)", other apps by name, then "Other…". Choose one: the file opens in it. Select a `.txt` and a `.md`: only apps that open both; select a `.txt` and a `.png`: TextEdit is not offered. "Other…" opens a panel in /Applications where only apps can be chosen; Cancel does nothing; choosing one opens the file in it. Right-click Safari.app and a `.pages` package: "Open With ▸" after "Show Package Contents". A plain folder and This PC rows have no "Open With". Select 60 files: the list is the focused one's, and choosing an app sends all 60 to it at once (not one window per file). If "Loading…" ever shows, right-click the same items again: the apps are there.
96. **Share.** Right-click a file ▸ "Share…": the system's share menu (AirDrop, Mail, Messages, Notes …) opens where you clicked; Mail gets the file as an attachment. Select three files ▸ Share… ▸ Mail: all three. File ▸ Share… and the command palette's "Share…" open it at the pointer (or the window's middle when the pointer is outside). Write whether the menu appeared every time (Apple says it should be opened on a mouse press). The menu stays up until you choose or click away (it is not closed at once) and closes cleanly.
97. **Quick Actions.** Right-click the `.txt`: "Quick Actions ▸" after "Share…" lists "Gezik Test"; choosing it reveals the file in Finder. An image-only Quick Action (Automator: "receives current image files") shows for a `.jpg` and not for a `.txt` or a folder; a mixed selection shows only actions every item fits. Also try a Quick Action that runs for a few seconds (Automator: "Run Shell Script" with `sleep 3; open "$@"`): it still gets the files (Gezik frees its pasteboard right after asking for the service; if a slow action gets nothing, write it down). Select more than 50 rows: no "Quick Actions" item. With no file-taking Quick Action installed, no "Quick Actions" item.
98. **Quick Look panel.** With `[system] quick-look = "system"` (the default): select a PDF, press Space: the system's Quick Look panel (as in Finder) shows it; Space again closes it. With the panel open, ↓/↑ in a list (or ←/→ in the grid) move Gezik's selection and the panel follows; select three files and press Space: the panel shows "1 of 3" and its own arrows page among them; Esc and the panel's close button close it, then Space opens it again. After the panel closes, the keyboard is Gezik's again (arrows move the selection at once, no extra click). A video plays, a folder shows its icon. `quick-look = "gezik"` brings Gezik's own window back at once. If the panel never opens, the status bar says so and Gezik's window opens: write it down (the default then becomes "gezik").
99. **Unchanged.** The row menu still has Make Alias, Create link, Copy path as ▸, Commands ▸ (when set up), and all four submenus show together on a file with commands. Right-clicking feels as quick as before; Activity Monitor shows no extra gezik thread after the menu closes.
100. **Exe size.** `ls -l target/release/gezik` on `feat/system-9a1` and on `feat/system-9a2`, both `cargo build --release -p gezik`: write both numbers; the difference should be under 256 KB plus the Quick Look UI bindings.

### 9a3, the Info window

101. **Probe.** `cargo run --release -p gezik-platform --example mac_probe` and paste the output. Section 6: every line PASS (it includes "setuid kept across a group change" and the locked-file and link checks); note the users/groups line (count and time; above 200 ms the window opens slowly). Also run `cargo test -p gezik-platform attrs` and `cargo test -p gezik-ops attrs` and report failures (their Mac/Linux-only tests: a link is not followed, setuid survives a group change, a locked file is unlocked first, a real chmod is undone through the job engine).
102. **Opening.** Select a PDF, press ⌘I: a panel on the right with "rapor.pdf Info", Kind, Size, Where, Created, Modified, Last opened. File ▸ Get Info and right-click ▸ Get Info (after "Delete permanently") do the same. On a folder the size says "calculating…" then a size. Three items: "3 items", "Where" their folder (or "several folders"), boxes that differ show a dash. Esc and Done close it; the list has the keyboard again.
103. **Permissions.** Tick Group ▸ Write: `ls -l` in Terminal shows `rw-rw-r--` at once; close the window, ⌘Z: back to `rw-r--r--`. Type 600 in Octal and Return: `rw-------`. Type 4755: "Setuid, setgid and sticky can't be changed here", nothing changes. Three files with different permissions: a dashed box turns on for all when clicked. With the window open, `chmod 777 <file>` in Terminal, then tick a box: the note says "1 item changed since; shown as it is now" and the boxes show 777; tick again: it works. With the window open, replace the file in Terminal (`cp other.pdf x && mv x rapor.pdf`), then tick a box: the new file is not changed (the note says "changed since").
104. **Owner and group.** Group ▾ lists your groups (staff, everyone, admin …); choose admin: `ls -l` shows it. A setuid file you own (`chmod 4755 f`): change its group to admin: `ls -l` still shows `rws`. Type `wheel` (if you are not in it) and Return: the note "Requires administrator: 1 item not changed" in red, the operations panel lists the item with "Requires administrator". Owner: type `root`, Return: the same note. Type `nobody-at-all`: "No user is named …", nothing runs.
105. **Hidden and Locked.** Tick Hidden: Finder no longer shows the file (Gezik still does: a known gap). Tick Locked: Finder shows the lock; the permission boxes, Octal, Owner and Group are greyed; untick Locked: they work again. ⌘Z after closing undoes each, in order. A locked file you try to give a group you are not in (refused): it is locked again afterwards (`ls -lO` shows `uchg`).
106. **Apply to enclosed items.** A folder with two files, a script (`chmod +x`), a subfolder and a symlink to a file outside it; set the folder to 750 and group staff, then "Apply to enclosed items…": the question; Cancel does nothing. Apply: `ls -lR` shows the files `rw-r-----`, the script `rwxr-x---`, the subfolder `rwxr-x---`, the symlink and the file it leads to unchanged, the folder itself unchanged. ⌘Z puts every item back. On a big folder (a copy of ~/Library/Caches) the operations panel shows progress and Cancel stops it; ⌘Z puts back the ones it did.
107. **Open with.** One PDF: "Open with: Preview ▾"; choose TextEdit: Finder's Get Info for that file says TextEdit, another PDF still opens in Preview. "Change All…" ▸ Change All: every PDF opens in TextEdit (write down whether macOS asked anything, and the macOS version: the call behind it is deprecated). Cancel and Esc change nothing. Put Preview back the same way. "Other…" opens the app panel. A folder, a link and several items have no "Open with" row; an `.app` has one. A file with no default app says "Not set".
108. **Links and ACLs.** Get Info on a symlink: Kind "Symbolic link", the permission boxes greyed; Group ▾ ▸ another group changes the link's own group (`ls -l` on the link), not the file's (`ls -lL`). `chmod +a "everyone deny delete" <file>`: Get Info says "This item has access control entries…".
109. **Exe size.** `ls -l target/release/gezik` on `feat/system-9a2` and on `feat/system-9a3`, both `cargo build --release -p gezik`: write both numbers; the difference should be under 256 KB.

### 9b1, command line and single instance

110. **Tests first.** In the clone: `cargo test -p gezik-platform instance`. These tests (socket, stale socket, hung and huge callers, peer uid) only compiled on Windows; they never ran on a Mac before.
111. **Hand-over.** With Gezik open, from Terminal: `/path/to/gezik ~/Documents` opens in the same window (if a Documents tab is open, it switches to it), the window comes to the front, and Terminal gets its prompt back at once. `gezik ~/Documents/x.pdf` opens the folder with `x.pdf` selected; Preview does not open.
112. **Help, version, new window.** `gezik --help` and `gezik --version` print to Terminal. `gezik --new-window ~` opens a second window, 32 px offset (note it if the Dock shows a second icon). ⌘N and File ▸ New Window do the same. Close the first window, then the second; open Gezik again: the first window's tabs come back.
113. **Hung or crashed Gezik.** `kill -STOP <pid>`, then `gezik ~`: its own window opens after about 2 s; `kill -CONT <pid>`. `kill -9 <pid>`, then `gezik ~`: a new first Gezik; `gezik /tmp` then goes to it. `ls -l "$TMPDIR"gezik-*` shows an `srw-------` socket and an `-rw-------` lock.
114. **Off.** `[system] single-instance = false` in settings.toml, restart Gezik: every call opens its own window.
115. **Bring to front.** The macOS arm of bringing the window forward (winit `set_minimized(false)` + `focus_window`) was not run or even compiled with tests on Windows: check that a minimized Gezik comes back and comes to the front, and note it if only the Dock icon bounces.

### 9b2, the Trash

116. **Tests first.** In the clone: `cargo test -p gezik-platform trash`, `cargo test -p gezik-platform fs`, `cargo test -p gezik-ops delete` and `cargo test -p gezik-ops restore`. The macOS arms (the `.DS_Store` reader on `~/.Trash`, `mac_original`'s volume rule, `only_items_in_a_bin_count` (only `~/.Trash/x` and `/Volumes/*/.Trashes/<uid>/x` may be deleted for good), `put_back_never_goes_through_a_link` with a symlink, `only_a_plain_file_is_read_as_a_record` with a FIFO, restore under `/var/folders`) only compiled on Windows; they never ran on a Mac. Then `cargo test --release -p gezik-platform ten_thousand -- --ignored` (10,000 items in under 1 s).
117. **Full Disk Access off** (System Settings ▸ Privacy & Security ▸ Full Disk Access: Gezik's terminal or binary not ticked). Click Trash in the sidebar (under the drives): the status bar says `Gezik needs Full Disk Access to show the Trash.` and a box asks once, `Open Privacy Settings` / `Not Now`. `Open Privacy Settings` opens Privacy & Security ▸ Full Disk Access (the `x-apple.systempreferences:…Privacy_AllFiles` address is **to be confirmed**: note where it lands). Open the Trash again in the same session: the note, no box. Items in a USB stick's `/Volumes/<stick>/.Trashes/<uid>` are still listed.
118. **Full Disk Access on.** A file trashed with Finder shows its own name, its Original location and its Date deleted. The `.DS_Store` reader (`ptbL`/`ptbN`) is **to be confirmed** against a real Mac trash: the place must be the one Finder's Put Back uses. Trash two files both named `a.txt` from two folders: Finder renames the second in the bin (`a 10.21.03.txt`); Gezik must show `a.txt` for both, each with its own place. A file trashed with Gezik (⌘⌫): does it have an Original location? (If not, `trashItemAtURL` writes no `ptbL`, and deviation 10's deferred work is needed; note it.) If the reader fails, items show a blank place and Put Back asks for a folder: note that as a failure, with a copy of `~/.Trash/.DS_Store` if you can.
119. **Real bins on USB volumes.** Trash a file on a USB stick (APFS or HFS+, and one exFAT/FAT stick) with Finder: it is listed with its place on the stick (`/Volumes/<stick>/…`). Put Back returns it to the stick, not to the startup disk. A `.Trashes/<uid>` that is a symlink or belongs to another user is not read (`sudo chown` another user on a test stick's folder, if you try it).
120. **Put Back** (row menu, File ▸ Put Back, the palette): the item goes back to its folder and leaves the list and Finder's Trash. Its folder deleted meanwhile: it is made again. A file of the same name there: the conflict list (Keep both gives `a (2).txt`, Replace puts the existing one in the Trash first). ⌘Z puts it back in the Trash. An item with no known place asks for a folder (starts at your home folder); a relative path or a file is refused with a note.
121. **Delete for good and Empty.** In the Trash, ⌘⌫ and the row menu's `Delete Permanently…` ask `Delete N items permanently?` / `This cannot be undone.`; Cancel does nothing. File ▸ Empty Trash…, ⌘⇧⌫, the background menu and the sidebar Trash row's menu ask `Empty the Trash?` / `Permanently delete N items (X GB)? This cannot be undone.` (no size if a folder is in it). The job panel shows progress; cancel an empty with a big folder: what is left still shows in Finder's Trash. Empty Trash with nothing in it: `The Trash is empty`.
122. **Live and refused.** With the Trash open, trash a file in Finder: it shows in Gezik within about 1 s. Go to another folder: Activity Monitor ▸ Gezik ▸ Open Files and Ports no longer lists `~/.Trash`. In the Trash: ⌘C, ⌘X, ⌘V, ⇧⌘N, ⌘D, the rename key and a `[[commands]]` key do nothing and the status bar says `Not available in the Trash`; opening a row (double-click, ⌘↓) opens nothing, with a note; rows can't be dragged out; Go ▸ Show Trash opens it.

### 9b3, the command line and --unregister

Write the results to `macos-test-results.md`. Use a Terminal for the commands; `gezik` below is the binary in the `.app` (or `~/.local/bin/gezik` once added).

123. **Tests first.** In the clone: `cargo test -p gezik-platform system` and `cargo test -p gezik system_changes`. The Unix arms (`replace_symlink` refusing a file or a foreign link, the sweep by exe name) only compiled on Windows; they never ran on a Mac.
124. **Panel.** Palette ▸ `System Integration…` and View ▸ `System Integration…` (menu bar): a box with three rows (`Command line (PATH)` Off/Add, `Changes made: 0`, `Undo all system changes`). Esc closes it; typing in the field does nothing; Up/Down and click work.
125. **Add, no `~/.local/bin`.** Move `~/.local/bin` away first if you have one. `Add gezik to PATH` (panel or palette) asks first and names every place it writes. After it: `~/.local` and `~/.local/bin` exist with mode 0755 and `ls -la ~/.local/bin` shows the `gezik` link pointing at the real binary inside the `.app`. The hint box shows `export PATH="$HOME/.local/bin:$PATH"`; `Copy` puts it on the clipboard (paste it in Terminal). With that line in `~/.zshrc`, a new Terminal's `gezik .` opens the folder in the running Gezik.
126. **Someone else's file.** Remove gezik from PATH, then `echo hi > ~/.local/bin/gezik` and Add again: the file is untouched (`cat` still says `hi`) and the panel says `<place> was not made by Gezik; left alone` (Off, no button). Do the same with `ln -s /usr/bin/true ~/.local/bin/gezik`: the link is untouched. Clean up both.
127. **Gezik moved.** Add, then copy the `.app` to another folder and open it from there: the panel says `Gezik's exe moved; gezik still starts <old path>` with `Update`. After Update, `ls -la ~/.local/bin/gezik` shows the new binary.
128. **`--unregister`.** Quit Gezik (or not: it must work either way). `gezik --unregister` prints one line per change and `echo $?` gives 0. The link is gone; `~/.local/bin` and `~/.local` are removed if Gezik made them and they are empty. Put another file in `~/.local/bin` before: the folder stays, its line says `left (not empty)` and `echo $?` gives 1. A second run says `No system-changes.toml: …` and `Nothing of Gezik's was found.` (0).
129. **No journal.** Add, delete `~/Library/Application Support/gezik/system-changes.toml` (or the `GEZIK_CONFIG_DIR` one), then `gezik --unregister`: only the link that points at this Gezik is swept; `~/.local/bin` stays. Write `version = 9` into a fresh `system-changes.toml`: the panel says `Fix or delete system-changes.toml first`, Add fails, `gezik --unregister` exits 2 and the file is unchanged.

### 9b5, cloud drives

Write the results to `macos-test-results.md` ("Run: 9b5"). Have iCloud Drive on, and OneDrive or Google Drive (File Provider) if you use them.

130. **Tests and probe first.** In the clone: `cargo test -p gezik-platform cloud`, then `cargo run -p gezik-platform --example cloud_probe -- ~/Library/Mobile\ Documents/com~apple~CloudDocs ~/Library/CloudStorage/*` and paste the output (the roots it finds, and each path's state). The macOS arms (`st_flags` → `SF_DATALESS`, the `CloudStorage` reader, the `NSFileManager` calls) only compiled on Windows; they never ran on a Mac.
131. **Sidebar.** A `CLOUD` heading right after the pinned items (before SEARCHES), with iCloud Drive, OneDrive, Google Drive (if installed) by their names; two OneDrive accounts show as `OneDrive (Personal)` / `OneDrive (…)`. Resting the pointer on a row shows the account and the path; a click goes there. `[sidebar] cloud = false` in settings.toml: the heading goes away (live), the row badges stay.
132. **State badges.** In iCloud Drive, a file that is only in the cloud: a small cloud on its icon's corner; selected alone, the status bar ends with `· Online only`. The preview panel and the grid's thumbnail start no download (Finder still shows the cloud icon on it). A downloaded file: a hollow check, `Available on this device`. Outside the cloud folders: no badge, no words.
133. **Download Now / Remove Download.** Right-click a cloud-only file ▸ `Download Now`: the job panel shows it, the file downloads, the badge turns into the check (the folder watcher; if it doesn't, note whether reloading the folder does it). On an uploaded file ▸ `Remove Download`: it goes back to the cloud badge. On a folder ▸ `Remove Download`: does it work, or do you get the note `Not known to be synced yet; left on this device`? (Deviation 2: write down which.) The palette has both commands too.
134. **Data safety.** Turn Wi-Fi off, edit a file in iCloud Drive, then `Remove Download`: the file stays, the job panel says `Not known to be synced yet; left on this device` (a note, not a failure). Turn Wi-Fi on, wait for the upload, try again: it is removed.
135. **OneDrive / Google Drive.** Items 132-134 in `~/Library/CloudStorage/OneDrive-…` and `GoogleDrive-…`: do `startDownloading…` / `evict…` work there, and is the "uploaded" key given (if not, `Remove Download` is always the note: write it down).
136. **Links.** A symlink inside iCloud Drive that points elsewhere: `Download Now` and `Remove Download` both say `A link; left as it is` and the target is untouched. A selection with one item outside the cloud folders: `Not in a cloud folder`, no job.
137. **Late roots.** Quit Gezik with an iCloud Drive folder as the last tab; open it again: the badges show without moving to another folder (the roots arrive after the folder).
138. **Search reads no cloud-only file.** In iCloud Drive, a text file `gezik-cloud.txt` containing `zebra42` that is only in the cloud (right-click ▸ `Remove Download`, the cloud badge shows), and a cloud-only folder if you have one. Search for `zebra42` with content search from iCloud Drive: the file is not found and stays in the cloud (the badge and Finder's cloud icon remain; no download in the Finder sidebar's progress); a cloud-only folder is not gone into. Folder size (the Size column of iCloud Drive's parent) starts no download either. After `Download Now` the same search finds it.

### 9b4, the default file manager

Write the results to `macos-test-results.md` ("Run: 9b4"). Use a Terminal for the commands; `gezik` is the binary in the `.app`. Keep a Finder window handy: `Restore` (or `gezik --unregister`) always gives folders back to Finder.

139. **Tests and probe first.** In the clone: `cargo test -p gezik-platform system`, `cargo test -p gezik system_changes`, then `cargo run --release -p gezik-platform --example mac_probe` and paste section 7 (the `public.folder` handler, `NSFileViewer`, the app delegate's class before winit). The macOS arms (LaunchServices, `CFPreferences`, `application:openURLs:`) only compiled on Windows; they never ran on a Mac.
140. **Panel and palette.** System Integration… shows `Default file manager` as the first row (`Folders open in Finder`, Off, `Make default`); the palette has `Make Gezik the default file manager` and `Restore the system file manager`.
141. **Make default from outside a bundle** (the bare `target/release/gezik`). The confirmation names every place it writes (`~/Applications/Gezik.app`, `public.folder`, `NSFileViewer`); run it once from `~/Downloads`: the first paragraph warns that Gezik is in a temporary place. After it: `~/Applications/Gezik.app` exists and `ls -la ~/Applications/Gezik.app/Contents/MacOS/gezik` is a link to the binary. Does LaunchServices accept the symlinked bundle (item 142 works)? If not, write it down: the copy route is the next commit. From inside a bundle (a `Gezik.app` you built): no `~/Applications/Gezik.app` is made, that bundle is registered.
142. **`open ~/Documents`** opens it in Gezik, with Gezik running (in the open window, the tab rule) and with Gezik quit (a window comes up showing it, without a flash of another folder first). `open -R ~/Documents/x.txt`, Safari's downloads ▸ "Show in Finder", TextEdit ▸ File ▸ "Show in Finder" (where it has one): write down which apps follow `NSFileViewer` and which still go to Finder. `defaults read -g NSFileViewer` gives Gezik's bundle id. The Dock's Finder icon still opens Finder.
143. **`application:openURLs:`.** Does it arrive at all (item 142 shows the folder in Gezik, not just a bare window)? Probe section 7 tells whether winit's delegate had its own method (then Gezik's is not added). Gezik adds the method and then sets the same delegate again (`setDelegate`) so AppKit asks `respondsToSelector:` again: if folders still don't arrive, say so (that re-set is the suspect). Non-file URLs are ignored: `open -a Gezik https://example.com` opens no tab and shows no error.
144. **Bundle launch while Gezik runs.** With Gezik open, `open ~/Desktop`: the folder shows in the running Gezik and the second process ends (`pgrep -l gezik` shows one). `open -a Gezik` with no folder: after about 2 s the running Gezik comes forward and nothing else opens. **The running Gezik gone meanwhile:** `kill -STOP <pid of the running gezik>`, then `open ~/Desktop`: after the 2 s wait plus the send timeout the new process opens its own window showing `~/Desktop`. Its event loop runs a second time after the 2 s wait: no crash, no hang, the window works normally (typing, menus, ⌘W). `kill -CONT <pid>` afterwards.
145. **Gezik moved.** Make default from a bundle, quit, move the `.app` to another folder and open it from there: about 2 s after start a `Gezik moved` box asks `Repair` / `Later`; after `Repair`, `open ~/Documents` opens in Gezik again. Before the repair the panel row says `Update`, which does the same. With no `system-changes.toml` Gezik starts as fast as before (the check is one file lookup).
146. **Restore and `--unregister`.** `Restore` (panel or palette): `public.folder` goes back to Finder (probe section 7), `defaults read -g NSFileViewer` says it does not exist, `~/Applications/Gezik.app` is gone, `open ~/Documents` opens Finder. Make default again, quit Gezik, `gezik --unregister`: the same, one line per change, `echo $?` gives 0. Make default again, delete `system-changes.toml`, `gezik --unregister`: the sweep takes back what points at this Gezik.

### 9b6, eject and Connect to Server

Write the results to `macos-test-results.md` ("Run: 9b6"). You need a `.dmg` (any) or a USB stick, and an SMB share (a NAS, or a folder shared from another Mac or a Windows PC, with a user and password).

147. **Tests and probes first.** In the clone: `cargo test -p gezik-platform network`, `cargo test -p gezik-platform eject`, `cargo run -p gezik-platform --example eject_probe` (each volume and the Eject it is offered; `/` gets `None`), `cargo run -p gezik-platform --example network_probe -- 'smb://nas/Foto Arşivi'`; paste the output. The NetFS (`NetFSMountURLSync`) and `NSWorkspace` arms only compiled on Windows; they never ran on a Mac.
148. **Eject a USB stick or disk image.** Mount a `.dmg` (double-click) and open one folder in it in a tab and another folder in a second tab. Sidebar volume row ▸ right-click ▸ `Eject`: both tabs go to This PC, the volume leaves Finder too, the status says `… can be removed`. Back (⌘[) in one tab asks for the volume's folder: the volume is gone, This PC stays. The same with ⌘E (while showing a folder of the volume) and with File ▸ Eject.
149. **In use.** Keep a file of the volume open in TextEdit, ⌘E: the status says `The drive is in use. Close the files on it and try again.` (or macOS's own text: write it down); after TextEdit closes, ⌘E works. On the startup disk (a folder under `/`) ⌘E: `This drive cannot be ejected`.
150. **Connect to Server.** ⌘K (Go ▸ Connect to Server…): under the field, while typing, `Opens smb://nas/foto` or an error (`\\nas` → `Add the share: smb://server/share`, `smb://a:b@nas/x` → `Leave the password out…`). `Connect` to an SMB share: macOS's login window comes up (Finder does not open), once connected Gezik shows `/Volumes/<share>` and the volume is in the sidebar. The same address again while connected: it goes there again (the EEXIST path; if not, write it down). Cancel: `Not connected`. A wrong server: after a few seconds `Cannot connect to …`, and Gezik responds meanwhile.
151. **Recent.** Quit and reopen Gezik: the ⌘K field opens with the last address; right-click empty space in This PC: `Connect to Server…` and `Connect to smb://…` items; clicking one connects at once. `~/Library/Application Support/gezik/state.toml` (or your config folder) has no password and no user name in `[servers] recent`.
152. **Disconnect.** The connected share's sidebar row ▸ `Disconnect` (or ⌘E while showing it): the volume goes away and its tabs go back to This PC.

### 9b7, the administrator helper

Write the results to `macos-test-results.md` ("Run: 9b7"). Use an admin account (you know its password). The automatic tests never ask for a password; every prompt below is one you answer by hand. The test folder: `sudo mkdir /Applications/GezikTest && sudo chmod 755 /Applications/GezikTest` (root's, so your account cannot write it without the prompt). Remove it at the end with `sudo rm -rf /Applications/GezikTest`.

153. **Tests first.** In the clone: `cargo test -p gezik-core elevated`, `cargo test -p gezik-platform secure`, `cargo test -p gezik-platform elevate`, `cargo test -p gezik-ops elevated`, `cargo test -p gezik elevated::`, `cargo test -p gezik admin::`; paste the output. Two tests run only on a Mac and never ran anywhere yet: `cargo test -p gezik-core -- sh_words_survive_a_real_sh applescript_survives_a_real_osascript` must show both as `ok` (the osascript one only runs `return "<text>"`; no password box may appear: if one does, Cancel it and write it down). The macOS arms of `secure_unix.rs` (`renameatx_np` with `RENAME_EXCL`, `fcopyfile` for xattrs, `fchflags`, opening a FIFO with `O_NONBLOCK`, dropping setuid/setgid in copies, the hard-link refusal) and of `elevate.rs` (`others_can_change`, the osascript launch) were only compiled on Windows: the platform tests `copies_keep_links_and_drop_setuid`, `permissions_and_owners_never_go_through_a_link` (it sets and clears Hidden with `chflags`) and `several_names_are_dropped_but_never_changed` run here for the first time.
154. **Copy an app into /Applications/GezikTest.** Paste a small `.app` there: the job fails with `Access denied`; the panel row ▸ Details shows `Retry as administrator`, `Retry`, `Close`. `Retry as administrator`: first Gezik's own box, titled `Copy 1 item as administrator`, says `Gezik asks the system for administrator rights to do exactly this:` and `Copy /…/X.app to /Applications/GezikTest/X.app`; buttons `Continue` (Return) and `Cancel` (Esc). `Continue`: macOS's password box comes from `osascript` and says `Gezik: Copy 1 item as administrator.` After the password the app is there and opens (its signature and the `Contents/Frameworks/*/Versions/Current` links are intact: `ls -l`), the copy is root's, the row says `Done`, History shows `Copy 1 item as administrator`. `ps ax | grep "gezik --elevated" | grep -v grep` is empty once the row is done.
155. **The "other programs" warning.** Run Gezik from the clone (`target/release/gezik`, in your own folder) and repeat 154 with another item: the box ends with `Gezik is in a folder other programs can change.` Then `sudo mkdir -p /opt/gezik-test && sudo cp target/release/gezik /opt/gezik-test/` and run `/opt/gezik-test/gezik`: the same box has no such line (the binary and every folder above it are root's and not writable by others). Write down if the line shows, or is missing, where it should not. Remove `/opt/gezik-test` afterwards.
156. **Cancel.** Do 154 again and press Cancel in the password box: every item says `Not done: the administrator prompt was cancelled`, History gets nothing, the row's `Retry` asks Gezik's question and the password again. Cancel in Gezik's own box: no password box at all. Two administrator jobs right after each other: does the second ask for the password too (macOS may remember it for about 5 min: write down which)?
157. **Undo and Redo.** After 154, ⌘Z: `Undoing this needs administrator rights.` with the list it sends (`Delete /Applications/GezikTest/X.app permanently`), buttons `Undo` / `Cancel`; Cancel does nothing; Undo → password → the copy is gone. ⌘⇧Z: the same with `Redoing this needs administrator rights.` → password → back.
158. **Get Info.** `sudo touch /Applications/GezikTest/f`. Get Info on `f`: make yourself the owner → `Requires administrator: 1 item not changed` and next to it `Change as administrator…`; click it: Gezik's box lists `Set the owner of /Applications/GezikTest/f to user <your uid> and group <gid>` (numbers, not names) → password → the owner changed (`ls -ln`), the window reads it again; ⌘Z → password → root again. Locked: `sudo chflags uchg /Applications/GezikTest/f`, then in Get Info turn Locked off and change a permission: one box and one password, the box lists `Change the flags of …: Locked off` before the permission line; `ls -lO` shows no `uchg` and the new mode. Turn Hidden on as administrator: `ls -lO` shows `hidden`.
159. **Delete and links.** `sudo ln -s ~/Documents /Applications/GezikTest/link && sudo touch /Applications/GezikTest/g`; delete the whole `GezikTest` folder (moving it to the trash fails with `Access denied`) → Retry as administrator → the box starts with `This will delete 1 item permanently as administrator. It cannot be undone.`, buttons `Cancel` (Return) and `Delete` → password → the folder is gone, `~/Documents` and its files are where they were. ⌘Z does not bring it back (`Nothing to undo` or the job before). Make the test folder again for 160.
160. **Refusals.** (a) Hard links: `sudo sh -c 'echo x > /Applications/GezikTest/h && ln /Applications/GezikTest/h /Applications/GezikTest/h2'`; Get Info on `h`, change a permission → `Change as administrator…` → password: the row fails with `It has several names; Gezik does not change it as administrator`, `ls -l` unchanged. Deleting `h` as administrator works and `h2` still says `x`. (b) setuid: `cp /bin/ls ~/s && chmod 4755 ~/s`, copy `~/s` into `GezikTest` as administrator: `ls -l` shows `-rwxr-xr-x` (no `s`), owner root. (c) FIFO: `mkfifo ~/fifo`, copy it into `GezikTest` as administrator: the row says `A device, pipe or socket; left alone`, Gezik and the helper do not hang (`ps ax | grep "gezik --elevated"` empty). (d) Rename as administrator inside `GezikTest` to a free name works (`renameatx_np`); to a taken name (`h2`) nothing is overwritten (`cat h2`), write down what Gezik said. (e) A system folder itself: select `/Applications` in `/` and Delete → Retry as administrator: the status bar says `Not done as administrator: /Applications (…)` and no box or password comes.

### 9b8, files promised by other programs

Write the results to `macos-test-results.md` ("Run: 9b8"). Programs like Mail, Photos and Safari drag a promise, not a file: Gezik asks for the files into a private folder (`$TMPDIR/gezik-drop-…`, only yours), then a job moves them into the folder you dropped on. Names come from the other program and are cleaned (no `/`, `..` or hidden characters; nothing leaves the folder).

161. **Tests first.** `cargo test -p gezik-core drop_names`, `cargo test -p gezik-ops materialize`, `cargo test -p gezik-platform promise`; paste the output. The promise code was only compiled on Windows: `only_link_promises_are_left_out`, `the_private_folder_is_new_and_only_ours` and `a_plain_copy_keeps_the_quarantine_mark` run here for the first time.
162. **Mail attachment.** Drag an attachment from a Mail message onto a Gezik folder: the drag shows `Copy to <folder>`, a job row `Copying 1 item to …` appears, the file arrives with its name, and it is selected. ⌘Z puts it in the Trash. Drag a whole message from Mail's list: a `.eml` file arrives (or write down what does).
163. **Photos.** Drag two photos from Photos into a folder: two files (HEIC or JPEG: write down which). A photo kept only in iCloud: the job row waits while Photos downloads it, then the file arrives; Gezik responds meanwhile.
164. **Safari.** Drag a picture from a web page: a picture file arrives (or the drag is refused: write down which). Drag a link: nothing is made (no `.webloc`), the drag shows it is refused.
165. **Same name twice.** Drop the same attachment twice: the second time the conflict list asks; Keep both gives `name (2).ext`, the first file stays as it was.
166. **Cancel and clean up.** Start a large promised drop (a long video from Photos, or a large attachment) and cancel the job row at once: nothing is left in the folder; `ls $TMPDIR | grep gezik-drop` lists nothing once the row is gone. An older app that promises files the old way (if you know one, e.g. an older FTP client): its files arrive too (write down the app and what happened).
167. **Into a sidebar row and a tab.** Drop an attachment on a sidebar folder and on a tab: it goes there. On the drop stack strip: refused. On a `.zip` file in the list: it goes into the zip's folder, not into the zip.
168. **The quarantine mark stays.** Save the same Mail attachment once with Mail's own Save and once by dragging it into Gezik (162); do the same for a Safari picture (Save Image As vs 164). `xattr -l` on each pair: if Mail's or Safari's copy has `com.apple.quarantine`, Gezik's copy has it too. A `.zip` attachment with a script or `.app` inside, dropped through Gezik and opened: Gatekeeper asks as it does for a downloaded file. Write down any file that arrived without the mark.

### 9b9, tray icon, global shortcut, start at login

Write the results to `macos-test-results.md` ("Run: 9b9"). Use your own Gezik and config folder. Numbers start at 169: 161–168 are 9b8's (PR #55), so whichever of the two merges second keeps these numbers as they are.

169. **Build and tests first.** `cargo build --release -p gezik`, `cargo clippy -p gezik --all-targets -- -D warnings`, `cargo test -p gezik-platform tray:: hotkey::`, `cargo test -p gezik resident:: integration::`; paste the output. The macOS arms (`NSStatusItem`, Carbon `RegisterEventHotKey`, the Dock reopen method, `applicationShouldTerminate:`) were only compiled on Windows: these are their first runs. In the terminal Gezik prints `the Dock icon does not show a hidden window (winit answers it)` if winit already handles Dock clicks: write down whether it does.
170. **Tray icon.** System Integration (palette ▸ `System Integration…`): the rows `Tray icon` (`Closing the window quits Gezik`, `Turn on`), `Global shortcut` (`No key shows Gezik from other apps`), `Start at login`. `Turn on` the tray: a folder symbol appears in the menu bar (it follows light/dark), tooltip `Gezik`, `~/Library/Application Support/gezik/settings.toml` (or your config folder) now has `tray = true` under `[system]`. Click the symbol: a menu `Show Gezik`, your first nine pinned folders, `Quit Gezik`.
171. **Close to the menu bar.** Close the window (red button): the first time Gezik asks `Gezik keeps running in the menu bar` with `OK` and `Quit Gezik`; `OK` hides the window, the Dock icon stays. Show it again three ways: the menu's `Show Gezik`, a pinned folder from the menu (the window opens there), and a click on the Dock icon (write down if the Dock click does nothing). Close again: no question this time (`state.toml` has `[tray] told = true`). ⌘Q still quits at once, also with the window hidden; the symbol goes, and the tabs and window size are saved (start again: they come back). ⌘Q does not ask about running jobs (known, as before 9b9).
172. **Quit from the menu.** Start a long copy, hide the window, `Quit Gezik` from the menu: the window comes back with Gezik's "jobs are running" question; `Cancel` keeps everything. Without jobs `Quit Gezik` quits at once and the symbol goes. `Turn off` the tray while the window is hidden (edit `settings.toml`: `tray = false`): the window comes back and the symbol goes.
173. **Global shortcut.** `Turn on` in the `Global shortcut` row: a box `Global shortcut` with an empty field and the text `… for example cmd+alt+e. A letter, a digit or F1-F12 with ⌘ (cmd) or ⌃ (ctrl), and ⌥ (alt) or ⇧ (shift).` Type `alt+e`: the line under the field says `"alt+e" needs ⌘ (cmd) or ⌃ (ctrl) with ⌥ (alt) or ⇧ (shift) on macOS` in red and `Turn On` asks again with the reason. Type `cmd+alt+e`: `⌥⌘E will show or hide Gezik`; `Turn On`: the row says `On`, the status bar `⌥⌘E shows or hides Gezik now`; no Accessibility prompt. From another app ⌥⌘E brings Gezik forward; with Gezik in front ⌥⌘E hides it (tray on) or minimizes it (tray off). A shortcut another app holds (one you set in another app): write down whether Gezik says `… is used by another app.` with `Choose Another` / `Close`, or registers it anyway. Turkish Q keyboard: does `cmd+alt+i` follow the key labelled I (write down; the code uses US positions).
174. **--background.** Quit Gezik. `target/release/gezik --background` (tray on): no window, the symbol appears; `gezik ~/Documents` from another terminal opens the window on Documents. Turn the tray off, quit, `gezik --background`: the window opens. With a Gezik running, `gezik --background` does nothing and returns at once. If the symbol does not come within 4 seconds, the window is shown with `The tray icon did not come up; the window is shown instead` (write down if you ever see it).
175. **Start at login.** `Turn on` in `Start at login`: the box lists `the file /Users/<you>/Library/LaunchAgents/com.wenlar.gezik.plist` (and the folder if it was missing) and ends with `At sign-in Gezik starts with --background …`. `Turn On`: `plutil -lint ~/Library/LaunchAgents/com.wenlar.gezik.plist` OK, `cat` shows your exe (an absolute path ending in `gezik`) and `--background`, `RunAtLoad`, `Aqua`, no `KeepAlive`. macOS may show "Background Items Added": write down what it says and where (System Settings ▸ General ▸ Login Items). Log out and in: Gezik is in the menu bar without a window (tray on), or opens its window (tray off). `Show` in the panel lists the change. `Turn off`: the file is gone. `Turn on` again, then `gezik --unregister`: `undone: …/com.wenlar.gezik.plist`, the file is gone.
176. **Memory.** Activity Monitor ▸ Memory for `gezik`: with the defaults, with tray + shortcut on, and hidden in the menu bar for a minute. Write the three numbers.

Report anything else that looks wrong: layout, fonts, Retina scaling, ⌘ shortcuts that don't work, and the wording of the boxes.

### 10d, grouping

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10d starts at 200 as its plan decided (other open branches take the numbers before it). Results go to `macos-test-results.md`.

200. **Tests first.** `cargo test -p gezik-core group layout selection drag`, `cargo test -p gezik view::`, `cargo test -p gezik-core --release -- --ignored grouped_sort`; paste the output (the five times).
201. **Group by Date.** In `~/Downloads`, View button ▸ Group by ▸ Date, and the menu bar's View ▸ Group By ▸ Date: headers `Today (n)`, `Yesterday`, … newest first with a chevron and a line under each; compare with Finder's "Use Groups" by Date Modified and write down what differs. Sort by Date Modified ascending (header click): the groups go oldest first; Name descending: the groups turn round. ↑/↓ skip the headers, ⌘↑ / ⌘↓ (Home/End if mapped) go to the first/last item, Page Up/Down move a screen.
202. **Collapse.** Click a header: its rows go, the header stays with ▸; click again: back, the scroll stays. Right-click a header: `Collapse All Groups`, `Expand All Groups`, `Group by ▸`. The View button's menu has `Collapse All Groups` / `Expand All Groups` only while grouped; the menu bar's View has them always (they do nothing ungrouped). With a group closed: ⌘A, the status bar counts only the shown items; ⌘⌫ moves only those to the Trash (use a test folder of copies). Shift+↓ across a closed group selects nothing in it. Typing the name of an item in a closed group opens the group and selects the item. Rename an item (Return) and, while the field is open, `touch ~/Downloads/zz-new.txt` in Terminal so the groups change above it: the rename field (and its error box, after typing a taken name) stays on the item's line.
203. **Grid.** ⌘⇧2: every group starts on a new line, its header a line as high as the cells (write down if it looks wrong); resize the window: the cells reflow; drag a rubber band across a header: only cells are selected; drop a file on a header: it goes into the folder shown, not a subfolder.
204. **Type and Size.** Group by Type: folders first (`Folder`), then the types by name as the Kind column shows them; Group by Size: `Folders`, `Gigantic` … `Empty`; View ▸ Calculate Folder Sizes: the folders move into their buckets once their sizes come.
205. **Results and Trash.** A search (⌘F) grouped by Type: flat while it searches, grouped when it ends; Go ▸ Trash grouped by Date: the dates are when the items were deleted. Do not empty or put back anything.
206. **Remembered.** Quit and start again: `~/Downloads` is still grouped by Date, another folder is not; View ▸ Apply to All Folders: `settings.toml` has `group = "date"` under `[view]`; Reset This Folder brings back the default.
207. **iCloud.** In an iCloud Drive folder with items only in the cloud, Group by Size: the cloud-only items stay in the size the system gives them (no download starts: `brctl status` or Finder's cloud icons unchanged).

### 10c, the sidebar folder tree

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10c starts at 210 as its plan decided (other open branches take the numbers before it). Results go to `macos-test-results.md`.

210. **Tests first.** `cargo test -p gezik-core tree`, `cargo test -p gezik sidebar`, `cargo test -p gezik-core drag`; paste the output.
211. **Arrows.** Click the arrow before Home, Macintosh HD (LOCATIONS), a pinned folder and iCloud Drive (CLOUD): each opens below its row, folders only, in Finder's name order (`a2` before `a10`), dot folders only with Show Hidden Items (⌘⇧.); toggling ⌘⇧. while a branch is open adds/removes them. A click on the row (not the arrow) goes there as before. Closing and opening reads the branch again.
212. **A huge branch.** `mkdir /tmp/gezik-tree && cd /tmp/gezik-tree && seq 0 49999 | xargs -n 1000 mkdir`, pin `/tmp/gezik-tree`, open its arrow: `…` while it reads, no beachball, the window stays usable meanwhile; then 20,000 rows and a last line `… 30,000 more (open the folder)`; scrolling the sidebar stays smooth; a click on the last line opens the folder. Close it: Activity Monitor's memory for Gezik falls back near where it was.
213. **A link loop.** `mkdir -p /tmp/loop/a && ln -s /tmp/loop /tmp/loop/a/back`; pin `/tmp/loop`, open `a`, then `back`: `back` shows without an arrow (its tip says `Leads back to a folder above it`); nothing hangs.
214. **iCloud, nothing downloads.** In an iCloud Drive folder whose items are only in the cloud (Finder's cloud icons), open branches below it: folder names show; no file starts downloading (`brctl status`, Finder's icons unchanged).
215. **A server.** ⌘K to an SMB share, then open its branch in LOCATIONS; turn Wi-Fi off and open another branch of it: the window stays usable, the arrow shows `…`, later the status bar says `Cannot open …`; close the branch while it reads — nothing appears later.
216. **Drop, menu, new tab.** Drag a copy of a file onto a tree folder: it goes there (⌘Z undoes); right-click a tree folder: the sidebar's folder menu (Open in new tab, Pin, Search in this folder…, Copy path ▸); middle-click (or ⌘Enter from the keyboard) opens it in a new tab. Drag a pinned folder over an open pin's branch: it lands after that pin.
217. **Keyboard and VoiceOver.** Tab until the sidebar has the keyboard (a focus ring on a row); ↑/↓ skip headings, → opens / goes in, ← closes / goes up, typing a name jumps, Enter goes, ⌘Enter opens a new tab, Esc goes back to the list; ⌘⌫ does nothing while the sidebar has the keyboard. With VoiceOver: rows read "expanded"/"collapsed" and "Level 2" for tree folders; VO's expand action opens a branch.
218. **Follow.** View ▸ Show in Sidebar Tree in a deep folder of Documents: the tree opens under Documents down to it, the row shows and has the keyboard. Set `[sidebar] tree-follow = true` in settings.toml: going into folders opens the tree along; a hidden folder (with hidden items off) leaves it as is, silently.
219. **Nothing at rest.** Quit and start again: every branch is closed; with every branch closed, Activity Monitor shows the same memory as before this change (10d's build) within noise.

### 10f, view rules

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10f starts at 220 as its plan decided (other open branches take the numbers before it). Results go to `macos-test-results.md`.

220. **Tests first.** `cargo test -p gezik-core view_rules`, `cargo test -p gezik-config view_rules`, `cargo test -p gezik a_folder_s_own_view`, `cargo run -p gezik-core --release --example rules_bench`; paste the output (the last one within its three budgets).
221. **A path rule.** Add to settings.toml `[[view-rules]]` with `path = "{pictures}/**"`, `mode = "grid"`, `grid-size = "large"`. Open Pictures and a folder inside it: each shows in the large grid at once (no list with rows first); the View button's menu has a greyed `View rule 1 applies` above `Reset this folder`. Another folder stays as it was.
222. **Your own view wins.** In Pictures choose View ▸ List; go elsewhere and back: it stays a list; the View menu now says `Reset to rule 1`; choose it: the large grid is back and the menu says `View rule 1 applies` again.
223. **Content.** `mkdir /tmp/gezik-rules && cd /tmp/gezik-rules && mkdir six five four && touch six/{1..6}.jpg six/{1..4}.txt five/{1..5}.jpg five/{1..5}.txt four/{1..4}.jpg four/{1..6}.txt`; rule `content = "pictures >= 50%"`, `mode = "grid"`: `six` and `five` open as a grid, `four` as a list. `touch four/.a.jpg four/.b.jpg`: with Show Hidden Items off `four` stays a list. In `five`, `touch five/{6..12}.txt` from Terminal while it is open: it stays a grid (a reload keeps the rule).
224. **Kinds.** Rules `kind = "trash"` (`sort = "modified"`, `sort-dir = "desc"`), `kind = "removable"` (`mode = "grid"`) and `kind = "cloud"` (`group = "type"`): the Trash (open it only; move nothing in or out) sorts newest first; a USB stick under /Volumes opens as a grid; iCloud Drive groups by type. `kind = "network"` with an SMB share opened with ⌘K: note whether it applies (only if the drive list calls the share a network one).
225. **Mistakes and reloads.** Add a rule with `mode = "columns"`, one with `content = "pictures > 50%"` and one with `kind = "archive-root"`: the notice lists `view-rules[n]: …` for each and the other rules still apply. With a ruled folder on screen change its rule's `grid-size` in settings.toml and save: the folder changes without being opened again.
226. **Case and rest.** A rule `path = "~/downloads"` (small d) does not match Downloads on macOS (case counts outside Windows). Remove every rule, quit and start again: Activity Monitor shows the same memory as the 10c build within noise.

### 10b, two panes

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10b starts at 230 as its plan decided. Results go to `macos-test-results.md`.

230. **Tests first.** `cargo test -p gezik dual::`, `cargo test -p gezik-config shortcuts`, `cargo test -p gezik-config state`, `cargo test -p gezik-core drag`; paste the output (`sync_browsing_makes_the_same_step_or_says_why_not` is macOS's case rule: "docs" finds "Docs").
231. **Open and close.** ⌃⌘P (and View ▸ Two Panes, which gets its tick): a second pane opens on the right at the same folder, and is the active one (a 2 px accent line on its active tab). The sidebar runs the full height on the left, the preview (if open) on the right. ⌃⌘P again closes it; open it again: its tabs of last time. Quit and start: both panes come back as they were (`~/Library/Application Support/gezik/state.toml` has `right-tabs`, `dual = true`, `active-pane`, `[panes] split`). With `[session] restore = false` none of it comes back. Activity Monitor: memory with the second pane closed is within 0.1 MB of before it was opened.
232. **Narrow window.** Make the window narrower than 640 px and press ⌃⌘P: `The window is too narrow for two panes`, View ▸ Two Panes has no tick. With two panes and the preview open, narrow the window: the preview closes first (said once in the status bar), the panes stop at 240 px. Drag the splitter between the panes; double-click it: they share the room evenly; the share is kept after a restart.
233. **Active pane.** Click in the other pane, or press Tab with the file list focused: it becomes active (accent line moves, the inactive pane's selection fades). Tab in the address bar, filter or a name being edited stays with the field; with one pane Tab moves the focus as before. The status bar, the window title, the sidebar highlight, the preview, Space (Quick Look) and ⌘I follow the active pane. VoiceOver says `Left pane, …` / `Right pane, …`.
234. **One watcher.** Both panes on different folders; from Terminal `touch` a file in the inactive pane's folder: it does not show; Tab to that pane: it shows, selection and scroll stay.
235. **F5 / F6.** With two panes fn+F5 (and File ▸ Copy to the Other Pane): `Copy N items to <folder>?` with the target field; Return copies, the other pane shows the copy at once; ⌘Z takes it back. fn+F6 moves. Type `new/sub` in the field: both folders are made, ⌘Z removes them. `Don't Ask Again` writes `[panes] confirm = false`. Other pane on the Trash: F6 says `Use Delete to move items to the Trash`; both panes on the same folder: `Both panes show the same folder`. With one pane fn+F5 does nothing and F6 nothing (no refresh key on macOS: ⌘R).
236. **Move a tab.** Right-click a tab ▸ `Move to other pane` (also with one pane: it opens the second); its history (⌘[ / ⌘]), lock and selection go with it; a pane's last tab moved leaves a new tab at the start folder. Drag a tab onto the other pane's tab strip: the insertion line shows there; release: it lands at that slot.
237. **Drag between panes.** Drag files from one pane onto the other's list, a folder row, a tab and an address-bar part: same volume moves, another volume copies, ⌥ copies, ⌘ moves; the pane dropped on becomes active. Drag from Finder onto the right pane: into its folder.
238. **Sync browsing.** View ▸ Sync Browsing (ticked; only with two panes, else `Sync browsing needs two panes`): a link icon between the address bars and a thin accent line under both tab strips. Both panes on folders with a sub-folder of the same name: double-click it (or Return) in one pane: the other goes in too; ⌘↑: both go up. A name the other side lacks: `Sync browsing off: no folder "x" in <folder>` and the tick goes. Name differing only in case ("Docs" vs "docs"): it is found (macOS ignores case). ⌘[ (Back), the address bar, the sidebar, another tab: `Sync browsing off: the panes went apart`. Quit with it on and start: it is on again (`sync = true` in state.toml); closing the second pane turns it off.
239. **Swap panes.** ⌃⌘U (and View ▸ Swap Panes): the panes trade their tabs; the tab you were on is now in the other pane and still active and focused; locks, history and selection kept. With sync browsing on it stays on. With one pane: `There is no other pane`.
240. **Same folder.** View ▸ Same Folder in the Other Pane: the other pane's tab goes to the active pane's folder (Back there returns). From search results: to the searched folder.
241. **Tab sets.** With two panes, Save tabs as… (tab menu): `settings.toml` gets `right = [...]` in that `[[tab-sets]]`. Close the second pane, open the set: the second pane opens with those tabs, the left gets its own. A set without `right` opens in the active pane as before.
242. **Second launch.** With two panes and the right one active, `open -a` / `gezik <folder>` from Terminal: the folder opens in the right pane (an existing tab on it in that pane is reused).

### 10e, Miller columns

Numbers stay as they are when branches merge (see 10b). 10e starts at 250 as its plan decided. Compare with Finder's column view (⌘3 there) side by side. Results go to `macos-test-results.md`.

250. **Tests first.** `cargo test -p gezik-core columns`, `cargo test -p gezik-core drag`, `cargo test -p gezik view::columns`, `cargo test -p gezik-config state`; paste the output.
251. **Mode and keys.** ⌃⌘3 in a folder (and View ▸ as Columns in the menu bar, whose item shows ⌃⌘3): one column with the folder; ⇧⌘3 still takes a screenshot, ⌘3 still goes to tab 3. ⌘1 / ⌘2 (list, grid) leave the columns. `~/Library/Application Support/gezik/views.toml` says `mode = "columns"` for that folder.
252. **Arrows.** ↑↓ / Home / End / fn+↑↓ move in the focused column and the column to the right follows at once (no slide). → goes into the selected folder (its first item selected), ← goes back (the folder stays selected on the left, deeper columns close); ← in the first column does nothing. Return on a file opens it; on a folder it goes into its column.
253. **Location and history.** The address bar, window title and status bar say the focused column's folder; moving between columns adds no ⌘[ step (⌘[ after several → leaves the columns' folder at once). The address bar, the sidebar, ⌘[ / ⌘] start the columns over at that folder; ⌘↑ from the first column makes the parent the first column with the old one selected.
254. **Mouse.** Click a row in another column: that column takes the focus with the row selected; double-click a file there opens it. Drag a column's edge: every column takes the width (120–600); after a restart it is back (`state.toml` `[panes] column-width`). Two-finger swipe sideways scrolls the columns.
255. **Preview column.** Select a picture, a text file and a PDF: the last column previews it (as Finder's); a folder has none; with the window's preview open (View ▸ Preview) there is no preview column.
256. **Drag and drop.** Drag a file from a column that is not the focused one onto a folder row of another column, onto another column's empty space (its folder), onto the other pane and onto the sidebar: same volume moves, another copies, ⌥ copies; the target row lights up while dragged over; ⌘Z undoes; the column dropped into shows the file at once. Drag from Finder onto a column's empty space: into that column's folder.
257. **Menus.** Right-click (two-finger click) a row in another column: that column takes the focus, the row is selected and its menu opens (Open, Copy, Move to Trash…; act on it: Rename, ⌘C / ⌘V, ⌘⌫ work there). Right-click a column's empty space: the folder's menu (New Folder lands in that column). With a mouse, middle-click a folder row in another column: it opens in a new tab.
258. **Falls back to the list.** In search results (⌘F), the Trash (open only), a flat view and the computer root, ⌃⌘3 shows the list and the status bar says `Columns show folders only: the list is shown here`; back in a folder the columns return. Grouping set in a folder does not show in columns and comes back with ⌘1.
259. **Two panes, memory.** ⌃⌘P with columns in both panes: only the active pane has a preview column; sync browsing follows → and ←. Open a folder 30 levels deep (`mkdir -p /tmp/gezik-deep/$(seq -s/ 1 30)`) in columns and go → to the bottom: Activity Monitor's memory grows only a little (write the number at the start and at the bottom; spec: the listings shown + 0.5 MB); go back ← to the top: it falls back. With columns never chosen the memory is as the 10b build's.

### 10g, windows

Numbers stay as they are when branches merge (see 10b). 10g starts at 260 as its plan decided. Some items answer questions the code could not (spec 10 §9.2): write what you see even when it is not a failure. Results go to `macos-test-results.md`.

260. **Tests first.** `cargo test -p gezik windows::`, `cargo test -p gezik tab_drag`, `cargo test -p gezik-config state`, `cargo test -p gezik-platform instance`; paste the output.
261. **⌘N.** A second window in the same process (Activity Monitor: one `gezik`), at the folder shown, 24 pt down and right, same size; a theme change in `settings.toml` reaches both windows.
262. **Menu bar follows the window.** With two windows, click one, then the other: View ▸ Two Panes / Sync Browsing ticks and Window ▸ Lock/Unlock Tab are each window's own; ⌘C/⌘V, ⌘1/⌘2/⌃⌘3, ⌃⌘P act on the window in front. Write whether the menu bar really changes with the key window (also after ⌘` between the windows and after clicking the Dock icon). Window menu: `Move Tab to New Window`, then the open windows listed once with their titles (write whether macOS adds a list of its own as well: then there are two); choosing one brings it forward; a title follows when its window goes to another folder.
263. **Close.** Close the second window (red button, or ⌘W on its last tab): the first stays, Activity Monitor's memory falls back to within 0.3 MB of before it was opened (write both numbers). Close the last: Gezik quits (with the menu bar icon on: it stays in the menu bar, 9b9's rule). A copy running when a window closes keeps running in the other's panel; a conflict list open in the closed window opens in the other.
264. **Move a tab by menu.** Tab ▸ Move to new window (and Window ▸ Move Tab to New Window): history (⌘[), lock and selection go with it; a window's only tab: nothing happens. With two windows, Tab ▸ Move to window ▸ <title>: it lands in that window's active pane, which comes forward; a window's only tab moved this way: it goes and its window closes.
265. **Tear a tab off.** Drag a tab out of the window onto the desktop: a new window opens there with it, the tab under the pointer (no ghost follows the pointer outside the window: known). Drag a tab onto the other window's tab strip: the insertion line shows there; release: it lands at that slot and that window comes forward. Onto the other window's list: it goes last. A window's only tab onto the other window: it goes there and its window closes; onto the desktop: nothing. With two windows overlapping, release over the overlap: it goes to the one used last. Back inside its own window: reordering as before. Esc mid-drag: nothing changes. Write whether the release outside the window arrives at all (if the tab only snaps back, tearing off does not work on macOS).
266. **Files between windows.** Drag files from one window to the other's list, a folder row and a tab: same volume moves, another copies, ⌥ copies; ⌘Z undoes in either window. Drag from Finder onto the second window: into its folder; then onto the first: into its folder (each window keeps its own drop target). Write whether the drag between two Gezik windows goes through macOS's drag (NSDraggingSession; the cursor badge changes) and whether the drop lands only once.
267. **Second launch.** `open -a` / `gezik <folder>` from Terminal with two windows: the folder opens in the window used last, which comes forward. `gezik --new-window <folder>`: a new window in the running Gezik (one process). With `[system] single-instance = false`: a second process, the first's windows untouched.
268. **Session.** Three windows (one with two panes), quit (⌘Q) and start: all three come back with their tabs, places and sizes, the one used last in front (`state.toml` has `[[session.window]]`). With `[session] restore = false`: one window.
269. **Retina memory.** On the Retina screen, note Activity Monitor's memory with one window, then with a second full-size window, then after closing it (spec: framebuffer ≈ width × height × 4 × 4 + 2.5 MB).
270. **Focus order.** Three windows: click them in the order 1, 3, 2, then `open -a` / `gezik <folder>`: it opens in 2. Hide Gezik (⌘H) and show it: the same window is in front. Quick Look (Space) in one window, then Space in the other: the first one's closes, one Quick Look at a time. Scrolling with the trackpad over the window behind scrolls that window, not the one in front.

### 11a, checksums

Numbers stay as they are when branches merge. 11a uses 271–276 (the other step 11 parts come later). Results go to `macos-test-results.md`.

271. **Tests first.** `cargo test -p gezik-ops hash`, `cargo test -p gezik-core checksum_file`, `cargo test -p gezik-platform fs::`; paste the output.
272. **Tools menu.** The menu bar's `Tools` (between Go and Commands) has `Calculate Checksums…` and `Verify Checksums` after Compare; right-click a file: `Tools ▸ Calculate checksums…` (and `Verify checksums` on a `.sha256` file). The row menu still shows Open With ▸, Copy path as ▸, Commands ▸ (if any) and Quick Actions ▸: write if one is missing.
273. **Compute.** Select two files and a folder, Tools ▸ Calculate Checksums…: one row per file (folders' files with `sub/name`), SHA-256 ticked by default; tick MD5: only MD5 is read (fast), the SHA-256 values stay. The operations panel shows the job with progress; pause and resume it there. Esc closes the layer and the job stops. Activity Monitor: the memory falls back to within 0.3 MB of before it was opened.
274. **shasum both ways.** Save… on a folder's files writes `<folder>.sha256`; in Terminal `cd` there and `shasum -a 256 -c <folder>.sha256`: every line `OK`. Then `shasum -a 256 * > mine.sha256` in a folder and Verify Checksums on `mine.sha256` in Gezik: all OK; change one file: `FAILED`; delete one: `MISSING`; `Failed and missing only` shows just those. Export CSV…: the file opens in Numbers with the names right.
275. **Expected value.** Copy a SHA-256 from a download page (⌘C), select the downloaded file, Calculate Checksums…: Expected is filled, the value is green with a tick and the line says `Matches the expected SHA-256`; change one digit of Expected: red, `Does not match`.
276. **iCloud and F_NOCACHE.** In iCloud Drive with a file that is not downloaded (the cloud icon), Calculate Checksums… on its folder: Gezik asks once `N files are online-only. Download them…`; Skip: those rows say `Online only` and Finder still shows them as not downloaded. Write whether a large file (≥ 1 GB) hashes at about the disk's read speed (time it once).
### 11c, file compare

Numbers stay as they are when branches merge. 11c starts at 281 as its plan decided (11a and 11h take the numbers before and after).

281. **Tests first.** `cargo test -p gezik-core diff::`, `cargo test -p gezik diff::`, `cargo test -p gezik-platform terminal::`; paste the output.
282. **⌃⌘C and the Tools menu.** Make two text files (`printf 'a\nb\nc\n' > /tmp/l.txt; printf 'a\r\nB x\r\nc\r\nd\r\n' > /tmp/r.txt`), select both, press ⌃⌘C: the compare opens over the window, `b` ↔ `B x` tinted with the changed part marked, `d` green on the right, `UTF-8 · LF` ↔ `UTF-8 · CRLF` marked in the header, `2 changes`. Menu bar ▸ Tools ▸ Compare does the same; right-click ▸ Tools ▸ Compare too (it is not there with one file or a file and a folder). With two panes (⌃⌘P), one file selected in each: left pane's on the left. Write whether ⌃⌘C reaches Gezik at all (no system or app shortcut takes it).
283. **Keys in the compare.** ⌥↓ / ⌥↑ go to the next / previous change (the row's ring moves, the list scrolls to it), ↑↓, PgUp/PgDn, Home/End move the ring, ⌘C copies `< b` and `> B x`, ⌘R and F5 read the files again (change one in TextEdit first), Esc closes and the list has the keyboard again. Ignore case and Ignore whitespace re-line the files and are still on after a restart. VoiceOver reads the rows ("Changed: b to B x").
284. **Binary and FileMerge.** Compare two copies of an app's binary (`cp /bin/ls /tmp/a; cp /bin/ls /tmp/b`): a question says `The files are identical`; change a byte (`printf 'x' | dd of=/tmp/b bs=1 seek=100 conv=notrunc`): `The files differ from byte 101 (of …)`. Then set `[tools] diff = "opendiff"` in settings.toml and compare the two text files: FileMerge opens (Gezik does not wait); an app path (`diff = "/Applications/Beyond Compare.app"`, or any compare `.app` you have) opens through `open -n -a`; `diff = "nope"`: the status bar says `The compare program "nope" was not found; using Gezik's own` and Gezik's compare opens.
285. **Memory.** Activity Monitor's memory before opening a compare of two 1 MB text files (`seq 1 150000 > /tmp/l1; seq 1 150000 | sed 's/7$/x/' > /tmp/r1`), with it open, and after Esc: the last within 0.3 MB of the first (write the three numbers).

### 11e, folder sync

Numbers stay as they are when branches merge (see 10b). 11e uses 332–339 as its plan decided. Results go to `macos-test-results.md`.

332. **Tests first.** `cargo test -p gezik-core compare::`, `cargo test -p gezik-ops sync`, `cargo test -p gezik-config sync_profiles`, `cargo test -p gezik sync::`; paste the output.
333. **Modes and arrows.** The 326 folders (`/tmp/L`, `/tmp/R`) plus `echo z > /tmp/R/zonly`. Compare: Mode reads `Mode: Two-way`; `only` shows `→`, `new` `←`, `sub/r` `←`, `zonly` `←`. Click Mode: Left to right: `new`, `r` and `zonly` go empty; Mirror: `new` `→`, `r` and `zonly` `×→`. The summary says `N items to synchronize (Ctrl+Enter)`; quit and start: the mode is remembered.
334. **Changing rows.** Select `only`, ⌘←: `←×` with a `*` after its state; Space: empty; ⇧Space: back to the mode's `→`. ⌫ on `zonly` marks `×→`. Select the `sub/` row and press ⌘←: every row under it turns. Direction turns the selected rows → ← empty. Compare Again (⌘R) after a change: `Your direction choices were reset`.
335. **Synchronize, Mirror.** Mirror, ⌘↩: the question lists `Copy N files (… B) to /tmp/R, replacing 1` and `Move 2 items on /tmp/R to the Trash`; Synchronize. One job in the panel; the rows say `Synchronized` with `=`; `/tmp/R` matches `/tmp/L`; `zonly` and `r` are in the Trash. ⌘Z once: everything is as before (the replaced `new` back with its old text, the trashed ones back, the copies gone).
336. **Changed since the compare.** Compare, then in Terminal `echo more >> /tmp/L/only` before Synchronize: the job ends with `1 item skipped`; Details says `Changed after the compare; left alone`, the row `Not done: …`, and nothing went to the Trash in that run.
337. **External disk.** Compare a folder with a copy on an exFAT USB stick (it has a `.Trashes`): Mirror deletions there go to the stick's trash and come back with ⌘Z. On an SMB share (no trash): the question says `… has no trash: N deletions there are left out` and, for files to replace, `… would be replaced without a way back` with Replace them anyway; unticked, those files stay as they were.
338. **Profiles.** Save Profile… `Photos` (Mode Mirror, Content on, a filter): settings.toml has `[[sync-profiles]]` with `name = "Photos"`, `mode = "mirror"`, `content = true` and the filter. Close the compare; ⌘⇧P `Sync: Photos`: the compare opens with those folders, mode and filter, reads, and copies nothing. Save Profile… again with `Photos`, Delete: the profile is gone from settings.toml and the palette.
339. **Two-way and memory.** Two-way with a file deleted on the right since the last sync: it shows `→` and is copied back (stateless, as the README says). Activity Monitor before a compare + sync of ≈ 10,000 files, and after Close: within 0.3 MB (write both numbers).

### 11h, list export

Numbers stay as they are when branches merge (see 10b). 11h starts at 291 as its plan decided. Results go to `macos-test-results.md`.

291. **Tools menu and the question.** `cargo test -p gezik-core export::` and `cargo test -p gezik export::` first; paste. In a folder with a few files (one named `a,b "c" <d>.txt`), the menu bar's Tools ▸ Export List… and a right-click on the background and on a row (Tools ▸ Export list…): the question shows `<folder>/<folder name>.csv`, the line under it says `CSV into <folder>`; type `.doc` instead: the line turns red (`End the name with .txt, .csv or .html`) and Save / Copy do nothing until the name is fixed (Esc still closes). Tab moves field → boxes → buttons; Space ticks a box (focus ring shown). With two files selected the box `Only the 2 items selected` is ticked. Write whether a row's menu shows all its submenus (Open With, Copy path as, Tools, Quick Actions).
292. **Files.** Save as `.csv`, `.txt` (once with `Full paths in text`) and `.html`: each file appears selected; ⌘Z removes it. Save the CSV again under the same name: the conflict list asks; Replace, then ⌘Z: the first one is back. Open the CSV in Numbers (and `cat` it in Terminal): `,` separators, no BOM (`head -c 3 file.csv | xxd`), LF line ends, the odd name in one quoted field, a file named `=1+1.txt` written as `'=1+1.txt`, sizes as plain bytes, dates like `2024-05-01T10:00:12`. Open the HTML in Safari in light and dark mode: readable, the name shows as typed, no script.
293. **Search results and the clipboard.** ⇧⌘F a search (`*.txt` under your home folder): Tools ▸ Export List… defaults to `Search results.csv` in the searched folder, the CSV has a Folder column and full paths. Copy instead of Save: the status bar says `Copied N items as CSV`; paste into TextEdit: the same text. In the Trash Export List says it is not available; in the computer root (go up from `/`) it says `Export List works on a folder or search results`.

### 11g, size map

Numbers stay as they are when branches merge (see 10b). 11g starts at 297 as its plan decided. Results go to `macos-test-results.md`.

297. **Tests first.** `cargo test -p gezik-core treemap`, `cargo test -p gezik-search size`, `cargo test -p gezik size_map`; paste the output.
298. **Open.** Tools ▸ Size Map in the menu bar with a folder selected (and the ⌘⇧P palette's `Size Map`, a folder row's right-click `Tools ▸ Size Map`, a folder's empty space `Tools ▸ Size Map`): a layer with nested boxes; while it scans the boxes grow (at most twice a second) and the status line says `Scanning…`; at the end it says `<size> · <n> files · <n> folders`. Esc while it scans: `Stopped`; Esc again closes it.
299. **Look on Retina.** Folders are tones of the accent colour, pictures green, video red, sound purple, documents blue, archives orange, the rest and `N small items` grey; box edges are sharp on the Retina screen (write if they look blurred or doubled); labels (`name  size`) only in boxes big enough, readable in light and dark. Resize the window: the map lays out again at once.
300. **Zoom and keys.** Click a folder box: the map zooms into it (no new scan: the status line never says `Scanning…`), the path at the top follows; Up, ⌫ and ⌥↑ go back out (not above the folder it opened on). Arrow keys move a frame between boxes, the status line names the framed one (`path · size · n files · 12% of <folder>`); Return on a folder: the pane goes there and the map closes; on a file: the pane shows its folder with it selected. Double-click a folder box: the pane goes to that folder. ⌘R / F5 scans again.
301. **Menu.** Right-click a box: Open, Show in Pane, Move to Trash, Copy Path. Move to Trash: the box goes at once, the file is in the Trash, ⌘Z brings it back (the map does not: F5). Copy Path: the path is on the clipboard.
302. **Sizes kept.** After a map of a folder, close it and look at the list with folder sizes on: the folders under it show their sizes without `Calculating`. Map `/` (or the startup disk in the computer root): it stays on the startup volume (no `/System/Volumes/Data` twice, no other disks), and Activity Monitor's memory goes back to within 0.3 MB of before after closing the map (write both numbers).
### 11i, filtered copy, split and join

Numbers stay as they are when branches merge. 11i starts at 303 as its plan decided. Results go to `macos-test-results.md`.

303. **Tests first.** `cargo test -p gezik-core file_filter`, `cargo test -p gezik-core ops::`, `cargo test -p gezik-ops pick`, `cargo test -p gezik-ops move_`, `cargo test -p gezik-batch --test split`, `cargo test -p gezik copy_to`; paste the output.
304. **Copy To… in one pane.** Select files and a folder, palette (⌘⇧P) `Copy To…`: the field is empty, `Type a folder` shows under it; type `~/Desktop/gezik-11i` (missing): `Into …` shows; ⌘↩/Enter copies and makes the folder; ⌘Z takes it all away. Right-click a row: `Copy to…`, `Move to…` are there with Compress and Convert; `Split file…` only for one file, `Join files` only for a `.001` or `.crc`.
305. **Name mask.** Copy To… a few `.txt` files with the field ending in `/*.bak`: the line says `Names as *.bak`, the copies are `name.bak`; folders inside keep their names. Two files that become one name (`a.txt`, `a.log`): the conflict list asks about the second.
306. **Filter.** Copy To… a folder tree with `Filter…`: `*.jpg | thumbs/` copies only the `.jpg` files outside `thumbs` folders, and no empty folders appear. A typing mistake (`a | b | c`) shows red under the field. `*.jpg>*.cr2` with `IMG_1.jpg` selected copies `IMG_1.CR2` too (any case of the extension); write whether a `img_1.cr2` (other case of the name) comes along on APFS (it should: case-insensitive volume).
307. **Filtered move.** Move To… a folder with `Filter…` `*.jpg`: only the `.jpg` files move, the rest stays, a source folder left empty goes; ⌘Z puts everything back. F6 with two panes offers the same `Filter…` button and `Don't Ask Again`; from the Trash, F6 offers no filter.
308. **Split.** Select a file of a few hundred MB, `Split File…`: the size field says `100 MB` and the line `N parts`; Split writes `name.001`… and `name.crc` (open it in TextEdit: `filename=`, `size=`, `crc32=`). On an exFAT/FAT32 USB stick as the other pane with a file over 4 GB, the field starts at `4095 MB`. Cancel a big split in the job panel: nothing is left in the target folder.
309. **Join.** Select `name.002`, `Join Files`: the file comes back; compare it (`cmp` or `shasum`) with the original. Change one byte in a part (`printf 'x' | dd of=name.002 bs=1 seek=5 conv=notrunc`): Join fails with `does not match name.crc`, and an existing `name` there is not touched. Delete a middle part: `Part name.00N is missing`, nothing written.
310. **Total Commander parts.** If you have parts and a `.crc` made by Total Commander (or `split -b 100m -d -a 3 --numeric-suffixes=1`), Join joins them; write which.

### 11f, duplicate files

11f uses 317–323.

317. **Tests first.** `cargo test -p gezik-search duplicates`, `cargo test -p gezik-core duplicates`; paste the output (the link test makes real symbolic links here).
318. **Find Duplicates.** Make a folder with a photo and a copy of it in a subfolder, a file with a hard link (`ln a.txt b.txt`), two empty files and a symbolic link to the photo (`ln -s`). Tools ▸ Find Duplicates on it: the search bar says `Find duplicates in <folder>`; Enter. One group, `2 files × … could be freed`: the photo and its copy (the symbolic link is not read); the hard link is no group and the empty files are not listed; the status bar says `1 group · 2 files · … could be freed · 1 hard link counted once`.
319. **Select helpers.** With a few groups: the search bar's ▾ menu and a row's right-click list `Select all but the newest in each group` … `Keep this one, select its copies`; each selects as it says and never every copy of a group. `Select all in this folder` on a folder holding every copy of a group: one stays unselected and the status bar says so.
320. **Delete and undo.** Delete on the selection: the Trash gets them, the rows go after the job, a group left with one file goes. ⌘Z brings the files back in Finder; the rows do not come back (search again). Select every copy of a group and Delete: Gezik asks `This deletes every copy of 1 file` first.
321. **No trash.** On a network share (SMB, no Trash there) find duplicates and Delete: nothing is deleted, no "delete permanently?" question; the status bar says the duplicates were not deleted and why.
322. **iCloud and APFS clones.** In iCloud Drive with files not downloaded: they are not read (no download starts) and the status bar counts them as online-only. Duplicate a large file in Finder (⌘D makes an APFS clone): it is listed as a duplicate (spec §19: deleting a clone may free little).
323. **Stop.** On a large folder (home), Esc or Stop while it says `comparing … of same size`: it stops within a second, the status bar says `Stopped`, Activity Monitor's memory falls back.

### 11b, copy verification, speed limit, queue

Numbers stay as they are when branches merge. 11b uses 311–316. Results go to `macos-test-results.md`.

311. **Tests first.** `cargo test -p gezik-ops verif`, `cargo test -p gezik-ops limited`, `cargo test -p gezik-ops waiting_jobs`, `cargo test -p gezik-core ops::rate`, `cargo test -p gezik-platform fs::`; paste the output.
312. **Clones are not verified.** In `settings.toml` set `[files] verify = true`. Copy a 2 GB file to another folder on the same APFS disk (⌘C, ⌘V): it is done at once and the panel never says `Verifying…`. Then copy it to a USB stick or another disk (not APFS, or another APFS volume): the row says `Verifying…` after the copy and the job takes about twice as long. Write both times.
313. **F_NOCACHE.** The same copy to the stick with `verify = true`: while it verifies, Activity Monitor ▸ Disk shows reads from the stick (not only from the Mac's disk). Write whether it does: macOS may give the read from the cache (a known limit).
314. **Speed limit.** `[files] max-speed = "20 MB/s"`, copy a 2 GB file to the stick: the row's speed stays near 20 MB/s; time it (≈ 100 s ±5 %). In the row's ⋯ menu choose Speed limit ▸ No limit: it speeds up at once; ▸ 5 MB/s: it slows at once. Pause and Cancel answer within a moment while limited.
315. **Queue.** Start three large copies to the same stick one after the other: the first runs, the others say `Waiting for the drive`. The third's ⋯ ▸ Move up: it goes above the second (the rows swap) and runs right after the first; Move up on a waiting job right after the running one is greyed; the running job's ⋯ has no Move items.
316. **F5 and the menu.** With two panes (F3) and `[panes] confirm = true`, F5: the question has `Verify copies`, ticked as `[files] verify` says; untick it and copy: no `Verifying…`. Tools ▸ Copy To… and Move To… ask the same question with the same box. During a long copy ⋯ ▸ Verify copies turns it on for the files not copied yet. A move to another disk with verification on keeps each source file until its copy checked (watch a big file: the source goes only after `Verifying…`).

### 11d, folder compare

Numbers stay as they are when branches merge (see 10b). 11d uses 324–331 as its plan decided. Results go to `macos-test-results.md`.

324. **Tests first.** `cargo test -p gezik-core compare::`, `cargo test -p gezik-search tree_scan`, `cargo test -p gezik compare::`; paste the output.
325. **Open three ways.** Select two folders, ⌃⌘C: the compare opens with `Compare  <left>  ⇄  <right>` and `Reading…`, then rows. Close (Esc); with two panes on two folders and nothing selected, the menu bar's Tools ▸ Compare: the panes' folders, the left pane's on the left. Right-click a folder's empty space with two panes: `Tools ▸ Compare` and `Select differences` are there.
326. **States and buttons.** Two folders made in Terminal (`mkdir -p /tmp/L/sub /tmp/R/sub; echo a > /tmp/L/same; cp -p /tmp/L/same /tmp/R/; echo b > /tmp/L/new; sleep 3; echo b > /tmp/R/new; echo c > /tmp/L/only; echo d > /tmp/R/sub/r`): `new` shows `←` Right newer, `only` → Only left, `- sub/` (open; `+` when closed) with `r` ← Only right, `same` hidden until `=` is ticked. The buttons' numbers match the rows; untick `2×`: only one-sided rows stay. Quit and start: the buttons are as you left them (`[compare]` in state.toml).
327. **Content and the hour.** Tick Content: `new` turns `=` with `Same content`; `touch -t 202401011200 /tmp/L/same; touch -t 202401011300 /tmp/R/same`: `same` is ← until Ignore 1 h is ticked, then `=`. Ignore dates: only sizes (and content) count.
328. **APFS and exFAT names.** On an exFAT USB stick copy a folder with `café.txt` and `Résumé/` from the Mac (Finder), then compare the Mac folder with the stick's: the names meet (no Only left / Only right pairs for them); `Docs` vs `docs` also meet (case ignored).
329. **Filter, links, stop.** Filter `*.txt | sub/` and Return: `sub` is gone and only `.txt` files show. `ln -s /tmp/L /tmp/R/loop`: the summary says `1 left out (links)` and nothing under `loop`. Compare your home folder with an external disk's copy: Esc while `Reading…` says `Stopped…`; a second Esc closes.
330. **Rows and panes.** ↓↓, → / ← open and close `sub`, a second click on a folder row does too, Return on `new` opens the file compare over it (⌘R there, Esc back to the folder compare with the keyboard), ⌘C copies the selected rows' paths. Show Right: two panes open, the right goes to `/tmp/R` with `new` selected, the compare hides and the status bar says `Compare: L (2 ≠)`; click it: the compare is back as it was; Hide then ✕ on the chip: gone. Copy a file in a pane while it is hidden, then click the chip: `Files may have changed: Compare Again (F5)`.
331. **Select Differences and memory.** Two panes on `/tmp/L` and `/tmp/R`, Tools ▸ Select Differences: the left pane selects `only`, the right `new` (what the other side lacks or has older), the status bar `Selected N on the left, M on the right`. Activity Monitor's memory before comparing a big folder (≈ 100,000 files) with its copy, with it open, and after Close: the last within 0.3 MB of the first (write the three numbers).

### 12a, tag storage (no interface yet)

Numbers stay as they are when branches merge (see 10b). 12a uses 340–343 as the step 12 spec decided. Gezik shows no tags yet: these checks run the tests and look in Finder. Results go to `macos-test-results.md`.

340. **Tests first.** `cargo test -p gezik-core meta`, `cargo test -p gezik-platform meta`, `cargo test -p gezik-ops meta`, `cargo test -p gezik-ops tags`; paste the output. Then `cargo test -p gezik-platform meta::tests::finder_probe -- --ignored`.
341. **Finder sees Gezik's tags.** After the probe of 340, `~/gezik-tag-probe.txt` in Finder: a red dot, and Get Info ▸ Tags lists `Gezik Probe` and `Red`; the sidebar's Red tag (or Finder's search `tag:Red`) finds it. `xattr -px com.apple.FinderInfo ~/gezik-tag-probe.txt`: its tenth byte (offset 9) is `0C`, red; `mdls -name kMDItemStarRating -name kMDItemFinderComment ~/gezik-tag-probe.txt` shows `3` and `Written by Gezik` (the comment may not show in Get Info; spec §17). Its Modified date did not change after the probe wrote the tags (`ls -l` before and after a second run).
342. **Gezik reads Finder's tags.** In Finder, add the Green tag to `~/gezik-tag-probe.txt` (right-click ▸ Tags). Then `cargo test -p gezik-platform meta::tests::finder_wrote_green -- --ignored` passes. Write down what `xattr -px com.apple.metadata:_kMDItemUserTags ~/gezik-tag-probe.txt` prints.
343. **exFAT stick.** With an exFAT (or FAT) USB stick mounted: `GEZIK_TEST_TAG_DIR=/Volumes/<STICK> cargo test -p gezik-platform meta::tests::a_copy_to_an_exfat_disk_keeps_its_tags -- --ignored` passes (macOS keeps the attributes in `._` files there; the test removes its copy).

### 12b, tagging and tag dots

Fresh settings (`GEZIK_CONFIG_DIR=/tmp/gz12b`): `[tags] dots = "auto"` shows dots on macOS from the start.

344. **The picker.** Select three files in `/tmp/tags`, ⌃⌘L (and File ▸ Tags ▸ Tags…): `Tags for 3 items` over the window, the seven colours with ⌃1…⌃7 at the right. Return on Red: `✓`; Return again: none; give one file Orange in Finder first: Orange shows `–`, Return makes it `✓`. Type `Proj`, Return on `Create "Proj"`: it is added (settings.toml's `[tags] list` gains it, comments kept). Esc: one job; ⌘Z takes all of it back. VoiceOver reads a row as `Red, on all items`.
345. **⌃1…⌃7, ⌃0.** With Mission Control's "Switch to Desktop" keys off: ⌃1 on the selection puts Red on all (`Red added to 3 items`), ⌃1 again takes it off; ⌃0 clears every tag (a rating written by 340's probe stays: `mdls -name kMDItemStarRating`). Each is one ⌘Z.
346. **File ▸ Tags and the row menu.** File ▸ Tags lists the list's names with ⌃1…⌃7, Tags… (⌃⌘L) and Clear Tags (⌃0); a row's right-click has the same items at the end of `Tools ▸` under a greyed `Tags`, `• Red` on a row that has it. Drives and the trash have no Tags.
347. **Finder sees them.** Tags given in Gezik show in Finder's list and Get Info with the same colours; a tag given in Finder shows as a dot in Gezik after F5. Modified dates do not change.
348. **Dots.** List, grid and columns: up to three overlapping dots after the name (under it in the grid), `+2` past three, a ring for a tag without colour; on a selected row each dot has a white edge. View ▸ Tag Dots off: no dots and settings.toml says `dots = false`.

## Known gaps (not bugs)

These are known differences from Finder and ForkLift (from the ForkLift comparison, §3). Don't report them as failures. A note is welcome if one hurts more than expected.
- **Icons and thumbnails:**
  - Alias rows show Finder's "Document" type and the generic document icon, not "Alias" with the arrow badge (Gezik reads no extra file data while listing).
  - Pinned folders keep their own labels; only the sidebar's known folders, the address bar, tab titles and the rows under `~` and `/` use Finder's localized names.
- **Context menu:**
  - Open With ▸ lists app names without their icons.
  - Quick Actions ▸ lists only Quick Actions and Services installed as bundles in ~/Library/Services and /Library/Services; services that apps provide (e.g. Terminal's "New Terminal at Folder") and the system's Markup/Rotate/Create PDF are not listed, and one turned off in System Settings still shows.
- **Folder sizes (8b):** a change deep inside a subfolder made outside Gezik shows the old size for up to 5 minutes (⌘R or F5 works it out again). Search results and the flat view show no folder sizes.
- **Missing features:**
  - Finder tags are not read or shown.
  - Gezik's own list does not hide items marked Hidden in Get Info (it hides by a leading dot only); Finder does.
  - Get Info shows no icon in its General part.
  - No iCloud Drive status, download or evict. What happens when a file that is only in the cloud is opened has not been tried; note it if you try.
  - Search walks the disk itself: Spotlight is not used (no Spotlight index, no `kMDItem` queries, no Spotlight comments or contents of PDFs and Office files). Results and the flat view don't follow changes made in Finder until ⌘R or F5.
  - No Column (⌘3) or Gallery (⌘4) view, and no ⌘+/⌘- text size.
  - Gezik can't be the default file viewer.
- **App bundle and menus:**
  - No `.app`, signing or notarization. The app menu items read "About/Hide/Quit gezik".
  - The menu bar shows the default shortcuts, not ones changed in settings.toml.
  - ⌘, does nothing.
- **Names:**
  - A kept copy is `b (2).txt` (Finder: `b 2.txt`).
  - A new folder is "New folder" (Finder: "untitled folder").
  - The interface is in English.
- **Drag and drop:**
  - Spring-loaded folders work only on tabs.
- **Look:** the controls are drawn by Slint. There is no Liquid Glass sidebar, no system accent colour, and no macOS window tabs.
- **Tray (9b9):** the menu bar icon is the system's folder symbol (Gezik has no icon of its own yet). The global shortcut is by key position on a US keyboard. ⌘Q quits without the running-jobs question.

## When you are done

Commit the results (and any fixes) in English, with no `Co-Authored-By` or Claude lines, on a new branch `test/macos-run2` from master (fixes on `fix/macos-2`), push it and open a PR.
