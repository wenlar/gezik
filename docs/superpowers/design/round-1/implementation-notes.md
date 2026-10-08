# Claude Design aktarımı: uygulama notları

Kaynak: Claude Design projesi `912ea006-9cee-4d44-916c-4ea5bd430046` (2026-10-08 sürümü, "Turn 1").
Bu klasördeki kopyalar: `Gezik Window.dc.html` (ana pencere maketi, parametreli), `Gezik Design Pass.dc.html`
(iki yön, token tabloları, kontrast, metrikler, glif sayfası, bileşen notları), `gezik-tokens.js` (bütün renkleri
üreten türetme kodu), `github.md` (eşleme notu). `support.js` genel dc-runtime kodu (React yükleyici), tasarım
içermediği için alınmadı. Dosyalarda bize yönelik bir talimat yok.

Karşılaştırma tabanı: `design-brief.md` ve master `d585527` (`ui/theme.slint`, `ui/app.slint`, `ui/widgets/*.slint`,
`gezik-config/themes/*.toml`, `gezik-config/src/theme.rs`, `src/theme_bridge.rs`).

## 0. Kapsam: tasarım neyi kapsıyor, neyi kapsamıyor

Bu tur **yalnızca token'lar ve ana pencere** (liste, ızgara, sekmeler, araç çubuğu, kenar çubuğu, durum çubuğu).
Tasarımın kendi notu: menüler + alt menü, çakışma diyaloğu ve katman çerçevesi, işlemler paneli ve bırakma
yığını, arama çubuğu + Filters açılır paneli, komut paleti, araç ipucu, önizleme paneli, durum sayfaları (hover/
focus/disabled/drop) **"Next turn"** olarak bırakılmış. Brief'in istediği şu teslimler eksik: popup menü + alt menü,
çakışma diyaloğu, işlemler paneli (çalışan + geçmiş), arama çubuğu (kapalı + Filters), komut paleti. Bu yüzden
aşağıda bu yüzeyler için yalnızca "token değişince kendiliğinden ne olur" yazıyor.

Tasarım **iki yön** öneriyor, birinin seçilmesi gerekiyor:

- **1a Graphite** (vurgu Amber `#c2410c` / `#ff8f57`): soğuk gri "chrome" katmanı (sekme şeridi, araç çubuğu,
  kenar çubuğu, durum çubuğu) içinde kenarlıklı, yuvarlatılmış tek bir "içerik levhası" (sheet). Etkin sekme
  levha renginde, altında kısa 2 px vurgu çubuğu. Satır seçimi kenarlardan `spacing` kadar içeride, yuvarlatılmış.
  `radius` 6.
- **1b Ledger** (vurgu Teal `#0b7a69` / `#41d1b6`): tek sıcak düzlem, yapı 1 px çizgilerle. Sekmeler çizgiyle
  ayrılmış düz hücreler, etkinin altında tam genişlik 2 px vurgu çizgisi. Satırlar kenardan kenara, `radius` 3.
  Durum çubuğu hücreleri çizgiyle ayrılmış.

Her iki yönde: sistem fontu, 26 px satır, mevcut token adları korunuyor; 18 yeni renk token'ı; seçim/odak/
ilerleme/marquee/drop-target tek `accent`ten türetiliyor; magenta ile "yalnız accent değişti" kanıtı var.

## 1. Yüzey yüzey değişiklikler

### 1.1 Token'lar

**Renk.** Bütün nötrler değişiyor (bugünkü Windows-11 grilerinin yerine 1a'da soğuk grafit, 1b'de sıcak kâğıt
tonları). Vurgu mavi (`#005fb8` / `#60cdff`) yerine Amber ya da Teal. Seçim artık sabit mavi değil:
`selection = mix(background, accent, %15 açık / %30 koyu)`. `hover` daha hafif (`#0000000a` / `#ffffff0f`,
bugün `0d` / `14`). Simge renkleri hafifçe yeniden ayarlanmış (aynı sekiz tür). `accent-foreground` koyu temada
saf siyah değil, vurgunun koyulaştırılmışı (`#24140c` / `#091d19`).

**Yeni renk token'ları (18):** `surface-raised`, `chrome`, `border-strong`, `accent-hover`, `accent-pressed`,
`selection-foreground-muted`, `selection-inactive`, `pressed`, `tab-active`, `tab-inactive`, `input-background`,
`danger-background`, `success`, `warning`, `shadow`, `overlay`, `scrollbar`, `scrollbar-hover`.

