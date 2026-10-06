# Files ile karşılaştırma: Gezik'te eksik olanlar

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** Files v4.2.38 (2026-10-05; files.community, GitHub `files-community/Files`), Gezik `master` (29dcb45) + 5d (PDF, ayrı dalda bitiriliyor; "var" sayıldı)
- **Yöntem:** Files'ın sitesi, belgeleri (`/docs`), ayar sayfaları, sürüm notları (v4.0–v4.2.38), GitHub deposundaki eylem/önizleme/özellikler klasörleri ve performans hataları. Gezik için `docs/superpowers/specs/`, README, `settings.toml` şablonu ve `gezik-config/src/shortcuts.rs`.
- **Durumlar:** **Var** · **Kısmen** (eksik yazılı) · **Yok** · **Planlı** (adımı yazılı)
- **Kalan yol haritası:** Taşınabilirlik (dışa/içe aktarma, senkron klasör, GitHub'dan tema) → Bulut senkronu → Gelişmiş (arama, çift panel, etiketler, Git, komut paleti; spec'e göre arşivin içinde gezinme ve arşivden silme de burada). Güncelleme bildirimi ilk herkese açık sürümden önce.

## 1. Özet

- Files, Windows'a özgü (Windows 10/11, WinUI 3, .NET 10, C#) ve özellik bakımından çok geniş bir dosya yöneticisi: 150'den fazla eylem, çift panel, sütunlar düzeni, etiketler, Git, FTP, bulut sürücüleri, ayrıntılı Özellikler penceresi.
- Gezik'in önünde olduğu yerler: dosya işlemleri motoru (çakışma listesi, oturum boyu geri alma, kuyruk), toplu yeniden adlandırma, arşiv, dönüştürme, PDF, kullanıcı komutları, üç işletim sistemi ve performans (açılış ~30 ms, boşta ~7 MB; Files'ta normal kullanımda ~86 MB, hata kayıtlarında 1-2 GB ve 45 sn açılış).
- En büyük boşluklar Gelişmiş adımında zaten planlı (arama, çift panel, komut paleti, etiketler, Git). Planda olmayan ama kolay ve değerli olanlar: sekme kolaylıkları, "yolu kopyala / terminalde aç", bulut klasörlerinin kenar çubuğunda görünmesi, ağaç kenar çubuğu, önizlemenin genişletilmesi.

## 2. Özellik özellik karşılaştırma

### 2.1 Gezinme: sekmeler, çift panel, sütunlar

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 1 | Sekmeler: aç/kapat, sonraki/önceki, çoğalt, sürükleyerek sırala, orta tıkla klasörü sekmede aç | Hepsi var (Ctrl+T/W, Ctrl+Tab, sekme menüsünde Çoğalt / Diğerlerini kapat) | **Var** |
| 2 | Kapatılan sekmeyi geri aç (Ctrl+Shift+T) | — | **Yok** |
| 3 | Numaralı sekmeye geç (Ctrl+1…9) | — (Ctrl+1/2 görünüm değiştirir) | **Yok** |
| 4 | Sekmeyi koparıp yeni pencere yapma, pencereler arası sekme taşıma | — | **Yok** |
| 5 | Dosya sürüklerken sekmenin üstünde bekleyince o sekmeye geçme | Var | **Var** |
| 6 | Açılışta önceki sekmeleri geri yükleme ("Continue where you left off") | Tek sekme + `start-folder`; bilerek kapsam dışı | **Yok** |
| 7 | Çift panel (yatay/dikey bölme, sekmeyi panele sürükleyerek bölme, "Open in other pane", Ctrl+Shift+S) | Gelişmiş adımında | **Planlı** (Gelişmiş) |
| 8 | Sütunlar (Columns / Miller) düzeni | — | **Yok** |
| 9 | Ağaç görünümlü kenar çubuğu (v4.2) | Düz liste: FOLDERS, PINNED, DRIVES | **Yok** |
| 10 | Kenar çubuğuna sabitleme, sürücüler, bilinen klasörler | Var; sabitlenenler `settings.toml`'da taşınabilir | **Var** |
| 11 | Ana sayfa widget'ları (Hızlı erişim, sürücüler, ağ konumları, etiketler, son dosyalar) | "This PC" yalnız sürücüleri gösterir | **Kısmen** (son dosyalar, ağ, etiket widget'ı yok) |
| 12 | Breadcrumb + yazarak yol (Ctrl+L) | Var | **Var** |
| 13 | Breadcrumb'da alt klasör açılır listesi (chevron) | Bilerek kapsam dışı | **Yok** |
| 14 | Geri/ileri/yukarı, fare yan tuşları | Var | **Var** |
| 15 | Boş alana çift tıkla üst klasöre çık | — | **Yok** |
| 16 | Ağ konumları, Ağ sayfası (ağ keşfi) | Bağlı ağ sürücüleri DRIVES'ta | **Kısmen** (ağ keşfi, ağ konumu ekleme yok) |
| 17 | FTP / FTPS bağlantısı (SFTP yok) | — | **Yok** |
| 18 | Geri Dönüşüm Kutusu görünümü: geri yükle, tümünü geri yükle, boşalt | Çöpe atma + Ctrl+Z ile geri getirme | **Kısmen** (çöpü gezme, geri yükleme, boşaltma yok) |
| 19 | Windows Kitaplıkları, WSL dağıtımları kenar çubuğunda | — | **Yok** |
| 20 | Yeni pencere, birden çok pencere | Program ikinci kez açılabilir; "yeni pencere" komutu yok | **Kısmen** |
| 21 | Tam ekran, "compact overlay" pencere | — | **Yok** |

