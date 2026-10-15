# Bulut senkronu (17): tasarım

- **Tarih:** 2026-10-15. **Dayandığı:** `2026-10-03-ayarlar-ve-tema-design.md` §2 (gizli kod, cihazda şifreleme, hesap yok), `2026-10-12-tasinabilirlik-design.md` §3.1 ve §8 (paylaşılan/yerel ayrımı, senkron klasörü, `settings.local.toml`, `three_way`, `Review…`).
- **Bakımcı kararları (2026-10-15):** sunucu Cloudflare Worker, `sync.wenlar.com` (Claude kurar; kullanıcı başına boyut ve istek sınırı); eşleme gizli kodla; senkron açılışta ve değişince, ayrıca `Sync Now`; içerik senkron klasörüyle aynı set ve ikisi birlikte açılmaz; çakışmada anahtar anahtar otomatik birleştirme, aynı anahtar iki yanda değiştiyse `Review…`; bir yıl erişilmeyen veri silinir, uygulamada `Delete from Server`.
- **Onay:** spec onaylanınca plan önceden onaylıdır; parçalar sormadan kodlanır.

## 1. Amaç ve ölçütler

Bir kullanıcı, hesap açmadan, ayarlarını cihazlar arasında taşır: ilk cihazda `Start Syncing` bir kod verir, öbür cihaza kod yapıştırılır, ondan sonra bir cihazdaki değişiklik öbürüne açılışta ya da `Sync Now` ile gelir.

- Sunucu içeriği okuyamaz: şifreleme anahtarı koddan cihazda türetilir, sunucuya yalnız şifreli bayt, rastgele görünen bir kimlik ve bir yetki özeti gider.
- Hiçbir değişiklik sessizce kaybolmaz: farklı anahtarlar kendiliğinden birleşir, aynı anahtar iki yanda değiştiyse 14f'nin `Review…` penceresi açılır.
- Senkron kapalıyken maliyet sıfır: iş parçacığı, zamanlayıcı, ağ yok. Açıkken boşta da yok; yalnız açılışta, değişiklikten 5 sn sonra, `Sync Now`'da ve kapanışta bekleyen gönderim varsa.
- Ağ yokken Gezik bugünkü gibi çalışır; senkron sonra yetişir.

## 2. Kapsam dışı

- Hesap, e-posta, parola, cihaz listesi, sunucuda sürüm geçmişi (yerelde 14d yedekleri var).
- Ara ara sunucuya sorma (bakımcı kararı); başka cihazın değişikliği bir sonraki açılışta ya da `Sync Now`'da gelir.
- QR kod.
- Makineye özgü dosyalar (`state.toml`, `views.toml`, `tags-store.tsv`, `settings.local.toml` vb.; 14 §3.1 "yerel").
- Senkron klasörüyle birlikte kullanım (biri açıkken öbürünün düğmesi kapalı, nedeni yazılı).
- Kodu işletim sisteminin parola kasasında tutma (bkz. §5.3).

## 3. Yapı: bulut, bir senkron klasörü gibi

Bulut senkronu açılınca paylaşılan yer yerel klasörün içinde `cloud/` olur (`location.toml`'da `shared = "{local}/cloud"`, 14e'nin yer çözümü). Böylece 14f'nin bütün düzeni olduğu gibi çalışır: `settings.local.toml`, `[system]`'in yerelde kalması, `SettingsWriter`'ın yarış denetimi, izleyici, `Review…`. Bulut istemcisi bu klasörü sunucuyla eşitleyen küçük bir eşitleyicidir; Syncthing'in yaptığını yapar ve çakışmada aynı yolla çakışan kopya bırakır.

```
settings.toml, themes/, templates/  (local/cloud/)
        │  manifest + üç yollu birleştirme (taban: local/cloud-base/)
        ▼
şifreli paket ──HTTPS──► sync.wenlar.com/v1/<id>  (Durable Object, sürüm numaralı)
```

## 4. Sunucu (17a)

- **Yer:** depoda `server/sync/` (Worker, JavaScript, bağımlılık yok; `wrangler.toml`). Cloudflare hesabında Worker `gezik-sync`, özel alan `sync.wenlar.com`, SQLite tabanlı Durable Object `Box` (ücretsiz plan; güçlü tutarlılık, alarm).
- **Uç noktalar** (hepsi `Authorization: Bearer <yetki, base64url>`; `<id>` 32 onaltılık karakter):
  - `GET /v1/<id>` → `200` gövde + `ETag: "<n>"`, ya da `404`.
  - `PUT /v1/<id>` + `If-Match: "<n>"` (ilk yazmada `If-None-Match: *`) → `200` + yeni `ETag`; sürüm tutmazsa `412`.
  - `DELETE /v1/<id>` → `204`.
