# Günlük kolaylıklar: platform denemeleri

Tarih: 2026-10-08 (denemeler 2026-10-07). Araç zinciri: rustc/cargo 1.99.0, edition 2024. Windows 11 Pro 26100, i9-14900K (32 iş parçacığı), yönetici olmayan (UAC'li) oturum, Geliştirici Modu kapalı.
Deneme crate'i: `scratchpad/probe7` (ayrı `[workspace]`, Gezik'in `Cargo.lock`'u kopyalanarak, `gezik-core` ve `gezik-platform`'a path bağımlılığıyla). Dosyalar `docs/superpowers/notes/gunluk-probe/` altında: `lnk.rs`, `junction.rs`, `clip.rs`, `clip_x11.rs`, `clip_wayland.rs`, `term.rs`, `attrs.rs` (her biri `src/bin/`), `Cargo.toml.txt`, `linux.sh` ve `terms.sh` (Docker betikleri), `*.out.txt` (çıktılar).

Aşağıdaki her Rust parçası derlenip çalışan deneme kodundan alındı. Windows'ta hepsi çalıştı. macOS ve Linux için `cargo check --target aarch64-apple-darwin --bins` ve `--target x86_64-unknown-linux-gnu --bins` uyarısız geçti. Linux denemeleri `gezik-linux` imajında çalıştı (Xvfb + xclip, başsız sway + wl-copy, apt ile kurulan 15 uçbirim). macOS'ta hiçbir şey çalıştırılmadı.

