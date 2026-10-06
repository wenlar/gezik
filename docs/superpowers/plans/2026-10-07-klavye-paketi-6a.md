# Klavye Paketi 6a (Süzgeç, Seçim ve Sekmeler) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dosya listesini yazarak süzmek (desen dili, sekme başına süzgeç, kayıtlı süzgeçler, isteğe bağlı "yazınca süz" kipi); desenle seçmek, seçimi ters çevirmek, aynı türü seçmek ve son dosya işleminden önceki seçimi geri getirmek; sekmeler arasında numarayla geçmek, kapatılan sekmeyi geçmişiyle geri açmak, sekme seçiciyle bulmak ve sekme kilitlemek. Her yeni eylem `[shortcuts]` ile değiştirilebilir ve macOS menü çubuğunda görünür.

**Architecture:** Desen dili `gezik-core::pattern`'de saf ve ayırmasız (desen bir kez derlenir, ad başına ayırma yok). Seçim işlemleri (`invert`, `set_where`) `gezik-core::selection`'da; kapatılan sekme yığını ve kilit `gezik-core::nav::Tabs`'ta; süzgeç metni sekmenin geçmiş kaydındaki `ViewState`'te (`filter`). Görünüm katmanı (`crates/gezik/src/view`) gizli dosya süzgecinden ve sıralamadan sonraki tam listeyi (`ViewData::full`) tutar, süzgeçle daralmış `Listing`'i kurar; sıra numarasıyla çalışan her şey (seçim, harfle atlama, sürükle-bırak, menüler) değişmeden yalnız görünenler üzerinde çalışır. Kısayol ayrıştırıcısına numerik tuşlar (`num+` …) ve eylem başına birden çok bağlama gelir; Slint'in metninden ayırt edilemeyen tuşlar (numerik `+`, Ctrl+Shift+rakam) winit kancasında fiziksel tuş koduyla etiketlenir. Arayüz: dosya listesinin üstünde süzgeç çubuğu, var olan soru kutusunun canlı notlu biçimiyle desen penceresi, sekme seçici katmanı (5a/5b katman kalıbı).

**Tech Stack:** Rust 2024, Slint 1.18 (winit 0.30 kancası `slint::winit_030`), `toml`/`toml_edit` (var olan). Yeni crate yok.

**Spec:** `docs/superpowers/specs/2026-10-07-klavye-paketi-design.md` (bölüm 3, 4, 5, 8'in 6a kısmı, 9-11)

## Spec'ten sapmalar ve netleştirmeler

1. **Numerik tuşlar winit'in fiziksel tuş koduyla.** Slint 1.18'in winit arka ucu (`i-slint-backend-winit` `winitwindowadapter.rs`, `to_slint_key`) `Key::Character("+")`'ı konumuna bakmadan `"+"` metnine çevirir: numerik `+` ile ana satırdaki `+` Slint olayında ayırt edilemez. Ama `window.on_winit_window_event` kancası Slint'ten önce çalışır (aynı dosya, `window_event_filter`) ve `KeyEvent::physical_key`'i görür. Kanca her tuş basışında `KeyCode::{NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide}`'ı ve `Digit0..9`'u `keys::note_pressed` ile not eder; tuş işleyici `keys::take_pressed` ile alır ve `keys::chord_from_press` ile akoru kurar. winit kancası üç platformda aynıdır (Windows, macOS, Linux X11/Wayland), yani yedek yol gerekmez. Yine de **Ctrl+= ve Ctrl+- de varsayılandır** (spec §4: ikisi birden): numerik tuş takımı olmayan klavyeler ve macOS menü çubuğu için. macOS menü çubuğu numerik akoru çalamaz (Slint'e metin olarak gider); menü öğesi o durumda eylemi doğrudan çalıştırır (`actions::run`).
2. **Ctrl+Shift+1/2 aynı kancayla.** Görünüm kısayolları spec §5'teki gibi Ctrl+Shift+1/2'ye taşınır; ama winit'in mantıksal tuşu Shift'i uygular: ABD düzeninde Ctrl+Shift+1 `"!"`, Türkçe Q'da Ctrl+Shift+2 `"'"` gelir ve bugünkü ayrıştırıcıda akor olmaz. Fiziksel tuş `Digit*` ise ve Ctrl/⌘ + Shift basılı, Alt basılı değilse akorun tuşu o rakamdır. Alt'lı durum dokunulmaz: Windows'ta AltGr = Ctrl+Alt (Türkçe Q'da AltGr+7 `{`), metin alanına yazılan karakter bozulmaz.
3. **Eylem başına birden çok bağlama.** `select-pattern` (`mod+=`, `num+`) ve `deselect-pattern` (`mod+-`, `num-`) iki varsayılanla gelir. `[shortcuts]`'ta değer metin ya da metin listesidir (`select-pattern = ["num+", "ctrl+shift+0"]`); kullanıcı bağlaması o eylemin bütün varsayılanlarını değiştirir. `chord_for` ilk bağlamayı verir; varsayılanlarda çalınabilen (numerik olmayan) akor önce yazılır.
4. **`=` için Shift gereken düzenler.** Türkçe Q'da `=` Shift+0'dır: Ctrl+= orada Ctrl+Shift+0 olarak gelir (sapma 2'deki kural onu `ctrl+shift+0` yapar) ve `mod+=` ile eşleşmez. Numerik `+` çalışır; isteyen `select-pattern = ["num+", "ctrl+shift+0"]` yazar. Ctrl+- Türkçe Q'da Shift'siz (`-` tuşu) çalışır. Notlara ve `linux-test.md`/`macos-test.md`'ye yazılır.
5. **`/` sabit bir liste tuşu, `[shortcuts]` girdisi değil.** Liste odaktayken yazılan `/` (hangi düzende hangi tuşla gelirse gelsin) süzgeci açar: hiçbir dosya adında `/` olmaz, bu yüzden harfle atlamayla çakışmaz; Türkçe Q'da `/` Shift+7 olduğundan akor olarak eşleşmezdi. `filter` eyleminin bağlanabilir varsayılanı yalnız `mod+f`.
6. **Boş tuşlar doğrulandı:** bugün klavyeyle yakınlaştırma yok (yalnız ızgarada Ctrl+tekerlek, `View::zoom`); Ctrl+=, Ctrl+-, Ctrl+F, Ctrl+3…9, Ctrl+Shift+I/A/T hiçbir eyleme bağlı değil; Slint `TextInput` bunların hiçbirini kullanmaz (Ctrl+A/C/V/X/Z/Y, oklar, Backspace/Delete). `keys.rs`'teki "Ctrl+Shift+T yeni sekme değil" denetimi `reopen-tab` olarak güncellenir.
7. **`clear-history` 6b'ye.** Spec §8.3'teki eylem listesinden yalnız 6a'nınkiler bu planda.
8. **Süzgeç sekme başına, geçmiş kaydında.** `ViewState` yeni alan `filter: Option<String>` (`None`: çubuk kapalı). Sekme değişimi ve yenileme (dosya izleyici, işlem sonrası) `save_view`/`capture` ile korur; gezinmeyle **başka bir yere geçmek — geri/ileri dahil — süzgeçsiz açılır** (spec §3.1 "başka klasöre geçmek kapatır"; `navigation::view_to_show`). Kapatılıp geri açılan sekme süzgeciyle döner. `state.toml`'a yazılmaz.
9. **Süzgeç yalnız klasörlerde.** "This PC"de (`Listing::Drives`) `filter` eylemi açmaz, durum çubuğu "The filter works in folders" der.
10. **Hatalı desen:** yalnız iki hata var: tek başına `!` (`Type a name after "!"`) ve `/` (`Names can't contain "/"`). Hata varken liste son geçerli desenin sonucunu gösterir (yazarken `!` için zıplamaz); sayacın yerinde kırmızı hata metni, alanın altında kırmızı ince çizgi (spec'teki "ipucu" bu metindir; Slint'te araç ipucu yok).
11. **Türkçe harfler:** `i`, `İ`, `ı`, `I` dördü aynı harfe katlanır (spec "i/İ ayrımı yok"); geri kalanı `char::to_lowercase`'in ilk karakteri. Parçalar baştan ve sondan kırpılır (`*.jpg; *.png`). Boş desen her adı geçirir (desen penceresinde boş = görünen her şey).
12. **Desen penceresi var olan soru kutusu (`Dialog`)** ve onun yeni canlı not satırıyla (`dialog-note`, `dialog-edited`): başlık "Select by pattern"/"Deselect by pattern", düğmeler Select|Deselect / Cancel. Son desen `state.toml [selection] last-pattern` (öteki anahtarlar gibi kebab-case; spec'te `last_pattern`).
13. **`select-same-type` seçime ekler** (desenle seçme gibi); uzantı büyük/küçük harf duyarsız; uzantısız dosyalar birbirinin türüdür; odak bir klasördeyse bütün klasörler.
14. **`restore-selection`:** seçim, bir iş motora verilirken (`Operations::submit`/`submit_chain`) ve çöpe atma/silmede satırlar gizlenmeden önce (`Operations::hide`) klasörüyle ve adlarıyla saklanır; boş seçim öncekini ezmez. Geri getirme yalnız aynı klasördeyken, görünen adlar arasından; başka klasörde hiçbir şey yapmaz.
15. **Kilitli sekme:** başlığın önünde kilit simgesi, × gösterilmez, sağ tık menüsünde "Close" yerine yok ve "Unlock tab" var; kapatma girişiminde durum çubuğunda "This tab is locked"; "Close other tabs" kilitlileri bırakır ve "1 locked tab stays open" / "N locked tabs stay open" der. "Close other tabs"in kapattıkları yığına tek tek girer; Ctrl+Shift+T her basışta birini yerine geri koyar. Geri açılan sekme etkin olur.
16. **Sekme seçici** başlıkta ya da yolda eşleşeni gösterir (aynı desen dili); hatalı ya da boş desende hepsi listelenir; açılınca etkin sekmenin satırı seçili.
17. **Sayaç** "12 / 100,000" (binlik ayırıcı `preview::with_commas` ile, `pub` yapılır).
18. **Gizlenmiş adlar ad çakışmasında sayılır:** yerinde yeniden adlandırmanın "already exists" denetimi (`has_other_named`) ve toplu adlandırmanın `all_names`'i süzgeçten önceki tam listeye bakar.
19. **Performans:** 30 ms bütçesinin yarısı (≤ 15 ms) saf kısma (derleme + eşleme + 100.000 `Entry`'den eşleşenlerin kopyası) ayrılır ve `#[ignore]`lı bir testte sınanır; tamamı (model sıfırlama ve çizim dahil) Task 10'daki betikle süreç CPU zamanı olarak ölçülür. Spec §11'deki "önceki sonuçtan daraltma" yalnız ölçüm bütçeyi aşarsa eklenir (önce ölçüm).
20. **Dal:** `feat/keyboard-6a` (var), master `0eb16fd`'den; üzerinde yalnız spec commit'leri.

## Global Constraints

- Rust edition 2024, stable (1.99). Komutlar `D:\Work\gezik`'ten; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Dal `feat/keyboard-6a`; aynı depoda başka bir çalışma sürüyorsa ayrı bir çalışma ağacında.
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- Çapraz denetimler: `cargo check -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- Yeni bağımlılık yok. winit'e yalnız `slint::winit_030::winit` üzerinden erişilir (`unstable-winit-030` özelliği zaten açık). `gezik-core` bağımlılıksız kalır.
- 100.000 dosyalık klasörde süzgeç her tuş vuruşundan sonra ≤ 30 ms'de güncellenir (sürüm derlemesi). Desen bir kez derlenir; ad başına ayırma yok.
- Boşta bellek değişmez (≤ 7 MB); exe büyümesi 6a + 6b ≤ +0,5 MB (Task 10 6a'nın payını ölçer).
- Her yeni eylem `[shortcuts]` ile değiştirilebilir, şablonda yorum satırı olarak görünür ve macOS menü çubuğunda bir öğesi vardır.
- Süzgeç metni, kilitler ve kapatılan sekmeler `state.toml`'a yazılmaz. `state.toml` yazımı var olan yazıcı iş parçacığından (`ConfigStore::update_state`); kayıtlı süzgeçler `settings.toml`'a 5a'nın hazır ayar yolundan (`edit_settings`).
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Claude imzası yok.
- Yol birleştirme yalnız `Path::join`.

## Review Focus

1. **Gerçek klavye düzenlerinde kısayollar:** ABD ve Türkçe Q'da Ctrl+Shift+1/2 (`!`, `@`/`'` yazan tuşlar), numerik `+`/`-`/`/`, Alt+numerik `+` eylemlerine ulaşır; AltGr ile yazılan `{ [ ] }` metin alanlarında bozulmaz; ana satırdaki `+` (Shift+=) desen penceresini açmaz. → Task 4 testleri (`chord_from_press`), Task 10 Linux `gui.sh keyboard` (xdotool `KP_Add`, `ctrl+shift+1`).
2. **Süzgeç açıkken görünmeyen bir adla yeniden adlandırma:** `b.txt` süzgeçle gizliyken `a.txt`'yi F2 ile `b.txt` yapmak, süzgeçsizdeki gibi anında "already exists" der; toplu adlandırma da gizli adları çakışma sayar. → Task 6 testi (`name_taken`).
3. **Süzgecin ömrü:** dosya izleyicinin yenilemesi ve sekme değişimi süzgeci, sayacı ve görünen seçimi korur; başka klasöre gidip **geri gelmek** süzgeçsiz açar; Ctrl+Shift+T ile geri açılan sekme süzgeciyle döner. → Task 3 (`ViewState.filter` geçmişte kalır, yığında kalır) ve Task 6 (`view_to_show`) testleri.
4. **Türkçe adlar ve harf büyüklüğü:** `istanbul` → `İSTANBUL.txt`, `ILIK` → `ılık.doc`, `ŞEHİR` → `şehir.png`, `*.JPG` → `a.jpg`; `[1]` düz karakter. → Task 1 testleri.
5. **"Close other tabs" kilitli sekmelerle, sonra art arda Ctrl+Shift+T:** kilitliler açık kalır, kapatılanlar tek tek, eski yerlerine ve eski geçmişleriyle döner, 21. kapatılan unutulur. → Task 3 testleri.

---

### Task 1: `gezik-core::pattern` — desen dili

**Files:**
- Create: `crates/gezik-core/src/pattern.rs`, `crates/gezik-core/tests/pattern_alloc.rs`
- Modify: `crates/gezik-core/src/lib.rs` (`pub mod pattern;`)

**Interfaces:**
- Consumes: `gezik_core::Entry`.
- Produces:

```rust
pub struct Pattern { /* private */ }          // Debug, Clone, PartialEq, Eq, Default (default: lets every name through)
impl Pattern {
    pub fn compile(text: &str) -> Result<Pattern, String>;
    pub fn is_empty(&self) -> bool;
    pub fn matches(&self, name: &str) -> bool; // allocates nothing
}
pub fn matching_entries(entries: &[Entry], pattern: &Pattern) -> Vec<Entry>;
```

- [ ] **Step 1: Write the failing tests** (`pattern.rs`'in sonunda `#[cfg(test)] mod tests` ve ayrı ayırma testi)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn m(pattern: &str, name: &str) -> bool {
        Pattern::compile(pattern).unwrap().matches(name)
    }

    fn entry(name: &str) -> Entry {
        Entry { name: name.to_owned(), is_dir: false, size: 0, modified: None, created: None }
    }

    #[test]
    fn a_part_without_wildcards_matches_anywhere() {
        assert!(m("jpg", "a.jpg"));
        assert!(m("jpg", "jpg notes.txt"));
        assert!(m("tat", "Tatil 2024"));
        assert!(!m("tatil", "tati"));
    }

    #[test]
    fn wildcards_match_the_whole_name() {
        assert!(m("*.jpg", "a.jpg"));
        assert!(!m("*.jpg", "a.jpg.txt"), "with * the whole name must match");
        assert!(m("a?c", "abc") && !m("a?c", "ac") && !m("a?c", "abcd"));
        assert!(m("a*b*c", "aXbYbZc"));
        assert!(m("*a", "aaa") && !m("a*", ""));
        assert!(m("**.txt", "x.txt"), "two stars are one");
        assert!(m("*", "anything") && m("*", ""));
    }

    #[test]
    fn brackets_are_plain_characters() {
        assert!(m("[1]", "foto [1].jpg"));
        assert!(!m("[1]", "foto 1.jpg"));
        assert!(m("*[a-z]*", "x[a-z]y"));
    }

    #[test]
    fn parts_are_split_by_semicolons_and_trimmed() {
        let p = Pattern::compile("*.jpg; *.png ;;").unwrap();
        assert!(p.matches("a.jpg") && p.matches("b.PNG") && !p.matches("c.gif"));
        assert!(m(" tatil ", "Tatil.doc"));
    }

    #[test]
    fn a_bang_leaves_out() {
        assert!(m("!*.tmp", "a.txt") && !m("!*.tmp", "a.tmp"), "only exclusions: everything else");
        let p = Pattern::compile("*.jpg;!*thumb*").unwrap();
        assert!(p.matches("a.jpg") && !p.matches("a thumb.jpg") && !p.matches("a.png"));
        assert!(m("! tmp", "a.txt") && !m("! tmp", "tmp.txt"), "spaces after ! are trimmed");
        assert!(m("!!a", "b") && !m("!!a", "x!a"), "a second ! is a plain character");
    }

    #[test]
    fn case_and_turkish_i_are_ignored() {
        assert!(m("istanbul", "İSTANBUL.txt"));
        assert!(m("ISTANBUL", "istanbul.txt"));
        assert!(m("ılık", "ILIK.doc") && m("ILIK", "ılık.doc"));
        assert!(m("ŞEHİR", "şehir.png") && m("şehir", "ŞEHİR.png"));
        assert!(m("*.JPG", "a.jpg") && m("*.jpg", "B.JPG"));
        assert!(m("ğüöç", "ĞÜÖÇ"));
    }

    #[test]
    fn bad_patterns_say_why() {
        assert_eq!(Pattern::compile("!").unwrap_err(), "Type a name after \"!\"");
        assert_eq!(Pattern::compile("*.jpg; ! ").unwrap_err(), "Type a name after \"!\"");
        assert_eq!(Pattern::compile("a/b").unwrap_err(), "Names can't contain \"/\"");
    }

    #[test]
    fn an_empty_pattern_lets_everything_through() {
        for text in ["", "  ", ";", " ; ; "] {
            let p = Pattern::compile(text).unwrap();
            assert!(p.is_empty() && p.matches("a.txt") && p.matches(""), "{text:?}");
        }
        assert!(Pattern::default().matches("x"));
        assert!(!Pattern::compile("!x").unwrap().is_empty());
    }

    #[test]
    fn matching_entries_keeps_the_order() {
        let entries: Vec<Entry> = ["b.jpg", "a.txt", "c.JPG"].map(entry).into();
        let kept = matching_entries(&entries, &Pattern::compile("*.jpg").unwrap());
        assert_eq!(kept.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["b.jpg", "c.JPG"]);
    }

    /// The pure part's share of the 30 ms budget (plan sapma 19): compile, match and copy what
    /// passes, for 100,000 names. Run: `cargo test --release -p gezik-core pattern -- --ignored`.
    #[test]
    #[ignore = "timing; run in release with --ignored"]
    fn a_hundred_thousand_names_filter_within_budget() {
        let entries: Vec<Entry> =
            (0..100_000).map(|i| entry(&format!("IMG_{i:06} Tatil ş{}.jpg", i % 7))).collect();
        for text in ["i", "img_0", "img_01234", "*.jpg;*.png", "tatil;!*ş3.jpg", "zzz"] {
            let started = Instant::now();
            let pattern = Pattern::compile(text).unwrap();
            let kept = matching_entries(&entries, &pattern);
            let took = started.elapsed();
            assert!(took < Duration::from_millis(15), "{text}: {took:?} ({} kept)", kept.len());
        }
    }
}
```

`crates/gezik-core/tests/pattern_alloc.rs` (kendi ikili dosyası, tek test; sayan ayırıcı yalnız burada):

```rust
//! Matching a name allocates nothing (spec 3.2), counted by a global allocator in this test
//! binary only.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use gezik_core::pattern::Pattern;

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn matching_allocates_nothing() {
    let pattern = Pattern::compile("*.jpg; foto ; !*ş?.tmp; İstanbul").unwrap();
    let names: Vec<String> = (0..2000)
        .map(|i| match i % 4 {
            0 => format!("IMG_{i}.JPG"),
            1 => format!("Foto şehir {i}.png"),
            2 => format!("rapor ş{}.tmp", i % 10),
            _ => format!("ISTANBUL {i}.txt"),
        })
        .collect();
    let before = ALLOCATIONS.load(Ordering::SeqCst);
    let hits = names.iter().filter(|name| pattern.matches(name)).count();
    let after = ALLOCATIONS.load(Ordering::SeqCst);
    assert_eq!(after - before, 0, "matching {} names allocated", names.len());
    // `rapor ş5.tmp` passes no including part (and `!*ş?.tmp` leaves it out): 500 of 2000.
    assert_eq!(hits, 1500);
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core pattern` → FAIL (`pattern` modülü yok).

- [ ] **Step 3: Implement** `pattern.rs`:

```rust
//! The pattern language of the filter, the selection box and the tab picker (spec 3.2).
//!
//! A pattern is parts split by `;`; each part is trimmed and empty ones are skipped. A part
//! starting with `!` leaves out what it matches. A part without `*` or `?` matches anywhere in
//! the name; with them it must match the whole name (`*.jpg`: only names ending in `.jpg`).
//! `[` and `]` are plain characters. A name passes if it matches an including part (or there
//! is none) and no leaving-out part. Case is ignored, and so is the Turkish i/İ/ı/I. A
//! pattern is compiled once; matching a name allocates nothing.

use crate::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    /// One character, folded (see [`fold`]).
    Char(char),
    /// `?`: any one character.
    Any,
    /// `*`: any run of characters, none too.
    Star,
}

/// A compiled pattern. The default (like an empty text) lets every name through.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pattern {
    include: Vec<Vec<Token>>,
    exclude: Vec<Vec<Token>>,
}

impl Pattern {
    /// Compiles `text`. The errors are shown under the field as they are: a `!` with nothing
    /// after it, and a `/` (no name has one).
    pub fn compile(text: &str) -> Result<Pattern, String> {
        let mut pattern = Pattern::default();
        for part in text.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (leave_out, body) = match part.strip_prefix('!') {
                Some(rest) => (true, rest.trim_start()),
                None => (false, part),
            };
            if body.is_empty() {
                return Err("Type a name after \"!\"".to_owned());
            }
            if body.contains('/') {
                return Err("Names can't contain \"/\"".to_owned());
            }
            let tokens = tokens(body);
            if leave_out {
                pattern.exclude.push(tokens);
            } else {
                pattern.include.push(tokens);
            }
        }
        Ok(pattern)
    }

    /// Whether the pattern has no parts (it lets every name through).
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Whether `name` passes. Allocates nothing.
    pub fn matches(&self, name: &str) -> bool {
        (self.include.is_empty() || self.include.iter().any(|part| glob(part, name)))
            && !self.exclude.iter().any(|part| glob(part, name))
    }
}

