# Taşınabilirlik 14f (senkron klasörü) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ayarlar, temalar ve şablonlar seçilen bir eşitleme klasöründe (OneDrive, Dropbox, iCloud Drive, Syncthing, ağ sürücüsü); durum, görünümler, araçlar, etiket kaydı, yedekler yerelde. `settings.local.toml` bu bilgisayarın değerleri; `[system]` oraya kendiliğinden. İki makinenin aynı anda yazması sessizce kaybolmaz: yazıcı yarışı yeniden uygular, eşitleyicinin çakışan kopyası tanınır, anahtar anahtar Review…; klasör yokken son kopyadan salt okunur açılış. Exe ≤ 14e + 262.144 B. Adım 14'ün son parçası.

**Spec:** `docs/superpowers/specs/2026-10-12-tasinabilirlik-design.md` §3.1, §3.5, §8, karar 11, 12, 15, §13.2 14f. Plan önceden onaylıdır. Dal `feat/portable-14f` (`feat/portable-14e` üstüne), `D:\Work\gezik-14f`.

## Kararlar (soru sorulmadı)

1. **Yer:** `paths::location()` yerel klasörde `location.toml`'u bir kez açar (yoksa bir başarısız `open`); `shared = "{home}/Dropbox/Gezik"` `KnownDirs` ile çözülür, `Dirs.synced = true`. Saf kısım `sync_folder::{parse_location, location_text}`.
2. **Çalışırken taşınır:** `ConfigStore`'un paylaşılan yeri `Arc<Mutex<Dirs>>` (bütün klonlar ve yazıcı aynısını görür); `set_shared(Some/None)`. Kurulum ve bırakma yeniden başlatma istemez: iş parçacığında dosyalar, `set_shared`, izleyicinin `follow()`'u, yeniden yükleme. `shared_dir()` artık `PathBuf` döner.
3. **Katman:** `Settings::parse_layered` paylaşılan tabloya yerel tabloyu `sync_folder::layer` ile koyar (tablolar derin, diziler ve tablo dizileri bütün; yerel kazanır). Bozuk yerel dosya uyarıyla dışarıda kalır. Senkron yokken okunmaz.
4. **Yazma yönü:** senkron açıkken `System` ve yeni `Local` değişkeni yerel dosyaya, `Value` yerel dosyada anahtarı varsa oraya; geri kalan paylaşılana. Kısayol: View menüsünün seçenekleri gibi öbür değişiklikler yerel dosyadaki aynı anahtarı görmez (paylaşılana yazar, yerel kazanmaya devam eder).
5. **Yarış:** `settings_writer::rewrite(path, missing, edit, between)`: okumadan önce `(boyut, mtime)`, yazmadan hemen önce yeniden; değişmişse düzenleme yeni metne yeniden uygulanır, en çok 3 kez; sonra uyarı ve değişiklik bellekte (`unsaved`) kalır, sonraki yeniden yüklemede yeniden verilir. Geçici dosya `write_atomic`'in `.settings.toml.<pid>.tmp`'i (izleyici yok sayar).
6. **Klasör yokken:** her başarılı okumadan sonra `sync-cache/` (yalnız değişen dosya yazılır, giden tema silinir). Okuma başarısızsa ya da `settings.toml` yoksa önbellek, `offline`, durum çubuğunda not, pencere salt okunur, yazıcı paylaşılan klasörü ve şablonu **yapmaz**, değişiklikleri bekletir. `ensure_initialized` senkron klasörünü asla yaratmaz.
7. **İzleyici:** tek süreç izleyicisi `watcher` içinde durağan; `follow()` paylaşılanı (varsa, alt klasörleriyle), senkron açıkken üst klasörü (geri gelişi görür) ve yerel klasörü (alt klasörsüz; `settings.local.toml`) izler. Üst klasör de yoksa (çıkarılmış sürücü) Sync satırında `Check Again`. Yoklama yok.
8. **Çakışan kopya adları** (`sync_folder::conflict_of`, testli): Dropbox/Nextcloud `(… conflicted copy …)` ve Türkçe `çakışan kopya`, Google Drive/Windows `(1)`, Syncthing `.sync-conflict-…`, ownCloud `_conflict-…`, iCloud `settings 2`, OneDrive `settings-<BİLGİSAYAR>[-2]` yalnız klasörün `machines/`'indeki adlarla. Asıl dosya yoksa çakışma sayılmaz (`solo (1).toml` bir temadır). Kurulumda bilgisayar adı `machines/<ad>` boş dosyasıyla yazılır (her bilgisayar kendi dosyası: yarışsız).
9. **Review:** iki yönlü fark (`sync_folder::differences`; tablolar anahtar anahtar, liste tek satır), satır başına `This copy` / `Other copy`, `Merge` / `Cancel`. `settings.toml` için `SettingsChange::FromCopy` yazıcıdan (yarış denetimli), tema için aynı `rewrite`; sonra kopya çöpe. Hiçbir şey kendiliğinden birleşmez.
10. **Kurulum:** `Keep Settings in a Folder` 14d'nin satır kalıbıyla yazılan yol + Enter (sistem klasör seçicisi yerine; Export/Import gibi). Boş klasöre yerel dosyalar kopyalanır (oradakiler ezilmez); `settings.toml` varsa `Use` / `Replace` / `Cancel`; her yolda önce yedek. Bu bilgisayarın `[system]` tablosu `settings.local.toml`'a kopyalanır (yoksa). Bulut kökündeyse `cloud_pin::keep` (Windows `CfSetPinState`). Şablonlar hiçbir yönde ezilmez.
11. **Bırakma:** klasörün (yoksa önbelleğin) `settings.toml` ve temaları (varsa şablonlar, ezmeden) yerele, önce yedek; `location.toml` silinir; klasöre dokunulmaz.
12. **14e kısayolu çözüldü:** `settings_backup::Folders { shared, local }`: `views.toml` ve `backups/` yerelde, gerisi paylaşılanda; dışa/içe aktarma, geri alma, yedek listesi buna göre.
13. **Pencere:** `SettingItem.place` (0 yok, 1 paylaşılan, 2 bu bilgisayar); sağ tık 2813 `Only on This Computer` / `Use the Shared Value`; satırda "this computer", ekran okuyucu da söyler. Yeni eylem yok (spec §10.2).
14. **Durum çubuğu bağlantısı yok:** not "Review it in Settings > Sync and Backup" der (bildirim metni tıklanamaz; ayar penceresi `Ctrl+,`).

