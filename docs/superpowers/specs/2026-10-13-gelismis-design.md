# Alt Proje 16: Gelişmiş — Tasarım

- **Tarih:** 2026-10-13
- **Durum:** Kapsam kullanıcı kararı (2026-10-13: arşivin içinde okuma ve yazma, Git durumu ve komutları, Android telefon (MTP) üç sistemde, adres çubuğunda `>` ile komut; ortak bir VFS katmanı; §2). Ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§18, her biri gerekçesiyle); gerçekten bakımcıya kalan dört soru §19'da. Spec onaylanınca plan önceden onaylıdır. Parçaların hiçbiri başlamadı.
- **Kapsam:** Gezik yol haritasının 16. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md` satır 116; `2026-10-12-ozellik-durumu.md` satır 12 "Arşivin içinde klasör gibi gezinme", 40 "Git", 44 "MTP / telefon", 53 "adres çubuğunda komut"; dördü de bugün 🔜 Planlı). Tek spec, yedi parça (16a–16g; §14).
- **Dayandığı:** `2026-10-05-toplu-islemler-design.md` (arşiv okuyucuları, yazıcı, `AddToArchiveTask`, 7-Zip indirmesi), `2026-10-04-dosya-islemleri-design.md` (motor, çakışmalar, geri alma), `2026-10-04-gezinme-design.md` (`Location`, geçmiş, oturum), `2026-10-09-arama-design.md` (`Scope`, sonuç listesi), `2026-10-10-sistem-butunlesmesi-design.md` (sürücüler, gvfs ile bağlanma, `WM_DEVICECHANGE`), `2026-10-12-etiketler-design.md` (satır başına yan veri: `folder_tags.rs`), `2026-10-12-tasinabilirlik-design.md` (14a şeması), `2026-10-13-yerellestirme-design.md` (`t!`/`tn!`/`tx!`, pot ve `tr.po` her PR'da).
- **Taban:** `master` bcb07fa. `Action::ALL` 132; menü kimlikleri 3199'a kadar ayrılmış (15'in 3000–3199'u boş kaldı; 16: **3200–3599**, `GEZIK_IDS_END = 4096` değişmez); denetim listeleri macOS 396'da (`checklists/l10n-overflow-macos.md`), Linux 285'te (`checklists/15-linux.md`) biter (16: macOS **397'den**, Linux **286'dan**). Açılış 32–38 ms, boşta 5,1 MB (900×600, `docs/performance.md`).
- **Rakip notları:** Arşiv içi yedi rakipte (Files, DO, FP, Far, FL, PF, TC; TC ve 7-Zip FM "arşivi klasör gibi", Far eklentiyle, DO ve FL yazarak da). Git üçünde (Files, FL, PF: durum işareti, sütun, add/commit/push/pull). MTP dördünde (OC, DO, PF, TC; Windows'ta hepsi WPD, macOS'ta PF ve OpenMTP libmtp). Panel altı komut satırı üçünde (Far, PF, TC).

## 1. Amaç

Gezik'te **arşivleri klasör gibi açmak ve değiştirmek**, **Git deposunun durumunu satırlarda görmek ve temel Git komutlarını menüden çalıştırmak**, **Android telefonu kenar çubuğunda görüp dosya alıp vermek** ve **adres çubuğuna `>` yazıp bulunulan klasörde komut çalıştırmak**. Bunlardan üçü (arşiv, telefon, ileride 18'in uzak bağlantıları) tek bir **VFS katmanına** oturur: diskte olmayan bir yer, gezinme, liste, önizleme ve işlem motoruna aynı yoldan girer.

Hafiflik bozulmaz: hiçbir özellik açılışta bir şey yüklemez, boşta hiçbir şey çalışmaz. Arşiv dizini, Git durumu, telefon oturumu yalnız ekranda gösterildiği sürece yaşar; gösterilmeyince bırakılır. Aygıt takılması olayla gelir (yoklama yok). Git ve komutlar sistemin programlarını çalıştırır; Gezik'e kütüphane eklenmez.

### Başarı ölçütleri

- **Arşivde gezinme (16a):** 10.000 girdili bir zip'e çift tık ≤ 150 ms'de ilk satırları gösterir; içinde sıralama, süzme, gruplama, ad araması, önizleme, Quick Look, kopyalayıp diske yapıştırma ve sürükleme çalışır. tar.gz/7z (katı) ilerlemeyle ve iptalle açılır. Şifreli arşiv parolayı bir kez sorar. Arşivdeki arşiv açılır. Arşivden çıkınca bellek açmadan önceki değere ±0,1 MB içinde döner.
- **Arşive yazma (16b):** zip, 7z ve tar ailesine dosya bırakma/yapıştırma, silme, yeniden adlandırma, yeni klasör; arşiv yanında geçici adla yeniden yazılır, doğrulanır, atomik yer değiştirir; eski arşiv çöpe gider, `Ctrl+Z` geri getirir. Yazma yarıda kesilirse (iptal, elektrik) eski arşiv bayt olarak aynı kalır (test).
- **Adres çubuğunda komut (16c):** `>git log -3` + Enter bulunulan klasörde çalışır; çıktı panelde akarak gelir, çıkış kodu görünür, `Stop` süreç ağacını bitirir; geçmiş ↑/↓ ile gelir. Enter'a basılmadan hiçbir şey çalışmaz.
- **Git durumu (16d):** bir depo klasörü gösterilince ≤ 300 ms'de (orta boy depo, 5.000 dosya) satırlarda değişti/eklendi/izlenmiyor/yok sayıldı/çakışma/hazırlandı işaretleri, Git sütunu, durum çubuğunda dal ve ↑↓; `git:modified` süzgeci. Depo dışındaki klasörlerde bir `git` süreci bile başlamaz.
- **Git komutları (16e):** hazırla/geri al, commit penceresi, push/pull/fetch, dal değiştirme, değişikliği at (çöpe, geri alınır), HEAD ile karşılaştır; kimlik doğrulama kullanıcının kendi yardımcılarıyla.
- **Telefon (16f, 16g):** Android telefon takılınca ≤ 2 s içinde kenar çubuğunda görünür (Windows WPD; macOS ve Linux libmtp); gezinme, telefondan/telefona kopyalama, silme, yeni klasör, yeniden adlandırma. Çıkarılınca satır ve sekmeler düzgün kapanır.
- **Hafiflik (hepsi):** boşta bellek `master`'a göre **0** (±0,02 MB; Linux'ta aygıt olayı için uyuyan tek iş parçacığı ≤ +0,05 MB), açılış ±0 (aygıt bildirimi ilk kareden sonra kurulur), klasör açma ve kaydırma (`stress.ps1`) `master` ±%2. Her parça exe'ye ≤ +262.144 B (§13).

### Kapsam dışı (bilerek)

- **Gömülü terminal öykünücüsü:** `>` komutu TTY vermez; tam ekran ve etkileşimli programlar (vim, top, less, parola soran ssh/sudo) çalışmaz ya da hemen biter. ANSI renkleri gösterilmez, silinir. Bunun için `Open in Terminal` (7a) var.
- **Yönetici olarak komut:** `>` komutları hep kullanıcının yetkisiyle; yükseltme yok.
- **Git'in ileri işleri:** clone/init, merge/rebase arayüzü, geçmiş/log görünümü, blame, stash arayüzü, alt modül yönetimi, LFS'e özgü işler, Gezik'in kendi parola penceresi (askpass), Gezik'le gelen git.
- **Arşivlerde:** içerik araması (yalnız ad, boyut, tarih), rar/iso/cab/7-Zip'e özgü biçimlere yazma, çok parçalı (`.7z.001`) arşive yazma, arşiv yorumu, kendiliğinden açılan (SFX) arşiv yazma.
- **iOS aygıtları:** iPhone MTP konuşmaz. Windows'ta WPD iPhone'un fotoğraflarını (DCIM) salt okunur gösterir; Gezik bunu aynı kodla bedavaya gösterir. macOS ve Linux'ta iPhone (ImageCaptureCore, libimobiledevice) ve uygulama belgelerine erişim yok.
- **Telefonda:** arama, çalma listeleri ve medya üst verisi, ADB, PTP kameralara yazma.
- **VFS öğelerini başka uygulamaya büyük boyutta vermek:** dış sürükleme ve sistem panosu gerçek dosya ister; küçük seçimler geçiciye çıkarılıp verilir (§4.6), büyükler için `Copy To…`.
- **Uzak bağlantılar (SFTP/WebDAV/S3):** 18. adım; VFS buna göre tasarlandı (§4.1).

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Arşiv (satır 12) | zip/7z/tar(.gz/.bz2/.xz/.zst) ve 7-Zip'in okuduğu her şey klasör gibi: gezinme, sıralama, süzme, arama, önizleme, Quick Look, kopyalama/sürükleme, çıkarma. Değiştirme: dosya ekleme (bırak/yapıştır), silme, yeniden adlandırma, yeni klasör; güvenli yeniden yazma (geçici, doğrula, atomik değiştir, mümkünse geri al). İç içe arşiv. Parolalı arşiv (sor) | Kullanıcı kararı 2026-10-13 |
| Git (satır 40) | Durum işaretleri (değişti, eklendi, izlenmiyor, yok sayıldı, çakışma, hazırlandı), Git sütunları, dal ve önde/geride, duruma göre süzme; menüden hazırla/geri al, commit, push, pull/fetch, belki dal değiştirme, değişikliği at (çöpe/geri alınır). Sistem `git`'i mi kütüphane mi: bu spec karar verir. Durum tembel ve iptal edilebilir, izleyiciyle yenilenir, boşta yoklama yok | Kullanıcı kararı |
| Telefon (satır 44) | Android MTP üç sistemde. Windows WPD; macOS/Linux libmtp (Linux sistem paketi çalışırken yüklenir; macOS istekle indirme ya da Homebrew: bu spec karar verir). Takılınca kenar çubuğunda (olayla). Kopyala, sil, yeni klasör, yeniden adlandır; küçük resim isteğe bağlı. iOS: ne mümkünse söylenir | Kullanıcı kararı |
| Adres çubuğunda komut (satır 53) | `>` + komut bulunulan klasörde sistem kabuğunda; çıktı panelde, çıkış kodu, iptal; terminal öykünücüsü değil; geçmiş; Enter'sız hiçbir şey çalışmaz; yönetici kapsam dışı | Kullanıcı kararı |
| Mimari | Arşiv ve MTP'nin (ve 18'in) paylaştığı bir VFS katmanı | Kullanıcı kararı |
| Hafiflik | Boşta hiçbir şey çalışmaz, özellikler istekle; parça başına ≤ +262.144 B | Ürün hedefi |

## 3. Bugünkü durum (araştırma)

### 3.1 Yer ve liste

- `gezik-core/src/nav.rs`: `enum Location { Path(PathBuf), Drives, Search(Box<SearchSpec>), Flat(PathBuf), Trash }`. `parent()`, `folder()` (gerçek klasör ya da `None`), `is_results()`. `Trash` diskte tek bir klasör olmayan bir yerin örneği: `navigation.rs` `list()` onu `LoadResult::Trash` ile ayrı okur. VFS de aynı yoldan girer.
- `gezik-core/src/lib.rs`: `Entry { name, is_dir, flags: u8, size, modified, created }`; `flags`'ın sekiz biti dolu (gizli, sistem, dört boyut biti, iki bulut biti). `list_dir(path)` yalnız disk.
- `gezik/src/navigation.rs`: `list(location, mode) -> LoadResult` arka plan iş parçacığında; `Listing::Files(PathBuf, Rc<Vec<Entry>>)` (`view/listing.rs`), satırın yolu `dir.join(name)`; `Navigator::entry_path(i) -> Option<(PathBuf, bool)>` işlemlerin, sürüklemenin, önizlemenin ve açmanın tek girişi.
- Oturum: `gezik-config/src/settings.rs` `tabs_toml`/`session_state` her `Location`'ı ayrı anahtarla yazar (`path`, `drives`, `trash`, `search`, `flat`); tanımadığı girdiyi `continue` ile atlar (eski Gezik yeni sekme türlerini sessizce bırakır). Sekme setleri `gezik/src/tab_sets.rs` `text_of`/`location_of` düz metin.
- Arama: `gezik-core/src/search.rs` `enum Scope { Folder(PathBuf), AllDrives }`.
- İzleme: `gezik/src/folder_watch.rs` (`notify`, yalnız gösterilen klasör, kendi iş parçacığında kurulur).
- Satır başına yan veri: `view/folder_tags.rs`, `view/folder_fields.rs` (bir iş parçacığı bütün klasörü okur, sonuç bir kez gelir, okunmayan satır soluk). Git işaretleri bu deseni izler.

### 3.2 Arşivler

- `gezik-batch/src/archive/`: saf Rust okuyucular (zip, 7z `sevenz-rust2`, tar ailesi, tek dosya `.gz/.xz/.bz2/.zst`, cab, iso, cpio, ar/deb; `rar` özelliği UnRAR ile), biçim `gezik_core::batch::archive::detect` ile ilk baytlardan. `trait ArchiveSource { fn list(..) -> Option<Vec<Entry>>; fn extract(dest, cx) }`: liste ve **bütünü** çıkarma; tek girdi çıkarma yok. Her ad `safe_join`'den geçer.
- `archive/write.rs`: zip, 7z, tar yazar (parola, düzey). `tasks/add.rs` `AddToArchiveTask`: arşivi geçici adla yeniden yazar (zip: girdiler ham kopya; 7z/tar: geçiciye aç, yeniden paketle, Unix kipi/tarih/sahip korunur), eski arşiv kenara, yenisi adını alır, eskisi çöpe: geri alma eskisini getirir. 16b'nin yazma yolu budur.
- `tasks/external.rs`: Gezik'in okumadığı biçimlerde 7-Zip (`7z x`), parola girdiden (komut satırında değil), iptalde süreç ağacı biter. `tools.rs`: 7-Zip/ffmpeg/pdfium bulma (ayar, indirme, PATH); indirmeler `gezik-core/src/batch/tools.rs` `MANIFEST`'te adres, boyut, SHA-256 ile; paketler `wenlar/gezik-tools` sürümlerinde, `scripts/tools/prepare*.ps1` üretir (README).
- `gezik/src/archives.rs`: Extract/Compress menüleri, parola soruları, araç indirme kutusu. `[archives] double-click = "system" | "extract-here"`.

### 3.3 İşlem motoru

`gezik-ops`: `trait Task { plan, run, resources, … }`; `PlanItem { source: Option<PathBuf>, target: Option<PathBuf>, facts, stage, … }`; `Outcome` (`Created`, `Placed`, `Moved`, `Trashed`, `Deleted`, …) geri almanın kaydı; `Resources { paths, work }` ile sürücü başına kuyruk (`engine.rs` `share_a_drive`). Çöpü olmayan sürücüde "kalıcı silinsin mi" yolu var. Hepsi `PathBuf`.

### 3.4 Süreç, aygıt, ağ

- `gezik-platform/src/process.rs` `ChildProcess`: pencere yok, Windows'ta iş nesnesi, Unix'te süreç grubu; `stdout_lines`, `wait_or_stop`, `kill_tree`. Git ve `>` komutları bunu kullanır.
- `gezik-platform/src/removal.rs`: `WM_DEVICECHANGE` (`DBT_DEVICEQUERYREMOVE`) zaten pencereye bağlı.
- `gezik/src/sidebar.rs`: sürücü imzası (`drive_signature`) **3 s'de bir** Slint zamanlayıcısıyla yoklanıyor (`DRIVE_POLL`). Bu adım bunu değiştirmez; aygıt olayları yeni yoklama eklemez (§10.5, §17).
- Linux'ta gvfs zaten kullanılıyor (`network.rs` `gio mount`, `drives.rs` `/run/user/<uid>/gvfs`). `audio/linux.rs` `libpulse-simple`'ı `dlopen` ile yükler: libmtp için örnek.

## 4. Ortak yapı: VFS

### 4.1 Türler (`gezik-core/src/vfs.rs`, yeni, saf)

```rust
pub enum Location { Path(PathBuf), Drives, Search(..), Flat(PathBuf), Trash, Vfs(VfsPath) }

