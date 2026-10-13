# Translating Gezik

Gezik's texts are written in English, and the English text is the key (gettext's way). A
translation is a gettext `.po` file: plain text that Poedit, Lokalize, Weblate or Crowdin open
directly, and any text editor too. Missing texts show in English, so a half-done translation
already works.

## Start a language

1. Take the template, [`crates/gezik-i18n/lang/gezik.pot`](../crates/gezik-i18n/lang/gezik.pot).
2. Open it in Poedit (*Create new translation*) or copy it to `<code>.po`, and fill the header:

   ```po
   msgid ""
   msgstr ""
   "Language: pl\n"
   "Plural-Forms: nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);\n"
   "X-Gezik-Name: Polski\n"
   "X-Gezik-Review: human\n"
   ```

   `X-Gezik-Name` is the language's name in itself (shown in Settings); `Plural-Forms` is the
   usual gettext rule (the built-in languages have theirs already).
3. Save it in the `lang` folder of Gezik's settings folder: `%APPDATA%\gezik\lang\pl.po` on
   Windows, `~/Library/Application Support/gezik/lang/` on macOS, `~/.config/gezik/lang/` on
   Linux (portable mode: `gezik-data/lang/`). The file name is the code.
4. Choose it: Settings ▸ General ▸ Language, or `language = "pl"` in `settings.toml`. Gezik
   reloads the file each time it is saved: no restart, no build.

A file named like a built-in language (`tr.po`) goes over it: your corrections win, the rest
comes from the built-in one.

## Problems

Gezik checks the file when it reads it and lists what is wrong in the status bar's notices,
with the line: a syntax error (then the file is not used), a placeholder missing in a
translation (`lang/pl.po line 120: "{0}" missing in the translation`; that text shows in
English), a plural with the wrong number of forms, and how many texts are missing.

With the source code, `cargo run -p gezik-i18n --bin gezik-lang -- check lang/pl.po` says the
same and more: texts not reviewed, texts the template no longer has, translations over twice
as long as the English.

## The pseudo language

`language = "pseudo"` shows every text that goes through the translations framed, accented
and about 40 % longer: `«Ŝéţţıñĝš ···»`. A text without the frame is not translatable yet (tell
us); a frame cut off on the right is clipped; two frames in one sentence mean the sentence is
put together from pieces.

## Rules

- **Placeholders** stay: `{}` (the next value), `{0}`, `{1}` (a given one, in any order you
  need), `{n}` (the count). `{{` and `}}` are braces.
- **No suffix on a placeholder.** Turkish `"{}'ı silinemedi"` breaks with names ending in a
  vowel or a quote; write around it: `"Silinemedi: {}"`, `"“{}” öğesi silinemedi"`.
- **Plurals:** fill every form `Plural-Forms` asks for; a form may leave `{n}` out ("one file").
- **Menus** follow the platform's habits in your language (sentence case in Turkish, "Yeni
  sekme"; nouns capitalised in German).
- Keep `…` at the end of a command that asks something before acting.
- Do not translate key names in shortcuts (`Ctrl+Shift+C`) except where the text's context is
  `key`.
- **Contexts** (`msgctxt`) tell the same English word apart: `menu`, `column`, `unit`, `key`,
  `button`, `choice:<setting>`, `setting:<table.key>`.
- **`#, fuzzy`** means "machine translated, not reviewed": Gezik shows these texts (and counts
  them); remove the flag once you have checked one. When none is left, set
  `X-Gezik-Review: human`.

## Contributing

Send the `.po` file as a pull request to `crates/gezik-i18n/lang/`. Whether a language becomes
built in (about 30 KB of the exe) is the maintainer's call; it works from the `lang` folder
either way.

For developers: new texts go through `t!`, `tn!` or `tx!` in Rust and `@tr` in Slint; then
`cargo run -p gezik-i18n --bin gezik-lang -- extract` writes the template and `… -- update`
fits every `.po` to it. `cargo test -p gezik-i18n` fails while the template is out of date.
