# Toplu İşlemler 5d (PDF) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Seçili resimlerden tek PDF yapmak (içeride, indirme gerektirmeden); PDF'leri birleştirmek, bölmek, sayfa ayıklamak ve sayfaları PNG/JPEG'e çevirmek (indirilen pdfium ile, ayrı bir yardımcı süreçte); pdfium'u tek tıkla indirmek. Hepsi 4a'nın kuyruğunda, çakışma listesi ve tek Ctrl+Z ile.

**Architecture:** Saf kararlar (aralık sözdizimi, bölme parçaları, çıktı adları, sayfa yerleşimi, EXIF yön matrisi, işleme boyutu sınırı) `gezik-core::batch::pdf`'te; yardımcı süreçle konuşulan satır protokolü `gezik-core::batch::pdf_worker`'da, ikisi de saf ve tam sınanır. Resim→PDF `gezik-batch::pdf::images`'ta süreç içinde (`pdf-writer` + `miniz_oxide`, nesne nesne dosyaya akarak). pdfium işleri aynı `gezik` exe'sinin gizli `--pdf-worker` kipinde çalışır: üst süreç (`gezik-batch::pdf::client`) isteği stdin'den bir kez yollar, ilerlemeyi ve sonucu stdout satırlarından okur; işçi (`gezik-batch::pdf::worker`) pdfium'u yükler, işi yapar, çıkar. Motor tarafında `PdfTask` işçiyi her girdi için bir aşama klasörüne çalıştırır, 5b'nin `PlaceTask`'ı çıktıları çakışma listesiyle yerine taşır (tek iş, tek Ctrl+Z); `ImagesToPdfTask` tek çıktıyı geçici adla yazar. Arayüz: Convert katmanında "PDF" grubu, sağ tıkta "Images to PDF…", araç kutusunda pdfium.

