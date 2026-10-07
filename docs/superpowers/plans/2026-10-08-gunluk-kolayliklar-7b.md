# Günlük Kolaylıklar 7b (Sabitlenen Klasör Grupları ve Görünüm Seçenekleri) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kenar çubuğundaki sabitlemelere takma ad ve grup (aynı `pinned` listesi; öğe metin ya da `{ path, name, group }` satır içi tablo, eski dosyalar aynen okunur ve takma ad/grup yoksa aynen yazılır), gruplar arası sürükleme, grup başlığı menüsü, `Rename…` / `Move to group ▸`, görünen ilk dokuz sabitlemeye `pin-1…pin-9` (Windows/Linux Alt+1…9, macOS ⌘⌥1…9); ve `[view]`'da her klasörde aynı yedi görünüm seçeneği (`hide-extensions`, `folders-first`, `date-format`, `size-format`, `single-click-open`, `show-hidden`, `show-system`), View menüsünden değiştirilip `settings.toml`'a yazılması, `toggle-hidden`'ın `show-hidden`'ı yazması, Windows'un gizli/sistem özniteliklerinin `Entry`'yi büyütmeden okunması.

**Architecture:** Saf kurallar `gezik-core`'da: `Entry`'nin bayrak baytı (`Entry::HIDDEN`, `Entry::SYSTEM`, `Entry::is_shown`), uzantısız ad (`shown_name`), `sort_entries`'in `folders_first`'ü, `view::{DateFormat, SizeFormat, ViewOptions, relative_date}`, `format_size_in`. `gezik-config`'te yeni `pins.rs` (`PinEntry`, okuma/yazma biçimi, liste işlemleri: `normalize`, `place`, `set_group`, `move_group`, `rename_group`, `ungroup`, `set_name`), `[view]` seçenekleri, `ViewOption` ve `SettingsChange::ViewOption`, dokuz yeni eylem. `gezik-platform::format_date` tarih biçimleri. Arayüzde yeni `view_options.rs` seçenekleri tutar, iyimser uygular ve tek yazıcıya verir (sidebar'ın sabitleme kalıbı); `View` gizli/sistem süzmesini, uzantı gizlemeyi, biçimleri ve tek tıkla açmayı uygular; `sidebar.rs` sabitlemeleri `pin_lines` ile başlıklı gruplar halinde çizer, satır numarasıyla sürükleme yerini (`slot_at`) bulur; `context_menu.rs` View menüsü seçeneklerini ve iki alt menüyü, sabitleme menüsünü ve grup başlığı menüsünü kurar; macOS menü çubuğu işaretli öğeler ve Go ▸ Pinned 1…9 alır.

**Tech Stack:** Rust 2024, Slint 1.18 (`MenuItem.checkable`/`checked`, `Timer`), `toml`/`toml_edit` 0.25 (var olan), `windows` 0.62 (var olan özellikler). Yeni crate yok, yeni `windows` özelliği yok.

**Spec:** `docs/superpowers/specs/2026-10-08-gunluk-kolayliklar-design.md` (bölüm 6, 7, 10.1'in `pinned`/`[view]` kısmı, 10.3'ün `pin-1…pin-9` satırı ve rakam tuşu kuralı, 11, 12, 13'ün 7b kısmı)

**7a'dan alınanlar:** `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` "Alt proje 7a … sonrası" bölümü: menü kimlikleri `1..4096` (7a'nın aralıkları 1000-1189), dört alt menü yuvası (`MAX_SUBMENUS`, `Submenu { title, at, items }`), tek `settings.toml` yazıcısı (`ConfigStore::write_settings`), yazımı yolda olan değişikliği dosyanın yeniden yüklenmesine ezdirmeme kalıbı (`sidebar.rs` `pins_on_their_way`), `gui.sh`'ın `scoped` sarmalayıcısı ve taze yapılandırmalarda `[session] restore = false`.

## Spec'ten sapmalar ve netleştirmeler

