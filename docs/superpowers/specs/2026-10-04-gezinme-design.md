# Alt Proje 2: Gezinme — Tasarım

- **Tarih:** 2026-10-04
- **Durum:** İncelemede
- **Kapsam:** Gezik yol haritasının 2. alt projesi
- **Dayandığı:** `2026-10-03-ayarlar-ve-tema-design.md` (ayar dosyası, tema, işaretli yollar, kısayol biçimi)

## 1. Amaç

Gezik'i günlük kullanımda Files ve Explorer'ın yerini alabilecek bir gezinme deneyimine kavuşturmak: sekmeler, kenar çubuğu, geri/ileri (fare yan tuşları dahil), tıklanabilir adres çubuğu, klavye kısayolları ve işletim sisteminin yerel sağ tık menüsü.

### Başarı ölçütleri

- Kullanıcı Gezik'i sekmeler, kenar çubuğu ve klavyeyle, Explorer alışkanlıklarını değiştirmeden kullanabilir.
- Windows'ta sağ tık, Explorer'daki gibi sistemin menüsünü (7-Zip, Birlikte aç, Gönder…) açar; Gezik'in öğeleri bu menünün en üstündedir.
- Sabitlenen klasörler `settings.toml`'da taşınabilir biçimde saklanır; dosyayı başka işletim sistemine kopyalamak çalışır.
- Kısayollar `settings.toml`'dan değiştirilebilir ve canlı uygulanır.
- Performans: mevcut ölçütler korunur (boşta ≤ 7 MB, açılış ≤ ~60 ms, 100 bin dosyada ≤ ~15 MB ve kaydırma CPU'su ~480 ms'yi belirgin aşmaz); 20 sekme açıkken bellek tek sekmeye göre en fazla +2 MB.

### Kapsam dışı (bilerek)

