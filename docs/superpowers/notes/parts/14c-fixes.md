# 14c screen test: settings window fixes — notes

Found in the Windows screen test of 14c; fixed on the 14d branch in a commit of their own.

1. **Installed row after Install / Keep Both:** the window now updates its theme list from the file it just wrote (and a config reload picks the rows again), so the `Installed` row and the "Use Now is in its row" line show at once. A reload no longer overwrites a neutral status line (it still replaces a red one).
2. **Reset to Default on a shortcut** goes through the same conflict check as Set Key: when another action or command has the default key, the row asks Replace?/Cancel and writes both changes only on Replace (test `reset_asks_when_another_has_the_default_key`).
3. **Get Theme… from the palette** scrolls the Get Theme row into view and gives its field the keyboard (window property `focus-row`). A row whose choice list opens near the bottom scrolls so the list shows. Rows are now updated in place when some come or go (added or taken at the end), so the Get Theme field keeps the keyboard after a download; the backup rows of 14d use the same path.
4. **Esc on Replace?** is Cancel: the question leaves the status line.
5. **Screen reader:** an installed theme whose file is not its name reads "Nord (nord-2.toml)"; the open choice list is named ("Choices for …", "Menu of …"); the page list's tab stop is named "Settings pages: <page>".
6. **`[shortcuts]` (any new table) after the last value** — `[[commands]]` included — gets a blank line before it; after a trailing comment it stays right below the comment, as before.

Not checked on screen (no GUI run): the scroll arithmetic of 3 and the UIA names of 5.
