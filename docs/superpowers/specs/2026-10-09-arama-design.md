# Alt Proje 8: Arama — Tasarım

- **Tarih:** 2026-10-09
- **Durum:** Tasarım taslağı; kullanıcı onayı bekliyor
- **Kapsam:** Gezik yol haritasının 8. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md` §2 "8 Arama": maddeler 2, 3, 27, 49, 51; §6'daki TC notları ve açık karar 7); tek spec, iki plan ve iki PR: **8a** özyinelemeli arama ve düz görünüm; **8b** klasör boyutu, komut paleti ve kayıtlı aramalar
- **Dayandığı:** `2026-10-04-gorunum-design.md` (liste modeli, sütunlar, sıralama, `views.toml`), `2026-10-04-gezinme-design.md` (sekmeler, geçmiş, adres çubuğu), `2026-10-04-dosya-islemleri-design.md` (iş motoru, çakışma listesi, geri alma, pano), `2026-10-05-toplu-islemler-design.md` (toplu ad), `2026-10-07-klavye-paketi-design.md` (süzgeç ve desen dili, sekme seçici, eylem listesi, AltGr kuralı), `2026-10-08-gunluk-kolayliklar-design.md` (oturum, `[view]` gizli/sistem, menü kimlik aralıkları, 7c'nin eylemleri)
- **7c ile ilişki:** 7c (`feat/daily-7c`) bu spec yazılırken birleşmemişti. 8, 7c'nin eklediklerini var sayar: 63 eylem (`new-folder-with-selection`, `add-to-stack`, `toggle-stack`, `show-history` dahil), menü kimlikleri 1000–1459 ve `GEZIK_IDS_END = 4096`, en çok dört alt menü (`MAX_SUBMENUS`). 8a'nın planı 7c birleştikten sonraki `master`'dan başlar.

## 1. Amaç

Rakip notlarının hepsinde ilk sırada çıkan boşluğu kapatmak: bir klasörün altında adıyla, içeriğiyle, tarihiyle ve boyutuyla dosya bulmak; bulunanlarla, klasördeki dosyalarla çalışır gibi çalışmak (sıralama, önizleme, kopyala, taşı, sil, toplu ad); bir klasör ağacını tek liste olarak görmek; klasörlerin boyutunu beklemeden görmek ve ona göre sıralamak; bütün eylemlere ve yerlere klavyeden tek bir pencereden ulaşmak; sık yapılan aramaları saklamak.

Tek ilke: **tarayıcı, süzme ve sonuç listesi ortaktır.** Arama, düz görünüm ve klasör boyutu aynı arka plan tarayıcısını; arama çubuğu, süzgeç, sekme seçici ve palet aynı desen dilini (`gezik-core::pattern`) ve aynı katlama kuralını kullanır; arama sonucu ile düz görünüm aynı sanal listedir (TC'nin Branch View + "Feed to listbox" modeli).

### Başarı ölçütleri

- 1.000.000 dosyalık yerel bir ağaçta (SSD, ısınmış önbellek) Gezik'in kendi tarayıcısıyla ad araması ≤ 10 sn'de biter; ilk sonuçlar ≤ 0,3 sn'de görünür. Everything kuruluyken aynı arama ≤ 0,5 sn.
- Arama, düz görünüm ve klasör boyutu sürerken arayüz iş parçacığı hiç beklemez: kaydırma, sekme değiştirme ve yazma akıcı kalır; arayüz iş parçacığındaki her parti uygulaması ≤ 4 ms.
- Arama her an durdurulabilir; Esc ya da Stop'tan sonra ≤ 100 ms içinde iş parçacıkları disk okumayı bırakır.
- Sonuç listesinde sütunlar, sıralama, süzgeç (Ctrl+F), önizleme, Hızlı Bakış, kopyala/kes/yapıştır, sürükle-bırak, çöpe at, yeniden adlandır ve toplu ad klasördeki gibi çalışır; her dosya işi tek Ctrl+Z ile geri alınır.
- Bellek: 250.000 sonuç ≤ 35 MB; sonuç sekmesi kapanınca bellek boşta değerine (+≤ 2 MB) döner. Arama yokken boşta bellek `master`'a göre büyümez.
- Klasör boyutu sütunu açıkken 100.000 öğelik klasörde açılış ve kaydırma süresi değişmez; boyuta göre sıralama boyutların gelmesini beklemez.
- Palet Ctrl+Shift+P'den sonra ≤ 50 ms'de açılır; 2.000 öğede her tuş vuruşunda süzme ≤ 5 ms.
- Exe büyümesi her parçada ≤ +0,25 MiB (7c birleştikten sonraki `master`'a göre); yeni crate bağımlılığı yok.
- Everything olmayan, kapalı ya da yanıt vermeyen makinede Gezik aynı sonuçları kendi tarayıcısıyla verir; kullanıcı farkı yalnız süreden ve durum çubuğundaki kaynaktan anlar.

### Kapsam dışı (bilerek)

- Diske yazılan kalıcı dizin (index) ve dosya sistemi günlüğü okumak (NTFS USN, MFT okuma, fanotify). Kalıcı hız isteyen Everything kurar.
- Windows Search, Spotlight, tracker/baloo (kullanıcı kararı 1).
- PDF, Office, arşiv içi ve meta veri (EXIF, ID3) içeriğinde arama (kullanıcı kararı 2). Arşiv içi 16. adımın VFS'iyle, meta veri alanları 13. adımın alan kaydıyla gelir.
- Sorgu metninde `size:>10mb`, `modified:<7d` gibi anahtar sözdizimi; VE/VEYA kural ağaçları. Ölçütler arama çubuğunun denetimleriyle verilir (Karar 3).
- Önizleme panelinde eşleşmenin vurgulanması ve eşleşen satıra kaydırma (Önizleme 2).
- Sonuçların canlı izlenmesi: dışarıdan yapılan değişiklikler sonuç listesine F5'e kadar yansımaz (Gezik'in kendi işleri yansır, §4.7).
- `.gitignore` ve benzeri dosyaları okumak (Karar 8).
- Yinelenen dosya bulucu (11. adım), boyut haritası (11. adım), kopya diyaloğunda arama süzgeci (11. adım; 8'in sorgu modeli oraya hazır bırakılır).
- Paletten kısayol atama (File Pilot'taki gibi); kısayollar `[shortcuts]` ile değişir.

### Önceki spec'lerle ilişki

- **Görünüm spec'i:** "Klasör boyutu hesaplanmaz, Boyut sütunu klasörde boş" kuralı 8b ile `[view] folder-sizes`'a bağlanır (§6). Varsayılan değerle yerel klasörlerde boyut görünür.
- **Klavye paketi spec'i §3:** süzgeç Ctrl+F'de kalır; arama ayrı bir eylemdir (Karar 1). Süzgeç çubuğu sonuç listesinde de çalışır ("sonuçlar içinde arama").
- **Gezinme spec'i:** `Location` iki yeni tür alır (`Search`, `Flat`); geçmiş, sekme ve oturum bunları taşır (§4.4, §9.2).

## 2. Alınan kararlar

| Konu | Karar | Kaynak |
|---|---|---|
| Dizin | Gezik'in kendi arka plan tarayıcısı her zaman; Windows'ta Everything kuruluysa ad araması ondan, anında; Everything yoksa her şey yine çalışır. Windows Search/Spotlight/tracker yok | Kullanıcı kararı 1 |
| İçerik | Yalnız düz metin dosyaları (txt, md, kod, json, log…); UTF-8/UTF-16 algılama; ikili dosyalar atlanır; PDF/Office yok | Kullanıcı kararı 2 |
| Sonuçlar | Sekmede sanal liste: sütunlar, sıralama, önizleme, kopyala/taşı/sil/toplu ad çalışır; düz görünüm aynı liste | Kullanıcı kararı 3 |
| Bölme | 8a arama + düz görünüm; 8b klasör boyutu + komut paleti + kayıtlı aramalar | Kullanıcı kararı 4 |
| Ortak kod | Yeni `gezik-search` crate'i: tarayıcı, eşleştiriciler, Everything istemcisi, sonuç kümesi, klasör boyutu. Sorgu modeli saf veri olarak `gezik-core`'da (geçmiş ve oturum onu taşır) | Rakip özeti "8 Arama" satırı |
| Desen dili | Ad alanı 6a'nın desen dilidir (`;`, `!`, `*`, `?`, Türkçe i katlama); palet aynı katlamayı kullanır | Rakip özeti "8 Arama" satırı |
| Diğer her şey | §13'teki kararlar; hepsi kullanıcı onayı bekler | Bu spec |

## 3. Ortak çekirdek

### 3.1 Sorgu modeli (`gezik-core::search`)

Saf veri; geçmiş, oturum ve kayıtlı aramalar bunu taşır, `gezik-search` onu derler.

```rust
pub struct SearchSpec {
    pub scope: Scope,                 // Folder(PathBuf) | AllDrives
    pub pattern: String,              // 6a desen dili ya da regex (name_regex)
    pub name_regex: bool,
    pub content: String,              // boş: içerik araması yok
    pub content_regex: bool,
    pub match_case: bool,             // yalnız içerik ve regex için; ad deseni hep katlanır
    pub size: SizeRange,              // min/max bayt, ikisi de isteğe bağlı
    pub modified: DateRange,          // Any | Today | LastDays(n) | ThisYear | Between(a, b)
    pub kind: KindFilter,             // Any | Files | Folders | Pictures | Videos | Audio | Documents | Archives | Code
    pub hidden: HiddenRule,           // FollowView | Include
    pub skipped: bool,                // [search] skip'teki klasörlere de gir
    pub flat: bool,                   // düz görünüm (§5): desen boş, yalnız dosyalar
}
```

- `KindFilter` `gezik_core::kind::Kind`'a eşlenir: Pictures = `Image`; Documents = `Document`, `Spreadsheet`, `Presentation`, `Pdf`, `Text`; Archives = `Archive`, `DiskImage`; Code = `Code`.
- `DateRange::LastDays(n)` aramanın başladığı ana göre hesaplanır; `Between` yerel saatle gün başı–gün sonu, iki ucu da dahil.
- Metin biçimi (oturum ve `[[searches]]`): `size-min = "500 MB"`, `modified = "7d" | "today" | "year" | "2026-01-01..2026-06-30"`, `type = "videos"`. Ayrıştırma ve yazma saf işlev, birim testli; boyut birimleri `B, KB, MB, GB, TB` (1024'lük) ve `kB` (1000'lik), `size-format`'tan bağımsız.
- Boş sorgu (desen ve içerik boş, ölçüt yok) arama değildir; düz görünüm (`flat`) dışında "Type something to search" der.

### 3.2 Ad eşleştirme

- Varsayılan: `Pattern::compile` (6a): parçası `*`/`?` içermeyen kısım adın her yerinde, joker içeren tüm adla eşleşir; `;` seçenek, `!` dışarıda bırakır; büyük/küçük harf ve Türkçe i/İ/ı/I katlanır. Ad alanının hata metinleri süzgeçteki gibidir.
- `name_regex`: Rust `regex` sözdizimi; `match_case` kapalıyken `(?i)` eklenir ve Türkçe i harfleri sınıfa çevrilir (aşağıda). Geçersiz regex alanın altında hata olarak yazılır, arama başlamaz.
- Eşleşme yalnız ada bakar, yola bakmaz (bir klasörün adıyla eşleşmek altındakileri getirmez). Yol parçasıyla arama 13. adımın işi değil, bilerek dışarıda; kapsamı daraltmak için kapsam seçilir.

### 3.3 İçerik eşleştirme

- **Hangi dosyalar** (Karar 10): ad, boyut, tarih ve tür ölçütlerinden geçen her dosya, şunlar dışında: `Kind` resim, video, ses, arşiv, disk görüntüsü, PDF, Office, yazı tipi, çalıştırılabilir olanlar (uzantıyla, okumadan atlanır) ve `[search] content-max-size`'tan (varsayılan `64 MB`) büyükler. Kalanlar okunarak sınanır.
- **Metin algılama** (önizlemenin `decode_text`'i ile ortak; `gezik-core::text`'e taşınır): ilk 8 KB'ta BOM varsa UTF-8 / UTF-16 LE / UTF-16 BE; BOM yoksa ve tek sıradaki baytların ≥ %40'ı NUL, çift sıradakilerin ≤ %5'i NUL ise BOM'suz UTF-16 LE (tersi BE); yoksa ilk 8 KB'ta NUL varsa ikili (atlanır); yoksa UTF-8, geçersiz UTF-8'de Windows'ta kullanıcının ANSI kod sayfası (`gezik_platform::decode_ansi`), başka yerde kayıplı UTF-8.
- **Okuma:** 256 KB'lık parçalarla, satır sınırında bölünerek; tek satır 1 MB'ı geçerse orada bölünür (sınırda kalan eşleşme kaçabilir; belgelenen sınır). UTF-16 parça parça UTF-8'e çevrilir. Dosya başına ilk eşleşmede durulur (yalnız "var mı" ve ilk satır gerekir).
- **Eşleştirici:** düz metin, `regex::escape` ile regex'e çevrilir; `match_case` kapalıyken `(?i)` ve Türkçe katlama için `i`, `I`, `İ`, `ı` harfleri `[iIİı]` sınıfına çevrilir (Unicode'un basit katlaması İ'yi i ile eşlemez; 6a'nın kuralıyla aynı sonuç). `content_regex` iken kullanıcının regex'i aynı katlama dönüşümünden geçmez, yalnız `(?i)` alır. Eşleşme satır satırdır (çok satırlı regex yok).
- **Eşleşme satırı** (Karar 10): ilk eşleşen satırın numarası ve eşleşmenin çevresinden ≤ 160 karakterlik alıntı (baştaki boşluk kırpılır) sonuçla saklanır ve "Match" sütununda görünür.
- `regex` crate'i zaten `gezik-batch`'in bağımlılığı (exe'de var). `perf-literal` özellikleri yalnız plan ölçüp bütçeye sığdığını gösterirse açılır; açılmazsa düz metin için `memchr` benzeri hızlı yol yazılmaz, regex motoru kullanılır.

### 3.4 Tarayıcı (`gezik-search::walk`)

- **Paralel yürüyüş:** paylaşılan bir klasör kuyruğu (`Mutex<VecDeque>` + `Condvar`) ve `gezik_core::ops::threads`'in seçtiği sayıda iş parçacığı (yerelde en çok 8, ağ yolunda 2). Her iş parçacığı bir klasörü `read_dir` ile okur, alt klasörleri kuyruğa ekler, öğeleri eşleştirir. Windows'ta okuma `FindFirstFileExW` (`FindExInfoBasic`, `FIND_FIRST_EX_LARGE_FETCH`) ile yapılır: `list_dir`'in öğe başına `metadata` çağrısı olmadan boyut, tarih ve öznitelik aynı kayıttan gelir. Unix'te `read_dir` + `d_type`; boyut/tarih yalnız bir ölçüt ya da sütun gerektiriyorsa ve öğe adla eşleştiyse `lstat` ile (eşleşmeyenler için çağrı yok).
- **Öncelik** (Karar 18): tarama iş parçacıkları düşük önceliklidir: Windows'ta `THREAD_MODE_BACKGROUND_BEGIN` (G/Ç önceliği de düşer), Linux'ta `ioprio_set(IOPRIO_CLASS_IDLE)` + `nice 10`, macOS'ta `setiopolicy_np(IOPOL_THROTTLE)` + QoS utility. Kopyalama işleri ve liste açılışı aramayla yarışmaz.
- **Bağlantılar** (Karar 9): sembolik bağlantılar, junction'lar ve bağlama noktaları izlenmez (`walk.rs`'teki kural); bağlantının kendisi adıyla sonuç olabilir. Böylece döngü olamaz; yine de derinlik 256'yı geçen yol "too deep" sayılıp atlanır.
- **Dosya sistemi sınırı** (Karar 9): Unix'te tarama kapsamın dosya sistemini geçmez (`st_dev` değişen klasöre inilmez; `find -xdev` gibi). Kapsam `/` ise `/proc`, `/sys`, `/dev`, `/run` zaten ayrı dosya sistemidir. macOS'ta `/System/Volumes/Data` aynı birimin ikinci görünümüdür: kapsam `/` iken atlanır. Windows'ta birim bağlama noktaları yeniden ayrıştırma noktasıdır, izlenmez.
- **Gizli ve sistem öğeleri** (Karar 8): `HiddenRule::FollowView` iken `[view] show-hidden` / `show-system` listedeki gibi uygulanır (`Entry::is_shown`); gizli bir klasörün altına da inilmez. `Include` iken hepsi.
- **Atlanan klasörler** (Karar 8): `[search] skip` (varsayılan `[".git", "node_modules"]`) adları tam adla (katlanarak) eşleşen klasörlere inilmez; klasörün kendisi adıyla sonuç olabilir. Durum çubuğu kaç klasörün atlandığını söyler (`Skipped 14 folders (search.skip)`); çubuktaki seçenek bu arama için atlamayı kapatır.
- **Hatalar:** okunamayan klasörler sayılır, ilk 50'si yoluyla ve nedeniyle tutulur; durum çubuğunda `· 3 folders could not be read` ve `Details` (işlem panelinin hata penceresi).
- **İptal:** her klasörden ve her içerik parçasından önce bayrağa bakılır; ağda takılan bir `read_dir` çağrısını bekleyen iş parçacığı terk edilir (çağrı dönünce sonucu atılır), arayüz beklemez.
- **Ağ yolları** (Karar 9): kapsam bir ağ yolundaysa (UNC, bağlı ağ sürücüsü, NFS/SMB bağlama) tarama yapılır ama 2 iş parçacığıyla; Everything kullanılmaz, ad önbelleği tutulmaz, klasör boyutu kendiliğinden hesaplanmaz.

### 3.5 Ad önbelleği (Karar 6)

- Arama çubuğu açıldığında, kapsamın yürüyüşü hemen (yazılmadan önce) arka planda başlar ve her öğenin adını, üst klasörünü, boyutunu, tarihini ve bayraklarını sıkışık bir yapıda tutar: adlar tek bir metin havuzunda, öğe başına ~28 bayt + ad. Ad, boyut, tarih ve tür ölçütleri bu önbellek üzerinde yazarken (150 ms'lik sessizlikten sonra) yeniden uygulanır; diske yeniden gidilmez.
- Sınır: 500.000 öğe (~35 MB). Sınır aşılırsa önbellek bırakılır ve arama "akış kipi"ne geçer: her Enter yeni bir yürüyüştür, yazarken canlı süzme yoktur (durum çubuğu `Large folder: press Enter to search`).
- Tek önbellek vardır (son kapsam). Sonuç sekmesi kapanınca, başka bir kapsamda arama başlayınca ya da 2 dakika kullanılmayınca bırakılır. Diske yazılmaz. F5 önbelleği yeniden kurar.
- İçerik araması önbelleği yalnız aday listesi olarak kullanır; dosyalar yine diskten okunur ve yalnız Enter ile başlar (yazarken içerik okunmaz).
- Ağ yolunda ve Everything kullanılırken önbellek tutulmaz.

### 3.6 Everything (yalnız Windows; Karar 11)

- **Bağlantı:** DLL yüklenmez, crate eklenmez. Everything'in herkese açık IPC'si (`everything_ipc.h`, sürüm 2 sorgusu `EVERYTHING_IPC_QUERY2`) doğrudan konuşulur: Everything'in gizli penceresi bulunur (`EVERYTHING_TASKBAR_NOTIFICATION`; 1.5 alfa için `EVERYTHING_TASKBAR_NOTIFICATION_(1.5a)`), sorgu `WM_COPYDATA` ile gönderilir, yanıt Gezik'in kendi iş parçacığındaki yalnız-ileti penceresine `WM_COPYDATA` ile gelir. Everything yönetici olarak çalışıyorsa yanıt UIPI'ye takılmasın diye pencerede `ChangeWindowMessageFilterEx(WM_COPYDATA, MSGFLT_ALLOW)`. Gerekli özellikler (`Win32_UI_WindowsAndMessaging`, `Win32_System_DataExchange`) `windows` crate'inde zaten açık. Sabitler başlıktan kopyalanır; ileti kurma ve yanıt çözme saf işlevdir (birim testli).
- **Ne zaman kullanılır:** `[search] everything = "auto"` (varsayılan; `"off"` hiç kullanmaz), Everything penceresi var, veritabanı yüklü (`EVERYTHING_IPC_IS_DB_LOADED`), kapsam yerel sabit bir sürücüde (`GetDriveTypeW == DRIVE_FIXED`) ve kapsam klasörünün kendisi Everything'de bulunuyor (tam yol sorgusuyla tek sonuçlu yoklama; dışlanmış klasör ya da dizinlenmemiş birim böyle anlaşılır). Bunlardan biri tutmazsa ya da yanıt 1 sn içinde gelmezse Gezik kendi tarayıcısına geçer; kullanıcıya hata gösterilmez, durum çubuğu yalnız kaynağı söyler.
- **Sorgu çevirisi:** kapsam `"<yol>\"` öneki; ad deseni Everything sözdizimine çevrilir (6a parçaları `|` ile, `!` önekiyle; jokerli parça `wfn:` ile tüm ada; regex için `regex:`); boyut `size:`, tarih `dm:`, tür `file:`/`folder:`. Çevrilemeyen bir durumda (ör. Türkçe i katlaması: Everything İ ile i'yi eşlemez) sorgu geniş tutulur ve sonuçlar Gezik'in kendi eşleştiricisinden bir kez daha geçirilir. Her durumda Gezik'in gizli/sistem ve `skip` kuralları Everything sonuçlarına da uygulanır, böylece iki kaynak aynı listeyi verir (Everything'in kendi dışlama ayarları dışında).
- **İstenen alanlar:** tam yol ve ad, boyut, değiştirilme ve oluşturulma tarihi, öznitelikler (`EVERYTHING_IPC_QUERY2_REQUEST_*`). Everything'de olmayan alan için öğe kendi tarayıcıyla değil, sonuç satırı çizilirken gereken alan bir `metadata` çağrısıyla tamamlanır.
- **İçerik:** Everything'in içerik araması kullanılmaz. İçerik varsa Everything ad/boyut/tarih adaylarını verir, içerik Gezik'in eşleştiricisiyle (§3.3) sınanır.
- **Yazarken:** Everything kullanılabiliyorsa ad araması her tuş vuruşunda (100 ms sessizlik) yeniden sorulur; önbellek tutulmaz.
- **Klasör boyutu (8b):** Everything "folder sizes" dizinliyorsa (`EVERYTHING_IPC_QUERY2_REQUEST_SIZE` klasörde değer döndürüyorsa) yerel sabit sürücülerdeki klasör boyutları ondan alınır.
- Everything 1.5'in adlandırılmış boru IPC'si (sürüm 3) bu adımda kullanılmaz; 1.5 alfa eski IPC'yi de yanıtladığı sürece çalışır. Plan, 1.4.1 ve 1.5 alfada yoklar ve sonucu not eder.

