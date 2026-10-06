# Directory Opus ile karşılaştırma: Gezik'te eksik olanlar

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** Directory Opus 13.25 (2026-08-21, kararlı; GPSoftware, yalnız Windows, kapalı kaynak, ücretli); Gezik `master` (29dcb45) + 5d (PDF, başka dalda; var sayıldı).
- **Kaynaklar:** gpsoft.com.au ana sayfa ve sipariş sayfaları, çevrimiçi kılavuz (docs.dopus.com: Basic Concepts, Lister, File Operations, FTP, Additional Functionality, Customize, Scripting, VFS), Opus 13 "Highlights" (6 sayfa) ve "Detailed list of changes" (45 bölümden okunanlar), 13.25 sürüm notu, Light/Pro farkları için forum ve inceleme (sonda). Opus çok büyük olduğundan derinlik değil genişlik hedeflendi.
- **Gezik tarafı:** `docs/superpowers/specs/` altındaki beş spec (Kapsam dışı bölümleriyle), README, `settings.toml` şablonu, `gezik-config::shortcuts::Action` (27 eylem), kodda kısa kontroller (klasör izleme `folder_watch.rs`, sağ tıkta `Commands ▸`, çok pencere komutu yok).

**Durumlar:** **Var** · **Kısmen** (eksiği yazılı) · **Yok** · **Planlı** (yol haritası adımı yazılı).

**Yol haritası adları:** *Etiketler* (5. spec'te ayrı adım 6; kullanıcının özetinde Gelişmiş'in içinde), *Taşınabilirlik* (dışa/içe aktarma, senkron klasör, GitHub'dan tema), *Bulut senkronu* (ayarların senkronu), *Gelişmiş* (arama, çift panel, arşivin içinde gezinme ve arşivden silme, Git, komut paleti; dosya işlemleri spec'ine göre MTP gibi sanal Shell öğeleri de), *Güncelleme* (gezinme spec'i: "ilk herkese açık sürümden önce").

---

## 1. Özellik tablosu

### 1.1 Gezinme ve düzenler (Lister, paneller, sekmeler, ağaç)

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 1 | Çift panel (yatay/dikey), kaynak/hedef kavramı, paneller arası kopyala/taşı | **Planlı** | Gelişmiş |
| 2 | Klasör ağacı: kök sırası, sabitlenen dallar, dal ön ayarları, Bulut ve ağ bölümleri; çift panelde çift ağaç | **Yok** | Kenar çubuğunda yalnız FOLDERS / PINNED / DRIVES listeleri |
| 3 | Sekmeler: yeni/kapat/çoğalt, orta tıkla yeni sekme, sürükleyerek sırala, sekmeye bırakma | **Var** | |
| 4 | Kilitli sekmeler, sekme grupları (kaydedilip açılan setler), Ctrl+Tab sekme seçici, sekmeyi dışarı sürükleyip yeni pencere | **Yok** | |
| 5 | Çoklu Lister penceresi, kaydedilen düzenler (Layouts), açık pencerelerin otomatik yedeği / oturum geri yükleme | **Yok** | Oturum geri yükleme gezinme spec'inde bilerek dışarıda |
| 6 | Lister stilleri ve Lister Defaults (ağaç, paneller, araç çubuklarının ön ayarlı düzenleri) | **Kısmen** | `layout.sidebar` ve `density` var; adlandırılmış stil yok |
| 7 | Favoriler çubuğu (yer imi gibi, orta tıkla sekmede) ve Favoriler listesi | **Kısmen** | Tek PINNED listesi, sürükleyerek sıralama; çubuk ve gruplar yok |
| 8 | Konum çubuğu: breadcrumb, yol yazma, otomatik tamamlama, parçada alt klasör listesi | **Kısmen** | Breadcrumb + Ctrl+L var; tamamlama yok, alt klasör listesi gezinme spec'inde bilerek dışarıda |
| 9 | Geri/ileri/üst, fare yan tuşları, geçmiş açılır listesi | **Kısmen** | Geçmiş listesi yok |
| 10 | Eşli klasörler (Paired Folders: yol ya da regex eşlemesiyle karşılık gelen klasöre atlama) | **Yok** | |
| 11 | Genişletilebilir klasörler (liste içinde alt klasörü açma) | **Yok** | |
| 12 | Explorer'ın yerine geçme (Win+E ve sistemde açılan klasörler Opus'ta) | **Yok** | |
| 13 | Takma adlar (`/desktop`, `/downloads` gibi yol kısaltmaları, kullanıcı tanımlı) | **Kısmen** | `{home}`, `{documents}`… yalnız `settings.toml`'da; adres çubuğunda yok |
| 14 | Sistem sanal klasörleri (This PC, Ağ, Çöp Kutusu, Denetim Masası) yerel olarak | **Kısmen** | Yalnız "This PC" |

