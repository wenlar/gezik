# Toplu İşlemler 5b (Arşivler ve Araç İndirme) — Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Arşivleri tek tıkla açmak (zip, 7z, rar, tar ailesi, tek dosya sıkıştırmaları, cab, iso, cpio, ar/deb; nadir biçimler indirilen 7-Zip ile), arşiv oluşturmak (zip, 7z, tar.gz, tar.xz, tar; şifre; 7z'de parçalara bölme), var olan arşive eklemek ve gerekli dış araçları tek tıkla indirip doğrulamak.

**Architecture:** Saf kararlar (biçim tanıma, yol güvenliği, "tek kök mü", ad üretimi, parça adları, bomba ölçüsü, araç bildirimi) `gezik-core::batch::archive` ve `gezik-core::batch::tools`'ta. Biçim kodu `gezik-batch::archive::{read,write,formats/*}`'te; tek arayüz `ArchiveSource` (liste + aşama klasörüne açma). Motor görevleri (`ExtractTask`, `PlaceTask`, `CompressTask`, `AddToArchiveTask`, `DownloadTask`, `ExternalExtractTask`) `gezik-batch`'te; motor (`gezik-ops`) zincir iş, soru olayı, aşama klasörü, bulunan iş miktarı ve alt süreç sonlandırma ile genişler. Açma bir zincir iştir: arşiv gizli aşama klasörüne açılır, sonra `PlaceTask` (4a'nın taşımasının "oluşturuldu" sonucu veren hâli) yerine koyar; tek kayıt, tek Ctrl+Z.

**Tech Stack:** Rust 2024, Slint 1.18; `zip` 8.6, `sevenz-rust2` 0.23, `tar` 0.4, `flate2` 1.1 (miniz), `lzma-rust2` 0.21, `bzip2` 0.6 (saf Rust), `ruzstd` 0.9, `unrar-ng` 0.7 (C++), `cab` 0.6, `hadris-iso` 2.5, `cpio` 0.4, `ar` 0.9, `sha2` 0.10.

**Spec:** `docs/superpowers/specs/2026-10-05-toplu-islemler-design.md` (bölüm 3, 5, 8, 10-12)
**Kütüphane özeti (doğrulanmış kod):** `docs/superpowers/notes/2026-10-05-arsiv-kutuphaneleri.md` ve derlenip çalıştırılmış örnek kaynaklar `docs/superpowers/notes/arsiv-probe/*.rs` (`safe_join`, `MultiFileReader`, `SplitWriter`, `ZstdReader`, `decoder`, `detect`). Bu plan biçim kodunda o özetin bölümlerine atıf yapar; oradaki kod parçaları birebir kullanılabilir.

## Spec'ten sapmalar ve netleştirmeler