### 3.7 Sonuç kümesi

```rust
pub struct ResultSet {
    pub root: PathBuf,              // kapsam ("This PC"de boş; klasörler tam yol)
    pub folders: Vec<Box<str>>,     // root'a göre üst klasör yolları, tekil
    pub entries: Vec<Entry>,        // bugünkü Entry, boyutu değişmez
    pub parent: Vec<u32>,           // entries[i]'nin folders içindeki yeri
    pub matches: Option<Vec<Option<(u32, Box<str>)>>>, // içerik aramasında satır no + alıntı
}
```

- `Listing` yeni bir tür alır: `Listing::Results(Rc<ResultSet>)`. `path_at(i)` = `root/folders[parent[i]]/name`; `folder()` `None` döner (yapıştırma, yeni klasör, şablon gibi "buraya" işleri devre dışı kalır, §4.6).
- Bir sonucun kimliği göreli yoludur: `ViewState.selected` ve `focus` sonuç listesinde ad yerine göreli yol tutar (adlar tekil değildir). `indices_of`, `index_of` buna göre çalışır.
- Bellek: öğe başına `Entry` (64 bitte ~72 bayt; plan `size_of` ile yazar) + `parent` 4 bayt + ad; üst klasör yolları bir kez. 250.000 sonuç ≈ 25–30 MB.
- Sonuç kümesi arayüz iş parçacığında `Rc` ile tutulur; tarayıcı partileri `Send` olan ayrı vektörlerle gönderir, arayüz iş parçacığı yalnız ekler.

