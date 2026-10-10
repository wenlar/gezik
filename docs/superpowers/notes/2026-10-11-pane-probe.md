# 10a-0 yoklaması: bir pencerede iki `PaneView` (Slint 1.18.1)

Spec: `docs/superpowers/specs/2026-10-11-duzenler-design.md` §3.2, §13.1 (10a-0), §9.2 (10g yoklaması, isteğe bağlı kısım).
Kod: `docs/superpowers/notes/pane-probe/` (atılabilir; ürün koduna girmez). `pane.slint` (`PaneView`), `app.slint`
(`for p[i] in panes: PaneView`), `main.rs` (kipler), `run.ps1` (bellek/CPU örnekleyici), `build.rs` (`PROBE_STATIC=1`
iki elle yazılmış örnek). Çıktılar: `check.out.txt`, `bench.out.txt`.

Derleme: `Cargo.toml.txt` → `Cargo.toml` ve kökteki `Cargo.lock` yanına kopyalanır (Slint yamalı 1.18.1'de kalsın diye),
`cargo build --release -j 4`. Gezik'in Slint özellikleri (yazılım çizici, winit, sistem yazı tipleri, erişilebilirlik),
`vendor/i-slint-core` yaması, Gezik'in release profili. Liste Gezik'in kendi `ListView`'u (`widgets/scrollbar.slint`,
satır geri dönüşümlü), satırlar Gezik'in `ItemsModel`'i gibi istendikçe üretilir (`row_data` sayılır).

Makine: Windows 11 24H2, 2026-10-10. Pencere 1000×700 mantıksal, birincil ekran. Olaylar `Window::dispatch_event` ile
verildi (gerçek klavye/fare kullanılmadı, klavye düzenlerine dokunulmadı). **Makine başka derlemelerle meşguldü**
(CPU yükü %63–87): CPU sayıları gürültülüdür, bellek sayıları değildir.

## Özet

- **Spec §3.2'nin yolu çalışıyor; kök özelliğe (`p0-`/`p1-`) düşmek gereken alan çıkmadı.** `[PaneData]` modeli +
  bölme numaralı geri çağrılar + sayaçlı istekler (`scroll-request`, `rename-request`, `focus-request`) 21/21 denetimi geçti.
- Bir bölmenin `PaneData`'sına yazmak **öbür bölmeye hiç dokunmuyor** (0 `row_data`, 0 bağ değerlendirmesi), odak
  korunuyor (örnek yerinde güncelleniyor, yeniden kurulmuyor). Ama **yazılan bölmenin `data`'ya bağlı bütün bağları**
  yeniden değerlendiriliyor (görünen 28 satırın hepsi): struct tek özelliktir. Seçim gibi sık değişenler `PaneData`'ya
  değil, bugünkü gibi satır modeline (`FileRow.selected`) gitmeli.
- İkinci bölme (100k satır) boşta **+0,29–0,55 MB**; kapatınca model bırakılıyor (zayıf tutamak) ve bellek +0,05 MB'a dönüyor.
  Exe: ikinci örnek kod eklemiyor (+512 B, elle yazılmış iki örnek; `for` ile örnek sayısından bağımsız).
- Kaydırma CPU'su (ortanca, bir çekirdeğin %'si): tek bölme tam genişlik **~11**, iki bölmede biri kayarken **~10**,
  ikisi birden **~17**.
- Tab'ı `PaneView` kendisi almalı: Slint'in kendi Tab zinciri bölme 0 → bölme 1 → pencere `FocusScope`'una gidiyor, bölmeler
  arasında dönmüyor.
- Bölmeler arası sürükleme tek pencerede sorunsuz: fare yakalaması basılan bölmede kalır, koordinatlar pencereye göre gelir;
  her bölme liste dikdörtgenini bir geri çağrıyla bildirir, Rust'ın saf isabet testi (`gezik-core` `drag.rs`) korunabilir.
- (İsteğe bağlı, 10g) İkinci pencere aynı olay döngüsünde: **+1,6 MB**, kapatınca bırakılıyor, modeli düşüyor.

## Sorular, cevaplar, kanıt

### 1. Bir `AppWindow` iki `PaneView`'u `[PaneData]` modeli ve bölme numaralı geri çağrılarla taşıyabilir mi?

