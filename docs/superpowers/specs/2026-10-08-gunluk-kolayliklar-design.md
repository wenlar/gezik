# Alt Proje 7: Günlük Kolaylıklar — Tasarım

- **Tarih:** 2026-10-08
- **Durum:** Tasarım onaylandı (2026-10-07); 7a uygulandı
- **Kapsam:** Gezik yol haritasının 7. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md` §2: maddeler 7, 8, 11, 15, 16, 19, 42, 46, 47, 48, 55); tek spec, üç plan ve üç PR: **7a** terminal, yolu kopyala, oturum ve sekme setleri; **7b** sabitlenen klasör grupları ve görünüm seçenekleri; **7c** şablonlar, panodan dosya, bağlantılar, bırakma yığını ve işlem günlüğü
- **Dayandığı:** `2026-10-04-gezinme-design.md` (sekmeler, kenar çubuğu, sağ tık menüsü, açılış), `2026-10-04-gorunum-design.md` (liste modeli, sıralama, `[view]`), `2026-10-04-dosya-islemleri-design.md` (iş motoru, geri alma, pano, sürükle-bırak), `2026-10-03-ayarlar-ve-tema-design.md` (ayar dosyası, `{home}` belirteçleri, kısayol biçimi), `2026-10-07-klavye-paketi-design.md` (eylem listesi, AltGr kuralı, kilitli sekmeler, `state.toml` yazıcısı)

## 1. Amaç

Rakiplerin çoğunda olan, her gün birkaç kez gereken ama Gezik'te ya hiç olmayan ya da yarım kalan küçük işleri tamamlamak: klasörde terminal açmak, yolu istenen biçimde kopyalamak, kapatıp açınca sekmelerin geri gelmesi, sık kullanılan klasör takımlarını kaydetmek, sabitlenen klasörleri gruplamak, görünümü alışılan biçime getirmek, şablondan dosya oluşturmak, panodaki resmi ya da metni dosyaya çevirmek, kısayol ve bağlantı oluşturmak, dağınık dosyaları bir yerde toplayıp taşımak ve biten işlerin dökümünü görmek. Hepsi düşük emekli, tek tek küçük ama birlikte günlük kullanımı belirgin kolaylaştıran işlerdir. `pinned` ve oturum biçimi burada kesinleşir; Taşınabilirlik adımı bu biçimlerin üzerine kurulur.

### Başarı ölçütleri

- "Open terminal" üç sistemde de doğru klasörde bir terminal açar; terminal bulunamazsa durum çubuğu ne yapılacağını söyler, hiçbir şey sessizce başarısız olmaz.
- "Copy path as ▸" biçimlerinin her biri cmd, PowerShell, bash/zsh ve tarayıcı adres çubuğuna yapıştırıldığında aynı öğeyi gösterir (boşluklu, Türkçe karakterli ve `'` içeren adlar dahil).
- 10 sekme (2'si kilitli) açıkken çıkıp yeniden açınca sekmeler aynı sırada, aynı etkin sekme ve aynı kilitlerle gelir; açılış süresi değişmez (≤ ~60 ms), yalnız etkin sekme okunur.
- Eski biçimdeki `pinned = ["…"]` dosyaları uyarısız okunur; takma ad ve grup eklenmemiş sabitlemeler dosyada bugünkü gibi yazılı kalır.
- Görünüm seçenekleri ayar dosyası kaydedilince ve menüden değiştirilince canlı uygulanır; 100.000 dosyalık klasörde kaydırma ve sıralama süreleri değişmez.
- Dosya değiştiren her yeni iş (şablondan oluşturma, seçimle yeni klasör, panodan dosya, bağlantı, yığından kopyala/taşı) iş motorundan geçer, işlem panelinde görünür ve tek Ctrl+Z ile geri alınır.
- Exe büyümesi her parçada ≤ +0,25 MiB, toplamda ≤ +0,75 MiB (`master` `38c65b3`'e göre; plan 7a ilk adımda ölçer). Boşta bellek `master`'a göre büyümez (bugün 7,2–7,3 MB).
- Her yeni eylem `[shortcuts]` ile değiştirilebilir, macOS menü çubuğunda görünür ve "her varsayılan Windows ve Linux'ta ulaşılabilir" testine girer.

### Kapsam dışı (bilerek)

- macOS takma ad (alias) dosyaları oluşturmak; Windows `.url` internet kısayolları.
- Sekme setlerinin makineler arasında eşitlenmesi (`settings.toml` taşınabilir, ama eşitleme Taşınabilirlik adımının işi).
- Kalıcı işlem günlüğü (diske yazılan log).
- Sabit bağlantı (hardlink) oluşturma (rakip özeti #15'te var; onaylanan tasarımda yok).
- Sabitlenen klasörlere simge ve renk (#19'un bir parçası), listede ve kenar çubuğunda yaylı klasörler (#56).
- Gezik içinde gömülü terminal paneli.
- İç içe (iki düzey) alt menüler: bu adımda menüler aynı düzeyde birden çok alt menü taşır (bkz. §3.5), iç içe olan yine sonraya kalır.
- Kapatılan sekme yığınının kalıcı olması (oturumla gelmez; pencere kapanınca gider, 6a'daki gibi).
- Birden çok Gezik penceresinin oturumlarının ayrı ayrı tutulması: her süreç kendi penceresidir, en son kapanan pencerenin oturumu kalır.
- Görünüm seçeneklerinin klasör başına hatırlanması: yeni seçenekler (`hide-extensions`, `folders-first`, tarih/boyut biçimi, gizli/sistem) her klasörde aynıdır; `views.toml` değişmez.

### Önceki spec'lerle ilişki

- **Gezinme spec'i §1 "Kapsam dışı":** "Açılışta önceki sekmelerin geri yüklenmesi (kullanıcı tek sekme + ayarlanabilir başlangıç klasörü seçti)" maddesi bu adımla kaldırılır. `start-folder` yeni sekmeler ve `[session] restore = false` için geçerli kalır; Gezinme spec'inin §2'deki "Açılış: Tek sekme" kararı da `restore = false` iken geçerlidir. (Bu spec yalnız kendini yazar; eski spec'teki madde değiştirilmez, buradaki not geçerlidir.)
- **Klavye paketi spec'i §1:** "Kilitli sekmelerin ve kapatılan sekme listesinin kalıcı olması" maddesinin kilitli sekmeler kısmı kaldırılır (kilit oturumla gelir); kapatılan sekme listesi kapsam dışında kalır.

## 2. Alınan kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Bölme | 7a terminal + yolu kopyala + oturum + sekme setleri; 7b sabitlenen grupları + görünüm seçenekleri; 7c şablonlar + panodan dosya + bağlantılar + yığın + günlük | Her PR kendi başına kullanılabilir; 7c iş motoruna dokunan parçaları toplar |
| Terminal | Windows: Windows Terminal (`wt`), yoksa PowerShell; yönetici olarak açma UAC ile; macOS: Terminal.app; Linux: `$TERMINAL`, `x-terminal-emulator`, bilinen terminaller; `[terminal] command` hepsini geçersiz kılar | Kullanıcı tercihi |
| Oturum | Varsayılan açık (`[session] restore = true`); `state.toml`'da (makineye özgü) | Klasörler makineye özgüdür; `settings.toml` taşınabilir kalır |
| Sekme setleri | `settings.toml`'da `[[tab-sets]]`, yollar `{home}` belirteçleriyle | Taşınabilir; kayıtlı süzgeçler ve kural setleriyle aynı yazma yolu |
| Sabitlenenler | Aynı `pinned` listesi; öğe metin (bugünkü) ya da `{ path, name, group }` satır içi tablo | Geriye uyum; eski sürümler grupsuz sabitlemeleri okumaya devam eder (§5.1) |
| Klasör kısayolları | Alt+1…9 (macOS ⌘⌥1…9) görünen ilk 9 sabitlemeye | Ctrl+Alt+rakam Windows'ta AltGr ile karakter yazar (§10.3) |
| Görünüm | Hepsi `[view]`'da, menüden de değişir; varsayılanlar bugünkü görünümü korur | Güncelleyen kullanıcı hiçbir farkı görmez |
| Bağlantılar | Windows: `.lnk`, klasöre junction, izin varsa symlink; macOS/Linux: symlink | Her sistemin olağan bağlantısı |
| Geri alma | Yeni işlerin hepsi iş motorunda, var olan `Outcome`'larla | Ayrı bir geri alma yolu yok |
| Yığın ve günlük | Yalnız oturum boyunca, bellekte | Hafiflik; kalıcı günlük kapsam dışı |

## 3. 7a — Terminal

### 3.1 Nerede açılır

- **Eylem:** `open-terminal`. Odaktaki satır bir klasörse o klasörde, bir dosyaysa bulunduğu klasörde açılır; odakta satır yoksa gösterilen klasörde. "This PC"de odaktaki sürücünün kökünde; sürücü odakta değilse bir şey yapmaz.
- **Menüler:** satır menüsünde (klasör: o klasör; dosya: klasörü; çoklu seçimde gösterilen klasör), klasör boşluğu menüsünde (gösterilen klasör) ve kenar çubuğu girdisinin menüsünde "Open terminal here". Windows'ta altında "Open terminal as administrator".
- `open-terminal-admin` eylemi (bağlamasız) yalnız Windows'ta çalışır; başka sistemde durum çubuğu "Only on Windows" der. Ad her sistemde geçerlidir (taşınabilir ayar dosyası).

### 3.2 Terminal seçimi

Seçim saf bir işlevdir (`gezik-platform::terminal::choose`): giriş olarak ortam değişkenleri, PATH'te bulunan programlar ve masaüstü adı alır, çıktı olarak program, argümanlar ve çalışma klasörü verir. Böylece her kural birim testiyle denenir.

- **`[terminal] command` varsa** her şeyin önüne geçer: liste, ilk öğe program. `{dir}` klasörle değiştirilir (`{{` `}}` düz süslü parantez, `[[commands]]`'daki gibi); `{dir}` olmasa da çalışma klasörü o klasördür. Kabuk kullanılmaz.
- **Windows:** PATH'te `wt.exe` varsa (Microsoft Store'un uygulama takma adı `%LOCALAPPDATA%\Microsoft\WindowsApps` PATH'tedir) `wt -d <klasör>`. `wt` `;` karakterini komut ayırıcı sayar: klasör adındaki `;` `\;` olarak geçirilir. Yoksa `pwsh.exe` (PowerShell 7) varsa o, yoksa `powershell.exe`; çalışma klasörü verilerek yeni konsolda (`CREATE_NEW_CONSOLE`).
- **Windows, yönetici olarak:** `ShellExecuteExW` ile `runas` fiili, aynı program. Yükseltilmiş süreç çalışma klasörünü her zaman almadığından klasör argümanla verilir: `wt -d <klasör>`, PowerShell için `-NoExit -Command Set-Location -LiteralPath '<klasör>'` (`'` iki kez yazılır). Yükseltilmiş oturum kullanıcının bağladığı ağ sürücülerini görmez: klasör bağlı bir sürücüdeyse UNC yoluna çevrilir (§4.2'deki işlevle). UAC'de "No" demek (`ERROR_CANCELLED`) sessizce bir şey yapmaz.
- **macOS:** `open -a Terminal <klasör>`.
- **Linux**, sırayla, ilk bulunan:
  1. `$TERMINAL` (boşluklarla bölünür, kabuk yok); çalışma klasörüyle başlatılır.
  2. `x-terminal-emulator` (Debian/Ubuntu); çalışma klasörüyle.
  3. Bilinen terminaller; masaüstünün kendi terminali öne alınır (`XDG_CURRENT_DESKTOP` KDE ise `konsole`, XFCE ise `xfce4-terminal`, GNOME ise `ptyxis`, `kgx`, `gnome-terminal`), sonra sırayla `gnome-terminal`, `ptyxis`, `kgx`, `konsole`, `xfce4-terminal`, `kitty`, `alacritty`, `foot`, `xterm`. Her birine kendi klasör bayrağı da verilir (yalnız çalışma klasörüne güvenmek her terminalde tutmaz): `gnome-terminal --working-directory=`, `ptyxis --new-window --working-directory=`, `kgx --working-directory=`, `konsole --workdir`, `xfce4-terminal --working-directory=`, `kitty --directory`, `alacritty --working-directory`, `foot --working-directory=`; `xterm` yalnız çalışma klasörüyle.
- Süreç Gezik'ten bağımsız başlatılır (Gezik kapanınca terminal açık kalır; Unix'te yeni süreç grubu, Windows'ta iş nesnesine bağlanmaz). Gezik'in iş süreçleri için kullanılan `ChildProcess` (kapanınca öldürülen ağaç) kullanılmaz.
- **Bulunamazsa** durum çubuğu: `No terminal found. Set [terminal] command in settings.toml.` Başlatma hatası: `Cannot open the terminal: <neden>`.

## 4. 7a — Yolu kopyala

### 4.1 Davranış

- **Eylem:** `copy-path` seçili öğelerin tam yollarını kopyalar; seçim yoksa gösterilen klasörün yolunu. "This PC"de sürücü köklerini. Durum çubuğu: `Copied the path` / `Copied 3 paths`.
- **Menü:** satır menüsünde ve kenar çubuğu girdisinde "Copy path as ▸" alt menüsü; klasör boşluğu menüsünde gösterilen klasör için aynı alt menü. Öğeler (UI metni):
  - `Full path` — `C:\Users\a\rapor.pdf`
  - `Quoted` — Windows'ta `"C:\Users\a\rapor.pdf"` (cmd, PowerShell ve Explorer'ın "Copy as path"i gibi); macOS/Linux'ta POSIX tek tırnağı: `'/home/a/it'\''s.pdf'` (kabuğa doğrudan yapıştırılır).
  - `With forward slashes` — `C:/Users/a/rapor.pdf`; yalnız Windows'ta görünür (Unix'te tam yolla aynı).
  - `Name` — `rapor.pdf`
  - `Folder path` — `C:\Users\a`
  - `file:// URL` — `file:///C:/Users/a/rapor%20%C3%A7.pdf`, RFC 8089; UNC yolu `file://sunucu/pay/…`. Yüzde kodlaması UTF-8 baytları üzerinde; `A–Z a–z 0–9 - . _ ~ /` ve sürücü harfinden sonraki `:` kodlanmaz.
  - `UNC path` — yalnız Windows'ta ve seçimin ilk öğesi bağlı bir ağ sürücüsündeyse görünür: `Z:\proje\a.txt` → `\\sunucu\pay\proje\a.txt`. Bağlı olmayan öğeler tam yollarıyla yazılır.
- **Birden çok öğe** ayrı satırlarda; satır sonu Windows'ta `\r\n`, başka yerde `\n`; sonda satır sonu yok.

### 4.2 Biçimleme ve pano

- Biçimler saf işlevlerdir (`gezik-core::path_text`): `format(paths, kind, platform)`; UNC dönüşümü bir kapanış olarak verilir. Birim testli.
- UNC: `WNetGetConnectionW` ile sürücü harfinin uzak adı (`windows` crate'ine `Win32_NetworkManagement_WNet` özelliği eklenir; ek kod boyutu yok denecek kadar az).
- **Pano:** `gezik-platform::clipboard` bugün yalnız dosya yazıp okur. `write_text(&str)` eklenir: Windows'ta `CF_UNICODETEXT`; macOS'ta `NSPasteboard` dizgisi (`NSPasteboardTypeString`); Linux'ta X11 ve Wayland arka uçları dosya hedeflerinin yanında metin hedeflerini (`UTF8_STRING`, `text/plain;charset=utf-8`, `TEXT`, `STRING`) sunar. Metin kopyalanınca Gezik'in kendi dosya panosu (`Operations.clip`, kesilenlerin soluk görünmesi) değişmez; sistem panosunun sıra numarası değiştiği için bir sonraki denetimde kesilenler temizlenir (bugünkü `clipboard_check`).

## 5. 7a — Oturum ve sekme setleri

### 5.1 Oturum

- **Açık/kapalı:** `settings.toml` `[session] restore = true` (varsayılan). `false` iken açılış bugünkü gibidir (tek sekme, `start-folder`) ve `state.toml`'daki oturum silinir.
- **Ne saklanır** (`state.toml`, makineye özgü):

  ```toml
  [session]
  active = 2              # 0'dan sayılır

  [[session.tabs]]
  path = "D:/Work/gezik"
  locked = true

  [[session.tabs]]
  drives = true           # This PC
  ```

  Yollar mutlak ve olduğu gibi yazılır (belirteç yok; dosya makineye özgü). Geri/ileri geçmişi, süzgeç metni ve kapatılan sekmeler saklanmaz.
- **Ne zaman yazılır:** sekme açılınca, kapanınca, taşınınca, kilitlenince, etkin sekme ya da bir sekmenin klasörü değişince bellekteki durum güncellenir; `state.toml` yazıcı iş parçacığı (6b'deki `StateCell`) bir patlamayı tek yazmada toplar. Çıkışta `save_and_quit` zaten `flush_state` yapar. Böylece çökme ya da zorla kapatma da son oturumu bırakır; arayüz iş parçacığı dosyaya dokunmaz. 6b'nin klasör geçmişi her ziyarette zaten durum yazdığından ek bir disk yükü oluşmaz.
- **Açılışta:** kayıtlı sekmeler sırayla açılır, `active` etkinleşir (aralık dışıysa ilk sekme), kilitler uygulanır. Yalnız etkin sekme okunur (Gezinme spec'indeki "yalnız aktif sekmenin listesi bellekte"); diğerleri gidilince okunur. Açılışta varlık denetimi yapılmaz (ağ yolu açılışı bekletmesin): klasör yoksa bugünkü `LoadResult::Gone` yolu en yakın var olan üst klasöre gider ve durum çubuğu "… no longer exists" der; bu geri düşüş ziyaret sayılmaz (`8629007`).
- **Komut satırı:** verilen klasör, kayıtlı sekmelerin sonuna yeni sekme olarak eklenir ve etkinleşir; dosya verilirse klasörü açılır, dosya seçilir (bugünkü `plan_start`). Verilen yol yoksa uyarı yazılır, oturum yine gelir. Oturum boşsa (ilk açılış, bozuk dosya) bugünkü davranış.
- Bozuk ya da tanınmayan girdiler sessizce atlanır (dosyayı uygulama yazar; `State::parse` kuralı).

### 5.2 Sekme setleri

```toml
[[tab-sets]]
name = "Gezik release"
tabs = ["D:/Work/gezik", "{downloads}", "drives"]
```

- `tabs` belirteçli yollardır (`KnownDirs::collapse`, sabitlenenler gibi); `"drives"` This PC'dir. `..` içeren yol ve boş `name` uyarıyla atlanır; ad büyük/küçük harf ayrımı olmadan tekildir (kayıtlı süzgeçlerin kuralı).
- **Save tabs as…:** sekmenin sağ tık menüsünde ve macOS Window menüsünde; ad sorulur (`ask_text`), aynı ad varsa "Replace?" sorulur. `save-tab-set` eylemi (bağlamasız) aynı pencereyi açar.
- **Open tab set ▸** (aynı yerlerde): her set için bir öğe (setin sekmelerini yeni sekmeler olarak sona ekler, ilki etkinleşir); ayraçtan sonra her set için `Replace tabs with "Ad"` (kilitli olmayan sekmeleri kapatıp seti açar; kilitliler kalır, durum çubuğu kaçının kaldığını söyler); sonra her set için `Delete "Ad"`. Süzgeç menüsündeki düzenle aynıdır; en çok 30 set listelenir.
- Bu makinede olmayan klasör açılınca §5.1'deki gibi en yakın üst klasöre düşer.
- Yazma `settings_writer`'dan geçer (`SettingsChange::TabSets`), `with_tables` ile; elle yazılmış ve okunamayan girdiler olduğu gibi korunur.

## 6. 7b — Sabitlenen klasör grupları

### 6.1 Biçim ve geçiş

Bugün `settings.toml`'da en üst düzeyde `pinned = ["{documents}/Projects", "D:/Work"]` vardır (`Settings.pinned: Vec<String>`, `settings_edit::with_pinned`). Yeni biçim aynı anahtar, aynı yer:

```toml
pinned = [
  "{documents}/Projects",
  { path = "D:/Work/gezik", name = "Gezik", group = "Work" },
  { path = "//nas/foto", group = "Media" },
]
```

- Öğe ya metindir (yol) ya da satır içi tablo: `path` (zorunlu), `name` (takma ad, isteğe bağlı), `group` (isteğe bağlı). Tanınmayan anahtar ve boş `path` uyarıyla atlanır; yinelenen yol (Windows'ta büyük/küçük harf ayrımsız) uyarıyla atlanır.
- **Geçiş yok:** eski dosyalar aynen okunur. Gezik yazarken takma adı ve grubu olmayan sabitlemeyi metin olarak, olanı tablo olarak yazar. Takma ad ya da grup hiç kullanılmadıysa dosya bugünkü gibi kalır.
- **Neden `[[pinned]]` değil:** TOML'da `pinned = [...]` ile `[[pinned]]` aynı dosyada bulunamaz; tablo dizisine geçmek listeyi dosyanın sonuna taşır ve eski Gezik sürümlerinin aynı (eşitlenen) dosyayı açınca bütün sabitlemeleri düşürmesine yol açar. Satır içi tablolarda eski sürüm yalnız takma adlı/gruplu öğeleri uyarıyla atlar.
- `Settings.pinned` `Vec<PinEntry { path: String, name: Option<String>, group: Option<String> }>` olur; `with_pinned` metin ya da satır içi tablo yazar. Dizinin içindeki yorumlar bugünkü gibi korunmaz (şablondaki not aynen kalır).

### 6.2 Gruplar

- Kenar çubuğunda önce grupsuz sabitlemeler "PINNED" başlığı altında, sonra her grup kendi başlığıyla (yazıldığı gibi, büyük harfe çevrilmeden), grupların sırası ilk sabitlemelerinin listedeki sırasıdır. Gezik yazarken bir grubun öğelerini listede yan yana tutar.
- Bir grup yalnız içinde sabitleme varken vardır; boşalan grup kaybolur.
- **Grup başlığının menüsü:** `Move group up`, `Move group down`, `Rename group…`, `Ungroup` (öğeler grupsuz olur).
- **Sürükleme:** sabitlemeler gruplar arasında sürüklenir; bırakılan yer (`Hit::PinAt`) grubu da belirler. Grup başlığı sürüklenmez (sırası menüden değişir).
- Var olmayan sabitlemeler bugünkü gibi gizlenir (arka planda denetim); grup başlığı yalnız görünen öğesi varsa çizilir.

### 6.3 Takma ad ve grup düzenleme

Sabitlenen girdinin menüsüne eklenir:

- `Rename…` — takma ad sorulur; boş bırakmak klasör adına döner. Takma adlı girdinin ipucu (tooltip) tam yolu gösterir.
- `Move to group ▸` — var olan gruplar, `New group…` (ad sorar), `No group`.
- Bugünkü `Move up` / `Move down` grup içinde çalışır.

### 6.4 Numaralı klasör kısayolları

- `pin-1` … `pin-9`: görünen sabitlemelerin kenar çubuğundaki sırasıyla ilk dokuzu (gruplar dahil, yukarıdan aşağı). Etkin sekmede o klasöre gider. Olmayan numara bir şey yapmaz.
- Varsayılan Windows ve Linux'ta Alt+1…9, macOS'ta ⌘⌥1…9 (gerekçe §10.3). Kenar çubuğunda ilk dokuz girdinin sağında soluk "Alt+1" ipucu yok; numara girdinin ipucunda yazılır.

## 7. 7b — Görünüm seçenekleri

### 7.1 Ayarlar

`[view]`'a eklenir (bugünkü `mode`, `sort`, `sort-dir`, `grid-size`, `icons`, `thumbnails` aynen kalır):

| Ayar | Değerler | Varsayılan | Ne yapar |
|---|---|---|---|
| `hide-extensions` | bool | `false` | Dosya adlarını uzantısız çizer (klasörlerde ve `.gitignore` gibi yalnız noktayla başlayan adlarda bir şey değişmez) |
| `folders-first` | bool | `true` | Klasörler her sıralamada önce (bugün her zaman böyle); `false` iken klasörler dosyalarla karışık sıralanır |
| `date-format` | `relative`, `short`, `iso`, `system` | `system` | Değiştirilme/oluşturulma sütunu ve önizleme |
| `size-format` | `binary`, `decimal` | `binary` | Boyut sütunu, seçim özeti, önizleme, işlem paneli |
| `single-click-open` | bool | `false` | Tek tıkla açma |
| `show-hidden` | bool | Windows/Linux `true`, macOS `false` | Gizli öğeleri gösterir |
| `show-system` | bool | `false` | Yalnız Windows: korunan sistem öğelerini gösterir |

- **Tarih biçimleri:** `system` bugünkü çıktıdır (Windows'ta sistemin kısa tarihi ve saati, başka yerde `YYYY-MM-DD HH:MM`); `short` yalnız tarih (Windows'ta sistemin kısa tarihi, başka yerde `YYYY-MM-DD`); `iso` her yerde `YYYY-MM-DD HH:MM`; `relative` son bir saat için `5 min ago`, bugün için `Today 14:05`, dün için `Yesterday 14:05`, daha eskisi için `system`. `relative` iken görünen satırlar dakikada bir yeniden biçimlenir (yalnız görünenler).
- **Boyut biçimleri:** `binary` bugünkü çıktıdır: 1024'lük adımlar, `KB`/`MB`/`GB` etiketleri (Explorer gibi). `decimal` 1000'lik adımlar, `kB`/`MB`/`GB` (Finder ve GNOME gibi). `gezik_core::format_size` biçimi parametre alır; ayarı okuyan tek yer arayüzdür.
- **Uzantı gizleme** yalnız çizimdir: harfle atlama, süzgeç, sıralama ve yeniden adlandırma gerçek adla çalışır. Yeniden adlandırma alanı tam adı gösterir (uzantı hariç kısım bugünkü gibi seçili gelir); böylece uzantı kazara değişmez ve gizliyken de görülür.
- **Tek tıkla açma:** tek tık klasörü ya da dosyayı açar; Ctrl/⌘ ve Shift ile tıklama seçer, sağ tık seçip menüyü açar, basılı tutup sürüklemek bugünkü gibi sürükler (eşiği geçmeyen basış tıklamadır). Klavye değişmez.
- **Gizli ve sistem öğeleri:** bugün gizli öğe yalnız noktayla başlayan addır, ayar dosyasında değil bellekte tutulur (`View.show_hidden`, Windows/Linux'ta açık, macOS'ta kapalı başlar) ve Windows'un gizli özniteliğine bakılmaz. Yeni kural:
  - `show-hidden = false` iken noktayla başlayan adlar ve (Windows'ta) gizli öznitelikli öğeler gizlenir.
  - Windows'ta hem gizli hem sistem öznitelikli öğeler ("korunan işletim sistemi dosyaları": `desktop.ini`, `$RECYCLE.BIN`, `System Volume Information`) `show-system = false` iken her durumda gizlenir. Bu Windows'ta görünür bir değişikliktir: bu öğeler bugün listede görünüyor; Explorer'ın varsayılanına uyulur.
  - Öznitelikler `list_dir`'in zaten okuduğu meta veriden alınır (`MetadataExt::file_attributes`, ek sistem çağrısı yok) ve `Entry`'ye bir bayrak baytı olarak eklenir; `Entry`'nin boyutu değişmez (`is_dir`'in yanındaki dolgu), plan bunu `size_of` testiyle doğrular.

### 7.2 Menüden değiştirme

- Windows ve Linux'ta menü çubuğu yoktur; "View menu" araç çubuğundaki View düğmesinin menüsüdür (`context_menu::view_items`). macOS'ta menü çubuğundaki View menüsü. İkisine de işaretli öğeler eklenir: `Hide extensions`, `Folders first`, `Single-click to open`, `Show hidden items`, Windows'ta `Show system items`, ve tek düzey alt menüler `Date format ▸`, `Size format ▸`.
- Menüden değiştirmek `settings.toml`'a yazar (`SettingsChange::ViewOption`, `with_view_defaults` gibi değerin yorumunu koruyarak); dosya izleyicisi yeniden yükleyince her yere uygulanır. `toggle-hidden` (Ctrl+H, macOS ⌘⇧.) artık `show-hidden`'ı yazar, yani gizli öğe ayarı yeniden açılışta korunur.

## 8. 7c — Yeni dosya şablonları ve seçimle yeni klasör

### 8.1 Şablonlar

- **Yerleşikler** (kodda; dosya olarak yazılmaz): `Folder` (bugünkü `NewTask::folder`, "New folder"), `Text file` (bugünkü `NewTask::file`, "New file.txt"), `Markdown file` ("New document.md", boş).
- **Kullanıcınınkiler:** yapılandırma klasöründeki `templates/` klasörünün öğeleri (dosya ya da klasör); menüde adları uzantısız ve alfabetik, yerleşiklerden sonra bir ayraçla. Noktayla başlayan ve Windows'ta gizli öğeler atlanır; en çok 50 öğe. Klasör şablonu içiyle birlikte kopyalanır (hazır klasör yapıları).
- `templates/` ilk çalıştırmada temalar klasörüyle birlikte oluşturulur (`ensure_initialized`; kullanıcının sildiği şey yeniden oluşturulmaz kuralı korunur). Menünün sonunda `Open templates folder` (yoksa oluşturur).
- **Menü:** macOS ve Linux'ta klasör boşluğu menüsündeki bugünkü `New folder` ve `New file` öğeleri tek bir `New ▸` alt menüsü olur. Windows'ta Explorer menüsünün kendi `New ▸` alt menüsü zaten vardır; iki aynı adlı alt menü olmasın diye Gezik'inki `New from template ▸` adını taşır ve bugünkü `New folder` / `New file` öğeleri yerinde kalır.
- **Çalışma:** şablon kopyası bir `CopyTask` (yeni kurucu: tek kaynak, hedef adı şablonun adı, karar önceden `KeepBoth`, yani ad alınmışsa `Ad (2).md`) ya da yerleşikler için `NewTask` (yeni `Markdown` türü). Bitince oluşan öğe seçilir ve yeniden adlandırma başlar (`After::Rename`). Geri alma: `Outcome::Created` → çöpe; etiket "New file" / "New folder".

### 8.2 Seçimle yeni klasör

- `new-folder-with-selection` (Windows/Linux Ctrl+Alt+N, macOS ⌃⌘N) ve satır menüsünde birden çok öğe seçiliyken `New folder with selection`.
- Seçili öğeler aynı klasörde yeni bir "New folder"a taşınır (ad alınmışsa `New folder (2)`), bitince klasör seçilir ve yeniden adlandırma başlar.
- **Görev:** `gezik-ops`'ta yeni `GroupTask`: planda klasör (KeepBoth) ve her öğenin taşınması; çıktılar `Created { klasör }` ve her öğe için `Moved`. Tek iş, tek geri alma ("New folder with 3 items").
- **Geri alma sırası (motor değişikliği):** `inverse::build` bugün önce yapılanları çöpe atar, sonra taşınanları geri taşır. Burada öğeler yapılan klasörün içinde olduğundan bu sıra öğeleri klasörle birlikte çöpe atar ve geri taşıma başarısız olur. Kural: bir taşınanın şimdiki yeri yapılan bir öğenin içindeyse, geri taşımalar çöpe atmadan önce çalışır; klasör o zaman boştur ve çöpe gider. Diğer işlerin sırası değişmez; kural birim testiyle sabitlenir.

## 9. 7c — Panodan dosya, bağlantılar, bırakma yığını, işlem günlüğü

### 9.1 Panodaki resmi ya da metni dosya olarak yapıştırma

- Ctrl+V (⌘V) liste odaktayken ve panoda dosya yokken: panoda resim varsa `Pasted image 2026-10-08 14.05.09.png`, yoksa metin varsa `Pasted text 2026-10-08 14.05.09.txt` oluşturur. Saat `:` yerine `.` ile yazılır (Windows adlarda `:` kabul etmez); yerel saat. Ad alınmışsa `(2)`.
- Resim metinden önce gelir (tarayıcıdan kopyalanan resim çoğu zaman ikisini birden taşır). Metin UTF-8, BOM'suz, panodaki satır sonlarıyla yazılır.
- Klasör boşluğu menüsünde `Paste` öğesi panoya göre `Paste image as file` / `Paste text as file` olur; Windows'ta Explorer menüsünün "Paste"i yalnız dosyada çıktığı için Gezik bunu kendi öğesi olarak üste ekler.
- **Pano okuma** (`gezik-platform::clipboard`, yeni): `read_image() -> Option<ClipboardImage>` (PNG baytları ya da RGBA) ve `read_text()`. Windows: kayıtlı `PNG` biçimi varsa baytları doğrudan, yoksa `CF_DIBV5`/`CF_DIB` (BMP başlığı eklenip `image` crate'inin zaten derlenen `bmp` çözücüsüyle); metin `CF_UNICODETEXT`. macOS: `public.png`, yoksa `NSBitmapImageRep` ile TIFF'ten PNG; metin `NSPasteboardTypeString`. Linux: X11 ve Wayland arka uçları `image/png` ve metin hedeflerini okur. Yeni bağımlılık yok (`image` `png`+`bmp` zaten `gezik-platform`'da).
- Menü açılırken ve Ctrl+V'de yalnız biçimin var olup olmadığına bakılır (veri okunmaz). Veri yapıştırmada arayüz iş parçacığında okunur; PNG kodlama ve yazma `NewTask`'ın yeni "içerikli dosya" türünde, iş parçacığında. Geri alma: `Outcome::Created` → çöpe.

### 9.2 Bağlantı oluşturma

- **Menü:** satır menüsünde. Windows'ta `Create link ▸`: `Shortcut` (`.lnk`, Explorer'ın yaptığı gibi `IShellLinkW` + `IPersistFile`; ad `Ad - Shortcut.lnk`), `Junction` (yalnız klasörde ve yerel NTFS birimindeyken; `FSCTL_SET_REPARSE_POINT`), `Symbolic link` (yalnız izin varken). macOS ve Linux'ta yalnız sembolik bağlantı olduğundan tek öğe: `Create link` (tek öğelik alt menü olmasın diye). Sembolik bağlantı ve junction adı `Link to Ad` (Nautilus gibi; uzantı adın sonunda kalır).
- **İzin (Windows):** sembolik bağlantı Geliştirici Modu ya da yönetici ister. Açılışta arka planda bir kez denenir (geçici klasörde `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` ile bir bağlantı oluşturup silmek); sonuç süreç boyunca saklanır, menü öğesi buna göre görünür.
- **Explorer'ın "Create shortcut"u:** Windows'ta Explorer menüsünün `link` fiili, bugün `Cut`/`Copy`/`Paste`/`Delete`/`Rename` gibi Gezik'e alınır (`ShellVerb::Link`): Gezik `.lnk`'yi kendisi oluşturur, böylece geri alınır.
- **Bırakarak:** bağlantı tuşuyla bırakmak hedef klasörde bağlantı oluşturur (Windows'ta `.lnk`, başka yerde sembolik bağlantı). Bugün Shift taşır, kopya tuşu (Ctrl; macOS'ta ⌥) kopyalar, ikisi birden varsayılanı seçer. Yeni kural: Windows'ta Alt ya da Ctrl+Shift (Explorer gibi); Linux'ta Ctrl+Shift (GTK/KDE; birçok pencere yöneticisi Alt-sürüklemeyi pencere taşımaya ayırır); macOS'ta ⌘⌥ (Finder gibi; ⌥ zaten kopyadır). İmleç bağlantı simgesini gösterir; dışarıdan gelen bırakmalarda da (Windows OLE `DROPEFFECT_LINK`, macOS, XDND/Wayland) aynı tuşlar geçerlidir. Sağ tuşla bırakma menüsüne `Create link here` eklenir. `gezik-core::drag`'a `Effect::Link` ve tuş kuralı eklenir.
- **Görev:** `gezik-ops`'ta `LinkTask` (tür `TaskKind::Link`, etiket "Create link"); her bağlantı için `Outcome::Created { path, facts }`. `facts` `symlink_metadata`'dan gelir (bağlantının kendisi; `walk.rs`'teki gibi bağlantı ne klasör ne boyutludur). Geri alma bugünkü yoldan bağlantının kendisini çöpe atar, hedefine dokunmaz; yineleme çöpten geri getirir.
- **Platform testi zorunlu:** her sistemde bağlantıyı çöpe atmak hedefi değiştirmemeli (Windows'ta junction ve sembolik bağlantı Geri Dönüşüm Kutusu'na giderken hedef klasörün içi yerinde kalmalı). Test Windows'ta junction için bunu sağlayamazsa plan, yalnız bağlantıyı kaldıran (`RemoveDirectoryW`, yeniden ayrıştırma noktası) ayrı bir `Outcome::Linked` ekler; geri alma o zaman çöpe değil kaldırmaya gider ve yineleme bağlantıyı yeniden oluşturur.

### 9.3 Bırakma yığını (drop stack)

- Pencerenin altında, durum çubuğunun üstünde tek satırlık şerit; `toggle-stack` (bağlamasız) ve View menüsündeki `Drop stack` ile açılıp kapanır, öğe eklenince kendiliğinden açılır.
- **Eklemek:** öğeleri şeride sürüklemek (Gezik içinden ya da başka programdan) ya da `add-to-stack` (Ctrl+Shift+S, macOS ⌘⇧S) seçili öğeleri ekler. Aynı yol iki kez eklenmez.
- **Gösterim:** her öğe simge + ad (ipucu tam yol) ve ×; sağda `Copy here`, `Move here`, `Clear`. Artık olmayan öğe soluk çizilir ve işlemlerde atlanır.
- **Kullanmak:** `Copy here` / `Move here` gösterilen klasöre bugünkü `transfer` ile (tek iş, tek geri alma, çakışma penceresi). Taşınan öğeler yığından çıkar, kopyalananlar kalır. Şeritten sürüklemek (bir öğe ya da hepsi) bugünkü sürükleme gibidir: pencere içinde Gezik'in kendi bırakması, pencereden çıkınca sisteme devredilir (`dnd::drag_out`).
- Yalnız yollar tutulur, yalnız oturum boyunca (diske yazılmaz); en çok 1000 öğe. `gezik-core::drag`'a `Hit::Stack` eklenir.

### 9.4 İşlem günlüğü

- İşlem panelinde iki sekme: `Current` (bugünkü satırlar) ve `History`. Panel boşken de açılabilsin diye durum çubuğunda oturumda bir iş bittikten sonra görünen `History` düğmesi ve `show-history` eylemi (bağlamasız).
- Her biten iş (geri alma ve yineleme dahil) bir kayıt: bitiş saati, başlık, sonuç (`Done`, `Done · 2 skipped`, `3 failed`, `Cancelled`), `Show in folder` (sonuçların ilk öğesinin klasörüne gider ve sonuçları seçer; sonuç yoksa değişen ilk klasöre) ve hata ya da atlama varsa `Details` (bugünkü pencere, en çok 50 satır, `MAX_DETAILS`).
- En yeni üstte; en çok 200 kayıt, dolunca en eski düşer. Yalnız bellekte, diske yazılmaz.

## 10. Ayarlar ve kısayollar

### 10.1 `settings.toml`

```toml
# Folders pinned to the sidebar: a path, or { path, name, group } for an alias and a group.
pinned = ["{documents}/Projects", { path = "D:/Work/gezik", name = "Gezik", group = "Work" }]

[view]
hide-extensions = false   # show names without their extension
folders-first = true      # folders before files in every sort
date-format = "system"    # relative | short | iso | system
size-format = "binary"    # binary (1024, KB) | decimal (1000, kB)
single-click-open = false
show-hidden = true        # macOS: false
show-system = false       # Windows only: protected system items (desktop.ini, $RECYCLE.BIN)

[session]
restore = true            # reopen the last tabs at start (kept in state.toml)

[terminal]
# The terminal "Open terminal" runs; {dir} is the folder. Empty: Gezik finds one.
# command = ["wezterm", "start", "--cwd", "{dir}"]

# [[tab-sets]]
# name = "Release"
# tabs = ["D:/Work/gezik", "{downloads}", "drives"]
```

Hatalı değerler bugünkü kalıpla uyarı verir (`view.date-format: expected "relative", "short", "iso" or "system", got …`, `terminal.command: expected a list of text …`); bilinmeyen yer tutucu (`{foo}`) uyarıyla komutu yok sayar.

### 10.2 `state.toml`

- `[session]` `active` ve `[[session.tabs]]` (`path` ya da `drives = true`, `locked`).

### 10.3 Yeni eylemler ve varsayılan tuşlar

| Eylem | Windows / Linux | macOS | Not |
|---|---|---|---|
| `open-terminal` | Shift+F4, Ctrl+Alt+T | ⌘⌥T | Shift+F4 Dolphin'inki |
| `open-terminal-admin` | — | — | Yalnız Windows'ta çalışır |
| `copy-path` | Ctrl+Shift+C | ⌘⌥C | Explorer'ın ve Finder'ın "Copy as path"i |
| `pin-1` … `pin-9` | Alt+1 … Alt+9 | ⌘⌥1 … ⌘⌥9 | |
| `new-folder-with-selection` | Ctrl+Alt+N | ⌃⌘N | Finder'ınki |
| `add-to-stack` | Ctrl+Shift+S | ⌘⇧S | |
| `toggle-stack`, `save-tab-set`, `show-history` | — | — | |

- **Çakışma denetimi** (`Action::default_texts`, `fixed_owner`, `[[commands]]`): hiçbiri 6a/6b varsayılanlarıyla (`mod+shift+t/a/n/i/1/2`, `mod+1…9`, `mod+=`, `mod+-`, `alt+p`, `alt+left/right/up`, `alt+num+`, `ctrl+h`, macOS `mod+alt+v`, `mod+alt+backspace`, `mod+shift+.` …) ya da macOS menü çubuğunun sabit tuşlarıyla (⌘Q, ⌘H, ⌘M, ⌘⌥H) çakışmaz.
- **Ctrl+Alt'tan kaçınma:** Windows sol Ctrl+Alt'ı AltGr sayar; `keys::altgr_types` bu yüzden Ctrl+Alt ile harf ya da rakam dışı bir karakter çıkan basışları kısayol saymaz (`ea6301f`). Türkçe Q'da Ctrl+Alt+1…9 `> £ # $ ½ { [ ]` yazar, Ctrl+Alt+T `₺` yazar; bu kısayollar orada hiç çalışmaz. Ayrıca GNOME/Ubuntu Ctrl+Alt+T'yi sistem genelinde terminal için alır. Bu yüzden: sabitleme numaraları Alt+rakam; `open-terminal`'ın ilk (menülerde gösterilen) tuşu Shift+F4, Ctrl+Alt+T ikinci tuş olarak kalır. `new-folder-with-selection`'ın Ctrl+Alt+N'si bazı düzenlerde (Polonya'da `ń`) yazar; ayar dosyası şablonundaki uyarı bunu zaten söyler, kullanıcı değiştirir.
- **Rakam tuşu kuralı:** `keys::chord_from_press` bugün Ctrl/⌘ + rakam tuşunu, tuş ne yazarsa yazsın o rakam sayar (AZERTY'de `&`, `é`), Alt varken saymaz. Kural iki durum için genişler: Windows/Linux'ta yalnız Alt (Ctrl'siz, yani AltGr olamaz) + rakam tuşu, macOS'ta ⌘⌥ + rakam tuşu. Böylece Alt+1…9 AZERTY'de de çalışır.
- Hepsi `Action` listesine (46 → 63), `settings.toml` şablonundaki `[shortcuts]` yorumlarına, macOS menü çubuğuna (File: `Open Terminal`, `New Folder with Selection`; Edit: `Copy Path`, `Add to Drop Stack`; View: görünüm seçenekleri, `Drop Stack`, `Operation History`; Window: `Save Tabs As…`, `Open Tab Set`; Go: `Pinned 1…9`) ve ulaşılabilirlik testine eklenir.

## 11. Kod yapısı

| Parça | Yer |
|---|---|
| Yol biçimleri, `file://` kodlaması | `gezik-core/src/path_text.rs` (yeni) |
| UNC çözümü, terminal seçimi ve başlatma, yükseltilmiş başlatma | `gezik-platform/src/terminal.rs` (yeni), `gezik-platform/src/fs/windows.rs` |
| Pano: metin yazma, resim ve metin okuma | `gezik-platform/src/clipboard.rs`, `linux/{x11,wayland}.rs` |
| Bağlantı oluşturma (`.lnk`, junction, symlink, izin denemesi) | `gezik-platform/src/link.rs` (yeni) |
| `LinkTask`, `GroupTask`, şablon kopyası, içerikli yeni dosya; geri alma sırası | `gezik-ops/src/tasks/{link, group, copy, new}.rs`, `gezik-ops/src/inverse.rs` |
| Oturum modeli (sekmelerden kayıt, kayıttan sekmeler), şablon ve yapıştırma adları | `gezik-core/src/nav.rs`, `gezik-core/src/templates.rs` (yeni) |
| Tarih ve boyut biçimleri, `folders-first`, gizli/sistem bayrağı | `gezik-core/src/{lib, sort}.rs`, `gezik-platform/src/datetime.rs` |
| Ayarlar (`[view]` ekleri, `[session]`, `[terminal]`, `[[tab-sets]]`, `pinned` öğeleri), durum (`[session]`), yazıcı değişiklikleri | `gezik-config/src/{settings, settings_edit, settings_writer}.rs`, `templates/settings.toml` |
| Yeni eylemler, rakam tuşu kuralı | `gezik-config/src/shortcuts.rs`, `gezik/src/keys.rs`, `gezik/src/actions.rs` |
| Bağlantı bırakma tuşu, `Hit::Stack` | `gezik-core/src/drag.rs`, `gezik/src/drag.rs` |
| Birden çok alt menü, yeni menü öğeleri, kimlik aralığı | `gezik/src/context_menu.rs`, `gezik-platform/src/shell_menu.rs`, `gezik/ui/app.slint`, `gezik/ui/widgets/popup-menu.slint` |
| Kenar çubuğu grupları, takma adlar | `gezik/src/sidebar.rs`, `gezik/ui/widgets/sidebar.slint` |
| Oturum geri yükleme, sekme setleri | `gezik/src/{start, navigation, tab_tools}.rs`, `gezik/src/main.rs` |
| Bırakma yığını | `gezik/src/stack.rs` (yeni), `gezik/ui/widgets/drop-stack.slint` (yeni) |
| İşlem günlüğü | `gezik/src/operations.rs`, `gezik/ui/widgets/ops-panel.slint` |

### 11.1 Menüler: birden çok alt menü ve kimlik aralığı

- Bugün üç menü yolunun üçü de (Windows'ta `show_shell_menu`'nün `ShellSubmenu`'sü, Slint `ContextMenuArea`'nın `menu-sub-entries`'i, Linux'ta Gezik'in `PopupMenu`'sü) yalnız **bir** alt menü taşır ("Commands ▸"). Bu adımda bir satır menüsünde `Copy path as ▸`, Windows'ta `Create link ▸` ve `Commands ▸` birlikte bulunur. `Option<Submenu>` `Vec<Submenu>` olur; Windows'ta `ShellSubmenu` bir dilim olarak geçer; `app.slint` ve `popup-menu.slint` en çok dört alt menü yuvası taşır (Slint'te `Menu`'nün `for` içinde kullanılıp kullanılamadığını plan dener; olmuyorsa sabit dört yuva). Alt menüler yine tek düzeydir.
- Gezik'in menü kimlikleri 1000'in altındadır (`FIRST_SHELL_ID = 1000`; Explorer'ın kimlikleri oradan başlar) ve 1–970 aralığı neredeyse doludur (boş kalan küçük aralıklar: 13–19, 25–29, 45–49, 64–69, 74–79, 83–89, 612–619, 971–999). Yeni öğeler (yol biçimleri, bağlantı türleri, 50 şablon, terminal, 30×3 sekme seti, gruplar, yığın) sığmaz: `FIRST_SHELL_ID` 4096'ya çıkar (`LAST_SHELL_ID = 0x7FFF` aynı; Explorer'a yine 28.000'den çok kimlik kalır) ve yeni aralıklar 1000–4095'e yerleşir.

## 12. Test

- **Birim (saf):**
  - Yol biçimleri: Windows ve Unix yolları, tırnaklama (`"` ve POSIX `'\''`), `/` biçimi, ad, klasör yolu, `file://` (boşluk, `%`, `#`, Türkçe harfler, UNC, sürücü kökü), UNC dönüşümü (kapanışla), çok satır ve satır sonu.
  - Terminal seçimi: Windows'ta `wt` → `pwsh` → `powershell`, `wt`'de `;` kaçışı, yükseltilmiş argümanlar ve `'` ikilemesi; Linux sırası (`$TERMINAL`, `x-terminal-emulator`, masaüstüne göre öne alınan, bilinenler), klasör bayrakları; `[terminal] command`'da `{dir}`, `{{`, bilinmeyen yer tutucu; hiçbiri yokken hata.
  - Oturum: yazma–okuma, `drives`, kilit, aralık dışı `active`, bozuk ve eksik girdiler, `restore = false`'ın oturumu silmesi, komut satırı yolunun sona eklenmesi (`plan_start` genişler).
  - Sekme setleri ve `pinned`: metin ve tablo karışık okuma, takma ad/grup, yinelenen ve `..` içeren yollar, eski biçimle uyum, `with_pinned`'in takma adsız öğeleri metin bırakması, grupların yan yana tutulması, grup sırası değişimi.
  - Şablon ve yapıştırma adları: menü etiketi, gizli ve noktalı öğelerin atlanması, sıra ve 50 sınırı, `Pasted image … .png` biçimi (`:` yok), `(2)` üretimi.
  - Görünüm: tarih biçimleri (`relative` sınırları, `short`, `iso`), boyut biçimleri (1024/1000, etiketler, sınır değerler), `folders-first` kapalı sıralama, gizli/sistem süzme kuralları, uzantı gizlemenin noktalı adlara dokunmaması, `Entry` boyutunun değişmemesi.
  - Geri alma: `GroupTask` çıktısının tersi (önce geri taşıma, sonra boş klasörü çöpe), eski işlerin sırasının değişmemesi; bağlantı `Outcome::Created`'ının tersi.
  - Kısayollar: yeni varsayılanların çakışmaması, Alt+rakam ve ⌘⌥rakamın AZERTY'de ulaşılabilirliği, Türkçe Q'da Ctrl+Alt+T'nin kısayol olmaması (bilinen durum), ulaşılabilirlik testi.
  - Sürükleme: bağlantı tuşu kuralı (üç platform), `Hit::Stack`.
- **Platform testleri:** bağlantılar (Windows: `.lnk` hedefi `IShellLinkW` ile geri okunur; junction ve izin varsa symlink; çöpe atınca hedefin içi yerinde; Unix: symlink, çöpe atınca hedef yerinde); pano (metin gidiş-dönüş üç sistemde, Windows'ta `PNG` ve `CF_DIB` resim okuma, Linux'ta X11 ve Wayland metin hedefleri); UNC dönüşümü (bağlı sürücü yoksa `None`); terminal başlatma Windows'ta `wt` yokken PowerShell'e düşme (programı çalıştırmadan, komutu kurarak).
- **Linux kabı (Docker):** `test.sh unit` ve `gui.sh`'a yeni kipler: `terminal` (kaba `xterm` kurulur; doğru çalışma klasörüyle süreç başladı mı), `copy-path` (pano metni `xclip`/`wl-paste` ile okunur), `session` (çık, aç, sekmeler ve kilitler), `pins` (grup, takma ad, Alt+rakam), `view-options` (uzantı gizleme, klasörler önce, tek tık, tarih/boyut biçimi ekran görüntüsüyle), `templates`, `paste-file` (resim ve metin), `links`, `stack`, `history`. X11 ve Wayland (sway) ikisinde de pano ve sürükleme kipleri.
- **Windows ekran testleri:** her planın sonunda bir kontrol listesi (5. ve 6. adımdaki gibi): Windows Terminal'li ve Terminal'siz makinede terminal, UAC ile yönetici terminali ve bağlı ağ sürücüsünde UNC, Explorer'ın "Create shortcut"unun geri alınması, Alt-sürükleme ile `.lnk`, Türkçe Q'da Alt+1…9, gizli/sistem öğeleri.
- **macOS:** yeni eylemler, Terminal.app, ⌘⌥ tuşları, pano resmi (ekran görüntüsü kopyası) `docs/superpowers/notes/macos-test.md`'ye eklenir.

## 13. Performans

- **Açılış:** oturum geri yüklemesi yalnız etkin sekmeyi okur, varlık denetimi yapmaz; açılış süresi (≤ ~60 ms) ve "20 sekme açıkken bellek tek sekmeye göre en fazla +2 MB" ölçütü (Gezinme spec'i) korunur. `scripts/perf/tabs.ps1` 20 sekmelik oturumla açılışı ölçer.
- **Liste:** gizli/sistem bayrağı ek sistem çağrısı istemez, `Entry` büyümez; uzantı gizleme ve tarih/boyut biçimi yalnız görünen satırlar biçimlenirken uygulanır; `folders-first` sıralamaya bir karşılaştırma ekler ya da çıkarır. 100.000 dosyada sıralama ve kaydırma süreleri `master`'dakini belirgin aşmaz (`scripts/perf/measure.ps1`, `grid.ps1`).
- **Pano:** menü açılırken ve Ctrl+V'de yalnız biçim varlığına bakılır; resim kodlama iş parçacığında.
- **Şablonlar:** `templates/` yalnız `New ▸` açılınca okunur (yerel, en çok 50 öğe). Sembolik bağlantı izni açılışta bir kez, arka planda denenir.
- **Bellek:** yığın en çok 1000 yol, günlük en çok 200 kayıt (kayıt başına en çok 50 hata satırı), ikisi de ilk kullanımda ayrılır. Boşta bellek `master`'a göre büyümez (bugün 7,2–7,3 MB; `measure.ps1` ile her planın sonunda).
- **Exe:** her parça ≤ +0,25 MiB, 7a + 7b + 7c ≤ +0,75 MiB, `master` `38c65b3`'ün sürüm derlemesine göre (plan 7a bayt olarak ölçüp yazar). Yeni crate bağımlılığı yok; `windows` crate'ine yalnız `Win32_NetworkManagement_WNet` özelliği eklenir.
