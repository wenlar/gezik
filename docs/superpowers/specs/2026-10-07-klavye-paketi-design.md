# Alt Proje 6: Klavye Paketi — Tasarım

- **Tarih:** 2026-10-07
- **Durum:** Tasarım onaylandı (2026-10-07); 6a uygulandı
- **Kapsam:** Gezik yol haritasının 6. alt projesi (yeni sıra: `docs/superpowers/notes/2026-10-07-rakip-ozet.md` §2); tek spec, iki plan ve iki PR: **6a** süzgeç, seçim ve sekmeler; **6b** yol tamamlama, klasör geçmişi ve kullanıcı komutları
- **Dayandığı:** `2026-10-04-gezinme-design.md` (sekmeler, geçmiş, adres çubuğu, harfle atlama), `2026-10-04-gorunum-design.md` (liste modeli, seçim, sıralama), `2026-10-03-ayarlar-ve-tema-design.md` (ayar dosyası, kısayol biçimi, canlı yeniden yükleme), `2026-10-05-toplu-islemler-design.md` §7 (`[[commands]]`)

## 1. Amaç

İncelenen sekiz dosya yöneticisinin hepsinde olan, Gezik'te ise olmayan ya da yarım kalan klavye işlerini tamamlamak: listeyi yazarak süzmek, adres çubuğunda yol tamamlamak ve sık gidilen klasörlere hızla dönmek, sekmeler arasında numarayla geçmek ve kapatılanı geri açmak, desenle seçmek, kullanıcı komutlarını kısayolla çalıştırmak. Hepsi ucuz, günlük kullanımda hemen hissedilen işlerdir.

### Başarı ölçütleri

- 100.000 dosyalık klasörde süzgeç her tuş vuruşundan sonra ≤ 30 ms'de güncellenir (sürüm derlemesi).
- Süzgeç açıkken seçim, harfle atlama, sıralama, kopyala/taşı/sil, sürükle-bırak ve yeniden adlandırma yalnız görünen öğeler üzerinde doğru çalışır.
- Yol tamamlama önerileri yazmaya ara verildikten sonra ≤ 100 ms'de gelir; ağ yolu ya da yavaş disk arayüzü dondurmaz.
- Kapatılan sekme geçmişiyle (geri/ileri) geri gelir.
- Boşta bellek değişmez (≤ 7 MB); exe büyümesi 6a + 6b ≤ +0,5 MB.
- Her yeni eylem `[shortcuts]` ile değiştirilebilir ve macOS menü çubuğunda görünür.

### Kapsam dışı (bilerek)

- Oturum geri yükleme (açılışta önceki sekmeler): Gezinme spec'inde bilerek dışarıda bırakıldı; 7. adımda (Günlük kolaylıklar) yeniden değerlendirilir.
- Bulanık (fuzzy) eşleşme.
- Alt klasörlerde arama ve düz görünüm: 8. adım (Arama).
- Kilitli sekmelerin ve kapatılan sekme listesinin kalıcı olması (oturum geri yükleme olmadığı için).

## 2. Alınan kararlar

| Konu | Karar |
|---|---|
| Yazınca ne olur | Ayarla seçilir: `[keyboard] typing = "jump"` (varsayılan, bugünkü harfle atlama) ya da `"filter"` (harf yazmak süzgeci açar) |
| Süzgeç eşleşmesi | Adın içinde geçen metin; `*` ve `?` joker; `;` ile birden çok desen; `!` ile hariç; büyük/küçük harf ve Türkçe i/İ ayrımı yok |
| Sekme numaraları | Ctrl+1…8 n'inci sekme, Ctrl+9 son sekme (macOS'ta ⌘); görünüm kısayolları Ctrl+Shift+1/2'ye taşınır (Görünüm spec'i §8.4'teki ilk karar) |
| Ekler | Kayıtlı süzgeçler, kilitli sekmeler, çok ziyaret edilen klasörler, komut onayı (`ask`) ve tüm seçimi geçen `{files}` |
| Bölme | 6a: süzgeç + seçim + sekmeler; 6b: yol tamamlama + geçmiş + komutlar |

## 3. 6a — Süzgeç

### 3.1 Davranış

