# Günlük Kolaylıklar 7c (Şablonlar, Panodan Dosya, Bağlantılar, Bırakma Yığını, İşlem Günlüğü) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Klasör boşluğu menüsünde yeni dosya şablonları (yerleşik `Folder` / `Text file` / `Markdown file` ve yapılandırma klasöründeki `templates/`'ın en çok 50 öğesi; macOS/Linux `New ▸`, Windows `New from template ▸`), seçili öğeleri yeni bir klasöre taşıyan `new-folder-with-selection` (tek iş, tek geri alma; `inverse::build`'in geri alma sırası kuralı), panodaki resmi ya da metni dosya olarak yapıştırma (üç sistemde `read_image`/`read_text`), bağlantılar (Windows `.lnk`, junction, izin varsa symlink; macOS/Linux symlink; Explorer'ın `link` fiili, bağlantı bırakma tuşları, sağ sürüklemede `Create link here`, bağlantıyı çöpe atmanın hedefe dokunmadığını sınayan platform testi), pencerenin altında oturumluk bırakma yığını (en çok 1000 yol, `Copy here` / `Move here` / `Clear`, `Hit::Stack`) ve işlem panelinde `History` sekmesi (en çok 200 kayıt, `Show in folder` / `Details`).

**Architecture:** Saf kurallar `gezik-core`'da: yeni `templates.rs` (şablon listesi ve etiketleri, `Pasted image … .png` adları, `LinkKind` ve bağlantı adları) ve `drag.rs` (`Effect::Link`, platforma göre bağlantı tuşu kuralı `keys_of`, `Allowed.link`, `Hit::Stack`). `gezik-ops`'a `GroupTask`, `LinkTask`, `NewTask`'ın Markdown ve içerikli dosya türleri, `CopyTask::template`; `inverse::build` yapılan bir klasörün içine taşınanlar için sırayı değiştirir (geri alma: önce geri taşı, sonra boş klasörü çöpe; yineleme: önce klasörü çöpten getir, sonra içine taşı). `gezik-platform`'a `link.rs` (`.lnk`, junction, symlink, izin denemesi) ve panodan resim/metin okuma (Windows `PNG`/`CF_DIBV5`/`CF_DIB`/`CF_UNICODETEXT`, macOS `public.png`/TIFF/dizgi, X11/Wayland `image/png` ve metin hedefleri; hedef seçimi saf ve Windows'ta da test edilir). Arayüzde yeni `templates.rs` (şablon listesini arka planda okur), `stack.rs` + `widgets/drop-stack.slint`, `op_history.rs` (günlük kaydı, saf) + `ops-panel.slint` sekmeleri; `context_menu.rs`, `operations.rs`, `drag.rs`, `actions.rs`, `keys.rs`, `menu_bar.rs`, `app.slint` yeni öğeleri bağlar.

**Tech Stack:** Rust 2024, Slint 1.18, `windows` 0.62 (var olan özellikler: `Win32_System_DataExchange`, `Win32_System_Ole`, `Win32_System_Ioctl`, `Win32_System_Com`, `Win32_UI_Shell`), `image` 0.25 (`png` + `bmp`, zaten `gezik-platform`'da), `x11rb`, `wayland-client`, `objc2-app-kit`/`objc2-foundation` (yalnız yeni crate özellikleri: `NSBitmapImageRep`, `NSImageRep`, `NSData`, `NSDictionary`). Yeni crate yok.

**Spec:** `docs/superpowers/specs/2026-10-08-gunluk-kolayliklar-design.md` (bölüm 8, 9, 10'un 7c kısmı, 11, 12, 13'ün 7c kısmı)

**7a ve 7b'den alınanlar:** `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` "7a … sonrası" ve "7b … sonrası": menü kimlikleri `1..4096` (7a 1000-1189, 7b 1200-1321; **7c 1400-4095**), en çok dört alt menü (`MAX_SUBMENUS`, `Submenu { title, at, items }`) ve alt menüde ayraç yok, tek `settings.toml` yazıcısı, iyimser uygulama kalıbı, tek tık / sürükleme kuralları (`drag.rs` `Phase`), arka plan iş parçacığında dosya sistemi denetimi (`sidebar.rs` `check_pins`, `copy_path.rs` paylaşım önbelleği).

## Spec'ten sapmalar ve netleştirmeler

1. **Şablon listesi arka planda okunur** (spec §13 "yalnız `New ▸` açılınca"): Windows ve Linux menüleri eşzamanlıdır ve arayüz iş parçacığı dosya sistemine dokunmaz (Global Constraints). Liste açılıştan sonra bir kez `gezik-templates` iş parçacığında (ilk çizimden sonra, açılışı geciktirmez) ve yapılandırma klasörü izleyicisi `templates/` altında bir değişiklik bildirdikçe yeniden okunur; menü bellekteki listeyi kullanır (en çok 50 `Template`, birkaç yüz bayt). `templates/` yoksa liste boştur.
2. **Alt menüde ayraç yok** (spec §8.1 "yerleşiklerden sonra bir ayraçla"): Gezik'in üç menü yolunun hiçbiri alt menüde ayraç taşımaz (7a kararı, `MenuEntry`'de ayraç yok). Sıra yine ayırır: yerleşikler, kullanıcı şablonları (alfabetik), en sonda `Open templates folder`.
3. **Menü içerikleri:** macOS ve Linux'ta klasör boşluğu menüsündeki `New folder` ve `New file` kalkar, yerine `New ▸`: `Folder` (`NEW_FOLDER`), `Text file` (`NEW_FILE`), `Markdown file`, şablonlar, `Open templates folder`. Windows'ta `New folder` ve `New file` yerinde kalır; `New from template ▸`: `Markdown file`, şablonlar, `Open templates folder` (Explorer'ın kendi `New ▸`'u da menüde).
4. **`templates/`** yalnız gerçek ilk çalıştırmada (yapılandırma klasörü yokken) `ensure_initialized` ile oluşturulur; kullanıcı silerse yeniden oluşturulmaz. `Open templates folder` klasörü (yoksa) arka planda oluşturup sistemin dosya yöneticisiyle değil **Gezik'te yeni sekmede** açar (Gezik bir dosya yöneticisi; `open::that` başka bir uygulama açardı).
5. **Şablon kopyası:** `CopyTask::template(kaynak, klasör, is_dir)`: hedef adı şablonun dosya adı (uzantısıyla), karar `KeepBoth`; tür `NewFile`/`NewFolder` (geri alma etiketi "New file"/"New folder"), başlık `Creating Report.docx`.
6. **`new-folder-with-selection`** tek öğeyle de çalışır (eylem); satır menüsündeki `New folder with selection` yalnız birden çok öğe seçiliyken (spec). This PC'de bir şey yapmaz. Geri alma etiketi `New folder with 3 items` (`TaskKind::NewFolderWith`).
7. **Geri alma sırası kuralı simetrik:** spec §8.2 geri almayı yazar; yinelemede de aynı sorun tersinden vardır (geri taşımalar çöpten gelecek klasörün içine gider). İki kural: (a) bir taşınanın şimdiki yeri yapılan bir öğenin **içindeyse** (kendisi değil), geri taşımalar çöpe atmadan önce; (b) bir taşınanın gideceği yer çöpten gelecek bir öğenin içindeyse, çöpten geri getirme geri taşımalardan önce. Ayrıca (a)'da içinde taşınan bulunan yapılan klasörler yalnız **boşsa** çöpe gider (`TrashTask::only_if_empty`): sonradan değiştirilen bir öğe geri taşınmaz ("changed since; skipped") ve klasör onunla birlikte çöpe gitmez. Başka işlerin sırası değişmez (testli).
8. **Panodan dosya:** Ctrl+V önce dosyaya bakar (bugünkü gibi); dosya yoksa resim, o da yoksa metin. Okuma arayüz iş parçacığında (Linux'ta format başına en çok 1 sn bekleme, bugünkü `read_files` gibi), PNG kodlama ve yazma işte. Gezik'in kendi `write_files`'ı dosyaların yanında metin de sunar: dosya önce geldiği için etkisi yok.
9. **Pano resmi:** `ClipboardImage::{Png(Vec<u8>), Bmp(Vec<u8>)}`; Windows `CF_DIBV5`/`CF_DIB` verisinin önüne 14 baytlık BMP başlığı eklenir (saf `bmp_from_dib`, testli), `png_bytes()` `image`'ın `bmp` çözücüsü ve `png` kodlayıcısıyla (ikisi de exe'de var: `gezik-batch`). macOS TIFF'i arayüz iş parçacığında `NSBitmapImageRep` ile PNG'ye çevirir (spec böyle diyor).
10. **Junction menüde:** bütün seçim klasörse ve klasör bir sürücü harfli yoldaysa (`\\` değil) ve `nav.places()`'te ağ sürücüsü değilse görünür; NTFS/ReFS denetimi dosya sistemine dokunduğu için menüde değil **işte** yapılır (`link::junctions_supported`), değilse her öğe "Junctions need a local NTFS drive" ile düşer. `Symbolic link` yalnız açılıştaki deneme başarılıysa. Symlink `std::os::windows::fs::symlink_{file,dir}` ile yapılır (std önce `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` ile dener). Bağlantının hedefi mutlak yoldur.
11. **Bağlantı bırakma:** yalnız Gezik'e bırakılanlar bağlantı olur; Gezik'ten başka programa sürüklemeler bugünkü gibi kopya/taşımadır (dışarıya bağlantı önerilmez). Wayland'da dışarıdan gelen sürüklemede bağlantı yok: protokolde bağlantı eylemi ve tuş bilgisi yok (Gezik içi sürükleme Wayland'da da bağlantı yapar). X11 `XdndActionLink`, Windows `DROPEFFECT_LINK` (+ "Create link in %1" açıklaması), macOS `NSDragOperation::Link`.
12. **Junction'ı çöpe atmak:** plan yazılırken bu makinede (Windows 11 26100) `SHFileOperation` ile bir junction Geri Dönüşüm Kutusu'na gönderildi: hedef klasörün içi yerinde kaldı. Gezik `IFileOperation` kullanır; Task 2'nin zorunlu testi bunu sınar. **Dal A (beklenen):** `Outcome::Created` ve bugünkü geri alma. **Dal B (test düşerse):** Task 2 Step 6'daki `Outcome::Linked` ve `UnlinkTask`.
13. **Yığın şeridi** işlem panelinin altında, durum çubuğunun üstünde; ilk 100 öğe çizilir, fazlası `+N more` (hepsi işlemlere girer). Şeridin başındaki "N items" tutamacı hepsini, bir öğe kendini sürükler. Var olmama denetimi arka planda (öğe eklenince, şerit açılınca, her iş bitince). Taşınanlar iş bitince (başarısız olanlar hariç) yığından çıkar. Dışarıdan şeride bırakmaya kaynağa `Copy` yanıtı verilir (dosyaya dokunulmaz; Windows'ta "Copy to Drop stack" yazar).
14. **İşlem günlüğü** yeni `op_history.rs`'te (saf kayıt ve metinler, testli). `Show in folder` sonuçların ilk öğesinin klasöründe ilk 1000 sonucu seçer (`Navigator::go_selecting`). View menüsüne (her sistemde) `Drop stack` (işaretli) ve `Operation history` eklenir; spec yalnız macOS için ikincisini sayıyordu, Windows/Linux'ta `show-history`'nin tuşu yok, menü ulaşılabilirlik sağlar.
15. **Kimlikler (7c):** `NEW_MARKDOWN 1400`, `OPEN_TEMPLATES 1401`, `NEW_FOLDER_WITH_SELECTION 1402`, `PASTE_AS_FILE 1403`, `LINK_SHORTCUT 1404`, `LINK_JUNCTION 1405`, `LINK_SYMLINK 1406`, `CREATE_LINK_HERE 1407`, `TOGGLE_STACK 1408`, `SHOW_HISTORY 1409`, `TEMPLATE_FIRST 1410-1459` (`TEMPLATE_MAX 50`).
16. **Test ortamı (kullanıcı kararı, 2026-10-08):** Docker/Linux kabı yok; spec §12'nin `gui.sh` kipleri yerine 7c maddeleri `linux-test.md`'ye ve `macos-test.md`'ye kontrol listesi olarak yazılır, gerçek makinelerde denenir. Linux'a özgü kod (X11/Wayland hedef seçimi) saf işlevlerle Windows'ta test edilir, ayrıca çapraz derleme. Windows ekran testleri kullanıcı uzaktayken koşulur; bu plan onları bekleyen liste olarak yazar.
17. **Dal:** `feat/daily-7c`, çalışma ağacı `D:\Work\gezik-7c`, master `056d860` + bu plan.

## Global Constraints

- Rust edition 2024, stable. Komutlar `D:\Work\gezik-7c`'den (Git Bash: `cd /d/Work/gezik-7c`); `D:\Work\gezik`'e dokunulmaz, orada derlenmez. Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- **Her görevin sonunda yalnız şu dört komut:** `cargo build --workspace -j 8`, `cargo test --workspace -j 8`, `cargo clippy --workspace --all-targets -j 8 -- -D warnings`, `cargo fmt --all -- --check`. Görev içinde odaklı koşular (`cargo test -j 8 -p <crate> <süzgeç>`) serbest.
- **Çapraz denetimler yalnız son görevde:** `cargo check -j 8 -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -j 8 -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -j 8 -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`. Linux/macOS'a özgü kodu yazan görev `#[cfg]` kollarını özenle yazar; derleme hatası son görevde düzeltilir.
- **Docker / Linux kabı yok.** Linux davranışı kullanıcının gerçek Linux masaüstünde, alt projenin sonunda `linux-test.md`'deki listeyle denenir.
- **Windows ekran testleri** yalnız kullanıcı uzakta olduğunu söylediğinde; bu plan onları bekleyen liste olarak kaydeder. **Klavye düzenleri asla değiştirilmez** (ne sistemde ne Gezik penceresinde).
- Yeni crate yok; `gezik-core` bağımlılıksız kalır; `windows` crate'ine yeni özellik eklenmez (gerekenler var). macOS'ta yalnız var olan `objc2-*` crate'lerine özellik eklenir.
- Exe: master `056d860`'ın sürüm derlemesi **22.665.216 bayt** (2026-10-08, bu çalışma ağacında `cargo build --release -p gezik -j 8`, kod master'la aynı). 7c sınırı +0,25 MiB = +262.144 → **22.927.360 bayt**. Spec'in toplam ölçütü 7a+7b+7c ≤ +0,75 MiB, `38c65b3`'ten (22.240.256) → 23.026.688 bayt. **Geçerli sınır, ikisinden sıkı olanı: 22.927.360 bayt.**
- Boşta bellek master'a göre büyümez; açılış süresi değişmez (≤ ~60 ms): yığın (≤ 1000 yol) ve günlük (≤ 200 kayıt, kayıt başına ≤ 50 ayrıntı satırı ve ≤ 1000 sonuç adı) ilk kullanımda ayrılır (`Vec::new()`/`VecDeque::new()` ayırmaz); şablon listesi ve symlink denemesi açılıştan sonra arka planda.
- Arayüz iş parçacığı dosya sistemine dokunmaz (var olma denetimleri, şablon listesi, klasör oluşturma arka planda); panoyu okumak dosya sistemi değildir (bugünkü `read_files` gibi arayüzde). `settings.toml` yalnız tek yazıcıdan (7c yeni yazım eklemez).
- Dosya değiştiren her yeni iş (şablondan oluşturma, seçimle yeni klasör, panodan dosya, bağlantı, yığından kopyala/taşı) iş motorundan geçer, işlem panelinde görünür ve tek Ctrl+Z ile geri alınır.
- Her yeni eylem (`new-folder-with-selection`, `add-to-stack`, `toggle-stack`, `show-history`; `Action::ALL` 59 → 63) `[shortcuts]` ile değiştirilebilir, şablonda yorum satırı, macOS menü çubuğunda ve ulaşılabilirlik testinde.
- Gezik'in menü kimlikleri `1..4096`; 7c `1400-1459`'u kullanır.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Co-Authored-By/Claude satırı yok. Plan ve not metinleri Türkçe.
- Yol birleştirme yalnız `Path::join`; yol eşitliği `gezik_core::ops::paths::{same_path, is_within}`.