**Tech Stack:** Rust 2024, Slint 1.18; `pdf-writer` 0.15.0, `miniz_oxide` 0.9.1, `pdfium-render` 0.9.4 (`pdfium_7881`, dinamik, `thread_safe` kapalı), `image` 0.25.10 ve `jpeg-encoder` 0.7.1 (5c'den); pdfium bblanchon/pdfium-binaries `chromium/8086` (PDFium 157.0.8086.0, indirilen).

**Spec:** `docs/superpowers/specs/2026-10-05-toplu-islemler-design.md` (bölüm 3, 8, 9, 10-12)
**Kütüphane özeti (doğrulanmış kod, sürümler, özetler):** `docs/superpowers/notes/2026-10-07-pdf-kutuphaneleri.md` ve `docs/superpowers/notes/pdf-probe/{jpeg.rs, img2pdf.rs, pdfops.rs, *.out.txt}`. Bu planda "özet §N" o belgenin bölümüdür; deneme kodu birebir temel alınır.

## Spec'ten sapmalar ve netleştirmeler

1. **pdfium `chromium/8086`, etiket `pdfium-8086-1`** (spec `pdfium-6996-1`): 6996, `pdfium_7881` bağlamasıyla yüklenmiyor (`GetProcAddress 127`, özet §3.4) ve pdfium-render README'sine göre macOS'ta bilinen hatası var. ffmpeg gibi yeniden paketlenir: platform başına tek solid 7z, içinde kitaplık + `LICENSE` + `licenses/` + `SOURCE.txt`; her üst kaynak dosyanın SHA-256'sı GitHub asset `digest`'inden `sources.sha256`'ya sabitlenir; betik `scripts/tools/prepare-pdfium.ps1`; yayın 5c Task 8'deki gibi `gh` ile.
2. **Bağlama `pdfium-render 0.9.4`, `default-features = false, features = ["pdfium_7881"]`**, `thread_safe` kapalı (yardımcı süreçte tek iş parçacığı zaten var; açmak yalnız muteks ekler, özet §2.7). `libloading` tek kopyaya `cargo update -p libloading@0.9.0 --precise 0.8.9` ile indirilir (özette denendi, 0.9.4 0.8.9 ile derleniyor). Çözülmezse ikinci kopya kalır ve Task 9'da boyut notu olarak yazılır.
3. **pdfium yardımcı süreçte** (spec "iş başında yükler, iş bitince bırakır"ı süreç içinde değil süreçle sağlar): aynı `gezik` exe'si gizli `--pdf-worker` argümanıyla, `ChildProcess` üzerinden, pencere açmadan çalışır. Üst süreç tek bir istek yollar (stdin, sonra kapanır), işçi ilerlemeyi ve sonucu satır satır yazar (protokol `gezik-core`'da saf tipler, testli), pdfium'u yükler, işi yapar, çıkar. Gerekçe: pdfium-render bağlamaları süreç genelinde `OnceCell`'de, `drop` boşaltmıyor, yeniden bağlama `PdfiumLibraryBindingsAlreadyInitialized` veriyor (özet §2.6); süreç bitince kitaplık gerçekten boşalır (boşta bellek değişmez), kötü niyetli PDF'in çökmesi Gezik'i düşürmez (özet §4.1), iptal anında olur (süreç ağacı öldürülür), bellek tasarım gereği sınırlıdır (sayfa başına 64 MP, iş bitince süreç biter). Şifre sorusu motorun var olan soru/yanıt kanalıyla üst süreçte sorulur: işçi "needs-password" bildirip çıkar, üst süreç sorar ve işçiyi parolayla **stdin'den** yeniden başlatır; parola hiçbir zaman komut satırına yazılmaz.
4. **Resim→PDF süreç içinde: `pdf-writer 0.15.0` + `miniz_oxide 0.9.1`**, her dolaylı nesne kendi `Chunk`'ında doğrudan geçici dosyaya, xref tablosu bizim (100 × 12 MP'de tepe 17 MB, özet §1.1). JPEG'ler DCTDecode ile aynen gömülür; Adobe APP14'lü CMYK'ya `Decode [1 0 1 0 1 0 1 0]`, ICC profili `ICCBased`. EXIF yönü içerik akışında dönüşüm matrisiyle (yeniden kodlama yok, `/Rotate` yok; sekiz yön doğrulandı). 12 bitlik, aritmetik kodlu ve kayıpsız JPEG'ler çözülüp Flate yoluna gider. PNG ve diğerleri Flate + `Predictor 15` (Paeth), düzey 1 (özet §1.4: düzey 1 ile 9 arasında %1,6 fark, süre 5 kat); alfa `SMask`; 16 bit korunur (big-endian). Sayfa: resim boyutu (JFIF dpi, yoksa 72), A4 ya da Letter; yatay resim yatay sayfaya; kenar boşluğu; 14.400 pt'yi aşan resim küçültülür. Sayfa sırası önizleme sırasıdır, kullanıcı sürükleyerek değiştirir.
5. **Sınırlar:** PDF→resimde sayfa başına en çok 64 MP ve kenar başına 65.535 px; aşan sayfanın DPI'ı o sayfa için düşürülür ve işin raporuna not (atlama satırı değil, `skipped` notu) olarak yazılır. Dev belgede "Each page" bölmesi, katmanda çalıştırmadan önce kaç dosya çıkacağını gösterir.
6. **Katman bilinen kayıpları söyler:** birleştirme, bölme ve ayıklama yer imlerini, formları ve belge bilgisini düşürür; şifreli kaynaktan şifresiz çıktı çıkar (özet §2.2, §2.5).
7. **Arayüz:** Convert katmanının hazır ayar listesinde "PDF" grubu; "Images to PDF…" ayrıca sağ tık menüsünde (katmanı o seçimle açar). Araç kutusu pdfium'u 7-Zip/ffmpeg gibi sunar: "PDF tools need pdfium (~3 MB, free)." (boyut MANIFEST'ten). Çıktı adları `ad (merged).pdf`, `ad - page 3.pdf`, `ad - pages 1-3.pdf`. Ctrl+Z çıktıları çöpe atar. Aralık ayrıştırma (`1-3, 5, 8-`) `gezik-core`'da saf, testli.
8. **Testler:** pdfium testleri yalnız `GEZIK_TEST_PDFIUM` bir kitaplık dosyasını gösterdiğinde çalışır, yoksa atlanır. Protokol ve aralık ayrıştırma saf ve tam sınanır. Motor uçtan uca 5c'nin sahte ffmpeg'i gibi derlenen sahte bir işçiyle (`examples/fake_pdf_worker.rs`) sınanır.
9. **Son görev:** ölçüm, Linux kabı testleri, notlar, spec Durum "5a, 5b, 5c, 5d uygulandı"; ardından Windows ekran testi listesi (5c'deki gibi, planın sonunda).
10. **Dal:** `feat/batch-ops-5d`, master `22f245b`'den.

Plan kararları (spec ve yukarıdaki kararlar bunları söylemiyor; gerekçeleriyle):

11. **Çıktılar girdinin klasörüne gider;** PDF seçeneklerinde Convert'in Output bölümü gizlenir. Gerekçe: spec 9.1 yalnız adları veriyor; "aslının yerine" PDF'te anlamsız; alt klasör ve "Choose…" sonraya.
12. **Bölme, ayıklama ve PDF→resim çıktıları aşama klasörüne yazılır, 5b'nin `PlaceTask`'ı yerine taşır** (Extract zinciri gibi). Gerekçe: çıktı sayısı sayfa sayısına bağlı ve iş başlamadan bilinmez; böylece çakışma listesi, yarım çıktının hiç görünmemesi, çökmede temizlik ve geri alma 5b'deki gibi hazır gelir. Her girdinin çıktıları kendi alt klasöründe birikir; işçi başarıyla bitmeden hiçbiri yerleştirilmez.
13. **Birleştirme adı ilk PDF'in adıyla:** `rapor (merged).pdf`. Resim→PDF adı: tek resimde `foto.pdf`, birden çokta bulunulan klasörün adı (`Tatil.pdf`; kök klasörde `Pictures.pdf`), Compress'in varsayılan adı gibi.
14. **Bölmede "Ranges" her virgüllü parçayı ayrı dosya yapar** (`1-3, 5, 8-` → `- pages 1-3`, `- page 5`, `- pages 8-14`); **ayıklama** bütün aralıkları tek dosyaya koyar, adı `ad - pages 1-3, 5, 8-14.pdf`; etiket 40 karakteri aşarsa `ad - 23 pages.pdf`.
15. **Birleştirmede bir girdinin parolası verilmezse birleştirme yapılmaz** (atlama notu: "no password given; nothing was merged"). Bölme/ayıklama/PDF→resimde yalnız o girdi atlanır.
16. **HEIC/HEIF/AVIF Images to PDF'e girmez** (atlama: "not a picture Gezik reads itself"); ffmpeg yolu sonraya.
17. **PDF→resim JPEG kalitesi sabit 90** (özetteki ölçümlerin ayarı); PNG ve JPEG çıktısı RGB (pdfium beyaz zeminle opak işler; alfa gereksiz).
18. **Kenar boşluğu üç seçenek:** None 0 pt, Small 18 pt (¼ in), Large 36 pt (½ in); varsayılan None. **Son seçimler** `state.toml [convert] pdf` satırında (işlem, bölme kipi ve N, DPI, resim biçimi, sayfa boyutu, kenar boşluğu); aralık metni belgeye özgü olduğu için saklanmaz.
19. **İşletim sistemi düzeyinde bellek sınırı (Job Object / `setrlimit`) eklenmez;** sınır 64 MP kuralı ve sürecin bitmesiyle gelir. `ChildProcess`'e yeni API gerekmez.
20. **Exe bütçesi:** spec 5d ≤ +1 MB. Kütüphane payı özetteki ölçümle ~0,73 MB (img2pdf +0,30, pdfium-render +0,42); katman ve denetleyici payı 5c'de ayrıca ~1,5 MB tutmuştu. Task 9 ölçer ve 5c'deki gibi dağılımı yazar.

## Global Constraints

- Rust edition 2024, stable (1.99). Komutlar `D:\Work\gezik`'ten; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Dal `feat/batch-ops-5d`, master `22f245b`'den (`git switch -c feat/batch-ops-5d 22f245b`; aynı depoda başka bir çalışma sürüyorsa ayrı bir çalışma ağacında).
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- Çapraz denetimler: `cargo check -p gezik --target aarch64-apple-darwin --no-default-features`, `cargo check -p gezik-core -p gezik-platform -p gezik-ops --target x86_64-unknown-linux-gnu`, `cargo check -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu`.
- Yeni bağımlılıklar tam olarak, yalnız `gezik-batch`'te: `pdf-writer = "0.15.0"`, `miniz_oxide = "0.9.1"`, `pdfium-render = { version = "0.9.4", default-features = false, features = ["pdfium_7881"] }`. Sonra `cargo update -p libloading@0.9.0 --precise 0.8.9`. Başka crate yok; `gezik-core` bağımlılıksız kalır.
- pdfium yalnız yardımcı süreçte yüklenir; `gezik` arayüz süreci pdfium'u hiçbir zaman bağlamaz. İşçi argümanı tam olarak `--pdf-worker`; `main()`'in ilk işi bu argümana bakmaktır (Slint, ayar, pencere yok).
- Parola yalnız işçinin stdin'ine yazılır; komut satırına, günlüğe, `Debug` çıktısına, `state.toml`'a hiçbir zaman.
- Arayüz iş parçacığı dosya sistemine dokunmaz; pdfium'u bulma, sayfa sayma ve çöp denetimi iş parçacığında.
- Hiçbir dosya kullanıcı seçmeden ezilmez: çıktılar geçici adla ya da aşama klasöründe yazılır; var olan hedef çakışma listesinden geçer.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Claude imzası yok.
- Yol birleştirme yalnız `Path::join` (Windows Çöp Kutusu karışık ayırıcıyı reddeder).

## Review Focus

1. **Şifreli PDF, önce yanlış sonra doğru parola:** ikinci soru "Wrong password" der, iş doğru parolayla biter, parola işçinin komut satırında hiç görünmez, çıktı şifresiz açılır. → Task 6 testi (sahte işçi argümanlarını dosyaya yazar) ve Task 5 testi (gerçek pdfium, şifreli örnek).
2. **Çöken ya da takılan işçi (kötü niyetli PDF):** Gezik ayakta kalır, satır "PDF engine stopped (…)" der, öteki girdilerin çıktıları yerleşir; iptal işçiyi 1 sn içinde öldürür, aşama klasörü ve süreç kalmaz. → Task 6 testleri.
3. **300 DPI'da dev sayfa (14.400 pt):** işleme 64 MP'yi aşmaz, DPI o sayfa için düşer, raporda not çıkar, bellek patlamaz. → Task 1 (saf) ve Task 5 (gerçek pdfium) testleri.
4. **Türkçe ve tuhaf adlar** (`rapor ş "a"; $.pdf`, Linux'ta UTF-8 olmayan ad): protokolden kayıpsız geçer, pdfium açar, çıktı adları doğru. → Task 2 (gidiş-dönüş) ve Task 5 (gerçek pdfium, Türkçe ad) testleri.
5. **"Each page" çıktıları zaten varken:** yerleştirmede çakışma listesi açılır, Skip eskiyi korur, Ctrl+Z yalnız yenileri çöpe atar. → Task 6 testi.

---

### Task 1: `gezik-core::batch::pdf` — saf kararlar

**Files:**
- Create: `crates/gezik-core/src/batch/pdf.rs`
- Modify: `crates/gezik-core/src/batch/mod.rs` (`pub mod pdf;`), `crates/gezik-core/src/batch/convert.rs` (`Kind::Pdf` ve onu kullanan `match`'ler; `Preset::default_output` zaten `_ =>` ile)

**Interfaces:**
- Consumes: `gezik_core::batch::convert::{image_inputs, needs_ffmpeg_to_read}`.
- Produces (hepsi saf):

```rust
/// The PDF group of the Convert layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfOp { ImagesToPdf, Merge, Split, ToImages, Extract }
impl PdfOp {
    pub const ALL: [PdfOp; 5];
    pub fn id(self) -> &'static str;        // "images-to-pdf" "merge-pdfs" "split-pdf" "pdf-to-images" "extract-pages"
    pub fn from_id(id: &str) -> Option<PdfOp>;
    pub fn label(self) -> &'static str;     // "Images to PDF" "Merge PDFs" "Split PDF" "PDF to images" "Extract pages"
    pub fn needs_pdfium(self) -> bool;      // all but ImagesToPdf
    pub fn loses_extras(self) -> bool;      // Merge, Split, Extract
}
pub fn is_pdf(name: &str) -> bool;                 // extension pdf, any case
pub fn is_pdf_picture(name: &str) -> bool;         // image_inputs && !needs_ffmpeg_to_read
pub fn ops_for(pictures: usize, pdfs: usize) -> Vec<PdfOp>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange { pub first: u32, pub last: Option<u32> } // 1-based; None: to the end
pub fn parse_ranges(text: &str) -> Result<Vec<PageRange>, String>;
pub fn resolve(ranges: &[PageRange], pages: u32) -> Result<Vec<Vec<u32>>, String>; // 0-based, one Vec per range

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Split { EachPage, Every(u32), Ranges(String) }
pub fn split_parts(split: &Split, pages: u32) -> Result<Vec<Vec<u32>>, String>;
pub fn extract_pages(ranges: &str, pages: u32) -> Result<Vec<u32>, String>;

pub fn pages_label(pages: &[u32]) -> String;       // 0-based in: "page 3", "pages 1-3", "pages 1-3, 5, 8-14"
pub fn merged_name(first: &Path) -> OsString;      // "a (merged).pdf"
pub fn part_name(input: &Path, pages: &[u32]) -> OsString; // "a - page 3.pdf"; label > 40 chars → "a - 23 pages.pdf"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageImage { Png, Jpeg }
pub const JPEG_QUALITY: u8 = 90;
pub fn page_image_name(input: &Path, page: u32, image: PageImage) -> OsString; // 0-based page: "a - page 3.png" / ".jpg"
pub fn pictures_pdf_name(pictures: &[PathBuf]) -> Option<PathBuf>;

pub const RENDER_DPIS: [u32; 3] = [72, 150, 300];
pub const MAX_RENDER_PIXELS: u64 = 64_000_000;
pub const MAX_RENDER_SIDE: u32 = 65_535;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSize { pub dpi: f32, pub width: u32, pub height: u32, pub lowered: bool }
pub fn render_size(width_pt: f32, height_pt: f32, dpi: u32) -> RenderSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize { Picture, A4, Letter }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margin { None, Small, Large }
impl Margin { pub fn points(self) -> f32 } // 0, 18, 36
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageOptions { pub size: PageSize, pub margin: Margin }
impl PageOptions { pub const DEFAULT: PageOptions = PageOptions { size: PageSize::Picture, margin: Margin::None }; }
pub const MAX_PAGE_POINTS: f32 = 14_400.0;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement { pub page_width: f32, pub page_height: f32, pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
pub fn place(shown_w: u32, shown_h: u32, dpi: Option<(f32, f32)>, options: &PageOptions) -> Placement;
pub fn shown_size(w: u32, h: u32, exif_orientation: u8) -> (u32, u32); // 5-8 swap
pub fn orient_matrix(exif_orientation: u8) -> [f32; 6];  // özet §1.3; unknown → identity
pub fn split_file_count(split: &Split, pages: u32) -> Result<usize, String>;
```

**Behavior:**
- Aralık sözdizimi: virgülle ayrılmış parçalar; boşluk serbest; `N`, `A-B`, `A-` (sona kadar), `-B` (baştan). Boş parçalar (`1,,3`) atlanır. Hata metinleri (arayüzde aynen gösterilir): hiç parça yok → `Type the pages, e.g. 1-3, 5, 8-`; sayı değil → `"x" is not a page or a range`; 0 → `Pages start at 1`; ters → `5-3 runs backwards`; `resolve`'da sona taşan → `Page 99 is past the end (14 pages)`. Tekrar eden sayfaya izin var (ayıklamada iki kez kopyalanır).
- `Every(0)` → `Every 0 pages: type 1 or more`. `pages == 0` olan belgede bölme → `This PDF has no pages`.
- `pages_label`: ardışık koşular `A-B`, tek sayfalar `N`, tek sayfalık listede `page N`, değilse `pages …`; sayılar 1 tabanlı.
- `render_size`: `w = ceil(w_pt·dpi/72)`, `h` aynı; `w·h > MAX_RENDER_PIXELS` ya da `max(w,h) > MAX_RENDER_SIDE` ise `s = min(√(MAX/(w·h)), MAX_SIDE/max(w,h)) · 0.999`, `dpi' = dpi·s`, boyutlar `dpi'` ile yeniden; `lowered = true`. Boyutlar en az 1.
- `place`: özet §1.5 ve `img2pdf.rs`'teki `place` birebir (`Picture`: `px·72/dpi`, 14.400 − 2·kenar'ı aşan küçültülür, sayfa = resim + 2·kenar; A4 595.276×841.89, Letter 612×792, yatay resimde sayfa yatay, sığdır ve ortala). `dpi` yoksa 72.
- `pictures_pdf_name`: boş → `None`; tek resim → aynı klasörde `stem.pdf`; birden çok → ilk resmin klasöründe `<klasör adı>.pdf`, klasör adı yoksa (kök) `Pictures.pdf`.
- `ops_for(pictures, pdfs)`: `pictures ≥ 1` → `ImagesToPdf`; `pdfs ≥ 2` → `Merge`; `pdfs ≥ 1` → `Split`, `ToImages`, `Extract`; sıra `PdfOp::ALL` sırası.

- [ ] **Step 1: Write the failing tests** (`pdf.rs` içinde `#[cfg(test)] mod tests`)

```rust
use super::*;
use std::path::{Path, PathBuf};

fn r(first: u32, last: Option<u32>) -> PageRange { PageRange { first, last } }

#[test]
fn ranges_parse_in_gezik_syntax() {
    assert_eq!(parse_ranges("1-3, 5, 8-").unwrap(), vec![r(1, Some(3)), r(5, Some(5)), r(8, None)]);
    assert_eq!(parse_ranges(" -2 ,, 4 ").unwrap(), vec![r(1, Some(2)), r(4, Some(4))]);
    assert_eq!(parse_ranges("-").unwrap(), vec![r(1, None)]);
    assert_eq!(parse_ranges("").unwrap_err(), "Type the pages, e.g. 1-3, 5, 8-");
    assert_eq!(parse_ranges(" , ").unwrap_err(), "Type the pages, e.g. 1-3, 5, 8-");
    assert_eq!(parse_ranges("x").unwrap_err(), "\"x\" is not a page or a range");
    assert_eq!(parse_ranges("1-2-3").unwrap_err(), "\"1-2-3\" is not a page or a range");
    assert_eq!(parse_ranges("0-2").unwrap_err(), "Pages start at 1");
    assert_eq!(parse_ranges("5-3").unwrap_err(), "5-3 runs backwards");
}

#[test]
fn ranges_resolve_against_the_page_count() {
    // The probe's case (özet §2.3).
    let parts = resolve(&parse_ranges("1-3, 5, 8-").unwrap(), 14).unwrap();
    assert_eq!(parts, vec![vec![0, 1, 2], vec![4], (7..14).collect::<Vec<u32>>()]);
    assert_eq!(resolve(&parse_ranges("1-99").unwrap(), 14).unwrap_err(), "Page 99 is past the end (14 pages)");
    assert_eq!(resolve(&parse_ranges("15-").unwrap(), 14).unwrap_err(), "Page 15 is past the end (14 pages)");
    assert_eq!(extract_pages("3, 3, 1", 5).unwrap(), vec![2, 2, 0]);
}

#[test]
fn splits_make_their_parts() {
    assert_eq!(split_parts(&Split::EachPage, 3).unwrap(), vec![vec![0], vec![1], vec![2]]);
    assert_eq!(split_parts(&Split::Every(4), 10).unwrap(), vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7], vec![8, 9]]);
    assert_eq!(split_parts(&Split::Every(0), 10).unwrap_err(), "Every 0 pages: type 1 or more");
    assert_eq!(split_parts(&Split::Ranges("2-, 1".into()), 3).unwrap(), vec![vec![1, 2], vec![0]]);
    assert_eq!(split_parts(&Split::EachPage, 0).unwrap_err(), "This PDF has no pages");
    assert_eq!(split_file_count(&Split::EachPage, 20_000).unwrap(), 20_000);
    assert_eq!(split_file_count(&Split::Every(3), 10).unwrap(), 4);
}

#[test]
fn names_follow_the_spec() {
    let a = Path::new("d").join("rapor ş.pdf");
    assert_eq!(merged_name(&a), "rapor ş (merged).pdf");
    assert_eq!(part_name(&a, &[2]), "rapor ş - page 3.pdf");
    assert_eq!(part_name(&a, &[0, 1, 2]), "rapor ş - pages 1-3.pdf");
    assert_eq!(part_name(&a, &[0, 1, 2, 4, 7, 8, 9]), "rapor ş - pages 1-3, 5, 8-10.pdf");
    // A label longer than 40 characters gives the page count instead.
    let scattered: Vec<u32> = (0..40).step_by(2).collect();
    assert_eq!(part_name(&a, &scattered), "rapor ş - 20 pages.pdf");
    assert_eq!(page_image_name(&a, 0, PageImage::Png), "rapor ş - page 1.png");
    assert_eq!(page_image_name(&a, 11, PageImage::Jpeg), "rapor ş - page 12.jpg");
    assert_eq!(pages_label(&[4]), "page 5");
    assert_eq!(pages_label(&[2, 2]), "pages 3, 3");
}

#[test]
fn a_pictures_pdf_is_named_after_the_picture_or_the_folder() {
    let dir = Path::new("x").join("Tatil");
    assert_eq!(pictures_pdf_name(&[dir.join("a.jpg")]), Some(dir.join("a.pdf")));
    assert_eq!(pictures_pdf_name(&[dir.join("a.jpg"), dir.join("b.png")]), Some(dir.join("Tatil.pdf")));
    assert_eq!(pictures_pdf_name(&[PathBuf::from("/a.jpg"), PathBuf::from("/b.jpg")]), Some(PathBuf::from("/Pictures.pdf")));
    assert_eq!(pictures_pdf_name(&[]), None);
}

#[test]
fn which_ops_a_selection_gets() {
    assert_eq!(ops_for(3, 0), vec![PdfOp::ImagesToPdf]);
    assert_eq!(ops_for(0, 1), vec![PdfOp::Split, PdfOp::ToImages, PdfOp::Extract]);
    assert_eq!(ops_for(2, 2), PdfOp::ALL.to_vec());
    assert!(is_pdf("A.PDF") && !is_pdf(".pdf") && !is_pdf("a.pdfx"));
    assert!(is_pdf_picture("a.jpg") && !is_pdf_picture("a.heic") && !is_pdf_picture("a.pdf"));
    for op in PdfOp::ALL {
        assert_eq!(PdfOp::from_id(op.id()), Some(op));
    }
    assert!(!PdfOp::ImagesToPdf.needs_pdfium() && PdfOp::Merge.loses_extras() && !PdfOp::ToImages.loses_extras());
}

#[test]
fn render_size_stays_under_64_megapixels_and_65535_px() {
    let a4 = render_size(595.276, 841.89, 300);
    assert_eq!((a4.width, a4.height, a4.lowered), (2481, 3508, false));
    // The probe's huge page (özet §4.2): 14400 pt square at 300 dpi would be 60000 × 60000.
    let huge = render_size(14_400.0, 14_400.0, 300);
    assert!(huge.lowered);
    assert!(u64::from(huge.width) * u64::from(huge.height) <= MAX_RENDER_PIXELS, "{huge:?}");
    assert!(huge.dpi > 39.0 && huge.dpi < 40.0, "{huge:?}");
    // A long thin strip hits the side limit first.
    let strip = render_size(30_000.0, 10.0, 300);
    assert!(strip.lowered && strip.width <= MAX_RENDER_SIDE, "{strip:?}");
    assert!(render_size(0.5, 0.5, 72).width >= 1);
}

#[test]
fn pages_are_placed_as_the_probe_placed_them() {
    // Picture size at 72 dpi, no margin: 640 × 480 pt.
    let p = place(640, 480, None, &PageOptions::DEFAULT);
    assert_eq!((p.page_width, p.page_height, p.x, p.y, p.width, p.height), (640.0, 480.0, 0.0, 0.0, 640.0, 480.0));
    // A 300 dpi JFIF density: 640 px → 153.6 pt.
    assert!((place(640, 480, Some((300.0, 300.0)), &PageOptions::DEFAULT).width - 153.6).abs() < 0.01);
    // A landscape picture gets a landscape A4 (özet §1.5).
    let a4 = PageOptions { size: PageSize::A4, margin: Margin::Small };
    let p = place(4000, 3000, None, &a4);
    assert_eq!((p.page_width, p.page_height), (841.89, 595.276));
    assert!((p.height - (595.276 - 36.0)).abs() < 0.01 && (p.x - (841.89 - p.width) / 2.0).abs() < 0.01);
    // Beyond 14400 pt it is scaled down.
    let big = place(30_000, 10_000, None, &PageOptions::DEFAULT);
    assert!(big.page_width <= MAX_PAGE_POINTS + 0.01);
    let letter = place(300, 400, None, &PageOptions { size: PageSize::Letter, margin: Margin::None });
    assert_eq!((letter.page_width, letter.page_height), (612.0, 792.0));
}

#[test]
fn exif_orientations_turn_the_unit_square() {
    assert_eq!(orient_matrix(1), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(orient_matrix(6), [0.0, -1.0, 1.0, 0.0, 0.0, 1.0]);
    assert_eq!(orient_matrix(8), [0.0, 1.0, -1.0, 0.0, 1.0, 0.0]);
    assert_eq!(orient_matrix(9), orient_matrix(1));
    assert_eq!(shown_size(640, 480, 6), (480, 640));
    assert_eq!(shown_size(640, 480, 3), (640, 480));
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core batch::pdf` → FAIL (`pdf` modülü yok).
- [ ] **Step 3: Implement** `pdf.rs` (yukarıdaki imzalar ve davranış; `orient_matrix` özet §1.3'teki tablo, 2→`[-1,0,0,1,1,0]`, 3→`[-1,0,0,-1,1,1]`, 4→`[1,0,0,-1,0,1]`, 5→`[0,-1,-1,0,1,1]`, 7→`[0,1,1,0,0,0]`). `Kind::Pdf`'i `convert.rs`'e ekle; `cargo build --workspace` hangi `match`'lerin kol istediğini gösterir (arayüz tarafı Task 8'de dolar; şimdilik `gezik` crate'inde `Kind::Pdf => "PDF"` başlığı ve `kind_of`'ta dokunulmaz).
- [ ] **Step 4: Run.** `cargo test -p gezik-core` → PASS; workspace derlenir.
- [ ] **Step 5: Commit** "Add the pure PDF decisions: page ranges, splits, names, page placement and render limits".

---

### Task 2: `gezik-core::batch::pdf_worker` — satır protokolü

**Files:**
- Create: `crates/gezik-core/src/batch/pdf_worker.rs`
- Modify: `crates/gezik-core/src/batch/mod.rs`

**Interfaces:**
- Consumes: Task 1'den `Split`, `PageImage`.
- Produces:

```rust
pub const MAGIC: &str = "gezik-pdf";
pub const VERSION: u32 = 1;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerJob { Merge, Split(Split), Extract(String), Render { dpi: u32, image: PageImage }, Count }
/// One job for the worker. `Debug` never shows the passwords.
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    pub library: PathBuf,           // the pdfium library file
    pub dir: PathBuf,               // where outputs go (an empty folder); unused by Count
    pub job: WorkerJob,
    pub inputs: Vec<PathBuf>,
    pub passwords: Vec<(usize, String)>, // by input index
}
impl Request {
    pub fn to_text(&self) -> String;
    pub fn parse(text: &str) -> Result<Request, String>;
    pub fn password(&self, input: usize) -> Option<&str>;
}
impl fmt::Debug for Request { /* every field but the passwords, which show as "1 given" */ }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure { Damaged, Library, Ranges, Io, Other }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Pages { input: usize, pages: u32 },  // Count's answer
    Steps(u64),                          // work found: this many page steps more
    Step,                                // one page step done
    Lowered { page: u32, dpi: u32 },     // 1-based page rendered at a lower dpi
    NeedsPassword(usize),
    WrongPassword(usize),
    Failed { input: Option<usize>, why: Failure, message: String },
    Done,
}
impl Reply {
    pub fn to_line(&self) -> String;               // no newline
    pub fn parse(line: &str) -> Option<Reply>;     // unknown → None (ignored by the reader)
}
pub fn encode_path(path: &Path) -> String;
pub fn decode_path(text: &str) -> Option<PathBuf>;
```

**Behavior (biçim):**
- Alanlar tek sekmeyle ayrılır. Metin alanları kaçışlıdır: `\` → `\\`, sekme → `\t`, LF → `\n`, CR → `\r`; başka kontrol karakteri `\u{XXXX}` değil, `\x` + iki onaltılık (`\x07`).
- Yol: UTF-8 olarak okunabiliyorsa `s:` + kaçışlı metin; değilse Windows'ta `w:` + her UTF-16 birimi dört küçük onaltılık hane (`OsStrExt::encode_wide` / `OsString::from_wide`), Unix'te `b:` + her bayt iki hane (`OsStrExt::as_bytes` / `OsString::from_vec`). `unsafe` yok.
- İstek metni (satırlar `\n` ile; sıra serbest, ilk satır hariç):
  ```
  gezik-pdf	1
  library	<path>
  dir	<path>
  job	merge | job	split	each | job	split	every	<n> | job	split	ranges	<text>
      | job	extract	<text> | job	render	<dpi>	png|jpeg | job	count
  in	<path>            (bir ya da daha çok; sırası girdi sırası)
  password	<index>	<text>
  ```
  `parse` hataları: ilk satır `gezik-pdf\t1` değil → `not a gezik-pdf 1 request`; eksik `library`/`job`/`in` → `missing <key>`; bilinmeyen anahtar → `unknown line "<key>"`; `password`'ün indeksi girdi sayısını aşıyor → `password for a missing input`; `merge` dışında birden çok `in` → `one input only for <job>`; `merge`'de tek `in` → `merge needs two inputs`.
- Yanıt satırları: `pages\t<i>\t<n>`, `steps\t<n>`, `step`, `lowered\t<page>\t<dpi>`, `needs-password\t<i>`, `wrong-password\t<i>`, `failed\t<i|->\t<damaged|library|ranges|io|other>\t<kaçışlı mesaj>`, `done`.

- [ ] **Step 1: Write the failing tests**

```rust
use super::*;
use crate::batch::pdf::{PageImage, Split};
use std::path::{Path, PathBuf};

fn request(job: WorkerJob, inputs: &[&str]) -> Request {
    Request {
        library: PathBuf::from(r"C:\Users\ş\AppData\Roaming\gezik\tools\pdfium-8086\pdfium.dll"),
        dir: PathBuf::from("/tmp/.gezik-x-1-0/x/.0"),
        job,
        inputs: inputs.iter().map(PathBuf::from).collect(),
        passwords: Vec::new(),
    }
}

#[test]
fn every_job_goes_there_and_back() {
    let jobs = [
        (WorkerJob::Merge, vec!["a.pdf", "b.pdf"]),
        (WorkerJob::Split(Split::EachPage), vec!["a.pdf"]),
        (WorkerJob::Split(Split::Every(10)), vec!["a.pdf"]),
        (WorkerJob::Split(Split::Ranges("1-3, 5,\t8-".into())), vec!["a.pdf"]),
        (WorkerJob::Extract("2-".into()), vec!["a.pdf"]),
        (WorkerJob::Render { dpi: 150, image: PageImage::Jpeg }, vec!["a.pdf"]),
        (WorkerJob::Render { dpi: 300, image: PageImage::Png }, vec!["a.pdf"]),
        (WorkerJob::Count, vec!["a.pdf"]),
    ];
    for (job, inputs) in jobs {
        let req = request(job, &inputs);
        assert_eq!(Request::parse(&req.to_text()).unwrap(), req);
    }
}

#[test]
fn odd_names_and_passwords_pass_unchanged() {
    let mut req = request(WorkerJob::Merge, &["rapor ş \"a\"; $ & b.pdf", "line\nbreak\ttab\\back.pdf"]);
    req.passwords = vec![(1, "p\tw\nö\\".to_owned())];
    let back = Request::parse(&req.to_text()).unwrap();
    assert_eq!(back, req);
    assert_eq!(back.password(1), Some("p\tw\nö\\"));
    assert_eq!(back.password(0), None);
    // Every field stays on its own line: a name or password cannot add a line.
    assert_eq!(req.to_text().lines().count(), 7);
}

#[test]
fn debug_hides_passwords() {
    let mut req = request(WorkerJob::Count, &["a.pdf"]);
    req.passwords = vec![(0, "sekret".to_owned())];
    let shown = format!("{req:?}");
    assert!(!shown.contains("sekret") && shown.contains("1 given"), "{shown}");
}

#[cfg(windows)]
#[test]
fn a_windows_name_that_is_not_unicode_passes() {
    use std::os::windows::ffi::OsStringExt;
    let lone = PathBuf::from(std::ffi::OsString::from_wide(&[0x61, 0xD800, 0x2E, 0x70, 0x64, 0x66]));
    let text = encode_path(&lone);
    assert!(text.starts_with("w:0061d800"), "{text}");
    assert_eq!(decode_path(&text), Some(lone));
}

#[cfg(unix)]
#[test]
fn a_unix_name_that_is_not_utf8_passes() {
    use std::os::unix::ffi::OsStringExt;
    let raw = PathBuf::from(std::ffi::OsString::from_vec(vec![b'a', 0xFF, b'.', b'p', b'd', b'f']));
    let text = encode_path(&raw);
    assert_eq!(text, "b:61ff2e706466");
    assert_eq!(decode_path(&text), Some(raw));
}

#[test]
fn bad_requests_say_why() {
    assert_eq!(Request::parse("hello").unwrap_err(), "not a gezik-pdf 1 request");
    assert_eq!(Request::parse("gezik-pdf\t2\n").unwrap_err(), "not a gezik-pdf 1 request");
    let ok = request(WorkerJob::Count, &["a.pdf"]).to_text();
    assert_eq!(Request::parse(&ok.replace("library\t", "librar\t")).unwrap_err(), "unknown line \"librar\"");
    let two = request(WorkerJob::Count, &["a.pdf", "b.pdf"]).to_text();
    assert_eq!(Request::parse(&two).unwrap_err(), "one input only for count");
    let one = request(WorkerJob::Merge, &["a.pdf"]).to_text();
    assert_eq!(Request::parse(&one).unwrap_err(), "merge needs two inputs");
    assert_eq!(Request::parse(&format!("{ok}password\t3\tx\n")).unwrap_err(), "password for a missing input");
}

#[test]
fn replies_go_there_and_back() {
    let replies = [
        Reply::Pages { input: 0, pages: 14 },
        Reply::Steps(20_000),
        Reply::Step,
        Reply::Lowered { page: 3, dpi: 41 },
        Reply::NeedsPassword(1),
        Reply::WrongPassword(0),
        Reply::Failed { input: Some(1), why: Failure::Damaged, message: "bad xref\ttable\nhere".into() },
        Reply::Failed { input: None, why: Failure::Library, message: "LoadLibraryError".into() },
        Reply::Done,
    ];
    for reply in replies {
        let line = reply.to_line();
        assert!(!line.contains('\n'), "{line}");
        assert_eq!(Reply::parse(&line), Some(reply));
    }
    assert_eq!(Reply::parse("progress=end"), None);
    assert_eq!(Reply::parse("steps\tmany"), None);
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core batch::pdf_worker` → FAIL.
- [ ] **Step 3: Implement** `pdf_worker.rs` as above.
- [ ] **Step 4: Run.** `cargo test -p gezik-core` → PASS; `cargo check -p gezik-core --target x86_64-unknown-linux-gnu` (Unix dalı derlenir).
- [ ] **Step 5: Commit** "Define the line protocol between Gezik and its PDF worker".

---

### Task 3: Resim→PDF — `gezik-batch::pdf::{jpeg, images}`

**Files:**
- Create: `crates/gezik-batch/src/pdf/mod.rs`, `crates/gezik-batch/src/pdf/jpeg.rs`, `crates/gezik-batch/src/pdf/images.rs`, `crates/gezik-batch/tests/pdf_images.rs`
- Modify: `crates/gezik-batch/Cargo.toml` (`pdf-writer`, `miniz_oxide`), `crates/gezik-batch/src/lib.rs` (`pub mod pdf;`, belge yorumuna 5d), `crates/gezik-batch/src/convert/image.rs` (`decode` ve `Source` `pub(crate)`), `THIRD-PARTY.md` (pdf-writer, ryu satırları), `Cargo.lock`

**Interfaces:**
- Consumes: Task 1'den `PageOptions`, `place`, `shown_size`, `orient_matrix`, `MAX_PAGE_POINTS`; 5c'den `crate::convert::image::decode(path) -> io::Result<Source>` (`Source { img, icc, exif, orientation, jpeg }`, alanlar `pub(crate)` yapılır).
- Produces:

```rust
// pdf/jpeg.rs — pdf-probe/jpeg.rs birebir (JpegInfo, parse, passthrough_ok) + şu ek:
pub fn icc_fits(icc: &[u8], components: u8) -> bool; // bytes 16..20: "GRAY"→1, "RGB "→3, "CMYK"→4

// pdf/images.rs
pub struct PicturesPdf { pub pages: usize, pub left_out: Vec<(PathBuf, io::Error)> }
/// Writes `pictures` (in this order, one page each) as a PDF at `out`. Each picture is read
/// alone; `on_picture(path, its size)` after each page; `stop` between pictures (then
/// `Interrupted`, and `out` is removed). A picture that cannot be read is left out (in
/// `left_out`); with none left, an error and no file.
pub fn write_pdf(pictures: &[PathBuf], out: &Path, options: &PageOptions,
                 on_picture: &mut dyn FnMut(&Path, u64), stop: &dyn Fn() -> bool) -> io::Result<PicturesPdf>;
pub const FLATE_LEVEL: u8 = 1;
```

**Behavior** (özet §1, `img2pdf.rs` temel alınır):
- `PdfStream` (`img2pdf.rs`'teki) `PdfFile` adıyla `images.rs`'e gelir: başlık `%PDF-1.7\n%\x80\x80\x80\x80\n\n`, `BufWriter` 1 MiB, her nesne `put` ile hemen yazılır, yalnız `(id, offset)` tutulur. **Sapma:** probe'daki `assert_eq!` yerine xref `1..size` her id için yazılır; hiç yazılmamış id (okunamayıp atlanan resmin ayrılmış numarası) `0000000000 65535 f ` satırı olur. Info sözlüğü `Producer (Gezik)`. Numaralar yalnız resim başarıyla okunduktan **sonra** ayrılır.
- JPEG yolu: `jpeg::parse` + `passthrough_ok` → DCTDecode, bayt bayt aynı; renk uzayı: ICC varsa ve `icc_fits(icc, components)` → `ICCBased` (Flate, `/N`, `/Alternate`), yoksa Device Gray/RGB/CMYK; 4 bileşen ve `adobe.is_some()` → `Decode [1 0 1 0 1 0 1 0]`; sayfa: `shown_size` + `place(…, info.dpi, options)` (dpi yalnız `PageSize::Picture`'te anlam taşır), içerik `q [w 0 0 h x y] cm [orient] cm /Im0 Do Q` (yön 1'de ikinci `cm` yazılmaz).
- Öteki yol (PNG, GIF, WebP, BMP, TIFF, ICO ve aynen geçemeyen JPEG): `decode` → `img.apply_orientation(orientation)` → gri/renk, 8/16 bit ayrımı, alfa düzlemi, `split`, `be16`, `paeth` (`img2pdf.rs` birebir) → Flate (`FLATE_LEVEL`) + `DecodeParms /Predictor 15 /Colors n /BitsPerComponent b /Columns w`; alfa tümüyle opak değilse `SMask` (DeviceGray, aynı bit, Flate); ICC: `icc_fits(icc, n)` ise `ICCBased`. `decode`'un 512 MiB ve 65.535 px sınırları geçerli; aşan resim atlanır ("too large to convert").
- `image::metadata::Orientation` → EXIF numarası: `images.rs`'te 8 kollu `fn exif_number(o: Orientation) -> u8`.
- Hata: okunamayan resim `left_out`'a (mesaj 5c'nin `image_error` metinleri); `stop` → dosya silinir, `Interrupted`; yazma hatası → dosya silinir, hata döner.

- [ ] **Step 1: Write the failing tests** (`tests/pdf_images.rs`; resimler testte üretilir, depoya resim girmez). Yardımcılar (her biri yazdığı yolu döndürür): `jpeg(path, w, h, color)` (`jpeg-encoder` RGB), `jpeg_with_exif(path, orientation)` (64×48, 5c'nin `convert_image.rs`'teki `camera_jpeg_with` kalıbı: APP1 `Exif\0\0` + yön etiketi), `cmyk_jpeg(path)` (`jpeg_encoder::ColorType::Cmyk`), `png_rgba(path)`, `png16(path)`. Küçük bir PDF okuyucu yardımcısı yazılır (yalnız test için): `startxref`'i okur, xref'teki her `n` girdisinin ofsetinde `"<id> 0 obj"` olduğunu doğrular, `/Type /Page` sayısını ve her nesnenin metnini (akışlar hariç) verir.

```rust
#[test]
fn three_pictures_make_three_pages_with_a_valid_xref() {
    let d = dir("three");
    let pics = [jpeg(&d.join("a.jpg"), 64, 48, [200, 30, 30]), png_rgba(&d.join("b.png")), jpeg_with_exif(&d.join("c.jpg"), 6)];
    let out = d.join("out.pdf");
    let report = write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    assert_eq!((report.pages, report.left_out.len()), (3, 0));
    let pdf = Parsed::read(&out); // checks every xref offset
    assert_eq!(pdf.pages(), 3);
    // The JPEG went in byte for byte (DCTDecode passthrough).
    assert!(pdf.stream_equal_to_file(&pics[0]));
    // EXIF 6: shown upright through the matrix, on a 48 × 64 pt page.
    assert!(pdf.text().contains("0 -1 1 0 0 1 cm"));
    assert!(pdf.text().contains("/MediaBox [0 0 48 64]"));
    // Alpha went to an SMask.
    assert!(pdf.text().contains("/SMask"));
}

#[test]
fn cmyk_jpegs_from_photoshop_get_the_decode_array() {
    let d = dir("cmyk");
    let cmyk = cmyk_jpeg(&d.join("cmyk.jpg"));
    assert!(gezik_batch::pdf::jpeg::parse(&std::fs::read(&cmyk).unwrap()).unwrap().adobe.is_some(), "jpeg-encoder writes APP14");
    let out = d.join("o.pdf");
    write_pdf(&[cmyk], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    let text = Parsed::read(&out).text();
    assert!(text.contains("/DeviceCMYK") && text.contains("/Decode [1 0 1 0 1 0 1 0]"), "{text}");
}

#[test]
fn sixteen_bits_stay_and_opaque_alpha_needs_no_mask() {
    let d = dir("png16");
    let out = d.join("o.pdf");
    write_pdf(&[png16(&d.join("a.png"))], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    let text = Parsed::read(&out).text();
    assert!(text.contains("/BitsPerComponent 16") && text.contains("/Predictor 15") && !text.contains("/SMask"), "{text}");
}

#[test]
fn a4_pages_turn_for_landscape_pictures() {
    let d = dir("a4");
    let pics = [jpeg(&d.join("wide.jpg"), 400, 300, [0, 0, 255]), jpeg(&d.join("tall.jpg"), 300, 400, [0, 255, 0])];
    let out = d.join("o.pdf");
    let a4 = PageOptions { size: PageSize::A4, margin: Margin::Small };
    write_pdf(&pics, &out, &a4, &mut |_, _| {}, &never).unwrap();
    let text = Parsed::read(&out).text();
    assert!(text.contains("/MediaBox [0 0 841.89 595.276]") && text.contains("/MediaBox [0 0 595.276 841.89]"), "{text}");
}

#[test]
fn unreadable_pictures_are_left_out_and_numbers_stay_valid() {
    let d = dir("bad");
    let bad = d.join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    let good = jpeg(&d.join("g.jpg"), 8, 8, [1, 2, 3]);
    let out = d.join("o.pdf");
    let report = write_pdf(&[bad.clone(), good], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    assert_eq!(report.pages, 1);
    assert_eq!(report.left_out[0].0, bad);
    Parsed::read(&out); // xref still checks out
    // Nothing readable: an error, and no file.
    let none = d.join("none.pdf");
    assert!(write_pdf(&[bad], &none, &PageOptions::DEFAULT, &mut |_, _| {}, &never).is_err());
    assert!(!none.exists());
}

#[test]
fn stop_removes_the_file() {
    let d = dir("stop");
    let pics: Vec<PathBuf> = (0..3).map(|i| jpeg(&d.join(format!("{i}.jpg")), 8, 8, [0, 0, 0])).collect();
    let out = d.join("o.pdf");
    let seen = std::cell::Cell::new(0);
    let err = write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| seen.set(seen.get() + 1), &|| seen.get() >= 1).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(!out.exists());
}
```

  `jpeg.rs` birim testleri (elle yazılmış başlıklar): SOF3 (lossless), SOF9 (aritmetik) ve 12 bitlik SOF1 → `passthrough_ok() == false`; iki parçalı ICC sırası tersten verilince doğru birleşir; JFIF birim 2 (cm) → dpi ×2,54; `icc_fits` üç uzay için doğru, uyuşmayanda false. pdfium varken (`GEZIK_TEST_PDFIUM`) ek doğrulama Task 5'te.
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-batch --test pdf_images` → FAIL.
- [ ] **Step 3: Implement** (bağımlılıkları ekle, `decode`'u aç, `jpeg.rs` ve `images.rs`). `THIRD-PARTY.md` tablosuna `pdf-writer 0.15.0 | MIT OR Apache-2.0 | https://github.com/typst/pdf-writer` ve `ryu (through pdf-writer)` satırı (sürüm `Cargo.lock`'tan, lisans `cargo metadata --format-version 1`'in `license` alanından).
- [ ] **Step 4: Run.** `cargo test -p gezik-batch` → PASS; çapraz denetimler.
- [ ] **Step 5: Commit** "Write pictures into a PDF without re-encoding JPEGs, streaming each object to disk".

---

### Task 4: Üst süreç tarafı — `gezik-batch::pdf::client` ve sahte işçi

**Files:**
- Create: `crates/gezik-batch/src/pdf/client.rs`, `crates/gezik-batch/examples/fake_pdf_worker.rs`, `crates/gezik-batch/tests/pdf_client.rs`
- Modify: `crates/gezik-batch/src/pdf/mod.rs`

**Interfaces:**
- Consumes: Task 2'nin `Request`, `Reply`, `Failure`; `gezik_platform::ChildProcess::spawn_with_input`; `crate::convert::ffmpeg::stderr_tail`.
- Produces:

```rust
pub const WORKER_ARG: &str = "--pdf-worker";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worker { pub program: PathBuf, pub args: Vec<OsString> }
impl Worker {
    /// This program in worker mode (`current_exe() --pdf-worker`).
    pub fn this_exe() -> io::Result<Worker>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended { Done, NeedsPassword(usize), WrongPassword(usize) }
/// Runs one request: writes it to the worker's input (then closes it), passes every reply to
/// `on_reply` as it comes, and looks every 50 ms whether to `stop` (then the worker is ended
/// with all it started: `Interrupted`). A `failed` reply becomes the error (`PdfFailed`); an
/// exit without `done` (a crash) is "PDF engine stopped (…)" with the end of its error output.
pub fn run(worker: &Worker, request: &Request, on_reply: &mut dyn FnMut(&Reply), stop: &dyn Fn() -> bool) -> io::Result<Ended>;
/// The page count of `input`; `None` when it needs a password.
pub fn count_pages(worker: &Worker, library: &Path, input: &Path, stop: &dyn Fn() -> bool) -> io::Result<Option<u32>>;
#[derive(Debug)]
pub struct PdfFailed { pub input: Option<usize>, pub why: Failure, pub message: String }
impl fmt::Display for PdfFailed; // Damaged → "the PDF is damaged ({message})"; Library → "pdfium could not be loaded ({message})"; others → message
pub fn engine_stopped_text(code: Option<i32>, tail: &str) -> String; // "PDF engine stopped (exit code 3)" / "PDF engine stopped (killed)" + ": tail"
```

**Behavior:**
- `run`: `ChildProcess::spawn_with_input(&worker.program, &worker.args, None, Some(request.to_text().into_bytes()))`; satırlar `stdout_lines().ready()` ile `wait_or_stop` kapanışında okunur (5c'nin `ffmpeg::run` kalıbı); `Reply::parse` `None` verirse satır atlanır. Bittikten sonra kalan satırlar okunur. Sıra: `stop` → `Interrupted("cancelled")`; `NeedsPassword`/`WrongPassword` görüldüyse `Ok(Ended::…)`; `Failed` görüldüyse `Err(io::Error::new(kind, PdfFailed))` (`Damaged` → `InvalidData`, `Ranges` → `InvalidInput`, `Io` → `Other`, ötekiler `Other`); `done` görüldü ve çıkış 0 → `Ok(Done)`; aksi → `Err(other(engine_stopped_text(…)))`.
- İşçi pencere açmaz (`ChildProcess` Windows'ta `CREATE_NO_WINDOW`, iş nesnesi; Unix'te süreç grubu).
- Sahte işçi (`examples/fake_pdf_worker.rs`, gönderilmez): isteği stdin'den okur, `Request::parse` eder; argümanlarını (`std::env::args`) her girdinin yanına `<girdi>.args` dosyasına satır satır yazar ve süreç numarasını `<girdi>.log`'a ekler. Her girdi dosyası bir betiktir (satır satır): `pages N` (varsayılan 3), `password P`, `damaged`, `crash` (ilk adımdan sonra çıkış kodu 101, `done` yok), `hang` (adımlardan sonra öldürülene dek bekler), `hold` (`<girdi>.hold` var oldukça bekler), `slow MS`, `lowered PAGE DPI`. Davranış: önce bütün girdileri "açar" (parola denetimi: `password P` varsa ve `request.password(i)` yoksa `needs-password i`, farklıysa `wrong-password i`, çıkış 0; `damaged` → `failed i damaged …`, çıkış 1); sonra işe göre `steps`/`step` yazar ve `dir`'e çıktıları gerçek adlarıyla (`gezik_core::batch::pdf` işlevleri) yazar: içerik `"fake <işlem> <sayfalar>"`; `count` → `pages 0 N`; sonda `done`.

- [ ] **Step 1: Write the failing tests** (`tests/pdf_client.rs`; `fake()` yardımcısı 5c `tests/ffmpeg.rs`'teki gibi `target/<profile>/examples/fake_pdf_worker[.exe]`; kopyalar `OnceLock` ile bir kez):

```rust
#[test]
fn a_split_reports_steps_and_writes_its_parts() {
    let d = dir("split");
    let input = script(&d, "a.pdf", "pages 4\n");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut replies = Vec::new();
    let ended = run(&worker(), &request(&out, WorkerJob::Split(Split::Every(3)), &[&input]), &mut |r| replies.push(r.clone()), &never).unwrap();
    assert_eq!(ended, Ended::Done);
    assert_eq!(replies.iter().filter(|r| **r == Reply::Step).count(), 4);
    assert!(replies.contains(&Reply::Steps(4)));
    assert_eq!(names(&out), ["a - page 4.pdf", "a - pages 1-3.pdf"]);
}

#[test]
fn passwords_go_through_the_input_not_the_command_line() {
    let d = dir("password");
    let input = script(&d, "s.pdf", "password gizli\n");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut req = request(&out, WorkerJob::Count, &[&input]);
    assert_eq!(run(&worker(), &req, &mut |_| {}, &never).unwrap(), Ended::NeedsPassword(0));
    req.passwords = vec![(0, "yanlış".into())];
    assert_eq!(run(&worker(), &req, &mut |_| {}, &never).unwrap(), Ended::WrongPassword(0));
    req.passwords = vec![(0, "gizli".into())];
    assert_eq!(run(&worker(), &req, &mut |_| {}, &never).unwrap(), Ended::Done);
    let args = std::fs::read_to_string(d.join("s.pdf.args")).unwrap();
    assert!(!args.contains("gizli") && !args.contains("yanlış"), "{args}");
    assert_eq!(count_pages(&worker(), Path::new("lib"), &input, &never).unwrap(), None);
}

#[test]
fn a_crash_is_an_engine_stop_and_damage_is_named() {
    let d = dir("crash");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let crash = script(&d, "c.pdf", "crash\n");
    let err = run(&worker(), &request(&out, WorkerJob::Split(Split::EachPage), &[&crash]), &mut |_| {}, &never).unwrap_err();
    assert!(err.to_string().starts_with("PDF engine stopped (exit code 101)"), "{err}");
    let bad = script(&d, "b.pdf", "damaged\n");
    let err = run(&worker(), &request(&out, WorkerJob::Count, &[&bad]), &mut |_| {}, &never).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().starts_with("the PDF is damaged"), "{err}");
}

#[test]
fn stop_ends_a_hanging_worker_at_once() {
    let d = dir("hang");
    let out = d.join("out");
    std::fs::create_dir(&out).unwrap();
    let input = script(&d, "h.pdf", "hang\n");
    let started = Instant::now();
    let err = run(&worker(), &request(&out, WorkerJob::Split(Split::EachPage), &[&input]), &mut |_| {}, &|| started.elapsed() > Duration::from_millis(300)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(started.elapsed() < Duration::from_secs(2));
    let pid: u32 = std::fs::read_to_string(d.join("h.pdf.log")).unwrap().trim().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
}

#[test]
fn this_exe_runs_itself_in_worker_mode() {
    let w = Worker::this_exe().unwrap();
    assert_eq!(w.args, [std::ffi::OsString::from(WORKER_ARG)]);
    assert!(w.program.is_absolute());
}
```

- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-batch --test pdf_client` → FAIL.
- [ ] **Step 3: Implement** `client.rs` and the fake worker.
- [ ] **Step 4: Run.** `cargo test -p gezik-batch` (örnekleri de derler) → PASS. Linux dalı için `cargo check -p gezik-batch --no-default-features --target x86_64-unknown-linux-gnu --examples --tests`.
- [ ] **Step 5: Commit** "Run the PDF worker as a separate process with progress, passwords and cancel".

---

### Task 5: İşçi — `gezik-batch::pdf::{pdfium, worker}` ve `gezik --pdf-worker`

**Files:**
- Create: `crates/gezik-batch/src/pdf/pdfium.rs`, `crates/gezik-batch/src/pdf/worker.rs`, `crates/gezik-batch/examples/pdf_worker.rs`, `crates/gezik-batch/tests/pdf_worker.rs`, `crates/gezik-batch/tests/data/pdf/enc_aes256.pdf`
- Modify: `crates/gezik-batch/Cargo.toml` (`pdfium-render`), `Cargo.lock` (libloading tek kopya), `crates/gezik-batch/src/pdf/mod.rs`, `crates/gezik/src/main.rs`, `crates/gezik-batch/tests/data/SOURCES.md`, `THIRD-PARTY.md` (pdfium-render ve getirdikleri)

**Interfaces:**
- Consumes: Task 1 (`split_parts`, `extract_pages`, `merged_name`, `part_name`, `page_image_name`, `render_size`, `JPEG_QUALITY`), Task 2 (`Request`, `Reply`, `Failure`), Task 4 (`WORKER_ARG`, `Worker`, `run`, `count_pages`).
- Produces:

```rust
// pdf/worker.rs
/// The worker's whole life: reads the request from stdin, does it, writes replies to stdout;
/// the exit code (0 done or a password asked, 1 failed, 2 a bad request).
pub fn main() -> i32;
pub fn serve(request_text: &str, out: &mut dyn Write) -> i32;

// pdf/pdfium.rs (özet §2, pdfops.rs birebir temel)
pub fn bind(library: &Path) -> Result<Pdfium, String>;      // Pdfium::bind_to_library(library)
pub enum OpenError { NeedsPassword, WrongPassword, Damaged, Other(String) }
pub fn open<'a>(p: &'a Pdfium, path: &Path, password: Option<&str>) -> Result<PdfDocument<'a>, OpenError>;
pub fn pages_to_new<'a>(p: &'a Pdfium, src: &PdfDocument, idx: &[u32]) -> Result<PdfDocument<'a>, PdfiumError>;
pub fn render_page(page: &PdfPage, dpi: u32, image: PageImage, path: &Path) -> io::Result<RenderSize>;
```

**Behavior:**
- `gezik` `main()`'in ilk satırları (Slint'ten, ayardan önce):
  ```rust
  if std::env::args_os().nth(1).is_some_and(|arg| arg == gezik_batch::pdf::client::WORKER_ARG) {
      std::process::exit(gezik_batch::pdf::worker::main());
  }
  ```
  `windows_subsystem = "windows"` sürüm derlemesinde de borular `ChildProcess`'in verdiği tutamaçlardan gelir.
- `examples/pdf_worker.rs`: yalnız `std::process::exit(gezik_batch::pdf::worker::main())` (testler gerçek işçiyi `gezik` exe'si olmadan çalıştırsın diye; gönderilmez).
- `serve` sırası: isteği ayrıştır (hata → `failed - other "bad request: …"`, 2); `bind` (hata → `failed - library "<hata>"`, 1); bütün girdileri sırayla aç (`open`; `NeedsPassword` → `needs-password i`, 0; `WrongPassword` → `wrong-password i`, 0; `Damaged` → `failed i damaged "…"`, 1; `Other` → `failed i io "…"`, 1); sonra iş:
  - `Count`: `pages 0 N`.
  - `Merge`: `steps <toplam sayfa>`; her girdi `pages_mut().append(&src)`'den sonra o girdinin sayfa sayısı kadar `step`; `dir.join(merged_name(&inputs[0]))`'a `save_to_file`.
  - `Split`: `split_parts` (hata → `failed 0 ranges "<metin>"`, 1); `steps <parçaların sayfa toplamı>`; her parça `pages_to_new` (ardışık koşular tek `copy_page_range_from_document` çağrısı, özet §2.3) → `dir.join(part_name(input, part))`; her sayfa için `step`.
  - `Extract`: `extract_pages` → tek `pages_to_new` → `part_name(input, &pages)`.
  - `Render`: `steps <sayfa sayısı>`; her sayfa `render_page(page, dpi, image, dir.join(page_image_name(input, i, image)))`; dönen `RenderSize.lowered` ise `lowered <i+1> <dpi'.round()>`; `step`.
  - Sonda `done`, 0. Her yanıt satırı yazıldıktan sonra `flush`.
- `render_page`: `render_size(page.width().value, page.height().value, dpi)`; `PdfRenderConfig::new().scale_page_by_factor(size.dpi / 72.0)`; `page.render_with_config(&cfg)?` → `bmp.as_rgba_bytes()` → **`bmp` hemen bırakılır** → RGBA yerinde RGB'ye sıkıştırılır (`for i in 0..w*h { buf.copy_within(4*i..4*i+3, 3*i) }; buf.truncate(3*w*h)`), böylece tepe bellek iki tam kopyayı aşmaz (64 MP'de ~512 MB). PNG: `image::save_buffer(path, &rgb, w, h, ExtendedColorType::Rgb8)`; JPEG: `jpeg_encoder::Encoder::new_file(path, JPEG_QUALITY)`, `set_density(PixelDensity { density: (d, d), unit: Inches })` (`d = size.dpi.round() as u16`), `encode(&rgb, w as u16, h as u16, ColorType::Rgb)`.
- Kayıplar özet §2.2/§2.5'teki gibi (yer imi, form, Info, şifre); kod bir şey yapmaz, katman söyler (Task 8).
- `pdfium-render` satırı ve `cargo update -p libloading@0.9.0 --precise 0.8.9`; `cargo tree -i libloading` tek sürüm göstermeli. Göstermezse ikinci kopya kalır, Task 9 notuna boyutuyla yazılır.
- Şifreli örnek: Task 3'ün `write_pdf`'iyle üretilmiş 3 sayfalık bir PDF'ten qpdf 12.4.2 ile (`https://github.com/qpdf/qpdf/releases/tag/v12.4.2`, Windows zip, scratchpad'e): `qpdf --encrypt --user-password=pw --owner-password=own --bits=256 -- three.pdf enc_aes256.pdf`. `SOURCES.md`'ye kaynak ve komut ("Gezik'in kendi ürettiği sayfalar; lisans sorunu yok").

- [ ] **Step 1: Write the failing tests** (`tests/pdf_worker.rs`; gerçek işçi `examples/pdf_worker`; `GEZIK_TEST_PDFIUM` yoksa pdfium testleri ilk satırda `return`):

```rust
fn library() -> Option<PathBuf> { std::env::var_os("GEZIK_TEST_PDFIUM").map(PathBuf::from) }

#[test]
fn a_missing_library_is_a_library_failure() {   // pdfium gerekmez, her zaman çalışır
    let d = dir("nolib");
    let pdf = three_page_pdf(&d);                // Task 3'ün write_pdf'iyle
    let req = Request { library: d.join("nope.dll"), dir: d.clone(), job: WorkerJob::Count, inputs: vec![pdf], passwords: vec![] };
    let err = run(&real_worker(), &req, &mut |_| {}, &never).unwrap_err();
    assert!(err.to_string().starts_with("pdfium could not be loaded"), "{err}");
}

#[test]
fn merge_split_extract_and_render_with_pdfium() {
    let Some(lib) = library() else { return };
    let d = dir("ops");
    let a = three_page_pdf(&d);                                  // "a.pdf": write_pdf of three 64×48 PNGs (no dpi → 72)
    let b = pdf_of(&d, "rapor ş.pdf", 2);                        // Türkçe ad
    let out = fresh(&d, "merge");
    run(&real_worker(), &req(&lib, &out, WorkerJob::Merge, &[&a, &b]), &mut |_| {}, &never).unwrap();
    assert_eq!(count(&lib, &out.join("a (merged).pdf")), 5);
    let out = fresh(&d, "split");
    run(&real_worker(), &req(&lib, &out, WorkerJob::Split(Split::Ranges("1-2, 3".into())), &[&a]), &mut |_| {}, &never).unwrap();
    assert_eq!(count(&lib, &out.join("a - pages 1-2.pdf")), 2);
    assert_eq!(count(&lib, &out.join("a - page 3.pdf")), 1);
    let out = fresh(&d, "extract");
    run(&real_worker(), &req(&lib, &out, WorkerJob::Extract("2-".into()), &[&b]), &mut |_| {}, &never).unwrap();
    assert_eq!(count(&lib, &out.join("rapor ş - page 2.pdf")), 1);
    let out = fresh(&d, "render");
    run(&real_worker(), &req(&lib, &out, WorkerJob::Render { dpi: 72, image: PageImage::Png }, &[&a]), &mut |_| {}, &never).unwrap();
    let png = image::open(out.join("a - page 1.png")).unwrap();
    assert_eq!((png.width(), png.height()), (64, 48));           // the 64 × 48 pt page at 72 dpi
}

#[test]
fn encrypted_pdfs_ask_and_give_an_unencrypted_copy() {
    let Some(lib) = library() else { return };
    let enc = data("pdf/enc_aes256.pdf");
    assert_eq!(count_pages(&real_worker(), &lib, &enc, &never).unwrap(), None);
    let d = dir("enc");
    let mut r = req(&lib, &d, WorkerJob::Split(Split::EachPage), &[&enc]);
    r.passwords = vec![(0, "nope".into())];
    assert_eq!(run(&real_worker(), &r, &mut |_| {}, &never).unwrap(), Ended::WrongPassword(0));
    r.passwords = vec![(0, "pw".into())];
    assert_eq!(run(&real_worker(), &r, &mut |_| {}, &never).unwrap(), Ended::Done);
    // The part opens without any password (özet §2.5).
    assert_eq!(count_pages(&real_worker(), &lib, &d.join("enc_aes256 - page 1.pdf"), &never).unwrap(), Some(1));
}

#[test]
fn a_huge_page_is_rendered_at_a_lower_dpi() {
    let Some(lib) = library() else { return };
    let d = dir("huge");
    let huge = huge_page_pdf(&d);   // pdf_writer::Pdf, one 14400 × 14400 pt page, empty content
    let out = fresh(&d, "out");
    let mut lowered = None;
    run(&real_worker(), &req(&lib, &out, WorkerJob::Render { dpi: 300, image: PageImage::Jpeg }, &[&huge]),
        &mut |r| if let Reply::Lowered { page, dpi } = r { lowered = Some((*page, *dpi)) }, &never).unwrap();
    assert_eq!(lowered.map(|l| l.0), Some(1));
    let jpg = image::open(out.join("huge - page 1.jpg")).unwrap();
    assert!(u64::from(jpg.width()) * u64::from(jpg.height()) <= MAX_RENDER_PIXELS);
}

#[test]
fn damaged_files_are_named_damaged() {
    let Some(lib) = library() else { return };
    let d = dir("damaged");
    let bytes = std::fs::read(three_page_pdf(&d)).unwrap();
    let cut = d.join("cut.pdf");
    std::fs::write(&cut, &bytes[..bytes.len() / 3]).unwrap();
    let err = count_pages(&real_worker(), &lib, &cut, &never).unwrap_err();
    assert!(err.to_string().starts_with("the PDF is damaged"), "{err}");
}

#[test]
fn pictures_written_by_gezik_render_like_the_pictures() {
    let Some(lib) = library() else { return };
    // Task 3'ün EXIF 6 ve CMYK örnekleri: pdfium'un işlediği sayfa, image'ın döndürüp çözdüğü
    // resimle ortalama |fark| < 20 (özet §1.6: 0,16 ve 15,93); yönler 1-8 dik çıkar.
}
```
  (Son testin gövdesi: Task 3'ün `jpeg_with_exif(…, o)` ile 8 yönün her biri ve `cmyk_jpeg` → `write_pdf` → `Render 72 png` → `image` ile kaynak çözülüp `apply_orientation`, boyutlar eşit, ortalama mutlak fark yön için < 2, CMYK için < 20. Ortalama fark yardımcısı `mean_abs_diff(a: &RgbImage, b: &RgbImage) -> f64` testte yazılır.)
  Türkçe adla açma ya da kaydetme düşerse (`load_pdf_from_file`/`save_to_file` Windows'ta ANSI yol kullanıyorsa) `open` `load_pdf_from_reader(std::fs::File::open(path)?, password)`, kaydetme `save_to_writer(&mut File::create(path)?)` olur; testler aynı kalır.
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-batch --test pdf_worker` → FAIL. Sonra özetteki 8086 Windows x64 kitaplığıyla (`https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8086/pdfium-win-x64.tgz`, scratchpad'e açılır, SHA-256 `1fd8af952832dbb0eb16d9249f68fe09e5f5ebf7c3dd9f6066ea2720cc28487d` denetlenir) `GEZIK_TEST_PDFIUM=<…>/bin/pdfium.dll`.
- [ ] **Step 3: Implement** `pdfium.rs`, `worker.rs`, örnek, `main.rs` kancası, bağımlılık ve libloading sabitlemesi; şifreli örneği üret ve `SOURCES.md`'ye yaz. `THIRD-PARTY.md`: `pdfium-render 0.9.4 | MIT OR Apache-2.0 | https://github.com/ajrcarey/pdfium-render` ve getirdiği `maybe-owned`, `utf16string`, `vecmath`, `piston-float` (sürüm `Cargo.lock`'tan, lisans `cargo metadata`'dan).
- [ ] **Step 4: Run.** `cargo test -p gezik-batch` (env'siz ve env'li) → PASS. Elle: `cargo build -p gezik` sonra Git Bash'te `printf 'gezik-pdf\t1\nlibrary\ts:/nope\ndir\ts:.\njob\tcount\nin\ts:a.pdf\n' | target/debug/gezik.exe --pdf-worker; echo $?` → `failed\t-\tlibrary\t…` ve `1`, pencere açılmaz.
- [ ] **Step 5: Commit** "Do merge, split, extract and render in a pdfium worker that runs as gezik --pdf-worker".

---

### Task 6: Motor görevleri — `PdfTask`, `ImagesToPdfTask`, `PlaceTask` genellemesi

**Files:**
- Create: `crates/gezik-batch/src/tasks/pdf.rs`, `crates/gezik-batch/src/tasks/images_pdf.rs`, `crates/gezik-batch/tests/pdf_tasks.rs`
- Modify: `crates/gezik-batch/src/tasks/mod.rs` (modüller, dışa aktarımlar, belge yorumu), `crates/gezik-batch/src/tasks/place.rs`, `crates/gezik-ops/src/task.rs` (`TaskKind::Pdf`)

**Interfaces:**
- Consumes: Task 1-5; `gezik_ops::{Question::Password, Answer, RunCx::{staging_dir, temp_file_for, found, one_done, skip, fail, ask, cancelled, paused}, restart, MoveTask::placing}`.
- Produces:

```rust
// gezik-ops
pub enum TaskKind { …, Pdf }   // verb "Make PDF from" (only a fallback: PDF jobs bring their own label)

// tasks/pdf.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfWork { Merge, Split(Split), Extract(String), Render { dpi: u32, image: PageImage } }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfTools { pub worker: Worker, pub library: PathBuf }
/// One "Merge/Split/Extract/PDF to images": the worker into a staging folder, then placed
/// next to the first input with the conflict list. Submit with
/// `submit_chain(pdf_chain(..), Some(pdf_label(..)))`.
pub fn pdf_chain(work: PdfWork, inputs: Vec<PathBuf>, tools: PdfTools) -> Vec<Box<dyn Task>>;
pub fn pdf_label(work: &PdfWork, inputs: &[PathBuf]) -> String;
// "Merge 3 PDFs", "Split a.pdf" / "Split 2 PDFs", "Extract pages from a.pdf", "Save a.pdf as pictures"

// tasks/images_pdf.rs
pub struct ImagesToPdfTask;
impl ImagesToPdfTask { pub fn new(pictures: Vec<PathBuf>, output: PathBuf, options: PageOptions) -> ImagesToPdfTask; }
pub fn images_pdf_label(count: usize) -> String; // "Make PDF from 12 pictures" / "Make PDF from 1 picture"

// tasks/place.rs
impl PlaceTask {
    /// What a job made in `stage` moved into `dir` as it is (conflicts asked); undo trashes it.
    pub fn outputs(dir: PathBuf, stage: Arc<Mutex<Option<PathBuf>>>, kind: TaskKind, title: String) -> PlaceTask;
}
```

**Behavior:**
- `PlaceTask`'a `kind: TaskKind` ve `title: String` alanları; `new` eskisi gibi (`TaskKind::Extract`, `"Extracting {archive}"`), `outputs` `ExtractTo::Into(dir)` ile; `kind()`/`title()` alanları döndürür, `MoveTask::placing(pairs, self.kind)`.
- `PdfTask` (`tasks/pdf.rs` içinde, `pub(super)`): `kind()` `TaskKind::Pdf`; başlıklar "Merging 3 PDFs", "Splitting a.pdf", "Extracting pages from a.pdf", "Saving a.pdf as PNG pictures" / "… JPEG pictures"; `count()` girdi sayısı; `resources()` girdiler + hedef klasör, `Work::Cpu`; `workers()` `Some(1)` (spec 9.1 "tek iş parçacığı").
- `plan`: birleştirmede tek öğe (bütün girdiler), ötekilerde girdi başına bir öğe; `Stage::Parallel`, `.uncounted()` (sayfalar sayılır), `.top(i)`.
- `run`: aşama klasörü bir kez (`run.staging_dir(&dir.join("x"))`, içinde `x/`; `Arc<Mutex<Option<PathBuf>>>`'a yazılır, ardından gelen `PlaceTask` okur). Her öğe kendi alt klasörüne yazar: `content/.<öğe>`. Döngü: istek (girdiler, alt klasör, parolalar) → `client::run(..., stop = || run.cancelled() || run.paused())`:
  - `Steps(n)` → öğenin bu denemede bulduğu toplam; ilk denemede aşılmayan kısım için `run.found(fazla, 0)`; `Step` → aynı mantıkla `run.one_done(0)` (duraklatma sonrası tekrar sayılmaz: öğe başına `Mutex<HashMap<usize, (u64, u64)>>` en yüksek bulunan/biten).
  - `Lowered { page, dpi }` → `run.skip(&dir.join(page_image_name(input, page-1, image)), &io::Error::other(format!("page {page} was made at {dpi} dpi: at {asked} dpi it would be too large")))`.
  - `Ended::Done` → alt klasördeki dosyalar `content`'e taşınır (aynı disk, `std::fs::rename`), alt klasör silinir, `Ok(Outcome::Nothing)` (yerleşenleri `PlaceTask` bildirir).
  - `NeedsPassword(i)` / `WrongPassword(i)` → `run.ask(Question::Password { archive: inputs[i].clone(), retry: wrong })`; `Answer::Text(p)` → parolalara eklenir, döngü sürer; başka yanıt → iptal edildiyse `Interrupted`, değilse `run.skip(&inputs[i], "no password given")` (birleştirmede `"no password given; nothing was merged"`), `Ok(Nothing)`.
  - Hata ya da duraklatma: alt klasör silinir; `paused && !cancelled` → `Err(gezik_ops::restart())`; iptal → `Interrupted`; öteki hatalar olduğu gibi (satır metni `PdfFailed`/"PDF engine stopped"); öbür girdiler sürer.
- `pdf_chain` = `[PdfTask, PlaceTask::outputs(dir, stage, TaskKind::Pdf, başlık)]`, `dir` = ilk girdinin klasörü.
- `ImagesToPdfTask`: `kind()` `Pdf`; başlık `"Making {ad} from 12 pictures"`; `Work::Cpu`, `workers()` `Some(1)`; plan: tek öğe `.target(output).checked()` (`Facts.size` = resimlerin toplamı); run: `temp = run.temp_file_for(output)`, `write_pdf(…, on_picture = |_, size| run.add_bytes(size), stop = || run.stopped())`; `left_out` → `run.skip(path, err)`; başarıda `move_entry(temp, output)` → `Outcome::Created { path, facts: facts_after(output, false), from: None }`; hata/iptalde geçici dosya silinir.

- [ ] **Step 1: Write the failing tests** (`tests/pdf_tasks.rs`; motor, `finish_with`, `undo`, `names`, `leftovers` yardımcıları 5c `convert_tasks.rs`'tekiler gibi; `fake_tools()` → `PdfTools { worker: Worker { program: fake(), args: vec![] }, library: "lib".into() }`; şifre sorularını yanıtlayan sürüm: `finish_answering(engine, job, answers: Vec<Answer>) -> (Report, Vec<Question>)`):

```rust
#[test]
fn split_each_page_places_the_parts_and_one_undo_trashes_them() {
    let d = dir("split");
    let a = script(&d, "a.pdf", "pages 3\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![a.clone()], fake_tools()), Some(pdf_label(&PdfWork::Split(Split::EachPage), &[a])));
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // (`.args` and `.log` are the fake worker's own notes.)
    assert_eq!(names(&d), ["a - page 1.pdf", "a - page 2.pdf", "a - page 3.pdf", "a.pdf", "a.pdf.args", "a.pdf.log"]);
    assert!(leftovers(&d).is_empty());
    undo(&engine);
    assert_eq!(names(&d), ["a.pdf", "a.pdf.args", "a.pdf.log"]);
}

#[test]
fn a_wrong_password_is_asked_again_and_never_put_on_the_command_line() {
    let d = dir("password");
    let s = script(&d, "s.pdf", "password gizli\npages 2\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Extract("2".into()), vec![s.clone()], fake_tools()), None);
    let (report, asked) = finish_answering(&engine, job, vec![Answer::Text("yanlış".into()), Answer::Text("gizli".into())]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(asked, [Question::Password { archive: s.clone(), retry: false }, Question::Password { archive: s.clone(), retry: true }]);
    assert!(d.join("s - page 2.pdf").is_file());
    let args = std::fs::read_to_string(d.join("s.pdf.args")).unwrap();
    assert!(!args.contains("gizli") && !args.contains("yanlış"));
}

#[test]
fn no_password_skips_the_pdf_and_a_merge_entirely() {
    // Extract: skipped note "no password given"; Merge of [plain, locked]: note "…nothing was merged", no output.
}

#[test]
fn a_crashing_worker_fails_its_pdf_and_the_others_still_land() {
    let d = dir("crash");
    let ok = script(&d, "ok.pdf", "pages 2\n");
    let bad = script(&d, "bad.pdf", "crash\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![bad.clone(), ok], fake_tools()), None);
    let (report, _) = finish_with(&engine, job, None);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, bad);
    assert!(report.failures[0].message.starts_with("PDF engine stopped"), "{}", report.failures[0].message);
    assert!(d.join("ok - page 2.pdf").is_file() && !d.join("bad - page 1.pdf").exists());
    assert!(leftovers(&d).is_empty());
}

#[test]
fn cancel_ends_the_worker_and_leaves_nothing() {
    let d = dir("cancel");
    let h = script(&d, "h.pdf", "pages 5\nhang\n");
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![h], fake_tools()), None);
    wait("the worker to start", || d.join("h.pdf.log").exists());
    let started = Instant::now();
    engine.cancel(job);
    let (report, _) = finish_with(&engine, job, None);
    assert!(report.cancelled && started.elapsed() < Duration::from_secs(2));
    let pid: u32 = std::fs::read_to_string(d.join("h.pdf.log")).unwrap().lines().next().unwrap().parse().unwrap();
    wait("the worker to end", || !gezik_platform::process_alive(pid));
    assert_eq!(names(&d), ["h.pdf", "h.pdf.args", "h.pdf.log"]);
    assert!(leftovers(&d).is_empty());
}

#[test]
fn a_pause_ends_the_worker_and_the_pdf_is_done_once_after_resume() {
    // script "pages 2\nhold\n" + h.pdf.hold; wait for log; engine.pause(job); wait worker pid dead;
    // remove .hold; engine.resume(job); finish → two outputs, log has two pids, report.failures empty,
    // and the panel's done count did not pass its total (Event::Progress items_done <= items_total).
}

#[test]
fn existing_outputs_go_through_the_conflict_list() {
    let d = dir("conflict");
    let a = script(&d, "a.pdf", "pages 2\n");
    std::fs::write(d.join("a - page 1.pdf"), b"old").unwrap();
    let engine = engine(&d);
    let job = engine.submit_chain(pdf_chain(PdfWork::Split(Split::EachPage), vec![a], fake_tools()), None);
    let (report, events) = finish_with(&engine, job, Some(Decision::Skip));
    assert!(events.iter().any(|e| matches!(e, Event::Conflicts { .. })));
    assert!(report.failures.is_empty());
    assert_eq!(std::fs::read(d.join("a - page 1.pdf")).unwrap(), b"old");
    undo(&engine);
    assert_eq!(std::fs::read(d.join("a - page 1.pdf")).unwrap(), b"old");
    assert!(!d.join("a - page 2.pdf").exists());
}

#[test]
fn a_lowered_page_is_a_note_not_a_failure() {
    // script "pages 2\nlowered 2 41\n", Render 300 png → report.skipped has dir/"a - page 2.png"
    // with "page 2 was made at 41 dpi: at 300 dpi it would be too large"; failures empty.
}

#[test]
fn damaged_pdfs_fail_with_their_reason() {
    // script "damaged\n" → failure message starts with "the PDF is damaged".
}

#[test]
fn pictures_make_one_pdf_that_one_undo_trashes() {
    let d = dir("pictures");
    let pics: Vec<PathBuf> = (0..3).map(|i| jpeg(&d.join(format!("{i}.jpg")), 16, 12, [9, 9, 9])).collect();
    let bad = d.join("x.png");
    std::fs::write(&bad, b"nope").unwrap();
    let mut all = pics.clone();
    all.push(bad.clone());
    let out = d.join("d.pdf");
    let engine = engine(&d);
    let report = run(&engine, ImagesToPdfTask::new(all, out.clone(), PageOptions::DEFAULT));
    assert!(report.failures.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].path, bad);
    assert!(out.is_file());
    undo(&engine);
    assert!(!out.exists());
    // An existing output asks first.
    std::fs::write(&out, b"old").unwrap();
    let job = engine.submit(Box::new(ImagesToPdfTask::new(pics, out.clone(), PageOptions::DEFAULT)));
    let (_, events) = finish_with(&engine, job, Some(Decision::Skip));
    assert!(events.iter().any(|e| matches!(e, Event::Conflicts { .. })));
    assert_eq!(std::fs::read(&out).unwrap(), b"old");
}

#[test]
fn split_and_render_with_real_pdfium() {
    let Some(lib) = std::env::var_os("GEZIK_TEST_PDFIUM").map(PathBuf::from) else { return };
    // ImagesToPdfTask (3 pictures) → pdf_chain Split(EachPage) with examples/pdf_worker → 3 PDFs of 1 page
    // (count_pages); pdf_chain Render { dpi: 150, image: Png } → 3 PNGs; undo removes them.
}

#[test]
fn labels_read_well() {
    let a = PathBuf::from("d/a.pdf");
    assert_eq!(pdf_label(&PdfWork::Merge, &[a.clone(), "d/b.pdf".into(), "d/c.pdf".into()]), "Merge 3 PDFs");
    assert_eq!(pdf_label(&PdfWork::Split(Split::EachPage), &[a.clone()]), "Split a.pdf");
    assert_eq!(pdf_label(&PdfWork::Extract("1".into()), &[a.clone()]), "Extract pages from a.pdf");
    assert_eq!(pdf_label(&PdfWork::Render { dpi: 72, image: PageImage::Png }, &[a]), "Save a.pdf as pictures");
    assert_eq!(images_pdf_label(1), "Make PDF from 1 picture");
}
```
  Gövdesi yorumla verilen dört test, yorumdaki adımlarla aynı kalıpta yazılır (girdi betiği, `pdf_chain`, `finish_with`, yorumdaki iddialar).
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-batch --test pdf_tasks` → FAIL.
- [ ] **Step 3: Implement** (`TaskKind::Pdf`, `PlaceTask` genellemesi — `cargo test -p gezik-batch --test extract` eskisi gibi geçmeli — `PdfTask`, `ImagesToPdfTask`).
- [ ] **Step 4: Run.** `cargo test -p gezik-ops -p gezik-batch` → PASS; env'li `split_and_render_with_real_pdfium` de.
- [ ] **Step 5: Commit** "Run PDF work as engine jobs: placed with the conflict list, passwords asked, one undo".

---

### Task 7: pdfium paketleri, MANIFEST ve bulma

**Files:**
- Create: `scripts/tools/prepare-pdfium.ps1`
- Modify: `crates/gezik-core/src/batch/tools.rs` (`Tool::Pdfium`, MANIFEST, test), `crates/gezik-batch/src/tools.rs` (bulma), `crates/gezik-batch/tests/tools.rs` (hazır paket testi), `scripts/tools/{README.md, sources.sha256}`, `THIRD-PARTY.md` ("## pdfium (downloaded on request)")

**Interfaces:**
- Consumes: 5b/5c'nin `DownloadTask`, `install_dir`, `find`.
- Produces: `Tool::Pdfium`; `build_for(Tool::Pdfium, platform)` her platformda; `gezik_batch::tools::find(Tool::Pdfium, data_dir, None) -> Option<PathBuf>` = indirilen kitaplığın yolu (PATH'e ve sisteme bakılmaz).

**Behavior:**
- `gezik-batch::tools`: `folder_name` → `"pdfium"`, `display_name` → `"pdfium"`, `path_names` → `&[]`, `installed_elsewhere` → `None`, `recent_enough(Tool::Pdfium, _)` → `true` (bir kitaplık çalıştırılıp sürümü sorulamaz; sürüm MANIFEST'te sabit ve bağlama özelliğine eşit ya da yeni, özet §3.4). `configured` pdfium için hep `None` (ayarı yok).
- `prepare-pdfium.ps1` (`prepare-ffmpeg.ps1`'in yapısı; parametreler `-Out`, `-Build 8086`, `-Release 1`, `-Cache`, `-UpdateSources`, `-SevenZip`): üst kaynaklar `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F<Build>/pdfium-<p>.tgz`, `p` ∈ `win-x64, win-arm64, mac-arm64, mac-x64, linux-x64, linux-arm64`; bizim adlarımız `pdfium-<Build>-<p>.tgz` (`sources.sha256`'da bu betiğin satırları "pdfium-" ile başlar, başlık `# The pdfium chromium/8086 builds prepare-pdfium.ps1 repackages (addresses in the script): sha256 size name`). Her dosya boyutu ve SHA-256'sıyla `sources.sha256`'ya karşı, ayrıca GitHub `digest`'ine karşı denetlenir (`Get-GitHubDigest "bblanchon/pdfium-binaries" "chromium%2F8086" "pdfium-<p>.tgz"`). Açma: `7z x` iki kez (tgz → tar → dosyalar). Denetimler: `VERSION` içinde `BUILD=<Build>`; `args.gn` içinde `pdf_enable_v8 = false` ve `pdf_enable_xfa = false` (değilse dur). Paket kökü: kitaplık (`pdfium.dll` / `libpdfium.dylib` / `libpdfium.so`; dokunulmaz, macOS imzası korunur), `LICENSE`, `licenses/` (17 dosya; sayı farklıysa dur), `SOURCE.txt` ("PDFium 157.0.8086.0 (chromium/8086) for <platform>, unmodified, from https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F8086 (build scripts: MIT, LICENSE). PDFium: https://pdfium.googlesource.com/pdfium/ (BSD-3-Clause and Apache-2.0, licenses/pdfium.txt); bundled libraries' licences in licenses/. V8 and XFA are off. Upstream file (SHA-256): …. Repackaged for Gezik (https://github.com/wenlar/gezik-tools, release pdfium-8086-1)."). 7z: `a -t7z -mx=9 -ms=on -mmt=1`, Windows x64'te ek `-mf=BCJ` (5c sapması: Gezik'in 7z okuyucusu BCJ2 açmaz); ad `pdfium-8086-<windows-x64|windows-arm64|macos-arm64|macos-x64|linux-x64|linux-arm64>.7z`. Çıktı: MANIFEST satırları (`tool: Tool::Pdfium`, `version: "8086"`, `programs: &["pdfium.dll"]` / `&["libpdfium.dylib"]` / `&["libpdfium.so"]`, `kind: "7z"`), başında üst kaynakların `sha256 size name` yorumları, `<Out>\manifest.rs.txt`.
- Bilinen üst kaynak özetleri (özet §3.2; betik bunları `-UpdateSources` ile yazar, değerler bunlarla aynı çıkmalı):

  | dosya (bizim adımız) | bayt | sha256 |
  |---|---|---|
  | `pdfium-8086-win-x64.tgz` | 3866531 | `1fd8af952832dbb0eb16d9249f68fe09e5f5ebf7c3dd9f6066ea2720cc28487d` |
  | `pdfium-8086-win-arm64.tgz` | 3636496 | `1799b8034e6d64946fec0ae79f3edfc8ba58ccd80c70a46194803b1eba408344` |
  | `pdfium-8086-mac-arm64.tgz` | 3521983 | `e98679e052c07edbb5a627980902abb823d4b3f35744d877bd21668bd9fc13ab` |
  | `pdfium-8086-mac-x64.tgz` | 3717448 | `933a85a138f6027243c56bff8676375c33ceeb767401389415ffc44d689ca85d` |
  | `pdfium-8086-linux-x64.tgz` | 3788766 | `588577cf52dabc1a444988bac841920df54cc2f141801424de97ab04f4fbb935` |
  | `pdfium-8086-linux-arm64.tgz` | 3708875 | `e7e2fe4686925618330103cb167950aca5a84bb00fd977a41b86be59dd1480a2` |

- [ ] **Step 1: Write the failing tests.** `gezik-core` `tools.rs`'e:

```rust
/// pdfium for every platform: one solid 7z from the `pdfium-<build>-<n>` release holding the
/// library at its root; chromium/7881 or newer (the bindings are `pdfium_7881`).
#[test]
fn every_pdfium_build_is_complete() {
    for platform in Platform::ALL {
        let build = build_for(Tool::Pdfium, platform).expect("pdfium for every platform");
        let release = format!("https://github.com/wenlar/gezik-tools/releases/download/pdfium-{}-", build.version);
        assert!(build.url.starts_with(&release) && build.url.ends_with(".7z"), "{}", build.url);
        assert_eq!(build.kind, "7z");
        assert_eq!(build.sha256.len(), 64);
        assert!(build.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(build.size > 1_000_000 && build.size < 8_000_000, "{}", build.size);
        let library = match platform {
            Platform::WindowsX64 | Platform::WindowsArm64 => "pdfium.dll",
            Platform::MacArm64 | Platform::MacX64 => "libpdfium.dylib",
            Platform::LinuxX64 | Platform::LinuxArm64 => "libpdfium.so",
        };
        assert_eq!(build.programs, [library]);
        assert!(build.version.parse::<u32>().unwrap() >= 7881, "{}", build.version);
    }
}
```
  `gezik-batch/src/tools.rs` testine: indirilmiş `pdfium-8086/pdfium.dll` (boş, çalıştırılabilir dosya) bulunur; PATH'te `pdfium.dll` bulunmaz. `tests/tools.rs`'teki `the_prepared_builds_install`'a `Tool::Pdfium => &["LICENSE", "SOURCE.txt", "licenses/pdfium.txt"]` ve bu platformun paketi için: `find(Tool::Pdfium, &data, None) == Some(installed.join(build.programs[0]))` ve `examples/pdf_worker` ile kurulan kitaplıkla üretilmiş 3 sayfalık PDF'in `count_pages`'i `Some(3)` (8086'nın `pdfium_7881` ile yüklendiğinin kanıtı).
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik-core tools` → FAIL (`Tool::Pdfium` yok).
- [ ] **Step 3: Script and packages.** `prepare-pdfium.ps1`'i yaz; `powershell -ExecutionPolicy Bypass -File scripts/tools/prepare-pdfium.ps1 -Out D:\Work\gezik-tools\pdfium-8086-1 -Cache D:\Work\gezik-tools\cache -UpdateSources`; `sources.sha256` satırlarının yukarıdaki tabloyla aynı olduğunu denetle. İkinci kez `-UpdateSources` olmadan, temiz bir `-Out`'a çalıştır; iki çalıştırmanın paketleri `cmp` ile aynı olmalı (tekrarlanabilirlik). Ek doğrulama (`gh` artık var): `gh attestation verify <cache>\pdfium-8086-<p>.tgz --repo bblanchon/pdfium-binaries` altısı için (`gh` dosya adına bakmaz, özete bakar); sonuç notlara.
- [ ] **Step 4: Implement** `Tool::Pdfium`, `manifest.rs.txt`'teki satırları MANIFEST'e yapıştır (yorum: "pdfium chromium/8086 (bblanchon/pdfium-binaries), one solid 7z per platform, repackaged by scripts/tools/prepare-pdfium.ps1"), modül belge yorumundaki "pdfium later"ı düzelt; `gezik-batch` bulma kolları; `README.md`'ye pdfium bölümü (tablo, yapım, yayım komutları); `THIRD-PARTY.md`'ye "## pdfium (downloaded on request)" (7-Zip/ffmpeg bölümlerinin kalıbında: kaynak, lisanslar, `licenses/` kurulumun yanında).
- [ ] **Step 5: Run.** `cargo test -p gezik-core -p gezik-batch` → PASS; `GEZIK_TOOLS_DIR='D:\Work\gezik-tools\pdfium-8086-1' cargo test -p gezik-batch --test tools prepared` → altı paket kurulur, bu makinede `count_pages` 3.
- [ ] **Step 6: Publish.** `"C:\Program Files\GitHub CLI\gh.exe" auth status`; `gh release create pdfium-8086-1 --repo wenlar/gezik-tools --title "pdfium chromium/8086" pdfium-8086-*.7z --notes "PDFium 157.0.8086.0 (chromium/8086), unmodified builds by Benoît Blanchon (https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F8086), without V8 and XFA, repackaged for Gezik. PDFium is under the BSD 3-clause and Apache 2.0 licences (licenses/pdfium.txt); the libraries built into it keep their own permissive licences (licenses/); the build scripts are MIT (LICENSE). Source: https://pdfium.googlesource.com/pdfium/. SOURCE.txt in each file names the upstream file and its SHA-256."`; sonra altı dosyayı `curl -sSfL … | sha256sum` ile indirip MANIFEST'le karşılaştır.
- [ ] **Step 7: Commit** "Prepare pdfium chromium/8086 downloads for every platform and pin their SHA-256".

---

### Task 8: Arayüz — Convert katmanında PDF grubu, sağ tık, pdfium kutusu

**Files:**
- Create: `crates/gezik/src/pdf.rs` (saf yardımcılar ve testleri)
- Modify: `crates/gezik/src/convert.rs`, `crates/gezik/src/archives.rs` (`Need::Pdf`, `tool_offer`, `offer_pdfium`), `crates/gezik/src/context_menu.rs` (`IMAGES_TO_PDF`), `crates/gezik/src/main.rs` (`mod pdf;`), `crates/gezik/ui/widgets/convert.slint`, `crates/gezik/ui/app.slint` (yeni geri çağrılar ve özellikler), `crates/gezik-config/src/settings.rs` (`ConvertState.pdf`)

**Interfaces:**
- Consumes: Task 1 (`PdfOp`, `ops_for`, `is_pdf`, `is_pdf_picture`, `parse_ranges`, `resolve`, `split_file_count`, `extract_pages`, `pictures_pdf_name`, `PageOptions`, `PageSize`, `Margin`, `RENDER_DPIS`, `PageImage`, `Split`), Task 4 (`Worker::this_exe`, `count_pages`), Task 6 (`pdf_chain`, `pdf_label`, `PdfWork`, `PdfTools`, `ImagesToPdfTask`, `images_pdf_label`), Task 7 (`tools::find(Tool::Pdfium, …)`, `build_for`).
- Produces (`crates/gezik/src/pdf.rs`):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfChoices { pub op: PdfOp, pub split: SplitChoice, pub every: u32, pub dpi: u32, pub image: PageImage, pub page: PageOptions }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitChoice { EachPage, Every, Ranges }
impl PdfChoices { pub const DEFAULT: PdfChoices; } // Split EachPage, every 10, 150 dpi, Png, PageOptions::DEFAULT
pub fn choices_text(c: &PdfChoices) -> String;               // "op=split split=every every=10 dpi=150 image=png size=a4 margin=small"
pub fn choices_from(text: &str, base: PdfChoices) -> PdfChoices; // unknown keys and bad values ignored
pub fn pdf_inputs(items: &[(PathBuf, bool)], op: PdfOp) -> (Vec<PathBuf>, usize); // taken, skipped
pub fn skipped_text(count: usize, op: PdfOp) -> String;      // "3 items skipped: not PDFs" / "… not pictures Gezik reads itself"
pub const LOSSES: &str = "Bookmarks, forms and document info are not kept. An encrypted PDF gives an unencrypted copy.";
pub fn files_note(split: &Split, counts: &[Option<u32>]) -> Result<String, String>; // "Makes 14 files", "Makes 20,000 files", "Makes 37 files, more from 1 encrypted PDF"
pub fn extract_note(ranges: &str, counts: &[Option<u32>]) -> Result<String, String>; // "Makes 1 PDF of 11 pages" / per-PDF when several
pub fn move_in_order(order: &mut Vec<PathBuf>, from: usize, to: usize);
```

**Behavior** (spec 9.1, sapma 7, 11, 13-18):
- **Gruplar:** `kinds_for` PDF'leri ve PDF'e girebilen resimleri sayar; `ops_for(resim, pdf)` boş değilse `Kind::Pdf` grubu ("PDF") eklenir — PDF ağırlıklı seçimde en başta, resim ağırlıklı seçimde Image grubundan sonra. `Choice::Pdf(PdfOp)` yeni bir seçim türü; `Choice::key` → `pdf:<id>`; `preset_list` grubun satırlarını `ops_for` sırasıyla `op.label()` ile yazar. `menu_entries` artık PDF'lerde de "Convert…" verir.
- **Katman (ConvertView `kind: 5`):** `pdf-op` (0-4) ve:
  - Images to PDF / Merge: **sıra listesi** (dosya adları, solda ≡ tutamağı; `rename-batch.slint`'teki `drag-from`/`drag-to`/`drop-row`/`reorder` kalıbı birebir; `cv-pdf-reorder(int, int)` → `move_in_order`); başlangıç sırası seçim sırası (dosya listesinin o anki sırası). Images to PDF'te "Page" düğmeleri `Picture size | A4 | Letter`, "Margin" `None | Small | Large`.
  - Split: `Each page | Every [N] pages | Ranges [metin]`; altta `files_note` (sayfa sayısı bilinmiyorsa yalnız sözdizimi denetimi, not boş).
  - PDF to images: `PNG | JPEG`, `72 | 150 | 300 dpi`.
  - Extract: `Pages [metin]`; altta `extract_note`.
  - Merge/Split/Extract'te `LOSSES` notu (ikincil metin rengi).
  - Output bölümü gizli (`show_output: false`); ayak satırında `skipped_text` ve pdfium gerekiyorsa "¹ needs pdfium" (bulunduysa yok).
  - Aralık hataları ayak satırında canlı (`parse_ranges`; sayılar biliniyorsa `resolve`); hata varken Convert hata metnini gösterir ve başlamaz (var olan `set_error` yolu).
- **Yoklama (iş parçacığında, `probe`'a eklenir):** `tools::find(Tool::Pdfium, data, None)`; bulunduysa ve seçimde ≤ 50 PDF varsa her biri için `count_pages(&Worker::this_exe()?, &lib, pdf, stop)` (katman kapanınca `opening` sayacıyla bırakılır); sonuçlar `Layer.pages: Vec<Option<u32>>`'ye.
- **Başlatma:** Images to PDF → `pictures_pdf_name(&order)` → `ImagesToPdfTask` (`submit_chain(vec![task], Some(images_pdf_label(n)), Some(again), After::Select)`). Öteki işlemler: iş parçacığında pdfium bulunur; yoksa `archives.offer_pdfium(hint, again, None)`; varsa `pdf_chain(work, inputs, PdfTools { worker: Worker::this_exe()?, library })` + `pdf_label`. `job_finished`'te bir satır "pdfium could not be loaded" ile düştüyse (bozuk kurulum) kutu "pdfium failed to load" ve Download (yeniden indirme) sunar.
- **Araç kutusu (`archives.rs`):** `Need::Pdf`; `Need::tool()` → `Tool::Pdfium`; `tool_offer(&Need::Pdf, …)`: başlık `"pdfium needed"`; boyut ve indirme açıkken `"PDF tools need pdfium (~{size}, free)."` + `["Download", "Where does it come from?", "Cancel"]` (Linux'ta paket önerisi yok: dağıtımlar pdfium'u paketlemiyor); `hint` (Linux'ta curl/wget yok) → `"PDF tools need pdfium (~{size}, free). {hint} to download it."` + `["OK"]`; yapı yok → `"PDF tools need pdfium. Gezik cannot download it for this system yet."` + `["OK"]`; `[tools] download = false` → `"PDF tools need pdfium. Downloading tools is off ([tools] download = false in settings.toml)."` + `["OK"]`. `offer_pdfium(hint, again, row)` = `self.offer(Need::Pdf, hint, Then::Again { again, row })`. `where_it_comes_from_is_the_release_page` testindeki araç listesine `Tool::Pdfium`.
- **Sağ tık:** `context_menu.rs`'te `pub const IMAGES_TO_PDF: u32 = 611;` (tek kimlikler listesine eklenir; çakışmazlık testi kapsar); `convert::menu_entries` seçimde `is_pdf_picture` olan en az bir resim varsa `(IMAGES_TO_PDF, "Images to PDF…")`'yi "Convert…"tan sonra ekler; seçildiğinde `convert.open_with(rows, Choice::Pdf(PdfOp::ImagesToPdf))` (yeni: `open`'ın istenen seçimle açılan biçimi). Windows'ta Gezik öğeleri Shell menüsüne `menu_items` yoluyla zaten girer.
- **Durum:** `ConvertState` yeni alan `pdf: Option<String>` (`state.toml [convert] pdf`; okuma/yazma `media` gibi, gidiş-dönüş testi `gezik-config`'te); Convert'te `choices_text`, açılışta `choices_from`. `last_preset` `pdf:<id>` olabilir.
- **Kısayollar:** Esc/Ctrl+Enter var olan yönlendirmeyle; sıra listesinde ↑/↓ seçili satırı taşımaz (YAGNI), sürükleme yeter.

- [ ] **Step 1: Write the failing tests** (`crates/gezik/src/pdf.rs` ve `convert.rs`/`archives.rs` testleri):

```rust
#[test]
fn choices_round_trip_through_state() {
    let c = PdfChoices { op: PdfOp::Split, split: SplitChoice::Every, every: 7, dpi: 300, image: PageImage::Jpeg,
        page: PageOptions { size: PageSize::A4, margin: Margin::Small } };
    assert_eq!(choices_from(&choices_text(&c), PdfChoices::DEFAULT), c);
    assert_eq!(choices_from("dpi=999 every=0 nonsense", PdfChoices::DEFAULT), PdfChoices::DEFAULT);
}

#[test]
fn inputs_are_taken_by_operation() {
    let items = vec![file("a.pdf"), file("b.PDF"), file("c.jpg"), file("d.heic"), folder("e")];
    assert_eq!(pdf_inputs(&items, PdfOp::Merge), (vec!["d/a.pdf".into(), "d/b.PDF".into()], 3));
    assert_eq!(pdf_inputs(&items, PdfOp::ImagesToPdf), (vec!["d/c.jpg".into()], 4));
    assert_eq!(skipped_text(4, PdfOp::ImagesToPdf), "4 items skipped: not pictures Gezik reads itself");
    assert_eq!(skipped_text(1, PdfOp::Split), "1 item skipped: not a PDF");
}

#[test]
fn the_layer_says_how_many_files_come_out() {
    assert_eq!(files_note(&Split::EachPage, &[Some(20_000)]).unwrap(), "Makes 20,000 files");
    assert_eq!(files_note(&Split::Every(5), &[Some(14), None]).unwrap(), "Makes 3 files, more from 1 encrypted PDF");
    assert_eq!(files_note(&Split::Ranges("1-99".into()), &[Some(14)]).unwrap_err(), "Page 99 is past the end (14 pages)");
    assert_eq!(files_note(&Split::EachPage, &[None]).unwrap(), "");
    assert_eq!(extract_note("1-3, 5", &[Some(14)]).unwrap(), "Makes 1 PDF of 4 pages");
}

#[test]
fn the_order_follows_the_drag() {
    let mut order: Vec<PathBuf> = ["a", "b", "c"].iter().map(PathBuf::from).collect();
    move_in_order(&mut order, 0, 2);
    assert_eq!(order, ["b", "c", "a"].iter().map(PathBuf::from).collect::<Vec<_>>());
}
```
  `convert.rs`'e: PDF seçiminde "Convert…" ve PDF grubu (`preset_list` satırları: `Heading("PDF")`, "Split PDF", "PDF to images", "Extract pages"); iki PDF'te "Merge PDFs" de; resim + PDF karışık seçimde iki grup; resim seçiminde menüde `(IMAGES_TO_PDF, "Images to PDF…")`, HEIC-tek seçimde yok. `archives.rs`'in `one_box_offers_each_tool`'una:

```rust
let (title, message, buttons) = tool_offer(&Need::Pdf, Some(2_900_000), true, true, None);
assert_eq!(title, "pdfium needed");
assert_eq!(message, "PDF tools need pdfium (~2.8 MB, free).");
assert_eq!(buttons, ["Download", "Where does it come from?", "Cancel"]);
let (_, off, buttons) = tool_offer(&Need::Pdf, Some(2_900_000), false, false, None);
assert!(off.contains("[tools] download = false") && buttons == ["OK"], "{off}");
```
  (`format_size` 1024 tabanlı: 2.900.000 bayt "2.8 MB", 5c testindeki 31.000.000 → "29.6 MB" gibi.) `gezik-config`: `[convert] pdf` gidiş-dönüş.
- [ ] **Step 2: Run to see them fail.** `cargo test -p gezik -p gezik-config` → FAIL.
- [ ] **Step 3: Implement** (Slint: `ConvertView`'e `pdf-op`, `pdf-order: [string]`, `pdf-page`, `pdf-margin`, `pdf-split`, `pdf-every`, `pdf-ranges`, `pdf-dpi`, `pdf-image`, `pdf-note`, `pdf-losses`; geri çağrılar `cv-pdf-set(string, int)`, `cv-pdf-reorder(int, int)`, `cv-pdf-drop-row(length, length, int) -> int`, alan metinleri için var olan `cv-edited`).
- [ ] **Step 4: Run.** `cargo test --workspace` → PASS; çapraz denetimler; `cargo run -p gezik` ile kısa bakış (PDF grubu, sıra listesi sürükleme, pdfium kutusu).
- [ ] **Step 5: Commit** "Merge, split and convert PDFs from the Convert layer, make PDFs from pictures, and offer to download pdfium".

---

### Task 9: Ölçüm, Linux kabı, notlar, spec

**Files:**
- Create: `crates/gezik-batch/examples/pdf_bench.rs`, `scripts/perf/pdf.ps1`
- Modify: `docs/superpowers/notes/2026-10-03-ayarlar-ve-tema-followups.md` ("## Alt proje 5d (PDF) sonrası"), `docs/superpowers/specs/2026-10-05-toplu-islemler-design.md` (Durum ve bölüm 8.1/8.2/9'daki pdfium satırları), `scripts/tools/README.md` (gerekiyorsa)

- [ ] **Step 1: Bench.** `pdf_bench.rs` (5c `convert_bench.rs` kalıbı, motorla): 100 adet 12 MP JPEG üret → `ImagesToPdfTask` → süre, çıktı boyutu, süreç tepe belleği (Windows'ta `GetProcessMemoryInfo` `PeakWorkingSetSize`, özetteki `mem.rs` gibi; Unix'te `getrusage` `ru_maxrss`); 5 adet 12 MP PNG → süre. `GEZIK_TEST_PDFIUM` varsa: çıkan 100 sayfalık PDF'i "Each page" ile böl, 150 dpi PNG'ye çevir (süre, işçinin tepe belleği yok — işçi ayrı süreç; yalnız süre). `scripts/perf/pdf.ps1` 5c `convert.ps1` gibi (try/finally ile temizlik). Hedef: özetteki ölçüme yakın (100 × 12 MP ~0,4 s G/Ç, tepe < 30 MB).
- [ ] **Step 2: Boşta bellek ve boşalma.** Sürüm derlemesinde Gezik'i aç, bir PDF böl, iş bitince Görev Yöneticisi'nde `gezik.exe --pdf-worker` süreci kalmadığını ve Gezik'in boşta belleğinin işten öncekiyle aynı (≤ 7 MB, spec 1) olduğunu yaz.
- [ ] **Step 3: Exe boyutu.** `cargo build --release -p gezik`; master `22f245b` ile (ayrı çalışma ağacı ve hedef klasörü) karşılaştır; 5c'deki gibi dağılım: PDF kodu boş bırakılmış bir derlemeyle kütüphane payı (img2pdf + pdfium-render; tahmin ~0,73 MB) ve kalan katman/denetleyici payı. libloading iki kopya kaldıysa (`cargo tree -i libloading`) payı.
- [ ] **Step 4: Linux kabı.** `docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/test.sh unit` → hepsi geçer (sahte işçi testleri dahil; gerçek pdfium testleri atlanır). Sonra Task 7'nin `pdfium-8086-linux-x64.7z` paketini kaba açıp (`/tmp/pdfium`) `-e GEZIK_TEST_PDFIUM=/tmp/pdfium/libpdfium.so` ile `cargo test -p gezik-batch --test pdf_worker --test pdf_tasks` (glibc 2.16 isteği, yazı tipi yolları; özet §3.3, §4.4); sonuçlar notlara.
- [ ] **Step 5: Notlar** (Türkçe, 5c bölümünün kalıbında): ölçümler; exe boyutu ve dağılımı; sapmalar (yukarıdaki 1-20 kısaca); `gh attestation verify` sonuçları; yayınlanan sürüm adresi `https://github.com/wenlar/gezik-tools/releases/tag/pdfium-8086-1`; Windows ekran testi sonuçları (aşağıdaki liste); denenemeyenler (macOS ekranı ve `dlopen`'ın hardened runtime ile durumu, özet §3.3; Linux masaüstünde gömülü olmayan CJK yazı tipleri; arm64 paketleri gerçek donanımda; bulanıklaştırma yalnız özetteki 400 dosya). Spec: Durum satırı "Tasarım onaylandı (2026-10-05); 5a, 5b, 5c, 5d uygulandı"; 8.1'de pdfium "~3 MB indirme, ~8 MB disk"; 8.2'de etiket `pdfium-8086-1`; 9.1'de "iş başında yükler, iş bitince bırakır" satırına "yardımcı süreçte (`gezik --pdf-worker`)" eki.
- [ ] **Step 6: Commit** "Measure PDF work and note what 5d does".

---

## Ekran testleri (Windows, alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici; pdfium başta **kurulu değil**; test verisi `%TEMP%\gezik-gui-5d\`: 6 JPEG (biri EXIF yönü 6, biri CMYK Adobe — Task 3 test yardımcılarıyla), 1 saydam PNG, 1 HEIC, `a.pdf` (3 sayfa) ve `b.pdf` (2 sayfa) Gezik'in Images to PDF'iyle yapılmış, `enc_aes256.pdf` (Task 5 örneği, parola `pw`), `rapor ş.pdf` (Türkçe ad), 14.400 pt'lik tek sayfalı `huge.pdf` (Task 5 testindeki gibi), 20.000 sayfalı `many.pdf` (özet §4.3'teki gibi, `pdf_writer` ile boş sayfalar).

1. 6 JPEG + PNG + HEIC seç → sağ tık "Images to PDF…" → katman "Images to PDF" seçili açılır, ayakta "1 item skipped: not a picture Gezik reads itself"; sıra listesinde sürükleyerek son resmi başa al; A4 + Small → Convert → `gezik-gui-5d.pdf` oluşur; bir PDF görüntüleyicide sayfa sırası değiştirilen sıra, EXIF 6'lı resim dik, CMYK renkleri doğru, yatay resimler yatay sayfada. Ctrl+Z dosyayı çöpe atar.
2. `a.pdf` + `b.pdf` seç → Convert… → PDF grubunda "Merge PDFs" → katmanda kayıp notu görünür, ayakta "¹ needs pdfium" → Convert → "pdfium needed" kutusu ("PDF tools need pdfium (~… MB, free).") → "Where does it come from?" sürüm sayfasını açar, kutu geri gelir → Download → panelde indirme → birleştirme kendiliğinden sürer → `a (merged).pdf` 5 sayfa.
3. `a.pdf` → Split → "Each page" → katman "Makes 3 files" der → 3 dosya; aynı işi yeniden çalıştır → çakışma listesi; Skip → eskiler kalır; Ctrl+Z yalnız yenileri çöpe atar.
4. `many.pdf` → Split → "Each page" → katman "Makes 20,000 files" der (çalıştırmadan önce); Cancel ile kapat.
5. `a.pdf` → Split → "Ranges" `1-2, 3` → `a - pages 1-2.pdf`, `a - page 3.pdf`; `1-99` yazınca ayakta "Page 99 is past the end (3 pages)", Convert başlamaz.
6. `rapor ş.pdf` → Extract pages `2-` → `rapor ş - page 2.pdf`.
7. `a.pdf` → PDF to images, PNG 150 dpi → 3 PNG; JPEG 300 dpi → 3 JPG, özelliklerde 300 dpi.
8. `huge.pdf` → PDF to images 300 dpi → iş biter, panelde not satırı "page 1 was made at … dpi: at 300 dpi it would be too large"; Görev Yöneticisi'nde işçinin belleği ~1 GB'ı aşmaz.
9. `enc_aes256.pdf` → Split → parola sorusu ("enc_aes256.pdf is encrypted. Password:") → `nope` → "Wrong password. Try again:" → `pw` → 3 dosya, şifresiz açılıyor; aynı işte Skip → not satırı "no password given".
10. 20.000 sayfalık bölmeyi başlat → panelde sayfa ilerlemesi akıcı → Pause → Görev Yöneticisi'nde `gezik.exe --pdf-worker` yok → Resume → baştan sürer → Cancel → yarım dosya ve aşama klasörü kalmaz.
11. İş bitince `gezik.exe --pdf-worker` süreci kalmaz; Gezik'in boşta belleği işten öncekiyle aynı.
12. Esc/Ctrl+Enter; son seçimler (işlem, DPI, biçim, sayfa boyutu, kenar boşluğu) katman yeniden açılınca geri gelir; aralık metni gelmez.

---

## Self-review

- **Spec kapsamı:** 9.1 beş işlem → Task 3 + 5 + 6 + 8; "PDF" grubu ve sağ tık → Task 8; yükle/bırak (süreçle) → Task 4-5, sapma 3; şifre → Task 5-6 (5.2'nin soru katmanı, `Question::Password`); `Created` + Ctrl+Z → Task 6; ad önerileri → Task 1; `Resources` → Task 6 (`Work::Cpu`, tek işçi). 8 (araç indirme, SHA-256, sürüm sayfası, `[tools] download`) → Task 7-8. 3.2 `TaskKind::Pdf` → Task 6. 10.2 son seçimler → Task 8. 11 ölçüm, exe büyümesi → Task 9. 12.1 saf testler → Task 1-2; 12.2 "Images to PDF içeride; pdfium testleri `GEZIK_TEST_PDFIUM`" → Task 3, 5, 6; 12.3 Linux kabı → Task 9; 12.4 elle → ekran listesi.
- **Yer tutucu taraması:** gövdesi yorumla verilen beş test (Task 5'te bir, Task 6'da dört) adımları ve iddiaları açıkça yazar; MANIFEST satırları betik çıktısından yapıştırılır (5c Task 8 gibi; değerler çalıştırmadan bilinemez, üst kaynak özetleri tabloda sabit).
- **Tip tutarlılığı:** `Split`, `PageImage`, `PageOptions` Task 1'de; `Request`/`Reply`/`WorkerJob`/`Failure` Task 2'de; `Worker`, `Ended`, `run`, `count_pages`, `WORKER_ARG` Task 4'te; `PdfWork`, `PdfTools`, `pdf_chain`, `pdf_label`, `ImagesToPdfTask`, `images_pdf_label`, `PlaceTask::outputs` Task 6'da; `Tool::Pdfium` Task 7'de; arayüz yalnız bunları kullanır.
- **Review Focus:** beş maddenin her biri adı geçen görevde test olarak var (1: Task 4, 5, 6; 2: Task 4, 6; 3: Task 1, 5, 6; 4: Task 2, 5; 5: Task 6).
