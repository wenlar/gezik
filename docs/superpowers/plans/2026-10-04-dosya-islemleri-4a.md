# Dosya İşlemleri 4a (Motor ve İşlemler) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gezik'e kendi dosya işlemleri motorunu kazandırmak: kopyala/taşı/çöpe at/kalıcı sil/yeniden adlandır/yeni klasör-dosya/çoğalt; çakışmaları önceden tek listede gösterme; sürücü başına kuyruk; pencere içi ilerleme paneli; sistem panosu; çok adımlı geri alma.

**Architecture:** Saf kurallar (ad üretimi, ad denetimi, çakışma kararları, yol karşılaştırma, hız/kalan süre, geri alma yığını, iş parçacığı sayısı) `gezik-core::ops`'tadır. Sistem işlemleri (ilerlemeli kopya, POSIX silme, çöp, sürücü bilgisi, pano, görev çubuğu) `gezik-platform`'dadır. Yeni `gezik-ops` crate'i bir `Task` arayüzünü çalıştıran motordur: her iş kendi koordinatör iş parçacığında sırasını (disk kümesi) bekler, ağacı tarar, çakışmasızları hemen işçilere verir, çakışmaları arayüze sorar, sonuçlardan geri alma görevlerini kendisi üretir. Uygulamada `operations.rs` motoru Slint'e bağlar: panel, çakışma katmanı, iletişim kutuları, yerinde yeniden adlandırma, menüler ve kısayollar.

**Tech Stack:** Rust 2024 (1.99), Slint 1.18 (yazılımla çizim), `windows` 0.62 + `windows-core` 0.62 (`#[implement]`), `libc` 0.2 (Unix), `objc2` 0.6 / `objc2-foundation` 0.3 / `objc2-app-kit` 0.3 (yalnız macOS).

**Spec:** `docs/superpowers/specs/2026-10-04-dosya-islemleri-design.md`

## Spec'ten sapmalar ve netleştirmeler

1. **`Task::undo` yok; tersler sonuçlardan üretilir.** Her `Task::run` bir `Outcome` döndürür (oluşturuldu, taşındı, çöpe atıldı, geri getirildi, silindi). Motorun `inverse` modülü bir işin tüm sonuçlarından ters görevleri kendisi kurar (oluşanları çöpe at → geri taşı → çöptekileri geri getir). Alt proje 5'in görevleri (dönüştürme, arşiv) yalnızca `Outcome::Created` döndürerek geri almayı bedava alır; spec 4.1 ve 4.3'teki davranış aynıdır, yalnız yeri değişir. `Task`'a bunun yerine `done(cancelled)` kancası eklenir (anında silmenin kaydını kapatmak için).
2. **Tarama Rust std'nin `read_dir`'i ile, tek iş parçacığında.** Windows'ta std zaten `FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)` kullanır ve türü bulma verisinden alır. Spec 5.1'deki "SSD'de klasörler paralel taranır" ilk sürümde yok; 100 bin öğe ≤ ~1 sn hedefi Task 16'da ölçülür, aşılırsa takip notuna yazılır.
3. **Windows panosu klasik pano API'siyle** (`CF_HDROP` + `Preferred DropEffect`), `SHCreateDataObject` ile değil. Explorer her iki yönde de bu biçimleri kullanır. Kesip yapıştırma bitince pano boşaltılır (Explorer'daki kesilmiş öğeler de böylece solukluktan çıkar); `Performed DropEffect`/`Paste Succeeded` yazılmaz. `SHCreateDataObject` 4b'de sürükleme için gelir.
4. **Linux sistem panosu 4b'ye kaldı.** X11 seçim sahipliği ve Wayland data-device altyapısı sürükle-bırakla ortaktır; 4a'da Linux'ta pano Gezik içidir (sekmeler arası çalışır). macOS panosu `NSPasteboard` (objc2) ile 4a'dadır.
5. **Linux'ta `redo` varsayılanı `mod+y`.** Kısayol sistemi yalnızca Mac/Diğer ayrımı yapıyor; `mod+shift+z` elle atanabilir.
6. **`drop-target` tema rengi 4b'de eklenir** (4a'da kullanılmaz). 4a üç renk ekler: `progress`, `progress-paused`, `progress-error`.
7. **Windows çöpe atma `FOF_WANTNUKEWARNING` ile.** Çöp Kutusu'na sığmayan bir öğe için Windows sessizce kalıcı silmek yerine kendi uyarısını gösterir.
8. **İş parçacıkları iş başına.** Her iş bir koordinatör + N işçi açar ve bitince kapatır; boşta hiç iş parçacığı yoktur, "~10 sn boşta kapanma" zamanlayıcısına gerek kalmaz. İlerleme bildiren tek bir raporcu iş parçacığı yalnızca iş varken çalışır.
9. **Farklı diske taşımada kopya başarılı ama kaynak silinemezse** öğe "Copied, but could not remove the original: …" hatasıyla başarısız sayılır; kopya geri alınmaz ve geri alma kaydına girmez.
10. **Windows satır menülerine de "Duplicate" eklenir** (spec 5.2: "Çoğalt: menü").
11. **Klasör çakışmalarının "Merge" satırları** çakışma listesinde yalnızca gerçek (dosya) çakışma da varsa gösterilir; tek başına birleşme liste açtırmaz.
12. **Çakışma listesinin alt satırı genel bir cümledir** ("Replaced files go to the Recycle Bin when their drive has one."). Hedef sürücünün çöpü olup olmadığını arayüz iş parçacığında sormamak için spec 6.2'deki "Replaced files cannot be restored" ayrımı yapılmaz; çöpsüz sürücüde değiştirilen dosya kalıcı silinir ve geri alma onu geri getiremez (motorun `Outcome::Deleted`'ı).
13. **Uygulama görevlerinin sırası:** yerinde yeniden adlandırma (Task 13), menülerin ve kısayolların "Rename" komutunu bağladığı görevden (Task 14) önce gelir; çakışma listesi en sonda (Task 15). Task 12–14 arasında çakışmalar varsayılan kararla (Atla) geçer.
14. **macOS/Linux kodu** bu makinede çalıştırılamaz. Task 16'da `rustup target add` başarılı olursa `gezik-core`, `gezik-platform`, `gezik-ops` için `cargo check --target x86_64-unknown-linux-gnu` ve `--target aarch64-apple-darwin` çalıştırılır; olmazsa takip notuna "derlenmedi" yazılır.

## Global Constraints

- Rust edition 2024, stable. Komutlar `D:\Work\gezik` içinden; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken çalıştırılır; uygulamayı elle çalıştırırken `GEZIK_CONFIG_DIR` geçici bir klasöre ayarlanır.
- Her görevin sonunda `cargo build --workspace` ve `cargo test --workspace` geçer, `cargo clippy --workspace --all-targets -- -D warnings` uyarısızdır, `cargo fmt --all -- --check` temizdir.
- Slint yalnızca yazılımla çizim; Slint özellik listesine hiçbir şey eklenmez.
- Yeni bağımlılıklar tam olarak: workspace'e `gezik-ops` (yol bağımlılığı); `gezik-ops` → `gezik-core`, `gezik-platform`; `gezik` → `gezik-ops`; `gezik-platform` → Windows'ta `windows-core = "0.62"` ve `windows` özelliklerine `Win32_Security`, `Win32_System_IO`, `Win32_System_Ioctl`, `Win32_System_Ole`, `Win32_System_DataExchange`, `Win32_System_Memory`, `Win32_System_Com_StructuredStorage`; macOS'ta `objc2 = "0.6"`, `objc2-foundation = { version = "0.3", features = ["NSArray", "NSError", "NSFileManager", "NSString", "NSURL"] }`, `objc2-app-kit = { version = "0.3", features = ["NSPasteboard"] }`.
- Arayüz iş parçacığı dosya sistemine dokunmaz (pano ve sürücü listesi gibi mevcut istisnalar dışında). Her dosya işlemi `gezik-ops::Engine`'den geçer.
- Hiçbir dosya kullanıcı "Replace" seçmeden ezilmez: kopyalama `COPY_FILE_FAIL_IF_EXISTS`/`create_new`, taşıma ezmeyen yeniden adlandırma kullanır.
- Kullanıcı verisi (dosya adları, bozuk `pending-deletes`, ayar ve durum dosyaları) hiçbir koşulda panic'e yol açmaz.
- Arayüz metinleri İngilizce. Tüm renk ve ölçüler `Theme` global'inden; yeni bileşenler erişilebilirlik rolü taşır.
- Sınırlar: geri alma geçmişi 100 kayıt; art arda 20 hata işi duraklatır; ilerleme olayları saniyede ≤ 10; `copy-threads` 1–16 (auto: SSD 6, HDD 1, ağ 4, bilinmiyor 1); ≥ 256 MB dosyalar önbelleksiz kopyalanır; panelde 1 sn'den kısa işler görünmez, başarılı biten satır 3 sn sonra kalkar; "Details" en çok 50 hata satırı gösterir.
- Kısayol biçimi önceki alt projelerdeki gibi (`mod` = macOS'ta Cmd, diğerlerinde Ctrl).
- Performans: açılış ≤ ~60 ms ve boşta ≤ 7 MB korunur; spec bölüm 11'deki kopyalama/silme hedefleri Task 16'da ölçülür.

## Review Focus

1. **Tarama ile çalıştırma arasında hedefte beliren dosya ezilmemeli** (başka bir program aynı adla dosya yazdı). Testler: Task 5 `copy_never_overwrites`, `move_entry_never_overwrites`.
2. **Elle bozulmuş veya yabancı yollar içeren `pending-deletes` dosyası açılışta yalnızca `.gezik-deleting-` ile başlayan adları silmeli.** Test: Task 10 `recovery_ignores_foreign_paths`.
3. **İşlemden sonra düzenlenen dosya Ctrl+Z ile çöpe gitmemeli.** Test: Task 11 `undo_skips_changed_files`.
4. **Bir klasörü kendi içine kopyalamak/taşımak sonsuz döngüye girmemeli, kendi üstüne taşımak hiçbir şeyi silmemeli.** Testler: Task 9 `copy_into_itself_fails`, `move_onto_itself_does_nothing`.
5. **Türkçe/Unicode adlar, çift uzantılar ve uzantılı Windows ayrılmış adları (`con.txt`)** numaralandırma ve ad denetiminde doğru işlenmeli. Testler: Task 1 `numbers_go_before_the_extension`, `windows_reserved_names_are_refused_with_any_extension`.

---

## Dosya yapısı

```
Cargo.toml                                       workspace: gezik-ops üyesi ve bağımlılığı
crates/gezik-core/src/lib.rs                     pub mod ops
crates/gezik-core/src/ops/mod.rs                 YENİ — modüller
crates/gezik-core/src/ops/names.rs               YENİ — split_name, numbered, next_free, rename_selection, validate_name
crates/gezik-core/src/ops/conflict.rs            YENİ — Facts, ConflictKind, Decision, Resolution, identical, source_newer, choices, resolve
crates/gezik-core/src/ops/paths.rs               YENİ — same_path, is_within, DriveSet, cover
crates/gezik-core/src/ops/rate.rs                YENİ — Rate, format_eta, format_rate
crates/gezik-core/src/ops/history.rs             YENİ — UndoStack<T>
crates/gezik-core/src/ops/threads.rs             YENİ — DiskKind, CopyThreads, workers
crates/gezik-config/src/settings.rs              [files] → FilesSettings; State.operations_collapsed
crates/gezik-config/src/shortcuts.rs             11 yeni eylem; varsayılanı olmayan eylemler
crates/gezik-config/src/theme.rs                 progress, progress-paused, progress-error
crates/gezik-config/themes/{dark,light}.toml     yeni renkler
crates/gezik-config/templates/{settings,example}.toml
crates/gezik-platform/Cargo.toml                 windows özellikleri, windows-core, objc2 (macOS)
crates/gezik-platform/src/lib.rs                 fs, clipboard, taskbar; MenuOutcome::Verb
crates/gezik-platform/src/fs/mod.rs              YENİ — DriveFacts, copy_file, delete, move_entry, drive_facts, drive_root, set_hidden, is_disk_full, trash, restore
crates/gezik-platform/src/fs/windows.rs          YENİ — CopyFileExW, POSIX silme, sürücü, IFileOperation çöpü
crates/gezik-platform/src/fs/unix.rs             YENİ — kopya (clone/copy_file_range/tampon), sürücü, çöp (macOS objc2, Linux freedesktop)
crates/gezik-platform/src/fs/freedesktop.rs      YENİ — .trashinfo biçimi, yüzde kodlama (her yerde derlenir ve test edilir)
crates/gezik-platform/src/clipboard.rs           YENİ — write_files, read_files, sequence, clear (Windows, macOS; Linux: Unsupported)
crates/gezik-platform/src/taskbar.rs             YENİ — Windows görev çubuğu ilerlemesi
crates/gezik-platform/src/shell_menu.rs          cut/copy/paste/delete/rename komutları Gezik'e
crates/gezik-ops/Cargo.toml                      YENİ
crates/gezik-ops/src/lib.rs                      YENİ
crates/gezik-ops/src/task.rs                     YENİ — Task, PlanItem, Stage, Outcome, Resources, ScanSink, RunCx, NoTrash, ChangedSince
crates/gezik-ops/src/walk.rs                     YENİ — ön sıralı ağaç gezme, facts_of
crates/gezik-ops/src/engine.rs                   YENİ — Engine, Event, kuyruk, iş akışı, raporcu, geçmiş
crates/gezik-ops/src/control.rs                  YENİ — Control (iptal, duraklatma, sayaçlar, kararlar)
crates/gezik-ops/src/run.rs                      YENİ — tarama alıcısı, çakışma tutma, karar uygulama, öğe çalıştırma
crates/gezik-ops/src/inverse.rs                  YENİ — sonuçlardan ters görevler
crates/gezik-ops/src/pending.rs                  YENİ — pending-deletes dosyası
crates/gezik-ops/src/tasks/{mod,copy,move_,trash,delete,restore,new}.rs   YENİ
crates/gezik-ops/src/testing.rs                  YENİ (cfg(test)) — test klasörü, bekleme yardımcıları, FakeTask, CollectSink
crates/gezik-ops/examples/ops_bench.rs           YENİ — motor ölçümü
crates/gezik/Cargo.toml                          gezik-ops
crates/gezik/src/operations.rs                   YENİ — Operations: motor ↔ Slint, panel, pano, komutlar
crates/gezik/src/dialog.rs                       YENİ — tek seferlik iletişim kutuları
crates/gezik/src/conflicts.rs                    YENİ — çakışma listesi modeli ve klavyesi
crates/gezik/src/view/mod.rs                     hide_names, on_shown, cut_names, yeniden adlandırma
crates/gezik/src/view/model.rs                   FileRow.cut
crates/gezik/src/navigation.rs                   refresh_showing
crates/gezik/src/context_menu.rs                 dosya öğeleri, arka plan menüsü, komut yönlendirme, çakışma kararı menüsü
crates/gezik/src/keys.rs                         testler: yeni varsayılanlar
crates/gezik/src/main.rs                         bağlantılar, tuşlar, kapanış sorusu, açılışta toparlama
crates/gezik/src/theme_bridge.rs                 yeni renkler
crates/gezik/ui/theme.slint                      yeni renkler
crates/gezik/ui/app.slint                        panel, çakışma katmanı, iletişim kutusu, özet, yeniden adlandırma bağları
crates/gezik/ui/widgets/ops-panel.slint          YENİ
crates/gezik/ui/widgets/conflict-list.slint      YENİ
crates/gezik/ui/widgets/dialog.slint             YENİ
crates/gezik/ui/widgets/text-button.slint        YENİ — metinli düğme (iletişim kutusu, çakışma listesi)
crates/gezik/ui/widgets/rename-field.slint       YENİ — yerinde yeniden adlandırma alanı
crates/gezik/ui/widgets/file-view.slint          yerinde yeniden adlandırma, kesilmiş öğe saydamlığı
scripts/perf/ops.ps1                             YENİ
README.md, docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md
```

---
### Task 1: Ad üretimi ve ad denetimi (`gezik-core/src/ops/names.rs`)

**Files:**
- Create: `crates/gezik-core/src/ops/mod.rs`
- Create: `crates/gezik-core/src/ops/names.rs`
- Modify: `crates/gezik-core/src/lib.rs` (modül listesi)

**Interfaces:**
- Consumes: —
- Produces:
  - `gezik_core::ops::names::split_name(name: &str, is_dir: bool) -> (&str, &str)`
  - `numbered(name: &str, is_dir: bool, n: u32) -> String`
  - `next_free(name: &str, is_dir: bool, taken: impl Fn(&str) -> bool) -> String`
  - `rename_selection(name: &str, is_dir: bool) -> (usize, usize)` (UTF-8 bayt aralığı)
  - `pub enum NameRules { Windows, Unix }`, `NameRules::current()`
  - `pub enum NameError { Empty, Dots, Character(char), Reserved(String), TrailingDotOrSpace, TooLong }` (`Display`)
  - `validate_name(name: &str, rules: NameRules) -> Result<(), NameError>`

- [ ] **Step 1: Modülü ve testlerini yaz**

`crates/gezik-core/src/lib.rs` içindeki `pub mod nav;` satırının altına ekle:

```rust
pub mod ops;
```

`crates/gezik-core/src/ops/mod.rs`:

```rust
//! File operations: the pure rules (names, conflicts, paths, undo history, speed). The
//! engine that runs them is the `gezik-ops` crate.

pub mod names;
```

`crates/gezik-core/src/ops/names.rs`:

```rust
//! New names (`rapor (2).pdf`) and whether a typed name is valid.

use std::fmt;

/// Endings kept together when a number goes in: `arsiv (2).tar.gz`, not `arsiv.tar (2).gz`.
const DOUBLE_EXTENSIONS: [&str; 4] = [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"];

/// `name` split into the part a number goes after and the rest: `("rapor", ".pdf")`,
/// `("arsiv", ".tar.gz")`, `("Makefile", "")`, `(".gitignore", "")`. A folder is all stem
/// (`v1.2`).
pub fn split_name(name: &str, is_dir: bool) -> (&str, &str) {
    if is_dir {
        return (name, "");
    }
    // ASCII lowercasing keeps every byte where it is, so offsets carry over to `name`.
    let lower = name.to_ascii_lowercase();
    for ext in DOUBLE_EXTENSIONS {
        if lower.len() > ext.len() && lower.ends_with(ext) {
            let at = name.len() - ext.len();
            return (&name[..at], &name[at..]);
        }
    }
    match name.rfind('.') {
        None | Some(0) => (name, ""),
        Some(at) => (&name[..at], &name[at..]),
    }
}

/// `stem` without a trailing ` (n)` (n ≥ 2, no leading zero) and that n; 1 if it has none.
fn strip_number(stem: &str) -> (&str, u32) {
    if let Some(inner) = stem.strip_suffix(')')
        && let Some(open) = inner.rfind(" (")
    {
        let digits = &inner[open + 2..];
        if !digits.starts_with('0')
            && let Ok(n) = digits.parse::<u32>()
            && n >= 2
        {
            return (&inner[..open], n);
        }
    }
    (stem, 1)
}

/// `name` with the number `n`: `rapor.pdf`, 2 → `rapor (2).pdf`; `rapor (2).pdf`, 3 →
/// `rapor (3).pdf`.
pub fn numbered(name: &str, is_dir: bool, n: u32) -> String {
    let (stem, ext) = split_name(name, is_dir);
    let (base, _) = strip_number(stem);
    format!("{base} ({n}){ext}")
}

/// The first of `name (2)`, `name (3)`… that `taken` does not claim. A name that already ends
/// in `(n)` goes on from n + 1. `name` itself counts as taken.
pub fn next_free(name: &str, is_dir: bool, taken: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_name(name, is_dir);
    let (base, from) = strip_number(stem);
    let mut n = from.saturating_add(1);
    loop {
        let candidate = format!("{base} ({n}){ext}");
        if !taken(&candidate) || n == u32::MAX {
            return candidate;
        }
        n += 1;
    }
}

/// What to select when renaming starts: the name without its extension (bytes, for Slint's
/// `set-selection-offsets`); all of it for folders and names like `.gitignore`.
pub fn rename_selection(name: &str, is_dir: bool) -> (usize, usize) {
    let (stem, _) = split_name(name, is_dir);
    (0, stem.len())
}

/// Which file system rules a name must follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRules {
    Windows,
    Unix,
}

impl NameRules {
    pub fn current() -> NameRules {
        if cfg!(windows) { NameRules::Windows } else { NameRules::Unix }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    Dots,
    Character(char),
    /// A Windows device name (`CON`, `COM1`…), in capitals.
    Reserved(String),
    TrailingDotOrSpace,
    TooLong,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => write!(f, "Type a name"),
            NameError::Dots => write!(f, "\".\" and \"..\" cannot be names"),
            NameError::Character(c) if c.is_control() => write!(f, "A name cannot contain control characters"),
            NameError::Character(c) => write!(f, "A name cannot contain {c}"),
            NameError::Reserved(name) => write!(f, "{name} is reserved by Windows"),
            NameError::TrailingDotOrSpace => write!(f, "A name cannot end with a dot or a space"),
            NameError::TooLong => write!(f, "The name is too long"),
        }
    }
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Whether `name` can be a file or folder name under `rules`.
pub fn validate_name(name: &str, rules: NameRules) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::Dots);
    }
    let too_long = match rules {
        NameRules::Windows => name.encode_utf16().count() > 255,
        NameRules::Unix => name.len() > 255,
    };
    if too_long {
        return Err(NameError::TooLong);
    }
    for c in name.chars() {
        let bad = match rules {
            NameRules::Windows => c < ' ' || "\\/:*?\"<>|".contains(c),
            NameRules::Unix => c == '/' || c == '\0',
        };
        if bad {
            return Err(NameError::Character(c));
        }
    }
    if rules == NameRules::Windows {
        if name.ends_with('.') || name.ends_with(' ') {
            return Err(NameError::TrailingDotOrSpace);
        }
        let device = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
        if RESERVED.contains(&device.as_str()) {
            return Err(NameError::Reserved(device));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_off_the_extension() {
        assert_eq!(split_name("rapor.pdf", false), ("rapor", ".pdf"));
        assert_eq!(split_name("Arsiv.TAR.GZ", false), ("Arsiv", ".TAR.GZ"));
        assert_eq!(split_name("Makefile", false), ("Makefile", ""));
        assert_eq!(split_name(".gitignore", false), (".gitignore", ""));
        assert_eq!(split_name("v1.2", true), ("v1.2", ""));
        assert_eq!(split_name(".tar.gz", false), (".tar", ".gz"), "nothing before the double extension");
        assert_eq!(split_name("şarkı.mp3", false), ("şarkı", ".mp3"));
    }

    #[test]
    fn numbers_go_before_the_extension() {
        assert_eq!(numbered("rapor.pdf", false, 2), "rapor (2).pdf");
        assert_eq!(numbered("arsiv.tar.gz", false, 2), "arsiv (2).tar.gz");
        assert_eq!(numbered("Makefile", false, 3), "Makefile (3)");
        assert_eq!(numbered("v1.2", true, 2), "v1.2 (2)");
        assert_eq!(numbered("rapor (2).pdf", false, 3), "rapor (3).pdf");
        assert_eq!(numbered("Çalışma Notları.txt", false, 2), "Çalışma Notları (2).txt");
    }

    #[test]
    fn next_free_skips_taken_names_and_continues_a_number() {
        let taken = ["rapor (2).pdf", "rapor (3).pdf"];
        assert_eq!(next_free("rapor.pdf", false, |n| taken.contains(&n)), "rapor (4).pdf");
        assert_eq!(next_free("rapor (2).pdf", false, |_| false), "rapor (3).pdf");
        assert_eq!(next_free("New folder", true, |_| false), "New folder (2)");
        // "(1)" and "(02)" are not Gezik numbers: the bracket stays part of the name.
        assert_eq!(next_free("a (1).txt", false, |_| false), "a (1) (2).txt");
        assert_eq!(next_free("a (02).txt", false, |_| false), "a (02) (2).txt");
    }

    #[test]
    fn rename_selects_the_stem() {
        assert_eq!(rename_selection("rapor.pdf", false), (0, 5));
        assert_eq!(rename_selection("şarkı.mp3", false), (0, "şarkı".len()));
        assert_eq!(rename_selection(".gitignore", false), (0, 10));
        assert_eq!(rename_selection("v1.2", true), (0, 4));
    }

    #[test]
    fn empty_dots_and_separators_are_refused_everywhere() {
        for rules in [NameRules::Windows, NameRules::Unix] {
            assert_eq!(validate_name("", rules), Err(NameError::Empty));
            assert_eq!(validate_name("   ", rules), Err(NameError::Empty));
            assert_eq!(validate_name("..", rules), Err(NameError::Dots));
            assert_eq!(validate_name("a/b", rules), Err(NameError::Character('/')));
            assert_eq!(validate_name("notlar.txt", rules), Ok(()));
        }
    }

    #[test]
    fn windows_characters_and_endings() {
        assert_eq!(validate_name("a:b", NameRules::Windows), Err(NameError::Character(':')));
        assert_eq!(validate_name("a\tb", NameRules::Windows), Err(NameError::Character('\t')));
        assert_eq!(validate_name("a.", NameRules::Windows), Err(NameError::TrailingDotOrSpace));
        assert_eq!(validate_name("a ", NameRules::Windows), Err(NameError::TrailingDotOrSpace));
        assert_eq!(validate_name("a:b", NameRules::Unix), Ok(()));
        assert_eq!(validate_name("a.", NameRules::Unix), Ok(()));
    }

    #[test]
    fn windows_reserved_names_are_refused_with_any_extension() {
        assert_eq!(validate_name("con", NameRules::Windows), Err(NameError::Reserved("CON".into())));
        assert_eq!(validate_name("con.txt", NameRules::Windows), Err(NameError::Reserved("CON".into())));
        assert_eq!(validate_name("Com1.tar.gz", NameRules::Windows), Err(NameError::Reserved("COM1".into())));
        assert_eq!(validate_name("console.txt", NameRules::Windows), Ok(()));
        assert_eq!(validate_name("con", NameRules::Unix), Ok(()));
    }

    #[test]
    fn long_names_are_counted_per_file_system() {
        let long = "ş".repeat(200);
        assert_eq!(validate_name(&long, NameRules::Windows), Ok(()), "200 UTF-16 units");
        assert_eq!(validate_name(&long, NameRules::Unix), Err(NameError::TooLong), "400 bytes");
        assert_eq!(validate_name(&"a".repeat(256), NameRules::Windows), Err(NameError::TooLong));
    }

    #[test]
    fn errors_read_well() {
        assert_eq!(NameError::Character(':').to_string(), "A name cannot contain :");
        assert_eq!(NameError::Character('\t').to_string(), "A name cannot contain control characters");
        assert_eq!(NameError::Reserved("CON".into()).to_string(), "CON is reserved by Windows");
    }
}
```

- [ ] **Step 2: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-core ops::names`
Expected: 9 test PASS. (Kod testlerle birlikte yazıldı; bir test başarısız olursa önce testin spec'e uygunluğunu, sonra kodu düzelt.)

- [ ] **Step 3: Tüm kontroller**

Run: `~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace && ~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`
Expected: hepsi temiz.

- [ ] **Step 4: Commit**

```bash
git add crates/gezik-core/src/lib.rs crates/gezik-core/src/ops
git commit -m "Name rules for file operations: numbered copies and name validation"
```

---

### Task 2: Çakışma kararları, yollar, hız, geri alma yığını, iş parçacığı sayısı (`gezik-core/src/ops`)

**Files:**
- Create: `crates/gezik-core/src/ops/conflict.rs`
- Create: `crates/gezik-core/src/ops/paths.rs`
- Create: `crates/gezik-core/src/ops/rate.rs`
- Create: `crates/gezik-core/src/ops/history.rs`
- Create: `crates/gezik-core/src/ops/threads.rs`
- Modify: `crates/gezik-core/src/ops/mod.rs`

**Interfaces:**
- Consumes: `gezik_core::format_size(u64) -> String`
- Produces:
  - `conflict::Facts { is_dir: bool, size: u64, modified: Option<SystemTime> }` (`Debug, Clone, Copy, PartialEq, Eq, Default`)
  - `conflict::ConflictKind { File, Folder, Mismatch }`, `conflict::kind_of(source: Facts, target: Facts) -> ConflictKind`
  - `conflict::Decision { Replace, Skip, KeepBoth, IfNewer, Merge }` (`Default` = Skip), `Decision::label(self) -> &'static str`
  - `conflict::identical(Facts, Facts) -> bool`, `conflict::source_newer(Facts, Facts) -> Option<bool>`
  - `conflict::choices(ConflictKind) -> &'static [Decision]`, `conflict::default_decision(ConflictKind, Facts, Facts) -> Decision`
  - `conflict::Resolution { Write, Replace, Skip, Rename }`, `conflict::resolve(Decision, ConflictKind, Facts, Facts) -> Resolution`
  - `paths::same_path(&Path, &Path) -> bool`, `paths::is_within(path: &Path, dir: &Path) -> bool`
  - `paths::DriveSet` (`new(impl IntoIterator<Item = String>)`, `intersects(&self, &DriveSet) -> bool`, `ids() -> &[String]`)
  - `paths::cover<T>(items: Vec<T>, path: impl Fn(&T) -> &Path) -> Vec<T>`
  - `rate::Rate` (`new(window: Duration)`, `record(now: Instant, done: u64)`, `per_second() -> Option<f64>`, `remaining(left: u64) -> Option<Duration>`), `rate::format_eta(Duration) -> String`, `rate::format_rate(f64) -> String`
  - `history::UndoStack<T>` (`new(cap)`, `push_new`, `push_undone`, `push_redone`, `pop_undo`, `pop_redo`, `peek_undo`, `peek_redo`)
  - `threads::DiskKind { Ssd, Hdd, Network, Unknown }`, `threads::CopyThreads { Auto, Fixed(u8) }` (`Default` = Auto), `threads::COPY_THREADS_RANGE`, `threads::workers(CopyThreads, &[DiskKind]) -> usize`

- [ ] **Step 1: `ops/mod.rs`'i güncelle**

```rust
//! File operations: the pure rules (names, conflicts, paths, undo history, speed). The
//! engine that runs them is the `gezik-ops` crate.

pub mod conflict;
pub mod history;
pub mod names;
pub mod paths;
pub mod rate;
pub mod threads;
```

- [ ] **Step 2: `conflict.rs`'i testleriyle yaz**

```rust
//! What happens when the target of a copy or move already exists.

use std::time::{Duration, SystemTime};

/// What is known about one side of a conflict (or about an item being copied).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Facts {
    pub is_dir: bool,
    /// 0 for folders.
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// Two files.
    File,
    /// Two folders: they are merged, their contents meet one by one.
    Folder,
    /// A file and a folder with the same name.
    Mismatch,
}

pub fn kind_of(source: Facts, target: Facts) -> ConflictKind {
    match (source.is_dir, target.is_dir) {
        (false, false) => ConflictKind::File,
        (true, true) => ConflictKind::Folder,
        _ => ConflictKind::Mismatch,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Decision {
    Replace,
    #[default]
    Skip,
    /// Keep the existing one; the new one gets `name (2)`.
    KeepBoth,
    /// Replace only if the source is newer.
    IfNewer,
    /// Folders only: merge (the only choice).
    Merge,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Decision::Replace => "Replace",
            Decision::Skip => "Skip",
            Decision::KeepBoth => "Keep both",
            Decision::IfNewer => "If newer",
            Decision::Merge => "Merge",
        }
    }
}

/// Modification times this close count as the same: FAT and exFAT store them in 2 second steps.
const SAME_TIME: Duration = Duration::from_secs(2);

fn same_time(a: SystemTime, b: SystemTime) -> bool {
    let gap = a.duration_since(b).or_else(|_| b.duration_since(a)).unwrap_or(Duration::MAX);
    gap <= SAME_TIME
}

/// Two files of the same size changed at the same time: very likely the same file.
pub fn identical(source: Facts, target: Facts) -> bool {
    !source.is_dir
        && !target.is_dir
        && source.size == target.size
        && matches!((source.modified, target.modified), (Some(a), Some(b)) if same_time(a, b))
}

/// `Some(true)` if the source is newer, `Some(false)` if the target is; `None` if they are
/// the same age or a time is unknown.
pub fn source_newer(source: Facts, target: Facts) -> Option<bool> {
    let (a, b) = (source.modified?, target.modified?);
    if same_time(a, b) { None } else { Some(a > b) }
}

/// The decisions offered for a conflict of `kind`.
pub fn choices(kind: ConflictKind) -> &'static [Decision] {
    match kind {
        ConflictKind::File => &[Decision::Replace, Decision::Skip, Decision::KeepBoth, Decision::IfNewer],
        ConflictKind::Folder => &[Decision::Merge],
        ConflictKind::Mismatch => &[Decision::Skip, Decision::KeepBoth],
    }
}

/// What a conflict starts with: nothing is ever replaced unless the user says so.
pub fn default_decision(kind: ConflictKind, _source: Facts, _target: Facts) -> Decision {
    match kind {
        ConflictKind::Folder => Decision::Merge,
        ConflictKind::File | ConflictKind::Mismatch => Decision::Skip,
    }
}

/// What the engine does with an item once its conflict is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Go ahead (a merged folder: its contents are handled one by one).
    Write,
    /// Move the existing target out of the way (to the trash), then go ahead.
    Replace,
    Skip,
    /// Go ahead under a free `name (n)`.
    Rename,
}

pub fn resolve(decision: Decision, kind: ConflictKind, source: Facts, target: Facts) -> Resolution {
    match (kind, decision) {
        (ConflictKind::Folder, _) => Resolution::Write,
        (_, Decision::KeepBoth) => Resolution::Rename,
        (ConflictKind::File, Decision::Replace) => Resolution::Replace,
        (ConflictKind::File, Decision::IfNewer) if source_newer(source, target) == Some(true) => Resolution::Replace,
        _ => Resolution::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(size: u64, secs: u64) -> Facts {
        Facts { is_dir: false, size, modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)) }
    }

    fn dir() -> Facts {
        Facts { is_dir: true, size: 0, modified: None }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(file(1, 0), file(2, 0)), ConflictKind::File);
        assert_eq!(kind_of(dir(), dir()), ConflictKind::Folder);
        assert_eq!(kind_of(file(1, 0), dir()), ConflictKind::Mismatch);
        assert_eq!(kind_of(dir(), file(1, 0)), ConflictKind::Mismatch);
    }

    #[test]
    fn identical_allows_fat_time_steps() {
        assert!(identical(file(10, 100), file(10, 101)));
        assert!(!identical(file(10, 100), file(10, 103)));
        assert!(!identical(file(10, 100), file(11, 100)));
        assert!(!identical(Facts { modified: None, ..file(10, 0) }, file(10, 0)));
        assert!(!identical(dir(), dir()));
    }

    #[test]
    fn newer_side() {
        assert_eq!(source_newer(file(1, 200), file(1, 100)), Some(true));
        assert_eq!(source_newer(file(1, 100), file(1, 200)), Some(false));
        assert_eq!(source_newer(file(1, 100), file(1, 101)), None);
        assert_eq!(source_newer(Facts { modified: None, ..file(1, 0) }, file(1, 0)), None);
    }

    #[test]
    fn default_never_replaces() {
        assert_eq!(default_decision(ConflictKind::File, file(1, 2), file(1, 1)), Decision::Skip);
        assert_eq!(default_decision(ConflictKind::Mismatch, file(1, 0), dir()), Decision::Skip);
        assert_eq!(default_decision(ConflictKind::Folder, dir(), dir()), Decision::Merge);
        assert!(!choices(ConflictKind::Mismatch).contains(&Decision::Replace));
    }

    #[test]
    fn resolutions() {
        let (new, old) = (file(1, 200), file(1, 100));
        assert_eq!(resolve(Decision::Replace, ConflictKind::File, old, new), Resolution::Replace);
        assert_eq!(resolve(Decision::IfNewer, ConflictKind::File, new, old), Resolution::Replace);
        assert_eq!(resolve(Decision::IfNewer, ConflictKind::File, old, new), Resolution::Skip);
        assert_eq!(resolve(Decision::KeepBoth, ConflictKind::Mismatch, old, dir()), Resolution::Rename);
        assert_eq!(resolve(Decision::Replace, ConflictKind::Mismatch, old, dir()), Resolution::Skip);
        assert_eq!(resolve(Decision::Skip, ConflictKind::Folder, dir(), dir()), Resolution::Write);
        assert_eq!(Decision::KeepBoth.label(), "Keep both");
    }
}
```

- [ ] **Step 3: `paths.rs`'i testleriyle yaz**

```rust
//! Comparing paths the way the file system does, and which drives a job touches.

use std::path::{Component, Path};

/// Windows and macOS file systems ignore case by default.
const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

fn parts(path: &Path) -> Vec<String> {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| {
            let text = c.as_os_str().to_string_lossy();
            if IGNORE_CASE { text.to_lowercase() } else { text.into_owned() }
        })
        .collect()
}

/// Whether `a` and `b` name the same entry (case-insensitive on Windows and macOS; a trailing
/// separator does not matter).
pub fn same_path(a: &Path, b: &Path) -> bool {
    parts(a) == parts(b)
}

/// Whether `path` is `dir` itself or inside it.
pub fn is_within(path: &Path, dir: &Path) -> bool {
    let (path, dir) = (parts(path), parts(dir));
    path.len() >= dir.len() && path[..dir.len()] == dir[..]
}

/// The drives (volumes) a job touches, by id. Two jobs that share a drive wait for each other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveSet(Vec<String>);

impl DriveSet {
    pub fn new(ids: impl IntoIterator<Item = String>) -> DriveSet {
        let mut ids: Vec<String> = ids.into_iter().collect();
        ids.sort();
        ids.dedup();
        DriveSet(ids)
    }

    pub fn intersects(&self, other: &DriveSet) -> bool {
        self.0.iter().any(|id| other.0.contains(id))
    }

    pub fn ids(&self) -> &[String] {
        &self.0
    }
}

/// The items not inside another item's path: trashing a created folder covers what is in it.
pub fn cover<T>(mut items: Vec<T>, path: impl Fn(&T) -> &Path) -> Vec<T> {
    // A parent sorts before what is inside it.
    items.sort_by_cached_key(|item| parts(path(item)));
    let mut kept: Vec<T> = Vec::new();
    for item in items {
        let inside = kept.last().is_some_and(|last| is_within(path(&item), path(last)));
        if !inside {
            kept.push(item);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn within_compares_whole_parts() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b"), Path::new("/a/b/")));
        assert!(!is_within(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a"), Path::new("/a/b")));
        assert!(same_path(Path::new("/a/./b"), Path::new("/a/b/")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_ignores_case_and_separators() {
        assert!(same_path(Path::new(r"C:\Users\A"), Path::new("c:/users/a/")));
        assert!(is_within(Path::new(r"C:\Data\Sub"), Path::new(r"c:\data")));
    }

    #[test]
    fn drive_sets_meet_on_a_shared_drive() {
        let c = DriveSet::new(["c".to_owned()]);
        let cd = DriveSet::new(["d".to_owned(), "c".to_owned(), "c".to_owned()]);
        let e = DriveSet::new(["e".to_owned()]);
        assert!(c.intersects(&cd) && cd.intersects(&c));
        assert!(!c.intersects(&e));
        assert_eq!(cd.ids(), ["c", "d"]);
    }

    #[test]
    fn cover_keeps_only_the_outermost() {
        let items = vec![PathBuf::from("/x/a/b"), PathBuf::from("/x/a"), PathBuf::from("/x/c"), PathBuf::from("/x/ab")];
        let kept = cover(items, |p| p.as_path());
        assert_eq!(kept, [PathBuf::from("/x/a"), PathBuf::from("/x/ab"), PathBuf::from("/x/c")]);
    }
}
```

(Windows'ta `Path::components` `/` ve `\`'yi zaten aynı ayırıcı sayar ve sürücü önekini `Prefix` olarak verir; `to_lowercase` `C:`/`c:` farkını da kapatır.)

- [ ] **Step 4: `rate.rs`'i testleriyle yaz**

```rust
//! Copy speed over the last few seconds and the time left.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Bytes per second, averaged over a sliding window of samples of the running total.
pub struct Rate {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl Rate {
    pub fn new(window: Duration) -> Rate {
        Rate { window, samples: VecDeque::new() }
    }

    /// `done` bytes in total at `now`.
    pub fn record(&mut self, now: Instant, done: u64) {
        self.samples.push_back((now, done));
        // Keep one sample older than the window, so the span covers all of it.
        while self.samples.len() > 2 && now.saturating_duration_since(self.samples[1].0) > self.window {
            self.samples.pop_front();
        }
    }

    /// `None` until the samples span half a second.
    pub fn per_second(&self) -> Option<f64> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        let span = last.0.saturating_duration_since(first.0).as_secs_f64();
        (span >= 0.5).then(|| last.1.saturating_sub(first.1) as f64 / span)
    }

    /// The time `left` more bytes take at the current speed.
    pub fn remaining(&self, left: u64) -> Option<Duration> {
        let speed = self.per_second().filter(|s| *s > 0.0)?;
        Some(Duration::from_secs_f64((left as f64 / speed).min(360_000.0)))
    }
}

/// `~0:42`, `~12:05`, `~1:05:10`.
pub fn format_eta(left: Duration) -> String {
    let secs = left.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 { format!("~{h}:{m:02}:{s:02}") } else { format!("~{m}:{s:02}") }
}

/// `84 MB/s`.
pub fn format_rate(per_second: f64) -> String {
    format!("{}/s", crate::format_size(per_second.max(0.0) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_over_the_window() {
        let t0 = Instant::now();
        let mut rate = Rate::new(Duration::from_secs(5));
        rate.record(t0, 0);
        assert_eq!(rate.per_second(), None, "one sample");
        rate.record(t0 + Duration::from_secs(1), 1000);
        rate.record(t0 + Duration::from_secs(2), 2000);
        assert_eq!(rate.per_second(), Some(1000.0));
        assert_eq!(rate.remaining(5000), Some(Duration::from_secs(5)));
        // Old samples fall out: the speed follows the last few seconds.
        for i in 3..20 {
            rate.record(t0 + Duration::from_secs(i), 2000 + (i - 2) * 100);
        }
        let speed = rate.per_second().unwrap();
        assert!((speed - 100.0).abs() < 1.0, "{speed}");
    }

    #[test]
    fn nothing_moving_has_no_eta() {
        let t0 = Instant::now();
        let mut rate = Rate::new(Duration::from_secs(5));
        rate.record(t0, 10);
        rate.record(t0 + Duration::from_secs(2), 10);
        assert_eq!(rate.remaining(100), None);
    }

    #[test]
    fn formats() {
        assert_eq!(format_eta(Duration::from_secs(42)), "~0:42");
        assert_eq!(format_eta(Duration::from_secs(725)), "~12:05");
        assert_eq!(format_eta(Duration::from_secs(3910)), "~1:05:10");
        assert!(format_rate(84.0 * 1024.0 * 1024.0).ends_with("/s"));
    }
}
```

- [ ] **Step 5: `history.rs`'i testleriyle yaz**

```rust
//! Undo and redo stacks.

use std::collections::VecDeque;

/// What can be undone and redone, at most `cap` deep (the oldest falls off).
pub struct UndoStack<T> {
    undo: VecDeque<T>,
    redo: Vec<T>,
    cap: usize,
}

impl<T> UndoStack<T> {
    pub fn new(cap: usize) -> UndoStack<T> {
        UndoStack { undo: VecDeque::new(), redo: Vec::new(), cap: cap.max(1) }
    }

    /// A new action: it can be undone; what was undone before can no longer be redone.
    pub fn push_new(&mut self, item: T) {
        self.redo.clear();
        self.push_undo(item);
    }

    /// The result of an undo: it can be redone.
    pub fn push_undone(&mut self, item: T) {
        self.redo.push(item);
    }

    /// The result of a redo: back on the undo stack; the rest stays redoable.
    pub fn push_redone(&mut self, item: T) {
        self.push_undo(item);
    }

    pub fn pop_undo(&mut self) -> Option<T> {
        self.undo.pop_back()
    }

    pub fn pop_redo(&mut self) -> Option<T> {
        self.redo.pop()
    }

    pub fn peek_undo(&self) -> Option<&T> {
        self.undo.back()
    }

    pub fn peek_redo(&self) -> Option<&T> {
        self.redo.last()
    }

    fn push_undo(&mut self, item: T) {
        self.undo.push_back(item);
        if self.undo.len() > self.cap {
            self.undo.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_then_redo() {
        let mut s = UndoStack::new(10);
        s.push_new("a");
        s.push_new("b");
        assert_eq!(s.pop_undo(), Some("b"));
        s.push_undone("b'");
        assert_eq!(s.peek_undo(), Some(&"a"));
        assert_eq!(s.pop_redo(), Some("b'"));
        s.push_redone("b''");
        assert_eq!(s.peek_undo(), Some(&"b''"));
        assert_eq!(s.peek_redo(), None);
    }

    #[test]
    fn a_new_action_drops_the_redo_stack() {
        let mut s = UndoStack::new(10);
        s.push_new(1);
        let undone = s.pop_undo().unwrap();
        s.push_undone(undone);
        s.push_new(2);
        assert_eq!(s.pop_redo(), None);
    }

    #[test]
    fn the_oldest_falls_off() {
        let mut s = UndoStack::new(2);
        for i in 0..5 {
            s.push_new(i);
        }
        assert_eq!((s.pop_undo(), s.pop_undo(), s.pop_undo()), (Some(4), Some(3), None));
    }
}
```

- [ ] **Step 6: `threads.rs`'i testleriyle yaz**

```rust
//! How many files a job copies or deletes at once.

/// What kind of disk a drive is; parallel work helps SSDs and hurts spinning disks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskKind {
    Ssd,
    Hdd,
    Network,
    Unknown,
}

/// `copy-threads` in settings.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyThreads {
    #[default]
    Auto,
    /// 1–16.
    Fixed(u8),
}

/// Allowed `copy-threads` numbers.
pub const COPY_THREADS_RANGE: std::ops::RangeInclusive<u8> = 1..=16;

/// Workers for a job touching drives of `kinds`: a fixed setting wins; else the slowest kind
/// decides (SSD 6, network 4, spinning or unknown 1).
pub fn workers(threads: CopyThreads, kinds: &[DiskKind]) -> usize {
    match threads {
        CopyThreads::Fixed(n) => usize::from(n.clamp(*COPY_THREADS_RANGE.start(), *COPY_THREADS_RANGE.end())),
        CopyThreads::Auto => kinds
            .iter()
            .map(|kind| match kind {
                DiskKind::Ssd => 6,
                DiskKind::Network => 4,
                DiskKind::Hdd | DiskKind::Unknown => 1,
            })
            .min()
            .unwrap_or(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slowest_disk_decides() {
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Ssd]), 6);
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Ssd, DiskKind::Network]), 4);
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Ssd, DiskKind::Hdd]), 1);
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Unknown]), 1);
        assert_eq!(workers(CopyThreads::Auto, &[]), 1);
    }

    #[test]
    fn a_fixed_number_wins_within_bounds() {
        assert_eq!(workers(CopyThreads::Fixed(3), &[DiskKind::Hdd]), 3);
        assert_eq!(workers(CopyThreads::Fixed(0), &[]), 1);
        assert_eq!(workers(CopyThreads::Fixed(99), &[]), 16);
    }
}
```

- [ ] **Step 7: Testleri ve tüm kontrolleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-core ops`
Expected: names, conflict, paths, rate, history, threads testleri PASS (`windows_ignores_case_and_separators` yalnız Windows'ta derlenir).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 8: Commit**

```bash
git add crates/gezik-core/src/ops
git commit -m "Conflict decisions, path rules, speed, undo stack and worker counts for file operations"
```

---

### Task 3: Ayarlar ve kısayollar: `[files]`, panel durumu, 11 yeni eylem

**Files:**
- Modify: `crates/gezik-config/src/settings.rs`
- Modify: `crates/gezik-config/src/shortcuts.rs`
- Modify: `crates/gezik-config/templates/settings.toml`
- Modify: `crates/gezik/src/keys.rs` (test `every_default_is_reachable_on_windows_and_linux`)
- Modify: `crates/gezik/src/main.rs` (`handle_key` içindeki `match action` — yeni eylemler şimdilik hiçbir şey yapmaz)

**Interfaces:**
- Consumes: `gezik_core::ops::threads::{CopyThreads, COPY_THREADS_RANGE}` (Task 2)
- Produces:
  - `gezik_config::settings::FilesSettings { confirm_trash: bool, copy_threads: CopyThreads }` (`Default`: false, Auto); `Settings.files: FilesSettings`
  - `State.operations_collapsed: bool` (`[operations] panel-collapsed`)
  - `Action::{Copy, Cut, Paste, PasteMove, Trash, DeletePermanently, Rename, NewFolder, Duplicate, Undo, Redo}`; `Action::ALL: [Action; 25]`
  - `Action::default_text(self, Platform) -> Option<&'static str>` (özel; varsayılanı olmayan eylem bağlanmaz)

- [ ] **Step 1: Başarısız testleri yaz**

`crates/gezik-config/src/settings.rs` test modülünün sonuna ekle:

```rust
    #[test]
    fn reads_the_files_table() {
        use gezik_core::ops::threads::CopyThreads;
        let (settings, warnings) = parse("[files]\nconfirm-trash = true\ncopy-threads = 3\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.files, FilesSettings { confirm_trash: true, copy_threads: CopyThreads::Fixed(3) });
        let (settings, _) = parse("[files]\ncopy-threads = \"Auto\"\n");
        assert_eq!(settings.files.copy_threads, CopyThreads::Auto);
        assert_eq!(Settings::default().files, FilesSettings::default());
    }

    #[test]
    fn bad_files_values_keep_defaults_with_warnings() {
        let (settings, warnings) = parse("[files]\nconfirm-trash = \"yes\"\ncopy-threads = 40\n");
        assert_eq!(settings.files, FilesSettings::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].starts_with("files.confirm-trash:"));
        assert!(messages[1].starts_with("files.copy-threads:") && messages[1].contains("1 to 16"));
        let (_, warnings) = parse("files = 1\n");
        assert!(warnings[0].message.starts_with("files: expected a table"));
    }

    #[test]
    fn operations_panel_state_round_trips() {
        let state = State { operations_collapsed: true, ..State::default() };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("operations"), "the default is not written");
    }
```

`crates/gezik-config/src/shortcuts.rs` test modülünün sonuna ekle:

```rust
    #[test]
    fn file_operation_defaults() {
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_for(&chord("delete")), Some(Action::Trash));
        assert_eq!(other.action_for(&chord("shift+delete")), Some(Action::DeletePermanently));
        assert_eq!(other.action_for(&chord("f2")), Some(Action::Rename));
        assert_eq!(other.action_for(&chord("ctrl+shift+n")), Some(Action::NewFolder));
        assert_eq!(other.action_for(&chord("ctrl+y")), Some(Action::Redo));
        assert_eq!(other.action_for(&chord("ctrl+d")), None, "Ctrl+D deletes in Explorer: no duplicate");
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(mac.action_for(&mac_chord("mod+d")), Some(Action::Duplicate));
        assert_eq!(mac.action_for(&mac_chord("mod+backspace")), Some(Action::Trash));
        assert_eq!(mac.action_for(&mac_chord("enter")), Some(Action::Rename));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+z")), Some(Action::Redo));
    }

    #[test]
    fn an_action_without_a_default_can_be_bound() {
        let (s, warnings) = build("[shortcuts]\nduplicate = \"ctrl+d\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+d")), Some(Action::Duplicate));
    }
```

Mevcut `defaults_cover_every_action` testini şununla değiştir:

```rust
    #[test]
    fn defaults_cover_every_action() {
        for platform in [Platform::Mac, Platform::Other] {
            let s = Shortcuts::defaults(platform);
            for action in Action::ALL {
                let Some(text) = action.default_text(platform) else { continue };
                let c = parse_chord(text, platform).unwrap().unwrap();
                assert_eq!(s.action_for(&c), Some(action), "{platform:?} {}", action.name());
            }
        }
    }
```

- [ ] **Step 2: Derlenmediğini gör**

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: FAIL — `FilesSettings`, `operations_collapsed`, `Action::Trash` … yok.

- [ ] **Step 3: `settings.rs`'i uygula**

En üstteki `use` satırlarına ekle:

```rust
use gezik_core::ops::threads::{COPY_THREADS_RANGE, CopyThreads};
```

`ViewDefaults`'tan sonra ekle:

```rust
/// `[files]`: file operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FilesSettings {
    /// Ask before moving to the trash.
    pub confirm_trash: bool,
    pub copy_threads: CopyThreads,
}
```

`Settings` yapısına `pub view: ViewDefaults,` satırından sonra ekle:

```rust
    pub files: FilesSettings,
```

`Default for Settings` içinde `view: ViewDefaults::default(),` satırından sonra ekle:

```rust
            files: FilesSettings::default(),
```

`Settings::parse` içinde `match table.get("view") { … }` bloğundan sonra, `settings` döndürülmeden önce ekle:

```rust
        match table.get("files") {
            None => {}
            Some(value) => match value.as_table() {
                Some(files) => settings.files = parse_files(files, file, warnings),
                None => warnings.push(Warning::new(file, format!("files: expected a table, got {value}"))),
            },
        }
```

`parse_view` fonksiyonundan sonra ekle:

```rust
fn parse_files(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> FilesSettings {
    let mut out = FilesSettings::default();
    if let Some(value) = table.get("confirm-trash") {
        match value.as_bool() {
            Some(on) => out.confirm_trash = on,
            None => {
                warnings.push(Warning::new(file, format!("files.confirm-trash: expected true or false, got {value}")))
            }
        }
    }
    if let Some(value) = table.get("copy-threads") {
        let parsed = match value {
            toml::Value::String(text) if text.eq_ignore_ascii_case("auto") => Some(CopyThreads::Auto),
            toml::Value::Integer(n) => {
                u8::try_from(*n).ok().filter(|n| COPY_THREADS_RANGE.contains(n)).map(CopyThreads::Fixed)
            }
            _ => None,
        };
        match parsed {
            Some(threads) => out.copy_threads = threads,
            None => warnings.push(Warning::new(
                file,
                format!(
                    "files.copy-threads: expected \"auto\" or a number from {} to {}, got {value}",
                    COPY_THREADS_RANGE.start(),
                    COPY_THREADS_RANGE.end()
                ),
            )),
        }
    }
    out
}
```

`State` yapısına `pub preview_width: Option<u32>,` satırından sonra ekle:

```rust
    /// The operations panel is folded into the status bar.
    pub operations_collapsed: bool,
```

`State::parse` içinde `State { … }` ifadesinden önce ekle ve ifadeye alanı ekle:

```rust
        let operations_collapsed = table
            .get("operations")
            .and_then(|v| v.as_table())
            .and_then(|o| o.get("panel-collapsed"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        State { window, sidebar_width, columns, preview_open, preview_width, operations_collapsed }
```

`State::to_toml` içinde `root.to_string()` satırından önce ekle:

```rust
        if self.operations_collapsed {
            let mut operations = toml::Table::new();
            operations.insert("panel-collapsed".into(), toml::Value::Boolean(true));
            root.insert("operations".into(), toml::Value::Table(operations));
        }
```

- [ ] **Step 4: `shortcuts.rs`'i uygula**

`Action` enum'una `QuickLook,` satırından sonra ekle:

```rust
    Copy,
    Cut,
    Paste,
    /// Paste what was copied as a move (macOS ⌘⌥V, as in Finder).
    PasteMove,
    Trash,
    DeletePermanently,
    Rename,
    NewFolder,
    Duplicate,
    Undo,
    Redo,
```

`Action::ALL`'u 25 öğeye çıkar (`QuickLook`'tan sonra aynı sırayla yeni on bir eylem):

```rust
    pub const ALL: [Action; 25] = [
        Action::NewTab,
        Action::CloseTab,
        Action::NextTab,
        Action::PrevTab,
        Action::Back,
        Action::Forward,
        Action::Up,
        Action::FocusPath,
        Action::Refresh,
        Action::SelectAll,
        Action::ViewList,
        Action::ViewGrid,
        Action::TogglePreview,
        Action::QuickLook,
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::PasteMove,
        Action::Trash,
        Action::DeletePermanently,
        Action::Rename,
        Action::NewFolder,
        Action::Duplicate,
        Action::Undo,
        Action::Redo,
    ];
```

`name()` içine `Action::QuickLook => "quick-look",` satırından sonra ekle:

```rust
            Action::Copy => "copy",
            Action::Cut => "cut",
            Action::Paste => "paste",
            Action::PasteMove => "paste-move",
            Action::Trash => "trash",
            Action::DeletePermanently => "delete-permanently",
            Action::Rename => "rename",
            Action::NewFolder => "new-folder",
            Action::Duplicate => "duplicate",
            Action::Undo => "undo",
            Action::Redo => "redo",
```

`default_text`'i `Option` döndürecek şekilde değiştir (gövdenin tamamı):

```rust
    /// The default chord, if the action has one on `platform`.
    fn default_text(self, platform: Platform) -> Option<&'static str> {
        Some(match (self, platform) {
            (Action::NewTab, _) => "mod+t",
            (Action::CloseTab, _) => "mod+w",
            (Action::NextTab, _) => "ctrl+tab",
            (Action::PrevTab, _) => "ctrl+shift+tab",
            (Action::Back, Platform::Mac) => "mod+[",
            (Action::Back, Platform::Other) => "alt+left",
            (Action::Forward, Platform::Mac) => "mod+]",
            (Action::Forward, Platform::Other) => "alt+right",
            (Action::Up, Platform::Mac) => "mod+up",
            (Action::Up, Platform::Other) => "alt+up",
            (Action::FocusPath, _) => "mod+l",
            (Action::Refresh, Platform::Mac) => "mod+r",
            (Action::Refresh, Platform::Other) => "f5",
            (Action::SelectAll, _) => "mod+a",
            (Action::ViewList, _) => "mod+1",
            (Action::ViewGrid, _) => "mod+2",
            (Action::TogglePreview, _) => "alt+p",
            (Action::QuickLook, _) => "space",
            (Action::Copy, _) => "mod+c",
            (Action::Cut, _) => "mod+x",
            (Action::Paste, _) => "mod+v",
            (Action::PasteMove, Platform::Mac) => "mod+alt+v",
            (Action::PasteMove, Platform::Other) => return None,
            (Action::Trash, Platform::Mac) => "mod+backspace",
            (Action::Trash, Platform::Other) => "delete",
            (Action::DeletePermanently, Platform::Mac) => "mod+alt+backspace",
            (Action::DeletePermanently, Platform::Other) => "shift+delete",
            (Action::Rename, Platform::Mac) => "enter",
            (Action::Rename, Platform::Other) => "f2",
            (Action::NewFolder, _) => "mod+shift+n",
            // Ctrl+D deletes in Explorer: Windows and Linux users get no surprise copies.
            (Action::Duplicate, Platform::Mac) => "mod+d",
            (Action::Duplicate, Platform::Other) => return None,
            (Action::Undo, _) => "mod+z",
            (Action::Redo, Platform::Mac) => "mod+shift+z",
            (Action::Redo, Platform::Other) => "mod+y",
        })
    }
```

`from_table` içindeki varsayılan döngüsünde

```rust
            let text = action.default_text(platform);
```

satırını şununla değiştir:

```rust
            let Some(text) = action.default_text(platform) else { continue };
```

- [ ] **Step 5: Şablonu güncelle**

`crates/gezik-config/templates/settings.toml` içinde `[view]` bölümünden sonra, `[shortcuts]`'tan önce ekle:

```toml
[files]
# Ask before moving to the trash (Delete).
confirm-trash = false
# Files copied or deleted at once: "auto" (SSD 6, spinning disk 1, network 4) or 1-16.
copy-threads = "auto"

```

`[shortcuts]` bölümünün sonuna (`# quick-look = …` satırından sonra) ekle:

```toml
# copy = "mod+c"
# cut = "mod+x"
# paste = "mod+v"
# paste-move = ""          # macOS: "mod+alt+v" (paste what was copied as a move)
# trash = "delete"         # macOS: "mod+backspace"
# delete-permanently = "shift+delete"   # macOS: "mod+alt+backspace"
# rename = "f2"            # macOS: "enter"
# new-folder = "mod+shift+n"
# duplicate = ""           # macOS: "mod+d"
# undo = "mod+z"
# redo = "mod+y"           # macOS: "mod+shift+z"
```

- [ ] **Step 6: Uygulamadaki kullanımı derlenir hale getir**

`crates/gezik/src/main.rs` içindeki `handle_key`'de `match action { … }` bloğunun son kolu `Action::QuickLook => preview.toggle_quick_look(),`'tan sonra ekle (Task 13 bunları gerçek komutlara bağlar):

```rust
                Action::Copy
                | Action::Cut
                | Action::Paste
                | Action::PasteMove
                | Action::Trash
                | Action::DeletePermanently
                | Action::Rename
                | Action::NewFolder
                | Action::Duplicate
                | Action::Undo
                | Action::Redo => return false,
```

`crates/gezik/src/keys.rs` testindeki `other_event`'te `Key::F(n)` kolunu ve isimli tuşları genişlet:

```rust
            Key::F(n) => text(match n {
                2 => SlintKey::F2,
                5 => SlintKey::F5,
                other => panic!("no default uses f{other}"),
            }),
            Key::Delete => text(SlintKey::Delete),
```

ve `every_default_is_reachable_on_windows_and_linux` içindeki `match action`'ı genişlet (varsayılanı olmayanlar atlanır):

```rust
            let text = match action {
                Action::NewTab => "ctrl+t",
                Action::CloseTab => "ctrl+w",
                Action::NextTab => "ctrl+tab",
                Action::PrevTab => "ctrl+shift+tab",
                Action::Back => "alt+left",
                Action::Forward => "alt+right",
                Action::Up => "alt+up",
                Action::FocusPath => "ctrl+l",
                Action::Refresh => "f5",
                Action::SelectAll => "ctrl+a",
                Action::ViewList => "ctrl+1",
                Action::ViewGrid => "ctrl+2",
                Action::TogglePreview => "alt+p",
                Action::QuickLook => "space",
                Action::Copy => "ctrl+c",
                Action::Cut => "ctrl+x",
                Action::Paste => "ctrl+v",
                Action::Trash => "delete",
                Action::DeletePermanently => "shift+delete",
                Action::Rename => "f2",
                Action::NewFolder => "ctrl+shift+n",
                Action::Undo => "ctrl+z",
                Action::Redo => "ctrl+y",
                Action::PasteMove | Action::Duplicate => continue,
            };
```

Ctrl+Shift+N, Slint'ten `"N"` metni, `control` ve `shift` ile gelir; `other_event` bunu `Key::Char('n')` için `"n"` gönderir — `chord_from_event` harfi küçültüp `shift`'i ayrıca taşıdığı için sonuç aynıdır.

- [ ] **Step 7: Testleri ve kontrolleri çalıştır**

Run: `~/.cargo/bin/cargo test --workspace`
Expected: PASS (yeni 5 test dahil; `starter_files_are_valid` yeni şablonu uyarısız okur).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 8: Commit**

```bash
git add crates/gezik-config crates/gezik/src/keys.rs crates/gezik/src/main.rs
git commit -m "Settings for file operations and shortcuts for copy, paste, trash, rename, undo"
```

---

### Task 4: Tema: ilerleme renkleri

**Files:**
- Modify: `crates/gezik-config/src/theme.rs`
- Modify: `crates/gezik-config/themes/dark.toml`, `crates/gezik-config/themes/light.toml`
- Modify: `crates/gezik-config/templates/example.toml`
- Modify: `crates/gezik/ui/theme.slint`
- Modify: `crates/gezik/src/theme_bridge.rs`

**Interfaces:**
- Consumes: —
- Produces: `ThemeColors.{progress, progress_paused, progress_error}`; Slint `Theme.progress`, `Theme.progress-paused`, `Theme.progress-error`

- [ ] **Step 1: Başarısız testi yaz**

`crates/gezik-config/src/theme.rs` test modülüne ekle:

```rust
    #[test]
    fn progress_colors_are_read() {
        let (theme, warnings) = parse("[colors]\nprogress = \"#112233\"\nprogress-paused = \"#445566\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        let theme = theme.unwrap();
        assert!(theme.colors.iter().any(|(k, _)| *k == "progress"));
        assert!(theme.colors.iter().any(|(k, _)| *k == "progress-paused"));
    }
```

- [ ] **Step 2: Başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config progress_colors_are_read`
Expected: FAIL (`progress` bilinmeyen anahtar olarak sessizce atlanır).

- [ ] **Step 3: Uygula**

`theme.rs`: `COLOR_KEYS`'i 24'e çıkar, `"danger"`'dan sonra:

```rust
pub const COLOR_KEYS: [&str; 24] = [
    "background",
    "surface",
    "foreground",
    "foreground-muted",
    "border",
    "accent",
    "accent-foreground",
    "selection",
    "selection-foreground",
    "hover",
    "icon-folder",
    "icon-image",
    "icon-video",
    "icon-audio",
    "icon-archive",
    "icon-document",
    "icon-code",
    "icon-other",
    "focus-ring",
    "marquee",
    "danger",
    "progress",
    "progress-paused",
    "progress-error",
];
```

`ThemeColors`'a `pub danger: Color,`'dan sonra:

```rust
    /// Progress bars of running file operations.
    pub progress: Color,
    /// A paused operation (waiting for decisions, a full disk, the user).
    pub progress_paused: Color,
    pub progress_error: Color,
```

`ThemeColors::set` içine `"danger" => &mut self.danger,`'dan sonra:

```rust
            "progress" => &mut self.progress,
            "progress-paused" => &mut self.progress_paused,
            "progress-error" => &mut self.progress_error,
```

`themes/dark.toml` `[colors]` sonuna (`danger`'dan sonra):

```toml
progress = "#60cdff"
progress-paused = "#e8b33f"
progress-error = "#ff6b6b"
```

`themes/light.toml` `[colors]` sonuna:

```toml
progress = "#005fb8"
progress-paused = "#9d5d00"
progress-error = "#c42b1c"
```

`templates/example.toml` içinde `marquee` satırından sonra (yorum biçimini koru):

```toml
progress = "#88c0d0"     # running file operations
progress-paused = "#ebcb8b"
progress-error = "#bf616a"
```

`crates/gezik/ui/theme.slint` içinde `in property <color> danger;`'dan sonra:

```slint
    in property <color> progress;
    in property <color> progress-paused;
    in property <color> progress-error;
```

`crates/gezik/src/theme_bridge.rs` `apply_global` içinde `global.set_danger(...)`'dan sonra:

```rust
    global.set_progress(color(c.progress));
    global.set_progress_paused(color(c.progress_paused));
    global.set_progress_error(color(c.progress_error));
```

- [ ] **Step 4: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test --workspace`
Expected: PASS (`builtin_themes_define_every_value` iki yerleşik temada 24 rengi bulur; `starter_files_are_valid` örnek temayı uyarısız okur).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 5: Commit**

```bash
git add crates/gezik-config crates/gezik/ui/theme.slint crates/gezik/src/theme_bridge.rs
git commit -m "Theme colors for operation progress: running, paused, failed"
```

---

### Task 5: Platform: ilerlemeli kopya, ezmeyen taşıma, silme, sürücü bilgisi (`gezik-platform/src/fs`)

**Files:**
- Modify: `crates/gezik-platform/Cargo.toml` (windows özellikleri)
- Modify: `crates/gezik-platform/src/lib.rs` (`pub mod fs;`)
- Create: `crates/gezik-platform/src/fs/mod.rs`
- Create: `crates/gezik-platform/src/fs/windows.rs`
- Create: `crates/gezik-platform/src/fs/unix.rs`

**Interfaces:**
- Consumes: `gezik_core::ops::threads::DiskKind` (Task 2)
- Produces (`gezik_platform::fs`):
  - `pub struct DriveFacts { pub id: String, pub kind: DiskKind, pub trash: bool }` (`Debug, Clone, PartialEq, Eq`)
  - `pub const BIG_FILE: u64` (256 MB)
  - `copy_file(from: &Path, to: &Path, size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()>` — `to` var ise `AlreadyExists`; `progress` false dönerse `ErrorKind::Interrupted` ve `to` silinmiş olur; sembolik bağlantı bağlantı olarak kopyalanır
  - `move_entry(from: &Path, to: &Path) -> io::Result<()>` — aynı diskte yeniden adlandırma; `to` varsa `AlreadyExists`; başka diskteyse hata
  - `delete(path: &Path) -> io::Result<()>` — dosya, bağlantı veya boş klasör; salt okunur dosya da
  - `drive_facts(path: &Path) -> io::Result<DriveFacts>`, `drive_root(path: &Path) -> Option<PathBuf>`
  - `set_hidden(path: &Path) -> io::Result<()>` (Unix'te bir şey yapmaz)
  - `is_disk_full(err: &io::Error) -> bool`, `nearest_existing(path: &Path) -> Option<PathBuf>`

- [ ] **Step 1: Cargo özelliklerini ekle**

`crates/gezik-platform/Cargo.toml` içindeki Windows `features` listesini şu hale getir (alfabetik):

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = [
    "Win32_Foundation",
    "Win32_Globalization",
    "Win32_Graphics_Gdi",
    "Win32_Security",
    "Win32_Storage_FileSystem",
    "Win32_System_Com",
    "Win32_System_IO",
    "Win32_System_Ioctl",
    "Win32_System_Time",
    "Win32_UI_Controls",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_Shell",
    "Win32_UI_Shell_Common",
    "Win32_UI_WindowsAndMessaging",
] }
```

`crates/gezik-platform/src/lib.rs` içinde `mod datetime;` satırından önce ekle:

```rust
pub mod fs;
```

- [ ] **Step 2: Ortak modülü ve testlerini yaz**

`crates/gezik-platform/src/fs/mod.rs`:

```rust
//! What the system does best for file operations: copying with progress, deleting, moving
//! without replacing, and what kind of drive a path is on.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::io;
use std::path::{Path, PathBuf};

pub use gezik_core::ops::threads::DiskKind;
#[cfg(unix)]
pub use unix::{copy_file, delete, drive_facts, drive_root, move_entry, set_hidden};
#[cfg(windows)]
pub use windows::{copy_file, delete, drive_facts, drive_root, move_entry, set_hidden};

/// What the engine needs to know about the drive a path is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveFacts {
    /// Same id, same drive: a volume serial number, a device number or a network share.
    pub id: String,
    pub kind: DiskKind,
    /// Whether deleting can go to the trash (Recycle Bin).
    pub trash: bool,
}

/// Files at least this big are copied past the system file cache, so a large copy does not
/// push everything else out of it.
pub const BIG_FILE: u64 = 256 * 1024 * 1024;

#[cfg(windows)]
const DISK_FULL_CODES: [i32; 2] = [112, 39];
#[cfg(unix)]
const DISK_FULL_CODES: [i32; 1] = [libc::ENOSPC];

/// Whether `err` means the disk is full.
pub fn is_disk_full(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::StorageFull || err.raw_os_error().is_some_and(|code| DISK_FULL_CODES.contains(&code))
}

/// `path` or its nearest ancestor that exists.
pub fn nearest_existing(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| std::fs::symlink_metadata(p).is_ok()).map(Path::to_path_buf)
}

/// The error a cancelled copy returns.
pub(crate) fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gezik-fs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_copies_and_reports_progress() {
        let dir = test_dir("copy");
        let (from, to) = (dir.join("a.bin"), dir.join("b.bin"));
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&from, &data).unwrap();
        let mut seen = Vec::new();
        copy_file(&from, &to, data.len() as u64, &mut |done| {
            seen.push(done);
            true
        })
        .unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), data);
        assert_eq!(seen.last().copied(), Some(data.len() as u64));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "progress only grows");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_never_overwrites() {
        let dir = test_dir("no-overwrite");
        let (from, to) = (dir.join("a.txt"), dir.join("b.txt"));
        std::fs::write(&from, "new").unwrap();
        std::fs::write(&to, "old").unwrap();
        let err = copy_file(&from, &to, 3, &mut |_| true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing() {
        let dir = test_dir("cancel");
        let (from, to) = (dir.join("a.bin"), dir.join("b.bin"));
        std::fs::write(&from, vec![7u8; 4_000_000]).unwrap();
        let err = copy_file(&from, &to, 4_000_000, &mut |_| false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted, "{err}");
        assert!(!to.exists(), "the half-written copy is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_entry_never_overwrites() {
        let dir = test_dir("move");
        let (a, b, c) = (dir.join("a.txt"), dir.join("b.txt"), dir.join("c.txt"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let err = move_entry(&a, &b).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
        move_entry(&a, &c).unwrap();
        assert!(!a.exists() && std::fs::read_to_string(&c).unwrap() == "a");
        let sub = dir.join("sub");
        std::fs::create_dir(&sub).unwrap();
        move_entry(&sub, &dir.join("sub2")).unwrap();
        assert!(dir.join("sub2").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_files_read_only_files_and_empty_folders() {
        let dir = test_dir("delete");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        delete(&file).unwrap();
        assert!(!file.exists());
        let full = dir.join("full");
        std::fs::create_dir(&full).unwrap();
        std::fs::write(full.join("x"), "x").unwrap();
        assert!(delete(&full).is_err(), "a folder with something in it stays");
        std::fs::remove_file(full.join("x")).unwrap();
        delete(&full).unwrap();
        assert!(!full.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drive_facts_name_the_same_drive_alike() {
        let dir = test_dir("drive");
        let a = drive_facts(&dir).unwrap();
        let b = drive_facts(&dir.join("not-yet-there.txt")).unwrap_or_else(|_| a.clone());
        assert!(!a.id.is_empty());
        assert_eq!(a.id, b.id);
        assert!(drive_root(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_full_is_recognized() {
        assert!(is_disk_full(&io::Error::from(io::ErrorKind::StorageFull)));
        assert!(is_disk_full(&io::Error::from_raw_os_error(DISK_FULL_CODES[0])));
        assert!(!is_disk_full(&io::Error::from(io::ErrorKind::NotFound)));
    }

    #[test]
    fn nearest_existing_walks_up() {
        let dir = test_dir("nearest");
        assert_eq!(nearest_existing(&dir.join("a").join("b")), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 3: Windows uygulamasını yaz**

`crates/gezik-platform/src/fs/windows.rs`:

```rust
//! Windows: CopyFileExW with progress, POSIX deletes, volume facts.

use std::ffi::{OsString, c_void};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    COPY_FILE_COPY_SYMLINK, COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, COPYPROGRESSROUTINE_PROGRESS, CopyFileExW,
    CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
    FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfoEx, GetDriveTypeW,
    GetFileAttributesW, GetVolumeInformationW, GetVolumePathNameW, INVALID_FILE_ATTRIBUTES,
    LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVE_FILE_FLAGS, MoveFileExW, OPEN_EXISTING, PROGRESS_CANCEL, PROGRESS_CONTINUE,
    SetFileAttributesW, SetFileInformationByHandle,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
    StorageDeviceSeekPenaltyProperty,
};
use windows::core::HSTRING;

use super::{BIG_FILE, DiskKind, DriveFacts, cancelled};

const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;

/// A Win32 error as an `io::Error` with its code (so `kind()` works).
pub(crate) fn io_error(err: windows::core::Error) -> io::Error {
    let hr = err.code().0 as u32;
    // HRESULT_FROM_WIN32: 0x8007xxxx carries the Win32 error code.
    if hr & 0xFFFF_0000 == 0x8007_0000 { io::Error::from_raw_os_error((hr & 0xFFFF) as i32) } else { io::Error::other(err) }
}

/// `wide` with the `\\?\` prefix (`\\?\UNC\` for shares), so Win32 calls take paths longer
/// than 260 characters. Already prefixed paths stay as they are.
fn verbatim_wide(wide: &[u16]) -> Vec<u16> {
    let text: Vec<u16> = wide.to_vec();
    let starts = |prefix: &str| {
        let prefix: Vec<u16> = prefix.encode_utf16().collect();
        text.starts_with(&prefix)
    };
    if starts(r"\\?\") || starts(r"\\.\") {
        text
    } else if starts(r"\\") {
        let mut out: Vec<u16> = r"\\?\UNC".encode_utf16().collect();
        out.extend_from_slice(&text[1..]);
        out
    } else {
        let mut out: Vec<u16> = r"\\?\".encode_utf16().collect();
        out.extend_from_slice(&text);
        out
    }
}

pub(crate) fn verbatim(path: &Path) -> HSTRING {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let wide: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    HSTRING::from_wide(&verbatim_wide(&wide))
}

pub fn copy_file(from: &Path, to: &Path, size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    struct Context<'a> {
        progress: &'a mut dyn FnMut(u64) -> bool,
        cancelled: bool,
    }

    #[allow(clippy::too_many_arguments, reason = "the CopyFileExW callback signature")]
    unsafe extern "system" fn routine(
        _total: i64,
        transferred: i64,
        _stream_size: i64,
        _stream_transferred: i64,
        _stream: u32,
        _reason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
        _source: HANDLE,
        _target: HANDLE,
        data: *const c_void,
    ) -> COPYPROGRESSROUTINE_PROGRESS {
        // SAFETY: `data` is the `Context` below, alive for the whole CopyFileExW call.
        let context = unsafe { &mut *(data as *mut Context<'_>) };
        if (context.progress)(u64::try_from(transferred).unwrap_or(0)) {
            PROGRESS_CONTINUE
        } else {
            context.cancelled = true;
            PROGRESS_CANCEL
        }
    }

    let mut flags = COPY_FILE_FAIL_IF_EXISTS | COPY_FILE_COPY_SYMLINK;
    if size >= BIG_FILE {
        flags |= COPY_FILE_NO_BUFFERING;
    }
    let mut context = Context { progress, cancelled: false };
    let data = &mut context as *mut Context<'_> as *const c_void;
    // CopyFileExW removes a half-written copy itself when cancelled or failed.
    let result = unsafe { CopyFileExW(&verbatim(from), &verbatim(to), Some(routine), Some(data), None, flags) };
    match result {
        Ok(()) => Ok(()),
        Err(_) if context.cancelled => Err(cancelled()),
        Err(err) => Err(io_error(err)),
    }
}

pub fn move_entry(from: &Path, to: &Path) -> io::Result<()> {
    // No MOVEFILE_REPLACE_EXISTING (an existing target fails) and no MOVEFILE_COPY_ALLOWED
    // (another drive fails): this is a rename, nothing else.
    unsafe { MoveFileExW(&verbatim(from), &verbatim(to), MOVE_FILE_FLAGS(0)) }.map_err(io_error)
}

pub fn delete(path: &Path) -> io::Result<()> {
    let wide = verbatim(path);
    let opened = unsafe {
        CreateFileW(
            &wide,
            DELETE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    };
    if let Ok(handle) = opened {
        // POSIX semantics: the name goes at once, even if another program still has the file
        // open, so the folder can be removed right after.
        let info = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                FILE_DISPOSITION_FLAG_DELETE.0
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
            ),
        };
        let done = unsafe {
            SetFileInformationByHandle(
                handle,
                FileDispositionInfoEx,
                &info as *const FILE_DISPOSITION_INFO_EX as *const c_void,
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        if done.is_ok() {
            return Ok(());
        }
    }
    // FAT, exFAT and many network shares know no POSIX deletes: the old way.
    let attributes = unsafe { GetFileAttributesW(&wide) };
    if attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_READONLY.0 != 0 {
        let _ = unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes & !FILE_ATTRIBUTE_READONLY.0)) };
    }
    let is_dir = attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    if is_dir { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
}

pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    unsafe { GetVolumePathNameW(&HSTRING::from(path.as_os_str()), &mut buffer) }.ok()?;
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(PathBuf::from(OsString::from_wide(&buffer[..end])))
}

pub fn drive_facts(path: &Path) -> io::Result<DriveFacts> {
    let root = drive_root(path).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no drive for this path"))?;
    let root_text = root.to_string_lossy().into_owned();
    let root_wide = HSTRING::from(root.as_os_str());
    let drive_type = unsafe { GetDriveTypeW(&root_wide) };
    if drive_type == DRIVE_REMOTE || root_text.starts_with(r"\\") {
        return Ok(DriveFacts { id: root_text.to_lowercase(), kind: DiskKind::Network, trash: false });
    }
    let mut serial = 0u32;
    unsafe { GetVolumeInformationW(&root_wide, None, Some(&mut serial), None, None, None) }.map_err(io_error)?;
    let kind = match seek_penalty(&root_text) {
        Some(true) => DiskKind::Hdd,
        Some(false) => DiskKind::Ssd,
        None => DiskKind::Unknown,
    };
    // Removable drives (USB sticks) and optical drives have no Recycle Bin.
    Ok(DriveFacts { id: format!("{serial:08x}"), kind, trash: drive_type == DRIVE_FIXED })
}

/// Whether the disk behind drive `root` (`C:\`) has to seek, like a spinning disk does.
/// `None` for folders mounted as drives and when Windows does not say.
fn seek_penalty(root: &str) -> Option<bool> {
    let letter = root.strip_suffix('\\').unwrap_or(root);
    if letter.len() != 2 || !letter.ends_with(':') {
        return None;
    }
    let device = HSTRING::from(format!(r"\\.\{letter}"));
    // No access rights needed: the query only reads properties.
    let handle = unsafe {
        CreateFileW(
            &device,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()?;
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut answer = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
    let mut returned = 0u32;
    let asked = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const STORAGE_PROPERTY_QUERY as *const c_void),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(&mut answer as *mut DEVICE_SEEK_PENALTY_DESCRIPTOR as *mut c_void),
            size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            Some(&mut returned),
            None,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    asked.ok()?;
    Some(answer.IncursSeekPenalty)
}

pub fn set_hidden(path: &Path) -> io::Result<()> {
    let wide = verbatim(path);
    let attributes = unsafe { GetFileAttributesW(&wide) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes | FILE_ATTRIBUTE_HIDDEN.0)) }
        .map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn verbatim_prefixes_local_and_share_paths_once() {
        assert_eq!(verbatim_wide(&wide(r"C:\a\b")), wide(r"\\?\C:\a\b"));
        assert_eq!(verbatim_wide(&wide(r"\\server\share\x")), wide(r"\\?\UNC\server\share\x"));
        assert_eq!(verbatim_wide(&wide(r"\\?\C:\a")), wide(r"\\?\C:\a"));
    }

    #[test]
    fn long_paths_work() {
        let dir = super::super::test_dir("long");
        let mut deep = dir.clone();
        for i in 0..12 {
            deep.push(format!("{i:02}-{}", "x".repeat(24)));
        }
        std::fs::create_dir_all(&deep).unwrap();
        let (from, to) = (deep.join("a.txt"), deep.join("b.txt"));
        assert!(to.as_os_str().len() > 300);
        std::fs::write(&from, "x").unwrap();
        copy_file(&from, &to, 1, &mut |_| true).unwrap();
        delete(&to).unwrap();
        assert!(!to.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_temp_folder_is_on_a_fixed_drive_with_a_recycle_bin() {
        let facts = drive_facts(&std::env::temp_dir()).unwrap();
        assert!(facts.trash, "{facts:?}");
        assert_eq!(facts.id.len(), 8);
    }
}
```

- [ ] **Step 4: Unix uygulamasını yaz**

`crates/gezik-platform/src/fs/unix.rs`:

```rust
//! macOS and Linux: copying (clone, copy_file_range, buffered), moving without replacing,
//! device facts.

use std::fs::{File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use super::{DiskKind, DriveFacts, cancelled, nearest_existing};

pub fn copy_file(from: &Path, to: &Path, size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        // A link stays a link; `symlink` fails if `to` exists.
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
        return Ok(());
    }
    if !progress(0) {
        return Err(cancelled());
    }
    #[cfg(target_os = "macos")]
    if clone_file(from, to).is_ok() {
        return if progress(size) { Ok(()) } else { Err(cancelled()) };
    }
    let mut source = File::open(from)?;
    let mut target = OpenOptions::new().write(true).create_new(true).mode(meta.mode()).open(to)?;
    let copied = copy_contents(&mut source, &mut target, size, progress);
    let copied = copied.and_then(|()| {
        let times = FileTimes::new().set_modified(meta.modified()?).set_accessed(meta.accessed()?);
        target.set_times(times)
    });
    if copied.is_err() {
        drop(target);
        let _ = std::fs::remove_file(to);
    }
    copied
}

/// APFS clones: the copy shares the blocks until either side changes, so it is instant.
#[cfg(target_os = "macos")]
fn clone_file(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let (from, to) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
    if unsafe { libc::clonefile(from.as_ptr(), to.as_ptr(), 0) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(target_os = "linux")]
fn copy_contents(source: &mut File, target: &mut File, size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // Btrfs and XFS clone in one call; other file systems refuse and we copy.
    if unsafe { libc::ioctl(target.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0 {
        return if progress(size) { Ok(()) } else { Err(cancelled()) };
    }
    let mut done = 0u64;
    loop {
        let n = unsafe {
            libc::copy_file_range(
                source.as_raw_fd(),
                std::ptr::null_mut(),
                target.as_raw_fd(),
                std::ptr::null_mut(),
                8 << 20,
                0,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            let unsupported = matches!(err.raw_os_error(), Some(libc::EXDEV | libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP));
            return if done == 0 && unsupported { buffered(source, target, progress) } else { Err(err) };
        }
        if n == 0 {
            return Ok(());
        }
        done += n as u64;
        if !progress(done) {
            return Err(cancelled());
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn copy_contents(source: &mut File, target: &mut File, _size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    buffered(source, target, progress)
}

fn buffered(source: &mut File, target: &mut File, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    let mut buffer = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }
        target.write_all(&buffer[..n])?;
        done += n as u64;
        if !progress(done) {
            return Err(cancelled());
        }
    }
}

pub fn move_entry(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let (c_from, c_to) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
        let done = unsafe {
            libc::renameat2(libc::AT_FDCWD, c_from.as_ptr(), libc::AT_FDCWD, c_to.as_ptr(), libc::RENAME_NOREPLACE)
        };
        if done == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if !matches!(err.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL)) {
            return Err(err);
        }
    }
    // No atomic "do not replace" here: check first (a tiny window remains).
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

pub fn delete(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
}

pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let start = nearest_existing(path)?;
    let dev = std::fs::metadata(&start).ok()?.dev();
    let mut root = start.clone();
    for parent in start.ancestors().skip(1) {
        match std::fs::metadata(parent) {
            Ok(meta) if meta.dev() == dev => root = parent.to_path_buf(),
            _ => break,
        }
    }
    Some(root)
}

pub fn drive_facts(path: &Path) -> io::Result<DriveFacts> {
    let existing = nearest_existing(path).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let dev = std::fs::metadata(&existing)?.dev();
    Ok(DriveFacts { id: format!("{dev:x}"), kind: disk_kind(dev), trash: true })
}

/// glibc's encoding of a device number's major part.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn dev_major(dev: u64) -> u64 {
    ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn dev_minor(dev: u64) -> u64 {
    (dev & 0xff) | ((dev >> 12) & !0xff)
}

#[cfg(target_os = "linux")]
fn disk_kind(dev: u64) -> DiskKind {
    let (major, minor) = (dev_major(dev), dev_minor(dev));
    let base = PathBuf::from(format!("/sys/dev/block/{major}:{minor}"));
    // A partition's own folder has no queue; its disk (the parent) does.
    for candidate in [base.join("queue/rotational"), base.join("../queue/rotational")] {
        if let Ok(text) = std::fs::read_to_string(candidate) {
            return if text.trim() == "1" { DiskKind::Hdd } else { DiskKind::Ssd };
        }
    }
    // No block device behind it (NFS, SMB, FUSE).
    if major == 0 { DiskKind::Network } else { DiskKind::Unknown }
}

#[cfg(not(target_os = "linux"))]
fn disk_kind(_dev: u64) -> DiskKind {
    // Every Mac Gezik runs on boots from an SSD; external spinning disks are rare.
    DiskKind::Ssd
}

pub fn set_hidden(_path: &Path) -> io::Result<()> {
    // A leading dot already hides it.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_numbers_split_like_glibc() {
        // makedev(8, 1) and makedev(259, 3).
        assert_eq!((dev_major(0x801), dev_minor(0x801)), (8, 1));
        let nvme = (259u64 << 8) | 3;
        assert_eq!((dev_major(nvme), dev_minor(nvme)), (259, 3));
    }
}
```

- [ ] **Step 5: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-platform fs::`
Expected: PASS (Windows'ta `fs::tests` ve `fs::windows::tests`; Unix testleri burada derlenmez).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 6: Commit**

```bash
git add crates/gezik-platform
git commit -m "Copy with progress and cancel, move without replacing, POSIX deletes, drive facts"
```

---

### Task 6: Platform: çöpe atma ve çöpten geri getirme

**Files:**
- Modify: `crates/gezik-platform/Cargo.toml` (`windows-core`, macOS objc2)
- Create: `crates/gezik-platform/src/fs/freedesktop.rs`
- Modify: `crates/gezik-platform/src/fs/mod.rs` (modül, dışa açılanlar, testler)
- Modify: `crates/gezik-platform/src/fs/windows.rs` (`trash`, `restore`)
- Modify: `crates/gezik-platform/src/fs/unix.rs` (`trash`, `restore`)

**Interfaces:**
- Consumes: Task 5'in `move_entry`, `drive_root`, `nearest_existing`, `cancelled`, `io_error`
- Produces (`gezik_platform::fs`):
  - `trash(path: &Path) -> io::Result<Option<PathBuf>>` — çöpteki yeni yol; `None`: sistem öğeyi çöpe sığdıramadı, kullanıcı Windows'un uyarısında kalıcı silmeyi seçti
  - `restore(trashed: &Path, original: &Path) -> io::Result<()>` — `original` varsa `AlreadyExists`; eksik üst klasörleri kurar

- [ ] **Step 1: Bağımlılıkları ekle**

`crates/gezik-platform/Cargo.toml` Windows bağımlılıklarına `windows`'tan sonra ekle:

```toml
# The #[implement] macro (a COM progress sink for the Recycle Bin) expands to windows_core paths.
windows-core = "0.62"
```

Dosyanın sonuna ekle:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSArray", "NSError", "NSFileManager", "NSString", "NSURL"] }
objc2-app-kit = { version = "0.3", features = ["NSPasteboard"] }
```

- [ ] **Step 2: freedesktop biçimini testleriyle yaz**

`crates/gezik-platform/src/fs/freedesktop.rs` (her yerde derlenir, testler Windows'ta da çalışır; yalnız Linux kullanır):

```rust
//! The freedesktop.org trash format: `files/NAME` holds the entry, `info/NAME.trashinfo` says
//! where it came from.

/// `path` as the `Path=` value: bytes outside `A-Z a-z 0-9 - _ . ~ /` percent-encoded.
pub(crate) fn encode_path(path: &[u8]) -> String {
    let mut out = String::with_capacity(path.len());
    for &b in path {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The `.trashinfo` file for `path` (absolute) deleted at `deleted_at` (local time,
/// `YYYY-MM-DDThh:mm:ss`).
pub(crate) fn info_text(path: &[u8], deleted_at: &str) -> String {
    format!("[Trash Info]\nPath={}\nDeletionDate={deleted_at}\n", encode_path(path))
}

pub(crate) fn format_date(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

/// `name`, else `name.2`, `name.3`… the first `taken` does not claim.
pub(crate) fn free_name(name: &[u8], taken: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    if !taken(name) {
        return name.to_vec();
    }
    (2u32..)
        .map(|n| {
            let mut candidate = name.to_vec();
            candidate.extend_from_slice(format!(".{n}").as_bytes());
            candidate
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| name.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_percent_encoded() {
        assert_eq!(encode_path(b"/home/a/My File.txt"), "/home/a/My%20File.txt");
        assert_eq!(encode_path("/home/a/şarkı".as_bytes()), "/home/a/%C5%9Fark%C4%B1");
    }

    #[test]
    fn info_file_layout() {
        let date = format_date(2026, 10, 4, 9, 5, 0);
        assert_eq!(date, "2026-10-04T09:05:00");
        assert_eq!(info_text(b"/a/b", &date), "[Trash Info]\nPath=/a/b\nDeletionDate=2026-10-04T09:05:00\n");
    }

    #[test]
    fn free_names_count_up() {
        let taken: [&[u8]; 2] = [b"a.txt", b"a.txt.2"];
        assert_eq!(free_name(b"a.txt", |n| taken.contains(&n)), b"a.txt.3");
        assert_eq!(free_name(b"b", |_| false), b"b");
    }
}
```

`crates/gezik-platform/src/fs/mod.rs` en üstündeki modül listesine ekle ve dışa açılanları güncelle:

```rust
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod freedesktop;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
```

```rust
#[cfg(unix)]
pub use unix::{copy_file, delete, drive_facts, drive_root, move_entry, restore, set_hidden, trash};
#[cfg(windows)]
pub use windows::{copy_file, delete, drive_facts, drive_root, move_entry, restore, set_hidden, trash};
```

- [ ] **Step 3: Windows'ta çöp ve geri getirme**

`crates/gezik-platform/src/fs/windows.rs` dosyasının `use` bloğuna ekle:

```rust
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
    FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IFileOperationProgressSink,
    IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, PCWSTR, Ref};
```

Ve dosyanın sonuna (testlerden önce) ekle:

```rust
/// Hears where each deleted item went in the Recycle Bin.
#[windows_core::implement(IFileOperationProgressSink)]
struct DeleteSink {
    trashed: Rc<RefCell<Option<PathBuf>>>,
    result: Rc<Cell<HRESULT>>,
}

impl IFileOperationProgressSink_Impl for DeleteSink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR, _: HRESULT, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreDeleteItem(&self, _: u32, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostDeleteItem(&self, _: u32, _: Ref<IShellItem>, hr: HRESULT, created: Ref<IShellItem>) -> windows::core::Result<()> {
        self.result.set(hr);
        // `created` is the item in the Recycle Bin; none if it was deleted for good.
        if let Ok(item) = created.ok()
            && let Ok(name) = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
        {
            let text = unsafe { name.to_string() }.unwrap_or_default();
            unsafe { CoTaskMemFree(Some(name.0 as *const c_void)) };
            if !text.is_empty() {
                *self.trashed.borrow_mut() = Some(PathBuf::from(text));
            }
        }
        Ok(())
    }
    fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Moves `path` to the Recycle Bin without any Windows dialog, except the one that asks before
/// deleting an item for good that does not fit in the Recycle Bin.
pub fn trash(path: &Path) -> io::Result<Option<PathBuf>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(io_error)?;
        operation
            .SetOperationFlags(
                FOF_ALLOWUNDO
                    | FOF_NOCONFIRMATION
                    | FOF_SILENT
                    | FOF_NOERRORUI
                    | FOF_NOCONFIRMMKDIR
                    | FOF_WANTNUKEWARNING
                    | FOFX_RECYCLEONDELETE
                    | FOFX_EARLYFAILURE,
            )
            .map_err(io_error)?;
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).map_err(io_error)?;
        let trashed = Rc::new(RefCell::new(None));
        let result = Rc::new(Cell::new(S_OK));
        let sink: IFileOperationProgressSink = DeleteSink { trashed: trashed.clone(), result: result.clone() }.into();
        operation.DeleteItem(&item, &sink).map_err(io_error)?;
        operation.PerformOperations().map_err(io_error)?;
        if operation.GetAnyOperationsAborted().map_err(io_error)?.as_bool() {
            return Err(cancelled());
        }
        result.get().ok().map_err(io_error)?;
        Ok(trashed.borrow_mut().take())
    }
}

/// Moves `trashed` (`…\$Recycle.Bin\…\$Rxxxxxx.ext`) back to `original`.
pub fn restore(trashed: &Path, original: &Path) -> io::Result<()> {
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    move_entry(trashed, original)?;
    // Next to the entry, `$Ixxxxxx.ext` records where it came from; without its entry the
    // Recycle Bin would list a broken item.
    if let (Some(dir), Some(name)) = (trashed.parent(), trashed.file_name().and_then(|n| n.to_str()))
        && let Some(rest) = name.strip_prefix("$R")
    {
        let _ = std::fs::remove_file(dir.join(format!("$I{rest}")));
    }
    Ok(())
}
```

- [ ] **Step 4: macOS ve Linux'ta çöp ve geri getirme**

`crates/gezik-platform/src/fs/unix.rs` sonuna (testlerden önce) ekle:

```rust
#[cfg(target_os = "macos")]
pub fn trash(path: &Path) -> io::Result<Option<PathBuf>> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let text = path.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path is not UTF-8"))?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let mut resulting = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
        .map_err(|err| io::Error::other(err.localizedDescription().to_string()))?;
    Ok(resulting.and_then(|url| url.path()).map(|p| PathBuf::from(p.to_string())))
}

#[cfg(target_os = "linux")]
pub fn trash(path: &Path) -> io::Result<Option<PathBuf>> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use super::freedesktop::{free_name, info_text};

    let absolute = std::path::absolute(path)?;
    let dev = std::fs::symlink_metadata(&absolute)?.dev();
    let trash_dir = trash_dir_for(&absolute, dev)?;
    let (files, info) = (trash_dir.join("files"), trash_dir.join("info"));
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let name = absolute.file_name().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?.as_bytes().to_vec();
    let info_of = |n: &[u8]| {
        let mut file = n.to_vec();
        file.extend_from_slice(b".trashinfo");
        info.join(std::ffi::OsString::from_vec(file))
    };
    // The info file is made first, with create_new: it claims the name.
    let (chosen, info_path) = loop {
        let candidate = free_name(&name, |n| {
            info_of(n).exists() || std::fs::symlink_metadata(files.join(std::ffi::OsStr::from_bytes(n))).is_ok()
        });
        let info_path = info_of(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&info_path) {
            Ok(mut file) => {
                file.write_all(info_text(absolute.as_os_str().as_bytes(), &local_now()).as_bytes())?;
                break (candidate, info_path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    };
    let target = files.join(std::ffi::OsString::from_vec(chosen));
    if let Err(err) = std::fs::rename(&absolute, &target) {
        let _ = std::fs::remove_file(&info_path);
        return Err(err);
    }
    Ok(Some(target))
}

/// The home trash if `path` is on the same device, else `.Trash-<uid>` at its mount point.
#[cfg(target_os = "linux")]
fn trash_dir_for(path: &Path, dev: u64) -> io::Result<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/share")))
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let home_trash = data_home.join("Trash");
    let home_dev = nearest_existing(&home_trash).and_then(|p| std::fs::metadata(p).ok()).map(|m| m.dev());
    if home_dev == Some(dev) {
        return Ok(home_trash);
    }
    let top = drive_root(path).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let uid = unsafe { libc::getuid() };
    Ok(top.join(format!(".Trash-{uid}")))
}

#[cfg(target_os = "linux")]
fn local_now() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    super::freedesktop::format_date(tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

pub fn restore(trashed: &Path, original: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(original).is_ok() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(trashed, original)?;
    // freedesktop: drop `info/NAME.trashinfo` next to `files/NAME`.
    if cfg!(target_os = "linux")
        && let (Some(files), Some(name)) = (trashed.parent(), trashed.file_name())
        && let Some(root) = files.parent()
    {
        let mut info = name.to_os_string();
        info.push(".trashinfo");
        let _ = std::fs::remove_file(root.join("info").join(info));
    }
    Ok(())
}
```

- [ ] **Step 5: Gerçek çöple gidiş-dönüş testini yaz**

`crates/gezik-platform/src/fs/mod.rs` test modülüne ekle (Windows'ta geçici klasör C: üzerindedir ve Çöp Kutusu vardır; test kendi öğesini geri getirerek temizler):

```rust
    #[test]
    fn trash_and_restore_round_trip() {
        let dir = test_dir("trash");
        let file = dir.join("çöp testi.txt");
        std::fs::write(&file, "keep me").unwrap();
        let trashed = trash(&file).unwrap().expect("the temp folder has a trash");
        assert!(!file.exists());
        assert!(std::fs::symlink_metadata(&trashed).is_ok(), "{}", trashed.display());
        restore(&trashed, &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep me");
        assert!(std::fs::symlink_metadata(&trashed).is_err());

        let folder = dir.join("sub");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("x"), "x").unwrap();
        let trashed = trash(&folder).unwrap().unwrap();
        std::fs::write(&folder, "in the way").ok();
        assert_eq!(restore(&trashed, &folder).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        std::fs::remove_file(&folder).unwrap();
        restore(&trashed, &folder).unwrap();
        assert!(folder.join("x").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trashing_what_is_not_there_fails() {
        let dir = test_dir("trash-missing");
        assert!(trash(&dir.join("nothing-here")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 6: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-platform fs::`
Expected: PASS (freedesktop testleri de Windows'ta çalışır).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 7: Commit**

```bash
git add crates/gezik-platform
git commit -m "Trash and restore: Recycle Bin with the new location, macOS trash, freedesktop trash"
```

---

### Task 7: Platform: pano, görev çubuğu ilerlemesi, Shell menüsü komutlarını yakalama

**Files:**
- Modify: `crates/gezik-platform/Cargo.toml` (windows özellikleri)
- Create: `crates/gezik-platform/src/clipboard.rs`
- Create: `crates/gezik-platform/src/taskbar.rs`
- Modify: `crates/gezik-platform/src/lib.rs` (`ShellVerb`, `MenuOutcome::Verb`, modüller)
- Modify: `crates/gezik-platform/src/shell_menu.rs`
- Modify: `crates/gezik/src/context_menu.rs` (yeni sonuç kolu, şimdilik boş)

**Interfaces:**
- Consumes: —
- Produces:
  - `gezik_platform::clipboard::{ClipboardFiles { paths: Vec<PathBuf>, cut: bool }, ClipboardError { Unsupported, Failed(String) }}`, `write_files(&[PathBuf], cut: bool) -> Result<(), ClipboardError>`, `read_files() -> Result<Option<ClipboardFiles>, ClipboardError>`, `sequence() -> u64`, `clear() -> Result<(), ClipboardError>`
  - `gezik_platform::taskbar::{Taskbar, TaskbarState { Off, Normal, Paused, Error }}`, `Taskbar::new(window: &impl HasWindowHandle) -> Taskbar`, `Taskbar::set(&self, TaskbarState, done: u64, total: u64)`
  - `gezik_platform::ShellVerb { Cut, Copy, Paste, Delete, Rename }`, `MenuOutcome::Verb(ShellVerb)`

- [ ] **Step 1: Windows özelliklerini ekle**

`windows` özellik listesine alfabetik sırayla ekle: `"Win32_System_Com_StructuredStorage"`, `"Win32_System_DataExchange"`, `"Win32_System_Memory"`, `"Win32_System_Ole"`.

- [ ] **Step 2: Pano modülünü testleriyle yaz**

`crates/gezik-platform/src/clipboard.rs`:

```rust
//! The system clipboard, for files: copy or cut in Gezik and paste in Explorer or Finder, and
//! the other way round. Linux has no system clipboard here yet (sub-project 4b).

use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    /// Cut (move on paste) rather than copied.
    pub cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// No system clipboard for files on this platform: Gezik keeps its own.
    Unsupported,
    Failed(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::Unsupported => write!(f, "no system clipboard for files"),
            ClipboardError::Failed(why) => write!(f, "{why}"),
        }
    }
}

pub use imp::{clear, read_files, sequence, write_files};

/// `CF_HDROP` data: a DROPFILES header (wide names) and each path, NUL-terminated, then a NUL.
#[cfg(windows)]
fn dropfiles(paths: &[PathBuf]) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    let mut out = Vec::new();
    out.extend_from_slice(&20u32.to_le_bytes()); // pFiles: the names start after the header
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0i32.to_le_bytes()); // fNC
    out.extend_from_slice(&1i32.to_le_bytes()); // fWide
    for path in paths {
        for unit in path.as_os_str().encode_wide().chain([0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE};
    use windows::Win32::UI::Shell::{CFSTR_PREFERREDDROPEFFECT, DragQueryFileW, HDROP};

    use super::{ClipboardError, ClipboardFiles};

    fn failed(err: impl std::fmt::Display) -> ClipboardError {
        ClipboardError::Failed(err.to_string())
    }

    /// The clipboard, open until dropped. Another program may hold it for a moment: retry.
    struct Open;

    impl Open {
        fn new() -> Result<Open, ClipboardError> {
            for _ in 0..10 {
                if unsafe { OpenClipboard(None) }.is_ok() {
                    return Ok(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(failed("another program is using the clipboard"))
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            let _ = unsafe { CloseClipboard() };
        }
    }

    fn put(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(failed)?;
            let target = GlobalLock(memory) as *mut u8;
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(failed("out of memory"));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            let _ = GlobalUnlock(memory);
            // The clipboard owns the memory once this succeeds.
            if let Err(err) = SetClipboardData(format, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(failed(err));
            }
        }
        Ok(())
    }

    fn effect_format() -> u32 {
        unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) }
    }

    pub fn write_files(paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.map_err(failed)?;
        put(u32::from(CF_HDROP.0), &super::dropfiles(paths))?;
        let effect = if cut { DROPEFFECT_MOVE.0 } else { DROPEFFECT_COPY.0 };
        put(effect_format(), &effect.to_le_bytes())
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        let _open = Open::new()?;
        let Ok(handle) = (unsafe { GetClipboardData(u32::from(CF_HDROP.0)) }) else { return Ok(None) };
        let drop = HDROP(handle.0);
        let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
        let mut paths = Vec::new();
        for i in 0..count {
            let len = unsafe { DragQueryFileW(drop, i, None) } as usize;
            let mut buffer = vec![0u16; len + 1];
            let written = unsafe { DragQueryFileW(drop, i, Some(&mut buffer)) } as usize;
            paths.push(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..written.min(len)])));
        }
        if paths.is_empty() {
            return Ok(None);
        }
        let cut = unsafe { GetClipboardData(effect_format()) }
            .ok()
            .and_then(|h| read_u32(HGLOBAL(h.0)))
            .is_some_and(|effect| effect & DROPEFFECT_MOVE.0 != 0);
        Ok(Some(ClipboardFiles { paths, cut }))
    }

    fn read_u32(memory: HGLOBAL) -> Option<u32> {
        unsafe {
            if GlobalSize(memory) < 4 {
                return None;
            }
            let source = GlobalLock(memory) as *const u8;
            if source.is_null() {
                return None;
            }
            let mut bytes = [0u8; 4];
            std::ptr::copy_nonoverlapping(source, bytes.as_mut_ptr(), 4);
            let _ = GlobalUnlock(memory);
            Some(u32::from_le_bytes(bytes))
        }
    }

    pub fn sequence() -> u64 {
        u64::from(unsafe { GetClipboardSequenceNumber() })
    }

    pub fn clear() -> Result<(), ClipboardError> {
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.map_err(failed)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::Cell;
    use std::path::PathBuf;

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::ClassType;
    use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    use super::{ClipboardError, ClipboardFiles};

    thread_local! {
        /// The pasteboard's change count when Gezik last cut: Finder has no "cut", so Gezik
        /// remembers its own.
        static CUT_AT: Cell<Option<isize>> = const { Cell::new(None) };
    }

    pub fn write_files(paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
            .iter()
            .filter_map(|p| p.to_str())
            .map(|p| ProtocolObject::from_retained(NSURL::fileURLWithPath(&NSString::from_str(p))))
            .collect();
        if !pasteboard.writeObjects(&NSArray::from_retained_slice(&urls)) {
            return Err(ClipboardError::Failed("the pasteboard refused the files".into()));
        }
        CUT_AT.set(cut.then(|| pasteboard.changeCount()));
        Ok(())
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let classes = NSArray::from_slice(&[NSURL::class()]);
        let Some(objects) = (unsafe { pasteboard.readObjectsForClasses_options(&classes, None) }) else {
            return Ok(None);
        };
        let paths: Vec<PathBuf> = objects
            .iter()
            .filter_map(|object| object.downcast::<NSURL>().ok())
            .filter_map(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .collect();
        if paths.is_empty() {
            return Ok(None);
        }
        let cut = CUT_AT.get() == Some(pasteboard.changeCount());
        Ok(Some(ClipboardFiles { paths, cut }))
    }

    pub fn sequence() -> u64 {
        NSPasteboard::generalPasteboard().changeCount() as u64
    }

    pub fn clear() -> Result<(), ClipboardError> {
        NSPasteboard::generalPasteboard().clearContents();
        CUT_AT.set(None);
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::path::PathBuf;

    use super::{ClipboardError, ClipboardFiles};

    pub fn write_files(_paths: &[PathBuf], _cut: bool) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported)
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }

    pub fn sequence() -> u64 {
        0
    }

    pub fn clear() -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn dropfiles_layout() {
        let bytes = dropfiles(&[PathBuf::from(r"C:\a"), PathBuf::from(r"C:\ş")]);
        assert_eq!(&bytes[0..4], &20u32.to_le_bytes());
        assert_eq!(&bytes[16..20], &1i32.to_le_bytes(), "wide names");
        let units: Vec<u16> = bytes[20..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let expected: Vec<u16> = "C:\\a\0C:\\ş\0\0".encode_utf16().collect();
        assert_eq!(units, expected);
    }

    /// Uses the real clipboard: run by hand (`cargo test -p gezik-platform -- --ignored`).
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "replaces the user's clipboard"]
    fn files_round_trip_through_the_clipboard() {
        let paths = vec![std::env::temp_dir().join("gezik clip a.txt"), std::env::temp_dir().join("b")];
        let before = sequence();
        write_files(&paths, true).unwrap();
        assert_ne!(sequence(), before);
        assert_eq!(read_files().unwrap(), Some(ClipboardFiles { paths: paths.clone(), cut: true }));
        write_files(&paths, false).unwrap();
        assert!(!read_files().unwrap().unwrap().cut);
        clear().unwrap();
        assert_eq!(read_files().unwrap(), None);
    }
}
```

- [ ] **Step 3: Görev çubuğu modülünü yaz**

`crates/gezik-platform/src/taskbar.rs`:

```rust
//! Windows: the taskbar button shows the progress of all running file operations. Elsewhere
//! this does nothing.

use raw_window_handle::HasWindowHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarState {
    Off,
    Normal,
    Paused,
    Error,
}

pub struct Taskbar {
    #[cfg(windows)]
    inner: Option<(windows::Win32::UI::Shell::ITaskbarList3, windows::Win32::Foundation::HWND)>,
}

impl Taskbar {
    /// Call on the UI thread once the window is shown.
    pub fn new(window: &impl HasWindowHandle) -> Taskbar {
        #[cfg(windows)]
        {
            use raw_window_handle::RawWindowHandle;
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
            use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
            let hwnd = match window.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(win32)) => HWND(win32.hwnd.get() as *mut _),
                _ => return Taskbar { inner: None },
            };
            let list: Option<ITaskbarList3> = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) }
                .ok()
                .filter(|list: &ITaskbarList3| unsafe { list.HrInit() }.is_ok());
            Taskbar { inner: list.map(|list| (list, hwnd)) }
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            Taskbar {}
        }
    }

    /// `done` of `total` (any unit); `Off` removes the bar.
    pub fn set(&self, state: TaskbarState, done: u64, total: u64) {
        #[cfg(windows)]
        if let Some((list, hwnd)) = &self.inner {
            use windows::Win32::UI::Shell::{TBPF_ERROR, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED};
            let flag = match state {
                TaskbarState::Off => TBPF_NOPROGRESS,
                TaskbarState::Normal => TBPF_NORMAL,
                TaskbarState::Paused => TBPF_PAUSED,
                TaskbarState::Error => TBPF_ERROR,
            };
            unsafe {
                let _ = list.SetProgressState(*hwnd, flag);
                if state != TaskbarState::Off {
                    let _ = list.SetProgressValue(*hwnd, done.min(total), total.max(1));
                }
            }
        }
        #[cfg(not(windows))]
        let _ = (state, done, total);
    }
}
```

- [ ] **Step 4: `ShellVerb` ve modülleri `lib.rs`'e ekle**

`crates/gezik-platform/src/lib.rs`: `pub mod fs;` satırının yanına ekle:

```rust
pub mod clipboard;
pub mod taskbar;
```

`MenuOutcome`'u şu hale getir ve `ShellVerb`'ü ekle:

```rust
/// Explorer menu commands Gezik does itself, with its own engine (not Explorer's dialogs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellVerb {
    Cut,
    Copy,
    Paste,
    Delete,
    Rename,
}

impl ShellVerb {
    /// The verb for a canonical command name (`GetCommandString`), if Gezik does it.
    pub fn from_name(name: &[u8]) -> Option<ShellVerb> {
        match name.to_ascii_lowercase().as_slice() {
            b"cut" => Some(ShellVerb::Cut),
            b"copy" => Some(ShellVerb::Copy),
            b"paste" => Some(ShellVerb::Paste),
            b"delete" => Some(ShellVerb::Delete),
            b"rename" => Some(ShellVerb::Rename),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    /// One of Gezik's own items, by id.
    Gezik(u32),
    /// An Explorer command Gezik does itself; nothing ran yet.
    Verb(ShellVerb),
    /// A system command ran; the folder may have changed.
    SystemCommandRan,
    Dismissed,
}
```

`lib.rs` sonuna test ekle:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_verbs_gezik_does() {
        assert_eq!(ShellVerb::from_name(b"delete"), Some(ShellVerb::Delete));
        assert_eq!(ShellVerb::from_name(b"Paste"), Some(ShellVerb::Paste));
        assert_eq!(ShellVerb::from_name(b"properties"), None);
    }
}
```

- [ ] **Step 5: Shell menüsünde komutları yakala**

`crates/gezik-platform/src/shell_menu.rs`:

1. `remove_explorer_only_items` fonksiyonunu ve `track` içindeki `remove_explorer_only_items(hmenu, menu);` çağrısını sil ("Rename" artık Gezik'in yerinde yeniden adlandırmasını açar). Kullanılmayan `DeleteMenu`, `GetMenuItemID`, `MF_BYCOMMAND` içe aktarmalarını kaldır.
2. `use crate::{MenuOutcome, MenuTarget};` satırını `use crate::{MenuOutcome, MenuTarget, ShellVerb};` yap.
3. `track` içindeki `id => { … }` kolunun başına ekle:

```rust
            id => {
                if let Some(verb) = gezik_verb(menu, id - FIRST_SHELL_ID) {
                    return Ok(MenuOutcome::Verb(verb));
                }
                let info = CMINVOKECOMMANDINFO {
```

4. Dosyaya ekle (`track`'ten sonra):

```rust
/// The command at `offset` if Gezik does it itself (cut, copy, paste, delete, rename).
unsafe fn gezik_verb(menu: &IContextMenu, offset: u32) -> Option<ShellVerb> {
    let mut verb = [0u8; 64];
    unsafe { menu.GetCommandString(offset as usize, GCS_VERBA, None, PSTR(verb.as_mut_ptr()), verb.len() as u32) }
        .ok()?;
    let end = verb.iter().position(|&b| b == 0).unwrap_or(verb.len());
    ShellVerb::from_name(&verb[..end])
}
```

- [ ] **Step 6: Uygulamadaki eşleşmeyi tamamla**

`crates/gezik/src/context_menu.rs` içindeki `open_native`'de `match outcome { … }` bloğuna `Dismissed` kolundan önce ekle (Task 13 gerçek komutlara bağlar):

```rust
                Ok(gezik_platform::MenuOutcome::Verb(_)) => {}
```

- [ ] **Step 7: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test --workspace`
Expected: PASS (`dropfiles_layout`, `explorer_verbs_gezik_does`; pano gidiş-dönüş testi `ignored`).
Run: `~/.cargo/bin/cargo test -p gezik-platform -- --ignored files_round_trip_through_the_clipboard`
Expected: PASS (panonu değiştirir).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 8: Commit**

```bash
git add crates/gezik-platform crates/gezik/src/context_menu.rs
git commit -m "System clipboard for files, taskbar progress, Explorer menu commands handed to Gezik"
```

---

### Task 8: `gezik-ops` motoru: görevler, kuyruk, olaylar, duraklat/iptal

**Files:**
- Modify: `Cargo.toml` (workspace üyesi ve bağımlılığı)
- Create: `crates/gezik-ops/Cargo.toml`
- Create: `crates/gezik-ops/src/lib.rs`
- Create: `crates/gezik-ops/src/task.rs`
- Create: `crates/gezik-ops/src/walk.rs`
- Create: `crates/gezik-ops/src/control.rs`
- Create: `crates/gezik-ops/src/engine.rs`
- Create: `crates/gezik-ops/src/run.rs`
- Create: `crates/gezik-ops/src/testing.rs`

**Interfaces:**
- Consumes: `gezik_core::ops::{conflict::*, paths::{DriveSet, is_within, same_path}, names::next_free, threads::{CopyThreads, DiskKind, workers}}` (Task 1–2), `gezik_platform::fs::{copy_file, drive_facts, drive_root, nearest_existing, trash, delete, is_disk_full, DriveFacts}` (Task 5–6)
- Produces (`gezik_ops`):
  - `pub trait Task: Send + Sync { fn kind(&self) -> TaskKind; fn title(&self) -> String; fn count(&self) -> usize; fn resources(&self) -> Resources; fn plan(&self, sink: &mut dyn ScanSink); fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome>; fn done(&self, _cancelled: bool) {} }`
  - `pub enum TaskKind { Copy, Move, Rename, Trash, Delete, Restore, NewFolder, NewFile }`, `TaskKind::label(self, count: usize) -> String`
  - `pub struct Resources { pub paths: Vec<PathBuf>, pub work: Work }`, `pub enum Work { Disk, Cpu, External }`
  - `pub enum Stage { Before, Parallel, After }`
  - `pub struct PlanItem { pub source, pub target: Option<PathBuf>, pub facts: Facts, pub stage, pub check_target: bool, pub preset: Option<Decision>, pub is_root: bool, pub root: usize, pub tag: u8, .. }` ve kurucu yöntemleri `new(stage, facts)`, `source`, `target`, `checked`, `top(root)`, `under(root)`, `preset`, `tag`, `path()`
  - `pub enum Outcome { Created { path, facts, from: Option<PathBuf> }, Moved { from, to, facts }, Trashed { original, trashed }, Restored { original, facts }, Deleted { path }, Nothing }`, `Outcome::result(&self) -> Option<&Path>`
  - `pub trait ScanSink { fn item(&mut self, item: PlanItem) -> bool; fn failed(&mut self, path: &Path, error: io::Error); }`
  - `pub struct RunCx<'a>`: `add_bytes(u64)`, `stopped() -> bool`, `has_trash(&Path) -> bool`, `copy_file(&Path, &Path, size: u64) -> io::Result<()>`
  - `pub struct ChangedSince;`, `pub struct NoTrash;` (hata işaretleri), `pub fn changed_since() -> io::Error`, `pub fn no_trash() -> io::Error`, `pub fn unchanged(&Path, Option<Facts>) -> bool`, `pub fn facts_after(&Path, is_dir: bool) -> Facts`
  - `walk::{walk, Step, facts_of}`
  - `pub struct Engine` (`Clone`): `new(Settings, notify: impl Fn() + Send + Sync + 'static)`, `set_threads(CopyThreads)`, `submit(Box<dyn Task>) -> JobId`, `drain() -> Vec<Event>`, `decide(JobId, Vec<Decision>)`, `pause(JobId)`, `resume(JobId)`, `cancel(JobId)`, `start_now(JobId)`, `cancel_all()`, `busy() -> bool`, `wait_idle(Duration) -> bool`
  - `pub struct Settings { pub threads: CopyThreads, pub pending_deletes: Option<PathBuf> }` (`Default`)
  - `pub type JobId = u64;`, `pub enum PauseReason { User, DiskFull, ManyFailures }`, `pub enum JobState { Waiting, Scanning, Running, Deciding, Paused(PauseReason) }`
  - `pub struct Progress { pub state, pub items_done, pub items_total, pub bytes_done, pub bytes_total }`
  - `pub struct ConflictItem { pub source, pub target: PathBuf, pub kind: ConflictKind, pub source_facts, pub target_facts: Facts, pub decision: Decision }`
  - `pub struct Failure { pub path: PathBuf, pub message: String }`
  - `pub struct Report { pub kind: TaskKind, pub cancelled: bool, pub failures: Vec<Failure>, pub skipped_changed: usize, pub no_trash: Vec<PathBuf>, pub results: Vec<PathBuf>, pub changed_dirs: Vec<PathBuf> }`
  - `pub enum Event { Added { job, title: String, kind: TaskKind, background: bool }, Progress { job, progress }, Conflicts { job, conflicts: Vec<ConflictItem> }, Paused { job, reason, path: Option<PathBuf> }, Finished { job, report }, Changed { dirs: Vec<PathBuf> }, History }`

- [ ] **Step 1: Crate'i kur**

Kök `Cargo.toml`: `members` listesine `"crates/gezik-ops"`'u `"crates/gezik-platform"`'tan sonra ekle; `[workspace.dependencies]`'e ekle:

```toml
gezik-ops = { path = "crates/gezik-ops" }
```

`crates/gezik-ops/Cargo.toml`:

```toml
[package]
name = "gezik-ops"
description = "File operations engine of the Gezik file manager: queue, conflicts, progress, undo"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
gezik-core.workspace = true
gezik-platform.workspace = true
```

`crates/gezik-ops/src/lib.rs`:

```rust
//! The file operations engine: runs [`Task`]s (copy, move, trash…) off the UI thread, one
//! queue per drive, with conflicts asked up front, progress, pause and cancel.

mod control;
mod engine;
mod run;
mod task;
#[cfg(test)]
mod testing;
pub mod walk;

pub use engine::{ConflictItem, Engine, Event, Failure, JobId, JobState, PauseReason, Progress, Report, Settings};
pub use gezik_core::ops::conflict::{ConflictKind, Decision, Facts};
pub use task::{
    ChangedSince, NoTrash, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since,
    facts_after, no_trash, unchanged,
};
```

- [ ] **Step 2: `task.rs`'i yaz**

```rust
//! What the engine runs: a `Task` lists its items (the plan), then does them one by one.

use std::cell::Cell;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::control::Control;
use crate::walk::facts_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Copy,
    Move,
    Rename,
    Trash,
    Delete,
    Restore,
    NewFolder,
    NewFile,
}

impl TaskKind {
    fn verb(self) -> &'static str {
        match self {
            TaskKind::Copy => "Copy",
            TaskKind::Move => "Move",
            TaskKind::Rename => "Rename",
            TaskKind::Trash => "Delete",
            TaskKind::Delete => "Delete permanently",
            TaskKind::Restore => "Restore",
            TaskKind::NewFolder => "New folder",
            TaskKind::NewFile => "New file",
        }
    }

    /// What the Undo item says: "Copy 3 items", "Rename".
    pub fn label(self, count: usize) -> String {
        let verb = self.verb();
        match self {
            TaskKind::Rename | TaskKind::NewFolder | TaskKind::NewFile => verb.to_owned(),
            _ if count == 1 => format!("{verb} 1 item"),
            _ => format!("{verb} {count} items"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Bound by the disk: as many workers as the disk kind suits.
    Disk,
    /// Bound by the processor (converting pictures, packing archives): one per core.
    Cpu,
    /// Runs other programs (ffmpeg): a couple at a time.
    External,
}

#[derive(Debug, Clone)]
pub struct Resources {
    /// Paths on every drive the task touches (sources and target folders).
    pub paths: Vec<PathBuf>,
    pub work: Work,
}

/// When an item runs. `Before` items run one by one as they are planned (folders to make,
/// before what goes in them); `Parallel` items on the workers; `After` items one by one once
/// everything else is done, in reverse plan order (folders to remove: the deepest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Before,
    Parallel,
    After,
}

#[derive(Debug, Clone)]
pub struct PlanItem {
    /// What it acts on; `None` for something new.
    pub source: Option<PathBuf>,
    /// Where the result goes; checked for an existing entry if `check_target`.
    pub target: Option<PathBuf>,
    /// The source's facts (for something new: only `is_dir`).
    pub facts: Facts,
    pub stage: Stage,
    pub check_target: bool,
    /// Settles a conflict without asking (a copy into its own folder keeps both).
    pub preset: Option<Decision>,
    /// One of the things the user chose (not something inside one).
    pub is_root: bool,
    /// Which of the task's roots it belongs to.
    pub root: usize,
    /// The task's own note on what to do with it.
    pub tag: u8,
    /// The existing target goes to the trash first (set by the engine for "Replace").
    pub(crate) replace: bool,
}

impl PlanItem {
    pub fn new(stage: Stage, facts: Facts) -> PlanItem {
        PlanItem {
            source: None,
            target: None,
            facts,
            stage,
            check_target: false,
            preset: None,
            is_root: false,
            root: 0,
            tag: 0,
            replace: false,
        }
    }

    pub fn source(mut self, path: impl Into<PathBuf>) -> PlanItem {
        self.source = Some(path.into());
        self
    }

    pub fn target(mut self, path: impl Into<PathBuf>) -> PlanItem {
        self.target = Some(path.into());
        self
    }

    pub fn checked(mut self) -> PlanItem {
        self.check_target = true;
        self
    }

    /// Root `root` itself.
    pub fn top(mut self, root: usize) -> PlanItem {
        self.is_root = true;
        self.root = root;
        self
    }

    /// Something inside root `root`.
    pub fn under(mut self, root: usize) -> PlanItem {
        self.root = root;
        self
    }

    pub fn preset(mut self, decision: Option<Decision>) -> PlanItem {
        self.preset = decision;
        self
    }

    pub fn tag(mut self, tag: u8) -> PlanItem {
        self.tag = tag;
        self
    }

    /// The path a failure names: the source, else the target.
    pub fn path(&self) -> &Path {
        self.source.as_deref().or(self.target.as_deref()).unwrap_or(Path::new(""))
    }
}

/// What doing an item changed; the engine builds undo from these.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// `path` was made: copied (`from` none), moved across drives (`from` the original), or new.
    Created { path: PathBuf, facts: Facts, from: Option<PathBuf> },
    /// Renamed or moved in one step.
    Moved { from: PathBuf, to: PathBuf, facts: Facts },
    Trashed { original: PathBuf, trashed: PathBuf },
    Restored { original: PathBuf, facts: Facts },
    Deleted { path: PathBuf },
    /// Nothing changed (a folder that was already there).
    Nothing,
}

impl Outcome {
    /// Where the result is now, to select it afterwards.
    pub fn result(&self) -> Option<&Path> {
        match self {
            Outcome::Created { path, .. } | Outcome::Restored { original: path, .. } | Outcome::Moved { to: path, .. } => {
                Some(path)
            }
            _ => None,
        }
    }
}

/// Where a task's plan goes.
pub trait ScanSink {
    /// Adds an item; false once the job is cancelled (stop planning).
    fn item(&mut self, item: PlanItem) -> bool;
    /// Something could not be read; it is reported as failed.
    fn failed(&mut self, path: &Path, error: io::Error);
}

pub trait Task: Send + Sync {
    fn kind(&self) -> TaskKind;
    /// The panel's line while it runs: "Copying 3 items to D:\Yedek".
    fn title(&self) -> String;
    /// How many things the user chose (for "Copy 3 items").
    fn count(&self) -> usize;
    fn resources(&self) -> Resources;
    /// Lists what to do; a folder before what is in it.
    fn plan(&self, sink: &mut dyn ScanSink);
    /// Does `item`. Its target is free: conflicts are settled before.
    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome>;
    /// Called once the job ends.
    fn done(&self, _cancelled: bool) {}
}

/// What `Task::run` may use: progress, pause and cancel, the drive's trash.
pub struct RunCx<'a> {
    pub(crate) control: &'a Control,
    pub(crate) trash: &'a dyn Fn(&Path) -> bool,
    /// Bytes this item counted so far (the engine counts the rest when it is done).
    pub(crate) added: Cell<u64>,
}

impl RunCx<'_> {
    pub fn add_bytes(&self, bytes: u64) {
        self.added.set(self.added.get() + bytes);
        self.control.add_bytes(bytes);
    }

    /// Waits while the job is paused; true once it is cancelled.
    pub fn stopped(&self) -> bool {
        self.control.stopped()
    }

    pub fn has_trash(&self, path: &Path) -> bool {
        (self.trash)(path)
    }

    /// Copies a file, counting its bytes and stopping when the job is cancelled.
    pub fn copy_file(&self, from: &Path, to: &Path, size: u64) -> io::Result<()> {
        let mut counted = 0u64;
        gezik_platform::fs::copy_file(from, to, size, &mut |done| {
            self.add_bytes(done.saturating_sub(counted));
            counted = counted.max(done);
            !self.stopped()
        })
    }
}

/// The item changed after the operation being undone: it is left alone.
#[derive(Debug)]
pub struct ChangedSince;

impl fmt::Display for ChangedSince {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "changed since; skipped")
    }
}

impl std::error::Error for ChangedSince {}

/// The item's drive has no trash; the user is asked whether to delete it for good.
#[derive(Debug)]
pub struct NoTrash;

impl fmt::Display for NoTrash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if cfg!(windows) { write!(f, "This drive has no Recycle Bin") } else { write!(f, "This drive has no trash") }
    }
}

impl std::error::Error for NoTrash {}

pub fn changed_since() -> io::Error {
    io::Error::other(ChangedSince)
}

pub fn no_trash() -> io::Error {
    io::Error::other(NoTrash)
}

/// Whether `err` carries the marker `E`.
pub(crate) fn is_marker<E: std::error::Error + 'static>(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<E>())
}

/// Whether `path` still looks as `expected` says (no expectation: yes).
pub fn unchanged(path: &Path, expected: Option<Facts>) -> bool {
    let Some(expected) = expected else { return true };
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let now = facts_of(&meta);
            now.size == expected.size && now.modified == expected.modified
        }
        Err(_) => false,
    }
}

/// `path`'s facts right after an item made it.
pub fn facts_after(path: &Path, is_dir: bool) -> Facts {
    std::fs::symlink_metadata(path).map(|m| facts_of(&m)).unwrap_or(Facts { is_dir, ..Facts::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_count_items() {
        assert_eq!(TaskKind::Copy.label(3), "Copy 3 items");
        assert_eq!(TaskKind::Trash.label(1), "Delete 1 item");
        assert_eq!(TaskKind::Rename.label(1), "Rename");
        assert_eq!(TaskKind::NewFolder.label(1), "New folder");
    }

    #[test]
    fn markers_are_recognized() {
        assert!(is_marker::<ChangedSince>(&changed_since()));
        assert!(!is_marker::<NoTrash>(&changed_since()));
        assert!(!is_marker::<NoTrash>(&io::Error::other("x")));
    }
}
```

- [ ] **Step 3: `walk.rs`'i testleriyle yaz**

```rust
//! Walking a folder tree: a folder before what is in it; links are not followed.

use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::Facts;

/// Facts from `symlink_metadata` (a link is neither a folder nor sized).
pub fn facts_of(meta: &Metadata) -> Facts {
    Facts { is_dir: meta.is_dir(), size: if meta.is_file() { meta.len() } else { 0 }, modified: meta.modified().ok() }
}

/// What [`walk`] reports.
pub enum Step<'a> {
    /// An entry: its full path, its path relative to the walked folder, its facts.
    Entry { path: &'a Path, relative: &'a Path, facts: Facts },
    Failed { path: &'a Path, error: io::Error },
}

/// Calls `visit` for everything inside `dir` (not `dir` itself), each folder before its
/// contents. `visit` returns false to stop; `walk` then returns false.
pub fn walk(dir: &Path, visit: &mut dyn FnMut(Step<'_>) -> bool) -> bool {
    // Folders still to read, relative to `dir`.
    let mut pending: Vec<PathBuf> = vec![PathBuf::new()];
    while let Some(relative_dir) = pending.pop() {
        let absolute = dir.join(&relative_dir);
        let entries = match std::fs::read_dir(&absolute) {
            Ok(entries) => entries,
            Err(error) => {
                if !visit(Step::Failed { path: &absolute, error }) {
                    return false;
                }
                continue;
            }
        };
        let mut folders = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    if !visit(Step::Failed { path: &absolute, error }) {
                        return false;
                    }
                    continue;
                }
            };
            let path = entry.path();
            let relative = relative_dir.join(entry.file_name());
            // DirEntry::metadata does not follow links.
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(error) => {
                    if !visit(Step::Failed { path: &path, error }) {
                        return false;
                    }
                    continue;
                }
            };
            let facts = facts_of(&meta);
            if !visit(Step::Entry { path: &path, relative: &relative, facts }) {
                return false;
            }
            if facts.is_dir {
                folders.push(relative);
            }
        }
        // Depth first: the first folder is read next.
        pending.extend(folders.into_iter().rev());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{test_dir, write};

    #[test]
    fn parents_come_before_their_contents() {
        let dir = test_dir("walk");
        write(&dir.join("a/b/c.txt"), "c");
        write(&dir.join("a/d.txt"), "dd");
        write(&dir.join("e.txt"), "eee");
        let mut seen: Vec<(PathBuf, bool, u64)> = Vec::new();
        assert!(walk(&dir, &mut |step| {
            if let Step::Entry { relative, facts, .. } = step {
                seen.push((relative.to_path_buf(), facts.is_dir, facts.size));
            }
            true
        }));
        let position = |p: &str| seen.iter().position(|(r, ..)| r == Path::new(p)).unwrap();
        assert_eq!(seen.len(), 5);
        assert!(position("a") < position("a/b") && position("a/b") < position("a/b/c.txt"));
        assert!(position("a") < position("a/d.txt"));
        assert_eq!(seen[position("e.txt")], (PathBuf::from("e.txt"), false, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stops_when_asked() {
        let dir = test_dir("walk-stop");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let mut count = 0;
        assert!(!walk(&dir, &mut |_| {
            count += 1;
            false
        }));
        assert_eq!(count, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 4: `control.rs`'i yaz**

```rust
//! A job's switches and counters, shared by its threads and the UI.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use gezik_core::ops::conflict::Decision;

use crate::engine::{PauseReason, lock};

#[derive(Default)]
pub(crate) struct Control {
    cancel: AtomicBool,
    paused: Mutex<Option<PauseReason>>,
    resumed: Condvar,
    decisions: Mutex<Option<Vec<Decision>>>,
    decided: Condvar,
    pub start_now: AtomicBool,
    pub running: AtomicBool,
    pub scanning: AtomicBool,
    pub deciding: AtomicBool,
    pub items_done: AtomicU64,
    pub items_total: AtomicU64,
    pub bytes_done: AtomicU64,
    pub bytes_total: AtomicU64,
    failures_in_row: AtomicU32,
}

/// How long a waiting thread sleeps before it looks at the cancel switch again.
const RECHECK: Duration = Duration::from_millis(100);

impl Control {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.resumed.notify_all();
        self.decided.notify_all();
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn pause(&self, reason: PauseReason) {
        *lock(&self.paused) = Some(reason);
    }

    pub fn resume(&self) {
        *lock(&self.paused) = None;
        self.resumed.notify_all();
    }

    pub fn pause_reason(&self) -> Option<PauseReason> {
        *lock(&self.paused)
    }

    /// Waits while paused; true once cancelled.
    pub fn stopped(&self) -> bool {
        let mut paused = lock(&self.paused);
        while paused.is_some() && !self.cancelled() {
            paused = match self.resumed.wait_timeout(paused, RECHECK) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        self.cancelled()
    }

    pub fn set_decisions(&self, decisions: Vec<Decision>) {
        *lock(&self.decisions) = Some(decisions);
        self.decided.notify_all();
    }

    /// Waits for the user's decisions; `None` if the job is cancelled meanwhile.
    pub fn wait_decisions(&self) -> Option<Vec<Decision>> {
        let mut decisions = lock(&self.decisions);
        loop {
            if let Some(chosen) = decisions.take() {
                return Some(chosen);
            }
            if self.cancelled() {
                return None;
            }
            decisions = match self.decided.wait_timeout(decisions, RECHECK) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    pub fn add_total(&self, items: u64, bytes: u64) {
        self.items_total.fetch_add(items, Ordering::Relaxed);
        self.bytes_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn item_done(&self) {
        self.items_done.fetch_add(1, Ordering::Relaxed);
    }

    /// One more failure in a row; returns how many.
    pub fn failed_once(&self) -> u32 {
        self.failures_in_row.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn succeeded(&self) {
        self.failures_in_row.store(0, Ordering::SeqCst);
    }
}
```

- [ ] **Step 5: `engine.rs`'i yaz**

```rust
//! The engine: jobs, the per-drive queue and the events the UI drains.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gezik_core::ops::conflict::{ConflictKind, Decision, Facts};
use gezik_core::ops::paths::DriveSet;
use gezik_core::ops::threads::CopyThreads;
use gezik_platform::fs::{self, DriveFacts};

use crate::control::Control;
use crate::task::{Outcome, Task, TaskKind};

pub type JobId = u64;

/// From `[files]` and the config folder.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub threads: CopyThreads,
    /// Where instant deletes note the hidden folders they have not finished; `None`: deletes
    /// run in place.
    pub pending_deletes: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    User,
    DiskFull,
    /// Many items failed in a row (a network drive went away).
    ManyFailures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Waiting for another job on the same drive.
    Waiting,
    Scanning,
    Running,
    /// Waiting for the user's conflict decisions.
    Deciding,
    Paused(PauseReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub state: JobState,
    pub items_done: u64,
    pub items_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConflictItem {
    pub source: PathBuf,
    pub target: PathBuf,
    pub kind: ConflictKind,
    pub source_facts: Facts,
    pub target_facts: Facts,
    /// What it starts with (Skip, or Merge for folders).
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub kind: TaskKind,
    pub cancelled: bool,
    pub failures: Vec<Failure>,
    /// Items an undo left alone because they changed since.
    pub skipped_changed: usize,
    /// Items not trashed because their drive has no trash: the UI offers to delete them.
    pub no_trash: Vec<PathBuf>,
    /// Where the chosen items are now (pasted, renamed, new), to select them.
    pub results: Vec<PathBuf>,
    pub changed_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `background`: an instant delete, which resumes on the next start if Gezik quits.
    Added { job: JobId, title: String, kind: TaskKind, background: bool },
    /// At most ten times a second per job, and only when something changed.
    Progress { job: JobId, progress: Progress },
    /// The job waits for `Engine::decide`, one decision per item, in this order.
    Conflicts { job: JobId, conflicts: Vec<ConflictItem> },
    /// The job waits for `Engine::resume` (or `cancel`).
    Paused { job: JobId, reason: PauseReason, path: Option<PathBuf> },
    Finished { job: JobId, report: Report },
    /// Folders whose contents changed (about once a second while a job runs, and at its end).
    Changed { dirs: Vec<PathBuf> },
    /// What Undo and Redo would do changed.
    History,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// At most this many changed folders are tracked per job.
const MAX_CHANGED: usize = 256;

/// What a job gathers while it runs.
#[derive(Default)]
pub(crate) struct Acc {
    pub outcomes: Vec<Outcome>,
    pub failures: Vec<Failure>,
    pub skipped_changed: usize,
    pub no_trash: Vec<PathBuf>,
    pub results: Vec<PathBuf>,
    pub changed: BTreeSet<PathBuf>,
    /// Changed folders not yet sent in a `Changed` event.
    pub unsent: BTreeSet<PathBuf>,
}

/// One submitted unit of work: its tasks run one after the other (an undo can be several).
pub(crate) struct Job {
    pub id: JobId,
    pub tasks: Vec<Box<dyn Task>>,
    pub control: Control,
    /// Set on the job's thread before it waits for its turn.
    pub drives: Mutex<DriveSet>,
    pub background: bool,
    pub done: AtomicBool,
    pub acc: Mutex<Acc>,
}

impl Job {
    pub fn outcome(&self, outcome: Outcome) {
        lock(&self.acc).outcomes.push(outcome);
    }

    pub fn fail(&self, path: &Path, error: &io::Error) {
        lock(&self.acc).failures.push(Failure { path: path.to_path_buf(), message: error.to_string() });
    }

    pub fn skipped_changed(&self) {
        lock(&self.acc).skipped_changed += 1;
    }

    pub fn no_trash(&self, path: &Path) {
        lock(&self.acc).no_trash.push(path.to_path_buf());
    }

    pub fn result(&self, path: PathBuf) {
        lock(&self.acc).results.push(path);
    }

    /// The folders holding `paths` changed.
    pub fn touch<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) {
        let mut acc = lock(&self.acc);
        for parent in paths.into_iter().filter_map(Path::parent) {
            if acc.changed.len() < MAX_CHANGED && acc.changed.insert(parent.to_path_buf()) {
                acc.unsent.insert(parent.to_path_buf());
            }
        }
    }

    pub fn snapshot(&self) -> Progress {
        let c = &self.control;
        let state = if let Some(reason) = c.pause_reason() {
            JobState::Paused(reason)
        } else if c.deciding.load(Ordering::SeqCst) {
            JobState::Deciding
        } else if c.scanning.load(Ordering::SeqCst) {
            JobState::Scanning
        } else if c.running.load(Ordering::SeqCst) {
            JobState::Running
        } else {
            JobState::Waiting
        };
        Progress {
            state,
            items_done: c.items_done.load(Ordering::Relaxed),
            items_total: c.items_total.load(Ordering::Relaxed),
            bytes_done: c.bytes_done.load(Ordering::Relaxed),
            bytes_total: c.bytes_total.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct Shared {
    settings: Mutex<Settings>,
    /// Jobs not finished yet, in the order they were submitted.
    jobs: Mutex<Vec<Arc<Job>>>,
    turn: Condvar,
    events: Mutex<Vec<Event>>,
    notify: Box<dyn Fn() + Send + Sync>,
    /// Drive facts by drive root.
    drives: Mutex<HashMap<PathBuf, DriveFacts>>,
    reporter: AtomicBool,
    next_id: AtomicU64,
}

impl Shared {
    pub fn push(&self, events: impl IntoIterator<Item = Event>) {
        let added = {
            let mut queue = lock(&self.events);
            let before = queue.len();
            queue.extend(events);
            queue.len() > before
        };
        if added {
            (self.notify)();
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// The facts of the drive `path` is on, cached per drive root.
    pub fn drive(&self, path: &Path) -> Option<DriveFacts> {
        let existing = fs::nearest_existing(path).unwrap_or_else(|| path.to_path_buf());
        let root = fs::drive_root(&existing).unwrap_or_else(|| existing.clone());
        if let Some(facts) = lock(&self.drives).get(&root) {
            return Some(facts.clone());
        }
        let facts = fs::drive_facts(&existing).ok()?;
        lock(&self.drives).insert(root, facts.clone());
        Some(facts)
    }

    pub fn has_trash(&self, path: &Path) -> bool {
        self.drive(path).is_some_and(|facts| facts.trash)
    }

    pub fn pause(&self, job: &Job, reason: PauseReason, path: Option<PathBuf>) {
        job.control.pause(reason);
        self.push([Event::Paused { job: job.id, reason, path }]);
    }

    /// Waits until no earlier unfinished job shares a drive with `job` (or it may start now).
    /// False if it is cancelled first.
    pub fn wait_turn(&self, job: &Job) -> bool {
        let mut jobs = lock(&self.jobs);
        loop {
            if job.control.cancelled() {
                return false;
            }
            let drives = lock(&job.drives).clone();
            let earlier = jobs.iter().take_while(|other| other.id != job.id);
            let blocked = !job.control.start_now.load(Ordering::SeqCst)
                && earlier.filter(|other| !other.done.load(Ordering::SeqCst)).any(|other| lock(&other.drives).intersects(&drives));
            if !blocked {
                job.control.running.store(true, Ordering::SeqCst);
                return true;
            }
            jobs = match self.turn.wait_timeout(jobs, Duration::from_millis(200)) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    /// Ends `job`: tells its tasks, reports, and lets waiting jobs go.
    pub fn finish(&self, job: &Arc<Job>, cancelled: bool) {
        for task in &job.tasks {
            task.done(cancelled);
        }
        let acc = std::mem::take(&mut *lock(&job.acc));
        let kind = job.tasks.first().map_or(TaskKind::Copy, |task| task.kind());
        let report = Report {
            kind,
            cancelled,
            failures: acc.failures,
            skipped_changed: acc.skipped_changed,
            no_trash: acc.no_trash,
            results: acc.results,
            changed_dirs: acc.changed.into_iter().collect(),
        };
        job.done.store(true, Ordering::SeqCst);
        lock(&self.jobs).retain(|other| other.id != job.id);
        self.turn.notify_all();
        let mut events = Vec::new();
        if !report.changed_dirs.is_empty() {
            events.push(Event::Changed { dirs: report.changed_dirs.clone() });
        }
        events.push(Event::Finished { job: job.id, report });
        self.push(events);
    }

    /// Starts the thread that reports progress ten times a second while any job runs.
    fn ensure_reporter(self: &Arc<Self>) {
        if self.reporter.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.clone();
        let spawned = std::thread::Builder::new().name("gezik-progress".into()).spawn(move || {
            let mut last: HashMap<JobId, Progress> = HashMap::new();
            let mut tick = 0u64;
            loop {
                std::thread::sleep(Duration::from_millis(100));
                tick += 1;
                let jobs = {
                    let jobs = lock(&shared.jobs);
                    if jobs.is_empty() {
                        // Under the jobs lock: a job submitted after this starts a new reporter.
                        shared.reporter.store(false, Ordering::SeqCst);
                        return;
                    }
                    jobs.clone()
                };
                let mut events = Vec::new();
                for job in &jobs {
                    let progress = job.snapshot();
                    if last.get(&job.id) != Some(&progress) {
                        last.insert(job.id, progress.clone());
                        events.push(Event::Progress { job: job.id, progress });
                    }
                    if tick.is_multiple_of(10) {
                        let dirs: Vec<PathBuf> = std::mem::take(&mut lock(&job.acc).unsent).into_iter().collect();
                        if !dirs.is_empty() {
                            events.push(Event::Changed { dirs });
                        }
                    }
                }
                last.retain(|id, _| jobs.iter().any(|job| job.id == *id));
                shared.push(events);
            }
        });
        if spawned.is_err() {
            self.reporter.store(false, Ordering::SeqCst);
        }
    }
}

#[derive(Clone)]
pub struct Engine(pub(crate) Arc<Shared>);

impl Engine {
    /// `notify` is called (on any thread) whenever events are waiting in [`Engine::drain`].
    pub fn new(settings: Settings, notify: impl Fn() + Send + Sync + 'static) -> Engine {
        Engine(Arc::new(Shared {
            settings: Mutex::new(settings),
            jobs: Mutex::default(),
            turn: Condvar::new(),
            events: Mutex::default(),
            notify: Box::new(notify),
            drives: Mutex::default(),
            reporter: AtomicBool::new(false),
            next_id: AtomicU64::new(0),
        }))
    }

    pub fn set_threads(&self, threads: CopyThreads) {
        lock(&self.0.settings).threads = threads;
    }

    pub fn submit(&self, task: Box<dyn Task>) -> JobId {
        self.start(vec![task], None)
    }

    /// Starts a job of `tasks` (run in order) on its own thread.
    pub(crate) fn start(&self, tasks: Vec<Box<dyn Task>>, title: Option<String>) -> JobId {
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(first) = tasks.first() else { return id };
        let title = title.unwrap_or_else(|| first.title());
        let kind = first.kind();
        let background = tasks.iter().all(|task| task.kind() == TaskKind::Delete);
        let job = Arc::new(Job {
            id,
            tasks,
            control: Control::default(),
            drives: Mutex::default(),
            background,
            done: AtomicBool::new(false),
            acc: Mutex::default(),
        });
        lock(&self.0.jobs).push(job.clone());
        self.0.push([Event::Added { job: id, title, kind, background }]);
        self.0.ensure_reporter();
        let (shared, running) = (self.0.clone(), job.clone());
        let spawned =
            std::thread::Builder::new().name("gezik-job".into()).spawn(move || crate::run::run_job(&shared, &running));
        if let Err(err) = spawned {
            job.fail(Path::new(""), &err);
            self.0.finish(&job, true);
        }
        id
    }

    pub fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *lock(&self.0.events))
    }

    fn job(&self, id: JobId) -> Option<Arc<Job>> {
        lock(&self.0.jobs).iter().find(|job| job.id == id).cloned()
    }

    /// The user's decisions for the job's `Conflicts` event, in its order.
    pub fn decide(&self, job: JobId, decisions: Vec<Decision>) {
        if let Some(job) = self.job(job) {
            job.control.set_decisions(decisions);
        }
    }

    pub fn pause(&self, job: JobId) {
        if let Some(job) = self.job(job)
            && job.control.pause_reason().is_none()
        {
            job.control.pause(PauseReason::User);
        }
    }

    pub fn resume(&self, job: JobId) {
        if let Some(job) = self.job(job) {
            job.control.resume();
        }
    }

    pub fn cancel(&self, job: JobId) {
        if let Some(job) = self.job(job) {
            job.control.cancel();
        }
        self.0.turn.notify_all();
    }

    /// Lets a job waiting for its drive start right away.
    pub fn start_now(&self, job: JobId) {
        if let Some(job) = self.job(job) {
            job.control.start_now.store(true, Ordering::SeqCst);
        }
        self.0.turn.notify_all();
    }

    pub fn cancel_all(&self) {
        for job in lock(&self.0.jobs).iter() {
            job.control.cancel();
        }
        self.0.turn.notify_all();
    }

    /// Whether a job runs that quitting would lose (instant deletes resume on the next start).
    pub fn busy(&self) -> bool {
        lock(&self.0.jobs).iter().any(|job| !job.background)
    }

    /// Waits until every job has finished; false if `timeout` passes first.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if lock(&self.0.jobs).is_empty() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
```

- [ ] **Step 6: `run.rs`'i yaz**

```rust
//! Running a job: wait for the drives, plan, settle conflicts, do the items.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use gezik_core::ops::conflict::{
    ConflictKind, Decision, Facts, Resolution, default_decision, kind_of, resolve,
};
use gezik_core::ops::names::next_free;
use gezik_core::ops::paths::{DriveSet, is_within};
use gezik_core::ops::threads::workers;
use gezik_platform::fs;

use crate::engine::{ConflictItem, Event, Job, PauseReason, Shared, lock};
use crate::task::{ChangedSince, NoTrash, Outcome, PlanItem, RunCx, ScanSink, Stage, Task, Work, is_marker};
use crate::walk::facts_of;

/// After this many failures in a row the job pauses and asks.
const MAX_FAILURES_IN_ROW: u32 = 20;

pub(crate) fn run_job(shared: &Shared, job: &Arc<Job>) {
    let mut ids = Vec::new();
    let mut kinds = Vec::new();
    for task in &job.tasks {
        for path in task.resources().paths {
            if let Some(facts) = shared.drive(&path) {
                ids.push(facts.id);
                kinds.push(facts.kind);
            }
        }
    }
    *lock(&job.drives) = DriveSet::new(ids);
    if shared.wait_turn(job) {
        for task in &job.tasks {
            if job.control.cancelled() {
                break;
            }
            run_task(shared, job, task.as_ref(), &kinds);
        }
    }
    shared.finish(job, job.control.cancelled());
}

fn run_task(shared: &Shared, job: &Job, task: &dyn Task, kinds: &[gezik_core::ops::threads::DiskKind]) {
    let control = &job.control;
    let count = match task.resources().work {
        Work::Disk => workers(shared.settings().threads, kinds),
        Work::Cpu => std::thread::available_parallelism().map_or(2, |n| n.get()),
        Work::External => 2,
    };
    let (sender, receiver) = mpsc::channel::<PlanItem>();
    let receiver = Mutex::new(receiver);
    let mut after: Vec<PlanItem> = Vec::new();
    std::thread::scope(|scope| {
        for _ in 0..count.max(1) {
            scope.spawn(|| {
                loop {
                    let next = lock(&receiver).recv();
                    let Ok(item) = next else { break };
                    execute(shared, job, task, item);
                }
            });
        }
        control.scanning.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut sink = Sink {
            shared,
            job,
            task,
            sender: &sender,
            after: &mut after,
            held: Vec::new(),
            merges: Vec::new(),
            renames: Vec::new(),
            blocked: Vec::new(),
            taken: HashSet::new(),
        };
        task.plan(&mut sink);
        control.scanning.store(false, std::sync::atomic::Ordering::SeqCst);
        if !sink.held.is_empty() && !control.cancelled() {
            sink.settle();
        }
        drop(sink);
        // The workers stop once the queue is empty and closed.
        drop(sender);
    });
    for item in after.into_iter().rev() {
        if control.cancelled() {
            break;
        }
        execute(shared, job, task, item);
    }
}

/// An item waiting for the user's decision, with what is inside it (a held folder).
struct Held {
    item: PlanItem,
    conflict: ConflictItem,
    children: Vec<PlanItem>,
}

/// Takes a task's plan: runs `Before` items, queues `Parallel` ones, keeps `After` ones and
/// holds conflicts.
struct Sink<'a> {
    shared: &'a Shared,
    job: &'a Job,
    task: &'a dyn Task,
    sender: &'a mpsc::Sender<PlanItem>,
    after: &'a mut Vec<PlanItem>,
    held: Vec<Held>,
    /// Folders merged into existing ones, shown with the conflicts.
    merges: Vec<ConflictItem>,
    /// Targets given a new name so far: what goes inside them follows.
    renames: Vec<(PathBuf, PathBuf)>,
    /// Held folders (target, index in `held`): what goes inside them waits with them.
    blocked: Vec<(PathBuf, usize)>,
    /// Names this task chose that may not exist on disk yet.
    taken: HashSet<PathBuf>,
}

fn conflict(item: &PlanItem, target: &Path, kind: ConflictKind, existing: Facts, decision: Decision) -> ConflictItem {
    ConflictItem {
        source: item.source.clone().unwrap_or_default(),
        target: target.to_path_buf(),
        kind,
        source_facts: item.facts,
        target_facts: existing,
        decision,
    }
}

impl ScanSink for Sink<'_> {
    fn item(&mut self, mut item: PlanItem) -> bool {
        let control = &self.job.control;
        if control.cancelled() {
            return false;
        }
        control.add_total(1, if item.facts.is_dir { 0 } else { item.facts.size });
        self.follow_renames(&mut item);
        if let Some(target) = &item.target
            && let Some(&(_, index)) = self.blocked.iter().find(|(dir, _)| is_within(target, dir))
        {
            self.held[index].children.push(item);
            return true;
        }
        if item.check_target
            && let Some(target) = item.target.clone()
            && let Ok(meta) = std::fs::symlink_metadata(&target)
        {
            let existing = facts_of(&meta);
            let kind = kind_of(item.facts, existing);
            if kind == ConflictKind::Folder && item.stage == Stage::Before {
                // The folder is there already: merge, its contents meet one by one.
                self.merges.push(conflict(&item, &target, kind, existing, Decision::Merge));
                control.item_done();
                if item.is_root {
                    self.job.result(target);
                }
                return true;
            }
            // A folder that cannot merge here (a one-step rename onto a folder).
            let kind = if kind == ConflictKind::Folder { ConflictKind::Mismatch } else { kind };
            if let Some(decision) = item.preset {
                self.apply(item, kind, existing, decision);
                return true;
            }
            let decision = default_decision(kind, item.facts, existing);
            let index = self.held.len();
            if item.facts.is_dir {
                self.blocked.push((target.clone(), index));
            }
            self.held.push(Held { conflict: conflict(&item, &target, kind, existing, decision), item, children: Vec::new() });
            return true;
        }
        self.dispatch(item);
        true
    }

    fn failed(&mut self, path: &Path, error: io::Error) {
        self.job.fail(path, &error);
    }
}

impl Sink<'_> {
    fn dispatch(&mut self, item: PlanItem) {
        match item.stage {
            Stage::Before => execute(self.shared, self.job, self.task, item),
            Stage::Parallel => {
                let _ = self.sender.send(item);
            }
            Stage::After => self.after.push(item),
        }
    }

    fn follow_renames(&self, item: &mut PlanItem) {
        let Some(target) = &item.target else { return };
        let moved = self
            .renames
            .iter()
            .find_map(|(old, new)| target.strip_prefix(old).ok().map(|rest| if rest.as_os_str().is_empty() { new.clone() } else { new.join(rest) }));
        if let Some(moved) = moved {
            item.target = Some(moved);
        }
    }

    /// A skipped item counts as done.
    fn skip(&self, item: &PlanItem) {
        let control = &self.job.control;
        control.item_done();
        if !item.facts.is_dir {
            control.add_bytes(item.facts.size);
        }
    }

    /// A free `name (n)` next to `target`.
    fn free_target(&mut self, target: &Path, is_dir: bool) -> PathBuf {
        let parent = target.parent().unwrap_or(Path::new(""));
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let taken = &self.taken;
        let free = next_free(&name, is_dir, |candidate| {
            let path = parent.join(candidate);
            taken.contains(&path) || std::fs::symlink_metadata(&path).is_ok()
        });
        let path = parent.join(free);
        self.taken.insert(path.clone());
        path
    }

    /// Settles one conflict; returns whether the item was skipped.
    fn apply(&mut self, mut item: PlanItem, kind: ConflictKind, existing: Facts, decision: Decision) -> bool {
        match resolve(decision, kind, item.facts, existing) {
            Resolution::Write => self.dispatch(item),
            Resolution::Replace => {
                item.replace = true;
                self.dispatch(item);
            }
            Resolution::Skip => {
                self.skip(&item);
                return true;
            }
            Resolution::Rename => {
                let Some(target) = item.target.clone() else { return true };
                let free = self.free_target(&target, item.facts.is_dir);
                self.renames.push((target, free.clone()));
                item.target = Some(free);
                self.dispatch(item);
            }
        }
        false
    }

    /// Shows the held conflicts and applies the user's decisions.
    fn settle(&mut self) {
        let control = &self.job.control;
        let merges = std::mem::take(&mut self.merges);
        let shown = merges.len();
        let conflicts: Vec<ConflictItem> = merges.into_iter().chain(self.held.iter().map(|h| h.conflict.clone())).collect();
        control.deciding.store(true, std::sync::atomic::Ordering::SeqCst);
        self.shared.push([Event::Conflicts { job: self.job.id, conflicts }]);
        let decisions = control.wait_decisions();
        control.deciding.store(false, std::sync::atomic::Ordering::SeqCst);
        let Some(decisions) = decisions else { return };
        let held = std::mem::take(&mut self.held);
        self.blocked.clear();
        let mut chosen = decisions.into_iter().skip(shown);
        for Held { item, conflict, children } in held {
            let decision = chosen.next().unwrap_or(conflict.decision);
            let skipped = self.apply(item, conflict.kind, conflict.target_facts, decision);
            for mut child in children {
                if skipped {
                    self.skip(&child);
                } else {
                    self.follow_renames(&mut child);
                    self.dispatch(child);
                }
            }
        }
    }
}

/// Moves an existing target out of the way for "Replace": to the trash if its drive has one.
fn replace_target(shared: &Shared, target: &Path) -> io::Result<Outcome> {
    if shared.has_trash(target) {
        return Ok(match fs::trash(target)? {
            Some(trashed) => Outcome::Trashed { original: target.to_path_buf(), trashed },
            None => Outcome::Deleted { path: target.to_path_buf() },
        });
    }
    fs::delete(target)?;
    Ok(Outcome::Deleted { path: target.to_path_buf() })
}

/// Does one item, on a worker or the planning thread.
pub(crate) fn execute(shared: &Shared, job: &Job, task: &dyn Task, item: PlanItem) {
    let control = &job.control;
    if control.stopped() {
        return;
    }
    if item.replace
        && let Some(target) = &item.target
    {
        match replace_target(shared, target) {
            Ok(outcome) => job.outcome(outcome),
            Err(err) => {
                job.fail(target, &err);
                control.item_done();
                return;
            }
        }
    }
    let has_trash = |path: &Path| shared.has_trash(path);
    let cx = RunCx { control, trash: &has_trash, added: std::cell::Cell::new(0) };
    loop {
        match task.run(&item, &cx) {
            Ok(outcome) => {
                control.succeeded();
                if item.is_root
                    && let Some(path) = outcome.result()
                {
                    job.result(path.to_path_buf());
                }
                job.touch(item.source.as_deref().into_iter().chain(item.target.as_deref()));
                job.outcome(outcome);
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted && control.cancelled() => return,
            Err(err) if fs::is_disk_full(&err) => {
                shared.pause(job, PauseReason::DiskFull, item.target.clone());
                if control.stopped() {
                    return;
                }
                // Resumed: try the same item again.
            }
            Err(err) if is_marker::<ChangedSince>(&err) => {
                job.skipped_changed();
                break;
            }
            Err(err) if is_marker::<NoTrash>(&err) => {
                job.no_trash(item.path());
                break;
            }
            Err(err) => {
                job.fail(item.path(), &err);
                if control.failed_once() >= MAX_FAILURES_IN_ROW {
                    control.succeeded();
                    shared.pause(job, PauseReason::ManyFailures, None);
                    let _ = control.stopped();
                }
                break;
            }
        }
    }
    control.item_done();
    // Whatever the task did not count itself (a rename, a delete) is done now too.
    if !item.facts.is_dir {
        control.add_bytes(item.facts.size.saturating_sub(cx.added.get()));
    }
}
```

- [ ] **Step 7: Test yardımcılarını ve motor testlerini yaz**

`crates/gezik-ops/src/testing.rs`:

```rust
//! Helpers for the engine's tests.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::engine::{ConflictItem, Engine, Event, JobId, Report, Settings, lock};
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

/// A fresh, empty folder.
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gezik-ops-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `text` to `path`, making its folders.
pub(crate) fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub(crate) fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

pub(crate) fn engine() -> Engine {
    Engine::new(Settings::default(), || {})
}

/// Each conflict's starting decision.
pub(crate) fn defaults(conflicts: &[ConflictItem]) -> Vec<Decision> {
    conflicts.iter().map(|c| c.decision).collect()
}

/// Waits until `job` finishes, answering its conflicts with `decide`; returns the report and
/// every event seen for it on the way.
pub(crate) fn finish(engine: &Engine, job: JobId, decide: impl Fn(&[ConflictItem]) -> Vec<Decision>) -> (Report, Vec<Event>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut seen = Vec::new();
    loop {
        for event in engine.drain() {
            match &event {
                Event::Conflicts { job: j, conflicts } if *j == job => engine.decide(job, decide(conflicts)),
                Event::Finished { job: j, report } if *j == job => {
                    let report = report.clone();
                    seen.push(event);
                    return (report, seen);
                }
                _ => {}
            }
            seen.push(event);
        }
        assert!(Instant::now() < deadline, "job {job} did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A gate the fake task's items wait at until it opens.
#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    pub fn open(&self) {
        *lock(&self.0.0) = true;
        self.0.1.notify_all();
    }

    fn wait(&self) {
        let mut open = lock(&self.0.0);
        while !*open {
            open = self.0.1.wait(open).unwrap();
        }
    }
}

/// A task for testing the engine itself: `items` items that log, wait at a gate, fail, or
/// find the disk full once.
pub(crate) struct FakeTask {
    pub name: &'static str,
    pub items: usize,
    /// Drive paths it claims (none: it shares no drive with anything).
    pub paths: Vec<PathBuf>,
    pub gate: Option<Gate>,
    pub fail: Vec<usize>,
    pub disk_full_once: Mutex<Vec<usize>>,
    pub log: Arc<Mutex<Vec<String>>>,
    pub running: Arc<AtomicUsize>,
}

impl FakeTask {
    pub fn new(name: &'static str, items: usize, log: &Arc<Mutex<Vec<String>>>) -> FakeTask {
        FakeTask {
            name,
            items,
            paths: Vec::new(),
            gate: None,
            fail: Vec::new(),
            disk_full_once: Mutex::default(),
            log: log.clone(),
            running: Arc::default(),
        }
    }
}

impl Task for FakeTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        format!("fake {}", self.name)
    }

    fn count(&self) -> usize {
        self.items
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths.clone(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for i in 0..self.items {
            let facts = Facts { is_dir: false, size: 10, modified: None };
            if !sink.item(PlanItem::new(Stage::Parallel, facts).top(i)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        self.running.fetch_add(1, Ordering::SeqCst);
        lock(&self.log).push(format!("{}+{}", self.name, item.root));
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        lock(&self.log).push(format!("{}-{}", self.name, item.root));
        self.running.fetch_sub(1, Ordering::SeqCst);
        if self.fail.contains(&item.root) {
            return Err(io::Error::other("boom"));
        }
        let mut full = lock(&self.disk_full_once);
        if let Some(at) = full.iter().position(|&i| i == item.root) {
            full.remove(at);
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        Ok(Outcome::Nothing)
    }
}
```

`crates/gezik-ops/src/engine.rs` sonuna testleri ekle:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeTask, Gate, defaults, engine, finish, test_dir};
    use std::sync::atomic::AtomicUsize;

    fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_job_reports_added_progress_and_finished() {
        let engine = engine();
        let log = Arc::default();
        let job = engine.submit(Box::new(FakeTask::new("a", 3, &log)));
        let (report, events) = finish(&engine, job, defaults);
        assert!(matches!(events.first(), Some(Event::Added { title, background: false, .. }) if title == "fake a"));
        assert!(!report.cancelled && report.failures.is_empty());
        assert_eq!(lock(&log).len(), 6);
        assert!(engine.wait_idle(Duration::from_secs(5)));
        assert!(!engine.busy());
    }

    #[test]
    fn jobs_on_the_same_drive_wait_their_turn() {
        let dir = test_dir("queue");
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let mut first = FakeTask::new("a", 1, &log);
        first.paths = vec![dir.clone()];
        first.gate = Some(gate.clone());
        let mut second = FakeTask::new("b", 1, &log);
        second.paths = vec![dir.clone()];
        let a = engine.submit(Box::new(first));
        let b = engine.submit(Box::new(second));
        wait_for("a to start", || lock(&log).contains(&"a+0".to_owned()));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!lock(&log).iter().any(|e| e.starts_with('b')), "b waits for a: {:?}", lock(&log));
        gate.open();
        finish(&engine, a, defaults);
        finish(&engine, b, defaults);
        assert_eq!(*lock(&log), ["a+0", "a-0", "b+0", "b-0"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jobs_without_a_shared_drive_run_together() {
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let running = Arc::new(AtomicUsize::new(0));
        let make = |name| {
            let mut task = FakeTask::new(name, 1, &log);
            task.gate = Some(gate.clone());
            task.running = running.clone();
            task
        };
        let a = engine.submit(Box::new(make("a")));
        let b = engine.submit(Box::new(make("b")));
        wait_for("both to run at once", || running.load(Ordering::SeqCst) == 2);
        gate.open();
        finish(&engine, a, defaults);
        finish(&engine, b, defaults);
    }

    #[test]
    fn start_now_skips_the_queue() {
        let dir = test_dir("start-now");
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let mut first = FakeTask::new("a", 1, &log);
        first.paths = vec![dir.clone()];
        first.gate = Some(gate.clone());
        let mut second = FakeTask::new("b", 1, &log);
        second.paths = vec![dir.clone()];
        let a = engine.submit(Box::new(first));
        let b = engine.submit(Box::new(second));
        wait_for("a to start", || lock(&log).contains(&"a+0".to_owned()));
        engine.start_now(b);
        finish(&engine, b, defaults);
        gate.open();
        finish(&engine, a, defaults);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pause_holds_and_cancel_ends_a_job() {
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let mut task = FakeTask::new("a", 50, &log);
        task.gate = Some(gate.clone());
        let job = engine.submit(Box::new(task));
        wait_for("the first item", || !lock(&log).is_empty());
        engine.pause(job);
        gate.open();
        std::thread::sleep(Duration::from_millis(150));
        let done = lock(&log).len();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(lock(&log).len(), done, "nothing new starts while paused");
        engine.cancel(job);
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.cancelled);
        assert!(lock(&log).len() < 100, "not every item ran");
    }

    #[test]
    fn many_failures_in_a_row_pause_the_job_until_resumed() {
        let engine = engine();
        engine.set_threads(CopyThreads::Fixed(1));
        let log = Arc::default();
        let mut task = FakeTask::new("a", 30, &log);
        task.fail = (0..30).collect();
        let job = engine.submit(Box::new(task));
        let deadline = Instant::now() + Duration::from_secs(10);
        let report = loop {
            let mut finished = None;
            for event in engine.drain() {
                match event {
                    Event::Paused { job: j, reason: PauseReason::ManyFailures, .. } if j == job => {
                        assert_eq!(lock(&log).len(), 40, "paused after 20 failures");
                        engine.resume(job);
                    }
                    Event::Finished { job: j, report } if j == job => finished = Some(report),
                    _ => {}
                }
            }
            if let Some(report) = finished {
                break report;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(report.failures.len(), 30);
    }

    #[test]
    fn a_full_disk_pauses_and_the_item_is_tried_again() {
        let engine = engine();
        engine.set_threads(CopyThreads::Fixed(1));
        let log = Arc::default();
        let task = FakeTask::new("a", 3, &log);
        *lock(&task.disk_full_once) = vec![1];
        let job = engine.submit(Box::new(task));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut paused = 0;
        let report = loop {
            let mut finished = None;
            for event in engine.drain() {
                match event {
                    Event::Paused { job: j, reason: PauseReason::DiskFull, .. } if j == job => {
                        paused += 1;
                        engine.resume(job);
                    }
                    Event::Finished { job: j, report } if j == job => finished = Some(report),
                    _ => {}
                }
            }
            if let Some(report) = finished {
                break report;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(paused, 1);
        assert!(report.failures.is_empty());
        assert_eq!(lock(&log).iter().filter(|e| *e == "a+1").count(), 2, "item 1 ran twice");
    }
}
```

- [ ] **Step 8: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-ops`
Expected: `task`, `walk`, `engine` testleri PASS.
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock crates/gezik-ops
git commit -m "File operations engine: tasks, per-drive queue, conflicts held for decisions, pause and cancel"
```

---

### Task 9: Kopyala, çoğalt, taşı, yeniden adlandır (`gezik-ops/src/tasks`)

**Files:**
- Create: `crates/gezik-ops/src/tasks/mod.rs`
- Create: `crates/gezik-ops/src/tasks/copy.rs`
- Create: `crates/gezik-ops/src/tasks/move_.rs`
- Modify: `crates/gezik-ops/src/lib.rs`

**Interfaces:**
- Consumes: Task 8'in `Task`, `PlanItem`, `ScanSink`, `RunCx`, `walk`, `facts_after`, `unchanged`, `changed_since`; `gezik_platform::fs::{move_entry, delete, drive_facts, nearest_existing}`
- Produces:
  - `gezik_ops::CopyTask::into(sources: Vec<PathBuf>, dir: &Path) -> CopyTask` (kaynağın kendi klasörüne kopyada `ad (2)`), `CopyTask::duplicate(sources: Vec<PathBuf>) -> CopyTask`
  - `gezik_ops::MoveTask::into(sources: Vec<PathBuf>, dir: &Path) -> MoveTask`, `MoveTask::rename(path: PathBuf, name: &str) -> MoveTask`, `pub(crate) MoveTask::back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> MoveTask` (Task 11)
  - `tasks::{what(&[PathBuf]) -> String, name(&Path) -> String, same_drive(&Path, &Path) -> bool}` (crate içi)

- [ ] **Step 1: Ortak yardımcıları yaz**

`crates/gezik-ops/src/tasks/mod.rs`:

```rust
//! The file operations the engine runs.

mod copy;
mod move_;

pub use copy::CopyTask;
pub use move_::MoveTask;

use std::path::{Path, PathBuf};

use gezik_platform::fs;

/// `rapor.pdf`, or `3 items`.
pub(crate) fn what(paths: &[PathBuf]) -> String {
    match paths {
        [one] => name(one),
        many => format!("{} items", many.len()),
    }
}

/// The last part of `path` (the whole path for a drive root).
pub(crate) fn name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Whether `a` and `b` (or their nearest existing ancestors) are on the same drive.
pub(crate) fn same_drive(a: &Path, b: &Path) -> bool {
    let id = |path: &Path| fs::nearest_existing(path).and_then(|p| fs::drive_facts(&p).ok()).map(|facts| facts.id);
    matches!((id(a), id(b)), (Some(x), Some(y)) if x == y)
}
```

`crates/gezik-ops/src/lib.rs` içine `mod task;` satırından sonra ekle ve dışa aç:

```rust
mod tasks;
```

```rust
pub use tasks::{CopyTask, MoveTask};
```

- [ ] **Step 2: `copy.rs`'i testleriyle yaz**

```rust
//! Copying files and folders, also next to themselves (Duplicate).

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_core::ops::paths::{is_within, same_path};

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::{Step, facts_of, walk};

pub struct CopyTask {
    /// (source, target) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// Keep both without asking: a copy into the source's own folder.
    presets: Vec<Option<Decision>>,
    /// Where they go, for the title; `None` for Duplicate.
    dir: Option<PathBuf>,
}

impl CopyTask {
    /// Copies `sources` into `dir`. A source already in `dir` becomes `name (2)`.
    pub fn into(sources: Vec<PathBuf>, dir: &Path) -> CopyTask {
        let presets = sources
            .iter()
            .map(|source| source.parent().is_some_and(|parent| same_path(parent, dir)).then_some(Decision::KeepBoth))
            .collect();
        let pairs = sources
            .into_iter()
            .map(|source| {
                let target = dir.join(source.file_name().unwrap_or_default());
                (source, target)
            })
            .collect();
        CopyTask { pairs, presets, dir: Some(dir.to_path_buf()) }
    }

    /// Copies each source next to itself as `name (2)`.
    pub fn duplicate(sources: Vec<PathBuf>) -> CopyTask {
        let presets = vec![Some(Decision::KeepBoth); sources.len()];
        CopyTask { pairs: sources.into_iter().map(|source| (source.clone(), source)).collect(), presets, dir: None }
    }

    fn sources(&self) -> Vec<PathBuf> {
        self.pairs.iter().map(|(source, _)| source.clone()).collect()
    }
}

/// A folder copied into itself would copy forever.
fn into_itself(source: &Path, target: &Path, facts: Facts) -> bool {
    facts.is_dir && is_within(target, source) && !same_path(target, source)
}

impl Task for CopyTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        match &self.dir {
            Some(dir) => format!("Copying {} to {}", what(&self.sources()), dir.display()),
            None => format!("Duplicating {}", what(&self.sources())),
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.sources();
        paths.extend(self.pairs.iter().filter_map(|(_, target)| target.parent().map(Path::to_path_buf)));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, ((source, target), preset)) in self.pairs.iter().zip(&self.presets).enumerate() {
            let meta = match std::fs::symlink_metadata(source) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let facts = facts_of(&meta);
            if into_itself(source, target, facts) {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, "Cannot copy a folder into itself"));
                continue;
            }
            let stage = if facts.is_dir { Stage::Before } else { Stage::Parallel };
            let item = PlanItem::new(stage, facts).source(source).target(target).checked().top(root).preset(*preset);
            if !sink.item(item) {
                return;
            }
            if facts.is_dir && !plan_tree(sink, source, target, root) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        if item.facts.is_dir {
            return match std::fs::create_dir(target) {
                Ok(()) => Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, true), from: None }),
                // Merged into a folder that was already there.
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => Ok(Outcome::Nothing),
                Err(err) => Err(err),
            };
        }
        cx.copy_file(source, target, item.facts.size)?;
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, false), from: None })
    }
}

/// Plans copying what is inside `source` into `target`; false once the job is cancelled.
fn plan_tree(sink: &mut dyn ScanSink, source: &Path, target: &Path, root: usize) -> bool {
    walk(source, &mut |step| match step {
        Step::Entry { path, relative, facts } => {
            let stage = if facts.is_dir { Stage::Before } else { Stage::Parallel };
            sink.item(PlanItem::new(stage, facts).source(path).target(target.join(relative)).checked().under(root))
        }
        Step::Failed { path, error } => {
            sink.failed(path, error);
            true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};
    use gezik_core::ops::conflict::ConflictKind;

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    #[test]
    fn copies_a_tree() {
        let dir = test_dir("copy-tree");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a/b/c.txt"), "c");
        write(&src.join("a/d.txt"), "dd");
        std::fs::create_dir_all(src.join("a/empty")).unwrap();
        std::fs::create_dir(&dst).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a")], &dst)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("a/b/c.txt")), "c");
        assert_eq!(read(&dst.join("a/d.txt")), "dd");
        assert!(dst.join("a/empty").is_dir());
        assert_eq!(read(&src.join("a/d.txt")), "dd", "the source stays");
        assert_eq!(report.results, [dst.join("a")]);
        assert!(report.changed_dirs.contains(&dst));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_into_itself_fails() {
        let dir = test_dir("copy-itself");
        write(&dir.join("a/x.txt"), "x");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![dir.join("a")], &dir.join("a"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.contains("into itself"), "{}", report.failures[0].message);
        assert!(!dir.join("a/a").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paste_into_the_same_folder_numbers_the_copy() {
        let dir = test_dir("copy-same");
        write(&dir.join("x.txt"), "x");
        write(&dir.join("x (2).txt"), "taken");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![dir.join("x.txt")], &dir)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert_eq!(read(&dir.join("x (3).txt")), "x");
        assert_eq!(report.results, [dir.join("x (3).txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_numbers_a_folder_and_keeps_its_contents_inside() {
        let dir = test_dir("duplicate");
        write(&dir.join("f/a.txt"), "a");
        write(&dir.join("f/sub/b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::duplicate(vec![dir.join("f")])));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("f (2)/a.txt")), "a");
        assert_eq!(read(&dir.join("f (2)/sub/b.txt")), "b");
        assert!(!dir.join("f/f").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conflicts_are_asked_once_and_skip_is_the_default() {
        let dir = test_dir("copy-conflict");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a.txt"), "new");
        write(&src.join("b.txt"), "b");
        write(&dst.join("a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a.txt"), src.join("b.txt")], &dst)));
        let (_, events) = finish(&engine, job, |conflicts| {
            assert_eq!(conflicts.len(), 1);
            assert_eq!((conflicts[0].kind, conflicts[0].decision), (ConflictKind::File, Decision::Skip));
            assert_eq!(conflicts[0].target, dst.join("a.txt"));
            defaults(conflicts)
        });
        assert_eq!(events.iter().filter(|e| matches!(e, crate::Event::Conflicts { .. })).count(), 1);
        assert_eq!(read(&dst.join("a.txt")), "old");
        assert_eq!(read(&dst.join("b.txt")), "b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_both_names_the_new_copy() {
        let dir = test_dir("copy-keep-both");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a.txt"), "new");
        write(&dst.join("a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a.txt")], &dst)));
        finish(&engine, job, |c| vec![Decision::KeepBoth; c.len()]);
        assert_eq!(read(&dst.join("a.txt")), "old");
        assert_eq!(read(&dst.join("a (2).txt")), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_writes_the_new_file() {
        let dir = test_dir("copy-replace");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a.txt"), "new");
        write(&dst.join("a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a.txt")], &dst)));
        let (report, _) = finish(&engine, job, |c| vec![Decision::Replace; c.len()]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("a.txt")), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_meeting_a_file_waits_with_its_contents() {
        let dir = test_dir("copy-mismatch");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("x/1.txt"), "1");
        write(&src.join("x/2.txt"), "2");
        write(&dst.join("x"), "a file");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("x")], &dst)));
        let (report, _) = finish(&engine, job, |conflicts| {
            assert_eq!(conflicts.len(), 1, "the folder only, not what is inside it");
            assert_eq!(conflicts[0].kind, ConflictKind::Mismatch);
            vec![Decision::KeepBoth]
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("x")), "a file");
        assert_eq!(read(&dst.join("x (2)/1.txt")), "1");
        assert_eq!(read(&dst.join("x (2)/2.txt")), "2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skipping_a_held_folder_skips_its_contents() {
        let dir = test_dir("copy-mismatch-skip");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("x/1.txt"), "1");
        write(&dst.join("x"), "a file");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("x")], &dst)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("x")), "a file");
        assert!(!dst.join("x (2)").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folders_merge_without_asking() {
        let dir = test_dir("copy-merge");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("d/new.txt"), "new");
        write(&dst.join("d/old.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("d")], &dst)));
        finish(&engine, job, no_conflicts);
        assert_eq!(read(&dst.join("d/new.txt")), "new");
        assert_eq!(read(&dst.join("d/old.txt")), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 3: `move_.rs`'i testleriyle yaz**

```rust
//! Moving and renaming: one rename on the same drive; across drives each file is copied, then
//! its original removed.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::{is_within, same_path};
use gezik_platform::fs;

use super::{name, same_drive, what};
use crate::task::{
    Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since, facts_after, unchanged,
};
use crate::walk::{Step, facts_of, walk};

const RENAME: u8 = 0;
/// Only the case of the name changes: through a temporary name.
const CASE: u8 = 1;
const COPY_DELETE: u8 = 2;
const MKDIR: u8 = 3;
const RMDIR: u8 = 4;

pub struct MoveTask {
    /// (where it is, where it goes) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// How each chosen item must still look (undo); `None`: anything.
    expect: Vec<Option<Facts>>,
    kind: TaskKind,
    /// Undo: missing parent folders are made again.
    back: bool,
}

impl MoveTask {
    pub fn into(sources: Vec<PathBuf>, dir: &Path) -> MoveTask {
        let pairs: Vec<(PathBuf, PathBuf)> = sources
            .into_iter()
            .map(|source| {
                let target = dir.join(source.file_name().unwrap_or_default());
                (source, target)
            })
            .collect();
        MoveTask { expect: vec![None; pairs.len()], pairs, kind: TaskKind::Move, back: false }
    }

    /// Renames `path` to `name` in its folder.
    pub fn rename(path: PathBuf, name: &str) -> MoveTask {
        let target = path.with_file_name(name);
        MoveTask { pairs: vec![(path, target)], expect: vec![None], kind: TaskKind::Rename, back: false }
    }

    /// Moves items back where they came from: (where it is, where it was, how it must look).
    pub(crate) fn back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> MoveTask {
        let (pairs, expect) = items.into_iter().map(|(now, was, facts)| ((now, was), facts)).unzip();
        MoveTask { pairs, expect, kind: TaskKind::Move, back: true }
    }

    fn sources(&self) -> Vec<PathBuf> {
        self.pairs.iter().map(|(source, _)| source.clone()).collect()
    }

    fn check(&self, item: &PlanItem, source: &Path) -> io::Result<()> {
        if item.is_root && !unchanged(source, self.expect.get(item.root).copied().flatten()) {
            return Err(changed_since());
        }
        Ok(())
    }

    fn make_parent(&self, target: &Path) -> io::Result<()> {
        if self.back
            && let Some(parent) = target.parent()
        {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl Task for MoveTask {
    fn kind(&self) -> TaskKind {
        self.kind
    }

    fn title(&self) -> String {
        let sources = self.sources();
        match (self.kind, self.pairs.first()) {
            (TaskKind::Rename, Some((from, to))) => format!("Renaming {} to {}", name(from), name(to)),
            _ if self.back => format!("Moving {} back", what(&sources)),
            (_, Some((_, to))) => {
                let dir = to.parent().map(|p| p.display().to_string()).unwrap_or_default();
                format!("Moving {} to {dir}", what(&sources))
            }
            (_, None) => "Moving".to_owned(),
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.sources();
        paths.extend(self.pairs.iter().filter_map(|(_, target)| target.parent().map(Path::to_path_buf)));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, (source, target)) in self.pairs.iter().enumerate() {
            if source == target {
                continue;
            }
            let meta = match std::fs::symlink_metadata(source) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let facts = facts_of(&meta);
            let planned = if same_path(source, target) {
                sink.item(PlanItem::new(Stage::Parallel, facts).source(source).target(target).top(root).tag(CASE))
            } else if facts.is_dir && is_within(target, source) {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, "Cannot move a folder into itself"));
                true
            } else if same_drive(source, target.parent().unwrap_or(target)) {
                plan_rename(sink, source, target, facts, root, true)
            } else {
                plan_cross(sink, source, target, facts, root)
            };
            if !planned {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(source) = &item.source else { return Ok(Outcome::Nothing) };
        if item.tag == RMDIR {
            return match fs::delete(source) {
                Ok(()) => Ok(Outcome::Deleted { path: source.clone() }),
                // Something inside was skipped or failed: the folder stays.
                Err(err) if matches!(err.kind(), io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound) => {
                    Ok(Outcome::Nothing)
                }
                Err(err) => Err(err),
            };
        }
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        match item.tag {
            RENAME => {
                self.check(item, source)?;
                self.make_parent(target)?;
                fs::move_entry(source, target)?;
                Ok(Outcome::Moved { from: source.clone(), to: target.clone(), facts: facts_after(target, item.facts.is_dir) })
            }
            CASE => {
                let temp = source.with_file_name(format!(".gezik-rename-{}", std::process::id()));
                fs::move_entry(source, &temp)?;
                if let Err(err) = fs::move_entry(&temp, target) {
                    let _ = fs::move_entry(&temp, source);
                    return Err(err);
                }
                Ok(Outcome::Moved { from: source.clone(), to: target.clone(), facts: facts_after(target, item.facts.is_dir) })
            }
            COPY_DELETE => {
                self.check(item, source)?;
                self.make_parent(target)?;
                cx.copy_file(source, target, item.facts.size)?;
                let facts = facts_after(target, false);
                if let Err(err) = fs::delete(source) {
                    return Err(io::Error::new(err.kind(), format!("Copied, but could not remove the original: {err}")));
                }
                Ok(Outcome::Created { path: target.clone(), facts, from: Some(source.clone()) })
            }
            MKDIR => {
                self.make_parent(target)?;
                match std::fs::create_dir(target) {
                    Ok(()) => Ok(Outcome::Created {
                        path: target.clone(),
                        facts: facts_after(target, true),
                        from: Some(source.clone()),
                    }),
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => Ok(Outcome::Nothing),
                    Err(err) => Err(err),
                }
            }
            _ => Ok(Outcome::Nothing),
        }
    }
}

/// Same drive: one rename, or, onto a folder that is already there, its contents one by one
/// and then the emptied source folder removed. False once the job is cancelled.
fn plan_rename(sink: &mut dyn ScanSink, source: &Path, target: &Path, facts: Facts, root: usize, top: bool) -> bool {
    if facts.is_dir && std::fs::symlink_metadata(target).is_ok_and(|meta| meta.is_dir()) {
        let entries = match std::fs::read_dir(source) {
            Ok(entries) => entries,
            Err(err) => {
                sink.failed(source, err);
                return true;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let child = entry.path();
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(&child, err);
                    continue;
                }
            };
            if !plan_rename(sink, &child, &target.join(entry.file_name()), facts_of(&meta), root, false) {
                return false;
            }
        }
        return sink.item(PlanItem::new(Stage::After, facts).source(source).under(root).tag(RMDIR));
    }
    let item = PlanItem::new(Stage::Parallel, facts).source(source).target(target).checked().tag(RENAME);
    sink.item(if top { item.top(root) } else { item.under(root) })
}

/// Another drive: folders made first, files copied then removed, emptied folders removed last.
fn plan_cross(sink: &mut dyn ScanSink, source: &Path, target: &Path, facts: Facts, root: usize) -> bool {
    if !facts.is_dir {
        return sink.item(PlanItem::new(Stage::Parallel, facts).source(source).target(target).checked().top(root).tag(COPY_DELETE));
    }
    if !sink.item(PlanItem::new(Stage::Before, facts).source(source).target(target).checked().top(root).tag(MKDIR))
        || !sink.item(PlanItem::new(Stage::After, facts).source(source).under(root).tag(RMDIR))
    {
        return false;
    }
    walk(source, &mut |step| match step {
        Step::Entry { path, relative, facts } => {
            let target = target.join(relative);
            if facts.is_dir {
                sink.item(PlanItem::new(Stage::Before, facts).source(path).target(&target).checked().under(root).tag(MKDIR))
                    && sink.item(PlanItem::new(Stage::After, facts).source(path).under(root).tag(RMDIR))
            } else {
                sink.item(PlanItem::new(Stage::Parallel, facts).source(path).target(target).checked().under(root).tag(COPY_DELETE))
            }
        }
        Step::Failed { path, error } => {
            sink.failed(path, error);
            true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};
    use gezik_core::ops::conflict::Decision;

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    #[test]
    fn a_move_on_the_same_drive_renames() {
        let dir = test_dir("move-rename");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/a.txt")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/a.txt")), "a");
        assert!(!dir.join("src/a.txt").exists());
        assert_eq!(report.results, [dir.join("dst/a.txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_onto_itself_does_nothing() {
        let dir = test_dir("move-itself");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("a.txt")], &dir)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty());
        assert_eq!(read(&dir.join("a.txt")), "a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_a_folder_into_itself_fails() {
        let dir = test_dir("move-into-itself");
        write(&dir.join("f/sub/x.txt"), "x");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("f")], &dir.join("f/sub"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(read(&dir.join("f/sub/x.txt")), "x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_into_an_existing_folder_merges() {
        let dir = test_dir("move-merge");
        write(&dir.join("src/d/a.txt"), "a");
        write(&dir.join("dst/d/b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/d/a.txt")), "a");
        assert_eq!(read(&dir.join("dst/d/b.txt")), "b");
        assert!(!dir.join("src/d").exists(), "the emptied source folder is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skipped_file_keeps_its_folder() {
        let dir = test_dir("move-merge-skip");
        write(&dir.join("src/d/a.txt"), "new");
        write(&dir.join("dst/d/a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/d/a.txt")), "old");
        assert_eq!(read(&dir.join("src/d/a.txt")), "new", "skipped: still where it was");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_changes_the_name_and_also_only_its_case() {
        let dir = test_dir("rename");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::rename(dir.join("a.txt"), "b.txt")));
        finish(&engine, job, no_conflicts);
        assert_eq!(read(&dir.join("b.txt")), "a");
        let job = engine.submit(Box::new(MoveTask::rename(dir.join("b.txt"), "B.txt")));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["B.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renaming_onto_a_taken_name_asks() {
        let dir = test_dir("rename-taken");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::rename(dir.join("a.txt"), "b.txt")));
        let (_, events) = finish(&engine, job, defaults);
        assert!(events.iter().any(|e| matches!(e, crate::Event::Conflicts { .. })));
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

(Farklı sürücü testi otomatik değildir: makinede ikinci bir yazılabilir sürücü olduğu bilinmez. Elle test listesinde yer alır, bkz. Task 16.)

- [ ] **Step 4: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-ops tasks::`
Expected: `copy` ve `move_` testleri PASS.
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 5: Commit**

```bash
git add crates/gezik-ops
git commit -m "Copy, duplicate, move and rename tasks: merges, held folders, numbered copies"
```

---

### Task 10: Çöpe at, anında kalıcı sil ve toparlama, geri getir, yeni klasör/dosya

**Files:**
- Create: `crates/gezik-ops/src/pending.rs`
- Create: `crates/gezik-ops/src/tasks/trash.rs`
- Create: `crates/gezik-ops/src/tasks/delete.rs`
- Create: `crates/gezik-ops/src/tasks/restore.rs`
- Create: `crates/gezik-ops/src/tasks/new.rs`
- Modify: `crates/gezik-ops/src/tasks/mod.rs`
- Modify: `crates/gezik-ops/src/engine.rs` (`pending` alanı, `pending_deletes()`, `recover_deletes()`)
- Modify: `crates/gezik-ops/src/lib.rs`
- Modify: `crates/gezik-ops/src/testing.rs` (`CollectSink`)

**Interfaces:**
- Consumes: Task 8–9; `gezik_platform::fs::{trash, restore, delete, move_entry, set_hidden}`
- Produces:
  - `gezik_ops::TrashTask::new(paths: Vec<PathBuf>) -> TrashTask`, `pub(crate) TrashTask::checked(items: Vec<(PathBuf, Option<Facts>)>) -> TrashTask` (geri alma: değişmiş olanı atlar; çöpsüz sürücüde boş klasörü/boş dosyayı siler)
  - `gezik_ops::DeleteTask::new(paths: Vec<PathBuf>, pending: Option<Arc<PendingDeletes>>) -> DeleteTask`
  - `gezik_ops::RestoreTask::new(pairs: Vec<(PathBuf, PathBuf)>) -> RestoreTask` ((çöpteki yol, asıl yer))
  - `gezik_ops::NewTask::folder(dir: &Path) -> NewTask`, `NewTask::file(dir: &Path) -> NewTask`
  - `gezik_ops::PendingDeletes` (`new(PathBuf)`, `add(&Path) -> io::Result<()>`, `remove(&Path)`, `load() -> Vec<PathBuf>`), `pending::{HIDDEN_PREFIX, is_hidden(&Path) -> bool, hidden_name() -> String}`
  - `Engine::pending_deletes(&self) -> Option<Arc<PendingDeletes>>`, `Engine::recover_deletes(&self) -> Option<JobId>`

- [ ] **Step 1: `pending.rs`'i testleriyle yaz**

```rust
//! `pending-deletes`: the hidden folders an instant delete has not finished, one path per
//! line. Read at start, so a delete cut short (Gezik closed or crashed) finishes later.

use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::lock;

/// How the folders an instant delete hides are named.
pub const HIDDEN_PREFIX: &str = ".gezik-deleting-";

pub struct PendingDeletes {
    file: PathBuf,
    guard: Mutex<()>,
}

impl PendingDeletes {
    pub fn new(file: PathBuf) -> PendingDeletes {
        PendingDeletes { file, guard: Mutex::new(()) }
    }

    pub fn add(&self, path: &Path) -> io::Result<()> {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        if !list.iter().any(|p| p == path) {
            list.push(path.to_path_buf());
        }
        self.write(&list)
    }

    pub fn remove(&self, path: &Path) {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        let before = list.len();
        list.retain(|p| p != path);
        if list.len() != before {
            let _ = self.write(&list);
        }
    }

    /// The listed folders Gezik itself hid; anything else in the file is ignored, so a damaged
    /// or edited file can never make Gezik delete other things.
    pub fn load(&self) -> Vec<PathBuf> {
        let _guard = lock(&self.guard);
        self.read().into_iter().filter(|path| is_hidden(path)).collect()
    }

    fn read(&self) -> Vec<PathBuf> {
        std::fs::read_to_string(&self.file)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    fn write(&self, list: &[PathBuf]) -> io::Result<()> {
        if list.is_empty() {
            return match std::fs::remove_file(&self.file) {
                Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
                _ => Ok(()),
            };
        }
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text: String = list.iter().map(|path| format!("{}\n", path.display())).collect();
        let temp = self.file.with_extension("tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.file)
    }
}

/// Whether `path` is an absolute path to a folder an instant delete hid.
pub fn is_hidden(path: &Path) -> bool {
    path.is_absolute()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.len() > HIDDEN_PREFIX.len() && name.starts_with(HIDDEN_PREFIX))
}

/// A fresh hidden name.
pub fn hidden_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    hasher.write_u64(nanos);
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{HIDDEN_PREFIX}{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_dir;

    #[test]
    fn adds_removes_and_deletes_the_file_when_empty() {
        let dir = test_dir("pending");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let (a, b) = (dir.join(hidden_name()), dir.join(hidden_name()));
        assert_ne!(a, b);
        pending.add(&a).unwrap();
        pending.add(&b).unwrap();
        pending.add(&a).unwrap();
        assert_eq!(pending.load(), [a.clone(), b.clone()]);
        pending.remove(&a);
        assert_eq!(pending.load(), [b.clone()]);
        pending.remove(&b);
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_gezik_hidden_folders_are_loaded() {
        let dir = test_dir("pending-foreign");
        let file = dir.join("pending-deletes");
        let ours = dir.join(format!("{HIDDEN_PREFIX}abc"));
        let text = format!("{}\n{}\n{HIDDEN_PREFIX}relative\n\n{}\n", dir.join("Documents").display(), ours.display(), dir.join(HIDDEN_PREFIX).display());
        std::fs::write(&file, text).unwrap();
        assert_eq!(PendingDeletes::new(file).load(), [ours]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: `CollectSink` test yardımcısını ekle**

`crates/gezik-ops/src/testing.rs` sonuna:

```rust
/// Keeps a task's plan, for testing a task without the engine.
#[derive(Default)]
pub(crate) struct CollectSink {
    pub items: Vec<PlanItem>,
    pub failed: Vec<PathBuf>,
}

impl ScanSink for CollectSink {
    fn item(&mut self, item: PlanItem) -> bool {
        self.items.push(item);
        true
    }

    fn failed(&mut self, path: &Path, _error: io::Error) {
        self.failed.push(path.to_path_buf());
    }
}
```

- [ ] **Step 3: `trash.rs`'i testleriyle yaz**

```rust
//! Moving to the trash (Recycle Bin).

use std::io;
use std::path::PathBuf;

use gezik_core::ops::conflict::Facts;
use gezik_platform::fs;

use super::what;
use crate::task::{
    Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since, no_trash, unchanged,
};
use crate::walk::facts_of;

pub struct TrashTask {
    /// Each item, and how it must still look (undo); `None`: anything.
    items: Vec<(PathBuf, Option<Facts>)>,
    /// An undo: on a drive without a trash, an empty folder or file is simply deleted.
    undoing: bool,
}

impl TrashTask {
    pub fn new(paths: Vec<PathBuf>) -> TrashTask {
        TrashTask { items: paths.into_iter().map(|path| (path, None)).collect(), undoing: false }
    }

    /// Undo of a copy or of something new: items changed since are left alone.
    pub(crate) fn checked(items: Vec<(PathBuf, Option<Facts>)>) -> TrashTask {
        TrashTask { items, undoing: true }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.items.iter().map(|(path, _)| path.clone()).collect()
    }
}

fn is_empty_dir(path: &std::path::Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

impl Task for TrashTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Trash
    }

    fn title(&self) -> String {
        let bin = if cfg!(windows) { "the Recycle Bin" } else { "the Trash" };
        format!("Moving {} to {bin}", what(&self.paths()))
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, (path, _)) in self.items.iter().enumerate() {
            match std::fs::symlink_metadata(path) {
                Ok(meta) => {
                    if !sink.item(PlanItem::new(Stage::Parallel, facts_of(&meta)).source(path).top(root)) {
                        return;
                    }
                }
                Err(err) => sink.failed(path, err),
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        let expected = self.items.get(item.root).and_then(|(_, facts)| *facts);
        if !unchanged(path, expected) {
            return Err(changed_since());
        }
        if !cx.has_trash(path) {
            let empty = if item.facts.is_dir { is_empty_dir(path) } else { item.facts.size == 0 };
            if self.undoing && empty {
                fs::delete(path)?;
                return Ok(Outcome::Deleted { path: path.clone() });
            }
            return Err(no_trash());
        }
        Ok(match fs::trash(path)? {
            Some(trashed) => Outcome::Trashed { original: path.clone(), trashed },
            None => Outcome::Deleted { path: path.clone() },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Control;
    use crate::task::is_marker;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    #[test]
    fn trash_takes_files_and_folders() {
        let dir = test_dir("trash");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("f/b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(TrashTask::new(vec![dir.join("a.txt"), dir.join("f")])));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!dir.join("a.txt").exists() && !dir.join("f").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_trash_only_an_empty_undo_item_is_deleted() {
        let dir = test_dir("trash-none");
        write(&dir.join("full.txt"), "x");
        std::fs::create_dir(dir.join("empty")).unwrap();
        let control = Control::default();
        let no_bin = |_: &std::path::Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: std::cell::Cell::new(0) };
        let item = |path: PathBuf| {
            let facts = facts_of(&std::fs::symlink_metadata(&path).unwrap());
            PlanItem::new(Stage::Parallel, facts).source(path).top(0)
        };
        let user = TrashTask::new(vec![dir.join("empty")]);
        assert!(is_marker::<crate::NoTrash>(&user.run(&item(dir.join("empty")), &cx).unwrap_err()));
        let undo = TrashTask::checked(vec![(dir.join("empty"), None)]);
        assert!(matches!(undo.run(&item(dir.join("empty")), &cx).unwrap(), Outcome::Deleted { .. }));
        let undo = TrashTask::checked(vec![(dir.join("full.txt"), None)]);
        assert!(is_marker::<crate::NoTrash>(&undo.run(&item(dir.join("full.txt")), &cx).unwrap_err()));
        assert!(dir.join("full.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 4: `delete.rs`'i testleriyle yaz**

```rust
//! Deleting for good. With a pending list, each chosen item is first renamed to a hidden name
//! (one quick rename), so it leaves the folder at once while its contents are deleted.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gezik_platform::fs;

use super::what;
use crate::engine::lock;
use crate::pending::{PendingDeletes, hidden_name};
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};
use crate::walk::{Step, facts_of, walk};

pub struct DeleteTask {
    roots: Vec<PathBuf>,
    pending: Option<Arc<PendingDeletes>>,
    /// The roots are hidden folders of an earlier delete (from `pending-deletes`).
    recovering: bool,
    /// (hidden, original) for each root this delete hid.
    hidden: Mutex<Vec<(PathBuf, PathBuf)>>,
}

impl DeleteTask {
    pub fn new(paths: Vec<PathBuf>, pending: Option<Arc<PendingDeletes>>) -> DeleteTask {
        DeleteTask { roots: paths, pending, recovering: false, hidden: Mutex::default() }
    }

    /// Finishes deleting folders an earlier delete hid.
    pub(crate) fn recover(paths: Vec<PathBuf>, pending: Arc<PendingDeletes>) -> DeleteTask {
        DeleteTask { roots: paths, pending: Some(pending), recovering: true, hidden: Mutex::default() }
    }

    /// Renames `root` to a hidden name next to it and notes it; the original if that fails.
    fn hide(&self, root: &Path) -> PathBuf {
        let (Some(pending), Some(parent)) = (&self.pending, root.parent()) else { return root.to_path_buf() };
        if self.recovering {
            return root.to_path_buf();
        }
        let hidden = parent.join(hidden_name());
        if fs::move_entry(root, &hidden).is_err() {
            return root.to_path_buf();
        }
        if pending.add(&hidden).is_err() {
            // Not noted: a crash would leave it hidden forever. Put it back, delete in place.
            let _ = fs::move_entry(&hidden, root);
            return root.to_path_buf();
        }
        let _ = fs::set_hidden(&hidden);
        lock(&self.hidden).push((hidden.clone(), root.to_path_buf()));
        hidden
    }
}

const FILE: u8 = 0;
const DIR: u8 = 1;

impl Task for DeleteTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Delete
    }

    fn title(&self) -> String {
        if self.recovering { "Finishing an earlier delete".to_owned() } else { format!("Deleting {}", what(&self.roots)) }
    }

    fn count(&self) -> usize {
        self.roots.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.roots.clone(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, original) in self.roots.iter().enumerate() {
            let path = self.hide(original);
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(original, err);
                    continue;
                }
            };
            let facts = facts_of(&meta);
            if !facts.is_dir {
                if !sink.item(PlanItem::new(Stage::Parallel, facts).source(&path).top(root).tag(FILE)) {
                    return;
                }
                continue;
            }
            // Folders go last, the deepest first (After items run in reverse).
            if !sink.item(PlanItem::new(Stage::After, facts).source(&path).top(root).tag(DIR)) {
                return;
            }
            let walked = walk(&path, &mut |step| match step {
                Step::Entry { path, facts, .. } => {
                    let (stage, tag) = if facts.is_dir { (Stage::After, DIR) } else { (Stage::Parallel, FILE) };
                    sink.item(PlanItem::new(stage, facts).source(path).under(root).tag(tag))
                }
                Step::Failed { path, error } => {
                    sink.failed(path, error);
                    true
                }
            });
            if !walked {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        fs::delete(path)?;
        Ok(Outcome::Deleted { path: path.clone() })
    }

    fn done(&self, _cancelled: bool) {
        let Some(pending) = &self.pending else { return };
        if self.recovering {
            for root in &self.roots {
                if std::fs::symlink_metadata(root).is_err() {
                    pending.remove(root);
                }
            }
            return;
        }
        for (hidden, original) in lock(&self.hidden).drain(..) {
            // Cancelled, or something inside could not be deleted: what is left goes back
            // under its own name, so nothing stays hidden and nothing is deleted later unasked.
            if std::fs::symlink_metadata(&hidden).is_ok() {
                let _ = fs::move_entry(&hidden, &original);
            }
            pending.remove(&hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, Settings};
    use crate::testing::{CollectSink, defaults, finish, test_dir, write};

    fn engine_with_pending(dir: &Path) -> Engine {
        Engine::new(Settings { pending_deletes: Some(dir.join("pending-deletes")), ..Settings::default() }, || {})
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn delete_hides_at_once_and_removes_everything() {
        let dir = test_dir("delete");
        let work = dir.join("work");
        write(&work.join("victim/a/b.txt"), "b");
        write(&work.join("victim/c.txt"), "c");
        write(&work.join("file.txt"), "f");
        write(&work.join("keep.txt"), "k");
        let engine = engine_with_pending(&dir);
        let task = DeleteTask::new(vec![work.join("victim"), work.join("file.txt")], engine.pending_deletes());
        let job = engine.submit(Box::new(task));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&work), ["keep.txt"], "nothing hidden is left behind");
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_pending_list_it_deletes_in_place() {
        let dir = test_dir("delete-in-place");
        write(&dir.join("victim/a.txt"), "a");
        let engine = crate::testing::engine();
        let job = engine.submit(Box::new(DeleteTask::new(vec![dir.join("victim")], None)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(names(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_delete_puts_the_folder_back() {
        let dir = test_dir("delete-cancel");
        write(&dir.join("victim/a.txt"), "a");
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let task = DeleteTask::new(vec![dir.join("victim")], Some(pending.clone()));
        let mut sink = CollectSink::default();
        task.plan(&mut sink);
        assert!(!dir.join("victim").exists(), "hidden at once");
        assert_eq!(pending.load().len(), 1);
        task.done(true);
        assert_eq!(std::fs::read_to_string(dir.join("victim/a.txt")).unwrap(), "a");
        assert!(pending.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_finishes_an_interrupted_delete() {
        let dir = test_dir("delete-recover");
        let hidden = dir.join(format!("{}left", crate::pending::HIDDEN_PREFIX));
        write(&hidden.join("x/y.txt"), "y");
        std::fs::write(dir.join("pending-deletes"), format!("{}\n", hidden.display())).unwrap();
        let engine = engine_with_pending(&dir);
        let job = engine.recover_deletes().expect("one folder to finish");
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!hidden.exists());
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_ignores_foreign_paths() {
        let dir = test_dir("delete-recover-foreign");
        write(&dir.join("Documents/important.txt"), "keep");
        let text = format!("{}\n{}\n", dir.join("Documents").display(), dir.join("gone").display());
        std::fs::write(dir.join("pending-deletes"), text).unwrap();
        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None);
        assert_eq!(std::fs::read_to_string(dir.join("Documents/important.txt")).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 5: `restore.rs` ve `new.rs`'i testleriyle yaz**

`crates/gezik-ops/src/tasks/restore.rs`:

```rust
//! Bringing items back from the trash.

use std::io;
use std::path::PathBuf;

use gezik_platform::fs;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::facts_of;

pub struct RestoreTask {
    /// (where it is in the trash, where it was).
    pairs: Vec<(PathBuf, PathBuf)>,
}

impl RestoreTask {
    pub fn new(pairs: Vec<(PathBuf, PathBuf)>) -> RestoreTask {
        RestoreTask { pairs }
    }
}

impl Task for RestoreTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Restore
    }

    fn title(&self) -> String {
        let originals: Vec<PathBuf> = self.pairs.iter().map(|(_, original)| original.clone()).collect();
        format!("Restoring {}", what(&originals))
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths: Vec<PathBuf> = self.pairs.iter().map(|(trashed, _)| trashed.clone()).collect();
        paths.extend(self.pairs.iter().filter_map(|(_, original)| original.parent().map(|p| p.to_path_buf())));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, (trashed, original)) in self.pairs.iter().enumerate() {
            match std::fs::symlink_metadata(trashed) {
                Ok(meta) => {
                    let item = PlanItem::new(Stage::Parallel, facts_of(&meta)).source(trashed).target(original).checked().top(root);
                    if !sink.item(item) {
                        return;
                    }
                }
                Err(_) => sink.failed(original, io::Error::new(io::ErrorKind::NotFound, "no longer in the trash")),
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(trashed), Some(original)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        fs::restore(trashed, original)?;
        Ok(Outcome::Restored { original: original.clone(), facts: facts_after(original, item.facts.is_dir) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decision;
    use crate::testing::{engine, finish, read, test_dir, write};

    #[test]
    fn restoring_onto_a_taken_name_asks() {
        let dir = test_dir("restore");
        let original = dir.join("x.txt");
        write(&original, "old");
        let trashed = fs::trash(&original).unwrap().unwrap();
        write(&original, "newer");
        let engine = engine();
        let job = engine.submit(Box::new(RestoreTask::new(vec![(trashed, original.clone())])));
        let (report, _) = finish(&engine, job, |c| vec![Decision::KeepBoth; c.len()]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&original), "newer");
        assert_eq!(read(&dir.join("x (2).txt")), "old");
        assert_eq!(report.results, [dir.join("x (2).txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

`crates/gezik-ops/src/tasks/new.rs`:

```rust
//! A new, empty folder or file.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

pub struct NewTask {
    dir: PathBuf,
    name: &'static str,
    folder: bool,
}

impl NewTask {
    pub fn folder(dir: &Path) -> NewTask {
        NewTask { dir: dir.to_path_buf(), name: "New folder", folder: true }
    }

    pub fn file(dir: &Path) -> NewTask {
        NewTask { dir: dir.to_path_buf(), name: "New file.txt", folder: false }
    }
}

impl Task for NewTask {
    fn kind(&self) -> TaskKind {
        if self.folder { TaskKind::NewFolder } else { TaskKind::NewFile }
    }

    fn title(&self) -> String {
        format!("Creating {}", self.name)
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.dir.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let facts = Facts { is_dir: self.folder, ..Facts::default() };
        // A taken name becomes "New folder (2)" without asking.
        let item = PlanItem::new(Stage::Parallel, facts).target(self.dir.join(self.name)).checked().top(0).preset(Some(Decision::KeepBoth));
        sink.item(item);
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        if self.folder {
            std::fs::create_dir(target)?;
        } else {
            std::fs::OpenOptions::new().write(true).create_new(true).open(target)?;
        }
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, self.folder), from: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{defaults, engine, finish, test_dir};

    #[test]
    fn a_new_folder_takes_the_next_free_name() {
        let dir = test_dir("new-folder");
        std::fs::create_dir(dir.join("New folder")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(NewTask::folder(&dir)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(dir.join("New folder (2)").is_dir());
        assert_eq!(report.results, [dir.join("New folder (2)")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_file_is_empty() {
        let dir = test_dir("new-file");
        let engine = engine();
        let job = engine.submit(Box::new(NewTask::file(&dir)));
        finish(&engine, job, defaults);
        assert_eq!(std::fs::metadata(dir.join("New file.txt")).unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

`crates/gezik-ops/src/tasks/mod.rs` modül listesini ve dışa açılanları genişlet:

```rust
mod copy;
mod delete;
mod move_;
mod new;
mod restore;
mod trash;

pub use copy::CopyTask;
pub use delete::DeleteTask;
pub use move_::MoveTask;
pub use new::NewTask;
pub use restore::RestoreTask;
pub use trash::TrashTask;
```

- [ ] **Step 6: Motoru bekleyen silmelere bağla**

`crates/gezik-ops/src/engine.rs`:

`use` satırlarına ekle:

```rust
use crate::pending::{PendingDeletes, is_hidden};
use crate::tasks::DeleteTask;
```

`Shared` yapısına `next_id: AtomicU64,`'ten sonra ekle:

```rust
    pending: Option<Arc<PendingDeletes>>,
```

`Engine::new` içinde `Shared { … }` kurulmadan önce ve alan olarak:

```rust
        let pending = settings.pending_deletes.clone().map(|file| Arc::new(PendingDeletes::new(file)));
```

```rust
            pending,
```

`impl Engine` içine ekle:

```rust
    /// The list instant deletes note their hidden folders in (`None`: deletes run in place).
    pub fn pending_deletes(&self) -> Option<Arc<PendingDeletes>> {
        self.0.pending.clone()
    }

    /// Finishes deletes an earlier run left unfinished (call once at start).
    pub fn recover_deletes(&self) -> Option<JobId> {
        let pending = self.0.pending.clone()?;
        let mut roots = Vec::new();
        for path in pending.load() {
            if is_hidden(&path) && std::fs::symlink_metadata(&path).is_ok() {
                roots.push(path);
            } else {
                pending.remove(&path);
            }
        }
        (!roots.is_empty()).then(|| self.submit(Box::new(DeleteTask::recover(roots, pending))))
    }
```

`PendingDeletes::load` yalnızca gizli adları döndürdüğü için yabancı satırlar listede kalır ama hiçbir zaman silinmez; `pending.remove` yalnızca var olmayan gizli adları temizler.

`crates/gezik-ops/src/lib.rs`: modül listesine `mod pending;` → `pub mod pending;` olarak ekle ve dışa açılanları güncelle:

```rust
pub use pending::PendingDeletes;
pub use tasks::{CopyTask, DeleteTask, MoveTask, NewTask, RestoreTask, TrashTask};
```

- [ ] **Step 7: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-ops`
Expected: PASS (Windows'ta çöp testleri gerçek Çöp Kutusu'nu kullanır; `restoring_onto_a_taken_name_asks` kendi öğesini geri getirir).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 8: Commit**

```bash
git add crates/gezik-ops
git commit -m "Trash, instant permanent delete with recovery, restore, new folder and file tasks"
```

---

### Task 11: Geri al / yinele: sonuçlardan ters görevler, geçmiş

**Files:**
- Create: `crates/gezik-ops/src/inverse.rs`
- Modify: `crates/gezik-ops/src/engine.rs` (`Origin`, `Record`, geçmiş, `undo`/`redo`, etiketler)
- Modify: `crates/gezik-ops/src/lib.rs`

**Interfaces:**
- Consumes: Task 8–10 (`Outcome`, `TrashTask::checked`, `MoveTask::back`, `RestoreTask::new`), `gezik_core::ops::{history::UndoStack, paths::cover}`
- Produces:
  - `inverse::build(outcomes: &[Outcome]) -> Vec<Box<dyn Task>>` (crate içi)
  - `Engine::undo(&self) -> Option<JobId>`, `Engine::redo(&self) -> Option<JobId>`, `Engine::undo_label(&self) -> Option<String>`, `Engine::redo_label(&self) -> Option<String>`
  - Geçmiş değişince `Event::History`

- [ ] **Step 1: `inverse.rs`'i yaz**

```rust
//! Undo, built from what a job did: what it made goes to the trash, what it moved goes back,
//! what it trashed comes back. Nothing here knows the task kinds.

use std::path::PathBuf;

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::cover;

use crate::task::{Outcome, Task};
use crate::tasks::{MoveTask, RestoreTask, TrashTask};

/// Files are checked before undo touches them; folders are not (they change as files land).
fn expect(facts: &Facts) -> Option<Facts> {
    (!facts.is_dir).then_some(*facts)
}

/// The tasks that undo `outcomes`, in order: trash what was made, move back what was moved,
/// restore what was trashed (an item replaced by a copy comes back after the copy left).
pub(crate) fn build(outcomes: &[Outcome]) -> Vec<Box<dyn Task>> {
    let mut made: Vec<(PathBuf, Option<Facts>)> = Vec::new();
    let mut moved: Vec<(PathBuf, PathBuf, Option<Facts>)> = Vec::new();
    let mut trashed: Vec<(PathBuf, PathBuf)> = Vec::new();
    for outcome in outcomes {
        match outcome {
            Outcome::Created { path, facts, from: None } => made.push((path.clone(), expect(facts))),
            Outcome::Created { path, facts, from: Some(from) } => moved.push((path.clone(), from.clone(), expect(facts))),
            Outcome::Moved { from, to, facts } => moved.push((to.clone(), from.clone(), expect(facts))),
            Outcome::Trashed { original, trashed: at } => trashed.push((at.clone(), original.clone())),
            Outcome::Restored { original, facts } => made.push((original.clone(), expect(facts))),
            Outcome::Deleted { .. } | Outcome::Nothing => {}
        }
    }
    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
    let made = cover(made, |(path, _)| path);
    if !made.is_empty() {
        tasks.push(Box::new(TrashTask::checked(made)));
    }
    let moved = cover(moved, |(path, _, _)| path);
    if !moved.is_empty() {
        tasks.push(Box::new(MoveTask::back(moved)));
    }
    if !trashed.is_empty() {
        tasks.push(Box::new(RestoreTask::new(trashed)));
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskKind;

    fn file() -> Facts {
        Facts { is_dir: false, size: 1, modified: None }
    }

    #[test]
    fn made_moved_and_trashed_each_get_their_inverse_in_order() {
        let outcomes = vec![
            Outcome::Created { path: "/d/new".into(), facts: Facts { is_dir: true, ..Facts::default() }, from: None },
            Outcome::Created { path: "/d/new/inside.txt".into(), facts: file(), from: None },
            Outcome::Moved { from: "/a/x".into(), to: "/b/x".into(), facts: file() },
            Outcome::Trashed { original: "/d/old.txt".into(), trashed: "/bin/1".into() },
            Outcome::Deleted { path: "/gone".into() },
        ];
        let tasks = build(&outcomes);
        let kinds: Vec<TaskKind> = tasks.iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Move, TaskKind::Restore]);
        assert_eq!(tasks[0].count(), 1, "the new folder covers what is inside it");
    }

    #[test]
    fn deletes_alone_cannot_be_undone() {
        assert!(build(&[Outcome::Deleted { path: "/x".into() }, Outcome::Nothing]).is_empty());
    }
}
```

- [ ] **Step 2: Başarısız motor testlerini yaz**

`crates/gezik-ops/src/engine.rs` test modülüne ekle:

```rust
    use crate::testing::{read, write};
    use crate::{CopyTask, DeleteTask, MoveTask, TrashTask};

    fn run(engine: &Engine, job: JobId) -> Report {
        finish(engine, job, defaults).0
    }

    #[test]
    fn undo_and_redo_a_copy() {
        let dir = test_dir("undo-copy");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        run(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst")))));
        assert_eq!(engine.undo_label().as_deref(), Some("Copy 1 item"));
        let report = run(&engine, engine.undo().unwrap());
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!dir.join("dst/a.txt").exists());
        assert_eq!(engine.undo_label(), None);
        assert_eq!(engine.redo_label().as_deref(), Some("Copy 1 item"));
        run(&engine, engine.redo().unwrap());
        assert_eq!(read(&dir.join("dst/a.txt")), "a", "back from the trash");
        assert_eq!(engine.undo_label().as_deref(), Some("Copy 1 item"));
        run(&engine, engine.undo().unwrap());
        assert!(!dir.join("dst/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_rename_brings_the_old_name_back() {
        let dir = test_dir("undo-rename");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        run(&engine, engine.submit(Box::new(MoveTask::rename(dir.join("a.txt"), "b.txt"))));
        assert_eq!(engine.undo_label().as_deref(), Some("Rename"));
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert!(!dir.join("b.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_skips_changed_files() {
        let dir = test_dir("undo-changed");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        run(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst")))));
        std::fs::write(dir.join("dst/a.txt"), "edited after the copy").unwrap();
        let report = run(&engine, engine.undo().unwrap());
        assert_eq!(report.skipped_changed, 1);
        assert_eq!(read(&dir.join("dst/a.txt")), "edited after the copy");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_replace_brings_the_old_file_back() {
        let dir = test_dir("undo-replace");
        write(&dir.join("src/a.txt"), "new");
        write(&dir.join("dst/a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst"))));
        finish(&engine, job, |c| vec![Decision::Replace; c.len()]);
        assert_eq!(read(&dir.join("dst/a.txt")), "new");
        let report = run(&engine, engine.undo().unwrap());
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/a.txt")), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_trash_restores() {
        let dir = test_dir("undo-trash");
        write(&dir.join("x/y.txt"), "y");
        let engine = engine();
        run(&engine, engine.submit(Box::new(TrashTask::new(vec![dir.join("x")]))));
        assert!(!dir.join("x").exists());
        assert_eq!(engine.undo_label().as_deref(), Some("Delete 1 item"));
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("x/y.txt")), "y");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_a_merging_move_moves_the_files_back() {
        let dir = test_dir("undo-merge");
        write(&dir.join("src/d/a.txt"), "a");
        write(&dir.join("dst/d/b.txt"), "b");
        let engine = engine();
        run(&engine, engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst")))));
        assert!(!dir.join("src/d").exists());
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("src/d/a.txt")), "a");
        assert_eq!(read(&dir.join("dst/d/b.txt")), "b");
        assert!(!dir.join("dst/d/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_action_ends_redo_and_deletes_are_not_undoable() {
        let dir = test_dir("undo-new-action");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        run(&engine, engine.submit(Box::new(MoveTask::rename(dir.join("a.txt"), "c.txt"))));
        run(&engine, engine.undo().unwrap());
        assert!(engine.redo_label().is_some());
        let (_, events) = finish(&engine, engine.submit(Box::new(MoveTask::rename(dir.join("b.txt"), "d.txt"))), defaults);
        assert!(events.iter().any(|e| *e == Event::History));
        assert_eq!(engine.redo_label(), None);
        run(&engine, engine.submit(Box::new(DeleteTask::new(vec![dir.join("d.txt")], None))));
        assert_eq!(engine.undo_label().as_deref(), Some("Rename"), "the delete is not on the undo stack");
        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 3: Başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-ops undo`
Expected: FAIL (`undo`, `undo_label` … yok).

- [ ] **Step 4: Geçmişi motora ekle**

`crates/gezik-ops/src/lib.rs` modül listesine `mod inverse;` ekle.

`crates/gezik-ops/src/engine.rs`:

`use` satırlarına ekle:

```rust
use gezik_core::ops::history::UndoStack;
```

Tiplerin yanına ekle:

```rust
/// Where a job came from: its result goes on the undo or the redo stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    New,
    Undo,
    Redo,
}

/// One undoable action: its label and the tasks that undo it.
pub(crate) struct Record {
    label: String,
    inverse: Vec<Box<dyn Task>>,
}

/// How many actions can be undone.
const HISTORY: usize = 100;
```

`Job` yapısına `pub background: bool,`'dan sonra ekle:

```rust
    pub origin: Origin,
    /// "Copy 3 items": what Undo will say.
    pub label: String,
```

`Shared` yapısına `pending`'den sonra ekle:

```rust
    history: Mutex<UndoStack<Record>>,
```

`Engine::new` içinde `Shared { … }`'a:

```rust
            history: Mutex::new(UndoStack::new(HISTORY)),
```

`Shared::finish`'in başını şu hale getir (`acc` alındıktan hemen sonra geri alma kaydı):

```rust
    pub fn finish(&self, job: &Arc<Job>, cancelled: bool) {
        for task in &job.tasks {
            task.done(cancelled);
        }
        let acc = std::mem::take(&mut *lock(&job.acc));
        let inverse = crate::inverse::build(&acc.outcomes);
        let recorded = !inverse.is_empty();
        if recorded {
            let record = Record { label: job.label.clone(), inverse };
            let mut history = lock(&self.history);
            match job.origin {
                Origin::New => history.push_new(record),
                Origin::Undo => history.push_undone(record),
                Origin::Redo => history.push_redone(record),
            }
        }
```

ve `events` kurulurken `Finished`'tan önce ekle:

```rust
        if recorded || job.origin != Origin::New {
            events.push(Event::History);
        }
```

`Engine::submit` ve `Engine::start`'ı şu hale getir:

```rust
    pub fn submit(&self, task: Box<dyn Task>) -> JobId {
        self.start(vec![task], Origin::New, None)
    }

    /// Starts a job of `tasks` (run in order) on its own thread. `label`: what Undo says
    /// (default: from the first task); an undo or redo keeps the action's label.
    pub(crate) fn start(&self, tasks: Vec<Box<dyn Task>>, origin: Origin, label: Option<String>) -> JobId {
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(first) = tasks.first() else { return id };
        let label = label.unwrap_or_else(|| first.kind().label(first.count()));
        let title = match origin {
            Origin::New => first.title(),
            Origin::Undo => format!("Undoing {label}"),
            Origin::Redo => format!("Redoing {label}"),
        };
        let kind = first.kind();
        let background = tasks.iter().all(|task| task.kind() == TaskKind::Delete);
        let job = Arc::new(Job {
            id,
            tasks,
            control: Control::default(),
            drives: Mutex::default(),
            background,
            origin,
            label,
            done: AtomicBool::new(false),
            acc: Mutex::default(),
        });
```

(gövdenin geri kalanı Task 8'deki gibi.)

`impl Engine` içine ekle:

```rust
    /// Undoes the last action; its progress shows like any job.
    pub fn undo(&self) -> Option<JobId> {
        let record = lock(&self.0.history).pop_undo()?;
        self.0.push([Event::History]);
        Some(self.start(record.inverse, Origin::Undo, Some(record.label)))
    }

    pub fn redo(&self) -> Option<JobId> {
        let record = lock(&self.0.history).pop_redo()?;
        self.0.push([Event::History]);
        Some(self.start(record.inverse, Origin::Redo, Some(record.label)))
    }

    /// "Copy 3 items" if there is something to undo.
    pub fn undo_label(&self) -> Option<String> {
        lock(&self.0.history).peek_undo().map(|record| record.label.clone())
    }

    pub fn redo_label(&self) -> Option<String> {
        lock(&self.0.history).peek_redo().map(|record| record.label.clone())
    }
```

- [ ] **Step 5: Testleri çalıştır**

Run: `~/.cargo/bin/cargo test -p gezik-ops`
Expected: tüm testler PASS.
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

- [ ] **Step 6: Commit**

```bash
git add crates/gezik-ops
git commit -m "Undo and redo built from what each job did: trash the made, move back the moved, restore the trashed"
```

---

### Task 12: Uygulama — motor bağlantısı, ilerleme paneli, özet, iletişim kutuları, kapanış sorusu

**Files:**
- Modify: `crates/gezik/Cargo.toml` (`gezik-ops`)
- Create: `crates/gezik/src/operations.rs`
- Create: `crates/gezik/src/dialog.rs`
- Create: `crates/gezik/ui/widgets/text-button.slint`
- Create: `crates/gezik/ui/widgets/dialog.slint`
- Create: `crates/gezik/ui/widgets/ops-panel.slint`
- Modify: `crates/gezik/ui/app.slint`
- Modify: `crates/gezik/src/navigation.rs` (`refresh_showing`, sessiz yükleme)
- Modify: `crates/gezik/src/view/mod.rs` (`hide_names`, `on_shown`, `folder`, `note`)
- Modify: `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: `gezik_ops::{Engine, Settings, Event, Progress, JobState, PauseReason, Report, Failure, JobId, Task, TaskKind}` (Task 8–11), `gezik_platform::taskbar` (Task 7), `gezik_config::settings::FilesSettings` (Task 3), `gezik_core::ops::rate` (Task 2)
- Produces:
  - `operations::Operations` (`Clone`): `new(window, nav, view, sidebar, dialogs, settings: gezik_ops::Settings, files: FilesSettings, collapsed: bool)`, `submit(task: Box<dyn Task>, retry: Option<Rc<dyn Fn() -> Box<dyn Task>>>, after: After) -> JobId`, `drain()`, `set_files(FilesSettings)`, `collapsed() -> bool`, `toggle_collapsed()`, `pause/resume/cancel/start_now/details/retry/dismiss(id: i32)`, `confirm_close(quit: impl FnOnce() + 'static) -> bool`, `recover()`
  - `operations::After { Select, Nothing }` (Task 15 `Rename` ekler), `operations::with_current(f)`
  - saf: `operations::{RowState, describe, summary_text, taskbar_progress, result_names}`
  - `dialog::Dialogs` (`Clone`): `new(&AppWindow)`, `ask(title, message, buttons: &[&str], answer: impl FnOnce(Option<usize>) + 'static)`
  - `Navigator::refresh_showing(&self, dirs: &[PathBuf], select: &[String])`
  - `View::folder(&self) -> Option<PathBuf>`, `View::note(&self, text: String)`
  - (İkili crate'te kullanılmayan `pub` öğe uyarı verir; her yardımcı ilk kullanıldığı görevde eklenir.)
  - Slint: `OpRow`, `OpsPanel`, `Dialog`, `TextButton`; `AppWindow` özellikleri `op-rows`, `ops-panel-open`, `ops-summary`, `ops-collapsed`, `dialog-open`, `dialog-title`, `dialog-message`, `dialog-buttons`; geri çağrılar `op-pause/op-resume/op-cancel/op-start-now/op-details/op-retry/op-dismiss(int)`, `ops-toggle()`, `dialog-chosen(int)`

- [ ] **Step 1: Bağımlılığı ekle**

`crates/gezik/Cargo.toml` `[dependencies]`'e `gezik-platform.workspace = true` satırından sonra:

```toml
gezik-ops.workspace = true
```

- [ ] **Step 2: Saf yardımcıları testleriyle yaz (`operations.rs`'in ilk hali)**

`crates/gezik/src/operations.rs`:

```rust
//! File operations in the app: the engine, the operations panel and its summary, the taskbar,
//! and what happens when a job ends (refresh, select, report problems).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gezik_config::settings::FilesSettings;
use gezik_core::format_size;
use gezik_core::ops::paths::same_path;
use gezik_core::ops::rate::{Rate, format_eta, format_rate};
use gezik_ops::{Engine, Event, JobId, JobState, PauseReason, Progress, Report, Settings, Task};
use gezik_platform::taskbar::{Taskbar, TaskbarState};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::dialog::Dialogs;
use crate::navigation::{Navigator, sync_model};
use crate::sidebar::Sidebar;
use crate::view::View;
use crate::{AppWindow, OpRow};

/// A job shows in the panel only if it still runs after this long.
const SHOW_AFTER: Duration = Duration::from_secs(1);
/// A finished row without problems stays this long.
const DONE_FOR: Duration = Duration::from_secs(3);
/// "Details" lists at most this many failures.
const MAX_DETAILS: usize = 50;

/// How a row looks; the numbers are `OpRow.state` in ops-panel.slint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Running = 0,
    Waiting = 1,
    Paused = 2,
    Deciding = 3,
    Done = 4,
    Failed = 5,
}

/// How far a job is, 0–1: by bytes when it has any, else by items.
fn fraction(progress: &Progress) -> f32 {
    let ratio = |done: u64, total: u64| (done as f64 / total as f64).clamp(0.0, 1.0) as f32;
    if progress.bytes_total > 0 {
        ratio(progress.bytes_done, progress.bytes_total)
    } else if progress.items_total > 0 {
        ratio(progress.items_done, progress.items_total)
    } else {
        0.0
    }
}

/// A row's state, its text after the title, and its bar (below 0: not known yet).
pub fn describe(
    progress: Option<&Progress>,
    report: Option<&Report>,
    speed: Option<f64>,
    left: Option<Duration>,
) -> (RowState, String, f32) {
    if let Some(report) = report {
        return match report.failures.len() {
            _ if report.cancelled => (RowState::Done, "Cancelled".to_owned(), 1.0),
            0 => (RowState::Done, "Done".to_owned(), 1.0),
            1 => (RowState::Failed, "1 item failed".to_owned(), 1.0),
            n => (RowState::Failed, format!("{n} items failed"), 1.0),
        };
    }
    let Some(progress) = progress else { return (RowState::Waiting, "Starting".to_owned(), -1.0) };
    let done = fraction(progress);
    match progress.state {
        JobState::Waiting => (RowState::Waiting, "Waiting for the drive".to_owned(), -1.0),
        JobState::Scanning => {
            let size = format_size(progress.bytes_total);
            (RowState::Running, format!("Scanning… {} items · {size}", progress.items_total), -1.0)
        }
        JobState::Deciding => (RowState::Deciding, "Waiting for your decisions".to_owned(), done),
        JobState::Paused(PauseReason::User) => (RowState::Paused, "Paused".to_owned(), done),
        JobState::Paused(PauseReason::DiskFull) => (RowState::Paused, "The disk is full".to_owned(), done),
        JobState::Paused(PauseReason::ManyFailures) => (RowState::Paused, "Paused after errors".to_owned(), done),
        JobState::Running => {
            let mut text = format!("{}%", (done * 100.0).floor() as u32);
            if let Some(speed) = speed.filter(|s| *s > 0.0 && progress.bytes_total > 0) {
                text.push_str(&format!(" · {}", format_rate(speed)));
            }
            if let Some(left) = left {
                text.push_str(&format!(" · {}", format_eta(left)));
            }
            (RowState::Running, text, done)
        }
    }
}

/// The status bar's line for the panel: `2 operations · 61%`; `3 items failed` once only
/// failed rows are left; empty when the panel has nothing.
pub fn summary_text(running: usize, done: Option<f32>, failed_items: usize, rows: usize) -> String {
    if rows == 0 {
        return String::new();
    }
    if running == 0 {
        return match failed_items {
            0 => "Operations done".to_owned(),
            1 => "1 item failed".to_owned(),
            n => format!("{n} items failed"),
        };
    }
    let what = if running == 1 { "1 operation".to_owned() } else { format!("{running} operations") };
    match done {
        Some(done) => format!("{what} · {}%", (done * 100.0).floor() as u32),
        None => what,
    }
}

/// The taskbar button for these rows (state, bar): paused if any waits for the user, red if
/// any failed; off when nothing runs or failed.
pub fn taskbar_progress(rows: &[(RowState, f32)]) -> (TaskbarState, u64, u64) {
    let active: Vec<&(RowState, f32)> = rows.iter().filter(|(state, _)| *state != RowState::Done).collect();
    if active.is_empty() {
        return (TaskbarState::Off, 0, 0);
    }
    let state = if active.iter().any(|(s, _)| *s == RowState::Failed) {
        TaskbarState::Error
    } else if active.iter().any(|(s, _)| matches!(s, RowState::Paused | RowState::Deciding)) {
        TaskbarState::Paused
    } else {
        TaskbarState::Normal
    };
    let total = active.len() as u64 * 1000;
    let done: u64 = active.iter().map(|(_, f)| (f.clamp(0.0, 1.0) * 1000.0) as u64).sum();
    (state, done, total)
}

/// The names in `folder` among `results` (to select them there).
pub fn result_names(results: &[PathBuf], folder: &Path) -> Vec<String> {
    results
        .iter()
        .filter(|path| path.parent().is_some_and(|parent| same_path(parent, folder)))
        .filter_map(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_ops::{Failure, TaskKind};

    fn progress(state: JobState, items: (u64, u64), bytes: (u64, u64)) -> Progress {
        Progress { state, items_done: items.0, items_total: items.1, bytes_done: bytes.0, bytes_total: bytes.1 }
    }

    fn report(failures: usize, cancelled: bool) -> Report {
        Report {
            kind: TaskKind::Copy,
            cancelled,
            failures: (0..failures).map(|i| Failure { path: format!("/f{i}").into(), message: "x".into() }).collect(),
            skipped_changed: 0,
            no_trash: Vec::new(),
            results: Vec::new(),
            changed_dirs: Vec::new(),
        }
    }

    #[test]
    fn running_rows_show_percent_speed_and_time_left() {
        let p = progress(JobState::Running, (3, 10), (61, 100));
        let (state, text, bar) = describe(Some(&p), None, Some(84.0 * 1024.0 * 1024.0), Some(Duration::from_secs(42)));
        assert_eq!(state, RowState::Running);
        assert!(text.starts_with("61% · ") && text.ends_with(" · ~0:42"), "{text}");
        assert!((bar - 0.61).abs() < 0.001);
        let items_only = progress(JobState::Running, (1, 4), (0, 0));
        assert_eq!(describe(Some(&items_only), None, None, None).1, "25%");
    }

    #[test]
    fn other_states() {
        let scanning = progress(JobState::Scanning, (0, 12400), (0, 3 * 1024 * 1024 * 1024));
        let (_, text, bar) = describe(Some(&scanning), None, None, None);
        assert!(text.starts_with("Scanning… 12400 items · "), "{text}");
        assert!(bar < 0.0);
        let waiting = progress(JobState::Waiting, (0, 0), (0, 0));
        assert_eq!(describe(Some(&waiting), None, None, None).0, RowState::Waiting);
        let full = progress(JobState::Paused(PauseReason::DiskFull), (1, 2), (0, 0));
        assert_eq!(describe(Some(&full), None, None, None).1, "The disk is full");
        assert_eq!(describe(None, Some(&report(0, false)), None, None).1, "Done");
        assert_eq!(describe(None, Some(&report(3, false)), None, None), (RowState::Failed, "3 items failed".into(), 1.0));
        assert_eq!(describe(None, Some(&report(3, true)), None, None).1, "Cancelled");
    }

    #[test]
    fn summary_line() {
        assert_eq!(summary_text(0, None, 0, 0), "");
        assert_eq!(summary_text(2, Some(0.615), 0, 2), "2 operations · 61%");
        assert_eq!(summary_text(1, None, 0, 1), "1 operation");
        assert_eq!(summary_text(0, None, 3, 1), "3 items failed");
        assert_eq!(summary_text(0, None, 0, 1), "Operations done");
    }

    #[test]
    fn taskbar_follows_the_rows() {
        assert_eq!(taskbar_progress(&[]), (TaskbarState::Off, 0, 0));
        assert_eq!(taskbar_progress(&[(RowState::Done, 1.0)]).0, TaskbarState::Off);
        assert_eq!(taskbar_progress(&[(RowState::Running, 0.5), (RowState::Running, 1.0)]), (TaskbarState::Normal, 1500, 2000));
        assert_eq!(taskbar_progress(&[(RowState::Running, 0.5), (RowState::Deciding, 0.1)]).0, TaskbarState::Paused);
        assert_eq!(taskbar_progress(&[(RowState::Paused, 0.5), (RowState::Failed, 1.0)]).0, TaskbarState::Error);
    }

    #[test]
    fn only_results_in_the_folder_are_selected() {
        let results = [PathBuf::from("/a/x.txt"), PathBuf::from("/b/y.txt"), PathBuf::from("/a/sub")];
        assert_eq!(result_names(&results, Path::new("/a")), ["x.txt", "sub"]);
    }
}
```

`crates/gezik/src/main.rs` modül listesine `mod operations;` ve `mod dialog;` ekle (alfabetik). `OpRow` ve `Dialogs` henüz yok; bu adımda derlemek için Step 3–5'i bitir.

- [ ] **Step 3: Slint bileşenlerini yaz**

`crates/gezik/ui/widgets/text-button.slint`:

```slint
import { Theme } from "../theme.slint";

// A button with a text label. `primary` fills it with the accent color; `checked` marks the
// chosen one of a pair.
export component TextButton inherits Rectangle {
    in property <string> text;
    in property <bool> primary;
    in property <bool> checked;
    callback clicked();

    height: Theme.row-height + 4px;
    min-width: max(72px, label.preferred-width + Theme.spacing * 4);
    border-radius: Theme.radius;
    border-width: root.primary ? 0px : (scope.has-focus || root.checked ? 2px : 1px);
    border-color: scope.has-focus || root.checked ? Theme.accent : Theme.border;
    background: root.primary ? Theme.accent : (touch.has-hover ? Theme.hover : transparent);
    forward-focus: scope;
    accessible-role: button;
    accessible-label: root.text;
    accessible-checked: root.checked;
    accessible-action-default => { root.clicked(); }

    label := Text {
        width: parent.width;
        height: parent.height;
        text: root.text;
        color: root.primary ? Theme.accent-foreground : Theme.foreground;
        horizontal-alignment: center;
        vertical-alignment: center;
    }

    scope := FocusScope {
        key-pressed(event) => {
            if (event.text == Key.Return || event.text == " ") {
                root.clicked();
                return accept;
            }
            return reject;
        }
    }

    touch := TouchArea {
        mouse-cursor: pointer;
        clicked => { root.clicked(); }
    }
}
```

`crates/gezik/ui/widgets/dialog.slint`:

```slint
import { Theme } from "../theme.slint";
import { TextButton } from "text-button.slint";

// A question over the window: a title, a message and buttons. Enter picks the first button,
// Esc the last.
export component Dialog inherits Rectangle {
    in property <string> title;
    in property <string> message;
    in property <[string]> buttons;
    callback chosen(int);

    background: Theme.background.with-alpha(0.6);

    // Clicks outside the box go nowhere.
    TouchArea { }

    scope := FocusScope {
        init => { self.focus(); }
        key-pressed(event) => {
            if event.text == Key.Return {
                root.chosen(0);
                return accept;
            }
            if event.text == Key.Escape {
                root.chosen(root.buttons.length - 1);
                return accept;
            }
            return reject;
        }
    }

    Rectangle {
        width: min(460px, root.width - 40px);
        height: content.preferred-height;
        x: (root.width - self.width) / 2;
        y: (root.height - self.height) / 3;
        background: Theme.surface;
        border-radius: Theme.radius * 1.5;
        border-width: 1px;
        border-color: Theme.border;
        accessible-role: groupbox;
        accessible-label: root.title;

        content := VerticalLayout {
            padding: Theme.spacing * 3;
            spacing: Theme.spacing * 2;
            Text {
                text: root.title;
                font-weight: 600;
                wrap: word-wrap;
                color: Theme.foreground;
            }
            if root.message != "": Flickable {
                height: min(300px, message-text.preferred-height);
                viewport-height: message-text.preferred-height;
                message-text := Text {
                    width: parent.width;
                    text: root.message;
                    wrap: word-wrap;
                    color: Theme.foreground-muted;
                }
            }
            HorizontalLayout {
                alignment: end;
                spacing: Theme.spacing;
                for label[i] in root.buttons: TextButton {
                    text: label;
                    primary: i == 0;
                    clicked => { root.chosen(i); }
                }
            }
        }
    }
}
```

`crates/gezik/ui/widgets/ops-panel.slint`:

```slint
import { Theme } from "../theme.slint";
import { ToolButton } from "button.slint";

// One running (or just finished) operation, as Rust (operations.rs) builds it.
export struct OpRow {
    id: int,
    title: string,
    detail: string,
    // 0–1; below 0 while the amount is not known (scanning).
    progress: float,
    // 0 running, 1 waiting, 2 paused, 3 deciding, 4 done, 5 failed (RowState in operations.rs).
    state: int,
    can-pause: bool,
    can-resume: bool,
    can-start-now: bool,
    can-retry: bool,
    can-details: bool,
    finished: bool,
}

// A link-like text button for the panel.
component LinkButton inherits Rectangle {
    in property <string> text;
    callback clicked();

    width: label.preferred-width + Theme.spacing * 2;
    border-radius: Theme.radius;
    background: touch.has-hover ? Theme.hover : transparent;
    accessible-role: button;
    accessible-label: root.text;
    accessible-action-default => { root.clicked(); }

    label := Text {
        width: parent.width;
        height: parent.height;
        text: root.text;
        color: Theme.accent;
        horizontal-alignment: center;
        vertical-alignment: center;
    }

    touch := TouchArea {
        mouse-cursor: pointer;
        clicked => { root.clicked(); }
    }
}

// Running operations above the status bar: one line each with a bar, the progress text and
// its buttons.
export component OpsPanel inherits Rectangle {
    in property <[OpRow]> rows;
    callback pause(int);
    callback resume(int);
    callback cancel(int);
    callback start-now(int);
    callback details(int);
    callback retry(int);
    callback dismiss(int);

    background: Theme.surface;
    accessible-role: list;
    accessible-label: "Operations";

    VerticalLayout {
        padding-left: Theme.spacing + 4px;
        padding-right: Theme.spacing;
        for row in root.rows: Rectangle {
            height: Theme.row-height + 6px;
            accessible-role: list-item;
            accessible-label: row.title;
            accessible-description: row.detail;

            HorizontalLayout {
                spacing: Theme.spacing * 2;
                Text {
                    text: row.title;
                    horizontal-stretch: 1;
                    min-width: 120px;
                    vertical-alignment: center;
                    overflow: elide;
                    color: Theme.foreground;
                }
                if !row.finished: VerticalLayout {
                    alignment: center;
                    Rectangle {
                        width: 160px;
                        height: 6px;
                        border-radius: 3px;
                        background: Theme.border;
                        accessible-role: progress-indicator;
                        accessible-value: row.progress >= 0 ? round(row.progress * 100) + "%" : "";
                        Rectangle {
                            x: 0;
                            width: row.progress >= 0 ? parent.width * clamp(row.progress, 0, 1) : parent.width * 0.3;
                            height: parent.height;
                            border-radius: 3px;
                            background: row.state == 2 || row.state == 3 ? Theme.progress-paused
                                : row.state == 5 ? Theme.progress-error : Theme.progress;
                        }
                    }
                }
                Text {
                    text: row.detail;
                    min-width: 160px;
                    vertical-alignment: center;
                    overflow: elide;
                    color: row.state == 5 ? Theme.danger : Theme.foreground-muted;
                }
                if row.can-start-now: LinkButton {
                    text: "Start now";
                    clicked => { root.start-now(row.id); }
                }
                if row.can-details: LinkButton {
                    text: "Details";
                    clicked => { root.details(row.id); }
                }
                if row.can-retry: LinkButton {
                    text: "Retry";
                    clicked => { root.retry(row.id); }
                }
                if row.can-pause: ToolButton {
                    text: "Pause";
                    icon: "M 5 3 L 5 13 M 11 3 L 11 13";
                    clicked => { root.pause(row.id); }
                }
                if row.can-resume: ToolButton {
                    text: "Resume";
                    icon: "M 5 3 L 13 8 L 5 13 Z";
                    clicked => { root.resume(row.id); }
                }
                ToolButton {
                    text: row.finished ? "Close" : "Cancel";
                    icon: "M 4 4 L 12 12 M 12 4 L 4 12";
                    clicked => {
                        if row.finished {
                            root.dismiss(row.id);
                        } else {
                            root.cancel(row.id);
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: `app.slint`'e paneli, özeti ve iletişim kutusunu ekle**

`crates/gezik/ui/app.slint` içinde:

1. İçe aktarmalara ekle ve dışa aç:

```slint
import { OpsPanel, OpRow } from "widgets/ops-panel.slint";
import { Dialog } from "widgets/dialog.slint";
```

```slint
export { Theme, PreviewInfo, CrumbItem, TabItem, SidebarRow, FileRow, ItemRow, OpRow }
```

2. `AppWindow` özelliklerine (`callback key-event(KeyEvent) -> bool;` satırından önce) ekle:

```slint
    // File operations (operations.rs): the panel above the status bar, folded into the
    // summary on the status bar's right when `ops-collapsed`.
    in property <[OpRow]> op-rows;
    in property <bool> ops-panel-open;
    in property <string> ops-summary;
    in property <bool> ops-collapsed;
    callback op-pause(int);
    callback op-resume(int);
    callback op-cancel(int);
    callback op-start-now(int);
    callback op-details(int);
    callback op-retry(int);
    callback op-dismiss(int);
    callback ops-toggle();
    // A question over the window (dialog.rs); `dialog-chosen` gets the button's index.
    in property <bool> dialog-open;
    in property <string> dialog-title;
    in property <string> dialog-message;
    in property <[string]> dialog-buttons;
    callback dialog-chosen(int);
```

3. Durum çubuğunun üstündeki ayırıcıdan (`Rectangle { height: 1px; background: Theme.border; }` — dosya görünümünü taşıyan `HorizontalLayout`'tan hemen sonraki) önce ekle:

```slint
            if root.ops-panel-open: Rectangle { height: 1px; background: Theme.border; }
            if root.ops-panel-open: OpsPanel {
                rows: root.op-rows;
                pause(i) => { root.op-pause(i); }
                resume(i) => { root.op-resume(i); }
                cancel(i) => { root.op-cancel(i); }
                start-now(i) => { root.op-start-now(i); }
                details(i) => { root.op-details(i); }
                retry(i) => { root.op-retry(i); }
                dismiss(i) => { root.op-dismiss(i); }
            }
```

4. Durum çubuğundaki `Text { text: root.notice; … }`'tan önce özeti ekle:

```slint
                    if root.ops-summary != "": Rectangle {
                        width: summary-text.preferred-width + 26px;
                        border-radius: Theme.radius;
                        background: summary-touch.has-hover ? Theme.hover : transparent;
                        accessible-role: button;
                        accessible-label: root.ops-summary;
                        accessible-action-default => { root.ops-toggle(); }
                        summary-text := Text {
                            x: 4px;
                            height: parent.height;
                            text: root.ops-summary;
                            vertical-alignment: center;
                            color: Theme.foreground-muted;
                        }
                        Path {
                            x: parent.width - 14px;
                            y: (parent.height - 8px) / 2;
                            width: 8px;
                            height: 8px;
                            viewbox-width: 8;
                            viewbox-height: 8;
                            stroke: Theme.foreground-muted;
                            stroke-width: 1.5px;
                            commands: root.ops-collapsed ? "M 1 5.5 L 4 2.5 L 7 5.5" : "M 1 2.5 L 4 5.5 L 7 2.5";
                        }
                        summary-touch := TouchArea {
                            mouse-cursor: pointer;
                            clicked => { root.ops-toggle(); }
                        }
                    }
```

5. `keys := FocusScope { … }` içinde, `VerticalLayout { … }` bloğunun kapanışından sonra (FocusScope'un son çocuğu olarak) ekle:

```slint
        if root.dialog-open: Dialog {
            x: 0;
            y: 0;
            width: root.width;
            height: root.height;
            title: root.dialog-title;
            message: root.dialog-message;
            buttons: root.dialog-buttons;
            chosen(i) => { root.dialog-chosen(i); }
        }
```

- [ ] **Step 5: `dialog.rs`'i yaz**

```rust
//! One question at a time over the window: a title, a message and buttons.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::AppWindow;

type Answer = Box<dyn FnOnce(Option<usize>)>;

struct Question {
    title: String,
    message: String,
    buttons: Vec<String>,
    answer: Answer,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    queue: RefCell<VecDeque<Question>>,
    /// The answer of the question on screen.
    open: RefCell<Option<Answer>>,
}

#[derive(Clone)]
pub struct Dialogs(Rc<Inner>);

impl Dialogs {
    pub fn new(window: &AppWindow) -> Dialogs {
        let dialogs =
            Dialogs(Rc::new(Inner { window: window.as_weak(), queue: RefCell::default(), open: RefCell::default() }));
        window.on_dialog_chosen({
            let dialogs = dialogs.clone();
            move |index| dialogs.chosen(index)
        });
        dialogs
    }

    /// Asks; `answer` gets the index of the chosen button (Enter: the first, Esc: the last).
    pub fn ask(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: &[&str],
        answer: impl FnOnce(Option<usize>) + 'static,
    ) {
        self.0.queue.borrow_mut().push_back(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            answer: Box::new(answer),
        });
        if self.0.open.borrow().is_none() {
            self.show_next();
        }
    }

    fn show_next(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let next = self.0.queue.borrow_mut().pop_front();
        match next {
            Some(question) => {
                window.set_dialog_title(question.title.into());
                window.set_dialog_message(question.message.into());
                let buttons: Vec<SharedString> = question.buttons.into_iter().map(Into::into).collect();
                window.set_dialog_buttons(ModelRc::new(VecModel::from(buttons)));
                *self.0.open.borrow_mut() = Some(question.answer);
                window.set_dialog_open(true);
            }
            None => {
                window.set_dialog_open(false);
                window.invoke_focus_list();
            }
        }
    }

    fn chosen(&self, index: i32) {
        let answer = self.0.open.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            window.set_dialog_open(false);
        }
        if let Some(answer) = answer {
            answer(usize::try_from(index).ok());
        }
        // The answer may have asked something new already.
        if self.0.open.borrow().is_none() {
            self.show_next();
        }
    }
}
```

- [ ] **Step 6: Gezinme ve görünüme yardımcıları ekle**

`crates/gezik/src/navigation.rs`:

`load`'u sessiz bir türe ayır: mevcut `fn load(&self, location: Location, mode: Mode, note: Option<String>)` gövdesini `fn load_with(&self, location: Location, mode: Mode, note: Option<String>, loading_text: bool)` olarak yeniden adlandır, içindeki

```rust
        if let Some(w) = window.upgrade() {
            w.set_status("Loading…".into());
        }
```

bloğunu `if loading_text && let Some(w) = window.upgrade() { … }` yap ve eski adı koruyan sarmalayıcıyı ekle:

```rust
    fn load(&self, location: Location, mode: Mode, note: Option<String>) {
        self.load_with(location, mode, note, true);
    }
```

`impl Navigator` içine (`reload`'dan sonra) ekle:

```rust
    /// Reloads the active tab if it shows one of `dirs` (a file operation changed them),
    /// keeping the scroll and selecting `select` (by name) if given. The status bar does not
    /// flash "Loading…".
    pub fn refresh_showing(&self, dirs: &[PathBuf], select: &[String]) {
        let Location::Path(current) = self.active_location() else { return };
        if self.0.borrow().cleared || !dirs.iter().any(|dir| gezik_core::ops::paths::same_path(dir, &current)) {
            return;
        }
        self.save_view();
        if !select.is_empty() {
            let mut inner = self.0.borrow_mut();
            let mut view = inner.tabs.active().view().clone();
            view.selected = select.to_vec();
            view.focus = select.first().cloned();
            inner.tabs.active_mut().set_view(view);
        }
        self.load_with(Location::Path(current), Mode::Show, None, false);
    }
```

`crates/gezik/src/view/mod.rs`:

`impl View` içine ekle:

```rust
    /// The folder shown; `None` for "This PC".
    pub fn folder(&self) -> Option<PathBuf> {
        self.0.data.borrow().listing.folder().map(Path::to_path_buf)
    }

    /// Shows `text` in the status bar until the selection changes.
    pub fn note(&self, text: String) {
        self.set_note(text);
    }
```

`use std::path::PathBuf;` satırını `use std::path::{Path, PathBuf};` yap.

- [ ] **Step 7: `Operations`'ı yaz**

`crates/gezik/src/operations.rs` dosyasına saf yardımcılardan sonra ekle:

```rust
thread_local! {
    static CURRENT: RefCell<Option<Operations>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's operations, if set up.
pub fn with_current(f: impl FnOnce(&Operations)) {
    if let Some(ops) = CURRENT.with(|c| c.borrow().clone()) {
        f(&ops);
    }
}

/// What to do with a job's results once their folder shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum After {
    #[default]
    Select,
    Nothing,
}

type Retry = Rc<dyn Fn() -> Box<dyn Task>>;

struct JobView {
    id: JobId,
    title: String,
    background: bool,
    shown: bool,
    progress: Option<Progress>,
    rate: Rate,
    report: Option<Report>,
    retry: Option<Retry>,
    after: After,
}

impl JobView {
    fn new(id: JobId, title: String) -> JobView {
        JobView {
            id,
            title,
            background: false,
            shown: false,
            progress: None,
            rate: Rate::new(Duration::from_secs(5)),
            report: None,
            retry: None,
            after: After::Nothing,
        }
    }

    fn row(&self) -> OpRow {
        let left = self.progress.as_ref().and_then(|p| self.rate.remaining(p.bytes_total.saturating_sub(p.bytes_done)));
        let (state, detail, progress) = describe(self.progress.as_ref(), self.report.as_ref(), self.rate.per_second(), left);
        let finished = self.report.is_some();
        let failed = state == RowState::Failed;
        let paused_by_user = matches!(self.progress.as_ref().map(|p| p.state), Some(JobState::Paused(PauseReason::User)));
        OpRow {
            id: i32::try_from(self.id).unwrap_or(i32::MAX),
            title: self.title.clone().into(),
            detail: detail.into(),
            progress,
            state: state as i32,
            can_pause: !finished && matches!(state, RowState::Running),
            can_resume: !finished && paused_by_user,
            can_start_now: !finished && state == RowState::Waiting && self.progress.is_some(),
            can_retry: failed && self.retry.is_some(),
            can_details: failed,
            finished,
        }
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    engine: Engine,
    nav: Navigator,
    view: View,
    sidebar: Sidebar,
    dialogs: Dialogs,
    rows: Rc<VecModel<OpRow>>,
    jobs: RefCell<Vec<JobView>>,
    files: Cell<FilesSettings>,
    collapsed: Cell<bool>,
    taskbar: RefCell<Option<Taskbar>>,
}

#[derive(Clone)]
pub struct Operations(Rc<Inner>);

impl Operations {
    #[allow(clippy::too_many_arguments, reason = "the parts of the app it works with")]
    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        view: View,
        sidebar: Sidebar,
        dialogs: Dialogs,
        settings: Settings,
        files: FilesSettings,
        collapsed: bool,
    ) -> Operations {
        // One wake-up for a burst of events: the flag is cleared just before draining.
        let waiting = Arc::new(AtomicBool::new(false));
        let engine = Engine::new(settings, {
            let waiting = waiting.clone();
            move || {
                if !waiting.swap(true, Ordering::SeqCst) {
                    let waiting = waiting.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        waiting.store(false, Ordering::SeqCst);
                        with_current(|ops| ops.drain());
                    });
                }
            }
        });
        let rows = Rc::new(VecModel::default());
        window.set_op_rows(ModelRc::from(rows.clone()));
        let ops = Operations(Rc::new(Inner {
            window: window.as_weak(),
            engine,
            nav,
            view,
            sidebar,
            dialogs,
            rows,
            jobs: RefCell::default(),
            files: Cell::new(files),
            collapsed: Cell::new(collapsed),
            taskbar: RefCell::default(),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(ops.clone()));
        ops
    }

    /// `[files]` changed.
    pub fn set_files(&self, files: FilesSettings) {
        self.0.files.set(files);
        self.0.engine.set_threads(files.copy_threads);
    }

    pub fn collapsed(&self) -> bool {
        self.0.collapsed.get()
    }

    pub fn toggle_collapsed(&self) {
        self.0.collapsed.set(!self.0.collapsed.get());
        self.update();
    }

    /// Runs `task`; `retry` runs the same operation again from its row, `after` says what to
    /// do with the results.
    pub fn submit(&self, task: Box<dyn Task>, retry: Option<Retry>, after: After) -> JobId {
        let title = task.title();
        let id = self.0.engine.submit(task);
        let mut job = JobView::new(id, title);
        job.retry = retry;
        job.after = after;
        self.0.jobs.borrow_mut().push(job);
        self.show_later(id);
        id
    }

    /// Finishes deletes an earlier run left unfinished.
    pub fn recover(&self) {
        let _ = self.0.engine.recover_deletes();
    }

    fn show_later(&self, id: JobId) {
        let ops = self.clone();
        slint::Timer::single_shot(SHOW_AFTER, move || {
            if let Some(job) = ops.0.jobs.borrow_mut().iter_mut().find(|j| j.id == id && j.report.is_none()) {
                job.shown = true;
            }
            ops.update();
        });
    }

    fn with_job(&self, id: JobId, f: impl FnOnce(&mut JobView)) {
        if let Some(job) = self.0.jobs.borrow_mut().iter_mut().find(|j| j.id == id) {
            f(job);
        }
    }

    /// Handles everything the engine reported.
    pub fn drain(&self) {
        for event in self.0.engine.drain() {
            match event {
                Event::Added { job, title, background, .. } => {
                    let known = self.0.jobs.borrow().iter().any(|j| j.id == job);
                    if !known {
                        let mut view = JobView::new(job, title);
                        view.background = background;
                        self.0.jobs.borrow_mut().push(view);
                        self.show_later(job);
                    } else {
                        self.with_job(job, |j| j.background = background);
                    }
                }
                Event::Progress { job, progress } => self.with_job(job, |j| {
                    j.rate.record(Instant::now(), progress.bytes_done);
                    j.progress = Some(progress);
                }),
                Event::Conflicts { job, conflicts } => self.conflicts(job, conflicts),
                Event::Paused { job, reason, path } => self.paused(job, reason, path),
                Event::Finished { job, report } => self.finished(job, report),
                Event::Changed { dirs } => self.0.nav.refresh_showing(&dirs, &[]),
                Event::History => {}
            }
        }
        self.update();
    }

    /// Until the conflict list exists (Task 14): every conflict keeps its default (Skip).
    fn conflicts(&self, job: JobId, conflicts: Vec<gezik_ops::ConflictItem>) {
        let decisions = conflicts.iter().map(|c| c.decision).collect();
        self.0.engine.decide(job, decisions);
    }

    fn paused(&self, job: JobId, reason: PauseReason, path: Option<PathBuf>) {
        self.with_job(job, |j| j.shown = true);
        let (title, message) = match reason {
            PauseReason::User => return,
            PauseReason::DiskFull => (
                "The disk is full".to_owned(),
                format!(
                    "Free some space{}, then resume.",
                    path.as_deref().map(|p| format!(" for {}", p.display())).unwrap_or_default()
                ),
            ),
            PauseReason::ManyFailures => (
                "Many items failed in a row".to_owned(),
                "The drive may have been disconnected. Resume to go on, or cancel.".to_owned(),
            ),
        };
        // Waiting for the user: the panel opens by itself.
        self.0.collapsed.set(false);
        let engine = self.0.engine.clone();
        self.0.dialogs.ask(title, message, &["Resume", "Cancel"], move |choice| match choice {
            Some(0) => engine.resume(job),
            _ => engine.cancel(job),
        });
    }

    fn finished(&self, id: JobId, report: Report) {
        let mut after = After::Nothing;
        self.with_job(id, |job| {
            after = job.after;
            if !report.failures.is_empty() {
                job.shown = true;
            }
            job.report = Some(report.clone());
        });
        if !report.failures.is_empty() {
            // Something failed: the panel opens by itself.
            self.0.collapsed.set(false);
        }
        let select = match (after, self.0.view.folder()) {
            (After::Nothing, _) | (_, None) => Vec::new(),
            (_, Some(folder)) => result_names(&report.results, &folder),
        };
        self.0.nav.refresh_showing(&report.changed_dirs, &select);
        self.0.sidebar.refresh();
        if report.skipped_changed > 0 {
            let n = report.skipped_changed;
            let what = if n == 1 { "1 item".to_owned() } else { format!("{n} items") };
            self.0.view.note(format!("{what} changed since; skipped"));
        }
        if !report.no_trash.is_empty() {
            self.ask_delete_for_good(report.no_trash.clone());
        }
        if report.failures.is_empty() {
            let ops = self.clone();
            slint::Timer::single_shot(DONE_FOR, move || ops.remove(id));
        }
    }

    /// Items whose drive has no trash: delete them for good?
    fn ask_delete_for_good(&self, paths: Vec<PathBuf>) {
        let bin = if cfg!(windows) { "Recycle Bin" } else { "trash" };
        let title = if paths.len() == 1 {
            format!("{} cannot go to the {bin}", paths[0].file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
        } else {
            format!("{} items cannot go to the {bin}", paths.len())
        };
        let ops = self.clone();
        self.0.dialogs.ask(
            title,
            format!("This drive has no {bin}. Delete permanently? This cannot be undone."),
            &["Delete", "Cancel"],
            move |choice| {
                if choice == Some(0) {
                    let pending = ops.0.engine.pending_deletes();
                    ops.submit(Box::new(gezik_ops::DeleteTask::new(paths, pending)), None, After::Nothing);
                }
            },
        );
    }

    fn remove(&self, id: JobId) {
        self.0.jobs.borrow_mut().retain(|j| j.id != id);
        self.update();
    }

    /// Redraws the panel, the summary and the taskbar.
    fn update(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let jobs = self.0.jobs.borrow();
        let shown: Vec<&JobView> = jobs.iter().filter(|j| j.shown).collect();
        let rows: Vec<OpRow> = shown.iter().map(|j| j.row()).collect();
        let states: Vec<(RowState, f32)> = rows
            .iter()
            .map(|r| {
                let state = match r.state {
                    0 => RowState::Running,
                    1 => RowState::Waiting,
                    2 => RowState::Paused,
                    3 => RowState::Deciding,
                    5 => RowState::Failed,
                    _ => RowState::Done,
                };
                (state, r.progress.max(0.0))
            })
            .collect();
        let running: Vec<&(RowState, f32)> = states.iter().filter(|(s, _)| !matches!(s, RowState::Done | RowState::Failed)).collect();
        let overall = (!running.is_empty()).then(|| running.iter().map(|(_, f)| *f).sum::<f32>() / running.len() as f32);
        let failed_items: usize = shown.iter().filter_map(|j| j.report.as_ref()).map(|r| r.failures.len()).sum();
        window.set_ops_summary(summary_text(running.len(), overall, failed_items, rows.len()).into());
        window.set_ops_collapsed(self.0.collapsed.get());
        window.set_ops_panel_open(!rows.is_empty() && !self.0.collapsed.get());
        sync_model(&self.0.rows, rows.into_iter());
        let (state, done, total) = taskbar_progress(&states);
        let mut taskbar = self.0.taskbar.borrow_mut();
        if taskbar.is_none() && state != TaskbarState::Off {
            *taskbar = Some(Taskbar::new(&window.window().window_handle()));
        }
        if let Some(taskbar) = taskbar.as_ref() {
            taskbar.set(state, done, total);
        }
    }

    fn id(id: i32) -> JobId {
        JobId::try_from(id).unwrap_or(0)
    }

    pub fn pause(&self, id: i32) {
        self.0.engine.pause(Self::id(id));
    }

    pub fn resume(&self, id: i32) {
        self.0.engine.resume(Self::id(id));
    }

    pub fn cancel(&self, id: i32) {
        self.0.engine.cancel(Self::id(id));
    }

    pub fn start_now(&self, id: i32) {
        self.0.engine.start_now(Self::id(id));
    }

    pub fn dismiss(&self, id: i32) {
        self.remove(Self::id(id));
    }

    /// Runs a failed row's operation again (what is done already shows as identical and is
    /// skipped).
    pub fn retry(&self, id: i32) {
        let id = Self::id(id);
        let retry = self.0.jobs.borrow().iter().find(|j| j.id == id).and_then(|j| j.retry.clone());
        if let Some(retry) = retry {
            self.remove(id);
            self.submit(retry(), Some(retry.clone()), After::Select);
        }
    }

    /// The failures of a row, with Retry.
    pub fn details(&self, id: i32) {
        let id = Self::id(id);
        let (title, message, can_retry) = {
            let jobs = self.0.jobs.borrow();
            let Some(job) = jobs.iter().find(|j| j.id == id) else { return };
            let Some(report) = &job.report else { return };
            let failures = &report.failures;
            let mut lines: Vec<String> = failures
                .iter()
                .take(MAX_DETAILS)
                .map(|f| format!("{}: {}", f.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), f.message))
                .collect();
            if failures.len() > MAX_DETAILS {
                lines.push(format!("…and {} more", failures.len() - MAX_DETAILS));
            }
            (job.title.clone(), lines.join("\n"), job.retry.is_some())
        };
        let ops = self.clone();
        let buttons: &[&str] = if can_retry { &["Retry", "Close"] } else { &["Close"] };
        self.0.dialogs.ask(title, message, buttons, move |choice| {
            if can_retry && choice == Some(0) {
                ops.retry(i32::try_from(id).unwrap_or(0));
            }
        });
    }

    /// Asks before closing while operations run; `quit` runs if the user cancels them.
    /// Returns whether the window must stay open for now.
    pub fn confirm_close(&self, quit: impl FnOnce() + 'static) -> bool {
        if !self.0.engine.busy() {
            return false;
        }
        let running = self.0.jobs.borrow().iter().filter(|j| j.report.is_none() && !j.background).count().max(1);
        let title = if running == 1 { "1 operation is running".to_owned() } else { format!("{running} operations are running") };
        let engine = self.0.engine.clone();
        self.0.dialogs.ask(
            title,
            "Quitting cancels them. What is already copied or moved stays.",
            &["Keep open", "Cancel them and quit"],
            move |choice| {
                if choice == Some(1) {
                    engine.cancel_all();
                    engine.wait_idle(Duration::from_secs(3));
                    quit();
                }
            },
        );
        true
    }
}
```

- [ ] **Step 8: `main.rs`'te bağla**

1. `apply_config` içinde `frame_limit::set_max_fps(…)` satırından sonra ekle:

```rust
    operations::with_current(|ops| ops.set_files(loaded.settings.files));
```

2. `handle_key`'in en başına (`let editing = …`'dan önce) ekle:

```rust
    // A question or the conflict list over the window has the keyboard.
    if window.get_dialog_open() {
        return false;
    }
```

3. `main` içinde, `sidebar.set_pinned(initial_settings.pinned);` satırından sonra `Operations`'ı kur (kenar çubuğu `Menus::new`'e taşınmadan önce klonlanır):

```rust
    let dialogs = dialog::Dialogs::new(&window);
    let engine_settings = gezik_ops::Settings {
        threads: initial_settings.files.copy_threads,
        pending_deletes: config.as_ref().map(|store| store.dir().join("pending-deletes")),
    };
    let ops = operations::Operations::new(
        &window,
        nav.clone(),
        view.clone(),
        sidebar.clone(),
        dialogs,
        engine_settings,
        initial_settings.files,
        saved_state.operations_collapsed,
    );
    window.on_op_pause({
        let ops = ops.clone();
        move |id| ops.pause(id)
    });
    window.on_op_resume({
        let ops = ops.clone();
        move |id| ops.resume(id)
    });
    window.on_op_cancel({
        let ops = ops.clone();
        move |id| ops.cancel(id)
    });
    window.on_op_start_now({
        let ops = ops.clone();
        move |id| ops.start_now(id)
    });
    window.on_op_details({
        let ops = ops.clone();
        move |id| ops.details(id)
    });
    window.on_op_retry({
        let ops = ops.clone();
        move |id| ops.retry(id)
    });
    window.on_op_dismiss({
        let ops = ops.clone();
        move |id| ops.dismiss(id)
    });
    window.on_ops_toggle({
        let ops = ops.clone();
        move || ops.toggle_collapsed()
    });
    // Deletes cut short last time finish in the background once the window is up.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), {
        let ops = ops.clone();
        move || ops.recover()
    });
```

4. Mevcut `window.window().on_close_requested({ … })` bloğunu sil ve `Operations` kurulduktan sonra şununla değiştir (gövde aynı, panelin daraltılma durumu da kaydedilir; işlem sürerken önce sorulur):

```rust
    let save_and_quit: Rc<dyn Fn()> = {
        let (weak, store, view, preview, ops) =
            (window.as_weak(), config.clone(), view.clone(), preview.clone(), ops.clone());
        Rc::new(move || {
            if let (Some(window), Some(store)) = (weak.upgrade(), &store) {
                let mut state = store.load_state();
                window_state::capture_into(&window, &mut state);
                state.columns = Some(view.columns());
                state.preview_open = preview.is_pane_open();
                state.preview_width = Some(window.get_preview_width().round().clamp(200.0, 600.0) as u32);
                state.operations_collapsed = ops.collapsed();
                if let Err(err) = store.save_state(&state) {
                    eprintln!("gezik: cannot save window state: {err}");
                }
                view.flush_memory();
            }
            // Its window would otherwise keep the event loop (and the process) running.
            preview.close_quick_look();
        })
    };
    window.window().on_close_requested({
        let (ops, save_and_quit, weak) = (ops.clone(), save_and_quit.clone(), window.as_weak());
        move || {
            let quit = {
                let (save_and_quit, weak) = (save_and_quit.clone(), weak.clone());
                move || {
                    save_and_quit();
                    if let Some(window) = weak.upgrade() {
                        let _ = window.hide();
                    }
                    let _ = slint::quit_event_loop();
                }
            };
            if ops.confirm_close(quit) {
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            save_and_quit();
            slint::CloseRequestResponse::HideWindow
        }
    });
```

`use std::rc::Rc;` ekle. `Menus::new(&window, nav.clone(), view.clone(), preview.clone(), sidebar)` çağrısı `sidebar`'ı taşır; `Operations::new`'e verilen `sidebar.clone()` bundan önce olduğu için sıralama doğrudur.

- [ ] **Step 9: Derle, test et, elle bak**

Run: `~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace`
Expected: PASS (`operations::tests` dahil).
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

Elle (henüz kısayol yok; panel boş kalır ve görünmez): `GEZIK_CONFIG_DIR` geçici klasörle uygulamayı aç, açılış ve kapanışın eskisi gibi olduğunu, durum çubuğunda özet görünmediğini doğrula. Yapılandırma klasörüne `.gezik-deleting-test` adında bir klasör yolu içeren `pending-deletes` dosyası koy (klasörü de oluştur), uygulamayı aç: klasör yarım saniye sonra silinir, dosya kaybolur.

- [ ] **Step 10: Commit**

```bash
git add crates/gezik
git commit -m "Operations in the app: engine wiring, progress panel and summary, dialogs, taskbar, close prompt"
```

---

### Task 13: Uygulama — yerinde yeniden adlandırma, yeni klasör ve yeni dosya

**Files:**
- Create: `crates/gezik/ui/widgets/rename-field.slint`
- Modify: `crates/gezik/ui/widgets/file-view.slint`
- Modify: `crates/gezik/ui/app.slint`
- Modify: `crates/gezik/src/view/mod.rs` (yeniden adlandırma durumu, `on_shown`, `single_selected`, `has_other_named`)
- Modify: `crates/gezik/src/operations.rs` (`After::Rename`, yeniden adlandırma ve yeni öğe komutları, `rename_check`)
- Modify: `crates/gezik/src/main.rs` (geri çağrılar, F2 / Ctrl+Shift+N, macOS ⌘↓)

**Interfaces:**
- Consumes: Task 12 (`Operations::submit`, `After`), `gezik_core::ops::names::{rename_selection, validate_name, NameRules}` (Task 1), `gezik_ops::{MoveTask, NewTask}`
- Produces:
  - `View::begin_rename(&self, index: usize) -> bool`, `View::begin_rename_by_name(&self, name: &str) -> bool`, `View::renaming(&self) -> Option<(usize, String)>`, `View::end_rename(&self)`, `View::single_selected(&self) -> Option<usize>`, `View::has_other_named(&self, name: &str, except: usize) -> bool`, `View::on_shown(&self, f: impl Fn() + 'static)`
  - `operations::After::Rename`; `Operations::{rename_start(), rename_edited(&str), rename_accepted(String), rename_cancelled(), rename_tab(String, bool), new_folder(Option<PathBuf>)}` (`new_file` menüyle birlikte Task 14'te)
  - saf: `operations::rename_check(typed: &str, old: &str, taken: impl Fn(&str) -> bool) -> Result<Option<String>, String>` (`Ok(None)`: değişmedi; `Ok(Some(ad))`: yeni ad; `Err(ileti)`)
  - Slint: `AppWindow` özellikleri `renaming-index`, `rename-text`, `rename-select`, `rename-error`; geri çağrılar `rename-accepted(string)`, `rename-cancelled()`, `rename-tab(string, bool)`, `rename-edited(string)`

- [ ] **Step 1: Ad denetimini testleriyle yaz**

`crates/gezik/src/operations.rs` saf yardımcılarına ekle:

```rust
/// Whether `typed` can replace the name `old`: `Ok(None)` if it stays the same, `Ok(Some(name))`
/// with surrounding spaces trimmed, or the problem to show. `taken` says whether another entry
/// in the folder already has a name.
pub fn rename_check(typed: &str, old: &str, taken: impl Fn(&str) -> bool) -> Result<Option<String>, String> {
    let name = typed.trim();
    if name == old {
        return Ok(None);
    }
    gezik_core::ops::names::validate_name(name, gezik_core::ops::names::NameRules::current())
        .map_err(|err| err.to_string())?;
    if taken(name) {
        return Err("A file with this name already exists".to_owned());
    }
    Ok(Some(name.to_owned()))
}
```

Test modülüne:

```rust
    #[test]
    fn rename_checks() {
        let taken = |name: &str| name == "b.txt";
        assert_eq!(rename_check("a.txt", "a.txt", taken), Ok(None));
        assert_eq!(rename_check("  c.txt ", "a.txt", taken), Ok(Some("c.txt".into())));
        assert_eq!(rename_check("b.txt", "a.txt", taken), Err("A file with this name already exists".into()));
        assert_eq!(rename_check("", "a.txt", taken), Err("Type a name".into()));
        assert!(rename_check("a/b", "a.txt", taken).is_err());
    }
```

Run: `~/.cargo/bin/cargo test -p gezik rename_checks`
Expected: PASS.

- [ ] **Step 2: Yeniden adlandırma alanını yaz**

`crates/gezik/ui/widgets/rename-field.slint`:

```slint
import { Theme } from "../theme.slint";

// The name of an entry being renamed. Enter or a click elsewhere keeps the new name, Esc
// keeps the old one, Tab keeps it and goes on to the next entry (Shift+Tab: the previous).
export component RenameField inherits Rectangle {
    in-out property <string> text;
    // Selected when it opens: the name up to its extension (bytes).
    in property <int> select-end;
    in property <bool> invalid;
    in property <bool> centered;
    callback accepted(string);
    callback cancelled();
    callback tab(string, bool);
    callback edited(string);

    // Esc, Enter or Tab ended it: losing the focus right after does not accept again.
    property <bool> done;

    background: Theme.background;
    border-width: 1px;
    border-color: root.invalid ? Theme.danger : Theme.accent;
    border-radius: 2px;
    accessible-role: text-input;
    accessible-label: "Name";
    accessible-value: root.text;

    FocusScope {
        key-pressed(event) => {
            if event.text == Key.Escape {
                root.done = true;
                root.cancelled();
                return accept;
            }
            if event.text == Key.Tab || event.text == Key.Backtab {
                root.done = true;
                root.tab(input.text, event.modifiers.shift || event.text == Key.Backtab);
                return accept;
            }
            return reject;
        }

        input := TextInput {
            x: 3px;
            width: parent.width - 6px;
            height: parent.height;
            vertical-alignment: center;
            horizontal-alignment: root.centered ? center : left;
            single-line: true;
            text <=> root.text;
            color: Theme.foreground;
            selection-background-color: Theme.selection;
            selection-foreground-color: Theme.selection-foreground;
            init => {
                self.focus();
                self.set-selection-offsets(0, root.select-end);
            }
            edited => {
                root.done = false;
                root.edited(self.text);
            }
            accepted => {
                root.done = true;
                root.accepted(self.text);
            }
            changed has-focus => {
                if !self.has-focus && !root.done {
                    root.done = true;
                    root.accepted(self.text);
                }
            }
        }
    }
}
```

- [ ] **Step 3: `file-view.slint`'e yeniden adlandırmayı ekle**

İçe aktarma: `import { RenameField } from "rename-field.slint";`

`FileView`'e `in property <bool> sort-desc;` satırından sonra ekle:

```slint
    // The entry being renamed (-1: none): its name shows as a text field.
    in property <int> renaming-index: -1;
    in-out property <string> rename-text;
    // Where the selection ends when renaming starts (bytes).
    in property <int> rename-select;
    // Why the typed name cannot be used; shown under the field.
    in property <string> rename-error;
    callback rename-accepted(string);
    callback rename-cancelled();
    // Tab (false) or Shift+Tab (true): keep the name, go on to the next or previous entry.
    callback rename-tab(string, bool);
    callback rename-edited(string);
```

Liste satırında adı gösteren `Text { text: entry.file.name; … }` öğesini şu ikiliyle değiştir:

```slint
                        if entry.index != root.renaming-index: Text {
                            text: entry.file.name;
                            vertical-alignment: center;
                            horizontal-stretch: 1;
                            overflow: elide;
                            color: entry.text-color;
                        }
                        if entry.index == root.renaming-index: RenameField {
                            horizontal-stretch: 1;
                            text <=> root.rename-text;
                            select-end: root.rename-select;
                            invalid: root.rename-error != "";
                            accepted(t) => { root.rename-accepted(t); }
                            cancelled => { root.rename-cancelled(); }
                            tab(t, back) => { root.rename-tab(t, back); }
                            edited(t) => { root.rename-edited(t); }
                        }
```

Izgara döşemesindeki ad `Text { x: 8px; y: 14px + root.picture-size; … }` öğesinin başına `if tile.index != root.renaming-index:` koşulunu ekle ve hemen ardından ekle:

```slint
                        if tile.index == root.renaming-index: RenameField {
                            x: 8px;
                            y: 14px + root.picture-size;
                            width: parent.width - 16px;
                            height: Theme.row-height;
                            centered: true;
                            text <=> root.rename-text;
                            select-end: root.rename-select;
                            invalid: root.rename-error != "";
                            accepted(t) => { root.rename-accepted(t); }
                            cancelled => { root.rename-cancelled(); }
                            tab(t, back) => { root.rename-tab(t, back); }
                            edited(t) => { root.rename-edited(t); }
                        }
```

`FileView`'in en sonuna (çerçeve seçimi dikdörtgeninden sonra) hatayı ekle — her şeyin üstünde çizilir:

```slint
    // Why the typed name cannot be used, just under the entry.
    if root.rename-error != "" && root.renaming-index >= 0: Rectangle {
        property <int> line: floor(root.renaming-index / root.per-row);
        x: list.x + (root.mode == 0 ? root.side + Theme.icon-size + root.gap : mod(root.renaming-index, root.per-row) * root.cell-width + 8px);
        y: list.y + root.scroll + (self.line + 1) * root.item-height;
        width: error-text.preferred-width + Theme.spacing * 2;
        height: error-text.preferred-height + Theme.spacing;
        background: Theme.danger;
        border-radius: Theme.radius;
        accessible-role: text;
        accessible-label: root.rename-error;
        error-text := Text {
            x: Theme.spacing;
            y: Theme.spacing / 2;
            text: root.rename-error;
            color: Theme.background;
        }
    }
```

- [ ] **Step 4: `app.slint`'te bağla**

`AppWindow` özelliklerine ekle:

```slint
    // Renaming in place (view/mod.rs, operations.rs).
    in property <int> renaming-index: -1;
    in-out property <string> rename-text;
    in property <int> rename-select;
    in property <string> rename-error;
    callback rename-accepted(string);
    callback rename-cancelled();
    callback rename-tab(string, bool);
    callback rename-edited(string);
```

`file-view := FileView { … }` içine ekle:

```slint
                    renaming-index: root.renaming-index;
                    rename-text <=> root.rename-text;
                    rename-select: root.rename-select;
                    rename-error: root.rename-error;
                    rename-accepted(t) => { root.rename-accepted(t); }
                    rename-cancelled => { root.rename-cancelled(); }
                    rename-tab(t, back) => { root.rename-tab(t, back); }
                    rename-edited(t) => { root.rename-edited(t); }
```

- [ ] **Step 5: Görünüme yeniden adlandırma durumunu ekle**

`crates/gezik/src/view/mod.rs`:

`use` satırlarına ekle: `use gezik_core::ops::names::rename_selection;`

`Inner`'a ekle (ve `View::new`'de `renaming: RefCell::new(None), on_shown: RefCell::new(Vec::new()),` ile kur):

```rust
    /// The entry being renamed: its index and its name before.
    renaming: RefCell<Option<(usize, String)>>,
    /// Called after each `show` (a folder loaded or reloaded).
    on_shown: RefCell<Vec<Listener>>,
```

`show`'un içinde `self.0.model.notify.reset();` satırından önce ekle (bir yenileme ad değiştirmeyi bozmasın):

```rust
        let renamed = self.0.renaming.borrow().as_ref().map(|(_, name)| name.clone());
        if let Some(name) = renamed {
            let index = self.0.data.borrow().listing.index_of(&name);
            match index {
                Some(index) => {
                    *self.0.renaming.borrow_mut() = Some((index, name));
                    if let Some(window) = self.0.window.upgrade() {
                        window.set_renaming_index(i32::try_from(index).unwrap_or(-1));
                    }
                }
                None => self.end_rename(),
            }
        }
```

`show`'un sonuna (`self.notify_listeners();`'tan sonra) ekle:

```rust
        let shown = self.0.on_shown.borrow().clone();
        for f in &shown {
            f();
        }
```

`impl View` içine ekle:

```rust
    /// Calls `f` after each folder load or reload is shown.
    pub fn on_shown(&self, f: impl Fn() + 'static) {
        self.0.on_shown.borrow_mut().push(Rc::new(f));
    }

    /// The only selected entry, if exactly one is.
    pub fn single_selected(&self) -> Option<usize> {
        let data = self.0.data.borrow();
        (data.selection.count() == 1).then(|| data.selection.iter().next()).flatten()
    }

    /// Whether an entry other than `except` is called `name` (ignoring case where the file
    /// system does).
    pub fn has_other_named(&self, name: &str, except: usize) -> bool {
        let data = self.0.data.borrow();
        (0..data.listing.len()).filter(|&i| i != except).any(|i| {
            data.listing.name_at(i).is_some_and(|other| {
                if cfg!(any(windows, target_os = "macos")) { other.to_lowercase() == name.to_lowercase() } else { other == name }
            })
        })
    }

    /// Turns entry `index`'s name into a text field (files and folders only, not drives).
    pub fn begin_rename(&self, index: usize) -> bool {
        let (name, is_dir) = {
            let data = self.0.data.borrow();
            if !matches!(data.listing, Listing::Files(..)) {
                return false;
            }
            let Some(name) = data.listing.name_at(index) else { return false };
            (name.to_owned(), data.listing.is_dir(index))
        };
        let changes = self.0.data.borrow_mut().selection.select_only(index);
        self.after_selection(&changes);
        self.reveal(index);
        let (_, end) = rename_selection(&name, is_dir);
        let Some(window) = self.0.window.upgrade() else { return false };
        window.set_rename_text(name.clone().into());
        window.set_rename_select(i32::try_from(end).unwrap_or(0));
        window.set_rename_error("".into());
        *self.0.renaming.borrow_mut() = Some((index, name));
        window.set_renaming_index(i32::try_from(index).unwrap_or(-1));
        true
    }

    pub fn begin_rename_by_name(&self, name: &str) -> bool {
        let index = self.0.data.borrow().listing.index_of(name);
        index.is_some_and(|index| self.begin_rename(index))
    }

    /// The entry being renamed (index, name before).
    pub fn renaming(&self) -> Option<(usize, String)> {
        self.0.renaming.borrow().clone()
    }

    pub fn end_rename(&self) {
        if self.0.renaming.borrow_mut().take().is_none() {
            return;
        }
        if let Some(window) = self.0.window.upgrade() {
            window.set_renaming_index(-1);
            window.set_rename_error("".into());
            window.invoke_focus_list();
        }
    }
```

- [ ] **Step 6: Komutları `Operations`'a ekle**

`After`'a `Rename` ekle:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum After {
    #[default]
    Select,
    /// Select and rename the first result (a new folder or file).
    Rename,
    Nothing,
}
```

`Inner`'a `rename_when_shown: RefCell<Option<PathBuf>>,` ekle (`Operations::new`'de `RefCell::default()`), ve `new` içinde `CURRENT` kurulmadan önce:

```rust
        ops.0.view.on_shown({
            let weak = Rc::downgrade(&ops.0);
            move || {
                if let Some(inner) = weak.upgrade() {
                    Operations(inner).folder_shown();
                }
            }
        });
```

`finished` içinde `self.0.nav.refresh_showing(...)` çağrısından önce ekle:

```rust
        if after == After::Rename {
            *self.0.rename_when_shown.borrow_mut() = report.results.first().cloned();
        }
```

`impl Operations` içine ekle:

```rust
    /// A folder is on screen: start a rename that waited for it (a new folder).
    fn folder_shown(&self) {
        let Some(path) = self.0.rename_when_shown.borrow().clone() else { return };
        let Some(folder) = self.0.view.folder() else { return };
        if !path.parent().is_some_and(|parent| same_path(parent, &folder)) {
            return;
        }
        self.0.rename_when_shown.borrow_mut().take();
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            self.0.view.begin_rename_by_name(&name);
        }
    }

    /// F2: renames the selected entry (or the focused one).
    pub fn rename_start(&self) {
        let view = &self.0.view;
        if let Some(index) = view.single_selected().or_else(|| view.focus()) {
            view.begin_rename(index);
        }
    }

    fn set_rename_error(&self, error: &str) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_rename_error(error.into());
        }
    }

    /// While typing: say at once what is wrong with the name.
    pub fn rename_edited(&self, typed: &str) {
        let Some((index, old)) = self.0.view.renaming() else { return };
        let error = rename_check(typed, &old, |name| self.0.view.has_other_named(name, index)).err().unwrap_or_default();
        self.set_rename_error(&error);
    }

    /// Ends renaming with `typed`: the entry's index if the field closed (unchanged or
    /// renamed), `None` if the name cannot be used (the field stays, with the problem shown).
    fn commit_rename(&self, typed: &str) -> Option<usize> {
        let view = &self.0.view;
        let (index, old) = view.renaming()?;
        match rename_check(typed, &old, |name| view.has_other_named(name, index)) {
            Err(error) => {
                self.set_rename_error(&error);
                None
            }
            Ok(None) => {
                view.end_rename();
                Some(index)
            }
            Ok(Some(name)) => {
                let path = view.entry_path(index).map(|(path, _)| path);
                view.end_rename();
                if let Some(path) = path {
                    self.submit(Box::new(gezik_ops::MoveTask::rename(path, &name)), None, After::Select);
                }
                Some(index)
            }
        }
    }

    pub fn rename_accepted(&self, typed: String) {
        self.commit_rename(&typed);
    }

    pub fn rename_cancelled(&self) {
        self.0.view.end_rename();
    }

    /// Tab / Shift+Tab: keep the name and rename the next / previous entry.
    pub fn rename_tab(&self, typed: String, back: bool) {
        let Some(index) = self.commit_rename(&typed) else { return };
        let next = if back { index.checked_sub(1) } else { Some(index + 1) };
        if let Some(next) = next {
            self.0.view.begin_rename(next);
        }
    }

    /// A new folder in `dir` (else the folder shown), renamed right away.
    pub fn new_folder(&self, dir: Option<PathBuf>) {
        if let Some(dir) = dir.or_else(|| self.0.view.folder()) {
            self.submit(Box::new(gezik_ops::NewTask::folder(&dir)), None, After::Rename);
        }
    }

```

- [ ] **Step 7: `main.rs`'te bağla**

1. `handle_key` imzasına `ops: &operations::Operations` parametresini ekle (çağrıda `&ops`), ve diyalog denetiminden hemen sonra:

```rust
    // The name field being edited has the keyboard (Enter, Esc, Tab are its own).
    if view.renaming().is_some() {
        return false;
    }
```

2. `match action`'daki geçici kolu daralt: `Action::Rename => ops.rename_start(),` ve `Action::NewFolder => ops.new_folder(None),` ekle; diğer dokuz yeni eylem `=> return false` kolunda kalır. Bu iki eylem için, adres çubuğuna yazarken çalışmasınlar diye `match action`'dan önce:

```rust
            if editing && matches!(action, Action::Rename | Action::NewFolder) {
                return false;
            }
            if action == Action::Rename && !window.get_list_focused() {
                return false;
            }
```

3. Liste tuşları bölümünde, `let mv = match chord.key { … }` satırından önce (macOS'ta Enter yeniden adlandırdığı için açma ⌘↓'dir, Finder gibi):

```rust
            if platform == Platform::Mac && primary && !chord.shift && chord.key == Key::Down {
                nav.open_selected();
                return true;
            }
```

4. Geri çağrıları ekle (`Operations` kurulduktan sonra):

```rust
    window.on_rename_accepted({
        let ops = ops.clone();
        move |text| ops.rename_accepted(text.into())
    });
    window.on_rename_cancelled({
        let ops = ops.clone();
        move || ops.rename_cancelled()
    });
    window.on_rename_tab({
        let ops = ops.clone();
        move |text, back| ops.rename_tab(text.into(), back)
    });
    window.on_rename_edited({
        let ops = ops.clone();
        move |text| ops.rename_edited(&text)
    });
```

`window.on_key_event` kapanışı `ops`'u da yakalamalı: `let (nav, view, preview, ops, weak) = (…, ops.clone(), …);` ve `handle_key(&window, &nav, &view, &preview, &ops, …)`. Bunun için `on_key_event` bağlantısını `Operations` kurulduktan sonraya taşı.

- [ ] **Step 8: Derle, test et, elle doğrula**

Run: `~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace`
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

Elle (`GEZIK_CONFIG_DIR` geçici klasör, bir deneme klasöründe):
- F2: uzantısız ad seçili gelir; yeni ad + Enter → dosya yeniden adlandırılır, yeni ad seçili; Ctrl+Z henüz bağlı değil.
- `a/b` yazınca alanın altında kırmızı ipucu; Enter bir şey yapmaz; Esc eski adı korur; Tab bir sonrakine geçer.
- Var olan bir adı yazınca "A file with this name already exists".
- Ctrl+Shift+N: "New folder" oluşur ve hemen yeniden adlandırma açılır; tekrar → "New folder (2)".
- Izgara görünümünde F2: ad döşemenin altında ortalı alan olur.

- [ ] **Step 9: Commit**

```bash
git add crates/gezik
git commit -m "Rename in place in the list and grid, new folder renamed right away"
```

---

### Task 14: Uygulama — pano, kesilmiş öğeler, sil/çöp, çoğalt, geri al, kısayollar ve menüler

**Files:**
- Modify: `crates/gezik/src/operations.rs` (pano, komutlar)
- Modify: `crates/gezik/src/view/mod.rs`, `crates/gezik/src/view/model.rs` (`hide_names`, kesilmiş adlar, `FileRow.cut`)
- Modify: `crates/gezik/ui/widgets/file-view.slint` (`cut` saydamlığı)
- Modify: `crates/gezik/src/context_menu.rs` (dosya öğeleri, arka plan menüsü, Explorer komutları)
- Modify: `crates/gezik/src/keys.rs` (`acts_on_files`, `needs_list`)
- Modify: `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: Task 7 (`gezik_platform::clipboard`, `ShellVerb`, `MenuOutcome::Verb`), Task 12–13 (`Operations`, `After`), `gezik_ops::{CopyTask, MoveTask, TrashTask, DeleteTask, NewTask}`
- Produces:
  - `Operations::{copy(cut: bool), copy_paths(Vec<PathBuf>, cut: bool), paste(into: Option<PathBuf>, force_move: bool), can_paste() -> bool, trash(permanent: bool), trash_paths(Vec<PathBuf>, permanent: bool), duplicate(), undo(), redo(), undo_label() -> Option<String>, redo_label() -> Option<String>, new_file(Option<PathBuf>), clipboard_check()}`
  - saf: `operations::items_text(&[PathBuf]) -> String`
  - `View::hide_names(&self, names: &[String])`, `View::set_cut_names(&self, names: HashSet<String>)`; `ViewData.cut: HashSet<String>`; Slint `FileRow.cut: bool`
  - `context_menu`: id'ler `UNDO 50, REDO 51, PASTE 52, NEW_FOLDER 53, NEW_FILE 54, REFRESH 55, CUT 56, COPY 57, DUPLICATE 58, RENAME 59, TRASH 60, DELETE_PERMANENTLY 61, PASTE_INTO 62`; `file_items(single: bool, folder: bool, can_paste: bool) -> Vec<(u32, &'static str)>`, `background_items(undo: Option<&str>, redo: Option<&str>, can_paste: bool) -> Vec<(u32, String)>`; `Menus::new(…, ops: Operations)`; `Menus::background(x: f32, y: f32)`
  - `keys::acts_on_files(Action) -> bool`, `keys::needs_list(Action) -> bool`

- [ ] **Step 1: Saf yardımcıları ve menü listelerini testleriyle yaz**

`crates/gezik/src/operations.rs` saf yardımcılarına:

```rust
/// `notlar.txt`, or `3 items` (for questions like "Delete 3 items permanently?").
pub fn items_text(paths: &[PathBuf]) -> String {
    match paths {
        [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| one.display().to_string()),
        many => format!("{} items", many.len()),
    }
}
```

test:

```rust
    #[test]
    fn items_read_well_in_questions() {
        assert_eq!(items_text(&[PathBuf::from("/a/notlar.txt")]), "notlar.txt");
        assert_eq!(items_text(&[PathBuf::from("/a"), PathBuf::from("/b")]), "2 items");
    }
```

`crates/gezik/src/context_menu.rs`'e id'leri ve listeleri ekle (`RESET_FOLDER`'dan sonra):

```rust
pub const UNDO: u32 = 50;
pub const REDO: u32 = 51;
pub const PASTE: u32 = 52;
pub const NEW_FOLDER: u32 = 53;
pub const NEW_FILE: u32 = 54;
pub const REFRESH: u32 = 55;
pub const CUT: u32 = 56;
pub const COPY: u32 = 57;
pub const DUPLICATE: u32 = 58;
pub const RENAME: u32 = 59;
pub const TRASH: u32 = 60;
pub const DELETE_PERMANENTLY: u32 = 61;
pub const PASTE_INTO: u32 = 62;

/// Gezik's file items for rows on macOS and Linux; Windows has them in its own menu (and
/// Gezik takes them over, see `Menus::run_verb`).
pub fn file_items(single: bool, folder: bool, can_paste: bool) -> Vec<(u32, &'static str)> {
    let mut out = vec![(CUT, "Cut"), (COPY, "Copy")];
    if folder && can_paste {
        out.push((PASTE_INTO, "Paste into folder"));
    }
    out.push((DUPLICATE, "Duplicate"));
    if single {
        out.push((RENAME, "Rename"));
    }
    out.push((TRASH, "Move to Trash"));
    out.push((DELETE_PERMANENTLY, "Delete permanently"));
    out
}

/// The items for empty space in a folder: Undo/Redo say what they would do.
pub fn background_items(undo: Option<&str>, redo: Option<&str>, can_paste: bool) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if let Some(label) = undo {
        out.push((UNDO, format!("Undo {label}")));
    }
    if let Some(label) = redo {
        out.push((REDO, format!("Redo {label}")));
    }
    if can_paste {
        out.push((PASTE, "Paste".to_owned()));
    }
    out.push((NEW_FOLDER, "New folder".to_owned()));
    out.push((NEW_FILE, "New file".to_owned()));
    out.push((REFRESH, "Refresh".to_owned()));
    out
}

/// `items` with owned labels, to add items whose labels are made at run time.
fn owned(items: Vec<(u32, &'static str)>) -> Vec<(u32, String)> {
    items.into_iter().map(|(id, title)| (id, title.to_owned())).collect()
}
```

Test modülüne:

```rust
    #[test]
    fn file_items_depend_on_the_selection() {
        assert_eq!(ids(file_items(true, false, true)), [CUT, COPY, DUPLICATE, RENAME, TRASH, DELETE_PERMANENTLY]);
        assert_eq!(ids(file_items(false, true, true)), [CUT, COPY, PASTE_INTO, DUPLICATE, TRASH, DELETE_PERMANENTLY]);
        assert!(!ids(file_items(true, true, false)).contains(&PASTE_INTO));
    }

    #[test]
    fn background_items_say_what_undo_does() {
        let items = background_items(Some("Copy 3 items"), None, true);
        assert_eq!(items[0], (UNDO, "Undo Copy 3 items".to_owned()));
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [UNDO, PASTE, NEW_FOLDER, NEW_FILE, REFRESH]);
        let bare: Vec<u32> = background_items(None, None, false).iter().map(|(id, _)| *id).collect();
        assert_eq!(bare, [NEW_FOLDER, NEW_FILE, REFRESH]);
    }
```

`crates/gezik/src/keys.rs`'e:

```rust
/// File operation shortcuts: never while typing in the address bar.
pub fn acts_on_files(action: Action) -> bool {
    matches!(
        action,
        Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::PasteMove
            | Action::Trash
            | Action::DeletePermanently
            | Action::Rename
            | Action::NewFolder
            | Action::Duplicate
            | Action::Undo
            | Action::Redo
    )
}

/// Those that act on the selection: only while the file list has the keyboard.
pub fn needs_list(action: Action) -> bool {
    matches!(
        action,
        Action::Copy | Action::Cut | Action::Trash | Action::DeletePermanently | Action::Rename | Action::Duplicate
    )
}
```

test:

```rust
    #[test]
    fn file_shortcuts_need_the_list_only_for_the_selection() {
        assert!(acts_on_files(Action::Paste) && !needs_list(Action::Paste));
        assert!(needs_list(Action::Trash) && needs_list(Action::Copy));
        assert!(!acts_on_files(Action::Refresh));
    }
```

Run: `~/.cargo/bin/cargo test -p gezik items_read_well file_items background_items file_shortcuts`
Expected: PASS.

- [ ] **Step 2: Kesilmiş adları ve gizlemeyi görünüme ekle**

`crates/gezik/src/view/model.rs`: `ViewData`'ya `pub icon_px: u32,`'dan sonra:

```rust
    /// Names in this folder on the clipboard as cut: they look faded.
    pub cut: std::collections::HashSet<String>,
```

`file_row` içinde `FileRow { … }`'a `focused` satırından sonra:

```rust
        cut: matches!(listing, Listing::Files(..)) && listing.name_at(i).is_some_and(|name| data.cut.contains(name)),
```

`crates/gezik/ui/widgets/file-view.slint`: `FileRow`'a `focused: bool,`'dan sonra:

```slint
    // On the clipboard as cut: drawn faded until it is pasted.
    cut: bool,
```

Liste satırındaki `FileIcon { … }` ile ad `Text`'ine ve ızgaradaki `FileIcon`/ad `Text`'ine `opacity: entry.file.cut ? 0.5 : 1;` (ızgarada `tile.cell.cut`) ekle.

`crates/gezik/src/view/mod.rs` `impl View` içine:

```rust
    /// The names in this folder on the clipboard as cut (they look faded).
    pub fn set_cut_names(&self, names: HashSet<String>) {
        if self.0.data.borrow().cut == names {
            return;
        }
        self.0.data.borrow_mut().cut = names;
        self.0.model.notify.reset();
    }

    /// Takes `names` out of the listing at once (trashed or deleted: the reload after the job
    /// brings back anything that stayed).
    pub fn hide_names(&self, names: &[String]) {
        let hidden: HashSet<&str> = names.iter().map(String::as_str).collect();
        let state = self.capture_all();
        let listing = std::mem::take(&mut self.0.data.borrow_mut().listing);
        let listing = match listing {
            Listing::Files(dir, entries) => {
                let kept: Vec<Entry> = entries.iter().filter(|e| !hidden.contains(e.name.as_str())).cloned().collect();
                Listing::Files(dir, Rc::new(kept))
            }
            other => other,
        };
        let selection = restore_selection(&listing, &state);
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.selection = selection;
        }
        self.0.model.notify.reset();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            window.set_list_scroll(state.scroll);
            self.keep_scroll_after_reset(state.scroll);
        }
        self.update_status();
        self.notify_listeners();
    }
```

(`gezik_core::Entry` `Clone` türetir; türetmiyorsa `crates/gezik-core/src/lib.rs`'teki `Entry`'ye `Clone` ekle.)

- [ ] **Step 3: Pano ve komutları `Operations`'a ekle**

`use` satırlarına ekle:

```rust
use std::collections::HashSet;

use gezik_ops::{CopyTask, DeleteTask, MoveTask, NewTask, TrashTask};
use gezik_platform::clipboard::{self, ClipboardError, ClipboardFiles};
```

`Inner`'a ekle (`new`'de `RefCell::default()` / `Cell::new(0)`):

```rust
    /// What Gezik put on the clipboard (the only clipboard where the system has none for files).
    clip: RefCell<Option<ClipboardFiles>>,
    /// Paths on the clipboard as cut (faded in the list), and the clipboard's change number
    /// when that was read.
    cut: RefCell<Vec<PathBuf>>,
    clip_sequence: Cell<u64>,
```

`new` içinde `on_shown` dinleyicisine kesilmiş adları da ekle (`folder_shown` çağrısından sonra):

```rust
                    let ops = Operations(inner);
                    ops.folder_shown();
                    ops.update_cut();
```

(`if let Some(inner) = weak.upgrade() { … }` gövdesini bununla değiştir.)

`impl Operations` içine:

```rust
    /// Ctrl+C / Ctrl+X on the selection.
    pub fn copy(&self, cut: bool) {
        self.copy_paths(self.0.view.selected_paths(), cut);
    }

    pub fn copy_paths(&self, paths: Vec<PathBuf>, cut: bool) {
        if paths.is_empty() {
            return;
        }
        match clipboard::write_files(&paths, cut) {
            Ok(()) | Err(ClipboardError::Unsupported) => {}
            Err(ClipboardError::Failed(why)) => return self.0.view.note(format!("Cannot use the clipboard: {why}")),
        }
        *self.0.cut.borrow_mut() = if cut { paths.clone() } else { Vec::new() };
        *self.0.clip.borrow_mut() = Some(ClipboardFiles { paths, cut });
        self.0.clip_sequence.set(clipboard::sequence());
        self.update_cut();
    }

    /// What paste would take: the system clipboard, or Gezik's own where the system has none.
    fn clipboard(&self) -> Option<ClipboardFiles> {
        match clipboard::read_files() {
            Ok(files) => files,
            Err(ClipboardError::Unsupported) => self.0.clip.borrow().clone(),
            Err(ClipboardError::Failed(_)) => None,
        }
    }

    pub fn can_paste(&self) -> bool {
        self.clipboard().is_some()
    }

    /// Ctrl+V: into `into` (a folder's menu) or the folder shown. Cut items move; `force_move`
    /// moves copied ones too (macOS ⌘⌥V).
    pub fn paste(&self, into: Option<PathBuf>, force_move: bool) {
        let Some(dir) = into.or_else(|| self.0.view.folder()) else { return };
        let Some(ClipboardFiles { paths, cut }) = self.clipboard() else { return };
        let moving = cut || force_move;
        let retry: Retry = {
            let (paths, dir) = (paths.clone(), dir.clone());
            Rc::new(move || -> Box<dyn Task> {
                if moving { Box::new(MoveTask::into(paths.clone(), &dir)) } else { Box::new(CopyTask::into(paths.clone(), &dir)) }
            })
        };
        self.submit(retry(), Some(retry), After::Select);
        if cut {
            // Pasted: cut items are no longer waiting anywhere (also Explorer's own).
            let _ = clipboard::clear();
            self.0.clip.borrow_mut().take();
            self.0.cut.borrow_mut().clear();
            self.0.clip_sequence.set(clipboard::sequence());
            self.update_cut();
        }
    }

    /// The clipboard may have changed in another program: re-read what is cut there (when
    /// the window gets the focus, and before a menu).
    pub fn clipboard_check(&self) {
        let sequence = clipboard::sequence();
        if sequence == 0 || sequence == self.0.clip_sequence.get() {
            return;
        }
        self.0.clip_sequence.set(sequence);
        let cut = match clipboard::read_files() {
            Ok(Some(files)) if files.cut => files.paths,
            _ => Vec::new(),
        };
        *self.0.cut.borrow_mut() = cut;
        self.update_cut();
    }

    /// Fades the cut items of the folder shown.
    fn update_cut(&self) {
        let names: HashSet<String> = match self.0.view.folder() {
            Some(folder) => result_names(&self.0.cut.borrow(), &folder).into_iter().collect(),
            None => HashSet::new(),
        };
        self.0.view.set_cut_names(names);
    }

    /// Delete / Shift+Delete on the selection.
    pub fn trash(&self, permanent: bool) {
        self.trash_paths(self.0.view.selected_paths(), permanent);
    }

    pub fn trash_paths(&self, paths: Vec<PathBuf>, permanent: bool) {
        if paths.is_empty() {
            return;
        }
        let what = items_text(&paths);
        let ops = self.clone();
        if permanent {
            self.0.dialogs.ask(format!("Delete {what} permanently?"), "This cannot be undone.", &["Delete", "Cancel"], move |choice| {
                if choice == Some(0) {
                    ops.delete_now(paths);
                }
            });
        } else if self.0.files.get().confirm_trash {
            let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
            self.0.dialogs.ask(format!("Move {what} to the {bin}?"), "", &["Delete", "Cancel"], move |choice| {
                if choice == Some(0) {
                    ops.trash_now(paths);
                }
            });
        } else {
            self.trash_now(paths);
        }
    }

    fn hide(&self, paths: &[PathBuf]) {
        if let Some(folder) = self.0.view.folder() {
            self.0.view.hide_names(&result_names(paths, &folder));
        }
    }

    fn trash_now(&self, paths: Vec<PathBuf>) {
        self.hide(&paths);
        let retry: Retry = {
            let paths = paths.clone();
            Rc::new(move || -> Box<dyn Task> { Box::new(TrashTask::new(paths.clone())) })
        };
        self.submit(retry(), Some(retry), After::Nothing);
    }

    fn delete_now(&self, paths: Vec<PathBuf>) {
        self.hide(&paths);
        let pending = self.0.engine.pending_deletes();
        let retry: Retry = {
            let paths = paths.clone();
            Rc::new(move || -> Box<dyn Task> { Box::new(DeleteTask::new(paths.clone(), pending.clone())) })
        };
        self.submit(retry(), Some(retry), After::Nothing);
    }

    /// A copy of each selected item next to it.
    pub fn duplicate(&self) {
        let paths = self.0.view.selected_paths();
        if paths.is_empty() {
            return;
        }
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(CopyTask::duplicate(paths.clone())) });
        self.submit(retry(), Some(retry), After::Select);
    }

    pub fn new_file(&self, dir: Option<PathBuf>) {
        if let Some(dir) = dir.or_else(|| self.0.view.folder()) {
            self.submit(Box::new(NewTask::file(&dir)), None, After::Rename);
        }
    }

    pub fn undo(&self) {
        if self.0.engine.undo().is_none() {
            self.0.view.note("Nothing to undo".to_owned());
        }
    }

    pub fn redo(&self) {
        if self.0.engine.redo().is_none() {
            self.0.view.note("Nothing to redo".to_owned());
        }
    }

    pub fn undo_label(&self) -> Option<String> {
        self.0.engine.undo_label()
    }

    pub fn redo_label(&self) -> Option<String> {
        self.0.engine.redo_label()
    }
```

Bir geri al/yinele işinin sonuçlarını seçmek için `drain`'deki `Event::Added` kolunda, bilinmeyen işler `JobView::new` ile `After::Nothing` alır; geri alma sonrası klasör yine `Changed` ile yenilenir.

- [ ] **Step 4: Kısayolları bağla**

`crates/gezik/src/main.rs` `handle_key`:

`match action {`'tan önce (Task 13'ün `Rename`/`NewFolder` için eklediği iki denetimi bununla değiştir):

```rust
            if keys::acts_on_files(action) && (editing || (keys::needs_list(action) && !window.get_list_focused())) {
                return false;
            }
```

`match action`'daki yeni eylem kollarını tamamla (geçici `=> return false` kolunu kaldır):

```rust
                Action::Copy => ops.copy(false),
                Action::Cut => ops.copy(true),
                Action::Paste => ops.paste(None, false),
                Action::PasteMove => ops.paste(None, true),
                Action::Trash => ops.trash(false),
                Action::DeletePermanently => ops.trash(true),
                Action::Rename => ops.rename_start(),
                Action::NewFolder => ops.new_folder(None),
                Action::Duplicate => ops.duplicate(),
                Action::Undo => ops.undo(),
                Action::Redo => ops.redo(),
```

Pencere odak kazandığında panoyu yeniden oku: `on_winit_window_event` kapanışına (`Resized` kolundan önce) ekle ve kapanışa `ops.clone()` ver:

```rust
            if let winit::event::WindowEvent::Focused(true) = event {
                ops.clipboard_check();
            }
```

- [ ] **Step 5: Menüleri bağla**

`crates/gezik/src/context_menu.rs`:

1. `use crate::operations::Operations;` ekle; `Menus`'e `ops: Operations` alanı; `Menus::new(window, nav, view, preview, sidebar, ops)`.
2. `Subject`'e ekle:

```rust
    /// Empty space in this folder.
    Background(PathBuf),
```

3. `open` ve `open_native` artık hazır öğe listesi alır:

```rust
    /// `at`: where the Windows menu opens (window position), else at the cursor.
    fn open(&self, subject: Subject, items: Vec<(u32, String)>, target: MenuTarget, x: f32, y: f32, at: Option<(f32, f32)>) {
        if cfg!(windows) {
            self.open_native(Some(subject), target, items, at);
        } else {
            *self.subject.borrow_mut() = Some(subject);
            self.open_slint(&items, x, y);
        }
    }
```

`open_native`'in (iki sürümünün de) `items: Vec<(u32, &'static str)>` parametresini `items: Vec<(u32, String)>` yap; Windows sürümünde `show_shell_menu` çağrısından önce:

```rust
            let items: Vec<(u32, &str)> = items.iter().map(|(id, title)| (*id, title.as_str())).collect();
```

ve sonuç eşleşmesinde `Verb` kolunu doldur:

```rust
                Ok(gezik_platform::MenuOutcome::Verb(verb)) => menus.run_verb(verb, subject),
```

4. `row_menu`'yu dosya öğeleriyle genişlet:

```rust
    fn row_menu(&self, index: i32, x: f32, y: f32, at_position: bool) {
        let Ok(i) = usize::try_from(index) else { return };
        let at = at_position.then_some((x, y));
        let native = cfg!(windows);
        self.ops.clipboard_check();
        if self.view.is_selected(i) && self.view.selection_count() > 1 {
            let paths = self.view.selected_paths();
            let mut list = owned(items(Place::Rows, native));
            list.extend(self.file_extras(false, false, native));
            return self.open(Subject::Rows(paths.clone()), list, MenuTarget::Items(paths), x, y, at);
        }
        let Some((path, is_dir)) = self.view.entry_path(i) else { return };
        let place = Place::Row { is_dir, pinned: is_dir && self.sidebar.is_pinned(&path) };
        let mut list = owned(items(place, native));
        list.extend(self.file_extras(true, is_dir, native));
        self.open(Subject::Row(path.clone()), list, MenuTarget::Item(path), x, y, at);
    }

    /// File items after the row's own: Windows already has Cut, Copy, Delete… (taken over in
    /// `run_verb`), so only Duplicate is added there.
    fn file_extras(&self, single: bool, folder: bool, native: bool) -> Vec<(u32, String)> {
        if native {
            vec![(DUPLICATE, "Duplicate".to_owned())]
        } else {
            owned(file_items(single, folder, self.ops.can_paste()))
        }
    }
```

5. Arka plan menüsünü her sistemde aç:

```rust
    /// Right-click on empty space in the file list, at window position `x`, `y`.
    pub fn background(&self, x: f32, y: f32) {
        self.background_menu(None, x, y);
    }

    fn background_menu(&self, at: Option<(f32, f32)>, x: f32, y: f32) {
        let Location::Path(dir) = self.nav.active_location() else { return };
        self.ops.clipboard_check();
        let list = background_items(self.ops.undo_label().as_deref(), self.ops.redo_label().as_deref(), self.ops.can_paste());
        self.open(Subject::Background(dir.clone()), list, MenuTarget::Background(dir), x, y, at);
    }
```

`keyboard` içindeki `self.background_menu(Some((x, y)));` çağrısını `self.background_menu(Some((x, y)), x, y);` yap. `sidebar_entry`'deki `self.open(…, place, …)` çağrısını `self.open(Subject::SidebarEntry(path.clone()), owned(items(place, cfg!(windows))), MenuTarget::Item(path), x, y, None)` yap.

6. `run`'a yeni kolları ekle (`_ => {}`'dan önce):

```rust
            (CUT | COPY, Subject::Row(path)) => self.ops.copy_paths(vec![path], id == CUT),
            (CUT | COPY, Subject::Rows(paths)) => self.ops.copy_paths(paths, id == CUT),
            (PASTE_INTO, Subject::Row(path)) => self.ops.paste(Some(path), false),
            (DUPLICATE, Subject::Row(_) | Subject::Rows(_)) => self.ops.duplicate(),
            (RENAME, Subject::Row(_)) => self.ops.rename_start(),
            (TRASH | DELETE_PERMANENTLY, Subject::Row(path)) => self.ops.trash_paths(vec![path], id == DELETE_PERMANENTLY),
            (TRASH | DELETE_PERMANENTLY, Subject::Rows(paths)) => self.ops.trash_paths(paths, id == DELETE_PERMANENTLY),
            (UNDO, Subject::Background(_)) => self.ops.undo(),
            (REDO, Subject::Background(_)) => self.ops.redo(),
            (PASTE, Subject::Background(dir)) => self.ops.paste(Some(dir), false),
            (NEW_FOLDER, Subject::Background(dir)) => self.ops.new_folder(Some(dir)),
            (NEW_FILE, Subject::Background(dir)) => self.ops.new_file(Some(dir)),
            (REFRESH, Subject::Background(_)) => self.nav.reload(),
```

7. Explorer komutlarını Gezik'e yönlendir:

```rust
    /// Explorer's own Cut, Copy, Paste, Delete and Rename, done by Gezik (its engine, its
    /// conflict list, its undo).
    #[cfg(windows)]
    fn run_verb(&self, verb: gezik_platform::ShellVerb, subject: Option<Subject>) {
        use gezik_platform::ShellVerb;
        let paths = match &subject {
            Some(Subject::Row(path) | Subject::SidebarEntry(path)) => vec![path.clone()],
            Some(Subject::Rows(paths)) => paths.clone(),
            _ => Vec::new(),
        };
        match verb {
            ShellVerb::Cut => self.ops.copy_paths(paths, true),
            ShellVerb::Copy => self.ops.copy_paths(paths, false),
            ShellVerb::Paste => {
                let into = match subject {
                    Some(Subject::Row(path) | Subject::SidebarEntry(path) | Subject::Background(path)) => Some(path),
                    _ => None,
                };
                self.ops.paste(into, false);
            }
            ShellVerb::Delete => {
                let keys = gezik_platform::modifier_keys_down();
                self.ops.trash_paths(paths, keys.left_shift || keys.right_shift);
            }
            ShellVerb::Rename => {
                if matches!(subject, Some(Subject::Row(_))) {
                    self.ops.rename_start();
                }
            }
        }
    }
```

8. Testlerde: `one_native_menu_at_a_time`, `folder_rows_offer_new_tab_and_pin_toggle` vb. değişmeden geçer (`items` aynı kaldı).

`crates/gezik/src/main.rs`:

- `Menus::new(&window, nav.clone(), view.clone(), preview.clone(), sidebar, ops.clone())`.
- `window.on_background_menu`'yu `move |x, y| { view.clear_selection(); menus.background(x, y) }` yap.

- [ ] **Step 6: Derle, test et, elle doğrula**

Run: `~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace`
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

Elle (deneme klasörlerinde):
- Ctrl+C → başka klasörde Ctrl+V: kopya gelir ve seçili olur; Explorer'a yapıştır: aynı dosya gelir. Explorer'da Ctrl+C → Gezik'te Ctrl+V.
- Ctrl+X: öğe soluk görünür; başka klasöre Ctrl+V: taşınır, soluk gösterim kalkar. Explorer'da kesilen öğe Gezik'te soluk görünür (pencereye geri dönünce).
- Del: öğe hemen listeden kalkar, çöpe gider; Ctrl+Z geri getirir; Ctrl+Y tekrar çöpe atar.
- Shift+Del: soru → Delete: öğe hemen kalkar, kalıcı silinir; Ctrl+Z bunu geri almaz (önceki işlemi alır).
- Aynı klasörde Ctrl+C, Ctrl+V: `ad (2)` oluşur.
- Windows: sağ tık → Explorer menüsündeki "Delete" Gezik'le çöpe atar (Explorer penceresi açılmaz); Shift basılı "Delete" kalıcı silmeyi sorar; "Rename" yerinde yeniden adlandırmayı açar; "Duplicate" öğesi görünür.
- Boş alana sağ tık: "Undo Delete 1 item", "Paste", "New folder", "New file", "Refresh".
- 2 GB'lık bir dosyayı kopyalarken panel 1 sn sonra görünür; duraklat/devam/iptal çalışır; özet `▾` ile daraltılır, `▴` ile açılır; görev çubuğu düğmesinde ilerleme görünür.
- Kopyalama sürerken pencereyi kapat: "1 operation is running" sorusu → "Keep open" pencereyi tutar; "Cancel them and quit" kapatır.

- [ ] **Step 7: Commit**

```bash
git add crates/gezik
git commit -m "Clipboard, cut items faded, trash and permanent delete, duplicate, undo and redo, menus and shortcuts"
```

---

### Task 15: Uygulama — çakışma listesi

**Files:**
- Create: `crates/gezik/src/conflicts.rs`
- Create: `crates/gezik/ui/widgets/conflict-list.slint`
- Modify: `crates/gezik/ui/app.slint`
- Modify: `crates/gezik/src/operations.rs` (çakışmalar listeye gider; iş bitince liste kapanır)
- Modify: `crates/gezik/src/context_menu.rs` (satır karar menüsü)
- Modify: `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: `gezik_ops::{ConflictItem, Engine, JobId}`, `gezik_core::ops::conflict::{choices, identical, source_newer, Decision, ConflictKind}`, `gezik_core::selection::Selection`, `keys::{chord_from_slint, is_primary}`
- Produces:
  - `conflicts::Conflicts` (`Clone`): `new(&AppWindow, Engine)`, `open(job: JobId, title: &str, items: Vec<ConflictItem>)`, `close_if(job: JobId)`, `decide(Decision)`, `decide_row(row: usize, Decision)`, `choices_for(row: usize) -> Vec<Decision>`, `pressed(row: usize, ctrl: bool, shift: bool)`, `key(text: &str, control, alt, shift, meta: bool) -> bool`, `start()`, `cancel()`, `set_apply_all(bool)`, `toggle_identical()`
  - saf: `conflicts::{DECISIONS, common_base(&[PathBuf]) -> PathBuf, row_name(&Path, &Path) -> String, apply(&mut [Decision], &[ConflictKind], &[usize], Decision)}`
  - `Operations::conflicts() -> &Conflicts`
  - `context_menu::CONFLICT_FIRST` (70–73), `Menus::conflict(row: i32, x: f32, y: f32)`
  - Slint: `ConflictRow`, `ConflictList`; `AppWindow` özellikleri `conflicts-open`, `conflict-title`, `conflict-rows`, `conflict-identical-count`, `conflict-hide-identical`, `conflict-apply-all`, `conflict-footer`, `conflict-scroll`; geri çağrılar `conflict-decide(int)`, `conflict-set-apply-all(bool)`, `conflict-toggle-identical()`, `conflict-pressed(int, bool, bool)`, `conflict-row-menu(int, length, length)`, `conflict-start()`, `conflict-cancel()`, `conflict-key(KeyEvent) -> bool`; `public function conflict-ensure-visible(int)`

- [ ] **Step 1: Saf yardımcıları testleriyle yaz**

`crates/gezik/src/conflicts.rs` (ilk hali):

```rust
//! The conflict list: what already exists where a copy or move goes, one decision each,
//! asked once before the job goes on.

use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{ConflictKind, Decision, choices};

/// What the buttons and the R / S / K / N keys give, in `ConflictList.decide`'s order.
pub const DECISIONS: [Decision; 4] = [Decision::Replace, Decision::Skip, Decision::KeepBoth, Decision::IfNewer];

/// The folder all `targets` are in; names are shown relative to it.
pub fn common_base(targets: &[PathBuf]) -> PathBuf {
    let mut base: Option<PathBuf> = None;
    for target in targets {
        let parent = target.parent().unwrap_or(target);
        base = Some(match base {
            None => parent.to_path_buf(),
            Some(base) => {
                let mut common = PathBuf::new();
                for (a, b) in base.components().zip(parent.components()) {
                    if a != b {
                        break;
                    }
                    common.push(a.as_os_str());
                }
                common
            }
        });
    }
    base.unwrap_or_default()
}

/// `target` relative to `base` (`fotolar/IMG_0012.jpg`).
pub fn row_name(target: &Path, base: &Path) -> String {
    target.strip_prefix(base).unwrap_or(target).display().to_string()
}

/// Sets `decision` on the conflicts at `indices` that offer it.
pub fn apply(decisions: &mut [Decision], kinds: &[ConflictKind], indices: &[usize], decision: Decision) {
    for &i in indices {
        if let (Some(slot), Some(kind)) = (decisions.get_mut(i), kinds.get(i))
            && choices(*kind).contains(&decision)
        {
            *slot = decision;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_relative_to_the_shared_folder() {
        let targets = [PathBuf::from("/d/Yedek/a.txt"), PathBuf::from("/d/Yedek/fotolar/b.jpg")];
        let base = common_base(&targets);
        assert_eq!(base, PathBuf::from("/d/Yedek"));
        assert_eq!(row_name(&targets[1], &base), PathBuf::from("fotolar").join("b.jpg").display().to_string());
        assert_eq!(common_base(&[]), PathBuf::new());
    }

    #[test]
    fn a_decision_only_lands_where_it_is_offered() {
        let kinds = [ConflictKind::File, ConflictKind::Mismatch, ConflictKind::Folder];
        let mut decisions = [Decision::Skip, Decision::Skip, Decision::Merge];
        apply(&mut decisions, &kinds, &[0, 1, 2], Decision::Replace);
        assert_eq!(decisions, [Decision::Replace, Decision::Skip, Decision::Merge]);
        apply(&mut decisions, &kinds, &[1], Decision::KeepBoth);
        assert_eq!(decisions[1], Decision::KeepBoth);
    }
}
```

`main.rs` modül listesine `mod conflicts;` ekle.

Run: `~/.cargo/bin/cargo test -p gezik conflicts::`
Expected: PASS.

- [ ] **Step 2: Slint bileşenini yaz**

`crates/gezik/ui/widgets/conflict-list.slint`:

```slint
import { ListView } from "std-widgets.slint";
import { Theme } from "../theme.slint";
import { TextButton } from "text-button.slint";

// One conflict, as Rust (conflicts.rs) builds it.
export struct ConflictRow {
    name: string,
    source-size: string,
    source-date: string,
    target-size: string,
    target-date: string,
    // 1: the source is newer, 2: the target is, 0: neither.
    newer: int,
    identical: bool,
    // 0 two files, 1 two folders (merged), 2 a file and a folder.
    kind: int,
    decision: string,
    selected: bool,
    focused: bool,
}

// Over the window: everything that already exists where the files go, with a decision each.
export component ConflictList inherits Rectangle {
    in property <bool> open;
    in property <string> title;
    in property <[ConflictRow]> rows;
    in property <int> identical-count;
    in property <bool> hide-identical;
    in property <bool> apply-all;
    in property <string> footer;
    in-out property <length> scroll;
    // 0 Replace, 1 Skip, 2 Keep both, 3 If newer (DECISIONS in conflicts.rs).
    callback decide(int);
    callback set-apply-all(bool);
    callback toggle-identical();
    callback pressed(int, bool, bool);
    callback row-menu(int, length, length);
    callback start();
    callback cancel-operation();
    callback key(KeyEvent) -> bool;

    changed open => {
        if self.open {
            scope.focus();
        }
    }

    public function ensure-visible(index: int) {
        if index * Theme.row-height < -root.scroll {
            root.scroll = -index * Theme.row-height;
        } else if (index + 1) * Theme.row-height > -root.scroll + list.visible-height {
            root.scroll = -((index + 1) * Theme.row-height - list.visible-height);
        }
    }

    background: Theme.background.with-alpha(0.6);

    // Clicks outside the box go nowhere.
    TouchArea { }

    scope := FocusScope {
        key-pressed(event) => {
            if root.key(event) {
                return accept;
            }
            return reject;
        }
    }

    Rectangle {
        x: 24px;
        y: 24px;
        width: root.width - 48px;
        height: root.height - 48px;
        background: Theme.surface;
        border-radius: Theme.radius * 1.5;
        border-width: 1px;
        border-color: Theme.border;
        accessible-role: groupbox;
        accessible-label: root.title;

        VerticalLayout {
            padding: Theme.spacing * 2;
            spacing: Theme.spacing;

            Text {
                text: root.title;
                font-weight: 600;
                overflow: elide;
                color: Theme.foreground;
            }

            HorizontalLayout {
                spacing: Theme.spacing;
                TextButton {
                    text: "Replace";
                    clicked => { root.decide(0); }
                }
                TextButton {
                    text: "Skip";
                    clicked => { root.decide(1); }
                }
                TextButton {
                    text: "Keep both";
                    clicked => { root.decide(2); }
                }
                TextButton {
                    text: "If newer";
                    clicked => { root.decide(3); }
                }
                Rectangle { horizontal-stretch: 1; }
                Text {
                    text: "Apply to:";
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
                TextButton {
                    text: "Selected";
                    checked: !root.apply-all;
                    clicked => { root.set-apply-all(false); }
                }
                TextButton {
                    text: "All";
                    checked: root.apply-all;
                    clicked => { root.set-apply-all(true); }
                }
            }

            if root.identical-count > 0: HorizontalLayout {
                alignment: start;
                TextButton {
                    text: (root.hide-identical ? "Show identical (" : "Hide identical (") + root.identical-count + ")";
                    checked: root.hide-identical;
                    clicked => { root.toggle-identical(); }
                }
            }

            HorizontalLayout {
                height: Theme.row-height;
                padding-left: Theme.spacing;
                spacing: Theme.spacing * 2;
                Text {
                    text: "Name";
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
                Text {
                    text: "Source";
                    width: 200px;
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
                Text {
                    text: "Target";
                    width: 200px;
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
                Text {
                    text: "Action";
                    width: 110px;
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
            }
            Rectangle { height: 1px; background: Theme.border; }

            list := ListView {
                vertical-stretch: 1;
                content-y <=> root.scroll;
                for row[i] in root.rows: line := Rectangle {
                    property <color> fg: row.selected ? Theme.selection-foreground : Theme.foreground;
                    property <color> muted: row.selected ? Theme.selection-foreground : Theme.foreground-muted;
                    height: Theme.row-height;
                    background: row.selected ? Theme.selection : touch.has-hover ? Theme.hover : transparent;
                    accessible-role: list-item;
                    accessible-label: row.name;
                    accessible-description: row.decision;
                    accessible-item-selectable: true;
                    accessible-item-selected: row.selected;
                    accessible-item-index: i;

                    touch := TouchArea {
                        pointer-event(event) => {
                            if event.kind == PointerEventKind.down && event.button == PointerEventButton.left {
                                scope.focus();
                                root.pressed(i, event.modifiers.control, event.modifiers.shift);
                            }
                            if event.kind == PointerEventKind.up && event.button == PointerEventButton.right {
                                root.row-menu(i, self.absolute-position.x + self.mouse-x, self.absolute-position.y + self.mouse-y);
                            }
                        }
                    }

                    HorizontalLayout {
                        padding-left: Theme.spacing;
                        spacing: Theme.spacing * 2;
                        Text {
                            text: row.name + (row.kind == 1 ? "  (folder)" : row.kind == 2 ? "  (file and folder)" : "")
                                + (row.identical ? "  · identical" : "");
                            horizontal-stretch: 1;
                            vertical-alignment: center;
                            overflow: elide;
                            color: line.fg;
                        }
                        Text {
                            text: row.source-size + "  " + row.source-date + (row.newer == 1 ? "  (newer)" : "");
                            width: 200px;
                            vertical-alignment: center;
                            overflow: elide;
                            color: line.muted;
                        }
                        Text {
                            text: row.target-size + "  " + row.target-date + (row.newer == 2 ? "  (newer)" : "");
                            width: 200px;
                            vertical-alignment: center;
                            overflow: elide;
                            color: line.muted;
                        }
                        Rectangle {
                            width: 110px;
                            border-radius: Theme.radius;
                            background: choice.has-hover && row.kind != 1 ? Theme.hover : transparent;
                            Text {
                                x: 4px;
                                height: parent.height;
                                text: row.decision;
                                vertical-alignment: center;
                                color: line.fg;
                            }
                            if row.kind != 1: Path {
                                x: parent.width - 14px;
                                y: (parent.height - 8px) / 2;
                                width: 8px;
                                height: 8px;
                                viewbox-width: 8;
                                viewbox-height: 8;
                                stroke: line.muted;
                                stroke-width: 1.5px;
                                commands: "M 1 2.5 L 4 5.5 L 7 2.5";
                            }
                            choice := TouchArea {
                                enabled: row.kind != 1;
                                clicked => { root.row-menu(i, self.absolute-position.x, self.absolute-position.y + self.height); }
                            }
                        }
                    }

                    if row.focused: Rectangle {
                        border-width: 1px;
                        border-color: Theme.focus-ring;
                    }
                }
            }

            Rectangle { height: 1px; background: Theme.border; }

            HorizontalLayout {
                spacing: Theme.spacing;
                Text {
                    text: root.footer;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                    overflow: elide;
                    color: Theme.foreground-muted;
                }
                TextButton {
                    text: "Cancel operation";
                    clicked => { root.cancel-operation(); }
                }
                TextButton {
                    text: "Start";
                    primary: true;
                    clicked => { root.start(); }
                }
            }
        }
    }
}
```

`crates/gezik/ui/app.slint`:

- İçe aktar ve dışa aç: `import { ConflictList, ConflictRow } from "widgets/conflict-list.slint";`, `export { …, OpRow, ConflictRow }`.
- Özellikler:

```slint
    // The conflict list over the window (conflicts.rs).
    in property <bool> conflicts-open;
    in property <string> conflict-title;
    in property <[ConflictRow]> conflict-rows;
    in property <int> conflict-identical-count;
    in property <bool> conflict-hide-identical;
    in property <bool> conflict-apply-all;
    in property <string> conflict-footer;
    in-out property <length> conflict-scroll;
    callback conflict-decide(int);
    callback conflict-set-apply-all(bool);
    callback conflict-toggle-identical();
    callback conflict-pressed(int, bool, bool);
    callback conflict-row-menu(int, length, length);
    callback conflict-start();
    callback conflict-cancel();
    callback conflict-key(KeyEvent) -> bool;

    public function conflict-ensure-visible(index: int) {
        conflict-list.ensure-visible(index);
    }
```

- `keys := FocusScope`'un içinde, `if root.dialog-open: Dialog { … }`'dan **önce** (iletişim kutusu listenin üstünde görünsün):

```slint
        // Always there (hidden when closed), so Rust can scroll it.
        conflict-list := ConflictList {
            x: 0;
            y: 0;
            width: root.width;
            height: root.height;
            visible: root.conflicts-open;
            open: root.conflicts-open;
            title: root.conflict-title;
            rows: root.conflict-rows;
            identical-count: root.conflict-identical-count;
            hide-identical: root.conflict-hide-identical;
            apply-all: root.conflict-apply-all;
            footer: root.conflict-footer;
            scroll <=> root.conflict-scroll;
            decide(i) => { root.conflict-decide(i); }
            set-apply-all(all) => { root.conflict-set-apply-all(all); }
            toggle-identical => { root.conflict-toggle-identical(); }
            pressed(i, ctrl, shift) => { root.conflict-pressed(i, ctrl, shift); }
            row-menu(i, x, y) => { root.conflict-row-menu(i, x, y); }
            start => { root.conflict-start(); }
            cancel-operation => { root.conflict-cancel(); }
            key(event) => { return root.conflict-key(event); }
        }
```

- [ ] **Step 3: `Conflicts`'ı yaz**

`crates/gezik/src/conflicts.rs`'e ekle (saf yardımcılardan sonra):

```rust
use std::cell::RefCell;
use std::rc::Rc;

use gezik_config::shortcuts::{Key, Platform};
use gezik_core::format_size;
use gezik_core::ops::conflict::{Facts, identical, source_newer};
use gezik_core::selection::Selection;
use gezik_ops::{ConflictItem, Engine, JobId};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::sync_model;
use crate::{AppWindow, ConflictRow};

/// Rows a PgUp / PgDn moves.
const PAGE: usize = 10;

#[derive(Default)]
struct Data {
    job: Option<JobId>,
    /// As the engine sent them (folder merges first); decisions go back in this order.
    items: Vec<ConflictItem>,
    decisions: Vec<Decision>,
    /// Indices into `items` on screen ("Hide identical" leaves some out).
    shown: Vec<usize>,
    /// Over `shown`.
    selection: Selection,
    hide_identical: bool,
    apply_all: bool,
    base: PathBuf,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    engine: Engine,
    rows: Rc<VecModel<ConflictRow>>,
    data: RefCell<Data>,
}

#[derive(Clone)]
pub struct Conflicts(Rc<Inner>);

impl Conflicts {
    pub fn new(window: &AppWindow, engine: Engine) -> Conflicts {
        let rows = Rc::new(VecModel::default());
        window.set_conflict_rows(ModelRc::from(rows.clone()));
        let conflicts =
            Conflicts(Rc::new(Inner { window: window.as_weak(), engine, rows, data: RefCell::default() }));
        let c = conflicts.clone();
        window.on_conflict_decide(move |i| {
            if let Some(decision) = usize::try_from(i).ok().and_then(|i| DECISIONS.get(i)) {
                c.decide(*decision);
            }
        });
        let c = conflicts.clone();
        window.on_conflict_set_apply_all(move |all| c.set_apply_all(all));
        let c = conflicts.clone();
        window.on_conflict_toggle_identical(move || c.toggle_identical());
        let c = conflicts.clone();
        window.on_conflict_pressed(move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                c.pressed(row, ctrl, shift);
            }
        });
        let c = conflicts.clone();
        window.on_conflict_start(move || c.start());
        let c = conflicts.clone();
        window.on_conflict_cancel(move || c.cancel());
        let c = conflicts.clone();
        window.on_conflict_key(move |event| {
            let m = event.modifiers;
            c.key(&event.text, m.control, m.alt, m.shift, m.meta)
        });
        conflicts
    }

    /// Shows the conflicts of `job` ("Copying 312 items to D:\Yedek").
    pub fn open(&self, job: JobId, title: &str, items: Vec<ConflictItem>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let real = items.iter().filter(|c| c.kind != ConflictKind::Folder).count();
        let identical_count = items.iter().filter(|c| identical(c.source_facts, c.target_facts)).count();
        let targets: Vec<PathBuf> = items.iter().map(|c| c.target.clone()).collect();
        {
            let mut data = self.0.data.borrow_mut();
            *data = Data {
                job: Some(job),
                decisions: items.iter().map(|c| c.decision).collect(),
                shown: (0..items.len()).collect(),
                selection: Selection::new(items.len()),
                base: common_base(&targets),
                items,
                ..Data::default()
            };
            if !data.shown.is_empty() {
                data.selection.set_focus(0);
            }
        }
        let what = if real == 1 { "1 conflict".to_owned() } else { format!("{real} conflicts") };
        window.set_conflict_title(format!("{what} · {title}").into());
        window.set_conflict_identical_count(i32::try_from(identical_count).unwrap_or(0));
        window.set_conflict_hide_identical(false);
        window.set_conflict_apply_all(false);
        let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
        window.set_conflict_footer(format!("Replaced files go to the {bin} when their drive has one.").into());
        window.set_conflict_scroll(0.0);
        self.refresh();
        window.set_conflicts_open(true);
    }

    /// Closes the list if it belongs to `job` (it ended some other way).
    pub fn close_if(&self, job: JobId) {
        if self.0.data.borrow().job == Some(job) {
            self.close();
        }
    }

    fn close(&self) {
        *self.0.data.borrow_mut() = Data::default();
        self.0.rows.clear();
        if let Some(window) = self.0.window.upgrade() {
            window.set_conflicts_open(false);
            window.invoke_focus_list();
        }
    }

    fn row(data: &Data, position: usize) -> Option<ConflictRow> {
        let i = *data.shown.get(position)?;
        let item = data.items.get(i)?;
        let date = |f: Facts| f.modified.map(gezik_platform::format_datetime).unwrap_or_default();
        let size = |f: Facts| if f.is_dir { String::new() } else { format_size(f.size) };
        Some(ConflictRow {
            name: row_name(&item.target, &data.base).into(),
            source_size: size(item.source_facts).into(),
            source_date: date(item.source_facts).into(),
            target_size: size(item.target_facts).into(),
            target_date: date(item.target_facts).into(),
            newer: match source_newer(item.source_facts, item.target_facts) {
                Some(true) => 1,
                Some(false) => 2,
                None => 0,
            },
            identical: identical(item.source_facts, item.target_facts),
            kind: match item.kind {
                ConflictKind::File => 0,
                ConflictKind::Folder => 1,
                ConflictKind::Mismatch => 2,
            },
            decision: data.decisions[i].label().into(),
            selected: data.selection.is_selected(position),
            focused: data.selection.focus() == Some(position),
        })
    }

    fn refresh(&self) {
        let data = self.0.data.borrow();
        let rows = (0..data.shown.len()).filter_map(|position| Self::row(&data, position));
        sync_model(&self.0.rows, rows);
    }

    /// The conflicts a button or key acts on: all shown, else the selected, else the focused.
    fn targets(data: &Data) -> Vec<usize> {
        if data.apply_all {
            return data.shown.clone();
        }
        let mut positions: Vec<usize> = data.selection.iter().collect();
        if positions.is_empty() {
            positions.extend(data.selection.focus());
        }
        positions.into_iter().filter_map(|p| data.shown.get(p).copied()).collect()
    }

    pub fn decide(&self, decision: Decision) {
        {
            let mut data = self.0.data.borrow_mut();
            let targets = Self::targets(&data);
            let kinds: Vec<ConflictKind> = data.items.iter().map(|c| c.kind).collect();
            apply(&mut data.decisions, &kinds, &targets, decision);
        }
        self.refresh();
    }

    /// From a row's own menu: the row, or all selected rows if it is one of them.
    pub fn decide_row(&self, row: usize, decision: Decision) {
        {
            let mut data = self.0.data.borrow_mut();
            let positions: Vec<usize> =
                if data.selection.is_selected(row) { data.selection.iter().collect() } else { vec![row] };
            let targets: Vec<usize> = positions.into_iter().filter_map(|p| data.shown.get(p).copied()).collect();
            let kinds: Vec<ConflictKind> = data.items.iter().map(|c| c.kind).collect();
            apply(&mut data.decisions, &kinds, &targets, decision);
        }
        self.refresh();
    }

    /// The decisions row `row` offers (none for a merged folder).
    pub fn choices_for(&self, row: usize) -> Vec<Decision> {
        let data = self.0.data.borrow();
        let Some(item) = data.shown.get(row).and_then(|&i| data.items.get(i)) else { return Vec::new() };
        choices(item.kind).iter().copied().filter(|d| DECISIONS.contains(d)).collect()
    }

    pub fn set_apply_all(&self, all: bool) {
        self.0.data.borrow_mut().apply_all = all;
        if let Some(window) = self.0.window.upgrade() {
            window.set_conflict_apply_all(all);
        }
    }

    pub fn toggle_identical(&self) {
        let hide = {
            let mut data = self.0.data.borrow_mut();
            data.hide_identical = !data.hide_identical;
            let hide = data.hide_identical;
            data.shown = (0..data.items.len())
                .filter(|&i| !(hide && identical(data.items[i].source_facts, data.items[i].target_facts)))
                .collect();
            data.selection = Selection::new(data.shown.len());
            if !data.shown.is_empty() {
                data.selection.set_focus(0);
            }
            hide
        };
        if let Some(window) = self.0.window.upgrade() {
            window.set_conflict_hide_identical(hide);
            window.set_conflict_scroll(0.0);
        }
        self.refresh();
    }

    pub fn pressed(&self, row: usize, ctrl: bool, shift: bool) {
        {
            let mut data = self.0.data.borrow_mut();
            match (ctrl, shift) {
                (_, true) => data.selection.extend_to(row, ctrl),
                (true, false) => data.selection.toggle(row),
                (false, false) => data.selection.select_only(row),
            };
        }
        self.refresh();
    }

    /// A key while the list has the keyboard (Slint's text and modifiers); returns whether it
    /// was used.
    pub fn key(&self, text: &str, control: bool, alt: bool, shift: bool, meta: bool) -> bool {
        let platform = Platform::current();
        let Some(chord) = crate::keys::chord_from_slint(text, control, alt, shift, meta, platform) else {
            return false;
        };
        let primary = crate::keys::is_primary(&chord, platform);
        let plain = !primary && !chord.alt && !chord.shift;
        match chord.key {
            Key::Enter => self.start(),
            Key::Escape => self.cancel(),
            Key::Char('a') if primary => {
                self.0.data.borrow_mut().selection.select_all();
                self.refresh();
            }
            Key::Char('r') if plain => self.decide(Decision::Replace),
            Key::Char('s') if plain => self.decide(Decision::Skip),
            Key::Char('k') if plain => self.decide(Decision::KeepBoth),
            Key::Char('n') if plain => self.decide(Decision::IfNewer),
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                let target = {
                    let mut data = self.0.data.borrow_mut();
                    let len = data.shown.len();
                    if len == 0 {
                        return true;
                    }
                    let at = data.selection.focus().unwrap_or(0);
                    let target = match chord.key {
                        Key::Up => at.saturating_sub(1),
                        Key::Down => (at + 1).min(len - 1),
                        Key::PageUp => at.saturating_sub(PAGE),
                        Key::PageDown => (at + PAGE).min(len - 1),
                        Key::Home => 0,
                        _ => len - 1,
                    };
                    if chord.shift {
                        data.selection.extend_to(target, primary);
                    } else if primary {
                        data.selection.set_focus(target);
                    } else {
                        data.selection.select_only(target);
                    }
                    target
                };
                self.refresh();
                if let Some(window) = self.0.window.upgrade() {
                    window.invoke_conflict_ensure_visible(i32::try_from(target).unwrap_or(0));
                }
            }
            _ => return false,
        }
        true
    }

    /// Start: the decisions go to the job, which goes on.
    pub fn start(&self) {
        let (job, decisions) = {
            let data = self.0.data.borrow();
            (data.job, data.decisions.clone())
        };
        self.close();
        if let Some(job) = job {
            self.0.engine.decide(job, decisions);
        }
    }

    /// Cancel operation: the job stops; what it already did stays (and can be undone).
    pub fn cancel(&self) {
        let job = self.0.data.borrow().job;
        self.close();
        if let Some(job) = job {
            self.0.engine.cancel(job);
        }
    }
}
```

- [ ] **Step 4: `Operations`'a bağla**

`crates/gezik/src/operations.rs`:

- `Inner`'a `conflicts: crate::conflicts::Conflicts,` ekle; `new`'de `let conflicts = crate::conflicts::Conflicts::new(window, engine.clone());` ve alanı kur.
- Erişim:

```rust
    pub fn conflicts(&self) -> &crate::conflicts::Conflicts {
        &self.0.conflicts
    }
```

- Task 12'deki `fn conflicts(&self, job, conflicts)` yöntemini (her çakışmaya varsayılanı veren) sil, yerine şunu ekle ve `drain`'deki çağrıyı `Event::Conflicts { job, conflicts } => self.show_conflicts(job, conflicts),` yap:

```rust
    /// A job found existing items: the list shows them, the job waits for Start.
    fn show_conflicts(&self, job: JobId, conflicts: Vec<gezik_ops::ConflictItem>) {
        let mut title = String::new();
        self.with_job(job, |j| {
            j.shown = true;
            title = j.title.clone();
        });
        if self.0.collapsed.get() {
            self.0.collapsed.set(false);
        }
        self.0.conflicts.open(job, &title, conflicts);
    }
```

- `finished`'in başına: `self.0.conflicts.close_if(id);`

- [ ] **Step 5: Satır karar menüsü ve klavye**

`crates/gezik/src/context_menu.rs`:

```rust
/// 70–73: the conflict row menu, in `conflicts::DECISIONS` order.
pub const CONFLICT_FIRST: u32 = 70;
```

`Subject`'e `Conflict(usize),` ekle. `impl Menus`'e:

```rust
    /// The decision menu of conflict row `row`, at window position `x`, `y`.
    pub fn conflict(&self, row: i32, x: f32, y: f32) {
        let Ok(row) = usize::try_from(row) else { return };
        let conflicts = self.ops.conflicts();
        let list: Vec<(u32, &'static str)> = crate::conflicts::DECISIONS
            .iter()
            .enumerate()
            .filter(|(_, d)| conflicts.choices_for(row).contains(d))
            .map(|(i, d)| (CONFLICT_FIRST + i as u32, d.label()))
            .collect();
        if list.is_empty() {
            return;
        }
        *self.subject.borrow_mut() = Some(Subject::Conflict(row));
        self.open_slint(&list, x, y);
    }
```

`run`'a:

```rust
            (id, Subject::Conflict(row)) if (CONFLICT_FIRST..CONFLICT_FIRST + 4).contains(&id) => {
                if let Some(decision) = crate::conflicts::DECISIONS.get((id - CONFLICT_FIRST) as usize) {
                    self.ops.conflicts().decide_row(row, *decision);
                }
            }
```

`crates/gezik/src/main.rs`:

- `handle_key`'in başındaki denetimi `if window.get_dialog_open() || window.get_conflicts_open() { return false; }` yap.
- `window.on_conflict_row_menu({ let menus = menus.clone(); move |row, x, y| menus.conflict(row, x, y) });` ekle (`menus` kurulduktan sonra, `on_tab_menu`'nün `menus`'ü taşımasından önce).

- [ ] **Step 6: Derle, test et, elle doğrula**

Run: `~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace`
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`

Elle: `A` klasöründe `rapor.pdf`, `fotolar/IMG.jpg` (aynı boyut ve tarihle `B`'de de), `notlar/` (B'de de var), `ozet` (B'de `ozet` bir klasör) hazırla; `A`'nın içeriğini `B`'ye kopyala:
- Liste açılır: başlık "3 conflicts · Copying …"; `notlar (folder)` satırı "Merge", seçilemez; `IMG.jpg` "· identical", Skip; "Hide identical (1)" onu gizler.
- `rapor.pdf` satırına sağ tık → menü; "Replace" → Start: yeni dosya yazılır, eskisi Çöp Kutusu'nda; Ctrl+Z eskisini geri getirir.
- `ozet` satırının menüsünde yalnızca Skip / Keep both var; Keep both → `ozet (2)` oluşur.
- Klavye: oklar, Shift+ok, R/S/K/N, Ctrl+A, Enter = Start, Esc = Cancel operation (çakışmasız kopyalananlar kalır; Ctrl+Z onları geri alır).
- Liste açıkken panel satırı "Waiting for your decisions" gösterir ve görev çubuğu sarıdır; panelden iptal edince liste kapanır.

- [ ] **Step 7: Commit**

```bash
git add crates/gezik
git commit -m "Conflict list before copying: a decision per item, hide identical, keyboard and row menu"
```

---

### Task 16: Performans, macOS/Linux derleme denetimi, belgeler, takip notu, elle test

**Files:**
- Create: `crates/gezik-ops/examples/ops_bench.rs`
- Create: `scripts/perf/ops.ps1`
- Modify: `README.md`
- Modify: `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md`

**Interfaces:**
- Consumes: her şey
- Produces: ölçüm sonuçları ve takip notu

- [ ] **Step 1: Motor ölçüm programını yaz**

`crates/gezik-ops/examples/ops_bench.rs`:

```rust
//! Times the engine on a generated tree of small files: copy, then an instant delete.
//! `cargo run -p gezik-ops --release --example ops_bench -- <work folder> [files]`

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_ops::{CopyTask, DeleteTask, Engine, Event, JobId, Settings, Task};

fn make_tree(dir: &Path, files: usize) {
    let folders = (files / 100).max(1);
    for f in 0..folders {
        let folder = dir.join(format!("folder{f:03}"));
        std::fs::create_dir_all(&folder).unwrap();
        for i in 0..files / folders {
            // 4–64 KB, different sizes.
            let size = 4096 + (i * 7919 + f * 104_729) % (60 * 1024);
            std::fs::write(folder.join(format!("file{i:04}.bin")), vec![(i % 251) as u8; size]).unwrap();
        }
    }
}

fn run(engine: &Engine, task: Box<dyn Task>) -> Duration {
    let started = Instant::now();
    let job: JobId = engine.submit(task);
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Finished { job: j, report } if j == job => {
                    assert!(report.failures.is_empty(), "{:?}", report.failures);
                    return started.elapsed();
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let work = PathBuf::from(args.next().expect("usage: ops_bench <work folder> [files]"));
    let files: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    let _ = std::fs::remove_dir_all(&work);
    let (src, dst) = (work.join("src"), work.join("dst"));
    make_tree(&src, files);
    std::fs::create_dir_all(&dst).unwrap();
    let engine = Engine::new(Settings { pending_deletes: Some(work.join("pending-deletes")), ..Settings::default() }, || {});

    let copy = run(&engine, Box::new(CopyTask::into(vec![src.clone()], &dst)));
    println!("copy {files} files: {} ms", copy.as_millis());

    let copied = dst.join("src");
    let started = Instant::now();
    let pending = engine.pending_deletes();
    let job = engine.submit(Box::new(DeleteTask::new(vec![copied.clone()], pending)));
    while copied.exists() {
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("delete: gone from the folder after {} ms", started.elapsed().as_millis());
    loop {
        let finished = engine.drain().into_iter().any(|e| matches!(e, Event::Finished { job: j, .. } if j == job));
        if finished {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("delete {files} files: {} ms in all", started.elapsed().as_millis());
    let _ = std::fs::remove_dir_all(&work);
}
```

- [ ] **Step 2: Karşılaştırma betiğini yaz**

`scripts/perf/ops.ps1`:

```powershell
# Compares Gezik's engine with Explorer on many small files (copy) and with rd /s on a delete.
# Release build; run on a quiet machine. Usage: scripts\perf\ops.ps1 [-Work <folder>] [-Files 10000]
param(
    [string]$Work = (Join-Path $env:TEMP "gezik-ops-perf"),
    [int]$Files = 10000
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root
try {
    cargo build -p gezik-ops --release --example ops_bench
    Write-Host "== Gezik engine"
    & "$root\target\release\examples\ops_bench.exe" $Work $Files

    Write-Host "== Explorer (Shell.Application CopyHere, no UI)"
    $src = Join-Path $Work "src"
    $dst = Join-Path $Work "explorer"
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $src | Out-Null
    New-Item -ItemType Directory -Force $dst | Out-Null
    $folders = [Math]::Max(1, [int]($Files / 100))
    for ($f = 0; $f -lt $folders; $f++) {
        $folder = Join-Path $src ("folder{0:D3}" -f $f)
        New-Item -ItemType Directory -Force $folder | Out-Null
        for ($i = 0; $i -lt [int]($Files / $folders); $i++) {
            $size = 4096 + (($i * 7919 + $f * 104729) % (60 * 1024))
            [IO.File]::WriteAllBytes((Join-Path $folder ("file{0:D4}.bin" -f $i)), (New-Object byte[] $size))
        }
    }
    $shell = New-Object -ComObject Shell.Application
    $watch = [Diagnostics.Stopwatch]::StartNew()
    # 4: no progress dialog, 16: yes to all, 1024: no error UI. CopyHere returns at once:
    # wait until every file is there.
    $shell.Namespace($dst).CopyHere($src, 1044)
    $target = Join-Path $dst "src"
    while (-not (Test-Path $target) -or (Get-ChildItem -Recurse -File $target).Count -lt $Files) {
        Start-Sleep -Milliseconds 50
    }
    $watch.Stop()
    Write-Host ("copy {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)

    Write-Host "== rd /s /q (delete baseline)"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    cmd /c rd /s /q "$target"
    $watch.Stop()
    Write-Host ("delete {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)
}
finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
    Pop-Location
}
```

(Explorer'ın kalıcı silmesi arayüzsüz başlatılamadığı için silmede taban çizgisi `rd /s /q`'dur; spec 11'deki "Explorer'dan ≥ 3 kat" hedefi Explorer'da Shift+Del ile elle süre tutularak da karşılaştırılır.)

- [ ] **Step 3: Ölç**

Run (Git Bash): `powershell -ExecutionPolicy Bypass -File scripts/perf/ops.ps1`
Run: `powershell -ExecutionPolicy Bypass -File scripts/perf/ops.ps1 -Files 50000` (silme hedefi için)

Expected (spec 11): Gezik kopyası Explorer'dan ≥ 2 kat kısa; silmede "gone from the folder" < 100 ms. Sonuçları takip notuna yaz (tutmayanlar dahil).

Ayrıca, önceki alt projelerin betikleriyle boşta belleği ve açılışı yeniden ölç:

Run: `powershell -ExecutionPolicy Bypass -File scripts/perf/measure.ps1 -Runs 5 -Config <geçici klasör>`
Expected: boşta ≤ 7 MB, açılış ≤ ~60 ms.

Tek 4 GB dosya (elle): `fsutil file createnew %TEMP%\big.bin 4294967296` ile dosya oluştur (seyrek değil; içini doldurmak için PowerShell'de 4 GB yaz), Gezik ve Explorer ile aynı SSD'de başka klasöre kopyala, süreleri karşılaştır (hedef ±%5).

- [ ] **Step 4: macOS/Linux derleme denetimi**

Run: `~/.cargo/bin/rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin`
Run: `~/.cargo/bin/cargo check -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`
Run: `~/.cargo/bin/cargo check -p gezik-core -p gezik-platform -p gezik-ops --target aarch64-apple-darwin`

Expected: derlenir. Hata verirse bu görevlerde yazılan Unix/macOS kodunu düzelt (ör. objc2 imzaları, libc sabitleri) ve tekrar çalıştır. `target add` ağ olmadan başarısız olursa iki denetimi atla ve takip notuna "macOS/Linux kodu derlenmedi" yaz. (`gezik` crate'i yazı tipi ve pencere bağımlılıkları yüzünden çapraz derlenmez; denetim yalnız bu üç crate içindir.)

- [ ] **Step 5: README'yi güncelle**

`README.md`'de "### Views" bölümünden sonra ekle:

```markdown
### File operations

Gezik copies, moves and deletes with its own engine on every OS:

- **Copy, cut, paste** (Ctrl+C / Ctrl+X / Ctrl+V) use the system clipboard on Windows and
  macOS: copy in Gezik and paste in Explorer or Finder, or the other way round. Cut items look
  faded until they are pasted. (Linux: Gezik's own clipboard for now.)
- **Conflicts are asked up front**: before anything is replaced, every item that already
  exists shows in one list with a decision each (Replace, Skip, Keep both, If newer).
  Nothing is replaced unless you choose so, and replaced files go to the trash.
- **Delete** moves to the Recycle Bin / Trash; **Shift+Delete** deletes for good, after a
  question. A permanent delete takes the items out of the folder at once and finishes in the
  background; if Gezik closes first, it finishes on the next start (`pending-deletes` in the
  config folder keeps the list).
- **Undo / Redo** (Ctrl+Z / Ctrl+Y, ⌘Z / ⌘⇧Z on macOS) work for copy, move, rename, new
  items, trash and replace, for the whole session. Items changed since are left alone.
- **Rename in place** with F2 (Enter on macOS); **New folder** with Ctrl+Shift+N.
- Operations on the same drive wait for each other; different drives run at the same time.
  Long ones show in a panel above the status bar (fold it into the status bar with the
  arrow), with pause, cancel and, on Windows, progress on the taskbar button.

```toml
[files]
confirm-trash = false     # ask before moving to the trash
copy-threads = "auto"     # auto (SSD 6, spinning disk 1, network 4) or 1-16
```
```

"### Keyboard shortcuts" tablosuna satırları ekle:

```markdown
| Copy / cut / paste | Ctrl+C / Ctrl+X / Ctrl+V | ⌘C / ⌘X / ⌘V (⌘⌥V moves) |
| Delete / delete permanently | Delete / Shift+Delete | ⌘⌫ / ⌘⌥⌫ |
| Rename | F2 | Enter |
| New folder | Ctrl+Shift+N | ⌘⇧N |
| Duplicate | (menu) | ⌘D |
| Undo / redo | Ctrl+Z / Ctrl+Y | ⌘Z / ⌘⇧Z |
```

ve "Settings and themes" listesine:

```markdown
- `pending-deletes` — permanent deletes not finished yet (Gezik finishes them on start).
  Local to this machine; not meant to be copied.
```

- [ ] **Step 6: Takip notunu yaz**

`docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` sonuna ekle (ölçümleri gerçek sonuçlarla doldur):

```markdown
## Dosya işlemleri 4a sonrası (2026-10-04)

- Linux'ta sistem panosu yok (Gezik içi pano); X11/Wayland altyapısıyla birlikte 4b'de.
- macOS/Linux kodu (kopyalama, çöp, pano, sürücü türü) bu makinede çalıştırılmadı; `cargo check --target` sonucu: <derlendi / derlenmedi: neden>.
- macOS ve Linux'ta `drive_facts().trash` her zaman true: çöpü olmayan bir sürücüde çöpe atma sistemden hata alır ve öğe başarısız sayılır (Windows'taki gibi "kalıcı silinsin mi?" sorusu gelmez).
- Tarama tek iş parçacığında (`read_dir`); 100 bin öğe ön tarama süresi: <ölçüm>.
- Geri alma, oluşturulmuş bir klasörü bütünüyle çöpe atar: işlemden sonra içinde düzenlenen dosyalar da gider (çöpten geri alınabilir). Yalnız tek tek dosyalar "changed since" denetlenir.
- Retry, işlemin tamamını yeniden çalıştırır (bitmiş olanlar çakışma listesinde "identical" ve Skip olarak görünür); yalnız başarısız öğeleri seçerek yeniden deneme yok.
- Farklı sürücüye taşıma otomatik testte yok (ikinci sürücü bilinmiyor); elle denendi: <sonuç>.
- Junction'lar kopyalanamıyor olabilir (CopyFileExW + COPY_FILE_COPY_SYMLINK sembolik bağlantıları kopyalar); denenmedi.
- Windows Çöp Kutusu'na büyük bir klasör atmak sistemin boyut hesaplaması yüzünden yavaş; öğe listeden hemen kalktığı için beklenmez.
- Çakışma listesi `VecModel` ile kuruluyor (tembel model değil); 10 bin satırda açılış: <ölçüm>.
- Performans (sürüm derlemesi, Windows 11, NVMe): 10 000 küçük dosya kopya Gezik <x> ms / Explorer <y> ms; 50 000 dosya silme: klasörden kalkış <x> ms, tamamı <y> ms (`rd /s /q` <z> ms); 4 GB dosya Gezik <x> s / Explorer <y> s; boşta bellek <x> MB; açılış <x> ms.
```

- [ ] **Step 7: Son kontroller ve elle test**

Run: `~/.cargo/bin/cargo build --workspace --release && ~/.cargo/bin/cargo test --workspace`
Run: `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check`
Run: `~/.cargo/bin/cargo test -p gezik-platform -- --ignored` (panonu değiştirir)

Spec 12.4'teki elle test listesini sürüm derlemesiyle (`GEZIK_CONFIG_DIR` geçici klasör) uygula; Task 13–15'in elle doğrulama maddelerine ek olarak:
- Kopyalama sürerken Görev Yöneticisi'nden Gezik'i öldür: kaynak sağlam, hedefte en çok bir yarım dosya.
- 50 000 dosyalık klasörde Shift+Del → Görev Yöneticisi'nden öldür → yeniden aç: klasör gizli kalmaz, silme devam eder ve biter.
- USB bellekte Del: "cannot go to the Recycle Bin … Delete permanently?" sorusu.
- Ağ sürücüsünden kopyalarken kabloyu çek: 20 hatadan sonra "Many items failed in a row".
- Diski dolu bir USB'ye kopyala: "The disk is full" → yer aç → Resume.
- 7-Zip gibi Shell eklentileri menüde çalışmaya devam eder.

- [ ] **Step 8: Commit**

```bash
git add crates/gezik-ops/examples scripts/perf/ops.ps1 README.md docs/superpowers/notes
git commit -m "Measure the file operations engine; document file operations; follow-up notes"
```