### 1.2 Görünümler (ayrıntılar, küçük resim, power, düz görünüm, klasör biçimleri)

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 15 | Görünüm modları: Ayrıntılar, Liste, Büyük/Küçük simge, Döşeme (Tiles), Küçük resim | **Kısmen** | Liste (ayrıntılar) ve ızgara (3 boy, Ctrl+tekerlek); Tiles/List yok |
| 16 | Power modu (Opus'a özgü seçim davranışlı ayrıntı görünümü) | **Yok** | Düşük değer |
| 17 | Çok sayıda sütun (EXIF, ID3, video, belge, öznitelik…), sütun dondurma, hücre kopyalama, Explorer sütunlarını alma | **Kısmen** | Ad, değişme, oluşturma, tür, boyut; göster/gizle ve genişlik |
| 18 | Özel sütunlar (betik ya da Evaluator ifadesiyle) | **Yok** | |
| 19 | Gruplama (daraltılabilir gruplar, özel gruplama) ve çok alanlı sıralama | **Yok** | Gruplama görünüm spec'inde bilerek dışarıda |
| 20 | Doğal sıralama | **Var** | Türkçe harf sırasıyla, her sistemde aynı |
| 21 | Klasör biçimleri: klasöre, joker yola, türe göre görünüm; alt klasörlere miras; değişikliği otomatik hatırlama | **Kısmen** | Genel varsayılan + klasör başına hatırlama (500, LRU); joker ve miras yok |
| 22 | Düz görünüm (Flat View: alt klasörlerin içeriği tek listede; karışık, gruplu, klasörsüz) | **Yok** | |
| 23 | Küçük resim ölçek/kırpma modları, klasöre özel küçük resim stili, bindirilen tür simgesi | **Kısmen** | 3 boy; kırpma/stil yok |
| 24 | Onay kutulu seçim modu | **Yok** | Görünüm spec'inde bilerek dışarıda |
| 25 | Durum çubuğu: kodlarla özelleştirme, seçim özeti (tür sayıları, uzantı pasta grafiği), boş alan | **Kısmen** | Öğe sayısı ve seçim boyutu; özelleştirme yok |
| 26 | Gizli/sistem dosyası görünürlüğü, kalıcı gizleme süzgeçleri (13.22) | **Kısmen** | Yalnız nokta ile başlayan adlar (`toggle-hidden`) |
| 27 | Canlı klasör izleme | **Var** | |
| 28 | Yazı tipi, satır aralığı, boyut ve tarih biçimi seçenekleri | **Kısmen** | `font-family`, `font-size`, `density`; boyut/tarih biçimi yok |

### 1.3 Görüntüleyici ve önizleme

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 29 | Görüntüleyici paneli (Viewer Pane) | **Var** | Alt+P |
| 30 | QuickShow (Quick Look benzeri hızlı resim penceresi) | **Var** | Boşluk ile hızlı bakış, oklarla sonraki |
| 31 | Bağımsız görüntüleyici: çoklu pencere, ikinci monitöre bağlama, tam ekran, slayt gösterisi | **Kısmen** | Hızlı bakış penceresi var; tam ekran ve slayt yok |
| 32 | Geniş resim desteği (RAW, PSD, DDS, AVIF, HEIC…), GPU hızlandırmalı (13.23), yakınlaştırma/kaydırma | **Kısmen** | Gezik png/jpeg/gif/webp/bmp çözer; diğerleri sistem küçük resmi; yakınlaştırma yok |
| 33 | Görüntüleyicide basit düzenleme: kayıpsız JPEG döndürme, kırpma, boyutlandırma, kaydetme | **Kısmen** | Convert katmanında boyutlandırma ve EXIF'e göre döndürme; görüntüleyicide düzenleme yok |
| 34 | Resim işaretleme (bakarken seçip ayıklama) | **Yok** | |
| 35 | Video/ses oynatma (AV1, HEVC, VP9; kapak resmi; çalma listesi) | **Yok** | Görünüm spec'inde bilerek dışarıda |
| 36 | Belge önizleme: PDF, Office, sözdizimi renkli metin, Windows önizleme işleyicileri, eklentiler | **Kısmen** | Metin (ilk 64 KB); PDF/Office yalnız sistem küçük resmi |
| 37 | GPS konumunu harita servisinde açma | **Yok** | |

### 1.4 Dosya işlemleri ve kopya kuyruğu

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 38 | Kopyala/taşı/sil, sistem panosu, sürükle-bırak (dışarı/içeri) | **Var** | Üç sistemde |
| 39 | Kopya kuyrukları: adlandırılmış kuyruklar, sürükleyerek sıralama, kuyruklar arası taşıma, otomatik kurallar | **Kısmen** | Disk kümesine göre otomatik kuyruk (aynı disk sırayla, farklı disk paralel); elle düzen yok |
| 40 | İlerleme penceresi: hız grafiği (ortalama/tepe), duraklat, iptal | **Var** | Pencere içi panel, Windows'ta görev çubuğu ilerlemesi; grafik yok |
| 41 | Çakışma (Replace) penceresi: değiştir, atla, yeniden adlandır, yeniyse; ad ön ayarları | **Var** | Gezik'te başlamadan tek liste, ezilen dosya çöpe |
| 42 | Çok düzeyli geri alma | **Var** | Oturum boyunca; Gezik'te daha geniş (bölüm 2) |
| 43 | İşlem günlüğü (logging), Task Manager | **Kısmen** | "N items failed · Details"; kalıcı günlük yok |
| 44 | Kopya sonrası doğrulama, yalnız yenileri kopyala, öznitelik/izin/zaman koruma seçenekleri | **Kısmen** | Zaman ve izin korunur, çakışmada "If newer"; doğrulama ve seçenekler yok |
| 45 | Sunucu tarafı kopya (SMB offload), büyük dosyada arabelleksiz kopya | **Var** | `CopyFileExW`, ≥ 256 MB `NO_BUFFERING`; macOS/Linux'ta klonlama |
| 46 | Süzgeçli işlemler (yalnız eşleşenleri kopyala/sil, özyinelemeli süzgeç) | **Yok** | |
| 47 | Hedefte hayalet yer tutucular, kaynakta simge bindirmesi | **Kısmen** | Kesilen öğeler soluk; hedef yer tutucu yok |
| 48 | Güvenli silme, boş alanı güvenli silme | **Yok** | |
| 49 | Çöp kutusu ve kalıcı silme | **Var** | Kalıcı silme anında (gizli ad + arka plan) |
| 50 | Öznitelik ve zaman damgası değiştirme (toplu, SetAttr) | **Yok** | |
| 51 | Kısayol, sembolik bağlantı, junction, sert bağlantı oluşturma | **Yok** | Dosya işlemleri spec'inde bilerek dışarıda |
| 52 | Dosya bölme / birleştirme | **Kısmen** | 7z parçalı arşiv; ham bölme/birleştirme yok |
| 53 | Yeni klasör (önceki adlardan öneri), yeni dosya, çoğalt | **Var** | Ad önerisi yok |
| 54 | UAC / yönetici modu (yükseltilmiş işlemler) | **Yok** | |
| 55 | MTP aygıtları (telefon, kamera) klasör gibi | **Planlı** | Gelişmiş (dosya işlemleri spec'i: sanal Shell öğeleri) |

### 1.5 Arama, süzgeçler, jokerler

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 56 | Yazdıkça bul (FAYT): ada atlama, süzme modu, eşleşenleri seç (Ctrl+S), sonraki (F3), kaydırma çubuğunda işaret | **Kısmen** | Yalnız ilk harflerle atlama |
| 57 | Süzgeç çubuğu / hızlı süzme (joker, regex) | **Yok** | |
| 58 | Dosya Bul: ad, içerik, tarih, boyut, öznitelik, meta veri, regex, ön ayarlar, hariç klasörler | **Planlı** | Gelişmiş (arama) |
| 59 | Everything bütünleşmesi (dizinli anlık arama), Windows Search, kayıtlı sorgular | **Planlı** | Gelişmiş (arama); dizin kullanımı spec'te henüz yok |
| 60 | Joker/regex ile seç, meta veriye göre seç, seçimi tersine çevir, aynı türü seç | **Yok** | Ctrl+A ve fare/klavye seçimi var |
| 61 | Gelişmiş süzgeçler (koşul ağacı, metin biçimli), genel gizleme süzgeçleri | **Yok** | |
| 62 | Dosya koleksiyonları (kalıcı sanal klasör) ve Kütüphaneler | **Yok** | |

### 1.6 Etiketler, derecelendirme, meta veri

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 63 | Renkli etiketler: kategoriler, satır rengi, başa sabitleme, joker/süzgeçle otomatik etiket | **Planlı** | Etiketler (otomatik kurallar planda yok) |
| 64 | Windows etiketleri (tags) ve durum simgeleri | **Planlı** | Etiketler (macOS etiketleri tüm sistemlere) |
| 65 | Derecelendirme (1-5 yıldız) | **Yok** | Etiketler adımına eklenebilir |
| 66 | Dosya açıklamaları (yorumlar, `descript.ion`) | **Yok** | |
| 67 | Meta veri paneli (EXIF, ID3, belge, MediaInfo ile video) | **Kısmen** | Önizlemede ad, tür, boyut, tarihler, piksel boyutu |
| 68 | Meta veri düzenleme: tekli/toplu, müzik etiketleri, GPS, çok değerli alanlar | **Kısmen** | Yalnız silme (Convert: EXIF ve konum) |

### 1.7 Tema, araç çubuğu ve menüler

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 69 | Koyu/açık mod, yüzlerce renk, tema | **Var** | TOML temalar, `auto`, kalıtım, canlı yenileme |
| 70 | Yazı tipleri (liste, diyaloglar, ipuçları ayrı ayrı) | **Kısmen** | Tek `font-family` / `font-size` |
| 71 | Simge setleri, yüksek DPI vektör simgeler | **Kısmen** | `icons` sistem ya da Gezik simgeleri; set yok |
| 72 | Araç çubukları: düzenleme, yeni çubuk, yüzen çubuk (program başlatıcı), genişliğe uyan düğmeler | **Yok** | Temanın düzen değiştirmesi ayarlar spec'inde bilerek dışarıda |
| 73 | Menü ve sağ tık menüsü özelleştirme; dosya türüne göre eylemler (File Types) | **Kısmen** | Yerel Shell menüsü + türe göre `[[commands]]`; menü düzenleme yok |
| 74 | Grafik Tercihler ve Özelleştir pencereleri | **Yok** | Ayarlar spec'inde ayrı alt proje diye dışarıda |
| 75 | Ayar yedekleme/geri yükleme, dışa/içe aktarma | **Planlı** | Taşınabilirlik (`settings.toml` bugün de kopyalanarak taşınır) |
| 76 | Düğme/araç çubuğu paylaşımı (forumdan sürükle-bırak) | **Planlı** | Taşınabilirlik (yalnız GitHub'dan tema) |

### 1.8 Arşivler ve sanal dosya sistemi

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 77 | Arşivi klasör gibi gezme (zip, 7z, rar, iso, vhd…) | **Planlı** | Gelişmiş |
| 78 | Arşiv açma (7-Zip'in tüm biçimleri) | **Var** | İçeride zip/7z/rar/tar/cab/iso/deb; nadirleri indirilen 7-Zip ile |
| 79 | Arşiv oluşturma, var olan arşive ekleme, şifre, sıkıştırma düzeyi | **Var** | zip/7z/tar; AES, ad şifreleme, 7z parçalı |
| 80 | Arşivden öğe silme, arşiv içinde yeniden adlandırma | **Planlı** | Gelişmiş (yeniden yazma motoru 5b'de hazır) |
| 81 | FTP (temel lisansta) ve adres defteri | **Yok** | Yol haritasında yok |
| 82 | SFTP/SSH ve FTPS (ücretli Advanced FTP eklentisi), FTP ile senkron | **Yok** | Yol haritasında yok |

### 1.9 Bulut

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 83 | Bulut klasörleri ağaçta ayrı bölüm; Özel Klasörler sayfasında bulut sayma/gizleme; Dropbox özel durumu | **Yok** | Bulut senkronu adımı bu değil (o Gezik'in ayarlarını taşır) |
| 84 | Bulut dosya komutları: "Always keep on this device" (PIN), "Free up space" (DEHYDRATE), durum | **Yok** | |

Directory Opus'ta ayar senkronu **yok** (yalnız yedekleme dosyası); Gezik'in Bulut senkronu adımı bu yönden öne geçer.

### 1.10 Klavye, kısayollar, komut paleti

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 85 | Uygulama içi kısayollar, düzenlenebilir | **Var** | 27 eylem, `[shortcuts]`, canlı |
| 86 | Her komuta ya da dış programa kısayol bağlama, kısayol yönetim arayüzü | **Kısmen** | `[[commands]]` kısayola bağlanamaz; arayüz yok |
| 87 | Sistem geneli kısayollar (Opus odakta değilken) | **Yok** | |
| 88 | FAYT komut modu (`>` ile komut, otomatik tamamlama), betikle eklenen FAYT modları | **Planlı** | Gelişmiş (komut paleti) |
| 89 | Komut satırı ve `dopusrt.exe` ile dışarıdan komut gönderme | **Kısmen** | Yalnız açılacak klasör argümanı |

### 1.11 Gelişmiş yeniden adlandırma

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 90 | Yerinde yeniden adlandırma (F2), yapılandırılabilir tuşlar | **Var** | Tuşlar sabit |
| 91 | Toplu: joker, bul/değiştir, regex, büyük/küçük harf, numaralandırma, canlı önizleme | **Var** | Önizlemede elle düzeltme, sürükleyerek sıra |
| 92 | Meta veri alanlarıyla ad (EXIF, ID3, belge, video) | **Kısmen** | EXIF çekim tarihi, değişme tarihi, boyut, üst klasör; ID3/diğerleri yok |
| 93 | Ön ayarlar, geçmişten öneriler | **Kısmen** | Kayıtlı kural setleri; geçmiş önerisi yok |
| 94 | Betikle adlandırma (JScript/VBScript) ve adlandırma makroları | **Yok** | |
| 95 | Alt klasörlerdeki dosyaları özyinelemeli adlandırma | **Yok** | Yalnız seçili öğeler |
| 96 | Adlandırmayı geri alma | **Var** | Döngü güvenli (a↔b) |

### 1.12 Betik ve komut dili

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 97 | İç komut dili (yüzlerce komut ve argüman; düğme, menü, kısayola yazılır) | **Yok** | |
| 98 | Active Scripting: JScript, VBScript (Python/Perl kuruluysa) | **Yok** | |
| 99 | Betik eklentileri: yeni komutlar, sütunlar, olaylar, özel diyaloglar | **Yok** | |
| 100 | Script IDE, betik günlüğü | **Yok** | |
| 101 | Evaluator ifade dili (durum çubuğu, sütun, başlık, süzgeçlerde) | **Yok** | |
| 102 | Dış program çalıştırma, seçili dosya yer tutucuları | **Kısmen** | `[[commands]]` kabuksuz, güvenli yer tutucular, paralel, geri alınabilir; tüm seçimi tek çağrıda verme ve soru penceresi yok |
| 103 | Eklenti API'si (görüntüleyici, VFS, sütun eklentileri; C++ SDK) | **Yok** | |

### 1.13 Görsel dönüştürme ve diğer araçlar

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 104 | Görsel dönüştürücü: biçim, boyut, döndürme, kalite, ön ayarlar, toplu kırpma | **Var** | Kırpma yok; Gezik'te HEIC/AVIF ve konum silme ek |
| 105 | Senkronize et (klasör karşılaştırma, tek/iki yönlü eşitleme) | **Yok** | |
| 106 | Yinelenen dosya bulucu (MD5, ad, boyut; konuma göre seçme) | **Yok** | |
| 107 | Klasör boyutu (sütun; Everything ile anında) | **Yok** | Önizlemede yalnız öğe sayısı |
| 108 | Kontrol toplamı sütunları (MD5, SHA-1) | **Yok** | |
| 109 | Klasör listesini yazdır / dışa aktar (metin, CSV, HTML, pano) | **Yok** | |
| 110 | Güncelleme denetleyicisi | **Planlı** | Güncelleme (ilk sürümden önce) |
| 111 | Arayüz dilleri | **Yok** | Gezinme spec'inde bilerek dışarıda |
| 112 | USB'den kurulumsuz çalışan kopya (USB Export, ücretli) | **Kısmen** | `GEZIK_CONFIG_DIR` ile taşınabilir ayar; paket yok |

### 1.14 Performans

| # | Directory Opus özelliği | Gezik | Not |
|---|---|---|---|
| 113 | 64 bit yerel kod, çok iş parçacıklı listeleme ve işlemler, GPU'lu görüntüleyici | **Var** | Rust; arayüz dosya sistemini beklemez; açılış 24-35 ms, boşta ~7 MB, 100 bin dosya ~17 MB |
| 114 | Çok sekme/pencerede ölçeklenme | **Var** | 20 sekme +0,1 MB (Opus için karşılaştırmalı ölçüm yapılmadı) |

### 1.15 Light ve Pro

Opus 13 ile **Light sürümü kaldırıldı**; artık tek sürüm var (GPSoftware: "çoğu kişi zaten Pro alıyordu"). Lisans kalıcıdır, belirli bir süre ücretsiz güncelleme içerir, sonrası yıllık güncelleme ücretidir; SFTP/FTPS (Advanced FTP) ve USB Export ek ücretlidir. Fiyatlar sitede dinamik gösteriliyor, bu notta doğrulanmadı (Opus 12'de Light ~A$30, Pro ~A$70 idi).

Opus 12'de yalnız Pro'da olanlar ve Gezik'teki karşılıkları (tabloda sayılmadı, yukarıdaki satırlara bağlı):

| Pro'ya özgüydü | Gezik |
|---|---|
| Explorer'ın yerine geçme | Yok (12) |
| Varsayılan araç çubukları, menüler, simgeler dahil tam arayüz düzenleme | Yok (72, 73) |
| Özyinelemeli süzgeçler, güvenli silme, UAC yönetici modu, çok düzeyli geri alma, günlük | Geri alma Var; diğerleri Yok/Kısmen (42-48, 54) |
| Senkronize et, yinelenen dosya bulucu, bölme/birleştirme | Yok / Kısmen (52, 105, 106) |
| Çok satırlı komutlar, Active Scripting | Yok; `[[commands]]` Kısmen (97-102) |
| zip dışı arşiv okuma/yazma (7z, rar, iso, vhd) | Var, ücretsiz (78, 79); içinde gezinme Planlı |
| FTP, MTP | Yok / Planlı (55, 81) |
| Meta veri düzenleme, düz görünüm, kayıtlı sorgular | Kısmen / Yok (22, 59, 68) |

---

## 2. Gezik'te olup Directory Opus'ta olmayanlar

| Alan | Gezik | Directory Opus |
|---|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından | Yalnız Windows (Amiga sürümü tarihsel) |
| Fiyat ve lisans | Kişisel, eğitim ve kâr amacı gütmeyen kullanımda ücretsiz; kaynak açık okunur (PolyForm Noncommercial). Ticari kullanım yok | Kapalı kaynak, ücretli lisans + yıllık güncelleme ücreti; SFTP ve USB ek ücretli; ticari kullanım satın alınabilir |
| Hafiflik | Açılış 24-35 ms, boşta ~7 MB, 100 bin dosya ~17 MB; exe ~13 MB | Ağır, çok özellikli yerel uygulama (bellek ölçülmedi) |
| Çakışmalar | Başlamadan tüm çakışmalar tek listede, satır başına karar, ezilen dosya çöpe (geri alınabilir) | Dosya dosya Replace penceresi |
| Kalıcı silme | Anında (gizli ada çevir, arka planda sil, çökmede açılışta tamamla) | Normal silme |
| Geri alma | Kopyala, taşı, ad, yeni öğe, çöp, değiştirme, toplu ad, arşiv açma/oluşturma, dönüştürme, kullanıcı komutu; tek Ctrl+Z | Kopyala, taşı, ad, silme, yeni klasör |
| Çökme güvenliği | Yarım kopya/arşiv/dönüşüm hiç tamamlanmış görünmez, açılışta temizlenir | Belgelenmemiş |
| PDF (5d) | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'ten resim (pdfium tek tıkla iner) | Yok (yalnız görüntüleme) |
| Ses/video dönüştürme | ffmpeg hazır ayarları (MP4, küçült, MP3, M4A, WAV, remux, GIF); ffmpeg tek tıkla, SHA-256 sabitli | Yok (görüntüleyici oynatır, dönüştürmez) |
| Metin dönüştürme | Kodlama algılama ve dönüştürme (Windows-1254 → UTF-8), satır sonu, sondaki boşluklar | Yok |
| HEIC / AVIF / konum silme | Dönüştürmede HEIC/AVIF girdi, AVIF/kayıplı WebP çıktı, EXIF konumunu silme | AVIF görüntüleme; dönüştürmede yok |
| Arşiv güvenliği | Zip-slip, mutlak yol, ADS, sıkıştırma bombası denetimi; aşama klasörü | Belgelenmemiş |
| Kullanıcı komutları | Kabuksuz argüman dizisi, güvenli yer tutucular, paralel, yerinde değişiklikte çöpe kopya ile geri alma | Güçlü ama kabuk/betik tabanlı |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı, satır numaralı hata, işletim sistemleri arası taşınabilir | İkili/XML yapılandırma, yedek dosyası; elle düzenlenmek için değil |
| Türkçe | Doğal sıralamada Türkçe harf sırası; büyük/küçük harfte i↔İ, ı↔I | Sistem karşılaştırması |
| Yol haritasında | Ayar senkronu (şifreli, hesapsız), GitHub'dan tema, Git | Hiçbiri (Git yalnız topluluk betikleriyle) |

---

## 3. Sayım

| Durum | Sayı |
|---|---|
| Var | 22 |
| Kısmen | 34 |
| Yok | 46 |
| Planlı | 12 |
| **Toplam** | **114** |

Bu sayım yalnız numaralı satırlardır (1-114); Light/Pro tablosu ve bölüm 2 sayılmadı.

---

## 4. Öneri: değer / emek sırası

Gezik'in kitlesi Explorer/Finder/Files'tan geçen, hızlı ve hafif bir yönetici isteyen günlük kullanıcıdır; Opus'un kitlesi ise yapılandırmayı seven Windows ileri kullanıcısıdır. Değer: tipik bir kullanıcının günlük işine etkisi. Emek: Gezik'in bugünkü altyapısına göre tahmin (D düşük, O orta, Y yüksek).

| Sıra | Özellik (satır) | Durum | Değer | Emek | Nereye |
|---|---|---|---|---|---|
| 1 | Dosya bulma + hızlı süzgeç (58, 57, 56) | Planlı/Yok/Kısmen | Çok yüksek | O | Gelişmiş'ten öne: **Arama** ayrı adım |
| 2 | Klasör boyutu (107) | Yok | Yüksek | O | Arama adımına (aynı arka plan tarayıcı ve önbellek) |
| 3 | Çift panel (1) | Planlı | Yüksek | Y | Gelişmiş; ağaçla birlikte **Düzenler** adımı |
| 4 | Klasör ağacı (2) | Yok | Yüksek (Explorer alışkanlığı) | O | Düzenler (yeni) |
| 5 | Oturum geri yükleme, Ctrl+Tab seçici, kapatılan sekmeyi aç (5, 4) | Yok | Yüksek | D | Yeni küçük adım **Günlük kolaylıklar** (gezinme spec'indeki "tek sekme" kararı ayara dönüşür) |
| 6 | Etiketler + derecelendirme (63-65) | Planlı/Yok | Orta-yüksek | O | Etiketler (yıldızı ekle; otomatik kurallar sonra) |
| 7 | Yinelenen dosya bulucu + kontrol toplamları (106, 108) | Yok | Orta-yüksek | O | Yeni adım **Araçlar** (5'in motoru, panel ve geri almayı kullanır) |
| 8 | Klasör karşılaştır / senkronize et (105) | Yok | Orta-yüksek (yedek alan kullanıcı) | O-Y | Araçlar |
| 9 | Zengin önizleme: video/ses, PDF sayfaları, RAW/HEIC, yakınlaştırma (32, 35, 36) | Kısmen/Yok | Orta-yüksek | O | Yeni adım **Önizleme 2**; 5c/5d'nin ffmpeg ve pdfium'u hazır |
| 10 | Arşivin içinde gezinme (77, 80) | Planlı | Orta-yüksek | O | Gelişmiş (olduğu gibi) |
| 11 | Joker/regex ile seç, gruplama (60, 19) | Yok | Orta | D-O | Arama adımına |
| 12 | Meta veri paneli ve basit düzenleme (67, 68) | Kısmen | Orta | O | Önizleme 2 (aynı meta veri okuyucu) |
| 13 | Komut paleti (88) | Planlı | Orta (Gezik'i ayırır) | O | Gelişmiş |
| 14 | Grafik ayar penceresi (74) | Yok | Yüksek (teknik olmayan kullanıcı) | Y | Taşınabilirlik (dışa/içe aktarma da arayüz ister) |
| 15 | Düz görünüm (22) | Yok | Orta | O | Arama adımına (aynı özyinelemeli tarayıcı) |
| 16 | Listeyi dışa aktar (109) | Yok | Düşük-orta | D | Araçlar |
| 17 | Bağlantı oluşturma, öznitelik/tarih değiştirme (51, 50) | Yok | Düşük-orta | D | Günlük kolaylıklar |
| 18 | Bulut dosya durumu ve komutları (83, 84) | Yok | Orta (OneDrive/iCloud kullanıcısı) | O | Yeni adım **Sistem bütünleşmesi** |
| 19 | Explorer'ın yerine geçme, sistem geneli kısayol (12, 87) | Yok | Orta | O | Sistem bütünleşmesi |
| 20 | SFTP/FTP (81, 82) | Yok | Düşük-orta (geliştirici) | Y | Yeni adım **Uzak konumlar**; ilk sürüm için gerekli değil |

**Gezik için değmeyenler** (Opus'un ileri kullanıcıya yönelik çekirdeği; "hızlı ve hafif" hedefiyle ya da kitleyle uyuşmaz):

- İç komut dili, Active Scripting, Script IDE, Evaluator, eklenti SDK'sı (97-101, 103): yorumlayıcı gerektirir; ayarlar spec'i bunu bilerek dışarıda bıraktı. Karşılığı `[[commands]]`'ı büyütmek (tüm seçimi tek çağrıda, kısayola bağlama: 86, 102) ve komut paletidir.
- Power modu, onay kutulu seçim, özel sütunlar, durum çubuğu kodları, joker yollu klasör biçimleri, Lister stilleri, eşli klasörler (16, 18, 21, 24, 25, 6, 10).
- Araç çubuğu/menü düzenleyici (72): değer orta, emek yüksek; komut paleti + kısayol daha ucuz yoldan aynı ihtiyacı karşılar.
- Süzgeçli işlemler, güvenli silme ve boş alan silme, hayalet yer tutucular, kuyruk düzenleme, USB Export (46-48, 39, 112): SSD'de güvenli silme zaten güvenilmez; diğerleri nadir.
- Resim işaretleme, slayt gösterisi, GPS haritası, Tiles görünümü (34, 31, 37, 15): Önizleme 2'ye ucuzsa eklenir, kendi başına adım istemez.

**Özet öneri**

- Var olan adımlara eklenecekler: arama + süzgeç + klasör boyutu + düz görünüm + joker seçim → *Gelişmiş*'ten çıkarılıp **Arama** adımı; yıldız → *Etiketler*; grafik ayar penceresi → *Taşınabilirlik*.
- Yeni adım gerekenler: **Düzenler** (çift panel + klasör ağacı; çift panel Gelişmiş'ten taşınır), **Araçlar** (yinelenen, senkron, kontrol toplamı, liste dışa aktarma), **Önizleme 2** (video/ses, PDF, meta veri), **Günlük kolaylıklar** (oturum, sekme seçici, bağlantılar, öznitelikler), **Sistem bütünleşmesi** (bulut durumu, Explorer yerine geçme, genel kısayol), sonra **Uzak konumlar** (SFTP).
- Arama, Düzenler çıkınca *Gelişmiş*'te Git, komut paleti, arşivin içinde gezinme ve MTP kalır. Git ve ayar senkronu Opus'ta yok: Gezik'in ayırt edici yanı. Opus'la özellik sayısında yarışmak hedef olmamalı; Gezik'in üstünlüğü çapraz platform, hafiflik, güvenli/geri alınabilir işlemler ve kurulum gerektirmeyen dönüştürme/PDF.

---

## Kaynaklar

GPSoftware'in kendi siteleri:

- https://www.gpsoft.com.au/ (ana sayfa, özellik listesi)
- https://www.gpsoft.com.au/order_intro.html (lisanslar, Advanced FTP, USB Export)
- https://www.gpsoft.com.au/upgrade (lisans yapısı, yıllık güncelleme)
- https://blog.dopus.com/2024/02/announcing-directory-opus-13.html
- https://docs.dopus.com/doku.php?id=start (kılavuz içindekiler)
- https://docs.dopus.com/doku.php?id=introduction
- https://docs.dopus.com/doku.php?id=basic_concepts
- https://docs.dopus.com/doku.php?id=basic_concepts:the_lister
- https://docs.dopus.com/doku.php?id=basic_concepts:searching_and_filtering
- https://docs.dopus.com/doku.php?id=basic_concepts:virtual_file_system
- https://docs.dopus.com/doku.php?id=file_operations
- https://docs.dopus.com/doku.php?id=file_operations:copying_moving_and_deleting_files
- https://docs.dopus.com/doku.php?id=file_operations:renaming_files
- https://docs.dopus.com/doku.php?id=ftp
- https://docs.dopus.com/doku.php?id=additional_functionality
- https://docs.dopus.com/doku.php?id=additional_functionality:viewing_images
- https://docs.dopus.com/doku.php?id=customize
- https://docs.dopus.com/doku.php?id=scripting
- https://docs.dopus.com/doku.php?id=release_history
- https://docs.dopus.com/doku.php?id=release_history:opus13 ve `:page1` … `:page6` (Opus 13 Highlights)
- https://docs.dopus.com/doku.php?id=release_history:opus13_detailed ve okunan bölümler: `:file_copying`, `:archives`, `:labels`, `:cloud_storage`, `:misc_features`
- https://docs.dopus.com/doku.php?id=release_history:opus13_upgradetips
- https://resource.dopus.com/t/directory-opus-13-25/60349 (13.25 sürüm notu)
- https://resource.dopus.com/t/directory-opus-13-23/59355 (GPU'lu görüntüleyici)
- https://resource.dopus.com/t/directory-opus-13-18/56946 (QuickShow)
- https://resource.dopus.com/t/directory-opus-13-22/58885 (kalıcı süzgeçler)

Light/Pro ve ek bilgiler (forum ve inceleme):

- https://resource.dopus.com/t/lite-directory-opus/55390 (Light'ın Opus 13'te kaldırılması)
- https://resource.dopus.com/t/killer-features-of-pro/26375 (Opus 12'de Pro'ya özgü özellikler)
- https://www.pcworld.com/article/455908/review-directory-opus-light-is-heavy-enough-for-most-purposes.html (Light/Pro fiyatları, Opus 12)
- https://resource.dopus.com/t/what-is-the-advanced-ftp-option/54205 (Advanced FTP: SFTP/SSH ve FTPS)
- https://resource.dopus.com/t/column-sha-256-and-sha-512/33525 (yerleşik MD5/SHA-1 sütunları)
- https://en.wikipedia.org/wiki/Directory_Opus (sürüm, koleksiyonlar, MTP)
