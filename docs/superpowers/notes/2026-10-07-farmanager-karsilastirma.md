# Far Manager ↔ Gezik özellik karşılaştırması

- **Tarih:** 2026-10-07
- **Karşılaştırılan:** Far Manager 3.0 build 6741 (2026-10-05, BSD-3, yalnız Windows x86/x64/ARM64) ve Linux/macOS/BSD portu **far2l** 2.6.x (beta)
- **Gezik durumu:** `master` (29dcb45); 1-4 ve 5a-5c tamam, **5d (PDF) var sayıldı**
- **Gezik yol haritası (kalan):** 6 Etiketler · 7 Taşınabilirlik · 8 Bulut senkronu · 9 Gelişmiş (arama, çift panel, arşivin içinde gezinme, Git, komut paleti). Numaralar `2026-10-05-toplu-islemler-design.md` §14'e göre.
- **Durum işaretleri:** **Var** · **Kısmen** (eksik yazılı) · **Yok** · **Planlı (adım)**
- **Kaynak sütunu:** Y = Far'a yerleşik · P = Far ile gelen (paketli) eklenti · E = popüler üçüncü taraf eklenti · F2L = far2l

## 1. Özet

Far, iki panelli, tamamen klavyeyle kullanılan, konsolda çalışan bir dosya yöneticisi. Gücünü üç şeyden alıyor: her şeyin F tuşlarıyla yapıldığı panel modeli, panelin altındaki kabuk komut satırı, Lua makroları ve sanal dosya sistemi eklentileri (arşiv, SFTP, süreç listesi, geçici panel). Gezik ise grafik arayüzlü ve tek panelli; Far'ın hiç sunmadığı geri alma, önceden listelenen çakışmalar, iş kuyruğu, toplu yeniden adlandırma, dönüştürme ve PDF işlemleri onda var. Far'dan alınmaya değer olanlar çoğunlukla **düşük maliyetli klavye fikirleri**: maskeyle seçim, panel filtresi, kullanıcı menüsü, numaralı klasör kısayolları ve geçmiş listeleri. Bunların yanında 9. adımın büyük parçaları da Far'da sınanmış bir modele sahip: çift panelde F5/F6 ile karşı panele kopyalama ve klasör karşılaştırma, sonuçları panele dökülen arama ve arşivi klasör gibi gezme.

## 2. Gezik envanteri (kısa)

| Alan | Gezik'te olan |
|---|---|
| Gezinme | Sekmeler (sürükleyerek sıralama, orta tıkla arka planda açma), kenar çubuğu (Klasörler / Sabitlenenler / Sürücüler), breadcrumb ve yol yazma (Ctrl+L), sekme başına geri/ileri (100 kayıt), fare yan tuşları, "This PC", yazarak atlama, canlı klasör izleme |
| Görünüm | Liste ve ızgara (3 boyut), 5 sütun (Ad, Değiştirilme, Oluşturulma, Tür, Boyut; göster/gizle, genişlik), Türkçe doğal sıralama, klasör başına görünüm hafızası, sistem ya da Gezik ikonları, küçük resimler, gizli dosyaları göster/gizle (Ctrl+H) |
| Önizleme | Sağ panel (Alt+P): resim, metnin ilk 64 KB'si, küçük resim, klasör öğe sayısı, dosya bilgileri; Boşluk ile quick look (oklarla gezilir) |
| Dosya işlemleri | Kendi motoru; sistem panosu (iki yönlü), dışarıya ve içeriye sürükle-bırak, çakışma listesi (Replace/Skip/Keep both/If newer), çöp kutusu / kalıcı silme (arka planda; açılışta tamamlanır), oturum boyu çok adımlı geri al/yinele, F2 ile yerinde yeniden adlandırma, yeni klasör/dosya, çoğaltma, disk bazlı paralel kuyruk, duraklat/iptal, görev çubuğu ilerlemesi |
| Toplu işlemler | 5a kural tabanlı yeniden adlandırma (regex, numara, harf, şablon, EXIF, önizleme, kayıtlı setler) · 5b arşiv açma (zip, 7z, rar, tar ailesi, cab, iso, deb…; nadir biçimler 7-Zip ile) ve oluşturma (zip/7z/tar; AES, 7z'yi parçalara bölme, var olan arşive ekleme) · 5c dönüştürme (resim, metin kodlaması ve satır sonu, ffmpeg ile ses/video) ve `[[commands]]` · 5d PDF (resimden PDF, birleştirme, bölme, PDF'ten resim, sayfa çıkarma) · araç indirme (SHA-256) |
| Özelleştirme | `settings.toml` (canlı yeniden yükleme, hatalar dosya ve satırla), TOML temaları (`base` kalıtımı, otomatik açık/koyu), düzen (kenar çubuğu yeri, yoğunluk), 29 eylemin kısayolu yeniden atanabilir |
| Platform ve performans | Windows/macOS/Linux (X11/Wayland) tek kod; 24-35 ms açılış, boşta 6,8 MB, 100 bin dosyalık klasör 17,2 MB, 100 bin adı sıralama 39 ms |
| Bilerek kapsam dışı | Sözdizimi renklendirme, video oynatma, Windows önizleme işleyicileri, arayüz çevirisi, RAR oluşturma, kullanıcı temalarının düzeni değiştirmesi (yorumlayıcı gerekir), geri alma geçmişini kalıcı tutma |

