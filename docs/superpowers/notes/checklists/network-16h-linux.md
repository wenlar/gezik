# Network discovery (16h) on Linux: manual checks

Numbering goes on after `update-linux.md` (315). Results go to `../linux-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §2.

**Setup:** `avahi-daemon` running and `avahi-browse` installed (`sudo apt install avahi-utils`); a Mac with File Sharing, a NAS or a Samba server announcing `_smb._tcp` on the same network (`avahi-browse -tr _smb._tcp` lists it).

316. **Closed at start.** The sidebar ends with a NETWORK heading (chevron right), nothing under it; `pgrep avahi-browse` finds nothing.
317. **Open and search.** Click the heading: "Searching…", then the computers with a monitor icon (one row each, even when they answer on IPv4 and IPv6). Within 5 s "Searching…" goes and `pgrep avahi-browse` finds nothing. Nothing announcing: "No computers found".
318. **Connect.** Click a computer: Connect to Server opens with `smb://<host>.local/`; add a share, Connect: gio mounts it (login asked if needed) and it opens.
319. **No Avahi.** `sudo systemctl stop avahi-daemon avahi-daemon.socket`, right-click the heading ▸ Refresh: "Avahi is not running". The same with `avahi-browse` uninstalled. Start the daemon again, Refresh: the list comes back. Closing the section drops the rows.
