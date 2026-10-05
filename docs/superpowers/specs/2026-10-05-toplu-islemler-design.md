# Alt Proje 5: Toplu İşlemler — Tasarım

- **Tarih:** 2026-10-05
- **Durum:** Tasarım onaylandı (2026-10-05); 5a, 5b uygulandı
- **Kapsam:** Gezik yol haritasının 5. alt projesi; tek spec, dört plan ve dört PR: **5a** toplu yeniden adlandırma, **5b** arşivler, **5c** dönüştürme ve kullanıcı komutları, **5d** PDF
- **Dayandığı:** `2026-10-04-dosya-islemleri-design.md` (motor, `Task`, çakışma listesi, geri alma, ilerleme paneli, `pending` toparlaması, sürükle-bırak), `2026-10-03-ayarlar-ve-tema-design.md` (ayar dosyası, canlı yeniden yükleme, kısayol biçimi)

## 1. Amaç

Explorer'da olmayan ya da zayıf kalan toplu işleri Gezik'in kendi motoruyla, her sistemde aynı ve geri alınabilir biçimde yapmak: çok dosyayı kurallarla yeniden adlandırmak, arşivleri açmak ve oluşturmak, resim/metin/ses/videoyu dönüştürmek, kullanıcının kendi komutlarını dosyalar üzerinde çalıştırmak, PDF'leri birleştirip bölmek. Hiçbir şey bilmeyen bir kullanıcı programı indirdiğinde bunların hepsini yapabilmelidir; gereken dış araçlar (ffmpeg, pdfium, 7-Zip) tek tıkla indirilir.

### Başarı ölçütleri

