# Taşınabilirlik 14c (GitHub'dan tema) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ayar penceresinin Appearance sayfasında `Get Theme`: GitHub deposu (`github.com/k/d`, `…/tree/…`, `…/blob/….toml`) ya da herhangi bir https `.toml` adresi yapıştırılır; indirme iş parçacığında, sistem HTTP istemcisiyle (≤ 5 MB, yalnız https, yönlendirmeler de); zip bellekte okunur, diske açılmaz (≤ 64 KB tema, ≤ 32 tema); her tema bir satır, üstüne gelince önizleme, seçilince `themes/<ad>.toml` (ilk satırda kaynak). Kurulu temanın satırında `Use Now`, `Check for Updates` (elle), `Open Source Page`, `Delete Theme`. Tema yalnız veri (test kilitler). Exe ≤ master + 262.144 B.

**Spec:** `docs/superpowers/specs/2026-10-12-tasinabilirlik-design.md` §6, §10, §13.2 14c. Plan önceden onaylıdır. Dal `feat/portable-14c` (`origin/chore/merge-hotspots` üstüne: master + eylem listesi `actions!`), `D:\Work\gezik-14c`.

## Kararlar (soru sorulmadı)

1. **Gerçek sayılar:** `Action::ALL` +1 (`install-theme`, Get Theme…, tuşsuz; pencereyi Appearance'ta açar). Yeni menü kimliği yok (2850–2869 kullanılmadı): tema satırları 14a'nın satır bileşenini kullanır, seçimleri satırın açılır listesinde. Denetim listeleri macOS 370–371, Linux 269 (`checklists/14-macos.md`, `14-linux.md`).
2. **Arayüz yeni Slint kodu değil:** Get Theme bir metin satırı (tür 2), bulunan tema seçim satırı (tür 1: `Install` ya da `Replace`/`Keep Both`; seçeneğin üstüne gelmek — fare ya da ok tuşları — önizler), kurulamayan ya da aynısı kurulu tema düğmesiz satır (tür 6, gerekçe açıklamada), kurulu tema seçim satırı (tür 1: `Use Now`, `Check for Updates`, `Open Source Page`, `Delete Theme`). Klavyeyle baştan sona; ekran okuyucu adları 14a'nınki.
3. **Saf kısım `gezik-config/src/theme_source.rs`:** `resolve` (adres → indirme; sorgu/parça atılır; `http`, başka şema, kullanıcı adı/port, `..`, boşluk ve denetim karakteri reddedilir; github.com dışında yalnız `.toml`), `theme_entry` (GitHub'ın üst klasöründen sonra klasörün kökü ya da `themes/`), `candidate` (UTF-8, `parse_theme`, `[colors]`/`[metrics]`/`base`'ten biri), `file_id` (`[a-z0-9-]`, ≤ 48, yerleşik ve `auto` değil), `free_id`, `with_source`/`source_of`/`body`.
4. **Zip okuma `gezik_batch::archive::read_zip_entries`:** mevcut `zip` sandığıyla, bellekte; klasör ve bağlantı girdileri atlanır, yalnız istenen adların içi okunur, okunan bayt `take(64 KB + 1)` ile sayılır (sıkıştırma bombası).
5. **İndirme `gezik_platform::http::download`:** geçici dosyaya (`gezik-theme-<pid>-<n>.download`), okunur ve silinir. İptal: yeni adres, boş adres + Enter ya da pencerenin kapanması (sayaç). GitHub 403/429'da "60 downloads an hour" ipucu; Linux'ta araç yoksa ipucu.
6. **Hedef dosya:** aynı adres ve girdiden kurulmuş tema varsa o dosya (elle `frost` adlı kurulmuşsa güncelleme oraya), yoksa Gezik'in adı. Aynı içerik "Installed", farklı içerik `Replace`/`Keep Both` (`nord-2`). Yazma iş parçacığında `write_atomic`; izleyici yeniden yükler.
7. **Güvenlik testi:** `theme.rs` `a_theme_holds_data_only`: `PartialTheme` `..`suz açılır (yeni alan derlemeyi kırar); yol gibi `base`, `font-family`, bilinmeyen `run` anahtarı dosya açmaz, çalışmaz. `Open Source Page` yalnız `resolve`'dan geçen https adresini açar (elle yazılmış `file://` kaynak satırı açılmaz).

## Global Constraints

`RUST_MIN_STACK=67108864`, `-j 1` (tek derleme): `cargo fmt --all -- --check`, `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`; sonda macOS clippy (`-p gezik --target aarch64-apple-darwin --no-default-features -D warnings`), sürüm derlemesi ve boyut (master 28.903.936 B), kopya `%TEMP%\gezik-gui-14\bin\t14c.exe`. İngilizce commit, atıf satırı yok, amend yok, push yok. GUI yok.

## Task 1: Saf kısım ve zip
- [x] `theme_source.rs`, `theme.rs` testi, `archive::read_zip_entries`, `shortcuts.rs` (`InstallTheme`).
- [x] Testler: adres çözümü ve retler, girdi seçimi, aday denetimi, ad, kaynak satırı gidiş-dönüşü; zip sınırları (bomba, bağlantı, klasör, sayı).

## Task 2: Pencere
- [x] `settings_window/themes.rs` (satırlar, hedef, kurulum metni, önizleme, indirme iş parçacığı), `settings_window.rs` (satır eşleme, seçimler, önizleme, `open_themes`), `actions.rs`, `main.rs`, `trash_view.rs`.
- [x] Testler: sahte depo zip'iyle uçtan uca (ağsız: derin klasör, `..`, büyük dosya, tema olmayan TOML), karşılaştırma, Keep Both adı, elle adı değişmiş kurulumun güncellemesi, arama, tek dosya, yalnız https kaynak sayfası.

## Task 3: Ölçüm, denetimler, notlar
- [x] Sürüm exe boyutu (28.966.400 B, master + 62.464 B), macOS clippy, `notes/parts/14c.md`, spec Durum satırı, denetim listeleri, `docs/themes.md`, README, exe kopyası.

## Sapmalar (spec'ten)

1. **Sayılar tabana göre:** menü kimliği yok, denetim listeleri macOS 370–371, Linux 269 (spec 394–395, 285).
2. **Ayrı soru penceresi yok:** akış Appearance sayfasının satırlarında (bütçe; 14a pencere bütçeyi zaten aşmıştı). Kutular ve tek `Install` yerine her temanın kendi satırında `Install`.
3. **Renk şeridi yok:** önizleme bütün temayı gösterir; `Check for Updates` farkı renk şeridiyle değil "Differs" ve önizlemeyle gösterir.
4. **Tek tema kurulunca "Use it now?" sorulmaz:** durum satırı satırdaki `Use Now`'u söyler.
5. **Bilinmeyen anahtarlar listelenmez:** satır yalnız atlanan değerlerin sayısını söyler.
6. **`Delete Theme` ve diğerleri sağ tık menüsünde değil** satırın seçim listesinde; sağ tık 14a'nın menüsü (Copy Key girdinin yolunu kopyalar).
7. **İndirme ilerlemesi ve iptal soruda değil:** durum satırında KB; iptal yeni/boş adres ya da pencereyi kapatmak.
8. **`api.github.com` doğrudan yapıştırılmaz:** depo adresinden üretilir; `codeload` ikinci deneme yok (yönlendirmeyi istemci izler).