### 2.2 Görünümler, önizleme ve önizleme bölmesi

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 22 | Ayrıntılar görünümü: sütunlar, başlıktan sıralama, sütun göster/gizle, genişlik | Var | **Var** |
| 23 | Sütun genişliğini içeriğe göre otomatik ayarlama (çift tık) | Bilerek kapsam dışı | **Yok** |
| 24 | Izgara görünümü + boyut kaydırıcısı / Ctrl+tekerlek | Izgara 3 boyut, `mod`+tekerlek | **Var** |
| 25 | Kartlar (Cards) ve sade Liste düzenleri | `density = compact` var; Kartlar yok | **Kısmen** (Kartlar düzeni yok) |
| 26 | Uyarlanır düzen (içeriğe göre otomatik) | — | **Yok** |
| 27 | Klasör başına görünüm, ya da her yerde aynı | Klasör başına istisna (500), "Apply to all folders" | **Var** |
| 28 | Gruplama (ad, tarih, boyut, tür, etiket; tarih birimi) | Bilerek kapsam dışı | **Yok** |
| 29 | Sıralama: ad, değiştirme, oluşturma, boyut, tür (+ etiket, yol, silinme tarihi, senkron durumu) | İlk beşi var, Türkçe doğal sıralama | **Kısmen** (etiket/yol/senkron durumu yok) |
| 30 | Sıralama önceliği: klasörler önce / dosyalar önce / karışık | Klasörler hep önce | **Kısmen** (seçenek yok) |
| 31 | Klasörde süzme (filtre başlığı, Ctrl+Shift+F) | — | **Yok** |
| 32 | Gizli öğeler, nokta dosyaları, korumalı sistem dosyaları, ADS göster | `toggle-hidden` yalnız noktayla başlayanlar | **Kısmen** (gizli öznitelik, sistem dosyası, ADS ayrımı yok) |
| 33 | Dosya uzantılarını göster/gizle; uzantı değişince uyarı | — | **Yok** |
| 34 | Seçim onay kutuları | Bilerek kapsam dışı | **Yok** |
| 35 | Tek tıkla açma (dosya/klasör, fare/dokunmatik ayrı), üstüne gelince seçme | — | **Yok** |
| 36 | Klasör boyutlarını hesaplama | Önizlemede yalnız öğe sayısı | **Yok** |
| 37 | Boyut birimi seçimi (ikili MiB / ondalık MB) | Sabit 1024 tabanlı | **Yok** |
| 38 | Tarih biçimi: uygulama / sistem / evrensel | Windows'ta sistem kısa biçimi, diğerlerinde ISO; seçilemez | **Kısmen** |
| 39 | Küçük resimler (aç/kapat) | `thumbnails`; resim, video, PDF | **Var** |
| 40 | Klasör başına arka plan resmi (`desktop.ini`) | — | **Yok** |
| 41 | Önizleme bölmesi: resim, metin, klasör, dosya bilgisi | Var (Alt+P) | **Var** |
| 42 | Önizleme: kod (sözdizimi renkli), Markdown, HTML, RTF | Düz metin (ilk 64 KB) | **Kısmen** (renklendirme ve biçimli görüntü yok) |
| 43 | Önizleme: PDF sayfası | Sistem küçük resmi | **Kısmen** (sayfa gezme yok; 5d'nin pdfium'u kullanılabilir) |
| 44 | Önizleme: video/ses oynatma, süre ipucu | Bilerek kapsam dışı | **Yok** |
| 45 | Önizleme: Windows önizleme işleyicileri (Office vb.) | Bilerek kapsam dışı | **Yok** |
| 46 | Ayrıntılar bölmesi: meta veri (EXIF, medya), etiketler, Git bilgisi | Ad, tür, boyut, tarihler, piksel boyutu | **Kısmen** (zengin meta veri yok) |
| 47 | Boşlukla hızlı bakış | Gezik'in kendi penceresi; Files'ta QuickLook / Peek / SeerPro ayrıca kurulmalı | **Var** |

