# Taşınabilirlik 14d (dışa/içe aktarma, yedekler) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ayar penceresinde yeni **Sync and Backup** sayfası: `Export Settings` tek zip yazar (settings.toml, temalar, kutuyla şablonlar ve görünümler, `gezik-export.toml`); `Import Settings` zip'i iş parçacığında bellekte okur, neyin değişeceğini (dosya türü başına kutu, anahtar farkı özeti) ve program çalıştıran ayarları kırmızı satırda adlarıyla gösterir; `Import` önce dokunacağı dosyaları `backups/`'a yedekler (son 10), sonra dosya dosya değiştirir; `Undo Import` bayt bayt geri getirir; `Restore Backup` yedekleri listeler. Exe ≤ 14c + 262.144 B.

**Spec:** `docs/superpowers/specs/2026-10-12-tasinabilirlik-design.md` §7, karar 10, §13.2 14d. Plan önceden onaylıdır. Dal `feat/portable-14d` (`feat/portable-14c` üstüne, sonra `origin/master`), `D:\Work\gezik-14d`.

## Kararlar (soru sorulmadı)

1. **Eylemler:** `export-settings`, `import-settings` (`actions!` listesinde, tuşsuz; pencereyi Sync and Backup'ta açar). Menü kimliği yok (2870–2889 kullanılmadı). Denetim listeleri macOS 372–373, Linux 270.
2. **Saf kısım `gezik-config/src/bundle.rs`:** izinli adlar (`settings.toml`, `views.toml`, `themes/<ad>.toml`, `templates/…`; `..`, mutlak, sürücü, `\`, Windows'un tutamadığı adlar reddedilir), manifest, program çalıştıran ayarlar (`[[commands]]` `run`, `[terminal] command`, `[tools] diff`+`diff-args`, `seven-zip`, `[convert] ffmpeg`, `[[sync-profiles]]`; yalnız yeni ya da farklı olanlar), anahtar farkı ve özeti, yedek sırası (en yeni 10).
3. **G/Ç `gezik/src/settings_backup.rs`:** topla, dışa aktar (var olan dosyanın üstüne yazmaz), oku (≤ 300 MB zip, ≤ 4.096 girdi, ≤ 64 MB dosya, toplam açılan ≤ 300 MB; bozuk settings.toml/views.toml reddedilir; aynı ad iki kez reddedilir), önizle, yedekle + döndür (çöpe), içe aktar (yazma hatasında yazılanları geri koyar), geri al (önce yine yedek).
4. **Zip:** `gezik_batch::archive::read_zip_entries`'e toplam sınır parametresi; yazmak için aynı yerde `write_zip_entries` (mevcut `zip` sandığı, yeni bağımlılık yok).
5. **Arayüz yeni pencere değil:** satırlar 14a'nın bileşeni: yol alanı (tür 2), kutu (tür 0), seçim (tür 1: Import/Cancel, yedek adları, Undo), bilgi (tür 6), yeni tür 7 (kırmızı başlıklı uyarı). `settings.toml` bozukken yalnız ayar satırları salt okunur (`locked-rows`), tema ve yedek satırları çalışır.
6. **İş parçacığı:** her okuma/yazma `work()` ile (tek seferde bir), sonuç olayla döner; yazmadan önce `flush_settings`/`flush_views`; içe aktarılan views.toml bellekteki klasör görünümlerini de değiştirir.

## Task 1: Saf kısım, zip, G/Ç
- [x] `bundle.rs`, `archive::{read_zip_entries, write_zip_entries}`, `settings_backup.rs`.
- [x] Testler: izinli/izinsiz adlar, manifest, program listesi (aynı olan listelenmez), anahtar farkı, 10 yedek; dışa aktarmada `state.toml` vb. yok, `{token}` korunur; `..`/mutlak/izinsiz girdi dışarıda, bozuk settings.toml, 4.096+ girdi, çift ad; yedek önce yazılır, `Undo` bayt aynı, eklenenler silinir; 11 yedekten 10 kalır; toplam açılan bayt sınırı.

## Task 2: Pencere
- [x] `schema.rs` (`Page::SyncAndBackup`), `settings_window/backup.rs` (satırlar), `settings_window.rs` (eşleme, iş parçacığı, yedek listesi), `settings.slint` (tür 7, `locked-rows`), `view/mod.rs` (`replace_memory`), `shortcuts.rs`, `actions.rs`, `main.rs`, `trash_view.rs`.
- [x] Testler: önizleme satırları, kırmızı satır, kutular ve seçilen dosyalar, dışa aktarma kutuları.

## Task 3: Ölçüm, denetimler, notlar
- [x] macOS clippy, sürüm exe boyutu, `notes/parts/14d.md`, spec Durum satırı, denetim listeleri, `docs/configuration.md` ("Backup and moving"), README, exe kopyası.

## Sapmalar (spec'ten)

1. **Kaydetme/açma penceresi yok:** Gezik'te dosya penceresi yok; yol alanı + Enter. Var olan dosya sorulmaz, reddedilir.
2. **Kutular dosya türü başına**, dosya başına değil.
3. **"3 paths do not exist here" listesi yok:** başka OS'tan gelen zip için yalnız `{token}`suz yolların burada olmayabileceği söylenir.
4. **256 MB'ı aşan şablonlar sorulmaz**, reddedilir ("leave them out").
5. **Kırmızı bölüm** kırmızı başlıklı bir satır + kırmızı durum satırı.
6. **Yedek yalnız dokunulacak türleri** (settings ve temalar hep; şablon/görünüm içe aktarılıyorsa) kapsar; `Restore Backup` dosya silmez.