- **Yetki:** ilk `PUT` yetkinin SHA-256'sını saklar; sonraki her istekte karşılaştırılır (sabit zamanlı), tutmazsa `403`. Sunucu anahtarı bilmez.
- **Sınırlar:** gövde en çok 8 MiB (`413`); kimlik başına saatte 120 istek, IP başına dakikada 60 (Workers Rate Limiting bağlaması; aşınca `429` + `Retry-After`). Veri 1 MiB'lık satırlara bölünür (SQLite satır sınırı).
- **Saklama:** her erişimde alarm `şimdi + 365 gün`e kurulur; alarm çalınca nesne her şeyini siler.
- **Günlük:** Worker istek gövdesini ve yetkiyi günlüğe yazmaz.
- **Test:** `server/sync/test.mjs` (Node'un kendi test aracıyla, `wrangler dev`'e karşı): yetkisiz, 412 yarışı, 413, silme, alarm sonrası 404. Kurulum ve sınırlar `docs/cloud-sync-server.md`'de (kendi sunucusunu isteyen için de).

## 5. İstemci

### 5.1 Kod ve anahtarlar (17b, saf)

- **Kod:** 25 karakter Crockford base32 (125 bit rastgele, `getrandom`), beşli gruplar: `7KQ2M-X9D4R-…`. Okurken büyük/küçük harf, boşluk, tire, `O→0`, `I/L→1` hoş görülür; denetim için son grup bir CRC-5 karakteri taşır, yanlış yazım "This code has a typo" der (sunucuya gitmeden).
- **Türetme:** HKDF-SHA256(kod) → `id` (16 bayt), `auth` (32 bayt), `key` (32 bayt), ayrı `info` etiketleriyle. Kodun 125 biti yavaş KDF gerektirmez.
- **Paket:** düz metin = `GZS1` + dosya sayısı + her dosya için (yol uzunluğu u16, yol `/` ile, boyut u32, baytlar); şifreli = `GZE1` + 24 bayt rastgele nonce + XChaCha20-Poly1305(key, düz metin, ek veri = `id`). Yollar `safe_relative` ile denetlenir; `..` ya da kök yol paketi geçersiz kılar.
- **Bağımlılıklar:** `chacha20poly1305`, `hkdf` (RustCrypto, saf Rust; `sha2` zaten var), `getrandom` (kilit dosyasında zaten var).

### 5.2 Eşitleme (17b saf, 17c bağlama)

Durum yerelde: `cloud-base/` (son eşitlenen paketin açık hâli: üç yollu birleştirmenin tabanı) ve `cloud-sync.toml` (`server`, `code`, `version`, `last`).

Bir tur (`sync_round`, iş parçacığında, aynı anda tek tur):

1. `GET`. `404` ve bizde sürüm yoksa: yeni kutu, adım 4'e (yerel her şey gönderilir).
2. Sunucu sürümü bizimkiyle aynıysa ve yerel tabandan farklı değilse bitti.
3. **Birleştirme** (`plan(taban, yerel, uzak) -> Plan`, saf, testli):
   - `settings.toml`: `three_way(taban, yerel, uzak)`. Sonuç yerele yazılır (`SettingsWriter` yoluyla, izleyici yeniden yükler). Aynı anahtarlar iki yanda farklıysa yerel olduğu gibi kalır, uzak `settings.cloud-conflict-<tarih>.toml` adıyla yanına yazılır; 14f'nin desen tablosuna bu ad eklenir ve durum çubuğu `Review…` der.
   - Öbür dosyalar (temalar, şablonlar) bayt özetiyle: yalnız bir yanda değişen o yanınki olur; iki yanda farklı değişen: yerel kalır, uzak `ad.cloud-conflict-<tarih>.uzantı` olarak yanına; bir yanda silinip öbüründe değişmeyen silinir (çöpe); silinip öbüründe değişen kalır.
4. Birleşik hâl yerelde farklıysa ya da sunucudan farklıysa `PUT` (`If-Match`); `412` gelirse tur başa döner (en çok 3 kez, sonra durum satırında "Another device is syncing — try again").
5. Başarıda `cloud-base/` ve `version` güncellenir, durum satırı "Synced just now".

Tetikleyiciler: açılış (pencere çizildikten sonra), izleyicinin `cloud/` altında gördüğü değişiklikten 5 sn sonra (tek atımlık; ardışık değişiklikler süreyi yeniler), `Sync Now`, kapanışta bekleyen gönderim varsa en çok 3 sn. Bu turun kendi yazdığı dosyalar izleyicide yeni bir tur doğurmaz (yazılan özetler bir kez yok sayılır).

Hatalar: ağ yok → sessiz, durum satırında "Not synced — offline", bir sonraki tetikleyicide yeniden; `403` → "This code is no longer valid" (kutu silinmiş ya da kod değişmiş) ve senkron durur; `413` → "Too large to sync (8 MB). Large templates are the usual cause."; `429` → `Retry-After` kadar sonra tek deneme.

### 5.3 HTTP (17b)

`gezik-platform::http`'e `request(method, url, headers, body, max) -> Response { status, etag, retry_after, body }` eklenir; bugünkü üç uygulama (WinHTTP, `NSURLSession`, `curl`) genişler, TLS yine sistemin. Yalnız https; sunucu adresi ayarda değiştirilebilir (`[cloud-sync] server`, varsayılan `https://sync.wenlar.com`; `http://localhost` yalnız testte).

