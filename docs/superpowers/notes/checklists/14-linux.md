# Step 14 on Linux: manual checks

Numbering goes on from `../linux-test.md` (14b ended at 268). Results go to `../linux-test-results.md`.

### 14c, themes from GitHub

269. **Get a theme, curl and wget.** As macOS 370 (`~/.config/gezik/themes/`, the browser for Open Source Page). Then with `curl` moved out of the `PATH` (`sudo mv /usr/bin/curl /usr/bin/curl.off`, put it back afterwards): the same download goes through `wget`; with neither, the red line says what to install.

### 14d, settings export and import

270. **Round trip.** As macOS 372–373 with `~/.config/gezik/` (a zip from Windows or the Mac imported on Linux, then Undo; the red row for a `[[commands]]` entry).

### 14e, portable mode

271. **Next to the AppImage.** Put the AppImage in a new folder (say `~/GezikPortable/`) with `touch ~/GezikPortable/gezik-portable`; note `ls -l --time-style=full-iso ~/.config/gezik`. Start the AppImage: `~/GezikPortable/gezik-data/` appears with the starter files; Settings ▸ Sync and Backup's first row says "Portable: settings are kept in …"; changes go there and `~/.config/gezik` is unchanged. With an installed Gezik running too, the two are separate windows. A download from Compress… (7-Zip) lands in `gezik-data/tools/linux-x86_64/` (or `linux-aarch64`).
272. **Read-only drive.** Copy that folder (with `gezik-data/`) to a USB stick, mount it read-only (`sudo mount -o remount,ro /media/$USER/<stick>`) and start the AppImage from it: Gezik opens with the portable settings, the status bar says "Portable data folder is read-only: changes are not saved", nothing crashes; a moved column or a new pin is not written (the stick is unchanged). Remount read-write afterwards.