**Türetme kuralları** (`gezik-tokens.js` `RULES`): `focus-ring = progress = accent`; `marquee = accent @ %14/%20`;
`drop-target = accent @ %20/%28`; `accent-hover/pressed = mix(accent, mürekkep, %12/%24)`; `accent-foreground` =
beyaz ya da koyu (hangisi daha kontrastlıysa); `selection-foreground-muted = mix(fg, muted, %35)`;
`selection-inactive = mix(bg, fg, %8/%11)`; `danger-background = mix(bg, danger, %9/%16)`;
`progress-paused = warning`; `progress-error = danger`; `scrollbar = fg @ %22`, `scrollbar-hover = fg @ %42`;
`tab-active = background`; `tab-inactive = chrome`.

**Kontrast** (hesaplandı, ikisi de geçiyor): fg/bg 14.5–17.9; muted/bg 6.5–6.9; muted/chrome 5.6–7.0;
sel-fg/sel 7.75–14.5; sel-fg-muted/sel 6.05–10.8; accent-fg/accent 5.18–9.17; danger/bg 6.1–7.1;
accent/bg 4.90–9.40; focus-ring/selection 3.99–4.83 (≥3); border-strong/input 3.42–3.68 (≥3). Magenta
vurguda da hiçbir çift düşmüyor.

**Metrikler.** Beş metrik anahtarı aynı (`font-size 13`, `row-height 26`, `icon-size 16`, `spacing 6`), yalnız
`radius` 1a'da 6, 1b'de **3**. Türetilenler (tema anahtarı değil): küçük yazı `font-size × 0.85` (11), başlık
`× 1.15` (15, 600), `radius-small = max(2, radius/2)`, sekme şeridi `row + 10` (36), araç çubuğu
`row + 2·spacing + 4` (42), glif çizgisi 1.5 / 1.2, odak halkası 2 px içe.
Yükseltme: düzey 1 = levha (1a) ya da çizgi (1b); düzey 2 = popup: `surface-raised` + `border` + tek gölge
(`shadow`, blur 12), yalnız popup ve katmanlarda. Hareket: hover/seçim renk geçişi 90 ms, popup opaklığı 140 ms,
"reduce motion" ile kapanabilir.

**Yoğunluk.** Tasarımın JS'i compact'ı `floor` ile hesaplıyor (20 / 4); bizim `Metrics::compact` `round`
kullanıyor (21 / 5). Tasarımın tablosu "compact → 20" diyor ama brief "yaklaşık 21". Önemsiz; bizim kural kalabilir.

**Font.** Değişmiyor: sistem fontu, gömülü font yok. Sayılar için "tabular-nums" isteniyor; Slint'te font özelliği
ayarı yok, ama Segoe UI / SF'nin varsayılan rakamları zaten eşit genişlikli. Bölüm etiketleri 600 ağırlık,
1a'da `letter-spacing 0.02em`, 1b'de `0.08em` (Slint `Text.letter-spacing` var). Etiketler kodda büyük harf
literal ("SABİTLENENLER"), çalışma anında büyütülmüyor (Türkçe İ/ı için).

**Glifler.** 17 glifli tutarlı bir set: 16×16 kutuda 1.5, 8×8 kutuda 1.2 çizgi, yuvarlak uç/birleşim:
Back, Forward, Up, Refresh, Plus, View (üç çizgi), Search, History (saat), Pin, Drive, Close, Chevron down,
Submenu ▸, Check, Lock, Folder (dolgu), Page (dolgu). Kenar çubuğu için ek olarak home, folder (çizgi),
alias (ok işaretli klasör). Yol komutları SVG göreli/kısa sözdiziminde (`a`, `h`, `v`, `l`, `Z`); Slint `Path`
`commands` SVG yol sözdizimini kabul ettiği için doğrudan yapıştırılabilir ama her biri bir kez derlenip
görsel olarak kontrol edilmeli (özellikle yay `a` komutları ve `h.01` nokta hilesi: Drive'daki `M11 11.25h.01`
yuvarlak uçla bir nokta çizdirir, yazılım çiziciyle denenmeli).