Kod `cloud-sync.toml`'da, yerel klasörde, yalnız kullanıcının okuyabileceği izinle (Unix `0600`; Windows'ta kullanıcı profilinin altında zaten). Kodu bilen ayarları okur; bunu ayar penceresi söyler ("Anyone with this code can read your settings"). İşletim sistemi kasası kapsam dışı: üç ayrı API, Linux'ta Secret Service D-Bus bağımlılığı; tehdit (kullanıcının kendi profiline erişen biri) zaten ayarları doğrudan okuyabilir.

### 5.4 Arayüz (17c)

Sync and Backup sayfasında yeni bölüm **Cloud Sync**:

- Kapalıyken: `Start Syncing…` ve `Join With a Code…`. Senkron klasörü açıksa ikisi kapalı, not "Stop keeping settings in a folder first".
- `Start Syncing…`: kutu oluşturulur, pencere kodu gösterir (`Copy`), "Keep this code. It is the only way to reach your settings from another device; Gezik cannot recover it."
- `Join With a Code…`: kod alanı → `GET`; kutu yoksa "No settings found for this code". Varsa bugünkü ayarlar 14d yedeğine alınır, bulut kopyası kurulur (birleştirme yok: katılan cihaz bulutu alır), not "Your previous settings were backed up. Restore Backup ▸ brings them back."
- Açıkken satır: `Synced 2 minutes ago` / hata metni; düğmeler `Sync Now`, `Show Code`, `Stop Syncing` (paylaşılan dosyalar yerel klasöre geri taşınır, sunucuya dokunulmaz), `Delete from Server…` (onaylı; `DELETE`, sonra Stop).
- Eylemler (`shortcuts.rs` `actions!`): `cloud-sync-now`, `cloud-sync-show`; varsayılan kısayol yok. Menü kimlikleri 2920–2939.

## 6. Parçalar ve sıra

| Parça | İçerik | Bağımlılık |
|---|---|---|
| **17a** | Worker + Durable Object, sınırlar, test, `sync.wenlar.com` kurulumu, `docs/cloud-sync-server.md` | yok |
| **17b** | `gezik-config/src/cloud_sync/` (kod, türetme, paket, şifre, `plan`), `gezik-platform::http::request`, çakışma adı deseni | yok |
| **17c** | Bağlama: yer değişimi, tur iş parçacığı, tetikleyiciler, ayar penceresi bölümü, eylemler, çeviriler, `docs/configuration.md`, README, denetim listeleri | 17a, 17b |

17a ve 17b aynı anda (ayrı çalışma klasörlerinde), 17c ikisinin üstünde; hepsi tek birleşik dalda tek PR olarak birleşir (16h–16m gibi). CI son PR'dan önce elle.

## 7. Test

- **Birim (17b):** kod üretme/okuma (yazım hatası, hoş görülen harfler), türetmenin sabit vektörleri, paket gidiş-dönüş ve bozuk/yanlış anahtarlı paket reddi, `safe_relative` reddi, `plan` tablosu (her dosya durumu × iki yan), `settings.toml` üç yollu birleştirme ve çakışan kopya adı, 14f desen tablosunda yeni ad.
- **HTTP:** bugünkü `http/tests.rs` yerel sunucusuna `PUT`/`If-Match`/`412`/`ETag` durumları.
- **Sunucu (17a):** `test.mjs` (§4), sonra gerçek `sync.wenlar.com`'a karşı bir duman testi.
- **Uçtan uca (17c, başsız):** iki geçici yapılandırma klasörü, yerel `wrangler dev`'e karşı: A başlatır, B katılır; A'da tema, B'de yazı boyutu değişir → iki tur sonra ikisinde de ikisi; aynı anahtar iki yanda → çakışan kopya.
- **Ekran (Windows):** başlat, kodu kopyala, ikinci `%TEMP%\gezik-gui-*` yapılandırmasıyla katıl, değişiklik gidip gelsin, `Review…`, `Delete from Server`.
- **Denetim listeleri:** `checklists/17-macos.md`, `17-linux.md` (iki makine arası gerçek senkron, çevrimdışı açılış, `0600` izni).

## 8. Gizlilik ve riskler

- Sunucu görür: kimlik, paket boyutu, erişim zamanları, IP. Görmez: içerik, dosya adları, kod. `docs/configuration.md` bunu açıkça yazar.
- Kod kaybolursa veri kurtarılamaz (tasarım gereği); `Show Code` açık bir cihazdan kodu her zaman yeniden gösterir.
- Cloudflare ücretsiz plan sınırları (günde 100 bin istek, Durable Object süresi): kimlik başına saatte 120 istekle tek kullanıcı sınırın çok altında; sınır aşılırsa istemci `429`/`5xx`'i ağ yok gibi ele alır.
- Bir cihaz yıllarca kapalı kalırsa kutu silinir; açılınca `404` + bizde sürüm var → "Your cloud copy was removed after a year without use. Start Syncing again?" (yerel ayarlar yerinde).
