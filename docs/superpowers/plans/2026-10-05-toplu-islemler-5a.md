# Toplu İşlemler 5a (Toplu Yeniden Adlandırma) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** İki veya daha fazla öğeyi kurallarla (bul-değiştir, numara, büyük/küçük harf, metin ekleme, uzantı, şablon, temizlik), canlı önizlemeyle ve tek Ctrl+Z ile geri alınabilir biçimde yeniden adlandırmak.

**Architecture:** Kuralların veri türleri, şablon ayrıştırma, tarih biçimleme ve büyük/küçük harf `gezik-core::batch`'te (bağımlılıksız, saf). Döngü/zincir sıralaması `gezik-core::ops::renames`'te (saf). Kuralları uygulayan motor (regex, EXIF) yeni `gezik-batch` crate'inde. Yeniden adlandırmayı yapan `RenameTask` `gezik-ops`'ta: geri alma (`inverse`) onu kurabilmeli. Arayüz pencere içi bir katman (`RenameBatch`), denetleyicisi `crates/gezik/src/batch_rename.rs`.

**Tech Stack:** Rust 2024, Slint 1.18 (yazılımla çizim), `regex` 1 (sınırlı özellikler), `kamadak-exif` 0.6, `toml`/`toml_edit` (var).

**Spec:** `docs/superpowers/specs/2026-10-05-toplu-islemler-design.md` (bölüm 3, 4, 10, 11, 12)

## Spec'ten sapmalar ve netleştirmeler

