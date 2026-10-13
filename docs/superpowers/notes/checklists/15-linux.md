# Step 15 on Linux: manual checks

Numbering goes on from `../linux-test.md` (it ends at 281). Results go to `../linux-test-results.md`.

### 15a, localisation machinery

282. **The system's language.** Start with `LANGUAGE=de:en LANG=de_DE.UTF-8 ./gezik.AppImage`: Gezik opens in English (German arrives in 15f), Settings ▸ General ▸ Language's list says `auto — System: English` (`System: Deutsch` from 15f on). Then `LANG=C ./gezik.AppImage` and `LANG=C.UTF-8 LANGUAGE=tr ./gezik.AppImage`: English, no warning. `language = "pseudo"` in `~/.config/gezik/settings.toml` while Gezik runs: the Settings window's title turns `«Ŝéţţıñĝš — Ĝéžık ·····»` at once, with « » drawn (GNOME and KDE). A dropped `~/.config/gezik/lang/pl.po` as macOS 389.

### 15d, CJK font fallback

285. **Han characters.** With Noto Sans CJK installed, a folder with `测试这检查对齐.txt` and `日本語のファイル.txt`: `LANG=en_US.UTF-8 ./gezik.AppImage` draws `这` and `对` in their Simplified forms (no boxes); `LANGUAGE=ja:en LANG=ja_JP.UTF-8 ./gezik.AppImage` draws `日本語` with the Japanese forms (compare `直` and `骨` in a file name: their strokes differ between the two runs). Without a CJK font: boxes, no crash (the "install a Chinese font" note comes with 15f).

### 15b, the interface's texts

283. **Menus in the pseudo language.** `language = "pseudo"`: the `≡` menu, the right-click menus (a file, the empty list, a tab, the sidebar, a column header), the drop menu after a right-drag, the palette (Ctrl+Shift+P) and the Settings window's pages, titles, descriptions and choices are framed; none cut off. The tray icon has no menu on Linux (spec 9 decision 24): nothing to check there. GNOME and KDE.

### 15c, messages

284. **Messages in the pseudo language.** As macOS 393 (the Trash wording, the operations panel, the tools, System Integration, a settings warning, the search status): every text framed, then plain English again with `auto`, the Filters button included. GNOME and KDE.