**Dosya simgesi yedeği.** Klasör: köşeleri yuvarlatılmış sekmeli klasör (yaylar içeriyor); sayfa: kıvrık köşeli
sayfa + köşede `background` renginde %55 opak üçgen. Bugün ikisi de düz çokgen. İkinci yol (köşe üçgeni) satır
başına ikinci bir `Path` demek; 100 000 satırda görünür olanlar kadar çizilir, ama satır şablonuna bir öğe
ekler. Alternatif: tek yolla (köşe kıvrımı boşluk olarak) çizmek.

### 1.2 Pencere düzeni

- Bilgi mimarisi aynı (sekme şeridi → araç çubuğu → kenar çubuğu | liste → durum çubuğu).
- Maketteki 30 px başlık çubuğu yalnızca çerçeve; OS başlık çubuğu yerli kalır, uygulanmayacak.
- **1a:** pencere zemini `chrome`; liste + başlık `background` renginde ayrı bir levha: sağda ve altta `spacing`
  boşluk, `border` kenarlık, `radius + 2` köşe. Sekme şeridi ile araç çubuğu arasında ve altında çizgi yok.
  Kenar çubuğu `chrome` üstünde, sağ çizgisi yok.
- **1b:** her şey `background` (chrome = background); şerit altı, araç çubuğu altı, kenar çubuğu sağı, durum
  çubuğu üstü 1 px `border`.
- Araç çubuğu yüksekliği 42 px (bugün `row + 2·spacing` ≈ 38 + çizgi). Sekme şeridi aynı (36).

### 1.3 Liste

- Sütun başlığı: yükseklik `row`, yazı küçük (×0.85) `foreground-muted`; sıralı sütun `foreground` + 8 px
  chevron. 1a zemin `background`, 1b zemin `surface` + alt çizgi. Bugün başlık `surface` ve normal boy yazı.
- Satır: 1a'da satırlar `spacing` kadar içeride ve `radius` yuvarlatılmış (bugün kenardan kenara, köşesiz);
  1b'de kenardan kenara, köşe 0.
- Seçili + geçerli satır: zemin `selection`, geçerli satırda **2 px içe** `focus-ring` (bugün 1 px). İkincil
  sütunlar seçili satırda `selection-foreground-muted` (bugün `selection-foreground`, yani tam renk).
- Hover: `hover` örtüsü, seçimle aynı köşe.
- Klasör boyutu hücresi durumları: `…` hesaplanıyor, `≥ 340 MB` kısmi + *italik* (Proje 8 klasör boyutu sütunu
  için; bugün sütun yok).
- Izgara hücresi: 120 px genişlik, 156 px yükseklik, küçük resim `radius-small` + 1 px `border` çerçeve; seçili
  hücre `selection` + 2 px halka.
- Kaydırma çubuğu: 6 px ince başparmak, iz yok, `scrollbar` / `scrollbar-hover`. std-widgets ListView çubuğunun
  yerine.
- Odaksız liste için `selection-inactive` tanımlı ama maket bu durumu göstermiyor.

### 1.4 Kenar çubuğu

- Bölüm etiketi (FOLDERS / PINNED / DRIVES): küçük, 600, `foreground-muted`, harf aralığı; satır yüksekliği
  `row − 2`, metin alta yaslı.
- Grup başlığı ("Work", "Media"): 8 px chevron + küçük yazı, yükseklik `row − 4`.
- Öğe satırları: **16 px çizgi simge** (home / folder / pin / alias / drive) + ad. Bugün kenar çubuğunda simge
  yok. Seçili öğe: `selection` zemin + simge `accent`. 1a'da içeride ve yuvarlatılmış, 1b'de kenardan kenara.
- Maketteki "FOLDERS" bölümü (Home, Desktop, Documents…) bugün de var (`SidebarRow.section == 0`); yapı
  değişmiyor.

### 1.5 Sekmeler, araç çubuğu, adres çubuğu

- Sekme: genişlik kuralı aynı (90–220). 1a: etkin sekme `tab-active` zemin + 1 px `border` + `radius`,
  altta kenarlardan 12 px içeride 2 px `accent` çubuğu; etkin olmayan `foreground-muted` yazı, hover `hover`.
  1b: köşesiz hücreler, aralarında 1 px çizgi, etkinin altında tam genişlik 2 px vurgu (şerit çizgisinin üstüne).
  Bugün etkin sekme yalnızca zemin farkıyla ayrılıyor; vurgu çubuğu yeni.
