# Path Finder ile karşılaştırma: Gezik'te eksik olanlar

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** Path Finder 26.1.9 (Cocoatech, yalnız macOS; 26.x macOS 13 Ventura ve sonrası, Intel + Apple Silicon); Gezik `master` (29dcb45) + 5d (PDF, başka dalda; var sayıldı).
- **Kaynaklar:** `store.cocoatech.io/updates` (Path Finder 10.0 → 26.1.9 sürüm notları), Cocoatech yardım merkezinin 50 makalesinin tamamı (`support.cocoatech.com`, Zendesk API'den), eski kılavuzun arşivlenmiş "List of available modules" sayfası, Cocoatech blogu (PF 8, 8.5, 9, 10, 2022, "Licenses are back"), FAQ ve Kullanım Koşulları, Setapp, TidBITS (sonda). `cocoatech.io` artık Flutter ile çizildiği için okunamıyor; `docs.cocoatech.com` kapanmış (yalnız arşivden).
- **Gezik tarafı:** `docs/superpowers/specs/` altındaki beş spec (Kapsam dışı bölümleriyle), README, `settings.toml` şablonu, `gezik-config::shortcuts::Action` (27 eylem), `macos-test-results.md` (macOS menü çubuğu, ⇧⌘., Launch Services tür adları), kodda kısa kontroller.

**Durumlar:** **Var** · **Kısmen** (eksiği yazılı) · **Yok** · **Planlı** (yol haritası adımı yazılı).

**Yol haritası adları:** *Etiketler* (5. spec'te ayrı adım 6; kullanıcının özetinde Gelişmiş'in içinde), *Taşınabilirlik* (dışa/içe aktarma, senkron klasör, GitHub'dan tema), *Bulut senkronu* (ayarların senkronu), *Gelişmiş* (arama, çift panel, arşivin içinde gezinme, Git, komut paleti), *Güncelleme* (gezinme spec'i: "ilk herkese açık sürümden önce").

**Not:** Yardım merkezi makaleleri genel yazılmış ("depending on your version"); ayrıntı gerektiğinde sürüm notları esas alındı. Kullanıcının saydığı "Histogram" modülü hiçbir kaynakta yok (modül listesi 20 modül; en yakını Size Browser'ın halka grafiği). Cover Flow 26'da yok (Apple kaldırdı); tabloya alınmadı.

---

## 1. Özellik tablosu

### 1.1 Gezinme, sekmeler, çift panel

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 1 | Çift panel: her panel tam bir tarayıcı (kendi görünümü, sıralaması, sekmeleri); ayırıcıya çift tıkla ortala, "Equalize Dual Pane" menü öğesi | **Planlı** | Gelişmiş |
| 2 | Paneller arası tek tıkla kopyala/taşı, sürükle-bırak | **Planlı** | Gelişmiş (çift panelle) |
| 3 | Sekmeler: yeni/kapat/döngü, sürükleyerek sırala, orta tıkla yeni sekme (⌥ ile arka planda), orta tıkla kapat | **Var** | Gezik'te orta tık zaten arka planda açar |
| 4 | Birden çok pencere (⌘N), sekmeyi dışarı sürükleyip yeni pencere | **Yok** | Gezik tek pencere |
| 5 | Sekme setleri (File › Tabs › Save Set / Load Set) | **Yok** | |
| 6 | Açılış: son klasörler, belirli klasör ya da boş pencere; "Window › Save As Default" ile pencere düzeni | **Kısmen** | `start-folder` var; oturum geri yükleme gezinme spec'inde bilerek dışarıda |
| 7 | Path Navigator: tıklanabilir/düzenlenebilir yol; "Go to Folder" (tam dosya yolu yapıştırılabilir) | **Var** | Ctrl+L / ⌘L |
| 8 | Bookmarks Bar: araç çubuğu altında yatay yer imleri, Applications/Utilities menüleri | **Yok** | |
| 9 | Kenar çubuğu: Favoriler (sürükle ekle/çıkar/sırala), Aygıtlar, Ağ, Son Belgeler, Etiketler bölümleri; "Reset Sidebar"; yazı/simge ölçeği | **Kısmen** | Sabit klasörler + Sabitlenenler + Sürücüler; Ağ, Son, Etiketler yok |
| 10 | İkinci kenar çubuğu modülü (çift panelde her panele bir tane) | **Yok** | |
| 11 | Son belgeler/klasörler/uygulamalar/sunucular (menü + modül, sayı ayarlı) | **Yok** | |
| 12 | Geri/ileri/üst klasör | **Var** | |
| 13 | Yaylı klasörler (sürüklerken üzerinde bekleyince açılır) | **Kısmen** | Yalnız sekmede; listedeki klasörde yok |
| 14 | Klavyeyle tam gezinme, harfle atlama | **Var** | |
| 15 | USB ile bağlı iOS aygıtlarında gezinme ve dosya işlemi | **Yok** | Gezik'te MTP/telefon da yok |
| 16 | Ağ konumları, sunucuya bağlanma, son sunucular | **Yok** | |

### 1.2 Modüller (raflar)

Path Finder'ın imzası. Pencerenin dört kenarında tam boy "raf" (shelf) var; her rafa istenen sayıda modül konur, sürükleyerek yerleştirilir, yığılır, sıralanır, başka pencereye kopyalanır/taşınır, sol/sağ/iki panele bağlanır; her modülün kendi ayarı var (dişli simgesi). Get Info penceresi ve Inspector da modüllerden kurulur. 26.0'da hepsi yeniden yazıldı.

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 17 | Modül sistemi: dört raf, serbest yerleşim, yığma, pencereler arası kopyalama, panele bağlama, modül başına ayar | **Yok** | Gezik'in düzeni sabit: kenar çubuğu sol/sağ/gizli, önizleme sağda. Temanın düzeni değiştirmesi bilerek dışarıda; ama bu ayarla yapılacak bir yerleşim, yorumlayıcı gerekmez |
| 18 | Preview modülü: QuickLook tabanlı, sözdizimi renklendirme, büyük metin, SVG, ses/video oynatma, "View as Text", metin/RTF'yi yerinde düzenleme | **Kısmen** | Resim, metnin başı, küçük resim; renklendirme, oynatma, düzenleme yok (renklendirme ve oynatma görünüm spec'inde bilerek dışarıda) |
| 19 | Info modülü: tür, boyut, yol, tarihler, öğe sayısı, Spotlight meta verisi (boyut, süre), yorum | **Kısmen** | Ad, tür, boyut, tarihler, öğe sayısı, piksel boyutu; süre ve diğer meta veri yok |
| 20 | Attributes/Permissions: sahip, grup, rwx düzenleme, kilitli/gizli/uzantıyı gizle bayrakları, Spotlight yorumu; ACL düzenleyici | **Yok** | Windows'ta Shell menüsündeki Özellikler'e gidilebilir |
| 21 | Get Info penceresi / Inspector (⌘I), modüllerden kurulur | **Kısmen** | Yalnız Windows'ta Shell menüsünden sistemin Özellikler penceresi |
| 22 | Open With modülü: dosya türünün varsayılan uygulamasını değiştirme | **Kısmen** | Windows'ta Shell menüsünde "Birlikte aç"; varsayılanı değiştirme yok |
| 23 | Processes modülü: süreçler, CPU, bellek, PID, sonlandırma | **Yok** | |
| 24 | Trash modülü: çöpü pencerede görme, içine sürükleme, boşaltma | **Yok** | |
| 25 | Image Browser modülü: klasördeki tüm resimlerin önizlemesi | **Kısmen** | Izgara + küçük resimler aynı işi görür; ayrı panel yok |
| 26 | Selection Path modülü: seçili öğenin yolunu klasör zinciri olarak gösterir | **Kısmen** | Adres çubuğu klasörü gösterir; arama sonuçlarında gereken "seçimin yolu" yok |
| 27 | Size modülü: seçili klasörün boyutunu anında gösterir | **Yok** | |
| 28 | Tags and Rating modülü | **Planlı** | Etiketler (puanlama dahil değil) |

Drop Stack, Terminal, Git ve Hex modülleri kendi bölümlerinde.

### 1.3 Görünümler

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 29 | Simge görünümü: boyut, ızgara aralığı, Clean Up (26.1.5'te yeniden tasarlandı) | **Var** | Üç boyut, Ctrl+tekerlek |
| 30 | Liste görünümü: sütun göster/gizle, başlığı sürükleyerek sırala, başlıkla sıralama (26.1.8'de yeniden tasarlandı) | **Kısmen** | Sütunları sürükleyerek yer değiştirme görünüm spec'inde bilerek dışarıda |
| 31 | Sütun (Miller) görünümü; önizleme sütunu (medya oynatma kontrolleriyle) | **Yok** | |
| 32 | Panel/klasör başına görünüm; "Reset View Settings to Defaults" | **Var** | `views.toml`, "Reset this folder" |
| 33 | Gruplama (Arrange By / Use Groups: tür, uygulama, tarih aralığı, boyut, etiket) | **Yok** | Görünüm spec'inde bilerek dışarıda |
| 34 | Sıralama: ad, değiştirme, oluşturma, son açılma, boyut, tür, etiket | **Kısmen** | Son açılma ve etiket yok |
| 35 | Klasör boyutlarını hesaplama (liste sütununda) | **Yok** | |
| 36 | Gizli dosyaları göster; tüm uzantıları göster | **Kısmen** | Nokta ile başlayanlar için anahtar (macOS'ta ⇧⌘.); uzantı gizleme seçeneği yok (hep görünür) |
| 37 | Görünüm arka plan rengi (üç görünümde ayrı), etiket renginin tam satıra boyanması | **Kısmen** | Tema renkleri var; etiket yok |
| 38 | Quick Look (Boşluk, oklarla sonraki), Aç/Kaydet pencerelerinde de | **Var** | Gezik'in kendi hızlı bakışı; sistemin QuickLook eklentileri kullanılmıyor |
| 39 | Geniş biçim önizlemesi (ARW raw, .webm, SVG, PDF, Office QuickLook ile) | **Kısmen** | Windows'ta sistem küçük resmi; macOS/Linux'ta 5 resim biçimi |
| 40 | Tek/çift tıklama, klasörü aynı/yeni sekmede/yeni pencerede açma ayarı | **Kısmen** | Orta tık yeni sekme; ayar yok |
| 41 | Path Finder Desktop: masaüstünü Finder yerine Path Finder çizer | **Yok** | macOS'a özgü |

### 1.4 Dosya işlemleri ve Drop Stack

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 42 | Kopyala/kes/yapıştır (⌘X ayarla açılır), sürükle-bırak: kenar çubuğu, sekmeler, paneller, modüller | **Var** | Gezik'te kesme her sistemde açık; sistem panosuyla iki yönlü |
| 43 | File Copy Queue: tüm işlemler tek yerde, ilerleme, ETA, duraklat/sürdür/iptal; arşiv işleri de kuyrukta | **Var** | Disk kümesine göre kuyruk; yeniden sıralama yok |
| 44 | **Drop Stack:** farklı klasör ve disklerden öğe toplama (dosyalara dokunmaz), sonra hedefe sürükleyip kopyala/taşı; tek öğe çıkar, temizle; Drop Stack'ten takma ad (alias) oluşturma (26.1.4) | **Yok** | |
| 45 | Klasör birleştirme: aynı adlı klasörün üzerine bırakınca Merge; üzerine yaz / atla / yalnız yeniler | **Var** | Çakışma listesinde klasörler birleşir; Replace / Skip / Keep both / If newer |
| 46 | Çöpe at (onaylı), çöpü boşaltma onayı | **Var** | `confirm-trash`; çöpü boşaltma Gezik'ten yapılmıyor |
| 47 | Secure Delete (1, 7 ya da 35 geçiş üzerine yazma) | **Yok** | SSD/APFS'te etkisi tartışmalı |
| 48 | Geri alma (geri alma paneli; symlink oluşturma da geri alınır) | **Var** | Gezik'te daha geniş (bölüm 2) |
| 49 | Takma ad (alias) ve sembolik bağlantı oluşturma | **Yok** | Dosya işlemleri spec'inde bilerek dışarıda |
| 50 | Yeni dosya oluşturma (Finder'da yok) | **Var** | Boş `New file.txt`; şablon yok |
| 51 | Çoğalt, yerinde yeniden adlandır, yeni klasör | **Var** | |
| 52 | Copy Path (varsayılan UNIX; diğer biçimler alt menüde) | **Kısmen** | Yalnız Windows Shell menüsündeki "Yol olarak kopyala" |
| 53 | Share menüsü/düğmesi: Mail, Messages, Notes, AirDrop, paylaşım uzantıları | **Kısmen** | Windows'ta Shell menüsündeki Paylaş; macOS'ta yok |
| 54 | Services menüsü bağlam menüsünde (26.1'den beri varsayılan açık) | **Yok** | Gezinme spec'inde bilerek dışarıda |
| 55 | Yönetici yetkisiyle dosya işlemleri (ayrıcalıklı yardımcı) | **Yok** | |
| 56 | Batch Select / Select by Name: desen (`IMG_*`, `*.log`) ve özniteliğe göre seçim | **Yok** | |
| 57 | Canlı klasör izleme (FSEvents) | **Var** | `folder_watch.rs`, macOS'ta symlink düzeltmesiyle |
| 58 | Ekran görüntüsü alma (Screen Capture, 26.1.1'de geri geldi) | **Yok** | Dosya yöneticisinin işi değil |

### 1.5 Toplu yeniden adlandırma

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 59 | Batch Rename: Replace Text, Add Text, Format (ad, tarih, sıra numarası), Add Date, Add Sequence; sürüklenebilir adımlar (2153); önizleme | **Var** | Gezik'te regex, canlı önizleme, elle düzeltme, döngü güvenli, EXIF tarihi, kayıtlı setler. PF'de 26.1.9'a kadar "31 dosya sınırı" hatası vardı |

### 1.6 Karşılaştırma, senkron ve disk araçları

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 60 | FolderSync: farkları göster, tek/iki yönlü, çakışma kuralı, ad/tür filtresi | **Yok** | |
| 61 | İki klasörü karşılaştırma (26.0'da eklendi) | **Yok** | Çift panelle birlikte düşünülmeli |
| 62 | Yinelenen dosyaları bulma (26.0'da eklendi) | **Yok** | |
| 63 | İki dosyayı karşılaştırma: metinde yan yana/birleşik fark, ikilide hex fark | **Yok** | |
| 64 | Checksum: MD5, SHA-1, SHA-256, panoya kopyalama (26.1.8'de iyileşti) | **Yok** | Gezik araç indirmede SHA-256 kullanıyor ama kullanıcıya açık değil |
| 65 | Size Browser: boyuta göre ağaç, halka grafik (canlı dolar), oradan açma/silme/taşıma | **Yok** | |

### 1.7 Arama

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 66 | Filter Bar: klasörde yazdıkça süzme | **Kısmen** | Yalnız harfle atlama |
| 67 | Find penceresi, iki mod (Spotlight ve FileSearch/düşük seviye): ad (içerir/başlar/biter/desen), içerik, tarihler, boyut, tür, izinler; çoklu ölçüt | **Planlı** | Gelişmiş |
| 68 | Kayıtlı aramalar (Smart Folder, 7.3'ten beri) | **Yok** | Arama adımına eklenmeli |

### 1.8 Etiketler ve renk etiketleri

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 69 | macOS etiketleri ve renk etiketleri: atama, Tags sütunu, etikete göre sıralama/süzme/arama, kenar çubuğunda etiketler, özel etiket rengi, satırı boyama | **Planlı** | Etiketler |
| 70 | Puan (rating), Spotlight yorumları, OpenMeta etiketleri | **Yok** | Etiketler adımına eklenebilir (yorum = "öğeye not") |

### 1.9 Arşivler

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 71 | Sıkıştırma: ZIP (parolalı; PKWare, AES-128, AES-256), 7z, XAR, TAR, PAX, CPIO + gzip/bzip2/xz | **Var** | Gezik: zip/7z/tar, AES, dosya adı şifreleme, 7z parçalı; XAR/PAX/CPIO oluşturma yok |
| 72 | Açma: ZIP, 7z, XAR, TAR, PAX, CPIO, AR, LHA/LZH, RAR, CAB; çift tıkla aç, "Extract To…" | **Var** | Gezik daha geniş (iso, zst, deb, indirilen 7-Zip ile nadirler); çift tık ayarlı |
| 73 | Arşivin içinde gezinme ("transparent archive browsing", Wikipedia; güncel belgelerde geçmiyor) | **Planlı** | Gelişmiş |

### 1.10 Git ve geliştirici araçları

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 74 | Git: dosyalarda durum işaretleri (modified, staged, untracked, ignored), bağlam menüsünde add/commit/pull/push/revert, satır satır fark; Git modülü | **Planlı** | Gelişmiş |
| 75 | Subversion modülü | **Yok** | Gerek yok |
| 76 | Hex editör: hex + ASCII, arama, yer imleri, düzenleme | **Yok** | |
| 77 | Metin editörü: sözdizimi renklendirme, satır numarası, bul/değiştir, kodlama seçimi | **Yok** | Gezik metin kodlaması/satır sonu dönüştürür, düzenlemez |
| 78 | Man sayfasını PDF'e çevirme; "Reports" menüsü | **Yok** | Düşük değer |
| 79 | Uygulama başlatıcı (Application Launcher) | **Yok** | Dosya yöneticisinin işi değil |

### 1.11 Terminal

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 80 | Gömülü terminal (modül ya da ayrı pencere; 26.0'da yeniden yazıldı): bulunulan klasörü izler, dosyayı sürükleyince yolunu yazar, kabuk/yazı tipi/geçmiş ayarı, SSH (ed25519) | **Yok** | |
| 81 | Harici terminalde açma (Terminal, iTerm) | **Yok** | |

### 1.12 Klavye, komut paleti, özelleştirme

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 82 | Kısayolları görme ve değiştirme (menü öğeleri) | **Var** | `[shortcuts]`, 27 eylem; PF'de çok daha çok eylem var |
| 83 | Araç çubuğu özelleştirme (sürükle; simge/metin/ikisi) | **Yok** | |
| 84 | Ayarlar penceresi (General, Browser, Features); özellikleri tek tek kapatma (Terminal, Git, bulut) | **Kısmen** | Düz metin TOML, canlı; grafik ekran ayarlar spec'inde ayrı alt proje diye dışarıda |
| 85 | Açık/koyu, tek "global renk sistemi" (26.0) | **Var** | Temalar, `theme = "auto"` |
| 86 | Kenar çubuğu yazı/simge ölçeği, modül boyutları | **Kısmen** | `density`, temada `font-size` |
| 87 | Arayüz çevirileri (20 dil) | **Yok** | Gezinme spec'inde bilerek dışarıda |
| 88 | Tam menü çubuğu: her komut menüde | **Kısmen** | macOS'ta File/Edit/View/Go/Window; menü kısayolu değiştirilen tuşu göstermiyor |

Komut paleti Path Finder'da **yok** (hiçbir kaynakta geçmiyor; komutlara menü çubuğundan ulaşılıyor). Gezik'te Planlı (Gelişmiş): bu alanda Gezik öne geçebilir.

### 1.13 Bulut ve paylaşım

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 89 | Cloud Uploader ("Send to Cloud"): SFTP/FTP, Amazon S3 (bölge seçimi), Dropbox, Backblaze B2; yalnız yükleme. Google Drive 2185'te kaldırıldı | **Yok** | Bulut senkronu adımı bu değil (o ayarları taşır) |
| 90 | iCloud/Dropbox/OneDrive senkron durumu ve indirme simgeleri (macOS'un verdiği) | **Yok** | |
| 91 | AirDrop (26.1.4'ten beri yalnız Share üzerinden; AirDrop görünümü kaldırıldı) | **Yok** | macOS'a özgü; satır 53 ile bağlantılı |

### 1.14 macOS bütünleşmesi ve diğerleri

| # | Path Finder özelliği | Gezik | Not |
|---|---|---|---|
| 92 | Finder'ın yerine geçme ("Replace Finder": klasör açma istekleri PF'ye gelir); "Reveal in Finder" ayrı | **Yok** | |
| 93 | Girişte başlatma; Dock/menü çubuğunda görünürlük | **Yok** | |
| 94 | AppleScript desteği | **Yok** | macOS'a özgü |
| 95 | Default Folder X ile uyum (Aç/Kaydet pencerelerinde PF'deki klasör) | **Yok** | macOS'a özgü |
| 96 | Tam Disk Erişimi algılama ve yönlendirme penceresi | **Yok** | Gezik macOS'ta paketlenince gerekecek |
| 97 | Otomatik güncelleme (Sparkle 2), "Release Only / Release and Beta" kanalı, eski sürümleri indirme | **Planlı** | Güncelleme |
| 98 | Geçerli sürüm hızı: 26.0'da daha az kaynak, gizli görünümlerde WindowServer yükü azaltıldı | **Var** | Gezik ölçülerde çok önde (açılış 24-35 ms, boşta ~7 MB, 20 sekme +0,1 MB); PF'nin indirmesi 77 MB (Setapp), Setapp yorumlarında "yavaş, kaynak yiyor" şikâyetleri |

### 1.15 Fiyat ve lisans

Path Finder'ın modeli 2019'dan beri üç kez değişti (yıllık ücretli sürüm → 2022'de yalnız abonelik → Ekim 2022'de "lisanslar geri döndü"). Bugün:

| Seçenek | Fiyat | Kurallar |
|---|---|---|
| Abonelik (e-posta ile, anahtarsız) | $29,95/yıl, kendiliğinden yenilenir; Setapp'te de var | 2 etkinleştirme (hesap başına, artırılabilir); iptalde orantılı iade |
| Lisans anahtarı | 1 yıl $32,95 · 2 yıl $55,95 · 5 yıl $122,95 | 3 etkinleştirme (makine başına). Süre bitince güncelleme hakkı biter; FAQ'a göre bitişten önce çıkmış sürümler çalışmaya devam eder |
| Deneme | 30 gün, kayıtsız | Donanıma bağlı, yeniden kurulumla sıfırlanmaz |
| İade | 30 gün koşulsuz | |

26 ücretli yükseltme (son bir yılda alanlara bir yıl serbest). Kurulum ve etkinleştirme için internet gerekir. Gezik PolyForm Noncommercial: kişisel kullanım ücretsiz, ticari kullanım için yol yok. Path Finder'ın tam tersi: herkes öder, ticari kullanım serbest.

---

## 2. Gezik'te olup Path Finder'da olmayanlar

| Alan | Gezik | Path Finder |
|---|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından | Yalnız macOS (26.x: macOS 13+) |
| Hafiflik | Açılış 24-35 ms, boşta ~7 MB, 100 bin dosya ~17 MB | Cocoa; 77 MB paket, kullanıcılar kaynak tüketiminden yakınıyor, sorun giderme makalesi "modülleri kapatın, klasör boyutunu kapatın" diyor |
| Geri alma | Kopyala, taşı, yeniden adlandır, yeni öğe, çöp, değiştirme, toplu adlandırma, arşiv, dönüştürme; tek Ctrl+Z, oturum boyunca | Geri alma paneli; kapsamı belgelenmemiş |
| Çakışmalar | Başlamadan tüm çakışmalar tek listede, satır başına karar, ezilen dosya çöpe | Birleştirmede üzerine yaz / atla / yalnız yeniler |
| Kalıcı silme | Anında (gizli ada çevir, arka planda sil, çökmede açılışta tamamla) | Secure Delete (yavaş, çok geçişli) |
| Arşivler | Daha çok biçim (iso, zst, deb, cab, rar + indirilen 7-Zip ile nadirler), dosya adı şifreleme, 7z parçalı, var olan arşive ekleme ve üstüne sürükleme, zip bombası ve yol güvenliği | libarchive tabanlı; parçalı arşiv ve arşive ekleme yok |
| Dönüştürme | Resim (JPEG/PNG/WebP/AVIF/HEIC, EXIF döndürme, konum silme), metin kodlaması ve satır sonu, ses/video (ffmpeg tek tıkla) | Yok |
| PDF (5d) | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'ten resim | Yalnız "man sayfasını PDF'e" |
| Toplu adlandırma | Regex, elle düzeltme, döngü güvenli (a↔b), EXIF tarih şablonu, Türkçe büyük/küçük harf, kayıtlı setler | Replace/Add/Format adımları; kayıtlı set belgelenmemiş |
| Kullanıcı komutları | `[[commands]]`: kabuksuz argüman dizisi, güvenli yer tutucular, paralel, geri alınabilir | AppleScript ve Services ile dolaylı |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı, OS'ler arası taşınabilir | plist; "sorun olursa tercihleri silin" (26.0'da bazı ayarlar taşınmadı) |
| Lisans | Kişisel kullanım ücretsiz, kaynak açık | Abonelik ya da süreli lisans, internetle etkinleştirme |
| Yol haritasında | Ayar senkronu (şifreli, hesapsız), GitHub'dan tema, komut paleti | Ayar senkronu ve komut paleti yok |

---

## 3. Path Finder'ın ayırt edici fikirleri

Diğer karşılaştırmalarla (OneCommander, Directory Opus, Far Manager, File Pilot, Files) yan yana:

| Fikir | Path Finder'da | Diğerlerinde |
|---|---|---|
| **Modül sistemi (raflar)** | Dört kenarda tam boy raf, serbest yerleşim, modülü başka pencereye kopyalama, panele bağlama, Get Info'nun da modüllerden kurulması | Hiçbirinde yok. Far'ın Bilgi/Hızlı görünüm panelleri sabit; Opus'un düzenleri araç çubuğu odaklı. ForkLift belgesi henüz yazılıyor; bilinen kadarıyla onda da yok |
| **Drop Stack** | Toplama alanı bir modül: rafa yerleşir, kopyala/taşı/takma ad | Files'ta benzeri "Shelf" var; Opus'ta kalıcı "Collections" (sanal klasör, başka amaç). Fikrin en eskisi PF'de |
| **Tek pencerede araç kutusu** | Süreç görüntüleyici, hex editör, metin editörü, terminal, Git, dosya karşılaştırma aynı pencerede, modül olarak | Far'da süreç listesi ve hex görüntüleyici var; diğerlerinde dağınık |
| **Size Browser halka grafiği** | Ayrı pencerede boyut ağacı + canlı dolan halka grafik | Opus'ta klasör boyutu sütunu; grafik yok |
| **Sekme setleri** | Adlandırılmış sekme takımları | Opus düzenleri, OneCommander adlandırılmış pencereler benzer |
| **Masaüstünü devralma** | Path Finder Desktop | Hiçbirinde yok (macOS'a özgü) |
| **Yalnız yükleyen bulut** | "Send to Cloud": S3, B2, SFTP'ye tek tıkla yükleme, gezinme yok | Opus'ta FTP gezinme; diğerlerinde yok |

Gezik için alınacak ders: modül sistemini olduğu gibi kopyalamak gerekmez. Değerli olan, paneli kullanıcının yerleştirebilmesi (önizleme, bilgi, Drop Stack, terminal) ve bunun `settings.toml`'da bir yerleşim listesi olarak saklanması. Tema yorumlayıcısı gerektirmez, ayarlar spec'indeki "hazır düzen seçenekleri" kararıyla uyumlu.

---

## 4. Sayım

| Durum | Sayı |
|---|---|
| Var | 21 |
| Kısmen | 21 |
| Yok | 48 |
| Planlı | 8 |
| **Toplam** | **98** |

Bu sayım yalnız numaralı satırlardır (1-98); fiyat tablosu, bölüm 2 ve 3 sayılmadı. Komut paleti Path Finder'da olmadığı için satır değildir.

---

## 5. Öneri: değer / emek sırası

Değer: tipik bir kullanıcının günlük işine etkisi. Emek: Gezik'in bugünkü altyapısına göre tahmin (D düşük, O orta, Y yüksek).

| Sıra | Özellik (satır) | Durum | Değer | Emek | Nereye |
|---|---|---|---|---|---|
| 1 | Arama + süzme çubuğu + kayıtlı arama (66-68) | Planlı/Kısmen/Yok | Çok yüksek | O | Gelişmiş'ten öne alınmalı: **Arama** ayrı adım |
| 2 | Çift panel + paneller arası kopyala/taşı (1, 2) | Planlı | Yüksek | Y | Gelişmiş; Miller sütunlarıyla (31) birlikte **Düzenler** adımı |
| 3 | Klasör boyutları + Size modülü + Size Browser (35, 27, 65) | Yok | Yüksek | O | Arama adımı (aynı arka plan tarayıcı ve önbellek); grafik sonra |
| 4 | Harici terminalde aç + yolu kopyala (81, 52) | Yok/Kısmen | Orta-yüksek | D | Yeni küçük adım **Günlük kolaylıklar** |
| 5 | Drop Stack (44) | Yok | Orta-yüksek | D-O | Günlük kolaylıklar; sürükle-bırak, pano ve kuyruk hazır. Önce kenar çubuğunda bir bölüm olarak, raf gelince modül |
| 6 | Oturum geri yükleme + sekme setleri (6, 5) | Kısmen/Yok | Yüksek | D | Günlük kolaylıklar (spec'teki "tek sekme" kararı ayarla seçenek olur) |
| 7 | Etiketler, etikete göre sıralama/süzme, kenar çubuğunda etiketler (69, 34) | Planlı | Orta-yüksek | O | Etiketler; macOS'ta Finder etiketleriyle aynı veriyi kullanmalı |
| 8 | Klasör karşılaştırma + senkron + yinelenen dosya + checksum (60-62, 64) | Yok | Orta-yüksek | O-Y | Yeni adım **Araçlar** (Gezik'in motoru, çakışma listesi ve geri alması hazır); karşılaştırma çift panelden sonra |
| 9 | Zengin önizleme: renklendirme, ses/video, çok sayfalı PDF, meta veri (18, 19, 39) | Kısmen | Orta-yüksek | O-Y | Yeni adım **Önizleme 2**; 5c/5d'nin indirilen ffmpeg ve pdfium'unu kullanır |
| 10 | Yerleştirilebilir paneller (modül sisteminin hafif sürümü) (17) | Yok | Orta | O-Y | Düzenler; `settings.toml`'da yerleşim listesi |
| 11 | Git (74) | Planlı | Orta (geliştiriciye yüksek) | Y | Gelişmiş |
| 12 | Grafik ayar penceresi (84) | Kısmen | Yüksek (teknik olmayan kullanıcı) | Y | Taşınabilirlik (dışa/içe aktarma da arayüz ister) |
| 13 | Gruplama (33) | Yok | Orta | O | Düzenler ya da Görünüm'e ek |
| 14 | Desenle seçme (56) | Yok | Orta | D | Arama adımı (aynı desen eşleyici) |
| 15 | Gömülü terminal (80) | Yok | Orta (geliştiriciye yüksek) | Y | Yerleştirilebilir panellerden sonra; PTY + terminal çizimi üç sistemde pahalı |
| 16 | Bulut durum simgeleri (90) | Yok | Orta | O | Yeni adım **Sistem bütünleşmesi** |
| 17 | Varsayılan dosya yöneticisi / Finder'ın yerine geçme, girişte başlatma (92, 93) | Yok | Orta | O-Y | Sistem bütünleşmesi |
| 18 | İzinler ve öznitelikler (20) | Yok | Düşük-orta | O | Önizleme 2 (bilgi paneline düzenleme olarak) |
| 19 | Arayüz çevirileri, önce Türkçe (87) | Yok | Yüksek (TR kullanıcı) | O | Yeni adım **Yerelleştirme** |
| 20 | Hex/metin görüntüleme, dosya karşılaştırma (76, 77, 63) | Yok | Düşük | O | Önizleme 2'ye salt okunur hex ve metin; editör alınmamalı |

Önerilmeyenler: süreç görüntüleyici (23), Secure Delete (47), uygulama başlatıcı (79), ekran görüntüsü (58), Subversion (75), man→PDF (78). Bunlar dosya yöneticisinin işi değil ya da SSD'de anlamsız; Path Finder'ın kendisi de bunları "Features" ayarıyla kapatmayı öneriyor.

**Özet öneri**

- Var olan adımlara eklenecekler: arama + süzme + kayıtlı arama + klasör boyutu + desenle seçme → *Gelişmiş*'ten çıkarılıp **Arama**; grafik ayar penceresi → *Taşınabilirlik*; puan ve yorum → *Etiketler*; çift panel + Miller sütunları + yerleştirilebilir paneller + gruplama → tek **Düzenler** işi.
- Yeni adım gerekenler: **Günlük kolaylıklar** (terminalde aç, yolu kopyala, oturum, sekme setleri, Drop Stack), **Araçlar** (karşılaştırma, senkron, yinelenen, checksum, Size Browser), **Önizleme 2**, **Sistem bütünleşmesi** (bulut durumu, varsayılan yönetici, girişte başlatma), **Yerelleştirme**.
- Path Finder'dan alınacak en özgün fikir Drop Stack'tir: emeği düşük, Gezik'in var olan sürükle-bırak ve kuyruk altyapısına oturur, OneCommander ve Opus'ta yok. Modül sistemi tam haliyle pahalı; "yerleştirilebilir paneller" olarak Düzenler adımında düşünülmeli.
- *Gelişmiş* adımı Arama ve Düzenler çıkınca Git, komut paleti ve arşivin içinde gezinmeyle kalır. Komut paleti Path Finder'da yok: Gezik'in ayırt edici yanı.
- Ticari fark: Path Finder yıllık ödeme ister ve internetle etkinleştirilir; Gezik kişisel kullanımda ücretsiz ama ticari kullanımda kapalı.

---

## Kaynaklar

Cocoatech'in kendi kaynakları:

- https://cocoatech.io/ (ana sayfa; Flutter, metin okunamıyor)
- https://store.cocoatech.io/updates (sürüm notları: 10.0 → 26.1.9)
- https://cocoatech.io/terms-of-service/ (lisans, abonelik, iade)
- https://support.cocoatech.com/hc/en-us (yardım merkezi; 50 makalenin tamamı `https://support.cocoatech.com/api/v2/help_center/en-us/articles.json` ile okundu). Başlıcaları:
  - …/articles/43462431781396-What-is-Path-Finder
  - …/articles/43462660340244-Path-Finder-User-Interface-Overview
  - …/articles/43462786920212-Modules-What-They-Are-and-How-to-Use-Them
  - …/articles/43463285047956-Module-Settings-and-Per-Module-Options
  - …/articles/43463478327956-Dual-Pane-View-and-Layout-Tips
  - …/articles/43463424936596-Views-Icon-List-Column
  - …/articles/43462942354196-Tabs-and-Tab-Presets
  - …/articles/43462979587732-Use-the-Bookmarks-Bar
  - …/articles/43462860207764-Use-the-Sidebar-for-Faster-Navigation
  - …/articles/43463169352980-Quick-Look-and-Preview-Your-Files
  - …/articles/43505869579796-Drop-Stack-Temporary-Holding-Area-for-Files
  - …/articles/43505294193428-Copying-and-Moving-Files
  - …/articles/43506640513300-File-Copy-Queues
  - …/articles/43505296286996-Deleting-and-Secure-Deleting-Files
  - …/articles/43506696739860-Batch-Renaming
  - …/articles/43505511434516-FolderSync-Synchronize-Folders
  - …/articles/43505575527060-Folder-Merging
  - …/articles/43505331612564-File-Comparison (checksum dahil)
  - …/articles/43505761188884-Size-Browser
  - …/articles/43505688857748-Filtering-and-Low-Level-Search
  - …/articles/43505619435540-Smart-Sorting
  - …/articles/43505714033940-Arrange-Files
  - …/articles/43505322799764-File-Tagging-and-Color-Labels
  - …/articles/43505626742164-Archiving-Files-Compress-and-Extract
  - …/articles/43505345615636-Git-Integration
  - …/articles/43505313427988-Terminal-Integration
  - …/articles/43505333954836-Hex-Editor
  - …/articles/43505359153428-Text-Editor
  - …/articles/43505404022676-Process-Viewer
  - …/articles/43505871189012-Attributes-and-Permissions
  - …/articles/43505891245588-Info-Panel-Detailed-File-and-Folder-Information
  - …/articles/43505823734036-Cloud-Uploader
  - …/articles/43505794853396-Dropbox-Integration
  - …/articles/43505871765524-iCloud-Drive-and-Path-Finder
  - …/articles/43505842124436-AirDrop-Support
  - …/articles/43505990015764-Share-Files-to-Other-Applications
  - …/articles/43506008623508-Make-Path-Finder-Your-Default-File-Manager-Replace-Finder
  - …/articles/43506004951316-Keyboard-Shortcuts-in-Path-Finder
  - …/articles/43462737976468-Customize-the-Toolbar
  - …/articles/43505977206164-Settings-General · …/43505959138964-Settings-Browser · …/43505960471060-Settings-Features
  - …/articles/43506073885332-Use-Quick-Look-everywhere-Spacebar
  - …/articles/43506126039060-Most-Common-Path-Finder-Issues-and-How-to-Fix-Them
  - …/articles/43462450454292-System-Requirements-for-Path-Finder
  - …/articles/43462585359508-Start-a-Trial-and-Activate-Path-Finder
  - …/articles/43462679410452-Manage-Activations-and-Multiple-Macs
- http://web.archive.org/web/2023/https://docs.cocoatech.com/?docs=getting-started/modules/list-of-available-modules (20 modülün listesi)
- Blog (arşivden): https://cocoatech.io/post/introducing-path-finder-8/ · https://www.cocoatech.io/post/introducing-path-finder-9 · https://cocoatech.io/post/welcome-to-path-finder-8-5/ (arşiv biçimleri, geri alma paneli) · https://cocoatech.io/post/introducing-path-finder-10/ · https://cocoatech.io/post/introducing-a-new-path-finder-for-2022/ · https://www.cocoatech.io/post/licenses-are-back · https://www.cocoatech.io/faq

Destekleyici:

- https://setapp.com/apps/path-finder (26.1.9, macOS 13+, 77,4 MB)
- https://tidbits.com/watchlist/path-finder-26/ · https://talk.tidbits.com/t/path-finder-26-0/33442 · https://talk.tidbits.com/t/path-finder-26-1-6/33761 · https://tidbits.com/tag/path-finder/ (fiyatlar, 26.x özetleri)
- https://bundlehunt.com/deal/pathfinder-macos (Path Finder 10 özellik listesi: secure delete geçişleri, ACL, OpenMeta, uygulama başlatıcı)
- https://en.wikipedia.org/wiki/Path_Finder (tarihçe, arşivin içinde gezinme)
- https://www.mactech.com/2015/11/20/path-finder-upgrade-adds-saved-searches (kayıtlı aramalar)