1. **`RenameTask` `gezik-ops`'ta, `gezik-batch`'te değil.** Geri alma (`gezik-ops::inverse`) bir döngüyü (a↔b) geri çevirirken aynı sıralamayı kullanmalı; `MoveTask::back` döngüde çakışırdı. Görev ağır bağımlılık gerektirmiyor. `gezik-batch` 5a'da yalnız kural motorunu (regex, EXIF) taşır.
2. **Ayrı `TaskKind::BatchRename` yok.** `TaskKind::Rename`'in etiketi sayıya göre değişir: 1 öğe "Rename", çok öğe "Rename 24 items". Panel ve Undo menüsü bunu kullanır.
3. **Kural veri türleri `gezik-core::batch`'te** (ayar dosyası ve `state.toml` onları okuyup yazabilsin diye; `gezik-config` `gezik-batch`'e bağlanmaz). Regex ve EXIF gerektiren uygulama `gezik-batch`'te.
4. **Elle düzenleme önizleme hücresinde değil, önizlemenin altındaki "Name" alanında:** seçili satırın yeni adı orada düzenlenir; çift tıklama veya F2 alanı odaklar. Slint'te tembel liste satırı içinde metin girişi odak sorunları doğuruyor (4a'nın GUI raporundaki hata 1-2); tek alan bunları önler.
5. **Önizlemede sürükleyerek sıralama Slint içinde yerel:** ≡ tutamağı basılıyken işaretçinin satır indeksi hesaplanır, bırakınca `reorder(from, to)`; 4b'nin pencere/sistem sürükleme kodu gerekmez.
6. **Klasör başına numara** ve şablon alanları spec'teki gibi; `{size}` insan okunur boyuttur (`gezik_core::format_size`).
7. **Toplu yeniden adlandırma bulunulan klasörün seçimine uygulanır** (Gezik'te seçim hep tek klasördedir). `existing` = klasördeki seçilmemiş öğelerin adları.
8. **Geçici adlar** `.gezik-rn-<pid>-<n>`; her biri `pending-deletes`'e `restore` satırı olarak yazılır (`was_hidden = true`: öznitelik değişmez) ve yerine varınca silinir. Çökmede açılışta eski adına döner (mevcut `recover_deletes`).
9. **regex boyutu:** `regex = { version = "1", default-features = false, features = ["std", "perf", "unicode-case", "unicode-perl"] }`. Görev 4 sürüm derlemesinde exe büyümesini ölçer; +0,5 MB'ı aşarsa `perf` çıkarılır, yine aşarsa `regex-lite`'a geçilir ve not edilir.

## Global Constraints

- Rust edition 2024, stable. Komutlar `D:\Work\gezik` içinden; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken; uygulamayı elle çalıştırırken `GEZIK_CONFIG_DIR` geçici klasöre.
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- `cargo check -p gezik --target aarch64-apple-darwin` ve `cargo check -p gezik-core -p gezik-platform -p gezik-ops -p gezik-batch --target x86_64-unknown-linux-gnu` platform veya bağımlılık değiştiren her görevden sonra geçer.
- Slint yalnızca yazılımla çizim; Slint özellik listesine ekleme yok.
- Yeni bağımlılıklar tam olarak: `gezik-batch` → `regex` (yukarıdaki özelliklerle), `kamadak-exif = "0.6"`. Başka crate yok.
- Arayüz iş parçacığı dosya sistemine dokunmaz (EXIF okuma arka plan iş parçacığında). Yeniden adlandırma motor işidir; tek Ctrl+Z geri alır.
- Hiçbir dosya kullanıcı seçmeden ezilmez: `move_entry` hedef varsa başarısız olur; plan, işin kendisinin boşaltmadığı dolu hedefler için çakışma listesini açar.
- Arayüz metinleri İngilizce (mevcut arayüz gibi); commit mesajları İngilizce, Claude imzası yok.
- Kısayol biçimi önceki alt projelerdeki gibi (`mod` = macOS'ta Cmd, diğerlerinde Ctrl).

## Review Focus

1. **Büyük/küçük harf duyarsız sistemde yalnız harf değişen ad ile başka öğenin adı çakışması** (`a.txt`→`B.txt`, `b.txt`→`a.txt`): döngü sayılmalı, iki öğe de yerine varmalı. → Görev 2 `case_insensitive_cycle` testi.
2. **Aynı yeni ad iki kez, biri büyük harfle** (`Foto.jpg` ve `foto.jpg` Windows'ta): `Duplicate` sayılmalı. → Görev 4 `duplicates_ignore_case_where_the_system_does` testi.
3. **İptal/hata ortada kalırsa geçici adlı öğe:** sonraki açılışta eski adına dönmeli. → Görev 3 `a_failed_second_step_leaves_a_restore_note` testi.
4. **Geçersiz regex veya bilinmeyen şablon alanı yazılırken:** önizleme çökmez, kural satırında hata görünür, Rename kapalı. → Görev 4 `a_bad_rule_reports_and_changes_nothing` testi.
5. **Bir döngünün geri alınması** (a↔b, sonra Ctrl+Z): çakışma listesi açılmadan eski adlar gelmeli. → Görev 3 `undo_of_a_swap_swaps_back` testi.

---

## Dosya yapısı

| Dosya | Sorumluluk |
|---|---|
| `crates/gezik-core/src/batch/mod.rs` (yeni) | Modül kökü |
| `crates/gezik-core/src/batch/rules.rs` (yeni) | `Rule`, `RuleEntry` ve alt türler; `Rule::summary()` |
| `crates/gezik-core/src/batch/template.rs` (yeni) | Şablon ayrıştırma (`{n:03}`, `{date:%Y}`…) |
| `crates/gezik-core/src/batch/date.rs` (yeni) | `DateParts`, strftime alt kümesi |
| `crates/gezik-core/src/batch/case.rs` (yeni) | Büyük/küçük harf, Türkçe kuralları |
| `crates/gezik-core/src/ops/renames.rs` (yeni) | Yeniden adlandırma sırası: zincirler, döngüler, yalnız harf değişikliği |
| `crates/gezik-core/src/ops/paths.rs` | `path_key` (karşılaştırma anahtarı) dışa açılır |
| `crates/gezik-ops/src/tasks/rename.rs` (yeni) | `RenameTask` |
| `crates/gezik-ops/src/tasks/move_.rs` | `MoveTask::rename` kalkar |
| `crates/gezik-ops/src/inverse.rs` | Aynı klasördeki taşımaların tersi `RenameTask::back` |
| `crates/gezik-ops/src/task.rs` | `RunCx::note_temp/forget_temp`; `TaskKind::Rename` etiketi |
| `crates/gezik-batch/` (yeni crate) | `rename.rs` (kural motoru, denetim), `exif.rs` (çekim tarihi) |
| `crates/gezik-platform/src/datetime.rs` | `local_date_parts` |
| `crates/gezik-platform/src/locale.rs` (yeni) | `language()` |
| `crates/gezik-config/src/batch_toml.rs` (yeni) | Kuralların TOML okuma/yazma |
| `crates/gezik-config/src/settings.rs` | `Settings::rename_presets`, `State::batch_rename` |
| `crates/gezik-config/src/settings_edit.rs` | `with_rename_presets` |
| `crates/gezik-config/src/store.rs` | `save_rename_presets` |
| `crates/gezik-config/src/shortcuts.rs` | `Action::BatchRename` |
| `crates/gezik-config/templates/settings.toml` | Yorumlu `[[rename-presets]]` örneği |
| `crates/gezik/ui/widgets/rename-batch.slint` (yeni) | Katman |
| `crates/gezik/src/batch_rename.rs` (yeni) | Katmanın denetleyicisi |
| `crates/gezik/src/operations.rs`, `main.rs`, `keys.rs`, `context_menu.rs`, `ui/app.slint` | Bağlantılar |
| `scripts/perf/batch.ps1` (yeni) | Önizleme ve uygulama ölçümü |

---

### Task 1: `gezik-core::batch` — kural türleri, şablon, tarih, büyük/küçük harf

**Files:**
- Create: `crates/gezik-core/src/batch/mod.rs`, `rules.rs`, `template.rs`, `date.rs`, `case.rs`
- Modify: `crates/gezik-core/src/lib.rs` (`pub mod batch;`)

**Interfaces:**
- Produces:
  - `gezik_core::batch::date::{DateParts, check_format(&str) -> Result<(), String>, format(&str, &DateParts) -> String}`
  - `gezik_core::batch::case::{Lang, CaseMode, convert(&str, CaseMode, Lang) -> String}`
  - `gezik_core::batch::template::{Piece, parse(&str) -> Result<Vec<Piece>, String>}`
  - `gezik_core::batch::rules::{RuleEntry, Rule, ReplaceRule, NumberRule, NumberAt, ExtensionRule, CleanRule, Rule::summary(&self) -> String, Rule::kind_name(&self) -> &'static str, Rule::default_of(kind: &str) -> Option<Rule>, RuleEntry::new(Rule) -> RuleEntry}`

- [ ] **Step 1: `lib.rs`'e modülü ekle ve `batch/mod.rs`'i yaz**

`crates/gezik-core/src/lib.rs`'te `pub mod drag;` satırından önce:

```rust
pub mod batch;
```

`crates/gezik-core/src/batch/mod.rs`:

```rust
//! Batch operations' pure parts: rename rules, name templates, dates and letter case. The
//! engine that applies them is the `gezik-batch` crate.

pub mod case;
pub mod date;
pub mod rules;
pub mod template;
```

- [ ] **Step 2: `date.rs`'in testlerini yaz**

`crates/gezik-core/src/batch/date.rs`:

```rust
//! Dates in new names: a local date and time, and the strftime subset templates use.

/// A local date and time, already in the user's time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateParts {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// The letters after `%` a format may use.
const LETTERS: &str = "YymdHMSjbB%";

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// Whether `format` uses only `%Y %y %m %d %H %M %S %j %b %B %%`; the error names the bad part.
pub fn check_format(format: &str) -> Result<(), String> {
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        match chars.next() {
            Some(letter) if LETTERS.contains(letter) => {}
            Some(letter) => return Err(format!("unknown date part %{letter}")),
            None => return Err("a date format cannot end with %".to_owned()),
        }
    }
    Ok(())
}

fn day_of_year(date: &DateParts) -> u32 {
    const BEFORE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (date.year % 4 == 0 && date.year % 100 != 0) || date.year % 400 == 0;
    let month = usize::from(date.month.clamp(1, 12)) - 1;
    BEFORE[month] + u32::from(date.day) + u32::from(leap && month >= 2)
}

/// `date` written with `format` (checked with [`check_format`] first; an unknown part is
/// written as it is).
pub fn format(format: &str, date: &DateParts) -> String {
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&format!("{:04}", date.year)),
            Some('y') => out.push_str(&format!("{:02}", date.year.rem_euclid(100))),
            Some('m') => out.push_str(&format!("{:02}", date.month)),
            Some('d') => out.push_str(&format!("{:02}", date.day)),
            Some('H') => out.push_str(&format!("{:02}", date.hour)),
            Some('M') => out.push_str(&format!("{:02}", date.minute)),
            Some('S') => out.push_str(&format!("{:02}", date.second)),
            Some('j') => out.push_str(&format!("{:03}", day_of_year(date))),
            Some('B') => out.push_str(MONTHS[usize::from(date.month.clamp(1, 12)) - 1]),
            Some('b') => out.push_str(&MONTHS[usize::from(date.month.clamp(1, 12)) - 1][..3]),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> DateParts {
        DateParts { year: 2024, month: 3, day: 9, hour: 7, minute: 5, second: 2 }
    }

    #[test]
    fn writes_each_part() {
        assert_eq!(format("%Y-%m-%d %H.%M.%S", &date()), "2024-03-09 07.05.02");
        assert_eq!(format("%y%j %b %B 100%%", &date()), "24069 Mar March 100%");
    }

    #[test]
    fn checks_formats() {
        assert!(check_format("%Y-%m-%d_%H%M").is_ok());
        assert_eq!(check_format("%Q").unwrap_err(), "unknown date part %Q");
        assert!(check_format("50%").is_err());
    }

    #[test]
    fn day_of_year_counts_leap_days() {
        assert_eq!(day_of_year(&DateParts { year: 2023, month: 3, day: 1, ..date() }), 60);
        assert_eq!(day_of_year(&DateParts { year: 2024, month: 3, day: 1, ..date() }), 61);
        assert_eq!(day_of_year(&DateParts { year: 1900, month: 3, day: 1, ..date() }), 60);
    }
}
```

- [ ] **Step 3: `case.rs`'i yaz**

`crates/gezik-core/src/batch/case.rs`:

```rust
//! Changing the letter case of names, with Turkish and Azerbaijani i/İ and ı/I.

/// Which letter rules to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// Turkish and Azerbaijani: i ↔ İ, ı ↔ I.
    Turkic,
    #[default]
    Other,
}

impl Lang {
    /// From a language code (`tr`, `tr-TR`, `az_AZ.UTF-8`, `en-US`).
    pub fn from_code(code: &str) -> Lang {
        let lang = code.split(['-', '_', '.']).next().unwrap_or_default().to_ascii_lowercase();
        if lang == "tr" || lang == "az" { Lang::Turkic } else { Lang::Other }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    Lower,
    Upper,
    /// Every word starts with a capital, the rest is small.
    Title,
    /// Only the first letter is a capital.
    Sentence,
}

impl CaseMode {
    pub const ALL: [CaseMode; 4] = [CaseMode::Lower, CaseMode::Upper, CaseMode::Title, CaseMode::Sentence];

    pub fn as_str(self) -> &'static str {
        match self {
            CaseMode::Lower => "lower",
            CaseMode::Upper => "upper",
            CaseMode::Title => "title",
            CaseMode::Sentence => "sentence",
        }
    }

    pub fn parse(text: &str) -> Option<CaseMode> {
        CaseMode::ALL.into_iter().find(|mode| mode.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            CaseMode::Lower => "lower case",
            CaseMode::Upper => "UPPER CASE",
            CaseMode::Title => "Title Case",
            CaseMode::Sentence => "Sentence case",
        }
    }
}

fn lower(c: char, lang: Lang, out: &mut String) {
    match (c, lang) {
        ('I', Lang::Turkic) => out.push('ı'),
        ('İ', Lang::Turkic) => out.push('i'),
        // Outside Turkish, İ lowercases to i plus a combining dot; a plain i reads better.
        ('İ', Lang::Other) => out.push('i'),
        _ => out.extend(c.to_lowercase()),
    }
}

fn upper(c: char, lang: Lang, out: &mut String) {
    match (c, lang) {
        ('i', Lang::Turkic) => out.push('İ'),
        ('ı', Lang::Turkic) => out.push('I'),
        _ => out.extend(c.to_uppercase()),
    }
}

/// Where a new word starts: after a space, `_`, `-`, `.` or `(`.
fn starts_word(previous: Option<char>) -> bool {
    previous.is_none_or(|p| p.is_whitespace() || matches!(p, '_' | '-' | '.' | '(' | '['))
}

pub fn convert(text: &str, mode: CaseMode, lang: Lang) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous: Option<char> = None;
    let mut first_letter_done = false;
    for c in text.chars() {
        let capital = match mode {
            CaseMode::Lower => false,
            CaseMode::Upper => true,
            CaseMode::Title => starts_word(previous),
            CaseMode::Sentence => !first_letter_done && c.is_alphabetic(),
        };
        if capital { upper(c, lang, &mut out) } else { lower(c, lang, &mut out) }
        if c.is_alphabetic() {
            first_letter_done = true;
        }
        previous = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_dotted_and_dotless_i() {
        assert_eq!(convert("İSTANBUL ILIK", CaseMode::Lower, Lang::Turkic), "istanbul ılık");
        assert_eq!(convert("istanbul ılık", CaseMode::Upper, Lang::Turkic), "İSTANBUL ILIK");
        assert_eq!(convert("istanbul", CaseMode::Upper, Lang::Other), "ISTANBUL");
        assert_eq!(convert("İzmir", CaseMode::Lower, Lang::Other), "izmir");
    }

    #[test]
    fn title_and_sentence() {
        assert_eq!(convert("tatil_foto-ikinci gün", CaseMode::Title, Lang::Turkic), "Tatil_Foto-İkinci Gün");
        assert_eq!(convert("  BÜYÜK harf", CaseMode::Sentence, Lang::Turkic), "  Büyük harf");
        assert_eq!(convert("2024 yaz", CaseMode::Sentence, Lang::Other), "2024 Yaz");
    }

    #[test]
    fn language_codes() {
        assert_eq!(Lang::from_code("tr-TR"), Lang::Turkic);
        assert_eq!(Lang::from_code("az_AZ.UTF-8"), Lang::Turkic);
        assert_eq!(Lang::from_code("en-US"), Lang::Other);
        assert_eq!(Lang::from_code(""), Lang::Other);
    }
}
```

- [ ] **Step 4: `template.rs`'i yaz**

`crates/gezik-core/src/batch/template.rs`:

```rust
//! Name templates: text with fields in braces, `{name} {n:03}` or `{taken:%Y-%m-%d}`.

use super::date::check_format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// The name so far (without its extension unless the extension is included).
    Name,
    /// The extension without its dot.
    Ext,
    /// The counter, at least `width` digits (zero-padded).
    Number { width: u8 },
    /// The name of the folder the item is in.
    Parent,
    /// The modified date, in this format.
    Modified(String),
    /// When the photo was taken (EXIF), else the modified date.
    Taken(String),
    /// The size, readable (`2.1 MB`).
    Size,
}

const DEFAULT_DATE: &str = "%Y-%m-%d";

/// The pieces of `text`; `{{` and `}}` are literal braces. The error names what is wrong.
pub fn parse(text: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '}' => return Err("a } without its {".to_owned()),
            '{' => {
                let mut field = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => field.push(c),
                        None => return Err(format!("{{{field} is not closed with }}")),
                    }
                }
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                pieces.push(field_piece(&field)?);
            }
            c => literal.push(c),
        }
    }
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    Ok(pieces)
}

fn field_piece(field: &str) -> Result<Piece, String> {
    let (key, arg) = match field.split_once(':') {
        Some((key, arg)) => (key.trim(), Some(arg)),
        None => (field.trim(), None),
    };
    let date = |arg: Option<&str>| -> Result<String, String> {
        let format = arg.unwrap_or(DEFAULT_DATE).to_owned();
        check_format(&format)?;
        Ok(format)
    };
    let no_arg = |piece: Piece| match arg {
        Some(_) => Err(format!("{{{key}}} takes no format")),
        None => Ok(piece),
    };
    match key {
        "name" => no_arg(Piece::Name),
        "ext" => no_arg(Piece::Ext),
        "parent" => no_arg(Piece::Parent),
        "size" => no_arg(Piece::Size),
        "n" => {
            let width = match arg {
                None => 1,
                Some(digits) => match digits.trim().parse::<u8>() {
                    Ok(width) if (1..=9).contains(&width) => width,
                    _ => return Err(format!("{{n:{digits}}}: give 1 to 9 digits, like {{n:03}}")),
                },
            };
            Ok(Piece::Number { width })
        }
        "date" => Ok(Piece::Modified(date(arg)?)),
        "taken" => Ok(Piece::Taken(date(arg)?)),
        other => Err(format!("unknown field {{{other}}}")),
    }
}

/// Whether the pieces use the EXIF date (so it must be read).
pub fn uses_taken(pieces: &[Piece]) -> bool {
    pieces.iter().any(|piece| matches!(piece, Piece::Taken(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_text() {
        assert_eq!(
            parse("{taken:%Y} - {name} {n:03}").unwrap(),
            [
                Piece::Taken("%Y".into()),
                Piece::Text(" - ".into()),
                Piece::Name,
                Piece::Text(" ".into()),
                Piece::Number { width: 3 },
            ]
        );
        assert_eq!(parse("{date}").unwrap(), [Piece::Modified("%Y-%m-%d".into())]);
        assert_eq!(parse("a{{b}}").unwrap(), [Piece::Text("a{b}".into())]);
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert_eq!(parse("{nmae}").unwrap_err(), "unknown field {nmae}");
        assert_eq!(parse("{n:0}").unwrap_err(), "{n:0}: give 1 to 9 digits, like {n:03}");
        assert_eq!(parse("{name").unwrap_err(), "{name is not closed with }");
        assert_eq!(parse("{date:%Q}").unwrap_err(), "unknown date part %Q");
        assert_eq!(parse("{ext:x}").unwrap_err(), "{ext} takes no format");
        assert!(parse("a}").is_err());
    }

    #[test]
    fn knows_when_exif_is_needed() {
        assert!(uses_taken(&parse("{taken}").unwrap()));
        assert!(!uses_taken(&parse("{date} {name}").unwrap()));
    }
}
```

- [ ] **Step 5: `rules.rs`'i yaz**

`crates/gezik-core/src/batch/rules.rs`:

```rust
//! Rename rules as data: what the rename layer edits, settings save and `gezik-batch`
//! applies, in order, each to what the one before made.

use super::case::CaseMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEntry {
    /// Unchecked rules are kept but skipped.
    pub enabled: bool,
    pub rule: Rule,
}

impl RuleEntry {
    pub fn new(rule: Rule) -> RuleEntry {
        RuleEntry { enabled: true, rule }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Replace(ReplaceRule),
    Number(NumberRule),
    Case(CaseMode),
    /// Text before and after the name; may use template fields.
    AddText { prefix: String, suffix: String },
    Extension(ExtensionRule),
    /// The whole name from a template.
    Template(String),
    Clean(CleanRule),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplaceRule {
    pub find: String,
    pub with: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Every match, else only the first.
    pub all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberAt {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberRule {
    pub start: i64,
    pub step: i64,
    /// Zero-padded to at least this many digits (1-9).
    pub digits: u8,
    pub at: NumberAt,
    /// Between the number and the name.
    pub separator: String,
    /// Counting starts again in each folder.
    pub per_folder: bool,
}

impl Default for NumberRule {
    fn default() -> Self {
        NumberRule { start: 1, step: 1, digits: 3, at: NumberAt::End, separator: " ".to_owned(), per_folder: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionRule {
    /// A new extension, without the dot (`jpg`); empty: none.
    Set(String),
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CleanRule {
    /// Spaces at the start and end go.
    pub trim: bool,
    /// Runs of spaces become one.
    pub collapse_spaces: bool,
    /// `_`, `.` and `-` become spaces.
    pub separators_to_spaces: bool,
    /// Spaces become `_`.
    pub spaces_to_underscores: bool,
}

/// The kinds the "Add rule" menu offers, in its order: (name in files, label).
pub const KINDS: [(&str, &str); 7] = [
    ("replace", "Replace text"),
    ("number", "Number"),
    ("case", "Change case"),
    ("add-text", "Add text"),
    ("extension", "Extension"),
    ("template", "Name from template"),
    ("clean", "Clean up"),
];

impl Rule {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Rule::Replace(_) => "replace",
            Rule::Number(_) => "number",
            Rule::Case(_) => "case",
            Rule::AddText { .. } => "add-text",
            Rule::Extension(_) => "extension",
            Rule::Template(_) => "template",
            Rule::Clean(_) => "clean",
        }
    }

    /// A new rule of `kind` (a name in [`KINDS`]) as the "Add rule" menu makes it.
    pub fn default_of(kind: &str) -> Option<Rule> {
        Some(match kind {
            "replace" => Rule::Replace(ReplaceRule { all: true, ..ReplaceRule::default() }),
            "number" => Rule::Number(NumberRule::default()),
            "case" => Rule::Case(CaseMode::Lower),
            "add-text" => Rule::AddText { prefix: String::new(), suffix: String::new() },
            "extension" => Rule::Extension(ExtensionRule::Lower),
            "template" => Rule::Template("{name}".to_owned()),
            "clean" => Rule::Clean(CleanRule { trim: true, collapse_spaces: true, ..CleanRule::default() }),
            _ => return None,
        })
    }

    /// One line for the rule list: `Replace "IMG_" → "Tatil "`.
    pub fn summary(&self) -> String {
        match self {
            Rule::Replace(r) => {
                let how = if r.regex { " (regex)" } else { "" };
                format!("Replace \"{}\" → \"{}\"{how}", r.find, r.with)
            }
            Rule::Number(n) => {
                let sample = format!("{:0width$}", n.start, width = usize::from(n.digits.clamp(1, 9)));
                let at = match n.at {
                    NumberAt::Start => "at start",
                    NumberAt::End => "at end",
                };
                format!("Number {sample}, {at}")
            }
            Rule::Case(mode) => format!("Case: {}", mode.label()),
            Rule::AddText { prefix, suffix } => format!("Add \"{prefix}\" … \"{suffix}\""),
            Rule::Extension(ExtensionRule::Set(ext)) if ext.is_empty() => "Remove extension".to_owned(),
            Rule::Extension(ExtensionRule::Set(ext)) => format!("Extension: .{ext}"),
            Rule::Extension(ExtensionRule::Lower) => "Extension: lower case".to_owned(),
            Rule::Extension(ExtensionRule::Upper) => "Extension: UPPER CASE".to_owned(),
            Rule::Template(text) => format!("Name: {text}"),
            Rule::Clean(_) => "Clean up spaces and separators".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_default_and_a_summary() {
        for (kind, _) in KINDS {
            let rule = Rule::default_of(kind).unwrap();
            assert_eq!(rule.kind_name(), kind);
            assert!(!rule.summary().is_empty());
        }
        assert!(Rule::default_of("nope").is_none());
    }

    #[test]
    fn summaries() {
        let replace = Rule::Replace(ReplaceRule { find: "IMG_".into(), with: "Tatil ".into(), ..Default::default() });
        assert_eq!(replace.summary(), "Replace \"IMG_\" → \"Tatil \"");
        assert_eq!(Rule::Number(NumberRule::default()).summary(), "Number 001, at end");
        assert_eq!(Rule::Extension(ExtensionRule::Set(String::new())).summary(), "Remove extension");
    }
}
```

- [ ] **Step 6: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-core batch`
Expected: PASS (date, case, template, rules testleri)

- [ ] **Step 7: Denetimler ve commit**

```bash
~/.cargo/bin/cargo clippy -p gezik-core --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-core
git commit -m "Add the pure parts of batch rename: rules, templates, dates and letter case"
```

---

### Task 2: `gezik-core::ops::renames` — yeniden adlandırma sırası

**Files:**
- Create: `crates/gezik-core/src/ops/renames.rs`
- Modify: `crates/gezik-core/src/ops/mod.rs` (`pub mod renames;`), `crates/gezik-core/src/ops/paths.rs` (`path_key`)

**Interfaces:**
- Produces:
  - `gezik_core::ops::paths::path_key(&Path) -> Vec<String>` (`same_path`'in karşılaştırdığı anahtar)
  - `gezik_core::ops::renames::{Step, order(pairs: &[(PathBuf, PathBuf)]) -> Vec<Step>}`; `Step::{Direct(usize), ToTemp(usize), FromTemp(usize)}`

- [ ] **Step 1: `path_key`'i dışa aç**

`crates/gezik-core/src/ops/paths.rs`'te `fn parts` imzasını şu yap ve üstüne yorum ekle:

```rust
/// What `same_path` compares: the parts, in lower case where the file system ignores case.
pub fn path_key(path: &Path) -> Vec<String> {
```

Dosyadaki diğer `parts(` çağrılarını `path_key(` ile değiştir (`same_path`, `is_within`, `cover`).

- [ ] **Step 2: Başarısız testleri yaz**

`crates/gezik-core/src/ops/renames.rs`:

```rust
//! In which order a set of renames runs so none lands on a name another still holds:
//! chains go from their free end, each cycle (a → b, b → a) goes through one temporary name,
//! and a rename that only changes letter case (where the file system ignores case) always does.

use std::collections::HashMap;
use std::path::PathBuf;

use super::paths::path_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Rename pair `i` straight to its target.
    Direct(usize),
    /// Rename pair `i` to a temporary name (its source is then free).
    ToTemp(usize),
    /// Rename pair `i` from its temporary name to its target.
    FromTemp(usize),
}

/// The steps for `pairs` (source, target). Pairs whose source and target are the same text
/// are left out. Targets must be distinct (the rename layer refuses duplicates).
pub fn order(pairs: &[(PathBuf, PathBuf)]) -> Vec<Step> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(PathBuf, PathBuf)> {
        list.iter().map(|(a, b)| (PathBuf::from(a), PathBuf::from(b))).collect()
    }

    /// Runs `steps` on a set of names; panics if a step lands on a held name.
    fn simulate(pairs: &[(PathBuf, PathBuf)], steps: &[Step]) -> Vec<String> {
        let mut held: Vec<Vec<String>> = pairs.iter().map(|(s, _)| path_key(s)).collect();
        let mut temp = vec![false; pairs.len()];
        let others = |held: &[Vec<String>], i: usize, key: &Vec<String>| {
            held.iter().enumerate().any(|(j, h)| j != i && h == key)
        };
        for step in steps {
            match *step {
                Step::Direct(i) | Step::FromTemp(i) => {
                    let target = path_key(&pairs[i].1);
                    assert!(!others(&held, i, &target), "{step:?} lands on a held name");
                    assert_eq!(matches!(step, Step::FromTemp(_)), temp[i]);
                    held[i] = target;
                    temp[i] = false;
                }
                Step::ToTemp(i) => {
                    held[i] = vec![format!("temp{i}")];
                    temp[i] = true;
                }
            }
        }
        assert!(temp.iter().all(|t| !t), "something stayed under a temporary name");
        held.iter().map(|k| k.join("/")).collect()
    }

    #[test]
    fn independent_renames_go_straight() {
        let p = pairs(&[("/d/a", "/d/x"), ("/d/b", "/d/y")]);
        let steps = order(&p);
        assert_eq!(steps, [Step::Direct(0), Step::Direct(1)]);
        simulate(&p, &steps);
    }

    #[test]
    fn a_chain_starts_at_its_free_end() {
        // a → b, b → c: b must leave first.
        let p = pairs(&[("/d/a", "/d/b"), ("/d/b", "/d/c")]);
        let steps = order(&p);
        assert_eq!(steps, [Step::Direct(1), Step::Direct(0)]);
        simulate(&p, &steps);
    }

    #[test]
    fn a_swap_goes_through_one_temporary_name() {
        let p = pairs(&[("/d/a", "/d/b"), ("/d/b", "/d/a")]);
        let steps = order(&p);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::ToTemp(_))).count(), 1);
        simulate(&p, &steps);
    }

    #[test]
    fn a_long_cycle_and_a_chain_into_it() {
        // a → b → c → a, and d → e (free).
        let p = pairs(&[("/d/a", "/d/b"), ("/d/b", "/d/c"), ("/d/c", "/d/a"), ("/d/d", "/d/e")]);
        let steps = order(&p);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::ToTemp(_))).count(), 1);
        simulate(&p, &steps);
    }

    #[test]
    fn same_text_is_left_out() {
        assert!(order(&pairs(&[("/d/a", "/d/a")])).is_empty());
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_only_goes_through_a_temporary_name() {
        let p = pairs(&[("/d/foto.JPG", "/d/foto.jpg")]);
        assert_eq!(order(&p), [Step::ToTemp(0), Step::FromTemp(0)]);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_insensitive_cycle() {
        // a.txt → B.txt while b.txt → a.txt: on these systems that is a swap.
        let p = pairs(&[("/d/a.txt", "/d/B.txt"), ("/d/b.txt", "/d/a.txt")]);
        let steps = order(&p);
        assert_eq!(steps.iter().filter(|s| matches!(s, Step::ToTemp(_))).count(), 1);
        simulate(&p, &steps);
    }
}
```

- [ ] **Step 3: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-core renames`
Expected: FAIL (`not yet implemented`)

- [ ] **Step 4: `order`'ı uygula**

`todo!()` yerine:

```rust
pub fn order(pairs: &[(PathBuf, PathBuf)]) -> Vec<Step> {
    let sources: Vec<Vec<String>> = pairs.iter().map(|(s, _)| path_key(s)).collect();
    let targets: Vec<Vec<String>> = pairs.iter().map(|(_, t)| path_key(t)).collect();
    let mut steps = Vec::new();
    let mut pending = vec![false; pairs.len()];
    // Which pending pair still holds each name (by its source).
    let mut holder: HashMap<&[String], usize> = HashMap::new();
    for (i, (source, target)) in pairs.iter().enumerate() {
        if source.as_os_str() == target.as_os_str() {
            continue;
        }
        if sources[i] == targets[i] {
            // Only the letter case changes: through a temporary name, nothing waits on it.
            steps.push(Step::ToTemp(i));
            steps.push(Step::FromTemp(i));
            continue;
        }
        pending[i] = true;
        holder.insert(&sources[i], i);
    }
    // Which pending pair wants each name (targets are distinct).
    let waiting: HashMap<&[String], usize> =
        (0..pairs.len()).filter(|&i| pending[i]).map(|i| (targets[i].as_slice(), i)).collect();
    // Pairs whose source went to a temporary name and that still wait for their target.
    let mut in_temp = vec![false; pairs.len()];
    let mut ready: Vec<usize> = (0..pairs.len())
        .filter(|&i| pending[i] && holder.get(targets[i].as_slice()).is_none_or(|&j| j == i))
        .collect();
    loop {
        // Everything whose target is free goes; each that goes may free the one waiting for it.
        while let Some(i) = ready.pop() {
            if !pending[i] {
                continue;
            }
            pending[i] = false;
            steps.push(if in_temp[i] { Step::FromTemp(i) } else { Step::Direct(i) });
            if !in_temp[i] {
                holder.remove(sources[i].as_slice());
                if let Some(&next) = waiting.get(sources[i].as_slice())
                    && pending[next]
                {
                    ready.push(next);
                }
            }
        }
        // What is left is in cycles: one of each leaves for a temporary name, which frees
        // the pair waiting for its name; the cycle then unwinds back to it.
        let Some(first) = (0..pairs.len()).find(|&i| pending[i] && !in_temp[i]) else { break };
        steps.push(Step::ToTemp(first));
        in_temp[first] = true;
        holder.remove(sources[first].as_slice());
        if let Some(&next) = waiting.get(sources[first].as_slice())
            && pending[next]
        {
            ready.push(next);
        }
        // `first` itself goes once its target is free: when the pair holding it has gone.
        let blocker = holder.get(targets[first].as_slice()).copied();
        if blocker.is_none() {
            ready.push(first);
        }
    }
    steps
}
```

Not: döngüdeki son çift (`first`'ün hedefini tutan) gidince `waiting.get(sources[that])` `first`'ü verir ve `first` `FromTemp` olarak kuyruğa girer (`in_temp[first]` doğru olduğu için `pending[first]` hâlâ true). `(0..n).find` yalnız döngü başına bir kez çalışır.

- [ ] **Step 5: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-core ops::`
Expected: PASS (renames testleri ve var olan paths testleri)

- [ ] **Step 6: Denetimler ve commit**

```bash
~/.cargo/bin/cargo clippy -p gezik-core --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-core
git commit -m "Order a set of renames so chains and cycles never land on a held name"
```

---

### Task 3: `RenameTask` — motorda toplu yeniden adlandırma ve geri alma

**Files:**
- Create: `crates/gezik-ops/src/tasks/rename.rs`
- Modify: `crates/gezik-ops/src/tasks/mod.rs`, `crates/gezik-ops/src/tasks/move_.rs` (`MoveTask::rename` ve testi kalkar), `crates/gezik-ops/src/inverse.rs`, `crates/gezik-ops/src/task.rs`, `crates/gezik-ops/src/lib.rs`, `crates/gezik/src/operations.rs:464`

**Interfaces:**
- Consumes: `gezik_core::ops::renames::{order, Step}` (Task 2)
- Produces:
  - `gezik_ops::RenameTask::{one(path: PathBuf, name: &str) -> RenameTask, many(pairs: Vec<(PathBuf, PathBuf)>) -> RenameTask}`; `pub(crate) fn back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> RenameTask`
  - `TaskKind::Rename.label(n)`: 1 → "Rename", n → "Rename n items"
  - `RunCx::note_temp(&self, temp: &Path, original: &Path)`, `RunCx::forget_temp(&self, temp: &Path)`

- [ ] **Step 1: `RunCx`'e geçici ad kaydını ekle**

`crates/gezik-ops/src/task.rs`'te `impl RunCx<'_>` içine, `has_trash`'ten sonra:

```rust
    /// `temp` holds `original` for a moment (a rename through a temporary name): if Gezik
    /// stops before [`forget_temp`](Self::forget_temp), the next start puts it back.
    pub fn note_temp(&self, temp: &Path, original: &Path) {
        if let Some(pending) = &self.temp.pending {
            pending.add_restore(&crate::pending::Restore {
                hidden: temp.to_path_buf(),
                original: original.to_path_buf(),
                was_hidden: true,
            });
        }
    }

    pub fn forget_temp(&self, temp: &Path) {
        if let Some(pending) = &self.temp.pending {
            pending.remove_restore(temp);
        }
    }
```

Aynı dosyada `TaskKind::label`'ı değiştir:

```rust
    pub fn label(self, count: usize) -> String {
        let verb = self.verb();
        match self {
            TaskKind::NewFolder | TaskKind::NewFile => verb.to_owned(),
            TaskKind::Rename if count <= 1 => verb.to_owned(),
            _ if count == 1 => format!("{verb} 1 item"),
            _ => format!("{verb} {count} items"),
        }
    }
```

ve `labels_count_items` testine `assert_eq!(TaskKind::Rename.label(24), "Rename 24 items");` ekle.

- [ ] **Step 2: Başarısız testlerle `rename.rs`'i yaz**

`crates/gezik-ops/src/tasks/rename.rs`:

```rust
//! Renaming in place, one item or many: in an order that never lands on a name another item
//! still holds (`gezik_core::ops::renames`), cycles through a temporary name each.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::path_key;
use gezik_core::ops::renames::{Step, order};
use gezik_platform::fs;

use super::name;
use crate::task::{
    Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since, facts_after, unchanged,
};
use crate::walk::facts_of;

const DIRECT: u8 = 0;
const TO_TEMP: u8 = 1;
const FROM_TEMP: u8 = 2;

pub struct RenameTask {
    /// (where it is, its new path) per item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// How each item must still look (undo); `None`: anything.
    expect: Vec<Option<Facts>>,
    /// The temporary name of each pair that needs one (set while planning).
    temps: Vec<PathBuf>,
}

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

impl RenameTask {
    /// Renames `path` to `name` in its folder.
    pub fn one(path: PathBuf, name: &str) -> RenameTask {
        let target = path.with_file_name(name);
        RenameTask::many(vec![(path, target)])
    }

    /// Renames each (path, new path); new paths must be distinct.
    pub fn many(pairs: Vec<(PathBuf, PathBuf)>) -> RenameTask {
        let expect = vec![None; pairs.len()];
        RenameTask::new(pairs, expect)
    }

    /// Undo: each (where it is now, its old path, how it must look).
    pub(crate) fn back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> RenameTask {
        let (pairs, expect) = items.into_iter().map(|(now, was, facts)| ((now, was), facts)).unzip();
        RenameTask::new(pairs, expect)
    }

    fn new(pairs: Vec<(PathBuf, PathBuf)>, expect: Vec<Option<Facts>>) -> RenameTask {
        let temps = pairs
            .iter()
            .map(|(source, _)| {
                let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
                source.with_file_name(format!(".gezik-rn-{}-{n}", std::process::id()))
            })
            .collect();
        RenameTask { pairs, expect, temps }
    }

    fn index(item: &PlanItem) -> usize {
        item.root
    }
}

impl Task for RenameTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Rename
    }

    fn title(&self) -> String {
        match self.pairs.as_slice() {
            [(from, to)] => format!("Renaming {} to {}", name(from), name(to)),
            pairs => {
                let dir = pairs
                    .first()
                    .and_then(|(from, _)| from.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                format!("Renaming {} items in {dir}", pairs.len())
            }
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let paths = self.pairs.iter().filter_map(|(source, _)| source.parent().map(Path::to_path_buf)).collect();
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // Names this job frees before it needs them: no conflict for those.
        let freed: std::collections::HashSet<Vec<String>> =
            self.pairs.iter().map(|(source, _)| path_key(source)).collect();
        // Before: run one by one, in this order, on the planning thread.
        for step in order(&self.pairs) {
            let (i, tag) = match step {
                Step::Direct(i) => (i, DIRECT),
                Step::ToTemp(i) => (i, TO_TEMP),
                Step::FromTemp(i) => (i, FROM_TEMP),
            };
            let (source, target) = &self.pairs[i];
            if super::refuse_root(sink, source, "rename") {
                continue;
            }
            let facts = match std::fs::symlink_metadata(source) {
                Ok(meta) => facts_of(&meta),
                // The temporary name is gone already only if an earlier step failed: skip.
                Err(_) if tag == FROM_TEMP => continue,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let mut item = PlanItem::new(Stage::Before, facts).source(source).target(target).top(i).tag(tag);
            // A target held by something outside this job is a real conflict.
            if tag != TO_TEMP && !freed.contains(&path_key(target)) {
                // The engine takes a Before folder onto an existing folder as a merge (and would
                // do nothing): a rename never merges, it says why instead.
                if (facts.is_dir || tag == FROM_TEMP)
                    && std::fs::symlink_metadata(target).is_ok_and(|meta| meta.is_dir())
                {
                    sink.failed(source, io::Error::new(io::ErrorKind::AlreadyExists, "A folder with this name already exists"));
                    continue;
                }
                item = item.checked();
            }
            if !sink.item(item) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let i = Self::index(item);
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        let temp = &self.temps[i];
        match item.tag {
            TO_TEMP => {
                if !unchanged(source, self.expect.get(i).copied().flatten()) {
                    return Err(changed_since());
                }
                cx.note_temp(temp, source);
                if let Err(err) = fs::move_entry(source, temp) {
                    cx.forget_temp(temp);
                    return Err(err);
                }
                // Nothing to undo yet: the item counts once it reaches its name.
                Ok(Outcome::Nothing)
            }
            FROM_TEMP => {
                if std::fs::symlink_metadata(temp).is_err() {
                    return Ok(Outcome::Nothing);
                }
                match fs::move_entry(temp, target) {
                    Ok(()) => {
                        cx.forget_temp(temp);
                        Ok(Outcome::Moved {
                            from: source.clone(),
                            to: target.clone(),
                            facts: facts_after(target, item.facts.is_dir),
                        })
                    }
                    Err(err) => {
                        // Back under its own name if it can; else the note puts it back later.
                        if fs::move_entry(temp, source).is_ok() {
                            cx.forget_temp(temp);
                        }
                        Err(err)
                    }
                }
            }
            _ => {
                if !unchanged(source, self.expect.get(i).copied().flatten()) {
                    return Err(changed_since());
                }
                fs::move_entry(source, target)?;
                Ok(Outcome::Moved {
                    from: source.clone(),
                    to: target.clone(),
                    facts: facts_after(target, item.facts.is_dir),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};
    use gezik_core::ops::conflict::Decision;

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn many_names_change_and_one_undo_brings_them_back() {
        let dir = test_dir("rename-many");
        for n in 1..=3 {
            write(&dir.join(format!("IMG_{n}.jpg")), &n.to_string());
        }
        let pairs = (1..=3).map(|n| (dir.join(format!("IMG_{n}.jpg")), dir.join(format!("Tatil {n}.jpg")))).collect();
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(RenameTask::many(pairs))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&dir), ["Tatil 1.jpg", "Tatil 2.jpg", "Tatil 3.jpg"]);
        assert_eq!(engine.undo_label().as_deref(), Some("Rename 3 items"));
        let (report, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&dir), ["IMG_1.jpg", "IMG_2.jpg", "IMG_3.jpg"]);
        assert_eq!(read(&dir.join("IMG_2.jpg")), "2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_a_swap_swaps_back() {
        let dir = test_dir("rename-swap");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let swap = vec![(dir.join("a.txt"), dir.join("b.txt")), (dir.join("b.txt"), dir.join("a.txt"))];
        let (report, _) = finish(&engine, engine.submit(Box::new(RenameTask::many(swap))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("b".into(), "a".into()));
        let (report, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        assert_eq!(names(&dir), ["a.txt", "b.txt"], "no temporary name is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_rename_also_only_its_case() {
        let dir = test_dir("rename-one");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "b.txt"))), no_conflicts);
        let (report, _) =
            finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("b.txt"), "B.txt"))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&dir), ["B.txt"]);
        assert_eq!(engine.undo_label().as_deref(), Some("Rename"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_held_outside_the_job_asks() {
        let dir = test_dir("rename-taken");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let (_, events) =
            finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "b.txt"))), defaults);
        assert!(events.iter().any(|e| matches!(e, crate::Event::Conflicts { .. })));
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_never_merges_into_a_folder_of_its_new_name() {
        let dir = test_dir("rename-folder-taken");
        write(&dir.join("a/x.txt"), "x");
        write(&dir.join("b/y.txt"), "y");
        let engine = engine();
        let (report, _) =
            finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a"), "b"))), no_conflicts);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(read(&dir.join("a/x.txt")), "x");
        assert!(!dir.join("b/x.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_second_step_leaves_a_restore_note() {
        let dir = test_dir("rename-note");
        write(&dir.join("a.txt"), "a");
        let pending = std::sync::Arc::new(crate::pending::PendingDeletes::new(dir.join("pending-deletes")));
        let temp = crate::task::TempCopies::new(Some(pending.clone()));
        let control = crate::control::Control::default();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: std::cell::Cell::new(0), temp: &temp };
        let task = RenameTask::many(vec![(dir.join("a.txt"), dir.join("x/b.txt"))]);
        let facts = facts_after(&dir.join("a.txt"), false);
        let to_temp =
            PlanItem::new(Stage::Before, facts).source(dir.join("a.txt")).target(dir.join("x/b.txt")).top(0).tag(TO_TEMP);
        task.run(&to_temp, &cx).unwrap();
        assert_eq!(pending.restores().len(), 1, "the temporary name is noted");
        // The target's folder does not exist: the second step fails and the item goes back.
        let from_temp = to_temp.clone().tag(FROM_TEMP);
        assert!(task.run(&from_temp, &cx).is_err());
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert!(pending.restores().is_empty(), "back under its name: the note is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

`TempCopies::new`, `Control::default` ve `RunCx`'in alanları `pub(crate)`: test aynı crate'te olduğu için erişilebilir (`TempCopies` için `crate::task::TempCopies` yolu `pub(crate) struct` ise yeterli).

- [ ] **Step 3: Görevi kaydet ve `MoveTask::rename`'i kaldır**

`crates/gezik-ops/src/tasks/mod.rs`: `mod rename;` ve `pub use rename::RenameTask;` ekle; `a_drive_root_is_never_planned` testine `refused(&RenameTask::one(root.clone(), "x"), &root);` ekle.

`crates/gezik-ops/src/lib.rs`'te `pub use tasks::{...}` listesine `RenameTask` ekle.

`crates/gezik-ops/src/tasks/move_.rs`: `pub fn rename(...)` fonksiyonunu ve `rename_changes_the_name_and_also_only_its_case` ile `renaming_onto_a_taken_name_asks` testlerini sil (yerlerini `rename.rs`'in testleri aldı). `title()` içindeki `(TaskKind::Rename, Some((from, to)))` kolu kalkar. `kind` alanı artık hep `TaskKind::Move` olduğu için alanı silip `fn kind` içinde `TaskKind::Move` döndür.

`crates/gezik/src/operations.rs:464`: `gezik_ops::MoveTask::rename(path, &name)` → `gezik_ops::RenameTask::one(path, &name)`.

- [ ] **Step 4: Geri almada aynı klasördeki taşımaları `RenameTask::back`'e ver**

`crates/gezik-ops/src/inverse.rs`'te `use crate::tasks::{MoveTask, RestoreTask, TrashTask};` → `use crate::tasks::{MoveTask, RenameTask, RestoreTask, TrashTask};` ve `moved` görevini kuran bloğu şöyle değiştir:

```rust
    let moved = cover(moved, |(path, _, _)| path);
    // Renames in one folder may swap names: they go back in an order that never collides.
    let (renamed, moved): (Vec<_>, Vec<_>) =
        moved.into_iter().partition(|(now, was, _)| now.parent().is_some() && now.parent() == was.parent());
    if !renamed.is_empty() {
        tasks.push(Arc::new(RenameTask::back(renamed)));
    }
    if !moved.is_empty() {
        tasks.push(Arc::new(MoveTask::back(moved)));
    }
```

`made_moved_and_trashed_each_get_their_inverse_in_order` testindeki `Moved` çifti farklı klasörlerde (`/a/x` → `/b/x`) olduğu için beklenen sıra değişmez. Yeni bir test ekle:

```rust
    #[test]
    fn renames_in_one_folder_undo_as_renames() {
        let outcomes = vec![
            Outcome::Moved { from: "/d/a".into(), to: "/d/b".into(), facts: file() },
            Outcome::Moved { from: "/d/b".into(), to: "/d/a".into(), facts: file() },
        ];
        let tasks = build(&outcomes);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind(), TaskKind::Rename);
        assert_eq!(tasks[0].count(), 2);
    }
```

- [ ] **Step 5: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-ops`
Expected: PASS (rename testleri dahil; move ve engine testleri değişmeden)

- [ ] **Step 6: Tüm çalışma alanı, denetimler, commit**

```bash
~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-ops crates/gezik/src/operations.rs
git commit -m "Rename many items in one job, through temporary names where they swap, and undo them the same way"
```

---

### Task 4: `gezik-batch` crate'i — kural motoru, denetim, EXIF

**Files:**
- Create: `crates/gezik-batch/Cargo.toml`, `crates/gezik-batch/src/lib.rs`, `crates/gezik-batch/src/rename.rs`, `crates/gezik-batch/src/exif.rs`, `crates/gezik-batch/tests/data/exif.jpg` (Step 6'da üretilir)
- Modify: kök `Cargo.toml` (`members`, `[workspace.dependencies]`)

**Interfaces:**
- Consumes: Task 1 türleri; `gezik_core::ops::names::{split_name, validate_name, NameRules, NameError}`; `gezik_core::format_size`
- Produces:
  - `gezik_batch::rename::{Item, Compiled, compile(&[RuleEntry]) -> Compiled, Compiled::errors() -> &[Option<String>], Compiled::ok() -> bool, Compiled::needs_taken() -> bool, new_names(&[Item], &Compiled, include_extension: bool, Lang) -> Vec<String>, Status, check(&[Item], &[String], existing: &dyn Fn(usize, &str) -> bool, NameRules, ignore_case: bool) -> Vec<Status>}`
  - `gezik_batch::exif::taken(&Path) -> Option<DateParts>`

- [ ] **Step 1: Crate'i kur**

Kök `Cargo.toml`'da `members` listesine `"crates/gezik-batch"` ekle (`gezik-ops`'tan sonra); `[workspace.dependencies]`'e `gezik-batch = { path = "crates/gezik-batch" }`.

`crates/gezik-batch/Cargo.toml`:

```toml
[package]
name = "gezik-batch"
description = "Batch operations of the Gezik file manager: rename rules, and later archives and conversion"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
gezik-core.workspace = true
# Unicode \w and case-insensitive matching, no other Unicode tables (size).
regex = { version = "1", default-features = false, features = ["std", "perf", "unicode-case", "unicode-perl"] }
kamadak-exif = "0.6"
```

`crates/gezik-batch/src/lib.rs`:

```rust
//! Batch operations of Gezik. 5a: the engine that turns rename rules into new names, and
//! the EXIF date reader. The renames themselves run as `gezik_ops::RenameTask`.

pub mod exif;
pub mod rename;
```

- [ ] **Step 2: Başarısız testlerle `rename.rs`'i yaz**

`crates/gezik-batch/src/rename.rs`:

```rust
//! Applies rename rules to names and checks the result.

use regex::{Regex, RegexBuilder};

use gezik_core::batch::case::{Lang, convert};
use gezik_core::batch::date::{DateParts, format as format_date};
use gezik_core::batch::rules::{CleanRule, ExtensionRule, NumberAt, NumberRule, Rule, RuleEntry};
use gezik_core::batch::template::{Piece, parse as parse_template, uses_taken};
use gezik_core::format_size;
use gezik_core::ops::names::{NameError, NameRules, split_name, validate_name};

/// One item to rename, as the layer knows it.
#[derive(Debug, Clone, Default)]
pub struct Item {
    pub name: String,
    pub is_dir: bool,
    /// The name of the folder it is in.
    pub parent: String,
    /// Which folder it is in (items in one folder share it): counting per folder, duplicates.
    pub folder: usize,
    pub size: u64,
    pub modified: Option<DateParts>,
    /// From EXIF, once read.
    pub taken: Option<DateParts>,
}

enum Ready {
    Replace { regex: Regex, with: String, all: bool },
    Number(NumberRule),
    Case(gezik_core::batch::case::CaseMode),
    AddText { prefix: Vec<Piece>, suffix: Vec<Piece> },
    Extension(ExtensionRule),
    Template(Vec<Piece>),
    Clean(CleanRule),
}

/// Rules made ready to apply; a rule that cannot be (a bad regex) has an error and is skipped.
pub struct Compiled {
    ready: Vec<Ready>,
    errors: Vec<Option<String>>,
    taken: bool,
}

impl Compiled {
    /// Per rule given to [`compile`]: what is wrong with it.
    pub fn errors(&self) -> &[Option<String>] {
        &self.errors
    }

    pub fn ok(&self) -> bool {
        self.errors.iter().all(Option::is_none)
    }

    /// Some rule uses `{taken}`: the EXIF dates must be read.
    pub fn needs_taken(&self) -> bool {
        self.taken
    }
}

pub fn compile(rules: &[RuleEntry]) -> Compiled {
    let mut compiled = Compiled { ready: Vec::new(), errors: Vec::new(), taken: false };
    for entry in rules {
        let ready = if entry.enabled { ready(&entry.rule) } else { Ok(None) };
        match ready {
            Ok(Some((ready, taken))) => {
                compiled.taken |= taken;
                compiled.ready.push(ready);
                compiled.errors.push(None);
            }
            Ok(None) => compiled.errors.push(None),
            Err(error) => compiled.errors.push(Some(error)),
        }
    }
    compiled
}

fn ready(rule: &Rule) -> Result<Option<(Ready, bool)>, String> {
    Ok(Some(match rule {
        Rule::Replace(r) => {
            if r.find.is_empty() {
                return Ok(None);
            }
            let pattern = if r.regex { r.find.clone() } else { regex::escape(&r.find) };
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!r.case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|err| first_line(&err.to_string()))?;
            // Plain text is put in as written: `$` means nothing there.
            let with = if r.regex { r.with.clone() } else { r.with.replace('$', "$$") };
            (Ready::Replace { regex, with, all: r.all }, false)
        }
        Rule::Number(n) => (Ready::Number(n.clone()), false),
        Rule::Case(mode) => (Ready::Case(*mode), false),
        Rule::AddText { prefix, suffix } => {
            let (prefix, suffix) = (parse_template(prefix)?, parse_template(suffix)?);
            let taken = uses_taken(&prefix) || uses_taken(&suffix);
            (Ready::AddText { prefix, suffix }, taken)
        }
        Rule::Extension(e) => (Ready::Extension(e.clone()), false),
        Rule::Template(text) => {
            let pieces = parse_template(text)?;
            let taken = uses_taken(&pieces);
            (Ready::Template(pieces), taken)
        }
        Rule::Clean(c) => (Ready::Clean(*c), false),
    }))
}

/// The regex crate's errors span lines with a caret drawing: the last line says what is wrong.
fn first_line(text: &str) -> String {
    text.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or(text).trim().trim_start_matches("error: ").to_owned()
}

struct Context<'a> {
    item: &'a Item,
    counter: i64,
}

fn render(pieces: &[Piece], name: &str, ext: &str, cx: &Context<'_>) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Name => out.push_str(name),
            Piece::Ext => out.push_str(ext.trim_start_matches('.')),
            Piece::Number { width } => out.push_str(&number(cx.counter, *width)),
            Piece::Parent => out.push_str(&cx.item.parent),
            Piece::Modified(format) => {
                if let Some(date) = &cx.item.modified {
                    out.push_str(&format_date(format, date));
                }
            }
            Piece::Taken(format) => {
                if let Some(date) = cx.item.taken.as_ref().or(cx.item.modified.as_ref()) {
                    out.push_str(&format_date(format, date));
                }
            }
            Piece::Size => out.push_str(&format_size(cx.item.size)),
        }
    }
    out
}

fn number(n: i64, width: u8) -> String {
    let width = usize::from(width.clamp(1, 9));
    if n < 0 { format!("-{:0width$}", n.unsigned_abs()) } else { format!("{n:0width$}") }
}

fn clean(text: &str, rule: &CleanRule) -> String {
    let mut text: String =
        if rule.separators_to_spaces { text.replace(['_', '.', '-'], " ") } else { text.to_owned() };
    if rule.collapse_spaces {
        let mut out = String::with_capacity(text.len());
        let mut space = false;
        for c in text.chars() {
            if c == ' ' {
                if !space {
                    out.push(c);
                }
                space = true;
            } else {
                out.push(c);
                space = false;
            }
        }
        text = out;
    }
    if rule.trim {
        text = text.trim().to_owned();
    }
    if rule.spaces_to_underscores {
        text = text.replace(' ', "_");
    }
    text
}

/// Each item's new name, in order: `items` in the order the layer shows them.
pub fn new_names(items: &[Item], compiled: &Compiled, include_extension: bool, lang: Lang) -> Vec<String> {
    let mut counters: Vec<(usize, usize)> = Vec::new(); // (folder, items seen there)
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let in_folder = match counters.iter_mut().find(|(folder, _)| *folder == item.folder) {
            Some((_, seen)) => {
                *seen += 1;
                *seen - 1
            }
            None => {
                counters.push((item.folder, 1));
                0
            }
        };
        let mut name = item.name.clone();
        for ready in &compiled.ready {
            let position = |rule: &NumberRule| if rule.per_folder { in_folder } else { index };
            let counter = match ready {
                Ready::Number(rule) => rule.start + rule.step * position(rule) as i64,
                _ => {
                    // `{n}` in templates counts like the first Number rule, else from 1.
                    let rule = compiled.ready.iter().find_map(|r| match r {
                        Ready::Number(rule) => Some(rule),
                        _ => None,
                    });
                    rule.map_or(1 + index as i64, |rule| rule.start + rule.step * position(rule) as i64)
                }
            };
            let cx = Context { item, counter };
            name = apply(ready, &name, item.is_dir, include_extension, lang, &cx);
        }
        out.push(name);
    }
    out
}

fn apply(ready: &Ready, name: &str, is_dir: bool, include_extension: bool, lang: Lang, cx: &Context<'_>) -> String {
    if let Ready::Extension(rule) = ready {
        let (stem, ext) = split_name(name, is_dir);
        if is_dir {
            return name.to_owned();
        }
        return match rule {
            ExtensionRule::Set(new) if new.trim_start_matches('.').is_empty() => stem.to_owned(),
            ExtensionRule::Set(new) => format!("{stem}.{}", new.trim_start_matches('.')),
            ExtensionRule::Lower => format!("{stem}{}", ext.to_lowercase()),
            ExtensionRule::Upper => format!("{stem}{}", ext.to_uppercase()),
        };
    }
    let (stem, ext) = if include_extension { (name, "") } else { split_name(name, is_dir) };
    let (_, real_ext) = split_name(name, is_dir);
    let changed = match ready {
        Ready::Replace { regex, with, all } => {
            if *all { regex.replace_all(stem, with.as_str()).into_owned() } else { regex.replace(stem, with.as_str()).into_owned() }
        }
        Ready::Number(rule) => {
            let n = number(cx.counter, rule.digits);
            match rule.at {
                NumberAt::Start => format!("{n}{}{stem}", rule.separator),
                NumberAt::End => format!("{stem}{}{n}", rule.separator),
            }
        }
        Ready::Case(mode) => convert(stem, *mode, lang),
        Ready::AddText { prefix, suffix } => {
            format!("{}{stem}{}", render(prefix, stem, real_ext, cx), render(suffix, stem, real_ext, cx))
        }
        Ready::Template(pieces) => render(pieces, stem, real_ext, cx),
        Ready::Clean(rule) => clean(stem, rule),
        Ready::Extension(_) => unreachable!("handled above"),
    };
    format!("{changed}{ext}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Changed,
    /// Another item gets the same new name.
    Duplicate,
    /// Something not being renamed already has this name.
    Exists,
    Invalid(NameError),
}

impl Status {
    /// Blocks the Rename button.
    pub fn blocks(&self) -> bool {
        matches!(self, Status::Duplicate | Status::Exists | Status::Invalid(_))
    }
}

/// What each new name means. `existing(folder, name)`: whether something outside the
/// selection in that folder has the name. `ignore_case`: the file system ignores case.
pub fn check(
    items: &[Item],
    new: &[String],
    existing: &dyn Fn(usize, &str) -> bool,
    rules: NameRules,
    ignore_case: bool,
) -> Vec<Status> {
    let key = |folder: usize, name: &str| (folder, if ignore_case { name.to_lowercase() } else { name.to_owned() });
    let mut counts: std::collections::HashMap<(usize, String), usize> = std::collections::HashMap::new();
    for (item, name) in items.iter().zip(new) {
        *counts.entry(key(item.folder, name)).or_default() += 1;
    }
    items
        .iter()
        .zip(new)
        .map(|(item, name)| {
            if let Err(err) = validate_name(name, rules) {
                return Status::Invalid(err);
            }
            if counts.get(&key(item.folder, name)).copied().unwrap_or(0) > 1 {
                return Status::Duplicate;
            }
            if *name == item.name {
                return Status::Unchanged;
            }
            let only_case = ignore_case && name.to_lowercase() == item.name.to_lowercase();
            if !only_case && existing(item.folder, name) {
                return Status::Exists;
            }
            Status::Changed
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::case::CaseMode;
    use gezik_core::batch::rules::ReplaceRule;

    fn items(names: &[&str]) -> Vec<Item> {
        names.iter().map(|n| Item { name: n.to_string(), parent: "Tatil".into(), ..Item::default() }).collect()
    }

    fn run(names: &[&str], rules: Vec<Rule>, include_extension: bool) -> Vec<String> {
        let entries: Vec<RuleEntry> = rules.into_iter().map(RuleEntry::new).collect();
        let compiled = compile(&entries);
        assert!(compiled.ok(), "{:?}", compiled.errors());
        new_names(&items(names), &compiled, include_extension, Lang::Turkic)
    }

    fn replace(find: &str, with: &str, regex: bool) -> Rule {
        Rule::Replace(ReplaceRule { find: find.into(), with: with.into(), regex, case_sensitive: false, all: true })
    }

    #[test]
    fn replace_then_number_then_case() {
        let rules = vec![
            replace("IMG_", "Tatil ", false),
            Rule::Number(NumberRule { digits: 2, separator: " - ".into(), ..NumberRule::default() }),
            Rule::Case(CaseMode::Upper),
        ];
        assert_eq!(run(&["IMG_0012.jpg", "img_0013.JPG"], rules, false), ["TATİL 0012 - 01.jpg", "TATİL 0013 - 02.JPG"]);
    }

    #[test]
    fn regex_groups_and_plain_dollars() {
        assert_eq!(run(&["2024_05_foto.png"], vec![replace(r"(\d+)_(\d+)", "$2-$1", true)], false), ["05-2024_foto.png"]);
        assert_eq!(run(&["a.txt"], vec![replace("a", "$1", false)], false), ["$1.txt"]);
    }

    #[test]
    fn the_extension_stays_unless_included() {
        assert_eq!(run(&["a.b.txt"], vec![replace(".", "_", false)], false), ["a_b.txt"]);
        assert_eq!(run(&["a.b.txt"], vec![replace(".", "_", false)], true), ["a_b_txt"]);
        assert_eq!(run(&["arsiv.tar.gz"], vec![Rule::Case(CaseMode::Upper)], false), ["ARSİV.tar.gz"]);
    }

    #[test]
    fn extension_rules() {
        let set = |e: &str| Rule::Extension(ExtensionRule::Set(e.into()));
        assert_eq!(run(&["a.JPEG"], vec![set("jpg")], false), ["a.jpg"]);
        assert_eq!(run(&["a.JPEG"], vec![set("")], false), ["a"]);
        assert_eq!(run(&["a.JPEG"], vec![Rule::Extension(ExtensionRule::Lower)], false), ["a.jpeg"]);
    }

    #[test]
    fn templates_and_add_text() {
        let date = DateParts { year: 2024, month: 7, day: 1, hour: 9, minute: 30, second: 0 };
        let mut list = items(&["IMG_1.jpg", "IMG_2.jpg"]);
        list[0].taken = Some(date);
        list[1].modified = Some(DateParts { day: 2, ..date });
        let entries = vec![
            RuleEntry::new(Rule::Template("{taken:%Y-%m-%d} {parent} {n:02}".into())),
            RuleEntry::new(Rule::AddText { prefix: "[".into(), suffix: "] {ext}".into() }),
        ];
        let compiled = compile(&entries);
        assert!(compiled.needs_taken());
        assert_eq!(
            new_names(&list, &compiled, false, Lang::Other),
            ["[2024-07-01 Tatil 01] jpg.jpg", "[2024-07-02 Tatil 02] jpg.jpg"]
        );
    }

    #[test]
    fn numbers_restart_per_folder_when_asked() {
        let mut list = items(&["a", "b", "c"]);
        list[2].folder = 1;
        let rule = NumberRule { per_folder: true, digits: 1, separator: "".into(), ..NumberRule::default() };
        let compiled = compile(&[RuleEntry::new(Rule::Number(rule))]);
        assert_eq!(new_names(&list, &compiled, false, Lang::Other), ["a1", "b2", "c1"]);
    }

    #[test]
    fn clean_up() {
        let rule = Rule::Clean(CleanRule {
            trim: true,
            collapse_spaces: true,
            separators_to_spaces: true,
            spaces_to_underscores: false,
        });
        assert_eq!(run(&["  my__holiday-photo .jpg"], vec![rule], false), ["my holiday photo.jpg"]);
    }

    #[test]
    fn unchecked_rules_are_skipped() {
        let mut entry = RuleEntry::new(Rule::Case(CaseMode::Upper));
        entry.enabled = false;
        let compiled = compile(&[entry]);
        assert_eq!(new_names(&items(&["a.txt"]), &compiled, false, Lang::Other), ["a.txt"]);
    }

    #[test]
    fn a_bad_rule_reports_and_changes_nothing() {
        let entries = vec![
            RuleEntry::new(replace("(", "x", true)),
            RuleEntry::new(Rule::Template("{nope}".into())),
            RuleEntry::new(Rule::Case(CaseMode::Upper)),
        ];
        let compiled = compile(&entries);
        assert!(!compiled.ok());
        assert!(compiled.errors()[0].as_deref().is_some_and(|e| !e.contains('\n')), "{:?}", compiled.errors());
        assert_eq!(compiled.errors()[1].as_deref(), Some("unknown field {nope}"));
        assert_eq!(compiled.errors()[2], None);
        // The good rules still show what they would do.
        assert_eq!(new_names(&items(&["a.txt"]), &compiled, false, Lang::Other), ["A.txt"]);
    }

    #[test]
    fn statuses() {
        let list = items(&["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]);
        let new: Vec<String> = ["x.txt", "x.txt", "taken.txt", "d.txt", "bad/name"].iter().map(|s| s.to_string()).collect();
        let statuses = check(&list, &new, &|_, name| name == "taken.txt", NameRules::Unix, false);
        assert_eq!(statuses[0], Status::Duplicate);
        assert_eq!(statuses[1], Status::Duplicate);
        assert_eq!(statuses[2], Status::Exists);
        assert_eq!(statuses[3], Status::Unchanged);
        assert!(matches!(statuses[4], Status::Invalid(_)));
        assert!(statuses[2].blocks() && !statuses[3].blocks());
    }

    #[test]
    fn duplicates_ignore_case_where_the_system_does() {
        let list = items(&["a.jpg", "b.jpg", "c.jpg"]);
        let new: Vec<String> = ["Foto.jpg", "foto.jpg", "C.jpg"].iter().map(|s| s.to_string()).collect();
        // C.jpg exists as c.jpg: that is the item itself, only its case changes.
        let existing = |_: usize, name: &str| name.eq_ignore_ascii_case("c.jpg");
        let statuses = check(&list, &new, &existing, NameRules::Windows, true);
        assert_eq!(statuses, [Status::Duplicate, Status::Duplicate, Status::Changed]);
        let statuses = check(&list, &new, &|_, _| false, NameRules::Unix, false);
        assert_eq!(statuses, [Status::Changed, Status::Changed, Status::Changed]);
    }

    #[test]
    fn a_thousand_names_are_quick() {
        let names: Vec<String> = (0..1000).map(|n| format!("IMG_{n:04}.jpg")).collect();
        let list: Vec<Item> = names.iter().map(|n| Item { name: n.clone(), ..Item::default() }).collect();
        let entries = vec![
            RuleEntry::new(replace(r"IMG_(\d+)", "Tatil $1", true)),
            RuleEntry::new(Rule::Number(NumberRule::default())),
            RuleEntry::new(Rule::Case(CaseMode::Title)),
        ];
        let start = std::time::Instant::now();
        let compiled = compile(&entries);
        let new = new_names(&list, &compiled, false, Lang::Turkic);
        let _ = check(&list, &new, &|_, _| false, NameRules::Windows, true);
        // Debug builds are several times slower than release; 50 ms is the release target.
        assert!(start.elapsed() < std::time::Duration::from_millis(500), "{:?}", start.elapsed());
    }
}
```

- [ ] **Step 3: Testleri çalıştır ve başarısız olanları düzelt**

Run: `~/.cargo/bin/cargo test -p gezik-batch rename`
Expected: PASS. (Önce `exif.rs` yoksa derlenmez: Step 4'ü önce yap ya da `lib.rs`'te `pub mod exif;`'i Step 4'e kadar yorumla.)

- [ ] **Step 4: `exif.rs`'i yaz**

`crates/gezik-batch/src/exif.rs`:

```rust
//! When a photo was taken, from its EXIF data (JPEG, TIFF, HEIC, WebP, PNG). Only the start
//! of the file is read.

use std::io::{BufReader, Read};
use std::path::Path;

use gezik_core::batch::date::DateParts;

/// Formats that can carry EXIF, by extension.
pub fn may_have_exif(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".jpg", ".jpeg", ".tif", ".tiff", ".heic", ".heif", ".webp", ".png"].iter().any(|ext| lower.ends_with(ext))
}

/// The date the photo was taken (`DateTimeOriginal`, else `DateTime`), as the camera wrote it
/// (local time). `None` if the file has none or cannot be read.
pub fn taken(path: &Path) -> Option<DateParts> {
    // The EXIF block is near the start; 256 KB is plenty and bounds the read.
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file.take(256 * 1024));
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    [exif::Tag::DateTimeOriginal, exif::Tag::DateTime].into_iter().find_map(|tag| {
        let field = exif.get_field(tag, exif::In::PRIMARY)?;
        match &field.value {
            exif::Value::Ascii(parts) => parse(std::str::from_utf8(parts.first()?).ok()?),
            _ => None,
        }
    })
}

/// `2024:07:01 09:30:00`.
fn parse(text: &str) -> Option<DateParts> {
    let text = text.trim();
    let (date, time) = text.split_once(' ')?;
    let mut d = date.split(':').map(|p| p.parse::<i32>().ok());
    let mut t = time.split(':').map(|p| p.parse::<u8>().ok());
    let parts = DateParts {
        year: d.next()??,
        month: u8::try_from(d.next()??).ok()?,
        day: u8::try_from(d.next()??).ok()?,
        hour: t.next()??,
        minute: t.next()??,
        second: t.next().flatten().unwrap_or(0),
    };
    (parts.year > 0 && (1..=12).contains(&parts.month) && (1..=31).contains(&parts.day)).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_dates() {
        let parts = parse("2024:07:01 09:30:05").unwrap();
        assert_eq!((parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second), (2024, 7, 1, 9, 30, 5));
        assert!(parse("0000:00:00 00:00:00").is_none(), "cameras write zeros when unset");
        assert!(parse("garbage").is_none());
    }

    #[test]
    fn reads_a_real_photo() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/exif.jpg");
        let parts = taken(&path).expect("tests/data/exif.jpg has a DateTimeOriginal");
        assert_eq!((parts.year, parts.month, parts.day), (2024, 7, 1));
    }

    #[test]
    fn files_without_exif() {
        assert!(taken(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml").as_path()).is_none());
        assert!(may_have_exif("IMG.JPG") && !may_have_exif("a.txt"));
    }
}
```

- [ ] **Step 5: Test fotoğrafını üret**

`tests/data/exif.jpg`: `DateTimeOriginal = 2024:07:01 09:30:00` taşıyan küçük bir JPEG. Makinedeki ffmpeg ile (`D:\ffmpeg\bin\ffmpeg.exe`):

```bash
mkdir -p crates/gezik-batch/tests/data
/d/ffmpeg/bin/ffmpeg -loglevel error -f lavfi -i color=c=red:s=16x16 -frames:v 1 -y "$TEMP/plain.jpg"
```

ffmpeg EXIF yazamadığı için tarih elle eklenir: aşağıdaki Python betiği JPEG'in SOI'sinden sonra en küçük bir APP1/EXIF bloğu (tek IFD0 → ExifIFD → DateTimeOriginal) ekler:

```bash
python - <<'EOF'
import struct, os
src = os.path.join(os.environ["TEMP"], "plain.jpg")
data = open(src, "rb").read()
date = b"2024:07:01 09:30:00\x00"
# TIFF header (little endian), IFD0 at 8 with one entry: ExifIFD pointer (0x8769) -> 26.
ifd0 = struct.pack("<H", 1) + struct.pack("<HHII", 0x8769, 4, 1, 26) + struct.pack("<I", 0)
# Exif IFD at 26 with one entry: DateTimeOriginal (0x9003), ASCII, 20 bytes at 44.
exif_ifd = struct.pack("<H", 1) + struct.pack("<HHII", 0x9003, 2, 20, 44) + struct.pack("<I", 0)
tiff = b"II*\x00" + struct.pack("<I", 8) + ifd0 + exif_ifd + date
app1 = b"Exif\x00\x00" + tiff
segment = b"\xff\xe1" + struct.pack(">H", len(app1) + 2) + app1
out = data[:2] + segment + data[2:]
open("crates/gezik-batch/tests/data/exif.jpg", "wb").write(out)
EOF
```

`crates/gezik-batch/tests/data/SOURCES.md`:

```markdown
# Test data

- `exif.jpg`: a 16×16 red JPEG made with ffmpeg, with a hand-written EXIF block
  (`DateTimeOriginal = 2024:07:01 09:30:00`); see `docs/superpowers/plans/2026-10-05-toplu-islemler-5a.md`, Task 4.
```

- [ ] **Step 6: Tüm testler, Linux/macOS denetimi, boyut ölçümü**

```bash
~/.cargo/bin/cargo test -p gezik-batch
~/.cargo/bin/cargo check -p gezik-batch --target x86_64-unknown-linux-gnu
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
```

Boyut: bu görevde `gezik` henüz `gezik-batch`'e bağlı değil; ölçüm Görev 7'de (exe öncesi/sonrası). Görev 7'de +0,5 MB aşılırsa sapma 9'daki sırayla daraltılır.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/gezik-batch
git commit -m "Add gezik-batch: apply rename rules, check new names and read photo dates"
```

---

### Task 5: Platform — yerel tarih parçaları ve sistem dili

**Files:**
- Modify: `crates/gezik-platform/src/datetime.rs`, `crates/gezik-platform/src/lib.rs`
- Create: `crates/gezik-platform/src/locale.rs`

**Interfaces:**
- Produces: `gezik_platform::local_date_parts(SystemTime) -> Option<DateParts>`, `gezik_platform::language() -> String` (`"tr-TR"`, `"en_US.UTF-8"`, boşsa `""`)

- [ ] **Step 1: `local_date_parts`'ı yaz**

`crates/gezik-platform/src/datetime.rs`'e ekle:

```rust
use gezik_core::batch::date::DateParts;

/// `time` as a local date and time.
pub fn local_date_parts(time: SystemTime) -> Option<DateParts> {
    #[cfg(windows)]
    if let Some(parts) = win::parts(time) {
        return Some(parts);
    }
    let secs = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok()?,
        Err(e) => -i64::try_from(e.duration().as_secs()).ok()?,
    };
    let local = secs.saturating_add(local_offset(secs));
    let (days, rest) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    Some(DateParts {
        year: i32::try_from(year).ok()?,
        month: month as u8,
        day: day as u8,
        hour: (rest / 3600) as u8,
        minute: (rest % 3600 / 60) as u8,
        second: (rest % 60) as u8,
    })
}
```

`mod win` içine (var olan `format`'un dönüşümünü yeniden kullanarak):

```rust
    pub fn parts(time: SystemTime) -> Option<gezik_core::batch::date::DateParts> {
        let ticks = time.duration_since(UNIX_EPOCH).ok()?.as_nanos() / 100 + UNIX_EPOCH_TICKS;
        let filetime = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        // SAFETY: pointers to live locals of the right types.
        unsafe {
            FileTimeToSystemTime(&filetime, &mut utc).ok()?;
            SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
        }
        Some(gezik_core::batch::date::DateParts {
            year: i32::from(local.wYear),
            month: local.wMonth as u8,
            day: local.wDay as u8,
            hour: local.wHour as u8,
            minute: local.wMinute as u8,
            second: local.wSecond as u8,
        })
    }