- Sekme simgesi: bugün `icon-folder` renkli küçük kare; tasarımda klasör yolu. Kilit: 8×8 kilit glifi
  (bugün 16'lık yol). Kapat düğmesi 20 px, `radius-small`.
- `+` düğmesi: Plus glifi `foreground-muted`.
- Geri/ileri/yukarı: bugün metin oklar ("←" "→" "↑"), tasarımda Path glifleri; devre dışı = %38 opaklık.
  Yenile glifi yeniden çizilmiş.
- Adres/kırıntı: 1a'da `input-background` + `border` kutu, yükseklik `row + 2`; son kırıntı `foreground`, diğerleri
  `foreground-muted`, ayraç 8 px chevron (bugün ayraç metin). 1b'de odaklanana kadar kutu yok, odakta
  `input-background` + `border-strong`.
- View düğmesi: bugün yalnız "dört kare" simgeli kare düğme; tasarımda **metinli** düğme: üç çizgi simge +
  "View" + chevron, 1 px `border`. Bu yeni bir düğme biçimi (metin + simge + chevron).

### 1.6 Menüler, diyaloglar/katmanlar, işlemler paneli, bırakma yığını, arama

Tasarımda yok (bir sonraki tur). Yalnızca tanımlanan kurallar: popup ve katmanlar `surface-raised` + `border` +
tek gölge (`shadow`, blur 12); örtü (scrim) `overlay`; menü satırı listeyle aynı vurgu tarifi; işlemler/bırakma
yığını satırı da aynı tarif; ilerleme durumları `progress` / `progress-paused` (= warning) / `progress-error`
(= danger); `success` yeni (tamamlanan iş); `danger-background` hata satırları için. Brief'teki bilinen kusurlar
(yığın düğmeleri boşken etkin görünüyor, çiplerde hover yok, yerli menü koyu temada açık) ele alınmamış.

### 1.7 Durum çubuğu

Yükseklik `row`, küçük yazı, 1a'da `chrome` zemin, 1b'de `background` + üst çizgi + hücre ayraçları. İçerik:
"8 items" | "1 selected · 193 B" (bu `foreground`) … sağda saat glifli "History". Bugün özet + History var; öğe
sayısının ayrı hücre olması küçük bir içerik değişikliği.

## 2. Token eşleme tablosu (1a / 1b; açık → koyu)

"Mevcut" sütunu bugünkü anahtar; "yeni" = `theme.slint`, `ThemeColors`, `COLOR_KEYS`, iki TOML ve
`theme_bridge.rs`'ye eklenecek.

| Tasarım token'ı | Gezik anahtarı | 1a açık | 1a koyu | 1b açık | 1b koyu | Kural / kullanım |
| --- | --- | --- | --- | --- | --- | --- |
| background | `background` | #ffffff | #1b1c20 | #f9f7f2 | #181715 | liste levhası, sekme-aktif |
| surface | `surface` | #f4f5f7 | #202227 | #f0ece4 | #1f1e1b | paneller, 1b başlık; **artık kenar çubuğu/şerit değil** |
| surface-raised | yeni | #ffffff | #26282e | #ffffff | #26241f | popup, menü, katman |
| chrome | yeni | #e8e9ed | #131417 | #f9f7f2 | #181715 | pencere zemini, şerit, araç çubuğu, kenar çubuğu, durum (1a) |
| foreground | `foreground` | #16171a | #ecedf0 | #1c1a16 | #efebe4 | |
| foreground-muted | `foreground-muted` | #575b65 | #9ca0aa | #5c574d | #a6a095 | |
| border | `border` | #d9dbe1 | #2e3036 | #dcd7cc | #34312c | |
| border-strong | yeni | #868b95 | #6c707a | #8c8679 | #726d63 | alan kenarı (≥3:1), pencere çerçevesi |
| accent | `accent` | #c2410c | #ff8f57 | #0b7a69 | #41d1b6 | |
| accent-foreground | `accent-foreground` | #ffffff | #24140c | #ffffff | #091d19 | türetilebilir |
| accent-hover | yeni | #ab390b | #ff9c6b | #0a6b5c | #58d7bf | mix(accent, ink, 12%) |
| accent-pressed | yeni | #933109 | #ffaa7f | #085d50 | #6fdcc8 | mix(accent, ink, 24%) |
| selection | `selection` | #f6e3db | #5f3f31 | #d5e4dd | #244f45 | mix(bg, accent, 15/30%) |
| selection-foreground | `selection-foreground` | #16171a | #ecedf0 | #1c1a16 | #efebe4 | = foreground |
| selection-foreground-muted | yeni | #2d2f34 | #d0d2d8 | #322f29 | #d5d1c8 | seçili satırda ikincil sütunlar |
| selection-inactive | yeni | #ececed | #323337 | #e7e5e0 | #302e2c | odaksız listede seçim |
| hover | `hover` | #0000000a | #ffffff0f | aynı | aynı | |
| pressed | yeni | #00000014 | #ffffff1c | aynı | aynı | basılı düğme/satır |
| focus-ring | `focus-ring` | = accent | = accent | = accent | = accent | |
| marquee | `marquee` | #c2410c24 | #ff8f5733 | #0b7a6924 | #41d1b633 | accent @ 14/20% |
| drop-target | `drop-target` | #c2410c33 | #ff8f5747 | #0b7a6933 | #41d1b647 | accent @ 20/28% |
| tab-active | yeni | #ffffff | #1b1c20 | #f9f7f2 | #181715 | = background |
| tab-inactive | yeni | #e8e9ed | #131417 | #f9f7f2 | #181715 | = chrome (maket kullanmıyor; şeffaf) |
| input-background | yeni | #ffffff | #141518 | #ffffff | #121110 | adres alanı, metin alanları |
| danger | `danger` | #b42318 | #ff7b6b | aynı | aynı | |
| danger-background | yeni | #f8ebea | #3f2b2c | #f3e4de | #3d2723 | mix(bg, danger, 9/16%) |
| success | yeni | #1a7f37 | #5ad27e | aynı | aynı | tamamlanan iş |
| warning | yeni | #9a5b00 | #f0b44c | aynı | aynı | |
| progress | `progress` | = accent | | | | |
| progress-paused | `progress-paused` | #9a5b00 | #f0b44c | aynı | aynı | = warning |
| progress-error | `progress-error` | #b42318 | #ff7b6b | aynı | aynı | = danger |
| shadow | yeni | #0000001f | #00000070 | aynı | aynı | bugün path-suggestions'ta literal `#00000040` |
| overlay | yeni | #14141859 | #0000008c | aynı | aynı | modal katman örtüsü |
| scrollbar | yeni | #16171a38 | #ecedf038 | #1c1a1638 | #efebe438 | fg @ 22% |
| scrollbar-hover | yeni | #16171a6b | #ecedf06b | #1c1a166b | #efebe46b | fg @ 42% |
| icon-folder … icon-other | `icon-*` (8) | #c98a12, #13866f, #c23a55, #7d4cc4, #9a6631, #2d6fc0, #4f8a22, #7b828c | #e9b649, #4fc7a8, #ec7088, #b994ef, #d39f69, #6aaef0, #9bd165, #a2a9b2 | aynı | aynı | iki yönde ortak |

Metrikler: `font-family ""`, `font-size 13`, `row-height 26`, `icon-size 16`, `spacing 6` aynı; `radius` 6 (1a) /
3 (1b). Türetilen boyutlar (küçük/başlık yazı, `radius-small`, şerit/araç çubuğu yüksekliği, glif çizgisi,
odak halkası) tema anahtarı **değil**; `Theme` global'inde `out`/hesaplanan özellik olarak tanımlanmalı
(ör. `out property <length> font-size-small: font-size * 0.85`).

## 3. Slint'te maliyet (yazılım çizici, `renderer-software`)

Kaba tahminler; her alt parçada `cargo build --release` ile exe boyutu ölçülmeli (bugün 22 546 432 bayt).

| Öğe | Sınıf | Tahmin | Not |
| --- | --- | --- | --- |
| 18 yeni renk özelliği (Theme global + Rust köprüsü + ayrıştırma) | Ucuz | +6–15 KB | Global özellik başına küçük kod; `set_*` çağrıları |
| Hesaplanan boyutlar (font-size-small vb.) | Bedava | ~1 KB | |
| Yeni renk değerleri, radius 3/6 | Bedava | 0 | yalnız TOML |
| 1a içerik levhası (radius + border + iç boşluk) | Ucuz | +2–5 KB | Köşelerde içeriğin kırpılması gerekir: `clip: true` + `border-radius`; yazılım çizicide yuvarlak kırpma desteği ve kaydırma hızı denenmeli. Alternatif: başlık ve satırlar zaten içeride olduğundan kırpmadan yalnız kenarlık |
| 1a satır iç boşluğu + köşe | Ucuz | +1–3 KB | Satır x/genişlik, `name-end`, yeniden adlandırma alanı x'i, marquee hesapları kayar; dikkatli test |
| Etkin sekme vurgu çubuğu | Ucuz | +1–2 KB | sekme şablonuna bir Rectangle |
| 1b sekme ayraç çizgileri, durum çubuğu ayraçları | Ucuz | +1–3 KB | |
| Odak halkası 2 px | Bedava | 0 | |
| `selection-foreground-muted` kullanımı | Bedava | 0 | satırdaki `muted-color` ifadesi |
| Kenar çubuğu simgeleri (tek Path, komut türüne göre) | Ucuz | +3–8 KB | `SidebarRow`'a `icon: int` alanı, Rust `sidebar.rs` doldurur; satır şablonuna bir Path |
| Araç çubuğu glifleri (metin oklar yerine) | Ucuz | +1–2 KB | `ToolButton.icon` zaten var |
| Metinli View düğmesi | Ucuz | +3–6 KB | Yeni küçük bileşen ya da `ToolButton`'a metin modu; Arama çubuğu (Proje 8) aynı düğmeyi kullanabilir |
| Kırıntı ayraçları chevron Path | Ucuz | +1 KB | |
| Yeni klasör/sayfa yedek simgesi | Ucuz | +0.5–2 KB | İkinci yol (köşe üçgeni) satır başına bir öğe daha; tek yola indirgenebilir |
| İnce kaydırma çubuğu (std ListView yerine kendi Flickable + başparmak) | Ucuz–Orta | +10–20 KB, std ScrollView stil kodu tamamen çıkarsa net azalma olabilir | file-view, conflict-list, convert (ListView), popup-menu (ScrollView); ops-panel zaten kendi çubuğunu çiziyor. Tekerlek, sürükleme, sayfa tıklaması yeniden yazılmalı. **En riskli kalem** |
| Popup/katman gölgesi (`drop-shadow-*`) | Ucuz | +1–3 KB her biri | Yazılım çizicide zaten path-suggestions'ta çalışıyor. Yalnız popup ve katmanlarda; satırda asla |
| Animasyon (90/140 ms renk/opaklık) | Ucuz ama dikkat | +2–5 KB şablon başına | Liste satır şablonunda `animate background` her hover'da kare üretir; yalnız sekme/popup/düğmede önerilir. "Reduce motion" için süre `Theme.motion-*` özelliğine bağlanmalı (animasyon süresinin bağlanabilirliği 1.18'de doğrulanmalı) |
| Özel font | — | 300 KB–2 MB | Tasarım önermiyor; gerek yok |
| Gradyan/bulanıklık/iç gölge | — | — | Tasarım kullanmıyor (ızgaradaki gradyanlar sahte küçük resim) |
| İmkânsız olan | — | — | Yok. Tek CSS'e özgü şey `outline-offset` ile içe halka: Slint'te üstte ayrı bir kenarlıklı Rectangle (bugünkü yöntem) |