**Evet.** `app.slint`: `for p[i] in root.panes: PaneView { data: p; index: i; … }` bir `HorizontalLayout` içinde.
`PaneData`: `path`, `items: [Row]`, `selected`, `active`, `scroll-request`/`scroll-to`, `rename-request`/`rename-row`/
`rename-text`, `focus-request`. Geri çağrılar `(pane, …)` taşır: `item-pressed`, `key`, `tab`, `scrolled`, `rename-edited`,
`rename-done`, `focus-changed`, `drag-start/move/end`, `drop-target`, `ready`, `geometry`.

Kanıt (`check.out.txt`):
- `both instances created`, `each pane reports its list rect` (0: x 0, 499 genişlik; 1: x 501).
- `pane 0 keeps focus across set_row_data`: `set_row_data(0, …)` sonrası hiçbir odak olayı yok.
- Sayaçlar: bölme 0'da seçim değişimi → `row_data a=0 b=0 | row evals p0=28 p1=0`; yol (skaler) değişimi → aynı.
  Bölme 1'de 10.000 satır aşağı kaydırma isteği → `b=29`, `p1=28`, `p0=0`.
- İç modelin (`items`) aynı `ModelRc` ile yeniden yazılması iç tekrarlayıcıyı sıfırlamıyor (`row_data` 0).

### 2. Bölme başına sanal liste (`ListView`) kaydırma, odak, tuşlarla çalışıyor mu?

**Evet.**
- Kaydırma: `scroll-request` yalnız bölme 1'i kaydırdı (`p0=0 p1=240000`). Fare tekerleği olayı imlecin altındaki bölmeye gider.
- Tuşlar: Gezik'in pencere çapında `capture-key-pressed`'i korunduğunda tuş önce oraya (`wkey Down`), Rust geri çevirince
  odaktaki bölmenin `FocusScope`'una (`key 0 Down`) gider; Tab'dan sonra `key 1 Down`. Yani iki yol da var: Rust etkin bölmeyi
  kendisi bilir (bugünkü `key-event` aynen kalır), bölme içi tuşlar da bölme numarasıyla gelir.
- Odak: `focus-request` sayacı `fs.focus()` çağırır; bölme odağı `focus-changed(pane, bool)` ile Rust'a bildirilir.

### 3. Modelden geçemeyenler; geri çağrı ve sayaç gerekenler

Rust, tekrarlayıcının içindeki örneğe ulaşamaz: örnek tutamağı yok, `out` özelliği okunamaz, işlev (`focus()`) çağrılamaz.
Bu yüzden:

| Bugün | Bölmede | Kanıt |
|---|---|---|
| `scroll <=> root.list-scroll`, Rust `get_list_scroll()` okur | Bileşen içinde; `changed content-y => scrolled(pane, y)` Rust'ta aynasını tutar; geri yükleme `scroll-request` sayacı | PASS: üç gezinme biçimi (yeni model + istek tek yazımda; yeni model sonra istek aynı tikte; `notify.reset()` sonra istek) |
| `rename-text <=>`, `rename-has-focus <=>` | Metin bileşende; `rename-edited(pane, text)`, `rename-done(pane, text)`; başlatma `rename-request` | PASS. Not: programla verilen metinde imleç başta kalır (`xabc.txt`); ürünün `RenameField`'ı seçimi kendisi kurar, 10a-3'te bakılmalı |
| `focus-list` gibi işlev çağrıları | `focus-request` sayacı | PASS |
| `drop-geometry` (`out`) + `get_list_scroll()` | `geometry(pane, x, y, w, h)` (`changed` liste konumu/boyutu) + `scrolled` aynası | PASS; bölme kapanınca bölme 0 tam genişliği yeniden bildirdi |
| `col-* <=>` | **Bölmeye girmez**: §3.3 gereği ortak; `AppWindow`'da kök özellik kalır, iki `PaneView`'a `in` geçer | (tasarım; ölçüm gerekmedi) |
| `path-editing <=>` | `path-editing-changed(pane, on)` + istek sayacı (aynı desen; yoklamada ayrıca denenmedi) | — |

**Dikkat — `changed` ilk değerde çalışmaz.** Yeni kurulan örnek (bölme açıldı, sekme taşındı) sayaçtaki isteği görmez;
`PaneView`'un `init`'i de aynı isteği uygulamalı. Yoklamada `init` + `scroll-to: 4800` → tekerlek adımından sonra `4824`
(PASS: kaydırma düzenden sonra korunuyor).

`SCROLL_RESTORE_DELAY`: istek, yeni modelle aynı yazımda ve `notify.reset()`'ten hemen sonra da tuttu. Yoklama, gecikmenin
sebebini (satır sayısının sonradan değiştiği okuma) yeniden üretmedi; gecikmeli ikinci istek aynı sayacı bir kez daha artırmaktır,
yani bugünkü yol değişmeden taşınır.

