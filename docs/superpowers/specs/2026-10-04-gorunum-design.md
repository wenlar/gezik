# Alt Proje 3: Görünüm — Tasarım

- **Tarih:** 2026-10-04
- **Durum:** İncelemede
- **Kapsam:** Gezik yol haritasının 3. alt projesi
- **Dayandığı:** `2026-10-03-ayarlar-ve-tema-design.md` (ayar dosyası, tema, kısayol biçimi), `2026-10-04-gezinme-design.md` (sekmeler, geçmiş, `EntryModel`, Shell menüsü)

## 1. Amaç

Dosya listesini Explorer ve Files düzeyine getirmek: sütunlar ve sütuna göre sıralama, doğal sıralama, çoklu seçim, sistem ikonları, ızgara görünümü ve küçük resimler, önizleme.

### Başarı ölçütleri

- Liste Explorer'daki gibi sütunlu; başlığa tıklayınca sıralanır; `dosya2` `dosya10`'dan önce gelir; Türkçe harfler doğru sıradadır.
- Ctrl/Shift ile tık, Ctrl+A, Shift+ok ve fareyle çerçeve seçimi Explorer'daki gibi çalışır; sağ tık seçili tüm öğelerin Shell menüsünü açar.
- Windows'ta ikonlar ve "Tür" sütunu Explorer'dakiyle aynıdır; `icons = "gezik"` ile Gezik'in tema renkli ikonları kullanılır.
- Izgara görünümünde resim/video/PDF küçük resimleri görünür (`thumbnails = false` ile kapanır).
- Seçili öğe sağdaki önizleme panelinde (Alt+P) ve Boşluk ile açılan geçici pencerede önizlenir.
- Görünüm genel olarak ayarlanır; bir klasörde değiştirilen görünüm o klasör için hatırlanır.
- Performans (bkz. 9): açılış ≤ ~60 ms korunur; 100 bin dosyada liste ≤ ~18 MB; ızgara + 1000 fotoğraf ≤ ~50 MB; 100 bin dosyada sıralama ≤ ~50 ms.

### Kapsam dışı (bilerek)

