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
- Hızlı bakış penceresi ana pencerenin ortasında açılıyor; çok monitörlü kurulumlarda ekran dışına taşma kontrolü yok. Kapatınca odak listeye açıkça döndürülmüyor. (Düzeltildi: seçim boşken Space artık pencere açmıyor; ana pencere kapanırken hızlı bakış da kapanıyor.)
- Çoklu "Open" / Enter 15 öğeyle sınırlı; menüdeki sınır klasörleri de sayıyor, Enter yalnızca dosyaları sayıyor.
- Ctrl+tekerlek ızgara boşluklarında yakınlaştırmak yerine kaydırıyor; dokunmatik yüzey kıstırması boyutları sıçratabilir. "Reset this folder" Bu Bilgisayar'da etkisiz. Klasör açmak "son kullanılan"ı kalıcılaştırmıyor.
- Sıralama durumu sekme başına değil görünüm geneli; klasör başına yalnızca `views.toml` ile saklanıyor.
- Bilinen küçük eksikler: `ByteLru` çıkarma taraması çok küçük resimde O(n); uzantı başına önbellekler sınırsız; NUL içermeyen ikili dosyalar değiştirme karakterleriyle metin gösteriliyor; önizlemede 64 KB'lık kaydırmalı metin yerleşimi maliyeti ölçülmedi.
- Ekranda doğrulanamayanlar (masaüstü kilitliydi; yalnızca derleme, testler ve kod incelemesi): ızgara ve liste çizimi, Ctrl+tekerlek yakınlaştırma, çerçeve seçimi, klavye seçimi, önizleme paneli ve hızlı bakış penceresi, görünüm menüsü, ikon/küçük resim yükleme, sütun yeniden boyutlandırma, tema renkleri (icon-*, focus-ring, marquee). Elle gözden geçirilmeli.
- Performans ölçümleri (sürüm derlemesi, Windows 11; masaüstü kilitli olduğu için kaydırma/PgDn adımları çalışmadı): açılış 24-35 ms (hedef ~60); boşta 7,8 MB (1334×600 pencereyle ölçülmüştü; aşağıdaki yeniden ölçüme bakın); 100 bin dosya yüklendikten sonra 17,2 MB (hedef ~18, tamam); 100 bin ad sıralama 39 ms (hedef 50, tamam; ilk sürümde aşıyordu, düzeltildi); kaydırma CPU'su, ızgara + 1000 fotoğraf belleği (`grid.ps1` boşta 8,2 MB verdi ama sayfalama çalışmadığı için anlamsız) ve 1/20 sekme farkı ölçülemedi — kilitli olmayan masaüstünde tekrar çalıştırılmalı (`scripts/perf/stress.ps1`, `grid.ps1`, `tabs.ps1 -Method SendKeys`). Task 13 sırasında elle bakılan ızgara örnekleri ~51,6 MB gösterdi (büyük küçük resimli klasör; hedef ~50 sınırında).
- Boşta bellek yeniden ölçüldü (sürüm derlemesi, `scripts/perf/measure.ps1 -Runs 5 -Config <geçici klasör>`, varsayılan 900×600 pencere, ev klasörü): sistem ikonlarıyla (varsayılan) **6,7-6,8 MB**, `[view] icons = "gezik"` ile **6,3-6,4 MB**; görünüm işinden önceki sürüm (master) aynı koşulda 5,8 MB. Hedef ≤ 7 MB her iki ayarda da tutuyor. Önceki 7,8 MB'ın farkı pencere boyutundan: yazılımla çizim pencerenin her pikseli için 4 bayt tutuyor; kullanıcının `state.toml`'undaki 1334×600 pencereyle aynı derleme 7,8 MB, master 6,8 MB ölçüyor (~1 MB = 434×600×4). Görünüm işinin gerçek maliyeti ~1,0 MB; bunun ~0,5 MB'ı sistem ikonları ve tür adları için Windows kabuğunun bir kerelik maliyeti (`icons = "gezik"` ile kalkıyor), kalanı yeni kod, görüntü çözücüler ve iş parçacıkları. Eski "5,0 MB" ölçümü bu koşullarla yeniden üretilemedi (master bugün 5,8 MB veriyor).
- Ekran testleri sonrası (2026-10-04, kilidi açık masaüstü): kaydırma CPU'su sütunlarla ~840 ms çıktı (master ~500 ms). Neden Slint'in yazılımla çizimi: her satırda ~44 harf (önce ~16) her karede yeniden çiziliyor ve 359 Hz ekranda saniyede ~360 kare çiziliyor; satır verisi hazırlamak %1'den az. Çözüm `max-fps` ayarı (varsayılan 120): çizim istekleri atılmıyor, bir sonraki kareye kadar bekletiliyor (atılan istek, pencere geri yüklenince sistemin istediği tam yeniden çizimi kaybettiriyordu). Sonuç ~270 ms. Izgara + 1000 fotoğraf (~600'ü sayfalanınca) 24 MB; 20 sekme +0,1 MB.
- `icons = "gezik"` ikonları (Slint `Path`) her karede yeniden çiziliyor ve kaydırmaya ~100-150 ms ekliyor; önceden çizilmiş resimle hızlandırılabilir.