### 4. İkinci bölmenin bellek ve çizim maliyeti (100k satır, yazılım çizici)

| Ölçüm | Değer |
|---|---|
| Boşta, tek bölme (`bench 1`) | 5,73–5,80 MB |
| Boşta, iki bölme baştan (`bench 2`) | 6,28–6,35 MB (**+0,55**) |
| Tek bölme → ikinci açıldı → kapandı (`openclose`, 5 koşu) | 6,01–6,05 → 6,20–6,35 (**+0,29**) → 6,06–6,10 (**+0,05**); model bırakıldı ×5 |
| Kaydırma CPU, tek bölme tam genişlik | 7,7–13,8 %, ortanca **11,1** (8 koşu) |
| Kaydırma CPU, iki bölme, biri kayıyor | 8,4–18,9 %, ortanca **10,4** |
| Kaydırma CPU, iki bölme, ikisi kayıyor | 9,4–25,9 %, ortanca **16,6** |
| Exe, `for` | 9.403.904 B |
| Exe, iki elle yazılmış örnek | 9.404.416 B (**+512 B**) |
| Probe sandığının release derlemesi (LTO, -j 4) | 42–56 s, iki biçimde de (gürültü) |

Yorum: yalnız görünen satırlar örneklendiği için 100k satır maliyeti satır sayısından bağımsız; ikinci bölmenin maliyeti
görünen ~28 satırın öğeleri ve bölmenin kendi ağacı. Biri kayarken öbür bölmenin varlığı CPU'yu artırmıyor (öbür bölme
kirlenmiyor). İkisi birden kaydığında ~1,5×: iki liste ayrı ayrı satır dönüştürüyor. 10a-3'ün gerçek exe/derleme farkı
çıkarım işinin kendisidir ve orada `master` ile ölçülmeli; bu yoklama yalnız ikinci örneğin kod eklemediğini gösterir.

### 5. Klavye odağı ve Tab

- `PaneView` Tab/Shift+Tab'ı kendisi kabul edip `tab(pane, shift)` derse Rust öbür bölmeyi etkin yapar ve `focus-request`'i
  artırır: PASS (`tab 0 false`, `focus 1 true`, `focus 0 false`; geri `tab 1 true`).
- Slint'in kendi zinciri (Tab reddedilince): bölme 0 → bölme 1 → **pencere `FocusScope`'u** (`focus 99 true`), sonra
  zincirde ne varsa (ad alanları, çubuklar). Bölmeler arasında dönmez. Spec karar 9 (Tab yalnız iki bölmede ve liste
  odaktayken) bu yüzden bileşenin kendi `key-pressed`'inde uygulanmalı; tek bölmede ve yazma alanlarında reddedip bugünkü
  zincire bırakır.
- Ad alanı (`TextInput`) odak aldığında bölmenin `FocusScope`'u odak kaybeder (`focus 10 true`), Return'de geri alır.

### 6. Tek pencerede bölmeler arası sürükleme

