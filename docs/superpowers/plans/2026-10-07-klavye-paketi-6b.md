# Klavye Paketi 6b (Yol Tamamlama, Klasör Geçmişi ve Kullanıcı Komutları) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adres çubuğunda yazarken alt klasör önerileri (iş parçacığında okuma, 80 ms bekleme, 1 sn sınır), `~` ve ortam değişkeni açma; ziyaret edilen klasörlerin "frecency" puanıyla tutulması ve boş adreste "Recent"/"Frequent" listesi, yazarken geçmişten eşleşenler, silinmiş yerel klasörlerin düşmesi, `[history] remember` ve `clear-history`; `[[commands]]`'a `shortcut`, `menu`, `ask` alanları ve bütün seçimi tek çalıştırmaya veren `{files}`.

**Architecture:** Saf kurallar `gezik-core`'da: klasör geçmişi ve puan `history.rs` (yeni), yazılan metnin açılması `nav::expand_typed`, son parçanın ayrılması ve önerilerin sıralanması `complete.rs` (yeni), `{files}` ve komut satırı uzunluğu `batch/convert.rs`. Komut satırı sınırı `gezik-platform::process`'te (Windows 32.000, Unix `ARG_MAX`/2). `gezik-config` `[history]` ayarını, `state.toml`'daki ziyaretleri, yeni komut alanlarını okur ve komut kısayollarını kısayol tablosuna aynı çakışma kuralıyla bağlar (`Shortcuts::bind_command`). `gezik-batch::CommandTask` `{files}` komutunu seçimin klasöründe bir kez çalıştırır ve geri alınacak bir şey bırakmaz. Arayüzde `path_box.rs` (yeni) adres çubuğunun öneri listesini yönetir: arayüz iş parçacığı diske hiç dokunmaz, klasörü ve silinmiş klasör denetimini iş parçacıkları yapar; ziyaretler `Navigator::on_visited` ile gelir ve `ConfigStore::update_state`'in yazıcı iş parçacığıyla `state.toml`'a gider. Komutlar `convert.rs`'te kısayolla, "Commands ▸" menüsünden ve macOS menü çubuğundan aynı yoldan (`run_checked` → `run_asked` → `run_command`) çalışır.

