# Spring-loaded folders on Linux: manual checks

Numbers 356–365 (spec `../../specs/2026-10-15-tamamlayicilar-design.md` §6, §8). Results go to `../linux-test-results.md`.

356. **List folder.** Drag a file and rest it, without moving, on a subfolder in the list: after about 0.7 s the pane opens it. Rest on a folder there: it opens in turn. Drop on the empty space: the file goes there and the pane stays.
357. **Esc goes back.** Spring twice, press Esc before releasing: the pane goes back to the folder the drag started in. The release after Esc does nothing.
358. **Moving resets the wait.** Move slowly across several folders, never resting 0.7 s: none opens.
359. **Sidebar and tree.** Rest on a pinned folder, a drive, and a folder in the sidebar tree: the active pane opens it. A tag row does not spring.
360. **Miller columns and grid.** A folder row of another column, and a folder cell in the grid view, spring the same way.
361. **Out of Gezik (X11 and Wayland).** Spring into a folder, drag out of the window and drop on the desktop or in Nautilus/Dolphin: the pane goes back to where the drag started. Come back into the window without releasing and drop on a folder: the pane stays.
362. **Setting.** Settings ▸ General ▸ Spring delay: 0 turns it off; 2000 waits 2 s; `[drag] spring-delay = 50` acts as 200, `= 9000` as 3000, no warning; `= -1` warns and keeps 700.
363. **Not into itself.** Dragging a folder over itself does not open it.
364. **Busy listing.** Spring into a folder with thousands of entries: nothing else springs until it is listed; then resting on a subfolder springs without moving first.
365. **From another program.** The spec asks only for Gezik's own drags on Linux, but offers from other programs take the same path: drag a file from Nautilus/Dolphin (X11, then Wayland) over a Gezik folder and rest. Note whether it opens; if it does, drop (the pane stays) and try again leaving the window without dropping (the pane goes back). If the source sends no position while the pointer rests, nothing opens: that is the known limit.