- Sütunları sürükleyerek yer değiştirme, çift tıkla otomatik genişlik, klasör başına sütun genişlikleri, gruplama.
- Windows önizleme işleyicileri (canlı PDF/Office görüntüleme), sözdizimi renklendirme, video/ses oynatma.
- Satırdan başlayan sürükleme (sürükle-bırak Alt proje 4'te; şimdilik hiçbir şey yapmaz).
- Öğe onay kutuları.
- İkon/küçük resim üretiminin ayrı süreçte izolasyonu (arayüz buna hazır tasarlanır, bkz. 5.4).
- Klasör içeriğinin canlı izlenmesi.

## 2. Alınan kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Kapsam | Altı özellik tek alt projede | Kullanıcı tercihi |
| Mimari | Durum Rust'ta, Slint yalnızca görüneni çizer (Yaklaşım A) | 100 bin dosyada bellek; tembel `EntryModel` korunur |
| İkonlar | Sistem ikonları (varsayılan) veya Gezik seti; `icons` ayarı | Kullanıcı tercihi |
| Küçük resimler | Varsayılan açık; `thumbnails = false` | Kullanıcı tercihi |
| Önizleme | Sağ panel (Alt+P) + Boşluk ile geçici pencere | Kullanıcı tercihi |
| Görünümün saklanması | Genel varsayılan + klasör başına istisna (en çok 500, LRU) | Kullanıcı tercihi |
| Çoklu seçim | Explorer temel davranışı + çerçeve seçimi | Kullanıcı tercihi |
| Tür sütunu | Sistemin tür adı, uzantı başına önbellek | Kullanıcı tercihi |
| Doğal sıralama | Kendi uygulamamız, Türkçe harf sırası, her sistemde aynı | Taşınabilir ve öngörülebilir |
| Resim çözme | `image` crate'i, yalnızca png/jpeg/gif/webp/bmp | Önizleme ve Linux küçük resimleri için; ikili boyutu sınırlı |

## 3. Arayüz düzeni

```
┌ Gezik ───────────────────────────────────────────────────────── _ □ × ┐
│ [■ Belgeler ×] [■ gezik] [+]                                          │
│ ← → ↑ ⟳ │ This PC › C: › Users › teoma › Belgeler │        [View ▾]   │
├────────────┬──────────────────────────────────────┬──────────────────┤
│ FOLDERS    │ Name ▲          Modified   Type  Size │  ┌────────────┐  │
│  Home      │ ▣ Faturalar     2026-10-01 File f…    │  │  önizleme  │  │
│  Documents │ ▣ Projeler      2026-09-12 File f…    │  └────────────┘  │
│ PINNED     │ ▤ notlar2.txt   2026-10-03 Text… 1 KB │  notlar10.txt    │
│ DRIVES     │ ▤ notlar10.txt  2026-10-04 Text… 2 KB │  Text Document   │
│            │                                      │  2.1 KB · …       │
├────────────┴──────────────────────────────────────┴──────────────────┤
│ 4 items · 2 selected (3.1 KB)                                         │
└───────────────────────────────────────────────────────────────────────┘
```

Önizleme paneli listenin sağındadır; kenar çubuğu sağdaysa panel liste ile kenar çubuğu arasında kalır. Yeni bileşenler mevcut `ToolButton`/`TextField` kalıbını izler; renk ve ölçüler `Theme`'den gelir.

## 4. Çekirdek (`gezik-core`)

### 4.1 `Entry`

`created: Option<SystemTime>` eklenir. Uzantı saklanmaz, gerektiğinde addan hesaplanır (100 bin dosyada artış ≈ 1,6 MB).

### 4.2 `view` modülü: sıralama ve görünüm ayarları

- `SortKey { Name, Modified, Created, Type, Size }`, `SortDir { Asc, Desc }`. Klasörler her yönde önce gelir.
- `natural_cmp(a, b)`:
  - Ad metin ve sayı parçalarına bölünür; sayılar değer olarak karşılaştırılır (baştaki sıfırlarda eşitlikte kısa olan önce).
  - Harfler büyük/küçük harf duyarsız, Türkçe sırayla: `a b c ç d e f g ğ h ı i j k l m n o ö p r s ş t u ü v y z`; `I`→`ı`, `İ`→`i` eşlenir. Diğer aksanlı harfler temel harflerinin hemen ardından gelir; harf olmayanlar harflerden önce.
  - Tam eşitlikte ham karşılaştırma (bugünkü gibi) kararı verir.
- Sıralamada her ad için bir kez anahtar üretilir (`sort_by_cached_key` benzeri), sıralama bitince bırakılır.
- Tür sütununa göre sıralama: sistem tür adı, eşitse uzantı. Tür adları sonradan geldiği için, bu sütunla sıralıyken adlar gelince liste bir kez yeniden sıralanır; seçim ve kaydırma korunur.
- `ViewSettings { mode: List | Grid, sort: (SortKey, SortDir), grid_size: Small | Medium | Large }`.
- `ColumnState { key, visible, width }`; Ad sütunu gizlenemez ve genişliği kalan alandır.

### 4.3 `selection` modülü

- `Selection { bits: BitSet, anchor: Option<usize>, focus: Option<usize> }` (100 bin dosyada ~12 KB).
- İşlemler: `click`, `ctrl_click`, `shift_click`, `ctrl_shift_click`, `select_all`, `clear`, `move_focus(to, extend, keep)` (keep: Ctrl ile odak seçimi değiştirmeden taşınır), `toggle_focus` (Ctrl+Boşluk), `set_rect(range_set, additive)` (çerçeve seçimi; Ctrl basılıysa başlangıçtaki seçime eklenir/çıkarılır).
- Her işlem değişen indeks aralıklarını döndürür; model yalnızca onları `row_changed` ile bildirir (Ctrl+A gibi büyük değişikliklerde `reset`).
- Liste yeniden sıralanınca seçim adlar üzerinden yeniden eşlenir.
- `ViewState.selected: Option<String>` → `ViewState { selected: Vec<String>, focus: Option<String>, scroll }`. 1000'den fazla öğe seçiliyse yalnızca odak saklanır.

### 4.4 `layout` modülü (geometri, saf)

- Liste: satır yüksekliği ile indeks ↔ y.
- Izgara: hücre boyutu, sütun sayısı `N = max(1, floor(genişlik / hücre))`, indeks ↔ (satır, sütun).
- `items_in_rect(rect, layout) -> Vec<Range<usize>>`: çerçeve seçimi için her iki görünümde.
- Klavye: ızgarada ←/→ ±1, ↑/↓ ±N, PgUp/PgDn ±(görünen satır × N), Home/End.

### 4.5 `kind` modülü

Uzantıdan kategori: `folder, file, image, video, audio, archive, document, spreadsheet, presentation, pdf, code, text, executable, font, disk_image, link`. Gezik ikonları ve sistem ikonu gelene kadar yer tutucu için kullanılır (bugünkü renkli kareler kalkar).

### 4.6 `view_memory` modülü

- Klasör başına istisna: `path → (ViewSettings, last_used)`. Windows'ta yollar büyük/küçük harf duyarsız karşılaştırılır.
- En çok 500 kayıt; aşılınca en eski kullanılan silinir. "This PC" kaydedilmez.
- TOML'a ve TOML'dan dönüştürme burada; dosya G/Ç uygulamada.

## 5. Platform (`gezik-platform`) ve medya iş hattı

### 5.1 `icons` modülü

Engelleyici fonksiyonlar; iş parçacığından çağrılır.

| Fonksiyon | Windows | macOS | Linux |
|---|---|---|---|
| `type_name(ext, is_dir)` | `SHGetFileInfoW(SHGFI_TYPENAME \| SHGFI_USEFILEATTRIBUTES)` | UTType açıklaması | shared-mime-info yorumu |
| `icon(target, px)` | sistem ikon listesi (`SHGetImageList`, en yakın boyut) → RGBA | `NSWorkspace.icon(for:)` | ikon temasında MIME ikonu |
| `thumbnail(path, px)` | `IShellItemImageFactory::GetImage(SIIGBF_THUMBNAILONLY)` | `QLThumbnailGenerator` | freedesktop önbelleği; yoksa png/jpeg/gif/webp/bmp Gezik tarafından çözülür |
| `format_datetime(t)` | `GetDateFormatEx`/`GetTimeFormatEx` (kısa biçim) | `YYYY-MM-DD HH:MM` | `YYYY-MM-DD HH:MM` |

- `target`: çoğu dosya için **uzantı** (diske dokunulmaz); kendi ikonu olan türler (`exe, lnk, ico, url, cur, ani, scr, msc, appref-ms`) ve `desktop.ini` içeren klasörler için **yol**.
- Sonuç: `Rgba { width, height, pixels: Vec<u8> }` (önceden çarpılmamış alfa).
- macOS ve Linux kodu o sistemlerde denenene kadar "doğrulanmadı" olarak takip notlarına yazılır.

### 5.2 Medya iş hattı (`gezik/src/media.rs`)

- İki iş parçacığı, ilk ihtiyaçta başlar:
  - **icon:** tür adları ve ikonlar (Windows'ta COM STA).
  - **thumb:** küçük resimler ve önizleme içeriği.
- İstekler `row_data` bir satırı ürettiğinde önbellekte karşılık yoksa kuyruğa girer; aynı istekler birleştirilir. Kuyruk LIFO'dur; görünür aralığın dışına çıkan istekler atılır.
- Her listenin bir **kuşak numarası** vardır; başka klasöre geçince eski kuşağın istekleri atılır, gelen sonuçları yok sayılır.
- Sonuç `SharedPixelBuffer<Rgba8Pixel>` olarak `invoke_from_event_loop` ile UI iş parçacığına gelir, `Image`'a çevrilir; yalnızca o anahtarı kullanan görünür satırlar `row_changed` ile bildirilir.

### 5.3 Önbellekler

| Önbellek | Anahtar | Sınır |
|---|---|---|
| tür adı | uzantı | sınırsız (küçük) |
| uzantı ikonu | (uzantı, px) | sınırsız (küçük) |
| yol ikonu | (yol, px) | LRU, 512 kayıt |
| küçük resim | (yol, değiştirilme, px) | LRU, toplam 32 MB |

Boyutlar ölçek faktörüyle çarpılır: liste 16 px; ızgara Small/Medium/Large = 48/96/192 px.

### 5.4 Hatalar ve izolasyon

- Hatalar sessizdir: küçük resim yoksa ikon, sistem ikonu yoksa Gezik ikonu, tür adı yoksa `"<EXT> File"`.
- İş hattı `MediaSource` adlı bir arayüzün arkasındadır; ileride yardımcı sürece taşınabilir.

## 6. Liste ve ızgara görünümü (Slint + `gezik` uygulaması)

### 6.1 Model

- `FileRow`'a eklenenler: `modified`, `created`, `type-name`, `kind: int`, `icon: image`, `has-icon: bool`, `selected: bool`, `focused: bool`.
- Slint'teki `selected: int` kalkar; seçim, odak ve tıklama/klavye işlemleri Rust'taki `Selection`'a gider. `ensure-visible` ve klavye menüsü odak indeksini kullanır.
- Odaklı satırın etrafında, liste odaktayken `Theme.accent` renkli 1 px çerçeve çizilir (takip notundaki "listede odak göstergesi yok" maddesini kapatır).
- Izgara: `GridModel`, `EntryModel`'i N hücrelik satırlara böler; her satır görünür olduğunda hücre modelini üretir. N, liste genişliği değişince Slint'ten Rust'a bildirilir. Izgara da `ListView` üzerinde çalışır; yalnızca görünür satırlar oluşturulur.
- Izgara hücresi: kare görsel alanı (küçük resim veya büyük ikon, oranı korunarak ortalanır) ve altında en çok iki satır ad.

### 6.2 Sütun başlığı (yalnız liste görünümü)

- Sabit başlık satırı: Name, Modified, Type, Size; Created varsayılan olarak gizli.
- Ad sütunu kalan alanı alır (en az 150 px); sığmazsa başlık ve liste birlikte yatay kayar.
- Diğer sütunlar kenarından sürüklenir (en az 50 px). Başlığa tık sıralar, tekrar tık yönü çevirir; etkin sütunda ▲/▼.
- Başlıkta sağ tık (Slint menüsü): sütunları göster/gizle, "Reset columns".
- Hücreler: tarihler `format_datetime` ile; klasörde Boyut boş, Tür sistemin klasör adı.

### 6.3 Fare

- Tık: tek seçim. Ctrl+tık: ekle/çıkar. Shift+tık: bağlantı noktasından aralık. Ctrl+Shift+tık: aralığı ekle.
- Boş alanda tık seçimi temizler. Boş alandan sürükleme çerçeve seçimi başlatır (yarı saydam `Theme.selection` dolgu, `Theme.accent` kenar); Ctrl basılıysa mevcut seçime eklenir. Kenara 24 px yaklaşınca liste kendiliğinden kayar.
- Seçili olmayan bir öğeye sağ tık önce onu tek başına seçer; seçili bir öğeye sağ tık seçimi korur. Menü seçili tüm öğeler için açılır (Windows Shell menüsü çoklu öğe alır; macOS/Linux Slint menüsünde "Open" yalnızca tek öğede).
- Orta tık ve çift tık bugünkü gibi.
- `mod`+tekerlek: ızgara boyutunu değiştirir (listede ızgaraya geçmez).

### 6.4 Klavye

- ↑ ↓ PgUp PgDn Home End (ızgarada ← → de): odağı taşır, seçimi odağa indirir.
- Shift ile: bağlantı noktasından odağa aralık. Ctrl ile: odak taşınır, seçim değişmez. Ctrl+Boşluk: odaktaki öğeyi seç/bırak.
- Enter: odak öğesini açar (birden çok seçiliyse dosyalar varsayılan uygulamayla, klasörlerden ilki gezilir).
- Harfle atlama: odağı taşır, seçimi tek öğeye indirir.
- Esc: seçimi temizler (adres çubuğu yazımında değil).

### 6.5 Durum çubuğu

`N items` veya `N items · K selected (toplam boyut)`; boyut yalnızca dosyalar için toplanır.

## 7. Önizleme

### 7.1 Panel

- Alt+P ile açılır/kapanır; açıklık ve genişlik (200–600 px, sürüklenebilir ayırıcı) `state.toml`'da.
- Seçim değişince 100 ms gecikmeyle odak öğesi için içerik istenir (thumb iş parçacığı, en yüksek öncelik).
- İçerik:
  - **Resim** (png/jpeg/gif/webp/bmp): Gezik çözer; panel boyutu × ölçeğe küçültülür. Çözülmüş boyutu 8192×8192'yi aşan veya 100 MB'tan büyük dosyalar çözülmez, sistem küçük resmi kullanılır. GIF yalnızca ilk kare.
  - **Metin:** ilk 8 KB'ta NUL baytı yoksa metin sayılır; ilk 64 KB, UTF-8 (kayıplı), tek aralıklı yazıyla, kaydırılabilir.
  - **Diğer:** sistemin 256 px küçük resmi, yoksa büyük ikon.
  - **Klasör:** ikon ve öğe sayısı (en çok 10 000 sayılır, fazlası `10,000+`).
  - **Birden çok seçim:** `K items selected` ve toplam boyut.
- Altında bilgiler: ad, tür, boyut, değiştirilme, oluşturulma, resimse piksel boyutu.
- Panel kapanınca içerik bırakılır.

### 7.2 Hızlı bakış penceresi

- Liste odaktayken Boşluk açar; Boşluk veya Esc kapatır. Ayrı bir Slint penceresi, ana pencerenin ~%70'i, ortada.
- Panel ile aynı içerik bileşenini kullanır, daha büyük boyutta.
- Açıkken ↑ ↓ (ızgarada ← →) ana listede odağı taşır ve içeriği günceller.
- Kapanınca pencere ve içeriği bırakılır.
- Boşluk varsayılan olarak `quick-look` kısayoludur; Ctrl+Boşluk seçim için ayrılmıştır.

## 8. Ayarlar, durum dosyaları ve kısayollar

### 8.1 `settings.toml` → `[view]`

```toml
[view]
mode = "list"          # list | grid
sort = "name"          # name | modified | created | type | size
sort-dir = "asc"       # asc | desc
grid-size = "medium"   # small | medium | large
icons = "system"       # system | gezik
thumbnails = true
```

Geçersiz değerler bugünkü gibi uyarı verir ve varsayılana döner. Canlı uygulanır. "Apply to all folders" komutu mode/sort/sort-dir/grid-size değerlerini buraya, yorumları koruyarak yazar (`settings_edit` kalıbı).

### 8.2 Yerel dosyalar

- `state.toml`: sütunlar (`[[columns]] key, visible, width`), önizleme paneli açıklığı ve genişliği.
- `views.toml` (yeni, `state.toml`'un yanında): klasör başına istisnalar. Değişiklikten ~1 sn sonra ve kapanışta atomik yazılır; okunamazsa uyarı verilir ve boş başlanır.

### 8.3 View menüsü

Araç çubuğunun sağında "View" düğmesi (Slint menüsü): List · Grid · Small / Medium / Large · Sort by ▸ (sütunlar, Ascending/Descending) · Preview pane · Apply to all folders · Reset this folder.

Bir klasörde mod, sıralama veya ızgara boyutu değişince o klasör için istisna kaydedilir.

### 8.4 Yeni kısayollar

| Eylem | Varsayılan |
|---|---|
| `view-list` | `mod+shift+1` |
| `view-grid` | `mod+shift+2` |
| `toggle-preview` | `alt+p` |
| `quick-look` | `space` (yalnız liste odaktayken) |
| `select-all` | `mod+a` |

### 8.5 Tema

Yeni anahtarlar (varsayılanlarla, bütün yerleşik temalarda): `icon-folder`, `icon-image`, `icon-video`, `icon-audio`, `icon-archive`, `icon-document`, `icon-code`, `icon-other`, `focus-ring`, `marquee`. Eski `folder-icon`/`file-icon` anahtarları `icon-folder`/`icon-other` için eski ad olarak okunur.

### 8.6 Erişilebilirlik

Satırlar ve hücreler `list-item` rolünde, `accessible-selected` ile; sütun başlıkları düğme rolünde, sıralama durumu açıklamada; önizleme bilgileri metin olarak okunur.

## 9. Performans

| Ölçüt | Hedef |
|---|---|
| Açılış | ≤ ~60 ms (ikonlar ve tür adları listeden sonra gelir) |
| Boşta | ≤ 7 MB |
| 100 bin dosya, liste, sistem ikonları | ≤ ~18 MB |
| Izgara + 1000 fotoğraf, küçük resimler açık | ≤ ~50 MB (küçük resim önbelleği ≤ 32 MB) |
| 100 bin dosyada sıralama / sütun değişimi | ≤ ~50 ms |
| Kaydırma CPU'su | ~480 ms'yi belirgin aşmaz |
| Önizleme paneli kapanınca | bellek panel açılmadan önceki düzeye ~döner |

`scripts/perf/` betiklerine ızgara ve 1000 resimli klasör ölçümü eklenir; sıralama süresi `#[ignore]` işaretli bir sürüm testiyle ölçülür.

## 10. Test

### 10.1 Birim testleri

- `natural_cmp`: sayılar, baştaki sıfırlar, Türkçe harfler (`ç`, `ğ`, `ı/I`, `i/İ`, `ö`, `ş`, `ü`), büyük/küçük harf, harf olmayanlar.
- Her `SortKey` ve yönde sıralama; klasörlerin önce gelmesi.
- `Selection`: tüm işlemler, döndürülen aralıklar, yeniden sıralamada eşleme.
- `layout`: liste ve ızgara geometri, `items_in_rect`, ızgara klavye adımları.
- `kind`: uzantı → kategori.
- `view_memory`: LRU sınırı, Windows'ta büyük/küçük harf duyarsız yol, TOML gidiş-dönüş.
- Ayarlar: `[view]` okuma ve uyarılar; "Apply to all folders" yazımının yorumları koruması; `state.toml` sütunları.
- Windows platform testleri: `type_name("txt")` boş değil; `icon("txt", 16)` boş olmayan RGBA; üretilen bir PNG için `thumbnail` sonuç verir.

### 10.2 Elle (ekran) testleri

- Sütunlar: sıralama, yön, genişlik, gizleme/sıfırlama, yatay kaydırma, kalıcılık.
- Seçim: tüm fare ve klavye kombinasyonları, çerçeve seçimi ve otomatik kaydırma, çoklu Shell menüsü (7-Zip ile birden çok dosya), durum çubuğu.
- İkonlar: `.exe`, `.lnk`, özel klasör ikonu, `icons = "gezik"`'e canlı geçiş, tema renkleri.
- Izgara: üç boyut, `mod`+tekerlek, küçük resimler (jpg, mp4, pdf), `thumbnails = false`.
- Önizleme: paneldeki ve hızlı bakıştaki tüm türler, büyük resim, ikili dosya, klasör, çoklu seçim.
- Görünüm hafızası: istisna, Apply to all folders, Reset this folder, yeniden başlatma.

## 11. Yol haritasındaki yeri

| # | Alt proje |
|---|---|
| 1 | Ayarlar + Tema sistemi — tamamlandı |
| 2 | Gezinme — tamamlandı |
| **3** | **Görünüm (bu belge)** |
| 4 | Dosya işlemleri: kopyala/taşı/sil, çakışmalar, sürükle-bırak, menüye Gezik dosya işlemi öğeleri |
| 5 | Etiketler |
| 6 | Taşınabilirlik |
| 7 | Bulut senkronu |
| 8 | Gelişmiş: arama, çift panel, arşivler, Git, komut paleti |
