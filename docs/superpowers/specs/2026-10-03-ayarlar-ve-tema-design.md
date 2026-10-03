# Alt Proje 1: Ayarlar ve Tema Sistemi — Tasarım

- **Tarih:** 2026-10-03
- **Durum:** İncelemede
- **Kapsam:** Gezik yol haritasının 1. alt projesi

## 1. Amaç

Gezik'in bundan sonraki tüm arayüz parçalarının üzerine kurulacağı iki temeli oluşturmak:

1. **Tema sistemi:** Kullanıcılar kendi temalarını yazabilir, birden fazla tema arasında geçiş yapabilir; uygulama birden fazla yerleşik tema sunabilir.
2. **Taşınabilir ayarlar:** Ayar dosyası, hiçbir değişiklik gerekmeden Windows, macOS ve Linux arasında taşınabilir. (Taşıma mekanizmaları — dışa/içe aktarma, senkron klasör, bulut senkronu — sonraki alt projelerdir; bu alt proje onlara uygun dosya biçimini kurar.)

### Başarı ölçütleri

- Bir kullanıcı, belgesine bakarak 5 satırlık bir tema dosyası yazıp uygulamada görebilir.
- Tema veya ayar dosyası kaydedildiğinde değişiklik uygulama yeniden başlatılmadan ekrana yansır.
- Hatalı bir tema veya ayar dosyası uygulamayı çökertmez, açılmasını engellemez; hata kullanıcıya satır numarasıyla gösterilir.
- Bir makinede oluşturulan `settings.toml` başka bir işletim sistemine kopyalandığında aynı şekilde çalışır.
- Performans gerilemesi yok: boşta RAM artışı ≤ 2 MB (bugün: ~5 MB), açılış süresinde belirgin artış yok (bugün: ~40 ms).

### Kapsam dışı (bilerek)