## 4. 8a — Arama

### 4.1 Açma ve kapsam

- **Eylem `search`** (Karar 1): Windows/Linux Ctrl+Shift+F ve F3, macOS ⌘⇧F. Gösterilen klasörde arama çubuğunu açar, kapsam o klasördür. "This PC"de kapsam bütün yerel sabit sürücülerdir (Karar 5). Arama çubuğu açıkken yeniden basmak ad alanına odaklanıp metni seçer.
- **Süzgeçten aramaya:** süzgeç çubuğunda yazılı bir desen varken Shift+Enter ya da çubuğun sağındaki `Search subfolders` düğmesi aynı deseni arama olarak başlatır (süzgeç kapanır).
- **Menüler:** klasör boşluğu menüsünde ve kenar çubuğu girdisinde `Search in this folder…`; klasör satırında `Search in "Ad"…`.
- **Kapsam seçici:** çubuğun solunda `in Work ▾`: `This folder` (başlangıç), üst klasörler (en çok 5), `Whole drive (D:)`, `This PC`. Seçim aramayı yeniden başlatır.

### 4.2 Arama çubuğu

Liste alanının üstünde, süzgeç çubuğunun yerinde tek satır (süzgeç çubuğu açılırsa altına gelir):

`[⌕ in Work ▾] [ad alanı .................] [Content] [Filters ▾ (2)] [Search | Stop] [▾]`

