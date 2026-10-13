# Step 15 on Linux: manual checks

Numbering goes on from `../linux-test.md` (it ends at 281). Results go to `../linux-test-results.md`.

### 15a, localisation machinery

282. **The system's language.** Start with `LANGUAGE=de:en LANG=de_DE.UTF-8 ./gezik.AppImage`: Gezik opens in English (German arrives in 15f), Settings ▸ General ▸ Language's list says `auto — System: English` (`System: Deutsch` from 15f on). Then `LANG=C ./gezik.AppImage` and `LANG=C.UTF-8 LANGUAGE=tr ./gezik.AppImage`: English, no warning. `language = "pseudo"` in `~/.config/gezik/settings.toml` while Gezik runs: the Settings window's title turns `«Ŝéţţıñĝš — Ĝéžık ·····»` at once, with « » drawn (GNOME and KDE). A dropped `~/.config/gezik/lang/pl.po` as macOS 389.

### 15d, CJK font fallback

285. **Han characters.** With Noto Sans CJK installed, a folder with `测试这检查对齐.txt` and `日本語のファイル.txt`: `LANG=en_US.UTF-8 ./gezik.AppImage` draws `这` and `对` in their Simplified forms (no boxes); `LANGUAGE=ja:en LANG=ja_JP.UTF-8 ./gezik.AppImage` draws `日本語` with the Japanese forms (compare `直` and `骨` in a file name: their strokes differ between the two runs). Without a CJK font: boxes, no crash (the "install a Chinese font" note comes with 15f).
