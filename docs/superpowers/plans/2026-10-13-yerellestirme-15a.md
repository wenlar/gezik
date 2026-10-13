# Yerelleştirme 15a (altyapı) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Çeviri altyapısı: yeni `gezik-i18n` sandığı (`.po` okuyucusu, çoğul kuralları ve `Plural-Forms` değerlendiricisi, Slint ile aynı biçimleyici, sözde dil, dil eşleme, katalog ve küresel geçerli dil, `t!`/`tn!`/`tx!`), gömülü diller için derleme anında karma tablolu deflate blob (bugün dil yok), Slint'e dış çevirmen, sistem dili, `language` ayarı ve ayar penceresi satırı, `<ayar klasörü>/lang/*.po` yükleme ve uyarıları, `gezik-lang` (extract/update/check), `gezik.pot`, `docs/translating.md` taslağı. Metin çıkarma yok: boru hattını kanıtlayan birkaç metin (dil satırının etiketleri `t!`, ayar penceresinin başlığı `@tr`). Exe ≤ master + 262.144 B.

**Spec:** `docs/superpowers/specs/2026-10-13-yerellestirme-design.md` §4, §5, §9, §11, §13.2 15a. Plan önceden onaylıdır. Dal `feat/i18n-15a` (`ci/github-actions` + `docs/step-15-spec` üstüne), `D:\Work\gezik-15a`.

## Kararlar (soru sorulmadı)

1. **Bağımlılıklar:** `gezik-i18n` yalnız `miniz_oxide` 0.9.1 (zaten bağlı, gezik-batch). Yeni dış sandık yalnız `tr` 0.1.11 (MIT, `default-features = false`: bağımlılıksız, ~300 satır): `i-slint-core`'un `tr` özelliği onu ister ve `Translator` arayüzü odur; başka yolu vendor yaması olurdu. `.po` okuyucusu ve çoğul kuralları sandıkta yazılır (testli).
2. **Katalog:** `Catalog { code, pseudo, layers }`; katman sırası dosya (`lang/<kod>.po`) > gömülü > kaynak (İngilizce). Dosya katmanı `HashMap<bağlam\x04kaynak, biçimler>`, gömülü katman açılmış metin + başlangıç konumları + `KEYS` (FNV-1a 64, sıralı) üzerinde ikili arama. `fuzzy` girdiler kullanılır (karar 7). Yer tutucu hatalı girdi katmana girmez (İngilizce görünür).
3. **Küresel dil:** `RwLock<Option<Arc<Catalog>>>`; `None` = İngilizce: `t()` kaynağı kopyalar, hiçbir katalog yüklenmez. `t`, `tn` genel olmayan işlevler; makrolar onlara iner.
4. **Gömülü diller:** `build.rs` `lang/gezik.pot` ve `lang/*.po`'dan `OUT_DIR/embedded.rs` (`KEYS`, `BUILTINS`) ve dil başına deflate'li `.bin`. 15a'da `.po` yok: `BUILTINS` boş; biçim birim testinde gidiş-dönüşle sınanır. Yalnız seçili dil açılır (`Catalog::open`).
5. **Slint:** `i-slint-core` normal bağımlılık, `features = ["std", "tr"]`. Çevirmen `Arc<Catalog>`'u tutar (`Cow` ödünç döner); dil değişince `set_external_translator(Some/None)` (Slint bütün `@tr` bağlarını kirli işaretler). İngilizcede çevirmen kurulmaz. Uyarlayıcı `src/translator.rs`; `tests/slint_translate.rs` aynı dosyayı `#[path]` ile kullanır.
6. **Sistem dili:** `gezik_platform::locale::preferred()` (Windows `GetUserPreferredUILanguages`, macOS `NSLocale.preferredLanguages`, Linux saf `posix_preferred` gettext kuralıyla). Açılışta bir kez okunur (`OnceLock`).
7. **Okuma iş parçacığında:** `ConfigStore::read_files` `lang/*.po`'yu (≤ 4 MB) okur, ayrıştırır, katmanı kurar ve uyarıları (sözdizimi satırıyla, yer tutucu, çoğul sayısı, eksik sayısı; dosya başına ≤ 10) ekler. UI iş parçacığı yalnız seçer ve (gömülüyse) açar. İzleyici `lang/*.po`'yu yapılandırma dosyası sayar.
8. **Ayar:** `language = "auto"` (kök anahtar, General sayfası, `Kind::Language`). Biçim denetimi `Settings::parse`'ta; bilinmeyen kod `resolve`'da uyarı ve `auto`.
9. **Satır:** seçenekler `kod — etiket` ("auto — System: English", "pl — Polski", "pseudo — Pseudo (for translators)"); seçilince ilk parça yazılır. Etiketler `t!` ile.
10. **`gezik-lang`:** `extract` Rust (`t!`, `tn!`, `tx!`; `gezik-i18n` ve `tests/` hariç; `//` satırları atlanır; `// TRANSLATORS:` notu) ve Slint `@tr`'den pot'u yazar; `update` her `.po`'yu pot'a uydurur (yeni boş, kalkan `#~` sonda); `check` raporu ve hata kodu. Ayar şablonunun açıklamaları 15b'de (ayar penceresi metinleriyle).
11. **CI bağı:** `cargo test -p gezik-i18n`: pot güncel, her yerleşik `.po`'da hata yok, `human` dilde eksik yok.

## Task 1: `gezik-i18n`
- [x] `po`, `plural`, `format`, `pseudo`, `lang`, `catalog`, `check`, `lib` (küresel, makrolar), `build.rs`, `lang/gezik.pot`.
- [x] Testler: okuyucu (çok satır, kaçış, bağlam, çoğul, başlık, `#~`, BOM, CRLF, bozuk girdi satırıyla, sınırlar ve rastgele bozulmuş girdi panik yok); çoğul (en/tr/de/es/fr/zh tablosu, Lehçe, Rusça, Arapça 0–1000, bozuk ifade, derinlik); biçimleyici Slint'in vakaları; katman sırası; seçim (§5.2 örnekleri, `C`, `C.UTF-8`, `zh_TW`); sözde dil; İngilizcede katalog yok; blob gidiş-dönüşü.

## Task 2: Platform ve yapılandırma
- [x] `locale::preferred`, `posix_preferred` testi; `Settings.language`, şablon, şema `Kind::Language`, `store` (`languages`, `lang/` izleme, uyarılar, bilinmeyen kod).

## Task 3: Uygulama
- [x] `Cargo.toml` (`i-slint-core` `tr`), `translator.rs`, `language.rs` (seçim, açma, kurma), `main.rs` (`apply_config`), `settings_window.rs` (satır), `settings.slint` `@tr`, `tests/slint_translate.rs`.

## Task 4: `gezik-lang`, belgeler, ölçüm
- [x] `bin/gezik-lang.rs` ve testleri; `docs/translating.md`, `docs/configuration.md`, `vendor/README.md`, `THIRD-PARTY.md`, notlar, denetim listeleri (macOS 388–389, Linux 282), spec Durum satırı; fmt, test, clippy (Windows, macOS), sürüm exe boyutu.

## Sapmalar (spec'ten)

1. **`number.rs` 15c'ye** (§8.1 biçimleri orada; 15a'da `{n}` düz sayı).
2. **Şablon açıklamaları pot'a 15b'de** (ayar penceresi metinleriyle birlikte; 15a pot'u yalnız boru hattının birkaç metni).
3. **Senkron klasörü çevrimdışıyken** `lang/` önbelleğe kopyalanmaz: klasör yokken topluluk dili İngilizceye düşer (kısayol notu kodda).