### 2.3 Dosya işlemleri

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 48 | Kes / kopyala / yapıştır, sistem panosu | Üç sistemde iki yönlü | **Var** |
| 49 | Sürükle-bırak (içeride ve diğer programlarla), sekmeler arası | Var; sağ tuşla "sor" | **Var** |
| 50 | Çöpe at / kalıcı sil, silme onayı (her zaman / yalnız kalıcı / asla) | `confirm-trash`, kalıcıda soru | **Var** |
| 51 | Geri al / yinele | Oturum boyu, tüm işlemler (ezilen dosyalar dahil) | **Var** |
| 52 | Yeni klasör, yeni dosya | Var | **Var** |
| 53 | Seçimle yeni klasör (seçilenleri yeni klasöre koy) | — | **Yok** |
| 54 | Kısayol oluştur, kısayol olarak yapıştır | Yalnız Windows Shell menüsündeki "Kısayol oluştur" | **Kısmen** |
| 55 | Seçili klasörün içine yapıştır | "Paste into folder" | **Var** |
| 56 | Yolu kopyala (tırnaklı / tırnaksız) | — | **Yok** |
| 57 | Klasörü düzleştir (deneysel) | — | **Yok** |
| 58 | Durum merkezi (ilerleme, iptal) | İlerleme paneli, duraklat/iptal, görev çubuğu ilerlemesi | **Var** |
| 59 | Panodaki ekran görüntüsünü dosya olarak yapıştır | Bilerek kapsam dışı | **Yok** |
| 60 | Resmi sola/sağa döndür | Dönüştürmede yalnız EXIF'e göre | **Kısmen** (elle döndürme yok) |
| 61 | Masaüstü/kilit ekranı arka planı yap, font/sertifika kur, sürücü biçimlendir, yönetici olarak çalıştır | Yalnız Windows Shell menüsü üzerinden | **Kısmen** (macOS/Linux'ta yok) |
| 62 | Raf (Shelf): dosyaları geçici olarak toplayıp başka yere taşıma | — | **Yok** |
| 63 | Medya dosyalarına meta veri ekleme, albüm kapağı kaldırma | — | **Yok** |

### 2.4 Arama

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 64 | Ad araması (klasör ve alt klasörler, Ctrl+F, Omnibar) | — | **Planlı** (Gelişmiş) |
| 65 | Dizinli içerik araması (Windows Search) | — | **Planlı** (Gelişmiş; sistem dizini Spotlight/tracker ile platform başına) |
| 66 | Öneri listesi, `tag:` sözdizimi | — | **Planlı** (Gelişmiş) |
| 67 | Ayarlar içinde arama | Ayarlar TOML dosyası | **Yok** |
| 68 | Listary entegrasyonu | — | **Yok** |

### 2.5 Etiketler

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 69 | Renkli etiket oluştur / düzenle / sil | — | **Planlı** (Gelişmiş) |
| 70 | Etiket atama: menü, ayrıntılar bölmesi, araç çubuğu, kenar çubuğuna sürükleme | — | **Planlı** (Gelişmiş) |
| 71 | Kenar çubuğunda etiketler, etikete göre süzme/sıralama/gruplama, etiket sütunu | Kenar çubuğu bölümleri buna hazır | **Planlı** (Gelişmiş) |
| 72 | Dosyayla taşınan saklama (Files: ADS, yalnız NTFS) | Gezik planı: macOS etiketlerini tüm sistemlere | **Planlı** (Gelişmiş) |

### 2.6 Tema ve özelleştirme

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 73 | Açık / koyu / sistemi izle | `theme = "auto"`, `theme-light`/`theme-dark` | **Var** |
| 74 | Hazır temalar (Glass, Finder, Nord, Dracula) | Yerleşik açık/koyu + kullanıcı TOML temaları; hazır galeri yok | **Kısmen** (GitHub'dan tema kurma Taşınabilirlik'te) |
| 75 | Bölge bölge arka plan renkleri | Tema dosyasında tüm renkler, ikon renkleri dahil | **Var** |
| 76 | Mica / Mica Alt / Acrylic saydamlık | — | **Yok** |
| 77 | Arka plan resmi / GIF | — | **Yok** |
| 78 | Yazı tipi seçimi | Temada `font-family`, `font-size` | **Var** |
| 79 | Tema değiştirme kısayolu (Ctrl+Alt+T) | — | **Yok** |
| 80 | Araç çubuğu özelleştirme (düğme seçimi, sıra, etiket) | — | **Yok** |
| 81 | Araç çubuğu / durum çubuğu / kenar çubuğu gizleme | Kenar çubuğu sol/sağ/gizli | **Kısmen** (araç ve durum çubuğu gizlenmez) |
| 82 | Arayüz dili (40+ dil) | İngilizce; bilerek kapsam dışı | **Yok** |
| 83 | Grafik ayarlar sayfası | Yalnız `settings.toml` (canlı uygulanır, hatalar satırıyla) | **Kısmen** (arayüz yok) |
| 84 | Ayarları dışa / içe aktarma | Dosyalar zaten taşınabilir | **Planlı** (Taşınabilirlik) |
| 85 | Windows açılışında başlat, arka planda çalış, sistem tepsisi | Gereksiz (açılış ~30 ms) | **Yok** |
| 86 | Varsayılan dosya yöneticisi yapma (Win+E, deneysel) | — | **Yok** |

### 2.7 Arşivler

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 87 | zip / 7z oluşturma (şifreli) | zip, 7z, tar ailesi; şifre, parçalı 7z, var olana ekleme | **Var** |
| 88 | Açma: buraya, klasöre, seçilen yere (akıllı) | Var; zip-slip ve sıkıştırma bombası korumalı | **Var** |
| 89 | rar, 7z ve diğer biçimleri açma | rar, cab, iso, deb…; nadirler indirilen 7-Zip ile | **Var** |
| 90 | zip'in içinde klasör gibi gezinme | — | **Planlı** (Gelişmiş: arşivin içinde gezinme ve arşivden silme) |

### 2.8 Git

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 91 | Durum çubuğunda dal, yerel/uzak commit farkı | — | **Planlı** (Gelişmiş) |
| 92 | Dal oluştur/değiştir; init, fetch, pull, push, sync; GitHub yetkilendirme | — | **Planlı** (Gelişmiş) |
| 93 | Git sütunları (durum, son commit, hash, yazar) | — | **Planlı** (Gelişmiş) |
| 94 | GitHub deposu klonlama (URL sürükleme, palet) | — | **Planlı** (Gelişmiş) |
| 95 | Klasörü/depoyu IDE'de aç | `[[commands]]` ile `folders = true` yapılabilir; hazır eylem yok | **Kısmen** |

### 2.9 Bulut sürücüleri

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 96 | 21 sağlayıcının (OneDrive, Google Drive, iCloud, Dropbox, Proton Drive…) klasörlerini kenar çubuğunda otomatik gösterme | Kullanıcı elle sabitleyebilir | **Yok** |
| 97 | Senkron durumu (sütun / sıralama), OneDrive kotası | — | **Yok** |

Not: Gezik'in "Bulut senkronu" adımı ayarların cihazlar arası senkronudur; bulut sürücüleriyle ilgili değildir.

### 2.10 Klavye ve komut paleti

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 98 | Komut paleti (Ctrl+Shift+P, 150+ eylem, kısayollarıyla) | — | **Planlı** (Gelişmiş) |
| 99 | Kısayolları yeniden atama / kaldırma / ekleme | `[shortcuts]`, 27 eylem, canlı | **Var** |
| 100 | Kısayolları arayüzden düzenleme (Actions sayfası) | Yalnız TOML | **Kısmen** |
| 101 | Harfle atlama, klavyeyle seçim | Var | **Var** |
| 102 | Alt ile menü erişim tuşları | — | **Yok** |

### 2.11 Toplu yeniden adlandırma

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 103 | Çoklu seçimde tek ad + sıra numarası (Explorer gibi) | Kurallar (regex, numara, şablon, EXIF tarihi, büyük/küçük harf), canlı önizleme, elle düzenleme, kayıtlı setler, tek Ctrl+Z | **Var** |

### 2.12 Shell eklentileri ve sağ tık menüsü

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 104 | Windows Shell sağ tık menüsü (üçüncü parti eklentiler) | Var; Gezik öğeleri en üstte | **Var** |
| 105 | Shell eklentilerini alt menüye taşıma, varsayılan öğeleri gizleme | — | **Yok** |
| 106 | "Birlikte aç" araç çubuğu açılır listesi | Shell menüsünde "Birlikte aç" (Windows) | **Kısmen** (macOS/Linux'ta yalnız varsayılan uygulama) |
| 107 | Terminalde aç (yönetici olarak da) | — | **Yok** |
| 108 | Paylaş (Windows paylaşım penceresi) | Shell menüsünde (Windows) | **Kısmen** |

### 2.13 Özellikler penceresi

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 109 | Kendi Özellikler penceresi: Genel, Ayrıntılar (meta veri düzenleme), Güvenlik (ACL), İmzalar, Uyumluluk, Özelleştirme, Kısayol, Kitaplık | Windows'ta Shell menüsündeki sistem "Özellikler"i; önizleme bölmesinde temel bilgiler | **Kısmen** (macOS/Linux'ta özellikler penceresi yok; düzenleme yok) |
| 110 | Hash görme, kopyalama, karşılaştırma (MD5, SHA…) | — | **Yok** |

### 2.14 Performans ve açılış

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 111 | Daha hızlı açılış için arka planda çalışmayı sürdürme | Gerek yok: açılış 24-35 ms | **Var** (gerekmeden) |

Ölçümler 3. bölümde.

### 2.15 Diğer

| # | Files özelliği | Gezik | Durum |
|---|---|---|---|
| 112 | Uygulama içi güncelleme bildirimi (Store / appinstaller) | Yol haritasında: ilk herkese açık sürümden önce | **Planlı** (güncelleme adımı) |
| 113 | Komut satırından klasörle açma | Var | **Var** |
| 114 | Günlük dosyasını açma (Ctrl+.) | — | **Yok** |
| 115 | Tümünü oynat, slayt gösterisi | Varsayılan uygulamayla açma | **Yok** |
| 116 | Dokunmatik uyum, sağdan sola (RTL) | — | **Yok** |
| 117 | Storage Sense, Başlat'a sabitleme (Windows'a özgü) | — | **Yok** |

