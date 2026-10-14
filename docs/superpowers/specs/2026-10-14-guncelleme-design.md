# Otomatik güncelleme — Tasarım

- **Tarih:** 2026-10-14
- **Durum:** Onaylandı (kullanıcı 2026-10-14). Kapsam kullanıcı kararı (§2), ayrıntılar bu spec'in (§9). Plan önceden onaylıdır.
- **Taban:** `master` fbec30ed (sürüm `0.1.0-rc.2`). Denetim listeleri macOS 425'te, Linux 309'da biter; bu iş macOS **426'dan**, Linux **310'dan** sürer.

## 1. Amaç

Gezik'i kuran herkese yeni sürüm, kullanıcı bir şey yapmadan ulaşsın. **Tetikleyen tek şey yayımlanmış bir GitHub Release'tir:** etiket itmek ya da taslak açmak hiçbir kullanıcıya bir şey göndermez. Bakımcı taslağı yayımladığı an sürüm, açılışta bakan bütün Gezik'lere görünür.

Hafiflik bozulmaz: boşta hiçbir şey çalışmaz. Gezik açılışta, ilk kareden sonra ve **günde en çok bir kez** GitHub'a tek bir istek atar. Yeni sürüm varsa arka planda indirir, denetler ve kapanışta kurar.

**Bilinen sınır:** güncelleyici güncelleyicisi olan ilk sürümle başlar. rc.2 ve öncesini kuranlar bir kez elle indirmek zorundadır.

### Başarı ölçütleri