```

Test (dosyanın test modülüne; yoksa ekle):

```rust
#[cfg(test)]
mod parts_tests {
    use super::*;

    #[test]
    fn local_parts_are_a_valid_date_and_match_the_list_format() {
        let now = SystemTime::now();
        let parts = local_date_parts(now).unwrap();
        assert!((1..=12).contains(&parts.month) && (1..=31).contains(&parts.day) && parts.hour < 24);
        // Off Windows the list shows `YYYY-MM-DD HH:MM` from the same conversion.
        #[cfg(not(windows))]
        assert_eq!(
            format_datetime(now),
            format!("{:04}-{:02}-{:02} {:02}:{:02}", parts.year, parts.month, parts.day, parts.hour, parts.minute)
        );
    }
}
```

- [ ] **Step 2: `locale.rs`'i yaz**

`crates/gezik-platform/src/locale.rs`:

```rust
//! The user's language, for letter case rules (Turkish i/İ).

/// The user's locale name (`tr-TR` on Windows, `tr_TR.UTF-8` elsewhere); empty if unknown.
pub fn language() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buffer = [0u16; 85];
        // SAFETY: the buffer and its length go together.
        let n = unsafe { GetUserDefaultLocaleName(&mut buffer) };
        if n > 1 {
            return String::from_utf16_lossy(&buffer[..n as usize - 1]);
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        // LC_ALL overrides LC_CTYPE overrides LANG (POSIX); macOS apps get LANG from the system.
        ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX"))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn language_does_not_fail() {
        let lang = super::language();
        assert!(lang.len() < 85, "{lang}");
    }
}
```

`crates/gezik-platform/src/lib.rs`: `mod locale;` ve `pub use locale::language;`; `datetime`'tan `pub use datetime::{format_datetime, local_date_parts};` (var olan `pub use` satırına ekle).

- [ ] **Step 3: Test, çapraz denetim, commit**

```bash
~/.cargo/bin/cargo test -p gezik-platform datetime locale
~/.cargo/bin/cargo check -p gezik-platform --target x86_64-unknown-linux-gnu
~/.cargo/bin/cargo check -p gezik-platform --target aarch64-apple-darwin
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-platform
git commit -m "Give local date parts and the user's language for rename rules"
```

---

### Task 6: Ayarlar — kural setleri, son kurallar, `batch-rename` eylemi

**Files:**
- Create: `crates/gezik-config/src/batch_toml.rs`
- Modify: `crates/gezik-config/src/lib.rs`, `settings.rs`, `settings_edit.rs`, `store.rs`, `shortcuts.rs`, `templates/settings.toml`, `crates/gezik/src/keys.rs`, `crates/gezik/src/main.rs` (`Action` eşleşmesi)

**Interfaces:**
- Consumes: Task 1 türleri
- Produces:
  - `gezik_config::batch_toml::{rule_from_toml(&toml::Table) -> Result<RuleEntry, String>, rule_to_toml(&RuleEntry) -> toml::Table}`
  - `gezik_config::settings::{RenamePreset { name: String, include_extension: bool, rules: Vec<RuleEntry> }, Settings::rename_presets: Vec<RenamePreset>, BatchRenameState { include_extension: bool, rules: Vec<RuleEntry> }, State::batch_rename: Option<BatchRenameState>}`
  - `gezik_config::settings_edit::with_rename_presets(&str, &[RenamePreset]) -> Result<String, String>`
  - `ConfigStore::save_rename_presets(&self, &[RenamePreset]) -> Result<(), Warning>`
  - `Action::BatchRename` (ad `batch-rename`, varsayılanı yok)

- [ ] **Step 1: `batch_toml.rs`'i testleriyle yaz**

`crates/gezik-config/src/batch_toml.rs`:

```rust
//! Rename rules in TOML: `{ kind = "replace", find = "IMG_", with = "Tatil " }`. Used by
//! `[[rename-presets]]` in settings.toml and the last rules in state.toml.

