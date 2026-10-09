# Alt Proje 9: Sistem bütünleşmesi — Tasarım

- **Tarih:** 2026-10-10
- **Durum:** Onaylandı (kullanıcı 2026-10-10: §17'nin 33 kararı onaylı; "Klasörde göster" yönlendirmesi 9b11 olarak kapsama alındı; 9a ve 9b paralel yürür); **9b1 uygulandı** (`feat/system-9b1`; Windows ekran testleri, macOS ve Linux listeleri bekliyor); **9b2 uygulandı** (`feat/system-9b2`; Windows ekran testleri, macOS ve Linux listeleri bekliyor); **9b3 uygulandı** (`feat/system-9b3`; Windows ekran testleri, macOS ve Linux listeleri bekliyor)
- **Kapsam:** Gezik yol haritasının 9. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md` §2 "9 Sistem bütünleşmesi": maddeler 6, 14, 22, 26, 33, 34, 37, 38, 39, 43, 45). Tek spec, iki bölüm: **9a** macOS yerelliği (3 PR), **9b** geri kalan her şey (10 PR). Parça listesi §14'te.
- **Dayandığı:** `2026-10-04-dosya-islemleri-design.md` (iş motoru, çakışma listesi, geri alma, çöp), `2026-10-04-gezinme-design.md` (`Location`, sekmeler, kenar çubuğu), `2026-10-04-gorunum-design.md` (sütunlar, simgeler, küçük resimler), `2026-10-08-gunluk-kolayliklar-design.md` (terminal `runas`, bağlantılar, oturum, menü kimlik aralıkları), `2026-10-09-arama-design.md` (palet, `Location::Search`, menü kimlikleri 1500–1699, `Action` 72)
- **Taban:** `master` 5f950fc (8b, D4 ve ekran düzeltmeleri birleşmiş). Exe 23,6 MB, boşta bellek 7,2–7,3 MB.

## 1. Amaç

Gezik'i kullanan biri Explorer'a ve Finder'a hiç ihtiyaç duymasın: klasörler, Win+E, çöp, bulut klasörleri, sürücü çıkarma, sunucuya bağlanma, dosya bilgisi, "Birlikte aç", paylaş, sistem simgeleri ve küçük resimler Gezik'in içinde olsun; sistemdeki "klasörü aç" istekleri Gezik'e gelsin. Bunu yaparken Gezik hafif kalsın: **kapalıyken hiçbir şey çalışmaz, sisteme dokunan her şey isteğe bağlıdır, açılışa maliyet eklenmez, her değişiklik tek komutla geri alınır.**

İki ilke:

1. **Okumak serbest, yazmak açık komutla.** Simge, küçük resim, çöp listesi, bulut durumu, sürücü listesi Gezik açıkken gerektiği an okunur. Kayıt defterine, LaunchServices'e, `mimeapps.list`'e, PATH'e, oturum açılışına yazan her şey kullanıcının verdiği bir komutla olur ve günlüğe (§11) yazılır.
2. **Yetki tek seferlik.** Yönetici yetkisi isteyen her iş kendi kendine kapanan, iş listesini komut satırından alan bir yardımcı süreçtir (§10). Kalıcı servis, daemon, sürücü, enjeksiyon yoktur.

### Başarı ölçütleri

- **macOS (9a):** liste ve ızgarada simgeler Finder'ınkiyle aynı (uygulamalar, özel klasör simgeleri, belge türleri); Finder'ın küçük resim çıkardığı her türün (PDF, HEIC, MOV, PSD, Pages, …) küçük resmi var. Finder'ın dosya sağ tık menüsündeki her öğenin Gezik'te karşılığı var (§4.3 tablosu). Space sistem Quick Look panelini açar. Bilgi penceresinde izin, sahip, gizli ve kilitli düzenlenir.
- **Varsayılan yönetici açıkken (9b4):** Windows'ta Win+E, masaüstündeki ve Başlat menüsündeki klasörler, `start .`, Çalıştır'a yazılan klasör yolu, masaüstündeki Geri Dönüşüm Kutusu ve This PC Gezik'te açılır; Gezik açıksa ≤ 300 ms'de var olan pencerede (§5). macOS'ta `open ~/Documents` ve "Reveal in Finder" diyen uygulamalar (NSFileViewer'a uyanlar), Linux'ta tarayıcıların "Klasörde göster"i Gezik'te açılır. Gezik'in gösteremediği sanal klasörler (Denetim Masası, Ağ, Kitaplıklar …) Explorer'da açılır; hiçbir yol döngüye girmez.
- **Geri alma:** `gezik --unregister` sonrası Gezik'in yazdığı her kayıt değeri, dosya ve bağlantı önceki hâline döner (testte yazmadan önceki ve geri almadan sonraki anlık görüntü bayt bayt aynı). Exe silinmiş olsa bile Windows'ta `restore-explorer.reg` çift tıkla aynı işi yapar.
- **Hafiflik:** varsayılan ayarlarla (her özellik kapalı) boşta bellek `master`'a göre ≤ +0,1 MB, açılış ≤ +2 ms (tek örnek yoklaması), yeni zamanlayıcı ya da yoklama yok, arka planda ek süreç yok. Tepsi + kısayol açıkken ≤ +0,5 MB.
- **Yönetici işleri:** yetki penceresi yalnız kullanıcının açık isteğiyle çıkar; yardımcı son işten sonra ≤ 1 sn içinde kapanır; diske ara dosya yazılmaz; sembolik bağlantı ya da junction değiştirilerek yardımcı korunan bir hedefe yönlendirilemez (§10.4 testleri).
- **Çöp:** 10.000 öğelik çöp (üç sistemde) ≤ 1 sn'de listelenir; seçerek geri yükleme ve boşaltma iş motorundan geçer.
- **Bütçe:** her parça exe'ye ≤ +0,25 MiB (Windows sürüm derlemesi; macOS/Linux kullanıcının derlemesinde not edilir). Linux'ta yeni crate yok (D-Bus kendi kodumuz, §8.3).

### Kapsam dışı (bilerek)

- **Explorer/Finder'ın içine girmek:** DLL enjeksiyonu, kanca, Finder eklentisi (Finder Sync), kabuk değiştirme (`Winlogon\Shell`). Masaüstü ve görev çubuğu Explorer'da, Dock ve masaüstü Finder'da kalır.
- ~~`SHOpenFolderAndSelectItems` ve `explorer.exe /select,…` çağrıları~~: kullanıcı kararıyla kapsama alındı (9b11). Yoklamayla başlanır; Explorer'a kanca atmadan güvenli bir yol (ör. `explorer.exe`'nin `/select` çağrısını karşılayan, yalnız HKCU'ya yazılan ve geri alınabilen bir yönlendirme) bulunamazsa parça düşer ve sınır olarak kalır.
- Ağ keşfi (Ağ komşuları, Bonjour taraması, WS-Discovery); SMB dışı protokoller (SFTP, WebDAV, FTP: 18. adım).
- Explorer bağlam menüsüne "Gezik'te aç" eklemek, `gezik://` protokolü, bildirimler.
- Etiketler, Finder yorumları (12. adım); zaman damgası düzenleme ve ACL düzenleme (§17 karar 4).
- Linux'ta sanal dosya bırakma (XDS), Linux'ta bulut indir/boşalt (standart yok), Linux tepsi menüsü (§17 karar 24).
- Kurucu, imzalama, notarization (Yayın hazırlığı).

## 2. Alınan kararlar

| Konu | Karar | Kaynak |
|---|---|---|
| Bölme | 9a macOS yerelliği (simgeler, küçük resimler, sağ tık öğeleri, Get Info, Finder adları, takma adlar); 9b geri kalan her şey | Kullanıcı kararı 1 |
| Öncelik | macOS → Windows → Linux; her parçada bu sırayla yazılır ve denenir | Kullanıcı kararı 2 |
| Varsayılan dosya yöneticisi | Var; yalnız açık "Make default" komutuyla; Windows'ta kullanıcı başına (HKCU), yönetici istemez, geri alınır | Kullanıcı kararı 3 |
| Windows kapsamı | Her şey: Win+E, klasör açma, `shell:` yerleri, Geri Dönüşüm Kutusu …; gösterilemeyen sanal klasör Explorer'a döner, geri alma sağlam | Kullanıcı kararı 4 |
| İlke | Sisteme dokunan her şey isteğe bağlı (palet, ayarlar, panel); ilk açılışta soru yok | Kullanıcı kararı 5 |
| Geri alma | `gezik --unregister` + palet komutu; yazılan her anahtar/dosya günlükte | Kullanıcı kararı 6 |
| Tek örnek ve CLI | Varsayılan tek örnek; `--new-window` kaçış; `--select`, `--new-tab` | Kullanıcı kararı 7 |
| PATH | İsteğe bağlı; macOS/Linux `~/.local/bin` bağlantısı, Windows kullanıcı PATH'i; yönetici yok | Kullanıcı kararı 8 |
| Tepsi, genel kısayol, girişte başlama | Üçü de; her biri ayrı açılır, varsayılan kapalı; kapalıyken hiçbir şey çalışmaz | Kullanıcı kararı 9 |
| Çöp görünümü | Üç sistemde: gez, geri yükle, boşalt | Kullanıcı kararı 10 |
| Bulut | Kenar çubuğunda otomatik + dosya başına durum + indir/boşalt komutları | Kullanıcı kararı 11 |
| Get Info | macOS/Linux'ta düzenlenebilir (izin, sahip, gizli/kilitli); gerekince yönetici yardımcısı; Windows'ta yararlıysa | Kullanıcı kararı 12 |
| Sürücü ve ağ | Çıkarma + "Sunucuya bağlan" (SMB adresi); keşif yok | Kullanıcı kararı 13 |
| Sanal dosya bırakma | Windows + macOS | Kullanıcı kararı 14 |
| Yönetici işleri | Üç sistemde tek seferlik yükseltilmiş yardımcı: iş başına başlar, iş listesi komut satırında, yalnız kopyala/taşı/sil/ad değiştir/klasör oluştur/öznitelik, işi bitince kapanır | Kullanıcı kararı 15 |
| Klasör boyutu | Varsayılan kapalı kalır (hafiflik) | Önceki karar |
| Diğer her şey | §17'deki ayrıntı kararları | Bu spec |

## 3. Ortak yapı

### 3.1 Üç durum: okuma, açık komut, ayar

| Tür | Örnekler | Ne zaman çalışır | Kapalıyken maliyet |
|---|---|---|---|
| Okuma | Simgeler, küçük resimler, çöp listesi, bulut durumu, sürücüler, Open With listesi | Gösterilen şey gerektirdiğinde | Sıfır |
| Açık komut (sistem durumu) | Make default, Add to PATH, Gezik.app oluşturma | Kullanıcı panelden ya da paletten verdiğinde; günlüğe yazılır | Sıfır (açılışta günlük bile okunmaz, §11.3) |
| Ayar (`[system]`) | Tepsi, genel kısayol, girişte başlama, tek örnek, Quick Look paneli | Ayar açıksa açılışta | Kapalıyken sıfır |

Varsayılanlık ve PATH ayar değildir, sistemin durumudur: `settings.toml`'a yazılmaz, günlükten okunur. Tepsi ve kısayol Gezik'in kendi davranışıdır: ayardır. Girişte başlama ikisinin arasıdır: ayardır, açılınca sisteme yazdığı kayıt günlüğe de girer (§9.3).

### 3.2 Sistem bütünleşmesi paneli

Eylem `system-integration` (palet ve macOS uygulama menüsünde `System Integration…`, Windows/Linux'ta Görünüm menüsünün sonunda). Sekme seçicinin katman bileşeninden (`picker.slint`) kurulan tek pencere; her satır bir özellik, durumu ve tek düğmesi:

```
Default file manager      Off            [Make default]
Command line (PATH)       On  ~/.local/bin/gezik   [Remove]
Tray icon                 Off            [Turn on]
Global shortcut           Off  Win+Shift+E          [Turn on]
Start at login            Off            [Turn on]
Single instance           On             [Turn off]
──────────────────────────────────────────────
Changes made: 7 (system-changes.toml)   [Show]  [Undo all]
```

- Her düğme paletin de bir komutudur (`Make Gezik the default file manager`, `Restore the system file manager`, `Add gezik to PATH`, `Remove gezik from PATH`, `Turn on tray icon` …); palet `Command` türünde listeler (8b'nin yolu).
- `Show` günlüğü okunur biçimde listeler (anahtar, değer, önce, sonra). `Undo all` = `--unregister` (§11.4).
- Panel açılınca durum canlı okunur (kayıt defteri değeri, `mimeapps.list` satırı, symlink hedefi); günlükle uyuşmayan satır `Changed outside Gezik` der ve düğmesi `Repair` olur.

### 3.3 Menü kimlikleri

8b 1500–1699'u kullanır. **9a: 1700–1799**, **9b: 1800–1999** (§13.3). `GEZIK_IDS_END = 4096` değişmez. Tepsi menüsü yerel menüdür, ayrı kimlik uzayı (`gezik-platform::tray`).

## 4. 9a — macOS yerelliği

### 4.1 Sistem simgeleri (9a1)

- `icons::imp::icon` macOS'ta `NSWorkspace.sharedWorkspace.iconForFile:` ile doldurulur; `NSImage` istenen piksel boyutunda bir `NSBitmapImageRep`'e çizilip RGBA'ya çevrilir (`picture.rs`'teki dönüştürme yolu).
- **Önbellek:** sıradan dosyalar için uzantı başına bir simge (Windows'taki `IconTarget` kuralı); klasörler için bir genel simge, ancak özel simgesi olanlar (`Icon\r` dosyası ya da `com.apple.FinderInfo` xattr'ında `kHasCustomIcon`) yol başına; uygulama ve paketler (`.app`, §4.2) yol başına; birim kökleri yol başına. İlk istekte doldurulur, bellekte kalır.
- `iconForFile:` herhangi bir iş parçacığından çağrılabilir mi: **doğrulanacak** (Apple belgesi AppKit çizimini ana iş parçacığına bağlar). Değilse simge istekleri ana iş parçacığında küçük partiler hâlinde (kare başına ≤ 2 ms) çalışır.

### 4.2 Finder adları, takma adlar, paketler (9a1)

- **Yerelleştirilmiş adlar:** `NSFileManager.displayNameAtPath:` yalnız şu yerlerde: kenar çubuğu, kırıntı yolu, sekme başlığı ve ev klasörü ile `/` altındaki klasör satırları (Applications → "Uygulamalar", Documents → "Belgeler"; `.localized` uzantılı klasörler). Sıralama, süzgeç ve arama gerçek adla çalışır, adres çubuğu gerçek yolu yazar. Bugünkü `known::display_name` macOS'ta buna bağlanır.
- **Takma adlar (alias):** açılırken (çift tık, Enter) dosya `NSURLIsAliasFileKey` taşıyorsa `NSURL.URLByResolvingAliasFileAtURL:options:error:` ile çözülür; klasörse oraya gidilir, dosyaysa hedef açılır. Çözülemezse "The original item can't be found" ve `Delete alias` seçeneği. Satırda tür `Alias` (UTType `com.apple.alias-file`'ın açıklaması). Yalnız açılırken bakılır, listelerken ek çağrı yok.
- **Takma ad oluşturma:** eylem `make-alias` (⌃⌘A, Finder'ınki); 7c'nin `New ▸ Link` menüsüne macOS'ta `Alias` girer. `bookmarkDataWithOptions:NSURLBookmarkCreationSuitableForBookmarkFile` + `NSURL.writeBookmarkData:toURL:options:error:`. Ad `X alias` (Finder gibi), çakışmada 7c'nin numaralama kuralı. İş motorunda `Outcome::Created`; Ctrl+Z siler.
- **Paketler:** uzantısı paket listesinde olan klasörler (`.app`, `.bundle`, `.pkg`, `.rtfd`, `.photoslibrary`, `.key`, `.pages`, `.numbers`, `.xcodeproj`, …; liste `gezik-core::kind`'da) yalnız bu durumda `NSURLIsPackageKey` ile doğrulanır ve dosya gibi davranır: Enter/çift tık açar (uygulamayı başlatır), içine gezinilmez. Eylem `show-package-contents` (satır menüsünde `Show Package Contents`) klasör olarak açar.

### 4.3 Sağ tık menüsü: Finder eşdeğerleri (9a2)

| Finder öğesi | Gezik | Mekanizma |
|---|---|---|
| Open | Var | — |
| Open With ▸ | **Yeni**: varsayılan uygulama başta, sonra diğerleri, `Other…` | `NSWorkspace.URLsForApplicationsToOpenURL:` (macOS 12+), `URLForApplicationToOpenURL:`; açma `openURLs:withApplicationAtURL:configuration:completionHandler:`; `Other…` `NSOpenPanel` (`/Applications`, yalnız `.app`) |
| Move to Trash | Var | — |
| Get Info | **Yeni** (§4.4) | — |
| Rename, Duplicate, Compress, Copy, Quick Look | Var | — |
| Make Alias | **Yeni** (§4.2) | — |
| Share… | **Yeni** | `NSSharingServicePicker.initWithItems:` + `showRelativeToRect:ofView:preferredEdge:` (fare konumunda, Gezik'in `NSView`'ı); AirDrop, Mail, Messages, Notes |
| Quick Actions / Services ▸ | **Yeni** (§17 karar 26) | Dosya kabul eden servisler: `~/Library/Services/*.workflow` ve sistem Quick Actions'ın `NSServices` girdileri; çalıştırma `NSPerformService(name, pasteboard)` (dosya URL'leri panoda). Liste nasıl alınır: **doğrulanacak** (genel bir API yok; plan önce yoklar) |
| Show Package Contents | **Yeni** (§4.2) | — |
| Tags | 12. adım | — |

- **Open With listesi:** birden çok öğe seçiliyse ≤ 50 öğede uygulama listelerinin kesişimi, fazlasında odaktaki öğeninki. Uygulama adları `displayNameAtPath:`, 16 px simgeleri §4.1'in önbelleğinden. En çok 40 uygulama (`OPEN_WITH_FIRST` 1700–1739); fazlası `Other…`'a kalır.
- Menü açılırken liste arka planda istenir; menü 50 ms içinde hazır değilse `Open With ▸` alt menüsü "Loading…" ile açılır ve gelince dolar (7c'nin alt menü yolu). Bekleme yok.

### 4.4 Bilgi penceresi (Get Info) (9a3)

- **Eylem `get-info`:** macOS ⌘I; Linux Alt+Enter. Windows'ta Alt+Enter sistemin Özellikler penceresini açar (`SHMultiFileProperties` ile birden çok öğe, tek öğede `SHObjectProperties`); Windows'ta Gezik paneli yoktur (§17 karar 3). Linux'taki pencere macOS'unkiyle aynı bileşendir; burada anlatılan her şey ikisi için geçerlidir, macOS'a özgüler işaretli.
- **Biçim:** sağdan açılan bir katman (`info.slint`), birden çok öğe seçiliyse tek pencerede karışık değerler (üç durumlu onay kutuları). Esc kapatır. Değişiklik anında uygulanır (Finder gibi).

| Bölüm | Alanlar | Düzenlenir mi |
|---|---|---|
| Genel | Simge, ad, tür, boyut (klasörde 8b'nin hesaplaması, istekle), yer, oluşturma/değiştirme/erişim tarihleri (Linux'ta oluşturma `statx` `btime` varsa) | Hayır (ad değişimi bugünkü yeniden adlandırmayla) |
| Bayraklar | Gizli (macOS `UF_HIDDEN`), Kilitli (macOS `UF_IMMUTABLE`) | Evet, `chflags`; Linux'ta bu satır yok (gizlilik nokta önekidir) |
| Birlikte aç | Bu dosyanın uygulaması, `Change All…` | Evet: macOS `NSWorkspace.setDefaultApplicationAtURL:toOpenContentType:completionHandler:` (macOS 12+); Linux `mimeapps.list` (§8.5'in yazıcısı, 9b10'da) |
| Paylaşım ve izinler | Sahip, grup, sahip/grup/diğerleri için okuma-yazma-çalıştırma, sekizli değer (`755`); ACL varsa "This item has access control entries" notu | Evet: `chmod`, `chown`; özel bitler (setuid, setgid, sticky) gösterilir, değiştirilemez |
| Klasörde | `Apply to enclosed items…` | Evet: izinleri ve sahibi alt öğelere uygular (onay sorulur) |

- **Sahip ve grup:** yazılabilir alan, tamamlama `getpwent`/`getgrent` listesinden (ilk açılışta bir kez okunur, pencere kapanınca bırakılır). Grup değişimi kullanıcının üyesi olduğu gruplara yönetici istemeden yapılır; sahip değişimi ve başkasının dosyasına her değişiklik yönetici ister.
- **İş motoru:** yeni `SetAttributesTask` (`gezik-ops/src/tasks/attrs.rs`): her öğe için önceki değerleri (mod, uid, gid, bayraklar) okur, yenisini yazar, `Outcome::AttributesChanged { before }` döner; geri alma öncekini yazar. Özyinelemeli uygulama 8a'nın yürüyüş kuralıyla (bağlantı izlenmez). Böylece her değişiklik tek Ctrl+Z ile geri alınır.
- **Yetki:** `EPERM`/`EACCES` gelirse değişiklik geri çevrilmez, alanın altında `Requires administrator` ve `Change as administrator…` düğmesi çıkar; düğme yardımcıyı (§10) aynı işle çalıştırır. 9a3'te yardımcı yoktur; düğme 9b7'de bağlanır (9a3'te yalnız not görünür).
- **Linux'ta** pencere 9a3'te gelir (aynı bileşen); "Birlikte aç" satırı 9b10'da.

### 4.5 Quick Look küçük resimleri ve paneli (9a1, 9a2)

- **Küçük resimler (9a1):** ızgarada ve önizlemede, Gezik'in kendi 5 biçimi dışında kalan her dosya için `QLThumbnailGenerator.sharedGenerator.generateBestRepresentationForRequest:completionHandler:` (`QLThumbnailGenerationRequest`, boyut ve ekran ölçeğiyle, tür `.thumbnail`). Tamamlama bloğu arka plan kuyruğunda gelir; `CGImage` RGBA'ya çevrilip bugünkü küçük resim kanalına konur. Yalnız görünen hücreler için istenir (bugünkü kural), ekrandan çıkan hücrenin isteği `cancelRequest:` ile bırakılır. Crate: `objc2-quick-look-thumbnailing` (sürüm ve adı **doğrulanacak**).
- **Bulut dosyaları:** `SF_DATALESS` bayraklı dosyalar için küçük resim istenmez (indirmeyi tetiklememek için; §7.3).
- **Sistem paneli (9a2):** `[system] quick-look = "system"` (varsayılan, macOS) iken Space `QLPreviewPanel.sharedPreviewPanel`'i açar; `"gezik"` bugünkü kendi penceresi. Panel, yanıtlayıcı zincirinde `acceptsPreviewPanelControl:`, `beginPreviewPanelControl:`, `endPreviewPanelControl:` ister: bunlar winit'in görünüm sınıfına `class_addMethod` ile eklenir (`dnd/macos.rs`'in yöntemiyle). Veri kaynağı seçili öğelerin URL'leri; paneldeyken ok tuşları Gezik'in odağını taşır (panel temsilcisinin `previewPanel:handleEvent:`'i). Panelin ok tuşu ve sahiplik davranışı **doğrulanacak**; çalışmazsa varsayılan `"gezik"` kalır.
- Windows ve Linux'ta Hızlı Bakış bugünkü gibi Gezik'in penceresidir.

## 5. 9b1 — Komut satırı ve tek örnek

### 5.1 Komut satırı

```
gezik [OPTIONS] [PATH...]
  PATH              folder: open it; file: open its folder with it selected; several: a tab each
  --new-tab         always a new tab (default: reuse a tab already showing that folder)
  --new-window      a separate Gezik window (a new process), not the running one
  --select PATH     open PATH's folder with PATH selected (may repeat)
  --background      start without a window (tray, shortcut, login)
  --unregister      undo every system change Gezik made (see system-changes.toml), then exit
  --version, --help
```

İç bayraklar (yardımda yok, başka biri çağırırsa zararsız): `--shell TARGET` (Windows klasör fiili, §6.1), `--dbus` (Linux D-Bus etkinleştirmesi, §8.3), `--elevated …` (§10), `--pdf-worker` (bugünkü).

- Ayrıştırma saf işlevdir (`gezik/src/cli.rs`, birim testli); bugünkü `start.rs` `plan_start` sonucu kullanır. Bilinmeyen bayrak uyarıdır (durum çubuğu ve konsol), Gezik yine açılır.
- Windows sürüm derlemesi pencere alt sistemindedir; `--help`, `--version`, `--unregister` çıktısı için `AttachConsole(ATTACH_PARENT_PROCESS)` (konsoldan çağrıldıysa yazar, yoksa sessiz). Yardım ve sürüm Slint'ten ve ayarlardan önce yazılıp çıkılır.
- **Göreli yollar** gönderen süreçte, kendi çalışma klasörüne göre mutlaklaştırılır (alıcı gönderenin klasörünü bilmez).

### 5.2 Tek örnek

- `[system] single-instance = true` (varsayılan). İkinci bir `gezik` çağrısı (bayraksız ya da `--new-tab`/`--select`) çalışan Gezik'e bağlanır, isteği gönderir, yanıtı bekler ve çıkar. `--new-window`, `--background`, `--unregister` tek örneğe gitmez.
- **Kanal:** Windows'ta adlandırılmış boru `\\.\pipe\gezik-<SID>-<oturum kimliği>-<anahtar>`; ilk Gezik `FILE_FLAG_FIRST_PIPE_INSTANCE` ve yalnız kullanıcının SID'sine izin veren DACL ile açar. macOS'ta `$TMPDIR/gezik-<anahtar>.sock` (`$TMPDIR` kullanıcıya özel, 0700); Linux'ta `$XDG_RUNTIME_DIR/gezik-<anahtar>.sock`, yoksa `/tmp/gezik-<uid>/` (Gezik açar; sahibi kullanıcı ve kipi 0700 değilse kullanmaz, tek örnek o oturumda kapalı kalır). Unix'te bağlanan sürecin kimliği `SO_PEERCRED` / `getpeereid` ile denetlenir; farklı uid kapatılır.
- **Anahtar:** yapılandırma klasörünün yolunun özeti (FNV-1a, 16 onaltılık); iki taşınabilir Gezik birbirine karışmaz.
- **İleti:** sürüm baytı, ardından uzunluk önekli alanlar: bayraklar, yollar (Unix'te ham baytlar), `XDG_ACTIVATION_TOKEN` (varsa). En çok 1 MB, en çok 1.000 yol; fazlası reddedilir. Yanıt: `ok` + alıcının pid'i. Kodlama ve çözme saf, birim testli.
- **Alan taraf:** bir iş parçacığı bağlantı bekler (boşta CPU yok); ileti olay döngüsüne `upgrade_in_event_loop` ile verilir. Her yol için: aynı klasörü gösteren sekme varsa ona geçilir (`--new-tab` yoksa), yoksa yeni sekme; dosyaysa klasörü açılıp seçilir. Pencere öne getirilir: Windows'ta gönderen `AllowSetForegroundWindow(pid)` çağırır, alan `SetForegroundWindow`; macOS'ta `activateIgnoringOtherApps`; Wayland'de gönderenin etkinleştirme belirteci (winit'in xdg-activation desteğiyle; **doğrulanacak**), X11'de `_NET_ACTIVE_WINDOW`.
- **Bayat kanal:** bağlanılamazsa (çökmüş Gezik'in soketi) Unix'te dosya silinip yeniden açılır; Windows'ta boru süreçle birlikte kaybolur.
- **Asılı Gezik:** gönderen 2 sn içinde `ok` alamazsa: `--shell` ile geldiyse Explorer'a döner (§6.3), değilse kendi penceresini açar (yeni süreç) ve tek örnek kanalını almaz.
- **Bayraksız ikinci çağrı (Win+E dahil):** `start-folder`'ı açar (aynı kural: o klasörü gösteren sekme varsa ona geçer).

### 5.3 Yeni pencere

Eylem `new-window` (Ctrl+N / ⌘N): Gezik'i `--new-window <gösterilen klasör>` ile yeni süreç olarak başlatır. Çok pencereli tek süreç yoktur (§17 karar 9). Yeni süreç tek örnek kanalını almaz; ilk Gezik kapanınca kanal boşta kalır, sonraki çağrı yeni bir ilk örnek olur.

## 6. 9b4 — Varsayılan dosya yöneticisi

### 6.1 Windows (HKCU, yönetici yok)

**Yazılanlar** (hepsi `HKCU\Software\Classes` altında; `<exe>` Gezik'in tam yolu):

| Anahtar | Değer | Ne için |
|---|---|---|
| `Directory\shell\gezik` | `(Default) = "Open in Gezik"` | Fiil |
| `Directory\shell\gezik\command` | `(Default) = "<exe>" --shell "%1"` | |
| `Directory\shell` | `(Default) = "gezik"` (önceki değer günlükte) | Klasör çift tıkı, `start .`, Çalıştır |
| `Drive\shell\gezik`, `…\command`, `Drive\shell` `(Default)` | Aynı | Sürücü kökleri |
| `Folder\shell\gezik`, `…\command`, `Folder\shell` `(Default)` | Aynı | Sanal klasörler: Geri Dönüşüm Kutusu, This PC, `shell:` yerleri |
| `CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\shell\opennewwindow\command` | `(Default) = "<exe>" --shell ""`, `DelegateExecute = ""` | Win+E ve görev çubuğundaki Explorer simgesi |

- `%1` mi `%V` mi (Directory için), Win+E CLSID'si ve `DelegateExecute`'un boş değerle bastırılması **doğrulanacak**: 9b4 planının ilk görevi bir yoklamadır; fiil davranışı yalnız gerçek `Classes` altında sınanabildiği için kullanıcı yokken ve kurtarma `.reg`'i önceden hazırken yapılır.
- Değerler `RegCreateKeyExW` + `RegSetValueExW` ile yazılır; her yazmadan önce önceki değer (yoksa "yok") günlüğe girer; yazmadan sonra `SHChangeNotify(SHCNE_ASSOCCHANGED)`.
- **Gelen hedefler** (`--shell TARGET`):

| TARGET | Gezik |
|---|---|
| Gerçek klasör ya da sürücü yolu (`shell:Downloads` gibi bilinen klasörler kabukça çözülmüş gelir) | Açar |
| `::{645FF040-5081-101B-9F08-00AA002F954E}` Geri Dönüşüm Kutusu | Çöp görünümü (§7.1) |
| `::{20D04FE0-3AEA-1069-A2D8-08002B30309D}` This PC, `::{F874310E-B6B7-47DC-BC84-B9E6B38F5903}` Ana Sayfa, boş (Win+E) | `start-folder` (varsayılan This PC) |
| `::{679F85CB-0220-4080-B29B-5540CC05AAB6}` Hızlı erişim | `start-folder` |
| `\\sunucu` (paylaşım değil sunucu) | Paylaşım listesi (§7.4) |
| `\\sunucu\paylaşım\…` | Açar |
| Dosya (ör. `.zip`, `Folder` sınıfına giren türler) | Gezik'in açabildiği arşivse klasörü açılıp seçilir; değilse Explorer'a |
| Başka her şey (Denetim Masası, Ağ, Kitaplıklar `*.library-ms`, Telefon bağlantısı, `::{…}` bilinmeyen) | Explorer'a (§6.3) |

- Eşleme tablosu saf işlevdir (`gezik/src/cli.rs`), CLSID'ler büyük/küçük harf duyarsız, `::{X}\alt` biçimleri de tanınır.

### 6.2 macOS

- **Gezik.app:** LaunchServices bir paket ister. Gezik bir paketin içinden çalışmıyorsa `Make default` önce `~/Applications/Gezik.app`'i kurar: `Contents/Info.plist` (gömülü `Info.plist` + `CFBundleDocumentTypes`: `LSItemContentTypes = [public.folder, public.volume]`, `CFBundleTypeRole = Viewer`), `Contents/Resources/gezik.icns`, `Contents/MacOS/gezik` → çalışan exe'ye **sembolik bağlantı**. LaunchServices'in sembolik bağlantılı paketi kabul etmesi **doğrulanacak**; etmezse kopya (24 MB) ve günlükte özet; Gezik güncellenince panel `Gezik.app is older than this Gezik` der, `Repair` yeniden kopyalar. Sonra `LSRegisterURL(app, true)`.
- **Klasör açma:** `NSWorkspace.setDefaultApplicationAtURL:toOpenContentType:completionHandler:` (`public.folder`; macOS 12+). Kullanıcıdan onay isteyip istemediği **doğrulanacak**. Önceki varsayılan (`URLForApplicationToOpenContentType:`) günlüğe.
- **Reveal in Finder:** genel alanda `NSFileViewer = com.wenlar.gezik` (`CFPreferencesSetAppValue(…, kCFPreferencesAnyApplication)` + `CFPreferencesAppSynchronize`); `activateFileViewerSelectingURLs:` kullanan uygulamalar Gezik'i açar. Hangi macOS sürümlerinin buna uyduğu **doğrulanacak**.
- **Belge açma olayı:** paketlenmiş Gezik'e LaunchServices klasörü `application:openURLs:` ile verir; winit bunu açmaz. winit'in uygulama temsilcisi sınıfına `class_addMethod` ile `application:openURLs:` eklenir ve tek örnek kanalının işleyicisine verilir (**doğrulanacak**: winit 0.30 temsilci sınıfı adı ve Slint'in bunu ezmemesi).
- **Sınır:** Finder kapatılamaz ve değiştirilemez; Dock'taki Finder simgesi, masaüstü, Finder'ın kendi pencereleri Finder'da kalır.

### 6.3 Explorer'a geri düşüş ve güvenlik ağı (Windows)

- **Geri düşüş:** `%WINDIR%\explorer.exe` doğrudan `CreateProcessW` ile, tek bağımsız değişken TARGET (tırnaklı). Explorer'ın komut satırına verilen hedefi varsayılan fiile bakmadan kendi penceresinde açtığı **doğrulanacak**.
- **Döngü koruması:** tek örnek sunucusu son 10 sn'de Explorer'a verilen hedefleri bellekte tutar; aynı hedef yine `--shell` ile gelirse ikinci kez Explorer'a verilmez, Gezik `Only Explorer can show this place, and it sent it back to Gezik. [Restore Explorer as default]` der. Tek örnek kapalıyken (ya da Gezik asılıyken) bellek paylaşılmaz: o durumda geri düşen süreç Explorer'ı `explorer.exe /separate,TARGET` ile başlatır; `/separate`'in varsayılan fiile bakmadan Explorer'da açtığı **doğrulanacak**, açmıyorsa plan yoklamasında `ShellExecuteExW` + `explore` fiili denenir. Hangi yol seçilirse seçilsin bir hedef için en çok bir geri düşüş olur.
- **Açılamayan Gezik:** `--shell` ile başlayan Gezik pencere kuramazsa (Slint hatası) çıkmadan önce hedefi Explorer'a verir. Exe yerinde değilse Windows "bulunamadı" der: bu durum için yapılandırma klasörüne `restore-explorer.reg` yazılır (§11.2) ve panel ile belge yolunu söyler.
- **Exe taşındı:** günlük boş değilse açılıştan 2 sn sonra (boşta) günlükteki exe yolu bugünkü exe ile karşılaştırılır; farklıysa durum çubuğu `Gezik moved; system registrations point to the old place. [Update]`.
- **Uyarı:** exe geçici görünen bir yerdeyse (`Downloads`, `%TEMP%`, çıkarılabilir sürücü) `Make default` önce `Gezik is in Downloads; if you move or delete it, folders will not open. Continue?` sorar.

### 6.4 Linux

- **Dosyalar:** `~/.local/share/applications/gezik.desktop` (`Exec="<exe>" %U`, `MimeType=inode/directory;`, `Icon=gezik`, `DBusActivatable=false`), `~/.local/share/icons/hicolor/256x256/apps/gezik.png` (gömülü simge), `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service` (`Exec="<exe>" --dbus`).
- **Varsayılan:** `~/.config/mimeapps.list` `[Default Applications]` bölümünde `inode/directory=gezik.desktop;` (önceki değer günlükte). `$XDG_CURRENT_DESKTOP` bileşenleri için `~/.config/<masaüstü>-mimeapps.list` varsa ve `inode/directory` içeriyorsa o da düzenlenir. Düzenleme saf işlev (satır ve yorum korunur, birim testli); `xdg-mime` çağrılmaz.
- **FileManager1:** varsayılanlık açıkken çalışan Gezik açılıştan sonra boşta (§11.3) oturum veriyolunda `org.freedesktop.FileManager1` adını ister (`DO_NOT_QUEUE`; başkası tutuyorsa almaz, panel bunu söyler). `ShowFolders`, `ShowItems`, `ShowItemProperties` (`as` URI'ler, `s` başlangıç kimliği) karşılanır: `file://` URI'ler `linux/uri.rs` ile çözülür, sırasıyla klasör açma, klasör açıp seçme, Bilgi penceresi; `trash:///` çöp görünümü; diğer şemalar yok sayılır. Gezik kapalıyken veriyolu `.service` dosyasıyla `gezik --dbus`'u başlatır. Kullanıcı `.service` dosyasının sistemdekinden (`/usr/share/dbus-1/services`) önce geldiği dbus-daemon ve dbus-broker için **doğrulanacak**.
- D-Bus bağlantısı §8.3'ün kodlayıcısıyla; varsayılanlık kapalıyken açılmaz.

## 7. 9b2, 9b5, 9b6 — Çöp, bulut, sürücüler ve ağ

### 7.1 Çöp görünümü (9b2)

- **Yer:** `Location::Trash`; kenar çubuğunda Drives'ın altında `Recycle Bin` (Windows) / `Trash`, simgesi dolu/boş. Geçmiş, sekme ve oturum taşır (`trash = true`).
- **Kaynaklar** (hepsi tek listede):

| Sistem | Kutular | Öğe bilgisi |
|---|---|---|
| Windows | Her yerel sabit ve çıkarılabilir sürücüde `X:\$Recycle.Bin\<SID>\` | `$Ixxxxxx.ext`: sürüm 1 (8 B sürüm, 8 B boyut, 8 B FILETIME, 520 B sabit yol) ve sürüm 2 (… + 4 B uzunluk + UTF-16 yol); öğe yanındaki `$Rxxxxxx.ext`. Saf ayrıştırıcı, bayt örnekleriyle test |
| macOS | `~/.Trash`, `/Volumes/*/.Trashes/<uid>` | Özgün yer `~/.Trash/.DS_Store`'un `ptbL`/`ptbN` kayıtlarından (salt okunur ayrıştırıcı; biçim **doğrulanacak**); yoksa boş ve geri yüklemede sorulur. Gezik'in kendi attıkları için iş geçmişindeki yol önce gelir |
| Linux | `$XDG_DATA_HOME/Trash`, bağlı her birimde `$topdir/.Trash/$uid` (yapışkan bitli, sembolik bağlantı değilse) ve `$topdir/.Trash-$uid` | `info/*.trashinfo` (`fs/freedesktop.rs`'e okuyucu eklenir) |

- **Sütunlar:** Name, Original location, Date deleted, Size, Kind. Sıralama, süzgeç, önizleme, Hızlı Bakış bugünkü gibi (öğeler gerçek dosyalardır). Okuma arka planda (8a'nın parti yolu).
- **Eylemler:** `Put Back` (eylem `put-back`; satır menüsü ve araç çubuğu) bugünkü `fs::restore` + `RestoreTask` ile; özgün yerde aynı ad varsa çakışma listesi; özgün klasör yoksa oluşturulur (bugünkü `restore`). Özgün yeri bilinmeyen öğe için hedef klasör sorulur. `Delete Permanently` bugünkü silme işi + bilgi dosyası. `Empty Trash` (eylem `empty-trash`, macOS ⌘⇧⌫): Gezik'in onay kutusu (`Permanently delete 1,234 items (3.2 GB)? This cannot be undone.`), sonra Windows'ta `SHEmptyRecycleBinW(hwnd, NULL, SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)`, macOS/Linux'ta iş motorunda silme (ilerleme panelde).
- **Çöpten çıkarma:** sürükleyip ya da kesip başka klasöre bırakmak geri yükleme gibi taşır (bilgi dosyası da silinir; Windows'ta öksüz `$I` kalmaz). Ctrl+Z öğeyi yeniden çöpe atar. Çöpe bırakmak ve yapıştırmak çöpe atmadır. Çöp görünümünde yapıştırma ve yeni klasör devre dışı.
- **Güncel tutma:** görünüm açıkken kutu klasörleri `folder_watch` ile izlenir; kenar çubuğu simgesi yalnız açılışta, Gezik'in çöp işlerinden sonra ve görünüm gösterilince güncellenir (yoklama yok; dışarıdan atılan öğeler simgeye bir sonraki fırsatta yansır).
- **macOS izni:** `~/.Trash` Tam Disk Erişimi ister. Okunamazsa liste yerine bant: `Gezik needs Full Disk Access to show the Trash. [Open Privacy Settings]` (`x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles`, adres **doğrulanacak**). Çöpe atma bugünkü gibi çalışmaya devam eder.

### 7.2 Bulut sürücüleri: kenar çubuğu (9b5)

- **Windows:** `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager\*\UserSyncRoots` altında kullanıcının SID'si adlı değer = kök yol; ad `DisplayNameResource` (`SHLoadIndirectString`). Ek olarak `%OneDrive%`, `%OneDriveCommercial%`. Okuma açılışta değil, kenar çubuğu ilk çizilirken arka planda (≤ 1 ms beklenir). Anahtar yapısı **doğrulanacak**. Google Drive for desktop sürücü harfi olarak zaten Drives'tadır.
- **macOS:** `~/Library/CloudStorage/*` (`OneDrive-Kişisel`, `GoogleDrive-e@posta`, `Dropbox`; ön ek → sağlayıcı adı, kalan → alt metin) ve `~/Library/Mobile Documents/com~apple~CloudDocs` (iCloud Drive).
- **Linux:** `~/Dropbox`, `~/OneDrive`, `~/Google Drive`, `/proc/mounts`'ta `fuse.rclone` bağlamaları, `$XDG_RUNTIME_DIR/gvfs/google-drive:*`. Durum ve komut yok (§17 karar 19).
- Kenar çubuğunda `CLOUD` başlığı, sabitlenenlerden sonra; `[sidebar] cloud = true` (kapatılabilir). Kök bulunamazsa başlık yok.

### 7.3 Bulut: durum ve komutlar (9b5)

- **Durum yalnız bulut kökü altındaki klasörlerde** ve yalnız var olan veriden (ek G/Ç yok):
  - Windows: liste okumasının öznitelikleri: `RECALL_ON_DATA_ACCESS` ya da `OFFLINE` → yalnız bulutta; `PINNED` → hep bu cihazda; ikisi de değilse → yerelde.
  - macOS: `lstat`'ın `st_flags`'ında `SF_DATALESS` → yalnız bulutta; değilse yerelde. ("Hep bu cihazda" bilgisi macOS'ta bayraktan okunmaz.)
- **Gösterim:** Ad sütununda simgenin sağ altına küçük işaret (bulut ana hattı / dolu onay / boş onay); ipucu sözle. Ayrı sütun yok. "Eşitleniyor/hata" durumu yok (§17 karar 18).
- **Komutlar** (satır menüsünde, yalnız bulut kökü altında; palet):

| Windows | macOS | Mekanizma |
|---|---|---|
| `Always keep on this device` (eylem `always-keep-offline`) | `Download Now` (aynı eylem) | Win: `CfSetPinState(h, CF_PIN_STATE_PINNED, CF_SET_PIN_FLAG_RECURSE, NULL)`; macOS: `NSFileManager.startDownloadingUbiquitousItemAtURL:error:` |
| `Free up space` (eylem `free-up-space`) | `Remove Download` (aynı eylem) | Win: `CfSetPinState(…, CF_PIN_STATE_UNPINNED, …)`; macOS: `evictUbiquitousItemAtURL:error:` |

  Windows'ta sağlayıcının sabitlenmemiş dosyayı kendiliğinden boşalttığı, macOS'ta bu API'lerin File Provider (OneDrive, Google Drive) alanlarında da çalıştığı **doğrulanacak**. Komutlar iş motorunda değil, arka plan iş parçacığında çağrılır; sonuç durum çubuğunda. Geri alma yok (zararsız ve sağlayıcıya ait).
- **Koruma:** 8'deki önizleme koruması (`is_offline`) macOS'ta `SF_DATALESS`'e genişler: yalnız bulutta olan dosya için önizleme, küçük resim ve içerik araması indirme tetiklemez.

### 7.4 Sürücüler ve ağ (9b6)

- **Çıkarma** (eylem `eject`, macOS ⌘E; kenar çubuğu ve Drives listesinde `Eject`; ağ sürücüsünde `Disconnect`): önce o sürücüdeki sekmeler This PC'ye döner, izleyiciler ve önizleme tutamakları bırakılır (`removal.rs`'in yolu), sonra:
  - Windows: sürücünün `IShellItem`'inin bağlam menüsünden `eject` fiili (`IContextMenu::InvokeCommand`, Explorer'ın kendi çıkarma kodu; pencere açmaz — **doğrulanacak**); eşlenmiş ağ sürücüsünde `WNetCancelConnection2W(letter, CONNECT_UPDATE_PROFILE, FALSE)`.
  - macOS: arka plan iş parçacığında `NSWorkspace.unmountAndEjectDeviceAtURL:error:`.
  - Linux: `gio mount -e <bağlama noktası>`; `gio` yoksa `udisksctl unmount -b <aygıt>` ve çıkarılabilir diskse `udisksctl power-off -b <disk>`. Ağ bağlamasında `gio mount -u`. Hata metni stderr'den.
  - Başarısızlık: `The drive is in use. Close the files on it and try again.` (Windows hangi sürecin tuttuğunu söylemez; söylemeye çalışılmaz.)
- **Sunucuya bağlan** (eylem `connect-to-server`, Ctrl+K / ⌘K; Drives listesinin boşluk menüsü): küçük katman, adres alanı (`\\sunucu\paylaşım`, `smb://sunucu/paylaşım`, `sunucu/paylaşım` hepsi kabul; saf ayrıştırıcı), son 10 adres (`state.toml` `[servers] recent`; parola asla), Windows'ta isteğe bağlı `Map to drive letter [Z: ▾]` ve `Reconnect at sign-in`.
  - Windows: önce doğrudan UNC'ye gidilir (çoğu zaman tek oturum açma yeter); erişim reddinde ya da sürücü harfi istendiyse `WNetAddConnection3W(hwnd, &NETRESOURCEW{ RESOURCETYPE_DISK, lpLocalName, lpRemoteName }, NULL, NULL, CONNECT_INTERACTIVE | CONNECT_PROMPT [| CONNECT_UPDATE_PROFILE])`: kimlik sorusu Windows'un penceresidir.
  - macOS: `NetFSMountURLAsync(url, NULL, NULL, NULL, {kNAUIOptionKey: kNAUIOptionAllowUI}, NULL, &id, queue, block)` (NetFS çerçevesi, `extern "C"`); bağlama noktasına gidilir. Finder açılmaz. İmza ve seçenek sabitleri **doğrulanacak**.
  - Linux: `gio mount smb://…`; gio'nun sorduğu kullanıcı/alan/parola Gezik'in kimlik katmanından yalnız stdin ile verilir (asla argv'de değil). Bağlama `$XDG_RUNTIME_DIR/gvfs/smb-share:server=…,share=…` altında bulunup gidilir. gio yoksa `Connecting to servers needs gvfs (the gio command).` Soru sırası **doğrulanacak**.
- **Sunucunun paylaşımları (yalnız Windows):** `\\sunucu` yolu `NetShareEnum(server, 1, …)` ile `STYPE_DISKTREE` paylaşımlarını (`$` ile bitenler hariç) klasör satırları olarak listeler. Keşif değildir: sunucu adı kullanıcıdan gelir. macOS/Linux'ta `smb://sunucu` paylaşım adı ister.

## 8. 9b8, 9b3 ve Linux altyapısı

### 8.1 Sanal dosya bırakma (9b8)

- **Windows:** `DragEnter`'da `CF_HDROP` yoksa `FileGroupDescriptorW` aranır; varsa teklif `Copy` olur, ipucu `Copy 3 items here`. `Drop`'ta veri nesnesi `CoMarshalInterThreadInterfaceInStream` ile işçi iş parçacığına geçirilir (arayüz iş parçacığı mesaj pompalamaya devam ettiği için kaynak uygulamaya geri çağrılar akar) ve yeni `MaterializeTask` her girdi için `FileContents`'i `lindex` ile ister: `TYMED_ISTREAM` / `TYMED_HGLOBAL` doğrudan yazılır, `TYMED_ISTORAGE` (Outlook `.msg`) `StgCreateStorageEx` + `IStorage::CopyTo` ile. Kaynak `IDataObjectAsyncCapability` destekliyorsa `SetAsyncMode`/`StartOperation`/`EndOperation`. İşçi iş parçacığı yaklaşımı **doğrulanacak** (Outlook ile).
- **macOS:** görünüm `NSFilePromiseReceiver.readableDraggedTypes` için de kaydolur (`dnd/macos.rs`). Panoda dosya URL'si varsa bugünkü yol; yoksa `NSFilePromiseReceiver`'lar `receivePromisedFilesAtDestination:` ile `$TMPDIR/gezik-drop-<rastgele>/`'ye (0700) alınır, sonra iş motoru buradan hedefe **taşır** (çakışma listesi ve geri alma oradan), ara klasör silinir. Okuyucu bloğu arka plan `NSOperationQueue`'da.
- **Güven sınırı:** adlar başka bir uygulamadan gelir. Her ad: mutlak yol, sürücü harfi, `..`, `.`, boş parça, NUL ve denetim karakteri reddedilir; Windows'ta geçersiz karakterler `_` olur, ayrılmış adlar (`CON`, `NUL` …) `_CON`; uzunluk sistem sınırına kırpılır. Göreli klasörlü girdiler (`a\b.txt`) hedefin altında oluşturulur ve kökün dışına çıkamaz. Boyut beyanı güvenilmez (yazılan bayt sayılır). Saf işlev, birim testli.
- **Sonuç:** Gezik'in iş geçmişinde tek iş; Ctrl+Z oluşturulan dosyaları çöpe atar. URL ve metin bırakmaları dosyaya dönüşmez (§17 karar 22).

### 8.2 PATH (9b3)

- **Windows:** `%LOCALAPPDATA%\Gezik\bin\gezik.cmd` (`@start "" "<exe>" %*`) yazılır ve bu klasör `HKCU\Environment\Path`'in sonuna eklenir. Değer `RegGetValueW(…, RRF_NOEXPAND)` ile ham okunur, türü (`REG_EXPAND_SZ`) ve `%DEĞİŞKEN%`'ler korunur; ekleme/çıkarma saf işlev (büyük/küçük harf duyarsız, sondaki `\` ve boş parçalar tolere edilir). Sonra `SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment", SMTO_ABORTIFHUNG, 2000)`. Ayrıca `HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\gezik.exe` `(Default) = <exe>` (Çalıştır penceresi). Exe'nin klasörü PATH'e **eklenmez** (`Downloads` gibi bir klasörü PATH'e sokmamak için). `start` ile `%*` tırnak davranışı **doğrulanacak**.
- **macOS ve Linux:** `~/.local/bin/gezik` → exe sembolik bağlantısı (klasör yoksa 0755 oluşturulur ve günlüğe girer). `~/.local/bin` `PATH`'te değilse panel eklenecek satırı gösterir (`export PATH="$HOME/.local/bin:$PATH"`) ve `Copy` düğmesi; kabuk profilleri düzenlenmez. Aynı adda Gezik'in olmayan bir dosya varsa dokunulmaz, panel söyler.

### 8.3 Linux D-Bus (9b4'te gelir; 9b9 genişletir)

- Crate eklenmez. `gezik-platform/src/linux/dbus.rs`: oturum veriyolu adresi (`DBUS_SESSION_BUS_ADDRESS`, yalnız `unix:path=` ve `unix:abstract=`), `AUTH EXTERNAL <uid onaltılık>`, `Hello`, küçük-sonlu ileti kodlama/çözme (başlık alanları; `y b n q i u x t d s o g a v ( ) { }` imzaları, imzayla yönlendirilen `Value` türü), yöntem çağrısı, yanıt, sinyal, `RequestName`, `AddMatch`. Gezik'e yeten kadar; `Introspect` ve `Peer.Ping` karşılanır. En çok 1 MB ileti; bozuk ileti bağlantıyı kapatır.
- Kodlayıcı saf, `dbus-monitor --binary` ile alınmış bayt örnekleriyle test edilir.
- Bir iş parçacığı soketi okur (boşta CPU yok); yalnız FileManager1, portal kısayolu ya da SNI tepsisi açıkken kurulur.

### 8.4 Günlük, panel, `--unregister` (9b3)

§3.2 ve §11. 9b3, varsayılanlıktan önce gelir ki her yazan parça günlüğü hazır bulsun.

### 8.5 Linux simgeleri, küçük resimler, Birlikte aç (9b10)

- **Simge teması:** `$XDG_CURRENT_DESKTOP`'a göre `gsettings get org.gnome.desktop.interface icon-theme` (GNOME; bir kez, ilk simge isteğinde) ya da `~/.config/kdeglobals` `[Icons] Theme` (KDE); bulunamazsa `hicolor` ve `Adwaita`. `index.theme` ayrıştırılır, MIME → simge adı `/usr/share/mime/generic-icons` ve `icons` dosyalarından, MIME türü uzantıdan (`globs2`). **Yalnız PNG** simgeler kullanılır; SVG'si olan tema için Gezik'in kendi tür simgeleri kalır (SVG çizici eklenmez).
- **Küçük resimler:** freedesktop önbelleği `~/.cache/thumbnails/{normal,large}/<md5(uri)>.png` yalnız okunur (`Thumb::MTime` dosyanın zamanıyla eşleşirse). Thumbnailer çalıştırılmaz; önbellekte yoksa Gezik'in kendi 5 biçimi.
- **Birlikte aç:** `mimeapps.list` (`$XDG_CONFIG_HOME`, `$XDG_CONFIG_DIRS`, `$XDG_DATA_HOME/applications`, `$XDG_DATA_DIRS/applications`; `Default`, `Added`, `Removed`) ve `MimeType=` taşıyan `.desktop` dosyaları; ana MIME türleri `subclasses` ile. Başlatma `Exec` alanının saf ayrıştırıcısıyla (`%f %F %u %U %i %c %k`, masaüstü girdisi tırnaklama kuralı), kabuk yok. `Change All…` (Bilgi penceresi) `mimeapps.list` `[Default Applications]`'a yazar (§6.4'ün yazıcısı); bu yazma kullanıcının kendi tercihidir, günlüğe girmez (`--unregister` geri almaz).

## 9. 9b9 — Tepsi, genel kısayol, girişte başlama

### 9.1 Tepsi

- `[system] tray = false`. Açıkken pencereyi kapatmak süreci bitirmez, pencereyi gizler (ilk kez olduğunda bir kez `Gezik keeps running in the tray. Quit from the tray menu.`). Tepsi simgesine tıklamak pencereyi gösterir/gizler; menü: `Show Gezik`, sabitlenmiş ilk 9 klasör, `Quit`.
- Windows: `Shell_NotifyIconW` ayrı bir iş parçacığındaki yalnız-ileti penceresinde (`NIF_ICON | NIF_TIP | NIF_MESSAGE`, `NOTIFYICON_VERSION_4`); `TaskbarCreated` iletisiyle Explorer yeniden başlayınca simge yeniden eklenir; menü `TrackPopupMenu`.
- macOS: `NSStatusBar.systemStatusBar.statusItemWithLength:` + `NSMenu`; Dock simgesi kalır.
- Linux: StatusNotifierItem (`org.kde.StatusNotifierWatcher.RegisterStatusNotifierItem`; `/StatusNotifierItem` nesnesinde `Id`, `Title`, `Status`, `IconPixmap`, `Activate`). **Menü yok** (`com.canonical.dbusmenu` büyük; §17 karar 24): tıklama gösterir/gizler, çıkış pencereden. İzleyici yoksa (eklentisiz GNOME) panel `No tray on this desktop` der ve ayar etkisiz kalır.
- Gizli pencerede bellek: plan ölçer; hedef ≤ boşta bellek (§12).

### 9.2 Genel kısayol

- `[system] hotkey = ""` (kapalı). Panel açarken öneri: Windows `win+shift+e`, macOS `mod+alt+e` (⌥⌘E), Linux `super+shift+e`. 6a'nın akor sözdizimi + `win`/`super`. Ctrl+Alt kullanılmaz (AltGr kuralı).
- Davranış: Gezik öndeyse simge durumuna küçültür (tepside ise gizler), değilse gösterir ve öne getirir; çalışmıyorsa zaten tetiklenemez (kısayolu sahiplenen Gezik'tir).
- Windows: `RegisterHotKey(hwnd, 1, MOD_WIN | MOD_SHIFT | MOD_NOREPEAT, 'E')` tepsinin ileti penceresinde. Alınmışsa `Win+Shift+E is used by another app`.
- macOS: Carbon `RegisterEventHotKey` + `InstallEventHandler` (`extern "C"`; Erişilebilirlik izni istemez). macOS 15'in yalnız ⌥/⇧'li kısayol kısıtı yüzünden öneri ⌘ içerir.
- Linux: X11'de `XGrabKey` (x11rb, kök pencere, NumLock/CapsLock varyantlarıyla); Wayland'de `org.freedesktop.portal.GlobalShortcuts` (`CreateSession`, `BindShortcuts`, `Activated` sinyali) §8.3 ile; portal yoksa `This desktop does not support global shortcuts`.

### 9.3 Girişte başlama

- `[system] start-at-login = false`. Açılınca yazılan kayıt günlüğe girer; kapanınca silinir. Açılışta yalnız ayar `true` ise kaydın var olduğu denetlenir (yoksa yeniden yazılır).
- Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` `Gezik = "<exe>" --background`.
- macOS: `~/Library/LaunchAgents/com.wenlar.gezik.plist` (`ProgramArguments = [<exe>, --background]`, `RunAtLoad = true`); `launchctl` çağrılmaz (bir sonraki girişte yüklenir). `SMAppService` paket ister, kullanılmaz (§17 karar 25).
- Linux: `~/.config/autostart/gezik.desktop` (`Exec="<exe>" --background`).
- `--background`: tepsi açıksa pencere gizli başlar; tepsi kapalıysa pencere açılır (yalnız kısayol açıksa da açılır; gizli ve tepsisiz bir Gezik'e kullanıcının ulaşacağı yer olmaz).

## 10. 9b7 — Yönetici yardımcısı

### 10.1 Ne zaman

- Yalnız kullanıcının açık isteğiyle: işlem panelinin sorun penceresinde erişim reddiyle biten öğeler varsa `Retry as administrator` düğmesi; Bilgi penceresinde `Change as administrator…`. Kendiliğinden yükseltme yok; hiçbir ayar bunu açmaz.
- Desteklenen işler: `copy`, `move`, `delete` (kalıcı; çöp yok), `rename`, `mkdir`; Unix'te ayrıca `chmod`, `chown`, `chflags` (macOS). Windows'ta öznitelik işi yoktur (sistem Özellikler penceresi kendi UAC'sini yönetir).

### 10.2 Akış

1. Üst süreç (normal Gezik) işleri hazırlar: çakışmaları **yükseltmeden önce** sorar (hedefleri yetkisiz `stat` ile görebilir), kullanıcının seçimine göre her iş `fail-if-exists` ya da `replace` kipinde olur. Silmede `This will delete 3 items permanently as administrator. It cannot be undone.` onayı.
2. İş listesi komut satırına kodlanır. Sığmazsa (Windows 32.767 UTF-16 karakter; macOS/Linux için §10.3) iş reddedilir: `Too many items for one administrator operation; select fewer.` Bölünmez (birden çok yetki penceresi çıkmasın).
3. Yardımcı başlatılır:
   - Windows: `ShellExecuteExW` (`lpVerb = "runas"`, `lpFile = <exe>`, `lpParameters` = kodlanmış satır, `SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC`), süreç tutamağıyla beklenir. Sonuçlar için üst süreç önceden rastgele adlı (`\\.\pipe\gezik-elev-<128 bit rastgele>`) tek örnekli bir boru açar (`FILE_FLAG_FIRST_PIPE_INSTANCE`, DACL: kullanıcı + Administrators); yardımcı yalnız **yazar**. UAC iptali `ERROR_CANCELLED` → `Cancelled`.
   - macOS: `/usr/bin/osascript -e 'do shell script "<komut>" with administrator privileges'`; `<komut>` yalnız exe yolu ve her bağımsız değişkenin `quoted form`'udur. Tırnaklama saf işlev (önce sh tek tırnağı, sonra AppleScript dizgesi kaçışı), bulanık testli. Sonuçlar osascript'in stdout'u (iş bitince toplu gelir; ilerleme yok, panel döner çubuk gösterir). İptal: çıkış kodu 1 ve `-128`.
   - Linux: `pkexec <exe> --elevated …`; sonuçlar stdout'tan akarak. `pkexec` yoksa `Administrator operations need pkexec (polkit).`; 126/127 iptal ve yetkisiz.
4. Yardımcı her işin sonucunu satır satır yazar (`ok <i>`, `err <i> <kod> <ileti>`, sonda `done`) ve çıkar. Üst süreç sonuçları iş motorunun `Outcome`'larına çevirir ve **diskten yeniden `stat` ile doğrular** (sonuç kanalına körü körüne güvenmez).
5. İş geçmişine "yönetici" işaretli tek girdi. Ctrl+Z ters işleri (copy → delete, move/rename → geri taşı, mkdir → boşsa rmdir, chmod/chown/chflags → önceki değer) **yeni bir yetki penceresiyle** çalıştırır; onay: `Undoing this needs administrator rights.` Silme geri alınamaz.

### 10.3 Komut satırı biçimi

```
gezik --elevated 1 <channel> <op> <args>... [<op> <args>...]...
  copy SRC DST | copy-replace SRC DST | move SRC DST | move-replace SRC DST
  delete PATH | rename PATH NEWNAME | mkdir PATH
  chmod PATH MODE | chmod-r PATH MODE | chown PATH UID GID | chown-r PATH UID GID
  chflags PATH SET CLEAR            (macOS: hidden, uchg)
```

- `1` sürümdür; uyuşmazsa yardımcı hiçbir şey yapmadan çıkar. `<channel>` Windows'ta boru adı, Unix'te `-` (stdout). Her işin bağımsız değişken sayısı sabittir (ayraç yok).
- Windows bağımsız değişkenleri `CommandLineToArgvW` kuralıyla tırnaklanır (saf `quote_arg`, gidiş-dönüş testi gerçek `CommandLineToArgvW` ile).
- Sınırlar: macOS ve Linux'ta toplam ≤ 128 KB (ARG_MAX'tan çok küçük; pkexec ve osascript katmanları için pay), her bağımsız değişken ≤ 32 KB.

### 10.4 Tehdit modeli ve önlemler

**Saldırgan:** kullanıcıyla aynı hakları olan bir süreç (kötü amaçlı yazılım, ele geçirilmiş bir uygulama). Hedefi: kullanıcının verdiği yönetici onayını, kullanıcının istemediği bir şeye (sistem dosyasını silmek, korunan dosyayı okunur yere kopyalamak, setuid ikili bırakmak) çevirmek.

| Tehdit | Önlem |
|---|---|
| İş listesini değiştirmek (ara dosya) | İş listesi yalnız komut satırında; hiçbir zaman dosyada, ortam değişkeninde ya da boruda/sokette. Yardımcı stdin okumaz. |
| Sahte iş, bozuk bağımsız değişken | Yardımcı her şeyi baştan doğrular: işler beyaz listede; her yol mutlak, normalleştirilmiş (`.`/`..`/boş parça yok), NUL ve denetim karakteri yok, Windows'ta `\\?\`, `\\.\` aygıt yolları ve ADS (`:`) yok; `NEWNAME` tek parça ad; `MODE` ≤ `0o1777` ve setuid/setgid bitleri **reddedilir** (sticky yalnız klasörde); `UID`/`GID` sayı. Bir iş geçersizse **hiçbiri** çalışmaz. |
| Kök ve sistem klasörleri | Yardımcı bir dosya sistemi kökünü, ev klasörünün kendisini ve işletim sistemi köklerini (`C:\Windows`, `C:\Program Files`, `C:\Program Files (x86)`, `C:\ProgramData`, `/System`, `/usr`, `/bin`, `/sbin`, `/etc`, `/var`, `/Library`, `/boot`, `/lib*`) **kendileri** silmez, taşımaz, adını değiştirmez, `-r` ile izin/sahip değiştirmez. Altlarındaki tek tek öğeler serbesttir (kullanıcı onay penceresinde yolu görür). |
| TOCTOU: onaydan sonra bir yol parçasını bağlantıyla değiştirmek | Yardımcı hiçbir bağlantıyı izlemez. Windows: her tutamak `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS` ile açılır; üst klasör tutamağı `FILE_SHARE_DELETE`'siz tutulur (iş sürerken adı değiştirilemez) ve `GetFinalPathNameByHandleW` istenen yolla aynı değilse iş durur; silme ve taşıma tutamak üzerinden (`SetFileInformationByHandle`, `FileDispositionInfoEx` POSIX anlamıyla, `FileRenameInfoEx`); yeni dosya `CREATE_NEW`. Unix: yol kökten `openat(…, O_NOFOLLOW | O_DIRECTORY | O_CLOEXEC)` ile parça parça açılır (Linux 5.6+'da `openat2` `RESOLVE_NO_SYMLINKS`; macOS 11+'da `O_NOFOLLOW_ANY`); işler üst klasör fd'sine göre (`unlinkat`, `renameat`, `mkdirat`, `fchmod`/`fchown` `O_NOFOLLOW` ile açılmış fd üzerinde); sembolik bağlantının izni değiştirilmez. Özyineleme klasör fd'leriyle, bağlantıya inilmeden (güvenli `rm -rf` kalıbı). |
| Kopyada hedefi yarım bırakmak | `*-replace` işleri hedef klasörde geçici adla (`.gezik-<rastgele>`) yazar, sonra yerine taşır (`renameat` / `FileRenameInfoEx` `REPLACE_IF_EXISTS`); hata olursa geçici silinir, eski hedef yerinde kalır. |
| Sahte sonuç (boruya bağlanmak) | Boru adı rastgele, tek örnekli; olsa olsa sonuç gösterimi bozulur, üst süreç diskten doğrular. Sonuç kanalı hiçbir zaman komut taşımaz. |
| DLL yükleme (Windows) | Yardımcı kipinde ilk iş `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`; statik içe aktarımların hepsinin KnownDLLs/System32'de olduğu plan sırasında `dumpbin /imports` ile denetlenir (**doğrulanacak**). |
| Ortam ve ayarlar | Yardımcı ayarları, temayı, `views.toml`'u okumaz; pencere, Slint, COM UI açmaz; ortam değişkenlerini kullanmaz; çalışma klasörünü `/` (Windows'ta System32) yapar; Unix'te `umask 022`. Yardımcı gerçekten yükseltilmiş değilse (Windows belirteç yükseltmesi, Unix `geteuid() != 0`) çıkar. |
| Exe değiştirme | Taşınabilir Gezik kullanıcının yazabildiği bir klasördedir; aynı haklarla çalışan saldırgan exe'yi değiştirebilir. Bu, her taşınabilir uygulamanın durumudur: onay penceresi programın yolunu (Windows'ta imzasız yayıncı uyarısını) gösterir. Belgede yazılır; imzalama Yayın hazırlığında. |
| Uzun ömürlü yetki | Yok: yardımcı başka iş almaz, dinlemez, son işten hemen sonra çıkar. macOS'ta `do shell script`'in yetki önbelleği (≈5 dk) Gezik'in değil sistemin davranışıdır; Gezik bunu kullanmaz, her iş yeni bir `osascript`'tir. |

### 10.5 Gezik'in asla yapmadıkları

- Kalıcı yetkili süreç, servis, daemon, LaunchDaemon, `SMJobBless`, polkit kuralı, sudoers girdisi kurmak.
- HKLM'e, sistem PATH'ine, `/etc`, `/usr`, `/Library`'ye kendi adına yazmak (yardımcı yalnız kullanıcının seçtiği dosya işini yapar).
- Explorer'a, Finder'a ya da başka bir sürece kod enjekte etmek; kanca kurmak; UAC/SIP/Gatekeeper ayarlarına dokunmak.
- Kullanıcı verisinden kabuk komutu kurup yetkili çalıştırmak (tek istisna macOS `do shell script` sarmalayıcısı; içeriği yalnız exe yolu ve tırnaklanmış argv'dir).
- Parola, ağ kimliği ya da belirteç saklamak; kimlik soruları işletim sisteminin penceresindedir (Linux `gio`'ya stdin ile geçen parola dışında, o da bellekte kalmaz).
- Setuid/setgid biti koymak; sembolik bağlantı izleyerek yetkili iş yapmak.
- İlk açılışta ya da güncellemede kendiliğinden sisteme yazmak.

### 10.6 Diğer güven sınırları

- **Tek örnek kanalı:** yalnız aynı kullanıcı; içerik komut satırıyla aynı güvende, aynı ayrıştırıcıdan geçer; boyut sınırlı.
- **FileManager1 (D-Bus):** oturumdaki her uygulama çağırabilir; yalnız aç/seç/bilgi göster, hiçbir şey çalıştırmaz, yalnız `file://` ve `trash:///`.
- **Sanal bırakma adları:** §8.1.
- **Kayıt defteri komut değerleri:** exe yolu tırnaklı; `%1` tırnaklı; değerler saf işlevde kurulur, testli.
- **`.desktop` ve plist metinleri:** exe yolu masaüstü girdisi ve XML kaçışıyla; saf işlev, testli.

## 11. Günlük ve geri alma

### 11.1 `system-changes.toml`

Yapılandırma klasöründe; yalnız Gezik yazar. Her değişiklikten **önce** girdi eklenir (yarıda kalan değişiklik de geri alınabilsin), sonra değişiklik yapılır, sonra girdi `done = true` olur.

```toml
version = 1
exe = 'C:\Tools\Gezik\gezik.exe'

[[change]]
feature = "default-file-manager"     # default-file-manager | path | start-at-login | app-bundle
kind = "registry-value"              # registry-value | registry-key | file | symlink | mimeapps | macos-default | macos-pref
where = 'HKCU\Software\Classes\Directory\shell'
name = ""                            # (Default)
before = { absent = true }           # or { type = "REG_SZ", data = "none" }
after = { type = "REG_SZ", data = "gezik" }
done = true

[[change]]
feature = "path"
kind = "symlink"
where = "/home/u/.local/bin/gezik"
after = { target = "/home/u/apps/gezik/gezik" }
done = true
```

### 11.2 Kurtarma dosyası (Windows)

Varsayılanlık her yazıldığında yapılandırma klasörüne `restore-explorer.reg` yazılır (günlükten üretilir; `Windows Registry Editor Version 5.00`, önceki değerler ya da `-` ile silme). HKCU olduğu için çift tıkla yönetici istemeden içe aktarılır. Exe silinmiş ya da bozulmuşsa tek kurtarma yolu budur; panel ve belge yerini söyler.

### 11.3 Ne zaman okunur

Açılışta okunmaz. Yalnız: panel açılınca, `--unregister`'da, bir özellik açılıp kapanırken, açılıştan 2 sn sonra boşta **dosya varsa** (tek `metadata` çağrısı; exe taşındı denetimi, §6.3; Linux'ta FileManager1 adının istenip istenmeyeceği de buradan anlaşılır, §6.4).

### 11.4 Geri alma kuralı

- Girdiler ters sırayla geri alınır. Her girdi için **şu anki değer `after` ile aynıysa** `before`'a döndürülür (`absent` → silinir); değilse dokunulmaz ve rapora `changed by something else since; left as is` yazılır. Gezik'in oluşturduğu anahtarlar (`…\shell\gezik`) ve dosyalar (içerik özeti eşleşirse; sembolik bağlantı hedefi eşleşirse) silinir. Boşalan ve Gezik'in oluşturduğu klasörler (`%LOCALAPPDATA%\Gezik\bin`, `~/.local/bin` yalnız Gezik oluşturduysa) silinir.
- **Günlük kaybolmuşsa** (yapılandırma klasörü silinmiş): sabit adlarla süpürme yapılır: `…\shell\gezik` anahtarları silinir; `Directory|Drive|Folder\shell` `(Default)` `gezik` ise silinir; Win+E CLSID anahtarı komutu Gezik'in exe adını içeriyorsa silinir; `Run\Gezik`, `App Paths\gezik.exe`, `%LOCALAPPDATA%\Gezik\bin`; macOS `NSFileViewer == com.wenlar.gezik` ise silinir, `public.folder` varsayılanı Gezik ise `com.apple.finder`, `~/Library/LaunchAgents/com.wenlar.gezik.plist`, `~/Applications/Gezik.app` (paket kimliği eşleşirse); Linux `gezik.desktop` dosyaları, `mimeapps.list`'te `=gezik.desktop` değerleri, FileManager1 `.service` dosyası (Exec Gezik ise), `~/.local/bin/gezik` (Gezik'i gösteriyorsa).
- `--unregister` konsola her satırı yazar ve çıkış kodu: 0 hepsi geri alındı, 1 bazıları dokunulmadan bırakıldı, 2 hata. Palet komutu `Undo all system changes` aynı raporu panelde gösterir.
- Geri alma sonrası günlük boşalır (dosya silinir); `restore-explorer.reg` de silinir.
- Uygulama yerinde güncellenince (exe aynı yolda) günlük geçerli kalır.

## 12. Bütçe ve performans

- **Exe:** her parça ≤ +0,25 MiB, `master` 5f950fc'nin sürüm derlemesine göre; plan her parçanın ilk görevinde bayt olarak ölçüp yazar. Yeni Windows özellikleri `windows` crate'inde (`Win32_System_Registry`, `Win32_System_Pipes`, `Win32_System_Console`, `Win32_Storage_CloudFilters`, `Win32_NetworkManagement_NetManagement`, `Win32_Security_Authorization`); yeni crate yok. macOS: `objc2-quick-look-thumbnailing`, `objc2-quick-look-ui` (ya da `objc2-quartz`), `objc2-core-graphics` gerekirse, aynı `objc2` 0.6 ailesi; NetFS, Carbon kısayolu ve LaunchServices `extern "C"`. Linux: yeni crate yok.
- **Boşta bellek:** varsayılan ayarlarla `master`'a göre ≤ +0,1 MB (tek örnek dinleyicisinin iş parçacığı); her özellik kapalıyken hiçbir yeni yapı ayrılmaz (simge önbelleği, çöp listesi, bulut kökleri ilk kullanımda). Tepsi + kısayol ≤ +0,5 MB; Linux D-Bus bağlantısı ≤ +0,3 MB; tepside gizli pencereyle bekleyen Gezik ≤ boşta bellek (**doğrulanacak**: Slint yazılım çizicisinin arabelleği gizli pencerede bırakılıyor mu; bırakılmıyorsa plan bunu ölçüp not eder).
- **Açılış:** ≤ +2 ms (tek örnek yoklaması: bağlanma denemesi; bulunamazsa dinleyici açma). Günlük, bulut kökleri, çöp açılışta okunmaz.
- **Zamanlayıcı ve yoklama yok:** hepsi olayla (izleyici, `WM_DEVICECHANGE`, kullanıcı eylemi). Tek istisna exe-taşındı denetiminin bir kerelik 2 sn gecikmesi.
- **macOS simge ve küçük resim:** 1.000 öğelik klasörde ilk çizim ≤ 150 ms (uzantı önbelleğiyle), kaydırma kare süresi `master` ile aynı; küçük resim istekleri ekrandan çıkınca iptal.
- **Ölçüm betikleri:** `measure.ps1`, `stress.ps1` her parçanın sonunda; yeni `scripts/perf/instance.ps1` (100 ardışık `gezik <klasör>` çağrısının var olan pencerede açılma süresi, p50/p95).

## 13. Ayarlar, eylemler, menüler

### 13.1 `settings.toml`

```toml
[system]
single-instance = true   # a second `gezik` opens in the running window
tray = false             # an icon in the tray / menu bar; closing the window keeps Gezik running
hotkey = ""              # a system-wide shortcut that shows Gezik, e.g. "win+shift+e"; "" = off
start-at-login = false
quick-look = "system"    # macOS: Space opens the system Quick Look panel; "gezik": Gezik's own

[sidebar]
cloud = true             # OneDrive, iCloud Drive, Dropbox, Google Drive under CLOUD
```

Hatalı değerler bugünkü kalıpla uyarır (`system.hotkey: "ctrl+alt+e" uses Ctrl+Alt, which types AltGr characters`, `system.quick-look: expected "system" or "gezik"`). Varsayılanlık ve PATH ayar değildir (§3.1).

### 13.2 Eylemler ve varsayılan tuşlar

| Eylem | Windows / Linux | macOS | Parça |
|---|---|---|---|
| `make-alias` | — | ⌃⌘A | 9a1 |
| `show-package-contents` | — | — | 9a1 |
| `share` | — | — | 9a2 |
| `get-info` | Alt+Enter (Windows: sistem Özellikler) | ⌘I | 9a3 |
| `new-window` | Ctrl+N | ⌘N | 9b1 |
| `show-trash`, `put-back` | — | — | 9b2 |
| `empty-trash` | — | ⌘⇧⌫ | 9b2 |
| `system-integration` | — | — | 9b3 |
| `always-keep-offline`, `free-up-space` | — | — | 9b5 |
| `eject` | — | ⌘E | 9b6 |
| `connect-to-server` | Ctrl+K | ⌘K | 9b6 |

- `Action` 72 → 76 (9a) → 85 (9b). Çakışma denetimi 6a–8b varsayılanlarıyla, `fixed_owner` ve macOS menü çubuğunun sabitleriyle; `mod+n`, `mod+k`, `mod+i`, `mod+e`, `alt+enter`, `mod+ctrl+a`, `mod+shift+backspace` bugün boş. Arama çubuğundaki ve paletteki Alt+Enter alan odaktayken önce gelir (bugünkü sıra).
- macOS menü çubuğu: File: `New Window`, `Get Info`, `Make Alias`, `Show Package Contents`, `Share…`, `Eject`; Go: `Trash`, `Connect to Server…`; Gezik menüsü: `System Integration…`, Finder'daki gibi `Empty Trash…`.
- Hepsi `templates/settings.toml`'un `[shortcuts]` yorumlarına ve "her varsayılan ulaşılabilir" testine eklenir.

### 13.3 Menü kimlikleri

- **9a (1700–1799):** `OPEN_WITH_FIRST` 1700–1739, `OPEN_WITH_OTHER` 1740, `SHARE` 1741, `GET_INFO` 1742, `MAKE_ALIAS` 1743, `SHOW_PACKAGE` 1744, `QUICK_ACTION_FIRST` 1750–1779, Bilgi penceresi seçimleri (sahip/grup tamamlama, `Change All…`) 1780–1799.
- **9b (1800–1999):** `SHOW_TRASH` 1800, `PUT_BACK` 1801, `TRASH_DELETE` 1802, `EMPTY_TRASH` 1803, `EJECT` 1810, `DISCONNECT` 1811, `CONNECT_SERVER` 1812, son sunucular 1820–1829, sürücü harfleri 1830–1855, `KEEP_OFFLINE` 1860, `FREE_UP` 1861, `RETRY_ADMIN` 1870, `NEW_WINDOW` 1880, `SYSTEM_INTEGRATION` 1881, Linux Open With 1900–1939, `OPEN_WITH_OTHER_LINUX` 1940.

## 14. Parçalar (her biri bir plan ve bir PR)

| Parça | İçerik | Bağımlılık |
|---|---|---|
| **9a1** | macOS sistem simgeleri (§4.1), QuickLook küçük resimleri (§4.5), Finder adları, takma adlar, paketler (§4.2) | — |
| **9a2** | macOS Open With, Share, Quick Actions/Services, sistem Quick Look paneli (§4.3, §4.5) | 9a1 (simge önbelleği) |
| **9a3** | Bilgi penceresi macOS + Linux, `SetAttributesTask`, Windows Alt+Enter → sistem Özellikler (§4.4) | — |
| **9b1** | Komut satırı, tek örnek, `new-window` (§5) | — |
| **9b2** | Çöp görünümü, üç sistem (§7.1) | — |
| **9b3** | Günlük, sistem bütünleşmesi paneli, `--unregister`, PATH (§3.2, §8.2, §11) | 9b1 (konsol çıktısı) |
| **9b4** | Varsayılan dosya yöneticisi: macOS (Gezik.app, LaunchServices, NSFileViewer), Windows (HKCU, sanal klasör eşleme, geri düşüş, `restore-explorer.reg`), Linux (`.desktop`, `mimeapps.list`, D-Bus kodlayıcı, FileManager1) (§6, §8.3) | 9b1, 9b2 (Geri Dönüşüm Kutusu eşlemesi), 9b3 |
| **9b5** | Bulut: kenar çubuğu, durum işaretleri, indir/boşalt (§7.2, §7.3) | — |
| **9b6** | Çıkarma, sunucuya bağlan, Windows sunucu paylaşımları (§7.4) | — |
| **9b7** | Yönetici yardımcısı, `Retry as administrator`, Bilgi penceresinin yönetici düğmesi (§10) | 9a3 |
| **9b8** | Sanal dosya bırakma, Windows + macOS (§8.1) | — |
| **9b9** | Tepsi, genel kısayol, girişte başlama, `--background` (§9) | 9b1, 9b3, 9b4 (Linux D-Bus) |
| **9b10** | Linux simge teması, küçük resim önbelleği, Birlikte aç (§8.5) | 9a2 (menü yolu), 9b4 (`mimeapps` yazıcısı) |
| **9b11** | "Klasörde göster" (`SHOpenFolderAndSelectItems`, `explorer /select`) çağrılarını Gezik'e yönlendirme; önce yoklama, kanca yok, HKCU, geri alınabilir; güvenli yol yoksa düşer | 9b4 |

Sıra tablodaki gibidir; kullanıcı kararıyla 9a ve 9b paralel yürür (9a derlenir ve macOS testi kullanıcının MacBook'unda yapılır; bu sırada Windows'ta denenebilen 9b parçaları ilerler). 9a'nın ve 9b'nin her parçasında macOS → Windows → Linux sırasıyla yazılır; bir sistemde yoklama başarısız olursa o parçanın o sistemdeki kısmı "doğrulanacak" notuyla bir sonraki parçaya kayar ve PR açıklamasında yazılır.

## 15. Kod yapısı

| Parça | Yer |
|---|---|
| CLI ayrıştırma, Windows sanal klasör eşleme (saf) | `gezik/src/cli.rs` (yeni), `gezik/src/start.rs`, `gezik/src/main.rs` (iç bayraklar, `AttachConsole`) |
| Tek örnek kanalı ve iletisi | `gezik-platform/src/instance.rs` (yeni; ileti kodlama saf) |
| Günlük okuma/yazma, geri alma kuralı (saf) | `gezik-config/src/system_journal.rs` (yeni) |
| Sistem yazıcıları (kayıt defteri, LaunchServices, NSFileViewer, `mimeapps.list`, `.desktop`, plist, `.reg`, PATH) | `gezik-platform/src/system/{mod, windows, macos, linux, text}.rs` (yeni; `text.rs` bütün metin üreticileri ve düzenleyicileri, saf, her sistemde testli) |
| Panel | `gezik/src/integration.rs`, `gezik/ui/widgets/integration.slint` (yeni) |
| Yönetici iş modeli, kodlama, doğrulama (saf) | `gezik-core/src/elevated.rs` (yeni) |
| Yetki isteme, sonuç kanalı | `gezik-platform/src/elevate.rs` (yeni) |
| Bağlantı izlemeyen dosya işleri | `gezik-platform/src/fs/secure_{windows, unix}.rs` (yeni) |
| Yardımcının `main`'i | `gezik/src/elevated.rs` (yeni; `main.rs`'te `--pdf-worker` gibi en başta) |
| İş motoru: yükseltilmiş iş, öznitelik işi, sanal bırakma işi | `gezik-ops/src/tasks/{elevated, attrs, materialize}.rs` (yeni) |
| Çöp: kutular, `$I`, `.trashinfo`, `.DS_Store` okuyucuları | `gezik-platform/src/trash/{mod, windows, macos}.rs` (yeni), `fs/freedesktop.rs` (okuyucu), `gezik-core/src/nav.rs` (`Location::Trash`), `gezik/src/view/listing.rs` (`Listing::Trash`), `gezik/src/trash_view.rs` (yeni) |
| Bulut kökleri ve durum | `gezik-platform/src/cloud.rs` (yeni), `gezik/src/sidebar.rs`, `gezik/src/view/model.rs` |
| Çıkarma, sunucuya bağlanma, paylaşım listesi | `gezik-platform/src/{eject, network}.rs` (yeni), `gezik/src/connect.rs`, `gezik/ui/widgets/connect.slint` (yeni) |
| Sanal bırakma | `gezik-platform/src/dnd/{windows, macos}.rs`, ad temizleme `gezik-core/src/drop_names.rs` (yeni, saf) |
| Tepsi, kısayol, girişte başlama | `gezik-platform/src/{tray, hotkey, login}.rs` (yeni), `gezik/src/resident.rs` (yeni) |
| Linux D-Bus ve kullanıcıları | `gezik-platform/src/linux/{dbus, file_manager1, sni, portal}.rs` (yeni) |
| macOS yerel parçalar | `gezik-platform/src/icons.rs`, `picture.rs`, `known.rs` (macOS `imp`), `gezik-platform/src/mac/{ql_panel, open_with, share, services, alias, package, info}.rs` (yeni klasör) |
| Bilgi penceresi | `gezik/src/info.rs`, `gezik/ui/widgets/info.slint` (yeni), `gezik-platform/src/fs/unix.rs` (`chmod`, `chown`, `chflags`, okuma) |
| Linux simge, küçük resim, Birlikte aç | `gezik-platform/src/linux/{icon_theme, thumbs, mime, desktop_entry}.rs` (yeni) |
| Ayarlar, eylemler, menüler | `gezik-config/src/{settings, settings_edit, settings_writer, shortcuts}.rs`, `templates/settings.toml`, `gezik/src/{keys, actions, menu_bar, context_menu}.rs` |

## 16. Test

### 16.1 Birim (saf; her sistemde koşar)

- **CLI:** bayraklar, birden çok yol, `--select` tekrarı, bilinmeyen bayrak uyarısı, göreli yolların mutlaklaştırılması; Windows sanal hedef eşlemesi (her CLSID, `::{X}\alt`, harf büyüklüğü, `\\sunucu`, arşiv/arşiv olmayan dosya).
- **Tek örnek iletisi:** kodlama/çözme, 1 MB ve 1.000 yol sınırı, bozuk ileti, sürüm uyuşmazlığı, Unix ham bayt yollar.
- **Günlük:** yazma/okuma, `done = false` girdinin geri alınması, ters sıra, "şu anki ≠ after ise dokunma", günlük yokken süpürme listesi, `restore-explorer.reg` metni (önceki değer, `absent` → `-`).
- **Metin üreticileri:** kayıt defteri planı (exe yolu → anahtar/değer listesi, tırnaklama, boşluklu ve Unicode yol), PATH ekleme/çıkarma (`%VAR%`, sondaki `\`, `;;`, büyük/küçük harf, yinelenen giriş), `mimeapps.list` düzenleme (bölüm yok, değer var, yorum ve sıra korunur, masaüstüne özgü dosya), `.desktop` (kaçış), LaunchAgent plist (XML kaçışı), `Info.plist` belge türleri, FileManager1 `.service`.
- **Yönetici işleri:** kodlama/çözme gidiş-dönüş; doğrulama reddeder: göreli yol, `..`, NUL, `\\?\`, `\\.\`, ADS, `NEWNAME`'de ayraç, setuid/setgid, `MODE > 0o1777`, bilinmeyen iş, eksik bağımsız değişken, kök ve sistem klasörleri; bir iş geçersizse hiçbirinin çalışmaması. Windows `quote_arg` (`CommandLineToArgvW` ile gidiş-dönüş, `"`, `\`, sondaki `\`, boşluk). AppleScript/sh tırnaklama bulanık testi (`'`, `"`, `\`, `$()`, `` ` ``, satır sonu, Unicode, çok uzun ad): üretilen komut `sh -c` içinde aynı argv'yi verir (macOS ve Linux'ta gerçek `sh` ile).
- **Çöp:** `$I` v1/v2 bayt örnekleri, bozuk/kısa dosya; `.trashinfo` okuma (yüzde kodlama, göreli `Path=` birim köküne göre); `.DS_Store` `ptbL`/`ptbN` örneği (bir macOS'tan alınmış örnek dosya); çöpten çıkarmanın bilgi dosyasını da silmesi (geçici ağaçta sahte kutu).
- **Bulut:** öznitelik → durum eşlemesi, `SF_DATALESS`; `SyncRootManager` örnek kayıt yapısının ayrıştırılması (sahte değerlerle); macOS `CloudStorage` klasör adı → sağlayıcı/hesap.
- **Ağ:** adres ayrıştırıcı (`\\s\p`, `smb://s/p`, `s/p`, boşluk, Unicode, geçersiz).
- **Sanal bırakma adları:** mutlak, `..`, sürücü harfi, ayrılmış adlar, geçersiz karakterler, uzunluk, göreli klasörlerin kökten çıkamaması.
- **D-Bus:** başlık ve gövde kodlama/çözme (bayt örnekleri), hizalama, imza doğrulama, 1 MB sınırı, bozuk ileti; `ShowItems` URI listesinin `linux/uri.rs` ile çözülmesi.
- **Linux:** `Exec` alan kodları ve tırnaklama, `mimeapps.list` öncelik sırası, `globs2` ve `subclasses`, `index.theme` arama sırası, küçük resim önbelleği adı (`md5(uri)`) ve `Thumb::MTime` denetimi.
- **Öznitelik işi:** önceki değerlerin kaydı ve geri alma (geçici dosyalarda, Unix); özyinelemede bağlantının izlenmemesi.
- **Kısayollar:** yeni varsayılanların çakışmaması, ulaşılabilirlik, `hotkey` ayarının ayrıştırılması ve Ctrl+Alt reddi.

### 16.2 Platform testleri (gerçek sistem; `#[ignore]` olanlar elle)

- **Güvenli dosya işleri (Unix, CI'da):** geçici ağaçta bir parça sembolik bağlantıyla değiştirilmişken `delete`, `chmod-r`, `copy` bağlantıyı izlemez ve durur; `openat2`'siz yol (zorla) aynı sonucu verir.
- **Güvenli dosya işleri (Windows):** junction'lı ağaçta `delete` junction'ın hedefine dokunmaz; `GetFinalPathNameByHandleW` uyuşmazlığında durma.
- **Kayıt defteri:** yazıcılar bir kök parametresiyle `HKCU\Software\GezikTest\Classes` altında koşar (gerçek `Classes`'a dokunmadan); yazma → anlık görüntü → geri alma → anlık görüntü aynı.
- **Tek örnek:** iki süreç, ileti, bayat soket, farklı uid'nin reddi (Linux'ta `sudo -u` ile elle).
- `$Recycle.Bin` okuma (Windows, gerçek çöpe atılmış geçici dosya ile).

### 16.3 Çapraz denetim

Planın sonunda (her görevde değil): `cargo check` macOS ve Linux hedefleri, Windows `clippy`/`fmt`/`test`. Docker yok.

### 16.4 Elle denetim listeleri

- **macOS** (`macos-test.md`'ye eklenir, sonuçlar `macos-test-results.md`'ye): simgeler (uygulamalar, özel klasör simgesi, birimler), QuickLook küçük resimleri (PDF, HEIC, MOV, Pages), Türkçe sistemde "Belgeler/Uygulamalar", alias açma/oluşturma, `.app` çift tık ve `Show Package Contents`, Open With listesi ve `Other…`, Share → AirDrop, Quick Actions, Space → sistem paneli ve oklar, Get Info'da izin/sahip/gizli/kilitli ve Ctrl+Z, `Change All…`, çöp (Tam Disk Erişimi yokken bant, varken liste, Put Back, Empty), iCloud ve OneDrive durumu, indir/boşalt, ⌘E, ⌘K SMB, Mail ve Photos'tan sürükleme, Make default sonrası `open ~/Documents` ve bir uygulamanın "Reveal in Finder"ı, `--unregister`, tepsi, ⌥⌘E, girişte başlama, yönetici ile `/Applications`'a kopya ve iptal.
- **Windows** (kullanıcı uzaktayken; klavye düzenine dokunulmaz; varsayılanlık denemesinden önce `restore-explorer.reg` hazır): Make default sonrası Win+E, masaüstü klasörü, Başlat menüsü klasörü, `start .`, Çalıştır'da yol, masaüstündeki Geri Dönüşüm Kutusu ve This PC, Denetim Masası'nın Explorer'a düşmesi ve döngü olmaması, Gezik asılıyken Win+E, exe taşınmışken uyarı, `--unregister` ve `.reg` ile kurtarma, PATH (yeni cmd'de `gezik .`), tek örnek (100 ardışık çağrı), çöp (iki sürücü, Put Back çakışması, Empty), OneDrive işaretleri ve iki komut, USB çıkarma, `\\sunucu`, ⌘K eşdeğeri Ctrl+K ile sürücü harfi eşleme, Outlook eki ve Chrome resmi sürükleme, tepsi + Win+Shift+E + girişte başlama, `Program Files`'a kopya ve UAC iptali, Ctrl+Z ile yönetici işinin geri alınması.
- **Linux** (gerçek makine, GNOME ve KDE, X11 ve Wayland): Make default sonrası Firefox "Klasörde göster" (FileManager1), `xdg-open ~`, çöp (ev ve USB kutusu), `gio`'lu ve `gio`'suz çıkarma, `smb://` bağlanma, Bilgi penceresi ve `pkexec` ile sahip değişimi, tepsi (KDE'de var, eklentisiz GNOME'da yok), Wayland portal kısayolu, simge teması (PNG ve SVG tema), küçük resim önbelleği, Birlikte aç ve `Change All…`, `--unregister`.

## 17. Ayrıntı kararları (kullanıcı onayı bekleyen)

Her madde önerilen seçimdir; ayraçta seçenek. Onaylanmayan madde plandan önce değişir.

1. **Parçalar:** 9a1–9a3, 9b1–9b10 (§14), her biri bir PR; 9a önce. (Seçenek: 9b3 ile 9b4'ü birleştirmek.)
2. **Varsayılan CLI davranışı:** çalışan Gezik'te aynı klasörü gösteren sekme varsa ona geçer, yoksa yeni sekme; `--new-tab` her zaman yeni sekme; bayraksız çağrı (Win+E) `start-folder`. (Seçenek: her çağrı yeni sekme.)
3. **Windows'ta Get Info:** Gezik paneli yok; Alt+Enter sistemin Özellikler penceresini açar (izinler, öznitelikler, güvenlik zaten orada). (Seçenek: Windows'ta da Gezik paneli.)
4. **Bilgi penceresinin sınırı:** izin, sahip, grup, gizli, kilitli ve "Birlikte aç" düzenlenir; zaman damgası ve ACL düzenlenmez (ACL varlığı gösterilir); setuid/setgid gösterilir ama değiştirilemez; değişiklik anında uygulanır, Ctrl+Z geri alır.
5. **Windows varsayılanlık mekanizması:** `Directory`, `Drive`, `Folder` altında `shell\gezik` + varsayılan fiil, Win+E için CLSID `{52205fd8-…}`; komut `--shell "%1"`; sanal klasör eşlemesi §6.1'deki tablo, geri kalanı Explorer'a.
6. **"Klasörde göster" yönlendirmesi:** kullanıcı kararıyla 9b11 (yoklamalı; kanca yok, güvenli yol yoksa düşer).
7. **Güvenlik ağı:** Explorer'a en çok bir geri düşüş (döngü koruması), `--shell` ile başlayıp pencere kuramayan Gezik hedefi Explorer'a verir, `restore-explorer.reg` her zaman güncel, exe taşındı uyarısı, geçici klasördeki exe için soru.
8. **macOS varsayılanlık:** gerekirse `~/Applications/Gezik.app` (exe'ye sembolik bağlantı; olmazsa kopya), `public.folder` varsayılan uygulaması ve `NSFileViewer`; Finder değiştirilmez.
9. **Tek örnek:** Windows adlandırılmış boru, Unix soketi; anahtar kullanıcı + yapılandırma klasörü; yeni pencere yeni süreçtir, çok pencereli tek süreç yok.
10. **Günlük:** `system-changes.toml` (yapılandırma klasöründe), değişiklikten önce yazılır; geri alma yalnız değer hâlâ Gezik'in yazdığıysa; günlük yoksa sabit adlarla süpürme.
11. **Panel:** tek "System Integration" katmanı, her özellik bir satır ve paletteki komutları; "Changed outside Gezik" ve `Repair`.
12. **PATH:** Windows'ta `%LOCALAPPDATA%\Gezik\bin\gezik.cmd` + kullanıcı PATH'i + App Paths; exe'nin klasörü PATH'e eklenmez. macOS/Linux'ta `~/.local/bin/gezik`; kabuk profilleri düzenlenmez, eklenecek satır gösterilir. (Seçenek: `~/.zprofile`'a işaretli satır eklemek.)
13. **Yönetici yardımcısı sınırları:** iş listesi komut satırına sığmazsa reddedilir (bölünüp birden çok onay istenmez); çakışmalar yükseltmeden önce sorulur; yönetici silmesi kalıcıdır; geri alma yeni bir onay ister; Windows'ta öznitelik işi yok.
14. **Sonuç kanalı:** Windows'ta rastgele adlı, yalnız-yazılır tek örnekli boru; Unix'te stdout; macOS'ta `osascript … with administrator privileges` (AuthorizationExecuteWithPrivileges kullanımdan kalktığı için) ve kendi tırnaklamamız. Üst süreç sonuçları diskten doğrular.
15. **Yardımcı ne zaman önerilir:** yalnız erişim reddinde sorun penceresindeki `Retry as administrator` ve Bilgi penceresi; hiç kendiliğinden değil.
16. **Çöp:** bütün kutular tek görünümde; macOS'ta özgün yer `.DS_Store`'dan, yoksa geri yüklemede sorulur; Tam Disk Erişimi için bant; Windows'ta boşaltma `SHEmptyRecycleBinW`, diğerlerinde iş motoru; kenar çubuğu simgesi yoklanmaz.
17. **Çöpten çıkarmak** (sürükle/kes) geri yükleme gibi taşır, bilgi dosyası silinir; Ctrl+Z yeniden çöpe atar.
18. **Bulut durumu:** yalnız var olan veriden (Windows öznitelikleri, macOS `SF_DATALESS`), ek G/Ç yok; "eşitleniyor/hata" durumu ve ayrı sütun yok; simgenin köşesinde işaret.
19. **Bulut komutları:** Windows `Always keep on this device` / `Free up space` (`CfSetPinState`), macOS `Download Now` / `Remove Download`; Linux'ta yalnız kenar çubuğu.
20. **Çıkarma:** Windows Explorer'ın `eject` fiili (Gezik'in kendi CM_ kodu yok); Linux `gio`, yoksa `udisksctl`; macOS `NSWorkspace`.
21. **Sunucuya bağlan:** yalnız SMB; son 10 adres `state.toml`'da, parola hiç; Windows'ta isteğe bağlı sürücü harfi ve `\\sunucu` paylaşım listesi; Linux'ta parola `gio`'ya yalnız stdin ile.
22. **Sanal bırakma:** hep kopya; Windows'ta veri nesnesi işçi iş parçacığına sıralanır; macOS'ta önce `$TMPDIR` ara klasörü, sonra iş motoru taşır; URL ve metin bırakmaları dosyaya dönüşmez (`.url`/`.webloc` yok).
23. **Genel kısayol önerileri:** Win+Shift+E, ⌥⌘E, Super+Shift+E (ayarda boş = kapalı); öndeyse küçültür/gizler, değilse gösterir.
24. **Linux tepsi:** menüsüz StatusNotifierItem (tıklama göster/gizle); eklentisiz GNOME'da tepsi yok. (Seçenek: `com.canonical.dbusmenu` ile tam menü, ~+400 satır.)
25. **Girişte başlama:** Windows `Run`, macOS LaunchAgent plist (`SMAppService` paket ister), Linux `autostart`; tepsi açıksa gizli başlar, değilse pencere açar.
26. **macOS Quick Actions/Services:** yoklama başarılı olursa 9a2'de; olmazsa 9a2'den çıkar ve PR'da yazılır (Open With ve Share yine gelir).
27. **Sistem Quick Look paneli:** macOS'ta varsayılan (`[system] quick-look = "system"`); yoklama başarısızsa varsayılan `"gezik"`.
28. **Linux D-Bus:** kendi küçük kodlayıcımız (`zbus` yok; ~1–1,5 MB ve async çalışma zamanı getirirdi). (Seçenek: `zbus`.)
29. **Linux simge ve küçük resim:** yalnız PNG tema simgeleri; hazır küçük resim önbelleği okunur, thumbnailer çalıştırılmaz; SVG çizici yok.
30. **Linux "Birlikte aç" ile değiştirilen varsayılan uygulama** kullanıcının tercihidir, günlüğe girmez ve `--unregister` geri almaz (macOS'taki `Change All…` için de aynı).
31. **Ayar adları:** `[system] single-instance`, `tray`, `hotkey`, `start-at-login`, `quick-look`; `[sidebar] cloud`. Varsayılanlık ve PATH ayar değil, günlük durumu.
32. **Menü kimlikleri:** 9a 1700–1799, 9b 1800–1999.
33. **Bütçe:** her parça ≤ +0,25 MiB; varsayılan ayarlarla boşta ≤ +0,1 MB, açılış ≤ +2 ms; zamanlayıcı ve yoklama yok.
