# commands-more: `{other}`, `{other-files}`, iç içe menü başlıkları — notlar

Özellik durumu satır 30 ("`[[commands]]` büyütme"): 6b'de (#14) kalan parçalar.

## Ne yapıldı

- **`{other}`**: diğer bölmenin klasörü. Her komutta (öğe başına ya da `{files}`) kullanılabilir; bir argümanın içinde de olur (`--to={other}`).
- **`{other-files}`**: diğer bölmede seçili öğeler, tam yol, her biri ayrı argüman. `{files}` ile aynı kurallar: kendi başına bir argüman olmalı, program olamaz, `{in}`, `{name}`, `{ext}`, `{out}` ve `output` ile kullanılamaz; komut bir kez çalışır, geri alınamaz. `{files}`, `{other}`, `{dir}`, `{outdir}` ile birlikte olur; tek başına da olur (`["cp", "{other-files}", "{dir}"]`). Kurallar yükleme sırasında `check_command`'da denetlenir (hata iletileri artık hangi yer tutucuysa onu söyler: `{{{}}} can't be the program` vb.).
- **Nasıl dolar**: komut çalışmadan hemen önce (`run_asked`'ta soru ve "çok uzun" denetiminden önce, Dönüştür katmanından gelince `run_command`'da) `dual::for_command()` diğer bölmenin klasörünü ve seçimini verir, `with_other` (gezik-core) bunları `run` metnine düz yazı olarak koyar (yoldaki `{` `}` ikilenir); kalanını eski `expand_command` / `expand_files_command` yapar. `{files}` olmadan `{other-files}` kullanan komut doldurulunca "bir kez" bilgisini yitirir: bunu `CommandTask::once` taşır.
- **Reddedilenler** (durum çubuğunda "Ad — neden"): tek bölme ("needs the second pane" / "ikinci bölme gerekiyor"); diğer bölme diskte bir klasör göstermiyor (arşiv içi, MTP/libmtp telefonu, arama sonuçları, çöp, "Bu bilgisayar"; `view.folder()` yalnız `Listing::Files` için bir şey döndürür); `{other-files}` için diğer bölmede seçim yok ("select the items in the other pane"; odaktaki öğe sayılmaz, yalnız gerçek seçim); Unicode olmayan bir yol (`run` metin olduğu için).
- **İç içe menü**: `menu = "Images/Resize"` ayrıştırılırken parçalara bölünür, kırpılır, boş parçalar atılır (`" / Images //Resize "` → `Images/Resize`; yalnız `/` → menü yok). "Komutlar ▸" içinde başlık `Images ▸ Resize` diye görünür; bir alt grup, üst grubunun hemen ardından gelir (gruplar ilk göründükleri sırada). Sağ tık menüsü (Windows yerel kabuk menüsü, Slint menüleri, Linux'ta Gezik'in kendi menüsü) ve macOS menü çubuğunun Komutlar menüsü aynı `grouped` sırasını kullanır. Komut paleti başlık göstermiyor, değişmedi.
- Şablon (`templates/settings.toml`), `docs/features.md`, tr ve de/es/fr/zh-Hans katalogları (yeni dizgiler; tr elle, diğerleri `fuzzy` makine çevirisi), özellik durumu satır 30 → ✅.

## Sapmalar

1. **Gerçek alt menü değil, yol başlığı**: "Komutlar ▸ Images ▸ Resize" ayrı bir alt alt menü olarak açılmıyor; "Komutlar ▸" içinde gri `Images ▸ Resize` başlığı ve altında komutlar. Neden: alt menü düzeneği her yerde tek düzey (`Submenu` düz liste; app.slint'te 5 sabit `menu-subs` yuvası; Windows `ShellSubmenu`; Gezik'in kendi açılır menüsü; macOS menü çubuğu düz liste). Gerçek iç içe menü dört çizicide değişiklik isterdi; görevin "mevcut alt menü düzeneğini kullan" kuralına uyuldu. Derinlik sınırı yok (başlık 60 karakterde kesilir, `MENU_NAME_MAX`).
2. **`/` artık ayırıcı**: eskiden `menu = "Audio/Video"` tek başlıktı, şimdi `Audio ▸ Video`. Düz `/` yazmanın yolu yok (şablonda yazıyor).
3. **`{other-files}` diğer bölmede odaktaki öğeyi almaz**: yalnız seçilenler (iki dosya aracı için daha güvenli).

## Bilinen sınırlar

- Öğe başına `{other}` kullanan ve `output`'u olmayan komut (`["cp", "{in}", "{other}"]`) yine "yerinde değiştirir" sayılır: özgün öğe Geri Al için çöpe gider, kopyası yerine konur; klasörlerde gri. Şablonda yazıyor.
- `CommandTask::resources()` diğer bölmenin yollarını bildirmez: iş kuyruğu o yollardaki çakışmaları görmez.
- Yeniden dene (panelde "Again") ilk çalıştırmadaki diğer bölme bilgisini kullanır, o anki bölmeyi değil.

## Boyut

Release exe (Windows, `cargo build --release -j 4 -p gezik`): master 31,810,560 B → 31,834,112 B, **+23,552 B** (bütçe +262,144 B).

## Testler

- `cargo test -p gezik-core -p gezik-config -p gezik-batch -p gezik`: hepsi geçti (yeni: `the_other_pane_goes_in_before_the_rest`, `other_files_stands_alone`, `a_group_under_another_follows_it_with_its_path`, settings ayrıştırma testinde `menu` normalleştirme, şablon örneği).
- `cargo clippy --all-targets -j 4 -- -D warnings`, `cargo fmt --check`: temiz. `gezik-lang check`: eksik çeviri yok.

## Ekran testi (Windows)

`%TEMP%\gezik-gui-cmds\` (bu dalın release'i; `Args both` = `{files} {other-files} {other}`, `menu = "Test/Deep"`; `Args other` = `{in} {other}`, `menu = "Test"`; `binrgs.ps1` argümanları `args.txt`'e yazar).

| # | Adım | Sonuç | Kanıt |
|---|---|---|---|
| 1 | Sağ tık ▸ Commands | PASS | Gri "Test" başlığı, altında `Args other`; gri "Test ▸ Deep" başlığı, altında `Args both` |
| 2 | Tek bölmede `Args other` | PASS | Hiçbir şey çalışmadı; durum çubuğu "Args other — needs the second pane" |
| 3 | F3, sağ bölme `right`'ta `c {x}.txt` seçili; **pasif** sol bölmede doğrudan sağ tık `a.txt` ▸ `Args other` | PASS | Sağ tık sol bölmeyi etkin yaptı (başlık "left — Gezik"); `args.txt`: `left.txt`, `right`; `a.txt` yerinde |
| 4 | Solda `a.txt` + `b.txt` seçili ▸ `Args both` | PASS | Dört satır: `left.txt`, `left.txt`, `right\c {x}.txt` (tek satır, ayraçlar bozulmadan), `right` |
| 5 | Sağ bölmede seçim yok ▸ `Args both` | PASS | Hiçbir şey çalışmadı; durum çubuğu "Args both — select the items in the other pane" |

macOS/Linux maddeleri: `../checklists/commands-more-macos.md` (413–415), `../checklists/commands-more-linux.md` (297–299).
