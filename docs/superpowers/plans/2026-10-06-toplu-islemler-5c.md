# Toplu İşlemler 5c (Dönüştürme ve Kullanıcı Komutları) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resimleri (boyut, biçim, kalite, döndürme, konum silme), metinleri (kodlama, satır sonu) ve ses/videoyu (ffmpeg hazır ayarları) dönüştürmek; kullanıcının `settings.toml`'da tanımladığı komutları dosyalar üzerinde güvenle çalıştırmak; ffmpeg'i tek tıkla indirmek. Ayrıca araç indirmeyi curl'den sistemin kendi indiricisine (Windows WinHTTP, macOS NSURLSession, Linux curl/wget) taşımak.

**Architecture:** Saf parçalar (hazır ayarlar, çıktı adları, boyut hesabı, kalite eşlemeleri, ffmpeg argümanları, ilerleme ayrıştırma, sürüm okuma, komut şablonları) `gezik-core::batch::convert`'te. Resim, metin ve süreç kodu `gezik-batch::convert::{image, text, ffmpeg, command}`'ta; motor görevleri `ConvertTask` ve `CommandTask`. İndirme `gezik-platform::http`'te sistem API'leriyle; `DownloadTask` onu kullanır. Arayüz bir Convert katmanı (5a/5b katman kalıbı) ve "Commands ▸" menüsü.

**Tech Stack:** Rust 2024, Slint 1.18; `image` 0.25.10 (+`tiff`, `ico`), `jpeg-encoder` 0.7.1, `fast_image_resize` 6.1 (`resize_typed::<U8x4>`), `encoding_rs` 0.8.42, `chardetng` 1.0; `windows` (WinHTTP özellikleri), `objc2-foundation` (NSURLSession); ffmpeg 9.0.2 (indirilen).

**Spec:** `docs/superpowers/specs/2026-10-05-toplu-islemler-design.md` (bölüm 3, 6, 7, 8, 10-12)
**Kütüphane özeti (doğrulanmış kod ve komutlar):** `docs/superpowers/notes/2026-10-06-donusturme-kutuphaneleri.md` ve `docs/superpowers/notes/donusturme-probe/{img.rs, exifclean.rs, text.rs}`.

## Spec'ten sapmalar ve netleştirmeler

