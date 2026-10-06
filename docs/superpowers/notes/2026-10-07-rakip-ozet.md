# Rakip karşılaştırmalarının özeti ve önerilen yol haritası

- **Tarih:** 2026-10-07
- **Kaynak notlar** (hepsi bu klasörde, 2026-10-07): OneCommander (**OC**), Files (**Fi**), Directory Opus (**DO**), File Pilot (**FP**), Far Manager + far2l (**Far**), ForkLift 4 (**FL**), Path Finder (**PF**).
- **Gezik tabanı:** `master` (29dcb45) + 5d (PDF) var sayıldı.
- **Adım numaraları:** Notlar adımları farklı numaralıyor. Far notu 6 Etiketler, 7 Taşınabilirlik, 8 Bulut, 9 Gelişmiş diyor. Diğerleri ad kullanıyor. Ayarlar spec'inin §8'inde ise 5 Taşınabilirlik, 6 Bulut, 7 Gelişmiş yazıyor. Bu belgede hepsi şu listeye çevrildi:
  - **Bitenler:** 1 Ayarlar + tema, 2 Gezinme, 3 Görünüm, 4 Dosya işlemleri, 5 Toplu işlemler (5a ad, 5b arşiv, 5c dönüştürme + komutlar, 5d PDF).
  - **Kalanlar:** *Taşınabilirlik* (dışa/içe aktarma, senkron klasör, GitHub'dan tema), *Bulut senkronu* (ayarların senkronu; bulut sürücüleri değil), *Gelişmiş* (arama, çift panel, etiketler, Git, komut paleti, arşivin içinde gezinme).
- **Tabloda "Planlı":** bugünkü yol haritasında var demek. **Yok** ya da **Kısmen** ise yol haritasında da yok demek.

---

## 1. Eksik ve kısmi özellikler: birleşik tablo

Benzer satırlar tek satırda birleştirildi. Örneğin "yazarken süz", "hızlı filtre", "panel filtresi", "Filter Bar" ve "bulanık süzme" tek satır oldu. Değer ve emek, notlardaki tahminlerin ortak paydası. Emek, Gezik'in bugünkü altyapısına göre tahmin edildi. Sıralama: önce kaç uygulamada olduğu, sonra değer, sonra emek (düşük emek önce).

| # | Özellik | OC | Fi | DO | FP | Far | FL | PF | Σ | Gezik | Değer | Emek | Adım |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Yazarken süzme / hızlı filtre (joker, `!` ile hariç, sonraki eşleşme, kayıtlı filtre) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 7 | Kısmen (yalnız harfle atlama) | Yüksek | Düşük | Klavye paketi |
| 2 | Özyinelemeli arama (ad, içerik, tarih/boyut; sonuçta önizleme) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 7 | Planlı | Yüksek | Orta | Arama |
| 3 | Klasör boyutu (arka planda, sütun, sıralama; sıralamayı bekletmeden) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 7 | Yok | Yüksek | Orta | Arama |
| 4 | Çift panel + karşı panele kopyala/taşı (F5/F6, Tab, sekmeyi diğer panele taşı) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 7 | Planlı | Yüksek | Yüksek | Düzenler |
| 5 | Grafik ayar penceresi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 7 | Kısmen (yalnız TOML) | Yüksek | Yüksek | Taşınabilirlik |
| 6 | Sürücü çıkarma, ağ paylaşımına bağlanma/eşleme, ağ keşfi | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 7 | Kısmen (liste var) | Orta | Orta | Sistem bütünleşmesi |
| 7 | Yolu kopyala: kısayol, biçimler (tırnaklı, `/`, UNC), üç sistemde | ✓ | ✓ | | ✓ | ✓ | ✓ | ✓ | 6 | Kısmen (yalnız Windows Shell menüsü) | Yüksek | Düşük | Günlük kolaylıklar |
| 8 | Oturum geri yükleme, sekme setleri / workspaces / kayıtlı düzenler | ✓ | ✓ | ✓ | ✓ | | ✓ | ✓ | 6 | Yok (spec'te bilerek dışarıda) | Yüksek | Düşük | Günlük kolaylıklar |
| 9 | Arayüz çevirisi (önce Türkçe) | ✓ | ✓ | ✓ | | ✓ | ✓ | ✓ | 6 | Yok (bilerek dışarıda) | Yüksek | Orta | Yerelleştirme |
| 10 | Yol tamamlama, son klasörler, geçmiş açılır listesi, ortam değişkeni / `{home}` | ✓ | | ✓ | ✓ | ✓ | ✓ | ✓ | 6 | Kısmen (Ctrl+L) | Orta | Düşük | Klavye paketi |
| 11 | Görünüm seçenekleri: gizli/sistem ayrımı, uzantı gizleme, "klasörler önce" anahtarı, boyut ve tarih biçimi | ✓ | ✓ | ✓ | ✓ | ✓ | | ✓ | 6 | Kısmen | Orta | Düşük | Günlük kolaylıklar |
| 12 | Arşivin içinde klasör gibi gezinme, içinden sürükleme, arşivden silme | | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | 6 | Planlı | Orta | Orta | Gelişmiş |
| 13 | Zengin önizleme: video/ses, çok sayfalı PDF, kod renklendirme, resim yakınlaştırma, tam metin/hex | ✓ | ✓ | ✓ | | ✓ | ✓ | ✓ | 6 | Kısmen | Orta | Orta | Önizleme 2 |
| 14 | Varsayılan dosya yöneticisi olma (Win+E, `NSFileViewer`) | ✓ | ✓ | ✓ | ✓ | | ✓ | ✓ | 6 | Yok | Orta | Orta | Sistem bütünleşmesi |
| 15 | Kısayol / symlink / hardlink / junction oluşturma | ✓ | ✓ | ✓ | | ✓ | ✓ | ✓ | 6 | Yok (bilerek dışarıda) | Düşük | Düşük | Günlük kolaylıklar |
| 16 | Terminalde aç (terminal seçimi; yönetici olarak) | ✓ | ✓ | | | ✓ | ✓ | ✓ | 5 | Yok | Yüksek | Düşük | Günlük kolaylıklar |
| 17 | Otomatik güncelleme | ✓ | ✓ | ✓ | | | ✓ | ✓ | 5 | Planlı (ilk sürümden önce) | Yüksek | Orta | Yayın hazırlığı |
| 18 | Etiketler: renk, kenar çubuğu, etikete göre süz/sırala; macOS'ta Finder uyumlu | ✓ | ✓ | ✓ | | | ✓ | ✓ | 5 | Planlı | Yüksek | Orta | Etiketler |
| 19 | Favori grupları, takma ad, simge/renk, numaralı klasör kısayolu | ✓ | | ✓ | | ✓ | ✓ | ✓ | 5 | Kısmen (tek PINNED listesi) | Orta | Düşük | Günlük kolaylıklar |
| 20 | Gruplama (tür, tarih, boyut, etiket) | | ✓ | ✓ | ✓ | | ✓ | ✓ | 5 | Yok (bilerek dışarıda) | Orta | Orta | Düzenler |
| 21 | Meta veri paneli ve sütunları (EXIF, ID3, süre, boyutlar) | ✓ | ✓ | ✓ | | | ✓ | ✓ | 5 | Kısmen (piksel boyutu) | Orta | Orta | Önizleme 2 |
| 22 | Bulut sürücüleri: kenar çubuğunda otomatik, durum simgeleri, indir/boşalt | ✓ | ✓ | ✓ | | | ✓ | ✓ | 5 | Yok | Orta | Orta | Sistem bütünleşmesi |
| 23 | Çok pencere, sekmeyi koparıp yeni pencere | ✓ | ✓ | ✓ | | | ✓ | ✓ | 5 | Kısmen (ikinci süreç) | Orta | Orta | Düzenler |
| 24 | Uzak bağlantılar: SFTP, FTP(S), WebDAV, S3 | | ✓ | ✓ | | ✓ | ✓ | ✓ | 5 | Yok | Orta (azınlığa yüksek) | Yüksek | Uzak bağlantılar (karar) |
| 25 | Puan, yorum, klasör notu (`descript.ion`, `.2do`) | ✓ | | ✓ | | ✓ | ✓ | ✓ | 5 | Yok | Düşük | Düşük | Etiketler |
| 26 | Özellikler / Get Info: izin, sahip, öznitelik ve zaman damgası düzenleme | | ✓ | ✓ | | ✓ | ✓ | ✓ | 5 | Kısmen (Windows sistem penceresi) | Düşük | Orta | Sistem bütünleşmesi |
| 27 | Komut paleti / Quick Open / FAYT komut modu | | ✓ | ✓ | ✓ | | ✓ | | 4 | Planlı | Yüksek | Orta | Arama |
| 28 | Sekme kolaylıkları: kapatılanı geri aç, Ctrl+1…9, sekme seçici/arama | ✓ | ✓ | ✓ | ✓ | | | | 4 | Yok | Orta | Düşük | Klavye paketi |
| 29 | Desenle seç/bırak, ters çevir, aynı uzantıyı seç, önceki seçime dön | | | ✓ | | ✓ | ✓ | ✓ | 4 | Yok | Orta | Düşük | Klavye paketi |
| 30 | `[[commands]]` büyütme: kısayol, alt menü, `ask`, tüm seçim `{files}`, diğer panel, iki dosya | ✓ | | ✓ | | ✓ | ✓ | | 4 | Kısmen | Orta | Düşük | Klavye paketi |
| 31 | Sağlama toplamı (MD5/SHA; kopyala, karşılaştır, CSV) | | ✓ | ✓ | | | ✓ | ✓ | 4 | Yok | Orta | Düşük | Araçlar |
| 32 | Klasör ağacı (kenar çubuğunda ya da panel) | | ✓ | ✓ | ✓ | ✓ | | | 4 | Yok | Orta | Orta | Düzenler |
| 33 | Çöp kutusu görünümü: gez, geri yükle, boşalt | ✓ | ✓ | ✓ | | | | ✓ | 4 | Kısmen (Ctrl+Z ile geri) | Orta | Orta | Sistem bütünleşmesi |
| 34 | "Birlikte aç" listesi, Share/AirDrop, Services (macOS/Linux'ta) | ✓ | ✓ | | | | ✓ | ✓ | 4 | Kısmen (yalnız Windows) | Orta | Orta | Sistem bütünleşmesi |
| 35 | Klasör karşılaştırma ve eşitleme (tek/iki yönlü, kayıtlı senkron) | | | ✓ | | ✓ | ✓ | ✓ | 4 | Yok | Orta | Yüksek | Araçlar |
| 36 | Miller sütunları / Column View | ✓ | ✓ | | | | ✓ | ✓ | 4 | Yok | Orta | Yüksek | Düzenler |
| 37 | Sistem geneli kısayol, tepside bekleme, girişte başlama | ✓ | ✓ | ✓ | | | | ✓ | 4 | Yok | Düşük | Orta | Sistem bütünleşmesi |
| 38 | Yönetici yetkisiyle işlem (UAC, ayrıcalıklı yardımcı) | ✓ | | ✓ | | ✓ | | ✓ | 4 | Yok | Düşük | Orta | Sistem bütünleşmesi |
| 39 | Tarayıcı/e-postadan sanal dosya bırakma (file promise, `FileGroupDescriptor`) | ✓ | | | ✓ | | ✓ | | 3 | Yok | Orta | Orta | Sistem bütünleşmesi |
| 40 | Git: durum işaretleri, sütunlar, add/commit/push/pull | | ✓ | | | | ✓ | ✓ | 3 | Planlı | Orta (geliştiriciye yüksek) | Yüksek | Gelişmiş |
| 41 | Eşli gezinme (Sync Browsing, Paired Folders, SameFolder) | | | ✓ | | ✓ | ✓ | | 3 | Yok | Düşük | Düşük | Düzenler |
| 42 | Tek tıkla açma seçeneği | | ✓ | | ✓ | | | ✓ | 3 | Yok | Düşük | Düşük | Günlük kolaylıklar |
| 43 | Komut satırı seçenekleri (`-newtab`, `/select`, PATH'e ekleme) | ✓ | | ✓ | ✓ | | | | 3 | Kısmen (yalnız klasör) | Düşük | Düşük | Sistem bütünleşmesi |
| 44 | MTP / telefon / iOS aygıtları | ✓ | | ✓ | | | | ✓ | 3 | Planlı | Düşük | Yüksek | Gelişmiş |
| 45 | macOS sistem simgeleri, QuickLook küçük resimleri, sistem QL paneli | | | | | | ✓ | ✓ | 2 | Yok (macOS'ta hep Gezik simgesi) | Yüksek (macOS'ta ilk izlenim) | Orta | Sistem bütünleşmesi |
| 46 | Panodaki resmi/metni dosya olarak yapıştırma | ✓ | ✓ | | | | | | 2 | Yok (bilerek dışarıda) | Orta | Düşük | Günlük kolaylıklar |
| 47 | Yeni dosya şablonları; seçilenlerle yeni klasör | ✓ | ✓ | | | | | | 2 | Kısmen (boş `.txt`) | Orta | Düşük | Günlük kolaylıklar |
| 48 | Drop Stack / Shelf (geçici toplama alanı) | | ✓ | | | | | ✓ | 2 | Yok | Orta | Düşük | Günlük kolaylıklar |
| 49 | Kayıtlı aramalar (Smart Folder) | | | ✓ | | | | ✓ | 2 | Yok | Orta | Düşük | Arama |
| 50 | Vurgulama kuralları (maske → renk), sıralama grupları | | | ✓ | | ✓ | | | 2 | Kısmen (tür rengi) | Orta | Düşük | Etiketler |
| 51 | Düz görünüm (alt klasörler tek listede) | | | ✓ | ✓ | | | | 2 | Yok | Orta | Orta | Arama |
| 52 | Yinelenen dosya bulucu | | | ✓ | | | | ✓ | 2 | Yok | Orta | Orta | Araçlar |
| 53 | Gömülü terminal / panel altı komut satırı (adres çubuğunda `>`) | | | | | ✓ | | ✓ | 2 | Yok | Orta | Yüksek | Gelişmiş (sonra) |
| 54 | İki dosyayı dış diff aracıyla ya da yan yana karşılaştırma | | | | | | ✓ | ✓ | 2 | Yok | Düşük | Düşük | Araçlar |
| 55 | Görünür işlem günlüğü (log) | | | ✓ | | | ✓ | | 2 | Kısmen ("N failed · Details") | Düşük | Düşük | Günlük kolaylıklar |
| 56 | Listede ve kenar çubuğunda yaylı klasörler | | | | | | ✓ | ✓ | 2 | Kısmen (yalnız sekmede) | Düşük | Düşük | Günlük kolaylıklar |
| 57 | Kuyruk düzenleme, bant sınırı, kopya sonrası doğrulama | | | ✓ | | | ✓ | | 2 | Kısmen | Düşük | Orta | — (biriken) |
| 58 | Yerleştirilebilir paneller (PF modüllerinin hafif hali; `settings.toml`'da yerleşim listesi) | | | | | | | ✓ | 1 | Yok | Orta | Yüksek | Düzenler |
| 59 | Boyut haritası (Size Browser halka grafiği) | | | | | | | ✓ | 1 | Yok | Düşük | Orta | Araçlar |
| 60 | Klasör listesini dışa aktarma (metin/CSV/HTML) | | | ✓ | | | | | 1 | Yok | Düşük | Düşük | Araçlar |

**Notlarda önerilmeyenler** (tabloya alınmadı):

- **Betik dili, eklenti SDK'sı, makro kaydı** (DO, Far): yorumlayıcı gerektirir ve "hızlı ve hafif" hedefine terstir. Karşılığı satır 27 ve 30'dur.
- **Dahili düzenleyici** (Far, PF).
- **Güvenli silme** (DO, Far, PF): SSD'de anlamsız.
- **Süreç görüntüleyici** (Far, PF).
- **App Deleter** (FL).
- **Mica, saydamlık, arka plan resmi** (OC, Fi).
- **Araç çubuğu ya da menü düzenleyici** (OC, Fi, DO, FL, PF): emeği yüksek. Komut paleti ve kısayollar aynı ihtiyacı karşılar.
- **Ekran yakalama, uygulama başlatıcı, Subversion, SFX, masaüstünü devralma.**

---

## 2. Önerilen yeni yol haritası

İlke: hızlı ve değeri yüksek işler önce gelir. Ayar biçimini değiştiren işler Taşınabilirlik'ten önce yapılır, çünkü dışa aktarılan biçim o adımda dondurulur. Çift paneli gerektiren işler Düzenler'den sonra gelir.

| Sıra | Adım | Kapsam (tek satır) | Neden bu sırada |
|---|---|---|---|
| 6 | **Klavye paketi** | Hızlı filtre (1), yol tamamlama ve geçmiş (10), sekme kolaylıkları (28), desenle seçme (29), `[[commands]]` büyütme (30) | En ucuz işler; yedi uygulamanın hepsinde olan süzme bunların içinde. Far notuna göre yaklaşık tek PR. `Selection`, `Action` ve `EntryModel` hazır |
| 7 | **Günlük kolaylıklar** | Terminalde aç (16), yolu kopyala (7), oturum ve sekme setleri (8), favori grupları (19), görünüm seçenekleri (11), bağlantılar (15), şablonlar (47), panodan dosya (46), Drop Stack (48), tek tık (42), günlük (55) | Düşük emek, her gün hissedilir. `pinned` ve oturum biçimi Taşınabilirlik'ten önce kesinleşir |
| 8 | **Arama** | Özyinelemeli arama (2), klasör boyutu (3), komut paleti (27), düz görünüm (51), kayıtlı aramalar (49) | En büyük boşluk, yedi notun hepsinde 1. sırada. Arka plan tarayıcısı ve süzme kodu ortak. Palet de aynı süzme bileşenini kullanır |
| 9 | **Sistem bütünleşmesi** | macOS: sistem simgeleri, QuickLook, Open With/Share/Services, Get Info, Finder adları (45, 34, 26). Hepsi: bulut sürücüleri (22), çöp görünümü (33), sürücü ve ağ (6), varsayılan yönetici (14), sanal dosya bırakma (39), CLI (43). Windows'ta karşılıkları | macOS kullanıcısının ilk dakikada gördüğü eksikler. Platform testlerinden (macOS, Linux) hemen sonra gelmesi doğal. Etiketler'in Finder uyumu buna dayanır |
| 10 | **Düzenler** | Çift panel ve karşı panel işlemleri (4), klasör ağacı (32), gruplama (20), çok pencere (23), eşli gezinme (41). Sonra Miller (36) ve yerleştirilebilir paneller (58) | Değeri yüksek, emeği de yüksek. Önceki adımlardaki seçim, süzme ve oturum altyapısının üstüne kurulur |
| 11 | **Araçlar** | Klasör karşılaştırma ve eşitleme (35), sağlama toplamı (31), yinelenen dosyalar (52), dosya diff (54), boyut haritası (59), liste dışa aktarma (60) | Karşılaştırma için çift panel gerekir. Kopya motoru, çakışma listesi ve geri alma hazır |
| 12 | **Etiketler** | Renk etiketleri (18), puan ve yorum (25), vurgulama kuralları (50); macOS xattr ile Finder uyumlu | Sistem bütünleşmesinden sonra gelir, böylece macOS'ta yerel veriyle başlar. Arama ve gruplama etiketi hemen kullanır |
| 13 | **Önizleme 2** | Video/ses, çok sayfalı PDF, renklendirme, yakınlaştırma, hex ve tam metin (13), meta veri paneli ve sütunları (21) | ffmpeg ve pdfium 5c/5d'den hazır. Değeri orta, kendi başına bağımsız |
| 14 | **Taşınabilirlik** | Dışa/içe aktarma, senkron klasör, GitHub'dan tema, grafik ayar penceresi (5) | Ayar biçimi değiştiren adımlardan sonra gelir. Ayar penceresi teknik olmayan kullanıcının kapısı ve dışa/içe aktarma da arayüz ister |
| 15 | **Yerelleştirme** | Metinleri dışarı çıkarma, önce Türkçe | İlk herkese açık sürümden önce olmalı. Ayar penceresinden sonra gelirse onun metinleri de bir kerede çevrilir |
| — | **Yayın hazırlığı** | Paketler (MSI/zip, `.app` imza + notarization, Linux), otomatik güncelleme (17) | İlk herkese açık sürümün kapısı (gezinme spec'indeki "Güncelleme") |
| 16 | **Gelişmiş** | Arşivin içinde gezinme (12), Git (40), MTP (44), adres çubuğunda komut (53) | Arşiv içi için kurulacak VFS katmanı MTP'nin ve olası uzak bağlantıların da temeli. Git yalnız üç rakipte var |
| 17 | **Bulut senkronu** | Ayarların şifreli, hesapsız cihazlar arası senkronu | Senkron klasör (14) ihtiyacın çoğunu karşılar. Sunucu işletme yükü var. Yedi rakibin hiçbirinde yok, yani acil değil ama ayırt edici |
| 18 | **Uzak bağlantılar** *(açık karar)* | SFTP, sonra WebDAV/S3; kimlik bilgileri sistem anahtarlığında; uzakta düzenle-yükle | En büyük iş. FL, DO ve Far kullanıcısı için önemli. Ya Gelişmiş'in VFS'i üstüne ayrı adım olur ya da spec'te "bilerek kapsam dışı" yazılır |

---

## 3. Gezik'in ayırt edici yanları

Yedi notta da tekrar eden üstünlükler:

| Alan | Gezik | Rakiplerde |
|---|---|---|
| Platform | Windows, macOS, Linux tek kod tabanından; ayar dosyası sistemler arası aynen taşınır | OC, Fi, DO, FP yalnız Windows; FL, PF yalnız macOS; Far yalnız Windows (far2l ayrı bir çatal) |
| Geri alma | Kopyala, taşı, ad, yeni öğe, çöp, ezme, toplu ad, arşiv, dönüştürme, komut; oturum boyu tek Ctrl+Z | FP ve Far'da yok; diğerlerinde dar ya da belgelenmemiş |
| Çakışma listesi | İş başlamadan tüm çakışmalar tek listede, satır başına karar, ezilen dosya çöpe | Herkeste dosya dosya soru ya da işe toplu ön ayar |
| Kalıcı silme ve çökme güvenliği | Anında (gizli ada çevir, arka planda sil, açılışta tamamla); yarım iş tamamlanmış görünmez | Hiçbirinde yok |
| Arşivler | Geniş açma, zip/7z/tar oluşturma, AES, ad şifreleme, 7z parçalı, var olan arşive ekleme; zip-slip ve zip bombası denetimi | OC'de açma kaldırıldı (V4 beta'da geri); FP'de arşiv yok; FL ve Fi'de oluşturma dar |
| Dönüştürme | Resim (HEIC/AVIF, konum silme), metin kodlaması ve satır sonu, ses/video (ffmpeg) | Toplu metin kodlaması hiçbirinde yok; ses/video dönüştürme yalnız OC'de (ses çıkarma) |
| PDF | Resimden PDF, birleştirme, bölme, sayfa çıkarma, PDF'ten resim | Hiçbirinde yok |
| Araç indirme | 7-Zip, ffmpeg, pdfium tek tıkla, SHA-256 sabitli | Hiçbirinde yok |
| Kullanıcı komutları | Kabuksuz, güvenli yer tutucular, paralel, geri alınabilir | Rakiplerde kabuk ya da betik tabanlı |
| Toplu ad | Canlı önizleme, elle düzeltme, döngü güvenli (a↔b), EXIF şablonu, Türkçe i/İ | En güçlüsü DO'da; çoğunda dar |
| Ayarlar | Düz metin TOML, yorumlar korunur, canlı, satır numaralı hata | JSON, plist ya da ikili; DO ve PF'de "sorun olursa sil" |
| Açılış ve bellek | 22-35 ms, boşta ~7 MB, 100 bin dosya ~17 MB, 20 sekme +0,1 MB | OC soğuk ~3 s, Fi ~86 MB ile >1 GB arası, PF şikâyetli |
| Türkçe | Doğal sıralamada Türkçe harf sırası | Hepsi sistem sıralaması |
| Ölçüm | Yayımlanmış, betikle tekrarlanabilir performans tablosu | FP hiç rakam vermiyor; diğerlerinde de yok |

**Yedisinde de olmayanlar** (notlara göre):

- Üç sistemde tek kod.
- PDF işlemleri.
- Toplu metin kodlaması dönüştürme.
- Önceden çakışma listesi.
- Anında, çökmeye dayanıklı kalıcı silme.
- SHA-256 sabitli araç indirme.
- **Şifreli ayar senkronu.** Planlı. FL yalnız favorileri iCloud'la eşitliyor, DO'da yalnız yedek var.

**Düzeltme:** Komut paleti ve Git bu gruba girmiyor.

- **Komut paleti:** Fi, FP ve FL'de var; DO'nun FAYT komut modu da benzer. Yalnız OC, PF ve Far'da yok (Far'da menü süzme var).
- **Git:** Fi, FL ve PF'de var. OC, DO, FP ve Far'da yok.

İkisi de bu dört rakibe karşı ayırt edici, ama genel bir üstünlük değil.

---

## 4. Performans ve lisans notları

**Performans**

- **Gezik ölçümleri:**
  - Açılış 22-35 ms; boşta 6,8-7,0 MB (`icons = "gezik"` ile 6,3 MB).
  - 100 bin dosya 17,2 MB; 100 bin adı sıralama 39 ms; 20 sekme +0,1 MB.
  - 10 000 küçük dosya kopyası 1,6 s (Explorer 12,4 s, 7,7×).
  - 359 Hz ekranda 100 bin dosyada kaydırma ~270 ms CPU (`max-fps = 120`).
- **Rakip rakamları:**
  - Fi: boşta ~86 MB, aramadan sonra >500 MB, uzun oturumda 1-2 GB; v4.2.34'te 45 sn açılış bildirildi.
  - OC: .NET, soğuk açılış ~3 s, bu yüzden arka planda bekliyor.
  - PF: 77 MB paket; destek makalesi "modülleri ve klasör boyutunu kapatın" diyor.
- **İkili boyut:** FP 2,46 MB, Gezik ~20,5 MB (5c ölçümü). Fark yaklaşık 8×. Fi ve DO notlarındaki "~13 MB" eski bir rakam; 20,5 MB esas alınmalı. unrar, iso, cab ve cpio okuyucuları pdfium gibi isteğe bağlı indirmeye alınabilir. Her adımda boyut bütçesi tutulmalı.
- **FP notundan dersler:**
  - Meta veri ve klasör boyutu sıralamayı ya da listeyi asla bekletmemeli.
  - Süzme ve gruplama gelince klasörü tümden yeniden yüklemek yerine satır bazında güncelleme gerekir (bugün 50 bin dosyada %2,3-2,7 CPU).
  - Kabuk/COM nesneleri iş bitince bırakılabilir.
  - Küçük resim önbelleği (bugün sabit ≤ 32 MB) ayar olarak açılabilir.
  - İsteğe bağlı GPU arka ucunun (Skia/FemtoVG) bellek maliyeti ölçülmeli.
- **Eksik ölçü:** Klasör yükleme süresi (100 bin dosya, soğuk ve sıcak) tabloya eklenmeli. Böylece FP'nin "milisaniyede" iddiasıyla doğrudan kıyaslanabilir.

**Lisans**

| Uygulama | Model |
|---|---|
| Gezik | PolyForm Noncommercial: kişisel, eğitim ve kâr amacı gütmeyen kullanımda ücretsiz, kaynak okunur; **ticari kullanım için hiç yol yok** |
| OneCommander | Evde ücretsiz; Pro $30 ömür boyu; ticari kullanım her durumda Pro ister |
| Files | Açık kaynak; Store sürümü ücretli, sideload ücretsiz |
| Directory Opus | Kapalı kaynak, kalıcı lisans + yıllık güncelleme ücreti; SFTP ve USB Export ek ücretli; Light 13'te kaldırıldı |
| File Pilot | Beta ücretsiz; Essential $50, Pro $200, Team |
| Far Manager | BSD-3: ticari dahil serbest |
| ForkLift 4 | 1 yıl güncellemeyle $19,95-$69,95; kitlesi büyük ölçüde profesyonel |
| Path Finder | $29,95/yıl abonelik ya da süreli anahtar; internetle etkinleştirme |

Satılık uygulamaların hepsi ticari kullanıma para karşılığı izin veriyor; Far ise ücretsiz izin veriyor. Gezik ise ticari kullanıcıya hiç yol bırakmıyor. FL ve PF kullanıcısı (geliştirici, ajans) bu yüzden Gezik'e geçemez. Bu, özellik değil ama karşılaştıran kullanıcının ilk göreceği fark.

---

## 5. Bakımcı için açık kararlar

1. Uzak bağlantılar (SFTP/WebDAV/S3) yol haritasına ayrı bir adım olarak mı girecek, yoksa spec'e "bilerek kapsam dışı" diye mi yazılacak?
2. Ticari kullanım için bir yol (ücretli ticari lisans ya da çift lisans) açılacak mı?
3. Gezinme spec'indeki "tek sekme + `start-folder`" kararı, oturum geri yükleme ayarına dönüşsün mü?
4. Bilerek kapsam dışı bırakılanlardan hangileri yeniden açılsın? Adaylar: gruplama, video oynatma, kod renklendirme, bağlantı oluşturma, panodan dosya, arayüz çevirisi.
5. Grafik ayar penceresi Taşınabilirlik'in parçası mı olsun, ayrı bir adım mı?
6. Çeviri altyapısı (metinlerin dışarı çıkarılması) baştan mı kurulsun, yoksa ilk herkese açık sürümden hemen önce mi?
7. Arama sistem dizinlerini (Everything/Windows Search, Spotlight, tracker) kullanacak mı, yoksa yalnız Gezik'in kendi tarayıcısını mı?
8. Düzenler adımı iki panelle mi sınırlı kalsın, yoksa FP gibi serbest bölme, Miller ve yerleştirilebilir paneller de kapsama girsin mi?
9. Arşiv içi için kurulacak VFS katmanı baştan MTP ve SFTP'yi de taşıyacak biçimde mi tasarlansın?
10. Etiketler Windows ve Linux'ta nerede saklanacak: ADS ve xattr mı, yan dosya mı, merkezi bir veritabanı mı?
11. Yayın hazırlığı (paket, imza, notarization, güncelleme) hangi adımdan sonra yapılacak? İlk herkese açık sürüm hangi adımla çıkacak?
12. Nadir arşiv okuyucuları ikili boyutu düşürmek için isteğe bağlı indirmeye alınsın mı?
13. Yüksek tazelemeli ekranlar için isteğe bağlı GPU çizici sunulsun mu?
14. Eklenti API'si (WASM ya da ayrı süreç) uzun vadede yol haritasına girsin mi, yoksa `[[commands]]` ve komut paleti yeterli mi?
