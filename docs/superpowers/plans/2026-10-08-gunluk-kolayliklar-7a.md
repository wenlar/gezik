# Günlük Kolaylıklar 7a (Terminal, Yolu Kopyala, Oturum ve Sekme Setleri) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Klasörde terminal açmak (Windows Terminal ya da PowerShell, yönetici olarak UAC ile; macOS Terminal.app; Linux `$TERMINAL`, `x-terminal-emulator`, bilinen terminaller; `[terminal] command` hepsini geçersiz kılar), yolu yedi biçimde sistem panosuna metin olarak kopyalamak ("Copy path as ▸" ve `copy-path`), kapatıp açınca sekmelerin sırası, etkin sekmesi ve kilitleriyle geri gelmesi (`[session] restore`, `state.toml`) ve açık sekmeleri ad vererek `settings.toml`'a sekme seti olarak kaydedip açmak; bunlar için bir menüde birden çok alt menü ve 4096'dan başlayan Explorer kimlikleri.

**Architecture:** Saf kurallar `gezik-core`'da: yol metni (`path_text`: tırnaklama, `/`, ad, klasör, `file://`, UNC), terminal komutunun `{dir}`'i (`batch::convert::{check_dir_command, expand_dir_command}`), oturum ve set açma (`nav::{Session, SessionTab}`, `Tabs::{from_session, session, open_set}`). `gezik-platform`: terminal seçimi saf bir işlev (`terminal::choose`, ortam ve PATH bir `Lookup` ile verilir) ve iş parçacığında başlatma (Windows'ta `ShellExecuteW` "open"/"runas", başka yerde ayrık süreç + bekleyici iş parçacığı), eşlenmiş sürücünün payı (`fs::mapped_remote`, `WNetGetConnectionW`), panoya metin (`clipboard::write_text`: `CF_UNICODETEXT`, `NSPasteboardTypeString`, X11/Wayland metin hedefleri), birden çok `ShellSubmenu`. `gezik-config`: `[session]`, `[terminal]`, `[[tab-sets]]`, `state.toml` `[session]`, `SettingsChange::TabSets`, dört yeni eylem. Arayüzde `context_menu.rs` menüleri dört alt menü yuvasıyla kurar (`MenuSub`, `app.slint` ve `popup-menu.slint`); `terminal.rs`, `copy_path.rs`, `tab_sets.rs` (yeni) işleri yapar; `start.rs` açılışı oturumdan planlar, `Navigator` her sekme değişiminde oturumu yalnız değişmişse `state.toml` yazıcısına verir.

**Tech Stack:** Rust 2024, Slint 1.18, `windows` 0.62 (+ `Win32_NetworkManagement_WNet`), `objc2-app-kit` 0.3, `x11rb` 0.13, `wayland-client` 0.31, `toml`/`toml_edit` (var olan). Yeni crate yok.

**Spec:** `docs/superpowers/specs/2026-10-08-gunluk-kolayliklar-design.md` (bölüm 1, 2, 3, 4, 5, 10, 11.1, 12, 13'ün 7a kısmı)

**Platform denemesi:** `docs/superpowers/notes/2026-10-08-gunluk-kolayliklar-probe.md` (§4 terminal, §3 pano, §0 özellikler) ve `docs/superpowers/notes/gunluk-probe/` (`term.rs`, `terms.sh`, `*.out.txt`). Aşağıdaki Windows ve Linux terminal kodları oradaki çalışan denemeden alındı.

## Spec'ten sapmalar ve netleştirmeler

1. **Windows'ta başlatma `ShellExecuteW` ile**, hem normal ("open") hem yönetici ("runas"). `ShellExecuteExW`/`SHELLEXECUTEINFOW` `windows` 0.62'de `Win32_System_Registry` özelliği ister (probe §0); `ShellExecuteW` var olan `Win32_UI_Shell` + `Win32_UI_WindowsAndMessaging` ile gelir. Probe üç sorunlu klasörde (`boşluk ve ş 日本`, `noktalı;virgül`, `[köşeli] it's`) "open"un doğru çalıştığını gösterdi. Konsol uygulamasına Shell kendi yeni konsolunu verir, Gezik'in std tanıtıcıları geçmez, `ChildProcess`'in iş nesnesi kullanılmaz. UAC'de "No": dönüş ≤ 32 ve `GetLastError() == ERROR_CANCELLED` (1223) → sessiz (probe §4.1). Böylece `windows` crate'ine yalnız `Win32_NetworkManagement_WNet` eklenir.
2. **PowerShell klasörü her zaman argümanla:** Windows PowerShell 5.1 başlangıç klasöründe `[` olunca oraya gidemez (probe §4.1, `[köşeli] it's` PowerShell'in kendi klasörüne düştü). Bu yüzden `pwsh`/`powershell` normalde de yönetici olarak da `-NoExit -Command "Set-Location -LiteralPath '<klasör>'"` alır (`'` iki kez); `lpDirectory` de klasördür. `wt -d` aynı kalır; varsayılan profili PowerShell 5.1 olan bir Windows Terminal köşeli parantezli klasörde aynı sorunu yaşar; profil bilinmediğinden bu uç durum kabul edilir ve notlara yazılır.
3. **Windows Terminal PATH'ten:** `%LOCALAPPDATA%\Microsoft\WindowsApps` PATH'tedir; takma ad (`wt.exe`, 0 bayt, uygulama yürütme bağlantısı) Rust 1.99'da `fs::metadata(..).is_file() == true` (bu makinede ve probe'da denendi). Sıra `wt` > `pwsh` > `powershell` (probe'un önerdiği `cmd.exe /K` gerekmez: `powershell.exe` her Windows'ta var; yine de bulunamazsa "No terminal found").
4. **Linux bayrakları probe'un düzelttiği tablodan:** `ptyxis --new-window --working-directory DIR` (yalnız `--working-directory` yok sayılıyor), `kgx --working-directory DIR`, `gnome-terminal --working-directory=DIR`, `konsole --workdir DIR`, `xfce4-terminal --working-directory=DIR`, `kitty --directory DIR`, `alacritty --working-directory DIR`, `foot --working-directory=DIR`, `xterm` yalnız çalışma klasörüyle. Her birine ayrıca çalışma klasörü verilir.
5. **`x-terminal-emulator` çözülür:** Debian alternatifi bir sembolik bağlantıdır; Gezik onu `canonicalize` edip gerçek programın adı bilinen listedeyse onun klasör bayrağını ekler (gnome-terminal'in sunucusu çalışma klasörünü miras almaz, probe §4.3). Saf `choose` bunu `Lookup::real_name` kapanışıyla alır.
6. **Seçim ve başlatma iş parçacığında:** PATH taraması diske (bir ağ klasörü de olabilir) dokunur; arayüz iş parçacığı yalnız iş parçacığını başlatır. Hata metni `slint::invoke_from_event_loop` ile durum çubuğuna gelir (`view::with_current`). Unix'te süreç `process_group(0)`, stdin/stdout/stderr `null`; zombi kalmasın diye 64 KiB yığınlı bir iş parçacığı `wait()` eder (probe §4.3).
7. **`{dir}` kuralı `gezik-core`'da:** `batch::convert::{check_dir_command, expand_dir_command}` var olan `pieces` ayrıştırıcısını kullanır (`{{`/`}}` aynı); `{dir}` dışındaki her yer tutucu "unknown placeholder {x}". `gezik-config` ayrıştırırken denetler, `gezik-platform` genişletir.
8. **Alt menü yuvaları sabit dört:** `MenuSub { title, entries, after }` (alt menünün öğeleri ve ondan sonraki düz öğeler). `app.slint`'in `ContextMenuArea`'sı `menu-subs[0]`…`[3]` için dört blok taşır; Slint'te dizinin dışındaki indeks varsayılan değeri (boş diziler) verir, yani boş yuva hiçbir şey çizmez. `for` içinde `Menu` denenmez: bir yinelemede hem bir alt menü hem ardındaki düz öğeler üretilemez. Linux'un `PopupMenu`'sünde açıcı satırların kimliği `-1…-4` (yuva numarası).
9. **Alt menüde ayraç yok:** `Open tab set ▸` süzgeç menüsü gibi açar, değiştirir, siler sırasıyla düz listelenir (spec "ayraçtan sonra" diyor; Slint `for … : MenuItem` ayraç üretemez, Linux menüsünde ayraç satırı yok). Bölümler metinleriyle ayrılır (`Replace tabs with "…"`, `Delete "…"`).
10. **Kimlikler:** `FIRST_SHELL_ID = 4096` (`gezik_platform`'da `pub`, Windows), `context_menu::GEZIK_IDS_END = 4096` her platformda testlerin sınırı. 7a'nın aralıkları: `OPEN_TERMINAL 1000`, `OPEN_TERMINAL_ADMIN 1001`, `COPY_PATH_FIRST 1010-1016` (`PathFormat::ALL` sırasıyla), `SAVE_TAB_SET 1020`, `TAB_SET_OPEN_FIRST 1100-1129`, `TAB_SET_REPLACE_FIRST 1130-1159`, `TAB_SET_DELETE_FIRST 1160-1189`. 1200-4095 7b ve 7c'ye kalır.
11. **Menü yerleşimi:** satır, kenar çubuğu ve klasör boşluğu menülerinde yerin kendi öğelerinden sonra `Open terminal here` (Windows'ta altında `Open terminal as administrator`) ve hemen ardından `Copy path as ▸`. Windows'ta dosya araçları (Extract, Compress, Convert, Commands ▸) bugünkü gibi en üstte. Sekme menüsünün sonunda `Save tabs as…` ve `Open tab set ▸` (set yoksa alt menü yok).
12. **`open-terminal-admin` macOS menü çubuğunda yok** (spec §10.3'ün menü listesinde de yok; orada yalnız "Only on Windows" derdi). Kısayol tablosunda, şablonda ve ulaşılabilirlik testinde var.
13. **`copy-path` yazma alanlarını bekler** (`keys::acts_on_selection`): adres çubuğu ya da süzgeç alanı yazıyorken Ctrl+Shift+C alanındır. `open-terminal` beklemez; Shift+F4 değiştiricisiz olduğundan alan yazıyorken zaten alanındır.
14. **Copy path kenarları:** "This PC"de seçili sürücülerin kökleri, seçim yoksa bir şey yapılmaz. Kök (`C:\`, `C:`, `\\sunucu\pay`, `/`) için `Name` ve `Folder path` yolun kendisidir; `C:\a`'nın klasörü `C:\`, `/a`'nınki `/`. Windows'ta `Quoted` hep `"…"` (Windows adlarında `"` olmaz). Biçimler metin üzerinde çalışır (Windows yolu `windows` bayrağıyla tanınır), böylece her kural her sistemde test edilir. Durum çubuğu `Copied the path` / `Copied 1,234 paths`; pano yoksa `No system clipboard here`.
15. **UNC:** `gezik_platform::fs::mapped_remote(letter)` (`WNetGetConnectionW`; bağlı değilse `None`; kalıcı ama kopuk bağlantı `ERROR_CONNECTION_UNAVAIL` ile de payı verir). Menü açılırken arayüz iş parçacığında, yalnız seçimin ilk öğesi için çağrılır (MPR'nin yerel kaydı; ağa gitmez). Yönetici terminali için klasör aynı işlevle UNC'ye çevrilir.
16. **Linux pano metni:** X11'de metin sahipken `TARGETS` = `TARGETS UTF8_STRING text/plain;charset=utf-8 TEXT STRING`; `STRING` Latin-1 (dışındakiler `?`). Wayland'da `text/plain;charset=utf-8`, `text/plain`, `UTF8_STRING`, `TEXT`, `STRING` (hepsi UTF-8; probe §3.4'teki wl-copy'nin sunduğu türler). Gezik metin sahibiyken `read_files` `None` döner. `Operations.clip` değişmez; sistem panosunun sıra numarası değiştiği için bir sonraki `clipboard_check` kesilenlerin solukluğunu kaldırır (spec §4.2).
17. **Oturum yazımı:** `Navigator` son yazdırdığı oturumu tutar ve `update_chrome`'un sonunda yalnız değişmişse `keep_session`'a verilen işleve (main.rs: `update_state`) verir. Açılışta `state.toml`'dakiyle aynıysa yazılmaz; `restore = false` iken kayıt zaten boşsa yazılmaz. Ayar canlı değişirse (`apply_config`) `set_session_restore` hemen yazar ya da siler.
18. **`plan_start` oturumla:** `StartPlan.first` yerine `session: Session`; `StartPlan::first()` öndeki sekmenin yeri. Kayıtlı oturumun yolları için `kind` hiç çağrılmaz. Komut satırındaki yol yoksa uyarı `…: not found; opening the last tabs` (oturum yokken bugünkü `opening start-folder`).
19. **Bozuk oturum girdisi atlanır**, `active` kalanlara göre yeniden sayılır, etkin girdi atlanmışsa 0. Göreli yol (elle yazılmış) atlanır.
20. **Sekme setinde `..` içeren ya da boş yol bütün seti dışarıda bırakır** (uyarıyla; `with_tables` onu dosyada olduğu gibi korur). Boş `tabs` "tabs is missing". En çok 30 set listelenir; 31.'si kaydedilmek istenince `Up to 30 tab sets`. Ad sorusu `Save tabs` / `Name for these tabs:`; aynı ad (harf büyüklüğü fark etmez) `A tab set called "X" already exists. Replace it?`. Kaydedilen sekmeler "Save tabs as…"a basıldığı andakilerdir.
21. **Sekme seti modülü `crates/gezik/src/tab_sets.rs`** (yeni; spec §11 `tab_tools.rs` diyor, orası sekme seçicisi). `filter.rs`'in kayıtlı süzgeç kalıbı aynen (`queue_write`, `finish_write`, tek yazıcı).
22. **Linux kabındaki eski `gui.sh` kipleri:** oturum varsayılan açık olduğundan Gezik'i aynı `/tmp/cfg` ile yeniden başlatan kipler önceki sekmeleri geri alır. `fresh_files` ve aynı yapılandırmayla ikinci kez başlatan her kip `settings.toml`'una `[session]\nrestore = false\n` yazar (Task 10).
23. **Dal:** `feat/daily-7a`, çalışma ağacı `D:\Work\gezik-7a`, master `38c65b3` + spec `caf831c` + bu plan.

## Global Constraints

- Rust edition 2024, stable (1.99). Komutlar `D:\Work\gezik-7a`'dan (7a'nın çalışma ağacı); `cargo` = `~/.cargo/bin/cargo` (Git Bash), her derleme ve testte `-j 8`. Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Dal `feat/daily-7a` (`caf831c` ve plan commit'i üstünde). `D:\Work\gezik`'te başka iş sürüyor: oraya dokunulmaz, orada derlenmez.
- Her görevin sonunda `cargo build --workspace -j 8`, `cargo test --workspace -j 8` geçer; `cargo clippy --workspace --all-targets -j 8 -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- Çapraz denetimler: `cargo check -j 8 -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -j 8 -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -j 8 -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- Yeni crate yok; `gezik-core` bağımlılıksız kalır. `windows` crate'ine yalnız `Win32_NetworkManagement_WNet` eklenir (sapma 1).
- Exe: master `38c65b3`'ün sürüm derlemesi **22.240.256 bayt** (2026-10-07, bu çalışma ağacında `caf831c`'de `cargo build --release -p gezik -j 8` ile ölçüldü; `caf831c` yalnız spec ekler, kod master'la aynı). 7a sınırı +0,25 MiB = +262.144 bayt → **en çok 22.502.400 bayt**. 7a + 7b + 7c ≤ +0,75 MiB.
- Boşta bellek master'a göre büyümez (Windows'ta 7,2–7,3 MB, `measure.ps1`; Linux kabında RssAnon aynı). Açılış süresi değişmez (≤ ~60 ms); oturumda yalnız etkin sekme okunur, açılışta varlık denetimi yok; 20 sekmelik oturumla da.
- Arayüz iş parçacığı dosya sistemine dokunmaz: terminal seçimi (PATH) ve başlatma iş parçacığında. `state.toml` yalnız var olan yazıcıdan (`ConfigStore::update_state`); `settings.toml` yalnız tek yazıcı iş parçacığından (`write_settings`, `SettingsChange::TabSets`).
- Her yeni eylem (`open-terminal`, `open-terminal-admin`, `copy-path`, `save-tab-set`) `[shortcuts]` ile değiştirilebilir, şablonda yorum satırı olarak görünür ve ulaşılabilirlik testine girer; macOS menü çubuğunda `open-terminal`, `copy-path`, `save-tab-set` ve `Open Tab Set` (sapma 12).
- Gezik'in menü kimlikleri `1..4096`; 4096 ve üstü Explorer'ın.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Co-Authored-By/Claude satırı yok. Plan ve not metinleri Türkçe.
- Yol birleştirme yalnız `Path::join`; Copy path biçimleri metin üretir (sapma 14).

## Review Focus

1. **Copy path özel adlarla:** boşluk, `'`, `%`, `#`, Türkçe harf, sürücü kökü, UNC payı, eşlenmiş sürücü; cmd, PowerShell, bash'e ve tarayıcıya yapıştırınca aynı öğe. → Task 1 testleri (`windows_paths_in_every_format`, `posix_paths_quote_for_the_shell`, `url_encoding_keeps_only_the_unreserved`, `roots_are_their_own_name_and_folder`, `a_mapped_drive_goes_through_its_share`), Task 10 `gui.sh copy-path`, ekran 6-8.
2. **Açılışta erişilemeyen yollar:** kayıtlı oturumda silinmiş klasör, takılı ağ yolu, This PC; açılış beklemez, yalnız etkin sekme okunur, silinmiş etkin klasör en yakın üst klasöre düşer. → Task 8 `a_saved_session_is_never_looked_at_on_disk`, Task 10 `gui.sh session` (silinen `two`), ekran 11.
3. **Elle düzenlenmiş ya da bozuk dosyalar:** `state.toml`'da yolsuz girdi, aralık dışı `active`, göreli yol, negatif `active`; `settings.toml`'da `..`'lı, adsız, boş ya da aynı adlı set; Gezik yazınca bozuk girdiler korunur. → Task 3 `broken_session_entries_are_left_out`, `bad_session_terminal_and_tab_sets_warn`, `tab_sets_are_written_keeping_the_rest`.
4. **Terminal klasör adları:** `;`, `'`, `[`, boşluk, Türkçe; `wt` kaçışı, PowerShell `-LiteralPath` ve `''`, Windows komut satırı tırnaklaması, Linux bayrakları, `[terminal] command`'da `{dir}` ve `{{`. → Task 4 testleri, Task 10 `gui.sh terminal`, ekran 1-4.
5. **Metin panosu dosya panosunu bozmaz:** Ctrl+X'lenen dosyanın solukluğu metin kopyalanınca kalkar, Ctrl+V kesilen dosyayı taşımaz; Linux'ta Gezik metin sahibiyken dosya okunmaz. → Task 5 `text_is_offered_as_text_only` (Wayland) ve `latin1_keeps_what_it_can`, Task 10 `gui.sh copy-path` (TARGETS, `copied` boş, Ctrl+V bir şey yapmaz), ekran 7.

---

### Task 1: `gezik-core` — yol metni (`path_text`) ve terminal komutunun `{dir}`'i

**Files:**
- Create: `crates/gezik-core/src/path_text.rs`
- Modify: `crates/gezik-core/src/lib.rs` (`pub mod path_text;`, `pattern`'den sonra), `crates/gezik-core/src/batch/convert.rs` (`check_dir_command`, `expand_dir_command`, testler)

**Interfaces:**
- Consumes: `batch::convert`'in özel `pieces` ayrıştırıcısı.
- Produces:

```rust
// path_text.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFormat { Full, Quoted, ForwardSlashes, Name, Folder, FileUrl, Unc }
impl PathFormat {
    pub const ALL: [PathFormat; 7];                       // menu order
    pub fn label(self) -> &'static str;                   // "Full path", "Quoted", …
    pub fn offered(self, windows: bool, unc: bool) -> bool;
}
pub fn format_paths(paths: &[String], kind: PathFormat, windows: bool, remote: &dyn Fn(char) -> Option<String>) -> String;
pub fn unc_path(path: &str, remote: &dyn Fn(char) -> Option<String>) -> Option<String>;
pub fn posix_quote(text: &str) -> String;
pub fn file_url(path: &str, windows: bool) -> String;
pub fn file_name(path: &str, windows: bool) -> &str;
pub fn folder_of(path: &str, windows: bool) -> String;
// batch/convert.rs
pub fn check_dir_command(run: &[String]) -> Result<(), String>;
pub fn expand_dir_command(run: &[String], dir: &Path) -> Result<Vec<OsString>, String>;
```

- [ ] **Step 0: Çalışma ağacı.** `cd /d/Work/gezik-7a && git status` temiz, dal `feat/daily-7a`, `git log --oneline -3` plan commit'i, `caf831c`, `38c65b3`.

- [ ] **Step 1: Write the failing tests** (`path_text.rs`'in sonunda; `convert.rs` testlerine bir test)

`path_text.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: char) -> Option<String> {
        None
    }

    fn mapped(letter: char) -> Option<String> {
        (letter == 'Z').then(|| r"\\sunucu\pay".to_owned())
    }

    fn one(path: &str, kind: PathFormat, windows: bool) -> String {
        format_paths(&[path.to_owned()], kind, windows, &mapped)
    }

    #[test]
    fn windows_paths_in_every_format() {
        let p = r"C:\Users\a\rapor ç.pdf";
        assert_eq!(one(p, PathFormat::Full, true), p);
        assert_eq!(one(p, PathFormat::Quoted, true), r#""C:\Users\a\rapor ç.pdf""#);
        assert_eq!(one(p, PathFormat::ForwardSlashes, true), "C:/Users/a/rapor ç.pdf");
        assert_eq!(one(p, PathFormat::Name, true), "rapor ç.pdf");
        assert_eq!(one(p, PathFormat::Folder, true), r"C:\Users\a");
        assert_eq!(one(p, PathFormat::FileUrl, true), "file:///C:/Users/a/rapor%20%C3%A7.pdf");
        assert_eq!(one(p, PathFormat::Unc, true), p, "not on a mapped drive: the full path");
    }

    #[test]
    fn posix_paths_quote_for_the_shell() {
        let p = "/home/a/it's ş.pdf";
        assert_eq!(one(p, PathFormat::Quoted, false), r"'/home/a/it'\''s ş.pdf'");
        assert_eq!(posix_quote("a'b'"), r"'a'\''b'\'''");
        assert_eq!(one(p, PathFormat::ForwardSlashes, false), p);
        assert_eq!(one(p, PathFormat::Name, false), "it's ş.pdf");
        assert_eq!(one(p, PathFormat::Folder, false), "/home/a");
        assert_eq!(one(p, PathFormat::FileUrl, false), "file:///home/a/it%27s%20%C5%9F.pdf");
    }

    #[test]
    fn url_encoding_keeps_only_the_unreserved() {
        assert_eq!(file_url("/a/100% #1 x~y_z-.txt", false), "file:///a/100%25%20%231%20x~y_z-.txt");
        assert_eq!(file_url(r"C:\", true), "file:///C:/");
        assert_eq!(file_url(r"\\nas\foto\yaz 1.jpg", true), "file://nas/foto/yaz%201.jpg");
        assert_eq!(file_url("/", false), "file:///");
    }

    #[test]
    fn roots_are_their_own_name_and_folder() {
        for (path, windows) in [(r"C:\", true), ("C:", true), (r"\\nas\foto", true), (r"\\nas\foto\", true), ("/", false)] {
            assert_eq!(file_name(path, windows), path);
            assert_eq!(folder_of(path, windows), path);
        }
        assert_eq!(folder_of(r"C:\a", true), r"C:\");
        assert_eq!(folder_of("/a", false), "/");
        assert_eq!(folder_of(r"\\nas\foto\x.jpg", true), r"\\nas\foto");
        assert_eq!(file_name(r"C:\a\sub\", true), "sub");
        assert_eq!(file_name(r"C:\a\b", false), r"C:\a\b", "a backslash is no separator off Windows");
    }

    #[test]
    fn a_mapped_drive_goes_through_its_share() {
        assert_eq!(unc_path(r"Z:\proje\a.txt", &mapped).as_deref(), Some(r"\\sunucu\pay\proje\a.txt"));
        assert_eq!(unc_path(r"z:\proje", &mapped).as_deref(), Some(r"\\sunucu\pay\proje"));
        assert_eq!(unc_path(r"Z:\", &mapped).as_deref(), Some(r"\\sunucu\pay\"));
        assert_eq!(unc_path(r"C:\a", &mapped), None);
        assert_eq!(unc_path(r"\\nas\x", &mapped), None);
        assert_eq!(unc_path("/z:/a", &mapped), None);
        let lines = format_paths(&[r"Z:\a.txt".into(), r"C:\b.txt".into()], PathFormat::Unc, true, &mapped);
        assert_eq!(lines, "\\\\sunucu\\pay\\a.txt\r\nC:\\b.txt", "items not on it keep their full path");
    }

    #[test]
    fn several_items_one_per_line_without_a_last_line_break() {
        let paths = ["/a/x".to_owned(), "/a/y".to_owned()];
        assert_eq!(format_paths(&paths, PathFormat::Name, false, &none), "x\ny");
        let win = [r"C:\x".to_owned(), r"C:\y".to_owned()];
        assert_eq!(format_paths(&win, PathFormat::Full, true, &none), "C:\\x\r\nC:\\y");
        assert_eq!(format_paths(&[], PathFormat::Full, true, &none), "");
    }

    #[test]
    fn the_menu_offers_slashes_and_unc_only_where_they_mean_something() {
        let offered = |windows, unc| -> Vec<&str> {
            PathFormat::ALL.iter().filter(|k| k.offered(windows, unc)).map(|k| k.label()).collect()
        };
        assert_eq!(offered(false, false), ["Full path", "Quoted", "Name", "Folder path", "file:// URL"]);
        assert_eq!(offered(true, false).len(), 6);
        assert_eq!(offered(true, true).last(), Some(&"UNC path"));
    }
}
```

`convert.rs` (test modülüne):

```rust
#[test]
fn the_terminal_command_takes_only_dir() {
    let run = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
    let dir = Path::new("/home/a/it's {x}");
    assert_eq!(
        expand_dir_command(&run(&["wezterm", "start", "--cwd={dir}", "{{dir}}"]), dir).unwrap(),
        [OsString::from("wezterm"), "start".into(), "--cwd=/home/a/it's {x}".into(), "{dir}".into()]
    );
    assert_eq!(check_dir_command(&run(&["x", "{in}"])), Err("unknown placeholder {in}".to_owned()));
    assert_eq!(check_dir_command(&run(&["x", "{foo}"])), Err("unknown placeholder {foo}".to_owned()));
    assert!(check_dir_command(&run(&["x", "{dir"])).unwrap_err().contains("not closed"));
    assert_eq!(check_dir_command(&[]), Err("the command has no program".to_owned()));
    assert_eq!(check_dir_command(&run(&[" "])), Err("the command has no program".to_owned()));
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core path_text the_terminal_command` → FAIL (`path_text` modülü ve işlevler yok).

- [ ] **Step 3: Implement.** `path_text.rs`:

```rust
//! A path as text to paste somewhere else (spec 4): quoted for a shell, with forward slashes,
//! as a `file://` URL (RFC 8089) or through a mapped drive's share. Pure, on text: a Windows
//! path is told by `windows`, so every rule is tested on every system.

use std::fmt::Write as _;

/// One way to write a path ("Copy path as ▸").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFormat {
    Full,
    Quoted,
    ForwardSlashes,
    Name,
    Folder,
    FileUrl,
    Unc,
}

impl PathFormat {
    /// In menu order (the menu ids follow it).
    pub const ALL: [PathFormat; 7] = [
        PathFormat::Full,
        PathFormat::Quoted,
        PathFormat::ForwardSlashes,
        PathFormat::Name,
        PathFormat::Folder,
        PathFormat::FileUrl,
        PathFormat::Unc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PathFormat::Full => "Full path",
            PathFormat::Quoted => "Quoted",
            PathFormat::ForwardSlashes => "With forward slashes",
            PathFormat::Name => "Name",
            PathFormat::Folder => "Folder path",
            PathFormat::FileUrl => "file:// URL",
            PathFormat::Unc => "UNC path",
        }
    }

    /// Whether the menu lists it: forward slashes only on Windows (elsewhere they are the full
    /// path), UNC only on Windows when the first item is on a mapped drive (`unc`).
    pub fn offered(self, windows: bool, unc: bool) -> bool {
        match self {
            PathFormat::ForwardSlashes => windows,
            PathFormat::Unc => windows && unc,
            _ => true,
        }
    }
}

fn is_separator(c: char, windows: bool) -> bool {
    c == '/' || (windows && c == '\\')
}

/// Whether `path` is a root: `C:\` (or `C:`) and `\\server\share` on Windows, `/` elsewhere.
fn is_root(path: &str, windows: bool) -> bool {
    let trimmed = path.trim_end_matches(|c| is_separator(c, windows));
    if !windows {
        return trimmed.is_empty() && !path.is_empty();
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    match trimmed.strip_prefix(r"\\").or_else(|| trimmed.strip_prefix("//")) {
        Some(rest) => rest.split(|c| is_separator(c, true)).filter(|part| !part.is_empty()).count() <= 2,
        None => false,
    }
}

/// The last part of `path` (`rapor.pdf`); a root is its own name.
pub fn file_name(path: &str, windows: bool) -> &str {
    if is_root(path, windows) {
        return path;
    }
    let trimmed = path.trim_end_matches(|c| is_separator(c, windows));
    match trimmed.rfind(|c| is_separator(c, windows)) {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

/// The folder `path` is in (`C:\Users\a`). A root is its own folder; a drive's top folder
/// keeps its separator (`C:\`, `/`).
pub fn folder_of(path: &str, windows: bool) -> String {
    if is_root(path, windows) {
        return path.to_owned();
    }
    let trimmed = path.trim_end_matches(|c| is_separator(c, windows));
    let Some(i) = trimmed.rfind(|c| is_separator(c, windows)) else { return path.to_owned() };
    let parent = &trimmed[..i];
    if parent.is_empty() || (windows && parent.len() == 2 && parent.ends_with(':')) {
        // `/a` → `/`, `C:\a` → `C:\`.
        return trimmed[..=i].to_owned();
    }
    parent.to_owned()
}

/// `text` in POSIX single quotes, safe to paste into bash or zsh: `it's` → `'it'\''s'`.
pub fn posix_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `path` as a `file://` URL (RFC 8089): its UTF-8 bytes percent-encoded but for
/// `A–Z a–z 0–9 - . _ ~ /`; `C:\a b` → `file:///C:/a%20b` (the drive's colon stays),
/// `\\server\share\x` → `file://server/share/x`, `/home/a` → `file:///home/a`.
pub fn file_url(path: &str, windows: bool) -> String {
    let slashed = if windows { path.replace('\\', "/") } else { path.to_owned() };
    if windows && let Some(rest) = slashed.strip_prefix("//") {
        return format!("file://{}", encode(rest));
    }
    let bytes = slashed.as_bytes();
    if windows && bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return format!("file:///{}{}", &slashed[..2], encode(&slashed[2..]));
    }
    format!("file://{}", encode(&slashed))
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// `path` on a mapped drive (`Z:\proje\a.txt`) through the share `remote` gives for its
/// letter (`\\server\share`): `\\server\share\proje\a.txt`. `None` when the path has no
/// drive letter or the drive is not mapped.
pub fn unc_path(path: &str, remote: &dyn Fn(char) -> Option<String>) -> Option<String> {
    let mut chars = path.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else { return None };
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let share = remote(letter.to_ascii_uppercase())?;
    let share = share.trim_end_matches('\\');
    let rest = &path[2..];
    Some(if rest.is_empty() { format!("{share}\\") } else { format!("{share}{rest}") })
}

fn format_one(path: &str, kind: PathFormat, windows: bool, remote: &dyn Fn(char) -> Option<String>) -> String {
    match kind {
        PathFormat::Full => path.to_owned(),
        // Windows names have no `"`: cmd, PowerShell and Explorer's "Copy as path" quote so.
        PathFormat::Quoted if windows => format!("\"{path}\""),
        PathFormat::Quoted => posix_quote(path),
        PathFormat::ForwardSlashes if windows => path.replace('\\', "/"),
        PathFormat::ForwardSlashes => path.to_owned(),
        PathFormat::Name => file_name(path, windows).to_owned(),
        PathFormat::Folder => folder_of(path, windows),
        PathFormat::FileUrl => file_url(path, windows),
        PathFormat::Unc => unc_path(path, remote).unwrap_or_else(|| path.to_owned()),
    }
}

/// `paths` written as `kind`, one per line (`\r\n` on Windows, `\n` elsewhere), with no line
/// break after the last.
pub fn format_paths(
    paths: &[String],
    kind: PathFormat,
    windows: bool,
    remote: &dyn Fn(char) -> Option<String>,
) -> String {
    let lines: Vec<String> = paths.iter().map(|path| format_one(path, kind, windows, remote)).collect();
    lines.join(if windows { "\r\n" } else { "\n" })
}
```

`convert.rs` (`check_command`'ın altına):

```rust
/// Checks the terminal's command (`[terminal] command`, spec 3.2): a program first, and
/// `{dir}` the only placeholder (`{{` and `}}` are braces).
pub fn check_dir_command(run: &[String]) -> Result<(), String> {
    if run.first().is_none_or(|program| program.trim().is_empty()) {
        return Err("the command has no program".to_owned());
    }
    for arg in run {
        if let Some(field) = pieces(arg)?.into_iter().find_map(|piece| piece.err().filter(|field| *field != "dir")) {
            return Err(format!("unknown placeholder {{{field}}}"));
        }
    }
    Ok(())
}

/// The terminal's command with `{dir}` put in; each argument stays one argument.
pub fn expand_dir_command(run: &[String], dir: &Path) -> Result<Vec<OsString>, String> {
    check_dir_command(run)?;
    run.iter()
        .map(|arg| {
            let mut expanded = OsString::new();
            for piece in pieces(arg)? {
                match piece {
                    Ok(text) => expanded.push(text),
                    Err(_) => expanded.push(dir),
                }
            }
            Ok(expanded)
        })
        .collect()
}
```

`lib.rs`: `pub mod path_text;` (`pattern`'den sonra, alfabetik).

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-core` → PASS; `cargo clippy -j 8 -p gezik-core --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** "Write paths as text for pasting and take a terminal command with {dir}".

---

### Task 2: `gezik-core::nav` — oturum ve sekme setini açma

**Files:**
- Modify: `crates/gezik-core/src/nav.rs`

**Interfaces:**
- Consumes: `Tabs`'ın özel `new_id`, `take_out`, `is_locked`, `set_locked`.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTab { pub location: Location, pub locked: bool }
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session { pub tabs: Vec<SessionTab>, pub active: usize }
impl Session {
    pub fn is_empty(&self) -> bool;
    pub fn single(location: Location) -> Session;
    pub fn active_location(&self) -> Option<&Location>;   // `active`, else the first
}
impl Tabs {
    pub fn from_session(session: &Session) -> Option<Tabs>;      // None for an empty one
    pub fn session(&self) -> Session;
    pub fn open_set(&mut self, locations: Vec<Location>, replace: bool) -> usize;   // locked kept
}
```

- [ ] **Step 1: Write the failing tests** (`nav.rs` test modülü; `p` yardımcısı var)

```rust
// ---- Session and tab sets ----

#[test]
fn a_session_round_trips_through_the_tabs() {
    let session = Session {
        tabs: vec![
            SessionTab { location: p("/a"), locked: false },
            SessionTab { location: Location::Drives, locked: true },
            SessionTab { location: p("/c"), locked: true },
        ],
        active: 2,
    };
    let tabs = Tabs::from_session(&session).unwrap();
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs.active_index(), 2);
    assert!(!tabs.is_locked(0) && tabs.is_locked(1) && tabs.is_locked(2));
    assert_eq!(tabs.session(), session);
    assert_eq!(ids(&tabs).len(), 3, "every tab has its own id");
    assert!(Tabs::from_session(&Session::default()).is_none());
}

#[test]
fn an_active_tab_out_of_range_is_the_first() {
    let session = Session { tabs: vec![SessionTab { location: p("/a"), locked: false }], active: 7 };
    assert_eq!(Tabs::from_session(&session).unwrap().active_index(), 0);
    assert_eq!(session.active_location(), Some(&p("/a")));
    assert_eq!(Session::default().active_location(), None);
    assert_eq!(Session::single(p("/x")).tabs, [SessionTab { location: p("/x"), locked: false }]);
}

#[test]
fn a_tab_set_opens_at_the_end_with_its_first_tab_in_front() {
    let mut tabs = Tabs::new(p("/a"));
    tabs.open(p("/b"), false);
    assert_eq!(tabs.open_set(vec![p("/x"), Location::Drives], false), 0);
    let shown: Vec<Location> = tabs.iter().map(|h| h.location().clone()).collect();
    assert_eq!(shown, [p("/a"), p("/b"), p("/x"), Location::Drives]);
    assert_eq!(tabs.active_index(), 2);
    assert_eq!(tabs.open_set(Vec::new(), true), 0);
    assert_eq!(tabs.len(), 4, "an empty set changes nothing");
}

#[test]
fn replacing_tabs_with_a_set_keeps_the_locked_ones() {
    let mut tabs = Tabs::new(p("/a"));
    tabs.open(p("/b"), true);
    tabs.open(p("/c"), false);
    tabs.set_locked(1, true);
    assert_eq!(tabs.open_set(vec![p("/x"), p("/y")], true), 1);
    let shown: Vec<Location> = tabs.iter().map(|h| h.location().clone()).collect();
    assert_eq!(shown, [p("/b"), p("/x"), p("/y")]);
    assert_eq!(tabs.active_index(), 1);
    assert!(tabs.is_locked(0));
    assert_eq!(tabs.closed_count(), 2, "the closed tabs can be reopened");
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-core nav` → FAIL (`Session` yok).

- [ ] **Step 3: Implement** (`Closed`'un altına türler, `Tabs`'a yöntemler):

```rust
/// A tab as the session keeps it (spec 5.1): where it is and whether it is locked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTab {
    pub location: Location,
    pub locked: bool,
}

/// The open tabs, to open them again at the next start; `active` counts from 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
}

impl Session {
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// One unlocked tab at `location` (today's start).
    pub fn single(location: Location) -> Session {
        Session { tabs: vec![SessionTab { location, locked: false }], active: 0 }
    }

    /// Where the tab in front is: `active`, or the first when that is out of range.
    pub fn active_location(&self) -> Option<&Location> {
        self.tabs.get(self.active).or(self.tabs.first()).map(|tab| &tab.location)
    }
}
```

`impl Tabs` içine:

```rust
    /// Adds a tab at the end, not activated; its index.
    fn push(&mut self, location: Location) -> usize {
        self.tabs.push(History::new(location));
        let id = self.new_id();
        self.ids.push(id);
        self.tabs.len() - 1
    }

    /// The tabs of `session`, its active one in front (the first if `active` is out of
    /// range), its locks on; `None` for an empty session.
    pub fn from_session(session: &Session) -> Option<Tabs> {
        let first = session.tabs.first()?;
        let mut tabs = Tabs::new(first.location.clone());
        for tab in &session.tabs[1..] {
            tabs.push(tab.location.clone());
        }
        for (i, tab) in session.tabs.iter().enumerate() {
            if tab.locked {
                tabs.set_locked(i, true);
            }
        }
        tabs.active = if session.active < tabs.tabs.len() { session.active } else { 0 };
        Some(tabs)
    }

    /// The tabs as the session keeps them.
    pub fn session(&self) -> Session {
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, history)| SessionTab { location: history.location().clone(), locked: self.is_locked(i) })
            .collect();
        Session { tabs, active: self.active }
    }

    /// Opens `locations` as tabs at the end, the first of them active (a tab set, spec 5.2).
    /// `replace` first closes the tabs that were open, but the locked ones, onto the closed
    /// list (as "close other tabs" does). Returns how many locked tabs stayed (0 without
    /// `replace`); no locations change nothing.
    pub fn open_set(&mut self, locations: Vec<Location>, replace: bool) -> usize {
        if locations.is_empty() {
            return 0;
        }
        let old = self.tabs.len();
        let mut first = None;
        for location in locations {
            let index = self.push(location);
            first.get_or_insert(self.ids[index]);
        }
        let mut locked = 0;
        if replace {
            // From the right, so that each tab's index is still the one it had.
            for i in (0..old).rev() {
                if self.is_locked(i) {
                    locked += 1;
                } else {
                    self.take_out(i);
                }
            }
        }
        self.active = first.and_then(|id| self.index_of(id)).unwrap_or(0);
        locked
    }
```

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-core` → PASS.
- [ ] **Step 5: Commit** "Keep the open tabs as a session and open a set of tabs at the end".

---

### Task 3: `gezik-config` — `[session]`, `[terminal]`, `[[tab-sets]]`, durumda oturum, yazıcı, dört eylem, şablon

**Files:**
- Modify: `crates/gezik-config/src/settings.rs`, `crates/gezik-config/src/settings_edit.rs`, `crates/gezik-config/src/settings_writer.rs`, `crates/gezik-config/src/store.rs`, `crates/gezik-config/src/shortcuts.rs`, `crates/gezik-config/templates/settings.toml`; derlemenin istediği kollar: `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs` (`handle_key`), `crates/gezik/src/keys.rs` (test)

**Interfaces:**
- Consumes: Task 1 (`check_dir_command`), Task 2 (`Session`, `SessionTab`).
- Produces:

```rust
// settings.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings { pub restore: bool }          // Default: restore = true
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalSettings { pub command: Option<Vec<String>> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSet { pub name: String, pub tabs: Vec<String> }   // tokenized paths or "drives"
// Settings: pub session: SessionSettings, pub terminal: TerminalSettings, pub tab_sets: Vec<TabSet>
// State:    pub session: gezik_core::nav::Session
pub(crate) fn parse_tab_set(value: &toml::Value) -> Result<TabSet, String>;
pub fn tab_set_to_toml(set: &TabSet) -> toml::Table;
// settings_edit.rs
pub fn with_tab_sets(text: &str, sets: &[TabSet]) -> Result<String, String>;
// settings_writer.rs
SettingsChange::TabSets(Vec<TabSet>)
// store.rs
impl ConfigStore { pub fn save_tab_sets(&self, sets: &[TabSet]) -> Result<(), Warning>; }
// shortcuts.rs
pub enum Action { …, OpenTerminal, OpenTerminalAdmin, CopyPath, SaveTabSet }   // ALL: [Action; 50]
```

**Behavior:**
- Türler (`HistorySettings`'in altına):

```rust
/// `[session]`: the tabs of last time at start (kept in state.toml).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    pub restore: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        SessionSettings { restore: true }
    }
}

/// `[terminal]`: the command "Open terminal" runs instead of the one Gezik finds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalSettings {
    /// A program and its arguments; `{dir}` is the folder.
    pub command: Option<Vec<String>>,
}

/// A tab set (`[[tab-sets]]`): tabs opened together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSet {
    pub name: String,
    /// Paths with `{home}`-style tokens, or "drives" (This PC).
    pub tabs: Vec<String>,
}
```
  `Settings`'e `session: SessionSettings`, `terminal: TerminalSettings`, `tab_sets: Vec<TabSet>` (`history`'den sonra; `Default`'ta varsayılanlar).
- Ayrıştırma (`Settings::parse`'ta `history` bloğundan sonra `session` ve `terminal`, `filters` bloğundan sonra `tab-sets`):

```rust
        match table.get("session") {
            None => {}
            Some(value) => match value.as_table() {
                Some(session) => settings.session = parse_session(session, file, warnings),
                None => warnings.push(Warning::new(file, format!("session: expected a table, got {value}"))),
            },
        }
        match table.get("terminal") {
            None => {}
            Some(value) => match value.as_table() {
                Some(terminal) => settings.terminal = parse_terminal(terminal, file, warnings),
                None => warnings.push(Warning::new(file, format!("terminal: expected a table, got {value}"))),
            },
        }
```

```rust
        if let Some(value) = table.get("tab-sets") {
            match value.as_array() {
                None => {
                    warnings.push(Warning::new(file, format!("tab-sets: expected [[tab-sets]] tables, got {value}")))
                }
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_tab_set(item) {
                            // Names are told apart ignoring case, as the menu and "Save tabs as…" do.
                            Ok(set) if settings.tab_sets.iter().any(|s| same_filter_name(&s.name, &set.name)) => {
                                warnings.push(Warning::new(
                                    file,
                                    format!("tab-sets[{}]: \"{}\" is already used; this one is left out", i + 1, set.name),
                                ));
                            }
                            Ok(set) => settings.tab_sets.push(set),
                            Err(err) => warnings.push(Warning::new(file, format!("tab-sets[{}]: {err}", i + 1))),
                        }
                    }
                }
            }
        }
```

```rust
fn parse_session(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> SessionSettings {
    let mut out = SessionSettings::default();
    if let Some(value) = table.get("restore") {
        match value.as_bool() {
            Some(on) => out.restore = on,
            None => warnings.push(Warning::new(file, format!("session.restore: expected true or false, got {value}"))),
        }
    }
    out
}

/// `[terminal] command`: a list of text with `{dir}` the only placeholder; an empty list is
/// none (Gezik finds a terminal).
fn parse_terminal(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> TerminalSettings {
    let Some(value) = table.get("command") else { return TerminalSettings::default() };
    let Ok(command) = string_list(value, "command") else {
        warnings.push(Warning::new(file, format!("terminal.command: expected a list of text, got {value}")));
        return TerminalSettings::default();
    };
    if command.is_empty() {
        return TerminalSettings::default();
    }
    match gezik_core::batch::convert::check_dir_command(&command) {
        Ok(()) => TerminalSettings { command: Some(command) },
        Err(err) => {
            warnings.push(Warning::new(file, format!("terminal.command: {err}; Gezik finds a terminal")));
            TerminalSettings::default()
        }
    }
}

/// One `[[tab-sets]]` entry; the error is why it is left out (a path with `..` leaves out the
/// whole set, which Gezik then keeps in the file as it is).
pub(crate) fn parse_tab_set(value: &toml::Value) -> Result<TabSet, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    let name =
        table.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|n| !n.is_empty()).ok_or("name is missing")?;
    let tabs = match table.get("tabs") {
        None => return Err("tabs is missing".to_owned()),
        Some(value) => string_list(value, "tabs")?,
    };
    if tabs.is_empty() {
        return Err("tabs is missing".to_owned());
    }
    if tabs.iter().any(|tab| tab.trim().is_empty()) {
        return Err("tabs has an empty path".to_owned());
    }
    if let Some(bad) = tabs.iter().find(|tab| crate::paths::has_parent_segment(tab)) {
        return Err(format!("\"{bad}\" must not contain \"..\""));
    }
    Ok(TabSet { name: name.to_owned(), tabs })
}

pub fn tab_set_to_toml(set: &TabSet) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String(set.name.clone()));
    table.insert("tabs".into(), toml::Value::Array(set.tabs.iter().cloned().map(toml::Value::String).collect()));
    table
}
```
- Durum (`State`'e `pub session: Session` (`history`'den sonra), `use gezik_core::nav::{Location, Session, SessionTab};` ve `use std::path::PathBuf;`):

```rust
/// state.toml's `[session]` (spec 5.1): the tabs in order and the one in front. An entry
/// without a path (or `drives = true`), or with a relative one, is left out, and `active`
/// counts the kept ones (the first if its entry was left out).
fn session_state(value: Option<&toml::Value>) -> Session {
    let Some(table) = value.and_then(|v| v.as_table()) else { return Session::default() };
    let wanted = table.get("active").and_then(|v| v.as_integer()).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let mut session = Session::default();
    let mut active = None;
    for (i, item) in table.get("tabs").and_then(|v| v.as_array()).into_iter().flatten().enumerate() {
        let Some(tab) = item.as_table() else { continue };
        let location = if tab.get("drives").and_then(|v| v.as_bool()) == Some(true) {
            Location::Drives
        } else {
            match tab.get("path").and_then(|v| v.as_str()).map(PathBuf::from) {
                Some(path) if path.is_absolute() => Location::Path(path),
                _ => continue,
            }
        };
        if i == wanted {
            active = Some(session.tabs.len());
        }
        let locked = tab.get("locked").and_then(|v| v.as_bool()).unwrap_or(false);
        session.tabs.push(SessionTab { location, locked });
    }
    session.active = active.unwrap_or(0);
    session
}
```
  `State::parse`'ta `session: session_state(table.get("session"))`; `to_toml`'da (`history`'den sonra):

```rust
        if !self.session.is_empty() {
            let tabs = self
                .session
                .tabs
                .iter()
                .map(|tab| {
                    let mut table = toml::Table::new();
                    match &tab.location {
                        Location::Path(path) => {
                            table.insert("path".into(), toml::Value::String(path.to_string_lossy().into_owned()));
                        }
                        Location::Drives => {
                            table.insert("drives".into(), toml::Value::Boolean(true));
                        }
                    }
                    if tab.locked {
                        table.insert("locked".into(), toml::Value::Boolean(true));
                    }
                    toml::Value::Table(table)
                })
                .collect();
            let mut session = toml::Table::new();
            session.insert("active".into(), toml::Value::Integer(i64::try_from(self.session.active).unwrap_or(0)));
            session.insert("tabs".into(), toml::Value::Array(tabs));
            root.insert("session".into(), toml::Value::Table(session));
        }