use gezik_core::batch::case::CaseMode;
use gezik_core::batch::rules::{CleanRule, ExtensionRule, NumberAt, NumberRule, ReplaceRule, Rule, RuleEntry};

fn text(table: &toml::Table, key: &str) -> Result<String, String> {
    match table.get(key) {
        None => Ok(String::new()),
        Some(value) => value.as_str().map(str::to_owned).ok_or_else(|| format!("{key}: expected text, got {value}")),
    }
}

fn flag(table: &toml::Table, key: &str, default: bool) -> Result<bool, String> {
    match table.get(key) {
        None => Ok(default),
        Some(value) => value.as_bool().ok_or_else(|| format!("{key}: expected true or false, got {value}")),
    }
}

fn int(table: &toml::Table, key: &str, default: i64) -> Result<i64, String> {
    match table.get(key) {
        None => Ok(default),
        Some(value) => value.as_integer().ok_or_else(|| format!("{key}: expected a number, got {value}")),
    }
}

pub fn rule_from_toml(table: &toml::Table) -> Result<RuleEntry, String> {
    let kind = text(table, "kind")?;
    let rule = match kind.as_str() {
        "replace" => Rule::Replace(ReplaceRule {
            find: text(table, "find")?,
            with: text(table, "with")?,
            regex: flag(table, "regex", false)?,
            case_sensitive: flag(table, "case-sensitive", false)?,
            all: flag(table, "all", true)?,
        }),
        "number" => {
            let digits = int(table, "digits", 3)?;
            let at = match text(table, "at")?.as_str() {
                "" | "end" => NumberAt::End,
                "start" => NumberAt::Start,
                other => return Err(format!("at: expected \"start\" or \"end\", got \"{other}\"")),
            };
            Rule::Number(NumberRule {
                start: int(table, "start", 1)?,
                step: int(table, "step", 1)?,
                digits: u8::try_from(digits).ok().filter(|d| (1..=9).contains(d)).ok_or("digits: expected 1 to 9")?,
                at,
                separator: if table.contains_key("separator") { text(table, "separator")? } else { " ".to_owned() },
                per_folder: flag(table, "per-folder", false)?,
            })
        }
        "case" => {
            let mode = text(table, "mode")?;
            Rule::Case(CaseMode::parse(&mode).ok_or_else(|| {
                format!("mode: expected \"lower\", \"upper\", \"title\" or \"sentence\", got \"{mode}\"")
            })?)
        }
        "add-text" => Rule::AddText { prefix: text(table, "prefix")?, suffix: text(table, "suffix")? },
        "extension" => match text(table, "mode")?.as_str() {
            "set" => Rule::Extension(ExtensionRule::Set(text(table, "text")?)),
            "lower" => Rule::Extension(ExtensionRule::Lower),
            "upper" => Rule::Extension(ExtensionRule::Upper),
            other => return Err(format!("mode: expected \"set\", \"lower\" or \"upper\", got \"{other}\"")),
        },
        "template" => Rule::Template(text(table, "text")?),
        "clean" => Rule::Clean(CleanRule {
            trim: flag(table, "trim", true)?,
            collapse_spaces: flag(table, "collapse-spaces", true)?,
            separators_to_spaces: flag(table, "separators-to-spaces", false)?,
            spaces_to_underscores: flag(table, "spaces-to-underscores", false)?,
        }),
        "" => return Err("kind is missing".to_owned()),
        other => return Err(format!("kind: unknown rule \"{other}\"")),
    };
    Ok(RuleEntry { enabled: flag(table, "enabled", true)?, rule })
}

