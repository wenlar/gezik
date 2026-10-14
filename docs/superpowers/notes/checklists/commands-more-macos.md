# Commands with the other pane and nested headings on macOS: manual checks

Numbering goes on from `16-macos.md` (it ends at 412). Results go to `../macos-test-results.md`.

413. **`{other}` and `{other-files}`.** In `settings.toml` add `[[commands]]` `name = "Args"`, `run = ["/bin/sh", "-c", "printf '%s\n' \"$@\" > /tmp/gezik-args.txt", "sh", "{files}", "{other-files}", "{other}"]`. Open the second pane (F3), select two files on the left and one on the right, go back to the left, right-click ▸ Commands ▸ Args: `/tmp/gezik-args.txt` lists the two left files, the right file and the right folder, one per line. Close the second pane and run it again (also from the palette, ⌘⇧P "Args"): nothing runs, the status bar says "Args — needs the second pane".
414. **Not a folder on disk.** With the second pane inside a zip (16a), then on a phone (16g), then showing search results: Args says "the other pane shows no folder on disk". With nothing selected in the other pane: "select the items in the other pane".
415. **Nested heading, native menus.** Give one command `menu = "Images/Resize"`, another `menu = "Images"` and a key (`shortcut = "ctrl+alt+r"`): right-click ▸ Commands shows the "Images" heading with its command, then "Images ▸ Resize" with the other; the menu bar's Commands menu shows the keyed one under its heading.