## Task 1: Yapılandırma kütüphanesi
- [x] `sync_folder.rs` (yer metni, çakışma adları, `machines/`, katman, farklar, `with_item_from`, `local_with_system`), `paths.rs` (`Dirs.synced`, `location.toml`), `settings.rs` (`parse_layered`), `settings_writer.rs` (`rewrite`, yön, `unsaved`, `Local`, `FromCopy`), `store.rs` (taşınabilir yer, önbellek, çevrimdışı, çakışmalar, `is_config_file`).
- [x] Testler: `{token}` gidiş-dönüş ve `..` reddi; beş+ eşitleyicinin adları (Türkçe ve İngilizce makine adı), yanlış pozitif yok; katman önceliği; fark ve kopyadan yazma; 1.000 turluk sahte eşitleyici (iki değişiklik de kalır); 3 yeniden denemeden sonra uyarı ve bekleyen; yerel dosyaya yön; klasör yokken yaratılmaz ve geri gelince yazılır; önbellekten okuma; çakışma listesi; taşınma.

## Task 2: Uygulama
- [x] `watcher.rs` (`follow`, `reload`), `main.rs` (bekleyenleri yeniden verme, şablon klasörü), `settings_backup.rs` (`Folders`, `look`, `start_sync`, `stop_sync`), `settings_window.rs` + `backup.rs` (Sync satırları, Review, 2813, salt okunur), `settings.slint` (`place`), `cloud.rs` (öneri).

## Task 3: Ölçüm, denetimler, notlar
- [x] fmt, workspace build/test/clippy, macOS clippy, sürüm exe boyutu, `notes/parts/14f.md`, spec Durum satırı (adım 14 tamam), denetim listeleri (macOS 376–378, Linux 273–274), `docs/configuration.md` ("Sync folder"), README, exe kopyası.

## Sapmalar (spec'ten)

1. **Klasör seçici yerine yazılan yol** (karar 10); bulut kökü açıklamada öneri olarak.
2. **Durum çubuğunda tıklanır `Review…` yok** (karar 14).
3. **Temalar da anahtar anahtar** birleştirilir (spec yalnız tanınmayı söylüyordu).
4. **OneDrive deseni** "bilinen makine adları"nı `machines/` klasöründen öğrenir; adı sonradan değişen bilgisayarın eski adı tanınmaz.
5. **macOS iCloud yer tutucusu:** ayrı `brctl` yok; okuma indirmeyi tetikler (spec'in dediği gibi), not gösterilmez.
