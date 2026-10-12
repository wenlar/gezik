# Alt Proje 14: Taşınabilirlik — Tasarım

- **Tarih:** 2026-10-12
- **Durum:** Kapsam kullanıcı kararı (2026-10-12: ayar penceresi, dışa/içe aktarma, senkron klasörü, GitHub'dan tema, uygunsa taşınabilir kip; §2). Kullanıcı uyurken yazıldı; ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§17, her biri gerekçesiyle). Spec onaylanınca plan önceden onaylıdır. Henüz hiçbir parça uygulanmadı.
- **Kapsam:** Gezik yol haritasının 14. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md` satır 23, 83, 113; `2026-10-12-ozellik-durumu.md` satır 5 "Grafik ayar penceresi" ve 65 "Taşınabilir mod"). Tek spec, altı parça (14a–14f; §13).
- **Dayandığı:** `2026-10-03` ayarlar ve tema spec'i (düz metin ayar klasörü, canlı yeniden yükleme, `{token}` yollar), `2026-10-05-toplu-islemler-design.md` (zip yazıcı/okuyucu, indirilen araçlar), `2026-10-10-sistem-butunlesmesi-design.md` (sistem değişikliği günlüğü, tek örnek anahtarı, bulut yer tutucuları), `2026-10-12-etiketler-design.md` (12e'nin yorum koruyan düzenleyicisi `settings_edit.rs`, merkezi kayıt), `2026-10-12-onizleme-2-design.md` (13'ün eylem ve kimlik sayıları).
- **Taban:** `master` c103e95; 12 ve 13'ün bu spec'ten önce birleşeceği varsayılır: `Action::ALL` 127, menü kimlikleri 2799'a kadar ayrılmış (14: **2800–2999**), denetim listeleri macOS 387, Linux 281'de biter. Ayar klasörü bugün tek yer: `paths::config_dir()` (`GEZIK_CONFIG_DIR` ya da sistemin yeri); `ConfigStore` bu klasörde `settings.toml`, `themes/`, `templates/`, `state.toml`, `views.toml`, `pending-deletes`, `system-changes.toml`, indirilen araçlar (ffmpeg, 7-Zip, pdfium) ve 12a'dan sonra `tags-store.tsv`'yi tutar. `settings.toml`'a Gezik'in bütün yazmaları tek iş parçacığından (`SettingsWriter`), her seferinde dosyayı yeniden okuyup `toml_edit` ile yalnız değişen değeri düzenleyerek, atomik geçer.
- **Rakip notları:** TC (Yapılandırma penceresi, `wincmd.ini` exe yanında, USB kipi), DO (Tercihler, `Preferences ▸ Import/Export`, "Taşınabilir kur"), XYplorer (taşınabilir doğuştan, ayar dosyası exe yanında), Files ve OneCommander (tema mağazası/topluluk temaları), VS Code (Ayarlar arayüzü düz metin dosyanın üstünde, `Ctrl+,`, "Settings Sync"), Finder/Explorer (ayar penceresi, aktarma yok).

## 1. Amaç

Gezik'in düz metin ayarlarını **teknik olmayan kullanıcıya bir pencereyle açmak**, ayarları **bir dosyayla başka bilgisayara taşımak**, **Dropbox/OneDrive/Syncthing klasörüyle cihazlar arasında eşitlemek**, **GitHub'daki temaları tek adresle kurmak** ve Gezik'i **USB bellekte, ayarları yanında taşınır** biçimde çalıştırmak. Düz metin ayar ilkesi değişmez: pencere `settings.toml`'un üstünde bir görünümdür, kullanıcının yorumlarına, sırasına ve tanımadığı anahtarlara dokunmaz.

Hafiflik bozulmaz: pencere açılmadıkça yoktur (açılınca kurulur, kapanınca bırakılır), hiçbir şey arka planda güncelleme denetlemez, eşitleme klasörü Gezik tarafından yoklanmaz (zaten var olan ayar klasörü izleyicisi olayla uyanır).

### Başarı ölçütleri

- **Ayar penceresi (14a):** `Settings…` (Windows/Linux `Ctrl+,`, macOS `⌘,`) ≤ 150 ms'de açılır; şablondaki her tekil ayar (aç/kapa, seçim, sayı, metin, yol) pencerede görünür, açıklaması şablon yorumundan gelir; değiştirince ≤ 200 ms'de uygulanır ve `settings.toml`'da yalnız o satır değişir (bayt karşılaştırmalı test). Arama kutusu anahtar, başlık ve açıklamada süzer. Bozuk `settings.toml`'da pencere salt okunur ve hatanın satırını söyler.
- **Kısayollar (14b):** her eylem bir satır; tuşa basarak atama, çakışma anında adıyla gösterilir, `Reset`; `[shortcuts]` tablosuna yalnız değişen eylem yazılır.
- **GitHub'dan tema (14c):** `github.com/kişi/depo` (ya da `…/tree/…`, `…/blob/….toml`, herhangi bir https `.toml`) yapıştırılır, depodaki temalar listelenir, önizlenir, seçilenler `themes/`'e yazılır. İndirme ≤ 5 MB, tema dosyası ≤ 64 KB, en çok 32 tema; tema dışında hiçbir dosya diske yazılmaz.
- **Dışa/içe aktarma (14d):** `Export Settings…` tek zip yazar (ayarlar, temalar, isteğe bağlı şablonlar ve görünümler); `Import Settings…` neyin değişeceğini gösterir, program çalıştıran ayarları (`[[commands]]`, terminal, dış araç yolları) ayrıca adlarıyla uyarır, önce mevcut ayarları `backups/`'a yedekler; tek tıkla geri alınır.
- **Taşınabilir kip (14e):** exe'nin (macOS'ta `Gezik.app`'in) yanında `gezik-portable` dosyası varsa Gezik'in bütün dosyaları yanındaki `gezik-data/` klasöründe; sistemde iz kalmaz (kullanıcı Sistem Bütünleşmesi'nden bir şey açmadıkça). Kurulu Gezik ile aynı anda ayrı örnek olarak çalışır.
- **Senkron klasörü (14f):** ayarlar, temalar ve şablonlar seçilen klasörde; makineye özgü dosyalar (durum, görünümler, araçlar, günlük, merkezi etiket kaydı) yerelde. İki makine aynı anda değiştirdiğinde hiçbir değişiklik sessizce kaybolmaz: Gezik'in yazması yarışı yeniden dener, eşitleyicinin "çakışan kopya"sı fark edilir ve anahtar anahtar birleştirme sunulur. Klasör yokken Gezik son iyi kopyayla açılır.
- **Hafiflik (hepsi):** varsayılan ayarlarla boşta bellek `master`'a göre **0** (±0,02 MB), açılış ≤ +1 ms (14e/14f: yer çözümü bir `metadata` çağrısı + küçük `location.toml` okuması), klasör açma `master` ±%2. Her parça exe'ye ≤ +262.144 B (§12). Pencere kapandıktan sonra bellek açılmadan önceki değere ±0,1 MB içinde döner.

### Kapsam dışı (bilerek)

- **Liste ayarlarının pencerede düzenlenmesi** (`[[commands]]`, `[[view-rules]]`, `[[highlight-rules]]`, `[[sync-profiles]]`, `[[searches]]`, `[[filters]]`, `[[tab-sets]]`, `[[rename-presets]]`, `pinned`, `[tags] list`): her birinin ya kendi arayüzü var (kaydetme, iğneleme, 12e'nin etiket listesi) ya da düz metinde daha iyi duruyor; pencere sayısını ve "Edit in settings.toml" düğmesini gösterir (karar 4).
- **Tema düzenleyicisi** (renk seçicili): temalar `base` + birkaç renkle yazılır; pencere seçer, önizler, kurar.
- **Tema galerisi/mağazası**: sunucu yok; adres yapıştırılır. README GitHub konusu `gezik-theme`'i önerir.
- **Kendiliğinden tema/ayar güncelleme denetimi**: boşta hiçbir şey çalışmaz; `Check for Updates` elle.
- **Şifreli, hesapsız bulut senkronu**: 17. adım; 14f'nin klasörü ihtiyacın çoğunu karşılar.
- **Taşınabilir paket (zip, imza, güncelleme)**: Yayın hazırlığı (madde 65). 14e yalnız kipin kendisi.
- **Komut satırından dışa/içe aktarma** (`--export-settings`): istenmedi; zip elle de açılır.
- **Ayar penceresinin çevirisi**: 15. adım; açıklamalar şablon yorumlarından geldiği için şablonun çevirisiyle birlikte gelir.

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Kapsam | Grafik ayar penceresi, dışa/içe aktarma, senkron klasörü, GitHub'dan tema; taşınabilir kip uygunsa | Kullanıcı kararı 2026-10-12 |
| Ayar penceresi | `settings.toml`'u yerinde düzenler; yorumlar ve bilinmeyen anahtarlar kalır; 12e'nin yorum koruyan düzenleyicisi yeniden kullanılır | Kullanıcı kararı |
| Senkron klasörü | Dropbox/OneDrive vb. klasörü gösterilir; eşzamanlı düzenlemeler güvenle ele alınır | Kullanıcı kararı |
| GitHub temaları | Depo adresinden kurulum; güvenli: tema dosyaları yalnız veri, boyut sınırı, kod yok | Kullanıcı kararı |
| Hafiflik | Varsayılanlar isteğe bağlı, boşta hiçbir şey çalışmaz, UI iş parçacığı dosya sistemini beklemez, parça başına ≈ +262.144 B | Ürün hedefi |
| Soru | Kullanıcı uyurken; ayrıntılar sorulmaz, §17'de gerekçeyle | Bakımcı kuralı (gece izni) |

## 3. Ortak yapı

### 3.1 Ayar yerleri: paylaşılan ve yerel

Bugünkü tek klasör ikiye ayrılır (14e'de doğar, 14f kullanır); senkron ve taşınabilir kip yokken ikisi aynı klasördür ve hiçbir şey değişmez.

| Klasör | İçinde | Neden |
|---|---|---|
| **paylaşılan** (`shared`) | `settings.toml`, `themes/`, `templates/` | Makineden bağımsız: `{token}` yollar, sistemler arası aynı |
| **yerel** (`local`) | `state.toml`, `views.toml`, `pending-deletes`, `system-changes.toml`, `tags-store.tsv`, indirilen araçlar, `backups/`, `location.toml`, `settings.local.toml`, `sync-cache/` | Bu makinenin pencereleri, yolları, ikili dosyaları, sistem değişiklikleri |

Çözüm sırası (`paths::resolve_dirs()`, saf kısmı test edilir):

1. `GEZIK_CONFIG_DIR` varsa: yerel = o klasör.
2. Yoksa exe'nin yanında `gezik-portable` dosyası varsa: yerel = yanındaki `gezik-data/` (§7).
3. Yoksa sistemin yeri (bugünkü `config_dir()`).
4. Yerelde `location.toml` (`shared = "{home}/Dropbox/Gezik"`) varsa paylaşılan = o klasör, yoksa paylaşılan = yerel.

`ConfigStore::new(dir)` → `ConfigStore::new(Dirs { shared, local })`; `dir()` yerine `shared_dir()` ve `local_dir()`. Bugün `store.dir()`'i kullanan araç indirmeleri (`archives.rs`, `convert.rs`) yerele bakar. Tek örnek anahtarı (`instance::key`) yerel klasörden türer: taşınabilir Gezik kurulu olanla aynı anda ayrı örnek olur.

### 3.2 Yazma yolu: tek kuyruk, genel değer düzenlemesi

Pencere yeni bir yazıcı kurmaz. `SettingsChange`'e bir değişken eklenir:

```rust
/// One value in a table (`[layout] density`); `None` for a top-level key. `Reset` writes the default.
SettingsChange::Value { table: Option<String>, key: String, value: toml_edit::Value }
SettingsChange::Shortcut { action: String, chords: Vec<String> }   // 14b
```

İkisi de `settings_edit.rs`'nin bugünkü `edit_table`'ını kullanır (12e'nin `add_table`'ıyla: sondaki yorumlar yerinde kalır; eski değerin süsü — satır içi yorum — korunur). `edit_table` `pub(crate)` olur, yeni kod yazılmaz. Yazmadan önce dosya her seferinde yeniden okunur (bugün öyle); 14f bunu yarışa karşı sıkılaştırır (§8.3).

### 3.3 Ayar şeması (14a'da doğar)

`gezik-config/src/schema.rs` (yeni, saf): her tekil ayar için bir satır.

```rust
pub struct Field { pub table: Option<&'static str>, pub key: &'static str, pub page: Page, pub kind: Kind }
pub enum Kind { Bool, Choice(&'static [&'static str]), Int { min: i64, max: i64 }, Float { min: f64, max: f64 }, Text, Path, Theme, List /* "Edit in settings.toml" */ }
```

- **Açıklama ve varsayılan şablondan:** gömülü `templates/settings.toml` (zaten exe'de) `toml_edit` ile bir kez, pencere açılınca okunur; anahtarın üstündeki yorum satırları açıklama, satır içi yorum (`# left | right | hidden`) seçim ipucu, değer varsayılandır. Exe'ye yeni metin girmez.
- **Değer:** bellekte yüklü `Settings`'ten değil, `settings.toml`'un son okunan metninden (izleyici zaten okuyor) alınır: pencere dosyada ne yazıyorsa onu gösterir; dosyada olmayan anahtar "Default" rozetiyle varsayılanı.
- **Testler bağlar:** şablondaki her anahtar şemada ya da açık bir `FILE_ONLY` listesinde; şemadaki her anahtar şablonda ve yorumlu; her `Choice`'un seçenekleri `Settings::parse`'ın kabul ettikleri (ayrıştırıcı gidiş-dönüşü). Yeni ayar ekleyen her gelecek adım bu testle pencereye de girer.

