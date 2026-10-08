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

### Task 3: `gezik-platform` — panodan resim ve metin okuma (üç sistem)

**Files:**
- Modify: `crates/gezik-platform/src/clipboard.rs` (`ClipboardImage`, `paste_kind`, `read_image`, `read_text`, `bmp_from_dib`, `text_from_unicode`; Windows, macOS ve Linux `imp`'leri), `crates/gezik-platform/src/linux/mod.rs` (saf `paste_kind_of`, `text_type`, `from_latin1`; `Backend`'e üç yöntem), `crates/gezik-platform/src/linux/x11.rs`, `crates/gezik-platform/src/linux/wayland.rs`, `crates/gezik-platform/Cargo.toml` (yalnız macOS: `objc2-foundation`'a `"NSData", "NSDictionary"`, `objc2-app-kit`'e `"NSBitmapImageRep", "NSImageRep"`)

**Interfaces:**
- Consumes: Task 2'nin `gezik_core::templates::PasteKind`.
- Produces:

```rust
// gezik_platform::clipboard
pub use gezik_core::templates::PasteKind;
pub enum ClipboardImage { Png(Vec<u8>), Bmp(Vec<u8>) }
impl ClipboardImage { pub fn png_bytes(&self) -> std::io::Result<Vec<u8>>; }
/// What paste would write as a file: None while the clipboard holds files (they paste as
/// files) or nothing usable. Looks at the formats only, reads no data.
pub fn paste_kind() -> Option<PasteKind>;
pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError>;
pub fn read_text() -> Result<Option<String>, ClipboardError>;
```

- [ ] **Step 1: Write the failing tests**

`clipboard.rs` test modülü: başındaki `#[cfg(any(windows, target_os = "macos"))] use super::*;` → `use super::*;`. Ekle:

```rust
    /// A 2×1 picture as clipboard DIB data: red then green, 24 bits, bottom-up.
    fn dib() -> Vec<u8> {
        let mut out = Vec::new();
        for field in [40u32, 2, 1] {
            out.extend_from_slice(&field.to_le_bytes()); // biSize, biWidth, biHeight
        }
        out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        out.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
        for field in [0u32, 8, 0, 0, 0, 0] {
            out.extend_from_slice(&field.to_le_bytes()); // compression, size, ppm × 2, colors × 2
        }
        out.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]); // BGR BGR, padded to 4 bytes
        out
    }

    #[test]
    fn a_dib_becomes_a_bmp_file() {
        let bmp = bmp_from_dib(&dib()).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize, bmp.len());
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 40, "the pixels follow the header");
        let picture = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).unwrap().to_rgb8();
        assert_eq!((picture.width(), picture.height()), (2, 1));
        assert_eq!(picture.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(picture.get_pixel(1, 0).0, [0, 255, 0]);
        assert_eq!(bmp_from_dib(&dib()[..20]), None, "too short");
        // 8 bits with no count given: a full 256-color table sits before the pixels.
        let mut paletted = dib();
        paletted[14..16].copy_from_slice(&8u16.to_le_bytes());
        paletted.resize(40 + 256 * 4 + 4, 0);
        let bmp = bmp_from_dib(&paletted).unwrap();
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 40 + 1024);
    }

    #[test]
    fn a_picture_is_written_as_png() {
        let png = ClipboardImage::Bmp(bmp_from_dib(&dib()).unwrap()).png_bytes().unwrap();
        let back = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgb8();
        assert_eq!(back.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(ClipboardImage::Png(png.clone()).png_bytes().unwrap(), png, "PNG data as it came");
        assert!(ClipboardImage::Bmp(b"BM nonsense".to_vec()).png_bytes().is_err());
    }

    #[test]
    fn unicode_text_ends_at_its_nul() {
        let bytes: Vec<u8> = "a ş\r\nb\0junk".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(text_from_unicode(&bytes), "a ş\r\nb", "line ends kept as they are");
    }

    /// Uses the real clipboard: run by hand (`cargo test -p gezik-platform -- --ignored`).
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "replaces the user's clipboard"]
    fn text_comes_back_as_text_to_paste() {
        write_text("satır 1\nsatır 2").unwrap();
        assert_eq!(paste_kind(), Some(PasteKind::Text));
        assert_eq!(read_text().unwrap().as_deref(), Some("satır 1\nsatır 2"));
        write_files(&[std::env::temp_dir().join("a")], false).unwrap();
        assert_eq!(paste_kind(), None, "files paste as files");
        clear().unwrap();
        assert_eq!(paste_kind(), None);
    }
```

`linux/mod.rs` test modülüne (her sistemde koşar):

```rust
    #[test]
    fn files_win_then_an_image_then_text() {
        use gezik_core::templates::PasteKind;
        // Gezik's own files also offer text; a browser's picture often comes with text too.
        assert_eq!(paste_kind_of(&["TARGETS", "text/uri-list", "UTF8_STRING"]), None);
        assert_eq!(paste_kind_of(&["x-special/gnome-copied-files", "image/png"]), None);
        assert_eq!(paste_kind_of(&["text/html", "image/png", "UTF8_STRING"]), Some(PasteKind::Image));
        assert_eq!(paste_kind_of(&["TEXT", "STRING"]), Some(PasteKind::Text));
        assert_eq!(paste_kind_of(&["text/html"]), None, "nothing paste can write");
        assert_eq!(text_type(&["STRING", "text/plain", "UTF8_STRING"]), Some("UTF8_STRING"));
        assert_eq!(text_type(&["STRING", "text/plain"]), Some("text/plain"));
        assert_eq!(text_type(&["image/png"]), None);
        assert_eq!(from_latin1(&[0xE7, b'a']), "ça");
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-platform clipboard` ve `… linux` → derlenmez.

- [ ] **Step 3: Implement the shared parts** (`clipboard.rs`, `imp`'lerin dışında; modül belgesine "…and pictures and text read to paste as files (spec 9.1)" eklenir):

```rust
pub use gezik_core::templates::PasteKind;
pub use imp::{paste_kind, read_image, read_text};

/// A picture on the clipboard, as it was read (the UI thread only reads; encoding happens in
/// the job that writes the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardImage {
    /// A PNG file's bytes (Windows' "PNG" format, macOS, `image/png` on Linux).
    Png(Vec<u8>),
    /// A BMP file's bytes, made from Windows' `CF_DIBV5` / `CF_DIB`.
    Bmp(Vec<u8>),
}

impl ClipboardImage {
    /// The picture as a PNG file's bytes.
    pub fn png_bytes(&self) -> std::io::Result<Vec<u8>> {
        match self {
            ClipboardImage::Png(bytes) => Ok(bytes.clone()),
            ClipboardImage::Bmp(bytes) => {
                let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::Bmp).map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("The picture cannot be read: {err}"))
                })?;
                let mut out = Vec::new();
                picture
                    .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                    .map_err(std::io::Error::other)?;
                Ok(out)
            }
        }
    }
}

/// A BMP file from clipboard DIB data (a BITMAPINFOHEADER or a later one, maybe color masks
/// and a color table, then the pixels): the 14-byte file header goes in front, saying where
/// the pixels start. None for data too short to be a DIB.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |at: usize| dib.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = usize::try_from(u32_at(0)?).ok()?;
    if dib.len() < 40 || !(40..=dib.len()).contains(&header) {
        return None;
    }
    let bit_count = u16::from_le_bytes([dib[14], dib[15]]);
    let compression = u32_at(16)?;
    let colors_used = usize::try_from(u32_at(32)?).ok()?;
    // BI_BITFIELDS (3) and BI_ALPHABITFIELDS (6) after a 40-byte header: the masks follow it
    // (later headers hold them inside).
    let masks = match (header, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let colors = if colors_used > 0 {
        colors_used
    } else if bit_count <= 8 {
        1usize << bit_count
    } else {
        0
    };
    let offset = 14 + header + masks + colors * 4;
    if offset > 14 + dib.len() {
        return None;
    }
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&u32::try_from(14 + dib.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(offset).ok()?.to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// `CF_UNICODETEXT` data as text: UTF-16 up to its NUL, line ends as they are.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn text_from_unicode(bytes: &[u8]) -> String {
    let units: Vec<u16> =
        bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&unit| unit != 0).collect();
    String::from_utf16_lossy(&units)
}
```

- [ ] **Step 4: Windows `imp`'ine** (`use`'lara `IsClipboardFormatAvailable`, `CF_DIB`, `CF_DIBV5`; `use super::{ClipboardImage, PasteKind};`):

```rust
    fn png_format() -> u32 {
        unsafe { RegisterClipboardFormatW(windows::core::w!("PNG")) }
    }

    fn available(format: u32) -> bool {
        unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    }

    /// The bytes of clipboard memory `memory` (while the clipboard is open).
    fn global_bytes(memory: HGLOBAL) -> Option<Vec<u8>> {
        unsafe {
            let size = GlobalSize(memory);
            let source = GlobalLock(memory) as *const u8;
            if source.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(source, size).to_vec();
            let _ = GlobalUnlock(memory);
            (!bytes.is_empty()).then_some(bytes)
        }
    }

    fn data(format: u32) -> Option<Vec<u8>> {
        let handle = unsafe { GetClipboardData(format) }.ok()?;
        global_bytes(HGLOBAL(handle.0))
    }

    /// Formats only: the clipboard is not opened, no data is read.
    pub fn paste_kind() -> Option<PasteKind> {
        if available(u32::from(CF_HDROP.0)) {
            return None;
        }
        if available(png_format()) || available(u32::from(CF_DIBV5.0)) || available(u32::from(CF_DIB.0)) {
            return Some(PasteKind::Image);
        }
        available(u32::from(CF_UNICODETEXT.0)).then_some(PasteKind::Text)
    }

    /// PNG as it is, else the DIB (V5 first: it keeps the alpha) as a BMP file.
    pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
        let _open = Open::new()?;
        if let Some(png) = data(png_format()) {
            return Ok(Some(ClipboardImage::Png(png)));
        }
        for format in [CF_DIBV5, CF_DIB] {
            if let Some(bmp) = data(u32::from(format.0)).and_then(|dib| super::bmp_from_dib(&dib)) {
                return Ok(Some(ClipboardImage::Bmp(bmp)));
            }
        }
        Ok(None)
    }

    pub fn read_text() -> Result<Option<String>, ClipboardError> {
        let _open = Open::new()?;
        Ok(data(u32::from(CF_UNICODETEXT.0)).map(|bytes| super::text_from_unicode(&bytes)))
    }
```

- [ ] **Step 5: macOS `imp`'ine** (`Cargo.toml` özellikleri yukarıda):

```rust
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardTypeTIFF,
    };
    use objc2_foundation::NSDictionary;

    use super::{ClipboardImage, PasteKind};

    /// The pasteboard's types: files first, then a picture, then text.
    pub fn paste_kind() -> Option<PasteKind> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let types = pasteboard.types()?;
        let has = |wanted: &NSString| types.iter().any(|t| &*t == wanted);
        if has(unsafe { NSPasteboardTypeFileURL }) {
            None
        } else if has(&NSString::from_str("public.png")) || has(unsafe { NSPasteboardTypeTIFF }) {
            Some(PasteKind::Image)
        } else {
            has(unsafe { NSPasteboardTypeString }).then_some(PasteKind::Text)
        }
    }

    /// `public.png` as it is; else the TIFF every picture copy carries, made PNG (spec 9.1).
    pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        if let Some(png) = pasteboard.dataForType(&NSString::from_str("public.png")) {
            return Ok(Some(ClipboardImage::Png(png.to_vec())));
        }
        let Some(tiff) = pasteboard.dataForType(unsafe { NSPasteboardTypeTIFF }) else { return Ok(None) };
        let Some(rep) = NSBitmapImageRep::imageRepWithData(&tiff) else { return Ok(None) };
        let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) };
        Ok(png.map(|data| ClipboardImage::Png(data.to_vec())))
    }

    pub fn read_text() -> Result<Option<String>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        Ok(pasteboard.stringForType(unsafe { NSPasteboardTypeString }).map(|text| text.to_string()))
    }
```

(Adlar son görevin `aarch64-apple-darwin` denetiminde doğrulanır; `objc2-app-kit` 0.3'teki imzaya uyulur — `unsafe` sınıf yöntemi, `types()`'ın dönüşü — davranış aynı kalır.)

- [ ] **Step 6: Linux.** `linux/mod.rs`'e (her sistemde derlenen saf kısım, `what_to_ask`'ın yanına):

```rust
/// The text types Gezik reads, best first.
pub(crate) const TEXT_TYPES: [&str; 5] = ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain", "TEXT", "STRING"];

/// What paste would write as a file, from the types (X11 targets, Wayland MIME types) the
/// clipboard's owner offers: None while it offers files (they paste as files), else a PNG
/// picture before text (a browser's picture often comes with text).
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn paste_kind_of(offered: &[&str]) -> Option<gezik_core::templates::PasteKind> {
    use gezik_core::templates::PasteKind;
    let has = |name: &str| offered.contains(&name);
    if has("x-special/gnome-copied-files") || has("text/uri-list") {
        None
    } else if has("image/png") {
        Some(PasteKind::Image)
    } else {
        text_type(offered).map(|_| PasteKind::Text)
    }
}

/// The best text type among `offered`.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn text_type<'a>(offered: &[&'a str]) -> Option<&'a str> {
    TEXT_TYPES.iter().find_map(|wanted| offered.iter().find(|o| **o == *wanted).copied())
}

/// `STRING` (Latin-1) bytes as text.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn from_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}
```

`Backend` trait'ine (`use crate::clipboard::{ClipboardError, ClipboardFiles, ClipboardImage, PasteKind};`):

```rust
        /// What paste would write as a file (formats only).
        fn paste_kind(&self) -> Option<PasteKind>;
        fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError>;
        fn read_text(&self) -> Result<Option<String>, ClipboardError>;
```

`clipboard.rs`'in Linux `imp`'ine (`use super::{ClipboardImage, PasteKind};`):

```rust
    pub fn paste_kind() -> Option<PasteKind> {
        backend()?.paste_kind()
    }

    pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.read_image()
    }

    pub fn read_text() -> Result<Option<String>, ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.read_text()
    }
```

**X11** (`x11.rs`): atom listesine `IMAGE_PNG: b"image/png",` ve `TEXT_PLAIN_ANY: b"text/plain",`; bir resim INCR ile parça parça gelir ve 1 sn yetmeyebilir: `const IMAGE_TIMEOUT: Duration = Duration::from_secs(5);`. `transfer` ikiye ayrılır (var olan çağrılar değişmez):

```rust
    fn transfer(&self, selection: Atom, target: Atom, time: u32) -> Option<Vec<u8>> {
        self.transfer_within(selection, target, time, TRANSFER_TIMEOUT)
    }

    /// `transfer` with its own time limit (a picture).
    fn transfer_within(&self, selection: Atom, target: Atom, time: u32, timeout: Duration) -> Option<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        *self.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx);
        let result = self.transfer_with(&rx, selection, target, time, timeout);
        *self.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        result
    }
```

`transfer_with` sonuna `timeout: Duration` alır ve `TRANSFER_TIMEOUT` yerine onu kullanır. `impl X11`'e:

```rust
    /// The clipboard owner's targets by name, those Gezik knows (the rest do not matter here).
    fn offered_names(&self) -> Vec<&'static str> {
        let s = &self.0;
        let a = &s.atoms;
        let Some(targets) = s.transfer(a.CLIPBOARD, a.TARGETS, CURRENT_TIME) else { return Vec::new() };
        let offered: Vec<Atom> = targets.as_chunks::<4>().0.iter().map(|c| u32::from_ne_bytes(*c)).collect();
        let known: [(Atom, &'static str); 8] = [
            (a.URI_LIST, "text/uri-list"),
            (a.GNOME_FILES, "x-special/gnome-copied-files"),
            (a.IMAGE_PNG, "image/png"),
            (a.UTF8_STRING, "UTF8_STRING"),
            (a.TEXT_PLAIN, "text/plain;charset=utf-8"),
            (a.TEXT_PLAIN_ANY, "text/plain"),
            (a.TEXT, "TEXT"),
            (AtomEnum::STRING.into(), "STRING"),
        ];
        known.iter().filter(|(atom, _)| offered.contains(atom)).map(|(_, name)| *name).collect()
    }

    /// The atom of text type `name` (one of `TEXT_TYPES`).
    fn text_atom(&self, name: &str) -> Atom {
        let a = &self.0.atoms;
        match name {
            "UTF8_STRING" => a.UTF8_STRING,
            "text/plain;charset=utf-8" => a.TEXT_PLAIN,
            "text/plain" => a.TEXT_PLAIN_ANY,
            "TEXT" => a.TEXT,
            _ => AtomEnum::STRING.into(),
        }
    }
```

`impl super::Backend for X11`'e:

```rust
    fn paste_kind(&self) -> Option<PasteKind> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) {
            // Gezik's own: copied paths are text, cut or copied files paste as files.
            return s.state().clipboard.as_ref().and_then(|o| o.text.as_ref()).map(|_| PasteKind::Text);
        }
        super::paste_kind_of(&self.offered_names())
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) || !self.offered_names().contains(&"image/png") {
            return Ok(None);
        }
        let png = s.transfer_within(s.atoms.CLIPBOARD, s.atoms.IMAGE_PNG, CURRENT_TIME, IMAGE_TIMEOUT);
        Ok(png.map(ClipboardImage::Png))
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) {
            return Ok(s.state().clipboard.as_ref().and_then(|o| o.text.clone()));
        }
        let offered = self.offered_names();
        let Some(name) = super::text_type(&offered) else { return Ok(None) };
        let Some(bytes) = s.transfer(s.atoms.CLIPBOARD, self.text_atom(name), CURRENT_TIME) else { return Ok(None) };
        Ok(Some(if name == "STRING" { super::from_latin1(&bytes) } else { String::from_utf8_lossy(&bytes).into_owned() }))
    }
```