pub struct VfsPath { pub root: VfsRoot, pub inner: String }   // '/' ile, başında '/' yok; "" = kök

pub enum VfsRoot {
    /// Bir arşiv: diskte (`Disk`) ya da başka bir VFS'in içinde (iç içe; telefondaki zip de).
    Archive(Container),
    /// MTP aygıtı: kalıcı kimlik (WPD PnP kimliği ya da libmtp seri numarası) ve görünen ad.
    Device { id: String, name: String },
    // 18: Remote { scheme, user, host, port }
}
pub enum Container { Disk(PathBuf), Inside(Box<VfsPath>) }
```

- `Location::parent()`: içte bir üst; arşiv kökünün üstü arşivin klasörü (`Location::Path`), arşiv seçili gelir; aygıt kökünün üstü `Drives`. `Location::folder()` VFS için **`None`**: gerçek klasör isteyen her özellik (terminal, `[[commands]]`, `>` komutu, klasör araması) kendiliğinden kapanır.
- `Caps` (bit kümesi): `WRITE`, `RENAME`, `MKDIR`, `DELETE`, `RANDOM_READ` (tek girdiyi ucuz okur: zip, depolanmış 7z değil), `THUMBS`, `TRASH` (hiçbirinde yok). Menüler ve eylemler buna bakar.

### 4.2 Yer sözdizimi

| Yer | Adres çubuğunda ve `Copy Path`'te | Neden |
|---|---|---|
| Arşiv | `C:\iş\yedek.zip\docs\2024` (macOS/Linux `/home/x/yedek.zip/docs`) | Arşiv dosya sisteminin içinde: yol diskteki yolun devamı. 7-Zip FM, TC ve Explorer'ın zip klasörü böyle; yazılan, yapıştırılan, sekme setine konan yol aynı |
| İç içe | `C:\iş\yedek.zip\eski\a.tar.gz\src` | Aynı kural her düzeyde |
| Telefon | `mtp://Pixel 8/Internal shared storage/DCIM` | Diskte yolu yok; 18'in `sftp://kişi@sunucu/…` biçimiyle aynı aile |

- **Çözüm** (`gezik_core::vfs::split_archive`, saf, testli; G/Ç çağıran verir): yazılan yol diskte yoksa soldan bileşen bileşen yürünür; **dosya olan** ve adı arşive benzeyen (`looks_like_archive`) ilk bileşen disk kısmını bitirir. Arşivin içinde aynı kural dizinle (iç içe). Var olan klasörün adı `x.zip` olsa da klasördür. Çözüm yükleme iş parçacığında, yalnız yazılan ya da geri yüklenen yolda (her gezinmede değil).
- Aygıt adı iki aygıtta aynıysa `mtp://Pixel 8 (2)/`. Kalıcı olan kimlik; ad yalnız gösterim.
- **Oturum (`state.toml`)**: yapılı, metin ayrıştırılmaz: `{ archive = "C:/iş/yedek.zip", inner = ["eski/a.tar.gz", "src"] }` (dizi: her düzeyin içi), `{ device = "<kimlik>", device-name = "Pixel 8", inner = "Internal shared storage/DCIM" }`. Eski Gezik bu girdileri atlar (bugünkü `continue`). Sekme setleri metin biçimini yazar (§4.2 tablosu).
- **Geri yükleme:** arşiv yoksa bugünkü `Gone` yolu (en yakın var olan klasör); aygıt takılı değilse `Drives` ve not "Pixel 8 is not connected".

### 4.3 `gezik-vfs` sandığı (yeni)

`gezik-core`, `gezik-i18n`, `gezik-platform`, `gezik-ops`, `gezik-batch`'e bağlı; `gezik`, `gezik-search` kullanır.

```rust
pub trait Vfs: Send + Sync {
    fn root(&self) -> &VfsRoot;
    fn caps(&self) -> Caps;
    fn list(&self, dir: &str, cx: &dyn Cx) -> io::Result<Vec<Entry>>;          // gezik_core::Entry
    /// Seçilen girdileri (klasörler içleriyle) `dest` altına yazar; katı/akış arşivde tek geçiş.
    fn stage(&self, items: &[String], dest: &Path, cx: &dyn Cx) -> io::Result<()>;
    /// Değişiklikler: arşivde tek yeniden yazma, telefonda tek tek.
    fn apply(&self, edits: &[Edit], cx: &dyn Cx) -> io::Result<Applied>;
    fn thumbnail(&self, _file: &str, _px: u32) -> Option<Vec<u8>> { None }
}
pub enum Edit { Put { from: PathBuf, to: String }, MakeDir(String), Delete(String), Rename { from: String, to: String } }
```

`Cx` ilerleme, iptal ve parola sorusu (bugünkü `ExtractCx` gibi). Uygulamalar: `archive.rs` (16a/16b), `mtp.rs` (16f/16g), 18'de `remote/`.

### 4.4 Bağlar (mount) ve ömür

`gezik/src/vfs_mounts.rs` (yeni, UI iş parçacığı): açık VFS'ler kök başına `Arc<dyn Vfs>`; sayaç = onu gösteren sekme sayısı (bütün bölmeler ve pencereler). Son sekme ayrılınca bırakılır: arşiv dizini, parola, telefon oturumu, geçici dosyalar. Önbellek yok (geri dönülünce zip'in merkezi dizini zaten hızlı; tar.gz yeniden taranır, bilinen sınır). Geçmişteki (Back) girdiler bağ tutmaz.

### 4.5 Geçici alan (staging)

- `std::env::temp_dir()/gezik-vfs-<pid>/<kök karması>/…`: önizleme, Quick Look, sistem uygulamasıyla açma, dış sürükleme ve iç içe arşiv için. İlk VFS kullanımında oluşturulur; o an ölü süreçlerin klasörleri silinir (açılışta değil). Bağ bırakılınca ve çıkışta silinir.
- Önizleme ve Quick Look kendiliğinden yalnız `[archives] preview-limit` (varsayılan 64 MB) altındaki dosyayı çıkarır; büyükte önizlemede `Extract to Preview` düğmesi. Telefonda da aynı sınır.

### 4.6 VFS ile disk arasında işlemler

