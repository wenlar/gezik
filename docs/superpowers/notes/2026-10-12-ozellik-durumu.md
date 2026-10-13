# Özellik durumu: rakip özetine göre (2026-10-12)

- **Tarih:** 2026-10-12, 14. ve 15. adımlardan sonra 2026-10-13'te güncellendi. **Taban:** `master` e2c5f46 (PR #128: 15e+15f). 5–14. adımlar birleşti: 12 Etiketler (#91–#97), 13 Önizleme 2 (#92, #98–#100, #102, #103), 14 Taşınabilirlik (#104, #105, #108–#110, #112).
- **Kaynak:** `2026-10-07-rakip-ozet.md` §1 tablosu (1–66), §3 ayırt edici yanlar, önerilmeyenler (§1 ve §6).
- **Nasıl denetlendi:** her satır `docs/features.md`, `README.md`, adım spec'lerinin "Kapsam dışı" bölümleri, `2026-10-03-ayarlar-ve-tema-followups.md`'deki adım notları ve gerekince kodla (`shortcuts.rs` eylem listesi, `settings.rs`, `preview.rs`, komut yer tutucuları) karşılaştırıldı. PR numaraları `git log --merges`'ten.

Rakip kısaltmaları rakip özetindeki gibi: OC, Fi, DO, FP, Far, FL, PF, TC.

## Özet

| Durum | §1 tablosu (66 satır) | Önerilmeyenler (13 madde) |
|---|---|---|
| ✅ Var | 53 | 0 |
| 🟡 Kısmen | 7 | 0 |
| 🔜 Planlı | 5 | 0 |
| ❌ Yok / bilerek kapsam dışı | 1 | 13 |

Rakip özetinde (2026-10-07) 66 satırın hiçbiri "Var" değildi: 24'ü Kısmen, 8'i Planlı, 34'ü Yok idi. İlk sürümde (2026-10-12, 11. adımdan sonra) 44 Var, 12 Kısmen, 9 Planlı idi; 12–14. adımlar yedi satırı Var'a taşıdı (5, 13, 18, 21, 25, 50, 65); 15. adım satır 9'u; 16. adım (16a, 16b) satır 12'yi.

## Ana tablo

"Rakiplerde": sekiz rakipten kaçında var (rakip özetindeki Σ). "Önceki durum": rakip özetindeki "Gezik" sütunu.