- 1.000 dosyalık toplu yeniden adlandırmada önizleme her tuş vuruşundan sonra ≤ 50 ms'de güncellenir; uygulama tek Ctrl+Z ile tamamen geri alınır (döngülü adlar a↔b dahil).
- zip, 7z, rar, tar.gz/xz/bz2/zst, cab ve iso tek tıkla açılır; "Extract here" tek kök klasörlü arşivi doğrudan, karışık içerikliyi `ad\` klasörüne açar. Kötü niyetli yollar (`..\..\`, mutlak yol) arşivin hedefi dışına hiçbir şey yazamaz.
- zip oluşturma, Windows'un "Sıkıştırılmış klasöre gönder"inden ≥ 2 kat hızlıdır (1 GB'lık karışık klasör).
- 100 fotoğrafı 1920 px jpeg'e dönüştürme tüm çekirdekleri kullanır.
- ffmpeg hiç kurulu değilken bir video, katmandaki tek "Download" tıklamasıyla dönüştürülebilir.
- Boşta bellek değişmez (≤ 7 MB); exe büyümesi 5a ≤ +0,5 MB (5a ölçülen +1,32 MiB: Slint katmanı + motor ~0,7 MB, regex ~0,45 MB, kalan diğer bağımlılıklar; Unicode regex Türkçe adlar için tutuldu), 5b ≤ +3 MB, 5c ≤ +1 MB, 5d ≤ +1 MB. Her PR'da ölçülür.
- Her iş 4a'nın kuyruğunda çalışır: panelde ilerleme, duraklat/iptal, çakışma listesi, tek Ctrl+Z.

### Kapsam dışı (bilerek)

- RAR oluşturma (unRAR lisansı izin vermiyor).
- Arşivden öğe silme: arşivin içini görmek gerektirir, **alt proje 9**'da arşivin içinde gezinmeyle gelir (yeniden yazma motoru 5b'de hazırlanır).
- Bölünmüş zip oluşturma (7z'de çok parçalı oluşturma var).
- Office (docx/xlsx/pptx) → PDF ve e-kitap dönüştürme: gerçekçi tek yollar LibreOffice (~350 MB) ve Calibre (~200 MB). Windows'ta Office COM yolu da alınmaz (tek sistemde, Office kuruluysa çalışır). Kullanıcı komutuyla yapılabilir; ayar şablonunda örnek bulunur (bkz. 7.6).
- Arşiv öğelerini, PDF sayfalarını veya resimleri dosya listesinde küçük resim olarak göstermek (Görünüm alt projesinin işi; HEIC küçük resmi ayrıca ele alınır).

## 2. Alınan kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Öncelik | Dört parça eşit önemde; sıra teknik bağımlılığa göre 5a → 5b → 5c → 5d | Kullanıcı tercihi; 5a dış kütüphane gerektirmez, araç indirme 5b'de kurulur |
| Arayüz | Pencere içi katmanlar (çakışma listesi gibi) | Ek pencere tamponu yok (~2 MB/pencere), odak ve çoklu monitör sorunu yok |
| Kod yeri | Saf mantık `gezik-core::batch`; görevler ve ağır kütüphaneler yeni `gezik-batch` crate'i | 4a'nın ayrımı korunur; `gezik-ops` motor olarak sade kalır |
| Yeniden adlandırma | Kurallar + canlı önizleme + önizlemede elle düzenleme | Kullanıcı tercihi |
| Arşiv kütüphaneleri | İçeride, saf Rust; tek istisna RAR açma için libunrar (C++) | "Kurmadan çalışır, üç sistemde aynı"; RAR'da WinRAR'ın kendi kodu en güvenilir yol |
| Nadir arşivler | lzh, arj, wim, dmg, msi… indirilen 7-Zip ile | Az kullanılır; gömmek boyut ister |
| Dönüştürme | Resim ve metin içeride; ses/video ffmpeg ile; kullanıcı komutları | Kullanıcı tercihi |
| ffmpeg | Pakete girmez; ilk kullanımda tek tıkla indirilir, Gezik'in kendi GitHub sürümlerinden, SHA-256 sabitli | Exe 13 MB kalır; adres değişmez; sahte dosya çalışmaz |
| HEIC/AVIF, kayıplı webp/AVIF | ffmpeg ile (aynı indirme) | Ayrı C kütüphanesi gerekmez |
| PDF | Resim→PDF içeride (`pdf-writer`); birleştirme, bölme, PDF→resim indirilen pdfium ile | pdfium Chrome'un motoru, BSD, ~5 MB; resim→PDF indirme gerektirmemeli |
| Office/e-kitap | Kapsam dışı; kullanıcı komutuyla | 200-350 MB araç; Office COM tek sistemde çalışır |
| Bölme | Tek spec, dört plan/PR | 4a/4b'deki gibi: tutarlı tasarım, yönetilebilir planlar |

## 3. Mimari

### 3.1 Katmanlar

| Katman | Yeri | Sorumluluk |
|---|---|---|
| Saf mantık | `gezik-core::batch` (yeni modül) | Yeniden adlandırma kuralları ve ad üretimi, ad şablonu ayrıştırma (`{n:03}`, `{date:%Y-%m-%d}`), döngü/zincir çözümü, çıktı yolu hesaplama, "tek kök mü" kararı, arşiv yolu güvenliği, komut şablonunun argüman dizisine açılması, araç bildirimi (sürüm, adres, özet) |
| Görevler | `gezik-batch` (yeni crate; `gezik-core`, `gezik-platform`, `gezik-ops`'a bağlı) | `Task` uygulamaları ve ağır kütüphaneler: arşiv biçimleri, resim kodlayıcıları, metin kodlama, alt süreç yönetimi, araç indirme, PDF |
| Motor | `gezik-ops` | Küçük genişletmeler (3.2); yeni bağımlılık yok |
| Arayüz | `gezik` | Katmanlar (Rename, Compress, Extract şifresi, Convert, PDF), menü öğeleri, kısayollar, araç indirme kutusu |

Ağır kütüphaneler yalnız ilgili iş çalışırken kullanılır; boşta bellek değişmez. `gezik-batch` arayüze bağlı değildir.

### 3.2 Motordaki değişiklikler

- `TaskKind`'a `BatchRename`, `Compress`, `Extract`, `Convert`, `Command`, `Download`, `Pdf` eklenir; etiketler (`label`) Undo/Redo menüsü için genişletilir ("Undo Rename 24 items", "Undo Extract archive.zip").
- **Alt öğe ilerlemesi:** bir öğeden çok dosya çıkan işler (açma, oluşturma) için `RunCx`'e öğe sayacı eklenir; panel "Extracting 1,240 / 3,000 · 412 MB of 1.1 GB" gösterir. Bayt sayacı zaten var.
- **`Work::External`:** alt süreç çalıştıran işlerde iptal ve duraklatma süreci sonlandırır (Windows'ta iş nesnesiyle alt süreç ağacı, Unix'te süreç grubu). 4a'da tanımlı ama kullanılmamıştı; 5b'de (7-Zip) uygulanır ve sınanır.
- **Ara klasör kaydı:** `pending`'e "aşama klasörü" kaydı eklenir (`.gezik-x-<pid>-<iş>`); çökmede açılışta silinir. 4a'nın geçici kopya kaydı genelleştirilir.
- **Bağlı iş:** bir işin bitince başka bir işi aynı kullanıcı eylemi olarak başlatması (açma: aşama klasörüne aç → 4a'nın Move'u ile yerine taşı). Geçmişte tek kayıt, panelde tek satır, tek Ctrl+Z.

### 3.3 Geri alma kuralı

- Yeni dosya üreten her iş `Outcome::Created` döndürür; Ctrl+Z üretilenleri çöpe atar (4a'nın `inverse` modülü değişmeden).
- Toplu yeniden adlandırma `Outcome::Moved` döndürür; Ctrl+Z eski adlara döndürür (döngü çözümü geri almada da işler).
- Yerinde değiştiren işler ("aslının yerine" dönüştürme, var olan arşive ekleme, yerinde çalışan kullanıcı komutu) önce aslını çöpe atar (`Outcome::Trashed`), sonra yenisini yazar (`Outcome::Created`); Ctrl+Z yeniyi çöpe atar, aslını geri getirir. Çöpü olmayan diskte bu seçenekler kapalıdır ve ipucu nedenini söyler.
- Araç indirme geçmişe girmez (Ctrl+Z ile geri alınmaz; araç `<veri>/tools`'tan elle silinebilir, bkz. 8).

### 3.4 Ortak akış

1. Seçim → menü öğesi veya kısayol → katman (seçenekler ve önizleme).
2. Start → iş kuyruğa girer, katman kapanır, ilerleme panelde görünür.
3. Var olan hedefler için 4a'nın çakışma listesi açılır (başlık işe göre: "3 conflicts · extracting archive.zip to D:\İndirilenler").
4. Uygun olmayan öğeler (resim hazır ayarında bir pdf) işe girmez; katmanın altında "3 items skipped: not images" yazar.
5. Her katmanın son seçimleri `state.toml`'da saklanır (`[batch]`): son kurallar, son arşiv biçimi ve düzeyi, son dönüştürme hazır ayarı ve çıktı seçimi. Bunlar tercih değil son kullanılan değerlerdir; ayar dosyasına girmez (adlandırılmış kural setleri hariç, bkz. 4.6).

### 3.5 Bölme

| PR | İçerik |
|---|---|
| **5a** | Motor değişiklikleri (3.2; `Work::External` 5b'de), `gezik-batch` iskeleti, toplu yeniden adlandırma (bölüm 4) |
| **5b** | Arşivler (bölüm 5) ve araç indirme mekanizması (bölüm 8; ilk aracı 7-Zip) |
| **5c** | Dönüştürme ve kullanıcı komutları (bölüm 6, 7); ffmpeg indirme |
| **5d** | PDF (bölüm 9); pdfium indirme |

Her biri tek başına kullanılabilir.

## 4. 5a — Toplu yeniden adlandırma

### 4.1 Açılış

- 2 veya daha fazla öğe seçiliyken `rename` kısayolu (Windows/Linux F2, macOS Enter) toplu yeniden adlandırma katmanını açar; tek öğede 4a'nın yerinde yeniden adlandırması aynen kalır.
- Sağ tık menüsünde "Rename 24 items…" (Windows'ta Gezik öğeleri arasında; Shell'in `rename` komutu çoklu seçimde de bu katmana yönlendirilir).
- Yeni kısayol eylemi `batch-rename` (varsayılanı yok; tek öğede de katmanı açmak isteyenler için).

### 4.2 Katman

```
┌ Rename 24 items ─────────────────────────── Presets ▾ ────────────────────┐
│ Rules                                │ Preview          ☐ Only changed     │
│ 1 ☑ Replace  "IMG_" → "Tatil "  [.*] │   Old name          New name         │
│ 2 ☑ Number   {n:03} at end, from 1   │ ≡ IMG_0012.jpg      Tatil 001.jpg    │
│ 3 ☐ Case     lower                   │ ≡ IMG_0013.jpg      Tatil 002.jpg    │
│ [+ Add rule ▾]  [↑][↓][✕]            │ ✎ IMG_0014.jpg      Kapak.jpg        │
│                                      │ ⚠ IMG_0015.jpg      Tatil 002.jpg    │
│ ☐ Include extension                  │                                      │
├──────────────────────────────────────┴───────────────────────────────────┤
│ 1 duplicate name · 22 will change                     [Cancel] [Rename]  │
└───────────────────────────────────────────────────────────────────────────┘
```

- Sol: kural listesi; seçili kuralın seçenekleri listenin altında açılır. Kurallar işaret kutusuyla geçici kapatılabilir, oklarla sıralanır.
- Sağ: önizleme. Başlangıç sırası dosya listesinin o anki sıralamasıdır.
- Klavye: Tab alanlar arasında; Ctrl+Enter Rename; Esc Cancel.

### 4.3 Kurallar

Kurallar sırayla uygulanır, her biri bir öncekinin çıktısını alır. Varsayılan olarak yalnız ad gövdesine (uzantısız); "Include extension" ile tüm ada. Klasörlerde uzantı yoktur (`a.b` klasörünün adı bütündür).

| Kural | Seçenekler |
|---|---|
| **Replace** | Bul / değiştir; düz veya regex (`regex` crate'i; `$1`, `${ad}` grupları); büyük/küçük harf duyarlılığı; ilk eşleşme veya hepsi |
| **Number** | Başlangıç, adım, basamak sayısı (sıfır doldurma); konum: başa / sona / ayırıcıyla; klasör başına yeniden başlama seçeneği |
| **Case** | küçük / BÜYÜK / Başlık Biçimi / Cümle biçimi. Dil kuralları sistem dilinden: Türkçede i↔İ, ı↔I |
| **Add text** | Başa ve/veya sona metin; şablon alanları kullanılabilir |
| **Extension** | Yeni uzantı; küçük/büyük harfe çevir; uzantıyı kaldır |
| **Template** | Adın tamamını şablondan kurar |
| **Trim / clean** | Baştaki/sondaki boşlukları sil; çoklu boşlukları teke indir; `_`, `.`, `-` → boşluk (ve tersi) |

**Şablon alanları:** `{name}` (o anki gövde), `{ext}`, `{n}` / `{n:03}`, `{parent}` (klasör adı), `{date:%Y-%m-%d}` (değişme tarihi; strftime biçimi), `{taken:%Y-%m-%d_%H%M}` (EXIF çekim tarihi, yoksa değişme tarihi), `{size}` (insan okunur). Bilinmeyen alan kural satırında hata olarak gösterilir.

- EXIF yalnız jpeg/tiff/heic/webp başlığından okunur (`kamadak-exif`, saf Rust); tüm dosya okunmaz. Okuma arka planda yapılır; okununcaya kadar önizlemede "…" görünür, Rename beklemeye geçer.

### 4.4 Önizleme

- Rust'ta tutulan tembel model (çakışma listesi gibi); 10.000 satırda akıcı.
- Her değişiklikten 50 ms sonra yeniden hesaplanır; hesap arayüz iş parçacığında yapılır ama ölçülür (1.000 öğede ≤ 50 ms hedefi; aşılırsa arka plana alınır).
- **Elle düzenleme:** "New name" hücresine çift tıklama veya satır seçiliyken F2. Elle yazılan ad ✎ ile işaretlenir; kurallar onu değiştirmez. Sağ tık: "Reset to rules".
- **Sürükleyerek sıralama:** satırın solundaki ≡ tutamağıyla; 4b'nin sürükleme kodu liste içinde kullanılır. Numaralandırma bu sırayı izler. "Sort by name / date / size" düğmeleri elle sırayı sıfırlar.
- **Uyarılar:**
  - ⚠ aynı yeni ad iki kez çıkıyor (büyük/küçük harf duyarsız sistemlerde harf farkı da aynı sayılır);
  - ⚠ klasörde seçimde olmayan bir öğe bu adı taşıyor;
  - ⛔ geçersiz ad (4a'nın denetimi: Windows'ta `\ / : * ? " < > |`, ayrılmış adlar, sonda nokta/boşluk; Unix'te `/` ve NUL; boş ad).
