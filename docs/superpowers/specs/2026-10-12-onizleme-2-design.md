# Alt Proje 13: Önizleme 2 — Tasarım

- **Tarih:** 2026-10-12
- **Durum:** Kapsam kullanıcı kararı (2026-10-12: dört başlığın hepsi, §2). Ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§17, her biri gerekçesiyle). Spec onaylanınca plan önceden onaylıdır. 13a uygulandı (`feat/preview-13a`, plan `2026-10-12-onizleme-2-13a.md`; ekran testleri bekliyor); 13b–13f bekliyor.
- **Kapsam:** Gezik yol haritasının 13. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md`, "Önizleme 2": satır 31, 39, 84; §4 satır 112; §7 satır 238). Tek spec, altı parça (13a–13f; §13).
- **Dayandığı:** `2026-10-04-gorunum-design.md` (önizleme bölmesi, sütunlar), `2026-10-05-toplu-islemler-design.md` (ad şablonu, ffmpeg ve pdfium indirmeleri, PDF işçisi), `2026-10-09-arama-design.md` (arama tarifi, tarayıcı, kayıtlı aramalar), `2026-10-10-sistem-butunlesmesi-design.md` (Quick Look, bulut yer tutucuları), `2026-10-11-duzenler-design.md` (sütun görünümünün önizleme sütunu, gruplama 10d, görünüm kuralları 10f), `2026-10-12-etiketler-design.md` (tembel meta okuyucu §3.4, süzgeç sözcükleri §6.3, vurgulama kuralları §9).
- **Taban:** `master` c103e95. Önizleme bugün `crates/gezik/src/preview.rs` (818 satır): tek işçi iş parçacığı yalnız son isteği alır (`Slot`), resim `gezik_platform::decode_image` ile en çok 1024 px'e, metin dosyanın ilk 64 KB'ı (`TEXT_BYTES`) tek `Text` içinde sarılarak, öbürleri sistem küçük resmi ya da simge. Aynı `PreviewInfo` üç yerde gösterilir: sağ bölme (Alt+P), sütun görünümünün önizleme sütunu, Gezik'in Quick Look penceresi (Space; macOS'ta varsayılan sistemin paneli). ffmpeg (ffprobe'la) ve pdfium istenince indirilen araçlar (`gezik_core::batch::tools`), pdfium ayrı süreçte (`gezik --pdf-worker`, tek istek, satır yanıtları). `Action::ALL` 12 bitince 124; menü kimlikleri 12 için 2400–2599 ayrılmış. Exe boyutu ve boşta bellek her parçanın ilk görevinde yeniden ölçülür.
- **Rakip notları:** TC (Lister: metin, hex, ikili kipler; WDX içerik alanları sütunda, aramada, çoklu ad değiştiricide), DO (meta veri paneli, sütunlar, ad şablonunda meta alanları), XYplorer (önizlemede oynatma, sayfa gezme), Finder/Quick Look (video ve PDF oynatma, Get Info'da "More Info"), Explorer (Ayrıntılar bölmesi, özellik sütunları), Dolphin (Bilgi paneli, Baloo alanları).

## 1. Amaç

Önizlemeyi "dosyanın başına bir bakış"tan dosyanın kendisine çevirmek: **videoyu ve sesi oynatmak, PDF'in bütün sayfalarını gezip yakınlaştırmak, kodu renkli, büyük metni baştan sona, ikili dosyayı hex olarak göstermek**; ve dosyaların içindeki bilgileri (EXIF, şarkı etiketleri, süre, boyutlar, belge özellikleri) **tek bir meta veri alanı kaydı** üzerinden her yerde kullanmak: önizlemedeki panelde, liste sütunlarında, süzgeçte ve aramada, toplu ad şablonunda, vurgulama ve görünüm kurallarında (TC'nin WDX eklentilerinin eklentisiz karşılığı).

Hafiflik bozulmaz: **gösterilmeyen hiçbir şey okunmaz ya da çözülmez; oynat'a basılmadan tek kare çözülmez; önizleme değişince ayrılan her şey (kareler, sayfalar, metin, dış süreçler) bırakılır.** Önizlemeyi ve yeni sütunları kullanmayan bir Gezik bugünkü Gezik kadar hafiftir; boşta zamanlayıcı, yoklama, süreç yok.

### Başarı ölçütleri

- **Metin, kod, hex (13a):** 2 MB'a kadar metin baştan sona kayar, satır numaralı, ≈ 25 dilde renkli; 2 MB'tan büyük metin (10 GB günlük dahil) bayt konumuyla kayar, sona atlama ≤ 100 ms; hex görünümü her boyutta. Bellek: küçük dosyada ≤ 3 × dosya boyutu, büyükte ≤ 1 MB.
- **PDF (13b):** sayfa gezme, yakınlaştırma (%25–%800), sığdır; yalnız görünen sayfa, yalnız görünen bölge kadar piksel çizilir; sayfa değişimi ≤ 150 ms (tipik sayfa, SSD). Resimler de aynı yüzeyle yakınlaşır.
- **Video ve ses (13c):** oynat/duraklat, ileri-geri sarma, ses düzeyi; oynat'a basınca ilk kare ≤ 500 ms (SSD, yerel dosya); 720p oynatmada Gezik süreci bir çekirdeğin ≤ %25'i (ffmpeg hariç); durdurunca dış süreçler ve kare tamponları ≤ 1 sn içinde gider.
- **Meta veri kaydı ve panel (13d):** tek dosyanın bütün alanları önizleme panelinde ≤ 30 ms (SSD); okuyucular bozuk ya da kötü niyetli dosyada çökmez (rastgele bayt testleri), dosyanın en çok sabit bir bölümünü okur.
- **Sütunlar, süzme, sıralama, gruplama (13e):** görünen 40 satırın alanları arka planda ≤ 50 ms (SSD, JPEG/MP3); 10.000 öğede tam okuma Windows ≤ 4 sn, Linux/macOS ≤ 2 sn, ilerlemeyle.
- **Arama, ad şablonu, kurallar (13f):** alan ölçütü olmayan aramada, şablonda ve kuralda hiçbir dosya içi okuma yok (sayaçla test).
- **Hafiflik (hepsi):** varsayılan ayarlarla boşta bellek `master`'a göre **0** (±0,02 MB), açılış ≤ +1 ms, klasör açma `master` ±%2 (alan sütunu yokken). Her parça exe'ye ≤ +262.144 B (§12). Yeni Slint katmanı yok (§3.5).

### Kapsam dışı (bilerek)

- **Önizlemede düzenleme, metin seçme/kopyalama, metin içinde arama, hex düzenleme.** Görüntüleyici salt okur; arama içerik aramasıyla (8) yapılır.
- **Altyazı, birden çok ses izi seçimi, HDR ton eşleme, oynatma listesi, oynatma hızı.** İlk görüntü ve ilk ses izi çalınır.
- **PDF'te metin seçme, arama, form doldurma, sürekli (alt alta) sayfa akışı, küçük resim şeridi.** Tek sayfa görünümü (karar 9).
- **Office belgelerinin (docx, xlsx, pptx) sayfa önizlemesi:** sistem küçük resmi kalır; özellikleri (başlık, yazar, sayfa) panelde ve sütunda.
- **Arşiv içeriğinin önizlemede listesi:** arşive girmek (5) bunu karşılar.
- **Meta veriyi yazma** (ID3, EXIF düzenleme): kayıt salt okunur; dosyaya yazan alanlar yalnız 12'ninkiler (etiket, puan, yorum).
- **Eşitleme karşılaştırmasında meta alanı** (rakip özeti satır 66'nın bir maddesi): 11e içerik, boyut, tarihle karşılaştırır; kayıt API'si ileride bunu taşıyabilir (karar 28).
- **Windows özellik işleyicilerinden ve macOS Spotlight'tan alan okumak:** üç sistemde aynı alanlar Gezik'in kendi okuyucularından (karar 18). 12d'nin Explorer etiket/puan okuması olduğu gibi kalır.

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Kapsam | Dört başlığın hepsi: (1) video/ses oynatma, (2) çok sayfalı PDF, (3) kod renklendirme + tam metin + hex, (4) meta veri paneli ve sütunları, tek alan kaydıyla | Kullanıcı kararı 2026-10-12 |
| Video/ses | Poster kare + oynat/duraklat, sarma, ses düzeyi; oynat'a basılmadan hiçbir şey çözülmez; durunca bellek bırakılır; 5c/5d'nin ffmpeg'i | Kullanıcı kararı 1 |
| PDF | Sayfa gezme, yakınlaştırma; yalnız görünen sayfa, gereken boyutta; pdfium hazır | Kullanıcı kararı 2 |
| Metin | Kod renklendirme (exe boyutu ağır basar; parça başına ≤ +262.144 B), büyük metinde tam kaydırma (sayfalı), ikili dosyada hex | Kullanıcı kararı 3 |
| Meta veri | Panel ve sütunlar; tek alan kaydı (66, TC WDX'in eklentisiz karşılığı): EXIF, ses/video etiketleri, belge özellikleri, resim boyutu, süre…; aynı kayıt sütunları, arama ölçütlerini (8), ad şablonlarını, vurgulama/görünüm kurallarını (10f, 12f) besler; yalnız görünen satırlar, tembel | Kullanıcı kararı 4 |
| Hafiflik | Boşta değişiklik yok, gösterilmeyen çözülmez, önizleme değişince bellek bırakılır | Ürün hedefi |
| Süreç | Parçalar sırayla ya da yığılı, paralel değil (11'in dersi) | Bakımcı kuralı |
| Soru | Ayrıntı kararları kullanıcıya sorulmaz; spec'te gerekçeyle (§17) | Bakımcı kuralı |

## 3. Ortak yapı

### 3.1 Önizleme modülü ve görüntüleyiciler

`crates/gezik/src/preview.rs` bir klasöre bölünür (`preview/mod.rs` bugünkü kod; davranışı değişmez):

| Dosya | İş | Parça |
|---|---|---|
| `preview/mod.rs` | `Target`, `Body`, `Slot`, işçi, `describe`, bölme/Quick Look bağlantısı (bugünkü) | — |
| `preview/viewer.rs` | Etkin görüntüleyici (`Viewer`) ve onun yaşam döngüsü | 13a |
| `preview/text.rs` | Satır görüntüleyici: bellekte metin ve sayfalı metin, sarma | 13a |
| `preview/hex.rs` | Hex görüntüleyici | 13a |
| `preview/scroller.rs` | Bayt konumlu kaydırıcı (sayfalı metin ve hex ortak) | 13a |
| `preview/zoom.rs` | Yakınlaştırma/kaydırma yüzeyi (PDF ve resim ortak) | 13b |
| `preview/pdf.rs` | PDF görüntüleyici, pdfium görüntü işçisinin istemcisi | 13b |
| `preview/media.rs` | Video/ses oynatıcı: ffmpeg süreçleri, saat, kareler | 13c |
| `preview/fields.rs` | Panelin alan satırları | 13d |

```rust
/// What the preview shows beyond today's picture/text/icon; at most one at a time.
pub enum Viewer {
    None,
    Text(text::TextView),     // 13a
    Hex(hex::HexView),        // 13a
    Pdf(pdf::PdfView),        // 13b
    Picture(zoom::ZoomView),  // 13b: a picture once zoomed past "fit"
    Media(media::Player),     // 13c
}
```

- **Tek sahip, tek yaşam:** `Viewer` `Preview`'in `Inner`'ında durur. Hedef değişince (`refresh`'te yeni `Target`), bölme ve Quick Look ikisi de kapanınca, ya da önizleme sütunu kapanınca (bugünkü `release`) eski görüntüleyici `drop` edilir: `Drop` dış süreci öldürür, tamponları bırakır, ses aygıtını kapatır. Böylece "önizleme değişince bellek bırakılır" tek yerden tutulur (karar 3).
- **Üç yer, bir görüntüleyici:** bölme, önizleme sütunu ve Quick Look aynı `Viewer`'ı gösterir (bugünkü `publish` gibi); Quick Look açıkken kareler ve sayfalar Quick Look'un boyutuna göre istenir (bugünkü `picture_px` kuralı: en büyük görünen yer).
- **Bugünkü işçi değişmez:** ilk görüntü (resim, metnin başı, küçük resim, poster) bugünkü yoldan gelir; görüntüleyici yalnız kullanıcı bir şey istediğinde ya da ilk görüntü yetmediğinde (2 MB'a kadar metin, sayfa 2, oynat) kendi iş parçacığını kurar.
- **macOS sistem Quick Look paneli** (`[system] quick-look = "system"`) değişmez: video ve PDF'i zaten kendisi oynatır/gösterir; 13 Gezik'in bölmesine, önizleme sütununa ve kendi Quick Look penceresine gelir.

### 3.2 Görüntüleyici çubuğu ve tuşlar

Önizleme alanının altında, görüntüleyiciye göre değişen tek satırlık ince bir çubuk (`preview-pane.slint`'te, `PreviewInfo.bar`'a göre). 200 px'lik bölmeye sığar; dar yerde yazılar düşer, düğmeler kalır.

| Görüntüleyici | Çubuk |
|---|---|
| Metin | `Text │ Hex` anahtarı, `Wrap` (aç/kapa), sağda `1,204 lines` ya da `12.4 GB · 37%` |
| Hex | `Text │ Hex`, sağda konum `0x0004_A3F0` |
| Başka dosya (simge/küçük resim) | `Hex` düğmesi |
| PDF | `‹  3 / 12  ›`, `−  100%  +`, `Fit` |
| Resim | `−  Fit  +` (yalnız fare üstündeyken) |
| Video/ses | `▶/❚❚`, sarma çubuğu, `1:02 / 3:25`, ses düğmesi ve düzey çubuğu |

- **Tuşlar yalnız görüntüleyicinin içinde** (Quick Look penceresinde her zaman; bölmede görüntüleyiciye tıklanıp odak oradayken; Esc odağı listeye geri verir). Seçicinin Alt+0…5'i gibi bunlar görüntüleyicinin kendi klavyesidir, kısayol tablosuna girmez:

| Tuş | İş |
|---|---|
| Page Up / Page Down, Home / End | metin/hex sayfa, başa/sona; PDF önceki/sonraki sayfa, ilk/son |
| `+` / `-` / `0` / `1` | yakınlaştır / uzaklaştır / sığdır / %100 (PDF, resim) |
| `K` | oynat/duraklat (video/ses) |
| `J` / `L` | 10 sn geri / ileri |
| `M` | sesi kapat/aç |
| `H` | metin ↔ hex |
| `W` | sarma aç/kapa |

  Oklar ve Space bugünkü gibi Quick Look'ta dosyalar arasında gezer ve pencereyi kapatır (değişmez). Fare tekeri metin/hex'i kaydırır, PDF'te sayfanın sonunda sonraki sayfaya geçer, Ctrl+teker (macOS ⌘+teker ya da kıstırma) yakınlaştırır.
- **Genel eylemler** (palet ve menüde, varsayılan tuşsuz; karar 13): `preview-hex` (`Show Preview as Hex`), `preview-play` (`Play/Pause Preview`).
- Ekran okuyucu: çubuktaki her düğmenin erişilebilir adı (`Play`, `Next page, page 3 of 12`); metin satırları `accessible-role: text`.

### 3.3 Dış süreçler ve ikili boru

- **Yeni platform işlevi:** `ChildProcess::spawn_binary(program, args, input)` (`gezik-platform/src/process.rs`): stdin yazılabilir, stdout ham bayt olarak okunur (`impl Read`), stderr bugünkü gibi son satırlarıyla. Bugünkü `ChildProcess` stdout'u 16 KB'ta kesilen satırlar olarak verdiği için kare ve sayfa pikselleri oradan geçemez. Windows'ta bugünkü iş nesnesi (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) korunur: Gezik çökse de çocuklar ölür; Unix'te süreç grubu ve `kill_tree`.
- **pdfium görüntü işçisi** (13b) ve **ffmpeg** (13c) bu yoldan çalışır. İkisi de önizleme değişince öldürülür; ikisinin de stdin'i kapanınca kendisi çıkar (çift güvence).
- **Araç yoksa:** hiçbir şey kendiliğinden indirilmez. Çubukta bir satır (`Playing needs ffmpeg (~30 MB) · Download`; boyut platforma göre bugünkü tablodan) ve tıklayınca bugünkü teklif kutusu (`archives.rs::offer_ffmpeg`/`offer_pdfium`, yeni `Need::Playback` ve `Need::PdfPages` metinleri). `[tools] download = false` iken bugünkü "kendin kur" metni.

### 3.4 Alan kaydı (13d'de doğar; özet)

Tek kayıt (`gezik-core/src/fields.rs`), biçim başına okuyucular (`gezik-batch/src/fields/`), tüketiciler: panel (13d), sütun/sıralama/gruplama/süzgeç/görünüm kuralları (13e), arama/ad şablonu/vurgulama kuralları (13f). Ayrıntı §7–§9.

### 3.5 Arayüz: yeni katman yok

- Görüntüleyiciler `preview-pane.slint`'in içinde (bölme, önizleme sütunu ve Quick Look penceresi aynı bileşeni kullanır). Metin ve hex satırları `ListView` değil, sabit satır yüksekliğiyle yalnız görünen satırları kuran basit bir `for` (bugünkü `file-view.slint`'in satır modeli deseni).
- "More Columns…" seçicisi = `widgets/picker.slint` (12b'nin `check` alanıyla).
- Arama çubuğundaki alan ölçütü = bugünkü ek seçenekler satırına tek metin alanı (§9.1).
- Ad şablonu alanları = `rename-batch.slint`'in var olan şablon alanı + bir `Fields ▾` açılır menüsü (bugünkü `popup-menu.slint`).

### 3.6 Hafiflik kuralı (her parça)

1. Önizleme kapalıyken (bölme, sütun, Quick Look) 13'ün hiçbir kodu çalışmaz; ayar anahtarları ayar yüklenirken okunur.
2. Görüntüleyici yalnız gösterilen dosya için ve yalnız gerekince kurulur: metin görüntüleyicisi ilk 64 KB'ı aşan kaydırmada, PDF görüntüleyicisi sayfa 1'den başka sayfa ya da yakınlaştırma istendiğinde (sayfa 1 bugünkü küçük resim), oynatıcı yalnız oynat'a basılınca.
3. Önizleme değişince görüntüleyici `drop` edilir (§3.1); duraklatılmış oynatıcı 60 sn sonra süreçlerini kapatır, konumu tutar (devam'da yeniden başlatır); PDF işçisi önizleme PDF dışına geçtikten 10 sn sonra çıkar (art arda PDF gezen kullanıcı her dosyada süreç kurmasın; karar 10).
4. Alanlar yalnız bir tüketici istediğinde okunur: panel açık ve dosya seçili, alan sütunu açık, süzgeçte/aramada/kuralda alan koşulu var, şablonda alan var (§7.5). Hiçbiri yoksa alan okuyucusu kurulmaz.
5. Zamanlayıcı yok: oynatıcının saati ses aygıtından ya da kare okumadan ilerler; çubuktaki süre yalnız oynarken güncellenir.

## 4. Metin, kod, hex (13a)

### 4.1 Hangi dosya, hangi görünüm

- Bugünkü sezgi değişmez: ilk 4 KB'ta NUL yoksa metin (`gezik_core::text::decode`: BOM'lar, BOM'suz UTF-16, UTF-8, ANSI kod sayfası). İlk görüntü bugünkü gibi ilk 64 KB'tır; kullanıcı aşağı kaydırıp bu sınıra gelince görüntüleyici kurulur ve kaydırma kesilmeden sürer.
- **Bellekte (≤ 2 MB):** dosyanın tamamı bir kez okunur ve çözülür (`String`), satır başları dizini (`Vec<u32>`), renklendirmenin satır başı durumu (`Vec<u8>`, satır başına 1 bayt). Satır numaraları solda.
- **Sayfalı (> 2 MB):** dosya açık tutulmaz; konum **bayt konumu**dur. Görünen pencere için konumdan önceki satır başına hizalanmış 64 KB okunur (`pread`/`ReadFile` ofsetli), çözülür, satırlara bölünür. Kaydırma çubuğu bayt oranını gösterir; satır numarası yok (bilinmez), çubukta `12.4 GB · 37%`. End tuşu dosyanın sonuna atlar (günlüğün kuyruğu). Bellek ≤ 2 pencere (≤ 128 KB metin + satırlar). Satır dizini kurulmaz (10 GB'ta dakikalar ve yüz MB'larca bellek; karar 6).
- UTF-16 sayfalı okumada pencere çift bayta hizalanır; UTF-8'de satır başına hizalama karakter ortasında kesmeyi önler. ANSI kod sayfası bellekteki gibi.
- Dosya önizlenirken değişirse (günlük büyür): görüntüleyici değişmiş boyutu yalnız kullanıcı End'e bastığında ya da kaydırdığında görür (izleme yok); F5 önizlemeyi yeniden kurar.
- **Bulut yer tutucusu:** bugünkü gibi yalnız-bulut dosya okunmaz (simge); görüntüleyici kurulmaz.

### 4.2 Satırların çizimi

- Eş aralıklı yazı tipi (bugünkü `mono-font`), sabit satır yüksekliği; yalnız görünen satırlar (+ bir ekran yukarı/aşağı) Slint modeline girer.
- **Sarma** (`Wrap`): eş aralıklı yazıda karakter sayısıyla yapılır: görünür satır = (mantıksal satır, parça). Doğu Asya geniş karakterleri 2 sütun sayılır (küçük bir aralık tablosu, `gezik-core/src/text.rs`'e `display_width`). Sekme 4 sütun. Varsayılan: `Text` türünde (txt, md, log, ini…) açık, `Code` türünde kapalı; anahtar dosya türü başına değil oturum başına hatırlanır (`state.toml [preview] wrap-text`, `wrap-code`).
- Sarma kapalıyken çok uzun satır (sıkıştırılmış JSON) 4.096 karakterde kesilir, sonunda `…`; yatay kaydırma bu genişliğe kadar.
- Kontrol karakterleri `·`, satır sonları gösterilmez (CRLF/LF/CR hepsi satır sonu).

### 4.3 Kod renklendirme

- **Seçim:** küçük, Gezik'in içinde, tablo güdümlü bir sözcük çözücü (`gezik-core/src/syntax.rs`, yeni, saf). syntect (sözdizimi dökümleri ve regex motoru, MB'larca) ve tree-sitter (dil başına 100 KB–2 MB ayrıştırma tablosu, C derleyicisi) bütçeye sığmaz (karar 4).
- **Model:** her dil bir `LangDef`:
  ```rust
  pub struct LangDef {
      pub names: &'static [&'static str],        // extensions and file names (Makefile, Dockerfile)
      pub line_comments: &'static [&'static str], // "//", "#", "--", ";"
      pub block_comments: &'static [(&'static str, &'static str)],
      pub strings: &'static [StringRule],         // quote, escape, multi-line, raw (r#"…"#, """…""")
      pub keywords: &'static [&'static str],      // sorted; binary search
      pub types: &'static [&'static str],         // sorted
      pub markup: bool,                           // <tag attr="…">
      pub keys: bool,                             // JSON/TOML/YAML/INI keys before = or :
  }
  ```
  Sınıflar: `Keyword`, `Type` (listede ya da büyük harfle başlayan tanımlayıcı, dil izin veriyorsa), `String`, `Number`, `Comment`, `Tag` (işaretleme etiketi ve anahtarlar), `Plain`. Satır başına durum (blok yorum içinde, çok satırlı dize içinde) bir bayttır; bellekteki dosyada tek geçişte hesaplanır (2 MB ≈ 10 ms), satırlar görünürken sözcüklere ayrılır.
- **Diller (≈ 25):** Rust, C/C++, C#, Java, Kotlin, Go, Swift, JavaScript/TypeScript (jsx/tsx), Python, Ruby, PHP, Lua, Shell (sh/bash/zsh), PowerShell, Batch, SQL, HTML/XML/SVG, CSS/SCSS, JSON, TOML, YAML, INI/conf, Markdown (başlık, kod bloğu, vurgu), Diff/patch (+/− satırları), Slint, Makefile, Dockerfile. Tanınmayan uzantı: renksiz.
- **Yalnız bellekteki dosyalar (≤ 2 MB) renklenir.** Sayfalı dosyada pencerenin başındaki durum bilinmez (blok yorumun ortası); yanlış renk vermektense renksiz (karar 5). Çubukta `Plain text: the file is larger than 2 MB`.
- **Çizim:** satır, en çok 64 renk parçasına bölünür; her parça sütun × karakter genişliği konumunda bir `Text`. Karakter genişliği yazı tipinden bir kez ölçülür. Parça sınırı aşılırsa kalan `Plain`.
- **Renkler:** temada altı yeni belirteç: `syntax-keyword`, `syntax-type`, `syntax-string`, `syntax-number`, `syntax-comment`, `syntax-tag`; açık ve koyu temada varsayılanlar, tema dosyalarında (`[colors]`) değiştirilebilir (`gezik-config/src/theme.rs`'in bilinen anahtarları). Karşıtlık ≥ 4,5:1 (WCAG) testle.

### 4.4 Hex görünümü

- Satır: `00004A30  48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 0A 00 00 00  Hello, world....`; satır başına 16 bayt (bölme 420 px'ten darsa 8 bayt, ofset kısalır). ASCII sütununda 0x20–0x7E dışı `.`.
- Her boyutta sayfalı (§4.1'in bayt konumlu kaydırıcısı, `scroller.rs`): görünen pencere için 4 KB okunur. 64 GB'lık dosyada da bellek sabit.
- Açılış: çubuktaki `Hex` / `H` / `preview-hex` eylemi; ikili dosya için hiçbir zaman kendiliğinden değil (bugünkü küçük resim/simge kalır, çubukta `Hex` düğmesi; karar 7). Seçim değişince kip bırakılır (bir sonraki dosya yine kendi varsayılanıyla açılır).

## 5. PDF ve yakınlaştırma (13b)

### 5.1 pdfium görüntü işçisi

- Bugünkü işçi tek istek okuyup çıkar; önizleme için uzun yaşayan bir kip eklenir: `gezik --pdf-view` (`gezik-batch/src/pdf/view.rs`, yeni). Gezik'in kendi exe'si (pdfium-render zaten bağlı), ayrı süreç: pdfium çökerse ya da bellek isterse Gezik etkilenmez (bugünkü ilke).
- **Protokol** (stdin satırları; stdout'ta satır başlığı + ham bayt):
  ```
  → open <library path>\t<pdf path>            ← pages 12\t612x792 612x792 …   (points)
  → password <text>                             ← needs-password | wrong-password | pages …
  → render <page> <scale> <x> <y> <w> <h>       ← image <page> <w> <h>\n<w*h*4 bytes RGBA>
  → quit (or stdin closes)                      ← (exits)
  ```
  Parola yalnız stdin'den, asla komut satırından (bugünkü kural). Yanıt sınırları: başlık satırı ≤ 1 KB, görüntü ≤ 4096 × 4096 (64 MB); aşan yanıt protokol hatası, işçi öldürülür.
- **İstemci** (`preview/pdf.rs`): bir işçi, bir açık belge; yeni istek gelince yanıtı beklenen eski istek iptal sayılır (yanıtı okunup atılır). İşçi çökerse çubukta `This PDF could not be shown` ve sistem küçük resmi; sonraki PDF yeni işçi kurar.
- **pdfium yoksa:** sayfa 1 bugünkü gibi sistem küçük resmi; çubukta `‹ ›` yerine `More pages need pdfium (~2.4 MB) · Download` (§3.3).
- **Parolalı PDF:** çubukta `Password protected · Enter Password…` → bugünkü parola sorusu (`dialog.rs`); parola yalnız o önizleme için bellekte.

### 5.2 Yalnız görünen sayfa, gereken boyutta

- Sayfa 1 açılışta bugünkü küçük resimdir; görüntüleyici yalnız sayfa değişince ya da yakınlaştırınca kurulur.
- **Sığdır** (varsayılan): sayfa görünen alanın fiziksel piksel boyutuna sığacak ölçekte, tek görüntü.
- **Yakınlaştırılmış:** yalnız **görünen bölge** çizilir (`render`'ın `x y w h` kırpması; pdfium'un matrisli çizimi): bit eşlem her zaman ≤ görünen alanın fiziksel boyutu. Sürükleyerek kaydırırken eldeki görüntü kaydırılır ve kenarlar boş kalır; bırakınca (ya da 150 ms durunca) yeni bölge istenir. Bellek: en çok iki görüntü (gösterilen + gelen).
- Önceden çizim yok (sonraki sayfa çizilmez; karar 9). Sayfa sayısı ve boyutları `open` yanıtından; 10.000 sayfalık belgede de tek satır.
- Yakınlaştırma %25–%800, adımlar 25/50/75/100/125/150/200/300/400/600/800; Ctrl+teker imlecin altındaki noktayı sabit tutar.
- Sayfa değişince yakınlaştırma kalır (sığdır seçiliyse sığdırır). Dosya değişince sıfırlanır.

### 5.3 Resimde yakınlaştırma (aynı yüzey)

- Bugünkü resim önizlemesi sığdırılmış ve ≤ 1024 px; yakınlaştırınca (`+`, Ctrl+teker, çift tık %100) resim bir kez **gerçek boyutunda** çözülür (`decode_image` hedef boyutsuz), yüzey kırpar ve kaydırır. Gerçek boyut 48 MP'i aşan resimde yakınlaştırma 1024 px'lik görüntüyle sınırlı kalır, çubukta `Too large to zoom (108 MP)` (bellek ≤ 192 MB tavanı; karar 11).
- Sığdır'a dönünce (ya da dosya değişince) büyük görüntü bırakılır.
- Yakınlaştırma yüzeyi (`zoom.rs`) PDF ile ortak: ölçek, kaydırma, imleç sabit yakınlaştırma, sürükleme, tuşlar.

## 6. Video ve ses (13c)

### 6.1 Poster ve oynatma öncesi

- **Video posteri** = bugünkü önizleme görüntüsü (sistemin küçük resmi: Windows kabuğu, macOS QuickLook, Linux küçük resim önbelleği; yoksa simge). Oynat'a basılmadan ffmpeg çalışmaz, tek kare çözülmez (kullanıcı kararı).
- **Ses posteri** = dosyanın gömülü kapağı (ID3 `APIC`, MP4 `covr`, FLAC `PICTURE`; 13d'nin ses okuyucusu, ≤ 8 MB, `image` sandığıyla çözülür) yoksa küçük resim ya da simge. Kapak bir resimdir, ses değil: "çözülmez" kuralı sesin kendisi için.
- Posterin üstünde ortada `▶` düğmesi; çubukta süre (13d'nin alanından, yoksa `--:--`).

### 6.2 Oynatma hattı

- **ffmpeg iki süreç** (`ChildProcess::spawn_binary`, §3.3):
  - görüntü: `ffmpeg -nostdin -hide_banner -hwaccel auto -ss <t> -i <file> -map 0:v:0 -an -sn -vf "scale=<w>:<h>:force_original_aspect_ratio=decrease,fps=<r>" -pix_fmt rgba -f rawvideo pipe:1`
  - ses: `ffmpeg -nostdin -hide_banner -ss <t> -i <file> -map 0:a:0 -vn -sn -ac 2 -ar 48000 -f s16le pipe:1`
  
  `<w>×<h>` = görünen alanın fiziksel boyutu, en çok 1280 × 720 (yazılım çizicisinin maliyeti; karar 15); `<r>` = kaynağın kare hızı, en çok 30. Döndürme meta verisini ffmpeg kendisi uygular. Kaynak bilgisi (kare hızı, boyut, süre, ses izi var mı) oynat'ta bir kez `ffprobe`'dan (bugünkü `duration_us`'un yanında `probe`, 10 sn zaman aşımı).
- **Saat:** ses varsa ses aygıtının çaldığı örnek sayısı; yoksa duvar saati (`Instant`). Görüntü iş parçacığı kare n'yi `n / r` zamanında gösterir; geç kalırsa kareyi okuyup atar, erkense bekler. Kare halkası 3 kare (720p'de ≈ 11 MB), ses halkası 0,5 sn.
- **Gösterim:** kare `SharedPixelBuffer`'a kopyalanır ve ana iş parçacığına bugünkü `windows::post` yoluyla gönderilir; önizleme `Image`'ı yalnız o alanı yeniden çizer. Pencere küçültülmüş ya da önizleme görünmüyorsa kare gönderilmez (ses sürer).
- **Sarma:** iki süreç öldürülür ve yeni `-ss` ile yeniden başlar (girişte `-ss`: ffmpeg anahtar kareden çözüp hedefe kadar atar, doğru konum). Sarma çubuğu sürüklenirken yalnız süre yazısı değişir; bırakınca sarılır. `J`/`L` 10 sn.
- **Duraklat:** okumayı bırakır (borular dolar, ffmpeg bekler), ses aygıtı durur; 60 sn duraklamadan sonra süreçler kapanır, kareler bırakılır, son kare poster olarak kalır, devam'da `-ss <konum>` ile yeniden başlar (§3.6 madde 3).
- **Son:** dosyanın sonunda durur, son kare kalır; `▶` baştan çalar. Döngü yok.
- **Ses düzeyi:** örnekler yazılımda ölçeklenir (0–100, kare kök eğrisi); `M` kapatır. `state.toml [preview] volume = 80`, `muted = false` (tek değer, bütün dosyalar).
- **Hata:** ffmpeg sıfırdan farklı çıkarsa çubukta `Cannot play this file` ve stderr'in son satırı ipucunda (bugünkü `stderr_tail`); poster kalır.

### 6.3 Ses aygıtı (platform)

`gezik-platform/src/audio/{mod, windows, mac, linux}.rs` (yeni): tek arayüz, 48 kHz, 2 kanal, s16:

```rust
pub struct AudioOut { /* platform */ }
impl AudioOut {
    pub fn open() -> io::Result<AudioOut>;                 // the default output device
    pub fn write(&mut self, samples: &[i16]) -> io::Result<()>; // blocks while the device is full
    pub fn played_frames(&self) -> u64;                    // the clock
    pub fn pause(&mut self); pub fn resume(&mut self);
}                                                          // Drop closes the device
```

- **Windows:** WASAPI paylaşımlı kip (`IAudioClient`, `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | SRC_DEFAULT_QUALITY` ile s16/48 kHz kabul edilir), olayla beslenen; `windows` sandığına `Win32_Media_Audio` özelliği. Aygıt değişirse (kulaklık takıldı) `AUDCLNT_E_DEVICE_INVALIDATED` → aygıt yeniden açılır, konum korunur.
- **macOS:** AudioToolbox `AudioQueue` (C işlevleri, çerçeve bağlantısı; Swift/ObjC yok), üç tampon.
- **Linux:** PulseAudio basit API'si `libpulse-simple.so.0` **çalışırken `dlopen`** ile (PipeWire'ın `pipewire-pulse`'ı da sunar); başlangıçta bağımlılık yok. Kütüphane yoksa video sessiz oynar, çubukta `No sound: PulseAudio or PipeWire is not available` (karar 16).
- Ses aygıtı yalnız oynatırken açıktır; duraklatmada 60 sn sonra, durdurmada hemen kapanır.

### 6.4 ffmpeg yoksa ya da eskiyse

- `▶`'a basınca teklif kutusu (`Need::Playback`: `Playing video and audio needs ffmpeg (~21 MB, free)`; boyut platforma göre bugünkü tablodan). İndirme bitince oynatma kendiliğinden başlar (bugünkü `again` geri çağrısı).
- Herhangi bir ffmpeg sürümü yeter (ham çıktı ve `-ss` eski sürümlerde de var).

## 7. Meta veri alanı kaydı ve panel (13d)

### 7.1 Kayıt (`gezik-core/src/fields.rs`, yeni, saf)

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FieldId(u8);                    // index into FIELDS

pub struct FieldDef {
    pub key: &'static str,                 // "artist", "duration", "camera": settings, filter words, templates
    pub label: &'static str,               // "Artist": column header, panel, pickers
    pub group: Group,                      // Picture, Photo, Media, Music, Document, Tags
    pub kind: ValueKind,                   // Text, Int, Real, Duration, Date, Geo
    pub right_align: bool,                 // numbers and durations in columns
    pub width: u16,                        // a new column's width (logical px)
    pub groupable: bool,                   // 10d grouping (text fields and years)
}

pub static FIELDS: [FieldDef; N];          // N ≤ 64 (FieldSet is a u64)
pub fn by_key(key: &str) -> Option<FieldId>;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct FieldSet(u64);                  // which fields a consumer needs

pub enum Value { Text(Box<str>), Int(i64), Real(f64), Duration(u64 /* ms */), Date(DateParts), Geo(f64, f64) }
pub struct Fields(Box<[(FieldId, Value)]>); // one file's values, sorted by id; missing = absent

pub fn text(id: FieldId, v: &Value) -> String;               // the one display form (panel, column)
pub fn name_text(id: FieldId, v: &Value, arg: Option<&str>) -> String; // the template form (§9.2)
pub fn compare(a: Option<&Value>, b: Option<&Value>) -> Ordering;   // missing last
pub fn parse_value(kind: ValueKind, text: &str) -> Result<Value, String>; // "3m", "1:30", "2024-05", "50 mm"

/// Which readers can fill what, by extension: pure description (no I/O).
pub struct ReaderDef { pub reader: Reader, pub exts: &'static [&'static str], pub provides: FieldSet }
pub static READERS: [ReaderDef; M];
pub fn readers_for(ext: &str, want: FieldSet) -> impl Iterator<Item = Reader>; // none: nothing is read
```

