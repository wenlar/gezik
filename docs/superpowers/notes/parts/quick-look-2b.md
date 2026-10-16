# Quick look 2b ("stage first") — notes

Design: Claude Design project "Gezik", `Gezik Quick Look.dc.html`, variant 2b, with the Info column of 2a.

## What is in

- `ui/widgets/quick-look.slint` (`QuickLookView`): the stage takes the window (on `Theme.chrome` in dark themes, `Theme.surface` in light ones, picked by the background's brightness). One 44 px bar: name (bold) and summary ("PNG image · 19.4 KB · 399 × 366"), the viewer's bar (`PreviewBar`), Info and Open (primary).
- Previous / next (36 px round buttons) and "12 / 56" float over the stage. They show on pointer moves and go 2 s after the pointer rests (a Slint `Timer` that runs only after a move, so nothing ticks while idle), not while the pointer is on one of them. ← → keep moving in the list as before.
- Info opens a 248 px column: name, type, then label / value rows (`PreviewInfo.facts`: Size, Contents, Items, Dimensions, Modified, Created, Location) and the properties. Its state is kept for the next quick look in the same run (not in state.toml).
- Open runs the list's Enter (`open_selected`) and closes quick look.
- `preview-pane.slint` is split into `PreviewStage` and `PreviewBar`, which the pane and quick look share; the pane looks as before.
- Pictures smaller than the area show at 100% (`own-width`/`own-height`), in the pane too; zooming starts from the same place (`ZoomView::at_most_100`: a picture's fit stops at 100%; a PDF page still fills the view).
- Quick look's bar puts Hex after the zoom (`bar_tools(.., hex_last)`).

## Left out

- The zoom figure is not shown while fitted ("Fit" stays the button), as before: the design shows it all the time.
- The facts column has no right-click menu (quick look's window never had one).
- The 2a/2b "below 640 px the column moves under the stage" rule: the column stays at the side.
- No GUI screen test yet (the user was at the computer).