**Tech Stack:** Rust 2024, Slint 1.18, `toml` (var olan), `libc` (yalnız `gezik-platform`'da, var olan). Yeni crate yok.

**Spec:** `docs/superpowers/specs/2026-10-07-klavye-paketi-design.md` (bölüm 6, 7, 8'in 6b kısmı, 9-11)

## Spec'ten sapmalar ve netleştirmeler

1. **`menu` alanı "Commands ▸" içinde gri başlık olarak.** Bugün üç menü yolunun üçü de (Windows'ta `gezik_platform::show_shell_menu`'nün `ShellSubmenu`'sü, Slint `ContextMenuArea` (Windows/macOS'ta sistem menüsü), Linux'ta Gezik'in kendi `PopupMenu`'sü) tek düzey alt menü taşıyor. İç içe alt menü üçünü de değiştirmeyi ister; 6b'nin ~0,18 MB'lık exe payını ve riskini aşar. Bu yüzden aynı `menu` adını taşıyan komutlar "Commands ▸" içinde o adın gri başlığının altında toplanır: önce grupsuzlar (dosyadaki sırayla), sonra her grup ilk göründüğü sırayla. Başlık satırının kimliği `COMMAND_GROUP = 970` (Windows menüsü her satıra 1..1000 arası bir kimlik ister; 970-999 boş). Gerçek iç içe menü notlara "sonra" olarak yazılır.
2. **Kısayolla çalıştırma.** Komut seçimdeki öğelere, seçim yoksa odaktaki öğeye uygulanır. Hiçbiri `types`/`folders` kurallarına uymuyorsa çalışmaz ve durum çubuğu nedenini yazar (`Small runs on .jpg and .png files only`, `Touch does not run on folders`). Bir kısmı uyuyorsa uyanlar üzerinde çalışır (menüdeki gibi; per-öğe komutta uymayanlar "skipped", `{files}`'ta argümana girmez). Programı bulunamayan ya da klasörü yerinde değiştirecek komut için durum çubuğu menüdeki gri yazıyı söyler. "This PC"de: `Commands run on files and folders`. Kısayol, kopyala/sil gibi yalnız liste klavyedeyken çalışır (adres çubuğu ve süzgeç alanı yazıyorken bekler).
3. **Komut kısayolu ayrıştırılamazsa** komut kalır, yalnız kısayolu düşer (`commands[N]: shortcut: unknown key "x"; the command has no key`); çakışmada da aynı (`commands[N]: shortcut "ctrl+f" is already used by filter; the command has no key`). Eylemler (varsayılan ya da `[shortcuts]`'ta yazılmış) ve dosyada önce gelen komut kazanır. Tür hatası (`shortcut = 3`, `menu = true`, `ask = "yes"`) öteki alanlardaki gibi komutu dışarıda bırakır.
4. **`{files}` kuralları.** `{files}` kendi başına bir argüman olmalı (`"--in={files}"` hata: `{files} must be an argument of its own`); `{in}`, `{name}`, `{ext}`, `{out}` ve `output` ile birlikte hata (`{files} can't be used with {in}, {name}, {ext}, {out} or output`). `{dir}` ve `{outdir}` seçimin klasörüdür. `parallel` yok sayılır (tek çalıştırma). **Pause onu durdurmaz** (yeniden başlatmak yaptığını yineler), Cancel durdurur. Uzunluk denetimi `ask` sorusundan önce yapılır; aşınca soru gelmez, durum çubuğu `Too many items for one run of <ad>` der. İş geri alma listesine girmez (`Outcome::Nothing`), panel satırının ayrıntısına ` · can't be undone` eklenir.
5. **`ask` nerede sorar:** "Commands ▸" menüsü, kısayol ve macOS menü çubuğu. Convert katmanının Start düğmesi zaten bir onaydır, sormaz; panelin "Retry"ı da sormaz. Soru: başlık komutun adı, metin `Run <ad> on N items?` (`1 item`; binlik ayırıcıyla), düğmeler Run / Cancel.
6. **macOS menü çubuğu:** kullanıcı kararıyla yalnız kısayolu olan komutlar bir "Commands" menüsünde, "Commands ▸" ile aynı gruplamayla; tuş, başlığın sonunda metin olarak (`Zip    ⌃⌥Z`): Slint'te `MenuItem.shortcut` `@keys(...)` derleme anı değeridir, Rust'tan atanamaz. `clear-history` Go menüsünde ("Clear Folder History"). Slint `MenuBar` içinde `if`/`for` derlenmezse Commands menüsü düşer ve notlara yazılır (Task 6, Adım 4).
7. **Adres çubuğu tuşları** (yalnız Ctrl/Alt/⌘'siz basışlar): ↓ liste kapalıyken açar, açıkken aşağı iner; ↑ yukarı çıkar, ilk satırdan metne döner; Tab açık listede seçili satırı (yoksa ilkini) alana yazar (sonuna ayırıcı), kapalıyken listeyi açar; Shift+Tab alanın; → yalnız bir satır seçiliyken yazar, yoksa imleci oynatır; Enter seçili satır varsa oraya gider, yoksa yazılan metne; Esc önce listeyi, ikinci Esc yazmayı kapatır. Satıra tıklamak oraya gider. Liste ilk düzenlemeyle açılır (Ctrl+L'nin seçili yolu için açılmaz; ↓ açar).
8. **Okuma.** Zaman sınırı her okuma için 1 sn (yerel okumalar ms sürer, ağ yolunu ayrıca tanımaya gerek yok). Sınır aşılırsa liste boş kalır (yalnız geçmiş eşleşmeleri); geç gelen sonuç önbelleğe girer ama listeyi değiştirmez, bir sonraki tuşta kullanılır. Okunmakta olan bir klasör yeniden okunmaz (asılı bir ağ paylaşımı iş parçacığı yığmaz). Okunan klasörün alt klasörleri o yazma boyunca bellekte; yazma bitince unutulur. Nokta ile başlayan klasörler yalnız yazılan da nokta ile başlıyorsa önerilir; Windows'un gizli özniteliğine bakılmaz. "This PC"deyken göreli metne öneri yok.
9. **Ziyaret:** başarılı bir taşınma (gezinme, geri/ileri, yukarı, yazılan yol, kenar çubuğu, satıra çift tık) ya da etkin açılan bir sekmenin ilk gösterimi. Yenileme, dosya izleyicinin yeniden yüklemesi, sekme değişimi ve arka planda açılan sekme sayılmaz. Zaman Unix saniyesi; puan çeyrek birimle tam sayı (×4 → 16, ×2 → 8, ×1 → 4, ×0,25 → 1). Aynı saniyedeki ziyaretlerde sonraki daha yeni sayılır.
10. **`state.toml`:** `[history] folders = [{ path, count, last }]` (`toml` bunu `[[history.folders]]` olarak yazar). Yollar bu makinenin yazımıyla: `state.toml` makineye aittir, `{home}` jetonu kullanılmaz. Bozuk kayıt sessizce atlanır.
11. **Silinmiş klasörler** yalnız Recent/Frequent listesi gösterilirken, gösterilen satırlar için, yazma başına bir kez denetlenir (iş parçacığında). Düşme koşulu: `metadata` "yok" der, yol `\\`/`//` ile başlamaz ve `gezik_platform::fs::drive_facts` sürücüyü ağ saymaz. Linux'ta major 0 aygıtlar (tmpfs, btrfs, overlay; kap dahil) 5c'deki `disk_kind` ile ağ sayılır, oradaki silinmiş klasörler düşmez. Bu zararsızdır ve notlara yazılır.
12. **`remember = false`** kaydı kapatır ve var olanı siler. Silme yalnız geçmiş boş değilse yazılır (her yapılandırma yeniden yüklemesinde değil). `clear-history` durum çubuğunda `Folder history cleared` der.
13. **Açma kuralları:** Windows'ta yalnız `%AD%`, Unix'te yalnız `$AD` ve `${AD}` (ad: harf/rakam/`_`, rakamla başlamaz). `~` yalnız başta, ardından ayırıcı ya da metin sonu gelince açılır; `~veli` açılmaz. Açma yalnız adres çubuğunda yapılır (`navigate_text` ve öneriler).
14. **Liste:** en çok 12 klasör önerisi; altında "History" başlığıyla geçmişten en çok 5 eşleşme (öneride zaten olanlar hariç; yolun herhangi bir yerinde geçen metin; Windows'ta `/` `\` sayılır). Boş adres, `~`, `/`, Windows'ta `\`, `C:`, `C:\`, `C:/`: "Recent" (son 5) ve "Frequent" (puana göre ilk 7, Recent'takiler hariç).
15. **Dal:** `feat/keyboard-6b`, `feat/keyboard-6a`'nın ucundan (zincirli; 6a henüz master'da değil). 6a'nın düzeltme dalgası (`edit_settings`'in tek yazıcı iş parçacığına taşınması dahil) işlendikten sonra açılır. 6a master'a girince 6b master'a yeniden tabanlanır.

## Global Constraints

- Rust edition 2024, stable (1.99). Komutlar `D:\Work\gezik`'ten; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Dal `feat/keyboard-6b`, `feat/keyboard-6a`'nın ucundan; aynı depoda başka bir çalışma sürüyorsa ayrı bir çalışma ağacında.
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- Çapraz denetimler: `cargo check -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- Yeni bağımlılık yok. `gezik-core` bağımlılıksız kalır; `libc` yalnız `gezik-platform`'da (var olan `cfg(unix)` bağımlılığı).
- Arayüz iş parçacığı dosya sistemine dokunmaz: öneri okumaları ve silinmiş klasör denetimi iş parçacıklarında; okuma yazmaya 80 ms ara verilince başlar, yeni tuş öncekini geçersiz kılar; 1 sn'yi aşan okuma listeyi boş bırakır; arayüze yalnız sonuç listesi gelir.
- Yol tamamlama önerileri yazmaya ara verildikten sonra ≤ 100 ms'de gelir (80 ms bekleme + 10.000 alt klasörde okuma ve sıralama ≤ 20 ms, sürüm derlemesi).
- Boşta bellek değişmez (≤ 7 MB); exe büyümesi 6a + 6b ≤ +0,5 MB, master `0eb16fd`'ye göre (21.741.056 bayt → en çok 22.241.056 bayt); 6a 0,32 MB kullandı, 6b'ye ~0,18 MB kalıyor.
- Her yeni eylem (`clear-history`) `[shortcuts]` ile değiştirilebilir, şablonda yorum satırı olarak görünür ve macOS menü çubuğunda bir öğesi vardır.
- `state.toml` yazımı yalnız var olan yazıcı iş parçacığından (`ConfigStore::update_state`); 6b `settings.toml`'a hiç yazmaz.
- `{files}` komut satırı sınırı: Windows'ta 32.000 karakter (UTF-16 birimi), Unix'te `sysconf(_SC_ARG_MAX)`'ın yarısı. Geri alma yok; panelde "can't be undone".
- Komutun `shortcut`'ı kısayol tablosuna aynı çakışma kurallarıyla girer; çakışmada komutun tuşu uyarıyla yok sayılır.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Claude imzası yok.
- Yol birleştirme yalnız `Path::join`. Tek istisna alana yazılan metin: `accept_text` sonuna `MAIN_SEPARATOR` ekler. Bu bir yol birleştirmesi değil, kullanıcının göreceği metindir.

## Review Focus

1. **Asılı bir ağ yolu ya da kopuk eşlenmiş sürücü adres çubuğunda:** `\\10.255.255.1\x\` yazmak arayüzü dondurmaz, liste 1 sn'de boş kalır, yazmaya devam edilir. Aynı klasörü yeniden yazmak ikinci bir asılı okuma başlatmaz. → Task 7 `a_folder_is_read_once_at_a_time` testi, Windows ekran listesi madde 3-4.
2. **Özel adlar `{files}` ile:** boşluk, `&`, `;`, tek tırnak, `-` ile başlayan ad, Türkçe harf; her biri tek argüman. Sınır Windows'ta UTF-16 birimiyle sayılır (😀 iki birim), sınırı aşan seçim hiç çalışmaz. → Task 3 `files_go_in_as_one_argument_each` ve `the_command_line_length_counts_what_the_system_counts`, Task 5 iki entegrasyon testi, Task 9 `gui.sh commands`.
3. **Komut kısayolu çakışmaları ve eski ayar dosyaları:** bir eylemin varsayılanı (`mod+t`), `[shortcuts]`'ta yazılmış bir bağlama, dosyada önce gelen komut, büyük harfle yazılmış aynı akor; komut her durumda kalır, yalnız tuşu düşer. 5c'nin `[[commands]]`'ı uyarısız okunur. → Task 4 `a_command_key_taken_by_an_action_or_an_earlier_command_is_left_out` ve var olan `reads_convert_and_commands`.
4. **Türkçe harfler ve açma kenarları:** `ind` → `İndirilenler`, `ILIK`; `~veli`, `/srv/~/x`, `%NOPE%`, `100%`, `%%`, `$1`, `${HOME` (kapanmamış), `$` sonda. → Task 1 ve Task 2 testleri.
5. **Adres çubuğunda tuş sahipliği:** liste açıkken ve kapalıyken Esc, → (imleç), Enter (seçimsiz → metne), Tab, Shift+Tab; satıra tıklamak yazmayı bitirmeden gider. → Task 7 `keys_in_the_address_bar`, Task 9 `gui.sh paths`.

---

### Task 1: `gezik-core::history` — klasör geçmişi ve puan

**Files:**
- Create: `crates/gezik-core/src/history.rs`
- Modify: `crates/gezik-core/src/lib.rs` (`pub mod history;`), `crates/gezik-core/src/pattern.rs` (`fold_text`)

**Interfaces:**
- Consumes: `gezik_core::ops::paths::same_path`; `pattern`'in özel `fold`'u.
- Produces:

```rust
// pattern.rs
pub fn fold_text(text: &str) -> String;     // as the pattern compares: lower case, Turkish i one letter

// history.rs
pub const MAX_VISITS: usize = 200;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit { pub path: PathBuf, pub count: u32, pub last: u64 }
pub fn score(visit: &Visit, now: u64) -> u64;
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderHistory { /* private */ }
impl FolderHistory {
    pub fn from_visits(visits: Vec<Visit>, now: u64) -> FolderHistory;
    pub fn visits(&self) -> &[Visit];
    pub fn is_empty(&self) -> bool;
    pub fn visit(&mut self, path: &Path, now: u64);
    pub fn remove(&mut self, paths: &[PathBuf]) -> bool;
    pub fn clear(&mut self);
    pub fn recent(&self, n: usize) -> Vec<&Visit>;
    pub fn frequent<'a>(&'a self, n: usize, now: u64, skip: &[&Visit]) -> Vec<&'a Visit>;
    pub fn matching(&self, text: &str, n: usize, now: u64, skip: impl Fn(&Path) -> bool) -> Vec<&Visit>;
}
```

- [ ] **Step 0: Dal.** 6a'nın düzeltme dalgası işlenmiş olmalı (`git log feat/keyboard-6a` son commit'leri; çalışma ağacı temiz). `git switch -c feat/keyboard-6b feat/keyboard-6a`.

- [ ] **Step 1: Write the failing tests** (`history.rs`'in sonunda; `pattern.rs` testlerine bir test)

`pattern.rs`:

```rust
#[test]
fn text_folds_as_names_do() {
    assert_eq!(fold_text("İndirilenler"), fold_text("indirilenler"));
    assert_eq!(fold_text("ILIK Şehir"), "ilik şehir");
}
```

`history.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn v(path: &str, count: u32, last: u64) -> Visit {
        Visit { path: p(path), count, last }
    }

    fn paths(list: &[&Visit]) -> Vec<PathBuf> {
        list.iter().map(|visit| visit.path.clone()).collect()
    }

    #[test]
    fn the_score_weighs_how_recent() {
        assert_eq!(score(&v("/a", 3, NOW - 10), NOW), 48);
        assert_eq!(score(&v("/a", 3, NOW - 2 * HOUR), NOW), 24);
        assert_eq!(score(&v("/a", 3, NOW - 2 * DAY), NOW), 12);
        assert_eq!(score(&v("/a", 3, NOW - 30 * DAY), NOW), 3);
        assert_eq!(score(&v("/a", 1, NOW + 50), NOW), 16, "a clock set back counts as now");
    }

    #[test]
    fn a_visit_counts_once_per_folder_and_moves_it_last() {
        let mut h = FolderHistory::default();
        h.visit(&p("/a"), NOW - 100);
        h.visit(&p("/b"), NOW - 50);
        h.visit(&p("/a"), NOW);
        assert_eq!(h.visits(), [v("/b", 1, NOW - 50), v("/a", 2, NOW)]);
        // A trailing separator is the same folder.
        h.visit(&p("/b/"), NOW);
        assert_eq!(h.visits(), [v("/a", 2, NOW), v("/b", 2, NOW)]);
    }

    #[test]
    fn of_two_visits_in_one_second_the_later_is_more_recent() {
        let mut h = FolderHistory::default();
        h.visit(&p("/a"), NOW);
        h.visit(&p("/b"), NOW);
        h.visit(&p("/a"), NOW);
        assert_eq!(paths(&h.recent(2)), [p("/a"), p("/b")]);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn case_makes_no_other_folder_where_the_system_ignores_it() {
        let mut h = FolderHistory::default();
        h.visit(&p("/Users/A/Belgeler"), NOW);
        h.visit(&p("/users/a/BELGELER"), NOW);
        assert_eq!(h.visits().len(), 1);
        assert_eq!(h.visits()[0].count, 2);
    }

    #[test]
    fn past_the_limit_the_lowest_score_goes() {
        let mut h = FolderHistory::default();
        // All older than a week: a score of 1 each.
        for i in 0..MAX_VISITS as u64 {
            h.visit(&p(&format!("/f{i}")), NOW - 2 * WEEK + i);
        }
        h.visit(&p("/f7"), NOW);
        h.visit(&p("/new"), NOW);
        assert_eq!(h.visits().len(), MAX_VISITS);
        assert!(!h.visits().iter().any(|x| x.path == p("/f0")), "the oldest of the lowest went");
        assert!(h.visits().iter().any(|x| x.path == p("/f7")) && h.visits().iter().any(|x| x.path == p("/new")));
    }

    #[test]
    fn recent_and_frequent_lists() {
        let h = FolderHistory::from_visits(
            vec![
                v("/old-often", 50, NOW - 30 * DAY),
                v("/today", 2, NOW - 3 * HOUR),
                v("/now", 1, NOW - 60),
                v("/week", 3, NOW - 3 * DAY),
            ],
            NOW,
        );
        let recent = h.recent(2);
        assert_eq!(paths(&recent), [p("/now"), p("/today")]);
        assert_eq!(paths(&h.frequent(7, NOW, &recent)), [p("/old-often"), p("/week")]);
    }

    #[test]
    fn matching_ignores_case_and_turkish_i() {
        let h = FolderHistory::from_visits(
            vec![v("/home/ali/İndirilenler", 1, NOW), v("/home/ali/Belgeler", 5, NOW), v("/srv/indir", 9, NOW)],
            NOW,
        );
        assert_eq!(paths(&h.matching("indir", 5, NOW, |_| false)), [p("/srv/indir"), p("/home/ali/İndirilenler")]);
        assert_eq!(h.matching("INDIR", 5, NOW, |path| path == Path::new("/srv/indir")).len(), 1);
        assert_eq!(h.matching("belge", 1, NOW, |_| false).len(), 1);
        assert!(h.matching("zzz", 5, NOW, |_| false).is_empty());
    }

    #[test]
    fn saved_visits_are_cleaned_up() {
        let h = FolderHistory::from_visits(vec![v("/a", 2, NOW - 10), v("/b", 0, NOW), v("/a/", 3, NOW)], NOW);
        assert_eq!(h.visits(), [v("/a", 5, NOW)]);
        let many: Vec<Visit> = (0..250u32).map(|i| v(&format!("/m{i}"), 1 + i, NOW)).collect();
        let h = FolderHistory::from_visits(many, NOW);
        assert_eq!(h.visits().len(), MAX_VISITS);
        assert!(h.visits().iter().all(|x| x.count > 50), "the lowest scores went");
    }

    #[test]
    fn forgetting_and_clearing() {
        let mut h = FolderHistory::from_visits(vec![v("/a", 1, NOW), v("/b", 1, NOW)], NOW);
        assert!(h.remove(&[p("/a"), p("/zzz")]));
        assert!(!h.remove(&[p("/zzz")]));
        assert_eq!(h.visits(), [v("/b", 1, NOW)]);
        h.clear();
        assert!(h.is_empty());
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core history` → FAIL (`history` modülü yok).

- [ ] **Step 3: Implement.** `pattern.rs` (`fold`'un altına):

```rust
/// `text` as the pattern compares it: lower case, with i, İ, ı and I one letter (for the
/// address bar, which matches by start and by "holds", not by pattern).
pub fn fold_text(text: &str) -> String {
    text.chars().map(fold).collect()
}
```

`history.rs`:

```rust
//! The folders visited, for the address bar's Recent and Frequent lists (spec 6.2): how often
//! and when each was last visited, scored by "frecency". Pure: the app records the visits and
//! writes them to state.toml.

use std::path::{Path, PathBuf};

use crate::ops::paths::same_path;
use crate::pattern::fold_text;

/// The most folders kept; past it the lowest score goes.
pub const MAX_VISITS: usize = 200;

const HOUR: u64 = 60 * 60;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

/// One folder visited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub path: PathBuf,
    /// How many times (at least 1).
    pub count: u32,
    /// When last, in seconds since 1970.
    pub last: u64,
}

/// A visit's score at `now`: its count times how recent it is, in quarters so that scores
/// are whole numbers (last hour ×4, last day ×2, last week ×1, older ×0.25).
pub fn score(visit: &Visit, now: u64) -> u64 {
    let age = now.saturating_sub(visit.last);
    let quarters = if age < HOUR {
        16
    } else if age < DAY {
        8
    } else if age < WEEK {
        4
    } else {
        1
    };
    u64::from(visit.count) * quarters
}

/// The visits, the most recently visited last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderHistory {
    visits: Vec<Visit>,
}

impl FolderHistory {
    /// From saved visits: one per folder (the first's path, the counts added up, the latest
    /// time), none counted 0, at most `MAX_VISITS` (the lowest scores at `now` go).
    pub fn from_visits(visits: Vec<Visit>, now: u64) -> FolderHistory {
        let mut history = FolderHistory::default();
        for visit in visits.into_iter().filter(|visit| visit.count > 0) {
            match history.visits.iter_mut().find(|kept| same_path(&kept.path, &visit.path)) {
                Some(kept) => {
                    kept.count = kept.count.saturating_add(visit.count);
                    kept.last = kept.last.max(visit.last);
                }
                None => history.visits.push(visit),
            }
        }
        while history.visits.len() > MAX_VISITS {
            history.drop_lowest(now);
        }
        history
    }

    pub fn visits(&self) -> &[Visit] {
        &self.visits
    }

    pub fn is_empty(&self) -> bool {
        self.visits.is_empty()
    }

    /// Counts a visit to `path` at `now` and makes it the latest. A new folder past
    /// `MAX_VISITS` makes the lowest score go (the oldest of equal ones).
    pub fn visit(&mut self, path: &Path, now: u64) {
        let found = self.visits.iter().position(|visit| same_path(&visit.path, path));
        let visit = match found {
            Some(i) => {
                let mut visit = self.visits.remove(i);
                visit.count = visit.count.saturating_add(1);
                visit.last = now;
                visit
            }
            None => {
                if self.visits.len() >= MAX_VISITS {
                    self.drop_lowest(now);
                }
                Visit { path: path.to_path_buf(), count: 1, last: now }
            }
        };
        self.visits.push(visit);
    }

    fn drop_lowest(&mut self, now: u64) {
        let lowest =
            self.visits.iter().enumerate().min_by_key(|(_, visit)| (score(visit, now), visit.last)).map(|(i, _)| i);
        if let Some(i) = lowest {
            self.visits.remove(i);
        }
    }

    /// Forgets `paths`; returns whether any of them was there.
    pub fn remove(&mut self, paths: &[PathBuf]) -> bool {
        let before = self.visits.len();
        self.visits.retain(|visit| !paths.iter().any(|path| same_path(path, &visit.path)));
        self.visits.len() != before
    }

    pub fn clear(&mut self) {
        self.visits.clear();
    }

    /// The `n` visited last, the latest first (of two in the same second, the later).
    pub fn recent(&self, n: usize) -> Vec<&Visit> {
        let mut out: Vec<&Visit> = self.visits.iter().rev().collect();
        out.sort_by(|a, b| b.last.cmp(&a.last));
        out.truncate(n);
        out
    }

    /// The `n` best scored at `now`, but those in `skip`; the best first, the latest of equal ones.
    pub fn frequent<'a>(&'a self, n: usize, now: u64, skip: &[&Visit]) -> Vec<&'a Visit> {
        let rest = self.visits.iter().filter(|visit| !skip.iter().any(|s| same_path(&s.path, &visit.path)));
        best(rest, n, now)
    }

    /// The `n` best scored whose path holds `text` (case and the Turkish i ignored), but those
    /// `skip` says yes to.
    pub fn matching(&self, text: &str, n: usize, now: u64, skip: impl Fn(&Path) -> bool) -> Vec<&Visit> {
        let needle = fold_text(text);
        let found = self
            .visits
            .iter()
            .filter(|visit| !skip(&visit.path) && fold_text(&visit.path.to_string_lossy()).contains(&needle));
        best(found, n, now)
    }
}

/// The `n` best scored of `visits` at `now`, the latest of equal ones first.
fn best<'a>(visits: impl Iterator<Item = &'a Visit>, n: usize, now: u64) -> Vec<&'a Visit> {
    let mut out: Vec<&Visit> = visits.collect();
    out.sort_by(|a, b| score(b, now).cmp(&score(a, now)).then(b.last.cmp(&a.last)));
    out.truncate(n);
    out
}
```

`lib.rs`: `pub mod history;` (`drag`'den sonra, alfabetik).

- [ ] **Step 4: Run.** `cargo test -p gezik-core` → PASS.
- [ ] **Step 5: Commit** "Keep the folders visited, scored by how often and how recently".

---

### Task 2: Yazılan yol — `~` ve ortam değişkeni açma, son parçanın ayrılması, önerilerin sırası

**Files:**
- Create: `crates/gezik-core/src/complete.rs`
- Modify: `crates/gezik-core/src/nav.rs` (`expand_typed`), `crates/gezik-core/src/lib.rs` (`pub mod complete;`)

**Interfaces:**
- Consumes: Task 1 (`pattern::fold_text`).
- Produces:

```rust
// nav.rs
pub fn expand_typed(text: &str, home: &Path, var: impl Fn(&str) -> Option<String>, windows: bool) -> String;
// complete.rs
pub const MAX_SUGGESTIONS: usize = 12;
pub fn split_typed(text: &str, windows: bool) -> (String, String);   // (folder part with its separator, start of a name)
pub fn shows_history(text: &str, windows: bool) -> bool;
pub fn rank(names: &[String], typed: &str) -> Vec<usize>;
pub fn sort_names(names: &mut [String]);
```

- [ ] **Step 1: Write the failing tests**

`nav.rs` testlerine:

```rust
#[test]
fn a_tilde_at_the_start_is_home() {
    let home = Path::new("/home/ali");
    let none = |_: &str| None;
    assert_eq!(expand_typed("~", home, none, false), "/home/ali");
    assert_eq!(expand_typed("~/Projeler", home, none, false), "/home/ali/Projeler");
    assert_eq!(expand_typed("~veli/x", home, none, false), "~veli/x", "another user's ~ stays");
    assert_eq!(expand_typed("/srv/~/x", home, none, false), "/srv/~/x", "only at the start");
    assert_eq!(expand_typed(r"~\Belgeler", Path::new(r"C:\Users\ali"), none, true), r"C:\Users\ali\Belgeler");
    assert_eq!(expand_typed(r"~\x", home, none, false), r"~\x", "a backslash is no separator on Unix");
}

#[test]
fn windows_variables_are_between_percent_signs() {
    let var = |name: &str| match name {
        "USERPROFILE" => Some(r"C:\Users\ali".to_owned()),
        "A" => Some("1".to_owned()),
        _ => None,
    };
    let home = Path::new(r"C:\Users\ali");
    assert_eq!(expand_typed(r"%USERPROFILE%\Desktop", home, var, true), r"C:\Users\ali\Desktop");
    assert_eq!(expand_typed("%A%%A%", home, var, true), "11");
    assert_eq!(expand_typed(r"%NOPE%\x", home, var, true), r"%NOPE%\x");
    assert_eq!(expand_typed("100%", home, var, true), "100%");
    assert_eq!(expand_typed("%%", home, var, true), "%%");
    assert_eq!(expand_typed("$A", home, var, true), "$A", "no $ on Windows");
}

#[test]
fn unix_variables_start_with_a_dollar() {
    let var = |name: &str| match name {
        "HOME" => Some("/home/ali".to_owned()),
        "X_1" => Some("x".to_owned()),
        _ => None,
    };
    let home = Path::new("/home/ali");
    assert_eq!(expand_typed("$HOME/Belgeler", home, var, false), "/home/ali/Belgeler");
    assert_eq!(expand_typed("${HOME}x", home, var, false), "/home/alix");
    assert_eq!(expand_typed("/a/$X_1-b", home, var, false), "/a/x-b");
    assert_eq!(expand_typed("$NOPE/x ${NOPE}", home, var, false), "$NOPE/x ${NOPE}");
    assert_eq!(expand_typed("a$ $1 ${HOME", home, var, false), "a$ $1 ${HOME");
    assert_eq!(expand_typed("%HOME%", home, var, false), "%HOME%", "no % on Unix");
}
```

`complete.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_owned(), b.to_owned())
    }

    #[test]
    fn the_last_part_is_split_off() {
        assert_eq!(split_typed("/home/ali/Pro", false), pair("/home/ali/", "Pro"));
        assert_eq!(split_typed("/home/ali/", false), pair("/home/ali/", ""));
        assert_eq!(split_typed("Pro", false), pair("", "Pro"));
        assert_eq!(split_typed(r"a\b", false), pair("", r"a\b"), "a backslash is part of a name on Unix");
        assert_eq!(split_typed(r"C:\Users\al", true), pair(r"C:\Users\", "al"));
        assert_eq!(split_typed("C:/Users/al", true), pair("C:/Users/", "al"));
        assert_eq!(split_typed("D:", true), pair(r"D:\", ""));
        assert_eq!(split_typed(r"\\server\share\do", true), pair(r"\\server\share\", "do"));
    }

    #[test]
    fn history_shows_for_nothing_home_or_a_root() {
        for text in ["", "  ", "~", "/"] {
            assert!(shows_history(text, false), "{text:?}");
        }
        for text in ["\\", "C:", "c:\\", "D:/"] {
            assert!(shows_history(text, true), "{text:?}");
        }
        for text in ["~/", "/a", "C:", "aş", "ş/"] {
            assert!(!shows_history(text, false), "{text:?}");
        }
        assert!(!shows_history(r"C:\a", true));
        assert!(!shows_history("şx", true), "three bytes, no drive");
    }

    #[test]
    fn names_starting_with_the_text_come_first() {
        let list = names(&["Alpha", "Beta", "alpine", "Kalabalık", ".alcohol", "İndirilenler"]);
        let shown = |typed: &str| rank(&list, typed).into_iter().map(|i| list[i].clone()).collect::<Vec<_>>();
        assert_eq!(shown("al"), ["Alpha", "alpine", "Kalabalık"]);
        assert_eq!(shown("ind"), ["İndirilenler"]);
        assert_eq!(shown(".al"), [".alcohol"]);
        assert_eq!(shown("").len(), 5, "everything but the dot name");
        assert!(shown("zz").is_empty());
    }

    #[test]
    fn at_most_twelve() {
        let list: Vec<String> = (0..30).map(|i| format!("dir{i:02}")).collect();
        assert_eq!(rank(&list, "dir").len(), MAX_SUGGESTIONS);
        assert_eq!(rank(&list, "dir")[0], 0);
    }

    #[test]
    fn names_sort_ignoring_case() {
        let mut list = names(&["beta", "Alpha", "İz", "alpine", "Alpha2"]);
        sort_names(&mut list);
        assert_eq!(list, ["Alpha", "Alpha2", "alpine", "beta", "İz"]);
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core nav complete` → FAIL.

- [ ] **Step 3: Implement.** `nav.rs` (`nearest_existing`'den sonra):

```rust
/// The text typed into the address bar with `~` and environment variables put in (spec 6.1):
/// `~` alone or followed by a separator is `home`; on Windows `%NAME%`, elsewhere `$NAME` and
/// `${NAME}`. A variable `var` does not know stays as typed, and so does `~name`.
pub fn expand_typed(text: &str, home: &Path, var: impl Fn(&str) -> Option<String>, windows: bool) -> String {
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    if let Some(after) = text.strip_prefix('~')
        && (after.is_empty() || after.starts_with(separators))
    {
        out.push_str(&home.to_string_lossy());
        rest = after;
    }
    if windows {
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let found = after.find('%').filter(|end| *end > 0).and_then(|end| Some((end, var(&after[..end])?)));
            match found {
                Some((end, value)) => {
                    out.push_str(&value);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            }
        }
    } else {
        while let Some(start) = rest.find('$') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let (name, used) = match after.strip_prefix('{') {
                Some(braced) => match braced.find('}') {
                    Some(end) => (&braced[..end], end + 2),
                    None => ("", 0),
                },
                None => {
                    let len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
                    (&after[..len], len)
                }
            };
            let valid = !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            match valid.then(|| var(name)).flatten() {
                Some(value) => {
                    out.push_str(&value);
                    rest = &after[used..];
                }
                None => {
                    out.push('$');
                    rest = after;
                }
            }
        }
    }
    out.push_str(rest);
    out
}
```

`complete.rs`:

```rust
//! The address bar's suggestions (spec 6.1): where the last part of a typed path starts, and
//! which sub-folder names fit it, in what order. Pure; the app reads the folder on a thread.

use crate::pattern::fold_text;

/// At most this many folders are suggested.
pub const MAX_SUGGESTIONS: usize = 12;

/// `C:` and the like.
fn is_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// `text` (already expanded) split at its last separator: the folder part, with its
/// separator (empty for a name alone), and the start of a name in it. On Windows `X:` alone
/// is that drive's root.
pub fn split_typed(text: &str, windows: bool) -> (String, String) {
    if windows && is_drive(text) {
        return (format!("{text}\\"), String::new());
    }
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    match text.rfind(separators) {
        Some(i) => (text[..=i].to_owned(), text[i + 1..].to_owned()),
        None => (String::new(), text.to_owned()),
    }
}

/// Whether the typed text asks for the history rather than a folder's names (spec 6.2):
/// nothing, `~` or a root (`/`; on Windows also `\`, `C:`, `C:\`, `C:/`).
pub fn shows_history(text: &str, windows: bool) -> bool {
    let text = text.trim();
    if matches!(text, "" | "~" | "/") {
        return true;
    }
    let bytes = text.as_bytes();
    let drive_root =
        bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    windows && (text == "\\" || is_drive(text) || drive_root)
}

/// Which of `names` (in their order) fit `typed`, best first: those starting with it, then
/// those holding it; case and the Turkish i ignored. A name starting with a dot only when
/// `typed` does. At most `MAX_SUGGESTIONS`.
pub fn rank(names: &[String], typed: &str) -> Vec<usize> {
    let needle = fold_text(typed);
    let dots = typed.starts_with('.');
    let mut starts = Vec::new();
    let mut holds = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if name.starts_with('.') && !dots {
            continue;
        }
        let folded = fold_text(name);
        if folded.starts_with(&needle) {
            starts.push(i);
        } else if folded.contains(&needle) {
            holds.push(i);
        }
    }
    starts.extend(holds);
    starts.truncate(MAX_SUGGESTIONS);
    starts
}

/// Folder names in the order suggestions list them: case and the Turkish i ignored, then as
/// written.
pub fn sort_names(names: &mut [String]) {
    names.sort_by_cached_key(|name| (fold_text(name), name.clone()));
}
```

`lib.rs`: `pub mod complete;` (`batch`'ten sonra).

- [ ] **Step 4: Run.** `cargo test -p gezik-core` → PASS.
- [ ] **Step 5: Commit** "Expand ~ and environment variables in a typed path and rank folder suggestions".

---

### Task 3: `[[commands]]` alanları, `{files}`, komut satırı uzunluğu ve sınırı

**Files:**
- Modify: `crates/gezik-core/src/batch/convert.rs`, `crates/gezik-platform/src/process.rs`; `CommandSpec` kurulumları: `crates/gezik-config/src/settings.rs` (`parse_command` ve `reads_convert_and_commands`), `crates/gezik-batch/src/tasks/command.rs` (test `spec`), `crates/gezik-batch/tests/convert_tasks.rs` (`command`), `crates/gezik/src/convert.rs` (test `spec`)

**Interfaces:**
- Produces:

```rust
// gezik-core::batch::convert
pub struct CommandSpec { /* … */ pub shortcut: Option<String>, pub menu: Option<String>, pub ask: bool }
pub const FILES_ALONE: &str = "{files} can't be used with {in}, {name}, {ext}, {out} or output";
pub fn uses_files(spec: &CommandSpec) -> bool;
pub fn expand_files_command(spec: &CommandSpec, dir: &Path, files: &[PathBuf]) -> Result<Vec<OsString>, String>;
pub fn command_line_len(args: &[OsString], windows: bool) -> usize;
pub fn too_many_text(name: &str) -> String;           // "Too many items for one run of <name>"
// gezik-platform::process
pub fn command_line_limit() -> usize;
```

- [ ] **Step 1: Write the failing tests.** `convert.rs` testlerindeki `command` yardımcısına `shortcut: None, menu: None, ask: false`; sonra:

```rust
#[test]
fn files_go_in_as_one_argument_each() {
    let mut spec = command(&["zip", "-q", "{dir}/all.zip", "{files}", "--", "x{{y}}"], None, &[]);
    assert!(uses_files(&spec) && check_command(&spec).is_ok());
    let dir = root().join("photos");
    let files = [dir.join("a b&c;d.jpg"), dir.join("-rf"), dir.join("ş 'q'.png")];
    let list = expand_files_command(&spec, &dir, &files).unwrap();
    let mut zip = dir.as_os_str().to_os_string();
    zip.push("/all.zip");
    assert_eq!(list.len(), 8);
    assert_eq!(list[2], zip);
    let wanted: Vec<OsString> = files.iter().map(|f| f.as_os_str().to_os_string()).collect();
    assert_eq!(&list[3..6], wanted.as_slice());
    assert_eq!(list[7], "x{y}");
    assert_eq!(expand_files_command(&spec, &dir, &[]).unwrap().len(), 5, "no files: no arguments for them");
    spec.run = vec!["t".into(), "{in}".into()];
    assert!(expand_files_command(&spec, &dir, &files).is_err());
}

#[test]
fn files_stands_alone() {
    let check = |run: &[&str], output: Option<&str>| check_command(&command(run, output, &[]));
    assert_eq!(check(&["t", "--in={files}"], None).unwrap_err(), "{files} must be an argument of its own");
    for run in [&["t", "{files}", "{in}"][..], &["t", "{files}", "{name}.x"], &["t", "{ext}", "{files}"]] {
        assert_eq!(check(run, None).unwrap_err(), FILES_ALONE, "{run:?}");
    }
    assert_eq!(check(&["t", "{files}"], Some("{name}.zip")).unwrap_err(), FILES_ALONE);
    assert_eq!(check(&["t", "{files}", "{out}"], Some("x")).unwrap_err(), FILES_ALONE);
    assert!(check(&["t", "{dir}", "{outdir}", "{files}"], None).is_ok());
    let per_item = command(&["t", "{files}"], None, &[]);
    assert!(expand_command(&per_item, &root().join("a"), false, None).is_err(), "never one item at a time");
    assert!(!uses_files(&command(&["t", "{in}", "{{files}}"], None, &[])), "escaped braces are text");
}

#[test]
fn the_command_line_length_counts_what_the_system_counts() {
    let args: Vec<OsString> = ["ab", "a\"b", "ş"].map(OsString::from).into();
    // Windows: UTF-16 units, room for each quote and backslash to be escaped, two quotes and a space.
    assert_eq!(command_line_len(&args, true), (2 + 3) + (3 + 1 + 3) + (1 + 3));
    // Elsewhere: bytes, the end and a pointer each (ş is two bytes).
    let p = std::mem::size_of::<usize>();
    assert_eq!(command_line_len(&args, false), (2 + 1 + p) + (3 + 1 + p) + (2 + 1 + p));
    assert_eq!(command_line_len(&[OsString::from("😀")], true), 2 + 3, "two UTF-16 units");
    assert_eq!(too_many_text("Zip"), "Too many items for one run of Zip");
}
```

`process.rs` testlerine (yoksa `#[cfg(test)] mod tests` açılır):

```rust
#[test]
fn the_command_line_limit_leaves_room() {
    let limit = command_line_limit();
    if cfg!(windows) {
        assert_eq!(limit, 32_000);
    } else {
        assert!((16 * 1024..=64 * 1024 * 1024).contains(&limit), "{limit}");
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core convert` ve `cargo test -p gezik-platform process` → FAIL (derlenmez).

- [ ] **Step 3: Implement.** `CommandSpec`'e (`parallel`'den sonra):

```rust
    /// The key that runs it on the selection (`shortcut`, written as `[shortcuts]` writes
    /// keys); the shortcut table checks it.
    pub shortcut: Option<String>,
    /// The heading it goes under in "Commands ▸" (`menu`).
    pub menu: Option<String>,
    /// Asks "Run … on N items?" before it runs (`ask`).
    pub ask: bool,
```

`PLACEHOLDERS`'a `"files"` (`[&str; 7]`). `check_command`'ın argüman döngüsü:

```rust
pub const FILES_ALONE: &str = "{files} can't be used with {in}, {name}, {ext}, {out} or output";

    let files = uses_files(spec);
    for arg in &spec.run {
        let pieces = pieces(arg)?;
        if pieces.contains(&Err("files")) && pieces.len() != 1 {
            return Err("{files} must be an argument of its own".to_owned());
        }
        for piece in &pieces {
            if files && matches!(piece, Err("in" | "name" | "ext" | "out")) {
                return Err(FILES_ALONE.to_owned());
            }
            if *piece == Err("out") && spec.output.is_none() {
                return Err("{out} needs an output".to_owned());
            }
        }
    }
    if files && spec.output.is_some() {
        return Err(FILES_ALONE.to_owned());
    }
```

`expand_command`'ın `match piece`'ine `Err("files") => return Err("{files} runs once on all the items".to_owned()),`. Yeniler (`command_applies`'ın üstüne):

```rust
/// Whether `spec` runs once on the whole selection (`{files}` in `run`).
pub fn uses_files(spec: &CommandSpec) -> bool {
    spec.run.iter().any(|arg| pieces(arg).is_ok_and(|pieces| pieces.contains(&Err("files"))))
}

/// The program and arguments for running `spec` (a `{files}` command) once on `files`
/// (absolute) in their folder `dir`: `{files}` becomes one argument per file, `{dir}` and
/// `{outdir}` are `dir`.
pub fn expand_files_command(spec: &CommandSpec, dir: &Path, files: &[PathBuf]) -> Result<Vec<OsString>, String> {
    if spec.run.is_empty() {
        return Err("run has no program".to_owned());
    }
    let mut out = Vec::with_capacity(spec.run.len() + files.len());
    for arg in &spec.run {
        let pieces = pieces(arg)?;
        if pieces == [Err("files")] {
            out.extend(files.iter().map(|file| file.as_os_str().to_os_string()));
            continue;
        }
        let mut expanded = OsString::new();
        for piece in pieces {
            match piece {
                Ok(text) => expanded.push(text),
                Err("dir" | "outdir") => expanded.push(dir),
                Err(field) => return Err(format!("{{{field}}} can't be used with {{files}}")),
            }
        }
        out.push(expanded);
    }
    Ok(out)
}

/// How long `args` are as one command line. On Windows in UTF-16 units as `CreateProcess`
/// gets them: each argument quoted, with room for every quote and backslash in it to be
/// escaped, one space between. Elsewhere in bytes as `ARG_MAX` counts them: each argument,
/// its end and its pointer.
pub fn command_line_len(args: &[OsString], windows: bool) -> usize {
    args.iter()
        .map(|arg| {
            if windows {
                let text = arg.to_string_lossy();
                let escapes = text.chars().filter(|c| matches!(c, '"' | '\\')).count();
                text.encode_utf16().count() + escapes + 3
            } else {
                arg.len() + 1 + std::mem::size_of::<usize>()
            }
        })
        .sum()
}

/// The error of a `{files}` run whose items do not fit one command line (spec 7).
pub fn too_many_text(name: &str) -> String {
    format!("Too many items for one run of {name}")
}
```

`process.rs` (`process_alive`'ların altına):

```rust
/// The longest command line Gezik starts a program with, kept short of the system's limit:
/// on Windows 32,000 characters (of 32,767); elsewhere half of `ARG_MAX` (the other half is
/// the environment's), or 64 KiB if the system does not say.
pub fn command_line_limit() -> usize {
    #[cfg(windows)]
    {
        32_000
    }
    #[cfg(unix)]
    {
        // SAFETY: sysconf only reads a value of the system.
        let max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        usize::try_from(max).ok().filter(|max| *max > 0).map_or(64 * 1024, |max| max / 2)
    }
}
```

Öteki `CommandSpec { … }` kurulumlarının hepsine `shortcut: None, menu: None, ask: false` (yukarıdaki dosya listesi; `parse_command`'da da şimdilik bu değerler, Task 4 okur).

- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler (Linux'ta `libc::sysconf`).
- [ ] **Step 5: Commit** "Add {files} to user commands, with the command line's length and limit".

---

### Task 4: `gezik-config` — `[history]`, durumda geçmiş, komut alanları, komut kısayolları, `clear-history`, şablon

**Files:**
- Modify: `crates/gezik-config/src/settings.rs`, `crates/gezik-config/src/shortcuts.rs`, `crates/gezik-config/templates/settings.toml`; derlemenin istediği kollar: `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs` (`handle_key`'in eylem listesi), `crates/gezik/src/keys.rs` (test `every_default_is_reachable_on_windows_and_linux`)

**Interfaces:**
- Consumes: Task 1 (`Visit`), Task 3 (`CommandSpec` alanları, `FILES_ALONE`).
- Produces:

```rust
// settings.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistorySettings { pub remember: bool }      // Default: remember = true
// Settings: pub history: HistorySettings
// State:    pub history: Vec<gezik_core::history::Visit>
// shortcuts.rs
pub enum Action { …, ClearHistory }                    // "clear-history", no default key; ALL: [Action; 46]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOwner { Action(Action), Command(usize) }
impl Shortcuts {
    pub fn bind_command(&mut self, index: usize, chord: Chord) -> Result<(), KeyOwner>;
    pub fn command_for(&self, chord: &Chord) -> Option<usize>;    // index among the valid [[commands]]
    pub fn command_chord(&self, index: usize) -> Option<Chord>;
}
```

**Behavior:**
- `parse_command`: `KEYS` dokuz anahtar (`"name", "run", "output", "types", "folders", "parallel", "shortcut", "menu", "ask"`); `shortcut` metin değilse `shortcut must be text, got …`; `menu` metin değilse `menu must be text, got …`, kırpılır, boşsa `None`; `ask` bool değilse `ask must be true or false, got …`.
- `Settings::parse`'ın `[[commands]]` döngüsü geçerli komutu eklemeden önce `bind_command_key` çağırır:

```rust
/// Gives `spec` (the `n`th `[[commands]]` entry, about to join `settings.commands`) its
/// shortcut, or warns why it has none (spec 7: the command stays, its key is left out).
fn bind_command_key(settings: &mut Settings, spec: &CommandSpec, n: usize, file: &str, warnings: &mut Vec<Warning>) {
    let Some(text) = &spec.shortcut else { return };
    let index = settings.commands.len();
    let chord = match parse_chord(text, Platform::current()) {
        Ok(Some(chord)) => chord,
        Ok(None) => return,
        Err(err) => {
            warnings.push(Warning::new(file, format!("commands[{n}]: shortcut: {err}; the command has no key")));
            return;
        }
    };
    if let Err(owner) = settings.shortcuts.bind_command(index, chord) {
        let who = match owner {
            KeyOwner::Action(action) => action.name().to_owned(),
            KeyOwner::Command(other) => format!("\"{}\"", settings.commands[other].name),
        };
        warnings.push(Warning::new(
            file,
            format!("commands[{n}]: shortcut \"{text}\" is already used by {who}; the command has no key"),
        ));
    }
}
```
- `Shortcuts`'a `commands: Vec<(Chord, usize)>` (`from_table` boş kurar); `#[derive(PartialEq, Eq)]` aynen.

```rust
/// What a key is bound to already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOwner {
    Action(Action),
    /// A `[[commands]]` entry, by its index among the valid ones.
    Command(usize),
}

    /// Binds `chord` to command `index` (its place among the valid `[[commands]]`), unless an
    /// action (default or written) or an earlier command has it: then nothing changes and
    /// the owner is returned.
    pub fn bind_command(&mut self, index: usize, chord: Chord) -> Result<(), KeyOwner> {
        if let Some(action) = self.action_for(&chord) {
            return Err(KeyOwner::Action(action));
        }
        if let Some(other) = self.command_for(&chord) {
            return Err(KeyOwner::Command(other));
        }
        self.commands.push((chord, index));
        Ok(())
    }

    /// The command `chord` runs, by its index among the valid `[[commands]]`.
    pub fn command_for(&self, chord: &Chord) -> Option<usize> {
        self.commands.iter().find(|(c, _)| c == chord).map(|(_, i)| *i)
    }

    /// The key of command `index`, if it has one.
    pub fn command_chord(&self, index: usize) -> Option<Chord> {
        self.commands.iter().find(|(_, i)| *i == index).map(|(c, _)| *c)
    }
```
- `Action::ClearHistory`: adı `clear-history`, `default_texts` `&[]`, `ALL`'ın sonunda (46).
- `[history]`: `remember` (bool); hatalı değer `history.remember: expected true or false, got …`, tablo değilse `history: expected a table, got …`. `Settings::default().history.remember == true`.
- `State.history`: `[history] folders = [{ path, count, last }]`; okurken `path` boş olmayan metin, `count` yoksa 1, 0 ya da negatifse kayıt atlanır, `last` yoksa ya da negatifse 0; boşsa yazılmaz.

```rust
fn parse_visit(value: &toml::Value) -> Option<Visit> {
    let table = value.as_table()?;
    let path = table.get("path")?.as_str().filter(|path| !path.is_empty())?;
    let count = match table.get("count") {
        None => 1,
        Some(count) => u32::try_from(count.as_integer()?).ok().filter(|n| *n > 0)?,
    };
    let last = table.get("last").and_then(|v| v.as_integer()).and_then(|n| u64::try_from(n).ok()).unwrap_or(0);
    Some(Visit { path: PathBuf::from(path), count, last })
}
```
  `to_toml`'da (`selection`'dan sonra):

```rust
        if !self.history.is_empty() {
            let folders = self
                .history
                .iter()
                .map(|visit| {
                    let mut table = toml::Table::new();
                    table.insert("path".into(), toml::Value::String(visit.path.to_string_lossy().into_owned()));
                    table.insert("count".into(), toml::Value::Integer(visit.count.into()));
                    table.insert("last".into(), toml::Value::Integer(i64::try_from(visit.last).unwrap_or(i64::MAX)));
                    toml::Value::Table(table)
                })
                .collect();
            let mut history = toml::Table::new();
            history.insert("folders".into(), toml::Value::Array(folders));
            root.insert("history".into(), toml::Value::Table(history));
        }
```
- Şablon: `[keyboard]`'dan sonra

```toml
[history]
# Remember the folders you open, for the address bar's Recent and Frequent lists (kept in
# state.toml). false also forgets them; "clear-history" (no key by default) empties the list.
remember = true
```
  `[shortcuts]` yorumlarının sonuna `# clear-history = ""     # forget the folders the address bar remembers`. `[[commands]]` açıklamasının sonuna (örneklerden önce):

```toml
# {files} puts in every selected item (full paths, one argument each) and runs the command
# once, in their folder; it can't be used with {in}, {name}, {ext}, {out} or "output", and
# what it does can't be undone. shortcut = "ctrl+alt+z" runs a command on the selection (the
# keys of [shortcuts]; a key already taken is left out, with a warning); menu = "Archives"
# lists it under that heading in "Commands >"; ask = true asks "Run … on N items?" first.
```
  ve son örnekten sonra:

```toml
#
# [[commands]]
# name = "Zip together (7-Zip)"
# run = ["7z", "a", "-tzip", "together.zip", "{files}"]
# folders = true
# shortcut = "ctrl+alt+z"
# menu = "Archives"
# ask = true
```
- `gezik` crate'inde derleme için: `actions.rs`'te "Not 6a's" listesine `| Action::ClearHistory` (Task 8 doldurur), `main.rs` `handle_key`'in 6a eylem listesine `| Action::ClearHistory`, `keys.rs`'te `every_default_is_reachable_on_windows_and_linux`'un `continue` koluna `| Action::ToggleTabLock | Action::ClearHistory`.

- [ ] **Step 1: Write the failing tests** (`settings.rs`)

```rust
#[test]
fn commands_read_their_shortcut_group_and_question() {
    let (settings, warnings) = parse(
        "[[commands]]\nname = \"Zip\"\nrun = [\"7z\", \"a\", \"x.zip\", \"{files}\"]\nshortcut = \"ctrl+alt+z\"\n\
         menu = \" Archives \"\nask = true\n\
         [[commands]]\nname = \"Plain\"\nrun = [\"x\", \"{in}\"]\nmenu = \"\"\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let zip = &settings.commands[0];
    assert_eq!((zip.shortcut.as_deref(), zip.menu.as_deref(), zip.ask), (Some("ctrl+alt+z"), Some("Archives"), true));
    let plain = &settings.commands[1];
    assert_eq!((plain.shortcut.as_deref(), plain.menu.as_deref(), plain.ask), (None, None, false));
    let chord = parse_chord("ctrl+alt+z", Platform::current()).unwrap().unwrap();
    assert_eq!(settings.shortcuts.command_for(&chord), Some(0));
    assert_eq!(settings.shortcuts.command_chord(0), Some(chord));
    assert_eq!(settings.shortcuts.command_chord(1), None);
}

#[test]
fn a_command_key_taken_by_an_action_or_an_earlier_command_is_left_out() {
    let (settings, warnings) = parse(
        "[shortcuts]\nrefresh = \"ctrl+alt+r\"\n\n\
         [[commands]]\nname = \"A\"\nrun = [\"x\"]\nshortcut = \"mod+t\"\n\
         [[commands]]\nname = \"B\"\nrun = [\"x\"]\nshortcut = \"ctrl+alt+r\"\n\
         [[commands]]\nname = \"C\"\nrun = [\"x\"]\nshortcut = \"ctrl+alt+k\"\n\
         [[commands]]\nname = \"D\"\nrun = [\"x\"]\nshortcut = \"CTRL+ALT+K\"\n\
         [[commands]]\nname = \"E\"\nrun = [\"x\"]\nshortcut = \"ctrl+q+\"\n",
    );
    assert_eq!(settings.commands.len(), 5, "every command stays, only its key goes");
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "commands[1]: shortcut \"mod+t\" is already used by new-tab; the command has no key",
            "commands[2]: shortcut \"ctrl+alt+r\" is already used by refresh; the command has no key",
            "commands[4]: shortcut \"CTRL+ALT+K\" is already used by \"C\"; the command has no key",
            "commands[5]: shortcut: missing key after \"+\"; the command has no key",
        ]
    );
    let k = parse_chord("ctrl+alt+k", Platform::current()).unwrap().unwrap();
    assert_eq!(settings.shortcuts.command_for(&k), Some(2));
    let t = parse_chord("mod+t", Platform::current()).unwrap().unwrap();
    assert_eq!(settings.shortcuts.action_for(&t), Some(crate::shortcuts::Action::NewTab), "the action keeps its key");
}

#[test]
fn bad_command_extras_leave_the_command_out() {
    for (body, expected) in [
        ("shortcut = 3", "shortcut must be text"),
        ("menu = true", "menu must be text"),
        ("ask = \"yes\"", "ask must be true or false"),
        ("asks = true", "unknown key \"asks\""),
    ] {
        let (settings, warnings) = parse(&format!("[[commands]]\nname = \"A\"\nrun = [\"x\"]\n{body}\n"));
        assert!(settings.commands.is_empty(), "{body}");
        assert!(warnings[0].message.contains(expected), "{body}: {warnings:?}");
    }
    let (settings, warnings) = parse("[[commands]]\nname = \"A\"\nrun = [\"x\", \"{files}\", \"{in}\"]\n");
    assert!(settings.commands.is_empty());
    assert!(warnings[0].message.contains(gezik_core::batch::convert::FILES_ALONE), "{warnings:?}");
}

#[test]
fn history_settings_are_read() {
    assert!(Settings::default().history.remember);
    let (settings, warnings) = parse("[history]\nremember = false\n");
    assert!(warnings.is_empty() && !settings.history.remember, "{warnings:?}");
    let (settings, warnings) = parse("[history]\nremember = \"no\"\n");
    assert!(settings.history.remember);
    assert!(warnings[0].message.starts_with("history.remember: expected true or false"), "{warnings:?}");
    let (_, warnings) = parse("history = 1\n");
    assert!(warnings[0].message.starts_with("history: expected a table"), "{warnings:?}");
}

#[test]
fn folder_history_round_trips_in_state() {
    use gezik_core::history::Visit;
    let state = State {
        history: vec![
            Visit { path: "D:/Work".into(), count: 3, last: 1_800_000_000 },
            Visit { path: "/srv/ş x".into(), count: 1, last: 0 },
        ],
        ..State::default()
    };
    assert_eq!(State::parse(&state.to_toml()), state);
    assert!(!State::default().to_toml().contains("history"));
    let broken = State::parse(
        "[history]\nfolders = [{ path = \"/a\", count = 0 }, { count = 2 }, { path = \"\" }, \
         { path = \"/b\", count = 2, last = -5 }, { path = \"/c\" }]\n",
    );
    assert_eq!(
        broken.history,
        [Visit { path: "/b".into(), count: 2, last: 0 }, Visit { path: "/c".into(), count: 1, last: 0 }]
    );
}
```

  `the_template_reads_with_the_defaults`'a `assert_eq!(settings.history, HistorySettings::default());`; `the_template_command_examples_read_without_warnings_once_uncommented`'ın ad listesine `"Zip together (7-Zip)"`; `reads_convert_and_commands`'ın beklenen `CommandSpec`'i Task 3'teki gibi. `shortcuts.rs`:

```rust
#[test]
fn clear_history_has_a_name_and_no_key() {
    assert_eq!(Action::from_name("clear-history"), Some(Action::ClearHistory));
    assert_eq!(Shortcuts::defaults(Platform::Other).chord_for(Action::ClearHistory), None);
    let (s, warnings) = build("[shortcuts]\nclear-history = \"ctrl+shift+h\"\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(s.action_for(&chord("ctrl+shift+h")), Some(Action::ClearHistory));
}

#[test]
fn a_command_key_never_takes_an_action_key() {
    let mut s = Shortcuts::defaults(Platform::Other);
    assert_eq!(s.bind_command(0, chord("ctrl+f")), Err(KeyOwner::Action(Action::Filter)));
    assert_eq!(s.bind_command(0, chord("ctrl+alt+x")), Ok(()));
    assert_eq!(s.bind_command(1, chord("ctrl+alt+x")), Err(KeyOwner::Command(0)));
    assert_eq!((s.command_for(&chord("ctrl+alt+x")), s.action_for(&chord("ctrl+alt+x"))), (Some(0), None));
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-config` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki türler ve davranış). `the_template_shortcut_examples_are_the_defaults_off_macos` ve öteki şablon testleri değişmeden geçmeli.
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS (`gezik` crate'i yeni eylem koluyla derlenir).
- [ ] **Step 5: Commit** "Read [history], the folders visited, and the shortcut, menu and ask of user commands".

---

### Task 5: `CommandTask` — `{files}` komutunu bir kez çalıştırmak

**Files:**
- Modify: `crates/gezik-batch/src/tasks/command.rs`, `crates/gezik-batch/tests/convert_tasks.rs`

**Interfaces:**
- Consumes: Task 3 (`uses_files`, `expand_files_command`, `command_line_len`, `too_many_text`, `command_line_limit`).
- Produces: `CommandTask::new` aynı; `{files}` komutu için plan uymayanları `LEFT_OUT`, uyanların hepsini tek `ALL` öğesi olarak verir. `run` bu öğe için programı seçimin klasöründe bir kez çalıştırır ve `Outcome::Nothing` döner.

**Behavior:**

```rust
/// The plan item's note: … and the one run of a `{files}` command on all the items it takes.
const ALL: u8 = 2;

impl CommandTask {
    /// The items the command takes, in their order.
    fn taken(&self) -> Vec<PathBuf> {
        self.inputs
            .iter()
            .filter(|(input, is_dir)| command_applies(&self.spec, &file_name(input), *is_dir))
            .map(|(input, _)| input.clone())
            .collect()
    }

    /// A `{files}` command's plan: the items it does not take are left out one by one, the
    /// others go into one run in their folder; refused at once when they do not fit one
    /// command line.
    fn plan_once(&self, sink: &mut dyn ScanSink) {
        let Some((first, _)) = self.inputs.first() else { return };
        let dir = first.parent().unwrap_or(Path::new("")).to_path_buf();
        for (root, (input, is_dir)) in self.inputs.iter().enumerate() {
            if !command_applies(&self.spec, &file_name(input), *is_dir) {
                let facts = Facts { is_dir: *is_dir, size: 0, modified: None };
                if !sink.item(PlanItem::new(Stage::Parallel, facts).source(input).top(root).tag(LEFT_OUT)) {
                    return;
                }
            }
        }
        let files = self.taken();
        if files.is_empty() {
            return;
        }
        match expand_files_command(&self.spec, &dir, &files) {
            Err(message) => sink.failed(first, invalid(message)),
            Ok(args) if command_line_len(&args, cfg!(windows)) > gezik_platform::process::command_line_limit() => {
                sink.failed(first, invalid(too_many_text(&self.spec.name)));
            }
            Ok(_) => {
                let facts = Facts { is_dir: true, size: 0, modified: None };
                let _ = sink.item(PlanItem::new(Stage::Parallel, facts).source(&dir).top(0).tag(ALL));
            }
        }
    }

    /// The one run of a `{files}` command, in `dir`. Pause does not stop it (running it again
    /// would repeat what it did); Cancel does. What it did is not known: nothing to undo.
    fn run_once(&self, dir: &Path, run: &RunCx<'_>) -> io::Result<Outcome> {
        let args = expand_files_command(&self.spec, dir, &self.taken()).map_err(invalid)?;
        self.execute(Some(dir), &args, run, false)?;
        Ok(Outcome::Nothing)
    }
}
```
- `execute`'un imzası `fn execute(&self, cwd: Option<&Path>, args: &[OsString], run: &RunCx<'_>, stop_on_pause: bool) -> io::Result<()>`. Gövdede `dir`, `cwd.filter(|dir| !dir.as_os_str().is_empty())` olur; `wait_or_stop` `run.cancelled() || (stop_on_pause && run.paused())`. Var olan çağrılar `execute(input.parent(), &args, run, true)`.
- `plan`'ın başında, `check_command`'dan sonra: `if uses_files(&self.spec) { return self.plan_once(sink); }`. `run`'da `LEFT_OUT`'tan sonra: `if item.tag == ALL { return self.run_once(input, run); }` (öğenin `source`'u klasör).
- Modül belgesine bir paragraf: "`{files}`: one run on every item it takes, in their folder; not paused, nothing to undo."

- [ ] **Step 1: Write the failing tests** (`tests/convert_tasks.rs`; `command` yardımcısı Task 3'te yeni alanları aldı)

```rust
#[test]
fn a_files_command_runs_once_on_every_item_in_their_folder_and_leaves_nothing_to_undo() {
    let d = dir("command-files");
    let names = ["a b&c;d.txt", "-x.txt", "ş 'q'.txt"];
    let inputs: Vec<(PathBuf, bool)> = names
        .iter()
        .map(|name| {
            let path = d.join(name);
            std::fs::write(&path, b"x").unwrap();
            (path, false)
        })
        .collect();
    std::fs::create_dir(d.join("sub")).unwrap();
    let mut all = inputs.clone();
    all.push((d.join("sub"), true));
    let engine = engine(&d);
    let report = run(&engine, CommandTask::new(all, command(&["args", "list.log", "{files}", "{dir}"], None)));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.skipped.len(), 1, "the folder is not for it");
    let text = std::fs::read_to_string(d.join("list.log")).unwrap();
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, (path, _)) in inputs.iter().enumerate() {
        assert_eq!(lines[i], path.to_string_lossy(), "one argument each");
    }
    assert_eq!(lines[3], d.to_string_lossy());
    assert_eq!(std::fs::canonicalize(lines[4]).unwrap(), std::fs::canonicalize(&d).unwrap(), "it ran in their folder");
    assert_eq!(engine.undo_label(), None, "nothing to undo");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_files_command_past_the_command_line_limit_does_not_run() {
    let d = dir("command-files-long");
    let long = "x".repeat(200);
    let count = gezik_platform::process::command_line_limit() / 200 + 10;
    // The plan of a {files} run reads no item: these need not exist.
    let inputs: Vec<(PathBuf, bool)> = (0..count).map(|i| (d.join(format!("{long}{i}.txt")), false)).collect();
    let engine = engine(&d);
    let report = run(&engine, CommandTask::new(inputs, command(&["args", "list.log", "{files}"], None)));
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(report.failures[0].message.contains("Too many items for one run of Fake"), "{:?}", report.failures);
    assert!(!d.join("list.log").exists());
    let _ = std::fs::remove_dir_all(&d);
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-batch --test convert_tasks files` → FAIL (komut her öğe için ayrı çalışır ya da "never one item at a time" der).
- [ ] **Step 3: Implement** (yukarıdaki kod). `title_and_workers` ve öteki komut testleri değişmeden geçmeli.
- [ ] **Step 4: Run.** `cargo test -p gezik-batch` → PASS; `cargo check -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- [ ] **Step 5: Commit** "Run a {files} command once in the selection's folder, with nothing to undo".

---

### Task 6: Arayüz — komutlar: kısayolla çalıştırma, `ask`, gruplu "Commands ▸", "can't be undone", macOS Commands menüsü

**Files:**
- Modify: `crates/gezik/src/convert.rs`, `crates/gezik/src/context_menu.rs` (`COMMAND_GROUP`, id testi), `crates/gezik/src/operations.rs` (not), `crates/gezik/src/keys.rs`, `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs` (`handle_key`, `apply_config`), `crates/gezik/src/menu_bar.rs`, `crates/gezik/ui/app.slint` (MenuBar)

**Interfaces:**
- Consumes: Task 3 (`uses_files`, `expand_files_command`, `command_line_len`, `too_many_text`, `command_line_limit`), Task 4 (`Shortcuts::{command_for, command_chord}`, `CommandSpec.{menu, ask}`).
- Produces:

```rust
// context_menu.rs
/// 970: a heading inside "Commands ▸" (`menu = "…"`), greyed, never chosen.
pub const COMMAND_GROUP: u32 = 970;
// convert.rs
pub fn not_for_text(spec: &CommandSpec) -> String;
pub fn ask_text(name: &str, count: usize) -> String;
pub fn bar_entries(commands: &[CommandSpec], key_of: impl Fn(usize) -> Option<String>) -> Vec<(i32, String, bool)>;
pub fn run_by_index(index: usize, items: Vec<(PathBuf, bool)>);   // a key or the macOS menu bar
// operations.rs
pub const CANT_UNDO: &str = "can't be undone";
pub fn with_note(detail: String, note: Option<&str>) -> String;
impl Operations { pub fn set_note(&self, id: JobId, note: &'static str); }
// keys.rs
pub fn command_for(chord: &Chord) -> Option<usize>;
pub fn command_chord(index: usize) -> Option<Chord>;
pub fn chord_label(chord: &Chord, platform: Platform) -> String;
// actions.rs
pub fn run_command(index: usize, view: &View);
// menu_bar.rs (macOS)
pub fn set_commands(window: &AppWindow, commands: &[CommandSpec]);
// app.slint
in property <[MenuEntry]> bar-commands;
```

**Behavior:**
- `command_state`: `{files}` komutu klasörü yerinde değiştirmez: `if spec.output.is_none() && !uses_files(spec) && taken.iter().any(|(_, is_dir)| *is_dir)`.
- Gruplama (`menu_entries` ve `bar_entries` paylaşır):

```rust
/// A line of a commands menu: a group's heading or a command (by its index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line<'a> {
    Heading(&'a str),
    Command(usize),
}

/// The commands `shown` (indices into `commands`) in menu order: those without a `menu`
/// first, then each group under its heading, groups in the order they first appear.
fn grouped<'a>(commands: &'a [CommandSpec], shown: &[usize]) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> = shown.iter().filter(|i| commands[**i].menu.is_none()).map(|i| Line::Command(*i)).collect();
    let mut groups: Vec<&str> = Vec::new();
    for i in shown {
        if let Some(group) = commands[*i].menu.as_deref()
            && !groups.contains(&group)
        {
            groups.push(group);
        }
    }
    for group in groups {
        lines.push(Line::Heading(group));
        lines.extend(
            shown.iter().filter(|i| commands[**i].menu.as_deref() == Some(group)).map(|i| Line::Command(*i)),
        );
    }
    lines
}
```
  `menu_entries`'in `sub`'ı:

```rust
    let shown: Vec<usize> = (0..commands.len().min(states.len()))
        .take(COMMAND_MAX as usize)
        .filter(|i| states[*i] != CommandState::Hidden)
        .collect();
    let sub: Vec<(u32, String, bool)> = grouped(commands, &shown)
        .into_iter()
        .map(|line| match line {
            Line::Heading(group) => (COMMAND_GROUP, group.to_owned(), false),
            Line::Command(i) => {
                let (title, enabled) = command_title(&commands[i], &states[i]);
                (COMMAND_FIRST + i as u32, title, enabled)
            }
        })
        .collect();
```
  (`runnable` başlıkları zaten gri olduğu için değişmez.)
- Metinler:

```rust
/// Why `spec` runs on none of the items it was asked to (spec 7): by its endings, else
/// because they are folders.
pub fn not_for_text(spec: &CommandSpec) -> String {
    let endings: Vec<String> = spec.types.iter().map(|t| format!(".{}", t.to_lowercase())).collect();
    let list = match endings.as_slice() {
        [] => return format!("{} does not run on folders", spec.name),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let what = if spec.folders { "files and folders" } else { "files" };
    format!("{} runs on {list} {what} only", spec.name)
}

/// The question of a command with `ask = true`.
pub fn ask_text(name: &str, count: usize) -> String {
    if count == 1 {
        format!("Run {name} on 1 item?")
    } else {
        format!("Run {name} on {} items?", crate::preview::with_commas(count))
    }
}

/// The macOS menu bar's Commands menu: the commands with a key (spec 8.3), grouped as in
/// "Commands ▸", each "<name>    <key>" by its index; headings greyed with id -1.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn bar_entries(commands: &[CommandSpec], key_of: impl Fn(usize) -> Option<String>) -> Vec<(i32, String, bool)> {
    let shown: Vec<usize> = (0..commands.len()).filter(|i| key_of(*i).is_some()).collect();
    grouped(commands, &shown)
        .into_iter()
        .map(|line| match line {
            Line::Heading(group) => (-1, group.to_owned(), false),
            Line::Command(i) => {
                let key = key_of(i).unwrap_or_default();
                (i32::try_from(i).unwrap_or(-1), format!("{}    {key}", commands[i].name), true)
            }
        })
        .collect()
}
```
- Çalıştırma yolları:

```rust
/// A command run by its key or the macOS menu bar (`index` in `[[commands]]`) on `items`.
pub fn run_by_index(index: usize, items: Vec<(PathBuf, bool)>) {
    let Some(spec) = commands().get(index).cloned() else { return };
    with_current(|convert| convert.run_checked(spec, items));
}

/// A command chosen from "Commands ▸" (by its index in the menu's list), run on `rows`.
pub fn run_menu_command(index: usize, rows: Vec<(PathBuf, bool)>) {
    let Some(spec) = MENU_COMMANDS.with(|m| m.borrow().get(index).cloned()) else { return };
    with_current(|convert| convert.run_asked(spec, rows));
}

/// Whether `items` fit one command line of `spec` (a `{files}` command) here.
fn fits_one_run(spec: &CommandSpec, items: &[(PathBuf, bool)]) -> bool {
    let Some((first, _)) = items.first() else { return true };
    let dir = first.parent().unwrap_or(Path::new(""));
    let files: Vec<PathBuf> = items.iter().map(|(path, _)| path.clone()).collect();
    expand_files_command(spec, dir, &files)
        .is_ok_and(|args| command_line_len(&args, cfg!(windows)) <= gezik_platform::process::command_line_limit())
}

impl Convert {
    fn note(&self, text: String) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_status(text.into());
        }
    }

    /// A command asked for by its key: on the items it takes; when it takes none or cannot
    /// run, the status bar says why (spec 7).
    fn run_checked(&self, spec: CommandSpec, items: Vec<(PathBuf, bool)>) {
        if items.is_empty() {
            return self.note(format!("Select the items to run {} on", spec.name));
        }
        match command_state(&spec, &items, found_program(&program_of(&spec)), cfg!(windows)) {
            CommandState::Hidden => self.note(not_for_text(&spec)),
            CommandState::Off(tip) => self.note(format!("{} — {tip}", spec.name)),
            CommandState::Ready => self.run_asked(spec, items),
        }
    }

    /// Runs `spec` on the items it takes, asking first when it says so (`ask`). A `{files}`
    /// run that does not fit one command line is refused before the question.
    fn run_asked(&self, spec: CommandSpec, items: Vec<(PathBuf, bool)>) {
        let taken: Vec<(PathBuf, bool)> =
            items.into_iter().filter(|(path, is_dir)| command_applies(&spec, &name_of(path), *is_dir)).collect();
        if taken.is_empty() {
            return;
        }
        if uses_files(&spec) && !fits_one_run(&spec, &taken) {
            return self.note(too_many_text(&spec.name));
        }
        if !spec.ask {
            return self.run_command(spec, taken);
        }
        let this = self.clone();
        let message = ask_text(&spec.name, taken.len());
        self.0.dialogs.ask(spec.name.clone(), message, &["Run", "Cancel"], move |answer| {
            if answer == Some(0) {
                this.run_command(spec, taken);
            }
        });
    }
}
```
  `run_command`'ın sonu:

```rust
        self.0.ops.remember_for(&paths);
        let once = uses_files(&spec);
        let task: Box<dyn gezik_ops::Task> = Box::new(CommandTask::new(items, spec));
        if once {
            // What it did is not known: nothing to undo (spec 7), and the panel says so.
            let id = self.0.ops.submit_chain(vec![task], None, Some(again), After::Nothing);
            self.0.ops.set_note(id, CANT_UNDO);
        } else {
            self.0.ops.submit_chain(vec![task], Some(label), Some(again), After::Select);
        }
```
  Convert katmanının `Ready::Command` yolu `run_command`'ı doğrudan çağırmaya devam eder (sapma 5).
- `operations.rs`: `JobView`'a `note: Option<&'static str>` (`new`'de `None`); `row()`'da `detail: with_note(detail, self.note).into()`.

```rust
/// The note of a `{files}` run in the panel (spec 7).
pub const CANT_UNDO: &str = "can't be undone";

/// A row's detail with the job's note after it: "Done · can't be undone".
pub fn with_note(detail: String, note: Option<&str>) -> String {
    match note {
        Some(note) => format!("{detail} · {note}"),
        None => detail,
    }
}

    /// Puts `note` after job `id`'s detail in the panel.
    pub fn set_note(&self, id: JobId, note: &'static str) {
        if let Some(job) = self.0.jobs.borrow_mut().iter_mut().find(|job| job.id == id) {
            job.note = Some(note);
        }
    }
```
- `keys.rs`:

```rust
/// The `[[commands]]` entry `chord` runs, by its index.
pub fn command_for(chord: &Chord) -> Option<usize> {
    SHORTCUTS.with(|s| s.borrow().command_for(chord))
}

/// The key of `[[commands]]` entry `index`, if it has one.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn command_chord(index: usize) -> Option<Chord> {
    SHORTCUTS.with(|s| s.borrow().command_chord(index))
}

/// A chord as a menu shows it: ⌃⌥⇧⌘ and the key on macOS, "Ctrl+Alt+Shift+K" elsewhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn chord_label(chord: &Chord, platform: Platform) -> String {
    let key = match chord.key {
        Key::Char(c) => c.to_ascii_uppercase().to_string(),
        Key::Num(c) => format!("Num {c}"),
        Key::F(n) => format!("F{n}"),
        Key::Left => "←".to_owned(),
        Key::Right => "→".to_owned(),
        Key::Up => "↑".to_owned(),
        Key::Down => "↓".to_owned(),
        Key::Enter => "Enter".to_owned(),
        Key::Tab => "Tab".to_owned(),
        Key::Backspace => "Backspace".to_owned(),
        Key::Delete => "Delete".to_owned(),
        Key::Home => "Home".to_owned(),
        Key::End => "End".to_owned(),
        Key::PageUp => "PgUp".to_owned(),
        Key::PageDown => "PgDn".to_owned(),
        Key::Escape => "Esc".to_owned(),
        Key::Space => "Space".to_owned(),
    };
    match platform {
        Platform::Mac => {
            let mut out = String::new();
            for (held, sign) in [(chord.ctrl, '⌃'), (chord.alt, '⌥'), (chord.shift, '⇧'), (chord.meta, '⌘')] {
                if held {
                    out.push(sign);
                }
            }
            out + &key
        }
        Platform::Other => {
            let mut parts: Vec<&str> = Vec::new();
            for (held, name) in [(chord.ctrl, "Ctrl"), (chord.alt, "Alt"), (chord.shift, "Shift"), (chord.meta, "Win")] {
                if held {
                    parts.push(name);
                }
            }
            parts.push(&key);
            parts.join("+")
        }
    }
}
```
- `actions.rs`:

```rust
/// `[[commands]]` entry `index`, run by its key or the macOS menu bar on the selection (the
/// focused item when nothing is selected).
pub fn run_command(index: usize, view: &View) {
    if view.shows_drives() {
        return view.note("Commands run on files and folders".to_owned());
    }
    let mut items = view.selected_items();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)));
    }
    crate::convert::run_by_index(index, items);
}
```
- `handle_key`: eylem bloğundan (`if let Some(action) = …`) sonra, `if editing || !window.get_list_focused()`'dan önce:

```rust
    // A `[[commands]]` key: on the selection, only while the file list has the keyboard.
    if let Some(index) = chord.as_ref().and_then(keys::command_for) {
        if editing || filtering || !window.get_list_focused() {
            return false;
        }
        actions::run_command(index, view);
        return true;
    }
```
- `app.slint`: `in property <[MenuEntry]> bar-commands;` (menü çubuğu özelliklerinin yanına) ve MenuBar'da Window menüsünden önce:

```slint
        if root.bar-commands.length > 0: Menu {
            title: "Commands";
            for entry in root.bar-commands: MenuItem {
                title: entry.title;
                enabled: entry.enabled;
                activated => { root.menu-command("command:" + entry.id); }
            }
        }
```
- `menu_bar.rs`: `on_menu_command`'ın `name =>` kolunun başı:

```rust
            name => {
                if let Some(index) = name.strip_prefix("command:").and_then(|i| i.parse::<usize>().ok()) {
                    return crate::actions::run_command(index, &view);
                }
                let Some(action) = Action::from_name(name) else { return };
```
  ve

```rust
/// The Commands menu: the commands that have a key, with the key in the title.
pub fn set_commands(window: &AppWindow, commands: &[CommandSpec]) {
    let entries = crate::convert::bar_entries(commands, |i| {
        keys::command_chord(i).map(|chord| keys::chord_label(&chord, Platform::Mac))
    });
    let entries: Vec<MenuEntry> =
        entries.into_iter().map(|(id, title, enabled)| MenuEntry { id, title: title.into(), enabled }).collect();
    window.set_bar_commands(ModelRc::new(VecModel::from(entries)));
}
```
  `apply_config`'te `keys::set_shortcuts`'tan sonra `#[cfg(target_os = "macos")] menu_bar::set_commands(window, &loaded.settings.commands);`.

- [ ] **Step 1: Write the failing tests**

`convert.rs` (test `spec` yardımcısı Task 3'te yeni alanları aldı):

```rust
#[test]
fn commands_with_a_menu_name_are_grouped_under_it() {
    let grouped = |name: &str, run: &[&str], group: &str| CommandSpec {
        menu: Some(group.to_owned()),
        ..spec(name, run, None, &[], true)
    };
    let commands = vec![
        grouped("Zip", &["7z", "{files}"], "Archives"),
        CommandSpec { menu: Some("Images".into()), ..spec("Small", &["m", "{in}", "{out}"], Some("{name}-s.{ext}"), &["jpg"], false) },
        spec("Plain", &["x", "{in}"], None, &[], false),
        grouped("Tar", &["tar", "{files}"], "Archives"),
    ];
    let states = vec![CommandState::Ready; 4];
    let (_, sub) = menu_entries(&[file("a.jpg")], &commands, &states);
    let lines: Vec<(u32, String, bool)> = sub.unwrap();
    assert_eq!(
        lines,
        [
            (COMMAND_FIRST + 2, "Plain".to_owned(), true),
            (COMMAND_GROUP, "Archives".to_owned(), false),
            (COMMAND_FIRST, "Zip".to_owned(), true),
            (COMMAND_FIRST + 3, "Tar".to_owned(), true),
            (COMMAND_GROUP, "Images".to_owned(), false),
            (COMMAND_FIRST + 1, "Small".to_owned(), true),
        ]
    );
}

#[test]
fn a_files_command_is_not_greyed_for_folders() {
    let zip = spec("Zip", &["7z", "a", "x.zip", "{files}"], None, &[], true);
    assert_eq!(command_state(&zip, &[folder("site"), file("a.txt")], Some(true), false), CommandState::Ready);
    let in_place = spec("Touch", &["touch", "{in}"], None, &[], true);
    assert!(matches!(command_state(&in_place, &[folder("site")], Some(true), false), CommandState::Off(_)));
}

#[test]
fn why_a_command_does_not_run_and_what_it_asks() {
    assert_eq!(not_for_text(&spec("Small", &["x"], None, &["jpg", "PNG"], false)), "Small runs on .jpg and .png files only");
    assert_eq!(not_for_text(&spec("Small", &["x"], None, &["jpg", "png", "gif"], false)), "Small runs on .jpg, .png and .gif files only");
    assert_eq!(not_for_text(&spec("Small", &["x"], None, &["jpg"], true)), "Small runs on .jpg files and folders only");
    assert_eq!(not_for_text(&spec("Touch", &["x"], None, &[], false)), "Touch does not run on folders");
    assert_eq!(ask_text("Zip", 1), "Run Zip on 1 item?");
    assert_eq!(ask_text("Zip", 1234), "Run Zip on 1,234 items?");
}

#[test]
fn the_menu_bar_lists_the_commands_with_a_key() {
    let zip = CommandSpec { menu: Some("Archives".into()), ..spec("Zip", &["7z", "{files}"], None, &[], true) };
    let commands = vec![spec("A", &["x"], None, &[], false), zip, spec("B", &["x"], None, &[], false)];
    let key = |i: usize| (i != 0).then(|| format!("⌃⌥{i}"));
    assert_eq!(
        bar_entries(&commands, key),
        [(2, "B    ⌃⌥2".to_owned(), true), (-1, "Archives".to_owned(), false), (1, "Zip    ⌃⌥1".to_owned(), true)]
    );
}
```

`operations.rs`:

```rust
#[test]
fn a_note_follows_the_detail() {
    assert_eq!(with_note("Done".to_owned(), Some(CANT_UNDO)), "Done · can't be undone");
    assert_eq!(with_note("45%".to_owned(), None), "45%");
}
```

`keys.rs`:

```rust
#[test]
fn chords_read_as_menus_show_them() {
    use gezik_config::shortcuts::parse_chord;
    let c = parse_chord("ctrl+alt+z", Platform::Other).unwrap().unwrap();
    assert_eq!(chord_label(&c, Platform::Other), "Ctrl+Alt+Z");
    let m = parse_chord("mod+shift+k", Platform::Mac).unwrap().unwrap();
    assert_eq!(chord_label(&m, Platform::Mac), "⇧⌘K");
    let num = parse_chord("num+", Platform::Other).unwrap().unwrap();
    assert_eq!(chord_label(&num, Platform::Other), "Num +");
}
```

`context_menu.rs`: `conversion_ids_meet_no_others`'ın `singles`'ına `COMMAND_GROUP`; `drop_menu_ids_are_their_own`'ın `others`'ına `COMMAND_GROUP`.

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik convert keys operations context_menu` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod ve davranış).
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler. macOS denetimi `app.slint`'i ve `menu_bar.rs`'i derler: Slint `MenuBar` içinde `if … : Menu` ya da `for` reddedilirse (derleme hatası), Commands menüsü ve `set_commands` çıkarılır, `bar_entries` kalır (test geçer) ve nota "macOS Commands menu: not built, Slint 1.18 MenuBar takes no dynamic menu" yazılır (sapma 6). `GEZIK_CONFIG_DIR` geçici bir klasörle `cargo run -p gezik`: `settings.toml`'a Task 9'daki "Copy txt" ve "List them" komutları; iki `.txt` seçili → Ctrl+Alt+K iki kopya; `.jpg`'de Ctrl+Alt+K → durum çubuğu "Copy txt runs on .txt files only"; Ctrl+Alt+L → "Run List them on 3 items?" → Run; panel "Done · can't be undone"; sağ tık → Commands ▸ "Tests" başlığı gri.
- [ ] **Step 5: Commit** "Run user commands by their keys, ask first when told to, group them under headings and say when a run can't be undone".

---

### Task 7: Arayüz — adres çubuğunda yol tamamlama

**Files:**
- Create: `crates/gezik/src/path_box.rs`, `crates/gezik/ui/widgets/path-suggestions.slint`
- Modify: `crates/gezik/ui/widgets/breadcrumb.slint`, `crates/gezik/ui/app.slint`, `crates/gezik/src/navigation.rs` (`resolve_typed` → `pub(crate)`, `navigate_text`), `crates/gezik/src/main.rs` (`mod path_box;`, `PathBox::new`, `handle_key`, `dirs_home` → `path_box::home`)

**Interfaces:**
- Consumes: Task 2 (`expand_typed`, `split_typed`, `shows_history`, `rank`, `sort_names`).
- Produces:

```rust
// path_box.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row { Heading(&'static str), Folder { title: String, detail: String, path: PathBuf } }
pub fn home() -> PathBuf;
pub fn expand(text: &str) -> String;
pub fn completion_target(text: &str, base: Option<&Path>, windows: bool) -> Option<(PathBuf, String)>;
pub fn folder_rows(folder: &Path, names: &[String], typed: &str) -> Vec<Row>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKey { Open, Move(bool), Accept, Go, Close, Field }
pub fn path_key(key: Key, shift: bool, open: bool, current: Option<usize>) -> PathKey;
pub fn step(rows: &[Row], current: Option<usize>, down: bool) -> Option<usize>;
pub fn first_folder(rows: &[Row]) -> Option<usize>;
pub fn accept_text(path: &Path) -> String;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read { Cached, Wait, Start }
pub fn read_needed(folder: &Path, cached: Option<&Path>, reading: &[PathBuf]) -> Read;
pub fn list_subfolders(dir: &Path) -> std::io::Result<Vec<String>>;   // worker threads only
#[derive(Clone)] pub struct PathBox;   // PathBox::new(window: &AppWindow, nav: Navigator) -> PathBox (Task 8 adds store and saved visits)
pub fn with_current(f: impl FnOnce(&PathBox));
impl PathBox {
    pub fn reset(&self);
    pub fn chord(&self, chord: &Chord) -> bool;
}
// navigation.rs
pub(crate) fn resolve_typed(text: &str, base: Option<&Path>) -> PathBuf;
```
- Slint: `path-suggestions.slint`'te `export struct PathRow { title: string, detail: string, path: string, heading: bool }` ve `export component PathSuggestions`; `breadcrumb.slint`'e `callback edited(string); public function set-text(text: string, at: int);`; `app.slint`'e `in property <[PathRow]> path-rows; in property <int> path-current: -1; callback path-edited(string); callback path-chosen(string); callback path-editing-ended(); public function set-path-text(text: string, at: int);`, `PathRow` dışa aktarılan listeye.

**Behavior:**
- `path_box.rs` saf kısmı:

```rust
//! The address bar while a path is typed (spec 6.1): suggestions of the sub-folders the text
//! points into, read on a thread of their own once typing pauses for 80 ms (1 s at most), and
//! the keys that move among them. The UI thread never touches the disk here.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::shortcuts::{Chord, Key};
use gezik_core::complete::{rank, shows_history, sort_names, split_typed};
use gezik_core::nav::{Location, expand_typed};
use gezik_core::ops::paths::same_path;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::{Navigator, resolve_typed};
use crate::{AppWindow, PathRow};

/// How long typing pauses before the folder is read.
const DEBOUNCE: Duration = Duration::from_millis(80);
/// How long a read may take (a network share, a sleeping disk) before the list gives up on it.
const READ_LIMIT: Duration = Duration::from_secs(1);

/// A line of the suggestion list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A greyed group title.
    Heading(&'static str),
    /// A folder: its name, the folder it is in (or ""), and where it leads.
    Folder { title: String, detail: String, path: PathBuf },
}

/// The user's home folder, `~`: USERPROFILE on Windows, HOME elsewhere.
pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `text` with `~` and the environment's variables put in (spec 6.1).
pub fn expand(text: &str) -> String {
    expand_typed(text, &home(), |name| std::env::var(name).ok(), cfg!(windows))
}

/// The folder whose names complete `text` (expanded) and the start of a name in it. A text
/// without a folder part completes in `base`, the folder on screen; `None` for a relative
/// text when there is none (This PC).
pub fn completion_target(text: &str, base: Option<&Path>, windows: bool) -> Option<(PathBuf, String)> {
    let (folder, typed) = split_typed(text, windows);
    let folder = if folder.is_empty() {
        base?.to_path_buf()
    } else if Path::new(&folder).is_relative() && base.is_none() {
        return None;
    } else {
        resolve_typed(&folder, base)
    };
    Some((folder, typed))
}

/// The suggestions of `names` (the sub-folders of `folder`) for `typed`.
pub fn folder_rows(folder: &Path, names: &[String], typed: &str) -> Vec<Row> {
    rank(names, typed)
        .into_iter()
        .map(|i| Row::Folder { title: names[i].clone(), detail: String::new(), path: folder.join(&names[i]) })
        .collect()
}

/// What a key does in the address bar (spec 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKey {
    /// Shows the list for the text now.
    Open,
    /// Moves the chosen row down (`true`) or up.
    Move(bool),
    /// Writes the chosen row (or the first) into the field.
    Accept,
    /// Goes to the chosen row.
    Go,
    /// Closes the list (typing goes on).
    Close,
    /// The field's own.
    Field,
}

/// What `key` (with `shift`, no other modifier) does with the list `open` or not and row
/// `current` chosen: ↓ opens and moves down, ↑ moves up, Tab writes the chosen or first row
/// (or opens), → writes only a chosen row, Enter goes only to a chosen row, Esc closes.
pub fn path_key(key: Key, shift: bool, open: bool, current: Option<usize>) -> PathKey {
    match key {
        Key::Down if open => PathKey::Move(true),
        Key::Down => PathKey::Open,
        Key::Up if open => PathKey::Move(false),
        Key::Tab if shift => PathKey::Field,
        Key::Tab if open => PathKey::Accept,
        Key::Tab => PathKey::Open,
        Key::Right if open && current.is_some() => PathKey::Accept,
        Key::Enter if open && current.is_some() => PathKey::Go,
        Key::Escape if open => PathKey::Close,
        _ => PathKey::Field,
    }
}

/// The row Up/Down moves to, past headings; up from the first folder is back to the text.
pub fn step(rows: &[Row], current: Option<usize>, down: bool) -> Option<usize> {
    let folder = |i: &usize| matches!(rows[*i], Row::Folder { .. });
    match (current, down) {
        (None, true) => (0..rows.len()).find(folder),
        (None, false) => None,
        (Some(at), true) => (at + 1..rows.len()).find(folder).or(Some(at)),
        (Some(at), false) => (0..at).rev().find(folder),
    }
}

/// The first folder row.
pub fn first_folder(rows: &[Row]) -> Option<usize> {
    step(rows, None, true)
}

/// What a taken suggestion writes into the field: its path with a separator at the end, so
/// that the next suggestions are its sub-folders.
pub fn accept_text(path: &Path) -> String {
    let text = path.display().to_string();
    if text.ends_with(std::path::is_separator) { text } else { format!("{text}{}", std::path::MAIN_SEPARATOR) }
}

/// Whether `folder`'s names are in memory, being read, or to be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    Cached,
    /// Being read already: a slow share is not read twice.
    Wait,
    Start,
}

pub fn read_needed(folder: &Path, cached: Option<&Path>, reading: &[PathBuf]) -> Read {
    if cached.is_some_and(|cached| same_path(cached, folder)) {
        Read::Cached
    } else if reading.iter().any(|path| same_path(path, folder)) {
        Read::Wait
    } else {
        Read::Start
    }
}

/// The names of the folders in `dir` (links to folders too, not Gezik's temporary names),
/// sorted as suggestions list them. Touches the disk: only on a worker thread.
pub fn list_subfolders(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let is_dir = match entry.file_type() {
            Ok(kind) if kind.is_symlink() => entry.path().is_dir(),
            Ok(kind) => kind.is_dir(),
            Err(_) => false,
        };
        if is_dir
            && let Some(name) = entry.file_name().to_str()
            && !gezik_ops::pending::is_internal_name(name)
        {
            names.push(name.to_owned());
        }
    }
    sort_names(&mut names);
    Ok(names)
}
```
- `PathBox` (aynı dosyada):

```rust
struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    /// The text as last typed; `None`: not changed since typing began (the path on screen).
    text: RefCell<Option<String>>,
    rows: RefCell<Vec<Row>>,
    current: Cell<Option<usize>>,
    debounce: slint::Timer,
    limit: slint::Timer,
    /// The last folder read and its sub-folders, for this typing.
    cache: RefCell<Option<(PathBuf, Rc<Vec<String>>)>>,
    /// Folders being read now.
    reading: RefCell<Vec<PathBuf>>,
    /// The folder, and the start of a name in it, the list waits for.
    wanted: RefCell<Option<(PathBuf, String)>>,
}

