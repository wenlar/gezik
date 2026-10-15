# Hard links on macOS: manual checks

Numbers 443–452 belong to part 16i. Results go to `../macos-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §3.

443. **Make one.** Right-click a file (`a.txt`) ▸ **Create hard link**: `Link to a.txt` appears beside it and is selected. In Terminal `ls -li` shows both names with the same inode number and a link count of 2; editing one shows the change in the other.
444. **Undo.** ⌘Z: `Link to a.txt` goes to the Trash (Put Back works), `a.txt` keeps its content. Its link count stays 2 (`ls -l`, Get Info's Links): the link in the Trash is still a name of the same file; emptying the Trash makes it 1. ⌘⇧Z brings the link back.
445. **Folders are refused.** Select a file and a folder ▸ **Create hard link**: one message "Hard links work only on files", nothing is made (not even the file's link).
446. **Info window.** Get Info (⌘I) on `a.txt` while the link exists: General ends with "Links: 2"; after the undo it still says 2 while the link is in the Trash, and has no Links line once the Trash is emptied. A folder never shows one.
447. **Taken name.** Make a hard link of `a.txt` twice: the second is `Link to a (2).txt`.
448. **Other volumes.** On an external drive (APFS, HFS+ or exFAT) and on an SMB share: Create hard link works on the drive itself or says why it cannot (exFAT has no hard links: the system's message).