- ⛔ veya ⚠ varken Rename kapalıdır; düğmenin ipucu sayıyı ve "fix the marked rows" yazar. Çakışma listesi bu işte açılmaz (hedeflerin hepsi önizlemede görünür).
- "Only changed" yalnız değişecek satırları gösterir.

### 4.5 Uygulama (`BatchRename` görevi)

- `plan` her (eski, yeni) çiftini bildirir. Döngüler ve zincirler (a→b, b→c, c→a) `gezik-core::batch::cycles` ile çözülür: bağımlılık grafiğinde döngüde olan her öğe önce geçici ada (`.gezik-rn-<iş>-<n>`), sonra yeni adına taşınır; zincirler uygun sırayla taşınır, geçici ad gerekmez.
- Büyük/küçük harf duyarsız sistemlerde yalnız harf değişikliği (`foto.JPG` → `foto.jpg`) geçici adla yapılır.
- Geçici adlar `pending`'e geri koyma kaydı olarak yazılır (4a'nın `add_restore` yolu); iptal veya çökmede eski adlarına döner.
- Uygulama anında hedef beklenmedik biçimde dolmuşsa (başka program oluşturdu) o öğe hata satırına düşer; diğerleri sürer.
- `Resources`: dokunulan klasörlerin diski, `Work::Disk`; tek işçi (yeniden adlandırma sıralı ve hızlı).
- 4a'nın tek dosyalık `Rename`'i bu göreve taşınır (tek çiftli toplu yeniden adlandırma); davranışı değişmez.

### 4.6 Adlandırılmış kural setleri

- "Presets ▾" → "Save current rules as…", kayıtlı setler, "Manage…" (yeniden adlandır / sil).
- `settings.toml`'a yazılır (taşınabilir; 7. alt projeyle cihazlar arasında taşınır):

```toml
[[rename-presets]]
name = "Tatil fotoğrafları"
include-extension = false
rules = [
  { kind = "template", text = "{taken:%Y-%m-%d} {n:03}" },
  { kind = "case", mode = "lower" },
]
```

- Ayar yazımı 1. alt projedeki biçim korumalı yazma ile yapılır (yorumlar korunur). Geçersiz set uyarı verir ve listede gösterilmez.

## 5. 5b — Arşivler

### 5.1 Biçimler

