# Ayarlar ve Tema Sistemi — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gezik'e çalışma anında yüklenen, canlı yenilenen stil temaları ve işletim sistemleri arasında taşınabilir bir ayar dosyası eklemek.

**Architecture:** Yeni `gezik-config` crate'i Slint'ten bağımsızdır: dosyaları okur, TOML'u elle doğrular ve her zaman eksiksiz bir `ResolvedTheme` ile bir uyarı listesi üretir. `gezik` uygulaması bu sonucu Slint'in `Theme` global'ine kopyalar (`theme_bridge.rs`), yapılandırma klasörünü `notify` ile izler (`watcher.rs`) ve pencere boyutunu/konumunu makineye özel `state.toml` dosyasında saklar (`window_state.rs`).

**Tech Stack:** Rust 2024 (stable 1.99), Slint 1.18 (yalnızca yazılımla çizim), `toml` 1.x, `dirs` 7, `notify` 8.

**Spec:** `docs/superpowers/specs/2026-10-03-ayarlar-ve-tema-design.md`

## Spec'ten sapmalar (plan yazılırken API doğrulamasıyla netleşti)

1. **Slint `Palette` ezilmiyor.** Slint 1.18'de `Palette` renkleri salt okunur (`out`). `Palette.color-scheme` ise `in-out` (yalnızca renkler salt okunur), yani atanabilir; ama zorlamak, `theme = "auto"` modunun dayandığı sistem açık/koyu değerini gizlerdi (sistemin modu yalnızca bu özellikten okunabiliyor, `SlintInternal` kullanıcı koduna kapalı). Bu yüzden `Palette.color-scheme` sisteme bırakılıyor. Sonuç: `ListView` kaydırma çubuğu sistemin moduna uyar. Varsayılan `auto` modunda bu, temayla her zaman uyumludur; yalnızca sistemin tersi bir tema zorlanırsa kaydırma çubuğu sistem renginde kalır. Spec'teki "doğrulanacak risk" maddesinin sonucu budur.
2. **Şablon tema adı `example.toml`.** Spec `ornek-tema.toml` diyordu. Arayüz metinleri İngilizce olduğundan şablon adı ve yorumları da İngilizce.
3. **Uyarı metinleri İngilizce:** `nord.toml line 12: ...` (spec örneği Türkçeydi).
4. **`serde` doğrudan bağımlılık değil.** TOML, `toml::Table` olarak okunup değerler elle doğrulanıyor. Bu, spec'in istediği "tek geçersiz değer yok sayılır, gerisi yüklenir" davranışını mümkün kılıyor (serde ile tek hatalı değer bütün dosyayı düşürürdü).
5. **Ayrıştırma bölünmesi:** Dosya okuma (I/O) arka plan iş parçacığında, çözümleme (birkaç mikrosaniyelik saf hesap) arayüz iş parçacığında yapılıyor. Böylece sistem modu değiştiğinde disk okumadan yeniden çözümlenebiliyor.
6. **Tema adları büyük/küçük harfe duyarsız.** `Nord.toml` dosyası `theme = "nord"` ile seçilebilir. Windows dosya sistemi duyarsız, Linux duyarlı olduğu için taşınabilirlik bunu gerektiriyor.
7. **`GEZIK_CONFIG_DIR` ortam değişkeni.** Ayarlanmışsa yapılandırma klasörü olarak kullanılır. Doğrulama adımlarının kullanıcının gerçek ayarlarına dokunmaması için gerekli; ileri düzey kullanıcılar için de işe yarar.
8. **Pencere konumu kontrolü** Slint'in `unstable-winit-030` özelliğiyle yapılıyor (monitör listesine erişmenin tek yolu). Bu özellik yalnızca winit'i yeniden dışa aktarır, ikili boyutunu değiştirmez.

## Global Constraints

- Rust edition 2024, stable toolchain. Komutlar `D:\Work\gezik` içinden çalıştırılır; `cargo` yolu `~/.cargo/bin/cargo`.
- Slint 1.18 yalnızca yazılımla çizim özellikleriyle derlenir; mevcut özellik listesine sadece `"unstable-winit-030"` eklenir.
- Yeni bağımlılıklar tam olarak: `gezik-config` → `toml = "1"`, `dirs = "7"`; `gezik` → `gezik-config`, `notify = "8"`. Başka bağımlılık eklenmez.
- Kullanıcı verisi (ayar/tema/durum dosyaları) hiçbir koşulda panic'e yol açmaz: kullanıcı verisi üzerinde `unwrap`/`expect` yok. `expect` yalnızca programa gömülü yerleşik temalar için kullanılır.
- Arayüz metinleri ve uyarılar İngilizcedir.
- Tema renkleri `"#rrggbb"` veya `"#rrggbbaa"`. Sayı aralıkları: `font-size` 8–32, `row-height` 16–64, `icon-size` 12–48, `radius` 0–16, `spacing` 0–24.
- `compact` yoğunluk: `row-height` ve `spacing` × 0,8, yuvarlanır; `row-height` en az 16.
- Dosya yazmaları her zaman geçici dosya + yeniden adlandırma ile yapılır (`write_atomic`).
- Uygulama `settings.toml` dosyasına yazmaz (yalnızca ilk açılışta yoksa şablonu oluşturur). Yazdığı tek dosya `state.toml`.
- Performans kabulü: boşta RAM (Görev Yöneticisi) bugünkü ~5 MB'tan en fazla +2 MB; açılış ~40 ms civarında kalır; 100 bin dosyada ~13 MB ve kaydırma CPU'su gerilemez.

## Review Focus

1. **Not Defteri'nin UTF-8 BOM ile kaydettiği dosyalar** (Windows'ta yaygın): BOM'lu bir `settings.toml` veya tema dosyası normal yüklenmeli. Test: Task 6 `strips_utf8_bom_and_matches_theme_names_case_insensitively`.
2. **Tema adında büyük/küçük harf farkı:** `themes/Nord.toml` + `theme = "nord"` her işletim sisteminde bulunmalı. Test: Task 4 `ids_are_case_insensitive`, Task 6 aynı test.
3. **Seçili tema dosyası uygulama açıkken silinir:** uygulama çökmemeli, `dark`'a dönüp uyarı göstermeli. Test: Task 6 `missing_selected_theme_falls_back_to_dark`.
4. **Editör kaydederken dosya boş veya yarım okunur:** boş tema = tümüyle `base` değerleri, boş ayar = varsayılanlar, yarım dosya = satır numaralı uyarı; hiçbiri çökme değil. Testler: Task 3 `empty_file_is_valid_and_empty`, Task 5 `defaults_when_empty`, Task 6 `broken_selected_theme_keeps_current`.
5. **İzleyicinin gereksiz yeniden yüklemeleri:** `state.toml` yazımı ve editör geçici dosyaları (`.settings.toml.tmp`, `.nord.toml.swp`, `nord.toml~`) yeniden yükleme tetiklememeli. Test: Task 6 `recognizes_config_files`.

---

## Dosya yapısı

```
Cargo.toml                                  workspace: gezik-config eklenir
crates/gezik-config/
  Cargo.toml
  src/lib.rs                                modül listesi, test yardımcı fonksiyonu
  src/color.rs                              Color: "#rrggbb[aa]" ayrıştırma
  src/warning.rs                            Warning: dosya + satır + mesaj
  src/paths.rs                              config_dir, KnownDirs ({home}…), write_atomic
  src/theme.rs                              tema ayrıştırma, doğrulama, base zinciri, yoğunluk
  src/settings.rs                           Settings (taşınabilir), State (makineye özel)
  src/store.rs                              ConfigStore: klasör düzeni, okuma, resolve()
  themes/dark.toml, themes/light.toml       yerleşik temalar (programa gömülü)
  templates/settings.toml                   ilk açılış şablonu
  templates/example.toml                    ilk açılış tema şablonu
crates/gezik/
  Cargo.toml                                gezik-config, notify, slint "unstable-winit-030"
  ui/theme.slint                            global Theme
  ui/widgets/button.slint                   ToolButton
  ui/widgets/text-field.slint               TextField
  ui/app.slint                              tüm sabit renkler Theme'e taşınır
  src/main.rs                               yapılandırmayı bağlar
  src/theme_bridge.rs                       ResolvedTheme → Theme global
  src/watcher.rs                            biriktiren dosya izleyici
  src/window_state.rs                       pencere boyutu/konumu
scripts/perf/screenshot.ps1                 pencere ekran görüntüsü (doğrulama)
scripts/perf/measure.ps1                    açılış + boşta RAM
scripts/perf/stress.ps1                     büyük klasör + kaydırma
README.md                                   tema ve ayar belgeleri
```

---

### Task 1: `gezik-config` iskeleti — `Color` ve `Warning`

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/gezik-config/Cargo.toml`
- Create: `crates/gezik-config/src/lib.rs`
- Create: `crates/gezik-config/src/color.rs`
- Create: `crates/gezik-config/src/warning.rs`

**Interfaces:**
- Consumes: —
- Produces:
  - `gezik_config::Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }` (`Debug, Clone, Copy, Default, PartialEq, Eq`), `Color::parse(&str) -> Option<Color>`
  - `gezik_config::Warning { pub file: String, pub line: Option<usize>, pub message: String }` (`Debug, Clone, PartialEq, Eq`, `Display`), `Warning::new(file, message)`, `Warning::from_toml_error(file, text: &str, err: &toml::de::Error)`
  - `crate::test_dir(name: &str) -> PathBuf` (yalnızca `#[cfg(test)]`, crate içi)

- [ ] **Step 1: Workspace'e crate'i ekle**

`Cargo.toml` (workspace) içinde:

```toml
[workspace]
resolver = "3"
members = ["crates/gezik-core", "crates/gezik-config", "crates/gezik"]

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
gezik-core = { path = "crates/gezik-core" }
gezik-config = { path = "crates/gezik-config" }
```

(`[profile.release]` bölümü olduğu gibi kalır.)

`crates/gezik-config/Cargo.toml`:

```toml
[package]
name = "gezik-config"
description = "Settings and theme handling for the Gezik file manager"
version.workspace = true
edition.workspace = true

[dependencies]
toml = "1"
dirs = "7"
```

`crates/gezik-config/src/lib.rs`:

```rust
//! Settings and themes for Gezik. Knows nothing about the UI: inputs are files and
//! text, outputs are validated values plus warnings to show the user.

mod color;
mod warning;

pub use color::Color;
pub use warning::Warning;

/// A fresh, empty folder for a test.
#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gezik-config-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
```

- [ ] **Step 2: `Color` için başarısız testleri yaz**

`crates/gezik-config/src/color.rs`:

```rust
/// An RGBA color, written in files as `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn parse(text: &str) -> Option<Color> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn parses_rgb_as_opaque() {
        assert_eq!(Color::parse("#2e3440"), Some(Color { r: 0x2e, g: 0x34, b: 0x40, a: 255 }));
    }

    #[test]
    fn parses_rgba() {
        assert_eq!(Color::parse("#3b425280"), Some(Color { r: 0x3b, g: 0x42, b: 0x52, a: 0x80 }));
    }

    #[test]
    fn accepts_uppercase_hex() {
        assert_eq!(Color::parse("#FFFFFF"), Some(Color { r: 255, g: 255, b: 255, a: 255 }));
    }

    #[test]
    fn rejects_invalid_text() {
        for bad in ["", "#", "2e3440", "#fff", "#12345", "#1234567", "#2e344g", "#ÿÿÿ"] {
            assert_eq!(Color::parse(bad), None, "{bad:?} should be rejected");
        }
    }
}
```

`lib.rs`'teki `warning` modülü henüz yok; derlenmesi için `warning.rs`'i Step 5'e kadar şu geçici içerikle oluştur:

```rust
pub struct Warning;
```

- [ ] **Step 3: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config color`
Expected: `parses_rgb_as_opaque`, `parses_rgba`, `accepts_uppercase_hex` FAIL (`left: None`); `rejects_invalid_text` PASS.

- [ ] **Step 4: `Color::parse`'ı yaz**

```rust
impl Color {
    pub fn parse(text: &str) -> Option<Color> {
        let hex = text.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Color {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }
}
```

Run: `~/.cargo/bin/cargo test -p gezik-config color`
Expected: 4 passed.

- [ ] **Step 5: `Warning` için başarısız testleri yaz**

`crates/gezik-config/src/warning.rs` (geçici içeriğin yerine):

