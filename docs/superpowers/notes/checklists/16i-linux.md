# Hard links on Linux: manual checks

Numbers 326–335 belong to part 16i. Results go to `../linux-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §3.

326. **Make one.** Right-click a file (`a.txt`) ▸ **Create hard link**: `Link to a.txt` appears beside it and is selected. `ls -li` shows both names with the same inode number and a link count of 2; editing one shows the change in the other.
327. **Undo.** Ctrl+Z: `Link to a.txt` goes to the trash (Restore works), `a.txt` keeps its content, the link count is 1 again. Ctrl+Y brings the link back.
328. **Folders are refused.** Select a file and a folder ▸ **Create hard link**: one message "Hard links work only on files", nothing is made.
329. **Info window.** Get Info (Alt+Enter) on `a.txt` while the link exists: General ends with "Links: 2"; after the undo no Links line. A folder never shows one.
330. **Other file systems.** On a FAT32/exFAT USB stick the system's error is shown; on ext4/btrfs it works.