### 2.16 Durum sayıları

| Durum | Sayı |
|---|---|
| Var | 30 |
| Kısmen | 23 |
| Yok | 48 |
| Planlı | 16 |
| **Toplam** | **117** |

## 3. Gezik'te olup Files'ta olmayanlar

| Alan | Gezik | Files |
|---|---|---|
| Platform | Windows, macOS, Linux; tek kod | Yalnız Windows 10/11 |
| Açılış | 24-35 ms (sürüm derlemesi, Windows 11) | Açılışı hızlandırmak için arka planda çalışmayı sürdürme seçeneği var (SSS: kapatınca açılış yavaşlar). v4.2.34'te ~45 sn bildirimi (#18955, 2026-09-15) |
| Boşta bellek | 6,8 MB; 100 bin dosyalı klasörde 17,2 MB; 20 sekme +0,1 MB | Normal gezinmede ~86 MB (#18984, v4.2.37); arama sonrası >500 MB; uzun oturumda veya 1-2 sekmeyle >1 GB (#18982), 2 GB+ (#18567). SSS bellek sızıntılarını kabul ediyor |
| Kurulum | Tek exe (~13 MB), yönetici izni yok | MSIX; Store sürümü ücretli, sideload ücretsiz; .NET 10 çalışma zamanı, Windows Update servisleri gerekir |
| Çakışmalar | Önce taranır, tek listede satır başına karar (Replace / Skip / Keep both / If newer); ezilen dosyalar çöpe | Kendi çakışma diyaloğu (öğe başına seçim); "If newer" ve ezileni çöpe atma yok |
| Geri alma | Oturum boyu çok adım: kopyala, taşı, sürükle-bırak, ad, yeni öğe, çöp, ezme, arşiv, dönüştürme | Geri al/yinele var; kapsamı belgelenmemiş |
| Kopyalama motoru | Disk kümesine göre kuyruk, SSD'de paralel kopya (`copy-threads`), duraklatma, anında görünen kalıcı silme | Windows Shell kopyası |
| Toplu yeniden adlandırma | Kural motoru, regex, EXIF tarihi, önizleme, kayıtlı setler | Yalnız ad + numara |
| Arşiv | tar ailesi, rar, cab, iso, deb, parçalı 7z, var olana ekleme, sürükleyerek arşive ekleme, güvenlik denetimleri | zip/7z oluşturma, açma |
| Dönüştürme | Resim (HEIC dahil), metin kodlaması ve satır sonu, ses/video (ffmpeg tek tıkla indirilir) | Yok |
| PDF (5d) | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'den resim | Yok |
| Kullanıcı komutları | `[[commands]]`: kabuksuz, türe göre menüde, paralel, geri alınabilir | Yok (yalnız PowerShell betiği çalıştırma) |
| Hızlı bakış | Yerleşik (Boşluk) | Üçüncü parti uygulama gerekir |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı, hatalar satır numarasıyla; her sistemde aynı dosya | JSON + ayarlar sayfası |
| Tema | Tüm renkler, ikon türü renkleri, odak halkası, çerçeve seçimi rengi; `base` ile kalıtım | Arka plan renkleri, saydamlık malzemeleri |
| Sıralama | Türkçe harf sırasıyla doğal sıralama, her sistemde aynı | Windows sıralaması |
| Kare sınırı | `max-fps` ile yüksek tazelemeli ekranda CPU tasarrufu | — |
| macOS | Yerel menü çubuğu, ⌘ kısayolları | — |

## 4. Öneri: eksikleri değer/çabaya göre sıralama

Değer: tipik kullanıcı (Explorer'dan gelen) için. Çaba: Gezik'in mevcut mimarisine göre kabaca (D düşük, O orta, Y yüksek).

| Sıra | Eksik | Değer | Çaba | Yol haritasında |
|---|---|---|---|---|
| 1 | Arama: ad araması + bulunulan klasörde süzme (Ctrl+F / Ctrl+Shift+F) | Çok yüksek | O (süzme D) | Gelişmiş. Süzme tek başına öne alınabilir |
| 2 | Çift panel | Yüksek | O | Gelişmiş |
| 3 | Sekme kolaylıkları: kapatılanı geri aç, Ctrl+1…9, isteğe bağlı oturum geri yükleme | Yüksek | D | Yok → yeni küçük adım ("Gezinme cilası") |
| 4 | Yolu kopyala, terminalde aç, IDE'de aç | Yüksek | D | Yok → aynı küçük adım |
| 5 | Komut paleti | Yüksek | O (eylem listesi `Action` ile hazır) | Gelişmiş |
| 6 | Bulut klasörlerini kenar çubuğunda otomatik gösterme | Orta-yüksek | D-O (platform başına bilinen yollar) | Yok → küçük adım veya Gelişmiş |
| 7 | Arşivin içinde gezinme | Orta-yüksek | O (5b'nin okuyucuları hazır) | Gelişmiş |
| 8 | Etiketler | Orta | O-Y (üç sistemde saklama) | Gelişmiş |
| 9 | Ağaç görünümlü kenar çubuğu | Orta | O | Yok → yeni adım (Görünüm 2) |
| 10 | Önizlemeyi genişletme: PDF sayfası (5d'nin pdfium'u), kod renklendirme, Markdown, video/ses | Orta | O | Yok → yeni adım (Görünüm 2) |
| 11 | Gruplama, klasör boyutu, uzantı gizleme, sıralama önceliği | Orta | O | Yok → Görünüm 2 |
| 12 | Hash görme/karşılaştırma; macOS/Linux için kendi Özellikler penceresi | Orta | D (hash) / O (pencere) | Yok → Gelişmiş'e eklenebilir |
| 13 | Çöp Kutusu görünümü (geri yükle, boşalt) | Orta | O (platform başına) | Yok → yeni |
| 14 | Git | Orta (geliştiriciler için yüksek) | Y | Gelişmiş |
| 15 | Ayarları dışa/içe aktarma | Düşük-orta (dosyalar zaten taşınabilir) | D | Taşınabilirlik |
| 16 | Arayüz dili (çeviri) | Orta (geniş kitle için yüksek) | Y (tüm metinler) | Yok → yeni adım, herkese açık sürümden önce düşünülmeli |
| 17 | Grafik ayarlar sayfası, kısayol düzenleyici | Orta | Y | Yok (bilerek kapsam dışı) |
| 18 | Sütunlar (Miller) düzeni | Düşük-orta | Y | Yok |
| 19 | FTP/SFTP | Düşük-orta | Y | Yok |
| 20 | Saydamlık, arka plan resmi, klasör arka planı | Düşük | O | Yok (hafiflik hedefiyle çelişebilir) |

**Sonuç:**

- Gelişmiş adımı Files'la farkın büyük kısmını kapatır. Sıra önerisi: arama/süzme → çift panel → komut paleti → arşivin içinde gezinme → etiketler → Git.
- Planda olmayan ama ucuz kazanımlar (3, 4, 6) Taşınabilirlik'ten önce tek küçük bir "Gezinme cilası" adımında toplanabilir.
- 9, 10 ve 11 yeni bir "Görünüm 2" adımına uyar.
- Çeviri (16) ilk herkese açık sürümden önce yeniden değerlendirilmeli.

## 5. Kaynaklar

- Files ana sayfa: https://files.community/
- Belgeler: https://files.community/docs
- Ayarlar: https://files.community/docs/customize-settings/general · …/appearance · …/layout · …/files-and-folders · …/advanced · …/actions · …/tags
- Özellik sayfaları: https://files.community/docs/features/tabs · …/dual-pane · …/git · …/tags · …/omnibar · …/cloud-drives · …/ftp · …/folder-config · …/layout-picker · …/command-palette · …/integrations
- Kurulum ve SSS: https://files.community/docs/getting-started/install · https://files.community/docs/getting-started/faq
- Sürüm notları: https://files.community/blog · https://files.community/blog/posts/v4-0-0/ · https://files.community/blog/posts/v4-1-0/ · https://files.community/blog/posts/v4-2-0/ · https://files.community/blog/posts/v4-2-38/ · https://files.community/blog/posts/ten-things/
- GitHub: https://github.com/files-community/Files · https://github.com/files-community/Files/releases
- Kaynak kodda eylemler, önizleyiciler, Özellikler sayfaları: https://github.com/files-community/Files/tree/main/src/Files.App/Actions · https://github.com/files-community/Files/tree/main/src/Files.App/UserControls/FilePreviews · https://github.com/files-community/Files/tree/main/src/Files.App/Views/Properties · https://github.com/files-community/Files/tree/main/src/Files.App/Utils/Storage/StorageItems
- Toplu yeniden adlandırma: https://github.com/files-community/Files/issues/8191 · https://github.com/files-community/Files/pull/15537
- Performans hataları: https://github.com/files-community/Files/issues/18955 (45 sn açılış) · https://github.com/files-community/Files/issues/18984 (~86 MB boşta, arama >500 MB) · https://github.com/files-community/Files/issues/18982 (>1 GB) · https://github.com/files-community/Files/issues/18567 (>2 GB)
- Gezik: `README.md` (performans tablosu), `docs/superpowers/specs/*.md`, `crates/gezik-config/templates/settings.toml`, `crates/gezik-config/src/shortcuts.rs`