#[derive(Clone)]
pub struct PathBox(Rc<Inner>);

thread_local! {
    static CURRENT: RefCell<Option<PathBox>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's path box, if set up.
pub fn with_current(f: impl FnOnce(&PathBox)) {
    if let Some(path_box) = CURRENT.with(|c| c.borrow().clone()) {
        f(&path_box);
    }
}

fn slint_row(row: &Row) -> PathRow {
    match row {
        Row::Heading(title) => PathRow { title: (*title).into(), detail: "".into(), path: "".into(), heading: true },
        Row::Folder { title, detail, path } => PathRow {
            title: title.as_str().into(),
            detail: detail.as_str().into(),
            path: path.display().to_string().into(),
            heading: false,
        },
    }
}

impl PathBox {
    pub fn new(window: &AppWindow, nav: Navigator) -> PathBox {
        let this = PathBox(Rc::new(Inner {
            window: window.as_weak(),
            nav,
            text: RefCell::default(),
            rows: RefCell::default(),
            current: Cell::new(None),
            debounce: slint::Timer::default(),
            limit: slint::Timer::default(),
            cache: RefCell::default(),
            reading: RefCell::default(),
            wanted: RefCell::default(),
        }));
        window.on_path_edited(|text| with_current(|p| p.edited(text.into())));
        // By path, not by row: whatever happened to the list meanwhile, the click goes there.
        window.on_path_chosen(|path| with_current(|p| p.go_to(PathBuf::from(path.as_str()))));
        window.on_path_editing_ended(|| with_current(PathBox::reset));
        CURRENT.with(|c| *c.borrow_mut() = Some(this.clone()));
        this
    }

    fn edited(&self, text: String) {
        *self.0.text.borrow_mut() = Some(text);
        self.0.debounce.start(slint::TimerMode::SingleShot, DEBOUNCE, || with_current(PathBox::update));
    }

    /// Typing began or ended: the list, the text and what was read are forgotten.
    pub fn reset(&self) {
        self.0.text.take();
        self.0.cache.take();
        self.0.wanted.take();
        self.0.debounce.stop();
        self.0.limit.stop();
        self.show(Vec::new());
    }

    fn typed(&self) -> String {
        self.0
            .text
            .borrow()
            .clone()
            .or_else(|| self.0.window.upgrade().map(|w| w.get_current_path().to_string()))
            .unwrap_or_default()
    }

    fn is_open(&self) -> bool {
        !self.0.rows.borrow().is_empty() && self.0.window.upgrade().is_some_and(|w| w.get_path_editing())
    }

    /// The list for the text now: the history (Task 8) or the folder's names, read first if
    /// they are not in memory.
    fn update(&self) {
        let typed = self.typed();
        if shows_history(&typed, cfg!(windows)) {
            return self.show(self.history_list());
        }
        let base = match self.0.nav.active_location() {
            Location::Path(path) => Some(path),
            Location::Drives => None,
        };
        let Some((folder, prefix)) = completion_target(&expand(&typed), base.as_deref(), cfg!(windows)) else {
            return self.show(self.suggestions(Vec::new()));
        };
        let cached = self.0.cache.borrow().clone();
        let decision = read_needed(&folder, cached.as_ref().map(|(f, _)| f.as_path()), &self.0.reading.borrow());
        match decision {
            Read::Cached => {
                let names = cached.map(|(_, names)| names).unwrap_or_default();
                self.show(self.suggestions(folder_rows(&folder, &names, &prefix)));
            }
            Read::Wait => self.wait_for(folder, prefix),
            Read::Start => {
                self.wait_for(folder.clone(), prefix);
                self.read(folder);
            }
        }
    }

    /// The folder's suggestions as shown (Task 8 adds the history's matches under them).
    fn suggestions(&self, rows: Vec<Row>) -> Vec<Row> {
        rows
    }

    /// The list for an empty address (Task 8: Recent and Frequent).
    fn history_list(&self) -> Vec<Row> {
        Vec::new()
    }

    fn wait_for(&self, folder: PathBuf, prefix: String) {
        *self.0.wanted.borrow_mut() = Some((folder, prefix));
        self.0.limit.start(slint::TimerMode::SingleShot, READ_LIMIT, || with_current(PathBox::too_slow));
    }

    /// Reads `folder` on a thread of its own.
    fn read(&self, folder: PathBuf) {
        self.0.reading.borrow_mut().push(folder.clone());
        let spawned = std::thread::Builder::new().name("gezik-complete".into()).spawn({
            let folder = folder.clone();
            move || {
                let names = list_subfolders(&folder);
                let _ = slint::invoke_from_event_loop(move || with_current(|p| p.listed(folder, names)));
            }
        });
        if spawned.is_err() {
            self.0.reading.borrow_mut().retain(|path| !same_path(path, &folder));
        }
    }

    /// A read is done: kept for this typing, shown if the list still waits for it.
    fn listed(&self, folder: PathBuf, names: std::io::Result<Vec<String>>) {
        self.0.reading.borrow_mut().retain(|path| !same_path(path, &folder));
        // An unreadable folder suggests nothing.
        let names = Rc::new(names.unwrap_or_default());
        *self.0.cache.borrow_mut() = Some((folder.clone(), names.clone()));
        let wanted = self.0.wanted.borrow().clone();
        if let Some((want, prefix)) = wanted
            && same_path(&want, &folder)
        {
            self.0.wanted.take();
            self.0.limit.stop();
            self.show(self.suggestions(folder_rows(&folder, &names, &prefix)));
        }
    }

    /// The read took longer than `READ_LIMIT`: no folder suggestions (spec 6.1); a later
    /// result waits for the next key.
    fn too_slow(&self) {
        if self.0.wanted.take().is_some() {
            self.show(self.suggestions(Vec::new()));
        }
    }

    fn show(&self, rows: Vec<Row>) {
        self.0.current.set(None);
        if let Some(window) = self.0.window.upgrade() {
            window.set_path_rows(ModelRc::new(VecModel::from(rows.iter().map(slint_row).collect::<Vec<_>>())));
            window.set_path_current(-1);
        }
        *self.0.rows.borrow_mut() = rows;
    }

    fn set_current(&self, current: Option<usize>) {
        self.0.current.set(current);
        if let Some(window) = self.0.window.upgrade() {
            window.set_path_current(current.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        }
    }

    fn path_of(&self, index: usize) -> Option<PathBuf> {
        match self.0.rows.borrow().get(index) {
            Some(Row::Folder { path, .. }) => Some(path.clone()),
            _ => None,
        }
    }

    /// A key while the address bar is typed in (no Ctrl, Alt or ⌘); returns whether it was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        match path_key(chord.key, chord.shift, self.is_open(), self.0.current.get()) {
            PathKey::Field => return false,
            PathKey::Open => {
                self.0.debounce.stop();
                self.update();
            }
            PathKey::Move(down) => {
                let next = step(&self.0.rows.borrow(), self.0.current.get(), down);
                self.set_current(next);
            }
            PathKey::Accept => {
                let chosen = self.0.current.get().or_else(|| first_folder(&self.0.rows.borrow()));
                if let Some(path) = chosen.and_then(|i| self.path_of(i)) {
                    self.accept(&path);
                }
            }
            PathKey::Go => {
                if let Some(path) = self.0.current.get().and_then(|i| self.path_of(i)) {
                    self.go_to(path);
                }
            }
            PathKey::Close => self.show(Vec::new()),
        }
        true
    }

    /// Writes `path` into the field (typing goes on, now inside it).
    fn accept(&self, path: &Path) {
        let Some(window) = self.0.window.upgrade() else { return };
        let text = accept_text(path);
        window.invoke_set_path_text(text.as_str().into(), i32::try_from(text.len()).unwrap_or(i32::MAX));
        self.edited(text);
    }

    fn go_to(&self, path: PathBuf) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_path_editing(false);
        }
        self.reset();
        self.0.nav.go(Location::Path(path));
    }
}
```
- `navigation.rs`: `resolve_typed` `pub(crate)`; `navigate_text`:

```rust
    /// Goes to a typed path: `~` and environment variables put in, then [`resolve_typed`].
    pub fn navigate_text(&self, text: String) {
        let text = crate::path_box::expand(text.trim());
        if text.is_empty() {
            return;
        }
        let path = match self.active_location() {
            Location::Path(base) => resolve_typed(&text, Some(&base)),
            Location::Drives => resolve_typed(&text, None),
        };
        self.go(Location::Path(path));
    }