- Yeni bir Release yayımlandıktan sonra açılan Gezik, ≤ 24 saat içinde paketi indirir ve SHA-256'sını `SHA256SUMS.txt` ile karşılaştırır. Durum çubuğunda bir kez "Gezik 0.2.0 hazır: kapanınca kurulacak · Şimdi yeniden başlat" yazar. Kapanınca dosyalar yer değiştirir. Sonraki açılış yeni sürümdür.
- Yarıda kalan indirme ya da kurulum eski Gezik'i bozmaz. Bir şey ters giderse eski dosya yerinde kalır (test).
- Yazılamayan bir yerde (Program Files, `/usr`, `/opt`, macOS'ta dmg'nin içinden ya da App Translocation'dan çalışma) indirme yapılmaz. Yalnızca haber verilir ve sürüm sayfası bağlantısı gösterilir.
- **Bellek ve açılış:** boşta bellek `master` ±0,02 MB, açılış süresi ±0. Denetim ilk kareden sonra kendi iş parçacığında koşar ve biter.
- **Exe boyutu:** ≤ +262.144 B.

## 2. Kullanıcı kararları (2026-10-14)

| Konu | Karar |
|---|---|
| Davranış | Arka planda indir, kapanınca kur. Kullanıcıya sorulmaz. |
| Varsayılan | Açık. Ayarlar'dan kapatılır. |
| Doğrulama | Yalnızca `SHA256SUMS.txt`. Kendi imzamız yok. |
| Kanal | Kullanılanın türüne göre: rc kullanan rc'leri ve kararlıları alır, kararlı kullanan yalnızca kararlıları alır. Ayarlardan değişir. |
| Tetikleyici | Yalnızca yayımlanmış GitHub Release. |

**Doğrulama kararının sonucu:** SHA-256, dosyanın Release'teki hâliyle aynı olduğunu gösterir. Release'i kimin koyduğunu göstermez. GitHub hesabını ya da depoya yazma iznini ele geçiren biri, bütün kullanıcılara kendi programını gönderebilir. İleride imza eklemek için bkz. §8.

## 3. Sürümü bulma

- **İstek:** `GET https://api.github.com/repos/wenlar/gezik/releases?per_page=20`, mevcut `gezik_platform::http::download` ile geçici bir dosyaya, en çok 1 MiB. Başlıklar: `Accept: application/vnd.github+json`, `User-Agent: Gezik/<sürüm>`. Kimliksiz istek; taslaklar bu listede hiç görünmez, bu yüzden yalnızca yayımlanmış sürümler gelir. GitHub'ın saatte 60 istek sınırı günde bir istekle sorun olmaz. 403/429 gelirse sessizce yarına kalır.
- **Ayrıştırma:** `serde_json` (Cargo.lock'ta zaten var, bugün yalnızca bir proc-macro'da) ve küçük bir `#[derive(Deserialize)]`. Alınanlar: `tag_name`, `draft`, `prerelease`, `html_url`, `assets[].name`, `assets[].browser_download_url`, `assets[].size`. Kalanlar yok sayılır.
- **Seçim (saf mantık, `gezik-core/src/update.rs`):**
  - `draft` olanlar atlanır.
  - `tag_name` `v` + semver olmayanlar atlanır.
  - Kanal "kararlı" ise `prerelease` ya da `-` içeren sürümler atlanır.
  - Kalanlardan semver'e göre en büyüğü seçilir (`0.1.0-rc.10 > 0.1.0-rc.9`, `0.1.0 > 0.1.0-rc.3`). Çalışan sürümden büyükse "yeni sürüm" sayılır.
  - Kanal "otomatik" ise çalışan sürüm rc'yse rc'ler de alınır, değilse yalnızca kararlılar.
- **Ne zaman:** açılışta, ilk kareden 10 s sonra, kendi iş parçacığında. Son denemenin zamanı `state.toml`'a `[update] checked = <unix s>` olarak yazılır, 20 saatten yeniyse istek atılmaz. Palette ve Yardım menüsünde **Check for Updates** (`check-updates` eylemi, `actions!`) bu sınırı atlar ve sonucu her durumda söyler ("Gezik is up to date").

## 4. İndirme ve doğrulama

- **Paket:** sistemin ve kurulum biçiminin paketi (§5) ve `SHA256SUMS.txt`, ikisi de seçilen Release'in varlıklarından. Varlık bulunamazsa ya da boyutu `size`'tan farklıysa güncelleme yapılmaz.
- **Hedef:** `<config>/update/<sürüm>/` (taşınabilir modda `gezik-data/update/`). İndirme `.part` adıyla yapılır, bitince ad değiştirilir. Yarım kalırsa ertesi gün baştan başlar.
- **Doğrulama:** paketin SHA-256'sı `SHA256SUMS.txt`'teki satırla karşılaştırılır (mevcut `gezik-ops` karma kodu). Tutmazsa dosya silinir ve günlüğe bir uyarı yazılır, kullanıcıya hiçbir şey denmez.
- **Açma:** zip, `tar.gz` ve `.app.tar.gz` mevcut arşiv kodlarıyla aynı klasöre açılır. Açılan program `--version` ile bir kez çalıştırılır ve beklenen sürümü yazması gerekir. Böylece bozuk ya da yanlış mimariye ait bir paket kurulmaz.
- **Hazır:** `update/<sürüm>/ready` işareti konur. Durum çubuğunda tek seferlik bildirim çıkar (§6).

## 5. Kurulum, sistem başına

Ortak kural: **değiştirilen dosya önce `.old` adıyla kenara alınır**, yenisi yerine taşınır. Bir adım başarısız olursa `.old` geri konur. `.old`, yeni sürüm bir kez sorunsuz açılınca silinir.

| Kurulum | Nasıl tanınır | Paket | Değiştirilen |
|---|---|---|---|
| Windows zip ve taşınabilir zip | `current_exe()` | `gezik-<v>-windows-x64.zip` (taşınabilir modda `-portable.zip`, içindeki exe aynı) | `gezik.exe`. Çalışan exe'nin adı Windows'ta değiştirilebilir, bu yüzden kurulum Gezik kapanırken kendi sürecinde yapılır. |
| macOS `Gezik.app` | `current_exe()`'den `.app` kökü | **yeni varlık** `gezik-<v>-macos-universal.app.tar.gz` (release.yml'e eklenir; dmg'yi bağlamaktan basit) | Bütün `Gezik.app` klasörü: `Gezik.app` → `Gezik.app.old`, yeni klasör yerine. Ad hoc imza paketin içinde gelir. NSURLSession indirmesine karantina işareti konmaz. |
| Linux AppImage | `$APPIMAGE` | `gezik-<v>-linux-x64.AppImage` | `$APPIMAGE` dosyası, çalıştırma izniyle |
| Linux tar.gz | `current_exe()` | `gezik-<v>-linux-x64.tar.gz` | `gezik` dosyası |

- **Yazılabilirlik:** hedef klasörde geçici bir dosya oluşturmayı denemek, açıklamadan daha güvenilirdir. Yazılamıyorsa, ya da macOS'ta yol `/Volumes/` veya `AppTranslocation` içindeyse, indirme yapılmaz. Bu durumda yalnızca haber verilir (§6).
- **Ne zaman:** Gezik'in son penceresi kapanıp süreç biterken (tepsi açıksa tepsiden çıkışta). Ayrıca bildirimdeki **Şimdi yeniden başlat** ile: kurar, yeni Gezik'i aynı argümansız başlatır, kendisi kapanır. Oturum (sekmeler) zaten kapanışta yazılıyor, olduğu gibi geri gelir.
- **Birden çok Gezik süreci:** kurulumu yalnızca dosyayı değiştirebilen yapar. Dosyayı açık tutan başka bir süreç yüzünden yer değiştirme başarısız olursa bir sonraki kapanışta yeniden denenir.

## 6. Arayüz

- **Durum çubuğu:** "Gezik 0.2.0 is ready — installs when you close it · Restart now · What's new" (Türkçe çeviri aynı anlamda). Yeni sürüm başına bir kez gösterilir, sonra kapatılınca bir daha çıkmaz. **What's new** `html_url`'yi sistem tarayıcısında açar.
- **Yazılamayan kurulum:** "Gezik 0.2.0 is out · Download". **Download** `html_url`'yi açar. Sürüm başına bir kez gösterilir.
- **Ayarlar ▸ Genel ▸ Güncellemeler:** "Check for updates automatically" (`[update] check = true`) ve "Channel" (`[update] channel = "auto" | "stable" | "pre"`). Altında son denemenin sonucu ve **Check now**.
- **Yeni sürümün ilk açılışı:** durum çubuğunda bir kez "Updated to 0.2.0 · What's new".

## 7. Release iş akışı

- `release.yml`'e macOS işinde `Gezik.app`'i `gezik-<v>-macos-universal.app.tar.gz` olarak paketleyen bir adım eklenir. `SHA256SUMS.txt` onu da kapsar.
- `docs/releasing.md`'ye eklenecekler:
  - **Taslağı yayımlamak güncellemeyi bütün kullanıcılara gönderir.** Paketleri denemeden yayımlama.
  - Bir sürüm geri çekilecekse Release silinir ya da taslağa çevrilir. Henüz indirmemiş olanlara gitmez, indirmiş olanlar kurar.
  - Bir sürümü geri almak, daha büyük bir sürüm numarasıyla yeni bir Release demektir. Güncelleyici hiçbir zaman küçük sürüme inmez.

## 8. Kapsam dışı

- **Kendi imzamız (ed25519) ya da Authenticode/notarization doğrulaması:** kullanıcı kararı. İleride eklemek kolay: Release'e `SHA256SUMS.txt.sig` konur, açık anahtar Gezik'e gömülür, imzası tutmayan güncelleme kurulmaz. Ama bunu ancak imzalı sürümler yayımlandıktan sonra çıkan Gezik'ler denetleyebilir.
- **Sürüm düşürme, fark (delta) güncellemesi, MSIX/Store, paket yöneticileri (winget, Homebrew, apt):** bu kanallar kendi güncellemelerini yapar, Gezik onlara karışmaz.
- **Güncellemeyi ertelemek ya da tek bir sürümü atlamak:** yok. Kapatmak isteyen ayarı kapatır.

## 9. Bu spec'in kararları

1. **İstek GitHub API'sine.** Atom akışı ön sürüm bayrağını vermiyor. `releases/latest` ön sürümleri hiç vermiyor.
2. **`serde_json`.** El yazımı bir JSON okuyucu daha çok kod ve test demek. Boyut PR'da ölçülür.
3. **macOS için yeni `.app.tar.gz` varlığı.** `hdiutil attach` başka bir süreç, bir bağlama noktası ve bir temizlik gerektiriyor. Tar ise Gezik'in kendi kodunda zaten var.
4. **Kurulum kendi süreç çıkışında.** Windows çalışan exe'nin adını değiştirmeye izin veriyor, macOS ve Linux açık dosyanın yerine yenisinin konmasına izin veriyor. Ayrı bir yardımcı program gerekmiyor.
5. **`--version` denetimi.** Ucuz, ve SHA-256'nın yakalamadığı "yanlış paket seçildi" hatasını yakalar.

## 10. Testler

- **`gezik-core` `update`:** semver karşılaştırması (rc sırası, rc'ye karşı kararlı), kanal seçimi, taslak ve bozuk etiketin atlanması, varlık adının sistem ve kurulum biçimine göre seçilmesi, `SHA256SUMS.txt` ayrıştırması, gerçek bir API yanıtı örneğiyle (fixture) uçtan uca seçim.
- **Kurulum:** geçici klasörde sahte bir "kurulu Gezik" ile yer değiştirme, `.old` ile geri dönüş, yarıda hata, yazılamayan klasör, çalışan bir exe'nin üzerine (Windows).
- **Ağ:** test sırasında gerçek GitHub'a gidilmez. İndirme işlevi enjekte edilir.
- **Ekran testi (Windows):** `GEZIK_UPDATE_FEED` ortam değişkeniyle API yerine yereldeki bir JSON dosyası ve paket kullanılır; bu değişkeni yalnızca `debug_assertions`'lı ya da `measure` profiliyle derlenmiş exe okur. Adımlar: bildirim, kapanınca kurulum, yeni sürümün açılışı ve "Updated to" bildirimi, yazılamayan kurulumda yalnızca haber verilmesi.
- **Denetim listeleri:** macOS 426– (`.app` değişimi, Translocation'da yalnızca haber verilmesi) ve Linux 310– (AppImage, tar.gz).
- **İlk gerçek deneme:** rc.3 güncelleyiciyle çıkar. rc.4 (ya da bir deneme sürümü) yayımlanınca rc.3'ün kendiliğinden güncellendiği üç sistemde de görülür.

## 11. Parçalar

Tek PR: `feat/auto-update`. Notlar `docs/superpowers/notes/parts/update.md` dosyasına, denetim listeleri `checklists/update-macos.md` ve `checklists/update-linux.md` dosyalarına yazılır.