| No | Özellik | Rakiplerde | Önceki durum | Şimdi | Nerede / not |
|---|---|---|---|---|---|
| 1 | Yazarken süzme / hızlı filtre (joker, `!`, kayıtlı) | 8/8 | Kısmen | ✅ Var | 6a (#13): Ctrl+F ya da `/`, `;`, `*`, `?`, `!`, `[[filters]]`, `typing = "filter"` |
| 2 | Özyinelemeli arama (ad, içerik, tarih/boyut, önizleme) | 8/8 | Planlı | ✅ Var | 8a (#22); Windows'ta Everything |
| 3 | Klasör boyutu (arka planda, sütun, sıralama) | 8/8 | Yok | ✅ Var | 8b (#29); varsayılan `folder-sizes = "off"` (#31), istekle ya da `local`/`all` |
| 4 | Çift panel + karşı panele kopyala/taşı | 8/8 | Planlı | ✅ Var | 10b (#64): F3, F5/F6, Tab, Ctrl+U, sekmeyi diğer bölmeye taşı |
| 5 | Grafik ayar penceresi | 8/8 | Kısmen (yalnız TOML) | ✅ Var | 14a (#104): sayfalar, arama, tema önizlemesi, anında uygulama, yorumlar korunur; 14b (#105) kısayol sayfası; 14d (#109) dışa/içe aktarma |
| 6 | Sürücü çıkarma, ağ paylaşımı, ağ keşfi | 8/8 | Kısmen | 🟡 Kısmen | 9b6 (#44): çıkarma, Sunucuya Bağlan, Windows sunucu paylaşımları. Ağ keşfi bilerek kapsam dışı (9 spec §1) |
| 7 | Yolu kopyala: kısayol, biçimler, üç sistem | 7/8 | Kısmen | ✅ Var | 7a (#17): Ctrl+Shift+C, Copy path as ▸ (tırnaklı, `/`, ad, klasör, `file://`, UNC) |
| 8 | Oturum geri yükleme, sekme setleri | 7/8 | Yok | ✅ Var | 7a (#17): `[session] restore`, `[[tab-sets]]`; çift bölme 10b, pencereler 10g (#71) |
| 9 | Arayüz çevirisi (önce Türkçe) | 7/8 | Yok | ✅ Var | 15 (#120, #123, #125–#128): İngilizce, Türkçe; Almanca, İspanyolca, Fransızca, Çince makine çevirisi; `.po` dosyasıyla yeni dil |
| 10 | Yol tamamlama, son klasörler, geçmiş, ortam değişkeni | 7/8 | Kısmen (Ctrl+L) | ✅ Var | 6b (#14): tamamlama, Recent/Frequent, `~`, `%AD%` |
| 11 | Görünüm seçenekleri (gizli/sistem, uzantı, klasörler önce, biçimler) | 7/8 | Kısmen | ✅ Var | 7b (#19): `[view]` ve View menüsü |
| 12 | Arşivin içinde klasör gibi gezinme | 7/8 | Planlı | ✅ Var | 16a (#139): zip, 7z, tar, rar, iso ve 7-Zip'in okuduğu her biçimde gezinme, önizleme, arama, iç içe arşiv; 16b: içine bırakma, yapıştırma, silme, yeniden adlandırma, yeni klasör, taşıma, iç içe yazma, "Update in Archive", geri alma |
| 13 | Zengin önizleme (video/ses, çok sayfalı PDF, renklendirme, yakınlaştırma, hex) | 7/8 | Kısmen | ✅ Var | 13a (#92) renkli kod, büyük metin, hex; 13b (#98) PDF sayfaları (pdfium, istekle) ve resim yakınlaştırma; 13c (#99) ffmpeg ile video/ses |
| 14 | Varsayılan dosya yöneticisi (Win+E, `NSFileViewer`) | 6/8 | Yok | ✅ Var | 9b4 (#42), geri alınabilir. Windows'ta başka programların "Klasörde göster"i yönlendirilemiyor (9b11 düştü) |
| 15 | Kısayol / symlink / hardlink / junction oluşturma | 7/8 | Yok | 🟡 Kısmen | 7c (#20): `.lnk`, junction, symlink; macOS takma ad 9a1 (#34). Hardlink bilerek kapsam dışı (7 spec) |
| 16 | Terminalde aç (seçim, yönetici olarak) | 6/8 | Yok | ✅ Var | 7a (#17): Shift+F4, `[terminal] command`, Windows'ta yönetici olarak |
| 17 | Otomatik güncelleme | 5/8 | Planlı | 🔜 Planlı | Yayın hazırlığı |
| 18 | Etiketler (renk, kenar çubuğu, süz/sırala, Finder uyumu) | 5/8 | Planlı | ✅ Var | 12a–12c (#91, #93, #94), 12e (#96): Finder xattr, Linux `user.xdg.*`, Windows `:gezik.meta` akışı + merkezi kayıt; TAGS kenar çubuğu, `tag:` süzgeci, Spotlight araması |
| 19 | Favori grupları, takma ad, simge/renk, numaralı kısayol | 6/8 | Kısmen | 🟡 Kısmen | 7b (#19): takma ad, grup, Alt+1…9. Simge ve renk bilerek kapsam dışı (7 spec) |
| 20 | Gruplama (tür, tarih, boyut, etiket) | 5/8 | Yok | ✅ Var | 10d (#58): tür, tarih, boyut; etiket ve puan 12c/12d (#94, #95); meta veri alanları 13e (#102) |
| 21 | Meta veri paneli ve sütunları (EXIF, ID3, süre) | 6/8 | Kısmen (piksel boyutu) | ✅ Var | 13d (#100): 31 alanlı kayıt, EXIF/HEIF, ID3/FLAC/Ogg/MP4/MKV/AVI, DOCX/PDF okuyucuları, önizleme paneli; 13e (#102): sütun, sıralama, gruplama, süzgeç |
| 22 | Bulut sürücüleri: kenar çubuğu, durum, indir/boşalt | 6/8 | Yok | ✅ Var | 9b5 (#41). Linux'ta indir/boşalt yok (standart yok) |
| 23 | Çok pencere, sekmeyi koparıp yeni pencere | 5/8 | Kısmen (ikinci süreç) | ✅ Var | 10g (#71): tek süreçte pencereler; koparma Windows ve macOS'ta, her yerde "Move to new window" |
| 24 | Uzak bağlantılar (SFTP, FTP, WebDAV, S3) | 6/8 | Yok | 🔜 Planlı | 18 Uzak bağlantılar (açık karar) |
| 25 | Puan, yorum, klasör notu | 6/8 | Yok | ✅ Var | 12d (#95): yıldız puan, yorum, sütun, sıralama, `rating:` süzgeci; Windows'ta Explorer puanı salt okunur. `descript.ion`/`.2do` bilerek kapsam dışı (12 spec) |
| 26 | Get Info: izin, sahip, öznitelik, zaman damgası düzenleme | 6/8 | Kısmen | 🟡 Kısmen | 9a3 (#39), 9b7 (#46): macOS/Linux'ta izin, sahip, gizli/kilitli; Windows'ta sistem Özellikler. Zaman damgası ve ACL bilerek kapsam dışı (9 spec) |
| 27 | Komut paleti / Quick Open | 4/8 | Planlı | ✅ Var | 8b (#29): Ctrl+Shift+P, Ctrl+P, `>` |
| 28 | Sekme kolaylıkları (kapatılanı aç, Ctrl+1…9, seçici) | 5/8 | Yok | ✅ Var | 6a (#13): Ctrl+Shift+T, Ctrl+1…9, Ctrl+Shift+A |
| 29 | Desenle seç/bırak, ters çevir, aynı uzantı, önceki seçim | 5/8 | Yok | ✅ Var | 6a (#13) |
| 30 | `[[commands]]` büyütme (kısayol, alt menü, `ask`, `{files}`, diğer panel, iki dosya) | 5/8 | Kısmen | 🟡 Kısmen | 6b (#14): `shortcut`, `menu`, `ask`, `{files}`. Diğer bölme yer tutucusu ve iç içe menü yok |
| 31 | Sağlama toplamı | 5/8 | Yok | ✅ Var | 11a (#78): CRC32–SHA-512, doğrulama, CSV |
| 32 | Klasör ağacı | 5/8 | Yok | ✅ Var | 10c (#59): kenar çubuğunda |
| 33 | Çöp kutusu görünümü | 5/8 | Kısmen (Ctrl+Z) | ✅ Var | 9b2 (#37): Put Back, kalıcı sil, boşalt |
| 34 | Birlikte aç, Share/AirDrop, Services (macOS/Linux) | 5/8 | Kısmen (yalnız Windows) | ✅ Var | 9a2 (#36) macOS; 9b10 (#56) Linux'ta Birlikte aç (Linux'ta Share standardı yok) |
| 35 | Klasör karşılaştırma ve eşitleme | 5/8 | Yok | ✅ Var | 11d (#83), 11e (#85): üç kip, `[[sync-profiles]]`, tek Ctrl+Z |
| 36 | Miller sütunları | 4/8 | Yok | ✅ Var | 10e (#66, #67) |
| 37 | Sistem geneli kısayol, tepsi, girişte başlama | 5/8 | Yok | ✅ Var | 9b9 (#57), varsayılan kapalı. Linux tepsi menüsü yok |
| 38 | Yönetici yetkisiyle işlem | 5/8 | Yok | ✅ Var | 9b7 (#46): tek seferlik yardımcı, "Retry as administrator" |
| 39 | Tarayıcı/e-postadan sanal dosya bırakma | 3/8 | Yok | ✅ Var | 9b8 (#55): Windows ve macOS; Linux XDS bilerek kapsam dışı |
| 40 | Git | 3/8 | Planlı | 🔜 Planlı | 16 Gelişmiş |
| 41 | Eşli gezinme | 4/8 | Yok | ✅ Var | 10b (#64): Sync Browsing |
| 42 | Tek tıkla açma | 4/8 | Yok | ✅ Var | 7b (#19): `single-click-open` |
| 43 | Komut satırı seçenekleri | 4/8 | Kısmen | ✅ Var | 9b1 (#35): `--new-tab`, `--new-window`, `--select`; PATH 9b3 (#38) |
| 44 | MTP / telefon | 4/8 | Planlı | 🔜 Planlı | 16 Gelişmiş |
| 45 | macOS sistem simgeleri, QuickLook | 2/8 | Yok | ✅ Var | 9a1 (#34), sistem paneli 9a2 (#36) |
| 46 | Panodaki resmi/metni dosya olarak yapıştırma | 2/8 | Yok | ✅ Var | 7c (#20) |
| 47 | Yeni dosya şablonları; seçimle yeni klasör | 3/8 | Kısmen | ✅ Var | 7c (#20): `templates/`, Ctrl+Alt+N |
| 48 | Drop Stack | 2/8 | Yok | ✅ Var | 7c (#20): oturum boyu |
| 49 | Kayıtlı aramalar | 3/8 | Yok | ✅ Var | 8b (#29): `[[searches]]`, kenar çubuğunda |
| 50 | Vurgulama kuralları, sıralama grupları | 3/8 | Kısmen (tür rengi) | ✅ Var | 12f (#97): `[[highlight-rules]]` ad, tür, boyut, yaş, etiket, puan, yoruma göre renk ve kalınlık |
| 51 | Düz görünüm | 3/8 | Yok | ✅ Var | 8a (#22): Ctrl+B |
| 52 | Yinelenen dosya bulucu | 3/8 | Yok | ✅ Var | 11f (#81) |
| 53 | Gömülü terminal / adres çubuğunda komut | 3/8 | Yok | 🔜 Planlı | 16 Gelişmiş (sonra) |
| 54 | İki dosyayı karşılaştırma | 3/8 | Yok | ✅ Var | 11c (#75): yan yana ve `[tools] diff` |
| 55 | Görünür işlem günlüğü | 3/8 | Kısmen | ✅ Var | 7c (#20): işlem panelinin History sekmesi; kalıcı log bilerek yok |
| 56 | Listede ve kenar çubuğunda yaylı klasörler | 2/8 | Kısmen (yalnız sekmede) | 🟡 Kısmen | Hâlâ yalnız sekmede; bilerek kapsam dışı (7 spec) |
| 57 | Kuyruk düzenleme, bant sınırı, kopya doğrulama | 3/8 | Kısmen | ✅ Var | 11b (#82) |
| 58 | Yerleştirilebilir paneller | 1/8 | Yok | ❌ Yok | Bilerek kapsam dışı (10 spec): iki bölme, yan yana |
| 59 | Boyut haritası | 1/8 | Yok | ✅ Var | 11g (#77): döşeme haritası; halka grafik bilerek yok |
| 60 | Klasör listesini dışa aktarma | 2/8 | Yok | ✅ Var | 11h (#76): txt, csv, html |
| 61 | Süzgeçli kopya, jokerle hedef adı | 3/8 | Yok | ✅ Var | 11i (#79): Copy To / Move To, `*.jpg>*.cr2` |
| 62 | Dosya bölme / birleştirme | 2/8 | Kısmen | ✅ Var | 11i (#79): `.001` + `.crc` |
| 63 | Otomatik görünüm kuralları | 1/8 | Kısmen | ✅ Var | 10f (#60): `[[view-rules]]` |
| 64 | Kilitli sekmeler | 2/8 | Yok | ✅ Var | 6a (#13); kilit oturumla gelir (7a) |
| 65 | Taşınabilir mod | 4/8 | Kısmen (`GEZIK_CONFIG_DIR`) | ✅ Var | 14e (#110): exe yanındaki `gezik-portable` işareti → `gezik-data/`; makine başına sistem günlüğü. Hazır taşınabilir paket Yayın hazırlığında |
| 66 | Meta veri alanlarını her yerde kullanma | 2/8 | Kısmen (EXIF tarihi adda) | 🟡 Kısmen | 13e/13f (#102, #103): sütun, süzgeç, arama (`artist:… duration>3m`), ad şablonu (`{track:02} {artist}`). Eksik: vurgulama kuralında `where`, etiketlerin alan kaydına bağlanması |

### Önerilmeyenler (rakip özeti §1 ve §6)

Hepsi ❌ (bilerek kapsam dışı); 2026-10-07'den beri hiçbiri eklenmedi.

| Madde | Neden |
|---|---|
| Betik dili, eklenti SDK'sı, makro kaydı | Yorumlayıcı ister; karşılığı palet ve `[[commands]]` |
| TC'nin eklenti API'leri (WCX/WFX/WLX/WDX) | Karşılığı meta veri alanı kaydı (13) ve `[[commands]]` |
| Dahili düzenleyici | Kapsam dışı |
| Güvenli silme | SSD'de anlamsız |
| Süreç görüntüleyici | Kapsam dışı |
| App Deleter | Kapsam dışı |
| Mica, saydamlık, arka plan resmi | Kapsam dışı (kodda yok) |
| Araç çubuğu / menü düzenleyici | Emek yüksek; palet ve `[shortcuts]` karşılar |
| Düğme çubuğu ve ana menü düzenleyici (TC) | Aynı neden |
| Ekran yakalama, uygulama başlatıcı, Subversion, SFX, masaüstünü devralma | Kapsam dışı |
| Kodla/çöz (UUE/XXE/BinHex) | Kapsam dışı |
| Paralel/USB kablo bağlantısı | Kapsam dışı |
| Sistem bilgisi | Kapsam dışı |

## Gezik'te olup rakiplerde olmayanlar

Rakip özeti §3'ün listesi bugün de geçerli; aşağıda o liste ve sonradan eklenenler var. Yalnız rakip notlarına göre sekiz rakipte de olmayanlar yazıldı.

- **Üç sistemde tek kod**, ayar dosyası sistemler arasında aynen taşınır.
- **Her dosya işleminde oturum boyu geri alma:** kopyala, taşı, ad, toplu ad, bağlantı, çöp, ezme, arşiv, dönüştürme, komut; sonradan eklenenler: bırakma yığını (7c), Bilgi penceresindeki izin/sahip değişikliği (9a3), yinelenenleri silme (11f), liste dışa aktarma (11h), Copy To ve böl/birleştir (11i), **eşitlemenin tamamı tek Ctrl+Z** (11e). TC'de dosya işlemleri için geri alma yok.
- **Önceden çakışma listesi:** iş başlamadan bütün çakışmalar tek listede, ezilen dosya çöpe.
- **Anında, çökmeye dayanıklı kalıcı silme.**
- **PDF işlemleri** (resimden PDF, birleştir, böl, sayfa çıkar, PDF'ten resim) ve **toplu metin kodlaması dönüştürme.**
- **SHA-256 sabitli araç indirme** (7-Zip, ffmpeg, pdfium).
- **Kabuksuz, geri alınabilir kullanıcı komutları.**
- **Yalnız çöpe silen eşitleme:** Mirror ya da `×` satırı bile kalıcı silmez; çöpü olmayan sürücüde hiç silmez (11e, #84).
- **Sisteme yazılan her şeyin günlüğü ve tek komutla geri alınması** (`system-changes.toml`, Undo all, `gezik --unregister`, Windows'ta `restore-explorer.reg`; 9b3, 9b4). Rakip notlarında bunun karşılığı geçmiyor.
- **Ölçülmüş hafiflik:** açılış 32–38 ms, boşta 5,1 MB, 100 bin dosya 13,1 MB, 20 sekme +0,4 MB (README, Windows). Boşta belleğin 7 MB'tan 5,1 MB'a inmesinin bir nedeni Windows'ta satır satır çizim (#73): tam pencere çerçeve arabelleği tutulmuyor.
- **Türkçe harf sırasıyla doğal sıralama.**
- **Eşitleme klasöründe çok bilgisayarlı ayarlar** (14f, #112): paylaşılan ve bilgisayara özel katman, çevrimdışı bekleyen değişikliklerin yalnız farkıyla uygulanması, bulut servislerinin çakışan kopyalarını anahtar anahtar inceleme; hiçbir şey kendiliğinden birleşmez.
- **GitHub'dan tema kurma** (14c, #108) yalnız veri olarak: zip diske açılmaz, boyut sınırlı, kurmadan önce önizleme.
- Rakip özetinde "planlı" geçen **şifreli, hesapsız ayar senkronu** henüz yok (17. adım); 14f bir eşitleme klasörü kullanır, şifrelemez.

Bu listeye girmeyenler: **görünüm kuralları** (TC'de var, satır 63), **bırakma yığını** (Fi ve PF'de var, satır 48), **boyut haritası** (PF'de var), **komut paleti** ve **Git** (rakip özeti §3 düzeltmesi).

## Kalan boşluklar

**12–14. adımlardan kalanlar**
- 66'nın eksiği: vurgulama kuralında `where`, etiketlerin meta veri alanı kaydına bağlanması
- macOS ve Linux'ta 12–14. adımlar gerçek cihazda sınanmadı (macOS: `checklists/release-macos.md`, #111; Linux: Docker turu sürüyor)

**Yayın hazırlığı**
- 17 Otomatik güncelleme
- 65'in eksiği: imza, AppImage (zip paketleri #121 ile var; 0.1.0-rc.1 imzasız ön sürüm olarak yayımlandı)

**16 Gelişmiş**
- 12 Arşivin içinde gezinme
- 40 Git
- 44 MTP / telefon
- 53 Adres çubuğunda komut / gömülü terminal

**17 Bulut senkronu**
- Ayarların şifreli, hesapsız senkronu (tablo satırı değil, §3)

**18 Uzak bağlantılar** (açık karar)
- 24 SFTP, FTP, WebDAV, S3

**Planı olmayan kısmi ya da bilerek dışarıda kalanlar**
- 6 Ağ keşfi (9 spec)
- 15 Hardlink oluşturma (7 spec)
- 19 Sabitlenen klasöre simge ve renk (7 spec)
- 26 Zaman damgası ve ACL düzenleme (9 spec)
- 30 `[[commands]]`'ta diğer bölme yer tutucusu, iç içe alt menü
- 56 Listede ve kenar çubuğunda yaylı klasörler (7 spec)
- 58 Yerleştirilebilir paneller (10 spec)
- 14'ün sınırı: Windows'ta başka programların "Klasörde göster"i (9b11 düştü)
