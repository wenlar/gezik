# OneCommander ile karşılaştırma: Gezik'te eksik olanlar

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** OneCommander 3.108 (2026-03-06, kararlı) ve V4 beta 0.0.72; Gezik `master` (29dcb45) + 5d (PDF, başka dalda; var sayıldı).
- **Kaynaklar:** OneCommander'ın ana sayfası, About, Pricing, Pro features, V4 beta sayfası, çevrimiçi kılavuzun 63 sayfasının tamamı (help2), `releasenotes.txt` (3.7 → 3.108). Site zayıf kaldığında birkaç inceleme (sonda).
- **Gezik tarafı:** `docs/superpowers/specs/` altındaki beş spec (Kapsam dışı bölümleriyle), README, `settings.toml` şablonu, `gezik-config::shortcuts::Action` (27 eylem), kodda kısa kontroller (klasör izleme `folder_watch.rs` var; klasörler hep üstte, `sort.rs`).

**Durumlar:** **Var** · **Kısmen** (eksiği yazılı) · **Yok** · **Planlı** (yol haritası adımı yazılı).

**Yol haritası adları:** *Etiketler* (5. spec'te ayrı adım 6; kullanıcının özetinde Gelişmiş'in içinde), *Taşınabilirlik* (dışa/içe aktarma, senkron klasör, GitHub'dan tema), *Bulut senkronu* (ayarların senkronu), *Gelişmiş* (arama, çift panel, arşivin içinde gezinme, Git, komut paleti), *Güncelleme* (gezinme spec'i: "ilk herkese açık sürümden önce").

---

## 1. Özellik tablosu