Toplam kaba tahmin: tokenlar + liste/kenar çubuğu/sekme/araç çubuğu ≈ **+25–50 KB**; kaydırma çubuğu ayrıca
±20 KB. Brief'in "100 KB'lık görsel özellik gerekçe ister" çizgisinin altında.

## 4. Kullanıcı tema dosyalarıyla uyumluluk

Bugünkü durum (`theme.rs`): bir kullanıcı teması `base` zincirinden eksik anahtarları devralır; zincir her zaman
bir yerleşik temada biter ve yerleşikler **her anahtarı açıkça** tanımlar (`builtin_themes_define_every_value`
testi). Bilinmeyen anahtarlar sessizce yok sayılır.

Ne bozulur:

1. **`surface`'in anlamı daralıyor.** 1a'da kenar çubuğu, sekme şeridi, araç çubuğu ve durum çubuğu `chrome`'a
   geçiyor. `surface`'i değiştirmiş eski bir tema (ör. Nord, base dark) yeni sürümde kenar çubuğunu yerleşiğin
   `#131417`'si ile görür: yarı kendi, yarı Gezik renkli bir pencere.
2. **Türetilmiş anahtarlar eskiyi takip etmiyor.** Bugün bile `accent`'i değiştirip `focus-ring`/`selection`'ı
   değiştirmeyen tema yerleşiğin mavisini alıyor (zaten var olan bir kusur). Yeni tasarımda ilişki daha sıkı
   (seçim = vurgu tonu), uyumsuzluk daha görünür olur.
