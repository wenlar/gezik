# Set Dates on macOS: manual checks

Numbering: the 16k block of the 16h–16m complements (463–472). Results go to `../macos-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §5.

463. **The menu and the dialog.** Right-click a file ▸ Tools ▸ **Set Dates…**: a dialog over the window with Modified (on), Created, Accessed, the four sources (Date and time chosen) and the file's own Modified in the date and time fields. Esc closes it; nothing changes.
464. **A typed date.** Select three files and a folder, Set Dates…, tick Created too, type `2020-01-02` and `03:04:05`, Set: Finder's Get Info (and `stat -f '%SB %Sm' <file>`) shows 2 Jan 2020 03:04:05 created and modified for all four. A wrong date (`2020-02-30`) keeps the dialog open with a red line.
465. **One undo.** ⌘Z after 464: every date of all four is back as it was, in one step (Edit ▸ Undo said "Set dates of 4 items").
466. **Now.** Choose Now with only Accessed ticked: `stat -f '%Sa' <file>` is now, Modified is unchanged.
467. **Date taken.** Select some iPhone photos (HEIC and JPEG) and a text file: Date taken (EXIF), Modified and Created: the photos take the time they were taken (local time, as Photos shows it); the operations panel says one item was skipped ("No date taken").
468. **From file.** Click **From file…**, type a path (relative names are in the items' folder): the path shows under the buttons; Set gives the items that file's modified, created and accessed dates. A path that is not there says "No such file".
469. **Info window.** Get Info (⌘I) on one file: a click on the Created/Modified/Last opened lines opens Set Dates for that file; after Set the lines show the new dates. ⌘Z brings them back (Get Info again shows the old ones).
470. **Locked and others' files.** A file with Locked on (Get Info) and one owned by root: the job reports them as failed; the others are changed.