```rust
use std::fmt;

/// A problem found in a settings or theme file. Never fatal: the caller falls back to a
/// sensible value and shows the warning to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The file the problem is in, e.g. `nord.toml`.
    pub file: String,
    /// 1-based line, when known.
    pub line: Option<usize>,
    pub message: String,
}

impl Warning {
    pub fn new(file: impl Into<String>, message: impl Into<String>) -> Self {
        Self { file: file.into(), line: None, message: message.into() }
    }

    /// A warning for a TOML syntax error, with the error's byte offset turned into a line.
    pub fn from_toml_error(file: impl Into<String>, text: &str, err: &toml::de::Error) -> Self {
        Self::new(file, "")
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Warning;

    #[test]
    fn displays_with_and_without_line() {
        let mut warning = Warning::new("nord.toml", "colors.accent: bad value");
        assert_eq!(warning.to_string(), "nord.toml: colors.accent: bad value");
        warning.line = Some(12);
        assert_eq!(warning.to_string(), "nord.toml line 12: colors.accent: bad value");
    }

    #[test]
    fn syntax_error_reports_line_and_message() {
        let text = "a = 1\nb = [\n";
        let err = text.parse::<toml::Table>().unwrap_err();
        let warning = Warning::from_toml_error("x.toml", text, &err);
        assert_eq!(warning.file, "x.toml");
        assert_eq!(warning.line, Some(2));
        assert!(warning.message.contains("unclosed array"), "{}", warning.message);
    }

    #[test]
    fn crlf_files_count_lines_the_same() {
        let text = "a = 1\r\nb = [\r\n";
        let err = text.parse::<toml::Table>().unwrap_err();
        assert_eq!(Warning::from_toml_error("x.toml", text, &err).line, Some(2));
    }
}
```

Run: `~/.cargo/bin/cargo test -p gezik-config warning`
Expected: 3 FAIL.

- [ ] **Step 6: `Warning`'i tamamla**