```
- `main.rs`: `mod path_box;`; `let _path_box = path_box::PathBox::new(&window, nav.clone());` (`nav.install()`'dan sonra); `dirs_home()` silinir, `resolve_start` `&path_box::home()` kullanır. `handle_key`'in yol düzenleme bloğu:

```rust
    if editing && let Some(chord) = &chord {
        // The suggestion list's keys first: ↓ ↑ Tab → Enter Esc (spec 6.1).
        if !has_modifier {
            let mut used = false;
            path_box::with_current(|p| used = p.chord(chord));
            if used {
                return true;
            }
        }
        if chord.key == Key::Escape && !has_modifier {
```
  ve `Action::FocusPath => { path_box::with_current(path_box::PathBox::reset); window.invoke_edit_path() }`.
- `breadcrumb.slint`: alanın `edited(text) => { root.edited(text); }`;

```slint
    // The field's text changed by typing (not when set).
    callback edited(string);

    // Puts `text` in the field with the cursor at byte `at` (a suggestion taken).
    public function set-text(text: string, at: int) {
        field.text = text;
        field.focus-at(at);
    }
```
- `path-suggestions.slint`:

```slint
import { Theme } from "../theme.slint";

export struct PathRow {
    title: string,
    // The folder it is in (history rows), or "".
    detail: string,
    // Where it leads, as shown ("" for a heading).
    path: string,
    // A greyed group title ("Recent", "Frequent", "History").
    heading: bool,
}

// The address bar's suggestions (path_box.rs), under the field while a path is typed. The
// field keeps the keyboard (a TouchArea takes no focus, as in the tab picker); ↑ ↓ Tab →
// Enter Esc reach Rust first; a click goes to the row's folder.
export component PathSuggestions inherits Rectangle {
    in property <[PathRow]> rows;
    in property <int> current: -1;
    callback chosen(string);

    property <length> row-height: Theme.row-height;
    height: min(root.rows.length, 18) * root.row-height + Theme.spacing;
    background: Theme.surface;
    border-radius: Theme.radius;
    border-width: 1px;
    border-color: Theme.border;
    drop-shadow-blur: 8px;
    drop-shadow-color: #00000040;
    accessible-role: list;
    accessible-label: "Suggestions";

    // Clicks between rows reach nothing under the list.
    TouchArea { }

    for row[i] in root.rows: Rectangle {
        x: 2px;
        y: Theme.spacing / 2 + i * root.row-height;
        width: parent.width - 4px;
        height: root.row-height;
        border-radius: Theme.radius;
        background: i == root.current ? Theme.selection : (touch.has-hover && !row.heading) ? Theme.hover : transparent;
        accessible-role: list-item;
        accessible-label: row.title;
        accessible-item-selectable: !row.heading;
        accessible-item-selected: i == root.current;
        HorizontalLayout {
            padding-left: Theme.spacing * 1.5;
            padding-right: Theme.spacing * 1.5;
            spacing: Theme.spacing * 2;
            Text {
                text: row.title;
                vertical-alignment: center;
                overflow: elide;
                font-weight: row.heading ? 600 : 400;
                font-size: row.heading ? Theme.font-size * 0.85 : Theme.font-size;
                color: row.heading ? Theme.foreground-muted : i == root.current ? Theme.selection-foreground : Theme.foreground;
            }
            if row.detail != "": Text {
                text: row.detail;
                vertical-alignment: center;
                horizontal-alignment: right;
                overflow: elide;
                color: i == root.current ? Theme.selection-foreground : Theme.foreground-muted;
            }
        }
        touch := TouchArea {
            enabled: !row.heading;
            mouse-cursor: row.heading ? MouseCursor.default : MouseCursor.pointer;
            clicked => { root.chosen(row.path); }
        }
    }
}
```
- `app.slint`: içe aktarma ve dışa aktarma listesine `PathSuggestions, PathRow`; özellikler ve işlev (yukarıda); `address := Breadcrumb { … edited(text) => { root.path-edited(text); } }`; `changed path-editing => { if !self.path-editing { root.path-editing-ended(); } if !self.path-editing && !keys.has-focus && !filter-bar.field-focused { keys.focus(); } }`; pencerenin sonunda, `Dialog`'dan önce:

```slint
    if root.path-editing && root.path-rows.length > 0: PathSuggestions {
        x: address.absolute-position.x;
        y: address.absolute-position.y + address.height + 2px;
        width: max(address.width, 320px);
        rows: root.path-rows;
        current: root.path-current;
        chosen(path) => { root.path-chosen(path); }
    }
```

- [ ] **Step 1: Write the failing tests** (`path_box.rs`'in sonunda)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn folder(name: &str) -> Row {
        Row::Folder { title: name.to_owned(), detail: String::new(), path: PathBuf::from(name) }
    }

    #[test]
    fn keys_in_the_address_bar() {
        use PathKey::*;
        assert_eq!(path_key(Key::Down, false, false, None), Open);
        assert_eq!(path_key(Key::Down, false, true, None), Move(true));
        assert_eq!(path_key(Key::Up, false, true, Some(0)), Move(false));
        assert_eq!(path_key(Key::Up, false, false, None), Field);
        assert_eq!(path_key(Key::Tab, false, false, None), Open);
        assert_eq!(path_key(Key::Tab, false, true, None), Accept, "Tab takes the first when none is chosen");
        assert_eq!(path_key(Key::Tab, true, true, Some(1)), Field, "Shift+Tab is the field's");
        assert_eq!(path_key(Key::Right, false, true, None), Field, "→ moves the cursor until a row is chosen");
        assert_eq!(path_key(Key::Right, false, true, Some(2)), Accept);
        assert_eq!(path_key(Key::Enter, false, true, None), Field, "Enter goes where the text says");
        assert_eq!(path_key(Key::Enter, false, true, Some(0)), Go);
        assert_eq!(path_key(Key::Escape, false, true, None), Close);
        assert_eq!(path_key(Key::Escape, false, false, None), Field, "the second Esc ends typing");
        assert_eq!(path_key(Key::Char('a'), false, true, Some(0)), Field);
    }

    #[test]
    fn up_and_down_skip_headings_and_up_from_the_top_is_the_text() {
        let rows = [Row::Heading("Recent"), folder("a"), folder("b"), Row::Heading("Frequent"), folder("c")];
        assert_eq!(step(&rows, None, true), Some(1));
        assert_eq!(step(&rows, Some(2), true), Some(4));
        assert_eq!(step(&rows, Some(4), true), Some(4), "stays at the end");
        assert_eq!(step(&rows, Some(4), false), Some(2));
        assert_eq!(step(&rows, Some(1), false), None, "back to the text");
        assert_eq!(step(&[], None, true), None);
        assert_eq!(first_folder(&rows), Some(1));
    }

    #[test]
    fn a_taken_suggestion_ends_in_a_separator() {
        let path = std::path::absolute("/a").unwrap().join("b");
        assert_eq!(accept_text(&path), format!("{}{}", path.display(), std::path::MAIN_SEPARATOR));
        let root = std::path::absolute("/").unwrap();
        assert_eq!(accept_text(&root), root.display().to_string(), "a root has its separator");
    }

    #[test]
    fn a_folder_is_read_once_at_a_time() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        assert_eq!(read_needed(a, Some(a), &[]), Read::Cached);
        assert_eq!(read_needed(a, Some(b), &[a.to_path_buf()]), Read::Wait, "a slow share is not read twice");
        assert_eq!(read_needed(a, Some(b), &[b.to_path_buf()]), Read::Start);
        assert_eq!(read_needed(a, None, &[]), Read::Start);
    }

    #[test]
    fn the_folder_to_list_is_where_the_text_points() {
        let windows = cfg!(windows);
        let base = std::path::absolute("/work").unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        let (folder, typed) = completion_target(&format!("src{sep}ma"), Some(&base), windows).unwrap();
        assert_eq!((folder, typed.as_str()), (base.join("src"), "ma"));
        let (folder, typed) = completion_target("ma", Some(&base), windows).unwrap();
        assert_eq!((folder, typed.as_str()), (base.clone(), "ma"));
        assert!(completion_target(&format!("src{sep}ma"), None, windows).is_none(), "nothing to start from in This PC");
        let root = std::path::absolute("/").unwrap();
        let (folder, typed) = completion_target(&format!("{}tmp{sep}x", root.display()), None, windows).unwrap();
        assert_eq!((folder, typed.as_str()), (root.join("tmp"), "x"));
    }

    #[test]
    fn suggestions_lead_into_the_folder() {
        let folder = Path::new("/home/ali");
        let rows = folder_rows(folder, &["Alpha".to_owned(), "beta".to_owned(), "alpine".to_owned()], "AL");
        assert_eq!(
            rows,
            [
                Row::Folder { title: "Alpha".into(), detail: String::new(), path: folder.join("Alpha") },
                Row::Folder { title: "alpine".into(), detail: String::new(), path: folder.join("alpine") },
            ]
        );
    }

    #[test]
    fn only_folders_are_listed_sorted() {
        let dir = std::env::temp_dir().join(format!("gezik-path-box-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for name in ["b", "A", ".gezik-copying-0123456789abcdef-0"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        std::fs::write(dir.join("c.txt"), "x").unwrap();
        assert_eq!(list_subfolders(&dir).unwrap(), ["A", "b"]);
        assert!(list_subfolders(&dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec 1: suggestions within 100 ms of a pause: 80 ms of it waiting, the rest for reading
    /// and ranking. Run: `cargo test --release -p gezik path_box -- --ignored`.
    #[test]
    #[ignore = "timing; run in release with --ignored"]
    fn ten_thousand_folders_are_listed_and_ranked_within_budget() {
        let dir = std::env::temp_dir().join(format!("gezik-complete-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..10_000 {
            std::fs::create_dir_all(dir.join(format!("Klasör {i:05}"))).unwrap();
        }
        let started = Instant::now();
        let names = list_subfolders(&dir).unwrap();
        let rows = folder_rows(&dir, &names, "klasör 0999");
        let took = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((names.len(), rows.len()), (10_000, 10));
        assert!(took < Duration::from_millis(20), "{took:?}");
    }
}
```

  `navigation.rs` testlerine:

```rust
#[test]
fn a_typed_path_is_expanded_before_it_is_resolved() {
    let base = std::path::absolute("/work").unwrap();
    let home = std::path::absolute("/home/ali").unwrap();
    let text = gezik_core::nav::expand_typed("~/x", &home, |_| None, cfg!(windows));
    assert_eq!(resolve_typed(&text, Some(&base)), home.join("x"));
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik path_box navigation` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod; Slint özellikleri ve bileşen).
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; `cargo test --release -p gezik path_box -- --ignored` → PASS (süre notlara, Task 9); çapraz denetimler. `cargo run -p gezik`: Ctrl+L, Ctrl+A, `C:\Wi` (Linux'ta `/us`) → liste 80 ms sonra; ↓ ↓ Tab → alan `C:\Windows\`, alt klasörler; → (seçimsiz) imleci oynatır; Esc liste kapanır, Esc yazma biter; `%USERPROFILE%\Desktop` / `~/Desktop` Enter gider; satıra tık gider.
- [ ] **Step 5: Commit** "Suggest sub-folders while a path is typed, read off the UI thread, and expand ~ and variables".

---

### Task 8: Arayüz — klasör geçmişi: ziyaretler, Recent/Frequent, geçmiş eşleşmeleri, silinmişler, `remember`, `clear-history`

**Files:**
- Modify: `crates/gezik/src/path_box.rs`, `crates/gezik/src/navigation.rs` (`on_visited`, `is_visit`), `crates/gezik/src/actions.rs` (`ClearHistory`), `crates/gezik/src/main.rs` (`PathBox::new`'in yeni argümanları, `apply_config`), `crates/gezik/ui/app.slint` (macOS Go menüsü)

**Interfaces:**
- Consumes: Task 1 (`FolderHistory`, `Visit`), Task 4 (`HistorySettings`, `State.history`, `Action::ClearHistory`), Task 7 (`PathBox`, `Row`).
- Produces:

```rust
// navigation.rs
impl Navigator { pub fn on_visited(&self, f: impl Fn(&Path) + 'static); }
fn is_visit(mode: &Mode, opened: bool) -> bool;
// path_box.rs
pub const RECENT: usize = 5;
pub const FREQUENT: usize = 7;
pub const HISTORY_MATCHES: usize = 5;
pub fn history_rows(history: &FolderHistory, now: u64) -> Vec<Row>;
pub fn with_history(rows: Vec<Row>, history: &FolderHistory, typed: &str, now: u64) -> Vec<Row>;
pub fn gone_locally(path: &Path) -> bool;      // worker threads only
pub fn set_settings(history: HistorySettings);
// PathBox::new(window: &AppWindow, nav: Navigator, store: Option<ConfigStore>, saved: Vec<Visit>) -> PathBox
impl PathBox { pub fn forget(&self, say: bool); }
```

**Behavior:**
- `navigation.rs`: `Inner`'a `on_visited: Vec<Rc<dyn Fn(&Path)>>` ve `visit_next_show: bool`; `open_tab`'ın `activate` dalı `with_tabs`'tan önce `self.0.borrow_mut().visit_next_show = true;`; `finish_load`'un sonunda (`schedule_refresh`'ten sonra):

```rust
        let (opened, visited) = {
            let mut inner = self.0.borrow_mut();
            (std::mem::take(&mut inner.visit_next_show), inner.on_visited.clone())
        };
        if is_visit(&mode, opened)
            && let Location::Path(path) = &location
        {
            for f in &visited {
                f(path);
            }
        }

/// Whether a finished load is a visit (spec 6.2): a move, or the first show of a tab opened
/// there; not a reload or a tab switch.
fn is_visit(mode: &Mode, opened: bool) -> bool {
    matches!(mode, Mode::Move(_)) || opened
}

    /// Calls `f` with each folder the user goes to (spec 6.2).
    pub fn on_visited(&self, f: impl Fn(&Path) + 'static) {
        self.0.borrow_mut().on_visited.push(Rc::new(f));
    }
```
- `path_box.rs` saf eklemeler:

```rust
/// Rows of the empty address's lists.
pub const RECENT: usize = 5;
pub const FREQUENT: usize = 7;
/// History rows under a folder's suggestions.
pub const HISTORY_MATCHES: usize = 5;

/// A visited folder's row: its name (the whole path for a root) and the folder it is in.
fn visit_row(visit: &Visit) -> Row {
    let title =
        visit.path.file_name().map_or_else(|| visit.path.display().to_string(), |name| name.to_string_lossy().into_owned());
    let detail = visit.path.parent().map(|parent| parent.display().to_string()).unwrap_or_default();
    Row::Folder { title, detail, path: visit.path.clone() }
}

/// The list of an empty address (or `~`, a root): "Recent" (the last 5) and "Frequent" (the
/// best 7 of the others) (spec 6.2).
pub fn history_rows(history: &FolderHistory, now: u64) -> Vec<Row> {
    let recent = history.recent(RECENT);
    let frequent = history.frequent(FREQUENT, now, &recent);
    let mut rows = Vec::new();
    if !recent.is_empty() {
        rows.push(Row::Heading("Recent"));
        rows.extend(recent.iter().map(|visit| visit_row(visit)));
    }
    if !frequent.is_empty() {
        rows.push(Row::Heading("Frequent"));
        rows.extend(frequent.iter().map(|visit| visit_row(visit)));
    }
    rows
}

/// `rows` (a folder's suggestions) with the history's folders whose path holds `typed` under
/// them, after a "History" heading (at most 5, none already listed).
pub fn with_history(mut rows: Vec<Row>, history: &FolderHistory, typed: &str, now: u64) -> Vec<Row> {
    let listed: Vec<PathBuf> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Folder { path, .. } => Some(path.clone()),
            Row::Heading(_) => None,
        })
        .collect();
    let found = history.matching(typed, HISTORY_MATCHES, now, |path| listed.iter().any(|l| same_path(l, path)));
    if !found.is_empty() {
        rows.push(Row::Heading("History"));
        rows.extend(found.iter().map(|visit| visit_row(visit)));
    }
    rows
}

/// `\\server\share` (and `//server`) paths are network ones, whatever the drive says.
fn is_network_text(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

/// Whether `path` is gone from a local disk (spec 6.2): the system says it is not there and
/// its drive is no network one (an unanswering share keeps its folders). Touches the disk:
/// only on a worker thread.
pub fn gone_locally(path: &Path) -> bool {
    if is_network_text(path) {
        return false;
    }
    match std::fs::metadata(path) {
        // The drive is asked about through what is still there of the path.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => gezik_platform::fs::nearest_existing(path)
            .and_then(|existing| gezik_platform::fs::drive_facts(&existing).ok())
            .is_some_and(|facts| facts.kind != gezik_platform::fs::DiskKind::Network),
        _ => false,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

thread_local! {
    /// settings.toml's `[history] remember`.
    static REMEMBER: Cell<bool> = const { Cell::new(true) };
}

fn remember() -> bool {
    REMEMBER.with(Cell::get)
}

/// settings.toml changed: with `remember = false` nothing is recorded and what was is
/// forgotten (spec 6.2).
pub fn set_settings(history: HistorySettings) {
    REMEMBER.with(|r| r.set(history.remember));
    if !history.remember {
        with_current(|p| p.forget(false));
    }
}
```
- `Inner`'a `history: RefCell<FolderHistory>`, `store: Option<ConfigStore>`, `checked: RefCell<Vec<PathBuf>>` (bu yazmada denetlenenler). `PathBox::new(window, nav, store, saved)`: `remember()` ise `FolderHistory::from_visits(saved, now())`; değilse boş geçmiş ve `saved` boş değilse `store.update_state(|s| s.history.clear())`. `new` sonunda `nav.on_visited(|path| with_current(|p| p.visited(path)));` (`nav` taşınmadan önce `nav.clone()` ile). `reset` `self.0.checked.borrow_mut().clear()` da yapar. Değişen/eklenen yöntemler:

```rust
    fn suggestions(&self, rows: Vec<Row>) -> Vec<Row> {
        let typed = expand(&self.typed());
        let typed = if cfg!(windows) { typed.replace('/', "\\") } else { typed };
        with_history(rows, &self.0.history.borrow(), &typed, now())
    }

    fn history_list(&self) -> Vec<Row> {
        let rows = history_rows(&self.0.history.borrow(), now());
        self.check_gone(&rows);
        rows
    }

    /// A folder was gone to: counted, and state.toml written by its writer thread.
    fn visited(&self, path: &Path) {
        if !remember() {
            return;
        }
        self.0.history.borrow_mut().visit(path, now());
        self.save();
    }

    fn save(&self) {
        if let Some(store) = &self.0.store {
            let visits = self.0.history.borrow().visits().to_vec();
            store.update_state(move |state| state.history = visits);
        }
    }

    /// Forgets every folder visited (`clear-history`; `remember = false`); `say`: the status
    /// bar says so.
    pub fn forget(&self, say: bool) {
        let had = !self.0.history.borrow().is_empty();
        self.0.history.borrow_mut().clear();
        if had {
            self.save();
        }
        if say && let Some(window) = self.0.window.upgrade() {
            window.set_status("Folder history cleared".into());
        }
        if self.is_open() {
            self.update();
        }
    }

    /// Looks on a thread whether the folders of `rows` not looked at yet in this typing are
    /// gone from a local disk.
    fn check_gone(&self, rows: &[Row]) {
        let paths: Vec<PathBuf> = {
            let checked = self.0.checked.borrow();
            rows.iter()
                .filter_map(|row| match row {
                    Row::Folder { path, .. } if !checked.iter().any(|c| same_path(c, path)) => Some(path.clone()),
                    _ => None,
                })
                .collect()
        };
        if paths.is_empty() {
            return;
        }
        self.0.checked.borrow_mut().extend(paths.iter().cloned());
        let _ = std::thread::Builder::new().name("gezik-history-check".into()).spawn(move || {
            let gone: Vec<PathBuf> = paths.into_iter().filter(|path| gone_locally(path)).collect();
            if !gone.is_empty() {
                let _ = slint::invoke_from_event_loop(move || with_current(|p| p.gone(gone)));
            }
        });
    }

    fn gone(&self, gone: Vec<PathBuf>) {
        if !self.0.history.borrow_mut().remove(&gone) {
            return;
        }
        self.save();
        if self.is_open() {
            self.update();
        }
    }
```
- `main.rs`: `let _path_box = path_box::PathBox::new(&window, nav.clone(), config.clone(), saved_state.history.clone());`; `apply_config`'te `path_box::set_settings(loaded.settings.history);`.
- `actions.rs`: `Action::ClearHistory => crate::path_box::with_current(|p| p.forget(true)),` ("Not 6a's" listesinden çıkar; modül belgesi "6a's and 6b's").
- `app.slint` Go menüsünün sonuna: `MenuSeparator { }` ve `MenuItem { title: "Clear Folder History"; activated => { root.menu-command("clear-history"); } }` (bağlaması olmadığı için `menu_bar.rs` `actions::run`'a düşer).

- [ ] **Step 1: Write the failing tests**

`path_box.rs`:

```rust
const NOW: u64 = 1_800_000_000;

fn titles(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Heading(title) => format!("# {title}"),
            Row::Folder { title, .. } => title.clone(),
        })
        .collect()
}

#[test]
fn an_empty_address_lists_recent_then_frequent() {
    let visits: Vec<Visit> = (0..6u64)
        .map(|i| Visit { path: PathBuf::from(format!("/r{i}")), count: 1, last: NOW - 100 + i })
        .chain([Visit { path: PathBuf::from("/often"), count: 40, last: NOW - 30 * 86_400 }])
        .collect();
    let history = FolderHistory::from_visits(visits, NOW);
    assert_eq!(
        titles(&history_rows(&history, NOW)),
        ["# Recent", "r5", "r4", "r3", "r2", "r1", "# Frequent", "often", "r0"]
    );
    let Row::Folder { detail, .. } = &history_rows(&history, NOW)[1] else { panic!("a folder row") };
    assert_eq!(detail, &Path::new("/").display().to_string());
    assert!(history_rows(&FolderHistory::default(), NOW).is_empty());
}

#[test]
fn typed_text_adds_matching_history_under_the_folders() {
    let history = FolderHistory::from_visits(
        vec![
            Visit { path: PathBuf::from("/srv/Projeler"), count: 3, last: NOW },
            Visit { path: PathBuf::from("/home/ali/proje-eski"), count: 1, last: NOW },
            Visit { path: PathBuf::from("/home/ali/Belgeler"), count: 9, last: NOW },
        ],
        NOW,
    );
    let folder = Path::new("/home/ali");
    let rows = folder_rows(folder, &["proje-eski".to_owned(), "Projeler".to_owned()], "proje");
    let rows = with_history(rows, &history, "proje", NOW);
    assert_eq!(titles(&rows), ["proje-eski", "Projeler", "# History", "Projeler"]);
    let Row::Folder { detail, .. } = &rows[3] else { panic!("a folder row") };
    assert_eq!(detail, &Path::new("/srv").display().to_string());
    assert!(with_history(Vec::new(), &history, "zzz", NOW).is_empty());
}

#[test]
fn only_a_folder_gone_from_a_local_disk_is_dropped() {
    let dir = std::env::temp_dir().join(format!("gezik-gone-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("here")).unwrap();
    assert!(!gone_locally(&dir.join("here")));
    assert!(!gone_locally(Path::new(r"\\gezik-no-such-server\share\x")));
    assert!(!gone_locally(Path::new("//gezik-no-such-server/share/x")));
    // A file system read as a network one (a container's overlay, tmpfs, btrfs on Linux)
    // keeps its folders.
    let local = gezik_platform::fs::drive_facts(&dir).is_ok_and(|f| f.kind != gezik_platform::fs::DiskKind::Network);
    assert_eq!(gone_locally(&dir.join("gone")), local);
    let _ = std::fs::remove_dir_all(&dir);
}
```

`navigation.rs`:

```rust
#[test]
fn moves_and_opened_tabs_are_visits_but_reloads_are_not() {
    for mode in moves() {
        assert!(is_visit(&mode, false), "{mode:?}");
    }
    assert!(!is_visit(&Mode::Show, false), "a reload or a tab switch");
    assert!(is_visit(&Mode::Show, true), "a tab opened there");
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik path_box navigation` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod).
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler. `cargo run -p gezik` (`GEZIK_CONFIG_DIR` geçici): birkaç klasöre git; Ctrl+L, Ctrl+A, Backspace → "Recent" ve "Frequent"; `state.toml`'da `[[history.folders]]`; `pro` yaz → alt klasörler, altında "History"; Explorer'da ziyaret edilmiş bir klasörü sil → boş adres listesinde 1 sn içinde düşer; `settings.toml`'a `[history] remember = false` → `state.toml`'dan gider; `[shortcuts] clear-history = "ctrl+shift+h"` → durum çubuğu "Folder history cleared".
- [ ] **Step 5: Commit** "Remember visited folders: Recent and Frequent in the address bar, matches while typing, gone ones dropped, clear-history".

---

### Task 9: Ölçüm, Linux kabı (`gui.sh paths`, `gui.sh commands`), notlar, test listeleri, spec

**Files:**
- Modify: `scripts/linux/gui.sh` (`paths` ve `commands` kipleri), `scripts/linux/gui-lib.sh` (`copied`, `sorted`, `copies` buraya taşınır), `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` ("## Alt proje 6b (Klavye: yol tamamlama, geçmiş, komutlar) sonrası"), `docs/superpowers/notes/macos-test.md`, `docs/superpowers/notes/linux-test.md`, `docs/superpowers/specs/2026-10-07-klavye-paketi-design.md` (Durum)

- [ ] **Step 1: Öneri süresi.** `cargo test --release -p gezik path_box -- --ignored` (10.000 alt klasör, okuma + sıralama). Hedef ≤ 20 ms (80 ms beklemeyle ≤ 100 ms). Aşarsa: önce sıralamanın payı ölçülür (`fold_text` her adda bir ayırma yapar; gerekirse adlar okuyucu iş parçacığında bir kez katlanıp önbellekte tutulur). Sayılar ve karar notlara.
- [ ] **Step 2: Exe ve bellek.** `cargo build --release -p gezik`; master `0eb16fd` ve 6a'nın ucu (`feat/keyboard-6a`) ile ayrı çalışma ağaçları ve hedef klasörleriyle karşılaştır. Hedef: ≤ 22.241.056 bayt (master + 0,5 MB); 6b'nin payı 6a'nın ucuna göre yazılır. Bütçe aşılırsa ölçülen dağılım (Slint bileşeni, `path_box`, komutlar) notlara; daraltma önerisiyle kullanıcıya sorulur. Boşta bellek: Linux kabında `scripts/linux/gui.sh memory`, Windows'ta (kilit açıksa) `scripts/perf/measure.ps1` (≤ 7 MB).
- [ ] **Step 3: Linux kabı.** `docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/test.sh unit` → hepsi geçer. `keyboard()`'daki `copied`, `sorted`, `copies` `gui-lib.sh`'a taşınır (davranış aynı). `gui.sh`'e iki kip (başlık yorumundaki listeye ve `*)` "all" listesine de):

```bash
paths() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/p /tmp/cfg /root/Projekt && mkdir -p /tmp/p/Alpha /tmp/p/alpine /tmp/p/Beta /tmp/cfg /root/Projekt
    echo x > /tmp/p/notes.txt
    printf '[shortcuts]\nclear-history = "ctrl+shift+h"\n' >/tmp/cfg/settings.toml
    : >/tmp/gezik-gui-paths.log
    GEZIK_TEST_DIR=/tmp/p GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/p >>/tmp/gezik-gui-paths.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    kept() { sleep 1; grep -qF "$1" /tmp/cfg/state.toml 2>/dev/null; }

    # Suggestions: "al" gives Alpha and alpine (notes.txt is a file). Esc closes the list only.
    key ctrl+l; typ /tmp/p/al; sleep 0.6; shot paths-suggest
    key Escape; sleep 0.3; shot paths-no-suggest
    check "paths: suggestions show under the field" '! cmp -s "$SHOTS/paths-suggest.png" "$SHOTS/paths-no-suggest.png"'
    key Down; sleep 0.3; key Down Tab; sleep 0.6; key Return; sleep 1
    check "paths: Down opens, Down chooses, Tab writes, Enter goes" 'is Alpha'
    key ctrl+l; typ /tmp/p/b; sleep 0.6; key Down Return; sleep 1
    check "paths: Enter on a chosen row goes there (case ignored)" 'is Beta'
    key ctrl+l; typ '~/Projekt'; key Return; sleep 1
    check "paths: ~ is the home folder" 'is Projekt'
    key ctrl+l; typ '$GEZIK_TEST_DIR/Alpha'; key Return; sleep 1
    check "paths: \$NAME is put in" 'is Alpha'
    key ctrl+l; typ '${GEZIK_TEST_DIR}/Beta'; key Return; sleep 1
    check "paths: \${NAME} too" 'is Beta'
    key ctrl+l; typ '$GEZIK_NOPE/x'; key Return; sleep 1
    check "paths: an unknown variable stays (nothing opens)" 'is Beta'
    key ctrl+l; typ /tmp/p/al; sleep 0.6; key Escape Escape; sleep 0.3; key Return; sleep 1
    check "paths: two Esc end typing (Enter is the list's then)" 'is Beta'

    # History: visits so far Alpha, Beta, Projekt, Alpha, Beta (the start folder is no visit).
    check "paths: visits go to state.toml" 'kept "/tmp/p/Alpha"'
    key ctrl+l BackSpace; sleep 0.6; key Down Down Return; sleep 1
    check "paths: the empty address lists the recent ones (Beta, then Alpha)" 'is Alpha'
    key ctrl+l; typ alp; sleep 0.6; shot paths-history-match
    key Escape Escape
    key ctrl+shift+h; sleep 1.5
    check "paths: clear-history empties it" '! grep -qF "/tmp/p/Alpha" /tmp/cfg/state.toml'
    key ctrl+l; typ /tmp/p/Alpha; key Return; sleep 1
    check "paths: a visit after clearing is kept again" 'kept "/tmp/p/Alpha"'
    printf '[history]\nremember = false\n' >/tmp/cfg/settings.toml; sleep 2
    check "paths: remember = false forgets" '! grep -qF "/tmp/p/Alpha" /tmp/cfg/state.toml'
    key ctrl+l; typ /tmp/p/Beta; key Return; sleep 1.5
    check "paths: and records nothing" '! grep -qF "/tmp/p/Beta" /tmp/cfg/state.toml'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-paths.log && fail "paths: no panic" || pass "paths: no panic"
}

commands() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/c /tmp/c-out /tmp/cfg && mkdir -p /tmp/c/Docs /tmp/c-out /tmp/cfg
    for n in a.txt b.txt c.jpg; do echo "$n" > "/tmp/c/$n"; done
    cat >/tmp/cfg/settings.toml <<'EOF'
[[commands]]
name = "List them"
run = ["sh", "-c", "printf '%s\\n' \"$@\" > /tmp/c-out/list.txt; pwd >> /tmp/c-out/list.txt", "sh", "{files}"]
folders = true
shortcut = "ctrl+alt+l"
menu = "Tests"
ask = true

[[commands]]
name = "Copy txt"
run = ["cp", "{in}", "{out}"]
output = "{name}-copy.{ext}"
types = ["txt"]
shortcut = "ctrl+alt+k"

[[commands]]
name = "Clash"
run = ["true"]
shortcut = "ctrl+f"
EOF
    : >/tmp/gezik-gui-commands.log
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/c >>/tmp/gezik-gui-commands.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5

    # Rows: Docs 118, a.txt 144, b.txt 170, c.jpg 196.
    click 255 144; xdotool keydown shift; click 255 170; xdotool keyup shift
    key ctrl+alt+k; sleep 2
    check "commands: a key runs a command on the selection" '[ -f /tmp/c/a-copy.txt ] && [ -f /tmp/c/b-copy.txt ]'
    click 255 "$(row /tmp/c c.jpg)"; key ctrl+alt+k; sleep 1.5
    check "commands: not for a .jpg: nothing runs" '[ ! -e /tmp/c/c-copy.jpg ]'
    key ctrl+a ctrl+alt+l; sleep 1
    check "commands: ask = true asks first (nothing ran yet)" '[ ! -e /tmp/c-out/list.txt ]'
    key Escape; sleep 1
    check "commands: Cancel runs nothing" '[ ! -e /tmp/c-out/list.txt ]'
    key ctrl+a ctrl+alt+l; sleep 1; key Return; sleep 2
    check "commands: {files} runs once with every item, in their folder" \
        '[ "$(head -n -1 /tmp/c-out/list.txt | xargs -n1 basename | paste -sd" ")" = "Docs a-copy.txt a.txt b-copy.txt b.txt c.jpg" ] && [ "$(tail -1 /tmp/c-out/list.txt)" = /tmp/c ]'
    key ctrl+z; sleep 2
    check "commands: {files} left nothing to undo (Ctrl+Z undid the copies)" '[ ! -e /tmp/c/a-copy.txt ] && [ ! -e /tmp/c/b-copy.txt ]'
    check "commands: a clashing key is left out, with a warning" \
        'grep -q "commands\[3\]: shortcut \"ctrl+f\" is already used by filter" /tmp/gezik-gui-commands.log'
    click 255 144; key ctrl+f; typ jpg; key Down ctrl+a
    check "commands: Ctrl+F is still the filter's" 'copies c.jpg'
    key Escape Escape

    # Past the command line limit: half of ARG_MAX, in names of about 220 bytes.
    local n=$(( $(getconf ARG_MAX) / 2 / 220 + 500 ))
    mkdir -p /tmp/c/many
    python3 -c "import sys
for i in range(int(sys.argv[1])): open('/tmp/c/many/%s%06d.txt' % ('x' * 200, i), 'w').close()" "$n"
    rm -f /tmp/c-out/list.txt
    # (No Return here: on the list it would open the selected files.)
    key ctrl+l; typ /tmp/c/many; key Return; sleep 4; click 255 118; key ctrl+a ctrl+alt+l; sleep 1.5; shot commands-too-many
    key Escape; sleep 1.5
    check "commands: too many items for one command line: nothing runs" '[ ! -e /tmp/c-out/list.txt ]'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-commands.log && fail "commands: no panic" || pass "commands: no panic"
}
```
  `case`'e `paths) paths ;;` ve `commands) commands ;;`. Koş: `… gezik-linux bash scripts/linux/gui.sh paths` ve `… commands`, sonra `all` (6a'nın 176 maddesi düşmemeli). Düşen madde düzeltilir ve yeniden koşulur; sonuçlar notlara. Wayland'da yol tamamlama `wayland()` kipine eklenmez (girdi yolu X11'le aynı Slint kodu); denenmeyen diye notlara.
- [ ] **Step 4: Notlar** (Türkçe, 6a bölümünün kalıbında): ölçümler (öneri süresi, exe 6a'ya ve master'a göre, bellek), sapmalar (1-15 kısaca; özellikle `menu` başlıkları, `{files}`'ın Pause'u, Linux'ta major 0 aygıtların ağ sayılması, macOS Commands menüsünün durumu), `gui.sh paths`/`commands` sonuçları, Windows ekran testi sonuçları (aşağıdaki liste), denenemeyenler (macOS menü çubuğu Commands menüsü ve Go ▸ Clear Folder History; Wayland'da öneri listesi; gerçek ağ paylaşımında 1 sn sınırı Linux'ta).
  - `macos-test.md` "6b, keyboard" başlığıyla (sıradaki numaralar): (a) ⌘L, `/Us` → öneriler, ↓/Tab/Enter, Esc iki kez, `~/Desktop`, `$HOME/Downloads`; (b) boş adreste Recent/Frequent, yazarken "History", Go ▸ Clear Folder History; (c) `[[commands]]` `shortcut = "ctrl+alt+z"` → ⌃⌥Z seçimde çalışır, menü çubuğunda Commands ▸ başlıklı grup ve tuş metni; `ask = true` sorusu; `{files}` paneli "can't be undone"; (d) `[history] remember = false`.
  - `linux-test.md`: "Keys on Linux" tablosuna Address bar suggestions (↓ ↑ Tab → Enter Esc), Clear folder history (`clear-history`, no key), Command keys (`[[commands]] shortcut`); checklist'e aynı maddelerin Linux biçimi ve "Wayland'da öneri listesi" maddesi.
  - Spec Durum: "Tasarım onaylandı (2026-10-07); 6a ve 6b uygulandı".
- [ ] **Step 5: Commit** "Measure path suggestions and note what 6b does".

---

## Ekran testleri (Windows, alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici. Test verisi `%TEMP%\gezik-gui-6b\`: `Alpha\`, `alpine\`, `Beta\`, `İndirilenler\`, `Gone\`, `a.txt`, `b.txt`, `c.jpg`, `ş 'q' & x.txt`, `-dash.txt`; `many\` içinde 200 karakterlik adlarla 200 dosya (`(32.000 / 200) + 40`). 7-Zip kurulu (`7z` PATH'te ya da Gezik'in indirdiği). `settings.toml`:

```toml
[shortcuts]
clear-history = "ctrl+shift+h"

[[commands]]
name = "Zip together"
run = ["7z", "a", "-tzip", "together.zip", "{files}"]
folders = true
shortcut = "ctrl+alt+z"
menu = "Archives"
ask = true

[[commands]]
name = "Copy txt"
run = ["cmd", "/c", "copy", "{in}", "{out}"]
output = "{name}-copy.{ext}"
types = ["txt"]
shortcut = "ctrl+alt+k"
menu = "Archives"

[[commands]]
name = "Clash"
run = ["cmd", "/c", "exit"]
shortcut = "ctrl+f"
```

Klavye düzeni önce ABD, sonra Türkçe Q (madde 15).

1. Ctrl+L, Ctrl+A, `%TEMP%\gezik-gui-6b\al` → 80 ms sonra alanın altında `Alpha`, `alpine` (dosyalar yok); ↓ ↓ ↑ seçimi gezdirir; Tab → alan `…\Alpha\`, liste boş (Alpha boş); Enter → Alpha. Liste görünürken ekran donmaz.
2. `~\Desktop`, `%USERPROFILE%\Documents` → gider; `%NOPE%\x` → "Cannot open %NOPE%\x"; `~veli` → açılmaz.
3. `\\10.255.255.1\x\` yaz (erişilemeyen ağ yolu): yazmak takılmaz, liste ~1 sn sonra boş kalır; `y` ekle, sil → yeni bir bekleme ama arayüz akıcı; Görev Yöneticisi'nde Gezik'in iş parçacığı sayısı her tuşta artmaz (aynı klasör yeniden okunmaz).
4. Bağlantısı kopuk bir eşlenmiş sürücü (ör. `net use Z: \\10.255.255.1\x /persistent:no` başarısızsa var olan bir kopuk sürücü) `Z:\` → madde 3 gibi.
5. Ctrl+L, Ctrl+A, Backspace → "Recent" (son 5) ve "Frequent"; `ind` → `İndirilenler` önerisi; başka bir klasördeyken `alp` → altında "History" ve `alpine`.
6. `Gone`'a git, geri dön, Explorer'da `Gone`'u sil; boş adres listesini aç → en geç 1 sn'de `Gone` listeden ve `state.toml`'dan düşer. Bir ağ yolu (madde 3'teki bir payın daha önce ziyaret edilmiş bir klasörü) erişilemezken düşmez.
7. Ctrl+Shift+H → durum çubuğu "Folder history cleared", liste boş; `settings.toml`'a `[history] remember = false` → `state.toml`'da `[history]` yok, yeni ziyaretler kaydedilmez; satırı sil.
8. `a.txt` ve `b.txt` seçili → Ctrl+Alt+K → `a-copy.txt`, `b-copy.txt`; `c.jpg` seçili → Ctrl+Alt+K → durum çubuğu "Copy txt runs on .txt files only"; adres çubuğu yazıyorken Ctrl+Alt+K bir şey yapmaz (alan Türkçe Q'da AltGr karakterini yazar).
9. Hepsini seç → Ctrl+Alt+Z → "Run Zip together on N items?" → Cancel bir şey yapmaz; yeniden → Run → klasörde tek `together.zip` (içinde `ş 'q' & x.txt` ve `-dash.txt` dahil hepsi); panel satırı "Done · can't be undone"; sağ tık boşlukta "Undo" bu işi değil önceki kopyayı söyler.
10. `many\` içinde hepsini seç → Ctrl+Alt+Z → soru gelmez, durum çubuğu "Too many items for one run of Zip together", hiçbir şey çalışmaz.
11. Sağ tık (Explorer menüsü) → "Commands ▸": gri "Archives" başlığı, altında Zip together ve Copy txt (txt seçiliyken); başlık seçilemez. Convert… katmanında komutlar sorusuz Start ile çalışır.
12. Bildirim satırında `commands[3]: shortcut "ctrl+f" is already used by filter; the command has no key`; Ctrl+F süzgeci açar.
13. `[[commands]]`'ta `shortcut = "ctrl+alt+zz"` → uyarı "shortcut: unknown key"; komut menüde kalır.
14. Uzun süren bir `{files}` çalıştırması (Zip together, birkaç GB'lık bir klasör seçiliyken): Pause 7-Zip'i durdurmaz, iş biter ve satır "Done · can't be undone" der. Aynı işi yeniden başlatıp Cancel'a basınca 7-Zip süreci sonlanır (Görev Yöneticisi); yarım bir `together.zip` kalabilir, bu 7-Zip'in işidir.
15. Türkçe Q: adres çubuğunda AltGr+Q `@`, AltGr+8/9 `[ ]` yazılır; öneri listesi açıkken de; Ctrl+Alt+K (AltGr+K) listede komutu çalıştırır, alanda yazmaz.
16. Exe boyutu ve boşta bellek (Görev Yöneticisi, ≤ 7 MB), Task 9'un sayılarıyla.

---

## Self-review

- **Spec kapsamı:** §6.1 öneriler (alt klasörler, en çok 12, dosyalar yok) → Task 2 (`rank`, `MAX_SUGGESTIONS`) + Task 7; iş parçacığında okuma, 80 ms, yeni tuşun öncekini geçersiz kılması, 1 sn → Task 7 (`DEBOUNCE`, `READ_LIMIT`, `wanted`, `read_needed`; sapma 8); tuşlar ↑/↓, Tab/→, Enter, Esc → Task 7 (`path_key`; sapma 7); önce baştan, sonra içinde, harf duyarsız → Task 2; `~`, `%AD%`, `$AD`, `${AD}`, bilinmeyen kalır, `navigate_text` → Task 2 + Task 7 (sapma 13). §6.2 kayıt (yol, sayı, zaman, 200, en düşük düşer) → Task 1 + Task 8; puan → Task 1 (`score`); boş adres/`~`/kök: Recent 5, Frequent 7 → Task 2 (`shows_history`) + Task 8 (`history_rows`); yazarken geçmişten 5 → Task 8 (`with_history`); silinmişlerin düşmesi (yalnız yerel) → Task 8 (`gone_locally`, sapma 11); `remember = false`, `clear-history` → Task 4 + Task 8 (sapma 12); yazım `state.toml` yazıcısından → Task 8 (`update_state`). §7 `shortcut` (aynı çakışma kuralı, çakışmada uyarı) → Task 4 (`bind_command`, sapma 3) + Task 6; `menu` → Task 6 (sapma 1); `ask` → Task 6 (sapma 5); `{files}` (tek çalıştırma, ayrı argümanlar, yasak eşler, çalışma klasörü) → Task 3 + Task 5 (sapma 4); komut satırı sınırı → Task 3 + Task 5 + Task 6; geri alma yok ve panel notu → Task 5 (`Outcome::Nothing`) + Task 6 (`CANT_UNDO`); kısayolun `types`/`folders`'a uymayan seçimde durum çubuğunda neden yazması → Task 6 (`not_for_text`, sapma 2). §8.1 `[history] remember` → Task 4; `[[commands]]` alanları → Task 4 (şablon dahil). §8.2 `[history]` → Task 4. §8.3 `clear-history` (`Action`, şablon, macOS menü çubuğu, ulaşılabilirlik testi) → Task 4 + Task 8. §9 kod yapısı → dosya listeleri (`complete.rs` §9'da yok, saf öneri kuralları için eklendi; `history.rs` ve `nav.rs` §9'daki gibi). §10 birim testleri (puan ve sınır, ortam değişkeni iki biçimde ve bilinmeyen, `{files}` kuralları ve komut satırı sınırı, kısayol çakışmaları, eski ayar dosyaları), Linux kabı (yol tamamlama, komut kısayolu), Windows ekran testleri, macOS → Task 1-9 ve ekran listesi. §11 öneriler iş parçacığında, arayüze yalnız liste → Task 7; geçmiş 200, puan gösterim anında → Task 1 + Task 8.
- **Yer tutucu taraması:** Task 7'deki `suggestions` (kimlik) ve `history_list` (boş) bilerek öyle ve Task 8'in onları dolduracağı yazılı; Task 4'teki `Action::ClearHistory` kolu `false` döner, Task 8 doldurur. Task 6'nın macOS Commands menüsü için derleme hatasında ne yapılacağı açıkça yazılı (sapma 6). Gövdesi yazılı olmayan test yok.
- **Tip tutarlılığı:** Task 1'de `fold_text`, `Visit`, `FolderHistory::{from_visits, visits, is_empty, visit, remove, clear, recent, frequent, matching}`, `score`, `MAX_VISITS`. Task 2'de `expand_typed`, `split_typed` (→ `(String, String)`), `shows_history`, `rank`, `sort_names`, `MAX_SUGGESTIONS`. Task 3'te `CommandSpec.{shortcut, menu, ask}`, `FILES_ALONE`, `uses_files`, `expand_files_command`, `command_line_len`, `too_many_text`, `command_line_limit`. Task 4'te `HistorySettings`, `Settings.history`, `State.history: Vec<Visit>`, `Action::ClearHistory`, `KeyOwner`, `Shortcuts::{bind_command, command_for, command_chord}`. Task 5'te `ALL`, `plan_once`, `run_once`, `execute(cwd, args, run, stop_on_pause)`. Task 6'da `COMMAND_GROUP`, `Line`, `grouped`, `not_for_text`, `ask_text`, `bar_entries`, `run_by_index`, `run_checked`, `run_asked`, `fits_one_run`, `CANT_UNDO`, `with_note`, `Operations::set_note`, `keys::{command_for, command_chord, chord_label}`, `actions::run_command`, `menu_bar::set_commands`, `bar-commands`. Task 7'de `Row`, `home`, `expand`, `completion_target`, `folder_rows`, `PathKey`, `path_key`, `step`, `first_folder`, `accept_text`, `Read`, `read_needed`, `list_subfolders`, `PathBox::{new, reset, chord}`, `with_current`, `PathRow { title, detail, path, heading }`, `path-chosen(string)`, `resolve_typed` (`pub(crate)`). Task 8'de `Navigator::on_visited`, `is_visit`, `RECENT`, `FREQUENT`, `HISTORY_MATCHES`, `history_rows`, `with_history`, `gone_locally`, `set_settings`, `PathBox::new(window, nav, store, saved)`, `PathBox::forget`. Task 8'deki `PathBox::new` imzası Task 7'ninkini genişletir ve `main.rs` çağrısı orada güncellenir.
- **Review Focus:** beş maddenin her biri adı geçen görevde test olarak var: 1 → Task 7 `a_folder_is_read_once_at_a_time` + ekran 3-4; 2 → Task 3 ve Task 5 testleri + `gui.sh commands`; 3 → Task 4 iki testi; 4 → Task 1 `matching_ignores_case_and_turkish_i` ve Task 2'nin üç açma testi; 5 → Task 7 `keys_in_the_address_bar` + `gui.sh paths`.
