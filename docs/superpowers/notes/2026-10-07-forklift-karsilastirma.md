# ForkLift 4 ile karşılaştırma: Gezik'te eksik olanlar

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** ForkLift 4.7.6 (2026-09-29; macOS 14.6+) ; Gezik `master` (29dcb45, `fix/macos` birleşmiş) + 5d (PDF, başka dalda; var sayıldı).
- **Kaynaklar:** ForkLift'in ana sayfası (30 özellik kartı), Store (fiyatlar), Support, Downloads, kılavuz (Quick Start Guide), BinaryNights blogundaki 4.0.2 → 4.7.6 sürüm notlarının hepsi (5 sayfa), "New features in the latest ForkLift versions" yazısı, beta 5 notları. Ayrıntılı bir çevrimiçi kılavuz yok: kılavuz kurulum, bağlantılar, temel işlemler, Sync, Multi-Rename, App Deleter ve varsayılan görüntüleyici ile sınırlı. Kalan ayrıntı sürüm notlarından alındı (sonda).
- **Gezik tarafı:** `docs/superpowers/specs/` altındaki beş spec (Kapsam dışı bölümleriyle), README, `settings.toml` şablonu, `gezik-config::shortcuts::Action` (28 eylem, `toggle-hidden` dahil), `context_menu.rs` (macOS/Linux menü öğeleri), `gezik-platform::icons` (macOS'ta sistem simgesi yok), `docs/superpowers/notes/macos-test-results.md` (macOS'ta bugün çalışanlar ve `fix/macos` düzeltmeleri).

**Durumlar:** **Var** · **Kısmen** (eksiği yazılı) · **Yok** · **Planlı** (yol haritası adımı yazılı).