- **Açma:** `filter` eylemi (Ctrl+F / ⌘F ve `/`). `typing = "filter"` iken liste odaktayken basılan yazılabilir bir karakter süzgeci açar ve o karakterle başlatır. `typing = "jump"` iken harfler bugünkü gibi ada atlar.
- **Çubuk:** dosya listesinin üstünde tek satır: metin alanı, sağında "12 / 340" sayacı ve bir menü düğmesi (kayıtlı süzgeçler). Görünüm temadan gelir; hatalı desende alan altı kırmızı ince çizgi ve açıklama ipucu.
- **Süzme:** her tuş vuruşunda (gecikme yok; 100.000 dosyada bütçe 30 ms) liste daralır. Seçim süzgeç değişince görünmeyen öğelerden temizlenir; odak görünen ilk öğeye geçer.
- **Klavye:** ↓ veya Enter odağı listeye verir (çubuk açık kalır); listede Esc önce süzgeci kapatır (ikinci Esc seçimi temizler); çubukta Esc süzgeci kapatıp tam listeye döner. Ctrl+F çubuk açıkken metni seçer.
- **Kapanma:** başka klasöre geçmek, sekmeyi değiştirmek (her sekmenin kendi süzgeci vardır, geri gelince süzgeç durur), Esc. Yenileme (dosya izleyici) süzgeci korur; ekrandaki klasöre yeniden gitmek (kendi adres parçasına ya da kenar çubuğu girdisine tıklamak, aynı yolu yazmak) da yenileme sayılır ve süzgeci korur. Yeni klasör ve yeni dosya, yeniden adlandırma başlamadan önce süzgeci kapatır; yapıştırma, bırakma, açma ya da dönüştürmenin süzgeçle gizlenen yeni öğeleri için süzgeç durur, durum çubuğu "N items hidden by the filter" der.
- **Gizli dosyalar:** süzgeç, gizli dosya süzgecinden sonra uygulanır.

### 3.2 Desen dili (`gezik-core::pattern`)

- Desen `;` ile parçalara ayrılır; boş parçalar atlanır. `!` ile başlayan parçalar hariç tutar.
- Joker içermeyen parça "adın içinde geçer" demektir. `*` veya `?` içeren parça bütün adla eşleşmelidir (`*.jpg` yalnız `.jpg` ile biteni bulur).
- Bir ad: içerme parçalarından en az birine uyuyorsa (içerme parçası yoksa herkes uyar) ve hiçbir hariç parçasına uymuyorsa görünür.
- Karşılaştırma büyük/küçük harf duyarsızdır; Türkçe i/İ/ı/I 5a'daki kurallarla eşlenir.
- `[` `]` gibi karakterler düz karakterdir (joker yalnız `*` ve `?`).
- Saf, ayırma yapmayan bir eşleyici; desen bir kez derlenir, ad başına ayırma olmaz.

### 3.3 Kayıtlı süzgeçler

```toml
[[filters]]
name = "Resimler"
pattern = "*.jpg;*.jpeg;*.png;*.heic;*.webp"
```

- Çubuktaki menü düğmesi kayıtlı süzgeçleri listeler; seçmek deseni alana yazar. "Save as…" o anki deseni bir ad sorarak `settings.toml`'a ekler (5a'daki hazır ayar yazma yolu); "Delete" siler.
- Hatalı girişler (`name` ya da `pattern` boş, yinelenen ad) uyarıyla atlanır.

### 3.4 Uygulama yeri

- Süzgeç, görünüm katmanında gizli dosya süzgecinin yanına eklenir (`View::show` ve `resort` yolu): `Listing` süzülmüş olarak kurulur. Böylece sıra numarasıyla çalışan her şey (`Selection`, `find_prefix`, `path_at`, sürükle-bırak, bağlam menüsü) değişmeden doğru çalışır.
- Süzgeç metni her sekmenin görünüm durumunda tutulur; `state.toml`'a yazılmaz.

## 4. 6a — Seçim

| Eylem | Varsayılan | Ne yapar |
|---|---|---|
| `invert-selection` | Ctrl+Shift+I (⌘⇧I) | Görünen öğelerde seçimi ters çevirir |
| `select-pattern` | numerik `+`, Ctrl+= | Desen penceresi; desene uyan görünen öğeleri seçime ekler |
| `deselect-pattern` | numerik `−`, Ctrl+- | Aynı pencere; uyanları seçimden çıkarır |
| `select-same-type` | Alt+numerik `+` | Odaktaki öğenin uzantısındaki bütün görünen dosyaları seçer (klasörde klasörleri) |
| `restore-selection` | numerik `/` | Son dosya işleminden (kopyala, taşı, sil, yeniden adlandır, dönüştür) önceki seçimi geri getirir; o klasörde değilse hiçbir şey yapmaz |

