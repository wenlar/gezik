# Alt Proje 15: Yerelleştirme — Tasarım

- **Tarih:** 2026-10-13
- **Durum:** Kapsam kullanıcı kararı (2026-10-13: İngilizce kaynak ve Türkçe tam, insan gözden geçirmeli; Almanca, İspanyolca, Fransızca, Basitleştirilmiş Çince önce makine çevirisi, sonra düzeltilir; dil sistemden, `language` ile değişir; yalnız uygulama; topluluk çevirileri yeniden derlemeden düz metin dosyayla; Çince için yazı tipi yedeği düzeltmesi; §2). Ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§17, her biri gerekçesiyle). Spec onaylanınca plan önceden onaylıdır. 15a uygulandı (altyapı; plan `../plans/2026-10-13-yerellestirme-15a.md`, notlar `../notes/parts/15a.md`); 15b–15f bekliyor.
- **Kapsam:** Gezik yol haritasının 15. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md` satır 27; `2026-10-12-ozellik-durumu.md` satır 9 "Arayüz çevirisi (önce Türkçe)", bugün 🔜 Planlı, "kodda çeviri altyapısı yok"). Tek spec, altı parça (15a–15f; §13).
- **Dayandığı:** `2026-10-03` ayarlar ve tema spec'i (düz metin ayarlar, canlı yeniden yükleme, uyarı listesi), `2026-10-12-tasinabilirlik-design.md` (14a'nın şeması ve ayar penceresi, açıklamaların gömülü şablondan gelmesi ve §16'daki "çeviri 15. adımda" notu; 14e'nin paylaşılan/yerel klasörleri), `2026-10-12-onizleme-2-design.md` (13a'nın CJK "Bilinen sınır"ı).
- **Taban:** `master` 2ff884b. `Action::ALL` 132; menü kimlikleri 2999'a kadar ayrılmış (15: **3000–3199**, `GEZIK_IDS_END = 4096` değişmez); denetim listeleri macOS 387, Linux 281'de biter (15: macOS **388'den**, Linux **282'den**). Sürüm exe'si 29.356.032 B (14f). Slint 1.18.1, `i-slint-core`, `i-slint-backend-winit`, `i-slint-renderer-software` `vendor/`'da yamalı (`vendor/README.md`).
- **Rakip notları:** TC ve Far (exe yanında `.lng` düz metin dil dosyaları, numaralı satırlar; kullanıcı dosya koyar), DO (dil paketleri), Files (Crowdin, 40+ dil), OneCommander (~20 dil, Türkçe dahil), ForkLift (14 dil, Türkçe yok), Path Finder (20 dil). Hepsi metni anahtar/numarayla tutar; topluluk çevirisi TC/Far'da dosya bırakmakla, Files'ta web aracıyla.

## 1. Amaç

Gezik'in bütün arayüzünü **İngilizce ve Türkçe** (insan gözden geçirmeli) ile **Almanca, İspanyolca, Fransızca ve Basitleştirilmiş Çince** (önce makine çevirisi, "gözden geçirilmedi" işaretli) göstermek; dili **sistemden** almak ve ayarla değiştirmek; herkesin **yeniden derlemeden bir dosya bırakarak** yeni dil eklemesini ya da var olanı düzeltmesini sağlamak; Çince metnin **Çince yazı tipiyle** çizilmesi.

Hafiflik bozulmaz: İngilizce kullanıcıya hiçbir maliyet yok (katalog yüklenmez, `t!` metni olduğu gibi döner). Başka dilde katalog açılışta bir kez açılır; boşta hiçbir şey çalışmaz; dil dosyaları yalnız zaten var olan ayar izleyicisiyle izlenir.

### Başarı ölçütleri

- **Kapsama (15b, 15c):** sözde dilde (`language = "pseudo"`, §9) ana pencere, bütün menüler (macOS menü çubuğu, Windows/Linux `≡` ve sağ tık menüleri, tepsi), sorular, ayar penceresi, işlem paneli, durum çubuğu, önizleme, araçlar ve hata metinleri gezildiğinde **çerçevesiz (çevrilmemiş) metin kalmaz**; kalanlar §3.6'daki bilerek çevrilmeyenlerdir. Slint dosyalarında `@tr` dışında görünür metin bırakan satırı test yakalar.
- **Diller (15e, 15f):** `tr.po` %100, hepsi gözden geçirilmiş; `de`, `es`, `fr`, `zh-Hans` %100 çevrilmiş, hepsi `fuzzy` (gözden geçirilmedi) işaretli; dil seçicisi bunu "machine translated" olarak gösterir.
- **Dil seçimi (15a):** `language = "auto"` sistem dilini izler (Windows UI dili, macOS tercih edilen diller, Linux `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`); `language = "tr"` ya da ayar penceresindeki satır değiştirir; değişiklik **yeniden başlatmadan** ≤ 150 ms'de bütün pencerelere uygulanır.
- **Topluluk (15a):** `<ayar klasörü>/lang/pl.po` bırakılınca Lehçe dil listesinde görünür, seçilince eksik metinler İngilizce kalır; dosya kaydedilince Gezik yeniden yükler; yer tutucu hatası, sözdizimi hatası, eksik sayısı uyarı listesinde satır numarasıyla.
- **Çince (15d):** Windows'ta `测试这检查对齐` (13a'nın boş çıkan karakterleri) arayüzde ve dosya adlarında Microsoft YaHei ile çizilir; Japonca sistemde Kanji biçimi değişmez.
- **Hafiflik (hepsi):** İngilizcede boşta bellek `master`'a göre **0** (±0,02 MB), açılış ±0; başka dilde boşta ≤ +0,3 MB, açılış ≤ +3 ms; klasör açma ve kaydırma (`stress.ps1`) `master` ±%2. Her parça exe'ye ≤ +262.144 B (§12).

### Kapsam dışı (bilerek)

- **README, `docs/`, spec ve notlar:** İngilizce kalır (kullanıcı kararı). Topluluk için tek yeni belge `docs/translating.md` (İngilizce).
- **`settings.toml` şablon yorumları:** İngilizce kalır (karar 6). Ayar penceresindeki açıklamalar ise çevrilir.
- **Ayar anahtarları ve değerleri, kısayol sözdizimi** (`ctrl+shift+c`), `{token}`'lar, komut satırı seçenekleri ve `--help`, günlük/hata ayıklama çıktısı: İngilizce (dosyalar ve betikler dilden bağımsız kalmalı; destek için tek dil).
- **Kullanıcının yazdığı adlar:** `[[commands]]`, etiketler, kayıtlı süzgeç/arama/profil adları, tema adları çevrilmez.
- **Sağdan sola diller** (Arapça, İbranice): topluluk dosyasıyla metin gelir ama yerleşim aynalanmaz (bilinen sınır).
- **Uygulama içinde çeviri düzenleyicisi, çeviri sunucusu (Weblate/Crowdin kurulumu), çevrimiçi dil indirme:** dosya bırakmak ve Poedit yeter; sunucu yok.
- **Sıralama ve eşleştirmede dile göre karşılaştırma:** dosya adı sıralaması ve süzme bugünkü gibi (Türkçe İ/ı zaten katlanıyor); arayüz diline bağlanmaz.

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Diller | İngilizce (kaynak) ve Türkçe tam, insan gözden geçirmeli; Almanca, İspanyolca, Fransızca, Basitleştirilmiş Çince önce makine çevirisi, sonra düzeltilir | Kullanıcı kararı 2026-10-13 |
| Dil seçimi | Varsayılan sistem dili; `settings.toml`'da `language = "tr"` ve ayar penceresinde bir satır (14a şeması); `"auto"` = sistem | Kullanıcı kararı |
| Kapsam | Yalnız uygulama: Slint metinleri, menüler (macOS/Windows yerel menüleri dahil), sağ tık menüleri, sorular, durum satırları, uyarı/hatalar, ayar penceresi başlık ve açıklamaları; README/belgeler değil. Şablon yorumlarına bu spec karar verir | Kullanıcı kararı |
| Topluluk çevirileri | Yeniden derlemeden bırakılan düz metin dosyalar (`<config>/lang/<kod>.<uzantı>`); eksik metin İngilizceye düşer; yerleşik diller exe'ye gömülü | Kullanıcı kararı |
| Çince | CJK yazı tipi yedeği düzeltmesi bir parça olarak dahil | Kullanıcı kararı |
| Süreç | Parçalar sırayla, paralel dal yok (birleştirme çakışmaları) | Kullanıcı kararı |
| Hafiflik | Boşta hiçbir şey çalışmaz, parça başına ≤ +262.144 B | Ürün hedefi |

## 3. Bugünkü durum (araştırma)

### 3.1 Metinler nerede, kaç tane

Ölçüm: test modülleri hariç kaynakta, büyük harfle başlayan ya da sözcük içeren dizgi sabitleri (betik spec'in yanında değil, sonuçları burada):

| Yer | Dizgi (yer) | Ayrı metin | Bayt (ayrı) | Not |
|---|---|---|---|---|
| `crates/gezik/ui/**/*.slint` (32 dosya, 10.680 satır) | 436 | 356 | 4,7 KB | `app.slint` 149, `convert` 54, `rename-batch` 40, `info` 33; `@tr` hiç yok; 79 `accessible-label`/`-description` |
| Rust (`crates/**/src`, test dışı) | 2.985 | 2.361 | 53 KB | Sezgisel: bir kısmı görünür değil (anahtar adları, MIME, biçim). Başlıcalar `context_menu.rs` 250, `shortcuts.rs` 146 (`Action::title`), `compare.rs` 120, `integration.rs` 118, `operations.rs` 89, `system_changes.rs` 77, `settings_window.rs` 65; çekirdek sandıklarda da var (`gezik-core` `fields.rs`, `group.rs`, `elevated.rs`, `batch/`; `gezik-platform` `fs/describe.rs`, `linux/sni.rs`) |
| Ayar penceresi | 64 alan + 9 sayfa | — | ≈ 10 KB | Başlık anahtardan türetiliyor (`schema.rs` `Field::title`: `reduce-motion` → "Reduce motion"); açıklama gömülü şablonun yorumundan (334 yorum satırı, 21 KB) |

**Tahmin:** görünür ≈ **2.300 ayrı metin, ≈ 60 KB İngilizce**. Çoğul elle yazılıyor (`if n == 1 { "1 item" } else { format!("{n} items") }`, `pdf.rs` `plural()`; 17 yer). Bazı Slint metinleri birleştirmeyle kuruluyor (`"… n more"`, `", tags "`).

### 3.2 Slint 1.18'in çeviri desteği

- `@tr("…")`, bağlamla `@tr("ctx" => "…")`, çoğulla `@tr("{n} file" | "{n} files" % count)`; yer tutucular `{}`, `{0}`, `{n}`.
- Üretilen Rust kodu, derleme anında gömülü çeviri (bundled `.po`) yoksa `slint::private_unstable_api::translate` → `i_slint_core::translations::translate` çağırır. Bu işlev sırayla: `tr` özelliği açıksa **dış çevirmen** (`tr::Translator`, `SlintContext::set_external_translator`), Unix'te `gettext` özelliği açıksa gettext, yoksa kaynak metin.
- Gömülü çeviri (`slint-build` `with_bundled_translations`): yalnız `.slint` metinleri, derleme anında, çalışırken dosya okumaz; `select_bundled_translation` ile dil değişir. Rust metinlerine ve topluluk dosyalarına yetmez.
- `slint::update_all_translations()` bütün `@tr` bağlarını yeniden değerlendirir (dil değişince).
- `slint` sandığı `tr` özelliğini dışarı açmıyor; `i-slint-core`'da var (`tr = ["dep:tr"]`). Gezik zaten `i-slint-core`'u `=1.18.1` ile (dev-bağımlılık) kullanıyor ve vendor'da tutuyor: normal bağımlılık olarak `features = ["std", "tr"]` eklenince özellik birleşir; `i_slint_core::context::with_global_context(…, |ctx| ctx.set_external_translator(…))` herkese açık bir işlev. Vendor yamaları çeviri yolunu değiştirmiyor.
- Ondalık ayıracı: Slint'in `Platform.decimal-separator`'ı yalnız gömülü çeviri ya da gettext ile değişir; Gezik kendi sayılarını kendisi biçimlediği için kullanılmaz.

### 3.3 Biçimler

- `gezik-core/src/lib.rs`: `format_size_in` (`1.5 KB`, birimler `B KB MB GB TB` / `kB`), `group_digits` (`1,234,567`).
- `gezik-core/src/view.rs` `relative_date`: `5 min ago`, `Today 14:05`, `Yesterday 14:05`.
- `gezik-platform/src/datetime.rs`: `system` biçimi Windows'ta sistemin bölge biçimi (`GetDateFormatEx`), başka yerde `YYYY-MM-DD HH:MM`; `short`, `iso`.
- Kısayol metni `keys.rs`/`shortcuts.rs`'de `Ctrl+Shift+C`, macOS'ta `⌘⇧C`.

### 3.4 Yazı tipi yedeği (13a "Bilinen sınır")

Slint'in metin yolu (parley 0.11 + fontique 0.11) yedek yazı tipini **yazı sistemi + dil** başına seçer; dil verilmediği için Han karakterleri için Japonca yazı tipi (Windows'ta Yu Gothic) seçilir, onda olmayan sadeleştirilmiş biçimler (测试这检查对齐) boş çıkar. `vendor/i-slint-core/textlayout/sharedparley/shaping.rs` varsayılan stile `FontFamily` ve `FALLBACK_FAMILIES` (`SansSerif`, `SystemUi`) veriyor, `StyleProperty::Locale` vermiyor. parley 0.11'de `StyleProperty::Locale(Option<Language>)` var.

### 3.5 Türkçe büyük/küçük harf

Eşleştirmede İ/ı zaten katlanıyor (`gezik-core` `sort.rs`, `palette.rs`, `pattern.rs`, …). Arayüz metninde çalışma anında büyük harfe çevirme yalnız `schema.rs`'nin başlık türetmesinde (`make_ascii_uppercase`, ASCII) var; 15b'de bu başlıklar katalogdan gelir.

### 3.6 Gezik'in denetleyemediği metinler

- Windows Kabuk'un sağ tık öğeleri (Explorer fiilleri), sistemin dosya/klasör seçicileri, Windows Özellikler penceresi, işletim sisteminin hata metinleri (`io::Error` metni sistem dilinde): sistem dilinde kalır.
- macOS uygulama menüsünün AppKit öğeleri (Services, Hide Others, Show All, Edit menüsüne eklenen Emoji & Symbols, Start Dictation): paketin `CFBundleLocalizations`'ına ve `AppleLanguages`'a göre (§10.2).

## 4. Ortak yapı

### 4.1 Yeni sandık `gezik-i18n` (saf, bağımlılıksız; yalnız `miniz_oxide`, zaten bağlı)

Bütün sandıklar (`gezik-core`, `gezik-config`, `gezik-platform`, `gezik-batch`, `gezik-ops`, `gezik-search`, `gezik`) kullanabilsin diye en altta. İçinde:

| Modül | Görev |
|---|---|
| `catalog.rs` | `Catalog`: (bağlam, kaynak) → çeviri (çoğul biçimleriyle). Gömülü dilden (§4.3) ya da `.po`'dan kurulur; kullanıcı dosyası gömülünün üstüne katman (önce dosya, sonra gömülü, sonra İngilizce) |
| `po.rs` | `.po` okuyucusu (msgctxt, msgid, msgid_plural, msgstr[n], `#,` bayrakları, başlık, çok satırlı dizgiler, kaçışlar); hatalar satır numarasıyla |
| `plural.rs` | Yerleşik dillerin kuralları (tablo) ve topluluk dosyaları için `Plural-Forms` ifadesinin değerlendiricisi (gettext'in C alt kümesi: `n`, sayılar, `== != < > <= >= && || ! % ?:`, parantez) |
| `format.rs` | Slint'in biçimleyicisiyle aynı sözdizimi: `{}`, `{0}`, `{n}`, `{{`/`}}`; yer tutucu kümesi karşılaştırması (denetleyici için) |
| `pseudo.rs` | Sözde dil (§9) |
| `number.rs` | Dilin ondalık ve basamak ayıracı (§8.1) |
| `lang.rs` | Dil kodu eşleme (`zh_CN.UTF-8` → `zh-Hans`, `tr-TR` → `tr`; §5.2), endonimler |
| `lib.rs` | Küresel geçerli katalog (`RwLock<Option<Arc<Catalog>>>`; `None` = İngilizce), `t()`, `tn()`, `tf()`; makrolar `t!`, `tn!`, `tx!` |

Sistem dilini okumak işletim sistemi işi: `gezik-platform/src/locale.rs` (yeni; §5.2).

### 4.2 Kaynak metin anahtardır (gettext usulü)

- Anahtar **İngilizce kaynak metin** + isteğe bağlı bağlam (karar 2). Slint `@tr` zaten böyle çalışır; Rust da aynı.
- **Rust:**
  - `t!("Copy Path")` → `String` (geçerli dilde; katalog yoksa kaynak).
  - `t!("Rename {} items", n)`, `t!("Moved {0} to {1}", name, dest)` → biçimli (Slint'in sözdizimi, çeviride sıra değişebilir).
  - `tn!("{n} item" | "{n} items", n)` → çoğul; `{n}` sayıyı dilin biçimiyle yazar (§8.1).
  - `t!("menu" => "Open")` → bağlamlı.
  - `tx!("New Tab")` → derleme anında olduğu gibi `&'static str` döner, yalnız çıkarıcıya işaret (gettext'in `N_`): `Action::title`, `Page::title`, sabit tablolar böyle kalır, gösterilirken `t(action.title())`.
- **Slint:** `text: @tr("Cancel");`, `@tr("{} selected", count)`, `@tr("{n} file" | "{n} files" % count)`. Birleştirerek kurulan metinler tek `@tr`'ye çevrilir.
- **Bağlam kuralı:** aynı İngilizce sözcüğün iki anlamı varsa ya da çeviride biçimi değişiyorsa: `menu`, `column`, `unit`, `key`, `choice:<anahtar>`, `setting:<tablo.anahtar>`, `button`. Başka yerde bağlam yok.
- **Cümle kuralı:** metin parçalardan birleştirilmez (`"Copy " + name` yok); yer tutuculu tam cümle. Türkçede eklerin yer tutucuya bitişmemesi için çevirmen kılavuzu (§11.4).

### 4.3 Gömülü diller: sıkıştırılmış, karma tablolu

`gezik-i18n/build.rs` derleme anında `lang/*.po`'dan:

1. `gezik.pot`'taki sırayla her anahtarın 64 bit FNV-1a karması (`bağlam \x04 kaynak`), **bir kez**, sıralı dizi (≈ 2.300 × 8 B; ilk dil gelene kadar boş).
2. Her dil için çeviriler aynı sırada, biçimler `\x1F` ile, girdiler `\0` ile ayrılmış, `miniz_oxide` ile deflate; başlık: kod, endonim, gözden geçirme durumu (§11.3), çoğul kuralı, ayraçlar.

Çalışırken yalnız **seçili dil** açılır (≈ 80 KB metin + ≈ 10 KB konum dizisi), arama karmanın ikili aramasıyla, ayırma yok. İngilizce seçiliyse hiçbir şey açılmaz. Çıkarıcı testi pot'taki karmaların çakışmadığını denetler. Topluluk dosyası `.po`'dan ayrıştırılıp `HashMap<String, …>` olur (karma yok, çakışma yok).

### 4.4 Slint bağlantısı

`gezik/Cargo.toml`: `i-slint-core = { version = "=1.18.1", default-features = false, features = ["std", "tr"] }` (bugün dev-bağımlılık; normal bağımlılığa taşınır). Açılışta, ilk pencereden önce: `with_global_context(…, |ctx| ctx.set_external_translator(Some(Box::new(GezikTranslator))))`; `GezikTranslator` `tr::Translator`'ı küresel katalogla uygular. Dil değişince `slint::update_all_translations()`. Bunu bağlayan test `crates/gezik/tests/slint_translate.rs`: bir `@tr` metni çevirmen kurulunca değişir, kurulmayınca kaynak kalır (Slint yükseltmesinde kırılırsa yol değişmiş demektir; `vendor/README.md`'ye not).

### 4.5 Rust'ın tuttuğu metinlerin yenilenmesi

`@tr` kendiliğinden yenilenir. Rust'ın modele verdiği metinler (macOS menü çubuğu girdileri `menu_bar.rs` `set_commands`/`set_tags`, palet satırları, sütun başlıkları, kenar çubuğu bölümleri, durum çubuğu, ayar penceresi satırları, tepsi menüsü) için tek kanca: `i18n::on_change` → her pencerenin bugünkü "yeniden kur" yolları (ayar yeniden yüklemesinin zaten çağırdıkları). Windows/Linux sağ tık menüleri her açılışta kurulduğu için kendiliğinden doğru. Açık bir soru penceresi kapanana kadar eski dilde kalabilir (bilinen sınır, §16).

### 4.6 Hafiflik kuralı

- İngilizce: `t!` küresel `None`'ı görüp kaynağı döner (bir okuma kilidi; kaynak `String`'e kopyalanır, bugünkü `to_owned` ile aynı).
- Satır başına metinler (tür adları "Folder", göreli tarih, boyut birimi) klasör dolarken bir kez, kaydırmada yeniden hesaplanmaz (bugünkü gibi); 15c `stress.ps1` ile doğrular.
- Dil dosyası izleme yeni izleyici kurmaz: `lang/` ayar klasöründe (14e'nin **paylaşılan** klasörü; senkronla taşınır).

## 5. Dil seçimi (15a)

### 5.1 Ayar

```toml
# Interface language: "auto" follows the system; or a code: "en", "tr", "de", "es",
# "fr", "zh-Hans", or the name of a file in the lang folder (lang/pl.po -> "pl").
# "pseudo" shows every text marked and lengthened, for translators.
language = "auto"
```

- Şemaya `f(None, "language", General, Language)`; yeni `Kind::Language` (14a'nın `Theme`'i gibi dinamik liste: yerleşikler + `lang/` dosyaları, her biri endonimiyle ve durumuyla: "Deutsch — machine translated", "Polski — 62 % translated").
- Bilinmeyen kod: uyarı listesinde satır ("language "xx": no such language, using the system's"), `auto` gibi davranır.

### 5.2 Sistem dili

`gezik_platform::locale::preferred() -> Vec<String>` (sıralı):

| Sistem | Kaynak |
|---|---|
| Windows | `GetUserPreferredUILanguages(MUI_LANGUAGE_NAME)` (Windows görüntü dili ve yedekleri) |
| macOS | `NSLocale.preferredLanguages` (Sistem Ayarları ▸ Dil ve Bölge, uygulamaya özgü dil dahil) |
| Linux | `LANGUAGE` (iki nokta ile liste; yalnız etkin yerel `C`/`POSIX` değilse, gettext kuralı), sonra `LC_ALL`, `LC_MESSAGES`, `LANG`; `.UTF-8`, `@euro` atılır |

Eşleme (`gezik_i18n::lang::choose`, saf, testli): kullanıcının sırasıyla her dil için önce tam kod, sonra yazı sistemi (`zh-CN`, `zh-SG`, `zh-Hans-*`, `zh_CN` → `zh-Hans`; `zh-TW`, `zh-HK`, `zh-MO`, `zh-Hant` → yok, Basitleştirilmiş'e **düşmez**), sonra temel dil (`de-AT` → `de`, `pt-BR` → `pt` dosyası varsa); hiçbiri yoksa `en`. `C`/`POSIX` → `en`. Topluluk dosyaları da aday.

### 5.3 Ne zaman

Açılışta ayarlar okunduktan sonra, ilk pencereden önce (bir sistem çağrısı + seçili dilin açılması; İngilizcede yalnız çağrı). Çalışırken `language` değişince ya da `lang/` dosyası değişince katalog iş parçacığında kurulur, UI iş parçacığında değiştirilir, §4.4/§4.5 yenilenir. Sistem dili çalışırken değişirse Gezik bir sonraki açılışta izler (Windows/macOS da çoğu uygulamada öyle).

## 6. Metinlerin çıkarılması (15b, 15c)

- **15b arayüz:** bütün `.slint` metinleri `@tr` (79 erişilebilirlik adı dahil); `Action::title` (`tx!`), menüler (`menu_bar.rs`, `app.slint` `MenuBar`, `context_menu.rs`, tepsi `tray/*`, `linux/sni.rs`), palet, sütun başlıkları, kenar çubuğu, ayar penceresi (sayfa adları `tx!`; alan başlıkları bağlamla `setting:<yol>`, kaynak bugünkü türetilmiş başlık; açıklamalar §7; seçim etiketleri `choice:<anahtar>`, yazılan değer ham kalır), kısayol sayfası.
- **15c iletiler:** durum çubuğu ve işlem paneli metinleri, sorular ve düğmeleri, uyarı/hata metinleri (Gezik'in sarmalayıcısı; içteki sistem metni değil), araçlar (karşılaştırma, sağlama, eşitleme, toplu yeniden adlandırma, dönüştürme, PDF), Bilgi penceresi, Sistem Bütünleşmesi soruları, çekirdek sandıkların görünür metinleri (`fields.rs` alan adları, `group.rs` grup başlıkları, `describe.rs` tür adları, `elevated.rs`), çoğulların `tn!`'ye geçişi, biçimler (§8).
- **Slint denetimi (test, 15b):** `.slint`'te `text:`, `title:`, `placeholder-text:`, `accessible-label:`, `accessible-description:`, `tooltip` özelliklerine verilen düz dizgi sabiti (izin listesi: yalnız sembol/boşluk, ör. `"…"`, `"✓"`, `"/"`) testi kırar.
- **Rust denetimi:** derleyiciyle yapılamaz; sözde dil gezintisi (§9) ve 15c'nin parça notundaki ekran listesi. `.to_uppercase()`/`.to_lowercase()`'in çevrilmiş metne uygulanmaması kuralı (Türkçe i/İ) için test: `t!(…)` ile aynı ifadede harf çevirisi grep'le yakalanır.

## 7. Ayar penceresi ve şablon

- **Şablon yorumları İngilizce kalır** (karar 6): `settings.toml` 14f ile makineler arasında paylaşılır (iki farklı dilli makine yorumları birbirine yeniden yazardı), anahtarlar ve değerler zaten İngilizce, yorumlar `docs/configuration.md` ile aynı dilde dosya biçimini belgeler, Gezik kullanıcının dosyasına dil değişince dokunmaz.
- **Pencere açıklamaları çevrilir:** çıkarıcı gömülü şablondan her alanın yorumunu `setting:<tablo.anahtar>` bağlamıyla pot'a koyar (14a'nın şablon ayrıştırıcısı `schema.rs` yeniden kullanılır); pencere açıklamayı `t(ctx, şablon_yorumu)` ile gösterir. Şablon yorumu değişince eski çeviri anahtar uymadığı için düşer, İngilizce görünür (yanlış açıklama göstermez).
- 14a'nın şema ↔ şablon testine bir bağ eklenir: her alanın başlığı ve açıklaması pot'ta.

## 8. Biçimler (15c)

### 8.1 Sayılar ve boyutlar

- Gezik'in kendi sayıları (**arayüz diline** göre; karar 9): ondalık ve basamak ayıracı dilin başlığından (`X-Gezik-Number: decimal=",", group="."`); yerleşikler: en `.` `,`; tr `,` `.`; de `,` `.`; es `,` `.`; fr `,` `U+202F`; zh-Hans `.` `,`. Topluluk dosyasında yoksa İngilizce.
- `format_size_in`, `group_digits` dil parametresi alır (saf kalır; çağıran `i18n::number()` verir). Birimler bağlamlı (`unit`): Fransızca `o Ko Mo Go To`, başkaları `B KB MB GB TB`. Ayar `size-format` (binary/decimal) değişmez.
- `tn!`'in `{n}`'i dilin basamak ayıracıyla yazar.

### 8.2 Tarihler

- `relative`: `tn!("{n} min ago" | …)`, `t!("Today {}", clock)`, `t!("Yesterday {}", clock)`.
- `system`: bugünkü gibi sistemin **bölge** biçimi (Windows API'si; macOS/Linux ISO benzeri). Arayüz diline bağlanmaz: kullanıcının bölge ayarı onun seçimidir.
- `short`, `iso`: dilden bağımsız. Ay/gün adı kullanan biçim yok (yeni de eklenmez).

### 8.3 Kısayol metinleri

- Windows/Linux'ta değiştirici ve tuş adları `key` bağlamıyla çevrilir: de `Strg`, `Umschalt`, `Entf`, `Rücktaste`, `Leertaste`, `Pos1`, `Ende`, `Bild↑`; fr `Maj`, `Entrée`, `Suppr`, `Espace`; es `Mayús`, `Supr`; tr ve zh-Hans `Ctrl`, `Shift` (Windows'un o dillerdeki kullanımı). Windows yerel menülerinde sekmeden sonraki hızlandırıcı metni de.
- macOS sembolleri (`⌘⌥⇧⌃`) değişmez. `settings.toml`'daki kısayol yazımı hep İngilizce (`ctrl+shift+c`); 14b'nin kısayol sayfası çevrilmiş adı gösterir, İngilizce yazar.

## 9. Sözde dil

`language = "pseudo"` (dil listesinde en sonda "Pseudo (for translators)"): katalog yerine her metni çalışma anında dönüştürür: `⟦Ŝéţţîñĝš ··⟧`. Harfler aksanlı benzerine, kısa metinler daha çok olmak üzere ≈ %40 uzatma (Almancanın en kötü durumu), `{…}` yer tutucuları ve `\n` korunur, `i` → `ı/İ` içerir (Türkçe harf denetimi). Kullanımı:

1. **Çevrilmemiş metin:** çerçevesiz görünen her metin katalogdan geçmiyor.
2. **Taşma:** sağda `⟧` görünmüyorsa metin kesiliyor ya da kırpılıyor.
3. **Birleştirme:** bir cümle içinde iki `⟦…⟧` → parçadan kurulmuş.

Kodu ≈ 1 KB; gömülü dosya yok.

## 10. Platform farkları

### 10.1 Windows

- Yerel sağ tık ve `≡` menüleri (`context_menu.rs`, `TrackPopupMenu`) Gezik metinleri çevrilir; Kabuk'un eklediği öğeler Windows dilinde (§3.6).
- 9b4'ün kayıt defterine yazdığı fiil adı ("Open in Gezik") **açıldığı anki dilde** yazılır; dil değişince Sistem Bütünleşmesi ▸ `Repair` yeniden yazar (bilinen sınır).
- Ekran okuyucu (Anlatıcı) sesi sistemin dilinden; erişilebilirlik adları çevrilir.

### 10.2 macOS

- `MenuBar` Slint'te: `@tr` ile çevrilir. Uygulama menüsünün Slint/muda'nın kurduğu öğeleri (About/Hide/Quit Gezik): muda'ya verilen metin İngilizce sabitse vendor'daki `i-slint-backend-winit`'e küçük bir yama metni dışarıdan alır (15b'nin ilk görevi bunu doğrular; yama "Gezik patch" işaretiyle ve `vendor/README.md`'de).
- `Info.plist`'e `CFBundleLocalizations` (en, tr, de, es, fr, zh-Hans) ve `CFBundleDevelopmentRegion = en`: AppKit'in kendi öğeleri (Services, Emoji & Symbols, dosya seçicisi) sistem dili bu listedeyse o dilde.
- `language` sistemden farklıysa Gezik kendi tercih alanına (`NSUserDefaults.standardUserDefaults`, yalnız Gezik'in alanı, sistem değişikliği değil) `AppleLanguages = [kod]` yazar, `auto`'da siler: AppKit öğeleri **bir sonraki açılışta** izler (Gezik'in metinleri hemen).
- Tercih edilen diller "zh-Hans-CN" biçiminde; §5.2 eşlemesi bunu kapsar.

### 10.3 Linux

- `LANGUAGE` listesi ve `C.UTF-8` (yaygın kapsayıcı varsayılanı) → `en`.
- Masaüstü dosyası (`gezik-platform/src/linux/desktop_entry.rs`, 9b): yerleşik diller için `Name[tr]`, `GenericName[tr]=Dosya Yöneticisi`, `Comment[…]` katalogdan yazılır (masaüstü ortamı kendi diline göre seçer; `language` ayarından bağımsız). 15e/15f'de.
- Çince yazı tipi sistemde yoksa (Noto Sans CJK kurulu değilse) Çince metin kutucuk çıkar: Gezik yazı tipi taşımaz; dil listesinde `zh-Hans` seçilince sistemde Han kapsayan yazı tipi yoksa durum çubuğunda "Install a Chinese font (e.g. Noto Sans CJK SC) to show this language" (fontique sorgusu, bir kez).
- Tepsi (SNI) menüsü metinleri çevrilir; GNOME uzantısının kendi metinleri değil.

## 11. Çevirmenler nasıl çalışır

### 11.1 Dosya biçimi: gettext `.po`

- Depoda `crates/gezik-i18n/lang/gezik.pot` (şablon) ve `tr.po`, `de.po`, `es.po`, `fr.po`, `zh-Hans.po`. Kullanıcının dosyaları `<paylaşılan ayar klasörü>/lang/<kod>.po` (Windows `%APPDATA%\gezik\lang\pl.po`, macOS `~/Library/Application Support/gezik/lang/`, Linux `~/.config/gezik/lang/`; taşınabilir kipte `gezik-data/lang/`).
- Her girdide `#:` yalnız dosya yolu (satır numarası yok: her değişiklikte pot'ta gürültü ve çakışma olmasın), `#.` çevirmen notu (koddaki `// TRANSLATORS:` yorumundan), `msgctxt`, `msgid`, `msgid_plural`.
- Başlık: `Language`, `Plural-Forms`, `X-Gezik-Name` (endonim, "Polski"), `X-Gezik-Number`, `X-Gezik-Review` (`human` | `machine`).
- Araçlar: Poedit, Lokalize, Weblate, Crowdin `.po`'yu doğrudan açar; düz metin, elle de düzenlenir.

### 11.2 Çıkarıcı ve denetleyici: `gezik-lang`

`crates/gezik-i18n/src/bin/gezik-lang.rs` (geliştirici aracı, exe'ye girmez; `cargo run -p gezik-i18n --bin gezik-lang -- <komut>`):

| Komut | İş |
|---|---|
| `extract` | `.slint` (`@tr`), Rust (`t!`, `tn!`, `tx!`), şablon açıklamalarından `gezik.pot`'u yazar; sıra deterministik (dosya yolu, ilk geçiş) |
| `update` | Her `.po`'yu pot'a uydurur: yeni anahtar boş eklenir, kaldırılan anahtar dosyanın sonuna `#~` ile; kaynak metni değişen anahtarın çevirisi **taşınmaz** (yanlış çeviri göstermektense İngilizce) |
| `check [dosya…]` | Dil başına: çevrilmiş/eksik/gözden geçirilmemiş (`fuzzy`) sayısı, kullanılmayan anahtarlar, yer tutucu kümesi farkı (`{}` sayısı, `{0}`, `{n}`), çoğul biçim sayısı `Plural-Forms` ile uyuşmazlığı, sözdizimi hatası satırıyla, İngilizcenin 2 katını aşan uzunluk (uyarı) |
| `pseudo` | Yok; sözde dil çalışma anında (§9) |

**CI ile bağ** (sıradan `cargo test -p gezik-i18n`, ek CI adımı yok): pot kaynakla güncel değilse test kırılır ("run gezik-lang extract"); her yerleşik `.po`'da yer tutucu/sözdizimi hatası kırar; `X-Gezik-Review: human` olan dilde (tr) **eksik** anahtar kırar (`fuzzy` yalnız uyarı); `machine` dillerde eksik yalnız uyarı (İngilizceye düşer). Böylece yeni metin ekleyen her gelecek PR pot'u ve `tr.po`'yu birlikte günceller.

Topluluk çevirmeni cargo'suz çalışır: Gezik aynı denetimi `lang/*.po` için açılışta ve dosya değişince yapar, sonuçları bugünkü **uyarı listesine** yazar (`lang/pl.po line 120: "{0}" missing in the translation`, `lang/pl.po: 140 of 2,300 texts missing`). Yeni arayüz yok.

### 11.3 Makine çevirisi işareti

- Makine çevirisiyle gelen her girdi `#, fuzzy`. Gezik gettext'ten ayrılarak `fuzzy` girdileri **kullanır** (yoksa makine çevirisi hiç görünmezdi) ama sayar; Poedit/Weblate `fuzzy`'yi zaten "gözden geçirilmeli" diye gösterir: düzelten kişi bayrağı kaldırır, başka işaret gerekmez.
- Dosya başlığı `X-Gezik-Review: machine`; dil listesinde "Deutsch — machine translated" (hepsi `fuzzy`) ya da "Deutsch — 40 % reviewed". Bütün `fuzzy`'ler kalkınca başlık `human` olur, not kaybolur.
- `update`'in "kaynak değişti" girdileri `fuzzy` yapılmaz, boşaltılır: `fuzzy` yalnız "makine çevirdi, insan bakmadı" anlamında kalır.

### 11.4 Kılavuz (`docs/translating.md`, İngilizce)

Nasıl başlanır (pot'u Poedit'te aç, `lang/`'a kaydet, `language = "pl"`), sözde dil, uyarı listesi, katkı (PR'la depoya `.po`; yerleşik yapılması bakımcı kararı), sözlük (≈ 30 terim × 6 dil: tab/sekme, pane/bölme, folder/klasör, Trash–Recycle Bin/Çöp Kutusu–Geri Dönüşüm Kutusu, …), kurallar: yer tutucuya ek bitiştirme (Türkçe "{}'ı" yerine "Silinemedi: {}" ya da "“{}” öğesi"), menülerde dilin platform alışkanlığı (Türkçe cümle düzeni "Yeni sekme"; Almanca adlar büyük), `…` korunur, kısayol metni çevrilmez, yalnız `key` bağlamı.

## 12. Bütçe ve performans

Dil başına gömülü çeviri ≈ 60–85 KB ham (zh-Hans daha az karakter, karakter başına 3 B), deflate ile ≈ 25–30 KB; karma tablosu bir kez ≈ 18 KB.

| Parça | Exe (tahmin; sınır ≤ +262.144 B) | Boşta bellek | Kullanılırken |
|---|---|---|---|
| 15a | ≈ +110 KB (`gezik-i18n` ≈ 45, `tr` yolu ve çevirmen ≈ 10, sistem dili ≈ 10, `Kind::Language` satırı ≈ 15, uyarılar ≈ 10, `.po` okuyucusu çalışma anında da ≈ 20) | 0 | dil değişimi ≤ 150 ms |
| 15b | ≈ +120 KB (≈ 450 `@tr` çağrı yeri ve ≈ 900 Rust `t!`) | 0 | menü açılışı `master` ±1 ms |
| 15c | ≈ +170 KB (≈ 1.400 `t!`/`tn!`, biçimler) | 0 | `stress.ps1` ±%2 |
| 15d | ≈ +5 KB | 0 | ilk metin ölçümü ±%2 |
| 15e | ≈ +50 KB (karma tablosu + tr) | İngilizcede 0; Türkçede ≤ +0,3 MB | açılış ≤ +3 ms |
| 15f | ≈ +115 KB (dört dil) | seçili dil kadar, ≤ +0,3 MB | aynı |

Toplam ≈ +570 KB. Sınıra en yakın 15c: aşılırsa sandık sınırında ikiye bölünür (15c1 `gezik`, 15c2 çekirdek sandıklar ve biçimler; PR'da gerekçeyle). Çağrı yeri maliyeti 15b'nin ilk görevinde 50 yerlik bir örnekle ölçülür; `t!` genel olmayan tek bir `fn t(&'static str) -> String`'e iner (şişmesin diye).

- **Ölçüm:** her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1` ve `stress.ps1`'i yazar; 15e/15f için ek: Türkçe ve Çince ile boşta bellek ve açılış.
- **Zamanlayıcı ve yoklama yok.**

## 13. Parçalar (her biri bir plan ve bir PR, sırayla)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **15a** | Altyapı: `gezik-i18n`, `.po` okuyucusu, çoğul kuralları ve değerlendiricisi, `t!`/`tn!`/`tx!`, Slint dış çevirmeni, sistem dili, `language` ayarı ve satırı, `lang/` yükleme ve uyarılar, sözde dil, `gezik-lang`, boş `gezik.pot`, `docs/translating.md` taslağı | — |
| 2 | **15b** | Arayüz metinleri: Slint, menüler (macOS dahil), eylem başlıkları, sağ tık, palet, tepsi, ayar penceresi (başlık, açıklama, seçim), kısayol sayfası, erişilebilirlik adları; Slint denetim testi | 15a |
| 3 | **15c** | İletiler ve biçimler: durum, sorular, hatalar, araçlar, çekirdek sandıklar, çoğullar, sayı/boyut/tarih, tuş adları | 15b |
| 4 | **15d** | CJK yazı tipi yedeği (vendor) | 15a (metin yerel ayarı arayüz dilinden) |
| 5 | **15e** | Türkçe: `tr.po`, bakımcının gözden geçirmesi, macOS `CFBundleLocalizations`/`AppleLanguages`, Linux masaüstü dosyası `[tr]`, Türkçe taşma düzeltmeleri | 15c |
| 6 | **15f** | Makine çevirileri `de`, `es`, `fr`, `zh-Hans` (`fuzzy`), Almanca taşma geçişi, masaüstü dosyası, `docs/translating.md` son hali, README ve özellik tablosu | 15e, 15d |

Sıra değer ve risk: altyapı bir kez ve küçük; metin çıkarma mekanik ama büyük, iki PR'a bölünür (arayüz önce, çünkü en görünür ve menü/ayar penceresi yapısına dokunur); Çince düzeltmesi küçük ve bağımsız, Çince gelmeden önce. Türkçe ilk gerçek dil: biçim, çoğul ve taşma kurallarını gerçek metinle sınar. Makine çevirileri en sona, hepsinin üstüne.

### 13.1 Süreç

- **Sırayla, paralel dal yok** (kullanıcının kuralı): her parça `master`'dan, öncekinin birleşmesinden sonra. Pot her PR'da yeniden üretildiği için iki açık dal aynı anda pot'a dokunmaz.
- **15e'nin gözden geçirmesi:** Türkçe taslak (`fuzzy`) PR'da; bakımcı Poedit'te ya da PR'da düzeltir. PR, CI ve kod incelemesinden sonra taslakla birleşebilir (dil listesi "Türkçe — 80 % reviewed" der); kalan inceleme **yalnız `tr.po`'ya dokunan** ayrı küçük PR'lar. 15f İngilizce kaynak metni değiştirmez (taşmayı genişlik/kırpma ile çözer), böylece `tr.po` incelemesiyle çakışmaz; değiştirmek gerekirse incelemeden sonra.
- Yeni eylem yalnız `actions!` listesine (dizinin türü sayıyı kendisi bulur); bu adım eylem eklemez, `Action::ALL` 132 kalır. Menü kimliği gerekirse **3000–3199** (bugünkü tasarımda gerekmiyor: dil satırı ayar penceresinde, satır sağ tık menüsü 14a'nın 2810–2819'u).
- Notlar `docs/superpowers/notes/parts/<parça>.md`'ye (`15a.md` …); denetim maddeleri `docs/superpowers/notes/checklists/15-macos.md` ve `15-linux.md`'ye; paylaşılan followups ve denetim dosyalarına eklenmez.
- PR'lar CI'dan geçmeli; yeni denetimler sıradan `cargo test` olduğu için ek CI adımı gerekmez. Çapraz denetim planın sonunda (`cargo check` macOS ve Linux hedefleri); Docker yok. Windows ekran testleri kullanıcı uzaktayken ya da birincil ekranda, oyun sırasında asla; klavye düzenine ve **sistemin görüntü diline** dokunulmaz (Windows dili değiştirmek oturum kapatır; sistem dili denemeleri kullanıcıya bırakılır, Gezik tarafı `language` ve `LANG` ile denenir).
- Her parçanın sonunda README özellik/performans tablosu; 15f'de `2026-10-12-ozellik-durumu.md` satır 9 ✅.

### 13.2 Parça ayrıntıları

**15a — altyapı**
- *Dosyalar:* `crates/gezik-i18n/` (yeni: `lib, catalog, po, plural, format, pseudo, number, lang`, `build.rs`, `bin/gezik-lang.rs`, `lang/gezik.pot`), `gezik-platform/src/locale.rs` (yeni), `gezik-config/src/{settings, schema}.rs`, şablon (`language`), `gezik/src/{main, settings_window, warnings}.rs`, `gezik/Cargo.toml` (`i-slint-core` `tr`), `THIRD-PARTY.md` (`tr`), `vendor/README.md` (çevirmen yolu notu), `docs/configuration.md`, `docs/translating.md`.
- *Testler:* `.po` okuyucusu (çok satır, kaçış, bağlam, çoğul, başlık, `#~`, BOM, CRLF, bozuk dosya satır numarasıyla); `Plural-Forms` değerlendiricisi (en, fr `n>1`, tr/zh `nplurals=1`, Lehçe, Rusça, Arapça 6 biçim; 0–1000 için tablo; bozuk ifade reddi); biçimleyici Slint'inkiyle aynı sonuç (aynı vakalar `i_slint_core::translations` testleriyle); katman sırası (dosya > gömülü > İngilizce); `lang::choose` (§5.2'nin bütün örnekleri, `C`, `C.UTF-8`, `zh_TW` düşmez, `LANGUAGE` listesi); Slint `@tr` çevirmenle değişir (`tests/slint_translate.rs`); dil değişince `update_all_translations`; sözde dil yer tutucuyu korur; İngilizcede katalog yüklenmez (küresel `None`); `gezik-lang check` hataları.
- *Denetim listesi:* macOS 388–389: (388) Sistem Ayarları ▸ Dil ve Bölge ▸ Uygulamalar ▸ Gezik = Türkçe iken `language = "auto"` → Türkçe (15e'den önce yalnız sözde dil ve satır; madde 15e'de yeniden); `language = "pseudo"` ⌘, satırından seçilince yeniden başlatmadan bütün pencerelerde; (389) `lang/pl.po` bırakılınca listede "Polski", bozuk satır uyarıda. Linux 282: `LANGUAGE=de:en LANG=de_DE.UTF-8` ile açılış (15f'den önce İngilizce, satır "auto (Deutsch)" der), `LANG=C`.

**15b — arayüz metinleri**
- *Dosyalar:* `gezik/ui/**/*.slint`, `gezik/src/{menu_bar, context_menu, palette, settings_window, settings_window/shortcuts, tray, columns, sidebar}.rs`, `gezik-config/src/{shortcuts, schema}.rs`, `gezik-platform/src/{tray/*, linux/sni}.rs`, gerekirse `vendor/i-slint-backend-winit` (macOS uygulama menüsü), `lang/gezik.pot`.
- *Testler:* Slint denetim testi (§6); pot güncel; her `Action` başlığı pot'ta; her şema alanının başlığı ve açıklaması pot'ta (14a'nın bağına ek); seçim etiketi çevrilir, yazılan değer ham (bayt testi); palet çevrilmiş ve İngilizce başlıkla bulur (`yolu kop` ve `copy pa`); sözde dilde `Action::title` çerçeveli.
- *Denetim listesi:* macOS 390–392: (390) sözde dilde menü çubuğunun her menüsü ve uygulama menüsü çerçeveli, kırpılmamış; (391) VoiceOver bir düğmeyi sözde adıyla okur; (392) ayar penceresi sözde dilde: başlık, açıklama, seçim etiketi. Linux 283: sözde dilde sağ tık menüsü ve tepsi menüsü.

**15c — iletiler ve biçimler**
- *Dosyalar:* `gezik/src/**` (iletiler), `gezik-core/src/{lib, view, fields, group, elevated, batch/*}.rs`, `gezik-platform/src/{datetime, fs/describe}.rs`, `gezik-batch`, `gezik-ops/src/task.rs`, `gezik/src/keys.rs`.
- *Testler:* `format_size_in` ve `group_digits` beş dilin ayraçlarıyla, Fransızca `Ko`; `relative_date` çoğulu; `tn!` 0/1/2/5/21/1000 tr/fr/de/Lehçe; tuş adları dil başına, `settings.toml`'a İngilizce yazılır; çevrilmiş metne harf çevirisi yok (grep testi); 17 elle çoğul yerinin kalmadığı (grep `== 1 {`); `stress.ps1` ±%2.
- *Denetim listesi:* macOS 393–394: (393) sözde dilde kopyalama, çakışma sorusu, hata (izinsiz klasöre yazma), durum çubuğu, işlem paneli Geçmiş; (394) sözde dilde araçlar (Compare, Checksums, Batch Rename) ve Bilgi. Linux 284: aynı gezinti kısa haliyle.

**15d — CJK yazı tipi**
- *Dosyalar:* `vendor/i-slint-core/textlayout/sharedparley/{shaping, cache}.rs` ("Gezik patch"), `vendor/README.md`, `gezik/src/main.rs`.
- *Değişiklik:* vendor'a süreç geneli `set_text_locale(Option<&str>)`; `shaping.rs` varsayılan stile `StyleProperty::Locale(dil)` verir; değişince paragraf önbelleği temizlenir (önbellek anahtarına dil girmez, değişim nadir). Gezik'in verdiği dil (karar 12): arayüz dili CJK ise o (`zh-Hans`, topluluk `ja`, `ko`, `zh-Hant`); değilse sistemin tercih listesindeki ilk CJK dil (Japon kullanıcıya `ja`, Çin sistemine `zh-Hans`); hiçbiri yoksa `zh-Hans` (bugün boş çıkan karakterleri kapsar; Japon sistemi zaten `ja` alır). parley yedeği yazı sistemine bakarak seçtiği için Latin metni etkilenmez.
- *Testler:* vendor testi (`crates/gezik/tests/slint_patch.rs`'nin yanında): yerel `zh-Hans` iken `这` için seçilen yazı tipi Han Sadeleştirilmiş'i kapsar (Windows'ta Microsoft YaHei, macOS PingFang SC; Linux'ta sistemde varsa); `ja` iken Japonca yazı tipi; Latin metnin genişliği yerel ayardan bağımsız; seçim kuralı (saf).
- *Denetim listesi:* macOS 395: Çince dosya adları (`测试这检查对齐.txt`) PingFang SC ile, Japonca adlar (`日本語のファイル.txt`) Hiragino ile; sistem Japonca iken Kanji biçimi değişmez. Linux 285: Noto Sans CJK kurulu ve kurulu değilken (`zh-Hans` seçilince not).

**15e — Türkçe**
- *Dosyalar:* `crates/gezik-i18n/lang/tr.po`, `gezik/macos/Info.plist`, `gezik-platform/src/{locale, mac/*}.rs` (`AppleLanguages`), `gezik-platform/src/linux/desktop_entry.rs`, taşma düzeltmeleri (`*.slint`), `docs/translating.md` (sözlüğün Türkçe sütunu).
- *İş:* taslak çeviri (bütün girdiler `fuzzy`), bakımcı gözden geçirmesi (§13.1), `X-Gezik-Review: human`; Türkçe biçim kuralları (cümle düzeni, ek kuralı) tutarlılık geçişi.
- *Testler:* `tr.po` eksiksiz (CI); yer tutucu ve çoğul (`nplurals=1`) uyumu; Türkçe sayı ayraçları uçtan uca (`1.234,5 MB`); `AppleLanguages` yalnız Gezik'in alanına, `auto`'da silinir (sahte tercihlerle); masaüstü dosyasında `Name[tr]`.
- *Denetim listesi:* macOS 396–397: (396) sistem Türkçe iken `auto` → Türkçe, uygulama menüsü (Gezik'in ve AppKit'in öğeleri) bir yeniden açılıştan sonra Türkçe; `language = "en"` → hemen İngilizce, AppKit öğeleri yeniden açılışta; (397) Türkçe ile bir tur: menüler, sorular, ayar penceresi, kırpılan metin yok, i/İ doğru. Linux 286: `LANG=tr_TR.UTF-8` ile tur, GNOME uygulama listesinde "Dosya Yöneticisi".

**15f — makine çevirileri ve son**
- *Dosyalar:* `lang/{de, es, fr, zh-Hans}.po`, taşma düzeltmeleri (`*.slint`; genişlik, `overflow: elide` ve tam metin araç ipucu), `desktop_entry.rs`, `Info.plist` (zaten listede), `docs/translating.md`, `README.md`, `2026-10-12-ozellik-durumu.md`.
- *Testler:* dört dosya eksiksiz, yer tutucu/çoğul uyumu (de/es `n != 1`, fr `n > 1`, zh `nplurals=1`); hepsi `fuzzy` ve `X-Gezik-Review: machine`; dil listesi notu; Fransızca birimler.
- *Denetim listesi:* macOS 398–400: (398) Almanca tur (en uzun dil): ana pencere en küçük boyutunda, ayar penceresi, sorular; kırpılan metin araç ipucunda tam; (399) Basitleştirilmiş Çince tur: bütün metin PingFang SC, boş kutu yok; (400) dil listesi "Deutsch — machine translated" der; bir girdinin `fuzzy`'si `lang/de.po` kopyasında kaldırılınca yüzde artar. Linux 287–288: (287) `LANG=zh_CN.UTF-8` ile Noto Sans CJK SC; (288) `LANG=fr_FR.UTF-8` ile boyutlar `Mo`, sayılar `1 234,5`.

## 14. Test

### 14.1 Birim (saf; her sistemde)

`gezik-i18n` (okuyucu, çoğul, biçimleyici, katman, eşleme, sözde dil, sayılar, gömülü blob gidiş-dönüşü, karma çakışmasızlığı, pot güncelliği, `.po` denetimleri), `gezik-config::schema` (pot bağları), `gezik-core` biçimleri, Slint denetim testi, `slint_translate.rs`, vendor CJK testi.

### 14.2 Platform

- Windows: `GetUserPreferredUILanguages` (İngilizce Windows'ta Türkçe kullanıcı dili ekli ve sırası), Microsoft YaHei, yerel menü hızlandırıcı metinleri.
- macOS: `preferredLanguages` uygulamaya özgü dille, `AppleLanguages`, `CFBundleLocalizations`, PingFang/Hiragino.
- Linux: ortam değişkenleri sırası, Noto Sans CJK yokken, masaüstü dosyası.

### 14.3 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken): sözde dilde tam tur (15b, 15c), Türkçe ve Almanca tur, 100/150/200 % ölçek, Anlatıcı, Çince dosya adları. Sistem dili değiştirilmez.
- **macOS** (`checklists/15-macos.md` 388–400, sonuçlar `macos-test-results.md`'ye) ve **Linux** (`checklists/15-linux.md` 282–288; GNOME ve KDE).

## 15. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| `i-slint-core`'un iç API'si (`tr` özelliği, `with_global_context`, `set_external_translator`) Slint yükseltmesinde değişir | `@tr` çevrilmez | Sürüm `=1.18.1` sabit ve vendor'da; `slint_translate.rs` testi kırılır; `vendor/README.md`'de yükseltme notu; yedek yol: vendor'a küçük bir çevirmen kancası |
| Exe bütçesi 15b/15c (binlerce çağrı yeri) | Sınır aşılır | `t!` genel olmayan tek işleve iner; 15b'nin ilk görevi çağrı yeri maliyetini ölçer; 15c gerekirse sandık sınırında ikiye bölünür |
| Kaçan metin (Rust'ta derleyici yakalamaz) | Çeviride İngilizce parça | Sözde dil turu, Slint testi, 15c'nin ekran listesi; kaçan her metin sonraki PR'da küçük düzeltme |
| Makine çevirisi yanlış ya da garip | Kullanıcı yanlış anlar | `fuzzy` ve "machine translated" notu; düzeltme yolu `lang/`'a dosya bırakmak ya da PR; tehlikeli işlemlerin soruları (silme, üzerine yazma, yönetici) 15f'de bakımcıya ayrı listeyle gösterilir |
| Almanca/Fransızca uzunluk | Kırpılan düğme ve başlıklar | Sözde dil %40, `elide` + araç ipucu, 15f'de Almanca tur |
| Türkçe ekler ve yer tutucu | "{}'ı" gibi bozuk ek | Kılavuz kuralı (§11.4), bakımcı incelemesi |
| `fuzzy`'nin gettext'ten farklı anlamı | Başka araç (msgfmt) çevirileri atar | Gezik msgfmt kullanmaz; anlam `docs/translating.md`'de; Poedit/Weblate'in "needs work" gösterimi aynı |
| Pot her PR'da değişir | Birleştirme çakışması | Satır numarasız, deterministik sıra; parçalar sırayla; 15e incelemesi yalnız `tr.po` |
| Kaynak metin değişince çeviri düşer | Küçük düzeltmede İngilizce görünür | Bilerek (yanlış çeviri göstermektense); `check` düşenleri listeler |
| CJK yerel ayarı Japon kullanıcının Kanji'sini değiştirir | Yanlış biçim | Önce arayüz dili, sonra sistemin ilk CJK dili; Japon sistemi `ja` alır; vendor testi |
| Linux'ta Çince yazı tipi yok | Kutucuk | Not ve kurulum önerisi; yazı tipi taşınmaz (bütçe) |
| Dil değişince Rust'ın tuttuğu metinler eski kalır | Karışık dil | Tek `on_change` kancası, açık soru pencereleri bilinen sınır |
| Kullanıcı `.po`'su kötü niyetli ya da dev | Bellek, yavaşlık | Dosya ≤ 4 MB, girdi ≤ 20.000, metin yalnız gösterilir (yer tutucu dışında yorumlanmaz; `Plural-Forms` değerlendiricisi döngüsüz, derinlik ≤ 64) |

## 16. Bilinen sınırlar

- Sistemin kendi metinleri (Kabuk fiilleri, dosya seçicileri, işletim sistemi hata metinleri) sistem dilinde (§3.6).
- macOS'ta AppKit'in uygulama menüsü öğeleri `language` değişince bir sonraki açılışta izler.
- Windows'ta kayıt defterine yazılan fiil adı açıldığı anki dilde; `Repair` yeniden yazar.
- Açık bir soru penceresi dil değişince kapanana kadar eski dilde kalabilir.
- Sağdan sola dillerde yerleşim aynalanmaz.
- Sistem dili çalışırken değişirse sonraki açılışta izlenir.
- Linux'ta Çince için sistemde CJK yazı tipi gerekir.
- `settings.toml` yorumları İngilizce.
- Makine çevirileri gözden geçirilene kadar "machine translated".

## 17. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 15a, 15b, 15c, 15d, 15e, 15f; sırayla, paralel değil** (§13). *Gerekçe:* kullanıcının kuralı; altyapı küçük ve önce; metin çıkarma büyük olduğu için arayüz/ileti diye ikiye; Çince düzeltmesi küçük ve Çinceden önce; Türkçe kuralları gerçek metinle sınar, makine çevirileri hepsinin üstüne.
2. **Anahtar İngilizce kaynak metin (+ bağlam), sembolik anahtar değil.** *Gerekçe:* Slint `@tr` böyle; eksik çeviride İngilizce kendiliğinden; çevirmen kaynağı görür; anahtar adı tartışması yok. Bedeli: kaynak değişince çeviri düşer, istenen de bu.
3. **Biçim gettext `.po`.** *Gerekçe:* Slint'in kendi araçları ve dünyadaki çeviri araçları (Poedit, Weblate, Crowdin, Lokalize) doğrudan açar; bağlam, çoğul, çevirmen notu ve "gözden geçirilmeli" bayrağı biçimde var; düz metin, bırakılır. TC/Far `.lng` numaraları her sürümde kayar; Fluent sandıkları bütçeyi aşar ve Slint konuşmaz; JSON'da çoğul/not yok.
4. **Kendi çalışma zamanımız (gettext/libintl yok), Slint'e dış çevirmenle bağlı; Slint'in gömülü çevirisi kullanılmaz.** *Gerekçe:* gettext `.mo` derleme adımı ister ve Windows/macOS'ta libintl taşır; gömülü Slint çevirisi yalnız `.slint` metinlerini kapsar, Rust metinlerini ve bırakılan dosyaları kapsamaz. Tek katalog ikisine de hizmet eder.
5. **Gömülü diller karma tablolu, deflate'li blob; yalnız seçili dil açılır.** *Gerekçe:* ham `.po` beş dilde ≈ 700 KB (anahtarlar her dilde tekrar); bu biçim ≈ 140 KB; açılışta ≤ 3 ms ve ayırmasız arama; `miniz_oxide` zaten bağlı.
6. **`settings.toml` şablon yorumları İngilizce; ayar penceresinin açıklamaları çevrilir.** *Gerekçe:* dosya senkronla dilleri farklı makineler arasında paylaşılır, dil değişince kullanıcının dosyasına dokunmamak ilkesi, anahtar/değerler zaten İngilizce, yorumlar `docs/configuration.md` ile aynı belge; pencere teknik olmayan kullanıcının yüzü.
7. **Makine çevirisi `#, fuzzy` + `X-Gezik-Review: machine`; Gezik `fuzzy` girdileri kullanır.** *Gerekçe:* araçların kendi "gözden geçirilmeli" işareti; düzelten kişi bayrağı kaldırır, ayrı bir işaret düzeni yok; kullanmasaydı makine çevirisi hiç görünmezdi.
8. **Topluluk dosyası gömülünün üstüne katman; yeni dil yalnız dosyayla; yerleşik set bakımcı kararı.** *Gerekçe:* kullanıcı kararı (yeniden derleme yok, eksikler İngilizceye); aynı mekanizma yerleşik dilin düzeltmesini de sağlar; her yerleşik dil ≈ 30 KB.
9. **Sayılar arayüz diline göre, `system` tarih biçimi sistemin bölgesine göre.** *Gerekçe:* Almanca cümlede `1.5 KB` yanlış görünür; ayraçlar küçük bir başlık satırı, işletim sistemi API'si (Linux `LC_NUMERIC`, üç farklı yol) gerekmez; `system` tarih biçimi kullanıcının adıyla seçtiği bölge biçimidir.
10. **Kısayol metinleri çevrilir, kısayol yazımı ve ayar dosyası İngilizce.** *Gerekçe:* Almanca Windows menüleri `Strg+C` gösterir; ayar dosyası makineler ve diller arasında aynı kalmalı.
11. **Sözde dil çalışma anında dönüşüm, gömülü dosya yok.** *Gerekçe:* çevrilmemiş metni, taşmayı ve birleştirmeyi tek bakışta gösterir; ≈ 1 KB kod; çevirmenler de kullanır.
12. **CJK düzeltmesi: vendor'da süreç geneli metin yerel ayarı; önce arayüz dili, sonra sistemin ilk CJK dili, sonra `zh-Hans`.** *Gerekçe:* 13a'nın notu düzeltmenin merkezde olması gerektiğini söylüyor; parley 0.11 `Locale`'i destekliyor; sıra Japon kullanıcının Kanji biçimini korur; yazı tipi listesine ad yazmak sisteme göre değişir ve Linux'ta işe yaramaz.
13. **Dil canlı değişir; sistem dili yalnız açılışta okunur.** *Gerekçe:* Slint `@tr` yeniden değerlendirmeyi zaten veriyor, Rust tarafı ayar yeniden yüklemesinin yollarını kullanır; sistem dili olayını izlemek boşta dinleyici demek.
14. **CI denetimi: pot güncelliği, yer tutucular her dilde; Türkçede eksik yok; makine dillerinde eksik serbest.** *Gerekçe:* her gelecek PR yeni metni pot'a ve Türkçeye birlikte koyar (Türkçe hep tam); makine dilleri İngilizceye düşer, bir PR'ı dört dil yüzünden durdurmaz.
15. **Yeni eylem ve menü kimliği yok; 3000–3199 ayrılmış kalır.** *Gerekçe:* dil seçimi ayar penceresinin satırı ve `settings.toml`; menüde ayrı bir dil menüsü nadiren kullanılan bir şey için yer kaplar.
16. **Bütçe: parça başına ≤ +262.144 B, İngilizcede boşta 0, başka dilde ≤ +0,3 MB.** *Gerekçe:* önceki adımların kuralı; çevirinin maliyeti yalnız onu kullanana.