**Yol haritası adları:** OneCommander notundakilerle aynı: *Etiketler* (spec'te ayrı adım 6: "macOS etiketleri tüm sistemlere"), *Taşınabilirlik* (dışa/içe aktarma, senkron klasör, GitHub'dan tema), *Bulut senkronu* (ayarların senkronu; dosya senkronu değil), *Gelişmiş* (arama, çift panel, arşivin içinde gezinme, Git, komut paleti), *Güncelleme* (gezinme spec'i: "ilk herkese açık sürümden önce").

---

## 1. Özellik tablosu

### 1.1 Çift panel, sekmeler, çalışma alanları, favoriler

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 1 | Çift panel; tek/çift panel arasında geçiş, ikinci panelin içeriği hatırlanır (4.3.3) | **Planlı** | Gelişmiş |
| 2 | Paneller arası kopyala/taşı (hedef: karşı panel), sekmeyi diğer panele taşıma (4.7.6) | **Planlı** | Gelişmiş (çift panelle) |
| 3 | Sync Browsing: iki aynı yapılı klasörde bir panelde gezince öteki izler | **Yok** | |
| 4 | Sekmeler: yeni/kapat/sırala, ⌘-çift tık yeni sekmede, klasör ve protokol simgeli sekmeler | **Var** | Protokol simgesi yok (uzak bağlantı yok) |
| 5 | Yeniden başlatınca sekmeleri geri yükleme, uzak bağlantılar dahil (4.4.1) | **Yok** | Gezinme spec'inde bilerek dışarıda |
| 6 | Workspaces: düzen + açık sekmeler + konumlar adla kaydedilir, yüklenir | **Yok** | |
| 7 | Favoriler: gruplar, özel simge ve renk (4.0.6, 4.4.2), Favorite Manager, grup silme | **Kısmen** | Tek PINNED listesi, sıralama, bırakarak işaretleme; grup, simge, renk yok |
| 8 | Favorilerin iCloud ile Mac'ler arasında senkronu | **Planlı** | `pinned` `settings.toml`'da; Taşınabilirlik (senkron klasör) / Bulut senkronu |
| 9 | Recent Folders kenar grubu (son 10), Go to Folder geçmişi (4.6.3) | **Yok** | |
| 10 | Devices grubu: sürücüler, sürükleyerek sıralama, çıkarma (eject), paylaşımı çıkarma | **Kısmen** | Sürücü listesi var; macOS'ta çıkarma ve sıralama yok |
| 11 | Go to Folder (⇧⌘G): yol yazma, alias/symlink çözme (4.6.4) | **Kısmen** | ⌘L ile yol yazılır; tamamlama ve geçmiş yok |
| 12 | Yol çubuğu: parçaya tıkla, kısalmış adlar üstüne gelince açılır (4.7.6) | **Var** | Breadcrumb; üstüne gelince açılma yok |
| 13 | Geri/ileri/üst klasör | **Var** | ⌘[ ⌘] Türkçe klavyede de çalışır (`fix/macos`) |
| 14 | Spring-loaded klasörler (sürüklerken üstünde bekleyince açılır; liste ve ikon görünümü) | **Kısmen** | Yalnız sekmede bekleyince |

### 1.2 Görünümler ve önizleme

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 15 | Liste görünümü: sütun ekle/çıkar (sağ tık), sürükleyerek sırala, çift tıkla sığdır, "Resize Columns to Fit" | **Kısmen** | Göster/gizle, genişlik var; sürükleyerek sıralama ve otomatik genişlik yok (görünüm spec'inde bilerek dışarıda) |
| 16 | Column View (Miller sütunları), sütunlarda yazı boyutu | **Yok** | |
| 17 | Icon View: boyut, öğe bilgisi (klasör öğe sayısı, video süresi, boyutlar), dosya boyutu (4.6.3, 4.7.2) | **Kısmen** | Izgara ve boyutlar var; alt bilgi satırı yok |
| 18 | Gruplama (tür, tarih…) | **Yok** | Görünüm spec'inde bilerek dışarıda |
| 19 | Meta veri sütunları: Comments, Duration, Dimensions (4.5), Date Last Opened (4.3.3), Date Added | **Yok** | Gezik'te Modified, Created, Type, Size |
| 20 | Klasöre özel görünüm, alt klasörlere uygulama, varsayılan görünüm; yazı tipi, kalın klasör adları, ⌘± yazı boyutu | **Kısmen** | Klasör başına görünüm + "Apply to all" var, yazı tipi temada; alt klasöre miras ve ⌘± yok |
| 21 | Gizli dosyaları göster/gizle (kısayol, araç çubuğu düğmesi) | **Var** | ⇧⌘. (macOS'ta varsayılan gizli) |
| 22 | Tüm klasör boyutlarını hesapla (File menüsü, Info penceresi) | **Yok** | |
| 23 | Önizleme paneli: resim, bilgi, metin | **Var** | Alt+P |
| 24 | Önizleme panelinde ses/video oynatma, PDF (metin seçimi), doc/docx/eml (uzakta da) | **Kısmen** | Yalnız resim, metnin başı, küçük resim; oynatma ve PDF sayfaları yok (görünüm spec'inde bilerek dışarıda) |
| 25 | Önizleme panelinde metin dosyasını yerinde düzenleme (alias'ın hedefi dahil) | **Yok** | |
| 26 | Quick Look (Boşluk), macOS'un QL eklentileriyle; arşiv içinde de | **Kısmen** | Gezik'in kendi hızlı bakış penceresi; QL eklentileri, video, PDF yok |
| 27 | Info penceresi: tam bayt, izinler (sekizli, sahip/grup) düzenleme, "gizli" kutusu, yorum düzenleme, bağlantı hedefi, "Open with" ile türün varsayılan uygulamasını değiştirme | **Yok** | |
| 28 | Finder'ın klasör renkleri (Tahoe) ve özel klasör simgeleri | **Yok** | macOS'ta sistem simgesi hiç yok (bkz. bölüm 3) |
| 29 | Temalar: hazır temalar (Polar Night…), kendi temanı oluşturma, Tahoe kenar çubuğu stili | **Var** | TOML, canlı; grafik tema düzenleyici yok |

### 1.3 Dosya işlemleri ve aktarım kuyruğu

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 30 | Kopyala/taşı/sürükle (⌥ ile kopya), seçili klasörün içine yapıştır | **Var** | Finder panosuyla iki yönlü |
| 31 | Çakışma: Replace, Replace Older, Keep Both, Skip, Stop, Merge | **Var** | Gezik: hepsi başlamadan tek listede, satır başına karar |
| 32 | Activities: canlı ilerleme, araç ipucunda boyut/hız/kalan süre | **Var** | Panel: %, hız, kalan süre, duraklat |
| 33 | Aktarım kuyruğu: sıra değiştirme, hata kuralları, indirme/yükleme bant genişliği sınırı | **Kısmen** | Disk başına kuyruk, duraklat/iptal; sıra değiştirme ve bant sınırı yok |
| 34 | Log görünümü: işlemlerin ve sonuçlarının geçmişi | **Yok** | Geri alma geçmişi var, görünür günlük yok |
| 35 | Geri al / yinele | **Var** | Gezik'te daha geniş (bölüm 2) |
| 36 | Çöpe at, onaylı kalıcı silme | **Var** | |
| 37 | Yeni klasör, yeni dosya | **Var** | |
| 38 | Çoğalt; alias / sembolik bağlantı oluşturma | **Kısmen** | ⌘D var; bağlantı oluşturma yok (dosya işlemleri spec'inde bilerek dışarıda) |
| 39 | İki dosyayı karşılaştırma: FileMerge, Kaleidoscope, Beyond Compare, Araxis; aynı paneldeki iki öğe (4.1.8) | **Yok** | `[[commands]]` ile tek dosyaya komut verilebilir, ikisini tek çağrıya veremez |
| 40 | Sağlama toplamı: MD5, SHA-1/256/384/512; çoklu seçim, CSV'ye aktarma (4.4.5, 4.6.2) | **Yok** | |
| 41 | Yazdırma (⌘P) | **Yok** | |
| 42 | Yolu kopyala, birkaç biçim (Terminal biçimi dahil) | **Kısmen** | Yalnız Windows Shell menüsünde; macOS'ta yok |
| 43 | Share menüsü (AirDrop, Mail, Messages…) | **Yok** | |
| 44 | Dropbox paylaşım bağlantısını kopyalama | **Yok** | |
| 45 | iCloud Drive: bulut durum simgeleri, indir, indirmeyi kaldır, iCloud'a boşalt (4.3.5, 4.5.1); Dropbox/Google Drive simgeleri | **Yok** | |
| 46 | Klasörün canlı yenilenmesi | **Var** | FSEvents; symlinkli yollar `fix/macos`'ta düzeldi |
| 47 | Başka uygulamalardan sürükleme (Photos gibi, dosya sözü / file promise) | **Kısmen** | Finder'dan bırakma var (`NSPasteboardTypeFileURL`); file promise denenmedi, büyük olasılıkla yok |

### 1.4 Uzak bağlantılar

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 48 | SFTP (OpenSSH; ProxyJump, Include, anahtar, çok adımlı doğrulama), FTP/FTPS | **Yok** | Yol haritasında yok |
| 49 | WebDAV | **Yok** | |
| 50 | Amazon S3 ve uyumlular (MinIO, Scaleway), Backblaze B2, Rackspace Cloud Files | **Yok** | |
| 51 | Google Drive (Team Drives), OneDrive, Dropbox (Teams), OAuth ile | **Yok** | *Bulut senkronu* adımı bu değil (o Gezik'in ayarlarını taşır) |
| 52 | SMB, AFP, NFS paylaşımlarına bağlanma; VNC | **Kısmen** | Finder'ın bağladığı paylaşım `/Volumes` altında sürücü olarak görünür; bağlanma (⌘K) yok |
| 53 | Bağlantı favorileri, parolalar Anahtar Zinciri'nde, aynı anda birçok sunucu, `$SOURCE_HOSTNAME` gibi yer tutucular | **Yok** | |
| 54 | Uzak dosyayı tercih edilen editörde açma, kaydedince otomatik yükleme | **Yok** | |

ForkLift 3'teki menü çubuğu uygulaması **ForkLiftMini** ve onun **Mount as Disk** özelliği (uzak bağlantıyı disk olarak bağlama) ForkLift 4'ün sitesinde, kılavuzunda ve 4.x sürüm notlarının hiçbirinde geçmiyor. ForkLift 4'te bunlar yok sayıldı; satır olarak sayılmadı.

### 1.5 Senkron ve klasör karşılaştırma

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 55 | Klasör karşılaştırma ve tek/iki yönlü senkron; yerel ve uzak; Added/Updated/Deleted süzgeç düğmeleri (4.7.5), hariç tutma, alt klasörler, gizli öğeler, saat farkı düzeltmesi | **Yok** | Çakışma listesi ve kopyalama motoru temel olabilir |
| 56 | Synclet: bir senkronu favori olarak kaydetme | **Yok** | |

### 1.6 Toplu yeniden adlandırma (Multi-Rename)

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 57 | Büyük/küçük harf (4 seçenek), regex ile değiştirme, konuma metin ekleme, numaralandırma, tarih (değişme/oluşturma/bugün), hazır ayarlar | **Var** | Gezik'te canlı önizleme, elle düzeltme, Türkçe harf kuralları |
| 58 | Meta veriyle adlandırma: fotoğraf, ses (sanatçı, albüm), belge; özel tarih biçimi (4.7.1, 4.7.2) | **Kısmen** | EXIF çekim tarihi + strftime var; ses ve belge alanları yok |
| 59 | Pencerede sürükleyerek elle sıralama (4.1.5) | **Var** | |

### 1.7 Arşivler

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 60 | Yerel ve uzak arşivlerde klasör gibi gezinme; içinde Quick Look, arama, süzme | **Planlı** | Gelişmiş (arşivin içinde gezinme) |
| 61 | Sıkıştırma (zip), birden çok arşivi tek seferde (4.1.6); açma | **Var** | Gezik çok daha geniş (bölüm 2) |

### 1.8 Arama

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 62 | Ada, uzantıya, türe, etikete, içeriğe göre arama; alt klasörler; uzak sunucularda da | **Planlı** | Gelişmiş |
| 63 | Yerelde Spotlight ile meta veri araması | **Planlı** | Gelişmiş (arama); Spotlight kullanımı henüz kararlaştırılmadı |
| 64 | Bulunulan klasörü yazarak süzme (filter) | **Kısmen** | Yalnız harfle atlama |
| 65 | Quick Select: ad, uzantı veya etiketle seçime ekle/çıkar (aksanlı harf duyarsız, 4.6.4) | **Yok** | |

### 1.9 Etiketler (Finder etiketleri)

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 66 | Etiket ekle/düzenle/sil, renk etiketlerine kısayollar (4.1.8), listede/sütunda/ikonda en çok 3 renk (4.2, 4.2.1), harici sürücüde etiket (4.6.2) | **Planlı** | Etiketler |
| 67 | Tags kenar grubu ve etikete göre süzme (4.3), etikete sürükleyerek atama, otomatik tamamlama, Ayarlar'da etiket yönetimi (4.6) | **Planlı** | Etiketler (spec'te "kenar çubuğu bölümü" var) |

### 1.10 Git

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 68 | Dosya başına Git durumu | **Planlı** | Gelişmiş |
| 69 | add, commit, push, pull | **Planlı** | Gelişmiş |

### 1.11 App Deleter

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 70 | Uygulamayı tercih/önbellek artıklarıyla birlikte kaldırma, silmeden önce gözden geçirme | **Yok** | macOS'a özgü; dosya yöneticisinin asıl işi değil |

### 1.12 Birlikte aç, editörler, araçlar

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 71 | "Open With" uygulama listesi (menü, Quick Open), türün varsayılan uygulamasını değiştirme | **Kısmen** | macOS'ta yalnız "Open" ve "Open with default app" |
| 72 | Tercih edilen editör ayarı | **Yok** | |
| 73 | Tools: komut satırı araçları; kısayol, sağ tık menüsü (4.1.7), araç çubuğu düğmesi ve özel simge (4.6), ortam değişkenleri (4.7.1) | **Kısmen** | `[[commands]]` sağ tık "Commands ▸" ve Convert katmanında; kısayol ve araç çubuğu düğmesi yok |
| 74 | Terminalde aç: Terminal, iTerm, Hyper, Kitty, Warp, Ghostty | **Yok** | |

### 1.13 Klavye ve Quick Open

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 75 | Quick Open (Esc): favoriler, aygıtlar, tüm menü komutları kısayollarıyla (4.3.3), seçili dosyayı bir uygulamayla açma, rename hazır ayarı uygulama | **Planlı** | Gelişmiş (komut paleti) |
| 76 | Tüm kısayollar değiştirilebilir | **Var** | `[shortcuts]` |
| 77 | Her işlem menü çubuğunda, klavyeyle | **Var** | File/Edit/View/Go/Window menüleri (`fix/macos`); değiştirilen kısayolu menü göstermez |

### 1.14 Özelleştirme ve diğer

| # | ForkLift özelliği | Gezik | Not |
|---|---|---|---|
| 78 | Özelleştirilebilir araç çubuğu | **Yok** | |
| 79 | Sidebar Editor (öğe ve grup gizleme), kenar simgelerini renklendirme | **Yok** | |
| 80 | Arayüz çevirileri (14 dil: İngilizce, Almanca, Macarca, Fransızca, İspanyolca, Portekizce + Brezilya, Lehçe, Ukraynaca, İtalyanca, Çekçe, Japonca, Çince + Geleneksel) | **Yok** | Türkçe ForkLift'te de yok; gezinme spec'inde bilerek dışarıda |
| 81 | Varsayılan dosya görüntüleyici: "Reveal in Finder" diyen uygulamalar ForkLift'i açar | **Yok** | |
| 82 | Birden çok pencere, tam ekran, Window menüsünden sekme taşıma | **Kısmen** | Tam ekran ve Window menüsü var; tek pencere |
| 83 | Otomatik güncelleme | **Planlı** | Güncelleme |
| 84 | Sağ tık menüsünde macOS Quick Actions ve Services | **Yok** | Gezinme spec'inde bilerek dışarıda ("Hizmetler" alt menüsü) |

### 1.15 Fiyat

ForkLift 4 tek seferlik lisans, belli süre güncellemeyle: 1 yıl güncelleme $19.95 (1 Mac), $29.95 (aile), $69.95 (küçük işletme, 5 Mac); 2 yıl $34.95 / $49.95 / $119.95. Süre bitince son kapsanan sürüm sonsuza dek kullanılır. Ücretsiz deneme var. Yalnız macOS 14.6+. Gezik PolyForm Noncommercial: kişisel kullanım ücretsiz, **ticari kullanım için yol yok**. ForkLift'in ana kitlesi (geliştirici, web, ajans) çoğunlukla ticari kullanıcıdır; bu, satır değil ama karşılaştırmanın en büyük farkı.

---

## 2. Gezik'te olup ForkLift'te olmayanlar

| Alan | Gezik | ForkLift 4 |
|---|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından; ayarlar OS'ler arası taşınır | Yalnız macOS 14.6+ |
| Hafiflik | Açılış 24-35 ms, boşta ~7 MB, 100 bin dosya ~17 MB (Windows ölçümü) | Yerel Cocoa; sürüm notlarında tekrar tekrar bellek sızıntısı düzeltmesi |
| Çakışmalar | Başlamadan tüm çakışmalar tek listede, satır başına karar, "identical" gizleme, ezilen dosya çöpe | İş başına seçenek (Replace, Replace Older…) |
| Geri alma | Kopyala, taşı, ad, yeni öğe, çöp, değiştirme, toplu ad, arşiv, dönüştürme, komut; tek ⌘Z | Undo/redo var; kapsamı belgelenmemiş |
| Kalıcı silme | Anında (gizli ada çevir, arka planda sil, çökmede açılışta bitir) | Onaylı silme |
| Arşivler | zip, 7z, rar, tar.gz/xz/bz2/zst, cab, iso, cpio/ar/deb açma (+ indirilen 7-Zip ile dmg, wim, lzh…); zip/7z/tar oluşturma, AES, dosya adı şifreleme, 7z parçalı; arşiv üstüne sürükleyip ekleme; zip bombası ve yol güvenliği | Arşivde gezinme var; oluşturmada sitede yalnız zip |
| Dönüştürme | Resim (JPEG/PNG/WebP/AVIF/HEIC, EXIF döndürme, konum silme), metin kodlaması ve satır sonu, ses/video (ffmpeg tek tıkla indirilir) | Yok |
| PDF (5d) | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'ten resim | Yok |
| Toplu adlandırma | Döngü güvenli (a↔b), uyarılı önizleme, şablon alanları (`{parent}`, `{size}`, `{taken}`), Türkçe i/İ, ı/I | Meta veri alanları daha zengin (ses, belge) |
| Kullanıcı komutları | Kabuksuz argüman dizisi, güvenli yer tutucular, paralel çalıştırma, yerinde değişen dosya için çöpe kopya ile geri alma | Tools: kabuk komutu, araç çubuğu düğmesi |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı yenileme, satır numaralı hata | Grafik Ayarlar penceresi |
| Araç indirme | 7-Zip, ffmpeg, pdfium tek tıkla, SHA-256 sabitli | Yok (dış araçlar elle) |
| Yol haritasında | Ayar senkronu (şifreli), GitHub'dan tema | Favoriler iCloud ile senkron (yalnız favoriler) |

---

## 3. macOS'ta Gezik'in Finder/ForkLift'e göre eksik kalan yerel davranışları

Kaynak: `macos-test-results.md` ve kod. `fix/macos` sonrası durum (menü çubuğu, ⌘M, ⌘[ Türkçe klavye, Finder'dan bırakma, dil algılama, "Computer"/"LOCATIONS", Launch Services tür adları, nokta dosyalarını gizleme düzeldi).

| # | Eksik | Finder / ForkLift | Not |
|---|---|---|---|
| a | **Sistem simgeleri yok:** macOS'ta `icons::icon` hep `None`; her öğe Gezik'in kendi simgesiyle | Uygulama simgeleri, belge simgeleri, klasör renkleri, özel klasör simgeleri | En görünür fark; `NSWorkspace iconForFile:` |
| b | **Küçük resimler kendi çözücüyle** (5 biçim); QuickLook küçük resmi yok (PDF, video, PSD, Office yok) | `QLThumbnailGenerator` her türde | |
| c | **Quick Look** Gezik'in kendi penceresi; QL eklentileri, video, PDF, Office yok | Sistem QL paneli | `QLPreviewPanel` |
| d | **Finder etiketleri** okunmuyor, gösterilmiyor | Renkli noktalar, Tags grubu | Etiketler adımı `com.apple.metadata:_kMDItemUserTags` ile Finder uyumlu olmalı |
| e | **Sağ tık menüsü** Gezik'in kendi menüsü: "Open With ▸" uygulama listesi, Share/AirDrop, Services, Quick Actions, "Show in Finder", "Get Info" yok | Hepsi var | |
| f | **Get Info:** izinler, sahip, yorum, "Open with" varsayılanı | Info penceresi | |
| g | **iCloud Drive:** durum simgesi, indir, indirmeyi kaldır yok; yalnız bulutta olan dosya açılınca ne olacağı denenmedi | Var | |
| h | **Spotlight araması** yok (arama da yok) | ⌘F, Spotlight | Arama adımı yerelde `NSMetadataQuery` kullanabilir |
| i | **Görünümler:** Column (⌘3) ve Gallery (⌘4) yok; ⌘± yazı boyutu yok | Var | |
| j | **Paket:** `.app`, imza, notarization yok; Slint'in eklediği uygulama menüsü öğeleri "About/Hide/Quit **gezik**" okunuyor | `.app`, imzalı | `.app` ile düzelir (test notu) |
| k | **Menü** değiştirilen kısayolu değil varsayılanı gösteriyor; ⌘, (Ayarlar) bir şey yapmıyor | Doğru kısayol, Ayarlar penceresi | ⌘, `settings.toml`'u açabilir |
| l | **Adlar:** kopya `b (2).txt` (Finder `b 2.txt`), yeni klasör "New folder" ("untitled folder"), arayüz İngilizce | Finder kalıbı, sistem dili | |
| m | **Sürücüler:** çıkarma (eject) yok, ağ paylaşımına bağlanma (⌘K) yok | Var | |
| n | **Varsayılan dosya görüntüleyici** olamaz (`NSFileViewer`) | ForkLift olabilir | |
| o | **Spring-loaded klasörler** yalnız sekmede; Finder her klasörde ve kenar çubuğunda | Var | |
| p | **Başka uygulamalardan dosya sözü** (Photos, Mail) ile sürükleme büyük olasılıkla alınmıyor | Var (ForkLift 4.5.1'de iyileşti) | Denenmeli |
| q | **Görünüş:** denetimler Slint'in çizdiği; Tahoe'nun Liquid Glass kenar çubuğu, sistem vurgu rengi yok; macOS pencere sekmeleri kapalı | Yerel AppKit | Bilerek (tek kod tabanı); vurgu rengini sistemden almak ucuz |

---

## 4. Sayım

| Durum | Sayı |
|---|---|
| Var | 18 |
| Kısmen | 19 |
| Yok | 35 |
| Planlı | 12 |
| **Toplam** | **84** |

Bu sayım yalnız numaralı satırlardır (1-84); bölüm 2 ve 3 sayılmadı. ForkLift 4'te olmayan ForkLiftMini / Mount as Disk satır değildir.

---

## 5. Öneri: değer / emek sırası

Değer: tipik bir kullanıcının (ForkLift'ten ya da Finder'dan gelen) günlük işine etkisi. Emek: Gezik'in bugünkü altyapısına göre tahmin (D düşük, O orta, Y yüksek). Yol haritası adları OneCommander notundaki önerilerle uyumlu tutuldu (*Arama*, *Düzenler*, *Günlük kolaylıklar*, *Önizleme 2*, *Sistem bütünleşmesi*).

| Sıra | Özellik (satır) | Durum | Değer | Emek | Nereye |
|---|---|---|---|---|---|
| 1 | Özyinelemeli arama + yazınca süzme; macOS'ta Spotlight (62-64) | Planlı/Kısmen | Çok yüksek | O | Gelişmiş'ten öne: **Arama** ayrı adım |
| 2 | Çift panel + karşı panele kopyala/taşı + Sync Browsing (1-3) | Planlı/Yok | Çok yüksek (ForkLift'in kimliği) | Y | Gelişmiş'ten **Düzenler** adımı; Sync Browsing aynı adımda |
| 3 | macOS sistem simgeleri + QuickLook küçük resimleri + sistem QL paneli (26, 28; bölüm 3 a-c) | Kısmen/Yok | Yüksek (macOS'ta ilk izlenim) | O | Yeni **Sistem bütünleşmesi** adımının macOS kısmı; Gelişmiş'ten önce |
| 4 | Finder etiketleri, Tags grubu, etikete göre süzme (66-67) | Planlı | Yüksek | O | Etiketler (olduğu gibi; Finder xattr'ıyla uyumlu) |
| 5 | Terminalde aç + yolu kopyala (macOS'ta da) (74, 42) | Yok/Kısmen | Yüksek (geliştirici) | D | **Günlük kolaylıklar** |
| 6 | Oturumu geri yükleme + Workspaces (5-6) | Yok | Yüksek | D-O | Günlük kolaylıklar (spec'teki "tek sekme" kararı ayar olur) |
| 7 | "Open With" listesi, Share/AirDrop, Services/Quick Actions, Get Info (27, 43, 71, 84) | Yok/Kısmen | Orta-yüksek | O | Sistem bütünleşmesi |
| 8 | Klasör karşılaştırma + yerel senkron, Synclet (55-56) | Yok | Orta-yüksek | O-Y | Yeni adım **Karşılaştır ve eşitle**, Düzenler'den sonra (çakışma listesi ve kopya motoru hazır) |
| 9 | Komut paleti / Quick Open (75) | Planlı | Orta-yüksek | O | Gelişmiş (Arama ile aynı süzme kodu) |
| 10 | Klasör boyutları (22) | Yok | Orta-yüksek | O | Arama adımına (aynı arka plan tarayıcı) |
| 11 | Uzak bağlantılar: önce SFTP, sonra WebDAV/S3; uzakta düzenleme, kuyrukta bant sınırı (48-54, 33) | Yok | Yüksek (ForkLift kullanıcısı için), orta (genel) | Y | Yeni büyük adım **Uzak bağlantılar**, Gelişmiş'ten sonra; `gezik-ops` görev modeli VFS katmanı ister |
| 12 | Git durumu + add/commit/push/pull (68-69) | Planlı | Orta (geliştirici) | O | Gelişmiş |
| 13 | Sağlama toplamı, iki dosyayı dış araçla karşılaştırma, yazdırma (39-41) | Yok | Orta | D | Günlük kolaylıklar (`[[commands]]`'a "iki seçili dosya" yer tutucusu da) |
| 14 | Quick Select, Recent Folders, Go to Folder tamamlama ve geçmişi (9, 11, 65) | Yok/Kısmen | Orta | D | Günlük kolaylıklar |
| 15 | Önizleme 2: ses/video, PDF sayfaları, metni yerinde düzenleme, meta veri sütunları (19, 24-25) | Kısmen/Yok | Orta | O-Y | **Önizleme 2** (5c/5d'nin ffmpeg ve pdfium'u) |
| 16 | Arşivin içinde gezinme (60) | Planlı | Orta | O | Gelişmiş (olduğu gibi) |
| 17 | Arayüz çevirileri, önce Türkçe (80) | Yok | Orta-yüksek (TR kullanıcı) | O | Yeni adım **Yerelleştirme** |
| 18 | Tools'a kısayol ve araç çubuğu düğmesi; özelleştirilebilir araç çubuğu (73, 78) | Kısmen/Yok | Orta | O | Taşınabilirlik'ten önce ayar biçimi kararlaştırılmalı |
| 19 | iCloud durum/indir/boşalt, eject, ⌘K ile paylaşıma bağlanma, varsayılan görüntüleyici (10, 45, 52, 81) | Yok/Kısmen | Orta | O | Sistem bütünleşmesi |
| 20 | Column View (16) | Yok | Orta (macOS kullanıcısının alışkanlığı) | Y | Düzenler (Miller sütunlarıyla tek iş; OneCommander notu sıra 11) |
| 21 | Favori grupları, simge/renk, Sidebar Editor (7, 79) | Kısmen/Yok | Düşük-orta | D-O | Günlük kolaylıklar (`pinned` biçimi Taşınabilirlik'ten önce kesinleşmeli) |
| 22 | App Deleter (70) | Yok | Düşük | O | Kapsam dışı bırakılmalı (macOS'a özgü, dosya yöneticisinin işi değil) |

**Özet öneri**

- macOS'ta Gezik'in en zayıf yeri özellik değil **yerellik**: simge, küçük resim, Quick Look, sağ tık menüsü (Open With, Share, Services) ve etiketler. Bunlar Finder kullanıcısının ilk dakikada fark ettiği şeyler; ForkLift'in hepsinde var. Bir **Sistem bütünleşmesi (macOS)** adımı Gelişmiş'ten önce gelmeli; Etiketler adımı da Finder etiketleriyle birebir uyumlu olmalı.
- ForkLift'in kimliği **çift panel + uzak bağlantılar + senkron**. Çift panel zaten planlı. **Uzak bağlantılar** ve **Karşılaştır ve eşitle** yol haritasında hiç yok. Uzak bağlantı en büyük iş (VFS katmanı, kimlik bilgileri, Anahtar Zinciri/Credential Manager/Secret Service); ForkLift ile rekabet edilecekse ayrı büyük adım, edilmeyecekse bilerek kapsam dışı yazılmalı. Yerel senkron daha ucuz: çakışma listesi ve kopyalama motoru hazır.
- *Gelişmiş*'ten **Arama** (+ klasör boyutu + süzme) ve **Düzenler** (çift panel + Sync Browsing + Column View) ayrılmalı; Git, komut paleti ve arşivin içinde gezinme kalır. Git ve komut paleti ForkLift'te de var, burada Gezik ayırt edici değil, eşitlenecek.
- **Günlük kolaylıklar** adımı OneCommander notuyla aynı: terminal, yol kopyalama, oturum/workspaces, Quick Select, son klasörler, sağlama toplamı, karşılaştırma aracı, yazdırma.
- Gezik'in ForkLift'e karşı açık üstünlükleri (arşiv biçimleri ve güvenliği, dönüştürme, PDF, geri alma kapsamı, çakışma listesi, üç işletim sistemi) korunmalı ve tanıtımda öne çıkarılmalı.
- Ticari kullanım: ForkLift'in kitlesi büyük ölçüde profesyonel; Gezik'in lisansı bu kullanıcılara bugün hiç yol vermiyor.

---

## Kaynaklar

ForkLift'in kendi sitesi:

- https://binarynights.com/ (ana sayfa: 30 özellik kartı, sürüm 4.7.6, sistem gereksinimi)
- https://binarynights.com/store (fiyatlar ve lisans)
- https://binarynights.com/manual (Quick Start Guide)
- https://binarynights.com/support (SSS)
- https://www.binarynights.com/downloads (sürüm listesi ve tarihleri)

Sürüm notları (BinaryNights blogu):

- https://blog.binarynights.com/category/release-notes/ (sayfa 1-5)
- https://blog.binarynights.com/2026/09/29/forklift-4-7-6-is-available-user-experience-improvements-and-macos-golden-gate-fixes/
- https://blog.binarynights.com/2026/09/01/forklift-4-7-5-is-available-improved-sync-and-safer-file-transfers/
- https://blog.binarynights.com/2026/08/18/forklift-4-7-4-is-available-fixes-for-file-transfers-and-tags/
- https://blog.binarynights.com/2026/07/28/forklift-4-7-3-is-available-smoother-navigation-and-selection-in-icon-view/
- https://blog.binarynights.com/2026/07/07/forklift-4-7-2-is-available-multi-rename-fixes-and-enhancements/
- https://blog.binarynights.com/2026/06/30/forklift-4-7-1-is-available-multi-rename-gets-metadata-support-for-smarter-file-naming/
- https://blog.binarynights.com/2026/06/16/forklift-4-6-4-is-available-brings-improvements-to-file-navigation/
- https://blog.binarynights.com/2026/06/02/forklift-4-6-3-is-available-item-info-in-icon-view-and-enhanced-go-to-folder-navigation/
- https://blog.binarynights.com/2026/05/12/forklift-4-6-2-is-available-stability-improvements-and-important-fixes-for-icon-previews-on-macos-sequoia/
- https://blog.binarynights.com/2026/04/28/forklift-4-6-1-is-available-tahoe-folder-colors-improved-memory-usage-and-restored-pdf-preview/
- https://blog.binarynights.com/2026/03/31/forklift-4-6-is-available-autocompletion-for-tags-and-one-click-access-to-your-custom-tools/
- https://blog.binarynights.com/2026/03/10/forklift-4-5-1-is-available-performance-improvements-and-fixes/
- https://blog.binarynights.com/2026/02/10/forklift-4-5-is-available-new-columns-and-editable-comments/
- https://blog.binarynights.com/2026/01/13/forklift-4-4-5-is-available-now-with-file-checksum-calculation/
- https://blog.binarynights.com/2025/11/12/forklift-4-4-4-is-available/
- https://blog.binarynights.com/2025/10/22/forklift-4-4-3-is-available/
- https://blog.binarynights.com/2025/09/30/forklift-4-4-2-is-available/
- https://blog.binarynights.com/2025/09/09/forklift-4-4-1-is-available/
- https://blog.binarynights.com/2025/08/26/forklift-4-4-is-available/
- https://blog.binarynights.com/2025/07/28/forklift-4-3-5-is-available/
- https://blog.binarynights.com/2025/05/27/forklift-4-3-3-is-available/
- https://blog.binarynights.com/2025/04/29/forklift-4-3-1-is-available/
- https://blog.binarynights.com/2025/04/01/forklift-4-3-is-available/
- https://blog.binarynights.com/2025/03/11/forklift-4-2-8-is-available/
- https://blog.binarynights.com/2025/02/26/forklift-4-2-6-is-available/
- https://blog.binarynights.com/2025/01/23/forklift-4-2-4-is-available/
- https://blog.binarynights.com/2025/01/21/forklift-4-2-3-is-available-explore-new-sidebar-features-and-enhanced-functionality/
- https://blog.binarynights.com/2025/01/07/forklift-4-2-1-is-available-hide-files-easily-with-one-click/
- https://blog.binarynights.com/2024/11/26/forklift-4-2-is-available-display-up-to-three-color-tags-in-list-view/
- https://blog.binarynights.com/2024/10/22/forklift-4-1-8-seamless-dropbox-connection-restored-with-support-for-dropbox-teams/
- https://blog.binarynights.com/2024/09/17/forklift-4-1-7-is-available/
- https://blog.binarynights.com/2024/08/13/forklift-4-1-6-is-available/
- https://blog.binarynights.com/2024/06/19/forklift-4-1-5-is-available/
- https://blog.binarynights.com/2024/06/04/forklift-4-1-3-is-available/
- https://blog.binarynights.com/2024/03/05/forklift-4-1-is-available/
- https://blog.binarynights.com/2024/03/12/new-features-in-the-latest-forklift-versions/ (klasör boyutları, yazdırma, Open With, Sidebar Editor)
- https://blog.binarynights.com/2024/01/30/forklift-4-0-7-is-available/
- https://blog.binarynights.com/2024/01/16/forklift-4-0-6-is-available/
- https://blog.binarynights.com/2023/06/27/forklift-4-beta-5-is-available/

Destekleyici:

- https://mjtsai.com/blog/2023/09/05/forklift-4/ (ForkLift 4'ün çıkışı: bulut protokolleri, iCloud favori senkronu, lisans modeli)
- https://alternativeto.net/news/2023/9/forklift-4-has-been-released-with-optimized-performance-and-enhanced-features