/// The entries whose names `pattern` lets through, in their order.
pub fn matching_entries(entries: &[Entry], pattern: &Pattern) -> Vec<Entry> {
    entries.iter().filter(|entry| pattern.matches(&entry.name)).cloned().collect()
}

/// One part as tokens; without wildcards it is `*part*` (anywhere in the name).
fn tokens(body: &str) -> Vec<Token> {
    let wild = body.contains(['*', '?']);
    let mut out = Vec::with_capacity(body.len() + 2);
    if !wild {
        out.push(Token::Star);
    }
    out.extend(body.chars().map(|c| match c {
        '*' => Token::Star,
        '?' => Token::Any,
        c => Token::Char(fold(c)),
    }));
    if !wild {
        out.push(Token::Star);
    }
    // `**` is `*`.
    out.dedup_by(|a, b| *a == Token::Star && *b == Token::Star);
    out
}

/// A character as compared: lower case, with i, İ, ı and I all one letter.
fn fold(c: char) -> char {
    match c {
        'I' | 'İ' | 'ı' => 'i',
        c if c.is_ascii() => c.to_ascii_lowercase(),
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// Whether the whole of `name` matches `tokens`: one pass with a single back-track point for
/// the last `*` (the classic wildcard matcher), on byte offsets into `name`.
fn glob(tokens: &[Token], name: &str) -> bool {
    let (mut t, mut n) = (0, 0);
    // After the last `*`: the token after it and where in the name it was tried from.
    let mut back: Option<(usize, usize)> = None;
    loop {
        let c = name[n..].chars().next();
        match (tokens.get(t), c) {
            (Some(Token::Star), _) => {
                t += 1;
                back = Some((t, n));
                continue;
            }
            (Some(Token::Any), Some(c)) => {
                t += 1;
                n += c.len_utf8();
                continue;
            }
            (Some(Token::Char(want)), Some(c)) if *want == fold(c) => {
                t += 1;
                n += c.len_utf8();
                continue;
            }
            (None, None) => return true,
            _ => {}
        }
        // A mismatch: let the last `*` take one more character, or give up.
        let Some((after_star, from)) = back else { return false };
        let Some(skipped) = name[from..].chars().next() else { return false };
        let from = from + skipped.len_utf8();
        back = Some((after_star, from));
        t = after_star;
        n = from;
    }
}
```

`lib.rs`: `pub mod pattern;` (`batch`'ten sonra, alfabetik).

- [ ] **Step 4: Run.** `cargo test -p gezik-core` → PASS (ayırma testi dahil); `cargo test --release -p gezik-core pattern -- --ignored` → PASS (süreler notlara, Task 10).
- [ ] **Step 5: Commit** "Add the filter pattern language: wildcards, several parts, exclusions, Turkish i".

---

### Task 2: `Selection::invert` ve `Selection::set_where`

**Files:**
- Modify: `crates/gezik-core/src/selection.rs`

**Interfaces:**
- Produces:

```rust
impl Selection {
    /// Flips every entry; focus and anchor stay. Returns the rows that changed.
    pub fn invert(&mut self) -> Vec<Range<usize>>;
    /// Selects (`on`) or unselects each entry `matches` says yes to; the others, focus and
    /// anchor stay. Returns the rows that changed.
    pub fn set_where(&mut self, on: bool, matches: impl FnMut(usize) -> bool) -> Vec<Range<usize>>;
}
```

- [ ] **Step 1: Write the failing tests** (`selection.rs` testlerine)

```rust
#[test]
fn invert_flips_every_entry_and_reports_the_rows() {
    let mut s = Selection::from_indices(130, [0, 1, 129], Some(1));
    assert_eq!(s.invert(), [0..130]);
    assert_eq!(s.count(), 127);
    assert!(!s.is_selected(0) && s.is_selected(2) && !s.is_selected(129) && !s.is_selected(130));
    assert_eq!((s.focus(), s.anchor()), (Some(1), Some(1)), "focus and anchor stay");
    s.invert();
    assert_eq!(selected(&s), [0, 1, 129]);
    let mut full = Selection::new(128);
    full.select_all();
    full.invert();
    assert_eq!(full.count(), 0);
    assert!(Selection::new(0).invert().is_empty());
}

#[test]
fn set_where_adds_or_removes_only_the_matches() {
    let mut s = Selection::from_indices(10, [1], Some(1));
    assert_eq!(s.set_where(true, |i| i % 3 == 0), [0..1, 3..4, 6..7, 9..10]);
    assert_eq!(selected(&s), [0, 1, 3, 6, 9]);
    assert_eq!(s.set_where(false, |i| i < 4), [0..2, 3..4]);
    assert_eq!(selected(&s), [6, 9]);
    assert!(s.set_where(true, |i| i == 6).is_empty(), "already selected: nothing changes");
    assert_eq!((s.focus(), s.count()), (Some(1), 2));
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core selection` → FAIL.
- [ ] **Step 3: Implement** (`select_all`'ın yanına):

```rust
    /// Flips every entry; focus and anchor stay.
    pub fn invert(&mut self) -> Vec<Range<usize>> {
        self.change(|s| {
            for word in &mut s.bits {
                *word = !*word;
            }
            // Bits past the end stay clear.
            if !s.len.is_multiple_of(64)
                && let Some(last) = s.bits.last_mut()
            {
                *last &= (1u64 << (s.len % 64)) - 1;
            }
            s.count = s.len - s.count;
        })
    }

    /// Selects (`on`) or unselects each entry `matches` says yes to; the others, focus and
    /// anchor stay.
    pub fn set_where(&mut self, on: bool, mut matches: impl FnMut(usize) -> bool) -> Vec<Range<usize>> {
        self.change(|s| {
            for i in 0..s.len {
                if matches(i) {
                    s.set(i, on);
                }
            }
        })
    }
```

- [ ] **Step 4: Run.** `cargo test -p gezik-core` → PASS.
- [ ] **Step 5: Commit** "Invert the selection and select or unselect by a test".

---

### Task 3: `gezik-core::nav` — kapatılan sekmeler, kilit, süzgeç alanı

**Files:**
- Modify: `crates/gezik-core/src/nav.rs`, `crates/gezik/src/navigation.rs` (yeni `Closed::Locked` kolu, `ViewState` kurulumları), `crates/gezik/src/view/mod.rs` (`capture_capped` ve testlerdeki `ViewState { … }` kurulumları: `filter: None`)

**Interfaces:**
- Produces:

```rust
pub struct ViewState { pub selected: Vec<String>, pub focus: Option<String>, pub scroll: f32,
                       /// The filter bar's text (`None`: closed).
                       pub filter: Option<String> }
pub const MAX_CLOSED: usize = 20;
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedTab { pub index: usize, pub history: History }
pub enum Closed { Remaining, LastTab, /// The tab is locked: nothing closed.
                  Locked }
impl Tabs {
    pub fn close(&mut self, index: usize) -> Closed;          // a locked tab: Locked, unchanged
    pub fn close_others(&mut self, index: usize) -> usize;    // how many locked tabs stayed
    pub fn reopen(&mut self) -> Option<usize>;                // the reopened tab's index; it is active
    pub fn closed_count(&self) -> usize;
    pub fn is_locked(&self, index: usize) -> bool;
    pub fn set_locked(&mut self, index: usize, locked: bool);
}
```
- `navigation.rs`'te `pub const LOCKED_TAB: &str = "This tab is locked";` ve `pub fn locked_kept_text(n: usize) -> String` (`"1 locked tab stays open"`, `"3 locked tabs stay open"`).

- [ ] **Step 1: Write the failing tests** (`nav.rs` testleri; `view()` yardımcısına `filter: None` eklenir)

```rust
#[test]
fn a_closed_tab_comes_back_where_it_was_with_its_history() {
    let mut tabs = tabs_at(&["/a", "/b", "/c"]);
    tabs.activate(1);
    tabs.active_mut().navigate(p("/b/deeper"));
    tabs.active_mut().set_view(ViewState { filter: Some("*.jpg".into()), ..view("x", -30.0) });
    assert_eq!(tabs.close(1), Closed::Remaining);
    assert_eq!(locations(&tabs), [p("/a"), p("/c")]);
    assert_eq!(tabs.closed_count(), 1);
    assert_eq!(tabs.reopen(), Some(1));
    assert_eq!(locations(&tabs), [p("/a"), p("/b/deeper"), p("/c")]);
    assert_eq!(tabs.active_index(), 1, "the reopened tab is active");
    assert_eq!(tabs.active().view().filter.as_deref(), Some("*.jpg"));
    assert!(tabs.active_mut().back());
    assert_eq!(tabs.active().location(), &p("/b"));
    assert_eq!(tabs.reopen(), None, "nothing more to reopen");
}

#[test]
fn the_closed_list_keeps_the_last_twenty() {
    let mut tabs = Tabs::new(p("/0"));
    for i in 1..=25 {
        tabs.open(p(&format!("/{i}")), true);
    }
    for _ in 0..25 {
        assert_eq!(tabs.close(1), Closed::Remaining);
    }
    assert_eq!(tabs.closed_count(), MAX_CLOSED);
    assert_eq!(tabs.reopen(), Some(1));
    assert_eq!(tabs.active().location(), &p("/25"), "the newest first");
    for _ in 1..MAX_CLOSED {
        assert!(tabs.reopen().is_some());
    }
    assert_eq!(tabs.reopen(), None, "/1-/5 were forgotten");
    assert_eq!(tabs.len(), 21);
}

#[test]
fn a_locked_tab_stays_open() {
    let mut tabs = tabs_at(&["/a", "/b", "/c", "/d"]);
    tabs.set_locked(1, true);
    assert!(tabs.is_locked(1) && !tabs.is_locked(0) && !tabs.is_locked(99));
    assert_eq!(tabs.close(1), Closed::Locked);
    assert_eq!((tabs.len(), tabs.closed_count()), (4, 0));
    assert_eq!(tabs.close_others(2), 1, "one locked tab stayed");
    assert_eq!(locations(&tabs), [p("/b"), p("/c")]);
    assert_eq!(tabs.active().location(), &p("/c"), "the kept tab is active");
    // The last tab, locked, does not close the window either.
    tabs.close(1);
    assert_eq!(tabs.close(0), Closed::Locked);
    tabs.set_locked(0, false);
    assert_eq!(tabs.close(0), Closed::LastTab);
}

#[test]
fn tabs_closed_together_come_back_in_their_places() {
    let mut tabs = tabs_at(&["/a", "/b", "/c", "/d", "/e"]);
    tabs.set_locked(3, true);
    assert_eq!(tabs.close_others(1), 1);
    assert_eq!(locations(&tabs), [p("/b"), p("/d")]);
    assert_eq!(tabs.closed_count(), 3, "each closed tab is on the list");
    while tabs.reopen().is_some() {}
    assert_eq!(locations(&tabs), [p("/a"), p("/b"), p("/c"), p("/d"), p("/e")]);
}

#[test]
fn the_lock_follows_its_tab_and_a_copy_is_unlocked() {
    let mut tabs = tabs_at(&["/a", "/b", "/c"]);
    tabs.set_locked(0, true);
    tabs.move_tab(0, 2);
    assert!(tabs.is_locked(2) && !tabs.is_locked(0));
    let copy = tabs.duplicate(2);
    assert!(!tabs.is_locked(copy));
    tabs.set_locked(2, false);
    assert!(!tabs.is_locked(2));
}

#[test]
fn a_reopened_tab_whose_place_is_gone_goes_last() {
    let mut tabs = tabs_at(&["/a", "/b"]);
    tabs.closed.push(ClosedTab { index: 9, history: History::new(p("/z")) });
    assert_eq!(tabs.reopen(), Some(2));
    assert_eq!(locations(&tabs), [p("/a"), p("/b"), p("/z")]);
    let new_id = tabs.id(2).unwrap();
    assert!(tabs.id(0) != Some(new_id) && tabs.id(1) != Some(new_id), "a new id");
}

#[test]
fn each_place_keeps_its_filter_in_the_history() {
    let mut h = History::new(p("/a"));
    h.set_view(ViewState { filter: Some("jpg".into()), ..ViewState::default() });
    h.navigate(p("/b"));
    assert_eq!(h.view().filter, None, "a new place starts without one");
    h.back();
    assert_eq!(h.view().filter.as_deref(), Some("jpg"), "kept; the navigator decides whether to show it");
}
```

`navigation.rs` testine:

```rust
#[test]
fn locked_tabs_are_counted_in_words() {
    assert_eq!(locked_kept_text(1), "1 locked tab stays open");
    assert_eq!(locked_kept_text(3), "3 locked tabs stay open");
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core nav` → FAIL.
- [ ] **Step 3: Implement** `nav.rs`:
  - `ViewState`'e `filter` alanı (belge yorumu: "The filter bar's text (`None`: closed). Kept by a tab switch and a reload; the navigator shows a move to another place without it.").
  - `MAX_CLOSED`, `ClosedTab`, `Closed::Locked` (yukarıdaki imzalar).
  - `Tabs`'a `locked: Vec<u64>` (kilitli sekmelerin id'leri) ve `closed: Vec<ClosedTab>` (en yenisi sonda); `Tabs::new` ikisini boş başlatır.
  - Gövdeler:

```rust
    /// Closes tab `index` (out of range: no-op; locked: `Locked`, nothing closes). Closing the
    /// active tab activates its right neighbour, or the left one if it was last. The closed
    /// tab goes on the list `reopen` takes from.
    pub fn close(&mut self, index: usize) -> Closed {
        if index >= self.tabs.len() {
            return Closed::Remaining;
        }
        if self.is_locked(index) {
            return Closed::Locked;
        }
        if self.tabs.len() == 1 {
            return Closed::LastTab;
        }
        self.take_out(index);
        if index < self.active || (index == self.active && self.active == self.tabs.len()) {
            self.active -= 1;
        }
        Closed::Remaining
    }

    /// Closes every tab but `index` and the locked ones; `index` becomes active. Returns how
    /// many locked tabs stayed.
    pub fn close_others(&mut self, index: usize) -> usize {
        if index >= self.tabs.len() {
            return 0;
        }
        let keep = self.ids[index];
        let mut locked = 0;
        // From the right, so that each tab's index is still the one it had.
        for i in (0..self.tabs.len()).rev() {
            if i == index {
                continue;
            }
            if self.is_locked(i) {
                locked += 1;
            } else {
                self.take_out(i);
            }
        }
        self.active = self.index_of(keep).unwrap_or(0);
        locked
    }

    /// Takes tab `index` out onto the closed list (the oldest is forgotten past `MAX_CLOSED`).
    fn take_out(&mut self, index: usize) {
        let history = self.tabs.remove(index);
        self.ids.remove(index);
        self.closed.push(ClosedTab { index, history });
        if self.closed.len() > MAX_CLOSED {
            self.closed.remove(0);
        }
    }

    /// Opens the last closed tab again, with its history, where it was (at the end if there
    /// are fewer tabs now), as the active tab. Returns its index; `None` if none is left.
    pub fn reopen(&mut self) -> Option<usize> {
        let ClosedTab { index, history } = self.closed.pop()?;
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, history);
        let id = self.new_id();
        self.ids.insert(index, id);
        self.active = index;
        Some(index)
    }

    /// How many closed tabs `reopen` can bring back.
    pub fn closed_count(&self) -> usize {
        self.closed.len()
    }

    pub fn is_locked(&self, index: usize) -> bool {
        self.id(index).is_some_and(|id| self.locked.contains(&id))
    }

    /// Locks or unlocks tab `index` (out of range: no-op). A locked tab does not close.
    pub fn set_locked(&mut self, index: usize, locked: bool) {
        let Some(id) = self.id(index) else { return };
        self.locked.retain(|&l| l != id);
        if locked {
            self.locked.push(id);
        }
    }
```

  `navigation.rs`: `LOCKED_TAB` ve `locked_kept_text`; `close_tab` başta `if self.0.borrow().tabs.is_locked(index) { return self.status(LOCKED_TAB.to_owned()); }`, `match`'e `Closed::Locked => {}` (yukarıda yakalandı); `close_other_tabs` iki dalda `close_others`'ın dönüşünü alır, `> 0` ise `update_chrome`'dan sonra `self.status(locked_kept_text(n))`. `Navigator::new`'deki `ViewState { … }`'ye `filter: None`. `view/mod.rs`'te `capture_capped` şimdilik `filter: None` yazar (Task 6 doldurur); testlerdeki `ViewState { … }`'lere `filter: None`.
- [ ] **Step 4: Run.** `cargo test -p gezik-core -p gezik` → PASS.
- [ ] **Step 5: Commit** "Keep the last twenty closed tabs, lock tabs, and give each place a filter field".

---

### Task 4: Kısayollar — yeni eylemler, numerik tuşlar, taşınan görünüm kısayolları, menü çubuğu

**Files:**
- Create: `crates/gezik/src/actions.rs`
- Modify: `crates/gezik-config/src/shortcuts.rs`, `crates/gezik-config/templates/settings.toml` (`[shortcuts]` yorumları), `crates/gezik/src/keys.rs`, `crates/gezik/src/main.rs` (winit kancası, tuş işleyici, `mod actions;`, `handle_key` kolları), `crates/gezik/src/menu_bar.rs`, `crates/gezik/ui/app.slint` (MenuBar)

**Interfaces:**
- Consumes: `Navigator::{activate_tab, tab_count}` (var olan).
- Produces (`gezik-config::shortcuts`):

```rust
pub enum Key { Char(char) /* + '=' '-' */, /// A key of the numeric keypad: '+', '-', '*' or '/'.
               Num(char), F(u8), Left, … }
pub enum Action { …var olanlar…, Filter, InvertSelection, SelectPattern, DeselectPattern, SelectSameType,
                  RestoreSelection, Tab1, Tab2, Tab3, Tab4, Tab5, Tab6, Tab7, Tab8, TabLast, ReopenTab,
                  TabPicker, ToggleTabLock }
impl Action {
    pub const ALL: [Action; 45];
    pub fn tab_number(self) -> Option<usize>;              // Tab1 → 1 … Tab8 → 8
    fn default_texts(self, platform: Platform) -> &'static [&'static str];
}
// `[shortcuts]` values: "text" or ["text", …]
```
- Produces (`gezik::keys`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Physical { Numpad(char), Digit(char), Other }
pub fn physical_of(code: slint::winit_030::winit::keyboard::KeyCode) -> Physical;
pub fn note_pressed(physical: Physical);
pub fn take_pressed() -> Physical;
pub fn chord_from_press(text: &str, physical: Physical, control: bool, alt: bool, shift: bool, meta: bool,
                        platform: Platform) -> Option<Chord>;
```
- Produces (`gezik::actions`): `pub fn run(action: Action, nav: &Navigator, view: &View) -> bool` — the 6a actions; returns whether it ran one.

**Behavior:**
- `Key::parse`: `"num-"`, `"num*"`, `"num/"` → `Key::Num`; tek karakterlerde `'='` ve `'-'` de kabul. `parse_chord`: `num+` ayırıcıyla bittiği için bölmeden önce sondan alınır; değilse son `+`'dan bölünür. Hata metinleri öncekiyle aynı.

```rust
pub fn parse_chord(text: &str, platform: Platform) -> Result<Option<Chord>, String> {
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return Ok(None);
    }
    // "num+" ends in the separator: it is taken off before the rest is split.
    let (modifiers, key) = match text.strip_suffix("num+") {
        Some("") => ("", Key::Num('+')),
        Some(rest) if rest.ends_with('+') => (&rest[..rest.len() - 1], Key::Num('+')),
        _ => {
            let (modifiers, name) = text.rsplit_once('+').unwrap_or(("", text.as_str()));
            let name = name.trim();
            if name.is_empty() {
                return Err("missing key after \"+\"".to_owned());
            }
            (modifiers, Key::parse(name).ok_or_else(|| format!("unknown key \"{name}\""))?)
        }
    };
    let mut chord = Chord { ctrl: false, alt: false, shift: false, meta: false, key };
    if !modifiers.is_empty() {
        for modifier in modifiers.split('+').map(str::trim) {
            match modifier {
                "ctrl" => chord.ctrl = true,
                "alt" => chord.alt = true,
                "shift" => chord.shift = true,
                "mod" => match platform {
                    Platform::Mac => chord.meta = true,
                    Platform::Other => chord.ctrl = true,
                },
                other => return Err(format!("unknown modifier \"{other}\"")),
            }
        }
    }
    Ok(Some(chord))
}
```

- Yeni eylem adları: `filter`, `invert-selection`, `select-pattern`, `deselect-pattern`, `select-same-type`, `restore-selection`, `tab-1` … `tab-8`, `tab-last`, `reopen-tab`, `tab-picker`, `toggle-tab-lock`. `ALL` sırası: var olan 27, sonra bu sırayla 18.
- `default_text` → `default_texts`; var olanların hepsi tek elemanlı dilim (`&["mod+t"]`), `None` dönenler `&[]`; değişen ve yeni olanlar (iki platformda aynı):

```rust
            (Action::ViewList, _) => &["mod+shift+1"],
            (Action::ViewGrid, _) => &["mod+shift+2"],
            (Action::Filter, _) => &["mod+f"],
            (Action::InvertSelection, _) => &["mod+shift+i"],
            // The playable chord first: the macOS menu bar plays `chord_for`, the first binding.
            (Action::SelectPattern, _) => &["mod+=", "num+"],
            (Action::DeselectPattern, _) => &["mod+-", "num-"],
            (Action::SelectSameType, _) => &["alt+num+"],
            (Action::RestoreSelection, _) => &["num/"],
            (Action::Tab1, _) => &["mod+1"],
            (Action::Tab2, _) => &["mod+2"],
            (Action::Tab3, _) => &["mod+3"],
            (Action::Tab4, _) => &["mod+4"],
            (Action::Tab5, _) => &["mod+5"],
            (Action::Tab6, _) => &["mod+6"],
            (Action::Tab7, _) => &["mod+7"],
            (Action::Tab8, _) => &["mod+8"],
            (Action::TabLast, _) => &["mod+9"],
            (Action::ReopenTab, _) => &["mod+shift+t"],
            (Action::TabPicker, _) => &["mod+shift+a"],
            (Action::ToggleTabLock, _) => &[],
```

- `from_table`: değer metin ya da metin listesi; değilse `shortcuts.<ad>: expected text or a list of texts, got <değer>` uyarısı. Bir metin ayrıştırılamazsa eylem öncekiyle aynı biçimde uyarıyla varsayılanda kalır. `user: Vec<(Action, Vec<Chord>)>` (boş: kapalı). Kullanıcı çakışmasının metni öncekiyle aynı (`shortcuts.<a>: already used by <b>; <a> is disabled`), eylemin birden çok tuşu varsa sonu `; that key is left out`. Varsayılan çakışmasında aynı ayrım (`shortcuts: the default "<t>" of <a> is used by <b>; <a> is disabled` / `; that key is left out`).
- `keys.rs`: `chord_from_event`'in kabul ettiği karakterler `'='` ve `'-'` ile genişler; `slint_keys` `Key::Num(c)` için `c.to_string()` verir. Yeni:

```rust
/// What winit says the key being pressed is, where Slint's text cannot tell: the keypad's
/// + - * / type what the main keys type, and Ctrl+Shift+1 arrives as Ctrl+Shift+!.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Physical {
    Numpad(char),
    Digit(char),
    Other,
}

thread_local! {
    /// The physical key of the press Slint is about to hand to `key-event`: set by the winit
    /// hook (main.rs), which runs first, and taken by the key handler.
    static PRESSED: std::cell::Cell<Physical> = const { std::cell::Cell::new(Physical::Other) };
}

pub fn note_pressed(physical: Physical) {
    PRESSED.with(|p| p.set(physical));
}

/// The press's physical key, once (a key Slint makes up itself finds `Other`).
pub fn take_pressed() -> Physical {
    PRESSED.with(|p| p.replace(Physical::Other))
}

pub fn physical_of(code: slint::winit_030::winit::keyboard::KeyCode) -> Physical {
    use slint::winit_030::winit::keyboard::KeyCode as K;
    match code {
        K::NumpadAdd => Physical::Numpad('+'),
        K::NumpadSubtract => Physical::Numpad('-'),
        K::NumpadMultiply => Physical::Numpad('*'),
        K::NumpadDivide => Physical::Numpad('/'),
        K::Digit0 => Physical::Digit('0'),
        K::Digit1 => Physical::Digit('1'),
        K::Digit2 => Physical::Digit('2'),
        K::Digit3 => Physical::Digit('3'),
        K::Digit4 => Physical::Digit('4'),
        K::Digit5 => Physical::Digit('5'),
        K::Digit6 => Physical::Digit('6'),
        K::Digit7 => Physical::Digit('7'),
        K::Digit8 => Physical::Digit('8'),
        K::Digit9 => Physical::Digit('9'),
        _ => Physical::Other,
    }
}

/// The chord of a key press (see `chord_from_slint`), with what winit said the key was: the
/// keypad's + - * / are `Key::Num`; Ctrl (⌘) + Shift on a digit key is that digit, whatever
/// Shift types there (`!`, `'`). Not with Alt: AltGr (Ctrl+Alt on Windows) types characters.
pub fn chord_from_press(
    text: &str,
    physical: Physical,
    control: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    platform: Platform,
) -> Option<Chord> {
    // Slint's `control` is ⌘ on macOS (see `chord_from_slint`).
    let (ctrl, cmd) = match platform {
        Platform::Mac => (meta, control),
        Platform::Other => (control, meta),
    };
    match physical {
        Physical::Numpad(c) if typed_char(text) == Some(c) => {
            Some(Chord { ctrl, alt, shift, meta: cmd, key: Key::Num(c) })
        }
        Physical::Digit(d) if (ctrl || cmd) && shift && !alt => {
            Some(Chord { ctrl, alt, shift, meta: cmd, key: Key::Char(d) })
        }
        _ => chord_from_slint(text, control, alt, shift, meta, platform),
    }
}
```

- `main.rs`: winit kancasının başına (`RedrawRequested` erken dönüşünden önce):

```rust
            // The keypad's keys and Ctrl+Shift+digits, which Slint's text cannot tell apart
            // (keys.rs `Physical`): noted before Slint hands the key to `key-event`.
            if let winit::event::WindowEvent::KeyboardInput { event, .. } = event
                && event.state == winit::event::ElementState::Pressed
            {
                keys::note_pressed(match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(code) => keys::physical_of(code),
                    winit::keyboard::PhysicalKey::Unidentified(_) => keys::Physical::Other,
                });
            }
```
  `on_key_event`'te `let physical = keys::take_pressed();` ve `chord_from_slint` yerine `keys::chord_from_press(&text, physical, m.control, m.alt, m.shift, m.meta, Platform::current())`.
- `actions.rs` (bu görevde sekme numaraları; öteki kollar sonraki görevlerde doldurulur, o zamana kadar `false` döner ve tuş başka bir şeye geçer):

```rust
//! The keyboard package's actions (6a), run by their shortcuts (main.rs `handle_key`) and on
//! macOS by the menu bar when their chord cannot be played there (the keypad's) or there is none.

use gezik_config::shortcuts::Action;

use crate::navigation::Navigator;
use crate::view::View;

/// Runs `action` if it is one of 6a's; returns whether it ran.
pub fn run(action: Action, nav: &Navigator, view: &View) -> bool {
    let _ = view;
    if let Some(n) = action.tab_number() {
        // A tab that is not there: nothing (spec 5).
        nav.activate_tab(n - 1);
        return true;
    }
    match action {
        Action::TabLast => nav.activate_tab(nav.tab_count().saturating_sub(1)),
        // Task 6-9 fill these in.
        Action::Filter
        | Action::InvertSelection
        | Action::SelectPattern
        | Action::DeselectPattern
        | Action::SelectSameType
        | Action::RestoreSelection
        | Action::ReopenTab
        | Action::TabPicker
        | Action::ToggleTabLock => return false,
        _ => return false,
    }
    true
}
```
- `handle_key`'in `match action`'ına: `Action::Filter | Action::InvertSelection | Action::SelectPattern | Action::DeselectPattern | Action::SelectSameType | Action::RestoreSelection | Action::Tab1 | Action::Tab2 | Action::Tab3 | Action::Tab4 | Action::Tab5 | Action::Tab6 | Action::Tab7 | Action::Tab8 | Action::TabLast | Action::ReopenTab | Action::TabPicker | Action::ToggleTabLock => { if !actions::run(action, nav, view) { return false; } }`.
- macOS menü çubuğu (`app.slint`): "as List" `@keys(Control + Shift + "1")`, "as Grid" `@keys(Control + Shift + "2")`. Edit menüsünün sonuna ayırıcı ve: "Filter…" `@keys(Control + F)` (`filter`), "Invert Selection" `@keys(Control + Shift + I)`, "Select by Pattern…" `@keys(Control + "=")`, "Deselect by Pattern…" `@keys(Control + "-")`, "Select Same Type", "Restore Selection". Window menüsüne ayırıcı ve: "Reopen Closed Tab" `@keys(Control + Shift + T)`, "Show All Tabs…" `@keys(Control + Shift + A)` (`tab-picker`), "Lock Tab" (`toggle-tab-lock`), ayırıcı, "Tab 1" … "Tab 8" `@keys(Control + "1")` … `"8"`, "Last Tab" `@keys(Control + "9")`. (Kısayolların gösterimi bu `@keys`'ten; ayar dosyasında değiştirilmiş kısayol menüde eskisini gösterir — bugünkü durum.)
- `menu_bar.rs`: `keys::chord_for(action).filter(|c| !matches!(c.key, Key::Num(_)))` varsa (bugünkü gibi sıfır gecikmeyle) çalınır; yoksa önce `crate::actions::run(action, &nav, &view)`, o da `false` dönerse var olan `BatchRename`/`ToggleHidden` kolları.
- Şablon `[shortcuts]` yorumları: `view-list`/`view-grid` satırları `"mod+shift+1"`/`"mod+shift+2"`; sona (aynı sütun hizasıyla):

```toml
# filter = "mod+f"         # also "/" on the file list (not changeable)
# invert-selection = "mod+shift+i"
# select-pattern = ["mod+=", "num+"]     # num+ is the keypad's +; a list gives several keys
# deselect-pattern = ["mod+-", "num-"]
# select-same-type = "alt+num+"
# restore-selection = "num/"   # the selection before the last file operation
# tab-1 = "mod+1"          # tab-2 … tab-8 likewise
# tab-last = "mod+9"
# reopen-tab = "mod+shift+t"
# tab-picker = "mod+shift+a"
# toggle-tab-lock = ""     # a locked tab does not close
```

- [ ] **Step 1: Write the failing tests.** `shortcuts.rs`:

```rust
#[test]
fn parses_keypad_keys_and_symbols() {
    assert_eq!(chord("num+").key, Key::Num('+'));
    assert_eq!(chord("alt+num+"), Chord { ctrl: false, alt: true, shift: false, meta: false, key: Key::Num('+') });
    assert_eq!(chord("NUM-").key, Key::Num('-'));
    assert_eq!(chord("num*").key, Key::Num('*'));
    assert_eq!(chord("num/").key, Key::Num('/'));
    assert_eq!(chord("ctrl+=").key, key('='));
    assert_eq!(chord("ctrl+-").key, key('-'));
    for bad in ["num", "num+x", "ctrl+num", "num%", "ctrlnum+", "ctrl++", "ctrl++t"] {
        assert!(parse_chord(bad, Platform::Other).is_err(), "{bad:?}");
    }
}

#[test]
fn the_view_keys_moved_for_the_tab_numbers() {
    let other = Shortcuts::defaults(Platform::Other);
    assert_eq!(other.action_for(&chord("ctrl+1")), Some(Action::Tab1));
    assert_eq!(other.action_for(&chord("ctrl+8")), Some(Action::Tab8));
    assert_eq!(other.action_for(&chord("ctrl+9")), Some(Action::TabLast));
    assert_eq!(other.action_for(&chord("ctrl+shift+1")), Some(Action::ViewList));
    assert_eq!(other.action_for(&chord("ctrl+shift+2")), Some(Action::ViewGrid));
    assert_eq!(other.action_for(&chord("ctrl+shift+t")), Some(Action::ReopenTab));
    assert_eq!(other.action_for(&chord("ctrl+shift+a")), Some(Action::TabPicker));
    assert_eq!(other.action_for(&chord("ctrl+shift+i")), Some(Action::InvertSelection));
    assert_eq!(other.action_for(&chord("ctrl+f")), Some(Action::Filter));
    assert_eq!(other.action_for(&chord("num/")), Some(Action::RestoreSelection));
    assert_eq!(other.chord_for(Action::ToggleTabLock), None);
    let mac = Shortcuts::defaults(Platform::Mac);
    let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
    assert_eq!(mac.action_for(&mac_chord("mod+3")), Some(Action::Tab3));
    assert_eq!(mac.action_for(&mac_chord("mod+shift+1")), Some(Action::ViewList));
}

#[test]
fn an_old_hand_binding_of_ctrl_1_still_works() {
    let (s, warnings) = build("[shortcuts]\nview-list = \"mod+1\"\n");
    assert_eq!(s.action_for(&chord("ctrl+1")), Some(Action::ViewList));
    assert_eq!(s.action_for(&chord("ctrl+shift+1")), None, "the user's binding replaces the default");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("tab-1") && warnings[0].message.ends_with("tab-1 is disabled"), "{warnings:?}");
}

