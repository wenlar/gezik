# Pinned folder colour and icon on macOS: manual checks

Numbers 453–462 (spec 16 §8). Results go to `../macos-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §4.

453. **Colour.** Right-click a pinned folder ▸ Color ▸ Red: its icon turns red, and `settings.toml` has `{ path = "…", color = "red" }`. Selected, the icon takes the accent colour; deselected, red again. Color ▸ None goes back to the theme's colour.
454. **Icon.** Icon ▸ Star: the row shows a star. Icon ▸ Default and Color ▸ None: the entry is plain text (`"…"`) in `settings.toml` again.
455. **All icons.** Go through the 24 icons in light and dark appearance: each is centred, not clipped, with the same stroke as the other sidebar icons (Retina too).
456. **Alias.** On a pin with an alias, a chosen icon replaces the alias icon; Default brings the alias icon back.
457. **Bad values.** Write `color = "teal", icon = "rocket"` by hand: two warnings, the default icon in the theme colour; after another pin change Gezik writes, both values are still in the file. `color = "#2e7d32"` draws green.
458. **Menu marks.** The current colour and icon are marked with `•` in their submenus; the submenus open and close with the keyboard (arrows, Return, Escape).
