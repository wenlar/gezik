# Tamamlayıcılar (16h–16m): tasarım

- **Tarih:** 2026-10-15. **Kaynak:** özellik durumu (`notes/2026-10-12-ozellik-durumu.md`) "Bilerek dışarıda bırakılanlardan geri alınanlar"; bakımcıyla satır satır konuşuldu, her satırda öneri kabul edildi.
- **Kapsam:** rakip özeti satırları 6, 15, 19, 26 (yalnız zaman damgası), 56 ve 58'in yerine iki düzen seçeneği. 17 Bulut senkronu bu adımdan sonra.
- **Onay:** bakımcı tasarımı sohbette gördü ve "onay beklemeden kodla" dedi; plan ayrıca onaya sunulmaz.

## 1. Kapsam dışı

- ACL düzenleme (Windows'ta sistem Özellikler; satır 26'nın o kısmı dışarıda kalır).
- Yerleştirilebilir paneller (satır 58); yerine §7.
- Ağda SMB dışı keşif (SFTP, WebDAV, AFP, NFS): 19 Uzak bağlantılar (eski 18).
- Ağ keşfi sonucunu önbelleğe alma, arka planda tarama, Windows "Ağ bulma" ayarını değiştirme.
- Sabitlenenlere kullanıcı resmi (yalnız yerleşik simge seti).
- Windows Bilgi penceresinde bağlantı sayısı (orada sistemin Özellikler sayfası açılır).
- Linux'ta başka programdan sürüklenen dosyada yaylı klasör (winit dış sürüklemenin konumunu vermiyor).

## 2. 16h Ağ keşfi

- **Kenar çubuğu:** `SECTION_NETWORK` (Telefonlar'dan sonra, Çöp'ten önce). Başlık "Network", varsayılan kapalı. Açılınca bir iş parçacığında tarama başlar, satırda "Searching…"; sonuçlar geldikçe eklenir, en geç 5 sn'de tarama biter. Hiç yoksa "No computers found". Başlığın sağ tık menüsü: **Refresh**. Bölüm kapanınca liste atılır; açık kalırken kendiliğinden yenilenmez.
- **Satır:** bilgisayar adı (Bonjour/NetBIOS adı), sunucu simgesi. Tıklama: Windows'ta `\\ad` (bugünkü paylaşım listesi, `navigation.rs` `server_of`); macOS/Linux'ta Sunucuya Bağlan akışı `smb://ad` ile (`connect.rs`), giriş gerekirse bugünkü pencere.
- **Platform** (`gezik-platform/src/network.rs` içinde `discover() -> Vec<Found>`; `Found { name, address }`):
  - Windows: kabuğun Ağ klasörü (`FOLDERID_NetworkFolder`) `IShellItem` ile sayılır, ayrıştırma adı `\\` ile başlayan bilgisayarlar alınır. Liste boşsa ve Ağ bulma kapalıysa satırda not ve **Open Settings** (`ms-settings:network-status` değil, `control.exe /name Microsoft.NetworkAndSharingCenter /page Advanced`); Gezik ayarı değiştirmez.
  - macOS: `dns_sd.h` (`DNSServiceBrowse` `_smb._tcp`, `local.`), 5 sn süre; ad Bonjour servis adı, adres `ad.local`.
  - Linux: Avahi D-Bus `ServiceBrowserNew` `_smb._tcp`; Avahi yoksa satırda "Avahi is not running".
- **Hafiflik:** tarama yalnız bölüm açılınca ya da Refresh'te; iş parçacığı bitince yok olur.

## 3. 16i Hard link

