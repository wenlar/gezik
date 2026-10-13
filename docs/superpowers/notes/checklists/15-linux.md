# Step 15 on Linux: manual checks

Numbering goes on from `../linux-test.md` (it ends at 281). Results go to `../linux-test-results.md`.

### 15a, localisation machinery

282. **The system's language.** Start with `LANGUAGE=de:en LANG=de_DE.UTF-8 ./gezik.AppImage`: Gezik opens in English (German arrives in 15f), Settings ▸ General ▸ Language's list says `auto — System: English` (`System: Deutsch` from 15f on). Then `LANG=C ./gezik.AppImage` and `LANG=C.UTF-8 LANGUAGE=tr ./gezik.AppImage`: English, no warning. `language = "pseudo"` in `~/.config/gezik/settings.toml` while Gezik runs: the Settings window's title turns `«Ŝéţţıñĝš — Ĝéžık ·····»` at once, with « » drawn (GNOME and KDE). A dropped `~/.config/gezik/lang/pl.po` as macOS 389.
