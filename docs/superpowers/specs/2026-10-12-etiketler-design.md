# Alt Proje 12: Etiketler — Tasarım

- **Tarih:** 2026-10-12
- **Durum:** Kapsam kullanıcı kararı (2026-10-12: dört başlığın hepsi, §2). Ayrıntı kararlarını kullanıcıya sormadan bu spec verdi (§19, her biri gerekçesiyle). Spec onaylanınca plan önceden onaylıdır. **12a uygulandı** (`feat/tags-12a`, plan `2026-10-12-etiketler-12a.md`: saklama çekirdeği arayüzsüz; `[tags]` ayarı 12b'ye kaldı, sapmalar planda). **12b uygulandı** (`feat/tags-12b`, 12a'nın üstüne yığılı, plan `2026-10-12-etiketler-12b.md`: seçici, `tags`/`tag-1…7`/`clear-tags`, menüler, palet, `[tags]` ayarı, görünen satırların tembel okuması ve etiket noktaları; sapmalar planda). **12c uygulandı** (`feat/tags-12c`, 12b'nin üstüne yığılı, plan `2026-10-12-etiketler-12c.md`: Tags sütunu, etikete göre sıralama ve gruplama, süzgeçte `tag:`, gerektiğinde bütün klasörün arka planda okunması, 12a'nın yeniden bulmasının bağlanması; sapmalar planda). **12d uygulandı** (`feat/tags-12d`, 12c'nin üstüne yığılı, plan `2026-10-12-etiketler-12d.md`: seçicide yıldızlar ve `Comment…`, `edit-comment`, Rating ve Comment sütunları, puana göre sıralama ve gruplama, süzgeçte `rating:`, Windows'ta Explorer'ın etiket ve puanı salt okunur; sapmalar planda). **12e uygulandı** (`feat/tags-12e`, 12d'nin üstüne yığılı, plan `2026-10-12-etiketler-12e.md`: kenar çubuğunda TAGS, tık ile arama (macOS Spotlight), satıra bırakınca etiketleme, liste yönetimi, aramada etiket/puan/yorum ölçütleri ve sözcükleri, `Clean Up Tag Store…`; sapmalar planda). **12f uygulandı** (`feat/tags-12f`, 12e'nin üstüne yığılı, plan `2026-10-12-etiketler-12f.md`: `[[highlight-rules]]` ayar yüklenince derlenir, ilk uyan kural satırın rengini ve kalın adını verir, meta yalnız meta koşullu kural varken okunur; sapmalar planda).
- **Kapsam:** Gezik yol haritasının 12. alt projesi (`docs/superpowers/notes/2026-10-07-rakip-ozet.md`, "Etiketler": satır 18, 25, 50; §4 satır 111; açık soru 10). Tek spec, altı parça (12a–12f; §15).
- **Dayandığı:** `2026-10-04-dosya-islemleri-design.md` (iş motoru, geri alma, iş paneli), `2026-10-07-klavye-paketi-design.md` (kısayol tablosu, süzgeç çubuğu), `2026-10-09-arama-design.md` (arama tarifi, tarayıcı, kayıtlı aramalar), `2026-10-10-sistem-butunlesmesi-design.md` (macOS xattr'ları, bulut yer tutucuları, sürücü bilgisi), `2026-10-11-duzenler-design.md` (gruplama 10d, görünüm kuralları 10f, kenar çubuğu 10c), `2026-10-11-araclar-design.md` (iş motorunda okuyan işler, ortak tablo, süreç dersleri).
- **Taban:** `master` c012482. `Action::ALL` = 113 (PR #87'den sonra sayı yalnız dizinin türünde durur; testler sayıyı ayrıca yazmaz), menü kimlikleri 2339'a kadar dolu (11: 2200–2399 ayrılmış). Exe boyutu ve boşta bellek her parçanın ilk görevinde yeniden ölçülür (11'in son ölçümleri notlarda).
- **Rakip notları:** Finder (etiketler, ⌃1–⌃7, kenar çubuğunda etiket listesi, etikete göre gruplama), Dolphin/Baloo (`user.xdg.tags`, `user.baloo.rating`, `user.xdg.comment`), TC (renk kuralları, `descript.ion`), DO (etiketler, puan, yorum sütunları), XYplorer (etiket veritabanı, renkli vurgulama).

## 1. Amaç

Dosyalara Finder'daki gibi **adlı, renkli etiketler**, **1–5 puan** ve **kısa bir yorum** eklemek; bunları sütunda göstermek, onlarla süzmek, sıralamak, gruplamak ve aramak; ve satırları kullanıcının kurallarına göre **renklendirmek/kalın yazmak** (TC'nin renk kuralları). macOS'ta Gezik ile Finder aynı etiketleri görür: Gezik Finder'ın kendi kaydını okur ve yazar. Windows ve Linux'ta etiket dosyanın yanında, dosya sisteminin kendi meta veri yerinde (NTFS akışı, xattr) durur; bunu desteklemeyen yerlerde Gezik'in küçük merkezi kaydında.

Hafiflik bozulmaz: **etiketlere hiçbir şey ihtiyaç duymadıkça hiçbir dosyanın meta verisi okunmaz.** Etiket kullanmayan bir Gezik bugünkü Gezik kadar hafiftir; boşta zamanlayıcı, yoklama, izleyici yok.

### Başarı ölçütleri

- **Saklama (12a):** macOS'ta Finder'da verilen etiket Gezik'te aynı ad ve renkle görünür, Gezik'te verilen Finder'da (liste, Get Info, kenar çubuğu araması) görünür. Linux'ta Dolphin'in etiketi, puanı ve yorumu Gezik'te görünür, tersi de. Windows'ta NTFS'te etiket dosyayla birlikte kopyalanır, taşınır, yeniden adlandırılır, çöpe gidip geri gelir. FAT/exFAT USB'de, salt okunur yerde ve akış desteklemeyen ağ paylaşımında etiket merkezi kayıtta durur ve Gezik'in kendi taşıma/yeniden adlandırma/geri almalarında dosyayı izler.
- **Toplu etiketleme (12b):** seçime etiket ekleme/çıkarma tek iş, tek geri alma; 10.000 dosyada SSD'de ≤ 3 sn (Windows NTFS), ≤ 1 sn (Linux/macOS). Kısayolla tek etiket aç/kapa: tek tuş, sorusuz.
- **Sütun, süzme, sıralama, gruplama (12c):** görünen 40 satırın meta verisi arka planda ≤ 10 ms (SSD); 10.000 öğelik klasörde etikete göre sıralama/gruplama için tam okuma Windows'ta ≤ 1 sn, Linux/macOS'ta ≤ 300 ms.
- **Puan ve yorum (12d):** satırda yıldızlar, yorum sütunu, puana göre sıralama/gruplama, `rating:` süzgeci; Windows'ta Explorer'ın "Tags" (System.Keywords) ve puanı (System.Rating) salt okunur görünür.
- **Arama ve kenar çubuğu (12e):** arama tarifinde etiket, en az puan, yorum metni; kenar çubuğunda etiket listesi; macOS'ta kenar çubuğundaki etiket bütün diskte Spotlight'la (Finder gibi) ≤ 1 sn.
- **Vurgulama kuralları (12f):** `[[highlight-rules]]` ayar yüklenince bir kez derlenir; satır başına maliyet ≤ 1 µs (etiket/puan koşulu olmayan kurallarda G/Ç yok).
- **Hafiflik (hepsi):** varsayılan ayarlarla ve hiç etiket kullanılmadan boşta bellek `master`'a göre **0** (±0,02 MB ölçüm gürültüsü), açılış ≤ +1 ms (yalnız ayar anahtarları), klasör açma süresi `master` ±%2. Her parça exe'ye ≤ +262.144 B (§14). Yeni Slint katmanı yok (§3.6).

### Kapsam dışı (bilerek)

- **Etiket dizini/veritabanı** (XYplorer'ın her dosyayı tutan veritabanı): etiketler dosyanın yanında durur; "etiketi taşıyan bütün dosyalar" aramayla bulunur (macOS'ta Spotlight). Merkezi kayıt yalnız geri düşüş yeridir (§4.4).
- **Etiketi bütün dosyalarda yeniden adlandırma** (Finder'ın sağ tık ▸ Rename Tag'i): Gezik'in listesinde ad değişir, dosyalardaki eski ad "listede olmayan etiket" olarak kalır (§7.3, karar 21).
- **TC'nin `descript.ion`'u ve `.2do` klasör notları:** okunmaz, yazılmaz (karar 24).
- **Explorer'ın "Tags"ına (System.Keywords) ve puanına yazma:** dosya içeriğini değiştirir (JPEG/Office yeniden yazılır); kullanıcı kararı yalnız okuma.
- **Arşivlere, buluta ve e-postaya etiket taşıma:** sıkıştırma etiketleri arşive yazmaz, açma geri getirmez; bulut eşitleyicilerinin taşıyıp taşımadığı onların işi (§17).
- **Akıllı klasörler** (Finder'ın Smart Folder'ı): kayıtlı aramalar (8) etiket ölçütüyle bunu karşılar.
- **Ad şablonunda, eşitleme karşılaştırmasında etiket:** 13. adımın (Önizleme 2) meta veri alanı kaydına kalır (rakip özeti satır 66).

## 2. Kullanıcı kararları

| Konu | Karar | Kaynak |
|---|---|---|
| Kapsam | Dört başlığın hepsi: (1) renk etiketleri, (2) puan ve yorum, (3) vurgulama kuralları, (4) toplu etiketleme | Kullanıcı kararı 2026-10-12 |
| Etiketler | Finder gibi adlı, renkli etiketler; kenar çubuğunda etiket listesi; etikete göre süz, sırala, grupla (10d), ara (8) | Kullanıcı kararı 1 |
| Puan, yorum | 1–5 puan ve dosya başına kısa yorum; sütun olarak; aramada kullanılır | Kullanıcı kararı 2 |
| Vurgulama | Ayarlarda kurallar (ad/desen, tür, yaş, boyut, etiket, puan…) → satır rengi / kalın; ayar yüklenince bir kez derlenir, satır başına ucuz (10f'nin görünüm kurallarından öğrenerek) | Kullanıcı kararı 3 |
| Toplu etiketleme | Seçime etiket ekle/çıkar, paletten, kısayollarla (boş varsayılan tuşlar; macOS karşılıkları) | Kullanıcı kararı 4 |
| Saklama | macOS Finder'ın kendi etiketleri (`com.apple.metadata:_kMDItemUserTags`, Finder etiket renkleri) okunur ve yazılır. Windows NTFS akışı; Linux `user.*` xattr (`user.xdg.tags` düşünülerek); desteklenmeyen yerlerde yapılandırma klasöründe küçük merkezi kayıt, kararlı dosya kimliğiyle; Gezik'in taşımalarında etiket izler; dışarıdaki taşımalarda ne olduğu yazılır. Explorer'ın "Tags"ı yalnız okunur, dosya içeriğine asla yazılmaz | Kullanıcı kararı |
| Hafiflik | Gerekene kadar hiçbir şey okunmaz; yalnız görünen satırlar ve yalnız sütun, süzgeç, vurgulama kuralı ya da etiket kenar çubuğu istediğinde; boşta değişiklik yok; parça başına bütçe; yeni katman yerine var olan bileşenler | Ürün hedefi |
| Soru | Ayrıntı kararları kullanıcıya sorulmaz; spec'te gerekçeyle (§19) | Bakımcı kuralı |

## 3. Ortak yapı

### 3.1 Meta veri modeli

`gezik-core/src/meta.rs` (yeni, saf):

```rust
pub enum TagColor { None, Gray, Green, Purple, Blue, Yellow, Red, Orange } // Finder's numbers 0–7
pub struct Tag { pub name: Box<str>, pub color: TagColor }
pub struct Meta {
    pub tags: Vec<Tag>,       // the file's order
    pub rating: u8,           // 0 = not rated, 1–5
    pub comment: Box<str>,    // ≤ 1,000 characters, one paragraph
}
```

- **Etiket adı:** 1–64 karakter, baş/son boşluksuz; **virgül ve satır sonu yasak** (Linux'un `user.xdg.tags` ayırıcısı virgül, macOS'un kaydı satır sonuyla renk ekler; karar 6). Ad karşılaştırması harf duyarsız (`pattern::fold_text`, Türkçe İ/ı dahil) ve NFC.
- **Renk numaraları Finder'ınkiler** (0 yok, 1 gri, 2 yeşil, 3 mor, 4 mavi, 5 sarı, 6 kırmızı, 7 turuncu); üç sistemde aynı sayılar yazılır, böylece kayıtlar arası çeviri yoktur.
- `Meta::is_empty()`: etiketsiz, puansız, yorumsuz; boş meta yazmak kaydı **siler** (akış/xattr kaldırılır, merkezi kayıttan çıkar).
- Bellekte boş meta tutulmaz: önbellekler (§3.4) yalnız boş olmayanları ve "okundu, boş" bilgisini bir bit kümesiyle tutar.

### 3.2 Nerede saklanır

| Sistem | Etiketler | Puan | Yorum | Desteklenmeyen yerde |
|---|---|---|---|---|
| **macOS** | `com.apple.metadata:_kMDItemUserTags` (ikili plist, `"Ad\nN"` dizgileri dizisi) + `com.apple.FinderInfo`'nun etiket rengi bitleri (§4.1) | `com.apple.metadata:kMDItemStarRating` (ikili plist tamsayı) | `com.apple.metadata:kMDItemFinderComment` (ikili plist dizgi) | APFS/HFS+ dışında macOS xattr'ı kendisi `._` (AppleDouble) dosyasında tutar; yalnız salt okunur ya da xattr reddeden ağ bağlamalarında merkezi kayıt |
| **Linux** | `user.xdg.tags` (virgülle ayrılmış, UTF-8; freedesktop önerisi, Dolphin/Baloo'nun kullandığı) | `user.baloo.rating` (`"0"`–`"10"`; Gezik yıldız × 2 yazar, okurken yukarı yuvarlar) | `user.xdg.comment` | FAT/exFAT/NTFS-3G'siz, `nouser_xattr`, tmpfs (eski çekirdek), salt okunur, SMB/NFS'in reddettiği yerler: merkezi kayıt |
| **Windows** | NTFS/ReFS akışı `:gezik.meta` (tek akış, UTF-8 metin, §4.2) | aynı akış | aynı akış | FAT/exFAT, akışsız ağ paylaşımı, salt okunur birim, salt okunur dosya, yazmayı reddeden bulut yer tutucusu: merkezi kayıt |

- **Okuma sırası:** dosyanın kendi kaydı; birim merkezi kayıtlıysa (§4.4) ya da merkezi kayıtta bu yol varsa o. İkisi birden varsa merkezi kayıt kazanır (Gezik'in son yazdığıdır; dosyaya yazılamadığı için oraya gitmişti).
- **Yazma:** `[tags] write-to-files = true` (varsayılan) iken dosyanın kendi kaydına; yazma `ENOTSUP`/`EROFS`/`EACCES`/`ERROR_NOT_SUPPORTED`/`ERROR_ACCESS_DENIED`/`ERROR_WRITE_PROTECT` ile reddedilirse merkezi kayda, ve o oturum için o birim "merkezi kayıtlı" diye işaretlenir (her dosyada yeniden denenmez). `write-to-files = false` iken her şey merkezi kayda (dosyalara hiç dokunmak istemeyen kullanıcı: yedek ya da eşitleme gürültüsü; karar 9).
- **Windows'ta Explorer'ın kaydı:** `System.Keywords` ve `System.Rating` (dosyanın içinde, özellik işleyicisiyle) **yalnız okunur**, Etiketler ve Puan sütununda gri gösterilir, Gezik'in kendi değeri varsa o önce gelir (§8.3).

### 3.3 Erişim yolu (`gezik-ops::meta`)

```rust
pub fn read(path: &Path) -> io::Result<Meta>                 // own record + central store
pub fn write(path: &Path, meta: &Meta) -> io::Result<Where>  // Where::File | Where::Store
pub fn carry(from: &Path, to: &Path)                         // after a copy, when the target cannot hold the source's record
pub fn follow(outcome: &Outcome)                             // central store follows Gezik's moves
```

- Platform kodu `gezik-platform/src/meta/{mod, windows, unix}.rs` (yeni): ham baytları okur/yazar (`read_raw(path) -> Raw { tags, rating, comment, finder_info }`), birimin desteğini söyler (`support(path) -> Support { Native, AppleDouble, None }`). Kodlama/çözme saf ve `gezik-core::meta`'da (§4).
- `gezik-ops` iş motoru içinden çağrılır (yazma işleri, kopya, taşıma); UI'dan okuma `gezik/src/meta.rs`'in arka plan okuyucusundan (§3.4). Ana iş parçacığında asla dosya meta verisi okunmaz.

### 3.4 Tembel okuma (her parça)

`gezik/src/meta.rs` (yeni), `crate::media::Media`'nın (simge ve küçük resimler) deseniyle:

1. **İhtiyaç:** bir görünümün meta veriye ihtiyacı yalnız şunlardan biri varken vardır: Etiketler/Puan/Yorum sütunu açık (12c, 12d), ad yanında etiket noktaları açık (12b, §5.4), bir vurgulama kuralının etiket/puan/yorum koşulu var (12f), süzgeçte `tag:`/`rating:` sözcüğü var, sıralama ya da gruplama etiket/puana göre (12c, 12d). Hiçbiri yoksa okuyucu kurulmaz (`Option` `None`), hiçbir şey okunmaz.
2. **Görünen satırlar:** Slint bir satır kurduğunda (`model.rs`, satırlar zaten yalnız ekrandakiler için kurulur) eksik meta sıraya girer; tek arka plan iş parçacığı (düşük öncelik, 8a'nın `priority` kuralı) 64'lük kümeler hâlinde okur, sonucu ana iş parçacığına bugünkü medya yolundan (zaten olan uyanma) gönderir, yalnız o satırlar yeniden çizilir. Kaydırma eski istekleri iptal eder (sıra yeniden kurulur).
3. **Bütün klasör:** sıralama, gruplama ya da süzgeç etiket/puana göreyse bütün listenin meta verisi gerekir: aynı iş parçacığı listeyi baştan sona okur, durum satırında `Reading tags… 4,000 of 12,000` (8b'nin klasör boyutu ilerlemesi gibi). Okuma bitene kadar sıra değişmez (satırlar zıplamaz); bitince bir kez yeniden sıralanır. Okunmamışlar sıralamada sona gider (8b'nin `SIZE_PENDING` kuralı).
4. **Önbellek:** görünüm başına `HashMap<Box<str>, Meta>` (yalnız boş olmayanlar) + okunmuş satırların bit kümesi. Klasör değişince düşer; izleyici olayı (dosya değişti) o adı yeniden okutur; Gezik'in kendi yazdığı meta önbelleğe doğrudan yazılır (yeniden okuma yok). F5 hepsini düşürür.
5. **Bulut yer tutucuları:** akış/xattr okumak dosyayı indirmez (yalnız meta veri); Windows'ta `System.Keywords` okuması işleyiciyi çalıştırıp indirebileceği için çevrimiçi yalnız dosyalarda yapılmaz (`Entry` bulut bitleri, 9b5).
6. **Arama sonuçları ve düz görünüm** aynı okuyucuyu kullanır (satır yolu `dir.join(name)` yerine sonucun kendi yolu).

### 3.5 Geri alma ve iş motoru

- Etiket/puan/yorum yazmak bir iş motoru işidir: `TaskKind::Tags` (`gezik-ops/src/tasks/tags.rs`, yeni). `plan` hedef dosyaları ve her birinin şimdiki meta verisini okur; `run` yazar, her dosya için `Outcome::MetaChanged { path, before: Meta, after: Meta }` döner.
- **Geri alma:** `inverse.rs` `MetaChanged`'i `before`'u yazan bir `TagsTask`'a çevirir; dosyanın meta verisi hâlâ `after` değilse (arada Finder ya da başka biri değiştirdi) o dosya atlanır ve raporda sayılır (`3 files were changed since; their tags were left alone`; 9'un `AttributesChanged` kuralı).
- Tek dosyada tek etiket değişikliği de iş motorundan geçer (kısayol): iş paneli ancak 300 ms'yi aşan işlerde görünür (bugünkü kural), tek dosyalık iş görünmez ama Ctrl+Z ile geri alınır.
- **Windows'ta değiştirme zamanı korunur:** NTFS'te akışa yazmak dosyanın `LastWriteTime`'ını değiştirir; yazmadan önce okunur, yazdıktan sonra `SetFileTime` ile geri konur (`FILE_WRITE_ATTRIBUTES`). xattr (Linux/macOS) yalnız `ctime`'ı değiştirir, `mtime` aynı kalır. Böylece "Modified" sütunu, sıralama ve yedek araçlarının tarih karşılaştırması etiketle bozulmaz (karar 8).

### 3.6 Arayüz: yeni katman yok

11'in araç katmanları Slint'te katman başına 120–190 KB tuttu. 12 yeni katman kurmaz:

- **Etiket seçici** = var olan `widgets/picker.slint` (sekme seçici, palet, hızlı aç), `PickRow`'a iki alan: `dot: color` (saydamsa nokta yok) ve `check: int` (0 yok, 1 bazısında, 2 hepsinde). §5.1.
- **Yorum düzenleme** = var olan soru penceresi (`dialog.rs`) tek satırlık metin alanıyla.
- **Sütunlar** = `file-view.slint`'in var olan başlık hücresi ve `FileRow`'a metin alanları.
- **Kenar çubuğu bölümü** = `sidebar.rs`'in var olan bölüm ve satır yapısı, `SidebarRow`'a `dot: color`.
- **Vurgulama** = `FileRow`'a `tint: color` (saydam = yok) ve `bold: bool`; liste, ızgara ve sütun görünümü aynı alanları okur.
- **Ortak tablo (`tool-table.slint`)** gerekmez: etiket yönetimi ayar dosyası, kenar çubuğu menüsü ve seçiciyle yapılır (§7.3).

### 3.7 Hafiflik kuralı (her parça)

1. Etiket kullanılmadıkça: ayar anahtarları ayar yüklenirken okunur (dosya erişimi yok); okuyucu, önbellek, merkezi kayıt kurulmaz; kenar çubuğunun etiket bölümü yalnız ayardaki listeyi gösterir (G/Ç yok).
2. Merkezi kayıt dosyası ilk meta veri ihtiyacında, dosya varsa bir kez okunur; yoksa hiç oluşturulmaz (ilk merkezi yazmaya kadar).
3. Kopya yolu yalnız hedef birim kendi kaydını tutamıyorsa (Windows NTFS → FAT; Linux xattr'sız hedef) kaynağın meta verisini okur (dosya başına bir deneme). Aynı tür birimler arası kopyada ek okuma yok (Windows'ta `CopyFileExW` akışı kendisi taşır, macOS `clonefile`/`fcopyfile` xattr'ı taşır, Linux xattr kopyası §4.3).
4. İzleme yok: ek izleyici kurulmaz; var olan klasör izleyicisinin olayları önbelleği tazeler.

## 4. Saklama ayrıntıları (12a)

### 4.1 macOS: Finder'ın kaydı

- **`_kMDItemUserTags`:** ikili plist (`bplist00`), kökte dizgi dizisi; her dizgi `Ad` ya da `Ad\nN` (N = renk numarası 0–7). Okuyucu ASCII (`0x5_`) ve UTF-16BE (`0x6_`) dizgileri, 1/2/4/8 baytlık ofset tablolarını, tek düzeyli diziyi kabul eder; başka biçimde kayıt "okunamadı" sayılır ve **yazılmaz** (üzerine yazılıp bozulmaz, satırda `!` ipucu). Yazıcı ASCII olmayan adları UTF-16BE yazar (Finder gibi). Kod: `gezik-core/src/meta/bplist.rs` (yeni, saf, ≈ 250 satır; `plist` sandığı eklenmez, karar 5).
- **`com.apple.FinderInfo`:** 32 bayt; 9. baytın 1–3. bitleri eski "etiket rengi". Finder etiket verince son renkli etiketin rengini buraya da yazar; Gezik de öyle yapar, öbür baytlara dokunmadan (kayıt yoksa 32 sıfır baytla oluşturur, renk yoksa ve öbür baytlar sıfırsa xattr'ı siler). Okurken `_kMDItemUserTags` yoksa ve FinderInfo renk taşıyorsa o renk, rengin varsayılan adıyla etiket sayılır (Finder'ın davranışı).
- **Puan:** `kMDItemStarRating` (plist tamsayı 0–5). Finder göstermez, Spotlight dizinler (`mdfind 'kMDItemStarRating >= 4'`). **Yorum:** `kMDItemFinderComment` (plist dizgi). Finder'ın Get Info'su yorumu `.DS_Store`'dan okuduğu için Gezik'in yazdığı yorum Get Info'da görünmeyebilir; Spotlight'ta görünür (§17, karar 11).
- Bağlantılar: `XATTR_NOFOLLOW` ile bağlantının kendisi okunur/yazılır.
- Kopya: `clonefile` xattr'ları taşır; tamponlu yolda (APFS dışı hedef, sürücüler arası) kopyadan sonra `fcopyfile(…, COPYFILE_XATTR)` (bugün yalnız `secure_unix.rs`'in yönetici yolunda var; `fs/unix.rs::copy_file`'a da eklenir). exFAT/FAT hedefte macOS `._ad` dosyası yazar: Finder ve Gezik orada da etiketi görür.

### 4.2 Windows: NTFS akışı

- Akış adı `gezik.meta` (`dosya:gezik.meta:$DATA`), en çok 16 KB, UTF-8 BOM'suz, LF, satır biçimi:
  ```
  gezik-meta 1
  tag 6 Red
  tag 0 Project X
  rating 4
  comment Ready for review; ask Ayşe about the cover.
  ```
  Bilinmeyen satırlar atlanır ve **korunur** (yazarken geri yazılır; sonraki sürümlerle ileri uyum). Sürüm satırı yoksa ya da sayı > 1 ise akış okunur ama yazılmaz (`!` ipucu).
- Klasörlerin de akışı olur (NTFS'te dizin adlı akış taşır). Bağlantı/bağlantı noktası: `FILE_FLAG_OPEN_REPARSE_POINT` ile kendisi.
- Okuma: `CreateFileW(path:gezik.meta, FILE_READ_DATA, paylaşım hepsi, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS)`; `ERROR_FILE_NOT_FOUND` = meta yok (hızlı yol). Yazma: geçici akışa değil doğrudan (`CREATE_ALWAYS`; akış 16 KB'tan küçük, yarım yazma riski kabul, karar 7), önce/sonra `LastWriteTime` (§3.5). Salt okunur öznitelikli dosya: öznitelik **değiştirilmez**, merkezi kayda gider.
- Birim desteği: `GetVolumeInformationW` → `FILE_NAMED_STREAMS`; birim başına önbellek (`DriveFacts`'in yanında).
- Kopya: `CopyFileExW` NTFS → NTFS akışları kendisi kopyalar; akışsız hedefe (FAT/exFAT) akışı sessizce bırakır (12a'nın ilk görevi bunu bir testle doğrular; hata dönerse `COPY_FILE_*` bayraklarıyla aynı davranış sağlanır, §17). O durumda `meta::carry` kaynağın akışını okur ve merkezi kayda yazar.
- Taşıma (aynı birim) `MoveFileExW` = yeniden adlandırma: akış dosyayla gider. Çöp (`$Recycle.Bin` aynı birimde yeniden adlandırma) ve geri yükleme akışı korur.

### 4.3 Linux: xattr

- `user.xdg.tags` = `Ad1,Ad2` (renk yok: renk Gezik'in listesinden gelir, listede yoksa gri; karar 4), `user.baloo.rating` = `"0"`–`"10"`, `user.xdg.comment` = UTF-8 metin. Dolphin'in yazdığı tek sayılı puan (ör. `"7"`) yukarı yuvarlanır (4 yıldız); Gezik yalnız çift sayı yazar.
- Çağrılar `libc::{getxattr, setxattr, removexattr}` (bağlantıda `l*` biçimleri: Linux bağlantılarda `user.*`'a izin vermez → merkezi kayıt). `xattr` sandığı ağaçta (tar ile) ama yalnız `libc` yeter, eklenmez.
- **Kopya:** `copy_file_range` ve `FICLONE` xattr taşımaz. `fs/unix.rs::copy_file` kopyadan sonra kaynağın **bütün `user.*` xattr'larını** (`flistxattr`/`fgetxattr`/`fsetxattr`, açık tutamaklarla; toplam ≤ 64 KB) hedefe yazar: GNOME Files ve `cp --preserve=xattr` gibi (karar 10). Hedef reddederse (`ENOTSUP`) ve kaynakta Gezik'in üç anahtarından biri varsa `meta::carry` merkezi kayda yazar. Yönetici yolu (`secure_unix.rs`) da aynı işlevi çağırır.
- Taşıma (aynı dosya sistemi) `rename`: xattr gider. `.Trash-<uid>`'e taşıma aynı dosya sisteminde yeniden adlandırma.

### 4.4 Merkezi kayıt

- **Dosya:** yapılandırma klasöründe `tags-store.tsv` (state.toml'un yanında; taşınabilir kurulumda onunla gider). UTF-8, LF, satır başına bir dosya:
  ```
  # gezik tags store 1
  <volume>\t<path in volume>\t<size>\t<mtime ns>\t<file id or ->\t<meta>
  ```
  `<meta>`, §4.2'nin satırlarının `\x1f` ile birleşmiş hâli (sekme ve satır sonu kaçışlı). Yazma geçici dosya + atomik yeniden adlandırma (bugünkü `state_store` yolu); her yazma işinin sonunda bir kez (zamanlayıcı yok).
- **Birim anahtarı:** Windows birim seri numarası (`GetVolumeInformationW`, onaltılık) ya da ağda `\\sunucu\paylaşım`; Linux dosya sistemi UUID'si (`/dev/disk/by-uuid` eşlemesinden; yoksa bağlama kaynağı metni); macOS `ATTR_VOL_UUID` (yoksa bağlama kaynağı). Yol birim köküne göreli, `/` ile. Böylece USB bellek başka sürücü harfiyle ya da başka bağlama noktasıyla takılınca etiketler yine bulunur (karar 3).
- **Kimlik:** anahtar **birim + göreli yol**. Boyut, `mtime` ve (sistem kararlı veriyorsa: NTFS dosya numarası, ext4/btrfs inode; FAT'te verilmez) dosya kimliği yalnız **dışarıda taşınan dosyayı yeniden bulmak** için saklanır. Dosyanın içeriği değişince (yol aynı) etiket kalır; boyut ve `mtime` sessizce güncellenir.
- **Gezik'in kendi işleri:** `meta::follow(outcome)` iş motorunun sonuç işleyicisinden çağrılır: `Moved { from, to }` (yeniden adlandırma ve taşıma; klasörde `from/` önekli bütün kayıtlar), `Trashed`/`Restored` (çöp yolu ile asıl yol arasında), `Deleted` (kayıt düşer), `Created { from: Some }` (sürücüler arası taşıma). Geri alma aynı görevlerle koştuğu için kendiliğinden izlenir. Kopya `carry` ile yeni kayıt açar (kaynak kayıt kalır).
- **Dışarıda yapılan değişiklikler** (Explorer, Finder, `mv`, başka bilgisayar):
  - Yeniden adlandırma/taşıma **aynı birimde**: Gezik o birimde bir klasör listelediğinde, listede kaydı olmayan bir dosya ile kaydı olup yolu artık bulunmayan bir kayıt **aynı ad, boyut ve `mtime`** (FAT'te 2 sn toleransla) ya da aynı dosya kimliğini taşıyorsa kayıt yeni yola bağlanır. Eşleşme yalnız meta ihtiyacı olan görünümde ve yalnız yolu kayıp kayıtlara (≤ 64 aday, yolu bulunmayanlar ilk erişimde işaretlenir) karşı yapılır; tam tarama yok.
  - Ad **ve** içerik birlikte değişirse, ya da başka birime taşınırsa: etiket kaybolur; kayıt "sahipsiz" kalır.
  - Eşi yeniden kullanılan yol (dosya silinip aynı adla yenisi konursa): boyut/`mtime`/kimlik üçü de farklıysa etiket **gösterilmez** ve kayıt sahipsiz sayılır (yanlış dosyaya etiket yapışmasın; karar 3).
  - Sahipsiz kayıtlar kendiliğinden silinmez (USB takılı değilken her şey "yok" görünür). Palette `Clean Up Tag Store…` (12e): takılı birimlerde yolu olmayan kayıtları sayar ve sorarak siler; takılı olmayan birimlerin kayıtları dokunulmaz, sayısı söylenir.
- **Boyut:** 20.000 kayıt ≈ 2 MB dosya; okuma ≤ 30 ms; bellekte `HashMap<(VolumeIdx, Box<str>), Record>` ≈ +3 MB. Üst sınır 200.000 kayıt; aşılınca yeni merkezi yazma reddedilir (`The tag store is full (200,000 files); clean it up or tag files on NTFS/APFS/ext4`).

### 4.5 Ayarlar (12a'da doğar)

```toml
[tags]
# Your tags, in order: the first seven get the keys Alt+Shift+1…7 (macOS ⌃1…⌃7).
# color: red, orange, yellow, green, blue, purple, gray or none.
list = [
  { name = "Red", color = "red" },
  { name = "Orange", color = "orange" },
  { name = "Yellow", color = "yellow" },
  { name = "Green", color = "green" },
  { name = "Blue", color = "blue" },
  { name = "Purple", color = "purple" },
  { name = "Gray", color = "gray" },
]
write-to-files = true   # false: keep every tag in Gezik's own store, never in the files
dots = "auto"           # tag dots beside names: auto (macOS: on; else on once you tag a file), true, false
```

- Hatalı öğe bugünkü uyarı kalıbıyla atlanır (`tags.list[3]: name: a tag name cannot hold a comma`). Aynı ad iki kez: ikincisi atlanır. En çok 60 etiket listede (menü kimlikleri, §12.3).
- Listede olmayan ama dosyada bulunan etiketler "listede olmayan etiket"tir: gösterilir, süzülür, aranır; renkleri dosyadakinden (macOS, Windows) ya da gri (Linux).
- Seçicideki `New Tag…` ve kenar çubuğu menüsü listeyi `settings_writer` ile yazar (`[[tab-sets]]` gibi; yorumlar korunur).

## 5. Toplu etiketleme ve etiket noktaları (12b)

### 5.1 Etiket seçici (`tags`, `Tags…`)

```
┌ Tags for 12 items                                         ┐
│ [Find or create a tag…                                 ]  │
│ ● Red              ✓        Alt+Shift+1                    │
│ ● Orange           –        Alt+Shift+2   (some of them)   │
│ ● Green                     Alt+Shift+4                    │
│ ○ Project X        ✓        (not in your list)             │
│ ＋ Create "Proj"                                           │
└───────────────────────────────────────────────────────────┘
```

- `picker.slint`'in kutusu (§3.6). Satırlar: listedeki etiketler (sıra), sonra seçimin dosyalarında bulunan listede olmayan etiketler, sonra yazılan metin hiçbir ada uymuyorsa `Create "…"`. Yazınca süzülür (`pattern::fold_text`).
- Durum işareti seçimin hepsine göre: `✓` hepsinde, `–` bazısında, boş hiçbirinde. Enter/tık: `✓` → hepsinden çıkar; `–` ya da boş → hepsine ekler. Kutu açık kalır (birkaç etiket art arda); Esc kapatır.
- **Değişiklikler kutu kapanınca tek iş olarak** uygulanır (tek `TagsTask`, tek geri alma: `Undo Tags`); kutu açıkken satırların durumu anında değişir (bellekte). Seçimin meta verisi kutu açılırken okunur (iş parçacığında; 1.000'den çok öğede `Reading tags…` satırı, okuma bitene kadar `–` gösterilmez, satırlar yalnız ekleme yapar).
- `Create "…"`: ad listeye eklenir (renk yok; renk kenar çubuğu menüsünden), seçime de eklenir.
- Seçim yoksa odaktaki öğe. Arama sonuçlarında ve düz görünümde de çalışır. This PC, çöp görünümü: eylem yok (menüde gösterilmez).
- 12d'de kutunun altına puan satırı (`★★★★☆  Rating`) ve `Comment…` satırı eklenir (§8.1).

### 5.2 Eylemler ve tuşlar

| Eylem | Windows / Linux | macOS | Not |
|---|---|---|---|
| `tags` (`Tags…`) | Ctrl+Shift+L | ⌃⌘L | seçici |
| `tag-1` … `tag-7` | Alt+Shift+1 … Alt+Shift+7 | ⌃1 … ⌃7 | listenin n. etiketini seçimde aç/kapa |
| `clear-tags` | Alt+Shift+0 | ⌃0 | seçimden bütün etiketleri kaldır (puan/yorum kalır) |

- **Aç/kapa kuralı** (`tag-n`): seçimin hepsinde varsa hepsinden çıkar, yoksa hepsine ekler (Finder'ın ⌃n'i gibi). Durum satırı `Red added to 12 items` / `Red removed from 12 items`; listede n. etiket yoksa `No tag 6 in your list ([tags] list)`.
- **Boş tuşlar (denetlendi):** Windows/Linux'ta Ctrl+1…9 sekmeler, Alt+1…9 iğneler, Ctrl+Shift+0…3 seçim/görünüm, Ctrl+Alt+rakam AltGr ile yazı (bugünkü test: `"AltGr+1 types"`) olduğu için Alt+Shift+rakam seçildi (bugün boş). macOS'ta ⌘rakam sekmeler, ⌥⌘rakam iğneler; ⌃rakam boş ve Finder'ın kendi etiket tuşlarıdır. Ctrl+Shift+L / ⌃⌘L boş (⌃⌘T Finder'da "Add to Sidebar" olduğu için alınmadı). Çakışmasızlık `fixed_owner` ve macOS menü sabitleriyle testte.
- Alt+Shift Windows'ta varsayılan klavye dili değiştirme tuşudur; Windows bu tuşu yalnız arada başka tuş basılmadan bırakılınca uygular, bu yüzden Alt+Shift+1 dil değiştirmez. Bu, Windows ekran testinde doğrulanır (klavye düzeni ayarına dokunulmadan); değiştirirse varsayılanlar kaldırılır ve tuşlar kullanıcıya bırakılır (§17, karar 13).

### 5.3 Menüler

- **Sağ tık** (satır, Windows'ta Gezik'in üstteki öğeleriyle): `Tags ▸` alt menüsü: listedeki etiketler (işaretli: hepsinde; Windows/Linux yerel menüde kısmi işaret yok, `–` metin önekiyle), ayraç, `Tags…`, `Clear Tags`; 12d'de `Rating ▸ (None, ★ … ★★★★★)` ve `Comment…`. Renk noktası yerel menüde yok (karar 14).
- **macOS menü çubuğu:** `File ▸ Tags ▸` (Finder'ın yeri), aynı öğeler, tuşlarıyla.
- **Palet:** `Tags…`, `Add Tag: Red` … (listedeki her etiket için `Toggle Tag: <ad>` öğesi; `Sync:` öğeleri gibi açılışta kurulur, kapanınca düşer), `Clear Tags`.

### 5.4 Etiket noktaları

- Liste görünümünde adın sağında, ızgarada adın altında, sütun görünümünde adın sağında en çok 3 renkli nokta (Finder gibi üst üste binen, 8 px) ve fazlası için `+2`. Rengi olmayan etiket nokta değil, içi boş halka. `FileRow`'a `tags: [color]` (en çok 3) ve `more: int`.
- **Ne zaman okunur:** `[tags] dots`. `auto`: macOS'ta açık (Finder kullanıcısı bekler); Windows/Linux'ta kullanıcı ilk kez bir etiket yazdığında açılır (`state.toml` `[tags] used = true`). Böylece hiç etiket kullanmayan Windows/Linux kullanıcısı hiçbir meta okuması ödemez (karar 15). `View ▸ Show ▸ Tag Dots` aç/kapa (ayara yazar).
- Okuma §3.4'ün görünen satırlar yolu. Seçili satırda noktalar seçim renginin üstünde 1 px kenarlıkla.

### 5.5 Ekran okuyucu

Satırın erişilebilir adına etiketler eklenir (`report.pdf, tags Red, Project X`). Seçici satırları `Red, on all 12 items`.

## 6. Sütunlar, süzme, sıralama, gruplama (12c)

### 6.1 Sütun

- `ColumnKey::Tags` (`Tags`): noktalar + adlar virgülle (`● Red, ● Project X`), kısa genişlikte `…`. Varsayılan kapalı; başlık menüsünden açılır (bugünkü sütun aç/kapa yolu, `views.toml`'a ve `[[view-rules]] columns`'a yazılır).
- `ColumnKey::RESULTS`'a da eklenir (arama sonuçları).

### 6.2 Sıralama ve gruplama

- `SortKey::Tags`: dosyanın etiketlerinden **listede en önde olanın** sırası (listede olmayanlar listedekilerden sonra, adlarıyla), sonra ad. Etiketsizler artan sırada sonda (8b'nin boş değer kuralı).
- `GroupBy::Tag` (10d): dosya **ilk etiketinin** (listede en önde olan) grubunda görünür; grup başlığı `● Red`; etiketsizler `No Tags` grubunda, sonda. Bir dosya birden çok grupta görünmez (karar 17).
- İkisi de bütün klasörün meta verisini ister (§3.4 madde 3). `View ▸ Sort By ▸ Tags`, `View ▸ Group By ▸ Tag`; `[[view-rules]]`'da `sort = "tags"`, `group = "tag"`.

### 6.3 Süzgeç

- Süzgeç çubuğu (6a/7'nin `filter-bar`) yeni sözcükleri tanır: `tag:Red` (bu etiket), `tag:"Project X"`, `tag:none` (etiketsiz), `tag:any`; 12d'de `rating:4` (en az 4), `rating:0` (puansız). Birden çok `tag:` hepsi (VE); ad deseniyle birlikte yazılabilir (`*.jpg tag:Red`). Windows'ta `:` dosya adında olamaz; Linux/macOS'ta `tag:` ile başlayan bir ad aramak için tırnak: `"tag:x"`.
- Kenar çubuğu etiketinin menüsü `Filter This Folder` süzgece `tag:<ad>` yazar (12e).
- Ayrıştırma `gezik-core/src/meta_filter.rs` (yeni, saf); süzgeç 7'nin bugünkü yolundan geçer, meta okunmamış satırlar okuma bitene kadar **gösterilir** ve soluk çizilir (`Reading tags…`), sonra süzülür (yanlış "yok" göstermemek için; karar 18).

### 6.4 Windows Explorer'ın etiketleri (salt okunur)

12d'de (§8.3); 12c'nin sütunu onları yer bırakarak çizer.

## 7. Kenar çubuğu ve arama (12e)

### 7.1 Kenar çubuğunda Etiketler

- Yeni bölüm `Tags` (Places'ten sonra, Drives'tan önce; 10c'nin bölüm yapısı): listedeki etiketler, renkli noktayla. Bölüm başlığı kapanır; `[sidebar] tags = true` (gizlemek için false). Satırlar yalnız ayardaki listeden kurulur: **G/Ç yok**.
- **Tık:** etiketin araması (`Location::Search`, tarif `tags = ["Red"]`):
  - **macOS:** bütün diskte Spotlight'la: `mdfind 'kMDItemUserTags == "Red"cd'` (`ChildProcess`, Finder'ın kenar çubuğuyla aynı sonuç), sonuçlar 8a'nın sonuç listesine akar; Spotlight kapalı birimler (dizinlenmemiş USB) sonuçta yok, üst satırda not (`Spotlight does not cover E:; search there to include it`).
  - **Windows/Linux:** gösterilen klasörde, alt klasörlerle (8a'nın tarayıcısı, etiket ölçütüyle). Ayrıca merkezi kayıtta bu etiketi taşıyan dosyalar (anında, G/Ç'siz) sonuçlara eklenir. Üst satırda `Searching in Projects; search all drives instead` bağlantısı (`Scope::AllDrives`).
- **Sürükle-bırak:** dosyaları bir etiket satırına bırakmak onlara o etiketi ekler (Finder gibi; kopya/taşıma değil; tek iş, geri alınır). Bırakma ipucu `Tag 3 items Red`.
- **Satır menüsü:** `Search This Folder`, `Search Everywhere` (Windows/Linux: bütün sürücüler; macOS: Spotlight, tık ile aynı), `Filter This Folder` (§6.3), ayraç, `Rename…`, `Color ▸` (8 renk), `Move Up`/`Move Down`, `Remove from List`, ayraç, `New Tag…`. Liste değişiklikleri `settings.toml`'a yazılır; dosyalara dokunmaz (§7.3).

### 7.2 Arama

- `SearchSpec`'e alanlar: `tags: Vec<String>` (hepsi; boş = ölçüt yok; `"none"` özel değeri yok, onun için `untagged: bool`), `min_rating: u8` (12d; 0 = ölçüt yok), `comment: String` (12d; içerir, harf duyarsız). `[[searches]]` ve `state.toml`'un oturumu bunları taşır (`tags = ["Red"]`, `min-rating = 4`, `comment = "review"`).
- **Arama çubuğu:** ek seçenekler satırında `Tags` alanı (virgülle, listedeki adlara tamamlama), `Rating ≥` seçimi (Any, ★…★★★★★), `Comment` alanı.
- **Sıra:** `Query::compile` bu ölçütleri en sona koyar ("en ucuzdan başla" kuralı): ad, tür, boyut, tarih elendikten sonra kalan aday için meta okunur, içerik aramasından önce. Ölçüt yoksa hiç meta okunmaz (sayaçla test).
- **Everything (Windows):** Everything etiket bilmez; etiket ölçütlü aramada Everything adayları verir, Gezik adayların metasını okuyup süzer.
- **macOS:** yalnız etiket/puan/yorum ölçütü olan bütün disk aramaları Spotlight'tan (`kMDItemUserTags`, `kMDItemStarRating`, `kMDItemFinderComment`); klasör kapsamlı aramalar tarayıcıdan (Spotlight'ın dizinlemediği yerlerde de doğru olsun diye).

### 7.3 Etiket listesini yönetme

- `Rename…`: listedeki ad değişir, merkezi kayıttaki eski adlar yenisine çevrilir (anında). **Dosyalardaki eski ad değişmez**: o dosyalar artık "listede olmayan etiket" taşır. Soru penceresi bunu söyler ve bir düğme önerir: `Search and Retag…` → eski etiketin araması açılır, sonuçlar seçilir ve seçici eski etiketi kaldırıp yenisini ekleyecek şekilde hazır gelir (iki adım, bir iş, geri alınır).
- `Remove from List`: yalnız listeden; dosyalar dokunulmaz.
- `Color ▸`: listedeki renk; macOS/Windows'ta dosyalardaki renk numarası sonraki yazmada güncellenir (göstermede listedeki renk kazanır).

### 7.4 Merkezi kayıt bakımı

`Clean Up Tag Store…` (palet; §4.4): yolu bulunmayan kayıtları takılı birimlerde sayar, soru penceresi `142 entries point to files that are gone. Remove them?`; takılı olmayan birimler için `318 entries on drives that are not connected were left alone`. Geri alınmaz (kayıt zaten sahipsiz), işten önce kaydın kopyası `tags-store.tsv.bak` yazılır.

## 8. Puan ve yorum (12d)

### 8.1 Verme

- Etiket seçicinin altında puan satırı: `☆☆☆☆☆` beş tıklanabilir yıldız (seçimin hepsinde aynıysa o, değilse soluk); klavye: kutuda Alt+0…5 (macOS ⌥0…5). Sağ tık `Rating ▸ None, ★, ★★, ★★★, ★★★★, ★★★★★`.
- `edit-comment` (`Comment…`, tuş yok): soru penceresinde tek satırlık alan (≤ 1.000 karakter); birden çok öğede alan, hepsinde aynıysa o metinle, değilse boş ve `(different on 3 items)` ipucuyla dolar; boş bırakıp kaydetmek yorumu siler. Tek iş, geri alınır.

### 8.2 Sütunlar, sıralama, gruplama, süzgeç

- `ColumnKey::Rating` (`Rating`): `★★★★☆` metni (yazı tipindeki yıldız; çizim yok), sağa hizalı değil, sabit genişlik. `ColumnKey::Comment` (`Comment`): ilk satır, `…` ile.
- `SortKey::Rating` (varsayılan azalan: yüksek puan üstte), `SortKey::Comment` yok (YAGNI). `GroupBy::Rating`: `★★★★★` … `★`, `Not Rated`.
- Süzgeç `rating:N` (§6.3).

### 8.3 Windows: Explorer'ın etiketi ve puanı (salt okunur)

- Yalnız Etiketler ya da Puan sütunu açıkken, yalnız görünen satırlarda, yalnız Windows'un bu iki özelliği yazan işleyicisi olan uzantılarda (sabit liste: `jpg jpeg tif tiff heic mp3 m4a wma mp4 wmv docx xlsx pptx`; her dosyada denemek işleyiciyi yüklemek demek olduğu için deneme yok) ve yalnız yerel (çevrimiçi yalnız değil) dosyalarda okunur: `SHGetPropertyStoreFromParsingName(path, GPS_DEFAULT)` ile `System.Keywords` ve `System.Rating` (1–99 → yıldız: 1, 25, 50, 75, 99 eşikleri Explorer'ınki).
- Gezik'in kendi etiketlerinden sonra gri, renksiz gösterilir; ipucu `From the file (Windows Explorer's Tags); read only`. Gezik'in puanı yoksa Explorer'ın puanı gri yıldızlarla.
- **Süzgeç, sıralama, gruplama, arama, vurgulama bunları kullanmaz** (karar 19). Seçici bunları göstermez. Info penceresinde (9) `Tags (Windows)` satırı.
- COM: okuyucu iş parçacığında `CoInitializeEx(MTA)` bir kez; işleyici takılırsa (bozuk dosya) 2 sn sonra o dosya atlanır (iş parçacığı başka dosyaya geçer; `IPropertyStore` çağrısı ayrı iş parçacığında, zaman aşımlı).

## 9. Vurgulama kuralları (12f)

### 9.1 Ayar

```toml
# The first rule that matches a row colors it (Total Commander's color rules).
[[highlight-rules]]
name = "*.tmp *.bak ~*"     # the filter bar's pattern language; `| x` leaves out
color = "gray"

[[highlight-rules]]
tag = "Red"
bold = true

[[highlight-rules]]
kind = "folder"
newer-than = "1d"           # modified within a day (search's text forms: 7d, 2w, 3m, 1y)
color = "#2e7d32"
dark-color = "#81c784"      # optional; the dark theme's color

[[highlight-rules]]
min-size = "1 GB"
rating = 0                  # 0: not rated; n: at least n stars
color = "orange"
```

- **Koşullar** (hepsi VE; olmayan koşul her şeye uyar): `name` (6a deseni, 11d'nin `file_filter` sözdiziminin `|` dışlamasıyla), `kind` (`folder`, `file`, ya da aramanın tür adları: `pictures`, `videos`, `audio`, `documents`, `archives`, `code`…), `newer-than`/`older-than` (değiştirme zamanı; aramanın süre metinleri), `min-size`/`max-size` (aramanın boyut metinleri), `hidden` (true/false), `tag` (bir etiket adı ya da `"any"`/`"none"`), `rating` (0 = puansız, n = en az n), `comment` (`"any"` ya da içerdiği metin).
- **Görünüş:** `color` (8 etiket rengi adı ya da `#rrggbb`), `dark-color` (koyu temada; yoksa adlı renklerin koyu karşılığı, onaltılıkta aynı renk), `bold`. En az biri gerekli.
- İlk uyan kural rengi **ve** kalınlığı birlikte verir (TC). En çok 64 kural. Hatalı kural uyarıyla atlanır (`highlight-rules[2]: min-size: expected a size like 100 MB`).
- Varsayılan: kural yok (`templates/settings.toml`'da yorumlu örnekler).

### 9.2 Derleme ve maliyet

- `gezik-core/src/highlight.rs` (yeni, saf), 10f'nin `view_rules.rs` deseniyle: kurallar ayar yüklenince bir kez derlenir (`Compiled { name: Option<Matcher>, kind_bits: u32, min_mtime, max_mtime, min_size, max_size, hidden: Option<bool>, meta: Option<MetaCond>, style }`); test sayaçları (`COMPILED`, `TRIED`) 10f'deki gibi: klasör açılırken derleme yok, satır başına deneme sayısı ilk uyana kadar.
- Zaman koşulları derlemede değil **satır kurulurken** `now`'a göre: `now` görünüm kurulurken bir kez alınır (satır başına saat çağrısı yok).
- Değerlendirme satır kurulurken (`model.rs`, yalnız ekrandaki satırlar): `Entry` alanları (ad, boyut, zaman, bayraklar, tür) ile; meta koşullu kural ancak satırın metası okunmuşsa değerlendirilir, okunmamışsa o kural "henüz bilinmiyor" sayılır ve sonraki kurallara geçilmez (satır metası gelince yeniden çizilir; yanlış kuralın rengi bir an görünmesin). Hiçbir kuralda meta koşulu yoksa okuyucu bu yüzden kurulmaz (§3.4).
- Satır başına ≤ 1 µs (64 kural, meta koşulsuz; birim süre testi).

### 9.3 Çizim

- `FileRow.tint` adın (ve sütun metinlerinin) yazı rengi; `bold` adın kalınlığı. Seçili satırda renk, seçim arka planına karşı karşıtlığı ≥ 3:1 ise kalır, değilse seçimin yazı rengi kullanılır (`theme_rules::luminance`, WCAG oranı; renk her tema yüklemesinde kural başına bir kez hesaplanır).
- Kesilmiş (cut) satırın solukluğu ve gizli dosyanın solukluğu renkle birleşir (bugünkü opaklık).
- Liste, ızgara, sütun görünümü, arama sonuçları aynı alanı okur. Kenar çubuğu ağacına uygulanmaz (YAGNI).

## 10. Dosya işlemleri ve etiketler (12a; özet tablo)

| İşlem | Windows NTFS | macOS | Linux | Merkezi kayıtlı birim |
|---|---|---|---|---|
| Yeniden adlandırma, aynı birimde taşıma | akış gider | xattr gider | xattr gider | `follow(Moved)` |
| Kopya, aynı tür birim | `CopyFileExW` akışı taşır | `clonefile`/`fcopyfile` | Gezik `user.*` kopyası (§4.3) | `carry` yeni kayıt |
| Kopya/taşıma → FAT/exFAT | akış düşer → `carry` merkezi kayda | `._` dosyası (yerel) | xattr reddedilir → `carry` | — |
| FAT/exFAT → NTFS/ext4 | `carry` merkezi kayıttan akışa/xattr'a | yerel | `carry` | — |
| Çöpe atma, geri yükleme | akış kalır | xattr kalır | xattr kalır | `follow(Trashed/Restored)` |
| Kalıcı silme | — | — | — | `follow(Deleted)` kaydı düşürür |
| Geri alma | aynı görevler (izler) | aynı | aynı | aynı |
| Eşitleme (11e), süzgeçli kopya (11i), bölme | kopya yolu (yukarıdaki) | aynı | aynı | aynı; bölme parçaları etiket taşımaz |
| Sıkıştırma, açma, dönüştürme | taşımaz | taşımaz | taşımaz | taşımaz |

`carry` (merkezi kayıt ↔ dosya kaydı) yalnız iki uç farklı türdeyse çalışır; `RunCx::copy_file`'ın sonunda, kopya başarılıysa, tek yerden (kopya, sürücüler arası taşıma, eşitleme, süzgeçli kopya hepsi oradan geçer). Hata (kayıt yazılamadı) kopyayı başarısız yapmaz; raporda `Tags of 3 files could not be carried` notu.

## 11. Bulut, ağ, çıkarılabilir diskler

- **Bulut klasörleri:** OneDrive akışları eşitlemez (Windows'ta etiket yalnız o bilgisayarda kalır); iCloud Drive etiketleri eşitler (macOS); Dropbox macOS/Linux'ta xattr'ları eşitler, Windows'ta akışları eşitlemez. Gezik bunlara karışmaz; README'de tablo (§18).
- **Ağ paylaşımları:** SMB çoğu zaman akış/xattr taşır (Windows sunucuda NTFS, Samba'da `streams_xattr`); taşımazsa birim merkezi kayıtlı olur (anahtar `\\sunucu\paylaşım`). NFS genelde xattr reddeder.
- **Salt okunur birim** (CD, kilitli SD, salt okunur bağlama): merkezi kayıt; okuma her zaman dosyanın kendi kaydını da okur.

## 12. Ayarlar, eylemler, menüler

### 12.1 Ayarlar

```toml
[tags]
list = [ … ]              # see §4.5
write-to-files = true
dots = "auto"

[sidebar]
tags = true               # the Tags section

# [[highlight-rules]] — see §9.1
```

`state.toml` (Gezik yazar): `[tags] used` (ilk etiket yazıldı; `dots = "auto"` için). `views.toml` ve `[[view-rules]]`: yeni sütun (`tags`, `rating`, `comment`), sıralama (`tags`, `rating`) ve gruplama (`tag`, `rating`) değerleri. `[[searches]]`: `tags`, `untagged`, `min-rating`, `comment`.

Hepsi `templates/settings.toml`'a yorumlarıyla ve `settings_writer`'ın bilinen anahtarlarına girer.

### 12.2 Eylemler ve varsayılan tuşlar

| Eylem | Windows / Linux | macOS | Parça |
|---|---|---|---|
| `tags` | Ctrl+Shift+L | ⌃⌘L | 12b |
| `tag-1` … `tag-7` | Alt+Shift+1 … 7 | ⌃1 … ⌃7 | 12b |
| `clear-tags` | Alt+Shift+0 | ⌃0 | 12b |
| `edit-comment` | — | — | 12d |
| `clean-tag-store` | — | — | 12e |

- `Action` 113 → 122 (12b) → 123 (12d) → 124 (12e). Sayı yalnız `Action::ALL`'ın dizi türünde yazılır (PR #87); testler sayıyı tekrar etmez.
- Seçici içi tuşlar (Alt+0…5 puan) seçicinin kendi klavyesidir, kısayol tablosuna girmez.

### 12.3 Menü kimlikleri

12 için **2400–2599**: `TAGS_*` 2400–2409 (`Tags…`, `Clear Tags`, `Comment…`, `New Tag…`), listedeki etiketler 2410–2469 (en çok 60), puanlar 2470–2475, kenar çubuğu etiket menüsü 2480–2489, renkler 2490–2497, `View` öğeleri (noktalar, sıralama, gruplama) 2500–2509. `GEZIK_IDS_END = 4096` değişmez.

## 13. Kod yapısı

| Parça | Yer |
|---|---|
| Model, kodlayıcılar | `gezik-core/src/meta.rs` (yeni; `Meta`, `Tag`, `TagColor`, Windows akış metni, xdg listesi), `gezik-core/src/meta/bplist.rs` (yeni; ikili plist okuma/yazma), `gezik-core/src/meta_store.rs` (yeni; kayıt satırları, arama, önek yeniden yazma, yeniden bulma eşlemesi; saf) |
| Platform | `gezik-platform/src/meta/{mod, windows, unix}.rs` (yeni; ham okuma/yazma, birim desteği, birim anahtarı, `mtime` koruma), `gezik-platform/src/fs/unix.rs` (`fcopyfile` tamponlu yolda, Linux `user.*` kopyası), `gezik-platform/src/fs/secure_unix.rs` (Linux xattr kopyası) |
| Erişim, iş | `gezik-ops/src/meta.rs` (yeni; `read`, `write`, `carry`, `follow`, merkezi kayıt örneği), `gezik-ops/src/tasks/tags.rs` (yeni; `TagsTask`), `gezik-ops/src/{task, inverse, run, engine}.rs` (`TaskKind::Tags`, `Outcome::MetaChanged`, `follow` çağrısı, `copy_file` sonunda `carry`) |
| Ayarlar | `gezik-config/src/tags.rs` (yeni; `[tags]` ayrıştırma/yazma), `gezik-config/src/highlight_rules.rs` (yeni), `gezik-config/src/{settings, settings_writer, shortcuts}.rs`, `templates/settings.toml` |
| Okuyucu, UI | `gezik/src/meta.rs` (yeni; tembel okuyucu ve önbellek), `gezik/src/tags.rs` (yeni; seçici, eylemler, kenar çubuğu tıkları), `gezik/src/view/{model, listing, columns}.rs`, `gezik/src/{actions, context_menu, menu_bar, palette, sidebar, filter, search, dialog, info}.rs`, `ui/widgets/{picker, file-view, column-view, sidebar}.slint` |
| Sütun, sıralama, gruplama, süzgeç | `gezik-core/src/{view, sort, group}.rs`, `gezik-core/src/meta_filter.rs` (yeni) |
| Arama | `gezik-core/src/search.rs` (`SearchSpec`), `gezik-search/src/query.rs` (son aşama meta ölçütü), `gezik-platform/src/mac/spotlight.rs` (yeni; `mdfind` sorgu metni ve sonuç akışı) |
| Vurgulama | `gezik-core/src/highlight.rs` (yeni), `gezik/src/view/model.rs` |
| Explorer'ın kaydı | `gezik-platform/src/meta/windows.rs` (`IPropertyStore` okuması) |

## 14. Bütçe ve performans

| Parça | Exe (tahmin; sınır ≤ +262.144 B) | Boşta bellek | Kullanılırken |
|---|---|---|---|
| 12a | ≈ +90 KB (bplist ≈ 15, kayıt ≈ 20, platform ≈ 30, iş ≈ 25) | 0 | merkezi kayıt 20.000 kayıtta ≤ +3 MB, okuma ≤ 30 ms; kopya `master` ±%3 (aynı tür birimler), FAT hedefinde ≤ +%3 (10.000 küçük dosya, `ops.ps1`) |
| 12b | ≈ +110 KB (seçici satırları, noktalar üç görünümde, menüler) | 0 | 40 görünen satırın metası ≤ 10 ms arka planda; 10.000 dosyaya etiket Windows ≤ 3 sn, Linux/macOS ≤ 1 sn |
| 12c | ≈ +70 KB | 0 | 10.000 öğede tam okuma Windows ≤ 1 sn, Linux/macOS ≤ 300 ms; önbellek etiketli dosya başına ≤ 80 B |
| 12d | ≈ +60 KB | 0 | Explorer özelliği okuma görünen satır başına ≤ 5 ms (arka planda) |
| 12e | ≈ +90 KB | 0 | kenar çubuğu bölümü G/Ç'siz; Spotlight sonucu ≤ 1 sn; meta ölçütsüz arama `master` ±%2 |
| 12f | ≈ +50 KB | 0 | satır başına ≤ 1 µs (64 kural); derleme ayar yüklemesinde ≤ 1 ms |

Toplam ≈ +470 KB exe. Varsayılan ayarlarla, etiket kullanılmadan boşta **0** (her parça ±0,02 MB), açılış ≤ +1 ms, klasör açma `master` ±%2 (Windows/Linux'ta `dots = "auto"` kapalı başlar; macOS'ta noktalar açık olduğundan macOS'ta klasör açma ±%2 ölçütü ilk çizim içindir, meta arka planda gelir).

- **Ölçüm:** her parçanın ilk görevi `master`'ın sürüm derlemesini bayt olarak, sonu `measure.ps1`, `stress.ps1` ve `ops.ps1`'i yazar. Yeni betik `scripts/perf/tags.ps1`: 10.000 dosyalık klasörde görünen satır ve tam okuma süreleri, 10.000 dosyaya etiket, FAT hedefine kopyada `carry` maliyeti, 20.000 kayıtlı merkezi kayıt okuma süresi ve belleği, 64 kurallı vurgulama.
- **Zamanlayıcı ve yoklama yok:** okuyucu olayla uyanır, merkezi kayıt iş bitince yazılır.
- **Yazılım çizicisi:** satır başına en çok 3 nokta + bir sayı; yıldızlar yazı tipi glifi.

## 15. Parçalar (her biri bir plan ve bir PR)

| Sıra | Parça | İçerik | Bağımlılık |
|---|---|---|---|
| 1 | **12a** | Saklama çekirdeği: model, bplist, akış/xattr okuma-yazma, birim desteği, merkezi kayıt, kopyada taşıma (`carry`, Linux xattr, macOS `fcopyfile`), Gezik'in taşımalarında izleme (`follow`), `TagsTask` ve geri alma, `[tags]` ayarı (§3.1–§3.5, §4, §10, §11). Arayüz yok | — |
| 2 | **12b** | Toplu etiketleme: seçici, `tags`/`tag-1…7`/`clear-tags`, tuşlar, sağ tık ve macOS menüsü, palet öğeleri, tembel okuyucu (görünen satırlar), etiket noktaları (§3.4, §5) | 12a |
| 3 | **12c** | Etiketler sütunu, etikete göre sıralama ve gruplama, süzgeçte `tag:`, tam klasör okuması (§6) | 12b (okuyucu) |
| 4 | **12d** | Puan ve yorum: seçicide yıldızlar, `Comment…`, Puan/Yorum sütunları, puana göre sıralama/gruplama, `rating:`, Windows Explorer'ın etiket ve puanı salt okunur (§8) | 12c (sütun ve süzgeç yolu) |
| 5 | **12e** | Kenar çubuğunda Etiketler, sürükle-bırakla etiketleme, liste yönetimi, arama ölçütleri ve çubuk alanları, macOS Spotlight, `Clean Up Tag Store…` (§7) | 12c (süzgeç eşleyicisi), 12d (puan/yorum ölçütleri) |
| 6 | **12f** | Vurgulama kuralları (§9) | 12b (okuyucu), 12d (puan koşulu) |

Sıra bağımlılık ve değer: saklama çekirdeği her şeyin temeli ve en riskli kısım (Finder ve Dolphin uyumu, kopyada kayıp), arayüzden ayrı gözden geçirilmeli. Toplu etiketleme ilk görünür değer. Sütun ve süzgeç yolu puan/yorumdan önce gelir, çünkü puan aynı yolu kullanır. Arama ve kenar çubuğu iki ölçütü birlikte ister. Vurgulama en bağımsız parça, ama okuyucu ve puan koşulu için sona.

### 15.1 Süreç (11'in dersleri)

- **Aynı ortak listelere dokunan parçalar paralel yürütülmez.** 11'de dokuz parça aynı `Action` enum'una, menü kimliklerine, Tools menüsüne, notlara ve denetim listelerine dokunduğu için birleştirmeler çakıştı ve sayılar elle düzeltildi. 12'de 12b–12f'nin hepsi `Action`, `context_menu.rs` kimlikleri, `Tags ▸` alt menüsü, `macos-test.md`/`linux-test.md` ve notlara dokunur: **sırayla** yapılır, ya da bir öncekinin dalının üstüne yığılır (stacked PR), asla ikisi aynı anda `master`'dan değil. Yalnız 12a (arayüzsüz) ile 12f'nin saf `highlight.rs` görevi aynı anda yürüyebilir.
- **Eylem sayısı yalnız dizinin türünde** (PR #87): yeni eylem `Action::ALL`'ın `[Action; N]` türünü değiştirir; testlerde ayrı `assert_eq!(…len(), N)` yazılmaz.
- **Menü kimlikleri parça başına önceden ayrılmış** (§12.3), denetim listesi aralıkları aşağıda önceden atanmış: birleştirmede numara kaymaz.
- Her parçada Windows → macOS → Linux sırasıyla yazılır; bir sistemde doğrulanamayan kısım PR açıklamasında "doğrulanacak" notuyla. Çapraz denetim yalnız planın sonunda (`cargo check` macOS ve Linux hedefleri); Docker yok.

### 15.2 Parça ayrıntıları

Denetim listesi aralıkları: macOS 340'tan, Linux 249'dan başlar (11'in son numaraları 339 ve 248).

**12a — saklama çekirdeği**
- *Kapsam:* §3.1–§3.5, §4, §10, §11; `TaskKind::Tags`, `Outcome::MetaChanged`; `[tags]` ayrıştırma.
- *Dosyalar:* `gezik-core/src/{meta, meta/bplist, meta_store}.rs`, `gezik-platform/src/meta/*`, `gezik-platform/src/fs/{unix, secure_unix}.rs`, `gezik-ops/src/{meta, tasks/tags, task, inverse, run, engine}.rs`, `gezik-config/src/{tags, settings, settings_writer}.rs`, `templates/settings.toml`.
- *Bütçe:* exe ≈ +90 KB; boşta 0.
- *Testler:* bplist gidiş-dönüşü (ASCII, UTF-16 Türkçe adlar `Öncelikli`, emoji; Finder'ın yazdığı gerçek bir örnek bayt dizisi), bozuk plist okunur ama yazılmaz; FinderInfo renk bitleri (öbür baytlar korunur, boşalınca silinir); Windows akış metni (bilinmeyen satır korunur, sürüm > 1 yazılmaz); xdg listesi ve Baloo puanı (tek sayı yukarı yuvarlanır); ad kuralları (virgül, satır sonu, 64 karakter, harf duyarsız eşitlik, İ/ı); boş meta kaydı siler; Windows `LastWriteTime` korunur; salt okunur dosya merkezi kayda gider ve özniteliği değişmez; birim desteği önbelleği (birim başına bir deneme); merkezi kayıt: satır kaçışı, atomik yazma, birim anahtarı, `follow` (dosya, klasör öneki, çöp ve geri, silme), dışarıda taşınmış dosyanın ad+boyut+`mtime` ile yeniden bulunması, yolu yeniden kullanılmış dosyaya etiket yapışmaması, 200.000 sınırı; kopya: NTFS→NTFS akış gider (`CopyFileExW`), NTFS→FAT32 (VHD/RAM diski testte; yoksa `#[ignore]`) akış düşer ve `carry` merkezi kayda yazar, FAT→NTFS geri yazar; Linux tmpfs/ext4 `user.*` kopyası, `ENOTSUP` hedefte `carry`; `TagsTask` geri alma ve arada değişmiş dosyanın atlanması; okuma sayaçları (meta ihtiyacı yokken sıfır).
- *macOS/Linux:* macOS'ta `clonefile` ile ve exFAT disk görüntüsüne (`hdiutil`) tamponlu kopyada etiket korunur; Linux'ta ext4 ve vfat döngü aygıtı (CI'da yoksa `#[ignore]`, kullanıcı makinesinde koşulur).
- *Denetim listesi:* macOS 340–343: (340) `cargo test -p gezik-platform meta -- --ignored` geçer; (341) testin bıraktığı `~/gezik-tag-probe.txt` Finder'da kırmızı ve `Gezik Probe` etiketiyle görünür; (342) Finder'da bir dosyaya Yeşil verilir, test bunu okur; (343) exFAT USB'ye kopyada (Finder'la değil, testle) etiket gider. Linux 249–251: (249) aynı test komutu; (250) Dolphin'de dosyanın etiketleri/puanı/yorumu testin yazdıklarıyla aynı; (251) vfat USB'de merkezi kayıt dosyası oluşur ve testin taşımasında izler.

**12b — toplu etiketleme ve noktalar**
- *Kapsam:* §3.4 (görünen satırlar), §5.
- *Dosyalar:* `gezik/src/{meta, tags, actions, context_menu, menu_bar, palette, keys}.rs`, `gezik/src/view/model.rs`, `ui/widgets/{picker, file-view, column-view}.slint`, `gezik-config/src/shortcuts.rs`, `gezik-core/src/palette.rs`.
- *Bütçe:* exe ≈ +110 KB; boşta 0 (Windows/Linux `dots = auto` kapalı başlar; macOS'ta okuyucu açık ama yalnız görünen satırlar).
- *Testler:* seçici durumları (hepsi/bazısı/hiçbiri) ve tıkın sonucu; kutu kapanınca tek iş, tek geri alma; `Create "…"` listeye yazar; `tag-n` aç/kapa kuralı, listede olmayan n; `clear-tags` puanı korur; kısayollar boş ve çakışmasız (Windows/Linux Alt+Shift+0–7, Ctrl+Shift+L; macOS ⌃0–⌃7, ⌃⌘L), `fixed_owner` testleri; `dots = auto`'nun ilk yazmada açılması; okuyucu yalnız istenen satırları okur (sayaç), kaydırmada eski istekler iptal; menü kimlikleri aralıkta; ekran okuyucu adı.
- *macOS/Linux:* ⌃1–⌃7 Finder ile aynı etiketi verir; Mission Control'ün "Switch to Desktop n" (⌃n) açıksa çakışır (§17); File ▸ Tags menüsü.
- *Denetim listesi:* macOS 344–348 (seçici, ⌃n, File ▸ Tags, Finder'ın görmesi, noktalar), Linux 252–254 (seçici, Alt+Shift+n GNOME ve KDE'de, noktalar).

**12c — sütun, sıralama, gruplama, süzgeç**
- *Kapsam:* §6.
- *Dosyalar:* `gezik-core/src/{view, sort, group, meta_filter}.rs`, `gezik/src/{meta, filter}.rs`, `gezik/src/view/{model, listing, columns}.rs`, `gezik-config/src/{views_file, view_rules}.rs`.
- *Bütçe:* exe ≈ +70 KB; boşta 0.
- *Testler:* sıralama (listede önde olan etiket, listede olmayanlar, etiketsizler sonda, azalan), gruplama (ilk etiket, `No Tags` sonda, tek grupta görünme), `tag:` ayrıştırma (tırnak, `none`, `any`, VE, ad deseniyle), okunmamış satırların gösterilip soluk çizilmesi, tam okumanın bitince bir kez yeniden sıralaması, izleyici olayıyla tazelenme, `views.toml` ve `[[view-rules]]` gidiş-dönüşü; 10.000 öğede süre (`tags.ps1`).
- *macOS/Linux:* APFS'te 10.000 öğelik okuma; Linux'ta harf duyarlı adlarla `tag:` harf duyarsız.
- *Denetim listesi:* macOS 349–352, Linux 255–257.

**12d — puan ve yorum**
- *Kapsam:* §8.
- *Dosyalar:* `gezik/src/{tags, dialog, info}.rs`, `gezik-core/src/{view, sort, group, meta_filter}.rs`, `gezik-platform/src/meta/windows.rs` (Explorer özellikleri), `ui/widgets/picker.slint`.
- *Bütçe:* exe ≈ +60 KB; boşta 0.
- *Testler:* yıldız tıkı ve Alt+0…5, karışık seçimde soluk; yorum sorusu (aynı/farklı metin, boşla silme, 1.000 sınırı); Baloo 0–10 dönüşümü; sıralama/gruplama; `rating:` süzgeci; Explorer: `System.Rating` eşikleri, `System.Keywords` sütunda gri, süzgeç/aramada kullanılmaz, çevrimiçi yalnız dosyada okunmaz, takılan işleyicide 2 sn sonra atlanır (sahte işleyici yerine zaman aşımı işlevinin birim testi).
- *macOS/Linux:* macOS'ta `mdls -name kMDItemStarRating` Gezik'in puanını gösterir; Finder Get Info'da yorum görünmeyebilir (§17, beklenen); Linux'ta Dolphin puanı ve yorumu gösterir.
- *Denetim listesi:* macOS 353–356, Linux 258–260.

**12e — kenar çubuğu ve arama**
- *Kapsam:* §7.
- *Dosyalar:* `gezik/src/{sidebar, sidebar_model, search, saved_searches, drag, tags}.rs`, `gezik-core/src/search.rs`, `gezik-search/src/{query, run, everything}.rs`, `gezik-platform/src/mac/spotlight.rs`, `gezik-config/src/settings.rs` (`[sidebar] tags`, `[[searches]]`), `ui/widgets/sidebar.slint`.
- *Bütçe:* exe ≈ +90 KB; boşta 0.
- *Testler:* bölüm satırları G/Ç'siz (sayaç); tık → doğru kapsamlı arama (Windows/Linux klasör + merkezi kayıt, macOS Spotlight sorgu metni ve kaçış: adda `"` ve `\`); sürükle-bırakla etiketleme tek iş; Rename/Color/Move/Remove ayara yazar, dosyaya dokunmaz; `Search and Retag…` iki adımı; arama: ölçütsüz aramada meta okuma yok (sayaç), ölçüt sırası, Everything adaylarının süzülmesi, `[[searches]]` gidiş-dönüşü; `Clean Up Tag Store…` takılı olmayan birime dokunmaz, `.bak` yazar.
- *macOS/Linux:* Spotlight kapalı birimde not satırı; Linux'ta Spotlight yok, tarayıcı.
- *Denetim listesi:* macOS 357–361, Linux 261–263.

**12f — vurgulama kuralları**
- *Kapsam:* §9.
- *Dosyalar:* `gezik-core/src/highlight.rs`, `gezik-config/src/highlight_rules.rs`, `gezik/src/view/model.rs`, `ui/widgets/{file-view, column-view}.slint`, `templates/settings.toml`.
- *Bütçe:* exe ≈ +50 KB; boşta 0.
- *Testler:* her koşul (ad deseni ve `|`, tür, yeni/eski, boyut, gizli, etiket `any`/`none`/ad, puan 0/n, yorum), ilk uyan kazanır, renk ve kalınlık birlikte, hatalı kural uyarısı ve öbürlerinin kalması, 64 sınırı; derleme sayacı (klasör açılırken 0), deneme sayacı; meta koşullu kuralın okunmamış satırda bekletmesi; meta koşulu yoksa okuyucu kurulmaz; seçili satırda karşıtlık kuralı; koyu tema rengi; satır başına ≤ 1 µs.
- *macOS/Linux:* koyu temada adlı renkler; Retina'da kalın yazı.
- *Denetim listesi:* macOS 362–364, Linux 264–265.

## 16. Test

### 16.1 Birim (saf; her sistemde)

`meta` (model, ad kuralları, akış metni, xdg, Baloo), `bplist` (okuma/yazma, Finder örneği), `meta_store` (satırlar, önek, yeniden bulma, sınır), `meta_filter`, `sort`/`group` yeni anahtarları, `highlight` (koşullar, sayaçlar, süre), `search` (yeni alanlar, `[[searches]]`), `shortcuts` (yeni varsayılanlar, çakışmasızlık), `tags` ayarı ve `highlight_rules` ayrıştırma.

### 16.2 Platform

- Windows: akış okuma/yazma, `LastWriteTime` koruma, `FILE_NAMED_STREAMS`, `CopyFileExW`'nin NTFS→NTFS taşıması ve FAT hedefinde düşürmesi, salt okunur dosya, klasör akışı, `IPropertyStore` okuması.
- macOS: `_kMDItemUserTags`/FinderInfo/`kMDItemStarRating`/`kMDItemFinderComment`, `XATTR_NOFOLLOW`, `clonefile` ve `fcopyfile` ile kopyada korunma, exFAT `._` dosyası, `mdfind` sorgusu.
- Linux: `user.xdg.tags`/`user.baloo.rating`/`user.xdg.comment`, bağlantıda reddedilme, `ENOTSUP` hedef, `user.*` kopyası, birim UUID'si.

### 16.3 Çapraz denetim

Planın sonunda (her görevde değil): `cargo check` macOS ve Linux hedefleri, Windows `clippy`/`fmt`/`test`. Docker yok.

### 16.4 Elle denetim listeleri

- **Windows** (kullanıcı uzaktayken; klavye düzeni ayarlarına dokunulmaz): seçici ve Alt+Shift+n (dil değişmediği gözlenir; değişirse karar 13'ün geri düşüşü), NTFS → FAT32 USB → NTFS gidiş-dönüşünde etiketin kalması, OneDrive klasöründe yazma, Explorer'ın JPEG etiketinin gri görünmesi, 150 ve 200 % ölçekte noktalar ve yıldızlar, vurgulama kuralları açık/koyu temada.
- **macOS** (`macos-test.md`'ye eklenir, sonuçlar `macos-test-results.md`'ye): Finder ↔ Gezik iki yönde etiket, renk ve FinderInfo, ⌃1–⌃7, File ▸ Tags, Spotlight'lı kenar çubuğu, exFAT USB, iCloud Drive'da etiketin öbür Mac'te görünmesi (olanak varsa).
- **Linux** (gerçek makine, GNOME ve KDE, X11 ve Wayland): Dolphin ↔ Gezik etiket/puan/yorum, vfat USB'de merkezi kayıt, Alt+Shift+n, GNOME Files'ın kopyasında xattr'ın korunması (Gezik'in kopyasıyla karşılaştırma).

## 17. Riskler

| Risk | Etki | Önlem |
|---|---|---|
| NTFS akışı FAT/exFAT'e kopyada, e-postada, zip'te, buluta yüklemede düşer | Etiket sessizce kaybolur | Gezik'in kendi kopyası FAT'e merkezi kayıtla taşır (`carry`); dışarıdaki kopyalar için README'de tablo; Info penceresinde `Tags are kept in Gezik's store on this drive` notu |
| `CopyFileExW` akışsız hedefte akışı düşürmek yerine hata verir | FAT'e kopya bozulur | 12a'nın ilk görevi testle doğrular; hata verirse akışlar `COPY_FILE_*` bayrağıyla bırakılır ya da kopya akışsız yoldan yapılır ve `carry` çalışır |
| Virüs tarayıcıları, yedek ve eşitleme araçları akışları/xattr'ları siler ya da taşımaz (OneDrive, bazı yedekleyiciler, `rsync` `-X`'siz) | Etiket kaybolur | Belgelenir; `write-to-files = false` seçeneği (her şey merkezi kayıtta); merkezi kayıt yapılandırma klasöründe olduğundan ayarlarla yedeklenir |
| Akışa yazmak `LastWriteTime`'ı değiştirir | Sıralama, yedekler, eşitleme yanılır | Yazma öncesi/sonrası `SetFileTime` (§3.5); test |
| Arşiv biti (`ARCHIVE`) akış yazınca kalkar | Yedek araçları dosyayı yeniden yedekler | Doğru davranış (meta değişti); belgelenir |
| Dosya sistemlerinin xattr desteği farklı (Linux `nouser_xattr`, tmpfs eski çekirdek, NFS, SMB `streams_xattr`'sız, FUSE) | Yazma reddedilir | Reddedilince merkezi kayıt, birim başına bir deneme; okuma her zaman ikisini birleştirir |
| Finder renk numaraları ile FinderInfo etiket bitlerinin eşlemesi yanlış | Finder'da yanlış renk | Finder'ın yazdığı gerçek örneklerle test; macOS denetim maddesi 341/342 |
| Türkçe macOS'ta Finder'ın varsayılan etiket adları yerelleştirilmiş (`Kırmızı`) | Gezik'in `Red`'i Finder'da ayrı etiket olarak görünür | `[tags] list` kullanıcının adlarıyla yazılabilir; renkler numarayla eşleşir; varsayılan liste sabit İngilizce adlarla gelir (Finder'ın tercih dosyası okunmaz) ve README bunu söyler (§18) |
| Finder'ın Get Info'su yorumu `.DS_Store`'dan okur | Gezik'in yorumu Finder'da görünmeyebilir | Belgelenir; Spotlight ve Gezik görür; AppleScript yolu (izin ister) bilerek yok (karar 11) |
| Alt+Shift+rakam Windows'ta dil değiştirir ya da Linux masaüstünde ayrılmıştır | Tuş çalışmaz / dil değişir | Ekran testi; olursa varsayılanlar kaldırılır (karar 13); seçici her zaman Ctrl+Shift+L |
| macOS'ta ⌃1–⌃7 Mission Control'ün masaüstü tuşları açıksa | Tuş sisteme gider | Varsayılan kapalıdır; açıksa kullanıcı Gezik'te değiştirir; README notu |
| Merkezi kayıtta yolu yeniden kullanılan dosyaya etiket yapışması | Yanlış dosya etiketli görünür | Boyut/`mtime`/kimlik üçü de farklıysa gösterilmez (§4.4) |
| Büyük klasörde etikete göre sıralama Windows'ta yavaş (dosya başına akış açma) | Bekleme | Arka plan, ilerleme, sıra okuma bitince bir kez değişir; 10.000'de ≤ 1 sn ölçütü; 100.000'de durum satırında ilerleme |
| Explorer özellik işleyicileri yavaş/takılır (bozuk dosya, üçüncü parti işleyici) | Okuyucu durur | Yalnız sütun açıkken, yalnız sabit uzantılarda, zaman aşımlı ayrı çağrı, çevrimiçi dosyalar dışarıda |
| Sabit bağlantılar (hard link) akışı/xattr'ı paylaşır | Bir adı etiketlemek öbürünü de etiketler | Doğru davranış (aynı dosya); belgelenir |
| Exe bütçesi (altı parça, Slint değişiklikleri) | Parça başına sınır aşılır | Yeni katman yok (§3.6); her parçanın ilk ve son ölçümü; aşılırsa PR'da gerekçeyle |
| Paralel parçaların ortak listelerde çakışması (11'in dersi) | Birleştirme hataları, sayı kayması | Sıralı ya da yığılı parçalar (§15.1) |

## 18. Bilinen sınırlar

- Etiket dizini yok: "bu etiketi taşıyan bütün dosyalar" Windows/Linux'ta tarama ister (macOS'ta Spotlight).
- Etiketi dosyalarda toplu yeniden adlandırma iki adımdır (ara, sonra seçiciyle değiştir).
- Windows'ta NTFS akışları OneDrive ile eşitlenmez, zip'e ve e-postaya girmez; FAT/exFAT'e Gezik dışında yapılan kopyada düşer.
- Merkezi kayıttaki etiket, dosya Gezik dışında başka birime taşınınca ya da adı ve içeriği birlikte değişince kaybolur.
- Linux'ta `user.xdg.tags` renk taşımaz; renk Gezik'in listesinden gelir.
- macOS'ta Gezik'in yazdığı yorum Finder'ın Get Info'sunda görünmeyebilir.
- Türkçe macOS'ta Finder'ın yerelleştirilmiş varsayılan etiket adları Gezik'in İngilizce varsayılan adlarıyla aynı etiket değildir (renkler aynıdır); kullanıcı `[tags] list`'i Finder'daki adlarla yazabilir.
- Explorer'ın "Tags" ve puanı yalnız gösterilir; süzme, arama, gruplama Gezik'in kendi kaydıyla.
- Sıkıştırma/açma etiket taşımaz; `descript.ion` okunmaz.

## 19. Kararlar (bu spec'in; kullanıcıya sorulmadı)

1. **Sıra: 12a, 12b, 12c, 12d, 12e, 12f** (§15). *Gerekçe:* saklama her şeyin temeli ve Finder/Dolphin uyumu en riskli kısım, arayüzsüz ayrı gözden geçirilmeli; toplu etiketleme ilk görünür değer; puan sütun/süzgeç yolunu yeniden kullanır; arama ve kenar çubuğu iki ölçütü birlikte ister; vurgulama bağımsız ama okuyucu ve puana dayanır.
2. **Parçalar sırayla ya da yığılı; paralel değil** (§15.1). *Gerekçe:* 11'de aynı `Action` listesine, menü kimliklerine, Tools menüsüne ve denetim listelerine paralel dokunan parçalar birleştirmede çakıştı; 12'nin beş arayüz parçası aynı listelere dokunuyor.
3. **Merkezi kayıtta anahtar birim + göreli yol; boyut, `mtime`, dosya kimliği yalnız yeniden bulma ve yanlış eşleşmeyi önleme için** (§4.4). *Gerekçe:* geri düşüş yerlerinde (FAT, exFAT, ağ) dosya kimliği kararlı değil ya da hiç yok; yol, içerik değişince de etiketi korur (en sık durum: etiketli dosyayı düzenlemek); birim seri numarası/UUID'si sürücü harfi ve bağlama noktası değişse de bulur; üç bilginin üçü de farklıysa bu başka bir dosyadır.
4. **Linux'ta `user.xdg.tags`, `user.baloo.rating` (×2), `user.xdg.comment`** (§4.3). *Gerekçe:* freedesktop önerisi ve KDE Dolphin/Baloo'nun bugün kullandığı anahtarlar; Linux'ta Finder'ın karşılığı Dolphin; Gezik'e özel anahtar başka hiçbir programla konuşmazdı. Renk yok, Gezik'in listesinden.
5. **İkili plist kodlayıcısı Gezik'in içinde, `plist` sandığı eklenmez** (§4.1). *Gerekçe:* gereken yalnız düz dizgi dizisi, tamsayı ve tek dizgi; `plist` sandığı serde ve XML'le birlikte gelir (exe bütçesi), ≈ 250 satırlık saf kod test edilebilir.
6. **Etiket adında virgül ve satır sonu yasak; harf duyarsız eşitlik** (§3.1). *Gerekçe:* virgül Linux'un ayırıcısı, satır sonu macOS'un renk ayırıcısı; üç sistemde aynı adın yazılabilmesi için ortak kural; Finder etiketleri harf duyarsız eşler.
7. **Windows'ta tek akış `gezik.meta`, sürümlü metin, bilinmeyen satırlar korunur** (§4.2). *Gerekçe:* tek akış = dosya başına tek açma (okuma maliyeti); metin elle ve `more < dosya:gezik.meta` ile okunur; ileri sürümler yeni satır ekleyebilir. Yarım yazma riski 16 KB'lık akışta kabul (atomik değiştirme akışlarda yok).
8. **Windows'ta akış yazınca `LastWriteTime` geri konur** (§3.5). *Gerekçe:* etiket vermek dosyayı "değiştirmiş" göstermemeli; Finder ve Dolphin'de xattr zaten `mtime`'ı değiştirmez, üç sistem aynı davranır.
9. **`[tags] write-to-files`, varsayılan true** (§3.2). *Gerekçe:* kullanıcı kararı dosya sisteminin kendi kaydını istiyor (Finder uyumu, taşımada izleme); ama dosyaların değişmesini istemeyen (eşitleme/yedek gürültüsü, paylaşılan klasör) kullanıcıya tek anahtarlık çıkış gerekli.
10. **Linux kopyası bütün `user.*` xattr'larını taşır** (§4.3). *Gerekçe:* yalnız Gezik'in anahtarlarını taşımak Dolphin'in ve başka programların meta verisini düşürürdü; `cp --preserve=xattr`, GNOME Files ve macOS'un `fcopyfile`'ı da hepsini taşır; 64 KB sınırı kötü niyetli büyük xattr'a karşı.
11. **macOS'ta puan `kMDItemStarRating`, yorum `kMDItemFinderComment` xattr'ında; Finder'a AppleScript ile yazılmaz** (§4.1). *Gerekçe:* Spotlight ikisini de dizinler, Gezik ve Spotlight tutarlı; AppleScript Otomasyon izni ister, Finder'ı çalıştırır ve yavaştır; Get Info'da görünmemesi bilinen sınır.
12. **macOS'ta FinderInfo etiket rengi de yazılır** (§4.1). *Gerekçe:* Finder kendisi yazar; eski uygulamalar ve bazı yedekleyiciler yalnız onu okur; yazmamak Finder'ın kendi yazdığı dosyadan farklı bir sonuç bırakırdı.
13. **Tuşlar: tek etiket Windows/Linux Alt+Shift+1…7 / 0, macOS ⌃1…⌃7 / ⌃0; seçici Ctrl+Shift+L / ⌃⌘L** (§5.2). *Gerekçe:* macOS'ta ⌃n Finder'ın kendi etiket tuşları; Windows/Linux'ta Ctrl+rakam sekmeler, Alt+rakam iğneler, Ctrl+Alt+rakam AltGr ile yazı (bugünkü kural), Alt+Shift+rakam boş. Windows dil değiştirme tuşuyla çakışma ekran testinde görülürse varsayılanlar kaldırılır, eylemler kalır. ⌃⌘T Finder'ın "Add to Sidebar"ı olduğu için L ("label").
14. **Yerel sağ tık menüsünde renk noktası yok; noktalar Gezik'in çizdiği yerlerde** (§5.3). *Gerekçe:* Win32 menüsünde renkli öğe bit eşlem ve sahip çizimi ister (kod ve bakım); seçici, satırlar ve kenar çubuğu rengi zaten gösterir.
15. **Etiket noktaları `auto`: macOS'ta açık, Windows/Linux'ta ilk etiket yazılınca açılır** (§5.4). *Gerekçe:* kullanıcının hafiflik kuralı (yalnız gerekince oku) ile Finder kullanıcısının beklentisi; etiket kullanmayan Windows/Linux kullanıcısı tek bir akış/xattr okuması ödemez, kullanan ise ayar aramadan görür.
16. **Seçicinin değişiklikleri kutu kapanınca tek iş** (§5.1). *Gerekçe:* üç etiketi art arda veren kullanıcı için tek Ctrl+Z; her tıkta bir iş geçmişi ve iş panelini doldururdu.
17. **Etikete göre gruplamada dosya yalnız ilk etiketinin grubunda** (§6.2). *Gerekçe:* Finder'ın davranışı; bir dosyanın iki satırda görünmesi seçimi, sayıları ve 10d'nin "satır = öğe" kuralını bozardı.
18. **Meta okunmamış satırlar süzgeçte okuma bitene kadar gösterilir, soluk** (§6.3). *Gerekçe:* önce gizleyip sonra göstermek "dosya yok" yanılgısı verir; soluk çizim beklendiğini söyler.
19. **Explorer'ın "Tags" ve puanı yalnız sütunda ve Info'da; süzgeç, arama, gruplama, vurgulama kullanmaz** (§8.3). *Gerekçe:* okumak her dosyada özellik işleyicisini çalıştırmak demek (yavaş, takılabilir, bulut dosyasını indirebilir); aramada binlerce dosyada bunu yapmak hafiflik kuralını bozar; kullanıcı kararı "oku ve göster".
20. **Kenar çubuğundaki etiket: macOS'ta Spotlight ile bütün disk, Windows/Linux'ta gösterilen klasör + merkezi kayıt** (§7.1). *Gerekçe:* macOS'ta Finder'la aynı sonuç ucuzca (Spotlight dizini zaten var); Windows/Linux'ta dizin yok, bütün disk taraması tık başına dakikalar; bir tıkla `Search all drives` seçeneği açık.
21. **Etiketi listede yeniden adlandırmak dosyalara dokunmaz; `Search and Retag…` iki adımı önerir** (§7.3). *Gerekçe:* dizin olmadan "bütün dosyalar" bilinemez; sessiz bir disk taraması ve toplu yazma kullanıcının görmediği bir iş olurdu; iki adım görünür ve geri alınabilir.
22. **Vurgulama: ilk uyan kural renk ve kalınlığı birlikte verir; en çok 64 kural** (§9.1). *Gerekçe:* TC'nin bilinen kuralı, sonucu tahmin edilebilir; kuralları birleştirmek (renk birinden, kalınlık öbüründen) sıralamayı anlaşılmaz yapar; 64 elle yazılabilecekten fazla ve satır başına süreyi sınırlar.
23. **Meta koşullu kural, satırın metası okunmamışken sonraki kurallara geçmez** (§9.2). *Gerekçe:* aksi hâlde satır önce alttaki kuralın rengini alıp sonra değişirdi (yanıp sönme); bekleyen satır kısa süre kuralsız görünür.
24. **`descript.ion` ve `.2do` yok** (§1). *Gerekçe:* kullanıcının saklama kararı dosya sisteminin kaydı ve merkezi kayıt; üçüncü bir yer (klasördeki yan dosya) her klasöre dosya ekler, iki kaydı eşitleme sorunu getirir; TC kullanıcıları için okuma 13. adımın meta veri alanı kaydına bırakılır.
25. **Merkezi kayıt düz metin `tags-store.tsv`, iş sonunda atomik yazma, 200.000 sınırı** (§4.4). *Gerekçe:* Gezik'in ayar ilkesi (düz metin, elle okunur, rakip notu "JSON, plist ya da ikili" karşıtı); SQLite bağımlılığı exe ve açılış maliyeti; geri düşüş kaydı çoğu kullanıcıda birkaç yüz satır.
26. **Sahipsiz kayıtlar kendiliğinden silinmez; `Clean Up Tag Store…` takılı birimlerde sorarak siler** (§7.4). *Gerekçe:* takılı olmayan USB'nin kayıtları "yok" görünür; kendiliğinden temizlik kullanıcının etiketlerini sessizce silerdi; arka plan temizliği "boşta hiçbir şey" kuralına aykırı.
27. **Etiket yazmak iş motorundan geçer, `Outcome::MetaChanged` ile geri alınır; arada değişen dosya atlanır** (§3.5). *Gerekçe:* bugünkü geri alma ve iş paneli bedava; 9'un `AttributesChanged` kuralı başkasının (Finder'ın) yaptığı değişikliği ezmemeyi zaten tanımlıyor.
28. **Kopyada merkezi kayıt ↔ dosya taşıması yalnız `RunCx::copy_file`'ın sonunda, yalnız uçlar farklı türdeyse** (§10). *Gerekçe:* kopya, sürücüler arası taşıma, eşitleme ve süzgeçli kopya hepsi oradan geçer (tek yer); aynı tür birimlerde sistem zaten taşır ve ek okuma maliyeti olmamalı.
29. **Yeni Slint katmanı yok; seçici `picker.slint`, yorum `dialog.rs`** (§3.6). *Gerekçe:* 11'in katmanları 120–190 KB tuttu; seçici bir süzgeçli liste, palet kutusu tam olarak bu; bütçe ve tutarlılık.
30. **Menü kimlikleri 2400–2599; `Action` 113 → 124** (§12). *Gerekçe:* 11 2200–2399'da biter; her parçanın aralığı önceden ayrılınca sıralı parçalar da çakışmaz.
31. **Bütçe: her parça ≤ +262.144 B (tahminler §14), boşta 0, açılış ≤ +1 ms, klasör açma ±%2** (§14). *Gerekçe:* önceki adımların kuralı; etiketler isteğe bağlı olduğu için kullanmayan için hiçbir maliyet kabul edilmez.
