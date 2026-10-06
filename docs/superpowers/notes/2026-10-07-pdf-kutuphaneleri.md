# PDF (5d) kütüphane özeti

Tarih: 2026-10-07 (ölçümler 2026-10-06). Araç zinciri: rustc/cargo 1.99.0, x86_64-pc-windows-msvc, edition 2024.
Deneme crate'i: `scratchpad/pdfprobe` (işlev denemeleri) ve `scratchpad/pdfprobe/sizeprobe` (exe boyutu, Gezik'in release profili).
pdfium: `scratchpad/pdfium/` (bblanchon/pdfium-binaries `chromium/8086`, karşılaştırma için `chromium/7881` ve `chromium/6996` Windows x64). Yapı denetimi: qpdf 12.4.2 (`qpdf --check`, scratchpad'e indirildi).

Aşağıdaki her Rust parçası derlenip çalışan deneme kodundan alındı; her komut çalıştırıldı ve çıktısı aktarıldı. Denemeler yalnız Windows x64'te çalıştı; macOS ve Linux ikilileri yalnız dosya düzeyinde incelendi (imza, bağımlılıklar, glibc).

Yeniden çalıştırma:
```
cd pdfprobe
cargo build --release --bins
target/release/img2pdf                      # out/*.pdf + 100 x 12 MP ölçümü (img2pdf.out.txt)
target/release/verify ../pdfium/win-x64/bin # pdfium ile açar, işler, image crate'iyle karşılaştırır (verify.out.txt)
target/release/gen                          # uç durum PDF'leri (dev sayfa, 20000 sayfa, yazı tipi, bozuk, yer imi)
qpdf --encrypt ...                          # şifreli örnekler, komutlar 2.5'te
target/release/ops ../pdfium/win-x64/bin    # birleştirme, bölme, ayıklama, işleme, şifre, yeniden yükleme (ops.out.txt)
target/release/rawload ../pdfium/win-x64/bin   # libloading ile boşaltıp yeniden yükleme (rawload.out.txt)
python fuzz.py 400                          # 400 bozulmuş PDF, her biri ayrı süreçte
cd sizeprobe && cargo build --release --features "<aşama>"   # bölüm 2.8
```
Dosyalar (`docs/superpowers/notes/pdf-probe/`): `jpeg.rs` (JPEG başlık okuyucu), `img2pdf.rs` (akışlı PDF yazıcı), `pdfops.rs` (pdfium işlemleri, aralık ayrıştırıcı), `mem.rs` (Windows bellek ölçümü), `bin_*.rs` (`src/bin/*.rs`), `sizeprobe.rs`, `fuzz.py`, `pe.py` (PE dışa aktarım listesi), `inspect_bin.py` (ELF glibc ve Mach-O imza/bağımlılık okuyucu), `*.out.txt` (çıktılar), `Cargo.toml.txt`, `sizeprobe.Cargo.toml.txt`. Örnekler: 5c denemesinin `samples/` klasörü (`DSCN0010.jpg`, `landscape_6.jpg` EXIF 6 + "Generic RGB" ICC, `all.jpg` EXIF 6 + Display P3 ICC, `cmyk.jpg` Adobe APP14), ImageMagick ile yapılan `gray.jpg`, `progressive.jpg`, `rgba_icc.png`, `orient/o1..o8.jpg` (aynı pikseller, EXIF yönü 1-8), `big12mp.jpg` (4000x3000, 3,8 MB), `big12mp.png` (16,5 MB), `screen.png`.

---

## 0. Cargo satırları (denendi)

```toml
# gezik-batch
pdf-writer = "0.15.0"
miniz_oxide = "0.9.1"          # flate2 1.1.10 zaten bunu kullanıyor: Cargo.lock'a yeni sürüm girmez
pdfium-render = { version = "0.9.4", default-features = false, features = ["pdfium_7881"] }
```
- Sürümler ve lisanslar (crates.io API, 2026-10-06):
  - `pdf-writer` 0.15.0 (2026-05-27), MIT OR Apache-2.0, MSRV belirtilmemiş. Bağımlılıkları bitflags 2, itoa, memchr, ryu. Gezik'e yalnız `pdf-writer` ve `ryu` yeni girer.
  - `miniz_oxide` 0.9.1 (2026-03-13), MIT OR Zlib OR Apache-2.0. Cargo.lock'ta zaten var (`flate2 1.1.10 -> miniz_oxide 0.9.1`; `png 0.18.1` ise 0.8.9 kullanıyor).
  - `pdfium-render` 0.9.4 (2026-09-06), MIT OR Apache-2.0, MSRV 1.61. Varsayılan özellikler `pdfium_latest` (= `pdfium_7881`), `image_latest` (image 0.25) ve `thread_safe`. Üçü de kapatılır, yalnız `pdfium_7881` açılır (gerekçe 2.1 ve 2.7).
- pdfium-render'ın getirdiği yeni crate'ler: `maybe-owned 0.3.4`, `utf16string 0.2.0`, `vecmath 1.0.0`, `piston-float 1.0.1` ve **`libloading 0.9.0`**. Gezik'te zaten `libloading 0.8.9` var; pdfium-render `libloading = "0"` istediği için ikinci kopya gelir. `cargo update -p libloading@0.9.0 --precise 0.8.9` ile tek kopyaya iner; denendi, pdfium-render 0.9.4 0.8.9 ile derleniyor. Zaten var olanlar: bitflags, bytemuck 1.25.2, bytes 1.12.1, chrono 0.4.45, itertools 0.15.0, log, once_cell.
- `image`, `jpeg-encoder` ve `kamadak-exif`, 5c'deki satırlarıyla aynen kullanılır. PNG/diğer resimleri image çözer, PDF→JPEG'i jpeg-encoder 0.7.1 yazar, PDF→PNG'yi image'in PNG kodlayıcısı yazar.

---

## 1. Resim → PDF: pdf-writer 0.15.0 + miniz_oxide 0.9.1

### 1.1 Bellek: dosyaya akışlı yazım
`pdf_writer::Pdf` dosyanın tamamını `finish()` çağrılana dek tek bir `Vec<u8>`'de tutar. 100 fotoğraf için bu, bütün JPEG'lerin toplamı kadar RAM demek (ölçümde 384 MB). Bu yüzden her dolaylı nesne kendi `Chunk`'ına yazılıp hemen dosyaya gider. Saklanan tek şey nesne başına `(id, offset)` çiftidir. xref tablosu ile trailer'ı 20 satırlık kod yazar:
```rust
/// Writes one chunk that holds exactly one indirect object.
fn put(&mut self, id: Ref, chunk: Chunk) -> std::io::Result<()> {
    self.offsets.push((id.get(), self.pos));
    let bytes = chunk.as_bytes();
    self.out.write_all(bytes)?;
    self.pos += bytes.len() as u64;
    Ok(())
}
// finish(): Pages ağacı (Kids = sayfa ref'leri), Catalog, Info, sonra:
let mut t = format!("xref\n0 {size}\n0000000000 65535 f \n");
for (_, off) in &self.offsets {
    t.push_str(&format!("{off:010} 00000 n \n"));   // her satır tam 20 bayt
}
t.push_str(&format!("trailer\n<< /Size {size} /Root {} 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n", ...));
```
- Sayfa ağacının ve katalogun id'leri baştan ayrılır (1 ve 2), ama nesneler en sona yazılır. Her sayfa `/Parent 2 0 R` gösterir.
- `Chunk` üzerinde `catalog()` yok, `pdf.catalog()` yalnız `Pdf`'te var. Bunun yerine `c.indirect(id).start::<pdf_writer::writers::Catalog>().pages(tree)` kullanılır.
- Ölçüm (`bench100`: aynı 3,8 MB'lık 12 MP JPEG 100 kez, A4 sayfa):
```
bench100.pdf: 384500739 B in 377.8864ms; working set 5.0 MiB (peak 16.1), private 1.2 MiB (peak 17.0)
```
  Çıktı boyutu girişlerin toplamı artı yaklaşık 2 KB'dır (JPEG'ler aynen gömülür). Süre neredeyse tümüyle G/Ç'dir (dosya önbellekteyken 0,4 s). Tepe bellek 17 MB, yani bir JPEG artı 1 MB'lık BufWriter. `qpdf --check`: "No syntax or stream encoding errors", `--show-npages` 100.

### 1.2 JPEG: DCTDecode ile aynen geçirme
Piksel çözülmez. `jpeg.rs` işaretçileri (marker) SOS'a kadar okur: SOFn (boyut, bileşen sayısı, bit), APP14 Adobe, APP1 Exif (yön), APP2 ICC_PROFILE (parçalar sıra numarasına göre birleştirilir) ve JFIF yoğunluğu.
```rust
let mut c = Chunk::new();
let mut x = c.image_xobject(img_ref, &data);       // dosyanın baytları, değişmeden
x.filter(Filter::DctDecode);
x.width(info.width as i32);
x.height(info.height as i32);
x.bits_per_component(8);
match (icc_ref, info.components) {
    (Some(r), _) => x.color_space().icc_based(r),
    (None, 1) => x.color_space().device_gray(),
    (None, 3) => x.color_space().device_rgb(),
    (None, _) => x.color_space().device_cmyk(),
}
if info.components == 4 && info.adobe.is_some() {
    // Adobe/Photoshop CMYK JPEGs store inverted ink values.
    x.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
}
x.finish();
```
- Aynen geçirme şu koşulda yapılır: `bits == 8`, SOF0/1/2 (baseline, extended, **progressive**) ve 1, 3 ya da 4 bileşen. Aritmetik kodlu (SOF9 ve sonrası), lossless (SOF3) ve 12 bitlik JPEG'ler image ile çözülüp Flate yoluna gider. Bunlar nadirdir ve PDF okuyucularının çoğu onları desteklemez.
- **Gri** (1 bileşen) → DeviceGray. **Progressive** sorunsuz.
- **CMYK:** `cmyk.jpg` dosyasında APP14 Adobe transform=2 (YCCK) var. Decode dizisiyle pdfium çıktısı image crate'inin çözümüne yakın (fark 15,93). Decode dizisi olmadan renkler ters çıkıyor (fark 102,76, bkz. 1.6). Kural: APP14 Adobe varsa ters çevrilmiş CMYK kabul edilir. APP14'süz CMYK JPEG pratikte yok denecek kadar azdır; Decode yazılmaz. Profilsiz CMYK→RGB dönüşümü okuyucuya göre değişir. Kalan 15,9'luk fark da bundan gelir (pdfium ile zune-jpeg farklı formül kullanır).
- **ICC:** APP2 profili `ICCBased` renk uzayı olarak gömülür (`/N` = bileşen sayısı, `/Alternate` Device*, akış Flate ile sıkıştırılır):
```rust
let z = compress_to_vec_zlib(icc, level);
let mut p = c.icc_profile(r, &z);
p.n(n);
p.filter(Filter::FlateDecode);
match n { 1 => p.alternate().device_gray(), 3 => p.alternate().device_rgb(), _ => p.alternate().device_cmyk() }
```
  pdfium profili uyguluyor: profil gömülünce fark landscape_6 için 12,27, all.jpg (Display P3) için 5,89. Profil gömülmeyince farklar 0,29 ve 0,17 (`noicc.pdf`). Yani fark doğru renk dönüşümünden geliyor; image crate ICC uygulamaz. Uygulamada profilin başlıktaki renk uzayı (16. bayttan itibaren `RGB `, `GRAY`, `CMYK`) bileşen sayısıyla uyuşmuyorsa profil atlanmalı. Bu denetim denemede yok.
- JFIF yoğunluğu "resim boyutu" sayfasında ölçek olarak kullanılabilir (`use_file_dpi`). ImageMagick'in yazdığı örnekler 300 dpi diyor ve 640 px sayfa 153,6 pt çıkıyor. Telefon fotoğrafları genelde 72 dpi der ya da hiç demez. Öneri: yoğunluk varsa kullan, yoksa 1 px = 1 pt (72 dpi).

### 1.3 EXIF yönü: resim dönüştürülmez, sayfa döndürülmez, dönüşüm matrisi kullanılır
JPEG'i yeniden kodlamamak için pikseller döndürülmez. `/Rotate` de kullanılmaz: sayfa döner, ama aynalı yönleri (2, 4, 5, 7) karşılamaz ve kenar boşluğu ile A4 yerleşimi karışır. Saklanan resim birim kareye bir yön matrisiyle çizilir, sonra yerleşim matrisiyle ölçeklenir. Sayfa ve yerleşim, döndükten sonraki görünen boyutla (5-8'de genişlik ile yükseklik yer değiştirir) hesaplanır:
```rust
pub fn orient_matrix(o: Orientation) -> [f32; 6] {
    match o {
        Orientation::NoTransforms => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        Orientation::FlipHorizontal => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0], // EXIF 2
        Orientation::Rotate180 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],     // EXIF 3
        Orientation::FlipVertical => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],   // EXIF 4
        Orientation::Rotate90FlipH => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0], // EXIF 5 (transpose)
        Orientation::Rotate90 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],       // EXIF 6 (90 cw)
        Orientation::Rotate270FlipH => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],  // EXIF 7 (transverse)
        Orientation::Rotate270 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],      // EXIF 8 (90 ccw)
    }
}
// içerik akışı: q  [w 0 0 h x y] cm  [orient] cm  /Im0 Do  Q
content.save_state();
content.transform([w, 0.0, 0.0, h, x, y]);
content.transform(orient_matrix(orient));
content.x_object(Name(b"Im0"));
content.restore_state();
```
Yön, image'in `Orientation::from_exif_chunk(&app1[6..])` işleviyle okunur (5c'de anlatıldı). Doğrulama: `orient/o1..o8.jpg` aynı pikselleri taşıyor, yalnız EXIF etiketi farklı. Her biri pdfium ile işlenip `image` + `apply_orientation` sonucuyla karşılaştırıldı:
```
out/orient_o1.pdf p1: page 320.0x240.0 pt, render 320x240, ref 320x240 (samples/orient/o1.jpg), mean |diff| = 0.16
...                                                                     (o2, o3, o4 aynı: 0.16)
out/orient_o5.pdf p1: page 240.0x320.0 pt, render 240x320, ref 240x320 (samples/orient/o5.jpg), mean |diff| = 0.16
...                                                                     (o6, o7, o8 aynı: 0.16)
```
Yanlış bir matris bu fotoğrafta onlarca birimlik fark verir (karşılaştırma için ters CMYK 102). Sekiz yönün hepsi 0,16'da. Gözle de denetlendi: `merged-72-png - page 3.png` (landscape_6, EXIF 6) dik ve yazılar okunuyor.

