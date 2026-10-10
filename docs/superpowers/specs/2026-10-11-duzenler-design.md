# Alt Proje 10: Düzenler — Tasarım

- **Tarih:** 2026-10-11
- **Durum:** Kapsam kullanıcı kararı (2026-10-11: dört başlığın hepsi, çift panel varsayılan kapalı, tek tuşla açılır). Ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§17, her biri gerekçesiyle). Mimari incelemeden sonra gözden geçirildi (2026-10-11: sıra, 10a dilimleri, `PaneView` yaklaşımı, bütçeler, kurallar, 10g). Plan bu spec'ten sonra önceden onaylıdır. **10d uygulandı** (`feat/layout-10d`; Windows ekran testleri, macOS 200–207 ve Linux 150–154 bekliyor); **10c uygulandı** (`feat/layout-10c`; Windows ekran testleri, macOS 210–219 ve Linux 160–165 bekliyor); **10f uygulandı** (`feat/layout-10f`; Windows ekran testleri, macOS 220–226 ve Linux 170–173 bekliyor); **10a-1 uygulandı** (`feat/layout-10a1`; `WindowCtx` 10g'ye kaldı, plan sapma 1).
- **Kapsam:** Gezik yol haritasının 10. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md`, "Düzenler": satır 4, 20, 23, 32, 36, 41, 63). Satır 58 (yerleştirilebilir paneller) kapsam dışıdır (§1). Tek spec, on parça (10d, 10c, 10a-0…10a-3, 10b, 10e, 10f, 10g; §13).
- **Dayandığı:** `2026-10-04-gezinme-design.md` (`Location`, sekmeler, geçmiş, kenar çubuğu), `2026-10-04-gorunum-design.md` (görünüm kipleri, sütunlar, `views.toml`), `2026-10-04-dosya-islemleri-design.md` (iş motoru, çakışma listesi, geri alma), `2026-10-07-klavye-paketi-design.md` (kısayol tablosu, süzgeç), `2026-10-08-gunluk-kolayliklar-design.md` (oturum, sekme setleri, bırakma yığını), `2026-10-09-arama-design.md` (palet, `Location::Search`), `2026-10-10-sistem-butunlesmesi-design.md` (tek örnek, `new-window`, çöp, bulut, Bilgi penceresi)
- **Taban:** `master` 4645f24. Exe ≈ 24,66 MB (crt-static), boşta bellek 7,2–7,3 MB, `Action::ALL` = 85.
- **Rakip notları:** aynı klasördeki TC, DO, OC, Fi, FP, Far, FL, PF karşılaştırmaları.

## 1. Amaç

Explorer ve Finder'ın düzenlerini (liste, ızgara, Finder'ın sütun görünümü, Explorer'ın gezinme ağacı, gruplama) ve gelişmiş yöneticilerin düzenlerini (çift panel, karşı panele kopyala/taşı, eşli gezinme, çok pencere, otomatik görünüm kuralları) Gezik'e getirmek. Hafiflik bozulmaz: **her yeni düzen açılana kadar bellekte yer tutmaz, okumaz, izlemez.** Tek bölmeli, ağacı kapalı, gruplamasız, tek pencereli bir Gezik bugünkü Gezik kadar hafiftir.

### Başarı ölçütleri

- **Çift panel (10b):** F3 (Windows/Linux) ya da ⌃⌘P (macOS) ikinci bölmeyi açıp kapatır. Açıkken Tab bölme değiştirir, F5/F6 seçimi öbür bölmenin klasörüne kopyalar/taşır (geri alınabilir, çakışma listesiyle), sekme menüsünden ve sürükleyerek sekme öbür bölmeye geçer, eşli gezinme açıkken bir bölmede alt klasöre inmek ve üst klasöre çıkmak öbürünü de götürür. Kapalıyken F5 bugünkü gibi yeniler.
- **Ağaç (10c):** kenar çubuğundaki her yer satırı açılıp kapanır. Yalnız açılan dal okunur; açık dallar izlenmez. 50.000 alt klasörlü bir dal açılınca arayüz takılmaz (okuma arka planda, satırlar pencereli modelle çizilir).
- **Gruplama (10d):** liste ve ızgarada tür, tarih, boyuta göre grup başlıkları; başlık tıklanınca grup kapanır. 100.000 öğede gruplu sıralama ≤ gruplamasız sıralamanın 1,2 katı.
- **Miller sütunları (10e):** üçüncü görünüm kipi; ok tuşlarıyla sütunlar arası gezinme, son sütunda önizleme. 30 sütun derinlikte gezinirken bellek ≤ taban + gösterilen sütunların listeleri + 0,5 MB.
- **Görünüm kuralları (10f):** `settings.toml`'da `[[view-rules]]`; yol, yer türü, içerik oranına göre kip, sıralama, grup, sütunlar. Kural yokken sıfır maliyet; 20 kural bir klasör açılışına ≤ 50 µs ekler ve hiç dosya sistemi çağrısı yapmaz.
- **Çok pencere (10g):** Ctrl+N/⌘N aynı süreçte yeni pencere açar; sekme menüsünden sekme yeni ya da başka pencereye taşınır (üç sistemde). **Windows ve macOS'ta** sekme pencereden dışarı sürüklenince yeni pencere olur, başka bir Gezik penceresinin sekme şeridine bırakılınca oraya geçer; Linux'ta (X11 ve Wayland) menü yolu kullanılır. İkinci pencere ≤ çerçeve arabelleği + 2,5 MB; kapanınca çerçeve arabelleği bırakılır ve bellek taban + 0,3 MB'a döner (açık test).
- **Hafiflik (hepsi):** varsayılan ayarlarla (tek bölme, ağaç kapalı, gruplama yok, kural yok, tek pencere) boşta bellek `master`'a göre ≤ +0,1 MB (10a dilimleri 0), açılış ≤ +2 ms, yeni zamanlayıcı ve yoklama yok. Boşta en çok **bir** klasör izleyicisi (etkin bölmeninki). Her parça exe'ye ≤ +262.144 B.

### Kapsam dışı (bilerek)

- **Yerleştirilebilir paneller** (satır 58, PF modülleri), serbest bölme sayısı ve yatay/dikey serbest bölme (FP): iki bölme, yan yana.
- Üst üste (dikey) iki bölme (TC'nin yatay düzeni): yalnız yan yana.
- Etikete göre gruplama: 12. adımda (Etiketler) gelir; gruplama altyapısı ona yer bırakır (§6.1).
- Karşılaştırma ve eşitleme (klasör farkı, TC "Synchronize dirs"): 11. adım (Araçlar).
- Kayıtlı çok pencereli düzenler (DO Layouts, FL Workspaces): oturum geri yükleme ve sekme setleri (çift bölmeli, §4.9) yeter.
- Hızlı görünümün öbür bölmede açılması (TC Ctrl+Q, Far): önizleme bölmesi ve Hızlı Bakış zaten var.
- Kurallarda renk, sekme rengi, otomatik komut (TC): yalnız görünüm (§8).

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Kapsam | Dört başlığın hepsi: (1) çift panel, F5/F6, Tab, sekmeyi öbür bölmeye taşıma, eşli gezinme; (2) kenar çubuğunda ağaç, yalnız açılan dal okunur, boşta hiçbir şey; (3) gruplama (tür, tarih, boyut; etiket sonra) + Miller sütunları; (4) tek süreçte çok pencere, sekmeyi koparıp yeni pencere + otomatik görünüm kuralları | Kullanıcı kararı 1 |
| Çift panel varsayılanı | Kapalı; tek tuş (F3 ya da menü) açar/kapatır | Kullanıcı kararı 2 |
| F5/F6 | Yalnız ikinci bölme açıkken öbür bölmeye kopyala/taşı; kapalıyken F5 bugünkü anlamında (Windows/Linux'ta yenile). Çakışmalar çözülür ve belgelenir; macOS tuşları ayrıca | Kullanıcı kararı 3 |
| Soru | Kullanıcıya yeniden soru sorulmaz; ayrıntı kararları spec'te gerekçeyle | Kullanıcı kararı 4 |
| Hafiflik | Explorer/Finder'ın yerine geçmek, hafif kalmak: isteğe bağlı, boşta hiçbir şey, boşta bellek/açılış/exe bütçeleri | Ürün hedefi |

## 3. Ortak yapı: bölme ve pencere bağlamı (10a)

Bugün bir pencere vardır ve pencereye bağlı her parça bir iş parçacığı yerel tekildir (`navigation`, `view`, `filter`, `search`, `path_box`, `sidebar`, `palette`, `preview`, `stack`, `info` …): **22 tekil, 35 dosyada ~279 `with_current` çağrısı**. Pencerede tek liste vardır: `AppWindow`'un `items`, `tabs`, `crumbs`, `current-path`, `col-*`, `list-scroll` özellikleri. Çift panel ve çok pencere bu iki varsayımı kırar. 10a görünür hiçbir şeyi değiştirmeden bu varsayımları kaldırır.

**10a dört dilimdir** (her biri bir PR), çünkü tek parça hâlinde bu genişlikte bir değişikliğin hatası özellikten ayrılamaz:

| Dilim | İş | Görünür değişiklik |
|---|---|---|
| **10a-0** | Slint yoklaması (§3.2): bir `AppWindow`'da iki `PaneView` örneği, `PaneView`'un durumu nasıl dışa açtığı, iki yönlü bağların geri çağrıya çevrilmesi; ölçüm (derleme süresi, exe, kaydırma CPU'su). Çıktı: yoklama notu (`docs/superpowers/notes/`); yoklama kodu atılabilir daldadır, ürün koduna girmez | Yok |
| **10a-1** | İnce `Pane` ve `WindowCtx` yapıları bugünkü tekilleri **sarar**, tek bölmeyle. `panes::with_active` bugünkü `with_current`'ların takma adıdır (çağrı yerleri mekanik olarak yeni ada geçer); arka plan sonuçları `PaneId` taşır. Mekanik, davranış aynı | Yok; boşta bütçe **0** |
| **10a-2** | `Media` ve `ViewMemory` süreç düzeyine (bugün `View::new` başına) | Yok; boşta bütçe 0 |
| **10a-3** | `PaneView` çıkarımı: `app.slint`'in sekme şeridi, araç satırı, çubuklar ve `FileView` bölümü 10a-0'ın seçtiği yolla `widgets/pane.slint`'e taşınır | Yok; boşta bütçe 0 |

**Zamanlama:** 10a-1…10a-3, 9. adımın açık dalları birleştikten sonra başlar: `main.rs`, `actions.rs`, `context_menu.rs` ve `navigation.rs` onlarla çakışır. 10a-0 bir yoklama olduğu için beklemez. Bu yüzden 10d ve 10c (bölme gerektirmez, daha az ortak dosya) önce gelir (§13).

### 3.1 Üç düzey

| Düzey | Ne | Rust | Slint |
|---|---|---|---|
| **Bölme** (`Pane`) | Sekmeler ve geçmiş, gösterilen listeleme, seçim, süzgeç çubuğu, arama çubuğu, adres çubuğu (yazma kipi ve öneriler), sütun genişlikleri değil (§3.3), kaydırma, yeniden adlandırma alanı, klasör izleyicisi, klasör boyutları, harfle atlama | `Pane { id, nav: Navigator, view: View, filter, search, path_box }` (`gezik/src/pane.rs`, yeni) | `PaneView` bileşeni (`widgets/pane.slint`, yeni): sekme şeridi, araç satırı (geri/ileri/yukarı/yenile, adres, View düğmesi), arama ve süzgeç çubukları, `FileView` |
| **Pencere** (`WindowCtx`) | 1–2 bölme ve hangisinin etkin olduğu, kenar çubuğu, önizleme bölmesi, durum satırı, iş paneli görünümü, bırakma yığını şeridi, palet, katmanlar (toplu ad, sıkıştır, dönüştür, Bilgi, sekme seçici, sistem paneli, soru, çakışma listesi), sürükleme durumu, pencere konumu | `WindowCtx { window: AppWindow, panes: [Pane; 1..2], active: usize, … }` (`gezik/src/windows.rs`, yeni) | `AppWindow` |
| **Süreç** | Ayarlar ve tema, `views.toml` belleği, `state.toml`, iş motoru ve geri alma geçmişi, bırakma yığınının içeriği, simge/küçük resim önbelleği ve işçileri (`Media`), arama ad önbelleği, tek örnek dinleyicisi, Hızlı Bakış penceresi, yapılandırma izleyicisi | iş parçacığı yerel tekiller (bugünkü gibi) | `Theme` her pencereye ayrı uygulanır (Slint'te global'ler bileşen örneğine özeldir; bugün Hızlı Bakış penceresinde yapıldığı gibi) |

### 3.2 Erişim

- `with_current` çağrıları (10a-1) üç ada ayrılır: `panes::with_active(|pane| …)` (odaktaki pencerenin etkin bölmesi), `windows::with_focused(|w| …)` (odaktaki pencere) ve bugünkü süreç tekilleri. Arka plan sonuçları (listeleme, arama partisi, klasör boyutu) **bölme kimliği** taşır ve `panes::with_id(id, …)` ile yerine bulur; kapanmış bölmenin sonucu düşer. Bugünkü kuşak (`generation`) denetimleri aynen kalır.
- `perform` ve `handle_key` bir `WindowCtx` ve etkin `Pane` alır. Eylemler (`actions::run`) bölmenin `nav` ve `view`'unu bugünkü gibi kullanır; değişen yalnız nereden alındıklarıdır.
- **`PaneView` iç durumu olan bir Slint bileşenidir.** Kaydırma konumu, adres çubuğunun yazma kipi, ad alanının metni, sütun sürükleme ara değerleri, üzerine gelme gibi anlık durum bileşenin **içinde** kalır. Rust'a yalnız şunlar açılır:
  - birkaç `in` model özelliği: `AppWindow`'da `[PaneData]` modeli (bir ya da iki öğe; `PaneData` yapısı `items`, `tabs`, `crumbs` modellerini ve skalerleri — yol, kip, sıralama, süzgeç metni, arama durumu — taşır) ve `for pane[i] in panes: PaneView { data: pane; index: i; … }`;
  - geri çağrılar, bölme numarasıyla: `item-pressed(pane, i, ctrl, shift)`, `scrolled(pane, y)`, `rename-edited(pane, text)`, `path-editing-changed(pane, on)` … Bugünkü iki yönlü bağlar (`scroll <=>`, `rename-text <=>`, `path-editing <=>`, `col-* <=>`) geri çağrıya ve Rust'tan gelen tek yönlü bir isteğe çevrilir (`PaneData` içinde artan sayaçlı alanlar: `scroll-request`, `rename-request`; bileşen sayaç değişince uygular).
- Bölme başına ayrı kök özellik takımı (`p0-…`/`p1-…`, `build.rs` üretimi) **kullanılmaz**. Yalnız 10a-0 yoklaması iç içe modelin bir şeyi taşıyamadığını gösterirse (ör. `ListView`'un kaydırma isteğinin model değişiminden sonra uygulanması) o tek alan için kök özelliğine düşülür ve yoklama notunda yazılır.
- 10a-0 şunları ölçer ve yazar: iki `PaneView` ile derleme süresi ve exe farkı; `PaneData` modelinde bir öğenin değişmesinin öbür bölmeyi yeniden çizip çizmediği (`set_row_data` yalnız değişen bölmeyi kirletmeli); kaydırma CPU'su tek ve iki bölmede; `scroll-request` yolunun bugünkü `SCROLL_RESTORE_DELAY` gecikmeli geri yüklemeyle çalışması.

### 3.3 Bölmeler arası paylaşılanlar

- **Sütun düzeni** (`state.toml` `columns`, `result_columns`) bütün bölmelerde ortaktır: bir bölmede genişletilen sütun öbüründe de genişler. Kural (§8) bir klasör için sütunları değiştirebilir.
- **Görünüm belleği** (`views.toml`): tek `ViewMemory` (`Rc<RefCell<…>>`), her `View` onu paylaşır; yazma bugünkü gibi ertelenmiş tek yazımdır.
- **`Media`**: tek örnek, iki işçi iş parçacığı süreç başına (bugün `View::new` başına; 10a-2 bunu sürece taşır).
- **Simge, tür adı, küçük resim önbellekleri:** süreç başına (bütçeler değişmez).

### 3.4 Kabul (10a)

Her dilimde görünür değişiklik yok. Bütün birim testleri, `scripts/perf/measure.ps1`, `stress.ps1`, `tabs.ps1`, `grid.ps1` `master` ile aynı (±%3); boşta bellek **+0** (ölçüm gürültüsü ±0,02 MB içinde); her dilim exe ≤ +262.144 B. 10a-3'ün sonunda bütün Windows ekran testi listesi (`test/windows-screen-*`'daki son liste) yeniden koşar.

## 4. Çift panel (10b)

### 4.1 Düzen

```
┌ sidebar ┬──────── sol bölme ────────┬─────── sağ bölme ────────┬ önizleme ┐
│         │ [sekme][sekme][+]         │ [sekme][+]               │          │
│         │ ◀ ▶ ▲ ⟳  C:\Work  [View▾] │ ◀ ▶ ▲ ⟳  D:\Yedek [View▾]│          │
│         │ (arama / süzgeç)          │                          │          │
│         │ liste / ızgara / sütunlar │ liste / ızgara / sütunlar│          │
└─────────┴───────────────────────────┴──────────────────────────┴──────────┘
  durum satırı (etkin bölmenin) · iş paneli · bırakma yığını
