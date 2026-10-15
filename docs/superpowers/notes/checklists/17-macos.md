# Cloud sync (17) on macOS: manual checks

Numbers 493–502 (after `16m-macos.md`). Results go to `../macos-test-results.md`. Spec: `../../specs/2026-10-15-bulut-senkronu-design.md` §5.4, §7.

**Setup:** a second computer (the Windows PC or the Linux laptop) with this branch's Gezik, both online. The server is the default `https://sync.wenlar.com`.

493. **Start Syncing.** Settings (⌘,) ▸ Sync and Backup ▸ Cloud Sync says "Off" with Start Syncing… and Join With a Code…. Start Syncing…: a row shows the code (`XXXXX-XXXXX-XXXXX-XXXXX-XXXXX`) with Copy and the "Keep this code…" text; Copy puts it on the clipboard. The Cloud Sync row then says "Synced just now". `~/Library/Application Support/gezik/` has `cloud/` (settings.toml, themes, templates), `cloud-base`, `cloud-sync.toml`, and `location.toml` says `shared = "{local}/cloud"`.
494. **The code file is private.** `ls -l ~/Library/Application\ Support/gezik/cloud-sync.toml` shows `-rw-------`.
495. **Join from the other computer.** On the other computer: Join With a Code…, paste the code (try it lowercase, without dashes), Enter: its settings become the Mac's (theme, pins), the status says the previous ones were backed up, and Restore Backup lists a new backup. A code with one wrong character says "This code has a typo"; another valid code says "No settings found for this code".
496. **A change goes both ways.** On the Mac change the theme; wait ~5 s (a round runs by itself). On the other computer change the font size, then Sync Now: it has the Mac's theme. On the Mac Sync Now (or restart Gezik): it has the font size. Neither change is lost.
497. **The same key on both.** Change the theme to different values on both computers, Sync Now on the Mac, then on the other one: the other keeps its own theme and Sync and Backup lists `settings.cloud-conflict-<date>.toml` with Review…; Merge works as with a sync folder.
498. **Offline.** Turn Wi-Fi off, Sync Now: "Not synced — offline", Gezik works on. Quit and start again offline: no delay at start. Wi-Fi on, Sync Now: "Synced just now".
499. **At close.** Change a setting and quit at once (⌘Q, within 5 s): on the other computer Sync Now brings the change (quitting waited at most ~3 s).
500. **Palette.** ⌘⇧P "Sync Settings Now" says "Settings synced" in the status bar; "Show Sync Code" opens Sync and Backup with the code shown. Neither has a key by default.
501. **Not with a sync folder.** While cloud sync is on, Keep Settings in a Folder is greyed with "Stop Cloud Sync first"; after Stop Syncing, set a sync folder: Cloud Sync is greyed with "Stop keeping settings in a folder first".
502. **Stop and Delete.** Stop Syncing: `cloud/`, `cloud-base`, `cloud-sync.toml` and `location.toml` are gone, `settings.toml` is back in the config folder with this Mac's `[system]` (tray, hotkey). On the other computer Delete from Server… ▸ Delete: it stops there; on the Mac, Join With a Code… with the old code now says "No settings found for this code".
