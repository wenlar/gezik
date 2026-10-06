# Total Commander ile karşılaştırma: Gezik'te eksik olanlar

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** Total Commander 11.58 (2026-07-01, Windows 32/64 bit) ve Android sürümü 3.62 (2026-02-15); Gezik `master` (29dcb45) + 5d (PDF, `feat/batch-ops-5d`; var sayıldı).
- **Kaynaklar:** ghisler.com sayfaları (ana sayfa, özellik listesi, sipariş, SSS, eklentiler, eklenti türleri, Android, USB kurulumu), `history.txt` (11.00 → 11.58 "Added" satırlarının tamamı), resmi 11.58 kurulum paketindeki İngilizce yardım (`TOTALCMD.CHM`; 265 sayfa, okunanlar sonda). TC'nin yardımı çevrimiçi HTML olarak yayımlanmıyor, kurulumla geliyor.
- **Gezik tarafı:** Diğer yedi notun Gezik envanteri (özellikle OneCommander ve Far notları), `docs/superpowers/specs/` altındaki beş spec (Kapsam dışı bölümleriyle), `2026-10-07-rakip-ozet.md`, `gezik-config::shortcuts::Action` (27 eylem).

**Durumlar:** **Var** · **Kısmen** (eksiği yazılı) · **Yok** (önerilen yol haritasının hiçbir adımında yok) · **Planlı** (rakip-ozet §2'deki önerilen yol haritasında; adım numarası yazılı, 6-18).

**Adımlar (rakip-ozet §2):** 6 Klavye paketi · 7 Günlük kolaylıklar · 8 Arama · 9 Sistem bütünleşmesi · 10 Düzenler · 11 Araçlar · 12 Etiketler · 13 Önizleme 2 · 14 Taşınabilirlik · 15 Yerelleştirme · Yayın hazırlığı (numarasız) · 16 Gelişmiş · 17 Bulut senkronu · 18 Uzak bağlantılar. "rö N" = rakip-ozet tablosunun N. satırı.

**Kaynak sütunu:** **Y** TC'ye yerleşik · **Eg** Ghisler'in kendi eklentisi (SFTP, WebDAV, Cloud…) · **E** üçüncü taraf eklenti · **A** yalnız Android sürümü.

---

## 1. Özellik tablosu

### 1.1 Paneller, sekmeler, gezinme

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 1 | İki panel; Tab ile geçiş, panelleri değiştir (Ctrl+U), Hedef=Kaynak; yatay/dikey yerleşim | Y | **Planlı** | 10 Düzenler (rö 4) |
| 2 | Karşı panele F5 kopyala / F6 taşı modeli | Y | **Planlı** | 10 (rö 4) |
| 3 | Klasör sekmeleri (panel başına): Ctrl+T, Ctrl+↑ ile alttaki klasörü sekmede aç, sürükleyerek sırala, çoğalt, orta tıkla kapat | Y | **Var** | Sekmeyi karşı panele taşıma Düzenler'le gelir |
| 4 | Sekme kilitleme: "kilitli" ve "kilitli ama gezilebilir" (geri dönünce kayıtlı klasöre döner); sekme adı | Y | **Yok** | Yeni rö 64 (öneri: 6) |
| 5 | Kapatılan sekmeleri geri açma; açık sekmelerin aranabilir listesi (Ctrl+Shift+A), Ctrl+Tab ile son ziyaret edilene | Y | **Planlı** | 6 (rö 28) |
| 6 | Sekmeleri dosyaya kaydet/yükle (`.tab`, iki panel birlikte), açılışta sekmeleri geri yükleme | Y | **Planlı** | 7 (rö 8) |
| 7 | Dizin hotlist (Ctrl+D): alt menüler, kısayol harfleri, sıralama, hedef paneli de ayarlayan girişler, 2000 öğe | Y | **Kısmen** | Tek PINNED listesi, sıralama ve bırakma var; alt menü, kısayol yok → 7 (rö 19) |
| 8 | Klasör geçmişi (Alt+↓): aranabilir/süzülebilir pencere, **sık kullanılan klasörler** (ziyaret sayısı, 30 günde yarılanır), kalıcı | Y | **Kısmen** | Sekme başına oturumluk geri/ileri; liste, sıklık ve kalıcılık yok → 6 (rö 10) |
| 9 | Breadcrumb, yol satırına yazma, yol tamamlama, `cd yol\:` ile üst klasöre gidip imleci koyma | Y | **Kısmen** | Breadcrumb ve Ctrl+L var; tamamlama yok → 6 (rö 10) |
| 10 | Sürücü düğmeleri ve listesi (etiket, boyut), sürücüyü çıkarma, sökülen sürücüden otomatik çıkma | Y | **Kısmen** | Kenar çubuğunda sürücüler; çıkarma yalnız Windows Shell menüsünden → 9 (rö 6) |
| 11 | Dizin ağacı: panel olarak ya da bir/iki ayrı ağaç paneli | Y | **Planlı** | 10 (rö 32) |
| 12 | Eşli gezinme (`cm_SyncChangeDir`; adlar farklıysa askıya alınır) | Y | **Planlı** | 10 (rö 41) |
| 13 | Ağ: ağ komşuları, sürücü eşle/ayır, klasör paylaş/paylaşımı kaldır, yönetici paylaşımları | Y | **Kısmen** | UNC yolu yazılıp açılır; tarama ve eşleme yok → 9 (rö 6) |
| 14 | Geri Dönüşüm Kutusu sanal klasörü (özgün konum sütunuyla) | Y | **Planlı** | 9 (rö 33); bugün yalnız Ctrl+Z ile geri |
| 15 | Android telefon / taşınabilir aygıtlar (MTP) sanal klasör olarak | Y | **Planlı** | 16 (rö 44) |
| 16 | Uzun yollar (> 259) ve Unicode adlar | Y | **Var** | Motor `\\?\` kullanır |

### 1.2 Görünüm, sütunlar, renkler

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 17 | Brief (çok sütunlu ad listesi), Full (ayrıntı), Comments (açıklama sütunu) görünümleri | Y | **Kısmen** | Ayrıntı listesi ve ızgara var; brief ve comments yok |
| 18 | Özel sütun setleri (29 adet): `tc` alanları + içerik eklentisi (WDX) alanları (EXIF, ID3, sürüm bilgisi, Explorer alanları), bu sütunlara göre sıralama | Y + E | **Planlı** | 5 sabit sütun bugün → 13 (rö 21) |
| 19 | Özel görünüm modları ve **otomatik görünüm kuralları**: yol, klasör türü (FTP, arşiv, arama sonucu, eklenti), dosya oranı ("en az yarısı resim") → sütun seti, sıralama, panel arka planı, sekme rengi/simgesi, otomatik komut | Y | **Kısmen** | Klasör başına sıralama/görünüm hafızası (`views.toml`); kural yok. Yeni rö 63 (öneri: 10) |
| 20 | Küçük resim görünümü: boyut, Ctrl+tekerlek, kalıcı veritabanı, EXIF gömülü önizlemesi, RAW, metin dosyası küçük resmi, altında özel alan satırları | Y | **Kısmen** | Izgara (3 boyut) ve bellekte önbellek var; kalıcı veritabanı ve alan satırı yok |
| 21 | Dosya türüne göre renk: maske, öznitelik, tarih, eklenti alanı → renk (açık/koyu ayrı) | Y | **Planlı** | Tema kategoriye göre ikonu renklendirir → 12 (rö 50) |
| 22 | Koyu mod (Windows'u izleme), renk ve yazı tipi ayarları | Y | **Var** | TOML temaları, `theme = "auto"` |
| 23 | Sıralama: ad, uzantı, boyut, tarih, sırasız; ikincil ölçütler; doğal sıralama | Y | **Kısmen** | Ad, tarih, tür, boyut ve Türkçe doğal sıra var; uzantı, sırasız, ikincil ölçüt yok |
| 24 | Gizli ve sistem dosyaları ayrı ayrı; **yok sayma listesi** (ad/yol maskesi, FTP için de) | Y | **Planlı** | Yalnız nokta ile başlayanlar (Ctrl+H) → 7 (rö 11) |
| 25 | Biçimler: boyut biçimi (bayt/k/M/dinamik), saniyeli tarih, uzantı ayrı sütun | Y | **Planlı** | 7 (rö 11) |
| 26 | Liste süzgeci menüsü: Tümü / Programlar / Özel maske / Yalnız seçililer | Y | **Planlı** | 6 (rö 1) |
| 27 | Klasör boyutu: Boşluk ile seçili klasör, Alt+Shift+Enter ile tümü, otomatik yükleme (yerel/ağ/Everything), "Calculate occupied space" | Y | **Planlı** | 8 (rö 3) |
| 28 | Arayüz dilleri (Türkçe dahil; standart pakette 19, ek dosyalarla daha çok) | Y | **Planlı** | 15 (rö 9) |
| 29 | Dosya açıklamaları (`descript.ion`, Ctrl+Z), kopya/taşımada korunur | Y | **Planlı** | 12 (rö 25) |

### 1.3 Klavye, Quick Search, komut satırı, düğme çubuğu

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 30 | F tuşu çubuğu (F3…F8) ve her işin klavyeyle yapıldığı düzen | Y | **Kısmen** | Explorer tarzı kısayollar; tuş çubuğu ve karşı panel modeli yok |
| 31 | Quick Search: Ctrl+Alt+harf / Alt+harf / yalnız harf; `*` ile adın her yerinde; "baştan" ve "noktaya kadar tam" seçenekleri | Y | **Kısmen** | Yalnız baştan harfle atlama → 6 (rö 1) |
| 32 | Quick Filter (Ctrl+S): eşleşmeyenleri gizle, son filtreyi geri yükle; `tcmatch.dll` ile dış eşleştirici (bulanık, Pinyin) | Y | **Planlı** | 6 (rö 1) |
| 33 | Seçim: maskeyle seç/bırak (Num +/−, `\|` ile hariç, regex, kayıtlı arama), ters çevir, aynı uzantı, her n'inci dosya; seçimi kaydet/geri yükle/dosyaya yaz; işlemden önceki seçime dön (Num /) | Y | **Planlı** | Ctrl+A, Shift/Ctrl, çerçeve var → 6 (rö 29) |
| 34 | Adları panoya kopyala: yalnız ad, tam yol, ayrıntılarla, yol + ayrıntı, UNC | Y | **Kısmen** | Yalnız Windows Shell menüsünde "Yol olarak kopyala" → 7 (rö 7) |
| 35 | Panel altı komut satırı: geçmiş, Ctrl+Enter ile ad ekleme, `cd`/`md`, `*` ile yönetici olarak, `cm_` komutları | Y | **Planlı** | 16 (rö 53) |
| 36 | Komut istemini burada aç | Y | **Planlı** | 7 (rö 16) |
| 37 | Düğme çubuğu (yatay + dikey), alt çubuklar (`.bar`), çubuğu açılır menü olarak açma, düğmeye dosya bırakma | Y | **Yok** | rakip-ozet'te önerilmeyenler (araç çubuğu düzenleyici) |
| 38 | Başlat menüsü (kullanıcı menüsü): alt menüler, kısayol, `%P %N %S %L %T`, `?` ile çalıştırmadan önce sorma, yönetici olarak | Y | **Kısmen** | `[[commands]]` sağ tıkta; alt menü, kısayol, soru, tüm seçim tek çağrıda yok → 6 (rö 30) |
| 39 | Kullanıcı komutları (`usercmd.ini`, `em_*`): parametreli, zincirlenebilir; kısayola, düğmeye, menüye, ilişkilendirmeye bağlanır | Y | **Kısmen** | `[[commands]]` var; kısayola bağlama ve zincirleme yok → 6 (rö 30) |
| 40 | ~550 dahili komut (`cm_*`), çoğu parametreli; hepsine kısayol atanabilir; "Choose command" aranabilir listesi | Y | **Kısmen** | 27 eylem atanabilir; komut paleti → 8 (rö 27) |
| 41 | Ana menüyü düzenleme (`wcmd_*.mnu`), menü/düğmelerde simge | Y | **Yok** | Önerilmeyenler (menü düzenleyici) |
| 42 | Dahili ilişkilendirmeler (yalnız TC'de): uzantıya göre Enter/F3/F4 için program ya da `cm_` komutu | Y | **Kısmen** | Sistem varsayılanı, `[archives] double-click`, türe göre komutlar; Enter tanımlanamaz |
| 43 | Ortam ve sahte değişkenler: `%COMMANDER_PATH%`, `%$DESKTOP%`, `%$CLIPBOARD%`, `%$CLIPNAME%`, `%$DATE+1Y%` | Y | **Kısmen** | `{in} {dir} {name} {ext} {out}`; pano ve tarih yer tutucusu yok → 6 (rö 30) |
| 44 | Komut satırı parametreleri: `/O`, `/T`, `/L= /R=`, `/S` (sekmede), `/P`, bağımsız arama `/S=F`, `/I` ini yolu | Y | **Kısmen** | Yalnız açılacak klasör → 9 (rö 43) |
| 45 | Tek tıkla açma modu | Y | **Planlı** | 7 (rö 42) |

### 1.4 Dosya işlemleri

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 46 | Kopyala/taşı/sil, Explorer ile iki yönlü sürükle-bırak ve pano | Y | **Var** | Üç sistemde |
| 47 | Arka plan aktarım yöneticisi: F2 Queue, elle kuyruk, sürükleyerek sıralama, duraklat, `sleep:` adımı, URL indirme | Y | **Kısmen** | Disk bazlı otomatik kuyruk, duraklat/iptal var; elle sıralama ve URL yok. rö 57 (adımı yok) |
| 48 | **Hız sınırı** (kopya, indirme, yükleme) | Y | **Yok** | rö 57 (adımı yok) |
| 49 | **Kopya sonrası doğrulama** (BLAKE3/MD5, disk önbelleği atlanarak) | Y | **Yok** | rö 57 (adımı yok) |
| 50 | Hız grafiği (ikinci ilerleme çubuğu), arka planda da | Y | **Kısmen** | Bayt ve öğe ilerlemesi, görev çubuğu; grafik yok |
| 51 | Üzerine yazma: yaz, atla, eskileri yaz, yeniden adlandır, **ekle (append)**, otomatik "(1)" (kopyalanan ya da hedef), "eskileri yeniden adlandır, yenileri atla", büyükse/küçükse kopyala; küçük resim, özel alan ve içerik karşılaştırma | Y | **Var** | Gezik: başlamadan tek liste, satır başına karar, ezilen çöpe; append, boyut ölçütü ve küçük resim yok |
| 52 | Kopyalarken **süzgeç** ("Only files of this type", `\|` hariç, klasör süzgeci, kayıtlı arama süzgeci) ve **ilişkili dosyalar** (`*.jpg>*.cr2`) | Y | **Yok** | Yeni rö 61 (öneri: 11) |
| 53 | Kopyalarken jokerle hedef adı (`*.bak`, `start_**_rest.*`) | Y | **Yok** | Yeni rö 61 |
| 54 | Seçili tüm hedef klasörlere kopyala; Branch View'dan göreli yollarla kopya | Y | **Yok** | Yeni rö 61 |
| 55 | NTFS izinlerini kopyalama, seyrek (sparse) dosya, klasör tarihlerini koruma | Y | **Kısmen** | Zaman ve öznitelik korunur; ACL seçeneği ve sparse yok |
| 56 | Yönetici olarak işlem (`tcmadmin.exe`) | Y | **Planlı** | 9 (rö 38) |
| 57 | Çöpe at / kalıcı sil (Shift ile ters) | Y | **Var** | Gezik'te kalıcı silme anında ve çökmeye dayanıklı |
| 58 | Yeni klasör F7 (çoklu ad, sayaçlı şablon `<1-3>ad`), yeni dosya Shift+F4 (şablon, çoklu `a\|b\|c`) | Y | **Kısmen** | Tek yeni klasör, boş `.txt` → 7 (rö 47) |
| 59 | Kısayol (.lnk), sembolik ve sert bağlantı oluşturma | Y | **Planlı** | 7 (rö 15) |
| 60 | Öznitelik ve zaman damgası değiştirme (toplu, alt klasörlerle, milisaniye, eklenti alanlarıyla) | Y | **Planlı** | 9 (rö 26) |
| 61 | Yerinde yeniden adlandırma (Shift+F6) | Y | **Var** | F2 |
| 62 | Klasör karşılaştır (Shift+F2): yenileri ve tek tarafta olanları seç; "Mark newer, hide same" | Y | **Planlı** | 11 (rö 35) |

### 1.5 Multi-Rename Tool

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 63 | Ad ve uzantı maskesi, yer tutucular (`[N]`, `[E]`, `[C]` sayaç, `[YMD]`, `[P]` üst klasör, `[N2-5]` parça), canlı sonuç listesi | Y | **Var** | 5a: kural tabanlı, canlı önizleme, sürükleyerek sıralama |
| 64 | Ara/değiştir (çoklu `\|`, regex, yalnız ilk), harf dönüşümü, kelime başı (İngilizce başlık istisnaları) | Y | **Var** | Türkçe i/İ doğru |
| 65 | Eklenti alanları yer tutucusu (`[=exif.DateTaken]`, ID3), MP4/HEIC kayıt tarihi `[T4]`, panodan metin `[X]` | Y + E | **Kısmen** | EXIF çekim tarihi var; ID3, video tarihi, pano yok. Yeni rö 66 (öneri: 13) |
| 66 | Adları metin dosyasından yükleme / dış düzenleyicide düzenleme | Y | **Kısmen** | Önizlemede elle düzenleme var; dosyadan yükleme yok |
| 67 | Ayar setlerini kaydetme, geri alma (`<UNDO>`), çakışmada otomatik "(2)" | Y | **Var** | Kayıtlı setler, tek Ctrl+Z, döngü güvenli (a↔b) |
| 68 | Adda `\` ile alt klasöre taşıma (yoksa oluşturur) | Y | **Yok** | Küçük iş; adımı yok |
| 69 | Branch View ya da arama sonuçlarındaki dosyalarla toplu ad | Y | **Planlı** | 8 (rö 51, düz görünüm) |

### 1.6 Karşılaştırma ve eşitleme

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 70 | **Synchronize Dirs**: alt klasörler, içerikle, tarihi yok say (FAT için 1-3 sn tolerans), simetrik ya da **asimetrik** (yedek: sağda fazla olan silinir), boş klasörler, göster düğmeleri (→ = ≠ ← tekler/çiftler), satır başına yön oku, sağ tık toplu yön, yalnız seçililer, ayarları kaydet | Y | **Planlı** | 11 (rö 35) |
| 71 | Eşitleme hedefleri: klasör↔arşiv, klasör↔FTP, klasör↔eklenti FS; FTP saat dilimi farkı, uzak checksum (XCRC/XMD5) | Y | **Planlı** | 11 klasör↔klasör; uzak taraf 18 |
| 72 | Eşitlemede süzgeç kipleri (1x, 2x, ←, →) ve içerik eklentisiyle özel karşılaştırma | Y + E | **Yok** | |
| 73 | Compare by content: yan yana, metin ve ikili (hex), düzenleme kipi, satır kopyalama, geri al, boşlukları ve sık satırları yok say, kodlama seçimi, elle yeniden hizalama | Y | **Planlı** | 11 (rö 54) |

### 1.7 Arama

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 74 | Find Files (Alt+F7): çoklu maske, `\|` hariç, `\**\`, klasör geçiş süzgeçleri, liste dosyası `@`; metin (regex, hex, kodlamalar, "içermeyen"), Office XML/EPUB içinde | Y | **Planlı** | 8 (rö 2) |
| 75 | Arşivlerin içinde ve arşivdeki dosyaların metninde arama | Y | **Planlı** | 8 + 16 (arşiv VFS) |
| 76 | Gelişmiş: tarih aralığı, yaş, boyut, öznitelik; eklenti alanlarıyla AND/OR kurallar | Y + E | **Planlı** | 8 (rö 2) |
| 77 | Yinelenen dosya bulucu (ad/boyut/içerik/eklenti alanı) + seçme diyaloğu (en eski/en yeni, klasöre göre, "her grupta en az biri kalsın") | Y | **Planlı** | 11 (rö 52) |
| 78 | Feed to listbox (sonuçlar panelde işlenir), sonuçlar içinde arama, sonuç geçmişi, ayrı süreçte arama, kayıtlı aramalar (kopya süzgeci olarak da) | Y | **Planlı** | 8 (rö 2, 49) |
| 79 | Everything bütünleşmesi (`ev:`, `ed:`; klasör boyutları) | Y + dış program | **Yok** | rakip-ozet açık karar 7 |
| 80 | **Branch View** (Ctrl+B; seçili klasörlerle Ctrl+Shift+B; imleçteki dosyanın klasörüne dönüş) | Y | **Planlı** | 8 (rö 51) |

### 1.8 Lister ve Quick View

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 81 | Lister (F3): her boyutta dosya; metin/ikili/hex/Unicode/HTML/RTF; kodlama menüsü; arama (regex, hex); yakınlaştırma; yazdırma | Y | **Kısmen** | Önizleme ve hızlı bakış yalnız ilk 64 KB metin → 13 (rö 13) |
| 82 | Lister'da resim ve medya oynatıcı (Windows kodekleri, çalma listesi, kapak) | Y | **Planlı** | 13 (rö 13) |
| 83 | Lister eklentileri (WLX): sözdizimi, PDF, Office, CAD, SQLite, DBF | E | **Yok** | Eklenti API'si önerilmedi; Önizleme 2 bir kısmını karşılar |
| 84 | Quick View paneli (Ctrl+Q): karşı panelde Lister; ayrı Quick View penceresi | Y | **Kısmen** | Sağ önizleme paneli (Alt+P) ve Boşluk ile hızlı bakış var; karşı panelde değil (10'dan sonra) |
| 85 | Çoklu Lister penceresi, N/P ile sonraki dosya | Y | **Kısmen** | Hızlı bakışta oklarla gezilir; çoklu pencere yok |
| 86 | F4 ile tanımlı düzenleyici (Lister'dan da) | Y | **Kısmen** | Sistemin varsayılanı; ayrı düzenleyici ayarı yok (`[[commands]]` ile yapılır) |

### 1.9 Arşivler

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 87 | Arşivi klasör gibi açma; içinden kopyala, görüntüle, sil; F5 ile arşive ekleme; arşivden arşive kopya | Y | **Kısmen** | Açma, ekleme ve üstüne sürükleme var; içinde gezinme ve silme → 16 (rö 12) |
| 88 | Yerleşik paketleyiciler: ZIP (libdeflate, AES), 7z (dahili 7-Zip DLL), TAR, GZ, TGZ | Y | **Var** | Gezik: zip/7z/tar.*, AES, ad şifreleme |
| 89 | Yerleşik açıcılar: ZIP, 7Z, RAR, ARJ, LZH, TAR, GZ, BZ2, XZ, ZSTD, Brotli, CAB, ACE | Y | **Var** | Gezik'te iso, deb, cpio da içeride; nadirler indirilen 7-Zip ile |
| 90 | Harici paketleyiciler (RAR, ARJ, ACE, LHA) ve WCX eklentileri (ISO, MSI, RPM, disk imajları, Total7zip) | Y + E | **Kısmen** | Yalnız 7-Zip; RAR oluşturma bilerek dışarıda |
| 91 | SFX arşiv (`.7z` → `.exe` yeniden adlandırınca önerir) | Y | **Yok** | Önerilmeyenler (SFX) |
| 92 | Arşivleri sına; "Unpack and verify"; alt klasörlerdeki arşivleri toplu açma | Y | **Kısmen** | Açarken bütünlük ve güvenlik denetlenir; ayrı "sına" komutu yok |
| 93 | Çok parçalı arşiv oluşturma ve açma | Y | **Kısmen** | 7z parçalı var; zip bölme bilerek dışarıda |

### 1.10 FTP, SFTP, eklenti dosya sistemleri

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 94 | Yerleşik FTP/FTPS istemcisi: bağlantı yöneticisi (klasörlü), proxy/SOCKS, FXP, sürdürme, arka planda indirme, listeden indirme, IPv6 | Y | **Planlı** | 18 (rö 24) |
| 95 | SFTP/SCP, WebDAV, bulut (OneDrive, Dropbox, Google Drive, Box, Yandex, HiDrive) | Eg | **Planlı** | 18 (rö 24) |
| 96 | HTTP URL indirme (transfer yöneticisine yapıştır) | Y | **Yok** | |
| 97 | Dosya sistemi eklentileri (WFX): kayıt defteri, süreçler, servisler, ext2/4, kamera | E | **Yok** | Eklenti API'si önerilmedi |
| 98 | Paralel/USB kablo ile iki PC arası bağlantı | Y | **Yok** | Eski; önerilmez |

### 1.11 Araçlar

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 99 | Sağlama toplamı oluştur/doğrula: CRC32 (SFV), MD5, SHA1, SHA2, SHA3, BLAKE3; tek dosya, dosya başına ya da klasör başına; panodan doğrulama; "yalnız hatalar" | Y | **Planlı** | 11 (rö 31) |
| 100 | Dosya böl/birleştir (`.001`…, CRC dosyası, otomatik birleştirme) | Y | **Kısmen** | 7z parçalı arşiv; açmada `.001` parçaları tanınır; ham bölme yok. Yeni rö 62 |
| 101 | Kodla/çöz (MIME/Base64, UUE, XXE, BinHex) | Y | **Yok** | Önerilmez |
| 102 | Dosya listesi yazdırma (alt klasörlerle), ayrıntılarla panoya kopyalama | Y | **Planlı** | 11 (rö 60) |
| 103 | Sistem bilgisi, birim etiketi | Y | **Yok** | Önerilmez |
| 104 | İşlem günlüğü dosyası | Y | **Kısmen** | "N failed · Details" → 7 (rö 55) |

### 1.12 Özelleştirme, kurulum, platform

| # | Total Commander özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 105 | Grafik ayar penceresi (çok sayfalı) ve `wincmd.ini`'yi doğrudan düzenleme | Y | **Kısmen** | Canlı TOML; grafik pencere → 14 (rö 5) |
| 106 | Taşınabilir/USB kullanım: ini program klasöründe (`/I".\"`), `tc2usb`, PortableApps | Y | **Kısmen** | `GEZIK_CONFIG_DIR`; paket yok. Yeni rö 65 (Yayın hazırlığı) |
| 107 | Sistem tepsisine küçültme, tek örnek çalıştırma | Y | **Planlı** | 9 (rö 37) |
| 108 | Android sürümü (ücretsiz): iki panel, zip, arama, Multi-rename, içerik karşılaştırma, medya oynatıcı, düzenleyici; FTP, WebDAV, LAN, WiFi Direct, bulut eklentileri | A | **Yok** | Gezik masaüstü uygulaması; mobil hedef değil |
| 109 | Dört eklenti türü (WCX, WFX, WLX, WDX), açık SDK, yüzlerce eklenti | Y + E | **Yok** | rakip-ozet açık karar 14 |

### 1.13 Yerleşik ve eklentiyle gelenler

Yerleşik olanlar: iki panel, sekmeler, hotlist, geçmiş, düğme çubuğu, başlat menüsü, `em_`/`cm_` komutları, Quick Search/Filter, komut satırı, aktarım yöneticisi, hız sınırı, doğrulama, Multi-Rename, Synchronize Dirs, içerik karşılaştırma, Find Files (arşivlerde, yinelenenler), Branch View, Lister, Quick View, ZIP/7z/TAR/GZ paketleme, 13 biçim açma, FTP/FTPS, küçük resimler, özel sütunlar (`tc` alanları), checksum, böl/birleştir, kodla/çöz, yazdırma, koyu mod, taşınabilir kurulum.

Eklentiyle gelenler: SFTP, WebDAV ve bulut (Ghisler'in eklentileri); EXIF, ID3, medya, PDF metni gibi meta veri alanları (WDX); sözdizimi renklendirme, PDF/Office görüntüleme (WLX); ISO, MSI, RPM, disk imajları (WCX); kayıt defteri, süreçler, ext4 (WFX). TC'nin "meta veri her yerde" gücü büyük ölçüde üçüncü taraf WDX eklentilerine dayanır; yerleşik `tc` alanları ad, boyut, tarih ve öznitelikle sınırlıdır.

### 1.14 Lisans

TC shareware: 30 gün deneme süresi var, deneme sürümü tam işlevlidir ama kayıt olunmamışsa açılışta numaralı düğmeye basılması istenen bir hatırlatma ekranı gösterir. Lisans 42 € + KDV (öğrenciye 31,50 €). Lisans eşzamanlı kullanıcı başınadır ve kalıcıdır. Kayıtlı kullanıcıya bütün güncellemeler 1.0'dan beri ücretsiz. Ticari kullanım aynı lisansla serbest. Android sürümü ücretsiz ve reklamsız. Gezik PolyForm Noncommercial: kişisel kullanım ücretsiz, **ticari kullanım için hiç yol yok**.

---

## 2. Gezik'te olup Total Commander'da olmayanlar

| Alan | Gezik | Total Commander |
|---|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından; ayar dosyası sistemler arası aynen taşınır | Yalnız Windows (95 → 11). Mac için emülatör ya da başka uygulama öneriyor; Linux sürümü yok |
| Geri alma | Kopyala, taşı, ad, yeni öğe, çöp, ezme, toplu ad, arşiv, dönüştürme, komut; oturum boyu tek Ctrl+Z | Yalnız Multi-Rename'in son işlemi (`<UNDO>`); dosya işlemlerinde geri alma yok |
| Çakışmalar | Başlamadan tüm çakışmalar tek listede, satır başına karar, ezilen dosya çöpe | Dosya dosya sorar; "hepsine" seçenekleri ve ön ayar var |
| Kalıcı silme | Anında (gizli ada çevir, arka planda sil, çökmede açılışta tamamla) | Klasik silme; büyük ağaçta bekletir |
| Dönüştürme | Resim (JPEG/PNG/WebP/AVIF/HEIC, EXIF döndürme, konum silme), metin kodlaması ve satır sonu, ses/video (ffmpeg) | Yok (Lister'da yalnız kodlama seçerek görüntüleme) |
| PDF (5d) | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'ten resim | Yok |
| Araç indirme | 7-Zip, ffmpeg, pdfium tek tıkla, SHA-256 sabitli | Yok (unrar ve 7z DLL'leri pakette) |
| Arşiv güvenliği | Zip-slip ve zip bombası denetimi, açma önce aşama klasörüne, tek Ctrl+Z | Şifreli ad sızıntısı 11.58'de kapatıldı; bomba denetimi belgelenmemiş |
| Toplu ad | Önizlemede elle düzeltme, sürükleyerek sıralama, döngü güvenli (a↔b), Türkçe i/İ | Çok güçlü (yer tutucular, eklenti alanları); Türkçe büyük/küçük harf ayrıca belgelenmemiş |
| Kullanıcı komutları | Kabuksuz argüman dizisi, güvenli yer tutucular, paralel, çıktısı geri alınabilir | Kabukla çalışır; geri alma yok |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı yenileme, satır numaralı hata | INI; bir kısmı yeniden başlatma ister |
| Sürükle-bırak | Sekmeye, breadcrumb'a, sabitlenenlere; bırakma da geri alınır | Var (geri alınmaz) |
| Yol haritasında | Komut paleti, Git, şifreli ayar senkronu | Üçü de yok |

Performans: TC de yerel kodla yazılmış, küçük ve hızlı (64 bit Lazarus, 32 bit Delphi). Karşılaştırmalı ölçüm yapılmadı; burada üstünlük iddia edilmiyor.

---

## 3. Sayım

| Durum | Sayı |
|---|---|
| Var | 12 |
| Kısmen | 36 |
| Yok | 20 |
| Planlı | 41 |
| **Toplam** | **109** |

Yalnız numaralı satırlar sayıldı (1-109). Bölüm 1.13, 1.14 ve 2 sayıma girmedi. "Planlı" satırların çoğu bugün **Yok**tur; yalnızca önerilen yol haritasında bir adımda yer alırlar.

---

## 4. Öneri

### 4.1 Değer / emek sırası

Değer: TC'den gelen ya da klavyeyle çalışan bir kullanıcının günlük işine etkisi. Emek: Gezik'in bugünkü altyapısına göre tahmin (D düşük, O orta, Y yüksek).

| Sıra | Özellik (satır) | Durum | Değer | Emek | Adım |
|---|---|---|---|---|---|
| 1 | Quick Search seçenekleri + Quick Filter + liste süzgeci (26, 31, 32) | Kısmen/Planlı | Çok yüksek | D | 6 Klavye paketi |
| 2 | Find Files + Feed to listbox + Branch View + kayıtlı aramalar (74-78, 80, 69) | Planlı | Çok yüksek | O | 8 Arama |
| 3 | Klasör boyutu (Boşluk ile seçili, tümü, otomatik) (27) | Planlı | Yüksek | O | 8 |
| 4 | Seçim araçları: maske, ters çevir, seçimi kaydet ve işlemden önceki seçime dön (33) | Planlı | Yüksek | D | 6 |
| 5 | Terminalde aç + adları/yolları biçimleriyle kopyala (34, 36) | Kısmen/Planlı | Yüksek | D | 7 Günlük kolaylıklar |
| 6 | Çift panel + F5/F6 karşı panel + ağaç + eşli gezinme (1, 2, 11, 12) | Planlı | Yüksek | Y | 10 Düzenler |
| 7 | Klasör geçmişi listesi + sık kullanılan klasörler + hotlist alt menüleri/kısayolları (7, 8) | Kısmen | Orta-yüksek | D | 6 (geçmiş), 7 (hotlist) |
| 8 | Synchronize Dirs + klasör karşılaştır (62, 70, 71) | Planlı | Orta-yüksek | Y | 11 Araçlar |
| 9 | Sağlama toplamları + kopya sonrası doğrulama (99, 49) | Planlı/Yok | Orta | D | 11 (doğrulama da 11'e alınmalı; rö 57) |
| 10 | Lister: tam dosya, hex, arama, kodlama seçimi (81) | Kısmen | Orta | O | 13 Önizleme 2 |
| 11 | İçerik karşılaştırma (73) | Planlı | Orta | O | 11 |
| 12 | Yinelenen dosyalar + "her grupta biri kalsın" seçimi (77) | Planlı | Orta | O | 11 |
| 13 | Başlat menüsü/kullanıcı komutu büyütme: kısayol, alt menü, `?` soru, `%L` liste, pano/tarih değişkenleri (38, 39, 43) | Kısmen | Orta | D | 6 |
| 14 | Kopya süzgeci, ilişkili dosyalar ve jokerli hedef adı (52-54) | Yok | Düşük-orta | O | 11 (yeni rö 61) |
| 15 | Meta veri alanlarını her yerde kullanma: sütun, arama kuralı, ad şablonu (18, 65, 76) | Planlı/Kısmen | Orta | O | 13 (yeni rö 66) |
| 16 | Otomatik görünüm kuralları (19) | Kısmen | Orta | O | 10 (yeni rö 63) |
| 17 | Hız sınırı, elle kuyruk sırası (47, 48) | Kısmen/Yok | Düşük | O | 11 (rö 57) |
| 18 | Taşınabilir paket (106) | Kısmen | Orta | D | Yayın hazırlığı (yeni rö 65) |
| 19 | Kilitli sekmeler, sekme dosyaları (4, 6) | Yok/Planlı | Düşük-orta | D | 6 / 7 (yeni rö 64) |
| 20 | Dosya böl/birleştir (100) | Kısmen | Düşük | D | 11 (yeni rö 62) |

**İlk 10 (tek satır):** 1 hızlı filtre ve Quick Search seçenekleri · 2 Find Files + Feed to listbox + Branch View · 3 klasör boyutu · 4 seçim araçları ve seçimi geri yükleme · 5 terminal + ad/yol kopyalama · 6 çift panel ve F5/F6 modeli · 7 geçmiş listesi, sık klasörler, hotlist alt menüleri · 8 Synchronize Dirs · 9 sağlama toplamı + kopya doğrulama · 10 tam dosya Lister.

### 4.2 Diğer yedi notta olmayan TC fikirleri

| Fikir | Neden ilginç | Gezik'e uyarlama |
|---|---|---|
| **Synchronize Dirs ayrıntıları** | Diğer notlar "klasör eşitleme" diye tek satır geçiyor. TC'de asimetrik (yedek) kip, satır başına değiştirilebilir yön oku, "eşit / farklı / yalnız solda / yalnız sağda" göster düğmeleri, 1x/2x/←/→ süzgeç kipleri, FAT için saniye toleransı, kayıtlı eşitleme profilleri, klasör↔arşiv eşitleme var | Gezik'in çakışma listesi zaten satır başına karar veriyor. Eşitleme ekranı da aynı listenin üç sütunlu (sol / yön / sağ) hali olabilir. Kopya motoru ve geri alma hazır. Profiller `settings.toml`'da `[[sync]]` olarak tutulabilir. Asimetrik kipte silinenler çöpe gider (TC'de bu güvence yok) |
| **Branch View + göreli yolla kopya** | Düz görünüm FP ve DO'da da var (rö 51), ama TC onu araçlarla birleştiriyor: Multi-Rename, kopya "keep relative paths" ve Ctrl+B'de imleçteki dosyanın klasörüne dönüş | 8 Arama'da düz görünüm, arama sonucu ile aynı "sanal liste" modeli olsun. Toplu ad ve kopya bu listeyle çalışsın |
| **İçerik eklentisi alanları (WDX) tek kayıtta** | Bir alan (ör. `exif.DateTaken`, ID3 sanatçı) tanımlanınca sütunda, aramada kural olarak, toplu ad şablonunda, eşitlemede karşılaştırıcı olarak, renk kuralında, küçük resim altında ve çakışma penceresinde kullanılabiliyor | Eklenti API'si olmadan da olur: `gezik-core`'da tek bir **meta veri alanı kaydı** (EXIF, ID3, PDF sayfa sayısı, video süresi; ffmpeg/pdfium hazır). Sütun (13), arama kuralı (8), ad şablonu (5a'yı genişletir) ve renk kuralı (12) hep aynı kaydı okur |
| **Otomatik görünüm kuralları** | Yol, konum türü (arşiv, arama sonucu, ağ) ya da içerik oranı ("çoğu resim") → sütun seti, sıralama, arka plan rengi. Gezik'in klasör başına hafızasından bir adım ötesi | `views.toml`'a kural listesi: önce kural, sonra klasör hafızası. "Çoğu resimse ızgara" kuralı tek başına çok kullanıcıya değer |
| **Kopya süzgeci ve ilişkili dosyalar** | `*.jpg>*.cr2`: yalnız JPEG'i olan RAW'ları da kopyala. Kayıtlı arama süzgeç olarak kullanılabiliyor | 8'deki arama ölçütleri kopya diyaloğunda süzgeç olarak yeniden kullanılsın |
| **Sık kullanılan klasörler** | Geçmiş, ziyaret sayısına göre sıralanır ve 30 günde bir yarılanır. Geçmiş penceresi aranabilir | Geçmiş listesine (6) sıklık sütunu; aynı veri yol tamamlamayı da besler |
| **Kilitli sekme "ama gezilebilir"** | Sekme bir projenin köküne kilitlenir; içinde gezilebilir, sekmeye dönülünce köke geri gelir | Sekme kolaylıklarına (6) küçük bir ek |
| **Yinelenen seçim kuralları** | "Her grupta en az biri kalsın", "en eskiyi bırak", "yedek klasöründekileri seç" | 11'deki yinelenen bulucunun asıl değeri bu seçim diyaloğu; silme zaten çöpe gider |
| **Üzerine yazma seçenekleri** | Ekle (append), "eskileri yeniden adlandır, yenileri atla", "büyükse kopyala" | Çakışma listesine iki ek karar seçeneği ("boyuta göre", "eskiyi yeniden adlandır") |
| **Kuyrukta `sleep:` ve hız sınırı** | Ağ paylaşımında arka plan kopyası işi boğmasın | rö 57'yi 11 Araçlar'a almak için ikinci bir gerekçe |

**Alınmaması önerilenler:** Düğme çubuğu ve menü düzenleyici (önerilmeyenler listesiyle aynı gerekçe), dört türlü eklenti API'si (açık karar 14; meta veri alanı kaydı en değerli kısmı karşılar), kodla/çöz (UUE/XXE/BinHex), SFX, paralel/USB kablo bağlantısı, sistem bilgisi, yerleşik FTP (uzak bağlantılar 18'de SFTP ile başlamalı, düz FTP sona kalmalı).

---

## Kaynaklar

Total Commander'ın kendi sitesi:

- https://www.ghisler.com/ (ana sayfa; 11.58, 2026-07-01)
- https://www.ghisler.com/featurel.htm (özellik listesi)
- https://www.ghisler.com/whatsnew.htm
- https://www.ghisler.com/history.txt (11.00 → 11.58 "Added" satırları okundu)
- https://www.ghisler.com/download.htm (32/64 bit, Mac notu, Android)
- https://www.ghisler.com/order.htm (42 €, 30 gün, öğrenci indirimi)
- https://www.ghisler.com/efaqorder.htm (eşzamanlı kullanıcı lisansı, ücretsiz güncellemeler)
- https://www.ghisler.com/usbinst.htm (`tc2usb`, PortableApps)
- https://www.ghisler.com/addons.htm (eklenti türleri, standart pakette 19 dil)
- https://www.ghisler.com/plugins.htm (resmi eklenti listesi: WCX, WFX, WLX, WDX)
- https://www.ghisler.com/android.htm (Android 3.62)
- https://www.ghisler.com/faq.htm
- https://totalcommander.ch/1158/tcmd1158x64.exe → `INSTALL.CAB` → `e\TOTALCMD.CHM` (İngilizce yardım, 11.58). Okunan sayfalar: `menu_files`, `menu_mark`, `menu_show`, `menu_net`, `menu_fileoperations`, `menu_fileactions`, `menu_configuration`, `menu_starter`, `options_mark`, `dlg_search`, `dlg_advancedsearch`, `dlg_searchplugins`, `dlg_duplicates`, `dlg_synchronize_dirs`, `synchronize_dirs`, `options_branch_view`, `compare_by_content`, `dlg_compare_contents`, `options_compare`, `proc_copy`, `options_background_transfer_manager`, `dlg_overwrite`, `dlg_configcopydelete`, `foldertabs`, `dlg_configfoldertabs`, `directoryhotlist`, `dlg_quicksearch`, `dlg_configquicksearch`, `show_customcolumns`, `show_customviewmodes`, `dlg_configviewmode`, `dlg_configautomodeswitch`, `def_colors_by_file_type`, `dark___normal`, `quick_view`, `show_separatetree`, `show_tree`, `dlg_configthumbs`, `listercontents`, `cmdline`, `dlg_usermenu`, `dlg_createsfv`, `dlg_filessplit`, `dlg_filesencode`, `options_print`, `multi_rename_tool`, `dialog_box___multi_rename_tool`, `dlg_configpack`, `dialog_box__connect`, `ftp_menu`, `dlg_config2`, `whats_new`

Destekleyici:

- https://ghisler.com/screenshots/en/08.html (Synchronize Dirs ekran görüntüsü)