## Review Focus

1. **Seçimle yeni klasörde ad ve değişmiş öğeler:** "New folder" zaten var (ya da seçilenlerden biri "New folder"); geri almadan önce klasördeki bir dosya düzenlendi. Beklenen: klasör `New folder (2)`, seçilen "New folder" onun içine; geri almada değişen dosya ve klasör yerinde kalır, ötekiler geri gelir, hiçbir şey çöpe yanlışlıkla gitmez. → Task 1 `a_selected_item_named_new_folder_goes_inside_the_numbered_one`, `an_item_changed_since_keeps_the_folder_out_of_the_trash`.
2. **Bağlantıyı çöpe atmak hedefi değiştirmez** (junction, symlink, `.lnk`; geri alma ve yineleme). → Task 2 `trashing_a_junction_leaves_its_folder`, `trashing_a_shortcut_leaves_its_folder`, `trashing_a_symbolic_link_leaves_its_folder`.
3. **Panoda birden çok biçim:** dosya + metin (Gezik'in kendi kopyası), resim + metin (tarayıcı), yalnız metin, Gezik'in kendi metni (Linux). Beklenen: dosya > resim > metin; menü etiketi buna göre. → Task 3 `files_win_then_an_image_then_text` (X11 ve Wayland seçimi), `a_dib_becomes_a_bmp_file`.
4. **Bağlantı tuşları üç sistemde ve izin verilmeyen etki:** Windows Alt ya da Ctrl+Shift, Linux Ctrl+Shift, macOS ⌘⌥; kaynak bağlantıya izin vermezse varsayılan kural. → Task 4 `link_keys_differ_by_system`, `a_link_falls_back_when_the_source_forbids_it`.
5. **Yığında artık olmayan, yinelenen ve 1000'i aşan yollar; taşıma yarıda başarısız.** Beklenen: yinelenen eklenmez, fazlası eklenmez ve söylenir, olmayan atlanır, başarısız taşınan yığında kalır. → Task 7 `the_stack_keeps_each_path_once_and_at_most_a_thousand`, `gone_items_are_left_out_and_failed_moves_stay`.

---

### Task 1: `gezik-ops` — geri alma sırası, `GroupTask`, `NewTask`'ın Markdown ve içerikli türleri, `CopyTask::template`

**Files:**
- Modify: `crates/gezik-ops/src/inverse.rs` (sıra kuralları, testler), `crates/gezik-ops/src/task.rs` (`TaskKind::{NewFolderWith, Link}`, etiketler), `crates/gezik-ops/src/tasks/trash.rs` (`only_if_empty`), `crates/gezik-ops/src/tasks/new.rs`, `crates/gezik-ops/src/tasks/copy.rs`, `crates/gezik-ops/src/tasks/mod.rs`, `crates/gezik-ops/src/lib.rs`
- Create: `crates/gezik-ops/src/tasks/group.rs`

**Interfaces:**
- Produces (sonraki görevler kullanır):

```rust
// gezik_ops
pub enum TaskKind { …, NewFolderWith, Link }   // label: "New folder with 3 items", "Create link" / "Create 3 links"
pub struct GroupTask;
impl GroupTask { pub fn new(items: Vec<PathBuf>, dir: &Path) -> GroupTask; }
impl NewTask {
    pub fn markdown(dir: &Path) -> NewTask;   // "New document.md", boş
    pub fn with_contents(dir: &Path, name: &str,
        contents: impl Fn() -> io::Result<Vec<u8>> + Send + Sync + 'static) -> NewTask;
}
impl CopyTask { pub fn template(source: PathBuf, dir: &Path, is_dir: bool) -> CopyTask; }
```

- [ ] **Step 0: Çalışma ağacı.** `cd /d/Work/gezik-7c && git status` temiz, dal `feat/daily-7c`, `git log --oneline -2` plan commit'i ve `056d860`.

- [ ] **Step 1: Write the failing tests**

`inverse.rs` test modülüne:

```rust
    fn folder() -> Facts {
        Facts { is_dir: true, ..Facts::default() }
    }

    #[test]
    fn items_moved_into_a_new_folder_come_out_before_it_goes() {
        let outcomes = vec![
            Outcome::Created { path: "/d/New folder".into(), facts: folder(), from: None },
            Outcome::Moved { from: "/d/a.txt".into(), to: "/d/New folder/a.txt".into(), facts: file() },
            Outcome::Moved { from: "/d/b".into(), to: "/d/New folder/b".into(), facts: folder() },
        ];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Move, TaskKind::Trash], "back out first, then the empty folder");
    }

    #[test]
    fn their_redo_brings_the_folder_back_before_they_go_in() {
        let outcomes = vec![
            Outcome::Moved { from: "/d/New folder/a.txt".into(), to: "/d/a.txt".into(), facts: file() },
            Outcome::Trashed { original: "/d/New folder".into(), trashed: "/bin/1".into() },
        ];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Restore, TaskKind::Move]);
    }

    #[test]
    fn a_move_next_to_a_new_item_keeps_the_old_order() {
        // The made item is the moved item's place itself, not a folder around it.
        let outcomes = vec![
            Outcome::Created { path: "/d/x".into(), facts: file(), from: None },
            Outcome::Moved { from: "/a/x".into(), to: "/d/x".into(), facts: file() },
            Outcome::Trashed { original: "/a/x".into(), trashed: "/bin/2".into() },
        ];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Move, TaskKind::Restore]);
    }
```

`task.rs` `labels_count_items`'e:

```rust
        assert_eq!(TaskKind::NewFolderWith.label(3), "New folder with 3 items");
        assert_eq!(TaskKind::NewFolderWith.label(1), "New folder with 1 item");
        assert_eq!(TaskKind::Link.label(1), "Create link");
        assert_eq!(TaskKind::Link.label(4), "Create 4 links");
```

Yeni `tasks/group.rs`'in test modülü (dosyanın sonu, Step 3'teki kodun altında):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};

    #[test]
    fn the_items_go_into_a_new_folder_and_one_undo_brings_them_back() {
        let dir = test_dir("group");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b/c.txt"), "c");
        let engine = engine();
        let job = engine.submit(Box::new(GroupTask::new(vec![dir.join("a.txt"), dir.join("b")], &dir)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let folder = dir.join("New folder");
        assert_eq!(report.results, [folder.clone()], "the folder is selected, not what went in");
        assert_eq!(read(&folder.join("a.txt")), "a");
        assert_eq!(read(&folder.join("b/c.txt")), "c");
        assert_eq!(engine.undo_label().as_deref(), Some("New folder with 2 items"));
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty() && undo.skipped_changed == 0, "{:?}", undo.failures);
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert_eq!(read(&dir.join("b/c.txt")), "c");
        assert!(!folder.exists(), "the emptied folder went to the trash");
        let (redo, _) = finish(&engine, engine.redo().unwrap(), defaults);
        assert!(redo.failures.is_empty(), "{:?}", redo.failures);
        assert_eq!(read(&folder.join("a.txt")), "a");
        assert_eq!(read(&folder.join("b/c.txt")), "c");
        assert!(!dir.join("a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_selected_item_named_new_folder_goes_inside_the_numbered_one() {
        let dir = test_dir("group-named");
        write(&dir.join("New folder/x.txt"), "x");
        write(&dir.join("y.txt"), "y");
        let engine = engine();
        let items = vec![dir.join("New folder"), dir.join("y.txt")];
        let (report, _) = finish(&engine, engine.submit(Box::new(GroupTask::new(items, &dir))), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let folder = dir.join("New folder (2)");
        assert_eq!(report.results, [folder.clone()]);
        assert_eq!(read(&folder.join("New folder/x.txt")), "x");
        assert_eq!(read(&folder.join("y.txt")), "y");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_changed_since_keeps_the_folder_out_of_the_trash() {
        let dir = test_dir("group-changed");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let items = vec![dir.join("a.txt"), dir.join("b.txt")];
        finish(&engine, engine.submit(Box::new(GroupTask::new(items, &dir))), defaults);
        let folder = dir.join("New folder");
        std::fs::write(folder.join("b.txt"), "edited after the move").unwrap();
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(undo.skipped_changed, 2, "the edited file and the folder holding it stay");
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert_eq!(read(&folder.join("b.txt")), "edited after the move");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

`tasks/new.rs` test modülüne:

```rust
    #[test]
    fn a_markdown_file_is_new_document_and_empty() {
        let dir = test_dir("new-markdown");
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(NewTask::markdown(&dir))), defaults);
        assert_eq!(report.results, [dir.join("New document.md")]);
        assert_eq!(std::fs::metadata(dir.join("New document.md")).unwrap().len(), 0);
        assert_eq!(engine.undo_label().as_deref(), Some("New file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_with_contents_is_written_whole_and_numbered() {
        let dir = test_dir("new-contents");
        std::fs::write(dir.join("Pasted text.txt"), "taken").unwrap();
        let engine = engine();
        let task = NewTask::with_contents(&dir, "Pasted text.txt", || Ok(b"line 1\r\nline 2".to_vec()));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.results, [dir.join("Pasted text (2).txt")]);
        assert_eq!(std::fs::read(dir.join("Pasted text (2).txt")).unwrap(), b"line 1\r\nline 2");
        assert_eq!(std::fs::read(dir.join("Pasted text.txt")).unwrap(), b"taken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn contents_that_cannot_be_made_leave_no_file() {
        let dir = test_dir("new-contents-bad");
        let engine = engine();
        let task = NewTask::with_contents(&dir, "x.png", || Err(io::Error::other("not a picture")));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert_eq!(report.failures.len(), 1);
        assert!(!dir.join("x.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

`tasks/copy.rs` test modülüne:

```rust
    #[test]
    fn a_template_is_copied_under_its_own_name_and_numbered() {
        let dir = test_dir("copy-template");
        write(&dir.join("templates/Report.docx"), "r");
        write(&dir.join("templates/Project/src/main.rs"), "m");
        std::fs::create_dir(dir.join("here")).unwrap();
        let engine = engine();
        for _ in 0..2 {
            let task = CopyTask::template(dir.join("templates/Report.docx"), &dir.join("here"), false);
            assert_eq!(task.title(), "Creating Report.docx");
            finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        }
        assert_eq!(read(&dir.join("here/Report.docx")), "r");
        assert_eq!(read(&dir.join("here/Report (2).docx")), "r");
        assert_eq!(engine.undo_label().as_deref(), Some("New file"));
        let task = CopyTask::template(dir.join("templates/Project"), &dir.join("here"), true);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert_eq!(report.results, [dir.join("here/Project")]);
        assert_eq!(read(&dir.join("here/Project/src/main.rs")), "m");
        assert_eq!(engine.undo_label().as_deref(), Some("New folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-ops` → derlenmez (`GroupTask`, `TaskKind::NewFolderWith`, `NewTask::markdown`, `with_contents`, `CopyTask::template` yok).

- [ ] **Step 3: Implement.**

`task.rs` — `TaskKind`'a `Pdf`'ten sonra:

```rust
    /// New folder with selection: a new folder and the items moved into it (spec 8.2).
    NewFolderWith,
    /// Shortcuts, junctions and symbolic links (spec 9.2).
    Link,
```

`verb`'e `TaskKind::NewFolderWith => "New folder with",` ve `TaskKind::Link => "Create link",`; `label`'ın `match`'ine `TaskKind::NewFolder | TaskKind::NewFile`'dan sonra:

```rust
            TaskKind::Link if count <= 1 => verb.to_owned(),
            TaskKind::Link => format!("Create {count} links"),
```

(`NewFolderWith` genel kola düşer: "New folder with 1 item" / "… 3 items".)

`tasks/trash.rs` — `TrashTask`'a alan ve kurucu:

```rust
pub struct TrashTask {
    /// Each item, and how it must still look (undo); `None`: anything.
    items: Vec<(PathBuf, Option<Facts>)>,
    /// An undo: on a drive without a trash, an empty folder or file is simply deleted.
    undoing: bool,
    /// Folders that go only if empty: what was moved into them came out first, and one that
    /// changed since stayed in (spec 8.2); the folder then stays with it.
    only_empty: Vec<PathBuf>,
}
```

`new` ve `checked` `only_empty: Vec::new()` ile; ekle:

```rust
    /// See `only_empty`.
    pub(crate) fn only_if_empty(mut self, dirs: Vec<PathBuf>) -> TrashTask {
        self.only_empty = dirs;
        self
    }
```

`run`'da `unchanged` denetiminden hemen sonra:

```rust
        if self.only_empty.iter().any(|dir| gezik_core::ops::paths::same_path(dir, path)) && !is_empty_dir(path) {
            return Err(changed_since());
        }
```

`inverse.rs` — `use gezik_core::ops::paths::{cover, is_within, same_path};`, `use std::path::{Path, PathBuf};` ve:

```rust
/// Whether `path` is inside `dir` (not `dir` itself).
fn inside(path: &Path, dir: &Path) -> bool {
    is_within(path, dir) && !same_path(path, dir)
}
```

`build`'in `flatten` döngüsünden sonraki kısmı:

```rust
    // Items moved into a folder the job made (New folder with selection): they come out
    // before it goes to the trash, else they would go with it, and it goes only if empty.
    let holders: Vec<PathBuf> = made
        .iter()
        .filter(|(dir, _)| moved.iter().any(|(now, _, _)| inside(now, dir)))
        .map(|(dir, _)| dir.clone())
        .collect();
    let moves_first = !holders.is_empty();
    // The redo of that: the folder comes back from the trash before they go into it.
    let restore_first = moved.iter().any(|(_, was, _)| trashed.iter().any(|(_, original)| inside(was, original)));
    let made = cover(made, |(path, _)| path);
    let mut trash: Option<Arc<dyn Task>> =
        (!made.is_empty()).then(|| Arc::new(TrashTask::checked(made).only_if_empty(holders)) as Arc<dyn Task>);
    let mut restore: Option<Arc<dyn Task>> =
        (!trashed.is_empty()).then(|| Arc::new(RestoreTask::new(trashed)) as Arc<dyn Task>);
    let moved = cover(moved, |(path, _, _)| path);
    // Renames in one folder may swap names: they go back in an order that never collides.
    let (renamed, moved): (Vec<_>, Vec<_>) =
        moved.into_iter().partition(|(now, was, _)| now.parent().is_some() && now.parent() == was.parent());
    let mut tasks: Vec<Arc<dyn Task>> = Vec::new();
    if restore_first {
        tasks.extend(restore.take());
    }
    if !moves_first {
        tasks.extend(trash.take());
    }
    if !renamed.is_empty() {
        tasks.push(Arc::new(RenameTask::back(renamed)));
    }
    if !moved.is_empty() {
        tasks.push(Arc::new(MoveTask::back(moved)));
    }
    tasks.extend(trash);
    tasks.extend(restore);
    tasks
```

Modül belgesi: "…what it trashed comes back; items moved into a folder the job made come out before it goes, and come back after it (spec 8.2). Nothing here knows the task kinds." `build`'in belgesine aynı kural.

`tasks/new.rs` — tamamı:

```rust
//! A new, empty folder or file, or a file with given contents (a pasted picture or text).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

/// Makes a new file's bytes, on the job's thread (a pasted picture encoded as PNG). Called
/// again if the item is tried again (a full disk).
type Contents = Box<dyn Fn() -> io::Result<Vec<u8>> + Send + Sync>;

pub struct NewTask {
    dir: PathBuf,
    name: String,
    folder: bool,
    contents: Option<Contents>,
}

impl NewTask {
    fn new(dir: &Path, name: &str, folder: bool) -> NewTask {
        NewTask { dir: dir.to_path_buf(), name: name.to_owned(), folder, contents: None }
    }

    pub fn folder(dir: &Path) -> NewTask {
        NewTask::new(dir, "New folder", true)
    }

    pub fn file(dir: &Path) -> NewTask {
        NewTask::new(dir, "New file.txt", false)
    }

    /// An empty Markdown file (spec 8.1).
    pub fn markdown(dir: &Path) -> NewTask {
        NewTask::new(dir, "New document.md", false)
    }

    /// A file named `name` (a taken name gets a number) with what `contents` makes.
    pub fn with_contents(
        dir: &Path,
        name: &str,
        contents: impl Fn() -> io::Result<Vec<u8>> + Send + Sync + 'static,
    ) -> NewTask {
        NewTask { contents: Some(Box::new(contents)), ..NewTask::new(dir, name, false) }
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
        let item = PlanItem::new(Stage::Parallel, facts)
            .target(self.dir.join(&self.name))
            .checked()
            .top(0)
            .preset(Some(Decision::KeepBoth));
        sink.item(item);
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        if self.folder {
            std::fs::create_dir(target)?;
        } else if let Some(contents) = &self.contents {
            // Made first: a picture that cannot be read leaves no file behind.
            let bytes = contents()?;
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(target)?;
            if let Err(err) = file.write_all(&bytes) {
                drop(file);
                let _ = std::fs::remove_file(target);
                return Err(err);
            }
        } else {
            std::fs::OpenOptions::new().write(true).create_new(true).open(target)?;
        }
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, self.folder), from: None })
    }
}
```

(Var olan iki test aynen kalır.)

`tasks/copy.rs` — `CopyTask`'a alan `new: Option<TaskKind>` (belge: "A template copied in (spec 8.1): its kind (New file, New folder); `None`: a copy."); `into` ve `duplicate` `new: None` ile. Ekle:

```rust
    /// The user's template `source` copied into `dir` under its own name (a taken name gets a
    /// number), undone as a new file or folder (spec 8.1).
    pub fn template(source: PathBuf, dir: &Path, is_dir: bool) -> CopyTask {
        let target = dir.join(source.file_name().unwrap_or_default());
        let kind = if is_dir { TaskKind::NewFolder } else { TaskKind::NewFile };
        CopyTask {
            pairs: vec![(source, target)],
            presets: vec![Some(Decision::KeepBoth)],
            dir: Some(dir.to_path_buf()),
            new: Some(kind),
        }
    }
```

`kind()` → `self.new.unwrap_or(TaskKind::Copy)`; `title()`'ın başına:

```rust
        if self.new.is_some()
            && let Some((_, target)) = self.pairs.first()
        {
            return format!("Creating {}", super::name(target));
        }
```

Yeni `tasks/group.rs` (testleri Step 1'de):

```rust
//! New folder with selection (spec 8.2): a new folder next to the items and the items moved
//! into it, as one job and one undo.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_platform::fs;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::facts_of;

/// The new folder's name; a taken one becomes "New folder (2)".
const FOLDER_NAME: &str = "New folder";
const MAKE: u8 = 0;
const MOVE: u8 = 1;

pub struct GroupTask {
    dir: PathBuf,
    items: Vec<PathBuf>,
}

impl GroupTask {
    /// Moves `items` (in `dir`) into a new folder in `dir`.
    pub fn new(items: Vec<PathBuf>, dir: &Path) -> GroupTask {
        GroupTask { dir: dir.to_path_buf(), items }
    }
}

impl Task for GroupTask {
    fn kind(&self) -> TaskKind {
        TaskKind::NewFolderWith
    }

    fn title(&self) -> String {
        format!("Moving {} into a new folder", what(&self.items))
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.dir.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let folder = self.dir.join(FOLDER_NAME);
        // Made as it is planned (a Before item), before anything goes in. A taken name gets a
        // number, and the items' targets follow it (the engine's renames). Only the folder is
        // a result: it is selected and renamed afterwards.
        let make = PlanItem::new(Stage::Before, Facts { is_dir: true, ..Facts::default() })
            .target(&folder)
            .checked()
            .top(0)
            .preset(Some(Decision::KeepBoth))
            .tag(MAKE);
        if !sink.item(make) {
            return;
        }
        for item in &self.items {
            if super::refuse_root(sink, item, "move") {
                continue;
            }
            let meta = match std::fs::symlink_metadata(item) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(item, err);
                    continue;
                }
            };
            let Some(name) = item.file_name() else { continue };
            let planned =
                PlanItem::new(Stage::Parallel, facts_of(&meta)).source(item).target(folder.join(name)).under(0).tag(MOVE);
            if !sink.item(planned) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        match (item.tag, &item.source) {
            (MAKE, _) => {
                std::fs::create_dir(target)?;
                Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, true), from: None })
            }
            // Same folder, same drive: one rename each.
            (_, Some(source)) => {
                fs::move_entry(source, target)?;
                let facts = facts_after(target, item.facts.is_dir);
                Ok(Outcome::Moved { from: source.clone(), to: target.clone(), facts })
            }
            _ => Ok(Outcome::Nothing),
        }
    }
}
```

`tasks/mod.rs`: `mod group;` ve `pub use group::GroupTask;`. `lib.rs`: `pub use tasks::{CopyTask, DeleteTask, GroupTask, MoveTask, NewTask, RenameTask, RestoreTask, TrashTask, trash_path};`.

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-ops` → PASS (eski `inverse` testleri değişmeden geçer: `made_moved_and_trashed_each_get_their_inverse_in_order`, `renames_in_one_folder_undo_as_renames`, `several_outcomes_are_flattened`; ve `engine.rs`'in geri alma/yineleme testleri). Sonra dört komut: `cargo build --workspace -j 8`, `cargo test --workspace -j 8`, `cargo clippy --workspace --all-targets -j 8 -- -D warnings`, `cargo fmt --all -- --check`.

- [ ] **Step 5: Commit** "Group a selection into a new folder, make Markdown and pasted files and template copies, and undo items moved into a new folder before the folder".

---

### Task 2: Bağlantılar — `gezik-core::templates`, `gezik-platform::link`, `LinkTask`, çöpe atma platform testi

**Files:**
- Create: `crates/gezik-core/src/templates.rs`, `crates/gezik-platform/src/link.rs`, `crates/gezik-ops/src/tasks/link.rs`
- Modify: `crates/gezik-core/src/lib.rs` (`pub mod templates;`), `crates/gezik-platform/src/lib.rs` (`pub mod link;`), `crates/gezik-ops/src/tasks/mod.rs`, `crates/gezik-ops/src/lib.rs`

**Interfaces:**
- Consumes: Task 1'in `TaskKind::Link`.
- Produces:

```rust
// gezik_core::templates
pub const TEMPLATE_MAX: usize = 50;
pub struct Template { pub name: String, pub label: String, pub is_dir: bool }
pub fn template_label(name: &str, is_dir: bool) -> String;
pub fn template_list(entries: impl IntoIterator<Item = (String, bool, bool)>) -> Vec<Template>; // (name, is_dir, hidden)
pub enum PasteKind { Image, Text }
pub fn pasted_name(kind: PasteKind, at: &DateParts) -> String;
pub enum LinkKind { Shortcut, Junction, Symlink }
impl LinkKind { pub fn for_drops() -> LinkKind; }   // Windows Shortcut, elsewhere Symlink
pub fn link_name(name: &str, kind: LinkKind) -> String;
// gezik_platform::link
pub use gezik_core::templates::LinkKind;
pub fn symlinks_allowed() -> bool;
pub fn probe_symlinks();
pub fn junctions_supported(dir: &Path) -> bool;
pub fn create(kind: LinkKind, target: &Path, at: &Path, target_is_dir: bool) -> io::Result<()>;
#[cfg(windows)] pub fn read_shortcut(lnk: &Path) -> io::Result<PathBuf>;
// gezik_ops
pub struct LinkTask;
impl LinkTask {
    pub fn into(sources: Vec<PathBuf>, dir: &Path, kind: LinkKind) -> LinkTask;   // a drop, Create link here
    pub fn beside(sources: Vec<PathBuf>, kind: LinkKind) -> LinkTask;             // Create link ▸, Explorer's Create shortcut
}
```

- [ ] **Step 1: Write the failing tests**

`gezik-core/src/templates.rs`'in test modülü:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::names::numbered;

    fn parts() -> DateParts {
        DateParts { year: 2026, month: 10, day: 8, hour: 14, minute: 5, second: 9 }
    }

    #[test]
    fn templates_are_listed_by_label_without_hidden_ones() {
        let entries = [
            ("Report.docx", false, false),
            (".git", true, false),
            ("desktop.ini", false, true),
            ("Project", true, false),
            ("budget.xlsx", false, false),
            ("backup.tar.gz", false, false),
            ("v1.2", true, false),
        ];
        let list = template_list(entries.iter().map(|(n, d, h)| ((*n).to_owned(), *d, *h)));
        let labels: Vec<&str> = list.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["backup", "budget", "Project", "Report", "v1.2"], "a folder keeps its dots");
        assert_eq!(list[3].name, "Report.docx");
        assert!(list[2].is_dir);
        let many = (0..60).map(|i| (format!("t{i:02}.txt"), false, false));
        let list = template_list(many);
        assert_eq!(list.len(), TEMPLATE_MAX);
        assert_eq!(list.last().unwrap().label, "t49");
    }

    #[test]
    fn pasted_names_carry_the_local_time_without_colons() {
        assert_eq!(pasted_name(PasteKind::Image, &parts()), "Pasted image 2026-10-08 14.05.09.png");
        assert_eq!(pasted_name(PasteKind::Text, &parts()), "Pasted text 2026-10-08 14.05.09.txt");
        assert!(!pasted_name(PasteKind::Text, &parts()).contains(':'));
        // A taken name gets its number before the extension (the engine's Keep both).
        assert_eq!(
            numbered(&pasted_name(PasteKind::Image, &parts()), false, 2),
            "Pasted image 2026-10-08 14.05.09 (2).png"
        );
    }

    #[test]
    fn links_are_named_as_explorer_and_nautilus_name_them() {
        assert_eq!(link_name("rapor.pdf", LinkKind::Shortcut), "rapor.pdf - Shortcut.lnk");
        assert_eq!(link_name("Docs", LinkKind::Junction), "Link to Docs");
        assert_eq!(link_name("rapor.pdf", LinkKind::Symlink), "Link to rapor.pdf");
        assert_eq!(numbered("Link to rapor.pdf", false, 2), "Link to rapor (2).pdf");
        assert_eq!(numbered("rapor.pdf - Shortcut.lnk", false, 2), "rapor.pdf - Shortcut (2).lnk");
        let expected = if cfg!(windows) { LinkKind::Shortcut } else { LinkKind::Symlink };
        assert_eq!(LinkKind::for_drops(), expected);
    }
}
```

`gezik-platform/src/link.rs`'in test modülü:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-link-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/inside.txt"), "kept").unwrap();
        dir
    }

    #[cfg(windows)]
    #[test]
    fn a_shortcut_points_at_its_target() {
        let dir = dir("shortcut");
        let lnk = dir.join("target - Shortcut.lnk");
        create(LinkKind::Shortcut, &dir.join("target"), &lnk, true).unwrap();
        assert_eq!(read_shortcut(&lnk).unwrap(), dir.join("target"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_leads_into_its_folder_and_goes_alone() {
        let dir = dir("junction");
        assert!(junctions_supported(&dir), "the temp folder is on a local NTFS drive");
        let link = dir.join("Link to target");
        create(LinkKind::Junction, &dir.join("target"), &link, true).unwrap();
        assert_eq!(std::fs::read_to_string(link.join("inside.txt")).unwrap(), "kept");
        crate::fs::delete(&link).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("target/inside.txt")).unwrap(), "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_buffer_names_the_folder_twice() {
        let buffer = imp::mount_point_buffer(Path::new(r"C:\a")).unwrap();
        let word = |at: usize| u16::from_le_bytes([buffer[at], buffer[at + 1]]);
        assert_eq!(&buffer[..4], &0xA000_0003u32.to_le_bytes(), "IO_REPARSE_TAG_MOUNT_POINT");
        // \??\C:\a (8 units) + NUL, C:\a (4 units) + NUL, after 8 bytes of offsets.
        assert_eq!(usize::from(word(4)), 8 + (8 + 1 + 4 + 1) * 2, "ReparseDataLength");
        assert_eq!((word(8), word(10), word(12), word(14)), (0, 16, 18, 8));
        assert_eq!(buffer.len(), 8 + usize::from(word(4)));
        assert!(imp::mount_point_buffer(Path::new(r"\\server\share\x")).is_err(), "only local folders");
        let verbatim = imp::mount_point_buffer(Path::new(r"\\?\C:\a")).unwrap();
        assert_eq!(verbatim, buffer, "a \\\\?\\ path is the same folder");
    }

    #[test]
    fn a_symbolic_link_leads_to_its_target_when_allowed() {
        probe_symlinks();
        if !symlinks_allowed() {
            eprintln!("symbolic links need Developer Mode here: skipped");
            return;
        }
        let dir = dir("symlink");
        let link = dir.join("Link to target");
        create(LinkKind::Symlink, &dir.join("target"), &link, true).unwrap();
        assert_eq!(std::fs::read_to_string(link.join("inside.txt")).unwrap(), "kept");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(windows))]
    #[test]
    fn only_symbolic_links_off_windows() {
        let dir = dir("unix-kinds");
        assert!(create(LinkKind::Shortcut, &dir.join("target"), &dir.join("x.lnk"), true).is_err());
        assert!(create(LinkKind::Junction, &dir.join("target"), &dir.join("j"), true).is_err());
        assert!(!junctions_supported(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

`gezik-ops/src/tasks/link.rs`'in test modülü (spec §9.2'nin zorunlu platform testi):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};

    /// Makes a link of `kind` to a folder with a file in it and undoes it (the link goes to the
    /// trash): the folder keeps its file. Redo brings the link back from the trash.
    fn trash_keeps_the_target(kind: LinkKind, name: &str) {
        let dir = test_dir(name);
        write(&dir.join("target/inside.txt"), "kept");
        std::fs::create_dir(dir.join("links")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(LinkTask::into(vec![dir.join("target")], &dir.join("links"), kind)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{kind:?}: {:?}", report.failures);
        let link = report.results[0].clone();
        assert_eq!(engine.undo_label().as_deref(), Some("Create link"));
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty() && undo.no_trash.is_empty(), "{kind:?}: {:?}", undo.failures);
        assert!(std::fs::symlink_metadata(&link).is_err(), "{kind:?}: the link went to the trash");
        assert_eq!(read(&dir.join("target/inside.txt")), "kept", "{kind:?}: the folder's contents stay");
        let (redo, _) = finish(&engine, engine.redo().unwrap(), defaults);
        assert!(redo.failures.is_empty(), "{kind:?}: {:?}", redo.failures);
        match kind {
            #[cfg(windows)]
            LinkKind::Shortcut => {
                assert_eq!(gezik_platform::link::read_shortcut(&link).unwrap(), dir.join("target"))
            }
            _ => assert_eq!(read(&link.join("inside.txt")), "kept", "{kind:?}: back and leading there"),
        }
        // Removed as a link, never through it.
        gezik_platform::fs::delete(&link).unwrap();
        assert_eq!(read(&dir.join("target/inside.txt")), "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn trashing_a_junction_leaves_its_folder() {
        trash_keeps_the_target(LinkKind::Junction, "link-junction");
    }

    #[cfg(windows)]
    #[test]
    fn trashing_a_shortcut_leaves_its_folder() {
        trash_keeps_the_target(LinkKind::Shortcut, "link-shortcut");
    }

    #[test]
    fn trashing_a_symbolic_link_leaves_its_folder() {
        gezik_platform::link::probe_symlinks();
        if !gezik_platform::link::symlinks_allowed() {
            eprintln!("symbolic links need Developer Mode here: skipped");
            return;
        }
        trash_keeps_the_target(LinkKind::Symlink, "link-symlink");
    }

    /// A kind every test machine can make: a junction on Windows, a symbolic link elsewhere.
    fn any_kind() -> LinkKind {
        if cfg!(windows) { LinkKind::Junction } else { LinkKind::Symlink }
    }

    #[test]
    fn links_beside_their_items_are_named_and_numbered() {
        let dir = test_dir("link-beside");
        write(&dir.join("Docs/a.txt"), "a");
        let engine = engine();
        for _ in 0..2 {
            let job = engine.submit(Box::new(LinkTask::beside(vec![dir.join("Docs")], any_kind())));
            let (report, _) = finish(&engine, job, defaults);
            assert!(report.failures.is_empty(), "{:?}", report.failures);
        }
        assert_eq!(read(&dir.join("Link to Docs/a.txt")), "a");
        assert_eq!(read(&dir.join("Link to Docs (2)/a.txt")), "a");
        for name in ["Link to Docs", "Link to Docs (2)"] {
            gezik_platform::fs::delete(&dir.join(name)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_to_a_file_fails_with_a_reason() {
        let dir = test_dir("link-junction-file");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let job = engine.submit(Box::new(LinkTask::beside(vec![dir.join("a.txt")], LinkKind::Junction)));
        let (report, _) = finish(&engine, job, defaults);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.contains("only point to a folder"), "{}", report.failures[0].message);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core templates`, `cargo test -j 8 -p gezik-platform link`, `cargo test -j 8 -p gezik-ops link` → derlenmez (modüller yok).

- [ ] **Step 3: Implement `gezik-core/src/templates.rs`** (testler sonda) ve `lib.rs`'e `pub mod templates;`:

```rust
//! Names for new things: the user's templates as New ▸ lists them (spec 8.1), pasted files
//! (spec 9.1) and links (spec 9.2).

use crate::batch::date::DateParts;
use crate::ops::names::split_name;

/// The most templates New ▸ lists.
pub const TEMPLATE_MAX: usize = 50;

/// An entry of the templates folder: its file name, its menu label, whether it is a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub label: String,
    pub is_dir: bool,
}

/// The menu label of template `name`: a file without its extension (`Report.docx` →
/// `Report`, `backup.tar.gz` → `backup`), a folder as it is.
pub fn template_label(name: &str, is_dir: bool) -> String {
    split_name(name, is_dir).0.to_owned()
}

/// What New ▸ lists from the templates folder's entries, given as (name, is a folder, has
/// the hidden attribute): no names starting with a dot, no hidden ones; by label, case
/// ignored; at most `TEMPLATE_MAX`.
pub fn template_list(entries: impl IntoIterator<Item = (String, bool, bool)>) -> Vec<Template> {
    let mut list: Vec<Template> = entries
        .into_iter()
        .filter(|(name, _, hidden)| !hidden && !name.is_empty() && !name.starts_with('.'))
        .map(|(name, is_dir, _)| Template { label: template_label(&name, is_dir), name, is_dir })
        .collect();
    list.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
    list.truncate(TEMPLATE_MAX);
    list
}

/// What the clipboard holds besides files, that paste can write as a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKind {
    Image,
    Text,
}

/// `Pasted image 2026-10-08 14.05.09.png`: local time, `.` for `:` (Windows takes no `:` in
/// a name).
pub fn pasted_name(kind: PasteKind, at: &DateParts) -> String {
    let (what, ext) = match kind {
        PasteKind::Image => ("image", "png"),
        PasteKind::Text => ("text", "txt"),
    };
    format!(
        "Pasted {what} {:04}-{:02}-{:02} {:02}.{:02}.{:02}.{ext}",
        at.year, at.month, at.day, at.hour, at.minute, at.second
    )
}

/// A link Gezik makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// A Windows shortcut (`.lnk`).
    Shortcut,
    /// A Windows junction: a folder that leads to another folder.
    Junction,
    Symlink,
}

impl LinkKind {
    /// What a drop with the link keys makes: a shortcut on Windows (as Explorer), a symbolic
    /// link elsewhere.
    pub fn for_drops() -> LinkKind {
        if cfg!(windows) { LinkKind::Shortcut } else { LinkKind::Symlink }
    }
}

/// A link's name for an item named `name`: `rapor.pdf - Shortcut.lnk` (Explorer),
/// `Link to rapor.pdf` (Nautilus; the extension stays at the end).
pub fn link_name(name: &str, kind: LinkKind) -> String {
    match kind {
        LinkKind::Shortcut => format!("{name} - Shortcut.lnk"),
        LinkKind::Junction | LinkKind::Symlink => format!("Link to {name}"),
    }
}
```

- [ ] **Step 4: Implement `gezik-platform/src/link.rs`** (testler sonda) ve `lib.rs`'e `pub mod link;` (alfabetik, `pub mod http;`'den sonra):

```rust
//! Making links (spec 9.2): Windows shortcuts (`.lnk`, as Explorer makes them), junctions and
//! symbolic links; symbolic links elsewhere.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

pub use gezik_core::templates::LinkKind;

/// Whether symbolic links can be made here: 0 not known yet, 1 yes, 2 no. Windows asks for
/// Developer Mode or an administrator.
static SYMLINKS: AtomicU8 = AtomicU8::new(0);

/// Whether the menus offer symbolic links: always off Windows; on Windows once
/// `probe_symlinks` found that they can be made.
pub fn symlinks_allowed() -> bool {
    !cfg!(windows) || SYMLINKS.load(Ordering::Relaxed) == 1
}

/// Tries once whether a symbolic link can be made (Windows: one in the temp folder, removed
/// at once); off Windows nothing to try. Called once on a thread of its own at start.
pub fn probe_symlinks() {
    #[cfg(windows)]
    {
        let dir = std::env::temp_dir();
        let link = dir.join(format!("gezik-symlink-probe-{}", std::process::id()));
        let _ = std::fs::remove_file(&link);
        // std tries with SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE (Developer Mode) first.
        let made = std::os::windows::fs::symlink_file(dir.join("gezik-symlink-probe-target"), &link).is_ok();
        if made {
            let _ = std::fs::remove_file(&link);
        }
        SYMLINKS.store(if made { 1 } else { 2 }, Ordering::Relaxed);
    }
}

/// Whether a junction can be made in `dir`: Windows, a local NTFS or ReFS drive. Reads the
/// volume: not for the UI thread.
pub fn junctions_supported(dir: &Path) -> bool {
    imp::junctions_supported(dir)
}

/// Makes a link of `kind` at `at` to `target` (absolute; `target_is_dir`: a folder).
pub fn create(kind: LinkKind, target: &Path, at: &Path, target_is_dir: bool) -> io::Result<()> {
    let target = std::path::absolute(target)?;
    match kind {
        LinkKind::Symlink => symlink(&target, at, target_is_dir),
        LinkKind::Shortcut => imp::shortcut(&target, at),
        LinkKind::Junction => imp::junction(&target, at),
    }
}

#[cfg(windows)]
pub use imp::read_shortcut;

#[cfg(unix)]
fn symlink(target: &Path, at: &Path, _target_is_dir: bool) -> io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
fn symlink(target: &Path, at: &Path, target_is_dir: bool) -> io::Result<()> {
    if target_is_dir {
        std::os::windows::fs::symlink_dir(target, at)
    } else {
        std::os::windows::fs::symlink_file(target, at)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    fn only_windows() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "Only on Windows")
    }

    pub(super) fn shortcut(_target: &Path, _at: &Path) -> io::Result<()> {
        Err(only_windows())
    }

    pub(super) fn junction(_target: &Path, _at: &Path) -> io::Result<()> {
        Err(only_windows())
    }

    pub(super) fn junctions_supported(_dir: &Path) -> bool {
        false
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile, STGM_READ,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
    use windows::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
    use windows::core::{HSTRING, Interface};

    use crate::fs::{io_error, verbatim};

    /// The biggest reparse data Windows takes (MAXIMUM_REPARSE_DATA_BUFFER_SIZE).
    const MAX_REPARSE: usize = 16 * 1024;

    fn shell_link() -> io::Result<IShellLinkW> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(io_error)
        }
    }

    /// A shortcut as Explorer's "Create shortcut" makes it: the target and its folder as the
    /// working folder.
    pub(super) fn shortcut(target: &Path, at: &Path) -> io::Result<()> {
        let link = shell_link()?;
        unsafe {
            link.SetPath(&HSTRING::from(target.as_os_str())).map_err(io_error)?;
            if let Some(dir) = target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(io_error)?;
            }
            let file: IPersistFile = link.cast().map_err(io_error)?;
            file.Save(&HSTRING::from(at.as_os_str()), true).map_err(io_error)
        }
    }

    /// Where shortcut `lnk` points.
    pub fn read_shortcut(lnk: &Path) -> io::Result<PathBuf> {
        let link = shell_link()?;
        let mut buffer = vec![0u16; 32 * 1024];
        unsafe {
            let file: IPersistFile = link.cast().map_err(io_error)?;
            file.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ).map_err(io_error)?;
            link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).map_err(io_error)?;
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
    }

    /// A junction: a new empty folder made a mount point that leads to `target`.
    pub(super) fn junction(target: &Path, at: &Path) -> io::Result<()> {
        let buffer = mount_point_buffer(target)?;
        std::fs::create_dir(at)?;
        let set = set_reparse_point(at, &buffer);
        if set.is_err() {
            let _ = std::fs::remove_dir(at);
        }
        set
    }

    /// The REPARSE_DATA_BUFFER of a mount point to `target` (`C:\…`, or `\\?\C:\…`): the
    /// substitute name `\??\C:\…` and the print name `C:\…`, each ending in a NUL.
    pub(crate) fn mount_point_buffer(target: &Path) -> io::Result<Vec<u8>> {
        let mut print: Vec<u16> = target.as_os_str().encode_wide().collect();
        let verbatim_prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
        if print.starts_with(&verbatim_prefix) && print.get(5) == Some(&u16::from(b':')) {
            print.drain(..verbatim_prefix.len());
        }
        if print.get(1) != Some(&u16::from(b':')) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "A junction can only point to a local folder"));
        }
        let substitute: Vec<u16> = r"\??\".encode_utf16().chain(print.iter().copied()).collect();
        let (substitute_bytes, print_bytes) = (substitute.len() * 2, print.len() * 2);
        // The four offsets and lengths, then both names with their NULs.
        let data_length = 8 + substitute_bytes + 2 + print_bytes + 2;
        if 8 + data_length > MAX_REPARSE {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "The folder's path is too long for a junction"));
        }
        let half = |n: usize| (n as u16).to_le_bytes();
        let mut out = Vec::with_capacity(8 + data_length);
        out.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        out.extend_from_slice(&half(data_length));
        out.extend_from_slice(&half(0)); // Reserved
        out.extend_from_slice(&half(0)); // SubstituteNameOffset
        out.extend_from_slice(&half(substitute_bytes));
        out.extend_from_slice(&half(substitute_bytes + 2)); // PrintNameOffset
        out.extend_from_slice(&half(print_bytes));
        for unit in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        Ok(out)
    }

    fn set_reparse_point(at: &Path, buffer: &[u8]) -> io::Result<()> {
        let handle = unsafe {
            CreateFileW(
                &verbatim(at),
                GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(io_error)?;
        let mut returned = 0u32;
        let set = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_SET_REPARSE_POINT,
                Some(buffer.as_ptr() as *const c_void),
                buffer.len() as u32,
                None,
                0,
                Some(&mut returned),
                None,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        set.map_err(io_error)
    }

    pub(super) fn junctions_supported(dir: &Path) -> bool {
        let Some(root) = crate::fs::drive_root(dir) else { return false };
        if root.to_string_lossy().starts_with(r"\\") || crate::fs::is_network(dir).unwrap_or(true) {
            return false;
        }
        let mut name = [0u16; 64];
        let read = unsafe {
            GetVolumeInformationW(&HSTRING::from(root.as_os_str()), None, None, None, None, Some(&mut name))
        };
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        read.is_ok() && matches!(String::from_utf16_lossy(&name[..end]).to_ascii_uppercase().as_str(), "NTFS" | "REFS")
    }
}
```

(Derleyici bir imzada ayrılırsa — `GetPath`'in bayrak türü, `Save`'in `bool`'u (0.62'de `bool`), `STGM_READ`'in modülü — `windows` 0.62.2 kaynağındaki imzaya uyulur; davranış aynı kalır.)

- [ ] **Step 5: Implement `gezik-ops/src/tasks/link.rs`** (testler sonda), `tasks/mod.rs`'e `mod link;` `pub use link::LinkTask;`, `lib.rs`'in `pub use tasks::{…}`'ine `LinkTask`:

```rust
//! Links (spec 9.2): a shortcut, junction or symbolic link to each item. Undo trashes the
//! link itself (the trash takes the link, not what it leads to), redo brings it back.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_core::templates::{LinkKind, link_name};
use gezik_platform::link;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

/// The item's target is a folder.
const DIR: u8 = 1;

pub struct LinkTask {
    /// (what it leads to, where the link goes) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    kind: LinkKind,
}

/// Where a link to `source` goes in `dir`.
fn link_in(source: &Path, dir: &Path, kind: LinkKind) -> PathBuf {
    dir.join(link_name(&super::name(source), kind))
}

impl LinkTask {
    /// Links to `sources` in `dir` (a drop with the link keys, "Create link here").
    pub fn into(sources: Vec<PathBuf>, dir: &Path, kind: LinkKind) -> LinkTask {
        let pairs = sources
            .into_iter()
            .map(|source| {
                let at = link_in(&source, dir, kind);
                (source, at)
            })
            .collect();
        LinkTask { pairs, kind }
    }

    /// A link next to each of `sources` ("Create link ▸", Explorer's "Create shortcut").
    pub fn beside(sources: Vec<PathBuf>, kind: LinkKind) -> LinkTask {
        let pairs = sources
            .into_iter()
            .map(|source| {
                let dir = source.parent().map(Path::to_path_buf).unwrap_or_default();
                let at = link_in(&source, &dir, kind);
                (source, at)
            })
            .collect();
        LinkTask { pairs, kind }
    }

    fn sources(&self) -> Vec<PathBuf> {
        self.pairs.iter().map(|(source, _)| source.clone()).collect()
    }
}

impl Task for LinkTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Link
    }

    fn title(&self) -> String {
        match self.pairs.as_slice() {
            [_] => format!("Creating a link to {}", what(&self.sources())),
            _ => format!("Creating links to {}", what(&self.sources())),
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.sources();
        paths.extend(self.pairs.iter().filter_map(|(_, at)| at.parent().map(Path::to_path_buf)));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // Junctions need a local NTFS drive: asked once, for where the first link goes.
        let junctions = self.kind != LinkKind::Junction
            || self.pairs.first().and_then(|(_, at)| at.parent()).is_some_and(link::junctions_supported);
        for (root, (source, at)) in self.pairs.iter().enumerate() {
            if super::refuse_root(sink, source, "link to") {
                continue;
            }
            // What it leads to decides a folder link (a link to a link to a folder too).
            let meta = match std::fs::metadata(source).or_else(|_| std::fs::symlink_metadata(source)) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let refused = match self.kind {
                LinkKind::Junction if !meta.is_dir() => Some("A junction can only point to a folder"),
                LinkKind::Junction if !junctions => Some("Junctions need a local NTFS drive"),
                _ => None,
            };
            if let Some(why) = refused {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, why));
                continue;
            }
            // The link itself is neither a folder nor sized (as `walk` sees links); a taken
            // name gets a number without asking.
            let item = PlanItem::new(Stage::Parallel, Facts::default())
                .source(source)
                .target(at)
                .checked()
                .top(root)
                .preset(Some(Decision::KeepBoth))
                .tag(if meta.is_dir() { DIR } else { 0 });
            if !sink.item(item) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        link::create(self.kind, source, target, item.tag == DIR)?;
        // `symlink_metadata`: the link's own facts, not its target's.
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, false), from: None })
    }
}
```

- [ ] **Step 6: Run — dal kararı.** `cargo test -j 8 -p gezik-core templates`, `cargo test -j 8 -p gezik-platform link`, `cargo test -j 8 -p gezik-ops link -- --nocapture`.
  - **Dal A (beklenen):** `trashing_a_junction_leaves_its_folder` ve öbürleri geçer. Bu adımın kalanı atlanır; notlara "junction çöpe gidince hedef yerinde (IFileOperation), Dal A" yazılır (Task 9).
  - **Dal B (yalnız `trashing_a_junction_leaves_its_folder` hedefin içinin gittiğini gösterirse):** `task.rs`'in `Outcome`'una `Linked { path: PathBuf, target: PathBuf, kind: gezik_core::templates::LinkKind, dir: bool, facts: Facts }` ve `Unlinked { path, target, kind, dir }` eklenir; `result()` `Linked { path, .. }`'u da sayar. `LinkTask::run` junction için `Outcome::Linked` döner. Yeni `tasks/unlink.rs`: `UnlinkTask { items: Vec<(PathBuf, PathBuf, LinkKind, bool, Facts)> }` (`kind` `TaskKind::Link`, başlık `Removing links`), `plan`'da `symlink_metadata` ile var olanlar, `run`'da `unchanged(path, Some(facts))` değilse `changed_since()`, sonra `gezik_platform::fs::delete(path)` (`FILE_FLAG_OPEN_REPARSE_POINT` ile yalnız bağlantı) ve `Outcome::Unlinked`. Yeniden oluşturma için `LinkTask::again(items: Vec<(PathBuf /*target*/, PathBuf /*at*/, LinkKind, bool)>)` (`preset` yok: ad o anda boş olmalı, doluysa çakışma sorulur). `inverse::build`: `Linked` → `UnlinkTask`, `Unlinked` → `LinkTask::again`; ikisi de diğerlerinden önce. Test `trash_keeps_the_target` junction için "undo → bağlantı yok, hedef yerinde; redo → bağlantı yine var" olarak aynen geçer (çöp yerine kaldırma); `inverse` testine `Linked`/`Unlinked` tersleri eklenir. Notlara "Dal B" ve neden yazılır.

  Sonra dört komut: `cargo build --workspace -j 8`, `cargo test --workspace -j 8`, `cargo clippy --workspace --all-targets -j 8 -- -D warnings`, `cargo fmt --all -- --check`. (Geliştirici Modu kapalı makinede symlink testleri "skipped" yazıp geçer.)

- [ ] **Step 7: Commit** "Make shortcuts, junctions and symbolic links as undoable jobs, and check that trashing a link leaves its target".

---