| Yön | Nasıl | Geri alma |
|---|---|---|
| VFS → disk (kopyala/yapıştır, `Copy To…`, Gezik içinde sürükle) | `VfsCopyOutTask`: `stage` ile hedef klasörde motorun geçici adına, sonra bugünkü yerleştirme (`tasks/place.rs`) ve çakışma soruları | `Created` (bugünkü gibi) |
| Disk → VFS (bırak, yapıştır) | `VfsEditTask` (`Edit::Put`); arşivde tek yeniden yazma, telefonda tek tek | Arşiv: eski arşiv çöpte (§6.3); telefon: `Outcome::VfsCreated` → silinir |
| VFS içinde sil/yeniden adlandır/yeni klasör | `VfsEditTask` | Arşiv: aynı; telefon: silme geri alınamaz (önceden söylenir) |
| VFS → VFS (telefondaki dosyayı arşive) | Önce geçiciye `stage`, sonra `Put` | Hedefinki |
| Taşıma (VFS'ten) | Kopyala, başarılıysa kaynakta `Delete` | Hedefinki; kaynak arşivse eski arşiv çöpte |

- Motor: `Resources`'a `vfs_roots: Vec<String>` (kök anahtarı) eklenir; `share_a_drive` bunları da karşılaştırır: aynı arşive ya da aynı telefona iki iş sırayla gider. Arşivin anahtarı arşiv dosyasının disk yolu (sürücüsünün kuyruğuna da girer).
- `Outcome`'a `VfsCreated { at: VfsPath }`, `VfsRenamed { from, to }` eklenir; `inverse.rs` bunları telefonda geri alır.
- **Gezik dışına:** sistem panosu ve dış sürükleme gerçek dosya ister. Seçimin toplamı ≤ 256 MB ise sürükleme başlarken (ilerleme halkasıyla) geçiciye çıkarılır, sonra bugünkü sürükleme verilir; büyükse sürükleme açılmaz ve durum çubuğu "Use Copy To… for large selections" der. Kopyala, Gezik'in kendi panosuna (süreç içi) ve sistem panosuna metin olarak yolları yazar; Explorer/Finder'a yapıştırma yok (bilinen sınır).

### 4.7 VFS içinde ne kapalı

`Navigator::entry_path(i)` VFS satırında **`None`** döner; yeni `entry_item(i) -> Option<Item>` (`Item::Disk(PathBuf, bool)` | `Item::Vfs(VfsPath, bool)`). VFS'i bilen özellik `entry_item`'a geçer; bilmeyen kendiliğinden kapanır, menüde gri değil hiç görünmez.

| Özellik | Arşivde | Telefonda | Neden |
|---|---|---|---|
| Etiket, puan, yorum, vurgulama kuralının etiket koşulu | Yok | Yok | ADS/xattr ve merkezi kayıt gerçek yol ister |
| Klasör boyutları | Var (dizinden, anında) | Yok | — |
| Özellikler (`fields`: EXIF, ses, PDF) | Yok | Yok | Her dosyayı çıkarmak gerekir |
| Ad araması | Var (dizinde) | Yok | Telefonda her klasör bir istek |
| İçerik araması, düz görünüm, klasör karşılaştırma, sağlama, dönüştürme, toplu yeniden adlandırma, `[[commands]]`, bağlantı/şablon | Yok | Yok | Gerçek dosya ister |
| Önizleme, Quick Look, sistem uygulamasıyla aç | Geçiciden | Geçiciden | §4.5 |
| Izgara küçük resimleri | `RANDOM_READ` ise görünen öğeler, tanesi ≤ 8 MB | Aygıtın verdiği küçük resim | Katı/akış arşivde her resim baştan açmak demek |
| Windows Kabuk menüsü, Özellikler penceresi, yönetici olarak yeniden dene | Yok | Yok | Kabuk gerçek yol ister |
| İzleme | Arşiv dosyası (üst klasörü, ada süzülü) | Yok (F5 ve kendi işlemlerimizden sonra) | MTP olayları güvenilmez |
| Git, `>` komutu, terminal | Yok | Yok | `folder()` `None` |

### 4.8 Hafiflik kuralı (her parça)

- Açılışta yüklenen yok: VFS sandığı kod olarak var, nesne yok; WPD COM, libmtp, git, kabuk ancak kullanıcı bir şeye bakınca.
- UI iş parçacığı arşiv okumaz, süreç beklemez, aygıtla konuşmaz: hepsi iş parçacığında, sonuç olayla.
- Zamanlayıcı yok. Git'in ve arşivin izlemesi `notify` (işletim sistemi olayı); aygıt takılması işletim sistemi olayı (§10.5).
- Bırakılan her şey bellekten çıkar: arşivden, depodan, telefondan çıkınca `measure.ps1` ile ±0,1 MB.

## 5. Arşivde gezinme (16a)

### 5.1 Açma

- `[archives] double-click`'e `"browse"` eklenir ve **varsayılan olur** (§19 soru 1): çift tık ya da Enter arşive girer. `"system"` ve `"extract-here"` kalır. Sağ tıkta her zaman `Open as Folder` (eylem `browse-archive`) ve `Open with System App`.
- Adres çubuğuna arşivin içine giden yol yazmak (§4.2), geçmiş, sekme seti, oturum aynı yere götürür.
- Hangi arşivler: Gezik'in okuduğu her biçim (§3.2) ve 7-Zip'in okuduğu her şey (dmg, wim, vhd, msi, chm, udf, …). 7-Zip yoksa bugünkü indirme kutusu.

### 5.2 Dizin (`gezik-vfs/src/archive/index.rs`)

- Açılışta bir geçiş: bütün girdiler tek bir ad yığınında (`String` + konumlar), klasör ağacı ad sırasıyla; arşivde klasör girdisi olmayan ara klasörler türetilir. 100.000 girdi ≈ 6 MB.
- **Kaynaklar:** zip merkezi dizin (açmadan); 7z başlık; tar ailesi ve tek dosya akış: bir kez açarak (ilerleme, iptal; 1 GB tar.gz ≈ birkaç saniye); rar UnRAR listesi; cab/iso/cpio/ar okuyucular; 7-Zip biçimleri `7z l -slt -ba` (parola girdiden).
- `ArchiveSource`'a `extract_some(&self, names: &[String], dest, cx)` eklenir (yalnız seçilenler `safe_join` ile; zip ve 7z rastgele, tar akışta süzerek, 7-Zip `7z x -i@<liste>`). Bugünkü `extract` aynı kalır.
- Girdi: `Entry` (ad, klasör mü, boyut, tarih) + dizinde `packed`, `encrypted`. Yeni sütun **Packed** (yalnız arşivde seçilebilir; `gezik-core/src/columns.rs`); şifreli girdide kilit rozeti.

### 5.3 Parola

Başlığı şifreli 7z/rar listelerken, öbürleri ilk okumada sorar (bugünkü `password_text` sorusu). Parola bağın ömrü boyunca bellekte; diske, oturuma, günlüğe yazılmaz; yanlışsa yeniden sorar; `Cancel` gezinmeyi bırakır.

### 5.4 İç içe

İçteki arşiv bütün olarak geçiciye çıkarılır (ilerleme, iptal), sonra disk arşivi gibi açılır; kökü `Container::Inside`. Derinlik ≤ 8. Telefondaki arşiv de aynı yoldan.

### 5.5 Arama

`Scope`'a `Vfs(VfsPath)` eklenir. Arşivde arama dizinde: ad deseni, boyut, tarih (içerik ölçütü kapalı, arama penceresi bunu söyler). Sonuç satırları `ResultSet`'te VFS yoluyla; açmak, önizlemek, kopyalamak §4.6 ile.

### 5.6 Değişiklik ve izleme

Arşiv dosyası başka bir programla değişirse (izleyici, üst klasörde ada süzülü) dizin yeniden okunur; seçim adla korunur. Arşiv silinirse sekme bugünkü `Gone` yoluna gider.

## 6. Arşive yazma (16b)

### 6.1 Ne yazılır

- **Yazılır:** zip, 7z, tar, tar.gz, tar.bz2, tar.xz, tar.zst (Gezik'in yazıcısı). **Salt okunur:** rar, iso, cab, cpio, ar/deb, tek dosya `.gz` vb., çok parçalı, 7-Zip'e özgü biçimler. Salt okunur arşivde bırakma ve yapıştırma kapalı; durum çubuğu "This kind of archive can only be read".
- İşlemler: dosya/klasör bırakma ve yapıştırma (`Put`), silme (`Delete`; arşivde çöp yok: soru "Delete from the archive? You can undo this."), yeniden adlandırma (F2, `Rename`), yeni klasör (`MakeDir`). Taşıma içeri: kopya + kaynağı çöpe.

### 6.2 Yeniden yazma (`gezik-vfs/src/archive/edit.rs`, `AddToArchiveTask`'ın genellemesi)

1. **Yer:** arşivin yanında boş alan arşiv boyutu + eklenenler kadar mı (yoksa başlamadan söyler).
2. **Yaz:** `.<ad>.gezik-<pid>.tmp` (aynı klasör: yer değiştirme atomik). zip: değişmeyen girdiler ham kopya (hızlı); 7z ve tar: bugünkü gibi geçiciye aç, düzenle, yeniden paketle (katı 7z'de bütün arşiv; süre tahmini soruda). Parolalı arşivde yeni girdiler aynı parolayla; adları şifreli 7z öyle kalır.
3. **Doğrula:** yeni arşiv yeniden açılır, dizini beklenen ağaçla (ad, boyut, klasör) karşılaştırılır; yeni ve değişen girdiler baştan sona okunup CRC/özetle denetlenir (değişmeyen zip girdileri ham kopyalandığı için onların CRC'si zaten aynı). Uymazsa geçici silinir, eski arşiv dokunulmamış.
4. **Değiştir:** bugünkü sıra: eski arşiv kenara (`.gezik-old` adı), yeni adını alır (`rename`, atomik), eski çöpe. Çöpü olmayan sürücüde bugünkü "kalıcı" sorusu ("The old archive cannot be kept for undo").
5. **Geri alma:** `Ctrl+Z` eski arşivi çöpten geri getirir, yenisini çöpe atar (bugünkü `Trashed` + `Created`).

İptal ya da hata 1–3 arasında: yalnız geçici silinir. 4'te hata: kenardaki eski arşiv adına geri döner (bugünkü `keep_aside` yolu).

### 6.3 İç içe yazma

İçteki arşiv geçicide yeniden yazılır, sonra dıştakine `Put` olarak tek bir dış yeniden yazmayla konur. Geri alma dış arşivin eski hali (tek adım).

### 6.4 İçinden açılan dosyayı geri koyma

Arşivdeki dosya sistem uygulamasıyla açılınca (§4.5 geçiciden) Gezik o geçici dosyayı izler (bağın `notify`'ı; yoklama yok). Değişince sekmenin üstünde şerit: "report.docx changed. **Update in Archive** · Ignore". `Update in Archive` bir `Put` (§6.2). Bağ bırakılırken değişmiş ve geri konmamış dosya varsa sorar: "Update in archive / Discard / Keep a copy in Downloads".

## 7. Adres çubuğunda komut (16c)

### 7.1 Kullanım

- Adres çubuğunda metin `>` ile başlarsa (baştaki boşluklar atlanır) komut kipine geçer: öneri listesi komut geçmişini gösterir, ↑/↓ geçmişte gezer. **Enter** çalıştırır; yapıştırmak, geçmişten seçmek, oturum ya da sekme seti hiçbir zaman çalıştırmaz.
- Eylem `run-command` (palette; varsayılan tuşu yok) adres çubuğunu `>` ile açar. `toggle-command-output` paneli açıp kapar.
- Çalışma klasörü: `Location::folder()`; yoksa (This PC, Çöp, VFS) çalışmaz, durum çubuğu "Commands run in a folder on disk".
- Seçim ortam değişkeniyle verilir (metin değiştirilmez: `{`, `$`, `%` kabukların kendi sözdizimi): `GEZIK_FILE` (odaktaki öğenin tam yolu), `GEZIK_FILES` (seçilenler, satır başına bir yol), `GEZIK_DIR`.

### 7.2 Kabuk

| Sistem | Varsayılan | Çağrı |
|---|---|---|
| Windows | `cmd` (§19 soru 3) | `cmd.exe /d /s /c "<metin>"` |
| Windows, `shell = "powershell"`/`"pwsh"` | — | `-NoLogo -NoProfile -NonInteractive -EncodedCommand <base64 UTF-16>`, başına `[Console]::OutputEncoding=[Text.UTF8Encoding]::new()` |
| macOS, Linux | `$SHELL`, yoksa `/bin/sh` | `<kabuk> -c '<metin>'` |
| Hepsi, `shell = "<yol>"` | — | `<yol> <shell-args…> <metin>` |

Ortam: `TERM=dumb`, `NO_COLOR=1`, `CLICOLOR=0`, `PAGER=cat`, `GIT_PAGER=cat`, `GIT_TERMINAL_PROMPT=0`; girdi boş (EOF): girdi bekleyen program takılmaz, biter. Süreç `ChildProcess` ile (pencere yok, iş nesnesi/süreç grubu).

### 7.3 Çıktı paneli (`ui/widgets/command-output.slint`, `gezik/src/command_line.rs`)

- Bölmenin listesinin altında, boyu sürüklenir, bölme başına bir panel. Başlık: komut, klasör, çalışırken süre, bitince çıkış kodu (0 yeşil, başkası kırmızı) ve süre. Düğmeler: `Stop` (süreç ağacı), `Run Again`, `Copy Output`, `Close` (Esc).
- stdout ve stderr geliş sırasıyla tek metin; stderr satırları uyarı rengiyle. Eş aralıklı yazı. ANSI kaçışları silinir. Satırlar UTF-8; geçersizse Windows'ta OEM kod sayfası (`GetOEMCP`) ile çözülür.
- En çok 2 MB tutulur; aşılınca baştan atılır, üstte "Earlier output dropped". Arayüz güncellemesi en çok saniyede 30 kez (bugünkü kare sınırı).
- Çalışan komut varken yenisi: "A command is still running. Stop it?". Bölme kapanınca komut biter.
- Bitince klasör zaten izleyiciyle yenilenir; ek olarak bir kez yenilenir.

### 7.4 Geçmiş

`state.toml` `[command-line] history` (yerel, eşitlenmez), son 100, tekrarsız. Boşlukla başlayan komut kaydedilmez (kabukların `ignorespace` alışkanlığı; parola içeren komut için). `[history] remember = false` ve `clear-history` komut geçmişini de kapsar.

## 8. Git durumu (16d)

### 8.1 Sistem `git`'i mi kütüphane mi: sistem `git`'i

| | Sistem `git` (CLI) | gitoxide (`gix`) | libgit2 (`git2`) |
|---|---|---|---|
| Exe | ≈ +0 (yalnız ayrıştırıcı) | durum için ≈ +2–4 MB | ≈ +1,5 MB C kodu |
| Push/pull kimliği | Kullanıcının yardımcıları (Git Credential Manager, macOS Anahtar Zinciri, libsecret), ssh-agent, `~/.ssh/config`: olduğu gibi | Push eksik; kimlik kendi yolu | libssh2 + OpenSSL (Linux'ta), yardımcılar eksik; TLS kodu taşımamak kararına (2026-10-06) ters |
| Uyum | fsmonitor, untracked cache, `core.*`, hook'lar, `safe.directory`, sparse, worktree: git'in kendisi | Kısmi | Kısmi, sürüm kayması |
| Maliyet | Her yenilemede bir süreç (Windows ≈ 20–40 ms) | Süreç yok | Süreç yok |

**Karar: sistem `git`'i.** Bütçe (parça başına 256 KB) kütüphaneyi tek başına dışarıda bırakır; push/pull kimliği CLI'da sıfır kodla doğru çalışır; geliştiricinin zaten kurulu git'i var. Süreç maliyeti yalnız depo gösterilirken ve olay geldiğinde.

- **Bulma:** `[git] program` (tam yol), yoksa PATH. macOS'ta `/usr/bin/git` bir kabuktur: Command Line Tools yoksa kurulum penceresi açar. Gezik onu yalnız `xcode-select -p` başarılıysa kullanır; yoksa Homebrew (`/opt/homebrew/bin/git`, `/usr/local/bin/git`); hiçbiri yoksa durum çubuğunda bir kez "Install Git to see repository status" (bağlantı). Bulma ilk `.git` görüldüğünde, iş parçacığında, bir kez (dosya değişim zamanıyla önbellek, `tools.rs` `Answers` gibi).

### 8.2 Tembel ve iptal edilebilir

- **Depo bulma:** klasör yüklenirken yükleme iş parçacığında üst klasörlere `.git` (klasör ya da dosya: worktree, alt modül) bakılır; ev klasörünün üstünde ve sürücü kökünde durur; ağ sürücüsünde bakılmaz (`[git] network = false`, varsayılan). Depo yoksa hiçbir şey olmaz.
- **Durum:** `git --no-optional-locks -c core.quotepath=off status --porcelain=v2 -z --branch --untracked-files=normal --ignored=matching -- <gösterilen klasör>` (yok sayılanlar `[git] ignored = true` ise). `--no-optional-locks`: kullanıcının kendi git komutlarıyla `index.lock` yarışı olmaz. Tek süreç, iş parçacığında; başka yere gidilince ya da yenisi gelince `kill_tree`. `[git] timeout` (varsayılan 10 s) aşılırsa durur, not "Git status took too long" (büyük depoda `untracked-files=no` önerisiyle).
- **Yenileme:** gösterilen klasörün izleyicisi (bugünkü) ve deponun `.git` klasörünün izleyicisi (`HEAD`, `index`, `refs/` değişimi; özyinelemesiz, `notify`) olay verince 300 ms birleştirmeyle bir koşu; koşu sürerken gelen olaylar en çok bir koşu daha doğurur. Depo dışına çıkınca `.git` izlemesi kalkar. Zamanlayıcı yok.
- Bölmeler ve pencereler aynı depoyu gösteriyorsa sonuç paylaşılır (depo kökü anahtarı).

### 8.3 İşaretler

`gezik-core/src/git.rs` (yeni, saf): porcelain v2 ayrıştırıcısı, `GitMark` ve klasör toplaması.

| İşaret | Kaynak (XY) | Satırda | Sütun |
|---|---|---|---|
| Çakışma | `u` satırı | kırmızı `!` rozeti | Conflict |
| Değişti | `Y = M/T` | turuncu nokta | Modified |
| Hazırlandı | `X ≠ .` (Y temiz) | yeşil nokta | Staged |
| Hazırlandı + değişti | ikisi | yarım nokta | Staged, modified |
| Eklendi | `X = A` | yeşil `+` | Added |
| Yeniden adlandırıldı | `2` satırı | yeşil ok | Renamed |
| İzlenmiyor | `?` | gri `?` | Untracked |
| Yok sayıldı | `!` | satır soluk (gizli dosya gibi) | Ignored |

- **Klasör:** içindekilerin en önemlisi (çakışma > değişti > hazırlandı > eklendi > izlenmiyor); bütünü yok sayılan klasör soluk.
- **Silinenler** diskte yok, satırı olmaz: durum çubuğu "3 deleted" der; tıklayınca adlarını listeler.
- Saklama: `view/folder_git.rs` (yeni; `folder_tags.rs` deseni): ad → `GitMark` (`u8`), yalnız işaretli adlar. `Entry::flags`'ta yer yok ve gerekmez.
- Renkler temaya yeni anahtarlar: `git-modified`, `git-staged`, `git-untracked`, `git-conflict` (14c'nin tema anahtar listesi testi güncellenir).

### 8.4 Sütun, süzgeç, gruplama, durum çubuğu

- Sütun **Git** (`gezik-core/src/columns.rs`): metin (Modified, Staged, …), sıralanır.
- Süzgeç kelimeleri (`gezik-core/src/git_filter.rs`, `tag:` gibi): `git:changed` (herhangi bir değişiklik), `git:modified`, `git:staged`, `git:untracked`, `git:conflict`, `git:ignored`; `-git:ignored` dışarıda bırakır. Gruplama `GroupBy::Git`.
- Durum çubuğu (depo içindeyken): `⎇ main ↑2 ↓1 · 5 changed` (`# branch.head`, `# branch.ab`; ağ yok, son fetch'e göre). Ayrık HEAD: `⎇ (a1b2c3d)`. Tıklayınca 16e'nin Git menüsü.
- Arşivde ve telefonda Git yok.

## 9. Git komutları (16e)

### 9.1 Komutlar

Satır sağ tık menüsünde `Git ▸` alt menüsü (yalnız depo içinde), durum çubuğunun menüsü, palet, macOS menü çubuğunda `Go`'nun yanında yeni bir **Git** menüsü.

| Eylem | Başlık | Ne çalışır |
|---|---|---|
| `git-stage` | Stage | `git add -- <seçilenler>` |
| `git-unstage` | Unstage | `git restore --staged -- <seçilenler>` |
| `git-commit` | Commit… | Pencere (§9.2), `git commit -F -` (ileti girdiden) |
| `git-push` | Push | `git push` |
| `git-pull` | Pull | `git pull` (kullanıcının `pull.rebase`/`ff` ayarıyla) |
| `git-fetch` | Fetch | `git fetch` |
| `git-switch-branch` | Switch Branch ▸ | yerel dallar (`git for-each-ref refs/heads`), en çok 50, şimdiki işaretli; `git switch <dal>` |
| `git-discard` | Discard Changes… | §9.3 |
| `git-compare-head` | Compare with HEAD | `git show HEAD:<yol>` geçiciye, 11'in karşılaştırma aracı (yerleşik ya da dış) |

Hiçbirinin varsayılan tuşu yok. Yol listeleri `--` sonrası ve 32 KB'ı geçerse `--pathspec-from-file=-` ile girdiden.

### 9.2 Commit penceresi (`ui/widgets/git-commit.slint`)

İleti alanı (çok satır, ilk satır 72 karakterde uyarı çizgisi), hazırlanan dosyaların listesi (salt okunur), `Stage all changes first` (`git commit -a`) kutusu, `Commit`. Hook'lar (`pre-commit`) çalışır; çıktıları ve hatası 16c'nin panelinde. Boş ileti ya da hazırlanmış dosya yokken düğme kapalı. İleti taslağı pencere kapanınca bellekte kalır (oturum boyunca).

### 9.3 Değişikliği atmak, geri alınabilir

- Soru: "Discard changes to 3 files? They go to the trash first, so you can undo."
- İzlenen ve değişmiş dosya: önce **çöpe** (bugünkü `TrashTask`), sonra `git restore --worktree -- <yol>` dizindeki halini yazar. İzlenmeyen dosya: yalnız çöpe. Hazırlanmış değişiklik dokunulmaz (önce Unstage).
- Geri alma: çöpteki asıl dosya geri gelir, git'in yazdığının üstüne (soru sormadan; kendi yazdığımız). Tek iş, tek `Ctrl+Z`.
- Çöpü olmayan sürücüde komut kapalı (geri alınamaz atma yok).

### 9.4 Çalıştırma ve kimlik

- Hepsi 16c'nin çalıştırıcısıyla (`command_line.rs`): kısa olanlar (stage, unstage, switch) yalnız hata olursa paneli açar; commit, push, pull, fetch paneli açar ve çıktıyı akıtır; `Stop` süreci bitirir. Bitince durum yenilenir (§8.2).
- `GIT_TERMINAL_PROMPT=0`: git terminalde parola sormaz, takılmaz. Kimlik: Git Credential Manager (Windows'ta kendi penceresi), macOS Anahtar Zinciri, libsecret, ssh-agent. Kimlik hatası ("Authentication failed", "Permission denied (publickey)") görülünce panelde `Open in Terminal` düğmesi aynı klasörde terminali açar (7a) ve komutu panoya koyar.
- Aynı depoda aynı anda tek Git komutu (kuyruk); durum koşusu komut sürerken bekler.

## 10. Telefon (MTP) (16f, 16g)

### 10.1 Ortak

- Kök listesi: aygıtın depoları (`Internal shared storage`, `SD card`) klasör olarak. Yer `mtp://<ad>/<depo>/…` (§4.2).
- `gezik-platform/src/mtp/mod.rs`: sistemden bağımsız arayüz (`devices()`, `open(id) -> Session`, `Session::{storages, list, read_to, put, mkdir, delete, rename, thumbnail}`); `gezik-vfs/src/mtp.rs` bunu `Vfs`'e uyarlar. Nesne kimliği → ad yolu önbelleği (`IdCache`) bağ boyunca; liste her açılışta tazelenir.
- Aynı klasörde aynı adlı iki dosya (MTP buna izin verir): ikisi de görünür, ikincisi `ad (2)` biçiminde gösterilir; işlem nesne kimliğiyle yapılır.
- Tek oturum, tek işçi: aygıta işler sırayla (`Resources::vfs_roots`, `Work` tek iş parçacığı).
- **Üzerine yazma:** MTP'de yerinde yazma yok. Aygıt yeniden adlandırmayı destekliyorsa yeni dosya `ad.gezik-tmp` olarak yazılır, eskisi silinir, yenisi adlandırılır; desteklemiyorsa soru "The old file is deleted before the new one is written".
- **Silme kalıcı** (telefonda Gezik'in çöpü yok): bugünkü "kalıcı silinsin mi" sorusu, "cannot be undone".
- Tarih: telefondan kopyalanan dosyaya aygıtın değiştirme tarihi yazılır; telefona kopyalananınki aygıtın kararı (çoğu "şimdi").
- Küçük resim: ızgara görünümünde aygıt veriyorsa (WPD `WPD_RESOURCE_THUMBNAIL`, libmtp `LIBMTP_Get_Thumbnail`); vermiyorsa simge. Tam resim indirilip küçültülmez.
- Çıkarılınca: o aygıttaki sekmeler `Drives`'a gider, not "Pixel 8 was disconnected"; süren iş hata verir (yarım dosya hedefte geçici adla kalmaz: motorun bugünkü temizliği).
- Telefon kilitliyse ya da "File transfer" kipinde değilse liste boş gelir: not "Unlock the phone and choose File transfer in its USB notification" (Android'in kendi davranışı).

### 10.2 Windows: WPD (16f)

- `gezik-platform/src/mtp/windows.rs`: `IPortableDeviceManager` (aygıtlar), `IPortableDevice` (oturum), `IPortableDeviceContent::EnumObjects`, özellikler (`WPD_OBJECT_ORIGINAL_FILE_NAME`, `WPD_OBJECT_SIZE`, `WPD_OBJECT_DATE_MODIFIED`, `WPD_OBJECT_CONTENT_TYPE`), `IPortableDeviceResources::GetStream` (okuma), `CreateObjectWithPropertiesAndData` (yazma, boyut önceden), `CreateObjectWithPropertiesOnly` (klasör), `Delete`, `IPortableDeviceProperties::SetValues` (yeniden adlandırma; `GetPropertyAttributes` ile yazılabilirlik). `windows` sandığının `Win32_Devices_PortableDevices` özelliği. COM yalnız telefon açılınca, iş parçacığında (MTA) başlatılır.
- **Algılama, WPD yüklemeden:** `CM_Get_Device_Interface_ListW(GUID_DEVINTERFACE_WPD)` ve `CM_Get_Device_Interface_PropertyW(DEVPKEY_Device_FriendlyName)` (cfgmgr32, zaten yüklü) ile ad ve kimlik. Olay: ana pencerenin `WM_DEVICECHANGE`'ine `RegisterDeviceNotificationW(DEV_BROADCAST_DEVICEINTERFACE, GUID_DEVINTERFACE_WPD)` (`removal.rs`'nin yanına), ilk kareden sonra. `DBT_DEVICEARRIVAL`/`REMOVECOMPLETE` gelince liste iş parçacığında yeniden okunur.
- WPD bazı USB bellekleri ve kameraları da aygıt diye verir: disk harfi olan aygıt (`WPD_DEVICE_TYPE` ve `WPD_FUNCTIONAL_CATEGORY_STORAGE` + sürücü harfi) atlanır, sürücü olarak zaten görünür. iPhone ve kameralar salt okunur görünür (§1 Kapsam dışı).

### 10.3 macOS: libmtp (16g)

- **Edinme:** önce kullanıcının kurduğu (`/opt/homebrew/lib/libmtp.9.dylib`, `/usr/local/lib/libmtp.9.dylib`, `[devices] libmtp` yolu), yoksa **istekle indirme**: yeni araç `Tool::Libmtp` (`MANIFEST`'te adres, boyut, SHA-256), paket `libmtp-<sürüm>-macos-universal.tar.xz` (`libmtp.9.dylib` + `libusb-1.0.0.dylib`, `install_name` `@loader_path`, LGPL metinleri), `wenlar/gezik-tools`'ta; `scripts/tools/prepare-libmtp.sh` bir Mac'te (bakımcının MacBook'u ya da elle başlatılan bir macOS iş akışı) kaynaktan derler, kaynak arşivlerin SHA-256'sı `sources.sha256`'da (§19 soru 4). İndirme kutusu bugünkü `tool_offer`.
- Yükleme `dlopen` ile, yalnız telefon açılınca (pdfium gibi). Kullanılan işlevler küçük bir tabloda (`LIBMTP_Init`, `Detect_Raw_Devices`, `Open_Raw_Device_Uncached`, `Get_Storage`, `Get_Files_And_Folders`, `Get_File_To_Handler`, `Send_File_From_Handler`, `Create_Folder`, `Delete_Object`, `Set_File_Name`/`Set_Folder_Name`, `Get_Thumbnail`, `Release_Device`); sürüm `LIBMTP_VERSION_STRING` ≥ 1.1.17 (`Open_Raw_Device_Uncached`).
- **Algılama:** IOKit `IOServiceAddMatchingNotification(kIOFirstMatchNotification / kIOTerminatedNotification, IOUSBHostInterface)`, bildirim kapısı ana çalışma döngüsünde (winit'in `CFRunLoop`'u), ilk kareden sonra. Arayüz sınıfı 6/1/1 (PTP/MTP) olan aygıt kenar çubuğunda görünür (libmtp yüklenmeden: ad IORegistry'den). Sınıfı 255 olan eski aygıtlar için kenar çubuğu sağ tık `Look for Phones` libmtp'nin taramasını çalıştırır.
- **Aygıtı başka uygulama tutuyorsa** (Image Capture/`ptpcamerad`, Android File Transfer, OpenMTP): açma hatası, not "Another app is using the phone (Image Capture, Android File Transfer). Quit it and try again."

### 10.4 Linux: libmtp sistem paketi (16g)

- `dlopen("libmtp.so.9")` (Debian/Ubuntu `libmtp9`, Fedora/Arch `libmtp`), yalnız telefon açılınca. Yoksa not "Install libmtp (e.g. sudo apt install libmtp9)". İzin yoksa (paketin udev kuralları) "No permission to open the phone (libmtp's udev rules)".
- **gvfs aygıtı tuttuysa:** GNOME/Cinnamon/MATE telefonu gvfs-mtp ile bağlamış olabilir; o zaman libmtp aygıtı açamaz. Gezik önce `/run/user/<uid>/gvfs/mtp:host=…` bağını arar; varsa sekme oraya, **düz yol** olarak gider (bütün dosya işlemleri FUSE ile, VFS gerekmez; 9'un gvfs kodu). Yoksa libmtp. Gezik aygıtı tutarken gvfs bağlayamaz; bağ bırakılınca aygıt da bırakılır.
- **Algılama:** `NETLINK_KOBJECT_UEVENT` soketi, `usb` alt sisteminin `add`/`remove` olayları; soketi uyuyan tek bir iş parçacığı bekler (CPU 0, ≤ +0,05 MB), ilk kareden sonra. Arayüz sınıfı `/sys/bus/usb/devices/*/bInterfaceClass`'tan (6) okunur; libmtp yüklenmez.

### 10.5 Kenar çubuğu

Telefonlar sürücülerin bölümünde, telefon simgesiyle, `This PC` listesinde "Devices" grubunda. `gezik_platform::Drive`'a `device: Option<DeviceId>` eklenir (yol yerine). Sağ tık: `Open`, `Open in New Tab`, (macOS) `Look for Phones`. Çıkarma düğmesi yok (MTP'de gerekmez). Var olan 3 s'lik sürücü yoklaması bu adımda değişmez; aygıt olayları ona eklenmez, ondan ayrı çalışır (§17).

## 11. Ayarlar, eylemler, menüler

### 11.1 Ayarlar (şablon `gezik-config/templates/settings.toml` + şema `schema.rs`; 14a'nın bağ testi her anahtarı ister)

```toml
[archives]
double-click = "browse"   # browse | system | extract-here   (16a: "browse" eklenir, varsayılan)
preview-limit = 64        # MB: files inside an archive or on a phone up to this size are extracted for the preview by themselves

[git]
status = true             # show Git status in repositories (runs the system's git only while a repository folder is shown)
program = ""              # path to git; empty = PATH (macOS: not the /usr/bin stub without Command Line Tools)
ignored = true            # also mark ignored files (drawn faint)
network = false           # also look for repositories on network drives
timeout = 10              # seconds a status may take

[command-line]
shell = ""                # empty = cmd on Windows, $SHELL elsewhere; or "powershell", "pwsh", or a full path
shell-args = []           # arguments before the command, for a full path

[devices]
phones = true             # show Android phones (MTP) when plugged in
libmtp = ""               # macOS/Linux: path to libmtp; empty = Homebrew or Gezik's download (macOS), the system's (Linux)
```

`state.toml`: `[command-line] history`. `docs/configuration.md` her anahtarı; `docs/features.md`'ye "Inside archives", "Git", "Phones", "Commands in the address bar" bölümleri; `docs/shortcuts.md` yeni eylemler.

### 11.2 Eylemler (yalnız `gezik-config/src/shortcuts.rs`'nin `actions!` listesine)

| Eylem | Başlık | Tuş | Parça |
|---|---|---|---|
| `browse-archive` | Open as Folder | — | 16a |
| `run-command` | Run Command… | — | 16c |
| `toggle-command-output` | Command Output | — | 16c |
| `git-stage`, `git-unstage`, `git-commit`, `git-push`, `git-pull`, `git-fetch`, `git-switch-branch`, `git-discard`, `git-compare-head` | §9.1 | — | 16e |

`Action::ALL` 132 → 133 (16a) → 135 (16c) → 144 (16e). Sayı yalnız dizinin türünde. 16b, 16d, 16f, 16g eylem eklemez (bugünkü kopyala/yapıştır/sil/yeniden adlandır/yeni klasör VFS'te de çalışır).

### 11.3 Menü kimlikleri

16 için **3200–3599**: arşiv 3200–3229 (`Open as Folder`, `Open with System App`, `Update in Archive`, `Extract to Preview`), VFS genel 3230–3249, Git 3250–3299 (alt menü), dallar 3300–3349 (50 dal), komut paneli 3350–3369 (`Stop`, `Run Again`, `Copy Output`, `Close`), komut geçmişi satırı 3370–3379, aygıtlar 3400–3429 (`Look for Phones`), 3430–3599 boş. `GEZIK_IDS_END = 4096` değişmez.

### 11.4 Metinler

Bütün yeni metinler `t!`/`tn!`/`tx!` ve Slint `@tr` ile (15); her PR `gezik.pot`'u ve `tr.po`'yu günceller (CI eksik Türkçeyi kırar), `de/es/fr/zh-Hans.po`'ya makine çevirisi `fuzzy` olarak. Git'in, kabuğun ve aygıtın kendi çıktısı çevrilmez.

## 12. Kod yapısı

| Parça | Yer |
|---|---|
| VFS türleri | `gezik-core/src/vfs.rs` (yeni, saf: `VfsPath`, `VfsRoot`, `Container`, `Caps`, `split_archive`, metin ↔ yer), `gezik-core/src/nav.rs` (`Location::Vfs`, `parent`, `folder`, `crumbs`), `gezik-core/src/search.rs` (`Scope::Vfs`) |
| VFS sandığı | `crates/gezik-vfs/` (yeni): `lib.rs` (`Vfs`, `Edit`, `Cx`), `archive/{mod, index, sevenzip_list, edit}.rs`, `mtp.rs`, `stage.rs` (geçici alan), `tasks.rs` (`VfsCopyOutTask`, `VfsEditTask`) |
| Arşiv okuyucular | `gezik-batch/src/archive/{mod, zip, sevenz, tar, rar, cab, iso, cpio, ar}.rs` (`extract_some`), `tasks/{add, external}.rs` (ortak yeniden yazma ve 7-Zip girdi listesi) |
| Motor | `gezik-ops/src/{task, engine, inverse}.rs` (`vfs_roots`, `VfsCreated`, `VfsRenamed`) |
| Uygulama | `gezik/src/{navigation, vfs_mounts (yeni), operations, drag, archives, path_box, tab_sets, places, sidebar, context_menu, menu_bar, palette, search}.rs`, `gezik/src/view/{listing, model, columns}.rs` (`Listing::Vfs`, `entry_item`), `gezik/src/preview/mod.rs` (geçiciden) |
| Oturum | `gezik-config/src/settings.rs` (`tabs_toml`, `session_state`, `[git]`, `[command-line]`, `[devices]`, `"browse"`), `schema.rs`, şablon |
| Komut | `gezik/src/command_line.rs` (yeni: kabuk, ortam, çalıştırıcı, çözme, geçmiş), `gezik/ui/widgets/command-output.slint` (yeni), `gezik-core/src/ansi.rs` (yeni, saf: kaçış silme) |
| Git | `gezik-core/src/{git, git_filter}.rs` (yeni, saf), `gezik/src/git.rs` (yeni: bulma, depo kökü, koşu, `.git` izleme), `gezik/src/view/folder_git.rs` (yeni), `gezik/src/git_commands.rs` (yeni, 16e), `gezik/ui/widgets/git-commit.slint` (yeni), `gezik-config/src/theme.rs` (renk anahtarları), `gezik-core/src/group.rs` |
| Telefon | `gezik-platform/src/mtp/{mod, windows, libmtp, watch_windows, watch_mac, watch_linux}.rs` (yeni), `gezik-platform/src/{drives, removal}.rs`, `gezik-core/src/batch/tools.rs` (`Tool::Libmtp`), `gezik-batch/src/tools.rs`, `scripts/tools/prepare-libmtp.sh` (yeni), `scripts/tools/README.md`, `THIRD-PARTY.md` (libmtp, libusb: LGPL-2.1) |

## 13. Bütçe ve performans

| Parça | Exe (tahmin; sınır ≤ +262.144 B) | Boşta bellek | Kullanılırken |
|---|---|---|---|
| 16a | ≈ +230 KB (VFS türleri ≈ 25, sandık ve dizin ≈ 70, `extract_some` ≈ 30, gezinme/liste/oturum ≈ 60, geçici alan/önizleme ≈ 25, arama ≈ 20) | 0 | 10.000 girdili zip ≤ 150 ms; 100.000 girdi dizini ≤ 8 MB; çıkınca ±0,1 MB |
| 16b | ≈ +120 KB (yazıcılar var) | 0 | 1 GB zip'e 1 dosya eklemek ≈ disk kopya hızı (ham kopya); doğrulama yalnız yeni girdiler |
| 16c | ≈ +90 KB (panel ≈ 45) | 0 | ilk çıktı satırı komut yazdığı anda ≤ 50 ms; 2 MB çıktıda kaydırma `master` gibi |
| 16d | ≈ +110 KB | 0 | depo dışı klasör açma `master` ±%2 (yalnız `.git` bakışı, yükleme iş parçacığında); 5.000 dosyalı depoda işaretler ≤ 300 ms |
| 16e | ≈ +120 KB (commit penceresi ≈ 50) | 0 | — |
| 16f | ≈ +170 KB (WPD bağları) | 0 (WPD COM telefon açılmadan yüklenmez) | açılış ≤ +0,1 ms (bildirim kaydı, ilk kareden sonra) |
| 16g | ≈ +100 KB (libmtp tablosu, IOKit, netlink) | macOS 0; Linux ≤ +0,05 MB (uyuyan iş parçacığı) | aynı |

Toplam ≈ +940 KB. Sınıra en yakın 16a: aşılırsa arşivde arama (§5.5) ve iç içe arşiv (§5.4) 16b'nin başına kayar (PR'da gerekçeyle).

- **Ölçüm:** her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1` ve `stress.ps1`'i yazar. Ek: 16a 10.000 ve 100.000 girdili zip'te ilk satır süresi ve bellek, çıkıştan sonra bellek (`scripts/perf/archive.ps1`, yeni); 16d Gezik deposunda ve 100.000 dosyalı yapay depoda işaret süresi; 16f/16g telefon takılıyken boşta bellek (açılmadan).
- **Zamanlayıcı ve yoklama yok** (§4.8).

## 14. Parçalar (her biri bir plan ve bir PR, sırayla)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **16a** | VFS çekirdeği (§4) ve arşivde gezinme (§5): `Location::Vfs`, `gezik-vfs`, dizin, `extract_some`, gezinme, adres, oturum, sekme setleri, liste, Packed sütunu, geçici alan, önizleme, Quick Look, açma, diske kopyalama ve sürükleme, parola, iç içe, arama, `"browse"`, `browse-archive` | — |
| 2 | **16b** | Arşive yazma (§6): `VfsEditTask`, yeniden yazma, doğrulama, geri alma, bırakma/yapıştırma/silme/yeniden adlandırma/yeni klasör, iç içe yazma, "Update in Archive" | 16a |
| 3 | **16c** | Adres çubuğunda komut (§7): komut kipi, kabuk, panel, geçmiş | — (16a'dan sonra sırayla) |
| 4 | **16d** | Git durumu (§8): bulma, depo kökü, koşu, `.git` izleme, işaretler, sütun, süzgeç, gruplama, durum çubuğu | 16c (çalıştırıcı) |
| 5 | **16e** | Git komutları (§9) | 16d, 16c (panel) |
| 6 | **16f** | Telefon Windows (§10.1, §10.2, §10.5): WPD, algılama, kenar çubuğu, `Vfs` uyarlaması, işlemler | 16a, 16b (`VfsEditTask`) |
| 7 | **16g** | Telefon macOS ve Linux (§10.3, §10.4): libmtp yükleme ve indirme, IOKit, netlink, gvfs önceliği | 16f |

Sıra değer ve risk: VFS bütün adımın temeli ve en çok kullanılan özellik (arşiv) onunla gelir; yazma okumanın üstüne. Komut paneli küçük ve bağımsız; Git ondan önce gelirse kendi çıktı yerini kurmak zorunda kalırdı. Git durumu komutlarından önce: çok kullanıcıya değer, salt okunur, risksiz. Telefon en sona: VFS ve yazma yolu sağlamlaştıktan sonra, önce Windows (doğrulanabilir), sonra macOS/Linux (gerçek aygıt kullanıcıda).

### 14.1 Süreç

- **Sırayla, paralel dal yok** (15'in kuralı): her parça `master`'dan, öncekinin birleşmesinden sonra; pot her PR'da yeniden üretilir. Saf modüller (`vfs.rs`, `git.rs`, `ansi.rs`) önceki parçanın incelemesi sürerken yazılabilir, PR'a sırası gelince girer.
- Yeni eylem yalnız `actions!` listesine; menü kimlikleri §11.3 aralığında; yeni ayar şablon + şema + `docs/configuration.md` (14a'nın bağ testi).
- Notlar `docs/superpowers/notes/parts/<parça>.md`; denetim maddeleri `docs/superpowers/notes/checklists/16-macos.md` ve `16-linux.md` (paylaşılan followups ve denetim dosyalarına eklenmez).
- CI elle, adımın son PR'ı birleşmeden önce (`gh workflow run ci.yml --ref <dal>`). Çapraz denetim planın sonunda (`cargo check` macOS ve Linux hedefleri); Docker yok. Derleme `D:/Work/.build-lock` ile, en çok `-j 4`.
- Windows ekran testleri kullanıcı uzaktayken ya da birincil ekranda; oyun sırasında asla; klavye düzenine dokunulmaz; Enter'dan önce odak doğrulanır (16c'de özellikle: yanlış alana Enter bir komut çalıştırır). Telefon testleri gerçek Android telefonla kullanıcının yanında ya da kullanıcı telefonu takılı bırakınca.
- Her parçanın sonunda README özellik/performans tablosu; 16b'de `2026-10-12-ozellik-durumu.md` satır 12 ✅, 16c'de satır 53, 16e'de satır 40, 16g'de satır 44.

### 14.2 Parça ayrıntıları

Denetim listesi aralıkları: macOS 397'den, Linux 286'dan başlar.

**16a — VFS çekirdeği ve arşivde gezinme**
- *Dosyalar:* `gezik-core/src/{vfs, nav, search, columns}.rs`, `crates/gezik-vfs/` (`lib`, `archive/{mod, index, sevenzip_list}`, `stage`, `tasks` (`VfsCopyOutTask`)), `gezik-batch/src/archive/*` (`extract_some`), `gezik-ops/src/{task, engine}.rs` (`vfs_roots`), `gezik/src/{navigation, vfs_mounts, operations, drag, archives, path_box, tab_sets, places, context_menu, menu_bar, palette, search}.rs`, `gezik/src/view/{listing, model, columns}.rs`, `gezik/src/preview/mod.rs`, `gezik-config/src/{settings, schema, shortcuts}.rs`, şablon, `docs/{features, configuration, shortcuts}.md`, `README.md`.
- *Testler:* `split_archive` (Windows ve Unix yolları, iç içe, `x.zip` adlı klasör, arşiv olmayan `.zip`, sondaki ayırıcı, büyük/küçük harf); `Location::Vfs` `parent`/`folder`/`crumbs`; oturum gidiş-dönüşü (arşiv, iç içe, aygıt) ve eski Gezik'in girdiyi atlaması (bugünkü ayrıştırıcıyla); sekme seti metni; dizin: türetilen ara klasörler, `..`/mutlak adların reddi, yinelenen ad, 100.000 girdi bellek sınırı; `extract_some` her biçimde yalnız seçilenleri ve `safe_join` ile; parola: soru, yanlış, iptal, belleğin bırakılması; iç içe derinlik sınırı; `entry_path` VFS'te `None`, `entry_item` dolu; §4.7 tablosunun her satırı (menüde yok); geçici alan: ölü süreç klasörü silinir, canlı olanınki silinmez; ad araması dizinde; 7-Zip listesi (`-slt` çıktısı örnekleriyle, sahte süreçle); son sekme ayrılınca bağ bırakılır (zayıf referans).
- *Denetim listesi:* macOS 397–400: (397) bir zip'e çift tık içine girer, ⌘↑ arşivin klasörüne ve arşiv seçili; adres çubuğu `…/x.zip/docs`; (398) arşivde Space Quick Look ve önizleme bölmesi bir resmi ve PDF'i gösterir, ⌘C + Finder'a değil Gezik'in başka sekmesine ⌘V çıkarır; Finder'a ≤ 256 MB'lık seçimi sürükleme çalışır; (399) parolalı 7z (başlığı şifreli) parolayı sorar, yanlış parola yeniden sorar; arşivdeki tar.gz'nin içine girilir; (400) Gezik'i kapatıp açınca arşivin içindeki sekme yerinde; arşiv silinmişse en yakın klasör. Linux 286–287: (286) zip, tar.xz, deb'in içinde gezinme, Nautilus'a sürükleme (X11 ve Wayland); (287) 7-Zip'le açılan bir biçim (ör. `.wim` ya da `.dmg`), 7-Zip yokken indirme kutusu.

**16b — arşive yazma**
- *Dosyalar:* `gezik-vfs/src/archive/edit.rs`, `gezik-vfs/src/tasks.rs` (`VfsEditTask`), `gezik-batch/src/tasks/{add, compress}.rs` (ortak yeniden yazma), `gezik-ops/src/{task, inverse}.rs`, `gezik/src/{operations, drag, archives, vfs_mounts}.rs`, `docs/features.md`.
- *Testler:* zip/7z/tar.* için `Put`, `Delete`, `Rename`, `MakeDir` gidiş-dönüşü (içerik, tarih, Unix kipi, parola); yazma 1–4'ün her adımında iptal ve sahte hata → eski arşiv bayt olarak aynı, geçici yok; doğrulama bozuk yazıyı yakalar (yazıcıya enjekte edilen hata); geri alma bayt olarak eski arşiv; salt okunur biçimde `Put` reddi; çöpsüz sürücü sorusu; iç içe yazma iki düzey; "Update in Archive": geçici dosya değişince şerit, bağ bırakılırken soru; aynı arşive iki iş sırayla (`vfs_roots`).
- *Denetim listesi:* macOS 401–402: (401) Finder'dan bir zip'in içine dosya bırakma, arşivde yeniden adlandırma ve silme, ⌘Z her birini geri alır; Archive Utility ile açılan sonuç doğru; (402) arşivdeki bir `.txt`'yi TextEdit'te açıp kaydet → şerit → `Update in Archive`. Linux 288: tar.gz'ye ekleme ve silme, `tar tzf` ile sonuç; Wayland'de dosya bırakma.

**16c — adres çubuğunda komut**
- *Dosyalar:* `gezik/src/{command_line, path_box, panes, windows, actions}.rs`, `gezik/ui/app.slint`, `gezik/ui/widgets/{pane, command-output}.slint`, `gezik-core/src/ansi.rs`, `gezik-config/src/{settings, schema, shortcuts}.rs`, şablon, `docs/{features, configuration}.md`.
- *Testler:* `>` algısı (baştaki boşluk, `>` yalnız, yolda `>` yok); Enter dışında hiçbir yol çalıştırmaz (yapıştırma, geçmiş seçimi, oturum); kabuk komut satırı her seçenekte (tırnak, `"`, `%`, `^`, Unicode; PowerShell base64); ortam değişkenleri; OEM kod sayfasına düşme; ANSI silme (SGR, imleç, OSC, yarım kaçış); 2 MB sınırı ve not; `Stop` süreç ağacını bitirir (çocuk süreç de, Windows iş nesnesi, Unix grup); girdi bekleyen program EOF ile biter; geçmiş: tekrarsız, 100, boşlukla başlayan kaydedilmez, `remember = false`; VFS'te ve This PC'de reddedilir.
- *Denetim listesi:* macOS 403–404: (403) `>ls -la` zsh'de; `>echo $GEZIK_FILES` seçimi yazar; `>sleep 30` `Stop` ile biter; (404) `>vim` hemen biter ya da hata verir, Gezik takılmaz; geçmiş ↑. Linux 289–290: (289) `$SHELL` bash ve fish ile; (290) `>sudo ls` "a terminal is required" der, takılmaz.

**16d — Git durumu**
- *Dosyalar:* `gezik-core/src/{git, git_filter, columns, group}.rs`, `gezik/src/{git, navigation, sidebar}.rs`, `gezik/src/view/folder_git.rs`, `gezik/ui/app.slint`, `gezik/ui/widgets/{file-icon, file-view}.slint`, `gezik-config/src/{settings, schema, theme}.rs`, şablon, `docs/{features, configuration, themes}.md`.
- *Testler:* porcelain v2 ayrıştırıcısı (1/2/u/?/! satırları, `-z`, boşluklu ve Unicode adlar, yeniden adlandırma, alt modül, `branch.ab` yok/var, ayrık HEAD); klasör toplaması önceliği; süzgeç kelimeleri; depo bulma (`.git` dosyası, ev klasörü sınırı, ağ sürücüsü atlanır); macOS `/usr/bin/git` kuralı (sahte `xcode-select`); koşu iptali ve zaman aşımı (sahte yavaş git); olay birleştirme (10 olay → 1 koşu, koşu sırasında gelen → en çok 1 koşu daha); depo dışında süreç başlamaz (sayaçlı sahte çalıştırıcı); gerçek git'le uçtan uca (geçici depo; git yoksa `#[ignore]`).
- *Denetim listesi:* macOS 405–406: (405) Command Line Tools'suz bir Mac'te (ya da `xcode-select -p` başarısızken) depo klasörü kurulum penceresi açmaz, not bir kez; Homebrew git ile işaretler; (406) terminalde `git add` → Gezik'te işaret ≤ 1 s içinde değişir (FSEvents). Linux 291: inotify ile aynısı; `.gitignore`'a yazılan klasör soluklaşır.

**16e — Git komutları**
- *Dosyalar:* `gezik/src/{git_commands, context_menu, menu_bar, palette, operations}.rs`, `gezik/ui/widgets/git-commit.slint`, `gezik-config/src/shortcuts.rs`, `gezik/src/diff.rs` (HEAD karşılaştırması), `docs/{features, shortcuts}.md`.
- *Testler:* her komutun komut satırı (yol listesi `--` sonrası, 32 KB'ta `--pathspec-from-file`); commit iletisi girdiden (Unicode, çok satır, `#` ile başlayan satır `--cleanup=verbatim` değil git'in varsayılanıyla); discard: çöpe sonra `restore`, geri alma asıl dosyayı getirir, izlenmeyen yalnız çöpe, çöpsüz sürücüde kapalı; kimlik hatası metinlerinde `Open in Terminal`; aynı depoda komut kuyruğu; dal listesi 50 sınırı; gerçek git'le uçtan uca (yerel çıplak depoya push/pull; ağsız).
- *Denetim listesi:* macOS 407–408: (407) Anahtar Zinciri'nde kimliği olan bir GitHub deposunda Push; ssh anahtarlı depoda (ssh-agent) Pull; (408) Discard Changes → ⌘Z dosyayı geri getirir; Compare with HEAD FileMerge'te (`[tools] diff = "opendiff"`). Linux 292: libsecret yardımcısıyla push; kimliksiz HTTPS deposunda hata ve `Open in Terminal`.

**16f — telefon, Windows**
- *Dosyalar:* `gezik-platform/src/mtp/{mod, windows, watch_windows}.rs`, `gezik-platform/src/{drives, removal}.rs`, `gezik-platform/Cargo.toml` (`Win32_Devices_PortableDevices`), `gezik-vfs/src/mtp.rs`, `gezik-ops/src/inverse.rs`, `gezik/src/{sidebar, places, navigation, vfs_mounts}.rs`, `docs/features.md`.
- *Testler:* `IdCache` (yinelenen adlar, yeniden adlandırma sonrası), üzerine yazma sırası (yeniden adlandırma var/yok), silme sorusu kalıcı, `VfsCreated` geri alması, çıkarma olayında sekmelerin `Drives`'a gitmesi, disk harfli WPD aygıtının atlanması (saf kural); sahte `Session` ile `Vfs` uyarlamasının bütün işlemleri; gerçek aygıtla `#[ignore]` testi (bakımcının telefonu).
- *Denetim listesi:* macOS ve Linux yok (Windows'a özgü). Windows ekran testi (kullanıcıyla): telefon tak → kenar çubuğu; DCIM'den masaüstüne kopya; telefona klasör ve dosya; yeniden adlandırma; silme; ızgarada küçük resimler; telefonu çıkar; iPhone takılınca fotoğraflar salt okunur.

**16g — telefon, macOS ve Linux**
- *Dosyalar:* `gezik-platform/src/mtp/{libmtp, watch_mac, watch_linux}.rs`, `gezik-core/src/batch/tools.rs` (`Tool::Libmtp`), `gezik-batch/src/tools.rs`, `gezik/src/archives.rs` (indirme kutusu), `scripts/tools/{prepare-libmtp.sh, README.md, sources.sha256}`, `THIRD-PARTY.md`, `docs/features.md`, `README.md`.
- *Testler:* libmtp sembol tablosu (eksik sembolde düzgün hata, sürüm denetimi); Homebrew yolları önceliği; gvfs bağ adı ayrıştırma (`mtp:host=SAMSUNG_…`); netlink olay ayrıştırma (örnek `add@/devices/…` iletileri); sınıf 6 algısı (`/sys` örnek ağacı); IOKit yalnız `cargo check`; gerçek aygıtla `#[ignore]` testi.
- *Denetim listesi:* macOS 409–412: (409) libmtp yokken telefon takılınca kenar çubuğunda görünür, tıklayınca indirme kutusu, indirince açılır; (410) Homebrew libmtp varken indirme sorulmaz; (411) DCIM'den kopya, telefona dosya ve klasör, yeniden adlandırma, silme, çıkarma; (412) Image Capture açıkken "Another app is using the phone" notu, kapatınca açılır. Linux 293–296: (293) GNOME'da gvfs telefonu bağlamışken Gezik gvfs yoluna gider; (294) gvfs'siz (KDE ya da `systemctl --user stop gvfs-daemon`) libmtp ile aynı işlemler; (295) `libmtp9` kurulu değilken not; (296) udev kuralı yokken izin notu.

## 15. Test

### 15.1 Birim (saf; her sistemde)

`gezik-core` (`vfs`, `git`, `git_filter`, `ansi`, `nav` VFS dalları, `columns`, `group`), `gezik-vfs` (dizin, yeniden yazma ve doğrulama, geçici alan, `IdCache`, sahte `Session` ile MTP), `gezik-batch` (`extract_some`), `gezik-ops` (`vfs_roots` kuyruğu, `VfsCreated` geri alma), `gezik-config` (oturum gidiş-dönüşü, şema bağları), `gezik` (`command_line` kabuk satırları ve sınırlar, `git` koşu ve birleştirme, `folder_git`).

### 15.2 Platform

- Windows: `cmd /d /s /c` tırnaklama, OEM çözme, iş nesnesi; WPD; `GUID_DEVINTERFACE_WPD` bildirimi; Git for Windows ve GCM.
- macOS: `/usr/bin/git` kabuğu, FSEvents ile `.git`, IOKit bildirimi, libmtp indirme, `ptpcamerad`.
- Linux: `$SHELL`, inotify, netlink, gvfs önceliği, `libmtp.so.9`.

### 15.3 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken; telefon testleri kullanıcıyla): arşiv turu (zip, 7z, rar, iso; parola; iç içe; büyük tar.gz iptali), yazma ve geri alma, `>` komutları (cmd ve PowerShell), Gezik deposunda Git turu, telefon turu. Gerçek dosyalar yedek + bayt karşılaştırmasıyla.
- **macOS** (`checklists/16-macos.md` 397–412, sonuçlar `macos-test-results.md`'ye) ve **Linux** (`checklists/16-linux.md` 286–296; GNOME ve KDE, X11 ve Wayland).

## 16. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| `entry_path`'i kullanan bir yer VFS'i yanlış ele alır | Gerçek olmayan yola işlem | VFS'te `None`: bilmeyen kod kendiliğinden kapanır; §4.7 tablosu testli |
| Arşiv yeniden yazımı veri kaybeder | Kullanıcının arşivi bozulur | Geçici + doğrulama + atomik + eskisi çöpte; her adımda hata enjeksiyonu testi |
| Katı 7z/tar.gz'de küçük değişiklik bütün arşivi yeniden yazar | Uzun süre, çöp dolar | Süre tahmini soruda; iptal; bilinen sınır (§17) |
| Arşiv bombası, yol aşımı | Disk dolar, dışarı yazma | Bugünkü `safe_join`, boyut sınırları, geçici alan sayacı; önizleme sınırı |
| 16a bütçesi | Sınır aşılır | Arama ve iç içe 16b'ye kayabilir (§13) |
| macOS `/usr/bin/git` kurulum penceresi açar | Kullanıcı rahatsız olur | `xcode-select -p` denetimi, Homebrew yolları, not bir kez |
| Büyük depoda `git status` yavaş | İşaretler geç, süreç birikir | Tek koşu, iptal, zaman aşımı, olay birleştirme, `--no-optional-locks`, git'in kendi fsmonitor'u |
| Kimlik isteyen push takılır | Panel sonsuza bekler | `GIT_TERMINAL_PROMPT=0`, girdi EOF, `Stop`, `Open in Terminal` |
| `>` komutu yanlışlıkla çalışır | İstenmeyen değişiklik | Yalnız Enter; geçmiş ve yapıştırma çalıştırmaz; ekran testlerinde odak doğrulanır |
| Komut çıktısı çok büyük | Bellek | 2 MB halka |
| WPD sürücü farkları (Samsung, Xiaomi) | Bazı işlemler başarısız | `GetPropertyAttributes` ile yetenek, hata metni sistemden; gerçek aygıt testi |
| macOS'ta `ptpcamerad` aygıtı tutar | Telefon açılmaz | Açık not; Image Capture'ın "bu aygıt bağlanınca aç" ayarı belgede |
| macOS'ta indirilen dylib ve ileride imza/noter | Kitaplık doğrulaması dylib'i reddeder | pdfium ile aynı durum; imza planında `disable-library-validation` ya da kendi imzamız (yayın planı notu) |
| libmtp ile gvfs aynı aygıtı ister | Biri açamaz | gvfs bağı varsa onun yolu; bağ bırakılınca aygıt bırakılır |
| Linux'ta netlink iş parçacığı | Boşta bellek | ≤ +0,05 MB, ölçülür; `[devices] phones = false` hiç başlatmaz |

## 17. Bilinen sınırlar

- VFS öğeleri Explorer/Finder'a yapıştırılamaz; dış sürükleme ≤ 256 MB'lık seçimle.
- Katı 7z ve tar ailesinde her değişiklik bütün arşivi yeniden paketler; her değişiklikte eski arşiv çöpe gider (büyük arşivde çöp büyür).
- tar.gz gibi akış arşivlerinde tek dosya açmak arşivi o dosyaya kadar açmak demek; arşivden çıkıp dönünce yeniden taranır.
- Arşivde içerik araması yok; telefonda arama yok.
- Telefonda silme geri alınamaz; değiştirme tarihi korunamaz.
- `>` komutu TTY vermez, renk göstermez; Windows'ta bazı eski programların çıktısı kod sayfası yüzünden bozuk görünebilir.
- Git durumu ağ sürücüsünde varsayılan kapalı; WSL yollarında (`\\wsl$`) Windows git'i yavaş ya da yanlış olabilir.
- Kimlik isteyen ama yardımcısı olmayan git işi terminalde yapılır (`Open in Terminal`).
- iPhone yalnız Windows'ta, salt okunur fotoğraflar.
- Kenar çubuğunun var olan 3 s'lik sürücü yoklaması (bugünden kalma) yerinde; aygıt olaylarıyla değiştirilmesi ayrı bir iş (bu adımın olay kaynakları onu da besleyebilir; parça notunda öneri).

## 18. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 16a, 16b, 16c, 16d, 16e, 16f, 16g; sırayla** (§14). *Gerekçe:* VFS temel ve en geniş değer arşivle gelir; yazma okumanın üstüne; komut paneli Git'in çıktı yeri; Git durumu komutlardan önce ve risksiz; telefon sağlam VFS'in üstüne, önce doğrulanabilir Windows.
2. **Yeni `Location::Vfs(VfsPath)` türü; `Location::Path`'e sahte yol koymak yok.** *Gerekçe:* `Path` her yerde gerçek dosya demek; ayrı tür derleyiciye her `match`'i gösterir; `Trash` aynı yolun örneği.
3. **Arşiv yolu disk yolunun devamı (`C:\a\b.zip\c`), telefon `mtp://`.** *Gerekçe:* kullanıcının bildiği biçim (7-Zip FM, TC, Explorer); yapıştırılan yol çalışır; telefon ve 18'in uzak yerleri için URL ailesi tek kural.
4. **Oturumda yapılı alanlar, sekme setinde metin.** *Gerekçe:* oturum metni yeniden ayrıştırmaz (arşiv silinse de doğru düşer); sekme seti kullanıcının elle yazdığı biçim.
5. **`entry_path` VFS'te `None`, yeni `entry_item`.** *Gerekçe:* VFS'i bilmeyen her özellik kendiliğinden kapanır; yanlışlıkla gerçek olmayan yola işlem imkânsız.
6. **Ayrı `gezik-vfs` sandığı; saf türler `gezik-core`'da; aygıt bağları `gezik-platform`'da.** *Gerekçe:* `Location` çekirdekte; arşiv uygulaması `gezik-batch`'in okuyucularını, telefon platformun COM/dlopen kodunu kullanır; 18'in uzak bağlantıları aynı sandığa girer.
7. **Değişiklik `Edit` listesi olarak, arşivde tek yeniden yazma.** *Gerekçe:* üç dosya bırakmak üç kez yeniden yazmak olmamalı; telefonda aynı liste tek tek uygulanır, tek arayüz.
8. **Arşiv yazma: geçici + doğrulama + atomik + eski arşiv çöpe; 7z/tar bugünkü `AddToArchiveTask` yolu.** *Gerekçe:* kullanıcı kararı "güvenli"; geri alma kod yazmadan çöpten; bugünkü yol test edilmiş.
9. **Bağ yalnız gösterilirken; önbellek yok.** *Gerekçe:* bırakılınca bellek döner (hafiflik); zip'in yeniden okunması hızlı; akış arşivinin yeniden taranması nadir ve bilinen sınır.
10. **Önizleme ve açma geçiciden, 64 MB'a kadar kendiliğinden.** *Gerekçe:* bugünkü önizleme ve Quick Look gerçek dosya ister; sınır büyük video yüzünden istenmeden GB çıkarmayı önler.
11. **Dış sürükleme ≤ 256 MB geçiciye çıkarılarak; sistem panosuna dosya yok.** *Gerekçe:* Windows'un sanal dosya (`CFSTR_FILEDESCRIPTOR`) ve macOS'un söz verilen dosya yolları büyük iş; Gezik içi kopyalama her boyutta çalışır.
12. **Git için sistem `git`'i, kütüphane yok** (§8.1). *Gerekçe:* bütçe, kimlik doğrulama, git ayarlarıyla tam uyum, TLS kodu taşımama.
13. **Git durumu tek `git status --porcelain=v2` koşusu, gösterilen klasöre sınırlı; `.git` izlemesi olayla.** *Gerekçe:* tek süreç her şeyi verir (dal, önde/geride dahil); yol sınırı büyük depoda süreyi düşürür; `--no-optional-locks` kullanıcının git'ini bozmaz.
14. **Değişikliği atma önce çöpe.** *Gerekçe:* kullanıcı kararı "geri alınabilir"; `git stash` kullanıcının stash listesini kirletir; çöp Gezik'in bugünkü geri alma yolu.
15. **Push/pull kimliği git'e bırakılır, `GIT_TERMINAL_PROMPT=0`, hata olursa terminal.** *Gerekçe:* kendi askpass'ımız ikinci bir süreç ve pencere demek; yardımcılar çoğu kurulumu zaten çözer.
16. **`>` komutu: Windows `cmd`, başka yerde `$SHELL -c`; seçim ortam değişkeniyle.** *Gerekçe:* cmd her Windows'ta var ve ≈ 20 ms'de başlar (PowerShell ≈ 300 ms+); metne yer tutucu koymak kabukların kendi `{}`/`%`/`$` sözdizimiyle çakışır.
17. **Komut çıktısı bölmenin altında, 2 MB, ANSI silinir.** *Gerekçe:* terminal öykünücüsü değil (kullanıcı kararı); sınır belleği korur.
18. **Windows telefonu WPD ile, algılama WPD yüklemeden (cfgmgr32).** *Gerekçe:* WPD sistemde; COM'u yalnız telefon açılınca yüklemek boşta belleği korur.
19. **macOS libmtp: önce Homebrew, yoksa istekle indirilen, bizim derlediğimiz paket.** *Gerekçe:* resmi ikili yok; Homebrew'u zorunlu kılmak teknik olmayan kullanıcıyı dışarıda bırakır; pdfium ile aynı mekanizma (SHA-256'lı, istekle, `dlopen`).
20. **Linux libmtp sistem paketi `dlopen`; gvfs aygıtı tutuyorsa gvfs yolu.** *Gerekçe:* kullanıcı kararı; aynı aygıtı iki taraf açamaz, GNOME kullanıcısında gvfs çoğu zaman önce davranır.
21. **Aygıt olayları: Windows `WM_DEVICECHANGE`, macOS IOKit, Linux netlink; yoklama yok.** *Gerekçe:* kullanıcı kararı; Windows ve macOS ana döngüye bağlanır (iş parçacığı yok), Linux'ta uyuyan tek iş parçacığı.
22. **Arşivde çift tık varsayılanı `"browse"`.** *Gerekçe:* Explorer zip'i böyle açar; boşta maliyeti yok; eski davranış bir ayar uzakta (§19 soru 1).
23. **Git işaretleri varsayılan açık, ağ sürücüsünde kapalı.** *Gerekçe:* maliyet yalnız depo içinde ve olayla; depo dışında bir `.git` bakışı yükleme iş parçacığında (§19 soru 2).
24. **Menü kimlikleri 3200–3599; `Action` 132 → 144; denetim listeleri macOS 397–412, Linux 286–296.** *Gerekçe:* 15 3199'da, 396/285'te bitiyor; aralıklar önceden ayrılınca parçalar çakışmaz.
25. **Bütçe: parça başına ≤ +262.144 B, boşta 0 (Linux ≤ +0,05 MB), açılış ±0.** *Gerekçe:* önceki adımların kuralı; her özellik istekle.

## 19. Açık sorular (bakımcıya)

1. **Arşivde çift tık:** varsayılan `"browse"` (Explorer gibi, arşive girer) mi, bugünkü `"system"` mı kalsın? Spec `"browse"` diyor (karar 22).
2. **Git işaretleri varsayılan açık mı?** Spec açık diyor (karar 23): maliyet yalnız depo içinde. "Varsayılanlar isteğe bağlı" ilkesine göre kapalı da olabilir; o zaman ilk depoda bir kez "Show Git status here?" önerisi.
3. **Windows'ta `>` komutunun varsayılan kabuğu:** `cmd` (hızlı, her yerde) mi, PowerShell (bugünün Windows terminali, ≈ 300 ms başlangıç) mi? Spec `cmd` diyor (karar 16).
4. **macOS libmtp paketi:** libmtp + libusb'yi bir Mac'te kendimiz derleyip `wenlar/gezik-tools`'ta yayımlamak (LGPL, kaynak bağlantısıyla) uygun mu? Değilse macOS'ta yalnız Homebrew (`brew install libmtp`) ve not.
