# File Pilot ↔ Gezik: Özellik Farkı Analizi

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** File Pilot Beta v0.8.5 (31 Ağustos 2026) ↔ Gezik `master` (29dcb45) + 5d (PDF, indirilen pdfium) var sayıldı
- **Durum işaretleri:** **Var** · **Kısmen** (eksik yazılı) · **Yok** · **Planlı** (yol haritası adımı yazılı)
- **Yol haritası adları:** *Taşınabilirlik* (ayar dışa/içe aktarma, senkron klasör, GitHub'dan tema kurma) · *Bulut senkronu* · *Gelişmiş* (arama, çift panel, etiketler, Git, komut paleti; toplu işlemler spec'ine göre arşivin içinde gezinme de burada)

## 1. Özet

File Pilot, tek kişilik Voidstar'ın (Vjekoslav Krajačić) saf C ile, kendi platform katmanı, kendi UI çatısı ve Direct3D 11 / OpenGL çizicisiyle yazdığı, yalnız Windows (7-11, 64 bit) için bir dosya yöneticisi. Gücü **gezinme ve bulma**: serbest bölünen paneller, kayıtlı düzenler, bulanık süzme, tüm sürücüyü düzleştirip milisaniyede gösterme, GoTo ve komut paleti. Dosya *işlemlerinde* zayıf: kopyalama Windows'un aktarım penceresiyle yapılır, **geri alma yok (yol haritasında planlı)**, arşiv açma/oluşturma, dönüştürme yok.

Gezik'in gücü tam tersi tarafta: kendi kopyalama motoru, çakışma listesi, çok adımlı geri alma, arşivler, dönüştürme, PDF, kullanıcı komutları, üç işletim sistemi. File Pilot'tan alınacak asıl şey **arama/süzme, komut paleti, GoTo, panel bölme ve klasör boyutları** — çoğu zaten *Gelişmiş* adımında.

## 2. File Pilot kısaca