| Biçim | Açma | Oluşturma | Kütüphane |
|---|---|---|---|
| zip | ✔ Stored, Deflate, Deflate64, bzip2, LZMA, zstd; ZipCrypto ve AES şifre; çok parçalı (`.z01`…) okunabildiği ölçüde | ✔ Deflate (çok çekirdekli, öğe başına paralel); AES-256 şifre | `zip`, `miniz_oxide` |
| 7z | ✔ LZMA/LZMA2, BCJ/BCJ2, Deflate, AES şifre; `.7z.001` parçaları | ✔ LZMA2; şifre, "Encrypt file names"; çok parçalı | `sevenz-rust2` |
| tar, tar.gz/tgz, tar.xz, tar.bz2, tar.zst | ✔ | tar, tar.gz, tar.xz | `tar`, `flate2` (miniz_oxide), `lzma-rust2`, `bzip2-rs`, `ruzstd` (yalnız açma) |
| Tek dosya .gz/.xz/.bz2/.zst | ✔ | .gz, .xz (tek dosya seçiliyse) | aynı |
| rar (RAR4, RAR5) | ✔ şifreli ve çok parçalı dahil | — | `unrar` (RARLAB'ın libunrar'ı, C++; unRAR lisansı) |
| cab | ✔ MSZIP, LZX | — | `cab` |
| iso | ✔ ISO 9660, Joliet, Rock Ridge, UDF | — | `isomage` veya `hadris-iso` (planda seçilir) |
| cpio, ar, deb | ✔ (deb: ar + içteki tar) | — | `cpio`, `ar` |
| lzh, arj, wim, dmg, msi, rpm, diğerleri | indirilen 7-Zip ile (bölüm 8) | — | dış süreç `7zz`/`7z.exe` |

- zstd oluşturma yok: saf Rust kodlayıcı zayıf, C kütüphanesi exe'yi büyütür.
- Biçim uzantıya değil dosyanın ilk baytlarına göre tanınır (uzantı yalnız ipucu); tanınmayan dosyada menü öğeleri görünmez.
- libunrar C++ derleyicisi gerektirir (Windows MSVC, macOS clang, Linux kapta g++); "saf Rust" kuralının tek bilinçli istisnasıdır. unRAR lisansı açmaya izin verir, RAR oluşturucu yazmayı yasaklar; Gezik'in lisansıyla çatışmaz. Lisans metni `THIRD-PARTY` notlarına eklenir.
- Boyut hedefi 5b için ≤ +3 MB; planın ilk görevi kütüphaneleri ekleyip sürüm derlemesinde ölçer. Aşılırsa özellik bayrakları daraltılır (önce Deflate64, bzip2-in-zip gibi nadir zip yöntemleri 7-Zip yoluna bırakılır).

### 5.2 Açma

- **Menü** (arşiv seçiliyken; birden çok arşiv seçiliyse her biri ayrı açılır, tek iş):
  - **Extract here** (akıllı),
  - **Extract to "ad\"**,
  - **Extract to…** (klasör seçici; son kullanılan hatırlanır).
