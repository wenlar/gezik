# Vurgulama kuralında `where` (özellik satırı 66) — notlar

Spec `../../specs/2026-10-12-onizleme-2-design.md` §9.3; 13f notunda (followups, "Vurgulama kuralında `where` hâlâ bağlı değil") ayrı iş diye bırakılmıştı.

## Yapılanlar

- `[[highlight-rules]] where = "duration>1h"`: süzgeç çubuğunun özellik sözcükleri (`field_filter::parse_where`), ayar yüklenince bir kez `Rule.fields`'a derlenir; öbür koşullarla VE. Bozuk `where` (`duration>3x`, `*.mp3`) uyarıyla kuralı dışarıda bırakır (`highlight-rules[7]: where: duration>3x: …`); öbür kurallar kalır.
- `Highlights::first` satırın alanlarını tembel alır (`impl FnOnce`, `LazyCell`): yalnız `where`'li bir kuralın öbür koşulları tutunca sorulur. Okunmamışsa kural "henüz bilinmiyor": renk yok, sonraki kurallara geçilmez (12 karar 23, `RowMeta::Pending` gibi). `name = "*.mp3"` + `where` olan kural `.flac` satırı için okuma başlatmaz.
- Okuma 13e'nin satır önbelleği (`Media::fields`, `MediaKey::Fields`): yalnız ekrandaki satırlar, yalnız kuralların istediği alanlar (`Highlights::fields_needed`, `tags.rs`'de ayar değişince bir kez hesaplanır). `where`'li kural yoksa küme boş ve hiçbir okuma başlamaz (testte bekleyen istek sayısıyla). Klasörün tamamı okunmuşsa (`FolderFields`, süzgeç/sıralama/gruplama) oradan yanıtlanır.
- Klasörler, diskte olmayan satırlar (arşiv içi, telefon) ve okuyucusu istenen alanı vermeyen uzantılar (`.txt`) değersiz bilinir: `duration>3s` tutmaz, `album:none` tutar (aramadaki `passes_fields` ile aynı).
- `Media::fields` artık değerlerle birlikte hangi alanlar için okunduklarını da verir; eski kuşağın (F5, klasör değişikliği) değerleri yeniden okunana dek geçerli sayılır, renk yenilemede yanıp sönmez.
- `Rule`, `Highlights` ve `Settings` artık `Eq` değil (alan değerleri `f64` taşır); yalnız `PartialEq` kullanılıyordu.
- Şablon (`[[highlight-rules]]` açıklaması ve `where = "duration>1h"` örneği), `docs/features.md`. `docs/configuration.md` kural anahtarlarını saymıyor, dokunulmadı. Yeni çevrilecek metin yok (`where:` öneki anahtar adı; hata metni `parse_where`'in zaten çevrilen metni).

## Sapmalar

1. Sütun hücreleri ve `where` aynı satır için ayrı istek yapar (iki ayrı `MediaKey::Fields`): ikisi de varken ve satır hiç okunmamışken dosya iki kez açılabilir. Birleştirmek klasör okumasının kapsama denetimini karıştırıyordu; ilk incelemede birleşik hali dışa aktarmada klasör değerlerini kaybettiriyordu.
2. Ekran testi yapılmadı (aşağıda).

## Boyut

Sürüm exe'si master (2e90d920) **31.810.560 B** → bu dal **31.814.144 B**: **+3.584 B** (bütçe 262.144).

## Testler

- `gezik-core` `highlight`: `where` eşleşir/eşleşmez, değersiz satır, okunmamış satırda bekleme (sonraki kural uygulanmaz), adı tutmayan satırda alan sorulmaz, `where`'siz kurallarda `fields_needed` boş; satır başına süre testi artık 32 `where`'li kural da deniyor (sürümde < 5 µs sınırı, hata ayıklamada < 1 ms).
- `gezik-config` `highlight_rules`: `where` okunur, iki bozuk `where` uyarıyla dışarıda.
- `gezik` `a_where_rule_reads_the_rows_drawn_and_waits_for_their_fields`: `where`'siz kural okuma başlatmaz; `where`'li kural yalnız çizilen `.mp3`'ü okur, okunana dek renk yok, okununca doğru kural; `.txt` ve klasör hemen sonraki kurala düşer.
- `cargo test -p gezik-core -p gezik-config -p gezik`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` temiz.

## Ekran testleri (Windows)

Yapılmadı: test Gezik'i açıldığında ön planda bir oyun vardı (ilk ekran görüntüsü oyunu gösterdi); hiçbir tuş/tıklama gönderilmeden kapatıldı, görüntü silindi. Hazır olanlar: `%TEMP%\gezik-gui-hlwhere\bin\hlwhere.exe`, ayar `%TEMP%\gezik-gui-hlwhere\cfg\settings.toml` (1: `where = "duration>3s"` kırmızı kalın, 2: `kind = "audio"` mavi, 3: bozuk `duration>3x`), veri `%TEMP%\gezik-gui-hlwhere\data` (1 s, 5 s, 10 s WAV, bir MP3, bir `.txt`, bir klasör). Beklenen: 5 s ve 10 s kırmızı kalın, 1 s mavi, MP3 süresine göre, `.txt` ve klasör renksiz, durum satırında `highlight-rules[3]` uyarısı.

macOS/Linux denetimleri: `../checklists/highlight-where-macos.md` (423–425, 387'nin yerine), `../checklists/highlight-where-linux.md` (307–309, 281'in yerine).
