# Gezik on macOS: build and manual test

Run 1 (2026-10-06, `feat/batch-ops-5c`) covered items 1-22. Its fixes are on master: the `fix/macos` branch, and the shared UI fixes from `fix/shared-ui`. Sub-projects 5c (convert, user commands, ffmpeg), 5d (PDF, pdfium) and 6a (keyboard: filter, selection, tabs; items 47-51) have never run on a Mac.

This file is for a person, or Claude Code on the Mac, testing the state after 5d. Write the results into `docs/superpowers/notes/macos-test-results.md`, in a new section **"Run 2 (after 5d)"** below Run 1. Don't change Run 1.
- Give PASS, FAIL or NOT TESTED for every item.
- For a FAIL, give the steps, what you expected, what happened, and a screenshot path. Don't commit screenshots.
- Start the section with the date, the branch and short SHA, the machine, the macOS version and the keyboard layout.
- Add a summary table like Run 1's.

## Build

```sh
xcode-select --install            # C/C++ compiler (UnRAR is C++); skip if already installed
curl https://sh.rustup.rs -sSf | sh   # Rust; then open a new terminal
git clone git@github.com:wenlar/gezik.git && cd gezik   # or: git fetch in the existing clone
git checkout feat/batch-ops-5d    # until it is merged; after that: git checkout master && git pull
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

Report anything else that looks wrong: layout, fonts, Retina scaling, ⌘ shortcuts that don't work, and the wording of the boxes.

## Known gaps (not bugs)

These are known differences from Finder and ForkLift (from the ForkLift comparison, §3). Don't report them as failures. A note is welcome if one hurts more than expected.
- **Icons and thumbnails:**
  - No system icons: every item has Gezik's own icon.
  - Thumbnails come from Gezik's own decoders (5 formats), with no Quick Look thumbnails (PDF, video, PSD, Office).
- **Quick Look and context menu:**
  - Quick Look is Gezik's own window, with no QL plugins, video, PDF or Office preview.
  - The right-click menu is Gezik's own: no "Open With ▸" list, Share/AirDrop, Services, Quick Actions, "Show in Finder" or "Get Info".
- **Missing features:**
  - Finder tags are not read or shown.
  - No Get Info window.
  - No iCloud Drive status, download or evict. What happens when a file that is only in the cloud is opened has not been tried; note it if you try.
  - No search and no Spotlight.
  - No Column (⌘3) or Gallery (⌘4) view, and no ⌘+/⌘- text size.
  - No eject and no Connect to Server (⌘K).
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
  - Drags of file promises from Photos or Mail probably don't arrive. This is worth trying and noting.
- **Look:** the controls are drawn by Slint. There is no Liquid Glass sidebar, no system accent colour, and no macOS window tabs.

## When you are done

Commit the results (and any fixes) in English, with no `Co-Authored-By` or Claude lines, and push to the branch you tested (`feat/batch-ops-5d`, or a new `fix/macos-2` from master for fixes).
