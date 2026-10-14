# update: otomatik güncelleme — notlar

Spec: `../../specs/2026-10-14-guncelleme-design.md`. Dal: `feat/auto-update`.

## Ne yapıldı

- **Saf mantık** (`gezik-core/src/update.rs`): `Version` (semver; `v` + sürüm olmayan etiket atlanır, metin klasör adı olarak güvenli: yalnız `[0-9A-Za-z.-]` ve `+`), semver sırası (`rc.10 > rc.9`, `0.1.0 > 0.1.0-rc.3`, `+build` sırada yok sayılır), `Channel` (`auto`/`stable`/`pre`), GitHub yanıtının `serde_json` ile okunması (`tag_name`, `draft`, `prerelease`, `html_url`, `assets[].name/browser_download_url/size`), `newest` (taslak atlanır, kararlı kanalda `prerelease` işaretli ya da `-` parçalı olanlar atlanır, yalnız çalışandan büyük), `Package` (kurulum biçimine göre varlık adı), `SHA256SUMS.txt` satırı (mevcut `checksum_file` ayrıştırıcısı).
- **Uygulama** (`gezik/src/update.rs`): ilk Gezik'te (ikincil süreçte ve salt okunur taşınabilir klasörde değil), ilk pencereden 10 s sonra kendi iş parçacığında: önce artıklar (önceki kurulumun `.old`'u, `update/installed` işareti → "Updated to X · What's new", bu sürüm ve öncesinin klasörleri), sonra `[update] check` açıksa ve `state.toml`'daki `[update] checked` 20 saatten eskiyse istek. İndirme `<config local>/update/<sürüm>/` içine (`.part` → boyut + SHA-256 tutunca asıl ad), açma `gezik_batch::archive` ile `x/` altına (her ad `safe_join`'den geçer; dışarı çıkan bir girdi açmayı başarısız kılar), `--version` denetimi (15 s sınır, `ChildProcess`: pencere yok), `ready` işareti.
- **Kurulum**: süreç sonunda (`main`'in sonu; macOS ⌘Q/Dock Quit yolunda `terminate` kancası), `swap`: yeni dosya önce hedefin yanına `.new` olarak kopyalanır, eski `.old`'a çevrilir, `.new` yerine konur; herhangi bir adım başarısız olursa eski yerinde kalır. Windows'ta çalışan exe'nin adı değişir; `.old` yeni sürüm açılınca silinir. Başka bir süreç hâlâ `.old`'dan çalışıyorsa kurulum yapılmaz, sonraki kapanışta yeniden denenir (`tidy` hazır bekleyeni bulur).
- **Şimdi yeniden başlat**: tepsi menüsünün Quit'i gibi kapanır (çalışan işler önce sorulur), kurar, yeni Gezik'i argümansız başlatır. Yeni süreç `GEZIK_RESTARTED_FROM` ile eskisinin bitmesini bekler (en çok 10 s), sonra tek örnek kanalına bakar: kanal hâlâ eskide olduğu için isteği ölen sürece vermez.
- **Yazılamayan kurulum**: hedef klasörde geçici dosya denemesi; macOS'ta `/Volumes/` ve `/AppTranslocation/` doğrudan hayır. O zaman indirme yok, yalnız "Gezik X is out · Download". Paket bulunamazsa (ör. bundle dışında çalışan macOS ikilisi) da aynısı.
- **Arayüz**: durum çubuğunda not (metin, Restart now, What's new/Download, ✕); sürüm başına bir kez (`[update] told`). Bağlantı yalnız `https://github.com/` ile başlıyorsa açılır. Ayarlar ▸ Genel'in sonunda `update.check`, `update.channel` ve "Updates" satırı (son sonuç, sürüm ve son denetim zamanı, **Check Now**). `check-updates` eylemi (`actions!`, tuş yok): palet ve macOS menü çubuğunda yeni **Help** menüsü. Elle denetim sonucu her durumda söyler ("Gezik is up to date", "Could not check for updates: …").
- **Release**: macOS işine `gezik-<v>-macos-universal.app.tar.gz` adımı (`COPYFILE_DISABLE=1`, `--no-mac-metadata`, içinde `Gezik.app/Contents/MacOS/gezik` var mı denetimi); `SHA256SUMS.txt` onu da kapsar.
- **Belgeler**: `docs/releasing.md` (yayımlamak herkese gönderir, geri çekme, geri alma = daha büyük sürüm, dosya adları), `docs/configuration.md` (`## Updates`, `update/` klasörü), `docs/features.md`, `docs/shortcuts.md`, README (günde bir istek, nasıl kapatılır), şablon `[update]` ve `check-updates` satırı.
- **Test kancası**: `GEZIK_UPDATE_FEED` yalnız `debug_assertions` ya da `gezik_measure` cfg'si olan derlemede okunur. `gezik_measure`'ı `build.rs` verir: `OUT_DIR`'in üç üstü `measure` ise. Bu modda varlık adresi `https://` değilse yerel yol olarak kopyalanır.
- Çeviriler: tr elle; de/es/fr/zh-Hans `fuzzy` makine çevirisi; `gezik-lang check`: eksik yok.

## Sapmalar

1. **İstek başlıkları eklenmedi** (`Accept`, `User-Agent: Gezik/<sürüm>`): `http::download` başlık almıyor; üç sistem için başlık eklemek her arka uçta değişiklik demek. GitHub başlıksız da aynı JSON'u veriyor; User-Agent zaten var (WinHTTP "Gezik", curl/wget/NSURLSession kendi adlarıyla).
2. **`--version` zaten vardı**, eklenmedi. Pencere alt sistemli exe borulu stdout'a yazıyor (ekran testinde doğrulandı).
3. **Yardım menüsü yalnız macOS'ta**: Windows ve Linux'ta menü çubuğu yok; orada palet ve Ayarlar'daki **Check Now**.
4. **Ayar başlığı "Check"**: ayar penceresinde başlık anahtardan gelir (`update.check` → "Check"); spec'teki "Check for updates automatically" yerine açıklama satırı anlatıyor. Türkçesi "Güncellemeleri kendiliğinden denetle".
5. **"Updates" satırı** yedekleme satırlarının (`backup::Row::Updates`) düzeneğiyle: Genel sayfasında alanlardan sonra; arama her sayfadan bulur. Seçim düğmesi ("Check Now" bir seçenek), diğer eylem satırları gibi.
6. **"Updated to" notu** ilk açılıştan 10 s sonra gelir (denetimle aynı iş parçacığı; açılışta dosya işi yok).
7. **Bekleme ortam değişkeni** (`GEZIK_RESTARTED_FROM`): spec'te yoktu; Şimdi yeniden başlat'ın yeni Gezik'i ölen sürecin kanalına teslim etmemesi için gerekli.
8. **Yalnız release paketleri kendini değiştirir**: `release.yml` derlemeye `GEZIK_RELEASE_BUILD=1` verir (`option_env!`); kaynaktan derlenen, `cargo install` ya da paket yöneticisinin derlediği Gezik yalnız "is out" der (spec §8: paket yöneticilerine karışılmaz). Ekran testi derlemeleri (`GEZIK_UPDATE_FEED` ile) de kurabilir.
9. **Unix'te tek dosya**: `.old` sabit bağlantı (hard link) olur, yeni dosya `rename` ile üstüne gelir: programın hiç olmadığı an yok. Windows ve macOS bundle'ı iki adımlı ad değişimiyle.

## Bilinen sınırlar

- Kendi imzamız yok (spec §8): SHA-256 dosyanın Release'teki hâli olduğunu gösterir, kimin koyduğunu değil.
- Güncelleyici rc.3 ile başlar; öncekiler bir kez elle güncellenir.
- Windows oturum kapanışında (`save_on_session_end`) kurulum yapılmaz; sonraki kapanışta yapılır.
- İki süreç aynı anda denetlerse (tek örneği atlayan dbus/arka plan başlatmaları) aynı `update/<sürüm>/` klasörünü paylaşabilir; kilit yok (nadir). `prepare` başta `ready`'yi siler: yarıda kalan açma hazır görünmez.
- Bir release etiketi `release.yml`'in kabul edip `Version::parse`'ın reddettiği biçimdeyse (`1.0.0-rc..1`) hiç önerilmez.

## İnceleme (ayrı alt ajan) ve düzeltmeler

1. `$APPIMAGE` her AppImage'ın başlattığına geçer: tar kurulumu bir AppImage terminalinden açılınca o başka AppImage'ı değiştirebilirdi → yalnız `APPDIR` altındaki exe için kabul.
2. Kaynaktan derlenen Gezik kendini release ile değiştirirdi → sapma 8.
3. "Şimdi yeniden başlat" sonrası çalışan iş sorusunda "Keep open" seçilince yeniden başlatma bayrağı kalıyordu → soru önce soruluyor, bayrak yalnız çıkış gerçekten olursa.
4. Önceki denemenin `ready` işareti yarım açmayı hazır gösterebilirdi → `prepare` başta siler.
5. Veri klasörü yokken Check Now "Checking…"de kalıyordu → hata söylenir.
6. Unix'te iki `rename` arasında program yoktu → sapma 9.
7. Saat geri alınınca otomatik denetim durabilirdi → `abs_diff`.

## Boyut

Release exe (Windows, `cargo build --release -j 4 -p gezik`): master 31,837,696 B → 31,996,928 B, **+159,232 B** (bütçe +262,144 B). `serde` + `serde_json` (artık normal bağımlılık) bunun bir parçası.

## Testler

- `gezik-core` `update`: 8 test (semver sırası, bozuk sürümler, kanal, taslak/işaret/bozuk etiket, varlık adları, SHA256SUMS, gerçek API yanıtıyla uçtan uca: `testdata/releases.json`, 2026-10-14, kırpılmış).
- `gezik` `update`: 9 test (dosya ve bundle değişimi, `.old` korunur, yarıda hata eskiyi bırakır, Windows'ta çalışan exe'nin değişimi ve çalışan `.old`'un bir sonrakini durdurması, yazılabilirlik, SHA-256 tutmayan indirme silinir, `../` girdili zip kurulmaz, ilk açılış notu bir kez ve eski klasörlerin silinmesi). Ağ yok: yerel yollar.
- `gezik-config`: `[update]` ayrıştırma ve `state.toml` `[update]` yazma/okuma; eylem sayısı 145.
- `cargo test -p gezik-core -p gezik-config -p gezik-i18n -p gezik`, `cargo clippy --all-targets -j 4 -- -D warnings`, `cargo fmt --check`: temiz. `gezik-lang check`: eksik çeviri yok.

## Ekran testi (Windows)

`%TEMP%\gezik-gui-update\`: `measure` profili, eski exe `0.1.0-rc.2`, yeni exe sürümü `0.1.0-rc.99`'a çekilmiş aynı dal; `feed\` içinde release.yml gibi yapılmış zip, `SHA256SUMS.txt` ve yerel yollu `releases.json`; `GEZIK_UPDATE_FEED` ve `GEZIK_CONFIG_DIR` ile.

| # | Adım | Sonuç |
|---|---|---|
| 1 | Eski exe açıldı, ~15 s | ✅ "Gezik 0.1.0-rc.99 is ready — installs when you close it · Restart now · What's new"; `update\0.1.0-rc.99\x\…\gezik.exe` ve `ready` var; `state.toml` `[update] checked`, `told` |
| 2 | Alt+F4 | ✅ `app\gezik.exe --version` = 0.1.0-rc.99, `gezik.exe.old` eski; `update\installed` yazıldı |
| 3 | Yeniden açıldı | ✅ ~10 s sonra "Updated to 0.1.0-rc.99 · What's new"; `.old` ve `update\0.1.0-rc.99\` silindi |
| 4 | Eski exe geri, `[update]` silindi, açıldı, **Restart now** | ✅ Pencere kapandı, yeni süreç (0.1.0-rc.99) "Home" sekmesiyle açıldı, "Updated to" notu, `.old` silindi |
| 5 | Klasöre yazma yasağı (`icacls /deny (WD,AD)`), açıldı | ✅ Yalnız "Gezik 0.1.0-rc.99 is out · Download"; indirme yok; kapanınca exe değişmedi; yasak sonra kaldırıldı |
| 6 | Palet ▸ "Check for Updates" | ✅ "Checking for updates…", sonra not yeniden ("is out") |
| 7 | Ayarlar ▸ Genel (paletten) | ✅ Check (kutucuk), Channel (auto), Updates satırı: "Gezik 0.1.0-rc.99 is out", "Gezik 0.1.0-rc.2. Last check: 2026-10-14 13:39" |
| 8 | Notun düzeni | İlk derlemede bağlantılar arasında boşluk vardı (iç düzen esniyordu): `horizontal-stretch: 0` ile düzeldi, yeniden bakıldı ✅ |
| 9 | ✕ ile notu kapatmak | ⏭️ Atlandı: kullanıcı o an başka bir uygulamada yazıyordu, girdi gönderilmedi; test Gezik'i durduruldu |

Yukarıdaki adımlar incelemeden önceki derlemeyle yapıldı. **Birleşmeden sonra `master` bf78acbd ile yeniden** (`measure`, eski exe `0.1.0-rc.2`, yeni exe sürümü `0.1.0-rc.99`'a çekilmiş aynı kaynak, yeniden yapılmış zip, `SHA256SUMS.txt` ve `releases.json`):

| # | Adım | Sonuç |
|---|---|---|
| 10 | Eski exe açıldı, ~19 s | ✅ "Gezik 0.1.0-rc.99 is ready — installs when you close it · Restart now · What's new"; `update\0.1.0-rc.99\…\gezik.exe` ve `ready` |
| 11 | Notta ✕ | ✅ Not kapandı |
| 12 | Alt+F4 (not kapatılmışken) | ✅ Kurulum yine yapıldı: `app\gezik.exe --version` = 0.1.0-rc.99, `gezik.exe.old` = 0.1.0-rc.2; `update\installed` |
| 13 | Yeniden açıldı | ✅ "Updated to 0.1.0-rc.99 · What's new"; `.old` ve `update\0.1.0-rc.99\` silindi |
| 14 | Eski exe geri, `[update]` silindi, açıldı, **Restart now** | ✅ Yeni süreç (başka PID) aynı sekmelerle açıldı, "Updated to 0.1.0-rc.99", `.old` silindi |

Bu turda düzeltilenler: düzen (8); elle denetimde "Checking for updates…" notunun sonuç notu gelince kalması (artık siliniyor).