| Konu | Bilgi |
|---|---|
| Platform | Yalnız Windows 7-11, 64 bit; ARM "gelecekte" |
| Teknoloji | C, dış kütüphane neredeyse yok (`stb_sprintf`), kendi çizici (D3D11 varsayılan, v0.7.1'den beri; OpenGL), kendi bellek yönetimi |
| Dağıtım | Tek exe, isteğe bağlı kurucu; arka plan servisi yok; hesap, abonelik, internet gerekmez |
| Durum | Açık beta, Şubat 2025'ten beri (v0.2); 400 bin+ indirme |
| Lisans | Beta ücretsiz; isteğe bağlı kalıcı lisanslar: **Essential** ($50; 1 yıl güncelleme), **Pro** ($200; ömür boyu güncelleme, insider sürümler, VIP kanal, öncelikli destek); **Team / Team Pro** (en çok 10 çalışan). Tüm cihazlara kurulur. Fiyatlar 2025 başı haberinden; sayfa şu an fiyat göstermiyor, "%20 erken kuş" indirimi var. |
| Belge | Çevrimiçi kılavuz yok (yol haritasında planlı); sürüm notları "Starlog" |
| Yol haritası (planlı) | Miller sütunları, çok satırlı adlar, büyük küçük resimler, gelişmiş arama, sanal klasörler, MTP, **geri al/yinele**, ağ paylaşımı iyileştirmeleri, CLI, bulut entegrasyonu, bağlam menüsü geliştirmeleri, Unicode desteği, yerelleştirme, otomatik güncelleme, çevrimiçi kılavuz. Gelecekte: gelişmiş dizinleme, Quick Access özelleştirme, fare kısayolları, ARM. |

## 3. Özellik karşılaştırması

### 3.1 Gezinme: sekmeler, paneller, konumlar

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Sekmeler, sürükleyerek sıralama | **Var** | Orta tıkla arka planda sekme, sekmede bekleyerek bırakma |
| Sekme çoğaltma, diğerlerini kapatma | **Var** | Sekme sağ tık menüsü |
| Kapatılan sekmeyi yeniden açma, soldakileri kapatma | **Yok** | |
| Sekmelerde arama (Search Tabs açılır listesi) | **Yok** | |
| Serbest panel bölme (dikey/yatay, sürükle-bırakla) | **Planlı** (Gelişmiş: çift panel) | Plan iki panel; File Pilot'ta panel sayısı serbest |
| Diğer panele kopyala/taşı kısayolu, etkin olmayan paneli soluklaştırma | **Planlı** (Gelişmiş: çift panel) | |
| Kayıtlı ve değiştirilebilir düzenler (layouts) | **Yok** | Çift panelden sonra anlamlı |
| GoTo: sistem yolları, son klasörler, otomatik tamamlama, `%ORTAM%` değişkenleri | **Kısmen** | Ctrl+L ile yol yazılır; tamamlama, son klasörler ve ortam değişkeni yok |
| Quick Access (yer imleri, sürücüler) | **Var** | FOLDERS + PINNED + DRIVES; sabitlenenler `settings.toml`'da, OS'ler arası taşınabilir |
| Kenar çubuğunda sürücü doluluk çubuğu | **Yok** | |
| UNC yolları, ağ paylaşımları | **Kısmen** | Ağ sürücüleri listelenir, motor `\\?\UNC\` yollarını işler; ağ konumlarını tarama ve kimlik sorma yok |
| Çıkarılabilir sürücü takma/çıkarma takibi | **Var** | 3 sn'de bir ucuz liste karşılaştırması |
| Ağaç görünümü (satır içi iç içe klasörler) | **Yok** | |
| Hiyerarşik klasör görünümleri (ayar alt klasörlere geçer) | **Kısmen** | Klasör başına görünüm var (`views.toml`, 500 klasör); alt klasörlere miras yok |
| Varsayılan dosya yöneticisi yapma | **Yok** | |
| Komut satırı aracı (`pilot`, PATH'e eklenir) | **Kısmen** | `gezik <yol>` o klasörde açar; PATH'e eklenmez, başka seçenek yok |
| Tek tıkla açma seçeneği | **Yok** | |

### 3.2 Görünümler

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Ayrıntılar (sütunlu liste), sıralama, sütun göster/gizle | **Var** | Doğal sıralama, Türkçe harf sırası |
| Sütunları (Ad dahil) sürükleyerek yer değiştirme | **Yok** | Görünüm spec'inde kapsam dışı |
| Izgara/ikon görünümü, yakınlaştırma | **Var** | Üç boyut, Ctrl+tekerlek |
| Küçük resimler | **Var** | Resim, video, PDF; `thumbnails = false` |
| Explorer "Liste" tarzı dikey sütun görünümü | **Yok** | |
| Group By (tarih, tür, boyuta göre katlanır bölümler) | **Yok** | Görünüm spec'inde kapsam dışı |
| Klasör boyutu ve öğe sayısı arka planda; boyut/kullanım çubuğu sütunu | **Yok** | Klasörde Boyut boş; önizleme paneli yalnız öğe sayısı (≤ 10 000) gösterir |
| Öğe onay kutuları, kutular üzerinde sürükleyerek seçme | **Yok** | Görünüm spec'inde kapsam dışı |
| Çerçeve (dikdörtgen) seçimi | **Var** | Kenarda kendiliğinden kayma |
| Gizli dosyaları / dotfile'ları gizleme | **Var** | `toggle-hidden` (Ctrl+H) |
| Alternatif satır renkleri | **Yok** | |
| Son değişen dosya işareti | **Yok** | |
| Akıllı değişiklik takibi (yeniden sıralama/süzmeyi azaltan) | **Kısmen** | Yalnız etkin sekmenin klasörü izlenir; değişiklikte tam yeniden yükleme, satır bazında güncelleme yok |

### 3.3 Önizleme

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Inspector paneli (metin, resim, klasör içeriği; EXIF yönü) | **Kısmen** | Alt+P paneli metin, resim, küçük resim, klasör öğe sayısı, dosya bilgisi; EXIF yönü uygulanmıyor, klasörün *içeriği* listelenmiyor |
| Quick Look (Boşluk) | **Var** | Ayrı pencere, oklarla gezinme |

### 3.4 Dosya işlemleri ve bağlam menüsü

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Kopyala, taşı, sil, yeniden adlandır | **Var** | File Pilot Windows'un aktarım penceresini kullanır; Gezik kendi motoruyla, çakışma listesi ve geri almayla |
| Sürükle-bırak: içeride, dışarıya, dışarıdan | **Var** | Windows'ta denendi; macOS/Linux derlendi |
| Tarayıcı/e-postadan sürükleme (sanal dosya) | **Yok** | Yalnız dosya yolu taşıyan bırakmalar alınıyor |
| ZIP içinden sürükleme | **Planlı** (Gelişmiş: arşivin içinde gezinme) | |
| Dosyayı exe/betik üzerine bırakma (onunla açma) | **Yok** | |
| Sağ tuşla sürükleme menüsü (3. parti öğelerle) | **Kısmen** | Gezik'in kendi Taşı/Kopyala menüsü var; kabuk eklentilerinin öğeleri yok |
| Sistem bağlam menüsü (3. parti öğeler dahil) | **Var** | Windows'ta yerel menü, Gezik öğeleri üstte |
| Bağlam menüsünde öğe sabitleme ve filtreleme | **Yok** | |
| Yol olarak kopyala (seçim yokken bulunulan klasör) | **Kısmen** | Yalnız Windows menüsündeki "Copy as path"; Gezik'in kendi komutu ve kısayolu yok |

### 3.5 Arama

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Bulanık süzme, uzantıya göre süzgeç | **Planlı** (Gelişmiş: arama) | Bugün yalnız harfle ilk eşleşmeye atlama |
| Düzleştirilmiş hiyerarşi; tüm sürücüyü milisaniyede listeleme | **Planlı** (Gelişmiş: arama) | |

### 3.6 Klavye ve komut paleti

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Komut paleti (eylem arama, oradan kısayol atama) | **Planlı** (Gelişmiş: komut paleti) | |
| Kısayolları değiştirme | **Var** | `settings.toml` `[shortcuts]`, 25 eylem, canlı; uygulama içinden atama yok |
| Tuş dizileri, numpad tuşları, takma adlar | **Yok** | Tek akor; numpad tuş adı yok |

### 3.7 Tema ve özelleştirme

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Renk temaları, özel renk şeması | **Var** | TOML, `base` mirası, canlı yenileme, satır numaralı hata |
| Yazı boyutu ve aralık | **Var** | `font-size`, `font-family`, `icon-size`, `density` |
| Yuvarlak/keskin köşeler | **Var** | `radius` 0-16 |
| Animasyonları ve araç ipuçlarını kapatma | **Yok** | Gezik'te animasyon az; anahtar yok |
| Grafik Seçenekler ekranı | **Yok** | Ayarlar yalnız dosyadan (Ayarlar spec'inde ayrı alt proje) |
| Uzantıları gizleme, kısaltılmış yol gösterimi | **Yok** | Adres çubuğu sığmayınca baştan `…` ile kısalır, seçenek değil |

### 3.8 Yeniden adlandırma

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Toplu yeniden adlandırma (benzersiz kimlik, tarih) | **Var** | 5a: 7 kural, regex, şablon, EXIF çekim tarihi, elle düzenleme, kayıtlı kural setleri, tek Ctrl+Z |

### 3.9 Platform ve dağıtım

| File Pilot özelliği | Gezik | Not |
|---|---|---|
| Tek küçük exe, isteğe bağlı kurucu | **Kısmen** | Tek exe ama ~20 MB; kurucu ve otomatik güncelleme yok (ilk herkese açık sürümden önce planlı) |
| GPU ile çizim (Direct3D 11 / OpenGL) | **Yok** | Slint yazılım çizicisi (bilinçli: bellek) |
| Windows 7 / 8.1 desteği | **Yok** | Rust araç zinciri Windows 10+ hedefliyor; değeri düşük |

### 3.10 Durum sayımı

| Durum | Sayı |
|---|---|
| Var | 18 |
| Kısmen | 9 |
| Yok | 23 |
| Planlı | 6 |
| **Toplam** | **56** |

## 4. Performans

File Pilot **sayısal ölçüm yayımlamıyor**: sitedeki "2 MB", "öne getirmek kadar hızlı açılıyor", "2,7 milyon dosyayı çökmeden dizinledi" cümleleri kullanıcı yorumları; resmî iddia yalnız "tüm sürücüleri düzleştirilmiş olarak milisaniyede görme". Kesin rakam yalnız indirme boyutu.

| Ölçüt | File Pilot (iddia/yayımlanan) | Gezik (ölçülen) | Koşullar ve not |
|---|---|---|---|
| İkili boyut | **2,46 MB** indirme (v0.8.5; v0.7.0'da 2,08 MB) | **~20,5 MB** exe (5c: 20.467.712 bayt; 5d bütçesi ≤ +1 MB) | Gezik: sürüm derlemesi, Windows. pdfium (~5 MB), ffmpeg, 7-Zip ayrı indirilir. Gezik'in ~7 MB'ı arşiv/dönüştürme kütüphaneleri |
| Açılış | Rakam yok; "anında" | **22-35 ms** pencereye | Gezik: `measure.ps1 -Runs 5`, Windows 11, sürüm derlemesi |
| Boşta bellek | Rakam yok (v0.8.4: COM kütüphanelerini boşaltarak açılış RAM'i düşürüldü) | **6,8-7,0 MB** | Gezik: 900×600 pencere, sistem ikonları; `icons = "gezik"` ile 6,3 MB; pencere büyüdükçe ~4 bayt/piksel artar |
| Büyük klasör | "Tüm sürücü düzleştirilmiş, milisaniyede"; 2,7 M dosya dizinleme (kullanıcı) | 100 bin dosya yüklendikten sonra **17,2 MB**; 100 bin ad sıralama **39 ms** | Gezik'te düzleştirme/dizin yok; klasör *yükleme süresi* ayrıca ölçülmüyor (ön tarama hedefi 100 bin öğe ≤ ~1 sn) |
| Kaydırma | GPU çizim; rakam yok | 100 bin dosyada **~270 ms CPU** (`max-fps = 120`) | Gezik: 359 Hz ekran, yazılım çizici |
| Sekmeler | "Hızlı açılıp kapanıyor" | 20 sekme **+0,1 MB** | Gezik: `tabs.ps1` |
| Kopyalama | Windows'un aktarım penceresi; iddia yok | 10 000 küçük dosya **1,6 s** (Explorer 12,4 s, 7,7×) | Gezik: `ops.ps1`, NVMe |

## 5. Gezik'te olup File Pilot'ta olmayanlar

| Alan | Gezik'te |
|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından; ayar dosyası OS'ler arası aynen taşınır |
| Geri alma | Kopya, taşıma, yeniden adlandırma, yeni öğe, çöp, değiştirme için oturum boyu Ctrl+Z/Y (File Pilot'ta yol haritasında) |
| Kopyalama motoru | Kendi motor: önceden tek listede çakışmalar (Replace/Skip/Keep both/If newer), disk kümesine göre kuyruk, duraklat/iptal, görev çubuğu ilerlemesi, zorla kapanmada toparlama; Explorer'dan 6,7-7,7× hızlı |
| Silme | Anında kalıcı silme (öğe ~31 ms'de kaybolur, arka planda biter, açılışta tamamlanır); değiştirilen dosyalar önce çöpe |
| Pano | Sistem panosu üç OS'de iki yönlü; kesilen öğeler soluk |
| Sürükle-bırak | Sekmeye, adres çubuğu parçasına, sabitlenenler arasına bırakma; bırakma da geri alınır |
| Arşivler | zip, 7z, rar, tar ailesi, cab, iso, cpio/deb açma; nadir biçimler indirilen 7-Zip ile; zip/7z/tar oluşturma, şifre, 7z parçalama, var olan arşive ekleme; yol/bomba güvenliği |
| Dönüştürme | Resim (boyutlandırma, EXIF, konum silme, HEIC/AVIF), metin kodlaması ve satır sonları, ses/video (tek tıkla indirilen ffmpeg) |
| PDF | Resimlerden PDF, birleştirme, bölme, sayfa çıkarma, PDF→resim (5d, indirilen pdfium) |
| Kullanıcı komutları | `[[commands]]`: kabuksuz, güvenli yer tutucular, türe göre menüde, paralel, geri alınabilir |
| Toplu yeniden adlandırma | Regex, şablon alanları, EXIF tarihi, Türkçe büyük/küçük harf, önizlemede elle düzenleme, kayıtlı setler |
| Ayarlar | Düz metin TOML, kaydedince canlı uygulanır, hata satır numarasıyla, yorumlar korunur; `GEZIK_CONFIG_DIR` ile taşınabilir klasör |
| Ölçüm | Yayımlanmış, betikle tekrarlanabilir performans tablosu (README, `scripts/perf/`) |
| Lisans | Ticari olmayan kullanım için ücretsiz, kaynak açık (PolyForm Noncommercial) |

## 6. Öneriler

Değer: kullanıcının her gün hissettiği etki. Emek: Gezik'in bugünkü koduna göre.

| # | Eksik özellik | Değer | Emek | Yol haritasındaki yeri |
|---|---|---|---|---|
| 1 | Klasör içi bulanık süzme (yazarken süz, uzantı süzgeci) | Yüksek | Düşük-orta | *Gelişmiş: arama*'nın ilk dilimi; bağımsız olduğundan öne çekilebilir. `EntryModel` zaten tembel, süzme Rust'ta |
| 2 | Komut paleti (eylem + menü komutu arama, kısayol gösterimi) | Yüksek | Orta | *Gelişmiş*; 25 eylemlik `Action` listesi ve `menu-command` adları hazır. Sonra paletten kısayol atama (dosyaya yorumu koruyarak yazma kalıbı var) |
| 3 | GoTo: yol otomatik tamamlama, son klasörler, `%ORTAM%`/`{home}` | Yüksek | Düşük | Küçük bağımsız iş, şimdi; komut paletiyle aynı açılır liste bileşenini paylaşır |
| 4 | Klasör boyutu ve öğe sayısı arka planda; kenar çubuğunda sürücü doluluk çubuğu | Yüksek | Orta | Görünüm eki; 4a'nın paralel ön taraması (`FindFirstFileExW`) yeniden kullanılır. Sıralamayı bekletmemeli (File Pilot'un düzelttiği hata) |
| 5 | Kapatılan sekmeyi yeniden açma, soldakileri kapatma, sekme arama listesi | Orta | Çok düşük | Küçük iş, şimdi |
| 6 | Bölünmüş/çift panel + diğer panele kopyala/taşı | Yüksek | Yüksek | *Gelişmiş: çift panel* (planlı); tasarımda ikiden çok panele ve kayıtlı düzenlere yer bırakılmalı |
| 7 | Düzleştirilmiş görünüm ve derin arama (alt ağaç/sürücü) | Yüksek | Yüksek | *Gelişmiş: arama*; önce düzleştirme (tarama + akan sonuç), sonra dizin |
| 8 | Group By (tarih/tür/boyut bölümleri) | Orta | Orta | Görünüm eki; Görünüm spec'inde kapsam dışıydı |
| 9 | Grafik Seçenekler ekranı | Orta-yüksek (teknik olmayan kullanıcı) | Orta | *Taşınabilirlik* ile birlikte (dışa/içe aktarma ve tema kurma aynı ekranda) |
| 10 | Ağaç görünümü (kenar çubuğunda veya satır içi) | Orta | Orta-yüksek | *Gelişmiş* öncesi ayrı küçük alt proje |
| 11 | Tarayıcı/e-postadan sanal dosya bırakma | Orta | Düşük-orta | Sürükle-bırak eki (Windows `FileGroupDescriptor`/`FileContents`) |
| 12 | Varsayılan dosya yöneticisi yapma; CLI'ı PATH'e ekleme | Düşük-orta | Orta (kayıt defteri riski) | İlk herkese açık sürüm/kurucu ile |
| 13 | Hiyerarşik görünüm mirası, alternatif satır rengi, son değişen işareti, onay kutuları, köşe/animasyon anahtarları | Düşük | Düşük | Biriken küçük işler |

## 7. Performans dersleri

1. **İkili boyut farkı 8×.** File Pilot 2,5 MB, Gezik ~20 MB; farkın büyük kısmı arşiv ve dönüştürme kütüphaneleri ile Slint. Açılış ve boşta bellek bundan etkilenmiyor (22-35 ms, ~7 MB), ama "küçük ve hızlı" algısında indirme boyutu ilk rakam. Seyrek kullanılan okuyucular (unrar, iso, cab, cpio) pdfium gibi isteğe bağlı indirmeye alınabilir; her alt projede boyut bütçesi tutulmaya devam etmeli.
2. **GPU çizim.** File Pilot D3D11'e geçti. Gezik'in yazılım çizicisi 359 Hz ekranda kaydırmada CPU harcıyor (`max-fps` ile ~270 ms'ye indi). Slint'in GPU arka ucunun (Skia/FemtoVG) boşta bellek maliyeti ölçülüp yüksek yenileme hızlı ekranlarda isteğe bağlı sunulabilir.
3. **Meta veriler listeyi bekletmesin.** File Pilot'un sürüm notlarında "arka plan meta veri işleri sıralamayı bekletiyordu", "küçük resim yüklemesi klasör güncellemesini geciktiriyordu" düzeltmeleri var. Gezik aynı ilkeyle kurulu; klasör boyutu (öneri 4) eklenirken bu kural korunmalı.
4. **Artımlı değişiklik takibi.** File Pilot "yeniden sıralama ve süzmeyi azaltan akıllı değişiklik takibi"ne geçti. Gezik değişiklikte klasörü tümden yeniden yüklüyor (50 bin dosyada %2,3-2,7 CPU); süzme ve gruplama gelince satır bazında güncelleme gerekecek.
5. **Bir kerelik sistem maliyetleri.** File Pilot kullanılmayan COM kütüphanelerini boşaltarak açılış RAM'ini düşürdü. Gezik'te sistem ikonları ve tür adları ~0,5 MB tutuyor; kabuk nesneleri iş bitince bırakılabilir.
6. **Ölçüm yayımlamak avantaj.** File Pilot hiç rakam vermiyor; Gezik'in betikle tekrarlanabilir tablosu bir ayırt edicidir. Eksik olan tek temel ölçü **klasör yükleme süresi** (100 bin dosya, soğuk/sıcak): eklenmeli ki "milisaniyede" iddiasıyla doğrudan kıyaslanabilsin.
7. **Önbellek ayarı.** File Pilot v0.7.1'de ikon/küçük resim önbellek boyutunu ayar yaptı. Gezik'in küçük resim önbelleği sabit ≤ 32 MB; düşük bellekli makineler için ayar olarak açılabilir.

## Kaynaklar

- https://filepilot.tech/ (özellikler, kullanıcı yorumları, v0.8.5 indirme boyutu)
- https://filepilot.tech/roadmap
- https://filepilot.tech/pricing
- https://filepilot.tech/about
- https://filepilot.tech/starlog (v0.6.6 – v0.8.5 sürüm notları)
- https://filepilot.tech/starlog/2026-07-15-v0.8.1
- https://filepilot.tech/starlog/2025-07-07-v0.5.0
- https://filepilot.tech/starlog/2025-11-14-v0.6.3
- https://filepilot.handmade.network/ (teknoloji: C, D3D11/OpenGL, bağımlılıklar)
- https://www.windowslatest.com/2025/02/24/hands-on-with-file-pilot-a-new-alternative-to-windows-11s-file-explorer/ (bölünmüş görünüm, Quick Look, komut paleti Ctrl+Shift+P, klasör boyutları, seçenekler)
- https://forest.watch.impress.co.jp/docs/news/1664864.html (Essential $50 / Pro $200, arka planda boyut hesaplama)
- https://www.justgeek.fr/?p=145707 (C ile yazılmış, ~2,1 MB kurulum)
- Gezik: `README.md`, `docs/superpowers/specs/2026-10-0*-*-design.md`, `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md`, `crates/gezik-config/templates/settings.toml`, `crates/gezik-config/src/shortcuts.rs`
