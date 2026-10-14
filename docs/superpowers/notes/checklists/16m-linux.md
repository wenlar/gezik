# Stacked panes and the preview at the bottom on Linux: manual checks

Numbers 366–375 (spec §8: 16m's block). Results go to `../linux-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §7.

366. **Stack Panes from the View button.** F3 for two panes, then the View button ▸ Stack panes: the second pane goes under the first, both the window's width; the item is marked. `settings.toml` has `[panes] stacked = true`. Again: side by side, `stacked = false`.
367. **Dragging the stacked splitter.** The pointer over the line between the stacked panes is an up-down arrow; dragging moves it, a double-click centres it. The share is kept after a restart (`state.toml [panes] split`) and is the same share side by side.
368. **Keyboard between stacked panes.** Tab, F5, F6 and Ctrl+U work as side by side.
369. **A narrow window.** Stacked, under 640 px wide, F3 opens the second pane.
370. **Preview at Bottom, one pane.** Alt+P, then View button ▸ Preview at bottom: the preview is under the list, its height dragged (120–600 px) and kept after a restart (`state.toml [preview] height`); `[preview] position = "bottom"`.
371. **Preview at Bottom, two panes.** Side by side and stacked: the preview is under both panes, full width; narrowing the window does not close it.
372. **Back to the right.** Preview at bottom again: on the right with its old width.
373. **Settings window.** Settings ▸ Layouts: "Stacked" and "Position" change the window at once.
374. **Palette.** Ctrl+Shift+P "Stack Panes" and "Preview at Bottom" work; no default keys.
375. **Fractional scaling (GNOME 125 %/150 %).** The splitter lines and the bottom preview's border are sharp, no gap or overlap between the stacked panes.
