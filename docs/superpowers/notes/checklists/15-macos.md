# Step 15 on macOS: manual checks

Numbering goes on from `../macos-test.md` (it ends at 387). Results go to `../macos-test-results.md`.

### 15a, localisation machinery

388. **The language row and the pseudo language.** System Settings ▸ General ▸ Language & Region ▸ Applications ▸ `+` Gezik = Türkçe, then start Gezik with `language = "auto"`: Settings (⌘,) ▸ General ▸ Language shows `auto` and its list says `auto — System: …` (English until Turkish arrives in 15e; this item is repeated then). Choose `pseudo — Pseudo (for translators)`: without a restart the Settings window's title reads `«Ŝéţţıñĝš — Ĝéžık ·····»` (in every window that has it open), and `settings.toml` says `language = "pseudo"`. The frame characters « » draw (no empty boxes). Choose `auto` again: the title is `Settings — Gezik`. Remove the per-app language afterwards.
389. **A dropped language.** Copy `crates/gezik-i18n/lang/gezik.pot` to `~/Library/Application Support/gezik/lang/pl.po`, add `"X-Gezik-Name: Polski\n"` to its header and translate `Settings — Gezik` as `Ustawienia — Gezik`. Within a second the Language list has `pl — Polski — … % translated`; choose it: the Settings title is Polish, everything else English. Break a line (delete a closing `"`) and save: the status bar's notice says `lang/pl.po line N: …` and the title goes back to English; fix it and save: Polish again. Translate `{} % translated` as `procent` (no `{}`): the notice says `"{0}" missing in the translation`.