```

- Her bölme tam bir gezgindir: kendi sekmeleri, geçmişi, adres çubuğu, görünüm kipi, sıralaması, süzgeci, araması (PF ve FL gibi). Kenar çubuğu, önizleme bölmesi, durum satırı, iş paneli ve bırakma yığını pencereye aittir.
- Bölmeler arasında sürüklenebilir ayraç (5 px, bugünkü ayraçlar gibi); çift tık ortalar. Oran `state.toml` `[panes] split` (0,2–0,8).
- Pencere 640 px'ten darsa ikinci bölme açılmaz (`The window is too narrow for two panes`); açıkken pencere daraltılırsa bölmeler en az 240 px'te kalır, önizleme bölmesi önce kapanır (durum satırında bir kez söylenir).

### 4.2 Açma ve kapama

- Eylem `toggle-dual-pane`: Windows/Linux **F3**, macOS **⌃⌘P**; View menüsünde `Two Panes` (işaretli), palet. Varsayılan kapalı.
- Açılınca sağ bölme **son kapanışındaki sekmeleri** gösterir (FL 4.3.3); hiç açılmamışsa sol bölmenin etkin klasörünü. Sağ bölme yeni etkin bölme olur.
- Kapanınca sağ bölmenin listelemesi, modeli, izleyicisi, klasör boyutu işleri ve küçük resim istekleri bırakılır; yalnız sekmelerinin yerleri (`Session`) bellekte ve `state.toml`'da kalır. Etkin bölme sağsa odak sola geçer. Yürüyen arama sonuçları sağ bölmedeyse arama durur. **Açık test:** kapatmadan sonra izleyici sayısı 1, sağ bölmenin `Rc`'leri düşmüş (zayıf tutamak `upgrade` edemez), boşta bellek tek bölmeli değere ±0,1 MB içinde döner (`panes.ps1`).
- Kapanırken bölmede kilitli sekme olması kapanmayı engellemez (sekmeler kapanmaz, saklanır).

### 4.3 Etkin bölme

- Tıklama, sürükleme bırakma, Tab ya da etkin bölmeye ait bir eylem etkin bölmeyi seçer. Klavye, kenar çubuğu, palet, menüler, önizleme, Hızlı Bakış, Bilgi penceresi, durum satırı etkin bölmeye bakar.
- **Gösterim:** etkin bölmenin sekme şeridinde etkin sekme vurgu renginde 2 px üst çizgi; etkin olmayan bölmenin seçili satırları `Theme.selection` %45 saydamlıkla (yeni tema anahtarı yok; FP'nin soluk etkin olmayan bölmesi gibi ama yalnız seçim).
- Eylem `focus-other-pane`: varsayılan **Tab** (her sistemde), **yalnız ikinci bölme açıkken ve liste klavyedeyken**. Bugün listede Tab Slint'in odak zincirine gider; bölme kapalıyken öyle kalır. Yazma alanlarında (adres, süzgeç, arama, ad alanı) Tab alanındır (bugünkü sıra). *Erişilebilirlik:* klavyeyle gezinen kullanıcı Tab ile denetimler arasında dolaşır; Tab'ı yalnız liste odaktayken ve iki bölmede almak bu yolu tek bölmede hiç, iki bölmede yalnız listede değiştirir. Listeden çıkmak için Shift+Tab ve Ctrl+L / Esc yolları kalır; ekran okuyucuya bölme değişimi `accessible-label` (`Left pane, C:\Work` / `Right pane, …`) ile söylenir.
- **Etkin olmayan bölme izlenmez.** Boşta en çok bir klasör izleyicisi çalışır (etkin bölmeninki). Bölme etkinliğini kaybedince izleyicisi durur; etkin olunca klasörü **arka planda yeniden okunur** (gösterilen liste, seçim ve kaydırma okuma gelene kadar yerinde kalır; titreme yok). Ayrıca Gezik'in kendi işleri (F5/F6, sürükle-bırak, yapıştırma, silme, yeniden adlandırma) hedef ya da kaynak olarak dokunduğu etkin olmayan bölmeyi iş bitince hemen yeniler (iş motorunun `Outcome`'larından; ek izleyici yok). *Gerekçe:* iki izleyici iki işletim sistemi tutamağı ve (Windows'ta `ReadDirectoryChangesW`, Linux'ta inotify) ek uyanmalar demektir; görülen bedel, başka bir uygulamanın etkin olmayan bölmenin klasörüne yaptığı değişikliğin o bölmeye dönülene kadar görünmemesidir (§16).

### 4.4 F5 ve F6: öbür bölmeye kopyala, taşı

- Eylemler `copy-to-other-pane` (**F5**) ve `move-to-other-pane` (**F6**), her sistemde (macOS dizüstü klavyelerinde fn+F5/F6; menüde ve palette her zaman).
- **Bağlama bağlı tuş:** kısayol tablosu yeni bir kural alır: `Action::needs_dual_pane()` olan bir eylem, olmayan bir eylemle aynı tuşu paylaşabilir (çakışma uyarısı yok). Bugünkü `Shortcuts::action_for(&chord)` bir bağlam almaz (ilk eşleşen bağ); yerine **`action_in(&chord, KeyContext { dual })`** gelir: bağlarda o tuşun bütün eylemlerine bakar, `dual` ise `needs_dual_pane` olanı, değilse olmayanı döner. `action_for` tek bölmeli bağlam için ince bir sarmalayıcı olarak kalır (çağıranların çoğu ve testler değişmez); `keys::action_for` pencerenin bağlamıyla `action_in`'i çağırır. Çözüm: bölme açıksa bölme eylemi, değilse öbürü. Bugün F5 Windows/Linux'ta `refresh`'tir: bölme açıkken F5 kopyalar, kapalıyken yeniler. F6 bugün boştur: tek bölmede hiçbir şey yapmaz (tuş geçer). Bölme açıkken yenilemek için **Ctrl+R** Windows/Linux'ta `refresh`'in ikinci varsayılanı olur (Explorer'ınki).
- **Akış:** kaynak etkin bölmenin seçimi (seçim yoksa odaktaki öğe), hedef öbür bölmenin etkin sekmesinin klasörü. `[panes] confirm = true` (varsayılan) iken soru penceresi: `Copy 3 items to D:\Yedek?` / `Move …`, hedef yazılabilir alan (Far gibi; boş bırakılmaz, göreli yol hedefe göre çözülür, olmayan klasör oluşturulur), `Enter` onaylar, `Don't ask again` ayarı yazar. İş, sürükleyip öbür bölmenin boşluğuna bırakmakla **aynı yoldan** geçer: çakışma listesi, iş paneli, geri alma, çöpten çıkarmada bilgi dosyasının silinmesi (9b2), sürücü kuralı yok (her zaman istenen etki).
- **Hedef klasör değilse:** öbür bölme This PC, arama sonuçları, düz görünüm ya da çöpse: `The other pane shows no folder to copy into` (F6 ve çöp: `Use Delete to move items to the Recycle Bin`). Kaynak ile hedef aynı klasörse: F5 `Duplicate` gibi davranmaz, `Both panes show the same folder` der.
- Kaynak arama sonuçları ya da düz görünüm olabilir (yolları bellidir); çöp görünümünden F6 geri yükleme gibi taşır.

### 4.5 Sekmeyi öbür bölmeye taşıma