#[test]
fn an_action_can_have_several_keys() {
    let other = Shortcuts::defaults(Platform::Other);
    assert_eq!(other.action_for(&chord("num+")), Some(Action::SelectPattern));
    assert_eq!(other.action_for(&chord("ctrl+=")), Some(Action::SelectPattern));
    assert_eq!(other.action_for(&chord("alt+num+")), Some(Action::SelectSameType));
    assert_eq!(other.chord_for(Action::SelectPattern), Some(chord("ctrl+=")), "the playable one first");
    let (s, warnings) = build("[shortcuts]\nselect-pattern = [\"num+\", \"ctrl+shift+0\"]\ndeselect-pattern = \"\"\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(s.action_for(&chord("ctrl+shift+0")), Some(Action::SelectPattern));
    assert_eq!(s.action_for(&chord("ctrl+=")), None, "the list replaces both defaults");
    assert_eq!(s.action_for(&chord("num-")), None);
    let (_, warnings) = build("[shortcuts]\nfilter = [\"ctrl+f\", 3]\n");
    assert!(warnings[0].message.starts_with("shortcuts.filter: expected text or a list of texts"), "{warnings:?}");
    let (s, warnings) = build("[shortcuts]\nrefresh = \"num+\"\n");
    assert_eq!(s.action_for(&chord("num+")), Some(Action::Refresh));
    assert_eq!(s.action_for(&chord("ctrl+=")), Some(Action::SelectPattern), "its other key stays");
    assert!(warnings[0].message.ends_with("that key is left out"), "{warnings:?}");
}

#[test]
fn every_action_has_a_name_that_reads_back() {
    for action in Action::ALL {
        assert_eq!(Action::from_name(action.name()), Some(action), "{}", action.name());
    }
    assert_eq!(Action::Tab3.tab_number(), Some(3));
    assert_eq!(Action::Tab8.tab_number(), Some(8));
    assert_eq!(Action::TabLast.tab_number(), None);
}
```
  `defaults_cover_every_action` `default_texts`'in her metnine bakacak biçimde güncellenir. `invalid_binding_keeps_default_with_warning`'in ikinci uyarısı artık `shortcuts.back: expected text or a list of texts, got 5` ile başlar (`starts_with("shortcuts.back:")` aynen geçer).

  `keys.rs` (testler):

```rust
#[test]
fn the_keypad_keys_are_their_own() {
    let defaults = Shortcuts::defaults(Platform::Other);
    let press = |text, physical, alt, shift| chord_from_press(text, physical, false, alt, shift, false, Platform::Other);
    assert_eq!(press("+", Physical::Numpad('+'), false, false).unwrap().key, Key::Num('+'));
    assert_eq!(defaults.action_for(&press("+", Physical::Numpad('+'), true, false).unwrap()), Some(Action::SelectSameType));
    assert_eq!(defaults.action_for(&press("/", Physical::Numpad('/'), false, false).unwrap()), Some(Action::RestoreSelection));
    // The main row's + (Shift+= on a US layout) is no keypad key and no chord.
    assert_eq!(press("+", Physical::Other, false, true), None);
    // A keypad key whose text is something else goes by its text.
    assert_eq!(press("4", Physical::Numpad('+'), false, false).unwrap().key, Key::Char('4'));
    // macOS: ⌘ is Slint's `control`.
    let cmd_plus = chord_from_press("+", Physical::Numpad('+'), true, false, false, false, Platform::Mac).unwrap();
    assert!(cmd_plus.meta && !cmd_plus.ctrl && cmd_plus.key == Key::Num('+'));
}

#[test]
fn ctrl_shift_and_a_digit_is_the_digit_whatever_shift_types() {
    let defaults = Shortcuts::defaults(Platform::Other);
    // US: Shift+1 types !, Shift+2 @; Turkish Q: Shift+2 types '.
    for (text, digit, action) in [("!", '1', Action::ViewList), ("@", '2', Action::ViewGrid), ("'", '2', Action::ViewGrid)] {
        let got = chord_from_press(text, Physical::Digit(digit), true, false, true, false, Platform::Other).unwrap();
        assert_eq!(defaults.action_for(&got), Some(action), "{text}");
    }
    let cmd = chord_from_press("!", Physical::Digit('1'), true, false, true, false, Platform::Mac).unwrap();
    assert_eq!(Shortcuts::defaults(Platform::Mac).action_for(&cmd), Some(Action::ViewList));
    // AltGr (Ctrl+Alt on Windows) types what the layout says: Turkish Q AltGr+7 is `{`.
    assert_eq!(chord_from_press("{", Physical::Digit('7'), true, true, false, false, Platform::Other), None);
    // Plain Shift+1 types `!`: no chord.
    assert_eq!(chord_from_press("!", Physical::Digit('1'), false, false, true, false, Platform::Other), None);
}

#[test]
fn winit_key_codes_are_sorted_out() {
    use slint::winit_030::winit::keyboard::KeyCode;
    assert_eq!(physical_of(KeyCode::NumpadAdd), Physical::Numpad('+'));
    assert_eq!(physical_of(KeyCode::NumpadSubtract), Physical::Numpad('-'));
    assert_eq!(physical_of(KeyCode::NumpadMultiply), Physical::Numpad('*'));
    assert_eq!(physical_of(KeyCode::NumpadDivide), Physical::Numpad('/'));
    assert_eq!(physical_of(KeyCode::Digit0), Physical::Digit('0'));
    assert_eq!(physical_of(KeyCode::Digit9), Physical::Digit('9'));
    assert_eq!(physical_of(KeyCode::Numpad1), Physical::Other);
    assert_eq!(physical_of(KeyCode::KeyA), Physical::Other);
}

#[test]
fn a_noted_key_is_taken_once() {
    note_pressed(Physical::Numpad('+'));
    assert_eq!(take_pressed(), Physical::Numpad('+'));
    assert_eq!(take_pressed(), Physical::Other);
}
```
  `every_default_is_reachable_on_windows_and_linux` yeniden yazılır: `other_event` artık `(String, Physical, bool, bool, bool, bool)` verir — `Key::Num(c)` → `(c, Physical::Numpad(c))`; rakam + Shift → ABD düzeninde Shift'in yazdığı (`")!@#$%^&*("`'den) ve `Physical::Digit(d)`; Shift'siz rakam → `(d, Physical::Digit(d))`; öteki tuşlar eskisi gibi `Physical::Other`. Çağrı `chord_from_press`. Eylem tablosu: var olanlar aynen (yalnız `ViewList => "ctrl+shift+1"`, `ViewGrid => "ctrl+shift+2"`), yeniler `Filter => "ctrl+f"`, `InvertSelection => "ctrl+shift+i"`, `SelectPattern => "ctrl+="`, `DeselectPattern => "ctrl+-"`, `SelectSameType => "alt+num+"`, `RestoreSelection => "num/"`, `Tab1 => "ctrl+1"` … `Tab8 => "ctrl+8"`, `TabLast => "ctrl+9"`, `ReopenTab => "ctrl+shift+t"`, `TabPicker => "ctrl+shift+a"`, `PasteMove | Duplicate | BatchRename | ToggleTabLock => continue`. Döngüden sonra ikinci varsayılanlar: `"num+"` → `SelectPattern`, `"num-"` → `DeselectPattern`. Sondaki "Ctrl+Shift+T yeni sekme değil" denetimi `Some(Action::ReopenTab)` olur. `played_keys_are_the_chord` `Key::Num` akorlarını atlar (yorum: "the menu bar runs those itself, actions::run").
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-config shortcuts` ve `cargo test -p gezik keys` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod ve davranış; `the_template_reads_with_the_defaults` şablonla uyarısız kalmalı).
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler (macOS: `menu_bar.rs` ve `app.slint`'in MenuBar'ı derlenir). `cargo run -p gezik` ile iki sekme aç: Ctrl+1/Ctrl+2/Ctrl+9 geçer, Ctrl+Shift+2 ızgaraya, Ctrl+Shift+1 listeye döner.
- [ ] **Step 5: Commit** "Add the keyboard package's actions, keypad keys and several keys per action; move the view keys to Ctrl+Shift+1/2".

