# Alt Proje 11: Araçlar — Tasarım

- **Tarih:** 2026-10-11
- **Durum:** Kapsam kullanıcı kararı (2026-10-11: sekiz başlığın hepsi, §2). Ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§20, her biri gerekçesiyle). Spec onaylanınca plan önceden onaylıdır. **11c uygulandı** (`feat/tools-11c`; Windows ekran testleri koşuldu, macOS 281–285 ve Linux 213–215 bekliyor). **11h uygulandı** (`feat/tools-11h`; Windows ekran testleri koşuldu, macOS 291–293 ve Linux 219–220 bekliyor). **11g uygulandı** (`feat/tools-11g`; Windows ekran testleri koşuldu, macOS 297–302 ve Linux 223–225 bekliyor). **11a uygulandı** (`feat/tools-11a`; Windows ekran testleri koşuldu, macOS 271–276 ve Linux 207–210 bekliyor; exe bütçeyi ortak tablo yüzünden aştı, notlarda). **11f uygulandı** (`feat/tools-11f`; Windows ekran testleri koşuldu, macOS 317–323 ve Linux 235–238 bekliyor).
- **Kapsam:** Gezik yol haritasının 11. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md`, "Araçlar": satır 31, 35, 52, 54, 57, 59, 60, 61, 62; §6'daki TC ayrıntıları). Tek spec, dokuz parça (11a–11i; §16).
- **Dayandığı:** `2026-10-04-dosya-islemleri-design.md` (iş motoru, çakışma listesi, geri alma, iş paneli), `2026-10-05-toplu-islemler-design.md` (katmanlar, arşivler, metin dönüştürme), `2026-10-07-klavye-paketi-design.md` (kısayol tablosu, süzgeç deseni), `2026-10-09-arama-design.md` (tarayıcı, klasör boyutları, palet, sonuç listesi), `2026-10-10-sistem-butunlesmesi-design.md` (çöp, bulut, sürücü bilgisi), `2026-10-11-duzenler-design.md` (çift panel, gruplama, çok pencere).
- **Taban:** `master` 448c4ac. Exe ≈ 25,66 MB (crt-static, son sürüm derlemesi; her parçanın ilk görevi yeniden ölçer), boşta bellek Windows'ta satır satır çizimle ≈ 5,5 MB (1280×800), `Action::ALL` = 102, menü kimlikleri 2199'a kadar dolu.
- **Rakip notları:** aynı klasördeki TC, DO, OC, Fi, FP, Far, FL, PF karşılaştırmaları; özellikle TC'nin "Synchronize dirs", "Compare by content", "Split/Combine", "Copy with filter" bölümleri.

## 1. Amaç

Explorer ve Finder'da olmayan, kullanıcının bugün ayrı programlarla (WinMerge, FreeFileSync, WinDirStat, HashCheck, dupeGuru) yaptığı işleri Gezik'e getirmek: klasör karşılaştırma ve eşitleme, sağlama toplamları, yinelenen dosyalar, dosya farkı, boyut haritası, liste dışa aktarma, süzgeçli kopya, bölme/birleştirme ve kopya işlerinin doğrulama, hız sınırı, kuyruk düzeni. Hafiflik bozulmaz: **bir araç açılana kadar bellekte yer tutmaz, hiçbir şey okumaz, hiçbir şey izlemez; kapanınca ayırdığı her şeyi bırakır.** Hiç araç açmayan bir Gezik bugünkü Gezik kadar hafiftir.

Her araç var olan parçaların üstüne kurulur: dosya değiştiren her şey **iş motorundan** geçer (çakışma listesi, iş paneli, tek geri alma, silme her zaman çöpe), taramalar **8a'nın tarayıcısını** (`gezik-search::walk`), iki yer isteyen araçlar **10b'nin çift panelini** kullanır. İkinci bir kopyalama, silme ya da tarama yolu yazılmaz.

### Başarı ölçütleri

- **Sağlama toplamları (11a):** seçilen dosyaların (klasörlerde altındakilerin) MD5, SHA-1, SHA-256, SHA-512 ve CRC32'si tek okumayla; panoya kopyalama, panodaki değerle karşılaştırma, `.sha256` vb. dosya yazma, CSV. `.md5`, `.sha1`, `.sha256`, `.sha512`, `.sfv`, `SHA256SUMS` dosyaları doğrulanır; Gezik'in yazdığı dosya `sha256sum -c` ve `shasum -a 256 -c` ile geçer. Tek dosyada SSD'de okuma hızı tek başına okuma hızının ≥ %90'ı (SHA-256, SHA-NI olan işlemcide).
- **Kopya işi (11b):** doğrulama açıkken her kopyalanan dosya özgün dosyayla bayt bayt karşılaştırılır, uyuşmayan kopya silinip bir kez yeniden denenir; sürücüler arası taşımada kaynak doğrulamadan önce silinmez. Hız sınırı uzun bir kopyada (≥ 30 sn) hedefin ±%5'i içinde. Bekleyen işler iş panelinde yukarı/aşağı taşınır.
- **Dosya farkı (11c):** iki metin dosyası yan yana, değişen satırlar ve satır içi değişen bölüm renkli; 10.000 satırlık iki dosyanın farkı ≤ 150 ms. Ayarlarda dış araç verilmişse (WinMerge, Meld, Beyond Compare, FileMerge…) o açılır.
- **Klasör karşılaştırma (11d):** iki klasör ağacı (alt klasörlerle) ad, boyut, tarih ve isteğe bağlı içerikle karşılaştırılır; TC'nin göster düğmeleri (→ = ≠ ← 1× 2×), FAT 2 sn ve 1 saat toleransı. 100.000 dosya çiftinde tarama iki tarafın ayrı ayrı yürünme süresinin en uzununun ≤ 1,1 katı, karşılaştırma ≤ 200 ms.
- **Eşitleme (11e):** tek yönlü, iki yönlü ve ayna (yedek) kipleri; satır başına yön oku; tek iş, tek geri alma; silinenler her zaman çöpe; kayıtlı profiller paletten.
- **Yinelenen dosyalar (11f):** boyut, sonra ön parça, sonra tam içerik özetiyle gruplar; gruplu sonuç listesi; "biri hariç hepsini seç" yardımcıları; silme çöpe ve geri alınabilir. Sabit bağlantılar (hard link) yineleme sayılmaz.
- **Boyut haritası (11g):** klasörün kare-yakın ağaç haritası (squarified treemap); tıklanan klasöre yakınlaşır, Enter bölmede açar. 8b'nin klasör boyutu taramasını ve önbelleğini kullanır.
- **Liste dışa aktarma (11h):** bölmenin gösterdiği liste (klasör ya da arama sonuçları) metin, CSV ya da HTML olarak dosyaya ya da panoya.
- **Süzgeçli kopya ve bölme (11i):** `Copy To…`/`Move To…` ve F5/F6 penceresinde süzgeç (`*.jpg *.png | thumbs\`), ilişkili dosyalar (`*.jpg>*.cr2`) ve hedefte ad maskesi (`D:\Yedek\*.bak`); dosya bölme (`.001` parçaları + `.crc`) ve CRC denetimli birleştirme.
- **Hafiflik (hepsi):** varsayılan ayarlarla ve hiç araç açılmadan boşta bellek `master`'a göre **0** (ölçüm gürültüsü ±0,02 MB içinde), açılış ≤ +1 ms (yalnız ayar anahtarları), yeni zamanlayıcı, yoklama, izleyici yok. Her parça exe'ye ≤ +262.144 B (§15'te tahminler). Açılıp kapanan her araçtan sonra bellek taban + 0,3 MB'a döner (açık test; +0,3 bilinen heap parçalanması payıdır, 10b/10g notları).

### Kapsam dışı (bilerek)

- **Eşitleme durumu veritabanı** (FreeFileSync'in `.ffs_db`'si): eşitleme durumsuzdur (TC gibi). İki yönlü kipte bir tarafta silinen dosya, öbür tarafta duruyorsa yeniden kopyalanır (§8.2, §19).
- **Zamanlanmış/otomatik eşitleme**, izleyerek canlı eşitleme: boşta hiçbir şey kuralı. Profil elle çalıştırılır.
- **Arşive yazan eşitleme** (TC'nin klasör↔arşiv eşitlemesinin yazma yönü) ve arşivden seçili girdileri çıkarma: arşiv tarafı yalnız karşılaştırılır (§8.5). Arşiv içinde gezinme 16. adımda (VFS).
- **Üç yollu birleştirme, fark içinde düzenleme**, yama (`.patch`) dışa aktarma: dış araç için ayar var (§6.4).
- **Yinelenenleri sabit bağlantıyla değiştirme** (dupeGuru'nun "hard link"i), benzer resim bulma (algısal özet).
- **Boyut haritasında halka grafik** (PF'nin Size Browser'ı), dosya türüne göre ayrı görünüm, ağ sürücüsünde Everything'den ağaç.
- **Kuyrukta sürükleyerek sıralama**, işler arası öncelik: yalnız menüyle yukarı/aşağı (§5.3).
- **BLAKE3, xxHash, SHA-3**: bağımlılık olarak yoklar (§3.2, karar 4).
- **Kayıt (log) dosyası**, e-postayla rapor, eşitleme sonrası komut (FFS): YAGNI.

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Kapsam | Sekiz başlığın hepsi: (1) klasör karşılaştırma + eşitleme (TC ayrıntılarıyla), (2) sağlama toplamları, (3) yinelenen dosyalar, (4) dosya farkı (iç + dış araç), (5) boyut haritası, (6) liste dışa aktarma, (7) süzgeçli kopya + bölme/birleştirme, (8) kopya doğrulama, hız sınırı, kuyruk düzeni | Kullanıcı kararı 2026-10-11 |
| Eşitleme | Tek ve iki yönlü, kayıtlı profiller; silmeler her zaman çöpe ve geri alınabilir; var olan kopya motoru, çakışma listesi ve geri alma | Kullanıcı kararı 1 |
| Sağlama | MD5/SHA-1/SHA-256; BLAKE3 yalnız bağımlılık zaten varsa; panoya kopya; `.md5/.sha1/.sha256` doğrulama; CSV | Kullanıcı kararı 2 |
| Yinelenenler | Önce boyut, sonra içerik özeti; gruplu liste; biri hariç seçme; çöpe silme (geri alınabilir) | Kullanıcı kararı 3 |
| Fark | Hafif iç yan yana metin farkı + ayarlardan isteğe bağlı dış araç | Kullanıcı kararı 4 |
| Boyut haritası | Kare-yakın ağaç haritası, tıklayınca klasöre girme, 8. adımın klasör boyutu taramasını kullanma | Kullanıcı kararı 5 |
| Dışa aktarma | Klasör ya da arama listesi; metin/CSV/HTML; kaydet ya da kopyala | Kullanıcı kararı 6 |
| Kopya | Süzgeçli kopya (yalnız eşleşenler, klasör süzgeci, ilişkili dosyalar, jokerli hedef adı) + ham `.001` bölme/birleştirme, CRC | Kullanıcı kararı 7 |
| İş eklentileri | Doğrulama varsayılan kapalı (iş başına ve ayarda), bant sınırı, kuyruk sıralama | Kullanıcı kararı 8 |
| Erişim | Bütün araçlar paletten ve menülerden; iki yer isteyen araçlar 10b'nin çift panelini kullanır | Kullanıcı kararı |
| Soru | Ayrıntı kararları kullanıcıya sorulmaz; spec'te gerekçeyle (§20) | Bakımcı kuralı |
| Hafiflik | Açılana kadar hiçbir şey ayrılmaz, okunmaz, izlenmez; boşta değişiklik yok; parça başına bütçe | Ürün hedefi |

## 3. Ortak yapı

### 3.1 Araç katmanları

- Ekran isteyen araçlar (sağlama, fark, karşılaştırma/eşitleme, boyut haritası) bugünkü toplu ad, sıkıştır, dönüştür katmanları gibi **pencere üstü katmanlardır**: `app.slint`'te `if root.xx-open: Xx { … }` ile yalnız açıkken kurulur (9. adımdan sonraki bellek düzeltmesinin kuralı). Rust tarafı bir `thread_local` `windows::Slot<Option<Tool>>` tutar; kapanınca `None` olur, arka plan iş parçacıkları iptal bayrağıyla durur, modeller düşer. Katman pencereye aittir (10g §3.1): iki pencerede iki ayrı karşılaştırma açık olabilir.
- Katmanlar Esc ile kapanır (yürüyen tarama önce durur), kendi klavyelerini alır (bugünkü `waits_for_text_fields` ve katman yolu), ekran okuyucuya başlığıyla söylenir.
- Küçük sorular (bölme parça boyutu, dışa aktarma biçimi, `Copy To…` hedefi) bugünkü soru penceresinin (`dialog.rs`) alanlarıyla gelir; yeni pencere türü yok.
- Yinelenen dosyalar katman değil, **sonuç listesidir** (8a'nın `Location::Search`'ü, §9): seçim, silme, önizleme, sağ tık, sütunlar bedava gelir.

### 3.2 Sağlama çekirdeği (11a)

`gezik-ops/src/hash.rs` (yeni). Sağlama aracı, doğrulama (11b), içerik karşılaştırma (11d), yinelenenler (11f) ve birleştirme (11i) bunu kullanır.

- `Algo { Crc32, Md5, Sha1, Sha256, Sha512 }`, `Digests` (bir okumada birden çok algoritma: her parça her seçili özetleyiciye verilir).
- `hash_file(path, algos, &mut progress) -> io::Result<Digests>`: 1 MiB tampon (iş parçacığı başına bir kez ayrılır, iş bitince bırakılır), sıralı okuma ipucu (Windows `FILE_FLAG_SEQUENTIAL_SCAN`, Linux `posix_fadvise(SEQUENTIAL)`, macOS `F_RDAHEAD`); `progress(bytes) -> bool` false dönerse durur (iptal, duraklatma bugünkü `RunCx::stopped` ile).
- `hash_prefix(path, n)`: ilk `n` baytın SHA-256'sı (yinelenenlerin ön elemesi).
- `same_content(a, b, uncached_b, &mut progress) -> io::Result<Option<u64>>`: iki dosyayı 1 MiB'lık iki tamponla yan yana okur, ilk farklı baytın yerini döner (`None`: aynı). Doğrulama ve içerik karşılaştırma özet hesaplamaz: bayt karşılaştırması hem daha ucuz hem çakışmasızdır.
- **Önbelleksiz okuma** (`gezik-platform::fs::open_uncached`, yalnız doğrulamada hedef için): Windows `FILE_FLAG_NO_BUFFERING` (tampon `VirtualAlloc` ile sayfa hizalı, okuma boyu 1 MiB, dosya sonu hizasız kısım ayrı bir tamponlu okumayla); Linux `fdatasync` + `posix_fadvise(DONTNEED)` sonra okuma; macOS `F_FULLFSYNC` + `F_NOCACHE`. macOS'ta önbellekte zaten duran sayfalar yine önbellekten gelebilir (§18).
- **Bağımlılıklar:** `sha2` 0.11 (gezik-batch'te zaten, indirme denetimi), `sha1` 0.11 (zip'in AES'iyle zaten bağlı), `crc32fast` (flate2 ile zaten bağlı) gezik-ops'a da eklenir; derlenen yeni kod yalnız **`md-5` 0.11** (aynı RustCrypto `digest` 0.11 ailesi, ≈ 12 KB). BLAKE3 ağaçta yok, eklenmez (kullanıcı kararı 2).
- **İş parçacıkları:** özetleme iş motorunda koşar (§4.3); dosya başına bir iş parçacığı, sayısı `ops::threads::workers` (SSD'de en çok 4, dönen diskte 1, ağda 2): CPU'yu değil diski doyurmak yeter.
- **Çevrimiçi yalnız dosyalar** (OneDrive/iCloud yer tutucuları, 9b5): okumak indirir. Sağlama aracı bir kez sorar (`12 files are online-only. Download them to compute checksums?`); karşılaştırma ve yinelenenler bunları okumaz ve sayar (§7.4, §9.2).

### 3.3 Erişim: palet, menüler, `compare`

- Her araç bir `Action`'dır; palet `Action::ALL`'dan kendiliğinden listeler. Kayıtlı eşitleme profilleri palette `Sync: <name>` öğeleridir (sekme setleri gibi, §8.4).
- **Sağ tık menüsü** (satır ve boşluk; Windows'ta Gezik'in üstteki öğeleriyle): yeni `Tools ▸` alt menüsü: `Compare`, `Calculate Checksums…`, `Verify Checksums` (yalnız sağlama dosyasında), `Find Duplicates`, `Size Map`, `Export List…`, `Copy To…`, `Move To…`, `Split File…` (tek dosya), `Join Files` (yalnız `.001`/`.crc`). Uygun olmayan öğe gösterilmez (devre dışı bırakılmaz).
- **macOS menü çubuğu:** Go ile Commands arasına `Tools` menüsü, aynı öğeler.
- **Windows/Linux:** menü çubuğu yok; sağ tık `Tools ▸` ve palet. View düğmesinin menüsü görünümle ilgili kalır.
- **`compare` tek eylemdir, seçime göre karar verir** (§7.1 tablosu): iki dosya → fark (11c), iki klasör ya da iki bölme → klasör karşılaştırma (11d). Windows/Linux **Shift+F2** (TC'nin "Compare dirs"i, bugün boş), macOS **⌃⌘C** (boş; ⌃⌘ dizisi 10b'den). Öbür araçların varsayılan tuşu yok.

### 3.4 Hafiflik kuralı (her parça)

1. Açılana kadar: araç başına tek bir `Option` alanı ve kod; model, iş parçacığı, tampon, izleyici yok. Ayar anahtarları ayar yüklenirken okunur (dosya erişimi yok).
2. Açıkken: tarama arka planda, düşük öncelikli (8a'nın `gezik-search::walk` kuralları: düşük CPU ve G/Ç önceliği, bağlantı izlenmez, Unix'te tek dosya sistemi, bulut klasörüne girilmez); sonuç modelleri sanal liste (`ListView`) ile yalnız görünen satırlar.
3. Kapanınca: iptal bayrağı, iş parçacıkları en geç 100 ms'de durur, modeller ve tamponlar düşer (zayıf tutamak testi, bellek taban + 0,3 MB).
4. İzleme yok: sonuçlar bayatlayabilir; Gezik'in kendi işleri dokunduğu satırları "değişmiş olabilir" diye işaretler (iş motorunun `Changed`/`Outcome`'larından), yeniden tarama kullanıcının (`Compare Again`, F5/Ctrl+R katmanda).

## 4. Sağlama toplamları (11a)

### 4.1 Hesaplama

- Eylem `checksums` (`Calculate Checksums…`): seçimin dosyaları; seçili klasörlerin altındaki bütün dosyalar (8a'nın yürüyüşü, gizliler görünüm ayarına göre), göreli yollarıyla. Seçim yoksa odaktaki öğe.
- Katman (`widgets/checksums.slint`): üstte algoritma kutuları (`CRC32`, `MD5`, `SHA-1`, `SHA-256`, `SHA-512`; varsayılan yalnız SHA-256, son seçim `state.toml` `[checksums] algorithms`), altında satırlar: ad (göreli yol), boyut, seçili her algoritma için bir sütun (küçük harf onaltılık, tek aralıklı yazı tipi), durum. Kutu değişince yalnız eksik algoritmalar için dosyalar yeniden okunur.
- İlerleme katmanın başlığında ve iş panelinde (iş motoru işi, §4.3).
- **Beklenen değer:** `Expected` alanı. Katman açılırken pano tek bir onaltılık değer tutuyorsa (8, 32, 40, 64, 128 hane) alan onunla dolar. Alan doluyken her satırın eşleşen sütunu yeşil ✓ ya da kırmızı ✗; tek dosyada durum satırı `Matches the expected SHA-256` / `Does not match`.
- **Düğmeler:** `Copy` (seçili satırlar, yoksa hepsi, GNU biçiminde: `<hex>  <yol>`; tek satır tek sütunda yalnız değer), `Save` (§4.2), `Export CSV` (sütunlar `Path,Size,CRC32,MD5,…`; §11.2'nin CSV yazıcısı burada doğar), `Close`.

### 4.2 Sağlama dosyası yazma

- `Save` her seçili algoritma için bir dosya yazar: GNU biçimi (`<hex> *<yol>`, ikili işaretiyle; yollar dosyaya göre göreli, `/` ile; satır sonu LF; UTF-8 BOM'suz). CRC32 için `.sfv` (`<yol> <HEX>`).
- Ad ve yer: seçim tek dosyaysa `<ad>.<uzantı>.sha256` yanında; birden çok öğede gösterilen klasörde `<klasör adı>.sha256`. Ad alanı soru penceresinde düzenlenebilir; var olan ad çakışma listesine gider. Yazma bir iş (`NewFile` gibi), geri alınabilir, yazılan dosya seçilir.

### 4.3 İş motorunda özetleme

- `HashTask` (`gezik-ops/src/tasks/hash.rs`, yeni `TaskKind::Checksum`): `plan` dosyaları listeler, `run` özetler ve sonucu katmanın paylaşılan kabına (`Arc<Mutex<Vec<Row>>>`) yazar, `Outcome::Nothing` döner. **Geri alma geçmişine girmez** (bugünkü kural: tersi boş iş kaydedilmez; testle korunur).
- İş motorundan geçmenin kazancı: sürücü kuyruğu (aynı diske kopyalayan bir işle yarışmaz), duraklat/devam/iptal, iş panelinde ilerleme ve ETA. Katman kapanırsa iş iptal edilir.

### 4.4 Doğrulama

- Eylem `verify-checksums` (`Verify Checksums`): seçili sağlama dosyası; ayrıca bir sağlama dosyasına çift tık **açmaz** (bugünkü açma davranışı kalır), yalnız sağ tıkta ve palette.
- **Tanınan dosyalar:** uzantı `.md5`, `.sha1`, `.sha256`, `.sha512`, `.sfv`, `.crc` (TC, §12.4) ve adlar `MD5SUMS`, `SHA1SUMS`, `SHA256SUMS`, `SHA512SUMS` (büyük/küçük harf duyarsız). Algoritma uzantıdan/addan, bilinmezse onaltılık değerin uzunluğundan.
- **Satır biçimleri:** GNU (`<hex>  <yol>`, `<hex> *<yol>`, ters bölü kaçışlı `\`-önekli satırlar), BSD (`SHA256 (<yol>) = <hex>`), SFV (`<yol> <crc>`, `;` yorum). UTF-8 BOM ve CRLF kabul. Bozuk satır atlanır ve sayılır (`3 lines could not be read`).
- **Yollar:** sağlama dosyasının klasörüne göre çözülür; Windows'ta `/` ve `\` ikisi de ayırıcı. Klasörün dışına çıkan girdi (mutlak yol, sürücü ya da paylaşım öneki, `..`) okunmaz, satırı `UNREADABLE` (`Outside the folder`); yalnız düzenli dosyalar okunur (aygıt, FIFO değil). *Gerekçe (11a gözden geçirmesi; ilk hâlin "mutlak yol ve `..` kabul edilir" kuralını tersine çevirir):* indirilmiş bir sağlama dosyasının `\\host\x` satırı Windows'un kullanıcının NTLM özetini o makineye göndermesine, `CON` ya da `/dev/zero` satırı doğrulamanın asılı kalmasına yol açardı.
- Sonuç aynı katmanda: satırlar `OK`, `FAILED`, `MISSING`, `UNREADABLE`; üstte sayılar (`412 OK · 1 failed · 2 missing`); göster süzgeci (`Failed and missing only`), CSV.

### 4.5 Kabul ölçütleri

- Test vektörleri: RFC 1321 (MD5), FIPS 180-4 örnekleri (SHA-1, SHA-256, SHA-512), CRC32 "123456789" = `CBF43926`; boş dosya; 1 MiB sınırında ve 4 GiB üstü dosya (seyrek dosya, birim testinde).
- Gezik'in yazdığı `.sha256` Linux'ta `sha256sum -c`, macOS'ta `shasum -a 256 -c` ile geçer; onların yazdığı Gezik'te doğrulanır (elle denetim).
- Hız: 4 GB tek dosya SSD'de SHA-256, okuma hızının ≥ %90'ı (`scripts/perf/tools.ps1`).

## 5. Kopya işinin ekleri: doğrulama, hız sınırı, kuyruk (11b)

### 5.1 Doğrulama

- **Ayar:** `[files] verify = false`. Yeni her kopya/taşıma işi değeri buradan alır.
- **İş başına:** F5/F6 ve `Copy To…` sorusunda `Verify copies` kutusu (ayarın değeriyle); iş panelinde işin `⋯` menüsünde `Verify Copies` ✓ (işlerken değişebilir; değişiklik henüz kopyalanmamış dosyalara uygulanır). Yapıştırma ve sürükle-bırak ayarın değerini alır.
- **Ne zaman:** `RunCx::copy_file` bir dosyayı bitirince (geçici addan son ada geçmeden önce): `hash::same_content(kaynak, hedef, uncached_b = true)`. Kopya ve taşıma (sürücüler arası, kopyala+sil) işleri, eşitleme işinin kopyaları (11e), süzgeçli kopya (11i). Aynı sürücüde yeniden adlandırarak taşıma veri kopyalamaz, doğrulanmaz. macOS APFS ve Linux `FICLONE` ile klonlanan dosya aynı blokları paylaşır, doğrulanmaz (`copy_file` klonladığını döner; küçük platform değişikliği).
- **Uyuşmazlık:** hedef (ya da geçici kopya) silinir, dosya **bir kez** yeniden kopyalanır ve doğrulanır; yine uyuşmazsa başarısızlık: `The copy differs from the original at byte 1,234,567`. Taşımada kaynak doğrulama geçmeden silinmez (taşımanın silme adımı doğrulamadan sonra).
- **İlerleme:** doğrulama açıkken işin toplam baytı iki katı sayılır (kopya + doğrulama okuması), ETA dürüst kalır; satırda `Verifying…`.
- Bellek: doğrulayan işçi başına iki 1 MiB tampon, dosya bitince bırakılmaz, iş bitince bırakılır.

### 5.2 Hız sınırı

- **Ayar:** `[files] max-speed = ""` (`"20 MB/s"`, `"500 KB/s"`, `"1.5 GB/s"`; birimler ondalık: 1 MB = 1.000.000 B; boş = sınırsız). Hatalı değer bugünkü uyarı kalıbıyla atlanır.
- **İş başına:** iş panelinde `⋯` ▸ `Speed Limit ▸ None, 1, 5, 10, 25, 50, 100 MB/s` (işlerken değişir).
- **Uygulama:** iş başına bir kova (1 saniyelik kapasite) işin bütün işçileri arasında paylaşılır (`Mutex<Bucket>` `Job`'da). Kopyanın ilerleme geri çağrısı kovadan bayt alır, yetmezse en çok 100 ms'lik dilimlerle uyur (duraklat/iptal 100 ms içinde duyulur). Windows'ta `CopyFileExW`'nin ilerleme geri çağrısında uyumak kopyayı bekletir; Unix'te `copy_file_range`/tamponlu döngü sınır varken 1 MiB parçalarla döner. Klonlama veri taşımadığı için sınırlanmaz. Doğrulama okumaları da sınıra sayılmaz (hedefin ağına değil, yerel okumaya; ağ hedefinde de sayılmaz, sade tutulur, §19).
- Sınır kopya işlerine (kopya, taşıma, eşitleme, süzgeçli kopya, bölme/birleştirme) uygulanır; çıkarma, sıkıştırma, silme değil.

### 5.3 Kuyruk düzeni

- Bugün işler sürücü başına kuyrukta ekleme sırasıyla bekler (`Shared::wait_turn`: aynı sürücüyü paylaşan daha önceki bitmemiş iş). `Start Now` var.
- Yeni: bekleyen (`JobState::Waiting`) bir işin `⋯` menüsünde `Move Up` ve `Move Down`: iş, `jobs` listesinde aynı sürücüyü paylaşan bir önceki/sonraki **başlamamış** işle yer değiştirir; `turn` koşulu uyandırılır. Çalışan iş yer değiştirmez. Engine API: `move_in_queue(job, up: bool)`, `set_verify(job, bool)`, `set_speed(job, Option<u64>)`.
- İş paneli satırları bugünkü sırada (ekleme sırası) kalır; yalnız bekleyenlerin sırası değişir ve satırlar yeniden sıralanır.

## 6. Dosya farkı (11c)

### 6.1 Açılış

- `compare` iki dosyada (§7.1). İki dosyadan biri ikili (ilk 8 KB'ta NUL baytı) ya da biri 16 MB'tan büyükse ya da 1.000.000 satırdan çoksa metin farkı yapılmaz: bayt karşılaştırması (`hash::same_content`, ilerleme durum satırında) ve soru penceresi: `The files are identical` ya da `The files differ from byte 1,234,567 (of 2.1 MB)`; dış araç ayarlıysa `Open in WinMerge` düğmesi.
- **Dış araç ayarlıysa** (`[tools] diff`, §6.4) `compare` doğrudan onu açar (iç katman açılmaz).

### 6.2 Okuma ve hizalama

- Kodlama: BOM, yoksa gezik-batch'in metin dönüştürmesindeki tahmin (`chardetng` + `encoding_rs`, zaten bağlı). Satır sonları farkı yok sayılır (CRLF/LF/CR); başlıkta iki dosyanın kodlaması ve satır sonu yazılır, farklıysa vurgulu (`UTF-8 · CRLF` ↔ `UTF-8 · LF`).
- Satırlar sayılara çevrilir (aynı metin aynı sayı, `HashMap<&str, u32>`); ortak baş ve son kırpılır; kalan kısım **Myers O(ND)** farkı (`gezik-core/src/diff.rs`, yeni, saf). D > 20.000 olursa hizalama bırakılır: `The files are too different to line up` ve dış araç önerisi (her iki dosya yine yan yana, hizasız gösterilir).
- **Satır içi:** değişen satır çiftlerinde (bir silme bloğuyla bir ekleme bloğunun aynı sıradaki satırları) ortak baş ve son karakterler bulunur, aradaki tek aralık vurgulanır (ucuz; kelime farkı yok).
- **Seçenekler:** `Ignore whitespace` (satır içindeki boşluk dizileri tek boşluk, baş/son boşluk yok), `Ignore case`. Son seçim `state.toml` `[diff]`.

### 6.3 Katman

```
┌ Compare  report-v1.txt  ↔  report-v2.txt            [Ignore whitespace] [Ignore case]  3 changes  [↑] [↓] [✕]
│  UTF-8 · CRLF                                         UTF-8 · LF
│  12  The quick brown fox                         12  The quick brown fox
│  13  jumps over the lazy dog                     13  jumps over the [lazy] cat          (değişti)
│  14  ───────────────────────                     14  A new line                         (eklendi)
└ …
```

- Tek sanal liste (`ListView`), her satırda sol satır no + metin, sağ satır no + metin, tür (`same`, `changed`, `added`, `removed`). İki taraf aynı satırda olduğu için kaydırma kendiliğinden eşli. Renkler bugünkü tema değerlerinden (`Theme.success`/`Theme.danger`/`Theme.accent`'in %20'si arka plan; yeni tema anahtarı yok).
- Sekmeler 4 boşluğa açılır; 4.000 karakterden uzun satır gösterimde `…` ile kesilir (fark tam satırda hesaplanır).
- **Klavye:** Alt+↓ / Alt+↑ (macOS ⌥↓/⌥↑) sonraki/önceki değişiklik, ↑↓ PgUp PgDn Home End, Ctrl+C odaktaki satırın iki tarafı (`< sol`/`> sağ`), F5 / Ctrl+R (⌘R) yeniden oku, Esc kapat.
- Salt okunur: düzenleme, birleştirme yok (dış araç için §6.4).

### 6.4 Dış araç

```toml
[tools]
diff = ""                         # an external compare program (WinMerge, Meld, Beyond Compare, FileMerge's opendiff…); empty = Gezik's own
diff-args = ["{left}", "{right}"] # its arguments; {left} and {right} become the two paths
folder-diff = false               # also hand it two folders (otherwise Gezik compares folders itself)
```

- `diff` tam yol ya da PATH'te bir ad (`meld`, `opendiff`, `bcomp`); macOS'ta `.app` yolu `open -n -a <app> --args …` ile başlar. Program bulunamazsa durum satırı `The compare program "…" was not found; using Gezik's own` ve iç katman açılır.
- Süreç bugünkü `ChildProcess` yoluyla başlar (terminal ve 7-Zip gibi), Gezik beklemez.
- `folder-diff = true` iken iki klasör de dış araca gider; değilse klasörleri Gezik karşılaştırır (§7).

