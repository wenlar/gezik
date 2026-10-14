# Pinned folder colour and icon on Linux: manual checks

Numbers 336–345 (spec 16 §8). Results go to `../linux-test-results.md`. Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §4.

336. **Colour.** Right-click a pinned folder ▸ Color ▸ Blue: its icon turns blue, and `settings.toml` has `{ path = "…", color = "blue" }`. Selected, the icon takes the accent colour; deselected, blue again. Color ▸ None goes back to the theme's colour.
337. **Icon.** Icon ▸ Leaf: the row shows a leaf. Icon ▸ Default and Color ▸ None: the entry is plain text (`"…"`) in `settings.toml` again.
338. **All icons.** Go through the 24 icons in light and dark themes and at 100 % and 200 % scale: each is centred, not clipped, with the same stroke as the other sidebar icons.
339. **Bad values.** Write `color = "teal", icon = "rocket"` by hand: two warnings, the default icon in the theme colour; after another pin change Gezik writes, both values are still in the file. `color = "#2e7d32"` draws green.