---

### Task 5: Ayarlar — `[keyboard]`, `[[filters]]`, durum `[selection]`

**Files:**
- Modify: `crates/gezik-config/src/settings.rs`, `crates/gezik-config/src/settings_edit.rs`, `crates/gezik-config/src/store.rs`, `crates/gezik-config/templates/settings.toml`

**Interfaces:**
- Consumes: `gezik_core::pattern::Pattern` (Task 1).
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Typing { #[default] Jump, Filter }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyboardSettings { pub typing: Typing }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFilter { pub name: String, pub pattern: String }
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelectionState { pub last_pattern: Option<String> }
// Settings: pub keyboard: KeyboardSettings, pub filters: Vec<SavedFilter>
// State: pub selection: SelectionState
pub(crate) fn parse_filter(value: &toml::Value) -> Result<SavedFilter, String>;
pub fn filter_to_toml(filter: &SavedFilter) -> toml::Table;
// settings_edit:
pub fn with_filters(text: &str, filters: &[SavedFilter]) -> Result<String, String>;
// store:
impl ConfigStore { pub fn save_filters(&self, filters: &[SavedFilter]) -> Result<(), Warning>; }
```

**Behavior:**
- `[keyboard] typing`: `"jump"` | `"filter"`; başka değer → `keyboard.typing: expected "jump" or "filter", got <değer>`; tablo değilse `keyboard: expected a table, got …` (öteki tablolar gibi).
- `parse_filter`:

```rust
pub(crate) fn parse_filter(value: &toml::Value) -> Result<SavedFilter, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    let name = table
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or("name is missing")?;
    let pattern = table.get("pattern").and_then(|v| v.as_str()).filter(|p| !p.trim().is_empty()).ok_or("pattern is missing")?;
    gezik_core::pattern::Pattern::compile(pattern).map_err(|err| format!("pattern: {err}"))?;
    Ok(SavedFilter { name: name.to_owned(), pattern: pattern.to_owned() })
}
```
  `Settings::parse`'ta `filters` dizisi `rename-presets` gibi okunur; geçerli ama adı daha önce gelmiş bir süzgeç `filters[N]: "<ad>" is already used; this one is left out` uyarısıyla atlanır; dizi değilse `filters: expected [[filters]] tables, got …`.
- `State`: `[selection] last-pattern` (boş metin yok sayılır); varsayılan yazılmaz.
- `settings_edit`: `with_rename_presets`'in gövdesi `fn with_tables(text: &str, key: &str, tables: Vec<toml::Table>, valid: impl Fn(&toml::Value) -> bool) -> Result<String, String>`'e taşınır (hata metni `format!("{key} must be [[{key}]] tables")`, öteki her şey aynen); `with_rename_presets` ve yeni `with_filters` onu çağırır. `with_rename_presets`'in var olan testleri değişmeden geçmeli.
- `ConfigStore::save_filters` → `edit_settings(|text| with_filters(text, filters))`.
- Şablon: `[tools]`'tan sonra, `[shortcuts]`'tan önce:

```toml
[keyboard]
# What typing a letter on the file list does: "jump" goes to the first name starting with
# what you typed; "filter" opens the filter (Ctrl+F, or / ) with it.
typing = "jump"           # jump | filter
```
  ve `[[rename-presets]]` örneğiyle `[[commands]]` açıklaması arasına (boş satırlarla ayrık):

```toml
# Saved filters, listed by the filter bar's ▾ button ("Save as…" there writes them here).
# The pattern is the filter's own: parts split by ";", * and ? as wildcards (a part without
# them matches anywhere in the name), "!" in front leaves out.
# [[filters]]
# name = "Pictures"
# pattern = "*.jpg;*.jpeg;*.png;*.heic;*.webp"
```

- [ ] **Step 1: Write the failing tests** (`settings.rs`, `settings_edit.rs`, `store.rs`)

```rust
#[test]
fn keyboard_typing_is_read() {
    let (settings, warnings) = parse("[keyboard]\ntyping = \"filter\"\n");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(settings.keyboard.typing, Typing::Filter);
    let (settings, warnings) = parse("[keyboard]\ntyping = \"search\"\n");
    assert_eq!(settings.keyboard.typing, Typing::Jump);
    assert!(warnings[0].message.starts_with("keyboard.typing: expected \"jump\" or \"filter\""), "{warnings:?}");
}