### 3.4 Arayüz

- **Ayrı pencere** `SettingsWindow` (`ui/settings.slint`, `QuickLookWindow` gibi): süreçte tek, ilk açılışta kurulur, kapanınca bırakılır (`hide` değil `drop`). Solda sayfalar, üstte arama, sağda satırlar.
- **Tek satır bileşeni:** `SettingRow { kind, title, value, hint, default?, local? }`; aç/kapa `Check`, seçim açılır menü (`popup-menu.slint`), sayı/metin/yol `TextField` (Enter ya da odak kaybında yazar, tuş başına değil), yol satırında `…` (sistemin klasör seçicisi). Sayfa başına ayrı bileşen yok; sayfalar yalnız şemanın `page` süzgecidir.
- **Sağ tık satırda:** `Reset to Default`, `Copy Key` (`layout.density`), `Edit in settings.toml` (dosyayı sistemin düzenleyicisiyle açar), 14f'de `Only on This Computer`.
- **Ekran okuyucu:** her satırın erişilebilir adı başlık + değer + "default"; sayfa listesi sekme sırası.

### 3.5 Hafiflik kuralı (her parça)

- Pencere, indirme, aktarma, eşitleme denetimi **yalnız kullanıcı istediğinde**. Zamanlayıcı yok; senkron klasörü zaten var olan ayar izleyicisiyle izlenir (klasör değişir, izleyici aynıdır).
- UI iş parçacığı dosya okumaz: pencere bellekteki metinden kurulur; yazmalar `SettingsWriter` kuyruğunda; indirme, zip okuma/yazma, yedek, çakışma karşılaştırması iş parçacığında, sonuç olayla döner.
- Açılışa eklenen: 14e'de bir `metadata` (işaret dosyası), 14f'de `location.toml` okuması (yoksa bir başarısız `open`).

