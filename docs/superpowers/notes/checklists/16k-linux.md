# Set Dates on Linux: manual checks

Numbering: the 16k block of the 16h–16m complements (346–355). Results go to `../linux-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §5.

346. **The menu and the dialog.** Right-click a file ▸ Tools ▸ **Set Dates…**: Modified (on) and Accessed, no Created box (Linux cannot set it), the four sources, the file's own Modified in the date and time fields. Esc closes it.
347. **A typed date.** Select three files and a folder, type `2020-01-02` `03:04:05`, tick Accessed too, Set: `stat -c '%y %x' <file>` shows 2020-01-02 03:04:05 (local time) for both on all four.
348. **One undo.** Ctrl+Z after 347: all dates back in one step ("Set dates of 4 items").
349. **Date taken.** Phone photos (JPEG, HEIC) and a text file: Date taken (EXIF): the photos take their capture time; one item skipped ("No date taken") in the operations panel.
350. **From file.** From file… with another file's path: the items take its modified and accessed times.
351. **Info window.** Get Info (Alt+Enter) on one file: a click on its date lines opens Set Dates; after Set the lines show the new dates.
352. **Someone else's file.** A file owned by root (`/tmp` is fine: `sudo touch /tmp/r`): the job reports it as failed; the user's own files in the same job are changed.