#[test]
fn saved_filters_are_read_and_bad_ones_warned() {
    let text = "[[filters]]\nname = \"Resimler\"\npattern = \"*.jpg;*.png\"\n\
                [[filters]]\nname = \"\"\npattern = \"x\"\n\
                [[filters]]\nname = \"Boş\"\npattern = \" \"\n\
                [[filters]]\nname = \"Kötü\"\npattern = \"a;!\"\n\
                [[filters]]\nname = \"Resimler\"\npattern = \"*.gif\"\n";
    let (settings, warnings) = parse(text);
    assert_eq!(settings.filters, [SavedFilter { name: "Resimler".into(), pattern: "*.jpg;*.png".into() }]);
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(messages, [
        "filters[2]: name is missing",
        "filters[3]: pattern is missing",
        "filters[4]: pattern: Type a name after \"!\"",
        "filters[5]: \"Resimler\" is already used; this one is left out",
    ]);
}

#[test]
fn selection_state_round_trips() {
    let state = State { selection: SelectionState { last_pattern: Some("*.jpg;!a*".into()) }, ..State::default() };
    assert_eq!(State::parse(&state.to_toml()), state);
    assert!(!State::default().to_toml().contains("selection"));
    assert_eq!(State::parse("[selection]\nlast-pattern = \"\"\n").selection, SelectionState::default());
}

#[test]
fn the_template_filter_example_reads_once_uncommented() {
    let template = include_str!("../templates/settings.toml");
    let start = template.find("# [[filters]]").expect("the template has a filter example");
    let example: String = template[start..]
        .lines()
        .take_while(|line| line.starts_with('#'))
        .map(|line| format!("{}\n", line.strip_prefix("# ").unwrap_or(line)))
        .collect();
    let (settings, warnings) = parse(&example);
    assert!(warnings.is_empty(), "{warnings:?}\n{example}");
    assert_eq!(settings.filters.len(), 1);
}
```
  `the_template_reads_with_the_defaults`'a `assert_eq!(settings.keyboard, KeyboardSettings::default()); assert!(settings.filters.is_empty());`.

  `settings_edit.rs`:

```rust
#[test]
fn filters_are_written_keeping_the_rest() {
    use crate::settings::{SavedFilter, Settings};
    let filter = SavedFilter { name: "Resimler".into(), pattern: "*.jpg;*.png".into() };
    let text = "# mine\ntheme = \"nord\"\n\n[[filters]]\nname = \"old\"\npattern = \"x\"\n\n[[filters]]\nname = \"broken\" # mine\n";
    let out = with_filters(text, std::slice::from_ref(&filter)).unwrap();
    assert!(out.contains("# mine") && out.contains("name = \"broken\" # mine"), "{out}");
    assert!(!out.contains("\"old\""), "a valid one is replaced: {out}");
    let mut warnings = Vec::new();
    let settings = Settings::parse("settings.toml", &out, &mut warnings);
    assert_eq!(settings.filters, [filter]);
    assert_eq!(warnings.len(), 1, "the broken one still warns: {warnings:?}");
}

