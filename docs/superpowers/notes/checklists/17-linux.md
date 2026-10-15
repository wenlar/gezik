# Cloud sync (17) on Linux: manual checks

Numbers 376–385 (after `16m-linux.md`). Results go to `../linux-test-results.md`. Spec: `../../specs/2026-10-15-bulut-senkronu-design.md` §5.4, §7.

**Setup:** `curl` installed (the HTTP client Gezik uses on Linux); a second computer (the Windows PC or the Mac) with this branch's Gezik, both online. The server is the default `https://sync.wenlar.com`.

376. **Start Syncing.** Settings ▸ Sync and Backup ▸ Cloud Sync ("Off") ▸ Start Syncing…: the code row with Copy and the "Keep this code…" text; Copy works (paste it in a terminal). The row then says "Synced just now". `~/.config/gezik/` has `cloud/`, `cloud-base`, `cloud-sync.toml`, and `location.toml` says `shared = "{local}/cloud"`.
377. **The code file is private.** `stat -c %a ~/.config/gezik/cloud-sync.toml` prints `600`.
378. **Join.** On the other computer Join With a Code… with the code: its settings become this one's; the status says the previous ones were backed up. One wrong character: "This code has a typo".
379. **A change goes both ways.** Change the theme here and wait ~5 s; change the font size on the other computer and Sync Now there, then Sync Now here: both computers have both changes.
380. **The same key on both.** Different themes on both computers, Sync Now on each: the second one lists `settings.cloud-conflict-<date>.toml` with Review….
381. **Offline.** `nmcli networking off` (or unplug), Sync Now: "Not synced — offline"; start Gezik offline: no delay. Back online, Sync Now: synced.
382. **At close.** Change a setting and close the window within 5 s: the other computer's Sync Now brings it.
383. **Portable.** In portable mode (a `gezik-portable` file next to the AppImage), Start Syncing: `location.toml` in `gezik-data/` says `{local}/cloud`; move the folder elsewhere (another mount point) and start Gezik: the settings are still found.
384. **Palette.** Ctrl+Shift+P "Sync Settings Now" and "Show Sync Code" work; no default keys.
385. **Stop and Delete.** Stop Syncing: settings back in `~/.config/gezik/settings.toml` with this computer's `[system]`; `cloud/`, `cloud-base`, `cloud-sync.toml` gone. Delete from Server… on a syncing computer: the other one's next Sync Now says "Your cloud copy was removed after a year without use. Start Syncing again?" and offers Start Syncing Again.