```rust
    pub fn from_toml_error(file: impl Into<String>, text: &str, err: &toml::de::Error) -> Self {
        Self {
            file: file.into(),
            line: err.span().map(|span| line_of(text, span.start)),
            message: err.message().to_string(),
        }
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())].iter().filter(|&&b| b == b'\n').count() + 1
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{} line {}: {}", self.file, line, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}
```

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: 7 passed, uyarı yok.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/gezik-config
git commit -m "Add gezik-config crate with Color and Warning"
```

---

### Task 2: Yollar — yapılandırma klasörü, işaretli yollar, güvenli yazma

**Files:**
- Create: `crates/gezik-config/src/paths.rs`
- Modify: `crates/gezik-config/src/lib.rs` (`pub mod paths;`)

**Interfaces:**
- Consumes: —
- Produces:
  - `paths::config_dir() -> Option<PathBuf>`: `GEZIK_CONFIG_DIR` varsa o, yoksa `dirs::config_dir()/gezik`
  - `paths::KnownDirs::new(Vec<(&'static str, PathBuf)>) -> KnownDirs`, `KnownDirs::system() -> KnownDirs`, `collapse(&self, &Path) -> String`, `expand(&self, &str) -> PathBuf`
  - `paths::write_atomic(path: &Path, contents: &str) -> io::Result<()>`

- [ ] **Step 1: Başarısız testleri yaz**

`lib.rs`'e `pub mod paths;` ekle (modül listesinin başına). `crates/gezik-config/src/paths.rs`:

```rust
//! Where Gezik keeps its files, and machine-independent `{token}` paths.

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

/// The per-user config folder: `GEZIK_CONFIG_DIR` if set, otherwise e.g.
/// `%APPDATA%\gezik` on Windows or `~/.config/gezik` on Linux.
pub fn config_dir() -> Option<PathBuf> {
    None
}

/// Well-known folders, used to store paths so they work on another machine or OS.
#[derive(Debug, Clone)]
pub struct KnownDirs {
    /// (token, folder), deepest folder first so nested folders win.
    dirs: Vec<(&'static str, PathBuf)>,
}

impl KnownDirs {
    pub fn new(dirs: Vec<(&'static str, PathBuf)>) -> Self {
        Self { dirs }
    }

    /// The current user's folders on this machine.
    pub fn system() -> Self {
        Self::new(Vec::new())
    }

    /// `C:\Users\a\Documents\Work` → `{documents}/Work`. Paths outside every known folder
    /// stay absolute, written with `/` separators.
    pub fn collapse(&self, path: &Path) -> String {
        String::new()
    }

    /// Inverse of [`collapse`](Self::collapse). Text that does not start with a known
    /// token is taken as a literal path.
    pub fn expand(&self, text: &str) -> PathBuf {
        PathBuf::new()
    }
}

/// Writes `contents` to a temporary file next to `path`, then renames it into place, so
/// readers and sync tools never see a half-written file.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake() -> KnownDirs {
        KnownDirs::new(vec![
            ("home", PathBuf::from("/u/alice")),
            ("documents", PathBuf::from("/u/alice/OneDrive/Documents")),
            ("downloads", PathBuf::from("/u/alice/Downloads")),
        ])
    }

    #[test]
    fn collapses_to_the_deepest_matching_folder() {
        let dirs = fake();
        assert_eq!(dirs.collapse(Path::new("/u/alice/OneDrive/Documents/Work/x")), "{documents}/Work/x");
        assert_eq!(dirs.collapse(Path::new("/u/alice/Music")), "{home}/Music");
        assert_eq!(dirs.collapse(Path::new("/u/alice/Downloads")), "{downloads}");
    }

    #[test]
    fn keeps_unknown_paths_absolute_with_forward_slashes() {
        assert_eq!(fake().collapse(Path::new("/srv/data")), "/srv/data");
    }

    #[test]
    fn expands_tokens() {
        let dirs = fake();
        assert_eq!(dirs.expand("{documents}/Work/x"), PathBuf::from("/u/alice/OneDrive/Documents/Work/x"));
        assert_eq!(dirs.expand("{downloads}"), PathBuf::from("/u/alice/Downloads"));
    }

    #[test]
    fn unknown_tokens_stay_literal() {
        assert_eq!(fake().expand("{nope}/x"), PathBuf::from("{nope}/x"));
    }

    #[test]
    fn round_trips() {
        let dirs = fake();
        for path in ["/u/alice/OneDrive/Documents/a/b", "/u/alice/Music", "/u/alice/Downloads", "/srv/data"] {
            assert_eq!(dirs.expand(&dirs.collapse(Path::new(path))), PathBuf::from(path), "{path}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_use_forward_slashes() {
        let dirs = KnownDirs::new(vec![("documents", PathBuf::from(r"C:\Users\a\Documents"))]);
        assert_eq!(dirs.collapse(Path::new(r"C:\Users\a\Documents\Work")), "{documents}/Work");
        assert_eq!(dirs.collapse(Path::new(r"D:\Work\gezik")), "D:/Work/gezik");
        assert_eq!(dirs.expand("D:/Work/gezik"), PathBuf::from(r"D:\Work\gezik"));
    }

    #[test]
    fn system_dirs_know_home() {
        assert_ne!(KnownDirs::system().expand("{home}"), PathBuf::from("{home}"));
    }

    #[test]
    fn config_dir_ends_with_gezik() {
        // GEZIK_CONFIG_DIR is not set when tests run.
        assert!(config_dir().is_some_and(|dir| dir.ends_with("gezik")));
    }

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp_file() {
        let dir = crate::test_dir("atomic");
        let path = dir.join("state.toml");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"]);
    }
}
```

- [ ] **Step 2: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config paths`
Expected: tüm `paths` testleri FAIL (taslak fonksiyonlar boş değer döndürüyor).

- [ ] **Step 3: Uygulamayı yaz**

```rust
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("GEZIK_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::config_dir().map(|dir| dir.join("gezik"))
}

impl KnownDirs {
    pub fn new(mut dirs: Vec<(&'static str, PathBuf)>) -> Self {
        dirs.sort_by_key(|(_, path)| std::cmp::Reverse(path.components().count()));
        Self { dirs }
    }

    pub fn system() -> Self {
        let candidates = [
            ("home", dirs::home_dir()),
            ("desktop", dirs::desktop_dir()),
            ("documents", dirs::document_dir()),
            ("downloads", dirs::download_dir()),
            ("pictures", dirs::picture_dir()),
            ("music", dirs::audio_dir()),
            ("videos", dirs::video_dir()),
        ];
        Self::new(candidates.into_iter().filter_map(|(token, dir)| Some((token, dir?))).collect())
    }

    pub fn collapse(&self, path: &Path) -> String {
        for (token, dir) in &self.dirs {
            if let Ok(rest) = path.strip_prefix(dir) {
                let rest = slash_path(rest);
                return if rest.is_empty() { format!("{{{token}}}") } else { format!("{{{token}}}/{rest}") };
            }
        }
        slash_path(path)
    }

    pub fn expand(&self, text: &str) -> PathBuf {
        if let Some((token, rest)) = text.strip_prefix('{').and_then(|inner| inner.split_once('}'))
            && let Some((_, dir)) = self.dirs.iter().find(|(t, _)| *t == token)
        {
            let mut path = dir.clone();
            path.extend(rest.split('/').filter(|part| !part.is_empty()));
            return path;
        }
        PathBuf::from(text)
    }
}

/// Joins path components with `/` on every OS: `C:\a\b` → `C:/a/b`.
fn slash_path(path: &Path) -> String {
    let mut out = String::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push_str(&prefix.as_os_str().to_string_lossy()),
            Component::RootDir => out.push('/'),
            other => {
                if !out.is_empty() && !out.ends_with('/') {
                    out.push('/');
                }
                out.push_str(&other.as_os_str().to_string_lossy());
            }
        }
    }
    out
}

pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let tmp = path.with_file_name(format!(".{}.tmp", name.to_string_lossy()));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}
```

- [ ] **Step 4: Testlerin geçtiğini gör**

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: Windows'ta 16 passed (Task 1'in 7'si + 9), diğer sistemlerde 15 (`windows_paths_use_forward_slashes` yalnızca Windows'ta derlenir).

- [ ] **Step 5: Commit**

```bash
git add crates/gezik-config
git commit -m "Add config folder lookup, portable {token} paths and atomic writes"
```

---

### Task 3: Tema dosyası ayrıştırma ve yerleşik temalar

**Files:**
- Create: `crates/gezik-config/themes/dark.toml`
- Create: `crates/gezik-config/themes/light.toml`
- Create: `crates/gezik-config/src/theme.rs`
- Modify: `crates/gezik-config/src/lib.rs` (`pub mod theme;`)

**Interfaces:**
- Consumes: `Color::parse`, `Warning::new`, `Warning::from_toml_error`
- Produces (Task 4 kullanır):
  - `pub const COLOR_KEYS: [&str; 13]`, `pub const METRIC_RANGES: [(&str, f32, f32); 5]`
  - `pub struct ThemeColors` (13 alan, `Color`), `pub struct Metrics { pub font_family: String, pub font_size: f32, pub row_height: f32, pub icon_size: f32, pub radius: f32, pub spacing: f32 }` — ikisi de `Debug, Clone, Default, PartialEq`
  - crate içi: `struct PartialTheme`, `fn parse_theme(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Option<PartialTheme>`, `fn builtin_source(id: &str) -> Option<&'static str>`, `ThemeColors::set(&mut self, key: &str, Color)`, `Metrics::set(&mut self, key: &str, f32)`

- [ ] **Step 1: Yerleşik temaları oluştur**

`crates/gezik-config/themes/dark.toml`:

```toml
name = "Dark"

[colors]
background = "#1c1c1c"
surface = "#202020"
foreground = "#ffffff"
foreground-muted = "#a0a0a0"
border = "#3a3a3a"
accent = "#60cdff"
accent-foreground = "#000000"
selection = "#2d4f6b"
selection-foreground = "#ffffff"
hover = "#ffffff14"
folder-icon = "#e8b33f"
file-icon = "#9aa3ad"
danger = "#ff6b6b"

[metrics]
font-family = ""
font-size = 13
row-height = 26
icon-size = 16
radius = 6
spacing = 6
```

`crates/gezik-config/themes/light.toml`:

```toml
name = "Light"

[colors]
background = "#fafafa"
surface = "#f3f3f3"
foreground = "#1b1b1b"
foreground-muted = "#616161"
border = "#d6d6d6"
accent = "#005fb8"
accent-foreground = "#ffffff"
selection = "#cce4f7"
selection-foreground = "#1b1b1b"
hover = "#0000000d"
folder-icon = "#d9a520"
file-icon = "#8a939c"
danger = "#c42b1c"

[metrics]
font-family = ""
font-size = 13
row-height = 26
icon-size = 16
radius = 6
spacing = 6
```

- [ ] **Step 2: Başarısız testleri yaz**

`lib.rs`'e `pub mod theme;` ekle. `crates/gezik-config/src/theme.rs`:

```rust
//! Theme files: parsing, validation and (in `resolve_theme`) `base` inheritance.

use crate::{Color, Warning};

/// Every color a theme can set, as written in `[colors]`.
pub const COLOR_KEYS: [&str; 13] = [
    "background",
    "surface",
    "foreground",
    "foreground-muted",
    "border",
    "accent",
    "accent-foreground",
    "selection",
    "selection-foreground",
    "hover",
    "folder-icon",
    "file-icon",
    "danger",
];

/// Numeric `[metrics]` keys with their allowed range (inclusive).
pub const METRIC_RANGES: [(&str, f32, f32); 5] = [
    ("font-size", 8.0, 32.0),
    ("row-height", 16.0, 64.0),
    ("icon-size", 12.0, 48.0),
    ("radius", 0.0, 16.0),
    ("spacing", 0.0, 24.0),
];

const DARK: &str = include_str!("../themes/dark.toml");
const LIGHT: &str = include_str!("../themes/light.toml");

pub(crate) fn builtin_source(id: &str) -> Option<&'static str> {
    match id {
        "dark" => Some(DARK),
        "light" => Some(LIGHT),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeColors {
    pub background: Color,
    pub surface: Color,
    pub foreground: Color,
    pub foreground_muted: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub selection: Color,
    pub selection_foreground: Color,
    pub hover: Color,
    pub folder_icon: Color,
    pub file_icon: Color,
    pub danger: Color,
}

impl ThemeColors {
    /// `key` must be one of [`COLOR_KEYS`].
    pub(crate) fn set(&mut self, key: &str, color: Color) {
        let slot = match key {
            "background" => &mut self.background,
            "surface" => &mut self.surface,
            "foreground" => &mut self.foreground,
            "foreground-muted" => &mut self.foreground_muted,
            "border" => &mut self.border,
            "accent" => &mut self.accent,
            "accent-foreground" => &mut self.accent_foreground,
            "selection" => &mut self.selection,
            "selection-foreground" => &mut self.selection_foreground,
            "hover" => &mut self.hover,
            "folder-icon" => &mut self.folder_icon,
            "file-icon" => &mut self.file_icon,
            "danger" => &mut self.danger,
            _ => unreachable!("not a color key: {key}"),
        };
        *slot = color;
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {
    /// Empty means the system font.
    pub font_family: String,
    pub font_size: f32,
    pub row_height: f32,
    pub icon_size: f32,
    pub radius: f32,
    pub spacing: f32,
}

impl Metrics {
    /// `key` must be one of [`METRIC_RANGES`].
    pub(crate) fn set(&mut self, key: &str, value: f32) {
        let slot = match key {
            "font-size" => &mut self.font_size,
            "row-height" => &mut self.row_height,
            "icon-size" => &mut self.icon_size,
            "radius" => &mut self.radius,
            "spacing" => &mut self.spacing,
            _ => unreachable!("not a metric key: {key}"),
        };
        *slot = value;
    }
}

/// One theme file as written: only the values it sets.
#[derive(Debug, Clone, Default)]
pub(crate) struct PartialTheme {
    pub display_name: Option<String>,
    pub base: Option<String>,
    pub colors: Vec<(&'static str, Color)>,
    pub font_family: Option<String>,
    pub numbers: Vec<(&'static str, f32)>,
}

/// Parses one theme file. A syntax error returns `None`; a single bad value is skipped
/// with a warning and the rest of the file still loads. Unknown keys are ignored so
/// themes written for newer versions keep working.
pub(crate) fn parse_theme(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Option<PartialTheme> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Option<PartialTheme>, Vec<Warning>) {
        let mut warnings = Vec::new();
        let theme = parse_theme("t.toml", text, &mut warnings);
        (theme, warnings)
    }

    #[test]
    fn builtin_themes_define_every_value() {
        for id in ["dark", "light"] {
            let (theme, warnings) = parse(builtin_source(id).unwrap());
            let theme = theme.unwrap();
            assert!(warnings.is_empty(), "{id}: {warnings:?}");
            assert_eq!(theme.colors.len(), COLOR_KEYS.len(), "{id}");
            assert_eq!(theme.numbers.len(), METRIC_RANGES.len(), "{id}");
            assert!(theme.font_family.is_some(), "{id}");
            assert!(theme.base.is_none(), "{id}");
        }
    }

    #[test]
    fn reads_values() {
        let (theme, warnings) = parse(
            "name = \"Nord\"\nbase = \"light\"\n[colors]\naccent = \"#88c0d0\"\n\
             [metrics]\nrow-height = 28\nradius = 4.5\nfont-family = \"Inter\"\n",
        );
        let theme = theme.unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.display_name.as_deref(), Some("Nord"));
        assert_eq!(theme.base.as_deref(), Some("light"));
        assert_eq!(theme.colors, [("accent", Color::parse("#88c0d0").unwrap())]);
        assert_eq!(theme.numbers, [("row-height", 28.0), ("radius", 4.5)]);
        assert_eq!(theme.font_family.as_deref(), Some("Inter"));
    }

    #[test]
    fn invalid_values_are_skipped_with_warnings() {
        let (theme, warnings) = parse(
            "[colors]\naccent = \"#zzz\"\nborder = \"#123456\"\n\
             [metrics]\nrow-height = 500\nfont-size = \"big\"\nfont-family = 3\n",
        );
        let theme = theme.unwrap();
        assert_eq!(theme.colors, [("border", Color::parse("#123456").unwrap())]);
        assert!(theme.numbers.is_empty());
        assert!(theme.font_family.is_none());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(warnings.len(), 4, "{messages:?}");
        assert!(messages[0].starts_with("colors.accent:"), "{messages:?}");
        assert!(messages.iter().any(|m| m.starts_with("metrics.row-height:") && m.contains("16 to 64")));
        assert!(messages.iter().any(|m| m.starts_with("metrics.font-size:")));
        assert!(messages.iter().any(|m| m.starts_with("metrics.font-family:")));
    }

    #[test]
    fn unknown_keys_are_ignored_silently() {
        let (theme, warnings) = parse("future = 1\n[colors]\nglow = \"#ffffff\"\n[sounds]\nclick = \"x\"\n");
        assert!(theme.unwrap().colors.is_empty());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn syntax_error_returns_none_with_line() {
        let (theme, warnings) = parse("name = \"x\"\n[colors\n");
        assert!(theme.is_none());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, Some(2));
        assert_eq!(warnings[0].file, "t.toml");
    }

    #[test]
    fn empty_file_is_valid_and_empty() {
        let (theme, warnings) = parse("");
        let theme = theme.unwrap();
        assert!(theme.colors.is_empty() && theme.numbers.is_empty() && theme.base.is_none());
        assert!(warnings.is_empty());
    }
}
```

- [ ] **Step 3: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config theme`
Expected: 6 FAIL (`called Option::unwrap() on a None value`).

- [ ] **Step 4: `parse_theme`'i yaz**

```rust
pub(crate) fn parse_theme(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Option<PartialTheme> {
    let table = match text.parse::<toml::Table>() {
        Ok(table) => table,
        Err(err) => {
            warnings.push(Warning::from_toml_error(file, text, &err));
            return None;
        }
    };
    let mut theme = PartialTheme {
        display_name: table.get("name").and_then(|v| v.as_str()).map(str::to_owned),
        base: table.get("base").and_then(|v| v.as_str()).map(str::to_owned),
        ..PartialTheme::default()
    };

    if let Some(colors) = table.get("colors").and_then(|v| v.as_table()) {
        for key in COLOR_KEYS {
            let Some(value) = colors.get(key) else { continue };
            match value.as_str().and_then(Color::parse) {
                Some(color) => theme.colors.push((key, color)),
                None => warnings.push(Warning::new(
                    file,
                    format!("colors.{key}: expected \"#rrggbb\" or \"#rrggbbaa\", got {value}"),
                )),
            }
        }
    }

    if let Some(metrics) = table.get("metrics").and_then(|v| v.as_table()) {
        if let Some(value) = metrics.get("font-family") {
            match value.as_str() {
                Some(family) => theme.font_family = Some(family.to_owned()),
                None => warnings.push(Warning::new(file, format!("metrics.font-family: expected text, got {value}"))),
            }
        }
        for (key, min, max) in METRIC_RANGES {
            let Some(value) = metrics.get(key) else { continue };
            let number = value.as_integer().map(|n| n as f32).or_else(|| value.as_float().map(|f| f as f32));
            match number {
                Some(n) if (min..=max).contains(&n) => theme.numbers.push((key, n)),
                _ => warnings.push(Warning::new(
                    file,
                    format!("metrics.{key}: expected a number from {min} to {max}, got {value}"),
                )),
            }
        }
    }
    Some(theme)
}
```

- [ ] **Step 5: Testlerin geçtiğini gör**

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: tüm testler geçer (Task 1–2'ninkiler + 6 yeni).

- [ ] **Step 6: Commit**

```bash
git add crates/gezik-config
git commit -m "Parse and validate theme files; add built-in light and dark themes"
```

---

### Task 4: Tema çözümleme — `base` zinciri, gölgeleme, döngüler, yoğunluk

**Files:**
- Modify: `crates/gezik-config/src/theme.rs`

**Interfaces:**
- Consumes: Task 3'teki `parse_theme`, `builtin_source`, `ThemeColors::set`, `Metrics::set`
- Produces (Task 6 ve 7 kullanır):
  - `pub struct ResolvedTheme { pub id: String, pub name: String, pub colors: ThemeColors, pub metrics: Metrics }` (`Debug, Clone, PartialEq`)
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ThemeError { NotFound, Invalid }`
  - `pub fn resolve_theme(id: &str, user_themes: &HashMap<String, String>, warnings: &mut Vec<Warning>) -> Result<ResolvedTheme, ThemeError>` — `user_themes` anahtarları küçük harfli tema kimlikleri
  - `pub fn builtin_dark() -> ResolvedTheme`
  - `Metrics::compact(&self) -> Metrics`

- [ ] **Step 1: Başarısız testleri yaz**

`theme.rs` başına `use std::collections::HashMap;` ekle. `PartialTheme`'in altına:

```rust
/// A theme with every value filled in, ready for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    /// Lowercase file name without extension (`nord`), or `dark` / `light`.
    pub id: String,
    /// The theme's `name`, or its id when it has none.
    pub name: String,
    pub colors: ThemeColors,
    pub metrics: Metrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeError {
    /// No user theme or built-in theme has this id.
    NotFound,
    /// The theme file has a syntax error (already reported as a warning).
    Invalid,
}

/// Resolves a theme by id (case-insensitive) from the user's theme files
/// (`id → TOML text`, ids lowercase) and the built-ins, filling every value the theme
/// leaves out from its `base` chain. A missing or cyclic base falls back to `dark`.
pub fn resolve_theme(
    id: &str,
    user_themes: &HashMap<String, String>,
    warnings: &mut Vec<Warning>,
) -> Result<ResolvedTheme, ThemeError> {
    Err(ThemeError::NotFound)
}

/// The built-in dark theme: what the UI shows before any config is read.
pub fn builtin_dark() -> ResolvedTheme {
    resolve_theme("dark", &HashMap::new(), &mut Vec::new()).expect("built-in dark theme resolves")
}
```

`Metrics` impl'ine:

```rust
    /// Compact density: rows and spacing at 80%, never below the allowed minimum.
    pub fn compact(&self) -> Metrics {
        self.clone()
    }
```

Test modülüne ekle:

```rust
    fn user(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(id, text)| (id.to_string(), text.to_string())).collect()
    }

    fn hex(text: &str) -> Color {
        Color::parse(text).unwrap()
    }

    #[test]
    fn builtin_dark_resolves_completely() {
        let mut warnings = Vec::new();
        let theme = resolve_theme("dark", &HashMap::new(), &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.id, "dark");
        assert_eq!(theme.name, "Dark");
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(theme.metrics.row_height, 26.0);
        assert_eq!(builtin_dark(), theme);
    }

    #[test]
    fn partial_theme_inherits_from_its_base() {
        let themes = user(&[("nord", "base = \"light\"\n[colors]\naccent = \"#88c0d0\"\n")]);
        let theme = resolve_theme("nord", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.name, "nord");
        assert_eq!(theme.colors.accent, hex("#88c0d0"));
        assert_eq!(theme.colors.background, hex("#fafafa"));
        assert_eq!(theme.metrics.font_size, 13.0);
    }

    #[test]
    fn default_base_is_dark() {
        let themes = user(&[("mine", "[colors]\naccent = \"#ff0000\"\n")]);
        let theme = resolve_theme("mine", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
    }

    #[test]
    fn follows_multi_level_chains() {
        let themes = user(&[
            ("a", "base = \"b\"\n[colors]\naccent = \"#aaaaaa\"\n"),
            ("b", "base = \"light\"\n[colors]\nborder = \"#bbbbbb\"\naccent = \"#000000\"\n"),
        ]);
        let theme = resolve_theme("a", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.colors.accent, hex("#aaaaaa"));
        assert_eq!(theme.colors.border, hex("#bbbbbb"));
        assert_eq!(theme.colors.background, hex("#fafafa"));
    }

    #[test]
    fn user_theme_shadows_a_builtin_and_extends_it() {
        let themes = user(&[("dark", "[colors]\naccent = \"#ff00ff\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("dark", &themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.colors.accent, hex("#ff00ff"));
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
    }

    #[test]
    fn cycle_falls_back_to_dark_with_warning() {
        let themes = user(&[
            ("a", "base = \"b\"\nname = \"A\"\n[colors]\naccent = \"#aaaaaa\"\n"),
            ("b", "base = \"a\"\n"),
        ]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.name, "A");
        assert_eq!(theme.colors.accent, hex("#aaaaaa"));
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].file, "b.toml");
        assert!(warnings[0].message.contains("\"a\""), "{}", warnings[0].message);
    }

    #[test]
    fn missing_base_falls_back_to_dark_with_warning() {
        let themes = user(&[("a", "base = \"gone\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("\"gone\""));
    }

    #[test]
    fn broken_base_falls_back_to_dark() {
        let themes = user(&[("a", "base = \"bad\"\n"), ("bad", "[colors\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].file, "bad.toml");
        assert_eq!(warnings[0].line, Some(1));
    }

    #[test]
    fn unknown_theme_is_not_found() {
        assert_eq!(resolve_theme("nope", &HashMap::new(), &mut Vec::new()), Err(ThemeError::NotFound));
    }

    #[test]
    fn broken_theme_is_invalid() {
        let themes = user(&[("bad", "[colors\n")]);
        let mut warnings = Vec::new();
        assert_eq!(resolve_theme("bad", &themes, &mut warnings), Err(ThemeError::Invalid));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn ids_are_case_insensitive() {
        let themes = user(&[("nord", "[colors]\naccent = \"#88c0d0\"\n")]);
        let theme = resolve_theme("Nord", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.id, "nord");
        assert_eq!(theme.colors.accent, hex("#88c0d0"));
    }

    #[test]
    fn compact_scales_rows_and_spacing_with_a_floor() {
        let metrics = builtin_dark().metrics;
        let compact = metrics.compact();
        assert_eq!(compact.row_height, 21.0);
        assert_eq!(compact.spacing, 5.0);
        assert_eq!(compact.font_size, metrics.font_size);
        let tiny = Metrics { row_height: 16.0, ..metrics };
        assert_eq!(tiny.compact().row_height, 16.0);
    }
```

- [ ] **Step 2: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config theme`
Expected: Task 3 testleri geçer; yeni 12 testten `unknown_theme_is_not_found` dışındakiler FAIL.

- [ ] **Step 3: Çözümlemeyi ve yoğunluğu yaz**

`Metrics::compact`:

```rust
    pub fn compact(&self) -> Metrics {
        Metrics {
            row_height: (self.row_height * 0.8).round().max(16.0),
            spacing: (self.spacing * 0.8).round(),
            ..self.clone()
        }
    }
```

`resolve_theme` ve yardımcıları:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    User,
    Builtin,
}

/// Finds a theme, preferring the user's file over a built-in with the same id. A theme
/// already in the chain is skipped, which lets a user `dark` extend the built-in `dark`
/// and turns real cycles into "not found".
fn lookup<'a>(
    id: &str,
    user_themes: &'a HashMap<String, String>,
    visited: &[(String, Source)],
) -> Option<(Source, &'a str)> {
    let seen = |source| visited.iter().any(|(v, s)| v == id && *s == source);
    if let Some(text) = user_themes.get(id)
        && !seen(Source::User)
    {
        return Some((Source::User, text));
    }
    builtin_source(id).filter(|_| !seen(Source::Builtin)).map(|text| (Source::Builtin, text))
}

fn label(id: &str, source: Source) -> String {
    match source {
        Source::User => format!("{id}.toml"),
        Source::Builtin => format!("built-in theme \"{id}\""),
    }
}

fn builtin(id: &str) -> PartialTheme {
    let text = builtin_source(id).expect("known built-in theme");
    parse_theme(id, text, &mut Vec::new()).expect("built-in themes are valid")
}

pub fn resolve_theme(
    id: &str,
    user_themes: &HashMap<String, String>,
    warnings: &mut Vec<Warning>,
) -> Result<ResolvedTheme, ThemeError> {
    let id = id.to_lowercase();
    let (source, text) = lookup(&id, user_themes, &[]).ok_or(ThemeError::NotFound)?;
    let first = parse_theme(&label(&id, source), text, warnings).ok_or(ThemeError::Invalid)?;
    let name = first.display_name.clone().unwrap_or_else(|| id.clone());

    let mut chain = vec![first];
    let mut visited = vec![(id.clone(), source)];
    loop {
        let (current, current_source) = visited.last().cloned().expect("chain is never empty");
        let base = chain.last().and_then(|theme| theme.base.clone());
        // Built-ins define every value, so the chain ends at one without a base.
        if current_source == Source::Builtin && base.is_none() {
            break;
        }
        let base = base.unwrap_or_else(|| "dark".to_owned()).to_lowercase();
        let Some((source, text)) = lookup(&base, user_themes, &visited) else {
            warnings.push(Warning::new(
                label(&current, current_source),
                format!("base theme \"{base}\" not found or forms a cycle; using \"dark\" instead"),
            ));
            chain.push(builtin("dark"));
            break;
        };
        match parse_theme(&label(&base, source), text, warnings) {
            Some(theme) => {
                chain.push(theme);
                visited.push((base, source));
            }
            None => {
                chain.push(builtin("dark"));
                break;
            }
        }
    }

    let mut resolved = ResolvedTheme { id, name, colors: ThemeColors::default(), metrics: Metrics::default() };
    for theme in chain.iter().rev() {
        for (key, color) in &theme.colors {
            resolved.colors.set(key, *color);
        }
        if let Some(family) = &theme.font_family {
            resolved.metrics.font_family = family.clone();
        }
        for (key, value) in &theme.numbers {
            resolved.metrics.set(key, *value);
        }
    }
    Ok(resolved)
}
```

- [ ] **Step 4: Testlerin geçtiğini gör**

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: tüm testler geçer.

- [ ] **Step 5: Commit**

```bash
git add crates/gezik-config
git commit -m "Resolve themes through base chains with cycle and shadowing rules"
```

---

### Task 5: `Settings` (taşınabilir) ve `State` (makineye özel)

**Files:**
- Create: `crates/gezik-config/src/settings.rs`
- Modify: `crates/gezik-config/src/lib.rs` (`pub mod settings;`)

**Interfaces:**
- Consumes: `Warning`
- Produces (Task 6, 9 kullanır):
  - `pub enum ThemeChoice { Auto, Named(String) }`, `pub enum SidebarPosition { Left, Right, Hidden }`, `pub enum Density { Compact, Comfortable }`
  - `pub struct Settings { pub theme: ThemeChoice, pub theme_light: String, pub theme_dark: String, pub sidebar: SidebarPosition, pub density: Density }` + `Default`
  - `Settings::parse(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Settings`, `Settings::active_theme(&self, system_dark: bool) -> &str`
  - `pub struct WindowState { pub width: u32, pub height: u32, pub x: Option<i32>, pub y: Option<i32> }` (`Copy`) — boyut mantıksal piksel, konum fiziksel piksel
  - `pub struct State { pub window: Option<WindowState> }` + `Default`, `State::parse(&str) -> State`, `State::to_toml(&self) -> String`

- [ ] **Step 1: Başarısız testleri yaz**

`lib.rs`'e `pub mod settings;` ekle. `crates/gezik-config/src/settings.rs`:

```rust
//! `settings.toml` (portable, may be synced) and `state.toml` (this machine only).

use crate::Warning;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Follow the system light/dark mode.
    Auto,
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPosition {
    Left,
    Right,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub theme_light: String,
    pub theme_dark: String,
    pub sidebar: SidebarPosition,
    pub density: Density,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Auto,
            theme_light: "light".to_owned(),
            theme_dark: "dark".to_owned(),
            sidebar: SidebarPosition::Left,
            density: Density::Comfortable,
        }
    }
}

impl Settings {
    /// Reads `settings.toml`. Anything missing or invalid keeps its default; problems
    /// become warnings. A syntax error yields all defaults.
    pub fn parse(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Settings {
        Settings::default()
    }

    /// The theme id to show right now.
    pub fn active_theme(&self, system_dark: bool) -> &str {
        ""
    }
}

/// Size in logical pixels, position in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub window: Option<WindowState>,
}

impl State {
    /// The app writes this file itself, so a damaged one just yields the defaults.
    pub fn parse(text: &str) -> State {
        State::default()
    }

    pub fn to_toml(&self) -> String {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Settings, Vec<Warning>) {
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        (settings, warnings)
    }

    #[test]
    fn defaults_when_empty() {
        let (settings, warnings) = parse("");
        assert_eq!(settings, Settings::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn reads_all_values() {
        let (settings, warnings) = parse(
            "theme = \"nord\"\ntheme-light = \"paper\"\ntheme-dark = \"ink\"\n\
             [layout]\nsidebar = \"right\"\ndensity = \"compact\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            settings,
            Settings {
                theme: ThemeChoice::Named("nord".to_owned()),
                theme_light: "paper".to_owned(),
                theme_dark: "ink".to_owned(),
                sidebar: SidebarPosition::Right,
                density: Density::Compact,
            }
        );
    }

    #[test]
    fn invalid_values_keep_defaults_with_warnings() {
        let (settings, warnings) = parse("theme = 5\n[layout]\nsidebar = \"top\"\ndensity = \"tiny\"\n");
        assert_eq!(settings, Settings::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].starts_with("theme:"));
        assert!(messages[1].starts_with("layout.sidebar:") && messages[1].contains("\"top\""));
        assert!(messages[2].starts_with("layout.density:"));
    }

    #[test]
    fn syntax_error_gives_defaults_with_line() {
        let (settings, warnings) = parse("theme = \"nord\"\n[layout\n");
        assert_eq!(settings, Settings::default());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, Some(2));
    }

    #[test]
    fn auto_mode_follows_the_system() {
        let settings = Settings::default();
        assert_eq!(settings.active_theme(true), "dark");
        assert_eq!(settings.active_theme(false), "light");
    }

    #[test]
    fn named_theme_ignores_the_system() {
        let settings = Settings { theme: ThemeChoice::Named("nord".to_owned()), ..Settings::default() };
        assert_eq!(settings.active_theme(true), "nord");
        assert_eq!(settings.active_theme(false), "nord");
    }

    #[test]
    fn state_round_trips() {
        let state = State { window: Some(WindowState { width: 1000, height: 700, x: Some(-50), y: Some(30) }) };
        assert_eq!(State::parse(&state.to_toml()), state);
        let no_position = State { window: Some(WindowState { width: 800, height: 600, x: None, y: None }) };
        assert_eq!(State::parse(&no_position.to_toml()), no_position);
    }

    #[test]
    fn state_rejects_tiny_or_broken_values() {
        assert_eq!(State::parse("[window]\nwidth = 10\nheight = 10\n"), State::default());
        assert_eq!(State::parse("[window]\nwidth = -900\nheight = 600\n"), State::default());
        assert_eq!(State::parse("garbage ["), State::default());
        assert_eq!(State::parse(""), State::default());
    }
}
```

- [ ] **Step 2: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config settings`
Expected: `defaults_when_empty` ve `state_rejects_tiny_or_broken_values` dışındakiler FAIL.

- [ ] **Step 3: Uygulamayı yaz**

```rust
impl Settings {
    pub fn parse(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Settings {
        let mut settings = Settings::default();
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(err) => {
                warnings.push(Warning::from_toml_error(file, text, &err));
                return settings;
            }
        };
        let mut text_value = |table: &toml::Table, key: &str, path: &str| -> Option<String> {
            let value = table.get(key)?;
            let text = value.as_str().map(str::to_owned);
            if text.is_none() {
                warnings.push(Warning::new(file, format!("{path}: expected text, got {value}")));
            }
            text
        };

        if let Some(theme) = text_value(&table, "theme", "theme") {
            settings.theme = if theme == "auto" { ThemeChoice::Auto } else { ThemeChoice::Named(theme) };
        }
        if let Some(theme) = text_value(&table, "theme-light", "theme-light") {
            settings.theme_light = theme;
        }
        if let Some(theme) = text_value(&table, "theme-dark", "theme-dark") {
            settings.theme_dark = theme;
        }

        let Some(layout) = table.get("layout").and_then(|v| v.as_table()) else { return settings };
        let sidebar = text_value(layout, "sidebar", "layout.sidebar");
        let density = text_value(layout, "density", "layout.density");
        match sidebar.as_deref() {
            None => {}
            Some("left") => settings.sidebar = SidebarPosition::Left,
            Some("right") => settings.sidebar = SidebarPosition::Right,
            Some("hidden") => settings.sidebar = SidebarPosition::Hidden,
            Some(other) => warnings.push(Warning::new(
                file,
                format!("layout.sidebar: expected \"left\", \"right\" or \"hidden\", got \"{other}\""),
            )),
        }
        match density.as_deref() {
            None => {}
            Some("compact") => settings.density = Density::Compact,
            Some("comfortable") => settings.density = Density::Comfortable,
            Some(other) => warnings.push(Warning::new(
                file,
                format!("layout.density: expected \"compact\" or \"comfortable\", got \"{other}\""),
            )),
        }
        settings
    }

    pub fn active_theme(&self, system_dark: bool) -> &str {
        match &self.theme {
            ThemeChoice::Auto if system_dark => &self.theme_dark,
            ThemeChoice::Auto => &self.theme_light,
            ThemeChoice::Named(id) => id,
        }
    }
}

impl State {
    pub fn parse(text: &str) -> State {
        let Ok(table) = text.parse::<toml::Table>() else { return State::default() };
        let window = table.get("window").and_then(|v| v.as_table()).and_then(|window| {
            let int = |key: &str| window.get(key).and_then(|v| v.as_integer());
            let coord = |key: &str| int(key).and_then(|v| i32::try_from(v).ok());
            Some(WindowState {
                width: u32::try_from(int("width")?).ok().filter(|w| *w >= 200)?,
                height: u32::try_from(int("height")?).ok().filter(|h| *h >= 150)?,
                x: coord("x"),
                y: coord("y"),
            })
        });
        State { window }
    }

    pub fn to_toml(&self) -> String {
        let mut root = toml::Table::new();
        if let Some(w) = self.window {
            let mut window = toml::Table::new();
            window.insert("width".into(), toml::Value::Integer(w.width.into()));
            window.insert("height".into(), toml::Value::Integer(w.height.into()));
            if let (Some(x), Some(y)) = (w.x, w.y) {
                window.insert("x".into(), toml::Value::Integer(x.into()));
                window.insert("y".into(), toml::Value::Integer(y.into()));
            }
            root.insert("window".into(), toml::Value::Table(window));
        }
        root.to_string()
    }
}
```

Not: `text_value` closure'ı `warnings`'i mutable ödünç alır; `sidebar` ve `density` değerleri `match`'lerden önce okunur, böylece `match` kollarındaki `warnings.push` closure'la çakışmaz. Derleyici yine de ödünç alma hatası verirse, closure'ı `fn text_value(table, key, path, file, warnings) -> Option<String>` imzalı serbest bir fonksiyona çevir; davranış aynı kalır.

- [ ] **Step 4: Testlerin geçtiğini gör**

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: tüm testler geçer.

- [ ] **Step 5: Commit**

```bash
git add crates/gezik-config
git commit -m "Add portable Settings and machine-local State"
```

---

### Task 6: `ConfigStore` — klasör düzeni, ilk açılış, okuma, `resolve`

**Files:**
- Create: `crates/gezik-config/templates/settings.toml`
- Create: `crates/gezik-config/templates/example.toml`
- Create: `crates/gezik-config/src/store.rs`
- Modify: `crates/gezik-config/src/lib.rs` (`pub mod store;`)

**Interfaces:**
- Consumes: `paths::{config_dir, write_atomic}`, `Settings`, `State`, `Density`, `resolve_theme`, `ThemeError`, `ResolvedTheme`, `Warning`
- Produces (Task 7–9 kullanır):
  - `pub struct ConfigFiles { pub settings: Option<String>, pub themes: HashMap<String, String>, pub warnings: Vec<Warning> }` (`Debug, Clone, Default`)
  - `pub struct ConfigStore` (`Debug, Clone`): `new(PathBuf)`, `system() -> Option<Self>`, `dir(&self) -> &Path`, `ensure_initialized(&self) -> io::Result<()>`, `read_files(&self) -> ConfigFiles`, `load_state(&self) -> State`, `save_state(&self, &State) -> io::Result<()>`, `is_config_file(&self, path: &Path) -> bool`
  - `pub struct Loaded { pub settings: Settings, pub theme: Option<ResolvedTheme>, pub warnings: Vec<Warning> }`
  - `pub fn resolve(files: &ConfigFiles, system_dark: bool) -> Loaded`

- [ ] **Step 1: Şablonları oluştur**

`crates/gezik-config/templates/settings.toml`:

```toml
# Gezik settings. This file is portable: copy it to another computer (Windows,
# macOS or Linux) and it works the same. Changes apply as soon as you save.

# "auto" follows the system light/dark mode. Or name a theme: "light", "dark",
# or the file name of a theme in the themes folder (themes/nord.toml -> "nord").
theme = "auto"
theme-light = "light"
theme-dark = "dark"

[layout]
# left | right | hidden
sidebar = "left"
# compact | comfortable
density = "comfortable"
```

`crates/gezik-config/templates/example.toml`:

```toml
# Example theme. Copy this file, rename it (e.g. my-theme.toml) and select it in
# settings.toml with:  theme = "my-theme"
# Every value is optional: anything you leave out comes from the base theme.
# Changes show up as soon as you save.

name = "Example (Nord)"
base = "dark"            # "dark", "light" or another theme's file name

[colors]                 # "#rrggbb" or "#rrggbbaa" (with transparency)
background = "#2e3440"   # file list
surface = "#3b4252"      # toolbar and status bar
foreground = "#eceff4"   # main text
foreground-muted = "#9aa3b5"  # sizes and other secondary text
border = "#434c5e"
accent = "#88c0d0"       # focus ring
accent-foreground = "#2e3440"
selection = "#434c5e"
selection-foreground = "#eceff4"
hover = "#4c566a80"
folder-icon = "#ebcb8b"
file-icon = "#9aa3ad"
danger = "#bf616a"       # warnings and destructive actions

[metrics]
# font-family = "Inter"  # leave out or "" for the system font
font-size = 13           # 8-32
row-height = 26          # 16-64
icon-size = 16           # 12-48
radius = 6               # 0-16
spacing = 6              # 0-24
```

- [ ] **Step 2: Başarısız testleri yaz**

`lib.rs`'e `pub mod store;` ekle. `crates/gezik-config/src/store.rs`:

```rust
//! The config folder on disk: layout, first-run files, reading and resolving.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::Warning;
use crate::paths::{config_dir, write_atomic};
use crate::settings::{Density, Settings, State};
use crate::theme::{ResolvedTheme, ThemeError, resolve_theme};

const SETTINGS_TEMPLATE: &str = include_str!("../templates/settings.toml");
const THEME_TEMPLATE: &str = include_str!("../templates/example.toml");

/// Raw contents of the config folder. Reading does I/O, so it happens off the UI thread;
/// resolving it ([`resolve`]) is cheap.
#[derive(Debug, Clone, Default)]
pub struct ConfigFiles {
    pub settings: Option<String>,
    /// Lowercase theme id (file name without `.toml`) → TOML text.
    pub themes: HashMap<String, String>,
    /// Problems found while reading: unreadable files, reserved names.
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The store in the standard per-user location, if the OS has one.
    pub fn system() -> Option<Self> {
        config_dir().map(Self::new)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.toml")
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("state.toml")
    }

    fn themes_dir(&self) -> PathBuf {
        self.dir.join("themes")
    }

    /// Creates the folder layout and commented starter files on first run. Existing
    /// files are never touched.
    pub fn ensure_initialized(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn read_files(&self) -> ConfigFiles {
        ConfigFiles::default()
    }

    pub fn load_state(&self) -> State {
        State::default()
    }

    pub fn save_state(&self, state: &State) -> io::Result<()> {
        Ok(())
    }

    /// Whether a change to `path` should reload the config: `settings.toml` or a theme
    /// file, but not `state.toml` or editor temp files.
    pub fn is_config_file(&self, path: &Path) -> bool {
        false
    }
}

/// Everything the UI needs after (re)loading the config folder.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub settings: Settings,
    /// `None` when the selected theme has a syntax error: keep showing the current one.
    pub theme: Option<ResolvedTheme>,
    pub warnings: Vec<Warning>,
}

pub fn resolve(files: &ConfigFiles, system_dark: bool) -> Loaded {
    Loaded { settings: Settings::default(), theme: None, warnings: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;
    use crate::settings::WindowState;

    fn store(name: &str) -> ConfigStore {
        ConfigStore::new(crate::test_dir(name))
    }

    fn write(store: &ConfigStore, relative: &str, text: &str) {
        let path = store.dir().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn first_run_creates_starter_files_once() {
        let store = store("init");
        store.ensure_initialized().unwrap();
        assert!(store.dir().join("settings.toml").is_file());
        assert!(store.dir().join("themes/example.toml").is_file());

        write(&store, "settings.toml", "theme = \"light\"\n");
        store.ensure_initialized().unwrap();
        assert_eq!(std::fs::read_to_string(store.dir().join("settings.toml")).unwrap(), "theme = \"light\"\n");
    }

    #[test]
    fn starter_files_are_valid() {
        let store = store("starter");
        store.ensure_initialized().unwrap();
        let files = store.read_files();
        let loaded = resolve(&files, false);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.theme.unwrap().id, "light");

        let mut warnings = Vec::new();
        let example = resolve_theme("example", &files.themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(example.name, "Example (Nord)");
    }

    #[test]
    fn strips_utf8_bom_and_matches_theme_names_case_insensitively() {
        let store = store("bom");
        write(&store, "settings.toml", "\u{feff}theme = \"nord\"\n");
        write(&store, "themes/Nord.toml", "\u{feff}[colors]\naccent = \"#88c0d0\"\n");
        let loaded = resolve(&store.read_files(), true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let theme = loaded.theme.unwrap();
        assert_eq!(theme.id, "nord");
        assert_eq!(theme.colors.accent, Color::parse("#88c0d0").unwrap());
    }

    #[test]
    fn reserved_auto_theme_is_skipped_with_warning() {
        let store = store("auto");
        write(&store, "themes/auto.toml", "");
        let files = store.read_files();
        assert!(!files.themes.contains_key("auto"));
        assert_eq!(files.warnings.len(), 1);
        assert_eq!(files.warnings[0].file, "auto.toml");
    }

    #[test]
    fn missing_settings_file_means_defaults() {
        let loaded = resolve(&store("nosettings").read_files(), true);
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.theme.unwrap().id, "dark");
    }

    #[test]
    fn missing_selected_theme_falls_back_to_dark() {
        let store = store("missing");
        write(&store, "settings.toml", "theme = \"gone\"\n");
        let loaded = resolve(&store.read_files(), false);
        assert_eq!(loaded.theme.unwrap().id, "dark");
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].message.contains("\"gone\""));
    }

    #[test]
    fn broken_selected_theme_keeps_current() {
        let store = store("broken");
        write(&store, "settings.toml", "theme = \"bad\"\n");
        write(&store, "themes/bad.toml", "[colors\n");
        let loaded = resolve(&store.read_files(), false);
        assert!(loaded.theme.is_none());
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(loaded.warnings[0].file, "bad.toml");
        assert_eq!(loaded.warnings[0].line, Some(1));
    }

    #[test]
    fn compact_density_is_applied() {
        let store = store("density");
        write(&store, "settings.toml", "[layout]\ndensity = \"compact\"\n");
        let theme = resolve(&store.read_files(), true).theme.unwrap();
        assert_eq!(theme.metrics.row_height, 21.0);
    }

    #[test]
    fn state_round_trips_through_disk() {
        let store = store("state");
        assert_eq!(store.load_state(), State::default());
        let state = State { window: Some(WindowState { width: 1000, height: 700, x: Some(10), y: Some(20) }) };
        store.save_state(&state).unwrap();
        assert_eq!(store.load_state(), state);
        let names: Vec<_> = std::fs::read_dir(store.dir()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"]);
    }

    #[test]
    fn recognizes_config_files() {
        let store = store("watch");
        let root = store.dir();
        assert!(store.is_config_file(&root.join("settings.toml")));
        assert!(store.is_config_file(&root.join("themes").join("nord.toml")));
        assert!(!store.is_config_file(&root.join("state.toml")));
        assert!(!store.is_config_file(&root.join(".settings.toml.tmp")));
        assert!(!store.is_config_file(&root.join("themes").join(".nord.toml.tmp")));
        assert!(!store.is_config_file(&root.join("themes").join(".nord.toml.swp")));
        assert!(!store.is_config_file(&root.join("themes").join("nord.toml~")));
        assert!(!store.is_config_file(&root.join("themes").join("old").join("nord.toml")));
        assert!(!store.is_config_file(Path::new("/elsewhere/settings.toml")));
    }
}
```

- [ ] **Step 3: Testlerin başarısız olduğunu gör**

Run: `~/.cargo/bin/cargo test -p gezik-config store`
Expected: `reserved_auto_theme_is_skipped_with_warning` dahil çoğu test FAIL.

- [ ] **Step 4: Uygulamayı yaz**

```rust
impl ConfigStore {
    pub fn ensure_initialized(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.themes_dir())?;
        write_if_missing(&self.settings_path(), SETTINGS_TEMPLATE)?;
        write_if_missing(&self.themes_dir().join("example.toml"), THEME_TEMPLATE)
    }

    pub fn read_files(&self) -> ConfigFiles {
        let mut files = ConfigFiles::default();
        match read_text(&self.settings_path()) {
            Ok(text) => files.settings = Some(text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => files.warnings.push(Warning::new("settings.toml", format!("cannot read: {err}"))),
        }

        let Ok(entries) = std::fs::read_dir(self.themes_dir()) else { return files };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let file = format!("{stem}.toml");
            let id = stem.to_lowercase();
            if id == "auto" {
                files.warnings.push(Warning::new(file, "\"auto\" is a reserved name; rename this theme"));
                continue;
            }
            match read_text(&path) {
                Ok(text) => {
                    files.themes.insert(id, text);
                }
                Err(err) => files.warnings.push(Warning::new(file, format!("cannot read: {err}"))),
            }
        }
        files
    }

    pub fn load_state(&self) -> State {
        read_text(&self.state_path()).map(|text| State::parse(&text)).unwrap_or_default()
    }

    pub fn save_state(&self, state: &State) -> io::Result<()> {
        write_atomic(&self.state_path(), &state.to_toml())
    }

    pub fn is_config_file(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.dir) else { return false };
        let parts: Vec<String> =
            relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        match parts.as_slice() {
            [name] => name == "settings.toml",
            [dir, name] => dir == "themes" && name.ends_with(".toml") && !name.starts_with('.'),
            _ => false,
        }
    }
}

/// Reads a text file, dropping the UTF-8 byte order mark some Windows editors add.
fn read_text(path: &Path) -> io::Result<String> {
    let text = std::fs::read_to_string(path)?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

fn write_if_missing(path: &Path, contents: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    write_atomic(path, contents)
}

pub fn resolve(files: &ConfigFiles, system_dark: bool) -> Loaded {
    let mut warnings = files.warnings.clone();
    let settings = files
        .settings
        .as_deref()
        .map(|text| Settings::parse("settings.toml", text, &mut warnings))
        .unwrap_or_default();

    let id = settings.active_theme(system_dark).to_owned();
    let theme = match resolve_theme(&id, &files.themes, &mut warnings) {
        Ok(theme) => Some(theme),
        Err(ThemeError::Invalid) => None,
        Err(ThemeError::NotFound) => {
            warnings.push(Warning::new("settings.toml", format!("theme \"{id}\" not found; using \"dark\"")));
            resolve_theme("dark", &files.themes, &mut warnings).ok()
        }
    };
    let theme = theme.map(|mut theme| {
        if settings.density == Density::Compact {
            theme.metrics = theme.metrics.compact();
        }
        theme
    });
    Loaded { settings, theme, warnings }
}
```

- [ ] **Step 5: Testlerin geçtiğini gör**

Run: `~/.cargo/bin/cargo test -p gezik-config`
Expected: tüm testler geçer, derleme uyarısı yok.

Run: `~/.cargo/bin/cargo clippy -p gezik-config --all-targets`
Expected: uyarı yok (varsa düzelt).

- [ ] **Step 6: Commit**

```bash
git add crates/gezik-config
git commit -m "Add ConfigStore: first-run files, reading, resolving and watch filter"
```

---

### Task 7: Uygulama — `Theme` global, kendi bileşenlerimiz, açılışta tema

**Files:**
- Modify: `crates/gezik/Cargo.toml`
- Create: `crates/gezik/ui/theme.slint`
- Create: `crates/gezik/ui/widgets/button.slint`
- Create: `crates/gezik/ui/widgets/text-field.slint`
- Modify: `crates/gezik/ui/app.slint` (tamamen yeniden yazılır)
- Create: `crates/gezik/src/theme_bridge.rs`
- Modify: `crates/gezik/src/main.rs`
- Create: `scripts/perf/screenshot.ps1`

**Interfaces:**
- Consumes: `ConfigStore::{system, ensure_initialized, read_files, dir}`, `store::{resolve, ConfigFiles}`, `theme::{builtin_dark, ResolvedTheme}`, `Warning`
- Produces (Task 8–9 kullanır):
  - Slint: `AppWindow` üzerinde `in property <string> notice`, `out property <bool> system-dark`, `callback system-scheme-changed()`
  - Rust: `theme_bridge::apply(window: &AppWindow, theme: &ResolvedTheme)`, `main.rs` içinde `fn apply_config(window: &AppWindow, files: &ConfigFiles)` ve `fn notice_text(&[Warning]) -> String`

- [ ] **Step 1: Bağımlılığı ekle**

`crates/gezik/Cargo.toml` `[dependencies]` bölümüne:

```toml
gezik-config.workspace = true
```

- [ ] **Step 2: `Theme` global'ini ve bileşenleri yaz**

`crates/gezik/ui/theme.slint`:

```slint
// Every color and size in the UI comes from here. Rust (theme_bridge.rs) sets these
// values at startup and whenever the theme or settings change.
export global Theme {
    in property <color> background;
    in property <color> surface;
    in property <color> foreground;
    in property <color> foreground-muted;
    in property <color> border;
    in property <color> accent;
    in property <color> accent-foreground;
    in property <color> selection;
    in property <color> selection-foreground;
    in property <color> hover;
    in property <color> folder-icon;
    in property <color> file-icon;
    in property <color> danger;

    // Empty means the system font.
    in property <string> font-family;
    in property <length> font-size: 13px;
    in property <length> row-height: 26px;
    in property <length> icon-size: 16px;
    in property <length> radius: 6px;
    in property <length> spacing: 6px;
}
```

`crates/gezik/ui/widgets/button.slint`:

```slint
import { Theme } from "../theme.slint";

// A square toolbar button. Replaces the std-widgets Button, whose colors cannot be themed.
export component ToolButton inherits Rectangle {
    in property <string> text;
    in property <bool> enabled: true;
    callback clicked();

    height: Theme.row-height + 4px;
    width: self.height;
    border-radius: Theme.radius;
    background: touch.has-hover && root.enabled ? Theme.hover : transparent;

    Text {
        width: parent.width;
        height: parent.height;
        text: root.text;
        color: Theme.foreground;
        opacity: root.enabled ? 1 : 0.35;
        horizontal-alignment: center;
        vertical-alignment: center;
    }

    touch := TouchArea {
        width: parent.width;
        height: parent.height;
        enabled: root.enabled;
        clicked => { root.clicked(); }
    }
}
```

`crates/gezik/ui/widgets/text-field.slint`:

```slint
import { Theme } from "../theme.slint";

// A single-line text box. Replaces the std-widgets LineEdit, whose colors cannot be themed.
export component TextField inherits Rectangle {
    in-out property <string> text;
    callback accepted(string);

    height: Theme.row-height + 4px;
    horizontal-stretch: 1;
    border-radius: Theme.radius;
    border-width: 1px;
    border-color: input.has-focus ? Theme.accent : Theme.border;
    background: Theme.background;

    input := TextInput {
        x: Theme.spacing + 2px;
        width: parent.width - 2 * (Theme.spacing + 2px);
        height: parent.height;
        vertical-alignment: center;
        single-line: true;
        text <=> root.text;
        color: Theme.foreground;
        selection-background-color: Theme.selection;
        selection-foreground-color: Theme.selection-foreground;
        accepted => { root.accepted(self.text); }
    }
}
```

- [ ] **Step 3: `app.slint`'i Theme'e taşı**

`crates/gezik/ui/app.slint` (tamamı):

```slint
import { ListView, Palette } from "std-widgets.slint";
import { Theme } from "theme.slint";
import { ToolButton } from "widgets/button.slint";
import { TextField } from "widgets/text-field.slint";

// Re-exported so Rust can set it with `window.global::<Theme>()`.
export { Theme }

export struct FileRow {
    name: string,
    is-dir: bool,
    size: string,
}

export component AppWindow inherits Window {
    title: "Gezik";
    preferred-width: 900px;
    preferred-height: 600px;
    background: Theme.background;
    default-font-family: Theme.font-family;
    default-font-size: Theme.font-size;

    in property <[FileRow]> rows;
    in-out property <string> current-path;
    in property <string> status;
    // Settings/theme problems, shown on the right of the status bar.
    in property <string> notice;
    in property <bool> can-go-back;
    in-out property <int> selected: -1;

    // Follows the OS light/dark mode (Palette is left to the system on purpose; see
    // theme_bridge.rs). Rust re-resolves the theme when it flips.
    out property <bool> system-dark: Palette.color-scheme == ColorScheme.dark;
    changed system-dark => { root.system-scheme-changed(); }

    callback open-row(int);
    callback go-back();
    callback go-up();
    callback navigate(string);
    callback system-scheme-changed();

    VerticalLayout {
        Rectangle {
            background: Theme.surface;
            HorizontalLayout {
                padding: Theme.spacing;
                spacing: Theme.spacing;
                ToolButton {
                    text: "←";
                    enabled: root.can-go-back;
                    clicked => { root.go-back(); }
                }
                ToolButton {
                    text: "↑";
                    clicked => { root.go-up(); }
                }
                TextField {
                    text <=> root.current-path;
                    accepted(path) => { root.navigate(path); }
                }
            }
        }
        Rectangle { height: 1px; background: Theme.border; }

        // ListView only instantiates the rows that are visible, so huge folders stay cheap.
        ListView {
            for row[i] in root.rows: Rectangle {
                height: Theme.row-height;
                background: i == root.selected ? Theme.selection : touch.has-hover ? Theme.hover : transparent;

                touch := TouchArea {
                    clicked => { root.selected = i; }
                    double-clicked => { root.open-row(i); }
                }

                HorizontalLayout {
                    padding-left: Theme.spacing + 4px;
                    padding-right: Theme.spacing + 4px;
                    spacing: Theme.spacing + 2px;
                    VerticalLayout {
                        alignment: center;
                        Rectangle {
                            width: Theme.icon-size * 0.75;
                            height: self.width;
                            border-radius: 2px;
                            background: row.is-dir ? Theme.folder-icon : Theme.file-icon;
                        }
                    }
                    Text {
                        text: row.name;
                        vertical-alignment: center;
                        horizontal-stretch: 1;
                        overflow: elide;
                        color: i == root.selected ? Theme.selection-foreground : Theme.foreground;
                    }
                    Text {
                        text: row.size;
                        width: 90px;
                        horizontal-alignment: right;
                        vertical-alignment: center;
                        color: i == root.selected ? Theme.selection-foreground : Theme.foreground-muted;
                    }
                }
            }
        }

        Rectangle { height: 1px; background: Theme.border; }
        Rectangle {
            height: Theme.row-height;
            background: Theme.surface;
            HorizontalLayout {
                padding-left: Theme.spacing + 4px;
                padding-right: Theme.spacing + 4px;
                spacing: Theme.spacing * 2;
                Text {
                    text: root.status;
                    vertical-alignment: center;
                    color: Theme.foreground-muted;
                }
                Text {
                    text: root.notice;
                    horizontal-stretch: 1;
                    horizontal-alignment: right;
                    vertical-alignment: center;
                    overflow: elide;
                    color: Theme.danger;
                }
            }
        }
    }
}
```

- [ ] **Step 4: `theme_bridge.rs`'i yaz**

`crates/gezik/src/theme_bridge.rs`:

```rust
//! Copies a resolved theme into the Slint `Theme` global.
//!
//! Slint's own `Palette` is read-only and is left to follow the system light/dark mode;
//! only std-widgets we still use (the ListView scrollbar) take their colors from it.

use gezik_config::Color;
use gezik_config::theme::ResolvedTheme;
use slint::ComponentHandle;

use crate::{AppWindow, Theme};

fn color(c: Color) -> slint::Color {
    slint::Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

/// No validation here: `gezik-config` guarantees every value is present and in range.
pub fn apply(window: &AppWindow, theme: &ResolvedTheme) {
    let global = window.global::<Theme>();
    let c = &theme.colors;
    global.set_background(color(c.background));
    global.set_surface(color(c.surface));
    global.set_foreground(color(c.foreground));
    global.set_foreground_muted(color(c.foreground_muted));
    global.set_border(color(c.border));
    global.set_accent(color(c.accent));
    global.set_accent_foreground(color(c.accent_foreground));
    global.set_selection(color(c.selection));
    global.set_selection_foreground(color(c.selection_foreground));
    global.set_hover(color(c.hover));
    global.set_folder_icon(color(c.folder_icon));
    global.set_file_icon(color(c.file_icon));
    global.set_danger(color(c.danger));

    let m = &theme.metrics;
    global.set_font_family(m.font_family.as_str().into());
    global.set_font_size(m.font_size);
    global.set_row_height(m.row_height);
    global.set_icon_size(m.icon_size);
    global.set_radius(m.radius);
    global.set_spacing(m.spacing);
}
```

- [ ] **Step 5: `main.rs`'te yapılandırmayı açılışta uygula**

`crates/gezik/src/main.rs` başına, `#![cfg_attr(...)]` satırının altına:

```rust
mod theme_bridge;
```

`use` bloğuna ekle:

```rust
use gezik_config::Warning;
use gezik_config::store::{self, ConfigFiles, ConfigStore};
use gezik_config::theme;
```

`fn home_dir()`'in altına ekle:

```rust
/// Resolves settings + theme from `files` and shows them. No I/O, so it runs on the UI
/// thread at startup, after config files change and when the system theme flips.
fn apply_config(window: &AppWindow, files: &ConfigFiles) {
    let loaded = store::resolve(files, window.get_system_dark());
    if let Some(theme) = &loaded.theme {
        theme_bridge::apply(window, theme);
    }
    for warning in &loaded.warnings {
        eprintln!("gezik: {warning}");
    }
    window.set_notice(notice_text(&loaded.warnings).into());
}

/// The first warning, plus how many more there are.
fn notice_text(warnings: &[Warning]) -> String {
    match warnings {
        [] => String::new(),
        [only] => only.to_string(),
        [first, rest @ ..] => format!("{first} (+{} more)", rest.len()),
    }
}
```

`fn main()` içinde `let window = AppWindow::new()?;` satırının hemen altına:

```rust
    let config = ConfigStore::system();
    let init_error = config.as_ref().and_then(|store| store.ensure_initialized().err().map(|e| (store, e)));
    let mut files = config.as_ref().map(ConfigStore::read_files).unwrap_or_default();
    if let Some((store, err)) = init_error {
        files.warnings.push(Warning::new(store.dir().display().to_string(), format!("cannot create config folder: {err}")));
    }
    // Something sensible is on screen even if the selected theme cannot be read.
    theme_bridge::apply(&window, &theme::builtin_dark());
    apply_config(&window, &files);
```

`main.rs`'in geri kalanı (`EntryModel`, `Ctx`, geri/yukarı/gezinme callback'leri, `window.run()`) değişmez.

- [ ] **Step 6: Derle**

Run: `~/.cargo/bin/cargo build --release -p gezik 2>&1 | grep -E "^(warning|error)|-->|Finished"`
Expected: `Finished`, uyarı yok. (Çalışan bir `gezik.exe` varsa önce kapat: `taskkill //IM gezik.exe //F`.)

- [ ] **Step 7: Ekran görüntüsü betiğini ekle**

`scripts/perf/screenshot.ps1`:

```powershell
# Captures the Gezik window to a PNG and prints its size and position.
# Usage: scripts/perf/screenshot.ps1 -Out shot.png
param([Parameter(Mandatory)] [string]$Out, [string]$Title = "Gezik")

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikShot {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct R { public int L, T, Rt, B; }
}
"@
[GezikShot]::SetProcessDPIAware() | Out-Null
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 100 -and $h -eq [IntPtr]::Zero; $i++) {
    # [NullString]::Value: PowerShell would pass $null as "" and FindWindow would fail.
    $h = [GezikShot]::FindWindow([NullString]::Value, $Title)
    Start-Sleep -Milliseconds 50
}
if ($h -eq [IntPtr]::Zero) { throw "Window '$Title' not found" }
[GezikShot]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400
$r = New-Object GezikShot+R
[GezikShot]::GetWindowRect($h, [ref]$r) | Out-Null
$bmp = New-Object Drawing.Bitmap ($r.Rt - $r.L), ($r.B - $r.T)
[Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out)
"{0}x{1} at {2},{3} -> {4}" -f ($r.Rt - $r.L), ($r.B - $r.T), $r.L, $r.T, $Out
```

- [ ] **Step 8: Elle doğrula (PowerShell, gerçek ayarlara dokunmadan)**

```powershell
$env:GEZIK_CONFIG_DIR = "$env:TEMP\gezik-verify"
Remove-Item -Recurse -Force $env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
$exe = "D:\Work\gezik\target\release\gezik.exe"
function Shot($name) { $p = Start-Process $exe -PassThru; & D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-$name.png"; Stop-Process -Id $p.Id }

Shot auto                                   # sistem moduna uygun tema
Get-ChildItem -Recurse $env:GEZIK_CONFIG_DIR | Select-Object FullName   # settings.toml, themes\example.toml
Set-Content "$env:GEZIK_CONFIG_DIR\settings.toml" 'theme = "light"' -Encoding utf8; Shot light
Set-Content "$env:GEZIK_CONFIG_DIR\settings.toml" 'theme = "dark"' -Encoding utf8; Shot dark
Set-Content "$env:GEZIK_CONFIG_DIR\settings.toml" 'theme = "example"' -Encoding utf8; Shot example
Set-Content "$env:GEZIK_CONFIG_DIR\themes\bad.toml" '[colors' -Encoding utf8
Set-Content "$env:GEZIK_CONFIG_DIR\settings.toml" 'theme = "bad"' -Encoding utf8; Shot broken
```

Expected (görüntüleri aç ve kontrol et):
- `light`: açık zemin, koyu metin, açık araç/durum çubuğu; düğmeler ve adres kutusu temaya uygun.
- `dark`: bugünkü görünüme yakın koyu tema.
- `example`: Nord renkleri (`#2e3440` zemin, sarı klasör kareleri).
- `broken`: uygulama koyu temayla açılır; durum çubuğunun sağında kırmızı `bad.toml line 1: ...` uyarısı.
- Tüm görüntülerde metinler okunur (yazı tipi boş = sistem yazı tipi çalışıyor).
- Not: `Set-Content -Encoding utf8` Windows PowerShell 5.1'de BOM yazar; uygulamanın bunu sorunsuz okuması Review Focus #1'in de gerçek dünya doğrulamasıdır.

Ardından: `Remove-Item Env:GEZIK_CONFIG_DIR`

- [ ] **Step 9: Commit**

```bash
git add crates/gezik scripts/perf/screenshot.ps1 Cargo.lock
git commit -m "Theme the UI from gezik-config at startup"
```

---

### Task 8: Uygulama — canlı yenileme ve sistem modu takibi

**Files:**
- Modify: `crates/gezik/Cargo.toml`
- Create: `crates/gezik/src/watcher.rs`
- Modify: `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: `ConfigStore::{dir, read_files, is_config_file}`, Task 7'deki `apply_config`, `AppWindow::on_system_scheme_changed`
- Produces: `watcher::watch_config(store: &ConfigStore, on_change: impl Fn() + Send + 'static) -> notify::Result<notify::RecommendedWatcher>`

- [ ] **Step 1: Bağımlılığı ekle**

`crates/gezik/Cargo.toml` `[dependencies]`:

```toml
notify = "8"
```

- [ ] **Step 2: `watcher.rs`'i yaz**

`crates/gezik/src/watcher.rs`:

```rust
//! Watches the config folder and reports bursts of changes as a single reload.

use std::sync::mpsc;
use std::time::Duration;

use gezik_config::store::ConfigStore;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Editors often write a file several times per save; wait this long for quiet.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Calls `on_change` on a background thread once per burst of changes to
/// `settings.toml` or a theme file. Keep the returned watcher alive while watching.
pub fn watch_config(store: &ConfigStore, on_change: impl Fn() + Send + 'static) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(store.dir(), RecursiveMode::Recursive)?;

    let store = store.clone();
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let relevant = event.is_ok_and(|e| e.paths.iter().any(|path| store.is_config_file(path)));
            if !relevant {
                continue;
            }
            // Swallow the rest of the burst, then reload once.
            while rx.recv_timeout(DEBOUNCE).is_ok() {}
            on_change();
        }
    });
    Ok(watcher)
}
```

- [ ] **Step 3: `main.rs`'e bağla**

`mod theme_bridge;` satırının altına:

```rust
mod watcher;
```

Task 7'de eklenen bloğu (`let config = ...`'dan `apply_config(&window, &files);`'e kadar) şununla değiştir:

```rust
    let config = ConfigStore::system();
    let init_error = config.as_ref().and_then(|store| store.ensure_initialized().err().map(|e| (store, e)));
    let mut files = config.as_ref().map(ConfigStore::read_files).unwrap_or_default();
    if let Some((store, err)) = init_error {
        files.warnings.push(Warning::new(store.dir().display().to_string(), format!("cannot create config folder: {err}")));
    }
    // Something sensible is on screen even if the selected theme cannot be read.
    theme_bridge::apply(&window, &theme::builtin_dark());
    apply_config(&window, &files);

    // The latest config files, so a system light/dark switch can re-resolve without I/O.
    let files = Arc::new(Mutex::new(files));

    window.on_system_scheme_changed({
        let weak = window.as_weak();
        let files = files.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                apply_config(&window, &files.lock().unwrap());
            }
        }
    });

    // Reading happens on the watcher thread; resolving and applying on the UI thread.
    let _watcher = config.as_ref().and_then(|store| {
        let reader = store.clone();
        let weak = window.as_weak();
        let files = files.clone();
        watcher::watch_config(store, move || {
            let fresh = reader.read_files();
            let files = files.clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                let mut current = files.lock().unwrap();
                *current = fresh;
                apply_config(&window, &current);
            });
        })
        .map_err(|err| eprintln!("gezik: cannot watch {}: {err}", store.dir().display()))
        .ok()
    });
```

(`Arc` ve `Mutex` zaten `use std::sync::{Arc, Mutex};` ile içe aktarılmış durumda. `_watcher`, `window.run()` dönene kadar yaşamalıdır; `main`'in yerel değişkeni olduğu için öyledir.)

- [ ] **Step 4: Derle**

Run: `~/.cargo/bin/cargo build --release -p gezik 2>&1 | grep -E "^(warning|error)|-->|Finished"`
Expected: `Finished`, uyarı yok.

- [ ] **Step 5: Canlı yenilemeyi elle doğrula**

```powershell
$env:GEZIK_CONFIG_DIR = "$env:TEMP\gezik-verify"
Remove-Item -Recurse -Force $env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
$p = Start-Process "D:\Work\gezik\target\release\gezik.exe" -PassThru
Start-Sleep -Seconds 1
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-live-0.png"
Set-Content "$env:GEZIK_CONFIG_DIR\settings.toml" 'theme = "example"' -Encoding utf8
Start-Sleep -Milliseconds 600
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-live-1.png"
Add-Content "$env:GEZIK_CONFIG_DIR\themes\example.toml" "`n[metrics]`nrow-height = 40" -Encoding utf8
Start-Sleep -Milliseconds 600
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-live-2.png"
Set-Content "$env:GEZIK_CONFIG_DIR\settings.toml" 'theme = "auto"' -Encoding utf8
Start-Sleep -Milliseconds 600
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-live-3.png"
Stop-Process -Id $p.Id
Remove-Item Env:GEZIK_CONFIG_DIR
```

Expected:
- `live-1`: uygulama yeniden başlatılmadan Nord renklerine geçmiş.
- `live-2`: `[metrics]` ikinci kez eklendiği için TOML söz dizimi hatası (yinelenen tablo) → Nord teması kalır, durum çubuğunda `example.toml line N: ...` uyarısı. Bu, "bozuk tema mevcut temayı korur" kuralının canlı doğrulamasıdır.
- `live-3`: sistem moduna uygun temaya döner, uyarı kaybolur.
- Sistem modu takibi: Uygulama açıkken Windows Ayarlar → Kişiselleştirme → Renkler → mod değiştirildiğinde (`theme = "auto"` ile) tema anında değişmeli. Bunu bir kez elle dene ve sonucu not et.

- [ ] **Step 6: Commit**

```bash
git add crates/gezik Cargo.lock
git commit -m "Reload settings and themes live; follow the system light/dark mode"
```

---

### Task 9: Uygulama — pencere boyutu ve konumu (`state.toml`)

**Files:**
- Modify: `crates/gezik/Cargo.toml` (slint özelliği)
- Create: `crates/gezik/src/window_state.rs`
- Modify: `crates/gezik/src/main.rs`

**Interfaces:**
- Consumes: `ConfigStore::{load_state, save_state}`, `settings::{State, WindowState}`
- Produces: `window_state::{restore(&AppWindow, &State), ensure_visible(&AppWindow), capture(&AppWindow) -> State}`

- [ ] **Step 1: Slint özelliğini ekle**

`crates/gezik/Cargo.toml`'daki slint özellik listesine `"unstable-winit-030",` ekle ve yorum satırını güncelle:

```toml
# Software renderer only: no GPU or OpenGL driver needed, and it measured far lower
# memory than FemtoVG (5 MB vs 35 MB idle) at the same scrolling CPU cost.
# unstable-winit-030: monitor list, to keep restored windows on screen.
slint = { version = "1.18", default-features = false, features = [
    "std",
    "compat-1-2",
    "backend-winit",
    "renderer-software",
    "software-renderer-systemfonts",
    "accessibility",
    "unstable-winit-030",
] }
```

- [ ] **Step 2: `window_state.rs`'i yaz**

`crates/gezik/src/window_state.rs`:

```rust
//! Window size and position, kept in the machine-local `state.toml`.

use gezik_config::settings::{State, WindowState};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, LogicalSize, PhysicalPosition};

use crate::AppWindow;

/// Applies the saved size and position. Call before the window is shown.
pub fn restore(window: &AppWindow, state: &State) {
    let Some(saved) = state.window else { return };
    window.window().set_size(LogicalSize::new(saved.width as f32, saved.height as f32));
    if let (Some(x), Some(y)) = (saved.x, saved.y) {
        window.window().set_position(PhysicalPosition::new(x, y));
    }
}

/// Moves the window onto the primary monitor if its restored position is on a monitor
/// that is no longer connected. Needs the native window, so call it from the event loop.
pub fn ensure_visible(window: &AppWindow) {
    let target = window
        .window()
        .with_winit_window(|native| {
            let position = native.outer_position().ok()?;
            // A point in the title bar must be on some monitor, so the window can be dragged.
            let (x, y) = (position.x + 100, position.y + 20);
            let on_screen = native.available_monitors().any(|monitor| {
                let (origin, size) = (monitor.position(), monitor.size());
                x >= origin.x && x < origin.x + size.width as i32 && y >= origin.y && y < origin.y + size.height as i32
            });
            if on_screen {
                return None;
            }
            let monitor = native.primary_monitor().or_else(|| native.available_monitors().next())?;
            Some(PhysicalPosition::new(monitor.position().x + 100, monitor.position().y + 100))
        })
        .flatten();
    if let Some(position) = target {
        window.window().set_position(position);
    }
}

/// The current size (logical pixels) and position (physical pixels).
pub fn capture(window: &AppWindow) -> State {
    let native = window.window();
    let size = native.size().to_logical(native.scale_factor());
    let position = native.position();
    State {
        window: Some(WindowState {
            width: size.width.round() as u32,
            height: size.height.round() as u32,
            x: Some(position.x),
            y: Some(position.y),
        }),
    }
}
```

- [ ] **Step 3: `main.rs`'e bağla**

`mod watcher;` satırının altına:

```rust
mod window_state;
```

Task 8'deki `let _watcher = ...;` ifadesinin altına ekle:

```rust
    if let Some(store) = &config {
        window_state::restore(&window, &store.load_state());
    }
    slint::Timer::single_shot(std::time::Duration::ZERO, {
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window_state::ensure_visible(&window);
            }
        }
    });
    window.window().on_close_requested({
        let weak = window.as_weak();
        let store = config.clone();
        move || {
            if let (Some(window), Some(store)) = (weak.upgrade(), &store)
                && let Err(err) = store.save_state(&window_state::capture(&window))
            {
                eprintln!("gezik: cannot save window state: {err}");
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
```

- [ ] **Step 4: Derle**

Run: `~/.cargo/bin/cargo build --release -p gezik 2>&1 | grep -E "^(warning|error)|-->|Finished"`
Expected: `Finished`, uyarı yok.

- [ ] **Step 5: Elle doğrula**

```powershell
$env:GEZIK_CONFIG_DIR = "$env:TEMP\gezik-verify"
Remove-Item -Recurse -Force $env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
$exe = "D:\Work\gezik\target\release\gezik.exe"
Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikMove {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
}
"@
# 1) Move/resize, close normally (WM_CLOSE), reopen: same rect.
$p = Start-Process $exe -PassThru; Start-Sleep -Seconds 1
$h = [GezikMove]::FindWindow([NullString]::Value, "Gezik")
[GezikMove]::MoveWindow($h, 200, 150, 1100, 750, $true) | Out-Null; Start-Sleep -Milliseconds 300
$p.CloseMainWindow() | Out-Null; $p.WaitForExit(5000) | Out-Null
Get-Content "$env:GEZIK_CONFIG_DIR\state.toml"
$p = Start-Process $exe -PassThru
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-state-1.png"
Stop-Process -Id $p.Id
# 2) Saved position on a monitor that does not exist: window must come back on screen.
Set-Content "$env:GEZIK_CONFIG_DIR\state.toml" "[window]`nwidth = 900`nheight = 600`nx = -30000`ny = -30000" -Encoding utf8
$p = Start-Process $exe -PassThru
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-state-2.png"
Stop-Process -Id $p.Id
# 3) Garbage state file: app starts with defaults.
Set-Content "$env:GEZIK_CONFIG_DIR\state.toml" "garbage [" -Encoding utf8
$p = Start-Process $exe -PassThru
& D:\Work\gezik\scripts\perf\screenshot.ps1 -Out "$env:TEMP\gezik-state-3.png"
Stop-Process -Id $p.Id
Remove-Item Env:GEZIK_CONFIG_DIR
```

Expected:
- `state.toml` içinde `[window]` ve `width`/`height`/`x`/`y` var; `MoveWindow` dış çerçeveyi ayarladığı için `width`/`height` 1100×750'den çerçeve kadar küçük olabilir, önemli olan:
- 1) `screenshot.ps1` çıktısındaki konum ve boyut, `MoveWindow` ile verilenle ±10 px aynı (ölçek %100 değilse boyut ölçek oranında farklılaşabilir; konum aynı kalmalı).
- 2) Konum, birincil monitörün sol üstüne +100,+100 civarı; pencere görünür.
- 3) Pencere 900×600 varsayılan boyutla açılır, hata yok.

- [ ] **Step 6: Commit**

```bash
git add crates/gezik Cargo.lock
git commit -m "Remember window size and position per machine"
```

---

### Task 10: Performans doğrulaması, ölçüm betikleri, belgeler

**Files:**
- Create: `scripts/perf/measure.ps1`
- Create: `scripts/perf/stress.ps1`
- Modify: `README.md`

**Interfaces:**
- Consumes: derlenmiş `target/release/gezik.exe`, `GEZIK_CONFIG_DIR`
- Produces: tekrarlanabilir performans ölçümleri; kullanıcı belgeleri

- [ ] **Step 1: Ölçüm betiklerini ekle**

`scripts/perf/measure.ps1`:

```powershell
# Startup time and idle memory, averaged over several runs. Windows only.
# Usage: scripts/perf/measure.ps1 [-Runs 3]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Runs = 3)

Get-Process gezik -ErrorAction SilentlyContinue | Stop-Process
$open = @(); $mem = @()
for ($i = 0; $i -lt $Runs; $i++) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process $Exe -PassThru
    while ($p.MainWindowHandle -eq 0 -and $sw.ElapsedMilliseconds -lt 10000) { Start-Sleep -Milliseconds 5; $p.Refresh() }
    $open += $sw.ElapsedMilliseconds
    Start-Sleep -Seconds 3
    # Task Manager's "Memory" column is the private working set.
    $mem += (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)").WorkingSetPrivate / 1MB
    Stop-Process -Id $p.Id
    Start-Sleep -Milliseconds 500
}
"open {0:N0} ms | Task Manager memory {1:N1} MB | exe {2:N2} MB" -f ($open | Measure-Object -Average).Average, ($mem | Measure-Object -Average).Average, ((Get-Item $Exe).Length / 1MB)
```

`scripts/perf/stress.ps1`:

```powershell
# Opens a huge folder, scrolls it with the mouse wheel and reports memory and CPU.
# Creates the test folder on first use. Windows only.
# Usage: scripts/perf/stress.ps1 [-Files 100000]
param(
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Files = 100000,
    [string]$Dir = "$env:TEMP\gezik-stress-$Files"
)

if (-not (Test-Path $Dir) -or (Get-ChildItem $Dir).Count -ne $Files) {
    New-Item -ItemType Directory -Force $Dir | Out-Null
    for ($i = 0; $i -lt $Files; $i++) { [IO.File]::WriteAllBytes("$Dir\file_$i.txt", [byte[]]::new(0)) }
}

Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikStress {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, int d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct R { public int L, T, Rt, B; }
}
"@
[GezikStress]::SetProcessDPIAware() | Out-Null
Get-Process gezik -ErrorAction SilentlyContinue | Stop-Process
function Mem($id) { (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB }

$p = Start-Process $Exe -ArgumentList "`"$Dir`"" -PassThru
Start-Sleep -Seconds 5
$loaded = Mem $p.Id
$h = [GezikStress]::FindWindow([NullString]::Value, "Gezik")
[GezikStress]::SetForegroundWindow($h) | Out-Null
$r = New-Object GezikStress+R; [GezikStress]::GetWindowRect($h, [ref]$r) | Out-Null
[GezikStress]::SetCursorPos([int](($r.L + $r.Rt) / 2), [int](($r.T + $r.B) / 2)) | Out-Null
Start-Sleep -Milliseconds 300
$p.Refresh(); $cpu0 = $p.TotalProcessorTime.TotalMilliseconds
$sw = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt 60; $i++) { [GezikStress]::mouse_event(0x0800, 0, 0, -360, [IntPtr]::Zero); Start-Sleep -Milliseconds 16 }
Start-Sleep -Milliseconds 500
$p.Refresh()
"{0:N0} files | after load {1:N1} MB | after scroll {2:N1} MB | scroll CPU {3:N0} ms over {4:N0} ms" -f $Files, $loaded, (Mem $p.Id), ($p.TotalProcessorTime.TotalMilliseconds - $cpu0), $sw.ElapsedMilliseconds
Stop-Process -Id $p.Id
```

- [ ] **Step 2: Ölç ve kabul ölçütleriyle karşılaştır**

```powershell
$env:GEZIK_CONFIG_DIR = "$env:TEMP\gezik-verify"
& D:\Work\gezik\scripts\perf\measure.ps1
& D:\Work\gezik\scripts\perf\stress.ps1
& D:\Work\gezik\scripts\perf\stress.ps1
Remove-Item Env:GEZIK_CONFIG_DIR
```

Expected (bu planın öncesindeki değerler: açılış ~40 ms, boşta 5,0 MB, 100 bin dosyada 12,7 MB, kaydırma CPU'su ~480 ms / 2,6 sn):
- Boşta bellek ≤ 7,0 MB
- Açılış ≤ ~60 ms (ölçüm gürültüsü payı ile)
- 100 bin dosya: bellek ≤ ~15 MB, kaydırma CPU'su ~480 ms'den belirgin yüksek değil

Ölçütlerden biri aşılıyorsa durma ve nedeni araştır (ör. `notify` iş parçacıkları, dosya izleyicinin yük altında olay üretmesi); kabul ölçütünü sessizce gevşetme.

- [ ] **Step 3: README'ye tema ve ayar belgelerini ekle**

`README.md` içinde `## Build` bölümünün üstüne ekle:

