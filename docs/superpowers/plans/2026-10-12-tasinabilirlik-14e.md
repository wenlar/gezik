# Taşınabilirlik 14e (paylaşılan/yerel ayrımı, taşınabilir kip) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** `ConfigStore` iki klasör bilir (`shared`: settings.toml, temalar, şablonlar; `local`: durum, görünümler, etiket kaydı, bekleyen silmeler, araçlar, yedekler, sistem değişiklikleri); işaretsiz ikisi bugünkü tek klasördür (göç yok). Exe'nin (macOS'ta `Gezik.app`'in, Linux'ta AppImage'ın) yanında `gezik-portable` varsa her şey yanındaki `gezik-data/`'da; araçlar `tools/<os>-<arch>/`, sistem değişikliği günlüğü makine adıyla, tek örnek anahtarı ayrı. Exe ≤ 14d + 262.144 B.

**Spec:** `docs/superpowers/specs/2026-10-12-tasinabilirlik-design.md` §3.1, §9, karar 13, 14, 16, §13.2 14e. Plan önceden onaylıdır. Dal `feat/portable-14e` (`feat/portable-14d` üstüne), `D:\Work\gezik-14e`.

## Kararlar (soru sorulmadı)

1. **Yer çözümü `gezik-config/src/paths.rs`:** saf `marker_folder(exe, appimage)` (paket yanı, AppImage klasörü), saf `choose(env, exe, appimage, has_marker, system)` (ortam > işaret > sistem; işaretsiz `/AppTranslocation/` yolu ayrı), `locate(choice, writable)` (diske bakar), süreç başına bir kez `location()` (`OnceLock`). `config_dir()` artık yerel klasörü verir: tek örnek anahtarı ve günlük kendiliğinden ayrılır.
2. **Yazılabilirlik denetimi** yalnız işaret varken: `gezik-data` varsa içine, yoksa işaretin klasörüne bir yoklama dosyası (hemen silinir). `gezik-data`'yı ilk açılış yapar (`ensure_initialized`, başlangıç dosyalarıyla).
3. **Salt okunur:** `gezik-data` var ama yazılamıyorsa kullanılır, `keep_state_unwritten`, `ensure_initialized` atlanır, durum çubuğunda `Portable data folder is read-only: changes are not saved`. `gezik-data` yok ve yapılamıyorsa sistem klasörüne düşülür, not söyler (çağıranın "fall back to the normal config dir" kuralı; spec yalnız ilk durumu anlatıyordu).
4. **`ConfigStore`:** `new(dir)` (tek klasör; testler) + `with_dirs(Dirs)`; `dir()` yerine `shared_dir()`/`local_dir()`/`tools_dir()`/`is_portable()`. İzleyici paylaşılanı izler.
5. **Araç yolu:** `gezik_batch::tools::install_dir` artık araç klasörünü alır (`<tools>/<ad>-<sürüm>`); kip yokken `local/tools` (bugünkü yer), taşınabilirde `local/tools/<os>-<arch>`.
6. **Günlük:** `system_journal::file_name(machine)`; taşınabilirde `system-changes-<makine>.toml` (`gezik_platform::system::machine_name`: Windows `COMPUTERNAME`, Unix `gethostname`, alan adı kısmı atılır). Kilit dosyası ortak kalır.
7. **Sistem Bütünleşmesi soruları** taşınabilirde "Gezik is running in portable mode from <klasör>. These stop working when the drive is removed." ile başlar.
8. **Sync and Backup sayfası:** taşınabilirde ilk satır `Portable: settings are kept in …`, seçeneği `Open Data Folder` (yerel klasörü sekmede açar). Yeni eylem ve menü kimliği yok (spec §10: 14e eylem eklemez; 2890–2899 kullanılmadı).
9. **Denetim listeleri:** macOS 374–375, Linux 271–272 (numaralar süregidiyor; spec'teki 398–399 / 287–288 yerine).

## Task 1: Yer çözümü ve depo
- [x] `paths.rs` (`Dirs`, `Choice`, `Location`, `marker_folder`, `choose`, `locate`, `location`), `store.rs` (iki klasör, `tools_dir`), `system_journal.rs` (`file_name`, `JournalFile::named`), `gezik-batch` araç yolu.
- [x] Testler: paket yanı, AppImage, paket olmayan `Contents/MacOS`; sıra (ortam > işaret > sistem, exe yok); Translocation; yazılamayan/olmayan `gezik-data`; yoklama dosyası kalmaz; tek klasörde dosyalar bugünkü yerde, araçlar `tools/`; ayrık depoda her dosya kendi klasöründe; taşınabilir araç yolu `tools/<os>-<arch>`; günlük adı (Türkçe ad, `.local`, yasak karakterler); makine adı var.

## Task 2: Uygulama
- [x] `main.rs` (salt okunur, not), `archives.rs`/`convert.rs`/`preview/mod.rs` (`tools_dir`), `system_changes.rs` (günlük adı, `portable_note`), `integration.rs` (soru başı), `settings_window.rs` + `settings_window/backup.rs` (satır, `Open Data Folder`), `watcher.rs`, `settings_backup.rs` (kısayol notu: tek klasör, 14f ayırır).

## Task 3: Ölçüm, denetimler, notlar
- [x] fmt, workspace build/test/clippy, macOS clippy, sürüm exe boyutu, `notes/parts/14e.md`, spec Durum satırı, denetim listeleri, `docs/configuration.md` ("Portable mode"), README, exe kopyası.

## Sapmalar (spec'ten)

1. **`gezik-data` yapılamıyorsa sistem klasörü** (yukarıda 3).
2. **Kurulum ve geçici klasör:** tek örnek kanalı (Windows adlı boru, Unix soketi geçici klasörde) zaten diske iz bırakmaz; yalnız anahtarı farklı.
3. **Dışa/içe aktarma ve yedekler** 14e'de paylaşılan klasörü kullanır (`views.toml` ve `backups/` dahil); kip yokken ve taşınabilirde iki klasör aynı olduğu için doğru, 14f ayırır.
4. **Mesajlardaki `system-changes.toml` adı** genel ad olarak kalır; satırdaki yol gerçek dosyayı gösterir.