## Dosya işlemleri 4a sonrası (2026-10-05)

- Linux'ta sistem panosu yok (Gezik içi pano); X11/Wayland altyapısıyla birlikte 4b'de.
- macOS/Linux kodu (kopyalama, çöp, pano, sürücü türü) bu makinede çalıştırılmadı; `cargo check -p gezik-core -p gezik-platform -p gezik-ops --target` sonucu: x86_64-unknown-linux-gnu ve aarch64-apple-darwin için derlendi (yalnız iki uyarı düzeltildi: kullanılmayan `DirBuilderExt` içe aktarımı, Windows dışında kullanılmayan `icons.rs` yardımcıları).
- macOS ve Linux'ta `drive_facts().trash` her zaman true: çöpü olmayan bir sürücüde çöpe atma sistemden hata alır ve öğe başarısız sayılır (Windows'taki gibi "kalıcı silinsin mi?" sorusu gelmez).
- Tarama tek iş parçacığında (`read_dir`); 100 bin öğe ön tarama süresi: ölçülmedi.
- Geri alma, oluşturulmuş bir klasörü bütünüyle çöpe atar: işlemden sonra içinde düzenlenen dosyalar da gider (çöpten geri alınabilir). Yalnız tek tek dosyalar "changed since" denetlenir.
- Retry, işlemin tamamını yeniden çalıştırır (bitmiş olanlar çakışma listesinde "identical" ve Skip olarak görünür); yalnız başarısız öğeleri seçerek yeniden deneme yok.
- Farklı sürücüye taşıma otomatik testte yok (ikinci sürücü bilinmiyor); ekranda denendi (2026-10-05, C: → D:, iç içe klasörler, küçük dosyalar ve 100 MB'lık dosya): kaynak kalktı, hedefteki dosyalar birebir aynı, geçici ad artığı yok; Ctrl+Z hepsini C:'ye geri taşıdı.
- Junction'lar kopyalanamıyor olabilir (CopyFileExW + COPY_FILE_COPY_SYMLINK sembolik bağlantıları kopyalar); denenmedi.
- Windows Çöp Kutusu'na büyük bir klasör atmak sistemin boyut hesaplaması yüzünden yavaş; öğe listeden hemen kalktığı için beklenmez.
- Çakışma listesi `VecModel` ile kuruluyor (tembel model değil); 10 bin satırda açılış: ölçülmedi (elle test aşaması).
- Performans (sürüm derlemesi, Windows 11, `scripts/perf/ops.ps1`): 10 000 küçük dosya kopya Gezik 1610 ms / Explorer 12 399 ms (7,7 kat, hedef ≥ 2 kat tuttu); 10 000 dosya silme: klasörden kalkış 7 ms (hedef < 100 ms, tuttu), tamamı 397 ms (`rd /s /q` 979 ms). 50 000 dosya: kopya Gezik 9312 ms / Explorer 62 602 ms (6,7 kat); silme: klasörden kalkış 31 ms, tamamı 2187 ms (`rd /s /q` 4575 ms). 4 GB dosya karşılaştırması yapılmadı (elle, bekliyor). Boşta bellek 6,9 MB (hedef ≤ 7, tuttu); açılış 25 ms (hedef ~60, tuttu); exe 12,4 MB (`measure.ps1 -Runs 5`).
- Geri alma işi hiçbir şey yapmazsa (örn. her şey "changed since") geri alma kaydı kayboluyor. (2026-10-05: hiçbir şey yapmadan iptal edilen geri alma kaydı yerine geri konuyor. Öğeleri başarısız olan ya da "changed since" atlanan geri almanın kaydı bilerek düşüyor: yeniden denemek aynı şekilde başarısız olur ve daha eski işlemleri erişilemez bırakırdı; rapor nedenini gösteriyor.)
- Bir önceki işin takılan sürücü sorgusu (örn. kopmuş ağ sürücüsü) sonraki işleri bloke ediyor. (Düzeltildi 2026-10-05: yalnız aynı kökteki (`c:\`, `\\sunucu\paylaşım\`) işleri bekletiyor. Unix'te her yol `/` altında olduğu için orada değişmedi; takılan iş sistem çağrısında olduğundan iptal edilemiyor.)
- Windows çöpü, adının bir bileşeni nokta veya boşlukla biten yolları reddediyor (Shell kardeş bir öğe üzerinde işlem yapardı). (2026-10-05: artık hata yerine "kalıcı olarak silinsin mi?" soruluyor; çöpe atmak hâlâ mümkün değil.)
- Kalıcı silmede, geri koyma yeniden adlandırması da başarısız olursa gizli klasör artığı kalabilir. (Düzeltildi 2026-10-05: birkaç kez deneniyor, olmazsa `pending-deletes`'e geri koyma kaydı yazılıyor ve bir sonraki açılışta eski adına dönüyor.)
- Spec'teki "kalıcı silmede Explorer'dan ≥ 3 kat hızlı" hedefi ölçülmedi: taban çizgisi olarak yalnız `rd /s /q` kullanıldı; Explorer'da Shift+Del süresinin elle tutulması bekliyor.
- Slint 1.18.1'de metin yerleşim önbelleği süpürülünce (1024 girişten sonra) seyrek çizilen metinler bir daha yeniden çizilmiyordu (örn. büyük klasörde kaydırdıktan sonra durum çubuğu eski seçim sayısında kalıyordu). Düzeltilmiş `i-slint-core` `vendor/` altında, `[patch.crates-io]` ile kullanılıyor (`vendor/README.md`). Slint'e bildirilmedi; Slint yükseltilirken yamanın hâlâ gerekip gerekmediğine bakılmalı.
- Yeniden adlandırılan satır ekrandan kayınca adlandırma, başka yere tıklanmış gibi yazılan adla bitiyor (Slint odaktaki öğe yok olunca tuşları hiçbir yere iletmiyor).
- Dosya işlemleri ekranda denendi (2026-10-05, Windows 11): kopyala/yapıştır, çakışma listesi (Replace, Keep both, If newer, klasör birleştirme, Cancel operation), geri al/yinele, kes/yapıştır ve soluk kesilmiş öğe, yeni klasör, çöpe atma ve geri getirme, kalıcı silme sorusu, çoğaltma, ilerleme paneli (duraklat, sürdür, iptal), kapanış sorusu (Keep open / Cancel them and quit). Bulunup düzeltilenler: öğeleri başarısız olan geri almanın daha eski işlemleri engellemesi, iptal edilen kopyanın yarım dosyasının listede kalması.
- "Identical" kararı 2 saniyelik zaman toleransı kullanıyor (FAT için); NTFS'te boyutu aynı ve 2 saniye içinde değişmiş iki farklı dosya da "identical" görünür. Varsayılan karar zaten Skip olduğu için veri kaybı yok, ama "Hide identical" ile gizlenen bir satır gerçekten farklı olabilir. Tolerans yalnız bir taraf FAT/exFAT olduğunda uygulanabilir.
- Hata ayrıntıları işletim sisteminin ham iletisini gösteriyor ("The system cannot find the file specified. (os error 2)"). (Düzeltildi 2026-10-05: bilinen sistem hataları ve Windows Kabuğu'nun çöp hata kodları sade cümlelerle gösteriliyor, örn. "It is open in another program", "It no longer exists"; bilinmeyenlerde sistem metni hata numarası olmadan, metinsiz kodlarda "Windows could not do it (0x…)".)
- Klasörler dışarıdan değişince liste kendiliğinden yenilenmiyor (yalnız ayar klasörü izleniyor; listeler Gezik'in kendi işlerinden sonra ya da F5 ile yenileniyor). (2026-10-05: etkin sekmenin klasörü izleniyor, alt klasörler değil; değişiklikten 200 ms sonra, değişiklik sürüyorsa en sık `max(1 s, 4 × son yükleme süresi)` arayla yenileniyor; çerçeve seçimi ya da yeniden adlandırma sürerken erteleniyor; aynı klasörün yenilenmesi ikon/küçük resim kuyruklarını sıfırlamıyor. Ölçüm (sürüm derlemesi): boşta CPU 0, boşta bellek 6,9-7,0 MB (önce 6,8-6,9); bir dosyaya 20 ms'de bir yazılırken küçük klasörde bir çekirdeğin %0,9-1,4'ü, 50 000 dosyalı klasörde %2,3-2,7'si. Bu Bilgisayar izlenmiyor; tam yeniden yükleme yapılıyor, değişen satırları tek tek güncelleme yok.)
- İşlem sürerken zorla kapatma denendi (2026-10-05): kopyada kaynak sağlam, küçük dosyalar tam; büyük dosya artık geçici adla kopyalanıyor (önceden yarım dosya tam boyutta ve tamamlanmış görünüyordu), artığı açılışta siliniyor. 50 000 dosyalık anında silmede gizli klasör açılışta silinip bitiyor; çok alt klasörlü silmede listenin yenilenmemesi düzeltildi. Küçük dosyalar (64 MB altı) kendi adlarıyla kopyalanıp işin günlüğüne yazılıyor; açılışta bitmemiş olanlar siliniyor (ekranda denendi: 5000 dosyalık kopyada öldürme sonrası kalan 4 bitmemiş dosya silindi, bitmiş hiçbir kopyaya dokunulmadı). Kalan risk: kullanıcı bitmemiş bir hedefi çökmeyle yeniden açma arasında kaynakla aynı boyut ve zamana getirirse silinmez; günlükten sonra değiştirdiği hedefe dokunulmaz.
- Bekleyen silme kayıtları artık süreç kimliği taşıyor (`deleting<TAB>pid<TAB>yol`; eski düz yol satırları da okunuyor): ikinci bir Gezik penceresinin açılışı, ilk pencerenin süren silmesini kendisi de başlatmıyor (2026-10-05, iki pencereyle denendi). Kopya kayıtları ve günlükleri gibi, sahibi çalışan (bu süreç dahil) kayda dokunulmuyor.
- Gezik'in geçici adları (`.gezik-copying-…`, `.gezik-deleting-…`) artık listelerde gösterilmiyor (2026-10-05).
- İzlenen klasörün kendisi silinince ya da adı değişince (boş klasör dahil) liste en yakın klasöre geçiyor; bunun için üst klasör de izleniyor, yalnız klasörün kendisine dair olaylar sayılıyor (2026-10-05). Adı değişen klasörün yeni adına gidilmiyor, silinmiş gibi davranılıyor.
- Ekranda ayrıca denendi (2026-10-05): "Retry" (panelden ve ayrıntı penceresinden; kilitli dosya açılınca işlem tamamlandı); çakışma listesi yalnız klavyeyle (↑/↓, Home, Ctrl+A, R/S/K/N, Enter ile başlatma, Esc ile iptal). Hata bulunmadı.
