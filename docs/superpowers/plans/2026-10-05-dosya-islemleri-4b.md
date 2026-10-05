# Dosya İşlemleri 4b (Sürükle-bırak ve Linux Sistem Panosu) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dosyaları Gezik içinde, Gezik'ten dışarı ve dışarıdan Gezik'e sürükleyip bırakmak (Windows, macOS, Linux X11/Wayland) ve Linux'ta sistem panosunu (kopyala/kes/yapıştır, X11 ve Wayland) kullanmak.

**Architecture:** Neyin işaretçinin altında olduğu, bırakmanın ne yapacağı (taşı/kopyala/sabitle/olmaz) ve etiket metni `gezik-core::drag`'de saf işlevlerdir; konum hesabı Rust'ta pencere geometrisiyle yapılır (Slint'in tekrarlanan öğeleri bir işlevle taranamaz, `changed` geri çağrıları ise bir olay döngüsü geç kalır). Slint yalnızca basma/sürükleme/bırakma olaylarını bildirir, geometriyi `out` özellikleriyle verir ve hayaleti/vurguları çizer. Pencere içi sürükleme tamamen Gezik kodudur; işaretçi pencereden çıkınca sürükleme sisteme devredilir (`gezik-platform::dnd`). Dışarıdan gelen bırakmalar aynı hedef mantığını kullanır. Linux'ta pano ve sürükle-bırak aynı arka ucu paylaşır: X11'de kendi `x11rb` bağlantımız, Wayland'da winit'in `wl_display`'i üzerinde kendi olay kuyruğumuz.

**Tech Stack:** Rust 2024, Slint 1.18 (yazılımla çizim, winit 0.30.13), `windows` 0.62 + `windows-core` 0.62 (`#[implement]` ile `IDropTarget`/`IDropSource`), `x11rb` 0.13 ve `wayland-client` 0.31 / `wayland-backend` 0.3 (yalnız Linux), `objc2` 0.6 / `objc2-app-kit` 0.3 (yalnız macOS).

**Spec:** `docs/superpowers/specs/2026-10-04-dosya-islemleri-design.md` (bölüm 8 Linux satırı, bölüm 9, 10.4 `drop-target`, 12.4)

## Spec'ten sapmalar ve netleştirmeler

1. **Konum hesabı Rust'ta.** Spec 9.2'deki "Slint işaretçi olayları" korunur, ama hangi satırın/sekmenin/kenar çubuğu öğesinin altında olunduğunu Rust, Slint'in verdiği geometriyle (`drop-geometry` yapısı, okunduğu anda hesaplanan bağlamalar) bulur. Sürükleme sırasında işaretçiyi tutan satır olayları aldığı için diğer öğelerin `has-hover`'ı çalışmaz; dışarıdan gelen sürüklemede Slint hiç işaretçi olayı almaz. Adres çubuğu parçalarının genişliği metne bağlı olduğundan her parça konumunu `crumb-span` geri çağrısıyla bildirir.
2. **Windows'ta winit'in bırakma hedefi kanca yerine `RevokeDragDrop` ile kaldırılır.** winit `with_drag_and_drop(true)` ile `OleInitialize`'ı zaten yapıyor; pencere oluşunca `RevokeDragDrop(hwnd)` + `RegisterDragDrop(hwnd, Gezik'in IDropTarget'ı)` yeterli ve `BackendSelector` gerektirmez. Kanca (`with_winit_window_attributes_hook`) Slint 1.18'de var; gerekirse yedek yol.
3. **Windows'ta dışarı sürükleme Shell'in kendi veri nesnesi ve hayaletiyle.** Veri nesnesi `IShellFolder::GetUIObjectOf(IDataObject)` (Explorer'ın kullandığı), başlatma `SHDoDragDrop` + Gezik'in `IDropSource`'u: Shell simge + sayı rozetli hayaleti kendisi çizer. Spec'teki "Gezik'in çizdiği hayalet bitmap" (`IDragSourceHelper::InitializeFromBitmap`) yerine bu; pencere içindeki hayalet yine Gezik'in.
4. **Windows'ta `DoDragDrop` sırasında pencere geri dönüşü:** `DoDragDrop` arayüz iş parçacığını bloklar ve winit olayları tamponlar (pencere çizilmez). Bu yüzden işaretçi Gezik penceresine geri girince `IDropSource::QueryContinueDrag` `DRAGDROP_S_CANCEL` döndürür, Gezik `SetCapture` ile işaretçiyi geri alır ve sürükleme Slint'te sürer. Bırakma veya Esc ile biten sistem sürüklemesinden sonra winit tuş bırakmayı hiç görmediği için Gezik pencereye `WM_LBUTTONUP`/`WM_RBUTTONUP` gönderir (Slint'in "basılı" durumu kapanır).
5. **Gezik kaynakken asla silmez.** Hedef "taşıdım" deyip kaynak silmeyi isterse (iyileştirilmemiş taşıma) özgün dosyalar yerinde kalır: veri kaybı yerine fazladan kopya. Explorer ve Shell veri nesnesi kullanan hedefler taşımayı kendileri yapar.
6. **Gezik hedefken taşıma "iyileştirilmiş taşıma"dır:** `Drop` `DROPEFFECT_NONE` döndürür ve veri nesnesine `CFSTR_PERFORMEDDROPEFFECT = DROPEFFECT_NONE`, `CFSTR_LOGICALPERFORMEDDROPEFFECT = DROPEFFECT_MOVE` yazılır (Microsoft "Handling Shell Data Transfer Scenarios"); kaynak silmez, taşımayı Gezik motoru yapar ve Ctrl+Z ile geri alınır. Kopyada `DROPEFFECT_COPY`.
7. **Dışarıdan sağ tuşla bırakma:** `Drop` hemen `DROPEFFECT_NONE` döner, ardından Gezik'in Copy here / Move here / Cancel menüsü açılır (Slint menüleri eşzamanlı değildir). Pencere içi sağ tuşla sürükleme de aynı menüyü kullanır.
8. **Sabitleme bölgesi:** PINNED satırlarının üst/alt çeyreği "araya sabitle", ortası "içine taşı/kopyala". Yalnızca sürüklenenlerin hepsi klasörse (dosya varsa bölge yoktur). PINNED bölümü boşken sürükleyerek sabitleme yoktur (sağ tık "Pin" kalır).
9. **"Yazılamayan hedef" ölçüsü:** arayüz iş parçacığı dosya sistemine dokunmaz; "olmaz" yalnızca kendi üstüne, kendi alt klasörüne, aynı klasöre taşıma, "This PC" ve optik sürücüler için gösterilir. Salt okunur klasöre bırakma motorun hata satırıyla biter.
10. **Aynı disk kararı dosya sistemine dokunmadan:** yolun bilinen sürücüler/bağlama noktaları listesindeki en uzun kökü (Unix'te `/proc/self/mounts`'tan gelen liste) karşılaştırılır; listede yoksa metinsel kök (`lexical_roots`).
11. **Değiştirici tuşlar:** basılı Shift/Ctrl (macOS'ta ⌥) işaretçi olaylarından okunur; tuşa basılıp bırakıldığında da (`capture-key-released`) etiket yenilenir. İkisi birden basılıysa varsayılan etki (bağlantı oluşturma kapsam dışı).
12. **Wayland'da sistem panosu `wl_data_device` ile** (spec'teki "yoksa" yolu ana yol olur): `data-control` protokolü GNOME'da yok, `wl_data_device` her bileşikte var ve sürükle-bırakla aynı bağlantıyı kullanır. Pano yalnızca Gezik odaklıyken okunur/yazılır (Ctrl+C/V ve menüler zaten öyle); seçim değişince sayaç artar.
13. **X11'de pano kendi `x11rb` bağlantımızla;** XDND hedefi `XdndProxy` ile kendi gizli penceremize yönlendirilir (winit'in penceresine gönderilen istemci iletileri yalnızca winit'in bağlantısına gider). X11'de dışarı sürükleme sistem döngüsü değildir: winit'in örtük işaretçi tutması sürer, Slint pencere dışındaki konumları almaya devam eder ve Gezik XDND iletilerini kendisi gönderir.
14. **Linux denemesi Docker'da:** bu makinede WSL dağıtımı yok ama Docker Desktop var. Linux derlemesi (`cargo build`), X11 panosu (Xvfb + `xclip`) ve mümkünse Wayland panosu (başsız `sway` + `wl-clipboard`) bir kapta denenir. macOS yalnızca `cargo check --target aarch64-apple-darwin` ile derlenir.
15. **Sekme üstünde bekleme ve kenarda kaydırma Rust zamanlayıcılarıyla** (600 ms, 30 ms); dışarıdan gelen sürüklemelerde de çalışır.
16. **Kesilmiş (soluk) öğe dışarı sürüklenirse** pano değişmez; bırakma ayrı bir iştir.

## Global Constraints

- Rust edition 2024, stable. Komutlar `D:\Work\gezik` içinden; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken; uygulamayı elle çalıştırırken `GEZIK_CONFIG_DIR` geçici klasöre.
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- `cargo check -p gezik --target aarch64-apple-darwin` ve `cargo check -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu` her platform görevinden sonra geçer (Linux'ta `gezik` crate'i yalnızca Docker'da derlenir: fontconfig çapraz derlemede yok).
- Slint yalnızca yazılımla çizim; Slint özellik listesine ekleme yok. Slint'in deneysel `DragArea`/`DropArea`'sı kullanılmaz.
- Yeni bağımlılıklar tam olarak: `gezik-platform` → Linux'ta `x11rb = { version = "0.13", features = ["xfixes"] }`, `wayland-client = "0.31"`, `wayland-backend = { version = "0.3", features = ["client_system"] }`; Windows özelliklerine gerekirse `Win32_System_SystemServices` (MK_* sabitleri); macOS'ta `objc2-app-kit` özelliklerine `NSDragging`, `NSDraggingItem`, `NSDraggingSession`, `NSView`, `NSResponder`, `NSEvent`, `NSApplication`, `NSImage`, `NSWorkspace`, `NSGraphics`; `objc2-foundation`'a `NSGeometry`. Başka crate yok.
- Arayüz iş parçacığı dosya sistemine dokunmaz. Her bırakma `gezik-ops::Engine` işi olur, Ctrl+Z ile geri alınır (sabitleme hariç: o ayar değişikliğidir).
- Hiçbir dosya kullanıcı seçmeden ezilmez (motorun mevcut çakışma listesi).
- Arayüz metinleri İngilizce: "Move to {folder}", "Copy to {folder}", "Pin to sidebar", menü "Copy here", "Move here", "Cancel".
- Sınırlar: eşik 4 mantıksal px (her eksende); sekme beklemesi 600 ms; kenar bölgesi 24 px, adım satır yüksekliğinin yarısı, 30 ms'de bir.
- Performans: boşta bellek ≤ 7 MB ve açılış ≤ ~60 ms korunur; sürükleme sırasında kare süresi kaydırmadakinden kötü değil.

## Review Focus

1. **Bir klasörü kendi içine veya kendi alt klasörüne sürüklemek** (listede, kenar çubuğunda, adres çubuğunda) hiçbir iş başlatmamalı ve "olmaz" göstermeli. Test: Task 1 `refuses_onto_itself_and_into_its_own_subfolder`.
2. **Sürüklemeden bırakılan tıklama** çoklu seçimi tek öğeye indirmeli, sürüklenen bırakma indirmemeli; Ctrl+tık seçili öğeyi bırakınca çıkarmalı. Test: Task 2 `pressing_a_selected_entry_waits_for_the_release`.
3. **Kaydırılmış liste ve ızgarada** doğru öğe hedeflenmeli; ızgara döşemeleri arasındaki boşluk ve son satırın boş hücreleri klasörün kendisidir. Test: Task 1 `grid_gaps_and_empty_cells_are_the_background`, `scrolled_list_hits_the_right_entry`.
4. **Explorer'dan taşıma** dosyayı bir kez taşımalı (Explorer silmemeli, Gezik taşımalı) ve Ctrl+Z geri getirmeli; Gezik'ten Explorer'a taşımada Gezik hiçbir şeyi silmemeli. Test: Task 5/6 elle denemeleri (çift kopya veya kayıp yok), `move_drop_reports_an_optimized_move`.
5. **Türkçe/boşluklu/özel karakterli adlar** Linux URI listelerinde ve `gnome-copied-files`'ta doğru gidip gelmeli (`%20`, `ş`, `#`, `%`). Test: Task 7 `file_uris_round_trip_unicode_and_reserved_characters`.

---

## Dosya yapısı

```
crates/gezik-core/src/lib.rs                     pub mod drag
crates/gezik-core/src/layout.rs                  Rect::contains
crates/gezik-core/src/drag.rs                    YENİ — eşik, Layout/Hit, hit(), Effect/Keys, choose(), refuse(), same_drive(), label(), edge_scroll()
crates/gezik-config/src/theme.rs                 drop-target rengi
crates/gezik-config/themes/{dark,light}.toml     drop-target
crates/gezik/ui/theme.slint                      drop-target
crates/gezik/src/theme_bridge.rs                 drop-target
crates/gezik/ui/app.slint                        sürükleme özellikleri/geri çağrıları, drop-geometry, hayalet
crates/gezik/ui/widgets/file-view.slint          item-down/item-drag/item-up, drop-entry vurgusu
crates/gezik/ui/widgets/sidebar.slint            scroll takma adı, drop-row ve sabitleme çizgisi
crates/gezik/ui/widgets/tab-bar.slint            scroll/tab-width dışa açık, drop-tab vurgusu
crates/gezik/ui/widgets/breadcrumb.slint         crumb-span, drop-crumb vurgusu
crates/gezik/src/drag.rs                         YENİ — Drags: durum makinesi, hedef, bırakma, zamanlayıcılar, dış bırakma işleyicisi
crates/gezik/src/view/mod.rs                     ertelenen seçim daralması (press/release)
crates/gezik/src/operations.rs                   transfer(paths, dir, effect)
crates/gezik/src/sidebar.rs                      pin_at(paths, position)
crates/gezik/src/main.rs                         bağlantılar, Esc, attach
crates/gezik-platform/Cargo.toml                 Linux/macOS bağımlılıkları
crates/gezik-platform/src/lib.rs                 pub mod dnd; linux modülü
crates/gezik-platform/src/dnd/mod.rs             YENİ — DropHandler, Offer, DragEnd, Attached, OutsideDrag, attach()
crates/gezik-platform/src/dnd/windows.rs         YENİ — IDropTarget, IDropSource, SHDoDragDrop
crates/gezik-platform/src/dnd/macos.rs           YENİ — NSDraggingSource, WinitView yöntem değişimi
crates/gezik-platform/src/linux/mod.rs           YENİ — arka uç seçimi (X11/Wayland), pano yönlendirmesi
crates/gezik-platform/src/linux/uri.rs           YENİ — file URI, text/uri-list, gnome-copied-files, kde-cutselection (her yerde derlenir)
crates/gezik-platform/src/linux/x11.rs           YENİ — seçim sahipliği, XFixes sayacı, XDND kaynak/hedef
crates/gezik-platform/src/linux/wayland.rs       YENİ — paylaşılan wl_display, wl_data_device pano + DnD
crates/gezik-platform/src/clipboard.rs           Linux imp → linux::*; Windows'ta HDROP okuma ortak yardımcıya
scripts/linux/Dockerfile, scripts/linux/test.sh  YENİ — Docker'da Linux derleme ve pano denemesi
docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md   4b sonuçları, denenmeyenler
README.md                                        sürükle-bırak, Linux panosu
```

---

### Task 1: `gezik-core::drag` — hedef, etki, ret, etiket

**Files:**
- Create: `crates/gezik-core/src/drag.rs`
- Modify: `crates/gezik-core/src/lib.rs` (`pub mod drag;`), `crates/gezik-core/src/layout.rs` (`Rect::contains`)

**Interfaces:**
- Produces:
  - `pub const THRESHOLD: f32 = 4.0; pub const TAB_HOVER: Duration; pub const EDGE: f32 = 24.0;`
  - `pub fn past_threshold(dx: f32, dy: f32) -> bool`
  - `pub struct ListArea { pub rect: Rect, pub scroll: f32, pub geometry: Geometry, pub count: usize }` (`rect`: listenin görünen alanı, pencere koordinatları; `scroll` ≤ 0)
  - `pub enum SideRow { Header, Item, Pinned(usize) }`
  - `pub struct SidebarArea { pub rect: Rect, pub scroll: f32, pub pad: f32, pub row_height: f32, pub rows: Vec<SideRow> }`
  - `pub struct TabArea { pub rect: Rect, pub scroll: f32, pub tab_width: f32, pub count: usize }`
  - `pub struct CrumbArea { pub rect: Rect, pub spans: Vec<(f32, f32)> }` (x, genişlik)
  - `pub struct Layout { pub width: f32, pub height: f32, pub list: ListArea, pub sidebar: Option<SidebarArea>, pub tabs: TabArea, pub crumbs: CrumbArea }`
  - `pub enum Hit { Outside, Nothing, Entry(usize), Background, Sidebar(usize), PinAt(usize), Tab(usize), Crumb(usize) }`
  - `pub fn hit(layout: &Layout, x: f32, y: f32, pin_zones: bool) -> Hit`
  - `pub enum Effect { Copy, Move }`, `pub struct Keys { pub shift: bool, pub copy: bool }`, `pub struct Allowed { pub copy: bool, pub move_: bool }` (`Allowed::BOTH`)
  - `pub fn choose(keys: Keys, same_drive: bool, allowed: Allowed) -> Option<Effect>`
  - `pub fn refuse(sources: &[PathBuf], target: &Path, effect: Effect) -> bool`
  - `pub fn same_drive(a: &Path, b: &Path, roots: &[PathBuf]) -> bool`
  - `pub enum Action { Transfer(Effect), Pin }`, `pub fn label(action: Action, folder: &Path) -> String`, `pub fn folder_name(path: &Path) -> String`
  - `pub fn edge_scroll(y_in_list: f32, visible: f32, step: f32) -> f32`

- [ ] **Step 1: Testleri yaz** (`drag.rs` altında `#[cfg(test)] mod tests`):

```rust
fn list(count: usize) -> ListArea {
    ListArea { rect: Rect { x: 200.0, y: 100.0, width: 600.0, height: 400.0 }, scroll: 0.0,
               geometry: Geometry::List { row_height: 20.0 }, count }
}
fn layout(list: ListArea) -> Layout {
    Layout { width: 1000.0, height: 700.0, list,
        sidebar: Some(SidebarArea { rect: Rect { x: 0.0, y: 100.0, width: 195.0, height: 400.0 }, scroll: 0.0, pad: 4.0,
            row_height: 20.0, rows: vec![SideRow::Header, SideRow::Item, SideRow::Header, SideRow::Pinned(0), SideRow::Pinned(1)] }),
        tabs: TabArea { rect: Rect { x: 0.0, y: 0.0, width: 960.0, height: 30.0 }, scroll: 0.0, tab_width: 100.0, count: 3 },
        crumbs: CrumbArea { rect: Rect { x: 300.0, y: 40.0, width: 500.0, height: 24.0 }, spans: vec![(300.0, 60.0), (372.0, 40.0)] } }
}

#[test]
fn threshold_is_four_pixels_on_either_axis() {
    assert!(!past_threshold(4.0, -4.0));
    assert!(past_threshold(4.5, 0.0));
    assert!(past_threshold(0.0, -5.0));
}

#[test]
fn list_rows_and_the_space_below_them() {
    let l = layout(list(3));
    assert_eq!(hit(&l, 300.0, 105.0, false), Hit::Entry(0));
    assert_eq!(hit(&l, 300.0, 145.0, false), Hit::Entry(2));
    assert_eq!(hit(&l, 300.0, 170.0, false), Hit::Background);
}

#[test]
fn scrolled_list_hits_the_right_entry() {
    let mut a = list(100);
    a.scroll = -200.0; // ten rows up
    assert_eq!(hit(&layout(a), 300.0, 105.0, false), Hit::Entry(10));
}

#[test]
fn grid_gaps_and_empty_cells_are_the_background() {
    let a = ListArea { geometry: Geometry::Grid { cell_width: 100.0, cell_height: 120.0, columns: 5 }, ..list(7) };
    let l = layout(a);
    assert_eq!(hit(&l, 250.0, 150.0, false), Hit::Entry(0));
    assert_eq!(hit(&l, 202.0, 150.0, false), Hit::Background, "inside the 4px inset");
    assert_eq!(hit(&l, 350.0, 250.0, false), Hit::Entry(6), "second line, second column");
    assert_eq!(hit(&l, 650.0, 250.0, false), Hit::Background, "no 8th cell");
    assert_eq!(hit(&l, 760.0, 150.0, false), Hit::Background, "right of the last column");
}

#[test]
fn sidebar_items_headers_and_pin_zones() {
    let l = layout(list(0));
    assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 10.0, true), Hit::Nothing, "header");
    assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 30.0, true), Hit::Sidebar(1));
    assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 61.0, true), Hit::PinAt(0), "top quarter of the first pin");
    assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 70.0, true), Hit::Sidebar(3));
    assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 99.0, true), Hit::PinAt(2), "bottom quarter of the last pin");
    assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 61.0, false), Hit::Sidebar(3), "files dragged: no pin zones");
}

#[test]
fn tabs_crumbs_and_outside() {
    let l = layout(list(0));
    assert_eq!(hit(&l, 150.0, 10.0, false), Hit::Tab(1));
    assert_eq!(hit(&l, 350.0, 10.0, false), Hit::Nothing, "past the last tab");
    assert_eq!(hit(&l, 380.0, 50.0, false), Hit::Crumb(1));
    assert_eq!(hit(&l, 365.0, 50.0, false), Hit::Nothing, "between parts");
    assert_eq!(hit(&l, -1.0, 50.0, false), Hit::Outside);
    assert_eq!(hit(&l, 500.0, 700.0, false), Hit::Outside);
}

#[test]
fn effect_follows_the_drive_and_the_keys() {
    let none = Keys { shift: false, copy: false };
    assert_eq!(choose(none, true, Allowed::BOTH), Some(Effect::Move));
    assert_eq!(choose(none, false, Allowed::BOTH), Some(Effect::Copy));
    assert_eq!(choose(Keys { shift: true, copy: false }, false, Allowed::BOTH), Some(Effect::Move));
    assert_eq!(choose(Keys { shift: false, copy: true }, true, Allowed::BOTH), Some(Effect::Copy));
    assert_eq!(choose(Keys { shift: true, copy: true }, true, Allowed::BOTH), Some(Effect::Move), "both: default");
    assert_eq!(choose(none, true, Allowed { copy: true, move_: false }), Some(Effect::Copy));
    assert_eq!(choose(none, true, Allowed { copy: false, move_: false }), None);
}

#[test]
fn refuses_onto_itself_and_into_its_own_subfolder() {
    let a = PathBuf::from(if cfg!(windows) { r"C:\w\a" } else { "/w/a" });
    let w = a.parent().unwrap().to_path_buf();
    assert!(refuse(&[a.clone()], &a, Effect::Copy));
    assert!(refuse(&[a.clone()], &a.join("sub"), Effect::Move));
    assert!(refuse(&[a.clone()], &w, Effect::Move), "moving into its own folder does nothing");
    assert!(!refuse(&[a.clone()], &w, Effect::Copy), "copying there makes 'a (2)'");
    assert!(!refuse(&[a.clone()], &w.join("b"), Effect::Move));
}

#[test]
fn drives_are_matched_by_their_longest_root() {
    let roots = if cfg!(windows) { vec![PathBuf::from(r"C:\"), PathBuf::from(r"D:\")] }
                else { vec![PathBuf::from("/"), PathBuf::from("/media/usb")] };
    let (a, b, c) = if cfg!(windows) { (r"C:\x", r"c:\y\z", r"D:\q") } else { ("/home/x", "/tmp/y", "/media/usb/q") };
    assert!(same_drive(Path::new(a), Path::new(b), &roots));
    assert!(!same_drive(Path::new(a), Path::new(c), &roots));
}

#[test]
fn labels_name_the_folder() {
    let p = PathBuf::from(if cfg!(windows) { r"C:\Users\Belgeler" } else { "/home/Belgeler" });
    assert_eq!(label(Action::Transfer(Effect::Move), &p), "Move to Belgeler");
    assert_eq!(label(Action::Pin, &p), "Pin to sidebar");
    let root = PathBuf::from(if cfg!(windows) { r"D:\" } else { "/" });
    assert_eq!(label(Action::Transfer(Effect::Copy), &root), format!("Copy to {}", root.display()));
}

#[test]
fn edges_scroll_the_list() {
    assert_eq!(edge_scroll(10.0, 400.0, 10.0), 10.0);
    assert_eq!(edge_scroll(390.0, 400.0, 10.0), -10.0);
    assert_eq!(edge_scroll(200.0, 400.0, 10.0), 0.0);
    assert_eq!(edge_scroll(-30.0, 400.0, 10.0), 10.0, "above the list still scrolls up");
}
```

- [ ] **Step 2:** `cargo test -p gezik-core drag` → derleme hatası (modül yok).
- [ ] **Step 3:** `drag.rs`'i yaz. Kurallar:
  - `hit`: önce pencere dışı → `Outside`; sonra sekmeler, adres parçaları, kenar çubuğu, liste; hiçbiri değilse `Nothing`.
  - Liste: `cy = y - rect.y - scroll`, `line = floor(cy / row_height)`. Listede `line < count` → `Entry(line)`, değilse `Background`. Izgarada `col = floor((x - rect.x) / cell_width)`; `col >= columns` veya `index >= count` → `Background`; hücre içi 4 px kenar payı dışındaysa `Background`.
  - Kenar çubuğu: `cy = y - rect.y - scroll - pad`, `row = floor(cy / row_height)`; `Header` → `Nothing`; `Pinned(i)` ve `pin_zones`: kesir < 0,25 → `PinAt(i)`, > 0,75 → `PinAt(i + 1)`; aksi `Sidebar(row)`.
  - Sekmeler: `floor((x - rect.x - scroll) / tab_width) < count` → `Tab`.
  - `refuse`: herhangi bir kaynak `same_path(target)` veya `is_within(target, source)`; taşımada tüm kaynakların ebeveyni hedefse.
  - `same_drive`: köklerden `is_within` olan en uzunu; ikisi de bulunursa `same_path` karşılaştır, değilse `lexical_roots` eşitliği.
  - `edge_scroll`: `y < EDGE` → `+step`, `y > visible - EDGE` → `-step`.
- [ ] **Step 4:** `cargo test -p gezik-core` geçer; clippy, fmt.
- [ ] **Step 5: Commit** — `Add the pure rules for dragging files: what is under the pointer and what a drop does`

### Task 2: Çoklu seçimde basma, bırakınca daralma

**Files:** Modify `crates/gezik/src/view/mod.rs`, `crates/gezik/ui/widgets/file-view.slint`, `crates/gezik/ui/app.slint`, `crates/gezik/src/main.rs`; notlar dosyasındaki madde.

**Interfaces:**
- Produces: `View::press(index, ctrl, shift)` seçili öğede (Shift'siz) değişiklik yapmaz, `pending: Option<(usize, bool /*ctrl*/)>` kaydeder; `View::release(index, dragged: bool)` bekleyeni uygular (sürüklenmediyse `select_only` / `toggle`), her durumda temizler. Slint: `callback item-released(int)` sol tuş bırakıldığında (satır ve döşeme).
- Consumes (Task 4): sürükleme başlarsa `Drags` `view.release(index, true)` çağırır.

- [ ] **Step 1: Test** (`view/mod.rs` testlerinde ya da saf yardımcıda): `pressing_a_selected_entry_waits_for_the_release` — üç öğe seçiliyken 1'e düz basış seçimi değiştirmez; `release(1, false)` sonrası yalnız 1 seçili; `release(1, true)` sonrası üçü de seçili; Ctrl+basış+bırakış 1'i çıkarır; seçili olmayan öğeye basış hemen seçer. Saf kısım `gezik-core::selection`'a `PendingPress` olarak konur ki birim testi olsun:

```rust
#[test]
fn pressing_a_selected_entry_waits_for_the_release() {
    let mut s = Selection::new(5);
    s.select_only(0); s.extend_to(2, false);
    let mut pending = PendingPress::default();
    assert!(pending.press(&mut s, 1, false, false).is_empty(), "nothing changes yet");
    assert_eq!(s.count(), 3);
    pending.release(&mut s, 1, true);
    assert_eq!(s.count(), 3, "a drag keeps the selection");
    pending.press(&mut s, 1, false, false);
    pending.release(&mut s, 1, false);
    assert_eq!(s.iter().collect::<Vec<_>>(), vec![1]);
    s.select_all();
    pending.press(&mut s, 3, true, false);
    pending.release(&mut s, 3, false);
    assert!(!s.is_selected(3) && s.count() == 4, "ctrl+click on a selected entry drops it on release");
    pending.press(&mut s, 3, false, false);
    assert_eq!(s.iter().collect::<Vec<_>>(), vec![3], "an unselected entry is selected at once");
}
```
- [ ] **Step 2–4:** uygula (`PendingPress { index: Option<(usize, bool)> }`, `press` → `Vec<Range<usize>>` değişiklik, `release`), `View` bunu kullanır; Slint satır/döşeme `pointer-event` up + sol → `root.item-released(index)`. Test geçer.
- [ ] **Step 5:** notlardaki "Seçili bir öğeye basmak…" maddesini kapat. Commit — `Narrow a multiple selection on release, not on press, so it can be dragged`

### Task 3: Tema rengi, geometri ve sürükleme arayüzü (Slint)

**Files:** `theme.rs`, `themes/*.toml`, `theme.slint`, `theme_bridge.rs`, `app.slint`, `file-view.slint`, `sidebar.slint`, `tab-bar.slint`, `breadcrumb.slint`.

**Interfaces (Produces, Task 4 kullanır):**
- `Theme.drop-target` (koyu tema `#3b6fd94d`? — mevcut `selection`'dan ayırt edilen, kenarlıklı vurgu: arka plan `drop-target`, 1 px `accent` kenar).
- `AppWindow`:
  - `in property <bool> drag-active; in property <length> drag-x, drag-y; in property <image> drag-icon; in property <bool> drag-has-icon; in property <int> drag-kind; in property <int> drag-count; in property <string> drag-label; in property <bool> drag-forbidden;`
  - `in property <int> drop-entry: -1; drop-sidebar-row: -1; drop-pin-row: -1; drop-tab: -1; drop-crumb: -1;`
  - `out property <DropGeometry> drop-geometry` — `struct DropGeometry { view-x, view-y, view-width, view-height, list-top, list-width, list-height, sidebar-scroll, tab-scroll, tab-width, tab-strip-width, tab-height, address-y, address-height: length }` (bağlamalar: `file-view.absolute-position`, `file-view.list-top/visible`, `root.sidebar-scroll`, `tab-bar.scroll`, `tab-bar.tab-width`, `address.absolute-position`).
  - `in-out property <length> sidebar-scroll` (her iki kenar çubuğu örneğinde `scroll <=> root.sidebar-scroll`).
  - Geri çağrılar: `item-down(int, length, length, bool right, bool can-drag)`, `item-drag(length, length, bool shift, bool ctrl, bool alt)`, `item-up(int, length, length, bool right) -> bool`, `item-cancel()`, `crumb-span(int, length, length)`, `drag-keys(bool shift, bool ctrl, bool alt)`.
- `FileView`: satır/döşeme `pointer-event`: sol veya sağ `down` → `item-down` (pencere koordinatı; `can-drag` = ızgara veya `x <= name-end`); düğme basılıyken `move` → `item-drag`; `up` → `item-up` (true dönerse sağ tıkta satır menüsü açılmaz); `cancel` → `item-cancel`. `drop-entry == index` iken `drop-target` arka planı ve `accent` kenarı. `out property <length> list-top: root.header-height`.
- `Sidebar`: `in-out property <length> scroll <=> scroller.content-y`; `in property <int> drop-row: -1; in property <int> pin-line: -1` (satır indeksi; çizgi o satırın üstüne).
- `TabBar`: `out property <length> scroll: strip.content-x; out property <length> tab-width-out: root.tab-width; out property <length> strip-width: strip.width;` `in property <int> drop-tab: -1`.
- `Breadcrumb`: her parça `init` ve `changed` (mutlak x, genişlik) ile `crumb-span(i, x, w)`; `in property <int> drop-crumb: -1`.
- Hayalet: kök düzeyde, `drag-active` iken `drag-x + 14px, drag-y + 14px`'te simge (FileIcon), sayı rozeti (`drag-count > 1`), altında etiket kutusu (`drag-label`, boşsa gizli). `drag-forbidden` iken simge 0,5 saydam; satır `TouchArea`'larının `mouse-cursor`'ı `drag-forbidden ? not-allowed : default` (sürükleme yokken değişmez).
- `keys` FocusScope'a `capture-key-released` → `drag-keys(...)`; `capture-key-pressed`'de de `drag-active` iken `drag-keys`.

- [ ] Uygula; `cargo build` (Slint derlemesi) ve mevcut testler geçer; görsel doğrulama Task 4 ile.
- [ ] Commit — `Show dragged files and drop targets, and give Rust the window geometry to find them`

### Task 4: Pencere içi sürükle-bırak (`crates/gezik/src/drag.rs`)

**Files:** Create `crates/gezik/src/drag.rs`; Modify `main.rs`, `operations.rs` (`transfer`), `sidebar.rs` (`pin_at`), `navigation.rs` (sekme konumu: `tab_location(i)`).

**Interfaces:**
- Consumes: Task 1 (`drag::*`), Task 2 (`View::release`), Task 3 (Slint).
- Produces:
  - `Operations::transfer(&self, paths: Vec<PathBuf>, dir: PathBuf, effect: Effect)` — `paste`'in iş/yeniden deneme kurgusu buraya taşınır, `paste` bunu çağırır.
  - `Sidebar::pin_at(&self, paths: &[PathBuf], position: usize)` — her yol `pin_entry`, sonra `move_entry` ile `position`'a.
  - `Navigator::tab_location(&self, index: usize) -> Option<Location>`.
  - `pub struct Drags(Rc<Inner>)` — `new(window, nav, view, sidebar, ops)`, `install()`, `is_active() -> bool`, `cancel()`, `drop_handler() -> Rc<dyn gezik_platform::dnd::DropHandler>` (Task 5'te), `with_current`.
- Durumlar: `Idle → Armed { index, start, right, can_drag } → Inside(Drag) → (Outside, Task 6) → Idle`. `Drag { sources, all_dirs, right, keys, at, target: Option<Target> }`; `Target { hit, dir: Option<PathBuf>, pin: Option<usize>, action: Option<Action> }` (`action: None` = olmaz).
- Hedef yolları: `Entry(i)` → `view.entry_path(i)` klasörse o, değilse `Background`; `Background` → `view.folder()` (`Drives`'ta yok → olmaz); `Sidebar(row)` → satırın `section/index`'i → `sidebar.location_of`; `PinAt(p)` → `Pin`; `Tab(i)` → `nav.tab_location(i)`; `Crumb(i)` → `crumbs(active, MAX_CRUMBS)[i]`. Optik sürücü ve `Drives` → olmaz.
- Etki: `choose(keys, same_drive(sources[0], dir, &drive_roots), Allowed::BOTH)`; `refuse` → olmaz. Etiket `label(action, dir)`.
- Bırakma: sol tuş → `Transfer(e)` için `ops.transfer`, `Pin` için `sidebar.pin_at`; sağ tuş → menü (`MENU_COPY_HERE = 900`, `MENU_MOVE_HERE = 901`, `MENU_CANCEL = 902`; `context_menu.rs`'teki `menu-activated` yönlendirmesi bu kimlikleri `Drags`'e verir).
- Zamanlayıcılar: sekme üstünde 600 ms → `nav.activate_tab(i)` ve hedef yeniden hesaplanır; kenar kaydırma 30 ms (`edge_scroll`, liste içeriği sınırı içinde `list-scroll`), her adımda hedef yeniden hesaplanır.
- Esc (`handle_key`'in başında): `drags.is_active()` → `cancel()`, `true`.

- [ ] **Step 1: Test** (`drag.rs` içinde Slint'siz saf yardımcılar için): `target_dir_rules` — klasör satırı → o klasör, dosya satırı → bulunulan klasör, `Drives` → yok; `menu_ids_do_not_clash` — 900–902 `context_menu.rs` kimlikleriyle çakışmaz.
- [ ] **Step 2–4:** uygula; `cargo test`, ardından elle: listede/ızgarada klasöre, boşluğa, kenar çubuğuna, adres parçasına, sekmeye (bekleyince geçiş), PINNED araya (sabitleme), Shift/Ctrl, sağ tuş menüsü, Esc, kendi üstüne "olmaz", kenarda kaydırma, Ctrl+Z.
- [ ] **Step 5:** Commit — `Drag files inside the window: onto folders, the sidebar, the address bar and tabs`

### Task 5: Windows — Explorer'dan Gezik'e bırakma

**Files:** Create `crates/gezik-platform/src/dnd/mod.rs`, `crates/gezik-platform/src/dnd/windows.rs`; Modify `lib.rs`, `clipboard.rs` (HDROP okuma `pub(crate) fn hdrop_paths`), `crates/gezik/src/drag.rs`, `main.rs`.

**Interfaces:**
- `dnd/mod.rs`:

```rust
pub use gezik_core::drag::{Allowed, Effect, Keys};

/// What is being dragged over Gezik's window.
pub struct Offer {
    pub paths: Vec<PathBuf>,
    pub allowed: Allowed,
    /// The right button is held (a menu follows the drop).
    pub right: bool,
}

/// Gezik's side of a drop target. Called on the UI thread; `x`, `y` are physical pixels in
/// the window's client area.
pub trait DropHandler {
    fn over(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Option<Effect>;
    fn leave(&self);
    /// Returns what was done; a move is done by Gezik itself (the source must not delete).
    fn dropped(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Option<Effect>;
}

pub struct Attached { /* platform state */ }
pub fn attach(window: &(impl HasWindowHandle + HasDisplayHandle), handler: Rc<dyn DropHandler>,
              wake: Arc<dyn Fn() + Send + Sync>) -> Option<Attached>;
impl Attached {
    /// Linux: hands queued events to the handler (call from `wake`, on the UI thread).
    pub fn poll(&self);
}
```
- `windows.rs`: `#[implement(IDropTarget)] struct Target { hwnd, handler, offer: RefCell<Option<Offer>>, helper: Option<IDropTargetHelper> }`. `DragEnter`: `CF_HDROP` yoksa `DROPEFFECT_NONE`; yolları oku, `allowed` = gelen `pdwEffect`; `right` = `MK_RBUTTON`; `keys` = `MK_SHIFT`/`MK_CONTROL`; nokta `ScreenToClient`; yardımcıya ilet. `Drop`: `right` ise işleyicinin `dropped`'ı menüyü açar ve `None` döner; `Move` → `*pdwEffect = DROPEFFECT_NONE` + `SetData(PERFORMEDDROPEFFECT = NONE)`, `SetData(LOGICALPERFORMEDDROPEFFECT = MOVE)`; `Copy` → `DROPEFFECT_COPY`. `attach`: `RevokeDragDrop` + `RegisterDragDrop`; `Attached` düşerken `RevokeDragDrop`.
- Uygulama: `Drags` dış teklif için aynı hedef mantığını kullanır (kaynaklar = teklif yolları; hayalet yok, vurgular var); `attach`, pencere tutamacı oluşunca (`keep_on_screen` gibi yeniden denemeli zamanlayıcı) çağrılır.

- [ ] **Step 1: Test:** `move_drop_reports_an_optimized_move` — `effects_after_drop(Some(Effect::Move)) == (DROPEFFECT_NONE, Some(DROPEFFECT_NONE), Some(DROPEFFECT_MOVE))`, kopya için `(DROPEFFECT_COPY, None, None)` (saf yardımcı `effects_after_drop`). `#[ignore]` elle: `drop_target_registers` (gizli pencereye kaydolur/kaldırılır).
- [ ] **Step 2–4:** uygula; elle: Explorer'dan klasöre/boşluğa/sekmeye/kenar çubuğuna; aynı diskte taşıma (Explorer'da kaynak kalmaz, ikinci kopya yok), farklı diskte kopya, Shift/Ctrl, sağ tuşla bırakma menüsü, Ctrl+Z.
- [ ] **Step 5:** Commit — `Take files dropped from Explorer, with Gezik's own engine and undo`

### Task 6: Windows — Gezik'ten dışarı sürükleme

**Files:** Modify `dnd/mod.rs`, `dnd/windows.rs`, `crates/gezik/src/drag.rs`.

**Interfaces:**

```rust
pub enum DragEnd {
    Dropped,
    Cancelled,
    /// The pointer came back into Gezik's window: the drag goes on there.
    Returned,
}

/// A drag outside the window that Gezik drives itself (X11); elsewhere the system drives it.
pub trait OutsideDrag {
    /// The pointer moved (physical client pixels, may be outside the window).
    fn moved(&mut self, x: f64, y: f64, keys: Keys);
    fn released(&mut self);
    fn cancel(&mut self);
}

pub enum Handoff {
    /// The system ran the drag to its end (Windows: `DoDragDrop` blocks).
    Ended(DragEnd),
    /// Running; `on_end` is called when it ends.
    Running(Box<dyn OutsideDrag>),
}

impl Attached {
    pub fn drag_out(&self, paths: &[PathBuf], right: bool, on_end: Box<dyn FnOnce(DragEnd)>) -> Result<Handoff, String>;
}
```
- `windows.rs`: veri nesnesi `IShellFolder(parent).GetUIObjectOf(hwnd, children, IDataObject)` (`shell_menu.rs`'teki `bind_folder`/`shared_parent` yardımcıları `pub(crate)` olur); `#[implement(IDropSource)] Source { hwnd, right, returned: Cell<bool> }`: Esc → `DRAGDROP_S_CANCEL`; düğme bırakıldı → `DRAGDROP_S_DROP`; imleç kendi istemci alanımızda ve `WindowFromPoint == hwnd` → `returned = true`, `DRAGDROP_S_CANCEL`; `GiveFeedback` → `DRAGDROP_S_USEDEFAULTCURSORS`. `SHDoDragDrop(hwnd, data, source, DROPEFFECT_COPY | DROPEFFECT_MOVE)`. Dönüşte `returned` → `SetCapture(hwnd)`, `Ended(Returned)`; değilse `PostMessageW(hwnd, WM_LBUTTONUP | WM_RBUTTONUP, 0, imlecin istemci koordinatı)` ve `Ended(Dropped | Cancelled)`.
- Uygulama: `Inside` durumunda `Hit::Outside` → hayalet gizlenir, vurgular temizlenir, `drag_out`; `Returned` → `Inside`'a dön; `Dropped/Cancelled` → `Idle` (gelen `WM_*BUTTONUP` `item-up`'ta yok sayılır, seçim daralmaz).

- [ ] **Step 1: Test:** `query_continue_rules` — saf `continue_drag(esc, buttons_down, inside_window) -> Continue::{Go, Drop, Cancel, Returned}` tablosu.
- [ ] **Step 2–4:** uygula; elle: Explorer'a (aynı disk taşıma: Explorer taşır, Gezik silmez), masaüstüne, Outlook/e-posta veya Not Defteri'ne (kopya), Geri Dönüşüm Kutusu'na, sağ tuşla Explorer'a (Explorer menüsü), pencereden çıkıp geri girme, dışarıda Esc.
- [ ] **Step 5:** Commit — `Drag files out of Gezik to Explorer and other programs`

### Task 7: Linux — URI ve pano biçimleri (saf)

**Files:** Create `crates/gezik-platform/src/linux/uri.rs` (her platformda derlenir: `#[cfg_attr(not(target_os = "linux"), allow(dead_code))]`), `linux/mod.rs` iskeleti.

**Interfaces:**
- `pub fn file_uri(path: &Path) -> String` (RFC 8089: `file:///` + yüzde kodlama; harf, rakam, `-._~/` ve `!$&'()*+,;=:@` dışındakiler `%XX`, UTF-8 baytları), `pub fn path_from_uri(uri: &str) -> Option<PathBuf>` (`file://localhost/` ve `file:///`; başka ana bilgisayar → `None`).
- `pub fn uri_list(paths: &[PathBuf]) -> String` (`\r\n` ayraç), `pub fn parse_uri_list(text: &str) -> Vec<PathBuf>` (`#` yorumları, boş satırlar atlanır).
- `pub fn gnome_copied_files(paths, cut: bool) -> String` (`copy\n` / `cut\n` + satır başına URI, sonda yeni satır yok), `pub fn parse_gnome_copied_files(text) -> Option<(bool, Vec<PathBuf>)>`.
- `pub fn kde_cut(cut: bool) -> &'static [u8]` (`b"1"`/`b"0"`).

- [ ] **Step 1: Testler:** `file_uris_round_trip_unicode_and_reserved_characters` (`/home/ş é/a#b%c.txt` → `file:///home/%C5%9F%20%C3%A9/a%23b%25c.txt` ve geri), `uri_lists_skip_comments_and_foreign_hosts`, `gnome_copied_files_round_trip` (copy/cut), `bad_percent_escapes_are_refused` (`%zz`, `%C5` yarım UTF-8 → yine bayt olarak yol: Unix yolları bayttır; `OsStr::from_bytes`; Windows'ta derleme için `#[cfg(unix)]` dönüşüm, testler Windows'ta UTF-8 alt kümesiyle).
- [ ] **Step 2–4:** uygula, `cargo test -p gezik-platform` (Windows) ve Linux çapraz `cargo check`.
- [ ] **Step 5:** Commit — `Read and write the Linux clipboard formats for files`

### Task 8: Linux X11 — pano ve XDND

**Files:** Create `crates/gezik-platform/src/linux/x11.rs`; Modify `linux/mod.rs`, `clipboard.rs`, `dnd/mod.rs`, `Cargo.toml`.

**Tasarım:**
- Bir arka plan iş parçacığı kendi `RustConnection`'ını açar (`DISPLAY`), gizli bir `InputOnly` pencere kurar, `XFixesSelectSelectionInput(CLIPBOARD)` ile seçim değişikliklerini dinler (sayaç `AtomicU64`).
- **Yazma:** `write_files(paths, cut)` iş parçacığına kanal ile gider: `SetSelectionOwner(CLIPBOARD, pencere)`; `SelectionRequest` cevapları: `TARGETS`, `text/uri-list`, `x-special/gnome-copied-files`, `application/x-kde-cutselection`, `UTF8_STRING`/`text/plain` (yollar), `MULTIPLE` yok (reddedilir). Büyük veri için INCR gerekmez (dosya listesi < 256 KB; daha büyükse INCR ile yazılır — `max_request_length`'ten büyükse).
- **Okuma:** sahip biz isek kendi verimiz; değilse iş parçacığı `ConvertSelection(CLIPBOARD, x-special/gnome-copied-files)` → yoksa `text/uri-list`; `SelectionNotify` en çok 1 sn beklenir; INCR okunur. Kesme bilgisi: gnome biçimi veya `application/x-kde-cutselection == "1"`.
- **sequence():** XFixes sayacı. **clear():** `SetSelectionOwner(NONE)` (yalnızca sahip bizsek).
- **XDND hedefi:** winit'in penceresine `XdndAware = 5` (winit zaten koyuyor) ve `XdndProxy = bizim pencere`; bizim pencereye `XdndProxy = kendisi`, `XdndAware = 5`. `XdndEnter` → tür listesi (`text/uri-list` yoksa reddet); `XdndPosition` → kök koordinatını `TranslateCoordinates` ile winit penceresine çevir, tuşlar `QueryPointer` maskesinden, olay UI'a (`wake`), cevap gelince `XdndStatus` (kabul + `XdndActionCopy/Move`); `XdndDrop` → `ConvertSelection(XdndSelection, text/uri-list)` → yollar → UI `dropped` → `XdndFinished` (gerçekleşen eylem; taşımada `XdndActionMove`, kaynak taşımanın yapıldığını bilir; GTK/Qt kaynakları taşımada silmez çünkü dosya yöneticisi hedefi taşır — `XdndActionMove` sonrası kaynağın silme davranışı uygulamaya bağlıdır, Nautilus/Dolphin silmez).
- **XDND kaynağı:** `drag_out` → `Running(X11Drag)`: `SetSelectionOwner(XdndSelection)`; `moved(x, y)` → kök koordinatı, `QueryPointer`/`TranslateCoordinates` zinciriyle `XdndAware` taşıyan üst pencereyi bul (`XdndProxy` izlenir), `XdndEnter/Position/Leave` gönder, `XdndStatus` cevabını izle; `released()` → kabul edildiyse `XdndDrop`, `XdndFinished` beklenir (2 sn sınırı) → `on_end(Dropped)`; değilse `Leave` + `Cancelled`. İşaretçi pencereye geri girerse uygulama `cancel()` değil `Returned` akışını kullanır (X11'de sistem döngüsü yok: `moved` pencere içindeyse `Leave` gönderilir ve Slint sürüklemesi sürer).

- [ ] **Step 1: Testler** (her yerde derlenen saf parçalar): `xdnd_version_and_actions` (ileti alanlarının paketlenmesi), `selection_targets_list` (sunulan hedef sırası).
- [ ] **Step 2–4:** uygula; Linux çapraz `cargo check`; Docker denemesi Task 11'de.
- [ ] **Step 5:** Commit — `Use the X11 clipboard and XDND for files on Linux`

### Task 9: Linux Wayland — pano ve sürükle-bırak

**Files:** Create `crates/gezik-platform/src/linux/wayland.rs`; Modify `linux/mod.rs`, `Cargo.toml`.

**Tasarım:**
- `attach`: pencere tutamacı `RawDisplayHandle::Wayland` ise `Backend::from_foreign_display(display)` → `Connection::from_backend`; kendi `EventQueue`'muz; kayıt defterinden `wl_seat` ve `wl_data_device_manager` (v3) bağlanır; kendi `wl_pointer` ve `wl_keyboard`'umuz yalnızca seri numarası içindir (düğme/tuş olaylarının `serial`'ı). Winit'in yüzeyi `RawWindowHandle::Wayland.surface` → `WlSurface::from_id`. Kuyruk kendi iş parçacığında `blocking_dispatch`.
- **Pano:** `wl_data_device.selection(offer)` → güncel teklif ve türleri; sayaç artar. Okuma: teklif `receive(mime, pipe)`, `flush`, borudan 1 sn sınırla oku (`x-special/gnome-copied-files`, yoksa `text/uri-list`). Yazma: `wl_data_source` (`text/uri-list`, `x-special/gnome-copied-files`, `application/x-kde-cutselection`, `text/plain;charset=utf-8`) + `set_selection(source, son tuş/düğme serial'ı)`; `send(mime, fd)` olaylarında veri yazılır; `cancelled` → kaynak bırakılır. Odak yoksa (serial yok) `Failed("Gezik needs the focus to use the clipboard")`.
- **DnD hedefi:** `enter(serial, surface, x, y, offer)` (yalnız bizim yüzey) → `text/uri-list` istenir, gelince UI `over`; cevapla `offer.accept(serial, "text/uri-list")` ve `set_actions(copy|move, tercih)`; `motion` → `over`; `leave` → `leave`; `drop` → UI `dropped` → `offer.finish()`.
- **DnD kaynağı:** `drag_out` → `start_drag(source, winit yüzeyi, None, son düğme serial'ı)`; kaynak olayları `dnd_drop_performed`, `dnd_finished` → `on_end(Dropped)`, `cancelled` → `on_end(Cancelled)`; geri giriş bizim hedef yolumuzla işlenir. Bittiğinde uygulama Slint'e `PointerReleased` gönderir (bileşik tuş bırakmayı winit'e iletmez).

- [ ] **Step 1: Testler:** saf `mime_preference` (gnome > uri-list), `pipe_read_times_out` (Unix'te `#[cfg(unix)]`, Docker'da çalışır).
- [ ] **Step 2–4:** uygula; Linux çapraz `cargo check`.
- [ ] **Step 5:** Commit — `Use the Wayland data device for the clipboard and drag and drop`

### Task 10: macOS — sürükle-bırak

**Files:** Create `crates/gezik-platform/src/dnd/macos.rs`; Modify `Cargo.toml`, `dnd/mod.rs`.

**Tasarım:**
- **Hedef:** winit'in görünüm sınıfına (pencere tutamacındaki `NSView`'ın `class()`'ı) `draggingEntered:`, `draggingUpdated:`, `draggingExited:`, `prepareForDragOperation:`, `performDragOperation:` `class_replaceMethod` ile Gezik'in işlevleri konur (winit'in `DroppedFile` olayı kaybolur, onun yerine Gezik işler). Yollar `draggingPasteboard` → `NSURL` dosya URL'leri; konum `draggingLocation` → `convertPoint:fromView:nil` (görünüm ters değilse y çevrilir) → `backingScaleFactor` ile fiziksel piksel; tuşlar `NSEvent::modifierFlags` (Shift, Option = kopya); izin verilen `draggingSourceOperationMask`. Dönüş `NSDragOperationCopy/Move/None`.
- **Kaynak:** `define_class!` ile `GezikDragSource: NSObject + NSDraggingSource` (`draggingSession:sourceOperationMaskForDraggingContext:` → Copy|Move; `draggingSession:endedAtPoint:operation:` → `on_end`). `beginDraggingSessionWithItems:event:source:` (öğeler: `NSDraggingItem(NSURL)`, çerçeve imleç çevresi, resim `NSWorkspace iconForFile:`), olay `NSApp.currentEvent`. `Handoff::Running` (no-op `OutsideDrag`); bitince uygulama Slint'e `PointerReleased` gönderir.

- [ ] `cargo check -p gezik --target aarch64-apple-darwin` geçer (çalıştırılamaz: "denenmedi").
- [ ] Commit — `Drag and drop files on macOS (compiled, not tried)`

### Task 11: Linux denemesi (Docker)

**Files:** Create `scripts/linux/Dockerfile`, `scripts/linux/test.sh`.

- [ ] Kap: `rust:1` + `libfontconfig-dev libxkbcommon-dev libwayland-dev xvfb xclip xdotool sway wl-clipboard` (+ gerekirse `libxkbcommon-x11`). Depo kaba bağlanır (`-v`), `CARGO_TARGET_DIR` kapta.
- [ ] `cargo build -p gezik` ve `cargo test --workspace` (Linux) geçer.
- [ ] X11: `Xvfb :99` + Gezik (`GEZIK_CONFIG_DIR` geçici, bir test klasöründe) + `xdotool` ile bir dosya seç, `ctrl+c` → `xclip -selection clipboard -t x-special/gnome-copied-files -o` beklenen çıktıyı verir; `ctrl+x` → `cut`; ters yön: `xclip -t text/uri-list` ile dosya koy, Gezik'te `ctrl+v` → dosya kopyalanır. Gezik'in kendi `XDND` kaynağı ↔ hedefi: iki Gezik penceresi arasında `xdotool mousedown/mousemove/mouseup` ile sürükleme (dışarı akışı + XdndProxy hedefi).
- [ ] Wayland: `WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway` + Gezik (`WAYLAND_DISPLAY`) ; `wtype`/`wlrctl` kullanılabilirse Ctrl+C → `wl-paste -t x-special/gnome-copied-files`. Kurulamazsa sonuç "denenmedi" olarak yazılır.
- [ ] Sonuçlar takip notuna; Commit — `Build and try the Linux clipboard in a container`

### Task 12: Elle denemeler, ölçüm, belgeler

- [ ] Windows ekran denemeleri (spec 12.4 sürükle-bırak listesi): tüm hedefler, Shift/Ctrl, sağ tuş, sekme bekleme, sabitleme, Explorer'a/masaüstüne/e-postaya, Explorer'dan; ızgara ve liste; yüksek DPI (125 %/150 %) konum doğruluğu.
- [ ] Ölçüm: boşta bellek, açılış süresi, sürükleme sırasında kare süresi (mevcut `scripts/perf` yöntemleri).
- [ ] `cargo check` macOS/Linux; takip notuna 4b sonuçları ve denenmeyenler; README; spec durumu.
- [ ] Commit — `Record what 4b does and what could not be tried here`