1. **Sabitleme modeli yeni `gezik-config/src/pins.rs`'te.** Spec §11 `settings.rs`/`settings_edit.rs` diyor; `PinEntry` (spec §6.1'in alanlarıyla), okuma (`parse_pin`), yazma (`pin_to_value`) ve kenar çubuğunun liste işlemleri (eskiden `sidebar.rs`'teki `pin_entry`, `insert_entries`, `move_entry`) tek yerde ve saf; `settings.rs` ve `settings_edit.rs` onu kullanır. Kenar çubuğunun satır düzeni (`pin_lines`, `slot_at`) `sidebar.rs`'te kalır.
2. **Gösterim sırası (normalleştirme):** önce grupsuzlar dosyadaki sırayla, sonra her grup ilk öğesinin sırasıyla, grubun öğeleri kendi sıralarıyla. Grup adları büyük/küçük harf ayrımı olmadan aynıdır ve ilk yazılışlarıyla gösterilir (`Work` ve `work` tek grup "Work"). `name` ve `group` kırpılır; boşsa yok sayılır. Elle yazılmış, yan yana olmayan grup öğeleri yan yana gösterilir; Gezik bir sonraki yazışında listeyi bu sırayla yazar (spec §6.2 "Gezik yazarken bir grubun öğelerini listede yan yana tutar").
3. **Okunamayan sabitlemeler dosyada kalır:** bilinmeyen anahtarlı tablo (`icon = …`, belki yeni bir sürümden), yolsuz ya da boş yollu girdi, `..` içeren yol, metin/tablo olmayan öğe uyarıyla atlanır ama `with_pinned` onları Gezik'in yazdıklarından sonra aynen geri yazar (`with_tables`'ın 5a kuralı). Bugün `..`'lı girdiler ilk yazımda düşüyordu; bu değişir. Yinelenen yol (Windows'ta harf büyüklüğü fark etmez) uyarıyla atlanır ve yazımda düşer.
4. **Dizi biçimi:** bütün öğeler düz metinse tek satır (`pinned = ["a", "b"]`, bugünkü gibi; "takma ad ya da grup hiç kullanılmadıysa dosya bugünkü gibi kalır"); bir tablo bile varsa her öğe kendi satırında, sonda virgül (spec §6.1'in örneği). Tablo anahtarları `path`, `name`, `group` sırasıyla, olmayanlar yazılmaz.
5. **Başlıklar:** "PINNED" başlığı yalnız görünen grupsuz sabitleme varken çizilir (grup başlığı kuralı, spec §6.2). Grup başlığı satırları yeni bölüm `SECTION_GROUP = 3`, `index` görünen gruplar arasındaki sırasıdır; tıklama ve orta tık bir şey yapmaz; sağ tık grup başlığı menüsünü açar (her sistemde Gezik/Slint menüsü, Explorer menüsü değil).
6. **Sürükleme yeri satır numarasıyla:** `Hit::PinAt(n)` artık "kenar çubuğunun n. satırının üstündeki çizgi" (eskiden n. sabitleme). Bir sabitlemenin üstüne bırakmak onun grubunda ondan önceye; bir başlığın üstündeki çizgiye ya da son sabitlemenin altına bırakmak önceki sabitlemenin grubunda ondan sonraya; bir grup başlığının kendisine bırakmak o grubun başına (`SideRow::PinHeader` → `PinAt(satır + 1)`). Görünen grupsuz sabitleme yokken grupsuz bölüme sürükleyerek bırakılamaz; `Move to group ▸ No group` bunu yapar. Kenar çubuğunun kendi sürüklemesi de aynı çizgi kuralını kullanır (`pinned-drop(from-row, line-row)`).
7. **Move up / Move down** grup içinde, görünen sıraya göre (gizli, var olmayan sabitlemeler yerinde kalır). **Move group up/down** gruplar listesinde komşu grupla yer değiştirir; komşu grubun bütün öğeleri bu makinede yoksa görünürde bir şey değişmez (kabul edilen uç durum).
8. **Rename…** soru penceresi takma adla, yoksa klasörün kenar çubuğundaki adıyla başlar; boş yanıt ya da klasörün kendi adı takma adı kaldırır. **New group…** var olan bir grubun adı verilirse (harf büyüklüğü fark etmez) ona katılır. **Move to group ▸** görünen grupları (kendi grubu hariç, en çok 50), `New group…` ve yalnız gruplu sabitlemede `No group` listeler. Gruba taşınan sabitleme o grubun sonuna gider; yeni grup en sona.
9. **İpucu (tooltip) yeni:** Gezik'te ipucu yoktu. Kenar çubuğunun sabitleme satırları 700 ms bekleyince satırın altında, pencere düzeyinde çizilen küçük bir ipucu gösterir: tam yol; ilk dokuzda ikinci satırda o numaranın tuşu (`Alt+1`, `⌥⌘1`; bağlanmamışsa satır yok). Başka satırlarda ipucu yok.
10. **`pin-N`** etkin sekmede görünen N. sabitlemeye gider (`nav.go`); adres çubuğu yazılırken de (6a'nın `tab-N`'si gibi). Olmayan numara bir şey yapmaz.
11. **Rakam tuşu kuralı (`keys::chord_from_press`):** bugünkü "Ctrl/⌘ + rakam tuşu, Alt yokken" kuralına iki durum eklenir: Windows/Linux'ta Alt varken Ctrl ve Win yoksa (Shift olabilir; AltGr Windows'ta Ctrl+Alt olduğundan girmez), macOS'ta ⌘ ve ⌥ birlikte. Latin harfi yazan basış yine rakam sayılmaz (`45f5915`).
12. **`show-hidden` şablonda yorum satırı:** varsayılan sisteme göre değişir (Windows/Linux `true`, macOS `false`), şablon taşınabilir olduğundan `# show-hidden = true …` olarak yazılır; menü ya da `toggle-hidden` ilk kez değiştirince `with_view_option` anahtarı `[view]`'ın sonuna ekler. Öbür altı seçenek şablonda varsayılan değerleriyle yazılıdır.
13. **Görünüm seçenekleri iyimser uygulanır:** View menüsü ya da `toggle-hidden` değişikliği hemen gösterir ve tek yazıcıya `SettingsChange::ViewOption` verir; yazımlar yoldayken dosyanın yeniden yüklenmesi seçenekleri ezmez (sidebar'ın `pins_on_their_way` kalıbı, `view_options::OptionsState`); yazım başarısızsa durum çubuğu nedenini söyler ve dosyadaki değerlere dönülür. Yapılandırma klasörü yoksa değişiklik yalnız bellekte. Dosya izleyicisinin yeniden yüklemesi (elle düzenleme) her yere uygulanır.
14. **Gizli/sistem kuralı:** sistem özniteliği tek başına bir şey gizlemez; gizli öznitelik ya da noktayla başlayan ad `show-hidden = false` iken gizler; hem gizli hem sistem öznitelikli ("korunan") öğe yalnız `show-system`'e bakar (`show-hidden`'dan bağımsız: menüdeki "Show system items" tek başına onları gösterir). macOS ve Linux'ta bayraklar hep 0. Gizli/sistem değişince klasör yeniden okunur (`nav.reload`, bugünkü Ctrl+H gibi).
15. **Windows ve Linux View menüsü** seçenekleri bugünkü öğeleri gibi "• " ile işaretler (Slint `MenuEntry`'de işaret alanı yok, Linux'un kendi menüsü onu çizmez); `Date format ▸` ve `Size format ▸` iki alt menü (`Submenu`, `MAX_SUBMENUS` içinde). **macOS menü çubuğu** `checkable` öğeler kullanır; Slint etkinleştirmede `checked`'i kendisi çevirdiği için her öğe pencerenin kendi `in-out` bool'una iki yönlü bağlanır (`checked <=> root.view-…`, alt menülerde her seçenek için bir bool) ve Rust her değişiklikten (değişmeyen seçimden de) sonra hepsini yeniden yazar. macOS'ta "Show system items" yok. Bugünkü "Show Hidden Files" "Show Hidden Items" olur ve işaretlenir.
16. **Boyut biçimi:** `gezik_core::format_size_in(bytes, SizeFormat)` eklenir; `format_size(bytes)` ikiliyle aynı kalır ve arayüz dışı ya da ayar olmayan yerlerde (arşiv aracı indirme boyutu, `gezik-batch`'in çıkarma boş yer iletisi, yeniden adlandırmanın `{size}` parçası) kullanılmaya devam eder. Arayüzün boyutları (sütun, seçim özeti, önizleme, işlem paneli taraması ve hızı, çakışma listesi) `view_options::size_text` ile ayarı izler; tarihler de (sütunlar, önizleme, çakışma listesi) `view_options::date_text` ile. Çakışma listesi spec'in saydığı yerlere tutarlılık için eklendi.
17. **Tarih biçimleri:** `relative`: 0-3599 sn önce `N min ago` (N en az 1), bugün `Today 14:05`, dün `Yesterday 14:05` (yerel saat, 24 saat, iki haneli), gelecekteki ve daha eski zamanlar `system`. `short`: Windows'ta sistemin kısa tarihi (`GetDateFormatEx`), başka yerde `YYYY-MM-DD`. `iso`: her yerde `YYYY-MM-DD HH:MM` (yerel). Dakikalık yeniden biçimleme yalnız `relative` iken çalışan bir zamanlayıcıyla, listenin kaydırma yeri ve yüksekliğinden hesaplanan görünen satırlara (`visible_lines`).
18. **Tek tıkla açma:** Ctrl/⌘ ve Shift'siz bir sol basış, sürüklemeye dönüşmeden bırakılınca açar (`drag.rs` `Phase::Armed` kolu); çift tık bu ayar açıkken yok sayılır (ilk tık zaten açtı); arşivin çift tık kuralı (`[archives] double-click`) tıklamaya da uygulanır (main.rs `open_entry`). Liste ve ızgara aynı.
19. **Uzantı gizleme kuralı:** son noktadan kesilir, yalnız nokta 0'dan büyük ve son karakter değilse (`.gitignore`, `x.`, klasörler aynen; `a.tar.gz` → `a.tar`). Yalnız `FileRow.name` değişir (liste, ızgara, erişilebilirlik adı); yeniden adlandırma alanı, süzgeç, harfle atlama, sıralama gerçek adla çalışır.
20. **macOS menü çubuğu:** View'a "Hide Extensions", "Folders First", "Single-Click to Open", "Show Hidden Items" (⌘⇧.), "Date Format ▸" (Relative, Short, ISO, System), "Size Format ▸" (Binary, Decimal); Go'ya ayraçtan sonra "Pinned 1"…"Pinned 9" (⌘⌥1…9; başlıklar sabit). Menü komutu `view-option:<kimlik>` (View menüsünün kimlikleri) ve `pin-N`.
21. **Kimlikler:** 7b'nin aralıkları `RENAME_PIN 1200`, `GROUP_NEW 1201`, `GROUP_NONE 1202`, `GROUP_MOVE_FIRST 1210-1259` (`GROUP_MAX 50`), `GROUP_UP 1260`, `GROUP_DOWN 1261`, `GROUP_RENAME 1262`, `UNGROUP 1263`, `HIDE_EXTENSIONS 1300`, `FOLDERS_FIRST 1301`, `SINGLE_CLICK_OPEN 1302`, `SHOW_HIDDEN 1303`, `SHOW_SYSTEM 1304`, `DATE_FORMAT_FIRST 1310-1313` (`DateFormat::ALL` sırası), `SIZE_FORMAT_FIRST 1320-1321`. 1400-4095 7c'ye kalır.
22. **Kenar çubuğu soru sorar:** `Sidebar::new` `Dialogs` alır; main.rs'te `Dialogs::new` kenar çubuğundan önceye taşınır.
23. **Linux kabı:** yeni kipler `pins` (Gezik boş bir `HOME` ile başlar, böylece FOLDERS'ta yalnız Home olur ve kenar çubuğu satırları sabit yerde) ve `view-options` (işlev adı `view_options`). İkisi de X11'de; Wayland'da bu kiplerin pano ve sürükleme yolu 7a'nınkiyle aynı olduğundan eklenmedi.
24. **Dal:** `feat/daily-7b`, çalışma ağacı `D:\Work\gezik-7b`, master `7151363` + not commit'i `e1ec4c0` + bu plan.

## Global Constraints

- Rust edition 2024, stable (1.99). Komutlar `D:\Work\gezik-7b`'den; `cargo` = `~/.cargo/bin/cargo` (Git Bash), her derleme ve testte `-j 8`. Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Dal `feat/daily-7b` (`e1ec4c0` ve plan commit'i üstünde). `D:\Work\gezik`'te başka iş sürüyor: oraya dokunulmaz, orada derlenmez.
- Her görevin sonunda `cargo build --workspace -j 8`, `cargo test --workspace -j 8` geçer; `cargo clippy --workspace --all-targets -j 8 -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- Çapraz denetimler (her görevin sonunda): `cargo check -j 8 -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -j 8 -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -j 8 -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- Yeni crate yok; `gezik-core` bağımlılıksız kalır; `windows` crate'ine yeni özellik eklenmez (`MetadataExt::file_attributes` std'de).
- Exe: master `7151363`'ün sürüm derlemesi **22.546.432 bayt** (2026-10-08, bu çalışma ağacında `e1ec4c0`'da `cargo build --release -p gezik -j 8` ile ölçüldü; `e1ec4c0` yalnız not ekler, kod master'la aynı). 7b sınırı +0,25 MiB = +262.144 bayt → **en çok 22.808.576 bayt**. Spec'in toplam ölçütü 7a + 7b + 7c ≤ +0,75 MiB, `38c65b3`'e (22.240.256 bayt) göre → en çok 23.026.688 bayt. 7a `e35e380`'de 22.546.432 bayttı; master 7a'dan önce PR #15/#16 ile 22.334.464'e büyümüştü ve 7a'nın sınırı o yüzden güncel master'dan sayıldı (karar L50). 7b sınırında kalınırsa toplama 7c için 218.112 bayt kalır.
- Boşta bellek master'a göre büyümez (Windows'ta 7,0-7,1 MB, `measure.ps1`; Linux kabında RssAnon aynı). Açılış süresi değişmez (≤ ~60 ms). 100.000 dosyada sıralama ve kaydırma master'dakini belirgin aşmaz (`cargo test --release -p gezik-core sorting_100k -- --ignored`, `scripts/perf/stress.ps1`).
- `Entry`'nin boyutu değişmez (bayrak baytı `is_dir`'in yanındaki dolguda; `size_of` testi). Gizli/sistem bayrağı ek sistem çağrısı istemez (`DirEntry::metadata`'nın zaten okuduğu öznitelik).
- Arayüz iş parçacığı dosya sistemine dokunmaz (sabitlemelerin varlık denetimi bugünkü gibi arka planda); `settings.toml` yalnız tek yazıcı iş parçacığından (`write_settings`: `SettingsChange::Pinned`, `SettingsChange::ViewOption`).
- `views.toml` değişmez (yeni seçenekler klasör başına değil).
- Her yeni eylem (`pin-1` … `pin-9`) `[shortcuts]` ile değiştirilebilir, şablonda yorum satırı olarak görünür, macOS menü çubuğunda (Go ▸ Pinned 1…9) bulunur ve ulaşılabilirlik testine girer.
- Gezik'in menü kimlikleri `1..4096`; 4096 ve üstü Explorer'ın.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Co-Authored-By/Claude satırı yok. Plan ve not metinleri Türkçe.
- Yol birleştirme yalnız `Path::join`; sabitleme yolları belirteçli metin olarak saklanır (`KnownDirs::collapse`/`expand_checked`).

## Review Focus

1. **Elle düzenlenmiş `pinned` listeleri:** metin ve tablonun karışığı, bilinmeyen anahtar, yolsuz tablo, Windows'ta harf büyüklüğü farklı aynı yol, `..`, harf büyüklüğü farklı aynı grup, yan yana olmayan grup öğeleri, eski sürümün tek satırlık listesi. Okumada hepsi uyarıyla ya da sessizce doğru yere gider; Gezik yazınca okunamayanlar korunur, gruplar yan yana olur, takma adsız/grupsuz liste tek satır kalır. → Task 3 `pins_of_both_forms_are_read_and_bad_ones_warned`, `old_lists_are_written_as_they_were`, `unreadable_pins_are_kept_after_ours`, `normalize_puts_groups_together_and_trims`.
2. **Bu makinede olmayan sabitlemeler** (başka makinenin yolu, takılı olmayan sürücü): numaralar ve başlıklar onları atlar, yalnız olmayan öğeli grubun başlığı çizilmez, sürükleme yeri hiçbir zaman gizli bir sabitlemeye düşmez, yazımlar onları korur. → Task 8 `pin_lines_skip_hidden_pins_and_empty_groups`, `a_line_finds_its_slot`, Task 3 `place_moves_and_groups`; `gui.sh pins` (`/tmp/pn/gone`).
3. **Gizli ve korunan öğeler:** Windows'ta `desktop.ini`, `$RECYCLE.BIN`; Linux/macOS'ta noktalı adlar; `show-system` tek başına; `Entry` büyümez. → Task 1 `hidden_and_system_items_follow_the_two_settings`, `the_flags_fit_in_the_padding_after_is_dir`, `windows_attributes_reach_the_entry`.
4. **Yazım yarışları ve bozuk dosya:** View menüsünde art arda iki değişiklik, yazım yoldayken izleyicinin yeniden yüklemesi, bozuk `settings.toml` (yazıcı dokunmaz, durum çubuğu söyler, arayüz dosyanın değerine döner). → Task 6 `a_reload_while_writes_are_on_their_way_is_ignored`, `a_failed_write_goes_back_to_the_file`.
5. **Klavye düzenleri:** AZERTY'de Alt+& (1 tuşu), Türkçe Q'da AltGr+rakam yazar, adres çubuğu yazılırken Alt+1, macOS ⌘⌥1 (⌥1 `¡` yazar), Ctrl+1 hâlâ sekme 1. → Task 4 `alt_and_a_digit_key_is_the_pin_on_every_layout`, `pins_have_alt_and_cmd_option_digits`, ulaşılabilirlik testi.

---

### Task 1: `gezik-core` — `Entry`'nin bayrak baytı, gizli/sistem kuralı, uzantısız ad, `folders_first`

**Files:**
- Modify: `crates/gezik-core/src/lib.rs` (`Entry.flags`, `Entry::{HIDDEN, SYSTEM, is_shown}`, `shown_name`, `list_dir`'de öznitelikler, testler), `crates/gezik-core/src/sort.rs` (`sort_entries`'e `folders_first`), `crates/gezik-core/src/pattern.rs` (test yardımcısı), `crates/gezik-core/examples/filter_bench.rs`, `crates/gezik/src/view/listing.rs` (test yardımcısı `files`), `crates/gezik/src/view/mod.rs` (`sort_entries` çağrısı, şimdilik `true`)

**Interfaces:**
- Produces:

```rust
// lib.rs
pub struct Entry { pub name: String, pub is_dir: bool, pub flags: u8, pub size: u64,
                   pub modified: Option<SystemTime>, pub created: Option<SystemTime> }
impl Entry {
    pub const HIDDEN: u8 = 1;
    pub const SYSTEM: u8 = 2;
    pub fn is_shown(&self, show_hidden: bool, show_system: bool) -> bool;
}
pub fn shown_name(name: &str, is_dir: bool, hide_extension: bool) -> &str;
// sort.rs
pub fn sort_entries(entries: &mut Vec<Entry>, spec: SortSpec, folders_first: bool,
                    type_name: impl Fn(&Entry) -> String) -> Vec<usize>;
```

- [ ] **Step 0: Çalışma ağacı.** `cd /d/Work/gezik-7b && git status` temiz, dal `feat/daily-7b`, `git log --oneline -3` plan commit'i, `e1ec4c0`, `7151363`.

- [ ] **Step 1: Write the failing tests**

`lib.rs` test modülüne (`extension_skips_folders_and_leading_dots`'taki `e` yardımcısı `flags: 0` alır):

```rust
    fn flagged(name: &str, flags: u8) -> Entry {
        Entry { name: name.to_owned(), is_dir: false, flags, size: 0, modified: None, created: None }
    }

    #[test]
    fn the_flags_fit_in_the_padding_after_is_dir() {
        // `Entry` before the flags: the byte must take no room of its own (spec 7.1).
        #[allow(dead_code)]
        struct Before {
            name: String,
            is_dir: bool,
            size: u64,
            modified: Option<SystemTime>,
            created: Option<SystemTime>,
        }
        assert_eq!(std::mem::size_of::<Entry>(), std::mem::size_of::<Before>());
    }

    #[test]
    fn hidden_and_system_items_follow_the_two_settings() {
        let plain = flagged("a.txt", 0);
        let dot = flagged(".git", 0);
        let hidden = flagged("notes.txt", Entry::HIDDEN);
        let system_only = flagged("pagefile.sys", Entry::SYSTEM);
        let protected = flagged("desktop.ini", Entry::HIDDEN | Entry::SYSTEM);
        for show_hidden in [false, true] {
            for show_system in [false, true] {
                assert!(plain.is_shown(show_hidden, show_system));
                assert!(system_only.is_shown(show_hidden, show_system), "the system attribute alone hides nothing");
                assert_eq!(dot.is_shown(show_hidden, show_system), show_hidden, "a dot name");
                assert_eq!(hidden.is_shown(show_hidden, show_system), show_hidden, "the hidden attribute");
                assert_eq!(protected.is_shown(show_hidden, show_system), show_system, "protected: show-system only");
            }
        }
    }

    #[test]
    fn hiding_the_extension_keeps_folders_and_dot_names() {
        assert_eq!(shown_name("rapor.pdf", false, true), "rapor");
        assert_eq!(shown_name("a.tar.gz", false, true), "a.tar");
        assert_eq!(shown_name(".gitignore", false, true), ".gitignore");
        assert_eq!(shown_name("x.", false, true), "x.");
        assert_eq!(shown_name("noext", false, true), "noext");
        assert_eq!(shown_name("dir.d", true, true), "dir.d");
        assert_eq!(shown_name("rapor.pdf", false, false), "rapor.pdf");
    }

    #[cfg(windows)]
    #[test]
    fn windows_attributes_reach_the_entry() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        let dir = temp_dir("attributes");
        let make = |name: &str, attributes: u32| {
            fs::OpenOptions::new().write(true).create_new(true).attributes(attributes).open(dir.join(name)).unwrap();
        };
        make("plain.txt", 0);
        make("hidden.txt", FILE_ATTRIBUTE_HIDDEN);
        make("desktop.ini", FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
        let entries = list_dir(&dir).unwrap();
        let flags = |name: &str| entries.iter().find(|e| e.name == name).unwrap().flags;
        assert_eq!(flags("plain.txt"), 0);
        assert_eq!(flags("hidden.txt"), Entry::HIDDEN);
        assert_eq!(flags("desktop.ini"), Entry::HIDDEN | Entry::SYSTEM);
        // Read-only and system files cannot always be deleted plainly: clear the attributes.
        for name in ["hidden.txt", "desktop.ini"] {
            let mut permissions = fs::metadata(dir.join(name)).unwrap().permissions();
            permissions.set_readonly(false);
            let _ = fs::set_permissions(dir.join(name), permissions);
        }
        let _ = fs::remove_dir_all(&dir);
    }
```

`sort.rs` test modülüne (`entry` yardımcısı `flags: 0` alır; var olan `sort_entries` çağrılarına `true`):

```rust
    #[test]
    fn folders_mix_with_files_when_not_first() {
        let mut v = vec![
            entry("b.txt", false, 3, None),
            entry("Zeta", true, 0, None),
            entry("alpha", true, 0, None),
            entry("a9", false, 7, None),
        ];
        sort_entries(&mut v, SortSpec::default(), false, |_| String::new());
        assert_eq!(names(&v), ["a9", "alpha", "b.txt", "Zeta"]);
        sort_entries(&mut v, SortSpec { key: SortKey::Size, dir: SortDir::Desc }, false, |_| String::new());
        assert_eq!(names(&v), ["a9", "b.txt", "Zeta", "alpha"], "a folder's size counts as 0");
        sort_entries(&mut v, SortSpec::default(), true, |_| String::new());
        assert_eq!(names(&v), ["alpha", "Zeta", "a9", "b.txt"], "folders first again");
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core` → derlenmez (`flags`, `shown_name`, `sort_entries`'in dördüncü argümanı yok).

- [ ] **Step 3: Implement.** `lib.rs`:

```rust
#[derive(Debug, Clone)]
pub struct Entry {
    /// File name only. The full path is `dir.join(name)`; storing it per entry would
    /// repeat the folder path for every file and dominate memory in large folders.
    pub name: String,
    pub is_dir: bool,
    /// `Entry::HIDDEN` and `Entry::SYSTEM`: Windows' attributes (0 elsewhere). It sits in the
    /// padding after `is_dir`, so an entry is no larger for it (spec 7.1).
    pub flags: u8,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

impl Entry {
    /// The hidden attribute (Windows).
    pub const HIDDEN: u8 = 1;
    /// The system attribute (Windows).
    pub const SYSTEM: u8 = 2;

    /// Whether the list shows it: an item both hidden and system ("protected operating system
    /// files": `desktop.ini`, `$RECYCLE.BIN`) only with `show_system`; a name starting with a
    /// dot or a hidden one only with `show_hidden`; anything else always (spec 7.1).
    pub fn is_shown(&self, show_hidden: bool, show_system: bool) -> bool {
        let has = |bit: u8| self.flags & bit != 0;
        if has(Entry::HIDDEN) && has(Entry::SYSTEM) {
            return show_system;
        }
        show_hidden || !(self.name.starts_with('.') || has(Entry::HIDDEN))
    }
    // `extension` stays as it is.
}

/// The name the list shows: without its extension when `hide_extension` asks for it (only for
/// files, and never a leading dot alone or a trailing one: `.gitignore`, `x.` stay). Drawing
/// only: everything else goes by the real name (spec 7.1).
pub fn shown_name(name: &str, is_dir: bool, hide_extension: bool) -> &str {
    if !hide_extension || is_dir {
        return name;
    }
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => &name[..i],
        _ => name,
    }
}

/// `Entry::HIDDEN` and `Entry::SYSTEM` from what the directory read already gave (no call of
/// its own: Windows fills `file_attributes` from the directory listing).
#[cfg(windows)]
fn attribute_flags(meta: &std::fs::Metadata) -> u8 {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let attributes = meta.file_attributes();
    let mut flags = 0;
    if attributes & FILE_ATTRIBUTE_HIDDEN != 0 {
        flags |= Entry::HIDDEN;
    }
    if attributes & FILE_ATTRIBUTE_SYSTEM != 0 {
        flags |= Entry::SYSTEM;
    }
    flags
}

#[cfg(not(windows))]
fn attribute_flags(_meta: &std::fs::Metadata) -> u8 {
    0
}
```
  `list_dir`'de `Entry { …, flags: meta.as_ref().map_or(0, attribute_flags), … }` ve `sort::sort_entries(&mut entries, sort::SortSpec::default(), true, |_| String::new())`. Belge yorumu: "folders first, then by natural name order (see `sort`); the view sorts again for other choices".
  `sort.rs`: belge "Sorts `entries` by `spec`: folders first (in either direction) unless `folders_first` is off, then the column, …"; karşılaştırmanın başı:

```rust
    order.sort_unstable_by(|&i, &j| {
        let (a, b) = (&entries[i], &entries[j]);
        let folders = if folders_first { b.is_dir.cmp(&a.is_dir) } else { Ordering::Equal };
        folders.then_with(|| {
            // … the column, the name and the exact name, as before …
        })
    });
```
  `sorting_100k_names_is_fast` `true` alır. Derleme için: `pattern.rs` testinin `entry`'si, `examples/filter_bench.rs`'in `entry`'si ve `crates/gezik/src/view/listing.rs`'in `files` yardımcısı `flags: 0`; `crates/gezik/src/view/mod.rs` `sort_now`'da `sort_entries(&mut entries, spec, true, |e| self.type_name_of(e))` (Task 6 ayarı bağlar).

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-core` → PASS (Windows'ta `windows_attributes_reach_the_entry` da); `cargo test --release -j 8 -p gezik-core sorting_100k -- --ignored` → ≤ 50 ms (değişmedi); `cargo build --workspace -j 8`, çapraz denetimler.
- [ ] **Step 5: Commit** "Read hidden and system attributes into a spare byte of each entry and let folders mix with files".

---

### Task 2: `gezik-core` — tarih ve boyut biçimleri, `ViewOptions`

**Files:**
- Modify: `crates/gezik-core/src/view.rs` (`DateFormat`, `SizeFormat`, `ViewOptions`, `relative_date`, `day_number`), `crates/gezik-core/src/lib.rs` (`format_size_in`), `crates/gezik-core/src/ops/rate.rs` (`format_rate` biçim alır), `crates/gezik/src/operations.rs` (`format_rate` çağrısı, şimdilik `SizeFormat::Binary`)

**Interfaces:**
- Consumes: `batch::date::DateParts`.
- Produces:

```rust
// view.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DateFormat { Relative, Short, Iso, #[default] System }
impl DateFormat { pub const ALL: [DateFormat; 4]; pub fn as_str(self) -> &'static str;
                  pub fn parse(text: &str) -> Option<DateFormat>; pub fn label(self) -> &'static str; }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SizeFormat { #[default] Binary, Decimal }
impl SizeFormat { pub const ALL: [SizeFormat; 2]; pub fn as_str(self) -> &'static str;
                  pub fn parse(text: &str) -> Option<SizeFormat>; pub fn label(self) -> &'static str; }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewOptions { pub hide_extensions: bool, pub folders_first: bool, pub date_format: DateFormat,
    pub size_format: SizeFormat, pub single_click_open: bool, pub show_hidden: bool, pub show_system: bool }
impl Default for ViewOptions;   // show_hidden: !macOS, folders_first: true, the rest off/System/Binary
pub fn relative_date(now: &DateParts, then: &DateParts, age_secs: i64) -> Option<String>;
// lib.rs
pub fn format_size_in(bytes: u64, format: SizeFormat) -> String;   // format_size = Binary
// ops/rate.rs
pub fn format_rate(per_second: f64, format: SizeFormat) -> String;
```

- [ ] **Step 1: Write the failing tests**

`view.rs` (test modülü yoksa `#[cfg(test)] mod tests { use super::*; … }` ile):

```rust
    use crate::batch::date::DateParts;

    fn at(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> DateParts {
        DateParts { year, month, day, hour, minute, second: 0 }
    }

    #[test]
    fn relative_dates_go_by_the_minute_the_day_and_yesterday() {
        let now = at(2026, 10, 8, 14, 30);
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 14, 25), 300).as_deref(), Some("5 min ago"));
        assert_eq!(relative_date(&now, &now, 20).as_deref(), Some("1 min ago"), "under a minute");
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 13, 31), 3599).as_deref(), Some("59 min ago"));
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 13, 30), 3600).as_deref(), Some("Today 13:30"));
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 0, 5), 52_500).as_deref(), Some("Today 00:05"));
        assert_eq!(relative_date(&now, &at(2026, 10, 7, 23, 59), 52_260).as_deref(), Some("Yesterday 23:59"));
        assert_eq!(relative_date(&now, &at(2026, 10, 6, 9, 0), 192_600), None, "older: the system's");
        assert_eq!(relative_date(&now, &at(2026, 10, 8, 15, 0), -1800), None, "in the future: the system's");
        // Across a month, a year and a leap day.
        let new_year = at(2027, 1, 1, 0, 10);
        assert_eq!(relative_date(&new_year, &at(2026, 12, 31, 22, 0), 7800).as_deref(), Some("Yesterday 22:00"));
        let march = at(2024, 3, 1, 9, 0);
        assert_eq!(relative_date(&march, &at(2024, 2, 29, 8, 0), 90_000).as_deref(), Some("Yesterday 08:00"));
    }

    #[test]
    fn format_names_round_trip() {
        for format in DateFormat::ALL {
            assert_eq!(DateFormat::parse(format.as_str()), Some(format));
        }
        for format in SizeFormat::ALL {
            assert_eq!(SizeFormat::parse(format.as_str()), Some(format));
        }
        assert_eq!(DateFormat::parse("weekday"), None);
        assert_eq!(SizeFormat::parse("Binary"), None, "as written in settings.toml: lower case");
        let options = ViewOptions::default();
        assert!(options.folders_first && !options.hide_extensions && !options.single_click_open);
        assert_eq!((options.date_format, options.size_format), (DateFormat::System, SizeFormat::Binary));
        assert_eq!(options.show_hidden, !cfg!(target_os = "macos"));
        assert!(!options.show_system);
    }
```

`lib.rs` testleri (`formats_sizes` yanına):

```rust
    #[test]
    fn sizes_in_both_formats() {
        use crate::view::SizeFormat::{Binary, Decimal};
        assert_eq!(format_size_in(1536, Binary), "1.5 KB");
        assert_eq!(format_size_in(1023, Binary), "1023 B");
        assert_eq!(format_size_in(1024, Binary), "1.0 KB");
        assert_eq!(format_size_in(1500, Decimal), "1.5 kB");
        assert_eq!(format_size_in(999, Decimal), "999 B");
        assert_eq!(format_size_in(1000, Decimal), "1.0 kB");
        assert_eq!(format_size_in(1_000_000, Decimal), "1.0 MB");
        assert_eq!(format_size_in(5 * 1024 * 1024, Decimal), "5.2 MB");
        assert_eq!(format_size_in(u64::MAX, Binary), "16777216.0 TB");
        assert_eq!(format_size_in(u64::MAX, Decimal), "18446744.1 TB");
        assert_eq!(format_size(1536), format_size_in(1536, Binary), "format_size stays binary");
    }
```

`ops/rate.rs` testlerine:

```rust
    #[test]
    fn rates_follow_the_size_format() {
        use crate::view::SizeFormat;
        assert_eq!(format_rate(1_500_000.0, SizeFormat::Decimal), "1.5 MB/s");
        assert_eq!(format_rate(1_500_000.0, SizeFormat::Binary), "1.4 MB/s");
        assert_eq!(format_rate(-3.0, SizeFormat::Binary), "0 B/s");
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core` → derlenmez.

- [ ] **Step 3: Implement.** `view.rs` (`IconMode`'dan sonra):

```rust
/// How the date columns and the preview write a time (`[view] date-format`, spec 7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DateFormat {
    /// `5 min ago`, `Today 14:05`, `Yesterday 14:05`, older ones as `System`.
    Relative,
    /// The date only.
    Short,
    /// `YYYY-MM-DD HH:MM` everywhere.
    Iso,
    /// The system's short date and time (Windows), `YYYY-MM-DD HH:MM` elsewhere.
    #[default]
    System,
}

impl DateFormat {
    /// In menu order (the menu ids follow it).
    pub const ALL: [DateFormat; 4] = [DateFormat::Relative, DateFormat::Short, DateFormat::Iso, DateFormat::System];

    pub fn as_str(self) -> &'static str {
        match self {
            DateFormat::Relative => "relative",
            DateFormat::Short => "short",
            DateFormat::Iso => "iso",
            DateFormat::System => "system",
        }
    }

    pub fn parse(text: &str) -> Option<DateFormat> {
        DateFormat::ALL.into_iter().find(|f| f.as_str() == text)
    }

    /// Its line in the Date format menu.
    pub fn label(self) -> &'static str {
        match self {
            DateFormat::Relative => "Relative",
            DateFormat::Short => "Short",
            DateFormat::Iso => "ISO",
            DateFormat::System => "System",
        }
    }
}

/// How sizes are written (`[view] size-format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SizeFormat {
    /// Steps of 1024, `KB`, `MB`, `GB` (as Explorer).
    #[default]
    Binary,
    /// Steps of 1000, `kB`, `MB`, `GB` (as Finder and GNOME).
    Decimal,
}

impl SizeFormat {
    pub const ALL: [SizeFormat; 2] = [SizeFormat::Binary, SizeFormat::Decimal];

    pub fn as_str(self) -> &'static str {
        match self {
            SizeFormat::Binary => "binary",
            SizeFormat::Decimal => "decimal",
        }
    }

    pub fn parse(text: &str) -> Option<SizeFormat> {
        SizeFormat::ALL.into_iter().find(|f| f.as_str() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            SizeFormat::Binary => "Binary (1 KB = 1024 bytes)",
            SizeFormat::Decimal => "Decimal (1 kB = 1000 bytes)",
        }
    }
}

/// `[view]`'s options that are the same in every folder (spec 7.1); `views.toml` does not
/// keep them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewOptions {
    pub hide_extensions: bool,
    pub folders_first: bool,
    pub date_format: DateFormat,
    pub size_format: SizeFormat,
    pub single_click_open: bool,
    pub show_hidden: bool,
    /// Windows only: the protected system items.
    pub show_system: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            hide_extensions: false,
            folders_first: true,
            date_format: DateFormat::System,
            size_format: SizeFormat::Binary,
            single_click_open: false,
            // Finder hides them; Explorer users have their dot files in sight today.
            show_hidden: !cfg!(target_os = "macos"),
            show_system: false,
        }
    }
}

/// Days since 1970-01-01 of a calendar date (Howard Hinnant's `days_from_civil`).
fn day_number(date: &crate::batch::date::DateParts) -> i64 {
    let (month, day) = (i64::from(date.month), i64::from(date.day));
    let year = i64::from(date.year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `then` (local) as `relative` writes it, `now` being the local time now and `age_secs` how
/// long ago `then` was: `5 min ago` within the hour (at least 1), `Today 14:05`, `Yesterday
/// 14:05`; `None` (the system's format) for older times and times in the future.
pub fn relative_date(
    now: &crate::batch::date::DateParts,
    then: &crate::batch::date::DateParts,
    age_secs: i64,
) -> Option<String> {
    if age_secs < 0 {
        return None;
    }
    if age_secs < 3600 {
        return Some(format!("{} min ago", (age_secs / 60).max(1)));
    }
    let clock = format!("{:02}:{:02}", then.hour, then.minute);
    match day_number(now) - day_number(then) {
        0 => Some(format!("Today {clock}")),
        1 => Some(format!("Yesterday {clock}")),
        _ => None,
    }
}
```

`lib.rs`:

```rust
/// Formats a byte count for display, e.g. `1.5 KB` (binary steps; see `format_size_in`).
pub fn format_size(bytes: u64) -> String {
    format_size_in(bytes, view::SizeFormat::Binary)
}

/// A byte count as `format` writes it: `1.5 KB` (steps of 1024) or `1.5 kB` (steps of 1000).
pub fn format_size_in(bytes: u64, format: view::SizeFormat) -> String {
    let (step, units): (f64, [&str; 5]) = match format {
        view::SizeFormat::Binary => (1024.0, ["B", "KB", "MB", "GB", "TB"]),
        view::SizeFormat::Decimal => (1000.0, ["B", "kB", "MB", "GB", "TB"]),
    };
    if (bytes as f64) < step {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= step && unit < units.len() - 1 {
        value /= step;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}
```

`ops/rate.rs`: `pub fn format_rate(per_second: f64, format: crate::view::SizeFormat) -> String { format!("{}/s", crate::format_size_in(per_second.max(0.0) as u64, format)) }`; var olan `format_rate` testleri `SizeFormat::Binary` alır. `crates/gezik/src/operations.rs`: `format_rate(speed, gezik_core::view::SizeFormat::Binary)` (Task 6 ayarı bağlar).

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-core` → PASS; `cargo build --workspace -j 8`, clippy, çapraz denetimler.
- [ ] **Step 5: Commit** "Add the date and size formats and the view options that apply to every folder".

---
### Task 3: `gezik-config` — `PinEntry`, `pinned` okuma/yazma, liste işlemleri (`pins.rs`)

**Files:**
- Create: `crates/gezik-config/src/pins.rs`
- Modify: `crates/gezik-config/src/lib.rs` (`pub mod pins;`, `paths`'tan sonra), `crates/gezik-config/src/settings.rs` (`Settings.pinned: Vec<PinEntry>`, okuma, testler), `crates/gezik-config/src/settings_edit.rs` (`with_pinned`), `crates/gezik-config/src/settings_writer.rs` (`SettingsChange::Pinned(Vec<PinEntry>)`, test), `crates/gezik-config/src/store.rs` (`save_pinned(&[PinEntry])`, testler), `crates/gezik-config/templates/settings.toml` (`pinned` yorumu); derlemenin istediği: `crates/gezik/src/sidebar.rs` (`PinEntry` ile; eski liste yardımcıları `pins`'e taşınır)

**Interfaces:**
- Produces:

```rust
// pins.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinEntry { pub path: String, pub name: Option<String>, pub group: Option<String> }
impl PinEntry { pub fn plain(path: impl Into<String>) -> PinEntry; }
pub(crate) fn parse_pin(value: &toml::Value) -> Result<PinEntry, String>;
pub(crate) fn pin_to_value(pin: &PinEntry) -> toml_edit::Value;
pub fn same_path_text(a: &str, b: &str) -> bool;            // ignoring case on Windows
pub fn same_group(a: &str, b: &str) -> bool;                // ignoring case
pub fn find(list: &[PinEntry], path: &str) -> Option<usize>;
pub fn paths(list: &[PinEntry]) -> Vec<&str>;
pub fn normalize(list: Vec<PinEntry>) -> Vec<PinEntry>;     // ungrouped first, then groups
pub fn groups(list: &[PinEntry]) -> Vec<String>;
pub fn pin(list: &mut Vec<PinEntry>, path: String) -> bool;
pub fn unpin(list: &mut Vec<PinEntry>, index: usize) -> bool;
pub fn place(list: &mut Vec<PinEntry>, paths: Vec<String>, anchor: usize, after: bool) -> bool;
pub fn set_group(list: &mut Vec<PinEntry>, index: usize, group: Option<&str>) -> bool;
pub fn set_name(list: &mut Vec<PinEntry>, index: usize, name: Option<&str>) -> bool;
pub fn move_group(list: &mut Vec<PinEntry>, group: &str, up: bool) -> bool;
pub fn rename_group(list: &mut Vec<PinEntry>, from: &str, to: &str) -> bool;
pub fn ungroup(list: &mut Vec<PinEntry>, group: &str) -> bool;
// settings_edit.rs
pub fn with_pinned(text: &str, pinned: &[PinEntry]) -> Result<String, String>;
// settings_writer.rs
SettingsChange::Pinned(Vec<PinEntry>)
// sidebar.rs (gezik)
pub struct Pin { pub entry: PinEntry, pub path: PathBuf }
pub fn check_pins(dirs: &KnownDirs, pinned: &[PinEntry], exists: impl Fn(&Path) -> bool) -> Vec<Pin>;
impl Sidebar { pub fn set_pinned(&self, pinned: Vec<PinEntry>); pub fn pinned(&self) -> Vec<PinEntry>;
               fn edit(&self, change: impl FnOnce(&mut Vec<PinEntry>) -> bool); }
```

- [ ] **Step 1: Write the failing tests**

`pins.rs`'in sonunda:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, group: Option<&str>) -> PinEntry {
        PinEntry { path: path.into(), name: None, group: group.map(str::to_owned) }
    }

    fn plain(list: &[&str]) -> Vec<PinEntry> {
        list.iter().map(|path| PinEntry::plain(*path)).collect()
    }

    /// `Group:/path`, or `/path` without a group.
    fn shown(list: &[PinEntry]) -> Vec<String> {
        list.iter()
            .map(|pin| match &pin.group {
                Some(group) => format!("{group}:{}", pin.path),
                None => pin.path.clone(),
            })
            .collect()
    }

    #[test]
    fn normalize_puts_groups_together_and_trims() {
        let list = vec![
            entry("/w1", Some("Work")),
            entry("/a", None),
            entry("/m", Some("Media")),
            entry("/w2", Some(" work ")),
            PinEntry { path: "/b".into(), name: Some("  ".into()), group: Some(String::new()) },
        ];
        let tidy = normalize(list);
        assert_eq!(shown(&tidy), ["/a", "/b", "Work:/w1", "Work:/w2", "Media:/m"]);
        assert_eq!(tidy[1], PinEntry::plain("/b"), "an empty alias and group are none");
        assert_eq!(groups(&tidy), ["Work", "Media"]);
        assert_eq!(normalize(tidy.clone()), tidy, "a second time changes nothing");
    }

    #[test]
    fn place_moves_and_groups() {
        let mut list = plain(&["/a", "/b", "/c"]);
        assert!(place(&mut list, vec!["/x".into(), "/y".into()], 1, false));
        assert_eq!(paths(&list), ["/a", "/x", "/y", "/b", "/c"]);
        assert!(place(&mut list, vec!["/c".into()], 0, false));
        assert_eq!(paths(&list), ["/c", "/a", "/x", "/y", "/b"], "a pinned one moves");
        assert!(place(&mut list, vec!["/a".into()], 4, true));
        assert_eq!(paths(&list), ["/c", "/x", "/y", "/b", "/a"], "after the anchor, the gap counted");
        assert!(!place(&mut list, vec!["/b".into()], 3, false), "onto itself: nothing");
        assert!(!place(&mut list, vec!["/z".into()], 99, false), "no such anchor");

        let mut list = vec![
            PinEntry { path: "/a".into(), name: Some("Alias".into()), group: None },
            entry("/w1", Some("Work")),
            entry("/w2", Some("Work")),
            entry("/gone", Some("Media")),
            entry("/m", Some("Media")),
        ];
        assert!(place(&mut list, vec!["/a".into()], 2, false));
        assert_eq!(shown(&list), ["Work:/w1", "Work:/a", "Work:/w2", "Media:/gone", "Media:/m"]);
        assert_eq!(list[1].name.as_deref(), Some("Alias"), "the alias goes along");
        assert!(place(&mut list, vec!["/new".into()], 4, true));
        assert_eq!(shown(&list).last().map(String::as_str), Some("Media:/new"));
    }

    #[test]
    fn groups_move_rename_and_dissolve() {
        let mut list = vec![
            entry("/a", None),
            entry("/w", Some("Work")),
            entry("/m", Some("Media")),
            entry("/p", Some("Photos")),
        ];
        assert!(move_group(&mut list, "media", true));
        assert_eq!(groups(&list), ["Media", "Work", "Photos"]);
        assert!(!move_group(&mut list, "Media", true), "the first goes no higher");
        assert!(!move_group(&mut list, "Photos", false), "the last no lower");
        assert!(rename_group(&mut list, "Work", "Jobs"));
        assert_eq!(groups(&list), ["Media", "Jobs", "Photos"]);
        assert!(!rename_group(&mut list, "Jobs", "  "), "no empty name");
        assert!(rename_group(&mut list, "Photos", "media"), "a name taken joins that group");
        assert_eq!(shown(&list), ["/a", "Media:/m", "Media:/p", "Jobs:/w"]);
        assert!(ungroup(&mut list, "MEDIA"));
        assert_eq!(shown(&list), ["/a", "/m", "/p", "Jobs:/w"]);
        assert!(!ungroup(&mut list, "Nope"));
    }

    #[test]
    fn a_pin_changes_group_and_alias() {
        let mut list = vec![entry("/a", None), entry("/b", None), entry("/w", Some("Work"))];
        assert!(set_group(&mut list, 0, Some("work")));
        assert_eq!(shown(&list), ["/b", "Work:/w", "Work:/a"], "at the group's end, spelled as it is");
        assert!(set_group(&mut list, 0, Some("New")));
        assert_eq!(shown(&list), ["Work:/w", "Work:/a", "New:/b"], "a new group goes last");
        assert!(!set_group(&mut list, 0, Some("WORK")), "its own group");
        assert!(set_group(&mut list, 2, None));
        assert_eq!(shown(&list), ["/b", "Work:/w", "Work:/a"]);
        assert!(set_name(&mut list, 0, Some(" Bee ")));
        assert_eq!(list[0].name.as_deref(), Some("Bee"));
        assert!(set_name(&mut list, 0, Some("")) && list[0].name.is_none());
        assert!(!set_name(&mut list, 0, None));
    }

    #[test]
    fn pinning_twice_is_once() {
        let mut list = plain(&["/a"]);
        assert!(!pin(&mut list, "/a".into()));
        assert!(pin(&mut list, "/b".into()));
        assert_eq!(paths(&list), ["/a", "/b"]);
        let mut grouped = vec![entry("/w", Some("Work"))];
        assert!(pin(&mut grouped, "/n".into()));
        assert_eq!(paths(&grouped), ["/n", "/w"], "a new pin has no group: before the groups");
        assert!(unpin(&mut grouped, 1) && !unpin(&mut grouped, 5));
    }

    #[cfg(windows)]
    #[test]
    fn paths_match_ignoring_case_on_windows() {
        let mut list = plain(&["C:/ÇALIŞMA"]);
        assert!(!pin(&mut list, "c:/çalişma".into()));
        assert_eq!(find(&list, "c:/ÇaLiŞma"), Some(0));
    }
}
```

`settings.rs` testleri (`use crate::pins::{self, PinEntry};` test modülünde):

```rust
    #[test]
    fn pins_of_both_forms_are_read_and_bad_ones_warned() {
        let (settings, warnings) = parse(
            "pinned = [\n  \"{documents}/Projects\",\n  { path = \"D:/Work/gezik\", name = \" Gezik \", group = \"Work\" },\n  \
             { path = \"//nas/foto\", group = \"Media\" },\n  { path = \"/x\", icon = \"star\" },\n  { name = \"no path\" },\n  \
             \"\",\n  \"{home}/../etc\",\n  3,\n  \"{documents}/Projects\",\n  { path = \"/n\", name = 5 },\n]\n",
        );
        assert_eq!(
            settings.pinned,
            [
                PinEntry::plain("{documents}/Projects"),
                PinEntry { path: "D:/Work/gezik".into(), name: Some("Gezik".into()), group: Some("Work".into()) },
                PinEntry { path: "//nas/foto".into(), name: None, group: Some("Media".into()) },
            ]
        );
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 7, "{messages:?}");
        assert!(messages[0].starts_with("pinned: unknown key \"icon\" in "), "{messages:?}");
        assert!(messages[1].starts_with("pinned: path is missing in "), "{messages:?}");
        assert_eq!(messages[2], "pinned: path is missing in \"\"");
        assert_eq!(messages[3], "pinned: \"{home}/../etc\" must not contain \"..\"");
        assert_eq!(messages[4], "pinned: expected text or { path, name, group }, got 3");
        assert_eq!(messages[5], "pinned: \"{documents}/Projects\" is pinned twice; the second is left out");
        assert_eq!(messages[6], "pinned: name must be text, got 5");
    }

    #[cfg(windows)]
    #[test]
    fn a_pin_twice_in_other_case_is_one_on_windows() {
        let (settings, warnings) = parse("pinned = [\"C:/Work\", \"c:/work\"]\n");
        assert_eq!(pins::paths(&settings.pinned), ["C:/Work"]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    #[test]
    fn the_template_pinned_example_reads_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let line = template.lines().find(|l| l.starts_with("# pinned = [")).expect("the template shows a pinned example");
        let (settings, warnings) = parse(&format!("{}\n", &line[2..]));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.pinned.len(), 2);
        assert_eq!(settings.pinned[1].group.as_deref(), Some("Work"));
    }
```
  Var olan testler: `reads_start_folder_and_pinned` ve `pinned_keeps_entries_that_do_not_exist_here` `assert_eq!(pins::paths(&settings.pinned), [...])` olur; `pinned_skips_duplicates_and_parent_segments` artık yinelenen için de uyarır: `assert_eq!(messages.len(), 3)` ve `assert!(messages.iter().any(|m| m.contains("pinned twice")))` (`expected text` ve `..` denetimleri kalır).

`settings_edit.rs` testleri (`pins` yardımcısı `Vec<PinEntry>` döner: `list.iter().map(|s| PinEntry::plain(*s)).collect()`; var olan `with_pinned` testleri böyle derlenir):

```rust
    #[test]
    fn old_lists_are_written_as_they_were() {
        let text = "# mine\npinned = [\"{documents}/Projects\", \"D:/Work\"]\n";
        let out = with_pinned(text, &pins(&["{documents}/Projects", "D:/Work", "/new"])).unwrap();
        assert_eq!(out, "# mine\npinned = [\"{documents}/Projects\", \"D:/Work\", \"/new\"]\n");
    }

    #[test]
    fn aliases_and_groups_are_inline_tables_one_per_line() {
        use crate::settings::Settings;
        let list = [
            PinEntry::plain("{documents}/Projects"),
            PinEntry { path: "D:/Work/gezik".into(), name: Some("Gezik".into()), group: Some("Work".into()) },
            PinEntry { path: "//nas/foto".into(), name: None, group: Some("Media".into()) },
        ];
        let out = with_pinned("pinned = []\n\n[layout]\nsidebar = \"left\"\n", &list).unwrap();
        // The spec's example (6.1), as written.
        assert!(
            out.starts_with(
                "pinned = [\n  \"{documents}/Projects\",\n  { path = \"D:/Work/gezik\", name = \"Gezik\", group = \"Work\" },\n  \
                 { path = \"//nas/foto\", group = \"Media\" },\n]\n"
            ),
            "{out}"
        );
        let mut warnings = Vec::new();
        assert_eq!(Settings::parse("settings.toml", &out, &mut warnings).pinned, list);
        assert!(warnings.is_empty(), "{warnings:?}");
        let back = with_pinned(&out, &pins(&["/a"])).unwrap();
        assert!(back.starts_with("pinned = [\"/a\"]\n"), "no alias or group left: one line again: {back}");
    }

    #[test]
    fn unreadable_pins_are_kept_after_ours() {
        use crate::settings::Settings;
        let text = "pinned = [\"/a\", { path = \"/b\", icon = \"star\" }, \"{home}/../x\", { name = \"no path\" }]\n";
        let out = with_pinned(text, &pins(&["/c"])).unwrap();
        let parsed = out.parse::<toml::Table>().unwrap();
        let items = parsed["pinned"].as_array().unwrap();
        assert_eq!(items.len(), 4, "{out}");
        assert_eq!(items[0].as_str(), Some("/c"));
        assert_eq!(items[1]["icon"].as_str(), Some("star"));
        assert_eq!(items[2].as_str(), Some("{home}/../x"));
        assert_eq!(items[3]["name"].as_str(), Some("no path"));
        let mut warnings = Vec::new();
        assert_eq!(Settings::parse("settings.toml", &out, &mut warnings).pinned, pins(&["/c"]));
        assert_eq!(warnings.len(), 3, "the kept ones still warn: {warnings:?}");
    }
```

`crates/gezik/src/sidebar.rs` testleri: `dropped_folders_are_pinned_where_they_land`, `pin_is_idempotent`, `pin_ignores_case_on_windows`, `pin_ignores_non_ascii_case_on_windows`, `move_entry_reorders` silinir (karşılıkları `pins.rs`'te). `strings` yardımcısı `fn plain(items: &[&str]) -> Vec<PinEntry>` olur; `pin(entry, path)` `Pin { entry: PinEntry::plain(entry), path: PathBuf::from(path) }`; `check_pins_keeps_existing_entries_in_order`, `a_new_list_keeps_known_pins_until_checked`, `an_overtaken_check_is_dropped` bu yardımcılarla aynı kalır. Yeni:

```rust
    #[test]
    fn a_new_alias_shows_at_once() {
        let mut pins = PinState::default();
        pins.set(plain(&["/a"]));
        let ticket = pins.begin_check();
        pins.finish_check(ticket, vec![pin("/a", "/a")]);
        let named = PinEntry { path: "/a".into(), name: Some("Alpha".into()), group: None };
        assert!(pins.set(vec![named.clone()]));
        assert_eq!(pins.visible, [Pin { entry: named, path: PathBuf::from("/a") }], "no check needed");
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-config` → derlenmez (`pins` modülü yok).

- [ ] **Step 3: Implement.** `pins.rs`:

```rust
//! Pinned folders (`pinned` in settings.toml, spec 6): each a path, or an inline table
//! `{ path, name, group }` for an alias and a group. Reading and writing them, and the changes
//! the sidebar makes to the list. Pure: the paths are text with `{home}`-style tokens.

/// One pinned folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinEntry {
    /// With `{home}`-style tokens and `/` separators.
    pub path: String,
    /// The alias the sidebar shows instead of the folder's name.
    pub name: Option<String>,
    pub group: Option<String>,
}

impl PinEntry {
    /// A pin without alias and group (written as plain text).
    pub fn plain(path: impl Into<String>) -> PinEntry {
        PinEntry { path: path.into(), name: None, group: None }
    }
}

/// Text that is empty once trimmed is none.
fn trimmed(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// One item of `pinned`; the error is why it is left out (settings.toml warns, and Gezik
/// keeps it in the file as it is).
pub(crate) fn parse_pin(value: &toml::Value) -> Result<PinEntry, String> {
    let pin = match value {
        toml::Value::String(path) => PinEntry::plain(path.as_str()),
        toml::Value::Table(table) => {
            if let Some(key) = table.keys().find(|k| !matches!(k.as_str(), "path" | "name" | "group")) {
                return Err(format!("unknown key \"{key}\" in {value}"));
            }
            let text = |key: &str| match table.get(key) {
                None => Ok(None),
                Some(toml::Value::String(text)) => Ok(Some(text.as_str())),
                Some(other) => Err(format!("{key} must be text, got {other}")),
            };
            let path = text("path")?.unwrap_or_default().to_owned();
            PinEntry { path, name: trimmed(text("name")?), group: trimmed(text("group")?) }
        }
        other => return Err(format!("expected text or {{ path, name, group }}, got {other}")),
    };
    if pin.path.trim().is_empty() {
        return Err(format!("path is missing in {value}"));
    }
    if crate::paths::has_parent_segment(&pin.path) {
        return Err(format!("\"{}\" must not contain \"..\"", pin.path));
    }
    Ok(pin)
}

/// `pin` as settings.toml keeps it: plain text without alias and group, else an inline table
/// (`path`, `name`, `group`; the ones it has).
pub(crate) fn pin_to_value(pin: &PinEntry) -> toml_edit::Value {
    if pin.name.is_none() && pin.group.is_none() {
        return toml_edit::Value::from(pin.path.as_str());
    }
    let mut table = toml_edit::InlineTable::new();
    table.insert("path", pin.path.as_str().into());
    if let Some(name) = &pin.name {
        table.insert("name", name.as_str().into());
    }
    if let Some(group) = &pin.group {
        table.insert("group", group.as_str().into());
    }
    table.fmt();
    toml_edit::Value::InlineTable(table)
}

/// Whether two pinned paths are one folder's: ignoring case (also of non-ASCII letters like
/// Ç) on Windows, exactly elsewhere.
pub fn same_path_text(a: &str, b: &str) -> bool {
    a == b || (cfg!(windows) && a.to_lowercase() == b.to_lowercase())
}

/// Whether two group names are one group: ignoring case.
pub fn same_group(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Where `path` is pinned in `list`.
pub fn find(list: &[PinEntry], path: &str) -> Option<usize> {
    list.iter().position(|pin| same_path_text(&pin.path, path))
}

/// The paths of `list`, in order.
pub fn paths(list: &[PinEntry]) -> Vec<&str> {
    list.iter().map(|pin| pin.path.as_str()).collect()
}

/// `list` in the order the sidebar shows it (spec 6.2): the pins without a group first, then
/// each group in the order of its first pin, its pins in their order. Aliases and groups are
/// trimmed (empty: none); a group's pins take the spelling of its first one.
pub fn normalize(list: Vec<PinEntry>) -> Vec<PinEntry> {
    let mut ungrouped = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut members: Vec<Vec<PinEntry>> = Vec::new();
    for mut pin in list {
        pin.name = trimmed(pin.name.as_deref());
        pin.group = trimmed(pin.group.as_deref());
        let Some(group) = pin.group.clone() else {
            ungrouped.push(pin);
            continue;
        };
        let i = match names.iter().position(|name| same_group(name, &group)) {
            Some(i) => i,
            None => {
                names.push(group);
                members.push(Vec::new());
                names.len() - 1
            }
        };
        pin.group = Some(names[i].clone());
        members[i].push(pin);
    }
    ungrouped.into_iter().chain(members.into_iter().flatten()).collect()
}

/// The groups of `list`, in the order of their first pins.
pub fn groups(list: &[PinEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for group in list.iter().filter_map(|pin| pin.group.as_deref()) {
        if !out.iter().any(|name| same_group(name, group)) {
            out.push(group.to_owned());
        }
    }
    out
}

fn tidy(list: &mut Vec<PinEntry>) {
    *list = normalize(std::mem::take(list));
}

fn in_group(pin: &PinEntry, group: &str) -> bool {
    pin.group.as_deref().is_some_and(|own| same_group(own, group))
}

/// Pins `path` (no alias, no group) unless it is pinned already. Returns whether it was added.
pub fn pin(list: &mut Vec<PinEntry>, path: String) -> bool {
    if find(list, &path).is_some() {
        return false;
    }
    list.push(PinEntry::plain(path));
    tidy(list);
    true
}

pub fn unpin(list: &mut Vec<PinEntry>, index: usize) -> bool {
    if index >= list.len() {
        return false;
    }
    list.remove(index);
    true
}

/// Puts the folders `paths` next to pin `anchor` (before it, or after it with `after`), in its
/// group: one already pinned moves there with its alias, a new one is added. Returns whether
/// the list changed (a pin dropped on itself changes nothing).
pub fn place(list: &mut Vec<PinEntry>, paths: Vec<String>, anchor: usize, after: bool) -> bool {
    let Some(target) = list.get(anchor).cloned() else { return false };
    if paths.iter().any(|path| same_path_text(path, &target.path)) {
        return false;
    }
    let before = list.clone();
    let mut moving: Vec<PinEntry> = Vec::new();
    for path in paths {
        let pin = match find(list, &path) {
            Some(i) => list.remove(i),
            None => PinEntry::plain(path),
        };
        if !moving.iter().any(|m| same_path_text(&m.path, &pin.path)) {
            moving.push(PinEntry { group: target.group.clone(), ..pin });
        }
    }
    let Some(at) = find(list, &target.path) else { return false };
    let at = if after { at + 1 } else { at };
    list.splice(at..at, moving);
    tidy(list);
    *list != before
}

/// Moves pin `index` into `group` (none: no group), at the group's end; a new group goes last.
pub fn set_group(list: &mut Vec<PinEntry>, index: usize, group: Option<&str>) -> bool {
    let group = trimmed(group);
    let Some(pin) = list.get(index) else { return false };
    let same = match (&pin.group, &group) {
        (None, None) => true,
        (Some(own), Some(new)) => same_group(own, new),
        _ => false,
    };
    if same {
        return false;
    }
    let mut pin = list.remove(index);
    pin.group = group;
    list.push(pin);
    tidy(list);
    true
}

/// Gives pin `index` the alias `name` (none or empty: the folder's own name again).
pub fn set_name(list: &mut Vec<PinEntry>, index: usize, name: Option<&str>) -> bool {
    let name = trimmed(name);
    match list.get_mut(index) {
        Some(pin) if pin.name != name => {
            pin.name = name;
            true
        }
        _ => false,
    }
}

/// Swaps `group`'s pins with the group before it (`up`) or after it.
pub fn move_group(list: &mut Vec<PinEntry>, group: &str, up: bool) -> bool {
    let mut order = groups(list);
    let Some(i) = order.iter().position(|name| same_group(name, group)) else { return false };
    let Some(j) = (if up { i.checked_sub(1) } else { Some(i + 1) }).filter(|j| *j < order.len()) else {
        return false;
    };
    order.swap(i, j);
    let all = std::mem::take(list);
    let mut out: Vec<PinEntry> = all.iter().filter(|pin| pin.group.is_none()).cloned().collect();
    for name in &order {
        out.extend(all.iter().filter(|pin| in_group(pin, name)).cloned());
    }
    *list = out;
    true
}

/// Renames group `from` to `to` (trimmed, not empty); a name another group has joins the two.
pub fn rename_group(list: &mut Vec<PinEntry>, from: &str, to: &str) -> bool {
    let to = to.trim();
    if to.is_empty() {
        return false;
    }
    let mut changed = false;
    for pin in list.iter_mut().filter(|pin| in_group(pin, from)) {
        if pin.group.as_deref() != Some(to) {
            pin.group = Some(to.to_owned());
            changed = true;
        }
    }
    if changed {
        tidy(list);
    }
    changed
}

/// Takes `group`'s pins out of it: they go after the pins without a group.
pub fn ungroup(list: &mut Vec<PinEntry>, group: &str) -> bool {
    let mut changed = false;
    for pin in list.iter_mut().filter(|pin| in_group(pin, group)) {
        pin.group = None;
        changed = true;
    }
    if changed {
        tidy(list);
    }
    changed
}
```

  `settings.rs`: `use crate::pins::{PinEntry, find as find_pin, parse_pin};`; alan belgesi "Pinned folders as written (tokenized, `/` separators), in the file's order (the sidebar shows them grouped: `pins::normalize`)." ve `pub pinned: Vec<PinEntry>`; okuma:

```rust
        if let Some(value) = table.get("pinned") {
            match value.as_array() {
                None => warnings.push(Warning::new(file, format!("pinned: expected a list of folders, got {value}"))),
                Some(items) => {
                    for item in items {
                        match parse_pin(item) {
                            Ok(pin) if find_pin(&settings.pinned, &pin.path).is_some() => warnings.push(Warning::new(
                                file,
                                format!("pinned: \"{}\" is pinned twice; the second is left out", pin.path),
                            )),
                            Ok(pin) => settings.pinned.push(pin),
                            Err(err) => warnings.push(Warning::new(file, format!("pinned: {err}"))),
                        }
                    }
                }
            }
        }
```

  `settings_edit.rs` (modül belgesi: "only `pinned`, the `[view]` values or the saved tables change"; `use crate::pins::{PinEntry, parse_pin, pin_to_value};`):

```rust
/// Returns `text` with `pinned` set to `pinned`: plain text for a pin without alias and group,
/// an inline table for the others. The entries already there that Gezik cannot read (an unknown
/// key, no path, `..`) stay, after `pinned`. All plain: one line, as before 7b; with a table:
/// one entry per line (spec 6.1). Errors with the parser's message if the file is not valid
/// TOML (the caller must then leave the file alone).
pub fn with_pinned(text: &str, pinned: &[PinEntry]) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if let Some(existing) = doc.get("pinned")
        && !existing.is_array()
    {
        return Err("pinned must be a list".to_owned());
    }
    let plain = text.parse::<toml::Table>().map_err(|err| err.to_string().trim().to_owned())?;
    let unreadable: Vec<usize> = plain
        .get("pinned")
        .and_then(|value| value.as_array())
        .map(|items| (0..items.len()).filter(|&i| parse_pin(&items[i]).is_err()).collect())
        .unwrap_or_default();
    let kept: Vec<toml_edit::Value> = doc
        .get("pinned")
        .and_then(|item| item.as_array())
        .map(|array| unreadable.iter().filter_map(|&i| array.get(i).cloned()).collect())
        .unwrap_or_default();
    let mut values: Vec<toml_edit::Value> = pinned.iter().map(pin_to_value).collect();
    values.extend(kept);
    let one_per_line = values.iter().any(|value| !value.is_str());
    if doc.get("pinned").is_none() {
        doc["pinned"] = toml_edit::value(toml_edit::Array::new());
    }
    let Some(array) = doc["pinned"].as_array_mut() else { return Err("pinned must be a list".to_owned()) };
    array.clear();
    for value in values {
        array.push_formatted(value);
    }
    array.fmt();
    if one_per_line {
        for value in array.iter_mut() {
            value.decor_mut().set_prefix("\n  ");
            value.decor_mut().set_suffix("");
        }
        array.set_trailing("\n");
        array.set_trailing_comma(true);
    }
    Ok(doc.to_string())
}
```
  (`toml_edit`'in boşluğu testteki metinden farklı çıkarsa test değil kod düzeltilir: test spec §6.1'in örneğidir.)
  `settings_writer.rs`: `/// The pinned folders (`pinned`).` `Pinned(Vec<PinEntry>)`; test `SettingsChange::Pinned(vec![PinEntry::plain(format!("/p{i}"))])` ve `assert_eq!(crate::pins::paths(&settings.pinned), ["/p19"], …)`. `store.rs`: `pub fn save_pinned(&self, pinned: &[PinEntry]) -> Result<(), Warning>`; testleri `&[PinEntry::plain("/a")]` ve `crate::pins::paths(&loaded.settings.pinned) == ["/a"]`.
  Şablon (`pinned = []`'ın üstündeki yorum; son cümle bugünkü gibi):

```toml
# Folders pinned to the sidebar: a path, or { path, name, group } for an alias and a group:
# pinned = ["{documents}/Projects", { path = "D:/Work/gezik", name = "Gezik", group = "Work" }]
# Gezik updates this list when you pin, unpin, rename, group or reorder folders; your
# comments elsewhere in this file are kept.
# Comments inside the pinned list itself are not kept when Gezik updates it.
pinned = []
```

  `crates/gezik/src/sidebar.rs` (davranış bugünkü gibi; gruplar Task 8'de çizilir, takma ad şimdiden etiket olur):

```rust
use gezik_config::pins::{self, PinEntry};

/// A pinned entry that exists on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// As settings.toml has it (the path with tokens, the alias, the group).
    pub entry: PinEntry,
    pub path: PathBuf,
}

/// The pinned entries that exist, in `pinned` order. `exists` checks the file system, so
/// this runs off the UI thread.
pub fn check_pins(dirs: &KnownDirs, pinned: &[PinEntry], exists: impl Fn(&Path) -> bool) -> Vec<Pin> {
    pinned
        .iter()
        .filter_map(|entry| {
            let path = dirs.expand_checked(&entry.path).filter(|p| exists(p))?;
            Some(Pin { entry: entry.clone(), path })
        })
        .collect()
}
```
  `PinState.pinned: Vec<PinEntry>` (belge: "As settings.toml has it, in the order the sidebar shows it (`pins::normalize`)"):

```rust
    fn set(&mut self, pinned: Vec<PinEntry>) -> bool {
        let pinned = pins::normalize(pinned);
        if self.pinned == pinned {
            return false;
        }
        let known = std::mem::take(&mut self.visible);
        self.visible = pinned
            .iter()
            .filter_map(|entry| {
                let pin = known.iter().find(|pin| pin.entry.path == entry.path)?;
                Some(Pin { entry: entry.clone(), path: pin.path.clone() })
            })
            .collect();
        self.pinned = pinned;
        true
    }

    fn stored_index(&self, index: usize) -> Option<usize> {
        let pin = self.visible.get(index)?;
        pins::find(&self.pinned, &pin.entry.path)
    }
```
  `Inner.pins_in_file: Vec<PinEntry>`; `set_pinned`, `show_pinned`, `pinned()`, `save`, `pins_written` `Vec<PinEntry>` alır/verir. Değişiklikler tek kapıdan:

```rust
    /// Changes the pinned list with `change`; saves it if `change` says it changed.
    fn edit(&self, change: impl FnOnce(&mut Vec<PinEntry>) -> bool) {
        let mut pinned = self.pinned();
        if change(&mut pinned) {
            self.save(pinned);
        }
    }

    pub fn pin(&self, path: PathBuf) {
        let entry = self.0.borrow().dirs.collapse(&path);
        self.edit(|list| pins::pin(list, entry));
    }

    /// Pins the folders `paths` at shown position `position` of the PINNED section (dropped
    /// between two pinned folders); a folder already pinned moves there.
    pub fn pin_at(&self, paths: &[PathBuf], position: usize) {
        let (entries, anchor) = {
            let inner = self.0.borrow();
            let entries: Vec<String> = paths.iter().map(|path| inner.dirs.collapse(path)).collect();
            let last = inner.pins.visible.len().checked_sub(1);
            let anchor = match inner.pins.stored_index(position) {
                Some(i) => Some((i, false)),
                None => last.and_then(|last| inner.pins.stored_index(last)).map(|i| (i, true)),
            };
            (entries, anchor)
        };
        self.edit(|list| match anchor {
            Some((at, after)) => pins::place(list, entries, at, after),
            None => entries.into_iter().fold(false, |changed, entry| pins::pin(list, entry) | changed),
        });
    }

    pub fn unpin(&self, index: usize) {
        let stored = self.0.borrow().pins.stored_index(index);
        if let Some(stored) = stored {
            self.edit(|list| pins::unpin(list, stored));
        }
    }

    pub fn move_pinned(&self, from: usize, to: usize) {
        let target = {
            let pins = &self.0.borrow().pins;
            let to = to.min(pins.visible.len().saturating_sub(1));
            pins.stored_index(from).zip(pins.stored_index(to)).map(|(f, t)| (f, t, to > from))
        };
        let Some((from, to, after)) = target else { return };
        self.edit(|list| {
            let path = list[from].path.clone();
            pins::place(list, vec![path], to, after)
        });
    }
```
  `update_rows`'ta etiket `pin.entry.name.clone().unwrap_or_else(|| places.title_for(&Location::Path(pin.path.clone())))`. `pin_entry`, `insert_entries`, `move_entry` silinir; `same_path` ve onun yerel `same_text`'i kalır.

- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS (şablonu yazan var olan testler `presets_go_after_the_template_comments`, `filters_and_presets_together_leave_the_template_as_it_was`, `tab_sets_are_written_keeping_the_rest` dahil); clippy, fmt, çapraz denetimler.
- [ ] **Step 5: Commit** "Read and write pinned folders with an alias and a group, keeping the plain list as it was".

---

### Task 4: `gezik-config` — `[view]` seçenekleri, `ViewOption`, `pin-1…pin-9`; rakam tuşu kuralı

**Files:**
- Modify: `crates/gezik-config/src/settings.rs` (`ViewDefaults.options`, okuma, `ViewOption`), `crates/gezik-config/src/settings_edit.rs` (`with_view_option`, ortak `edit_view`), `crates/gezik-config/src/settings_writer.rs` (`SettingsChange::ViewOption`), `crates/gezik-config/src/shortcuts.rs` (dokuz eylem), `crates/gezik-config/templates/settings.toml` (`[view]` satırları, `[shortcuts]` örneği); `crates/gezik/src/keys.rs` (rakam kuralı, testler); derleme kolları `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs` (`handle_key`)

**Interfaces:**
- Consumes: Task 2 (`ViewOptions`, `DateFormat`, `SizeFormat`).
- Produces:

```rust
// settings.rs
pub struct ViewDefaults { pub view: ViewSettings, pub icons: IconMode, pub thumbnails: bool, pub options: ViewOptions }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewOption { HideExtensions(bool), FoldersFirst(bool), DateFormat(DateFormat), SizeFormat(SizeFormat),
                      SingleClickOpen(bool), ShowHidden(bool), ShowSystem(bool) }
impl ViewOption { pub fn key(self) -> &'static str; pub fn apply(self, options: &mut ViewOptions); }
// settings_edit.rs
pub fn with_view_option(text: &str, option: ViewOption) -> Result<String, String>;
// settings_writer.rs
SettingsChange::ViewOption(ViewOption)
// shortcuts.rs
pub enum Action { …, Pin1, Pin2, Pin3, Pin4, Pin5, Pin6, Pin7, Pin8, Pin9 }   // ALL: [Action; 59]
impl Action { pub fn pin_number(self) -> Option<usize>; pub fn pin(number: usize) -> Option<Action>; }
// keys.rs (gezik)
fn digit_chord(ctrl: bool, alt: bool, cmd: bool, platform: Platform) -> bool;
```

- [ ] **Step 1: Write the failing tests**

`settings.rs`:

```rust
    #[test]
    fn view_options_are_read() {
        use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
        let (settings, warnings) = parse(
            "[view]\nhide-extensions = true\nfolders-first = false\ndate-format = \"relative\"\nsize-format = \"decimal\"\n\
             single-click-open = true\nshow-hidden = false\nshow-system = true\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            settings.view.options,
            ViewOptions {
                hide_extensions: true,
                folders_first: false,
                date_format: DateFormat::Relative,
                size_format: SizeFormat::Decimal,
                single_click_open: true,
                show_hidden: false,
                show_system: true,
            }
        );
        assert_eq!(ViewDefaults::default().options, ViewOptions::default());
    }

    #[test]
    fn bad_view_options_keep_defaults_with_warnings() {
        let (settings, warnings) =
            parse("[view]\ndate-format = \"weekday\"\nsize-format = 1024\nhide-extensions = \"yes\"\n");
        assert_eq!(settings.view.options, gezik_core::view::ViewOptions::default());
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "view.hide-extensions: expected true or false, got \"yes\"",
                "view.date-format: expected \"relative\", \"short\", \"iso\" or \"system\", got \"weekday\"",
                "view.size-format: expected \"binary\" or \"decimal\", got 1024",
            ]
        );
    }

    #[test]
    fn a_view_option_sets_its_own_field() {
        use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
        let mut options = ViewOptions::default();
        for option in [
            ViewOption::HideExtensions(true),
            ViewOption::FoldersFirst(false),
            ViewOption::DateFormat(DateFormat::Iso),
            ViewOption::SizeFormat(SizeFormat::Decimal),
            ViewOption::SingleClickOpen(true),
            ViewOption::ShowHidden(false),
            ViewOption::ShowSystem(true),
        ] {
            option.apply(&mut options);
        }
        assert!(options.hide_extensions && !options.folders_first && options.single_click_open);
        assert!(!options.show_hidden && options.show_system);
        assert_eq!((options.date_format, options.size_format), (DateFormat::Iso, SizeFormat::Decimal));
        assert_eq!(ViewOption::SingleClickOpen(true).key(), "single-click-open");
        assert_eq!(ViewOption::DateFormat(DateFormat::Iso).key(), "date-format");
    }
```
  `the_template_reads_with_the_defaults`'a `assert_eq!(settings.view, ViewDefaults::default());` ve `assert!(settings.pinned.is_empty());`. `the_template_shortcut_examples_are_the_defaults_off_macos`'ta atlama `if action.tab_number().is_some_and(|n| n > 1) || action.pin_number().is_some_and(|n| n > 1) { continue; }` (yorum: "tab-2 … tab-8 and pin-2 … pin-9 likewise").

`settings_edit.rs`:

```rust
    #[test]
    fn a_view_option_is_written_keeping_its_comment() {
        use crate::settings::{Settings, ViewOption};
        use gezik_core::view::DateFormat;
        let template = include_str!("../templates/settings.toml");
        let out = with_view_option(template, ViewOption::DateFormat(DateFormat::Relative)).unwrap();
        let line = out.lines().find(|l| l.starts_with("date-format")).unwrap();
        assert!(
            line.starts_with("date-format = \"relative\"") && line.ends_with("# relative | short | iso | system"),
            "{line}"
        );
        // show-hidden is a comment in the template: the key is added, the comment stays.
        let out = with_view_option(template, ViewOption::ShowHidden(false)).unwrap();
        assert!(out.contains("# show-hidden = true"), "{out}");
        let settings = Settings::parse("settings.toml", &out, &mut Vec::new());
        assert!(!settings.view.options.show_hidden);
        assert_eq!(settings.view.view, gezik_core::view::ViewSettings::default(), "the rest of [view] stays");
        let added = with_view_option("theme = \"auto\"\n", ViewOption::HideExtensions(true)).unwrap();
        assert_eq!(added.parse::<toml::Table>().unwrap()["view"]["hide-extensions"].as_bool(), Some(true));
        assert!(with_view_option("view = 3\n", ViewOption::HideExtensions(true)).is_err());
        assert!(with_view_option("theme = \n", ViewOption::HideExtensions(true)).is_err());
    }
```

`shortcuts.rs`:

```rust
    #[test]
    fn pins_have_alt_and_cmd_option_digits() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        for n in 1..=9 {
            let action = Action::pin(n).unwrap();
            assert_eq!(action.name(), format!("pin-{n}"));
            assert_eq!(action.pin_number(), Some(n));
            assert_eq!(Action::from_name(&format!("pin-{n}")), Some(action));
            let alt = chord(&format!("alt+{n}"));
            assert_eq!(other.chord_for(action), Some(alt));
            assert_eq!(fixed_owner(&alt, Platform::Other), None);
            let cmd_option = mac_chord(&format!("mod+alt+{n}"));
            assert_eq!(mac.action_for(&cmd_option), Some(action));
            assert_eq!(fixed_owner(&cmd_option, Platform::Mac), None);
        }
        assert_eq!((Action::pin(0), Action::pin(10)), (None, None));
        assert_eq!(Action::ALL.len(), 59);
        assert_eq!(other.action_for(&chord("ctrl+1")), Some(Action::Tab1), "Ctrl+1 is still tab 1");
        assert_eq!(other.action_for(&chord("ctrl+alt+1")), None, "AltGr+1 types");
    }
```

`keys.rs` (gezik):

```rust
    #[test]
    fn alt_and_a_digit_key_is_the_pin_on_every_layout() {
        let defaults = Shortcuts::defaults(Platform::Other);
        let alt = |text: &str, digit: char| {
            chord_from_press(text, Physical::Digit(digit), false, true, false, false, Platform::Other)
                .and_then(|c| defaults.action_for(&c))
        };
        assert_eq!(alt("1", '1'), Some(Action::Pin1), "US and Turkish Q");
        assert_eq!(alt("&", '1'), Some(Action::Pin1), "AZERTY: the 1 key types &");
        assert_eq!(alt("ç", '9'), Some(Action::Pin9));
        // AltGr (Ctrl+Alt on Windows) still types: Turkish Q AltGr+7 is `{`.
        assert_eq!(chord_from_press("{", Physical::Digit('7'), true, true, false, false, Platform::Other), None);
        let ctrl = chord_from_press("1", Physical::Digit('1'), true, false, false, false, Platform::Other);
        assert_eq!(ctrl.and_then(|c| defaults.action_for(&c)), Some(Action::Tab1));
        // macOS: ⌘⌥ and the 1 key (⌥1 types ¡ on US, & on AZERTY).
        let mac = Shortcuts::defaults(Platform::Mac);
        for text in ["¡", "&", "1"] {
            let got = chord_from_press(text, Physical::Digit('1'), true, true, false, false, Platform::Mac);
            assert_eq!(got.and_then(|c| mac.action_for(&c)), Some(Action::Pin1), "{text}");
        }
        // ⌥ alone on macOS types a character: no shortcut.
        assert_eq!(chord_from_press("¡", Physical::Digit('1'), false, true, false, false, Platform::Mac), None);
    }
```
  `every_default_is_reachable_on_windows_and_linux`'a `Action::Pin1 => "alt+1"`, `Action::Pin2 => "alt+2"` … `Action::Pin9 => "alt+9"` (dokuz kol; `other_event` rakamı `Physical::Digit` ile, Alt basılı yollar).

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-config` ve `cargo test -j 8 -p gezik keys` → derlenmez.

- [ ] **Step 3: Implement.**
  `settings.rs`: `use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};`; `ViewDefaults`'a `/// The options that are the same in every folder (spec 7.1).` `pub options: ViewOptions` (`Default`'ta `ViewOptions::default()`). `parse_view`'da `thumbnails` bloğunun yerine ve ardına:

```rust
    if let Some(on) = view_bool(table, "thumbnails", file, warnings) {
        out.thumbnails = on;
    }
    let options = &mut out.options;
    if let Some(on) = view_bool(table, "hide-extensions", file, warnings) {
        options.hide_extensions = on;
    }
    if let Some(on) = view_bool(table, "folders-first", file, warnings) {
        options.folders_first = on;
    }
    let dates = "\"relative\", \"short\", \"iso\" or \"system\"";
    if let Some(format) = view_choice(table, "date-format", dates, DateFormat::parse, file, warnings) {
        options.date_format = format;
    }
    let sizes = "\"binary\" or \"decimal\"";
    if let Some(format) = view_choice(table, "size-format", sizes, SizeFormat::parse, file, warnings) {
        options.size_format = format;
    }
    if let Some(on) = view_bool(table, "single-click-open", file, warnings) {
        options.single_click_open = on;
    }
    if let Some(on) = view_bool(table, "show-hidden", file, warnings) {
        options.show_hidden = on;
    }
    if let Some(on) = view_bool(table, "show-system", file, warnings) {
        options.show_system = on;
    }
```

```rust
/// `[view].key` as true or false: `None` if missing; a bad value also warns.
fn view_bool(table: &toml::Table, key: &str, file: &str, warnings: &mut Vec<Warning>) -> Option<bool> {
    let value = table.get(key)?;
    let on = value.as_bool();
    if on.is_none() {
        warnings.push(Warning::new(file, format!("view.{key}: expected true or false, got {value}")));
    }
    on
}

/// One `[view]` option as the View menu (or toggle-hidden) sets it, written into settings.toml
/// with `SettingsChange::ViewOption` (spec 7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewOption {
    HideExtensions(bool),
    FoldersFirst(bool),
    DateFormat(DateFormat),
    SizeFormat(SizeFormat),
    SingleClickOpen(bool),
    ShowHidden(bool),
    ShowSystem(bool),
}

impl ViewOption {
    /// Its key in `[view]`.
    pub fn key(self) -> &'static str {
        match self {
            ViewOption::HideExtensions(_) => "hide-extensions",
            ViewOption::FoldersFirst(_) => "folders-first",
            ViewOption::DateFormat(_) => "date-format",
            ViewOption::SizeFormat(_) => "size-format",
            ViewOption::SingleClickOpen(_) => "single-click-open",
            ViewOption::ShowHidden(_) => "show-hidden",
            ViewOption::ShowSystem(_) => "show-system",
        }
    }

    pub fn apply(self, options: &mut ViewOptions) {
        match self {
            ViewOption::HideExtensions(on) => options.hide_extensions = on,
            ViewOption::FoldersFirst(on) => options.folders_first = on,
            ViewOption::DateFormat(format) => options.date_format = format,
            ViewOption::SizeFormat(format) => options.size_format = format,
            ViewOption::SingleClickOpen(on) => options.single_click_open = on,
            ViewOption::ShowHidden(on) => options.show_hidden = on,
            ViewOption::ShowSystem(on) => options.show_system = on,
        }
    }
}
```
  `settings_edit.rs`: `with_view_defaults`'ın gövdesi ortak yardımcıya taşınır:

```rust
/// Returns `text` with `[view]`'s `mode`, `sort`, `sort-dir` and `grid-size` set from
/// `view` ("Apply to all folders"); other `[view]` keys and the rest of the file stay.
pub fn with_view_defaults(text: &str, view: &gezik_core::view::ViewSettings) -> Result<String, String> {
    edit_view(
        text,
        vec![
            ("mode", view.mode.as_str().into()),
            ("sort", view.sort.key.as_str().into()),
            ("sort-dir", view.sort.dir.as_str().into()),
            ("grid-size", view.grid_size.as_str().into()),
        ],
    )
}

/// Returns `text` with `[view]`'s `option` set (the View menu, toggle-hidden; spec 7.2).
pub fn with_view_option(text: &str, option: crate::settings::ViewOption) -> Result<String, String> {
    use crate::settings::ViewOption;
    let value: toml_edit::Value = match option {
        ViewOption::HideExtensions(on)
        | ViewOption::FoldersFirst(on)
        | ViewOption::SingleClickOpen(on)
        | ViewOption::ShowHidden(on)
        | ViewOption::ShowSystem(on) => on.into(),
        ViewOption::DateFormat(format) => format.as_str().into(),
        ViewOption::SizeFormat(format) => format.as_str().into(),
    };
    edit_view(text, vec![(option.key(), value)])
}

/// Sets `entries` in `[view]` (adding the table and the missing keys), keeping each old value's
/// decor so inline comments (`# list | grid`) survive.
fn edit_view(text: &str, entries: Vec<(&str, toml_edit::Value)>) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if doc.get("view").is_none() {
        doc["view"] = toml_edit::table();
    }
    let Some(table) = doc["view"].as_table_like_mut() else { return Err("view must be a table".to_owned()) };
    for (key, value) in entries {
        if let Some(old) = table.get_mut(key).and_then(|item| item.as_value_mut()) {
            let decor = old.decor().clone();
            *old = value;
            *old.decor_mut() = decor;
        } else {
            table.insert(key, toml_edit::Item::Value(value));
        }
    }
    Ok(doc.to_string())
}
```
  `settings_writer.rs`: `/// One `[view]` option (the View menu, toggle-hidden).` `ViewOption(ViewOption)`; `apply`'da `SettingsChange::ViewOption(option) => with_view_option(text, *option)`; modül belgesine "the view options".
  `shortcuts.rs`: `SaveTabSet`'ten sonra `/// Goes to the first pinned folder the sidebar shows (7b); `Pin2` … `Pin9` to the next ones.` `Pin1`, ardından `Pin2`…`Pin9`; `ALL: [Action; 59]` sonuna aynı sırayla; adlar `pin-1`…`pin-9`;

```rust
    /// The pinned folder a `pin-N` action goes to (1-based): `Pin1` → 1 … `Pin9` → 9.
    pub fn pin_number(self) -> Option<usize> {
        Some(match self {
            Action::Pin1 => 1,
            Action::Pin2 => 2,
            Action::Pin3 => 3,
            Action::Pin4 => 4,
            Action::Pin5 => 5,
            Action::Pin6 => 6,
            Action::Pin7 => 7,
            Action::Pin8 => 8,
            Action::Pin9 => 9,
            _ => return None,
        })
    }

    /// `pin-number` (1-9).
    pub fn pin(number: usize) -> Option<Action> {
        Action::ALL.into_iter().find(|action| action.pin_number() == Some(number))
    }
```
  varsayılanlar (`SaveTabSet`'ten sonra):

```rust
            // Alt+digit, not Ctrl+Alt+digit: Windows takes the left Ctrl+Alt for AltGr, which
            // types with the digit keys on many layouts (Turkish Q: > £ # $ ½ { [ ]; spec 10.3).
            (Action::Pin1, Platform::Mac) => &["mod+alt+1"],
            (Action::Pin1, Platform::Other) => &["alt+1"],
            (Action::Pin2, Platform::Mac) => &["mod+alt+2"],
            (Action::Pin2, Platform::Other) => &["alt+2"],
            (Action::Pin3, Platform::Mac) => &["mod+alt+3"],
            (Action::Pin3, Platform::Other) => &["alt+3"],
            (Action::Pin4, Platform::Mac) => &["mod+alt+4"],
            (Action::Pin4, Platform::Other) => &["alt+4"],
            (Action::Pin5, Platform::Mac) => &["mod+alt+5"],
            (Action::Pin5, Platform::Other) => &["alt+5"],
            (Action::Pin6, Platform::Mac) => &["mod+alt+6"],
            (Action::Pin6, Platform::Other) => &["alt+6"],
            (Action::Pin7, Platform::Mac) => &["mod+alt+7"],
            (Action::Pin7, Platform::Other) => &["alt+7"],
            (Action::Pin8, Platform::Mac) => &["mod+alt+8"],
            (Action::Pin8, Platform::Other) => &["alt+8"],
            (Action::Pin9, Platform::Mac) => &["mod+alt+9"],
            (Action::Pin9, Platform::Other) => &["alt+9"],
```
  Şablon `[view]` (bugünkü altı satırdan sonra):

```toml
# The same in every folder; the View menu changes these here too.
hide-extensions = false   # show names without their extension
folders-first = true      # folders before files in every sort
date-format = "system"    # relative | short | iso | system
size-format = "binary"    # binary (1024, KB) | decimal (1000, kB)
single-click-open = false # one click opens; Ctrl (Cmd) and Shift clicks still select
# show-hidden = true      # dot names and Windows' hidden items; default true (macOS: false)
show-system = false       # Windows only: protected system items (desktop.ini, $RECYCLE.BIN)
```
  `[shortcuts]` örneklerinin sonuna (`save-tab-set` satırından sonra, boş satırsız):

```toml
# pin-1 = "alt+1"          # the first pinned folder; pin-2 … pin-9 likewise; macOS: "mod+alt+1"
```
  `keys.rs` (gezik): `chord_from_press`'in rakam kolu `Physical::Digit(d) if digit_chord(ctrl, alt, cmd, platform) && typed_char(text).is_some_and(|c| !c.is_ascii_alphabetic()) =>` olur ve:

```rust
/// Whether a digit key with these modifiers is that digit whatever it types: Ctrl (⌘) without
/// Alt (AltGr is Ctrl+Alt on Windows, and types); Alt alone (Shift may be held) on Windows and
/// Linux; ⌘⌥ on macOS (the pinned folders, spec 10.3).
fn digit_chord(ctrl: bool, alt: bool, cmd: bool, platform: Platform) -> bool {
    ((ctrl || cmd) && !alt)
        || match platform {
            Platform::Other => alt && !ctrl && !cmd,
            Platform::Mac => cmd && alt,
        }
}
```
  `chord_from_press`'in belgesi: "… Ctrl (⌘) on a digit key is that digit, with or without Shift, whatever the key types (…); so is Alt alone off macOS and ⌘⌥ on macOS (`digit_chord`). Not Ctrl+Alt: AltGr (Ctrl+Alt on Windows) types characters. …"
  Derleme kolları: `actions.rs`'te "Not theirs" listesine `| Action::Pin1 | Action::Pin2 | Action::Pin3 | Action::Pin4 | Action::Pin5 | Action::Pin6 | Action::Pin7 | Action::Pin8 | Action::Pin9` (Task 9 doldurur); `main.rs` `handle_key`'in 6a/7a koluna (`| Action::SaveTabSet`'in yanına) aynı dokuzu.

- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS (`defaults_cover_every_action` yeni varsayılanların hiçbir eskisiyle çakışmadığını, `view_defaults_keep_inline_comments_of_the_template` şablonun yorumlarının kaldığını doğrular); clippy, fmt, çapraz denetimler.
- [ ] **Step 5: Commit** "Read the view options of [view], write one at a time, and name pin-1 to pin-9 on Alt and Cmd+Option digits".

---

### Task 5: `gezik-platform` — tarih biçimleri (`format_date`)

**Files:**
- Modify: `crates/gezik-platform/src/datetime.rs` (`format_date`, `short_date`, `iso_text`, Windows'ta tarih/saat ayrımı), `crates/gezik-platform/src/lib.rs` (`pub use datetime::{format_date, format_datetime, local_date_parts};`)

**Interfaces:**
- Consumes: Task 2 (`DateFormat`, `relative_date`).
- Produces:

```rust
pub fn format_date(time: SystemTime, format: DateFormat, now: SystemTime) -> String;
```

- [ ] **Step 1: Write the failing test** (`datetime.rs` `parts_tests` modülüne)

```rust
    #[test]
    fn every_date_format_has_its_shape() {
        use gezik_core::view::DateFormat;
        use std::time::Duration;
        let now = SystemTime::now();
        let p = local_date_parts(now).unwrap();
        let iso = format_date(now, DateFormat::Iso, now);
        assert_eq!(iso, format!("{:04}-{:02}-{:02} {:02}:{:02}", p.year, p.month, p.day, p.hour, p.minute));
        let short = format_date(now, DateFormat::Short, now);
        #[cfg(not(windows))]
        assert_eq!(short, iso[..10]);
        #[cfg(windows)]
        assert!(!short.is_empty() && !short.contains(':'), "the system's date, no time: {short}");
        let old = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(format_date(old, DateFormat::System, now), format_datetime(old));
        assert_eq!(format_date(old, DateFormat::Relative, now), format_datetime(old), "long ago: the system's");
        assert_eq!(format_date(now - Duration::from_secs(300), DateFormat::Relative, now), "5 min ago");
        assert_eq!(format_date(now, DateFormat::Relative, now), "1 min ago");
        let later = now + Duration::from_secs(7200);
        assert_eq!(format_date(later, DateFormat::Relative, now), format_datetime(later), "the future: the system's");
        let earlier = now - Duration::from_secs(7200);
        let e = local_date_parts(earlier).unwrap();
        let day = if (e.year, e.month, e.day) == (p.year, p.month, p.day) { "Today" } else { "Yesterday" };
        assert_eq!(
            format_date(earlier, DateFormat::Relative, now),
            format!("{day} {:02}:{:02}", e.hour, e.minute)
        );
    }
```

- [ ] **Step 2: Run to see it fail.** `cargo test -j 8 -p gezik-platform datetime` → derlenmez.

- [ ] **Step 3: Implement.** Modül belgesi: "Dates in the file list: the system's short date and time on Windows, else `YYYY-MM-DD HH:MM`, in local time; and the other formats of `[view] date-format` (spec 7.1)." Ekler:

```rust
use gezik_core::view::{DateFormat, relative_date};

/// `time` as `format` writes it; `now` is the time now (for `Relative`).
pub fn format_date(time: SystemTime, format: DateFormat, now: SystemTime) -> String {
    match format {
        DateFormat::System => format_datetime(time),
        DateFormat::Iso => local_date_parts(time).map(|p| iso_text(&p, true)).unwrap_or_default(),
        DateFormat::Short => short_date(time),
        DateFormat::Relative => {
            let age = match now.duration_since(time) {
                Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
                Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
            };
            local_date_parts(now)
                .zip(local_date_parts(time))
                .and_then(|(now, then)| relative_date(&now, &then, age))
                .unwrap_or_else(|| format_datetime(time))
        }
    }
}

/// `YYYY-MM-DD`, and ` HH:MM` with `clock`.
fn iso_text(p: &DateParts, clock: bool) -> String {
    let date = format!("{:04}-{:02}-{:02}", p.year, p.month, p.day);
    if clock { format!("{date} {:02}:{:02}", p.hour, p.minute) } else { date }
}

/// The date only: the system's short date on Windows, `YYYY-MM-DD` elsewhere.
fn short_date(time: SystemTime) -> String {
    #[cfg(windows)]
    if let Some(text) = win::format_date(time) {
        return text;
    }
    local_date_parts(time).map(|p| iso_text(&p, false)).unwrap_or_default()
}
```
  `win` modülünde `format` ikiye bölünür (davranış aynı):

```rust
    /// `time` as a local `SYSTEMTIME`.
    fn local(time: SystemTime) -> Option<SYSTEMTIME> {
        let ticks = time.duration_since(UNIX_EPOCH).ok()?.as_nanos() / 100 + UNIX_EPOCH_TICKS;
        let filetime = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        // SAFETY: pointers to live locals of the right types.
        unsafe {
            FileTimeToSystemTime(&filetime, &mut utc).ok()?;
            SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
        }
        Some(local)
    }

    /// The system's short date of `local`.
    fn date_text(local: &SYSTEMTIME) -> Option<String> {
        let mut date = [0u16; 80];
        // SAFETY: the buffer's length is passed with it; `local` is a live SYSTEMTIME.
        let n = unsafe {
            GetDateFormatEx(PCWSTR::null(), DATE_SHORTDATE, Some(local as *const SYSTEMTIME), PCWSTR::null(), Some(&mut date), PCWSTR::null())
        };
        (n > 1).then(|| String::from_utf16_lossy(&date[..n as usize - 1]))
    }

    /// The system's time of `local`, without seconds.
    fn clock_text(local: &SYSTEMTIME) -> Option<String> {
        let mut clock = [0u16; 80];
        // SAFETY: as in `date_text`.
        let m = unsafe {
            GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(local as *const SYSTEMTIME), PCWSTR::null(), Some(&mut clock))
        };
        (m > 1).then(|| String::from_utf16_lossy(&clock[..m as usize - 1]))
    }

    pub fn format(time: SystemTime) -> Option<String> {
        let local = local(time)?;
        Some(format!("{} {}", date_text(&local)?, clock_text(&local)?))
    }

    /// The system's short date of `time`, without the time.
    pub fn format_date(time: SystemTime) -> Option<String> {
        date_text(&local(time)?)
    }
```
  (`parts` de `local`'i kullanır.) `lib.rs`: `pub use datetime::{format_date, format_datetime, local_date_parts};`.

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-platform` → PASS; çapraz denetimler (Linux denetimi `win` dışını derler).
- [ ] **Step 5: Commit** "Write dates as relative, short, ISO or the system's".

---

### Task 6: Arayüz — görünüm seçeneklerinin uygulanması (`view_options.rs`, liste, tek tıkla açma, gizli/sistem)

**Files:**
- Create: `crates/gezik/src/view_options.rs`
- Modify: `crates/gezik/src/view/mod.rs`, `crates/gezik/src/view/model.rs`, `crates/gezik/src/view/listing.rs`, `crates/gezik/src/drag.rs` (tek tık), `crates/gezik/src/main.rs` (`mod view_options;`, kurulum, `apply_config`, `open_entry`, `on_open_row`, `ToggleHidden` kolu), `crates/gezik/src/actions.rs` (`ToggleHidden`), `crates/gezik/src/menu_bar.rs` (`ToggleHidden` yedeği kalkar), `crates/gezik/src/preview.rs`, `crates/gezik/src/operations.rs`, `crates/gezik/src/conflicts.rs`

**Interfaces:**
- Consumes: Task 1 (`Entry::is_shown`, `shown_name`, `sort_entries(.., folders_first, ..)`), Task 2 (`ViewOptions`, `format_size_in`, `format_rate(.., SizeFormat)`), Task 4 (`ViewOption`, `SettingsChange::ViewOption`, `ViewDefaults.options`), Task 5 (`format_date`).
- Produces:

```rust
// view_options.rs
pub fn install(window: &AppWindow, store: Option<ConfigStore>);
pub fn current() -> ViewOptions;
pub fn set_from_file(options: ViewOptions);
pub fn change(option: ViewOption);
pub fn toggle_hidden();
pub fn size_text(bytes: u64) -> String;
pub fn date_text(time: SystemTime) -> String;
// view/mod.rs
impl View { pub fn set_options(&self, options: ViewOptions) -> bool;   // true: read the folder again
            pub fn take_plain_press(&self, index: usize) -> bool; }
pub fn visible_lines(scroll: f32, height: f32, line_height: f32) -> std::ops::Range<usize>;
// view/listing.rs
impl Listing { pub fn without_hidden(self, show_hidden: bool, show_system: bool) -> Listing; }
// view/model.rs: ViewData { …, pub options: ViewOptions }
// main.rs
fn open_entry(nav: &navigation::Navigator, view: &view::View, index: usize);
```

- [ ] **Step 1: Write the failing tests**

`view_options.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::view::{DateFormat, SizeFormat};

    #[test]
    fn a_reload_while_writes_are_on_their_way_is_ignored() {
        let mut state = OptionsState::default();
        let first = state.change(ViewOption::HideExtensions(true));
        let second = state.change(ViewOption::DateFormat(DateFormat::Iso));
        assert!(second.hide_extensions && second.date_format == DateFormat::Iso);
        // The reload after the first write: the file has only that one yet.
        assert_eq!(state.read(first), None, "what is shown stays");
        assert_eq!(state.written(true, first), None);
        assert_eq!(state.written(true, second), None, "the last one: the file is what is shown");
        assert_eq!(state.shown, second);
        assert_eq!(state.read(second), None, "its own reload changes nothing");
        let by_hand = ViewOptions { size_format: SizeFormat::Decimal, ..second };
        assert_eq!(state.read(by_hand), Some(by_hand), "a hand edit shows at once");
    }

    #[test]
    fn a_failed_write_goes_back_to_the_file() {
        let mut state = OptionsState::default();
        let before = state.shown;
        let changed = state.change(ViewOption::ShowHidden(!before.show_hidden));
        assert_ne!(changed, before);
        assert_eq!(state.written(false, changed), Some(before), "back to what the file has");
        assert_eq!(state.shown, before);
    }
}
```

`view/listing.rs`:

```rust
    #[test]
    fn hidden_and_protected_items_can_be_left_out() {
        let listing = || {
            let Listing::Files(dir, entries) = files("/x", &[".DS_Store", ".git/", "a.txt", "b/", "desktop.ini", "notes.txt"])
            else {
                unreachable!()
            };
            let mut entries = Rc::unwrap_or_clone(entries);
            entries[4].flags = Entry::HIDDEN | Entry::SYSTEM;
            entries[5].flags = Entry::HIDDEN;
            Listing::Files(dir, Rc::new(entries))
        };
        let names = |l: Listing| (0..l.len()).filter_map(|i| l.name_at(i).map(str::to_owned)).collect::<Vec<_>>();
        assert_eq!(names(listing().without_hidden(false, false)), ["a.txt", "b/"]);
        assert_eq!(names(listing().without_hidden(true, false)), [".DS_Store", ".git/", "a.txt", "b/", "notes.txt"]);
        assert_eq!(names(listing().without_hidden(false, true)), ["a.txt", "b/", "desktop.ini"]);
        assert_eq!(names(listing().without_hidden(true, true)).len(), 6);
    }
```
  (`dotfiles_can_be_left_out` silinir: yeni test onu kapsar.)

`view/model.rs`:

```rust
    #[test]
    fn rows_follow_the_view_options() {
        let mut data = ViewData {
            listing: super::super::listing::files("/x", &["sub.d/", "a.txt", ".gitignore", "a.tar.gz"]),
            media: Media::idle(),
            ..Default::default()
        };
        data.options.hide_extensions = true;
        let names: Vec<String> = (0..4).map(|i| file_row(&data, i).name.to_string()).collect();
        assert_eq!(names, ["sub.d/", "a", ".gitignore", "a.tar"], "folders and dot names keep theirs");
        data.options.hide_extensions = false;
        assert_eq!(file_row(&data, 1).name.as_str(), "a.txt");
        assert_eq!(file_row(&data, 1).size.as_str(), "10 B");
    }
```

`view/mod.rs` testleri:

```rust
    #[test]
    fn the_lines_on_screen_are_redrawn() {
        assert_eq!(visible_lines(0.0, 260.0, 26.0), 0..10, "ten whole lines");
        assert_eq!(visible_lines(-260.0, 260.0, 26.0), 10..20);
        assert_eq!(visible_lines(0.0, 270.0, 26.0), 0..11, "and a part of the eleventh");
        assert_eq!(visible_lines(-13.0, 26.0, 26.0), 0..2, "half of two lines");
        assert_eq!(visible_lines(0.0, 100.0, 0.0), 0..0, "no line height: none");
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik view_options listing model the_lines_on_screen` → derlenmez.

- [ ] **Step 3: Implement.**
  `view_options.rs`:

```rust
//! The view options of `[view]` that are the same in every folder (spec 7). settings.toml has
//! them; the View menu and toggle-hidden change them there through the one writer, showing the
//! change at once (as the sidebar does with its pins), and every part of the window reads them
//! here.

use std::cell::RefCell;
use std::time::SystemTime;

use gezik_config::Warning;
use gezik_config::settings::ViewOption;
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::view::ViewOptions;
use slint::ComponentHandle;

use crate::AppWindow;

/// What is shown, what settings.toml has, and how many writes are on their way.
#[derive(Debug, Default)]
struct OptionsState {
    shown: ViewOptions,
    in_file: ViewOptions,
    on_their_way: usize,
}

impl OptionsState {
    /// settings.toml was read: the options to show now, if they changed. While writes are on
    /// their way the file is behind them, and what is shown stays.
    fn read(&mut self, options: ViewOptions) -> Option<ViewOptions> {
        self.in_file = options;
        if self.on_their_way > 0 || self.shown == options {
            return None;
        }
        self.shown = options;
        Some(options)
    }

    /// `option` changed from a menu: shown at once, its write on its way.
    fn change(&mut self, option: ViewOption) -> ViewOptions {
        option.apply(&mut self.shown);
        self.on_their_way += 1;
        self.shown
    }

    /// A write ended (`written`: the options it wrote). After the last one, what the file has
    /// is shown: the same after a success, the values from before after a failure.
    fn written(&mut self, ok: bool, written: ViewOptions) -> Option<ViewOptions> {
        self.on_their_way = self.on_their_way.saturating_sub(1);
        if ok {
            self.in_file = written;
        }
        if self.on_their_way > 0 || self.shown == self.in_file {
            return None;
        }
        self.shown = self.in_file;
        Some(self.shown)
    }
}

thread_local! {
    static STATE: RefCell<OptionsState> = RefCell::new(OptionsState::default());
    /// The window (for the macOS menu bar's marks) and where settings.toml is.
    static CONTEXT: RefCell<Option<(slint::Weak<AppWindow>, Option<ConfigStore>)>> = const { RefCell::new(None) };
}

/// Once, at start, before the settings are first applied.
pub fn install(window: &AppWindow, store: Option<ConfigStore>) {
    CONTEXT.with(|c| *c.borrow_mut() = Some((window.as_weak(), store)));
}

/// The options in effect.
pub fn current() -> ViewOptions {
    STATE.with(|s| s.borrow().shown)
}

/// settings.toml was read (every resolve).
pub fn set_from_file(options: ViewOptions) {
    let shown = STATE.with(|s| s.borrow_mut().read(options));
    if let Some(options) = shown {
        show(options);
    }
}

/// The View menu (or toggle-hidden) changes `option`: shown at once and written into
/// settings.toml; a failure is said in the status bar and the file's values come back.
pub fn change(option: ViewOption) {
    let store = CONTEXT.with(|c| c.borrow().as_ref().and_then(|(_, store)| store.clone()));
    let Some(store) = store else {
        // No config folder: the change lives in memory.
        let shown = STATE.with(|s| {
            let mut s = s.borrow_mut();
            option.apply(&mut s.shown);
            s.in_file = s.shown;
            s.shown
        });
        return show(shown);
    };
    let shown = STATE.with(|s| s.borrow_mut().change(option));
    show(shown);
    store.write_settings(SettingsChange::ViewOption(option), move |result| {
        let _ = slint::invoke_from_event_loop(move || written(shown, result));
    });
}

fn written(sent: ViewOptions, result: Result<(), Warning>) {
    if let Err(warning) = &result {
        crate::view::with_current(|view| view.note(warning.to_string()));
    }
    let back = STATE.with(|s| s.borrow_mut().written(result.is_ok(), sent));
    if let Some(options) = back {
        show(options);
    }
}

/// `toggle-hidden` (Ctrl+H, ⌘⇧.): writes `show-hidden` (spec 7.2).
pub fn toggle_hidden() {
    change(ViewOption::ShowHidden(!current().show_hidden));
}

/// Applies `options` to the list; the folder is read again when hidden or system items
/// come or go.
fn show(options: ViewOptions) {
    let mut reload = false;
    crate::view::with_current(|view| reload = view.set_options(options));
    if reload {
        crate::navigation::with_current(|nav| nav.reload());
    }
}

/// A size as `[view] size-format` writes it (the list, the status bar, the preview, the
/// operations panel, the conflict list).
pub fn size_text(bytes: u64) -> String {
    gezik_core::format_size_in(bytes, current().size_format)
}

/// A time as `[view] date-format` writes it.
pub fn date_text(time: SystemTime) -> String {
    gezik_platform::format_date(time, current().date_format, SystemTime::now())
}
```
  (`slint::ComponentHandle` `as_weak` için.)
  `view/listing.rs`: `without_dotfiles`'ın yerine:

```rust
    /// Without what `[view]` hides: dot names and hidden items unless `show_hidden`, protected
    /// system items unless `show_system` (`Entry::is_shown`). Shares the entries when nothing
    /// is left out.
    pub fn without_hidden(self, show_hidden: bool, show_system: bool) -> Listing {
        match self {
            Listing::Files(dir, entries) if entries.iter().any(|e| !e.is_shown(show_hidden, show_system)) => {
                let kept = entries.iter().filter(|e| e.is_shown(show_hidden, show_system)).cloned().collect();
                Listing::Files(dir, Rc::new(kept))
            }
            other => other,
        }
    }
```
  `view/model.rs`: `ViewData`'ya `/// `[view]`'s options in effect (view_options.rs).` `pub options: gezik_core::view::ViewOptions`; `file_row`:

```rust
    let options = data.options;
    let now = std::time::SystemTime::now();
    let date = |time: Option<std::time::SystemTime>| {
        time.map(|time| gezik_platform::format_date(time, options.date_format, now)).unwrap_or_default()
    };
    let name = listing.name_at(i).unwrap_or_default();
    let hide = options.hide_extensions && matches!(listing, Listing::Files(..));
    FileRow {
        name: gezik_core::shown_name(name, is_dir, hide).into(),
        // …
        size: match listing {
            Listing::Files(..) if !is_dir => format_size_in(listing.file_size(i), options.size_format).into(),
            _ => "".into(),
        },
        // `cut` looks up the real name, as before.
```
  (`use gezik_core::format_size_in;`; `cut` satırı `name`'i değil `listing.name_at(i)`'yi kullanmaya devam eder.)
  `view/mod.rs`:
  - `Inner`: `show_hidden: Cell<bool>` yerine `/// `[view]`'s options in effect (view_options.rs).` `options: Cell<ViewOptions>`; ekler `/// Redraws the dates on screen every minute while they are relative.` `minute: slint::Timer`, `/// The entry the last left press without Ctrl (⌘) or Shift was on (single-click-open).` `plain_press: Cell<Option<usize>>`. `new`'de `options: Cell::new(ViewOptions::default())`, `minute: slint::Timer::default()`, `plain_press: Cell::new(None)`.
  - `show`'da `let options = self.0.options.get(); let listing = listing.without_hidden(options.show_hidden, options.show_system);`.
  - `sorted`: `Listing::Files(dir, entries) if !(by_name && self.sort() == SortSpec::default() && self.0.options.get().folders_first) =>`.
  - `sort_now`: `sort_entries(&mut entries, spec, self.0.options.get().folders_first, |e| self.type_name_of(e))`.
  - `toggle_hidden` silinir; `shows_hidden` `self.0.options.get().show_hidden` döner (path_box kullanır).
  - `press`'in başında `self.0.plain_press.set((!ctrl && !shift).then_some(index));` ve:

```rust
    /// Whether the last left press, plain (no Ctrl, ⌘ or Shift), was on entry `index`: its
    /// release opens it with single-click-open. Forgotten once asked.
    pub fn take_plain_press(&self, index: usize) -> bool {
        self.0.plain_press.take() == Some(index)
    }

    /// `[view]`'s options (view_options.rs). Returns whether the folder must be read again
    /// (hidden or system items come or go); the rest applies here at once.
    pub fn set_options(&self, options: ViewOptions) -> bool {
        let old = self.0.options.replace(options);
        if old == options {
            return false;
        }
        self.0.data.borrow_mut().options = options;
        self.follow_relative_dates(options.date_format == DateFormat::Relative);
        if old.folders_first != options.folders_first {
            self.resort(false);
        } else if (old.hide_extensions, old.date_format, old.size_format)
            != (options.hide_extensions, options.date_format, options.size_format)
        {
            let scroll = self.0.window.upgrade().map_or(0.0, |w| w.get_list_scroll());
            self.0.model.notify.reset();
            self.keep_scroll_after_reset(scroll);
        }
        self.update_status();
        self.notify_listeners();
        (old.show_hidden, old.show_system) != (options.show_hidden, options.show_system)
    }

    /// With relative dates the lines on screen are drawn again every minute ("5 min ago").
    fn follow_relative_dates(&self, on: bool) {
        if !on {
            return self.0.minute.stop();
        }
        if self.0.minute.running() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        self.0.minute.start(slint::TimerMode::Repeated, Duration::from_secs(60), move || {
            if let Some(inner) = weak.upgrade() {
                View(inner).redraw_visible();
            }
        });
    }

    /// Draws again the lines on screen only (a minute passed).
    fn redraw_visible(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let line_height = match self.geometry() {
            Geometry::List { row_height } => row_height,
            Geometry::Grid { cell_height, .. } => cell_height,
        };
        let lines = visible_lines(window.get_list_scroll(), window.get_drop_geometry().list_height, line_height);
        let per_row = self.0.model.per_row();
        let len = self.0.data.borrow().listing.len();
        let entries = (lines.start * per_row).min(len)..(lines.end * per_row).min(len);
        self.0.model.entries_changed(std::slice::from_ref(&entries));
    }
```
  ve modül düzeyinde (`status_text`'in yanında):

```rust
/// The lines on screen: from the one the list is scrolled to (`scroll` is zero or negative) to
/// the last one any part of which shows.
pub fn visible_lines(scroll: f32, height: f32, line_height: f32) -> std::ops::Range<usize> {
    if line_height <= 0.0 {
        return 0..0;
    }
    let first = (-scroll / line_height).floor().max(0.0) as usize;
    let last = ((-scroll + height) / line_height).ceil().max(0.0) as usize;
    first..last.max(first)
}
```
  `use gezik_core::view::{DateFormat, ViewOptions};` eklenir. `status_text`'in boyutu `crate::view_options::size_text(size)` (`format_size` importu kalkar).
  `drag.rs`, `up`'ın `Phase::Armed` kolu:

```rust
            Phase::Armed { index, right: pressed_right, .. } => {
                if !pressed_right && !right {
                    self.0.view.release(index, false);
                    // Single-click-open: a plain click (no Ctrl, ⌘ or Shift) opens (spec 7.1);
                    // after the release is fully handled.
                    if crate::view_options::current().single_click_open && self.0.view.take_plain_press(index) {
                        let (nav, view) = (self.0.nav.clone(), self.0.view.clone());
                        Timer::single_shot(Duration::ZERO, move || crate::open_entry(&nav, &view, index));
                    }
                }
                false
            }
```
  `main.rs`: `mod view_options;`; `let config = ConfigStore::system();`'den hemen sonra `view_options::install(&window, config.clone());`; `apply_config`'te `view::with_current(...)`'ın ardına `view_options::set_from_file(loaded.settings.view.options);`; `view.set_defaults(initial_settings.view);`'dan sonra `view.set_options(view_options::current());`. Çift tık:

```rust
/// Opens entry `index` of the list (a double-click, or a click with single-click-open): an
/// archive is extracted next to itself if `[archives] double-click` says so.
fn open_entry(nav: &navigation::Navigator, view: &view::View, index: usize) {
    let archive = view.entry_path(index).filter(|(path, is_dir)| {
        !is_dir
            && path.file_name().is_some_and(|n| gezik_core::batch::archive::looks_like_archive(&n.to_string_lossy()))
    });
    match archive {
        Some((path, _)) if archives::extracts_on_double_click() => {
            archives::with_current(|archives| archives.extract_here(vec![path]));
        }
        _ => nav.open_row(i32::try_from(index).unwrap_or(-1)),
    }
}
```

```rust
    // Double-click; with single-click-open the click already opened it.
    window.on_open_row({
        let (nav, view) = (nav.clone(), view.clone());
        move |i| {
            if let Ok(index) = usize::try_from(i)
                && !view_options::current().single_click_open
            {
                open_entry(&nav, &view, index);
            }
        }
    });
```
  (`let archives = archives::Archives::new(…)` artık yalnız yaşaması gereken değer: `let _archives = …`.) `handle_key`'te `Action::ToggleHidden => { view.toggle_hidden(); nav.reload(); }` kolu silinir, `ToggleHidden` 6a/7a koluna eklenir; `actions.rs`'te `Action::ToggleHidden => crate::view_options::toggle_hidden(),` ("Not theirs"'tan çıkar); `menu_bar.rs`'in yedek kolundaki `Action::ToggleHidden => { … }` silinir (artık `actions::run` çalıştırır).
  Boyut ve tarihler: `preview.rs` `describe`'da `format_size(…)` → `crate::view_options::size_text(…)`, `format_datetime(*time)` → `crate::view_options::date_text(*time)` (importlar düzelir); `operations.rs` `format_size(progress.bytes_total)` → `crate::view_options::size_text(progress.bytes_total)`, `format_rate(speed, …)` → `format_rate(speed, crate::view_options::current().size_format)`; `conflicts.rs` `date`/`size` kapanışları `crate::view_options::date_text` ve `crate::view_options::size_text`.

- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler. Elle (Windows, `GEZIK_CONFIG_DIR` geçici, `cargo run -j 8 -p gezik`): `settings.toml`'a `[view]` `hide-extensions = true` yazıp kaydet → adlar uzantısız, F2 tam adı gösterir; `date-format = "relative"` → az önce değişen dosya "N min ago"; `size-format = "decimal"` → `kB`; `folders-first = false` → klasörler karışık; `single-click-open = true` → tek tık açar, Ctrl+tık seçer; Ctrl+H → `settings.toml`'da `show-hidden = false` ve noktalı/gizli öğeler gider; `C:\`'de `$RECYCLE.BIN` ve `System Volume Information` görünmez, `show-system = true` → görünür.
- [ ] **Step 5: Commit** "Apply the view options: hidden and system items, extensions, folders first, date and size formats, single-click open".

---

### Task 7: Menüler — View menüsünde seçenekler ve iki alt menü, macOS View menüsü

**Files:**
- Modify: `crates/gezik/src/context_menu.rs` (kimlikler, `view_items`, `format_subs`, `view_option_for`, `from_submenu`, `view_menu`, `run`), `crates/gezik/src/view_options.rs` (`sync_window`), `crates/gezik/src/menu_bar.rs` (`view-option:`), `crates/gezik/ui/app.slint` (macOS View menüsü, işaret özellikleri)

**Interfaces:**
- Consumes: Task 4 (`ViewOption`), Task 6 (`view_options::{current, change}`).
- Produces:

```rust
// context_menu.rs
pub const HIDE_EXTENSIONS: u32 = 1300;  pub const FOLDERS_FIRST: u32 = 1301;
pub const SINGLE_CLICK_OPEN: u32 = 1302;  pub const SHOW_HIDDEN: u32 = 1303;  pub const SHOW_SYSTEM: u32 = 1304;
pub const DATE_FORMAT_FIRST: u32 = 1310;   // 1310-1313, DateFormat::ALL order
pub const SIZE_FORMAT_FIRST: u32 = 1320;   // 1320-1321, SizeFormat::ALL order
pub fn view_items(view: ViewSettings, preview_open: bool, options: ViewOptions, windows: bool) -> Vec<(u32, String)>;
pub fn format_subs(options: ViewOptions, at: usize) -> Vec<Submenu>;
pub fn view_option_for(id: u32, options: ViewOptions) -> Option<ViewOption>;
// view_options.rs
pub fn sync_window(options: ViewOptions);
```
```slint
// app.slint (macOS marks)
in-out property <bool> view-hide-extensions; view-folders-first; view-single-click; view-show-hidden;
in-out property <bool> view-date-relative; view-date-short; view-date-iso; view-date-system;
in-out property <bool> view-size-binary; view-size-decimal;
```

- [ ] **Step 1: Write the failing tests** (`context_menu.rs`)

```rust
    #[test]
    fn view_menu_lists_the_options_and_their_marks() {
        use gezik_core::view::{DateFormat, ViewOptions};
        let options = ViewOptions { hide_extensions: true, ..ViewOptions::default() };
        let items = view_items(ViewSettings::default(), false, options, true);
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            &ids[10..],
            [HIDE_EXTENSIONS, FOLDERS_FIRST, SINGLE_CLICK_OPEN, SHOW_HIDDEN, SHOW_SYSTEM, APPLY_TO_ALL, RESET_FOLDER]
        );
        assert!(items[10].1.starts_with("• ") && items[11].1.starts_with("• "), "extensions hidden, folders first");
        assert!(!items[12].1.starts_with("• "));
        assert_eq!(items[13].1.starts_with("• "), options.show_hidden);
        let elsewhere: Vec<u32> =
            view_items(ViewSettings::default(), false, options, false).iter().map(|(id, _)| *id).collect();
        assert!(!elsewhere.contains(&SHOW_SYSTEM), "Show system items: Windows only");
        let subs = format_subs(ViewOptions { date_format: DateFormat::Iso, ..options }, 15);
        let places: Vec<(&str, usize)> = subs.iter().map(|s| (s.title.as_str(), s.at)).collect();
        assert_eq!(places, [("Date format", 15), ("Size format", 15)]);
        let dates: Vec<(u32, &str)> = subs[0].items.iter().map(|(id, t, _)| (*id, t.as_str())).collect();
        assert_eq!(
            dates,
            [
                (DATE_FORMAT_FIRST, "    Relative"),
                (DATE_FORMAT_FIRST + 1, "    Short"),
                (DATE_FORMAT_FIRST + 2, "• ISO"),
                (DATE_FORMAT_FIRST + 3, "    System")
            ]
        );
        assert_eq!(subs[1].items.len(), 2);
    }

    #[test]
    fn view_menu_ids_say_which_option_changes() {
        use gezik_config::settings::ViewOption;
        use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
        let options = ViewOptions::default();
        assert_eq!(view_option_for(HIDE_EXTENSIONS, options), Some(ViewOption::HideExtensions(true)));
        assert_eq!(view_option_for(FOLDERS_FIRST, options), Some(ViewOption::FoldersFirst(false)));
        assert_eq!(view_option_for(SINGLE_CLICK_OPEN, options), Some(ViewOption::SingleClickOpen(true)));
        assert_eq!(view_option_for(SHOW_HIDDEN, options), Some(ViewOption::ShowHidden(!options.show_hidden)));
        assert_eq!(view_option_for(SHOW_SYSTEM, options), Some(ViewOption::ShowSystem(true)));
        assert_eq!(view_option_for(DATE_FORMAT_FIRST, options), Some(ViewOption::DateFormat(DateFormat::Relative)));
        assert_eq!(view_option_for(SIZE_FORMAT_FIRST + 1, options), Some(ViewOption::SizeFormat(SizeFormat::Decimal)));
        assert_eq!(view_option_for(SIZE_FORMAT_FIRST + 2, options), None);
        assert_eq!(view_option_for(VIEW_LIST, options), None);
        assert!(from_submenu(DATE_FORMAT_FIRST + 3) && from_submenu(SIZE_FORMAT_FIRST) && !from_submenu(HIDE_EXTENSIONS));
    }
```
  `view_menu_marks_the_current_choices`: çağrılar `view_items(…, ViewOptions::default(), false)`; kimlik listesi `PREVIEW_PANE`'den sonra `HIDE_EXTENSIONS, FOLDERS_FIRST, SINGLE_CLICK_OPEN, SHOW_HIDDEN` alır; ızgaranın işaretlileri `let mut expected = vec!["Grid", "Large icons", "Sort by size", "Descending", "Folders first"]; if !cfg!(target_os = "macos") { expected.push("Show hidden items"); }`. `conversion_ids_meet_no_others`'ın `singles`'ına `HIDE_EXTENSIONS, FOLDERS_FIRST, SINGLE_CLICK_OPEN, SHOW_HIDDEN, SHOW_SYSTEM`, aralıklarına `DATE_FORMAT_FIRST..DATE_FORMAT_FIRST + 4`, `SIZE_FORMAT_FIRST..SIZE_FORMAT_FIRST + 2`.

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik context_menu` → derlenmez.

- [ ] **Step 3: Implement.**
  `context_menu.rs` (sabitler `TAB_SET_MAX`'tan sonra; belge: "1300-1304: the View menu's options (Hide extensions, Folders first, Single-click to open, Show hidden items, Show system items); 1310-1313 Date format ▸ in `DateFormat::ALL` order; 1320-1321 Size format ▸ in `SizeFormat::ALL` order."):

```rust
pub const HIDE_EXTENSIONS: u32 = 1300;
pub const FOLDERS_FIRST: u32 = 1301;
pub const SINGLE_CLICK_OPEN: u32 = 1302;
pub const SHOW_HIDDEN: u32 = 1303;
pub const SHOW_SYSTEM: u32 = 1304;
pub const DATE_FORMAT_FIRST: u32 = 1310;
pub const SIZE_FORMAT_FIRST: u32 = 1320;
```
  `view_items` (yeni imza; `mark` kapanışı aynı; `PREVIEW_PANE`'den sonra):

```rust
/// The View menu; the current choices are marked with a bullet. `options`: `[view]`'s options
/// (spec 7.2), "Show system items" only on Windows; Date format ▸ and Size format ▸ go before
/// "Apply to all folders" (`format_subs`).
pub fn view_items(view: ViewSettings, preview_open: bool, options: ViewOptions, windows: bool) -> Vec<(u32, String)> {
    // … List, Grid, sizes, sorts, directions, Preview pane as before …
    out.push((HIDE_EXTENSIONS, mark(options.hide_extensions, "Hide extensions")));
    out.push((FOLDERS_FIRST, mark(options.folders_first, "Folders first")));
    out.push((SINGLE_CLICK_OPEN, mark(options.single_click_open, "Single-click to open")));
    out.push((SHOW_HIDDEN, mark(options.show_hidden, "Show hidden items")));
    if windows {
        out.push((SHOW_SYSTEM, mark(options.show_system, "Show system items")));
    }
    out.push((APPLY_TO_ALL, "Apply to all folders".to_owned()));
    out.push((RESET_FOLDER, "Reset this folder".to_owned()));
    out
}

/// Date format ▸ and Size format ▸ of the View menu, at place `at`, the current one marked.
pub fn format_subs(options: ViewOptions, at: usize) -> Vec<Submenu> {
    let mark = |on: bool, title: &str| format!("{}{title}", if on { "• " } else { "    " });
    let dates = DateFormat::ALL
        .iter()
        .enumerate()
        .map(|(i, f)| (DATE_FORMAT_FIRST + i as u32, mark(*f == options.date_format, f.label()), true))
        .collect();
    let sizes = SizeFormat::ALL
        .iter()
        .enumerate()
        .map(|(i, f)| (SIZE_FORMAT_FIRST + i as u32, mark(*f == options.size_format, f.label()), true))
        .collect();
    vec![
        Submenu { title: "Date format".to_owned(), at, items: dates },
        Submenu { title: "Size format".to_owned(), at, items: sizes },
    ]
}

/// What View menu item `id` changes, from `options` as they are now.
pub fn view_option_for(id: u32, options: ViewOptions) -> Option<ViewOption> {
    Some(match id {
        HIDE_EXTENSIONS => ViewOption::HideExtensions(!options.hide_extensions),
        FOLDERS_FIRST => ViewOption::FoldersFirst(!options.folders_first),
        SINGLE_CLICK_OPEN => ViewOption::SingleClickOpen(!options.single_click_open),
        SHOW_HIDDEN => ViewOption::ShowHidden(!options.show_hidden),
        SHOW_SYSTEM => ViewOption::ShowSystem(!options.show_system),
        id if (DATE_FORMAT_FIRST..DATE_FORMAT_FIRST + DateFormat::ALL.len() as u32).contains(&id) => {
            ViewOption::DateFormat(DateFormat::ALL[(id - DATE_FORMAT_FIRST) as usize])
        }
        id if (SIZE_FORMAT_FIRST..SIZE_FORMAT_FIRST + SizeFormat::ALL.len() as u32).contains(&id) => {
            ViewOption::SizeFormat(SizeFormat::ALL[(id - SIZE_FORMAT_FIRST) as usize])
        }
        _ => return None,
    })
}
```
  `from_submenu`'ya `|| (DATE_FORMAT_FIRST..DATE_FORMAT_FIRST + 4).contains(&id) || (SIZE_FORMAT_FIRST..SIZE_FORMAT_FIRST + 2).contains(&id)`. `Menus::view_menu`:

```rust
    /// The View button's menu, under it.
    pub fn view_menu(&self, at: Anchor) {
        *self.subject.borrow_mut() = Some(Subject::View);
        let options = crate::view_options::current();
        let items = view_items(self.view.view_settings(), self.preview.is_pane_open(), options, cfg!(windows));
        let place = items.iter().position(|(id, _)| *id == APPLY_TO_ALL).unwrap_or(items.len());
        let entries: Vec<(u32, String, bool)> = items.into_iter().map(|(id, title)| (id, title, true)).collect();
        self.open_slint_entries(&entries, format_subs(options, place), at);
    }
```
  `run`'a (`(RESET_FOLDER, Subject::View)`'dan sonra):

```rust
            (id, Subject::View) if view_option_for(id, crate::view_options::current()).is_some() => {
                if let Some(option) = view_option_for(id, crate::view_options::current()) {
                    crate::view_options::change(option);
                }
            }
```
  `use gezik_config::settings::ViewOption; use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};`.
  `view_options.rs`:

```rust
/// Puts the macOS menu bar's View marks as `options` are (elsewhere the window has the
/// properties too, unused). Also after a choice that changed nothing: Slint flipped that
/// item's mark itself.
pub fn sync_window(options: ViewOptions) {
    let window = CONTEXT.with(|c| c.borrow().as_ref().and_then(|(window, _)| window.upgrade()));
    let Some(window) = window else { return };
    window.set_view_hide_extensions(options.hide_extensions);
    window.set_view_folders_first(options.folders_first);
    window.set_view_single_click(options.single_click_open);
    window.set_view_show_hidden(options.show_hidden);
    window.set_view_date_relative(options.date_format == DateFormat::Relative);
    window.set_view_date_short(options.date_format == DateFormat::Short);
    window.set_view_date_iso(options.date_format == DateFormat::Iso);
    window.set_view_date_system(options.date_format == DateFormat::System);
    window.set_view_size_binary(options.size_format == SizeFormat::Binary);
    window.set_view_size_decimal(options.size_format == SizeFormat::Decimal);
}
```
  `show`'un sonuna `sync_window(options);`, `install`'ın sonuna `sync_window(current());` (`use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};`).
  `menu_bar.rs`, `on_menu_command`'da `tab-set:` kolundan sonra:

```rust
                if let Some(id) = name.strip_prefix("view-option:").and_then(|i| i.parse::<u32>().ok()) {
                    let options = crate::view_options::current();
                    if let Some(option) = crate::context_menu::view_option_for(id, options) {
                        crate::view_options::change(option);
                    }
                    return crate::view_options::sync_window(crate::view_options::current());
                }
```
  `app.slint`: `menu-keys`'in altına

```slint
    // The View menu's marks on macOS (view_options.rs keeps them as the options are). Two-way:
    // Slint flips a checkable item's mark itself when it is chosen.
    in-out property <bool> view-hide-extensions;
    in-out property <bool> view-folders-first;
    in-out property <bool> view-single-click;
    in-out property <bool> view-show-hidden;
    in-out property <bool> view-date-relative;
    in-out property <bool> view-date-short;
    in-out property <bool> view-date-iso;
    in-out property <bool> view-date-system;
    in-out property <bool> view-size-binary;
    in-out property <bool> view-size-decimal;
```
  ve View menüsü:

```slint
        Menu {
            title: "View";
            MenuItem { title: "as List"; shortcut: root.menu-keys ? @keys(Control + Shift + "1") : @keys(); activated => { root.menu-command("view-list"); } }
            MenuItem { title: "as Grid"; shortcut: root.menu-keys ? @keys(Control + Shift + "2") : @keys(); activated => { root.menu-command("view-grid"); } }
            MenuSeparator { }
            MenuItem { title: "Show Preview"; activated => { root.menu-command("toggle-preview"); } }
            MenuItem { title: "Show Hidden Items"; checkable: true; checked <=> root.view-show-hidden; shortcut: root.menu-keys ? @keys(Control + Shift + ".") : @keys(); activated => { root.menu-command("toggle-hidden"); } }
            MenuItem { title: "Hide Extensions"; checkable: true; checked <=> root.view-hide-extensions; activated => { root.menu-command("view-option:1300"); } }
            MenuItem { title: "Folders First"; checkable: true; checked <=> root.view-folders-first; activated => { root.menu-command("view-option:1301"); } }
            MenuItem { title: "Single-Click to Open"; checkable: true; checked <=> root.view-single-click; activated => { root.menu-command("view-option:1302"); } }
            Menu {
                title: "Date Format";
                MenuItem { title: "Relative"; checkable: true; checked <=> root.view-date-relative; activated => { root.menu-command("view-option:1310"); } }
                MenuItem { title: "Short"; checkable: true; checked <=> root.view-date-short; activated => { root.menu-command("view-option:1311"); } }
                MenuItem { title: "ISO"; checkable: true; checked <=> root.view-date-iso; activated => { root.menu-command("view-option:1312"); } }
                MenuItem { title: "System"; checkable: true; checked <=> root.view-date-system; activated => { root.menu-command("view-option:1313"); } }
            }
            Menu {
                title: "Size Format";
                MenuItem { title: "Binary (1 KB = 1024 bytes)"; checkable: true; checked <=> root.view-size-binary; activated => { root.menu-command("view-option:1320"); } }
                MenuItem { title: "Decimal (1 kB = 1000 bytes)"; checkable: true; checked <=> root.view-size-decimal; activated => { root.menu-command("view-option:1321"); } }
            }
            MenuSeparator { }
            MenuItem { title: "Refresh"; shortcut: root.menu-keys ? @keys(Control + R) : @keys(); activated => { root.menu-command("refresh"); } }
        }
```
  (Menü komutundaki sayılar `context_menu`'nün kimlikleridir. macOS çapraz denetimi `checked <=>`'yi derlemezse: tek yönlü `checked: root.view-…` yerine işaretsiz öğe ve başlıkta `"✓ "` öneki (`title: (root.view-hide-extensions ? "✓ " : "") + "Hide Extensions"`), `checkable` yok; notlara yazılır.)

- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler (macOS denetimi `app.slint`'in menü çubuğunu derler). Elle (Windows): View düğmesi → "Hide extensions" "• " ile işaretlenir, `settings.toml`'da `hide-extensions = true` (yorum yerinde); "Date format ▸ Relative" → sütun değişir, alt menüde "• Relative"; bozuk `settings.toml` (`[view` yazıp kaydet) iken "Folders first" → durum çubuğu "settings.toml: Fix settings.toml first (…)" ve sıralama eskiye döner.
- [ ] **Step 5: Commit** "Change the view options from the View menu and the macOS menu bar".

---

### Task 8: Kenar çubuğu — gruplar, başlıklar, gruplar arası sürükleme, ipucu

**Files:**
- Modify: `crates/gezik-core/src/drag.rs` (`SideRow::{PinHeader, Pinned}`, `Hit::PinAt` satır numarası, test), `crates/gezik/src/sidebar.rs` (`SECTION_GROUP`, `PinLine`, `pin_lines`, `PinSlot`, `slot_at`, `pin_tip`, satırlar, sürükleme ve grup içi taşıma), `crates/gezik/ui/widgets/sidebar.slint`, `crates/gezik/ui/app.slint` (özellikler, iki `Sidebar`, ipucu katmanı), `crates/gezik/src/drag.rs` (satır eşlemesi, çizgi, bırakma), `crates/gezik/src/context_menu.rs` (Move up/down grup içinde), `crates/gezik/src/keys.rs` (`chord_for`'un `cfg_attr`'ı kalkar), `crates/gezik/src/main.rs` (`on_pinned_drop`, `apply_config` sırası)

**Interfaces:**
- Consumes: Task 3 (`PinEntry`, `pins::{place, pin}`, `Pin`), Task 4 (`Action::pin`).
- Produces:

```rust
// gezik-core drag.rs
pub enum SideRow { Header, PinHeader, Item, Pinned }
Hit::PinAt(usize)   // the sidebar row a line is drawn above
// sidebar.rs
pub const SECTION_GROUP: i32 = 3;
#[derive(Debug, Clone, PartialEq, Eq)] pub enum PinLine { Header(Option<String>), Pin(usize) }
pub fn pin_lines(visible: &[Pin]) -> Vec<PinLine>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct PinSlot { pub pin: usize, pub after: bool }
pub fn slot_at(lines: &[PinLine], line: usize) -> Option<PinSlot>;
pub fn pin_tip(path: &Path, key: Option<&str>) -> String;
impl Sidebar {
    pub fn drop_pinned(&self, from_row: usize, line_row: usize);
    pub fn pin_at_row(&self, paths: &[PathBuf], row: usize);
    pub fn group_ends(&self, path: &Path) -> (bool, bool);
    pub fn move_in_group(&self, path: &Path, up: bool);
    pub fn shown_groups(&self) -> Vec<String>;
    pub fn group_of(&self, path: &Path) -> Option<String>;
    pub fn relabel(&self);
}
// removed: Sidebar::{pin_at, move_pinned, visible_pinned_count}
```
```slint
// sidebar.slint: SidebarRow { …, tip: string }; in pinned-end-row; callback pinned-drop(int, int);
//                in-out tip-text, tip-x, tip-y      (pinned-count and pinned-move are gone)
// app.slint: in sidebar-pinned-end-row; callback pinned-drop(int, int);
//            in-out sidebar-tip, sidebar-tip-x, sidebar-tip-y
```

- [ ] **Step 1: Write the failing tests**

`gezik-core/src/drag.rs` (`layout`'un satırları `vec![SideRow::Header, SideRow::Item, SideRow::PinHeader, SideRow::Pinned, SideRow::Pinned, SideRow::PinHeader, SideRow::Pinned]` olur; `sidebar_items_headers_and_pin_zones`'un yerine):

```rust
    #[test]
    fn sidebar_items_headings_and_pin_lines() {
        let l = layout(list(0));
        let y = |offset: f32| 100.0 + 4.0 + offset;
        assert_eq!(hit(&l, 50.0, y(10.0), true), Hit::Nothing, "a section title");
        assert_eq!(hit(&l, 50.0, y(30.0), true), Hit::Sidebar(1));
        assert_eq!(hit(&l, 50.0, y(50.0), true), Hit::PinAt(3), "on a heading of the pinned part: its first place");
        assert_eq!(hit(&l, 50.0, y(50.0), false), Hit::Nothing, "files dragged: a heading takes nothing");
        assert_eq!(hit(&l, 50.0, y(61.0), true), Hit::PinAt(3), "top quarter of a pin: the line above it");
        assert_eq!(hit(&l, 50.0, y(70.0), true), Hit::Sidebar(3));
        assert_eq!(hit(&l, 50.0, y(99.0), true), Hit::PinAt(5), "bottom quarter: the line below it");
        assert_eq!(hit(&l, 50.0, y(110.0), true), Hit::PinAt(6), "the next group's heading");
        assert_eq!(hit(&l, 50.0, y(61.0), false), Hit::Sidebar(3), "files dragged: no pin lines");
        assert_eq!(hit(&l, 50.0, y(150.0), true), Hit::Nothing, "below the rows");
    }
```

`crates/gezik/src/sidebar.rs`:

```rust
    fn grouped(path: &str, group: Option<&str>) -> Pin {
        Pin { entry: PinEntry { path: path.into(), name: None, group: group.map(str::to_owned) }, path: PathBuf::from(path) }
    }

    #[test]
    fn pin_lines_skip_hidden_pins_and_empty_groups() {
        // As the check left them: Media's only pin and another one are not on this machine.
        let visible = [
            grouped("/a", None),
            grouped("/w1", Some("Work")),
            grouped("/w2", Some("Work")),
            grouped("/p", Some("Photos")),
        ];
        assert_eq!(
            pin_lines(&visible),
            [
                PinLine::Header(None),
                PinLine::Pin(0),
                PinLine::Header(Some("Work".into())),
                PinLine::Pin(1),
                PinLine::Pin(2),
                PinLine::Header(Some("Photos".into())),
                PinLine::Pin(3),
            ]
        );
        let only_grouped = [grouped("/w", Some("Work"))];
        assert_eq!(
            pin_lines(&only_grouped),
            [PinLine::Header(Some("Work".into())), PinLine::Pin(0)],
            "no PINNED heading without a pin under it"
        );
        assert!(pin_lines(&[]).is_empty());
    }

    #[test]
    fn a_line_finds_its_slot() {
        let lines = pin_lines(&[grouped("/a", None), grouped("/w1", Some("Work")), grouped("/w2", Some("Work"))]);
        // 0 PINNED, 1 /a, 2 Work, 3 /w1, 4 /w2; a line is drawn above row n (5: below the last).
        assert_eq!(slot_at(&lines, 0), None, "above the first heading");
        assert_eq!(slot_at(&lines, 1), Some(PinSlot { pin: 0, after: false }));
        assert_eq!(slot_at(&lines, 2), Some(PinSlot { pin: 0, after: true }), "above a heading: after the pin before it");
        assert_eq!(slot_at(&lines, 3), Some(PinSlot { pin: 1, after: false }), "the group's first place");
        assert_eq!(slot_at(&lines, 5), Some(PinSlot { pin: 2, after: true }), "below the last");
        assert_eq!(slot_at(&lines, 6), None);
    }

    #[test]
    fn a_pins_tip_is_its_path_and_its_key() {
        let path = Path::new("/home/a/Projects");
        assert_eq!(pin_tip(path, Some("Alt+1")), format!("{}\nAlt+1", path.display()));
        assert_eq!(pin_tip(path, None), path.display().to_string());
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core drag` ve `cargo test -j 8 -p gezik sidebar` → derlenmez.

- [ ] **Step 3: Implement.**
  `gezik-core/src/drag.rs`:

```rust
/// A sidebar row: a section title, a place, a pinned folder, or a heading in the pinned part
/// ("PINNED" or a group's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideRow {
    Header,
    PinHeader,
    Item,
    Pinned,
}
```
  `Hit::PinAt` belgesi: "A line in the pinned part: pins above sidebar row n (in the group of the pin there, or of the one before it; spec 6.2)." `sidebar_hit`:

```rust
    match side.rows.get(row) {
        None | Some(SideRow::Header) => Hit::Nothing,
        // On a heading: the first place of its group.
        Some(SideRow::PinHeader) if pin_zones => Hit::PinAt(row + 1),
        Some(SideRow::PinHeader) => Hit::Nothing,
        Some(SideRow::Item) => Hit::Sidebar(row),
        Some(SideRow::Pinned) => {
            let within = (offset - row as f32 * side.row_height) / side.row_height;
            if pin_zones && within < 0.25 {
                Hit::PinAt(row)
            } else if pin_zones && within > 0.75 {
                Hit::PinAt(row + 1)
            } else {
                Hit::Sidebar(row)
            }
        }
    }
```
  `crates/gezik/src/sidebar.rs`:

```rust
pub const SECTION_GROUP: i32 = 3;

/// A row of the pinned part of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinLine {
    /// "PINNED" (`None`: the pins without a group) or a group's heading.
    Header(Option<String>),
    /// Shown pin `n` (an index into the shown pins).
    Pin(usize),
}

/// The pinned part's rows for the shown pins (in `pins::normalize` order): a heading before
/// each run of one group, none before nothing (spec 6.2: a group shows only with a pin in it).
pub fn pin_lines(visible: &[Pin]) -> Vec<PinLine> {
    let mut lines = Vec::new();
    let mut current: Option<Option<&str>> = None;
    for (n, pin) in visible.iter().enumerate() {
        let group = pin.entry.group.as_deref();
        if current != Some(group) {
            lines.push(PinLine::Header(group.map(str::to_owned)));
            current = Some(group);
        }
        lines.push(PinLine::Pin(n));
    }
    lines
}

/// Where a pin dropped on a line goes: next to shown pin `pin`, before it or `after` it, in
/// its group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinSlot {
    pub pin: usize,
    pub after: bool,
}

/// The slot of the line above row `line` of the pinned part (`lines.len()`: below the last):
/// above a pin, before it; above a heading or below the last, after the pin before it.
pub fn slot_at(lines: &[PinLine], line: usize) -> Option<PinSlot> {
    if let Some(PinLine::Pin(n)) = lines.get(line) {
        return Some(PinSlot { pin: *n, after: false });
    }
    match line.checked_sub(1).and_then(|i| lines.get(i)) {
        Some(PinLine::Pin(n)) => Some(PinSlot { pin: *n, after: true }),
        _ => None,
    }
}

/// What a pinned row shows when the pointer rests on it: its full path, and the key of its
/// number (`pin-N`, the first nine).
pub fn pin_tip(path: &Path, key: Option<&str>) -> String {
    match key {
        Some(key) => format!("{}\n{key}", path.display()),
        None => path.display().to_string(),
    }
}
```
  `Inner`'a `/// The pinned part as last drawn: its rows, the sidebar row of its first, the groups shown.` `lines: Vec<PinLine>`, `first_pin_row: Option<usize>`, `groups: Vec<String>` (`new`'de boş). `update_rows`:

```rust
    /// Rebuilds the rows: sections, labels, the pinned part's headings and tips, and the
    /// highlight of the exact current location. No file system access.
    fn update_rows(&self) {
        let (lines, first, groups) = {
            let inner = self.0.borrow();
            let Some(window) = inner.window.upgrade() else { return };
            let places = inner.nav.places();
            let current = inner.nav.active_location();
            let is_current = |path: &Path| matches!(&current, Location::Path(p) if same_path(p, path));
            let index = |i: usize| i32::try_from(i).unwrap_or(i32::MAX);
            let header = |label: &str, section, i| SidebarRow {
                header: true,
                label: label.into(),
                section,
                index: i,
                active: false,
                tip: "".into(),
            };
            let item = |label: &str, section, i, path: &Path| SidebarRow {
                header: false,
                label: label.into(),
                section,
                index: index(i),
                active: is_current(path),
                tip: "".into(),
            };

            let mut rows = vec![header("FOLDERS", SECTION_FOLDERS, -1)];
            rows.extend(places.known.iter().enumerate().map(|(i, f)| item(&f.name, SECTION_FOLDERS, i, &f.path)));
            let visible = &inner.pins.visible;
            let lines = pin_lines(visible);
            let first = (!lines.is_empty()).then_some(rows.len());
            let mut groups: Vec<String> = Vec::new();
            for line in &lines {
                rows.push(match line {
                    PinLine::Header(None) => header("PINNED", SECTION_PINNED, -1),
                    PinLine::Header(Some(group)) => {
                        groups.push(group.clone());
                        header(group.as_str(), SECTION_GROUP, index(groups.len() - 1))
                    }
                    PinLine::Pin(n) => {
                        let pin = &visible[*n];
                        let label =
                            pin.entry.name.clone().unwrap_or_else(|| places.title_for(&Location::Path(pin.path.clone())));
                        let key = Action::pin(n + 1)
                            .and_then(crate::keys::chord_for)
                            .map(|chord| crate::keys::chord_label(&chord, Platform::current()));
                        SidebarRow { tip: pin_tip(&pin.path, key.as_deref()).into(), ..item(&label, SECTION_PINNED, *n, &pin.path) }
                    }
                });
            }
            let end = first.map(|_| rows.len());
            rows.push(header(DRIVES_HEADER, SECTION_DRIVES, -1));
            rows.extend(places.drives.iter().enumerate().map(|(i, d)| item(&d.label, SECTION_DRIVES, i, &d.path)));

            let as_row = |row: Option<usize>| row.map_or(-1, index);
            window.set_sidebar_pinned_first_row(as_row(first));
            window.set_sidebar_pinned_end_row(as_row(end));
            sync_model(&inner.rows, rows.into_iter());
            (lines, first, groups)
        };
        let mut inner = self.0.borrow_mut();
        inner.lines = lines;
        inner.first_pin_row = first;
        inner.groups = groups;
    }
```
  (`use gezik_config::shortcuts::{Action, Platform};`.) Sürükleme ve grup içi taşıma (`pin_at`, `move_pinned`, `visible_pinned_count` silinir):

```rust
    /// The shown pin on sidebar row `row`, as its index in the pinned list.
    fn pin_on_row(&self, row: usize) -> Option<usize> {
        let inner = self.0.borrow();
        match inner.lines.get(row.checked_sub(inner.first_pin_row?)?)? {
            PinLine::Pin(n) => inner.pins.stored_index(*n),
            PinLine::Header(_) => None,
        }
    }

    /// Where a pin dropped on the line above sidebar row `row` goes: next to which pin of the
    /// list, and whether after it.
    fn anchor_at_row(&self, row: usize) -> Option<(usize, bool)> {
        let inner = self.0.borrow();
        let slot = slot_at(&inner.lines, row.checked_sub(inner.first_pin_row?)?)?;
        Some((inner.pins.stored_index(slot.pin)?, slot.after))
    }

    /// The pinned row on sidebar row `from` was dragged to the line above row `line`: it goes
    /// there, into that group (spec 6.2).
    pub fn drop_pinned(&self, from: usize, line: usize) {
        let (Some(pin), Some((anchor, after))) = (self.pin_on_row(from), self.anchor_at_row(line)) else { return };
        self.edit(|list| {
            let path = list[pin].path.clone();
            pins::place(list, vec![path], anchor, after)
        });
    }

    /// Folders dropped on the line above sidebar row `row` (drag.rs `Hit::PinAt`) are pinned
    /// there, in that group; a folder already pinned moves there.
    pub fn pin_at_row(&self, paths: &[PathBuf], row: usize) {
        let entries: Vec<String> = {
            let inner = self.0.borrow();
            paths.iter().map(|path| inner.dirs.collapse(path)).collect()
        };
        let anchor = self.anchor_at_row(row);
        self.edit(|list| match anchor {
            Some((at, after)) => pins::place(list, entries, at, after),
            None => entries.into_iter().fold(false, |changed, entry| pins::pin(list, entry) | changed),
        });
    }

    /// Whether the shown pin of `path` is the first and the last of its group (Move up/down).
    pub fn group_ends(&self, path: &Path) -> (bool, bool) {
        let inner = self.0.borrow();
        let visible = &inner.pins.visible;
        let Some(n) = visible.iter().position(|pin| same_path(&pin.path, path)) else { return (true, true) };
        let same = |m: usize| visible.get(m).is_some_and(|other| other.entry.group == visible[n].entry.group);
        (n == 0 || !same(n - 1), !same(n + 1))
    }

    /// Move up / Move down: past the shown pin before or after it, in its group (the pins not on
    /// this machine stay where they are).
    pub fn move_in_group(&self, path: &Path, up: bool) {
        let target = {
            let inner = self.0.borrow();
            let visible = &inner.pins.visible;
            let Some(n) = visible.iter().position(|pin| same_path(&pin.path, path)) else { return };
            let Some(m) = (if up { n.checked_sub(1) } else { Some(n + 1) }) else { return };
            match visible.get(m) {
                Some(other) if other.entry.group == visible[n].entry.group => {
                    inner.pins.stored_index(n).zip(inner.pins.stored_index(m))
                }
                _ => None,
            }
        };
        let Some((from, to)) = target else { return };
        self.edit(|list| {
            let path = list[from].path.clone();
            pins::place(list, vec![path], to, !up)
        });
    }

    /// The groups shown, in order (a group heading's `index` is its place here).
    pub fn shown_groups(&self) -> Vec<String> {
        self.0.borrow().groups.clone()
    }

    /// The group of the shown pin of `path`.
    pub fn group_of(&self, path: &Path) -> Option<String> {
        let inner = self.0.borrow();
        inner.pins.visible.iter().find(|pin| same_path(&pin.path, path)).and_then(|pin| pin.entry.group.clone())
    }

    /// The pins' tips name their keys: the rows again once the shortcuts changed.
    pub fn relabel(&self) {
        self.update_rows();
    }
```
  `location_of`'ta `SECTION_GROUP` bir şeye götürmez (`_ => None` zaten).
  `sidebar.slint` (değişen yerler):

```slint
export struct SidebarRow {
    header: bool,
    label: string,
    // 0 folders, 1 pinned, 2 drives; a heading of the pinned part is 1 ("PINNED") or 3 (a
    // group's, `index` its place among the groups shown).
    section: int,
    index: int,
    active: bool,
    // Shown when the pointer rests on the row: a pinned folder's full path and its key.
    tip: string,
}

// Sections of places. Pinned rows (section 1) can be dragged, into another group too; a
// group's heading (section 3) has a menu.
export component Sidebar inherits Rectangle {
    in property <[SidebarRow]> rows;
    // The pinned part: its first row (a heading; -1: none) and the row after its last pin.
    in property <int> pinned-first-row: -1;
    in property <int> pinned-end-row: -1;
    callback clicked(int, int);
    callback middle-clicked(int, int);
    // Right-click on item (section, index), or on a group's heading (3, group), at a window position.
    callback menu(int, int, length, length);
    // The pinned row `from` (a row of `rows`) was dropped on the line above row `line`.
    callback pinned-drop(int, int);
    // … scroll, drop-row, pin-line as before …
    // The tip of the row the pointer rests on and where it goes (window position); app.slint
    // draws it over everything.
    in-out property <string> tip-text;
    in-out property <length> tip-x;
    in-out property <length> tip-y;

    property <length> row-height: Theme.row-height;
    // While a pinned row is dragged: its row, and the row the line is drawn above.
    property <int> drag-from: -1;
    property <int> drag-line: -1;
    // … dragged, drag-blocked as before …
    property <int> row-count: root.rows.length;
    changed row-count => {
        root.cancel-drag();
        root.hover-row = -1;
        root.tip-text = "";
    }
    // The row with a tip the pointer is on (-1: none).
    property <int> hover-row: -1;

    function cancel-drag() {
        root.drag-blocked = true;
        root.drag-from = -1;
        root.drag-line = -1;
    }

    // The tip shows once the pointer rests on a row for a moment.
    Timer {
        interval: 700ms;
        running: root.hover-row >= 0 && root.tip-text == "" && root.drag-from < 0;
        triggered => {
            root.tip-text = root.rows[root.hover-row].tip;
            root.tip-x = root.absolute-position.x + Theme.spacing * 3;
            root.tip-y = root.absolute-position.y + root.scroll + Theme.spacing + (root.hover-row + 1) * root.row-height + 2px;
        }
    }
```
  satır döngüsünde başlık metninden sonra:

```slint
            // A group's heading: its menu on a right-click.
            if row.header && row.section == 3: TouchArea {
                pointer-event(event) => {
                    if event.kind == PointerEventKind.up && event.button == PointerEventButton.right {
                        root.menu(row.section, row.index, self.absolute-position.x + self.mouse-x, self.absolute-position.y + self.mouse-y);
                    }
                }
                scroll-event(event) => {
                    root.scroll-by(event.delta-y);
                    accept
                }
            }
```
  satırın `touch := TouchArea`'sına:

```slint
                    changed has-hover => {
                        if self.has-hover && row.tip != "" {
                            root.hover-row = i;
                        } else if root.hover-row == i {
                            root.hover-row = -1;
                        }
                        root.tip-text = "";
                    }
```
  `pointer-event`'te sol basışta ayrıca `root.tip-text = "";`; sol bırakışta:

```slint
                        if event.kind == PointerEventKind.up && event.button == PointerEventButton.left && root.drag-from >= 0 {
                            // The line above or below itself: no move.
                            if root.drag-line != root.drag-from && root.drag-line != root.drag-from + 1 {
                                root.pinned-drop(root.drag-from, root.drag-line);
                            }
                            root.cancel-drag();
                        }
```
  `moved`:

```slint
                    moved => {
                        if row.section == 1 && self.pressed && !root.drag-blocked && (root.dragged || abs(self.mouse-y - self.pressed-y) > 6px) {
                            root.dragged = true;
                            root.drag-from = i;
                            // The line nearest the pointer, inside the pinned part.
                            root.drag-line = clamp(round(i + self.mouse-y / root.row-height), root.pinned-first-row + 1, root.pinned-end-row);
                        }
                    }
```
  ve sürükleme çizgisi:

```slint
        // A pinned row being dragged goes here.
        if root.drag-from >= 0 && root.pinned-first-row >= 0: Rectangle {
            x: Theme.spacing;
            y: Theme.spacing + root.drag-line * root.row-height - 1px;
            width: parent.width - Theme.spacing * 2;
            height: 2px;
            background: Theme.accent;
        }
```
  `app.slint`: `sidebar-pinned-count` yerine `in property <int> sidebar-pinned-end-row: -1;` (belge: "Position in `sidebar-rows` of the pinned part's first row (a heading; -1 if none), and the row after its last pin."); `callback pinned-move(int, int)` yerine `// A pinned row (a row of sidebar-rows) was dropped on the line above another row.` `callback pinned-drop(int, int);`; `// The sidebar's tip (sidebar.slint), drawn over everything.` `in-out property <string> sidebar-tip; in-out property <length> sidebar-tip-x; in-out property <length> sidebar-tip-y;`. İki `Sidebar`'da `pinned-count: …` → `pinned-end-row: root.sidebar-pinned-end-row;`, `pinned-move(a, b) => …` → `pinned-drop(a, b) => { root.pinned-drop(a, b); }`, ve `tip-text <=> root.sidebar-tip; tip-x <=> root.sidebar-tip-x; tip-y <=> root.sidebar-tip-y;`. Sürüklenen öğelerin hayaletinden (`if root.drag-active: Rectangle { … }`) sonra:

```slint
    // The sidebar's tip, inside the window.
    if root.sidebar-tip != "": Rectangle {
        x: max(4px, min(root.sidebar-tip-x, root.width - self.width - 4px));
        y: min(root.sidebar-tip-y, root.height - self.height - 4px);
        width: tip-label.preferred-width + 12px;
        height: tip-label.preferred-height + 8px;
        background: Theme.surface;
        border-width: 1px;
        border-color: Theme.border;
        border-radius: Theme.radius;
        tip-label := Text {
            x: 6px;
            y: 4px;
            text: root.sidebar-tip;
            color: Theme.foreground;
        }
    }
```
  `crates/gezik/src/drag.rs`: `use crate::sidebar::{SECTION_GROUP, SECTION_PINNED, Sidebar};`; satır eşlemesi:

```rust
                    .map(|row| match (row.header, row.section) {
                        (true, SECTION_PINNED | SECTION_GROUP) => SideRow::PinHeader,
                        (true, _) => SideRow::Header,
                        (false, SECTION_PINNED) => SideRow::Pinned,
                        _ => SideRow::Item,
                    })
```
  `show`'da `window.set_drop_pin_row(match target.hit { Hit::PinAt(row) if on => index(row), _ => -1 });`; `drop_on`'da `self.0.sidebar.pin_at_row(&d.sources, row);`.
  `context_menu.rs` `sidebar_entry`: `let (first, last) = if pinned_section { self.sidebar.group_ends(&path) } else { (true, true) };` ve `Place::Sidebar { pinned_section, pinned: self.sidebar.is_pinned(&path), first, last }` (`count`/`index` hesabı kalkar); `run`'da iki taşıma kolu yerine `(MOVE_UP | MOVE_DOWN, Subject::SidebarEntry(path)) => self.sidebar.move_in_group(&path, id == MOVE_UP),`.
  `keys.rs`: `chord_for`'un `#[cfg_attr(not(target_os = "macos"), allow(dead_code))]`'ı silinir (artık her yerde kullanılır).
  `main.rs`: `on_pinned_move` yerine:

```rust
    window.on_pinned_drop({
        let sidebar = sidebar.clone();
        move |from, line| {
            if let (Ok(from), Ok(line)) = (usize::try_from(from), usize::try_from(line)) {
                sidebar.drop_pinned(from, line);
            }
        }
    });
```
  `apply_config`'te `keys::set_shortcuts(loaded.settings.shortcuts.clone());` `sidebar::with_current(|sidebar| sidebar.set_pinned(…))`'in üstüne taşınır ve ardından `sidebar::with_current(sidebar::Sidebar::relabel);` (ipuçlarındaki tuşlar).

- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler (macOS denetimi `sidebar.slint`'in `Timer`'ını ve `changed has-hover`'ı derler). Elle (Windows): elle yazılmış iki gruplu `pinned` → "PINNED" altında grupsuzlar, sonra "Work" ve "Media" başlıkları yazıldığı gibi; bir sabitlemeyi başka grubun iki öğesi arasına sürükle → orada ve o grupta; listeden bir klasörü bir başlığın üstüne sürükle → o grubun başında; sabitleme satırında dur → yol ve "Alt+1".
- [ ] **Step 5: Commit** "Show pinned folders in their groups, drag them between groups and show their path and key".

---

### Task 9: Sabitleme menüleri, grup başlığı menüsü, `pin-1…pin-9`, macOS Go ▸ Pinned

**Files:**
- Modify: `crates/gezik/src/context_menu.rs` (kimlikler, `items`'ta `Rename…`, `group_items`, `group_heading_items`, `Subject::PinGroup`, `sidebar_entry`, `group_heading`, `run`, `from_submenu`), `crates/gezik/src/sidebar.rs` (`Dialogs`, `alias_for`, `ask_alias`, `ask_new_group`, `set_group`, `move_group`, `ask_group_name`, `ungroup`, `pin_location`), `crates/gezik/src/actions.rs` (`Pin1…Pin9`), `crates/gezik/src/main.rs` (`Dialogs` kenar çubuğundan önce), `crates/gezik/ui/app.slint` (Go ▸ Pinned 1…9)

**Interfaces:**
- Consumes: Task 3 (`pins::{set_group, set_name, move_group, rename_group, ungroup, find}`), Task 4 (`Action::pin_number`), Task 8 (`SECTION_GROUP`, `group_ends`, `shown_groups`, `group_of`).
- Produces:

```rust
// context_menu.rs
pub const RENAME_PIN: u32 = 1200;  pub const GROUP_NEW: u32 = 1201;  pub const GROUP_NONE: u32 = 1202;
pub const GROUP_MOVE_FIRST: u32 = 1210;  pub const GROUP_MAX: u32 = 50;
pub const GROUP_UP: u32 = 1260;  pub const GROUP_DOWN: u32 = 1261;  pub const GROUP_RENAME: u32 = 1262;  pub const UNGROUP: u32 = 1263;
pub fn group_items(groups: &[String], own: Option<&str>) -> Vec<(u32, String, bool)>;
pub fn group_heading_items(first: bool, last: bool) -> Vec<(u32, &'static str)>;
// sidebar.rs
impl Sidebar { pub fn new(window: &AppWindow, nav: Navigator, store: Option<ConfigStore>, dialogs: Dialogs) -> Sidebar;
    pub fn ask_alias(&self, path: &Path); pub fn ask_new_group(&self, path: &Path);
    pub fn set_group(&self, path: &Path, group: Option<&str>); pub fn move_group(&self, group: &str, up: bool);
    pub fn ask_group_name(&self, group: &str); pub fn ungroup(&self, group: &str);
    pub fn pin_location(&self, n: usize) -> Option<Location>; }
pub fn alias_for(answer: &str, folder_name: &str) -> Option<String>;
```

- [ ] **Step 1: Write the failing tests**

`context_menu.rs`:

```rust
    #[test]
    fn move_to_group_lists_the_other_groups_new_and_none() {
        let groups = vec!["Work".to_owned(), "Media".to_owned()];
        let titles = |items: Vec<(u32, String, bool)>| items.into_iter().map(|(id, t, _)| (id, t)).collect::<Vec<_>>();
        assert_eq!(
            titles(group_items(&groups, None)),
            [(GROUP_MOVE_FIRST, "Work".to_owned()), (GROUP_MOVE_FIRST + 1, "Media".to_owned()), (GROUP_NEW, "New group…".to_owned())]
        );
        assert_eq!(
            titles(group_items(&groups, Some("work"))),
            [(GROUP_MOVE_FIRST + 1, "Media".to_owned()), (GROUP_NEW, "New group…".to_owned()), (GROUP_NONE, "No group".to_owned())],
            "its own group left out; the ids go by place"
        );
        let many: Vec<String> = (0..GROUP_MAX + 5).map(|i| format!("G{i}")).collect();
        assert_eq!(group_items(&many, None).len(), GROUP_MAX as usize + 1);
        assert!(from_submenu(GROUP_MOVE_FIRST + 3) && from_submenu(GROUP_NEW) && from_submenu(GROUP_NONE));
        assert!(!from_submenu(RENAME_PIN));
    }

    #[test]
    fn a_group_heading_moves_renames_and_ungroups() {
        assert_eq!(ids(group_heading_items(true, false)), [GROUP_DOWN, GROUP_RENAME, UNGROUP]);
        assert_eq!(ids(group_heading_items(false, true)), [GROUP_UP, GROUP_RENAME, UNGROUP]);
        assert_eq!(ids(group_heading_items(false, false)), [GROUP_UP, GROUP_DOWN, GROUP_RENAME, UNGROUP]);
        assert_eq!(ids(group_heading_items(true, true)), [GROUP_RENAME, UNGROUP]);
    }
```
  `pinned_sidebar_rows_can_move_within_bounds`: `first` → `[OPEN_IN_NEW_TAB, UNPIN, MOVE_DOWN, RENAME_PIN]`, `middle` → `[OPEN_IN_NEW_TAB, UNPIN, MOVE_UP, MOVE_DOWN, RENAME_PIN]`, `folder` aynı. `conversion_ids_meet_no_others`'ın `singles`'ına `RENAME_PIN, GROUP_NEW, GROUP_NONE, GROUP_UP, GROUP_DOWN, GROUP_RENAME, UNGROUP`, aralıklarına `GROUP_MOVE_FIRST..GROUP_MOVE_FIRST + GROUP_MAX`.

`sidebar.rs`:

```rust
    #[test]
    fn an_alias_is_dropped_when_empty_or_the_folders_own_name() {
        assert_eq!(alias_for(" Gezik ", "gezik").as_deref(), Some("Gezik"));
        assert_eq!(alias_for("   ", "gezik"), None);
        assert_eq!(alias_for("gezik", "gezik"), None);
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik context_menu sidebar` → derlenmez.

- [ ] **Step 3: Implement.**
  `context_menu.rs` sabitler (`TAB_SET_MAX`'tan sonra; belge: "1200: Rename… (a pinned folder's alias). 1201: Move to group ▸ New group…; 1202: No group; 1210-1259: Move to group ▸ group N (the groups shown, by place). 1260-1263: a group heading's menu."):

```rust
pub const RENAME_PIN: u32 = 1200;
pub const GROUP_NEW: u32 = 1201;
pub const GROUP_NONE: u32 = 1202;
pub const GROUP_MOVE_FIRST: u32 = 1210;
pub const GROUP_MAX: u32 = 50;
pub const GROUP_UP: u32 = 1260;
pub const GROUP_DOWN: u32 = 1261;
pub const GROUP_RENAME: u32 = 1262;
pub const UNGROUP: u32 = 1263;
```
  `items`'ın `Place::Sidebar` kolunun sonuna `if pinned_section { out.push((RENAME_PIN, "Rename…")); }`. Ve:

```rust
/// "Move to group ▸" for a pin in group `own`: the groups shown but its own (by their place
/// among them, up to `GROUP_MAX`), "New group…", and "No group" when it has one.
pub fn group_items(groups: &[String], own: Option<&str>) -> Vec<(u32, String, bool)> {
    let mut out: Vec<(u32, String, bool)> = groups
        .iter()
        .enumerate()
        .take(GROUP_MAX as usize)
        .filter(|(_, group)| own.is_none_or(|own| !gezik_config::pins::same_group(own, group)))
        .map(|(i, group)| (GROUP_MOVE_FIRST + i as u32, group.clone(), true))
        .collect();
    out.push((GROUP_NEW, "New group…".to_owned(), true));
    if own.is_some() {
        out.push((GROUP_NONE, "No group".to_owned(), true));
    }
    out
}

/// A group heading's menu: move it (not past the first or the last), rename it, ungroup it.
pub fn group_heading_items(first: bool, last: bool) -> Vec<(u32, &'static str)> {
    let mut out = Vec::new();
    if !first {
        out.push((GROUP_UP, "Move group up"));
    }
    if !last {
        out.push((GROUP_DOWN, "Move group down"));
    }
    out.push((GROUP_RENAME, "Rename group…"));
    out.push((UNGROUP, "Ungroup"));
    out
}
```
  `Subject`'e `/// A group heading of the sidebar, by name.` `PinGroup(String)`. `Menus`'a `/// The groups the last sidebar menu listed in Move to group ▸ (items are by place).` `pin_groups: Rc<RefCell<Vec<String>>>` (`new`'de `Rc::default()`). `from_submenu`'ya `|| (GROUP_MOVE_FIRST..GROUP_MOVE_FIRST + GROUP_MAX).contains(&id) || id == GROUP_NEW || id == GROUP_NONE`. `sidebar_entry`:

```rust
    /// Right-click on sidebar entry (`section`, `index`), at window position `x`, `y`; on a
    /// group's heading, its menu.
    pub fn sidebar_entry(&self, section: i32, index: i32, x: f32, y: f32) {
        if section == SECTION_GROUP {
            return self.group_heading(index, x, y);
        }
        let Some(Location::Path(path)) = self.sidebar.location_of(section, index) else { return };
        let pinned_section = section == SECTION_PINNED;
        let (first, last) = if pinned_section { self.sidebar.group_ends(&path) } else { (true, true) };
        let place = Place::Sidebar { pinned_section, pinned: self.sidebar.is_pinned(&path), first, last };
        let mut list = owned(items(place, cfg!(windows)));
        let mut subs = Vec::new();
        if pinned_section {
            let groups = self.sidebar.shown_groups();
            let own = self.sidebar.group_of(&path);
            subs.push(Submenu { title: "Move to group".to_owned(), at: list.len(), items: group_items(&groups, own.as_deref()) });
            *self.pin_groups.borrow_mut() = groups;
        }
        list.extend(owned(terminal_items(cfg!(windows))));
        subs.push(self.copy_path_sub(std::slice::from_ref(&path), list.len()));
        self.open(Subject::SidebarEntry(path.clone()), list, subs, MenuTarget::Item(path), x, y, None);
    }

    /// Right-click on the heading of group `index` (its place among the groups shown).
    fn group_heading(&self, index: i32, x: f32, y: f32) {
        let groups = self.sidebar.shown_groups();
        let Some(i) = usize::try_from(index).ok().filter(|i| *i < groups.len()) else { return };
        *self.subject.borrow_mut() = Some(Subject::PinGroup(groups[i].clone()));
        self.open_slint(&group_heading_items(i == 0, i + 1 == groups.len()), Anchor::point(x, y));
    }
```
  `run`'a (taşıma kolunun yanına):

```rust
            (RENAME_PIN, Subject::SidebarEntry(path)) => self.sidebar.ask_alias(&path),
            (GROUP_NEW, Subject::SidebarEntry(path)) => self.sidebar.ask_new_group(&path),
            (GROUP_NONE, Subject::SidebarEntry(path)) => self.sidebar.set_group(&path, None),
            (id, Subject::SidebarEntry(path)) if (GROUP_MOVE_FIRST..GROUP_MOVE_FIRST + GROUP_MAX).contains(&id) => {
                let group = self.pin_groups.borrow().get((id - GROUP_MOVE_FIRST) as usize).cloned();
                if let Some(group) = group {
                    self.sidebar.set_group(&path, Some(&group));
                }
            }
            (GROUP_UP | GROUP_DOWN, Subject::PinGroup(group)) => self.sidebar.move_group(&group, id == GROUP_UP),
            (GROUP_RENAME, Subject::PinGroup(group)) => self.sidebar.ask_group_name(&group),
            (UNGROUP, Subject::PinGroup(group)) => self.sidebar.ungroup(&group),
```
  (`use crate::sidebar::{SECTION_GROUP, SECTION_PINNED, Sidebar};`.)
  `sidebar.rs`: `Inner`'a `dialogs: crate::dialog::Dialogs`; `new(window, nav, store, dialogs)`. Ekler:

```rust
/// The alias an answer to Rename… gives: none for an empty one or the folder's own name.
pub fn alias_for(answer: &str, folder_name: &str) -> Option<String> {
    let answer = answer.trim();
    (!answer.is_empty() && answer != folder_name).then(|| answer.to_owned())
}
```

```rust
    /// The shown pin of `path`: its path as settings.toml has it.
    fn entry_of(&self, path: &Path) -> Option<String> {
        let inner = self.0.borrow();
        inner.pins.visible.iter().find(|pin| same_path(&pin.path, path)).map(|pin| pin.entry.path.clone())
    }

    /// Rename… on a pin: its alias (spec 6.3); empty, or the folder's own name, takes it away.
    pub fn ask_alias(&self, path: &Path) {
        let found = {
            let inner = self.0.borrow();
            inner.pins.visible.iter().find(|pin| same_path(&pin.path, path)).map(|pin| {
                let folder = inner.nav.places().title_for(&Location::Path(pin.path.clone()));
                (pin.entry.path.clone(), pin.entry.name.clone().unwrap_or_else(|| folder.clone()), folder)
            })
        };
        let Some((entry, initial, folder)) = found else { return };
        let (this, dialogs) = (self.clone(), self.0.borrow().dialogs.clone());
        let message = "Name in the sidebar (empty: the folder's own):";
        dialogs.ask_text("Rename", message, initial, &["Rename", "Cancel"], move |answer| {
            let Some(answer) = answer else { return };
            let name = alias_for(&answer, &folder);
            this.edit(|list| pins::find(list, &entry).is_some_and(|i| pins::set_name(list, i, name.as_deref())));
        });
    }

    /// Move to group ▸ New group…: asks for its name; a name a group has joins that group.
    pub fn ask_new_group(&self, path: &Path) {
        let Some(entry) = self.entry_of(path) else { return };
        let (this, dialogs) = (self.clone(), self.0.borrow().dialogs.clone());
        dialogs.ask_text("New group", "Name for the group:", "", &["Create", "Cancel"], move |answer| {
            let Some(name) = answer.filter(|name| !name.trim().is_empty()) else { return };
            this.edit(|list| pins::find(list, &entry).is_some_and(|i| pins::set_group(list, i, Some(&name))));
        });
    }

    /// Move to group ▸ a group, or No group (`None`).
    pub fn set_group(&self, path: &Path, group: Option<&str>) {
        let Some(entry) = self.entry_of(path) else { return };
        self.edit(|list| pins::find(list, &entry).is_some_and(|i| pins::set_group(list, i, group)));
    }

    pub fn move_group(&self, group: &str, up: bool) {
        self.edit(|list| pins::move_group(list, group, up));
    }

    /// Rename group…: asks for the new name; a name another group has joins the two.
    pub fn ask_group_name(&self, group: &str) {
        let (this, dialogs, old) = (self.clone(), self.0.borrow().dialogs.clone(), group.to_owned());
        dialogs.ask_text("Rename group", "Name for the group:", group, &["Rename", "Cancel"], move |answer| {
            if let Some(name) = answer {
                this.edit(|list| pins::rename_group(list, &old, &name));
            }
        });
    }

    pub fn ungroup(&self, group: &str) {
        self.edit(|list| pins::ungroup(list, group));
    }

    /// Where `pin-N` goes: shown pin `n` (0-based), in the sidebar's order (spec 6.4).
    pub fn pin_location(&self, n: usize) -> Option<Location> {
        self.0.borrow().pins.visible.get(n).map(|pin| Location::Path(pin.path.clone()))
    }
```
  `actions.rs` (Pin kolları "Not theirs"'tan çıkar; modül belgesi "… and of 7a and 7b"):

```rust
        Action::Pin1
        | Action::Pin2
        | Action::Pin3
        | Action::Pin4
        | Action::Pin5
        | Action::Pin6
        | Action::Pin7
        | Action::Pin8
        | Action::Pin9 => {
            // A number with no pin shown: nothing (spec 6.4).
            let mut location = None;
            if let Some(n) = action.pin_number() {
                crate::sidebar::with_current(|sidebar| location = sidebar.pin_location(n - 1));
            }
            if let Some(location) = location {
                nav.go(location);
            }
        }
```
  `main.rs`: `let dialogs = dialog::Dialogs::new(&window);` `Sidebar::new`'den önceye; `sidebar::Sidebar::new(&window, nav.clone(), config.clone(), dialogs.clone())`.
  `app.slint` Go menüsü ("Go to Folder…"dan sonraki ayraçtan sonra, "Clear Folder History"den önce bir ayraçla):

```slint
            MenuItem { title: "Pinned 1"; shortcut: root.menu-keys ? @keys(Control + Alt + "1") : @keys(); activated => { root.menu-command("pin-1"); } }
            MenuItem { title: "Pinned 2"; shortcut: root.menu-keys ? @keys(Control + Alt + "2") : @keys(); activated => { root.menu-command("pin-2"); } }
            MenuItem { title: "Pinned 3"; shortcut: root.menu-keys ? @keys(Control + Alt + "3") : @keys(); activated => { root.menu-command("pin-3"); } }
            MenuItem { title: "Pinned 4"; shortcut: root.menu-keys ? @keys(Control + Alt + "4") : @keys(); activated => { root.menu-command("pin-4"); } }
            MenuItem { title: "Pinned 5"; shortcut: root.menu-keys ? @keys(Control + Alt + "5") : @keys(); activated => { root.menu-command("pin-5"); } }
            MenuItem { title: "Pinned 6"; shortcut: root.menu-keys ? @keys(Control + Alt + "6") : @keys(); activated => { root.menu-command("pin-6"); } }
            MenuItem { title: "Pinned 7"; shortcut: root.menu-keys ? @keys(Control + Alt + "7") : @keys(); activated => { root.menu-command("pin-7"); } }
            MenuItem { title: "Pinned 8"; shortcut: root.menu-keys ? @keys(Control + Alt + "8") : @keys(); activated => { root.menu-command("pin-8"); } }
            MenuItem { title: "Pinned 9"; shortcut: root.menu-keys ? @keys(Control + Alt + "9") : @keys(); activated => { root.menu-command("pin-9"); } }
            MenuSeparator { }
```
  (`menu_bar.rs` değişmez: bu eylemlerin tuşu var, menü onu çalar; tuş kaldırılmışsa `actions::run` çalıştırır.)

- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler. Elle (Windows): sabitlemeye sağ tık (Explorer menüsü, üstte Gezik'in öğeleri) → "Rename…" → "Gezik" → kenar çubuğunda "Gezik", `settings.toml`'da tablo; "Move to group ▸ New group…" → "Work" → "WORK" başlığı değil "Work" başlığı; grup başlığına sağ tık → "Move group up/down", "Rename group…", "Ungroup"; Alt+1…9 görünen sırayla.
- [ ] **Step 5: Commit** "Rename pins, move them between groups, edit groups from their heading, and go to pins 1-9".

---

### Task 10: Ölçüm, Linux kabı (`gui.sh pins`, `view-options`), notlar, test listeleri, spec

**Files:**
- Modify: `scripts/linux/gui.sh` (iki kip, `scoped`'un listesi, başlık, `case`, `*)`), `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` ("## Alt proje 7b (Günlük kolaylıklar: sabitlenen klasör grupları, görünüm seçenekleri) sonrası"), `docs/superpowers/notes/macos-test.md`, `docs/superpowers/notes/linux-test.md`, `docs/superpowers/specs/2026-10-08-gunluk-kolayliklar-design.md` (Durum)

- [ ] **Step 1: Exe.** `cargo build --release -p gezik -j 8`; `stat -c %s target/release/gezik.exe`. Taban 22.546.432 bayt (Global Constraints), sınır **22.808.576**. Aşılırsa görev görev dağılım ayrı bir çalışma ağacında (`git worktree add ../gezik-7b-measure <commit>`, kendi `target`'ı) ölçülür, notlara yazılır ve daraltma önerisiyle kullanıcıya sorulur (en olası büyükler: Task 8'in Slint değişiklikleri ve ipucu katmanı, Task 7'nin macOS menüsü).
- [ ] **Step 2: Bellek, açılış, 100.000 dosya.** Windows'ta master'ın exe'si ayrı çalışma ağacında (`git worktree add ../gezik-master 7151363`, `cargo build --release -p gezik -j 8`); her ikisi için `scripts/perf/measure.ps1 -Runs 5 -Config <boş geçici klasör>` (ikişer tur; bellek master'dan büyük değil, açılış ≤ ~60 ms). 100.000 dosya: `cargo test --release -j 8 -p gezik-core sorting_100k -- --ignored` (≤ 50 ms) ve `scripts/perf/stress.ps1` master ve 7b exe'leriyle (kaydırma CPU'su ve bellek master'dakini belirgin aşmaz); ayrıca `[view] folders-first = false` ve `date-format = "relative"` olan bir yapılandırmayla 7b'de bir tur daha. Linux kabında `gui.sh memory` master ve 7b exe'leriyle (5'er; RssAnon aynı).
- [ ] **Step 3: Linux kabı.** `docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/test.sh unit` → hepsi geçer. `gui.sh`'a iki kip (başlıktaki kip listesine `pins|view-options`, `case`'e `pins) scoped pins ;;` ve `view-options) scoped view_options ;;`, `*)` listesinin sonuna `pins view_options`; `scoped`'un `unset -f` listesine `srow smenu line_of view_pick`):

```bash
# 7b: pinned folders in groups (spec 6): an old plain list stays plain, aliases and groups from
# a hand-written list, Alt+1…9 in the sidebar's order, a group heading's menu, Rename… and Move
# to group ▸ on a pin, a pin dragged into another group, folders dropped on a heading.
pins() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/pn /tmp/ph /tmp/cfg && mkdir -p /tmp/pn/one /tmp/pn/two /tmp/pn/three /tmp/pn/four /tmp/pn/five /tmp/ph /tmp/cfg
    printf 'pinned = ["/tmp/pn/one", "/tmp/pn/two"]\n\n[session]\nrestore = false\n' > /tmp/cfg/settings.toml
    : >/tmp/gezik-gui-pins.log
    # An empty HOME: FOLDERS has Home only, so the sidebar's rows are where srow says.
    HOME=/tmp/ph GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/pn >>/tmp/gezik-gui-pins.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    shot pins-start
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    differ() { compare -metric AE "$SHOTS/$1.png[$3x$4+$5+$6]" "$SHOTS/$2.png[$3x$4+$5+$6]" null: 2>&1 | cut -d' ' -f1; }
    # Sidebar row N (FOLDERS 0, Home 1, then the pinned part); item K of a sidebar row's menu.
    srow() { echo $((98 + 26 * $1)); }
    smenu() { local y; y=$(srow "$1"); rclick 60 "$y"; click 130 $((y + 20 + 32 * $2)); }
    line_of() { grep -n -F "$1" /tmp/cfg/settings.toml | head -1 | cut -d: -f1; }

    # List rows: five 118, four 144, one 170, three 196, two 222. A folder's menu: Open in new
    # tab, Pin to sidebar, …
    menu /tmp/pn three 1; sleep 1.5
    check "pins: a list without aliases or groups stays one plain line" \
        'grep -qx "pinned = \[\"/tmp/pn/one\", \"/tmp/pn/two\", \"/tmp/pn/three\"\]" /tmp/cfg/settings.toml'
    click 255 400
    key alt+3; sleep 1; check "pins: Alt+3 goes to the third pin" 'is three'
    key alt+1; sleep 1; check "pins: Alt+1 to the first" 'is one'
    key alt+5; sleep 1; check "pins: a number with no pin does nothing" 'is one'

    # By hand: an alias, two groups (one spelled twice), a folder not on this machine.
    cat > /tmp/cfg/settings.toml <<'EOF'
pinned = [
  "/tmp/pn/one",
  { path = "/tmp/pn/two", name = "Second", group = "Work" },
  { path = "/tmp/pn/four", group = "Media" },
  { path = "/tmp/pn/three", group = "work" },
  "/tmp/pn/gone",
]

[session]
restore = false
EOF
    sleep 2; xdotool mousemove 500 400; sleep 0.3; shot pins-groups
    # Rows: 2 PINNED, 3 one, 4 Work, 5 Second, 6 three, 7 Media, 8 four.
    key alt+2; sleep 1; check "pins: a group's pins number together (Alt+2: Second)" 'is two'
    key alt+3; sleep 1; check "pins: \"work\" is Work (Alt+3: three)" 'is three'
    key alt+4; sleep 1; check "pins: then Media (Alt+4: four)" 'is four'
    click 60 "$(srow 5)"; sleep 1; check "pins: the aliased row opens its folder" 'is two'
    click 60 "$(srow 4)"; sleep 1; check "pins: a heading opens nothing" 'is two'
    xdotool mousemove 60 "$(srow 3)"; sleep 1.5; shot pins-tip
    check "pins: resting on a pin shows its tip" '[ "$(differ pins-groups pins-tip 300 60 0 200)" != 0 ]'
    xdotool mousemove 500 400; sleep 0.3

    # Work's heading (the first of two groups): Move group down, Rename group…, Ungroup.
    smenu 4 0; sleep 1.5
    check "pins: Move group down puts Media first" \
        '[ "$(line_of "group = \"Media\"")" -lt "$(line_of "group = \"Work\"")" ]'
    check "pins: a group is written in one spelling" '! grep -q "group = \"work\"" /tmp/cfg/settings.toml'
    # Rows: 2 PINNED, 3 one, 4 Media, 5 four, 6 Work, 7 Second, 8 three.
    key alt+2; sleep 1; check "pins: Alt+2 follows the new order" 'is four'
    smenu 6 1; sleep 0.8; key ctrl+a; typ Jobs; key Return; sleep 1.5
    check "pins: Rename group… renames all its pins" \
        '[ "$(grep -c "group = \"Jobs\"" /tmp/cfg/settings.toml)" = 2 ] && ! grep -q Work /tmp/cfg/settings.toml'
    # A pin's menu (one, the only one without a group): Open in new tab, Unpin from sidebar,
    # Rename…, Move to group ▸, Open terminal here, Copy path as ▸.
    smenu 3 2; sleep 0.8; key ctrl+a; typ Uno; key Return; sleep 1.5
    check "pins: Rename… writes an alias as a table" 'grep -qF "{ path = \"/tmp/pn/one\", name = \"Uno\" }" /tmp/cfg/settings.toml'
    smenu 3 2; sleep 0.8; key ctrl+a BackSpace Return; sleep 1.5
    check "pins: an empty name takes the alias away" \
        '! grep -q Uno /tmp/cfg/settings.toml && grep -qx "  \"/tmp/pn/one\"," /tmp/cfg/settings.toml'
    # Move to group ▸ by the keys: Media, Jobs, New group….
    rclick 60 "$(srow 3)"; sleep 0.5; key Down Down Down Down Right; sleep 0.5; key Return; sleep 1.5
    check "pins: Move to group ▸ Media" 'grep -qF "{ path = \"/tmp/pn/one\", group = \"Media\" }" /tmp/cfg/settings.toml'
    # No pin without a group is shown now, so no PINNED heading. Rows: 2 Media, 3 four, 4 one,
    # 5 Jobs, 6 Second, 7 three.
    key alt+2; sleep 1; check "pins: a pin moved into a group is its last" 'is one'
    # three (row 7) to the line above one (row 4): into Media, between four and one.
    xdotool mousemove 60 "$(srow 7)" mousedown 1; sleep 0.2
    for y in 270 240 210 195 190; do xdotool mousemove 60 $y; sleep 0.15; done
    xdotool mouseup 1; sleep 1.5
    check "pins: a pin dragged into another group takes that group" \
        'grep -qF "{ path = \"/tmp/pn/three\", group = \"Media\" }" /tmp/cfg/settings.toml'
    key alt+2; sleep 1; check "pins: ...where it was dropped" 'is three'
    # Rows: 2 Media, 3 four, 4 three, 5 one, 6 Jobs, 7 Second. The folder five from the list onto
    # Jobs' heading: pinned first in Jobs.
    xdotool mousemove 255 118 mousedown 1; sleep 0.2
    for p in "200 150" "120 170" "60 230" "60 $(srow 6)"; do xdotool mousemove $p; sleep 0.2; done
    xdotool mouseup 1; sleep 1.5
    check "pins: folders dropped on a heading join that group" \
        'grep -qF "{ path = \"/tmp/pn/five\", group = \"Jobs\" }" /tmp/cfg/settings.toml'
    key alt+4; sleep 1; check "pins: ...at its start" 'is five'
    check "pins: the pin not on this machine stays in the file" 'grep -qF "\"/tmp/pn/gone\"" /tmp/cfg/settings.toml'
    kill $gezik $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-pins.log && fail "pins: no panic" || pass "pins: no panic"
}

# 7b: the view options (spec 7): Ctrl+H writes show-hidden, the View menu the others; folders
# among files, extensions hidden on screen only, dates and sizes in their formats
# (screenshots), one click opens, a hand edit applies at once.
view_options() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/vo /tmp/cfg && mkdir -p /tmp/vo/Zeta /tmp/cfg
    echo a > /tmp/vo/alpha.txt; head -c 1500 /dev/zero > /tmp/vo/big.bin; echo h > /tmp/vo/.hidden
    echo n > /tmp/vo/new.txt; echo o > /tmp/vo/old.txt; touch -d '2020-01-02 03:04' /tmp/vo/old.txt
    printf '[session]\nrestore = false\n' > /tmp/cfg/settings.toml
    : >/tmp/gezik-gui-viewopts.log
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/vo >>/tmp/gezik-gui-viewopts.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    shot vo-start
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    differ() { compare -metric AE "$SHOTS/$1.png[$3x$4+$5+$6]" "$SHOTS/$2.png[$3x$4+$5+$6]" null: 2>&1 | cut -d' ' -f1; }
    # The View button's menu (list mode), by the keys: List 0, Grid 1, five sorts, Ascending 7,
    # Descending 8, Preview pane 9, Hide extensions 10, Folders first 11, Single-click to open
    # 12, Show hidden items 13, Date format ▸ 14, Size format ▸ 15, Apply… 16, Reset… 17.
    # view_pick N [keys…]: line N, then the keys (into a submenu), then Return. The View button
    # is at the toolbar's right end (check against vo-start if the menu does not open).
    view_pick() {
        local n=$1; shift
        click 862 57; sleep 0.5
        for _ in $(seq 0 "$n"); do key Down; done
        for k in "$@"; do key "$k"; done
        key Return; sleep 1.5
    }

    # Rows (folders first, dot names shown): Zeta 118, .hidden 144, alpha.txt 170, big.bin 196,
    # new.txt 222, old.txt 248.
    click 255 400
    key ctrl+h; sleep 2
    check "view-options: Ctrl+H writes show-hidden = false" 'grep -q "^show-hidden = false" /tmp/cfg/settings.toml'
    check "view-options: ...and the dot names go" 'key ctrl+a; copies Zeta alpha.txt big.bin new.txt old.txt'
    key ctrl+h; sleep 2
    check "view-options: Ctrl+H again brings them back" \
        'grep -q "^show-hidden = true" /tmp/cfg/settings.toml && key ctrl+a && copies .hidden Zeta alpha.txt big.bin new.txt old.txt'
    key Escape
    view_pick 11
    check "view-options: Folders first in the View menu writes folders-first = false" \
        'grep -q "^folders-first = false" /tmp/cfg/settings.toml'
    # Mixed by name: .hidden 118, alpha.txt 144, big.bin 170, new.txt 196, old.txt 222, Zeta 248.
    click 255 248
    check "view-options: the folder sorts among the files" 'copies Zeta'
    view_pick 11
    check "view-options: ...and back" 'grep -q "^folders-first = true" /tmp/cfg/settings.toml'
    key Escape; shot vo-names
    view_pick 10; shot vo-noext
    check "view-options: Hide extensions writes hide-extensions = true" 'grep -q "^hide-extensions = true" /tmp/cfg/settings.toml'
    check "view-options: a file's name is drawn without its extension" '[ "$(differ vo-names vo-noext 200 26 230 157)" != 0 ]'
    check "view-options: a folder's name stays" '[ "$(differ vo-names vo-noext 200 26 230 105)" = 0 ]'
    click 255 170; key F2; sleep 0.5; key ctrl+a ctrl+c; sleep 0.3
    check "view-options: renaming shows the whole name" '[ "$(xclip -selection clipboard -o 2>/dev/null)" = alpha.txt ]'
    key Escape; sleep 0.5
    key ctrl+f; typ txt; sleep 0.5; key Down ctrl+a
    check "view-options: the filter goes by the real names" 'copies alpha.txt new.txt old.txt'
    key Escape Escape; sleep 0.5; shot vo-system
    view_pick 14 Right; shot vo-relative
    check "view-options: Date format ▸ Relative writes it" 'grep -q "^date-format = \"relative\"" /tmp/cfg/settings.toml'
    check "view-options: a file changed just now gets a relative date" '[ "$(differ vo-system vo-relative 640 26 230 209)" != 0 ]'
    check "view-options: one from 2020 keeps the system's date" '[ "$(differ vo-system vo-relative 640 26 230 235)" = 0 ]'
    view_pick 15 Right Down; shot vo-decimal
    check "view-options: Size format ▸ Decimal writes it" 'grep -q "^size-format = \"decimal\"" /tmp/cfg/settings.toml'
    check "view-options: 1,500 bytes are 1.5 kB now, not 1.5 KB" '[ "$(differ vo-relative vo-decimal 640 26 230 183)" != 0 ]'
    view_pick 12
    check "view-options: Single-click to open writes it" 'grep -q "^single-click-open = true" /tmp/cfg/settings.toml'
    xdotool keydown ctrl; click 255 118; xdotool keyup ctrl; sleep 1
    check "view-options: Ctrl+click only selects" 'is vo'
    click 255 118; sleep 1.5
    check "view-options: one click opens a folder" 'is Zeta'
    printf '[session]\nrestore = false\n\n[view]\nsingle-click-open = false\n' > /tmp/cfg/settings.toml; sleep 2
    key alt+Up; sleep 1; click 255 118; sleep 1
    check "view-options: a hand edit applies at once (one click selects again)" 'is vo'
    kill $gezik $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-viewopts.log && fail "view-options: no panic" || pass "view-options: no panic"
}
```
  Koş: `… gezik-linux bash scripts/linux/gui.sh pins`, `view-options`, sonra `all`. Kenar çubuğu satırları (`srow`: FOLDERS'ın orta noktası y 98, satırlar 26 piksel), View düğmesi (862, 57) ve menü satırı sayıları ekran görüntüsünden (`.superpowers/linux-shots/pins-start.png`, `vo-start.png`) doğrulanır; düşen bir denetimde önce ekran görüntüsüne bakılır, sayılar düzeltilir; fare ile açılan Gezik menüsü tuşları almıyorsa `Move to group ▸` adımı açıcı satıra ve alt menünün ilk satırına tıklayarak yapılır. Uygulamadaki bir hata düzeltilir ve yeniden koşulur. `filter` kipinin Ctrl+H adımı artık `settings.toml` yazar (dosya yoksa şablondan oluşturulur); düşerse bu değişiklik notlara. Sonuçlar notlara.
- [ ] **Step 4: Notlar** (Türkçe, 7a bölümünün kalıbında): ölçümler (exe: taban ve sınırla bayt ve MiB, toplam ölçüte kalan; boşta bellek Windows ve Linux; 100.000 dosyada sıralama ve `stress.ps1`), sapmalar (1-24 kısaca; özellikle `pins.rs`, normalleştirme, okunamayan girdilerin korunması, tek satır/çok satır biçimi, satır numaralı `PinAt`, iyimser seçenekler, korunan öğe kuralı, `show-hidden`'ın şablonda yorum olması, macOS `checked <=>`'nin derlenip derlenmediği, Alt+rakam kuralı), `gui.sh` sonuçları (yeni iki kip ve `all`), Windows ekran testi sonuçları (aşağıdaki liste), denenemeyenler.
  - `macos-test.md` "7b, daily" başlığıyla (sıradaki numaralar 61-65): (61) elle yazılmış iki gruplu `pinned`: başlıklar, grup başlığına sağ tık menüsü, gruplar arası sürükleme, Rename… ve Move to group ▸; (62) ⌘⌥1…9 ve Go ▸ Pinned 1…9 (ABD, Türkçe Q ve AZERTY'de; menüdeki kısayol tetikleniyor mu, yoksa tuş pencereye mi gidiyor); (63) View menüsünün işaretli öğeleri ve Date Format/Size Format alt menüleri (işaret seçilene taşınıyor mu, `settings.toml`'u elle değiştirince işaretler güncelleniyor mu); ⌘⇧. `show-hidden`'ı yazıyor ve "Show Hidden Items" işaretleniyor; (64) uzantı gizleme, klasörler karışık, göreli tarih, ondalık boyut (Finder'la aynı `kB`), tek tıkla açma; (65) macOS varsayılanı `show-hidden = false`: `.DS_Store` görünmez, şablonda satır yorum.
  - `linux-test.md` (47-49): Keys on Linux tablosuna Alt+1…9 (pinned folders; masaüstü Alt+rakamı alıyor mu); sabitleme grupları ve menüleri; görünüm seçenekleri (Ctrl+H `show-hidden`'ı yazar, `decimal` GNOME'la aynı).
  - Spec Durum: "Tasarım onaylandı (2026-10-07); 7a ve 7b uygulandı".
- [ ] **Step 5: Commit** "Measure 7b and note what it does".

---

## Ekran testleri (Windows, alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici. Test verisi `%TEMP%\gezik-gui-7b\`: `one\` … `five\`, `alpha.txt`, `big.bin` (1.500 bayt), `old.txt` (2020 tarihli), `.hidden.txt`, `gizli.txt` (`attrib +h`). Klavye düzeni önce ABD, sonra Türkçe Q (madde 3).

1. Boş yapılandırmada iki klasörü "Pin to sidebar" ile sabitle → `settings.toml`'da `pinned = ["…", "…"]` tek satır, tablo yok; şablonun `pinned` yorumları yerinde.
2. `settings.toml`'a elle iki gruplu liste (takma adlı biri, bu makinede olmayan biri, aynı grubun farklı yazılışı) → kenar çubuğunda "PINNED", "Work", "Media" başlıkları yazıldığı gibi; olmayan sabitleme ve yalnız ondan oluşan grup görünmez; takma adlı satır takma adla; Gezik bir sonraki yazımında grupları yan yana ve tek yazılışla yazar, olmayan girdi dosyada kalır.
3. Alt+1…9 görünen sırayla; olmayan numara bir şey yapmaz; adres çubuğu yazılırken Alt+1 sabitlemeye gider; Ctrl+1 hâlâ sekme 1. Türkçe Q: Alt+1 çalışır, AltGr+7 adres çubuğunda `{` yazar ve bir şey açmaz.
4. Sabitleme satırında dur → ipucu: tam yol ve "Alt+1" (dokuzuncudan sonrakilerde tuş satırı yok); `[shortcuts] pin-1 = ""` → ipucunda tuş satırı yok.
5. Sabitlemeye sağ tık (Explorer menüsü, Gezik'in öğeleri üstte): Move up/Move down yalnız grup içinde (grubun ilkinde Move up yok); "Rename…" → takma ad; boş bırak → klasör adı geri, dosyada düz metin; "Move to group ▸" (Windows alt menüsü): diğer gruplar, "New group…", gruplu sabitlemede "No group".
6. Grup başlığına sağ tık (Slint menüsü): "Move group up/down" (ilkte/sonda olmayan), "Rename group…" (var olan bir adla iki grup birleşir), "Ungroup" (öğeler grupsuzların sonuna).
7. Bir sabitlemeyi grup içinde ve başka grubun iki öğesi arasına sürükle → çizgi doğru yerde, bırakınca o grupta; bir grup başlığının üstüne bırak → hiçbir şey ya da o grubun başı (çizgi gösterdiği gibi); listeden bir klasörü bir başlığa bırak → o grubun başında sabitlenir; dosya sürüklenirken başlık ve çizgi bölgeleri bir şey almaz.
8. View düğmesi → "Hide extensions", "Folders first", "Single-click to open", "Show hidden items", "Show system items" "• " ile işaretli; "Date format ▸", "Size format ▸" alt menüleri ve seçili olan işaretli; her seçim `settings.toml`'a yazılır, şablonun satır yorumları kalır.
9. Ctrl+H → `show-hidden = false`, `.hidden.txt` ve `gizli.txt` (gizli öznitelik) gider; yeniden aç → ayar korunur; Ctrl+H → geri gelir.
10. `C:\`'de `$RECYCLE.BIN` ve `System Volume Information`, Masaüstü'nde `desktop.ini` görünmez; "Show system items" → görünürler (`show-hidden = false` iken de).
11. "Hide extensions" → adlar uzantısız (klasörler ve `.hidden.txt` aynen); F2 tam adı gösterir, uzantı hariç kısım seçili; süzgeçte `txt` dosyaları bulur; harfle atlama gerçek adla.
12. Tarih: `system` (bölge ayarının kısa tarihi ve saati), `short` (yalnız tarih), `iso` (`2026-10-08 14:05`), `relative`: yeni oluşturulan dosya "1 min ago", birkaç dakika sonra fareye dokunmadan güncellenir, saatler önce değişen "Today …", dün değişen "Yesterday …"; önizleme panosu ve çakışma listesi de aynı biçimde.
13. Boyut `decimal`: sütunda `1.5 kB`, durum çubuğunun seçim özeti, önizleme, büyük bir kopyalamada işlem panelinin hızı (`MB/s`) ve çakışma listesi `kB`/`MB` ile.
14. Tek tıkla açma: tık klasörü ve dosyayı açar; Ctrl+tık ve Shift+tık seçer; basılı tutup sürüklemek sürükler; sağ tık seçip menüyü açar; çift tık iki kez açmaz; `[archives] double-click = "extract-here"` iken arşive tık → yanına çıkarır.
15. "Folders first" kapalı: her sıralama sütununda klasörler dosyalarla karışık; ızgarada da.
16. `settings.toml` bozukken (`[view` yazıp kaydet) View menüsünden bir seçenek → durum çubuğu "settings.toml: Fix settings.toml first (…)" ve görünüm eski haline döner; dosya düzeltilince elle yazılan değerler hemen uygulanır.
17. Exe boyutu (≤ 22.808.576 bayt), boşta bellek (Görev Yöneticisi, master'dan büyük değil) ve 100.000 dosyada kaydırma (`stress.ps1`), Task 10'un sayılarıyla.

---

## Self-review

- **Spec kapsamı:** §6.1 biçim ve geçiş (metin ya da `{ path, name, group }`, tanınmayan anahtar ve boş yol uyarıyla atlanır, yinelenen yol Windows'ta harf büyüklüğü fark etmeden, takma adsız/grupsuz düz metin, `Settings.pinned: Vec<PinEntry>`, `with_pinned`) → Task 3 (sapma 1, 3, 4). §6.2 gruplar (önce PINNED, sonra yazıldığı gibi grup başlıkları, ilk öğe sırası, yan yana yazım, boşalan grup kaybolur, başlık menüsü, gruplar arası sürükleme ve `Hit::PinAt`'ın grubu belirlemesi, başlık sürüklenmez, görünür öğesiz başlık çizilmez) → Task 3 (`normalize`, `move_group`, `rename_group`, `ungroup`) + Task 8 (`pin_lines`, `slot_at`, `SideRow::PinHeader`) + Task 9 (başlık menüsü) (sapma 2, 5, 6, 7). §6.3 Rename…, ipucunda tam yol, Move to group ▸ (gruplar, New group…, No group), Move up/down grup içinde → Task 8 (`move_in_group`, `pin_tip`) + Task 9 (sapma 8, 9). §6.4 `pin-1…pin-9` görünen ilk dokuz, Alt+1…9 / ⌘⌥1…9, numara ipucunda → Task 4 + Task 8 + Task 9 (sapma 10). §7.1 yedi ayar ve varsayılanları, tarih ve boyut biçimleri, `format_size`'ın biçim alması, uzantı gizlemenin yalnız çizim olması, tek tıkla açma kuralları, gizli/sistem kuralı, `MetadataExt::file_attributes`, `Entry` bayrak baytı ve `size_of` testi → Task 1 + Task 2 + Task 4 + Task 5 + Task 6 (sapma 12, 14, 16-19). §7.2 View menüsünde işaretli öğeler ve iki alt menü, macOS View menüsü, `SettingsChange::ViewOption` yorumu koruyarak, izleyiciyle her yere uygulanma, `toggle-hidden`'ın `show-hidden`'ı yazması → Task 4 + Task 6 + Task 7 (sapma 13, 15, 20). §10.1 şablon (`pinned` yorumu, `[view]` satırları) → Task 3 + Task 4 (sapma 12). §10.3 `pin-1…pin-9` varsayılanları, çakışma denetimi, Ctrl+Alt'tan kaçınma, rakam tuşu kuralı (Alt+rakam, ⌘⌥rakam, AZERTY), `Action` listesi (50 → 59), şablon yorumu, macOS Go ▸ Pinned 1…9, ulaşılabilirlik testi → Task 4 + Task 9 (sapma 11, 20). §11 kod yapısı → dosya listeleri (`pins.rs` ve `view_options.rs` yeni, sapma 1). §11.1 kimlik aralığı → Task 7, Task 9 (sapma 21). §12 birim testleri (`pinned` karışık okuma, takma ad/grup, yinelenen ve `..`, eski biçim, metin bırakma, grupların yan yana tutulması, grup sırası; tarih ve boyut biçimleri ve sınırları, `folders-first` kapalı, gizli/sistem, noktalı adlar, `Entry` boyutu; Alt+rakam ve ⌘⌥rakam AZERTY'de, ulaşılabilirlik), Linux kabı `pins` ve `view-options`, Windows ekran testleri, macOS listesi → Task 1-10 ve ekran listesi. §13 liste performansı (ek sistem çağrısı yok, `Entry` büyümez, biçimler yalnız görünen satırda, `folders-first` bir karşılaştırma), bellek, exe → Global Constraints + Task 10. 7a'nın spec maddeleri ve 7c'ninkiler (şablonlar, pano, bağlantılar, yığın, günlük; `new-folder-with-selection`, `add-to-stack`) bu planda yok.
- **Yer tutucu taraması:** Task 4'teki `actions.rs` Pin kolları bilerek "Not theirs" listesinde (false döner) ve Task 9 doldurur. Task 6 `sync_window`'u çağırmaz; Task 7 ekler (pencere özellikleri Task 7'de). Task 7'de macOS `checked <=>` derlenmezse ne yapılacağı yazılı. Task 10'da `gui.sh` koordinatları (kenar çubuğu satırları, View düğmesi) hesapla verildi; ilk ekran görüntüsüyle doğrulanacağı yazılı. Gövdesi yazılı olmayan test yok.
- **Tip tutarlılığı:** Task 1 `Entry.flags`, `Entry::{HIDDEN, SYSTEM, is_shown}`, `shown_name`, `sort_entries(entries, spec, folders_first, type_name)`. Task 2 `DateFormat::{ALL, as_str, parse, label}`, `SizeFormat::{ALL, as_str, parse, label}`, `ViewOptions` (yedi alan), `relative_date`, `format_size_in`, `format_rate(per_second, SizeFormat)`. Task 3 `PinEntry::{plain}`, `pins::{parse_pin, pin_to_value, same_path_text, same_group, find, paths, normalize, groups, pin, unpin, place, set_group, set_name, move_group, rename_group, ungroup}`, `with_pinned(&[PinEntry])`, `SettingsChange::Pinned(Vec<PinEntry>)`, `Pin { entry, path }`, `check_pins(&[PinEntry])`, `Sidebar::edit`. Task 4 `ViewDefaults.options`, `ViewOption::{key, apply}` (yedi kol), `with_view_option`, `SettingsChange::ViewOption`, `Action::{Pin1…Pin9, pin_number, pin}`, `digit_chord`. Task 5 `format_date(time, DateFormat, now)`. Task 6 `view_options::{install, current, set_from_file, change, toggle_hidden, size_text, date_text}`, `OptionsState::{read, change, written}`, `View::{set_options, take_plain_press}`, `visible_lines`, `Listing::without_hidden`, `ViewData.options`, `open_entry(nav, view, usize)`. Task 7 `HIDE_EXTENSIONS…SHOW_SYSTEM`, `DATE_FORMAT_FIRST`, `SIZE_FORMAT_FIRST`, `view_items(view, preview_open, options, windows)`, `format_subs(options, at)`, `view_option_for(id, options)`, `view_options::sync_window`, `view-*` pencere özellikleri. Task 8 `SideRow::{Header, PinHeader, Item, Pinned}`, `Hit::PinAt(row)`, `SECTION_GROUP`, `PinLine`, `pin_lines`, `PinSlot`, `slot_at`, `pin_tip`, `Sidebar::{drop_pinned, pin_at_row, group_ends, move_in_group, shown_groups, group_of, relabel}`, `pinned-end-row`, `pinned-drop`, `sidebar-tip*`. Task 9 `RENAME_PIN`, `GROUP_*`, `UNGROUP`, `group_items`, `group_heading_items`, `Subject::PinGroup`, `Sidebar::{new(…, dialogs), ask_alias, ask_new_group, set_group, move_group, ask_group_name, ungroup, pin_location}`, `alias_for`. Task 8'in kaldırdığı `pin_at`, `move_pinned`, `visible_pinned_count`'u kullanan üç yer (drag.rs, main.rs, context_menu.rs) aynı görevde değişir.
- **Review Focus:** beş maddenin her biri adı geçen görevde test olarak var: 1 → Task 3 `pins_of_both_forms_are_read_and_bad_ones_warned`, `old_lists_are_written_as_they_were`, `unreadable_pins_are_kept_after_ours`, `normalize_puts_groups_together_and_trims`; 2 → Task 8 `pin_lines_skip_hidden_pins_and_empty_groups`, `a_line_finds_its_slot`, Task 3 `place_moves_and_groups` (`/gone`) + `gui.sh pins` (`/tmp/pn/gone`); 3 → Task 1 `hidden_and_system_items_follow_the_two_settings`, `the_flags_fit_in_the_padding_after_is_dir`, `windows_attributes_reach_the_entry`; 4 → Task 6 `a_reload_while_writes_are_on_their_way_is_ignored`, `a_failed_write_goes_back_to_the_file`; 5 → Task 4 `alt_and_a_digit_key_is_the_pin_on_every_layout`, `pins_have_alt_and_cmd_option_digits`, `every_default_is_reachable_on_windows_and_linux`.