pub fn rule_to_toml(entry: &RuleEntry) -> toml::Table {
    let mut t = toml::Table::new();
    let mut put = |key: &str, value: toml::Value| {
        t.insert(key.to_owned(), value);
    };
    let s = |text: &str| toml::Value::String(text.to_owned());
    put("kind", s(entry.rule.kind_name()));
    match &entry.rule {
        Rule::Replace(r) => {
            put("find", s(&r.find));
            put("with", s(&r.with));
            put("regex", toml::Value::Boolean(r.regex));
            put("case-sensitive", toml::Value::Boolean(r.case_sensitive));
            put("all", toml::Value::Boolean(r.all));
        }
        Rule::Number(n) => {
            put("start", toml::Value::Integer(n.start));
            put("step", toml::Value::Integer(n.step));
            put("digits", toml::Value::Integer(n.digits.into()));
            put("at", s(if n.at == NumberAt::Start { "start" } else { "end" }));
            put("separator", s(&n.separator));
            put("per-folder", toml::Value::Boolean(n.per_folder));
        }
        Rule::Case(mode) => put("mode", s(mode.as_str())),
        Rule::AddText { prefix, suffix } => {
            put("prefix", s(prefix));
            put("suffix", s(suffix));
        }
        Rule::Extension(ExtensionRule::Set(ext)) => {
            put("mode", s("set"));
            put("text", s(ext));
        }
        Rule::Extension(ExtensionRule::Lower) => put("mode", s("lower")),
        Rule::Extension(ExtensionRule::Upper) => put("mode", s("upper")),
        Rule::Template(text) => put("text", s(text)),
        Rule::Clean(c) => {
            put("trim", toml::Value::Boolean(c.trim));
            put("collapse-spaces", toml::Value::Boolean(c.collapse_spaces));
            put("separators-to-spaces", toml::Value::Boolean(c.separators_to_spaces));
            put("spaces-to-underscores", toml::Value::Boolean(c.spaces_to_underscores));
        }
    }
    if !entry.enabled {
        put("enabled", toml::Value::Boolean(false));
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::rules::KINDS;

    #[test]
    fn every_kind_survives_a_round_trip() {
        for (kind, _) in KINDS {
            let mut entry = RuleEntry::new(Rule::default_of(kind).unwrap());
            entry.enabled = kind != "case";
            assert_eq!(rule_from_toml(&rule_to_toml(&entry)).unwrap(), entry, "{kind}");
        }
    }

    #[test]
    fn errors_name_the_key() {
        let table: toml::Table = "kind = \"case\"\nmode = \"loud\"".parse().unwrap();
        assert!(rule_from_toml(&table).unwrap_err().starts_with("mode:"));
        let table: toml::Table = "kind = \"nope\"".parse().unwrap();
        assert_eq!(rule_from_toml(&table).unwrap_err(), "kind: unknown rule \"nope\"");
        let table: toml::Table = "kind = \"number\"\ndigits = 12".parse().unwrap();
        assert_eq!(rule_from_toml(&table).unwrap_err(), "digits: expected 1 to 9");
    }
}
```

`crates/gezik-config/src/lib.rs`: `pub mod batch_toml;`.

- [ ] **Step 2: `Settings::rename_presets` ve `State::batch_rename`**

`crates/gezik-config/src/settings.rs`:

```rust
/// A saved set of rename rules (`[[rename-presets]]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreset {
    pub name: String,
    pub include_extension: bool,
    pub rules: Vec<gezik_core::batch::rules::RuleEntry>,
}

/// The rename layer's last rules (state.toml `[batch-rename]`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchRenameState {
    pub include_extension: bool,
    pub rules: Vec<gezik_core::batch::rules::RuleEntry>,
}
```

`Settings`'e `pub rename_presets: Vec<RenamePreset>,` (varsayılan boş). `Settings::parse`'ın sonunda (`settings` dönmeden önce):

```rust
        if let Some(value) = table.get("rename-presets") {
            match value.as_array() {
                None => warnings.push(Warning::new(file, format!("rename-presets: expected [[rename-presets]] tables, got {value}"))),
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_preset(item) {
                            Ok(preset) => settings.rename_presets.push(preset),
                            Err(err) => warnings.push(Warning::new(file, format!("rename-presets[{}]: {err}", i + 1))),
                        }
                    }
                }
            }
        }
```

ve modül düzeyinde:

```rust
fn parse_rules(value: Option<&toml::Value>) -> Result<Vec<gezik_core::batch::rules::RuleEntry>, String> {
    let Some(value) = value else { return Ok(Vec::new()) };
    let items = value.as_array().ok_or_else(|| format!("rules: expected a list, got {value}"))?;
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let table = item.as_table().ok_or_else(|| format!("rules[{}]: expected a table", i + 1))?;
            crate::batch_toml::rule_from_toml(table).map_err(|err| format!("rules[{}]: {err}", i + 1))
        })
        .collect()
}

fn parse_preset(value: &toml::Value) -> Result<RenamePreset, String> {
    let table = value.as_table().ok_or("expected a table")?;
    let name = table.get("name").and_then(|v| v.as_str()).filter(|n| !n.trim().is_empty()).ok_or("name is missing")?;
    let include_extension = table.get("include-extension").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(RenamePreset { name: name.to_owned(), include_extension, rules: parse_rules(table.get("rules"))? })
}

pub fn preset_to_toml(preset: &RenamePreset) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String(preset.name.clone()));
    table.insert("include-extension".into(), toml::Value::Boolean(preset.include_extension));
    let rules = preset.rules.iter().map(|r| toml::Value::Table(crate::batch_toml::rule_to_toml(r))).collect();
    table.insert("rules".into(), toml::Value::Array(rules));
    table
}
```

`State`'e `pub batch_rename: Option<BatchRenameState>,`; `State::parse`'ta:

```rust
        let batch_rename = table.get("batch-rename").and_then(|v| v.as_table()).map(|t| BatchRenameState {
            include_extension: t.get("include-extension").and_then(|v| v.as_bool()).unwrap_or(false),
            // The app wrote it: a broken rule just drops the list.
            rules: parse_rules(t.get("rules")).unwrap_or_default(),
        });
```

(`State { ..., batch_rename }`), `to_toml`'da:

```rust
        if let Some(batch) = &self.batch_rename {
            let mut table = toml::Table::new();
            table.insert("include-extension".into(), toml::Value::Boolean(batch.include_extension));
            let rules = batch.rules.iter().map(|r| toml::Value::Table(crate::batch_toml::rule_to_toml(r))).collect();
            table.insert("rules".into(), toml::Value::Array(rules));
            root.insert("batch-rename".into(), toml::Value::Table(table));
        }
```

Testler (`settings.rs` test modülüne):

```rust
    #[test]
    fn rename_presets_are_read_and_bad_ones_warned() {
        let text = r#"
[[rename-presets]]
name = "Tatil"
rules = [{ kind = "template", text = "{taken} {n:03}" }, { kind = "case", mode = "lower" }]

[[rename-presets]]
rules = []
"#;
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        assert_eq!(settings.rename_presets.len(), 1);
        assert_eq!(settings.rename_presets[0].rules.len(), 2);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].to_string().contains("rename-presets[2]: name is missing"), "{}", warnings[0]);
    }

    #[test]
    fn last_rename_rules_survive_state() {
        use gezik_core::batch::rules::{Rule, RuleEntry};
        let state = State {
            batch_rename: Some(BatchRenameState {
                include_extension: true,
                rules: vec![RuleEntry::new(Rule::Template("{name}".into()))],
            }),
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
    }
```

- [ ] **Step 3: `with_rename_presets` ve `save_rename_presets`**

`crates/gezik-config/src/settings_edit.rs`:

```rust
/// Returns `text` with `[[rename-presets]]` replaced by `presets`; the rest stays.
pub fn with_rename_presets(text: &str, presets: &[crate::settings::RenamePreset]) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    doc.remove("rename-presets");
    if !presets.is_empty() {
        let mut array = toml_edit::ArrayOfTables::new();
        for preset in presets {
            let table = crate::settings::preset_to_toml(preset);
            let text = toml::to_string(&table).map_err(|err| err.to_string())?;
            let parsed = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string())?;
            array.push(parsed.as_table().clone());
        }
        doc.insert("rename-presets", toml_edit::Item::ArrayOfTables(array));
    }
    Ok(doc.to_string())
}
```

Test:

```rust
    #[test]
    fn rename_presets_are_written_keeping_the_rest() {
        use crate::settings::{RenamePreset, Settings};
        use gezik_core::batch::rules::{Rule, RuleEntry};
        let preset = RenamePreset {
            name: "Tatil".into(),
            include_extension: false,
            rules: vec![RuleEntry::new(Rule::Template("{n:03}".into()))],
        };
        let text = "# mine\ntheme = \"nord\"\n\n[[rename-presets]]\nname = \"old\"\n";
        let out = with_rename_presets(text, std::slice::from_ref(&preset)).unwrap();
        assert!(out.contains("# mine"), "{out}");
        let settings = Settings::parse("settings.toml", &out, &mut Vec::new());
        assert_eq!(settings.rename_presets, [preset]);
        let out = with_rename_presets(&out, &[]).unwrap();
        assert!(!out.contains("rename-presets"), "{out}");
    }
```

`crates/gezik-config/src/store.rs`'e (diğer `save_*`'ların yanına):

```rust
    /// Writes the saved rename rule sets into `settings.toml`, keeping everything else.
    pub fn save_rename_presets(&self, presets: &[crate::settings::RenamePreset]) -> Result<(), Warning> {
        self.edit_settings(|text| crate::settings_edit::with_rename_presets(text, presets))
    }
```

- [ ] **Step 4: `Action::BatchRename`**

`crates/gezik-config/src/shortcuts.rs`: enum'a `Redo`'dan sonra `/// Opens the batch rename layer even for one item.` + `BatchRename,`; `ALL` uzunluğunu 26 yap ve sona ekle; `name()`: `Action::BatchRename => "batch-rename",`; `default_text`: `(Action::BatchRename, _) => return None,`.

`crates/gezik/src/keys.rs`: `acts_on_files` ve `needs_list` listelerine `| Action::BatchRename` ekle; test tablosundaki `Action::PasteMove | Action::Duplicate => continue,` satırını `Action::PasteMove | Action::Duplicate | Action::BatchRename => continue,` yap.

`crates/gezik/src/main.rs` `handle_key`'teki eşleşmeye geçici olarak `Action::BatchRename => ops.rename_start(),` ekle (Görev 7 gerçek katmanı bağlar).

`crates/gezik-config/templates/settings.toml`'un sonuna:

```toml

# Saved rule sets for batch rename (Rename… with two or more items selected). The rename
# layer writes them with "Presets ▾ › Save current rules as…"; they can also be written here.
# [[rename-presets]]
# name = "Holiday photos"
# include-extension = false
# rules = [
#   { kind = "template", text = "{taken:%Y-%m-%d} {n:03}" },
#   { kind = "case", mode = "lower" },
# ]
```

ve `[shortcuts]` bölümündeki yorumlu eylem listesine `# batch-rename = ""` satırını, var olan biçimle ekle (önce dosyayı oku ve aynı biçimi izle).

- [ ] **Step 5: Test, denetim, commit**

```bash
~/.cargo/bin/cargo test -p gezik-config && ~/.cargo/bin/cargo test -p gezik
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-config crates/gezik/src/keys.rs crates/gezik/src/main.rs
git commit -m "Read and write rename rule sets, keep the last rules, and add the batch-rename action"
```

---

### Task 7: Arayüz — toplu yeniden adlandırma katmanı (kurallar, önizleme, uygulama)

**Files:**
- Create: `crates/gezik/ui/widgets/rename-batch.slint`, `crates/gezik/src/batch_rename.rs`
- Modify: `crates/gezik/Cargo.toml` (`gezik-batch.workspace = true`), `crates/gezik/ui/app.slint`, `crates/gezik/src/main.rs`, `crates/gezik/src/operations.rs`, `crates/gezik/src/view/mod.rs`

**Interfaces:**
- Consumes: Task 3 `RenameTask::many`; Task 4 `gezik_batch::rename::*`; Task 5 `local_date_parts`, `language`; Task 6 `State::batch_rename`, `Settings::rename_presets`
- Produces:
  - `crate::batch_rename::BatchRename::{new(&AppWindow, Operations, View) -> BatchRename, open(&self, items: Vec<(PathBuf, bool)>), is_open(&self) -> bool, state(&self) -> BatchRenameState, set_last(&self, BatchRenameState)}`
  - `View::all_names(&self) -> Vec<String>`
  - Slint: `RenameBatch` bileşeni, `RuleRow`, `RenamePreviewRow` yapıları, `AppWindow`'da `rb-*` özellik ve geri çağrıları

**Davranış özeti (bu görevin teslim ettiği):** Katman açılır, kural ekleme/silme/sıralama/işaretleme ve seçili kuralın seçenekleri düzenlenir, önizleme her değişiklikten 50 ms sonra yenilenir, uyarı sayısı ve Rename düğmesi doğru durur, Rename `RenameTask::many` gönderir ve katman kapanır, Esc/Cancel kapatır, son kurallar `state.toml`'a yazılır. (Elle düzenleme, sürükleyerek sıralama, kural setleri, EXIF okuma ve menüler Görev 8'de.)

- [ ] **Step 1: `View::all_names` ve Cargo bağlantısı**

`crates/gezik/Cargo.toml` `[dependencies]`: `gezik-batch.workspace = true`.

`crates/gezik/src/view/mod.rs`'e `has_other_named`'in yanına (aynı modeli okuyarak):

```rust
    /// The names of every entry shown (the folder's listing).
    pub fn all_names(&self) -> Vec<String> {
        (0..self.len()).filter_map(|i| self.entry_path(i)).filter_map(|(path, _)| path.file_name().map(|n| n.to_string_lossy().into_owned())).collect()
    }
```

- [ ] **Step 2: Slint bileşenini yaz**

`crates/gezik/ui/widgets/rename-batch.slint`:

```slint
import { ListView } from "std-widgets.slint";
import { Theme } from "../theme.slint";
import { TextButton } from "text-button.slint";
import { TextField } from "text-field.slint";

// One rule in the list (batch_rename.rs builds them).
export struct RuleRow {
    summary: string,
    enabled: bool,
    error: string,
    selected: bool,
}

// One item in the preview.
export struct RenamePreviewRow {
    old-name: string,
    new-name: string,
    // 0 unchanged, 1 changed, 2 duplicate, 3 exists, 4 invalid.
    status: int,
    // The tip for a warning ("Another item gets this name").
    note: string,
    // Typed by hand: the rules leave it alone.
    manual: bool,
    selected: bool,
}

// The switches of the selected rule; which ones show depends on `kind`. Its texts are
// separate two-way properties (`opt-*`): a typed field keeps its binding that way.
export struct RuleOptions {
    kind: string,
    // Replace
    regex: bool,
    case-sensitive: bool,
    all: bool,
    // Number
    at-start: bool,
    per-folder: bool,
    // Case (0-3: lower, upper, title, sentence) and Extension (0 set, 1 lower, 2 upper)
    mode: int,
    // Clean
    trim: bool,
    collapse: bool,
    separators: bool,
    underscores: bool,
}

component Check inherits Rectangle {
    in property <string> text;
    in property <bool> checked;
    callback toggled();
    height: Theme.row-height;
    min-width: label.preferred-width + 24px;
    accessible-role: checkbox;
    accessible-checked: root.checked;
    accessible-label: root.text;
    accessible-action-default => { root.toggled(); }
    Rectangle {
        x: 0;
        y: (parent.height - 14px) / 2;
        width: 14px;
        height: 14px;
        border-radius: 3px;
        border-width: 1px;
        border-color: root.checked ? Theme.accent : Theme.border;
        background: root.checked ? Theme.accent : transparent;
        if root.checked: Path {
            width: 14px;
            height: 14px;
            viewbox-width: 14;
            viewbox-height: 14;
            stroke: Theme.accent-foreground;
            stroke-width: 2px;
            commands: "M 3 7 L 6 10 L 11 4";
        }
    }
    label := Text {
        x: 20px;
        height: parent.height;
        text: root.text;
        vertical-alignment: center;
        color: Theme.foreground;
    }
    TouchArea {
        mouse-cursor: pointer;
        clicked => { root.toggled(); }
    }
}

component Labeled inherits HorizontalLayout {
    in property <string> label;
    spacing: Theme.spacing;
    Text {
        text: root.label;
        width: 72px;
        vertical-alignment: center;
        color: Theme.foreground-muted;
    }
    @children
}

export component RenameBatch inherits Rectangle {
    in property <bool> open;
    in property <bool> dialog-open;
    in property <string> title;
    in property <[RuleRow]> rules;
    in property <int> selected-rule: -1;
    in property <RuleOptions> options;
    // The selected rule's texts (Rust loads them when another rule is selected).
    in-out property <string> opt-find;
    in-out property <string> opt-with;
    in-out property <string> opt-start;
    in-out property <string> opt-step;
    in-out property <string> opt-digits;
    in-out property <string> opt-separator;
    in-out property <string> opt-prefix;
    in-out property <string> opt-suffix;
    in-out property <string> opt-text;
    in property <[RenamePreviewRow]> rows;
    in property <bool> include-extension;
    in property <bool> only-changed;
    in property <string> footer;
    in property <bool> can-rename;
    in-out property <length> scroll;
    // The selected preview row's new name (Task 8 edits it).
    in-out property <string> name-text;
    in property <bool> name-enabled;

    callback add-rule(length, length);
    callback select-rule(int);
    callback toggle-rule(int);
    callback move-rule(int);   // -1 up, +1 down
    callback remove-rule();
    // A text option changed: (key, value), keys as in RuleOptions.
    callback option-text(string, string);
    // A switch option: (key, value).
    callback option-flag(string, bool);
    callback option-mode(int);
    callback toggle-extension();
    callback toggle-only-changed();
    callback row-pressed(int, bool, bool);
    callback rename();
    callback cancel();
    callback presets(length, length);
    callback name-edited(string);
    callback name-reset();
    callback reorder(int, int);
    callback key(KeyEvent) -> bool;

    changed open => {
        if self.open && !root.dialog-open {
            scope.focus();
        }
    }

    public function take-focus() {
        scope.focus();
    }

    public function focus-name() {
        name-field.focus-and-select();
    }

    public function ensure-visible(index: int) {
        if index * Theme.row-height < -root.scroll {
            root.scroll = -index * Theme.row-height;
        } else if (index + 1) * Theme.row-height > -root.scroll + preview.visible-height {
            root.scroll = -((index + 1) * Theme.row-height - preview.visible-height);
        }
    }

    background: Theme.background.with-alpha(0.6);
    TouchArea { }

    scope := FocusScope {
        key-pressed(event) => {
            if root.key(event) { return accept; }
            return reject;
        }
    }

    Rectangle {
        x: 24px;
        y: 24px;
        width: root.width - 48px;
        height: root.height - 48px;
        background: Theme.surface;
        border-radius: Theme.radius * 1.5;
        border-width: 1px;
        border-color: Theme.border;
        accessible-role: groupbox;
        accessible-label: root.title;

        VerticalLayout {
            padding: Theme.spacing * 2;
            spacing: Theme.spacing;

            HorizontalLayout {
                spacing: Theme.spacing;
                Text {
                    text: root.title;
                    font-weight: 600;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                    overflow: elide;
                    color: Theme.foreground;
                }
                TextButton {
                    text: "Presets ▾";
                    clicked => { root.presets(self.absolute-position.x, self.absolute-position.y + self.height); }
                }
            }

            HorizontalLayout {
                spacing: Theme.spacing * 2;
                vertical-stretch: 1;

                // Rules and the selected rule's options.
                VerticalLayout {
                    width: min(360px, parent.width * 0.42);
                    spacing: Theme.spacing;
                    Text { text: "Rules"; color: Theme.foreground-muted; }
                    rule-list := ListView {
                        vertical-stretch: 1;
                        for rule[i] in root.rules: Rectangle {
                            height: Theme.row-height;
                            background: rule.selected ? Theme.selection : rule-touch.has-hover ? Theme.hover : transparent;
                            accessible-role: list-item;
                            accessible-label: rule.summary;
                            accessible-item-selectable: true;
                            accessible-item-selected: rule.selected;
                            rule-touch := TouchArea {
                                clicked => { root.select-rule(i); }
                            }
                            HorizontalLayout {
                                padding-left: Theme.spacing;
                                spacing: Theme.spacing;
                                Check {
                                    width: 20px;
                                    text: "";
                                    checked: rule.enabled;
                                    toggled => { root.toggle-rule(i); }
                                }
                                Text {
                                    text: (i + 1) + "  " + rule.summary;
                                    horizontal-stretch: 1;
                                    vertical-alignment: center;
                                    overflow: elide;
                                    color: rule.error != "" ? Theme.danger
                                        : rule.selected ? Theme.selection-foreground
                                        : rule.enabled ? Theme.foreground : Theme.foreground-muted;
                                }
                            }
                        }
                    }
                    HorizontalLayout {
                        spacing: Theme.spacing;
                        TextButton {
                            text: "+ Add rule ▾";
                            clicked => { root.add-rule(self.absolute-position.x, self.absolute-position.y + self.height); }
                        }
                        TextButton { text: "↑"; min-width: 32px; clicked => { root.move-rule(-1); } }
                        TextButton { text: "↓"; min-width: 32px; clicked => { root.move-rule(1); } }
                        TextButton { text: "✕"; min-width: 32px; clicked => { root.remove-rule(); } }
                    }
                    if root.selected-rule >= 0 && root.rules[root.selected-rule].error != "": Text {
                        text: root.rules[root.selected-rule].error;
                        color: Theme.danger;
                        wrap: word-wrap;
                    }
                    // The selected rule's options.
                    if root.options.kind == "replace": VerticalLayout {
                        spacing: Theme.spacing;
                        Labeled { label: "Find"; TextField { text <=> root.opt-find; edited(t) => { root.option-text("find", t); } } }
                        Labeled { label: "Replace"; TextField { text <=> root.opt-with; edited(t) => { root.option-text("with", t); } } }
                        HorizontalLayout {
                            spacing: Theme.spacing * 2;
                            Check { text: "Regex"; checked: root.options.regex; toggled => { root.option-flag("regex", !root.options.regex); } }
                            Check { text: "Match case"; checked: root.options.case-sensitive; toggled => { root.option-flag("case-sensitive", !root.options.case-sensitive); } }
                            Check { text: "All"; checked: root.options.all; toggled => { root.option-flag("all", !root.options.all); } }
                        }
                    }
                    if root.options.kind == "number": VerticalLayout {
                        spacing: Theme.spacing;
                        HorizontalLayout {
                            spacing: Theme.spacing;
                            Labeled { label: "Start"; TextField { text <=> root.opt-start; edited(t) => { root.option-text("start", t); } } }
                            Labeled { label: "Step"; TextField { text <=> root.opt-step; edited(t) => { root.option-text("step", t); } } }
                        }
                        HorizontalLayout {
                            spacing: Theme.spacing;
                            Labeled { label: "Digits"; TextField { text <=> root.opt-digits; edited(t) => { root.option-text("digits", t); } } }
                            Labeled { label: "Separator"; TextField { text <=> root.opt-separator; edited(t) => { root.option-text("separator", t); } } }
                        }
                        HorizontalLayout {
                            spacing: Theme.spacing;
                            TextButton { text: "At start"; checked: root.options.at-start; clicked => { root.option-flag("at-start", true); } }
                            TextButton { text: "At end"; checked: !root.options.at-start; clicked => { root.option-flag("at-start", false); } }
                            Check { text: "Per folder"; checked: root.options.per-folder; toggled => { root.option-flag("per-folder", !root.options.per-folder); } }
                        }
                    }
                    if root.options.kind == "case": HorizontalLayout {
                        spacing: Theme.spacing;
                        TextButton { text: "lower"; checked: root.options.mode == 0; clicked => { root.option-mode(0); } }
                        TextButton { text: "UPPER"; checked: root.options.mode == 1; clicked => { root.option-mode(1); } }
                        TextButton { text: "Title"; checked: root.options.mode == 2; clicked => { root.option-mode(2); } }
                        TextButton { text: "Sentence"; checked: root.options.mode == 3; clicked => { root.option-mode(3); } }
                    }
                    if root.options.kind == "add-text": VerticalLayout {
                        spacing: Theme.spacing;
                        Labeled { label: "Before"; TextField { text <=> root.opt-prefix; edited(t) => { root.option-text("prefix", t); } } }
                        Labeled { label: "After"; TextField { text <=> root.opt-suffix; edited(t) => { root.option-text("suffix", t); } } }
                    }
                    if root.options.kind == "extension": VerticalLayout {
                        spacing: Theme.spacing;
                        HorizontalLayout {
                            spacing: Theme.spacing;
                            TextButton { text: "New"; checked: root.options.mode == 0; clicked => { root.option-mode(0); } }
                            TextButton { text: "lower"; checked: root.options.mode == 1; clicked => { root.option-mode(1); } }
                            TextButton { text: "UPPER"; checked: root.options.mode == 2; clicked => { root.option-mode(2); } }
                        }
                        if root.options.mode == 0: Labeled { label: "Extension"; TextField { text <=> root.opt-text; edited(t) => { root.option-text("text", t); } } }
                    }
                    if root.options.kind == "template": VerticalLayout {
                        spacing: Theme.spacing;
                        Labeled { label: "Name"; TextField { text <=> root.opt-text; edited(t) => { root.option-text("text", t); } } }
                        Text {
                            text: "{name} {ext} {n:03} {parent} {date:%Y-%m-%d} {taken:%Y-%m-%d} {size}";
                            color: Theme.foreground-muted;
                            wrap: word-wrap;
                        }
                    }
                    if root.options.kind == "clean": VerticalLayout {
                        spacing: Theme.spacing;
                        Check { text: "Trim spaces at the ends"; checked: root.options.trim; toggled => { root.option-flag("trim", !root.options.trim); } }
                        Check { text: "Collapse repeated spaces"; checked: root.options.collapse; toggled => { root.option-flag("collapse", !root.options.collapse); } }
                        Check { text: "_ . - become spaces"; checked: root.options.separators; toggled => { root.option-flag("separators", !root.options.separators); } }
                        Check { text: "Spaces become _"; checked: root.options.underscores; toggled => { root.option-flag("underscores", !root.options.underscores); } }
                    }
                    Check {
                        text: "Include extension";
                        checked: root.include-extension;
                        toggled => { root.toggle-extension(); }
                    }
                }

                Rectangle { width: 1px; background: Theme.border; }

                // Preview.
                VerticalLayout {
                    horizontal-stretch: 1;
                    spacing: Theme.spacing;
                    HorizontalLayout {
                        spacing: Theme.spacing;
                        Text { text: "Preview"; horizontal-stretch: 1; color: Theme.foreground-muted; }
                        Check { text: "Only changed"; checked: root.only-changed; toggled => { root.toggle-only-changed(); } }
                    }
                    HorizontalLayout {
                        height: Theme.row-height;
                        padding-left: Theme.spacing + 20px;
                        spacing: Theme.spacing * 2;
                        Text { text: "Old name"; horizontal-stretch: 1; color: Theme.foreground-muted; vertical-alignment: center; }
                        Text { text: "New name"; horizontal-stretch: 1; color: Theme.foreground-muted; vertical-alignment: center; }
                    }
                    preview := ListView {
                        vertical-stretch: 1;
                        content-y <=> root.scroll;
                        for row[i] in root.rows: line := Rectangle {
                            property <color> fg: row.selected ? Theme.selection-foreground : Theme.foreground;
                            height: Theme.row-height;
                            background: row.selected ? Theme.selection : row-touch.has-hover ? Theme.hover : transparent;
                            accessible-role: list-item;
                            accessible-label: row.old-name + " to " + row.new-name;
                            accessible-description: row.note;
                            accessible-item-selectable: true;
                            accessible-item-selected: row.selected;
                            accessible-item-index: i;
                            row-touch := TouchArea {
                                pointer-event(event) => {
                                    if event.kind == PointerEventKind.down && event.button == PointerEventButton.left {
                                        scope.focus();
                                        root.row-pressed(i, event.modifiers.control, event.modifiers.shift);
                                    }
                                }
                                double-clicked => { name-field.focus-and-select(); }
                            }
                            HorizontalLayout {
                                padding-left: Theme.spacing;
                                spacing: Theme.spacing * 2;
                                // The drag handle (Task 8) and the row's mark.
                                handle := Rectangle {
                                    width: 14px;
                                    Text {
                                        text: row.manual ? "✎" : row.status >= 2 ? "⚠" : "≡";
                                        color: row.status >= 2 ? Theme.danger : Theme.foreground-muted;
                                        vertical-alignment: center;
                                        height: parent.height;
                                    }
                                }
                                Text {
                                    text: row.old-name;
                                    horizontal-stretch: 1;
                                    vertical-alignment: center;
                                    overflow: elide;
                                    color: line.fg;
                                }
                                Text {
                                    text: row.new-name;
                                    horizontal-stretch: 1;
                                    vertical-alignment: center;
                                    overflow: elide;
                                    color: row.status >= 2 && !row.selected ? Theme.danger : line.fg;
                                    font-weight: row.status == 1 ? 600 : 400;
                                }
                            }
                        }
                    }
                    HorizontalLayout {
                        spacing: Theme.spacing;
                        Text { text: "Name"; vertical-alignment: center; color: Theme.foreground-muted; }
                        name-field := TextField {
                            text <=> root.name-text;
                            enabled: root.name-enabled;
                            edited(t) => { root.name-edited(t); }
                        }
                        TextButton { text: "Reset to rules"; clicked => { root.name-reset(); } }
                    }
                }
            }

            Rectangle { height: 1px; background: Theme.border; }
            HorizontalLayout {
                spacing: Theme.spacing;
                Text {
                    text: root.footer;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                    overflow: elide;
                    color: Theme.foreground-muted;
                }
                TextButton { text: "Cancel"; clicked => { root.cancel(); } }
                TextButton {
                    text: "Rename";
                    primary: root.can-rename;
                    clicked => { if root.can-rename { root.rename(); } }
                }
            }
        }
    }
}
```

