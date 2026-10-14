# Spring-loaded folders on macOS: manual checks

Numbers 473–482 (spec `../../specs/2026-10-15-tamamlayicilar-design.md` §6, §8). Results go to `../macos-test-results.md`.

473. **List folder.** Drag a file from a folder that has subfolders and rest it, without moving, on a subfolder in the list: after about 0.7 s the pane opens that folder. Rest on a folder there: it opens in turn. Drop on the empty space: the file moves (or copies) there and the pane stays in that folder.
474. **Esc goes back.** Do the same two springs, then press Esc before releasing: the pane goes back to the folder the drag started in (one step, not through the folder in between). The release after Esc does nothing.
475. **Moving resets the wait.** Move slowly across several folders, never resting 0.7 s on one: none opens. Wiggle inside one folder's row: it still opens after 0.7 s from the moment the pointer came onto it.
476. **Sidebar and tree.** Rest the drag on a pinned folder, a drive and a folder in the sidebar tree: the active pane opens it. A tag row or "This PC" does not spring.
477. **Miller columns and grid.** In the columns view, rest on a folder row of another column: the pane opens it. In the grid view the folder cell springs the same way.
478. **Out of Gezik.** Spring into a folder, then drag out of the window and drop on the Desktop: the file is copied there and the Gezik pane goes back to where the drag started. Again, but come back into the window without releasing and drop on a folder: the pane stays where it sprang.
479. **From Finder.** Drag a file from Finder over a Gezik folder and rest: it opens (`draggingUpdated`). Drop: copied there, the pane stays. Drag from Finder, spring, then leave the Gezik window and release in Finder: the pane goes back.
480. **Setting.** Settings ▸ General ▸ Spring delay: 0 turns it off (no folder opens, tabs still open after resting on them); 2000 waits 2 s; `[drag] spring-delay = 50` in settings.toml acts as 200, `= 9000` as 3000, with no warning; `= -1` warns in the status bar and keeps 700.
481. **Not into itself.** Drag a folder and keep the pointer on that same folder: it does not open (nothing can be dropped into itself). A folder in a read-only place (a mounted dmg) does not spring either.
482. **Busy listing.** Spring into a folder with thousands of entries: until it is listed nothing else springs; once listed, resting on a subfolder springs again without moving the mouse first (the target is found anew).
