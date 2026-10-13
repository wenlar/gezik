# Yerelleştirme 15d (CJK yazı tipi yedeği) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Han karakterleri okuyucunun beklediği biçimle çizilsin: Slint'in metin yolu (parley 0.11 + fontique 0.11) metne bir dil versin; dil önce arayüz dilinden (CJK ise), sonra sistemin ilk CJK dilinden, yoksa `zh-Hans`. Windows'ta `测试这检查对齐` Microsoft YaHei ile çizilir (bugün yedek Yu Gothic UI; onda olmayan yalnız-Sadeleştirilmiş karakterler `这`, `对` Arial'ın boş kutusuna düşer); Japon sisteminde Kanji Yu Gothic/Hiragino'da kalır. Exe ≤ master + 8 KB, kare başına iş yok.

**Spec:** `docs/superpowers/specs/2026-10-13-yerellestirme-design.md` §3.4, §13.2 15d, karar 12. Plan önceden onaylıdır. Dal `feat/i18n-15d` (`origin/master` üstüne), `D:\Work\gezik-15d`.

## Bulgular (kaynakta)

1. **Dil nereye girer.** Bütün paragraflar tek yerden kurulur: `vendor/i-slint-core/textlayout/sharedparley/shaping.rs` `LayoutWithoutLineBreaksBuilder::ranged_builder` (çizim, ölçüm, içerik genişliği, `TextInput`; başka `ranged_builder`/`tree_builder` çağrısı yok). Orada `push_default(StyleProperty::Locale(Some(dil)))` yeter. Öğe başına dil gerekmez: bir arayüzde tek okuyucu var; dosya adının dili bilinmez.
2. **parley'de dil ne yapar.** `shape/mod.rs`: öğe (`item.locale`) yazı sistemiyle birlikte `FontSelector`'a gider, `query.set_fallbacks(FallbackKey::new(script, locale))`; ayrıca harfrust dili (`locl`). Adlı aileler (`SansSerif` = Arial, `SystemUi` = Segoe UI) Han kapsamadığı için Han kümesi yedeğe düşer.
3. **fontique'te anahtar.** `fallback.rs` `canonical_locale`: yalnız `Arab`, `Beng`, `Deva`, `Ethi`, `Hani`, `Hebr`, `Tibt` dile bakar; Latin, Kiril, Kana, Hangul dilden etkilenmez (anahtar "izlenmez"). `Hani` için `ja`, `ko`, `zh-HK`, `zh-TW`, `zh-MO`, `zh-SG` (ve `zh-Hant` → `zh-TW`) ayrı yuvalar; **`zh`, `zh-CN`, `zh-Hans` ve dilsiz (`None`) aynı "varsayılan" yuvayı paylaşır**, yuvanın değeri ise ilk sorgunun diliyle sistemden alınır (`collection/mod.rs` `fallback_families`). Yani ilk Han sorgusu dilsiz olursa Yu Gothic varsayılana yerleşir ve sonra `zh-Hans` da onu alır: dil, ilk Han metni şekillenmeden önce verilmeli (Gezik `apply_config`'te verir; pencere oluşur ama ilk çizim `run`'dadır).
4. **Platformlar.** Windows (`backend/dwrite.rs`): DirectWrite'ın `IDWriteFontFallback::MapCharacters`'ına yazı sisteminin örnek metni ve dil (`GetLocaleName`); boş dilde Japonca seçer. macOS (`backend/coretext.rs`): `CTFontCreateForString` benzeri `create_fallback_font_for_text(örnek, dil)`; PingFang kullanılamazsa `Heiti SC`. Linux (`backend/fontconfig.rs`): desene `FC_LANG` (dil) ve yazı sisteminin `FC_CHARSET`'i; Noto Sans CJK gibi çok dilli bir dosyada aile aynı kalır, biçimi harfrust'ın `locl`'u (dil) seçer.
5. **Önbellekler.** parley'in plan önbelleği dili anahtarda tutuyor (`ShapePlanKey`). fontique'in yedek önbelleği (yazı sistemi, dil) anahtarlı. Slint'in `TextLayoutCache` (öğe başına şekillenmiş paragraflar) ve `content_widths` önbelleği anahtar değil **bağımlılık** ile geçersizleşir: girdi `shape` içinde girişin izleyicisinde okunur. Dil bir `Property` olursa ve `ranged_builder`'da okunursa, değişince bütün girişler ve onlara bağlı çizim/yerleşim izleyicileri kendiliğinden kirlenir; anahtara dil eklemek ya da elle temizlemek gerekmez. Okuma yalnız önbellek ıskasında (şekillendirmede) olur: kare başına iş yok. Yazılımsal çizicinin glif önbelleği yazı tipi + glif kimliğiyle anahtarlı, etkilenmez.
6. **Arayüz dili.** `crates/gezik/src/language.rs` `apply` (her `apply_config`'te, ilk kez pencere çizilmeden): `lang::resolve` kodu verir (`en`, `tr`, `zh-Hans`, topluluk `ja`, `pseudo`, …); sistem listesi `system()` (açılışta bir kez). `apply` aynı dilde erken döner ve başlangıç değeri `en`'dir: dil o dönüşten **önce** verilmeli.
7. **Sınamak.** Vendor sandığının kendi testleri Slint deposu dışında derlenmiyor (vendor/README.md); test `crates/gezik/tests/` altında, yayımlanmış bir `#[doc(hidden)]` kancayla gerçek `ranged_builder` yolundan geçer.

## Kararlar (soru sorulmadı)

1. **Süreç geneli dil, vendor'da:** `shaping.rs`'de `thread_local!` bir `Pin<Box<Property<Option<fontique::Language>>>>` ve `pub fn set_text_locale(Option<&str>)` (ayrıştıramazsa `None` = Slint'in bugünkü davranışı). Slint'in UI'si tek iş parçacığında; `GLOBAL_CONTEXT` de iş parçacığına bağlı. Bağlam yapısına alan eklemek daha büyük yama olurdu.
2. **Dil kuralı `gezik_i18n::lang::text_locale(ui, preferred)`** (saf, testli): arayüz kodu `zh`/`ja`/`ko` ile başlıyorsa o; değilse sistem listesindeki ilk CJK etiketi (`normalize`'lı: `zh_TW.UTF-8` → `zh-TW`, `ja-JP`, `zh-Hans-CN`); yoksa `zh-Hans`. Sözde dil ve Latin diller sistem listesine bakar.
3. **Çağrı yeri:** `language::apply`, `resolve`'dan hemen sonra, erken dönüşten önce. `Property::set` aynı değerde bir şey yapmaz.
4. **Test kancası:** `sharedparley::fallback_families_for_tests(font_ctx, text) -> Vec<String>` (`#[doc(hidden)]`): `FontRequest::default()` ile gerçek yoldan şekillendirir, satırları kırar, her glif koşusunun yazı tipinin aile adını (skrifa, `FAMILY_NAME`) döner. Her dil için taze bir `parley::FontContext` (yedek yuvaları paylaşılmasın, bulgu 3).
5. **Linux'ta "Install a Chinese font" notu 15f'ye** (sapma 1).

## Task 1: Vendor yaması
- [x] `vendor/i-slint-core/textlayout/sharedparley/shaping.rs`: `TEXT_LOCALE`, `set_text_locale`, `ranged_builder`'da `font_request` bloğunun dışında `if let Some(locale) = text_locale() { push_default(Locale(Some(locale))) }`; test kancası. Hepsi "Gezik patch" işaretli.
- [x] `sharedparley.rs`: `pub use shaping::{set_text_locale, fallback_families_for_tests};`
- [x] `crates/gezik/Cargo.toml`: `i-slint-core` özelliklerine `shared-parley` (Slint zaten açıyor; modülü doğrudan kullandığımız için açıkça).

## Task 2: Gezik
- [x] `crates/gezik-i18n/src/lang.rs`: `text_locale` ve testi (`zh-Hans` arayüzü; `ja` topluluk dosyası; `tr` + `["tr-TR", "ja-JP"]` → `ja-JP`; `en` + `["zh_TW.UTF-8"]` → `zh-TW`; `pseudo` + `[]` → `zh-Hans`; `en` + `["en-US"]` → `zh-Hans`).
- [x] `crates/gezik/src/language.rs` `apply`: `set_text_locale(Some(&lang::text_locale(&code, system())))`.

## Task 3: Test
- [x] `crates/gezik/tests/slint_cjk.rs`: `zh-Hans`'ta `这` ve `日` Windows'ta `Microsoft YaHei UI`, macOS'ta `PingFang`/`Heiti`; `ja`'da `日` Windows'ta `Yu Gothic UI`, macOS'ta `Hiragino` (`这` Japonca yazı tiplerinde yok: `ja`'da Arial'a düşer, sınanmaz) (aile sistemde yoksa — Linux'ta CJK yazı tipi kurulu değilse — atlanır). `abc` iki dilde aynı aile. Bağımlılık: bir `PropertyTracker` içinde kanca çağrılır, `set_text_locale` değişince izleyici kirli (önbelleğin geçersizleşmesinin dayanağı).

## Task 4: Belgeler, ölçüm
- [x] `vendor/README.md` (i-slint-core bölümüne ikinci yama), `docs/superpowers/notes/parts/15d.md`, denetim listeleri macOS 395, Linux 285 (kendi `### 15d` bölümleri, dosya sonunda), spec Durum satırı yok (spec'i 15f kapatır).
- [x] fmt, `cargo test -p gezik-i18n`, `cargo test -p gezik --test slint_cjk --test slint_patch`, clippy, sürüm exe boyutu. Makine çapında derleme kilidi, `-j 4`.

## Sapmalar (spec'ten)

1. **Linux'ta Çince yazı tipi yokken durum çubuğu notu 15f'de:** `zh-Hans` ancak 15f'de seçilebilir (`zh-Hans.po` orada gelir); not dil satırıyla birlikte yazılır.
2. **Önbellek temizlenmez, bağımlılıkla geçersizleşir** (spec "değişince paragraf önbelleği temizlenir" diyor): aynı sonuç, kod yok, kaçırılan bir önbellek olamaz.

## Uygulamada görülen (Windows, fontique yoklaması)

Han yedeği: dilsiz ve `ja` → Yu Gothic UI, `zh-Hans` → Microsoft YaHei UI, `zh-TW` → Microsoft JhengHei UI, `ko` → Malgun Gothic. Dilsiz `这` Yu Gothic UI'da yok, parley ilk aileye (Arial) düşer: bugünkü boş kutu budur.

## Riskler

| Risk | Etki | Önlem |
|---|---|---|
| İlk Han sorgusu dilsiz olursa fontique'in varsayılan yuvasına Japonca yazı tipi yerleşir (bulgu 3) | `zh-Hans` yine Yu Gothic | Dil `apply_config`'te, ilk çizimden önce; Gezik dili hiç `None` bırakmaz |
| Latin metne de `zh` dili gider (harfrust `locl`) | Türkçe `locl` (`fi` bağı) yok | Bugün de dil yok (gerileme değil); test Latin ailesinin dilden bağımsız olduğunu sınar |
| Slint yükseltmesi | Yama kaybolur | vendor/README.md'de; `slint_cjk.rs` yama yoksa derlenmez |
| Linux'ta CJK yazı tipi yok | Kutular | Spec §10.3; not 15f'de |
| Dil değişince bütün metin yeniden şekillenir | Bir kez, dil değişiminde | `@tr` zaten bütün metni yeniden çevirir |

## Boyut

≈ +4–6 KB (bir `Property`, kanca, kural). `stress.ps1` gerekmez: kare yolunda yeni iş yok.

## Denetim listesi

- **Windows:** `测试这检查对齐.txt` ve `日本語のファイル.txt` olan bir klasör; `language = "auto"` (Türkçe/İngilizce sistem): Çince ad Microsoft YaHei biçiminde, boş kutu yok (`这`, `对`). Japonca ad: Kana Yu Gothic, Kanji'si YaHei biçiminde (beklenen: arayüz dili Japonca değil). Sistem dil listesine Japonca eklenip Gezik yeniden başlatılınca Kanji Yu Gothic.
- **macOS 395:** Çince adlar PingFang SC, Japonca adlar Hiragino; sistem Japonca iken Kanji biçimi değişmez.
- **Linux 285:** Noto Sans CJK kurulu: `LANG=zh_CN.UTF-8` ve `LANG=ja_JP.UTF-8` ile `这`/`误` biçimleri farklı (aynı dosya, `locl`). Kurulu değil: kutular (not 15f'de).