## 7. Klasör karşılaştırma (11d)

### 7.1 `compare`'in seçimi

| Durum | Sonuç |
|---|---|
| Etkin bölmede iki dosya seçili | Dosya farkı (§6) |
| Etkin bölmede iki klasör seçili | Klasör karşılaştırma: ilk seçilen sol |
| İki bölme açık, her bölmede tek öğe seçili (ikisi dosya / ikisi klasör) | Fark / klasör karşılaştırma; sol bölmeninki sol |
| İki bölme açık, başka her durum | İki bölmenin gösterdiği klasörler (sol bölme sol) |
| Tek bölme, yukarıdakilerden hiçbiri | Durum satırı: `Select two files or two folders, or open the second pane (F3)` |
| Bir taraf klasör, öbürü dosya; ya da biri arama sonucu, This PC, çöp | `Compare needs two files or two folders` |

Karşılaştırma katmanı açıkken sağ ve sol yollar katmanın üstündeki iki yol alanından değiştirilebilir (path completion bugünkü adres çubuğununki) ve `⇄` ile yer değiştirir.

### 7.2 Katman

```
┌ Compare  C:\Work\foto  ⇄  E:\Yedek\foto                  [Two-way ▾] [Options ▾] [Compare Again] [Synchronize…] [✕]
│ Show: [→ 12] [= 340] [≠ 3] [← 5] [1× 9] [2× 360]   Filter: [*.jpg *.cr2 | thumbs\          ]
│ Name                         Size     Modified            ⇄   Size     Modified
│ ▾ 2024\                                                    ≠
│     IMG_0001.jpg             3.2 MB   2024-05-01 10:00    →   —        —
│     IMG_0002.jpg             3.1 MB   2024-05-01 10:01    ≠   3.0 MB   2024-05-01 10:01
│ ▸ 2025\  (only left, 1,204 files, 3.4 GB)                  →
└ 412 files · 12 → · 5 ← · 3 ≠ · 9 only on one side · Selected 3 (9.4 MB)
```