## 4. Ayar penceresi (14a)

### 4.1 Sayfalar

| Sayfa | İçerik (şablondaki tablolar) |
|---|---|
| General | `start-folder`, `max-fps`, `[session]`, `[history]`, `[files]` |
| Appearance | `theme`, `theme-light`, `theme-dark`, `[layout]`, `[view]` (ikon, tarih/boyut biçimi), `[preview]` |
| Files and Tools | `[archives]`, `[convert]`, `[tools]`, `[terminal]`, `[search]` |
| Layouts | `[panes]`, `[sidebar]`, gruplama/görünüm varsayılanları |
| Tags | `[tags]` tekil anahtarları; liste için 12e'nin yönetimi |
| Keyboard | `[keyboard]`; 14b'de kısayol tablosu |
| System | `[system]` satırları ve "System Integration…" düğmesi (9'un paneli; değerleri o yazar) |
| Advanced | `List` türü ayarlar: sayıları ve `Edit in settings.toml`; `Open Config Folder` |

14c, 14d, 14f'nin düğmeleri Appearance ve yeni bir **Sync and Backup** sayfasına girer.

### 4.2 Davranış

- **Anında uygulanır**, Tamam/İptal yok (karar 2): değişiklik kuyruğa gider, izleyici yeniden yükler, bütün pencereler güncellenir. `Reset to Default` varsayılan değeri yazar (satır ve yorumu yerinde kalır; anahtarı silmez).
- **Tema satırı** temaları listeler (yerleşikler + `themes/`), üstüne gelince geçici önizleme (yazmadan, bellekte `resolve_theme`), tıkla yazılır.
- **Doğrulama** türden: seçim yalnız seçenekler, sayı aralıkta (dışı kırmızı çerçeve, yazılmaz), yol yoksa sarı not ama yazılır (USB takılı olmayabilir).
- **Dış düzenleme:** pencere açıkken dosya değişirse satırlar yeniden okunur; odaktaki yazılmamış alan korunur, çakışırsa alanın altında "Changed in settings.toml: <değer>".
- **Bozuk dosya:** `toml_edit` ayrıştıramazsa pencere salt okunur, üstte `settings.toml line 12: …` ve `Edit in settings.toml`.
- **Bilinmeyen anahtarlar** pencerede görünmez, dosyada kalır (Advanced sayfasında "N keys Gezik does not know" bilgisi ve uyarı listesine bağlantı).

### 4.3 Menüler

- Windows/Linux: `≡` menüsünde ve sağ tık arka plan menüsünün sonunda `Settings…`; palette `settings`.
- macOS: uygulama menüsü `Settings…` `⌘,` (sistem kuralı); pencere macOS'ta da Gezik'in penceresi (NSPanel değil).

## 5. Kısayol sayfası (14b)

- Keyboard sayfasında eylem tablosu: ad, başlık, tuş(lar). Süzgeç kutusu ada ve tuşa göre ("ctrl+shift" yazınca o tuşları kullananlar).
- **Atama:** satırda `Set Key…` → "Press keys" alanı tuş birleşimini `Chord` olarak yakalar (yalnız değiştirici basılırsa bekler, `Esc` iptal, `Backspace` siler). `Shortcuts::bind` bugünkü çakışma denetimini yapar; sahibi varsa "Used by Copy Path — Replace?".
- **Yazma:** `[shortcuts]`'a yalnız değişen eylem (`new-tab = ["ctrl+t"]`), varsayılana dönünce anahtar yerinde varsayılan değerle kalır. `[[commands]]`'ın `shortcut` alanı bu tabloda "Command: X" satırı olarak görünür, yazma `[[commands]]`'a gider (12e'nin `with_tables`'ı).
- **macOS:** tuş metinleri `⌘⌥⇧⌃` sembolleriyle; menü çubuğu sistem kısayolları (⌘Q, ⌘H, ⌘,) değiştirilemez satır.

## 6. GitHub'dan tema (14c)

### 6.1 Akış

1. Appearance ▸ `Get Theme…` (palette `install-theme`): tek alanlı soru, adres yapıştırılır.
2. Adres çözülür (saf, test edilir):
   - `https://github.com/k/d` ya da `…/tree/<dal>/<yol>` → `https://api.github.com/repos/k/d/zipball[/<dal>]` (GitHub `codeload`'a yönlendirir; JSON ayrıştırılmaz).
   - `…/blob/<dal>/<yol>.toml` → `https://raw.githubusercontent.com/k/d/<dal>/<yol>.toml`.
   - Herhangi bir `https://…/*.toml` → olduğu gibi.
   - Başka her şey: "Paste a GitHub repository or a .toml address".
3. `gezik_platform::http::download` (sistem HTTP istemcisi, yalnız https, ≤ 5 yönlendirme) geçici dosyaya, **en çok 5 MB**, ilerleme ve iptal soruda.
4. Zip ise `zip` sandığıyla (gezik-batch'te zaten var) yalnız okunur, diske açılmaz: depo kökünde ya da `themes/` altında, `.toml` uzantılı, **≤ 64 KB**, UTF-8 girdiler, en çok 32 tane; başka hiçbir girdinin içi okunmaz.
5. Her aday `theme::parse_theme` ile denetlenir: geçerli TOML, `[colors]`/`[metrics]`/`base`'ten en az biri; bilinmeyen anahtarlar listelenir (Gezik zaten yok sayar). Geçmeyenler gerekçesiyle soluk satır.
6. Liste: ad (`name`), dosya adı, renk şeridi (accent, arka plan, metin), `Preview` (bellekte uygular, soru kapanınca geri döner), kutular. `Install` seçilenleri `themes/<ad>.toml` olarak atomik yazar; ad çakışırsa `Replace`/`Keep Both` (`nord-2.toml`). Tek tema kurulduysa "Use it now?".

### 6.2 Güvenlik (karar 7)

Tema yalnız veridir: renkler, ölçüler, `name`, `base`, `font-family` (sistem yazı tipi adı, dosya değil). Gezik tema dosyasından yol, adres, komut okumaz; bunu koruyan test: tema ayrıştırıcısının kabul ettiği anahtarlar sabit listeyle karşılaştırılır, listeye yol/komut türü eklenirse test kırılır. Dosya adı Gezik'in seçtiği (`[a-z0-9-]`, ≤ 48), zip'teki ad yalnız öneri; `..`, mutlak yol, bağlantı girdisi imkânsız çünkü hiçbir girdi açılmaz, yalnız içerik okunur. İndirme boyutu, girdi boyutu ve sayısı sınırlı (sıkıştırma bombası: okunan bayt sayılır, 64 KB'ta kesilir).

### 6.3 Kaynak ve güncelleme

Yazılan dosyanın ilk satırı: `# Installed from https://github.com/k/d (themes/nord.toml) on 2026-10-12`. Appearance'ta kaynaklı temanın sağ tık menüsü: `Check for Updates` (aynı adresi yeniden indirir, değişen varsa farkı renk şeridiyle gösterir), `Open Source Page`, `Delete Theme` (çöpe). Kendiliğinden denetim yok.

## 7. Dışa ve içe aktarma (14d)

### 7.1 Dışa aktarma

Sync and Backup ▸ `Export Settings…` (palette `export-settings`): kutular (`Settings and themes` hep, `Templates` açık, `Folder views` kapalı) → kaydetme yeri (varsayılan `Documents/gezik-settings-2026-10-12.zip`). İş parçacığında zip yazılır:

```
gezik-export.toml      # format = 1, gezik = "0.14.0", os = "windows", date = …
settings.toml          # paylaşılan + "Only on this computer" katmanı birleşmeden: yalnız paylaşılan
themes/*.toml
templates/**           # kutu açıksa; dosya başına ≤ 64 MB, toplam ≤ 256 MB, aşılırsa sorulur
views.toml             # kutu açıksa
```

`state.toml`, `system-changes.toml`, `tags-store.tsv`, araçlar, yedekler asla girmez (makineye özgü; karar 9).

### 7.2 İçe aktarma

`Import Settings…` → zip seçilir → iş parçacığında okunur (**≤ 300 MB, girdi ≤ 4.096**, yalnız izinli yollar: yukarıdaki liste; `..`, mutlak yol, bağlantı girdileri reddedilir; açılan bayt sayılır). Soru penceresi:

- Dosya listesi: `new` / `replaces` / `same`, kutularla.
- `settings.toml` farkı anahtar düzeyinde özet ("12 settings change: theme dark → nord, …").
- **Program çalıştıran ayarlar** ayrı, kırmızı başlıklı bölümde adlarıyla: `[[commands]]` (her birinin `run`'ı), `[terminal]`, dış diff aracı, `[convert]`/`[archives]` araç yolları, `[[sync-profiles]]` hedefleri. "These run programs on this computer. Import only files you trust." (karar 10).
- `gezik-export.toml` başka bir OS'tan diyorsa ve mutlak yollar varsa: "3 paths do not exist here" listesi; yine de alınır (`{token}` yollar zaten çevrilir).
- `Import` önce bugünkü paylaşılan dosyaları `local/backups/gezik-settings-<zaman>.zip` olarak dışa aktarır (aynı biçim; en çok 10 yedek, en eskisi çöpe), sonra seçili dosyaları atomik yazar. Bozuk `settings.toml` içeren zip alınmaz (hatası gösterilir).
- Sonuç satırı `Imported. Undo` — `Undo` son yedeği aynı yoldan geri alır. Sync and Backup ▸ `Restore Backup ▸` son 10 yedeği listeler.

## 8. Senkron klasörü (14f)

### 8.1 Kurma ve bırakma

Sync and Backup ▸ `Keep Settings in a Folder…`: klasör seçilir (önerilen: OneDrive/Dropbox/iCloud Drive/Syncthing kökü altında `Gezik`, Gezik tanıdığı bulut köklerini `cloud.rs`'den önerir).

- Klasörde `settings.toml` yoksa: paylaşılan dosyalar oraya kopyalanır (yerel kopyalar `backups/`'a da dışa aktarılır), `location.toml` yazılır.
- Varsa (başka makine önce kurmuş): "Use the settings in this folder" (varsayılan; yerel olanlar yedeklenir) ya da "Replace them with this computer's" (oradakiler önce `backups/`'a). Birleştirme sunulmaz; yedek her iki yolda da var.
- `Stop Keeping Settings There` paylaşılan dosyaları yerele kopyalar, `location.toml`'u siler; klasördeki dosyalara dokunmaz.
- `location.toml` yolu `{token}` ile yazar (`{home}/Dropbox/Gezik`): aynı Dropbox Windows ve macOS'ta farklı yerde olsa da çözülür.

### 8.2 Bu bilgisayara özgü ayarlar

Paylaşılan `settings.toml`'un üstüne yerelde isteğe bağlı `settings.local.toml` okunur: içindeki anahtarlar kazanır (tablo düzeyinde derin birleştirme; diziler bütün olarak). Pencerede her satırın sağ tık `Only on This Computer` seçeneği değeri yerel dosyaya taşır, satırda "This computer" rozeti. `[system]` (tepsi, genel kısayol) senkron açıkken kendiliğinden yerele yazılır (bir makinede tepsi açmak öbürlerinde açmamalı; karar 12). Senkron yokken dosya okunmaz (yoksa açılışa maliyet yok).

### 8.3 Eşzamanlı düzenleme güvenliği

Üç katman (karar 11):

1. **Gezik'in kendi yazması kaybetmez:** `SettingsWriter` okurken dosyanın `(boyut, mtime)`'ını alır; atomik değiştirmeden hemen önce yeniden bakar; değişmişse düzenlemeyi yeni metne yeniden uygular (en çok 3 kez, sonra uyarı). Düzenleme zaten anahtar düzeyinde olduğundan başka makinenin farklı anahtardaki değişikliği korunur. Geçici dosya `.settings.toml.<pid>.tmp` adıyla aynı klasörde (yeniden adlandırma atomik kalsın); izleyici ve `is_config_file` onu zaten yok sayar.
2. **Eşitleyicinin çakışan kopyası fark edilir:** izleyici paylaşılan klasörde şu adlardan birini görünce durum çubuğunda `settings.toml has a conflicting copy — Review…`: Dropbox `settings (… conflicted copy …).toml`, OneDrive `settings-<MAKİNE>.toml`, Google Drive/Windows `settings (1).toml`, Syncthing `settings.sync-conflict-*.toml`, Nextcloud `settings (conflicted copy …).toml` (desenler tek tabloda, testli). Temalar için de aynı.
3. **Anahtar anahtar birleştirme:** `Review…` iki metni ayrıştırır, farklı anahtarları listeler (`theme: nord | dark`), her satırda `This copy` / `Other copy`; `Merge` seçilenleri bizim yazma yolumuzla `settings.toml`'a yazar, çakışan kopyayı çöpe atar. Ortak ata yok (eşitleyiciler vermez); iki yönlü fark yeterli çünkü kullanıcı her farkı görür. Liste ayarlarında (`[[commands]]` gibi) bütün liste tek satır.

### 8.4 Klasör yokken

Paylaşılan klasöre her başarılı okumadan sonra `local/sync-cache/` güncellenir (yalnız değişince; boyut küçük). Açılışta klasör yoksa ya da okunamıyorsa (USB/ağ sürücüsü takılı değil, Dropbox bağlanmamış): ayarlar önbellekten okunur, durum çubuğunda `Settings folder not available — using the last copy`, pencere salt okunur; oturumdaki değişiklikler (iğneleme gibi) bellekte kalır ve not "not saved" der. Klasör geri gelince izleyici (üst klasör izlenir) uyarır ve yeniden yükler; bellekteki kaydedilmemiş değişiklikler o an kuyruğa yeniden verilir (yazma yolu anahtar düzeyinde olduğundan güvenli).

OneDrive "Files On-Demand" ve iCloud "Optimize Storage" `settings.toml`'u yer tutucuya çevirebilir: kurulumda klasör yer tutucu destekleyen bir köke ise Gezik dosyayı "her zaman bu cihazda" yapar (Windows `CfSetPinState`, 9'un bulut kodu; macOS'ta `brctl download` yerine okuma indirmeyi tetikler, not gösterilir).

## 9. Taşınabilir kip (14e)

### 9.1 İşaret ve yer

- İşaret: exe'nin yanında (Windows `Gezik.exe`, Linux ikili ya da `$APPIMAGE`'ın klasörü, macOS **`Gezik.app`'in yanında**, paketin içinde değil — imza bozulmasın) `gezik-portable` adlı dosya (içerik önemsiz). Veri: yanındaki `gezik-data/` (yoksa oluşturulur; yerel = paylaşılan = bu klasör, ilk açılışta şablonlar yazılır).
- `GEZIK_CONFIG_DIR` işaretten önce gelir (geliştirici ve testler için bugünkü davranış).
- Sync and Backup sayfasında `Portable: settings are kept in E:\Gezik\gezik-data` satırı ve `Open Data Folder`. Kipe geçiş için `Make Portable…` yok: kullanıcı dosyayı koyar; bugünkü ayarları taşımak için `Export`/`Import` (README tarifi; karar 13).

### 9.2 Sistemde iz

- Taşınabilir kip kendiliğinden hiçbir sistem yerine yazmaz: tek örnek kanalı, çöp, indirmeler hep `gezik-data` ya da işletim sisteminin geçici klasörü.
- Sistem Bütünleşmesi (varsayılan dosya yöneticisi, PATH, tepsi oturum açılışında, genel kısayol) yine çalışır ama her sorunun başında "Gezik is running in portable mode from E:\. These stop working when the drive is removed." Günlük dosyası makine adıyla ayrılır: `system-changes-<makine>.toml` (USB başka bilgisayarda açılınca o makinenin değişiklikleri karışmasın; karar 14).
- İndirilen araçlar (ffmpeg, 7-Zip, pdfium) `gezik-data/tools/<os>-<arch>/` altına: aynı bellek üç sistemde kullanılabilir.

### 9.3 Yazılamayan yer

`gezik-data` yazılamıyorsa (salt okunur USB, `Program Files`, CD): Gezik açılır, ayarlar okunur, `state.toml` yazılmaz (`keep_state_unwritten`, bugün var), durum çubuğunda `Portable data folder is read-only: changes are not saved`. macOS'ta karantinalı indirmede App Translocation paketi rastgele salt okunur yerden çalıştırır: yol `/AppTranslocation/` içeriyorsa not "Move Gezik.app out of Downloads (or open it once from Finder after moving) to use portable mode".

## 10. Ayarlar, eylemler, menüler

### 10.1 Ayarlar

Yeni `settings.toml` anahtarı yok (karar 3). Yeni dosyalar: yerelde `location.toml` (14f; `shared = "…"`), `settings.local.toml` (14f; `settings.toml` ile aynı biçim), `backups/` (14d), `sync-cache/` (14f); exe yanında `gezik-portable` ve `gezik-data/` (14e). `docs/configuration.md`'ye "Settings window", "Backup and moving", "Sync folder", "Portable mode" bölümleri; `docs/themes.md`'ye "Installing from GitHub" ve "Publishing a theme" (depo kökünde ya da `themes/` altında `.toml`, `gezik-theme` konusu).

### 10.2 Eylemler

| Eylem | Başlık | Windows / Linux | macOS | Parça |
|---|---|---|---|---|
| `settings` | Settings… | Ctrl+, | ⌘, | 14a |
| `edit-shortcuts` | Keyboard Shortcuts… | — | — | 14b |
| `install-theme` | Get Theme… | — | — | 14c |
| `export-settings` | Export Settings… | — | — | 14d |
| `import-settings` | Import Settings… | — | — | 14d |

- `Action` 127 → 128 (14a) → 129 (14b) → 130 (14c) → 132 (14d). Sayı yalnız `Action::ALL`'ın dizi türünde (PR #87). 14e ve 14f yeni eylem eklemez (pencere düğmeleri ve durum çubuğu bağlantısı).
- `Ctrl+,` bugün boş; `Key::parse` virgülü tanımıyorsa 14a'nın ilk görevi ekler (`comma`).

### 10.3 Menü kimlikleri

14 için **2800–2999**: `SETTINGS_*` 2800–2809 (`Settings…`, `Open Config Folder`, `Open settings.toml`), ayar satırı sağ tık 2810–2819 (`Reset to Default`, `Copy Key`, `Edit in settings.toml`, `Only on This Computer`), sayfa listesi 2820–2829, kısayol satırı 2830–2849, temalar 2850–2869 (`Get Theme…`, `Check for Updates`, `Open Source Page`, `Delete Theme`, `Open Themes Folder`), dışa/içe aktarma 2870–2879, `Restore Backup ▸` 2880–2889 (10 yedek), taşınabilir 2890–2899, senkron 2900–2919 (`Keep Settings in a Folder…`, `Stop Keeping Settings There`, `Review Conflict…`), 2920–2999 boş (ileri). `GEZIK_IDS_END = 4096` değişmez.

## 11. Kod yapısı

| Parça | Yer |
|---|---|
| Yerler | `gezik-config/src/paths.rs` (`Dirs`, `resolve_dirs`, işaret, `location.toml`), `gezik-config/src/store.rs` (`shared_dir`/`local_dir`), `gezik/src/{main, single_instance, archives, convert, system_changes}.rs` |
| Şema, yazma | `gezik-config/src/schema.rs` (yeni), `gezik-config/src/{settings_edit, settings_writer}.rs` (`Value`, `Shortcut`, yarış denetimi), `gezik-config/src/settings.rs` (`settings.local.toml` katmanı) |
| Pencere | `gezik/src/settings_window.rs` (yeni; sayfalar, satır modeli, olaylar), `gezik/ui/settings.slint` (yeni; `SettingsWindow`, `SettingRow`), `gezik/src/{actions, menu_bar, context_menu, palette}.rs` |
| Kısayollar | `gezik/src/settings_window/shortcuts.rs`, `gezik-config/src/shortcuts.rs` (`comma`, `bind` sorusu) |
| Tema kurulumu | `gezik-config/src/theme_source.rs` (yeni, saf: adres çözümü, aday süzme, ad seçimi, kaynak satırı), `gezik/src/theme_install.rs` (yeni; indirme iş parçacığı, soru) |
| Aktarma | `gezik-config/src/bundle.rs` (yeni; zip yaz/oku, izinli yollar, sınırlar, anahtar farkı, tehlikeli ayar listesi; `zip` çalışma alanı bağımlılığı, exe'de zaten var), `gezik/src/settings_backup.rs` (yeni) |
| Senkron | `gezik-config/src/sync_folder.rs` (yeni, saf: çakışma desenleri, iki yönlü fark, derin birleştirme), `gezik/src/{watcher, settings_window}.rs`, `gezik-platform/src/cloud` (sabitleme) |

## 12. Bütçe ve performans

| Parça | Exe (tahmin; sınır ≤ +262.144 B) | Boşta bellek | Kullanılırken |
|---|---|---|---|
| 14a | ≈ +190 KB (Slint penceresi ve satır ≈ 110, şema ≈ 35, olaylar ≈ 45) | 0 | açılış ≤ 150 ms; değer yazımı ≤ 200 ms görünür; pencere açıkken ≤ +4 MB, kapanınca ±0,1 MB |
| 14b | ≈ +60 KB | 0 | 132 eylemlik tablo ≤ 16 ms süzme |
| 14c | ≈ +45 KB (zip okuyucu var) | 0 | 5 MB'lık zip'ten aday çıkarma ≤ 100 ms |
| 14d | ≈ +55 KB (zip yazıcı var) | 0 | şablonsuz aktarma ≤ 100 ms; içe aktarma önizlemesi ≤ 200 ms |
| 14e | ≈ +15 KB | 0 | açılış ≤ +0,2 ms (bir `metadata`) |
| 14f | ≈ +80 KB (çakışma sorusu `tool-table`'la) | 0 | açılış ≤ +1 ms (`location.toml`); yazma yarış denetimi ≤ +1 ms |

Toplam ≈ +445 KB. 14a sınıra en yakın parça: aşılırsa sayfa listesi ve arama ilk PR'da kalır, Advanced sayfası bir sonraki parçaya kayar (PR'da gerekçeyle).

- **Ölçüm:** her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1` ve `stress.ps1`'i yazar; 14a için ek: pencereyi 20 kez aç/kapa sonrası bellek (`scripts/perf/settings.ps1`, yeni).
- **Zamanlayıcı ve yoklama yok** (§3.5).

## 13. Parçalar (her biri bir plan ve bir PR)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **14a** | Ayar penceresi: şema, `SettingsChange::Value`, pencere, sayfalar, arama, tema seçimi ve önizlemesi, `settings` eylemi, `Ctrl+,`/`⌘,` (§3.2–§3.4, §4) | — |
| 2 | **14b** | Kısayol sayfası (§5) | 14a |
| 3 | **14c** | GitHub'dan tema (§6) | 14a (Appearance sayfası) |
| 4 | **14d** | Dışa/içe aktarma, yedekler (§7) | 14a (Sync and Backup sayfası) |
| 5 | **14e** | Paylaşılan/yerel ayrımı ve taşınabilir kip (§3.1, §9) | 14d (yedek yolu `local/backups`) |
| 6 | **14f** | Senkron klasörü, `settings.local.toml`, yarış denetimi, çakışma birleştirme, önbellek (§8) | 14e (yer çözümü), 14d (kurulumda yedek) |

Sıra değer ve risk: pencere en çok kullanıcıya dokunan ve sonraki her parçanın düğme yeri. Tema kurulumu küçük ve bağımsız, erken görünür değer. Aktarma, senkronun güvenlik ağı (yedek) olduğu için ondan önce. Taşınabilir kip `ConfigStore`'un ikiye ayrılmasını küçük, gözden geçirilebilir bir değişiklikte getirir; senkron en riskli kısım (eşzamanlılık, bulut istemcileri) en sona, sağlam bir tabanın üstüne.

### 13.1 Süreç (11–13'ün dersleri)

- **Parçalar yığılı (stacked PR), paralel değil:** 14a–14d aynı `settings_window.rs`'ye, `Action`'a, menü kimliklerine ve `macos-test.md`/`linux-test.md`'ye dokunur. Yalnız saf modüller (`theme_source.rs`, `bundle.rs`, `sync_folder.rs`) önceki parçayla aynı anda yazılabilir.
- **Eylem sayısı yalnız dizinin türünde** (PR #87); menü kimlikleri ve denetim listesi aralıkları önceden ayrıldı.
- Windows → macOS → Linux sırası; doğrulanamayan kısım PR'da "doğrulanacak". Çapraz denetim planın sonunda (`cargo check` macOS ve Linux hedefleri); Docker yok. Windows ekran testleri kullanıcı uzaktayken ya da birincil ekranda (film izlerken), oyun sırasında asla; klavye düzeni ayarlarına dokunulmaz; Enter'dan önce odak doğrulanır.
- Gerçek ayar klasörüne dokunan ekran testleri yedek + bayt karşılaştırmalı geri yükleme ile (kullanıcının kuralı); OneDrive/Dropbox oturum açma kullanıcıya bırakılır.

### 13.2 Parça ayrıntıları

Denetim listesi aralıkları: macOS 388'den, Linux 282'den başlar.

**14a — ayar penceresi**
- *Dosyalar:* `gezik-config/src/{schema, settings_edit, settings_writer, shortcuts}.rs`, `gezik/src/{settings_window, actions, menu_bar, context_menu, palette}.rs`, `gezik/ui/settings.slint`, `docs/configuration.md`, `README.md`.
- *Testler:* şema ↔ şablon ↔ ayrıştırıcı bağları (§3.3); `Value` düzenlemesi bayt düzeyinde yalnız bir satırı değiştirir (yorumlu, yorumsuz, tablo yok, anahtar yok, satır içi tablo, sondaki yorum); `Reset` varsayılanı yazar; bozuk TOML'da yazma yok, pencere salt okunur; aralık dışı sayı yazılmaz; bilinmeyen anahtarlar kalır; dış düzenlemede odaktaki alan korunur; arama süzgeci; pencere kapandıktan sonra bileşen bırakılır (zayıf referans testi); `Ctrl+,` ayrıştırma ve çakışmasızlık.
- *macOS/Linux:* `⌘,` uygulama menüsünde; Retina'da satırlar; Linux'ta klasör seçici portal (GNOME/KDE).
- *Denetim listesi:* macOS 388–391: (388) `⌘,` pencereyi açar, ikinci `⌘,` aynı pencereyi öne getirir; (389) Density değişince ana pencere hemen sıkışır, `settings.toml`'da yalnız o satır değişmiş (`diff`); (390) dosyaya elle bozuk satır yazılınca pencere salt okunur ve satır numarasını söyler; (391) VoiceOver satırı adı ve değeriyle okur. Linux 282–283: (282) `Ctrl+,`, değişiklik ve `diff`; (283) Orca ile satır adı.

**14b — kısayollar**
- *Dosyalar:* `gezik/src/settings_window/shortcuts.rs`, `gezik-config/src/{shortcuts, settings_edit}.rs`, `gezik/ui/settings.slint`.
- *Testler:* yakalama (yalnız değiştirici, `Esc`, `Backspace`, AltGr ile yazılan karakter reddi), çakışma sorusu ve değiştirme, `[shortcuts]`'a yalnız değişen eylem, `[[commands]]` kısayolu kendi tablosuna, macOS sembol metni, `fixed_owner` satırları değiştirilemez.
- *Denetim listesi:* macOS 392–393: (392) `Copy Path`'e ⌘⌥C atanır ve menüde görünür; (393) ⌘Q satırı değiştirilemez. Linux 284: GNOME'da Super'li birleşim yakalanır ya da masaüstü aldıysa not.

**14c — GitHub'dan tema**
- *Dosyalar:* `gezik-config/src/theme_source.rs`, `gezik/src/theme_install.rs`, `docs/themes.md`.
- *Testler:* adres çözümü (depo, dal, alt klasör, blob, raw, `.git` sonu, sorgu dizgisi, http reddi, başka alan adı yalnız `.toml` ile); zip'ten aday süzme (kök, `themes/`, derin klasör yok sayılır, 64 KB sınırı okunan baytla, 32 sınırı, UTF-8 olmayan, bozuk TOML, tema olmayan TOML); ad temizleme ve çakışma (`nord-2`); kaynak satırı ve `Check for Updates` aynı adresi üretir; tema anahtar listesi sabiti (§6.2); yerel sahte zip ile uçtan uca (ağsız). Ağlı test `#[ignore]` (gerçek bir genel tema deposu).
- *Denetim listesi:* macOS 394–395: (394) bir GitHub tema deposu adresiyle liste, önizleme, kurulum; (395) proxy'li ağda (varsa) sistem ayarıyla indirir. Linux 285: `curl` yokken `wget`, ikisi de yokken ipucu.

**14d — dışa/içe aktarma**
- *Dosyalar:* `gezik-config/src/bundle.rs`, `gezik/src/settings_backup.rs`, `gezik/src/settings_window.rs`, `docs/configuration.md`.
- *Testler:* gidiş-dönüş (Windows'ta aktar, Linux yolu biçiminde al: `{token}` korunur); izinsiz yol, `..`, mutlak, bağlantı girdisi, 300 MB/4.096 sınırları, sıkıştırma bombası; tehlikeli ayar listesi (yeni/değişen `[[commands]]`, terminal, araç yolları; aynı olanlar listelenmez); anahtar farkı özeti; yedek önce yazılır, 10 yedek sınırı en eskiyi çöpe atar; `Undo` bayt aynı geri getirir; bozuk `settings.toml`'lu zip reddedilir; `state.toml` hiç girmez.
- *Denetim listesi:* macOS 396–397: (396) Windows'ta aktarılan zip macOS'ta alınır, temalar ve iğneler (`{home}`) çalışır; (397) `[[commands]]` içeren zip'te kırmızı bölüm. Linux 286: aynı gidiş-dönüş.

**14e — taşınabilir kip**
- *Dosyalar:* `gezik-config/src/{paths, store}.rs`, `gezik/src/{main, single_instance, archives, convert, system_changes, integration}.rs`, `docs/configuration.md`.
- *Testler:* çözüm sırası (ortam > işaret > sistem; saf, sahte exe yolu ile); macOS paket yanı (`…/X/Gezik.app/Contents/MacOS/gezik` → `…/X/gezik-portable`); `$APPIMAGE`; App Translocation algısı; yazılamayan `gezik-data` → durum yazılmaz, uyarı; araç yolu `tools/<os>-<arch>`; günlük dosyası makine adıyla; tek örnek anahtarı farklı; varsayılan kipte `shared == local` ve dosya yerleri bugünküyle aynı (göç yok).
- *Denetim listesi:* macOS 398–399: (398) `Gezik.app` yanına `gezik-portable` konunca `gezik-data` oluşur, `~/Library/Application Support/gezik` değişmez (zaman damgası); (399) karantinalı zip'ten açılışta Translocation notu. Linux 287–288: (287) AppImage yanında işaret; (288) salt okunur bağlanmış USB'de uyarı.

**14f — senkron klasörü**
- *Dosyalar:* `gezik-config/src/{sync_folder, paths, store, settings, settings_writer}.rs`, `gezik/src/{watcher, settings_window}.rs`, `gezik-platform/src/cloud`, `docs/configuration.md`, `README.md`.
- *Testler:* `location.toml` ve `{token}`; kurulumun iki dalı ve yedekler; `settings.local.toml` derin birleştirme (tablo, dizi bütün, yerel kazanır), `[system]`'in yerele gitmesi; yarış: okuma ile yazma arasında dosyayı değiştiren sahte "eşitleyici" iş parçacığı → iki değişiklik de kalır (1.000 tekrar), 3 denemeden sonra uyarı; çakışma desenleri (beş eşitleyici, Türkçe ve İngilizce makine adları; yanlış pozitif yok: `settings-old.toml` ve `settings-backup.toml` çakışma sayılmaz, OneDrive deseni yalnız bu makinenin ve bilinen makine adlarının biçimiyle); iki yönlü fark ve `Merge`; klasör yokken önbellekten okuma, salt okunur pencere, geri gelince kuyruğa yeniden verme; izleyici geçici dosyayı yok sayar.
- *Denetim listesi:* macOS 400–402: (400) iCloud Drive'da klasör, Windows'taki değişiklik macOS'ta görünür; (401) iki makinede çevrimdışı farklı değişiklik → çakışan kopya → `Review…` → birleşmiş dosya; (402) iCloud "Optimize Storage" açıkken açılış. Linux 289–290: (289) Syncthing `sync-conflict` dosyası; (290) Dropbox çalışmıyorken (klasör var ama eski) ve bağlanmamış ağ klasöründe açılış.

## 14. Test

### 14.1 Birim (saf; her sistemde)

`schema` (şablon bağları), `settings_edit` (`Value`, `Shortcut`; bayt düzeyinde), `settings_writer` (yarış yeniden denemesi), `theme_source`, `bundle`, `sync_folder`, `paths::resolve_dirs`, `shortcuts` (`comma`, yakalama kuralları).

### 14.2 Platform

- Windows: `GetModuleFileNameW` yolu, salt okunur USB, `CfSetPinState`, WinHTTP proxy'si, OneDrive çakışma adı (`settings-<MAKİNE>.toml`).
- macOS: paket yanı işaret, App Translocation, iCloud yer tutucusu, `NSURLSession` indirme.
- Linux: `$APPIMAGE`, portal klasör seçici, `curl`/`wget`.

### 14.3 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken): ayar penceresi 100/150/200 % ölçekte, ekran okuyucu (Anlatıcı), gerçek OneDrive klasöründe iki Gezik örneğiyle eşzamanlı yazma, USB'de taşınabilir kip ve kurulu Gezik aynı anda, gerçek bir GitHub tema deposu. Gerçek ayar klasörü yedek + bayt karşılaştırmalı geri yüklemeyle.
- **macOS** (`macos-test.md`'ye 388–402, sonuçlar `macos-test-results.md`'ye) ve **Linux** (`linux-test.md`'ye 282–290; GNOME ve KDE, X11 ve Wayland).

## 15. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| 14a exe bütçesi (yeni pencere) | Sınır aşılır | Tek satır bileşeni, sayfa başına bileşen yok, açıklamalar gömülü şablondan; ilk ve son ölçüm; aşılırsa Advanced sayfası sonraki parçaya |
| Ayrı Slint penceresi kapanınca belleği tam bırakmaz | Boşta bellek artar | Kapanınca `drop`; 20 aç/kapa ölçümü; bırakmıyorsa gizli tutulan tek pencere ve ölçüm PR'da |
| `toml_edit` düzenlemesi bazı biçimlerde (noktalı anahtar `layout.density = …`, satır içi tablo) yorumu ya da yeri bozar | Kullanıcının dosyası değişir | Bayt düzeyinde testler bu biçimlerle; noktalı/satır içi tanımlı anahtar pencerede salt okunur "edit in file" (düzenlenmez) |
| İçe aktarılan ayarlar program çalıştırır (`[[commands]]`, terminal) | Güvenilmeyen dosyadan kod çalışır | Ayrı kırmızı bölüm, adlarıyla; hiçbir komut içe aktarma sırasında çalışmaz; README uyarısı |
| Zip yol aşımı, sıkıştırma bombası | Ayar klasörü dışına yazma, disk dolması | İzinli yol listesi, `..`/mutlak/bağlantı reddi, okunan bayt sayılır, toplam ve girdi sınırları |
| GitHub API oranı (saatte 60 kimliksiz istek), `zipball` yönlendirmesi değişir | Kurulum başarısız | Hata metni "GitHub limit reached, try again in an hour"; blob/raw adresi API'siz çalışır; `codeload` doğrudan adres ikinci deneme |
| Tema dosyasına ileride yol/komut türü bir anahtar eklenmesi | "Yalnız veri" ilkesi sessizce bozulur | Tema anahtar listesi sabit testi (§6.2) |
| Eşitleyicinin çakışma adı biçimleri değişir ya da yerelleştirilir | Çakışma fark edilmez | Desenler tek tabloda, gevşek (`conflict`, `sync-conflict`, makine adı); fark edilmese de veri kaybı yok (kopya klasörde kalır); README |
| İki makine aynı saniyede yazar, eşitleyici "son yazan kazanır" der (iCloud) | Bir değişiklik kaybolur | Gezik tarafında anahtar düzeyinde yazma ve yeniden okuma; iCloud'un çakışma kopyası yoksa kayıp mümkün: bilinen sınır (§16) |
| OneDrive/iCloud yer tutucusu açılışta indirmeyi bekletir | Açılış gecikir | Ayar okuması zaten UI dışında; kurulumda dosya sabitlenir; macOS'ta not |
| Taşınabilir kipte Sistem Bütünleşmesi exe'si çıkarılan sürücüde | Varsayılan dosya yöneticisi ve PATH çalışmaz | Sorularda uyarı; günlük makine adıyla, geri alma bugünkü gibi |
| macOS App Translocation | İşaret görülmez, normal kipe düşer | Algı ve not (§9.3) |
| `Ctrl+,` bazı klavye düzenlerinde AltGr ile ya da başka tuşta | Tuş çalışmaz | Menü ve palette her zaman; ekran testinde görülürse varsayılan kaldırılır |
| Paralel parçaların ortak listelerde çakışması | Birleştirme hataları | Yığılı parçalar (§13.1) |

## 16. Bilinen sınırlar

- Liste ayarları (`[[commands]]`, kurallar, eşitleme profilleri) pencerede düzenlenmez; sayı ve `Edit in settings.toml`.
- Çakışma birleştirmesi iki yönlüdür (ortak ata yok); liste ayarları bütün olarak seçilir.
- Çakışan kopya üretmeyen eşitleyicilerde (iCloud Drive bazen, "son yazan kazanır") aynı anda yapılan iki değişiklikten biri kaybolabilir.
- Klasör yokken yapılan değişiklikler o oturumda bellektedir; Gezik klasör gelmeden kapanırsa kaybolur (not bunu söyler).
- GitHub dışı tema kaynakları yalnız doğrudan `.toml` adresiyle; GitLab/Gitea depo adresi yok.
- Taşınabilir kipte Gezik'in kendisi değil, kullanıcının açtığı sistem bütünleşmesi sistemde iz bırakır.
- Pencerenin ve şablon açıklamalarının çevirisi 15. adımda.

## 17. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 14a, 14b, 14c, 14d, 14e, 14f; yığılı** (§13). *Gerekçe:* pencere en geniş değer ve öbürlerinin düğme yeri; tema küçük ve bağımsız; aktarma senkronun yedek ağı; taşınabilir kip `ConfigStore` ayrımını küçük bir PR'da getirir; en riskli senkron sona.
2. **Pencerede değişiklik anında uygulanır, Tamam/İptal yok.** *Gerekçe:* Gezik'te ayarlar zaten kaydedince uygulanır; macOS ayarları ve VS Code böyle; İptal için bütün değişiklikleri tutup geri yazmak ikinci bir durum ve kod demek; `Reset to Default` ve 14d'nin yedekleri geri dönüşü verir.
3. **Yeni `settings.toml` anahtarı yok; yer bilgisi `location.toml`'da, makineye özgü değerler `settings.local.toml`'da.** *Gerekçe:* senkron klasörünün yeri paylaşılan dosyada olamaz (önce onu okumak gerekir); taşınabilir kipin işareti dosya varlığıdır, ayar değil.
4. **Liste ayarları pencerede düzenlenmez.** *Gerekçe:* her birinin bir arayüzü ya var (iğneleme, kayıtlı süzgeç/arama/profil, 12e etiket listesi) ya da düz metinde okunaklı; genel bir dizi düzenleyicisi bütçenin büyük kısmını yerdi ve kötü bir TOML düzenleyicisi olurdu.
5. **Açıklamalar ve varsayılanlar gömülü şablondan; şema yalnız tür ve sayfa taşır; testler üçünü bağlar.** *Gerekçe:* şablon zaten her anahtarı yorumuyla tutuyor ve exe'de; iki yerde metin bakımı ve exe maliyeti olmaz; yeni ayar ekleyen her adım testten pencereye girer.
6. **Ayrı pencere, açılınca kurulur, kapanınca bırakılır.** *Gerekçe:* ayarlar ana pencereden ayrı bir iştir (her sistemde öyle); bırakılan pencere boşta bellek tutmaz; ana pencerenin katman ağacı büyümez.
7. **Tema yalnız veri; zip açılmaz, yalnız `.toml` girdileri okunur; 5 MB / 64 KB / 32 sınırları; sistem HTTP istemcisi.** *Gerekçe:* kullanıcı kararı "kod yok"; temada yol/komut anahtarı yok ve test bunu kilitler; açılmayan zip'te yol aşımı imkânsız; sınırlar gerçek temaların yüzlerce katı; TLS kodu taşımamak 2026-10-06 kararı.
8. **Depo adresi `api.github.com/…/zipball` ile.** *Gerekçe:* varsayılan dalı JSON ayrıştırmadan çözer (yönlendirme); tek istek; dosya listesi için API sayfalaması gerekmez; blob/raw adresleri API'siz.
9. **Aktarma: tek zip; `state.toml`, günlük, etiket kaydı, araçlar asla; görünümler isteğe bağlı kapalı.** *Gerekçe:* `configuration.md` bunları zaten "taşınmaz" diyor; araçlar sisteme özgü ikili; zip her sistemde açılır ve elle incelenebilir.
10. **İçe aktarma birleştirmez, dosya dosya değiştirir; önce yedek; program çalıştıran ayarlar ayrı uyarılır.** *Gerekçe:* anahtar düzeyinde içe aktarma birleştirmesi ne olacağını tahmin edilemez kılar; yedek + `Undo` tam geri dönüş; paylaşılan ayar dosyası bir saldırı yolu olabilir (`[[commands]]`).
11. **Senkronda üç katman: Gezik'in yazmasında yarış denetimi, çakışan kopyanın algılanması, anahtar anahtar birleştirme; kilit dosyası yok.** *Gerekçe:* eşitleyiciler arasında kilit dosyası işe yaramaz (gecikmeyle gelir, kalırsa her şeyi kilitler); yazma zaten anahtar düzeyinde; kalan tek kayıp yolu eşitleyicinin çakışması ve onun kopyası kullanıcıya gösterilir.
12. **`settings.local.toml` katmanı; `[system]` senkron açıkken kendiliğinden yerelde.** *Gerekçe:* tepsi, genel kısayol ve makineye özgü yollar (terminal programı, `start-folder` `D:/…`) cihazlar arasında yayılmamalı; tek bir küçük birleştirme işleviyle olur; senkron yokken okunmaz.
13. **Taşınabilir kip: exe (macOS'ta `.app`) yanında `gezik-portable` dosyası → `gezik-data/`; `Make Portable…` yok.** *Gerekçe:* kullanıcının istediği işaret dosyası; macOS'ta paketin içi imzayı bozar; veri klasörü işaretten ayrı olunca işaret silinip kip kapatılabilir ve veri kalır; mevcut ayarları taşımak 14d ile tek adım.
14. **Taşınabilir kipte sistem değişikliği günlüğü makine adıyla.** *Gerekçe:* bir USB birkaç bilgisayarda açılır; tek günlük öbür makinenin PATH/kayıt değişikliklerini "geri alınacak" gösterirdi.
15. **Senkron klasörü yokken önbellekten salt okunur açılış.** *Gerekçe:* bir ayar sorunu Gezik'in açılmasını hiç engellemez (bugünkü ilke); önbelleğe yazmak geri gelen klasörle ikinci bir çakışma kaynağı olurdu.
16. **İndirilen araçlar yerel klasörde, taşınabilir kipte `tools/<os>-<arch>/`.** *Gerekçe:* senkron klasörüne yüz MB'lık ikili koymak eşitleyiciyi boğar ve başka sistemde çalışmaz; USB'de üç sistem yan yana durabilir.
17. **Menü kimlikleri 2800–2999; `Action` 127 → 132; denetim listeleri macOS 388–402, Linux 282–290.** *Gerekçe:* 13 2799'da, 387/281'de biter; aralıklar önceden ayrılınca yığılı parçalar çakışmaz.
18. **Bütçe: parça başına ≤ +262.144 B, boşta 0, açılış ≤ +1 ms.** *Gerekçe:* önceki adımların kuralı; bu adımın her özelliği isteğe bağlı, kullanmayana maliyet yok.