`TextField`'te `edited` geri çağrısı ve `enabled` özelliği yoksa ekle (`crates/gezik/ui/widgets/text-field.slint`):

```slint
    in property <bool> enabled: true;
    callback edited(string);
```

ve `input := TextInput { ... enabled: root.enabled; edited => { root.edited(self.text); } }` (var olan `accepted` satırının yanına). `TextField`'in `text` bağlaması `in-out` olduğu için Rust'tan yazılan değer görünür.

- [ ] **Step 3: `app.slint`'e bağla**

`crates/gezik/ui/app.slint`:
- `import { RenameBatch, RuleRow, RenamePreviewRow, RuleOptions } from "widgets/rename-batch.slint";` ve `export { ..., RuleRow, RenamePreviewRow, RuleOptions }`.
- `AppWindow`'a (conflict özelliklerinin yanına):

```slint
    // The batch rename layer (batch_rename.rs).
    in property <bool> rb-open;
    in property <string> rb-title;
    in property <[RuleRow]> rb-rules;
    in property <int> rb-selected-rule: -1;
    in property <RuleOptions> rb-options;
    in-out property <string> rb-opt-find;
    in-out property <string> rb-opt-with;
    in-out property <string> rb-opt-start;
    in-out property <string> rb-opt-step;
    in-out property <string> rb-opt-digits;
    in-out property <string> rb-opt-separator;
    in-out property <string> rb-opt-prefix;
    in-out property <string> rb-opt-suffix;
    in-out property <string> rb-opt-text;
    in property <[RenamePreviewRow]> rb-rows;
    in property <bool> rb-include-extension;
    in property <bool> rb-only-changed;
    in property <string> rb-footer;
    in property <bool> rb-can-rename;
    in-out property <length> rb-scroll;
    in-out property <string> rb-name-text;
    in property <bool> rb-name-enabled;
    callback rb-add-rule(length, length);
    callback rb-select-rule(int);
    callback rb-toggle-rule(int);
    callback rb-move-rule(int);
    callback rb-remove-rule();
    callback rb-option-text(string, string);
    callback rb-option-flag(string, bool);
    callback rb-option-mode(int);
    callback rb-toggle-extension();
    callback rb-toggle-only-changed();
    callback rb-row-pressed(int, bool, bool);
    callback rb-rename();
    callback rb-cancel();
    callback rb-presets(length, length);
    callback rb-name-edited(string);
    callback rb-name-reset();
    callback rb-reorder(int, int);
    callback rb-key(KeyEvent) -> bool;

    public function rb-ensure-visible(index: int) {
        rename-batch.ensure-visible(index);
    }
    public function rb-focus-name() {
        rename-batch.focus-name();
    }
```

- `focus-list()` içinde: `if root.conflicts-open { conflict-list.take-focus(); } else if root.rb-open { rename-batch.take-focus(); } else { keys.focus(); }`
- `conflict-list := ConflictList {...}`'ten **önce** (çakışma listesi üstte kalsın):

```slint
        rename-batch := RenameBatch {
            x: 0;
            y: 0;
            width: root.width;
            height: root.height;
            visible: root.rb-open;
            open: root.rb-open;
            dialog-open: root.dialog-open || root.conflicts-open;
            title: root.rb-title;
            rules: root.rb-rules;
            selected-rule: root.rb-selected-rule;
            options: root.rb-options;
            opt-find <=> root.rb-opt-find;
            opt-with <=> root.rb-opt-with;
            opt-start <=> root.rb-opt-start;
            opt-step <=> root.rb-opt-step;
            opt-digits <=> root.rb-opt-digits;
            opt-separator <=> root.rb-opt-separator;
            opt-prefix <=> root.rb-opt-prefix;
            opt-suffix <=> root.rb-opt-suffix;
            opt-text <=> root.rb-opt-text;
            rows: root.rb-rows;
            include-extension: root.rb-include-extension;
            only-changed: root.rb-only-changed;
            footer: root.rb-footer;
            can-rename: root.rb-can-rename;
            scroll <=> root.rb-scroll;
            name-text <=> root.rb-name-text;
            name-enabled: root.rb-name-enabled;
            add-rule(x, y) => { root.rb-add-rule(x, y); }
            select-rule(i) => { root.rb-select-rule(i); }
            toggle-rule(i) => { root.rb-toggle-rule(i); }
            move-rule(d) => { root.rb-move-rule(d); }
            remove-rule => { root.rb-remove-rule(); }
            option-text(k, v) => { root.rb-option-text(k, v); }
            option-flag(k, v) => { root.rb-option-flag(k, v); }
            option-mode(m) => { root.rb-option-mode(m); }
            toggle-extension => { root.rb-toggle-extension(); }
            toggle-only-changed => { root.rb-toggle-only-changed(); }
            row-pressed(i, c, s) => { root.rb-row-pressed(i, c, s); }
            rename => { root.rb-rename(); }
            cancel => { root.rb-cancel(); }
            presets(x, y) => { root.rb-presets(x, y); }
            name-edited(t) => { root.rb-name-edited(t); }
            name-reset => { root.rb-name-reset(); }
            reorder(a, b) => { root.rb-reorder(a, b); }
            key(event) => { return root.rb-key(event); }
        }
```

- [ ] **Step 4: Denetleyicinin saf kısmını testleriyle yaz**

`crates/gezik/src/batch_rename.rs` — önce arayüzsüz, test edilebilir çekirdek:

```rust
//! The batch rename layer: rules on the left, the preview on the right, Rename runs one
//! `RenameTask`. The rules and their results live here; Slint only shows them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gezik_batch::rename::{Item, Status, check, compile, new_names};
use gezik_config::settings::BatchRenameState;
use gezik_core::batch::case::{CaseMode, Lang};
use gezik_core::batch::rules::{ExtensionRule, KINDS, NumberAt, Rule, RuleEntry};
use gezik_core::ops::names::NameRules;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, RenamePreviewRow, RuleOptions, RuleRow};

/// Whether this system's file names ignore case (Windows, macOS).
const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

/// The menu ids of "Add rule" (context_menu.rs reserves 90-99 for this layer).
pub const ADD_RULE_FIRST: u32 = 90;

/// Everything the layer knows; no Slint in here, so it is tested on its own.
#[derive(Default)]
pub struct Rules {
    pub items: Vec<Item>,
    pub paths: Vec<PathBuf>,
    /// Display order: indices into `items`.
    pub order: Vec<usize>,
    pub rules: Vec<RuleEntry>,
    pub selected_rule: Option<usize>,
    pub include_extension: bool,
    /// Names typed by hand, by item.
    pub manual: HashMap<usize, String>,
    /// Names in the folder that are not being renamed.
    pub others: HashSet<String>,
    pub lang: Lang,
    // Results, by display position.
    pub new: Vec<String>,
    pub statuses: Vec<Status>,
    pub rule_errors: Vec<Option<String>>,
}

impl Rules {
    pub fn recompute(&mut self) {
        let items: Vec<Item> = self.order.iter().map(|&i| self.items[i].clone()).collect();
        let compiled = compile(&self.rules);
        let mut new = new_names(&items, &compiled, self.include_extension, self.lang);
        for (position, &i) in self.order.iter().enumerate() {
            if let Some(name) = self.manual.get(&i) {
                new[position] = name.clone();
            }
        }
        let others: HashSet<String> =
            self.others.iter().map(|n| if IGNORE_CASE { n.to_lowercase() } else { n.clone() }).collect();
        let existing = |_: usize, name: &str| others.contains(&if IGNORE_CASE { name.to_lowercase() } else { name.to_owned() });
        self.statuses = check(&items, &new, &existing, NameRules::current(), IGNORE_CASE);
        self.rule_errors = compiled.errors().to_vec();
        self.new = new;
    }

    pub fn changed(&self) -> usize {
        self.statuses.iter().filter(|s| **s == Status::Changed).count()
    }

    pub fn blocked(&self) -> usize {
        self.statuses.iter().filter(|s| s.blocks()).count()
    }

    pub fn can_rename(&self) -> bool {
        self.changed() > 0 && self.blocked() == 0 && self.rule_errors.iter().all(Option::is_none)
    }

    /// "1 duplicate name · 22 will change".
    pub fn footer(&self) -> String {
        let count = |want: fn(&Status) -> bool| self.statuses.iter().filter(|s| want(s)).count();
        let mut parts = Vec::new();
        let plural = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
        let duplicates = count(|s| *s == Status::Duplicate);
        if duplicates > 0 {
            parts.push(plural(duplicates, "duplicate name", "duplicate names"));
        }
        let exists = count(|s| *s == Status::Exists);
        if exists > 0 {
            parts.push(plural(exists, "name already in the folder", "names already in the folder"));
        }
        let invalid = count(|s| matches!(s, Status::Invalid(_)));
        if invalid > 0 {
            parts.push(plural(invalid, "invalid name", "invalid names"));
        }
        if self.rule_errors.iter().any(Option::is_some) {
            parts.push("a rule has an error".to_owned());
        }
        parts.push(format!("{} will change", self.changed()));
        if !parts.is_empty() && self.blocked() > 0 {
            parts.push("fix the marked rows".to_owned());
        }
        parts.join(" · ")
    }

    /// The pairs to rename: only those that change.
    pub fn pairs(&self) -> Vec<(PathBuf, PathBuf)> {
        self.order
            .iter()
            .enumerate()
            .filter(|(position, _)| self.statuses.get(*position) == Some(&Status::Changed))
            .map(|(position, &i)| (self.paths[i].clone(), self.paths[i].with_file_name(&self.new[position])))
            .collect()
    }

    pub fn add_rule(&mut self, kind: &str) {
        if let Some(rule) = Rule::default_of(kind) {
            self.rules.push(RuleEntry::new(rule));
            self.selected_rule = Some(self.rules.len() - 1);
        }
    }

    pub fn move_rule(&mut self, by: i32) {
        let Some(at) = self.selected_rule else { return };
        let to = at as i64 + i64::from(by);
        if to < 0 || to as usize >= self.rules.len() {
            return;
        }
        self.rules.swap(at, to as usize);
        self.selected_rule = Some(to as usize);
    }

    pub fn remove_rule(&mut self) {
        let Some(at) = self.selected_rule else { return };
        if at < self.rules.len() {
            self.rules.remove(at);
        }
        self.selected_rule = if self.rules.is_empty() { None } else { Some(at.min(self.rules.len() - 1)) };
    }

    /// A text option of the selected rule changed (`key` as in `RuleOptions`).
    pub fn set_text(&mut self, key: &str, value: &str) {
        let Some(rule) = self.selected_rule.and_then(|i| self.rules.get_mut(i)) else { return };
        let number = |text: &str, old: i64| text.trim().parse::<i64>().unwrap_or(old);
        match (&mut rule.rule, key) {
            (Rule::Replace(r), "find") => r.find = value.to_owned(),
            (Rule::Replace(r), "with") => r.with = value.to_owned(),
            (Rule::Number(n), "start") => n.start = number(value, n.start),
            (Rule::Number(n), "step") => n.step = number(value, n.step),
            (Rule::Number(n), "digits") => n.digits = number(value, i64::from(n.digits)).clamp(1, 9) as u8,
            (Rule::Number(n), "separator") => n.separator = value.to_owned(),
            (Rule::AddText { prefix, .. }, "prefix") => *prefix = value.to_owned(),
            (Rule::AddText { suffix, .. }, "suffix") => *suffix = value.to_owned(),
            (Rule::Template(text), "text") => *text = value.to_owned(),
            (Rule::Extension(ExtensionRule::Set(ext)), "text") => *ext = value.to_owned(),
            _ => {}
        }
    }

    pub fn set_flag(&mut self, key: &str, value: bool) {
        let Some(rule) = self.selected_rule.and_then(|i| self.rules.get_mut(i)) else { return };
        match (&mut rule.rule, key) {
            (Rule::Replace(r), "regex") => r.regex = value,
            (Rule::Replace(r), "case-sensitive") => r.case_sensitive = value,
            (Rule::Replace(r), "all") => r.all = value,
            (Rule::Number(n), "at-start") => n.at = if value { NumberAt::Start } else { NumberAt::End },
            (Rule::Number(n), "per-folder") => n.per_folder = value,
            (Rule::Clean(c), "trim") => c.trim = value,
            (Rule::Clean(c), "collapse") => c.collapse_spaces = value,
            (Rule::Clean(c), "separators") => c.separators_to_spaces = value,
            (Rule::Clean(c), "underscores") => c.spaces_to_underscores = value,
            _ => {}
        }
    }

    pub fn set_mode(&mut self, mode: i32) {
        let Some(rule) = self.selected_rule.and_then(|i| self.rules.get_mut(i)) else { return };
        match &mut rule.rule {
            Rule::Case(case) => {
                if let Some(m) = usize::try_from(mode).ok().and_then(|m| CaseMode::ALL.get(m)) {
                    *case = *m;
                }
            }
            Rule::Extension(ext) => {
                *ext = match mode {
                    1 => ExtensionRule::Lower,
                    2 => ExtensionRule::Upper,
                    _ => ExtensionRule::Set(match ext {
                        ExtensionRule::Set(text) => text.clone(),
                        _ => String::new(),
                    }),
                }
            }
            _ => {}
        }
    }

    pub fn options(&self) -> RuleOptions {
        let mut o = RuleOptions::default();
        let Some(entry) = self.selected_rule.and_then(|i| self.rules.get(i)) else { return o };
        o.kind = entry.rule.kind_name().into();
        match &entry.rule {
            Rule::Replace(r) => {
                o.regex = r.regex;
                o.case_sensitive = r.case_sensitive;
                o.all = r.all;
            }
            Rule::Number(n) => {
                o.at_start = n.at == NumberAt::Start;
                o.per_folder = n.per_folder;
            }
            Rule::Case(mode) => o.mode = CaseMode::ALL.iter().position(|m| m == mode).unwrap_or(0) as i32,
            Rule::Extension(ExtensionRule::Set(_)) => o.mode = 0,
            Rule::Extension(ExtensionRule::Lower) => o.mode = 1,
            Rule::Extension(ExtensionRule::Upper) => o.mode = 2,
            Rule::Clean(c) => {
                o.trim = c.trim;
                o.collapse = c.collapse_spaces;
                o.separators = c.separators_to_spaces;
                o.underscores = c.spaces_to_underscores;
            }
            Rule::AddText { .. } | Rule::Template(_) => {}
        }
        o
    }

    /// The selected rule's texts, by `opt-*` key (find, with, start, step, digits, separator,
    /// prefix, suffix, text); keys the rule does not have are empty.
    pub fn option_texts(&self) -> Vec<(&'static str, String)> {
        let mut texts: Vec<(&'static str, String)> =
            ["find", "with", "start", "step", "digits", "separator", "prefix", "suffix", "text"]
                .into_iter()
                .map(|k| (k, String::new()))
                .collect();
        let mut set = |key: &str, value: String| {
            if let Some(slot) = texts.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = value;
            }
        };
        match self.selected_rule.and_then(|i| self.rules.get(i)).map(|e| &e.rule) {
            Some(Rule::Replace(r)) => {
                set("find", r.find.clone());
                set("with", r.with.clone());
            }
            Some(Rule::Number(n)) => {
                set("start", n.start.to_string());
                set("step", n.step.to_string());
                set("digits", n.digits.to_string());
                set("separator", n.separator.clone());
            }
            Some(Rule::AddText { prefix, suffix }) => {
                set("prefix", prefix.clone());
                set("suffix", suffix.clone());
            }
            Some(Rule::Template(text)) | Some(Rule::Extension(ExtensionRule::Set(text))) => set("text", text.clone()),
            _ => {}
        }
        texts
    }

    pub fn rule_rows(&self) -> Vec<RuleRow> {
        self.rules
            .iter()
            .enumerate()
            .map(|(i, entry)| RuleRow {
                summary: entry.rule.summary().into(),
                enabled: entry.enabled,
                error: self.rule_errors.get(i).cloned().flatten().unwrap_or_default().into(),
                selected: self.selected_rule == Some(i),
            })
            .collect()
    }

    pub fn preview_row(&self, position: usize, selected: bool) -> Option<RenamePreviewRow> {
        let i = *self.order.get(position)?;
        let status = self.statuses.get(position)?;
        let (code, note) = match status {
            Status::Unchanged => (0, String::new()),
            Status::Changed => (1, String::new()),
            Status::Duplicate => (2, "Another item gets this name".to_owned()),
            Status::Exists => (3, "Something in the folder already has this name".to_owned()),
            Status::Invalid(err) => (4, err.to_string()),
        };
        Some(RenamePreviewRow {
            old_name: self.items[i].name.clone().into(),
            new_name: self.new.get(position)?.clone().into(),
            status: code,
            note: note.into(),
            manual: self.manual.contains_key(&i),
            selected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(names: &[&str], others: &[&str]) -> Rules {
        let items: Vec<Item> = names.iter().map(|n| Item { name: n.to_string(), ..Item::default() }).collect();
        let paths = names.iter().map(|n| PathBuf::from("/d").join(n)).collect();
        Rules {
            order: (0..items.len()).collect(),
            items,
            paths,
            others: others.iter().map(|s| s.to_string()).collect(),
            ..Rules::default()
        }
    }

    #[test]
    fn rules_are_added_edited_moved_and_removed() {
        let mut m = model(&["IMG_1.jpg", "IMG_2.jpg"], &[]);
        m.add_rule("replace");
        m.set_text("find", "IMG_");
        m.set_text("with", "Tatil ");
        m.add_rule("case");
        m.set_mode(1);
        m.recompute();
        assert_eq!(m.new, ["TATIL 1.jpg", "TATIL 2.jpg"]);
        m.move_rule(-1);
        assert_eq!(m.selected_rule, Some(0));
        m.remove_rule();
        m.recompute();
        assert_eq!(m.new, ["Tatil 1.jpg", "Tatil 2.jpg"]);
        assert_eq!(m.pairs().len(), 2);
        assert!(m.can_rename());
    }

    #[test]
    fn warnings_block_and_say_why() {
        let mut m = model(&["a.txt", "b.txt"], &["x.txt"]);
        m.add_rule("template");
        m.set_text("text", "x");
        m.recompute();
        assert!(!m.can_rename());
        assert!(m.footer().starts_with("2 duplicate names"), "{}", m.footer());
        m.manual.insert(1, "y.txt".into());
        m.recompute();
        assert_eq!(m.statuses, [Status::Exists, Status::Changed]);
        assert!(m.footer().contains("1 name already in the folder"), "{}", m.footer());
    }

    #[test]
    fn a_bad_rule_blocks() {
        let mut m = model(&["a.txt"], &[]);
        m.add_rule("replace");
        m.set_flag("regex", true);
        m.set_text("find", "(");
        m.recompute();
        assert!(!m.can_rename());
        assert!(m.rule_rows()[0].error.len() > 0);
    }

    #[test]
    fn options_reflect_the_selected_rule() {
        let mut m = model(&["a"], &[]);
        m.add_rule("number");
        m.set_text("digits", "5");
        m.set_flag("at-start", true);
        let digits = |m: &Rules| m.option_texts().into_iter().find(|(k, _)| *k == "digits").unwrap().1;
        let o = m.options();
        assert_eq!((o.kind.as_str(), digits(&m).as_str(), o.at_start), ("number", "5", true));
        m.set_text("digits", "x");
        assert_eq!(digits(&m), "5", "a bad number keeps the old one");
    }
}
```