#[test]
fn filters_and_presets_together_leave_the_template_as_it_was() {
    let template = include_str!("../templates/settings.toml");
    let filter = crate::settings::SavedFilter { name: "Resimler".into(), pattern: "*.jpg".into() };
    let with_both = with_rename_presets(&with_filters(template, &[filter]).unwrap(), &[tatil()]).unwrap();
    let mut warnings = Vec::new();
    let settings = crate::settings::Settings::parse("settings.toml", &with_both, &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!((settings.filters.len(), settings.rename_presets.len()), (1, 1));
    // Removing both keeps every comment of the template (the examples move, they are not lost).
    let back = with_rename_presets(&with_filters(&with_both, &[]).unwrap(), &[]).unwrap();
    let settings = crate::settings::Settings::parse("settings.toml", &back, &mut warnings);
    assert!(warnings.is_empty() && settings.filters.is_empty() && settings.rename_presets.is_empty(), "{back}");
    for line in template.lines().filter(|l| l.starts_with('#')) {
        assert!(back.lines().any(|b| b == line), "{line} lost:\n{back}");
    }
}
```
  `store.rs`: `save_filters` ile yazılan dosya yeniden okununca süzgeç gelir (`view_defaults_are_written_into_settings` kalıbı).
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-config` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki türler ve davranış; `filter_to_toml` `name` ve `pattern` yazar). `presets_go_after_the_template_comments` değişmeden geçmeli (yeni yorum bloğu `# Saved rule sets`'ten sonra).
- [ ] **Step 4: Run.** `cargo test -p gezik-config` → PASS; `cargo build --workspace`.
- [ ] **Step 5: Commit** "Read the typing mode and saved filters, and remember the last selection pattern".

---

### Task 6: Süzgeç — görünümde süzme, çubuk, Ctrl+F ve Esc

**Files:**
- Create: `crates/gezik/src/filter.rs`, `crates/gezik/ui/widgets/filter-bar.slint`
- Modify: `crates/gezik/src/view/mod.rs`, `crates/gezik/src/view/model.rs` (`ViewData::full`), `crates/gezik/src/view/listing.rs`, `crates/gezik/src/navigation.rs`, `crates/gezik/src/preview.rs` (`with_commas` → `pub fn`), `crates/gezik/src/actions.rs` (`Filter` kolu), `crates/gezik/src/main.rs` (`mod filter;`, `Filter::new`, `handle_key`), `crates/gezik/ui/app.slint`, `crates/gezik/ui/widgets/text-field.slint` (`focus-at`)

**Interfaces:**
- Consumes: Task 1 (`Pattern`, `matching_entries`), Task 3 (`ViewState.filter`).
- Produces:

```rust
// view/listing.rs
/// The listing of `dir` showing what `pattern` lets through of `full`: the same entries
/// (no copy) for an empty pattern.
pub fn filtered_listing(dir: &Path, full: &Rc<Vec<Entry>>, pattern: &Pattern) -> Listing;
/// Whether an entry of `entries` other than `except` is called `name` (ignoring case where
/// the file system does).
pub fn name_taken(entries: &[Entry], name: &str, except: &str) -> bool;

// view/mod.rs
/// The filter bar's text, the pattern the list shows, and what is wrong with the text.
struct FilterState { text: String, pattern: Pattern, error: Option<String> }
impl FilterState { fn new(text: &str, before: Option<&FilterState>) -> FilterState } // an error keeps `before`'s pattern
pub fn filter_count_text(shown: usize, total: usize) -> String;   // "12 / 100,000"
fn selection_after_filter(listing: &Listing, selected: &[String]) -> Selection; // focus on the first shown
impl View {
    pub fn set_filter(&self, text: Option<&str>);   // None closes the filter
    pub fn filter_text(&self) -> Option<String>;
}

// navigation.rs
/// The view to show once a load is done: a move to another place starts without the filter.
fn view_to_show(mode: &Mode, saved: &ViewState) -> ViewState;

// filter.rs
pub struct Filter;   // Filter::new(window: &AppWindow, view: View) -> Filter
pub fn with_current(f: impl FnOnce(&Filter));
impl Filter {
    pub fn open(&self);              // Ctrl+F: open empty and focus; an open bar: its text selected
    pub fn typed(&self, c: char);    // opens with `c`, or adds `c` to the open bar's text (Task 7 uses it)
    pub fn close(&self);             // Esc: whole folder, the list gets the keyboard
}
```
- Slint (`app.slint`): `in property <bool> filter-open; in-out property <string> filter-text; in property <string> filter-count; in property <string> filter-error; out property <bool> filter-focused; callback filter-edited(string); callback filter-menu(length, length, length, length);` `public function focus-filter(at: int)` (alanı odaklar, imleci `at` baytına koyar), `public function select-filter-text()`.
- `filter-bar.slint`: `export component FilterBar inherits Rectangle` — `in-out property <string> text; in property <string> count; in property <string> error; out property <bool> field-focused; callback edited(string); callback menu(length, length, length, length); public function focus-at(at: int); public function select-all();`. Tek satır: soldan büyüteç (çizilmiş `Path`, 5c'nin simge kalıbı), `TextField` (`edited` → `root.edited`), sağda `error != ""` ise `Theme.danger` renkli hata metni, değilse `count` (`Theme.foreground-muted`), en sağda "▾" `ToolButton` (Task 7'de menüyü açar; bu görevde `menu(...)` çağrısı yapılır, Rust tarafında boş geri çağrı). Hata varken alanın altında 1 px `Theme.danger` çizgi. `accessible-role: text-input` alanın kendi; çubuk `accessible-label: "Filter"`.
- `text-field.slint`: `public function focus-at(offset: int) { input.focus(); input.set-selection-offsets(offset, offset); }`.

**Behavior:**
- `ViewData::full: Rc<Vec<Entry>>`: gizli dosyalar çıkmış, sıralanmış tam liste (yalnız `Listing::Files`; "This PC" ve boş listede boş). `View` `Inner`'a `filter: RefCell<Option<FilterState>>`.
- `show(listing, state, note)`: gizli dosya süzgeci ve sıralamadan sonra `full`'e konur; `Listing::Files` ise ve `state.filter` `Some` ise `FilterState::new(text, None)` kurulur ve `filtered_listing`; `Listing::Drives` ise süzgeç `None`. Seçim `restore_selection(&shown, state)` (gizlenenler kendiliğinden düşer).
- `resort`: `full` sıralanır, sonra yeniden süzülür (seçim adlarla korunur, `capture_all`).
- `set_filter(Some(text))`: rename sürüyorsa `end_rename(false)`; `FilterState::new(text, önceki)`; hata yoksa ya da desen değiştiyse `filtered_listing`; seçim `selection_after_filter(&shown, &capture_all().selected)`, kaydırma 0, model sıfırlanır, `sync_focus`, `update_status`, dinleyiciler; sonra çubuk eşitlenir. `set_filter(None)`: tam liste, seçim `restore_selection` (odaktaki ad korunur) ve `reveal(odak)`.
- Çubuk eşitleme (`View`'in özel `sync_filter_bar(&window)`): `filter-open`, `filter-text` (yalnız farklıysa: yazarken imleç zıplamasın), `filter-count` (`filter_count_text(shown, full.len())`), `filter-error`. Çubuk kapanırken alan odaktaysa önce `invoke_focus_list`. `show` ve `clear` sonunda da çağrılır.
- `capture_capped` `filter: self.filter_text()` yazar.
- `hide_names` `full`'den de çıkarır. `has_other_named` ve `all_names` `full`'e bakar (`name_taken`), "This PC"de bugünkü gibi `listing`'e.
- `navigation.rs` `finish_load`: `(inner.view.clone(), view_to_show(&mode, inner.tabs.active().view()))`.
- `filter.rs`: `open`: görünüm "This PC" ise `view.note("The filter works in folders")`; çubuk açık ve alan odaktaysa `invoke_select_filter_text`; açıksa ama liste odaktaysa alanı odakla ve metni seç; kapalıysa `view.set_filter(Some(""))` ve sıfır gecikmeli zamanlayıcıda `invoke_focus_filter(0)` (çubuk görünür olduktan sonra odaklansın). `typed(c)`: açık değilse `set_filter(Some(&c.to_string()))`, açıksa metne ekler; alan odaklanır, imleç sona (bayt uzunluğu). `close`: `view.set_filter(None)`, `invoke_focus_list`. `window.on_filter_edited` → `view.set_filter(Some(&text))`.
- `app.slint` yerleşimi: `file-view`'in yerine `VerticalLayout { horizontal-stretch: 1; filter-bar := FilterBar { height: root.filter-open ? Theme.row-height + 8px : 0px; visible: root.filter-open; … } file-view := FileView { … } }` (her zaman var: Rust odaklayabilsin; ikisi de `keys` FocusScope'un içinde, kısayollar çalışır). `drop-geometry` `file-view`'den okunmaya devam eder.
- `handle_key` (`main.rs`), yol düzenleme bloğundan sonra:

```rust
    // The filter bar's field: Esc closes the filter, Down or Enter give the list the keyboard
    // (the bar stays); other plain keys and the text editing shortcuts are the field's.
    let filtering = window.get_filter_focused();
    if filtering && let Some(chord) = &chord {
        if !has_modifier && !chord.shift {
            match chord.key {
                Key::Escape => {
                    filter::with_current(filter::Filter::close);
                    return true;
                }
                Key::Down | Key::Enter => {
                    window.invoke_focus_list();
                    return true;
                }
                _ => {}
            }
        }
        if !has_modifier || keys::is_text_edit(chord, Platform::current()) {
            return false;
        }
    }
```
  `acts_on_files` denetimi `editing || filtering || …` olur; eylemden sonra odak listeye dönüşü `(editing || filtering) && !matches!(action, Action::FocusPath | Action::Filter)`; `Action::Filter` yol düzenlenirken `window.set_path_editing(false)` ile başlar. Listede Esc: `view.filter_text().is_some()` ise `Filter::close`, değilse seçimi temizler.
- `actions.rs`: `Action::Filter => filter::with_current(filter::Filter::open)` (kol `false` listesinden çıkar).

- [ ] **Step 1: Write the failing tests**

`view/listing.rs`:

```rust
#[test]
fn a_filtered_listing_shares_the_entries_without_a_pattern() {
    let Listing::Files(_, full) = files("/x", &["a.jpg", "b.txt", "c.JPG"]) else { unreachable!() };
    let all = filtered_listing(Path::new("/x"), &full, &Pattern::default());
    assert!(matches!(&all, Listing::Files(_, shown) if Rc::ptr_eq(shown, &full)), "no copy");
    let jpgs = filtered_listing(Path::new("/x"), &full, &Pattern::compile("*.jpg").unwrap());
    let names: Vec<&str> = (0..jpgs.len()).filter_map(|i| jpgs.name_at(i)).collect();
    assert_eq!(names, ["a.jpg", "c.JPG"]);
    assert_eq!(jpgs.folder(), Some(Path::new("/x")));
}

#[test]
fn a_hidden_name_is_still_taken() {
    let Listing::Files(_, full) = files("/x", &["a.txt", "b.txt"]) else { unreachable!() };
    assert!(name_taken(&full, "b.txt", "a.txt"), "b.txt is filtered out, but it is there");
    assert!(!name_taken(&full, "a.txt", "a.txt"), "its own name");
    assert!(!name_taken(&full, "c.txt", "a.txt"));
    assert_eq!(name_taken(&full, "B.TXT", "a.txt"), cfg!(any(windows, target_os = "macos")));
}
```

`view/mod.rs`:

```rust
#[test]
fn a_bad_filter_keeps_the_last_good_pattern() {
    let first = FilterState::new("*.jpg", None);
    assert_eq!(first.error, None);
    let bad = FilterState::new("*.jpg;!", Some(&first));
    assert_eq!(bad.error.as_deref(), Some("Type a name after \"!\""));
    assert_eq!(bad.pattern, first.pattern, "the list keeps showing the jpgs");
    assert_eq!(bad.text, "*.jpg;!");
    assert!(FilterState::new("!", None).pattern.is_empty(), "nothing good before: everything shows");
}

#[test]
fn the_counter_reads_well() {
    assert_eq!(filter_count_text(12, 340), "12 / 340");
    assert_eq!(filter_count_text(1234, 100_000), "1,234 / 100,000");
}

#[test]
fn after_a_filter_change_the_hidden_are_unselected_and_the_focus_is_first() {
    let shown = files("/x", &["b.jpg", "d.jpg"]);
    let selection = selection_after_filter(&shown, &["a.txt".into(), "d.jpg".into()]);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [1]);
    assert_eq!(selection.focus(), Some(0));
    assert_eq!(selection_after_filter(&files("/x", &[]), &[]).focus(), None);
}
```

`navigation.rs`:

```rust
#[test]
fn a_move_shows_the_place_without_its_filter_and_a_reload_keeps_it() {
    let saved = ViewState { filter: Some("*.jpg".into()), ..ViewState::default() };
    assert_eq!(view_to_show(&Mode::Show, &saved).filter.as_deref(), Some("*.jpg"), "reload, tab switch");
    for mode in moves() {
        assert_eq!(view_to_show(&mode, &saved).filter, None, "{mode:?}");
    }
}
```
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki davranış; Slint özellikleri ve `FilterBar`).
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler. `cargo run -p gezik` ile kısa bakış: Ctrl+F, `jpg` yaz (liste daralır, sayaç "2 / 6"), ↓ (liste klavyeyi alır), Ctrl+A ve Delete yalnız görünenleri çöpe atar, Ctrl+Z; `!` yazınca kırmızı çizgi ve metin, liste yerinde; Esc listede süzgeci kapatır, ikinci Esc seçimi temizler; sekme değiştir ve geri gel (süzgeç yerinde); klasöre gir ve geri dön (süzgeçsiz).
- [ ] **Step 5: Commit** "Filter the file list as you type: per tab, kept by reloads, hidden names still count for renames".

---

### Task 7: Yazma kipi, `/` ve kayıtlı süzgeçler