- Eylem `move-tab-to-other-pane` (varsayılan tuş yok): sekme menüsünde `Move to Other Pane`, palet. Bölme kapalıysa önce açar. Sekme geçmişiyle, kilidiyle, görünüm durumuyla (seçim, kaydırma, süzgeç) taşınır; bölmenin son sekmesiyse bölme o sekmenin yerine başlangıç klasöründe yeni bir sekme açar (bölme boş kalmaz).
- **Sürükleyerek:** sekme şeridindeki sürükleme (bugün yalnız sıralama) öbür bölmenin sekme şeridine bırakılınca oraya taşır; araya yerleştirme çizgisi bugünkü sıralama çizgisiyle. Sekme öbür bölmenin listesine bırakılırsa da aynı (Files'ın "sekmeyi bölmeye sürükle"si).

### 4.6 Bölmeler arası sürükle-bırak

- `gezik-core::drag::Layout` bölme başına bir `PaneArea` taşır (liste, sekme şeridi, adres parçaları); `Hit` bölme numarası alır (`Entry(pane, i)`, `Background(pane)`, `Tab(pane, i)`, `Crumb(pane, i)`). Bırakma etkisi bugünkü kuraldır (aynı sürücü taşır, değilse kopyalar, Shift/Ctrl/Alt). Bırakılan bölme etkin olur.
- Bırakma yığını, kenar çubuğu ve dış uygulamalardan bırakma değişmez; dışarıdan bırakma imlecin altındaki bölmeye gider.

### 4.7 Eşli gezinme

- Eylem `sync-browsing` (aç/kapa; tuş yok; View menüsünde `Sync Browsing`, palet). Yalnız iki bölme açıkken açılır; açıkken iki adres çubuğunun arasında bağlantı simgesi ve her iki bölmenin sekme şeridinde ince vurgu.
- **Yalnız göreli adımlar eşlenir:** etkin bölmede bir alt klasöre inmek (çift tık, Enter, adres parçası değil), üst klasöre çıkmak (Up; Miller'de sol ok) öbür bölmede aynı adla aynı adımı yapar. Geri/İleri, adres çubuğu, kenar çubuğu, sekme değiştirme mutlak sıçramadır: eşlemeyi **kapatır** ve durum satırı `Sync browsing off: the panes went apart` der.
- Öbür tarafta aynı adlı klasör yoksa (büyük/küçük harf duyarsız Windows/macOS, duyarlı Linux) eşleme kapanır: `Sync browsing off: no folder "x" in D:\Yedek` (TC'nin `cm_SyncChangeDir`'i adlar uyuşmayınca askıya alır; Gezik açık bir sözle kapatır).
- Öbür bölmenin okuması arka plandadır; etkin bölmeyi beklemez.

### 4.8 Diğer bölme eylemleri

- `swap-panes`: Windows/Linux **Ctrl+U** (TC, Far), macOS **⌃⌘U**: iki bölmenin sekme takımlarını değiştirir; etkin bölme yer değiştirir, odak aynı sekmede kalır.
- `other-pane-same-folder` (tuş yok; TC "Target = Source"): öbür bölmenin etkin sekmesi etkin bölmenin klasörüne gider (geçmişe yazılır).

### 4.9 Oturum, sekme setleri, tek örnek

- **`state.toml`:** bugünkü `[session] tabs, active` sol bölmedir (eski dosyalar aynen okunur). Yeni anahtarlar: `right-tabs`, `right-active` (bölme kapalıyken de saklanır), `dual = false`, `active-pane = 0`, `sync = false`; `[panes] split = 0.5`. `[session] restore = false` iken sağ bölme sekmeleri de unutulur.
- **Sekme setleri:** `[[tab-sets]]` isteğe bağlı `right = [...]` alır; böyle bir set açılınca ikinci bölme de açılır. `Save Tab Set…` bölme açıkken iki takımı da yazar.
- **Tek örnek ve CLI:** gelen yollar odaktaki pencerenin **etkin bölmesinde** açılır (aynı klasörü gösteren sekme kuralı yalnız o bölmede aranır). Yeni bayrak yok.

### 4.10 Öbür parçalarla etkileşim

| Parça | İki bölmede |
|---|---|
| Palet, hızlı açma | Pencerenin; "git" öğeleri etkin bölmede açar; Alt+Enter yeni sekme etkin bölmede |
| Arama, düz görünüm | Bölmenin (bir `Location`); iki bölmede iki ayrı arama aynı anda yürüyebilir, ad önbelleği ortak |
| Süzgeç çubuğu, harfle atlama | Bölmenin; Tab ile bölme değişince harfle atlama sıfırlanır |
| Önizleme bölmesi, Hızlı Bakış | Pencerenin, etkin bölmenin seçimini gösterir; bölme değişince güncellenir |
| Bilgi penceresi (9a3), `Get Info` | Etkin bölmenin seçimi |
| Çöp görünümü (9b2) | Bölmenin; `Put Back`, `Empty Trash` etkin bölmede çöp gösteriliyorsa |
| Bulut işaretleri (9b5) | Satır verisi; değişiklik yok |
| İş paneli, geri alma | Sürecin; Ctrl+Z hangi bölmede basılırsa basılsın son işi geri alır (bugünkü anlam) |
| Bırakma yığını | Pencerenin şeridi, sürecin içeriği; `Copy here` / `Move here` etkin bölmeye |
| Klasör izleyicisi | Yalnız etkin bölmede (§4.3); etkin olmayan bölme etkin olunca yeniden okunur, Gezik'in işleri onu hemen yeniler; kapanan bölmeninki bırakılır |
| Klasör boyutları (8b) | Bölme başına, gösterilen klasör için; ayar aynı |
| Çıkarma (9b6) | Çıkarılan sürücüyü gösteren her bölmedeki sekmeler This PC'ye döner |
| Kenar çubuğu, ağaç (10c) | Pencerenin; tık etkin bölmeyi götürür, orta tık etkin bölmede yeni sekme; vurgu etkin bölmenin klasörü |
| Yeniden adlandırma alanı | Bölmenin; alan açıkken bölme değiştirmek alanı bitirir (bugünkü odak kaybı kuralı) |

## 5. Kenar çubuğunda klasör ağacı (10c)

### 5.1 Biçim

- Kenar çubuğunun klasör gösteren her satırı (Folders, Pinned, Drives, Cloud; Searches ve Trash değil) solunda açma oku taşır. Ok tıklanınca ya da satır odaktayken → / ← ile dal açılır/kapanır; satır tıklanınca bugünkü gibi gider. Ayrı bir "ağaç" bölümü yoktur (Explorer'ın gezinme bölmesi gibi yerler kök olur).
- Alt satırlar girintili (her düzey 12 px), yalnız klasörler, adla sıralı (doğal sıra), gizliler `[view] show-hidden`'a göre. Ok yalnız alt klasörü olabilecek satırda: okunana kadar her klasör "olabilir" sayılır (dosya sistemi sormadan); açıldığında boş çıkarsa ok kaybolur.
- Kısayol ve fare: orta tık yeni sekmede, sağ tık klasör satırının menüsü (listedeki klasör satırının menüsüyle aynı: Windows'ta Shell menüsü dahil), sürükleyip bırakma hedefidir (`SideRow::Item`), öğeler sürüklenebilir değildir (pin değil).

### 5.2 Okuma ve bellek

- Dal açılınca alt klasörleri arka planda okunur (`list_dir`'in yalnız klasörleri; bugünkü listeleme iş parçacığı yolu), sonuç gelene kadar satırda dönen işaret. Okuma 300 ms'yi aşarsa kullanıcı başka şeyler yapabilir; sonuç geldiğinde dal hâlâ açıksa doldurulur.
- **İzleme yok.** Açık dal şu anlarda yeniden okunur: dal kapatılıp açılınca; Gezik'in kendi işi (oluştur, sil, taşı, yeniden adlandır) o klasöre dokununca (iş motorunun `Outcome`'larından); bir bölme o klasörü gösterip listeledikçe (listeleme zaten okunmuştur, alt klasör adları ondan alınır, ek G/Ç yok).
- Dal kapanınca altındaki bütün düğümler bırakılır (yeniden açmak yeniden okur). Açık dallar oturumda saklanmaz: Gezik her açılışta kapalı ağaçla başlar (açılışta okuma yok).
- **Pencereli model:** kenar çubuğu bugün bütün satırları `for` ile kurar. 10c'de satırlar sanal listeye geçer: Rust yalnız görünen satırları (+ bir ekran pay) modele koyar, Slint toplam yüksekliği bir sayıdan alır (`ItemsModel`'in yolu). Böylece 50.000 alt klasörlü bir dal yalnız görünen ~40 satırlık öğe kurar. Tek dalda en çok 20.000 alt klasör gösterilir, fazlası için son satır `… 31,204 more (open the folder)`.

### 5.3 Etkin klasörü izleme

- 10c, 10a'dan önce geldiği için "etkin bölmenin klasörü" 10c'de bugünkü gösterilen klasördür; 10a-1'den sonra etkin bölmeninki olur (aynı çağrı, `with_active` üzerinden).
- `[sidebar] tree-follow = false` (varsayılan; Explorer'ın "Expand to open folder"ı da varsayılan kapalıdır). Açıkken etkin bölmenin klasörü değişince en yakın kök satırının altında atalar açılır (her ata düzeyi bir arka plan okuması, yalnız açık değilse) ve satır görünür kaydırılır. Kapalıyken yalnız zaten görünen satır vurgulanır.
- Eylem `reveal-in-tree` (tuş yok; adres çubuğunun menüsünde `Show in Sidebar Tree`, palet): ayar kapalıyken bir kerelik aynı işi yapar.

### 5.4 Kabul

Ağaç kapalıyken boşta bellek ve açılış `master` ile aynı (≤ +0,02 MB). 1.000 satırlık açık ağaç ≤ +0,5 MB. 50.000 alt klasörlü dal açılırken kare süresi takılmaz (`stress.ps1`'e eklenen ölçüm: açma sırasında en uzun kare ≤ 50 ms). Kenar çubuğu kaydırma CPU'su 1.000 satırda bugünkü 30 satırlık kenar çubuğununkinden ≤ %20 fazla.

## 6. Gruplama (10d)

### 6.1 Gruplar

- `ViewSettings` yeni alan alır: `group: GroupBy` (`none` varsayılan, `type`, `date`, `size`). `views.toml` `group = "type"`, varsayılan `[view] group = "none"`. Görünüm belleği ve kurallar (§8) onu da taşır. 12. adımın `tag`'i aynı sıralamaya bir değer ekler.
- **Tür:** satırın tür adı (`Type` sütununun metni); klasörler `Folder` grubunda, `folders-first` açıksa başta. Gruplar ada göre.
- **Tarih:** `Modified` (sıralama `Created` ise oluşturma tarihi) Explorer'ın kovaları: `Today`, `Yesterday`, `Earlier this week`, `Last week`, `Earlier this month`, `Last month`, `Earlier this year`, sonra yıllar (`2024`), `A long time ago` (≥ 10 yıl), `No date`. Yeniden eskiye.
- **Boyut:** Explorer'ın kovaları: `Empty` (0), `Tiny` (< 16 KB), `Small` (< 1 MB), `Medium` (< 128 MB), `Large` (< 1 GB), `Huge` (< 4 GB), `Gigantic`; klasörler `Folders` (boyutu bilinmeyen; klasör boyutu hesaplanmışsa onun kovası). Büyükten küçüğe.
- Grup içi sıra bugünkü sıralamadır; grupların sırası yukarıdaki gibi, sıralama yönü ters çevrilince gruplar da ters döner (Explorer gibi).
- Arama sonuçları ve düz görünümde de çalışır (aynı alanlar). Çöpte `date` "silinme tarihi"dir.

### 6.2 Çizim ve etkileşim

- Liste: grup başlığı tam genişlikte bir satır (`Today (12)`, kalın, altında ince çizgi); ızgara: bir hücre satırını kaplar. `ItemRow` yeni alan alır: `header: string`, `collapsed: bool`, `count: int`; başlık satırı `cells` boş.
- Başlık tıklanınca grup kapanır/açılır; sağ tık: `Collapse All Groups`, `Expand All Groups`, `Group By ▸`. Kapalı grupların durumu yalnız o klasör gösterildiği sürece tutulur.
- Klavye: ok tuşları başlıkları atlar; Home/End ilk/son öğe; seçim yalnız görünen (açık gruplardaki) öğelerdir: `Select All` kapalı gruplardakini seçmez. Harfle atlama kapalı gruptaki öğeye gelince grubu açar.
- Yerleşim (`gezik-core::layout`) satır başına bir öğe yerine "başlık + hücreler" satırları bilir; `Geometry` başlık satırlarını da sayar (lastik bant seçimi ve sürükleme isabeti onlardan geçmez).
- `View ▸ Group By ▸ None / Type / Date / Size`, sütun başlığının menüsünde aynısı. Eylemler `group-none`, `group-type`, `group-date`, `group-size` (tuş yok; palet).

### 6.3 Maliyet

Grup anahtarı sıralama sırasında hesaplanır (aynı geçiş, ek dosya sistemi çağrısı yok). Gruplama yokken model ve yerleşim bugünkü yoldan gider (dal yok sayılacak kadar küçük). Gruplu model başlık satırlarını bir `Vec<(usize, GroupKey)>` olarak tutar: öğe başına ek bellek yok, grup başına ≤ 64 B.

## 7. Miller sütunları (10e)

### 7.1 Kip

- `ViewMode::Columns` (`views.toml` `mode = "columns"`). Eylem `view-columns`: Windows/Linux **Ctrl+Shift+3**, macOS **⌃⌘3** (⇧⌘3 sistemin ekran görüntüsüdür). View menüsünde `as Columns`.
- Bölme genişliğinde yatay kayan sütunlar; her sütun bir klasörün listesi (simge, ad, klasörse sağda ›). Sütun genişliği 220 px (sürüklenip değiştirilebilir, hepsine uygulanır; `state.toml` `[panes] column-width`). Son sütun seçili öğe dosyaysa **önizleme sütunu**dur: bugünkü `PreviewPane` bileşeni ve `preview.rs`'in bilgisi (PF'nin önizleme sütunu). Pencerenin önizleme bölmesi açıksa önizleme sütunu gösterilmez (aynı şey iki kez çizilmez).
- **Konum:** odaktaki sütunun klasörü sekmenin konumudur (adres çubuğu, başlık, durum satırı onu söyler). Sütunlar arasında gezinmek **geçmişe yeni girdi yazmaz**, bugünkü girdiyi değiştirir (geçmiş temiz kalır; Finder da sütunlar içindeki her adımı geçmişe yazmaz). Adres çubuğu, kenar çubuğu, Geri/İleri sütun kökünü değiştirir: kök o klasör olur, sütun yolu kökten yeniden kurulur. Kök, gidilen klasörün kendisidir; ata sütunlar gösterilmez (Up atayı kök yapar).
- **Klavye:** ↑↓ sütun içinde; → seçili klasöre girer (yeni sütun, ilk öğe seçili), ← üst sütuna döner (sağdaki sütunlar kapanır); Enter dosyayı açar; Home/End, PgUp/PgDn sütun içinde. Harfle atlama ve süzgeç odaktaki sütundadır. Çoklu seçim yalnız odaktaki sütunda.
- **Fare:** tıklama seçer ve klasörse sağına sütun açar; çift tık dosyayı açar; sürükleme ve bırakma her sütunun satırlarında ve boşluğunda (boşluk = o sütunun klasörü).

### 7.2 Okuma, bellek, çizim

- Her sütun bir listelemedir (arka planda okunur). Bellekte yalnız **görünen sütunlar ve odaktaki sütunun atalarının ilk 8'i** kalır; daha soldaki sütunlar sola kayıp ekrandan çıkınca listeleri bırakılır, geri kaydırınca yeniden okunur.
- İzleyici yalnız odaktaki sütunun klasöründedir (bugünkü tek izleyici kuralı); diğer sütunlar odak onlara gelince ya da Gezik'in kendi işi dokununca yenilenir.
- Satırlar sütun başına sanal liste (`ListView`). Yazılım çizicisinde yatay kaydırma bütün içerik alanını yeniden çizer: sütun açılışında kaydırma canlandırılmaz (`reduce-motion`'dan bağımsız tek adım), böylece bir sütun açılışı tek tam kare maliyetindedir.
- Küçük resim yok (yalnız simge); önizleme sütunu bugünkü önizleme yolundan.

### 7.3 Sınırlar

- Sütun kipinde sütun başlıkları, boyut ve tarih sütunları yoktur (Finder gibi; önizleme sütunu bilgiyi verir). Gruplama sütun kipinde uygulanmaz (ayar korunur, kip değişince geri gelir).
- Arama sonuçları, düz görünüm, çöp ve This PC sütun kipine geçemez: bu yerlerde `view-columns` liste kipini seçer ve durum satırı bir kez söyler.

## 8. Otomatik görünüm kuralları (10f)

### 8.1 Biçim

`settings.toml` (kullanıcının yazdığı, taşınabilir) — `views.toml` Gezik'in yazdığı makineye özgü dosya olduğu için kurallar oraya girmez:

```toml
# The first rule that matches a folder sets its view, unless you changed that folder's view
# yourself (views.toml keeps that). Keys left out keep the default.
[[view-rules]]
path = "{pictures}/**"        # glob on the folder path; {home} {documents} {downloads} … tokens
mode = "grid"
grid-size = "large"

[[view-rules]]
kind = "network"              # drives | trash | search | flat | network | removable | cloud | archive-root
mode = "list"
columns = ["modified", "size"]

[[view-rules]]
content = "pictures >= 50%"   # pictures | videos | audio | documents | archives | code | folders
mode = "grid"

[[view-rules]]
path = "~/Downloads"
sort = "modified"
sort-dir = "desc"
group = "date"
```

- Anahtarlar: `path` (glob: `*`, `**`, `?`; Windows'ta büyük/küçük harf duyarsız; `~` ve bugünkü `{home}` belirteçleri), `kind`, `content`; en az biri gerekli, birden çoğu verilirse hepsi eşleşmeli. Uygulananlar: `mode` (`list`, `grid`; `columns` 10e'nin ardından, §8.3), `sort`, `sort-dir`, `group`, `grid-size`, `columns`.
- Hatalı kural bugünkü uyarı kalıbıyla atlanır (`view-rules[2]: content: expected "<kind> >= <n>%"`).

### 8.2 Öncelik ve zamanlama

1. Kullanıcının o klasör için değiştirdiği görünüm (`views.toml`) — en son açık niyet odur.
2. Eşleşen ilk kural.
3. `[view]` varsayılanları.

- `path` ve `kind` kuralları listeleme başlamadan bilinir (ilk kare doğru kipte çizilir). `content` kuralları listeleme geldiğinde, **ilk çizimden önce** değerlendirilir (listeleme tek parça gelir; tür sayımı ada bakar, ek G/Ç yok). Sonuç aramaları yalnız `kind = "search"`/`"flat"` ile eşleşir.
- Kural tarafından seçilen görünümde kullanıcı bir şeyi değiştirirse (kip, sıralama …) o klasör `views.toml`'a yazılır ve kural o klasör için bir daha uygulanmaz. View menüsünde `Reset to Rule` (o klasörün belleğini siler) ve hangi kuralın uygulandığı (`View rule 2 applies`).
- Kural yokken hiçbir şey ayrılmaz.

### 8.3 Maliyet ve 10e ile ilişki

- **Önceden derleme:** ayarlar okunurken her `path` glob'u bir kez derlenir (belirteçler çözülmüş, Windows'ta küçük harfe çevrilmiş parça listesi; `regex` kullanılmaz); `content` oranı sayıya, `kind` bir sayısal türe çevrilir. Klasör açılışında yalnız bellekteki karşılaştırma çalışır.
- **Erken çıkış:** kurallar sırayla denenir, ilk eşleşen kazanır; bir kuralın koşulları ucuzdan pahalıya (`kind`, `path`, `content`) değerlendirilir ve ilk uymayan koşulda bırakılır. `content` yalnız `kind` ve `path`'i uyan kuralda ve listeleme geldiğinde bir kez sayılır (aynı listeleme için sayım önbelleklenir).
- **Dosya sistemi çağrısı yok:** `kind`'ın bilgisi `Location`'dan ve zaten bilinen sürücü listesinden (`removable`, `network`), bulut köklerinden (9b5) gelir; hiçbir kural `stat`, `canonicalize` ya da okuma yapmaz (birim testinde sahte dosya sistemiyle sayılır: 0 çağrı).
- **Başarım testi:** 20 kural (10 `path`, 5 `kind`, 5 `content`) ve 100.000 öğelik listelemede değerlendirme ≤ 50 µs (`content` sayımı hariç; sayım ≤ 2 ms), `cargo bench` yerine `#[test]` içinde süre sınırıyla ve `scripts/perf/rules.ps1` ile sürüm derlemesinde.
- **`mode = "columns"`** 10f'de **yoktur**: 10f yalnız 10d'ye bağlıdır ve 10e'den önce de birleşebilir. 10e birleştikten sonra küçük bir ek PR (**10e+**: kurallarda `mode = "columns"`, ayrıştırıcıya bir değer ve bir test) bunu açar. O zamana kadar `mode = "columns"` bugünkü uyarı kalıbıyla atlanır.

## 9. Tek süreçte çok pencere (10g)

### 9.1 Neden şimdi ve 9b1'in kararını değiştirmek

9b1'de `new-window` yeni süreçti (9. spec §5.3, karar 9). Sekmeyi koparıp başka pencereye bırakmak, bırakma yığınını, iş panelini ve geri alma geçmişini paylaşmak bir süreç ister. 10g bu kararın yerine geçer: **yeni pencere aynı süreçtedir.** 10g'nin PR'ı 9. spec'in §5.1 yardım metnini ve §5.3'ünü, `gezik --help` çıktısını ve README'nin komut satırı bölümünü `--new-window`'un yeni anlamıyla günceller (9. spec'e "10g ile değişti" notu düşülür, eski metin silinmez).

### 9.2 Önce yoklama

10g'nin ilk görevi bir yoklamadır (Windows ve macOS; Linux X11/Wayland'de yalnız ilk iki madde):

1. İki `AppWindow` örneği bir olay döngüsünde: odak olayları, kapatma, birinin kapanınca çerçeve arabelleğini bırakması (süreç belleği ölçümüyle).
2. Winit olay yönlendirici (§9.3) iki pencerede `Focused` olaylarını doğru pencere kimliğiyle veriyor mu.
3. **macOS menü çubuğu:** her `AppWindow`'daki Slint `MenuBar`'ı, anahtar pencere değişince sistem menü çubuğuna geçiyor mu. Geçmiyorsa menü tek pencereye (ilk pencere) bağlı kalır ve bütün `menu-command(name)` çağrıları `windows::with_focused`'a yönlendirilir; işaretli öğeler (`Two Panes` ✓, `Unlock Tab`) odak değişince yeniden kurulur. Hangisi olduğu yoklama notuna yazılır, plan ona göre kurulur.
4. Sekme koparma yakalaması (§9.5): Windows'ta fare basılıyken pencere dışı hareketler, macOS'ta pencere dışı sürükleme olayları.
5. Aynı süreçte pencereler arası OLE/`NSDraggingSession` dosya bırakması.

### 9.3 Pencereler

- `windows.rs`'te iş parçacığı yerel kayıt: `Vec<WindowCtx>`, odaktaki pencerenin kimliği ve odak sırası.
- **Olay yönlendirici:** bugün `main.rs` tek pencereye `on_winit_window_event` kancası takar. 10g'de her pencereye aynı kanca takılır ve `windows::route(window_id, &event)`'e verir; yönlendirici `Focused(true)`'da odak sırasını günceller, kalan olayları (bugün kancanın işlediği her şey) o pencerenin `WindowCtx`'ine iletir. Odak için zamanlayıcı ya da yoklama yok.
- `new-window` (Ctrl+N / ⌘N): odaktaki pencerenin etkin klasörüyle tek bölmeli yeni pencere; konum odaktakinin +24 px kaydırılmışı, boyut onunki.
- **Her pencerede yeniden uygulananlar:** Slint'te global'ler ve kök özellikleri bileşen örneğine özeldir. Yeni pencere açılırken ve ayar/tema her yeniden yüklendiğinde **her pencereye** uygulanır:
  - `Theme` global'inin bütün değerleri (`theme_bridge::apply_global`: renkler, boyutlar, yazı tipleri) ve `Theme.reduce-motion` (`set_reduce_motion`);
  - `Glyph` global'i sabittir (Rust yazmaz), yeniden uygulanmaz;
  - `apply_config`'in kök özellikleri: `notice`, `sidebar-position`, `sidebar-tip` (sıfırlama), `view-*` seçenek işaretleri (`view_options`), `mono-font`, `native-menus`, macOS'ta `bar-commands` ve `bar-tab-sets`, pencerenin kendi `sidebar-width`, `preview-width`, `preview-open` değerleri (yeni pencere odaktakinden kopyalar).
  Bunlar `windows::for_each(|w| …)` ile tek bir `apply_to_window(window, &loaded)` işlevinden geçer; testte iki pencereli kayıtla, tema değişiminden sonra iki pencerenin `Theme.background`'ı karşılaştırılır.
- **Kapatma:** son sekmesi kapanan ya da kapat düğmesine basılan pencere kapanır; başka pencere varsa süreç sürer; son pencere kapanınca süreç biter (tepsi açıksa 9b9'un kuralı). Kapanan pencerenin `AppWindow`'u düşürülür ve **çerçeve arabelleği bırakılır** (açık test: `windows.ps1` ikinci pencereyi açıp kapatır, özel bellek taban + 0,3 MB'a döner). Kapanan pencerenin işleri sürer (iş motoru sürecin); sorusu bekleyen bir iş varsa soru odaktaki pencereye geçer.
- **Tek örnek:** gelen yollar **en son odaklanan** pencerede açılır (o öne gelir). `--new-window` artık çalışan Gezik'te yeni pencere açar (Gezik çalışmıyorsa ilk pencere olur); ayrı süreç isteyen için `[system] single-instance = false` kalır.
- **9b1'in asılı örnek geri düşüşü korunur:** gönderen 2 sn içinde `ok` alamazsa kendi penceresini **yeni süreç** olarak açar ve tek örnek kanalını almaz (9. spec §5.2). 10g'de bu yol bir testle korunur: sahte dinleyici yanıt vermez, gönderen 2 sn sonra kendi penceresini açar; `--new-window` için de aynı.
- **macOS:** §9.2 yoklamasının sonucuna göre. Window menüsünde `Move Tab to New Window`; açık pencerelerin listesi AppKit'in kendi Window menüsü listesidir (yoklamada doğrulanır).

### 9.4 Sekmeyi menüyle taşıma (üç sistem)

- Eylem `move-tab-to-new-window` (tuş yok; sekme menüsünde `Move to New Window`, palet): sekme geçmişiyle, kilidiyle ve görünüm durumuyla yeni pencereye taşınır. Pencerenin tek sekmesiyse hiçbir şey yapmaz.
- Sekme menüsünde `Move to Window ▸` (yalnız birden çok pencere varken): açık pencereler başlıklarıyla; seçilen pencerenin etkin bölmesine taşır.

### 9.5 Sekmeyi sürükleyerek koparma (Windows ve macOS)

- Sekme sürüklemesi imleç pencerenin dışına çıkınca sekmenin hayalet görüntüsü imleçle gider (bugünkü sürükleme hayaleti). Bırakılan yer:
  - başka bir Gezik penceresinin sekme şeridi ya da listesi → sekme oraya (o pencerenin etkin bölmesine) taşınır;
  - Gezik penceresi olmayan bir yer → imlecin olduğu yerde yeni pencere;
  - pencerenin içine geri dönüp bırakmak → bugünkü sıralama/öbür bölmeye taşıma.
- Bu sürükleme **sistem sürüklemesi değildir** (dosya taşımaz): Gezik'in fare yakalamasıyla izlenir. İmlecin altındaki Gezik penceresi kayıttaki dış çerçevelerden (ekran koordinatlarında) bulunur.
- **Linux'ta (X11 ve Wayland) sürükleyerek koparma yoktur:** pencere dışına çıkan sekme sürüklemesi bugünkü gibi iptal olur, sekme yerinde kalır; §9.4'ün menü yolu kullanılır. *Gerekçe:* Wayland genel imleç konumunu vermez ve pencere konumlandırmaya izin vermez; X11'de çalışsa da yalnız bir oturum türünde olan bir davranış ekran testlerini ikiye böler. Başarı ölçütü bu yüzden yalnız Windows ve macOS'u sayar.
- Dosya sürüklemesi pencereler arasında bugünkü gibi sistem sürüklemesiyle olur (aynı süreçte kaynak ve hedef, §9.2 madde 5).

### 9.6 Paylaşılanlar ve pencereye özgüler

§3.1'in tablosu. Ek olarak: Hızlı Bakış penceresi süreçte tektir ve odaktaki pencerenin etkin bölmesine bağlanır; sistem bütünleşmesi paneli, `Connect to Server` katmanı odaktaki pencerede açılır; bir pencerede açık katman (soru, çakışma listesi) yalnız o pencerenin klavyesini alır.

### 9.7 Oturum

- `state.toml` `[session]` ilk penceredir (§4.9 anahtarlarıyla); ek pencereler `[[session.window]]`: aynı anahtarlar + `x`, `y`, `width`, `height`, `maximized`. `restore = true` iken hepsi açılır (kullanıcının açık bıraktığı pencereler; her biri bir çerçeve arabelleği maliyetindedir, §12).
- Pencerelerin sırası odak sırasıdır; açılışta en son odaklanan öne gelir.

## 10. Ayarlar, eylemler, tuşlar, menüler

### 10.1 `settings.toml`

```toml
[view]
group = "none"           # none | type | date | size: the list's groups when a folder has no view of its own

[panes]
confirm = true           # F5/F6 ask before copying or moving to the other pane

[sidebar]
tree-follow = false      # the sidebar tree opens down to the folder shown

# [[view-rules]] — see §8
```

`state.toml` (Gezik yazar): `[session] right-tabs, right-active, dual, active-pane, sync`, `[[session.window]]`, `[panes] split, column-width`.

### 10.2 Eylemler ve varsayılan tuşlar

| Eylem | Windows / Linux | macOS | Parça |
|---|---|---|---|
| `toggle-dual-pane` | F3 | ⌃⌘P | 10b |
| `focus-other-pane` | Tab (yalnız iki bölmede, liste odaktayken) | Tab (aynı) | 10b |
| `copy-to-other-pane` | F5 (yalnız iki bölmede) | F5 | 10b |
| `move-to-other-pane` | F6 (yalnız iki bölmede) | F6 | 10b |
| `move-tab-to-other-pane` | — | — | 10b |
| `swap-panes` | Ctrl+U | ⌃⌘U | 10b |
| `sync-browsing` | — | — | 10b |
| `other-pane-same-folder` | — | — | 10b |
| `reveal-in-tree` | — | — | 10c |
| `group-none`, `group-type`, `group-date`, `group-size` | — | — | 10d |
| `view-columns` | Ctrl+Shift+3 | ⌃⌘3 | 10e |
| `move-tab-to-new-window` | — | — | 10g |

- `Action` 85 → 93 (10b) → 94 (10c) → 98 (10d) → 99 (10e) → 100 (10g).
- **Değişen varsayılanlar (Windows/Linux):** F3 kullanıcının seçimiyle `toggle-dual-pane`'indir. `search` F3'ü bırakır (`mod+shift+f` kalır) ve **Ctrl+E** alır (Explorer'ın arama tuşu; bugün boş: `eject` yalnız macOS'ta `mod+e`). `refresh` F5'e ek olarak **Ctrl+R** alır (Explorer'ınki). F3'te `search`'ü kendi ayarına yazmış kullanıcının seçimi kazanır; o durumda `toggle-dual-pane` F3'süz kalır ve bugünkü uyarı söyler (`the default "f3" of toggle-dual-pane is used by search`).
- **Geçiş notu (F3):**
  - README'nin kısayol tablosu ve "What changed" bölümü: `F3 now opens a second pane. Search is Ctrl+Shift+F or Ctrl+E.`; `templates/settings.toml`'un `[shortcuts]` yorumu aynı satırı taşır.
  - **Tek seferlik ipucu:** 10b'li Gezik'in ilk açılışında (yalnız Windows/Linux, yalnız `[shortcuts]`'ta `search` ya da `toggle-dual-pane` yazılı değilse) durum satırı bir kez `F3 now opens a second pane; search is Ctrl+Shift+F or Ctrl+E` der. Gösterildiği `state.toml` `[hints] f3-moved = true` ile işaretlenir, bir daha çıkmaz. Ek olarak, ipucu işaretlendikten sonraki 7 gün içinde F3'e basılıp ikinci bölme açılınca aynı söz bir kez daha durum satırında görünür (kullanıcı aramayı beklerken bölme açtıysa nedenini görsün); sonra yoktur. Zamanlayıcı yok: tarih yalnız F3'e basılınca karşılaştırılır.
- **Bağlama bağlı tuş kuralı (§4.4):** yalnız `needs_dual_pane()` eylemleri (`focus-other-pane`, `copy-to-other-pane`, `move-to-other-pane`) bir başka eylemle tuş paylaşabilir; iki bölme eylemi birbiriyle paylaşamaz. Arama `Shortcuts::action_in(&chord, KeyContext { dual })` ile bağlama göredir (§4.4); `keys::action_for` pencerenin bağlamını verir. `focus-other-pane` ayrıca `needs_list` eylemidir: yalnız liste odaktayken (`waits_for_text_fields` yolu). Tab bir `[[commands]]` tuşu olamaz (bugünkü `leaves_typing_alone` kuralı).
- **Değişen kısayol testleri** (`gezik-config/src/shortcuts.rs`):
  - `defaults_cover_every_action`: her varsayılan, eyleminin bağlamında (`dual` = `needs_dual_pane()`) `action_in` ile aranır; F5 iki bağlamda ayrı ayrı denetlenir.
  - `the_search_actions_have_their_keys`: `f3` artık `Search` değil `ToggleDualPane`; `ctrl+e` `Search`; `fixed_owner` döngüsüne `ctrl+e` eklenir.
  - `empty_string_disables_an_action`, `invalid_binding_keeps_default_with_warning` ve kullanıcı tuşlu tablo testi (`refresh = ""`): F5 tek bölme bağlamında bugünkü sonuç; iki bölme bağlamında `CopyToOtherPane` olduğu satırı eklenir.
  - Sekiz `assert_eq!(Action::ALL.len(), 85)` satırı her parçada yeni sayıya.
  - Yeni testler: paylaşım kuralı (bölme eylemi + normal eylem uyarısız, iki bölme eylemi uyarılı), `ctrl+r` iki bağlamda `Refresh`, `tab`'ın `[[commands]]`'a bağlanamaması, macOS'ta `mod+ctrl+p`/`mod+ctrl+u`/`mod+ctrl+3`.
  - `gezik/src/keys.rs`: `waits_for_text_fields` ve `needs_list` tablolarına `focus-other-pane`; `handle_key`'in iki bölmeli bağlam testi.
- Çakışma denetimi: `ctrl+u`, `ctrl+e`, `ctrl+r`, `ctrl+shift+3`, `f3`, `f6` Windows/Linux'ta, `mod+ctrl+p`, `mod+ctrl+u`, `mod+ctrl+3`, `f5`, `f6` macOS'ta bugün boş; `fixed_owner` ve macOS menü çubuğu sabitleriyle çakışmaz (test). macOS'ta ⌃⌘ + harf sistemin kısayolları dışında (⌃⌘D sözlük, ⌃⌘Q kilit, ⌃⌘F tam ekran kullanılmadı).
- Hepsi `templates/settings.toml`'un `[shortcuts]` yorumlarına ve "her varsayılan ulaşılabilir" testine girer.

### 10.3 Menüler

- **View** (Windows/Linux düğme menüsü ve macOS menü çubuğu): `as List`, `as Grid`, `as Columns`; `Group By ▸`; `Two Panes` ✓, `Sync Browsing` ✓, `Swap Panes`; `Reset to Rule` (bir kural uygulanmışken).
- **Sekme menüsü:** `Move to Other Pane`, `Move to New Window`.
- **Adres çubuğu menüsü:** `Show in Sidebar Tree`.
- **Liste başlık menüsü:** `Group By ▸`.
- **Menü kimlikleri:** 10 için **2000–2199**: `GROUP_NONE…GROUP_SIZE` 2000–2003 (2004–2009 etiket ve gelecek için ayrılmış), `COLLAPSE_GROUPS` 2010, `EXPAND_GROUPS` 2011, `MOVE_TAB_OTHER` 2020, `MOVE_TAB_WINDOW` 2021, `TWO_PANES` 2030, `SYNC_BROWSING` 2031, `SWAP_PANES` 2032, `SAME_FOLDER` 2033, `VIEW_COLUMNS` 2040, `RESET_TO_RULE` 2041, `REVEAL_IN_TREE` 2050, ağaç satırı menüsü 2060–2099. `GEZIK_IDS_END = 4096` değişmez.

## 11. Kod yapısı

| Parça | Yer |
|---|---|
| Bölme | `gezik/src/pane.rs` (yeni): `Pane`, `PaneHandle`, `panes::with_active`, `with_id` |
| Pencere bağlamı ve kayıt | `gezik/src/windows.rs` (yeni): `WindowCtx`, `with_focused`, odak, açma/kapama, oturum |
| Bölme verisi | `AppWindow`'da `[PaneData]` modeli, `gezik/src/pane.rs`'te modelin bakımı; yoklama notu `docs/superpowers/notes/2026-10-xx-10a0-probe.md` (§3.2) |
| Bölme bileşeni | `gezik/ui/widgets/pane.slint` (yeni; bugünkü `app.slint`'in sekme şeridi, araç satırı, çubuklar ve `FileView` bölümü taşınır) |
| Çift panel eylemleri, eşli gezinme | `gezik/src/dual.rs` (yeni); eşleme kararı saf: `gezik-core/src/sync_nav.rs` (yeni) |
| Sürükleme yerleşimi | `gezik-core/src/drag.rs` (`PaneArea`, `Hit` bölme numarasıyla), `gezik/src/drag.rs` |
| Bağlama bağlı tuş | `gezik-config/src/shortcuts.rs` (`needs_dual_pane`, çakışma kuralı), `gezik/src/keys.rs` |
| Ağaç | `gezik-core/src/tree.rs` (yeni; düğümler, düz satır listesi, açma/kapama, saf), `gezik/src/sidebar.rs`, `widgets/sidebar.slint` (pencereli model) |
| Gruplama | `gezik-core/src/group.rs` (yeni; kovalar, anahtar, saf), `gezik-core/src/sort.rs`, `gezik-core/src/layout.rs` (başlık satırları), `gezik/src/view/{model, listing}.rs`, `widgets/file-view.slint` |
| Miller | `gezik/src/view/columns.rs` (yeni), `gezik-core/src/columns.rs` (yeni; sütun yolu, bırakma kuralı, saf), `widgets/column-view.slint` (yeni) |
| Görünüm kuralları | `gezik-config/src/view_rules.rs` (yeni; ayrıştırma), `gezik-core/src/view_rules.rs` (yeni; eşleme ve içerik oranı, saf), `gezik/src/view/mod.rs` |
| Oturum | `gezik-core/src/nav.rs` (`Session` → `WindowSession { left, right, dual, active_pane, sync, geometry }`), `gezik-config/src/settings.rs` (`state.toml` okuma/yazma) |
| Pencere olay yönlendirici | `gezik/src/windows.rs` (`route`), `main.rs`'teki bugünkü kanca oraya taşınır |
| Sekmeyi koparma | `gezik/src/tab_drag.rs` (yeni; Windows ve macOS) |
| Ayarlar, eylemler, menüler | `gezik-config/src/{settings, settings_writer, shortcuts}.rs`, `templates/settings.toml`, `gezik/src/{actions, context_menu, menu_bar}.rs`, `ui/app.slint` |

## 12. Bütçe ve performans

| Parça | Exe | Boşta bellek (varsayılan ayarlar) | Özellik açıkken |
|---|---|---|---|
| 10d | ≤ +262.144 B | 0 | Gruplu 100.000 öğe: ≤ +0,1 MB, sıralama ≤ 1,2× |
| 10c | ≤ +262.144 B | ≤ +0,02 MB (ağaç kapalı) | 1.000 açık satır ≤ +0,5 MB |
| 10a-0 | — (ürüne girmez) | — | — |
| 10a-1 | ≤ +262.144 B | **0** | — |
| 10a-2 | ≤ +262.144 B | **0** | — |
| 10a-3 | ≤ +262.144 B | **0** | — |
| 10b | ≤ +262.144 B | ≤ +0,03 MB (sağ bölme kapalı) | İki bölme, iki küçük klasör: ≤ +0,6 MB, **tek izleyici** (etkin bölmeninki); sağ bölme kapanınca listeleme, model ve izleyici bırakılır, bellek tek bölmeli değere ±0,1 MB döner (açık test); açılışta iki bölme geri yüklenirse açılış ≤ +5 ms (ikinci listeleme arka planda) |
| 10e | ≤ +262.144 B | 0 | Sütun kipi: ≤ gösterilen sütunların listeleri + 0,5 MB |
| 10f | ≤ +262.144 B | 0 (kural yok) | 20 kural ≤ +20 KB, değerlendirme ≤ 50 µs, dosya sistemi çağrısı 0 |
| 10g | ≤ +262.144 B | ≤ +0,02 MB (tek pencere) | Her ek pencere ≤ çerçeve arabelleği (genişlik × yükseklik × 4 × ölçek²; 1280×800 @1x ≈ 4 MB, @2x ≈ 16 MB) + 2,5 MB; **kapanınca arabellek bırakılır**, bellek taban + 0,3 MB'a döner (açık test) |

Varsayılan ayarlarla toplam: 10c +0,02, 10b +0,03, 10g +0,02, diğerleri 0 → ≤ +0,07 MB (≤ +0,1 MB ölçütünün içinde).

- Ölçüm: her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1`, `stress.ps1`'i yazar. Yeni betikler: `scripts/perf/panes.ps1` (iki bölme açık/kapalı bellek, kapatmadan sonra geri dönüş, açık izleyici sayısı, F5 ile 1.000 dosya kopyası, iki bölmede kaydırma CPU'su), `scripts/perf/windows.ps1` (1, 2, 5 pencere bellek; ikinci pencereyi açıp kapatınca geri dönüş), `scripts/perf/rules.ps1` (20 kural, 100.000 öğe).
- **Yazılım çizicisi:** iki bölme aynı çerçeve arabelleğini paylaşır; bir bölmede kaydırma yalnız o bölmenin alanını kirletir, kaydırma CPU'su tek bölmeninkinden fazla olmamalı (hedef: iki bölmede bir bölmeyi kaydırmak ≤ tek bölmede kaydırmanın 1,05 katı). Ek pencere kendi arabelleğini ayırır: bu çok pencerenin bilinen maliyetidir ve bu yüzden Ctrl+N bir kullanıcı eylemidir, oturum yalnız kullanıcının açık bıraktığı pencereleri açar. Gizli/simge durumundaki pencerenin arabelleğinin bırakılıp bırakılmadığı 9b9'daki ölçümle birlikte not edilir.
- **Zamanlayıcı ve yoklama yok:** ağaç izlemez, etkin olmayan bölme izlenmez, eşli gezinme olayla, kurallar ayar yüklenirken derlenir, pencere odağı olay yönlendiricisinden.

## 13. Parçalar (her biri bir plan ve bir PR)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **10d** | Gruplama: tür, tarih, boyut; başlık satırları, kapanır gruplar, `views.toml` `group` (§6) | — |
| 2 | **10c** | Kenar çubuğunda ağaç, pencereli kenar çubuğu modeli, `tree-follow`, `reveal-in-tree` (§5); vurgu bugünkü gösterilen klasördür, 10a-1'den sonra etkin bölmeninki olur | — |
| 3 | **10a-0** | Slint yoklaması: iki `PaneView`, `PaneData` modeli, iki yönlü bağların geri çağrıya çevrilmesi, ölçümler (§3.2); yalnız yoklama notu | — (9. adımı beklemez) |
| 4 | **10a-1** | İnce `Pane`/`WindowCtx` bugünkü tekilleri sarar, tek bölme; `with_active` takma adı; `PaneId` arka plan sonuçlarında; mekanik, boşta 0 (§3) | 9. adımın açık dalları birleşmiş; 10a-0 |
| 5 | **10a-2** | `Media` ve `ViewMemory` süreç düzeyine | 10a-1 |
| 6 | **10a-3** | `PaneView` çıkarımı, `[PaneData]` (10a-0'ın seçtiği yol) | 10a-2 |
| 7 | **10b** | Çift panel: aç/kapa, etkin bölme ve tek izleyici, Tab, F5/F6 (bağlama bağlı arama, Ctrl+R, Ctrl+E, F3 geçiş notu), sekmeyi öbür bölmeye taşıma (menü + sürükleme), bölmeler arası sürükle-bırak, eşli gezinme, `swap-panes`, `other-pane-same-folder`, oturum ve sekme setleri (§4) | 10a-3 |
| 8 | **10e** | Miller sütunları: `ViewMode::Columns`, önizleme sütunu, klavye, bellek kuralı (§7) | 10a-3 (bölme), 10d (`ViewSettings` biçimi) |
| 9 | **10f** | Otomatik görünüm kuralları, `mode = "columns"` hariç (§8) | 10d (`group`) |
| — | **10e+** | Kurallarda `mode = "columns"` (küçük ek PR, §8.3) | 10e, 10f |
| 10 | **10g** | Yoklama (§9.2), tek süreçte çok pencere, olay yönlendirici, `new-window` değişikliği ve belgeleri, sekmeyi menüyle (üç sistem) ve sürükleyerek (Windows, macOS) taşıma, çok pencereli oturum (§9) | 10a-3, 10b (sekme taşıma yolu) |

Sıra bağımlılık, değer ve çakışma riskiyledir: 10d ve 10c bölme gerektirmez ve 9. adımın açık dallarının değiştirdiği dosyalara (`main.rs`, `actions.rs`, `context_menu.rs`, `navigation.rs`) en az dokunur, bu yüzden önce gelir. 10a-0 yoklaması beklemeden yapılır; 10a-1…10a-3, 9. adımın dalları birleştikten sonra sırayla. 10b en yüksek değer (8 rakibin 8'inde) ve 10a'nın hemen ardından. 10f yalnız 10d'ye bağlı olduğu için 10e'yi beklemeden de alınabilir (sıra numarası tercih sırasıdır). Çok pencere en riskli (Slint'te çoklu pencere, sürükleme yakalaması, macOS menüsü) ve en son. Her parçada Windows → macOS → Linux sırasıyla yazılır; bir sistemde yoklama başarısız olursa o kısım "doğrulanacak" notuyla PR açıklamasına yazılır.

### 13.1 Parça kabul testleri

**10a-0**
- Yoklama notu: iki `PaneView` ile derleme süresi, exe farkı, kaydırma CPU'su; bir bölmenin `PaneData` değişiminin öbürünü kirletmediği; `scroll-request` ve `rename-request` yollarının çalıştığı; kök özelliğine düşülmesi gereken alan varsa hangisi.

**10a-1**
- Bütün birim testleri geçer; davranış aynı. Arka plan listelemesi, arama partisi ve klasör boyutu sonucu bilinmeyen `PaneId` ile gelirse düşer (birim). `PaneId` sonuç türlerinde zorunlu alan (derleme denetimi).
- `measure.ps1`, `stress.ps1`, `tabs.ps1`, `grid.ps1` `master` ±%3; boşta +0; exe ≤ +262.144 B.

**10a-2**
- `Media` işçi iş parçacıkları süreçte ikiden fazla değil (iki `View` kurulan birim testinde); `ViewMemory` tek örnek, `views.toml` tek yazım. Ölçümler 10a-1 gibi; boşta +0.

**10a-3**
- `handle_key`'in her yolunun etkin bölmeyi kullandığı test (sahte `WindowCtx` iki bölmeyle kurulur, biri etkin: eylem yalnız onu değiştirir). Ölçümler 10a-1 gibi; boşta +0. Windows ekran listesinin tamamı (son tur) aynı sonuç.

**10b**
- F3 aç/kapa; kapanıp açılınca sağ bölme eski sekmeleriyle; `state.toml` gidiş-dönüş (eski dosya → sol bölme).
- **Kapatma bırakır:** sağ bölme kapanınca izleyici sayısı 1, sağ bölmenin `View`/`Navigator` `Rc`'leri düşmüş (zayıf tutamak testi), bellek tek bölmeli değere ±0,1 MB (`panes.ps1`).
- **Tek izleyici:** iki bölme açıkken izleyici sayısı 1; etkin olmayan bölmenin klasörüne dışarıdan dosya eklenince bölme değişmez, Tab ile etkin olunca yeni dosya görünür (seçim ve kaydırma korunur); F5 hedefi olan etkin olmayan bölme iş bitince hemen yenilenir.
- F3 geçiş ipucu: ilk açılışta bir kez; `[shortcuts]`'ta `search` yazılıysa hiç; `[hints] f3-moved` sonrası çıkmaz.
- Bölme kapalıyken F5 yeniler, F6 bir şey yapmaz; açıkken F5 kopyalar, F6 taşır, Ctrl+R yeniler, Ctrl+E arar (kısayol birim testleri: her iki bağlamda `action_in`; §10.2'deki değişen testler).
- F5: soru, hedef alanına göreli yol, olmayan klasörün oluşması, `Don't ask again`; çakışma listesi; Ctrl+Z; hedef This PC/arama/çöp iken sözler; aynı klasör sözü; çöpten F6 geri yükleme gibi.
- Tab yalnız iki bölmede ve liste odaktayken bölme değiştirir; tek bölmede ve yazma alanlarında bugünkü gibi; harfle atlama sıfırlanır; bölme değişimi erişilebilirlik etiketiyle söylenir.
- Sekme menüsü ve sürükleme ile öbür bölmeye taşıma (geçmiş, kilit, seçim korunur; son sekme boşluğu).
- Bölmeler arası sürükle-bırak: aynı sürücü taşır, farklı sürücü kopyalar, Shift/Ctrl değiştirir; dışarıdan bırakma imlecin bölmesine.
- Eşli gezinme: alt klasör ve üst klasör eşlenir; olmayan ad, geri/ileri, kenar çubuğu eşlemeyi kapatır ve söyler (saf `sync_nav` birim testleri: Windows harf duyarsız, Linux duyarlı).
- `swap-panes`, `other-pane-same-folder`; sekme seti `right = [...]`.
- Önizleme, Hızlı Bakış, Bilgi, palet, durum satırı etkin bölmeyi izler (§4.10 tablosundaki her satır için bir ekran testi maddesi).
- Bütçe satırı; `panes.ps1`.

**10c**
- Ağaç düğümlerinin saf testleri: açma/kapama, düz satır listesi, girinti, 20.000 kesme satırı, gizli klasör ayarı, Gezik işinin `Outcome`'undan yenileme.
- Açılışta okuma yok (bir sayaçla birim testi: ağaç kapalıyken `list_dir` çağrısı 0).
- 50.000 alt klasörlü dal (geçici ağaç) açılırken en uzun kare ≤ 50 ms; kaydırma CPU'su (§5.4).
- Ağaç satırına bırakma, orta tık, sağ tık menüsü; `tree-follow` açık/kapalı; `reveal-in-tree`.

**10d**
- Kova sınırları (saf): tarih kovaları (gün, hafta başı yerel ayara göre Pazartesi, ay, yıl dönümleri, saat dilimi), boyut kovaları (sınır değerleri), tür grupları, ters sıralama.
- Klavye başlıkları atlar; kapalı grup `Select All`'a girmez; harfle atlama grubu açar; lastik bant başlık satırından geçmez.
- 100.000 öğede gruplu sıralama ≤ 1,2× (`stress.ps1`'e eklenir); `views.toml` `group` gidiş-dönüş.

**10e**
- Saf sütun yolu: → / ← / Up / adres değişimi; geçmişe yazmama; bellek kuralı (görünen + 8 ata).
- Önizleme sütunu dosyada, klasörde yok; önizleme bölmesi açıkken yok.
- 30 derinlikte bellek ölçümü (§1); arama/çöp/This PC'de liste kipine düşme.
- macOS'ta ⌃⌘3, Windows'ta Ctrl+Shift+3.

**10f**
- Ayrıştırma ve uyarılar; glob (Windows harf duyarsız, `**`, belirteçler); `kind` her değer; `content` oranı (sınırda %50); `mode = "columns"` 10e+'ya kadar uyarıyla atlanır.
- Glob'lar ayar yüklenirken bir kez derlenir (klasör açılışında derleme sayacı 0); ilk eşleşmede çıkış (sonraki kurallar değerlendirilmez, sayaçla); sahte dosya sistemiyle değerlendirmede dosya sistemi çağrısı 0.
- Başarım: 20 kural, 100.000 öğe: değerlendirme ≤ 50 µs, `content` sayımı ≤ 2 ms (`rules.ps1`).
- Öncelik: bellek > kural > varsayılan; kullanıcı değişikliği kuralı o klasör için kapatır; `Reset to Rule`.
- İlk karenin doğru kipte çizilmesi (`content` kuralında da: kip değişimi ilk `show`'dan önce; birim testi `View::show` sırasıyla).
- Kural yokken ayrılan bellek 0.

**10g**
- Yoklama notu (§9.2), özellikle macOS menü çubuğu sonucu ve seçilen yol.
- Ctrl+N aynı süreçte (süreç sayısı testi); tema ve ayar yeniden yüklemesi her pencereye (§9.3'ün listesi: iki pencerede `Theme` değerleri ve kök özellikleri karşılaştırılır).
- Olay yönlendirici: iki pencerede odak sırası doğru; kapanan pencerenin olayı düşer.
- **Kapanınca arabellek bırakılır:** ikinci pencere açılıp kapanınca özel bellek taban + 0,3 MB (`windows.ps1`, açık test).
- Son pencere kapanınca çıkış; tepsi açıkken 9b9 kuralı; kapanan penceredeki işin sorusu odaktakine.
- Tek örnek isteği en son odaklanan pencerede; `--new-window` çalışan Gezik'te pencere açar; **asılı örnek geri düşüşü:** yanıt vermeyen sahte dinleyiciyle gönderen 2 sn sonra yeni süreçte kendi penceresini açar (bayraksız ve `--new-window` ile).
- `--help` metni, README ve 9. spec notu güncel.
- Sekmeyi menüyle yeni ve başka pencereye (üç sistem); sürükleyerek dışarı ve başka pencerenin şeridine (Windows, macOS); Linux'ta dışarı sürükleme iptal olur, sekme yerinde kalır.
- Pencereler arası dosya sürükleme (aynı süreç, üç sistem).
- `[[session.window]]` gidiş-dönüş; 3 pencereyle açılış.

## 14. Test

### 14.1 Birim (saf; her sistemde)

`sync_nav` (eşleme kararları), `tree` (düğümler, satırlar, kesme), `group` (kovalar), `columns` (sütun yolu, bırakma), `view_rules` (ayrıştırma, glob, içerik oranı, öncelik), `shortcuts` (bağlama bağlı tuş, yeni varsayılanlar, çakışmasızlık, F3/Ctrl+E/Ctrl+R değişimi ve kullanıcı ayarının kazanması), `drag::hit` (iki bölmeli yerleşim, bölme ayracı `Nothing`), `state.toml` oturumu (eski biçim, sağ bölme, çok pencere), sekme setinde `right`.

### 14.2 Platform

- Windows: aynı süreçte pencereler arası OLE sürükle-bırak; sekme koparmada pencere dışı yakalama.
- macOS: ikinci `AppWindow`'un menü çubuğu; sekme koparma; ⌃⌘P/⌃⌘U/⌃⌘3'ün sisteme takılmadığı.
- Linux: X11 ve Wayland'de menüyle sekme taşıma, dışarı sürüklemenin iptali, pencereler arası dosya sürükleme.

### 14.3 Çapraz denetim

Planın sonunda (her görevde değil): `cargo check` macOS ve Linux hedefleri, Windows `clippy`/`fmt`/`test`. Docker yok.

### 14.4 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken; klavye düzenine dokunulmaz): §13.1'in ekran maddeleri, 150 ve 200 % ölçekte iki bölme, dar pencere, F5 ile ağ sürücüsüne kopya, iki bölmede iki arama, ağaçta `\\sunucu`, OneDrive köklerinde ağaç ve gruplama, çöpte gruplama, sütun kipinde arşiv klasörü.
- **macOS** (`macos-test.md`'ye eklenir, sonuçlar `macos-test-results.md`'ye): ⌃⌘P, fn+F5/F6, Tab, sütun kipi ve önizleme sütunu (Finder ile karşılaştırma), Retina'da ikinci pencere belleği, menü çubuğunun odaktaki pencereye gitmesi, sekme koparma, iCloud'da ağaç.
- **Linux** (gerçek makine, GNOME ve KDE, X11 ve Wayland): F3, F5/F6, ağaç (`/` ve ev), menüyle sekme taşıma (`Move to New Window`, `Move to Window ▸`), pencereler arası dosya sürükleme.

## 15. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| 10a'nın genişliği: 22 tekil, 35 dosyada ~279 `with_current` çağrısı ve `app.slint`'in yarısı; gizli bir "tek liste" varsayımı kalır | Yanlış bölmede eylem, kaybolan sonuç | Dört dilim (yoklama, mekanik sarma, süreç tekilleri, `PaneView`), her biri görünür değişikliksiz; bütün Windows ekran listesi 10a-3'te yeniden; `PaneId` sonuç türlerinde zorunlu alan |
| 9. adımın açık dallarıyla çakışma (`main.rs`, `actions.rs`, `context_menu.rs`, `navigation.rs`) | Uzun birleştirmeler, kaybolan düzeltmeler | 10d ve 10c önce; 10a-1…10a-3 9. adımın dalları birleştikten sonra |
| `PaneView` iç durumlu bileşen: iki yönlü bağlar `[PaneData]` modelinden geçmez | Kaydırma geri yükleme, ad alanı, adres yazma kipi bozulur | Bu durum bileşenin içinde kalır, Rust'a geri çağrı ve sayaçlı istek alanlarıyla açılır; 10a-0 yoklaması bunu ölçer; gerekirse tek alan kök özelliğe düşer |
| F3'ün anlamının değişmesi (8a'da arama; kullanıcının seçimi) | Explorer alışkanlığı kırılır | Ctrl+E (Explorer'ın asıl arama tuşu) ve Ctrl+Shift+F kalır; kullanıcı `search = "f3"` yazarsa onun seçimi kazanır; README, şablon yorumu ve ilk açılışta tek seferlik durum satırı ipucu (§10.2) |
| Etkin olmayan bölmenin izlenmemesi | Dışarıdan yapılan değişiklik o bölmede görünmez | Bölme etkin olunca yeniden okunur; Gezik'in işleri dokunduğu bölmeyi hemen yeniler; §16'da sınır olarak yazılı |
| Bağlama bağlı tuş (F5) kullanıcıyı şaşırtır | İstemeden kopya | Varsayılan soru (`[panes] confirm = true`), Ctrl+Z, iş paneli |
| Yazılım çizicisinde çok pencere | Pencere başına 4–16 MB | Kullanıcı eylemi; ölçüm ve not; gizli pencere arabelleği 9b9 ölçümüyle |
| Sekme koparmada pencere dışı fare yakalaması | Özellik bir sistemde yarım | Sürükleme yalnız Windows ve macOS'ta (yoklamayla); menü yolu üç sistemde |
| macOS tek menü çubuğu, birden çok `MenuBar` (doğrulanmadı) | Komut yanlış pencereye | 10g'nin ilk görevi yoklama; olmazsa tek `MenuBar` ve `menu-command` metin yolu `with_focused`'a |
| `--new-window`'un anlamının değişmesi | Betiklerde ayrı süreç bekleyen kullanım | `--help`, README ve 9. spec notu; `single-instance = false` ayrı süreç verir; asılı örnek geri düşüşü testle korunur |
| Aynı süreçte kendine OLE bırakma (Windows) | Pencereler arası dosya sürüklemesi kilitlenir | Yoklama; olmazsa pencereler arası bırakma Gezik'in kendi iç sürüklemesiyle (aynı süreç, yakalama yolu) |
| Miller sütununda yatay kaydırma bütün alanı çizer | Sütun açılışı başına tam kare | Canlandırma yok; sütun başına sanal liste |
| Ağaçta çok büyük dal, ağ yolu | Uzun okuma | Arka plan, dönen işaret, 20.000 kesme, ağ yolunda bugünkü zaman aşımı |
| Kural ile görünüm belleği çatışması | "Neden bu klasör ızgarada?" | `View rule N applies` ve `Reset to Rule` |

## 16. Bilinen sınırlar

- Yalnız iki bölme, yalnız yan yana.
- Eşli gezinme yalnız göreli adımları eşler; mutlak sıçrama eşlemeyi kapatır (yeniden açmak tek tık).
- Etkin olmayan bölme izlenmez: başka bir uygulamanın o bölmenin klasörüne yaptığı değişiklik bölme etkin olana kadar görünmez (Gezik'in kendi işleri hemen yeniler).
- Ağaç açık dalları izlemez: başka bir uygulamanın oluşturduğu klasör, dal yeniden açılana ya da bir bölme o klasörü gösterene kadar görünmez. Açık dallar oturumda saklanmaz.
- Gruplama sütun kipinde yok; etiket grubu 12. adımda.
- Sütun kipinde boyut/tarih sütunu ve küçük resim yok; arama, çöp ve This PC sütun kipinde gösterilmez.
- Kurallar yalnız görünümü değiştirir (renk, komut yok); `content` kuralı yalnız adlara (uzantılara) bakar.
- Linux'ta (X11 ve Wayland) sekme sürükleyerek koparılmaz; menüyle taşınır.
- macOS'ta pencere başına menü çubuğu yoklamaya bağlıdır; olmazsa tek menü odaktaki pencereye yönlendirilir.
- Ek pencereler yazılım çizicisinin arabelleği kadar bellek tutar; GPU çizici yoktur.
- Durum satırı tektir (etkin bölmenin); bölme başına bilgi satırı yok.

## 17. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 10d, 10c, 10a-0…10a-3, 10b, 10e, 10f, 10g** (§13; mimari inceleme kararı). *Gerekçe:* gruplama ve ağaç bölme gerektirmez ve 9. adımın açık dallarının değiştirdiği dosyalara en az dokunur; çift panel ve çok pencere aynı önkoşulu paylaşır (22 tekilin, ~279 çağrının bölme/pencereye ayrılması); bunu dört görünür değişikliksiz dilimde yapmak hatayı özellikten ayırır; 10a-1…10a-3 9. adımın dalları birleştikten sonra başlar. Çok pencere en çok bilinmeyeni taşır.
2. **Üç düzey: bölme, pencere, süreç** (§3.1). *Gerekçe:* her parçanın nereye ait olduğu bir tabloda karar verilince iki bölme ve çok pencere aynı kodla çözülür; iş motoru ve geri alma süreçte kalınca pencereler arası taşıma doğal olur.
3. **`PaneView` iç durumlu bir Slint bileşeni; Rust'a yalnız `[PaneData]` modeli ve bölme numaralı geri çağrılarla açılır; kaydırma ve ad metni geri çağrı + sayaçlı istek** (§3.2; mimari inceleme kararı). *Gerekçe:* anlık arayüz durumu Slint'te kalınca Rust yüzeyi küçülür; `build.rs` ile üretilmiş `p0-`/`p1-` takımları `app.slint`'i ikiye katlar ve üçüncü bir kopyaya (pencereler) ölçeklenmez. Kök özelliğe düşmek yalnız 10a-0 yoklaması bir alanın modelden geçemediğini gösterirse.
4. **Her bölme tam bir gezgin** (kendi sekmeleri, adresi, görünümü); kenar çubuğu, önizleme, durum satırı pencereye ait. *Gerekçe:* 8 rakibin hepsi böyle (PF, FL, TC); tek kenar çubuğu ve tek önizleme alanı ve belleği korur.
5. **Çift panel kapalıyken sağ bölmenin yalnız yerleri saklanır** (§4.2). *Gerekçe:* FL 4.3.3 gibi geri açınca aynı yer; listeleme ve izleyici tutulmaz (boşta hiçbir şey).
6. **Aç/kapa: Windows/Linux F3, macOS ⌃⌘P; arama Ctrl+E alır, Ctrl+Shift+F kalır** (§10.2). *Gerekçe:* kullanıcı F3'ü önerdi; Explorer'da arama tuşu Ctrl+E/Ctrl+F'tir, F3 eski bir eş anlamlı; macOS'ta F3 Mission Control'ündür, ⌥⌘D Dock'u, ⌃⌘D sözlüğü alır, ⌃⌘P boştur ("pane").
7. **F5/F6 bağlama bağlı; tek bölmede F5 yeniler, F6 boş; Ctrl+R ikinci yenile tuşu** (§4.4). *Gerekçe:* kullanıcı kararı 3; Ctrl+R Explorer'ın da yenileme tuşudur, iki bölmede yenilemeyi ulaşılır tutar.
8. **macOS'ta da F5/F6** (fn ile), ayrıca menü ve palet. *Gerekçe:* Commander One ve TC alışkanlığı; macOS'ta F5/F6 Gezik'te boş; ⌘-harf kombinasyonları Finder'ınkilerle çakışır.
9. **Tab bölme değiştirir, yalnız iki bölmede ve liste odaktayken** (§4.3). *Gerekçe:* TC/Far/OC'nin evrensel tuşu; erişilebilirlik: klavyeyle denetimler arasında dolaşan kullanıcının Tab yolu tek bölmede hiç, iki bölmede yalnız listede değişir (Shift+Tab, Ctrl+L, Esc kalır; bölme değişimi ekran okuyucuya söylenir); yazma alanlarında Tab alanın.
10. **F5/F6 varsayılan sorar; hedef düzenlenebilir; `[panes] confirm`** (§4.4). *Gerekçe:* Explorer kullanıcısı F5'i yenile bilir; iki bölmede yanlışlıkla büyük kopya başlatmamak için bir Enter yeterince ucuz; Far'ın düzenlenebilir hedefi tek alanla gelir.
11. **F5/F6 sürükle-bırakın yolundan geçer** (çakışma listesi, geri alma, çöp kuralı). *Gerekçe:* ikinci bir kopyalama yolu yazılmaz; davranış tutarlı.
12. **Sekme öbür bölmeye menüyle ve sürükleyerek; bölme boş kalmaz** (§4.5). *Gerekçe:* FL 4.7.6, Files ve OC; boş bölme anlamsız bir durum olurdu.
13. **Eşli gezinme yalnız göreli adımları eşler, uyuşmazlıkta açık sözle kapanır** (§4.7). *Gerekçe:* mutlak sıçramada "öbür taraf nereye gitmeli" belirsizdir; TC askıya alır ama sessizce; açık söz daha anlaşılır, yeniden açmak tek tık. DO'nun yol eşlemeli Paired Folders'ı yok (YAGNI).
14. **`swap-panes` Ctrl+U / ⌃⌘U ve `other-pane-same-folder`** (§4.8). *Gerekçe:* TC ve Far'ın temel iki komutu, ucuz.
15. **Ağaç ayrı bölüm değil, kenar çubuğu yer satırlarının açılır dalları** (§5.1). *Gerekçe:* Explorer'ın gezinme bölmesi gibi; ek başlık ve yer kaplamaz; kökler zaten kullanıcının yerleri.
16. **Ağaç izlemez, açık dallar saklanmaz, açılışta okuma yok** (§5.2). *Gerekçe:* kullanıcı kararı "boşta hiçbir şey"; yenileme Gezik'in işleri ve bölmelerin listelemelerinden bedava gelir.
17. **Kenar çubuğu pencereli modele geçer; dal başına 20.000 kesme** (§5.2). *Gerekçe:* bugünkü `for` her satır için öğe kurar; büyük dal belleği ve çizimi patlatırdı.
18. **`tree-follow` varsayılan kapalı + tek seferlik `reveal-in-tree`** (§5.3). *Gerekçe:* her gezinmede ata okumaları maliyettir; Explorer'da da varsayılan kapalı.
19. **Gruplar: tür, tarih, boyut; Explorer'ın kovaları; etiket 12. adımda** (§6.1). *Gerekçe:* kullanıcı kararı; Explorer kovaları Windows kullanıcısının bildiği adlar; sabit kovalar ayar gerektirmez.
20. **Grup kapanır; seçim yalnız görünenler** (§6.2). *Gerekçe:* DO ve FP'de var; kapalı gruptaki öğeyi seçip silmek sürpriz olurdu.
21. **Grup anahtarı sıralamayla aynı geçişte; ek G/Ç yok** (§6.3). *Gerekçe:* hafiflik; 100.000 öğede ≤ 1,2×.
22. **Miller üçüncü görünüm kipi; Windows/Linux Ctrl+Shift+3, macOS ⌃⌘3** (§7.1). *Gerekçe:* Finder'ın ⌘3'ü Gezik'te Tab3, ⇧⌘3 sistemin ekran görüntüsü; ⌃⌘3 boş ve ⇧⌘1/⇧⌘2 dizisini sürdürür.
23. **Sütunlar arasında gezinmek geçmişe yazmaz; odaktaki sütun sekmenin konumu** (§7.1). *Gerekçe:* her ok tuşu bir geçmiş girdisi olsaydı Geri anlamsızlaşırdı; adres çubuğu yine doğru yeri söyler.
24. **Sütun belleği: görünenler + 8 ata; izleyici yalnız odaktaki sütunda; canlandırmasız kaydırma** (§7.2). *Gerekçe:* derin gezinmede bellek sınırlı; yazılım çizicisinde canlandırma her karede tam alan çizerdi.
25. **Önizleme sütunu, önizleme bölmesi kapalıyken** (§7.1). *Gerekçe:* PF ve Finder'ın sütun görünümü bunu bekletir; ikisi birden aynı şeyi iki kez çizer.
26. **Sütun kipinde gruplama, başlık sütunları, küçük resim yok; arama/çöp/This PC sütun kipine girmez** (§7.3). *Gerekçe:* Finder da böyle; bu yerlerin klasör zinciri yoktur.
27. **Kurallar `settings.toml`'da `[[view-rules]]`, `views.toml`'da değil** (§8.1). *Gerekçe:* `views.toml` Gezik'in yazdığı makineye özgü dosya; kurallar kullanıcının yazdığı, taşınabilir ayardır (`[[commands]]`, `[[tab-sets]]` gibi).
28. **Öncelik: klasör belleği > kural > varsayılan** (§8.2). *Gerekçe:* kullanıcının o klasörde elle yaptığı değişiklik en son ve en belirli niyettir; TC notunun "kural önce" önerisi bu yüzden ters çevrildi; `Reset to Rule` kuralı geri getirir.
29. **`content` kuralı yalnız adlardan, ilk çizimden önce** (§8.2). *Gerekçe:* ek G/Ç yok; kip değişimi titreme yaratmaz.
30. **Kurallar yalnız görünüm: kip, sıralama, grup, ızgara boyutu, sütunlar** (§8.1). *Gerekçe:* TC'nin renk ve otomatik komutu YAGNI; komut çalıştıran bir kural güvenlik yüzeyi açar.
31. **Yeni pencere aynı süreçte; 9. spec §5.3 ve karar 9'un yerine geçer; `--new-window` çalışan Gezik'te pencere açar** (§9.1, §9.3). *Gerekçe:* kullanıcı kararı (sekme koparma tek süreç ister); ortak iş paneli ve geri alma; ayrı süreç isteyen tek örneği kapatabilir. Anlam değişikliği `--help`, README ve 9. spec notunda yazılır; 9b1'in asılı örnek geri düşüşü (yeni süreç) korunur ve testlidir.
32. **Tek örnek istekleri en son odaklanan pencereye** (§9.3). *Gerekçe:* kullanıcının o an baktığı pencere; Explorer ve Finder da böyle davranır.
33. **Sekme koparma Gezik'in kendi fare yakalamasıyla, yalnız Windows ve macOS'ta; Linux (X11 ve Wayland) menü yolu; menü yolu üç sistemde** (§9.4, §9.5; mimari inceleme kararı). *Gerekçe:* sekme dosya değildir, sistem sürüklemesi başka uygulamalara yanlış veri sunar; Wayland genel imleç konumunu vermez ve pencere konumlandırmaya izin vermez, X11'e özel bir davranış ekran testlerini böler.
34. **Bırakma yığını içeriği, iş motoru, geri alma, `Media`, `ViewMemory`, Hızlı Bakış süreçte tek; katmanlar pencerede** (§3.1, §9.6). *Gerekçe:* bir pencereye toplanan dosyalar başka pencerede bırakılabilmeli; Ctrl+Z tek geçmişi geri alır (Explorer gibi); simge önbelleği iki kez tutulmaz.
35. **Çok pencereli oturum geri yüklenir** (§9.7). *Gerekçe:* kullanıcı `[session] restore` ile oturumu istedi; maliyet kullanıcının açık bıraktığı pencerelerdir ve bütçede yazılıdır.
36. **Sütun düzeni bölmelerde ortak** (§3.3). *Gerekçe:* `state.toml` biçimi değişmez; bölme başına sütun genişliği iki kat ayar ve şaşkınlık getirirdi; klasöre özgü sütun isteyen kural yazar.
37. **Etkin olmayan bölmenin seçimi %45 saydam; yeni tema anahtarı yok** (§4.3). *Gerekçe:* FP'nin soluk bölme fikrinin hafif hâli; temalar (D1–D4) değişmeden çalışır.
38. **Tek durum satırı** (§4.10). *Gerekçe:* yer ve sadelik; etkin bölmeyi izler.
39. **Menü kimlikleri 2000–2199; `Action` 85 → 100** (§10). *Gerekçe:* 9b 1800–1999'da biter; her parça kendi aralığında.
40. **Bütçe: her parça ≤ +262.144 B; 10a dilimlerinde boşta 0; varsayılan ayarlarla boşta ≤ +0,1 MB toplam (hesap ≤ +0,07), açılış ≤ +2 ms; ek pencere çerçeve arabelleği + 2,5 MB ve kapanınca bırakılır** (§12; mimari inceleme kararı). *Gerekçe:* önceki adımların kuralı; mekanik bir yeniden yapılanma bellek eklememeli; çok pencerenin kaçınılmaz maliyeti açıkça yazılır, bırakılması açık testle denetlenir.
41. **Etkin olmayan bölme izlenmez; etkin olunca arka planda yeniden okunur; Gezik'in işleri dokunduğu bölmeyi hemen yeniler** (§4.3; mimari inceleme kararı). *Gerekçe:* boşta tek izleyici (hafiflik); görünen bedel yalnız başka bir uygulamanın değişikliğinin geç görünmesi; yeniden okuma gösterilen listeyi, seçimi ve kaydırmayı yerinde tutar.
42. **Sağ bölme kapanınca listeleme, model, izleyici, klasör boyutu işleri bırakılır ve bu açık testle denetlenir** (§4.2). *Gerekçe:* "kapalı = maliyetsiz" ölçütünün kanıtı; zayıf tutamak testi sızıntıyı yakalar.
43. **Kurallar: glob'lar ayar yüklenirken derlenir, ilk eşleşmede çıkış, koşullar ucuzdan pahalıya, dosya sistemi çağrısı yok; 20 kural için başarım testi; `mode = "columns"` 10e+'da** (§8.3; mimari inceleme kararı). *Gerekçe:* her klasör açılışında çalışan kodun maliyeti sınırlı ve ölçülü olmalı; 10f'nin 10e'ye bağımlılığı kalkar, kurallar Miller'i beklemeden gelir.
44. **Bağlama bağlı arama `Shortcuts::action_in(&chord, KeyContext)`; `action_for` tek bölmeli sarmalayıcı** (§4.4, §10.2). *Gerekçe:* bugünkü `action_for` ilk eşleşen bağı döner, aynı tuşta iki eylemi ayıramaz; sarmalayıcı çağıranların çoğunu ve testlerin çoğunu değiştirmeden bırakır; değişen testler §10.2'de sayılı.
45. **F3 kullanıcının seçimiyle çift panelin; geçiş notu README'de, şablonda ve ilk açılışta tek seferlik durum satırı ipucunda** (§10.2). *Gerekçe:* 8a'dan beri F3 ile arayan kullanıcı ne olduğunu bir kez, yerinde öğrenmeli; ipucu zamanlayıcısız ve `state.toml`'da bir işaretle bir kez.
46. **Çok pencerede her pencereye yeniden uygulananlar tek işlevden (`apply_to_window`): `Theme` global'i ve `reduce-motion`, `apply_config`'in kök özellikleri; odak winit olay yönlendiricisinden; macOS menü çubuğu önce yoklanır** (§9.2, §9.3; mimari inceleme kararı). *Gerekçe:* Slint global'leri bileşen örneğine özeldir, unutulan bir değer yalnız ikinci pencerede görünen bir hata olur; tek yönlendirici bugünkü tek pencere kancasının yerini alır; macOS'ta pencere başına `MenuBar`'ın davranışı doğrulanmadan plan kurulmaz.
