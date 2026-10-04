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

## Gezinme ekran testleri sonrası (2026-10-04)

- Etkin sekme kapatıldıktan sonra durum çubuğundaki öğe sayısının eski değerde kaldığı bir kez görüldü; 13 denemede tekrar oluşturulamadı, kodda neden bulunamadı (şüphe: kaçırılan yeniden çizim).
- End ile uzak atlamada son satır bir iki kare kısmen görünüp düzeliyor (Slint'in satır sınırına yaslaması; 50 ms sonra yeniden uygulanıyor).
- Dar sekmede üzerine gelince × yer aldığı için başlık yeniden kısalıyor.
- Kaydırma CPU'su sınırda (~499 ms / 2,6 sn, hedef ~480); ölçüm sırasında bilgisayar kullanılıyordu, tekrar ölçülmeli.
- Test edilemeyenler: USB bellek takma/çıkarma, ağ sürücüsü / bağlantısı kopmuş Z:, ekran okuyucu, macOS.

## Görünüm sonrası (2026-10-04)

- macOS ve Linux'ta sistem ikonları yok (Gezik ikonları kullanılıyor); macOS'ta tür adı uzantıdan; iki sistemde de küçük resimler yalnızca Gezik'in çözdüğü png/jpeg/gif/webp/bmp. Linux tür adları (shared-mime-info) ve tarih biçimi denenmedi. macOS/Linux'a özgü kod (cfg) yalnızca Windows hedefiyle derlendi, hiç derlenip çalıştırılmadı.
- Sütunlar sığmayınca yatay kaydırma yok (sağdaki sütunlar kırpılıyor); sütunları sürükleyerek sıralama ve çift tıkla otomatik genişlik yok.
- Seçili bir öğeye (çoklu seçim içinde) basmak seçimi hemen tek öğeye indiriyor; sürükle-bırak (Alt proje 4) için seçimin bırakmada daralması gerekecek.
- Ekran ölçeği değişince (pencereyi başka monitöre taşımak) ikon boyutu bir sonraki klasör gösterimine kadar eskisi kalıyor.
- Küçük resimlerin alfa kanalı önceden çarpılmış olabilir; saydam PNG küçük resimlerinde kenarlar koyu görünebilir. Tamamen saydam (alfa 0) 32 bit ikon/küçük resim opak yapılıyor.
- Windows önizleme işleyicileri (PDF/Office canlı görüntüleme) yok; sistem küçük resmi gösteriliyor.
- Çerçeve seçimi bir satırdan başlayıp o satır ekrandan çıkınca işaretçi yakalaması kaybolabiliyor; satırlar ve arka plan `move`/`up` olaylarını izleyerek telafi ediyor — farklı Slint sürümlerinde yeniden doğrulanmalı. Bırakma listenin dışında olursa çerçeve, bir sonraki tuş bırakılışına kadar takılı kalabilir.
- Hızlı bakış penceresi ana pencerenin ortasında açılıyor; çok monitörlü kurulumlarda ekran dışına taşma kontrolü yok. Kapatınca odak listeye açıkça döndürülmüyor; seçim boşken Space boş pencere açıyor; ana pencere kapanırken açık hızlı bakış uygulamayı canlı tutabilir (kontrol edilmedi).
- Çoklu "Open" / Enter 15 öğeyle sınırlı; menüdeki sınır klasörleri de sayıyor, Enter yalnızca dosyaları sayıyor.
- Ctrl+tekerlek ızgara boşluklarında yakınlaştırmak yerine kaydırıyor; dokunmatik yüzey kıstırması boyutları sıçratabilir. "Reset this folder" Bu Bilgisayar'da etkisiz. Klasör açmak "son kullanılan"ı kalıcılaştırmıyor.
- Sıralama durumu sekme başına değil görünüm geneli; klasör başına yalnızca `views.toml` ile saklanıyor.
- Bilinen küçük eksikler: `ByteLru` çıkarma taraması çok küçük resimde O(n); uzantı başına önbellekler sınırsız; NUL içermeyen ikili dosyalar değiştirme karakterleriyle metin gösteriliyor; önizlemede 64 KB'lık kaydırmalı metin yerleşimi maliyeti ölçülmedi.
- Ekranda doğrulanamayanlar (masaüstü kilitliydi; yalnızca derleme, testler ve kod incelemesi): ızgara ve liste çizimi, Ctrl+tekerlek yakınlaştırma, çerçeve seçimi, klavye seçimi, önizleme paneli ve hızlı bakış penceresi, görünüm menüsü, ikon/küçük resim yükleme, sütun yeniden boyutlandırma, tema renkleri (icon-*, focus-ring, marquee). Elle gözden geçirilmeli.
- Performans ölçümleri (sürüm derlemesi, Windows 11; masaüstü kilitli olduğu için kaydırma/PgDn adımları çalışmadı): açılış 24-35 ms (hedef ~60); boşta 7,8 MB (hedef ≤ 7, **aşıldı**; görünüm işinden önce 5,0 MB — +2,8 MB'ın nedeni araştırılmadı, şüpheli: görüntü çözücüler, ikon/küçük resim hattı, ek iş parçacıkları); 100 bin dosya yüklendikten sonra 17,2 MB (hedef ~18, tamam); 100 bin ad sıralama 39 ms (hedef 50, tamam; ilk sürümde aşıyordu, düzeltildi); kaydırma CPU'su, ızgara + 1000 fotoğraf belleği (`grid.ps1` boşta 8,2 MB verdi ama sayfalama çalışmadığı için anlamsız) ve 1/20 sekme farkı ölçülemedi — kilitli olmayan masaüstünde tekrar çalıştırılmalı (`scripts/perf/stress.ps1`, `grid.ps1`, `tabs.ps1 -Method SendKeys`). Task 13 sırasında elle bakılan ızgara örnekleri ~51,6 MB gösterdi (büyük küçük resimli klasör; hedef ~50 sınırında).