- **Satırlar:** göreli yola göre ağaç sırası, klasör satırları 10d'nin grup başlıkları gibi kapanır (▾/▸); yalnız bir tarafta olan klasör tek satırdır (içindekiler sayılır, açılınca gösterilir). Sütunlar: ad, sol boyut, sol tarih, orta durum/yön simgesi, sağ boyut, sağ tarih. Tarih biçimi `[view] date-format`.
- **Durumlar** (`gezik-core::compare::State`): `LeftOnly`, `RightOnly`, `Equal`, `LeftNewer`, `RightNewer`, `Differs` (aynı tarih, farklı boyut ya da içerik), `Mismatch` (bir yanda dosya, öbüründe klasör), `Unreadable`. Simgeler: `→` (sol yeni ya da yalnız solda), `←`, `=`, `≠`, `!` (uyuşmazlık, okunamayan). 11d'de orta sütun durumu gösterir; 11e'de satırın **yönünü** (eşitleme kararı) gösterir ve tıklanabilir olur.
- **Göster düğmeleri** (TC): `→`, `=`, `≠`, `←` durum süzgeçleri; `1×` (yalnız bir tarafta olanlar), `2×` (iki tarafta olanlar). Her düğme sayısını taşır, aç/kapa. Varsayılan: `=` kapalı, diğerleri açık (eşitler kalabalık eder). Son seçim `state.toml` `[compare] show`.
- **Seçim ve klavye:** satırlar seçilir (tık, Shift, Ctrl, Ctrl+A), ↑↓ PgUp PgDn, → / ← klasör aç/kapa, Enter odaktaki çift dosyada fark (§6), Esc kapat.
- **Satır menüsü:** `Compare Files` (iki tarafta dosya), `Show in Left Pane` / `Show in Right Pane` (§7.5), `Copy Path ▸ Left / Right`; 11e'de yön öğeleri eklenir.

### 7.3 Seçenekler (`Options ▾`)

| Seçenek | Varsayılan | Anlam |
|---|---|---|
| `Subfolders` | açık | Alt klasörlere iner |
| `Compare content` | kapalı | Boyutu eşit çiftlerde baytlar karşılaştırılır (§7.4) |
| `Ignore dates` | kapalı | Yalnız boyut (ve içerik açıksa içerik) |
| `Ignore 1-hour difference` | kapalı | Tam 1 saat (±2 sn) fark eşit sayılır: FAT'te yaz saati kayması |
| `Case-sensitive names` | Linux açık, Windows/macOS kapalı | Ad eşlemesi |
| `Filter` | boş | §7.6 sözdizimi |

- FAT/exFAT'in **2 saniye** adımı her zaman tolerans (bugünkü `conflict::same_time`; ayar değil).
- Gizli öğeler görünüm ayarına (`show-hidden`, `show-system`) uyar.
- Son seçenekler `state.toml` `[compare]`; profil (11e) kendi seçeneklerini taşır.

### 7.4 Tarama ve içerik karşılaştırma

