# Network discovery (16h) on macOS: manual checks

Numbering goes on after `update-macos.md` (432). Results go to `../macos-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §2.

**Setup:** another Mac with File Sharing on (System Settings ▸ General ▸ Sharing), or a NAS that announces SMB over Bonjour, on the same network. Finder's sidebar ▸ Network should list it.

433. **Closed at start.** The sidebar ends with a NETWORK heading with a right-pointing chevron and nothing under it. Activity Monitor or `sudo lsof -i UDP:5353 -p <gezik pid>`: no mDNS traffic from Gezik while it is closed.
434. **Open and search.** Click the heading: the chevron points down, "Searching…" shows, the other Mac (its Bonjour name, a monitor icon) comes within a second or two. After 5 s "Searching…" goes. With nothing announcing SMB: "No computers found" after 5 s.
435. **Connect.** Click the other Mac: Connect to Server opens with `smb://<host>.local/` and the note "Add the share: smb://server/share". Add a share's name and Connect: the login window (NetAuth) asks if needed, the share opens and shows under the drives.
436. **Refresh and close.** Right-click the heading ▸ Refresh: the list empties and the search runs again. Turn off File Sharing on the other Mac, Refresh: it is gone. Click the heading: the section closes and its rows go; open it again: a new search. A Bonjour name with spaces or an apostrophe ("Ali's MacBook") shows as is and connects to its host name.