**Wayland** (`wayland.rs`): `const IMAGE_TIMEOUT: Duration = Duration::from_secs(5);`; `receive` ikiye ayrılır: `fn receive(&self, offer: &WlDataOffer, mime: &str) -> Option<Vec<u8>> { self.receive_within(offer, mime, TRANSFER_TIMEOUT) }` ve bugünkü gövdeyle `fn receive_within(&self, offer: &WlDataOffer, mime: &str, timeout: Duration) -> Option<Vec<u8>>` (`read_with_timeout(reader, timeout)`). `impl Wayland`'e:

```rust
    /// The selection offer and its types, if the compositor told Gezik one.
    fn selection(&self) -> Option<(WlDataOffer, Vec<String>)> {
        let offer = self.0.inner().selection.clone()?;
        let types = offer
            .data::<OfferData>()
            .map(|d| d.lock().unwrap_or_else(std::sync::PoisonError::into_inner).types.clone())
            .unwrap_or_default();
        Some((offer, types))
    }

    /// Gezik's own text on the clipboard (what is there when the compositor tells no offer).
    fn own_text(&self) -> Option<String> {
        self.0.inner().clipboard.as_ref().and_then(|(_, owned)| owned.text.clone())
    }
```

`impl super::Backend for Wayland`'e:

```rust
    fn paste_kind(&self) -> Option<PasteKind> {
        let Some((_, types)) = self.selection() else { return self.own_text().map(|_| PasteKind::Text) };
        let names: Vec<&str> = types.iter().map(String::as_str).collect();
        super::paste_kind_of(&names)
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        let Some((offer, types)) = self.selection() else { return Ok(None) };
        if !types.iter().any(|t| t == "image/png") {
            return Ok(None);
        }
        Ok(self.0.receive_within(&offer, "image/png", IMAGE_TIMEOUT).map(ClipboardImage::Png))
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let Some((offer, types)) = self.selection() else { return Ok(self.own_text()) };
        let names: Vec<&str> = types.iter().map(String::as_str).collect();
        let Some(name) = super::text_type(&names) else { return Ok(None) };
        let Some(bytes) = self.0.receive(&offer, name) else { return Ok(None) };
        Ok(Some(if name == "STRING" { super::from_latin1(&bytes) } else { String::from_utf8_lossy(&bytes).into_owned() }))
    }
```