Yeniden çalıştırma:
```
cd probe7
cargo build --release --bins
target/release/lnk          # Q1
target/release/junction     # Q2 (Geri Dönüşüm Kutusu'na bir bağlantı atar; deneme sonrası elle silindi)
target/release/clip         # Q3: panoyu değiştirir, sonunda eski metni geri koyar
target/release/clip read    # Q3: panoda ne varsa okur (Alt+PrtScn sonrası gibi)
target/release/term         # Q4: wt / PowerShell pencereleri açıp kapatır
target/release/attrs        # Q5
docker run --rm -v D:/Work/gezik:/src:ro -v <probe7>:/probe:ro -v gezik-cargo:/usr/local/cargo/registry \
    -v probe7-target:/target gezik-linux bash /probe/linux.sh      # Q3/Q4/Q5 Linux
docker run --rm -v <probe7>:/probe:ro -v probe7-target:/target gezik-linux bash /probe/terms.sh   # Q4, 15 uçbirim
```
(Git Bash'ten `MSYS_NO_PATHCONV=1` ile.)

---

## 0. Bağımlılıklar: yeni crate yok

- **Windows:** `gezik-platform`'daki `windows` 0.62.2 özellik listesi **aynen** yetti; probe7'nin `Cargo.toml`'u o listeyi birebir kopyalıyor, eklenen özellik yok. Kullanılanlar:
  - Q1: `Win32_System_Com` (`CoInitializeEx`, `CoCreateInstance`, `IPersistFile`, `STGM_READ`), `Win32_UI_Shell` (`IShellLinkW`, `ShellLink`, `SLR_*`, `SLGP_RAWPATH`), `Win32_Storage_FileSystem` (`WIN32_FIND_DATAW`).
  - Q2: `Win32_System_IO` (`DeviceIoControl`), `Win32_System_Ioctl` (`FSCTL_SET_REPARSE_POINT`, `FSCTL_GET_REPARSE_POINT`), `Win32_System_SystemServices` (`IO_REPARSE_TAG_MOUNT_POINT`, `IO_REPARSE_TAG_SYMLINK`), `Win32_Storage_FileSystem` (`CreateFileW`, `RemoveDirectoryW`), `Win32_Security` ve `Win32_System_Threading` (token denetimi).
  - Q3: `Win32_System_DataExchange`, `Win32_System_Memory`, `Win32_System_Ole` (`CF_DIB`, `CF_DIBV5`, `CF_UNICODETEXT`).
  - Q4: `Win32_UI_Shell` + `Win32_UI_WindowsAndMessaging` (`ShellExecuteW`, `SW_SHOWNORMAL`).
  - Q5: hiçbiri (std'nin `MetadataExt::file_attributes`). İsteğe bağlı olarak `Win32_UI_Shell`'den `SHGetSetSettings`.
- **Tek tuzak:** `ShellExecuteExW` ve `SHELLEXECUTEINFOW`, windows 0.62'de `Win32_System_Registry` özelliğinin arkasında (yapıda `HKEY` alanı var). Bu özellik listede yok. Bu yüzden `ShellExecuteW` kullanıldı; yükseltme için o da yetiyor (4.1). Kayıt defterinden Geliştirici Modu okumak da aynı özelliği isterdi; deneme yoluyla gerek kalmıyor (2.3).
- **macOS:** yeni crate yok. Var olan `objc2-app-kit 0.3`'e `"NSBitmapImageRep"` ve `"NSImageRep"` özellikleri, `objc2-foundation 0.3`'e `"NSData"` ve `"NSDictionary"` özellikleri eklenmeli: TIFF→PNG çevirisi için (3.3). `NSPasteboard` ve `NSImage` zaten açık.
- **Linux:** yeni crate yok. Gezik'in `x11rb` ve `wayland-client`'ı yetiyor. Deneme `wayland-protocols` (xdg-shell) kullandı, ama yalnız kendi penceresini açmak için. Gezik winit'in yüzeyini kullandığından buna gerek yok.
- **image:** var olan `image 0.25.10` (`png`, `bmp`) yetiyor. TIFF özelliği gerekmiyor; macOS'ta TIFF'i AppKit çeviriyor.
- **arboard 3.6.1** `Cargo.lock`'ta zaten var (`i-slint-backend-winit` getiriyor). Kullanmayı önermiyorum:
  - Linux'ta kendi X11/Wayland bağlantısını kurar, Gezik'in var olan arka uçlarıyla (`linux/x11.rs`, `linux/wayland.rs`) yarışır.
  - Görüntü desteği ayrı bir `image-data` özelliği ister.
  - Gezik'in zaten sahip olduğu biçim seçimini (PNG > DIBV5 > DIB) bize bırakmaz.

---

## 1. Windows .lnk kısayolu (IShellLinkW + IPersistFile)

### 1.1 Kod (`lnk.rs`, çalıştı)
```rust
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, STGM_READ};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, SLR_NO_UI, SLR_NOSEARCH, SLR_NOTRACK, SLR_NOUPDATE, ShellLink};
use windows::core::{HSTRING, Interface};

/// COM for this thread until dropped.
pub struct Com(bool);
impl Com {
    pub fn init() -> windows::core::Result<Com> {
        // S_OK: started here; S_FALSE: already on (same model); RPC_E_CHANGED_MODE: an MTA thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        hr.ok()?;
        Ok(Com(true))
    }
}
impl Drop for Com { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize() } } } }

pub fn create(target: &Path, link: &Path, description: Option<&str>) -> windows::core::Result<()> {
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        shell_link.SetPath(&HSTRING::from(target.as_os_str()))?;
        if let Some(dir) = target.parent() {
            shell_link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        if let Some(text) = description { shell_link.SetDescription(&HSTRING::from(text))?; }
        let file: IPersistFile = shell_link.cast()?;
        file.Save(&HSTRING::from(link.as_os_str()), true)
    }
}

pub fn read(link: &Path) -> windows::core::Result<LinkInfo> {
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let file: IPersistFile = shell_link.cast()?;
        file.Load(&HSTRING::from(link.as_os_str()), STGM_READ)?;
        // No UI, no search, no tracking, no rewrite of the .lnk.
        let resolve = shell_link.Resolve(HWND::default(),
            (SLR_NO_UI.0 | SLR_NOSEARCH.0 | SLR_NOTRACK.0 | SLR_NOUPDATE.0) as u32);
        let mut buf = vec![0u16; 32768];
        let mut find = WIN32_FIND_DATAW::default();
        shell_link.GetPath(&mut buf, &mut find, SLGP_RAWPATH.0 as u32)?;   // find.dwFileAttributes: 0x10 = klasör
        // GetWorkingDirectory(&mut buf), GetArguments(&mut buf) aynı biçimde
        ...
    }
}
```
İş parçacığı: `std::thread::spawn(move || { let _com = Com::init()?; create(...); read(...); })`. ShellLink in-proc bir nesne; STA iş parçacığında mesaj döngüsü gerekmedi. Gezik'in başka yerleri (`known.rs`, `icons.rs`, `fs/windows.rs::trash`) `let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED)` yapıp hiç kapatmıyor. Aynı kalıp burada da çalışır. Ancak `Com` koruyucusu, iş parçacığı havuzunda MTA'ya çevrilmiş bir iş parçacığına düşüldüğünde `RPC_E_CHANGED_MODE`'u görünür kılıyor.

### 1.2 Sonuçlar (`lnk.out.txt`)
| Hedef | Sonuç |
|---|---|
| `…\probe7 lnk Ğüşİ 日本 🙂\belge ç 日本.txt` (klasör adında emoji ve CJK var) | Kaydedildi (ilk çağrı 23 ms, sonrakiler 2-4 ms). Geri okunan yol aynı, `dwFileAttributes` 0x20. |
| Bir klasör (`hedef klasör ş`) | Kaydedildi. `dwFileAttributes` = 0x10, yani okuyan tarafta klasör olduğu `WIN32_FIND_DATAW`'dan anlaşılıyor. |
| Olmayan dosya | Kaydedildi. `Resolve(SLR_NO_UI…)` yine `Ok`, `dwFileAttributes` 0. **Kırık kısayolu Gezik kendisi denetlemeli** (hedefin `exists()`'i). |
| 441 karakterlik hedef (MAX_PATH üstü) | `SetPath` → `E_FAIL (0x80004005)`. `\\?\` önekiyle de aynı. **.lnk hedefi 260 karakteri geçemez.** Hata metni "Unspecified error" olduğundan kullanıcıya Gezik kendi iletisini göstermeli. |

Explorer tarafı: `Shell.Application`'ın `GetLink.Path`'i üç kısayolu da doğru Unicode yolla okudu ve `IsLink=True` döndü. Karşılaştırma: `WScript.Shell.CreateShortcut` aynı dosyalarda `?` gösteriyor (ANSI'ye düşüyor). Bu, WSH'nin bilinen sınırı; kısayollar sağlam.

### 1.3 Öneriler
- Ad: Explorer'ın kalıbı `"<ad> - Kısayol.lnk"` biçimindedir (yerelleştirilmiş). Gezik kendi metnini (`"<ad> - Shortcut.lnk"` / i18n) kullanmalı ve çakışmada `(2)` eklemeli.
- Geri alma: oluşturulan `.lnk` sıradan bir dosya, `gezik_platform::fs::delete` ile silinir.
- Okuma ("kısayolun hedefine git"): `SLR_NOUPDATE` olmadan `Resolve`, hedef taşınmışsa .lnk'yi yeniden yazabilir; salt okuma için bayrak açık kalmalı.
- macOS/Linux'ta karşılığı symlink (`std::os::unix::fs::symlink`); ayrıca denenmedi.

---

## 2. Windows junction (yönetici olmadan), symlink izni, güvenli kaldırma

### 2.1 Oluşturma: `FSCTL_SET_REPARSE_POINT` + mount-point arabelleği (`junction.rs`, çalıştı)
```rust
const HEADER: usize = 8;              // ReparseTag u32, ReparseDataLength u16, Reserved u16
const MOUNT_POINT_HEADER: usize = 8;  // SubstituteNameOffset/Length, PrintNameOffset/Length (u16)
const MAX_REPARSE: usize = 16 * 1024; // MAXIMUM_REPARSE_DATA_BUFFER_SIZE

/// SubstituteName `\??\C:\x` then PrintName `C:\x`, each NUL-terminated (lengths leave the NULs out).
pub fn mount_point_buffer(target: &Path) -> io::Result<Vec<u8>> {
    let text = target.as_os_str().to_string_lossy();
    let plain = text.strip_prefix(r"\\?\").unwrap_or(&text);
    if plain.starts_with(r"\\") || !target.is_absolute() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a junction needs an absolute local path"));
    }
    let print: Vec<u16> = plain.encode_utf16().collect();
    let substitute: Vec<u16> = format!(r"\??\{plain}").encode_utf16().collect();
    let (sub_bytes, print_bytes) = (substitute.len() * 2, print.len() * 2);
    let data_len = MOUNT_POINT_HEADER + sub_bytes + 2 + print_bytes + 2;
    if HEADER + data_len > MAX_REPARSE { return Err(/* too long */) }
    let mut b = Vec::with_capacity(HEADER + data_len);
    b.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    b.extend_from_slice(&(data_len as u16).to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());                     // SubstituteNameOffset
    b.extend_from_slice(&(sub_bytes as u16).to_le_bytes());
    b.extend_from_slice(&((sub_bytes + 2) as u16).to_le_bytes()); // PrintNameOffset
    b.extend_from_slice(&(print_bytes as u16).to_le_bytes());
    for u in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) { b.extend_from_slice(&u.to_le_bytes()); }
    Ok(b)
}

pub fn create(link: &Path, target: &Path) -> io::Result<()> {
    let buffer = mount_point_buffer(target)?;
    std::fs::create_dir(link)?;                       // boş klasör, sonra reparse noktası olur
    let result = (|| {
        // CreateFileW(GENERIC_WRITE, share all, OPEN_EXISTING,
        //             FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        let handle = open(link, GENERIC_WRITE.0)?;
        let done = unsafe { DeviceIoControl(handle, FSCTL_SET_REPARSE_POINT,
            Some(buffer.as_ptr() as *const c_void), buffer.len() as u32, None, 0, None, None) };
        let _ = unsafe { CloseHandle(handle) };
        done.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
    })();
    if result.is_err() { let _ = std::fs::remove_dir(link); }   // yarım kalmasın
    result
}
```
`FSCTL_GET_REPARSE_POINT` ile geri okuma da var (`read()`). Junction'da adlar `HEADER + 8`'den, symlink'te `HEADER + 12`'den başlar (araya u32 Flags girer). `\??\` öneki atılır.

### 2.2 Sonuçlar (`junction.out.txt`; yükseltilmemiş, `SeCreateSymbolicLinkPrivilege` yok)
- Aynı birimde, adında `Ğş 日本` geçen junction **yönetici olmadan** 0,12 ms'de oluştu. İçinden dosya okundu (`"içerik"`).
- `symlink_metadata`: `is_symlink()=true`, `is_dir()=false`, öznitelik 0x410 (DIRECTORY | REPARSE_POINT). Gezik'in `list_dir`'i bunu zaten `is_symlink() && path().is_dir()` ile klasör sayıyor.
- **Birimler arası** (C: → `D:\tmp\…`): çalıştı.
- **Olmayan hedef:** oluştu (Windows denetlemiyor). İçinden `exists()` false. Gezik kendisi denetlemeli.
- **Dosya hedefi:** oluştu ama `read_dir` 267 (NotADirectory) verdi. Junction yalnız klasöre yapılmalı.
- **Göreli yol ve UNC:** probe en baştan reddediyor. Junction yalnız yerel birimlere işaret edebilir; ağ paylaşımı için symlink gerekir.
- **379 karakterlik hedef** (MAX_PATH üstü): çalıştı (arabellek sınırı 16 KB).

### 2.3 Symlink izni nasıl anlaşılır
- `std::os::windows::fs::symlink_dir` / `symlink_file`, std'nin içinde zaten `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` (0x2) bayrağını geçiyor. Bu makinede (Geliştirici Modu kapalı, yükseltilmemiş) ikisi de **1314 `ERROR_PRIVILEGE_NOT_HELD`** verdi.
- Kayıt defteri: `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock` anahtarı var ama `AllowDevelopmentWithoutDevLicense` değeri yok (Geliştirici Modu kapalı). Bu değeri okumak `Win32_System_Registry` özelliğini ister. Üstelik yalnız Geliştirici Modu'nu söyler; yükseltilmiş yöneticiyi ve ilkeyle verilmiş ayrıcalığı kaçırır.
- Token'da `SeCreateSymbolicLinkPrivilege` aramak (`LookupPrivilegeValueW` + `GetTokenInformation(TokenPrivileges)`, var olan özelliklerle) çalıştı: `false`. Bu da Geliştirici Modu'nu kaçırır, çünkü Geliştirici Modu ayrıcalık eklemez, bayrağı geçirir.
- **Önerilen:** oturum başına bir kez temp'te deneme. Sonucu önbelleğe al:
```rust
pub fn can_symlink() -> bool {
    let dir = std::env::temp_dir();
    let link = dir.join(format!("gezik-symlink-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&link);
    let ok = std::os::windows::fs::symlink_file(dir.join("gezik-nonexistent-target"), &link).is_ok();
    let _ = std::fs::remove_file(&link);
    ok
}
```
Menüde "Sembolik bağlantı" bu `false` iken gizlenmeli ya da gri olmalı. Klasörler için "Junction" her zaman sunulabilir. Hata 1314 gelirse ileti: "Geliştirici Modu'nu açın ya da yönetici olarak çalıştırın".

### 2.4 Güvenli kaldırma (geri alma)
```rust
/// Undo of "make junction": removes `link` only if it is still a junction to `target`.
/// RemoveDirectoryW on a reparse point removes the point, never what it points to.
pub fn remove_if_points_to(link: &Path, target: &Path) -> io::Result<()> {
    match read(link)? {
        Reparse::Junction(to) if same(&to, target) => {}
        other => return Err(io::Error::other(format!("not the junction made: {other:?}"))),
    }
    unsafe { RemoveDirectoryW(&HSTRING::from(link.as_os_str())) }.map_err(...)
}
```
Denenen yollar (hepsinde hedefin içindeki `alt\dosya ü.txt` **yerinde kaldı**):
- `remove_if_points_to` (`RemoveDirectoryW`): bağlantı gitti, hedef sağlam.
- **`gezik_platform::fs::delete(link)`**: Gezik'in var olan silmesi `FILE_FLAG_OPEN_REPARSE_POINT` ile açıp POSIX silme yapıyor. Yalnız bağlantı gitti. Yani geri alma için Gezik'in kendi `delete`'i de güvenli.
- İçinde junction olan bir klasöre `std::fs::remove_dir_all`: junction'ın içine girmedi, hedef sağlam.
- `gezik_platform::fs::trash(link)` (IFileOperation): bağlantıyı Geri Dönüşüm Kutusu'na taşıdı, hedef sağlam. (Kutuya düşen bağlantı deneme sonrası `Directory.Delete(non-recursive)` ile temizlendi.)
- Ret durumları: başka hedefe dönmüş junction ("not the junction made") ve sıradan klasör (4390 `ERROR_NOT_A_REPARSE_POINT`) silinmedi.

Öneri: geri alma kaydı `(link, target)` tutsun. Geri alırken `remove_if_points_to` çağrılsın; kullanıcı bağlantıyı değiştirdiyse dokunulmasın.

---

## 3. Pano: resim ve metin okuma

### 3.1 Bugün ne var
`gezik-platform/src/clipboard.rs` yalnız **dosya listesi** taşıyor: `write_files`, `read_files`, `sequence`, `clear`.
- Windows: `CF_HDROP` + `Preferred DropEffect`.
- macOS: `NSURL` nesneleri + `changeCount` ile kendi "kes" işareti.
- Linux: `linux::Backend` trait'i. X11'de seçim sahipliği, `transfer()` (INCR'li) ve XFixes. Wayland'da winit'in bağlantısı üzerinde `wl_data_device`, `receive()` + `read_with_timeout`. Biçimler `x-special/gnome-copied-files`, `text/uri-list`, `application/x-kde-cutselection`. Metin yalnız **yazılıyor** (Wayland kaynağı `TEXT` sunuyor), okunmuyor.

Resim ve metin okuma yeni. Linux'ta var olan `transfer`/`receive` doğrudan yeniden kullanılabilir (3.4). Önerilen API:
```rust
pub enum ClipboardImage { Png(Vec<u8>), Rgba(image::RgbaImage) }
pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError>;
pub fn read_text() -> Result<Option<String>, ClipboardError>;
// Backend trait'ine: fn read_image(&self) -> ...; fn read_text(&self) -> ...;
```

### 3.2 Windows (`clip.rs`, çalıştı)
Biçim sırası: kayıtlı **"PNG"** (`RegisterClipboardFormatW(w!("PNG"))`; tarayıcılar ve Office koyar, alfa korunur, hiç kodlama yok), sonra **CF_DIBV5**, sonra **CF_DIB**. Pano yalnız baytlar kopyalanırken açık. Çözme pano kapandıktan sonra.

```rust
/// CF_DIB / CF_DIBV5 bytes (no BITMAPFILEHEADER) to RGBA. A BITMAPFILEHEADER is put in
/// front so the pixel offset is ours.
pub fn decode_dib(bytes: &[u8]) -> Option<image::RgbaImage> {
    let file = with_file_header(bytes)?;
    let decoder = image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(file)).ok()?;
    let mut rgba = image::DynamicImage::from_decoder(decoder).ok()?.into_rgba8();
    fix_alpha(&mut rgba);
    Some(rgba)
}

/// "BM" header whose bfOffBits points at the pixels: for uncompressed DIBs the pixels are
/// the last stride * height bytes; otherwise header + masks + palette.
pub fn with_file_header(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |i: usize| Some(u32::from_le_bytes(dib.get(i..i + 4)?.try_into().ok()?));
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = (u32_at(8)? as i32).unsigned_abs() as usize;
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?) as usize;
    let compression = u32_at(16)?;
    let colors = u32_at(32)? as usize;
    let stride = (width.unsigned_abs() as usize * bits).div_ceil(32) * 4;
    let pixels_at = if compression == 0 || compression == 3 {
        dib.len().checked_sub(stride * height)?
    } else {
        let masks = if compression == 3 && header == 40 { 12 } else { 0 };
        let palette = if bits <= 8 { (if colors == 0 { 1 << bits } else { colors }) * 4 } else { 0 };
        header + masks + palette
    };
    if pixels_at < header { return None; }
    let mut file = Vec::with_capacity(14 + dib.len());
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&((14 + pixels_at) as u32).to_le_bytes());
    file.extend_from_slice(dib);
    Some(file)
}

/// A screenshot's alpha is often all 0: such an image is opaque, not invisible.
pub fn fix_alpha(rgba: &mut image::RgbaImage) -> bool {
    let first = rgba.as_raw()[3];
    if rgba.pixels().all(|p| p.0[3] == first) && first != 255 {
        rgba.pixels_mut().for_each(|p| p.0[3] = 255);
        return true;
    }
    false
}
```
Metin: `GetClipboardData(CF_UNICODETEXT)` → `GlobalLock` → ilk NUL'a kadar `String::from_utf16_lossy`.
- `"Merhaba ğüşİı 日本 🙂\r\nikinci satır"` gidip geldi.
- Panoda yalnız `CF_TEXT` (ANSI) varken Windows `CF_UNICODETEXT`'i kendisi üretti.

**Bulunan tuzaklar:**
1. **`BmpDecoder::new_without_file_header` (image 0.25.10)**, V4/V5 başlık + `BI_BITFIELDS` görünce başlıktan sonra 12 baytlık maske **olduğunu varsayıyor**. Windows'un kendi ürettiği CF_DIBV5'te bu maske gerçekten var: gerçek Alt+PrtScn'de CF_DIBV5 = 124 + **12** + pikseller. Maskeyi yazmayan bir program V5 koyarsa çözme `UnexpectedEof` ("failed to fill whole buffer") ile düşüyor; probe'da denendi. Piksel ofsetini sondan hesaplayıp `BITMAPFILEHEADER` eklemek iki düzende de çalıştı. 40 baytlık başlık + maskeli CF_DIB de çözüldü.
2. **Alfa:** `BI_BITFIELDS` + alfa maskeli bir V5 ve sıfır alfa baytları, olduğu gibi çözülünce alfa = 0 olur, yani görünmez PNG. `fix_alpha` bunu düzeltti. Gerçek Alt+PrtScn'de alfa zaten 255 çıktı (CF_DIB: 40 + maskeler, compression 3; CF_DIBV5'te alfa maskesi yok). Düzeltme yine de gerekli; tek geçiş, 4K'da ~5 ms.
3. Gerçek **Alt+PrtScn** (`SendKeys "%{PRTSC}"`) panoya yalnız `CF_BITMAP(2)`, `CF_DIB(8)`, `CF_DIBV5(17)` koydu, "PNG" yok (`clip-altprtscn.out.txt`). .NET `Clipboard.SetImage` da "PNG" koymuyor (DataObject + Bitmap + DIB/DIBV5). Snipping Tool'un (Win+Shift+S) "PNG" koyup koymadığı denenmedi; kod iki durumu da karşılıyor.

**4K maliyeti** (release, 3840×2160, `clip.out.txt`):
| Adım | Süre |
|---|---|
| CF_DIB koyma (33,2 MB) | 12 ms |
| CF_DIBV5 okuma (Windows'un ürettiği, 33,2 MB kopya) | 27-29 ms |
| okuma + DIB çözme + alfa (toplam) | 40-48 ms |
| "PNG" hazır gelince okuma (5 MB) | 3 ms (kodlama yok) |
| PNG kodlama, sahte ekran görüntüsü: Fast / Default / Best | 17 ms 4,99 MB / 250 ms 4,37 MB / 690 ms 4,37 MB |
| PNG kodlama, gerçek masaüstü 2560×1440: Fast / Default / Best | 19 ms 3,30 MB / 434 ms 2,78 MB / 1,66 s 2,67 MB |
| aynı masaüstü 4K'ya döşenmiş: Fast / Default / Best | 40 ms 6,87 MB / 686 ms 4,45 MB / 2,36 s 4,23 MB |

Öneri:
- `CompressionType::Fast` + `FilterType::Adaptive` kullanılsın. 4K'da ~20-40 ms; dosya Default'tan %15-50 büyük.
- Default isteniyorsa yarım saniyelik iş, iş parçacığında yapılsın.
- Tepe bellek: DIB kopyası (33 MB) + RGBA (33 MB) + PNG (~5 MB), yaklaşık 70 MB, geçici.

### 3.3 macOS (`clip.rs` `mac` modülü, yalnız `cargo check`)
```rust
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypePNG,
    NSPasteboardTypeString, NSPasteboardTypeTIFF};