- **Biçim başına okuyucu, alan başına değil** (WDX'in "algıla + alanlar" fikri): bir dosya için uzantısına uyan ve istenen alanlardan en az birini sağlayan okuyucular çalışır; her biri dosyanın kendi bölümünü **bir kez** okur ve bildiği bütün alanları doldurur; istenmeyenler atılır. İstenen alanlarla kesişmeyen okuyucu hiç çalışmaz (karar 19).
- Alan sayısı ≤ 64 (`FieldSet` = `u64`); bu spec'te 34 (§7.2).

### 7.2 Alanlar

| Grup | Anahtar (`key`) | Etiket | Tür | Okuyucu |
|---|---|---|---|---|
| Picture | `width`, `height` | Width, Height | Int (px) | Resim başlığı; video kapsayıcısı |
| | `dimensions` | Dimensions | Int (piksel sayısı; `4032 × 3024` gösterilir) | aynı |
| Photo | `taken` | Date Taken | Date | EXIF |
| | `camera` | Camera | Text (Make + Model, yinelenen marka atılır) | EXIF |
| | `lens`, `exposure`, `aperture`, `iso`, `focal-length` | Lens, Exposure, Aperture, ISO, Focal Length | Text, Real (sn; `1/250 s`), Real (`f/2.8`), Int, Real (`50 mm`) | EXIF |
| | `gps` | Location | Geo (`41.0082° N, 28.9784° E`) | EXIF |
| Media | `duration` | Duration | Duration (`3:25`, `1:02:03`) | Ses/video kapsayıcısı |
| | `video-codec`, `audio-codec` | Video Codec, Audio Codec | Text (`H.264`, `AAC`) | kapsayıcı |
| | `frame-rate`, `bitrate`, `sample-rate`, `channels` | Frame Rate, Bit Rate, Sample Rate, Channels | Real, Int (kb/s), Int (Hz), Int | kapsayıcı |
| Music | `title` | Title | Text | ses etiketleri; belge özellikleri |
| | `artist`, `album`, `album-artist`, `genre`, `composer` | Artist, Album, Album Artist, Genre, Composer | Text | ses etiketleri |
| | `track`, `disc`, `year` | Track, Disc, Year | Int | ses etiketleri |
| Document | `author`, `subject` | Author, Subject | Text | belge özellikleri |
| | `pages`, `words` | Pages, Words | Int | belge özellikleri, PDF |
| Tags (12) | `tags`, `rating`, `comment` | Tags, Rating, Comment | Text, Int, Text | 12'nin `gezik-ops::meta` okuması |

- `title` müzikte ve belgede aynı alandır (Explorer'ın `System.Title`'ı gibi): sütunda ikisi bir arada sıralanır.
- 12'nin alanları kayda girer, yeni okuma yolu açmaz: `Reader::Meta` 12'nin `meta::read`'ini çağırır. Böylece ad şablonunda `{tags}`, `{rating}` (12 spec'inin 13'e bıraktığı) ve sütun seçicide tek liste bedavadır.
- Bugünkü sütunlar (ad, boyut, tarihler, tür) ve şablon alanları (`{name}`, `{ext}`, `{n}`, `{parent}`, `{date}`, `{size}`) kayda taşınmaz: G/Ç'siz ve zaten çalışıyorlar (karar 20). `{taken}` şablonda bugünkü anlamıyla kalır (EXIF tarihi, yoksa değiştirme tarihi); kayıttaki `taken` aynı okuyucudan.

### 7.3 Okuyucular (`gezik-batch/src/fields/`, yeni)

| Okuyucu | Uzantılar | Okunan bölüm (en çok) | Not |
|---|---|---|---|
| `image` | png jpg jpeg gif bmp webp tif tiff ico heic heif avif | başlık, ≤ 64 KB | `image` sandığının `into_dimensions`'ı; HEIF/AVIF `ispe` kutusu kendi kodla; EXIF yönü 5–8 ise genişlik/yükseklik yer değiştirir (`shown_size`) |
| `exif` | jpg jpeg tif tiff heic heif webp png dng cr2 nef arw | JPEG ≤ 256 KB, öbürleri ≤ 16 MB (bugünkü `exif.rs` sınırları) | `kamadak-exif` (zaten bağımlılık); `taken()` bu okuyucuya taşınır |
| `audio` | mp3 flac ogg opus m4a aac wav aiff | ID3v2 başta (≤ 1 MB, kapak hariç), ID3v1 son 128 B, MP4 `moov/udta/meta/ilst`, FLAC/OGG/Opus Vorbis yorumu, WAV `LIST/INFO` | kendi kodu; süre: MP3 Xing/VBRI başlığı, yoksa sabit bit hızı tahmini (`≈ 3:25`), FLAC STREAMINFO, OGG son granül (son 64 KB), WAV veri boyutu. WMA/ASF yok (karar 21) |
| `video` | mp4 m4v mov mkv webm avi | MP4: üst düzey kutu başlıkları arayarak `moov` (dosya sonunda olabilir; ≤ 4 MB), MKV/WebM: EBML `Segment/Info`, `Tracks` (≤ 1 MB), AVI `avih`/`strh` | kendi kodu: süre, boyut, kodek, kare hızı, bit hızı, ses kodeki; MP4 `ilst` etiketleri `audio` okuyucusuyla ortak |
| `document` | docx xlsx pptx odt ods odp | ZIP içinde `docProps/core.xml` + `app.xml` ya da `meta.xml`, her biri ≤ 256 KB açılmış | `zip` sandığı (zaten bağımlılık); küçük etiket çıkarıcı, 5 temel XML varlığı; başlık, yazar, konu, sayfa, sözcük |
| `pdf` | pdf | baştaki ve sondaki 64 KB | `/Info` sözlüğü (Title, Author, Subject; PDFDocEncoding ve UTF-16BE) ve kök `/Pages /Count`; nesne akışında (sıkıştırılmış) ise bulunamaz → alan yok. Panelde pdfium işçisi açıksa (13b) `pages` ondan |
| `meta` | hepsi | 12'nin akış/xattr okuması | 12a'nın `meta::read` |

- **Ortak kurallar (güvenlik sınırı: dosya içeriği güvenilmez):**
  - Her okuyucu `Read + Seek` üzerinde `take()` ile sınırlı okur; uzunluk alanlarından ayrılan bellek sınırlıdır (çerçeve ≤ 1 MB, kapak ≤ 8 MB, metin değeri ≤ 1.000 karakter, kesilir).
  - `fields` modülünde `#![deny(clippy::indexing_slicing)]`: dizinleme yerine `get()`; yayın derlemesi `panic = "abort"` olduğundan bir okuyucu paniği Gezik'i kapatır, `catch_unwind` işe yaramaz. Her okuyucu için kesilmiş ve rastgele baytlarla 10.000 girişlik testler (sandık eklemeden küçük bir xorshift) (karar 22).
  - Metin değerlerinden kontrol karakterleri atılır; çözülemeyen kodlama `encoding_rs` ile (ID3v1 Latin-1, ID3v2 UTF-16/UTF-8).
  - Yalnız-bulut dosya okunmaz (`only_in_cloud`; alanlar boş). Okuma `FILE_FLAG_SEQUENTIAL_SCAN`'siz, bugünkü dosya açma yolundan.
- Giriş: `pub fn read(path: &Path, ext: &str, want: FieldSet, stop: &dyn Fn() -> bool) -> Fields` (`gezik-batch/src/fields/mod.rs`); `stop` okuyucular arasında denenir.

### 7.4 Panel

- Önizlemenin bilgi satırlarının (`details`) altında `Properties` bölümü: dosyanın değeri olan bütün alanları gruplarıyla (`Photo`, `Music`, `Media`, `Document`), `Label  Value` satırları, ikinci sütun hizalı. Değer yoksa bölüm yok.
- Okuma: bugünkü önizleme işçisinde, gövde (resim/metin) yüklendikten sonra, `FieldSet::ALL` ile; ayrı bir sonuç olarak gelir (resim beklemez). Tek dosya, bir kez.
- `PreviewInfo.properties: [PropRow { group, label, value }]`; satır sağ tıkla `Copy` (değer panoya), `Show Column` (alanı etkin görünümün sütunlarına ekler, 13e'de).
- Bilgi penceresi (`info.rs`, 9) aynı satırları kendi `Properties` bölümünde gösterir (aynı `read`, aynı `text`).
- `[preview] properties = true` (false: bölme bölümü göstermez, okumaz).
- Konum alanında harita bağlantısı yok (dış siteye konum göndermek; karar 23), `Copy` var.

### 7.5 Tembel okuma (sütunlar ve kurallar için; 13e)

12'nin okuyucusu (`gezik/src/meta.rs`, 12 §3.4) genelleşir: istek "bu satırların meta verisi" yerine "bu satırların şu `FieldSet`'i". Aynı iş parçacığı, aynı 64'lük kümeler, aynı iptal, aynı tam klasör okuması ve `Reading properties… 4,000 of 12,000` ilerlemesi, aynı önbellek düşme kuralları (klasör değişince, izleyici olayında, F5). Önbellek satır başına `Fields` (yalnız boş olmayanlar) + okunmuş bit kümesi; istenen küme büyüyünce (yeni sütun açıldı) yalnız eksik okuyucular çalışır.

## 8. Sütunlar, sıralama, gruplama, süzgeç, görünüm kuralları (13e)

### 8.1 Sütun

- `ColumnKey::Field(FieldId)` (`gezik-core/src/view.rs`; `ColumnKey` `Copy` kalır). `as_str` alanın `key`'i; `views.toml`'da ve `[[view-rules]] columns`'ta aynı adla (`columns = ["modified", "size", "duration", "artist"]`). Bilinmeyen ad bugünkü uyarı kalıbıyla atlanır.
- Başlık menüsünde bugünkü sütun anahtarlarından sonra `More Columns…` (`more-columns` eylemi): seçici (`picker.slint`), alanlar gruplarıyla, açık olanlar işaretli, yazınca süzülür. Bir görünümde en çok 8 alan sütunu (satır genişliği ve okuma maliyeti; karar 24).
- `ColumnKey::RESULTS`'a da girer (arama sonuçları, düz görünüm).
- Hücre: okunmamışsa boş; okunmuş ve değersizse boş; değer `fields::text`. Sağa hizalı sayılar.

### 8.2 Sıralama ve gruplama

- `SortKey::Field(FieldId)`: `fields::compare`, değersizler artan sırada sonda, eşitlikte ad. Bütün klasörün alanını ister (§7.5; 12'nin "okuma bitene kadar sıra değişmez" kuralı).
- `GroupBy::Field(FieldId)`: yalnız `groupable` alanlarda (metin alanları: artist, album, album-artist, genre, composer, camera, lens, author; `year`; `taken` yıla göre). Değersizler `No <Label>` grubunda, sonda. Sayısal alanlarda gruplama yok, sıralama var (karar 25).
- `[[view-rules]]`: `sort = "duration"`, `group = "artist"`. 10f'nin kural koşulları değişmez: kural klasör açılmadan seçilir ve G/Ç yapmaz; alan yalnız kuralın **sonucunda** (sütun, sıralama, gruplama) kullanılır (karar 26). Örnek: `[[view-rules]] path = "~/Music/**"`, `columns = ["artist", "album", "track", "duration"]`.

### 8.3 Süzgeç sözcükleri

- 12c'nin süzgeç sözcükleri (`tag:`, `rating:`) genelleşir: kayıttaki her alan bir sözcüktür. Sözdizimi (`gezik-core/src/field_filter.rs`, yeni, saf; 12'nin `meta_filter.rs`'i bunun içine taşınır):
  ```
  artist:tarkan        contains (case-insensitive, pattern::fold_text)
  artist="Sezen Aksu"  equals
  duration>3m  width>=1920  year<2000  taken>=2024-05  pages<=10
  camera:any  album:none
  ```
  Birden çok sözcük VE; ad deseniyle birlikte (`*.mp3 artist:tarkan duration>3m`). Değer `fields::parse_value` ile türüne göre (süre `90s`, `3m`, `1:30`; tarih `2024`, `2024-05`, `2024-05-01`; gerçek sayı `2.8`, `50mm`); hatalı değer süzgeç çubuğunda bugünkü hata ipucuyla (`duration>3x: expected a length like 3m or 1:30`).
- Yalnız kayıttaki anahtarlar sözcüktür; `foo:bar` ad deseni kalır. Unix'te `:` içeren adı aramak için tırnak (12'nin kuralı).
- Okunmamış satırlar okuma bitene kadar gösterilir, soluk (12'nin karar 18'i).

## 9. Arama, ad şablonu, vurgulama kuralları (13f)

### 9.1 Arama ölçütü

- `SearchSpec.fields: String` (süzgeç sözcükleri, §8.3; boş = ölçüt yok); `[[searches]] where = "artist:tarkan duration>3m"`, `state.toml` oturumu da. Derlenmiş biçim `FieldFilter` (`field_filter.rs`).
- Arama çubuğunun ek seçenekler satırında `Properties` metin alanı, anahtar tamamlamalı (yazarken `art` → `artist:`, `album-artist:`; bugünkü `path-suggestions.slint` deseni).
- **Sıra:** `Query::compile` alan ölçütünü ad, tür, boyut, tarih, 12'nin meta ölçütlerinden sonra, içerik aramasından önce koyar ("en ucuzdan başla"). Yalnız uzantısı bir okuyucuya uyan adaylar okunur (`readers_for` boşsa aday elenir: `duration>3m` bir `.txt`'i okumaz). Ölçüt yoksa hiçbir okuma yok (sayaç).
- `gezik-search` alanları `gezik-batch::fields::read` ile okur (yeni bağımlılık `gezik-search → gezik-batch`; döngü yok; karar 27). Everything (Windows) adayları verir, Gezik süzer (12'nin kuralı).

### 9.2 Ad şablonu

- Şablon alanları kayıttan genişler: `{artist}`, `{album}`, `{track:02}`, `{year}`, `{camera}`, `{width}x{height}`, `{duration}`, `{tags}`, `{rating}`… (`Piece::Field(FieldId, Option<String>)`, `gezik-core/src/batch/template.rs`). Bilinmeyen alan bugünkü hata (`unknown field {foo}`).
- **Biçim argümanı** türe göre: Int → sıfır dolgulu genişlik (`{track:02}`, `{n:03}` gibi); Date → tarih biçimi (`{taken:%Y}` gibi); Real → ondalık sayısı (`{aperture:1}`); Duration → `3m25s`/`1h02m03s` (ad içinde `:` olamaz), argüman yok; Text → argüman yok.
- **Boş değer:** `{artist|Unknown}` — `|`'den sonrası değer yoksa kullanılır (`|` Windows adlarında yasak olduğu için şablonda çakışmaz). Yedeksiz boş alan boş metin olur ve önizleme satırında uyarı (`no artist`); bugünkü çakışma/boş ad denetimleri aynen işler.
- **Ad güvenliği:** alan değerleri ada girmeden önce temizlenir: `/ \ : * ? " < > |` ve kontrol karakterleri `_` olur, baş/son boşluk ve nokta atılır, `.` ve `..` tek başına `_` olur, değer başına ≤ 100 karakter. Böylece kötü niyetli bir etiket (`artist = "../../x"`) yolu değiştiremez (karar 22'nin ikinci yarısı).
- `uses_taken` → `needed(pieces) -> FieldSet`; toplu ad değiştirme önizlemesi yalnız şablonun istediği okuyucuları, yalnız seçili dosyalarda, bugünkü arka plan yolundan çalıştırır.
- Şablon alanının yanında `Fields ▾` düğmesi: kaydın alanlarını gruplarıyla listeler, seçilen `{key}` imlecin yerine yazılır.

### 9.3 Vurgulama kuralları (12f)

- Yeni koşul `where` (süzgeç sözcükleri, §8.3): `[[highlight-rules]] where = "duration>1h"`, `color = "purple"`. Öbür koşullarla VE.
- 12f'nin meta koşulu kuralı aynen: alanı okunmamış satırda kural "henüz bilinmiyor"dur ve sonraki kurallara geçilmez (12 karar 23); `where`'li kural yoksa alan okuyucusu bu yüzden kurulmaz. Derleme ayar yüklenince bir kez; satır başına maliyet G/Ç'siz kısım için ≤ 1 µs kalır.
- Görünüm kurallarında (10f) koşul değil yalnız sonuç (§8.2).

## 10. Ayarlar, eylemler, menüler

### 10.1 Ayarlar

```toml
[preview]
properties = true        # the Properties section in the preview pane and Quick Look

# [[highlight-rules]] where = "…"   — see §9.3
# [[view-rules]] columns/sort/group take field keys — see §8.2
# [[searches]] where = "…"        — see §9.1
```

`state.toml` (Gezik yazar): `[preview] volume`, `muted`, `wrap-text`, `wrap-code`. `views.toml`: alan sütunları, `sort`/`group` alan anahtarları. Tema dosyaları: `syntax-*` renkleri (§4.3). Hepsi `templates/settings.toml`'a yorumlarıyla ve `settings_writer`'ın bilinen anahtarlarına girer.

### 10.2 Eylemler

| Eylem | Başlık | Tuş | Parça |
|---|---|---|---|
| `preview-hex` | Show Preview as Hex | — | 13a |
| `preview-play` | Play/Pause Preview | — | 13c |
| `more-columns` | More Columns… | — | 13e |

- `Action` 124 → 125 (13a) → 126 (13c) → 127 (13e). Sayı yalnız `Action::ALL`'ın dizi türünde (PR #87).
- Varsayılan tuş yok; görüntüleyici içi tuşlar §3.2 (karar 13).
- Menüler: `View ▸ Columns ▸ More Columns…` (macOS menü çubuğu ve başlık menüsü), önizleme alanında sağ tık `Show as Hex`/`Show as Text`, `Copy Value` (panel), `Wrap Lines`.

### 10.3 Menü kimlikleri

13 için **2600–2799**: `PREVIEW_*` 2600–2619 (hex/metin, sarma, oynat, kopya), `COLUMNS_MORE` 2620, alan sütunu aç/kapa 2630–2693 (en çok 64 alan), `Sort By ▸ More…` (alanla sıralama seçicisi; alan başına öğe yok) 2700, gruplama `Group By ▸ <alan>` 2710–2739, `Fields ▾` şablon menüsü 2740–2799 (kendi açılır menüsü, 60 alan). `GEZIK_IDS_END = 4096` değişmez.

## 11. Kod yapısı

| Parça | Yer |
|---|---|
| Önizleme | `gezik/src/preview/{mod, viewer, text, hex, scroller, zoom, pdf, media, fields}.rs` (bugünkü `preview.rs` bölünür), `gezik/src/quick_look.rs` (görüntüleyici tuşları), `ui/widgets/preview-pane.slint` (çubuk, satırlar, yüzey, oynatıcı), `ui/theme.slint` (`syntax-*`) |
| Renklendirme, metin | `gezik-core/src/syntax.rs` (yeni), `gezik-core/src/text.rs` (`display_width`) |
| PDF | `gezik-batch/src/pdf/view.rs` (yeni; `--pdf-view`), `gezik-batch/src/pdf/pdfium.rs` (kırpmalı çizim), `gezik/src/main.rs` (yeni argüman) |
| Süreç, ses | `gezik-platform/src/process.rs` (`spawn_binary`), `gezik-platform/src/audio/{mod, windows, mac, linux}.rs` (yeni), `gezik-batch/src/convert/ffmpeg.rs` (`probe`) |
| Kayıt | `gezik-core/src/fields.rs` (yeni), `gezik-core/src/field_filter.rs` (yeni; 12'nin `meta_filter.rs`'i içine) |
| Okuyucular | `gezik-batch/src/fields/{mod, image, exif, audio, video, document, pdf}.rs` (yeni; `exif.rs` buraya taşınır) |
| Panel, Info | `gezik/src/preview/fields.rs`, `gezik/src/info.rs`, `ui/widgets/{preview-pane, info}.slint` |
| Sütun, sıralama, gruplama, süzgeç | `gezik-core/src/{view, sort, group}.rs`, `gezik/src/meta.rs` (12'nin okuyucusu `FieldSet`'le), `gezik/src/view/{model, listing, columns}.rs`, `gezik/src/filter.rs`, `gezik-config/src/{views_file, view_rules}.rs` |
| Arama | `gezik-core/src/search.rs` (`fields`), `gezik-search/src/query.rs`, `gezik/src/search.rs`, `gezik-config/src/settings.rs` (`[[searches]] where`) |
| Şablon | `gezik-core/src/batch/template.rs`, `gezik/src/batch_rename.rs`, `ui/widgets/rename-batch.slint` |
| Vurgulama | `gezik-core/src/highlight.rs` (12f), `gezik-config/src/highlight_rules.rs` (12f) |
| Araç teklifleri | `gezik/src/archives.rs` (`Need::Playback`, `Need::PdfPages`) |

## 12. Bütçe ve performans

| Parça | Exe (tahmin; sınır ≤ +262.144 B) | Boşta bellek | Kullanılırken |
|---|---|---|---|
| 13a | ≈ +140 KB (dil tabloları ≈ 30, çözücü ≈ 15, satır/hex görüntüleyici ve Slint ≈ 95) | 0 | 2 MB'lık metin ≤ 60 ms'de görünür, bellek ≤ 3 × dosya; sayfalı ≤ 1 MB, 10 GB'ta sona atlama ≤ 100 ms; hex ≤ 64 KB |
| 13b | ≈ +100 KB (işçi kipi ≈ 25, istemci ≈ 25, yüzey ve Slint ≈ 50; pdfium-render zaten bağlı) | 0 | sayfa ≤ 150 ms; Gezik tarafında ≤ 2 görüntü (görünen alan boyutunda); işçi süreci PDF'ten çıkınca 10 sn'de kapanır |
| 13c | ≈ +130 KB (oynatıcı ≈ 50, WASAPI ≈ 30, AudioQueue ≈ 15, Pulse `dlopen` ≈ 10, Slint ≈ 25) | 0 | ilk kare ≤ 500 ms; 720p'de Gezik ≤ %25 bir çekirdek; ≤ 12 MB tampon; durunca ≤ 1 sn'de 0 |
| 13d | ≈ +200 KB (kayıt ≈ 20, okuyucular ≈ 150, panel ≈ 30) | 0 | tek dosya ≤ 30 ms |
| 13e | ≈ +80 KB | 0 | 40 satır ≤ 50 ms; 10.000 öğe Windows ≤ 4 sn, Linux/macOS ≤ 2 sn; önbellek değerli dosya başına ≤ 200 B |
| 13f | ≈ +70 KB | 0 | alan ölçütsüz arama `master` ±%2; şablon önizlemesi 1.000 MP3'te ≤ 1 sn |

Toplam ≈ +720 KB exe. Varsayılan ayarlarla boşta **0** (her parça ±0,02 MB), açılış ≤ +1 ms, klasör açma `master` ±%2 (alan sütunu yokken).

- **13d riskli:** okuyucular tahmini en belirsiz kalem. 13d'nin ilk görevi kaydı ve `image`+`exif` okuyucusunu yazıp ölçer; çizgi aşılacaksa `document` ve `pdf` okuyucuları 13e'ye geçer (planın kararı, PR'da yazılır).
- **Ölçüm:** her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1` ve `stress.ps1`'i yazar. Yeni betik `scripts/perf/preview.ps1`: 2 MB kod dosyası açılış süresi ve belleği, 10 GB günlükte sona atlama, 100 sayfalık PDF'te sayfa süreleri, 1080p videoda ilk kare süresi ve 60 sn oynatmada Gezik'in CPU'su, durdurduktan sonra bellek ve süreç sayısı, 10.000 JPEG/MP3 klasöründe görünen satır ve tam okuma süreleri.
- **Yazılım çizicisi:** oynatırken yalnız önizleme `Image`'ı her karede yeniden çizilir; satır başına en çok 64 renk parçası.

## 13. Parçalar (her biri bir plan ve bir PR)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **13a** | Önizleme modülünün bölünmesi, `Viewer`, çubuk, bellekte ve sayfalı metin, sarma, renklendirme, hex, `preview-hex` (§3.1–§3.2, §4) | — |
| 2 | **13b** | `spawn_binary`, pdfium görüntü işçisi, PDF sayfa/yakınlaştırma, resim yakınlaştırma (§3.3, §5) | 13a (`Viewer`, çubuk, `preview-pane.slint` yapısı) |
| 3 | **13c** | Video/ses oynatma, ses aygıtları, `preview-play` (§6) | 13b (`spawn_binary`, araç teklifi yolu) |
| 4 | **13d** | Alan kaydı, okuyucular, panel, Info (§7.1–§7.4) | 13a (panelin yeri); `meta` okuyucusu **12a**'yı ister; kaydın saf görevi (`fields.rs`) 13a ile aynı anda yürüyebilir |
| 5 | **13e** | Alan sütunları, `More Columns…`, sıralama, gruplama, süzgeç sözcükleri, görünüm kurallarında alanlar (§7.5, §8) | 13d; **12c** (okuyucu ve süzgeç yolu) |
| 6 | **13f** | Arama ölçütü, ad şablonu alanları, vurgulama kuralında `where` (§9) | 13e (`field_filter`); **12e** (arama alanları), **12f** (vurgulama kuralları) |

Sıra bağımlılık ve değer: metin görüntüleyicisi önizlemenin yeni yapısını (görüntüleyici, çubuk, satır modeli) dış araç gerektirmeden kurar ve en sık görülen dosyalara (kod, günlük) gider. PDF ikili boruyu getirir, oynatıcı onu kullanır. Kayıt önizlemeden bağımsızdır ama paneli önizlemenin yeni yerine oturur. Tüketiciler kayıttan ve 12'nin okuyucusundan sonra gelir; 13e/13f, 12c/12e/12f birleşmeden başlamaz.

### 13.1 Süreç (11'in dersleri)

- **Aynı ortak listelere dokunan parçalar paralel yürütülmez.** 13a, 13b ve 13c aynı `preview-pane.slint`'e, `PreviewInfo`'ya ve `preview/` modülüne; 13a, 13c, 13e `Action`'a; hepsi `macos-test.md`/`linux-test.md`'ye dokunur: **sırayla** yapılır ya da bir öncekinin dalının üstüne yığılır (stacked PR), asla ikisi aynı anda `master`'dan değil. Yalnız 13d'nin saf `fields.rs` görevi (Slint'siz, listesiz) 13a ile aynı anda yürüyebilir.
- **Eylem sayısı yalnız dizinin türünde** (PR #87); testler sayıyı tekrar etmez.
- **Menü kimlikleri ve denetim listesi aralıkları önceden ayrıldı** (§10.3, aşağıda): birleştirmede numara kaymaz.
- Her parçada Windows → macOS → Linux sırasıyla yazılır; bir sistemde doğrulanamayan kısım PR açıklamasında "doğrulanacak" notuyla. Çapraz denetim yalnız planın sonunda (`cargo check` macOS ve Linux hedefleri); Docker yok. Windows ekran testleri kullanıcı uzaktayken.

### 13.2 Parça ayrıntıları

Denetim listesi aralıkları: macOS 366'dan, Linux 266'dan başlar (12: macOS 340–364, Linux 249–265).

**13a — metin, kod, hex**
- *Kapsam:* §3.1, §3.2, §3.5, §4; `preview-hex`.
- *Dosyalar:* `gezik/src/preview/{mod, viewer, text, hex, scroller}.rs`, `gezik/src/{quick_look, actions, context_menu, palette}.rs`, `gezik-core/src/{syntax, text}.rs`, `ui/widgets/preview-pane.slint`, `ui/theme.slint`, `gezik-config/src/{theme, state_store}.rs`.
- *Bütçe:* exe ≈ +140 KB; boşta 0.
- *Testler:* 64 KB'tan görüntüleyiciye geçişte konum kaybolmaz; bellekte/sayfalı eşiği (2 MB ± 1 bayt); sayfalı pencere satır başına hizalanır, UTF-8 karakter ortasında kesmez, UTF-16 çift bayta hizalanır, ANSI; sona atlama son satırı tam gösterir; sarma (geniş karakter 2 sütun, sekme, 4.096 kesme); çözücü: her dilde yorum/dize/sayı/anahtar sözcük, çok satırlı blok yorum ve dize durumu satırlar arası, Rust ham dizesi, Python üçlü tırnak, işaretleme etiketi, TOML/YAML anahtarı; 64 parça sınırı; renk karşıtlığı ≥ 4,5:1 iki temada; hex satır biçimi (16 ve 8 bayt), dosya sonu kısmi satır; görüntüleyici `drop` edilince dosya tutamacı kalmaz (Windows'ta dosya silinebilir); okuma sayaçları (önizleme kapalıyken 0).
- *macOS/Linux:* Retina'da eş aralıklı karakter genişliği; Linux'ta yazı tipi geri düşüşüyle CJK satırı.
- *Denetim listesi:* macOS 366–368: (366) Gezik'in Quick Look penceresinde bir `.rs` dosyası renkli, satır numaralı, Page Down/End çalışır; (367) 3 GB'lık bir günlük dosyasında End son satırı gösterir, bellek (Etkinlik İzleyicisi) ≤ +5 MB; (368) bir `.dylib`'de `H` hex gösterir. Linux 266–267: (266) aynı renklendirme ve sona atlama; (267) Japonca metinde sarma hizası.

**13b — PDF ve yakınlaştırma**
- *Kapsam:* §3.3, §5.
- *Dosyalar:* `gezik-platform/src/process.rs`, `gezik-batch/src/pdf/{view, pdfium}.rs`, `gezik/src/main.rs`, `gezik/src/preview/{zoom, pdf, viewer}.rs`, `gezik/src/{archives, dialog}.rs`, `ui/widgets/preview-pane.slint`.
- *Bütçe:* exe ≈ +100 KB; boşta 0.
- *Testler:* `spawn_binary` büyük ikili çıktıyı eksiksiz okur (8 MB, sahte program); protokol ayrıştırma ve sınırlar (başlık > 1 KB, görüntü > 4096² → hata, işçi öldürülür); parola stdin'den, komut satırında yok; yeni istek eskisini geçersiz kılar; işçi çökmesinde ileti ve yeni işçi; kırpmalı çizimin boyutu ≤ görünen alan; yakınlaştırma adımları ve imleç sabit noktası (saf); 10 sn sonra işçinin çıkması (sahte saatle); pdfium yokken sayfa 1 küçük resim ve teklif satırı; resim: 48 MP sınırı, sığdır'a dönünce büyük görüntünün bırakılması.
- *macOS/Linux:* indirilen pdfium'la sayfa çizimi; Retina'da fiziksel piksel boyutu.
- *Denetim listesi:* macOS 369–372: (369) 50 sayfalık PDF'te `›` ve Page Down sayfa değiştirir; (370) %400'de sürükleme ve keskin metin; (371) parolalı PDF parola sorar; (372) bir PDF'ten başka dosyaya geçince 10 sn içinde `gezik --pdf-view` süreci yok (`ps`). Linux 268–270: aynı üç davranış ve süreç denetimi.

**13c — video ve ses**
- *Kapsam:* §6; `preview-play`.
- *Dosyalar:* `gezik/src/preview/{media, viewer}.rs`, `gezik-platform/src/audio/*`, `gezik-platform/Cargo.toml` (`Win32_Media_Audio`), `gezik-batch/src/convert/ffmpeg.rs` (`probe`), `gezik/src/{archives, actions, palette}.rs`, `ui/widgets/preview-pane.slint`.
- *Bütçe:* exe ≈ +130 KB; boşta 0.
- *Testler:* ffmpeg komut satırları (boyut sınırı 1280×720, kare hızı ≤ 30, `-ss`, ses parametreleri; saf); saat: ses saati ve duvar saati, geç karenin atılması, erken karenin beklemesi (sahte saatle); sarma iki süreci yeniden başlatır; 60 sn duraklamada süreçlerin kapanması ve devam'da konum; dosya sonu; ses düzeyi ölçeklemesi ve kesme (i16 taşması yok); `drop`'ta süreçler ve aygıt kapanır (sahte `AudioOut`); ffmpeg yokken teklif ve indirme sonrası başlama; ffmpeg hatasında poster ve ileti; Windows WASAPI'nin gerçek aygıtla açılıp kapanması (`#[ignore]`, CI'da aygıt yok).
- *macOS/Linux:* AudioQueue ve Pulse `dlopen`; Pulse yokken sessiz oynatma iletisi.
- *Denetim listesi:* macOS 373–378: (373) MP4 oynat/duraklat, görüntü ve ses eşzamanlı; (374) sarma çubuğu ve `J`/`L`; (375) ses düzeyi ve `M`; (376) MKV/WebM oynar; (377) MP3'te kapak posteri ve oynatma; (378) başka dosyaya geçince 1 sn içinde `ffmpeg` süreci yok. Linux 271–275: (271) MP4 ve ses (PipeWire); (272) sarma; (273) MKV; (274) ses sunucusu durdurulmuş oturumda (`systemctl --user stop pipewire-pulse`) video sessiz oynar ve iletiyi gösterir; (275) süreç denetimi.

**13d — alan kaydı ve panel**
- *Kapsam:* §7.1–§7.4.
- *Dosyalar:* `gezik-core/src/fields.rs`, `gezik-batch/src/fields/*` (`exif.rs` taşınır), `gezik-batch/src/rename.rs` (yeni `taken` yolu), `gezik/src/preview/fields.rs`, `gezik/src/info.rs`, `ui/widgets/{preview-pane, info}.slint`, `gezik-config/src/settings.rs` (`[preview] properties`), `templates/settings.toml`.
- *Bütçe:* exe ≈ +200 KB (ilk görevde ölçülür, §12); boşta 0.
- *Testler:* kayıt: anahtarlar benzersiz, ≤ 64 alan, `by_key`, `text` ve `compare` her türde, `parse_value` (süre, tarih, gerçek sayı, birimler); okuyucular: gerçek küçük örnek dosyalar (`tests/fixtures/fields/`, her biri ≤ 20 KB: JPEG EXIF, HEIC `ispe`, MP3 ID3v2.3/2.4/v1 Türkçe etiketler, FLAC, Opus, M4A, MP4 `moov` sonda, MKV, AVI, DOCX, ODT, PDF `/Info` UTF-16), her okuyucuda kesilmiş ve rastgele 10.000 giriş panik etmez, bellek sınırları (dev uzunluk alanı ayırma yapmaz), yalnız-bulut dosya okunmaz; `readers_for` istenmeyen okuyucuyu çalıştırmaz (sayaç); panel satırları ve gruplar; `properties = false` okumaz.
- *macOS/Linux:* APFS ve ext4'te aynı değerler; macOS'ta HEIC fotoğraf.
- *Denetim listesi:* macOS 379–381: (379) iPhone HEIC fotoğrafında kamera, tarih, konum panelde; (380) bir MP3'te sanatçı/albüm/süre; (381) bir DOCX'te başlık/yazar/sayfa. Linux 276–277: (276) aynı fotoğraf ve MP3 değerleri; (277) Info penceresinde Properties.

**13e — sütunlar, sıralama, gruplama, süzgeç**
- *Kapsam:* §7.5, §8; `more-columns`.
- *Dosyalar:* `gezik-core/src/{view, sort, group, field_filter}.rs`, `gezik/src/{meta, filter, actions, context_menu, menu_bar, palette}.rs`, `gezik/src/view/{model, listing, columns}.rs`, `gezik-config/src/{views_file, views_writer, view_rules}.rs`, `ui/widgets/{picker, file-view, column-view}.slint`.
- *Bütçe:* exe ≈ +80 KB; boşta 0.
- *Testler:* `ColumnKey::Field` gidiş-dönüşü (`views.toml`, `[[view-rules]]`), bilinmeyen ad uyarısı, 8 sütun sınırı; okuyucu yalnız görünen satırların yalnız istenen kümesini okur, küme büyüyünce yalnız eksik okuyucu çalışır (sayaç); sıralama (değersizler sonda, eşitlikte ad, okuma bitince bir kez); gruplama (yalnız `groupable`, `taken` yıla göre, `No Artist` sonda); süzgeç sözcükleri (her işleç, tırnak, `any`/`none`, birlikte ad deseni, hatalı değer iletisi, bilinmeyen anahtar ad deseni kalır, 12'nin `tag:`/`rating:`'i aynı davranır); okunmamış satırların soluk gösterilmesi; görünüm kuralının alan sütunlarıyla klasör açması G/Ç'siz seçilir; 10.000 öğede süre (`preview.ps1`).
- *macOS/Linux:* harf duyarlı adlarda süzgeç harf duyarsız.
- *Denetim listesi:* macOS 382–384: (382) Müzik klasöründe Artist/Album/Duration sütunları ve sıralama; (383) Fotoğraflarda `taken>=2024 camera:iphone` süzgeci; (384) `[[view-rules]]` ile müzik klasörü sütunlarla açılır. Linux 278–279: (278) sütunlar ve sıralama; (279) süzgeç.

**13f — arama, şablon, vurgulama**
- *Kapsam:* §9.
- *Dosyalar:* `gezik-core/src/{search, batch/template, highlight}.rs`, `gezik-search/{Cargo.toml, src/query.rs, src/run.rs}`, `gezik/src/{search, saved_searches, batch_rename}.rs`, `gezik-config/src/{settings, highlight_rules}.rs`, `ui/widgets/rename-batch.slint`, `templates/settings.toml`.
- *Bütçe:* exe ≈ +70 KB; boşta 0.
- *Testler:* arama: ölçüt sırası, `readers_for` boşsa aday okunmadan elenir, ölçütsüz aramada okuma yok (sayaç), `[[searches]] where` gidiş-dönüşü, Everything adaylarının süzülmesi; şablon: her türün argümanı, `|` yedeği, bilinmeyen alan hatası, **ad güvenliği** (`/`, `\`, `..`, `:`, kontrol karakterleri, 100 sınırı, `"../../x"` aynı klasörde kalır), `needed` yalnız gereken okuyucular, `{tags}`/`{rating}`; vurgulama: `where` koşulu, okunmamış satırda bekleme, `where`'siz kurallarda okuyucu kurulmaz, satır başına ≤ 1 µs.
- *macOS/Linux:* Linux'ta adda `:` olabilir ama şablon yine `_` yazar (üç sistemde aynı ad).
- *Denetim listesi:* macOS 385–387: (385) `artist:x duration>3m` aramasında sonuçlar; (386) MP3'leri `{track:02} {artist} - {title}` ile adlandırma; (387) `where = "duration>1h"` vurgulaması. Linux 280–281: (280) arama ve şablon; (281) vurgulama.

## 14. Test

### 14.1 Birim (saf; her sistemde)

`syntax` (diller, durumlar, sınırlar), `text::display_width`, sayfalı pencere hizalaması, hex biçimi, yakınlaştırma matematiği, PDF görüntü protokolü ayrıştırması, oynatıcı saati ve ffmpeg komut satırları, ses ölçekleme, `fields` (kayıt, değerler, biçimler, karşılaştırma, ayrıştırma), okuyucular (örnek dosyalar + rastgele girişler), `field_filter`, `sort`/`group` yeni anahtarları, `template` alanları ve ad güvenliği, `search` ve `[[searches]] where`, `highlight` `where` koşulu, ayar ayrıştırmaları.

### 14.2 Platform

- Windows: `spawn_binary` ve iş nesnesi (Gezik ölünce çocuklar ölür), WASAPI açma/kapama ve aygıt değişimi, görüntüleyici kapanınca dosya tutamacının bırakılması (silme testi).
- macOS: AudioQueue, Retina fiziksel piksel boyutu, indirilen ffmpeg/pdfium ile uçtan uca.
- Linux: Pulse `dlopen` (var/yok), süreç grubu öldürme, X11 ve Wayland'da oynatma.

### 14.3 Çapraz denetim

Planın sonunda (her görevde değil): `cargo check` macOS ve Linux hedefleri, Windows `clippy`/`fmt`/`test`. Docker yok.

### 14.4 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken ya da ikinci ekranda film izlerken birinci ekranda; klavye düzeni ayarlarına dokunulmaz): 150 ve 200 % ölçekte metin, PDF ve oynatıcı çubuğu; kulaklık takıp çıkarmada sesin sürmesi; OneDrive yalnız-bulut dosyasında görüntüleyicinin kurulmaması; 4K videoda CPU; 10.000 fotoğraflık klasörde Date Taken sütunu ve sıralama.
- **macOS** (`macos-test.md`'ye eklenir, sonuçlar `macos-test-results.md`'ye): §13.2'nin 366–387 maddeleri.
- **Linux** (gerçek makine, GNOME ve KDE, X11 ve Wayland): §13.2'nin 266–281 maddeleri.

## 15. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| Okuyucu paniği `panic = "abort"` ile Gezik'i kapatır (bozuk/kötü niyetli dosya, yalnız görüntülemek bile) | Çökme, veri kaybı yok ama güven kaybı | `#![deny(clippy::indexing_slicing)]`, sınırlı okuma ve ayırma, okuyucu başına 10.000 rastgele giriş testi; PDF ve video zaten ayrı süreçte |
| Ad şablonunda kötü niyetli meta değeri (`../`) | Dosya başka klasöre gider | Değer temizleme (§9.2), testle |
| ffmpeg yok, indirme büyük (21–54 MB) | Oynatma ilk kez beklenir | Poster her zaman var; indirme yalnız kullanıcı isteyince; bugünkü teklif kutusu ve `[tools] download = false` yolu |
| Yazılım çizicisinde her kareyi yeniden çizmek CPU yer | Fan, pil | Kare ≤ 1280×720, ≤ 30 kare/sn, yalnız önizleme `Image`'ı; pencere gizliyken kare yok; `preview.ps1`'de ölçüt |
| Ses/görüntü kayması (iki ayrı ffmpeg süreci) | Dudak eşzamansız | Ses saati tek doğru; kareler ona göre atılır/bekler; sarmada ikisi aynı `-ss` ile yeniden başlar |
| Ses aygıtı API'leri (aygıt değişimi, Bluetooth gecikmesi, Linux'ta Pulse yok) | Ses kesilir ya da yok | WASAPI geçersizleşmede yeniden açma; Pulse yoksa sessiz oynatma ve ileti; ALSA yolu bilerek yok (karar 16) |
| pdfium işçisinin bellek kullanımı (büyük taranmış PDF) | Bellek | Ayrı süreç; görüntü ≤ görünen alan; PDF'ten çıkınca 10 sn'de kapanır |
| Eş aralıklı yazıda geniş karakterlerin genişliği yazı tipine göre 2 sütundan sapar | Renk parçaları kayar | Parça konumları satırın kendi ölçümüyle değil sütunla; sapma yalnız CJK/emoji satırlarında, rengi değil hizayı bozar; Linux denetim maddesi 267 |
| Renklendirme yanlış durumda kalır (kapanmayan blok yorum) | Dosyanın geri kalanı yorum renginde | Editörlerle aynı davranış; yalnız ≤ 2 MB dosyada; renksize geçiş yok (doğru davranış) |
| Alan sütunu ağ klasöründe yavaş | Bekleme | Yalnız görünen satırlar, arka plan, ilerleme; tam okuma yalnız alanla sıralamada/süzmede |
| Exe bütçesi (13d okuyucuları) | Parça sınırı aşılır | İlk görevde ölçüm; `document`/`pdf` okuyucuları 13e'ye kayabilir (§12) |
| 12 bitmeden 13e/13f başlarsa | Okuyucu ve süzgeç yolu yok | Sıra tablosu (§13); 13a–13d 12'den bağımsız |
| Paralel parçaların ortak listelerde çakışması (11'in dersi) | Birleştirme hataları | Sıralı ya da yığılı (§13.1) |

## 16. Bilinen sınırlar

- 2 MB'tan büyük metin renklenmez ve satır numarası göstermez; kaydırma bayt oranıyla.
- PDF tek sayfa görünümünde; metin seçilemez, aranamaz. Sayfa 2 ve yakınlaştırma pdfium ister (indirme).
- Video oynatma ffmpeg ister (indirme); altyazı yok, ilk ses izi; Linux'ta ses PulseAudio/PipeWire ister.
- Resim yakınlaştırması 48 MP'e kadar gerçek boyutta.
- WMA/ASF etiketleri, RAW dosyaların üretici notları, Office'in eski ikili biçimleri (doc, xls, ppt) okunmaz.
- Nesne akışlı PDF'lerde başlık/yazar/sayfa sayısı sütunda boş kalabilir (panelde pdfium varsa sayfa sayısı gelir).
- Alan değerleri dosyaya yazılmaz; eşitleme karşılaştırması alan kullanmaz.

## 17. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 13a, 13b, 13c, 13d, 13e, 13f; sıralı ya da yığılı, paralel değil** (§13). *Gerekçe:* metin görüntüleyicisi dış araç gerektirmeden önizlemenin yeni yapısını kurar ve en sık dosyalara gider; PDF ikili boruyu getirir, oynatıcı onu kullanır; tüketiciler kayıttan ve 12'nin okuyucusundan sonra. 11'de aynı listelere paralel dokunan parçalar birleştirmede çakıştı; 13a–13c aynı Slint dosyasına ve modüle dokunuyor.
2. **Önizleme `preview/` klasörüne bölünür, bugünkü işçi değişmez; yeni şeyler tek bir `Viewer`'da** (§3.1). *Gerekçe:* 818 satırlık dosyaya dört görüntüleyici eklemek okunmaz; ilk görüntü yolunu (son istek, eski sonucu atma) yeniden yazmak gereksiz risk.
3. **Görüntüleyicinin tek sahibi ve `Drop`'u süreçleri, tamponları, aygıtı bırakır** (§3.1). *Gerekçe:* "önizleme değişince bellek bırakılır" kuralı tek yerde tutulursa unutulamaz; üç ayrı temizlik yolu (dosya değişti, bölme kapandı, Quick Look kapandı) hata kaynağı olurdu.
4. **Renklendirme: Gezik'in içinde tablo güdümlü küçük çözücü; syntect ve tree-sitter yok** (§4.3). *Gerekçe:* syntect sözdizimi dökümleri ve regex motoruyla MB'larca, tree-sitter dil başına 100 KB–2 MB tablo ve C derlemesi; parça başına 262 KB bütçede yalnız tablo yolu ≈ 25 dili sığdırır. Önizleme için anahtar sözcük, dize, yorum, sayı doğruluğu yeter; anlam çözümlemesi gerekmez.
5. **Yalnız ≤ 2 MB metin renklenir** (§4.3). *Gerekçe:* doğru renk satır başı durumunu ister, bu da dosyanın başından okumak demek; 2 MB bunun ≈ 10 ms'si. Büyük dosyalar çoğunlukla günlük ve veri, rengi az değerli; yanlış renk renksizden kötü.
6. **Büyük metin bayt konumuyla kayar; satır dizini kurulmaz; bellek eşlemesi yok** (§4.1). *Gerekçe:* 10 GB'ta satır dizini dakikalar ve yüz MB'larca bellek; bayt konumu anında ve sabit bellekte. Bellek eşlemesi ağ ve çıkarılabilir diskte dosya kısalınca `SIGBUS`/`EXCEPTION_IN_PAGE_ERROR` ile çöker; ofsetli okuma aynı hızı güvenle verir.
7. **Hex hiçbir zaman kendiliğinden açılmaz** (§4.4). *Gerekçe:* çoğu kullanıcı için bir exe'nin baytları gürültü; bugünkü simge/küçük resim daha yararlı; hex isteyen tek tuşla alır.
8. **Sarma eş aralıklı yazıda karakter sayısıyla; metinde açık, kodda kapalı** (§4.2). *Gerekçe:* bugünkü önizleme sarıyor (geri gidiş olmasın); sabit satır yüksekliği yalnız görünen satırları kurmayı ve büyük dosyada kaydırmayı mümkün kılar; kodda sarma girintiyi bozar.
9. **PDF tek sayfa, önceden çizim yok, yakınlaştırmada yalnız görünen bölge** (§5.2). *Gerekçe:* kullanıcı kararı "yalnız görünen sayfa, gereken boyutta"; kırpmalı çizim %800'de de bit eşlemi görünen alanla sınırlar. Sonraki sayfayı önceden çizmek ≈ 150 ms kazandırır ama her sayfada bir görüntü daha bellekte tutar.
10. **pdfium ayrı uzun yaşayan süreçte (`--pdf-view`), PDF dışına çıkınca 10 sn sonra kapanır** (§5.1, §3.6). *Gerekçe:* bugünkü ilke (pdfium çökmesi ve belleği Gezik'i etkilemez) korunur; art arda PDF gezen kullanıcı her dosyada ≈ 100 ms süreç kurma ödemez; 10 sn boşta bellek tutmanın kısa ve sınırlı bedeli. Zamanlayıcı yalnız işçi varken kurulur.
11. **Resim yakınlaştırması aynı yüzeyle, 48 MP'e kadar gerçek boyut** (§5.3). *Gerekçe:* yol haritası satır 13 resim yakınlaştırmayı da sayıyor ve yüzey PDF'le ortak (ek ≈ 15 KB); `image` sandığı kırpmalı çözmüyor, 48 MP (≈ 192 MB) bugünkü fotoğraf makinelerini kapsayan ve geçici bir tavan; daha büyüğü 1024 px'lik görüntüyle kalır.
12. **Video için ffmpeg, üç sistemde; işletim sisteminin oynatıcısı (Media Foundation, AVFoundation, GStreamer) yok** (§6.2). *Gerekçe:* kullanıcı kararı 5c/5d'nin ffmpeg'i; tek kod yolu, üç sistemde aynı biçimler (MKV, WebM dahil); üç ayrı yerel hattın her biri kendi kare aktarma kodunu ister ve Linux'ta GStreamer çoğu kurulumda eksik eklentili. Bedeli ilk oynatmada indirme.
13. **Önizleme eylemlerinin varsayılan tuşu yok; tuşlar yalnız görüntüleyicinin içinde** (§3.2, §10.2). *Gerekçe:* liste odaktayken tuşlar (Space, oklar, harfle atlama) zaten dolu; çakışma denetimi gerektiren yeni genel tuşlar yerine Quick Look penceresinin ve odaklı görüntüleyicinin kendi klavyesi (YouTube'un K/J/L/M'si bilinir). Palet ve menüde eylemler var; kullanıcı isterse tuş atar.
14. **Poster = bugünkü küçük resim (video), gömülü kapak (ses); posteri ffmpeg çıkarmaz** (§6.1). *Gerekçe:* kullanıcı kararı "oynat'a basılmadan hiçbir şey çözülmez"; sistem küçük resmi zaten var ve önbellekli; kapak bir resim, ses değil.
15. **Kareler görünen alanın boyutunda, en çok 1280×720 ve 30 kare/sn, ffmpeg ölçekler** (§6.2). *Gerekçe:* Slint yazılım çizicisi her kareyi CPU'da çizer; önizleme alanı bundan büyük nadiren; ölçeklemeyi ffmpeg yaparsa Gezik'e giden bayt ve kopya en aza iner. Tam ekran izleme için sistemin oynatıcısı (Enter ile açılır) var.
16. **Linux'ta ses yalnız PulseAudio basit API'si, çalışırken `dlopen`; ALSA yolu yok** (§6.3). *Gerekçe:* başlangıçta yeni bağımlılık olmaz (taşınabilir Linux derlemesi); PipeWire kurulumları `pipewire-pulse`'ı sunar; yalnız ALSA'lı masaüstü nadir, orada video sessiz oynar ve bunu söyler. İkinci bir ses yolu kod ve test yükü.
17. **Ses düzeyi yazılımda, tek değer, `state.toml`'da** (§6.2). *Gerekçe:* sistem düzeyine dokunmamak (kullanıcının genel sesi değişmesin); dosya başına düzey gereksiz karmaşa.
18. **Alanlar Gezik'in kendi okuyucularından; Windows özellik işleyicileri ve Spotlight yok** (§1, §7.3). *Gerekçe:* üç sistemde aynı alanlar ve aynı değerler (bir arama Windows'ta bulup Linux'ta bulmamamalı); özellik işleyicileri yavaş, takılabilir, bulut dosyasını indirebilir (12'nin karar 19'u); Spotlight dizinlenmemiş birimlerde boş. Okuyucular zaten bağlı sandıklarla (kamadak-exif, zip, image, encoding_rs) ve küçük kendi kodla.
19. **Okuyucular biçim başına; bir dosyada istenen alanlarla kesişen okuyucular bir kez çalışır** (§7.1). *Gerekçe:* bir MP3'ün sanatçısını, albümünü ve süresini üç ayrı okumayla almak üç kat G/Ç olurdu; WDX'in "algıla + alanlar" modeli kanıtlanmış. Kesişme yoksa (Duration sütunu açık, satır `.txt`) hiçbir şey okunmaz.
20. **Bugünkü sütunlar ve şablon alanları kayda taşınmaz** (§7.2). *Gerekçe:* ad, boyut, tarih G/Ç'siz ve çalışıyor; taşımak yalnız değişiklik ve risk getirir. Kayıt dosya içi ve 12'nin alanları içindir.
21. **WMA/ASF, eski Office ikili biçimleri ve RAW üretici notları okunmaz** (§7.3). *Gerekçe:* her biri ayrı bir kapsayıcı ayrıştırıcısı (OLE bileşik belgesi, ASF nesneleri) ve bütçenin anlamlı bir kısmı; kullanımları azalıyor. Kayıt ileride okuyucu eklemeye açık.
22. **Okuyucularda dizinleme yasak (`get()`), sınırlı okuma ve ayırma, rastgele giriş testleri; şablona giren değerler temizlenir** (§7.3, §9.2). *Gerekçe:* dosya içeriği güvenilmez ve yayın derlemesi panikte kapanır; okuyucular yalnız klasörü açınca da çalışır (sütun), kullanıcı dosyayı açmadan. Ad şablonu, meta değerini yola çeviren tek yer; `../` temizlenmezse dosya klasör dışına gider.
23. **Konum alanında harita bağlantısı yok** (§7.4). *Gerekçe:* tıklamak fotoğrafın konumunu dış bir siteye gönderir; Gezik ağ isteği yalnız araç indirirken yapar. `Copy` yeter.
24. **Görünüm başına en çok 8 alan sütunu** (§8.1). *Gerekçe:* her alan sütunu satır başına okuma ister; 8, müzik (sanatçı, albüm, parça, süre, yıl, tür) ve fotoğraf (tarih, kamera, boyut, lens, ISO) görünümlerine yeter ve görünen satır okumasını ölçüt içinde tutar.
25. **Gruplama yalnız metin alanlarında ve yılda; sayısal alanlarda yok** (§8.2). *Gerekçe:* sayısal aralıkların (süre, çözünürlük) adlandırılması alan başına keyfi seçim; sıralama aynı işi tahmin edilebilir biçimde görüyor.
26. **Görünüm kuralları (10f) alanları yalnız sonuçta kullanır, koşulda değil** (§8.2). *Gerekçe:* 10f kuralı klasör listelenmeden ve G/Ç'siz seçer (10'un §13.1 ölçütü); alan koşulu dosyaları okumayı gerektirirdi. "Müzik klasörü" yol ve içerik payı koşuluyla zaten tanınıyor.
27. **Süzgeç, arama ve vurgulama kuralı aynı sözcük sözdizimini kullanır; `gezik-search` okuyuculara doğrudan bağımlı** (§8.3, §9). *Gerekçe:* tek ayrıştırıcı, tek öğrenme; kayıtlı aramada ve ayarda tek metin biçimi. `gezik-search → gezik-batch` döngüsüz ve bir satır; geri çağrı arayüzü aynı işi daha çok kodla yapardı.
28. **Eşitleme karşılaştırmasında alan yok** (§1). *Gerekçe:* kullanıcının dört başlığında yok; 11e içerik/boyut/tarihle doğru karşılaştırıyor; meta alanıyla eşitleme (ör. "etiketi değişmiş") ayrı bir tasarım sorusu, kayıt API'si (`read`, `compare`) onu ileride taşıyabilir.
29. **Panel varsayılan açık (`[preview] properties = true`)** (§7.4). *Gerekçe:* önizleme zaten açıkken tek dosyanın ≤ 30 ms'lik sınırlı okuması; Finder/Explorer kullanıcısı bilgileri burada bekler. Önizleme kapalıyken hiçbir şey okunmaz.
30. **Menü kimlikleri 2600–2799; `Action` 124 → 127; denetim listesi macOS 366–387, Linux 266–281** (§10, §13.2). *Gerekçe:* 12 2400–2599'da biter; aralıklar önceden ayrılınca sıralı parçalar da çakışmaz.
31. **Bütçe: her parça ≤ +262.144 B, boşta 0, açılış ≤ +1 ms, klasör açma ±%2** (§12). *Gerekçe:* önceki adımların kuralı; önizleme ve alanlar isteğe bağlı olduğu için kullanmayan için hiçbir maliyet kabul edilmez.