- Desen penceresi süzgeçle aynı desen dilini kullanır, son deseni hatırlar (`state.toml [selection] last_pattern`), canlı olarak "N items match" yazar.
- `Selection`'a `invert` eklenir (değişen aralıkları döndürür, bugünkü kalıpla).
- Kısayol ayrıştırıcısına numerik tuş adları eklenir: `num+`, `num-`, `num*`, `num/` (Slint'in tuş kodlarından; platform başına doğrulanır). Ctrl+= ve Ctrl+- yakınlaştırma için kullanılmıyorsa varsayılandır; çakışma varsa plan bunu çözer.

## 5. 6a — Sekmeler

- **Numarayla geçiş:** `tab-1` … `tab-8` (Ctrl+1…8), `tab-last` (Ctrl+9). Olmayan numara bir şey yapmaz. `view-list` ve `view-grid` Ctrl+Shift+1/2'ye taşınır; ayar dosyasında bunları elle bağlamış olanların bağlamaları aynen çalışır (bugünkü çakışma kuralı: kullanıcı bağlaması varsayılanı gölgeler).
- **Kapatılanı geri aç:** `reopen-tab` (Ctrl+Shift+T). Gezinme modeli kapatılan son 20 sekmeyi (klasör, geri/ileri geçmişi, görünüm durumu) bir yığında tutar; geri açılan sekme kapatıldığı konuma, yoksa sona eklenir. Pencere kapanınca yığın gider.
- **Sekme seçici:** `tab-picker` (Ctrl+Shift+A). Açık sekmeleri ad ve yolla listeleyen küçük bir katman; yazmak süzer (desen dili), Enter geçer, Esc kapatır.
- **Kilitli sekme:** sekmenin sağ tık menüsünde "Lock tab" / "Unlock tab"; `toggle-tab-lock` eylemi (varsayılan bağlama yok). Kilitli sekmede başlık önünde kilit simgesi görünür; `close-tab`, "Close other tabs" ve orta tık onu kapatmaz (bunu bir an için durum çubuğunda söyler). İçinde gezinmek serbesttir. Kilit oturum boyunca geçerlidir.

## 6. 6b — Yol tamamlama ve klasör geçmişi

### 6.1 Tamamlama

- Adres çubuğu düzenleme kipindeyken (Ctrl+L) yazılan metnin son parçasına göre alt klasör önerileri, alanın altında açılır listede (en çok 12) görünür. Dosyalar önerilmez.
- Okuma bir iş parçacığında yapılır; yazmaya 80 ms ara verilince başlar, yeni tuş öncekini geçersiz kılar. Ağ yolları ve sürücü kökleri için zaman sınırı 1 sn; aşılırsa liste boş kalır.
- ↑/↓ öneriler arasında gezer; Tab ya da → seçili öneriyi alana yazar (sonuna ayırıcı ekler); Enter gider; Esc önce listeyi, sonra düzenlemeyi kapatır.
- Eşleşme: önce baştan eşleşenler, sonra içinde geçenler; büyük/küçük harf duyarsız.
- **Açma:** `~` (yalnız başta) ev klasörü; Windows'ta `%AD%`, Unix'te `$AD` ve `${AD}` ortam değişkenleri. Bilinmeyen değişken olduğu gibi kalır. Açma `navigate_text` yolunda yapılır.

### 6.2 Klasör geçmişi

- Ziyaret edilen her klasör (`state.toml [history]`) kaydedilir: yol, ziyaret sayısı, son ziyaret zamanı. En çok 200 kayıt; dolunca en düşük puanlı düşer.
- **Puan:** ziyaret sayısı × yakınlık ağırlığı (son saat ×4, son gün ×2, son hafta ×1, daha eski ×0,25) — "frecency".
- Adres boşken ya da yalnız `~` / kök yazılıyken açılan liste iki bölüm gösterir: "Recent" (son 5) ve "Frequent" (puana göre ilk 7, Recent'takiler hariç). Yazmaya başlayınca tamamlama önerilerinin altında geçmişten eşleşenler (en çok 5) görünür.
- Artık olmayan klasörler liste gösterilirken arka planda denetlenip düşürülür (yalnız yerel diskler; ağ yolları atılmaz).
- `[history] remember = false` kaydı kapatır ve var olanı siler. `clear-history` eylemi (bağlamasız) geçmişi temizler.
- Yazma 5c'deki `state.toml` yazıcı iş parçacığından geçer; arayüz iş parçacığı dosyaya dokunmaz.

## 7. 6b — Kullanıcı komutları

`[[commands]]`'a eklenenler (5c §7.1'in üzerine):

| Alan | Tür | Anlamı |
|---|---|---|
| `shortcut` | kısayol metni | Komutu çalıştıran kısayol. `[shortcuts]` ile aynı çakışma kuralları; çakışırsa komutun kısayolu yok sayılır ve uyarı yazılır |
| `menu` | metin | Komutlar menüsünde alt menü adı; aynı adı taşıyanlar bir alt menüde toplanır |
| `ask` | bool | Çalıştırmadan önce "Run *ad* on N items?" sorusu (Run / Cancel) |

- **`{files}`:** bütün seçimi (tam yollarla, her biri ayrı argüman) tek çalıştırmaya verir. `{files}` içeren komut yalnız bir kez çalışır; `{in}`, `{name}`, `{ext}`, `{out}` ve `output` ile birlikte kullanılamaz (ayar hatası). Çalışma klasörü seçimin bulunduğu klasördür.
- **Komut satırı sınırı:** Windows'ta toplam uzunluk 32.000 karakteri, Unix'te `ARG_MAX`'ın yarısını aşarsa iş başlamadan "Too many items for one run of *ad*" hatası.
- `{files}`'lı komutun geri alınması yoktur (çıktısı bilinmez); panelde "can't be undone" notu görünür. Bu yüzden `ask` varsayılanı `{files}`'lı komutlarda da `false` kalır; kullanıcı açar.
- Kısayolla çalıştırma seçimdeki öğelere uygulanır; `types` ve `folders` kurallarına uymayan seçimde komut çalışmaz ve durum çubuğunda neden yazar.

## 8. Ayarlar ve kısayollar

### 8.1 `settings.toml`

```toml
[keyboard]
typing = "jump"          # "jump": letters jump to a name; "filter": letters open the filter

[history]
remember = true          # remember visited folders for the address bar

# [[filters]] name = "...", pattern = "*.jpg;*.png" — see section 3.3
# [[commands]] … shortcut = "ctrl+alt+r", menu = "Images", ask = true — see section 7
```

### 8.2 `state.toml`

- `[history]`: ziyaret kayıtları (yol, sayı, son zaman).
- `[selection] last_pattern`.

### 8.3 Yeni eylemler

`filter`, `invert-selection`, `select-pattern`, `deselect-pattern`, `select-same-type`, `restore-selection`, `tab-1` … `tab-8`, `tab-last`, `reopen-tab`, `tab-picker`, `toggle-tab-lock`, `clear-history`. Hepsi `Action` listesine, şablona, macOS menü çubuğuna (uygun menüye) ve "her varsayılan Windows ve Linux'ta ulaşılabilir" testine eklenir. Değişen varsayılanlar: `view-list` Ctrl+Shift+1, `view-grid` Ctrl+Shift+2.

## 9. Kod yapısı

| Parça | Yer |
|---|---|
| Desen dili, eşleyici | `gezik-core/src/pattern.rs` (yeni) |
| Seçimi ters çevirme, desenle seçme | `gezik-core/src/selection.rs` |
| Kapatılan sekme yığını, kilit | `gezik-core/src/nav.rs` |
| Klasör geçmişi ve puanlama, ortam değişkeni açma | `gezik-core/src/history.rs` (yeni), `gezik-core/src/nav.rs` |
| Ayarlar (`[keyboard]`, `[history]`, `[[filters]]`, komut alanları), durum | `gezik-config` |
| Numerik tuşlar, yeni eylemler | `gezik-config/src/shortcuts.rs`, `gezik/src/keys.rs` |
| Süzgeç çubuğu, desen penceresi, sekme seçici, öneri listesi | `gezik/ui/widgets/*.slint`, `gezik/src/{view, navigation, filter, …}.rs` |
| Klasör okuma (öneriler, var mı denetimi) | iş parçacığı; `gezik-platform` gerekirse |

## 10. Test

- **Birim (saf):** desen dili (joker, `;`, `!`, Türkçe harfler, boş ve hatalı desen), eşleyicinin ayırma yapmaması, ters çevirme ve desenle seçmenin değişen aralıkları, kapatılan sekme yığını (sınır, konum, geçmişin korunması), kilitli sekmenin kapanmaması, puanlama ve sınır, ortam değişkeni açma (iki platform biçimi, bilinmeyen değişken), `{files}` kuralları ve komut satırı sınırı, kısayol çakışmaları ve taşınan görünüm kısayolları, eski ayar dosyalarıyla uyum.
- **Performans:** 100.000 öğede süzgeç güncellemesi (`scripts/perf`).
- **Linux kabı:** `test.sh unit` ve `gui.sh` ile süzgeç, desenle seçme, sekme numaraları, kapatılanı geri açma, yol tamamlama ve komut kısayolu.
- **Windows ekran testleri:** her planın sonunda, 5. adımdaki gibi.
- **macOS:** yeni eylemler `macos-test.md`'ye eklenir.

## 11. Performans

- Süzgeç: derlenmiş desen, ad başına ayırma yok; 100.000 öğede ≤ 30 ms. Gerekirse sonuç önceki süzülmüş listeden daraltılır (yeni metin eskisinin uzantısıysa).
- Geçmiş en çok 200 kayıt; puanlama gösterim anında, ölçülemeyecek kadar ucuz.
- Öneriler iş parçacığında; arayüze yalnız sonuç listesi gelir.
