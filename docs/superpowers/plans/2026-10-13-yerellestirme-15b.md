# Yerelleştirme 15b (arayüz metinleri) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Arayüzün görünür metinleri katalogdan geçer: bütün `.slint` metinleri `@tr` (erişilebilirlik adları dahil), macOS menü çubuğu (`app.slint` `MenuBar`), Windows/Linux `≡` ve sağ tık menüleri (`context_menu.rs`), eylem başlıkları (`Action::title` `tx!`, gösterilirken `t`), palet, tepsi menüsü, kenar çubuğu başlıkları, ayar penceresi (sayfa adları, alan başlıkları `setting:<yol>`, şablon açıklamaları `setting:<yol>`, seçim etiketleri `choice:<yol>`; yazılan değer ham), kısayol sayfası. Durum satırları, sorular, hatalar ve biçimler 15c'de. Slint denetim testi. `gezik.pot` yeniden üretilir. Exe ≤ 15a + 262.144 B.

**Spec:** `docs/superpowers/specs/2026-10-13-yerellestirme-design.md` §4.2, §4.5, §6, §7, §10, §13.2 15b. Plan önceden onaylıdır. Dal `feat/i18n-15b` (`feat/i18n-15a` üstüne), `D:\Work\gezik-15b`.

## Kararlar (soru sorulmadı)

1. **`t` her ömürle:** `gezik_i18n::t<'a>(ctx, id: &'a str) -> Cow<'a, str>` (şablondan okunan açıklama ve türetilen başlık `'static` değil); genel değil, ömür silinir, şişmez. `tx!("ctx" => "id")` kolu.
2. **Eylem başlıkları:** `Action::title` `&'static str` kalır (`tx!`), sıralama ve arama İngilizceyi de görür; gösterilen yer `t("", title)`. Palet hem çevrilmiş hem İngilizce başlıkla bulur.
3. **Ayar penceresi:** alan listesi (`Page`, `Kind`, `Field`, `FIELDS`) ve şablon satır ayrıştırıcısı std'den başka bağımlılığı olmayan `gezik-config/src/schema/table.rs`'ye taşınır; `gezik-lang` aynı dosyayı `#[path]` ile kullanır ve her alanın başlığını ve açıklamasını `setting:<yol>` bağlamıyla pot'a yazar (karar: şablon yorumu İngilizce kalır, pencere çevirir). Açıklamanın değerden sonraki yorumu toml_edit yerine aynı dosyadaki küçük okuyucuyla alınır, böylece pencere ile pot aynı metni görür (test). Seçimler `tx!("choice:<yol>" => "değer")`; satırda çevrilmiş etiket, yazılan ham değer (etiketten geri eşlenir).
4. **Bağlamlar** yalnız gerekince: `column` (sütun başlıkları), `button` (çakışma listesinin Replace/Start), `unit`, `choice:…`, `setting:…`; aynı anlamdaki menü öğesi ile düğme aynı anahtarı paylaşır.
5. **Dil değişimi:** dil değişimi zaten `apply_config`'ten geçer (ayar ya da `lang/` dosyası); Rust'ın tuttuğu metinler bugünkü yeniden kurma yollarıyla yenilenir (kenar çubuğu `relabel`, macOS menü komutları, ayar penceresinin satırları, tepsi menüsü dil değişince yeniden kurulur). Sağ tık menüleri her açılışta kurulur.
6. **Slint denetimi:** `gezik-lang` testi `crates/gezik/ui/**/*.slint`'te `@tr`/`@image-url`/`import` dışındaki her dizgi sabitini okur; harf içeren ve kod gibi görünmeyen (küçük harfli kimlik, SVG yolu) ya da izin listesinde olmayan (`Gezik`, biçim adları, `{token}` ipucu) sabit testi kırar.
7. **Sözde dil turu GUI'siz:** yalnız statik (denetim testi, Rust taraması); taşma adayları sabit genişlikli metinlerden çıkarılıp notta listelenir; ekranda tur kullanıcının Windows denetim listesinde.

## Task 1: Altyapı
- [x] `t<'a>`, `tx!` bağlamı; `gezik-platform` → `gezik-i18n`.

## Task 2: Slint
- [x] Bütün `.slint` metinleri `@tr`; birleştirilen metinler tek `@tr`; mantıkta karşılaştırılan dizgiler (ayar penceresi Replace/Cancel) Rust'ta da çevrilmiş metinle karşılaştırılır.
- [x] Denetim testi (`gezik-lang`).

## Task 3: Rust menüleri
- [x] `context_menu.rs` (menü metinleri; durum/hata 15c), `menu_bar.rs`, tepsi (`tray.rs`), `Action::title` `tx!`, palet, kenar çubuğu başlıkları ve ipuçları, kısayol sayfası.

## Task 4: Ayar penceresi
- [x] `schema/table.rs`, başlık/açıklama/seçim çevirisi, sayfa adları, yedek ve tema satırlarının başlıkları ve seçimleri; `gezik-lang extract` şablon metinleri.
- [x] Testler: her `Action` başlığı ve her alanın başlığı/açıklaması pot'ta; seçim etiketi çevrilir, yazılan ham (bayt); palet çevrilmiş ve İngilizce başlıkla bulur; sözde dilde `Action::title` çerçeveli.

## Task 5: Pot, belgeler, ölçüm
- [x] `gezik-lang extract`; notlar `parts/15b.md`, denetim listeleri macOS 390–392, Linux 283; fmt, test, clippy (Windows, macOS), sürüm exe boyutu.