## 3. Karşılaştırma

### 3.1 Paneller ve panel modları

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 1 | İki bağımsız dosya paneli; Tab ile panel değiştirme, Ctrl+U ile panelleri yer değiştirme | Y | **Planlı (9)** | Şimdilik tek liste ve sekmeler |
| 2 | Panelleri gizleme, Ctrl+O ile arkadaki konsol çıktısını görme | Y | **Yok** | Konsola bağlı; GUI'de karşılığı zayıf |
| 3 | Panel genişliğini ve yüksekliğini klavyeyle ayarlama | Y | **Kısmen** | Kenar çubuğu ve önizleme fareyle genişletilir; klavye yok |
| 4 | 10 hazır görünüm modu (Brief, Medium, Full, Wide, Detailed, Descriptions, Owners, Links, Alternative); LeftCtrl+0…9 | Y | **Kısmen** | Liste ve ızgara (Ctrl+1/2); çok sütunlu "brief" listesi yok |
| 5 | Sütun türlerini ve genişliklerini düzenleme (ayırma boyutu, erişim ve değişim tarihi, öznitelik, sahip, hardlink sayısı, ADS, açıklama), "şerit" düzeni | Y | **Kısmen** | 5 sütun göster/gizle ve genişlik var; uzantı, erişim tarihi, öznitelik ve sahip sütunu yok |
| 6 | Ağaç paneli (Ctrl+T), ağaç önbelleği | Y | **Yok** | Kenar çubuğu ağaç değil |
| 7 | Bilgi paneli (Ctrl+L): disk türü, boş alan, etiket, bellek, klasör açıklama dosyası, güç durumu | Y | **Kısmen** | Önizleme dosya bilgisini gösterir, durum çubuğu seçim boyutunu; disk boş alanı ve bellek yok |
| 8 | Hızlı görünüm paneli (Ctrl+Q): karşı panelde dosya içeriği (görüntüleyici komutlarıyla), klasörde toplam boyut, dosya sayısı ve slack | Y | **Kısmen** | Alt+P ve quick look var; tam dosya kaydırma, hex ve klasörün toplam boyutu yok |
| 9 | Klasör boyutunu hesaplama (F3 klasörde) | Y | **Yok** | Önizleme yalnız öğeleri sayar (≤ 10.000) |
| 10 | Sıralama modları: ad, uzantı, yazma, boyut, sırasız, oluşturma, erişim, açıklama, sahip; ters; ikincil ölçütler; "seçilenler önce" | Y | **Kısmen** | Ad, değiştirilme, oluşturulma, tür ve boyut, iki yön, doğal sıra; uzantı, sırasız, ikincil ölçüt ve "seçilenler önce" yok |
| 11 | Gizli/sistem dosyalarını göster (Ctrl+H) | Y | **Var** | `toggle-hidden` (Ctrl+H) |
| 12 | Hızlı bulma: Alt+harfler, `*`/`?` jokerleri, Ctrl+Enter ile sonraki eşleşme | Y | **Kısmen** | Yazarak atlama var; joker ve sonraki eşleşme yok |
| 13 | Seçim: Ins, Gray+ ile maskeyle seç, Gray− ile bırak, Gray* ile ters çevir, aynı uzantıyı/adı seç, önceki seçimi geri al (Ctrl+M) | Y | **Kısmen** | Ctrl+A, Shift/Ctrl, Ctrl+Boşluk ve çerçeve seçimi var; maske, ters çevirme ve geri alma yok |
| 14 | Fonksiyon tuş çubuğu (Ctrl/Alt/Shift'e basınca değişen F1-F12 etiketleri) | Y | **Yok** | Yalnız durum çubuğu |
| 15 | Klasör kısayolları: Ctrl+Shift+0…9 ile kaydet, RightCtrl+0…9 ile git | Y | **Kısmen** | Sabitlenenler var ama fareyle; numaralı tuş yok |
| 16 | Sürücü menüsü (Alt+F1/F2): sürücüler ve eklenti panelleri; ağ sürücüsünü ayırma, çıkarma, USB'yi güvenle kaldırma | Y | **Kısmen** | Kenar çubuğunda Sürücüler ve "This PC"; çıkarma ve ayırma yalnız Windows Shell menüsünden |
| 17 | Klasör geçmişi (Alt+F12) ve görüntüleme/düzenleme geçmişi (Alt+F11); öğe kilitleme; kalıcı | Y | **Kısmen** | Sekme başına oturumluk geri/ileri; açılır liste ve kalıcılık yok |
| 18 | Ekran listesi (F12, Ctrl+Tab): birden çok görüntüleyici ve düzenleyici penceresi | Y | **Kısmen** | Sekmeler var; görüntüleyici ve düzenleyici yok |
| 19 | Paneller arası sürükle-bırak; sağ tuşla kopyala/taşı | Y | **Var** | Daha geniş: dışarıya ve dışarıdan da çalışır |
| 20 | Panelin kendini yenilemesi (dışarıdaki değişikliklerde) | Y | **Var** | `folder_watch` |

### 3.2 Komut satırı

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 21 | Panellerin altında sürekli duran kabuk komut satırı (cmd/PowerShell), bulunulan klasörde çalışır | Y | **Yok** | "Burada terminal aç" da yok |
| 22 | Panelden ad ve yol ekleme (Ctrl+Enter, Ctrl+F, Ctrl+[ / ], karşı panel) | Y | **Yok** | |
| 23 | Komut geçmişi (Alt+F8, Ctrl+E/X); öğe kilitleme; kalıcı | Y | **Yok** | |
| 24 | Otomatik tamamlama (dosya sistemi, geçmiş, PATH, ortam değişkenleri) | Y | **Yok** | Adres çubuğunda da tamamlama yok |
| 25 | İç komutlar (CD, `disk:`, SET, IF EXIST, CHCP, PUSHD/POPD) ve önekler (`arc:`, `lua:`, `view:`, `edit:`, `clip:`, `goto:`) | Y + P (FarCmds) | **Kısmen** | Ctrl+L ile yol yazıp gitme var; komut ve önek yok |
| 26 | Yönetici olarak çalıştırma (Ctrl+Alt+Enter), yeni pencerede çalıştırma (Shift+Enter) | Y | **Kısmen** | Yalnız Windows Shell menüsünden; kısayolu yok |
| 27 | İstem biçimi (`$p$g`, yönetici işareti) | Y | **Yok** | Gerek yok |

### 3.3 Klavye ve makrolar

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 28 | Her işin F tuşuna bağlı olduğu tutarlı düzen (F3 görüntüle, F4 düzenle, F5 kopyala, F6 taşı, F7 klasör, F8 sil) | Y | **Kısmen** | Temel işlerin Explorer tarzı kısayolları var; "karşı panele F5/F6" modeli yok |
| 29 | Kısayolları yeniden atama (Far'da makrolarla) | Y | **Var** | `[shortcuts]`, 29 eylem; `mod` platforma göre |
| 30 | Makro kaydı ve oynatma (Ctrl+.) | Y (LuaMacro) | **Yok** | |
| 31 | Lua/MoonScript makro betikleri: alanlara göre, koşullu, olay işleyicileri, Macro API | P (LuaMacro) | **Yok** | Tema spec'i yorumlayıcıyı "hızlı ve hafif" hedefi yüzünden reddetti |
| 32 | Menü ve listelerde yazarak süzme (Ctrl+Alt+F, RAlt) | Y | **Yok** | Komut paletiyle aynı fikir; bkz. 9 |
| 33 | Diyalog alanlarında geçmiş (Ctrl+↑/↓); öğe kilitleme | Y | **Kısmen** | Katmanlar son değerleri `state.toml`'da tutar; açılır geçmiş listesi yok |
| 34 | XLat: yanlış klavye düzeniyle yazılanı çevirme | Y | **Yok** | Az kullanılır |
| 35 | Ekran yakalayıcı (Alt+Ins) | Y | **Yok** | GUI'de gerek yok |
| 36 | Görev listesi (Ctrl+W), süreçleri panelde gösterme ve sonlandırma | Y + P (ProcList) | **Yok** | Dosya yöneticisinin işi değil |

### 3.4 Görüntüleyici ve düzenleyici

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 37 | Dahili görüntüleyici (F3): metin, hex ve dump modları; satır kaydırma; kod sayfası seçimi ve algılama; regex ve hex arama; satıra gitme; dev dosyalar | Y | **Kısmen** | Önizleme/quick look yalnız ilk 64 KB'yi gösterir; arama, hex ve kod sayfası seçimi yok |
| 38 | stdin'den görüntüleme (`far -v -`), büyüyen dosyayı izleme | Y | **Yok** | |
| 39 | Dahili düzenleyici (F4): regex ile ara/değiştir, blok ve sütun seçimi, kod sayfaları, satır sonunu koruma, geri al, yer imleri, birden çok pencere, "Find all" | Y | **Yok** | Dosya sistemin uygulamasıyla açılır |
| 40 | Sözdizimi renklendirme (FarColorer) | E | **Yok** | Görünüm spec'inde kapsam dışı |
| 41 | Düzenleyici yardımcıları (Align, AutoWrap, Brackets, DrawLine, EditCase) | P | **Yok** | Gezik için gereksiz |
| 42 | Metin kodlamasını ve satır sonunu değiştirme | Y (düzenleyicide) | **Var** | 5c: toplu ve algılamalı (UTF-8, 1254, UTF-16…, LF/CRLF) |
| 43 | Dış görüntüleyici/düzenleyici seçme (Alt+F3/F4) | Y | **Kısmen** | Sistemin varsayılanı ve Windows'ta "Birlikte aç"; ayarlanabilir dış düzenleyici yok (`[[commands]]` ile yapılabilir) |
| 44 | Resim görüntüleme (PicView vb.) | E | **Var** | Önizleme, quick look, ızgarada küçük resimler |

### 3.5 Dosya işlemleri

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 45 | Karşı panele ya da yazılan yola kopyala/taşı (F5/F6); birden çok hedef (`;`); sonda `\` ile hedef klasör oluşturma | Y | **Kısmen** | Pano ve sürükle-bırak var; hedef yazılan kopyalama diyaloğu ve çoklu hedef yok |
| 46 | Üzerine yazma sorusu: yaz, atla, yeniden adlandır, ekle, yalnız yeniyse, hepsine | Y | **Var** | Çakışmalar baştan tek listede, satır başına karar; değiştirilen dosya çöpe gider |
| 47 | Kopyalarken filtre kullanma | Y | **Yok** | |
| 48 | Erişim haklarını (ACL) kopyalama; şifreli dosya ve ADS uyarıları | Y | **Kısmen** | Sistem kopyalama çağrısı ADS'yi ve öznitelikleri taşır; ACL seçeneği yok |
| 49 | Çöp kutusuna/kalıcı silme (F8, Shift+Del) | Y | **Var** | Ayrıca kalıcı silme arka planda biter ve açılışta tamamlanır |
| 50 | Güvenli silme (Wipe, Alt+Del) | Y | **Yok** | SSD'de anlamı az |
| 51 | Klasör oluşturma (F7; birden çok, ara klasörlerle) | Y | **Var** | Yeni klasör ve yeni dosya; ara klasörlü yol yazılamıyor |
| 52 | Yeniden adlandırma (Shift+F6) | Y | **Var** | F2 ile yerinde; ayrıca 5a toplu yeniden adlandırma (Far'da yerleşik yok) |
| 53 | Ad harf biçimini toplu değiştirme | P (FileCase) | **Var** | 5a Case kuralı, Türkçe i/İ |
| 54 | Bağlantı oluşturma: hardlink, junction, symlink (Alt+F6) | Y | **Yok** | |
| 55 | Öznitelik ve 4 zaman damgasını toplu düzenleme (Ctrl+A, üç durumlu kutular) | Y | **Kısmen** | Yalnız Windows'ta sistemin Özellikler diyaloğu; zaman damgası düzenleme yok |
| 56 | Seçili her dosyaya komut uygulama (Ctrl+G, `!.!` meta sembolleri) | Y | **Kısmen** | `[[commands]]` var (geri alınabilir, paralel); anlık yazılan komut yok |
| 57 | İşlem sırasında yönetici yetkisi isteme | Y | **Yok** | Gezik hata satırı gösterir |
| 58 | Dosyaları panoya kopyala/kes (Ctrl+Shift+C/X), Explorer'a yapıştır | Y | **Var** | Üç sistemde iki yönlü |
| 59 | Adı, tam yolu ve UNC yolunu panoya kopyalama (Ctrl+Ins, Alt+Shift+Ins, Ctrl+Alt+Ins) | Y | **Kısmen** | Yalnız Windows Shell menüsünde "Yol olarak kopyala"; kısayol yok, macOS/Linux'ta yok |
| 60 | Dosya açıklamaları (`descript.ion`, Ctrl+Z); kopyalarken ve taşırken korunur | Y | **Yok** | 6. adımdaki etiketlerle kısmen örtüşür |
| 61 | Yazdırma (Alt+F5) | Y | **Yok** | Windows Shell menüsünde "Yazdır" var |
| 62 | Explorer bağlam menüsü | P (EMenu) | **Var** | Yerel menü ve Gezik öğeleri; macOS/Linux'ta Gezik'in kendi menüsü |

### 3.6 Arama, filtreler, vurgulama ve sıralama grupları

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 63 | Dosya bulma (Alt+F7): maske listesi, metin içeren/içermeyen, büyük/küçük harf, tam kelime, bulanık, hex, kod sayfaları, arşivlerin içi, ADS, symlink; kapsam: tüm diskler, PATH, seçili klasörler; filtreyle | Y | **Planlı (9)** | |
| 64 | Bulunanları geçici panele dökmek ve orada işlemek | Y + P (TmpPanel) | **Planlı (9)** | 9'daki arama tasarımına girmeli |
| 65 | Ağaçta klasör bulma (Alt+F10) | Y | **Planlı (9)** | |
| 66 | Panel filtresi (Ctrl+I): kullanıcı filtreleri ve paneldeki maskeler; +/− ile dahil et/hariç tut; ad, boyut, tarih (göreli), öznitelik, hardlink | Y | **Yok** | |
| 67 | Dosya maskeleri: virgülle liste, `\|` ile hariç tutma, regex maske, adlandırılmış maske grupları (`<arc>`, `<exec>`) | Y | **Kısmen** | `[[commands]] types` yalnız uzantı listesi |
| 68 | Dosya vurgulama grupları: maske ya da öznitelik → renk ve işaret karakteri | Y | **Kısmen** | Tema kategoriye göre ikonu renklendirir, kesilen öğeler soluk görünür; kural yazılamaz |
| 69 | Sıralama grupları (Shift+F11): maskeye göre öne ya da sona | Y | **Yok** | |

### 3.7 Kullanıcı menüsü ve ilişkilendirmeler

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 70 | Kullanıcı menüsü (F2): genel, kullanıcıya özel ve klasöre özel menüler; alt menüler, harf/F tuşu kısayolları, çalıştırmadan önce parametre sorma (`!?başlık?varsayılan!`) | Y | **Kısmen** | `[[commands]]` sağ tıkta "Commands ▸" altında ve Convert katmanında, türe göre süzülür; kısayol, alt menü, klasöre özel menü ve parametre sorma yok |
| 71 | İlişkilendirmeler: her maske için Enter, Ctrl+PgDn, F3, Alt+F3, F4, Alt+F4 komutları; birden çok seçenekte menü | Y | **Kısmen** | Sistemin varsayılanı, `[archives] double-click`, türe göre komutlar; Enter'ın davranışı tanımlanamaz |
| 72 | Meta semboller: ad, uzantı, kısa ad, seçili adların listesi (`!&`), liste dosyası (`!@!`), karşı panelin adı ve yolu (`!^`, `!##`) | Y | **Kısmen** | `{in}` `{dir}` `{name}` `{ext}` `{out}` `{outdir}`; çoklu seçimi ya da karşı paneli gösteren yer tutucu yok (komut her dosyada ayrı çalışır) |

### 3.8 Arşivler

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 73 | Arşivin içine girip panel gibi gezmek (Enter, Ctrl+PgDn); F5 ile çıkarma, ekleme, F8 ile arşivden silme | P (ArcLite) | **Planlı (9)** | "Arşivin içinde gezinme ve arşivden silme"; yeniden yazma motoru 5b'de hazır |
| 74 | 7z.dll ile 7-Zip'in açtığı her biçimi açma | P (ArcLite) | **Var** | Yaygın biçimler içeride, nadirler indirilen 7-Zip ile |
| 75 | Arşiv oluşturma: biçim, düzey, şifre, adlandırılmış profiller, tarih makrolu ad, parçalara bölme | P (ArcLite) | **Kısmen** | zip/7z/tar.*, 4 düzey, AES, 7z'yi bölme var; profil yok (son seçim hatırlanır); zip bölme kapsam dışı |
| 76 | SFX (kendiliğinden açılan) arşiv | P (ArcLite) | **Yok** | Kapsam dışı bırakılması uygun |
| 77 | Çıkarma seçenekleri: gerekirse ayrı klasör, çakışmada ne yapılacağı, şifre, hataları atlama, sonra arşivi silme | P (ArcLite) | **Var** | Akıllı "Extract here", çakışma listesi, şifre katmanı, hata satırları; "sonra sil" yok |
| 78 | Var olan arşive ekleme/güncelleme | P (ArcLite) | **Var** | 5b; arşivin üstüne sürükleyip bırakmak da olur |
| 79 | Harici arşivleyicileri tanımlama | P (MultiArc) | **Kısmen** | Yalnız 7-Zip; yine de `[[commands]]` ile yapılabilir |

### 3.9 Ağ, FTP ve SFTP

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 80 | Ağ tarayıcısı: SMB paylaşımları, sürücü eşleme ve ayırma | P (Network) | **Kısmen** | UNC yolu yazılıp açılır, ağ diski için 4 iş parçacığı; tarama ve eşleme yok |
| 81 | FTP istemcisi paneli | P (FTP) | **Yok** | |
| 82 | NetBox: SFTP, SCP, FTP/FTPS, WebDAV, S3; oturumlar, anahtarla giriş | P (NetBox) | **Yok** | |
| 83 | far2l NetRocks: SFTP, SCP, FTP(S), SMB, NFS, WebDAV, S3, SSH üstünden SHELL | F2L | **Yok** | |

### 3.10 Karşılaştırma ve eşitleme

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 84 | Klasör karşılaştırma: iki panelde yalnız birinde olanları ya da daha yenileri seçer (ad, boyut, zaman; alt klasörsüz) | Y | **Yok** | Önce çift panel gerekir |
| 85 | Advanced Compare: alt klasörler, en fazla derinlik, içerik, satır sonunu ve boşlukları yok sayma, saat dilimi toleransı | P (Compare) | **Yok** | |
| 86 | Karşı paneli aynı klasöre getirme | P (SameFolder) | **Planlı (9)** | Çift panelle gelir |
| 87 | Klasörleri eşitleme (tek ya da iki yönlü) | E | **Yok** | Far'da yerleşik değil |

### 3.11 Özelleştirme ve renkler

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 88 | Her arayüz öğesinin rengi; 16 renk, 256 renk ve RGB; topluluk renk temaları | Y | **Var** | TOML temaları, `base` kalıtımı, otomatik açık/koyu, ikon renkleri, boyutlar |
| 89 | `far:config`: tüm ayarların düzenleyicisi (varsayılandan farklılar işaretli, süzülebilir) | Y | **Kısmen** | Canlı yeniden yüklenen `settings.toml`, hatalar satırıyla; grafik ayar ekranı yok |
| 90 | Ayarları dışa/içe aktarma (`-export`, `-import`), profil klasörünü seçme (`-s`) | Y | **Kısmen** | `settings.toml` doğrudan kopyalanabilir, `GEZIK_CONFIG_DIR`; dışa/içe aktarma ve senkron klasör **Planlı (7)** |
| 91 | Arayüz dilleri (çok dilli dil ve yardım dosyaları) | Y | **Yok** | Gezinme spec'inde kapsam dışı |
| 92 | Hangi işlemin onay isteyeceğini seçme (12 tür) | Y | **Kısmen** | `confirm-trash`; diğer onaylar sabit |
| 93 | Arayüz ayarları (saat, tuş çubuğu, pencere başlığı biçimi, imleç) | Y | **Kısmen** | Kenar çubuğu yeri, yoğunluk, yazı boyutu, satır yüksekliği, `max-fps` |

### 3.12 Eklenti API'si

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 94 | C/C++ eklenti API'si (`plugin.hpp`): sanal dosya sistemi panelleri, komut önekleri, düzenleyici, görüntüleyici ve diyalog eklentileri | Y | **Yok** | |
| 95 | Lua ile eklenti yazma (LuaFAR, LuaMacro modülleri); .NET (FarNet); Python (far2l) | P + E + F2L | **Yok** | |
| 96 | Eklenti yöneticisi, önbellek, ihtiyaç anında yükleme (`-co`, `-p`) | Y | **Yok** | |
| 97 | PlugRing'de yüzlerce üçüncü taraf eklenti | E | **Yok** | Gezik'teki tek genişletme yolu `[[commands]]` |

### 3.13 Performans ve platform

| # | Far özelliği | Kaynak | Gezik | Not |
|---|---|---|---|---|
| 98 | Native, küçük, anında açılış; düşük bellek | Y | **Var** | 24-35 ms açılış, boşta 6,8 MB |
| 99 | Çok büyük klasörlerde akıcı kullanım | Y | **Var** | 100 bin dosya 17,2 MB; kaydırmada kare sınırı |
| 100 | Linux/macOS/BSD: far2l (TTY, X11, wx GUI) | F2L | **Var** | Gezik tek kodla üç sistemde native GUI; far2l ayrı ve Far 2 tabanlı bir çatal |
| 101 | Kurulumsuz (portable) kullanım: 7z dağıtımı, `-s` profil yolu | Y | **Planlı (7)** | |
| 102 | SSH ya da terminal üstünden uzakta çalışma (far2l TTY) | F2L | **Yok** | GUI'nin doğası gereği; hedef değil |

### 3.14 Durum sayıları

| Durum | Sayı |
|---|---|
| Var | 20 |
| Kısmen | 35 |
| Yok | 40 |
| Planlı | 7 (6'sı adım 9, 1'i adım 7) |
| **Toplam** | **102** |

## 4. Gezik'te olup Far'da olmayanlar

| Gezik özelliği | Far'da durum |
|---|---|
| Tüm işlemlerde oturum boyu, çok adımlı geri al/yinele (kopyalama, taşıma, yeniden adlandırma, çöpe atma, değiştirme, arşiv, dönüştürme) | Yok |
| Çakışmaların iş başlamadan tek listede gösterilmesi; değiştirilen dosyanın çöpe gitmesi | Soru dosya dosya, işin ortasında gelir |
| Disk kümesine göre paralel iş kuyruğu, duraklat/iptal, görev çubuğu ilerlemesi; kalıcı silmenin arka planda bitmesi ve açılışta toparlanması | İşlemler çoğunlukla ön planda; kuyruk yok |
| Önizlemeli, kural tabanlı toplu yeniden adlandırma (regex, numara, EXIF şablonu, elle düzeltme, döngüleri çözme, kayıtlı setler) | Yerleşik değil; üçüncü taraf eklentilerle |
| Dönüştürme: resim (boyutlandırma, EXIF'e göre döndürme, konumu silme, HEIC), ffmpeg ile ses/video, toplu metin kodlaması | Yok |
| PDF: resimden PDF, birleştirme, bölme, PDF'ten resim | Yok |
| Dış araçları tek tıkla, SHA-256 doğrulamasıyla indirme | Yok |
| Arşivden kötü niyetli yolların ve sıkıştırma bombalarının engellenmesi | 7z.dll'in davranışına bağlı |
| Izgara görünümü, küçük resimler, sistem ikonları, resim önizleme ve quick look | Metin tabanlı; resim için eklenti gerekir |
| Sekmeler ve sabitlenmiş klasörlerin kenar çubuğu | Sekme yok (ekran listesi ve klasör kısayolları var) |
| Dışarıya ve dışarıdan sürükle-bırak; panonun üç sistemde iki yönlü çalışması | Sürükleme yalnız paneller arasında |
| Yerel sağ tık menüsü ve Gezik öğeleri | EMenu, yalnız Windows |
| Otomatik açık/koyu tema, yüksek DPI, fare ve dokunmatik yüzey dostu | Konsol paleti |
| Windows, macOS ve Linux'ta tek kodla native GUI | Far yalnız Windows; far2l ayrı bir çatal |
| Türkçe doğal sıralama (ç ğ ı ö ş ü, I/ı) | Sistemin sıralaması |

## 5. Klavye öncelikli fikirler GUI'ye nasıl taşınır

| Far fikri | GUI'ye uyum | Gezik'te nasıl olabilir |
|---|---|---|
| Çift panel, F5/F6 ile karşı panele kopyala/taşı, Tab, Ctrl+U | **Çok iyi** | 9'un çift paneli; F5/F6 kısayolu çift panel açıkken karşı paneli hedef alır, kapalıyken hedef soran küçük bir katman açar |
| Maskeyle seç/bırak/ters çevir, aynı uzantıyı seç, önceki seçimi geri al | **Çok iyi** | `select-pattern` (`+`), `deselect-pattern` (`-`), `invert-selection` (`*`), `select-same-ext` eylemleri; maske için küçük bir katman |
| Panel filtresi (Ctrl+I) | **Çok iyi** | Liste üstünde süzme alanı (yazdıkça daralır, `*.jpg` gibi maske, `!` ile hariç) ve kayıtlı filtreler; arama ile aynı süzme koduyla |
| Hızlı bulma (jokerli, sonraki eşleşme) | **İyi** | Var olan yazarak atlamaya `*`/`?` ve "F3/Enter ile sonraki" eklemek |
| Kullanıcı menüsü (F2) | **Çok iyi** | `[[commands]]`'a `key`, `group` (alt menü), `ask` (çalıştırmadan önce sorulan değer), `{files}` / `{list}` yer tutucuları; kısayolla açılan menü. Klasöre özel menü güvenlik açısından yalnız onaylı klasörlerde |
| Menü süzme ve F11 eklenti menüsü | **Çok iyi** | Komut paleti (9): tüm eylemler, `[[commands]]`, sabitlenenler ve geçmiş tek süzülebilir listede |
| Komut satırı | **Orta** | Tam gömülü kabuk pahalı ve platforma bağımlı. Yerine: "Burada terminal aç" (sistemin terminali) ve adres çubuğunda `>` ile bulunulan klasörde komut çalıştırma (çıktı iş panelinde, `{name}` yer tutucularıyla) |
| Klasör kısayolları (Ctrl+Shift+0…9) | **Çok iyi** | Sabitlenen ilk 9 klasöre `mod+1…9` gibi numaralar (Ctrl+1/2 görünümde kullanılıyor, yeni bir seçim gerekir) |
| Klasör geçmişi (Alt+F12) | **İyi** | Geri/ileri düğmesinde sağ tık ya da uzun basışla liste; isteğe bağlı kalıcı "son klasörler" |
| Hızlı görünüm paneli | **Var** | Önizlemeye kaydırılabilir tam metin, hex ve klasör toplam boyutu (arka planda, iptal edilebilir) eklenebilir |
| Fonksiyon tuş çubuğu | **Orta** | İsteğe bağlı alt çubuk: F2…F8 eylemleri, değiştiriciye basınca değişen etiketler; yeni kullanıcıya kısayol öğretir |
| Vurgulama ve sıralama grupları | **İyi** | `[[highlight]]` (maske ya da öznitelik → tema rengi) ve `[view] sort-groups`; çizim zaten temadan |
| Lua makroları, makro kaydı | **Zayıf** | Yorumlayıcı hedefe ters. Komut paleti, kullanıcı menüsü ve eylem zinciri (`[[macros]]`: sırayla çalışan eylem adları) ihtiyacın çoğunu karşılar |
| Dahili düzenleyici, Wipe, XLat, Ctrl+O, ekran yakalayıcı | **Zayıf** | Önerilmez |

## 6. Öneriler (değer ↔ efor)

| Sıra | Özellik | Değer | Efor | Yol haritasındaki yeri |
|---|---|---|---|---|
| 1 | Hızlı panel filtresi (yazarak, maskeyle, `!` ile hariç) ve kayıtlı filtreler | Yüksek | Düşük-orta | 6'dan önce küçük bir ara adım ("klavye paketi") ya da 9'un ilk parçası; 9'daki aramayla aynı süzme kodu |
| 2 | Maskeyle seç/bırak/ters çevir, aynı uzantıyı seç, önceki seçimi geri al | Yüksek | Düşük | Ara adım ("klavye paketi"); `Selection` hazır |
| 3 | Çift panel: F5/F6 ile karşı panele kopyala/taşı, Tab, Ctrl+U, aynı klasör | Çok yüksek (güçlü kullanıcı) | Yüksek | 9; 4a motoru ve çakışma listesi doğrudan kullanılır |
| 4 | Dosya bulma (ad maskesi ve içerik, filtre) ve sonuçları panel gibi işleme | Çok yüksek | Yüksek | 9 arama; Far'ın "Panel" ve TmpPanel fikri tasarıma alınmalı |
| 5 | Kullanıcı menüsü: `[[commands]]`'a kısayol, alt menü, `ask`, `{files}`/`{list}` | Yüksek | Düşük-orta | Ara adım ya da 9 komut paletiyle birlikte |
| 6 | Klasör karşılaştırma (ad, boyut, zaman; sonra içerik ve alt klasörler) ve tek yönlü eşitleme | Yüksek | Orta | 9, çift panelin hemen arkasından |
| 7 | Komut paleti ve menü/listelerde süzme | Yüksek | Orta | 9 (makroların yerini tutar) |
| 8 | Ad/yol kopyalama kısayolları (üç sistemde), numaralı klasör kısayolları, klasör geçmişi listesi | Orta-yüksek | Düşük | Ara adım ("klavye paketi") |
| 9 | "Burada terminal aç" ve adres çubuğunda `>` ile komut çalıştırma | Orta-yüksek | Düşük (terminal) / orta (komut) | Terminal hemen; komut satırı 9 |
| 10 | Arşivin içinde gezinme (Enter ile girme, F5 ile çıkarma, silme) | Yüksek | Orta-yüksek | 9; sanal dosya sistemi arayüzü, ileride SFTP için de kullanılacak biçimde tasarlanmalı |
| 11 | Önizlemede tam metin kaydırma, hex, klasör toplam boyutu | Orta | Düşük-orta | Görünüm'e ek, istendiğinde |
| 12 | Vurgulama kuralları ve sıralama grupları | Orta | Düşük | Tema ya da görünüm eki |
| 13 | SFTP, FTP, WebDAV ve S3 bağlantıları | Yüksek (azınlık için) | Çok yüksek | 9'dan sonra (10); 10. sıradaki sanal dosya sistemi arayüzünün üstüne |
| 14 | Bağlantı oluşturma (symlink, hardlink, junction), toplu öznitelik ve zaman damgası düzenleme | Orta | Orta | 9 ya da sonrası |
| 15 | Eklenti API'si (WASM ya da ayrı süreçte) | Uzun vadede yüksek | Çok yüksek | Yol haritasının dışında; önce `[[commands]]` ve kullanıcı menüsü genişletilmeli |
| — | Dahili düzenleyici, makro yorumlayıcı, Wipe, XLat, konsol çıktısı, bilgi panelinde bellek/güç | Düşük | — | Önerilmez |

**Öne çıkan sonuç:** 1, 2, 5 ve 8 birlikte küçük bir "klavye paketi" ediyor. Tahminen tek bir PR; 6. adımdan önce yapılırsa Gezik güçlü kullanıcıya Far gibi hissettirir. Büyük parçalarda (3, 4, 6, 7, 10) 9. adım alt adımlara bölünmeli: önce çift panel ve karşılaştırma, sonra arama ve sonuç paneli, sonra komut paleti, kullanıcı menüsü ve komut satırı, en son sanal dosya sistemi (arşivin içi). Arşiv içinde gezinme için kurulacak sanal dosya sistemi arayüzü, ileride SFTP ve WebDAV'ın da temeli olur. Far'ın ArcLite ve NetBox'ı da aynı panel eklentisi modeline dayanıyor.

## 7. Kaynaklar

- Far Manager sitesi: https://www.farmanager.com/ (Anubis koruması yüzünden içerik okunamadı; bilgiler aşağıdaki depodan alındı)
- PlugRing: https://plugring.farmanager.com/ (aynı nedenle okunamadı)
- GitHub deposu: https://github.com/FarGroup/FarManager
- Yerleşik yardım (özellik ve tuş başvurusu): https://raw.githubusercontent.com/FarGroup/FarManager/master/far/FarEng.hlf.m4
- Değişiklik günlüğü (build 6741, 2026-10-02): https://raw.githubusercontent.com/FarGroup/FarManager/master/far/changelog
- Sürüm 3.0.6741.5010 (2026-10-05) ve paketli eklentiler (`Plugins\` listesi: Align, ArcLite, AutoWrap, Brackets, Compare, DrawLine, EditCase, EMenu, FarCmds, FileCase, HlfViewer, LuaMacro, NetBox, Network, ProcList, SameFolder, TmpPanel): https://github.com/FarGroup/FarManager/releases
- Advanced Compare yardımı: https://raw.githubusercontent.com/FarGroup/FarManager/master/plugins/compare/CmpEng.hlf
- ArcLite yardımı: https://raw.githubusercontent.com/FarGroup/FarManager/master/plugins/arclite/arclite_eng.hlf
- TmpPanel yardımı: https://raw.githubusercontent.com/FarGroup/FarManager/master/plugins/tmppanel/TmpEng.hlf
- far2l: https://github.com/elfmz/far2l
- Wikipedia, Far Manager: https://en.wikipedia.org/wiki/Far_Manager
- FarColorer: https://colorer.sourceforge.net/farplugin.html
- Gezik: `README.md`, `crates/gezik-config/templates/settings.toml`, `crates/gezik-config/src/shortcuts.rs`, `crates/gezik/src/{context_menu,folder_watch,preview}.rs`, `docs/superpowers/specs/*.md`