```
- `settings_edit.rs`:

```rust
/// Returns `text` with `[[tab-sets]]` replaced by `sets`; the rest stays, and so do the
/// entries that do not read as a tab set (see `with_rename_presets`).
pub fn with_tab_sets(text: &str, sets: &[crate::settings::TabSet]) -> Result<String, String> {
    let tables = sets.iter().map(crate::settings::tab_set_to_toml).collect();
    with_tables(text, "tab-sets", tables, |item| crate::settings::parse_tab_set(item).is_ok())
}
```
- `settings_writer.rs`: `SettingsChange::TabSets(Vec<TabSet>)` (belge: "The tab sets (`[[tab-sets]]`)."), `apply`'da `SettingsChange::TabSets(sets) => with_tab_sets(text, sets)`; modül belgesine "the tab sets" eklenir.
- `store.rs`:

```rust
    /// Writes the tab sets into `settings.toml` now.
    pub fn save_tab_sets(&self, sets: &[crate::settings::TabSet]) -> Result<(), Warning> {
        self.save_settings(&SettingsChange::TabSets(sets.to_vec()))
    }
```
- `shortcuts.rs`: `ClearHistory`'den sonra dört eylem (belge yorumlarıyla: "Opens a terminal in the focused folder, else the one shown (7a).", "The same as administrator (Windows only).", "Copies the selected items' full paths, else the folder's.", "Saves the open tabs as a tab set."); `ALL: [Action; 50]` sonuna aynı sırayla; adlar `open-terminal`, `open-terminal-admin`, `copy-path`, `save-tab-set`; varsayılanlar:

```rust
            // Shift+F4 is Dolphin's; Ctrl+Alt+T second: AltGr types with it on some layouts
            // (₺ on Turkish Q) and GNOME takes it system-wide (spec 10.3).
            (Action::OpenTerminal, Platform::Mac) => &["mod+alt+t"],
            (Action::OpenTerminal, Platform::Other) => &["shift+f4", "ctrl+alt+t"],
            (Action::OpenTerminalAdmin, _) => &[],
            (Action::CopyPath, Platform::Mac) => &["mod+alt+c"],
            (Action::CopyPath, Platform::Other) => &["ctrl+shift+c"],
            (Action::SaveTabSet, _) => &[],