- [ ] **Step 7: Run.** `cargo test -j 8 -p gezik-platform` → PASS (Windows'ta `a_dib_becomes_a_bmp_file`, `a_picture_is_written_as_png`, `unicode_text_ends_at_its_nul`, `files_win_then_an_image_then_text`). Panoyu değiştiren `text_comes_back_as_text_to_paste` yalnız kullanıcı uzaktayken (`-- --ignored`), değilse Task 9'un bekleyen listesine. Dört komut.

- [ ] **Step 8: Commit** "Read pictures and text from the clipboard on Windows, macOS, X11 and Wayland".

---

### Task 4: Bağlantı bırakma — `Effect::Link`, tuş kuralı, sistemlerin sürükle-bırakı, `Create link here`

**Files:**
- Modify: `crates/gezik-core/src/drag.rs`, `crates/gezik-platform/src/dnd/windows.rs`, `crates/gezik-platform/src/dnd/macos.rs`, `crates/gezik-platform/src/linux/x11.rs` (tuşlar, `XdndActionLink`, gelenin izinleri), `crates/gezik-platform/src/linux/wayland.rs` (`Allowed.link`), `crates/gezik/src/drag.rs`, `crates/gezik/src/operations.rs` (`transfer`'in `Link` kolu), `crates/gezik/src/context_menu.rs` (`CREATE_LINK_HERE`, `drop_menu`)

**Interfaces:**
- Consumes: Task 2'nin `LinkTask::into`, `LinkKind::for_drops`.
- Produces:

```rust
// gezik_core::drag
pub enum Effect { Copy, Move, Link }
pub struct Keys { pub shift: bool, pub copy: bool, pub link: bool }
pub struct Allowed { pub copy: bool, pub move_: bool, pub link: bool }
impl Allowed { pub const ALL: Allowed; }          // BOTH kalkar
pub enum DragOs { Windows, Mac, Linux }
impl DragOs { pub fn current() -> DragOs; }
pub fn keys_of(os: DragOs, shift: bool, ctrl: bool, alt: bool, command: bool) -> Keys;
// gezik::context_menu
pub const CREATE_LINK_HERE: u32 = 1407;
impl Menus {
    pub fn drop_menu(&self, paths: Vec<PathBuf>, dir: PathBuf, archive: Option<PathBuf>,
                     can_copy: bool, can_move: bool, can_link: bool, x: f32, y: f32);
}
```

- [ ] **Step 1: Write the failing tests** (`gezik-core/src/drag.rs` test modülü; var olan `Keys { shift, copy }` yazımları `Keys { shift: …, copy: …, ..Keys::default() }`, `Allowed::BOTH` → `Allowed::ALL`, `Allowed { copy, move_ }` → `Allowed { copy, move_, link: false }` olur):

```rust
    #[test]
    fn link_keys_differ_by_system() {
        // (shift, ctrl, alt, command)
        let link = |os, k: (bool, bool, bool, bool)| keys_of(os, k.0, k.1, k.2, k.3).link;
        assert!(link(DragOs::Windows, (false, false, true, false)), "Windows: Alt");
        assert!(link(DragOs::Windows, (true, true, false, false)), "Windows: Ctrl+Shift");
        assert!(!link(DragOs::Linux, (false, false, true, false)), "Linux: Alt-drags are the window manager's");
        assert!(link(DragOs::Linux, (true, true, false, false)), "Linux: Ctrl+Shift");
        assert!(link(DragOs::Mac, (false, false, true, true)), "macOS: Cmd+Option");
        assert!(!link(DragOs::Mac, (false, false, true, false)), "macOS: Option alone copies");
        assert_eq!(keys_of(DragOs::Mac, false, false, true, false), Keys { shift: false, copy: true, link: false });
        assert_eq!(keys_of(DragOs::Windows, true, false, false, false), Keys { shift: true, copy: false, link: false });
    }

    #[test]
    fn a_link_falls_back_when_the_source_forbids_it() {
        let link = Keys { shift: true, copy: true, link: true };
        assert_eq!(choose(link, true, Allowed::ALL), Some(Effect::Link));
        let no_link = Allowed { link: false, ..Allowed::ALL };
        assert_eq!(choose(link, true, no_link), Some(Effect::Move), "both keys: the drive rule");
        assert_eq!(choose(link, false, no_link), Some(Effect::Copy));
        assert_eq!(choose(Keys::default(), true, Allowed { copy: false, move_: false, link: true }), Some(Effect::Link));
        let p = PathBuf::from(if cfg!(windows) { r"C:\w\a" } else { "/w/a" });
        assert!(!refuse(std::slice::from_ref(&p), p.parent().unwrap(), Effect::Link), "a link next to it");
        assert!(refuse(std::slice::from_ref(&p), &p.join("sub"), Effect::Link), "never inside itself");
        assert_eq!(label(Action::Transfer(Effect::Link), &p), "Create link in a");
    }
```

`gezik-platform/src/dnd/windows.rs` testine:

```rust
        assert_eq!(drop_reply(Some(Effect::Link)), (DROPEFFECT_LINK, None, None));
        assert_eq!(to_dropeffect(Some(Effect::Link)), DROPEFFECT_LINK);
        let d = description(Some(&Answer { effect: Some(Effect::Link), folder: Some("Belgeler".into()) }));
        assert_eq!(d.r#type, DROPIMAGE_LINK);
        assert_eq!(keys_of(MODIFIERKEYS_FLAGS(MK_ALT)), Keys { shift: false, copy: false, link: true });
        assert_eq!(allowed_by(DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0 | DROPEFFECT_LINK.0)), Allowed::ALL);
```

(Var olan `keys_of(MK_SHIFT | MK_CONTROL)` beklentisi `Keys { shift: true, copy: true, link: true }`, `allowed_by(DROPEFFECT_COPY)` beklentisi `Allowed { copy: true, move_: false, link: false }` olur.)

`gezik/src/context_menu.rs` testlerinde: `conversion_ids_meet_no_others`'ın `singles`'ına ve `drop_menu_ids_are_their_own`'ın `for id in [COPY_HERE, MOVE_HERE, CANCEL_DROP, …]` listesine `CREATE_LINK_HERE`.

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core drag` → derlenmez.

- [ ] **Step 3: Implement `gezik-core/src/drag.rs`** (eski `Effect`, `Keys`, `Allowed`, `choose` yerine):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Copy,
    Move,
    /// A link to each item in the folder (spec 9.2).
    Link,
}

/// The keys held during a drag, by what they ask for: Shift moves, the copy key (Ctrl;
/// Option on macOS) copies, the link keys (`keys_of`) link.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Keys {
    pub shift: bool,
    pub copy: bool,
    pub link: bool,
}

/// Whose keys a drag reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOs {
    Windows,
    Mac,
    Linux,
}

impl DragOs {
    pub fn current() -> DragOs {
        if cfg!(windows) {
            DragOs::Windows
        } else if cfg!(target_os = "macos") {
            DragOs::Mac
        } else {
            DragOs::Linux
        }
    }
}

/// The keys held, read as each system's file manager reads them (spec 9.2): a link is Alt or
/// Ctrl+Shift on Windows (Explorer), Ctrl+Shift on Linux (GTK and KDE; many window managers
/// take Alt-drags), Cmd+Option on macOS (Finder; Option alone copies). `ctrl` is the Control
/// key, `command` macOS's Command key.
pub fn keys_of(os: DragOs, shift: bool, ctrl: bool, alt: bool, command: bool) -> Keys {
    match os {
        DragOs::Windows => Keys { shift, copy: ctrl, link: alt || (shift && ctrl) },
        DragOs::Linux => Keys { shift, copy: ctrl, link: shift && ctrl },
        DragOs::Mac => Keys { shift, copy: alt, link: alt && command },
    }
}

/// What the drag's source lets the target do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    pub copy: bool,
    pub move_: bool,
    pub link: bool,
}

impl Allowed {
    pub const ALL: Allowed = Allowed { copy: true, move_: true, link: true };
}

/// The effect of a drop: the link keys link (if the source lets them); Shift moves, the copy
/// key copies; otherwise (or with both) a move on the same drive and a copy to another.
/// Limited to what the source allows (None: nothing is).
pub fn choose(keys: Keys, same_drive: bool, allowed: Allowed) -> Option<Effect> {
    if keys.link && allowed.link {
        return Some(Effect::Link);
    }
    let wanted = match (keys.shift, keys.copy) {
        (true, false) => Effect::Move,
        (false, true) => Effect::Copy,
        _ if same_drive => Effect::Move,
        _ => Effect::Copy,
    };
    let allows = |effect| match effect {
        Effect::Copy => allowed.copy,
        Effect::Move => allowed.move_,
        Effect::Link => allowed.link,
    };
    let other = if wanted == Effect::Copy { Effect::Move } else { Effect::Copy };
    [wanted, other, Effect::Link].into_iter().find(|&effect| allows(effect))
}
```

`label`'a `Action::Transfer(Effect::Link) => format!("Create link in {}", folder_name(folder)),`.

- [ ] **Step 4: Sistemler.**

`dnd/windows.rs` (`use`'a `DROPEFFECT_LINK`, `windows::Win32::System::Ole::MK_ALT` (bir `u32`), `DROPIMAGE_LINK`):

```rust
pub(crate) fn drop_reply(effect: Option<Effect>) -> (DROPEFFECT, Option<DROPEFFECT>, Option<DROPEFFECT>) {
    match effect {
        Some(Effect::Move) => (DROPEFFECT_NONE, Some(DROPEFFECT_NONE), Some(DROPEFFECT_MOVE)),
        Some(Effect::Copy) => (DROPEFFECT_COPY, None, None),
        Some(Effect::Link) => (DROPEFFECT_LINK, None, None),
        None => (DROPEFFECT_NONE, None, None),
    }
}

fn allowed_by(effects: DROPEFFECT) -> Allowed {
    let has = |effect: DROPEFFECT| effects.0 & effect.0 != 0;
    Allowed { copy: has(DROPEFFECT_COPY), move_: has(DROPEFFECT_MOVE), link: has(DROPEFFECT_LINK) }
}

fn keys_of(state: MODIFIERKEYS_FLAGS) -> Keys {
    let held = |bit: u32| state.0 & bit != 0;
    gezik_core::drag::keys_of(gezik_core::drag::DragOs::Windows, held(MK_SHIFT.0), held(MK_CONTROL.0), held(MK_ALT), false)
}
```

`to_dropeffect`'e `Some(Effect::Link) => DROPEFFECT_LINK,`; `description`'a `Some(Effect::Link) => (DROPIMAGE_LINK, "Create link in %1"),`. Gezik'in dışarıya sürüklemesinin izinleri değişmez (sapma 11).

`dnd/macos.rs`:

```rust
fn operation_for(effect: Option<Effect>) -> NSDragOperation {
    match effect {
        Some(Effect::Copy) => NSDragOperation::Copy,
        Some(Effect::Move) => NSDragOperation::Generic,
        Some(Effect::Link) => NSDragOperation::Link,
        None => NSDragOperation::None,
    }
}

fn allowed_by(mask: NSDragOperation) -> Allowed {
    Allowed {
        copy: mask.contains(NSDragOperation::Copy),
        move_: mask.contains(NSDragOperation::Move) || mask.contains(NSDragOperation::Generic),
        link: mask.contains(NSDragOperation::Link),
    }
}

fn keys_now() -> Keys {
    let flags = NSEvent::modifierFlags_class();
    gezik_core::drag::keys_of(
        gezik_core::drag::DragOs::Mac,
        flags.contains(NSEventModifierFlags::Shift),
        flags.contains(NSEventModifierFlags::Control),
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Command),
    )
}
```

`linux/x11.rs`: atom listesine `XdndActionLink,`;

```rust
    fn keys(&self) -> Keys {
        let mask = self.conn.query_pointer(self.root).ok().and_then(|c| c.reply().ok()).map(|r| r.mask);
        let held = |bit: KeyButMask| mask.is_some_and(|mask| u16::from(mask) & u16::from(bit) != 0);
        gezik_core::drag::keys_of(
            gezik_core::drag::DragOs::Linux,
            held(KeyButMask::SHIFT),
            held(KeyButMask::CONTROL),
            held(KeyButMask::MOD1),
            false,
        )
    }
```

`action_atom`'a `Some(Effect::Link) => self.atoms.XdndActionLink,`; gelen sürüklemenin `Offer`'ında `Allowed::BOTH` → `Allowed::ALL`. Gezik'in dışarı sürüklemesi (`if keys.shift && !keys.copy { Move } else { Copy }`) değişmez.

`linux/wayland.rs`: `Allowed { copy: actions.contains(DndAction::Copy), move_: actions.contains(DndAction::Move), link: false }` (protokolde bağlantı eylemi yok, sapma 11); `answer` değişmez.

- [ ] **Step 5: Arayüz.** `gezik/src/drag.rs`:

```rust
/// The keys held, as this system's file manager reads them (`drag::keys_of`). Slint reports
/// macOS's Command key as `control`.
fn keys(shift: bool, ctrl: bool, alt: bool) -> Keys {
    match drag::DragOs::current() {
        drag::DragOs::Mac => drag::keys_of(drag::DragOs::Mac, shift, false, alt, ctrl),
        os => drag::keys_of(os, shift, ctrl, alt, false),
    }
}
```

`Allowed::BOTH` → `Allowed::ALL` (üç yer; `Platform` içe aktarımı kullanılmıyorsa kalkar); `drop_on`'un sağ tuş dalı:

```rust
        if d.right {
            let can = |effect| {
                let allowed = match effect {
                    Effect::Copy => d.allowed.copy,
                    Effect::Move => d.allowed.move_,
                    Effect::Link => d.allowed.link,
                };
                allowed && self.writable(&dir) && !drag::refuse(&d.sources, &dir, effect)
            };
            let (can_copy, can_move, can_link) = (can(Effect::Copy), can(Effect::Move), can(Effect::Link));
            let archive = target.archive.filter(|_| target.action == Some(Action::AddToArchive));
            self.0.menus.drop_menu(d.sources, dir, archive, can_copy, can_move, can_link, d.x, d.y);
            return None;
        }
```

Testlerdeki `Keys { shift: true, copy: false }` → `Keys { shift: true, ..Keys::default() }`.

`operations.rs` (`use gezik_ops::LinkTask; use gezik_core::templates::LinkKind;`):

```rust
    /// Copies, moves or links `paths` into folder `dir` (a paste or a drop), as one undoable
    /// job.
    pub fn transfer(&self, paths: Vec<PathBuf>, dir: PathBuf, effect: Effect) {
        self.remember_for(&paths);
        let retry: Retry = Rc::new(move || -> Box<dyn Task> {
            match effect {
                Effect::Move => Box::new(MoveTask::into(paths.clone(), &dir)),
                Effect::Copy => Box::new(CopyTask::into(paths.clone(), &dir)),
                Effect::Link => Box::new(LinkTask::into(paths.clone(), &dir, LinkKind::for_drops())),
            }
        });
        self.submit(retry(), Some(retry), After::Select);
    }
```

`context_menu.rs`: kimlik (`SIZE_FORMAT_FIRST`'ten sonra, 7c bloğunu başlatır):

```rust
/// 7c's ids are 1400-1459 (spec 11.1). 1407: "Create link here" after a drag with the right
/// button.
pub const CREATE_LINK_HERE: u32 = 1407;
```

`drop_menu` `can_link: bool`'u `can_move`'dan sonra alır; `can_move` öğesinden sonra:

```rust
        if can_link {
            list.push((CREATE_LINK_HERE, "Create link here"));
        }
```

`run`'a `(CREATE_LINK_HERE, Subject::Drop(paths, dir, _)) => self.ops.transfer(paths, dir, Effect::Link),`.

- [ ] **Step 6: Run.** `cargo test -j 8 -p gezik-core drag`, `cargo test -j 8 -p gezik-platform dnd`, `cargo test -j 8 -p gezik context_menu drag` → PASS. Elle (Windows, kullanıcı uzaktayken değilse Task 9 listesine): Explorer'dan bir dosyayı Alt ile Gezik'in listesine sürükle → imleç bağlantı, "Create link in …", bırakınca `… - Shortcut.lnk`, Ctrl+Z çöpe atar. Dört komut.

- [ ] **Step 7: Commit** "Make links by dropping with each system's link keys and from the right-drag menu".

---

### Task 5: Eylemler ve tuşlar, `templates/` klasörü ve şablon listesi

**Files:**
- Modify: `crates/gezik-config/src/shortcuts.rs` (dört eylem, varsayılanlar, testler), `crates/gezik-config/templates/settings.toml` (`[shortcuts]` yorumları), `crates/gezik-config/src/store.rs` (`templates_dir`, `is_template_path`, ilk çalıştırmada `templates/`), `crates/gezik/src/keys.rs` (eylem grupları, ulaşılabilirlik testi), `crates/gezik/src/actions.rs` (dördü şimdilik "Not theirs"), `crates/gezik/src/main.rs` (`handle_key`'in eylem listesi, şablonların okunması, izleyici), `crates/gezik/src/watcher.rs` (`templates/` değişikliği ayrı haber)
- Create: `crates/gezik/src/templates.rs`

**Interfaces:**
- Consumes: Task 2'nin `gezik_core::templates::{Template, template_list}`.
- Produces:

```rust
// gezik_config::shortcuts
Action::{NewFolderWithSelection, AddToStack, ToggleStack, ShowHistory}   // "new-folder-with-selection", "add-to-stack", "toggle-stack", "show-history"; Action::ALL.len() == 63
// gezik_config::store::ConfigStore
pub fn templates_dir(&self) -> PathBuf;
pub fn is_template_path(&self, path: &Path) -> bool;
// gezik::templates
pub fn set_dir(dir: Option<PathBuf>);
pub fn dir() -> Option<PathBuf>;
pub fn current() -> Vec<Template>;
pub fn read(dir: &Path) -> Vec<Template>;
pub fn refresh(dir: PathBuf);            // any thread: reads, then keeps the list on the UI thread
pub fn load_in_background();             // UI thread: `refresh` of `dir()` on a thread of its own
pub fn open_folder();                    // makes the folder if needed (off the UI thread), opens it in a new tab
// gezik::watcher
pub fn watch_config(store: &ConfigStore, on_change: impl Fn() + Send + 'static,
                    on_templates: impl Fn() + Send + 'static) -> notify::Result<RecommendedWatcher>;
```

- [ ] **Step 1: Write the failing tests**

`shortcuts.rs` test modülüne (ve `pins_have_alt_and_cmd_option_digits`'teki `assert_eq!(Action::ALL.len(), 59);` → `63`):

```rust
    #[test]
    fn the_daily_7c_actions_have_their_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(other.action_for(&chord("ctrl+alt+n")), Some(Action::NewFolderWithSelection));
        assert_eq!(mac.action_for(&mac_chord("mod+ctrl+n")), Some(Action::NewFolderWithSelection), "Finder's ⌃⌘N");
        assert_eq!(other.action_for(&chord("ctrl+shift+s")), Some(Action::AddToStack));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+s")), Some(Action::AddToStack));
        for action in [Action::ToggleStack, Action::ShowHistory] {
            assert_eq!((other.chord_for(action), mac.chord_for(action)), (None, None), "{}", action.name());
        }
        for (text, platform) in [("ctrl+alt+n", Platform::Other), ("ctrl+shift+s", Platform::Other)] {
            assert_eq!(fixed_owner(&parse_chord(text, platform).unwrap().unwrap(), platform), None, "{text}");
        }
        for (text, name) in [
            ("mod+ctrl+n", "new-folder-with-selection"),
            ("mod+shift+s", "add-to-stack"),
        ] {
            assert_eq!(fixed_owner(&mac_chord(text), Platform::Mac), None, "{name}");
        }
        for name in ["new-folder-with-selection", "add-to-stack", "toggle-stack", "show-history"] {
            assert!(Action::from_name(name).is_some(), "{name}");
        }
    }
```

`store.rs` test modülüne:

```rust
    #[test]
    fn the_templates_folder_comes_with_the_first_run_only() {
        let store = fresh_store("templates");
        store.ensure_initialized().unwrap();
        assert!(store.templates_dir().is_dir());
        std::fs::remove_dir(store.templates_dir()).unwrap();
        store.ensure_initialized().unwrap();
        assert!(!store.templates_dir().exists(), "a deleted templates folder does not come back");
    }

    #[test]
    fn recognizes_template_paths() {
        let store = store("templates-watch");
        let root = store.dir();
        assert!(store.is_template_path(&root.join("templates")));
        assert!(store.is_template_path(&root.join("templates").join("Report.docx")));
        assert!(store.is_template_path(&root.join("templates").join("Project").join("a.txt")));
        assert!(!store.is_template_path(&root.join("settings.toml")));
        assert!(!store.is_template_path(&root.join("themes").join("templates")));
        assert!(!store.is_config_file(&root.join("templates").join("x.toml")), "no settings reload for a template");
    }
```

`keys.rs` `every_default_is_reachable_on_windows_and_linux`'un `match`'ine `Action::NewFolderWithSelection => "ctrl+alt+n",`, `Action::AddToStack => "ctrl+shift+s",`, `continue` listesine `| Action::ToggleStack | Action::ShowHistory`. Ayrıca:

```rust
    #[test]
    fn the_7c_actions_wait_for_the_list() {
        assert!(acts_on_files(Action::NewFolderWithSelection) && needs_list(Action::NewFolderWithSelection));
        assert!(acts_on_selection(Action::AddToStack) && needs_list(Action::AddToStack));
        assert!(!waits_for_text_fields(Action::ToggleStack) && !waits_for_text_fields(Action::ShowHistory));
        // Ctrl+Alt+N types nothing with AltGr on US and Turkish Q: a shortcut (spec 10.3).
        assert!(!altgr_types('n', true, true, true));
    }
```

Yeni `crates/gezik/src/templates.rs`'in test modülü:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_read_as_new_lists_it() {
        let dir = std::env::temp_dir().join(format!("gezik-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Project/src")).unwrap();
        std::fs::write(dir.join("Report.docx"), "r").unwrap();
        std::fs::write(dir.join(".DS_Store"), "x").unwrap();
        let list = read(&dir);
        let shown: Vec<(&str, &str, bool)> = list.iter().map(|t| (t.name.as_str(), t.label.as_str(), t.is_dir)).collect();
        assert_eq!(shown, [("Project", "Project", true), ("Report.docx", "Report", false)]);
        assert!(read(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-config` ve `cargo test -j 8 -p gezik keys templates` → derlenmez.

- [ ] **Step 3: Implement `gezik-config`.**

`shortcuts.rs`: `Action`'a `Pin9`'dan sonra:

```rust
    /// Moves the selected items into a new folder next to them (7c).
    NewFolderWithSelection,
    /// Puts the selected items on the drop stack.
    AddToStack,
    /// Shows or hides the drop stack.
    ToggleStack,
    /// Opens the operations panel on its History.
    ShowHistory,
```

`ALL: [Action; 63]` (dördü sona), `name`'e `"new-folder-with-selection"`, `"add-to-stack"`, `"toggle-stack"`, `"show-history"`; `default_texts`'e:

```rust
            // Finder's ⌃⌘N; Ctrl+Alt+N may type with AltGr on a few layouts (Polish ń): the
            // template's comment says so (spec 10.3).
            (Action::NewFolderWithSelection, Platform::Mac) => &["mod+ctrl+n"],
            (Action::NewFolderWithSelection, Platform::Other) => &["ctrl+alt+n"],
            (Action::AddToStack, _) => &["mod+shift+s"],
            (Action::ToggleStack, _) => &[],
            (Action::ShowHistory, _) => &[],
```

`templates/settings.toml`'un `[shortcuts]` yorumlarının sonuna (`pin-1` satırından sonra):

```toml
# new-folder-with-selection = "ctrl+alt+n"   # the selected items into a new folder; macOS: "mod+ctrl+n" (may type a letter with AltGr on some layouts: pick another key then)
# add-to-stack = "mod+shift+s"   # the selected items onto the drop stack
# toggle-stack = ""        # show or hide the drop stack
# show-history = ""        # the operations panel's History
```

(`the_template_lists_every_action_with_its_default`-türü var olan test (`settings.rs` ~2005) yorumları okuyup her eylemin varsayılanını karşılaştırır; satırlar oradaki biçimde `# ad = "…"` olmalı.)

`store.rs`:

```rust
    /// The user's templates for New ▸ (spec 8.1).
    pub fn templates_dir(&self) -> PathBuf {
        self.dir.join("templates")
    }

    /// Whether `path` is the templates folder or in it: New ▸ reads the folder again (and
    /// settings are not reloaded for it).
    pub fn is_template_path(&self, path: &Path) -> bool {
        path.strip_prefix(&self.dir)
            .is_ok_and(|rest| rest.components().next().is_some_and(|first| first.as_os_str() == "templates"))
    }
```

`ensure_initialized`'ın `if first_run` bloğuna `std::fs::create_dir_all(self.templates_dir())?;`, belgesine "…and the templates folder".

- [ ] **Step 4: Implement the UI side.**

`keys.rs`: `acts_on_files`'a `| Action::NewFolderWithSelection`, `acts_on_selection`'a `| Action::AddToStack`, `needs_list`'e `| Action::NewFolderWithSelection | Action::AddToStack`.

`actions.rs`: dört eylem şimdilik "Not theirs" listesine (`| Action::NewFolderWithSelection | Action::AddToStack | Action::ToggleStack | Action::ShowHistory => return false,`); Task 6, 7, 8 doldurur. `main.rs` `handle_key`: dördü `actions::run`'a giden kolun listesine (`| Action::Pin9`'dan sonra).

Yeni `templates.rs` (testi Step 1'de), `main.rs`'e `mod templates;`:

```rust
//! The user's templates (spec 8.1): `<config>/templates/`, read on a thread of its own after
//! start and again whenever the folder changes (the UI thread reads no folder); New ▸ lists
//! what is kept here.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use gezik_core::nav::Location;
use gezik_core::templates::{Template, template_list};

thread_local! {
    /// The templates folder (none without a config folder) and its list as last read.
    static STATE: RefCell<(Option<PathBuf>, Vec<Template>)> = const { RefCell::new((None, Vec::new())) };
}

pub fn set_dir(dir: Option<PathBuf>) {
    STATE.with(|state| state.borrow_mut().0 = dir);
}

pub fn dir() -> Option<PathBuf> {
    STATE.with(|state| state.borrow().0.clone())
}

/// The templates as last read (at most `TEMPLATE_MAX`).
pub fn current() -> Vec<Template> {
    STATE.with(|state| state.borrow().1.clone())
}

/// The folder's entries as New ▸ lists them; none if it cannot be read. A link to a folder is
/// a folder template; a name that is not Unicode is left out (it is copied by name).
pub fn read(dir: &Path) -> Vec<Template> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    template_list(entries.filter_map(Result::ok).filter_map(|entry| {
        let path = entry.path();
        let name = entry.file_name().into_string().ok()?;
        let is_dir = std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir());
        Some((name, is_dir, gezik_platform::fs::is_hidden_attr(&path)))
    }))
}

/// Reads `dir` (on the calling thread, never the UI's) and keeps the list on the UI thread.
pub fn refresh(dir: PathBuf) {
    let list = read(&dir);
    let _ = slint::invoke_from_event_loop(move || STATE.with(|state| state.borrow_mut().1 = list));
}

/// `refresh` of the templates folder on a thread of its own.
pub fn load_in_background() {
    let Some(dir) = dir() else { return };
    let _ = std::thread::Builder::new().name("gezik-templates".into()).spawn(move || refresh(dir));
}

/// "Open templates folder": made if it is not there (off the UI thread), then opened in a new
/// tab of Gezik.
pub fn open_folder() {
    let Some(dir) = dir() else {
        crate::view::with_current(|view| view.note("No config folder for templates".to_owned()));
        return;
    };
    let _ = std::thread::Builder::new().name("gezik-templates".into()).spawn(move || {
        let made = std::fs::create_dir_all(&dir);
        let _ = slint::invoke_from_event_loop(move || match made {
            Ok(()) => crate::navigation::with_current(|nav| nav.open_tab(Location::Path(dir), true)),
            Err(err) => {
                let why = gezik_platform::fs::describe(&err);
                crate::view::with_current(|view| view.note(format!("Cannot make the templates folder: {why}")));
            }
        });
    });
}
```

(`crate::view::with_current` yoksa — `operations.rs` onu kullanıyor, var — aynı adla.)

`watcher.rs`:

```rust
/// Calls `on_change` on a background thread once per burst of changes to `settings.toml` or a
/// theme file, and `on_templates` once per burst in the templates folder. Keep the returned
/// watcher alive while watching.
pub fn watch_config(
    store: &ConfigStore,
    on_change: impl Fn() + Send + 'static,
    on_templates: impl Fn() + Send + 'static,
) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(store.dir(), RecursiveMode::Recursive)?;

    let store = store.clone();
    std::thread::spawn(move || {
        // (settings or a theme, a template) a change touched.
        let touched = |event: &notify::Result<notify::Event>| match event {
            Ok(e) => (
                e.paths.iter().any(|path| store.is_config_file(path)),
                e.paths.iter().any(|path| store.is_template_path(path)),
            ),
            Err(_) => (false, false),
        };
        while let Ok(event) = rx.recv() {
            let (mut config, mut templates) = touched(&event);
            if !config && !templates {
                continue;
            }
            // Gather the rest of the burst, then reload once.
            while let Ok(event) = rx.recv_timeout(DEBOUNCE) {
                let (c, t) = touched(&event);
                config |= c;
                templates |= t;
            }
            if config {
                on_change();
            }
            if templates {
                on_templates();
            }
        }
    });
    Ok(watcher)
}
```

`main.rs`: `config` bulunduktan hemen sonra `templates::set_dir(config.as_ref().map(ConfigStore::templates_dir));`; `watch_config` çağrısına üçüncü argüman:

```rust
            {
                let dir = store.templates_dir();
                move || templates::refresh(dir.clone())
            },
```

ve `places::load_in_background(…)` satırından sonra (açılışı bekletmez; okuma kendi iş parçacığında):

```rust
    // The templates of New ▸, read once the window is up.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), templates::load_in_background);
```

- [ ] **Step 5: Run.** `cargo test -j 8 -p gezik-config` (`defaults_cover_every_action`, şablon testi `[shortcuts]` yorumlarını varsayılanlarla karşılaştırır), `cargo test -j 8 -p gezik keys templates` → PASS. Dört komut.

- [ ] **Step 6: Commit** "Name new-folder-with-selection, add-to-stack, toggle-stack and show-history, and keep the templates folder's list".

---

### Task 6: Menüler ve işler — `New ▸`, seçimle yeni klasör, panodan dosya, `Create link ▸`, Explorer'ın `link` fiili

**Files:**
- Modify: `crates/gezik/src/context_menu.rs` (kimlikler, `background_items`, `new_sub`, `link_items`, `junction_offered`, menülerin kurulması, `run`, `run_verb`, `from_submenu`, testler), `crates/gezik/src/operations.rs` (`new_markdown`, `new_from_template`, `new_folder_with`, `new_folder_with_selection`, `paste_as`, `paste_as_file`, `create_links`; `paste`'in dosyasız dalı), `crates/gezik/src/actions.rs` (`NewFolderWithSelection`), `crates/gezik/src/main.rs` (symlink denemesi), `crates/gezik/ui/app.slint` (macOS File ▸ New Folder with Selection), `crates/gezik-platform/src/lib.rs` (`ShellVerb::Link`)

**Interfaces:**
- Consumes: Task 1 (`GroupTask::new`, `NewTask::{markdown, with_contents}`, `CopyTask::template`), Task 2 (`LinkTask::beside`, `LinkKind`, `pasted_name`, `PasteKind`, `Template`, `TEMPLATE_MAX`, `link::{symlinks_allowed, probe_symlinks}`), Task 3 (`clipboard::{paste_kind, read_image, read_text, ClipboardImage}`), Task 5 (`templates::{current, dir, open_folder}`, `Action::NewFolderWithSelection`).
- Produces:

```rust
// gezik::context_menu
pub const NEW_MARKDOWN: u32 = 1400;  pub const OPEN_TEMPLATES: u32 = 1401;
pub const NEW_FOLDER_WITH_SELECTION: u32 = 1402;  pub const PASTE_AS_FILE: u32 = 1403;
pub const LINK_SHORTCUT: u32 = 1404;  pub const LINK_JUNCTION: u32 = 1405;  pub const LINK_SYMLINK: u32 = 1406;
pub const TEMPLATE_FIRST: u32 = 1410; pub const TEMPLATE_MAX: u32 = 50;
pub fn background_items(undo: Option<&str>, redo: Option<&str>, can_paste: bool, paste_as: Option<PasteKind>, windows: bool) -> Vec<(u32, String)>;
pub fn new_sub(templates: &[Template], windows: bool, at: usize) -> Submenu;
pub fn link_items(windows: bool, junction: bool, symlink: bool) -> Vec<(u32, String, bool)>;
pub fn junction_offered(rows: &[(PathBuf, bool)], network_drives: &[PathBuf]) -> bool;
// gezik::operations::Operations
pub fn new_markdown(&self, dir: PathBuf);
pub fn new_from_template(&self, dir: PathBuf, template: &Template);
pub fn new_folder_with(&self, paths: Vec<PathBuf>);
pub fn new_folder_with_selection(&self);
pub fn paste_as(&self) -> Option<PasteKind>;      // what paste would write as a file (no files on the clipboard)
pub fn paste_as_file(&self, dir: PathBuf);
pub fn create_links(&self, paths: Vec<PathBuf>, kind: LinkKind);
// gezik_platform
ShellVerb::Link   // Explorer's "Create shortcut" (`link`)
```

- [ ] **Step 1: Write the failing tests**

`gezik-platform/src/lib.rs` `explorer_verbs_gezik_does`'a: `assert_eq!(ShellVerb::from_name(b"link"), Some(ShellVerb::Link));`.

`context_menu.rs` testlerinde: var olan `background_items_say_what_undo_does` yeni imzayla:

```rust
    #[test]
    fn background_items_say_what_undo_does() {
        let items = background_items(Some("Copy 3 items"), None, true, None, true);
        assert_eq!(items[0], (UNDO, "Undo Copy 3 items".to_owned()));
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [UNDO, PASTE, NEW_FOLDER, NEW_FILE, REFRESH]);
        let bare: Vec<u32> = background_items(None, None, false, None, true).iter().map(|(id, _)| *id).collect();
        assert_eq!(bare, [NEW_FOLDER, NEW_FILE, REFRESH]);
    }

    #[test]
    fn the_background_menu_says_what_paste_does() {
        use gezik_core::templates::PasteKind;
        let ids = |items: Vec<(u32, String)>| items.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(
            ids(background_items(None, None, true, Some(PasteKind::Image), true)),
            [PASTE, NEW_FOLDER, NEW_FILE, REFRESH],
            "files paste as files"
        );
        assert_eq!(
            background_items(None, None, false, Some(PasteKind::Image), false),
            [(PASTE_AS_FILE, "Paste image as file".to_owned()), (REFRESH, "Refresh".to_owned())],
            "macOS and Linux: New folder and New file are in New ▸"
        );
        assert_eq!(background_items(None, None, false, Some(PasteKind::Text), true)[0].1, "Paste text as file");
    }

    #[test]
    fn new_lists_the_built_ins_the_templates_and_the_folder() {
        let templates = gezik_core::templates::template_list([
            ("Report.docx".to_owned(), false, false),
            ("Project".to_owned(), true, false),
        ]);
        let titles = |sub: &Submenu| sub.items.iter().map(|(id, t, _)| (*id, t.clone())).collect::<Vec<_>>();
        let elsewhere = new_sub(&templates, false, 3);
        assert_eq!((elsewhere.title.as_str(), elsewhere.at), ("New", 3));
        let expected: Vec<(u32, String)> = [
            (NEW_FOLDER, "Folder"),
            (NEW_FILE, "Text file"),
            (NEW_MARKDOWN, "Markdown file"),
            (TEMPLATE_FIRST, "Project"),
            (TEMPLATE_FIRST + 1, "Report"),
            (OPEN_TEMPLATES, "Open templates folder"),
        ]
        .iter()
        .map(|(id, t)| (*id, (*t).to_owned()))
        .collect();
        assert_eq!(titles(&elsewhere), expected);
        let windows = new_sub(&templates, true, 5);
        assert_eq!(windows.title, "New from template", "Explorer's own New ▸ is in the same menu");
        assert_eq!(titles(&windows)[0], (NEW_MARKDOWN, "Markdown file".to_owned()));
        assert!(from_submenu(TEMPLATE_FIRST + 49) && from_submenu(NEW_MARKDOWN) && from_submenu(OPEN_TEMPLATES));
        assert_eq!(TEMPLATE_MAX as usize, gezik_core::templates::TEMPLATE_MAX);
    }

    #[test]
    fn link_items_follow_the_system_and_what_can_be_made() {
        let ids = |v: Vec<(u32, String, bool)>| v.into_iter().map(|(id, _, _)| id).collect::<Vec<_>>();
        assert_eq!(ids(link_items(true, true, true)), [LINK_SHORTCUT, LINK_JUNCTION, LINK_SYMLINK]);
        assert_eq!(ids(link_items(true, false, false)), [LINK_SHORTCUT]);
        assert_eq!(link_items(false, false, true), [(LINK_SYMLINK, "Create link".to_owned(), true)]);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_is_offered_for_local_folders_only() {
        let row = |path: &str, is_dir| (PathBuf::from(path), is_dir);
        let network = [PathBuf::from(r"Z:\")];
        assert!(junction_offered(&[row(r"C:\a", true), row(r"C:\b", true)], &network));
        assert!(!junction_offered(&[row(r"C:\a", true), row(r"C:\f.txt", false)], &network), "a file");
        assert!(!junction_offered(&[row(r"\\srv\share\a", true)], &network), "a share");
        assert!(!junction_offered(&[row(r"Z:\a", true)], &network), "a mapped drive");
        assert!(!junction_offered(&[], &network));
    }
```

`conversion_ids_meet_no_others`'ın `singles`'ına `NEW_MARKDOWN, OPEN_TEMPLATES, NEW_FOLDER_WITH_SELECTION, PASTE_AS_FILE, LINK_SHORTCUT, LINK_JUNCTION, LINK_SYMLINK`, `ranges`'ına `TEMPLATE_FIRST..TEMPLATE_FIRST + TEMPLATE_MAX`.

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik context_menu` ve `cargo test -j 8 -p gezik-platform explorer_verbs` → derlenmez.

- [ ] **Step 3: `gezik-platform/src/lib.rs`.** `ShellVerb`'e `/// Explorer's "Create shortcut".` `Link,`; `from_name`'e `b"link" => Some(ShellVerb::Link),`.

- [ ] **Step 4: `operations.rs`** (`use gezik_core::templates::{LinkKind, PasteKind, Template, pasted_name};`, `use gezik_ops::{GroupTask, LinkTask};`, `use gezik_platform::clipboard::ClipboardImage;` gerekirse):

```rust
    /// An empty Markdown file in `dir`, renamed right away (spec 8.1).
    pub fn new_markdown(&self, dir: PathBuf) {
        self.submit(Box::new(NewTask::markdown(&dir)), None, After::Rename);
    }

    /// A copy of the user's `template` in `dir`, renamed right away (spec 8.1).
    pub fn new_from_template(&self, dir: PathBuf, template: &Template) {
        let Some(templates) = crate::templates::dir() else { return };
        let (source, is_dir) = (templates.join(&template.name), template.is_dir);
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(CopyTask::template(source.clone(), &dir, is_dir)) });
        self.submit(retry(), Some(retry), After::Rename);
    }

    /// `new-folder-with-selection`: the selected items into a new folder next to them.
    pub fn new_folder_with_selection(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        self.new_folder_with(self.0.view.selected_paths());
    }

    /// `paths` (in the folder shown) moved into a new "New folder" there, as one job, and the
    /// folder renamed right away (spec 8.2).
    pub fn new_folder_with(&self, paths: Vec<PathBuf>) {
        let Some(dir) = self.0.view.folder() else { return };
        let paths = self.without_roots(paths, "move");
        if paths.is_empty() {
            return self.0.view.note("Select the items to put in a new folder".to_owned());
        }
        self.remember_for(&paths);
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(GroupTask::new(paths.clone(), &dir)) });
        self.submit(retry(), Some(retry), After::Rename);
    }

    /// What paste would write as a file: nothing while there are files to paste.
    pub fn paste_as(&self) -> Option<PasteKind> {
        if self.can_paste() { None } else { clipboard::paste_kind() }
    }

    /// The clipboard's picture (before its text: a browser's picture often carries both) as a
    /// new `Pasted image … .png` in `dir`, else its text as `Pasted text … .txt` (spec 9.1).
    /// The data is read here; the picture is encoded in the job.
    pub fn paste_as_file(&self, dir: PathBuf) {
        let Some(at) = gezik_platform::local_date_parts(std::time::SystemTime::now()) else { return };
        let failed = |err: ClipboardError| match err {
            ClipboardError::Failed(why) => Some(format!("Cannot use the clipboard: {why}")),
            ClipboardError::Unsupported => None,
        };
        match clipboard::read_image() {
            Ok(Some(image)) => {
                let name = pasted_name(PasteKind::Image, &at);
                self.submit(Box::new(NewTask::with_contents(&dir, &name, move || image.png_bytes())), None, After::Select);
                return;
            }
            Ok(None) => {}
            Err(err) => return failed(err).into_iter().for_each(|text| self.0.view.note(text)),
        }
        match clipboard::read_text() {
            Ok(Some(text)) if !text.is_empty() => {
                let (name, bytes) = (pasted_name(PasteKind::Text, &at), text.into_bytes());
                self.submit(Box::new(NewTask::with_contents(&dir, &name, move || Ok(bytes.clone()))), None, After::Select);
            }
            Ok(_) => {}
            Err(err) => failed(err).into_iter().for_each(|text| self.0.view.note(text)),
        }
    }

    /// A link of `kind` next to each of `paths` ("Create link ▸", Explorer's "Create shortcut").
    pub fn create_links(&self, paths: Vec<PathBuf>, kind: LinkKind) {
        let paths = self.without_roots(paths, "link to");
        if paths.is_empty() {
            return;
        }
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(LinkTask::beside(paths.clone(), kind)) });
        self.submit(retry(), Some(retry), After::Select);
    }
```

`paste`'te `let Some(ClipboardFiles { paths, cut }) = self.clipboard() else { return };` → `else { return self.paste_as_file(dir) };` (Ctrl+V, "Paste into folder"). Belgesine "…no files: the picture or text as a new file (spec 9.1)".

- [ ] **Step 5: `context_menu.rs`.** Kimlikler (Task 4'ün `CREATE_LINK_HERE`'inin yanına):

```rust
/// 1400: New ▸ Markdown file; 1401: Open templates folder; 1402: New folder with selection;
/// 1403: Paste image/text as file; 1404-1406: Create link ▸ Shortcut, Junction, Symbolic link
/// (elsewhere the one "Create link" is 1406); 1410-1459: the user's templates by place.
pub const NEW_MARKDOWN: u32 = 1400;
pub const OPEN_TEMPLATES: u32 = 1401;
pub const NEW_FOLDER_WITH_SELECTION: u32 = 1402;
pub const PASTE_AS_FILE: u32 = 1403;
pub const LINK_SHORTCUT: u32 = 1404;
pub const LINK_JUNCTION: u32 = 1405;
pub const LINK_SYMLINK: u32 = 1406;
pub const TEMPLATE_FIRST: u32 = 1410;
pub const TEMPLATE_MAX: u32 = gezik_core::templates::TEMPLATE_MAX as u32;
```

`background_items` yerine:

```rust
/// The items for empty space in a folder: Undo/Redo say what they would do; Paste, or what
/// paste would write as a file; New folder and New file on Windows (elsewhere in New ▸).
pub fn background_items(
    undo: Option<&str>,
    redo: Option<&str>,
    can_paste: bool,
    paste_as: Option<PasteKind>,
    windows: bool,
) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if let Some(label) = undo {
        out.push((UNDO, format!("Undo {label}")));
    }
    if let Some(label) = redo {
        out.push((REDO, format!("Redo {label}")));
    }
    match (can_paste, paste_as) {
        (true, _) => out.push((PASTE, "Paste".to_owned())),
        (false, Some(PasteKind::Image)) => out.push((PASTE_AS_FILE, "Paste image as file".to_owned())),
        (false, Some(PasteKind::Text)) => out.push((PASTE_AS_FILE, "Paste text as file".to_owned())),
        (false, None) => {}
    }
    if windows {
        out.push((NEW_FOLDER, "New folder".to_owned()));
        out.push((NEW_FILE, "New file".to_owned()));
    }
    out.push((REFRESH, "Refresh".to_owned()));
    out
}

/// New ▸ (macOS, Linux: Folder, Text file, Markdown file) or New from template ▸ (Windows,
/// where Explorer's New ▸ is in the same menu: Markdown file), then the user's templates,
/// then Open templates folder; at place `at` (spec 8.1). No separator: submenus have none.
pub fn new_sub(templates: &[Template], windows: bool, at: usize) -> Submenu {
    let mut items = Vec::new();
    if !windows {
        items.push((NEW_FOLDER, "Folder".to_owned(), true));
        items.push((NEW_FILE, "Text file".to_owned(), true));
    }
    items.push((NEW_MARKDOWN, "Markdown file".to_owned(), true));
    items.extend(
        templates
            .iter()
            .take(TEMPLATE_MAX as usize)
            .enumerate()
            .map(|(i, template)| (TEMPLATE_FIRST + i as u32, template.label.clone(), true)),
    );
    items.push((OPEN_TEMPLATES, "Open templates folder".to_owned(), true));
    let title = if windows { "New from template" } else { "New" };
    Submenu { title: title.to_owned(), at, items }
}

/// Create link ▸ on Windows (Shortcut; Junction for local folders; Symbolic link when it can
/// be made), or the one "Create link" (a symbolic link) elsewhere (spec 9.2).
pub fn link_items(windows: bool, junction: bool, symlink: bool) -> Vec<(u32, String, bool)> {
    if !windows {
        return vec![(LINK_SYMLINK, "Create link".to_owned(), true)];
    }
    let mut out = vec![(LINK_SHORTCUT, "Shortcut".to_owned(), true)];
    if junction {
        out.push((LINK_JUNCTION, "Junction".to_owned(), true));
    }
    if symlink {
        out.push((LINK_SYMLINK, "Symbolic link".to_owned(), true));
    }
    out
}

/// Whether Junction is offered for `rows`: all folders, none on a share or on one of
/// `network_drives` (mapped drives). The file system (NTFS) is checked by the job: the menu
/// reads no disk.
pub fn junction_offered(rows: &[(PathBuf, bool)], network_drives: &[PathBuf]) -> bool {
    !rows.is_empty()
        && rows.iter().all(|(path, is_dir)| {
            *is_dir
                && !path.to_string_lossy().starts_with(r"\\")
                && !network_drives.iter().any(|drive| gezik_core::ops::paths::is_within(path, drive))
        })
}
```

`Menus`'a alan `menu_templates: Rc<RefCell<Vec<Template>>>` (belge: "The templates the last New ▸ listed (items are by place)"), `new`'de `Rc::default()`.

`row_menu`, çoklu seçim dalı (`let mut subs = vec![self.copy_path_sub(&paths, list.len())];`'dan sonra, `add_file_tools`'tan önce):

```rust
            self.add_links(&mut list, &mut subs, &rows, native);
```

ve `list.extend(self.file_extras(false, false, native));`'den sonra:

```rust
            if !self.view.shows_drives() {
                list.push((NEW_FOLDER_WITH_SELECTION, "New folder with selection".to_owned()));
            }
```

(`rows`, `add_file_tools`'a taşınmadan önce `let rows = self.view.selected_items();` olarak zaten var; `add_links` ona referansla bakar, sonra `add_file_tools(…, rows, …)`.) Tek satır dalında `subs`'tan sonra `self.add_links(&mut list, &mut subs, &[(path.clone(), is_dir)], native);`. Ekle:

```rust
    /// Create link ▸ (Windows) or Create link at the end of the items so far, for `rows`;
    /// nothing for drives.
    fn add_links(&self, list: &mut Vec<(u32, String)>, subs: &mut Vec<Submenu>, rows: &[(PathBuf, bool)], native: bool) {
        if self.view.shows_drives() {
            return;
        }
        let network: Vec<PathBuf> = self
            .nav
            .places()
            .drives
            .into_iter()
            .filter(|drive| drive.kind == gezik_platform::DriveKind::Network)
            .map(|drive| drive.path)
            .collect();
        let items = link_items(native, junction_offered(rows, &network), gezik_platform::link::symlinks_allowed());
        if native {
            subs.push(Submenu { title: "Create link".to_owned(), at: list.len(), items });
        } else {
            list.extend(items.into_iter().map(|(id, title, _)| (id, title)));
        }
    }
```

`background_menu`:

```rust
    fn background_menu(&self, at: Option<(f32, f32)>, x: f32, y: f32) {
        let Location::Path(dir) = self.nav.active_location() else { return };
        self.ops.clipboard_check();
        let can_paste = self.ops.can_paste();
        let paste_as = if can_paste { None } else { gezik_platform::clipboard::paste_kind() };
        let windows = cfg!(windows);
        let mut list = background_items(
            self.ops.undo_label().as_deref(),
            self.ops.redo_label().as_deref(),
            can_paste,
            paste_as,
            windows,
        );
        let templates = crate::templates::current();
        let new_at = list.iter().position(|(id, _)| *id == REFRESH).unwrap_or(list.len());
        let mut subs = vec![new_sub(&templates, windows, new_at)];
        *self.menu_templates.borrow_mut() = templates;
        list.extend(owned(terminal_items(windows)));
        subs.push(self.copy_path_sub(std::slice::from_ref(&dir), list.len()));
        self.open(Subject::Background(dir.clone()), list, subs, MenuTarget::Background(dir), x, y, at);
    }
```

`run`'a (`(NEW_FILE, Subject::Background(dir)) …` satırının yanına):

```rust
            (NEW_MARKDOWN, Subject::Background(dir)) => self.ops.new_markdown(dir),
            (OPEN_TEMPLATES, Subject::Background(_)) => crate::templates::open_folder(),
            (id, Subject::Background(dir)) if (TEMPLATE_FIRST..TEMPLATE_FIRST + TEMPLATE_MAX).contains(&id) => {
                let template = self.menu_templates.borrow().get((id - TEMPLATE_FIRST) as usize).cloned();
                if let Some(template) = template {
                    self.ops.new_from_template(dir, &template);
                }
            }
            (PASTE_AS_FILE, Subject::Background(dir)) => self.ops.paste_as_file(dir),
            (NEW_FOLDER_WITH_SELECTION, Subject::Rows(paths)) => self.ops.new_folder_with(paths),
            (LINK_SHORTCUT | LINK_JUNCTION | LINK_SYMLINK, Subject::Row(path)) => {
                self.ops.create_links(vec![path], link_kind(id))
            }
            (LINK_SHORTCUT | LINK_JUNCTION | LINK_SYMLINK, Subject::Rows(paths)) => {
                self.ops.create_links(paths, link_kind(id))
            }
```

ve modül düzeyinde:

```rust
/// The link a Create link item makes.
fn link_kind(id: u32) -> LinkKind {
    match id {
        LINK_SHORTCUT => LinkKind::Shortcut,
        LINK_JUNCTION => LinkKind::Junction,
        _ => LinkKind::Symlink,
    }
}
```

`run_verb`'e `ShellVerb::Link => self.ops.create_links(paths, LinkKind::Shortcut),`. `from_submenu`'ya:

```rust
        || (TEMPLATE_FIRST..TEMPLATE_FIRST + TEMPLATE_MAX).contains(&id)
        || matches!(id, NEW_FOLDER | NEW_FILE | NEW_MARKDOWN | OPEN_TEMPLATES | LINK_SHORTCUT | LINK_JUNCTION | LINK_SYMLINK)
```

(`use gezik_core::templates::{LinkKind, PasteKind, Template};`.)

- [ ] **Step 6: Eylem, açılış, macOS.** `actions.rs`: `Action::NewFolderWithSelection => crate::operations::with_current(crate::operations::Operations::new_folder_with_selection),` ve onu "Not theirs" listesinden çıkar. `main.rs`'te `ops.recover()`'ı çağıran zamanlayıcının yanına:

```rust
    // Whether symbolic links can be made (Windows: Developer Mode), tried once in the
    // background: Create link ▸ offers them from then on (spec 9.2).
    slint::Timer::single_shot(std::time::Duration::from_millis(500), || {
        let _ = std::thread::Builder::new()
            .name("gezik-symlink-probe".into())
            .spawn(gezik_platform::link::probe_symlinks);
    });
```

`app.slint` macOS File menüsünde `New Folder`'ın altına:

```slint
            MenuItem { title: "New Folder with Selection"; shortcut: root.menu-keys ? @keys(Control + Meta + N) : @keys(); activated => { root.menu-command("new-folder-with-selection"); } }
```

(Slint macOS'ta ⌘'i `Control`, ⌃'yi `Meta` diye adlandırır; `menu_bar.rs` adı `Action::from_name` ile bulur ve tuşu çalar, değişiklik gerekmez.)

- [ ] **Step 7: Run.** `cargo test -j 8 -p gezik context_menu operations`, `cargo test -j 8 -p gezik-platform` → PASS. Elle (Windows; kullanıcı uzakta değilse Task 9'un bekleyen listesine): `GEZIK_CONFIG_DIR` boş bir geçici klasör, `cargo run -j 8 -p gezik` → klasör boşluğuna sağ tık: "New from template ▸ Markdown file" → `New document.md`, ad kutusu açık; `templates/`'e `Report.docx` konunca (Open templates folder) menüde "Report"; iki dosya seç → "New folder with selection" → klasör seçili ve adı düzenleniyor, Ctrl+Z ikisini geri getirir; Ekran Alıntısı Aracı ile kopyalanmış görüntüde Ctrl+V → `Pasted image … .png`; bir klasöre sağ tık "Create link ▸ Junction" → `Link to …`; Explorer menüsündeki "Create shortcut" → `… - Shortcut.lnk`, Ctrl+Z çöpe atar. Dört komut.

- [ ] **Step 8: Commit** "Add New ▸ with templates, new folder with selection, paste as file, Create link ▸, and Explorer's Create shortcut to the menus".

---

### Task 7: Bırakma yığını — `Hit::Stack`, `stack.rs`, `drop-stack.slint`, eylemler ve menüler

**Files:**
- Modify: `crates/gezik-core/src/drag.rs` (`Hit::Stack`, `Layout.stack`, `Action::AddToStack`), `crates/gezik/src/drag.rs` (yığına bırakma, şeritten sürükleme), `crates/gezik/src/operations.rs` (`transfer_job`, biten işin yığına haber verilmesi), `crates/gezik/src/context_menu.rs` (`TOGGLE_STACK`, `view_items`), `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs`, `crates/gezik/ui/app.slint`
- Create: `crates/gezik/src/stack.rs`, `crates/gezik/ui/widgets/drop-stack.slint`

**Interfaces:**
- Consumes: Task 4'ün `Effect`, `Keys`, `Allowed::ALL`, `drop_menu`; Task 5'in `Action::{AddToStack, ToggleStack}`.
- Produces:

```rust
// gezik_core::drag
Hit::Stack;  Layout { …, pub stack: Option<Rect> };  Action::AddToStack   // label "Add to drop stack"
// gezik::stack
pub const STACK_MAX: usize = 1000;  pub const STACK_SHOWN: usize = 100;
pub struct StackItem { pub path: PathBuf, pub is_dir: bool, pub gone: bool }  // Rust-side; Slint's is `StackRow`
pub struct StackItems;   // add, remove, clear, items, usable, checked, moved
pub struct Stack;        // the UI's
pub fn with_current(f: impl FnOnce(&Stack));
impl Stack {
    pub fn new(window: &AppWindow, view: View, ops: Operations) -> Stack;
    pub fn add(&self, items: Vec<(PathBuf, bool)>);
    pub fn add_selection(&self);
    pub fn toggle(&self);
    pub fn is_open(&self) -> bool;
    pub fn job_finished(&self, id: JobId, report: &Report);
}
// gezik::operations::Operations
pub fn transfer_job(&self, paths: Vec<PathBuf>, dir: PathBuf, effect: Effect) -> JobId;   // `transfer` calls it
// gezik::drag::Drags
pub fn stack_down(&self, items: Vec<(PathBuf, bool)>, x: f32, y: f32);
// gezik::context_menu
pub const TOGGLE_STACK: u32 = 1408;
pub fn view_items(view: ViewSettings, preview_open: bool, stack_open: bool, options: ViewOptions, windows: bool) -> Vec<(u32, String)>;
```

- [ ] **Step 1: Write the failing tests**

`gezik-core/src/drag.rs` test modülünde `layout()` yardımcısı `stack: Some(Rect { x: 0.0, y: 600.0, width: 1000.0, height: 30.0 })` alır; ekle:

```rust
    #[test]
    fn the_drop_stack_strip_takes_drops() {
        let mut l = layout(list(0));
        assert_eq!(hit(&l, 500.0, 610.0, false), Hit::Stack);
        assert_eq!(hit(&l, 500.0, 590.0, false), Hit::Nothing, "between the list and the strip");
        l.stack = None;
        assert_eq!(hit(&l, 500.0, 610.0, false), Hit::Nothing, "a closed strip takes nothing");
        assert_eq!(label(Action::AddToStack, Path::new("")), "Add to drop stack");
    }
```

Yeni `crates/gezik/src/stack.rs`'in test modülü:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(if cfg!(windows) { format!(r"C:\s\{name}") } else { format!("/s/{name}") })
    }

    #[test]
    fn the_stack_keeps_each_path_once_and_at_most_a_thousand() {
        let mut s = StackItems::default();
        assert_eq!(s.add([(p("a"), false), (p("b"), true), (p("a"), false)]), (2, 0));
        if cfg!(windows) {
            assert_eq!(s.add([(p("A"), false)]), (0, 0), "the case is ignored as Windows does");
        }
        let many = (0..1100).map(|i| (p(&format!("f{i}")), false));
        assert_eq!(s.add(many), (STACK_MAX - 2, 1100 - (STACK_MAX - 2)));
        assert_eq!(s.items().len(), STACK_MAX);
        s.remove(0);
        assert_eq!(s.items()[0].path, p("b"));
        s.remove(5000);
        s.clear();
        assert!(s.items().is_empty());
    }

    #[test]
    fn gone_items_are_left_out_and_failed_moves_stay() {
        let mut s = StackItems::default();
        s.add([(p("a"), false), (p("b"), false), (p("c"), false)]);
        s.checked(&[(p("b"), None), (p("a"), Some(false)), (p("c"), Some(true))]);
        assert!(s.items()[1].gone);
        assert!(s.items()[2].is_dir, "the check tells folders apart");
        assert_eq!(s.usable(), [p("a"), p("c")]);
        s.moved(&[p("a"), p("c")], &[p("c").join("inner.txt")]);
        let left: Vec<PathBuf> = s.items().iter().map(|i| i.path.clone()).collect();
        assert_eq!(left, [p("b"), p("c")], "c had a failure inside: it stays; a went");
        s.checked(&[(p("b"), Some(false))]);
        assert!(!s.items()[0].gone, "back again");
    }

    #[test]
    fn the_strip_says_how_many_and_how_many_more() {
        assert_eq!(count_text(1), "1 item");
        assert_eq!(count_text(12), "12 items");
        assert_eq!(more_text(STACK_SHOWN), "");
        assert_eq!(more_text(STACK_SHOWN + 900), "+900 more");
    }
}
```

`gezik/src/drag.rs` testine:

```rust
    #[test]
    fn a_press_on_the_stack_is_forgotten_by_a_menu() {
        let press = Phase::StackArmed { items: vec![(PathBuf::from("a.txt"), false)], x: 0.0, y: 0.0 };
        assert_eq!(at_menu(&press), AtMenu::Forget);
        assert_eq!(window_drives(true, &press), None);
    }
```

`context_menu.rs`: `view_items(…)` çağrıları yeni imzayla (`stack_open` `preview_open`'dan sonra, testlerde `false`); `view_menu_marks_the_current_choices`'ın beklenen kimliklerinde `PREVIEW_PANE`'den sonra `TOGGLE_STACK`; `view_menu_lists_the_options_and_their_marks`'ta `&ids[10..]` → `&ids[11..]`, `items[10..13]` → `items[11..14]`; `format_subs(…, 15)` → `16`; ve:

```rust
    #[test]
    fn the_view_menu_shows_and_hides_the_drop_stack() {
        use gezik_core::view::{ViewOptions, ViewSettings};
        let shown = view_items(ViewSettings::default(), false, true, ViewOptions::default(), false);
        assert!(shown.contains(&(TOGGLE_STACK, "• Drop stack".to_owned())));
        let hidden = view_items(ViewSettings::default(), false, false, ViewOptions::default(), false);
        assert!(hidden.contains(&(TOGGLE_STACK, "    Drop stack".to_owned())));
    }
```

`conversion_ids_meet_no_others`'ın `singles`'ına `TOGGLE_STACK`.

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core drag`, `cargo test -j 8 -p gezik stack drag context_menu` → derlenmez.

- [ ] **Step 3: `gezik-core/src/drag.rs`.** `Layout`'a `/// The drop stack strip; None while it is closed. pub stack: Option<Rect>,`; `Hit`'e `/// The drop stack strip (spec 9.3). Stack,`; `Action`'a `/// Onto the drop stack: the paths are kept there. AddToStack,`; `label`'a `Action::AddToStack => "Add to drop stack".to_owned(),`. `hit`'te `crumbs` denetiminden sonra:

```rust
    if let Some(stack) = &layout.stack
        && stack.contains(x, y)
    {
        return Hit::Stack;
    }
```

- [ ] **Step 4: `drop-stack.slint`:**

```slint
import { Theme } from "../theme.slint";
import { FileIcon } from "file-icon.slint";
import { TextButton } from "text-button.slint";

// One item of the drop stack (stack.rs): its name, `Kind` (gezik-core kind.rs), and whether
// it is gone (drawn faded, left out of Copy here / Move here).
export struct StackRow {
    name: string,
    kind: int,
    gone: bool,
}

// The drop stack (spec 9.3): a strip above the status bar. Files dropped on it are kept;
// Copy here / Move here take them to the folder shown. A press on an item drags it, a press on
// the count drags them all.
export component DropStack inherits Rectangle {
    in property <[StackRow]> rows;
    in property <string> count;
    in property <string> more;
    // A drop would land here.
    in property <bool> highlight;
    // A press on item `int` (-1: the count, all of them), at a window position.
    callback down(int, length, length);
    // The press moved (window position, Shift, Ctrl, Alt); it came up; it was taken away.
    callback drag(length, length, bool, bool, bool);
    callback up(length, length);
    callback cancel();
    callback hovered(int);
    callback remove(int);
    callback copy-here();
    callback move-here();
    callback clear();

    height: Theme.row-height + 6px;
    background: root.highlight ? Theme.drop-target : Theme.surface;
    accessible-role: list;
    accessible-label: "Drop stack";

    // The pointer events of a press that may become a drag (`index`: what it drags).
    component Grip inherits TouchArea {
        in property <int> index;
        callback down(int, length, length);
        callback drag(length, length, bool, bool, bool);
        callback up(length, length);
        callback cancel();
        pointer-event(event) => {
            if event.kind == PointerEventKind.down && event.button == PointerEventButton.left {
                root.down(self.index, self.absolute-position.x + self.mouse-x, self.absolute-position.y + self.mouse-y);
            }
            if event.kind == PointerEventKind.move && self.pressed {
                root.drag(self.absolute-position.x + self.mouse-x, self.absolute-position.y + self.mouse-y,
                    event.modifiers.shift, event.modifiers.control, event.modifiers.alt);
            }
            if event.kind == PointerEventKind.up && event.button == PointerEventButton.left {
                root.up(self.absolute-position.x + self.mouse-x, self.absolute-position.y + self.mouse-y);
            }
            if event.kind == PointerEventKind.cancel {
                root.cancel();
            }
        }
    }

    HorizontalLayout {
        padding-left: Theme.spacing + 4px;
        padding-right: Theme.spacing;
        spacing: Theme.spacing * 2;
        Rectangle {
            width: caption.preferred-width + Theme.spacing * 2;
            accessible-role: button;
            accessible-label: root.rows.length == 0 ? "Drop files here" : "Drag all " + root.count;
            caption := Text {
                x: Theme.spacing;
                height: parent.height;
                text: root.rows.length == 0 ? "Drop files here" : root.count;
                vertical-alignment: center;
                color: Theme.foreground-muted;
            }
            Grip {
                index: -1;
                enabled: root.rows.length > 0;
                down(i, x, y) => { root.down(i, x, y); }
                drag(x, y, s, c, a) => { root.drag(x, y, s, c, a); }
                up(x, y) => { root.up(x, y); }
                cancel => { root.cancel(); }
            }
        }
        Flickable {
            horizontal-stretch: 1;
            viewport-width: chips.preferred-width;
            chips := HorizontalLayout {
                spacing: Theme.spacing;
                for row[i] in root.rows: Rectangle {
                    width: chip.preferred-width;
                    border-radius: Theme.radius;
                    background: grip.has-hover ? Theme.hover : transparent;
                    accessible-role: list-item;
                    accessible-label: row.name;
                    chip := HorizontalLayout {
                        padding-left: Theme.spacing;
                        spacing: Theme.spacing;
                        alignment: start;
                        VerticalLayout {
                            alignment: center;
                            FileIcon { width: 16px; height: 16px; kind: row.kind; opacity: row.gone ? 0.4 : 1; }
                        }
                        Text {
                            text: row.name;
                            vertical-alignment: center;
                            color: row.gone ? Theme.foreground-muted : Theme.foreground;
                        }
                        Rectangle {
                            width: 18px;
                            accessible-role: button;
                            accessible-label: "Remove " + row.name;
                            Text { text: "×"; horizontal-alignment: center; vertical-alignment: center; color: Theme.foreground-muted; }
                            TouchArea { clicked => { root.remove(i); } }
                        }
                    }
                    grip := Grip {
                        width: parent.width - 18px;
                        x: 0;
                        index: i;
                        changed has-hover => { root.hovered(self.has-hover ? i : -1); }
                        down(i, x, y) => { root.down(i, x, y); }
                        drag(x, y, s, c, a) => { root.drag(x, y, s, c, a); }
                        up(x, y) => { root.up(x, y); }
                        cancel => { root.cancel(); }
                    }
                }
                if root.more != "": Text {
                    text: root.more;
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
            }
        }
        VerticalLayout {
            alignment: center;
            HorizontalLayout {
                spacing: Theme.spacing;
                TextButton { text: "Copy here"; clicked => { root.copy-here(); } }
                TextButton { text: "Move here"; clicked => { root.move-here(); } }
                TextButton { text: "Clear"; clicked => { root.clear(); } }
            }
        }
    }
}
```

(Slint bir bileşenin içinde bileşen tanımına izin vermez: `Grip` dosyada `DropStack`'ten **önce**, dışa aktarılmadan, `component Grip inherits TouchArea { … }` olarak yazılır; gövdesindeki `root` Grip'in kendisidir, yani yalnız kendi geri çağrılarını çağırır, yukarıdaki kod aynen geçerlidir.)

- [ ] **Step 5: `app.slint`.** `import { DropStack, StackRow } from "widgets/drop-stack.slint";`, `export { …, StackRow }`. `DropGeometry`'ye `stack-y: length, stack-height: length,`; `drop-geometry`'ye (şerit durum çubuğunun hemen üstünde, onun 1px çizgisi ve yüksekliği kadar yukarıda):

```slint
        stack-y: root.stack-open ? root.height - Theme.row-height - 1px - (Theme.row-height + 6px) : 0px,
        stack-height: root.stack-open ? Theme.row-height + 6px : 0px,
```

Özellikler (`ops-toggle`'ın yanına):

```slint
    // The drop stack (stack.rs): open, its first items, "12 items", "+900 more", and a drop
    // landing on it.
    in property <bool> stack-open;
    in property <[StackRow]> stack-rows;
    in property <string> stack-count;
    in property <string> stack-more;
    in property <bool> drop-stack;
    callback stack-down(int, length, length);
    callback stack-hovered(int);
    callback stack-remove(int);
    callback stack-copy();
    callback stack-move();
    callback stack-clear();
```

`OpsPanel`'in ardından, durum çubuğunun `Rectangle { height: 1px; … }` çizgisinden önce:

```slint
            if root.stack-open: Rectangle { height: 1px; background: Theme.border; }
            if root.stack-open: DropStack {
                rows: root.stack-rows;
                count: root.stack-count;
                more: root.stack-more;
                highlight: root.drop-stack;
                down(i, x, y) => { root.stack-down(i, x, y); }
                drag(x, y, shift, ctrl, alt) => { root.item-drag(x, y, shift, ctrl, alt); }
                up(x, y) => { root.item-up(-1, x, y, false); }
                cancel => { root.item-cancel(); }
                hovered(i) => { root.stack-hovered(i); }
                remove(i) => { root.stack-remove(i); }
                copy-here => { root.stack-copy(); }
                move-here => { root.stack-move(); }
                clear => { root.stack-clear(); }
            }
```

(`stack-y` şeridin üstündeki 1px çizgiyi şeride saymaz; çizgiye bırakmak bir şey yapmaz. İlk elle denemede şeridin üstü ve altı `drop-stack` vurgusuyla doğrulanır.)

macOS menü çubuğunda Edit'e `Copy Path`'ten sonra `MenuItem { title: "Add to Drop Stack"; shortcut: root.menu-keys ? @keys(Control + Shift + S) : @keys(); activated => { root.menu-command("add-to-stack"); } }`, View'a `Show Preview`'dan sonra `MenuItem { title: "Drop Stack"; activated => { root.menu-command("toggle-stack"); } }`.

- [ ] **Step 6: `stack.rs`** (testleri Step 1'de), `main.rs`'e `mod stack;`:

```rust
//! The drop stack (spec 9.3): paths gathered from anywhere in a strip above the status bar,
//! then copied or moved together into the folder shown. Kept for the session only, at most
//! `STACK_MAX`; whether each still exists is checked off the UI thread.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gezik_core::drag::Effect;
use gezik_core::kind::Kind;
use gezik_core::ops::paths::{is_within, same_path};
use gezik_ops::{JobId, Report};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::operations::Operations;
use crate::view::View;
use crate::{AppWindow, StackRow};

/// The most paths the stack keeps.
pub const STACK_MAX: usize = 1000;
/// The most items the strip draws (all of them are copied or moved).
pub const STACK_SHOWN: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackItem {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Not there when last checked: drawn faded, left out.
    pub gone: bool,
}

/// The stack's paths, in the order added.
#[derive(Debug, Default)]
pub struct StackItems {
    items: Vec<StackItem>,
}

impl StackItems {
    /// Adds the paths not there yet, while there is room: (added, left out for room).
    pub fn add(&mut self, paths: impl IntoIterator<Item = (PathBuf, bool)>) -> (usize, usize) {
        let (mut added, mut full) = (0, 0);
        for (path, is_dir) in paths {
            if self.items.iter().any(|item| same_path(&item.path, &path)) {
                continue;
            }
            if self.items.len() >= STACK_MAX {
                full += 1;
                continue;
            }
            self.items.push(StackItem { path, is_dir, gone: false });
            added += 1;
        }
        (added, full)
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.items = Vec::new();
    }

    pub fn items(&self) -> &[StackItem] {
        &self.items
    }

    /// The paths still there, for Copy here / Move here.
    pub fn usable(&self) -> Vec<PathBuf> {
        self.items.iter().filter(|item| !item.gone).map(|item| item.path.clone()).collect()
    }

    /// What a check found: per path, whether it is a folder, or `None` if it is gone.
    pub fn checked(&mut self, found: &[(PathBuf, Option<bool>)]) {
        for item in &mut self.items {
            if let Some((_, state)) = found.iter().find(|(path, _)| same_path(path, &item.path)) {
                item.gone = state.is_none();
                if let Some(is_dir) = state {
                    item.is_dir = *is_dir;
                }
            }
        }
    }

    /// `moved` were moved away: they leave the stack, but for one that failed (or had a
    /// failure inside it), which stays.
    pub fn moved(&mut self, moved: &[PathBuf], failed: &[PathBuf]) {
        self.items.retain(|item| {
            let went = moved.iter().any(|path| same_path(path, &item.path));
            !went || failed.iter().any(|failure| is_within(failure, &item.path))
        });
    }
}

/// "12 items".
pub fn count_text(n: usize) -> String {
    if n == 1 { "1 item".to_owned() } else { format!("{n} items") }
}

/// "+900 more" past what the strip draws; empty otherwise.
pub fn more_text(n: usize) -> String {
    if n > STACK_SHOWN { format!("+{} more", n - STACK_SHOWN) } else { String::new() }
}

/// Per path: whether it is a folder, `None` if it is not there. Reads the disk.
fn check(paths: Vec<PathBuf>) -> Vec<(PathBuf, Option<bool>)> {
    paths
        .into_iter()
        .map(|path| {
            let state = std::fs::metadata(&path)
                .map(|meta| meta.is_dir())
                .ok()
                .or_else(|| std::fs::symlink_metadata(&path).ok().map(|_| false));
            (path, state)
        })
        .collect()
}

thread_local! {
    static CURRENT: RefCell<Option<Stack>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's stack, if set up.
pub fn with_current(f: impl FnOnce(&Stack)) {
    if let Some(stack) = CURRENT.with(|c| c.borrow().clone()) {
        f(&stack);
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    view: View,
    ops: Operations,
    items: RefCell<StackItems>,
    open: Cell<bool>,
    rows: Rc<VecModel<StackRow>>,
    /// A Move here under way: its job and what it moves.
    moving: RefCell<Option<(JobId, Vec<PathBuf>)>>,
}

#[derive(Clone)]
pub struct Stack(Rc<Inner>);

impl Stack {
    pub fn new(window: &AppWindow, view: View, ops: Operations) -> Stack {
        let rows = Rc::new(VecModel::default());
        window.set_stack_rows(ModelRc::from(rows.clone()));
        let stack = Stack(Rc::new(Inner {
            window: window.as_weak(),
            view,
            ops,
            items: RefCell::default(),
            open: Cell::new(false),
            rows,
            moving: RefCell::default(),
        }));
        window.on_stack_down(|i, x, y| with_current(|stack| stack.down(i, x, y)));
        window.on_stack_hovered(|i| with_current(|stack| stack.hovered(i)));
        window.on_stack_remove(|i| {
            with_current(|stack| {
                if let Ok(i) = usize::try_from(i) {
                    stack.0.items.borrow_mut().remove(i);
                    stack.sync();
                }
            })
        });
        window.on_stack_copy(|| with_current(|stack| stack.send(Effect::Copy)));
        window.on_stack_move(|| with_current(|stack| stack.send(Effect::Move)));
        window.on_stack_clear(|| {
            with_current(|stack| {
                stack.0.items.borrow_mut().clear();
                stack.sync();
            })
        });
        CURRENT.with(|c| *c.borrow_mut() = Some(stack.clone()));
        stack
    }

    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }

    /// Adds `items` (path, is a folder as far as known); the strip opens. Says so when the
    /// stack is full.
    pub fn add(&self, items: Vec<(PathBuf, bool)>) {
        let (added, full) = self.0.items.borrow_mut().add(items);
        if full > 0 {
            self.0.view.note(format!("The drop stack holds at most {STACK_MAX} items"));
        }
        if added > 0 {
            self.0.open.set(true);
            self.check();
        }
        self.sync();
    }

    /// `add-to-stack`: the selected items.
    pub fn add_selection(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        let items = self.0.view.selected_items();
        if items.is_empty() {
            return self.0.view.note("Select the items to add to the drop stack".to_owned());
        }
        self.add(items);
    }

    /// `toggle-stack`, View ▸ Drop stack.
    pub fn toggle(&self) {
        self.0.open.set(!self.0.open.get());
        if self.0.open.get() {
            self.check();
        }
        self.sync();
    }

    /// A job ended: a Move here takes what it moved off the stack; what is there may have
    /// changed, so it is checked again.
    pub fn job_finished(&self, id: JobId, report: &Report) {
        let moving = self.0.moving.borrow_mut().take_if(|(job, _)| *job == id);
        if let Some((_, moved)) = moving {
            let failed: Vec<PathBuf> = report.failures.iter().map(|f| f.path.clone()).collect();
            self.0.items.borrow_mut().moved(&moved, &failed);
        }
        if !self.0.items.borrow().items().is_empty() {
            self.check();
        }
        self.sync();
    }

    /// Copy here / Move here: the paths still there, to the folder shown, as one job.
    fn send(&self, effect: Effect) {
        let Some(dir) = self.0.view.folder() else {
            return self.0.view.note("Open a folder to copy or move the drop stack into".to_owned());
        };
        let paths = self.0.items.borrow().usable();
        if paths.is_empty() {
            return self.0.view.note("The drop stack has nothing to copy or move".to_owned());
        }
        let job = self.0.ops.transfer_job(paths.clone(), dir, effect);
        if effect == Effect::Move {
            *self.0.moving.borrow_mut() = Some((job, paths));
        }
    }

    /// A press on item `index` (-1: all of them): Gezik's drag starts once it moves.
    fn down(&self, index: i32, x: f32, y: f32) {
        let items: Vec<(PathBuf, bool)> = {
            let stack = self.0.items.borrow();
            let pick = |item: &StackItem| (!item.gone).then(|| (item.path.clone(), item.is_dir));
            match usize::try_from(index) {
                Ok(i) => stack.items().get(i).and_then(pick).into_iter().collect(),
                Err(_) => stack.items().iter().filter_map(pick).collect(),
            }
        };
        if !items.is_empty() {
            crate::drag::with_current(|drags| drags.stack_down(items, x, y));
        }
    }

    /// The pointer rests on item `index`: its full path in the status bar.
    fn hovered(&self, index: i32) {
        let path = usize::try_from(index).ok().and_then(|i| self.0.items.borrow().items().get(i).map(|item| item.path.clone()));
        if let Some(path) = path {
            self.0.view.note(path.display().to_string());
        }
    }

    /// Checks on a thread of its own which paths are still there.
    fn check(&self) {
        let paths: Vec<PathBuf> = self.0.items.borrow().items().iter().map(|item| item.path.clone()).collect();
        let _ = std::thread::Builder::new().name("gezik-stack-check".into()).spawn(move || {
            let found = check(paths);
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|stack| {
                    stack.0.items.borrow_mut().checked(&found);
                    stack.sync();
                })
            });
        });
    }

    /// The strip as the stack is now.
    fn sync(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let stack = self.0.items.borrow();
        let rows = stack.items().iter().take(STACK_SHOWN).map(|item| {
            let name = item.path.file_name().map_or_else(|| item.path.display().to_string(), |n| n.to_string_lossy().into_owned());
            StackRow { kind: Kind::of(&name, item.is_dir).index(), name: name.into(), gone: item.gone }
        });
        crate::navigation::sync_model(&self.0.rows, rows);
        window.set_stack_count(count_text(stack.items().len()).into());
        window.set_stack_more(more_text(stack.items().len()).into());
        window.set_stack_open(self.0.open.get());
    }
}
```

(`Option::take_if` kararlı; yoksa `if matches!(…) { take() }`.)

- [ ] **Step 7: Bağlantılar.**

`operations.rs`: `transfer`'in gövdesi `transfer_job`'a taşınır ve `JobId` döner; `transfer` `{ self.transfer_job(paths, dir, effect); }` olur (imzası değişmez). `finished`'in sonunda (`convert::with_current`'in yanına) `crate::stack::with_current(|stack| stack.job_finished(id, &report));`.

`gezik/src/drag.rs`: `Dragging`'e `/// From the drop stack: the strip takes none of it back. from_stack: bool,` (bütün yazımlar `false`, şeritten başlayan `true`); `Phase`'e:

```rust
    /// A button went down on the drop stack's strip (an item, or all of them); a drag starts
    /// once the pointer moves far enough.
    StackArmed {
        items: Vec<(PathBuf, bool)>,
        x: f32,
        y: f32,
    },
```

`at_menu`: `Phase::Armed { .. } | Phase::StackArmed { .. } | Phase::Ended => AtMenu::Forget,`; `up`: `Phase::StackArmed { .. } => false,`. Ekle:

```rust
    /// A press on the drop stack's strip (`items`: what it drags).
    pub fn stack_down(&self, items: Vec<(PathBuf, bool)>, x: f32, y: f32) {
        *self.0.phase.borrow_mut() = Phase::StackArmed { items, x, y };
    }
```

`moved`'ın başlangıç denetimi:

```rust
        enum Begin {
            Entry(usize, bool),
            Stack(Vec<(PathBuf, bool)>),
        }
        let begin = match &*self.0.phase.borrow() {
            Phase::Armed { index, x: x0, y: y0, right, can_drag } => {
                (*can_drag && drag::past_threshold(x - x0, y - y0)).then_some(Begin::Entry(*index, *right))
            }
            Phase::StackArmed { items, x: x0, y: y0 } => {
                drag::past_threshold(x - x0, y - y0).then(|| Begin::Stack(items.clone()))
            }
            _ => None,
        };
        match begin {
            Some(Begin::Entry(index, right)) => self.start(index, right, keys),
            Some(Begin::Stack(items)) => self.start_with(items, None, false, keys, None, true),
            None => {}
        }
```

`start`'ın gövdesi `start_with`'e taşınır:

```rust
    /// The pointer went far enough from the press on entry `index`: drag the selection.
    fn start(&self, index: usize, right: bool, keys: Keys) {
        let items = self.0.view.selected_items();
        let row = self.0.view.file_row(index);
        self.start_with(items, row, right, keys, Some(index), false);
    }

    /// Drags `items` (path, is a folder); `row` gives the picture, else Gezik's own icon of the
    /// first item.
    fn start_with(
        &self,
        items: Vec<(PathBuf, bool)>,
        row: Option<crate::FileRow>,
        right: bool,
        keys: Keys,
        pressed: Option<usize>,
        from_stack: bool,
    ) {
        if items.is_empty() {
            *self.0.phase.borrow_mut() = Phase::Idle;
            return;
        }
        let Some(window) = self.0.window.upgrade() else { return };
        match row {
            Some(row) => {
                window.set_drag_icon(row.icon);
                window.set_drag_has_icon(row.has_icon);
                window.set_drag_kind(row.kind);
            }
            None => {
                let (path, is_dir) = &items[0];
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                window.set_drag_has_icon(false);
                window.set_drag_kind(gezik_core::kind::Kind::of(&name, *is_dir).index());
            }
        }
        window.set_drag_count(i32::try_from(items.len()).unwrap_or(i32::MAX));
        window.set_drag_active(true);
        self.0.handoff_failed.set(false);
        self.0.grab_lost.set(false);
        *self.0.phase.borrow_mut() = Phase::Dragging(Dragging {
            all_dirs: items.iter().all(|(_, is_dir)| *is_dir),
            sources: items.into_iter().map(|(path, _)| path).collect(),
            right,
            keys,
            allowed: Allowed::ALL,
            x: 0.0,
            y: 0.0,
            target: None,
            pressed,
            from_stack,
        });
    }
```

`layout`'ta `Layout { …, stack }`:

```rust
        let stack = (g.stack_height > 0.0)
            .then(|| Rect { x: 0.0, y: g.stack_y, width: g.window_width, height: g.stack_height });
```

`resolve`'a `Hit::Stack if d.from_stack => return Target::none(hit),` ve `Hit::Stack => return Target { action: Some(Action::AddToStack), ..Target::none(hit) },`. `drop_on`'un başına (pinden sonra, sağ tuş dalından önce: sağ tuşla da yalnız eklenir):

```rust
        if target.action == Some(Action::AddToStack) {
            let items = d.sources.into_iter().map(|path| (path, false)).collect();
            crate::stack::with_current(|stack| stack.add(items));
            // To a source program: nothing is moved, so it deletes nothing.
            return Some(Effect::Copy);
        }
```

`show`: `label` eşleşmesine `(Some(Action::AddToStack), _) => drag::label(Action::AddToStack, Path::new("")),` ve `window.set_drop_stack(target.hit == Hit::Stack && on);`. `offer_answer`'a:

```rust
            Phase::Offer(Dragging { target: Some(Target { action: Some(Action::AddToStack), .. }), .. }) => {
                Answer { effect: Some(Effect::Copy), folder: Some("Drop stack".to_owned()) }
            }
```

`context_menu.rs`: kimlik `/// 1408: View ▸ Drop stack. pub const TOGGLE_STACK: u32 = 1408;`; `view_items` `stack_open: bool`'u `preview_open`'dan sonra alır ve `PREVIEW_PANE`'den sonra `out.push((TOGGLE_STACK, mark(stack_open, "Drop stack")));`; `view_menu` `crate::stack` açıklığını geçirir (`let mut open = false; crate::stack::with_current(|s| open = s.is_open());`); `run`'da `(id, Subject::View)` genel kolundan **önce** `(TOGGLE_STACK, Subject::View) => crate::stack::with_current(crate::stack::Stack::toggle),`.

`actions.rs`: `Action::AddToStack => crate::stack::with_current(crate::stack::Stack::add_selection),`, `Action::ToggleStack => crate::stack::with_current(crate::stack::Stack::toggle),` ("Not theirs"'ten çıkar). `main.rs`: `ops` kurulduktan sonra `let _stack = stack::Stack::new(&window, view.clone(), ops.clone());`.

- [ ] **Step 8: Run.** `cargo test -j 8 -p gezik-core drag`, `cargo test -j 8 -p gezik stack drag context_menu` → PASS. Elle (Windows; kullanıcı uzakta değilse Task 9 listesine): Ctrl+Shift+S iki dosyayı ekler, şerit açılır; Explorer'dan şeride bırakılan dosya eklenir (Explorer'da kaynak yerinde); başka klasörde "Move here" → taşınır, yığından çıkar, Ctrl+Z geri getirir; silinen bir öğe soluk ve atlanır; şeritteki bir öğeyi listeye sürükleyince kopya/taşıma kuralı; View ▸ Drop stack kapatır. Dört komut.

- [ ] **Step 9: Commit** "Gather files on a drop stack and copy or move them together".

---

### Task 8: İşlem günlüğü — `op_history.rs`, panelde `History` sekmesi, `show-history`

**Files:**
- Create: `crates/gezik/src/op_history.rs`
- Modify: `crates/gezik/src/operations.rs` (kayıt, sekme, `show_history`, `history_show`, `history_details`; `details` ortak metni kullanır), `crates/gezik/src/navigation.rs` (`go_selecting`), `crates/gezik/ui/widgets/ops-panel.slint` (sekmeler, `HistoryRow`), `crates/gezik/ui/app.slint` (özellikler, durum çubuğunun `History` düğmesi, macOS View ▸ Operation History), `crates/gezik/src/context_menu.rs` (`SHOW_HISTORY`), `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: Task 5'in `Action::ShowHistory`; Task 7'nin `view_items(view, preview_open, stack_open, options, windows)`.
- Produces:

```rust
// gezik::op_history
pub const HISTORY_MAX: usize = 200;  pub const SHOW_MAX: usize = 1000;  pub const MAX_DETAILS: usize = 50;
pub struct Record { pub id: u64, pub time: String, pub title: String, pub result: String, pub failed: bool,
                    pub show: Option<(PathBuf, Vec<String>)>, pub details: Option<String> }
pub struct History;   // push(time, title, &Report) -> u64, records() (newest first), get(id)
pub fn result_text(report: &Report) -> (String, bool);
pub fn show_target(report: &Report) -> Option<(PathBuf, Vec<String>)>;
pub fn details_text(report: &Report) -> Option<String>;
// gezik::navigation::Navigator
pub fn go_selecting(&self, dir: PathBuf, names: Vec<String>);
// gezik::operations::Operations
pub fn show_history(&self);
// gezik::context_menu
pub const SHOW_HISTORY: u32 = 1409;
```

- [ ] **Step 1: Write the failing tests** (yeni `op_history.rs`'in test modülü):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gezik_ops::{Failure, TaskKind};

    fn report() -> Report {
        Report {
            kind: TaskKind::Copy,
            cancelled: false,
            failures: Vec::new(),
            skipped: Vec::new(),
            skipped_changed: 0,
            no_trash: Vec::new(),
            results: Vec::new(),
            changed_dirs: Vec::new(),
        }
    }

    fn failures(n: usize) -> Vec<Failure> {
        (0..n).map(|i| Failure { path: PathBuf::from(format!("f{i}.txt")), message: "It is open".to_owned() }).collect()
    }

    #[test]
    fn results_read_as_the_spec_says() {
        assert_eq!(result_text(&report()), ("Done".to_owned(), false));
        assert_eq!(result_text(&Report { skipped: failures(2), ..report() }), ("Done · 2 skipped".to_owned(), false));
        assert_eq!(result_text(&Report { failures: failures(3), ..report() }), ("3 failed".to_owned(), true));
        assert_eq!(result_text(&Report { failures: failures(1), ..report() }), ("1 failed".to_owned(), true));
        assert_eq!(result_text(&Report { cancelled: true, failures: failures(1), ..report() }), ("Cancelled".to_owned(), false));
    }

    #[test]
    fn the_history_keeps_the_newest_two_hundred() {
        let mut history = History::default();
        let first = history.push("14:05:09".to_owned(), "1".to_owned(), &report());
        for i in 2..=HISTORY_MAX + 5 {
            history.push("14:05:09".to_owned(), i.to_string(), &report());
        }
        let titles: Vec<&str> = history.records().map(|r| r.title.as_str()).collect();
        assert_eq!(titles.len(), HISTORY_MAX);
        assert_eq!(titles[0], (HISTORY_MAX + 5).to_string(), "the newest on top");
        assert!(history.get(first).is_none(), "the oldest went");
    }

    #[test]
    fn show_in_folder_goes_to_the_first_results_folder() {
        let (d, e) = (PathBuf::from("d"), PathBuf::from("e"));
        let results = vec![d.join("a"), d.join("b"), e.join("c")];
        assert_eq!(show_target(&Report { results, ..report() }), Some((d.clone(), vec!["a".to_owned(), "b".to_owned()])));
        let changed = Report { changed_dirs: vec![e.clone()], ..report() };
        assert_eq!(show_target(&changed), Some((e, Vec::new())), "no results: the first folder it changed");
        assert_eq!(show_target(&report()), None);
        let many = Report { results: (0..1500).map(|i| d.join(format!("f{i}"))).collect(), ..report() };
        assert_eq!(show_target(&many).unwrap().1.len(), SHOW_MAX);
    }

    #[test]
    fn details_list_at_most_fifty_lines() {
        assert_eq!(details_text(&report()), None);
        let text = details_text(&Report { failures: failures(60), ..report() }).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_DETAILS + 1);
        assert_eq!(lines[0], "f0.txt: It is open");
        assert_eq!(lines.last().copied(), Some("…and 10 more"));
        let skipped = details_text(&Report { skipped: failures(1), ..report() }).unwrap();
        assert_eq!(skipped, "Skipped:\nf0.txt: It is open");
    }
}
```

`context_menu.rs`: `conversion_ids_meet_no_others`'ın `singles`'ına `SHOW_HISTORY`; `view_menu_marks_the_current_choices`'ın beklenen kimliklerinde `TOGGLE_STACK`'ten sonra `SHOW_HISTORY` (ve `view_menu_lists_the_options_and_their_marks`'ta `&ids[11..]` → `&ids[12..]`, `items[11..14]` → `items[12..15]`, `format_subs(…, 16)` → `17`).

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik op_history context_menu` → derlenmez.

- [ ] **Step 3: `op_history.rs`** (testleri Step 1'de), `main.rs`'e `mod op_history;`:

```rust
//! The operations panel's History (spec 9.4): each finished job (undo and redo too) as one
//! record, newest first, at most `HISTORY_MAX`, in memory only.

use std::collections::VecDeque;
use std::path::PathBuf;

use gezik_ops::{Failure, Report};

/// The most records kept; the oldest goes first.
pub const HISTORY_MAX: usize = 200;
/// The most results "Show in folder" selects.
pub const SHOW_MAX: usize = 1000;
/// "Details" lists at most this many lines.
pub const MAX_DETAILS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: u64,
    /// When it ended: `14:05:09`.
    pub time: String,
    pub title: String,
    pub result: String,
    pub failed: bool,
    /// Where "Show in folder" goes and what it selects there.
    pub show: Option<(PathBuf, Vec<String>)>,
    pub details: Option<String>,
}

#[derive(Debug, Default)]
pub struct History {
    records: VecDeque<Record>,
    next: u64,
}

impl History {
    /// Records a job that ended with `report`; its id.
    pub fn push(&mut self, time: String, title: String, report: &Report) -> u64 {
        self.next += 1;
        let (result, failed) = result_text(report);
        self.records.push_front(Record {
            id: self.next,
            time,
            title,
            result,
            failed,
            show: show_target(report),
            details: details_text(report),
        });
        self.records.truncate(HISTORY_MAX);
        self.next
    }

    /// Newest first.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    pub fn get(&self, id: u64) -> Option<&Record> {
        self.records.iter().find(|record| record.id == id)
    }
}

/// `Done`, `Done · 2 skipped`, `3 failed`, `Cancelled`; and whether it failed.
pub fn result_text(report: &Report) -> (String, bool) {
    if report.cancelled {
        return ("Cancelled".to_owned(), false);
    }
    match (report.failures.len(), report.skipped.len()) {
        (0, 0) => ("Done".to_owned(), false),
        (0, skipped) => (format!("Done · {skipped} skipped"), false),
        (failed, _) => (format!("{failed} failed"), true),
    }
}

/// The folder of the first result and the results there (at most `SHOW_MAX`); with no
/// results, the first folder the job changed.
pub fn show_target(report: &Report) -> Option<(PathBuf, Vec<String>)> {
    if let Some(dir) = report.results.first().and_then(|first| first.parent()) {
        return Some((dir.to_path_buf(), crate::operations::result_names(&report.results, dir).into_iter().take(SHOW_MAX).collect()));
    }
    report.changed_dirs.first().map(|dir| (dir.clone(), Vec::new()))
}

/// The failures, then what was skipped under its own heading, at most `MAX_DETAILS` lines;
/// None when there are neither.
pub fn details_text(report: &Report) -> Option<String> {
    if report.failures.is_empty() && report.skipped.is_empty() {
        return None;
    }
    let named = |f: &Failure| {
        format!(
            "{}: {}",
            f.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            crate::pdf::failure_shown(&f.message)
        )
    };
    let mut lines: Vec<String> = report.failures.iter().take(MAX_DETAILS).map(named).collect();
    let room = MAX_DETAILS.saturating_sub(lines.len());
    if !report.skipped.is_empty() && room > 0 {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Skipped:".to_owned());
        lines.extend(report.skipped.iter().take(room.saturating_sub(1)).map(named));
    }
    let all = report.failures.len() + report.skipped.len();
    if all > MAX_DETAILS {
        lines.push(format!("…and {} more", all - MAX_DETAILS));
    }
    Some(lines.join("\n"))
}
```

`operations.rs`'in `details`'i satırlarını `crate::op_history::details_text(report).unwrap_or_default()` ile kurar; `MAX_DETAILS` sabiti oradan kalkar (`op_history::MAX_DETAILS`). (Eski kodda "Skipped:" başlığı satır sayısına girmiyordu; ortak işlev başlığı sayar: en çok 50 satır, spec §9.4.)

- [ ] **Step 4: `ops-panel.slint`.** Başa:

```slint
// One finished job in the History (op_history.rs).
export struct HistoryRow {
    id: int,
    time: string,
    title: string,
    result: string,
    failed: bool,
    can-show: bool,
    can-details: bool,
}
```

`OpsPanel`'e `in property <int> tab;` (0 Current, 1 History), `in property <[HistoryRow]> history;`, `callback tab-chosen(int); callback history-show(int); callback history-details(int);`. `VerticalLayout`'un başına sekme satırı, `for row in root.rows` bloğu `if root.tab == 0: VerticalLayout { … }` içine, ardından:

```slint
        HorizontalLayout {
            height: Theme.row-height;
            spacing: Theme.spacing;
            accessible-role: tab-list;
            for name[i] in ["Current", "History"]: LinkButton {
                text: (root.tab == i ? "• " : "") + name;
                accessible-role: tab;
                clicked => { root.tab-chosen(i); }
            }
            Rectangle { horizontal-stretch: 1; }
        }
        if root.tab == 1: ListView {
            height: min(root.history.length, 8) * (Theme.row-height + 2px) + 2px;
            for row in root.history: Rectangle {
                height: Theme.row-height + 2px;
                accessible-role: list-item;
                accessible-label: row.title;
                accessible-description: row.result;
                HorizontalLayout {
                    spacing: Theme.spacing * 2;
                    Text { text: row.time; width: 64px; vertical-alignment: center; color: Theme.foreground-muted; }
                    Text { text: row.title; horizontal-stretch: 1; vertical-alignment: center; overflow: elide; color: Theme.foreground; }
                    Text { text: row.result; vertical-alignment: center; color: row.failed ? Theme.danger : Theme.foreground-muted; }
                    if row.can-show: LinkButton { text: "Show in folder"; clicked => { root.history-show(row.id); } }
                    if row.can-details: LinkButton { text: "Details"; clicked => { root.history-details(row.id); } }
                }
            }
        }
```

(`ListView` için `import { ListView } from "std-widgets.slint";`; boş günlükte yükseklik 2px, satır yok: başlık "History" seçili ve boş.)

- [ ] **Step 5: `app.slint`.** `import { OpsPanel, OpRow, HistoryRow } …`, `export { …, HistoryRow }`. Özellikler:

```slint
    // The panel's tab (0 Current, 1 History), the History's rows, and whether the status
    // bar shows its History button (once a job ended this session).
    in property <int> ops-tab;
    in property <[HistoryRow]> history-rows;
    in property <bool> history-available;
    callback ops-tab-chosen(int);
    callback history-show(int);
    callback history-details(int);
    callback history-toggle();
```

`OpsPanel`'e `tab: root.ops-tab; history: root.history-rows; tab-chosen(i) => { root.ops-tab-chosen(i); } history-show(i) => { root.history-show(i); } history-details(i) => { root.history-details(i); }`. Durum çubuğunda `ops-summary` düğmesinden sonra:

```slint
                    if root.history-available: Rectangle {
                        width: history-text.preferred-width + 8px;
                        border-radius: Theme.radius;
                        background: history-touch.has-hover ? Theme.hover : transparent;
                        accessible-role: button;
                        accessible-label: "History";
                        accessible-action-default => { root.history-toggle(); }
                        history-text := Text {
                            x: 4px;
                            height: parent.height;
                            text: "History";
                            vertical-alignment: center;
                            color: Theme.foreground-muted;
                        }
                        history-touch := TouchArea {
                            mouse-cursor: pointer;
                            clicked => { root.history-toggle(); }
                        }
                    }
```

macOS View menüsüne `Drop Stack`'ten sonra `MenuItem { title: "Operation History"; activated => { root.menu-command("show-history"); } }`.

- [ ] **Step 6: `navigation.rs`.** `Inner`'a `/// Names to select once the next move is shown ("Show in folder"). select_next: Option<Vec<String>>,` (`new`'de `None`). Ekle:

```rust
    /// Goes to `dir` and selects `names` there ("Show in folder"); the folder shown is
    /// reloaded with them selected.
    pub fn go_selecting(&self, dir: PathBuf, names: Vec<String>) {
        if matches!(self.active_location(), Location::Path(ref current) if same_path(current, &dir))
            && self.refresh_showing(std::slice::from_ref(&dir), &names, None)
        {
            return;
        }
        self.0.borrow_mut().select_next = Some(names);
        self.go(Location::Path(dir));
    }
```

`finish_load`'da görünüm durumu:

```rust
        let (view, state) = {
            let mut inner = self.0.borrow_mut();
            if let Mode::Move(steps) = &mode {
                inner.tabs.active_mut().apply_steps(steps);
            }
            inner.cleared = false;
            let mut state = view_to_show(&mode, inner.tabs.active().view());
            if let Some(names) = inner.select_next.take().filter(|names| !names.is_empty()) {
                state.focus = names.first().cloned();
                state.selected = names;
            }
            (inner.view.clone(), state)
        };
```

(`refresh_showing` boş `names` ile seçimi değiştirmez; yalnız yeniden yükler.)

- [ ] **Step 7: `operations.rs`.** `Inner`'a `history: RefCell<crate::op_history::History>`, `tab: Cell<i32>`, `history_rows: Rc<VecModel<crate::HistoryRow>>` (`VecModel::default()` ayırmaz; `window.set_history_rows(…)` `new`'de). `finished`'in başında (`with_job`'dan sonra, `after`/`title` alınırken `title = job.title.clone()`):

```rust
        let time = gezik_platform::local_date_parts(std::time::SystemTime::now())
            .map(|t| format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second))
            .unwrap_or_default();
        self.0.history.borrow_mut().push(time, title, &report);
        self.sync_history();
```

Ekle:

```rust
    /// The History's rows and the status bar's button.
    fn sync_history(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let history = self.0.history.borrow();
        let rows = history.records().map(|record| crate::HistoryRow {
            id: i32::try_from(record.id).unwrap_or(i32::MAX),
            time: record.time.as_str().into(),
            title: record.title.as_str().into(),
            result: record.result.as_str().into(),
            failed: record.failed,
            can_show: record.show.is_some(),
            can_details: record.details.is_some(),
        });
        sync_model(&self.0.history_rows, rows);
        window.set_history_available(true);
    }

    /// `show-history`, View ▸ Operation history: the panel opens on its History.
    pub fn show_history(&self) {
        self.0.tab.set(1);
        self.0.collapsed.set(false);
        self.update();
    }

    /// The status bar's History button: opens the History, or closes it if it is shown.
    pub fn history_toggle(&self) {
        let shown = self.0.tab.get() == 1 && !self.0.collapsed.get();
        self.0.tab.set(if shown { 0 } else { 1 });
        self.0.collapsed.set(false);
        self.update();
    }

    pub fn choose_tab(&self, tab: i32) {
        self.0.tab.set(tab.clamp(0, 1));
        self.update();
    }

    /// "Show in folder" of record `id`.
    pub fn history_show(&self, id: i32) {
        let show = u64::try_from(id).ok().and_then(|id| self.0.history.borrow().get(id).and_then(|r| r.show.clone()));
        if let Some((dir, names)) = show {
            self.0.nav.go_selecting(dir, names);
        }
    }

    /// "Details" of record `id`: what failed or was skipped.
    pub fn history_details(&self, id: i32) {
        let record = u64::try_from(id).ok().and_then(|id| self.0.history.borrow().get(id).cloned());
        if let Some(record) = record {
            self.0.dialogs.ask(record.title, record.details.unwrap_or_default(), &["Close"], |_| {});
        }
    }
```

`update`'te panelin açıklığı: `let history = self.0.tab.get() == 1;` ve `window.set_ops_panel_open((!rows.is_empty() || history) && !self.0.collapsed.get());`, `window.set_ops_tab(self.0.tab.get());`. (Current sekmesi boşken panel bugünkü gibi kapanır; History sekmesi boşken de açık kalır.)

`main.rs`: `window.on_ops_tab_chosen`, `on_history_show`, `on_history_details`, `on_history_toggle` → `ops.choose_tab`, `ops.history_show`, `ops.history_details`, `ops.history_toggle` (var olan `on_op_*` kalıbıyla). `actions.rs`: `Action::ShowHistory => crate::operations::with_current(crate::operations::Operations::show_history),` ("Not theirs"'ten çıkar; artık listede 7c'den eylem kalmaz).

`context_menu.rs`: `/// 1409: View ▸ Operation history. pub const SHOW_HISTORY: u32 = 1409;`; `view_items`'te `TOGGLE_STACK`'ten sonra `out.push((SHOW_HISTORY, "    Operation history".to_owned()));` (işaret sütunu hizası); `run`'da genel View kolundan önce `(SHOW_HISTORY, Subject::View) => self.ops.show_history(),`.

- [ ] **Step 8: Run.** `cargo test -j 8 -p gezik op_history context_menu operations navigation` → PASS. Elle (Windows; kullanıcı uzakta değilse Task 9 listesine): bir kopya, bir başarısız taşıma (açık dosya), bir geri alma → durum çubuğunda "History" → üç kayıt, en yeni üstte, saatleriyle; "Show in folder" sonuçları seçer; başarısızda "Details". Panel kapalıyken View ▸ Operation history açar. Dört komut.

- [ ] **Step 9: Commit** "Keep a history of the session's operations in the operations panel".

---

### Task 9: Ölçüm, çapraz denetimler, notlar, test listeleri, spec

**Files:**
- Modify: `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` (yeni "Alt proje 7c … sonrası" bölümü), `docs/superpowers/notes/linux-test.md`, `docs/superpowers/notes/macos-test.md`, `docs/superpowers/specs/2026-10-08-gunluk-kolayliklar-design.md` (Durum)

- [ ] **Step 1: Çapraz denetimler** (yalnız bu görevde): `cargo check -j 8 -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -j 8 -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -j 8 -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`. Beklenen düzeltme yerleri: Task 3'ün macOS pano adları (`objc2-app-kit` 0.3 imzaları), X11/Wayland `Backend` yöntemleri, `link.rs`'in `#[cfg(unix)]` kolları. Her düzeltmeden sonra dört komut yeniden; uyarısız olunca devam.

- [ ] **Step 2: Exe.** `cargo build --release -p gezik -j 8`; `stat -c %s target/release/gezik.exe`. Taban 22.665.216, **sınır 22.927.360 bayt** (Global Constraints). Aşılırsa görev görev dağılım ayrı bir çalışma ağacında (`git worktree add ../gezik-7c-measure <commit>`, kendi `target`'ı) ölçülür, notlara yazılır ve daraltma önerisiyle kullanıcıya sorulur (en olası büyükler: Task 7'nin Slint şeridi, Task 8'in sekmeleri ve `ListView`'i, Task 3'ün PNG kodlayıcı yolu).

- [ ] **Step 3: Bellek ve açılış (Windows; yalnız kullanıcı uzaktayken, klavye düzenine dokunmadan):** master `056d860` ayrı çalışma ağacında (`git worktree add ../gezik-master-7c 056d860`, `cargo build --release -p gezik -j 8`); her ikisi için `scripts/perf/measure.ps1 -Runs 5 -Config <boş geçici klasör>` ikişer tur (bellek master'dan büyük değil, açılış ≤ ~60 ms). Kullanıcı bilgisayardaysa bu adım notlarda **bekliyor** olarak kaydedilir.

- [ ] **Step 4: `linux-test.md`** (sıradaki numaralar 50-55, var olan biçimde, İngilizce):
  - 50. **New ▸ and templates.** Right-click empty space: "New ▸" holds Folder, Text file, Markdown file, then the files and folders in `~/.config/gezik/templates/` (put `Report.odt` and a folder `Project/` with a file in it there; "Open templates folder" opens it in a new tab), then "Open templates folder". Each makes its item in the folder shown, numbered `(2)` when the name is taken, and starts renaming it; Ctrl+Z takes it away. Dot files in the folder are not listed.
  - 51. **New folder with selection.** Select three files, Ctrl+Alt+N (and the right-click item "New folder with selection"): they are in "New folder", which is selected and being renamed. Ctrl+Z puts them back and the folder goes to the trash; Ctrl+Y (redo) moves them in again. Note whether the desktop takes Ctrl+Alt+N.
  - 52. **Paste as file.** Copy a picture (a screenshot to the clipboard; an image in Firefox: "Copy Image") and press Ctrl+V on the list: `Pasted image <date> <time>.png` opens in an image viewer. Copy text in a text editor: right-click empty space says "Paste text as file", and Ctrl+V makes `Pasted text … .txt` with that text. Copy files in Nautilus/Dolphin: Ctrl+V pastes the files, not a text file. Repeat on X11 and on Wayland; a large picture (4K screenshot) too.
  - 53. **Links.** Right-click a file ▸ "Create link": `Link to <name>` (a symbolic link, `ls -l`). Drag a file with Ctrl+Shift inside Gezik: a link in the target folder, the label says "Create link in …". Drag from Nautilus/Dolphin with Ctrl+Shift onto Gezik: X11 makes a link; Wayland copies (no link action in the protocol, a known gap). Right-drag ▸ "Create link here". Ctrl+Z on a link to a folder: the link goes to the trash, the folder's files stay.
  - 54. **Drop stack.** Ctrl+Shift+S adds the selection; the strip opens above the status bar. Drag files from Nautilus/Dolphin onto it. Go to another folder: "Copy here" copies them (one Ctrl+Z), "Move here" moves them and they leave the strip. Delete one stacked file in a terminal: it fades and is left out. Drag an item from the strip to another app (it leaves the window). View ▸ Drop stack hides it.
  - 55. **History.** After a few jobs (one failing), the status bar shows "History": each job with its time, newest first; "Show in folder" goes there with the results selected; "Details" lists what failed. View ▸ Operation history opens it while the panel is empty.
  - "Known gaps"e: Wayland'da başka programdan bağlantı bırakma yok.

- [ ] **Step 5: `macos-test.md`** "### 7c, daily" başlığıyla (66-71): (66) New ▸ şablonlarla (`~/Library/Application Support/…/templates/`), Finder'ın `.DS_Store`'u listelenmez; (67) ⌃⌘N ve File ▸ New Folder with Selection, Ctrl+Z/Ctrl+Y; (68) ekran görüntüsü panoya (⌃⇧⌘4) ve Safari'den "Copy Image" → ⌘V `Pasted image … .png`; metin → `Pasted text … .txt`; Finder'da kopyalanmış dosyalar dosya olarak; (69) satır menüsü "Create link" (symlink), ⌘⌥ ile sürükleme (Gezik içinde ve Finder'dan; imleç bağlantı oku), sağ sürüklemede "Create link here", bağlantıyı çöpe atmak hedefe dokunmaz; (70) ⌘⇧S ve Edit ▸ Add to Drop Stack, View ▸ Drop Stack, Copy here / Move here; (71) View ▸ Operation History, Show in folder, Details.

- [ ] **Step 6: Notlar** (`2026-10-03-ayarlar-ve-tema-followups.md`, Türkçe, 7b bölümünün kalıbında): ölçümler (exe: taban, sınır, bayt ve MiB, toplam ölçüte kalan; bellek/açılış ya da "bekliyor"), sapmalar (1-17 ve ek netleştirmeler kısaca), junction testi **Dal A mı Dal B mi** ve neden, Geliştirici Modu'nun bu makinede kapalı olup symlink testlerinin atlandığı (açıksa koştuğu), denetimler (dört komutun ve üç çapraz denetimin sonucu, test sayıları), Windows ekran testleri (aşağıdaki liste: "bekliyor" ya da sonuçlar), denenemeyenler (macOS 66-71, Linux 50-55), açık kalan küçükler (incelemelerden).

- [ ] **Step 7: Spec.** Durum satırı: "Tasarım onaylandı (2026-10-07); 7a, 7b ve 7c uygulandı".

- [ ] **Step 8: Dört komut** son kez; **Commit** "Measure 7c and note what it does".

---

## Ek netleştirmeler (Task 3-8 yazılırken)

18. **Yığın öğesinin ipucu:** Slint'te ipucu yok (7b'nin ipucu yalnız kenar çubuğunda); işaretçi bir yığın öğesinde durunca tam yolu durum çubuğu söyler (`stack-hovered`).
19. **Yığın şeridinin yeri `drop-geometry`'de hesapla:** şerit bir `if` içinde (kapalıyken bellek tutmaz) olduğundan yeri pencere yüksekliğinden çıkarılır: durum çubuğu (`Theme.row-height`) + 1px çizgi + şerit (`Theme.row-height + 6px`). Düzen değişirse bu formül de değişmelidir (Task 7 Step 5'teki not).
20. **`Operations::transfer_job`** `transfer`'in gövdesini taşır ve iş kimliğini döner (yığının taşıdıklarını iş bitince düşürmek için); `transfer`'in imzası değişmez.
21. **Günlük ayrıntıları:** "Skipped:" başlığı da 50 satıra girer (eskiden girmiyordu; spec "en çok 50 satır"). Paneldeki "Details" de aynı ortak metni kullanır.
22. **Günlük sonuç metni** spec'in kısa biçimidir (`Done · 2 skipped`, `3 failed`); işlem satırının kendi metni (`Done · 2 items skipped`, `3 items failed`) değişmez.

---

## Ekran testleri (Windows, alt planın sonunda; yalnız kullanıcı uzaktayken, klavye düzeni değiştirilmeden)

**Durum: bekliyor.** `GEZIK_CONFIG_DIR` boş bir geçici klasör. Test verisi `%TEMP%\gezik-gui-7c\`: `one\` (içinde `a.txt`), `two\`, `a.txt`, `b.txt`, `big.png` (bir ekran görüntüsü), `locked.txt` (testte açık tutulur).

1. Klasör boşluğu menüsü (Explorer menüsü, Gezik'in öğeleri üstte): "New folder", "New file", "New from template ▸" (Markdown file, Open templates folder) ve Explorer'ın kendi "New ▸"'u; "Markdown file" → `New document.md`, ad kutusu açık; ikincisi `New document (2).md`; Ctrl+Z çöpe atar.
2. "Open templates folder" yeni sekmede `…\templates`'i açar; oraya `Report.docx` ve `Project\x.txt` konunca menüde (bir saniye içinde) "Project", "Report"; "Report" → `Report.docx` kopyası, ad kutusu açık; "Project" içiyle kopyalanır.
3. İki dosya seçili sağ tık → "New folder with selection" → `New folder` seçili, adı düzenleniyor; Ctrl+Alt+N aynısı (ABD düzeninde); Ctrl+Z ikisini geri getirir, klasör çöpe; Ctrl+Y yine taşır. "New folder" zaten varken `New folder (2)`.
4. Ekran Alıntısı Aracı ile panoya resim → sağ tık "Paste image as file" → `Pasted image <tarih> <saat>.png`, görüntüleyicide doğru; Notepad'den metin → "Paste text as file", Ctrl+V `Pasted text … .txt` (satır sonları CRLF); Explorer'da kopyalanmış dosyalar → Ctrl+V dosyaları yapıştırır. Tarayıcıdan "Copy image" (resim + HTML) → resim.
5. Dosyaya sağ tık "Create link ▸ Shortcut" → `a.txt - Shortcut.lnk`, çift tık `a.txt`'yi açar; klasöre "Create link ▸ Junction" → `Link to one` içinde `a.txt`; Geliştirici Modu kapalıyken "Symbolic link" yok (açıksa var ve çalışır); Ctrl+Z bağlantıyı Geri Dönüşüm Kutusu'na atar, `one\a.txt` yerinde (Geri Dönüşüm Kutusu'ndaki junction'ı Explorer'dan açmayı denemeden).
6. Explorer menüsünün "Create shortcut"u → `… - Shortcut.lnk` Gezik'in işi olarak (işlem panelinde/History'de görünür), Ctrl+Z geri alır.
7. Gezik içinde Alt ile ve Ctrl+Shift ile sürükleme → etiket "Create link in two", bırakınca `.lnk`; Explorer'dan Alt ile Gezik'e → imleç bağlantı, "Create link in …"; sağ sürüklemede "Create link here".
8. Ctrl+Shift+S → şerit; Explorer'dan şeride bırakılan dosya eklenir (Explorer'da kaynak yerinde kalır); yinelenen eklenmez; `two\`'de "Copy here" (tek Ctrl+Z), "Move here" → şeritten çıkarlar; `locked.txt` açıkken taşınmaz ve şeritte kalır; Explorer'dan silinen öğe soluklaşır; şeritten bir öğeyi listeye ve masaüstüne sürükleme; "Clear".
9. History: durum çubuğunda düğme; kayıtlar en yeni üstte, saat, sonuç ("Done", "1 failed", "Cancelled"); "Show in folder" sonuçları seçer (başka klasördeyken de); "Details" başarısızları listeler; View ▸ Operation history panel boşken açar.
10. Ulaşılabilirlik: Türkçe Q penceresinde (yalnız pencere düzeni zaten Türkçe Q ise; düzen **değiştirilmez**) Ctrl+Shift+S ve Ctrl+Alt+N; yoksa ABD'de denenir ve not düşülür.
11. Exe (≤ 22.927.360), boşta bellek (master'dan büyük değil) ve açılış (≤ ~60 ms), Task 9'un sayılarıyla.

---

## Self-review

- **Spec kapsamı:** §8.1 yerleşikler (Folder/Text file/Markdown file), `templates/` (uzantısız alfabetik etiket, noktalı/gizli atlanır, en çok 50, klasör şablonu içiyle), ilk çalıştırmada oluşturma ve silineni geri getirmeme, `Open templates folder`, `New ▸` (macOS/Linux) / `New from template ▸` (Windows), `CopyTask` (KeepBoth) / `NewTask` (`Markdown`), `After::Rename`, etiketler → Task 1 + 2 + 5 + 6 (sapma 1-5). §8.2 `new-folder-with-selection` (Ctrl+Alt+N, ⌃⌘N, satır menüsü), `GroupTask`, tek geri alma "New folder with 3 items", `inverse::build` kuralı → Task 1 + 5 + 6 (sapma 6, 7). §9.1 Ctrl+V'de resim > metin, adlar (`:` yok, `(2)`), UTF-8, menü etiketleri, Windows'ta Gezik öğesi, `read_image`/`read_text` üç sistem (PNG, `CF_DIBV5`/`CF_DIB` + BMP başlığı, TIFF → PNG, X11/Wayland hedefleri), yalnız biçim varlığı menüde, kodlama işte → Task 2 + 3 + 6 (sapma 8, 9). §9.2 `Create link ▸` / `Create link`, `.lnk` (`IShellLinkW`+`IPersistFile`, `Ad - Shortcut.lnk`), junction (`FSCTL_SET_REPARSE_POINT`, yerel NTFS), symlink izin denemesi, `Link to Ad`, Explorer `link` fiili, bırakma tuşları (üç sistem, dışarıdan gelenler), `Create link here`, `Effect::Link`, `LinkTask` (`Outcome::Created`, `symlink_metadata`), zorunlu çöp testi ve Dal B → Task 2 + 4 + 6 (sapma 10-12). §9.3 şerit, `toggle-stack`, View ▸ Drop stack, kendiliğinden açılma, sürükleyerek ve `add-to-stack` ile ekleme, yinelenmeme, simge + ad + ×, Copy here/Move here/Clear, soluk ve atlanan, taşınanın çıkması, şeritten sürükleme (pencere içi ve dışarı), yalnız yollar, 1000, `Hit::Stack` → Task 7 (sapma 13, 18-20). §9.4 Current/History sekmeleri, durum çubuğu düğmesi, `show-history`, kayıt alanları ve sonuç metinleri, Show in folder, Details (50 satır), en yeni üstte, 200 → Task 8 (sapma 14, 21, 22). §10 dört yeni eylem (63), varsayılanlar ve çakışmasızlık, şablon yorumları, macOS menü çubuğu (File: New Folder with Selection; Edit: Add to Drop Stack; View: Drop Stack, Operation History), ulaşılabilirlik testi → Task 5 + 6 + 7 + 8. §11 dosya yerleri (sapma: `op_history.rs`, arayüz `templates.rs`, kimlikler 1400-1459). §12 birim testleri (şablon/yapıştırma adları, `GroupTask` geri alma, bağlantı tersi, bağlantı tuşu kuralı, `Hit::Stack`, kısayollar), platform testleri (bağlantılar ve çöp, pano biçimleri, `.lnk` geri okuma), Linux/macOS listeleri (Docker yerine, kullanıcı kararı), Windows ekran listesi → Task 1-9. §13 performans (biçim varlığına bakma, kodlama işte, şablon listesi arka planda, symlink denemesi bir kez arka planda, yığın ve günlük ilk kullanımda, exe ve bellek) → Global Constraints + Task 9.
- **Yer tutucu taraması:** Task 5 dört eylemi "Not theirs"e koyar; Task 6 (`NewFolderWithSelection`), Task 7 (`AddToStack`, `ToggleStack`), Task 8 (`ShowHistory`) doldurur (7b'nin Pin kalıbı). Task 2 Dal B koşulludur ve adımları yazılıdır. macOS ve Windows API imzalarında derleyiciye uyulacak noktalar (Task 2 Step 4, Task 3 Step 5) belirtilmiştir; davranış sabittir. Gövdesi yazılı olmayan test yok.
- **Tip tutarlılığı:** Task 1 `GroupTask::new(Vec<PathBuf>, &Path)`, `NewTask::{markdown, with_contents}`, `CopyTask::template(PathBuf, &Path, bool)`, `TaskKind::{NewFolderWith, Link}`, `TrashTask::only_if_empty`. Task 2 `templates::{Template, TEMPLATE_MAX, template_label, template_list, PasteKind, pasted_name, LinkKind::{for_drops}, link_name}`, `link::{create, symlinks_allowed, probe_symlinks, junctions_supported, read_shortcut}`, `LinkTask::{into, beside}`. Task 3 `clipboard::{ClipboardImage::png_bytes, paste_kind, read_image, read_text}`. Task 4 `Effect::Link`, `Keys.link`, `Allowed::ALL`, `DragOs`, `keys_of`, `drop_menu(…, can_link, …)`, `CREATE_LINK_HERE`. Task 5 `Action::{NewFolderWithSelection, AddToStack, ToggleStack, ShowHistory}`, `ConfigStore::{templates_dir, is_template_path}`, `templates::{set_dir, dir, current, read, refresh, load_in_background, open_folder}`, `watch_config(store, on_change, on_templates)`. Task 6 kimlikler 1400-1406, 1410-1459, `background_items(…, paste_as, windows)`, `new_sub`, `link_items`, `junction_offered`, `Operations::{new_markdown, new_from_template, new_folder_with, new_folder_with_selection, paste_as, paste_as_file, create_links}`, `ShellVerb::Link`. Task 7 `Hit::Stack`, `Layout.stack`, `Action::AddToStack`, `stack::{STACK_MAX, STACK_SHOWN, StackItem, StackItems, Stack, with_current, count_text, more_text}`, `StackRow` (Slint), `Drags::stack_down`, `Phase::StackArmed`, `Dragging.from_stack`, `Operations::transfer_job`, `TOGGLE_STACK`, `view_items(view, preview_open, stack_open, options, windows)`. Task 8 `op_history::{History, Record, result_text, show_target, details_text, HISTORY_MAX, SHOW_MAX, MAX_DETAILS}`, `HistoryRow` (Slint), `Navigator::go_selecting`, `Operations::{show_history, history_toggle, choose_tab, history_show, history_details}`, `SHOW_HISTORY`. Task 8 `view_items`'e öğe ekler, imzayı değiştirmez.
- **Review Focus:** 1 → Task 1 `a_selected_item_named_new_folder_goes_inside_the_numbered_one`, `an_item_changed_since_keeps_the_folder_out_of_the_trash`; 2 → Task 2 üç `trashing_a_…_leaves_its_folder` testi; 3 → Task 3 `files_win_then_an_image_then_text`, `a_dib_becomes_a_bmp_file`; 4 → Task 4 `link_keys_differ_by_system`, `a_link_falls_back_when_the_source_forbids_it`; 5 → Task 7 `the_stack_keeps_each_path_once_and_at_most_a_thousand`, `gone_items_are_left_out_and_failed_moves_stay`.