- **Ad alanı:** 6a'nın alanı ve hata satırı (`text-field.slint`); boşken soluk `Name, e.g. *.pdf;!*draft*`.
- **Content:** basılınca ad alanının yanında ikinci bir alan açar (`Text in files`); doluyken düğme vurgulu.
- **Filters ▾:** açılan katmanda `Size` (en az / en çok, metin alanları, `500 MB` biçimi), `Modified` (`Any`, `Today`, `Last 7 days`, `Last 30 days`, `This year`, `Between…` iki tarih alanı `YYYY-MM-DD`), `Type` (§3.1'deki liste), `Options`: `Name is a regular expression`, `Content is a regular expression`, `Match case`, `Include hidden items`, `Include skipped folders`. Düğmenin üstündeki sayı etkin ölçüt sayısıdır.
- **Search / Stop:** tarama sürerken `Stop`. Enter de başlatır; Esc sürerken durdurur, dururken çubuğu kapatır ve listeyi bırakır (sonuçlar kalır, liste klavyeyi alır). Alt+Enter aramayı yeni sekmede açar (Karar 2).
- **▾ menüsü:** 8a'da `Search in new tab`, `Clear`; 8b kayıtlı aramaları buraya ekler (§8).
- Alanlar Slint'te `filter-bar.slint` ile aynı bileşenlerden kurulur; std-widgets kullanılmaz (7c'nin exe dersi).

### 4.3 Akış

- **Canlı:** önbellek (§3.5) ya da Everything varken ad/boyut/tarih/tür ölçütleri yazarken uygulanır; liste her 150 ms'de güncellenir. İçerik ölçütü varken ya da akış kipinde arama Enter ile başlar.
- **Partiler:** tarayıcı bulduklarını 100 ms'de ya da 2.000 öğede bir parti olarak gönderir; arayüz ekler ve satırları bildirir (`row_added`), modeli sıfırlamaz. Kaydırma ve seçim yerinde kalır.
- **Sıra** (Karar 7): arama sürerken sonuçlar bulundukları sırayla eklenir, sütun başlığı sıralamanın arama bitince uygulanacağını ipucunda söyler. Bitince sıralama arka plan iş parçacığında yapılır (adlar ve anahtarlar orada), arayüz yalnız gelen sıra dizisini (`Vec<u32>`) uygular; odak ve seçim göreli yolla korunur, odaktaki satır görünür kalır. Sürerken sütun başlığına tıklamak o ana kadarkileri hemen sıralar ve sonraki partiler sıralı birleştirilir.
- **Sınır** (Karar 7): `[search] max-results = 250000`. Dolunca tarama durur, durum çubuğu `Stopped at 250,000 results. Narrow the search.`
- **Durum çubuğu:** sürerken `Searching… 12,345 found · 48,210 folders` (+ `via Everything`), bitince `1,234 results in 2.4 s` (+ atlanan ve okunamayan klasörler). Seçim varken bugünkü seçim özeti.
- **Biten aramanın iş parçacıkları** hemen biter; önbellek §3.5'e göre kalır.

### 4.4 Sonuç sekmesi

- **Konum:** `Location::Search(Box<SearchSpec>)`. Arama, etkin sekmede bir gezinme adımıdır (Karar 2): Geri arama öncesindeki klasöre döner, İleri sonuçlara. Sonuçtan bir klasöre girmek normal gezinmedir; Geri sonuçlara döner.
- **Sonuçların saklanması** (Karar 12): her sekme kendi son arama sonucunu bellekte tutar; Geri/İleri ve sekme değiştirme ile o sonuca dönmek yeniden aramaz. Geçmişteki daha eski arama adımlarına dönmek aramayı yeniden çalıştırır. Bellek sınırı tek sekme için §3.7.
- **Başlık ve yol:** sekme başlığı `Search: *.pdf` (içerik varsa `Search: "fatura"`); kırıntı yolu kapsam klasörünün kırıntıları ve sonda `Search "*.pdf"`; bir kırıntıya tıklamak o klasöre gider. Adres çubuğuna yol yazmak aramadan çıkar.
- **Oturum** (Karar 12): `state.toml`'da sekme `search = { … }` olarak (§9.2) saklanır; açılışta etkin sekmeyse arama yeniden çalışır, değilse sekmeye geçilince. Sonuçlar diske yazılmaz.
- **Görünüm hafızası:** sonuç ve düz görünüm listeleri `views.toml`'da tek bir ortak kayıt tutar (`<results>` anahtarı): görünüm kipi, sıralama, sütunlar.

### 4.5 Sütunlar, sıralama, süzgeç, önizleme

- **Yeni sütunlar** (`ColumnKey`): `Folder` (göreli üst klasör, `sub\dir`; kapsam This PC iken tam yol) ve `Match` (içerik aramasında `12: …alıntı…`). İkisi yalnız sonuç ve düz görünüm listelerinde seçilebilir; varsayılan sütunlar `Name`, `Folder`, `Modified`, `Size` (+ içerik varken `Match`).
- **Sıralama:** `SortKey::Folder` eklenir (doğal sıra, sonra ad). Diğer anahtarlar bugünkü gibi; `folders-first` geçerli. 50.000'den büyük listelerde sıralama arka planda (§4.3'teki gibi).
- **Izgara** kipi de çalışır (küçük resimler bugünkü gibi yalnız görünenler için).
- **Süzgeç** (Ctrl+F) sonuçların içinde süzer ("sonuçlar içinde arama"); `filtered_listing` `Results` için aynı kuralla satır dizisi döndürür.
- **Önizleme ve Hızlı Bakış** yola göre çalıştığından değişmeden çalışır. Önizlemede eşleşme vurgusu kapsam dışı.
- **Harfle atlama** ada göre (klasörü değil).

### 4.6 Sonuçlarda işlemler