### 1.1 Gezinme (Miller sütunları, sekmeler, çift panel)

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 1 | Miller sütunları: klasör ağacı sütun sütun; soldakiler daralır, üzerine gelince genişler; `\`` ile breadcrumb ↔ sütun | **Yok** | Gezik'te yalnız breadcrumb |
| 2 | Çift panel ("iki tarayıcı"), ayırıcıyla tek panele daraltma | **Planlı** | Gelişmiş |
| 3 | Paneller arası kopyala/taşı (Alt+C / Alt+M), odak değiştir (Alt+F), sekmeyi diğer panele taşı | **Planlı** | Gelişmiş (çift panelle) |
| 4 | Üç düzen: Standard, Columns, Classic (tek panel) | **Kısmen** | Yalnız Classic benzeri tek düzen |
| 5 | Sekmeler: yeni/kapat/sonraki/önceki, orta tıkla yeni sekmede aç, sürükleyerek sırala, çoğalt, diğerlerini kapat, sekmeye bırakma | **Var** | |
| 6 | Kapatılan sekmeyi geri aç (Ctrl+Shift+T) | **Yok** | |
| 7 | Sekme takma adı (alias) | **Yok** | |
| 8 | Sekmeyi dışarı sürükleyip yeni pencere; bayat sekmeleri kapat; tekerlekle sekme değiştirme | **Yok** | |
| 9 | Oturum geri yükleme; adlandırılmış, kaydedilen çoklu pencereler (V4: tam oturum, sanal masaüstü) | **Yok** | Gezinme spec'inde bilerek dışarıda (tek sekme + `start-folder`) |
| 10 | Geri/ileri (fare yan tuşları), üst klasör, sekme başına geçmiş | **Var** | |
| 11 | Yol düzenleme: otomatik tamamlama, favori/sekme içinde arama, panodaki yolu önerme, "Yapıştır ve aç" | **Kısmen** | Ctrl+L ile yol yazılır; tamamlama ve öneri yok |
| 12 | Adres çubuğuna `cmd`, `ps`, `wsl`, `wt`, `explorer` yazınca orada açma | **Yok** | |
| 13 | Favoriler: sınırsız grup, takma ad, grup sırası, [1][2] ile panel seçme, üzerine bırakınca kopyala/taşı | **Kısmen** | Tek PINNED listesi, sıralama ve bırakma var; grup ve takma ad yok |
| 14 | Geçmiş (History) kenar bölümü ve "Recent destinations" | **Yok** | |
| 15 | Sürücüler: boş alan, çıkar (Eject), uyuyan disk simgesi, BitLocker kilit açma | **Kısmen** | Liste ve "This PC" var; çıkarma, uyku, BitLocker yok |
| 16 | Ağ: ağ sürücüsü eşleme, ağ taraması (IP aralığı), Networks grubu | **Yok** | |
| 17 | Klasörü sürücü harfi olarak bağlama (subst) | **Yok** | Windows'a özgü |
| 18 | Telefon (Phone Link klasörü), WSL listeleme | **Yok** | Windows'a özgü; düşük öncelik |
| 19 | Uzun yol (> 260 karakter) | **Var** | Motor `\\?\` kullanır |
| 20 | Klavyeyle gezinme, harfle atlama | **Var** | |

### 1.2 Görünümler ve önizleme

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 21 | Ayrıntı listesi, sütuna göre sıralama, klasör başına sıralama/görünüm hatırlama | **Var** | `views.toml`, en çok 500 klasör |
| 22 | Küçük resim ızgarası, Ctrl+tekerlek boyut | **Var** | |
| 23 | Thumbs+Details ve Adaptive görünüm (ikinci satırda meta veri, metin alıntısı) | **Yok** | |
| 24 | Dinamik sütunlar (genişliğe göre gizlenir), File View Designer, görünümün alt klasörlere mirası | **Kısmen** | Sütun göster/gizle/genişlik var; tasarlayıcı ve miras yok |
| 25 | "Klasörler üstte" anahtarı, "klasörleri hep ada göre sırala" | **Kısmen** | Klasörler hep üstte, anahtar yok |
| 26 | Meta veriye göre sıralama (çözünürlük, süre, çekim tarihi), etikete göre sıralama | **Yok** | |
| 27 | Renkli "dosya yaşı" sütunu, Owner ve öznitelik sütunları, uzun adı aç (F7) | **Yok** | |
| 28 | Klasör boyutları (hesaplanıp önbelleğe alınır, V4'te ayrı veritabanı, boyuta göre sıralama) | **Yok** | |
| 29 | Gizli / sistem / geçici dosya görünürlüğü ayrı ayrı | **Kısmen** | Yalnız nokta ile başlayan adlar için anahtar |
| 30 | Boyut biçimi (bayt, MB/MiB), tarih biçimleri | **Yok** | |
| 31 | Önizleme paneli (resim, metin, klasör, bilgiler) | **Var** | Alt+P |
| 32 | Ayrı önizleme penceresi (Boşluk), oklarla sonraki dosya | **Var** | Hızlı bakış |
| 33 | Resim önizlemede imlece doğru yakınlaştırma, kaydırma | **Yok** | |
| 34 | Video/ses oynatma (atlama, kare kare, J/K/L, sessize alma; V4'te ffmpeg ile 4K/8K) | **Yok** | Görünüm spec'inde bilerek dışarıda |
| 35 | PDF önizleme: sayfa çevirme, sığdırma, form alanları | **Kısmen** | Yalnız sistem küçük resmi |
| 36 | Office (Windows önizleme işleyicisi) ve e-posta (.msg/.eml) önizleme | **Yok** | Görünüm spec'inde bilerek dışarıda |
| 37 | Arşiv önizleme (ilk 200 öğenin listesi) | **Planlı** | Gelişmiş (arşivin içinde gezinme) |
| 38 | Meta veri paneli: EXIF, MP3/FLAC, PDF sayfa sayısı, öznitelikler; GPS → harita düğmesi | **Kısmen** | Yalnız resim piksel boyutu |
| 39 | QuickLook / Seer dış önizleyici seçimi | **Yok** | Windows'a özgü |
| 40 | Kalıcı küçük resim veritabanı, "Regenerate thumbnail" | **Kısmen** | Yalnız bellekte önbellek |
| 41 | Geniş küçük resim desteği (PSD, SVG, video, .ico'nun en büyüğü) | **Kısmen** | Windows'ta sistem küçük resmi; macOS/Linux'ta 5 biçim |
| 42 | Klasör arka plan resmi (`cover.jpg`…), özel klasör küçük resmi ve 4 stil | **Yok** | |

### 1.3 Dosya işlemleri

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 43 | Kopyala/kes/yapıştır, sistem panosuyla iki yönlü | **Var** | Üç sistemde |
| 44 | Sürükle-bırak: sekmelere, favorilere, breadcrumb'a, dışarı/içeri; kenarda değişken hızlı kaydırma | **Var** | |
| 45 | Alt+sürükle menüsü: kısayol, symlink, hardlink, sıkıştır, aç, TeraCopy | **Kısmen** | Sağ tuşla Copy/Move/Cancel; diğerleri yok |
| 46 | Kendi kopyalama motoru (Taskmaster): kuyruk, ETA, duraklat, çakışma kararı | **Var** | |
| 47 | Geri alma (Taskmaster, V4'te varsayılan) | **Var** | Gezik'te daha geniş (bkz. bölüm 2) |
| 48 | Çöpe at, kalıcı sil | **Var** | |
| 49 | Yeni dosya şablonları (Ctrl+N, kendi şablon dosyaların) | **Kısmen** | Yalnız boş `New file.txt` |
| 50 | Yeni klasör: "oluştur ve aç", "seçilileri içine taşı" (Shift+Enter) | **Yok** | |
| 51 | Çoğalt (Ctrl+D) | **Var** | Windows'ta menüden |
| 52 | Kısayol (.lnk), sembolik ve sert bağlantı oluşturma, "kısayol olarak yapıştır" (Ctrl+Shift+V) | **Yok** | Dosya işlemleri spec'inde bilerek dışarıda |
| 53 | Panodaki metni/resmi dosya olarak yapıştır; URL'den .url/.txt/indirme; yt-dlp ile video indirme | **Yok** | Panodan dosya, dosya işlemleri spec'inde bilerek dışarıda |
| 54 | Outlook'tan e-posta bırakma (ad biçimi ayarlı) | **Yok** | Windows'a özgü |
| 55 | Yolu kopyala (Ctrl+Shift+C) ve biçimleri (`\`, `/`, `\\`, tırnaklı, `\\?\`, 8.3) | **Kısmen** | Yalnız Windows Shell menüsündeki "Yol olarak kopyala" |
| 56 | Birlikte aç (Ctrl+Enter), yönetici olarak çalıştır, parametreyle çalıştır | **Kısmen** | Windows'ta Shell menüsünden; parametreyle çalıştırma yok |
| 57 | Geri Dönüşüm Kutusu kenar öğesi ve "Geri yükle" menüsü | **Yok** | |
| 58 | Canlı klasör izleme; WebDAV/WSL için yoklama | **Kısmen** | İzleme var; yoklama yok |
| 59 | Yerel sağ tık menüsü ve menü öğelerini gizleme | **Kısmen** | Yerel menü var; gizleme yok |

### 1.4 Arama

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 60 | Özyinelemeli arama (F3/Ctrl+F), joker karakter, sonucu ana pencerede bulma, sonuçlarda Boşluk ile önizleme | **Planlı** | Gelişmiş |
| 61 | Yazınca süzme (1 harf: başlayan, 2+: içeren, `.pdf` ile tür) | **Kısmen** | Yalnız harfle atlama |
| 62 | Arama penceresinde "Tag" sekmesi (etikete göre listeleme) | **Planlı** | Etiketler + Gelişmiş |

### 1.5 Etiketler, renkler, notlar

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 63 | Renk etiketleri (Ctrl+Alt+T, `*` ile hızlı, sayıyla renk) | **Planlı** | Etiketler |
| 64 | Klasör içi not / yapılacak (`.2do`, Alt+N, işaretleyince `.2dx`) | **Yok** | |

### 1.6 Tema ve özelleştirme

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 65 | Açık/koyu tema, sistemi izleme | **Var** | `theme = "auto"` |
| 66 | Düzenlenebilir temalar (XAML), tema yeniden yükleme | **Var** | TOML, canlı; düzen değiştirilemez (bilerek) |
| 67 | Vurgu rengi, Windows'un vurgu rengini izleme | **Kısmen** | Temada `accent`; sistemden alma yok |
| 68 | Mica/Acrylic, saydamlık, arka plan resmi | **Yok** | |
| 69 | Yazı tipi seçimi (Pro) | **Var** | Temada `font-family`, `font-size` |
| 70 | Simge paketleri, klasöre özel simge | **Kısmen** | Sistem ya da Gezik simgeleri; paket yok |
| 71 | Dolgu, kaydırma çubuğu stilleri, animasyon süreleri, odak görünümü | **Kısmen** | `density` compact/comfortable |
| 72 | Kısayol düzenleyici | **Var** | `[shortcuts]` |
| 73 | Özelleştirilebilir araç çubukları, değiştirilebilir menü simgeleri (V4) | **Yok** | |
| 74 | Grafik ayar penceresi (V4'te aranabilir, açıkken canlı) | **Yok** | Ayarlar spec'inde ayrı alt proje diye dışarıda |
| 75 | Arayüz çevirileri (~20 dil, Türkçe dahil) | **Yok** | Gezinme spec'inde bilerek dışarıda |
| 76 | Kenar çubuğunu gizle (Alt+S) / konum | **Var** | `layout.sidebar` |

### 1.7 Arşivler

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 77 | zip'e sıkıştırma (Alt+sürükle, sağ tık) | **Var** | Gezik: zip/7z/tar, AES, parçalı |
| 78 | Arşiv açma (7z kütüphanesi; 3.103'te güvenlik nedeniyle kaldırıldı, V4 beta'da geri) | **Var** | Gezik çok daha geniş (bölüm 2) |

### 1.8 Git

OneCommander'da Git özelliği **yok** (site, kılavuz ve sürüm notlarının hiçbirinde geçmiyor). Gezik'te Git **Planlı** (Gelişmiş); bu, Gezik'in öne geçebileceği bir alan.

### 1.9 Bulut

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 79 | Bulut durum simgeleri (OneDrive, SharePoint, Google Drive; V4'te WebDAV/Cryptomator) | **Yok** | Bulut senkronu adımı bu değil (o ayarları taşır) |
| 80 | Bulut kenar grubu, yalnız bulutta olan dosyada küçük resim/indir seçeneği, Google Drive takılınca yeniden başlatma önerisi | **Yok** | |

### 1.10 Klavye, komut paleti, sistemle bütünleşme

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 81 | Geniş kısayol seti (seçim, sekmeler, önizleme, dosya işlemleri) | **Var** | 27 eylem + sabit liste tuşları |
| 82 | Genel kısayol (Win+Alt+E), alt yarıya yuvalanan pencere (Ctrl+\`), tepside arka planda çalışma, Windows'la başlama | **Yok** | |
| 83 | Terminali burada aç (F10, Ctrl+F10 yönetici; kabuk seçimi; PowerShell `oc` takma adı) | **Yok** | |
| 84 | Varsayılan dosya yöneticisi olma / Win+E, Explorer'a "OneCommander'da aç" | **Yok** | |
| 85 | Dialog Connector: Aç/Kaydet pencerelerini OC'deki klasöre götürür | **Yok** | |
| 86 | Komut satırı: `-p`, `-p2`, `-newtab`, `-openwin`, `-nowindow`, `/select`, `.`; V4'te panel/sekme hedefleme ve ayar değiştirme | **Kısmen** | Yalnız ilk argüman: açılacak klasör |
| 87 | Yönetici olarak yeniden başlat (Ctrl+Shift+F12), farklı kullanıcı olarak çalıştır (Pro) | **Yok** | |

Komut paleti OneCommander'da da **yok**; Gezik'te Planlı (Gelişmiş).

### 1.11 Toplu yeniden adlandırma ve dönüştürme (File Automator)

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 88 | RegEx ile yeniden adlandırma, Simple kurallar, numaralandırma, profiller, sıraya göre numara | **Var** | Gezik'te canlı önizleme, elle düzeltme, kayıtlı setler |
| 89 | Çoklu seçimde F2 → toplu adlandırma | **Var** | |
| 90 | MP3 adlarını etiketten düzeltme, "köşeli parantezleri sil" profili | **Kısmen** | Parantez regex'le yapılır; ID3 alanı yok |
| 91 | Toplu resim dönüştürme ve boyutlandırma | **Var** | |
| 92 | Ses çıkarma | **Var** | ffmpeg ile MP3/M4A |

### 1.12 Betikler ve kullanıcı komutları

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 93 | Scripts menüsü (PowerShell/bat/py, klasörlü menü), `$SelectedFiles`, `$SelectedFilesInactiveBrowser`, File Automator'da özel betik (Pro), betik dosyasının üstüne dosya bırakınca çalıştırma | **Kısmen** | `[[commands]]` var (kabuksuz, güvenli, geri alınabilir); tüm seçimi tek çağrıda verme, diğer panel, betiğe bırakma yok |

### 1.13 Performans

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 94 | Hız: DirectX çizim, simge/küçük resim/meta veri önbellekleri | **Var** | Gezik ölçülerde önde: açılış 24-35 ms, boşta ~7 MB (OC .NET; soğuk açılış ~3 s, bu yüzden arka planda bekler) |
| 95 | Sekmeler bellek tutmaz | **Var** | 20 sekme +0,1 MB |

### 1.14 Diğer

| # | OneCommander özelliği | Gezik | Not |
|---|---|---|---|
| 96 | Otomatik güncelleme, "Version Ahead" kanalı | **Planlı** | Güncelleme (ilk sürümden önce) |
| 97 | Kurulum çeşitleri: MSI, taşınabilir zip, Store, winget, Scoop; ARM64 (V4) | **Kısmen** | `GEZIK_CONFIG_DIR` ile taşınabilir kullanım; paketleme henüz yok |
| 98 | Bildirim paneli (son bildirimlerin listesi) | **Kısmen** | Durum çubuğunda ilk uyarı + sayı |

### 1.15 Ücretsiz ve Pro

OneCommander ev kullanımında ücretsiz ve reklamsız; Pro ömür boyu $30 (kullanıcı başına, 10 cihaz), şirketlerde en az 3 lisans. Ticari kullanım her durumda Pro ister. Gezik PolyForm Noncommercial: kişisel kullanım serbest, **ticari kullanım için hiç yol yok**. Bu bir özellik değil ama karşılaştırmada en büyük ticari fark.

Pro'nun kilitlediği özellikler ve Gezik'teki karşılıkları (tabloda sayılmadı, yukarıdaki satırlara bağlı):

| Pro özelliği | Gezik |
|---|---|
| Yazı tipi seçimi; varsayılan dosya görünümü | Var (ücretsiz) |
| Özel betikler, 8'den çok betik | Var/Kısmen (sınırsız `[[commands]]`) |
| Boyut biçimi, klasör küçük resim stili, metin alıntısı ayarları | Yok |
| Terminali burada aç; `oc` takma adı | Yok |
| Belirli uzantılarda küçük resmi kapatma | Kısmen (yalnız genel `thumbnails`) |
| Gizlenecek ad/önek/uzantı listesi | Kısmen (nokta ile başlayanlar) |
| Shell menü öğelerini gizleme; Outlook e-posta ad biçimi | Yok |
| Symlink / hardlink oluşturma | Yok |
| Video indirme komutunu düzenleme; önizleme dosya türü listeleri; harita sitesi | Yok |

---

## 2. Gezik'te olup OneCommander'da olmayanlar

| Alan | Gezik | OneCommander |
|---|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından | Yalnız Windows; "Mac/Linux planı yok" (WPF) |
| Hafiflik | Açılış 24-35 ms, boşta ~7 MB, 100 bin dosya ~17 MB | .NET; arka planda bekleyerek hızlı açılır |
| Geri alma | Kopyala, taşı, yeniden adlandır, yeni öğe, çöp, değiştirme, toplu işler, arşiv, dönüştürme; tek Ctrl+Z | Taskmaster ile kopyala/taşı |
| Çakışmalar | Başlamadan tüm çakışmalar tek listede, satır başına karar, ezilen dosya çöpe | Taskmaster'da işe toplu ön ayar |
| Kalıcı silme | Anında (gizli ada çevir, arka planda sil, çökmede açılışta tamamla) | Kılavuzda Shift+Del "not available yet" |
| Arşivler | zip, 7z, rar, tar.gz/xz/bz2/zst, cab, iso, cpio/ar/deb açma (+ indirilen 7-Zip ile nadir biçimler); zip/7z/tar oluşturma, AES, dosya adı şifreleme, 7z parçalı; var olan arşive ekleme ve üstüne sürükleme; zip bombası ve yol güvenliği | 3.103'te açma kaldırıldı (V4 beta'da geri); yalnız zip oluşturma |
| Dönüştürme | Resim (JPEG/PNG/WebP/AVIF/HEIC, EXIF döndürme, konum silme), metin kodlaması ve satır sonu, ses/video (ffmpeg tek tıkla indirilir) | File Automator'da resim ve ses |
| PDF (5d) | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'ten resim | Yok (kılavuz Acrobat'ın "Combine"ını önerir) |
| Toplu adlandırma | Canlı önizleme, elle düzeltme, sürükleyerek sıralama, döngü güvenli (a↔b), EXIF tarih şablonu, Türkçe büyük/küçük harf | Regex ve Simple kurallar; "zor" diye kabul ediliyor |
| Kullanıcı komutları | Kabuksuz argüman dizisi, güvenli yer tutucular, paralel çalışma, geri alma için çöpe kopya | PowerShell betikleri |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı yenileme, satır numaralı hata, OS'ler arası taşınabilir | JSON elle düzenlenmemeli; buluta konursa çöker, senkron yok |
| Araç indirme | 7-Zip, ffmpeg, pdfium tek tıkla, SHA-256 sabitli | yt-dlp elle kurulur |
| Yol haritasında | Ayar senkronu (şifreli), GitHub'dan tema, Git, komut paleti, arşivin içinde gezinme | Hiçbiri yok (arşiv içinde gezinmeyi bilerek desteklemiyor) |

---

## 3. Sayım

| Durum | Sayı |
|---|---|
| Var | 28 |
| Kısmen | 26 |
| Yok | 37 |
| Planlı | 7 |
| **Toplam** | **98** |

Bu sayım yalnız numaralı satırlardır (1-98); Pro tablosu ve bölüm 2 sayılmadı. Bulut, Git ve komut paletinde OneCommander'da olmayanlar satır değildir.

---

## 4. Öneri: değer / emek sırası

Değer: tipik bir kullanıcının günlük işine etkisi. Emek: Gezik'in bugünkü altyapısına göre tahmin (D düşük, O orta, Y yüksek).

| Sıra | Özellik (satır) | Durum | Değer | Emek | Nereye |
|---|---|---|---|---|---|
| 1 | Özyinelemeli arama (60) | Planlı | Çok yüksek | O | Gelişmiş'ten öne alınmalı: **Arama** ayrı adım |
| 2 | Yazınca süzme (61) | Kısmen | Yüksek | D | Arama adımına |
| 3 | Klasör boyutları (28) | Yok | Yüksek | O | Arama adımına (aynı arka plan tarayıcı ve önbellek) |
| 4 | Çift panel + paneller arası kopyala/taşı (2, 3) | Planlı | Yüksek | Y | Gelişmiş; Miller sütunlarıyla birlikte **Düzenler** adımı |
| 5 | Oturumu geri yükleme + kapatılan sekmeyi geri aç (6, 9) | Yok | Yüksek | D | Yeni küçük adım: **Günlük kolaylıklar** (spec'teki "tek sekme" kararı ayarla seçenek olur) |
| 6 | Renk etiketleri (63) | Planlı | Orta-yüksek | O | Etiketler (olduğu gibi) |
| 7 | Terminali burada aç + yolu kopyala biçimleri (83, 55) | Yok/Kısmen | Orta-yüksek | D | Günlük kolaylıklar |
| 8 | Zengin önizleme: video/ses, çok sayfalı PDF, resim yakınlaştırma, meta veri (33-35, 38) | Yok/Kısmen | Orta-yüksek | O-Y | Yeni adım **Önizleme 2**; 5c/5d'nin indirilen ffmpeg ve pdfium'unu kullanır |
| 9 | Grafik ayar penceresi (74) | Yok | Yüksek (teknik olmayan kullanıcı için) | Y | Taşınabilirlik'e (dışa/içe aktarma da arayüz ister) |
| 10 | Arayüz çevirileri, önce Türkçe (75) | Yok | Yüksek (TR kullanıcı) | O | Yeni adım **Yerelleştirme** |
| 11 | Miller sütunları (1, 4) | Yok | Orta (OC'nin imzası) | Y | Düzenler adımı |
| 12 | Yeni dosya şablonları + yeni klasöre taşı (49, 50) | Kısmen/Yok | Orta | D | Günlük kolaylıklar |
| 13 | Favori grupları, takma ad, Geçmiş bölümü (13, 14, 7) | Kısmen/Yok | Orta | D-O | Günlük kolaylıklar (`pinned` biçimi Taşınabilirlik'ten önce kararlaştırılmalı) |
| 14 | Panodaki resmi/metni dosya yapma (53) | Yok | Orta | D | Günlük kolaylıklar |
| 15 | Bulut durum simgeleri (79, 80) | Yok | Orta | O | Yeni adım **Sistem bütünleşmesi** |
| 16 | Genel kısayol, tepside bekleme, varsayılan dosya yöneticisi, Aç/Kaydet bağlayıcısı (82, 84, 85) | Yok | Orta | O-Y | Sistem bütünleşmesi |
| 17 | Kısayol/symlink/hardlink oluşturma (52, 45) | Yok | Düşük-orta | D | Günlük kolaylıklar |
| 18 | Thumbs+Details / Adaptive görünüm, meta veri sıralaması (23, 26) | Yok | Orta | O | Önizleme 2 (aynı meta veri okuyucu) |
| 19 | Sürücü çıkarma, ağ sürücüsü eşleme (15, 16) | Kısmen/Yok | Düşük-orta | O | Sistem bütünleşmesi |
| 20 | Klasör notları / yapılacaklar (64) | Yok | Düşük | D | Etiketler (aynı "öğeye bilgi ekleme" alanı) |

**Özet öneri**

- Var olan adımlara eklenecekler: arama + süzme + klasör boyutu → *Gelişmiş* adımından **Arama** olarak öne çekilip birleştirilmeli; grafik ayar penceresi → *Taşınabilirlik*; notlar → *Etiketler*; Miller sütunları → *Gelişmiş*'teki çift panelle tek **Düzenler** işi.
- Yeni adım gerekenler: **Günlük kolaylıklar** (küçük, hızlı kazanımlar: oturum, kapatılan sekme, terminal, yol kopyalama, şablonlar, favori grupları, panodan dosya, bağlantılar), **Önizleme 2** (ffmpeg/pdfium hazır), **Yerelleştirme**, **Sistem bütünleşmesi** (bulut durumu, genel kısayol, tepsi, varsayılan yönetici, sürücü/ağ).
- *Gelişmiş* adımı bugün beş büyük işi taşıyor; Arama ve Düzenler çıkarılınca Git, komut paleti ve arşivin içinde gezinme kalır. Git ve komut paleti OneCommander'da hiç yok: Gezik için ayırt edici.
- Ticari kullanım: OneCommander'ın gelir modeli ($30 Pro) Gezik'te yok; lisans kararı özelliklerden bağımsız ama karşılaştıran kullanıcının ilk göreceği fark.

---

## Kaynaklar

OneCommander'ın kendi sitesi:

- https://onecommander.com/ (ana sayfa)
- https://onecommander.com/about
- https://onecommander.com/pricing
- https://onecommander.com/pro-features
- https://onecommander.com/beta (V4 beta özellikleri)
- https://onecommander.com/support
- https://onecommander.com/releasenotes.txt (3.7 → 3.108 sürüm notları)
- https://onecommander.com/help2 (çevrimiçi kılavuz; içindekiler `https://onecommander.com/help2/_toc.json`, okunan sayfalar arasında): `BrowsersandLayouts.html`, `FolderColumnsandNavigationPane.html`, `FolderPane.html`, `FileViews.html`, `FileViewDesignerEditor.html`, `Tabs.html`, `Sidebar.html`, `Favorites.html`, `DrivesColumn.html`, `NetworkDrives.html`, `PathEditDropdown.html`, `NewFileTemplates.html`, `Basicfolderandfileoperations.html`, `Selectingandopeningfilesandfolde.html`, `DragDrop.html`, `Clipboard.html`, `Downloadanywebsitevideoasafile.html`, `Preview.html`, `Preview1.html`, `DetailsPane.html`, `Searchingfilesfolders.html`, `Archives.html`, `Phonestorage.html`, `Themes.html`, `Theme.html`, `Creatingthemes.html`, `Shortcutkeys.html`, `Settings.html`, `General.html`, `Columns.html`, `View.html`, `Window.html`, `Other.html`, `Advanced.html`, `FileAutomator.html`, `StartingfromCommandline.html`, `Startingclosing.html`, `Windows.html`, `Memorymanagement.html`, `Installing.html`, `Migratingsettings.html`, `Updating.html`, `PurchasingProlicense.html`, `SoftwareLicenseAgreement.html`, `Systemrequirements.html`, `LimitationsofMicrosoftStoreversi.html`, `FAQRandom.html`
- https://www.onecommander.com/help/ (eski kılavuz, "Other useful features" sayfası)

Destekleyici (site zayıf kaldığı yerlerde):

- https://maketecheasier.com/onecommander-file-explorer-alternative (File Automator, etiketler, notlar, arama)
- https://tech.yahoo.com/apps/articles/file-manager-solves-windows-file-130015598.html (klasör boyutları, etiketler)
- https://redlib.hbubli.cc/r/OneCommander/comments/1it6ujh/file_automator (File Automator'da regex ve Simple Rename; geliştiricinin yorumu)