```
- Şablon (`templates/settings.toml`): `[history]` bloğundan sonra, `[shortcuts]`'tan önce:

```toml
[session]
# Open the tabs of last time at start, in order, with the one in front and the locks (kept
# in state.toml, this computer's own). false: one tab in start-folder, the kept tabs forgotten.
restore = true

[terminal]
# The terminal "Open terminal" runs: a program and its arguments, {dir} is the folder ("{{"
# and "}}" are braces). Unset: Gezik finds one (Windows Terminal, else PowerShell; Terminal on
# macOS; $TERMINAL, x-terminal-emulator or a terminal it knows on Linux).
# command = ["wezterm", "start", "--cwd", "{dir}"]
```
  `[shortcuts]` yorumlarının sonuna (`clear-history` satırından sonra, boş satırsız):

```toml
# open-terminal = ["shift+f4", "ctrl+alt+t"]   # macOS: "mod+alt+t"
# open-terminal-admin = ""   # Windows only: the terminal as administrator
# copy-path = "ctrl+shift+c"   # the selected items' paths as text; macOS: "mod+alt+c"
# save-tab-set = ""        # save the open tabs under a name
```
  Kayıtlı süzgeç örneğinden (`# pattern = …` satırı) sonra bir boş satır ve:

```toml
# Tab sets: tabs opened together from a tab's right-click menu ("Open tab set >"; "Save tabs
# as…" there writes them here). "drives" is This PC; tokens like {home} work on every OS.
# [[tab-sets]]
# name = "Release"
# tabs = ["{documents}/Projects", "{downloads}", "drives"]
```
- `gezik` crate'inde derleme için: `actions.rs`'te "Not 6a's" listesine `| Action::OpenTerminal | Action::OpenTerminalAdmin | Action::CopyPath | Action::SaveTabSet` (Task 7 ve 9 doldurur); `main.rs` `handle_key`'in 6a/6b eylem koluna (`| Action::ClearHistory`'nin yanına) aynı dördü; `keys.rs`'te `every_default_is_reachable_on_windows_and_linux`'a `Action::OpenTerminal => "shift+f4"`, `Action::CopyPath => "ctrl+shift+c"`, `continue` koluna `| Action::OpenTerminalAdmin | Action::SaveTabSet`; `other_event`'in `Key::F` eşlemesine `4 => SlintKey::F4`.

- [ ] **Step 1: Write the failing tests**

`settings.rs`:

```rust
#[test]
fn session_terminal_and_tab_sets_are_read() {
    let (settings, warnings) = parse(
        "[session]\nrestore = false\n\n[terminal]\ncommand = [\"wezterm\", \"start\", \"--cwd\", \"{dir}\"]\n\n\
         [[tab-sets]]\nname = \" Release \"\ntabs = [\"D:/Work/gezik\", \"{downloads}\", \"drives\"]\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!settings.session.restore);
    assert_eq!(settings.terminal.command.as_ref().map(Vec::len), Some(4));
    assert_eq!(
        settings.tab_sets,
        [TabSet { name: "Release".into(), tabs: vec!["D:/Work/gezik".into(), "{downloads}".into(), "drives".into()] }]
    );
    let defaults = Settings::default();
    assert!(defaults.session.restore && defaults.terminal.command.is_none() && defaults.tab_sets.is_empty());
}

#[test]
fn bad_session_terminal_and_tab_sets_warn() {
    let (settings, warnings) = parse(
        "[session]\nrestore = \"yes\"\n\n[terminal]\ncommand = [\"x\", \"{foo}\"]\n\n\
         [[tab-sets]]\nname = \"\"\ntabs = [\"/a\"]\n\
         [[tab-sets]]\nname = \"Up\"\ntabs = [\"{home}/../x\"]\n\
         [[tab-sets]]\nname = \"None\"\ntabs = []\n\
         [[tab-sets]]\nname = \"Good\"\ntabs = [\"/a\"]\n\
         [[tab-sets]]\nname = \"GOOD\"\ntabs = [\"/b\"]\n\
         [[tab-sets]]\nname = \"Num\"\ntabs = [3]\n",
    );
    assert!(settings.session.restore, "a bad value keeps the default");
    assert_eq!(settings.terminal.command, None);
    assert_eq!(settings.tab_sets.len(), 1);
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "session.restore: expected true or false, got \"yes\"",
            "terminal.command: unknown placeholder {foo}; Gezik finds a terminal",
            "tab-sets[1]: name is missing",
            "tab-sets[2]: \"{home}/../x\" must not contain \"..\"",
            "tab-sets[3]: tabs is missing",
            "tab-sets[5]: \"GOOD\" is already used; this one is left out",
            "tab-sets[6]: tabs must be a list of text, got 3",
        ]
    );
    let (_, warnings) = parse("terminal = 1\n");
    assert!(warnings[0].message.starts_with("terminal: expected a table"), "{warnings:?}");
    let (_, warnings) = parse("[terminal]\ncommand = \"wt\"\n");
    assert!(warnings[0].message.starts_with("terminal.command: expected a list of text"), "{warnings:?}");
    let (settings, warnings) = parse("[terminal]\ncommand = []\n");
    assert!(warnings.is_empty() && settings.terminal.command.is_none());
}

#[test]
fn the_session_round_trips_in_state() {
    use gezik_core::nav::{Location, Session, SessionTab};
    let state = State {
        session: Session {
            tabs: vec![
                SessionTab { location: Location::Path(std::env::temp_dir().join("gezik ş 'x'")), locked: true },
                SessionTab { location: Location::Drives, locked: false },
            ],
            active: 1,
        },
        ..State::default()
    };
    assert_eq!(State::parse(&state.to_toml()), state);
    assert!(!State::default().to_toml().contains("session"));
}

#[test]
fn broken_session_entries_are_left_out() {
    use gezik_core::nav::{Location, Session, SessionTab};
    let abs = std::env::temp_dir().join("a");
    let text = format!(
        "[session]\nactive = 3\n\n[[session.tabs]]\npath = \"relative/x\"\n\n[[session.tabs]]\nlocked = true\n\n\
         [[session.tabs]]\ndrives = true\n\n[[session.tabs]]\npath = {}\nlocked = \"yes\"\n",
        toml::Value::String(abs.to_string_lossy().into_owned())
    );
    let session = State::parse(&text).session;
    assert_eq!(
        session.tabs,
        [SessionTab { location: Location::Drives, locked: false }, SessionTab { location: Location::Path(abs), locked: false }]
    );
    assert_eq!(session.active, 1, "active counts the kept entries");
    let gone = State::parse("[session]\nactive = 0\n\n[[session.tabs]]\npath = \"x\"\n\n[[session.tabs]]\ndrives = true\n");
    assert_eq!(gone.session.active, 0, "the active entry was left out: the first");
    assert_eq!(State::parse("[session]\nactive = -2\n").session, Session::default());
}

#[test]
fn the_template_tab_set_example_reads_once_uncommented() {
    let template = include_str!("../templates/settings.toml");
    let start = template.find("# [[tab-sets]]").expect("the template has a tab set example");
    let example: String = template[start..]
        .lines()
        .take_while(|line| line.starts_with('#'))
        .map(|line| format!("{}\n", line.strip_prefix("# ").unwrap_or(line)))
        .collect();
    let (settings, warnings) = parse(&example);
    assert!(warnings.is_empty(), "{warnings:?}\n{example}");
    assert_eq!(settings.tab_sets.len(), 1);
}
```
  `the_template_reads_with_the_defaults`'a `assert_eq!(settings.session, SessionSettings::default()); assert_eq!(settings.terminal, TerminalSettings::default()); assert!(settings.tab_sets.is_empty());`.

`settings_edit.rs`:

```rust
#[test]
fn tab_sets_are_written_keeping_the_rest() {
    use crate::settings::{Settings, TabSet};
    let set = TabSet { name: "Release".into(), tabs: vec!["{downloads}".into(), "drives".into()] };
    let text = "# mine\n\n[[tab-sets]]\nname = \"old\"\ntabs = [\"/a\"]\n\n\
                [[tab-sets]]\nname = \"broken\" # mine\ntabs = [\"{home}/../x\"]\n";
    let out = with_tab_sets(text, std::slice::from_ref(&set)).unwrap();
    assert!(out.contains("# mine") && out.contains("name = \"broken\" # mine"), "{out}");
    assert!(!out.contains("\"old\""), "a valid one is replaced: {out}");
    let mut warnings = Vec::new();
    let settings = Settings::parse("settings.toml", &out, &mut warnings);
    assert_eq!(settings.tab_sets, [set]);
    assert_eq!(warnings.len(), 1, "the broken one still warns: {warnings:?}");
    let template = include_str!("../templates/settings.toml");
    let one = [TabSet { name: "X".into(), tabs: vec!["/x".into()] }];
    let back = with_tab_sets(&with_tab_sets(template, &one).unwrap(), &[]).unwrap();
    for line in template.lines().filter(|l| l.starts_with('#')) {
        assert!(back.lines().any(|b| b == line), "{line} lost:\n{back}");
    }
}
```

`shortcuts.rs`:

