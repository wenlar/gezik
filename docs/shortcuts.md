# Keyboard shortcuts

| Action | Windows / Linux | macOS |
|---|---|---|
| New tab / close tab | Ctrl+T / Ctrl+W | ⌘T / ⌘W |
| New window | Ctrl+N | ⌘N |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab | same |
| Tab 1…8 / last tab | Ctrl+1…8 / Ctrl+9 | ⌘1…8 / ⌘9 |
| Reopen closed tab / list tabs | Ctrl+Shift+T / Ctrl+Shift+A | ⌘⇧T / ⌘⇧A |
| Back / forward | Alt+← / Alt+→ (or mouse side buttons) | ⌘[ / ⌘] |
| Parent folder | Alt+↑ | ⌘↑ |
| Type a path | Ctrl+L | ⌘L |
| Refresh | F5 or Ctrl+R (Ctrl+R with two panes) | ⌘R |
| Select all | Ctrl+A | ⌘A |
| List / grid / columns view | Ctrl+Shift+1 / Ctrl+Shift+2 / Ctrl+Shift+3 | ⌘⇧1 / ⌘⇧2 / ⌃⌘3 |
| Show or hide the preview pane | Alt+P | Alt+P |
| Quick look (only while the file list has focus) | Space | Space |
| Show hidden items | Ctrl+H | ⌘⇧. |
| Filter | Ctrl+F or / | ⌘F or / |
| Search / flat view | Ctrl+Shift+F or Ctrl+E / Ctrl+B | ⌘⇧F / ⌘B |
| Two panes / switch to the other pane | F3 / Tab (file list) | ⌃⌘P / Tab (file list) |
| Copy / move to the other pane (two panes; asks first) | F5 / F6 | F5 / F6 |
| Swap the panes' tabs | Ctrl+U | ⌃⌘U |
| Command palette / quick open | Ctrl+Shift+P / Ctrl+P | ⌘⇧P / ⌘P |
| Copy / cut / paste | Ctrl+C / Ctrl+X / Ctrl+V | ⌘C / ⌘X / ⌘V (⌘⌥V moves) |
| Delete / delete permanently | Delete / Shift+Delete | ⌘⌫ / ⌘⌥⌫ |
| Rename | F2 | Enter |
| New folder / new folder with selection | Ctrl+Shift+N / Ctrl+Alt+N | ⌘⇧N / ⌃⌘N |
| Duplicate | (menu) | ⌘D |
| Undo / redo | Ctrl+Z / Ctrl+Y | ⌘Z / ⌘⇧Z |
| Open terminal | Shift+F4 or Ctrl+Alt+T | ⌘⌥T |
| Copy path | Ctrl+Shift+C | ⌘⌥C |
| Get Info (Windows: Properties) | Alt+Enter | ⌘I |
| Empty Trash | (menu) | ⌘⇧⌫ |

**What changed:** F3 now opens a second pane. Search is Ctrl+Shift+F or Ctrl+E. With two
panes F5 copies and F6 moves to the other pane; with one, F5 refreshes as before.

Change them in `settings.toml` under `[shortcuts]` (`"mod"` is ⌘ on macOS and Ctrl
elsewhere, `""` disables one, a list gives several keys). The template lists every action,
including those with no key by default (`show-trash`, `system-integration`,
`calculate-folder-sizes`, `toggle-stack` and more). Ctrl+wheel in a folder changes the grid
or icon size.

The file list keys are fixed: arrow keys, PgUp/PgDn, Home/End move the focus; Shift with
them extends the selection; Ctrl with the arrows moves the focus without selecting;
Ctrl+Space toggles the focused item; Enter opens the selection (on macOS Enter renames and ⌘↓ opens); Esc clears it. Type a
name's first letters to jump to it. In quick look, the arrows move through the folder
and Space or Esc closes it.

---

[← Back to README](../README.md)