(Bu adımda dosya yalnız `Rules` ve testleri içerir; `mod batch_rename;` satırını `main.rs`'e ekle.)

Run: `~/.cargo/bin/cargo test -p gezik batch_rename`
Expected: PASS

- [ ] **Step 5: Slint'e bağlayan `BatchRename`'i yaz**

Aynı dosyaya:

```rust
struct Inner {
    window: slint::Weak<AppWindow>,
    ops: crate::operations::Operations,
    rules: RefCell<Rules>,
    rows: Rc<VecModel<RenamePreviewRow>>,
    /// Over display positions (shown rows when "Only changed").
    selection: RefCell<gezik_core::selection::Selection>,
    /// Display positions on screen ("Only changed" hides some).
    shown: RefCell<Vec<usize>>,
    only_changed: std::cell::Cell<bool>,
    open: std::cell::Cell<bool>,
    timer: slint::Timer,
    /// The rule (and rule count) whose texts the fields show.
    shown_rule: std::cell::Cell<Option<(usize, usize)>>,
    /// Load the texts on the next `show_rules` (a preset or a mode replaced the rule).
    reload_texts: std::cell::Cell<bool>,
}

#[derive(Clone)]
pub struct BatchRename(Rc<Inner>);

impl BatchRename {
    pub fn new(window: &AppWindow, ops: crate::operations::Operations) -> BatchRename {
        let rows = Rc::new(VecModel::default());
        window.set_rb_rows(ModelRc::from(rows.clone()));
        let this = BatchRename(Rc::new(Inner {
            window: window.as_weak(),
            ops,
            rules: RefCell::default(),
            rows,
            selection: RefCell::new(gezik_core::selection::Selection::new(0)),
            shown: RefCell::default(),
            only_changed: Default::default(),
            open: Default::default(),
            timer: slint::Timer::default(),
            shown_rule: Default::default(),
            reload_texts: Default::default(),
        }));
        this.install(window);
        this
    }

    fn install(&self, window: &AppWindow) {
        let t = self.clone();
        window.on_rb_select_rule(move |i| t.edit(|r| r.selected_rule = usize::try_from(i).ok(), false));
        let t = self.clone();
        window.on_rb_toggle_rule(move |i| {
            t.edit(
                |r| {
                    if let Some(rule) = usize::try_from(i).ok().and_then(|i| r.rules.get_mut(i)) {
                        rule.enabled = !rule.enabled;
                    }
                },
                true,
            )
        });
        let t = self.clone();
        window.on_rb_move_rule(move |by| t.edit(|r| r.move_rule(by), true));
        let t = self.clone();
        window.on_rb_remove_rule(move || t.edit(Rules::remove_rule, true));
        let t = self.clone();
        window.on_rb_option_text(move |key, value| t.edit(|r| r.set_text(&key, &value), true));
        let t = self.clone();
        window.on_rb_option_flag(move |key, value| t.edit(|r| r.set_flag(&key, value), true));
        let t = self.clone();
        window.on_rb_option_mode(move |mode| {
            t.0.reload_texts.set(true);
            t.edit(|r| r.set_mode(mode), true)
        });
        let t = self.clone();
        window.on_rb_toggle_extension(move || t.edit(|r| r.include_extension = !r.include_extension, true));
        let t = self.clone();
        window.on_rb_toggle_only_changed(move || {
            t.0.only_changed.set(!t.0.only_changed.get());
            t.refresh();
        });
        let t = self.clone();
        window.on_rb_row_pressed(move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                t.pressed(row, ctrl, shift);
            }
        });
        let t = self.clone();
        window.on_rb_rename(move || t.rename());
        let t = self.clone();
        window.on_rb_cancel(move || t.close());
        let t = self.clone();
        window.on_rb_key(move |event| {
            let m = event.modifiers;
            t.key(&event.text, m.control, m.alt, m.shift, m.meta)
        });
        // Add rule ▾ and Presets ▾ open Slint menus through context_menu.rs (Task 8 for presets).
    }

    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }

    /// Opens the layer for `items` (paths and whether each is a folder), in list order.
    /// `others`: the folder's other names; `last`: the rules used last time.
    pub fn open(&self, items: Vec<(PathBuf, bool)>, others: Vec<String>, last: BatchRenameState) {
        let Some(window) = self.0.window.upgrade() else { return };
        let selected: HashSet<String> =
            items.iter().filter_map(|(p, _)| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
        let lang = Lang::from_code(&gezik_platform::language());
        let list: Vec<Item> = items
            .iter()
            .map(|(path, is_dir)| {
                let meta = std::fs::symlink_metadata(path).ok();
                Item {
                    name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    is_dir: *is_dir,
                    parent: path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    folder: 0,
                    size: meta.as_ref().map_or(0, |m| m.len()),
                    modified: meta.and_then(|m| m.modified().ok()).and_then(gezik_platform::local_date_parts),
                    taken: None,
                }
            })
            .collect();
        {
            let mut r = self.0.rules.borrow_mut();
            *r = Rules {
                order: (0..list.len()).collect(),
                paths: items.into_iter().map(|(p, _)| p).collect(),
                items: list,
                others: others.into_iter().filter(|n| !selected.contains(n)).collect(),
                include_extension: last.include_extension,
                selected_rule: if last.rules.is_empty() { None } else { Some(0) },
                rules: last.rules,
                lang,
                ..Rules::default()
            };
            if r.rules.is_empty() {
                r.add_rule("replace");
            }
        }
        let count = self.0.rules.borrow().items.len();
        window.set_rb_title(format!("Rename {count} items").into());
        window.set_rb_only_changed(false);
        self.0.only_changed.set(false);
        self.0.shown_rule.set(None);
        self.0.reload_texts.set(true);
        window.set_rb_scroll(0.0);
        self.0.open.set(true);
        self.recompute();
        window.set_rb_open(true);
    }

    pub fn close(&self) {
        self.0.open.set(false);
        self.0.timer.stop();
        self.0.rows.clear();
        if let Some(window) = self.0.window.upgrade() {
            window.set_rb_open(false);
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }

    /// What to keep for next time (state.toml).
    pub fn state(&self) -> BatchRenameState {
        let r = self.0.rules.borrow();
        BatchRenameState { include_extension: r.include_extension, rules: r.rules.clone() }
    }

    /// Changes the rules; `later`: the preview follows 50 ms after the last change (typing).
    fn edit(&self, f: impl FnOnce(&mut Rules), later: bool) {
        f(&mut self.0.rules.borrow_mut());
        self.show_rules();
        if later {
            let t = self.clone();
            self.0.timer.start(slint::TimerMode::SingleShot, Duration::from_millis(50), move || t.recompute());
        }
    }

    fn show_rules(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let r = self.0.rules.borrow();
        window.set_rb_rules(ModelRc::new(VecModel::from(r.rule_rows())));
        window.set_rb_selected_rule(r.selected_rule.map_or(-1, |i| i as i32));
        window.set_rb_options(r.options());
        window.set_rb_include_extension(r.include_extension);
        // The text fields are loaded only when another rule shows: rewriting the one being
        // typed in would move its caret.
        let showing = r.selected_rule.map(|i| (i, r.rules.len()));
        if self.0.shown_rule.replace(showing) != showing || self.0.reload_texts.replace(false) {
            for (key, value) in r.option_texts() {
                let value = value.into();
                match key {
                    "find" => window.set_rb_opt_find(value),
                    "with" => window.set_rb_opt_with(value),
                    "start" => window.set_rb_opt_start(value),
                    "step" => window.set_rb_opt_step(value),
                    "digits" => window.set_rb_opt_digits(value),
                    "separator" => window.set_rb_opt_separator(value),
                    "prefix" => window.set_rb_opt_prefix(value),
                    "suffix" => window.set_rb_opt_suffix(value),
                    _ => window.set_rb_opt_text(value),
                }
            }
        }
    }

    fn recompute(&self) {
        self.0.rules.borrow_mut().recompute();
        self.show_rules();
        self.refresh();
    }

    fn refresh(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let r = self.0.rules.borrow();
        let only_changed = self.0.only_changed.get();
        let shown: Vec<usize> = (0..r.order.len())
            .filter(|&p| !only_changed || r.statuses.get(p).is_some_and(|s| *s != Status::Unchanged))
            .collect();
        let mut selection = self.0.selection.borrow_mut();
        if selection.len() != shown.len() {
            *selection = gezik_core::selection::Selection::new(shown.len());
        }
        let rows: Vec<RenamePreviewRow> = shown
            .iter()
            .enumerate()
            .filter_map(|(row, &p)| r.preview_row(p, selection.is_selected(row)))
            .collect();
        self.0.rows.set_vec(rows);
        *self.0.shown.borrow_mut() = shown;
        window.set_rb_footer(r.footer().into());
        window.set_rb_can_rename(r.can_rename());
        window.set_rb_only_changed(only_changed);
    }

    fn pressed(&self, row: usize, ctrl: bool, shift: bool) {
        {
            let mut selection = self.0.selection.borrow_mut();
            match (ctrl, shift) {
                (_, true) => selection.extend_to(row, ctrl),
                (true, false) => selection.toggle(row),
                (false, false) => selection.select_only(row),
            };
        }
        self.refresh();
    }

    fn rename(&self) {
        let pairs = {
            let mut r = self.0.rules.borrow_mut();
            r.recompute();
            if !r.can_rename() {
                return;
            }
            r.pairs()
        };
        let state = self.state();
        self.close();
        crate::operations::with_current(|ops| ops.save_batch_rename(state));
        self.0.ops.submit(Box::new(gezik_ops::RenameTask::many(pairs)), None, crate::operations::After::Select);
    }

    /// A key while the layer has the keyboard; returns whether it was used.
    fn key(&self, text: &str, control: bool, alt: bool, shift: bool, meta: bool) -> bool {
        use gezik_config::shortcuts::{Key, Platform};
        let platform = Platform::current();
        let Some(chord) = crate::keys::chord_from_slint(text, control, alt, shift, meta, platform) else { return false };
        let primary = crate::keys::is_primary(&chord, platform);
        match chord.key {
            Key::Escape if !primary && !chord.shift => self.close(),
            Key::Enter if primary => self.rename(),
            _ => return false,
        }
        true
    }
}
```

`Selection`'ın `len()` yöntemi yoksa `crates/gezik-core/src/selection.rs`'te `pub fn len(&self) -> usize` ekle (var olan iç uzunluğu döndürür) ve testine bir satır ekle.

- [ ] **Step 6: `Operations`'a açılış ve son kuralları ekle**

`crates/gezik/src/operations.rs`:
- `Inner`'a `batch_last: RefCell<BatchRenameState>` ve `store: Option<ConfigStore>` (Operations::new'e `store: Option<gezik_config::store::ConfigStore>` ve `batch_last: BatchRenameState` parametresi; `main.rs`'te `config.clone()` ve `saved_state.batch_rename.clone().unwrap_or_default()` geçir).
- `rename_start`'ı değiştir:

```rust
    /// F2: renames the selected entry in place, or opens batch rename for two or more.
    pub fn rename_start(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        let view = &self.0.view;
        if view.selection_count() >= 2 {
            self.batch_rename();
            return;
        }
        if let Some(index) = view.single_selected().or_else(|| view.focus()) {
            view.begin_rename(index);
        }
    }

    /// The batch rename layer for the selection (also for one item: the batch-rename shortcut).
    pub fn batch_rename(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        let mut items = self.0.view.selected_items();
        if items.is_empty() {
            items.extend(self.0.view.focus().and_then(|i| self.0.view.entry_path(i)));
        }
        if items.is_empty() {
            return;
        }
        let others = self.0.view.all_names();
        let last = self.0.batch_last.borrow().clone();
        crate::batch_rename::with_current(|layer| layer.open(items, others, last));
    }

    /// The rules used last; written to state.toml now (the layer may be used once per run).
    pub fn save_batch_rename(&self, state: BatchRenameState) {
        *self.0.batch_last.borrow_mut() = state.clone();
        if let Some(store) = &self.0.store {
            let mut saved = store.load_state();
            saved.batch_rename = Some(state);
            if let Err(err) = store.save_state(&saved) {
                eprintln!("gezik: cannot save the rename rules: {err}");
            }
        }
    }
```

`batch_rename.rs`'e, `operations.rs`'teki `CURRENT` kalıbıyla aynı biçimde:

```rust
thread_local! {
    static CURRENT: RefCell<Option<BatchRename>> = const { RefCell::new(None) };
}

pub fn with_current(f: impl FnOnce(&BatchRename)) {
    if let Some(layer) = CURRENT.with(|c| c.borrow().clone()) {
        f(&layer);
    }
}
```

ve `BatchRename::new`'in sonunda `CURRENT.with(|c| *c.borrow_mut() = Some(this.clone()));`.

`main.rs`'te `Operations::new`'den sonra `let batch_rename = batch_rename::BatchRename::new(&window, ops.clone());`; `handle_key`'in başındaki `if window.get_dialog_open() || window.get_conflicts_open()` koşuluna `|| window.get_rb_open()` ekle; `Action::BatchRename => ops.batch_rename(),` (Görev 6'daki geçici satırı değiştir). `save_and_quit` içinde `state.batch_rename` ayrıca yazılmaz (`save_batch_rename` yazıyor; `store.load_state()` onu zaten okuyor).

- [ ] **Step 7: Derle, testler, uygulamayı aç ve elle dene**

```bash
~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
~/.cargo/bin/cargo check -p gezik --target aarch64-apple-darwin
```

Elle: geçici klasörde 5 dosya, `GEZIK_CONFIG_DIR` geçici klasör, `cargo run -p gezik -- <klasör>`; hepsini seç, F2 → katman açılır; Replace kuralı yaz → önizleme değişir; Rename → dosyalar yeniden adlanır; Ctrl+Z → eski adlar.

- [ ] **Step 8: exe boyutunu ölç**

```bash
git stash -q && ~/.cargo/bin/cargo build --release -p gezik && ls -l target/release/gezik.exe | awk '{print $5}' > "$TEMP/size-before" && git stash pop -q
~/.cargo/bin/cargo build --release -p gezik && ls -l target/release/gezik.exe | awk '{print $5}'
cat "$TEMP/size-before"
```

(`git stash` yalnız çalışma ağacındaki değişiklikleri saklar; bu görevin değişiklikleri commit'lenmeden önce ölç ya da `git worktree` ile master'ı ayrı derle: `git worktree add "$TEMP/gezik-base" master && (cd "$TEMP/gezik-base" && ~/.cargo/bin/cargo build --release -p gezik --target-dir "$TEMP/gezik-base-target")`.) Fark > 512 KB ise sapma 9'daki sırayla regex özelliklerini daralt, yeniden ölç; sonucu commit mesajına yaz.

- [ ] **Step 9: Commit**

```bash
git add crates/gezik crates/gezik-core/src/selection.rs
git commit -m "Add the batch rename layer: rules, live preview, warnings and one undoable rename"
```

---

### Task 8: Arayüz — elle düzenleme, sürükleyerek sıralama, kural setleri, EXIF, menüler

**Files:**
- Modify: `crates/gezik/src/batch_rename.rs`, `crates/gezik/ui/widgets/rename-batch.slint`, `crates/gezik/src/context_menu.rs`, `crates/gezik/src/main.rs`, `crates/gezik-platform/src/shell_menu.rs` (gerekirse), `crates/gezik/src/operations.rs`

**Interfaces:**
- Consumes: Task 7 `BatchRename`, `Rules`; Task 6 `ConfigStore::save_rename_presets`, `Settings::rename_presets`; Task 4 `gezik_batch::exif::{taken, may_have_exif}`
- Produces: `BatchRename::set_presets(Vec<RenamePreset>)`; `Rules::reorder(from, to)`, `Rules::set_manual(position, name)`, `Rules::reset_manual(positions)`; context menu ids 90-99 (`ADD_RULE_FIRST` .. ), 100-149 presets

- [ ] **Step 1: `Rules`'a elle ad ve sıralamayı testleriyle ekle**

```rust
impl Rules {
    /// Display position `from` moves to `to`.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.order.len() || to >= self.order.len() || from == to {
            return;
        }
        let item = self.order.remove(from);
        self.order.insert(to, item);
    }

    /// The name at display position `position`, typed by hand; the rules' name clears it.
    pub fn set_manual(&mut self, position: usize, name: &str) {
        let Some(&i) = self.order.get(position) else { return };
        let ruled = self.new.get(position).cloned().unwrap_or_default();
        self.manual.remove(&i);
        if name != ruled {
            // What the rules would give without the manual name.
            self.recompute();
            if self.new.get(position).is_none_or(|ruled| ruled != name) {
                self.manual.insert(i, name.to_owned());
            }
        }
    }

    pub fn reset_manual(&mut self, positions: &[usize]) {
        for &p in positions {
            if let Some(i) = self.order.get(p) {
                self.manual.remove(i);
            }
        }
    }

    /// Back to the list's order (the sort buttons).
    pub fn reset_order(&mut self) {
        self.order = (0..self.items.len()).collect();
    }
}
```

Testler:

```rust
    #[test]
    fn numbers_follow_a_dragged_order() {
        let mut m = model(&["a", "b", "c"], &[]);
        m.add_rule("number");
        m.set_text("digits", "1");
        m.set_text("separator", "");
        m.recompute();
        assert_eq!(m.new, ["a1", "b2", "c3"]);
        m.reorder(2, 0);
        m.recompute();
        assert_eq!(m.new, ["c1", "a2", "b3"]);
        assert_eq!(m.pairs()[0], (PathBuf::from("/d/c"), PathBuf::from("/d/c1")));
    }

    #[test]
    fn a_manual_name_wins_until_reset() {
        let mut m = model(&["a.txt", "b.txt"], &[]);
        m.add_rule("case");
        m.set_mode(1);
        m.recompute();
        m.set_manual(1, "Kapak.txt");
        m.recompute();
        assert_eq!(m.new, ["A.txt", "Kapak.txt"]);
        assert!(m.preview_row(1, false).unwrap().manual);
        m.set_manual(1, "B.txt");
        assert!(m.manual.is_empty(), "typing what the rules give is no manual name");
        m.set_manual(1, "x.txt");
        m.reset_manual(&[1]);
        m.recompute();
        assert_eq!(m.new[1], "B.txt");
    }
```

