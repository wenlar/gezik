# Commands with the other pane and nested headings on Linux: manual checks

Numbering goes on from `commands-more-macos.md` (it ends at 415). Results go to `../linux-test-results.md`.

297. **`{other}` and `{other-files}`.** In `settings.toml` add `[[commands]]` `name = "Args"`, `run = ["/bin/sh", "-c", "printf '%s\n' \"$@\" > /tmp/gezik-args.txt", "sh", "{files}", "{other-files}", "{other}"]`. Open the second pane (F3), select two files on the left and one on the right, go back to the left, right-click ▸ Commands ▸ Args: `/tmp/gezik-args.txt` lists the two left files, the right file and the right folder, one per line. A name with a space and a `{` in it arrives as one line. Close the second pane and run it again: nothing runs, the status bar says "Args — needs the second pane".
298. **Not a folder on disk.** With the second pane inside a zip, then on a phone through libmtp (`mtp://…`), then showing search results: Args says "the other pane shows no folder on disk". A phone gvfs mounted (`/run/user/…/gvfs/…`) is a plain folder and runs. With nothing selected in the other pane: "select the items in the other pane".
299. **Nested heading in Gezik's own menu.** Give one command `menu = "Images/Resize"` and another `menu = " Images / "`: right-click ▸ Commands shows "Images" with its command, then "Images ▸ Resize" with the other (greyed headings, not clickable).