- Bağlantı menüsüne `LINK_HARDLINK` (symlink'in yanında) "Hard Link". `LinkTask` yeni tür `Hard` (`fs::hard_link`). Ön denetim (iş başlamadan, tek mesaj): klasör seçiliyse "Hard links work only on files", hedef başka birimdeyse "Hard links must stay on the same drive". Geri alma `Outcome::Created` ile bugünkü bağlantı geri alması.
- macOS/Linux Bilgi penceresi: dosyada `nlink > 1` ise "Links: N".

## 4. 16j Sabitlenenlere renk ve simge

- `PinEntry` + `color: Option<String>`, `icon: Option<String>`; yazılırken yalnız doluysa tablo biçimine geçer (bugünkü `name`/`group` gibi). `color`: `Hue::parse` (yedi etiket rengi ya da `#rrggbb`); geçersiz değer uyarı verir, yok sayılır. `icon`: yerleşik adlardan biri; bilinmeyen ad varsayılan simge + uyarı.
- Simge seti (`Glyph` içinde SVG yol): folder, star, heart, home, work, code, music, photo, video, document, download, cloud, archive, book, game, money, mail, link, lock, flag, bolt, leaf, globe, person (24).
- Sağ tık (sabitlenen satır): **Color ▸** (None + yedi renk) ve **Icon ▸** (Default + 24). Kayıt `SettingsChange::Pinned`.
- Çizim: `sidebar.slint` satırında simge yolu rengi doluysa o renk; seçili satırda tema rengi korunur.

## 5. 16k Tarihleri ayarla

- Sağ tık ▸ **Set Dates…** (dosya ve klasör; Araçlar alt menüsü). Pencere: Modified / Created / Accessed için onay kutuları (Linux'ta Created yok); kaynak: **Date and time** (tarih + saat alanı, yerel saat), **Now**, **Date taken (EXIF)** (`gezik-batch` `exif::taken`; okunamayan dosya atlanır ve sonda sayılır), **From file…** (dosya seç; onun üç tarihi).
- Görev: `gezik-ops` `SetTimesTask` (yeni `TaskKind::Times`, `Outcome::TimesChanged { path, old }`); yazma `File::set_times` (`FileTimes` + Windows/macOS `set_created`). Geri alma eski üç tarihi yazar; tek Ctrl+Z bütün iş.
- macOS/Linux Bilgi penceresinde tarih satırına tıklamak aynı pencereyi açar.
- Saf parça (`gezik-core`): kaynak + seçili alanlar + mevcut tarihler → yazılacak tarihler; birim testli.

## 6. 16l Yaylı klasörler

- Sürüklerken (iç ya da dış) liste satırı klasörse, ağaç düğümü ya da yol taşıyan kenar çubuğu satırıysa ve imleç `spring-delay` ms aynı hedefte durursa o bölme hedefe gider (sekme yaylanması `drag.rs` `follow_tab` deseni). Gidilen klasörde yeniden yaylanabilir.
- Başlangıç yeri saklanır; sürükleme bırakılmadan biterse (Esc, pencere dışında bırakma) bölme başlangıca döner. Bırakma olursa yeni yerde kalır.
- Ayar `[drag] spring-delay = 700` (ms; 0 kapalı; 200–3000 arası kırpılır); ayarlar penceresinde.
- Dış sürükleme: Windows `IDropTarget::DragOver`, macOS `draggingUpdated` konum verir; Linux yalnız iç sürükleme.

## 7. 16m Düzen seçenekleri

- `[panes] stacked = false`: true ise iki bölme üst üste; ayırıcı yatay, oran `[panes] split` aynı anahtar. Eylem `stack-panes` (View ▸ Stack Panes, onay işaretli).
- `[preview] position = "right" | "bottom"`; altta yükseklik `preview` durumunda ayrı (`height`). Eylem `preview-at-bottom` (View ▸ Preview at Bottom).
- Yeni eylemler yalnız `shortcuts.rs` `actions!` içinde; varsayılan kısayol yok.

## 8. Parçalar ve sıra

| Parça | Ana dosyalar | Not dosyası |
|---|---|---|
| 16h | `sidebar.rs`, `gezik-platform/src/network.rs`, `connect.rs` | `parts/16h.md` |
| 16i | `context_menu.rs`, `gezik-ops/src/tasks/link.rs`, `gezik-platform/src/link.rs`, `info.rs` | `parts/16i.md` |
| 16j | `pins.rs`, `sidebar.rs`, `sidebar.slint`, `glyphs.slint`, `context_menu.rs` | `parts/16j.md` |
| 16k | `gezik-ops` yeni görev, `gezik-core` saf parça, yeni pencere, `context_menu.rs`, `info.rs` | `parts/16k.md` |
| 16l | `drag.rs`, `gezik-core/src/drag.rs`, `gezik-platform/src/dnd/*`, ayarlar | `parts/16l.md` |
| 16m | `app.slint`, `dual.rs`, `settings.rs`, `shortcuts.rs`, `menu_bar.rs` | `parts/16m.md` |

Her parça kendi dalı ve PR'ı; birbirine bağlı değiller. Denetim listeleri: `checklists/16h-macos.md`… / `-linux.md`; numaralar macOS 433'ten, Linux 316'dan, parça başına 10'luk blok (16h 433/316, 16i 443/326, 16j 453/336, 16k 463/346, 16l 473/356, 16m 483/366).

## 9. Test

- Birim: tarih kaynağı çözümü (16k), `PinEntry` okuma/yazma ve geçersiz renk/simge (16j), hard link ön denetimi (16i), yaylanma zamanlayıcısının hedef değişince sıfırlanması (16l, saf parça), `stacked`/`position` ayrıştırma (16m), ağ adı ayrıştırma (16h, platform çıktısından).
- Windows ekran testi (bakımcı uzaktayken, `scripts/gui/gui.ps1`): her parça.
- macOS/Linux: denetim listeleri. CI son PR'dan önce elle.