1. **Aşama klasörü adı `.gezik-deleting-x-<pid>-<n>`** (spec: `.gezik-x-…`): mevcut "anında silme" kaydı (`pending-deletes`'e pid'li `deleting` satırı) ve açılış toparlaması aynen işler, listeler bu öneki zaten gizler. Yeni kayıt türü gerekmez.
2. **"Extract to…" klasör seçici yerine yol sorusu:** Gezik'te yerel klasör seçici yok; yeni bağımlılık eklememek için `Dialogs::ask_text` bulunulan klasörle dolu bir yol alanı açar (son kullanılan hatırlanır). Yol yoksa oluşturulur; geçersizse hata.
3. **Tek kök kararı açtıktan sonra verilir:** aşama klasörünün kökünde tek öğe varsa o öğe hedefe, değilse aşama içeriği `ad\` klasörüne konur. tar.gz gibi akış biçimleri iki kez okunmaz.
4. **zip "var olana ekleme" `raw_copy_file` ile geçici dosyaya yeniden yazarak** (`new_append` AES girdilerini bozuyor, özet §1).
5. **Paralel zip:** her işçi tek girdili zip'i bellekte (≥ 64 MB girdiler için geçici dosyada) üretir, ana iş parçacığı `merge_archive` ile birleştirir (özet §1).
6. **7z parçaları bayt bölmesi** (`SplitWriter`, özet §2); okuma `MultiFileReader`.
7. **RAR ilerlemesi dosya başınadır** (unrar-ng bayt ilerlemesi vermiyor); büyük girdide yazılan dosyanın boyutu 200 ms'de bir okunur.
8. **cab "spanning", gerçek `.z01` bölünmüş zip ve UDF-yalnız ISO** içeride açılmaz: indirilen 7-Zip'e yönlendirilir ("7-Zip needed" kutusu).
9. **Windows'ta arşivdeki sembolik bağlantılar atlanır** ve hata satırı "symbolic link skipped" yazar (Unix'te yalnız hedef aşama içinde kalıyorsa kurulur).
10. **Sıkıştırma bombası sorusu ve boş yer sorusu** liste boyutu bilinen biçimlerde (zip, 7z, rar, iso, cab) iş başlamadan sorulur; tar.* ve tek dosya biçimlerinde yalnız "gerçek boyut bildirileni aşarsa dur" kuralı ve disk dolu duraklatması (4a) geçerli.
11. **Araç deposu:** `wenlar/gezik-tools` sürümleri; dosyalar ve SHA-256'lar Görev 9'da üretilir. Depo oluşturma ve yükleme `gh` gerektirir; Görev 9 `gh` yoksa yükleme adımını "bekliyor" diye bırakır ve bildirimi yine yerel dosyaların özetleriyle yazar (adresler son hâlinde olur).
12. **MSRV:** `sevenz-rust2` 0.23 Rust 1.93 istiyor; bir crate'te `rust-version = "1.92"` varsa 1.93 yapılır (araç zinciri 1.99).
13. **unRAR lisansı:** `THIRD-PARTY.md` (yeni) unRAR lisansının 2. paragrafını ve diğer kütüphanelerin lisanslarını listeler.

## Global Constraints

- Rust edition 2024, stable. Komutlar `D:\Work\gezik` içinden; `cargo` = `~/.cargo/bin/cargo` (Git Bash). Testler `GEZIK_CONFIG_DIR` ayarlı değilken.
- Her görevin sonunda `cargo build --workspace`, `cargo test --workspace` geçer; `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo fmt --all -- --check` temiz.
- `cargo check -p gezik --target aarch64-apple-darwin` ve `cargo check -p gezik-core -p gezik-platform -p gezik-ops -p gezik-batch --target x86_64-unknown-linux-gnu` bağımlılık veya platform kodu değiştiren her görevden sonra geçer. (unrar-ng çapraz derlemede C++ derleyicisi ister: Linux hedefinde `cargo check` C++ derlemez — `cargo check` build script'i çalıştırır; başarısız olursa Görev 4 Linux denetimini Docker kabına taşır ve notlar.)
- Yeni bağımlılıklar tam olarak (`gezik-batch`'e): `zip = { version = "8.6.0", default-features = false, features = ["aes-crypto", "deflate-flate2", "deflate64", "bzip2"] }`, `sevenz-rust2 = { version = "0.23.0", default-features = false, features = ["aes256", "compress", "bzip2", "ppmd"] }`, `tar = { version = "0.4.46", default-features = false }`, `flate2 = "1.1.10"`, `lzma-rust2 = { version = "0.21.0", default-features = false, features = ["std", "encoder", "optimization", "xz"] }`, `bzip2 = "0.6.1"`, `ruzstd = { version = "0.9.0", default-features = false, features = ["std"] }`, `unrar-ng = { version = "0.7.7", default-features = false }`, `cab = "0.6.0"`, `hadris-iso = { version = "2.5.0", default-features = false, features = ["std", "sync", "read"] }`, `cpio = "0.4.1"`, `ar = "0.9.0"`, `sha2 = "0.10"`. zip'in `zstd`, `lzma`, `xz`, sevenz'in `deflate` özellikleri **kapalı**. Test için `dev-dependencies`'e yalnız `hadris-iso` `write` özelliği eklenebilir.
- Arayüz iş parçacığı dosya sistemine dokunmaz (biçim tanıma, liste okuma, boş yer sorgusu iş iş parçacığında).
- Hiçbir dosya kullanıcı seçmeden ezilmez: yerine koyma 4a'nın çakışma listesinden geçer; arşiv yazımı geçici adla, bitince yeniden adlandırma.
- Her arşiv girdi adı `safe_join`'den geçer (`..`, kök, sürücü öneki, UNC, `:` ADS, Windows ayrılmış adları reddedilir).
- Şifreler yalnız bellekte; hiçbir dosyaya, günlüğe, `state.toml`'a yazılmaz.
- Arayüz metinleri İngilizce; commit mesajları İngilizce, Claude imzası yok.
- exe büyümesi 5b için ≤ +3 MB (ölçüm Görev 10).

## Review Focus

1. **Kötü niyetli girdi adları** (`../x`, `C:\x`, `\\srv\s\x`, `a:stream`, `CON`, mutlak yol, `/` ile başlayan) her biçimde: aşama klasörü dışında hiçbir şey yazılmamalı. → Görev 2 `safe_join` testleri + Görev 3 zip-slip zip ve tar testi.
2. **Yanlış şifre** (zip AES/ZipCrypto, 7z başlık şifreli/şifresiz, RAR4 `BadData`, RAR5 `BadPassword`): tekrar sorulmalı, iptal o arşivi atlamalı, yarım dosya kalmamalı. → Görev 3 ve 4 şifre testleri, Görev 5 soru akışı testi.
3. **İptal veya çökme ortasında** açma/oluşturma: aşama klasörü ve yarım arşiv kalmamalı (çökmede bir sonraki açılışta silinir). → Görev 5 ve 6 iptal testleri.
4. **Çok parçalı arşivin ortadaki parçası seçilip açılması** (`a.7z.002`, `a.part2.rar`): ilk parçadan açılmalı; eksik parça açık bir hatayla bitmeli. → Görev 3/4 parça testleri.
5. **Zaten var olan hedefe "Extract here"** (aynı adlı klasör/dosya): çakışma listesi; birleştirme; Ctrl+Z yalnız eklenenleri çöpe atmalı. → Görev 5 `extract_into_an_existing_folder_merges_and_undoes` testi.

---

## Dosya yapısı

| Dosya | Sorumluluk |
|---|---|
| `crates/gezik-core/src/batch/archive.rs` (yeni) | `Format`, `detect(head: &[u8], name: &str) -> Option<Format>`, `safe_join`, `archive_stem`, `volumes_of`, `first_volume`, `single_root`, `bomb_suspect`, `split_sizes` |
| `crates/gezik-core/src/batch/tools.rs` (yeni) | `Tool`, `ToolBuild`, `MANIFEST`, `build_for(tool, os, arch)` |
| `crates/gezik-ops/src/{engine,run,task,control,inverse}.rs` | Zincir iş, soru olayı, `RunCx::{found, staging_dir, temp_file_for, ask}`, `Outcome::Several`, `MoveTask::placing` |
| `crates/gezik-platform/src/process.rs` | `Child` sarmalayıcı: süreç ağacını sonlandırma (Windows iş nesnesi, Unix süreç grubu), pencere açmadan çalıştırma |
| `crates/gezik-batch/src/archive/mod.rs` | `ArchiveSource` arayüzü, `open(path, password) -> Result<Box<dyn ArchiveSource>>`, `Entry`, `ExtractCx` |
| `crates/gezik-batch/src/archive/{zip,sevenz,tar,single,rar,cab,iso,cpio,ar}.rs` | Biçim başına okuma |
| `crates/gezik-batch/src/archive/io.rs` | `MultiFileReader`, `SplitWriter`, `ZstdReader`, `decoder`, ilerleme sarmalayıcıları |
| `crates/gezik-batch/src/archive/write.rs` | Oluşturma (zip, 7z, tar, tar.gz, tar.xz, .gz, .xz) ve yeniden yazma (ekleme) |
| `crates/gezik-batch/src/tasks/{extract,place,compress,add,download,external}.rs` | Motor görevleri |
| `crates/gezik-batch/src/tools.rs` | Araç bulma (ayar, indirilen, PATH), indirme ve kurma |
| `crates/gezik-batch/tests/data/` | RAR örnekleri (libarchive, BSD) ve `SOURCES.md` |
| `crates/gezik-config/src/settings.rs` ve şablon | `[archives] double-click`, `[tools] download`, `State::archive` |
| `crates/gezik/src/archives.rs` (yeni) | Menü komutları, Compress katmanı denetleyicisi, soru köprüsü, araç indirme kutusu |
| `crates/gezik/ui/widgets/compress.slint` (yeni) | Compress katmanı |
| `scripts/tools/prepare.ps1` (yeni) | Araç dosyalarını üst kaynaktan indirip paketleyen ve özet çıkaran betik |
| `THIRD-PARTY.md` (yeni) | Lisans notları |

---

### Task 1: Motor genişletmeleri — zincir iş, soru, aşama klasörü, bulunan iş, yerleştirme sonucu

**Files:**
- Modify: `crates/gezik-ops/src/engine.rs`, `run.rs`, `task.rs`, `control.rs`, `inverse.rs`, `lib.rs`, `tasks/move_.rs`, `pending.rs`

**Interfaces:**
- Produces:
  - `Engine::submit_chain(&self, tasks: Vec<Box<dyn Task>>, label: Option<String>) -> JobId` — görevler sırayla, tek kayıt.
  - `Event::Question { job: JobId, question: Question }`, `pub enum Question { Password { archive: PathBuf, retry: bool }, Confirm { title: String, message: String, buttons: Vec<String> } }`, `pub enum Answer { Text(String), Button(usize), Cancel }`, `Engine::answer(&self, job: JobId, answer: Answer)`.
  - `RunCx::ask(&self, question: Question) -> Answer` (iş bekler; iptal edilirse `Answer::Cancel`; durum `JobState::Deciding`).
  - `RunCx::found(&self, items: u64, bytes: u64)` (çalışırken bulunan işi toplama ekler), `RunCx::one_done(&self, bytes: u64)` (bir alt öğe bitti).
  - `RunCx::staging_dir(&self, near: &Path) -> io::Result<PathBuf>` — `near`'ın klasöründe `.gezik-deleting-x-<pid>-<n>` oluşturur, `pending.add` ile not eder; iş bitince (görevin `done`'u değil, motor) silinir ve not kalkar.
  - `RunCx::temp_file_for(&self, target: &Path) -> PathBuf` — `TempCopies::next_to` (COPYING öneki, pending notu; çökmede açılışta silinir).
  - `Outcome::Several(Vec<Outcome>)`; `inverse::build` ve `Job::outcome` düzleştirir.
  - `MoveTask::placing(pairs: Vec<(PathBuf, PathBuf)>, label_kind: TaskKind) -> MoveTask` — aynı sürücüde taşır ama sonuç `Outcome::Created { path, facts, from: None }` (geri alma oluşanları çöpe atar); klasör birleştirmede içerideki her dosya `Created` olur.
  - `TaskKind::{Extract, Compress, AddToArchive, Download}` (+ `verb`: "Extract", "Compress", "Add to archive", "Download").

- [ ] **Step 1: Testleri yaz (gezik-ops)**

`engine.rs` testlerine:

```rust
    #[test]
    fn a_chain_is_one_job_and_one_undo() {
        let dir = test_dir("chain");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let tasks: Vec<Box<dyn Task>> = vec![
            Box::new(CopyTask::into(vec![dir.join("a.txt")], &dir.join("b"))),
            Box::new(CopyTask::into(vec![dir.join("a.txt")], &dir.join("c"))),
        ];
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::create_dir_all(dir.join("c")).unwrap();
        let job = engine.submit_chain(tasks, Some("Copy twice".into()));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(engine.undo_label().as_deref(), Some("Copy twice"));
        finish(&engine, engine.undo().unwrap(), defaults);
        assert!(!dir.join("b/a.txt").exists() && !dir.join("c/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_task_can_ask_and_wait_for_the_answer() {
        // A test task asks once and records the answer in its outcome path.
        let engine = engine();
        let job = engine.submit(Box::new(crate::testing::AskingTask::default()));
        let mut asked = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            for event in engine.drain() {
                if let Event::Question { job: j, question: Question::Password { retry, .. } } = event {
                    assert_eq!(j, job);
                    assert!(!retry);
                    asked = true;
                    engine.answer(job, Answer::Text("pw".into()));
                }
                if let Event::Finished { job: j, report } = event {
                    assert_eq!(j, job);
                    assert!(asked);
                    assert_eq!(report.results, [PathBuf::from("pw")]);
                    return;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn cancelling_a_waiting_question_answers_cancel() {
        let engine = engine();
        let job = engine.submit(Box::new(crate::testing::AskingTask::default()));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            for event in engine.drain() {
                if matches!(event, Event::Question { .. }) {
                    engine.cancel(job);
                }
                if let Event::Finished { report, .. } = event {
                    assert!(report.cancelled);
                    assert!(report.results.is_empty());
                    return;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_staging_folder_is_noted_and_gone_when_the_job_ends() {
        let dir = test_dir("staging");
        let pending = dir.join("pending-deletes");
        let engine = Engine::new(Settings { pending_deletes: Some(pending.clone()), ..Settings::default() }, || {});
        let job = engine.submit(Box::new(crate::testing::StagingTask::new(dir.clone())));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(left.iter().all(|name| !name.starts_with(".gezik-")), "{left:?}");
        assert!(PendingDeletes::new(pending).load_unowned().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

`testing.rs`'e yardımcı görevler (yalnız test):

```rust
/// Asks for a password once; its result is the answer's text.
#[derive(Default)]
pub(crate) struct AskingTask;

impl Task for AskingTask {
    fn kind(&self) -> TaskKind { TaskKind::Extract }
    fn title(&self) -> String { "Asking".into() }
    fn count(&self) -> usize { 1 }
    fn resources(&self) -> Resources { Resources { paths: Vec::new(), work: Work::Cpu } }
    fn plan(&self, sink: &mut dyn ScanSink) {
        sink.item(PlanItem::new(Stage::Parallel, Facts::default()).top(0));
    }
    fn run(&self, _: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        match cx.ask(Question::Password { archive: "a.zip".into(), retry: false }) {
            Answer::Text(text) => Ok(Outcome::Created { path: text.into(), facts: Facts::default(), from: None }),
            _ => Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled")),
        }
    }
}

/// Makes a staging folder with a file in it.
pub(crate) struct StagingTask(PathBuf);

impl StagingTask {
    pub fn new(dir: PathBuf) -> StagingTask { StagingTask(dir) }
}

impl Task for StagingTask {
    fn kind(&self) -> TaskKind { TaskKind::Extract }
    fn title(&self) -> String { "Staging".into() }
    fn count(&self) -> usize { 1 }
    fn resources(&self) -> Resources { Resources { paths: vec![self.0.clone()], work: Work::Disk } }
    fn plan(&self, sink: &mut dyn ScanSink) {
        sink.item(PlanItem::new(Stage::Parallel, Facts::default()).target(self.0.join("x")).top(0));
    }
    fn run(&self, _: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let staging = cx.staging_dir(&self.0.join("x"))?;
        std::fs::write(staging.join("f.txt"), "f")?;
        Ok(Outcome::Nothing)
    }
}
```

(Test `a_task_can_ask_and_wait_for_the_answer`'daki `report.results`: `AskingTask` sonucu kök öğe olduğu için `job.result` olarak gelir. `PlanItem::top` sonucu `is_root` yapar; `Outcome::result()` `Created`'ın yolunu döndürür.)

`inverse.rs` testine:

```rust
    #[test]
    fn several_outcomes_are_flattened() {
        let outcomes = vec![Outcome::Several(vec![
            Outcome::Trashed { original: "/d/a.zip".into(), trashed: "/bin/1".into() },
            Outcome::Created { path: "/d/a.zip".into(), facts: file(), from: None },
        ])];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Restore]);
    }
```

`move_.rs` testine:

```rust
    #[test]
    fn placing_reports_created_so_undo_trashes() {
        let dir = test_dir("move-placing");
        write(&dir.join("stage/x/a.txt"), "a");
        write(&dir.join("dst/x/b.txt"), "b");
        let engine = engine();
        let task = MoveTask::placing(vec![(dir.join("stage/x"), dir.join("dst/x"))], TaskKind::Extract);
        let job = engine.submit(Box::new(task));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/x/a.txt")), "a");
        finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(!dir.join("dst/x/a.txt").exists(), "the placed file went to the trash");
        assert_eq!(read(&dir.join("dst/x/b.txt")), "b", "what was there stays");
        let _ = std::fs::remove_dir_all(&dir);
    }
```

(Çöp kutusu olmayan test ortamında `TrashTask` "no trash" der; mevcut motor testlerinin çöp kullanımını izle — `testing::engine()` çöpü nasıl ele alıyorsa (`undo_of_trash_restores` testi çöpü kullanıyor) aynısı.)

- [ ] **Step 2: Başarısız olduklarını gör**

Run: `~/.cargo/bin/cargo test -p gezik-ops`
Expected: derleme hatası (yeni türler yok).

- [ ] **Step 3: Uygula**

- `TaskKind`'e dört tür ve `verb`'leri; `label`: `Extract`, `Compress`, `AddToArchive`, `Download` için `count == 1` → "Extract archive" yerine fiil + öğe ("Extract 1 item") kalıbı mevcut koldan gelir; ek kural yok.
- `Question`, `Answer` (`task.rs`'te, `lib.rs`'ten dışa açılır). `Control`'e `question: Mutex<Option<Answer>>` + `Condvar` (`set_decisions`/`wait_decisions` kalıbıyla) ve `asking: AtomicBool`. `RunCx::ask`: olay kuyruğuna `Event::Question` gönderir (RunCx'in olay gönderebilmesi için `RunCx`'e `shared: &Shared` ve `job: JobId` alanı ekle; `execute`'ta doldur), `asking`'i kurar, cevabı bekler (iptalde `Answer::Cancel`; `Control::cancel` bekleyeni uyandırır), `asking`'i indirir. Raporlayıcı `asking` iken durumu `JobState::Deciding` gösterir (conflicts'teki `deciding` gibi). Aynı anda tek soru: `ask` bir mutex ile sıralanır (paralel işçiler).
- `Engine::answer(job, answer)`: işi bulur, `Control`'e cevabı koyar.
- `Engine::submit_chain`: `self.start(tasks.into_iter().map(Arc::from).collect(), Origin::New, label)`.
- `RunCx::found`/`one_done`: `control.add_total(items, bytes)` / `control.item_done(); control.add_bytes(bytes)` (kök öğenin kendi sayımı ayrıca yapılmaz: bu görevler kök öğeyi `uncounted()` planlar — 5a'nın `PlanItem::uncounted` alanı).
- `RunCx::staging_dir`: ad `format!("{HIDDEN_PREFIX}x-{}-{n}", std::process::id())` (n: atomik sayaç), `near.parent()` içinde `create_dir`; `pending` varsa `pending.add(&path)`; `Job`'a `staging: Mutex<Vec<PathBuf>>` ekle, yolu oraya kaydet. `Shared::finish`'te görevlerin `done`'undan **sonra** her aşama klasörü `fs::delete` + `remove_dir_all` yedeğiyle silinir ve `pending.remove` çağrılır (silme başarısızsa not kalır, açılışta silinir). Windows'ta klasöre gizli öznitelik verilir (mevcut `fs::set_hidden` varsa; yoksa ad öneki yeter).
- `RunCx::temp_file_for`: `self.temp.next_to(target)` (yoksa `target.with_file_name(format!("{COPYING_PREFIX}…"))`) — `next_to`'yu `pub(crate)` yap.
- `Outcome::Several`: `Outcome::result()` içteki ilk `Created`'ın yolunu döndürür; `Job::outcome` kaydederken düzleştirir (ya da `inverse::build` başında düzleştir — ikisinden biri; `build` başında düzleştirmek tek yer).
- `MoveTask::placing`: `MoveTask`'e `placing: bool` alanı; `run`'da `RENAME` kolunda `placing` ise `Outcome::Created { path: target, facts, from: None }`; `MKDIR` kolu (yeni klasör) `placing` ise `from: None`; `COPY_DELETE` kolu aynı (`from: None`). `kind` alanı geri gelir (`MoveTask::placing` için verilen tür). Geri alma tersine `TrashTask::checked(made)` kurar → yerleştirilenler çöpe, birleştirilen klasörün önceden var olan içeriği kalır.

- [ ] **Step 4: Testler, denetimler, commit**

```bash
~/.cargo/bin/cargo test -p gezik-ops
~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-ops
git commit -m "Let jobs chain tasks, ask questions, stage files and place them as new"
```

---

### Task 2: `gezik-core::batch::archive` ve `tools` — saf kararlar

**Files:**
- Create: `crates/gezik-core/src/batch/archive.rs`, `crates/gezik-core/src/batch/tools.rs`
- Modify: `crates/gezik-core/src/batch/mod.rs`

**Interfaces:**
- Produces:
  - `pub enum Format { Zip, SevenZ, Rar, Tar(Codec), Single(Codec), Cab, Iso, Udf, Cpio, Ar, Deb, Other(String) }`, `pub enum Codec { None, Gz, Bz2, Xz, Zst }`
  - `detect(head: &[u8], name: &str) -> Option<Format>` — `head` en az 0x9010 bayt (daha kısa dosyada ne varsa)
  - `safe_join(dest: &Path, entry: &str) -> Option<PathBuf>`
  - `archive_stem(name: &str) -> &str` (`a.tar.gz` → `a`, `a.part1.rar` → `a`, `a.7z.001` → `a`, `a.zip` → `a`)
  - `volume_set(name: &str) -> Option<VolumeSet>`; `VolumeSet { first: String, pattern: VolumeKind }`; `volume_name(base: &str, kind: VolumeKind, n: u32) -> String`
  - `single_root(names: &[(String, bool)]) -> bool` (aşamanın kökündeki öğeler: ad, klasör mü)
  - `bomb_suspect(declared: u64, packed: u64) -> bool` (`declared > 10 GiB && declared / packed.max(1) > 1000`)
  - `split_sizes() -> [(&'static str, u64); 3]` = `[("100 MB", 100_000_000), ("700 MB", 700_000_000), ("4 GB (FAT32)", 4_294_967_295)]`
  - `tools::{Tool, Platform, ToolBuild, build_for(Tool, Platform) -> Option<&'static ToolBuild>, MANIFEST}`

- [ ] **Step 1: Testleri yaz**

`archive.rs` test modülü:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn head(bytes: &[u8], at: usize) -> Vec<u8> {
        let mut h = vec![0u8; at + bytes.len()];
        h[at..].copy_from_slice(bytes);
        h
    }

    #[test]
    fn formats_by_magic() {
        assert_eq!(detect(b"PK\x03\x04rest", "a.bin"), Some(Format::Zip));
        assert_eq!(detect(b"7z\xBC\xAF\x27\x1C", "x"), Some(Format::SevenZ));
        assert_eq!(detect(b"Rar!\x1A\x07\x00", "x"), Some(Format::Rar));
        assert_eq!(detect(b"Rar!\x1A\x07\x01\x00", "x"), Some(Format::Rar));
        assert_eq!(detect(b"\x1F\x8B\x08", "a.tar.gz"), Some(Format::Tar(Codec::Gz)));
        assert_eq!(detect(b"\x1F\x8B\x08", "a.tgz"), Some(Format::Tar(Codec::Gz)));
        assert_eq!(detect(b"\x1F\x8B\x08", "notes.txt.gz"), Some(Format::Single(Codec::Gz)));
        assert_eq!(detect(b"\xFD7zXZ\x00", "a.tar.xz"), Some(Format::Tar(Codec::Xz)));
        assert_eq!(detect(b"BZh9", "a.bz2"), Some(Format::Single(Codec::Bz2)));
        assert_eq!(detect(b"\x28\xB5\x2F\xFD", "a.tar.zst"), Some(Format::Tar(Codec::Zst)));
        assert_eq!(detect(&head(b"ustar\x0000", 257), "a.tar"), Some(Format::Tar(Codec::None)));
        assert_eq!(detect(b"MSCF\0\0\0\0", "a.cab"), Some(Format::Cab));
        assert_eq!(detect(&head(b"CD001", 0x8001), "a.iso"), Some(Format::Iso));
        assert_eq!(detect(b"070701", "a.cpio"), Some(Format::Cpio));
        assert_eq!(detect(b"070707", "a.cpio"), Some(Format::Cpio));
        assert_eq!(detect(b"!<arch>\ndebian-binary   ", "a.deb"), Some(Format::Deb));
        assert_eq!(detect(b"!<arch>\nfoo.o/          ", "a.a"), Some(Format::Ar));
        assert_eq!(detect(b"hello", "a.txt"), None);
    }

    #[test]
    fn rare_formats_go_to_seven_zip_by_extension() {
        for name in ["a.lzh", "a.lha", "a.arj", "a.wim", "a.dmg", "a.msi", "a.rpm", "a.z01", "a.vhd", "a.xar"] {
            assert!(matches!(detect(b"\0\0\0\0", name), Some(Format::Other(_))), "{name}");
        }
    }

    #[test]
    fn unsafe_names_are_refused() {
        let dest = Path::new("/stage");
        assert_eq!(safe_join(dest, "a/b.txt"), Some(dest.join("a").join("b.txt")));
        assert_eq!(safe_join(dest, "a\\b.txt"), Some(dest.join("a").join("b.txt")));
        assert_eq!(safe_join(dest, "./a"), Some(dest.join("a")));
        for bad in ["../x", "a/../../x", "/etc/passwd", "\\x", "C:\\x", "C:x", "\\\\srv\\s\\x", "a:stream", "", ".."] {
            assert_eq!(safe_join(dest, bad), None, "{bad}");
        }
        for reserved in ["CON", "con.txt", "a/NUL", "LPT1.log"] {
            assert_eq!(safe_join(dest, reserved), None, "{reserved}");
        }
    }

    #[test]
    fn stems_and_volumes() {
        assert_eq!(archive_stem("arsiv.tar.gz"), "arsiv");
        assert_eq!(archive_stem("Fotolar.zip"), "Fotolar");
        assert_eq!(archive_stem("big.part01.rar"), "big");
        assert_eq!(archive_stem("big.7z.003"), "big");
        assert_eq!(archive_stem("notes.txt.gz"), "notes.txt");
        let set = volume_set("big.7z.003").unwrap();
        assert_eq!(set.first, "big.7z.001");
        assert_eq!(volume_name("big.7z", set.kind, 12), "big.7z.012");
        assert_eq!(volume_set("x.part3.rar").unwrap().first, "x.part1.rar");
        assert_eq!(volume_set("x.part03.rar").unwrap().first, "x.part01.rar");
        assert!(volume_set("x.zip").is_none());
    }

    #[test]
    fn single_root_and_bombs() {
        assert!(single_root(&[("Fotolar".into(), true)]));
        assert!(single_root(&[("readme.txt".into(), false)]));
        assert!(!single_root(&[("a".into(), true), ("b.txt".into(), false)]));
        assert!(!single_root(&[]));
        assert!(bomb_suspect(11 << 30, 1 << 20));
        assert!(!bomb_suspect(11 << 30, 1 << 30));
        assert!(!bomb_suspect(1 << 30, 1));
    }
}
```

`tools.rs` testi:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_seven_zip_build_is_complete() {
        for platform in Platform::ALL {
            let build = build_for(Tool::SevenZip, platform).expect("7-Zip for every platform");
            assert!(build.url.starts_with("https://github.com/wenlar/gezik-tools/releases/download/"), "{}", build.url);
            assert_eq!(build.sha256.len(), 64);
            assert!(build.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(build.size > 0);
            assert!(!build.programs.is_empty());
        }
    }

    #[test]
    fn this_platform_is_known() {
        assert!(Platform::current().is_some());
    }
}
```

- [ ] **Step 2: Uygula**

`archive.rs`: tanıma sırası özet §8'deki gibi (özel imzalar, sonra 257'de `ustar`, sonra 0x8001'de `CD001`/`BEA01`, sonra v7 tar sağlama toplamı — v7 kontrolü: 512 baytlık başlıkta 148..156 sekizlik sağlama, baytların toplamıyla (sağlama alanı boşluk sayılarak) eşit). Gzip/xz/bz2/zstd için `Tar(c)` kararı ada göre: `.tar.gz .tgz .tar.xz .txz .tar.bz2 .tbz2 .tbz .tar.zst .tzst` → `Tar`, değilse `Single`. ISO'da 0x8001 `CD001` yoksa ama `BEA01` ve 0x8801/0x9001 `NSR02`/`NSR03` varsa `Udf`. `Other(ext)` listesi: `lzh lha arj wim swm esd dmg hfs msi rpm z01 vhd vhdx vmdk xar squashfs chm cramfs ext ext4 fat ntfs qcow2 uefi`. `safe_join` özet probe `lib.rs`'teki `safe_join`'i temel al ve ekle: bileşen `:` içeriyorsa (Windows'ta ADS; her sistemde reddet), Windows ayrılmış adlar (`gezik_core::ops::names` içindeki listeyi yeniden kullan: `validate_name(component, NameRules::Windows)` `Reserved` dönerse reddet — Windows'a özel değil, her sistemde reddet ki taşınabilir olsun), boş ad. `.`'ları atla. `archive_stem`: önce parça soneklerini (`.partN.rar`, `.7z.NNN`, `.zip.NNN`, `.NNN`), sonra çift uzantıları (`names::split_name` mantığı + `.tgz/.txz/.tbz2/.tzst`), sonra tek uzantıyı at; `Single` biçimi için yalnız sıkıştırma uzantısı atılır (`notes.txt.gz` → `notes.txt`) — `archive_stem`'i `Format` almayan iki fonksiyon yap: `archive_stem(name)` (arşivler) ve `single_stem(name)` (`.gz .xz .bz2 .zst` atar). Testi buna göre düzenle (`notes.txt.gz` satırı `single_stem`'i kullanır).

`tools.rs`:

```rust
//! The tools Gezik can download (7-Zip now; ffmpeg and pdfium later): where each build is,
//! its size and SHA-256, and which programs are inside.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    SevenZip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    WindowsArm64,
    MacArm64,
    MacX64,
    LinuxX64,
    LinuxArm64,
}

impl Platform {
    pub const ALL: [Platform; 6] = [
        Platform::WindowsX64,
        Platform::WindowsArm64,
        Platform::MacArm64,
        Platform::MacX64,
        Platform::LinuxX64,
        Platform::LinuxArm64,
    ];

    pub fn current() -> Option<Platform> {
        Some(match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Platform::WindowsX64,
            ("windows", "aarch64") => Platform::WindowsArm64,
            ("macos", "aarch64") => Platform::MacArm64,
            ("macos", "x86_64") => Platform::MacX64,
            ("linux", "x86_64") => Platform::LinuxX64,
            ("linux", "aarch64") => Platform::LinuxArm64,
            _ => return None,
        })
    }
}

pub struct ToolBuild {
    pub tool: Tool,
    pub platform: Platform,
    /// The tool's version, part of its folder name (`<data>/tools/7zip-25.01`).
    pub version: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// What is inside the download: (path in the archive, is the program to run).
    pub programs: &'static [&'static str],
    /// The archive the download is (`zip` or `tar.xz`).
    pub kind: &'static str,
}

/// Filled by `scripts/tools/prepare.ps1` (Task 9); the placeholders below fail the tests until then.
pub static MANIFEST: &[ToolBuild] = &[];

pub fn build_for(tool: Tool, platform: Platform) -> Option<&'static ToolBuild> {
    MANIFEST.iter().find(|b| b.tool == tool && b.platform == platform)
}
```

`every_seven_zip_build_is_complete` Görev 9'a kadar başarısız olur: bu testi `#[ignore = "filled in by Task 9 (scripts/tools/prepare.ps1)"]` ile işaretle; Görev 9 `ignore`'u kaldırır.

- [ ] **Step 3: Testler ve commit**

```bash
~/.cargo/bin/cargo test -p gezik-core batch::
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add crates/gezik-core
git commit -m "Recognize archive formats, check entry paths and name volumes, purely"
```

---

### Task 3: Arşiv okuma — zip, 7z, tar ailesi, tek dosya, parçalar

**Files:**
- Create: `crates/gezik-batch/src/archive/mod.rs`, `io.rs`, `zip.rs`, `sevenz.rs`, `tar.rs`, `single.rs`
- Modify: `crates/gezik-batch/Cargo.toml` (zip, sevenz-rust2, tar, flate2, lzma-rust2, bzip2, ruzstd), `crates/gezik-batch/src/lib.rs`; MSRV (sapma 12)

**Interfaces:**
- Consumes: Görev 2 `Format`, `Codec`, `detect`, `safe_join`, `volume_set`
- Produces:

```rust
/// One entry of an archive, as listed.
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub packed: Option<u64>,
    pub encrypted: bool,
}

/// What extraction may use: progress, cancel, the password question.
pub trait ExtractCx {
    /// Bytes written so far for the current entry grew by `n`.
    fn add_bytes(&self, n: u64);
    /// An entry finished.
    fn entry_done(&self);
    /// True once the job is cancelled (stop at the next chance).
    fn stopped(&self) -> bool;
    /// The archive needs a password (`retry`: the last one was wrong); `None`: skip it.
    fn password(&self, retry: bool) -> Option<String>;
    /// An entry was skipped or failed; the rest goes on.
    fn entry_failed(&self, name: &str, error: &io::Error);
}

pub trait ArchiveSource {
    /// Entries with known sizes (None for streams: tar.*, .gz…). May ask for a password
    /// (header-encrypted 7z, rar -hp).
    fn list(&mut self, cx: &dyn ExtractCx) -> io::Result<Option<Vec<Entry>>>;
    /// Writes every entry under `dest` (a staging folder), through `safe_join`.
    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> io::Result<()>;
}

/// The archive at `path` (any volume of a set opens the first), by its first bytes and name.
pub fn open(path: &Path) -> io::Result<Box<dyn ArchiveSource + Send>>;
/// Whether Gezik itself can open it (false: 7-Zip is needed).
pub fn supported(format: &Format) -> bool;
```

**Behavior (bütün biçimler):**
- Her girdi adı `safe_join(dest, name)`; `None` → `entry_failed(name, "unsafe path; skipped")`, akış biçimlerinde veri boşaltılır.
- Klasörler oluşturulur; dosyalar `File::create` + 64 KB tamponla kopyalanır, her parçada `add_bytes`, `stopped()` ise `Interrupted` hatası döner (yarım dosya silinir).
- Değişme tarihi (`File::set_modified`) korunur; Unix'te izinler `0o777` maskesiyle; Windows'ta zip DOS öznitelikleri (salt okunur, gizli) uygulanır.
- Sembolik bağlantılar: sapma 9.
- Bildirilen boyut biliniyorsa yazılan bayt `size * 1.1 + 1 MiB`'ı aşınca girdi durdurulur (`entry_failed`: "larger than the archive says; stopped"), dosya silinir.
- Bozuk girdi (CRC, kesik): `entry_failed`, dosya silinir, sonraki girdiye geçilir (akış biçimlerinde akış bozulduysa iş o arşiv için biter).
- Şifre: zip'te şifreli girdide `password(false)`, yanlışsa `password(true)` döngüsü; `None` → arşiv atlanır (`entry_failed(archive, "no password")`). 7z başlık şifreliyse `open` aşamasında `list`/`extract` şifre sorar. Aynı arşivde bulunan şifre bir kez sorulur, tüm girdilerde kullanılır.
- 7z: özet §2 tuzakları (her okuyucu sonuna kadar boşaltılır; iptal için `Err(SzError::other("cancelled"))`).
- Parçalar: `.7z.001`/`.zip.001` → `MultiFileReader` (özet `lib.rs`); `volume_set` ile ilk parça bulunur.
- tar ailesi: `decoder` (özet §3) + `tar::Archive`; `unpack_in` yerine kendi yazımımız (`safe_join` + akış kopyası) — `entry_failed` ve ilerleme için. Uzun ad/PAX yolları `e.path()` ile gelir.
- Tek dosya (`.gz .xz .bz2 .zst`): çıktı adı `single_stem(arsiv)`; gzip başlığındaki özgün ad varsa (`GzDecoder::header().filename()`) ve `safe_join`'den geçiyorsa o kullanılır.

- [ ] **Step 1: Bağımlılıklar ve `io.rs`**

Cargo satırları Global Constraints'teki gibi. `io.rs`: özet probe `lib.rs`'ten `MultiFileReader` (volumes + Read + Seek), `sevenzops.rs`'ten `SplitWriter` (Görev 6 kullanır), `tarops.rs`'ten `ZstdReader` ve `decoder` — birebir taşı, doc yorumları kısa İngilizce, `#[cfg(test)]` testleri: `MultiFileReader` 3 parçalı dosyayı tek akış gibi okur ve `seek` doğru; `SplitWriter` yazılanı parçalara böler ve sona/başa seek ile yazılanı doğru yere koyar.

- [ ] **Step 2: Testleri yaz (`crates/gezik-batch/tests/archives.rs`, bütünleşme)**

Test arşivleri testte üretilir (zip/7z/tar yazıcılarıyla; Görev 6'dan önce olduğumuz için yazıcıları testte doğrudan crate API'leriyle kullan — özet §1-3 yazım örnekleri):

```rust
use std::path::{Path, PathBuf};

use gezik_batch::archive::{self, ExtractCx};

struct Cx {
    password: Option<String>,
    asked: std::cell::Cell<u32>,
    failed: std::cell::RefCell<Vec<String>>,
    cancel_after: Option<u64>,
    bytes: std::cell::Cell<u64>,
}

impl Cx {
    fn new(password: Option<&str>) -> Cx {
        Cx { password: password.map(str::to_owned), asked: 0.into(), failed: Default::default(), cancel_after: None, bytes: 0.into() }
    }
}

impl ExtractCx for Cx {
    fn add_bytes(&self, n: u64) { self.bytes.set(self.bytes.get() + n); }
    fn entry_done(&self) {}
    fn stopped(&self) -> bool { self.cancel_after.is_some_and(|limit| self.bytes.get() >= limit) }
    fn password(&self, retry: bool) -> Option<String> {
        self.asked.set(self.asked.get() + 1);
        if retry { None } else { self.password.clone() }
    }
    fn entry_failed(&self, name: &str, _: &std::io::Error) { self.failed.borrow_mut().push(name.to_owned()); }
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-batch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The files under `root` as (relative path with '/', contents).
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> { /* walk recursively, sorted */ todo!() }
```

(`tree`'yi `std::fs::read_dir` ile özyinelemeli yaz; plan kodu buradaki `todo!()` yerine gerçek gövdeyi ister — 10 satırlık klasik yürüyüş.)

Testler (her biri: üret → `archive::open` → `extract(stage)` → `tree(stage)` beklenenle eşit):
- `zip_round_trip_with_folders_dates_and_big_file` (bir klasör, Türkçe adlı dosya `Türkçe ağaç.txt`, 70 MB'lık `large_file(true)` girdi — boyut testi yavaşlatmasın diye sıfır bayt deseni, Deflate seviye 1; değişme tarihi korunur: `metadata.modified()` 2 sn toleransla).
- `zip_aes_asks_once_and_wrong_password_skips` (AES-256 girdiler; doğru şifre → bir kez soruldu; `Cx { password: Some("wrong") }` → `asked == 2` (false sonra true) ve `failed` arşivi içerir, aşama boş).
- `zip_slip_entries_are_refused` (`zip::ZipWriter::start_file("../evil.txt")` ve `"/abs.txt"`; zip crate bu adları yazmaya izin veriyorsa testte yaz; vermiyorsa ham bayt düzenle: dosya adını aynı uzunlukta değiştirip CRC'yi değiştirme — ad CRC'ye girmez) → `failed` iki adı içerir, `stage`'in dışında dosya yok.
- `sevenz_header_encrypted_and_solid` (AES + şifreli başlık + katı blok; `num_cycles_power = 19`).
- `sevenz_volumes_open_from_any_part` (`SplitWriter` ile 3 parça yaz; `archive::open(".7z.002")` ilk parçadan açar).
- `tar_families` (`tar`, `tar.gz`, `tar.xz`, `tar.bz2`, `tar.zst`; zstd'yi ruzstd'nin `encoding::compress` (Fastest) ile üret; uzun ad (>100 bayt) korunur).
- `single_file_streams_use_their_inner_name` (`notes.txt.gz` → `notes.txt`).
- `cancel_leaves_no_half_file` (`cancel_after: Some(1 MiB)`, 5 MB'lık girdi → `extract` `Interrupted` döner, aşamada yarım dosya yok).
- `oversized_entry_is_stopped` (zip girdisinin merkez dizinindeki boyutu küçük yaz — ham düzenleme: yerel başlık ve merkez dizindeki "uncompressed size" alanlarını 10 yap, veri Stored 3 MB) → `failed` girdiyi içerir, dosya yok.

- [ ] **Step 3: Uygula** (`mod.rs`, `zip.rs`, `sevenz.rs`, `tar.rs`, `single.rs`)

`open`: ilk 0x9010 baytı oku (`read` döngüsü, kısa dosyada ne varsa), `volume_set` ile parça dizisini çöz (ilk parça yoksa `NotFound: "the first part (x.7z.001) is missing"`), `detect`, biçime göre kaynağı kur; desteklenmeyende `Unsupported` hatası (`io::ErrorKind::Unsupported`, metin "7-Zip needed"). Biçim kodunu özet §1-3'teki doğrulanmış parçalardan kur.

- [ ] **Step 4: Testler, çapraz denetim, commit**

```bash
~/.cargo/bin/cargo test -p gezik-batch
~/.cargo/bin/cargo check -p gezik-batch --target x86_64-unknown-linux-gnu
~/.cargo/bin/cargo build --workspace && ~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings && ~/.cargo/bin/cargo fmt --all -- --check
git add Cargo.toml Cargo.lock crates/gezik-batch
git commit -m "Read zip, 7z, tar families and single-file streams into a staging folder"
```

---

### Task 4: Arşiv okuma — rar, cab, iso, cpio, ar/deb

**Files:**
- Create: `crates/gezik-batch/src/archive/{rar,cab,iso,cpio,ar}.rs`, `crates/gezik-batch/tests/data/rar/*`, `tests/data/SOURCES.md` (güncelle)
- Modify: `crates/gezik-batch/Cargo.toml` (unrar-ng, cab, hadris-iso, cpio, ar; dev: hadris-iso `write`), `archive/mod.rs`, `THIRD-PARTY.md` (yeni)

**Interfaces:** Görev 3'ün `ArchiveSource`/`ExtractCx`'i; `supported` artık Rar/Cab/Iso/Cpio/Ar/Deb için true, `Udf` ve `Other` için false.

**Behavior:**
- **rar** (özet §4): `Archive::with_password`/`new`; liste `open_for_listing` (`-hp` → şifre sorusu, `MissingPassword`/`BadPassword` → `password(retry)`; RAR4 veride `BadData` + şifreli girdi → yanlış şifre say). Çıkarma girdi girdi `open_for_processing` + `read_header`; her başlıkta önce `safe_join(stage, filename)` (Windows'ta `\` ayırıcı), reddedilirse `skip()`; kabul edilirse `extract_to(hedef_dosya)`. İlerleme: girdi başına `add_bytes(unpacked_size)`; `unpacked_size > 64 MiB` ise ayrı iş parçacığında çıkar, ana iş parçacığı 200 ms'de bir hedef dosya boyutunu okuyup farkı `add_bytes` eder (sapma 7); iptalde iş parçacığı bitince dosya silinir (unrar iptal edilemez — girdi bitene kadar beklenir; not edilir). Parçalar `as_first_part()`; sonda `Err(EOpen)` → "a later part (…) is missing".
- **cab** (§5): `read_file` her çağrıda klasörü baştan çözdüğü için, LZX/MSZIP klasörünün dosyalarını **klasör sırasıyla** ve tek okuyucuyla oku: `cab.read_folder(i)` gibi bir akış API'si varsa onu kullan; yoksa dosya sayısı ≤ 200 ise `read_file`, fazlaysa "large cabinet — 7-Zip is faster" diyerek `Unsupported` (7-Zip yoluna) dön. (Uygulayıcı crate'te klasör akışı arar ve raporlar.)
- **iso** (§6): hadris-iso; RRIP adı > Joliet > ISO adı (`display_name`); çok kapsamlı dosyalar `read_file_chunked`; UDF-yalnız imaj (`Format::Udf`) → `Unsupported` ("7-Zip needed").
- **cpio**: newc crate ile; `070707` (odc) için 76 baytlık sekizlik başlık ayrıştırıcısı (alanlar: magic 6, dev 6, ino 6, mode 6, uid 6, gid 6, nlink 6, rdev 6, mtime 11, namesize 6, filesize 11; ad `namesize` bayt NUL ile; veri hemen ardından; `TRAILER!!!` ile biter) — kendi `cpio.rs`'imizde, testli.
- **ar / deb** (§7): `ar` girdileri; `.deb`'de `data.tar.*` içeriği aşamanın köküne, `control.tar.*` `DEBIAN/` klasörüne açılır (dpkg-deb düzeni), `debian-binary` atlanır.
- RAR test dosyaları: libarchive deposundan (`libarchive/test/test_read_format_rar*.rar.uu` — uuencode'lu; `python -c` ile çöz) RAR4 (`test_read_format_rar.rar.uu`), RAR4 şifreli (`test_read_format_rar_encryption_data.rar.uu`, şifre "12345678"), RAR5 (`test_read_format_rar5_*.rar.uu` arasından normal + şifreli başlık "password"), çok parçalı (`test_read_format_rar_multivolume.part0001.rar.uu`…). İndirme: `curl -L https://raw.githubusercontent.com/libarchive/libarchive/master/libarchive/test/<ad>`. Hangi dosyaların içeriği ve şifresi ne, libarchive test kaynağından (`test_read_format_rar.c`, `test_read_format_rar5.c`) okunup `SOURCES.md`'ye yazılır (dosya, kaynak URL, BSD-2 lisansı, şifre, içerik).
- ISO test dosyası testte `hadris-iso` yazıcısıyla (dev-dependency `write`) üretilir; cab `cab::CabinetBuilder`; cpio `cpio::write_cpio` (newc) ve elle yazılmış odc baytları; deb `ar::Builder` + tar.gz.

- [ ] **Step 1: Testler** (`tests/archives.rs`'e): `rar4_and_rar5_extract`, `rar_wrong_password_asks_again_then_skips` (RAR4 `BadData` ve RAR5 `BadPassword` ikisi de `password(true)`'yu tetikler), `rar_missing_volume_is_a_clear_error` (son parçayı geçici kopyadan sil), `cab_mszip_and_lzx` (LZX yazıcısı yoksa yalnız MSZIP + `None`; LZX'i makecab ile üretmek Windows'a özgü — `#[cfg(windows)]` testte `makecab /D CompressionType=LZX` çalıştır, yoksa atla), `iso_with_joliet_and_rock_ridge`, `cpio_newc_and_odc`, `deb_layout`, `udf_and_rare_formats_need_seven_zip` (`supported(&Format::Udf) == false`, `open("a.lzh")` hatası `Unsupported`).
- [ ] **Step 2: Uygula** (yukarıdaki davranış).
- [ ] **Step 3: `THIRD-PARTY.md`**: her yeni crate'in lisansı (crates.io'dan) ve unRAR lisansının 2. paragrafı (unrar-ng-sys kaynağındaki `license.txt`'ten birebir).
- [ ] **Step 4: Denetimler ve commit** (Görev 3 Step 4'teki komutlar; Linux `cargo check` unrar C++ yüzünden başarısız olursa Docker kabında `scripts/linux/test.sh` ile dene ve sonucu rapora yaz).

```bash
git commit -m "Read rar, cab, iso, cpio and deb archives"
```

---

### Task 5: Açma görevleri — `ExtractTask`, `PlaceTask` zinciri, 7-Zip yedeği, sorular

**Files:**
- Create: `crates/gezik-batch/src/tasks/mod.rs`, `extract.rs`, `place.rs`, `external.rs`
- Modify: `crates/gezik-batch/Cargo.toml` (`gezik-ops`, `gezik-platform` bağımlılıkları), `crates/gezik-platform/src/process.rs`

**Interfaces:**
- Consumes: Görev 1 (`submit_chain`, `RunCx::{ask, found, one_done, staging_dir}`, `Question`, `Answer`, `MoveTask::placing`), Görev 3-4 (`archive::open`, `ArchiveSource`, `ExtractCx`)
- Produces:

```rust
/// Where the extracted files go.
pub enum ExtractTo {
    /// Into `dir`: a single top-level item goes in as it is, several go into `dir/<stem>/`.
    Smart(PathBuf),
    /// Into `dir/<stem>/` always.
    Folder(PathBuf),
    /// Into `dir` as it is.
    Into(PathBuf),
}

/// The tasks of one "Extract": unpack each archive into a staging folder, then place it.
pub fn extract_chain(archives: Vec<PathBuf>, to: ExtractTo, seven_zip: Option<PathBuf>) -> Vec<Box<dyn gezik_ops::Task>>;

/// 7-Zip process helpers (gezik-platform::process).
pub struct ChildProcess; // spawn without a window, kill the whole tree, wait with a stop check
```

**Behavior:**
- `extract_chain` görevleri: her arşiv için bir `ExtractTask` (aşamaya açar; aşama yolu `Arc<Mutex<Option<PathBuf>>>` ile sonraki göreve geçer) ve ardından bir `PlaceTask` (`plan` anında aşamanın kökünü okur, `ExtractTo` kararını uygular, `MoveTask::placing`'in planına devreder — `PlaceTask` içinde bir `MoveTask` kurup `plan`/`run`'u ona yönlendir). Bütün zincir `submit_chain(…, Some(label))`; etiket "Extract a.zip" / "Extract 3 archives".
- `ExtractTask::plan`: arşiv başına tek kök öğe (`uncounted()`); `resources`: arşivin ve hedef klasörün yolları, `Work::Disk`.
- `ExtractTask::run`:
  1. `archive::open`; `Unsupported` ise ve `seven_zip` (yol) varsa `external.rs` ile 7-Zip'e açtır (aşağıda); yoksa hata "7-Zip needed to open this kind of archive" (arayüz bu metni tanıyıp indirme kutusunu açar — hata türü: `io::Error::other(SevenZipNeeded)` işaretli; `gezik_batch::tasks::is_seven_zip_needed(&io::Error)`).
  2. `list`; boyutlar biliniyorsa toplam açık boyut + %5 > hedef sürücünün boş alanı (`gezik_platform::fs::free_space(path)` — yoksa ekle: Windows `GetDiskFreeSpaceExW`, Unix `statvfs`) ise `ask(Confirm { "Not enough space", "<n> needed, <m> free on <drive>", ["Extract anyway", "Cancel"] })`; `bomb_suspect` ise `ask(Confirm { "Very large archive", "This archive says it unpacks to <n> (<ratio>× its size)…", ["Extract", "Cancel"] })`. İptal → `Interrupted`.
  3. `found(entries, total_size)`; `staging_dir(&target_dir)`; `extract(stage, cx_adapter)` — `cx_adapter` `ExtractCx`'i `RunCx` üzerine kurar (`add_bytes` → `RunCx::add_bytes`, `entry_done` → `one_done(0)`, `password` → `ask(Password { archive, retry })`, `entry_failed` → motorun hata satırı: `RunCx`'e `fail(path, &err)` yöntemi gerekiyorsa Görev 1'in `RunCx`'ine `pub fn fail(&self, path: &Path, err: &io::Error)` ekle (job.fail)).
  4. `Outcome::Nothing` (yerleştirme sonucu `PlaceTask`'ten gelir).
- `PlaceTask::plan`: aşama boşsa hiçbir şey; `Smart` ve `single_root` → (`stage/<tek>` → `dir/<tek>`); `Smart` çoklu veya `Folder` → (`stage` içindekiler → `dir/<stem>/` altına; `dir/<stem>` yoksa `MKDIR` ile oluşur — `MoveTask::placing`'e her kök öğe için çift ver: (`stage/a` → `dir/<stem>/a`); `make_parent` gerekir: `placing` iken ebeveyn klasörü oluşturulur); `Into` → (`stage/*` → `dir/*`). Çakışmalar 4a listesinden; klasör birleşir.
- Aşama klasörü motor tarafından iş sonunda silinir (Görev 1).
- **7-Zip yedeği (`external.rs`):** `7z x -y -bsp1 -bso0 -o<stage> [-p<pw>] -- <arşiv>`; `-bsp1` çıktısındaki `NN%` satırları ilerlemedir (`found(1, arşiv_boyutu)`, yüzde × boyut → `add_bytes` farkı); şifre: önce `-p` vermeden çalıştır; çıkış kodu 2 ve stderr "Wrong password" / "Can not open encrypted archive" → `ask(Password)` ile tekrar (`-p<pw>`); iptal → süreç ağacı sonlandırılır (`ChildProcess::kill_tree`), aşama motor tarafından silinir. 7-Zip yolları/kaçış: argüman dizisi, kabuk yok; Windows'ta `CREATE_NO_WINDOW`.
- **`gezik-platform::process::ChildProcess`:** `spawn(program, args, cwd) -> io::Result<ChildProcess>` (Windows: `CREATE_NO_WINDOW` + iş nesnesi `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, süreç ona atanır; Unix: `process_group(0)`), `stdout_lines(&mut self) -> impl Iterator<Item=String>` (ayrı iş parçacığından kanal), `wait_or_stop(&mut self, stop: impl Fn() -> bool) -> io::Result<Option<ExitStatus>>` (50 ms'de bir `try_wait`; `stop()` true ise `kill_tree` ve `None`), `kill_tree(&mut self)` (Windows: `TerminateJobObject`; Unix: `killpg(SIGKILL)`). Testleri: Windows'ta `cmd /c ping -n 30 127.0.0.1 >nul` çocuk ve torun süreç; `kill_tree` sonrası ikisi de bitmiş (`process_alive`), Unix'te `sh -c "sleep 30 & sleep 30"`.

- [ ] **Step 1: Testler** (`crates/gezik-batch/tests/extract.rs`, motorla): 
  - `extract_here_single_root_goes_in_directly` (zip içinde `Fotolar/a.jpg`) → `dir/Fotolar/a.jpg`; aşama kalmadı; Ctrl+Z → `dir/Fotolar` çöpte (çöp yoksa test ortamı kuralı — Görev 1'deki gibi).
  - `extract_here_many_roots_go_into_a_folder` (`a.txt`, `b/`) → `dir/<stem>/a.txt`.
  - `extract_into_an_existing_folder_merges_and_undoes` (hedefte `Fotolar/old.jpg`) → birleşir; undo yalnız `a.jpg`'yi çöpe atar, `old.jpg` kalır.
  - `extract_conflict_asks` (hedefte aynı adlı dosya) → `Event::Conflicts`.
  - `password_question_flow` (AES zip; ilk cevap yanlış → `Question::Password{retry: true}`; ikinci doğru → dosyalar yerinde).
  - `cancel_mid_extract_leaves_nothing` (büyük girdi, ilk `Progress` olayında `cancel`) → hedefte ve aşamada bir şey yok, `pending-deletes` boş.
  - `seven_zip_fallback` (`#[ignore]` değil: makinede `C:\Program Files\7-Zip\7z.exe` varsa onu kullanarak bir `.lzh` yerine `.wim`… — kolay üretilebilen biçim: 7-Zip'le bir `.tar` değil, gerçekten desteklenmeyen biçim olarak `x.cab` LZX çok dosya yolu ya da `.wim` (7z `a -twim`); 7z yoksa test kendini atlar ve bunu yazar).
  - `process_tree_is_killed` (platform testi, yukarıda).
- [ ] **Step 2: Uygula.**
- [ ] **Step 3: Denetimler ve commit.**

```bash
git commit -m "Extract archives through a staging folder and place them with one undo"
```

---

### Task 6: Oluşturma ve var olana ekleme

**Files:**
- Create: `crates/gezik-batch/src/archive/write.rs`, `crates/gezik-batch/src/tasks/compress.rs`, `add.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat { Zip, SevenZ, Tar, TarGz, TarXz, Gz, Xz }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level { Store, Fast, Normal, Best }

pub struct CompressOptions {
    pub format: OutFormat,
    pub level: Level,
    pub password: Option<String>,
    /// 7z: encrypt the file names too.
    pub encrypt_names: bool,
    /// 7z: split into parts of this many bytes.
    pub split: Option<u64>,
}

/// Compresses `sources` (relative to their common parent) into `target`.
pub struct CompressTask; // CompressTask::new(sources: Vec<PathBuf>, target: PathBuf, options: CompressOptions)
/// Adds `sources` to the archive `archive` (zip, 7z, tar family).
pub struct AddToArchiveTask; // AddToArchiveTask::new(archive: PathBuf, sources: Vec<PathBuf>, password: Option<String>)

/// The archive name for `sources` (one item: its name; several: their folder's name) with the format's extension.
pub fn default_name(sources: &[PathBuf], format: OutFormat) -> String;
```

**Behavior:**
- Düzey eşlemesi: zip Deflate `Store`→Stored, `Fast`→1, `Normal`→6, `Best`→9; 7z LZMA2 `Fast`→1, `Normal`→5, `Best`→9, `Store`→ kopyala (`Lzma2Options::from_level(0)` yoksa "Copy" yöntemi); tar.gz flate2 seviye aynı; tar.xz `XzOptions::with_preset(1/6/9)`, büyük girdide `XzWriterMt` (blok 8 MiB, çekirdek sayısı); `Store` tar.* için sıkıştırmasız `.tar`.
- Yazım her zaman `RunCx::temp_file_for(target)`'a; bitince `fs::move_entry(temp, target)`; hedef varsa plan çakışma listesinden geçer (`checked()`); iptalde geçici dosya silinir; `Outcome::Created`. 7z parçaları: `SplitWriter` geçici taban adla (`.gezik-copying-…7z`), bitince her parça `target.7z.001…`'e taşınır; sonuç `Outcome::Several(Created…)`.
- Yollar seçimin ortak üst klasörüne göre; klasörler özyinelemeli; `.gezik-*` adları atlanır; sembolik bağlantılar zip/7z'de atlanır (hata satırı), tar'da bağlantı olarak yazılır.
- Paralel zip (sapma 5): dosyalar boyuta göre işçilere (çekirdek sayısı) dağıtılır; her işçi mini zip üretir (≥ 64 MiB girdi geçici dosyada); ana iş parçacığı sırayla `merge_archive`; ilerleme işçilerin okuduğu baytlardan.
- AES: zip `AesMode::Aes256`; 7z `AesEncoderOptions` + `num_cycles_power = 19` + `set_encrypt_header(encrypt_names)`.
- Zip64: `set_auto_large_file()`.
- `AddToArchiveTask`: zip → yeni geçici dosya, var olan girdiler `raw_copy_file` ile (şifreli olanlar da), yeniler eklenir; aynı adlı girdi varsa plan çakışma listesini açar (Replace: eski girdi kopyalanmaz; Skip: yeni eklenmez; Keep both: `ad (2).ext`) — çakışma kaynağı arşivin içindeki ad olduğundan motorun dosya sistemi çakışma denetimi kullanılamaz: `plan` öncesi arşivi listeleyip çakışanları `ConflictItem` yerine `RunCx::ask(Confirm { "<n> items already in the archive", list…, ["Replace", "Skip", "Keep both", "Cancel"] })` ile tek soru olarak sor (sapma: satır başına karar yok; bilerek basit). 7z ve tar ailesi: arşivi aşamaya aç (Görev 3), yenileri ekle, aynı biçim ve düzeyde yeniden oluştur. Sonra eskisi çöpe (`trash` — RunCx'in çöp yolu: mevcut `TrashTask` mantığını `gezik_ops::trash_path(path) -> io::Result<PathBuf>` olarak dışa aç), yenisi asıl adına: `Outcome::Several([Trashed, Created])`. Çöpü olmayan sürücüde görev başlamadan hata: "Adding needs a Recycle Bin to undo" (spec 5.4). Şifreli arşive eklerken şifre sorulur, yeni girdiler aynı şifreyle şifrelenir.
- `default_name`: tek öğe → `a` (dosyada uzantısız gövde) + `.zip`; çok öğe → ortak üst klasörün adı; kök sürücü ise "Archive".

- [ ] **Step 1: Testler** (`tests/compress.rs`): her `OutFormat` için oluştur → Görev 3 okuyucusuyla aç → ağaç aynı; `zip_parallel_matches_serial_content` (100 dosya, 4 işçi; 7-Zip varsa `7z t` ile doğrula — `#[cfg(windows)]` ve 7z yoksa atla); `sevenz_split_parts_open` (3 parça); `aes_zip_and_7z_need_the_password`; `cancel_removes_the_temp_file`; `add_to_zip_keeps_aes_entries` (AES girdili zip'e ekle → okuyucu doğru şifreyle hepsini açar; 7z varsa `7z t -p…` başarılı); `add_to_tar_gz_repacks`; `add_undo_restores_the_old_archive` (motorla; çöp kuralı Görev 1'deki gibi); `default_names`.
- [ ] **Step 2: Uygula.**
- [ ] **Step 3: Denetimler ve commit.**

```bash
git commit -m "Create zip, 7z and tar archives, in parallel where it helps, and add to existing ones"
```

---

### Task 7: Araç indirme ve 7-Zip bulma

**Files:**
- Create: `crates/gezik-batch/src/tools.rs`, `crates/gezik-batch/src/tasks/download.rs`
- Modify: `crates/gezik-batch/Cargo.toml` (`sha2`), `crates/gezik-config/src/settings.rs` (`[tools]`, `[archives]`), şablon

**Interfaces:**

```rust
/// Where a tool's program is: the path in settings, Gezik's download, then PATH.
pub fn find(tool: Tool, data_dir: &Path, configured: Option<&Path>) -> Option<PathBuf>;
/// `<data>/tools/<name>-<version>/`.
pub fn install_dir(build: &ToolBuild, data_dir: &Path) -> PathBuf;
/// Downloads, checks and unpacks `build` into `install_dir`; then removes older versions.
pub struct DownloadTask; // DownloadTask::new(build: &'static ToolBuild, data_dir: PathBuf) + with_url(String) for tests
pub fn is_damaged(err: &io::Error) -> bool;
```

Ayarlar (`gezik-config`): `Settings::tools: ToolsSettings { download: bool /* default true */, seven_zip: Option<String> }` (`[tools] download = true`, `seven-zip = ""` — boş = yok), `Settings::archives: ArchivesSettings { double_click: DoubleClick /* System | ExtractHere */ }`; `State::archive: ArchiveState { format, level, split, last_extract_to: Option<String> }` (`[archive]` tablosu). Geçersiz değer → uyarı (mevcut kalıp). Şablona yorumlu `[archives]` ve `[tools]` bölümleri (spec 10.1 metni İngilizce yorumlarla).

**Behavior (`DownloadTask`):**
- `Work::External`; `resources`: veri klasörü.
- `curl` bulunur (PATH; Windows'ta `%SystemRoot%\System32\curl.exe`); yoksa hata "curl not found". Komut: `curl --fail --location --proto =https --max-redirs 5 --silent --show-error --output <temp> <url>` (testte `with_url` yerel `http://127.0.0.1:<port>/…` kullanır; `--proto =https` test için `--proto =http,https` olur: `DownloadTask` URL'nin şemasına göre seçer, ama `MANIFEST` adresleri yalnız https).
- `found(1, build.size)`; ilerleme: 200 ms'de bir geçici dosyanın boyutu → `add_bytes` farkı; `ChildProcess::wait_or_stop(cx.stopped)`.
- Bitince SHA-256 (`sha2`, 1 MiB tampon) ≠ `build.sha256` → geçici dosya silinir, `io::Error::other(Damaged)` ("download damaged — try again").
- Doğruysa `archive::open` + `extract` (Görev 3) ile `install_dir`'in yanındaki gizli aşama klasörüne açılır, `move_entry` ile `install_dir`'e taşınır (yarım kurulum görünmez); Unix'te `programs`'a `0o755`; macOS'ta ek işlem yok (spec 8.2).
- Eski sürüm klasörleri (`<data>/tools/<name>-*` içinden bu sürüm dışındakiler) silinir.
- Sonuç `Outcome::Nothing` (geri alınmaz, geçmişe girmez — spec 3.3).
- `find`: `configured` dosya ve çalıştırılabilir ise o; sonra `install_dir(build_for(tool, current))`'teki program; sonra PATH'te `7z`/`7zz`/`7za` (Windows `7z.exe`); Windows'ta ayrıca `%ProgramFiles%\7-Zip\7z.exe`.

- [ ] **Step 1: Testler:** `download_checks_the_hash_and_installs` (test içinde `std::net::TcpListener` + iş parçacığıyla en küçük HTTP/1.1 sunucusu: GET'e `200 OK`, `Content-Length` ve gövde; gövde Görev 6 yazıcısıyla üretilmiş, içinde `7z.exe` adlı küçük dosya olan zip; `ToolBuild` testte `Box::leak` ile kurulur, `sha256` gövdeden hesaplanır) → `install_dir`'de program var; `a_bad_hash_leaves_nothing` (yanlış özet → `is_damaged`, `install_dir` yok, geçici dosya yok); `cancel_kills_curl` (sunucu gövdeyi yavaş yollar: 64 KB'da bir 100 ms bekler; ilk ilerlemede iptal → iş biter, geçici dosya yok); `find_prefers_settings_then_download_then_path`; ayar ayrıştırma testleri (`gezik-config`).
- [ ] **Step 2: Uygula.**
- [ ] **Step 3: Denetimler ve commit.**

```bash
git commit -m "Download tools with curl, check their SHA-256 and install them per version"
```

---

### Task 8: Arayüz — menüler, Compress katmanı, sorular, araç kutusu, arşive sürükleme

**Files:**
- Create: `crates/gezik/src/archives.rs`, `crates/gezik/ui/widgets/compress.slint`
- Modify: `crates/gezik/src/{context_menu.rs, operations.rs, main.rs, drag.rs, dialog.rs}`, `crates/gezik/ui/{app.slint, widgets/dialog.slint}`, `crates/gezik/src/view/*` (çift tıklama), `crates/gezik/Cargo.toml` (gerekirse)

**Behavior:**
- **Menü öğeleri** (macOS/Linux Gezik menüsü; Windows'ta Gezik öğeleri listesinin başına, Shell menüsünün üstüne): seçimdeki her öğe tanınan bir arşivse (tanıma arka planda: menü açılırken uzantıya göre hızlı karar — uzantı listesi `gezik_core::batch::archive::looks_like_archive(name) -> bool`, Görev 2'ye ekle; gerçek biçim iş başında) **Extract here**, **Extract to "<stem>\"**, **Extract to…**; her seçimde **Compress…** ve **Compress to "<default_name>.zip"** (son biçimle: "Compress to \"x.7z\""); tek arşiv + başka öğeler seçiliyse yok. Kimlikler `context_menu.rs`'te yeni aralık (çakışma testi güncellenir).
- **Extract to…:** `ask_text("Extract to", "Folder:", <son ya da bulunulan klasör>, ["Extract", "Cancel"])`; yol mutlak değilse bulunulan klasöre göre; boşsa iptal.
- **Çift tıklama:** `[archives] double-click = "extract-here"` ise arşivde Extract here; değilse mevcut davranış.
- **Sorular:** `Operations::drain` `Event::Question`'ı karşılar: `Password { archive, retry }` → `ask_text` (başlık "Password", mesaj "<archive> is encrypted. Password:" / retry'da "Wrong password. Try again:", düğmeler ["OK", "Skip"]; alan **gizli**: `Dialog`'a `in property <bool> input-secret` + `TextInput { input-type: password }` ve "Show" onay kutusu) → `engine.answer(job, Text|Cancel)`; `Confirm { title, message, buttons }` → `ask_escape` → `Button(i)` / `Cancel`. Soru açıkken başka soru kuyruğa girer (Dialogs zaten kuyruklu).
- **Compress katmanı (`compress.slint`):** ad alanı, klasör alanı (metin, bulunulan klasör), biçim düğmeleri (zip / 7z / tar.gz / tar.xz / tar; tek dosyada .gz / .xz), düzey düğmeleri (Store/Fast/Normal/Best), şifre alanı (gizli, "Show"; yalnız zip/7z'de etkin), "Encrypt file names" (7z), parça (yalnız 7z: None / 100 MB / 700 MB / 4 GB (FAT32) / Custom… → MB alanı), toplam girdi boyutu (arka planda hesaplanır, "Calculating…"), alt satır Cancel / Compress; "Add to existing archive…" bağlantısı → `ask_text` ile arşiv yolu → `AddToArchiveTask`. Son seçimler `State::archive`'a. Katman 5a katmanı kalıbında (`rb-*` gibi `cp-*` özellikleri, tuşlar `handle_key` yakalama aşamasında: Esc kapatır, Ctrl+Enter sıkıştırır).
- **Araç kutusu:** iş `is_seven_zip_needed` hatasıyla biterse (ya da menüden açılan bir arşiv `Other`/`Udf` ise iş başlamadan) diyalog: "Opening .lzh archives needs 7-Zip (~<size>, free)." düğmeler ["Download", "Cancel"] (+ "Where does it come from?" → `open::that` ile `https://github.com/wenlar/gezik-tools` ve 7-Zip lisans sayfası); Linux'ta mesaja "or install it with your package manager (p7zip / 7zip)" eklenir; `[tools] download = false` ise yalnız bilgi ve "OK". Download → `DownloadTask` gönderilir; bitince aynı açma işi otomatik yeniden gönderilir.
- **Arşive sürükleme:** 4b'nin bırakma hedefleri bir zip/7z/tar.* dosyası satırını da hedef sayar (`gezik_core::drag` hedef türüne `Archive` ekle; etiket "Add to <name>"); bırakınca `AddToArchiveTask`. Sağ tuşla sürüklemede menüye "Add to archive" eklenir.
- **Panel:** `TaskKind::Extract/Compress/AddToArchive/Download` başlıkları görevlerin `title()`'ından ("Extracting a.zip to D:\İndirilenler", "Compressing 12 items to Fotolar.zip", "Downloading 7-Zip").

- [ ] **Step 1: Saf parçalar için testler** (`archives.rs`: menü öğesi listesi seçime göre; `default_name`; Extract to yol çözümü; soru metinleri), `context_menu` kimlik çakışma testi.
- [ ] **Step 2: Uygula** (Slint + Rust).
- [ ] **Step 3: Derle, başlat (geçici `GEZIK_CONFIG_DIR`, birkaç saniye, kapat), denetimler, commit.**

```bash
git commit -m "Extract and compress from the menus, ask for passwords and offer to download 7-Zip"
```

---

### Task 9: Araç dosyaları, özetler ve lisans notları

**Files:**
- Create: `scripts/tools/prepare.ps1`, `scripts/tools/README.md`
- Modify: `crates/gezik-core/src/batch/tools.rs` (`MANIFEST` dolu, `#[ignore]` kalkar), `THIRD-PARTY.md`

**Behavior:**
- `prepare.ps1 -Out <klasör>`: 7-Zip'in resmi sürümünü (7-zip.org, en son kararlı, ör. 25.01) indirir:
  - Windows x64/arm64: `7z<sürüm>-x64.exe`/`-arm64.exe` kurulum paketi — içinden `7z.exe` ve `7z.dll`'yi makinedeki 7-Zip (`7z x`) ile çıkarır, `7zip-<sürüm>-windows-x64.zip` olarak paketler (`7z.exe`, `7z.dll`, `License.txt`).
  - macOS: `7z<sürüm>-mac.tar.xz` (evrensel `7zz`) → `7zip-<sürüm>-macos-arm64.tar.xz` ve `-macos-x64.tar.xz` (aynı evrensel ikili).
  - Linux x64/arm64: `7z<sürüm>-linux-x64.tar.xz` / `-linux-arm64.tar.xz` → `7zz`.
  - Her dosyanın boyutu ve SHA-256'sı hesaplanır, `tools.rs`'teki `MANIFEST` için Rust satırları `manifest.rs.txt` olarak yazılır (adresler `https://github.com/wenlar/gezik-tools/releases/download/7zip-<sürüm>-1/<dosya>`).
- Betik bu makinede çalıştırılır, üretilen satırlar `MANIFEST`'e konur, `every_seven_zip_build_is_complete`'in `#[ignore]`'u kalkar.
- `scripts/tools/README.md`: depo düzeni, yayınlama adımları (`gh repo create wenlar/gezik-tools --public`, `gh release create 7zip-<sürüm>-1 <dosyalar> --notes …` — notlar lisans metni ve kaynak bağlantısı), güncelleme akışı.
- **Yükleme:** `gh` kurulu ve oturum açıksa (`gh auth status`) depo ve sürüm oluşturulur, dosyalar yüklenir, ardından bir dosya `curl -L` ile indirilip özeti doğrulanır. `gh` yoksa adım atlanır ve raporda "upload pending: gh not installed" yazar (yükleme sonra yapılır; bildirim adresleri değişmez).
- `THIRD-PARTY.md`'ye 7-Zip (LGPL + unRAR kısıtı) notu.

- [ ] **Step 1: Betik, çalıştırma, MANIFEST, test (`cargo test -p gezik-core tools`), commit.**

```bash
git commit -m "Prepare 7-Zip downloads for every platform and pin their SHA-256"
```

---

### Task 10: Ölçüm, Linux kabı, notlar

**Files:**
- Create: `scripts/perf/archives.ps1`, `crates/gezik-batch/examples/archive_bench.rs`
- Modify: `scripts/linux/Dockerfile` (g++), notlar, spec Durum

- [ ] **Step 1:** `archive_bench`: verilen klasörü zip'e (Normal) Gezik'le sıkıştırma ve açma süreleri; `archives.ps1`: 1 GB karışık klasör (1000 küçük + 20 büyük sıkıştırılabilir/sıkıştırılamaz dosya) üretir, Gezik süreleri ile Windows'un `Compress-Archive` (Sıkıştırılmış klasöre gönder'in PowerShell karşılığı; ayrıca `Shell.Application` `CopyHere` ile .zip'e kopya) ve `Expand-Archive`/Shell açma sürelerini ölçer, temizler (try/finally).
- [ ] **Step 2:** Sürüm exe boyutu (5a sonrası ile karşılaştır; hedef ≤ +3 MB).
- [ ] **Step 3:** `scripts/linux/Dockerfile`'a `g++` (unrar C++); kapta `cargo build -p gezik` ve `cargo test -p gezik-batch` (`scripts/linux/test.sh` mevcut akışıyla); sonuçlar notlara.
- [ ] **Step 4:** Notlar (Türkçe, 5a bölümünün kalıbında): ölçümler, sapmalar, denenemeyenler (macOS; Linux'ta GUI), `gh` yükleme durumu. Spec `Durum`: "5a, 5b uygulandı".
- [ ] **Step 5:** Tüm denetimler, commit.

```bash
git commit -m "Measure archives against Windows' own and note what 5b does"
```

---

## Ekran testleri (alt planın sonunda, Win32 otomasyonuyla)

`GEZIK_CONFIG_DIR` geçici; test verisi `%TEMP%\gezik-gui-5b\`: `photos.zip` (tek kök `photos/`), `mixed.zip` (çok kök), `secret.zip` (AES, şifre "pw"), `big.7z` (çok parçalı 3 parça), `a.tar.gz`, RAR (test verisinden), `x.wim` (7-Zip ile; 7-Zip indirme akışı için `[tools] seven-zip` boş ve PATH'te 7z yokken — PATH'i Gezik'i başlatırken daraltarak), boş bir hedef klasör, `Fotolar\old.jpg` içeren klasör.

1. `photos.zip` → Extract here → `photos\` doğrudan; panelde "Extracting…" satırı; Ctrl+Z → `photos\` çöpte.
2. `mixed.zip` → Extract here → `mixed\` klasörü.
3. Var olan `Fotolar\` ile çakışan arşiv → çakışma listesi; Start → birleşme; Ctrl+Z yalnız yenileri kaldırır.
4. `secret.zip` → şifre sorusu (alan gizli, Show çalışır); yanlış → "Wrong password"; doğru → açılır; Skip → hiçbir şey açılmaz.
5. `big.7z.002` seçip Extract here → ilk parçadan açılır.
6. RAR ve `a.tar.gz` açılır.
7. Extract to… → yol sorusu → başka klasöre açılır.
8. Compress… katmanı: zip Normal → `x.zip` oluşur, panel ilerlemesi; 7z + şifre + "Encrypt file names" + 100 MB parça (büyük dosyayla) → `.7z.001…`; Esc kapatır.
9. Compress to "x.zip" (katmansız) çalışır.
10. Dosyaları `x.zip` satırının üstüne sürükle → "Add to x.zip" etiketi → eklenir; Ctrl+Z → eski arşiv geri.
11. `x.wim` → "needs 7-Zip" kutusu → Download (indirme deposu yoksa: kutunun açıldığını ve Download'ın panelde "Downloading 7-Zip" satırı açtığını doğrula; ağ/depo yoksa hata satırının "download failed"/"damaged" dediğini; depo hazırsa açılır).
12. Açma sürerken iptal → hedefte ve klasörde `.gezik-*` artığı yok.
13. `[archives] double-click = "extract-here"` ile çift tıklama açar.
