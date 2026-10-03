# Ayarlar ve Tema — Takip Edilecekler

Alt proje 1 tamamlandıktan sonra bilerek ertelenen maddeler. Kaynak: görev incelemeleri ve son tüm-dal incelemesi.

## İlk macOS / Linux derlemesinde (doğrulanmadan kapatılmamalı)

- **macOS'ta Cmd+Q pencere durumunu kaydetmiyor.** Slint/winit, uygulama menüsündeki "Quit" ile çıkışta `CloseRequested` üretmiyor; `on_close_requested` hiç çalışmıyor (`crates/gezik/src/main.rs`). Çözüm adayı: boyut/konum değiştikçe ~500 ms gecikmeli kaydetmek ya da winit'in çıkış yoluna bağlanmak.
- **Canlı yenileme yolları:** `ConfigStore::is_config_file` kanonikleştirilmemiş klasörle karşılaştırıyor. macOS FSEvents `/private/var/...` gibi gerçek yollar, göreli veya sembolik bağlantılı `GEZIK_CONFIG_DIR` canlı yenilemeyi sessizce kapatabilir. `config_dir()` içinde kanonikleştirme düşünülmeli.
- **Linux'ta `auto`:** xdg portal yoksa winit sistem temasını bildiremiyor, `auto` hep açık tema olur. Belgelenmeli.

## Alt proje 2 (Gezinme) başlarken

- `KnownDirs::expand` `..` parçalarını kabul ediyor; sabitlenen klasörlerde kullanılmadan önce reddedilmeli. **Gezinme'de kapatıldı.**
- `collapse` Windows'ta büyük/küçük harfe duyarlı; `\\?\` önekli yollar `\` ile kalıyor.

## Düşük öncelikli

- `write_atomic`: yazma/sync hatasında `.tmp` dosyası kalıyor; sabit geçici ad eşzamanlı yazımlarda çakışabilir.
- `name`/`base` metin değilse veya `[colors]`/`[metrics]`/`[layout]` tablo değilse uyarısız yok sayılıyor.
- `state.toml`: boyut için üst sınır yok, geri yüklenen boyut monitöre sığdırılmıyor; tek başına `x` veya `y` yazılmıyor.
- Aynı ada farklı büyük/küçük harfle sahip tema dosyaları (`Nord.toml` / `nord.toml`) `read_dir` sırasına göre seçiliyor; `.TOML` uzantısı yok sayılıyor.
- `notify` hata olayları (`Err`) yeniden yükleme tetiklemiyor; `themes/` silinirse izleme yeniden kurulmuyor; sürekli olay akışı yeniden yüklemeyi geciktirebilir.
- Pencere görünür kalsın diye `keep_on_screen` 10 ms aralıkla yeniden deniyor; `WinitWindowAccessor::winit_window()` future'ı ile olay tabanlı hale getirilebilir.
- Uyarıların yalnızca ilki ve sayısı gösteriliyor; sürüm derlemesinde tam listeyi görmenin yolu yok.
- `ToolButton`, `TextField` dışındaki bazı ölçüler (90px, 1px, 0.35) temadan gelmiyor.
- Performans betikleri çalışan tüm `gezik` süreçlerini kapatıyor; `stress.ps1`'de try/finally yok; README betikler için `GEZIK_CONFIG_DIR` önermiyor.
- Test boşlukları: `notice_text`, `config_dir` ortam değişkeni, izleyici biriktirme mantığı (döngü `Receiver` alan bir fonksiyona çıkarılırsa test edilebilir).

## Gezinme sonrası

- Üçüncü parti Shell menü eklentileri Gezik'in sürecinde çalışır; hatalı bir eklenti Gezik'i çökertebilir. Ayrı süreçte izolasyon değerlendirilecek.
- macOS "Hizmetler" alt menüsü yok.
- Adres çubuğu kısaltması parça sayısına göre (genişliğe göre değil).
- Klasör içeriği canlı izlenmiyor; Shell komutlarından sonra elle yenileniyor.
- Windows'ta arka plan Shell menüsünde Yeni/Yapıştır/Yenile/Özellikler yok; bunları Explorer'ın görünümü sağlıyor. Gezik'in kendi sürümleri alt proje 4'te gelecek.
- Sekmeler ve kenar çubuğu ayırıcısı klavye ile odaklanamıyor.
- Listede odak göstergesi yok.
- Explorer menüsü açıkken boşta CPU döngüsü olabilir; doğrulanacak.
- Bazı ölçüler hâlâ px sabiti.
- Pencere başına yalnızca bir winit pencere-olayı kancası kurulabiliyor.
- Performans betikleri pencereyi başlığa göre değil süreç kimliğine göre buluyor (başlık artık "<yer> — Gezik"); `tabs.ps1` sekmeleri PostMessage ile açıyor, Gezik değiştirici tuş durumunu klavyeden okuyorsa `-Method SendKeys` gerekir. Gezinme performans ölçümleri (measure/stress/tabs) henüz yapılmadı.