**Files:**
- Modify: `crates/gezik/src/filter.rs`, `crates/gezik/src/context_menu.rs`, `crates/gezik/src/main.rs` (`apply_config`, `Filter::new`'in yeni argümanları, `on_filter_menu`, `handle_key` sonu)

**Interfaces:**
- Consumes: Task 5 (`Typing`, `KeyboardSettings`, `SavedFilter`, `ConfigStore::save_filters`), Task 6 (`Filter`, `View::set_filter`, `View::filter_text`).
- Produces:

```rust
// filter.rs
pub fn set_settings(keyboard: KeyboardSettings, filters: Vec<SavedFilter>);  // apply_config, every resolve
pub fn typing() -> Typing;
pub fn saved() -> Vec<SavedFilter>;
impl Filter {
    // Filter::new(window: &AppWindow, view: View, dialogs: Dialogs, store: Option<ConfigStore>) -> Filter
    pub fn apply_saved(&self, name: &str);   // the saved pattern into the bar (opening it), applied
    pub fn ask_save(&self);                  // "Save filter" box: a name; the same name replaces
    pub fn delete_saved(&self, name: &str);
}
// context_menu.rs
pub const LOCK_TAB: u32 = 11;          // Task 9 uses them; reserved here with the filter ids
pub const UNLOCK_TAB: u32 = 12;
pub const FILTER_FIRST: u32 = 900;
pub const FILTER_MAX: u32 = 30;
pub const FILTER_SAVE: u32 = 930;
pub const FILTER_DELETE_FIRST: u32 = 940;
/// The filter bar's ▾ menu: each saved filter, "Save as…" (only with a good, non-empty text), then a Delete item for each.
pub fn filter_items(names: &[String], can_save: bool) -> Vec<(u32, String, bool)>;
impl Menus { pub fn filter_menu(&self, at: Anchor); }   // Subject::Filter(Vec<String>)
```

**Behavior:**
- `handle_key`'in sonu (harfle atlamadan önce):

```rust
    let Some(c) = keys::typed_char(text) else { return false };
    // `/` is in no name: it opens the filter whatever the typing mode (and on a layout where
    // it needs Shift, as no shortcut could).
    if c == '/' {
        filter::with_current(filter::Filter::open);
        return true;
    }
    if filter::typing() == Typing::Filter && !view.shows_drives() {
        filter::with_current(|f| f.typed(c));
        return true;
    }
```
- `apply_config`: `filter::set_settings(loaded.settings.keyboard, loaded.settings.filters.clone());`.
- `▾` menüsü: `window.on_filter_menu(move |l, b, r, t| menus.filter_menu(popup::Anchor::below(l, t, r, b)))`. `Menus::filter_menu`: `names = filter::saved()` adları, `can_save` = çubuk metni boş değil ve hatasız; `Subject::Filter(names)`; `open_slint_entries(&filter_items(..), None, at)`. `run`: `FILTER_FIRST..+FILTER_MAX` → `apply_saved(name)`, `FILTER_SAVE` → `ask_save()`, `FILTER_DELETE_FIRST..+FILTER_MAX` → `delete_saved(name)` (adla: ayar dosyası menü açıkken yeniden okunmuş olabilir; 5a'daki gibi).
- `ask_save`: `dialogs.ask_text("Save filter", "Name for this filter:", "", &["Save", "Cancel"], …)`; ad kırpılır, boşsa bir şey yapılmaz; aynı ad varsa deseni değişir, yoksa sona eklenir; yerel liste hemen güncellenir, `store.save_filters`; hata `view.note(warning.to_string())`.
- `apply_saved`: deseni çubuğa yazar (çubuk kapalıysa açar), `view.set_filter(Some(pattern))`; klavye listede kalır (menü kapanınca liste alır).
- Yazma kipi `filter` iken Space bugünkü gibi hızlı bakış (harf değil); harfle atlama yalnız `jump`'ta.

- [ ] **Step 1: Write the failing tests** (`context_menu.rs`)

```rust
#[test]
fn the_filter_menu_lists_saves_and_deletes() {
    let names = vec!["Resimler".to_owned(), "Belgeler".to_owned()];
    let items = filter_items(&names, true);
    let ids: Vec<u32> = items.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(ids, [FILTER_FIRST, FILTER_FIRST + 1, FILTER_SAVE, FILTER_DELETE_FIRST, FILTER_DELETE_FIRST + 1]);
    assert_eq!(items[2], (FILTER_SAVE, "Save as…".to_owned(), true));
    assert_eq!(items[3].1, "Delete \"Resimler\"");
    assert!(!filter_items(&[], false)[0].2, "nothing to save: greyed");
}
```
  `conversion_ids_meet_no_others`'ın `singles`'ına `LOCK_TAB, UNLOCK_TAB, FILTER_SAVE`, `ranges`'ına `FILTER_FIRST..FILTER_FIRST + FILTER_MAX` ve `FILTER_DELETE_FIRST..FILTER_DELETE_FIRST + FILTER_MAX`; `drop_menu_ids_are_their_own`'ın `others`'ına `LOCK_TAB, UNLOCK_TAB`.
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik context_menu` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS. `cargo run -p gezik` ile: `/` süzgeci açar; `GEZIK_CONFIG_DIR` geçici ve `[keyboard] typing = "filter"` ile harf yazmak süzgeci o harfle açar; ▾ → Save as… → "Resimler" → `settings.toml`'da `[[filters]]`; Gezik'i yeniden aç, ▾ → Resimler; Delete.
- [ ] **Step 5: Commit** "Open the filter by typing or with /, and save filters in settings.toml".

---

### Task 8: Seçim araçları — desen penceresi, ters çevirme, aynı tür, seçimi geri getirme

**Files:**
- Create: `crates/gezik/src/select_tools.rs`
- Modify: `crates/gezik/src/dialog.rs`, `crates/gezik/ui/widgets/dialog.slint`, `crates/gezik/ui/app.slint`, `crates/gezik/src/view/mod.rs`, `crates/gezik/src/view/listing.rs`, `crates/gezik/src/operations.rs` (`submit`, `submit_chain`, `hide`), `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs` (`mod select_tools;`, `SelectTools::new`)

**Interfaces:**
- Consumes: Task 1 (`Pattern`), Task 2 (`Selection::invert`, `set_where`), Task 5 (`SelectionState`), Task 6 (`ViewData::full`).
- Produces:

```rust
// dialog.rs
impl Dialogs {
    /// Like `ask_text`, with a line under the field that `note` writes as the text changes:
    /// `(line, is_error)`.
    pub fn ask_text_noted(&self, title: impl Into<String>, message: impl Into<String>, initial: impl Into<String>,
                          buttons: &[&str], note: impl Fn(&str) -> (String, bool) + 'static,
                          answer: impl FnOnce(Option<String>) + 'static);
}
// listing.rs
impl Listing { pub fn is_same_type(&self, a: usize, b: usize) -> bool; }
// view/mod.rs
impl View {
    pub fn invert_selection(&self);
    pub fn count_matching(&self, pattern: &Pattern) -> usize;
    pub fn select_matching(&self, pattern: &Pattern, on: bool);
    pub fn select_same_type(&self);        // the focused entry's type, added; nothing without a focus
    pub fn remember_selection(&self);      // folder + selected names; an empty selection keeps the last
    pub fn restore_remembered(&self);      // only in that folder, among the names shown
}
// select_tools.rs
pub struct SelectTools;   // SelectTools::new(view: View, dialogs: Dialogs, store: Option<ConfigStore>, last: Option<String>)
pub fn with_current(f: impl FnOnce(&SelectTools));
impl SelectTools { pub fn ask(&self, select: bool); }
/// The line under the pattern field: how many shown items match, or what is wrong.
pub fn match_note(text: &str, count: impl Fn(&Pattern) -> usize) -> (String, bool);
```
- Slint: `dialog.slint`'e `in property <string> note; in property <bool> note-error; callback edited(string);` (alanın `edited` → `root.edited`; alanın altında `note != ""` iken metin, `note-error` ise `Theme.danger`, değilse `Theme.foreground-muted`); `app.slint`'e `in property <string> dialog-note; in property <bool> dialog-note-error; callback dialog-edited(string);`.

**Behavior:**
- `Dialogs`: `Question`'a `note: Option<Rc<dyn Fn(&str) -> (String, bool)>>`; gösterilirken notu ilk metinle yazar, notsuz soruda notu boşaltır; `on_dialog_edited` açık sorunun not işlevini çağırıp `dialog-note`/`dialog-note-error`'ı yazar.
- `SelectTools::ask(select)`: başlık `Select by pattern` / `Deselect by pattern`, ileti `Names that match (* and ? are wildcards, ; separates patterns, ! leaves out):`, ilk metin son desen, düğmeler `["Select", "Cancel"]` / `["Deselect", "Cancel"]`, not `match_note(text, |p| view.count_matching(p))`. Yanıt: derlenirse `view.select_matching(&p, select)`, son desen bellekte ve `store.update_state(|s| s.selection.last_pattern = Some(text))`; derlenmezse `view.note(err)`.
- `count_matching`/`select_matching`: yalnız görünen liste (`listing`), adlarla; `select_matching` `selection.set_where(on, |i| pattern.matches(name(i)))`.
- `invert_selection`: `selection.invert()` → `after_selection`.
- `select_same_type`: odak yoksa bir şey yapmaz; `set_where(true, |i| listing.is_same_type(focus, i))`.
- `Listing::is_same_type(a, b)`: ikisi de klasör → doğru; biri klasör → yanlış; dosyalar → `Entry::extension()` büyük/küçük harf duyarsız eşit (ikisi de uzantısızsa doğru); "This PC"de sürücüler hep aynı tür.
- `remember_selection`: `Inner.remembered: RefCell<Option<(PathBuf, Vec<String>)>>`; klasör yoksa ("This PC") ya da seçim boşsa dokunmaz. `Operations::submit` ve `submit_chain` ilk satırda, `Operations::hide` `hide_names`'ten önce çağırır.
- `restore_remembered`: `folder()` hatırlananla `same_path` ise seçim `Selection::from_indices(len, listing.indices_of(&names), ilk)`, `after_selection`, `reveal(odak)`; değilse bir şey yapmaz.
- `actions.rs`: `InvertSelection => view.invert_selection()`, `SelectSameType => view.select_same_type()`, `RestoreSelection => view.restore_remembered()`, `SelectPattern => select_tools::with_current(|s| s.ask(true))`, `DeselectPattern => select_tools::with_current(|s| s.ask(false))` (kollar `false` listesinden çıkar).

- [ ] **Step 1: Write the failing tests**

`select_tools.rs`:

```rust
#[test]
fn the_note_says_how_many_match_or_what_is_wrong() {
    let names = ["a.jpg", "b.JPG", "c.png"];
    let count = |p: &Pattern| names.iter().filter(|n| p.matches(n)).count();
    assert_eq!(match_note("*.jpg", count), ("2 items match".to_owned(), false));
    assert_eq!(match_note("*.png", count), ("1 item matches".to_owned(), false));
    assert_eq!(match_note("*.gif", count), ("No items match".to_owned(), false));
    assert_eq!(match_note("", count), ("3 items match".to_owned(), false), "empty: everything shown");
    assert_eq!(match_note("!", count), ("Type a name after \"!\"".to_owned(), true));
    let many = |_: &Pattern| 12_345;
    assert_eq!(match_note("x", many).0, "12,345 items match");
}
```

`view/listing.rs`:

```rust
#[test]
fn same_type_goes_by_the_ending_or_folders() {
    let listing = files("/x", &["sub/", "other/", "a.JPG", "b.jpg", "c.png", "README", "LICENSE"]);
    assert!(listing.is_same_type(0, 1), "two folders");
    assert!(!listing.is_same_type(0, 2));
    assert!(listing.is_same_type(2, 3), "endings ignore case");
    assert!(!listing.is_same_type(2, 4));
    assert!(listing.is_same_type(5, 6), "no ending is a type too");
    assert!(!listing.is_same_type(2, 99));
}
```
  `dialog.rs` saf değil (pencere ister); not satırı Task 10'un Linux kabı modunda ve ekran listesinde sınanır.
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler. `cargo run -p gezik`: Ctrl+= (ve numerik +) → kutu son desenle, yazdıkça "N items match"; Select; Ctrl+- ile çıkar; Ctrl+Shift+I; Alt+numerik + bir `.jpg` odaktayken bütün jpg'ler; bir dosyayı Delete, Ctrl+Z, numerik / → seçim geri.
- [ ] **Step 5: Commit** "Select by pattern with a live count, invert, select the same type and restore the last selection".

---

### Task 9: Sekmeler — numaralar, geri açma, kilit, sekme seçici

**Files:**
- Create: `crates/gezik/src/tab_tools.rs`, `crates/gezik/ui/widgets/tab-picker.slint`
- Modify: `crates/gezik/src/navigation.rs`, `crates/gezik/src/context_menu.rs`, `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs` (`mod tab_tools;`, `TabTools::new`, `handle_key` katman kolu), `crates/gezik/ui/widgets/tab-bar.slint`, `crates/gezik/ui/app.slint`

**Interfaces:**
- Consumes: Task 1 (`Pattern`), Task 3 (`Tabs::{reopen, closed_count, is_locked, set_locked}`, `LOCKED_TAB`), Task 7 (`LOCK_TAB`, `UNLOCK_TAB`).
- Produces:

```rust
// navigation.rs
impl Navigator {
    pub fn reopen_tab(&self);                       // nothing to reopen: nothing
    pub fn toggle_tab_lock(&self, index: usize);
    pub fn is_tab_locked(&self, index: usize) -> bool;
    pub fn tab_list(&self) -> Vec<(String, String)>; // (title, path; "" for This PC) in tab order
}
// TabItem { title, active, locked }
// context_menu.rs
pub enum Place { …, Tab { only_tab: bool, locked: bool } }
// tab_tools.rs
pub struct TabTools;   // TabTools::new(window: &AppWindow, nav: Navigator) -> TabTools
pub fn with_current(f: impl FnOnce(&TabTools));
impl TabTools {
    pub fn open(&self);
    /// A key while the picker is open: Esc closes, Enter switches, Up/Down move; else the field's.
    pub fn chord(&self, chord: &Chord) -> bool;
}
/// The tabs whose title or path the query lets through (all for an empty or unfinished one).
pub fn picker_rows(tabs: &[(String, String)], query: &str) -> Vec<usize>;
/// The row Up/Down moves to, within `len` rows.
pub fn step(current: usize, len: usize, down: bool) -> usize;
```
- Slint: `tab-bar.slint` `TabItem`'a `locked: bool`; kilitliyse başlığın önünde 12 px'lik çizilmiş kilit (`Path`, gövde dikdörtgeni + yay; 5c'nin "×" gibi glif değil çizim), `close-shown: !item.locked && (item.active || hovered)`. `tab-picker.slint`: `export struct TabPickRow { title: string, path: string, active: bool }`, `export component TabPicker inherits Rectangle` — `in property <bool> open; in property <bool> dialog-open; in-out property <string> query; in property <[TabPickRow]> rows; in property <int> current; callback edited(string); callback chosen(int); public function take-focus();` Dialog gibi yarı saydam zemin, ortada 480 px kutu: üstte `TextField` (`edited` → `root.edited`), altında en çok 10 satır görünen liste (başlık, altında soluk yol; `current` satırı `Theme.selection`), satıra tık → `chosen(i)`. `app.slint`: `in property <bool> tp-open; in-out property <string> tp-query; in property <[TabPickRow]> tp-rows; in property <int> tp-current; callback tp-edited(string); callback tp-chosen(int);` `focus-list()`'e `else if root.tp-open { tab-picker.take-focus(); }`; `TabPickRow` dışa aktarılan listeye.

**Behavior:**
- `update_chrome` `TabItem { title, active, locked: inner.tabs.is_locked(i) }`; `navigation.rs` testindeki `tab()` yardımcısı `locked: false` ile.
- `reopen_tab`: `closed_count() == 0` ise bir şey yapmaz; değilse `with_tabs(Tabs::reopen)` ve `after_tabs_changed` (sekmenin kayıtlı görünümü — seçim, kaydırma, süzgeç — yüklenince gelir).
- `toggle_tab_lock(i)`: `keep_active_tab(|t| t.set_locked(i, !t.is_locked(i)))`, `update_chrome`.
- `items(Place::Tab { only_tab, locked })`: `Duplicate`, sonra `locked` ise `(UNLOCK_TAB, "Unlock tab")`, değilse `(LOCK_TAB, "Lock tab")` ve `(CLOSE_TAB, "Close")`; `!only_tab` ise `Close other tabs`. `Menus::tab` `locked: self.nav.is_tab_locked(index)`. `run`: `(LOCK_TAB | UNLOCK_TAB, Subject::Tab(id))` → `tab_index(id)` → `toggle_tab_lock`.
- `TabTools::open`: `tab_list()`, sorgu boş, satırlar hepsi, `current` = etkin sekmenin satırı; `tp-open` açık, alan odakta. `edited(q)`: `picker_rows`, `current` yeni listede aynı sekme varsa o, yoksa 0. `chord`: Esc → kapat ve `focus_list`; Enter → `current` satırının sekmesi `activate_tab`, kapat; Up/Down → `step`; öteki `false` (alana gider). `chosen(i)` → aynı Enter.
- `handle_key` başında (Convert katmanından sonra): `if window.get_tp_open() { let mut used = false; if let Some(chord) = &chord { tab_tools::with_current(|t| used = t.chord(chord)); } return used; }`.
- `actions.rs`: `ReopenTab => nav.reopen_tab()`, `ToggleTabLock => nav.toggle_tab_lock(nav.active_index())`, `TabPicker => tab_tools::with_current(TabTools::open)`; artık `false` listesi kalmaz (`_ => return false` kalır).

- [ ] **Step 1: Write the failing tests**

`tab_tools.rs`:

```rust
fn tabs() -> Vec<(String, String)> {
    [("Documents", "C:\\Users\\a\\Documents"), ("İndirilenler", "C:\\Users\\a\\Downloads"), ("This PC", "")]
        .map(|(t, p)| (t.to_owned(), p.to_owned()))
        .into()
}

#[test]
fn the_picker_matches_titles_and_paths() {
    assert_eq!(picker_rows(&tabs(), ""), [0, 1, 2]);
    assert_eq!(picker_rows(&tabs(), "doc"), [0]);
    assert_eq!(picker_rows(&tabs(), "downloads"), [1], "by path");
    assert_eq!(picker_rows(&tabs(), "indir"), [1], "Turkish İ");
    assert_eq!(picker_rows(&tabs(), "zzz"), Vec::<usize>::new());
    assert_eq!(picker_rows(&tabs(), "!"), [0, 1, 2], "an unfinished pattern shows them all");
}

#[test]
fn up_and_down_stay_within_the_rows() {
    assert_eq!(step(0, 3, true), 1);
    assert_eq!(step(2, 3, true), 2);
    assert_eq!(step(0, 3, false), 0);
    assert_eq!(step(0, 0, true), 0);
}
```

`context_menu.rs` (`tab_menu_hides_close_others_for_a_single_tab` yerine):

```rust
#[test]
fn the_tab_menu_offers_the_lock_and_no_close_on_a_locked_tab() {
    let tab = |only_tab, locked| ids(items(Place::Tab { only_tab, locked }, false));
    assert_eq!(tab(false, false), [DUPLICATE_TAB, LOCK_TAB, CLOSE_TAB, CLOSE_OTHER_TABS]);
    assert_eq!(tab(true, false), [DUPLICATE_TAB, LOCK_TAB, CLOSE_TAB]);
    assert_eq!(tab(false, true), [DUPLICATE_TAB, UNLOCK_TAB, CLOSE_OTHER_TABS]);
    assert_eq!(tab(true, true), [DUPLICATE_TAB, UNLOCK_TAB]);
}
```
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler. `cargo run -p gezik`: üç sekme; Ctrl+W, Ctrl+Shift+T (yerinde, Alt+Left eski yere); sağ tık → Lock tab (kilit simgesi, × yok); Ctrl+W ve orta tık "This tab is locked"; Close other tabs "1 locked tab stays open"; Ctrl+Shift+A → yaz → Enter.
- [ ] **Step 5: Commit** "Reopen closed tabs, lock tabs and pick a tab by typing".

---

### Task 10: Ölçüm, Linux kabı, notlar, test listeleri, spec

**Files:**
- Create: `crates/gezik-core/examples/filter_bench.rs`, `scripts/perf/filter.ps1`
- Modify: `scripts/linux/gui.sh` (`keyboard` kipi), `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` ("## Alt proje 6a (Klavye: süzgeç, seçim, sekmeler) sonrası"), `docs/superpowers/notes/macos-test.md`, `docs/superpowers/notes/linux-test.md`, `docs/superpowers/specs/2026-10-07-klavye-paketi-design.md` (Durum)

- [ ] **Step 1: Saf ölçüm.** `filter_bench.rs`: 100.000 `Entry` (`IMG_000123 Tatil ş3.jpg` biçiminde ve %20 `rapor N.pdf`, %10 klasör), `typed` dizileri `img_01234`, `*.pdf;!rapor 9*`, `tatil` harf harf: her vuruşta `Pattern::compile` + `matching_entries` + `HashSet` ile ad kümesinden seçim geri yükleme (görünümün yaptığı gibi, `Listing::indices_of`'un eşdeğeri) süresi; en kötü ve ortanca ms yazdırılır. `cargo test --release -p gezik-core pattern -- --ignored` de koşulur. Hedef: saf kısım ≤ 15 ms.
- [ ] **Step 2: Arayüzle ölçüm.** `scripts/perf/filter.ps1` (`stress.ps1` kalıbı, `try/finally`): bench'i sürümde çalıştırır; `-Gui` ile `$env:TEMP\gezik-stress-100000`'ı (yoksa oluşturur) sürüm Gezik'te açar, öne getirir, `SendKeys` ile `^f` ve `file_1234` harf harf, her vuruştan önce ve 300 ms sonra süreç CPU zamanını okur; vuruş başına CPU ms'nin en kötüsünü ve ortancasını yazar, Esc ile kapanış süresini de. Hedef ≤ 30 ms. Aşarsa sapma 19'daki daraltma (yeni metin eskisinin uzantısı ve desen düz tek parça ise yalnız görünenler arasından süz) `View::set_filter`'a eklenir ve yeniden ölçülür; karar ve sayılar notlara.
- [ ] **Step 3: Exe ve bellek.** `cargo build --release -p gezik`; master `0eb16fd` ile (ayrı çalışma ağacı ve hedef klasörü) karşılaştır; `scripts/perf/measure.ps1` ile boşta bellek (≤ 7 MB). Bütçe 6a + 6b ≤ +0,5 MB: 6a'nın payı yazılır, 6b'ye kalan söylenir.
- [ ] **Step 4: Linux kabı.** `docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/test.sh unit` → hepsi geçer. `gui.sh`'e `keyboard)` kipi (`*)` "all" listesine de eklenir; Xvfb, 900×600, `x11()` kalıbı; `copied()` yardımcısı `xclip -selection clipboard -t x-special/gnome-copied-files -o`'dan adları verir):
  - Veri `/tmp/k`: `Docs/`, `a.jpg`, `b.JPG`, `c.png`, `d.txt`, `İSTANBUL.txt`; `/tmp/cfg/settings.toml`: `[shortcuts]\ntoggle-tab-lock = "ctrl+shift+l"\n`.
  - Ctrl+F, `jpg`, Down, Ctrl+A, Ctrl+C → `a.jpg b.JPG` ("filter: only the shown items are selected").
  - Esc (listede) → Ctrl+A Ctrl+C → 6 ad. `/` `istanbul` Down Ctrl+A Ctrl+C → `İSTANBUL.txt`. Alanda Esc → kapalı.
  - Esc (seçimi temizle), `ctrl+equal`, `*.png;*.txt`, Return → Ctrl+C → `c.png d.txt İSTANBUL.txt`; `ctrl+shift+i` → Ctrl+C → `Docs a.jpg b.JPG`.
  - Esc; `KP_Add` → kutu açılır (ekran görüntüsü farkı) → `d*` Return → Ctrl+C → `Docs d.txt`; `KP_Subtract` `*.txt` Return → `Docs`.
  - `d.txt`'ye tık, Delete, Ctrl+Z, bekle, `KP_Divide` → Ctrl+C → `d.txt`.
  - Ctrl+T, Ctrl+L `/tmp/k/Docs` Return; `ctrl+1` → başlık `k — Gezik`; `ctrl+2` ve `ctrl+9` → `Docs — Gezik`; `ctrl+shift+2` → ekran görüntüsü listeden farklı (ızgara); `ctrl+shift+1`.
  - `ctrl+w` (Docs sekmesi) → `k`; `ctrl+shift+t` → `Docs`; `alt+Left` → başlık `Docs` değil (geçmiş geldi); `alt+Right`.
  - `ctrl+shift+l`, `ctrl+w` → başlık aynı ("locked tab stays").
  - `ctrl+shift+a`, `docs`, Return → `Docs — Gezik`.
  - Gezik'i `[keyboard]\ntyping = "filter"\n` ile yeniden başlat: `c.` Down Ctrl+A Ctrl+C → `c.png`.
  - Günlükte panik yok.
  Sonuçlar notlara; düşen madde varsa düzeltilir ve yeniden koşulur.
- [ ] **Step 5: Notlar** (Türkçe, 5d bölümünün kalıbında): ölçümler (saf ve arayüz, en kötü/ortanca), exe ve bellek, sapmalar (1-20 kısaca; özellikle winit kancası, Türkçe Q'da Ctrl+= ve `ctrl+shift+0` önerisi, `/`), Linux kabı ve `gui.sh keyboard` sonuçları, Windows ekran testi sonuçları (aşağıdaki liste), denenemeyenler (macOS ekranı: `@keys(Control + Shift + "1")`, `@keys(Control + "=")` menü kısayolları ve numerik tuş takımı; Wayland'da numerik tuşlar).
  - `macos-test.md`: "6a, keyboard" başlığıyla yeni maddeler (sıradaki numaralar): (a) ⌘F / `/` süzgeç, sayaç, ↓/Enter, Esc iki kez, sekme değişiminde kalır, klasör değişiminde gider, Türkçe ad `istanbul` → `İSTANBUL`; (b) ⌘= / ⌘- desen kutusu canlı sayı, ⌘⇧I, numerik tuş takımı varsa `+`/`-`/`/`, ⌥+numerik +; menü çubuğunda Edit ▸ Select by Pattern… vb. (fareyle de); (c) ⌘1…⌘8, ⌘9, ⌘⇧1/⌘⇧2 (menüde de; Türkçe Q'da da), ⌘⇧T geçmişiyle, ⌘⇧A seçici, sağ tık Lock tab ve kilitliyken ⌘W; (d) `[keyboard] typing = "filter"`; (e) ▾ Save as… `settings.toml`'da `[[filters]]`.
  - `linux-test.md`: "Keys on Linux" tablosuna Filter (Ctrl+F, /), Select/Deselect by pattern (Ctrl+= / Ctrl+-, keypad + / -), Invert (Ctrl+Shift+I), Restore selection (keypad /), Tabs (Ctrl+1…9, Ctrl+Shift+T, Ctrl+Shift+A), View (Ctrl+Shift+1/2); checklist'e aynı maddelerin Linux biçimi ve "Wayland'da keypad +" maddesi.
  - Spec Durum: "Tasarım onaylandı (2026-10-07); 6a uygulandı".
- [ ] **Step 6: Commit** "Measure the filter and note what 6a does".

---

## Ekran testleri (Windows, alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici; test verisi `%TEMP%\gezik-gui-6a\`: `Docs\`, `a.jpg`, `b.JPG`, `c.png`, `d.txt`, `İSTANBUL.txt`, `ılık.doc`, `notlar [1].txt`, `.gizli`; ayrıca `scripts/perf/stress.ps1`'in 100.000 dosyalık klasörü. Klavye düzeni önce ABD, sonra Türkçe Q (madde 13).

1. Ctrl+F → çubuk listenin üstünde açılır, alan odakta; `jpg` → liste `a.jpg`, `b.JPG`, sayaç "2 / 8"; ↓ → liste klavyeyi alır, çubuk açık; Ctrl+A → Delete → yalnız iki jpg çöpe gider; Ctrl+Z geri getirir ve süzgeç yerinde kalır.
2. Alanda `!` → kırmızı çizgi ve "Type a name after "!"", liste yerinde; `*.txt;!notlar*` → `d.txt`, `İSTANBUL.txt`; `[1]` → `notlar [1].txt`.
3. `istanbul` → `İSTANBUL.txt`; `ILIK` → `ılık.doc`.
4. Listede Esc → süzgeç kapanır, tam liste, seçim (görünenlerden) kalır; ikinci Esc seçimi temizler. Alanda Esc → kapanır.
5. Süzgeç açıkken Explorer'dan klasöre yeni `e.jpg` kopyala → liste kendiliğinden yenilenir, `e.jpg` görünür, süzgeç ve sayaç güncel. Ctrl+H iki kez → süzgeç kalır.
6. Süzgeçle `b.txt` gizliyken `a.txt`'yi F2 ile `d.txt` yapmaya çalış → "already exists" anında.
7. Sekme 2'de başka süzgeç; sekmeler arasında Ctrl+Tab → her sekme kendi süzgeciyle; `Docs`'a gir ve Alt+Left → süzgeçsiz.
8. `/` (liste odakta) süzgeci açar; `settings.toml`'a `[keyboard] typing = "filter"` yaz (canlı yenilenir) → listede `c` yazmak süzgeci `c` ile açar; `jump`'a dön → harfle atlama.
9. ▾ → Save as… → "Resimler" → `settings.toml`'da `[[filters]]`; ▾ → Resimler deseni yazar; ▾ → Delete "Resimler".
10. Ctrl+= ve numerik + → "Select by pattern" son desenle, yazdıkça "N items match", hatalı desende kırmızı not; Select. Ctrl+- ve numerik - → Deselect. Ana satırdaki `+` (Shift+=) kutuyu açmaz. Ctrl+Shift+I ters çevirir. `a.jpg` odaktayken Alt+numerik + → `a.jpg`, `b.JPG`.
11. `c.png` ve `d.txt` seçili → Delete → Ctrl+Z → numerik / → ikisi seçili. Başka klasörde numerik / → bir şey olmaz.
12. Dört sekme: Ctrl+1…Ctrl+4 geçer, Ctrl+7 bir şey yapmaz, Ctrl+9 sonuncu. Ctrl+Shift+2 ızgara, Ctrl+Shift+1 liste. `settings.toml`'a `view-list = "mod+1"` → Ctrl+1 liste görünümü, durum çubuğunda tab-1 uyarısı; satırı sil.
13. Türkçe Q düzeni: Ctrl+Shift+1/2 görünümü değiştirir; numerik +/- çalışır; Ctrl+- desenden çıkarır; Ctrl+= (Ctrl+Shift+0) eşleşmez, `select-pattern = ["num+", "ctrl+shift+0"]` ile çalışır; adres çubuğunda AltGr+8/9/7/0 `[ ] { }` yazar.
14. Bir sekmede iki klasör ilerle, süzgeç aç, Ctrl+W → Ctrl+Shift+T → aynı yerde, süzgeciyle; Alt+Left önceki klasöre. Üç sekme kapat, üç kez Ctrl+Shift+T → eski sırayla.
15. Sağ tık → Lock tab → kilit simgesi, × yok; Ctrl+W, orta tık ve Close other tabs (başka sekmeden) kilitliyi kapatmaz, durum çubuğu söyler; içinde gezinmek serbest; Unlock tab.
16. Ctrl+Shift+A → sekme seçici, etkin sekme seçili; `indir` → yalnız İndirilenler; ↑/↓, Enter geçer; Esc kapatır ve klavye listeye döner.
17. 100.000 dosyalık klasörde Ctrl+F ve `file_1234` harf harf: her vuruşta takılma yok (Task 10'un sayıları ile karşılaştır); Esc tam listeye anında döner.

---

## Self-review

- **Spec kapsamı:** §3.1 açma (Ctrl+F, `/`, yazma kipi) → Task 6-7; çubuk (sayaç, menü düğmesi, hata) → Task 6-7; süzme ve seçim temizliği, odak ilk öğede → Task 6; klavye (↓/Enter, Esc'ler, Ctrl+F metni seçer) → Task 6; kapanma (klasör, sekme başına, yenileme korur) → Task 3 + 6 (`view_to_show`); gizli dosyalardan sonra → Task 6 (`show`). §3.2 desen dili → Task 1. §3.3 kayıtlı süzgeçler → Task 5 + 7. §3.4 uygulama yeri (`View::show`/`resort`, sekme görünüm durumu, `state.toml`'a yazılmaz) → Task 3 + 6. §4 tablo → Task 2 + 4 + 8; desen penceresi ve `last_pattern` → Task 5 + 8; `Selection::invert` → Task 2; numerik tuş adları ve Ctrl+=/Ctrl+- → Task 4 (sapma 1, 4, 6). §5 numarayla geçiş ve taşınan görünüm kısayolları → Task 4 (sapma 2); kapatılanı geri açma (20, konum, geçmiş) → Task 3 + 9; sekme seçici → Task 9; kilit → Task 3 + 9. §8 `[keyboard]`, `[[filters]]` yorumu, `[selection]`, yeni eylemler, şablon, macOS menü çubuğu, ulaşılabilirlik testi → Task 4-5 (`clear-history` 6b, sapma 7). §9 kod yapısı → dosya listeleri. §10 birim testleri (desen, ayırmasızlık, ters çevirme, yığın, kilit, kısayol çakışmaları ve taşınanlar, eski ayar dosyaları: `an_old_hand_binding_of_ctrl_1_still_works`), performans, Linux kabı, Windows ekran testleri, macOS → Task 1-10 ve ekran listesi. §11 → Task 1 (ignored test) + Task 10.
- **Yer tutucu taraması:** Task 4'teki `actions.rs` kolları bilerek `false` döner ve hangi görevin doldurduğu yazılı (Task 6: Filter; Task 8: seçim kolları; Task 9: sekme kolları); her görev değiştirdiği kolu açıkça verir. Tamamı gövdesiyle yazılı olmayan test yok.
- **Tip tutarlılığı:** `Pattern`, `matching_entries` Task 1'de; `invert`, `set_where` Task 2'de; `ViewState.filter`, `ClosedTab`, `MAX_CLOSED`, `Closed::Locked`, `Tabs::{close_others → usize, reopen, closed_count, is_locked, set_locked}`, `LOCKED_TAB`, `locked_kept_text` Task 3'te; `Key::Num`, yeni `Action`'lar, `tab_number`, `default_texts`, `Physical`, `physical_of`, `note_pressed`, `take_pressed`, `chord_from_press`, `actions::run` Task 4'te; `Typing`, `KeyboardSettings`, `SavedFilter`, `SelectionState`, `with_filters`, `save_filters` Task 5'te; `ViewData::full`, `FilterState`, `filtered_listing`, `name_taken`, `filter_count_text`, `selection_after_filter`, `view_to_show`, `View::{set_filter, filter_text}`, `Filter::{open, typed, close}` Task 6'da; `filter::{set_settings, typing, saved}`, `Filter::{apply_saved, ask_save, delete_saved}`, `LOCK_TAB`, `UNLOCK_TAB`, `FILTER_*`, `filter_items`, `Menus::filter_menu` Task 7'de; `ask_text_noted`, `Listing::is_same_type`, `View::{invert_selection, count_matching, select_matching, select_same_type, remember_selection, restore_remembered}`, `SelectTools`, `match_note` Task 8'de; `Navigator::{reopen_tab, toggle_tab_lock, is_tab_locked, tab_list}`, `Place::Tab { only_tab, locked }`, `TabTools`, `picker_rows`, `step`, `TabPickRow` Task 9'da.
- **Review Focus:** beş maddenin her biri adı geçen görevde test olarak var (1: Task 4 `chord_from_press` testleri + Task 10 `gui.sh keyboard`; 2: Task 6 `name_taken`; 3: Task 3 + Task 6 `view_to_show`; 4: Task 1; 5: Task 3).