PASS: bölme 0'da basıp bölme 1'in 6. görünen satırına gelince bölme 1 kendi satırını buldu (`drop 1 1505`: bölme 1 1500.
satırda), bırakma **basılan bölmeye** (`drag-end 0 750 156`) pencere koordinatlarıyla geldi. Slint fareyi basılan
`TouchArea`'ya bağlar; öbür bölmenin `has-hover`'ı sürükleme boyunca çalışmaz, bu yüzden hedef vurgusu sürükleme konumundan
hesaplanmalı (bugünkü `drag-x/drag-y` pencere özellikleri, §3.1'deki gibi pencere düzeyinde).

İki yol denendi ve ikisi de çalışıyor: (a) her bölme pencere sürükleme noktasından kendi satırını hesaplar (`drop-target`);
(b) her bölme liste dikdörtgenini `geometry(pane, …)` ile bildirir, Rust kaydırma aynasıyla birlikte `gezik-core`'un saf isabet
testine verir. **Öneri (b)**: bugünkü `Layout { list: ListArea }` bölme listesine (`lists: [ListArea; 1..2]`) genişler, isabet
testi saf ve birim testli kalır.

### 7. (İsteğe bağlı, 10g madde 1) Bir olay döngüsünde iki pencere

`windows` kipi, 3 + 4 koşu: tek pencere 7,34–7,40 MB → ikinci pencere (1000×700, 100k satırlı bir bölme) 8,97–9,04 MB
(**+1,6 MB**) → `hide()` + bırakma sonrası 6,00–6,16 MB; ikinci pencerenin modeli her koşuda bırakıldı. Spec'in "ek pencere
+2,5 MB ve kapanınca bırakılır" bütçesi bu boyutta tutuyor (çerçeve arabelleği pencere alanıyla büyür; büyük pencerede ölçülmeli).
Açıklanamayan bir şey: bu kipte tek pencereli başlangıç değeri `openclose`'un aynı durumdaki değerinden ~1,35 MB yüksek ve
kapanıştan sonra ondan aşağı iniyor; farklar (+1,6, bırakılıyor) koşular arasında tutarlı, mutlak değer değil.
Denenmedi: winit `Focused` olaylarının pencere kimliği (madde 2), macOS menü çubuğu (madde 3), sekme koparma, pencereler arası
OLE bırakma — 10g'nin kendi yoklamasına kalıyor.

## Öneriler

**10a-1 (`Pane`/`WindowCtx`):** Rust tarafında bölme başına anlık durum aynaları gerekecek: `scroll` (bugün `get_list_scroll()`
okuyan her yer: geçmiş girdisi, sürükleme, `views.toml`), ad alanı metni, adres yazma kipi, liste dikdörtgeni. Bunlar `Pane`'e
alan olarak konur; 10a-3'e kadar tek bölmede bugünkü kök özelliklerden doldurulur, böylece 10a-3 yalnız kaynağı değiştirir.

**10a-2:** yoklamayla ilgisi yok (değişiklik yok).

**10a-3 (`PaneView` çıkarımı):**
1. Spec §3.2'deki biçim: `for p[i] in panes: PaneView`; kök özellik takımı yok.
2. `PaneData`'ya yalnız seyrek değişenler: `items`/`tabs`/`crumbs` modelleri, yol, kip, sıralama, süzgeç/arama durumu, `active`,
   istek sayaçları. Her `PaneData` yazımı o bölmenin bütün `data` bağlarını yeniden değerlendirir; seçim, odak halkası, kesik,
   klasör boyutu bugünkü gibi satır modelinde (`notify_plan`) kalır. Rust'ta `edit(pane, |d| …)` tek yardımcı, aynı tikte
   birden çok alanı tek yazımda toplar.
3. İki yönlü bağlar: `scroll` → `scrolled` + `scroll-request`; `rename-text`/`rename-has-focus` → `rename-edited`/odak geri
   çağrısı + `rename-request`; `path-editing` → geri çağrı + istek; işlev çağrıları → `focus-request`. Her istek `changed`
   **ve** `init`'te uygulanır.
4. Sütun genişlikleri, sürükleme durumu (`drag-active/x/y`), tema ve pencere düzeyindeki her şey `AppWindow` kök özelliği kalır,
   `PaneView`'a `in` geçer.
5. Pencere çapındaki `keys` `FocusScope`'u kalır; etkin bölmeyi Rust bilir. Tab'ı `PaneView` alır (10b'de iki bölmede).
6. Geri çağrılar `AppWindow`'da bölme numarasıyla yeniden açılır (yoklamada 14 geri çağrı için 14 satır yönlendirme); Rust'ta
   `windows::with_focused(|w| w.panes[pane]…)`.

**10b:** kapatma testi için `Weak<ItemsModel>` deseni çalışıyor (`closing pane 1 frees its rows model`); bölme açma/kapama
yalnız `panes` modeline `push`/`remove`, düzen kendiliğinden ikiye bölünüyor/birleşiyor ve kalan bölme dikdörtgenini yeniden
bildiriyor. Etkin olmayan bölmenin seçim rengi `active` alanından (yoklamada %45 saydamlık) — bölmeyi kirletir ama yalnız
etkin bölme değişiminde.

## Sınırlar

- Olaylar `dispatch_event` ile; gerçek tekerlek/klavye, IME, erişilebilirlik ağacı (iki listenin ekran okuyucuda ayrılması)
  denenmedi. Yalnız Windows.
- CPU ölçümleri meşgul makinede; 10a-3'te `scripts/perf` ile sessiz makinede tekrarlanmalı.
- Ürün `FileView`'unun ızgara kipi, sütun başlıkları, işaretleme dikdörtgeni yoklamada yok; bunlar `FileView` içinde kaldığı
  için bölme sınırından etkilenmez, yalnız `drop-geometry`/`list-scroll` okuyan Rust kodu yukarıdaki aynalara geçer.