- Sekmelerin başlık çubuğuna taşınması.
- Açılışta önceki sekmelerin geri yüklenmesi (kullanıcı tek sekme + ayarlanabilir başlangıç klasörü seçti).
- Çoklu seçim, sütunlar, ızgara görünümü (Alt proje 3).
- Gezik'in kendi dosya işlemleri: kopyala/yapıştır ilerleme penceresi, çakışmalar (Alt proje 4). Windows Shell menüsündeki sistem komutları (Sil, Yeni, Yapıştır…) zaten çalışır.
- Etiketler (ayrı alt proje, Alt proje 4'ten sonra). Kenar çubuğu bölümleri bu bölümün sonradan eklenebileceği şekilde tasarlanır.
- Klasör içeriğinin canlı izlenmesi (Shell komutlarından sonra klasör yenilenir).
- macOS "Hizmetler" alt menüsü; üçüncü parti menü eklentilerinin ayrı süreçte izolasyonu.
- Adres çubuğu parçalarında alt klasör açılır listesi.
- Arayüz metinlerinin çevirisi (bölüm başlıkları İngilizce; sistem klasörü adları işletim sisteminin dilinde).

## 2. Alınan kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Sekme yeri | Araç çubuğunun üstünde, sistem başlık çubuğu korunur | Snap, sürükleme, büyütme kendiliğinden çalışır; çerçeve çizmek yok |
| Açılış | Tek sekme, `start-folder` ayarındaki konumda | Kullanıcı tercihi |
| Kenar çubuğu | Sabit klasörler + Sabitlenenler + Sürücüler | Kullanıcı tercihi; sistem klasörleri yanlışlıkla kaldırılamaz |
| Sağ tık | İşletim sisteminin yerel menüsü; Gezik öğeleri içine eklenir | Kullanıcı tercihi: sistem menüsü kaybolmasın |
| Sekme durumu | Yalnızca aktif sekmenin listesi bellekte (Yaklaşım A) | Hafiflik; sekme sayısı belleği büyütmez |
| Platforma özgü kod | Yeni `gezik-platform` crate'i | Ortak kod platformdan bağımsız kalır |
| Kısayollar | `settings.toml`'dan değiştirilebilir | Kullanıcı tercihi |
| Sıralama | Sekmeler ve sabitlenenler sürükle-bırakla sıralanır | Kullanıcı tercihi |

## 3. Arayüz düzeni ve davranışlar

```
┌ Gezik ─────────────────────────────────────────────── _ □ × ┐
│ [■ Belgeler  ×] [■ İndirilenler] [■ gezik] [+]               │  sekme çubuğu
│ ← → ↑ ⟳  │ This PC › C: › Users › teoma › Belgeler │          │  araç çubuğu
├──────────────┬──────────────────────────────────────────────┤
│ FOLDERS      │ ■ Faturalar                                  │
│  Home        │ ■ Projeler                                   │
│  Desktop     │ □ notlar.txt                        2.1 KB   │
│  Documents ◀ │                                              │
│  Downloads   │                                              │
│ PINNED       │                                              │
│  gezik       │                                              │
│ DRIVES       │                                              │
│  Local (C:)  │                                              │
│  Data (D:)   │                                              │
├──────────────┴──────────────────────────────────────────────┤
│ 3 items                                                     │
└─────────────────────────────────────────────────────────────┘
```

Tüm renk ve ölçüler `Theme` global'inden gelir; yeni bileşenler 1. alt projenin `ToolButton`/`TextField` kalıbını izler (erişilebilirlik rolü, Tab ile odak, odak halkası).

### 3.1 Sekmeler

- Sekmede konumun görünen adı (bkz. 5.2) yazar.
- × düğmesi aktif sekmede ve fareyle üzerine gelinen sekmede görünür; orta tık sekmeyi kapatır.
- Son sekme kapatılırsa pencere kapanır.
- "+" yeni sekmeyi `start-folder` konumunda açar ve aktif yapar.
- Listede bir klasöre orta tık, klasörü arka planda yeni sekmede açar.
- Sekmeler pencereye sığmazsa en küçük genişliğe kadar daralır; sonra sekme çubuğu yatay kayar.
- **Sürükle-bırak sıralama:** Bir sekme tutulup sekme çubuğunda başka bir konuma bırakılınca yer değiştirir; sürükleme sırasında bırakılacak yer dikey bir çizgiyle (`Theme.accent`) gösterilir. Aktif sekme taşınsa da aktif kalır.
- Sekme sağ tık menüsü (yalnızca Gezik öğeleri): Çoğalt · Kapat · Diğerlerini kapat.

### 3.2 Araç çubuğu

- Geri, ileri, üst klasör, yenile düğmeleri. Geçmişi olmayan yöne ait düğme devre dışıdır; "This PC"de üst klasör devre dışıdır.
- **Adres çubuğu (breadcrumb):** Konum tıklanabilir parçalar halinde gösterilir (`This PC › C: › Users › teoma`). Bir parçaya tıklamak o konuma gider. Yol sığmazsa baştaki parçalar `…` ile kısalır (son parça her zaman görünür).
- Adres çubuğunda boş alana tıklamak ya da `focus-path` kısayolu adres çubuğunu metin moduna çevirir (tam yol, tamamı seçili). Enter gider; Esc veya odak kaybı iptal eder. Geçersiz yol: konum değişmez, durum çubuğunda uyarı.

### 3.3 Kenar çubuğu

Bölümler genel bir yapıdır (başlık + öğe listesi); ileride "Tags" bölümü aynı yapıyla eklenir.

- **FOLDERS:** Ev, Masaüstü, Belgeler, İndirilenler, Resimler, Müzik, Videolar — sistemde var olanlar, işletim sisteminin dilindeki adlarıyla. Kaldırılamaz, sıralanamaz.
- **PINNED:** `settings.toml`'daki `pinned` listesi; bu makinede var olmayan yollar gösterilmez. Sürükle-bırakla ve sağ tık menüsündeki Yukarı/Aşağı taşı ile sıralanır.
- **DRIVES:** Otomatik; etiket ve sürücü harfi/bağlama noktası (`Local Disk (C:)`).
- Bulunulan konuma karşılık gelen öğe vurgulanır (en uzun eşleşen değil, tam eşleşen).
- Genişlik ayırıcı sürüklenerek değiştirilir (en az 120, en çok 480 mantıksal piksel) ve `state.toml`'da saklanır.
- `layout.sidebar`: `left` | `right` | `hidden` uygulanır; canlı değişir.

### 3.4 "This PC" konumu

- Sürücüleri dosya listesinde gösteren sanal konum (`Location::Drives`). Satırda sürücü etiketi ve harfi/bağlama noktası, boyut sütununda boş.
- Windows'ta bir sürücünün kökünden (`C:\`) yukarı çıkınca; macOS ve Linux'ta `/`'den yukarı çıkınca buraya gelinir.
- Bir sürücüye çift tık/Enter o sürücünün köküne gider.
- `start-folder = "drives"` ile başlangıç konumu olabilir.

### 3.5 Dosya listesinde klavye ve fare

- ↑ ↓ PgUp PgDn Home End seçimi taşır ve seçili satırı görünür tutar; Enter açar (klasöre gider / dosyayı varsayılan uygulamayla açar).
- Harf yazmak, adı (büyük/küçük harf duyarsız) o harflerle başlayan ilk öğeye atlar; 1 saniye içinde yazılan harfler birleşir.
- Bu temel tuşlar değiştirilemez.
- Fare geri/ileri yan tuşları pencerenin her yerinde geri/ileri yapar.

## 4. Gezinme modeli (`gezik-core/src/nav.rs`)

Arayüzden bağımsız, saf Rust; birim testli.

```rust
pub enum Location { Path(PathBuf), Drives }

pub struct ViewState { pub selected: Option<String>, pub scroll: f32 }

pub struct History {
    back: Vec<(Location, ViewState)>,      // en fazla 100 kayıt
    current: (Location, ViewState),
    forward: Vec<(Location, ViewState)>,
}

pub struct Tabs { tabs: Vec<History>, active: usize }
```

### 4.1 Geçmiş kuralları

- `navigate(konum)`: mevcut (konum, görünüm) geri yığınına itilir, ileri yığını temizlenir, yeni konumun görünümü boştur. Mevcut konuma gitmek geçmiş oluşturmaz (yenileme sayılır).
- `back()` / `forward()`: mevcut (konum, görünüm) karşı yığına itilir; hedefin kayıtlı görünümü (seçim, kaydırma) geri döner.
- Geri yığını 100 kaydı aşarsa en eski kayıt atılır.
- Seçim satır indeksi değil dosya adıdır; geri gelindiğinde ad artık yoksa seçim boş olur.

### 4.2 Sekme işlemleri

- `open(konum, activate: bool)`: yeni sekme aktif sekmenin hemen sağına eklenir.
- `close(i)`: kapanan aktif sekmeyse sağındaki, sağı yoksa solundaki aktif olur. Son sekme kapanınca `Closed::LastTab` döner (pencere kapanır).
- `close_others(i)`, `duplicate(i)` (geçmişiyle birlikte kopya, sağına), `activate(i)`, `next()`, `prev()` (uçlarda başa/sona sarar).
- `move_tab(from, to)`: sekmeyi taşır; aktif sekmenin indeksi doğru güncellenir.

### 4.3 Yükleme ve hata durumları

- Listeleme arka plan iş parçacığında yapılır (mevcut nesil sayacı); başka bir yere gidilir ya da sekme değiştirilirse eski sonuç atılır.
- Yalnızca aktif sekmenin listesi bellektedir; sekmeye geçince klasör yeniden listelenir ve görünüm durumu (seçim, kaydırma) uygulanır.
- **Gidilemeyen konum** (erişim reddedildi, yok): geçmiş değişmez, mevcut konumda kalınır, durum çubuğunda uyarı.
- **Sekmeye dönüldüğünde ya da yenilendiğinde klasör yoksa:** var olan en yakın üst klasöre gidilir (geçmişe yeni kayıt olarak), uyarı gösterilir; hiçbir üst klasör yoksa (sürücü çıkarıldı) "This PC" açılır.
- Model ile Slint arasındaki bağ `gezik/src/navigation.rs`'tedir; mevcut `Ctx` yapısının yerini alır ve `main.rs`'i sadeleştirir.

## 5. Platform katmanı (`crates/gezik-platform`)

```rust
pub struct Drive { pub path: PathBuf, pub label: String, pub kind: DriveKind }
pub enum DriveKind { Fixed, Removable, Network, Optical }
pub fn drives() -> Vec<Drive>;

pub struct KnownFolder { pub path: PathBuf, pub name: String }
pub fn known_folders() -> Vec<KnownFolder>;     // var olanlar, sıra: Ev, Masaüstü, Belgeler, İndirilenler, Resimler, Müzik, Videolar

pub fn show_context_menu(window: &slint::Window, target: MenuTarget, at: LogicalPosition,
                         extra: &[GezikItem]) -> MenuResult;
pub enum MenuTarget { Item(PathBuf), Background(PathBuf) }
pub enum MenuResult { Gezik(GezikItemId), SystemCommandRan, Dismissed }
```

### 5.1 Sürücüler

- Windows: `GetLogicalDrives`, `GetVolumeInformationW` (etiket), `GetDriveTypeW` (tür). macOS: `/Volumes` girdileri. Linux: `/proc/self/mounts` içinden `/`, `/media/*`, `/run/media/*/*`, `/mnt/*`.
- Ayrıntı sorguları takılabilir (kopmuş ağ sürücüsü, diski olmayan CD), bu yüzden sürücü listesi arka plan iş parçacığında okunur; arayüz hiçbir zaman beklemez.
- Değişiklik takibi: 3 saniyede bir yalnızca ucuz liste (Windows'ta bit maskesi, diğerlerinde bağlama noktası listesi) karşılaştırılır; değiştiyse ayrıntılar yeniden okunur.

### 5.2 Görünen adlar

- Sistem klasörleri ve sürücüler işletim sisteminin verdiği yerelleştirilmiş adla gösterilir (Windows'ta Shell görünen adı; macOS/Linux'ta klasör adı).
- Diğer klasörlerde dosya adı.

### 5.3 Sağ tık menüsü — Windows

- Hedefin Shell menüsü (`IShellFolder::GetUIObjectOf` → `IContextMenu`; boş alan için klasörün arka plan menüsü) bir `HMENU`'ye doldurulur.
- Gezik öğeleri menünün en üstüne eklenir, ardından ayırıcı. Gezik öğeleri ve Shell komutları ayrı kimlik aralıklarında tutulur.
- Menü `TrackPopupMenuEx(TPM_RETURNCMD)` ile pencereye bağlı açılır. Alt menülerin ("Gönder", "Birlikte aç") çalışması için menü açıkken ilgili pencere mesajları `IContextMenu2/3::HandleMenuMsg(2)`'ye iletilir.
- Seçilen öğe Gezik öğesiyse `MenuResult::Gezik`; Shell komutuysa `InvokeCommand` çalıştırılır ve `SystemCommandRan` döner — bu durumda bulunulan klasör yenilenir.
- Yalnızca Explorer içinde çalışan komutlar (`rename`) menüden çıkarılır.
- Pencere tanıtıcısı (HWND) Slint'in `raw-window-handle` desteğiyle alınır.
- **Bilinen risk:** üçüncü parti menü eklentileri Gezik'in süreci içinde çalışır; hatalı bir eklenti Gezik'i çökertebilir (Files ve diğer dosya yöneticileriyle aynı risk). Takip notuna eklenir.

### 5.4 Sağ tık menüsü — macOS ve Linux

- Slint `ContextMenuArea` (macOS'ta yerel `NSMenu`, Linux'ta Slint'in çizdiği menü).
- Gezik öğelerine ek olarak: Aç · Varsayılan uygulamayla aç (dosyalar için).

### 5.5 Gezik menü öğeleri

| Nerede | Öğeler |
|---|---|
| Listede bir klasör | Yeni sekmede aç · Kenar çubuğuna sabitle (zaten sabitliyse: Sabitlemeyi kaldır) |
| Listede bir dosya | (yalnızca sistem menüsü; macOS/Linux'ta Aç · Varsayılan uygulamayla aç) |
| Listede boş alan | Yalnızca sistem menüsü (Windows) |
| Kenar çubuğunda bir öğe | Yeni sekmede aç · Sabitle / Sabitlemeyi kaldır · Yukarı taşı · Aşağı taşı (son ikisi yalnızca PINNED'de) |
| Sekme | Çoğalt · Kapat · Diğerlerini kapat |

Sağ tıklanan satır önce seçilir.

## 6. Ayarlar ve kısayollar

### 6.1 `settings.toml` eklemeleri

```toml
start-folder = "{home}"               # "{home}" | "drives" | herhangi bir yol (işaretler kullanılabilir)
pinned = ["{documents}/Projeler", "D:/Work/gezik"]

[shortcuts]                           # yalnızca değiştirmek istenenler yazılır
new-tab = "mod+t"
```

- `start-folder` geçersizse ya da yoksa ev klasörü kullanılır, uyarı verilir. Komut satırından verilen klasör `start-folder`'ın önüne geçer.
- Yol işaretleri 1. alt projedeki gibi (`{home}`, `{desktop}`, `{documents}`, `{downloads}`, `{pictures}`, `{music}`, `{videos}`). `..` içeren yollar reddedilir (uyarı, öğe yok sayılır).
- `pinned`: Sabitlerken yol `KnownDirs::collapse` ile işaretli biçime çevrilir. Aynı yol iki kez sabitlenemez.

### 6.2 `settings.toml`'a yazma

- Gezik bu dosyaya yalnızca `pinned` dizisini, sabitleme / kaldırma / sıralama yapıldığında yazar.
- Yazma `toml_edit` ile yapılır: kullanıcının yorumları, anahtar sırası ve biçimi korunur; yalnızca `pinned` değeri değişir (yoksa dosyanın üst düzeyine eklenir).
- Yazma `write_atomic` ile yapılır. Dosya izleyici bu yazımı görüp ayarları yeniden yükler; bu zararsızdır (aynı sonuç).
- `settings.toml`'da söz dizimi hatası varsa Gezik dosyaya dokunmaz; işlem yapılmaz ve "Fix settings.toml first" uyarısı gösterilir.
- 1. alt projedeki "Gezik `settings.toml`'a yazmaz" kuralı bu kapsamla güncellenir.

### 6.3 Kısayollar

| Eylem | Windows / Linux varsayılanı | macOS varsayılanı |
|---|---|---|
| `new-tab` | mod+t | mod+t |
| `close-tab` | mod+w | mod+w |
| `next-tab` | ctrl+tab | ctrl+tab |
| `prev-tab` | ctrl+shift+tab | ctrl+shift+tab |
| `back` | alt+left | mod+[ |
| `forward` | alt+right | mod+] |
| `up` | alt+up | mod+up |
| `focus-path` | mod+l | mod+l |
| `refresh` | f5 | mod+r |

- `mod` Windows/Linux'ta Ctrl, macOS'ta Cmd'dir.
- Biçim: değiştiriciler `ctrl`, `alt`, `shift`, `mod` (`+` ile birleşir, sırası önemsiz) ve tek bir tuş: `a`–`z`, `0`–`9`, `f1`–`f12`, `left`, `right`, `up`, `down`, `enter`, `tab`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, `escape`, `space`, `[`, `]`. Büyük/küçük harf duyarsız.
- `""` kısayolu kapatır.
- Geçersiz kısayol: uyarı, varsayılan geçerli kalır. Bilinmeyen eylem: uyarı.
- Çakışmalar:
  - Kullanıcının yazdığı kısayol, başka bir eylemin **varsayılan** tuşuyla çakışırsa kullanıcınınki geçerli olur; diğer eylemin varsayılanı devre dışı kalır ve bunu bildiren bir uyarı gösterilir.
  - `[shortcuts]` içinde iki eylem aynı tuşa yazılırsa dosyada önce yazılan geçerli olur, ikincisi devre dışı kalır, uyarı gösterilir.
- Kısayollar ayar dosyası değişince canlı güncellenir.
- Adres çubuğunda yazarken değiştiricisiz kısayollar metne gider; değiştiricili olanlar (ör. `mod+w`) çalışır. Metin düzenleme kısayolları (mod+a/c/v/x/z) metin kutusunda metin kutusuna aittir.

### 6.4 `state.toml` eklemesi

```toml
[sidebar]
width = 200
```

## 7. Kod yapısı

```
crates/
  gezik-core/src/nav.rs           YENİ — Location, ViewState, History, Tabs (saf, testli)
  gezik-config/src/settings.rs    start-folder, pinned, [shortcuts] okuma
  gezik-config/src/shortcuts.rs   YENİ — kısayol ayrıştırma, varsayılanlar, çakışma kontrolü (saf, testli)
  gezik-config/src/settings_edit.rs YENİ — toml_edit ile pinned yazımı
  gezik-config/src/paths.rs       expand: `..` reddi
  gezik-platform/                 YENİ — drives, known_folders, show_context_menu
    src/lib.rs
    src/windows/{drives.rs, shell_menu.rs, known_folders.rs}
    src/macos.rs, src/linux.rs
  gezik/
    src/navigation.rs             YENİ — Tabs + arka plan yükleme + Slint bağı (Ctx'in yerine)
    src/sidebar.rs                YENİ — kenar çubuğu bölümleri, sürücü takibi
    src/shortcuts.rs              YENİ — tuş olayını eyleme eşleme
    src/context_menu.rs           YENİ — menü öğeleri ve sonuçlarının işlenmesi
    ui/widgets/tab-bar.slint, breadcrumb.slint, sidebar.slint
```

**Yeni bağımlılıklar:** `toml_edit` (gezik-config); `windows` (gezik-platform, yalnızca Windows hedefinde, gerekli özelliklerle); Slint `raw-window-handle-06` özelliği (gezik).

## 8. Test stratejisi

### 8.1 Birim testleri

- `nav`: navigate/back/forward ve görünüm durumunun geri gelmesi; aynı konuma gitme; ileri yığınının temizlenmesi; 100 kayıt sınırı; open (konum), close (aktif/aktif olmayan, son sekme), close_others, duplicate, move_tab (ilk/son konum, aynı yer, aktif sekme indeksi), next/prev sarma.
- `shortcuts`: geçerli/geçersiz biçimler, büyük/küçük harf, değiştirici sırası, `mod` çözümü (platform parametreli), `""` ile kapatma, bilinmeyen eylem, çakışma kuralı.
- `settings`: `start-folder` ve `pinned` okuma, `..` reddi, aynı yolun tekrarı.
- `settings_edit`: yorumlar ve diğer anahtarlar korunarak `pinned` güncelleme; `pinned` yokken ekleme; bozuk dosyaya dokunmama.
- `gezik-platform`: `drives()` ve `known_folders()` her sistemde boş olmayan sonuç döner (CI'da her işletim sisteminde).

### 8.2 Elle doğrulama (ekran görüntüsüyle)

- Sekmeler: açma, kapatma, orta tık, sürükleyerek sıralama, sekme menüsü.
- Adres çubuğu: parçaya tıklama, metin modu, geçersiz yol uyarısı, uzun yolun kısaltılması.
- Kenar çubuğu: üç bölüm, vurgulama, sabitleme/kaldırma/sıralama ve `settings.toml`'daki yorumların korunması, genişliğin hatırlanması, `sidebar = right/hidden`.
- Windows Shell menüsü: bir dosyada ve klasörde sistem öğeleri + Gezik öğeleri; "Gönder" alt menüsü; kurulu bir üçüncü parti eklentinin (ör. 7-Zip) görünmesi; Shell'den yeni klasör oluşturunca listenin yenilenmesi.
- Fare yan tuşları, klavye kısayolları ve harfle atlama.
- "This PC": `C:\`'den yukarı çıkma, sürücüye girme.

### 8.3 Performans

`scripts/perf/measure.ps1` ve `stress.ps1` ile mevcut ölçütler; yeni ölçüt: 20 sekme açıkken bellek tek sekmeye göre en fazla +2 MB.

## 9. Yol haritasındaki yeri

| # | Alt proje |
|---|---|
| 1 | Ayarlar + Tema sistemi — tamamlandı |
| **2** | **Gezinme (bu belge)** — yerel sağ tık menüsü altyapısı dahil |
| 3 | Görünüm: sütunlar, doğal sıralama, çoklu seçim, ikonlar, ızgara, önizleme |
| 4 | Dosya işlemleri: kopyala/taşı/sil, çakışmalar, sürükle-bırak, menüye Gezik dosya işlemi öğeleri |
| 5 | Etiketler: macOS etiketleri tüm sistemlere; depolama, atama, kenar çubuğu bölümü |
| 6 | Taşınabilirlik: dışa/içe aktarma, senkron klasör, GitHub'dan tema kurma |
| 7 | Bulut senkronu |
| 8 | Gelişmiş: arama, çift panel, arşivler, Git, komut paleti |
| — | Güncelleme bildirimi + otomatik güncelleme: ilk herkese açık sürümden önce |