- [ ] **Step 2: Slint'te ≡ ile sürükleme ve ad alanı**

`rename-batch.slint`'te `handle := Rectangle { ... }` içine:

```slint
                                    TouchArea {
                                        mouse-cursor: grab;
                                        moved => {
                                            if self.pressed {
                                                root.drag-to = clamp(floor((line.y + self.mouse-y + root.scroll) / Theme.row-height), 0, root.rows.length - 1);
                                                root.drag-from = i;
                                            }
                                        }
                                        changed pressed => {
                                            if !self.pressed && root.drag-from >= 0 {
                                                if root.drag-to != root.drag-from { root.reorder(root.drag-from, root.drag-to); }
                                                root.drag-from = -1;
                                            }
                                        }
                                    }
```

ve bileşene `property <int> drag-from: -1; property <int> drag-to: -1;`; satır arka planında sürüklenen hedefin üstüne çizgi:

```slint
                            if root.drag-from >= 0 && root.drag-to == i && root.drag-to != root.drag-from: Rectangle {
                                y: root.drag-to < root.drag-from ? 0 : parent.height - 2px;
                                height: 2px;
                                background: Theme.accent;
                            }
```

Not: `line.y` liste içeriğinin koordinatıdır; `root.scroll` (`content-y`) negatif olduğu için toplama değil çıkarma gerekebilir — elle deneyip `(line.y + self.mouse-y) / Theme.row-height` (scroll'suz) doğru indeksi veriyorsa onu kullan; satırın `y`'si içerik koordinatında olduğu için scroll eklemek gerekmez.

Görev 7'de `name-text` ve `name-edited` zaten bağlı. Seçim değişince `name-text` güncellenir (Step 3).

- [ ] **Step 3: Denetleyicide elle ad, sıralama, EXIF, sıralama düğmeleri**

`BatchRename::install`'a:

```rust
        let t = self.clone();
        window.on_rb_reorder(move |from, to| {
            if let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) {
                let shown = t.0.shown.borrow().clone();
                if let (Some(&f), Some(&to)) = (shown.get(from), shown.get(to)) {
                    t.0.rules.borrow_mut().reorder(f, to);
                    t.recompute();
                }
            }
        });
        let t = self.clone();
        window.on_rb_name_edited(move |text| {
            let Some(position) = t.focused_position() else { return };
            t.0.rules.borrow_mut().set_manual(position, &text);
            let t2 = t.clone();
            t.0.timer.start(slint::TimerMode::SingleShot, Duration::from_millis(50), move || t2.recompute());
        });
        let t = self.clone();
        window.on_rb_name_reset(move || {
            let positions = t.selected_positions();
            t.0.rules.borrow_mut().reset_manual(&positions);
            t.recompute();
            t.show_name();
        });
```

ve yardımcılar:

```rust
    /// Display positions of the selected rows (the focused one if none).
    fn selected_positions(&self) -> Vec<usize> {
        let shown = self.0.shown.borrow();
        let selection = self.0.selection.borrow();
        let mut rows: Vec<usize> = selection.iter().collect();
        if rows.is_empty() {
            rows.extend(selection.focus());
        }
        rows.into_iter().filter_map(|row| shown.get(row).copied()).collect()
    }

    fn focused_position(&self) -> Option<usize> {
        let row = self.0.selection.borrow().focus()?;
        self.0.shown.borrow().get(row).copied()
    }

    /// The name field shows the focused row's new name.
    fn show_name(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let r = self.0.rules.borrow();
        match self.focused_position().and_then(|p| r.new.get(p)) {
            Some(name) => {
                window.set_rb_name_text(name.clone().into());
                window.set_rb_name_enabled(true);
            }
            None => {
                window.set_rb_name_text("".into());
                window.set_rb_name_enabled(false);
            }
        }
    }
```

`pressed` sonunda `self.show_name();` çağır. `key`'e: `Key::F2 if !primary => window.invoke_rb_focus_name()` (pencereyi `self.0.window.upgrade()` ile al), ok tuşları (conflicts.rs'teki `Key::Up | Key::Down | ...` bloğunu aynen uyarla: `self.0.selection`, `self.0.shown.borrow().len()`, `invoke_rb_ensure_visible`, sonra `show_name()`), `Key::Char('a') if primary` hepsini seçer.

**EXIF:** `open()`'ın sonunda, kurallar `{taken}` kullanıyorsa ya da kullanmaya başladığında okunur. `recompute()`'ta `compile` sonrası `needs_taken()` true ve okuma başlamadıysa:

```rust
    fn read_taken(&self) {
        if self.0.reading.replace(true) {
            return;
        }
        let paths: Vec<(usize, PathBuf)> = self
            .0
            .rules
            .borrow()
            .paths
            .iter()
            .enumerate()
            .filter(|(_, p)| p.file_name().is_some_and(|n| gezik_batch::exif::may_have_exif(&n.to_string_lossy())))
            .map(|(i, p)| (i, p.clone()))
            .collect();
        let weak = self.0.window.clone();
        std::thread::spawn(move || {
            let dates: Vec<(usize, gezik_core::batch::date::DateParts)> =
                paths.into_iter().filter_map(|(i, p)| gezik_batch::exif::taken(&p).map(|d| (i, d))).collect();
            let _ = weak.upgrade_in_event_loop(move |_| {
                with_current(|layer| {
                    {
                        let mut r = layer.0.rules.borrow_mut();
                        for (i, date) in dates {
                            if let Some(item) = r.items.get_mut(i) {
                                item.taken = Some(date);
                            }
                        }
                    }
                    layer.0.taken_ready.set(true);
                    layer.recompute();
                });
            });
        });
    }
```

`Inner`'a `reading: Cell<bool>` ve `taken_ready: Cell<bool>` ekle (`open`'da ikisi de false). `Rules::recompute`'a `compiled.needs_taken()` sonucunu `pub needs_taken: bool` alanına yaz; `BatchRename::recompute` sonrası `if r.needs_taken && !reading { self.read_taken() }`. Okuma bitene kadar `{taken}` modified tarihine düşer; altbilgiye `"reading photo dates…"` ekle ve Rename kapalı kalsın: `Rules::can_rename`'e bir `pub waiting: bool` alanı ekle (`needs_taken && !taken_ready`), `footer` başına "reading photo dates…" yazsın. Test:

```rust
    #[test]
    fn waits_for_photo_dates() {
        let mut m = model(&["a.jpg"], &[]);
        m.add_rule("template");
        m.set_text("text", "{taken}");
        m.waiting = true;
        m.recompute();
        assert!(!m.can_rename());
        assert!(m.footer().starts_with("reading photo dates"), "{}", m.footer());
    }
```

- [ ] **Step 4: "Add rule ▾" ve "Presets ▾" menüleri**

`crates/gezik/src/context_menu.rs`:

```rust
/// 90-99: "Add rule" in the batch rename layer, in `gezik_core::batch::rules::KINDS` order.
pub const ADD_RULE_FIRST: u32 = 90;
/// 100-149: saved rule sets; 150 "Save current rules as…", 151 "Delete preset ▸" entries are 160-199.
pub const PRESET_FIRST: u32 = 100;
pub const PRESET_SAVE: u32 = 150;
pub const PRESET_DELETE_FIRST: u32 = 160;
```

`Subject`'e `BatchRename` kolu ekle. `Menus`'e:

```rust
    /// "Add rule ▾" of the batch rename layer.
    pub fn add_rule(&self, x: f32, y: f32) {
        let list: Vec<(u32, &str)> = gezik_core::batch::rules::KINDS
            .iter()
            .enumerate()
            .map(|(i, (_, label))| (ADD_RULE_FIRST + i as u32, *label))
            .collect();
        *self.subject.borrow_mut() = Some(Subject::BatchRename);
        self.open_slint(&list, x, y);
    }

    /// "Presets ▾": the saved sets, save, delete.
    pub fn presets(&self, x: f32, y: f32) {
        let names = crate::batch_rename::preset_names();
        let mut list: Vec<(u32, String)> =
            names.iter().enumerate().take(50).map(|(i, n)| (PRESET_FIRST + i as u32, n.clone())).collect();
        list.push((PRESET_SAVE, "Save current rules as…".to_owned()));
        list.extend(names.iter().enumerate().take(40).map(|(i, n)| (PRESET_DELETE_FIRST + i as u32, format!("Delete \"{n}\""))));
        *self.subject.borrow_mut() = Some(Subject::BatchRename);
        self.open_slint(&list, x, y);
    }
```

`run`'a:

```rust
            (id, Subject::BatchRename) if (ADD_RULE_FIRST..ADD_RULE_FIRST + 10).contains(&id) => {
                if let Some((kind, _)) = gezik_core::batch::rules::KINDS.get((id - ADD_RULE_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.add_rule(kind));
                }
            }
            (id, Subject::BatchRename) if (PRESET_FIRST..PRESET_SAVE).contains(&id) => {
                crate::batch_rename::with_current(|layer| layer.apply_preset((id - PRESET_FIRST) as usize));
            }
            (PRESET_SAVE, Subject::BatchRename) => crate::batch_rename::with_current(|layer| layer.ask_preset_name()),
            (id, Subject::BatchRename) if id >= PRESET_DELETE_FIRST && id < PRESET_DELETE_FIRST + 40 => {
                crate::batch_rename::with_current(|layer| layer.delete_preset((id - PRESET_DELETE_FIRST) as usize));
            }
```

`main.rs`'te `window.on_rb_add_rule(move |x, y| menus.add_rule(x, y))` ve `window.on_rb_presets(move |x, y| menus.presets(x, y))` (var olan `menus` klonlama kalıbıyla).

`batch_rename.rs`'e kural setleri:

```rust
thread_local! {
    static PRESETS: RefCell<Vec<gezik_config::settings::RenamePreset>> = const { RefCell::new(Vec::new()) };
}

/// settings.toml changed: the saved sets.
pub fn set_presets(presets: Vec<gezik_config::settings::RenamePreset>) {
    PRESETS.with(|p| *p.borrow_mut() = presets);
}

pub fn preset_names() -> Vec<String> {
    PRESETS.with(|p| p.borrow().iter().map(|p| p.name.clone()).collect())
}

impl BatchRename {
    pub fn add_rule(&self, kind: &str) {
        self.edit(|r| r.add_rule(kind), true);
    }

    pub fn apply_preset(&self, index: usize) {
        let Some(preset) = PRESETS.with(|p| p.borrow().get(index).cloned()) else { return };
        self.0.reload_texts.set(true);
        self.edit(
            |r| {
                r.rules = preset.rules;
                r.include_extension = preset.include_extension;
                r.selected_rule = if r.rules.is_empty() { None } else { Some(0) };
            },
            true,
        );
    }

    /// Asks for a name over the layer, then saves the current rules under it.
    pub fn ask_preset_name(&self) {
        crate::operations::with_current(|ops| {
            let t = self.clone();
            ops.ask_text("Save rules as", "Name for this set of rules:", move |name| t.save_preset(name));
        });
    }

    fn save_preset(&self, name: String) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let state = self.state();
        let mut presets = PRESETS.with(|p| p.borrow().clone());
        let preset = gezik_config::settings::RenamePreset { name: name.clone(), include_extension: state.include_extension, rules: state.rules };
        match presets.iter_mut().find(|p| p.name == name) {
            Some(old) => *old = preset,
            None => presets.push(preset),
        }
        self.write_presets(presets);
    }

    pub fn delete_preset(&self, index: usize) {
        let mut presets = PRESETS.with(|p| p.borrow().clone());
        if index < presets.len() {
            presets.remove(index);
            self.write_presets(presets);
        }
    }

    fn write_presets(&self, presets: Vec<gezik_config::settings::RenamePreset>) {
        set_presets(presets.clone());
        crate::operations::with_current(|ops| ops.save_rename_presets(&presets));
    }
}
```

`main.rs` `apply_config`'te: `batch_rename::set_presets(loaded.settings.rename_presets.clone());`.

`operations.rs`'e:

```rust
    pub fn save_rename_presets(&self, presets: &[gezik_config::settings::RenamePreset]) {
        if let Some(store) = &self.0.store
            && let Err(warning) = store.save_rename_presets(presets)
        {
            self.0.view.note(warning.to_string());
        }
    }
```

**Metin sorusu (`ask_text`):** `dialog.rs`'in `Dialogs` yalnız düğme soruyor. `Dialog` bileşenine isteğe bağlı bir metin alanı ekle: `in property <bool> has-input; in-out property <string> input;` (Dialog Slint'inde mesajın altında `if root.has-input: TextField { text <=> root.input; accepted => { root.chosen(0); } }`), `Dialogs::ask_text(title, message, initial, answer: FnOnce(Option<String>))` ekle (soru kuyruğuna `input: Option<String>` alanı; `chosen(0)` = OK, metni `window.get_dialog_input()` ile oku). `AppWindow`'a `in property <bool> dialog-has-input; in-out property <string> dialog-input;` ve `Dialog { has-input: root.dialog-has-input; input <=> root.dialog-input; }`. `Operations::ask_text(title, message, f: impl FnOnce(String) + 'static)` düğmeler `["Save", "Cancel"]`, Esc = 1, OK'te `f(text)`. Diyalog açılınca metin alanı odak alır (`changed open` benzeri `init => { field.focus-and-select(); }` alanda).

- [ ] **Step 5: Menüler — "Rename N items…"**

- macOS/Linux: `context_menu::file_items(single, folder, can_paste)` içinde `if single { out.push((RENAME, "Rename")) }` satırını şöyle değiştir: `out.push((RENAME, if single { "Rename" } else { "Rename items…" }));`. `run`'daki `(RENAME, Subject::Row(_))` kolunu `(RENAME, Subject::Row(_) | Subject::Rows(_)) => self.ops.rename_start(),` yap.
- Windows: Shell menüsünün kendi "Rename"i çoklu seçimde `can_rename` false olduğu için yok. Gezik öğesi ekle: satır menüsü için `open(...)`'a geçirilen `items` listesine (Windows yolunda üstte gösterilen Gezik öğeleri) seçim ≥ 2 ise `(BATCH_RENAME, format!("Rename {n} items…"))` ekle; `pub const BATCH_RENAME: u32 = 63;` ve `run`'da `(BATCH_RENAME, Subject::Rows(_)) => self.ops.batch_rename(),`. Satır menüsünün öğeleri nerede kuruluyorsa (`row` fonksiyonu; `items(place, ...)`) orayı oku ve aynı yere ekle; macOS/Linux'ta `file_items` zaten "Rename items…" gösterdiği için `BATCH_RENAME` yalnız `cfg!(windows)` iken eklenir.
- Test (`context_menu.rs` testlerine): `file_items(false, false, false)` "Rename items…" içerir.

- [ ] **Step 6: Derle, test, elle dene**

```bash
~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
~/.cargo/bin/cargo check -p gezik --target aarch64-apple-darwin
```

Elle: ≡ ile sürükle → numaralar yeni sırayı izler; satır seç, ad alanına yaz → ✎; Reset to rules; Presets ▾ › Save current rules as… → ad sor, `settings.toml`'da `[[rename-presets]]` görünür; yeniden aç → Presets'te görünür, seçince kurallar gelir; `{taken}` şablonu, EXIF'li fotoğraf (tests/data/exif.jpg kopyaları) → 2024-07-01; sağ tık (Windows) "Rename 3 items…".

- [ ] **Step 7: Commit**

```bash
git add crates/gezik crates/gezik-platform
git commit -m "Edit names by hand, drag the order, save rule sets and read photo dates in batch rename"
```

---

### Task 9: Ölçüm, notlar ve son denetim

**Files:**
- Create: `scripts/perf/batch.ps1`
- Modify: `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md`, `docs/superpowers/specs/2026-10-05-toplu-islemler-design.md` (Durum)

- [ ] **Step 1: Önizleme ve uygulama ölçümü**

`gezik-batch`'e sürüm derlemesinde çalışan bir ölçüm örneği ekle: `crates/gezik-batch/examples/rename_bench.rs`:

```rust
//! cargo run --release -p gezik-batch --example rename_bench
use gezik_batch::rename::{Item, check, compile, new_names};
use gezik_core::batch::case::{CaseMode, Lang};
use gezik_core::batch::rules::{NumberRule, ReplaceRule, Rule, RuleEntry};
use gezik_core::ops::names::NameRules;

fn main() {
    let list: Vec<Item> = (0..1000).map(|n| Item { name: format!("IMG_{n:04}.jpg"), ..Item::default() }).collect();
    let rules = vec![
        RuleEntry::new(Rule::Replace(ReplaceRule { find: r"IMG_(\d+)".into(), with: "Tatil $1".into(), regex: true, case_sensitive: false, all: true })),
        RuleEntry::new(Rule::Number(NumberRule::default())),
        RuleEntry::new(Rule::Case(CaseMode::Title)),
    ];
    let mut best = std::time::Duration::MAX;
    for _ in 0..20 {
        let start = std::time::Instant::now();
        let compiled = compile(&rules);
        let new = new_names(&list, &compiled, false, Lang::Turkic);
        let _ = check(&list, &new, &|_, _| false, NameRules::Windows, true);
        best = best.min(start.elapsed());
    }
    println!("1000 items, 3 rules: {:.2} ms", best.as_secs_f64() * 1000.0);
}
```

`scripts/perf/batch.ps1`: 10.000 boş dosyalık geçici klasör üretir, `cargo run --release -p gezik-ops --example rename_many -- <klasör>` ile tek `RenameTask::many` işinin süresini ölçer, temizler. Örnek `crates/gezik-ops/examples/rename_many.rs`:

```rust
//! cargo run --release -p gezik-ops --example rename_many -- <folder with files>
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("a folder"));
    let pairs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| { let p = e.path(); let new = p.with_file_name(format!("r-{}", e.file_name().to_string_lossy())); (p, new) })
        .collect();
    let engine = gezik_ops::Engine::new(gezik_ops::Settings::default(), || {});
    let start = std::time::Instant::now();
    let job = engine.submit(Box::new(gezik_ops::RenameTask::many(pairs)));
    loop {
        if engine.drain().iter().any(|e| matches!(e, gezik_ops::Event::Finished { job: j, .. } if *j == job)) { break; }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    println!("renamed in {} ms", start.elapsed().as_millis());
}
```

```powershell
# scripts/perf/batch.ps1 — batch rename timings (5a). Usage: .\scripts\perf\batch.ps1
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:TEMP "gezik-perf-rename"
Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
New-Item -ItemType Directory $dir | Out-Null
1..10000 | ForEach-Object { [IO.File]::WriteAllBytes((Join-Path $dir ("IMG_{0:D5}.jpg" -f $_)), [byte[]]@()) }
cargo run --release -q -p gezik-batch --example rename_bench
cargo run --release -q -p gezik-ops --example rename_many -- $dir
Remove-Item -Recurse -Force $dir
```

Çalıştır: `powershell -File scripts/perf/batch.ps1`. Hedefler: önizleme ≤ 50 ms, 10.000 öğe ≤ 2 sn. Sonuçları not dosyasına yaz.

- [ ] **Step 2: Notlar ve spec durumu**

`docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` sonuna yeni bölüm:

```markdown
## Alt proje 5a (Toplu yeniden adlandırma) sonrası

- Ölçümler (sürüm derlemesi, Windows 11): önizleme 1.000 öğe 3 kural <ölçülen> ms (hedef ≤ 50); 10.000 öğe uygulama <ölçülen> ms (hedef ≤ 2 sn); exe <önce> → <sonra> MB (hedef ≤ +0,5 MB).
- Sapmalar: `RenameTask` `gezik-ops`'ta (geri alma döngüleri sıralayabilsin); elle ad önizleme hücresinde değil altındaki alanda; sürükleyerek sıralama Slint içinde.
- macOS/Linux: derlendi, ekranda denenmedi.
```

Spec'te `**Durum:**` satırını `Tasarım onaylandı (2026-10-05); 5a uygulandı` yap.

- [ ] **Step 3: Son tam denetim ve commit**

```bash
~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
~/.cargo/bin/cargo check -p gezik --target aarch64-apple-darwin
~/.cargo/bin/cargo check -p gezik-core -p gezik-platform -p gezik-ops -p gezik-batch --target x86_64-unknown-linux-gnu
git add scripts crates/gezik-batch/examples crates/gezik-ops/examples docs
git commit -m "Measure batch rename and note what 5a does"
```

---

## Ekran testleri (alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici, test verisi `%TEMP%\gezik-gui-5a\` (20 dosya `IMG_0001.jpg`…, 3 alt klasör, `a.txt`/`b.txt` takası için, 3 kopya `exif.jpg`). Sürüm derlemesi.

1. 5 dosya seç, F2 → "Rename 5 items" katmanı; tek dosyada F2 → yerinde yeniden adlandırma (değişmedi).
2. Replace `IMG_` → `Tatil ` → önizleme anında; Rename → diskte yeni adlar; panelde/Undo menüsünde "Rename 5 items"; Ctrl+Z → eski adlar.
3. Number (3 hane, sonda) + Case UPPER + Include extension açık/kapalı → önizleme doğru.
4. Regex `(` → kural satırı kırmızı, hata metni, Rename kapalı.
5. Template `x` → "5 duplicate names", satırlarda ⚠; tek satırı elle `y.jpg` → ✎, uyarı azalır; Reset to rules.
6. a.txt ↔ b.txt takası (iki satırı elle) → Rename → içerikler yer değiştirdi; Ctrl+Z → geri.
7. ≡ ile son satırı başa sürükle → numaralar yeni sırayı izler.
8. Presets › Save → ad → settings.toml'da set; katmanı kapat/aç → Presets'te seç → kurallar gelir; Delete.
9. `{taken:%Y-%m-%d}` → exif.jpg kopyaları 2024-07-01; diğerleri değişme tarihi.
10. Esc kapatır, odak listeye döner; Ctrl+Enter uygular.
11. Windows sağ tık çoklu seçim → "Rename 3 items…" katmanı açar.
12. Uygulama kapat/aç → son kurallar geri gelir (`state.toml` `[batch-rename]`).
