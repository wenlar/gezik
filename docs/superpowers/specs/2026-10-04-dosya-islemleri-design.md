# Alt Proje 4: Dosya İşlemleri — Tasarım

- **Tarih:** 2026-10-04
- **Durum:** İncelemede
- **Kapsam:** Gezik yol haritasının 4. alt projesi; iki plan ve iki PR: **4a** (motor ve işlemler), **4b** (sürükle-bırak)
- **Dayandığı:** `2026-10-03-ayarlar-ve-tema-design.md` (ayar dosyası, tema, kısayol biçimi), `2026-10-04-gezinme-design.md` (sekmeler, Shell menüsü), `2026-10-04-gorunum-design.md` (çoklu seçim, `EntryModel`, ızgara)

## 1. Amaç

Gezik'e her sistemde aynı çalışan, Explorer'dan hızlı ve geri alınabilir dosya işlemleri kazandırmak: kopyala, taşı, çöpe at, kalıcı sil, yeniden adlandır, yeni klasör/dosya, çoğalt; çakışmaları önceden gösteren liste; sistem panosu; çok adımlı geri alma; sürükle-bırak. Motor, ileride gelecek toplu işlemlerin (toplu yeniden adlandırma, biçim dönüştürme, arşivler) eklenebileceği biçimde genel tasarlanır.

### Başarı ölçütleri

- Ctrl+C/X/V, Del, Shift+Del, F2, Ctrl+Shift+N, Ctrl+Z/Y Windows, macOS ve Linux'ta Gezik'in kendi motoruyla çalışır; Windows'ta Explorer'ın ilerleme penceresi hiç açılmaz.
- Gezik'te kopyalanan dosya Explorer'a/Finder'a yapıştırılabilir ve tersi.
- Hedefte çakışma varsa kopyalamadan önce tüm çakışmalar tek listede gösterilir; hiçbir dosya kullanıcı seçmeden ezilmez.
- Her kullanıcı eylemi tek Ctrl+Z ile geri alınır (kalıcı silme hariç; ezilen dosyalar çöpten geri gelir).
- Süren işlemler pencere içindeki panelde görünür; aynı diske dokunan işlemler sıraya girer, farklı disklerdekiler paralel çalışır.
- Dosyalar Gezik içinde, Gezik'ten dışarı ve dışarıdan Gezik'e sürüklenebilir (üç sistem; macOS/Linux denenmemiş olarak).
- Performans (bkz. 11): 10 bin küçük dosyayı Explorer'dan ≥ 2 kat hızlı kopyalar; 50 bin dosyalık ağacı kalıcı silmede öğe < 100 ms'de kaybolur; boşta bellek değişmez.

### Kapsam dışı (bilerek)

