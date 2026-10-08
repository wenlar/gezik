# Tasarım 1. tur: Graphite token'ları ve ana pencere — Tasarım

- **Tarih:** 2026-10-09
- **Durum:** Tasarım onaylandı (kullanıcı kararları 2026-10-09); ayrıntı kararları onay bekliyor
- **Kapsam:** Görsel yenilemenin ilk turu: renk/ölçü token'ları, tema çözümlemesi ve ana pencere (sekme şeridi, araç çubuğu, adres çubuğu, kenar çubuğu, liste/ızgara, durum çubuğu). Tek spec, dört plan ve dört PR: **D1** token'lar + tema çözümlemesi, **D2** pencere iskeleti, **D3** liste ve kenar çubuğu, **D4** ince kaydırma çubuğu.
- **Dayandığı:** `2026-10-03-ayarlar-ve-tema-design.md` (tema dosyası, `base` zinciri, canlı yenileme, yoğunluk), `2026-10-04-gorunum-design.md` (liste, ızgara, sütunlar), `2026-10-04-gezinme-design.md` (sekmeler, kenar çubuğu, kırıntı), `2026-10-08-gunluk-kolayliklar-design.md` (sabitlenen grupları, bırakma yığını, işlem günlüğü), `2026-10-09-arama-design.md` (8a: arama çubuğu, sonuç sütunları; bu işten önce birleşir)
- **Tasarım kaynağı:** Claude Design "Turn 1" (2026-10-08). Kopyalar `docs/superpowers/design/round-1/`: `Gezik Window.dc.html` (ana pencere maketi; `dir`/`theme`/`accent`/`density`/`view` parametreli), `Gezik Design Pass.dc.html` (iki yön, token tabloları, kontrast, metrikler, glif sayfası, bileşen notları), `gezik-tokens.js` (bütün renkleri üreten türetme kodu; bu spec'in kurallarının başvuru uygulaması), `implementation-notes.md` (Türkçe çözümleme: eşleme tablosu, maliyetler, uyumluluk, bölme). HTML dosyaları genel `support.js` çalışma zamanı olmadan açılmaz (tasarım içermediği için alınmadı); okunacak kısım işaretlemedeki stiller ve `renderVals()` kodudur. Tasarımcıya verilen brief: `D:\Work\gezik-tools\design-package\design-brief.md`, "önce" ekran görüntüleri aynı klasörün `screenshots\` altında (depoda değil).

## 1. Amaç

Gezik'in "ilk taslak gibi" duran görünümünü, exe'yi ve hızı bozmadan, tutarlı bir görsel dile taşımak. Tasarımın iki yönünden **1a Graphite** seçildi: soğuk gri bir "chrome" katmanı (sekme şeridi, araç çubuğu, kenar çubuğu, durum çubuğu) içinde kenarlıklı, köşeleri yuvarlatılmış tek bir içerik levhası (sheet); satırlar levhanın kenarından içeride ve yuvarlatılmış. Varsayılan vurgu rengi **Amber**. Bütün vurgu kullanan renkler (seçim, odak, ilerleme, marquee, bırakma hedefi) tek `accent`'ten türetilir; böylece ileride bir vurgu seçici tek değer yazar.

### Başarı ölçütleri

- Varsayılan `light` ve `dark` temaları Graphite + Amber'dır; ekran görüntüleri `Gezik Window.dc.html` maketiyle (`dir=a`, light/dark, list/grid, comfortable/compact) yan yana konunca aynı yapıyı, renkleri ve ölçüleri gösterir (§10.4).
- Eski görünüm `theme = "classic-light"` / `"classic-dark"` ile seçilebilir.
- Bugün yazılmış hiçbir kullanıcı tema dosyası değiştirilmeden çalışır, uyarı vermez; yazmadığı türetilmiş anahtarlar yazdığı anahtarlardan kuralla hesaplanır (yalnız `accent` yazan tema, seçimi/odağı/ilerlemeyi o vurguyla alır).
- Metin kontrastı her yerleşik temada ve Amber/Teal/magenta/klasik mavi vurgularda ≥ 4,5:1, metin dışı öğeler (odak halkası, alan kenarı) ≥ 3:1; odak her odaklanabilir denetimde 2 px görünür.
- 100.000 dosyalık klasörde kaydırma ve boşta bellek `master`'dakiyle aynı; boşta CPU 0 (animasyonlar biter, sürekli çalışan yok).
- Exe büyümesi D1, D2 ve D3'te her biri ≤ +0,1 MiB; D4'te net ≤ +20 KB, aşarsa D4 yapılmaz (§11).
- Arama çubuğu ve sonuç sütunları (8a) yeni token'ları kendiliğinden alır; ayrı bir iş gerektirmez.

### Kapsam dışı (bilerek)

- **Tasarım 2. tur yüzeyleri:** popup menü ve alt menüler, katman çerçevesi ve diyaloglar (çakışma, Compress, Convert, toplu ad), işlemler paneli, bırakma yığını, araç ipucu, önizleme panelinin içi, arama çubuğu + Filters paneli, komut paleti, durum sayfaları (hover/focus/disabled/drop). 2. turda öncelik **arama çubuğu ve komut paleti**. Bu turda bu yüzeyler yalnız token'ları alır (§9).
- 1b Ledger yönü (ikinci yerleşik tema olarak bile değil: düzen farkı token'la ifade edilemez).
- Vurgu seçici (arayüz); bu tur yalnız onun dayanacağı türetme kurallarını kurar.
- Gömülü font, ikon fontu, gradyan, bulanıklık, satır gölgesi.
- OS başlık çubuğunun çizimi (maketteki 30 px başlık çubuğu yalnız çerçevedir; yerli kalır). Windows'un yerli sağ tık ve View menüleri (koyu temada açık kalmaları bilinen eksik, 2. tur konusu).
- Klasör boyutu hücresinin durumları (`…`, `≥ 340 MB` italik): klasör boyutu sütunu gelince.
- "Reduce motion"un OS ayarını izlemesi (bkz. Karar 8).

## 2. Alınan kararlar (kullanıcı, 2026-10-09)

| # | Konu | Karar |
|---|---|---|
| 1 | Yön | **1a Graphite**: gri chrome katmanı, tek kenarlıklı yuvarlatılmış içerik levhası; satırlar içeride ve yuvarlatılmış |
| 2 | Varsayılan vurgu | **Amber** (`#c2410c` açık / `#ff8f57` koyu) |
| 3 | Eski görünüm ve eski tema dosyaları | `classic-light` / `classic-dark` yerleşik tema olarak kalır; eski kullanıcı dosyaları çalışmaya devam eder: kullanıcının yazmadığı türetilmiş anahtar, yazdığı anahtarlardan kendi kuralıyla hesaplanır; TOML göçü yok |
| 4 | Zamanlama | Şimdi başlanır; tasarımcının 2. turu menüler, katmanlar/diyaloglar, işlemler paneli, bırakma yığını, arama çubuğu ve komut paletini kapsar, öncelik arama çubuğu + palet |
| 5 | Kenar çubuğu simgeleri | Evet |
| 6 | İnce kaydırma çubuğu (6 px) | Evet; net exe maliyeti 20 KB'ı aşarsa yapılmaz |
| 7 | Animasyon | Yalnız popup ve hover geçişleri, "reduce motion" ayarıyla kapatılabilir; liste satırlarında yok |
| 8 | View düğmesi ve yoğunluk | Araç çubuğunda metinli "View ▾" düğmesi; compact yoğunluk tasarımdaki gibi (satır 20 px, tasarımın yuvarlaması) |
| — | Bölme | D1 token'lar + çözümleme, D2 pencere iskeleti, D3 liste + kenar çubuğu, D4 ince kaydırma çubuğu; her biri ayrı plan ve PR |

## 3. Token'lar

### 3.1 Renk anahtarları

Bugünkü 25 anahtarın hepsi kalır (ad değişmez, kaldırılan yok). 18 yeni anahtar eklenir; `COLOR_KEYS` 43 olur. Değerler `gezik-tokens.js` `derive('a', …)` çıktısıdır (`implementation-notes.md` §2'deki tablo, 1a sütunları).

| Anahtar | Graphite açık | Graphite koyu | Kullanım |
|---|---|---|---|
| `background` | `#ffffff` | `#1b1c20` | içerik levhası, etkin sekme, sütun başlığı |
| `surface` | `#f4f5f7` | `#202227` | paneller (işlemler, önizleme içi, katmanlar); artık şerit/araç çubuğu/kenar çubuğu değil |
| `surface-raised` *(yeni)* | `#ffffff` | `#26282e` | popup, menü, katman (2. tur bunları biçimlendirir; D1 bağlar) |
| `chrome` *(yeni)* | `#e8e9ed` | `#131417` | pencere zemini, sekme şeridi, araç çubuğu, kenar çubuğu, durum çubuğu |
| `foreground` | `#16171a` | `#ecedf0` | |
| `foreground-muted` | `#575b65` | `#9ca0aa` | |
| `border` | `#d9dbe1` | `#2e3036` | levha kenarı, ayraçlar |
| `border-strong` *(yeni)* | `#868b95` | `#6c707a` | alan kenarı (≥ 3:1), odaklı adres alanı |
| `accent` | `#c2410c` | `#ff8f57` | |
| `accent-foreground` | `#ffffff` | `#24140c` | |
| `accent-hover` *(yeni)* | `#ab390b` | `#ff9c6b` | birincil düğme hover |
| `accent-pressed` *(yeni)* | `#933109` | `#ffaa7f` | birincil düğme basılı |
| `selection` | `#f6e3db` | `#5f3f31` | |
| `selection-foreground` | `#16171a` | `#ecedf0` | |
| `selection-foreground-muted` *(yeni)* | `#2d2f34` | `#d0d2d8` | seçili satırda ikincil sütunlar |
| `selection-inactive` *(yeni)* | `#ececed` | `#323337` | odağı olmayan listede seçim |
| `hover` | `#0000000a` | `#ffffff0f` | |
| `pressed` *(yeni)* | `#00000014` | `#ffffff1c` | basılı düğme |
| `focus-ring` | `#c2410c` | `#ff8f57` | |
| `marquee` | `#c2410c24` | `#ff8f5733` | |
| `drop-target` | `#c2410c33` | `#ff8f5747` | |
| `tab-active` *(yeni)* | `#ffffff` | `#1b1c20` | etkin sekme zemini |
| `tab-inactive` *(yeni)* | `#e8e9ed` | `#131417` | etkin olmayan sekme (bugün saydam çizilir; anahtar 2. tur ve temalar için) |
| `input-background` *(yeni)* | `#ffffff` | `#141518` | adres alanı, metin alanları |
| `danger` | `#b42318` | `#ff7b6b` | |
| `danger-background` *(yeni)* | `#f8ebea` | `#3f2b2c` | hata satırları (2. tur) |
| `success` *(yeni)* | `#1a7f37` | `#5ad27e` | tamamlanan iş (2. tur) |
| `warning` *(yeni)* | `#9a5b00` | `#f0b44c` | |
| `progress` | `#c2410c` | `#ff8f57` | |
| `progress-paused` | `#9a5b00` | `#f0b44c` | |
| `progress-error` | `#b42318` | `#ff7b6b` | |
| `shadow` *(yeni)* | `#0000001f` | `#00000070` | popup gölgesi (bugün `path-suggestions.slint`'te literal `#00000040`) |
| `overlay` *(yeni)* | `#14141859` | `#0000008c` | modal katman örtüsü (2. tur) |
| `scrollbar` *(yeni)* | `#16171a38` | `#ecedf038` | ince kaydırma çubuğu (D4) |
| `scrollbar-hover` *(yeni)* | `#16171a6b` | `#ecedf06b` | |
| `icon-folder` … `icon-other` | `#c98a12` `#13866f` `#c23a55` `#7d4cc4` `#9a6631` `#2d6fc0` `#4f8a22` `#7b828c` | `#e9b649` `#4fc7a8` `#ec7088` `#b994ef` `#d39f69` `#6aaef0` `#9bd165` `#a2a9b2` | sırasıyla folder, image, video, audio, archive, document, code, other |

### 3.2 Ölçü anahtarları

`font-family ""`, `font-size 13`, `row-height 26`, `icon-size 16`, `radius 6`, `spacing 6` aynı kalır. **Yeni anahtar `inset`** (aralık 0–16; Graphite 6, klasik 0): levhanın chrome'dan uzaklığı ve satırların levha kenarından içeriliği. `inset = 0` iken levha ve satır köşeleri 0 olur (klasik temanın düz, kenardan kenara görünümü; bkz. Karar 4).

Türetilen boyutlar tema anahtarı **değildir**; `Theme` global'inde hesaplanan `out` özelliklerdir:

| Özellik | Kural | 26/6 için |
|---|---|---|
| `font-size-small` | `round(font-size × 0.85)` | 11 |
| `font-size-heading` | `round(font-size × 1.15)`, ağırlık 600 | 15 |
| `radius-small` | `max(2, round(radius / 2))` | 3 |
| `sheet-radius` | `inset > 0 ? radius + 2 : 0` | 8 |
| `row-radius` | `inset > 0 ? radius : 0` | 6 |
| `tab-strip-height` | `row-height + 10` | 36 |
| `toolbar-height` | `row-height + 2 × spacing + 4` | 42 |
| `control-height` | `row-height + 2` (adres alanı, View ▾, etkin sekme) | 28 |
| `focus-width` | sabit 2 px, içe | 2 |
| glif çizgisi | 16'lık kutuda 1.5, 8'lik kutuda 1.2 (kenar çubuğu simgeleri 1.3) | |

**Yoğunluk:** `compact`, `row-height`, `spacing` ve `inset`'i 0,8 ile çarpıp **aşağı yuvarlar** (tasarımın kuralı; bugün `round`): 26 → 20, 6 → 4; `row-height` alt sınırı 16 korunur, `radius` değişmez. `Metrics::compact` buna göre değişir; bu, compact kullanan her temada satırı 1 px kısaltabilir (bilerek).

### 3.3 Hareket

`Theme.reduce-motion` (tema anahtarı değil, ayardan gelir; §6) ve iki süre: `motion-hover` 90 ms, `motion-popup` 140 ms; `reduce-motion` açıkken ikisi 0. Kullanım yerleri §5.6.

## 4. Tema çözümlemesi ve türetme

### 4.1 Kurallar

`dark` burada temanın koyu olup olmadığıdır: çözümlenmiş `background`'ın WCAG bağıl parlaklığı < 0,2 (Karar 2). `ink` koyuda beyaz, açıkta siyah. `mix(a, b, t)` kanal başına `a + (b − a)·t`, en yakın tamsayıya yuvarlanır (`Math.round` ile aynı); `a @ p` rengin opaklığını `p` yapar.

| Anahtar | Kural (açık / koyu) | Girdiler |
|---|---|---|
| `chrome` | `mix(background, foreground, 10%)` / `mix(background, #000, 30%)` | background, foreground |
| `surface-raised` | `background` / `mix(background, foreground, 6%)` | background, foreground |
| `input-background` | `surface-raised` / `mix(background, #000, 25%)` | surface-raised, background |
| `border-strong` | `mix(border, foreground, 40%)` | border, foreground |
| `accent-foreground` | beyaz, ya da `mix(accent, #000, 86%)`: `accent` ile hangisi daha kontrastlıysa (beyaz, `#101010`'a eşit ya da daha iyiyse) | accent |
| `accent-hover` | `mix(accent, ink, 12%)` | accent |
| `accent-pressed` | `mix(accent, ink, 24%)` | accent |
| `selection` | `mix(background, accent, 15% / 30%)` | background, accent |
| `selection-foreground` | `= foreground` | foreground |
| `selection-foreground-muted` | `mix(foreground, foreground-muted, 35%)` | foreground, foreground-muted |
| `selection-inactive` | `mix(background, foreground, 8% / 11%)` | background, foreground |
| `hover` | `ink @ 4% / 6%` | background (karanlık mı) |
| `pressed` | `ink @ 8% / 11%` | background |
| `focus-ring`, `progress` | `= accent` | accent |
| `marquee` | `accent @ 14% / 20%` | accent |
| `drop-target` | `accent @ 20% / 28%` | accent |
| `tab-active` | `= background` | background |
| `tab-inactive` | `= chrome` | chrome |
| `danger-background` | `mix(background, danger, 9% / 16%)` | background, danger |
| `progress-paused` | `= warning` | warning |
| `progress-error` | `= danger` | danger |
| `shadow` | `#000 @ 12% / 44%` | background |
| `overlay` | `#141418 @ 35%` / `#000 @ 55%` | background |
| `scrollbar` | `foreground @ 22%` | foreground |
| `scrollbar-hover` | `foreground @ 42%` | foreground |

Türetilmeyen ("temel") anahtarlar: `background`, `surface`, `foreground`, `foreground-muted`, `border`, `accent`, `danger`, `success`, `warning`, sekiz `icon-*`. Ölçülerde türetme yok (`inset` yazmayan tema zincirinden alır).

İlk altı kuralın yüzdeleri tasarımın elle ayarladığı nötrlere yakınsar ama birebir değildir; yerleşik temalar bu yüzden her değeri açıkça yazar ve kural yalnız kullanıcı bir girdiyi değiştirdiğinde devreye girer (§4.2). `RULES` kümesindeki diğer kurallar (seçim, vurgu tonları, marquee, …) tasarımın tablosunu birebir üretir; bu bir testtir (§10.1).

### 4.2 Çözümleme sırası

`resolve_theme` bugünkü gibi `base` zincirini yürür ve her anahtarı zincirde onu tanımlayan ilk temadan alır. Yeni olarak her anahtar için **kaynağı** tutulur: bir kullanıcı dosyası (`themes/*.toml`, yerleşikle aynı adlı kullanıcı dosyası dahil) mı, yerleşik mi. Ardından bir `derive` adımı türetilmiş anahtarları yukarıdaki tablonun sırasıyla (bağımlılık sırası; `input-background` `surface-raised`'den, `tab-inactive` `chrome`'dan sonra) dolaşır:

1. Anahtarı bir kullanıcı dosyası açıkça yazdıysa: o değer.
2. Değilse, kuralının girdilerinden biri **kullanıcı dokunuşlu** ise (kullanıcı dosyasından geldiyse ya da bu adımda yeniden hesaplandıysa): kuraldan hesaplanır ve kendisi de kullanıcı dokunuşlu sayılır (zincirleme: yalnız `background` yazan temada `chrome`, onunla da `tab-inactive` yeniden hesaplanır).
3. Değilse: zincirdeki yerleşik temanın değeri (tasarımcının değeri korunur).

`hover`, `pressed`, `shadow`, `overlay` için "girdi" `background`'dır: kullanıcı açık bir temadan koyu bir tema türetirse bunlar koyu tarafa geçer. Eski adlar (`folder-icon`, `file-icon`) bugünkü gibi okunur. Kural yalnız `gezik-config`'tedir (Slint'i bilmez); `theme_bridge.rs` yalnız 43 rengi ve yeni ölçüleri kopyalar.

**Davranış değişikliği (bilerek):** bugün `accent`'i değiştirip `selection`/`focus-ring`/`progress`'i yazmayan tema yerleşiğin mavisini alıyordu; artık kendi vurgusundan türetilmiş değerleri alır. Bunları açıkça yazmış temalar etkilenmez.

### 4.3 Yerleşik temalar

| Kimlik | `name` | İçerik |
|---|---|---|
| `light` | `Light` | Graphite açık + Amber, `inset = 6` |
| `dark` | `Dark` | Graphite koyu + Amber, `inset = 6` |
| `classic-light` | `Classic Light` | bugünkü `light.toml`'un 25 değeri aynen; 18 yeni anahtar bu değerlerden §4.1 kurallarıyla hesaplanıp dosyaya yazılır, `chrome = surface` (`#f3f3f3`; eski sekme şeridi/araç çubuğu/kenar çubuğu rengi), `inset = 0` |
| `classic-dark` | `Classic Dark` | aynısı `dark.toml`'dan, `chrome = #202020` |

- Hepsi her anahtarı açıkça yazar (`builtin_themes_define_every_value` dört temayı ve 43 rengi + `inset`'i kapsar). `base`'siz bir kullanıcı teması bugünkü gibi aynı adlı yerleşiği, yoksa `dark`'ı (artık Graphite) genişletir; `base = "classic-dark"` yazılabilir.
- `theme = "light"`/`"dark"`/`"auto"` ve `theme-light`/`theme-dark` değişmez. `auto` + klasik isteyen `theme-light = "classic-light"` yazar.
- Klasik temalar eski **renkleri** ve düz düzeni (`inset = 0`: levha boşluğu yok, satırlar kenardan kenara, köşesiz) verir; yeni iskelet öğeleri (glifler, View ▾, etkin sekme çubuğu, küçük sütun başlığı, kenar çubuğu simgeleri) onlarda da görünür. Piksel piksel eski görünüm değildir (Karar 4).
- `templates/example.toml` yeni anahtarları yorum satırı olarak listeler ve "yazmazsan şundan hesaplanır" der; `inset` açıklanır.

## 5. Ana pencere (D2: iskelet)

Ölçüler Graphite (`inset 6`) içindir; `inset 0`'da levha boşlukları ve köşeleri 0 olur. Koordinatlar ve renkler `Gezik Window.dc.html` `vars` tablosundan.

### 5.1 Pencere ve levha

- Pencere zemini `chrome`. Yukarıdan aşağı: sekme şeridi, araç çubuğu, (8a arama/süzgeç çubuğu), ana satır (kenar çubuğu | levha | önizleme), işlemler paneli, bırakma yığını, durum çubuğu. Bilgi mimarisi değişmez.
- Sekme şeridi ile araç çubuğu arasında ve araç çubuğunun altında **çizgi yok**; kenar çubuğunun sağ çizgisi yok.
- **Levha:** sütun başlığı + liste/ızgara tek bir `Rectangle`: zemin `background`, 1 px `border`, köşe `sheet-radius`; kenar çubuğunun karşı yanında ve altında `inset` boşluk (kenar çubuğu sağdaysa solda ve altta, gizliyse iki yanda ve altta). Levha içeriği yuvarlak kırpılmaz; kenarlık içeriğin üstünde çizilir (Karar 7).
- **Ayırıcı (splitter):** 5 px tutma alanı kalır; çizgi yalnız hover/sürüklemede `accent` görünür, diğer zamanlarda görünmez (levhanın kenarı sınırı belirler).
- `DropGeometry` (drag.rs) levhanın iç kutusunu verir: `view-x`, `list-top`, `list-width`, `list-height` levha boşluğu ve kenarlığı kadar kayar. D2 bu alanları günceller ve sürükle-bırak isabet testlerini (liste, sekme, kırıntı, kenar çubuğu, yığın) Windows'ta yeniden doğrular.

### 5.2 Sekme şeridi

- Yükseklik `tab-strip-height` (36), zemin `chrome`, yanlarda `inset` boşluk, sekmeler arası 4 px. Genişlik kuralı aynı (90–220 px).
- Sekme yüksekliği `control-height` (28), köşe `radius`. **Etkin:** zemin `tab-active`, 1 px `border`, yazı `foreground`; altta kenarlardan 12 px içeride, alttan 3 px yukarıda, 2 px yüksekliğinde, köşesi 1 px **`accent` çubuğu** (sekme şablonuna tek `Rectangle`). **Etkin olmayan:** saydam, yazı `foreground-muted`, hover `hover`.
- Sekme simgesi: 12 px dolgulu klasör yolu (`icon-folder`) — bugünkü renkli kare yerine; 8a'nın arama sekmesi de aynı simgeyi kullanır (ayrı simge 2. tur). Kilit: 8×8 kilit glifi, `foreground-muted`. Kapat düğmesi 20 px, köşe `radius-small`, 8 px çarpı glifi `foreground-muted`; görünürlük kuralı bugünkü gibi.
- `+` düğmesi: 28 px, Plus glifi `foreground-muted`, hover `hover`. Sekmeye bırakma vurgusu (`drop-target` + 1 px `accent`) aynı kalır.

### 5.3 Araç çubuğu ve adres çubuğu

- Yükseklik `toolbar-height` (42), zemin `chrome`, yatay iç boşluk `inset + 4`, öğeler arası `spacing`.
- Geri/İleri/Yukarı/Yenile: 26 px kare `ToolButton`'lar, aralarında 2 px; metin oklar (`←` `→` `↑`) yerine glif sayfasının Back/Forward/Up/Refresh yolları (16'lık kutu, çizgi 1.5, yuvarlak uç). Devre dışı: opaklık 0,38. Erişilebilir adlar ("Back", "Forward", "Up", "Refresh") kalır.
- **Adres alanı (kırıntı):** yükseklik `control-height`, zemin `input-background`, 1 px `border`, köşe `radius`; son kırıntı `foreground`, diğerleri `foreground-muted`; kırıntı hover `hover`, köşe `radius-small`; ayraç 8 px sağ chevron (`foreground-muted`, çizgi 1.2), bugünkü metin ayraç yerine. Düzenlemede (Ctrl+L ya da tıklama) kenar 2 px `focus-ring` (Karar 9). Yol önerileri (`path-suggestions`) gölgeyi `shadow`'dan alır (D1), görünümü 2. tur.
- **View ▾:** metinli düğme: 16 px üç çizgi glifi + "View" + 8 px aşağı chevron (`foreground-muted`); yükseklik `control-height`, yatay iç boşluk 10 px, 1 px `border`, köşe `radius`, hover `hover`, basılı `pressed`. Yeni bileşen değil: `TextButton`'a 16'lık bir **ön glif** (`lead-icon`) eklenir; 8a'nın kapsam düğmesi ("in Work ▾") ve "Filters ▾" aynı bileşendir. Tıklama davranışı ve menü konumu aynı.

### 5.4 Durum çubuğu

- Yükseklik `row-height`, zemin `chrome`, üst çizgi yok, yazı `font-size-small`, `foreground-muted`, yatay iç boşluk `inset + 4`.
- İçerik bugünkü gibi tek durum metni (8a'nın arama iletileri dahil) + işlem özeti çipi + "History" + uyarılar; "History"nin önüne 12 px saat glifi gelir. Öğe sayısı/seçim ayrımı yapılmaz (Karar 10).

### 5.5 Odak halkaları

Ana penceredeki her odaklanabilir denetim (araç düğmeleri, View ▾, adres alanı, `TextButton`'lar, sekme kapat düğmesi) odakta **2 px içe `focus-ring`** kenar çizer; hover ve odak birlikteyken ikisi de görünür. Liste ve kenar çubuğu satırları D3'te (§7.2, §7.3).

### 5.6 Animasyon

- **Hover geçişi** (`motion-hover`, 90 ms, `background`): `ToolButton`, `TextButton`, sekme, sekme kapat ve `+` düğmesi, kırıntı, durum çubuğu çipleri. **Popup açılışı** (`motion-popup`, 140 ms, opaklık 0 → 1): popup menü, yol önerileri, kenar çubuğu araç ipucu.
- Liste satırlarında, ızgara hücrelerinde, kenar çubuğu satırlarında, işlemler paneli ve yığın satırlarında animasyon **yok**.
- `reduce-motion` açıkken süreler 0. Slint 1.18'de `animate … { duration: <bağlı ifade> }` çalışmazsa (D1 planının ilk adımı dener) yol: animasyonlu geçiş yalnız `!Theme.reduce-motion` koşullu bir `states` girdisinin `in`/`out` geçişine konur.
- Popup opaklık animasyonu Slint `PopupWindow` içinde temiz kurulamazsa popup kısmı 2. tura kalır; hover geçişleri kalır.

## 6. Ayar

```toml
[layout]
# left | right | hidden
sidebar = "left"
# compact | comfortable
density = "comfortable"
# true: no fades on hover and popups.
reduce-motion = false
```

`reduce-motion` canlı yenilenir (diğer `[layout]` anahtarları gibi), `Settings`'e `bool` olarak eklenir, geçersiz değer uyarı + varsayılan. `templates/settings.toml` güncellenir.

## 7. Liste ve kenar çubuğu (D3)

### 7.1 Sütun başlığı

- Yükseklik `row-height`, zemin `background` (levha rengi; bugün `surface`), alt 1 px `border`.
- Yazı `font-size-small`, `foreground-muted`, sütun adları bugünkü gibi ("Name", "Modified", …; büyük harfe çevrilmez, Karar 11). Sıralı sütun `foreground` + 8 px yukarı/aşağı chevron. Hover `hover`. 8a'nın "Folder" ve "Match" başlıkları aynı `HeaderCell`'dir.
- Başlık hücreleri satırlarla hizalı kalır: başlığın yatay iç boşluğu `inset + 10`, satır içeriğininki 10.

### 7.2 Satırlar (liste)

- Satırlar levha kenarından `inset` içeride (sol ve sağ), köşe `row-radius`; listenin üstünde ve altında 2 px boşluk. Satır şablonuna yeni öğe eklenmez: var olan arka plan `Rectangle`'ının `x`/`width`'i ve `border-radius`'u değişir.
- Zemin: bırakma hedefi `drop-target` + 1 px `accent` (aynı) › seçili ve liste odaklı `selection` › seçili ve liste odaksız **`selection-inactive`** › hover `hover` › saydam.
- Geçerli satır: **2 px içe `focus-ring`** (bugün 1 px). Geçerli satır seçim ve hover'dan ayırt edilir (Amber açık temada `focus-ring/selection` ≥ 3:1).
- Yazı: ad `selection-foreground` (seçiliyken) / `foreground`; ikincil sütunlar (Modified, Created, Type, Size, 8a'nın Folder'ı) seçiliyken **`selection-foreground-muted`** (bugün tam renk), değilken `foreground-muted`. 8a'nın Match sütunu ad gibi davranır.
- İç boşluğun kaydırdığı hesaplar: ad sütunu bitişi (`name-end`), satır içi yeniden adlandırma alanının `x`'i, marquee'nin satırla kesişimi (Rust `layout.rs`), sürüklemede satır isabeti (yalnız `y`; 6 px'lik kenar boşluğu satıra sayılır). D3 bunları birim testiyle ve Windows ekran testiyle doğrular.
- Yedek dosya simgeleri (`icons = "gezik"` ve sistem simgesi yokken): tasarımın köşeleri yuvarlatılmış klasörü ve kıvrık köşeli sayfası, **tek yol** olarak (köşe kıvrımı boşlukla çizilir; satır başına ikinci `Path` yok).

### 7.3 Izgara

Hücre ölçüleri bugünkü `grid-size` kuralıyla kalır (tasarımın 120×156'sı yalnız orta boydur). Hücre köşesi `radius`, seçili `selection` (odaksız `selection-inactive`), geçerli hücre 2 px içe `focus-ring`, hover `hover`; küçük resim köşesi `radius-small` + 1 px `border` çerçeve. Hücreler levhanın içinde, kenardan `inset + 4` boşlukla.

### 7.4 Kenar çubuğu

- Zemin `chrome`, sağ çizgi yok, satırlar `inset` içeride, köşe `row-radius`.
- **Bölüm etiketi** (FOLDERS / PINNED / DRIVES / LOCATIONS): yükseklik `row-height − 2`, metin alta yaslı (alt boşluk 3 px), `font-size-small`, 600, `foreground-muted`, literal büyük harf (bugünkü gibi, çalışma anında büyütülmez).
- **Grup başlığı** ("Work", "Media"): yükseklik `row-height − 4`, 8 px aşağı chevron + `font-size-small` `foreground-muted`; sağ tık menüsü aynı.
- **Öğe satırları:** 16 px çizgi simge (çizgi 1.3) + ad, aralarında 8 px. `SidebarRow`'a `icon: int` alanı eklenir, `sidebar.rs` doldurur: 1 home (FOLDERS'taki ev klasörü), 2 folder (diğer bilinen klasörler), 3 pin (takma adsız sabitleme), 4 alias (takma adlı sabitleme), 5 drive (sürücüler, "This PC", ağ ve çıkarılabilir dahil); satır şablonunda simge için **tek `Path`**, `commands` bu sayıya göre seçilir. Simge rengi `foreground-muted`, etkin öğede `accent`.
- Etkin öğe `selection` (+ yazı `selection-foreground`), hover `hover`, bırakma hedefi ve sabitleme çizgisi aynı. Klavye odağı varsa 2 px içe `focus-ring`.
- Glif yolları `Gezik Design Pass.dc.html` glif sayfasından (Pin, Drive) ve `Gezik Window.dc.html` kenar çubuğu verisinden (home, folder, alias); her biri yazılım çizicide bir kez görsel olarak denetlenir (yay `a` komutları, `h.01` nokta hilesi).

## 8. İnce kaydırma çubuğu (D4)

- 6 px başparmak, iz yok, köşe 3 px, kenardan 3 px içeride; renk `scrollbar`, hover/sürükleme `scrollbar-hover`; tutma alanı 10 px. En kısa başparmak 24 px. İçerik sığıyorsa çizilmez. Otomatik gizlenmez.
- Davranış: tekerlek (bugünkü adımlar), başparmağı sürükleme, boş alana tıklayınca bir sayfa. Klavye kaydırması listenin kendi tuşlarıyla (değişmez).
- Tek küçük bileşen (`widgets/scrollbar.slint`), bütün kaydırılan yerlerde aynı: dosya listesi/ızgara, `conflict-list`, `convert`, `rename-batch` (iki liste) — std `ListView` yerine `Flickable` + bu çubuk — ve `popup-menu` (std `ScrollView` yerine). Kenar çubuğu da aynı çubuğu alır. `ops-panel` kendi çubuğunu bu bileşenle değiştirir.
- Amaç std-widgets'ın kaydırma stil kodunu exe'den tamamen çıkarmak (koyu temada sistem renkli çubuk bugün görünür bir kusur). **Kural:** D4'ün net exe farkı (+/−) D3 sonrasına göre ölçülür; **+20 KB'ı aşarsa D4 birleştirilmez**, std çubuklar kalır ve bu spec'e not düşülür.
- 100.000 satırda kaydırma: `ListView`'ın görünen satırları yeniden kullanma davranışı korunmalıdır; `Flickable` + `for` buna eşdeğer değilse dosya listesi `ListView` olarak kalır ve yalnız çubuğu gizlenip üstüne kendi çubuğumuz konur (std kod çıkmaz; 20 KB kuralı yine geçerli).

## 9. Arama (8a) ve 2. tur yüzeyleri

8a (`FilterBar` arama kipi, kapsam düğmesi, "Text in files" alanı, arama sekmeleri, sonuç listesinin Folder/Match sütunları, düz görünüm) bu işten önce `master`'a girer. Bu tur onlara özel tasarım yapmaz; yalnız:

- Süzgeç/arama çubuğu chrome üzerinde, araç çubuğunun altında, levhanın üstünde durur; altındaki ayraç çizgisi kalkar (Karar 12). Alanları `input-background` + `border`, düğmeleri D2'nin `TextButton`'ı (View ▾ ile aynı), placeholder `foreground-muted`, hata `danger`.
- Sonuç sütunları §7.1–7.2'nin kurallarını kendiliğinden alır (Folder ikincil, Match ad gibi).
- İşlemler paneli ve bırakma yığını levhanın dışında, levha ile durum çubuğu arasında chrome üzerinde kalır; aralarındaki 1 px çizgiler kalkar, içleri değişmez (`surface` zemin). Önizleme paneli ana satırda levhanın sağında kendi levhası olarak çizilir (aynı levha `Rectangle`'ı, arada `inset` boşluk; Karar 12). Popup'lar ve katmanlar D1'den itibaren `surface-raised` + `border` + `shadow` alır, örtü `overlay`; biçimleri 2. tur.
- 2. tur brief'i bu spec'in token'larını ve yukarıdaki yerleşimi verir; öncelik sırası: arama çubuğu + Filters paneli, komut paleti, popup menü, katman çerçevesi + diyaloglar, işlemler paneli + yığın, araç ipucu, önizleme. 2. turdan gelenler D5+ olarak ayrı spec'e yazılır.

## 10. Test

### 10.1 Saf birim testleri (`gezik-config`, D1)

- **Renk yardımcıları:** `mix`, `alpha`, bağıl parlaklık, kontrast oranı, alfa düzleştirme; `gezik-tokens.js` ile aynı yuvarlama.
- **Kurallar tasarımı üretir:** Graphite açık ve koyu temel değerlerine uygulanan `selection`, `selection-foreground-muted`, `selection-inactive`, `accent-hover`, `accent-pressed`, `accent-foreground`, `marquee`, `drop-target`, `danger-background`, `scrollbar`, `scrollbar-hover`, `focus-ring`, `progress`, `progress-paused`, `progress-error`, `tab-active`, `tab-inactive` sonuçları §3.1 tablosuyla birebir aynı.
- **Çözümleme:** yerleşik `light`/`dark`/`classic-*` TOML'daki değerleri aynen verir (türetme yerleşik değerlere dokunmaz); "yalnız `accent`" teması seçim/odak/ilerleme/marquee/bırakma/vurgu tonlarını kuraldan alır, nötrler yerleşikten; "yalnız `background`" teması `chrome`, `tab-active`, `tab-inactive`, `surface-raised`, `input-background`, `selection`, `selection-inactive`, `danger-background`, `hover`, `pressed`, `shadow`, `overlay`'i yeniden hesaplar (zincirleme dahil); açık yazılmış değer her zaman kazanır; çok düzeyli zincirde (kullanıcı → kullanıcı → yerleşik) ara dosyanın yazdığı da "kullanıcı"dır; açık temadan `base = "light"` ile koyu tema türetilince `hover`/`pressed` koyu tarafa geçer; karanlık eşiği (0,2) iki yanından birer örnek.
- **Kontrast:** dört yerleşik tema ve Graphite üzerinde yalnız `accent` yazan Amber, Teal (`#0b7a69`/`#41d1b6`), magenta (`#b0158f`/`#f27ad6`) ve klasik mavi (`#005fb8`/`#60cdff`) temaları için: metin ≥ 4,5 (`foreground`/`background`, `foreground-muted` / `background`, `surface`, `chrome`, `input-background`; `selection-foreground`/`selection`; `selection-foreground-muted`/`selection`; `foreground`/`selection-inactive`; `accent-foreground`/`accent`; `danger`/`background`), metin dışı ≥ 3 (`focus-ring` / `selection`, `background`, `chrome`; `border-strong`/`input-background`). Klasik temalarda tutmayan bir çift olursa yalnız onların **yeni** anahtarları ayarlanır, eski 25 değer değişmez.
- **Ölçüler:** `inset` okunur, aralık dışı uyarı; compact `floor` (26 → 20, 6 → 4, `inset` 6 → 4, 16 alt sınırı, `radius` değişmez).
- **Ayar:** `reduce-motion` okunur, geçersiz değer uyarı + `false`.

### 10.2 Tema dosyası uyumluluk testleri (D1)

Fikstür olarak `gezik-config/tests/themes/` altında, bugünkü biçimde yazılmış dosyalar: tema spec'i §4.1'deki Nord örneği (`folder-icon`/`file-icon` eski adlarıyla), bugünkü `templates/example.toml`, yalnız `accent` yazan, `base = "light"` olup yalnız `background`/`foreground` yazan, bugünkü `light.toml` ve `dark.toml`'un tam kopyası (kullanıcı dosyası olarak; 18 yeni anahtar kurallarla hesaplanır, eski 25 değer aynen kalır). Her biri **uyarısız** çözülür, her anahtar dolu ve beklenen değerde; bilinmeyen anahtar hâlâ sessiz.

### 10.3 Windows ekran testi (her planın sonunda kontrol listesi)

GUI otomasyonu yalnız kullanıcı bilgisayar başında değilken; klavye düzenine dokunulmaz (yalnız Türkçe Q).

- **D1:** dört yerleşik tema ve `auto` geçişi; tema dosyası kaydedilince canlı yenileme; yalnız `accent = "#b0158f"` yazan kullanıcı teması seçimi/odağı magenta yapar; Nord fikstürü; `reduce-motion` değişince canlı uygulanır; bozuk tema dosyası uygulamayı açık tutar.
- **D2:** sekme şeridi (etkin çubuk, kilit, kapat, `+`, sekmeye bırakma), araç çubuğu glifleri ve devre dışı hâlleri, View ▾ (fare ve klavye), kırıntı tıklama/düzenleme/odak halkası/kırıntıya bırakma, yol önerileri gölgesi, durum çubuğu (özet çipi, History, uyarı, arama iletisi), kenar çubuğu sol/sağ/gizli iken levha boşlukları, ayırıcı sürükleme, hover ve popup geçişleri açık/kapalı, Tab ile odak dolaşımı, %100 ve %200 ölçekte 1 px çizgiler.
- **D3:** liste ve ızgarada seçim/odaksız seçim/geçerli satır/hover/bırakma hedefi; satır içi yeniden adlandırma alanının yeri; marquee (kenar boşluğundan başlayan dahil); başlık hizası ve sıralı sütun; 8a sonuç listesi (Folder/Match, seçiliyken soluk); kenar çubuğu simgeleri (home, folder, pin, alias, drive, This PC), bölüm etiketleri, grup başlıkları, sabitleme sürükleme çizgisi; compact yoğunluk; klasik temalarda düz düzen.
- **D4:** tekerlek, başparmak sürükleme, sayfa tıklaması; 100.000 dosyalık klasörde uzun sürükleme; popup menüde (uzun alt menü) ve diyalog listelerinde çubuk; koyu temada sistem renkli çubuk kalmadı.

### 10.4 Önce/sonra ekran görüntüleri

- **Ortam:** brief'in görüntüleriyle aynı: Windows 11, %100 ölçek, pencere 1280×800, geçici yapılandırma klasörü, `D:\Work\gezik-tools` altındaki uydurma klasör ağacı; `scripts/perf/screenshot.ps1` ile.
- **"Önce":** her planın başında `master`'ın sürüm derlemesinden (8a dahil) brief'in kümesi yeniden çekilir: 01 liste, 02 ızgara, 03 sekmeler, 04 kenar çubuğu grupları, 05 adres tamamlama, 06 süzgeç çubuğu, 16 şerit ve durum çubuğu, 17 This PC; ayrıca arama sonucu listesi; açık ve koyu.
- **"Sonra":** aynı küme aynı adımlarla PR dalından; ek olarak compact (koyu, liste), klasik açık/koyu ve magenta vurgulu kullanıcı teması (açık, liste).
- **Karşılaştırma:** `D:\Work\gezik-tools\design-shots\<parça>\{before,after}\` (depoya girmez). Her çift için üç sütunlu bir karşılaştırma sayfası: önce | sonra | maket (`Gezik Window.dc.html`, `dir=a`, aynı tema/görünüm/yoğunluk, 1280×800; maketteki 30 px başlık çubuğu ve numara rozetleri yok sayılır). PR açıklaması farkları sayar: maketten bilinçli sapmalar (bu spec'teki kararlar) ve kalan kusurlar. Klasik temalarda "önce" ile "sonra" arasındaki farklar yalnız §4.3'te sayılan yeni iskelet öğeleri olmalı.

### 10.5 Linux ve macOS (kullanıcının kendi makinelerinde, Docker yok)

D4'ten (ya da D4 yapılmazsa D3'ten) sonra bir kontrol listesi: `docs/superpowers/notes/linux-test.md` ve `macos-test.md`'ye yeni bir bölüm, sonuçlar `macos-test-results.md`'ye (macOS) ve Linux notuna. Maddeler: dört yerleşik tema ve `auto` (GNOME/KDE ve macOS açık/koyu değişimi), %200 ölçekte (Retina, Linux HiDPI) 1 px çizgiler ve levha köşesi, 11 px küçük yazının okunurluğu (Linux'ta fontconfig'in seçtiği font), glifler (özellikle Refresh ve History yayları, Drive'daki nokta), kenar çubuğu simgeleri, macOS'ta LOCATIONS bölümü, View ▾ ve popup menü, ince kaydırma çubuğu (trackpad ile pürüzsüz kaydırma, macOS'ta ters yön), `reduce-motion`, compact yoğunluk, klasik tema.

## 11. Performans ve boyut

- **Exe:** her parçanın taban ölçüsü, parçanın başladığı `master`'ın sürüm derlemesidir (8a birleşmiş hâli; başvuru için `d585527` 22.546.432 bayt). D1 ≤ +0,1 MiB, D2 ≤ +0,1 MiB, D3 ≤ +0,1 MiB, D4 net ≤ +20 KB (aşarsa yapılmaz); toplam ≤ +0,3 MiB. Her plan ilk ve son adımında bayt olarak ölçüp PR'a yazar. Beklenen (`implementation-notes.md` §3): token'lar +6–15 KB, iskelet + liste + kenar çubuğu +25–50 KB.
- **Kaydırma:** 100.000 dosyalık klasörde `scripts/perf/measure.ps1` ve `stress.ps1` (liste), `grid.ps1` (ızgara) sonuçları `master`'ınkinden belirgin kötü değil (ölçüm gürültüsü içinde). Satır şablonuna öğe eklenmez; satır başına gölge, gradyan, opaklık katmanı ya da kırpma yok; yalnız yuvarlatılmış dolgu.
- **Bellek:** boşta bellek `master`'a göre büyümez (`measure.ps1`, her planın sonunda).
- **CPU:** boşta 0; animasyonlar 90/140 ms sürer ve biter, sürekli çalışan animasyon yok.
- **Açılış:** türetme adımı tema başına en çok 26 renk hesabıdır, ölçülemeyecek kadar küçük; açılış süresi değişmez.

## 12. Kod yapısı

```
crates/
  gezik-config/
    src/theme.rs        COLOR_KEYS 43, ThemeColors (+18), Metrics.inset, compact floor,
                        kaynak takibi, derive adımı (§4.2)
    src/theme_rules.rs  YENİ — mix/alpha/kontrast, kural tablosu; saf, testli
    src/settings.rs     [layout] reduce-motion
    themes/light.toml, dark.toml                Graphite + Amber (tam)
    themes/classic-light.toml, classic-dark.toml YENİ — eski renkler + 18 anahtar, inset 0
    templates/example.toml, settings.toml       yeni anahtarlar ve ayar açıklaması
    tests/themes/*.toml YENİ — uyumluluk fikstürleri (§10.2)
  gezik/
    ui/theme.slint      18 renk, inset, reduce-motion, hesaplanan boyutlar ve süreler
    src/theme_bridge.rs yeni değerleri kopyalar
    ui/app.slint        D1: chrome/tab-active/input-background bağlama; D2: levha, şerit,
                        araç çubuğu, durum çubuğu, DropGeometry
    ui/widgets/tab-bar.slint, button.slint, text-button.slint, breadcrumb.slint   D2
    ui/widgets/path-suggestions.slint   D1: shadow token
    ui/widgets/file-view.slint, sidebar.slint, file-icon.slint   D3
    src/sidebar.rs      D3: SidebarRow.icon
    src/layout.rs       D3: satır iç boşluğu marquee/isabet hesaplarında
    ui/widgets/scrollbar.slint   YENİ (D4)
```

**Bölmenin içeriği:**

- **D1 — Token'lar + çözümleme:** §3, §4, §6; var olan yüzeylerin yeni token'lara bağlanması (pencere/şerit/araç çubuğu/kenar çubuğu/durum çubuğu `chrome`, etkin sekme `tab-active`, metin alanları `input-background`, gölge `shadow`); geometri değişmez. Görünür sonuç: Graphite renkleri eski iskelette; klasik temalar bugünkü görünüm.
- **D2 — Pencere iskeleti:** §5 (levha, sekme şeridi ve vurgu çubuğu, araç çubuğu glifleri, View ▾, adres alanı, durum çubuğu, odak halkaları, animasyonlar), §9'daki yerleşim (süzgeç/arama çubuğu, işlemler paneli, yığın, önizleme levhası).
- **D3 — Liste ve kenar çubuğu:** §7.
- **D4 — İnce kaydırma çubuğu:** §8, 20 KB kuralıyla.

## 13. Kararlar (onay bekleyen)

Kullanıcı kararlarının (§2) bıraktığı ayrıntılar; uygulama bu hâlleriyle başlar, itiraz edilen madde ilgili planda değişir.

1. **Türetme kuralları ve kapsamı (§4.1):** 43 renkten 26'sı türetilir; tasarımın `RULES`'ına ek olarak nötr kurallar benim: `chrome` (açık %10 ön plana, koyu %30 siyaha), `surface-raised`, `input-background`, `border-strong` (%40), `hover`/`pressed`/`shadow`/`overlay` (koyuluğa göre). `surface`, `success`, `warning` ve `icon-*` türetilmez.
2. **Açık/koyu ayrımı:** `background` bağıl parlaklığı < 0,2 ise kurallar koyu yüzdelerini kullanır.
3. **Eski türetilebilir anahtarlar da türetilir** (`selection`, `focus-ring`, `progress`, `marquee`, `drop-target`, `accent-foreground`, `selection-foreground`, `progress-paused/-error`): `accent` yazıp bunları yazmamış eski temalar artık yerleşiğin mavisini değil kendi vurgularını alır (§4.2).
4. **Klasik tema = eski renkler + düz düzen, yeni iskelet:** yeni ölçü anahtarı `inset` (Graphite 6, klasik 0) levha boşluğunu ve satır içeriliğini/köşesini sıfırlar; glifler, View ▾, sekme çubuğu, küçük başlık ve kenar çubuğu simgeleri klasikte de var. Klasik `chrome` = eski `surface`. Yerleşik adlar "Light"/"Dark" kalır, klasikler "Classic Light"/"Classic Dark".
5. **Compact:** "tasarımın yuvarlaması" aşağı yuvarlama (`floor`) olarak okundu: 20/4, `inset` 4; `radius` compact'ta değişmez.
6. **Hover geçişi alan yerler:** araç düğmeleri, `TextButton`, sekme, sekme kapat ve `+`, kırıntı, durum çubuğu çipleri; popup geçişi: popup menü, yol önerileri, kenar çubuğu ipucu. Kenar çubuğu satırları "liste satırı" sayıldı, animasyonsuz. Popup geçişi `PopupWindow`'da temiz kurulamazsa 2. tura kalır.
7. **Levha kırpılmaz:** kenarlık içeriğin üstünde çizilir; marquee/bırakma vurgusu köşe yayında en çok ~2 px taşabilir. Ayırıcı çizgisi Graphite'ta yalnız hover/sürüklemede görünür.
8. **`reduce-motion`:** `[layout]` altında `bool`, varsayılan `false`; OS'un "animasyonları azalt" ayarını izlemez (sonra `"auto"` eklenebilir).
9. **Adres alanı odakta 2 px `focus-ring`** (tasarım yalnız kutu diyor); `border-strong` yalnız alan kenarı karşıtlığı için.
10. **Durum çubuğu metni bölünmez:** öğe sayısı | seçim hücreleri yok (8a'nın arama iletileri aynı metni kullanıyor); yalnız küçük yazı, chrome zemin ve History saat glifi.
11. **"Small caps" sütun başlığı** küçük boy (×0,85) soluk yazı olarak okundu, büyük harfe çevrilmez (tasarımdaki gibi "Name", "Modified"); büyük harf yalnız kenar çubuğu bölüm etiketlerinde.
12. **Arama/süzgeç çubuğu, işlemler paneli, yığın ve önizleme yerleşimi (§9):** süzgeç/arama çubuğu chrome'da levhanın üstünde, işlemler paneli ve yığın levhanın altında chrome'da (içleri değişmeden), önizleme kendi levhasında; ayraç çizgileri kalkar. Kesin biçimleri 2. tur.
13. **D4 kapsamı:** std `ListView`/`ScrollView`'ın bütün kullanımları değiştirilir ki std kaydırma kodu exe'den çıksın; 100.000 satırda `Flickable` yeterli değilse dosya listesi `ListView` kalır, üstüne kendi çubuğumuz biner; net > +20 KB ise D4 bırakılır. Başparmak en az 24 px, otomatik gizlenmez.
14. **Izgara hücre ölçüleri** bugünkü `grid-size` kuralıyla kalır; tasarımdan yalnız köşe, seçim, 2 px halka ve küçük resim çerçevesi alınır. Yedek dosya simgeleri tek yol (ikinci `Path` yok).
15. **Ekran görüntüleri** depoya girmez (`D:\Work\gezik-tools\design-shots\`); "önce" kümesi her planın başında `master`'dan yeniden çekilir.