use objc2_foundation::{NSData, NSDictionary};

/// PNG bytes of the pasteboard's picture: public.png as is, else public.tiff converted by AppKit.
pub fn read_png() -> Option<Vec<u8>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    unsafe {
        if let Some(png) = pasteboard.dataForType(NSPasteboardTypePNG) { return Some(png.to_vec()); }
        let tiff: Retained<NSData> = pasteboard.dataForType(NSPasteboardTypeTIFF)?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let png = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?;
        Some(png.to_vec())
    }
}
pub fn read_text() -> Option<String> {
    unsafe { NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeString) }.map(|s| s.to_string())
}
```
- Gereken özellikler: `objc2-app-kit`: `"NSBitmapImageRep"`, `"NSImageRep"` (+ var olan `"NSPasteboard"`, `"NSImage"`); `objc2-foundation`: `"NSData"`, `"NSDictionary"`.
- TIFF'i image crate'ine çözdürmek `tiff` özelliğini ve yeni bir crate'i (`tiff`, `weezl`…) getirirdi; AppKit çevirisi bunu önlüyor.
- Çalıştırılmadı: ekran görüntüsünün (Cmd+Ctrl+Shift+4) hangi türleri koyduğu ve süreler MacBook'ta denenmeli (çapraz platform test planına eklenecek).

### 3.4 Linux (`clip_x11.rs`, `clip_wayland.rs`, Docker'da çalıştı; `linux.out.txt`)
**X11** (Xvfb + `xclip -t image/png`). Gezik'in `transfer()`'ının aynısı: `convert_selection` → `SelectionNotify` → `get_property`; `INCR`'de `PropertyNotify(NEW_VALUE)` başına bir parça.
- `TARGETS` 0,4-0,6 ms.
- `image/png` 4,99 MB: **5 INCR parçası, 7-8 ms**. 37,8 MB'lık (sıkışmayan plazma) 4K PNG: **37 parça, 24-26 ms**. xclip parçaları ~1 MB.
- `UTF8_STRING`: Türkçe, CJK ve emoji doğru, 0,4 ms. Düz `xclip` yalnız `UTF8_STRING` sunuyor; GTK/Qt ayrıca `text/plain;charset=utf-8` sunar. Sıra `UTF8_STRING` > `text/plain;charset=utf-8` > `STRING`.

**Wayland** (başsız sway + `wl-copy`). Kendi xdg_toplevel'ı ve `wl_data_device`'ı olan istemci. Okuma Gezik'in `receive` + `read_with_timeout`'unun aynısı (pipe + poll).
- `image/png` 4,99 MB **4,5 ms**, 37,8 MB **30 ms**.
- Metinde sunulan türler: `text/plain`, `text/plain;charset=utf-8`, `TEXT`, `STRING`, `UTF8_STRING`. Hepsi doğru, < 1 ms.

Sonuçlar:
- **Gezik'in 1 s'lik `TRANSFER_TIMEOUT`'u yerel resim için fazlasıyla yetiyor** (en kötü 30 ms). Ancak X11'deki süre **bütün aktarım için tek bir son tarih**. Uzak X'te (ssh -X) ya da yavaş sahipte büyük bir resim kesilebilir. Öneri: resim okumada son tarihi INCR parçası başına yenilemek (ya da resim için 5 s).
- Linux'ta PNG kodlama gerekmiyor; sahipler `image/png` veriyor. Yalnız `image/png` yoksa (bazı uygulamalar `image/bmp`, `image/jpeg` sunar) `image` ile çözüp kodlamak gerekir. Sıra: `image/png` > `image/bmp` > `image/jpeg`.
- Wayland'da seçim (bilinen kural, Gezik'in notunda da var) yalnız klavyesi olan pencereye bildiriliyor; yapıştırma anında Gezik odakta olduğundan sorun değil.

---

## 4. Uçbirim açma

### 4.1 Windows (`term.rs`, çalıştı; `term.out.txt`)
```rust
/// Windows Terminal through its App Execution Alias, if installed and the alias is on.
pub fn find_wt() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let alias = PathBuf::from(local).join(r"Microsoft\WindowsApps\wt.exe");
    // The alias is a reparse point (IO_REPARSE_TAG_APPEXECLINK): symlink_metadata sees it.
    std::fs::symlink_metadata(&alias).is_ok().then_some(alias)
}
/// wt splits its command line at `;` (a new-tab separator) even inside one argument: `\;` keeps it.
pub fn wt_dir_arg(dir: &Path) -> String { dir.to_string_lossy().replace(';', r"\;") }
pub fn wt_command(wt: &Path, dir: &Path) -> Command {
    let mut c = Command::new(wt);
    c.arg("-d").arg(wt_dir_arg(dir));
    c
}
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
pub fn powershell_command(dir: &Path) -> Command {   // yedek
    let mut c = Command::new("powershell.exe");
    c.arg("-NoExit").current_dir(dir).creation_flags(CREATE_NEW_CONSOLE);
    c
}
/// Elevated: ShellExecuteW "runas" (ShellExecuteExW needs Win32_System_Registry). A cancelled
/// UAC prompt returns 5 (SE_ERR_ACCESSDENIED) and GetLastError() == ERROR_CANCELLED (1223).
pub fn shell_execute(verb: &str, file: &str, params: &str, dir: &Path) -> Result<(), (isize, u32)> {
    let r = unsafe { ShellExecuteW(None, &HSTRING::from(verb), &HSTRING::from(file),
        &HSTRING::from(params), &HSTRING::from(dir.as_os_str()), SW_SHOWNORMAL) };
    let code = r.0 as isize;
    if code > 32 { Ok(()) } else { Err((code, unsafe { GetLastError() }.0)) }
}
pub fn ps_quote(s: &str) -> String { format!("'{}'", s.replace('\'', "''")) }
```
Doğrulama yöntemi: her uçbirim, açıldığı klasörü bir dosyaya yazan bir PowerShell komutu çalıştırıp kapandı.

Sonuçlar:
- Takma ad (alias) için Rust 1.99'da `exists()`, `symlink_metadata` ve `metadata` hepsi `Ok`. App Execution Alias'ı std artık görüyor. `wt.exe` 40-55 ms'de **0 ile çıkıyor**: asıl pencereyi `WindowsTerminal.exe`'ye devrediyor. Uçbirimin sonradan başarısız olup olmadığı bilinemez.
- `wt -d "…\boşluk ve ş 日本"`: doğru klasör.
- `wt -d "…\noktalı\;virgül"`: doğru. **Kaçışsız `;`** ile hiç çıktı gelmedi (wt komutu `;`'den böldü). `\;` kaçışı şart.
- `wt -d "…\[köşeli] it's"`: **PowerShell'in kendi klasörüne düştü** (`C:\WINDOWS\System32\WindowsPowerShell\v1.0`). Aynısı `powershell.exe` + `current_dir` ile de oldu. Windows PowerShell 5.1, başlangıç klasöründe `[` olunca Set-Location'ı joker sanıp yapamıyor. **Düzeltme:** klasörü argümanla ve `-LiteralPath` ile ver: `-NoExit -Command Set-Location -LiteralPath '<dir>'`. Bu biçim (`ShellExecuteW` ile) üç klasörde de doğruydu. wt'nin varsayılan profili PowerShell 5.1 ise `wt -d` köşeli parantezli klasörde aynı sorunu yaşar. Profil bilinmediğinden bu uç durum kabul edilebilir ya da `[` varsa `wt -d <dir> powershell -NoExit -Command "Set-Location -LiteralPath '…'"` kullanılabilir.
- `ShellExecuteW("open", "powershell.exe", "-NoProfile -Command Set-Location -LiteralPath '<dir>'; …", dir)`: üç klasörde de doğru.
- `ShellExecuteW("open", wt.exe, "-d \"<dir>\" …")`: doğru.
- **`runas` çalıştırılmadı.** Bu makinede `ConsentPromptBehaviorAdmin = 5`, UAC penceresi kullanıcı tıklayana kadar bekletirdi. `runas` yalnız fiil farkı; parametre ve klasör biçimi "open" ile doğrulandı. Klasörün argümanla (`wt -d`, `Set-Location -LiteralPath`) verilmesinin nedeni, yükseltilmiş süreçte `lpDirectory`'ye güvenmemek. Kullanıcı tıklayarak MacBook/Linux turundan önce bir kez denemeli.
- Sıra önerisi: `wt` (alias varsa) > `pwsh.exe` (PATH'te varsa; bu makinede yok) > `powershell.exe` > `cmd.exe /K`. Hepsine `current_dir(dir)` de verilsin.

### 4.2 macOS (yalnız `cargo check`)
```rust
/// `open -a Terminal <dir>`: a new Terminal window in that folder. `app` may name another
/// (iTerm, Ghostty, WezTerm); `open` fails (exit 1) if it is not installed.
pub fn open_terminal(dir: &Path, app: Option<&str>) -> std::io::Result<()> {
    let status = Command::new("/usr/bin/open").arg("-a").arg(app.unwrap_or("Terminal")).arg(dir).status()?;
    if status.success() { Ok(()) } else { Err(std::io::Error::other(format!("open: {status}"))) }
}
```
Yükseltme macOS'ta anlamsız (sudo uçbirim içinde). Çalışma davranışı MacBook turunda denenecek.

### 4.3 Linux (`term.rs` `linux_plan`; Docker'da 15 uçbirimle çalıştı; `linux.out.txt`, `terms.out.txt`, `ptyxis.out.txt`)
Sıra: `$TERMINAL` (program + isteğe bağlı sözcükler) > `x-terminal-emulator` (Debian alternatifi) > bilinen liste. `x-terminal-emulator` ve sarmalayıcılar `canonicalize` ile çözülüp gerçek uçbirimin bayrağı kullanılıyor. Her uçbirime **ayrıca `current_dir(dir)`** veriliyor; bayraksız olanlar (xterm, st) bununla çalışıyor.

Tablo, testten sonra düzeltilmiş hâliyle:
| Uçbirim | Bayrak | Denendi |
|---|---|---|
| ptyxis (GNOME 47+) | `--new-window --working-directory DIR` | ok. **Yalnız `--working-directory` yok sayılıyor** (kabuk `$HOME`'da açıldı). Yardım metni: "Use DIR for --tab, --tab-with-profile, --new-window, or -x". Çalışan bir örnek varken de ok. |
| kgx (GNOME Console) | `--working-directory DIR` | ok |
| gnome-terminal | `--working-directory=DIR` | ok (sunucusu D-Bus'ta; cwd miras kalmaz, bayrak şart) |
| konsole | `--workdir DIR` | ok |
| xfce4-terminal | `--working-directory=DIR` | ok |
| mate-terminal | `--working-directory=DIR` | ok |
| lxterminal | `--working-directory=DIR` | ok |
| qterminal | `--workdir DIR` | ok |
| tilix | `--working-directory=DIR` | ok |
| terminator | `--working-directory=DIR` | ok |
| terminology | `--current-directory=DIR` | ok |
| alacritty | `--working-directory DIR` | ok |
| kitty | `--directory DIR` | ok |
| foot | `--working-directory=DIR` | ok (Wayland, sway; x-terminal-emulator → foot) |
| urxvt | `-cd DIR` | ok |
| xterm | yalnız cwd | ok |
| wezterm | `start --cwd DIR` | Debian'da yok, denenmedi (belgelere göre) |
| ghostty | `--working-directory=DIR` | Debian'da yok, denenmedi |
| deepin-terminal | `--work-directory DIR` | denenmedi |
| cosmic-term, st | yalnız cwd | denenmedi |

- Doğrulama: kabuk `pwd`'yi yazıp çıkan bir betikti (`SHELL` ve `/etc/passwd`'de; VTE tabanlılar passwd'deki kabuğu kullanıyor). Gezik'in kendi cwd'si `/` iken klasör `/tmp/t/boşluk ş 日本` idi.
- Bayrağın tek başına çalıştığı da ayrıca doğrulandı (cwd `/` iken, `terms.out.txt`'nin "flags alone" bölümü). ptyxis'in oradaki FAIL'i eski bayrakla; düzeltilmiş hâli `ptyxis.out.txt`'de ok.
- `$TERMINAL="foot --app-id x"` → `foot --app-id x --working-directory=/tmp`.
- Olmayan `$TERMINAL`, `x-terminal-emulator`'a düşüyor. Hiçbiri yoksa "no terminal found".
- Klasör adında boşluk, `ş`, CJK, `[`, `'` sorun çıkarmadı (argümanlar ayrı geçiyor, kabuk yok).
- Gezik çocuğu beklemez. Bazı uçbirimler (foot, xterm) kapanana dek yaşar; zombi kalmasın diye çocuk bir iş parçacığında `wait` edilmeli ya da çift fork / `setsid` kullanılmalı.

---

## 5. Windows gizli ve sistem öznitelikleri

### 5.1 Bugün ne var
- `gezik_core::list_dir` (`crates/gezik-core/src/lib.rs`), `std::fs::read_dir` + `DirEntry::metadata()` okuyor. `Entry`'de gizlilik alanı **yok**.
- Görünümdeki "gizlileri göster" (`crates/gezik/src/view/mod.rs` `show_hidden`, `view/listing.rs` `without_dotfiles`) yalnız **nokta ile başlayan adları** süzüyor, her platformda. Windows'ta varsayılan `show_hidden = true` (macOS'ta false).
- `gezik-platform/src/fs/windows.rs`'te `set_hidden` / `clear_hidden` / `is_hidden_attr` var (`GetFileAttributesW`, dosya başına bir çağrı). Bunlar anlık silmenin gizli klasörü için; listelemede kullanılmıyor.

### 5.2 Ucuz okuma (`attrs.rs`, çalıştı; `attrs.out.txt`)
Windows'ta `DirEntry::metadata()` `FindNextFileW`'nin `WIN32_FIND_DATAW`'ından geliyor. Öznitelikler zaten elde, ek sistem çağrısı yok:
```rust
#[cfg(windows)]
pub fn flags(meta: &std::fs::Metadata) -> Flags {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2; // FILE_ATTRIBUTE_HIDDEN
    const SYSTEM: u32 = 0x4; // FILE_ATTRIBUTE_SYSTEM
    let a = meta.file_attributes();
    Flags { hidden: a & HIDDEN != 0, system: a & SYSTEM != 0 }
}
/// macOS: UF_HIDDEN (chflags hidden; Finder hides these), from the lstat Gezik already does.
#[cfg(target_os = "macos")]
pub fn flags(meta: &std::fs::Metadata) -> Flags {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    Flags { hidden: meta.st_flags() & UF_HIDDEN != 0, system: false }
}
```
Ölçüm (5 çalıştırmanın en iyisi, sıcak önbellek):
| Klasör | Öğe | read_dir+metadata | + öznitelikler | gezik_core::list_dir | + dosya başına GetFileAttributesW |
|---|---|---|---|---|---|
| `C:\` | 33 (12 gizli, 8 sistem, 8 ikisi) | 25,8 µs | 25,6 µs | 128 µs | 353 µs |
| `C:\Windows\System32` | 4960 (1 gizli, 7 sistem) | 1,36 ms | 1,33 ms | 4,6 ms | **106,8 ms** |
| `C:\Users\teoma` | 78 (18 gizli, 16 sistem, 16 ikisi) | 63 µs | 55 µs | 384 µs | 1,55 ms |

Sonuç:
- Öznitelik okumanın **maliyeti ölçülemeyecek kadar küçük**. Dosya başına `GetFileAttributesW` (bugünkü `is_hidden_attr` yolu) System32'de 80 kat yavaş; listelemede kullanılmamalı.
- Öneri: `Entry`'ye `hidden: bool` ve `system: bool` (ya da tek bir `u8` bayrak) eklensin. `is_dir`'in yanındaki dolguya sığar, `Entry` büyümez. `list_dir`'de `cfg(windows)` / `cfg(target_os = "macos")` ile doldurulsun.
- Yapıcı tek bir yerde (`list_dir`). Testlerdeki 4 yardımcı (`lib.rs:142`, `pattern.rs:179`, `sort.rs:302`) alanı eklemeli.
- Linux'ta öznitelik yok; nokta kuralı sürer. `without_dotfiles` Windows'ta `hidden`'ı, macOS'ta `UF_HIDDEN || '.'`'yı da süzmeli.

### 5.3 Explorer'ın kuralı
- **Gizli** (H): "Gizli öğeler" kapalıyken gizlenir.
- **Korunan işletim sistemi dosyası** (H **ve** S): ayrıca "Korunan işletim sistemi dosyalarını gizle" açıkken gizlenir (varsayılan açık).
- **Yalnız sistem** (S, H değil): **gösterilir**. System32'de `AppV`, `Configuration`, `Microsoft` gibi. Yani "system" tek başına gizleme nedeni değil.

Örnekler (`attrs.out.txt`):
- `C:\`: `$Recycle.Bin [H][S]`, `pagefile.sys [H][S]`, `hiberfil.sys [H][S]`, `Documents and Settings [H][S]` (junction), `OneDriveTemp [H]`.
- Ev klasöründe: `AppData [H]`, `NTUSER.DAT [H]`, `Application Data [H][S]`.

Explorer'ın ayarları `SHGetSetSettings` ile okunabiliyor (`Win32_UI_Shell`, yeni özellik yok):
```rust
let mut state = SHELLSTATEA::default();
unsafe { SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS | SSF_SHOWSUPERHIDDEN, false) };
let bits = state._bitfield1;   // fShowAllObjects = bit 0, fShowSuperHidden = bit 15
```
Bu makinede: gizli öğeleri göster = true, korunan sistem dosyalarını göster = false.

Öneri: Gezik'in "gizlileri göster"i iki kademeli olsun. Kapalıyken H gizlenir. Açıkken de H+S, ayrı bir "korunan sistem dosyalarını da göster" ayarı olmadan gizli kalır. Varsayılanlar Explorer'dan okunabilir ya da Gezik'in kendi ayarı olabilir.

---

## 6. Açık kalanlar / denenmeyenler
- `ShellExecuteW("runas")` gerçek UAC onayıyla denenmedi (kullanıcı tıklaması gerekir).
- macOS'ta hiçbir şey çalıştırılmadı (yalnız `cargo check`): `NSPasteboard` PNG/TIFF/metin okuma, `open -a Terminal`, `UF_HIDDEN`. MacBook test listesine eklenmeli.
- Snipping Tool'un (Win+Shift+S) panoya "PNG" koyup koymadığı denenmedi (Alt+PrtScn ve .NET denendi).
- Linux pano yalnız Xvfb/xclip ve başsız sway/wl-copy ile denendi. GNOME/KDE'nin gerçek ekran görüntüsü araçları, `image/png` dışı türler ve uzak X denenmedi.
- wezterm, ghostty, deepin-terminal, cosmic-term, st kurulmadı. Bayrakları belgelerden alındı.
- Engelleyici bir şey çıkmadı.