```rust
#[test]
fn the_daily_actions_have_their_keys() {
    let other = Shortcuts::defaults(Platform::Other);
    assert_eq!(other.action_for(&chord("shift+f4")), Some(Action::OpenTerminal));
    assert_eq!(other.action_for(&chord("ctrl+alt+t")), Some(Action::OpenTerminal));
    assert_eq!(other.chord_for(Action::OpenTerminal), Some(chord("shift+f4")), "the one the menus show");
    assert_eq!(other.action_for(&chord("ctrl+shift+c")), Some(Action::CopyPath));
    assert_eq!(other.chord_for(Action::OpenTerminalAdmin), None);
    assert_eq!(other.chord_for(Action::SaveTabSet), None);
    let mac = Shortcuts::defaults(Platform::Mac);
    let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
    assert_eq!(mac.action_for(&mac_chord("mod+alt+t")), Some(Action::OpenTerminal));
    assert_eq!(mac.action_for(&mac_chord("mod+alt+c")), Some(Action::CopyPath));
    for name in ["open-terminal", "open-terminal-admin", "copy-path", "save-tab-set"] {
        assert!(Action::from_name(name).is_some(), "{name}");
    }
    for platform in [Platform::Mac, Platform::Other] {
        let defaults = Shortcuts::defaults(platform);
        for action in [Action::OpenTerminal, Action::CopyPath] {
            let chord = defaults.chord_for(action).unwrap();
            assert_eq!(fixed_owner(&chord, platform), None, "{}", action.name());
        }
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-config` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki türler, ayrıştırma, yazma, şablon). Var olan şablon testleri (`the_template_shortcut_examples_are_the_defaults_off_macos`, `presets_go_after_the_template_comments`, `filters_and_presets_together_leave_the_template_as_it_was`, `the_template_filter_example_reads_once_uncommented`, `the_template_command_examples_read_without_warnings_once_uncommented`) değişmeden geçmeli; `defaults_cover_every_action` yeni varsayılanların hiçbir eskisiyle çakışmadığını denetler.
- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS (`gezik` crate'i yeni kollarla derlenir; `keys` ulaşılabilirlik testi Shift+F4'ü ve Ctrl+Shift+C'yi bulur).
- [ ] **Step 5: Commit** "Read [session], [terminal] and [[tab-sets]], keep the session in state.toml, and name the 7a actions".

---

### Task 4: `gezik-platform` — terminal seçimi ve başlatma, eşlenmiş sürücünün payı

**Files:**
- Create: `crates/gezik-platform/src/terminal.rs`
- Modify: `crates/gezik-platform/src/lib.rs` (`pub mod terminal;`), `crates/gezik-platform/src/fs/windows.rs` (`mapped_remote`), `crates/gezik-platform/src/fs/unix.rs` (`mapped_remote`), `crates/gezik-platform/src/fs/mod.rs` (dışa açma), `crates/gezik-platform/Cargo.toml` (`Win32_NetworkManagement_WNet`, `Win32_Graphics_Gdi`'den sonra)

**Interfaces:**
- Consumes: Task 1 (`expand_dir_command`, `path_text::unc_path`); `fs::describe`.
- Produces:

```rust
// terminal.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { Windows, Mac, Linux }
impl Os { pub fn current() -> Os; }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch { pub program: OsString, pub args: Vec<OsString>, pub dir: PathBuf, pub elevated: bool }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError { NotFound, OnlyOnWindows, Cancelled, Failed(String) }
pub struct Lookup<'a> {
    pub os: Os,
    pub command: Option<&'a [String]>,          // [terminal] command
    pub terminal_var: Option<&'a str>,          // $TERMINAL
    pub desktop: Option<&'a str>,               // XDG_CURRENT_DESKTOP
    pub found: &'a dyn Fn(&str) -> bool,        // runs: on PATH or a path to a program
    pub real_name: &'a dyn Fn(&str) -> Option<String>,   // the program a link on PATH is
}
pub const KNOWN: [(&str, &[&str]); 9];
pub fn choose(dir: &Path, admin: bool, lookup: &Lookup<'_>) -> Result<Launch, TerminalError>;
pub fn windows_command_line(args: &[String]) -> String;
pub fn wt_dir(dir: &str) -> String;
pub fn open(dir: &Path, admin: bool, command: Option<&[String]>) -> Result<(), TerminalError>;
// fs
pub fn mapped_remote(letter: char) -> Option<String>;
```

- [ ] **Step 1: Write the failing tests** (`terminal.rs`'in sonunda)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn no_link(_: &str) -> Option<String> {
        None
    }

    fn look<'a>(os: Os, found: &'a dyn Fn(&str) -> bool) -> Lookup<'a> {
        Lookup { os, command: None, terminal_var: None, desktop: None, found, real_name: &no_link }
    }

    fn args(launch: &Launch) -> Vec<String> {
        launch.args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    fn program(launch: &Launch) -> String {
        launch.program.to_string_lossy().into_owned()
    }

    #[test]
    fn windows_terminal_first_then_powershell() {
        let dir = Path::new(r"C:\Users\a\x;y");
        let all = |_: &str| true;
        let wt = choose(dir, false, &look(Os::Windows, &all)).unwrap();
        assert_eq!((program(&wt), args(&wt)), ("wt.exe".to_owned(), vec!["-d".to_owned(), r"C:\Users\a\x\;y".to_owned()]));
        assert_eq!((wt.dir.as_path(), wt.elevated), (dir, false));
        let no_wt = |p: &str| p != "wt";
        assert_eq!(program(&choose(dir, false, &look(Os::Windows, &no_wt)).unwrap()), "pwsh.exe");
        let only_ps = |p: &str| p == "powershell";
        assert_eq!(program(&choose(dir, false, &look(Os::Windows, &only_ps)).unwrap()), "powershell.exe");
        let none = |_: &str| false;
        assert_eq!(choose(dir, false, &look(Os::Windows, &none)), Err(TerminalError::NotFound));
    }

    #[test]
    fn powershell_always_gets_its_folder_as_a_literal_path() {
        // Windows PowerShell 5.1 does not start in a folder with `[` (probe 4.1).
        let dir = Path::new(r"C:\[köşeli] it's");
        let only_ps = |p: &str| p == "powershell";
        for admin in [false, true] {
            let ps = choose(dir, admin, &look(Os::Windows, &only_ps)).unwrap();
            assert_eq!(ps.elevated, admin);
            assert_eq!(args(&ps), ["-NoExit", "-Command", "Set-Location -LiteralPath 'C:\\[köşeli] it''s'"]);
        }
        let all = |_: &str| true;
        assert_eq!(args(&choose(dir, true, &look(Os::Windows, &all)).unwrap()), ["-d", r"C:\[köşeli] it's"]);
        assert_eq!(choose(dir, true, &look(Os::Linux, &all)), Err(TerminalError::OnlyOnWindows));
        assert_eq!(choose(dir, true, &look(Os::Mac, &all)), Err(TerminalError::OnlyOnWindows));
    }

    #[test]
    fn windows_command_lines_split_back_into_the_arguments() {
        let line = |a: &[&str]| windows_command_line(&a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(line(&["-d", r"C:\"]), r"-d C:\");
        assert_eq!(line(&["-d", r"C:\a b\"]), r#"-d "C:\a b\\""#);
        assert_eq!(line(&[r#"say "hi""#, ""]), r#""say \"hi\"" """#);
        assert_eq!(line(&[r"a\\b", "tab\there"]), "a\\\\b \"tab\there\"");
        assert_eq!(
            line(&["-NoExit", "-Command", "Set-Location -LiteralPath 'C:\\it''s'"]),
            r#"-NoExit -Command "Set-Location -LiteralPath 'C:\it''s'""#
        );
    }

    #[test]
    fn macos_opens_terminal_app() {
        let none = |_: &str| false;
        let mac = choose(Path::new("/Users/a/İş"), false, &look(Os::Mac, &none)).unwrap();
        assert_eq!(
            (program(&mac), args(&mac)),
            ("/usr/bin/open".to_owned(), vec!["-a".to_owned(), "Terminal".to_owned(), "/Users/a/İş".to_owned()])
        );
    }

    #[test]
    fn linux_tries_terminal_then_the_debian_one_then_the_known_ones() {
        let dir = Path::new("/home/a/my dir");
        let all = |_: &str| true;
        let mut lookup = look(Os::Linux, &all);
        lookup.terminal_var = Some("  foot --app-id x ");
        let t = choose(dir, false, &lookup).unwrap();
        assert_eq!((program(&t), args(&t)), ("foot".to_owned(), vec!["--app-id".to_owned(), "x".to_owned()]));
        let not_missing = |p: &str| p != "missing";
        let mut lookup = look(Os::Linux, &not_missing);
        lookup.terminal_var = Some("missing");
        let x = choose(dir, false, &lookup).unwrap();
        assert_eq!((program(&x), x.args.len()), ("x-terminal-emulator".to_owned(), 0), "a $TERMINAL not found is passed over");
        let kitty = |p: &str| p == "kitty" || p == "xterm";
        let k = choose(dir, false, &look(Os::Linux, &kitty)).unwrap();
        assert_eq!((program(&k), args(&k)), ("kitty".to_owned(), vec!["--directory".to_owned(), "/home/a/my dir".to_owned()]));
        let xterm = |p: &str| p == "xterm";
        let x = choose(dir, false, &look(Os::Linux, &xterm)).unwrap();
        assert_eq!((program(&x), x.args.len(), x.dir.as_path()), ("xterm".to_owned(), 0, dir));
        let none = |_: &str| false;
        assert_eq!(choose(dir, false, &look(Os::Linux, &none)), Err(TerminalError::NotFound));
    }

    #[test]
    fn the_debian_link_gets_the_flags_of_the_terminal_it_is() {
        let dir = Path::new("/d");
        let debian = |p: &str| p == "x-terminal-emulator";
        let gnome = |_: &str| Some("gnome-terminal.wrapper".to_owned());
        let mut lookup = look(Os::Linux, &debian);
        lookup.real_name = &gnome;
        let g = choose(dir, false, &lookup).unwrap();
        assert_eq!((program(&g), args(&g)), ("x-terminal-emulator".to_owned(), vec!["--working-directory=/d".to_owned()]));
        let unknown = |_: &str| Some("st".to_owned());
        lookup.real_name = &unknown;
        assert!(choose(dir, false, &lookup).unwrap().args.is_empty(), "an unknown one gets the working folder only");
    }

    #[test]
    fn the_desktops_own_terminal_comes_first() {
        let dir = Path::new("/d");
        let known = |p: &str| p != "x-terminal-emulator";
        let pick = |desktop: Option<&str>| {
            let mut lookup = look(Os::Linux, &known);
            lookup.desktop = desktop;
            let launch = choose(dir, false, &lookup).unwrap();
            (program(&launch), args(&launch))
        };
        assert_eq!(pick(Some("KDE")), ("konsole".to_owned(), vec!["--workdir".to_owned(), "/d".to_owned()]));
        assert_eq!(pick(Some("XFCE")).0, "xfce4-terminal");
        assert_eq!(
            pick(Some("ubuntu:GNOME")),
            ("ptyxis".to_owned(), vec!["--new-window".to_owned(), "--working-directory".to_owned(), "/d".to_owned()])
        );
        assert_eq!(pick(None), ("gnome-terminal".to_owned(), vec!["--working-directory=/d".to_owned()]));
        assert_eq!(linux_order(Some("GNOME")).len(), KNOWN.len(), "each once");
    }

    #[test]
    fn a_command_in_settings_wins_everywhere() {
        let none = |_: &str| false;
        let command = vec!["wezterm".to_owned(), "start".to_owned(), "--cwd".to_owned(), "{dir}".to_owned()];
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let mut lookup = look(os, &none);
            lookup.command = Some(&command);
            let launch = choose(Path::new("/w {x}"), false, &lookup).unwrap();
            assert_eq!(
                (program(&launch), args(&launch)),
                ("wezterm".to_owned(), vec!["start".to_owned(), "--cwd".to_owned(), "/w {x}".to_owned()])
            );
        }
        let bad = vec!["x".to_owned(), "{in}".to_owned()];
        let mut lookup = look(Os::Linux, &none);
        lookup.command = Some(&bad);
        assert_eq!(
            choose(Path::new("/w"), false, &lookup),
            Err(TerminalError::Failed("unknown placeholder {in}".to_owned()))
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_local_drive_is_not_mapped() {
        assert_eq!(crate::fs::mapped_remote('C'), None);
        assert_eq!(crate::fs::mapped_remote('1'), None);
    }

    /// Opens a real terminal: run by hand (`cargo test -p gezik-platform terminal -- --ignored`).
    #[test]
    #[ignore = "opens a terminal window"]
    fn opens_a_terminal_in_the_temp_folder() {
        open(&std::env::temp_dir(), false, None).unwrap();
    }
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-platform terminal` → FAIL (modül yok).

- [ ] **Step 3: Implement.** `terminal.rs` (Windows başlatması probe'un `term.rs`'indeki `shell_execute`'tan):

```rust
//! "Open terminal" (spec 3): which terminal opens a folder on this system, and opening it so
//! that it lives on after Gezik. The choice is pure (`choose`, with the environment and PATH
//! given in a `Lookup`), so every rule is tested on every system; `open` asks the real ones.
//! Windows' and Linux's rules were tried in docs/superpowers/notes/2026-10-08-gunluk-kolayliklar-probe.md.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        }
    }
}

/// What to run: a program (a name on PATH or a path), its arguments, its working folder, and
/// whether as administrator (Windows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub dir: PathBuf,
    pub elevated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    NotFound,
    /// "as administrator" elsewhere than Windows.
    OnlyOnWindows,
    /// The UAC question was answered No.
    Cancelled,
    Failed(String),
}

/// The environment `choose` decides by.
pub struct Lookup<'a> {
    pub os: Os,
    /// `[terminal] command`.
    pub command: Option<&'a [String]>,
    /// `$TERMINAL` (Linux).
    pub terminal_var: Option<&'a str>,
    /// `XDG_CURRENT_DESKTOP` (Linux), `:`-separated.
    pub desktop: Option<&'a str>,
    /// Whether a program runs: a name on PATH (`.exe` added on Windows) or a path.
    pub found: &'a dyn Fn(&str) -> bool,
    /// The file name of the program a link on PATH stands for (`x-terminal-emulator`).
    pub real_name: &'a dyn Fn(&str) -> Option<String>,
}

/// The terminals Gezik knows on Linux, in the order it tries them, with the flags that give
/// each its folder (the working folder alone does not hold in all: gnome-terminal's server
/// has its own). Checked with each one in probe 4.3.
pub const KNOWN: [(&str, &[&str]); 9] = [
    ("gnome-terminal", &["--working-directory={dir}"]),
    // Without --new-window, Ptyxis leaves --working-directory out.
    ("ptyxis", &["--new-window", "--working-directory", "{dir}"]),
    ("kgx", &["--working-directory", "{dir}"]),
    ("konsole", &["--workdir", "{dir}"]),
    ("xfce4-terminal", &["--working-directory={dir}"]),
    ("kitty", &["--directory", "{dir}"]),
    ("alacritty", &["--working-directory", "{dir}"]),
    ("foot", &["--working-directory={dir}"]),
    ("xterm", &[]),
];

/// The known terminals, the desktop's own first.
fn linux_order(desktop: Option<&str>) -> Vec<&'static str> {
    let desktops: Vec<String> = desktop.unwrap_or("").split(':').map(str::to_ascii_lowercase).collect();
    let has = |name: &str| desktops.iter().any(|d| d == name);
    let mut order: Vec<&'static str> = Vec::new();
    if has("kde") {
        order.push("konsole");
    }
    if has("xfce") {
        order.push("xfce4-terminal");
    }
    if has("gnome") {
        order.extend(["ptyxis", "kgx", "gnome-terminal"]);
    }
    for (name, _) in KNOWN {
        if !order.contains(&name) {
            order.push(name);
        }
    }
    order
}

/// The flags that give known terminal `name` its folder (a `.wrapper` ending is Debian's).
fn flags_of(name: &str, dir: &Path) -> Vec<OsString> {
    let name = name.strip_suffix(".wrapper").unwrap_or(name);
    let flags = KNOWN.iter().find(|(known, _)| *known == name).map_or(&[][..], |(_, flags)| *flags);
    flags.iter().map(|flag| with_dir(flag, dir)).collect()
}

fn with_dir(flag: &str, dir: &Path) -> OsString {
    match flag.split_once("{dir}") {
        Some((before, after)) => {
            let mut out = OsString::from(before);
            out.push(dir);
            out.push(after);
            out
        }
        None => flag.into(),
    }
}

/// `dir` for `wt -d`: Windows Terminal takes `;` for its command separator even inside an
/// argument, `\;` for itself (probe 4.1).
pub fn wt_dir(dir: &str) -> String {
    dir.replace(';', r"\;")
}

/// `args` as one Windows command line, the way `CommandLineToArgvW` splits it back.
pub fn windows_command_line(args: &[String]) -> String {
    let mut out = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            out.push_str(arg);
            continue;
        }
        out.push('"');
        let mut backslashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    out.extend(std::iter::repeat_n('\\', backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
    }
    out
}

/// The terminal for `dir` (spec 3.2). `admin` is only for Windows.
pub fn choose(dir: &Path, admin: bool, lookup: &Lookup<'_>) -> Result<Launch, TerminalError> {
    if admin && lookup.os != Os::Windows {
        return Err(TerminalError::OnlyOnWindows);
    }
    let launch = |program: &str, args: Vec<OsString>| Launch {
        program: program.into(),
        args,
        dir: dir.to_path_buf(),
        elevated: admin,
    };
    if let Some(command) = lookup.command {
        let mut args = gezik_core::batch::convert::expand_dir_command(command, dir).map_err(TerminalError::Failed)?;
        let program = args.remove(0);
        return Ok(Launch { program, args, dir: dir.to_path_buf(), elevated: admin });
    }
    let found = lookup.found;
    match lookup.os {
        Os::Windows => {
            let text = dir.to_string_lossy();
            if found("wt") {
                return Ok(launch("wt.exe", vec!["-d".into(), wt_dir(&text).into()]));
            }
            let shell = if found("pwsh") {
                "pwsh.exe"
            } else if found("powershell") {
                "powershell.exe"
            } else {
                return Err(TerminalError::NotFound);
            };
            // Given as an argument: PowerShell 5.1 does not start in a folder with `[`, and an
            // elevated one does not always get its working folder (probe 4.1).
            let go = format!("Set-Location -LiteralPath '{}'", text.replace('\'', "''"));
            Ok(launch(shell, vec!["-NoExit".into(), "-Command".into(), go.into()]))
        }
        Os::Mac => Ok(launch("/usr/bin/open", vec!["-a".into(), "Terminal".into(), dir.as_os_str().to_owned()])),
        Os::Linux => {
            if let Some(var) = lookup.terminal_var {
                let mut words = var.split_whitespace();
                if let Some(program) = words.next().filter(|program| found(program)) {
                    return Ok(launch(program, words.map(OsString::from).collect()));
                }
            }
            if found("x-terminal-emulator") {
                let flags = (lookup.real_name)("x-terminal-emulator").map(|real| flags_of(&real, dir)).unwrap_or_default();
                return Ok(launch("x-terminal-emulator", flags));
            }
            linux_order(lookup.desktop)
                .into_iter()
                .find(|name| found(name))
                .map(|name| launch(name, flags_of(name, dir)))
                .ok_or(TerminalError::NotFound)
        }
    }
}

/// Whether `name` runs: a path to a program, or a program in a folder of PATH (`.exe` added on
/// Windows, where the Store's app aliases count: Rust 1.99 sees them as files, probe 4.1).
fn on_path(name: &str) -> bool {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return runnable(path);
    }
    let Some(dirs) = std::env::var_os("PATH") else { return false };
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&dirs).filter(|dir| dir.is_absolute()).any(|dir| runnable(&dir.join(&file)))
}

fn runnable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    meta.is_file()
}

/// The file name of the program `name` on PATH stands for, links followed.
fn real_name(name: &str) -> Option<String> {
    let dirs = std::env::var_os("PATH")?;
    std::env::split_paths(&dirs)
        .map(|dir| dir.join(name))
        .find(|path| runnable(path))
        .and_then(|path| std::fs::canonicalize(path).ok())
        .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// Opens a terminal in `dir` (`admin`: as administrator, Windows only). Looks through PATH, so
/// not on the UI thread. The terminal does not end with Gezik.
pub fn open(dir: &Path, admin: bool, command: Option<&[String]>) -> Result<(), TerminalError> {
    let os = Os::current();
    // An elevated session does not see the drives this user mapped: their share instead.
    let unc = (admin && os == Os::Windows)
        .then(|| gezik_core::path_text::unc_path(&dir.to_string_lossy(), &crate::fs::mapped_remote))
        .flatten()
        .map(PathBuf::from);
    let dir = unc.as_deref().unwrap_or(dir);
    let terminal_var = std::env::var("TERMINAL").ok();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let lookup = Lookup {
        os,
        command,
        terminal_var: terminal_var.as_deref(),
        desktop: desktop.as_deref(),
        found: &on_path,
        real_name: &real_name,
    };
    start(&choose(dir, admin, &lookup)?)
}

/// Through the Shell, as Explorer starts programs: a console program gets a console of its
/// own, nothing of Gezik's is handed on. "runas" asks UAC (probe 4.1: `ShellExecuteW`,
/// `ShellExecuteExW` would need the Registry feature).
#[cfg(windows)]
fn start(launch: &Launch) -> Result<(), TerminalError> {
    use windows::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::HSTRING;
    let args: Vec<String> = launch.args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let verb = HSTRING::from(if launch.elevated { "runas" } else { "open" });
    let result = unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        ShellExecuteW(
            None,
            &verb,
            &HSTRING::from(launch.program.as_os_str()),
            &HSTRING::from(windows_command_line(&args)),
            &HSTRING::from(launch.dir.as_os_str()),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        return Ok(());
    }
    let err = unsafe { GetLastError() };
    if err == ERROR_CANCELLED {
        return Err(TerminalError::Cancelled);
    }
    Err(TerminalError::Failed(crate::fs::describe(&std::io::Error::from_raw_os_error(err.0 as i32))))
}

/// A process of its own group (Ctrl+C where Gezik was started does not reach it), waited for
/// on a small thread so that one that exits at once (gnome-terminal hands its window to its
/// server) leaves no zombie.
#[cfg(unix)]
fn start(launch: &Launch) -> Result<(), TerminalError> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| TerminalError::Failed(crate::fs::describe(&err)))?;
    let _ = std::thread::Builder::new().name("gezik-terminal".into()).stack_size(64 * 1024).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
```

`fs/windows.rs`:

```rust
/// The share a mapped drive letter stands for (`Z` → `\\server\share`); `None` for a drive
/// that is not mapped. A persistent mapping that is not connected now still says its share.
pub fn mapped_remote(letter: char) -> Option<String> {
    use windows::Win32::Foundation::{ERROR_CONNECTION_UNAVAIL, NO_ERROR};
    use windows::Win32::NetworkManagement::WNet::WNetGetConnectionW;
    use windows::core::PWSTR;
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let local = HSTRING::from(format!("{}:", letter.to_ascii_uppercase()));
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    let result = unsafe { WNetGetConnectionW(&local, Some(PWSTR(buffer.as_mut_ptr())), &mut len) };
    if result != NO_ERROR && result != ERROR_CONNECTION_UNAVAIL {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    (end > 0).then(|| String::from_utf16_lossy(&buffer[..end]))
}
```
`fs/unix.rs`:

```rust
/// No drive letters here.
pub fn mapped_remote(_letter: char) -> Option<String> {
    None
}
```
`fs/mod.rs`'te iki `pub use` listesine `mapped_remote`. `lib.rs`: `pub mod terminal;` (`taskbar`'dan sonra).

- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-platform` → PASS; çapraz denetimler (Linux ve macOS `start`'ı derler). Elle: `cargo test -j 8 -p gezik-platform opens_a_terminal -- --ignored` → Windows Terminal (yoksa PowerShell) `%TEMP%`'te açılır.
- [ ] **Step 5: Commit** "Choose and open a terminal in a folder on every system, and find a mapped drive's share".

---

### Task 5: Pano — metin yazma (Windows, macOS, X11, Wayland)

**Files:**
- Modify: `crates/gezik-platform/src/clipboard.rs`, `crates/gezik-platform/src/linux/mod.rs` (`Backend::write_text`, `latin1`), `crates/gezik-platform/src/linux/x11.rs`, `crates/gezik-platform/src/linux/wayland.rs`

**Interfaces:**
- Produces: `gezik_platform::clipboard::write_text(text: &str) -> Result<(), ClipboardError>`; `Backend::write_text(&self, text: &str) -> Result<(), ClipboardError>`; X11 ve Wayland `Owned`'a `text: Option<String>`.

**Behavior:**
- `clipboard.rs`: `pub use imp::{clear, read_files, sequence, write_files, write_text};`; modül belgesine "and text (paths copied as text, spec 4.2)".
- Windows `imp`:

```rust
    pub fn write_text(text: &str) -> Result<(), ClipboardError> {
        let bytes = super::unicode_text(text);
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.map_err(failed)?;
        put(u32::from(CF_UNICODETEXT.0), &bytes)
    }
```
  (`CF_UNICODETEXT` `windows::Win32::System::Ole`'den, `CF_HDROP`'un yanında) ve modül düzeyinde:

```rust
/// `CF_UNICODETEXT` data: UTF-16 with a closing NUL.
#[cfg(windows)]
fn unicode_text(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}
```
- macOS `imp`:

```rust
    pub fn write_text(text: &str) -> Result<(), ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        CUT_AT.set(None);
        // NSPasteboardTypeString is an extern static.
        let written = unsafe { pasteboard.setString_forType(&NSString::from_str(text), objc2_app_kit::NSPasteboardTypeString) };
        if written { Ok(()) } else { Err(ClipboardError::Failed("the pasteboard refused the text".into())) }
    }
```
- Linux `imp`: `pub fn write_text(text: &str) -> Result<(), ClipboardError> { backend().ok_or(ClipboardError::Unsupported)?.write_text(text) }`.
- `linux/mod.rs`: `Backend`'e `fn write_text(&self, text: &str) -> Result<(), ClipboardError>;` ve (her platformda derlenen kısma):

```rust
/// `text` in Latin-1, for the X11 `STRING` target: other characters become `?`.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn latin1(text: &str) -> Vec<u8> {
    text.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect()
}
```
- X11 (`x11.rs`): atom listesine `TEXT,`; `Owned`'a `text: Option<String>` (belge: "Text instead of files (copied paths): offered as text only."); `write_files` ve `drag_out`'taki `Owned { … }`'lara `text: None`. Ortak sahiplenme:

```rust
impl X11 {
    /// Puts `owned` on the clipboard: Gezik's hidden window becomes its owner.
    fn own_clipboard(&self, owned: Owned) -> Result<(), ClipboardError> {
        let s = &self.0;
        s.state().clipboard = Some(owned);
        let set =
            s.conn.set_selection_owner(s.window, s.atoms.CLIPBOARD, CURRENT_TIME).is_ok() && s.conn.flush().is_ok();
        if set && s.owns(s.atoms.CLIPBOARD) {
            Ok(())
        } else {
            s.state().clipboard = None;
            Err(ClipboardError::Failed("cannot take the X11 clipboard".into()))
        }
    }
}
```
  `write_files` → `self.own_clipboard(Owned { paths: paths.to_vec(), cut, text: None })`; `write_text` → `self.own_clipboard(Owned { paths: Vec::new(), cut: false, text: Some(text.to_owned()) })`. `read_files`'ın sahiplik kolu `s.state().clipboard.clone().filter(|o| o.text.is_none()).map(…)`. `Shared`'a:

```rust
    /// The formats Gezik offers for `owned`, `TARGETS` first.
    fn targets(&self, owned: &Owned) -> Vec<Atom> {
        let a = &self.atoms;
        if owned.text.is_some() {
            vec![a.TARGETS, a.UTF8_STRING, a.TEXT_PLAIN, a.TEXT, AtomEnum::STRING.into()]
        } else {
            vec![a.TARGETS, a.URI_LIST, a.GNOME_FILES, a.KDE_CUT, a.UTF8_STRING, a.TEXT_PLAIN]
        }
    }
```
  `serve`'deki sabit `targets` dizisi `self.targets(&owned)` olur; `render`'ın başına:

```rust
        if let Some(text) = &owned.text {
            return if target == a.UTF8_STRING || target == a.TEXT_PLAIN || target == a.TEXT {
                Some(text.as_bytes().to_vec())
            } else if target == Atom::from(AtomEnum::STRING) {
                Some(super::latin1(text))
            } else {
                None
            };
        }
```
- Wayland (`wayland.rs`): `const TEXT_TYPES: [&str; 5] = [TEXT, "text/plain", "UTF8_STRING", "TEXT", "STRING"];`; `Owned`'a `text: Option<String>`; `render`'ın başına `if let Some(text) = &self.text { return TEXT_TYPES.contains(&mime).then(|| text.as_bytes().to_vec()); }`; `write_files`'ın gövdesi `fn own_clipboard(&self, owned: Owned, types: &[&str]) -> Result<(), ClipboardError>` olur (aynı kod, `create_source(owned.clone(), types)`); `write_files` → `own_clipboard(Owned { paths: paths.to_vec(), cut, text: None }, &[GNOME_FILES, URI_LIST, KDE_CUT, TEXT])`, `write_text` → `own_clipboard(Owned { paths: Vec::new(), cut: false, text: Some(text.to_owned()) }, &TEXT_TYPES)`; `drag_out`'taki `Owned`'a `text: None`; `read_files`'ın "kendi panosu" kolu `.filter(|(_, o)| o.text.is_none())`.

- [ ] **Step 1: Write the failing tests**

`clipboard.rs` testlerine:

```rust
    #[cfg(windows)]
    #[test]
    fn unicode_text_is_utf16_with_a_nul() {
        assert_eq!(unicode_text("aş"), [0x61, 0, 0x5F, 0x01, 0, 0]);
    }

    /// Uses the real clipboard: run by hand (`cargo test -p gezik-platform -- --ignored`).
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "replaces the user's clipboard"]
    fn text_replaces_files_on_the_clipboard() {
        write_files(&[std::env::temp_dir().join("a")], true).unwrap();
        let before = sequence();
        write_text("C:\\a b\\ş.txt").unwrap();
        assert_ne!(sequence(), before);
        assert_eq!(read_files().unwrap(), None, "no files once text is copied");
        clear().unwrap();
    }
```
`linux/mod.rs` testlerine:

```rust
    #[test]
    fn latin1_keeps_what_it_can() {
        assert_eq!(latin1("ça ş"), [0xE7, b'a', b' ', b'?']);
    }
```
`wayland.rs` testlerine:

```rust
    #[test]
    fn text_is_offered_as_text_only() {
        let owned = Owned { paths: Vec::new(), cut: false, text: Some("/a/it's ş".to_owned()) };
        for mime in TEXT_TYPES {
            assert_eq!(owned.render(mime).as_deref(), Some("/a/it's ş".as_bytes()), "{mime}");
        }
        assert_eq!(owned.render(URI_LIST), None);
        assert_eq!(owned.render(GNOME_FILES), None);
        let files = Owned { paths: vec![PathBuf::from("/a/b")], cut: false, text: None };
        assert_eq!(files.render(TEXT).as_deref(), Some(b"/a/b".as_slice()));
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik-platform clipboard latin1` → FAIL; `cargo check -j 8 -p gezik-platform --target x86_64-unknown-linux-gnu --tests` → FAIL (`text` alanı yok).
- [ ] **Step 3: Implement** (yukarıdaki kod).
- [ ] **Step 4: Run.** `cargo test -j 8 -p gezik-platform` → PASS; çapraz denetimler; `cargo test -j 8 -p gezik-platform text_replaces -- --ignored` Windows'ta → PASS, ardından PowerShell `Get-Clipboard` → `C:\a b\ş.txt`. Wayland testi Linux kabında Task 10'daki `test.sh unit` ile koşar.
- [ ] **Step 5: Commit** "Put text on the system clipboard on every system".

---

### Task 6: Menüler — bir menüde birden çok alt menü, 4096'dan başlayan Explorer kimlikleri

Davranış değişmez (bugünkü tek "Commands ▸" yeni yoldan gösterilir); yeni öğeler Task 7 ve 9'da.

**Files:**
- Modify: `crates/gezik-platform/src/shell_menu.rs`, `crates/gezik-platform/src/lib.rs` (`pub use shell_menu::{FIRST_SHELL_ID, ShellSubmenu, show_shell_menu};`), `crates/gezik/src/context_menu.rs`, `crates/gezik/ui/widgets/popup-menu.slint`, `crates/gezik/ui/app.slint`

**Interfaces:**
- Produces:

```rust
// shell_menu.rs (Windows)
pub const FIRST_SHELL_ID: u32 = 4096;
pub fn show_shell_menu(window: &impl HasWindowHandle, target: &MenuTarget, extra: &[(u32, &str)],
    subs: &[ShellSubmenu<'_>], at: Option<(i32, i32)>, can_rename: bool) -> Result<MenuOutcome, String>;
// context_menu.rs
pub const GEZIK_IDS_END: u32 = 4096;
pub const MAX_SUBMENUS: usize = 4;
fn split_menu<'a, T: Clone>(items: &[T], subs: &'a [Submenu]) -> (Vec<T>, Vec<(&'a Submenu, Vec<T>)>);
fn menu_lines(before: Vec<MenuEntry>, subs: Vec<(String, Vec<MenuEntry>)>) -> Vec<MenuEntry>;
fn from_submenu(id: u32) -> bool;
// Menus: open(…, subs: Vec<Submenu>, …), open_slint_entries(&self, items, subs: Vec<Submenu>, anchor),
//        add_file_tools(&self, list, subs: &mut Vec<Submenu>, rows, native)
```
```slint
// popup-menu.slint
export struct MenuSub { title: string, entries: [MenuEntry], after: [MenuEntry] }
// PopupMenu: in property <[MenuSub]> subs;   (sub-lines is no longer an input)
// app.slint: in property <[MenuSub]> menu-subs;   (replaces menu-sub-title, menu-sub-entries, menu-entries-after)
```

**Behavior:**
- `shell_menu.rs`: `pub const FIRST_SHELL_ID: u32 = 4096;` (belge: "Shell command ids start here; Gezik's own are below it (spec 11.1: 1-970 was full). Explorer keeps 4096-0x7FFF, over 28,000 ids."). `validate_ids`: `(1..FIRST_SHELL_ID)`, hata `format!("menu item ids must be in 1..{FIRST_SHELL_ID}")`. `show_shell_menu` `sub: Option<ShellSubmenu>` yerine `subs: &[ShellSubmenu<'_>]`, her birinin kimlikleri denetlenir. `track`'te ekleme:

```rust
/// Where each submenu goes in the menu (index into `subs`, position), the empty ones left out:
/// at its place among the `extra` items, the earlier places first, each one inserted moving
/// the ones after it down by one.
fn submenu_places(subs: &[ShellSubmenu<'_>], extra: usize) -> Vec<(usize, u32)> {
    let mut order: Vec<usize> = (0..subs.len()).filter(|i| !subs[*i].items.is_empty()).collect();
    order.sort_by_key(|i| subs[*i].at);
    order.into_iter().enumerate().map(|(inserted, i)| (i, (subs[i].at.min(extra) + inserted) as u32)).collect()
}
```
  `track` içinde tek `sub` bloğunun yerine:

```rust
        for (i, position) in submenu_places(subs, extra.len()) {
            let sub = &subs[i];
            // Once in `hmenu`, destroyed with it (DestroyMenu takes its submenus along).
            let popup = CreatePopupMenu()?;
            let filled = sub.items.iter().try_for_each(|(id, label, enabled)| {
                let flags = if *enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
                AppendMenuW(popup, flags, *id as usize, &menu_label(label))
            });
            let inserted = filled.and_then(|()| {
                InsertMenuW(hmenu, position, MF_BYPOSITION | MF_POPUP, popup.0 as usize, &menu_label(sub.title))
            });
            if let Err(err) = inserted {
                let _ = DestroyMenu(popup);
                return Err(err);
            }
            ours += 1;
        }
```
- `context_menu.rs`:
  - Sabitler (`HEADING`'in yanına):

```rust
/// Gezik's menu ids are below this; the Explorer menu's start here (gezik_platform's
/// `FIRST_SHELL_ID` on Windows). 1000-4095 are new ranges (spec 11.1).
pub const GEZIK_IDS_END: u32 = 4096;
/// Most submenus a menu shows: app.slint and popup-menu.slint have this many places.
pub const MAX_SUBMENUS: usize = 4;
```
  - `split_menu` ve `menu_lines`:

```rust
/// `items` cut at `subs`' places: the items before the first submenu, then each submenu (by
/// place, empty ones left out, at most `MAX_SUBMENUS`) with the items after it.
fn split_menu<'a, T: Clone>(items: &[T], subs: &'a [Submenu]) -> (Vec<T>, Vec<(&'a Submenu, Vec<T>)>) {
    let mut kept: Vec<&Submenu> = subs.iter().filter(|sub| !sub.items.is_empty()).collect();
    kept.sort_by_key(|sub| sub.at);
    kept.truncate(MAX_SUBMENUS);
    let place = |sub: &Submenu| sub.at.min(items.len());
    let first = kept.first().map_or(items.len(), |sub| place(sub));
    let parts = kept
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            let end = kept.get(i + 1).map_or(items.len(), |next| place(next));
            (*sub, items[place(sub)..end].to_vec())
        })
        .collect();
    (items[..first].to_vec(), parts)
}

/// Gezik's own menu in one list (widgets/popup-menu.slint): `before`, then for each submenu
/// its opener (id -1 for the first, -2 for the second…, titled as it) and the items after it.
fn menu_lines(before: Vec<MenuEntry>, subs: Vec<(String, Vec<MenuEntry>)>) -> Vec<MenuEntry> {
    let mut lines = before;
    for (k, (title, after)) in subs.into_iter().enumerate() {
        lines.push(MenuEntry { id: -1 - k as i32, title: title.into(), enabled: true });
        lines.extend(after);
    }
    lines
}

/// Whether `id` is an item of a submenu: once one is chosen, Slint leaves the keyboard
/// nowhere (it gives it to the parent menu, which is gone).
fn from_submenu(id: u32) -> bool {
    (COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX).contains(&id)
}
```
  - `Menus::new`'deki `(COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX).contains(&id)` → `from_submenu(id)`.
  - `open_slint_entries`:

```rust
    /// Shows `items` (id, title, enabled), with `subs` among them, at `anchor`.
    /// (Slint shows these as native menus on Windows, where `&` marks the access key, and on
    /// macOS; elsewhere Gezik draws its own, which popup.rs keeps inside the window.)
    fn open_slint_entries(&self, items: &[(u32, String, bool)], subs: Vec<Submenu>, anchor: Anchor) {
        let Some(window) = self.window.upgrade() else { return };
        if items.is_empty() && subs.iter().all(|sub| sub.items.is_empty()) {
            return;
        }
        let entry = |(id, title, enabled): &(u32, String, bool)| MenuEntry {
            id: i32::try_from(*id).unwrap_or(0),
            title: menu_title(title).into(),
            enabled: *enabled,
        };
        let model = |entries: Vec<MenuEntry>| ModelRc::new(VecModel::from(entries));
        let (before, parts) = split_menu(items, &subs);
        let before: Vec<MenuEntry> = before.iter().map(entry).collect();
        let parts: Vec<(String, Vec<MenuEntry>, Vec<MenuEntry>)> = parts
            .into_iter()
            .map(|(sub, after)| (menu_title(&sub.title), sub.items.iter().map(entry).collect(), after.iter().map(entry).collect()))
            .collect();
        let slots: Vec<MenuSub> = parts
            .iter()
            .map(|(title, inner, after)| MenuSub {
                title: title.as_str().into(),
                entries: model(inner.clone()),
                after: model(after.clone()),
            })
            .collect();
        window.set_menu_entries(model(before.clone()));
        window.set_menu_subs(ModelRc::new(VecModel::from(slots)));
        if !window.get_native_menus() {
            let lines = parts.into_iter().map(|(title, _, after)| (title, after)).collect();
            window.set_menu_lines(model(menu_lines(before, lines)));
        }
        window.invoke_show_menu(anchor.x, anchor.y, anchor.flip_x, anchor.flip_y);
    }
```
  - `open`, `open_native` (iki sürüm) `sub: Option<Submenu>` yerine `subs: Vec<Submenu>`; Windows `open_native`'de:

```rust
            let items: Vec<(u32, &str)> = items.iter().map(|(id, title)| (*id, title.as_str())).collect();
            let sub_items: Vec<Vec<(u32, &str, bool)>> = subs
                .iter()
                .map(|sub| sub.items.iter().map(|(id, title, on)| (*id, title.as_str(), *on)).collect())
                .collect();
            let shell_subs: Vec<gezik_platform::ShellSubmenu> = subs
                .iter()
                .zip(&sub_items)
                .map(|(sub, items)| gezik_platform::ShellSubmenu { title: sub.title.as_str(), at: sub.at, items })
                .collect();
            // … show_shell_menu(&handle, &target, &items, &shell_subs, at, can_rename)
```
  - `add_file_tools`:

```rust
    /// Extract, Compress, Convert and Commands ▸ for `rows`: first on Windows (above the
    /// Explorer menu's own items, moving the places of `subs` down), after the row's other
    /// items elsewhere. Not for drives.
    fn add_file_tools(
        &self,
        list: &mut Vec<(u32, String)>,
        subs: &mut Vec<Submenu>,
        rows: Vec<(PathBuf, bool)>,
        native: bool,
    ) {
        if self.view.shows_drives() {
            return;
        }
        let mut extra = crate::archives::menu_items(&rows);
        let (convert, commands) = crate::convert::menu_items(&rows);
        extra.extend(convert);
        *self.rows.borrow_mut() = rows;
        let start = if native { 0 } else { list.len() };
        if native {
            for sub in subs.iter_mut() {
                sub.at += extra.len();
            }
        }
        subs.extend(commands.map(|items| Submenu { title: "Commands".to_owned(), at: start + extra.len(), items }));
        if native {
            list.splice(0..0, extra);
        } else {
            list.extend(extra);
        }
    }
```
  `row_menu`'nün iki kolu `let mut subs = Vec::new(); self.add_file_tools(&mut list, &mut subs, rows, native);` ve `self.open(…, subs, …)`; `background_menu` ve `sidebar_entry` `Vec::new()`; `open_slint` `Vec::new()`; `convert_menu` ve `filter_menu` `Vec::new()`.
  - `use crate::{AppWindow, MenuEntry, MenuSub};`.
- `popup-menu.slint`:

```slint
// A submenu of a context menu: its title, its items, and the menu's items after it (up to
// the next submenu). Rust fills up to four (context_menu.rs `MAX_SUBMENUS`).
export struct MenuSub {
    title: string,
    entries: [MenuEntry],
    after: [MenuEntry],
}
```
  `MenuEntry` belgesi: "`id` … (-1…-4 in Gezik's own menu: the line that opens submenu 1…4)". `MenuLine`'da ok `if root.entry.id < 0`. `PopupMenu`: belge "with up to four submenus"; `in property <[MenuEntry]> sub-lines;` yerine:

```slint
    in property <[MenuSub]> subs;
    // The open submenu: which (0-3), and its lines (an index past the end gives none).
    property <int> sub-index: 0;
    property <[MenuEntry]> sub-lines: root.subs[root.sub-index].entries;
```
  `open-sub`:

```slint
    function open-sub(index: int, keyboard: bool) {
        let which = -1 - root.lines[index].id;
        if root.subs[which].entries.length == 0 {
            return;
        }
        if !root.sub-open || root.sub-at != index {
            root.sub-at = index;
            root.sub-index = which;
            sub.scroll-top();
        }
        root.sub-open = true;
        root.in-sub = keyboard;
        root.sub-current = keyboard ? 0 : -1;
    }
```
  `activate`, tuş işleyicisi (iki yer) ve `main`'in `hovered`'ındaki `id == -1` denetimleri `id < 0` olur.
- `app.slint`: `import { PopupMenu, MenuEntry, MenuPlace, MenuSub } from "widgets/popup-menu.slint";`, dışa açma satırına `MenuSub`; `menu-sub-title`, `menu-sub-entries`, `menu-entries-after` yerine (yorum: "the items, then up to four submenus, each with the items after it"):

```slint
    in property <[MenuSub]> menu-subs;
```
  `ContextMenuArea`'nın `Menu`'sü (dört yuva, her biri aynı kalıp; dizinin dışı boştur):

```slint
        Menu {
            for entry in root.menu-entries: MenuItem {
                title: entry.title;
                enabled: entry.enabled;
                activated => { root.menu-activated(entry.id); }
            }
            if root.menu-subs[0].entries.length > 0: Menu {
                title: root.menu-subs[0].title;
                for entry in root.menu-subs[0].entries: MenuItem {
                    title: entry.title;
                    enabled: entry.enabled;
                    activated => { root.menu-activated(entry.id); }
                }
            }
            for entry in root.menu-subs[0].after: MenuItem {
                title: entry.title;
                enabled: entry.enabled;
                activated => { root.menu-activated(entry.id); }
            }
            // The same for menu-subs[1], [2] and [3].
        }
```
  (Yorum satırındaki "the same" kodda yazılır: `[1]`, `[2]`, `[3]` için aynı iki blok, toplam dört alt menü bloğu ve dört `after` bloğu.) `popup-menu := PopupMenu`'de `sub-lines: root.menu-sub-entries;` → `subs: root.menu-subs;`.

- [ ] **Step 1: Write the failing tests**

`context_menu.rs` (eski `gezik_menus_list_the_submenu_among_the_items` yerine):

```rust
    #[test]
    fn gezik_menus_list_the_submenus_among_the_items() {
        let entry = |id: i32, title: &str| MenuEntry { id, title: title.into(), enabled: true };
        let lines = menu_lines(
            vec![entry(1, "Open")],
            vec![("Copy path as".to_owned(), vec![entry(2, "Cut")]), ("Commands".to_owned(), Vec::new())],
        );
        let shown: Vec<(i32, &str)> = lines.iter().map(|l| (l.id, l.title.as_str())).collect();
        assert_eq!(shown, [(1, "Open"), (-1, "Copy path as"), (2, "Cut"), (-2, "Commands")]);
    }

    #[test]
    fn a_menu_is_cut_at_its_submenus() {
        let sub = |title: &str, at: usize, n: usize| Submenu { title: title.into(), at, items: vec![(1, "x".to_owned(), true); n] };
        let items = ["a", "b", "c", "d"];
        let subs = [sub("Late", 9, 1), sub("Empty", 1, 0), sub("Mid", 2, 1), sub("First", 0, 2)];
        let (before, parts) = split_menu(&items, &subs);
        assert!(before.is_empty());
        let shown: Vec<(&str, Vec<&str>)> = parts.iter().map(|(s, after)| (s.title.as_str(), after.clone())).collect();
        assert_eq!(shown, [("First", vec!["a", "b"]), ("Mid", vec!["c", "d"]), ("Late", vec![])]);
        let many: Vec<Submenu> = (0..6).map(|i| sub("S", i, 1)).collect();
        let (_, parts) = split_menu(&items, &many);
        assert_eq!(parts.len(), MAX_SUBMENUS);
        assert_eq!(parts.last().unwrap().1, ["d"], "the items after one left out stay");
        let (before, parts) = split_menu(&items, &[]);
        assert_eq!((before.len(), parts.len()), (4, 0));
    }

    #[cfg(windows)]
    #[test]
    fn gezik_ids_end_where_explorers_begin() {
        assert_eq!(gezik_platform::FIRST_SHELL_ID, GEZIK_IDS_END);
    }
```
  `conversion_ids_meet_no_others`'ta `a.end <= 1000` → `a.end <= GEZIK_IDS_END` (ileti "1..4096 (0 is a heading, 4096 on the Shell's)").

`shell_menu.rs`:

```rust
    #[test]
    fn several_submenus_go_in_their_places() {
        let items = [(700, "A", true)];
        let sub = |at| ShellSubmenu { title: "S", at, items: &items };
        let empty = ShellSubmenu { title: "E", at: 0, items: &[] };
        // extra [x, y, z]: Commands before x (0), Copy path after y (2), one past the end.
        assert_eq!(submenu_places(&[sub(2), sub(0), empty, sub(9)], 3), [(1, 0), (0, 3), (3, 5)]);
    }
```
  `extra_ids_must_be_below_the_shell_range`: `(4095, "b")` geçer, `(4096, "a")` geçmez; `submenu_ids_are_checked_too`: `Some(sub)` → `&[sub]`, kötü kimlik `4096`, ileti `contains("1..4096")`.

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik context_menu` ve `cargo test -j 8 -p gezik-platform shell_menu` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod). `grep -rn "menu_sub_\|menu-sub-\|entries_after\|entries-after" crates/gezik` boş kalmalı.
- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler (macOS denetimi `app.slint`'i derler). Elle (Windows): `GEZIK_CONFIG_DIR` geçici, `settings.toml`'a 6b planının "Copy txt" komutu (`types = ["txt"]`) → `.txt`'ye sağ tık: Explorer menüsünün üstünde "Commands ▸" ve içinde "Copy txt"; sekmeye sağ tık (Slint menüsü) bugünkü gibi. Linux'ta `gui.sh popups` ve `commands` Task 10'da koşar.
- [ ] **Step 5: Commit** "Show several submenus in one menu and move Explorer's item ids to 4096".

---

### Task 7: Arayüz — terminal ve yolu kopyala

**Files:**
- Create: `crates/gezik/src/terminal.rs`, `crates/gezik/src/copy_path.rs`
- Modify: `crates/gezik/src/context_menu.rs`, `crates/gezik/src/actions.rs`, `crates/gezik/src/keys.rs`, `crates/gezik/src/main.rs` (`mod copy_path; mod terminal;`, `apply_config`), `crates/gezik/src/view/mod.rs` (`is_folder_row`'un `cfg_attr`'ı kalkar), `crates/gezik/ui/app.slint` (macOS File ve Edit menüleri)

**Interfaces:**
- Consumes: Task 1 (`PathFormat`, `format_paths`, `unc_path`), Task 3 (`Action::{OpenTerminal, OpenTerminalAdmin, CopyPath}`, `TerminalSettings`), Task 4 (`terminal::{open, TerminalError}`, `fs::mapped_remote`), Task 5 (`clipboard::write_text`), Task 6 (`Submenu`, `subs`).
- Produces:

```rust
// context_menu.rs
pub const OPEN_TERMINAL: u32 = 1000;
pub const OPEN_TERMINAL_ADMIN: u32 = 1001;
pub const COPY_PATH_FIRST: u32 = 1010;          // 1010-1016, PathFormat::ALL order
pub fn terminal_items(windows: bool) -> Vec<(u32, &'static str)>;
pub fn copy_path_items(windows: bool, unc: bool) -> Vec<(u32, String, bool)>;
// terminal.rs (app)
pub const ONLY_ON_WINDOWS: &str;   pub const NOT_FOUND: &str;
pub fn set_settings(command: Option<Vec<String>>);
pub fn folder_for(focused: Option<(PathBuf, bool)>, shown: Option<PathBuf>) -> Option<PathBuf>;
pub fn error_text(err: &TerminalError) -> Option<String>;
pub fn open_in(dir: PathBuf, admin: bool);
pub fn open_for_view(view: &View, admin: bool);
// copy_path.rs
pub fn copied_text(count: usize) -> String;
pub fn unc_offered(first: Option<&Path>) -> bool;
pub fn copy(view: &View, paths: &[PathBuf], kind: PathFormat);
pub fn copy_selection(view: &View);
```

**Behavior:**
- `terminal.rs` (app):

```rust
//! "Open terminal" in the app (spec 3.1): where it opens, and the status bar when it does not.
//! Choosing and starting it (gezik_platform::terminal) happens on a thread: it looks through PATH.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use gezik_platform::terminal::TerminalError;

use crate::view::View;

pub const ONLY_ON_WINDOWS: &str = "Only on Windows";
pub const NOT_FOUND: &str = "No terminal found. Set [terminal] command in settings.toml.";

thread_local! {
    /// `[terminal] command` of the settings in effect.
    static COMMAND: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// settings.toml changed (every resolve).
pub fn set_settings(command: Option<Vec<String>>) {
    COMMAND.with(|c| *c.borrow_mut() = command);
}

/// Where the terminal opens for the file list: the focused row's folder (its own if it is
/// one; a drive in This PC is one), else the folder shown; in This PC with no drive focused,
/// nowhere.
pub fn folder_for(focused: Option<(PathBuf, bool)>, shown: Option<PathBuf>) -> Option<PathBuf> {
    match focused {
        Some((path, true)) => Some(path),
        Some((path, false)) => path.parent().map(Path::to_path_buf).or(shown),
        None => shown,
    }
}

/// What the status bar says when the terminal did not open; nothing for a No to UAC.
pub fn error_text(err: &TerminalError) -> Option<String> {
    match err {
        TerminalError::NotFound => Some(NOT_FOUND.to_owned()),
        TerminalError::OnlyOnWindows => Some(ONLY_ON_WINDOWS.to_owned()),
        TerminalError::Cancelled => None,
        TerminalError::Failed(why) => Some(format!("Cannot open the terminal: {why}")),
    }
}

fn note(text: String) {
    crate::view::with_current(|view| view.note(text));
}

/// Opens a terminal in `dir` (`admin`: as administrator, Windows only), on a thread.
pub fn open_in(dir: PathBuf, admin: bool) {
    if admin && !cfg!(windows) {
        return note(ONLY_ON_WINDOWS.to_owned());
    }
    let command = COMMAND.with(|c| c.borrow().clone());
    let started = std::thread::Builder::new().name("gezik-open-terminal".into()).spawn(move || {
        let result = gezik_platform::terminal::open(&dir, admin, command.as_deref());
        if let Some(text) = result.err().as_ref().and_then(error_text) {
            let _ = slint::invoke_from_event_loop(move || note(text));
        }
    });
    if let Err(err) = started {
        note(format!("Cannot open the terminal: {err}"));
    }
}

/// `open-terminal` and `open-terminal-admin` on the file list.
pub fn open_for_view(view: &View, admin: bool) {
    let focused = view.focus().and_then(|i| view.entry_path(i));
    if let Some(dir) = folder_for(focused, view.folder()) {
        open_in(dir, admin);
    }
}
```
- `copy_path.rs`:

```rust
//! "Copy path as ▸" and `copy-path` (spec 4): paths as text on the system clipboard. Gezik's
//! own file clipboard (the cut items, paste) is left as it is.

use std::path::{Path, PathBuf};

use gezik_core::path_text::{PathFormat, format_paths, unc_path};
use gezik_platform::clipboard::{self, ClipboardError};

use crate::view::View;

/// The status line after `count` paths were copied.
pub fn copied_text(count: usize) -> String {
    if count == 1 { "Copied the path".to_owned() } else { format!("Copied {} paths", crate::preview::with_commas(count)) }
}

/// Whether "UNC path" is offered: on Windows, when the first item is on a mapped drive.
pub fn unc_offered(first: Option<&Path>) -> bool {
    cfg!(windows)
        && first.is_some_and(|path| unc_path(&path.to_string_lossy(), &gezik_platform::fs::mapped_remote).is_some())
}

/// Copies `paths` written as `kind`; the status bar says how it went.
pub fn copy(view: &View, paths: &[PathBuf], kind: PathFormat) {
    if paths.is_empty() {
        return;
    }
    let texts: Vec<String> = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let text = format_paths(&texts, kind, cfg!(windows), &gezik_platform::fs::mapped_remote);
    view.note(match clipboard::write_text(&text) {
        Ok(()) => copied_text(paths.len()),
        Err(ClipboardError::Unsupported) => "No system clipboard here".to_owned(),
        Err(ClipboardError::Failed(why)) => format!("Cannot use the clipboard: {why}"),
    });
}

/// `copy-path`: the selected items' full paths, else the folder shown's; in This PC the
/// selected drives (none selected: nothing).
pub fn copy_selection(view: &View) {
    let mut paths = view.selected_paths();
    if paths.is_empty() {
        paths.extend(view.folder());
    }
    copy(view, &paths, PathFormat::Full);
}
```
- `context_menu.rs`: sabitler (belge: "1000-1001: Open terminal here / as administrator. 1010-1016: Copy path as ▸, in `PathFormat::ALL` order."), `use gezik_core::path_text::PathFormat;`, `use std::path::{Path, PathBuf};` ve:

```rust
/// "Open terminal here", and on Windows "Open terminal as administrator" under it.
pub fn terminal_items(windows: bool) -> Vec<(u32, &'static str)> {
    let mut out = vec![(OPEN_TERMINAL, "Open terminal here")];
    if windows {
        out.push((OPEN_TERMINAL_ADMIN, "Open terminal as administrator"));
    }
    out
}

/// "Copy path as ▸": the formats offered here (`PathFormat::offered`), each by its place in
/// `PathFormat::ALL`.
pub fn copy_path_items(windows: bool, unc: bool) -> Vec<(u32, String, bool)> {
    PathFormat::ALL
        .iter()
        .enumerate()
        .filter(|(_, kind)| kind.offered(windows, unc))
        .map(|(i, kind)| (COPY_PATH_FIRST + i as u32, kind.label().to_owned(), true))
        .collect()
}
```
  `Menus`'a:

```rust
    /// "Copy path as ▸" for `paths`, at place `at` among the items.
    fn copy_path_sub(&self, paths: &[PathBuf], at: usize) -> Submenu {
        let unc = crate::copy_path::unc_offered(paths.first().map(PathBuf::as_path));
        Submenu { title: "Copy path as".to_owned(), at, items: copy_path_items(cfg!(windows), unc) }
    }
```
  `row_menu` (iki kol da: yerin öğeleri, terminal, Copy path ▸, dosya araçları, dosya öğeleri):

```rust
        if self.view.is_selected(i) && self.view.selection_count() > 1 {
            let rows = self.view.selected_items();
            let paths: Vec<PathBuf> = rows.iter().map(|(path, _)| path.clone()).collect();
            let mut list = owned(items(Place::Rows, native));
            list.extend(owned(terminal_items(native)));
            let mut subs = vec![self.copy_path_sub(&paths, list.len())];
            self.add_file_tools(&mut list, &mut subs, rows, native);
            list.extend(self.file_extras(false, false, native));
            if native && !self.view.shows_drives() {
                list.push((BATCH_RENAME, format!("Rename {} items…", paths.len())));
            }
            return self.open(Subject::Rows(paths.clone()), list, subs, MenuTarget::Items(paths), x, y, at);
        }
        let Some((path, is_dir)) = self.view.entry_path(i) else { return };
        let place = Place::Row { is_dir, pinned: is_dir && self.sidebar.is_pinned(&path) };
        let mut list = owned(items(place, native));
        list.extend(owned(terminal_items(native)));
        let mut subs = vec![self.copy_path_sub(std::slice::from_ref(&path), list.len())];
        self.add_file_tools(&mut list, &mut subs, vec![(path.clone(), is_dir)], native);
        list.extend(self.file_extras(true, is_dir, native));
        self.open(Subject::Row(path.clone()), list, subs, MenuTarget::Item(path), x, y, at);
```
  `background_menu`: `let mut list = background_items(…); list.extend(owned(terminal_items(cfg!(windows)))); let subs = vec![self.copy_path_sub(std::slice::from_ref(&dir), list.len())];` → `open(…, subs, …)`. `sidebar_entry`: `let mut list = owned(items(place, cfg!(windows))); list.extend(owned(terminal_items(cfg!(windows))));` ve aynı `subs`.
  `run`'a (`_ => {}`'den önce):

```rust
            (OPEN_TERMINAL | OPEN_TERMINAL_ADMIN, subject) => {
                let dir = match subject {
                    Subject::Row(path) if self.view.is_folder_row(&path) => Some(path),
                    Subject::Row(path) => path.parent().map(Path::to_path_buf),
                    Subject::Rows(_) => self.view.folder(),
                    Subject::SidebarEntry(path) | Subject::Background(path) => Some(path),
                    _ => None,
                };
                if let Some(dir) = dir {
                    crate::terminal::open_in(dir, id == OPEN_TERMINAL_ADMIN);
                }
            }
            (id, subject) if (COPY_PATH_FIRST..COPY_PATH_FIRST + PathFormat::ALL.len() as u32).contains(&id) => {
                let paths = match subject {
                    Subject::Row(path) | Subject::SidebarEntry(path) | Subject::Background(path) => vec![path],
                    Subject::Rows(paths) => paths,
                    _ => Vec::new(),
                };
                if let Some(kind) = PathFormat::ALL.get((id - COPY_PATH_FIRST) as usize) {
                    crate::copy_path::copy(&self.view, &paths, *kind);
                }
            }
```
  `from_submenu`'ya `|| (COPY_PATH_FIRST..COPY_PATH_FIRST + PathFormat::ALL.len() as u32).contains(&id)`.
- `actions.rs`: kollar `Action::OpenTerminal => crate::terminal::open_for_view(view, false)`, `Action::OpenTerminalAdmin => crate::terminal::open_for_view(view, true)`, `Action::CopyPath => crate::copy_path::copy_selection(view)`; bu üçü "Not 6a's" listesinden çıkar; modül belgesi "The actions of the keyboard package (6a, 6b) and of 7a …".
- `keys.rs`: `acts_on_selection`'a `| Action::CopyPath` (belge: "… and copy-path: they act on the list, …").
- `main.rs`: `mod copy_path;`, `mod terminal;`; `apply_config`'te `terminal::set_settings(loaded.settings.terminal.command.clone());`.
- `app.slint` (macOS menü çubuğu): File'da "New Folder"dan sonra `MenuItem { title: "Open Terminal"; shortcut: root.menu-keys ? @keys(Control + Alt + T) : @keys(); activated => { root.menu-command("open-terminal"); } }`; Edit'te "Copy"den sonra `MenuItem { title: "Copy Path"; shortcut: root.menu-keys ? @keys(Control + Alt + C) : @keys(); activated => { root.menu-command("copy-path"); } }`. (`menu_bar.rs` değişmez: bu iki eylemin tuşu var, menü onu çalar.)

- [ ] **Step 1: Write the failing tests**

`terminal.rs` (app):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_opens_in_the_focused_folder_or_the_one_shown() {
        let shown = Some(PathBuf::from("/a"));
        assert_eq!(folder_for(Some((PathBuf::from("/a/sub"), true)), shown.clone()), Some(PathBuf::from("/a/sub")));
        assert_eq!(folder_for(Some((PathBuf::from("/a/f.txt"), false)), shown.clone()), Some(PathBuf::from("/a")));
        assert_eq!(folder_for(None, shown.clone()), shown);
        assert_eq!(folder_for(None, None), None, "This PC with no drive focused");
    }

    #[test]
    fn what_the_status_bar_says() {
        assert_eq!(error_text(&TerminalError::NotFound).as_deref(), Some(NOT_FOUND));
        assert_eq!(error_text(&TerminalError::OnlyOnWindows).as_deref(), Some("Only on Windows"));
        assert_eq!(error_text(&TerminalError::Cancelled), None, "a No to UAC is no error");
        assert_eq!(error_text(&TerminalError::Failed("x".into())).as_deref(), Some("Cannot open the terminal: x"));
    }
}
```
`copy_path.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_counts_the_paths() {
        assert_eq!(copied_text(1), "Copied the path");
        assert_eq!(copied_text(3), "Copied 3 paths");
        assert_eq!(copied_text(1234), "Copied 1,234 paths");
    }

    #[test]
    fn unc_is_offered_only_for_a_mapped_drive() {
        assert!(!unc_offered(None));
        assert!(!unc_offered(Some(Path::new("/home/a"))));
        #[cfg(windows)]
        assert!(!unc_offered(Some(Path::new(r"C:\Windows"))), "a local drive");
    }
}
```
`context_menu.rs`:

```rust
    #[test]
    fn terminal_and_copy_path_items() {
        assert_eq!(ids(terminal_items(false)), [OPEN_TERMINAL]);
        assert_eq!(ids(terminal_items(true)), [OPEN_TERMINAL, OPEN_TERMINAL_ADMIN]);
        let titles = |items: Vec<(u32, String, bool)>| items.into_iter().map(|(_, t, _)| t).collect::<Vec<_>>();
        assert_eq!(titles(copy_path_items(false, false)), ["Full path", "Quoted", "Name", "Folder path", "file:// URL"]);
        let windows = copy_path_items(true, true);
        assert_eq!(windows.len(), 7);
        assert_eq!(windows[6], (COPY_PATH_FIRST + 6, "UNC path".to_owned(), true));
        assert_eq!(copy_path_items(false, false)[2].0, COPY_PATH_FIRST + 3, "ids follow PathFormat::ALL, not the menu");
    }
```
  `conversion_ids_meet_no_others`'un `singles`'ına `OPEN_TERMINAL, OPEN_TERMINAL_ADMIN`, aralıklarına `COPY_PATH_FIRST..COPY_PATH_FIRST + 7`.
`keys.rs`: `selection_keys_wait_while_a_text_field_has_the_keyboard`'un listesine `Action::CopyPath`; `every_default_is_reachable_on_windows_and_linux`'un sonuna:

```rust
        // open-terminal's second key; on Turkish Q, Ctrl+Alt+T (AltGr+T) types ₺: no shortcut
        // there (known, spec 10.3), Shift+F4 is the one that always works.
        assert_eq!(reach("ctrl+alt+t"), Some(Action::OpenTerminal));
        assert_eq!(chord_from_slint("₺", true, true, false, false, Platform::Other), None);
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik terminal copy_path context_menu keys` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod).
- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler. Elle (Windows, `GEZIK_CONFIG_DIR` geçici, `cargo run -j 8 -p gezik`): bir klasör satırında Shift+F4 → Windows Terminal o klasörde; dosya satırında Ctrl+Alt+T → klasöründe; sağ tık → "Open terminal here", "Open terminal as administrator" (UAC → No: hiçbir şey, durum çubuğu değişmez), "Copy path as ▸" → "Quoted" → PowerShell'de `Get-Clipboard` tırnaklı yol; boşlukta sağ tık → aynı öğeler gösterilen klasör için; Ctrl+Shift+C üç seçili öğede → "Copied 3 paths".
- [ ] **Step 5: Commit** "Open a terminal and copy paths from the menus, the keys and the macOS menu bar".

---

### Task 8: Oturum — açılışta geri yükleme, sekmeler değiştikçe kayıt

**Files:**
- Modify: `crates/gezik/src/start.rs`, `crates/gezik/src/navigation.rs`, `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: Task 2 (`Session`, `SessionTab`, `Tabs::{from_session, session}`), Task 3 (`State.session`, `Settings.session`).
- Produces:

```rust
// start.rs
pub struct StartPlan { pub session: Session, pub select: Option<String>, pub start: Location, pub warnings: Vec<Warning> }
impl StartPlan { pub fn first(&self) -> &Location; }
pub fn plan_start(setting: &str, cli: Option<PathBuf>, home: &Path, expand: impl Fn(&str) -> Option<PathBuf>,
    kind: impl Fn(&Path) -> PathKind, saved: Option<&Session>) -> StartPlan;
// navigation.rs
impl Navigator {
    pub fn new(window: &AppWindow, view: View, session: Session, select: Option<String>, start: Location) -> Navigator;
    pub fn keep_session(&self, saved: Session, restore: bool, sink: impl Fn(&Session) + 'static);
    pub fn set_session_restore(&self, restore: bool);
}
fn session_to_send(on: bool, tabs: Session, sent: &Session) -> Option<Session>;
```

**Behavior:**
- `start.rs` (modül belgesine "and the tabs of last time (spec 5.1)"):

```rust
/// The resolved start: the tabs that open (the one in front among them) and where new tabs
/// open.
#[derive(Debug, Clone, PartialEq)]
pub struct StartPlan {
    /// The tabs to open: the saved session, or one tab.
    pub session: Session,
    /// Entry to select in the tab in front (a file given on the command line).
    pub select: Option<String>,
    /// The `start-folder` setting, resolved: where new tabs open. Never the command line.
    pub start: Location,
    pub warnings: Vec<Warning>,
}

impl StartPlan {
    /// Where the tab in front opens.
    pub fn first(&self) -> &Location {
        self.session.active_location().unwrap_or(&self.start)
    }
}

/// The saved tabs (`saved`, when `[session] restore` is on and there are any) come back as
/// they were, and a command-line path opens after them, in front; without them, the
/// command-line path wins as before: a folder opens as is, a file opens its folder with the
/// file selected, a missing path warns. The saved tabs are not looked at on disk (a gone
/// folder falls back when it is shown). The setting is always resolved, for new tabs.
pub fn plan_start(
    setting: &str,
    cli: Option<PathBuf>,
    home: &Path,
    expand: impl Fn(&str) -> Option<PathBuf>,
    kind: impl Fn(&Path) -> PathKind,
    saved: Option<&Session>,
) -> StartPlan {
    let (start, warning) = resolve_start_folder(setting, home, expand, &kind);
    let restored = saved.filter(|session| !session.is_empty()).cloned();
    let from_session = restored.is_some();
    let mut plan = StartPlan {
        session: restored.unwrap_or_else(|| Session::single(start.clone())),
        select: None,
        start,
        warnings: warning.into_iter().collect(),
    };
    let Some(path) = cli else { return plan };
    let (location, select) = match kind(&path) {
        PathKind::Dir => (Location::Path(path), None),
        PathKind::File => match (path.parent().filter(|p| !p.as_os_str().is_empty()), path.file_name()) {
            (Some(parent), Some(name)) => {
                (Location::Path(parent.to_path_buf()), Some(name.to_string_lossy().into_owned()))
            }
            _ => return plan,
        },
        PathKind::Missing => {
            let opening = if from_session { "opening the last tabs" } else { "opening start-folder" };
            plan.warnings.push(Warning::new("command line", format!("{}: not found; {opening}", path.display())));
            return plan;
        }
    };
    if from_session {
        plan.session.tabs.push(SessionTab { location, locked: false });
        plan.session.active = plan.session.tabs.len() - 1;
    } else {
        plan.session = Session::single(location);
    }
    plan.select = select;
    plan
}
```
  `use gezik_core::nav::{Location, Session, SessionTab};`. Var olan testlerde `plan(setting, cli)` yardımcısı `plan_start(…, None)` çağırır; `x.first` → `x.first().clone()` (tuple karşılaştırmalarında).
- `navigation.rs`:
  - `Navigator::new`'in imzası `first: Location` yerine `session: Session`; gövde `let mut tabs = Tabs::from_session(&session).unwrap_or_else(|| Tabs::new(start.clone()));` (sonrası aynı: `select` öndeki sekmenin görünümüne). Belge: "The tabs of `session` open, the one in front with `select` selected; …".
  - `Inner`'e:

```rust
    /// Told the tabs as the session keeps them when they change (main.rs: state.toml).
    session_sink: Option<Rc<dyn Fn(&Session)>>,
    /// What the sink was told last (at first: what state.toml had).
    session_sent: Session,
    /// `[session] restore`: off, the sink is told an empty session (state.toml forgets it).
    session_on: bool,
```
  (`new`'de `None`, `Session::default()`, `false`.)
  - Yöntemler:

```rust
    /// Tells `sink` the tabs whenever they change, while `restore` is on (spec 5.1); `saved`
    /// is what state.toml has now, so an unchanged session is not written again.
    pub fn keep_session(&self, saved: Session, restore: bool, sink: impl Fn(&Session) + 'static) {
        {
            let mut inner = self.0.borrow_mut();
            inner.session_sink = Some(Rc::new(sink));
            inner.session_sent = saved;
            inner.session_on = restore;
        }
        self.send_session();
    }

    /// `[session] restore` changed (settings.toml was reloaded): written or forgotten now.
    pub fn set_session_restore(&self, restore: bool) {
        self.0.borrow_mut().session_on = restore;
        self.send_session();
    }

    /// Tells the sink the session if it is not what it was told last.
    fn send_session(&self) {
        let (sink, session) = {
            let mut inner = self.0.borrow_mut();
            let Some(sink) = inner.session_sink.clone() else { return };
            let Some(session) = session_to_send(inner.session_on, inner.tabs.session(), &inner.session_sent) else {
                return;
            };
            inner.session_sent = session.clone();
            (sink, session)
        };
        // With no borrow held: the sink may use the navigator.
        sink(&session);
    }
```
  ve modül düzeyinde:

```rust
/// What state.toml should get, if anything: the tabs while restoring is on, else an empty
/// session; `None` when that is what it was told last.
fn session_to_send(on: bool, tabs: Session, sent: &Session) -> Option<Session> {
    let wanted = if on { tabs } else { Session::default() };
    (wanted != *sent).then_some(wanted)
}
```
  - `update_chrome`'un sonuna (dinleyicilerden sonra) `self.send_session();`. Her sekme değişimi (açma, kapama, taşıma, kilit, etkinleştirme, `close_other_tabs`, `reopen`, `duplicate`) ve her gösterilen yükleme (`finish_load`) `update_chrome`'dan geçer.
  - `use gezik_core::nav::{…, Session}`.
- `main.rs`:
  - `let saved_state = config.as_ref().map(ConfigStore::load_state).unwrap_or_default();` satırı `apply_config_and_start` çağrısının **önüne** taşınır (oturumu plan alır).
  - `apply_config_and_start(window, files, cli, saved: Option<&Session>)` ve `resolve_start(settings, cli, saved)`; `resolve_start`'ta `let saved = saved.filter(|_| settings.session.restore);` ve `start::plan_start(…, saved)`. Açılışta `Some(&saved_state.session)`, dosya izleyicisinin yeniden çözümünde `None`.
  - `let StartPlan { session, select, start, .. } = plan;` ve `navigation::Navigator::new(&window, view.clone(), session, select, start)`.
  - `nav.install();`'dan sonra:

```rust
    // The open tabs go to state.toml as they change (spec 5.1); its own thread writes them, so
    // a crash or a kill leaves the last tabs too.
    if let Some(store) = config.clone() {
        nav.keep_session(saved_state.session.clone(), initial_settings.session.restore, move |session| {
            let session = session.clone();
            store.update_state(move |state| state.session = session);
        });
    }
```
  - `apply_config`'e `navigation::with_current(|nav| nav.set_session_restore(loaded.settings.session.restore));` (açılıştaki ilk çağrıda gezgin henüz yok: bir şey yapmaz).

- [ ] **Step 1: Write the failing tests**

`start.rs` (test modülüne):

```rust
    fn saved() -> Session {
        Session {
            tabs: vec![
                SessionTab { location: p("/gone/far"), locked: true },
                SessionTab { location: Location::Drives, locked: false },
            ],
            active: 1,
        }
    }

    #[test]
    fn the_saved_tabs_come_back_and_new_tabs_still_open_in_the_setting() {
        let plan = plan_start("{home}/Docs", None, &home(), expand, kind, Some(&saved()));
        assert_eq!(plan.session, saved());
        assert_eq!((plan.first().clone(), plan.start.clone(), plan.select.clone()), (Location::Drives, p("/home/u/Docs"), None));
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn a_command_line_path_joins_the_saved_tabs_at_the_end() {
        let folder = plan_start("{home}", Some(PathBuf::from("/work/sub")), &home(), expand, kind, Some(&saved()));
        assert_eq!(folder.session.tabs.len(), 3);
        assert_eq!((folder.session.active, folder.first().clone()), (2, p("/work/sub")));
        assert!(folder.session.tabs[0].locked, "the saved locks stay");
        let file = plan_start("{home}", Some(PathBuf::from("/work/notes.txt")), &home(), expand, kind, Some(&saved()));
        assert_eq!((file.first().clone(), file.select.as_deref()), (p("/work"), Some("notes.txt")));
        let gone = plan_start("{home}", Some(PathBuf::from("/nowhere")), &home(), expand, kind, Some(&saved()));
        assert_eq!(gone.session, saved(), "the session still comes");
        assert!(gone.warnings[0].message.ends_with("not found; opening the last tabs"), "{:?}", gone.warnings);
    }

    #[test]
    fn a_saved_session_is_never_looked_at_on_disk() {
        let asked = std::cell::RefCell::new(Vec::new());
        let counting = |path: &Path| {
            asked.borrow_mut().push(path.to_path_buf());
            kind(path)
        };
        let plan = plan_start("{home}", None, &home(), expand, counting, Some(&saved()));
        assert_eq!(plan.session, saved());
        assert_eq!(*asked.borrow(), [PathBuf::from("/home/u")], "only start-folder is looked at");
    }

    #[test]
    fn an_empty_session_is_todays_start() {
        let plan = plan_start("drives", Some(PathBuf::from("/work")), &home(), expand, kind, Some(&Session::default()));
        assert_eq!(plan.session, Session::single(p("/work")));
        let plain = plan_start("drives", None, &home(), expand, kind, None);
        assert_eq!(plain.session, Session::single(Location::Drives));
    }
```
`navigation.rs` (test modülüne):

```rust
    #[test]
    fn the_session_is_sent_when_it_changed_and_emptied_when_off() {
        use gezik_core::nav::SessionTab;
        let one = Session::single(Location::Path(PathBuf::from("/a")));
        assert_eq!(session_to_send(true, one.clone(), &Session::default()), Some(one.clone()));
        assert_eq!(session_to_send(true, one.clone(), &one), None, "unchanged: not written again");
        assert_eq!(session_to_send(false, one.clone(), &one), Some(Session::default()), "off: forgotten");
        assert_eq!(session_to_send(false, one.clone(), &Session::default()), None, "off and empty: nothing to write");
        let locked = Session { tabs: vec![SessionTab { location: Location::Drives, locked: true }], active: 0 };
        assert!(session_to_send(true, locked, &one).is_some());
    }
```

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik start navigation` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod).
- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler. Elle (Windows, `GEZIK_CONFIG_DIR` geçici): üç sekme aç, birini kilitle, ikinciyi öne al, pencereyi kapat; `state.toml`'da `[session] active = 1` ve üç `[[session.tabs]]` (kilitli olanda `locked = true`); yeniden aç → aynı üç sekme, ikincisi önde, kilit simgesi yerinde; Görev Yöneticisi'nden "Görevi sonlandır" ile öldür, yeniden aç → aynı. `settings.toml`'a `[session]\nrestore = false` → `state.toml`'da `[session]` kalmaz; yeniden aç → tek sekme `start-folder`.
- [ ] **Step 5: Commit** "Open the tabs of last time at start and keep them as they change".

---

### Task 9: Sekme setleri

**Files:**
- Create: `crates/gezik/src/tab_sets.rs`
- Modify: `crates/gezik/src/navigation.rs` (`open_tab_set`, `tab_locations`), `crates/gezik/src/context_menu.rs`, `crates/gezik/src/actions.rs`, `crates/gezik/src/main.rs`, `crates/gezik/src/menu_bar.rs`, `crates/gezik/ui/app.slint` (macOS Window menüsü)

**Interfaces:**
- Consumes: Task 2 (`Tabs::open_set`), Task 3 (`TabSet`, `SettingsChange::TabSets`, `ConfigStore::save_tab_sets`, `Action::SaveTabSet`), Task 6 (`Submenu`, `subs`, `from_submenu`), `navigation::locked_kept_text`, `gezik_config::paths::KnownDirs`.
- Produces:

```rust
// navigation.rs
impl Navigator {
    pub fn open_tab_set(&self, locations: Vec<Location>, replace: bool);
    pub fn tab_locations(&self) -> Vec<Location>;
}
// context_menu.rs
pub const SAVE_TAB_SET: u32 = 1020;
pub const TAB_SET_OPEN_FIRST: u32 = 1100;
pub const TAB_SET_REPLACE_FIRST: u32 = 1130;
pub const TAB_SET_DELETE_FIRST: u32 = 1160;
pub const TAB_SET_MAX: u32 = 30;
pub fn tab_set_items(names: &[String]) -> Vec<(u32, String, bool)>;
// Subject::Tab(u64, Vec<String>)   (the tab's id, the set names the menu listed)
// tab_sets.rs
pub fn set_settings(sets: Vec<TabSet>);
pub fn saved() -> Vec<TabSet>;
pub fn names() -> Vec<String>;
pub fn text_of(location: &Location, dirs: &KnownDirs) -> String;
pub fn location_of(text: &str, dirs: &KnownDirs) -> Option<Location>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetItem { Open(usize), Replace(usize), Delete(usize) }
pub fn set_item(id: u32) -> Option<SetItem>;
pub fn with_current(f: impl FnOnce(&TabSets));
impl TabSets {
    pub fn new(window: &AppWindow, nav: Navigator, view: View, dialogs: Dialogs, store: Option<ConfigStore>) -> TabSets;
    pub fn chosen(&self, id: u32, names: &[String]);
    pub fn open(&self, name: &str, replace: bool);
    pub fn ask_save(&self);
    pub fn delete(&self, name: &str);
}
// menu_bar.rs (macOS)
pub fn set_tab_sets(window: &AppWindow, names: &[String]);
// app.slint
in property <[MenuEntry]> bar-tab-sets;
```

**Behavior:**
- `navigation.rs`:

```rust
    /// Opens a tab set (spec 5.2): `locations` as tabs at the end, the first in front (a
    /// visit); `replace` first closes the unlocked tabs, and the status bar then says how many
    /// locked ones stayed. A folder gone on this computer falls back when it is shown.
    pub fn open_tab_set(&self, locations: Vec<Location>, replace: bool) {
        if locations.is_empty() {
            return;
        }
        let locked = self.with_tabs(|tabs| tabs.open_set(locations, replace));
        let front = self.tab_id(self.active_index());
        self.0.borrow_mut().visit_next_show = front;
        self.after_tabs_changed_noted((locked > 0).then(|| locked_kept_text(locked)));
    }

    /// Every tab's location, in tab order.
    pub fn tab_locations(&self) -> Vec<Location> {
        self.0.borrow().tabs.iter().map(|history| history.location().clone()).collect()
    }
```
- `tab_sets.rs`:

```rust
//! Tab sets (spec 5.2): the open tabs saved under a name in settings.toml (`[[tab-sets]]`,
//! paths with `{home}`-style tokens, "drives" for This PC) and opened again from a tab's menu
//! or the macOS Window menu. Saving and deleting go through the one settings.toml writer, as
//! the saved filters do (filter.rs).

use std::cell::RefCell;

use gezik_config::Warning;
use gezik_config::paths::KnownDirs;
use gezik_config::settings::TabSet;
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use slint::ComponentHandle;

use crate::AppWindow;
use crate::context_menu::{TAB_SET_DELETE_FIRST, TAB_SET_MAX, TAB_SET_OPEN_FIRST, TAB_SET_REPLACE_FIRST};
use crate::dialog::Dialogs;
use crate::navigation::Navigator;
use crate::view::View;

thread_local! {
    /// The tab sets of this (UI) thread, for the menus and the actions.
    static CURRENT: RefCell<Option<TabSets>> = const { RefCell::new(None) };
    /// `[[tab-sets]]` of the settings in effect.
    static SETS: RefCell<Vec<TabSet>> = const { RefCell::new(Vec::new()) };
    /// Saves on their way to settings.toml: the last one's number, and the list after it while
    /// any is on its way (the next edit builds on it).
    static WRITING: RefCell<(u64, Option<Vec<TabSet>>)> = const { RefCell::new((0, None)) };
}

/// settings.toml changed (every resolve).
pub fn set_settings(sets: Vec<TabSet>) {
    SETS.with(|s| *s.borrow_mut() = sets);
}

/// The tab sets, in settings.toml's order.
pub fn saved() -> Vec<TabSet> {
    SETS.with(|s| s.borrow().clone())
}

/// Their names, for the menus.
pub fn names() -> Vec<String> {
    SETS.with(|s| s.borrow().iter().map(|set| set.name.clone()).collect())
}

/// The sets as the next edit sees them: after the saves on their way, if any.
fn latest() -> Vec<TabSet> {
    WRITING.with(|w| w.borrow().1.clone()).unwrap_or_else(saved)
}

fn queue_write(sets: Vec<TabSet>) -> u64 {
    WRITING.with(|w| {
        let mut w = w.borrow_mut();
        w.0 += 1;
        w.1 = Some(sets);
        w.0
    })
}

/// Save `seq` of `sets` ended with `result`: written, they are in effect; not, they stay as
/// settings.toml has them and the error is returned, to be said.
fn finish_write(seq: u64, sets: Vec<TabSet>, result: Result<(), Warning>) -> Option<String> {
    WRITING.with(|w| {
        let mut w = w.borrow_mut();
        if w.0 == seq {
            w.1 = None;
        }
    });
    match result {
        Ok(()) => {
            set_settings(sets);
            None
        }
        Err(warning) => Some(warning.to_string()),
    }
}

/// The set called `name`, ignoring case (names are unique that way).
fn find(sets: &[TabSet], name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    sets.iter().position(|set| set.name.to_lowercase() == name)
}

/// Whether `name` can be saved next to `sets`: a name already there is replaced; a new one
/// needs fewer than `TAB_SET_MAX` (what the menu lists).
fn room_for(sets: &[TabSet], name: &str) -> bool {
    find(sets, name).is_some() || sets.len() < TAB_SET_MAX as usize
}

/// Puts `set` in the place of the one of its name (which takes this spelling), or at the end.
fn put(sets: &mut Vec<TabSet>, set: TabSet) {
    match find(sets, &set.name) {
        Some(i) => sets[i] = set,
        None => sets.push(set),
    }
}

/// Removes the set called exactly `name`; whether it was there.
fn remove(sets: &mut Vec<TabSet>, name: &str) -> bool {
    let Some(i) = sets.iter().position(|set| set.name == name) else { return false };
    sets.remove(i);
    true
}

/// A tab's place as a set keeps it: "drives", or the path with tokens.
pub fn text_of(location: &Location, dirs: &KnownDirs) -> String {
    match location {
        Location::Drives => "drives".to_owned(),
        Location::Path(path) => dirs.collapse(path),
    }
}

/// A set's entry as a place; one with `..` is none (settings.toml leaves those sets out).
pub fn location_of(text: &str, dirs: &KnownDirs) -> Option<Location> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("drives") {
        return Some(Location::Drives);
    }
    dirs.expand_checked(text).map(Location::Path)
}

/// What a tab set menu item does, by its id: open, replace the tabs with, or delete set N.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetItem {
    Open(usize),
    Replace(usize),
    Delete(usize),
}

pub fn set_item(id: u32) -> Option<SetItem> {
    let index = |first: u32| (first..first + TAB_SET_MAX).contains(&id).then(|| (id - first) as usize);
    index(TAB_SET_OPEN_FIRST)
        .map(SetItem::Open)
        .or_else(|| index(TAB_SET_REPLACE_FIRST).map(SetItem::Replace))
        .or_else(|| index(TAB_SET_DELETE_FIRST).map(SetItem::Delete))
}

/// Runs `f` with this UI thread's tab sets, if there are any yet.
pub fn with_current(f: impl FnOnce(&TabSets)) {
    if let Some(sets) = CURRENT.with(|c| c.borrow().clone()) {
        f(&sets);
    }
}

#[derive(Clone)]
pub struct TabSets {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    view: View,
    dialogs: Dialogs,
    /// Where settings.toml is (none without a config folder: the sets live in memory then).
    store: Option<ConfigStore>,
}

impl TabSets {
    pub fn new(window: &AppWindow, nav: Navigator, view: View, dialogs: Dialogs, store: Option<ConfigStore>) -> TabSets {
        let sets = TabSets { window: window.as_weak(), nav, view, dialogs, store };
        CURRENT.with(|c| *c.borrow_mut() = Some(sets.clone()));
        sets
    }

    /// Tab set menu item `id` was chosen; `names` are the sets the menu listed, by place.
    pub fn chosen(&self, id: u32, names: &[String]) {
        let Some(item) = set_item(id) else { return };
        let (SetItem::Open(i) | SetItem::Replace(i) | SetItem::Delete(i)) = item;
        let Some(name) = names.get(i) else { return };
        match item {
            SetItem::Open(_) => self.open(name, false),
            SetItem::Replace(_) => self.open(name, true),
            SetItem::Delete(_) => self.delete(name),
        }
    }

    /// Opens set `name` (if it is still there) after the open tabs; `replace` closes the
    /// unlocked ones first.
    pub fn open(&self, name: &str, replace: bool) {
        let sets = saved();
        let Some(i) = find(&sets, name) else { return };
        let dirs = KnownDirs::system();
        let locations: Vec<Location> = sets[i].tabs.iter().filter_map(|text| location_of(text, &dirs)).collect();
        self.nav.open_tab_set(locations, replace);
    }

    /// "Save tabs as…": asks for a name, then saves the tabs open now under it; a name already
    /// there (ignoring case) is replaced only if the answer says so.
    pub fn ask_save(&self) {
        let dirs = KnownDirs::system();
        let tabs: Vec<String> = self.nav.tab_locations().iter().map(|location| text_of(location, &dirs)).collect();
        let this = self.clone();
        self.dialogs.ask_text("Save tabs", "Name for these tabs:", "", &["Save", "Cancel"], move |name| {
            let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
            let sets = latest();
            match find(&sets, &name) {
                Some(i) => {
                    let message = format!("A tab set called \"{}\" already exists. Replace it?", sets[i].name);
                    let again = this.clone();
                    this.dialogs.ask("Save tabs", message, &["Replace", "Cancel"], move |choice| {
                        if choice == Some(0) {
                            again.save(TabSet { name, tabs });
                        }
                    });
                }
                None => this.save(TabSet { name, tabs }),
            }
        });
    }

    fn save(&self, set: TabSet) {
        let mut sets = latest();
        if !room_for(&sets, &set.name) {
            return self.view.note(format!("Up to {TAB_SET_MAX} tab sets"));
        }
        put(&mut sets, set);
        self.write(sets);
    }

    /// Deletes set `name`, if it is still there.
    pub fn delete(&self, name: &str) {
        let mut sets = latest();
        if remove(&mut sets, name) {
            self.write(sets);
        }
    }

    /// Sends `sets` to settings.toml; the menus list them once written.
    fn write(&self, sets: Vec<TabSet>) {
        let Some(store) = &self.store else { return set_settings(sets) };
        let seq = queue_write(sets.clone());
        let window = self.window.clone();
        store.write_settings(SettingsChange::TabSets(sets.clone()), move |result| {
            let _ = window.upgrade_in_event_loop(move |_| with_current(|t| t.written(seq, sets, result)));
        });
    }

    fn written(&self, seq: u64, sets: Vec<TabSet>, result: Result<(), Warning>) {
        if let Some(note) = finish_write(seq, sets, result) {
            self.view.note(note);
        }
    }
}
```
- `context_menu.rs`: sabitler (belge: "1020: Save tabs as…. 1100-1129: open tab set N; 1130-1159: replace the tabs with set N; 1160-1189: delete set N."), ve:

```rust
/// "Open tab set ▸": each set (opened after the tabs), then "Replace tabs with" each, then
/// "Delete" each (up to `TAB_SET_MAX` each), as the filter menu lists its own.
pub fn tab_set_items(names: &[String]) -> Vec<(u32, String, bool)> {
    let shown = names.iter().take(TAB_SET_MAX as usize).enumerate();
    let mut list: Vec<(u32, String, bool)> =
        shown.clone().map(|(i, name)| (TAB_SET_OPEN_FIRST + i as u32, name.clone(), true)).collect();
    list.extend(
        shown.clone().map(|(i, name)| (TAB_SET_REPLACE_FIRST + i as u32, format!("Replace tabs with \"{name}\""), true)),
    );
    list.extend(shown.map(|(i, name)| (TAB_SET_DELETE_FIRST + i as u32, format!("Delete \"{name}\""), true)));
    list
}
```
  `items`'in `Place::Tab` kolunun sonuna `out.push((SAVE_TAB_SET, "Save tabs as…"));`. `Subject::Tab(u64)` → `Subject::Tab(u64, Vec<String>)` (belge: "A tab by id, and the tab set names its menu listed"); var olan kollar `Subject::Tab(id, _)`. `tab`:

```rust
    /// Right-click on tab `index`, at window position `x`, `y`. Tabs get Gezik's own menu
    /// everywhere, with "Open tab set ▸" when there are sets.
    pub fn tab(&self, index: usize, x: f32, y: f32) {
        let Some(id) = self.nav.tab_id(index) else { return };
        let place = Place::Tab { only_tab: self.nav.tab_count() == 1, locked: self.nav.is_tab_locked(index) };
        let list: Vec<(u32, String, bool)> =
            items(place, false).into_iter().map(|(id, title)| (id, title.to_owned(), true)).collect();
        let names = crate::tab_sets::names();
        let subs = vec![Submenu { title: "Open tab set".to_owned(), at: list.len(), items: tab_set_items(&names) }];
        *self.subject.borrow_mut() = Some(Subject::Tab(id, names));
        self.open_slint_entries(&list, subs, Anchor::point(x, y));
    }
```
  `run`'a:

```rust
            (SAVE_TAB_SET, Subject::Tab(..)) => crate::tab_sets::with_current(crate::tab_sets::TabSets::ask_save),
            (id, Subject::Tab(_, names)) if crate::tab_sets::set_item(id).is_some() => {
                crate::tab_sets::with_current(|sets| sets.chosen(id, &names));
            }
```
  `from_submenu`'ya `|| crate::tab_sets::set_item(id).is_some()`.
- `actions.rs`: `Action::SaveTabSet => crate::tab_sets::with_current(crate::tab_sets::TabSets::ask_save)`, listeden çıkar.
- `main.rs`: `mod tab_sets;`; `let dialogs = dialog::Dialogs::new(&window);`'dan sonra `let _tab_sets = tab_sets::TabSets::new(&window, nav.clone(), view.clone(), dialogs.clone(), config.clone());`; `apply_config`'te `tab_sets::set_settings(loaded.settings.tab_sets.clone());` ve ardından `#[cfg(target_os = "macos")] menu_bar::set_tab_sets(window, &tab_sets::names());`.
- `menu_bar.rs`: `on_menu_command`'ın `name =>` kolunun başına, `command:`'dan sonra:

```rust
                if let Some(id) = name.strip_prefix("tab-set:").and_then(|i| i.parse::<u32>().ok()) {
                    let names = crate::tab_sets::names();
                    return crate::tab_sets::with_current(|sets| sets.chosen(id, &names));
                }
```
  ve:

```rust
/// The Window menu's "Open Tab Set": the sets to open, to replace the tabs with, to delete.
pub fn set_tab_sets(window: &AppWindow, names: &[String]) {
    let entries: Vec<MenuEntry> = crate::context_menu::tab_set_items(names)
        .into_iter()
        .map(|(id, title, enabled)| MenuEntry {
            id: i32::try_from(id).unwrap_or(-1),
            title: crate::context_menu::menu_title(&title).into(),
            enabled,
        })
        .collect();
    window.set_bar_tab_sets(ModelRc::new(VecModel::from(entries)));
}
```
- `app.slint`: `in property <[MenuEntry]> bar-tab-sets;` (`bar-commands`'ın yanına); Window menüsünde "Lock Tab" öğesinden sonra:

```slint
            MenuSeparator { }
            MenuItem { title: "Save Tabs As…"; activated => { root.menu-command("save-tab-set"); } }
            if root.bar-tab-sets.length > 0: Menu {
                title: "Open Tab Set";
                for entry in root.bar-tab-sets: MenuItem {
                    title: entry.title;
                    enabled: entry.enabled;
                    activated => { root.menu-command("tab-set:" + entry.id); }
                }
            }
```
  (`save-tab-set`'in tuşu yok: `menu_bar` onu `actions::run` ile çalıştırır.)

- [ ] **Step 1: Write the failing tests**

`tab_sets.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn set(name: &str, tabs: &[&str]) -> TabSet {
        TabSet { name: name.into(), tabs: tabs.iter().map(|t| (*t).to_owned()).collect() }
    }

    fn home() -> std::path::PathBuf {
        std::env::temp_dir().join("u")
    }

    fn dirs() -> KnownDirs {
        KnownDirs::new(vec![("home", home()), ("downloads", home().join("İndirilenler"))])
    }

    #[test]
    fn tabs_are_kept_with_tokens_and_drives() {
        let dirs = dirs();
        let downloads = home().join("İndirilenler");
        assert_eq!(text_of(&Location::Path(downloads.clone()), &dirs), "{downloads}");
        assert_eq!(text_of(&Location::Drives, &dirs), "drives");
        assert_eq!(location_of("{downloads}", &dirs), Some(Location::Path(downloads)));
        assert_eq!(location_of(" Drives ", &dirs), Some(Location::Drives));
        assert_eq!(location_of("{home}/../x", &dirs), None);
    }

    #[test]
    fn saving_replaces_the_same_name_ignoring_case() {
        let mut sets = vec![set("Work", &["/a"]), set("Media", &["/m"])];
        put(&mut sets, set("work", &["/b"]));
        assert_eq!(sets, [set("work", &["/b"]), set("Media", &["/m"])]);
        put(&mut sets, set("New", &["drives"]));
        assert_eq!(sets.len(), 3);
        assert!(remove(&mut sets, "Media") && !remove(&mut sets, "Media"));
    }

    #[test]
    fn thirty_sets_at_most() {
        let full: Vec<TabSet> = (0..TAB_SET_MAX).map(|i| set(&format!("S{i}"), &["/a"])).collect();
        assert!(!room_for(&full, "New"));
        assert!(room_for(&full, "s3"), "replacing one needs no room");
    }

    #[test]
    fn menu_ids_name_the_item() {
        assert_eq!(set_item(TAB_SET_OPEN_FIRST + 2), Some(SetItem::Open(2)));
        assert_eq!(set_item(TAB_SET_REPLACE_FIRST), Some(SetItem::Replace(0)));
        assert_eq!(set_item(TAB_SET_DELETE_FIRST + TAB_SET_MAX - 1), Some(SetItem::Delete(29)));
        assert_eq!(set_item(TAB_SET_DELETE_FIRST + TAB_SET_MAX), None);
        assert_eq!(set_item(crate::context_menu::SAVE_TAB_SET), None);
    }

    #[test]
    fn a_save_reaches_settings_toml() {
        let dir = std::env::temp_dir().join(format!("gezik-tab-sets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ConfigStore::new(dir.clone());
        set_settings(Vec::new());
        let wanted = vec![set("Release", &["{downloads}", "drives"])];
        let seq = queue_write(wanted.clone());
        assert_eq!(latest(), wanted, "the next edit builds on the save on its way");
        assert_eq!(finish_write(seq, wanted.clone(), store.save_tab_sets(&wanted)), None);
        assert_eq!(saved(), wanted);
        let text = std::fs::read_to_string(dir.join("settings.toml")).unwrap();
        assert!(text.contains("[[tab-sets]]") && text.contains("name = \"Release\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```
`context_menu.rs`:

```rust
    #[test]
    fn the_tab_set_menu_opens_replaces_and_deletes() {
        let names = vec!["Work".to_owned(), "Media".to_owned()];
        let items = tab_set_items(&names);
        let ids: Vec<u32> = items.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(
            ids,
            [
                TAB_SET_OPEN_FIRST,
                TAB_SET_OPEN_FIRST + 1,
                TAB_SET_REPLACE_FIRST,
                TAB_SET_REPLACE_FIRST + 1,
                TAB_SET_DELETE_FIRST,
                TAB_SET_DELETE_FIRST + 1
            ]
        );
        assert_eq!(items[2].1, "Replace tabs with \"Work\"");
        assert_eq!(items[5].1, "Delete \"Media\"");
        assert!(tab_set_items(&[]).is_empty());
        let many: Vec<String> = (0..TAB_SET_MAX + 3).map(|i| format!("S{i}")).collect();
        assert_eq!(tab_set_items(&many).len(), 3 * TAB_SET_MAX as usize);
    }
```
  `the_tab_menu_offers_the_lock_and_no_close_on_a_locked_tab`'in dört listesinin sonuna `SAVE_TAB_SET`; `conversion_ids_meet_no_others`'a `SAVE_TAB_SET` ve üç aralık (`TAB_SET_*_FIRST..+TAB_SET_MAX`).

- [ ] **Step 2: Run to see them fail.** `cargo test -j 8 -p gezik tab_sets context_menu` → FAIL.
- [ ] **Step 3: Implement** (yukarıdaki kod).
- [ ] **Step 4: Run.** `cargo test -j 8 --workspace` → PASS; çapraz denetimler (macOS denetimi Window menüsünün iç içe `if … : Menu`'sünü derler; reddederse `Open Tab Set` alt menüsü ve `set_tab_sets` çıkarılır, "Save Tabs As…" kalır ve nota yazılır). Elle (Windows): üç sekme (biri This PC) → sekmeye sağ tık → "Save tabs as…" → "Release" → `settings.toml`'da `[[tab-sets]]` ve `tabs = […, "drives"]`, yollar `{home}` belirteçleriyle; aynı adla yeniden → "Replace?"; sekmeye sağ tık → "Open tab set ▸" → "Release" → üç sekme sona eklenir, ilki önde; bir sekmeyi kilitle → "Replace tabs with "Release"" → kilitli kalır, durum çubuğu "1 locked tab stays open"; "Delete "Release"" → menüden gider.
- [ ] **Step 5: Commit** "Save the open tabs as a set and open a set from the tab menu and the macOS Window menu".

---

### Task 10: Ölçüm, Linux kabı (`gui.sh terminal`, `copy-path`, `session`, `tabsets`), notlar, test listeleri, spec

**Files:**
- Modify: `scripts/linux/gui.sh` (dört kip, `wayland()`'a bir denetim, `fresh_files` ve yeniden başlatan kiplere `restore = false`), `scripts/perf/tabs.ps1` (`-Session`), `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` ("## Alt proje 7a (Günlük kolaylıklar: terminal, yolu kopyala, oturum, sekme setleri) sonrası"), `docs/superpowers/notes/macos-test.md`, `docs/superpowers/notes/linux-test.md`, `docs/superpowers/specs/2026-10-08-gunluk-kolayliklar-design.md` (Durum)

- [ ] **Step 1: Exe.** `cargo build --release -p gezik -j 8`; `stat -c %s target/release/gezik.exe`. Taban 22.240.256 bayt (Global Constraints), sınır **22.502.400**. Payı görev görev ayırmak için her görev commit'inde sürüm derlemesi gerekirse `git stash`/`git checkout <commit>` yerine ayrı bir çalışma ağacı (`git worktree add ../gezik-7a-measure <commit>`, kendi `target`'ı) kullanılır. Aşılırsa ölçülen dağılım (Slint dört alt menü yuvası, terminal, pano, sekme setleri) notlara ve daraltma önerisiyle kullanıcıya sorulur.
- [ ] **Step 2: Bellek ve açılış.** Windows'ta (oturum açıksa) master exe'si ayrı çalışma ağacında (`git worktree add ../gezik-master 38c65b3`, `cargo build --release -p gezik -j 8`) ve 7a exe'si için `scripts/perf/measure.ps1 -Runs 5 -Config <boş geçici klasör>` (Görev Yöneticisi belleği, master 7,2–7,3 MB; büyümemeli; açılış ≤ ~60 ms). Oturumlu açılış: `scripts/perf/tabs.ps1 -Session 20` (aşağıda) → açılış süresi ve bellek, tek sekmeyle karşılaştırılır (≤ +2 MB, Gezinme spec'i). Linux kabında `scripts/linux/gui.sh memory` master ve 7a exe'leriyle (5'er çalıştırma; RssAnon aynı kalmalı). `tabs.ps1`'e:

```powershell
# -Session N: no tabs are typed; Gezik starts with a state.toml of N tabs (folders under %TEMP%)
# in a config folder of its own, and the time to its window is measured: only the tab in front
# is read (spec 5.1).
param(
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Tabs = 20,
    [ValidateSet("PostMessage", "SendKeys")] [string]$Method = "PostMessage",
    [int]$Session = 0
)
```
  ve `$p = Start-Process …`'tan önce:

```powershell
if ($Session -gt 0) {
    $cfg = Join-Path $env:TEMP "gezik-session-perf"
    Remove-Item -Recurse -Force $cfg -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $cfg | Out-Null
    $lines = @("[session]", "active = 0", "")
    for ($i = 0; $i -lt $Session; $i++) {
        $dir = Join-Path $env:TEMP "gezik-session-perf-tabs\t$i"
        New-Item -ItemType Directory -Force $dir | Out-Null
        $lines += "[[session.tabs]]", ("path = '" + $dir + "'"), ""
    }
    Set-Content -Encoding utf8 (Join-Path $cfg "state.toml") $lines
    $env:GEZIK_CONFIG_DIR = $cfg
    $times = @()
    for ($r = 0; $r -lt 5; $r++) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $q = Start-Process $Exe -PassThru
        try {
            if ((Find-GezikWindow $q.Id 10000) -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
            $times += $sw.ElapsedMilliseconds
            Start-Sleep -Seconds 3
            $mem = Mem $q.Id
        } finally { Stop-Process -Id $q.Id -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 500
    }
    "{0} saved tabs: open {1:N0} ms | Task Manager memory {2:N1} MB" -f $Session, ($times | Measure-Object -Average).Average, $mem
    return
}
```
- [ ] **Step 3: Linux kabı.** `docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/test.sh unit` → hepsi geçer (Wayland `text_is_offered_as_text_only`, `latin1_keeps_what_it_can`, terminal seçimi testleri dahil). `gui.sh`:
  - `fresh_files`'ın sonuna `printf '[session]\nrestore = false\n' > /tmp/cfg/settings.toml` (bugünkü tek sekmeli açılış). `grep -n 'GEZIK_CONFIG_DIR=/tmp/cfg' scripts/linux/gui.sh` her başlatmayı listeler; aynı `/tmp/cfg` ile ikinci kez başlatan kip (`keyboard`'ın `typing = "filter"` yeniden başlatması, `wayland`'ın arşiv bölümü) yazdığı `settings.toml`'a da `[session]\nrestore = false` ekler.
  - `wayland()`'da "copy and paste" denetiminden sonra:

```bash
    vclick 260 "$(wrow /tmp/t b.txt)"; wk -P Control_L -P Shift_L -k c -p Shift_L -p Control_L; sleep 0.5
    check "wayland: Ctrl+Shift+C puts the path on the clipboard as text" '[ "$(wl-paste -n 2>/dev/null)" = /tmp/t/b.txt ]'
```
  - Dört kip (başlık yorumundaki listeye, `case`'e ve `*)` "all" listesine; `copy-path`'in işlevi `copy_path`):

```bash
# 7a: "Open terminal" with Debian's x-terminal-emulator (xterm), $TERMINAL, a known terminal
# (a fake kitty on PATH), [terminal] command, and none found; from the keys and the menus.
terminal() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    command -v xterm >/dev/null || { apt-get update -qq >/dev/null; DEBIAN_FRONTEND=noninteractive \
        apt-get install -y -qq --no-install-recommends xterm >/dev/null 2>&1; }
    rm -rf /tmp/te /tmp/cfg /tmp/fakebin /tmp/empty /tmp/term-*.txt
    mkdir -p "/tmp/te/my dir" /tmp/te/Docs /tmp/cfg /tmp/fakebin /tmp/empty
    echo x > /tmp/te/f.txt
    # A fake terminal: writes the folder it ran in, then its arguments, one per line.
    printf '#!/bin/sh\nprintf "%%s\\n" "$PWD" "$@" > /tmp/term-run.txt\n' > /tmp/fakebin/kitty
    chmod +x /tmp/fakebin/kitty
    printf '[session]\nrestore = false\n' > /tmp/cfg/settings.toml
    : >/tmp/gezik-gui-terminal.log
    local gezik=
    run() {  # run [VAR=value…]: (re)starts Gezik on /tmp/te with that environment
        kill $gezik 2>/dev/null; wait $gezik 2>/dev/null
        rm -f /tmp/term-*.txt
        env "$@" GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/te >>/tmp/gezik-gui-terminal.log 2>&1 &
        gezik=$!
        sleep 3
        xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    }
    line() { sed -n "$1p" /tmp/term-run.txt 2>/dev/null; }
    xterm_in() { local pid; pid=$(pgrep -n xterm) && [ "$(readlink /proc/$pid/cwd)" = "$1" ]; }

    # Rows: Docs 118, my dir 144, f.txt 170.
    run
    click 255 144; key shift+F4; sleep 2
    check "terminal: Shift+F4 opens x-terminal-emulator (xterm) in the focused folder" 'xterm_in "/tmp/te/my dir"'
    pkill xterm; sleep 0.5
    click 255 170; key ctrl+alt+t; sleep 2
    check "terminal: Ctrl+Alt+T on a file opens in its folder" 'xterm_in /tmp/te'
    kill $gezik; wait $gezik 2>/dev/null; gezik=; sleep 1
    check "terminal: it stays open after Gezik ends" 'pgrep -x xterm >/dev/null'
    pkill xterm

    run TERMINAL=/tmp/fakebin/kitty
    click 255 144; key shift+F4; sleep 1.5
    check "terminal: \$TERMINAL comes first, in the folder" '[ "$(line 1)" = "/tmp/te/my dir" ] && [ -z "$(line 2)" ]'

    run PATH=/tmp/fakebin
    click 255 144; key shift+F4; sleep 1.5
    check "terminal: a known terminal gets its folder flag" \
        '[ "$(line 1)" = "/tmp/te/my dir" ] && [ "$(line 2)" = --directory ] && [ "$(line 3)" = "/tmp/te/my dir" ]'
    menu /tmp/te Docs 2; sleep 1.5
    check "terminal: Open terminal here in a folder's menu" '[ "$(line 3)" = /tmp/te/Docs ]'
    # Empty space: the background menu (New folder, New file, Refresh, Open terminal here).
    rclick 255 400; click 330 $((400 + 20 + 32 * 3)); sleep 1.5
    check "terminal: Open terminal here on empty space: the folder shown" '[ "$(line 3)" = /tmp/te ]'

    run PATH=/tmp/empty
    click 255 144; key shift+F4; sleep 1.5; shot terminal-none
    check "terminal: none found: nothing runs" '[ ! -e /tmp/term-run.txt ]'

    cat > /tmp/cfg/settings.toml <<'EOF'
[session]
restore = false

[terminal]
command = ["/bin/sh", "-c", "pwd > /tmp/term-cwd.txt; printf '%s' \"$1\" > /tmp/term-arg.txt", "sh", "{dir} {{x}}"]
EOF
    sleep 2
    click 255 144; key shift+F4; sleep 1.5
    check "terminal: [terminal] command runs in the folder with {dir} put in" \
        '[ "$(cat /tmp/term-cwd.txt 2>/dev/null)" = "/tmp/te/my dir" ] && [ "$(cat /tmp/term-arg.txt 2>/dev/null)" = "/tmp/te/my dir {x}" ]'
    kill $gezik $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-terminal.log && fail "terminal: no panic" || pass "terminal: no panic"
}

# 7a: Copy path (Ctrl+Shift+C and "Copy path as ▸" in Gezik's own menu) puts text on the X11
# clipboard, and leaves the files Gezik cut alone.
copy_path() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/cp /tmp/cfg && mkdir -p /tmp/cp/Docs /tmp/cfg
    echo a > "/tmp/cp/it's ş #1.txt"; echo b > /tmp/cp/b.txt
    printf '[session]\nrestore = false\n' > /tmp/cfg/settings.toml
    : >/tmp/gezik-gui-copypath.log
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/cp >>/tmp/gezik-gui-copypath.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    text() { xclip -selection clipboard -o -t UTF8_STRING 2>/dev/null; }
    # POSIX single quotes, as Gezik writes them: ' becomes '\''.
    pq() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }
    local q="/tmp/cp/it's ş #1.txt"
    # Rows: Docs 118, b.txt 144, it's ş #1.txt 170 (by name, ignoring case).

    # The background menu by the menu key: New folder, New file, Refresh, Open terminal here,
    # Copy path as ▸ (Full path first).
    click 255 400; key Escape Menu; sleep 0.5; key Down Down Down Down Down Right Return; sleep 0.5
    check "copy-path: the background menu copies the folder shown" '[ "$(text)" = /tmp/cp ]'
    click 255 170; key ctrl+shift+c; sleep 0.5
    check "copy-path: Ctrl+Shift+C copies the full path" '[ "$(text)" = "$q" ]'
    check "copy-path: as text, not as files" \
        'xclip -selection clipboard -o -t TARGETS | grep -qx UTF8_STRING && xclip -selection clipboard -o -t TARGETS | grep -qx STRING && ! xclip -selection clipboard -o -t TARGETS | grep -q uri-list'
    xdotool keydown shift; click 255 144; xdotool keyup shift; key ctrl+shift+c; sleep 0.5
    check "copy-path: several items one per line" '[ "$(text)" = "$(printf "%s\n%s" /tmp/cp/b.txt "$q")" ]'
    key Escape; key ctrl+shift+c; sleep 0.5
    check "copy-path: no selection: the folder shown" '[ "$(text)" = /tmp/cp ]'
    # A file's menu by the menu key: Open, Open with default app, Open terminal here, Copy
    # path as ▸ (Full path, Quoted, Name, Folder path, file:// URL).
    pick() { click 255 170; key Menu; sleep 0.5; key Down Down Down Down Right; for _ in $(seq 1 "$1"); do key Down; done; key Return; sleep 0.5; }
    pick 1
    check "copy-path: Quoted for the shell" '[ "$(text)" = "$(pq "$q")" ]'
    pick 2
    check "copy-path: Name" '[ "$(text)" = "$(basename "$q")" ]'
    pick 4
    check "copy-path: file:// URL" '[ "$(text)" = "file:///tmp/cp/it%27s%20%C5%9F%20%231.txt" ]'
    # Cut b.txt, then copy a path: the cut is over, Ctrl+V in Docs moves nothing.
    click 255 144; key ctrl+x; sleep 0.5; click 255 170; key ctrl+shift+c; sleep 0.5
    check "copy-path: the clipboard has no files then" '[ -z "$(copied)" ]'
    dclick 255 118; key ctrl+v; sleep 1.5
    check "copy-path: a later Ctrl+V moves nothing" '[ -f /tmp/cp/b.txt ] && [ ! -e /tmp/cp/Docs/b.txt ]'
    kill $gezik $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-copypath.log && fail "copy-path: no panic" || pass "copy-path: no panic"
}

# 7a: the tabs of last time: written as they change (so a kill keeps them), back in order with
# the one in front and the locks, a command-line folder after them, a gone folder falling
# back, and restore = false.
session() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/se /tmp/cfg && mkdir -p /tmp/se/one /tmp/se/two /tmp/se/three /tmp/se/four /tmp/cfg
    printf '[shortcuts]\ntoggle-tab-lock = "ctrl+shift+l"\n' > /tmp/cfg/settings.toml
    : >/tmp/gezik-gui-session.log
    local gezik=
    run() {  # run [path]: (re)starts Gezik, on the path if one is given
        [ -n "$gezik" ] && { kill $gezik 2>/dev/null; wait $gezik 2>/dev/null; }
        GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK "$@" >>/tmp/gezik-gui-session.log 2>&1 &
        gezik=$!
        sleep 3
        xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    }
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    goto() { key ctrl+l; typ "$1"; key Return; sleep 1; }
    tabs_kept() { grep -c '^\[\[session.tabs\]\]' /tmp/cfg/state.toml 2>/dev/null; }

    run /tmp/se/one
    click 255 300
    key ctrl+t; sleep 1; goto /tmp/se/two
    key ctrl+t; sleep 1; goto /tmp/se/three
    key ctrl+shift+l; sleep 0.5
    key ctrl+2; sleep 2
    check "session: the tabs go to state.toml as they change" '[ "$(tabs_kept)" = 3 ] && grep -qx "active = 1" /tmp/cfg/state.toml'
    # Killed, not closed: what was written as the tabs changed comes back.
    kill -9 $gezik; wait $gezik 2>/dev/null; gezik=
    run
    check "session: the tab in front comes back in front" 'is two'
    key ctrl+1; sleep 1; check "session: the first tab is back" 'is one'
    key ctrl+9; sleep 1; check "session: so is the last" 'is three'
    key ctrl+w; sleep 1; check "session: and its lock" 'is three'
    run /tmp/se/four
    check "session: a folder on the command line opens after them, in front" 'is four'
    key ctrl+3; sleep 1; check "session: the saved tabs come before it" 'is three'
    key ctrl+2; sleep 2
    kill $gezik; wait $gezik 2>/dev/null; gezik=
    rm -rf /tmp/se/two
    run
    check "session: a folder gone since falls back to the nearest one there" 'is se'
    printf 'start-folder = "/tmp/se/one"\n\n[shortcuts]\ntoggle-tab-lock = "ctrl+shift+l"\n\n[session]\nrestore = false\n' > /tmp/cfg/settings.toml
    sleep 2
    check "session: restore = false forgets the tabs" '! grep -q "session" /tmp/cfg/state.toml'
    run
    check "session: then one tab opens in start-folder" 'is one'
    key ctrl+2; sleep 1; check "session: and only one" 'is one'
    kill $gezik $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-session.log && fail "session: no panic" || pass "session: no panic"
}

# 7a: tab sets from a tab's menu (Gezik's own): open after the tabs, replace keeping a locked
# tab, save with a key, delete.
tabsets() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/ts /tmp/cfg && mkdir -p /tmp/ts/a /tmp/ts/b /tmp/ts/home /tmp/cfg
    cat > /tmp/cfg/settings.toml <<'EOF'
[session]
restore = false

[shortcuts]
save-tab-set = "ctrl+alt+s"
toggle-tab-lock = "ctrl+shift+l"

[[tab-sets]]
name = "Work"
tabs = ["/tmp/ts/a", "/tmp/ts/b", "drives"]
EOF
    : >/tmp/gezik-gui-tabsets.log
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/ts/home >>/tmp/gezik-gui-tabsets.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    # A tab's menu (one tab): Duplicate, Lock tab, Close, Save tabs as…, Open tab set ▸ with
    # Work, Replace tabs with "Work", Delete "Work". The first tab locked, with others:
    # Duplicate, Unlock tab, Close other tabs, Save tabs as…, Open tab set ▸ (fifth line too).
    tab_menu() { rclick 100 20; sleep 0.5; for _ in $(seq 1 "$1"); do key Down; done; key Right; for _ in $(seq 1 "$2"); do key Down; done; key Return; sleep 1.5; }
    tab_menu 5 0
    check "tabsets: Open tab set opens the set's first tab in front" 'is a'
    key ctrl+9; sleep 1; check "tabsets: after the open tab, ending with This PC" 'is "This PC"'
    key ctrl+1; sleep 1; check "tabsets: the open tab stays first" 'is home'
    key ctrl+shift+l; sleep 0.5
    tab_menu 5 1
    check "tabsets: Replace tabs with keeps the locked tab" 'is a'
    key ctrl+1; sleep 1; check "tabsets: the locked tab is first, the others went" 'is home'
    key ctrl+4; sleep 1; check "tabsets: one locked tab and the set's three" 'is "This PC"'
    key ctrl+alt+s; sleep 0.8; typ Mine; key Return; sleep 2
    check "tabsets: Save tabs as… writes the open tabs" \
        'grep -q "name = \"Mine\"" /tmp/cfg/settings.toml && grep -q "\"drives\"" /tmp/cfg/settings.toml'
    tab_menu 5 4
    check "tabsets: Delete removes a set" '! grep -q "name = \"Work\"" /tmp/cfg/settings.toml'
    kill $gezik $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-tabsets.log && fail "tabsets: no panic" || pass "tabsets: no panic"
}
```
  `case`'e `terminal) terminal ;;`, `copy-path) copy_path ;;`, `session) session ;;`, `tabsets) tabsets ;;`. Başlıktaki kip listesi ve `*)` listesi güncellenir.

  Koş: `… gezik-linux bash scripts/linux/gui.sh terminal`, `copy-path`, `session`, `tabsets`, sonra `all`. Kip içindeki tıklama yerleri (menü satırı sırası, sekme menüsünün `Down` sayısı) düşerse önce ekran görüntüsüne (`.superpowers/linux-shots/`) bakılır, sayılar düzeltilir; uygulamadaki bir hata düzeltilir ve yeniden koşulur. Eski kiplerden düşen olursa (oturum yüzünden ikinci başlatmada fazladan sekme) o kipin `settings.toml`'una `restore = false` eklenir. Sonuçlar notlara.
- [ ] **Step 4: Notlar** (Türkçe, 6b bölümünün kalıbında): ölçümler (exe: master'a göre bayt ve MiB, sınırla karşılaştırma, varsa dağılım; boşta bellek Windows ve Linux; 20 sekmelik oturumla açılış ve bellek), sapmalar (1-23 kısaca; özellikle `ShellExecuteW`, PowerShell'in `[`'i ve `wt`'nin varsayılan profili, x-terminal-emulator çözümü, dört alt menü yuvası ve alt menüde ayraç olmaması, 4096 sınırı, oturumun sürekli yazılması), `gui.sh` sonuçları (yeni dört kip ve `all`), Windows ekran testi sonuçları (aşağıdaki liste), denenemeyenler (macOS: Terminal.app, ⌘⌥T, ⌘⌥C, Window ▸ Save Tabs As… / Open Tab Set; gerçek UAC "Yes" ile yönetici terminali ve eşlenmiş ağ sürücüsünde UNC, oturum açıksa; Wayland'da terminal; GNOME/KDE masaüstünde öne alınan terminal).
  - `macos-test.md` "7a, daily" başlığıyla (sıradaki numaralar): (a) bir klasörde ⌘⌥T ve File ▸ Open Terminal → Terminal.app o klasörde; sağ tık "Open terminal here"; (b) ⌘⌥C ve Edit ▸ Copy Path, "Copy path as ▸" → Quoted (`'…'\''…'`) Terminal'e yapıştırılınca aynı dosya, file:// URL Safari'de açılır; (c) üç sekme, biri kilitli, ⌘Q, yeniden aç → aynı sekmeler, önde olan ve kilit; (d) Window ▸ Save Tabs As… → ad → Window ▸ Open Tab Set ▸ ad; (e) `[terminal] command = ["open", "-a", "iTerm", "{dir}"]`.
  - `linux-test.md`: "Keys on Linux" tablosuna Open terminal (Shift+F4, Ctrl+Alt+T: GNOME/Ubuntu Ctrl+Alt+T'yi kendisi alır), Copy path (Ctrl+Shift+C); checklist'e: masaüstünün kendi terminali (GNOME'da Ptyxis/Console, KDE'de Konsole) doğru klasörde, `$TERMINAL`, "Copy path as ▸" biçimleri bash'e ve Firefox'a yapıştırılınca, oturum ve sekme setleri, Wayland'da Ctrl+Shift+C.
  - Spec Durum: "Tasarım onaylandı (2026-10-07); 7a uygulandı".
- [ ] **Step 5: Commit** "Measure 7a and note what it does".

---

## Ekran testleri (Windows, alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici. Test verisi `%TEMP%\gezik-gui-7a\`: `my dir\`, `x;y\`, `[köşeli] it's\`, `Docs\`, `it's ş #1.txt`, `b.txt`, `100% done.txt`. Windows Terminal kurulu (sonra Uygulama yürütme takma adları'ndan `wt.exe` kapatılarak madde 3). Klavye düzeni önce ABD, sonra Türkçe Q (madde 13).

1. `my dir` odakta → Shift+F4 → Windows Terminal `…\gezik-gui-7a\my dir`'de açılır (istemde klasör); `x;y` odakta → Shift+F4 → `…\x;y`'de (`;` kaçışı); `[köşeli] it's` → Windows Terminal'de açılır (varsayılan profil PowerShell 5.1 ise kendi klasörüne düşebilir: notlara).
2. Dosya satırında Ctrl+Alt+T → klasöründe; adres çubuğu yazıyorken Shift+F4 alanın (terminal açılmaz); "This PC"de sürücü odakta → Shift+F4 → sürücünün kökünde; This PC'de odak yokken bir şey olmaz.
3. Ayarlar ▸ Uygulamalar ▸ Uygulama yürütme takma adları'nda `wt.exe` kapalı → Shift+F4 → PowerShell yeni konsolda `my dir`'de; `[köşeli] it's`'te de doğru klasörde (`-LiteralPath`). `wt` yeniden açılır.
4. Sağ tık (Explorer menüsü) → üstte Gezik öğeleri: "Open terminal here", "Open terminal as administrator", "Copy path as ▸" (Commands ▸ de varsa ikisi birlikte, yerlerinde). "Open terminal as administrator" → UAC → No → hiçbir şey, durum çubuğu değişmez; Yes → yönetici Windows Terminal o klasörde. Bir ağ payı `Z:`'ye eşlenmişse `Z:\` altındaki bir klasörde yönetici terminali `\\sunucu\pay\…`'da açılır.
5. Klasör boşluğunda ve kenar çubuğunda sabitlenmiş bir klasörde sağ tık → aynı üç öğe, o klasör için. Gezik'i kapat → açılan terminaller açık kalır.
6. `it's ş #1.txt` seçili → Ctrl+Shift+C → durum çubuğu "Copied the path"; cmd'de `type <yapıştır>` dosyayı yazar (Quoted ile), PowerShell'de `Get-Item <Quoted>` bulur, Explorer'ın adres çubuğuna Full path, tarayıcıya file:// URL (`%27`, `%20`, `%C5%9F`, `%23`) aynı dosyayı açar. With forward slashes, Name, Folder path beklenen metni verir. Üç öğe → "Copied 3 paths", satırlar `\r\n` ile.
7. `b.txt` Ctrl+X (soluk) → başka bir öğede Ctrl+Shift+C → `b.txt` soluk görünmez olur (pencere odağı gidip gelince ya da menü açılınca); Docs'ta Ctrl+V bir şey taşımaz. Explorer'da bir dosya kopyala → Gezik'te Ctrl+V yapıştırır (dosya panosu bozulmadı).
8. Eşlenmiş sürücüde bir dosyada "Copy path as ▸" → "UNC path" görünür ve `\\sunucu\pay\…` verir; yerel diskte görünmez. Sürücü kökünde (`C:\` This PC'de seçili) Name ve Folder path `C:\`.
9. Adres çubuğu yazıyorken Ctrl+Shift+C alanın (bir şey kopyalanmaz); süzgeç alanında da.
10. Dört sekme (biri This PC, biri kilitli), ikincisi önde → pencereyi kapat → yeniden aç: aynı sırayla, ikincisi önde, kilit simgesi yerinde, açılış süresi tek sekmeyle aynı (`tabs.ps1 -Session 20`'nin sayılarıyla). Görev Yöneticisi'nden sonlandır → yeniden aç → aynı.
11. Kapalıyken sekmelerden birinin klasörünü sil, bir sekmeyi erişilemeyen bir ağ yoluna (`\\10.255.255.1\x`) götürüp kapat → yeniden aç: pencere hemen gelir; erişilemeyen sekme önde değilse açılış beklemez; silinen klasöre geçince "… no longer exists" ve üst klasör.
12. `gezik.exe C:\Windows` → kayıtlı sekmelerin sonunda yeni sekme, önde; `gezik.exe C:\yok` → sekmeler yine gelir, bildirim "…: not found; opening the last tabs". `settings.toml`'a `[session] restore = false` → `state.toml`'dan `[session]` kalkar; yeniden aç → tek sekme `start-folder`.
13. Türkçe Q: Ctrl+Alt+T (AltGr+T `₺` yazar) terminal açmaz, Shift+F4 açar; adres çubuğunda AltGr+T `₺` yazar.
14. Sekmeye sağ tık → "Save tabs as…" → "Release" → `settings.toml`'da `[[tab-sets]]`, yollar `{home}`/`{downloads}` belirteçleriyle, This PC `"drives"`; aynı adla (harf büyüklüğü farklı) yeniden → "Replace?"; "Open tab set ▸ Release" → sekmeler sona, ilki önde; bir sekme kilitliyken "Replace tabs with "Release"" → kilitli kalır, "1 locked tab stays open"; "Delete "Release"" → menüden gider. Elle yazılmış `..`'lı bir set bildirim verir ve Gezik kaydedince dosyada aynen kalır.
15. Exe boyutu (≤ 22.502.400 bayt) ve boşta bellek (Görev Yöneticisi, master'dan büyük değil), Task 10'un sayılarıyla.

---

## Self-review

- **Spec kapsamı:** §3.1 eylem ve yerleri (odak klasörü, dosyanın klasörü, gösterilen klasör, This PC'de sürücü) → Task 7 (`folder_for`, `run`'daki `OPEN_TERMINAL` kolu); menülerde "Open terminal here" ve Windows'ta yönetici öğesi → Task 7 (`terminal_items`); `open-terminal-admin` başka sistemde "Only on Windows" → Task 4 (`OnlyOnWindows`) + Task 7 (`open_in`). §3.2 saf seçim → Task 4 (`choose`, `Lookup`); `[terminal] command`, `{dir}`, `{{`, kabuk yok → Task 1 + Task 4; Windows `wt` → `pwsh` → `powershell`, `;` kaçışı, yeni konsol → Task 4 (sapma 1-3); yönetici `runas`, argümanla klasör, `''`, UNC, UAC No → Task 4 (sapma 2, 15); macOS `open -a Terminal` → Task 4; Linux sırası, masaüstüne göre öne alma, bayraklar → Task 4 (sapma 4-5); Gezik'ten bağımsız süreç → Task 4 (`start`); bulunamadı ve başlatma hatası metinleri → Task 7 (`error_text`). §4.1 `copy-path`, seçim yoksa klasör, This PC → Task 7 (`copy_selection`, sapma 14); "Copy path as ▸" satır, kenar çubuğu, boşluk → Task 7; yedi biçim, UNC görünürlüğü, çok satır ve satır sonu → Task 1 + Task 7. §4.2 saf biçimler → Task 1; `WNetGetConnectionW` → Task 4; `write_text` üç sistem, `Operations.clip` değişmez → Task 5 (sapma 16). §5.1 `[session] restore`, `state.toml` biçimi, sürekli yazım, açılışta yalnız etkin sekme, varlık denetimi yok, `Gone` geri düşüşü, komut satırı, bozuk girdiler → Task 2 + Task 3 + Task 8 (sapma 17-19). §5.2 `[[tab-sets]]`, `{home}`, `"drives"`, `..` ve boş ad, büyük/küçük harf, Save tabs as…, Open tab set ▸ (aç, değiştir, sil, 30), yazıcıdan geçme → Task 3 + Task 9 (sapma 9, 20, 21). §10.1 şablon → Task 3; §10.2 → Task 3; §10.3 eylemler, varsayılan tuşlar, çakışma denetimi, Ctrl+Alt kuralı, macOS menü çubuğu → Task 3 + Task 7 + Task 9 (sapma 12, 13; `pin-1…9`, `new-folder-with-selection`, `add-to-stack` ve rakam tuşu kuralı 7b/7c'nin). §11 kod yapısı → dosya listeleri (`tab_sets.rs` ve `copy_path.rs` yeni, sapma 21). §11.1 birden çok alt menü ve 4096 → Task 6 (sapma 8, 10). §12 birim testleri (yol biçimleri, terminal seçimi, oturum, sekme setleri, kısayollar), platform testleri (pano metni, UNC'de `None`, terminali çalıştırmadan kurma), Linux kabı (`terminal`, `copy-path`, `session` ve sekme setleri), Windows ekran testleri, macOS → Task 1-10 ve ekran listesi. §13 açılış (yalnız etkin sekme, `tabs.ps1` 20 sekme), bellek, exe (taban baytla) → Global Constraints + Task 10.
- **Yer tutucu taraması:** Task 3'teki `actions.rs` kolları bilerek `false` döner ve Task 7 (`OpenTerminal`, `OpenTerminalAdmin`, `CopyPath`) ile Task 9 (`SaveTabSet`) doldurur. Task 6'da `app.slint`'in dört yuvası bir blokla ve "aynısı `[1]`, `[2]`, `[3]` için" diye verildi: kalıp birebir aynı, yalnız indeks değişir. Task 9'da macOS `if … : Menu` derlenmezse ne yapılacağı yazılı. Gövdesi yazılı olmayan test yok.
- **Tip tutarlılığı:** Task 1 `PathFormat::{ALL, label, offered}`, `format_paths`, `unc_path`, `posix_quote`, `file_url`, `file_name`, `folder_of`, `check_dir_command`, `expand_dir_command`. Task 2 `SessionTab`, `Session::{is_empty, single, active_location}`, `Tabs::{push (özel), from_session, session, open_set}`. Task 3 `SessionSettings`, `TerminalSettings`, `TabSet`, `Settings.{session, terminal, tab_sets}`, `State.session`, `parse_tab_set`, `tab_set_to_toml`, `with_tab_sets`, `SettingsChange::TabSets`, `ConfigStore::save_tab_sets`, `Action::{OpenTerminal, OpenTerminalAdmin, CopyPath, SaveTabSet}`. Task 4 `Os`, `Launch`, `TerminalError`, `Lookup { os, command, terminal_var, desktop, found, real_name }`, `KNOWN`, `choose`, `windows_command_line`, `wt_dir`, `open`, `fs::mapped_remote`. Task 5 `clipboard::write_text`, `Backend::write_text`, `latin1`, `Owned.text`, `TEXT_TYPES`. Task 6 `FIRST_SHELL_ID`, `GEZIK_IDS_END`, `MAX_SUBMENUS`, `split_menu`, `menu_lines`, `from_submenu`, `submenu_places`, `MenuSub { title, entries, after }`, `menu-subs`, `subs`. Task 7 `OPEN_TERMINAL`, `OPEN_TERMINAL_ADMIN`, `COPY_PATH_FIRST`, `terminal_items`, `copy_path_items`, `Menus::copy_path_sub`, `terminal::{set_settings, folder_for, error_text, open_in, open_for_view, ONLY_ON_WINDOWS, NOT_FOUND}`, `copy_path::{copied_text, unc_offered, copy, copy_selection}`. Task 8 `StartPlan { session, select, start, warnings }`, `StartPlan::first`, `plan_start(…, saved)`, `Navigator::{new(…, session, …), keep_session, set_session_restore}`, `session_to_send`. Task 9 `SAVE_TAB_SET`, `TAB_SET_{OPEN,REPLACE,DELETE}_FIRST`, `TAB_SET_MAX`, `tab_set_items`, `Subject::Tab(u64, Vec<String>)`, `Navigator::{open_tab_set, tab_locations}`, `tab_sets::{set_settings, saved, names, text_of, location_of, SetItem, set_item, with_current, TabSets::{new, chosen, open, ask_save, delete}}`, `menu_bar::set_tab_sets`, `bar-tab-sets`. Task 6'nın `add_file_tools(list, subs, rows, native)` imzası Task 7'nin çağrılarıyla aynı; Task 8'in `Navigator::new` imzası main.rs'teki tek çağrıda güncellenir.
- **Review Focus:** beş maddenin her biri adı geçen görevde test olarak var: 1 → Task 1'in beş testi + `gui.sh copy-path`; 2 → Task 8 `a_saved_session_is_never_looked_at_on_disk` + `gui.sh session`; 3 → Task 3 `broken_session_entries_are_left_out`, `bad_session_terminal_and_tab_sets_warn`, `tab_sets_are_written_keeping_the_rest`; 4 → Task 4 `windows_terminal_first_then_powershell`, `powershell_always_gets_its_folder_as_a_literal_path`, `windows_command_lines_split_back_into_the_arguments`, `a_command_in_settings_wins_everywhere` + `gui.sh terminal`; 5 → Task 5 `text_is_offered_as_text_only`, `latin1_keeps_what_it_can` + `gui.sh copy-path`.
