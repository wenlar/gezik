# Stacked panes and the preview at the bottom on macOS: manual checks

Numbers 483–492 (spec §8: 16m's block). Results go to `../macos-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §7.

483. **Stack Panes from the menu bar.** View ▸ Two Panes (⌃⌘P), then View ▸ Stack Panes: the second pane goes under the first, both the window's width; the item has a check mark. `settings.toml` now has `[panes] stacked = true`. Choose it again: side by side, `stacked = false`, no check mark.
484. **Dragging the stacked splitter.** Stacked, the pointer over the line between the panes is an up-down arrow; dragging moves it (each pane keeps a fifth of the room, and at least its tab bar, address bar and column header), a double-click puts it in the middle. Unstack: the left pane has the same share of the width as the top one had of the height; quit and start again: the share is kept (`state.toml [panes] split`).
485. **Keyboard between stacked panes.** Tab in the list moves to the other pane and back, F5/F6 copy/move to it, ⌃⌘U (Swap Panes) swaps them, as side by side.
486. **A narrow window.** Stacked, make the window narrower than 640 px and open the second pane (⌃⌘P): it opens (side by side it says "The window is too narrow for two panes").
487. **Preview at Bottom, one pane.** View ▸ Show Preview, then View ▸ Preview at Bottom: the preview is under the list (the sidebar stays full height), checked in the menu, `[preview] position = "bottom"` in `settings.toml`. Drag the line above it: its height follows (120–600 px); quit and start again: the height is kept (`state.toml [preview] height`). A picture is sharp at the new size.
488. **Preview at Bottom, two panes.** With two panes (side by side and stacked) the preview is under both, the full width of the panes; dragging works the same. Narrowing the window does not close the preview (it only does that when it is on the right).
489. **Back to the right.** Choose Preview at Bottom again: the preview is on the right with its old width.
490. **Settings window.** Settings ▸ Layouts: "Stacked" (panes) and "Position" (preview: right, bottom) change the window at once, like the menu items, and the menu's check marks follow.
491. **Palette.** ⌘⇧P "Stack Panes" and "Preview at Bottom" work like the menu items; neither has a key by default.
492. **Several windows.** With two windows open, Stack Panes changes both.