- **Extract here kuralı:** arşivin kökünde tek klasör ya da tek dosya varsa doğrudan bulunulan klasöre; birden çok öğe varsa `ad\` klasörüne (ad = arşiv adı, `.tar.gz` gibi çift uzantılar tümüyle atılır).
- **Çift tıklama** arşivi sistemin varsayılan uygulamasıyla açmaya devam eder (alt proje 9'a kadar); `settings.toml` `[archives] double-click = "system" | "extract-here"` (varsayılan `system`).
- **Akış:** arşiv hedef klasörde gizli aşama klasörüne (`.gezik-x-<pid>-<iş>`, aynı disk) açılır; ardından bağlı iş olarak 4a'nın `Move`'u ile yerine taşınır. Çakışma listesi, klasör birleştirme ve geri alma aynen çalışır; baştan sona okunması gereken biçimler (tar.gz, solid 7z) iki kez okunmaz. Kullanıcı için tek iş, tek Ctrl+Z.
- Hedef diskte yer yetmezse (arşivin bildirdiği açık boyut + %5) iş başlamadan sorulur.
- **Güvenlik:**
  - Her öğe yolu `gezik-core::batch::archive_path` ile denetlenir: `..` bileşeni, mutlak yol, sürücü harfi, UNC, Windows'ta ADS (`:`) ve ayrılmış adlar reddedilir; reddedilen öğe hata satırına düşer.
  - Sembolik bağlantılar: Unix'te yalnız hedefi aşama klasörünün içinde kalıyorsa kurulur; Windows'ta atlanır (hata satırı: "symbolic link skipped").
  - Sıkıştırma bombası: bildirilen toplam açık boyut > 10 GB **ve** oran > 1000:1 ise önce sorulur; açarken gerçek boyut bildirileni %10 aşarsa öğe durdurulur.
  - Bozuk öğeler (CRC hatası, kesik veri) hata satırına düşer; o öğenin yarım dosyası silinir, kalanlar açılır.
- **Şifre:** şifreli öğe görülünce pencere içi soru katmanı şifre ister ("Show" düğmesiyle). Yanlış şifre yeniden sorulur; Cancel o arşivi atlar. Şifre yalnız bellekte, iş bitene kadar tutulur; hiçbir dosyaya yazılmaz.
- Değişme tarihleri korunur; Unix izinleri yalnız Unix'te uygulanır; zip'teki DOS öznitelikleri (salt okunur, gizli) Windows'ta uygulanır.
- `Resources`: arşivin ve hedefin diski, `Work::Disk` (tek işçi; tek akışlı okuma). Paralel çözme yalnız zip'te öğe başına (SSD'de).

### 5.3 Oluşturma

- **Menü:** **Compress…** (katman) ve **Compress to "ad.zip"** (katmansız, son biçim ve düzeyle).
- **Katman:**
  - Ad (varsayılan: tek öğede onun adı, birden çok öğede bulunulan klasörün adı) ve hedef klasör (varsayılan bulunulan klasör).
  - Biçim: zip / 7z / tar.gz / tar.xz / tar (tek dosyada .gz / .xz da).
  - Düzey: Store / Fast / Normal / Best.
  - Şifre (zip, 7z) ve 7z'de "Encrypt file names".
  - Parçalara böl (yalnız 7z): 100 MB, 700 MB, 4 GB (FAT32), özel. Parçalar `ad.7z.001`, `.002`… diye yazılır (7-Zip'in standardı: çıktının bayt olarak bölünmesi).
  - Ayar değiştikçe tahmini sıkıştırılmış boyut gösterilmez (tahmin güvenilmez); toplam girdi boyutu gösterilir.
- Arşiv önce gizli geçici adla yazılır, bitince asıl adına çevrilir; yarım arşiv hiçbir zaman tamamlanmış görünmez, iptalde silinir, çökmede açılışta silinir (`pending` kaydı). `Outcome::Created`; Ctrl+Z çöpe atar.
- Yollar seçimin ortak üst klasörüne göre görelidir. Sembolik bağlantılar zip/7z'de izlenmez (bağlantının kendisi atlanır, hata satırı), tar'da bağlantı olarak saklanır.
- Gezik'in gizli geçici dosyaları (`.gezik-*`) arşive girmez.
- İlerleme bayt bazlı: "Compressing 1,240 / 3,000 · 412 MB of 1.1 GB".
- `Resources`: girdi ve çıktı diskleri, `Work::Cpu` (zip'te çekirdek sayısı kadar paralel Deflate; 7z LZMA2'de kütüphanenin iş parçacığı sayısı).

### 5.4 Var olan arşive ekleme

- Compress katmanında "Add to existing archive…" (zip, 7z, tar ailesi); ayrıca dosyaları zip/7z/tar dosyasının üstüne sürükleyip bırakmak (4b'nin bırakma hedefleri arşiv dosyalarını da hedef sayar; etiket "Add to archive.zip").
- Arşiv geçici adla yeniden yazılır: zip'te var olan öğeler sıkıştırılmadan ham kopyalanır (hızlı), 7z ve tar ailesinde açılıp yeniden paketlenir. Eskisi çöpe gider (`Trashed` + `Created`); Ctrl+Z eskisini geri getirir. Çöpü olmayan diskte bu seçenek kapalıdır.
- Arşivde aynı yolda öğe varsa çakışma listesi açılır (Replace / Skip / Keep both).
- Şifreli arşive eklemek için şifre sorulur; yeni öğeler aynı şifreyle şifrelenir.
- Yeniden yazma motoru (`gezik-batch::archive::rewrite`) arşivden silmeyi de destekleyecek biçimde kurulur; silme arayüzü alt proje 9'da gelir.

## 6. 5c — Dönüştürme

### 6.1 Convert katmanı

```
┌ Convert 12 items ────────────────────────────────────────────────────────┐
│ Preset  [ Resize photos (JPEG)        ▾ ]                                │
│ ─ Image ───────────────────────────────────────────────────────────────  │
│ Format  ◉ JPEG  ○ PNG  ○ WebP  ○ AVIF¹  ○ BMP      Quality [ 85 ]       │
│ Resize  [Longest side ▾] [ 1920 ] px   ☑ Never enlarge                  │
│ ☑ Rotate by EXIF   ☐ Remove metadata (incl. location)                   │
│ ─ Output ──────────────────────────────────────────────────────────────  │
│ ◉ Same folder, new name   ○ Subfolder "converted"   ○ Choose…           │
│ ○ Replace originals (originals go to the Recycle Bin)                    │
├──────────────────────────────────────────────────────────────────────────┤
│ 3 items skipped: not images     ¹ needs ffmpeg      [Cancel] [Convert]  │
└──────────────────────────────────────────────────────────────────────────┘
```

- Hazır ayar listesi seçili dosyaların türlerine göre süzülür ve gruplanır: Image, Text, Audio/Video, Commands. Karışık seçimde en çok dosyanın türü önce gelir.
- **Çıktı:**
  - Aynı klasör, yeni ad: uzantı değişiyorsa `foto.png` → `foto.webp`; aynı uzantıda `foto (converted).jpg`.
  - Alt klasör `converted\` (bulunulan klasörde; yoksa oluşturulur).
  - Seçilen klasör.
  - Aslının yerine: yeni dosya asıl adla yazılır, aslı çöpe gider (3.3). Uzantı değişiyorsa asıl silinir, yeni ad yeni uzantıyla oluşur. Çöpü olmayan diskte kapalı.
- Var olan çıktılar için çakışma listesi. Çıktılar gizli geçici adla yazılır, bitince asıl adına çevrilir.
- Dönüştürme hata satırı dosya adı ve nedeni gösterir ("photo.png: unsupported color type").

### 6.2 Resim (içeride)

- **Girdi:** png, jpeg, gif (ilk kare), webp, bmp, tiff, ico. HEIC, HEIF, AVIF girdisi ffmpeg ile (6.5).
- **Çıktı:** JPEG (kalite 1-100, varsayılan 85), PNG, WebP **kayıpsız**, BMP içeride; **kayıplı WebP** (kalite) ve **AVIF** ffmpeg ile.
- **Seçenekler:**
  - Boyutlandırma: en uzun kenar / genişlik / yükseklik / yüzde; "Never enlarge" (varsayılan açık); `fast_image_resize` (SIMD, saf Rust; Lanczos3).
  - EXIF'e göre döndürme (varsayılan açık; döndürülen çıktıda yön etiketi sıfırlanır).
  - Meta veriyi kaldırma (EXIF, konum, XMP); kapalıyken JPEG→JPEG'de EXIF korunur (yön düzeltilmiş), diğer biçimlere taşınmaz.
  - Saydamlıktan JPEG'e geçerken arka plan rengi (varsayılan beyaz).
- `image` crate'ine özellik eklenir (`tiff`, `ico` çözme; `jpeg`, `png`, `webp`, `bmp` kodlama). Renk profili (ICC) JPEG→JPEG'de korunur, diğerlerinde sRGB varsayılır.
- `Resources`: `Work::Cpu`, çekirdek sayısı kadar paralel; bellek sınırı için aynı anda en fazla `çekirdek` kadar resim açık.
- **Yerleşik hazır ayarlar:** "Resize photos (JPEG 1920 px)", "Convert to JPEG", "Convert to PNG", "Convert to WebP", "Convert to AVIF", "Remove location data".

### 6.3 Metin (içeride)

- **Kodlama:** algılama `chardetng` (BOM varsa önce BOM), dönüştürme `encoding_rs`: UTF-8, UTF-8 BOM'lu, UTF-16 LE/BE, Windows-1254/1252/1251/1250, ISO-8859-9/1/15, KOI8-R, Shift_JIS, GBK, Big5 (encoding_rs'in tüm WHATWG kodlamaları seçilebilir; listede sık olanlar önce).
- **Satır sonu:** LF / CRLF / CR / olduğu gibi.
- **Temizlik:** sondaki boşlukları sil; dosya sonunda tek satır sonu.
- Kaynak kodlama katmanda algılanan değerle gelir ve değiştirilebilir ("Detected: Windows-1254 (11 files), UTF-8 (1 file)").
- İkili dosya (ilk 8 KB'ta NUL baytı) atlanır. Kayıplı dönüşümde (hedef kodlamada olmayan karakter) dosya hata satırına düşer — "can't encode 'ş' in Windows-1252 (line 14)" — ve değiştirilmez.
- Metinde varsayılan çıktı "aslının yerine"dir. 64 MB'tan büyük metin dosyası akış hâlinde dönüştürülür.
- **Yerleşik hazır ayarlar:** "Convert to UTF-8", "Windows line endings (CRLF)", "Unix line endings (LF)".

### 6.4 Ses ve video (ffmpeg)

- `ffmpeg` sırasıyla `settings.toml` `[convert] ffmpeg` yolundan, Gezik'in indirdiği araçtan, PATH'ten bulunur. Hiçbiri yoksa ses/video hazır ayarları katmanda araç kutusuyla görünür (bölüm 8): "Video conversion needs ffmpeg (~35 MB, free). [Download]".
- **Yerleşik hazır ayarlar:**
  - MP4 (H.264 + AAC) — `-c:v libx264 -crf 20 -preset medium -c:a aac -b:a 160k -movflags +faststart`;
  - Smaller video — H.264 CRF 28, en fazla 1080p (oranı korunur), AAC 128k;
  - MP3 — 192 kb/s (videodan sesi çıkarma dahil);
  - M4A (AAC) — 192 kb/s;
  - WAV — PCM 16 bit;
  - Remux to MP4 — `-c copy` (yeniden kodlamadan kap değiştirme; uyumsuz akış varsa hata satırı);
  - GIF from video — 480 px, 12 fps, palet üretimiyle.
- **İlerleme:** `-progress pipe:1` satırlarından `out_time_us`; toplam süre `ffprobe -show_entries format=duration` ile (ffprobe yoksa yalnız dosya sayacı).
- **Süreç:** aynı anda tek ffmpeg (ffmpeg tüm çekirdekleri kullanır); `-nostdin -y` ile geçici çıktıya yazar. İptal ve duraklatma süreci sonlandırır (duraklatmada dosya baştan alınır), yarım çıktı silinir.
- Çıkış kodu sıfır değilse stderr'in son 20 satırı "Details"te görünür.
- ffmpeg hazır ayarları içeride kullanıcı komutlarıyla aynı yapıda (7.1) tanımlıdır; tek kod yolu.

### 6.5 ffmpeg ile resim biçimleri

- **HEIC/HEIF girdisi** (iPhone fotoğrafları, karo yapılı ızgara dahil), **AVIF girdisi**, **kayıplı WebP** ve **AVIF çıktısı** ffmpeg ile yapılır; bu seçenekler "¹ needs ffmpeg" işaretiyle görünür ve ffmpeg yoksa araç kutusunu açar.
- Akış: ffmpeg girdiyi kayıpsız PNG'ye (geçici) çözer → Gezik boyutlandırma/döndürme/meta veriyi uygular → çıktı içeride kodlanabiliyorsa içeride, değilse (kayıplı WebP, AVIF) ffmpeg ile kodlanır.
- **Doğrulama (plan görevi):** indirilecek ffmpeg sürümünün iPhone HEIC'lerini (ızgara + EXIF yön) doğru çözdüğü örnek dosyalarla sınanır. Yetmezse yedek yol: Windows'ta WIC (HEIF/HEVC uzantıları kuruluysa), macOS'ta ImageIO; Linux'ta "not supported" + kullanıcı komutu önerisi. Bu yedek yol yalnız doğrulama başarısız olursa yapılır ve spec'e işlenir (2026-10-05 kararı: başarısızsa yedek yol yazılır, HEIC kapsamdan çıkarılmaz).
- HEIC dosyaları toplu yeniden adlandırmada EXIF tarihi için yine içeride okunur (4.3; yalnız başlık).

## 7. 5c — Kullanıcı komutları

### 7.1 Tanım

```toml
[[commands]]
name = "Resize to 50% (ImageMagick)"
run = ["magick", "{in}", "-resize", "50%", "{out}"]
output = "{name}-small.{ext}"   # yoksa: komut dosyayı yerinde değiştirir
types = ["jpg", "jpeg", "png"]   # yoksa: her dosya
parallel = 4                     # varsayılan 1
```

- Kabuk yok: `run` bir argüman dizisidir; yer tutucular argümanların içinde değiştirilir. Dosya adındaki `"`, `;`, `$`, `&`, boşluk güvenle geçer. Kabuk isteyen açıkça yazar (`["sh", "-c", "..."]`, `["cmd", "/c", "..."]`).
- Yer tutucular: `{in}` (tam yol), `{out}` (tam çıktı yolu; aslında Gezik'in geçici adı), `{dir}`, `{name}`, `{ext}`, `{outdir}`.
- `output` yoksa komutun girdiyi yerinde değiştirdiği varsayılır: Gezik önce aslını çöpe **kopyalar** (`Trashed` kaydı bir kopya için; asıl yerinde kalır, komut onu değiştirir). Ctrl+Z değişmiş dosyayı çöpe atar, kopyayı geri getirir. Çöpü olmayan diskte bu komut çalışmaz ("needs a recycle bin to undo").
- `types` uzantı listesidir (büyük/küçük harf duyarsız); `folders = true` klasörlerde de çalıştırır.
- `parallel` 1-16; `Work::External`.
- Program `run[0]` PATH'te veya mutlak yolda aranır; bulunamazsa komut menüde gri ve ipucu "magick not found".

### 7.2 Çalıştırma

- Komutlar Convert katmanında "Commands" grubunda ve sağ tık menüsünde "Commands ▸" altında görünür (yalnız `types`'a uyan seçimde).
- Çalışma klasörü girdinin klasörü; ortam Gezik'inki; stdin kapalı; pencere açılmaz (Windows'ta `CREATE_NO_WINDOW`).
- Başarı: çıkış kodu 0 **ve** `output` tanımlıysa `{out}` oluşmuş olmalı. Değilse hata satırı; stderr'in son 20 satırı "Details"te.
- Zaman aşımı yok; iptal süreci sonlandırır, yarım çıktı silinir.
- `settings.toml` değişince komut listesi canlı yenilenir (1. alt projenin yeniden yüklemesi). Geçersiz komut uyarı verir ve listede görünmez.

### 7.3 Şablondaki örnekler

`settings.toml` şablonuna yorum satırı olarak eklenir:

```toml
# [[commands]]
# name = "Office to PDF (LibreOffice)"
# run = ["soffice", "--headless", "--convert-to", "pdf", "--outdir", "{outdir}", "{in}"]
# output = "{name}.pdf"
# types = ["docx", "doc", "xlsx", "pptx", "odt"]
#
# [[commands]]
# name = "E-book to EPUB (Calibre)"
# run = ["ebook-convert", "{in}", "{out}"]
# output = "{name}.epub"
# types = ["mobi", "azw3", "fb2"]
```

(LibreOffice `--outdir` ile kendi adını verdiği için `{out}` yerine `{outdir}` kullanılır; Gezik `output` adıyla oluşan dosyayı arar ve sonra geçici addan asıl ada taşır.)

## 8. Araç indirme (5b'de kurulur)

### 8.1 Akış

- Bir özellik dış araca ihtiyaç duyup aracı bulamadığında katmanda araç kutusu çıkar: "<Özellik> needs <araç> (~<boyut>, free). [Download]" ve küçük bir "Where does it come from?" bağlantısı (kaynak, lisans, sürüm).
- Download → `Download` görevi kuyruğa girer (panelde ilerleme, iptal); bitince kutu kaybolur, katmandaki ilgili seçenekler etkinleşir.
- Araçlar: **7-Zip** (5b; `7zz`/`7z.exe`, ~2 MB), **ffmpeg + ffprobe** (5c; ~35-80 MB, platforma göre), **pdfium** (5d; ~5 MB).

### 8.2 Kaynak ve doğrulama

- Dosyalar Gezik'in kendi GitHub deposunun sürümlerinde (`wenlar/gezik-tools`, sabit etiketler: `ffmpeg-7.1-1`, `7zip-24.09-1`, `pdfium-6996-1`) barındırılır. Platformlar: Windows x64/arm64, macOS arm64/x64, Linux x64/arm64. Depo herkese açıktır (yalnız araç ikilileri, lisans notları ve hazırlama betiği). Depoyu ve sürümleri `gh` ile Gezik geliştiricisi oluşturur (2026-10-05 kararı). ffmpeg **essentials** türü derlemedir (x264, x265, libwebp, SVT-AV1/aom, dav1d; GPL); üst kaynaklar: Windows gyan.dev, macOS ve Linux için planda seçilen sabit sürümlü statik derlemeler.
- Araç bildirimi `gezik-core::batch::tools`'ta derlemeye gömülüdür: araç, sürüm, platform başına adres, boyut, **SHA-256**, arşiv içindeki çalıştırılabilir yollar.
- İndirme sistemin kendi HTTP istemcisiyle yapılır (2026-10-06 kararı; Gezik'te Rust TLS yığını yok): Windows'ta WinHTTP, macOS'ta `NSURLSession` (ikisinde de sistemin proxy ayarları ve sertifikaları geçerli), Linux ve diğer Unix'lerde `curl`, yoksa `wget`; ikisi de yoksa hata "Install curl with your package manager (sudo apt install curl)". Yalnız HTTPS (yönlendirmeler dahil, https'ten http'ye inilmez), en çok 5 yönlendirme, gövde araç boyutu + 1 MiB'ı aşarsa durur; geçici dosyaya yazılır, ilerleme en çok 100 ms'de bir gelen bayt sayısından. Kod `gezik-platform::http`'te.
- İndirilen dosyanın SHA-256'sı Rust'ta (`sha2`) hesaplanır; tutmazsa silinir ve hata satırı "download damaged — try again". Tutarsa 5b'nin arşiv kodu ile `<veri>/tools/<araç>-<sürüm>/` altına açılır (aşama klasörü + yeniden adlandırma; yarım kurulum görünmez).
- Unix'te çalıştırılabilir izni verilir. macOS'ta `NSURLSession` indirdiği dosyaya karantina özniteliği koymaz; ikili dosyalar en az ad-hoc imzalı olarak barındırılır (arm64'te zorunlu).
- Yönetici izni gerekmez. Gezik eski sürüm klasörlerini yeni sürüm kurulduktan sonra siler.
- **Linux:** kutuda önce paket yöneticisi önerisi ("or install with your package manager: `sudo apt install ffmpeg`"), yanında Download.
- `settings.toml` `[tools] download = true | false` (varsayılan `true`; `false` kutuyu yalnız bilgi olarak gösterir, kurumsal kullanım için).

### 8.3 Lisans ve bakım

- ffmpeg GPL derlemesi ayrı bir program olarak dağıtılır; sürüm sayfasında lisans metni ve kaynak bağlantısı bulunur. 7-Zip LGPL + unRAR kısıtı, pdfium BSD/Apache; hepsi sürüm notlarında.
- Güvenlik güncellemesinde depo yeni etiketle güncellenir, Gezik'in yeni sürümü yeni özetleri taşır.
- `scripts/tools/` altında depo sürümünü hazırlayan betik (indir, özet çıkar, bildirimi güncelle) bulunur.

## 9. 5d — PDF

### 9.1 İşlemler

| İşlem | Nasıl | Araç |
|---|---|---|
| **Images to PDF** | Seçili resimler (önizleme sırasıyla, sürükleyerek değiştirilebilir) tek PDF'e; sayfa boyutu: resim boyutu / A4 / Letter; kenar boşluğu; JPEG'ler yeniden kodlanmadan gömülür | İçeride (`pdf-writer`, `miniz_oxide`) |
| **Merge PDFs** | Seçili PDF'ler sırayla tek PDF'e | pdfium |
| **Split PDF** | Her sayfa ayrı dosya / her N sayfa / aralıklar (`1-3, 5, 8-`) | pdfium |
| **PDF to images** | Sayfa başına PNG veya JPEG; DPI (72/150/300) | pdfium |
| **Extract pages** | Aralıklar yeni PDF'e | pdfium |

- Katman "PDF" grubu olarak Convert katmanındadır (hazır ayar listesinde); "Images to PDF" ayrıca sağ tık menüsünde.
- pdfium (`pdfium-render`, dinamik yükleme) indirilen kütüphaneyi iş başında yükler, iş bitince bırakır; boşta bellek değişmez.
- Şifreli PDF'te şifre sorulur (5.2'deki soru katmanı).
- Çıktılar `Created`; Ctrl+Z çöpe atar. Ad önerileri: `ad (merged).pdf`, `ad - page 3.pdf`, `ad - pages 1-3.pdf`.
- `Resources`: Images to PDF `Work::Cpu`; pdfium işleri `Work::Cpu`, tek iş parçacığı (pdfium iş parçacığı güvenli değil).

## 10. Ayarlar, durum, kısayollar

### 10.1 `settings.toml`

```toml
[archives]
double-click = "system"     # system | extract-here

[convert]
ffmpeg = ""                 # path to ffmpeg; empty = Gezik's download or PATH

[tools]
download = true             # offer to download ffmpeg, 7-Zip and pdfium when needed

# [[rename-presets]] and [[commands]]: see sections 4.6 and 7
```

Geçersiz değer uyarı verir ve varsayılan kullanılır (mevcut uyarı kalıbı).

### 10.2 `state.toml`

`[batch]`: son yeniden adlandırma kuralları, "Include extension"; son arşiv biçimi, düzeyi, parça boyutu; son dönüştürme hazır ayarı ve seçenekleri; son çıktı seçimi; "Extract to…" son klasörü. Şifreler hiçbir zaman yazılmaz.

### 10.3 Yeni kısayol eylemleri

| Eylem | Varsayılan |
|---|---|
| `batch-rename` | — |
| `compress` | — |
| `extract-here` | — |
| `convert` | — |

Varsayılanı olmayan eylemler 4a'da desteklendi. `rename` çoklu seçimde toplu yeniden adlandırmayı açar (4.1).

### 10.4 Tema

Yeni renk yok: uyarılar `progress-error` ve mevcut ikincil metin rengini, ✎ işareti vurgu rengini kullanır.

## 11. Performans

Ölçümler: Windows 11, sürüm derlemesi, NVMe; betik `scripts/perf/batch.ps1`.

| Ölçüm | Hedef |
|---|---|
| Boşta bellek | Değişmez (≤ 7 MB) |
| Toplu yeniden adlandırma önizlemesi, 1.000 öğe, 3 kural | ≤ 50 ms / değişiklik |
| Toplu yeniden adlandırma uygulama, 10.000 öğe | ≤ 2 sn |
| zip oluşturma, 1 GB karışık klasör (Normal) | Windows "Sıkıştırılmış klasöre gönder"den ≥ 2 kat hızlı |
| zip açma, aynı arşiv | Explorer'ın "Tümünü ayıkla"sından ≥ 2 kat hızlı |
| 100 fotoğraf (24 MP) → 1920 px JPEG | Tüm çekirdekler kullanılır; ölçülüp kaydedilir |
| Arayüz, iş sürerken | Kaydırma CPU'su ve kare süresi iş yokkenkiyle aynı (olaylar ≤ 10/s) |
| 5a: 10.000 öğe yeniden adlandırma | ölçülen 6,9 sn (hedef ≤ 2 sn tutmadı; sistemin kendi taşıması ~2,3 sn), bkz. notlar |
| exe büyümesi | 5a ≤ +0,5 MB (5a ölçülen +1,32 MiB: Slint katmanı + motor ~0,7 MB, regex ~0,45 MB, kalan diğer bağımlılıklar; Unicode regex Türkçe adlar için tutuldu), 5b ≤ +3 MB, 5c ≤ +1 MB, 5d ≤ +1 MB |

## 12. Test

### 12.1 Birim testleri (`gezik-core::batch`)

Her kural ve kural sırası; şablon ayrıştırma ve hatalı şablon; Türkçe büyük/küçük harf; uzantılı/uzantısız, klasör adları; döngü ve zincir çözümü (a↔b, a→b→c→a, harf değişikliği); geçersiz ad ve çift ad uyarıları; "tek kök mü" kararı; arşiv yolu güvenliği (zip-slip, mutlak yol, sürücü harfi, ADS, ayrılmış adlar); çıktı adı üretimi; komut şablonunun argümanlara açılması (özel karakterli adlar); araç bildirimi platform seçimi.

### 12.2 Bütünleşme testleri (`gezik-batch`, geçici klasörler)

- Her yazılabilir biçimde oluştur → aç → karşılaştır (içerik, tarih, Unix izinleri).
- Depoda küçük test arşivleri (`crates/gezik-batch/tests/data/`): rar4, rar5, şifreli rar/zip/7z, çok parçalı 7z ve rar, bozuk zip (CRC), zip-slip zip'i, sembolik bağlantılı tar, cab, iso, deb, sıkıştırma bombası taklidi (küçük ama yüksek oran bildiren). RAR örnekleri libarchive'in test arşivlerinden (BSD), HEIC örnekleri libheif örneklerinden indirilir; `tests/data/SOURCES.md` kaynak ve lisansı yazar. zip/7z/tar örnekleri testte üretilir ya da makinedeki 7-Zip ile bir kez üretilip eklenir.
- Açma: akıllı Extract here, çakışma listesi, birleştirme, geri alma; iptal ve çökme sonrası aşama klasörünün açılışta silinmesi.
- Var olan arşive ekleme ve geri alma.
- Toplu yeniden adlandırma: döngüler, iptal ortasında geri koyma, geri alma.
- Resim: boyutlandırma, EXIF yönü, meta veri silme, saydamlık → JPEG.
- Metin: kodlama ve satır sonu, kayıplı dönüşüm hatası, ikili dosya atlama.
- Süreç: sahte bir "ffmpeg"/komut test ikilisi (`tests/fake-tool`: ilerleme satırı basar, isteğe bağlı hata koduyla çıkar, bekler) ile ilerleme, iptal (süreç ağacı sonlanır), hata yolu, yarım çıktının silinmesi.
- İndirme: yerel dosya sunucusu (test içinde `127.0.0.1`) ile başarılı indirme, bozuk özet, kesilen indirme, iptal.
- PDF: Images to PDF içeride; pdfium testleri yalnız pdfium mevcutsa (`GEZIK_TEST_PDFIUM` yolu) çalışır, yoksa atlanır.

### 12.3 Platform

Windows'ta tüm testler ve elle ekran testleri. Linux Docker kabında derleme ve bütünleşme testleri (4b'deki kap; libunrar için g++ eklenir). macOS `cargo check --target aarch64-apple-darwin`, "denenmedi" işaretiyle.

### 12.4 Elle (ekran) testleri

- Toplu yeniden adlandırma: tüm kurallar, elle düzenleme, sürükleyerek sıralama, uyarılar, kural setleri, Ctrl+Z.
- Arşivler: menüdeki üç açma yolu, şifre katmanı, Compress katmanı, parçalı 7z, arşiv üstüne sürükleyip ekleme; 7-Zip indirip lzh açma.
- Dönüştürme: her yerleşik hazır ayar; ffmpeg yokken indirme kutusu ve indirmenin ardından video dönüştürme; iPhone HEIC → JPEG; kullanıcı komutu (başarılı, hatalı, yerinde).
- PDF: beş işlem; pdfium indirme.
- Her işte panel ilerlemesi, duraklat/iptal, çakışma listesi, tek Ctrl+Z.

## 13. Bölme

- **5a — Toplu yeniden adlandırma:** bölüm 3, 4, 10, 11-12'nin ilgili kısımları. Motor değişiklikleri ve `gezik-batch` iskeleti burada.
- **5b — Arşivler:** bölüm 5 ve 8.
- **5c — Dönüştürme ve kullanıcı komutları:** bölüm 6 ve 7.
- **5d — PDF:** bölüm 9.

## 14. Yol haritasındaki yeri

| # | Alt proje |
|---|---|
| 1 | Ayarlar + Tema sistemi — tamamlandı |
| 2 | Gezinme — tamamlandı |
| 3 | Görünüm — tamamlandı |
| 4 | Dosya işlemleri — tamamlandı (4a, 4b) |
| **5** | **Toplu işlemler (bu belge): 5a yeniden adlandırma, 5b arşivler, 5c dönüştürme ve komutlar, 5d PDF** |
| 6 | Etiketler |
| 7 | Taşınabilirlik |
| 8 | Bulut senkronu |
| 9 | Gelişmiş: arama, çift panel, arşivin içinde gezinme ve arşivden silme, Git, komut paleti |
