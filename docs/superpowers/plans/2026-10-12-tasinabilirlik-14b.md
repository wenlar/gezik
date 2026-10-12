# Taşınabilirlik 14b (Kısayol sayfası) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ayar penceresinin Keyboard sayfasında her eylemin, her `[[commands]]` girdisinin ve (macOS) menü çubuğunun değiştirilemez tuşlarının satırı; `Set Key…` tuşa basarak atar, başka bir eylemin ya da komutun tuşuysa "… is used by X — Replace?" sorar; `[shortcuts]`'a yalnız değişen eylem yazılır (yorumlar kalır), komutun tuşu kendi girdisine; `Reset to Default`; baştan sona klavyeyle. UI iş parçacığı dosya okumaz. Exe ≤ 14a düzeltmesi (27.989.504 B) + 262.144 B.

**Spec:** `docs/superpowers/specs/2026-10-12-tasinabilirlik-design.md` §5, §10.2, §13.2 14b. Plan önceden onaylıdır. Dal `feat/portable-14b` (`feat/portable-14a` da94d47 üstüne), `D:\Work\gezik-14b`.

## Kararlar (soru sorulmadı)

1. **Gerçek sayılar:** `Action::ALL` 125 → 126 (`edit-shortcuts`, Keyboard Shortcuts…, tuşsuz; pencereyi Keyboard sayfasında açar). Yeni menü kimliği yok: satır menüsü 14a'nınki (2810 Reset to Default, 2811 Copy Key, 2812 Edit in settings.toml) kısayol satırında da çalışır. Denetim listeleri macOS 369, Linux 268 (14a'nın 365–368 / 266–267'sinden sonra).
2. **Yazma:** yeni `SettingsChange::Shortcut` yerine 14a'nın `Value { table: "shortcuts", key: <eylem>, value }`'su (aynı yorum koruyan `edit_table`); değer şablonun biçiminde: `""` (tuşsuz), `"mod+t"`, `["f5", "ctrl+r"]`. Birincil değiştirici `mod` yazılır (dosya her sistemde aynı anlamda). Komut için `SettingsChange::CommandShortcut { name, shortcut }` → `settings_edit::with_command_shortcut` (adıyla bulunan ilk `[[commands]]` girdisinde yalnız `shortcut`; 12e'nin `with_tables`'ı bütün listeyi yeniden yazar, burada gereksiz).
3. **Çakışma:** `Shortcuts::taken_by(text, to, chord, platform)` tabloyu `to` yazılmış haliyle yeniden okuyup (`from_table`) kimin tuşu kaybedeceğini söyler: bölme eylemiyle öbürünün paylaşması (F5 kopyala/yenile, spec 10 §4.4) soru açmaz, dosyada önce yazılmış eylem sorulur, komut her eyleme kaybeder. Replace kaybedene kalan tuşlarını yazar (yoksa `""`), sonra hedefi.
4. **Yakalama:** satırda gizli bir `FocusScope` her tuşu alır (Tab ve Esc de); Rust `keys::chord_from_press` ile ana pencerelerle aynı okur (pencereye `note_key` kancası: tuş takımı, Ctrl+Shift+rakam, AltGr). Yalnız değiştirici: bekler; `Esc`: iptal; `Backspace`: tuşsuz; AltGr'nin yazdığı karakter ve adı olmayan tuş: "cannot use that key", bekler. `shortcuts::refused`: `fixed_owner` (dosya listesi, macOS menü çubuğu), Windows/Super tuşu (tablo yazamaz), tek başına harf/rakam (yazmaya gider), komutta Ctrl/Alt/Cmd ya da F tuşu şartı.
5. **Soru:** satırın altında 14a'nın satır içi listesiyle `Replace` / `Cancel` (imleç Replace'te, Enter alır, Esc kapatır), soru metni durum satırında.
6. **Satırlar:** eylemler başlığa göre sıralı, sonra "Command: X", macOS'ta `MENU_BAR_KEYS` (⌘Q, ⌘H, ⌥⌘H, ⌘M; düğmesiz satır). Değer `keys::chord_label` (macOS'ta ⌃⌥⇧⌘). Arama başlık, ad, gösterilen ve yazılan tuş metninde (`ctrl+shift`, macOS'ta `cmd+…`). Keyboard sayfasında alanlardan sonra; aramada her sayfadan.
7. **Reset:** yazılmışsa varsayılan metinleri yerinde yazar (anahtar kalır); çakışma sorulmaz (yükleme uyarısı söyler). Komutta tuşu siler.

## Global Constraints

`RUST_MIN_STACK=67108864`, `-j 2`: `cargo fmt --all -- --check`, `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`; sonda macOS clippy (`--target aarch64-apple-darwin --no-default-features -D warnings`), sürüm derlemesi ve boyut, kopya `%TEMP%\gezik-gui-14\bin\t14b.exe`. İngilizce commit, atıf satırı yok, amend yok, push yok. GUI yok.

## Task 1: Yapılandırma
- [x] `shortcuts.rs`: `EditShortcuts`, `KEY_NAMES` (ayrıştırma ve yazma tek tablo), `chord_text`, `refused`, `MENU_BAR_KEYS`, `Shortcuts::chords_for`, `taken_by`, `default_texts` açık. `settings_edit.rs`: `with_command_shortcut`, `set_keeping_decor`. `settings_writer.rs`: `CommandShortcut`.
- [x] Testler: `chord_text` her tuş × 16 değiştirici × iki sistemde geri okunur; ret nedenleri; menü çubuğu tuşları `fixed_owner`; `taken_by` (varsayılan sahibi, F5 paylaşımı, yazılmış tuşun bölme eylemini kapatması, dosyada önce yazılmış eylem, komut ↔ eylem) ve yazılan tablonun hedefe tuşu verdiği; şablona tek satır; komut girdisinde yorum korunur.

## Task 2: Pencere
- [x] `gezik/src/settings_window/shortcuts.rs` (satırlar, arama, `pressed`, `reset`, değişiklikler), `settings_window.rs` (satırlar alanlardan sonra, `on_key`, Replace, satır menüsü, `open_shortcuts`, `note_key` kancası), `ui/widgets/settings.slint` (tür 5 ve 6, `Set Key…`, yakalama alanı, Replace/Cancel), `actions.rs`, `main.rs`, `trash_view.rs`, `keys.rs`.
- [x] Testler: satırlar ve arama, macOS sembolleri ve değiştirilemez satırlar, yakalama sonuçları (değiştirici, AltGr, Esc, Backspace, harf, dosya listesi tuşu, aynı tuş, yazma, soru), komut, Reset, sayfa/arama satır eşlemesi.

## Task 3: Ölçüm, denetimler, notlar
- [x] Sürüm exe boyutu, macOS clippy, notlar, spec Durum satırı, macOS 369, Linux 268, `docs/shortcuts.md`, `docs/configuration.md`, exe kopyası.

## Sapmalar (spec'ten)

1. **Sayılar tabana göre:** `Action::ALL` 126 (spec 129), denetim listeleri macOS 369, Linux 268 (spec 392–393, 284).
2. **`SettingsChange::Shortcut` yok:** eylem `Value` ile yazılır; yalnız komut için `CommandShortcut`.
3. **Kısayol satırı menü kimlikleri (2830–2849) kullanılmadı:** 14a'nın satır menüsü yeterli.
4. **`Shortcuts::bind` yok:** çakışma `taken_by`'da, tablo yeniden okunarak (yükleme kurallarıyla birebir).
5. **`edit-shortcuts` yalnız palette:** menüye eklenmedi (Settings… zaten View menüsünde; bütçe).
6. **macOS `⌘,` değiştirilemez satır değil:** 14a'da `Settings…` uygulama menüsünde değil View menüsünde bir eylem; değiştirilebilir. Değiştirilemezler ⌘Q, ⌘H, ⌥⌘H, ⌘M.
7. **Set Key bir tuş atar:** birden çok tuşlu eylem (Refresh F5 + Ctrl+R) tek tuşa iner; birden çok tuş dosyada yazılır.
8. **Reset çakışma sormaz** (karar 7); ölü tuşlu rakam (Türkçe Q'da Ctrl+Shift+3) ana pencerelerdeki `dead_digit` düzeltmesi olmadan yakalanmaz.
