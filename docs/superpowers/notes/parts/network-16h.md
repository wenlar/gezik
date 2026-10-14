# 16h: ağ keşfi — notlar

Spec: `../../specs/2026-10-15-tamamlayicilar-design.md` §2. Dal: `feat/network-16h`.

## Ne yapıldı

- **Platform** (`gezik-platform/src/network/discover.rs`, `network::discover` olarak dışa açık): `discover(&mut dyn FnMut(Found)) -> Result<(), &'static str>`; `Found { name, address }`. Bulunan her bilgisayar geldikçe geri çağrıya verilir.
  - Windows: `SHGetKnownFolderItem(FOLDERID_NetworkFolder)` → `BindToHandler(BHID_EnumItems)` → `IEnumShellItems`; ayrıştırma adı `\\AD` olan öğeler (yazıcı, medya sunucusu, `\\AD\paylaşım` atlanır). Ad: öğenin görünen adı; adres: `AD`.
  - macOS: `dns_sd` FFI (libSystem, yeni crate yok): `DNSServiceBrowse` `_smb._tcp` `local.`, her eklenen hizmet için `DNSServiceResolve` (en çok 1 s) → adres hizmetin ana makine adı (`Alis-MacBook.local`, sondaki nokta atılır). Bekleme `poll` ile, toplam 5 s.
  - Linux: `avahi-browse -tpr _smb._tcp` (`ChildProcess`), `=` satırlarından ad ve ana makine (Avahi'nin `\DDD` kaçışları çözülür). 5 s'de bitmezse süreç ağacıyla öldürülür. Program yoksa ya da başarısız biter ve hiçbir şey bulmazsa (`Daemon not running`): "Avahi is not running".
- **Uygulama** (`gezik/src/network_places.rs`): açık/kapalı, arama sürüyor mu, liste, sorun; hepsi UI iş parçacığında, kalıcı değil. Açılış ve Refresh bir iş parçacığı başlatır; sonuçlar `invoke_from_event_loop` ile gelir, eski aramanınkiler (`ASKED`) atılır, aynı ad bir kez. `slint::Timer::single_shot(5 s)` aramayı bitmiş sayar (Windows'un kendi taraması daha uzun sürebilir: bu makinede boş ağda 14 s); sonra gelen atılır. Kapanınca liste atılır.
- **Kenar çubuğu**: `SECTION_NETWORK = 11`, başlık "NETWORK" sağ/aşağı ok ile (`arrow` 1/2), tıklayınca açılır/kapanır (`toggled` → `toggle_row` başlığı tanır), sağ tık **Refresh** (`NETWORK_REFRESH = 1813`). Satırlar: bilgisayarlar (yeni `computer` glifi, `ICON_COMPUTER = 10`), "Searching…" / "No computers found" / "Avahi is not running" notu (`index -1`, soluk), Windows'ta boş listede **Open Settings** (`index -2`: `control.exe /name Microsoft.NetworkAndSharingCenter /page Advanced`).
- **Tıklama**: Windows'ta `\\AD` (paylaşım listesi, bugünkü `server_of` yolu); orta tık yeni sekmede. macOS/Linux'ta Sunucuya Bağlan penceresi `smb://ana-makine/` ile açılır (`connect::open_with`).
- Belgeler: `docs/features.md` `## Network`, README satırı. Çeviriler: tr elle, de/es/fr/zh-Hans `fuzzy`.

## Sapmalar

1. **Bölüm Çöp'ten sonra**, önce değil: Çöp'ün başlığı yok, sürücülerin altında duruyor; NETWORK başlığı önüne girince Çöp ağın bir satırı gibi görünürdü.
2. **macOS/Linux tıklaması paylaşım ister**: `parse_address` Windows dışında paylaşımsız adresi reddediyor (`NEED_SHARE`); bu yüzden doğrudan bağlanmak yerine Sunucuya Bağlan penceresi `smb://ana-makine/` dolu açılır, kullanıcı paylaşımı ekler.
3. **Windows'ta "Ağ bulma kapalı" tespiti yok**: liste boşsa her zaman "No computers found" + **Open Settings** gösterilir. Ayarın gerçek durumunu okumak (güvenlik duvarı kural grubu, FDResPub hizmeti) güvenilir ve ucuz değil.
4. **Linux'ta D-Bus değil `avahi-browse`**: doğrudan D-Bus crate'i bağımlılık değil (zbus yalnız dolaylı); yeni bağımlılık eklenmedi. `avahi-utils` yoksa da "Avahi is not running" denir.
5. **macOS adresi ana makine adı**, `ad.local` değil: Bonjour adı boşluk/kesme içerebilir ("Ali's MacBook") ve `parse_address` boşluklu sunucuyu reddeder; `DNSServiceResolve` gerçek ana makineyi verir. Çözülemeyen hizmet listelenmez.
6. **Başlık klavyeyle açılmıyor**: kenar çubuğunun klavye imleci başlıklara gitmez (mevcut düzen); başlık ekran okuyucuya genişletilebilir düğme olarak açık.

## Bilinen sınırlar

- Windows'ta 5 s'de gelmeyen bilgisayar o aramada görünmez (Refresh ile ikinci aramada kabuk önbelleğinden genelde hemen gelir). Windows'un tarama iş parçacığı kendi bitene kadar yaşar (UI onu beklemez).
- macOS ve Linux kodu yalnız derleme denetimi gördü (aşağıda); çalıştırılmadı.
- Durum tüm pencerelerde ortak (tek liste, her kenar çubuğu aynısını gösterir).

## Testler

- `gezik-platform` `network::discover`: Ağ klasörü öğesi süzme; `avahi-browse -p` satırı ve kaçış çözme.
- `gezik` `network_places`: satırların arama durumuna göre metni.

## Ekran testi için ne denenmeli (Windows)

1. Açılışta en altta kapalı NETWORK başlığı (sağ ok), altında satır yok; Görev Yöneticisi'nde ek iş parçacığı/ağ yok.
2. Başlığa tıkla: ok aşağı, "Searching…"; ağda paylaşımlı bilgisayar varsa monitör simgesiyle gelir; en geç 5 s'de not gider. Hiç yoksa "No computers found" ve **Open Settings** (Denetim Masası ▸ Gelişmiş paylaşım ayarları açılır).
3. Bir bilgisayara tıkla: `\\AD` paylaşım listesi; orta tık yeni sekmede.
4. Başlığa sağ tık ▸ Refresh: liste boşalır, yeniden arar. Başlığa yeniden tıkla: kapanır, satırlar gider.
5. İki pencere açıkken birinde aç: ikisinde de aynı liste.
