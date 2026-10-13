# 15c (messages): status lines, questions, errors — notes

Spec §6, §13.2 15c. The other half of 15c (numbers, sizes, dates) is in `15c-formats.md`; it is
merged into this branch so the two ship as one PR.

## What is in

- **Rust messages through `t!`/`tn!`/`tx!`**: status lines, dialog titles, messages and
  buttons, the operations panel and Undo labels, notices, settings warnings, the tools
  (checksums, compare, sync, duplicates, size map, export list, copy to, split, join, convert,
  batch rename, PDF), the System Integration window, the preview and player bars, the search
  status, Get Info, tags, the sidebar, field names (`fields.rs`, marked inside the `fields!`
  table), group headings, kind names for macOS/Linux (`describe.rs`), platform errors written
  for the user (network, eject, cloud, clipboard, download, administrator helper).
- **Pot:** 2,786 texts (15b: 951); this half adds ~1,826, ~187 of them plurals.
- **Plurals:** every count is a `tn!`. A plain number is `{n}` (grouped by the language since the
  formats half); a number the code already grouped (`with_commas`) goes in as `{}`, its
  formatting as it was.
- **Whole sentences:** `TaskKind::label` is one plural per kind ("Copy {n} item"); texts naming
  the Recycle Bin or the Trash are two sentences chosen with `cfg!(windows)` (`bin_name()` is
  gone); "Cannot {verb} a drive", the administrator prompt's lines, the tool offers (7-Zip,
  ffmpeg, pdfium), the hotkey errors and the key-modifier lists are whole sentences. A noun
  phrase (a name, "3 items") still goes into a placeholder. Lists keep a joiner
  (`t!("list" => " and ")`, `" · "` between status segments).
- **Constants** that are compared or sent between processes stay English and are marked
  `tx!`; they are translated where shown with `gezik_i18n::t("", X)` (the comparisons accept
  both: `NEEDS_ADMIN`, `settings_writer`'s kept notes, the preview row labels). The checksum
  verdicts have the context `checksum` (`OK` is also a button) and stay `sha256sum`'s words in
  files.
- **`gezik_i18n::t` of an empty text** is empty (the pseudo language framed it).
- **Language change:** the Filters button, the scope label and the field errors are rewritten
  by `Searches::set_settings` (15b's follow-up); `language::apply` says whether the language
  changed and `apply_config` resolves the settings again, so the warnings come out in it.
- **Decimals left by the formats half:** the search's seconds and GPS degrees use
  `gezik_i18n::number()`; the size map's percent has no decimals; `DateFormat`/`SizeFormat`
  labels and the date group headings are translated texts already.
- **Extractor (`gezik-lang`):** it stopped at a file's first `#[cfg(test)]`, so texts after a
  test-only item were lost (15b's `integration.rs` from line 308, `tags.rs`, `preview/mod.rs`,
  …); now it stops at the test module only (`mod x {`, with `pub`/`pub(crate)`, not
  `mod tests;`) and skips a macro's pattern (`tx!($label:literal)`).
- **Rust check:** `rust_messages_are_marked` (gezik-lang's tests) reads every call to a
  message sink (`status`, `tell`, `notify`, `warn`, `note`, `ask*`, `confirm*`,
  `set_*_{text,label,title,status,note,error,message,hint,tip,empty}`) and fails on an
  argument that is a plain string literal, a `format!`'s or one in a `&[…]` of them, unless it
  is an id (lower case, no space), tokens only, or in `KEPT` (two key-path joins). It cannot
  see texts built in helpers and returned; the pseudo-language walk covers those.

## Decisions against the task text

- **English changed where it was wrong at a count of 1** (was a missing plural): "1 more
  problem", "1 text missing", "Last 1 day", "1 file" in the size map, "Rename 1 item",
  "Convert 1 item", "1 folder … goes file by file", and progress segments now singular at 1.
  Two wordings changed for whole sentences: "Find duplicates {}" → "Find duplicates in {}",
  status segments "Stopped · …" and "… via Everything" are whole segments. No test needed the
  old forms.
- **Names made on disk follow the language** (as Explorer and Finder): "New folder",
  "New file.txt", "New document.md" and the "New folder with" folder.
- **Kept English:** logs, `--help`, settings keys and the template, file contents (exported
  CSV/HTML headers, checksum files, `.reg`, desktop entries), default export file names,
  sentinel texts matched by code ("ffmpeg needed"), the PDF worker processes' replies,
  internal diagnostics (D-Bus, registry, X11, HTTP internals), the `Verdict` words in files,
  shortcut key names (spec §8.3, a later part).

## Follow-ups

- **Texts made before the language is known** stay English: the portable/translocated notes
  (`paths.rs`), "no config folder", the Windows `--shell` fallback question.
- **Read once, then kept:** the known folder "Home" and the drive fallback names stay in the
  old language after a language change until they are read again.
- **Translators need context** on multi-part texts with notes (`// TRANSLATORS:`); the Turkish
  pass (15e) should check the tool offers and the administrator prompt read well.
- **Spoken/list joins** (`" · "`, `" and "`) are lists, as 15b's.

## Checks

`cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test
--workspace` clean on Windows (`-j 4`); `cargo clippy -p gezik --target aarch64-apple-darwin
--no-default-features -D warnings` and `cargo check` of core/platform/ops/config/search and
`gezik-batch --no-default-features` for x86_64-unknown-linux-gnu clean. The GUI was not run
(pseudo-language walk: checklist items below).

## Size

Release exe 29,800,960 B with both 15c halves and master (15b + 15d) merged. Master was not
built here: 15b's 29,598,208 B + 15d's +5,120 B ≈ 29,603,328 B, so 15c is ≈ +197,632 B
(budget +262,144 B; no split needed). Before merging master: 29,798,400 B against 15b, +200,192
B. Copy: `%TEMP%\gezik-gui-15\bin\t15c-msg.exe`.