1. **Araç indirme sistem indiricisiyle** (2026-10-06 kullanıcı kararı): Windows WinHTTP, macOS `NSURLSession`, Linux `curl` yoksa `wget`; ikisi de yoksa kutu "Install curl with your package manager (`sudo apt install curl`)" der. 5b'nin curl çağrısı kaldırılır. SHA-256 doğrulaması, aşama klasörü ve sürüm klasörü aynen kalır. Gerekçe: ek boyut ~0, şirket proxy'si ve sertifikaları sistemde, TLS güncellemeleri sistemde.
2. **ffmpeg 9.0.2** (spec `ffmpeg-7.1-1` yerine): yalnız 9.x iPhone ızgaralı HEIC'leri doğru çözüyor; 7.1/8.0 tek 512 px karo verip başarıyla çıkıyor (özet §5.2). Etiket `ffmpeg-9.0.2-1`. HEIC/AVIF girdisi için bulunan ffmpeg ≥ 9.0 şart (`ffmpeg -version` ilk satırı); ses/video ve WebP/AVIF çıktısı için PATH'teki daha eski ffmpeg kabul edilir.
3. **HEIC/AVIF ffmpeg ile çözülünce:** döndürmeyi ffmpeg zaten uygular → Gezik EXIF'e göre bir daha döndürmez; ızgaralı resimlerde renk profili (iPhone Display P3) ve saydamlık kaybolur, tek sayılı kırpma 1 px küçülür. Bunlar bilinen sınırlar olarak notlara yazılır; profil kurtarma (HEIC `colr` kutusu) sonraya.
4. **Kayıplı WebP ve AVIF çıktısı ffmpeg ile;** AVIF her zaman `-pix_fmt yuv420p`, saydamsa ayrı alfa akışı (özet §5.3). AVIF kalite eşlemesi `crf = round(63 - q*0.55)`.
5. **JPEG kodlayıcı `jpeg-encoder`** (image'ın kodlayıcısı yerine): %18 küçük, 2 kat hızlı, EXIF uzunluk taşması yok; +68 KB. Optimize Huffman tabloları açık.
6. **Boyutlandırma `fast_image_resize::resize_typed::<U8x4>`** (genel `resize()` +1,9 MB); 16 bit kaynaklar boyutlandırılırken 8 bite iner (PNG çıktısı boyutlandırma yoksa 16 bit kalır).
7. **Konum silme kendi küçük EXIF düzenleyicimizle** (özet §3, `exifclean.rs`): GPS IFD'si ve küçük resim yerinde temizlenir, piksel boyutları düzeltilir; yeniden kodlamadan "Remove location data" yolunda XMP APP1 de atılır. Yeni crate yok.
8. **Metin kodlama listesi encoding_rs etiketleri:** ISO-8859-1 ve ISO-8859-9 encoding_rs'te Windows-1252/1254'tür; listede "Windows-1254 (ISO-8859-9)" gibi gösterilir. UTF-16 çıktısı elle kodlanır; BOM'suz UTF-16 ikili sayılır ve atlanır (not: "looks binary").
9. **Exe bütçesi 5c için +1,3 MB** (spec +1 MB; ölçülen önerilen set +1,19 MB + jpeg-encoder 68 KB). Görev 10 ölçer.
10. **Kullanıcı komutunun yerinde değiştirmesi:** aslının **kopyası** çöpe gider (spec 7.1); sonuç `[Created{asıl yol, değişmiş hâl}, Trashed{özgün kopya}]` → Ctrl+Z değişmişi çöpe atar, kopyayı asıl adına geri getirir.

## Global Constraints

- Rust edition 2024, stable (1.99). Komutlar `D:\Work\gezik`'ten; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- Çapraz denetimler: `cargo check -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- Yeni bağımlılıklar tam olarak: `gezik-batch` → `image = { version = "0.25.10", default-features = false, features = ["png", "jpeg", "gif", "webp", "bmp", "tiff", "ico"] }`, `jpeg-encoder = "0.7.1"`, `fast_image_resize = { version = "6.1.0", default-features = false, features = ["image"] }` (rayon yok), `encoding_rs = "0.8.42"`, `chardetng = "1.0.0"`; `gezik-platform` → Windows'ta `windows` özelliklerine `Win32_Networking_WinHttp`, macOS'ta `objc2-foundation` özelliklerine `NSURLSession`, `NSURLRequest`, `NSURL`, `NSData`, `NSError`, `NSURLResponse`, `NSProgress` (gerekenler) ve `block2` (objc2 blokları; zaten dolaylı varsa sürümü ona uyar). Başka crate yok.
- Arayüz iş parçacığı dosya sistemine dokunmaz; süreçler (ffmpeg, ffprobe, kullanıcı komutları) iş iş parçacığında, kabuk olmadan, Windows'ta pencere açmadan (`ChildProcess`).
- Hiçbir dosya kullanıcı seçmeden ezilmez: çıktılar geçici adla yazılır, bitince yeniden adlandırılır; var olan hedef çakışma listesinden geçer; "aslının yerine" aslını çöpe atar (çöp yoksa seçenek kapalı).
- Kullanıcı komutları: argüman dizisi, yer tutucular argüman içinde değişir, `{in}`/`{out}` mutlak yollar; komut çalışma klasörü girdinin klasörü; stdin kapalı.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Claude imzası yok.
- Yol birleştirme yalnız `Path::join` (Windows Çöp Kutusu karışık ayırıcıyı reddeder).

## Review Focus

1. **Kayıplı metin dönüşümü** (Türkçe karakterin Windows-1252'ye yazılması): dosya değişmemeli, hata satırı karakteri ve satırı söylemeli. → Görev 4 testi.
2. **ffmpeg iptali ortasında** yarım çıktı ve süreç kalmamalı. → Görev 5 testi (sahte ffmpeg).
3. **Kullanıcı komutu adında özel karakterler** (`"`, `;`, `$`, `&`, boşluk, Türkçe harf) güvenle geçmeli; kabuk yorumlamamalı. → Görev 2 ve 6 testleri.
4. **"Aslının yerine" + Ctrl+Z** (resim, metin, yerinde komut): asıl içerik byte byte geri gelmeli. → Görev 6 testleri.
5. **EXIF yönü 6 olan fotoğraf** boyutlandırılıp JPEG'e çevrilince: görüntü dik, EXIF yönü 1, konum silme seçiliyse GPS yok. → Görev 3 testi.

---

### Task 1: Sistem indiricisi — `gezik-platform::http`

**Files:** Create `crates/gezik-platform/src/http/{mod.rs, windows.rs, macos.rs, unix.rs}`; Modify `crates/gezik-platform/{Cargo.toml, src/lib.rs}`, `crates/gezik-batch/src/tasks/download.rs`, spec (karar satırı).

**Interfaces:**
```rust
/// Downloads `url` (https; http only when `allow_http`) into `dest`, calling `progress(bytes_so_far)`
/// at most every 100 ms; stops and returns `Interrupted` when `stop()` turns true; follows at
/// most 5 redirects, all https (unless `allow_http`); fails if the body exceeds `max_bytes`.
pub fn download(url: &str, dest: &Path, max_bytes: u64, allow_http: bool,
                progress: &mut dyn FnMut(u64), stop: &dyn Fn() -> bool) -> io::Result<()>;
pub fn tool_missing_hint() -> Option<String>; // Linux: Some("Install curl …") when neither curl nor wget; else None
```

**Behavior:**
- **Windows:** WinHTTP (`WinHttpOpen` with `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`, `WinHttpConnect`, `WinHttpOpenRequest` with `WINHTTP_FLAG_SECURE` for https, `WinHttpSendRequest`, `WinHttpReceiveResponse`, status 200 check via `WinHttpQueryHeaders(WINHTTP_QUERY_STATUS_CODE)`, `WinHttpReadData` loop into the file). Redirects: WinHTTP follows them by default; set `WINHTTP_OPTION_REDIRECT_POLICY` to `WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP` (always) and limit with `WINHTTP_OPTION_MAX_HTTP_AUTOMATIC_REDIRECTS = 5`. Connect/receive timeouts 30 s / 60 s (`WinHttpSetTimeouts`). Stop: check between reads; close handles.
- **macOS:** `NSURLSession` with `defaultSessionConfiguration` and a download task whose completion handler moves the temp file, or a data task delegate-free loop: simplest correct path is `NSURLSession.dataTaskWithRequest:completionHandler:` is all-in-memory (too big for ffmpeg) — use `downloadTaskWithRequest:completionHandler:` (file on disk), wait on a semaphore/channel, poll `countOfBytesReceived` every 100 ms for progress, `cancel` on stop, then move the file from the given temp URL to `dest` inside the handler (the temp file is deleted when the handler returns). HTTPS-only via the URL check before starting (and ATS defaults). Compile-checked only here (`cargo check --target aarch64-apple-darwin`).
- **Linux (and other Unix):** `curl` (5b's arguments: `--disable --fail --location --proto =https --proto-redir =https --max-redirs 5 --max-filesize <max> --connect-timeout 30 --silent --show-error --output`), else `wget` (`--no-config --https-only --max-redirect=5 --timeout=60 --tries=1 --quiet -O <dest> <url>` and a size check after), else error with `tool_missing_hint`. Progress by polling the output file size every 100 ms (5b's way). Both through `ChildProcess`.
- `DownloadTask` uses `http::download` (temp file from `temp_file_for`; hash check, unpack and install unchanged). The 5b download tests keep passing (the local test server is http: pass `allow_http` from `with_url`'s scheme as before).
- Spec section 8.2: replace the curl sentence with this decision (2026-10-06).

- [ ] Steps: tests first (Windows: download from the in-test HTTP server — reuse 5b's helper — with progress callbacks, cancel mid-body, `max_bytes` exceeded → error and no file, 404 → error); implement per platform; `cargo test -p gezik-platform http` and `-p gezik-batch download`; cross checks; commit "Download tools with the system's own HTTP client".

---

### Task 2: `gezik-core::batch::convert` — saf parçalar

**Files:** Create `crates/gezik-core/src/batch/convert.rs` (+ `mod.rs` satırı).

**Interfaces (all pure, tested):**
```rust
pub enum Kind { Image, Text, Media, Command }
pub enum ImageFormat { Jpeg, Png, WebpLossless, WebpLossy, Avif, Bmp }
pub enum Resize { None, Longest(u32), Width(u32), Height(u32), Percent(u32) }
pub struct ImageOptions { pub format: ImageFormat, pub quality: u8, pub resize: Resize, pub never_enlarge: bool,
                          pub rotate_by_exif: bool, pub strip_metadata: bool, pub background: [u8; 3] }
pub enum Eol { Keep, Lf, Crlf, Cr }
pub struct TextOptions { pub from: Option<String> /* None: detect */, pub to: String, pub bom: bool, pub eol: Eol, pub trim_trailing: bool, pub final_newline: bool }
pub enum MediaPreset { Mp4, Smaller, Mp3, M4a, Wav, Remux, Gif }
pub enum Output { SameFolder, Subfolder, Folder(PathBuf), ReplaceOriginal }
pub struct Preset { pub id: &'static str, pub label: &'static str, pub kind: Kind, … }
pub const PRESETS: &[Preset];   // spec 6.2-6.4 built-ins with their options
pub fn target_size(w: u32, h: u32, resize: &Resize, never_enlarge: bool) -> Option<(u32, u32)>; // özet §1.3
pub fn output_path(input: &Path, ext: &str, output: &Output) -> PathBuf; // spec 6.1 naming incl. "x (converted).jpg", subfolder "converted"
pub fn image_inputs(name: &str) -> bool; pub fn needs_ffmpeg_to_read(name: &str) -> bool; // heic/heif/avif
pub fn media_inputs(name: &str) -> bool;
pub fn avif_crf(quality: u8) -> u8; // round(63 - q*0.55), clamped 0..=63
pub fn ffmpeg_args(preset: MediaPreset, input: &Path, output: &Path) -> Vec<OsString>; // özet §5.4 exactly
pub fn ffmpeg_image_args(…decode heic→png | encode png→webp lossy/avif (alpha aware)…) -> Vec<OsString>; // özet §5.2-5.3
pub struct Progress { pub out_us: Option<u64>, pub end: bool }
pub fn parse_progress_block(lines: &[&str]) -> Progress; // out_time_us; N/A → None
pub fn parse_ffmpeg_version(first_line: &str) -> Option<(u32, u32)>; // "ffmpeg version 9.0.2-essentials…" → (9,0); git builds "2025-05-19-git-…" → None (treated as old for HEIC)
pub struct CommandSpec { pub name: String, pub run: Vec<String>, pub output: Option<String>, pub types: Vec<String>, pub folders: bool, pub parallel: u8 }
pub fn expand_command(spec: &CommandSpec, input: &Path, out: Option<&Path>) -> Result<Vec<OsString>, String>; // {in} {out} {dir} {name} {ext} {outdir}; unknown placeholder → Err
pub fn command_applies(spec: &CommandSpec, name: &str, is_dir: bool) -> bool;
```

- [ ] Steps: write tests from the spec and the digest (each preset's exact args; the "Smaller video" scale filter string; GIF filter; avif crf table q85→16, q50→35; output names in every mode incl. uzantı aynı → "(converted)"; `expand_command` with a name `a "b"; $c & d ş.jpg` gives that exact string in one argument; unknown `{x}` → Err; types case-insensitive), implement, commit "Add the pure parts of conversion: presets, names, sizes, ffmpeg arguments and commands".

---

### Task 3: Resim dönüştürme — `gezik-batch::convert::image` + `exifclean`

**Files:** Create `crates/gezik-batch/src/convert/{mod.rs, image.rs, exifclean.rs}`; Modify `crates/gezik-batch/Cargo.toml`, `THIRD-PARTY.md`.

**Interfaces:**
```rust
pub struct ImageJob<'a> { pub input: &'a Path, pub output: &'a Path /* temp */, pub options: &'a ImageOptions, pub ffmpeg: Option<&'a Path> }
pub fn convert_image(job: &ImageJob, stop: &dyn Fn() -> bool) -> io::Result<()>;
pub enum ImageError { NotSupported(String), NeedsFfmpeg }  // carried in io::Error::other
pub fn strip_location(jpeg_in: &Path, jpeg_out: &Path) -> io::Result<()>; // lossless path (özet §3 + XMP removal)
```

**Behavior** (özet §1-3, `img.rs`, `exifclean.rs` birebir temel):
- Decode with metadata (ICC, EXIF, orientation read **before** `from_decoder`); GIF first frame; 16-bit handled.
- HEIC/HEIF/AVIF inputs: `ffmpeg` decode to a temp PNG (`-compression_level 1`), no EXIF rotation afterwards (sapma 3); if `ffmpeg` is None or < 9.0 → `NeedsFfmpeg`.
- Apply orientation (if `rotate_by_exif`) **before** `target_size`; resize with `resize_typed::<U8x4>` (Lanczos3, premultiplied alpha) when a size is set.
- Output: JPEG via `jpeg-encoder` (quality, optimized Huffman; `flatten` onto `background` first; ICC kept; EXIF kept for JPEG→JPEG with orientation reset to 1, thumbnail dropped, pixel dims set, GPS stripped if `strip_metadata`… note: `strip_metadata` drops all EXIF/XMP/ICC? spec: "meta veriyi kaldırma (EXIF, konum, XMP)" → strip all EXIF and XMP, keep ICC (colour stays right)); PNG (`CompressionType::Default`), lossless WebP, BMP via image; lossy WebP and AVIF via ffmpeg from a temp PNG (sapma 4).
- Non-JPEG outputs carry no EXIF (as spec 6.2).
- Built-in "Remove location data" preset: JPEG input and no other change → lossless `strip_location` (no re-encode); other formats → re-encode with `strip_metadata`.
- `stop()` checked between stages; temp files removed on failure.

- [ ] Steps: tests with the probe samples copied into `crates/gezik-batch/tests/data/images/` (`DSCN0010.jpg`, `landscape_6.jpg` from ianare/exif-samples with SOURCES.md entries; small PNG with alpha; 16-bit PNG made in-test) — orientation 6 → output upright, EXIF orientation 1; GPS removed (kamadak-exif reads no GPS) and file still valid; transparency → JPEG on white; never-enlarge; lossless WebP round trip identical; strip_location keeps pixels identical (decode both, compare); HEIC test only when `GEZIK_TEST_FFMPEG` points to ffmpeg ≥ 9 (use D:\ffmpeg if ≥9, else skip) with libheif `example.heic` downloaded into tests/data (BSD/LGPL? check license; if not redistributable, download in the test into a temp dir and skip offline); commit "Convert pictures: resize, rotate, re-encode and remove location data".

---

### Task 4: Metin dönüştürme — `gezik-batch::convert::text`

**Files:** Create `crates/gezik-batch/src/convert/text.rs`; Modify Cargo.toml.

**Interfaces:**
```rust
pub fn detect(head: &[u8]) -> Detected;            // BOM first, then chardetng; Binary when NUL in first 8 KB
pub enum Detected { Text(&'static encoding_rs::Encoding, bool /*bom*/), Binary }
pub fn encodings() -> Vec<(&'static str /*label*/, &'static str /*name for UI*/)>; // common first; ISO aliases shown (sapma 8); UTF-16LE/BE
pub fn convert_text(input: &Path, output: &Path, options: &TextOptions, stop: &dyn Fn() -> bool) -> io::Result<()>;
pub struct Unmappable { pub ch: char, pub line: u64 }  // in io::Error::other → "can't encode 'ş' in Windows-1252 (line 14)"
```
**Behavior** (özet §4, `text.rs`): streaming (files > 64 MB never fully in memory), malformed input in the source encoding → error naming the line, unmappable → error (file not written), UTF-16 output by hand with BOM option, EOL normalisation, trailing-space trim, final newline. Default output for text presets is "Replace original" (spec 6.3).

- [ ] Steps: tests (Windows-1254 Turkish → UTF-8 exact bytes; UTF-8 with `ş` → Windows-1252 fails with the char and line and leaves no output; UTF-16LE BOM output; CRLF→LF; binary skipped; 100 MB streaming test marked `#[ignore]` or kept small 5 MB); commit "Convert text between encodings and line endings".

---

### Task 5: ffmpeg çalıştırma — bulma, sürüm, ilerleme, iptal

**Files:** Create `crates/gezik-batch/src/convert/ffmpeg.rs`; Modify `crates/gezik-batch/src/tools.rs` (Tool::Ffmpeg lookup), `crates/gezik-core/src/batch/tools.rs` (`Tool::Ffmpeg`), tests `tests/ffmpeg.rs` + a fake ffmpeg test binary (`crates/gezik-batch/src/bin/fake-ffmpeg.rs` behind a `test-tools` feature or `examples/fake_ffmpeg.rs` built by the test).

**Interfaces:**
```rust
pub struct Ffmpeg { pub ffmpeg: PathBuf, pub ffprobe: Option<PathBuf>, pub version: Option<(u32, u32)> }
pub fn find_ffmpeg(data_dir: &Path, configured: Option<&Path>) -> Option<Ffmpeg>; // configured → download → PATH; version via `-version` (cached per path+mtime, like 5b's 7-Zip banner)
pub fn duration_us(ff: &Ffmpeg, input: &Path) -> Option<u64>; // ffprobe; N/A → None
pub fn run_preset(ff: &Ffmpeg, args: Vec<OsString>, duration: Option<u64>, on_progress: &mut dyn FnMut(f64), stop: &dyn Fn() -> bool) -> io::Result<()>; // `-progress pipe:1 -nostats`, stderr last 20 lines in the error
```
**Behavior:** through `ChildProcess` (stdin closed — `-nostdin`), progress = out_us / duration clamped; no duration → None progress (file counter only); cancel kills the tree and the caller deletes the temp output; exit ≠ 0 → error with stderr tail.
- The fake ffmpeg (a tiny Rust binary built for tests) prints progress blocks with sleeps, can exit with a code, can hang until killed, and writes the output path given last.

- [ ] Steps: tests with the fake binary (progress callbacks monotonic and reach 1.0; cancel kills within 1 s and returns Interrupted; failure exit gives the stderr tail; version parsing on real banners); real ffmpeg test guarded by `GEZIK_TEST_FFMPEG` (MP3 from a generated 1 s sine via `-f lavfi`); commit "Run ffmpeg with progress, version checks and cancel".

---

### Task 6: Motor görevleri — `ConvertTask` ve `CommandTask`

**Files:** Create `crates/gezik-batch/src/tasks/{convert.rs, command.rs}`; Modify `crates/gezik-batch/src/tasks/mod.rs`, `crates/gezik-ops` (TaskKind::{Convert, Command} if not present — 5b added `Convert`? check; add `Command`).

**Interfaces:**
```rust
pub enum ConvertWhat { Image(ImageOptions), Text(TextOptions), Media(MediaPreset) }
pub struct ConvertTask; // ConvertTask::new(inputs: Vec<PathBuf>, what: ConvertWhat, output: Output, tools: ConvertTools{ffmpeg: Option<Ffmpeg>})
pub struct CommandTask; // CommandTask::new(inputs: Vec<(PathBuf, bool)>, spec: CommandSpec)
pub fn skipped_inputs(inputs: &[(PathBuf, bool)], what: &ConvertWhat) -> Vec<PathBuf>; // "3 items skipped: not images"
pub fn is_ffmpeg_needed(err_text: &str) -> bool;
```
**Behavior:**
- Plan: one item per input (`checked()` target = final output path; "Replace original" targets the original → no conflict; uzantı değişiyorsa yeni ad hedef, eskisi çöpe), conflicts through the list.
- Run: write to `temp_file_for(target)`, then (Replace original) trash the original (`trash_path`) and move the temp to the target name → `Several([Trashed, Created])`; else move → `Created`. Subfolder created on demand (its creation is a `Created` too so undo removes it when empty… simpler: create the folder in plan as a Before item with `Created` outcome).
- `Resources`: images `Work::Cpu` (cores), text `Work::Disk`, media `Work::External` with 1 worker (ffmpeg uses all cores).
- Media progress: `found(1, file_size)` per item; ffmpeg fraction × file size → `add_bytes` deltas.
- Errors per item → failure rows; `NeedsFfmpeg` → failure text recognised by `is_ffmpeg_needed` ("ffmpeg needed").
- `CommandTask`: per input `expand_command`; with `output`: run with `{out}` = temp path, success requires exit 0 and the temp file exists → move into the final name (`output` template resolved with {name}/{ext}) → Created; without `output` (in-place): first copy the original to a temp file and trash that copy (`trash_path`) → then run → `Several([Created{original after}, Trashed{the copy}])`; on failure delete nothing extra and report stderr tail; drive without trash → refuse ("needs a recycle bin to undo"). `parallel` workers (1-16), `Work::External`. Working dir = input's folder; stdin closed; no window. Program lookup: absolute or PATH (absolute entries only); missing → failure "magick not found".

- [ ] Steps: engine tests (image JPEG→PNG same folder + undo; replace original + undo restores bytes; text replace original + undo; subfolder mode; conflict when output exists; skipped list; command with output on a fake tool (the fake-ffmpeg binary doubles as a generic tool or a tiny `fake-tool` example: copies {in} to {out}); in-place command + undo restores bytes; special-character file name passes through intact; nonzero exit → failure with stderr); commit "Convert files and run user commands as engine jobs with one undo".

---

### Task 7: Ayarlar — `[convert]`, `[[commands]]`, durum

**Files:** Modify `crates/gezik-config/src/{settings.rs, settings_edit.rs?}`, `templates/settings.toml`.

**Behavior:** `Settings::convert: ConvertSettings { ffmpeg: Option<String> /* absolute, else warning */ }`; `Settings::commands: Vec<CommandSpec>` parsed from `[[commands]]` (name non-empty, `run` non-empty array of strings, `output` optional string, `types` optional array, `folders` bool, `parallel` 1-16; invalid entry → warning `commands[N]: …`, skipped); `State::convert: ConvertState { last_preset: Option<String>, image/text/media last options as strings, last_output: String, last_folder: Option<String> }`. Template: spec 10.1 `[convert]` + spec 7.1/7.3 comment examples (ImageMagick, LibreOffice with `{outdir}`, Calibre). Live reload is the existing watcher (no work).

- [ ] Steps: parse tests (good, each invalid field, relative ffmpeg path warning, template parses with no warnings), state round trip; commit "Read conversion settings and user commands".

---

### Task 8: ffmpeg araç paketleri ve yayın

**Files:** Create `scripts/tools/prepare-ffmpeg.ps1` (or extend `prepare.ps1` with `-Tool ffmpeg`); Modify `scripts/tools/{README.md, sources.sha256}`, `crates/gezik-core/src/batch/tools.rs` (MANIFEST entries for `Tool::Ffmpeg`), `THIRD-PARTY.md`.

**Behavior** (özet §6): sources — Windows x64 gyan 9.0.2 essentials `.7z` (verify the published sha256), Windows arm64 BtbN month-end winarm64 gpl 9.0.x (asset digest), macOS arm64/x64 Martin Riedl 9.0.2 ffmpeg.zip + ffprobe.zip (`.sha256`), Linux x64/arm64 BtbN linux64/linuxarm64 gpl 9.0.x (asset digest). Pin every upstream file in `sources.sha256`. Repack per platform into **one solid 7z** (`7z a -t7z -mx=9 -ms=on`) holding `ffmpeg`, `ffprobe`, `LICENSE` + `SOURCE.txt` (FFmpeg commit/tag and builder script repo URL); file names `ffmpeg-9.0.2-<platform>.7z`; `kind = "7z"` (5b's reader handles 7z; make sure `DownloadTask` accepts it). Keep macOS signatures (no re-sign). Size + SHA-256 into MANIFEST (`programs = ["ffmpeg", "ffprobe"]` / `.exe`). Copy packages to `D:\Work\gezik-tools\ffmpeg-9.0.2-1\`. **Publish** with gh (installed at `C:\Program Files\GitHub CLI\gh.exe`, logged in as an admin of wenlar): `gh release create ffmpeg-9.0.2-1 --repo wenlar/gezik-tools …` with notes (GPL v3, FFmpeg source link, builders' links), then download two assets back and compare hashes.

- [ ] Steps: script, run, MANIFEST, completeness test for ffmpeg (all six platforms), install test against the local folder (`GEZIK_TOOLS_DIR`), publish, verify; commit "Prepare ffmpeg 9.0.2 downloads for every platform and pin their SHA-256".

---

### Task 9: Arayüz — Convert katmanı, Commands menüsü, ffmpeg kutusu

**Files:** Create `crates/gezik/src/convert.rs`, `crates/gezik/ui/widgets/convert.slint`; Modify `context_menu.rs`, `main.rs`, `app.slint`, `operations.rs`, `archives.rs` (share the tool offer code: generalise 5b's 7-Zip box into a tool box for 7-Zip and ffmpeg).

**Behavior** (spec 6.1, 6.2-6.4, 7.2):
- Menu: **Convert…** (any selection with at least one convertible file) and **Commands ▸** (submenu of `[[commands]]` that apply to the selection; missing program greyed with "<prog> not found" tip — checked on a thread when the menu data is prepared, cached).
- Convert layer: preset dropdown grouped (Image, Text, Audio/Video, Commands) filtered by the selection's types, most common type first; per-kind options (Image: format buttons incl. "WebP (lossy) ¹" / "AVIF ¹", quality, resize mode + value, never enlarge, rotate by EXIF, remove metadata, background colour; Text: from (Detected: …), to, BOM, line endings, trim, final newline; Media: preset only); Output: same folder / subfolder "converted" / choose… (path question like 5b's Extract to…) / replace originals (disabled without a recycle bin — checked on a thread); footer "N items skipped: not images" and "¹ needs ffmpeg"; Convert / Cancel; Esc / Ctrl+Enter via capture-stage routing (5a/5b pattern). Last choices in `State::convert`.
- ffmpeg box: when the chosen preset needs ffmpeg and none (or < 9 for HEIC/AVIF) is found, or a job fails with `is_ffmpeg_needed` → "Video conversion needs ffmpeg (~30 MB, free)." Download / Where does it come from? / Cancel; `[tools] download = false` → info only; Linux adds the package hint (`sudo apt install ffmpeg` — only if ≥ 9 will satisfy HEIC; say "ffmpeg 9 or newer"); after a successful download the conversion is resubmitted (5b's pattern).
- Curl-missing (Linux): the download failure text from Task 1 shows in the box.
- Panel titles from tasks ("Converting 12 items to WebP", "Running Resize to 50% on 3 items").

- [ ] Steps: pure tests (preset filtering by selection, skipped counts, footer texts, menu ids non-overlapping), implement, run the app briefly, commit "Convert files and run user commands from the menus, and offer to download ffmpeg".

---

### Task 10: Ölçüm, Linux kabı, notlar

- [ ] `crates/gezik-batch/examples/convert_bench.rs`: 100 generated 12 MP photos → 1920 px JPEG (all cores) time; text 100 MB conversion time. `scripts/perf/convert.ps1` runs it (try/finally).
- [ ] Exe size vs 5b head (budget +1,3 MB, sapma 9).
- [ ] Linux Docker: `cargo test -p gezik-batch` (fake ffmpeg tests run there; real ffmpeg tests skip), add `ffmpeg` to nothing (image has none), report.
- [ ] Notes (Turkish, 5b section style): measurements, deviations (system downloader, ffmpeg 9 pin and HEIC limits, jpeg-encoder, fir U8x4, exifclean, encoding labels), not tried (macOS GUI, Linux GUI of the convert layer), ffmpeg release URL. Spec Durum "5a, 5b, 5c uygulandı". Commit "Measure conversion and note what 5c does".

---

## Ekran testleri (alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici; test verisi `%TEMP%\gezik-gui-5c\`: 5 JPEG (biri EXIF yönü 6 ve GPS'li — `tests/data/images`'tan), 1 PNG saydam, 1 HEIC (libheif örneği), 1 Windows-1254 Türkçe metin, 1 UTF-8 metin içinde `ş`, 1 kısa video (ffmpeg lavfi ile üretilmiş 3 sn mp4), `[[commands]]` içinde `cmd /c copy` tabanlı bir test komutu (çıktılı) ve yerinde değiştiren bir komut (`powershell -NoProfile -Command "Add-Content …"`).

1. 5 JPEG + PNG seç → Convert… → "Resize photos (JPEG 1920 px)" → dosyalar oluşur, dik görünür, GPS yok; Ctrl+Z çıktıları çöpe atar.
2. PNG → JPEG (saydamlık beyaz arka plan).
3. WebP (lossless) ve WebP (lossy) — ffmpeg yokken lossy seçince ffmpeg kutusu; Download → indirme paneli → dönüştürme sürer.
4. HEIC → JPEG (ffmpeg ile).
5. Metin: Windows-1254 → UTF-8, aslının yerine; Ctrl+Z eski baytlar geri.
6. UTF-8 `ş` → Windows-1252: hata satırı karakteri ve satırı söyler, dosya değişmez.
7. Video → MP3 ve Smaller video; panel ilerlemesi akıcı; iptal → yarım dosya yok.
8. Commands ▸ çıktılı komut ve yerinde komut; Ctrl+Z yerinde değişikliği geri alır.
9. Output modları: alt klasör "converted", "Choose…", aynı uzantıda "(converted)" adı; var olan çıktı → çakışma listesi.
10. Esc/Ctrl+Enter, son seçimler kalıcı.