### 1.4 PNG ve diğerleri: Flate, SMask, 16 bit, ICC
Resim image ile çözülür (5c'deki decode yolu), `apply_orientation` uygulanır, örnekler renk ve alfa düzlemlerine ayrılır:
- Gri ya da renkli, 8 ya da 16 bit: `L8/La8` → DeviceGray, `Rgb8/Rgba8` → DeviceRGB. 16 bitlik resimler `BitsPerComponent 16` olarak gömülür; örnekler **big-endian** olmalı (`to_be_bytes`). Fark rgb16 için 0,16, rgba16 için 0,03, yani pdfium 16 biti doğru okuyor.
- Alfa, ayrı bir `/SMask` resmi olarak yazılır (DeviceGray, aynı bit derinliği). Alfanın tamamı opaksa SMask yazılmaz. rgba.png beyaz zemin üzerine bindirildiğinde fark 0,00.
- PNG `iCCP` profili, JPEG'deki gibi `ICCBased` olur (`decoder.icc_profile()`).
- Sıkıştırma: Flate'ten önce satır başına PNG Paeth süzgeci uygulanır ve `DecodeParms << /Predictor 15 /Colors n /BitsPerComponent b /Columns w >>` yazılır:
```rust
x.filter(Filter::FlateDecode);
x.decode_parms().predictor(pdf_writer::types::Predictor::PngOptimum).colors(n).bits_per_component(bpc).columns(w as i32);
```
```
12 MP PNG + screenshot, predictor=false level=6: 30323251 B in 732ms
12 MP PNG + screenshot, predictor=true  level=1: 19215940 B in 408ms
12 MP PNG + screenshot, predictor=true  level=9: 18917095 B in 2.27s
bench_png.pdf (5 x 12 MP PNG, Paeth+Flate 6): 94089246 B in 12.9s
```
  Öneri: Paeth + miniz düzey 1-3. Düzey 6, gürültülü 12 MP bir resimde 2,5 s sürüyor ve yalnız %1 kazandırıyor. Bu yol bir resmin tamamını bellekte açar: 12 MP RGBA8 48 MB, RGBA16 96 MB. 5c'deki 512 MiB sınırı burada da geçerlidir.
- Tüm doğrulama çıktısı (`verify.out.txt`): DSCN0010 0,17, progressive 0,18, gray.jpg 0,01, gray.png 0,00, rgba_icc.png 0,00.

### 1.5 Sayfa boyutu ve kenar boşluğu
- **Resim boyutu:** sayfa = görünen px × 72 / dpi + 2 × kenar boşluğu. Acrobat'ın sayfa sınırı 14400 pt (200 inç). Bunu aşan resim orantılı küçültülür (`UserUnit` kullanılmaz; pdfium onu yok sayıyor, bkz. 4).
- **A4** 595,276 × 841,89 pt, **Letter** 612 × 792 pt. Resim, kenar boşlukları içine en-boy oranı korunarak sığdırılır ve ortalanır. Yatay resim yatay sayfaya konur (spec bunu söylemiyor, öneridir):
```
out/three_a4.pdf: 3 pages, first sizes ["841.89x595.28", "595.28x841.89", "841.89x595.28"]
out/three_letter.pdf: 3 pages, first sizes ["792.00x612.00", "612.00x792.00", "792.00x612.00"]
```

### 1.6 İstenen 3 sayfalık deneme ve doğrulama
`out/three.pdf` = DSCN0010.jpg (JPEG), rgba.png (alfalı PNG), landscape_6.jpg (EXIF 6 + ICC):
```
  samples/DSCN0010.jpg   jpeg passthrough sof=0xc0 comps=3 adobe=None icc=0 orient=NoTransforms
  samples/rgba.png       flate Rgba8 -> 8 bpc n=3 smask=true icc=0 (196608 -> 1705 B)
  samples/landscape_6.jpg jpeg passthrough sof=0xc0 comps=3 adobe=None icc=1960 orient=Rotate90
out/three.pdf: 314846 B
```
`qpdf --check out/three.pdf`: "No syntax or stream encoding errors found". pdfium ile açıldı, 3 sayfa, her sayfa işlendi:
```
out/three.pdf p1: page 640.0x480.0 pt, render 640x480, ref 640x480 (samples/DSCN0010.jpg), mean |diff| = 0.17
out/three.pdf p2: page 256.0x256.0 pt, render 256x256, ref 256x256 (samples/rgba.png), mean |diff| = 0.00
out/three.pdf p3: page 600.0x450.0 pt, render 600x450, ref 600x450 (samples/landscape_6.jpg), mean |diff| = 12.27   (ICC uygulandı; profilsiz 0.29)
out/extra.pdf p1: ... (samples/cmyk.jpg), mean |diff| = 15.93
out/cmyk_nodecode.pdf p1: ... (samples/cmyk.jpg), mean |diff| = 102.76      (Decode dizisi olmadan: ters)
```
`extra.pdf` (cmyk, gray.jpg, progressive, all.jpg, rgba16, rgb16, gray.png, rgba_icc), A4/Letter sürümleri, bench PDF'leri ve sekiz yön dosyası da `qpdf --check`'ten hatasız geçti.

### 1.7 Exe boyutu
| aşama (`sizeprobe`) | exe bayt | fark |
|---|---|---|
| taban (`image::open().to_rgba8()`, png/jpeg/gif/webp/bmp) | 794,112 | – |
| + img2pdf (pdf-writer, miniz deflate, jpeg.rs, 8/16 bit, SMask, Paeth) | 1,094,656 | **+300,544** |

Bu farkın bir kısmı miniz_oxide'ın sıkıştırıcısıdır. Gezik'te bu sıkıştırıcı zip yazımı için flate2 üzerinden zaten bağlı, bu yüzden gerçek artış daha azdır.

---

## 2. pdfium-render 0.9.4

### 2.1 Dinamik yükleme
```rust
pub fn bind(dir: &Path) -> Result<Pdfium, PdfiumError> {
    let lib = Pdfium::pdfium_platform_library_name_at_path(dir); // pdfium.dll / libpdfium.dylib / libpdfium.so
    Ok(Pdfium::new(Pdfium::bind_to_library(lib)?))
}
```
```
[mem] start: working set 4.2 MiB (peak 4.2), private 0.6 MiB (peak 0.6)
bind + FPDF_InitLibrary: 4.3609ms
[mem] after load: working set 8.0 MiB (peak 8.0), private 1.3 MiB (peak 1.3)
```
- `bind_to_library`, `DynamicPdfiumBindings::new` içinde **bütün** API işlevlerini (7881 için 460'tan fazla) hemen `GetProcAddress` ile arar. Biri eksikse yükleme hata verir (bkz. 3.4).
- Özellikler: `static` yok (dinamik varsayılan). `image_*` kapalı: `PdfBitmap::as_image()` gelmez, `as_rgba_bytes()` yeter. `thread_safe` kapalı (bkz. 2.7).

### 2.2 Birleştirme
```rust
pub fn merge(p: &Pdfium, inputs: &[&Path], out: &Path) -> Result<i32, PdfiumError> {
    let mut dst = p.create_new_pdf()?;
    for path in inputs {
        let src = p.load_pdf_from_file(path, None)?;
        dst.pages_mut().append(&src)?;   // = copy_page_range_from_document(src, 0..=n-1, len) -> FPDF_ImportPagesByIndex
        // `src` drops here: FPDF_CloseDocument; the imported pages stay valid in `dst`.
    }
    dst.save_to_file(out)?;              // FPDF_SaveAsCopy, flags 0 (tam yeniden yazım)
    Ok(dst.pages().len())
}
```
```
merge 3+8+3 -> out/merged.pdf: 14 pages in 7.2963ms        (qpdf --check: hata yok; 1,272,102 B = girişlerin toplamı)
```
- 0.9'da `PdfPageIndex` `i32`'dir (eski belgelerdeki `u16` değil).
- **Kaybolanlar** (`outline.pdf`, 2 yer imi ve Title içerir): birleştirilmiş dosyada `outlines` 0 çıktı. Info sözlüğü `<< /Creator (PDFium) /Producer (PDFium) /CreationDate ... >>` ile değişti, Title gitti. `FPDF_ImportPages` yalnız sayfaları ve onların kaynaklarını kopyalar; yer imleri, adlandırılmış hedefler, AcroForm ve belge meta verisi kopyalanmaz. Spec bu konuda bir şey söylemiyor; katmanda belirtilmesi önerilir.

### 2.3 Bölme ve ayıklama
Aralıklar Gezik'in sözdizimiyle Rust'ta ayrıştırılır (pdfium'un `FPDF_ImportPages` dize sözdizimi `8-` gibi açık uçları bilmez). Ardışık sayfalar tek çağrıda kopyalanır:
```rust
pub fn parse_ranges(s: &str, pages: i32) -> Option<Vec<i32>>   // "1-3, 5, 8-" -> 0 tabanlı
pub fn pages_to_new<'a>(p: &'a Pdfium, src: &PdfDocument, idx: &[i32]) -> Result<PdfDocument<'a>, PdfiumError> {
    let mut out = p.create_new_pdf()?;
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && idx[j + 1] == idx[j] + 1 { j += 1; }
        let at = out.pages().len();
        out.pages_mut().copy_page_range_from_document(src, idx[i]..=idx[j], at)?;
        i = j + 1;
    }
    Ok(out)
}
```
```
ranges '1-3, 5, 8-' of 14 -> [0, 1, 2, 4, 7, 8, 9, 10, 11, 12, 13]
  parse_ranges("0-2") = None   ("5-3", "1-99", "x", "" de None)
extract -> out/extract.pdf: 11 pages
split 14 single pages + 4 parts of 4 in 5.4709ms
```
Bütün çıktılar `qpdf --check`'ten hatasız geçti. Her parça yalnız kendi sayfalarının nesnelerini taşır.

### 2.4 Sayfayı DPI ile işleme
```rust
let mut d = dpi;
let px = (wpt * d / 72.0).ceil() as u64 * (hpt * d / 72.0).ceil() as u64;
if px > max_px { d *= (max_px as f64 / px as f64).sqrt() as f32 * 0.999; }   // sayfa başına piksel sınırı
let cfg = PdfRenderConfig::new().scale_page_by_factor(d / 72.0);
let bmp = page.render_with_config(&cfg)?;        // beyaz zemin, açıklamalar ve form verisi dahil (varsayılanlar)
let rgba = bmp.as_rgba_bytes();                  // kopya; varsayılanda FPDF_REVERSE_BYTE_ORDER açık
// PNG: image::save_buffer(.., Rgba8)   JPEG: alfa atılır, jpeg-encoder (q 90, JFIF yoğunluğu = dpi)
```
```
render 14 pages at 72 dpi to png:   89.6ms;  page 1 640x480,   A4 page 12 842x595
render 14 pages at 150 dpi to jpg: 297.1ms;  page 1 1333x1000, A4 page 12 1754x1240
render 14 pages at 300 dpi to png: 405.8ms;  page 1 2667x2000, A4 page 12 3508x2480
render 14 pages at 300 dpi to jpg: 1.0036s
[mem] after merge/split/render: working set 13.9 MiB (peak 106.2), private 3.9 MiB (peak 97.1)
```
- Ad önerisi: `ad - page N.png|jpg`.
- `as_rgba_bytes()` ve `as_raw_bytes()` arabelleği kopyalar. RGB'ye çevirmek bir kopya daha demek; bellek hesabı için 4.2'ye bakın.

### 2.5 Şifreli PDF
Örnekler qpdf ile yapıldı: `--encrypt --user-password=pw --owner-password=own --bits=256` (AES-256, R6), `--allow-weak-crypto ... --bits=128 --use-aes=n` (RC4, R3) ve yalnız sahip parolası olan `--user-password= ... --modify=none --extract=n`.
```rust
p.load_pdf_from_file(path, password).map_err(|e| match e {
    PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) =>
        if password.is_none() { OpenError::NeedsPassword } else { OpenError::WrongPassword },
    PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => OpenError::Damaged,
    e => OpenError::Other(format!("{e:?}")),
})
```
```
open out/enc_aes256.pdf pw=None: NeedsPassword
open out/enc_aes256.pdf pw=Some("nope"): WrongPassword
open out/enc_aes256.pdf pw=Some("pw"): OK, 3 pages
open out/enc_aes256.pdf pw=Some("own"): OK, 3 pages        (sahip parolası da açar)
open out/enc_rc4.pdf pw=None: NeedsPassword /  pw=Some("pw"): OK
open out/enc_owner_only.pdf pw=None: OK, 3 pages           (izin kısıtı parola sormaz; pdfium izinleri uygulamaz)
open out/truncated.pdf / garbage.pdf / notpdf.pdf: Damaged
open out/missing.pdf: Other("IoError(Os { code: 2, kind: NotFound, .. })")
```
- Parolanın eksik ya da yanlış olduğunu pdfium ayırt etmez; ikisi de `FPDF_ERR_PASSWORD`'dür. Ayrımı çağıran yapar: parola verilmediyse "Password", verildiyse "Wrong password". 5.2'deki soru katmanı bu ikisiyle çalışır.
- Şifreli bir kaynaktan çıkan dosya **şifresiz** olur: `qpdf --show-encryption out/from_encrypted.pdf` "File is not encrypted" dedi. pdfium yeni belgeye şifre koyamaz. Spec bu konuda bir şey söylemiyor; ya katmanda uyarı verilir ya da kabul edilir.

### 2.6 Boşaltma ve yeniden yükleme: **pdfium-render ile mümkün değil**
pdfium-render 0.9.4 bağlamaları süreç genelinde bir `static BINDINGS: OnceCell<Box<dyn PdfiumLibraryBindings>>`'a koyar. `Pdfium::drop` yalnız varsayılan yazı tipi bilgisini serbest bırakır: `FPDF_DestroyLibrary` çağrılmaz, `Library` kapanmaz. İkinci `bind_to_library` hata döner:
```
[mem] after drop(Pdfium): working set 13.8 MiB, private 4.5 MiB
rebind after drop: PdfiumLibraryBindingsAlreadyInitialized
```
Yani bir süreçte yalnız bir `Pdfium` olabilir; bırakılırsa o süreç bir daha pdfium kullanamaz. Spec'teki "iş başında yükler, iş bitince bırakır" pdfium-render ile süreç içinde yapılamaz.

pdfium'un kendisi boşaltılabiliyor. `rawload.rs`, libloading ile `FPDF_InitLibrary`, belge açma, `FPDF_DestroyLibrary` ve `FreeLibrary` sırasını üç kez çalıştırdı:
```
round 1: pages=14 loaded=true ws=7.9 MiB private=1.3 MiB
  after FreeLibrary: loaded=false ws=6.7 MiB private=1.1 MiB
round 2: pages=14 loaded=true ws=8.4 MiB private=1.4 MiB
  after FreeLibrary: loaded=false ws=7.3 MiB private=1.2 MiB
round 3: pages=14 loaded=true ws=8.5 MiB private=1.4 MiB
  after FreeLibrary: loaded=false ws=7.5 MiB private=1.2 MiB
```
Seçenekler (öneri 5'te):
1. **Yardımcı süreç:** `gezik --pdf-worker`, işi stdin'den alır, ilerlemeyi stdout'a yazar (ffmpeg ve 7-Zip gibi). İş bitince süreç biter, kütüphane kendiliğinden boşalır. Bu yol çökmeye karşı yalıtım da sağlar (bkz. 4.1).
2. Süreç içinde, ilk PDF işinde yükleyip çıkışa kadar tutmak. Ölçülen maliyet: belgeler kapandıktan sonra yaklaşık 3-4 MB private ve yaklaşık 6-10 MB paylaşılan/kod sayfası.
3. pdfium-render yerine yaklaşık 20 işlevlik kendi `libloading` bağlamamız (+16 KB, bkz. 2.8). Boşaltma yapılabilir, ama Unicode yollar için `FPDF_LoadCustomDocument` geri çağrılarını da kendimiz yazmamız gerekir.

### 2.7 İş parçacığı güvenliği
- pdfium iş parçacığı güvenli değil (README: "should be assumed _not_ to be thread safe ... recommend parallel processing, not multi-threading").
- `thread_safe` özelliği bütün çağrıları tek bir muteks arkasına alır; hız kazancı yoktur. Bu özellik olmadan `Pdfium` ve `PdfDocument` **`Send` değildir** (denendi: `is_send::<Pdfium>()` "`*mut _FPDF_SYSFONTINFO` cannot be sent between threads safely" hatası verdi; `--features thread_safe` ile derlendi). Yani `Pdfium` onu yaratan iş parçacığında kalır. 2.6 ile birlikte düşünüldüğünde süreç içi seçenek "tek, kalıcı bir pdfium iş parçacığı" demektir.
- Spec'teki "tek iş parçacığı" kararı geçerli. Yardımcı süreçte bu kendiliğinden sağlanır ve `thread_safe` gerekmez.

### 2.8 Exe boyutu (Gezik release profili)
| aşama (`sizeprobe`) | exe bayt | fark |
|---|---|---|
| taban | 794,112 | – |
| + pdfium-render (merge, copy range, save, render, as_rgba_bytes) | 1,218,048 | **+423,936** |
| + kendi libloading bağlamamız (aynı işlemler, 17 işlev) | 810,496 | **+16,384** |
| + img2pdf + pdfium-render | 1,519,616 | +725,504 |

---

## 3. pdfium ikilileri (bblanchon/pdfium-binaries)

### 3.1 Sürüm
En son sürüm: **`chromium/8086`**, "PDFium 157.0.8086.0", 2026-10-05 (önceki sürümler: 8076 09-29, 8066 09-21, 8057 09-14; haftalık çıkıyor). `VERSION` dosyası `MAJOR=157 MINOR=0 BUILD=8086 PATCH=0` diyor. `args.gn` içinde `pdf_enable_v8 = false`, `pdf_enable_xfa = false`, `is_debug = false`, `pdf_use_partition_alloc = false` var: JavaScript ve XFA yok, bu da saldırı yüzeyini daraltır.

### 3.2 Dosyalar (8086; `digest`'ler GitHub API `assets[].digest`'ten, yerel `sha256sum` ile birebir aynı)
| platform | dosya | tgz bayt | kitaplık | kitaplık bayt | sha256 |
|---|---|---|---|---|---|
| Windows x64 | `pdfium-win-x64.tgz` | 3,866,531 | `bin/pdfium.dll` | 7,494,656 | `1fd8af95…cc28487d` |
| Windows arm64 | `pdfium-win-arm64.tgz` | 3,636,496 | `bin/pdfium.dll` | 6,871,552 | `1799b803…eba408344` |
| macOS arm64 | `pdfium-mac-arm64.tgz` | 3,521,983 | `lib/libpdfium.dylib` | 7,339,520 | `e98679e0…9fc13ab` |
| macOS x64 | `pdfium-mac-x64.tgz` | 3,717,448 | `lib/libpdfium.dylib` | 7,715,768 | `933a85a1…9ca85d` |
| Linux x64 (glibc) | `pdfium-linux-x64.tgz` | 3,788,766 | `lib/libpdfium.so` | 7,942,208 | `588577cf…f28bb935` |
| Linux arm64 (glibc) | `pdfium-linux-arm64.tgz` | 3,708,875 | `lib/libpdfium.so` | 8,075,568 | `e7e2fe46…1480a2` |
| Linux x64 musl | `pdfium-linux-musl-x64.tgz` | 7,350,076 | `lib/libpdfium.so` | 17,528,864 | `82d96a7e…f28051` |

Tam özetler: win-x64 `1fd8af952832dbb0eb16d9249f68fe09e5f5ebf7c3dd9f6066ea2720cc28487d`, win-arm64 `1799b8034e6d64946fec0ae79f3edfc8ba58ccd80c70a46194803b1eba408344`, mac-arm64 `e98679e052c07edbb5a627980902abb823d4b3f35744d877bd21668bd9fc13ab`, mac-x64 `933a85a138f6027243c56bff8676375c33ceeb767401389415ffc44d689ca85d`, linux-x64 `588577cf52dabc1a444988bac841920df54cc2f141801424de97ab04f4fbb935`, linux-arm64 `e7e2fe4686925618330103cb167950aca5a84bb00fd977a41b86be59dd1480a2`.
- Ayrı bir `.sha256` dosyası yok. Bunun yerine GitHub asset `digest`'i ve `pdfium-attestation.json` var. Bu dosya bir SLSA provenance v1 (Sigstore/DSSE) belgesidir; `subject` listesi aynı sha256'ları taşıyor (ayrıştırılıp karşılaştırıldı) ve iş akışı `bblanchon/pdfium-binaries/.github/workflows/build-all.yml@refs/heads/master`. `gh attestation verify pdfium-win-x64.tgz --repo bblanchon/pdfium-binaries` ile doğrulanabilir, ama bu makinede `gh` olmadığı için çalıştırılmadı. `prepare` betiği ffmpeg'deki gibi asset `digest`'ini `sources.sha256`'ya sabitler.
- Ayrıca `pdfium-mac-univ.tgz` (7,1 MB, iki mimari tek dosyada) ve `pdfium-v8-*` (JavaScript'li, 12-14 MB) var; ikisine de gerek yok.
- **Yeniden paketleme** (dll/dylib/so + LICENSE + licenses/): zip -9 ile 3,4-3,7 MB, tar.xz -9e ile 2,5-2,9 MB (win-x64 için 3,746,949 / 2,927,596). Kurulumda diskte yaklaşık 7,5-8 MB. Spec'teki "~5 MB" yerine "~3 MB indirme, ~8 MB disk" yazılmalı.

### 3.3 Lisans, imza ve platform
- Gönderilecekler: `LICENSE` (Benoit Blanchon, MIT; derleme betikleri için) ve `licenses/` klasörünün 17 dosyası: `pdfium.txt` (BSD-3-Clause + Apache-2.0), abseil, agg23, dragonbox (Apache2-LLVM ve Boost), fast_float, freetype (FTL), harfbuzz, icu, lcms, libjpeg_turbo (`.ijg` ve `.md`), libopenjpeg, libpng, llvm-libc, simdutf, zlib. Liste bütün platformlarda aynı. Hepsi izin verici lisanslar; GPL yok.
- **macOS:** arm64 dylib `LC_CODE_SIGNATURE` taşıyor; CodeDirectory bayrakları `0x20002` = ad-hoc + linker-signed, sertifika yok. **x64 dylib'de imza hiç yok.** İkisi de yalnız sistem çerçevelerine bağlı (AppKit, CoreGraphics, CoreFoundation, Foundation, libSystem), `minos 13.0`. Spec "en az ad-hoc imzalı (arm64'te zorunlu)" diyor: arm64 bunu karşılıyor; Intel'de imzasız dylib yüklenir. Gezik hardened runtime ile imzalanırsa (depoda imzalama adımı yok) `dlopen` kütüphane doğrulamasına takılır; o zaman `com.apple.security.cs.disable-library-validation` gerekir ya da Gezik'in ekip kimliğiyle yeniden imzalamak gerekir. Bu, yardımcı süreç seçeneğinde de aynıdır. `NSURLSession` karantina özniteliği koymadığı için Gatekeeper sorun çıkarmaz (7zz'de görüldü, `macos-test-results.md`). Dosyayı değiştirmek (`install_name_tool`) linker imzasını bozar; dosyaya dokunulmamalı.
- **Linux glibc:** x64 en çok `GLIBC_2.16`, arm64 en çok `GLIBC_2.17` istiyor; bağımlılıkları libc, libm, libpthread, libgcc_s. Yani çok geniş uyum var. **musl** ayrı bir dosya ve 17,5 MB (libc++ içinde). Gezik'in musl hedefi yoksa sunulmaz.

### 3.4 pdfium-render ↔ pdfium sürüm uyumu
`pdfium_NNNN` özelliği, bağlanacak işlev listesini seçer. API eklemeli ilerliyor:
```
python pe.py win-x64-6996/bin/pdfium.dll win-x64/bin/pdfium.dll -> 448 / 471 dışa aktarım; 8086'da fazladan 23, 6996'da fazladan 0
python pe.py win-x64-7881/bin/pdfium.dll win-x64/bin/pdfium.dll -> 460 / 471; 8086'da fazladan 11, 7881'de fazladan 0
```
| bağlama (`pdfium_7881`) → ikili | sonuç |
|---|---|
| chromium/8086 | çalışıyor (yukarıdaki bütün denemeler) |
| chromium/7881 | çalışıyor (`merge 14 pages`, işleme aynı) |
| chromium/6996 (spec'teki etiket) | `bind failed: LoadLibraryError(GetProcAddress { source: 127 })` |

Kural: ikili, bağlama özelliğinin sürümüne eşit ya da daha yeni olmalı. Bağlama hemen yapıldığı için, gelecekteki bir pdfium bir işlevi kaldırırsa yükleme tümden başarısız olur. Bu yüzden manifest pdfium sürümünü sabitler, yükseltmede bu deneme tekrar çalıştırılır. pdfium-render README'si 6996'nın macOS'ta bilinen bir hatası olduğunu da yazıyor (issue #192). `pdfium-6996-1` etiketi bırakılmalı.

---

## 4. Riskler

### 4.1 Kötü niyetli PDF
- pdfium Chrome'un motorudur ve düzenli CVE alır (bellek güvenliği hataları). Chrome onu sandbox'lı bir süreçte çalıştırır. Gezik onu süreç içinde çalıştırırsa, kötü niyetli bir PDF Gezik'in yetkileriyle kod çalıştırabilir ya da en azından bütün pencereleriyle birlikte dosya yöneticisini çökertebilir (`abort`, segfault; Rust bunu yakalayamaz).
- Bu ikilide V8 (JavaScript) ve XFA kapalı; yüzey daha dar.
- Hafif bulanıklaştırma (`fuzz.py`): üç PDF'in rastgele bayt değişikliği, silme ve ekleme ile 400 türevi üretildi, her biri ayrı süreçte açılıp bütün sayfaları işlendi. Sonuç `{'handled': 400}`, çökme yok. Bu güven vermez (hedefli saldırı başka şeydir) ama sıradan bozuk dosyaların düzgün hata verdiğini gösterir (`Damaged`).
- **Öneri: yardımcı süreç** (`gezik --pdf-worker`, aynı exe). Getirdikleri: çökme yalıtımı (iş satırına "PDF engine stopped" yazılır, Gezik ayakta kalır); iş bitince kütüphanenin gerçekten boşalması (spec 9.1'deki "boşta bellek değişmez" kuralı tutar); anında iptal (pdfium çağrıları bloklar, süreç öldürülür); bellek tavanı (Windows Job Object `JOB_OBJECT_LIMIT_PROCESS_MEMORY`, Unix `setrlimit(RLIMIT_AS)`). ffmpeg ve 7-Zip için alt süreç ve ilerleme okuma altyapısı 5b/5c'de zaten kuruluyor.

### 4.2 Bellek: 300 DPI'da dev sayfa
- `huge_page.pdf` 14400 × 14400 pt. 300 DPI'da bu 60000 × 60000 px, yani 14,4 GB BGRA eder. Sınır olmadan pdfium `FPDFBitmap_Create`'te geri çevirdi, çökme olmadı:
```
uncapped 300 dpi (60000x60000 = 14.4 GB BGRA): Err("PdfiumLibraryInternalError(Unknown)")
```
- 100 MP sınırıyla sayfa 9990 × 9990'a (50 dpi) indi, ama süreç tepesi **1060 MB** oldu: bitmap 400 MB + `as_rgba_bytes` kopyası 400 MB + JPEG için RGB kopyası 300 MB.
```
render 300 dpi capped at 100 MP -> 9990x9990 at 50.0 dpi in 675.3469ms
[mem] after capped huge render: working set 12.5 MiB (peak 1060.1), private 2.5 MiB (peak 1052.2)
```
- **Öneri:** sayfa başına en çok **64 MP**. A4 300 dpi 8,7 MP, A2 300 dpi 35 MP, A1 300 dpi 70 MP eder; sınır aşılırsa DPI o sayfa için düşürülür ve iş satırına yazılır. Ek olarak doğrudan `PdfBitmapFormat::BGR` ile işleyip (ters bayt sırası açık olduğu için RGB gelir) tek kopya kullanmak. JPEG'in kenar sınırı 65535 px (jpeg-encoder `u16` alır); sınır o da olmalı. `UserUnit` pdfium tarafından yok sayılıyor (`userunit.pdf` 14400 pt bildirdi), bu yüzden hesap `page.width()` ile doğru kalır.

### 4.3 Çok sayfalı belge
`many_pages.pdf` (20000 sayfa, 2,6 MB):
```
many_pages: 20000 pages, open 22.9274ms
  append all + save: 182.0783ms
  split first 200 single pages: 58.9339ms
[mem] after many pages: working set 59.8 MiB (peak 1060.1), private 52.2 MiB (peak 1052.2)
```
pdfium sayfaları açılışta tembel yükler. Birleştirme ve kaydetme 20000 sayfada 0,18 s sürdü, 52 MB private bellek tuttu. "Her sayfa ayrı dosya" 20000 dosya üretir. Katmanda dosya sayısının gösterilmesi (ve belki onay istenmesi) önerilir; ilerleme sayfa başına bildirilir, iptal sayfalar arasında denetlenir.

### 4.4 Yazı tipleri
- `fonts.pdf`: gömülü olmayan Helvetica (temel 14) ile gömülü olmayan `Arial,Bold`. İkisi de Windows'ta doğru işlendi (`fonts - page 1.png`'de kalın Arial). pdfium temel 14 yerine kendi içine gömülü Foxit yazı tiplerini kullanır; geri kalanlar için sistem yazı tiplerini arar.
- macOS ve Linux denenmedi. pdfium kaynağına göre Linux'ta `/usr/share/fonts`, `/usr/local/share/fonts` ve X11 klasörleri taranır. Gömülü olmayan CJK metin, uygun yazı tipi kurulu değilse boş ya da yanlış glif çıkar. Bu, PDF→resim'in kalitesini etkiler, birleştirme ve bölmeyi etkilemez. Gerekirse `PdfiumLibraryConfig` ile ek yazı tipi yolu verilebilir.

### 4.5 Diğer
- Birleştirme ve bölme yer imlerini, formları ve meta veriyi düşürür; şifreli kaynaktan şifresiz çıktı üretir (2.2, 2.5).
- pdfium-binaries her hafta çıkar. Güvenlik güncellemesi için `gezik-tools`'a yeni etiket açılır (`pdfium-NNNN-1`), aynı Gezik sürümünde bu denemeler bir kez daha koşulur (bağlama uyumu, 3.4).

---

## Sonuç

**Önerilen sürümler**
- `pdf-writer = "0.15.0"` (MIT OR Apache-2.0), `miniz_oxide = "0.9.1"` (lock'ta zaten var).
- `pdfium-render = { version = "0.9.4", default-features = false, features = ["pdfium_7881"] }` (MIT OR Apache-2.0); ardından `cargo update -p libloading@0.9.0 --precise 0.8.9` (tek libloading kopyası).
- pdfium: bblanchon/pdfium-binaries **`chromium/8086`** (PDFium 157.0.8086.0, 2026-10-05), V8/XFA'sız `pdfium-{win-x64,win-arm64,mac-arm64,mac-x64,linux-x64,linux-arm64}.tgz`. `gezik-tools` etiketi **`pdfium-8086-1`**. İçerik: kitaplık + `LICENSE` + `licenses/*` (17 dosya). İndirme 2,5-3,7 MB, disk ~8 MB. Kaynak doğrulaması asset `digest` ve SLSA attestation ile; Gezik yalnız kendi paketinin SHA-256'sını sabitler. 7881 ile de çalışır; 6996 çalışmaz.

**Spec'ten sapmalar ve kararlar**
1. `pdfium-6996-1` → `pdfium-8086-1`. 6996, `pdfium_7881` bağlamasıyla yüklenmiyor (`GetProcAddress 127`) ve macOS hatası var.
2. "İş başında yükler, iş bitince bırakır" pdfium-render ile süreç içinde yapılamaz: bağlamalar süreç genelinde `OnceCell`'de, `drop` boşaltmıyor, yeniden bağlama `PdfiumLibraryBindingsAlreadyInitialized` veriyor. Öneri: pdfium işleri **yardımcı süreçte** (`gezik --pdf-worker`). Bu hem boşaltmayı hem çökme yalıtımını, iptali ve bellek tavanını sağlar. Süreç içinde kalınacaksa ilk kullanımda yüklenip çıkışa kadar tek bir kalıcı iş parçacığında tutulur (~3-4 MB private).
3. `thread_safe` kapalı. `Pdfium` bu durumda `Send` değil, tek iş parçacığı kararıyla uyumlu.
4. Boyut: spec "~5 MB" diyor; gerçek indirme ~3 MB, disk ~8 MB.
5. EXIF yönü dönüşüm matrisiyle uygulanır; `/Rotate` ya da yeniden kodlama yok. Sekiz yön doğrulandı.
6. Adobe APP14'lü CMYK JPEG'e `Decode [1 0 1 0 1 0 1 0]` yazılır. ICC profilleri `ICCBased` olarak gömülür. 16 bit korunur (big-endian). Alfa SMask olur. Flate'ten önce Paeth süzgeci uygulanır (`/Predictor 15`, düzey 1-3).
7. pdf-writer'ın `Pdf` tipi bütün dosyayı bellekte tutar. Nesne başına `Chunk` yazılıp xref tablosu kendimiz yazılır (100 × 12 MP'de tepe 17 MB).
8. Yeni sayfa kuralı (öneri): yatay resim A4/Letter'da yatay sayfaya konur; "resim boyutu" sayfası JFIF dpi'yi kullanır, yoksa 72 dpi; 14400 pt'yi aşan resim küçültülür.
9. PDF→resim için sayfa başına 64 MP ve kenar başına 65535 px sınırı; aşan sayfanın DPI'ı düşürülür.
10. Katmanda belirtilecekler: birleştirme ve bölme yer imlerini, formları ve meta veriyi düşürür; şifreli girişten şifresiz çıktı üretilir. Parolanın eksik ya da yanlış olduğunu Gezik, parola verip vermediğine bakarak ayırt eder.
11. Exe artışı: img2pdf +0,30 MB, pdfium-render +0,42 MB. Kendi 20 işlevlik libloading bağlamamız +16 KB tutar. Boyut önemliyse yedek seçenek budur; o yolda Unicode yollar için `FPDF_LoadCustomDocument` geri çağrısı yazılmalı.
