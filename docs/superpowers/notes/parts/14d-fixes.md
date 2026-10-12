# 14d: fixes after the screen test

The 14d screen test passed 10/10; these small issues from it are fixed on `feat/portable-14e`.

1. **Palette Export Settings… / Import Settings…** now give the keyboard to their path field (scrolled into view), as Get Theme… does, instead of the search box.
2. **Text fields are named by their title** (the screen reader read the old default path as the Export field's name after the text changed); the value is the field's text.
3. **▸ in descriptions** (the bundled font has no such glyph) shows as `>`: "The New > templates", and the template's "Group by ▸ …" comments in the window. The rows leave room on the right for the scroll bar, so the on/off boxes no longer touch the edge.
4. **"1 setting in it runs programs"** in the singular. When an import's preview comes in, its Import or Cancel row is scrolled into view (and has the keyboard).
5. **An outcome stays in the status line:** a reload replaces the rows' own earlier status (a problem now fixed) or says a new problem, but no longer turns "Imported: Undo Import puts the files back" into "Type in the box above to search every page" after an import over a broken settings.toml.

Not screen-tested again (no GUI while the user is at the PC).