- Toplu yeniden adlandırma ekranı, biçim dönüştürme, arşiv oluşturma/açma (Alt proje 5; motor buna hazır, bkz. 4.3).
- Arşivin içinde klasör gibi gezinme, sanal Shell öğeleri (MTP telefon, zip içi) üzerinde işlem (Alt proje 9).
- Panodaki resmi/metni dosya olarak yapıştırma.
- Sürükleyerek kısayol veya sembolik bağlantı oluşturma (Alt tuşu).
- Klasör içeriğinin canlı izlenmesi (Gezik'in kendi işlemlerinden sonra etkilenen klasörler yenilenir).
- Geri alma geçmişinin uygulama kapandıktan sonra saklanması.
- Windows Çöp Kutusu'na `IFileOperation` dışında bir yolla taşıma (belgelenmemiş `$Recycle.Bin` yapısına dokunulmaz).

## 2. Alınan kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Motor | Her sistemde Gezik'in kendi motoru | Kuyruk, ön tarama, geri alma ve tek tip arayüz için denetim gerekli |
| Kopyalama yolu | Planlama ve uygulama ayrı; dosya kopyası sistemin hızlı çağrısıyla (Yaklaşım A) | Klonlama, SMB sunucu tarafı kopya, ADS/öznitelikler kendiliğinden gelir |
| Geri alma | Çok adımlı, oturum boyunca; Ctrl+Z / Ctrl+Y | Kullanıcı tercihi |
| İlerleme | Pencere içi panel, durum çubuğuna daraltılabilir; Windows'ta görev çubuğu ilerlemesi | Ek pencere yok, bellek ve odak sorunu yok |
| Pano | Sistem panosu, üç sistemde iki yönlü | Kullanıcı tercihi |
| Çakışmalar | Önce tara, tek listede göster, satır başına karar; varsayılan Atla | Kullanıcı tercihi; sessiz ezme yok |
| Kuyruk | Disk kümesine göre: kesişen işlemler sırayla, diğerleri paralel | Aynı diskte çekişme yok, farklı disklerde bekleme yok |
| Hız | SSD'de işlem içi paralellik, büyük dosyada önbelleksiz kopya, paralel POSIX silme | Explorer'ın yavaş olduğu durumlar küçük dosyalar ve büyük ağaçlar |
| Kalıcı silme | Anında görünür: gizli ada çevir, arka planda sil; açılışta toparlama | Kullanıcı tercihi |
| Ezilen dosyalar | Önce çöpe | Değiştirme de geri alınabilir |
| Windows Shell menüsü | Kalır; cut/copy/paste/delete/rename komutları Gezik motoruna yönlendirilir | Aynı komutun iki davranışı olmaz |
| Sürükle-bırak | Üç sistemde tam; pencere içi Gezik kodu, dışarı/içeri sistem API'leri | Kullanıcı tercihi |
| Genişleme | Motor bir `Task` arayüzünü çalıştırır; işlemler bu arayüzün uygulamaları | Toplu işlemler (Alt proje 5) aynı kuyruk, panel, çakışma listesi ve geri almayı kullanır |
| Bölme | Tek spec, iki plan/PR (4a, 4b) | Tasarım tutarlı kalır, plan yönetilebilir boyutta olur |

## 3. Mimari

### 3.1 Katmanlar

| Katman | Yeri | Sorumluluk |
|---|---|---|
| Planlama | `gezik-core::ops` (yeni modül) | Sistemden ve dosya sisteminden bağımsız saf mantık: işlem türleri, hedef yolu hesaplama, `ad (2)` üretimi, kendi içine kopyalama reddi, çakışma kararları, geçersiz ad denetimi, yeniden adlandırma döngülerinin çözümü, geri alma kaydının tersini çıkarma |
| Sistem işlemleri | `gezik-platform::fs` (yeni modül); pano `gezik-platform::clipboard`, sürükle-bırak `gezik-platform::dnd` | Dosya kopyalama (ilerleme ve iptalle), POSIX tarzı silme, çöpe atma (çöpteki yeni yolu döndürür), çöpten geri getirme, disk kimliği ve türü (SSD/HDD/ağ), çöp desteği sorgusu |
| Motor | `gezik-ops` (yeni crate; `gezik-core` ve `gezik-platform`'a bağlı, arayüze bağlı değil) | Ön tarama, kuyruk, işçi havuzları, duraklat/devam/iptal, ilerleme olayları, geri alma geçmişi, anında silme toparlaması |
| Arayüz | `gezik` | İlerleme paneli, çakışma listesi, yerinde yeniden adlandırma, menüler ve kısayollar, soluk (kesilmiş) öğeler, klasör yenileme, sürükleme hayaleti ve bırakma vurgusu |

### 3.2 Veri akışı

```
UI ──submit(Task)──▶ Engine ──▶ Scan (worker) ──▶ conflicts? ──▶ UI conflict list
                       │                                       ◀── decisions
                       ▼
                drive-set queue ──▶ Job (N workers) ──events──▶ channel
                                                                  │
                       UI ◀── slint::invoke_from_event_loop (≤ 10/s, batched)
```

- Arayüz iş parçacığı dosya sistemine dokunmaz; tek dosyalık yeniden adlandırma da motordan geçer.
- Olaylar kanaldan akar; arayüz bunları saniyede en çok ~10 kez toplu alır (`media.rs` kalıbı).
- Motorun iş parçacıkları ilk işlemle açılır, ~10 sn boşta kalınca kapanır. İşlem yokken bellek maliyeti sıfırdır.
- Her iş bitince ve sürerken ~1 sn aralıkla değişen klasörlerin listesi gelir; o klasörü gösteren sekmeler seçim ve kaydırma korunarak yenilenir (`Navigator::reload` genişletilir). Yeni oluşturulan öğe (yeni klasör, yapıştırılanlar) yenilemeden sonra seçili gelir.

### 3.3 Kuyruk

- Her işin bir **disk kümesi** vardır: dokunduğu disklerin kimlikleri (Windows'ta birim seri numarası, Unix'te `st_dev`). C:→D: kopyalama {C, D}, C:→C: {C}.
- Kümesi çalışan hiçbir işle kesişmeyen iş hemen başlar; kesişen iş sırayla bekler ("waiting for C:"). Bekleyen iş için "Start now" kuyruğu atlatır.
- Ağ yolları için anahtar sunucu ve paylaşım adıdır (`\\sunucu\paylaşım`).
- Sıra, işin çalıştığı süre boyunca değişmez; çakışma listesinde bekleyen iş diskini tutmaya devam eder (çakışmasız öğeleri zaten kopyalıyordur).

## 4. Motor (`gezik-ops`)

### 4.1 `Task` arayüzü

```rust
pub trait Task: Send {
    fn title(&self) -> String;                                  // "Copying 312 items to D:\Yedek"
    fn kind(&self) -> TaskKind;                                 // Copy, Move, Trash, … (undo label, icons)
    fn resources(&self) -> Resources;                           // drive set + Disk | Cpu | External
    fn plan(&mut self, cx: &ScanCx) -> io::Result<Plan>;        // every item: source → target(s)
    fn run(&self, item: &PlanItem, cx: &RunCx) -> io::Result<Outcome>; // progress, cancel, pause
    fn undo(&self, done: &[Outcome]) -> Option<Box<dyn Task>>;  // inverse, if any
}
```

- `plan` her öğenin hedefini çalışmadan önce bildirir; ön tarama ve çakışma bulma işlem türünden bağımsızdır. Plan akış halinde üretilir (`ScanCx` üzerinden parça parça), böylece çakışmasız öğeler tarama sürerken başlayabilir.
- `Resources` paralelliği seçer: `Disk` → disk türüne göre işçi (bkz. 5.1); `Cpu` → çekirdek sayısı; `External` → sınırlı sayıda alt süreç, iptalde sonlandırılır.
- `Outcome` geri alma için gerekenleri taşır: oluşan yol, çöpteki yol, eski ad ve işlemden sonraki boyut/değişme tarihi.
- 4a'nın uygulamaları: `Copy`, `Move`, `Trash`, `Delete`, `Restore`, `Rename`, `NewFolder`, `NewFile`, `Duplicate` (Copy'nin aynı klasör hâli).

### 4.2 Geri alma geçmişi

- Bir kullanıcı eylemi geçmişte tek kayıttır (300 öğelik kopya = tek Ctrl+Z). Geçmiş oturum boyunca tutulur, uzunluğu 100 kayıtla sınırlıdır; uygulama kapanınca silinir.
- Geri alma tersine görevi (`Task::undo`) normal bir iş olarak kuyruğa koyar; ilerlemesi panelde görünür. Yinele (Ctrl+Y) ters görevin tersidir.
- Tersler: Copy/Duplicate/NewFolder/NewFile → oluşanları çöpe at (çöp yoksa boş klasör ve yeni boş dosya silinir; diğerleri için geri alma "cannot undo: no recycle bin" der). Move → geri taşı. Rename → eski ad. Trash → çöpten geri getir. Delete → yok. Replace ile ezilen hedef → çöpten geri getir.
- Geri almadan önce her öğe denetlenir: yolu, boyutu ve değişme tarihi `Outcome`'dakiyle aynı mı? Değişen öğeler atlanır, panelde "2 items changed since; skipped" yazar.
- Yeni bir işlem yinele zincirini keser (geri alınmış kayıtlar atılır).
- İptal edilen işte o ana kadar biten öğeler kayda girer.

### 4.3 Genişleme (Alt proje 5 için)

Yeni bir özellik yeni bir `Task` demektir; kuyruk, panel, çakışma listesi, iptal ve geri alma onunla birlikte gelir:

- **Toplu yeniden adlandırma:** `plan` tüm yeni adları önceden hesaplar (önizleme ekranı aynı planı kullanır); döngüler (a→b, b→a) geçici adlarla çözülür. 4a'daki tek dosyalık `Rename` aynı `Task`'tır.
- **Dönüştürme** (resim, metin kodlaması/satır sonu, ffmpeg ile ses/video, kullanıcı komutu) ve **arşiv oluşturma/açma** (zip, 7z, gzip/tar): yeni dosya üretenlerin tersi üretilenleri çöpe atmak; yerinde değiştirenler asıl dosyayı önce çöpe kopyalar.
- Bu nedenle 4a'da `Task`, `Plan`, `Outcome` ve `Resources` türleri yalnızca 4a işlemlerinin ihtiyacına göre değil, bu kullanımlar düşünülerek tanımlanır; ama 4a bunların hiçbirini uygulamaz.

## 5. İşlemler

### 5.1 Hız

- **İşlem içi paralellik:** `Disk` işlerinde işçi sayısı disk türüne göre: SSD/NVMe 6, HDD 1, ağ 4 (`copy-threads` ile değiştirilebilir). Disk türü Windows'ta `IOCTL_STORAGE_QUERY_PROPERTY` → `IncursSeekPenalty`, Linux'ta `/sys/block/*/queue/rotational`, macOS'ta SSD kabul edilir. Belirlenemezse 1.
- **Kopyalama çağrısı:** Windows `CopyFileExW` (ilerleme bildirimi + iptal bayrağı; ≥ 256 MB dosyada `COPY_FILE_NO_BUFFERING`); macOS `copyfile` (`COPYFILE_CLONE` ile APFS'te anında); Linux önce `ioctl(FICLONE)`, sonra `copy_file_range`, olmazsa tamponlu kopya. Zaman damgaları ve izinler korunur.
- **Ön tarama:** Windows'ta `FindFirstFileExW` (`FindExInfoBasic`, `FIND_FIRST_EX_LARGE_FETCH`); SSD'de klasörler paralel taranır.
- **Kalıcı silme:** saymadan, SSD'de paralel; Windows'ta `SetFileInformationByHandle(FileDispositionInfoEx, POSIX_SEMANTICS | IGNORE_READONLY_ATTRIBUTE)`, olmazsa `DeleteFileW`.
- **Arayüz yükü:** ilerleme olayları birleştirilip saniyede en çok ~10 kez iletilir.

### 5.2 İşlem tablosu

| İşlem | Tetikleyen | Nasıl | Geri alma |
|---|---|---|---|
| Kopyala | Ctrl+C → Ctrl+V | 5.1 | Kopyaları çöpe at |
| Taşı | Ctrl+X → Ctrl+V | Aynı diskte yeniden adlandırma; farklı diskte dosya dosya kopyala → başarılıysa kaynağı sil | Geri taşı |
| Çöpe at | Del | Sistem çöpü (bkz. 5.4); öğe listeden hemen gizlenir | Çöpten geri getir |
| Kalıcı sil | Shift+Del, onaylı | Anında silme (5.3) | Yok; onay penceresi söyler |
| Yeniden adlandır | F2 (macOS: Enter), yerinde | Tek yeniden adlandırma | Eski ad |
| Yeni klasör | Ctrl+Shift+N, menü | `New folder`, `New folder (2)`…; oluşunca yerinde yeniden adlandırma açılır | Çöpe at (boşsa sil) |
| Yeni dosya | Menü | Boş `New file.txt`; yerinde yeniden adlandırma açılır | Çöpe at (boşsa sil) |
| Çoğalt | Menü (macOS: ⌘D) | Aynı klasöre `ad (2)` | Kopyayı çöpe at |

### 5.3 Kurallar

- **Ad üretimi:** `rapor.pdf` → `rapor (2).pdf`, `(3)`…; uzantısız `Makefile (2)`; çift uzantı `arsiv.tar.gz` → `arsiv (2).tar.gz` (bilinen çift uzantılar: `.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`); klasörlerde uzantı yok sayılır (`v1.2` → `v1.2 (2)`). Mevcut `(n)` soneki artırılır (`rapor (2).pdf` → `rapor (3).pdf`).
- **Aynı klasöre yapıştırma:** kaynağın kendi klasörüne kopyalamada çakışma listesi açılmaz; kopyalar doğrudan `ad (2)` olur. Aynı klasöre taşıma hiçbir şey yapmaz.
- **Kendi içine:** klasörü kendi içine veya alt klasörüne kopyalama/taşıma planlamada reddedilir ("Cannot copy a folder into itself").
- **Bağlantılar:** sembolik bağlantılar ve junction'lar bağlantı olarak kopyalanır; silmede içine girilmez.
- **Uzun yollar:** Windows'ta motor yolları `\\?\` önekiyle kullanır; arayüze öneksiz döner.
- **Öğe hataları** (erişim, kilit, kayıp kaynak): öğe atlanır, iş sürer; bitince "N items failed" + Details / Retry failed.
- **Yer kalmaması:** iş duraklatılır ve sorulur ("D: is full — free space and Resume, or Cancel").
- **Art arda hatalar:** 20 öğe art arda başarısız olursa (ör. ağ koptu) iş duraklatılır ve sorulur.
- **İptal:** büyük dosyada blok düzeyinde etki eder; yarım hedef dosya silinir; bitmiş öğeler kalır ve geri alınabilir.
- **Hız/kalan süre:** son ~5 sn'nin hareketli ortalaması; kalan süre ön taramadaki toplam bayttan. Tarama bitmeden yalnızca öğe sayısı gösterilir.
- **Yeniden adlandırmada ad varsa:** çakışma listesi açılmaz; ipucu "A file with this name already exists". Yalnızca büyük/küçük harf değişen ad (Windows/macOS) iki adımda (geçici ad üzerinden) yapılır.

### 5.4 Çöp kutusu

- **Windows:** `IFileOperation` arayüzsüz (`FOF_NO_UI`, `FOFX_RECYCLEONDELETE`) ve bir `IFileOperationProgressSink` ile; `PostDeleteItem`'daki yeni öğenin yolu `Outcome`'a yazılır. Geri getirme bu yoldan asıl yere taşımadır. Büyük klasörde sistem önce boyutu hesapladığı için yavaştır; öğe listeden hemen gizlendiğinden kullanıcı beklemez.
- **macOS:** `NSFileManager trashItemAtURL:resultingItemURL:`; geri getirme sonuç yolundan taşıma.
- **Linux:** freedesktop çöp belirtimi: aynı diskteyse `$XDG_DATA_HOME/Trash`, değilse diskin `.Trash-$uid`'i; `files/` + `info/*.trashinfo`. Geri getirme taşıma + `.trashinfo` silme.
- **Çöp yoksa** (ağ, bazı USB): Del sorar: "This drive has no recycle bin. Delete permanently?" Çöp desteği diske göre önbelleklenir.
- `confirm-trash = true` ise Del önce onay ister.

### 5.5 Anında kalıcı silme

1. Her kök öğe aynı klasörde gizli bir ada çevrilir: `.gezik-deleting-<rastgele>` (Windows'ta ayrıca gizli özniteliği). Öğe listeden kaybolur.
2. Gizli yol, yapılandırma klasöründeki `pending-deletes` dosyasına (satır başına bir mutlak yol) yazılır.
3. Arka planda paralel silinir; bitince yol dosyadan çıkarılır. Panelde "Deleting 1,204 items" görünür.
4. Yeniden adlandırma yapılamazsa (izin, kilit, salt okunur ana klasör) o öğe için normal paralel silmeye düşülür.
5. Açılışta `pending-deletes` doluysa, var olan yollar için silmeye arka planda devam edilir; olmayanlar dosyadan atılır. Yol `.gezik-deleting-` ile bitmiyorsa güvenlik için silinmez ve atılır.
6. Pencere kapatılırken süren anında silmeler kapatma sorusunu tetiklemez; 5. adım tamamlar.

## 6. Çakışma listesi

### 6.1 Akış

1. İş başlar, ağaç taranır; panelde "Scanning… 12,400 items · 3.1 GB".
2. Çakışmasız öğeler tarama sürerken kopyalanmaya başlar ve liste açıkken de sürer.
3. Tarama bitince çakışma varsa liste, dosya listesinin üstünde pencere içi bir katman olarak açılır; panelde iş "waiting for decisions" gösterir, panel kapalıysa açılır.

```
┌ 7 conflicts · copying 312 items to D:\Yedek ─────────────────────────────┐
│ [Replace] [Skip] [Keep both] [If newer]   apply to: ◉ selected ○ all      │
│ ☐ Hide identical (2)                                                      │
├───────────────────────────────────────────────────────────────────────────┤
│   Name                    Source              Target             Action   │
│ ▸ rapor.pdf               2.1 MB  10-04 14:02 1.9 MB  09-30 09:11 Replace ▾│
│   fotolar\IMG_0012.jpg    4.0 MB  10-01 10:00 4.0 MB  10-01 10:00 Skip   ▾ │
│   notlar  (folder)        —                   —                  Merge    │
│   ozet    (file→folder)   12 KB               folder             Skip   ▾ │
├───────────────────────────────────────────────────────────────────────────┤
│ Replaced files go to the Recycle Bin.        [Cancel operation] [Start]  │
└───────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Kurallar

- Satır başına karar: **Replace / Skip / Keep both** (`ad (2)`) **/ If newer**. Daha yeni taraf tarihin yanında işaretlenir.
- Varsayılan karar **Skip**. Üstteki düğmeler seçili satırlara veya tümüne uygulanır.
- Boyutu ve değişme tarihi aynı olan dosyalar "identical" işaretlenir, varsayılan olarak atlanır; "Hide identical" ile gizlenir.
- Klasör–klasör: birleştirilir; satır bilgi amaçlıdır (karar seçilmez); içerideki dosya çakışmaları ayrı satırlardır. Dosya–klasör (aynı ad): yalnızca Skip veya Keep both.
- Liste Rust'ta tutulur, Slint'e tembel model olarak verilir (dosya listesi gibi); 10 bin satırda akıcıdır.
- Klavye: oklar, Shift/Ctrl ile seçim; R / S / K / N = Replace / Skip / Keep both / If newer; Enter = Start; Esc = Cancel operation.
- **Cancel operation** işi durdurur; o ana kadar kopyalanmış çakışmasız öğeler kalır ve Ctrl+Z ile geri alınabilir (düğmenin ipucu söyler).
- Alt satır: çöpü olan hedefte "Replaced files go to the Recycle Bin." (macOS/Linux'ta "Trash"); olmayanda "Replaced files cannot be restored."
- Taşıma, Restore ve Alt proje 5 işlemleri aynı listeyi kullanır; başlık işleme göre değişir.

## 7. Arayüz

### 7.1 İlerleme paneli

```
├──────────────────────────────────────────────────────────────── [▾] ┤
│ Copying 312 items to D:\Yedek     ██████████░░░░ 61%  84 MB/s ~0:42 ‖ ×│
│ Moving 3 items to C:\Arsiv        waiting for C:        [Start now]  × │
│ Deleted 1,204 items · 3 failed                  [Details] [Retry]   × │
├──────────────────────────────────────────────────────────────────────┤
│ 4 items · 1 selected                                                 │
```

- Panel durum çubuğunun üstündedir; her iş bir satır: başlık, çubuk, yüzde, hız, kalan süre, duraklat (‖) / devam, iptal (×).
- 1 sn'den kısa süren işler hiç görünmez (iş 1 sn sonra hâlâ sürüyorsa satır eklenir).
- Başarılı biten satır 3 sn sonra kaybolur; hatalı biten satır kapatılana kadar kalır. "Details" başarısız öğeleri ve nedenlerini gösteren bir katman açar.
- `[▾]` paneli durum çubuğunun sağındaki özete daraltır: `⟳ 2 operations · 61%`; özete tıklamak açar. Çakışma listesi açıldığında, iş duraklatıldığında (yer yok, art arda hata) veya hatayla bittiğinde panel kendiliğinden açılır. Daraltılmış hâl `state.toml`'da saklanır.
- **Windows görev çubuğu:** `ITaskbarList3::SetProgressValue` ile tüm işlerin toplam ilerlemesi; çakışma beklerken veya duraklatılmışken `TBPF_PAUSED`, hata varken `TBPF_ERROR`; iş yokken `TBPF_NOPROGRESS`.
- **Pencereyi kapatma:** iş sürerken pencere içi soru katmanı: "2 operations are running" → [Keep open] [Cancel them and quit]. Yalnızca anında silmeler sürüyorsa sorulmaz.

### 7.2 Menüler

- **Windows, dosya satırları:** sistemin Shell menüsü kalır. Seçilen komutun kanonik adı `IContextMenu::GetCommandString(GCS_VERBW)` ile okunur; `cut`, `copy`, `paste`, `delete`, `rename` Gezik'e yönlendirilir (`delete` Shift basılıyken kalıcı silme). Diğer tüm komutlar sisteme gider.
- **Windows, arka plan menüsü:** en üste Gezik öğeleri: Undo `<işlem>`, Redo `<işlem>`, Paste, New folder, New file, Refresh; ardından sistemin arka plan menüsü.
- **macOS ve Linux:** Gezik'in kendi menüsü: Cut, Copy, Paste, Duplicate, Rename, Move to Trash, Delete permanently, New folder, New file, Undo/Redo. Klasöre özel öğeler (yeni sekmede aç, sabitle) korunur.
- Undo/Redo öğesi neyi geri alacağını yazar ("Undo Move 3 items"); geçmiş boşsa öğe görünmez. Paste öğesi panoda dosya yoksa görünmez.
- Bir klasör satırının menüsündeki Paste o klasörün içine yapıştırır.

### 7.3 Yerinde yeniden adlandırma

- Liste satırındaki veya ızgara döşemesindeki ad metin alanına dönüşür (mevcut `TextField` kalıbı); uzantı hariç ad seçili gelir (klasörde tüm ad).
- Enter veya başka yere tıklamak kaydeder; Esc vazgeçer; Tab kaydedip sıradaki öğeyi düzenlemeye açar, Shift+Tab öncekini.
- Yazarken anında denetim, alanın altında kırmızı ipucu: Windows'ta `\ / : * ? " < > |`, denetim karakterleri, `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9` (uzantılı hâlleri dahil), sonda nokta veya boşluk; Unix'te `/` ve NUL; her yerde boş ad, `.` ve `..`. Geçersizken Enter çalışmaz.
- Düzenleme sırasında liste yenilenirse (başka bir iş bitti) düzenleme aynı öğede (adıyla bulunarak) sürer; öğe kaybolduysa düzenleme kapanır.

### 7.4 Kesilmiş öğeler

- Gezik'te veya başka bir uygulamada kesilip panoda bekleyen öğeler, Gezik listesinde 0,5 saydamlıkla çizilir. Pano değişince (bkz. 8) soluk gösterim kalkar.

## 8. Pano

- Ctrl+C / Ctrl+X seçili yolları sistem panosuna yazar. Ctrl+V bulunulan klasöre yapıştırır; yapıştırma her zaman Gezik motoruyla yapılır.
- Kesip yapıştırma bitince pano temizlenir ve kaynağa "taşındı" bilgisi bırakılır.
- Paste öğesi ve soluk gösterim için panonun değişiklik sayacı (Windows `GetClipboardSequenceNumber`, macOS `changeCount`) menü açılırken, Ctrl+V'de ve pencere odak kazandığında okunur; pano sürekli dinlenmez.

| Sistem | Yazma | Okuma |
|---|---|---|
| Windows | Seçili öğelerin PIDL'lerinden `SHCreateDataObject`; kesmede `Preferred DropEffect = DROPEFFECT_MOVE`; `OleSetClipboard` + `OleFlushClipboard` (Gezik kapansa da pano kalır) | `OleGetClipboard`: `CF_HDROP` + `Preferred DropEffect`; taşıma bitince `Performed DropEffect` ve `Paste Succeeded` yazılır. Yalnızca sanal öğe içeren panoda sistemin yapıştırma komutuna düşülür |
| macOS | `NSPasteboard` `writeObjects` ile dosya URL'leri; Gezik kendi kestiğini `changeCount` ile hatırlar | Dosya URL'leri; ⌘⌥V her zaman taşır (Finder gibi) |
| Linux | `text/uri-list`, `x-special/gnome-copied-files` (`copy`/`cut` + URI'ler), `application/x-kde-cutselection`; X11'de seçim sahipliği, Wayland'da `wlr-data-control`/`ext-data-control`, yoksa pencere odaklıyken `wl_data_device`. Crate seçimi (`x11rb`, `wl-clipboard-rs` veya benzeri) planda doğrulanır | Aynı biçimler |

macOS ve Linux pano kodu bu geliştirme makinesinde çalıştırılamaz; "denenmedi" olarak takip notuna yazılır.

## 9. Sürükle-bırak (4b)

### 9.1 Davranış

- İşaretçi ~4 mantıksal px hareket edince sürükleme başlar. Seçili öğeden başlarsa tüm seçim, seçili olmayandan başlarsa önce o öğe seçilir.
- Çoklu seçimdeki bir öğeye basmak seçimi artık basınca değil, sürüklenmeden bırakınca tek öğeye indirir (Görünüm takip notundaki madde kapanır).
- Varsayılan etki: aynı diskte taşı, farklı diskte kopyala. Shift taşı, Ctrl (macOS ⌥) kopyala. İmleç yanında etiket: "Move to Belgeler" / "Copy to D:\".
- Sağ tuşla sürükleme: bırakınca Copy here / Move here / Cancel menüsü.
- Bırakma hedefleri: listedeki klasör satırı/döşemesi (vurgulu, `drop-target` rengi), boş alan (bulunulan klasör), kenar çubuğundaki klasörler ve sürücüler, adres çubuğu parçaları, sekmeler (üstünde ~600 ms beklenince etkinleşir, sürükleme sürer). Liste kenarında otomatik kaydırma (çerçeve seçimindeki mantık).
- Kenar çubuğunda PINNED öğelerinin arasına bırakılan klasör sabitlenir (pano işlemi değil, `pinned` listesine ekleme); bir öğenin üstüne bırakmak içine taşır/kopyalar.
- Kendi üstüne, kendi alt klasörüne ve yazılamayan hedeflere bırakmada "olmaz" imleci. Esc iptal eder.
- Bırakma normal bir iştir; Ctrl+Z ile geri alınır.

### 9.2 Teknik yaklaşım

- **Pencere içi** sürükleme Gezik kodudur: Slint işaretçi olayları, sürüklenen öğelerin hayaleti (ikon + sayı rozeti) Slint katmanında. Slint'in deneysel `DragArea`/`DropArea`'sı kullanılmaz.
- **Dışarı:** işaretçi pencere sınırını geçince sürükleme sisteme devredilir; Gezik'in hayaleti gizlenir.
- **İçeri:** pencereye Gezik'in kendi bırakma hedefi kaydedilir; konum ve değiştirici tuşlar bilindiği için bırakma noktasındaki satır vurgulanır. Pencere içi sürükleme sistemden geri gelirse (dışarı çıkıp geri girdi) aynı hedef mantığı kullanılır.

| Sistem | Dışarı | İçeri | Risk |
|---|---|---|---|
| Windows | `DoDragDrop` + `SHCreateDataObject` + `IDragSourceHelper` (Gezik'in çizdiği hayalet bitmap) | winit'in bırakma hedefi kapatılır (`with_drag_and_drop(false)`, Slint'in winit pencere özniteliği kancası üzerinden), `RegisterDragDrop` ile kendi `IDropTarget` | Düşük; kancanın varlığı planın ilk adımında doğrulanır |
| macOS | `NSView beginDraggingSessionWithItems:` (winit'in görünümü, `NSDraggingSource` nesnesiyle) | winit'in `NSView`'ı bırakma hedefi; konum/tuşlar için sınıfa yöntem eklenir | Yüksek; winit iç yapısına bağlı, burada denenemez |
| Linux X11 | XDND (`x11rb`, winit'in pencere kimliğiyle) | XDND | Orta; burada denenemez |
| Linux Wayland | `wl_data_device` + `wl_data_source`, winit'in `wl_display`'i paylaşılarak (`raw-window-handle`) | `wl_data_offer` | Yüksek; burada denenemez |

- 4b'nin ilk görevi her sistem için küçük bir fizibilite denemesidir. Windows sonucu burada doğrulanır; macOS/Linux kodu derlenir ve "denenmedi" olarak işaretlenir.
- **Yedek yol:** bir sistemde yol tıkanırsa o sistemde içeri bırakma winit'in konumsuz `DroppedFile` olayına düşer (her zaman bulunulan klasöre, varsayılan etkiyle) ve dışarı sürükleme yapılmaz; README ve takip notuna yazılır.

## 10. Ayarlar, durum, kısayollar, tema

### 10.1 `settings.toml` → `[files]`

```toml
[files]
confirm-trash = false     # ask before moving to the trash
copy-threads = "auto"     # auto | 1-16 (auto: SSD 6, HDD 1, network 4)
```

Geçersiz değer uyarı verir ve varsayılan kullanılır (mevcut uyarı kalıbı).

### 10.2 Yerel dosyalar

- `state.toml`: `[operations] panel-collapsed = false`.
- `pending-deletes`: anında silmede yarım kalan gizli yollar (5.5); düz metin, satır başına bir yol; boşalınca silinir.

### 10.3 Yeni kısayollar

| Eylem | Windows / Linux | macOS |
|---|---|---|
| `copy` / `cut` / `paste` | `mod+c` / `mod+x` / `mod+v` | aynı |
| `paste-move` | — | `mod+alt+v` |
| `trash` | `delete` | `mod+backspace` |
| `delete-permanently` | `shift+delete` | `mod+alt+backspace` |
| `rename` | `f2` | `enter` |
| `new-folder` | `mod+shift+n` | aynı |
| `duplicate` | — (yok; Explorer'da Ctrl+D silme demek) | `mod+d` |
| `undo` / `redo` | `mod+z` / `mod+y` | `mod+z` / `mod+shift+z` |

- Kısayol sistemi varsayılanı olmayan eylemleri destekleyecek şekilde genişletilir (`default_text` → `Option`).
- macOS'ta `rename = enter` yalnızca liste odaklıyken geçerlidir; açma ⌘↓ / çift tık ile yapılır (Finder gibi). Windows/Linux'ta Enter açmaya devam eder.
- Metin alanı (yerinde yeniden adlandırma, adres çubuğu) odaklıyken dosya kısayolları (`copy`, `paste`, `undo`, `trash`…) metin alanına bırakılır.
- `settings.toml` şablonuna yorum satırı olarak eklenir.

### 10.4 Tema

Yeni renkler: `progress`, `progress-paused`, `progress-error`, `drop-target`. Yerleşik temalarda tanımlanır; kullanıcı temalarında yoksa yerleşik temanın değeri kullanılır (mevcut kalıtım). Kesilmiş öğe saydamlığı sabit 0,5.

### 10.5 Erişilebilirlik

Panel satırları ve çakışma listesi erişilebilirlik rolleri ve adlarıyla (ilerleme çubuğu değeri, satır başına karar) sunulur; tüm düğmeler Tab ile odaklanır.

## 11. Performans

Ölçümler: Windows 11, sürüm derlemesi, NVMe; karşılaştırma aynı makinede Explorer'ın `Shell.Application` `CopyHere` / `InvokeVerb("delete")` çağrısıyla. Yeni betik `scripts/perf/ops.ps1` ağaçları üretir, iki yolu ölçer, temizler.

| Ölçüm | Hedef |
|---|---|
| Boşta bellek | Değişmez (≤ 7 MB) |
| 10.000 küçük dosya (4-64 KB) kopyalama | Explorer'dan ≥ 2 kat hızlı |
| Tek 4 GB dosya kopyalama | Explorer'ın ±%5'i |
| 50.000 dosyalık ağacı kalıcı silme | Öğe < 100 ms'de kaybolur; tamamı Explorer'dan ≥ 3 kat hızlı |
| 100.000 öğe ön tarama (sıcak önbellek) | ≤ ~1 sn |
| Kopyalama sırasında arayüz | Kaydırma CPU'su ve kare süresi işlem yokkenkiyle aynı (olaylar ≤ 10/s) |
| 10.000 çakışmalı liste | Açılış ≤ ~200 ms, akıcı kaydırma |

## 12. Test

### 12.1 Birim testleri (`gezik-core::ops`)

`ad (2)` üretimi (uzantılı, uzantısız, çift uzantı, klasör, mevcut sonek), hedef hesaplama, kendi içine kopyalama reddi, çakışma sınıflandırma (identical, dosya–klasör), karar uygulama, tersini çıkarma, yeniden adlandırma döngüsü çözümü, geçersiz ad denetimi (Windows ve Unix kuralları), disk kümesi kesişimi.

### 12.2 Bütünleşme testleri (`gezik-ops`, geçici klasörler)

Kopyala; taşı (aynı disk; farklı disk için ikinci yazılabilir sürücü yoksa test atlanır); çöpe at ve geri getir; kalıcı sil; anında silme ve `pending-deletes` ile açılış toparlaması (sahte çökme: dosya elle yazılır); iptal ve yarım dosya temizliği; duraklat/devam; çakışma kararları; geri al/yinele; değişmiş öğenin geri almada atlanması; kuyrukta kesişen ve kesişmeyen işler; öğe hatası (salt okunur/kilitli dosya) ve Retry failed; art arda hata duraklatması.

### 12.3 Platform testleri (`gezik-platform`)

Kopyalamanın ilerleme ve iptali, POSIX silme, disk türü/kimliği, çöp desteği her derlemede. Gerçek çöpe dokunan testler geçici dosyalarla. Pano ve sürükle-bırak testleri `#[ignore]` (kullanıcının panosunu bozar), elle çalıştırılır.

### 12.4 Elle (ekran) testleri

- Kısayollar ve menüler: tüm işlemler, Shell menüsündeki yönlendirilen komutlar, 7-Zip gibi eklentilerin çalışmaya devam etmesi.
- Pano: Gezik → Explorer, Explorer → Gezik (kopya ve kesme), kesilmiş öğelerin soluk görünmesi ve temizlenmesi.
- Çakışma listesi: tüm kararlar, identical, klasör birleştirme, dosya–klasör, Cancel operation sonrası Ctrl+Z.
- Panel: daraltma, kendiliğinden açılma, Start now, Details, Retry, görev çubuğu ilerlemesi, kapatma sorusu.
- Anında silme: süreç Görev Yöneticisi'nden öldürülüp yeniden açılınca toparlama.
- Geri alma: her işlem türü, değişmiş öğe, çöpsüz sürücü.
- Sürükle-bırak (4b): tüm hedefler, Shift/Ctrl, sağ tuş menüsü, sekme üstünde bekleme, sabitleme, Explorer'a/masaüstüne/e-postaya sürükleme, Explorer'dan bırakma.
- Test edilemeyenler: macOS, Linux (X11/Wayland), ağ kopması (elle kablo çekilerek denenebilir).

## 13. Bölme: 4a ve 4b

- **4a — Motor ve işlemler:** bölüm 3–8, 10–12 (sürükle-bırak hariç). Tek başına kullanılabilir.
- **4b — Sürükle-bırak:** bölüm 9; 4a'nın motorunu, `Task`'larını ve platform veri nesnelerini (pano için kurulan `SHCreateDataObject` yolu) kullanır.

## 14. Yol haritasındaki yeri

| # | Alt proje |
|---|---|
| 1 | Ayarlar + Tema sistemi — tamamlandı |
| 2 | Gezinme — tamamlandı |
| 3 | Görünüm — tamamlandı |
| **4** | **Dosya işlemleri (bu belge): 4a motor ve işlemler, 4b sürükle-bırak** |
| 5 | Toplu işlemler: toplu yeniden adlandırma, dönüştürme (resim, metin kodlaması/satır sonu, ffmpeg ile ses/video, kullanıcı komutu), arşiv oluşturma ve açma (zip, 7z, gzip/tar) |
| 6 | Etiketler |
| 7 | Taşınabilirlik |
| 8 | Bulut senkronu |
| 9 | Gelişmiş: arama, çift panel, arşivin içinde gezinme, Git, komut paleti |