3. Yeni nötr anahtarlar (`chrome`, `surface-raised`, `input-background`, `border-strong`, `selection-inactive`,
   `scrollbar`) yerleşikten gelirse, kendi `background`/`foreground`'unu değiştirmiş temada sırıtır.
4. 1b seçilirse `radius` 6 → 3: `radius` yazmamış temalar köşelerin değiştiğini görür (zararsız).
5. Kaldırılan ya da yeniden adlandırılan anahtar yok; ayrıştırma bozulmaz, uyarı çıkmaz.

Önerilen geçiş (tek kural): **"türetilmiş anahtar girdilerini izler."** Her yeni ve her türetilebilir anahtar için
bir kural tanımlanır (yukarıdaki RULES + nötrler için ek kurallar, ör. `chrome` için 1a: `mix(background,
foreground, %6–9)`, 1b: `= background`; `surface-raised = background`'a göre; `input-background`; `border-strong =
mix(border, foreground, %40)`). Çözümlemede bir anahtar:
- zincirdeki **kullanıcı** dosyalarından birinde açıkça yazılmışsa o değer;
- değilse, kuralının girdilerinden herhangi biri kullanıcı tarafından değiştirilmişse kuraldan hesaplanır;
- değilse yerleşik değer (tasarımcının elle ayarladığı değer korunur).
Böylece hiçbir eski tema dosyasına dokunmak gerekmez; yalnız `accent` yazan tema seçim/odak/ilerleme/marquee/
drop-target'ı otomatik doğru alır (gelecekteki vurgu seçici de tam bunu kullanır). Uygulama yeri `theme.rs`
`resolve_theme` sonrası bir `derive` adımı (+ testler: "yalnız accent", "yalnız surface", "açık değer kazanır").
Ek olarak: `name = "Light"/"Dark"` yerleşik adları korunmalı (`settings.toml`'daki `theme = "dark"` çalışmaya
devam eder); eski mavi görünümü isteyenler için bugünkü TOML'lar `classic-light/dark` olarak saklanabilir (soru 3).

## 5. Önerilen bölme

Not: 8a `gezik-8`'de bitiyor ve arama UI'ı aynı dosyalara (`app.slint`, `theme.slint`, filtre çubuğu) dokunacak.
Tasarım işinin 8a birleştikten sonra başlaması çakışmayı azaltır.

- **9a: Token'lar + çözümleme.** Yeni 18 anahtar (Theme global, `ThemeColors`, `COLOR_KEYS`, köprü), türetme
  adımı ve testleri, yeni yerleşik TOML değerleri (seçilen yön), hesaplanan boyut özellikleri, literal
  `#00000040` gölgenin `shadow`'a bağlanması. Görünür sonuç: yeni renkler, tüm uygulama tutarlı. Düşük risk.
- **9b: Ana pencere iskeleti.** Pencere/levha düzeni (1a sheet ya da 1b çizgiler), sekme vurgu çubuğu ve sekme
  biçimi, araç çubuğu glifleri + metinli View düğmesi, adres kutusu (`input-background`, chevron ayraçlar),
  durum çubuğu.
- **9c: Liste + kenar çubuğu.** Satır iç boşluk/köşe (1a), 2 px odak halkası, `selection-foreground-muted`,
  `selection-inactive`, küçük sütun başlığı, ızgara hücresi/küçük resim çerçevesi, yeni yedek simgeler, kenar
  çubuğu etiket/grup başlığı/simgeli satırlar.
- **9d: İnce kaydırma çubuğu** (std ListView/ScrollView çıkarma). Ayrı, çünkü davranış riski ve boyut ölçümü var;
  istenirse ertelenebilir.
- **Tasarım 2. tur bekleniyor:** menüler, katman çerçevesi + çakışma/Compress/Convert/toplu ad, işlemler paneli +
  bırakma yığını, araç ipucu, önizleme, arama çubuğu + Filters + komut paleti. Bunlar 2. tur gelince 9e/9f olur;
  o zamana kadar yalnız 9a'nın token'ları (`surface-raised`, `overlay`, `shadow`, `pressed`, `success`)
  uygulanabilir.

## 6. Spec öncesi açık sorular

1. **Hangi yön?** (a) 1a Graphite: levha + ton, yuvarlak satırlar, daha "modern"; (b) 1b Ledger: tek düzlem +
   çizgiler, köşe 3, en yoğun; (c) 1a düzeni + 1b nötr/vurgu karışımı. **Öneri: 1a.** Brief'teki "ilk taslak gibi
   görünüyor" sorununu en çok o çözüyor, maliyeti 1b ile neredeyse aynı (levha Rectangle'ı), Windows 11'in yuvarlak
   seçimiyle de uyumlu. 1b'yi ikinci yerleşik tema olarak sunmak düşünülebilir ama düzen farkı (çizgi vs levha)
   token'la ifade edilemez; o yüzden tek yön.
2. **Vurgu rengi:** (a) tasarımın Amber'ı (1a) ya da Teal'i (1b); (b) bugünkü mavi `#005fb8` / `#60cdff` yeni
   nötrlerle; (c) magenta. **Öneri: (a) Amber**, ama mavi de kontrastı geçer; bu tamamen zevk. Vurgu seçici
   ayrı iş.
3. **Eski görünüm yerleşik olarak kalsın mı?** (a) Hayır, `light`/`dark` yeni görünüm olur; (b) `classic-light`/
   `classic-dark` olarak gömülür (her TOML ~1 KB). **Öneri: (b)**, ucuz ve geri dönüş yolu.
4. **Uyumluluk kuralı:** (a) "türetilmiş anahtar girdilerini izler" (bölüm 4); (b) yeni anahtarlar her zaman
   yerleşikten gelir, kullanıcılar dosyalarını güncellesin; (c) yalnız yeni anahtarlar türetilir, eski
   türetilebilir anahtarlar (`focus-ring`, `selection` …) açık kalır. **Öneri: (a).**
5. **Tasarımın ikinci turu beklensin mi?** (a) 9a–9c'yi şimdi yap, menü/katman/panel/arama için 2. turu bekle;
   (b) hepsini 2. turdan sonra tek seferde; (c) eksik yüzeyleri bu turun kurallarıyla kendimiz türet. **Öneri:
   (a)**, ve 2. tur için Claude Design'a arama çubuğu + komut paletine öncelik ver (Proje 8 bunları yapacak).
6. **Kenar çubuğu simgeleri:** (a) beş çizgi simge (home/folder/pin/alias/drive), seçili öğede `accent`;
   (b) simge yok, yalnız renk/boşluk değişsin. **Öneri: (a)**, satır başına tek Path, +3–8 KB.
7. **İnce kaydırma çubuğu:** (a) 9d olarak yap; (b) ertele, std çubuk kalsın. **Öneri: (a)**, ama boyut ölçümü
   kötü çıkarsa (net artış > 20 KB) geri çek. Koyu temada sistem renkli çubuk bugün görünür bir kusur.
8. **Hareket (animasyon):** (a) hiç yok; (b) yalnız popup opaklığı + sekme/düğme hover geçişi, "reduce motion"
   ayarıyla; (c) tasarımdaki gibi satır seçim geçişleri dahil. **Öneri: (b).** Liste satırında animasyon yok.
9. **Compact yoğunluk yuvarlama:** (a) bugünkü `round` (21/5) kalsın; (b) tasarımın `floor`'u (20/4). **Öneri:
   (a)**, değişiklik gereksiz.
10. **Metinli View düğmesi:** (a) "View ▾" metinli düğme (tasarım); (b) bugünkü simge düğmesi, yalnız glif
    değişsin. **Öneri: (a)**; aynı bileşen arama çubuğundaki "in Work ▾" ve "Filters ▾" düğmeleri için de
    kullanılır, yani Proje 8'e yatırım.
