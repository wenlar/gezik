# Taşınabilirlik 14a (Ayar penceresi) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** `settings.toml`'un tekil ayarlarını ayrı bir pencerede göstermek ve değiştirmek (`Settings…`, Windows/Linux `Ctrl+,`, macOS `⌘,`): solda sekiz sayfa, üstte arama, sağda satırlar; değişiklik anında, mevcut yazıcı kuyruğundan (`SettingsWriter`), yalnız o satırı değiştirerek yazılır. Pencere ilk açılışta kurulur, kapanınca bırakılır; boşta bellek +0; exe ≤ taban + 262.144 B (taban `feat/tags-12f` `t12f.exe`: 27.705.344 B).

**Spec:** `docs/superpowers/specs/2026-10-12-tasinabilirlik-design.md` §3.2–§3.5, §4, §10, §13.2 14a. Plan önceden onaylıdır. Dal `feat/portable-14a` (`feat/tags-12f` üstüne, `docs/step-14-spec` birleştirildi), `D:\Work\gezik-14a`.

## Kararlar (soru sorulmadı)

1. **Gerçek sayılar:** taban 12f; `Action::ALL` 124 → 125 (`settings`), menü kimliği yalnız `SETTINGS` 2800 ve satır menüsünün 2810–2812'si (Reset to Default, Copy Key, Edit in settings.toml); denetim listeleri macOS 365–368, Linux 266–267 (spec 13'ün de birleşeceğini varsayıp 388/282 diyordu).
2. **`Ctrl+,`:** `Key::parse` virgülü tanımıyordu: `,` ve `comma` eklendi; `chord_from_event` de `,`'yı alır. Varsayılan `mod+,`, başka eylemde ve `fixed_owner`'da yok (test).
3. **Yazma:** `SettingsChange::Value { table, key, value: Option<String> }`; değer TOML metni (`"compact"`, `true`), `None` anahtarı çıkarır (yalnız şablonda yorum olarak duran `show-hidden`'ın Reset'i). `settings_edit::with_value` var olan `edit_table`'ı kullanır; üst düzey anahtar ve satır içi tablo/noktalı anahtar yerinde düzenlenir (salt okunur yapılmadı: bayt testleri yerinde kaldığını gösteriyor).
4. **Satır sonları:** toml_edit her satır sonunu `\n` yazıyordu; `\r\n`'li (Not Defteri) dosyada her yazma bütün dosyayı değiştiriyordu. `SettingsChange::apply` artık dosyanın `\r\n`'sini korur (bütün değişiklik türleri için).
5. **Şema `gezik-config/src/schema.rs`:** `FIELDS` (tablo, anahtar, sayfa, tür); açıklama ve varsayılan gömülü şablondan bir satır tarayıcısıyla (yorumlu örnek anahtarlar ve `[[…]]` örnekleri tanınır), değer dosya metninden (`toml::Table`). Doğrulama ayrıştırıcıyla: değer tek anahtarlık bir metinde `Settings::parse`'tan uyarısız geçmezse yazılmaz, uyarının metni satırın altında kırmızı. Bu yüzden `Int`'te min/max yok, `Float` yok (şablonda kesirli ayar yok).
6. **Türler:** `Bool`, `Choice`, `Int`, `Text`, `Path`, `Theme`, `Integration` (`[system] tray`/`hotkey`: System Integration yazar, satırda düğme), `List` (sayı ve `Edit in settings.toml`).
7. **Pencere `gezik/src/settings_window.rs` + `ui/widgets/settings.slint`:** `SettingsWindow` tek; satırlar tek `SettingLine` bileşeni (onay kutusu, açılır seçim `PopupWindow`'la, metin alanı Enter/odak kaybında yazar, liste/sistem düğmesi), sağ tık satır menüsü. Kapanınca `drop` (bir sonraki olay döngüsü turunda). UI iş parçacığı dosya okumaz: değerler `Process.files`'ın son okunan metninden; dış değişiklikte izleyicinin yeniden yüklemesi pencereyi tazeler (odaktaki alan korunur, farklıysa "Changed in settings.toml: …").
8. **Tema önizlemesi:** seçim listesinde temanın üstüne gelince `resolve_theme` bellekte, bütün pencerelere; ayrılınca (kısa gecikmeyle) ya da pencere kapanınca yüklü temaya döner. Seçim yazılır.
9. **Menüler:** Windows/Linux View düğmesinin menüsünde System Integration'dan sonra ve klasör arka plan menüsünün sonunda `Settings…`; macOS View menüsünün sonunda `Settings… ⌘,` (Slint uygulama menüsüne öğe ekletmiyor); palet eylemden.

## Global Constraints

`RUST_MIN_STACK=67108864`, `-j 2`: `cargo fmt --all -- --check`, `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`; sonda macOS clippy (`--target aarch64-apple-darwin --no-default-features`), sürüm derlemesi ve boyut, kopya `%TEMP%\gezik-gui-14\bin\t14a.exe`. İngilizce commit, atıf satırı yok, amend yok, push yok. GUI yok.

## Task 1: Yapılandırma
- [x] `shortcuts.rs` (`,`, `comma`, `Action::Settings`), `settings_edit.rs` (`with_value`), `settings_writer.rs` (`Value`, `\r\n`), `schema.rs`, `theme.rs` (`BUILTIN_THEMES`), şablon (`theme-light`/`theme-dark` yorumları, `# settings = "mod+,"`).
- [x] Testler: şablon ↔ şema ↔ ayrıştırıcı bağları (her şablon anahtarı şemada ya da bir liste ayarının tablosunda; her alan şablonda ve açıklamalı; her seçim ve varsayılan uyarısız okunur), yanlış değerler reddedilir, `with_value` bayt düzeyinde tek satır (yorumlu, yorumsuz, üst düzey, eksik anahtar/tablo, satır içi tablo, noktalı anahtar, bilinmeyen anahtar kalır, geri yazınca aynı baytlar), `\r\n`, bozuk dosyada hata, arama süzgeci, bilinmeyen anahtar sayısı, `Ctrl+,`.
- [x] Commit.

## Task 2: Pencere
- [x] `gezik/src/settings_window.rs`, `ui/widgets/settings.slint`, `ui/app.slint` (dışa aktarım, macOS menüsü), `actions.rs`, `main.rs` (eylem listesi, yeniden yükleme), `keys.rs`, `trash_view.rs`, `context_menu.rs` (`SETTINGS` 2800), `windows.rs` (`files`), `theme_bridge.rs` (açık pencereye tema).
- [x] Testler (saf kısım): satır modeli (sayfa süzgeci, arama tüm sayfalarda, varsayılan rozeti, bozuk dosyada salt okunur ve satır numarası), yazılacak değer ve reddi, menü kimlikleri.
- [x] Commit.

## Task 3: Ölçüm, denetimler, notlar
- [x] Sürüm exe boyutu, macOS clippy, notlar, spec Durum satırı, macOS 365–368, Linux 266–267, README, `docs/configuration.md`, exe kopyası.
- [x] Commit `Note step 14a: checklists, notes, size (14a)`.

## Sapmalar (spec'ten)

Uygulama sonunda doldurulur.