- Grafik ayarlar ekranı (ayrı alt proje; ayar dosyasına yazma da onunla gelir).
- Ayar taşıma mekanizmaları: dışa/içe aktarma, senkron klasör, GitHub'dan tema kurma (Alt proje 5), bulut senkronu (Alt proje 6).
- Sekmelerin başlık çubuğuna taşınması (Alt proje 2'de değerlendirilecek).
- Kısayol sistemi (Alt proje 2). Bu belge yalnızca biçim kuralını sabitler (bkz. 5.4).
- Kullanıcı temalarının pencere düzenini değiştirmesi (yorumlayıcı gerektirir; "hızlı ve hafif" hedefiyle çelişir).

## 2. Alınan kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Tema derinliği | Stil temaları + ayarlarda hazır düzen seçenekleri | Yorumlayıcı gerektirmez, hafif ve güvenli; en çok istenen düzen değişikliklerini karşılar |
| Düzen seçeneklerinin yeri | `settings.toml` (temada değil) | Tema değiştirmek kenar çubuğunu yerinden oynatmamalı |
| Uygulama yöntemi | Slint `global Theme` + Rust'tan değer atama (Yaklaşım A) | Sıfıra yakın maliyet; canlı yenilemeye uygun |
| Dosya biçimi | TOML | İnsan tarafından okunur ve yazılır, yorum satırlarını destekler |
| Tema paylaşımı | GitHub linkleri (Alt proje 5) | Kendi sunucumuzda tema barındırmaya gerek yok |
| Bulut senkronu | Gizli senkron kodu + cihazda şifreleme (Alt proje 6) | Hesap gerektirmez; sunucu içeriği okuyamaz |

## 3. Dosyalar ve konumları

### 3.1 Ayar klasörü

| Sistem | Konum |
|---|---|
| Windows | `%APPDATA%\gezik\` |
| macOS | `~/Library/Application Support/gezik/` |
| Linux | `$XDG_CONFIG_HOME/gezik/` (varsayılan `~/.config/gezik/`) |

Konum `dirs::config_dir()` ile bulunur.

```
gezik/
  settings.toml     taşınabilir; senkronlanabilir
  state.toml        bu makineye özel; asla senkronlanmaz
  themes/
    ornek-tema.toml ilk açılışta oluşturulan açıklamalı şablon
    *.toml          kullanıcı temaları
```

### 3.2 `settings.toml` (taşınabilir)

```toml
theme = "auto"            # "auto" | tema adı
theme-light = "light"     # auto modunda sistem açık temadayken
theme-dark  = "dark"      # auto modunda sistem koyu temadayken

[layout]
sidebar = "left"          # left | right | hidden
density = "comfortable"   # compact | comfortable
```

Kural: Bu dosyada makineye özel hiçbir şey bulunmaz. Klasör yolları yalnızca 5.3'teki işaretli biçimde saklanır.

Bu alt projede uygulama `settings.toml` dosyasına **yazmaz**, yalnızca okur. (İstisna: ilk açılışta dosya yoksa açıklamalı varsayılan dosya oluşturulur.)

`sidebar` değeri bu alt projede okunur ve doğrulanır; kenar çubuğu Alt proje 2'de eklendiğinde kullanılacaktır.

### 3.3 `state.toml` (makineye özel)

```toml
[window]
width = 900
height = 600
x = 120
y = 80
```

Pencere kapanırken yazılır, açılırken okunur. Kaydedilen konum artık hiçbir ekranda görünmüyorsa (ör. ikinci monitör çıkarıldıysa) yok sayılır.

### 3.4 Güvenli yazma

Tüm yazmalar önce aynı klasördeki geçici bir dosyaya yapılır, ardından hedef dosyanın yerine taşınır (rename). Böylece yarım yazılmış dosya oluşmaz; bu, ileride senkron klasörlerde özellikle önemlidir.

## 4. Tema sistemi

### 4.1 Tema dosyası

```toml
name = "Nord"
base = "dark"                    # eksik değerler buradan; varsayılan "dark"

[colors]                         # "#rrggbb" veya "#rrggbbaa"
background           = "#2e3440"
surface              = "#3b4252"
foreground           = "#eceff4"
foreground-muted     = "#9aa3b5"
border               = "#434c5e"
accent               = "#88c0d0"
accent-foreground    = "#2e3440"
selection            = "#434c5e"
selection-foreground = "#eceff4"
hover                = "#3b425280"
folder-icon          = "#ebcb8b"
file-icon            = "#9aa3ad"
danger               = "#bf616a"

[metrics]
font-family = "Inter"            # bulunamazsa sistem varsayılanı
font-size   = 13                 # 8–32
row-height  = 26                 # 16–64
icon-size   = 16                 # 12–48
radius      = 6                  # 0–16
spacing     = 6                  # 0–24
```

| Renk | Kullanım yeri |
|---|---|
| `background` | Dosya listesi zemini |
| `surface` | Araç çubuğu, durum çubuğu, (ileride) kenar çubuğu ve sekmeler |
| `foreground` / `foreground-muted` | Ana metin / ikincil metin (boyut, tarih) |
| `border` | Ayırıcı çizgiler, metin kutusu çerçevesi |
| `accent` / `accent-foreground` | Odak çerçevesi, aktif öğeler / vurgu üzerindeki metin |
| `selection` / `selection-foreground` | Seçili satır |
| `hover` | Fareyle üzerine gelinen satır ve düğme |
| `folder-icon` / `file-icon` | Geçici ikon renkleri (gerçek ikonlar Alt proje 3'te) |
| `danger` | Silme gibi geri alınamaz işlemler |

### 4.2 Tema adı ve çözümleme

- Tema adı, dosya adından gelir (`themes/nord.toml` → `nord`). `name` alanı yalnızca görünen addır.
- Yerleşik temalar: `light` ve `dark`. Programa gömülüdür (`include_str!`); tema klasörü silinse bile uygulama çalışır.
- Kullanıcı teması yerleşik bir temayla aynı adı taşıyorsa kullanıcı teması kazanır (yerleşik temayı düzenlemenin yolu budur).
- `auto` ayrılmış bir addır; `themes/auto.toml` yok sayılır ve uyarı verilir.
- `base` zinciri izlenir: `nord → dark`. Her değer, zincirde onu tanımlayan ilk temadan gelir. Zincirin sonu her zaman yerleşik bir temadır; `base` belirtilmemişse, tema aynı adlı bir yerleşik temayı eziyorsa (ör. kullanıcının `light.toml` dosyası) o yerleşik tema, aksi halde `dark` kabul edilir.
- Döngü (`a → b → a`) veya bulunamayan `base` → uyarı; zincir o noktada kesilir ve `dark` ile tamamlanır.
- `theme = "auto"`: sistem açık/koyu moduna göre `theme-light` veya `theme-dark` uygulanır; sistem modu değişince anında geçilir.

### 4.3 Yoğunluk

`density = "compact"`, temanın `row-height` ve `spacing` değerlerini 0,8 ile çarpar (alt sınırlar korunarak). `comfortable` değerleri olduğu gibi kullanır.

### 4.4 Hata davranışı

Temel ilke: **Tema veya ayar yüzünden uygulama asla çökmez veya açılmamazlık yapmaz.**

| Durum | Davranış |
|---|---|
| TOML söz dizimi hatası (tema) | Tema yüklenmez, o an geçerli tema kalır (açılışta: `dark`). Uyarı: `nord.toml satır 12: ...` |
| TOML söz dizimi hatası (`settings.toml`) | Varsayılan ayarlar kullanılır, dosyaya dokunulmaz. Satır numaralı uyarı |
| Geçersiz tek değer (`"#zzz"`, aralık dışı sayı, bilinmeyen enum) | O değer yok sayılır, `base`'den gelir. Uyarı |
| Bilinmeyen anahtar | Sessizce yok sayılır (ileri uyumluluk) |
| Seçili tema bulunamadı | `dark` kullanılır. Uyarı |
| Ayar klasörü oluşturulamıyor / yazılamıyor | Varsayılanlarla çalışılır, uyarı gösterilir |

Uyarılar durum çubuğunda gösterilir; birden fazlaysa ilki ve toplam sayı görünür. Tüm uyarılar ayrıca standart hata çıktısına yazılır.

### 4.5 Canlı yenileme

`settings.toml` ve `themes/` klasörü `notify` ile izlenir. Kısa bir süre içinde gelen değişiklikler biriktirilir (~150 ms) ve tek bir yeniden yüklemeye dönüştürülür. Yeniden yükleme arka plan iş parçacığında ayrıştırılır, sonuç arayüz iş parçacığında uygulanır.

## 5. Taşınabilirlik kuralları

### 5.1 Ayrım

Senkronlanabilecek her şey `settings.toml` ve `themes/` içindedir; makineye özel her şey `state.toml` içindedir.

### 5.2 Biçim

Değerler işletim sisteminden bağımsızdır: renkler, sayılar, enum'lar ve işaretli yollar. Sistem yolu ayırıcıları (`\` / `/`) dosyada her zaman `/` olarak yazılır.

### 5.3 İşaretli yollar

Ayarlarda saklanacak klasör yolları (ilk kullanım: Alt proje 2'deki sabitlenen klasörler) şu işaretlerle yazılır:

| İşaret | Karşılığı |
|---|---|
| `{home}` | `dirs::home_dir()` |
| `{desktop}` | `dirs::desktop_dir()` |
| `{documents}` | `dirs::document_dir()` |
| `{downloads}` | `dirs::download_dir()` |
| `{pictures}` | `dirs::picture_dir()` |
| `{music}` | `dirs::audio_dir()` |
| `{videos}` | `dirs::video_dir()` |

- **Kaydederken (collapse):** Yol, işaretlerden hangisinin altındaysa en uzun eşleşen işaretle yazılır (`C:\Users\a\Documents\Projeler` → `{documents}/Projeler`). Eşleşme yoksa mutlak yol olduğu gibi saklanır.
- **Okurken (expand):** İşaret bu makinedeki karşılığıyla değiştirilir. Sonuçta oluşan yol yoksa öğe gösterilmez ama ayarlardan silinmez (başka makinede var olabilir).
- Bu alt proje yalnızca dönüştürme fonksiyonlarını ve testlerini içerir; kullanımı Alt proje 2'dedir.

### 5.4 Kısayol biçimi (yalnızca kural)

Kısayollar `mod+shift+t` biçiminde yazılır. `mod`, Windows ve Linux'ta Ctrl, macOS'ta Cmd anlamına gelir. Uygulaması Alt proje 2'dedir.

## 6. Kod yapısı

```
crates/
  gezik-core/                 değişmez
  gezik-config/               YENİ — arayüzden bağımsız
    src/lib.rs
    src/paths.rs              ayar klasörü; işaretli yollar (KnownDirs ile test edilebilir)
    src/settings.rs           Settings, State; okuma, varsayılanlar, güvenli yazma
    src/theme.rs              Color, Metrics, Theme; ayrıştırma, base zinciri, doğrulama, uyarılar
    themes/light.toml         yerleşik
    themes/dark.toml          yerleşik
    templates/settings.toml   ilk açılış şablonu (açıklamalı)
    templates/ornek-tema.toml ilk açılış şablonu (açıklamalı)
  gezik/
    ui/theme.slint            global Theme
    ui/widgets/button.slint   Theme'den beslenen kendi bileşenlerimiz
    ui/widgets/text-field.slint
    ui/app.slint              tüm sabit renk/ölçüler Theme'e taşınır
    src/main.rs
    src/theme_bridge.rs       ResolvedTheme → Slint Theme global; sistem açık/koyu takibi
    src/watcher.rs            notify tabanlı, biriktiren dosya izleyici
```

**Birim sınırları**

- `gezik-config` Slint'i bilmez; girdisi dosyalar/metin, çıktısı `ResolvedTheme` (tüm değerleri dolu, doğrulanmış) ve uyarı listesidir.
- `theme_bridge` yalnızca `ResolvedTheme`'i Slint global'ine kopyalar; doğrulama yapmaz.
- `watcher` neyin değiştiğini bilmez; yalnızca "yeniden yükle" sinyali üretir.

**Yeni bağımlılıklar:** `serde` (derive), `toml`, `dirs`, `notify`.

**Doğrulanacak risk:** Slint hazır bileşenlerinin (`ListView` kaydırma çubuğu) `Palette` renklerinin dışarıdan atanıp atanamadığı. Atanabiliyorsa Theme'den beslenir; atanamıyorsa yalnızca `Palette.color-scheme` (açık/koyu) temanın `base` değerine göre ayarlanır. Düğme ve metin kutusu her durumda kendi bileşenlerimizdir.

## 7. Test stratejisi

### 7.1 `gezik-config` birim testleri

- Tam tema ayrıştırma; tüm değerlerin doğru okunması
- Kısmi tema + `base`: eksik değerlerin zincirden gelmesi, çok seviyeli zincir
- Geçersiz renk / aralık dışı sayı / bilinmeyen enum → uyarı + `base` değeri
- Söz dizimi hatası → satır numaralı hata, tema yüklenmez
- `base` döngüsü ve bulunamayan `base` → uyarı, `dark` ile tamamlanma
- Bilinmeyen anahtarlar → uyarı yok, hata yok
- Kullanıcı temasının yerleşik temayı ezmesi
- Yoğunluk ölçeklemesi ve alt sınırlar
- İşaretli yollar: collapse/expand gidiş-dönüşü, en uzun eşleşme, eşleşmeyen yol, sahte `KnownDirs` ile her işletim sisteminde aynı test
- `settings.toml` yokken / bozukken varsayılanlar
- Güvenli yazma: yazma sonrası içerik doğru, geçici dosya kalmıyor

### 7.2 Uygulama doğrulaması

- `light`, `dark` ve örnek bir kullanıcı temasıyla ekran görüntüsü
- Tema dosyası kaydedildiğinde canlı yenileme (önce/sonra ekran görüntüsü)
- Bozuk tema dosyasında uygulamanın açılması ve uyarının görünmesi
- `state.toml`: pencere boyutunun yeniden açılışta korunması

### 7.3 Performans

`measure.ps1` ve `stress.ps1` ile ölçüm. Kabul: boşta RAM artışı ≤ 2 MB, açılış süresinde belirgin artış yok, 100 bin dosyada bellek ve kaydırma maliyetinde gerileme yok.

## 8. Yol haritasındaki yeri

| # | Alt proje |
|---|---|
| **1** | **Ayarlar + Tema sistemi (bu belge)** |
| 2 | Gezinme: sekmeler, kenar çubuğu, geri/ileri + fare yan tuşları, breadcrumb, kısayollar |
| 3 | Görünüm: sütunlar, sıralama (doğal), çoklu seçim, ikonlar, ızgara, önizleme |
| 4 | Dosya işlemleri: kopyala/taşı/sil, çakışmalar, sürükle-bırak, sağ tık menüsü |
| 5 | Taşınabilirlik: dışa/içe aktarma, senkron klasör, GitHub'dan tema kurma |
| 6 | Bulut senkronu: gizli kod, cihazda şifreleme, istemci + sunucu |
| 7 | Gelişmiş: arama, çift panel, arşivler, etiketler, Git, komut paleti |