````markdown
## Settings and themes

Gezik keeps its settings in a plain-text folder:

| OS | Location |
|---|---|
| Windows | `%APPDATA%\gezik` |
| macOS | `~/Library/Application Support/gezik` |
| Linux | `~/.config/gezik` |

Set the `GEZIK_CONFIG_DIR` environment variable to use another folder.

- `settings.toml` — your preferences. Portable: copy it to another computer, on any OS.
- `themes/*.toml` — your themes. `themes/example.toml` is a commented starting point.
- `state.toml` — window size and position on this machine. Not meant to be copied.

Changes apply as soon as you save; no restart needed. Problems (a typo, an invalid
color) show up in the status bar with the file and line, and never stop Gezik from
starting.

### Writing a theme

A theme only needs the values it changes; the rest comes from its `base`:

```toml
# themes/sunset.toml  ->  select it with  theme = "sunset"  in settings.toml
base = "dark"

[colors]
accent = "#ff8a3d"
folder-icon = "#ffb347"
```

See `themes/example.toml` for every color and size you can set.

## Performance checks (Windows)

```powershell
scripts/perf/measure.ps1   # startup time and idle memory
scripts/perf/stress.ps1    # 100,000-file folder: memory and scrolling CPU
```
````

- [ ] **Step 4: Tüm testleri ve derlemeyi son kez çalıştır**

Run: `~/.cargo/bin/cargo test --workspace && ~/.cargo/bin/cargo clippy --workspace --all-targets`
Expected: tüm testler geçer; clippy uyarısı yok.

- [ ] **Step 5: Commit**

```bash
git add scripts/perf README.md
git commit -m "Add performance scripts and document settings and themes"
```
