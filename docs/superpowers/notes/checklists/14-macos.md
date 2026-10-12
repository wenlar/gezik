# Step 14 on macOS: manual checks

Numbering goes on from `../macos-test.md` (14b ended at 369). Results go to `../macos-test-results.md`.

### 14c, themes from GitHub

370. **Get a theme.** Settings ▸ Appearance (or the palette's Get Theme…): after the settings, Get Theme. Paste a public GitHub repository with `.toml` themes at its top or in `themes/` (search GitHub for the `gezik-theme` topic, or any repository with a Gezik theme) and press Return: the line at the bottom counts the download, then each theme is a row. Click a row's `Not installed` button and point at `Install`: every window previews the theme; moving away goes back. Choose `Install`: `~/Library/Application Support/gezik/themes/<name>.toml` appears with `# Installed from …` as its first line, and an `Installed` row with `Use Now`, `Check for Updates`, `Open Source Page`, `Delete Theme`. `Use Now` switches to it; `Check for Updates` lists the same theme as `Installed` (unchanged); `Open Source Page` opens the repository in Safari; `Delete Theme` moves the file to the Trash. Also paste a `…/blob/main/x.toml` address, and `http://github.com/a/b` (refused, red line).
371. **Proxy.** If a network with a proxy (System Settings ▸ Network ▸ Proxies) is at hand: the download of 370 goes through it. Otherwise write "no proxy to test".

### 14d, settings export and import

372. **A Windows zip on the Mac.** On Windows, Settings ▸ Sync and Backup ▸ Export Settings (Enter on the path) with a theme of your own, a pin under `{home}` and a template; copy the zip to the Mac. On the Mac, Import Settings: paste the zip's path, Return. The rows say what each kind of file does ("replaces", "new") and how many settings change; choose Import. The theme, the pin (it opens the Mac's home folder) and the template (New ▸) work; `~/Library/Application Support/gezik/backups/` has a `gezik-settings-<time>.zip`. Undo Import ▸ Undo: the settings, the theme and the template are back as before (the theme and template gone).
373. **The red row.** Add a `[[commands]]` entry (any `run`) to the Windows `settings.toml`, export again and preview the zip on the Mac: a red "These run programs on this computer" row names the command, the bottom line is red. Cancel: nothing changes. Restore Backup lists the backup of 372; choosing it shows what it would put back.