- **Kopyala/kes/sürükle:** seçili öğelerin tam yollarıyla, bugünkü gibi. Bir klasöre yapıştırmak ya da bırakmak öğeleri **düz** koyar (Explorer gibi); aynı adlar bugünkü çakışma listesine düşer.
- **Klasör yapısını koruyarak** (Karar 14; TC "keep relative paths"): satır menüsünde `Copy with folders` ve `Cut with folders` (eylemler `copy-with-folders`, `cut-with-folders`, varsayılan tuş yok). Pano bu bayrakla birlikte kapsam kökünü tutar; yapıştırınca her öğe hedefte göreli yoluyla oluşturulur (`D:\Work\a\b\x.txt`, kök `D:\Work` → `Hedef\a\b\x.txt`). İş motorunda `CopyTask`/`MoveTask` öğe başına hedef göreli yolu alan yeni bir kurucu alır; eksik ara klasörler `Outcome::Created` olarak oluşturulur, böylece geri alma onları da kaldırır (7c'nin geri alma sırası kuralı burada da geçerli). Gezik'in panosu dışına (Explorer'a) çıkan kopya düz kalır.
- **Çöpe atma, kalıcı silme, yeniden adlandırma** bugünkü işlerle; satır içi ad değiştirmede ad çakışması iş motorunun hatasıyla söylenir (sonuç listesinde klasörün diğer adları bilinmez).
- **Toplu ad:** `Item.folder` zaten öğelerin farklı klasörlerde olmasını destekliyor (`gezik-batch::rename`, klasör başına numara). Değişen: "seçilmeyen adlar" (`Rules.others`) klasör başına tutulur ve katman açılırken her klasör arka planda okunur (okunana kadar "Rename" bekler, bugünkü `waiting` gibi). `check`'in `existing(folder, name)` kapanışı artık klasörü kullanır.
- **Show in folder** (eylem `show-in-folder`, Ctrl+Shift+E / ⌘⇧E; satır menüsünde `Show in folder`): odaktaki sonucun klasörünü aynı sekmede açar ve öğeyi seçer (Geri sonuçlara döner). Alt+Enter gibi yeni sekmede açmak için satır menüsünde `Show in folder in new tab`.
- **Devre dışı:** `Paste`, `New folder`, `New ▸`, `Paste image as file`, `New folder with selection` ve sonuç listesinin boşluğuna bırakma; durum çubuğu `Not in search results: open a folder first`. Bir klasör satırına bırakmak çalışır.
- **Windows Explorer menüsü** (Karar 14): seçimin hepsi tek klasördeyse bugünkü gibi Explorer menüsü (o klasörün `IShellFolder`'ıyla); farklı klasörlerdeyse yalnız Gezik'in öğeleri (`CDefFolderMenu` tek üst klasör ister). macOS ve Linux'ta Gezik'in menüsü zaten tektir.
- **Arşiv, dönüştürme, kullanıcı komutları** yol listesiyle çalıştığından çalışır; çıktının "buraya" yazıldığı işler (ör. `Extract here`, `Compress`) her öğenin kendi klasörünü hedef alır, tek hedef isteyenler (`Compress to…`) bugünkü klasör seçimini sorar.

### 4.7 Güncel tutma

- Gezik'in kendi işleri sonuç listesine yansır: iş motorunun çıktıları (`Outcome`) arayüzde zaten işleniyor; `Results` listesinde çöpe atılan, silinen ve başka yere taşınan öğeler satırdan çıkar, yeniden adlandırılanın adı güncellenir, geri alma bunları geri getirir (çıktının yolu kapsamın altındaysa yeni satır olarak eklenir).
- Dışarıdan değişiklikler izlenmez (Karar 12 ile birlikte; kapsamı özyinelemeli izlemek pahalı). F5 aramayı yeniden çalıştırır (önbellek de yenilenir). Bir satırın dosyası artık yoksa önizleme bunu söyler, işlem bugünkü "no longer exists" yolundan geçer.

## 5. 8a — Düz görünüm

- **Eylem `flat-view`** (Karar 13): Ctrl+B / ⌘B (TC'nin Branch View'u). Gösterilen klasörün bütün alt klasörlerindeki **dosyaları** tek listede gösterir; klasörler satır olarak çıkmaz, `Folder` sütununda görünür. Düz görünümde yeniden basmak klasöre döner; odaktaki dosya varsa onun klasörü açılır ve dosya seçilir (TC'deki gibi).
- Aynı `SearchSpec` (`flat = true`, desen boş) ve aynı sonuç listesi: sütunlar, sıralama, süzgeç, önizleme, işlemler, `Copy with folders`, toplu ad, sınır (`max-results`) ve akış §4'teki gibidir. Ad önbelleği varsa ondan anında gelir.
- `Location::Flat(PathBuf)`; sekme başlığı `Work (all files)`, kırıntıların sonunda `All files`. Oturumda `flat = "…"`.
- Görünüm menüsünde `Flat view` (işaretli); "This PC"de devre dışı (`Flat view works in folders`).
- Gizli/sistem ve `skip` kuralları aramadaki gibi; `View ▸ Show hidden items` değişince liste yeniden kurulur.

## 6. 8b — Klasör boyutu

### 6.1 Ne zaman hesaplanır (Karar 15)

- `[view] folder-sizes = "local"` (varsayılan): gösterilen klasörün alt klasörleri yerel sabit ve çıkarılabilir sürücülerde kendiliğinden hesaplanır; ağda hesaplanmaz. `"all"` ağda da hesaplar; `"off"` hiç hesaplamaz (bugünkü gibi).
- Eylem `calculate-folder-sizes` (varsayılan tuş yok; satır menüsünde ve Görünüm menüsünde `Calculate folder sizes`) ayardan bağımsız olarak seçili klasörlerin, seçim yoksa gösterilen klasördeki bütün klasörlerin boyutunu hesaplar.
- Önce ekranda görünen satırlar, sonra diğerleri; 2 iş parçacığı (ağda 1), düşük öncelik (§3.4). Klasörden çıkınca bekleyenler iptal edilir, bitenler önbellekte kalır.

### 6.2 Ne gösterilir

- `Size` sütununda klasörün içindeki dosyaların mantıksal boyut toplamı (`size-format` ile); hesaplanırken `…`, hesaplanmamışsa boş. Bağlantılar izlenmez (§3.4), sabit bağlantılar her görüldüğü yerde sayılır, okunamayan alt klasör varsa değer `≥ 1.2 GB` yazılır ve ipucu nedenini söyler.
- Önizleme paneli klasör için boyutu ve dosya/klasör sayısını gösterir (bugünkü 10.000'lik sayım sınırı hesaplanan değer gelince kalkar).
- Seçim özeti seçili klasörlerin bilinen boyutlarını da toplar; bilinmeyen varsa `+` ile (`3 items · 1.2 GB+`).
- Everything kullanılabiliyorsa (§3.6) yerel sabit sürücüde boyut ondan, anında gelir.

### 6.3 Beklemeden sıralama

- Boyuta göre sıralarken boyutu bilinen klasörler boyutlarıyla sıralanır, bilinmeyenler (iki yönde de) en sona gelir; `folders-first` açıkken bu klasörler arasında olur.
- Boyutlar geldikçe liste en çok saniyede bir yeniden sıralanır; odak ve seçim korunur, odaktaki satır ekrandaki yerinde tutulur (kaydırma ona göre ayarlanır). Kullanıcı son 1 sn içinde kaydırdıysa ya da fare listedeyse yeniden sıralama 1 sn ertelenir. Bütün boyutlar gelince son bir kez sıralanır.
- Sıralama 50.000'den büyük listelerde arka planda (§4.3'teki yol).

### 6.4 Önbellek ve geçersizleme

- Yalnız bellekte (diske yazılmaz): yol → (boyut, dosya sayısı, klasör sayısı, eksik mi, hesaplandığı an). En çok 50.000 klasör; dolunca en eski kullanılan düşer.
- **Geçersizleme:** Gezik'in her işi, çıktılarındaki yolların bütün üst klasörlerinin kaydını düşürür. Gösterilen klasörün izleyicisi (`folder_watch.rs`, özyinelemeli değil) bir alt klasörün değiştiğini söylerse o kayıt düşer. Bunların dışındaki derin değişiklikleri yakalamak için 5 dakikadan eski kayıtlar klasör yeniden gösterilince arka planda yeniden hesaplanır (eski değer o arada soluk çizilir). F5 gösterilen klasörün bütün kayıtlarını düşürür.
- Arama yürüyüşü (§3.4) ve düz görünüm yürüdükleri ağaçların klasör boyutlarını da toplar ve önbelleğe yazar (ek okuma olmadan).

## 7. 8b — Komut paleti ve hızlı açma

### 7.1 Açma (Karar 16)

- `command-palette`: Ctrl+Shift+P / ⌘⇧P; alan `>` ile başlar, yalnız eylemler ve komutlar listelenir.
- `quick-open`: Ctrl+P / ⌘P; alan boş başlar, önce yerler (klasörler), sonra diğerleri. `>` yazmak komut kipine geçirir, silmek geri döndürür.
- Görünüş ve klavye 6a'nın sekme seçicisinden (`tab-picker.slint`) gelir: pencerenin üstünde katman, alan, en çok 10 görünen satır, Yukarı/Aşağı, Enter, Esc; tıklamak seçer. Satırda başlık, soluk alt metin (yol, açıklama), sağda tür etiketi ve varsa kısayol (`Ctrl+Shift+C`).

### 7.2 Neler listelenir

| Tür | Kaynak | Enter |
|---|---|---|
| Action | Bütün `Action`'lar (yeni `Action::title()`: macOS menü çubuğunun başlıkları ortak yere taşınır) | Eylemi çalıştırır (`actions::run` ya da kısayolunu oynatır, menü çubuğundaki yol) |
| Command | `[[commands]]` | Komutu seçime uygular |
| View option | Görünüm menüsünün işaretli öğeleri (`Show hidden items` …) | Değiştirir |
| Pinned | Sabitlenen klasörler (takma adıyla) | Etkin sekmede gider; Alt+Enter yeni sekmede |
| Recent | 6b'nin klasör geçmişi, en çok 50 | Aynı |
| Tab | Açık sekmeler | O sekmeye geçer |
| Tab set | `[[tab-sets]]` | Açar |
| Saved search | `[[searches]]` (§8) | Çalıştırır |
| Saved filter | `[[filters]]` | Süzgeci uygular |
| Search | Son satır: `Search for "metin" in Work` | Metinle arama başlatır (§4) |

- Gizli bilgi olmasın diye dosya listelenmez (dizin yok; hızlı açma yer içindir). Dosyaya gitmek için son satırdaki arama.
- O an çalışamayacak eylemler (ör. "This PC"de `new-folder`) listelenir ve çalıştırılınca bugünkü durum çubuğu notunu verir; ayrıca soluk çizilmez (kurallar eylem başına dağınık, tekrar yazılmaz).

### 7.3 Eşleştirme ve sıra

- Yazılan metin boşluklarla sözcüklere bölünür; her sözcük başlıkta ya da alt metinde (6a'nın `fold_text` katlamasıyla) geçmelidir (VE). Desen dilindeki `*`, `?`, `;`, `!` burada düz karakterdir (palet için bulanık değil, öngörülebilir).
- Sıra: başlığın başıyla eşleşen > bir sözcüğün başıyla eşleşen > içinde geçen; eşitlikte son kullanılanlar önce (`state.toml` `[palette] recent`, en çok 20 öğe, tür + kimlik), sonra türlerin tablodaki sırası (hızlı açmada yerler önce), sonra alfabetik.
- Saf işlev (`gezik-core::palette`): öğeler, metin, son kullanılanlar → sıralı dizinler; birim ve 2.000 öğelik süre testi.

## 8. 8b — Kayıtlı aramalar

- **Biçim** (`settings.toml`, taşınabilir; Karar 17):

  ```toml
  [[searches]]
  name = "Large videos"
  folder = "{home}"          # or "{here}": the folder shown when it is run
  pattern = "*.mp4;*.mkv"
  # content = "invoice"     # text in files
  # name-regex = false
  # content-regex = false
  # match-case = false
  size-min = "500 MB"
  # size-max = "4 GB"
  modified = "30d"           # today | 7d | 30d | year | 2026-01-01..2026-06-30
  type = "videos"            # files | folders | pictures | videos | audio | documents | archives | code
  # hidden = false
  ```

  Yollar `KnownDirs::collapse` belirteçleriyle (`{home}`, `{documents}` …); `"{here}"` çalıştırılınca gösterilen klasördür ("This PC"de bütün sürücüler). `"drives"` This PC'dir. Boş `name`, `..` içeren yol, bilinmeyen anahtar ve hatalı değer uyarıyla atlanır; ad büyük/küçük harf ayrımsız tekildir; en çok 30.
- **Kaydetme:** arama çubuğunun ▾ menüsünde `Save search…` (ve `save-search` eylemi, tuşsuz): ad sorulur, aynı ad varsa `Replace?`. Kapsam sorusu: `Save with this folder` / `Save for any folder ({here})` (iki düğmeli soru). Yazma `settings_writer`'dan (`SettingsChange::Searches`), elle yazılmış girdiler korunur (`[[filters]]` ve `[[tab-sets]]`'in yolu).
- **Çalıştırma:** ▾ menüsünde kayıtlı aramaların listesi (tıklamak çalıştırır) ve `Delete "Ad"` alt listesi (süzgeç menüsünün düzeni); palette `Saved search` türü; kenar çubuğunda en az bir kayıtlı arama varsa `SEARCHES` başlığı altında, sabitlenenlerden sonra (Smart Folder). Kenar çubuğu girdisinin menüsü: `Run in new tab`, `Rename…`, `Delete`. Sürükleyerek sıralama yok (sıra dosyadaki sıra).
- Kayıtlı aramayla açılan sonuç sekmesinin başlığı kaydın adıdır.

## 9. Ayarlar, durum ve kısayollar

### 9.1 `settings.toml`

```toml
[search]
everything = "auto"        # Windows: use Everything for names when it runs; "off": never
skip = [".git", "node_modules"]   # folders a search does not go into (by name)
max-results = 250000
content-max-size = "64 MB" # larger files are not read for text

[view]
folder-sizes = "local"     # off | local (not on network folders) | all

# [[searches]] — see section 8
```

Hatalı değerler bugünkü kalıpla uyarı verir (`search.everything: expected "auto" or "off", got …`, `search.max-results: expected a number from 1000 to 2000000`, `view.folder-sizes: expected "off", "local" or "all"`).

### 9.2 `state.toml`

```toml
[[session.tabs]]
search = { folder = "D:/Work", pattern = "*.pdf", content = "fatura", modified = "30d" }

[[session.tabs]]
flat = "D:/Photos"

[palette]
recent = ["action:copy-path", "pinned:D:/Work/gezik", "search:Large videos"]
```

`search` tablosu `[[searches]]` ile aynı anahtarları taşır (`name` hariç; `folder` mutlak, `{here}` yok). Tanınmayan girdi sessizce atlanır (`State::parse` kuralı); eski Gezik `search`/`flat` sekmesini tanımaz ve atlar (oturumun geri kalanı gelir).

### 9.3 Yeni eylemler ve varsayılan tuşlar

| Eylem | Windows / Linux | macOS | Parça |
|---|---|---|---|
| `search` | Ctrl+Shift+F, F3 | ⌘⇧F | 8a |
| `flat-view` | Ctrl+B | ⌘B | 8a |
| `show-in-folder` | Ctrl+Shift+E | ⌘⇧E | 8a |
| `copy-with-folders`, `cut-with-folders` | — | — | 8a |
| `command-palette` | Ctrl+Shift+P | ⌘⇧P | 8b |
| `quick-open` | Ctrl+P | ⌘P | 8b |
| `calculate-folder-sizes`, `save-search` | — | — | 8b |

- `Action` listesi 63 → 68 (8a) → 72 (8b). Çakışma denetimi: 6a/6b/7 varsayılanlarıyla (`mod+f`, `mod+shift+t/a/n/i/s/c/1/2`, `mod+1…9`, `alt+1…9`, `shift+f4`, `f2`, `f5`, `ctrl+h` …), `fixed_owner` ve macOS menü çubuğunun sabitleriyle çakışmaz; Ctrl+Alt kullanılmaz (AltGr kuralı). F3 macOS'ta sistemindir (Mission Control), bu yüzden yalnız Windows/Linux'ta.
- Hepsi `settings.toml` şablonundaki `[shortcuts]` yorumlarına, macOS menü çubuğuna (Edit: `Find…` = `search`; View: `Flat View`, `Calculate Folder Sizes`; Go: `Show in Folder`, `Quick Open…`, `Command Palette…`) ve "her varsayılan Windows ve Linux'ta ulaşılabilir" testine eklenir.

### 9.4 Menü kimlikleri

7c 1000–1459'u kullanır. 8a **1500–1599**: `SEARCH_HERE` 1500, `SHOW_IN_FOLDER` 1501, `SHOW_IN_FOLDER_NEW_TAB` 1502, `COPY_WITH_FOLDERS` 1503, `CUT_WITH_FOLDERS` 1504, `FLAT_VIEW` 1505, `SEARCH_NEW_TAB` 1506, `SEARCH_CLEAR` 1507, kapsam seçici 1510–1519, `Modified` hazırları 1520–1529, `Type` 1530–1539, seçenekler 1540–1549. 8b **1600–1699**: `CALC_FOLDER_SIZES` 1600, `SAVE_SEARCH` 1601, kenar çubuğu kaydı `RUN_SEARCH_NEW_TAB` 1602, `RENAME_SEARCH` 1603, `DELETE_SEARCH` 1604, kayıtlı aramalar 1610–1639, silme 1640–1669. `GEZIK_IDS_END = 4096` değişmez.

## 10. Kod yapısı

| Parça | Yer |
|---|---|
| Sorgu modeli, metin biçimi (boyut, tarih aralığı, tür), `Location::Search`/`Flat`, oturum sekmesi | `gezik-core/src/search.rs` (yeni), `gezik-core/src/nav.rs` |
| Metin algılama (BOM, NUL, BOM'suz UTF-16), önizlemeyle ortak | `gezik-core/src/text.rs` (yeni; `gezik/src/preview.rs`'teki `decode_text` buraya taşınır) |
| Palet sıralaması | `gezik-core/src/palette.rs` (yeni, 8b) |
| Yeni crate: tarayıcı, ad ve içerik eşleştiricileri, ad önbelleği, sonuç kümesi, Everything iletileri, klasör boyutu ve önbelleği | `crates/gezik-search/src/{lib, walk, name, content, cache, results, everything, size}.rs` (yeni; bağımlılıklar `gezik-core`, `gezik-platform`, `regex`) |
| Everything penceresi ve `WM_COPYDATA`, `FindFirstFileExW` okuyucu, sürücü türü, iş parçacığı önceliği | `gezik-platform/src/{everything, priority}.rs` (yeni), `gezik-platform/src/fs/{windows, unix}.rs`, `drives.rs` |
| Göreli hedefli kopya/taşı, ara klasörlerin `Created` çıktısı | `gezik-ops/src/tasks/{copy, move_}.rs` |
| `Listing::Results`, `Folder`/`Match` sütunları, `SortKey::Folder`, arka plan sıralaması, sonuç kimliği | `gezik/src/view/{mod, listing, model}.rs`, `gezik-core/src/{view, sort}.rs` |
| Arama çubuğu, akış, sonuç sekmesi, düz görünüm, kayıtlı aramalar | `gezik/src/search.rs` (yeni), `gezik/ui/widgets/search-bar.slint` (yeni), `gezik/src/navigation.rs`, `gezik/src/sidebar.rs` (8b) |
| Toplu adın klasör başına `others`'ı | `gezik/src/batch_rename.rs` |
| Pano bayrağı (klasörlerle kopya), sonuçta devre dışı işler, Explorer menüsü kuralı | `gezik/src/operations.rs`, `gezik/src/context_menu.rs`, `gezik-platform/src/shell_menu.rs` |
| Klasör boyutu sütunu, sıralama, seçim özeti, önizleme sayısı | `gezik/src/folder_sizes.rs` (yeni, 8b), `gezik/src/view/*`, `gezik/src/preview.rs` |
| Palet | `gezik/src/palette.rs` (yeni, 8b), `gezik/ui/widgets/tab-picker.slint` genelleşir (`picker.slint`) |
| Ayarlar ve durum (`[search]`, `[[searches]]`, `folder-sizes`, oturum sekmeleri, `[palette]`), eylemler ve başlıklar | `gezik-config/src/{settings, settings_edit, settings_writer, shortcuts}.rs`, `templates/settings.toml`, `gezik/src/{keys, actions, menu_bar}.rs` |

## 11. Test

- **Birim (saf, `gezik-core` ve `gezik-search`):**
  - Sorgu: boyut ve tarih metinlerinin ayrıştırılması/yazılması (`500 MB`, `1.5 GB`, `kB`, `7d`, `year`, aralık, hatalı değerler), `Between`'in gün sınırları, `KindFilter` eşlemesi, boş sorgu.
  - Ad: 6a deseni (zaten testli) + regex, `(?i)` ve Türkçe i sınıfı, geçersiz regex hatası.
  - İçerik: UTF-8, UTF-8 BOM, UTF-16 LE/BE BOM'lu ve BOM'suz, ANSI (Windows), ikili atlama, parça sınırında eşleşme, 1 MB'lık satır bölünmesi, `content-max-size`, Türkçe katlama (`İSTANBUL` ~ `istanbul`, `ılık` ~ `ILIK`), `Match case`, alıntının 160 karakter sınırı ve satır numarası, uzantıyla atlanan türler.
  - Tarayıcı (geçici ağaçlarda): `skip` ve `Include skipped folders`, gizli/sistem kuralları, bağlantının izlenmemesi (Unix'te üst klasöre symlink döngüsü, Windows'ta junction döngüsü; ikisi de sonlanır), derinlik sınırı, okunamayan klasörün sayılması, `max-results`'ta durma, iptalin ≤ 100 ms'de sonuçlanması, Unix'te `st_dev` sınırı (sahte cihaz numarasıyla saf kural), parti boyutu.
  - Ad önbelleği: 500.000 sınırında akış kipine geçiş, yeniden süzmenin diske gitmemesi, bırakılma kuralları.
  - Sonuç kümesi: `path_at`, göreli yol kimliği, `indices_of`, `Folder` sıralaması, sıra dizisinin uygulanması, iş çıktılarıyla satır çıkarma/güncelleme/ekleme.
  - Everything: `QUERY2` iletisinin kurulması, yanıtın çözülmesi (kayıtlı örnek baytlarla), desen → Everything sorgusu çevirisi, geri düşüş kararları (pencere yok, veritabanı yüklü değil, sürücü türü, yoklama boş, zaman aşımı).
  - Klasör boyutu: bilinmeyenlerin sonda sıralanması, yeniden sıralama sıklığı ve erteleme kuralı, odak koruma, önbellek geçersizleme (üst klasörler, izleyici, 5 dakika), `≥` eksik değer, seçim özeti `+`.
  - Palet: VE eşleştirmesi, katlama, sıra kuralları, son kullanılanlar, `>` kipi, 2.000 öğede ≤ 5 ms (`--ignored` süre testi).
  - Ayarlar ve durum: `[search]`, `[[searches]]` (`{here}`, belirteçler, yinelenen ad, 30 sınırı), `folder-sizes`, oturumda `search`/`flat` sekmeleri ve eski sürümün bunları atlaması, `[palette] recent`.
  - Kısayollar: yeni varsayılanların çakışmaması, ulaşılabilirlik testi.
  - İş motoru: göreli hedefli kopya ve taşıma, ara klasörlerin oluşturulması ve geri almada kaldırılması, çakışma listesi.
- **Platform testleri:** Windows'ta `FindFirstFileExW` okuyucusunun `list_dir` ile aynı öğeleri vermesi; Everything kurulu makinede (`#[ignore]`, elle) gerçek sorgu ve yoklama; iş parçacığı önceliğinin ayarlanabilmesi (üç sistemde hata vermemesi).
- **Docker yok.** Linux'ta gerçek makinede, macOS'ta MacBook'ta elle kontrol listeleri; sonuçlar `docs/superpowers/notes/macos-test-results.md` ve Linux notuna.
- **Windows ekran testleri** (her planın sonunda kontrol listesi, kullanıcı uzaktayken; klavye düzenine dokunulmaz): Everything'li ve Everything'siz makinede aynı aramanın aynı sonuçları vermesi, Everything'i aramanın ortasında kapatmak, yönetici olarak çalışan Everything, ağ sürücüsünde arama, junction'lı ağaç, `C:\` üzerinde düz görünümün sınırda durması, sonuçlarda Explorer menüsü (tek klasör / çok klasör), `Copy with folders` + Ctrl+Z, toplu ad çok klasörde, Türkçe Q'da yeni kısayollar, klasör boyutuna göre sıralarken kaydırma.
- **macOS kontrol listesi** (`macos-test.md`'ye eklenir): ⌘⇧F, ⌘B, ⌘⇧P, ⌘P, ⌘⇧E; ev klasöründe aramada TCC izin pencereleri ve okunamayan `~/Library` klasörlerinin sayılması; `/` kapsamında `/System/Volumes/Data` atlanması; menü çubuğu öğeleri.
- **Linux kontrol listesi** (gerçek makine): `/` kapsamında `/proc`, `/sys` ve bağlı diskleri geçmeme, NFS/SMB bağlamasında 2 iş parçacığı, X11 ve Wayland'de palet ve arama çubuğu odakları, `ioprio` etkisi (arama sürerken kopya hızı).

## 12. Performans ve bütçe

- **Ölçüm betikleri:** `scripts/perf/search.ps1` (yeni): geçici klasörde bir kez kurulan 1.000.000 boş dosyalık ağaç (1.000 klasör × 1.000 dosya, iç içe üç düzey); ad araması (ilk sonuç süresi, toplam süre, tepe ve kapanış sonrası bellek), içerik araması (10.000 küçük metin dosyası), düz görünüm, Everything'li ve Everything'siz. `measure.ps1` ve `stress.ps1` her planın sonunda (boşta bellek, açılış, 100.000 dosyada kaydırma; 8b'de `folder-sizes = "local"` ile).
- **Hedefler:** §1'deki ölçütler. Arayüz iş parçacığında: parti uygulaması ≤ 4 ms, sıra dizisi uygulaması 250.000 öğede ≤ 15 ms, 150 ms'lik canlı süzme önbellekte 500.000 öğede arka planda ≤ 100 ms (sonucu arayüze tek parti).
- **Bellek:** ad önbelleği ≤ 35 MB (500.000), sonuçlar ≤ 35 MB (250.000), klasör boyutu önbelleği ≤ 5 MB (50.000), palet öğeleri açıkken birkaç yüz KB. Hepsi ilk kullanımda ayrılır; arama yokken boşta bellek değişmez.
- **Exe** (Karar 18): her parça ≤ +0,25 MiB, 7c birleştikten sonraki `master`'ın sürüm derlemesine göre (plan 8a ilk adımda bayt olarak ölçüp yazar). Yeni crate bağımlılığı yok; `gezik-search` yalnız var olan `regex`'i kullanır. 7c'nin ölçümüne göre büyümenin çoğu Slint arayüzünden gelir: arama çubuğu ve palet var olan bileşenlerden (süzgeç çubuğu, sekme seçici) kurulur, std-widgets kullanılmaz. Plan her görevden sonra ölçer; sınır aşılırsa önce arayüz sadeleşir.

## 13. Kararlar (kullanıcı onayı bekleyen)

Her madde önerilen seçimdir; ayraçta seçenek. Onaylanmayan madde plandan önce değişir.

1. **Arama tuşu:** `search` Ctrl+Shift+F ve F3 (macOS ⌘⇧F); Ctrl+F süzgeçte kalır; süzgeç çubuğunda Shift+Enter / `Search subfolders` süzgeci aramaya çevirir. (Seçenek: Ctrl+F aramayı açsın, süzgeç `/`'de kalsın.)
2. **Sonuçlar nerede:** etkin sekmede bir gezinme adımı (Geri klasöre döner); Alt+Enter ve ▾ menüsü yeni sekmede. (Seçenek: her arama yeni sekme.)
3. **Sorgu:** ad alanında 6a desen dili, diğer ölçütler çubuğun denetimleriyle (Content, Size, Modified, Type, Options); `size:>10mb` gibi metin sözdizimi yok. (Seçenek: anahtar sözdizimi de.)
4. **Regex ve büyük/küçük harf:** ad ve içerik için ayrı "regular expression" seçeneği (Rust regex); varsayılan harf duyarsız, Türkçe İ/ı katlamalı; `Match case` seçeneği.
5. **Kapsam:** gösterilen klasör ve altı; "This PC"de bütün yerel sabit sürücüler; kapsam seçici üst klasörlere, sürücü köküne ve This PC'ye geçirir.
6. **Canlı arama ve ad önbelleği:** çubuk açılınca yürüyüş arka planda başlar, ad/boyut/tarih yazarken bellekteki önbellekten süzülür (≤ 500.000 öğe, ~35 MB; kapanınca ya da 2 dk boşta bırakılır); içerik araması Enter ile. (Seçenek: önbellek yok, her arama Enter ile yeni yürüyüş.)
7. **Sınır ve sıra:** en çok 250.000 sonuç (`[search] max-results`); arama sürerken bulunan sırayla, bitince arka planda sıralanır (sürerken başlığa tıklamak hemen sıralar).
8. **Gizli/atlanan:** `[view]`'ın gizli/sistem kuralına uyar; `[search] skip = [".git", "node_modules"]`; `.gitignore` okunmaz; ikisi de arama başına seçenekle açılır.
9. **Bağlantılar ve dosya sistemleri:** symlink/junction/bağlama noktası izlenmez (döngü olmaz); Unix'te tek dosya sistemi (`-xdev`), macOS'ta `/System/Volumes/Data` atlanır; ağda arama var ama 2 iş parçacığı, Everything ve önbellek yok.
10. **İçerik:** metin testinden geçen her dosya (resim/video/arşiv/PDF/Office uzantıları okunmadan atlanır), ≤ 64 MB; UTF-8, UTF-16 (BOM'lu ve BOM'suz), ANSI; ilk eşleşen satır "Match" sütununda.
11. **Everything:** DLL ve crate yok, `WM_COPYDATA` IPC'si doğrudan (1.4 ve 1.5a); yalnız yerel sabit sürücü ve kapsam Everything'de bulunuyorsa; 1 sn'de yanıt yoksa kendi tarayıcı; `[search] everything = "auto" | "off"`; içerik ve gizli/atlama kuralları hep Gezik'te.
12. **Oturum ve geçmiş:** arama/düz görünüm sekmeleri sorgusuyla saklanır, gösterilince yeniden çalışır; sonuçlar diske yazılmaz; her sekme son sonucunu Geri için bellekte tutar; dış değişiklikler F5'e kadar yansımaz.
13. **Düz görünüm:** Ctrl+B / ⌘B açıp kapatır; yalnız dosyalar (klasör `Folder` sütununda); aramayla aynı liste ve sınır.
14. **Sonuçlarda işlemler:** Ctrl+C/V düz kopyalar; `Copy/Cut with folders` göreli yolları korur (geri alınabilir); Windows'ta Explorer menüsü yalnız seçim tek klasördeyse; `Show in folder` Ctrl+Shift+E / ⌘⇧E.
15. **Klasör boyutu:** `[view] folder-sizes = "local"` (ağda kendiliğinden yok); bilinmeyenler sonda, yeniden sıralama en çok saniyede bir; yalnız bellekte önbellek (50.000 klasör, 5 dk); mantıksal boyut; Everything varsa ondan.
16. **Palet:** Ctrl+Shift+P (`>` eylemler) ve Ctrl+P hızlı açma (önce yerler); eylemler, komutlar, görünüm seçenekleri, sabitlenenler, son klasörler, sekmeler, sekme setleri, kayıtlı arama ve süzgeçler; sözcük-VE eşleştirme; son 20 kullanım `state.toml`'da; dosya listelenmez, son satır "Search for…".
17. **Kayıtlı aramalar:** `settings.toml` `[[searches]]` (belirteçli yol ya da `{here}`), en çok 30; arama ▾ menüsü, palet ve kenar çubuğunda `SEARCHES` başlığı.
18. **Bütçe ve öncelik:** her parça ≤ +0,25 MiB (7c sonrası `master`'a göre), yeni crate yok; tarama iş parçacıkları düşük CPU ve G/Ç önceliğinde.
