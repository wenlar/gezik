# Step 16 on Linux: manual checks

Numbering goes on from `15-linux.md` (it ends at 285). Results go to `../linux-test-results.md`.

### 16a, inside archives

286. **Zip, tar.xz and deb as folders.** Double-click a `.zip`, a `.tar.xz` and a `.deb` (any from `/var/cache/apt/archives` or `apt download hello`): each opens as a folder (the deb shows `DEBIAN` and `usr`), sort and the filter (Ctrl+F) work inside. Drag a file from inside the zip onto a Nautilus window (GNOME, X11 and Wayland) and onto Dolphin (KDE): it is copied there. Ctrl+C inside the zip, Ctrl+V in another tab's folder: copied, Ctrl+Z removes it.
287. **Formats only 7-Zip reads.** A `.wim` or `.dmg` (or an `.msi`) with 7-Zip installed (`7zz` 25 or newer): double-click shows its contents. Without 7-Zip (rename it away, or `[tools] seven-zip` pointing nowhere): the box offering 7-Zip appears; after the download the archive opens.

### 16b, writing to archives

288. **Add to and delete from a tar.gz.** Make one with `tar czf t.tar.gz somefolder` and open it in Gezik. Drag a file from Nautilus (GNOME on Wayland) and from Dolphin (KDE) into its list; paste another with Ctrl+V; delete one inside (Delete, confirm). `tar tzvf t.tar.gz` lists exactly what Gezik shows, with the old entries' dates and permissions unchanged. Ctrl+Z three times: `sha256sum t.tar.gz` matches a copy taken before. No `.gezik-*` file or folder is left next to it.