- İki taraf **aynı anda** iki `gezik-search::walk` ile yürünür (her biri sürücü türüne göre iş parçacığı sayısıyla; aynı fiziksel diskteyse ikisi sırayla — `DriveFacts`'in disk kimliği). Bağlantılar izlenmez; bağlantılar karşılaştırılmaz ve sayılır (`14 links left out`).
- Bellek düzeni: taraf başına `Vec<Node>`; düğüm = üst klasör indeksi (`u32`), ad (`Box<str>`), boyut, değiştirme zamanı (`i64`), bayraklar. Göreli yol saklanmaz. 100.000 dosyada taraf başına ≈ 5 MB.
- **Ad eşleme:** her klasörün çocukları ada göre sıralı birleştirilir (merge join). Ad karşılaştırması: Unicode NFC (macOS'un NFD adları ile FAT/NTFS'in NFC adları eşleşsin; `icu_normalizer`, Slint ile zaten bağlı; yalnız ASCII dışı adlarda çalışır) ve harf duyarsızken `pattern::fold_text`.
- Karşılaştırma saf işlevdir (`gezik-core/src/compare.rs`): iki ağaçtan satırlar ve sayılar; 100.000 çiftte ≤ 200 ms (birim süre testi).
- **İçerik:** `Compare content` açıkken boyutu eşit ve tarihi farklı (ya da `Ignore dates` açıkken boyutu eşit) her çift `hash::same_content` ile okunur: arka planda, katmanın ilerleme çubuğuyla (`Comparing content… 1.2 GB of 8.4 GB`), durdurulabilir. İçeriği aynı çıkan çift `Equal` olur (tarihi farklı da olsa; satırda küçük `dates differ` notu). Çevrimiçi yalnız dosyalar okunmaz, boyut/tarihle kalır ve sayılır.
- Okuma hataları satırda `Unreadable` (nedeni ipucunda); 8a'nın "N folders could not be read" sayısı üst satırda.
- **Üst sınır:** taraf başına 2.000.000 öğe; aşılınca tarama durur: `More than 2,000,000 items; narrow the filter or compare subfolders`.

### 7.5 Bölmelerle ilişki

- `Show in Left Pane` / `Show in Right Pane`: iki bölme kapalıysa açılır (10b), o bölme satırın klasörüne gider ve öğeyi seçer; katman **gizlenir**, sonuçlar bellekte kalır ve durum satırında bir çip görünür: `Compare: foto (3 ≠)`. Çipe tık katmanı geri getirir; çipin ✕'i sonuçları bırakır. Gezik'in işlerinin dokunduğu satırlar geri dönüşte `may have changed` işaretlidir.
- Eylem `select-differences` (iki bölmede; TC'nin "Compare directories"i; tuş yok; palet ve View menüsü değil sağ tık `Tools ▸`): **alt klasörsüz**, yalnız iki bölmenin gösterilen listeleriyle (G/Ç yok; zaten okunmuş): iki bölmede, öbüründe olmayan ya da daha yeni olan öğeleri seçer. Durum satırı `Selected 12 on the left, 5 on the right`.

### 7.6 Süzgeç sözdizimi (11d'de doğar, 11i de kullanır)

`gezik-core/src/file_filter.rs` (yeni, saf). 6a'nın desen dili (`*`, `?`, büyük/küçük harf duyarsız) üzerine:

- Boşluk ya da `;` ile ayrılmış desenler: **içerilenler**. Hiç içerilen yoksa hepsi.
- `|`'dan sonrakiler **dışlananlar** (TC).
- `/` ya da `\` ile biten desen klasöre uygulanır: içerilenlerde "yalnız bu klasörlere in", dışlananlarda "bu klasörlere girme" (girilmeyen klasör hiç okunmaz).
- `A>B` (11i, yalnız kopya): A'ya uyan her dosya ve aynı klasörde aynı kök adlı B'ye uyanlar (§12.2). Karşılaştırmada `>` hatadır (`Related files (>) work only when copying`).
- Örnek: `*.jpg *.cr2 | thumbs\ .git\ *.tmp`.

## 8. Eşitleme (11e)

### 8.1 Kipler ve yönler

`Mode ▾` (katmanın üstünde):

| Kip | Yalnız solda | Yalnız sağda | Sol yeni | Sağ yeni | `≠` (aynı tarih, farklı) | `!` |
|---|---|---|---|---|---|---|
| **Two-way** (varsayılan) | → kopyala | ← kopyala | → | ← | yok (kullanıcı seçer) | yok |
| **Left to right** (güncelle) | → | yok | → | yok (`≠` gibi uyarılı) | yok | yok |
| **Mirror** (TC asimetrik, yedek) | → | **× sağda çöpe** | → | → (sol kazanır) | → | yok |

- Sağdan sola güncelleme/ayna için `⇄` iki tarafı değiştirir (kip adları hep soldan sağa okunur).
- **Satır başına yön:** orta sütun yön okunu gösterir (`→`, `←`, `×→` sağda sil, `←×` solda sil, boş = hiçbir şey). Oka tıklamak döndürür: `→`, `←`, boş (yalnız bir taraftaysa: kopyala, sil, boş). Satır menüsü: `Copy Left to Right`, `Copy Right to Left`, `Do Nothing`, `Delete on Left`, `Delete on Right` (çöpe), `Reset to Default`. Seçili birden çok satıra uygulanır. Klavye (seçili satırlara): **Ctrl+→ / Ctrl+←** yön, **Delete** silme yönü (kopyalama değil, yalnız işaret), **Space** boş; macOS'ta ⌘→ / ⌘←.
- Kullanıcının değiştirdiği yön satırda kalın gösterilir; `Compare Again` kullanıcı yönlerini sıfırlar (yeniden tarama yeni gerçeklik; uyarı satırı `Your direction choices were reset`).
- Klasör satırının yönü altındaki bütün satırlara uygulanır; yalnız bir tarafta olan klasör **tek öğe** olarak kopyalanır ya da silinir (içindekiler tek tek iş öğesi olmaz).

### 8.2 Uygulama

- `Synchronize…` (Ctrl+Enter / ⌘↩) özet sorusunu açar:
  ```
  Synchronize C:\Work\foto and E:\Yedek\foto
    Copy 120 files (3.2 GB) to E:\Yedek\foto, replacing 14
    Copy 4 files (12 MB) to C:\Work\foto
    Move 7 items on E:\Yedek\foto to the Recycle Bin
  [x] Verify copies          (11b, [files] verify'dan)
  [Synchronize]  [Cancel]
  ```
- **Tek iş, tek geri alma:** `Engine::submit_chain([CopyTask L→R, CopyTask R→L, TrashTask], label "Synchronize foto")`. `CopyTask::pairs(pairs, presets)` (yeni kurucu: açık kaynak→hedef çiftleri, üzerine yazılacaklara `Decision::Replace` önayarı; ara klasörler `MadeParent` olarak). Üzerine yazılan dosya bugünkü `Replace` yolundan **çöpe** gider (`Outcome::Trashed`), silinenler `TrashTask` ile çöpe: Ctrl+Z hepsini geri getirir.
- **Çöpü olmayan sürücü** (ağ paylaşımı, bazı çıkarılabilir sürücüler; `RunCx::has_trash`): kullanıcı kararı "silmeler her zaman çöpe". Bu yüzden orada **silme yapılmaz**: özet `E:\ has no Recycle Bin: 7 deletions there are left out` der ve o satırlar iş dışında kalır. Üzerine yazma geri alınamaz olacağından özet ayrıca `14 files on E:\ would be replaced without a way back` der ve ayrı bir kutu ister (`[ ] Replace them anyway`); işaretlenmezse o satırlar atlanır.
- Önceden bilinen kararlar çakışma listesine düşmez (önayar); tarama ile uygulama arasında değişen hedef (yeni çıkan dosya) bugünkü çakışma listesine gelir.
- Akış sırası: önce sağa kopyalar, sonra sola, sonra silmeler (veri kaybı riski en sona). İş bitince katman yeniden taramaz; uygulanan satırlar `=` olur (iş `Report`'undan), başarısızlar `!` ve neden.
- **Durumsuz iki yönlü:** önceki eşitlemenin kaydı tutulmaz. Bir tarafta silinen dosya öbüründe duruyorsa iki yönlü kip onu geri kopyalar (`→`/`←`); silmeyi yaymak için kullanıcı satırın yönünü `×` yapar ya da ayna kipini kullanır. Bu, kipin ipucunda ve README'de yazılır (§19).

### 8.3 Hız ve doğrulama

Eşitleme işinin kopyaları 11b'nin doğrulamasını ve hız sınırını kullanır (aynı `CopyTask`). Kopya hızı aynı dosyaların F5'le kopyasıyla ±%3 (ölçüm).

### 8.4 Profiller

```toml
# Saved by Compare's "Save Profile…"; run from the palette ("Sync: Photos backup").
[[sync-profiles]]
name = "Photos backup"
left = "{pictures}/2024"
right = "E:/Yedek/foto"
mode = "mirror"            # two-way | left-to-right | mirror
subfolders = true
content = false
ignore-dates = false
ignore-hour = false
filter = "*.jpg *.cr2 | thumbs\\"
```

- `settings.toml`'da (kullanıcının yazdığı, taşınabilir; `[[tab-sets]]` ve `[[searches]]` gibi `settings_writer` ile yazılır). Katmanda `Save Profile…` (ad sorusu; aynı ad varsa üzerine yazmayı sorar) ve `Profiles ▾` (aç, sil).
- Palette `Sync: <name>`: katmanı profille açar ve **karşılaştırır**, uygulamaz (eşitleme her zaman özet sorusundan geçer). Kullanıcının satır yönleri profile yazılmaz.
- Hatalı profil bugünkü uyarı kalıbıyla atlanır (`sync-profiles[2]: mode: expected two-way, left-to-right or mirror`).

### 8.5 Klasör ↔ arşiv (yalnız karşılaştırma)

- Sağ yol bir arşiv dosyasıysa (zip, 7z, tar; sıkıştırılmış akış `tar.gz` hariç, onlar bütün okunmadan listelenemez) taraf arşivin listesinden kurulur (`archive::open(..).list(..)`; `Entry`'ye `modified: Option<SystemTime>` eklenir, zip/7z/tar bunu zaten okur). Karşılaştırma ad, boyut ve (varsa) tarihle; içerik yok.
- Arşiv tarafı **salt okunur**: yön okları ve `Synchronize…` kapalı, başlıkta `The archive is compared only; it is not changed`. Arşivden seçili girdileri çıkarmak 16. adımın VFS'iyle gelir.
- Bu kısım 11e'nin son görevidir; arşiv listelerinden tarih okumak beklenmedik iş çıkarırsa (format başına) o format "tarihsiz karşılaştırma" ile kalır ya da kısım PR açıklamasında ertelenir (karar 22).

## 9. Yinelenen dosyalar (11f)

### 9.1 Başlatma

- Eylem `find-duplicates` (`Find Duplicates`): kapsam seçili klasörler, yoksa gösterilen klasör (This PC'de seçili sürücüler). Arama çubuğu (8a) **yineleme kipinde** açılır: içerik alanı gizli, ad deseni, boyut aralığı, tür, tarih süzgeçleri bugünkü gibi; başlıkta `Find duplicates in C:\Work`. Enter başlatır.
- `SearchSpec` yeni alan alır: `duplicates: bool`. Sonuç `Location::Search` olduğu için sekme, geçmiş, Geri, kayıtlı aramalar (`[[searches]] duplicates = true`) çalışır; oturum geri yüklemede bugünkü arama kuralı (yeniden koşulmaz, `Run again`).

### 9.2 Aşamalar

1. **Yürüyüş** (8a'nın tarayıcısı): yalnız dosyalar, boyutları; boş dosyalar (0 B) alınmaz; bulut klasörüne girilmez, çevrimiçi yalnız dosyalar alınmaz ve sayılır. Öğe = (klasör indeksi, ad, boyut, zaman); klasör adları ayrı bir tabloda bir kez.
2. **Boyut grupları:** aynı boyutta tek dosya olanlar bırakılır (çoğu dosya burada düşer, bellek iner).
3. **Ön özet:** kalanların ilk 64 KB'ının SHA-256'sı (`hash_prefix`); 64 KB'tan küçükler için bu tam özettir.
4. **Tam özet:** ön özeti eşleşenlerin tamamı SHA-256 (bir grupta iki dosya varsa özet yerine `same_content`: çift okuma aynı, özet hesabı yok, çakışma yok).
5. **Sabit bağlantılar:** son gruplarda aynı dosya kimliği (Unix `dev`+`ino` metadatadan; Windows'ta grup üyesi dosyalar açılıp `GetFileInformationByHandle` dizin numarası) tek dosya sayılır; tek kalırsa grup düşer. Satırda `+1 hard link` notu.
- İlerleme arama çubuğunun bugünkü sayacında: `Scanned 120,400 files · comparing 3,210 of same size (4.2 GB)`. Durdurulabilir; durunca o ana kadar kesinleşen gruplar gösterilir.
- Aşama 3–4 dosya okur: düşük G/Ç önceliği, okuma iş parçacığı `threads::workers`. İş motoru işi **değildir** (diskte değişiklik yok, sonuç bölmenin; aramanın iş parçacığı yolu).

### 9.3 Sonuç listesi

- Bugünkü sonuç listesi, **yineleme gruplarıyla**: 10d'nin başlık satırları (`GroupBy::Duplicates`, iç değer; View ▸ Group By menüsünde görünmez). Başlık: `3 files × 12.4 MB · 24.8 MB could be freed`. Gruplar boşa giden bayta göre büyükten küçüğe; grup içi yola göre. Kapanır gruplar 10d'den.
- Sütunlar sonuç listesinin bugünkü sütunları (ad, klasör, boyut, tarih). Önizleme, Hızlı Bakış, sağ tık, sürükleme bugünkü gibi.
- **Seçim yardımcıları** (arama çubuğunda `Select ▾`, ayrıca sağ tık):
  - `All but the Newest in Each Group`, `All but the Oldest in Each Group`, `All but the First in Each Group` (yol sırası),
  - `All in This Folder` (odaktaki satırın klasöründeki bütün yinelemeler; "bu klasördekileri sil, öbürleri kalsın"),
  - `Keep This One` (odaktaki satırın grubunda o hariç hepsi).
  Yardımcılar hiçbir grupta **bütün dosyaları seçmez**: `All in This Folder` bir grubun bütün kopyalarını seçecekse o grubun sonuncusu seçilmez ve durum satırı söyler (`2 groups had every copy in this folder; one copy of each is kept`).
- **Silme:** Delete bugünkü gibi çöpe (iş motoru, geri alınabilir). Bir grupta bütün dosyalar seçiliyken Delete bir kez sorar (`This deletes every copy of 2 files`). Silinen satırlar sonuçtan düşer (8a'nın sonuçların taşınanı izlemesi); tek dosyası kalan grup listeden kalkar; Ctrl+Z geri getirince satır **geri gelmez** (arama yeniden koşulmaz; `Run again`).
- Durum satırı: `48 groups · 131 files · 1.8 GB could be freed`, seçimle `Selected 83 (1.7 GB)`.

## 10. Boyut haritası (11g)

### 10.1 Açılış ve tarama

- Eylem `size-map` (`Size Map`): seçili klasör, yoksa gösterilen klasör (This PC'de seçili sürücü).
- Tarama 8b'nin yürüyüşüyle (`gezik-search::size`'ın `Visit`'i genişletilir): **yalnız klasör düğümleri** tutulur: ad, alt klasörler, doğrudan dosyalarının toplam boyutu ve sayısı, alt ağacın toplamı. Dosyalar tek tek tutulmaz (düğüm ≈ 64 B + ad; 100.000 klasör ≈ 8 MB).
- Bulunan her klasör toplamı 8b'nin `SizeCache`'ine de yazılır (LRU sınırı 50.000 aynı): harita sonrası listenin boyut sütunu yeniden taramaz. Önbellekte taze toplamı olan klasör **yine de yürünür** (haritanın alt ağaca ihtiyacı var); taze toplam yalnız ilk çizimde ilerleme yerine kullanılır.
- Tarama sürerken harita kısmi toplamlarla çizilir (en çok saniyede 2 kez yeniden düzen).

### 10.2 Çizim

- Kare-yakın ağaç haritası (squarified, Bruls–Huizing–van Wijk; `gezik-core/src/treemap.rs`, yeni, saf). Gösterilen düzey: odaktaki klasörün çocukları; her alt klasör **bir düzey daha** iç içe bölünür (alt klasörleri + `files` bloğu). Odaktaki klasörün kendi dosyaları ayrı ayrı gösterilir: klasör açıkken bir kez `list_dir` (tek klasör okuması) ile alınır.
- Dikdörtgenler Slint modelinde (`[MapRect { x, y, w, h, color, label, size-text, depth }]`), **en çok 2.000**; 4×4 px'ten küçükler komşularıyla `N small items` bloğunda birleşir. Etiket 64×20 px'ten büyük dikdörtgenlerde (ad + boyut).
- Renk: klasörler derinliğe göre `Theme.accent`'in tonları; dosyalar `gezik-core::kind`'ın türüne göre altı renk (resim, video, ses, belge, arşiv, diğer) — sabit tablo, tema anahtarı yok; renk körlüğü için türler ayrıca ipucunda yazılır.
- Pencere boyutu değişince yeniden düzen (≤ 30 ms, 2.000 dikdörtgen).

### 10.3 Etkileşim

- Üstte yol parçaları (yakınlaşma yolu) ve `Up`; tık klasöre **yakınlaşır** (veri bellekte, okuma yok), Backspace/Alt+↑ üst düzey.
- Üzerine gelince durum satırı: `C:\Work\video\raw · 38.2 GB · 1,204 files · 12% of C:\Work`.
- **Enter / çift tık:** klasörse etkin bölme oraya gider ve harita kapanır; dosyaysa bölme dosyanın klasörüne gider ve dosyayı seçer.
- Sağ tık: `Open`, `Show in Pane`, `Move to Trash` (iş motoru, geri alınabilir; harita düğümün boyutunu düşer, yeniden taramaz), `Copy Path`.
- Klavye: ok tuşları dikdörtgenler arasında (en yakın komşu), Enter, Backspace, Esc kapat, F5 / Ctrl+R yeniden tara. Erişilebilirlik: odaktaki dikdörtgenin etiketi `accessible-label`.

## 11. Liste dışa aktarma (11h)

### 11.1 Soru penceresi

- Eylem `export-list` (`Export List…`): etkin bölmenin **gösterdiği** liste: klasör, arama sonuçları, düz görünüm, yinelenenler; bugünkü sıralama ve süzgeçle. Seçim varsa `Only the 12 selected items` kutusu (işaretli).
- Alanlar: `Format` (`Text`, `CSV`, `HTML`), `Text` için `Names` / `Full paths`; hedef yol (varsayılan: gösterilen klasörde `<klasör adı>.csv`, arama sonuçlarında aramanın kök klasöründe `<arama adı>.csv`; düzenlenebilir, path completion, uzantı biçimle değişir).
- Düğmeler: `Save` (Enter) ve `Copy`. Kaydetme bir iş (`NewFile` gibi, var olan ad çakışma listesine), geri alınabilir, yazılan dosya seçilir. Son biçim `state.toml` `[export]`.
- Alt klasörlerin listesi için kullanıcı önce düz görünümü açar (8a): ayrı "recursive" seçeneği yok.

### 11.2 Biçimler

- **Text:** satır başına ad ya da tam yol; Windows'ta CRLF, diğerlerinde LF; UTF-8 (BOM'suz).
- **CSV:** gösterilen sütunlar (ad, klasör (sonuçlarda), boyut, tarihler, tür …) + `Path`. Sayılar ham bayt, tarihler ISO 8601 yerel saat (`2024-05-01T10:00:12`), makine okunur; RFC 4180 tırnaklama. Ayırıcı: Windows'ta sistemin liste ayırıcısı (`GetLocaleInfoEx(LOCALE_SLIST)`; Türkçe Windows'ta `;`, Excel'in açtığı gibi), macOS/Linux'ta `,`. Windows'ta UTF-8 **BOM'lu** (Excel'in Türkçe adları doğru açması için), diğerlerinde BOM'suz. CSV yazıcısı 11a'nın (`gezik-core/src/csv.rs`).
- **HTML:** tek dosya, `<table>`, satır içi CSS (açık/koyu `prefers-color-scheme`), betik yok, her metin kaçışlı; başlıkta yol, tarih ve öğe sayısı. Boyut ve tarih gösterimdeki metinle.
- **Copy:** Text ve CSV düz metin olarak panoya; HTML de düz metin (kaynak) olarak (panoya HTML biçimi koymak platform işi, YAGNI).
- Yazma akışla (satır satır, 64 KB tampon): 100.000 satır ≤ 1 s, ≤ +1 MB.

## 12. Süzgeçli kopya, ad maskesi, bölme ve birleştirme (11i)

### 12.1 `Copy To…` ve `Move To…`

- Yeni eylemler `copy-to`, `move-to` (tuş yok; sağ tık `Tools ▸`, palet): 10b'nin F5/F6 sorusunun aynısı, tek bölmede de; hedef alanı boş başlar (iki bölmede öbür bölmenin klasörüyle). F5/F6 sorusu aynı bileşene geçer: her ikisinde aşağıdaki alanlar.
- Sorunun yeni alanları (`Options ▸` ile açılır, kapalı başlar; son açıklık `state.toml`):
  - `Only:` süzgeç (§7.6 sözdizimi + `>`),
  - `Verify copies` (11b),
  - hedef alanında **ad maskesi** (§12.3).
- Süzgeçsiz ve maskesiz davranış bugünkü F5/F6 ile aynı (aynı iş yolu).

### 12.2 Süzgeç uygulaması

- `CopyTask`/`MoveTask` yeni alan `filter: Option<FileFilter>`: `plan` yürüyüşü dışlanan klasörlere girmez, uymayan dosyaları atlar. Seçimin kendisi (üst düzey öğeler) de süzgeçten geçer: seçili bir dosya uymuyorsa kopyalanmaz (sayı soruda: `Copy 312 of 1,040 items`). Hesap soruda önceden yapılmaz (yürüyüş işin planında); soru yalnız seçimi sayar, iş paneli gerçek sayıyı söyler.
- **Boş klasör yok:** hedefte yalnız içine dosya gelen klasörler oluşur (`MadeParent`, geri almada boş kalırsa gider).
- **İlişkili dosyalar** `A>B`: A'ya uyan her dosya için aynı kaynak klasörde, aynı kök adlı (ilk noktaya kadar değil, **son** uzantıdan önceki ad) ve B'ye uyan dosyalar da plana girer (bir kez). Örnek: `IMG_1.jpg` seçili, süzgeç `*.jpg>*.cr2 *.xmp` → `IMG_1.cr2`, `IMG_1.xmp`.
- Taşımada süzgeç dışında kalan dosyalar yerinde kalır; kaynak klasör boşalmadıkça silinmez.

### 12.3 Ad maskesi

- Hedef alanının son parçası `*` ya da `?` içeriyorsa maske: `D:\Yedek\*.bak`, `*_old.*`, `????-*.jpg`. Maske yalnız dosya adlarına uygulanır (klasör adları korunur).
- Anlam (DOS/TC): ad ve maske son noktadan ikiye bölünür; maskenin ad kısmında `*` kaynağın adının kalanı, `?` kaynağın o konumdaki karakteri (yoksa hiçbir şey), diğer karakterler aynen; uzantı kısmı aynı kuralla. `*.*` adı değiştirmez. Saf işlev `gezik-core/src/ops/names.rs`'te (`apply_mask`), birim testleriyle (TC'nin belgelenmiş örnekleri).
- İki kaynak aynı hedef ada düşerse ikincisi çakışma listesine gider (bugünkü yol).

### 12.4 Bölme

- Eylem `split-file` (`Split File…`, tek dosya seçiliyken): soru penceresi: parça boyutu (alan + açılır liste: `100 MB`, `650 MB`, `700 MB`, `4,095 MB (FAT32)`, `4.7 GB (DVD)`; birimler 11b'ninki), hedef klasör (iki bölmede öbür bölme, değilse aynı klasör). Hedef FAT32 ise ve dosya 4 GiB'ı aşıyorsa varsayılan `4,095 MB` (`DriveFacts::max_file`).
- Çıktı: `<ad>.001`, `<ad>.002`, … (ham, başlıksız; `<ad>` uzantıyı içerir: `film.mkv.001`) ve `<ad>.crc` (TC biçimi: `filename=film.mkv`, `size=…`, `crc32=…`). CRC32 bölerken hesaplanır (aynı okuma).
- `SplitTask` (`gezik-batch/src/tasks/split.rs`): iş motoru işi, ilerleme, iptal (yarım parçalar silinir), geri alma çıktıları çöpe atar. 1 MiB tampon. Var olan parça adı çakışma listesine.

### 12.5 Birleştirme

- Eylem `join-files` (`Join Files`): seçili `.001` (ya da kümenin herhangi bir parçası ya da `.crc`). Küme `gezik-core::batch::archive::volume_set`'in ham `.NNN` kuralıyla; eksik parça varsa başlamadan durur: `Part film.mkv.004 is missing`.
- Hedef: iki bölmede öbür bölme, değilse aynı klasör; ad `.crc`'deki `filename`, yoksa `.001`'siz ad. Var olan ad çakışma listesine.
- Yazarken CRC32 ve boyut hesaplanır; `.crc` varsa karşılaştırılır. **Uyuşmazsa** çıktı silinir (yeni yapılmış, geri alma gerekmez) ve iş başarısız: `The joined file does not match film.mkv.crc (CRC or size); a part may be damaged`. `.crc` yoksa birleştirme yapılır ve raporda `No .crc file: the result was not checked` notu.
- `.7z.001`/`.zip.001` kümeleri arşivdir: birleştirme yine ham birleştirir (çalışır), ama sağ tıkta `Extract` (5. adım) önce gelir.
- `JoinTask` (aynı dosya), 1 MiB tampon; geri alma çıktıyı çöpe atar.

## 13. Ayarlar, eylemler, tuşlar, menüler

### 13.1 `settings.toml`

```toml
[files]
verify = false            # check every copied file against its original (reads both again; slower)
max-speed = ""            # copy speed limit for new jobs, e.g. "20 MB/s"; empty = no limit

[tools]
diff = ""                 # an external compare program; empty = Gezik's own (see §6.4)
diff-args = ["{left}", "{right}"]
folder-diff = false       # also hand it two folders

# [[sync-profiles]] — written by Compare's "Save Profile…" (see §8.4)
```

`state.toml` (Gezik yazar): `[checksums] algorithms`, `[diff] ignore-whitespace, ignore-case`, `[compare] show, subfolders, content, ignore-dates, ignore-hour, mode`, `[export] format, text`, `[copy-to] options-open`.

Hepsi `templates/settings.toml`'a yorumlarıyla ve `settings_writer`'ın bilinen anahtarlarına girer.

### 13.2 Eylemler ve varsayılan tuşlar

| Eylem | Windows / Linux | macOS | Parça |
|---|---|---|---|
| `checksums` | — | — | 11a |
| `verify-checksums` | — | — | 11a |
| `compare` | Shift+F2 | ⌃⌘C | 11c (dosyalar), 11d (klasörler) |
| `select-differences` | — | — | 11d |
| `find-duplicates` | — | — | 11f |
| `size-map` | — | — | 11g |
| `export-list` | — | — | 11h |
| `copy-to`, `move-to` | — | — | 11i |
| `split-file`, `join-files` | — | — | 11i |

- `Action` 102 → 104 (11a) → 105 (11c) → 106 (11d) → 107 (11f) → 108 (11g) → 109 (11h) → 113 (11i). Her parçada `assert_eq!(Action::ALL.len(), …)` satırları yeni sayıya.
- `shift+f2` (Windows/Linux) ve `mod+ctrl+c` (macOS) bugün boş; `fixed_owner` ve macOS menü sabitleriyle çakışmaz (test). ⌃⌘C macOS'un sistem kısayolu değildir.
- Katman içi tuşlar (Alt+↓/↑, Ctrl+→/←, Ctrl+Enter …) katmanın kendi klavyesidir, kısayol tablosuna girmez (bugünkü toplu ad katmanı gibi).

### 13.3 Menüler

- **Sağ tık `Tools ▸`** (satır ve boşluk, §3.3).
- **macOS menü çubuğu `Tools`:** `Compare`, `Select Differences`, ayraç, `Calculate Checksums…`, `Verify Checksums`, `Find Duplicates`, `Size Map`, ayraç, `Export List…`, `Copy To…`, `Move To…`, `Split File…`, `Join Files`.
- **İş paneli satırının `⋯` menüsü** (11b): `Verify Copies` ✓, `Speed Limit ▸`, `Move Up`, `Move Down`, `Start Now` (var olan).
- **Menü kimlikleri:** 11 için **2200–2399**: `TOOLS_*` 2200–2219 (alt menü öğeleri), karşılaştırma satır menüsü 2220–2249, eşitleme yönleri 2250–2259, profiller 2260–2299 (profil başına bir kimlik, en çok 40), boyut haritası menüsü 2300–2309, yineleme seçim yardımcıları 2310–2319, iş paneli menüsü 2320–2339 (hız değerleri dahil). `GEZIK_IDS_END = 4096` değişmez.

## 14. Kod yapısı

| Parça | Yer |
|---|---|
| Sağlama çekirdeği | `gezik-ops/src/hash.rs` (yeni), `gezik-platform/src/fs/{windows, unix}.rs` (`open_uncached`, sıralı okuma ipucu, `copy_file`'ın klon bilgisi) |
| Sağlama işi ve dosyaları | `gezik-ops/src/tasks/hash.rs` (yeni; `HashTask`), `gezik-core/src/checksum_file.rs` (yeni; GNU/BSD/SFV/CRC ayrıştırma ve yazma, saf), `gezik-core/src/csv.rs` (yeni, saf) |
| Sağlama katmanı | `gezik/src/checksums.rs` (yeni), `ui/widgets/checksums.slint` (yeni) |
| Doğrulama, hız, kuyruk | `gezik-ops/src/{task, engine, run}.rs`, `gezik-core/src/ops/rate.rs` (kova, saf), `gezik/src/operations.rs`, `ui/widgets/ops-panel.slint` |
| Fark | `gezik-core/src/diff.rs` (yeni; Myers, satır içi aralık, saf), `gezik/src/diff.rs` (yeni; okuma, kodlama, dış araç), `ui/widgets/diff.slint` (yeni) |
| Karşılaştırma | `gezik-core/src/compare.rs` (yeni; durumlar, kip varsayılanları, sayılar, saf), `gezik-core/src/file_filter.rs` (yeni, saf), `gezik-search/src/tree_scan.rs` (yeni; iki ağaç taraması, NFC), `gezik/src/compare.rs` (yeni), `ui/widgets/compare.slint` (yeni) |
| Eşitleme | `gezik-core/src/compare.rs` (yönler), `gezik/src/sync.rs` (yeni; özet, iş zinciri, profiller), `gezik-ops/src/tasks/copy.rs` (`CopyTask::pairs`), `gezik-config/src/sync_profiles.rs` (yeni; ayrıştırma ve yazma), `gezik-batch/src/archive/*` (`Entry::modified`) |
| Yinelenenler | `gezik-search/src/duplicates.rs` (yeni; aşamalar), `gezik-core/src/search.rs` (`SearchSpec::duplicates`), `gezik-core/src/group.rs` (`GroupBy::Duplicates`), `gezik/src/search.rs` (çubuk kipi, seçim yardımcıları) |
| Boyut haritası | `gezik-core/src/treemap.rs` (yeni; squarified, saf), `gezik-search/src/size.rs` (ağaç toplayan `Visit`), `gezik/src/size_map.rs` (yeni), `ui/widgets/size-map.slint` (yeni) |
| Dışa aktarma | `gezik/src/export.rs` (yeni), `gezik-core/src/csv.rs`, `gezik-platform/src/locale.rs` (`list_separator`) |
| Süzgeçli kopya | `gezik-ops/src/tasks/{copy, move_}.rs` (`filter`), `gezik-core/src/ops/names.rs` (`apply_mask`), `gezik/src/dual.rs` → soru bileşeni `gezik/src/copy_to.rs` (yeni; F5/F6 ve `Copy To…` ortak) |
| Bölme, birleştirme | `gezik-batch/src/tasks/split.rs` (yeni; `SplitTask`, `JoinTask`) |
| Ayarlar, eylemler, menüler | `gezik-config/src/{settings, settings_writer, shortcuts}.rs`, `templates/settings.toml`, `gezik/src/{actions, context_menu, menu_bar, palette}.rs`, `ui/app.slint` |

## 15. Bütçe ve performans

| Parça | Exe (tahmin; sınır ≤ +262.144 B) | Boşta bellek | Araç açıkken |
|---|---|---|---|
| 11a | ≈ +150 KB (`md-5` ≈ 12 KB, çekirdek ≈ 25 KB, katman ≈ 110 KB) | 0 | 10.000 satır ≤ +2 MB; iş parçacığı başına 1 MiB tampon; SHA-256 ≥ okuma hızının %90'ı; kapanınca taban + 0,3 MB |
| 11b | ≈ +60 KB | 0 | doğrulayan işçi başına +2 MiB (iş bitince bırakılır); sınır ±%5; doğrulama kapalıyken kopya hızı `master` ±%3 |
| 11c | ≈ +180 KB | 0 | iki 1 MB metin ≤ +8 MB; 10.000 satırlık fark ≤ 150 ms; 16 MB/1.000.000 satır sınırı |
| 11d | ≈ +220 KB | 0 | 100.000 çift ≤ +15 MB; tarama ≤ en uzun tek taraf × 1,1; karşılaştırma ≤ 200 ms; çipte bekleyen sonuç kullanıcının seçimi |
| 11e | ≈ +130 KB | 0 | profil başına ≤ 1 KB ayar; eşitleme kopyası F5 ±%3 |
| 11f | ≈ +100 KB | 0 | 100.000 dosya taramasında tepe ≤ +10 MB; sonuçta yalnız gruplar |
| 11g | ≈ +150 KB | 0 | 100.000 klasör ≤ +10 MB; ≤ 2.000 dikdörtgen; yeniden düzen ≤ 30 ms |
| 11h | ≈ +50 KB | 0 | akışla; 100.000 satır ≤ 1 s, ≤ +1 MB |
| 11i | ≈ +150 KB | 0 | bölme/birleştirme disk hızında (1 MiB tampon), CRC ek CPU ≤ %10; süzgeç yokken kopya `master` ±%3 |

Toplam ≈ +1,19 MB exe. Varsayılan ayarlarla boşta **0** (her parça ±0,02 MB ölçüm gürültüsü içinde), açılış ≤ +1 ms.

- **Ölçüm:** her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1`, `stress.ps1` ve `ops.ps1`'i yazar. Yeni betik `scripts/perf/tools.ps1`: 4 GB dosyada SHA-256/MD5 hızı, 100.000 dosyalık iki ağaç karşılaştırması (tarama, karşılaştırma, bellek), 100.000 dosyada yinelenen taraması, 100.000 klasörde boyut haritası, her aracın açılıp kapanınca bellek dönüşü. `ops.ps1`'e doğrulamalı/sınırlı kopya satırları.
- **Zamanlayıcı ve yoklama yok:** taramalar olayla biter; çip ve ilerleme bugünkü iş paneli/arama ilerleme yolundan (zaten olan uyanmalar); izleyici yok.
- **Yazılım çizicisi:** katmanlar sanal liste; boyut haritası en çok 2.000 dikdörtgen (tek tam kare ≤ 16 ms, 1280×800); fark katmanında satır içi vurgu tek dikdörtgen.

## 16. Parçalar (her biri bir plan ve bir PR)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **11a** | Sağlama çekirdeği (`hash.rs`, önbelleksiz okuma), `HashTask`, sağlama katmanı, sağlama dosyası yazma/doğrulama, CSV yazıcısı (§3.2, §4) | — |
| 2 | **11b** | Kopya doğrulama, hız sınırı, kuyruk düzeni, iş paneli `⋯` menüsü (§5) | 11a (`same_content`, `open_uncached`) |
| 3 | **11c** | Dosya farkı: iç katman, Myers, kodlama, dış araç; `compare` eylemi (yalnız dosyalar) (§6) | — (11a'nın `same_content`'i ikili dosyada; 11a'dan sonra) |
| 4 | **11d** | Klasör karşılaştırma: tarama, durumlar, göster düğmeleri, seçenekler, içerik karşılaştırma, süzgeç sözdizimi, çip, `select-differences`; `compare`'in klasör dalları (§7) | 11a, 11c (satırdan fark) |
| 5 | **11e** | Eşitleme: kipler, satır yönleri, özet, iş zinciri, çöpsüz sürücü kuralı, profiller, arşiv karşılaştırma (§8) | 11d, 11b (doğrulama kutusu) |
| 6 | **11f** | Yinelenen dosyalar: arama kipi, aşamalar, gruplu sonuç, seçim yardımcıları (§9) | 11a (`hash_prefix`, `same_content`) |
| 7 | **11g** | Boyut haritası (§10) | — |
| 8 | **11h** | Liste dışa aktarma (§11) | 11a (CSV yazıcısı) |
| 9 | **11i** | `Copy To…`/`Move To…`, süzgeçli kopya, ilişkili dosyalar, ad maskesi, bölme ve birleştirme (§12) | 11b (doğrulama kutusu), 11d (`file_filter`) |

Sıra bağımlılık ve değer: sağlama çekirdeği dört aracın temeli olduğu için ilk; doğrulama hemen ardından (küçük, iş motoruna dokunur, sonraki kopya işleri onu kullanır). Fark karşılaştırmadan önce gelir çünkü karşılaştırmanın satırı farkı açar. Karşılaştırma ve eşitleme en büyük değer ve en büyük iş (iki PR). 11f, 11g, 11h birbirinden bağımsızdır, sıra numarası tercih sırasıdır (11g 11a'yı da beklemez). 11i süzgeç sözdizimini 11d'den alır. Her parçada Windows → macOS → Linux sırasıyla yazılır; bir sistemde doğrulanamayan kısım PR açıklamasında "doğrulanacak" notuyla.

### 16.1 Parça ayrıntıları

Denetim listesi aralıkları plan yazılırken atanır (şu anki son numaralar: macOS 270, Linux 206); aşağıdaki sayılar parça başına beklenen madde sayısıdır.

**11a — sağlama çekirdeği ve sağlama toplamları**
- *Kapsam:* §3.2, §4; `checksums`, `verify-checksums`; `TaskKind::Checksum`; CSV yazıcısı.
- *Dosyalar:* `gezik-ops/src/{hash.rs, tasks/hash.rs}`, `gezik-platform/src/fs/*` (`open_uncached`, okuma ipuçları), `gezik-core/src/{checksum_file, csv}.rs`, `gezik/src/checksums.rs`, `ui/widgets/checksums.slint`, `gezik-ops/Cargo.toml` (`md-5`, `sha1`, `sha2`, `crc32fast`).
- *Bütçe:* exe ≈ +150 KB; boşta 0; §15 satırı.
- *Testler:* test vektörleri; `Digests` tek okumada çok algoritma; 4 GiB üstü seyrek dosya; iptal 100 ms içinde; GNU/BSD/SFV/`.crc` ayrıştırma (BOM, CRLF, `*`, `\` kaçış, bozuk satır sayımı, `..` ve mutlak yol); yazılan dosyanın gidiş-dönüşü; panodan beklenen değer (uzunluk tanıma); `HashTask` geri alma geçmişine girmez; çevrimiçi yalnız dosya sorusu (sahte bulut bilgisiyle); CSV kaçışı (virgül, tırnak, satır sonu); katman kapanınca zayıf tutamak düşer.
- *macOS/Linux:* `F_NOCACHE`/`posix_fadvise` yolları; `sha256sum -c` ve `shasum -a 256 -c` uyumu her iki yönde; Finder'da olmayan bir araç olduğu için Tools menüsü macOS menü çubuğunda ilk kez çıkar.
- *Denetim listesi:* macOS ≈ 6, Linux ≈ 4 madde.

**11b — doğrulama, hız sınırı, kuyruk**
- *Kapsam:* §5.
- *Dosyalar:* `gezik-ops/src/{task, engine, run, control}.rs`, `gezik-core/src/ops/rate.rs` (kova), `gezik-platform/src/fs/*` (`copy_file` klon bilgisi, Unix parçalı döngü), `gezik-config/src/settings.rs` (`verify`, `max-speed`), `gezik/src/{operations, dual}.rs`, `ui/widgets/ops-panel.slint`.
- *Bütçe:* exe ≈ +60 KB; boşta 0.
- *Testler:* doğrulama: bozulan hedef (testte kopyadan sonra bir bayt değiştiren kanca) yeniden kopyalanır, ikincisinde başarısız ve neden metni; taşımada kaynak doğrulama başarısızsa durur; klon kopyası doğrulanmaz; ilerleme toplamı iki kat. Hız sınırı: kova saf testleri (doluluk, paylaşım, 100 ms dilim); 50 MB'lık kopya 10 MB/s'te 5 sn ±%5 (sahte saatli birim + `ops.ps1` gerçek); duraklat/iptal sınırlıyken 100 ms içinde. Kuyruk: `move_in_queue` yalnız başlamamış ve aynı sürücüyü paylaşanlarla yer değiştirir; `wait_turn` yeni sırayı izler; çalışan iş yer değiştirmez. `max-speed` ayrıştırma ve uyarı.
- *macOS/Linux:* APFS `clonefile` ve Linux `FICLONE`'da doğrulama yok, sınır yok; ağ (SMB/NFS) hedefinde doğrulama okuması önbelleksiz; Linux'ta `copy_file_range`'in parçalı döngüsü.
- *Denetim listesi:* macOS ≈ 5, Linux ≈ 4.

**11c — dosya farkı**
- *Kapsam:* §6; `compare` eylemi (yalnız dosya dalları), Shift+F2 / ⌃⌘C.
- *Dosyalar:* `gezik-core/src/diff.rs`, `gezik/src/diff.rs`, `ui/widgets/diff.slint`, `gezik-config/src/{settings, shortcuts}.rs`, `gezik/src/{actions, context_menu, menu_bar}.rs`.
- *Bütçe:* exe ≈ +180 KB; boşta 0.
- *Testler:* Myers saf testleri (boş, aynı, tümü farklı, baş/son kırpma, D sınırı), satır içi aralık, boşluk/harf yok sayma; kodlama tanıma (UTF-16 BOM, Windows-1254, UTF-8 BOM'suz), satır sonu farkının yok sayılıp başlıkta söylenmesi; ikili ve büyük dosyada bayt karşılaştırması ve ilk fark konumu; dış araç argüman yerleştirme (`{left}`, `{right}`, boşluklu yollar), bulunamayan program geri düşüşü; 10.000 satırda ≤ 150 ms (süre sınırlı test); `compare` seçim tablosunun dosya satırları; kısayol testleri (`shift+f2`, `mod+ctrl+c` çakışmasız).
- *macOS/Linux:* `opendiff`, `.app` ile `open -n -a`, Linux'ta `meld`/`kdiff3` PATH'ten; ⌥↓/⌥↑ katman içinde.
- *Denetim listesi:* macOS ≈ 5, Linux ≈ 3.

**11d — klasör karşılaştırma**
- *Kapsam:* §7.
- *Dosyalar:* `gezik-core/src/{compare, file_filter}.rs`, `gezik-search/src/tree_scan.rs`, `gezik/src/compare.rs`, `ui/widgets/compare.slint`, `gezik/src/{dual, actions, context_menu}.rs` (`select-differences`, çip), durum satırı.
- *Bütçe:* exe ≈ +220 KB; boşta 0; 100.000 çift ≤ +15 MB.
- *Testler:* durum sınıflaması (her durum, FAT 2 sn, 1 saat, `Ignore dates`), içerik karşılaştırmanın `Equal`'a çevirmesi, göster süzgeçleri ve sayılar, harf duyarlılığı (Windows/macOS duyarsız, Linux duyarlı), NFC/NFD eşleşmesi, yalnız bir tarafta klasörün tek satır olması, süzgeç sözdizimi (`|`, klasör deseni, girilmeyen klasör okunmaz: sayaçla), 2.000.000 sınırı, `compare` seçim tablosunun her satırı, `select-differences` G/Ç yapmaz (sayaç), çip gizle/geri getir ve ✕ ile bırakma (zayıf tutamak), 100.000 çiftte ≤ 200 ms, katman kapanınca iş parçacıkları 100 ms içinde durur.
- *macOS/Linux:* APFS NFD adları ile exFAT USB'deki NFC adları eşleşir; Linux'ta `Docs` ve `docs` iki satır; iki taraf aynı fiziksel diskteyse sırayla taranır.
- *Denetim listesi:* macOS ≈ 7, Linux ≈ 5.

**11e — eşitleme ve profiller**
- *Kapsam:* §8.
- *Dosyalar:* `gezik-core/src/compare.rs` (yönler, kip varsayılanları), `gezik/src/sync.rs`, `gezik-ops/src/tasks/copy.rs` (`CopyTask::pairs`), `gezik-config/src/sync_profiles.rs`, `gezik-config/src/settings_writer.rs`, `gezik/src/palette.rs` (`Sync:` öğeleri), `gezik-batch/src/archive/*` (`Entry::modified`).
- *Bütçe:* exe ≈ +130 KB; boşta 0.
- *Testler:* kip tablosunun her hücresi (saf); satır yönünün döndürülmesi, çoklu seçime uygulanması, klasör yönünün alt satırlara inmesi, `Compare Again`'in sıfırlaması; özet sayıları; zincir tek geri alma (üzerine yazılan çöpten döner, silinen çöpten döner, kopyalanan gider); çöpsüz sürücüde silmelerin dışarıda kalması ve `Replace them anyway`; tarama ile uygulama arasında çıkan dosya çakışma listesine; durumsuz iki yönlünün silinmiş dosyayı geri kopyalaması (belgelenen davranış testi); profil ayrıştırma/yazma gidiş-dönüşü, uyarılar, paletten açınca uygulamadan karşılaştırma; arşiv tarafı salt okunur (oklar ve `Synchronize…` kapalı), zip/7z/tar tarihleri.
- *macOS/Linux:* çöp: macOS'ta harici diskin `.Trashes`'i, Linux'ta `.Trash-<uid>` (9b2'nin kuralları); ağ bağlamalarında çöp yok kuralı.
- *Denetim listesi:* macOS ≈ 7, Linux ≈ 5.

**11f — yinelenen dosyalar**
- *Kapsam:* §9.
- *Dosyalar:* `gezik-search/src/duplicates.rs`, `gezik-core/src/{search, group}.rs`, `gezik/src/search.rs`, `gezik-config` (`[[searches]] duplicates`).
- *Bütçe:* exe ≈ +100 KB; boşta 0; tepe ≤ +10 MB / 100.000 dosya.
- *Testler:* aşamalar (boyut eleme, ön özet, tam özet, iki üyeli grupta `same_content`), boş dosyalar dışarıda, sabit bağlantı tek sayılır (Unix `ino`, Windows dizin numarası), çevrimiçi yalnız dosyalar sayılır ve okunmaz, durdurunca kesinleşen gruplar; seçim yardımcılarının her biri ve "her kopyayı seçmeme" kuralı; Delete'in bütün kopyalar sorusu; silinen satırın düşmesi ve tek kalan grubun kalkması; geri alma; `[[searches]] duplicates` gidiş-dönüşü; 100.000 dosyada bellek tepesi (`tools.ps1`).
- *macOS/Linux:* iCloud yer tutucuları (`.icloud`) atlanır; APFS klonları ayrı dosya sayılır (iki kopya gerçekten yer tutmaz ama silmek de bozmaz; §19).
- *Denetim listesi:* macOS ≈ 5, Linux ≈ 4.

**11g — boyut haritası**
- *Kapsam:* §10.
- *Dosyalar:* `gezik-core/src/treemap.rs`, `gezik-search/src/size.rs`, `gezik/src/size_map.rs`, `ui/widgets/size-map.slint`.
- *Bütçe:* exe ≈ +150 KB; boşta 0; 100.000 klasör ≤ +10 MB.
- *Testler:* squarified düzen (alanların toplamı, en-boy oranı sınırı, sıfır boyutlular, tek öğe), 2.000 dikdörtgen sınırı ve küçüklerin birleşmesi, etiket eşiği; toplamların 8b önbelleğine yazılması; kısmi toplamla çizim; yakınlaşma okumasız (sayaç); Enter'in bölmeyi götürmesi; çöpe atmanın boyutu düşmesi; ok tuşu komşuları; yeniden düzen ≤ 30 ms.
- *macOS/Linux:* Retina'da 2× ölçekte dikdörtgen kenarları; Linux'ta `/` taramasında tek dosya sistemi kuralı (`/proc`, `/sys` yok).
- *Denetim listesi:* macOS ≈ 4, Linux ≈ 3.

**11h — liste dışa aktarma**
- *Kapsam:* §11.
- *Dosyalar:* `gezik/src/export.rs`, `gezik-core/src/csv.rs`, `gezik-platform/src/locale.rs` (`list_separator`).
- *Bütçe:* exe ≈ +50 KB; boşta 0.
- *Testler:* üç biçim (sütunlar, sıra, süzgeç, yalnız seçim), CSV tırnaklama ve Windows'ta `;`/BOM, HTML kaçışı (`<`, `&`, `"` adlarda), satır sonları, arama sonuçlarında klasör sütunu, hedef adın çakışma listesine gitmesi, geri alma, 100.000 satırda ≤ 1 s.
- *macOS/Linux:* `,` ayırıcı, BOM yok, LF.
- *Denetim listesi:* macOS ≈ 3, Linux ≈ 2.

**11i — süzgeçli kopya, ad maskesi, bölme, birleştirme**
- *Kapsam:* §12.
- *Dosyalar:* `gezik/src/copy_to.rs` (F5/F6 sorusu buraya taşınır), `gezik-ops/src/tasks/{copy, move_}.rs` (`filter`), `gezik-core/src/{file_filter, ops/names}.rs`, `gezik-batch/src/tasks/split.rs`.
- *Bütçe:* exe ≈ +150 KB; boşta 0.
- *Testler:* süzgeçli plan (dışlanan klasöre girilmez: sayaç), boş klasör oluşmaz, ilişkili dosyalar (bir kez, son uzantı kuralı), taşımada kalanlar yerinde; maske (TC örnekleri, `*.*`, `?`, uzantısız ad, nokta ile başlayan ad), aynı hedefe düşen iki ad çakışma listesinde; F5/F6'nın süzgeçsiz davranışı `master` ile aynı (10b testleri değişmeden geçer); bölme parça adları ve boyutları, `.crc` içeriği, iptalde yarım parçaların silinmesi, FAT32 varsayılanı; birleştirme eksik parça, CRC/boyut uyuşmazlığında çıktının silinmesi, `.crc`'siz not, geri alma.
- *macOS/Linux:* FAT (msdos/vfat) tanıma `DriveFacts` ile; ilişkili dosyalarda desen harf duyarsızdır (`IMG_1.CR2` `*.cr2`'ye uyar), kök ad Linux'ta harf duyarlı (`img_1.cr2`, `IMG_1.jpg`'nin ilişkilisi değildir), Windows/macOS'ta duyarsız.
- *Denetim listesi:* macOS ≈ 6, Linux ≈ 5.

## 17. Test

### 17.1 Birim (saf; her sistemde)

`hash` (vektörler, çok algoritma, iptal), `checksum_file` (biçimler), `csv`, `rate` (kova), `diff` (Myers, satır içi), `compare` (durumlar, kipler, yönler, sayılar, süzgeçler), `file_filter`, `names::apply_mask`, `treemap`, `duplicates` aşamaları (sahte dosya sistemi), `sync_profiles` ayrıştırma, `shortcuts` (yeni varsayılanlar, sayı, çakışmasızlık).

### 17.2 Platform

- Windows: `FILE_FLAG_NO_BUFFERING` hizalı okuma (dosya sonu hizasız), dizin numarasıyla sabit bağlantı, `LOCALE_SLIST`, `CopyFileExW` geri çağrısında hız sınırı.
- macOS: `F_NOCACHE`/`F_FULLFSYNC`, `clonefile`'ın doğrulamasız geçmesi, NFD adları, `opendiff`.
- Linux: `posix_fadvise`, `FICLONE`, `copy_file_range` parçaları, `.Trash-<uid>`, `meld`.

### 17.3 Çapraz denetim

Planın sonunda (her görevde değil): `cargo check` macOS ve Linux hedefleri, Windows `clippy`/`fmt`/`test`. Docker yok.

### 17.4 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken; klavye düzenine dokunulmaz): §16.1'in ekran maddeleri; ağ paylaşımına eşitleme (çöpsüz sürücü kuralı), USB FAT32'ye bölme, OneDrive klasöründe yinelenenler ve sağlama sorusu, 150 ve 200 % ölçekte boyut haritası, iki pencerede iki karşılaştırma.
- **macOS** (`macos-test.md`'ye eklenir, sonuçlar `macos-test-results.md`'ye): Tools menüsü, ⌃⌘C, `shasum` uyumu, APFS ↔ exFAT karşılaştırmada NFD adları, iCloud yer tutucuları, FileMerge dış aracı, Retina'da harita.
- **Linux** (gerçek makine, GNOME ve KDE, X11 ve Wayland): `sha256sum -c` uyumu, harf duyarlı karşılaştırma, `meld`, çıkarılabilir diskte `.Trash-<uid>` ile eşitleme ve geri alma, `/` boyut haritası.

## 18. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| Eşitlemede yanlış yönde toplu kopya ya da silme | Veri kaybı | Her zaman özet sorusu; silmeler yalnız çöpe, çöpsüz sürücüde yapılmaz; üzerine yazılanlar çöpe; tek iş tek Ctrl+Z; önce kopyalar sonra silmeler; ayna kipinde `×` satırları kırmızı |
| Durumsuz iki yönlü eşitleme silinen dosyayı geri getirir | Kullanıcı şaşırır | Kipin ipucu, README, §19; ayna kipi ve `×` yönü |
| Doğrulama okuması önbellekten gelir (özellikle macOS'ta önbellekte duran sayfalar) | Disk bozulması gözden kaçar | Windows ve Linux'ta önbellek atlanır; macOS'ta `F_FULLFSYNC` + `F_NOCACHE`, sınır §19'da yazılı |
| `FILE_FLAG_NO_BUFFERING`'in hizalama kuralları (sektör boyu, ağ sürücüsü) | Doğrulama hata verir | Sayfa hizalı tampon; hata olursa o dosyada tamponlu okumaya düşülür ve raporda `verified through the cache` notu |
| Hız sınırında uzun uyku | Duraklat/iptal geç duyulur | ≤ 100 ms dilimler, test |
| Büyük ağaçlarda karşılaştırma ve yinelenen belleği | Bellek patlar | Düğüm düzeni (indeks + ad), boyut elemesi, 2.000.000 sınırı, kapanınca bırakma testi |
| Yinelenenlerde sabit bağlantı ya da klonu silmek | Yer kazanılmaz / beklenmedik | Sabit bağlantılar tek sayılır; klonlar §19'da |
| Çevrimiçi yalnız dosyaları okumak | Büyük indirmeler | Karşılaştırma ve yinelenenler okumaz; sağlama sorar |
| Myers'ın kötü durumu (çok farklı dosyalar) | Donma | Baş/son kırpma, D ≤ 20.000 sınırı, arka plan iş parçacığı, dış araç önerisi |
| Boyut haritasında çok dikdörtgen | Yazılım çizicisinde yavaş kare | 2.000 sınırı, küçüklerin birleşmesi, canlandırma yok |
| F5/F6 sorusunun `copy_to.rs`'e taşınması | 10b davranışı bozulur | 10b'nin F5/F6 testleri değişmeden geçer; süzgeçsiz yol aynı iş |
| Arşiv listelerinde tarih yok ya da tutarsız (saat dilimi, DOS tarihleri) | Arşiv karşılaştırması yanıltır | Tarihsiz kalma seçeneği; DOS tarihleri yerel saat (5. adımın kuralı); kısım ertelenebilir |
| Exe bütçesi (dokuz katman ve Slint kodu) | Parça başına sınır aşılır | Katmanlar ortak bileşenleri (`ListView`, düğmeler, alanlar) kullanır; her parçanın ilk ve son ölçümü; aşılırsa PR'da gerekçeyle |

## 19. Bilinen sınırlar

- Eşitleme durumsuzdur: iki yönlü kip bir tarafta silinen dosyayı geri kopyalar; zamanlanmış/canlı eşitleme yok.
- Arşiv tarafı yalnız karşılaştırılır; arşive yazılmaz, arşivden seçili girdi çıkarılmaz.
- Bağlantılar (symlink, junction) karşılaştırılmaz, yinelenen sayılmaz; sayıları söylenir.
- Fark salt okunurdur; 16 MB / 1.000.000 satır üstü ve ikili dosyalar yalnız bayt karşılaştırması.
- macOS'ta doğrulama okuması önbellekte duran sayfaları atlamayabilir.
- APFS/Btrfs klonları yinelenen sayılır (silmek yer kazandırmayabilir); sabit bağlantılar sayılmaz.
- Hız sınırı iş başınadır, işler toplamı değil; doğrulama okumaları sınıra sayılmaz.
- Kuyrukta yalnız menüyle yukarı/aşağı; sürükleme yok.
- Liste dışa aktarma alt klasörleri yalnız düz görünümden alır; panoya HTML biçimi konmaz.
- Boyut haritası izlemez; Gezik'in kendi çöpe atması dışında değişiklik için yeniden tarama gerekir.
- Araçlar açıkken de izleme yok: sonuçlar bayatlayabilir, Gezik'in işleri dokunduğu satırları işaretler.

## 20. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 11a, 11b, 11c, 11d, 11e, 11f, 11g, 11h, 11i** (§16). *Gerekçe:* sağlama çekirdeği doğrulama, içerik karşılaştırma, yinelenenler ve birleştirmenin ortak temeli; fark karşılaştırmanın satırından açıldığı için ondan önce; karşılaştırma ve eşitleme iki PR, çünkü eşitleme dosya değiştirir ve ayrı gözden geçirilmeli; 11f–11h bağımsız.
2. **Dosya değiştiren her araç iş motorundan geçer; okuyan araçlar (karşılaştırma taraması, yinelenenler) arama/boyut tarayıcısının iş parçacıklarından** (§3). *Gerekçe:* ikinci bir kopya/silme yolu yazılmaz, çakışma listesi ve geri alma bedava; okuma işleri diski değiştirmediği için geri alma kaydına ve iş paneline girmemeli, ama sağlama aracı iş panelinin duraklat/iptal ve sürücü kuyruğundan yararlanır (karar 6).
3. **Sağlama çekirdeği `gezik-ops/src/hash.rs`'te** (§3.2). *Gerekçe:* doğrulama iş motorunun içinde koşar ve gezik-ops'a bağımlıdır; gezik-core bağımlılıksız kalmalı; gezik-search ve gezik-batch gezik-ops'u zaten kullanabilir.
4. **Algoritmalar MD5, SHA-1, SHA-256, SHA-512, CRC32; yalnız `md-5` eklenir; BLAKE3 yok** (§3.2). *Gerekçe:* kullanıcı kararı (BLAKE3 yalnız bağımlılık varsa; yok); `sha1`, `sha2`, `crc32fast` zaten bağlı, SHA-512 aynı `sha2` sandığında ve Linux dağıtımlarının `SHA512SUMS`'u için; CRC32 `.sfv` ve bölme/birleştirme için.
5. **Doğrulama ve içerik karşılaştırması özet değil bayt karşılaştırması** (§3.2). *Gerekçe:* iki dosyayı okumak her iki yolda da gerekir; bayt karşılaştırması daha ucuz, çakışmasız ve ilk farkın yerini verir.
6. **Sağlama hesaplaması iş motoru işi (`TaskKind::Checksum`), geri alma geçmişine girmez** (§4.3). *Gerekçe:* büyük dosyalarda dakikalar sürer; iş paneli ilerleme, ETA, duraklatma ve aynı diske kopyalayan işle sıraya girmeyi verir; tersi boş iş bugünkü kuralla kaydedilmez.
7. **Sağlama dosyası GNU biçiminde, ikili işaretli, göreli `/` yollarla** (§4.2). *Gerekçe:* `sha256sum -c` ve `shasum -c` ile doğrudan doğrulanır; Windows araçlarının çoğu (HashCheck, 7-Zip) bu biçimi okur.
8. **Panodaki değer `Expected` alanına kendiliğinden gelir** (§4.1). *Gerekçe:* en sık kullanım "indirme sayfasındaki SHA-256'yı kopyala, dosyayı denetle"; ayrı bir "panoyla karşılaştır" komutu gereksiz adım olurdu.
9. **Doğrulama önbelleksiz okur ve uyuşmazlıkta bir kez yeniden kopyalar; taşımada kaynak doğrulamadan önce silinmez** (§5.1). *Gerekçe:* önbellekten okunan doğrulama yalnız belleği doğrular; tek yeniden deneme geçici ağ/USB hatasını kapatır, kalıcı bozulmayı gizlemez; taşıma veri kaybı yolu olmamalı.
10. **Doğrulama varsayılan kapalı; ayar + iş başına kutu ve iş panelinde aç/kapa** (§5.1). *Gerekçe:* kullanıcı kararı 8; kopya süresini yaklaşık ikiye katlar.
11. **Hız sınırı iş başına kova, ondalık birimler, klon ve doğrulama sayılmaz** (§5.2). *Gerekçe:* hız sınırının amacı ağı ya da yavaş diski başkalarına bırakmak; işler toplamı sınırı işler arası adalet ve ayrı ayar ister (YAGNI); ağ hızları ondalık söylenir.
12. **Kuyruk yalnız menüyle yukarı/aşağı; çalışan iş yer değiştirmez** (§5.3). *Gerekçe:* `wait_turn` zaten liste sırasını izler, değişiklik bir yer değiştirmedir; sürükleyerek sıralama iş paneli için ayrı bir sürükleme yolu isterdi.
13. **`compare` tek eylem, seçime göre fark ya da klasör karşılaştırma; Shift+F2 / ⌃⌘C** (§3.3, §7.1). *Gerekçe:* kullanıcı "karşılaştır" der, aracı Gezik seçer; Shift+F2 TC'nin klasör karşılaştırma tuşu ve boş; ⌃⌘C macOS'ta boş ve 10b'nin ⌃⌘ dizisini sürdürür; diğer araçlar seyrek kullanılır, tuş harcanmaz.
14. **İç fark salt okunur, Myers + satır içi tek aralık, D ≤ 20.000; dış araç ayarlıysa doğrudan o** (§6). *Gerekçe:* kullanıcı kararı 4 ("hafif"); birleştirme ve düzenleme WinMerge/Meld'in işi; tek aralıklı satır içi vurgu kelime farkının çoğunu karşılar; ayarlı dış araç kullanıcının açık tercihidir.
15. **Karşılaştırma ve eşitleme aynı katman; iki bölmenin klasörleri varsayılan taraflar** (§7). *Gerekçe:* TC'nin Synchronize dirs penceresi gibi: önce görür, sonra yönleri düzeltir, sonra uygular; çift panel (10b) iki yeri seçmenin en doğal yolu (kullanıcı kararı).
16. **Göster düğmeleri `→ = ≠ ← 1× 2×`, `=` varsayılan kapalı** (§7.2). *Gerekçe:* TC'nin bilinen düzeni (rakip notu §6); eşit satırlar binlerce olup farkları gömer.
17. **FAT 2 sn her zaman, 1 saat seçenek; adlar NFC ve sisteme göre harf duyarlılığı** (§7.3, §7.4). *Gerekçe:* 2 sn FAT'in doğası (bugünkü `same_time`), 1 saat yalnız yaz saati kaymasında doğru ve bazen yanlış eşitler; macOS'un NFD adları USB disklerle karşılaştırmada aksi hâlde her dosyayı "yalnız bir tarafta" gösterirdi; `icu_normalizer` zaten bağlı.
18. **Katman gizlenip durum satırında çip olarak bekleyebilir** (§7.5). *Gerekçe:* "bölmede göster" ile bakıp geri dönmek sık; sonucu bırakmak yeniden tarama demek; çip kullanıcının seçimiyle bellek tutar, ✕ bırakır.
19. **`select-differences` alt klasörsüz ve G/Ç'siz** (§7.5). *Gerekçe:* TC'nin en çok kullanılan komutu, iki bölmenin zaten okunmuş listesiyle bedava; derin karşılaştırma katmanın işi.
20. **Eşitleme durumsuz; kipler Two-way, Left to right, Mirror; sağdan sola `⇄` ile** (§8.1). *Gerekçe:* TC gibi durumsuz eşitleme veritabanı, yolu taşınan klasör ve bozulan veritabanı sorunlarını getirmez; üç kip FFS/TC kullanıcısının bildiği üç niyet; yön adları hep soldan sağa okununca karışmaz.
21. **Silmeler yalnız çöpe; çöpsüz sürücüde silme yapılmaz; geri alınamayacak üzerine yazma ayrı onay ister** (§8.2). *Gerekçe:* kullanıcı kararı 1 ("silmeler her zaman çöpe ve geri alınabilir"); çöpsüz sürücüde "kalıcı sil"e düşmek bu kararı bozardı.
22. **Klasör ↔ arşiv yalnız karşılaştırma, 11e'nin son ve ertelenebilir görevi** (§8.5). *Gerekçe:* kullanıcı "ucuzsa" dedi: listeleme var, yazma ya da seçili girdi çıkarma yok; arşive yazmak zip'i yeniden yazmak demek ve 16. adımın VFS'iyle daha doğru gelir.
23. **Profiller `settings.toml`'da `[[sync-profiles]]`; paletten açınca karşılaştırır, uygulamaz** (§8.4). *Gerekçe:* `[[tab-sets]]`/`[[searches]]` gibi kullanıcının taşınabilir ayarı; bir palet seçimi hiçbir zaman sorusuz dosya kopyalamamalı.
24. **Yinelenenler bir arama kipi ve sonuç listesi** (§9). *Gerekçe:* seçim, silme, geri alma, önizleme, sağ tık, sütunlar, sekme ve geçmiş 8a ve 10d'den bedava; ayrı katman aynı liste arayüzünü ikinci kez yazardı.
25. **Yinelenen aşamaları: boyut, ilk 64 KB, tam SHA-256; iki üyeli grupta bayt karşılaştırması; boş dosyalar ve çevrimiçi yalnız dosyalar dışarıda; sabit bağlantılar tek** (§9.2). *Gerekçe:* dosyaların çoğu boyutta düşer, ön özet büyük medya dosyalarını ucuzca ayırır; boş dosyalar anlamsız gürültü; yer tutucuyu okumak indirme olur; sabit bağlantıyı silmek yer kazandırmaz.
26. **Seçim yardımcıları hiçbir grubun bütün kopyalarını seçmez** (§9.3). *Gerekçe:* aracın amacı yer açmak, veri kaybetmek değil; Delete yine de bütün kopyaları elle seçeni bir kez uyarır.
27. **Boyut haritası yalnız klasör düğümlerini tutar, odaktaki klasörün dosyalarını o an okur; 2.000 dikdörtgen** (§10). *Gerekçe:* dosya başına düğüm 1 milyon dosyada yüzlerce MB olurdu; tek klasör okuması ucuz; yazılım çizicisinde dikdörtgen sayısı kare süresidir.
28. **Haritada tık yakınlaşır, Enter bölmede açar** (§10.3). *Gerekçe:* kullanıcı "tıklayınca klasöre gir" dedi: haritada girmek okuma gerektirmez ve keşfi sürdürür; bölmeye gitmek ayrı ve açık bir niyet.
29. **8b'nin önbelleği haritanın toplamlarıyla dolar** (§10.1). *Gerekçe:* kullanıcı kararı 5; harita sonrası listenin boyut sütunu bedava gelir; önbellek sınırı aynı kalır.
30. **Dışa aktarma gösterileni aktarır (sıra, süzgeç, seçim); alt klasörler düz görünümle** (§11). *Gerekçe:* "gördüğünü al" en az sürprizli kural; ayrı bir özyineleme seçeneği 8a'nın düz görünümünü yineler.
31. **CSV makine okunur (ham bayt, ISO tarih), Windows'ta sistemin liste ayırıcısı ve BOM** (§11.2). *Gerekçe:* CSV'nin amacı tabloya almak; Türkçe Windows'ta Excel `;` bekler ve BOM'suz UTF-8'i bozuk açar. `=`, `+`, `-`, `@`, sekme ya da CR ile başlayan alan `'` önekiyle yazılır (OWASP CSV enjeksiyonu; 11h gözden geçirmesi).
32. **Dosya iletişim kutusu yok; hedef yazılabilir alan (path completion)** (§4.2, §11.1). *Gerekçe:* Gezik bugün sistem kaydet penceresi kullanmıyor (F5'in hedef alanı gibi); varsayılan yer gösterilen klasör, yazılan dosya seçilir ve geri alınabilir.
33. **`Copy To…`/`Move To…` yeni eylemler; F5/F6 ile aynı soru bileşeni; süzgeç ve maske `Options ▸` altında** (§12.1). *Gerekçe:* süzgeçli kopya tek bölmede de gerekir; tek soru bileşeni iki davranışı ayrıştırmaz; seçenekler kapalı başlayınca F5'in bugünkü hızı bozulmaz.
34. **Süzgeç sözdizimi tek (`gezik-core::file_filter`): 6a deseni, `|` dışlama, `\`/`/` klasör, `>` ilişkili dosya (yalnız kopyada)** (§7.6). *Gerekçe:* karşılaştırma ve kopya aynı dili konuşur; TC kullanıcısının bildiği sözdizimi; `>` yalnız kopyada anlamlı.
35. **Süzgeçli kopyada boş klasör oluşmaz** (§12.2). *Gerekçe:* "yalnız JPG'leri kopyala" isteyen boş klasör ağacı beklemez; `MadeParent` geri almayı temiz tutar.
36. **Ad maskesi DOS/TC kuralı (`*`, `?`, son nokta)** (§12.3). *Gerekçe:* kullanıcı kararı 7 ("jokerli hedef adı"); toplu ad (5. adım) daha güçlü yeniden adlandırma için zaten var.
37. **Bölme ham `.001` + TC biçimli `.crc`; birleştirmede CRC uyuşmazsa çıktı silinir** (§12.4, §12.5). *Gerekçe:* kullanıcı kararı 7 (ham parçalar, CRC); TC ve 7-Zip'in "split" çıktısıyla uyumlu; yanlış birleşmiş bir dosyayı bırakmak sessiz bozulmadır, parçalar yerinde kaldığı için silmek veri kaybettirmez.
38. **Araç katmanları `if` ile yalnız açıkken kurulur, kapanınca bırakılır; izleme yok, bayat satırlar işaretlenir** (§3.1, §3.4). *Gerekçe:* kullanıcının hafiflik kuralı; bellek düzeltmesinde (`if` ile katman kurma) ölçülen kazanç; izleyici hem tutamak hem uyanma maliyeti.
39. **Menüler: sağ tık `Tools ▸` (üç sistem), macOS menü çubuğunda `Tools`; Windows/Linux'ta ayrıca menü çubuğu yok** (§3.3, §13.3). *Gerekçe:* kullanıcı kararı "paletten ve menülerden"; Windows/Linux'ta Gezik'in menü çubuğu yok, bir araç için eklemek düzeni değiştirirdi; sağ tık seçimin üstünde çalışan araçlar için doğal yer.
40. **Menü kimlikleri 2200–2399; `Action` 102 → 113** (§13). *Gerekçe:* 10. adım 2000–2199'da biter; her parça kendi aralığında.
41. **Bütçe: her parça ≤ +262.144 B (tahminler §15), boşta 0, açılış ≤ +1 ms, kapanınca taban + 0,3 MB** (§15). *Gerekçe:* önceki adımların kuralı; araçlar isteğe bağlı olduğu için boşta hiçbir maliyet kabul edilmez; +0,3 MB 10b/10g'de ölçülen heap parçalanması payıdır.
42. **Çevrimiçi yalnız dosyalar: sağlama sorar, karşılaştırma ve yinelenenler okumaz** (§3.2). *Gerekçe:* sağlama belirli dosyalar için açık bir istek, indirmeyi kabul edebilir; tarama araçları binlerce dosyayı sessizce indirmemeli.
