# Türkçe terim sözlüğü (15e)

Gezik'in arayüzünü Türkçeye çevirirken kullanılacak terimler ve yazım kuralları. Kaynaklar: `crates/gezik-i18n/lang/gezik.pot` (15a + 15b dalındaki 962 metin), `.slint` dosyaları, kısayol eylem adları, bağlam menüsü ve menü çubuğu, `templates/settings.toml`, `docs/features.md`, rakip karşılaştırma notları (`*karsilastirma.md`, bu notlarda zaten "kenar çubuğu", "komut paleti", "etiket", "taşınabilir", "Miller sütunları", "hızlı bakış" kullanılıyor).

Referans: Windows 11 Türkçe arayüzü (Microsoft Language Portal / Microsoft terminoloji koleksiyonu) ve macOS Türkçe Finder. "Win" ve "mac" sütunu bellekten yazıldı; **(doğrulanmalı)** yazanlar emin olmadığım karşılıklar, ekran testinde gerçek sistemden bakılmalı.

**[TARTIŞMALI]** işaretli satırlarda bir seçim yaptım ama karar bakımcının. Tablonun sonunda kısa liste var.

## 1. Yazım kuralları

| Kural | Ne yapıyoruz | Not |
|---|---|---|
| Hitap | Mesajlarda ve açıklamalarda "siz": "Emin misiniz?", "Bir klasör seçin." | Windows ve macOS ikisi de "siz" kullanır. "Sen" yok. |
| Düğme ve menü fiilleri | Emir kipi, tekil, kısa: **Kopyala, Sil, Yeniden adlandır, Aç** | "Kopyalayın" değil. Windows ve macOS menüleri böyle. Uzun açıklama cümlelerinde "-in/-ın" çekimi serbest ("Kaydetmek için Enter'a basın"). |
| Açık/kapalı durum | Ad + "açık/kapalı": "Satır kaydırma, açık" | İngilizce "Wrap lines, on" gibi ekran okuyucu metinleri. |
| Büyük harf | Türkçe **cümle düzeni**: yalnız ilk harf ve özel adlar büyük: "Yeni klasör", "Kapatılan sekmeyi yeniden aç" | **[TARTIŞMALI]** macOS Türkçe menüleri Başlık Düzeni kullanır ("Yeni Klasör"). İngilizce kaynakta macOS menü çubuğu Title Case, bağlam menüleri sentence case; Türkçede ikisini de cümle düzenine indirmeyi öneriyorum (Windows böyle, tutarlı ve kısa). |
| İ/ı | Büyük harfli başlıkları çevirmen kendisi yazar: "SÜRÜCÜLER", "SABİTLENENLER", "ETİKETLER", "ARAMALAR", "BULUT", "KLASÖRLER", "KONUMLAR" | Rust `to_uppercase()` Türkçe değil (`i` → `I`, olması gereken `İ`). Kodda çalışma anında büyük harfe çevrilen metin olmamalı; çeviride "İ" ve "ı" elle yazılır. |
| Üç nokta | İngilizcede `…` varsa Türkçede de `…` (tek karakter U+2026), sona: "Yeniden adlandır…" | Bir pencere/soru açan komutları gösterir. Üç ayrı nokta (`...`) yazılmaz. Yoksa eklenmez. |
| Yer tutucular | `{}`, `{0}`, `{n}`, `{left}`, `{right}` olduğu gibi kalır, sayısı aynı olmalı (`gezik-lang check` denetler). Sıra değişecekse `{}` yerine `{0}`, `{1}` yazılır: "Page {} of {}" → "Sayfa {0} / {1}". | `{{` ve `}}` düz süslü ayraçtır. |
| Yer tutucuya ek | Yer tutucuya doğrudan ek bitiştirilmez (ünlü uyumu bilinemez: "{}'e" mi "{}'a" mı?). Ek, önüne konan bir ada gelir: "Search in \"{}\"…" → "\"{}\" içinde ara…", "Connect to {}" → "Bağlan: {}" ya da "{} sunucusuna bağlan". | En güvenli yol "Etiket: {}" gibi iki nokta kalıbı. |
| Çoğul | Türkçede `nplurals=1` (tablodan): yalnız `msgstr[0]` dolar, sayıdan sonra ad tekil kalır: "{n} öğe", "{n} dosya", "{n} yıldız". | "3 dosyalar" yanlış. |
| Sayılar | Ondalık virgül, binlik nokta: "1,5 MB", "12.480 öğe". Sayı ile birim arasında boşluk. | 15c sayı biçimini yapacak; çeviride elle sayı yazılmaz. |
| Yüzde | İşaret sayının **önünde**: "%45", "%45 çevrildi" | "{} % translated" → "%{} çevrildi". TDK kuralı. |
| Tarih/saat | gg.aa.yyyy, 24 saat: "13.10.2026 14:05" | Biçim koddan gelir (15c). |
| Özel adlar | Gezik, Everything, Spotlight, Finder, 7-Zip, ffmpeg, pdfium, PATH, EXIF, ZIP, PDF çevrilmez. Ek kesme işaretiyle: "Gezik'i", "Gezik'in", "Finder'da", "Spotlight'ta", "Everything'de", "7-Zip'i", "PATH'e" | Ünlü uyumu okunuşa göre ("Everything" → "Everything'de", "Finder" → "Finder'ı"). |
| Explorer | Ürün adı değil, Windows'taki Türkçe adıyla: **Dosya Gezgini** | Windows Türkçe "File Explorer" = "Dosya Gezgini". |
| Kısaltma | Dar sütun başlıklarında bile kelime kesilmez; sığmazsa ipucu (tooltip) uzun hâli verir. | |
| Noktalama | Tam cümle olan açıklamalar noktayla biter (İngilizcede nokta varsa). Düğme ve menüde nokta yok. | |
| Tırnak | Dosya adı tırnağı İngilizcedeki gibi düz `"…"`. | Kaynakla aynı karakter, denetim kolay. |

## 2. Dosya işlemleri

| İngilizce | Türkçe | Not |
|---|---|---|
| Copy | Kopyala | Win/mac aynı. |
| Cut | Kes | Win aynı; macOS'te Finder'da yok (Kopyala + Option ile taşı). |
| Paste | Yapıştır | Win/mac aynı. |
| Paste into folder | Klasöre yapıştır | |
| Paste text as file / image as file | Metni dosya olarak yapıştır / Resmi dosya olarak yapıştır | |
| Copy with folders / Cut with folders | Klasörleriyle kopyala / Klasörleriyle kes | Düz görünümde yol yapısını koruyan kopya. |
| Copy To… / Move To… | Kopyala… / Taşı… → **Şuraya kopyala… / Şuraya taşı…** | Win: "Şuraya kopyala", "Şuraya taşı". |
| Copy here / Move here / Create link here | Buraya kopyala / Buraya taşı / Buraya bağlantı oluştur | Sürükle-bırak menüsü. Win: "Buraya kopyala", "Buraya taşı", "Kısayolları burada oluştur". |
| Move | Taşı | |
| Copy to the Other Pane / Move to the Other Pane | Diğer bölmeye kopyala / Diğer bölmeye taşı | Bkz. "pane". |
| Rename / Rename… | Yeniden adlandır / Yeniden adlandır… | Win aynı; mac "Yeniden Adlandır". |
| Rename items… / Rename {n} item… | Öğeleri yeniden adlandır… / {n} öğeyi yeniden adlandır… | Toplu adlandırma. |
| Old name / New name | Eski ad / Yeni ad | "İsim" değil "ad" (Win "Ad"). |
| Delete | Sil | |
| Delete Permanently… / Delete Immediately… | Kalıcı olarak sil… / Hemen sil… | Win: "Kalıcı olarak sil"; mac: "Hemen Sil…". |
| Move to Trash | Çöp Sepetine taşı (mac) / Geri Dönüşüm Kutusuna taşı (Win) | Kaynakta iki işletim sistemine ayrı metin varsa ayrı çevrilir. Tek metinse bkz. "Trash". |
| Trash (macOS, Linux) | Çöp Sepeti | macOS Türkçe adı. Linux masaüstleri "Çöp" / "Çöp Kutusu" der; tek terim için **Çöp Sepeti**. **[TARTIŞMALI]** Linux için "Çöp Kutusu" daha yaygın. |
| Recycle Bin (Windows) | Geri Dönüşüm Kutusu | Win aynı, özel ad gibi büyük harfle. |
| Empty Trash… / Empty Recycle Bin… | Çöp Sepetini boşalt… / Geri Dönüşüm Kutusunu boşalt… | Ek kesmesiz bitişir (özel ad değil, sistem öğesi adı). |
| Show Trash | Çöp Sepetini göster | |
| Confirm trash | Çöpe atmayı onayla | Ayar adı. |
| Put Back | Geri koy | mac: "Geri Koy" **(doğrulanmalı)**. Win karşılığı "Geri yükle". |
| Restore | Geri yükle | Win aynı. Yedek için de aynı. |
| Original location / Date deleted | Özgün konum / Silinme tarihi | Win: "Özgün konum", "Silinme tarihi". |
| Undo / Redo | Geri al / Yinele | Win aynı; mac "Geri Al" / "Yinele". |
| Undo {} / Redo {} | Geri al: {} / Yinele: {} | Ek bitiştirmemek için iki nokta. |
| Duplicate (verb) | Çoğalt | mac: "Çoğalt". Win'de yok. "Yinelenen" (duplicates) ile karışmasın. |
| New Folder | Yeni klasör | |
| New Folder with Selection | Seçimle yeni klasör | mac: "Seçimle Yeni Klasör" **(doğrulanmalı)**. |
| New file / New from template | Yeni dosya / Şablondan yeni | |
| Name from template | Ad şablondan | |
| Keep Both / Replace / Skip | İkisini de tut / Değiştir / Atla | Çakışma penceresi. Win: "Dosyaların yerini değiştir", "Bu dosyaları atla"; mac: "İkisini de Sakla", "Değiştir", "Atla". **[TARTIŞMALI]** "tut" mu "sakla" mı: "sakla" "gizle" ile karışabildiği için **tut**. |
| If newer | Daha yeniyse | |
| Overwrite | Üzerine yaz | |
| Retry | Yeniden dene | |
| Cancel / Cancel operation | İptal / İşlemi iptal et | Düğme "İptal" (Win/mac), cümle içi "iptal et". |
| Pause / Resume / Stop | Duraklat / Sürdür / Durdur | Win kopyalama penceresi: "Duraklat", "Sürdür". |
| Operation / Operations | İşlem / İşlemler | |
| Operation history | İşlem geçmişi | |
| Verify copies | Kopyaları doğrula | |
| Speed limit / Max speed | Hız sınırı / En yüksek hız | |
| Copy threads | Kopyalama iş parçacığı | Teknik ayar; Microsoft "thread" = "iş parçacığı". |
| Create link / Symbolic link / Junction / Hard link | Bağlantı oluştur / Sembolik bağlantı / Dizin bağlantısı / Sabit bağlantı | Microsoft: "sembolik bağlantı", "sabit bağlantı"; junction için "dizin bağlantısı" **(doğrulanmalı)**. |
| Shortcut (.lnk file) | Kısayol | Windows bu adı kullanır. Klavye kısayolu ile karışmasın diye o her zaman "klavye kısayolu" ya da bağlamla. |
| Make Alias / alias | Takma ad oluştur / takma ad | mac: "Takma Ad Yarat" eski, yenisi "Takma Ad Oluştur" **(doğrulanmalı)**. |
| Copy Path / Copy path as | Yolu kopyala / Yolu farklı kopyala | Win: "Yol olarak kopyala". |
| Full path / UNC path / file:// URL / With forward slashes / Quoted | Tam yol / UNC yolu / file:// URL / Eğik çizgiyle / Tırnaklı | |
| Split file… / Join files | Dosyayı böl… / Dosyaları birleştir | |
| Open / Open with / Open with default app | Aç / Birlikte aç / Varsayılan uygulamayla aç | Win: "Birlikte aç"; mac: "Birlikte Aç". |
| Open in new tab | Yeni sekmede aç | Win aynı. |
| Show in folder / Enclosing Folder | Klasörde göster / Üst klasör | mac "Enclosing Folder" = "Kapsayan Klasör" **(doğrulanmalı)**; "Üst klasör" daha anlaşılır. |
| Get Info / Properties | Bilgi al / Özellikler | mac "Bilgi Al", Win "Özellikler". |
| Show Package Contents | Paket içeriğini göster | mac aynı. |
| Share… | Paylaş… | |
| Eject | Çıkar | Win/mac aynı. Arşiv "extract" için bu sözcük **kullanılmaz** (bkz. 8). |
| Select All / Invert Selection | Tümünü seç / Seçimi tersine çevir | Win: "Tümünü seç", "Seçimi ters çevir". |
| Select by Pattern… / Deselect by Pattern… | Desene göre seç… / Desene göre seçimi kaldır… | |
| Select Same Type / Restore Selection | Aynı türü seç / Seçimi geri getir | |
| Selected / {n} selected | Seçili / {n} seçili | Win durum çubuğu "3 öğe seçildi"; kısa hâli "seçili". |
| Refresh | Yenile | |

## 3. Öğeler, birimler, özellikler

| İngilizce | Türkçe | Not |
|---|---|---|
| item / items | öğe | Win: "öğe". "{n} öğe". |
| file / files | dosya | |
| folder / folders | klasör | "Dizin" arayüzde yok; yalnız teknik metinde. |
| drive / drives | sürücü / sürücüler | Kenar çubuğu başlığı "SÜRÜCÜLER". |
| path | yol | |
| extension | uzantı | Win: "Dosya adı uzantıları". |
| Hide extensions | Uzantıları gizle | |
| hidden items / Show hidden items | gizli öğeler / Gizli öğeleri göster | Win aynı. |
| system items / Show system items | sistem öğeleri / Sistem öğelerini göster | |
| Name / Size / Type / Modified / Created | Ad / Boyut / Tür / Değiştirme tarihi / Oluşturma tarihi | Win sütun adları. Dar sütunda "Değiştirilme" yerine tam ad. |
| Date | Tarih | |
| Owner | Sahip | |
| bytes / KB / MB / GB / TB | bayt / KB / MB / GB / TB | "byte" değil "bayt" (Win). |
| Binary (1 KB = 1024 bytes) / Decimal (1 kB = 1000 bytes) | İkili (1 KB = 1024 bayt) / Ondalık (1 kB = 1000 bayt) | |
| {} MB/s | {} MB/sn | |
| Folder sizes / Calculate folder sizes | Klasör boyutları / Klasör boyutlarını hesapla | |
| Size at least… / at most… | En az … boyut / En çok … boyut | "Boyut en az…" kalıbı da olur. |
| Relative / Short / ISO / System (date formats) | Göreli / Kısa / ISO / Sistem | "2 saat önce", "dün". |
| Encoding / Line endings / Byte order mark (BOM) | Kodlama / Satır sonları / Bayt sırası işareti (BOM) | Microsoft terimleri. |
| Hex / Octal / Text | Onaltılık / Sekizlik / Metin | "Show as Hex" → "Onaltılık göster". |
| Wrap lines | Satırları kaydır | Not Defteri "Sözcük kaydırma"; burada satır. |
| Metadata | Meta veri | Microsoft yazımı ayrı: "meta veri". |
| Permissions / Sharing & Permissions | İzinler / Paylaşım ve İzinler | mac aynı. |
| Read / Write / Execute | Okuma / Yazma / Çalıştırma | İzin adları. |
| Everyone | Herkes | |
| Loading… / Looking… | Yükleniyor… / Aranıyor… | |
| Nothing matches | Eşleşen yok | |
| (not set) | (ayarlanmadı) | |
| default / (default) | varsayılan / (varsayılan) | |
| None / All / Any | Hiçbiri / Tümü / Herhangi | "Any rating" → "Herhangi bir puan". |
| On / Off | Açık / Kapalı | |

## 4. Görünümler ve pencere

| İngilizce | Türkçe | Not |
|---|---|---|
| View | Görünüm | Menü adı Win "Görünüm", mac "Görüntü" **(doğrulanmalı)**; ikisinde de **Görünüm**. |
| List / as List | Liste / Liste olarak | mac "Liste olarak". |
| Details | Ayrıntılar | Win görünüm adı. |
| Grid / as Grid | Izgara / Izgara olarak | Win'de "Simgeler" görünümü, mac'te "Simgeler olarak". **[TARTIŞMALI]** "Izgara" mı "Simgeler" mi: Gezik ızgara + küçük resim, "Izgara" seçildi. |
| Small / Medium / Large icons | Küçük / Orta / Büyük simgeler | Win aynı. |
| Columns / as Columns | Sütunlar / Sütunlar olarak | mac aynı. |
| Miller columns | Miller sütunları | Özel ad; notlarda zaten böyle. |
| Flat view | Düz görünüm | Alt klasörlerin hepsi tek liste. |
| Two panes / dual pane | İki bölme | **[TARTIŞMALI]** Microsoft "pane" = "bölme" (Önizleme bölmesi, Gezinti bölmesi); Total Commander kullanıcıları "çift panel" der. Notlarda ikisi de geçiyor. Win terimi seçildi: **bölme**. |
| pane / Left pane / Right pane / Other pane | bölme / Sol bölme / Sağ bölme / Diğer bölme | |
| Swap panes / Switch to the other pane | Bölmeleri değiştir / Diğer bölmeye geç | |
| Same folder in the other pane | Diğer bölmede aynı klasör | |
| Sync browsing | Gezinmeyi eşitle | İki bölme birlikte gezinir. |
| Sidebar | Kenar çubuğu | mac "Kenar Çubuğu"; Win "Gezinti bölmesi". Notlarda "kenar çubuğu". |
| Pin to sidebar / Unpin from sidebar / Pinned | Kenar çubuğuna sabitle / Kenar çubuğundan kaldır / Sabitlenenler | Win "Hızlı erişime sabitle", "Sabitlemeyi kaldır". |
| Places / Locations | Yerler / Konumlar | mac kenar çubuğu başlığı "Konumlar". |
| Tree / Show in sidebar tree / Tree follow | Ağaç / Kenar çubuğu ağacında göster / Ağaç izlesin | |
| Expand / Collapse (All Groups) | Genişlet / Daralt (tüm grupları) | Win: "Tüm grupları genişlet/daralt". |
| Tab / Tabs | Sekme / Sekmeler | |
| New tab / Close tab / Close other tabs | Yeni sekme / Sekmeyi kapat / Diğer sekmeleri kapat | |
| Reopen closed tab | Kapatılan sekmeyi yeniden aç | |
| Lock tab / Unlock tab / Locked | Sekmeyi kilitle / Sekme kilidini aç / Kilitli | |
| Move tab to new window | Sekmeyi yeni pencereye taşı | |
| Show next/previous tab / Show all tabs… | Sonraki/önceki sekmeyi göster / Tüm sekmeleri göster… | |
| Tab set / Save tabs as… / Open tab set | Sekme seti / Sekmeleri farklı kaydet… / Sekme setini aç | **[TARTIŞMALI]** "set" mi "küme" mi "grup" mu; "grup" sekme gruplarıyla karışır, "küme" resmi; **sekme seti** seçildi. |
| Window / New window | Pencere / Yeni pencere | |
| Minimize / Zoom In / Zoom Out / Actual Size | Küçült / Yakınlaştır / Uzaklaştır / Gerçek boyut | mac "Küçült". |
| Layout / Layouts | Düzen / Düzenler | |
| Preview / Preview pane | Önizleme / Önizleme bölmesi | Win aynı. |
| Quick Look | Hızlı bakış | mac "Hızlı Bakış" (özel ad gibi). macOS'ta büyük harfle bırakmak düşünülebilir; cümle düzeni seçildi. |
| Select a file to preview | Önizlemek için bir dosya seçin | Win: "Önizlemek istediğiniz dosyayı seçin". |
| Thumbnails | Küçük resimler | Win terimi. |
| Address / address bar | Adres / Adres çubuğu | |
| Status bar / Toolbar / Menu bar | Durum çubuğu / Araç çubuğu / Menü çubuğu | |
| Context menu | Bağlam menüsü | Microsoft terimi; kullanıcı metninde "sağ tık menüsü" de anlaşılır. |
| Back / Forward / Up | Geri / İleri / Yukarı | Win: "Geri", "İleri", "Yukarı". |
| Go to Folder… | Klasöre git… | mac aynı. |
| Folder history / Clear folder history | Klasör geçmişi / Klasör geçmişini temizle | |
| Recent | Son kullanılanlar | mac "Son Kullanılanlar" **(doğrulanmalı)**. |
| Home | Giriş klasörü | Win 11 "Giriş" (Home sayfası), mac kullanıcı adını gösterir. **[TARTIŞMALI]** "Ana klasör" da olur. |
| This PC | Bu bilgisayar | Win aynı. |
| Desktop / Documents / Downloads / Pictures / Music / Videos | Masaüstü / Belgeler / İndirilenler / Resimler / Müzik / Videolar | Win adları; mac "Filmler" der, Gezik Videolar. |
| Group by / Group / Ungroup / No group | Grupla / Grup / Grubu çöz / Grup yok | Win "Gruplandırma ölçütü". Menüde kısa "Grupla". |
| Sort / Sort by / Ascending / Descending | Sırala / Sıralama ölçütü / Artan / Azalan | Win aynı. |
| Folders first | Önce klasörler | |
| Density / compact / comfortable | Yoğunluk / Sıkışık / Rahat | **[TARTIŞMALI]** Win 11 "Compact view" = "Sıkıştırılmış görünüm" **(doğrulanmalı)**; "sıkıştırılmış" arşiv anlamıyla çakışır. "Sıkışık" seçildi; alternatif "Sık". |
| Reduce motion | Hareketi azalt | mac "Hareketi Azalt". |
| Single-click to open | Tek tıklamayla aç | |
| Double click | Çift tıklama | |
| Show / Hide | Göster / Gizle | "Show Size" → "Boyutu göster". |
| More columns… / Reset columns | Diğer sütunlar… / Sütunları sıfırla | Win: "Diğer…". |
| Folder views / View rules / Reset this folder | Klasör görünümleri / Görünüm kuralları / Bu klasörü sıfırla | |
| Apply to all folders | Tüm klasörlere uygula | Win aynı. |

## 5. Etiketler, puan, yorum

| İngilizce | Türkçe | Not |
|---|---|---|
| Tag / Tags | Etiket / Etiketler | mac "Etiketler". |
| New tag… / Clear tags / Toggle tag | Yeni etiket… / Etiketleri temizle / Etiketi aç/kapat | |
| Tag dots | Etiket noktaları | |
| Clean up tag store… | Etiket deposunu temizle… | |
| Rating / {n} star | Puan / {n} yıldız | **[TARTIŞMALI]** Win "Derecelendirme"; kısa ve günlük olan **Puan** seçildi. |
| Comment | Yorum | **[TARTIŞMALI]** Win özellik adı "Açıklamalar", mac Bilgi penceresi "Yorumlar". "Yorum" seçildi. |
| Highlight rules | Vurgu kuralları | |
| Color / No color | Renk / Renk yok | |
| Red / Orange / Yellow / Green / Blue / Purple / Gray | Kırmızı / Turuncu / Sarı / Yeşil / Mavi / Mor / Gri | mac etiket renkleri. |
| Bulk tagging | Toplu etiketleme | |
| Apply to enclosed items… | İçerdeki öğelere uygula… | mac aynı kalıp. |

## 6. Arama ve süzme

| İngilizce | Türkçe | Not |
|---|---|---|
| Search (verb / noun) | Ara / Arama | |
| Search this folder / everywhere | Bu klasörde ara / Her yerde ara | |
| Search subfolders | Alt klasörlerde ara | |
| Find / Find… | Bul / Bul… | |
| Saved search / Searches | Kayıtlı arama / Aramalar | mac "Akıllı Klasör" der; Gezik kendi adını kullanır. |
| Save search… | Aramayı kaydet… | |
| Filter / Filters / Clear filters | Filtrele / Filtreler / Filtreleri temizle | **[TARTIŞMALI]** Microsoft terimi seçildi; öz Türkçe "Süz / Süzgeçler" daha az bilinir. |
| Saved filter | Kayıtlı filtre | Yukarıdaki karara bağlı. |
| Match case / Ignore case | Büyük/küçük harf duyarlı / Büyük/küçük harfi yoksay | Win Not Defteri "Büyük/küçük harf duyarlı". |
| Regular expression / Regex | Normal ifade | **[TARTIŞMALI]** Microsoft "normal ifade", yazılımcılar "düzenli ifade" der. Microsoft terimi seçildi. |
| Content / Text in files | İçerik / Dosyalardaki metin | |
| Max results | En fazla sonuç | |
| Suggestions | Öneriler | |
| Command palette | Komut paleti | Notlarda böyle. |
| Quick open | Hızlı aç | |
| No commands match | Eşleşen komut yok | |

## 7. Araçlar

| İngilizce | Türkçe | Not |
|---|---|---|
| Tools | Araçlar | |
| Checksum / Calculate checksums… / Verify checksums | Sağlama toplamı / Sağlama toplamlarını hesapla… / Sağlama toplamlarını doğrula | Microsoft "checksum" = "sağlama toplamı". |
| Hash | Özet (hash) | **[TARTIŞMALI]** Microsoft "karma"; yaygın kullanım "hash". Arayüzde algoritma adları (SHA-256, MD5) çevrilmez; "hash" sözcüğü gerekmedikçe "sağlama toplamı" denir. |
| Compare / Diff / Folder diff | Karşılaştır / Fark / Klasör farkı | |
| Differs / The same / Identical | Farklı / Aynı / Özdeş | |
| Select differences | Farkları seç | |
| Ignore whitespace | Boşlukları yoksay | |
| Sync / Sync profile | Eşitle / Eşitleme profili | **[TARTIŞMALI]** Microsoft "eşitle"; günlük dilde "senkronize et". Microsoft seçildi. |
| One-way / two-way sync | Tek yönlü / İki yönlü eşitleme | |
| Sync and Backup | Eşitleme ve yedekleme | |
| Duplicates / Find duplicates | Yinelenenler / Yinelenenleri bul | "Kopyalar" "copy" ile karışır. |
| Keep this one, select its copies | Bunu tut, kopyalarını seç | |
| Select all but the newest/oldest in each group | Her grupta en yeni/en eski dışındakileri seç | |
| Size map | Boyut haritası | Treemap. |
| Drop Stack | Bırakma yığını | **[TARTIŞMALI]** Gezik'e özgü; "Toplama rafı" ya da "Raf" da olur. |
| Templates | Şablonlar | |
| Open terminal here / as administrator | Burada terminal aç / Yönetici olarak terminal aç | Win "Terminalde aç", "Yönetici olarak çalıştır". |
| Export list… | Listeyi dışa aktar… | |

## 8. Arşiv, dönüştürme, PDF

| İngilizce | Türkçe | Not |
|---|---|---|
| Archive / Archives | Arşiv / Arşivler | |
| Compress / Compress… / Add to archive | Sıkıştır / Sıkıştır… / Arşive ekle | Win "ZIP dosyasına sıkıştır". |
| Compress to "{}" | "{}" olarak sıkıştır | |
| Add to existing archive… | Var olan arşive ekle… | |
| Extract here / Extract to… / Extract to "{}" | Buraya ayıkla / Şuraya ayıkla… / "{}" klasörüne ayıkla | **[TARTIŞMALI]** Win "Tümünü ayıkla"; 7-Zip/WinRAR Türkçe "çıkart". "Çıkar" sürücü çıkarmayla (Eject) çakıştığı için Windows terimi **ayıkla** seçildi. |
| Extract each to its own folder | Her birini kendi klasörüne ayıkla | |
| Password / Encrypt file names | Parola / Dosya adlarını şifrele | Win/mac "parola"; "şifre" kullanılmaz (şifreleme ile karışır). |
| Level / Format / Split (volumes) | Düzey / Biçim / Bölümle | "Sıkıştırma düzeyi" (Microsoft). |
| Convert / Convert… | Dönüştür / Dönüştür… | |
| Resize / Width / Height / Longest side / Never enlarge | Yeniden boyutlandır / Genişlik / Yükseklik / Uzun kenar / Asla büyütme | |
| Quality / Resolution / {} dpi | Kalite / Çözünürlük / {} dpi | |
| Rotate by EXIF | EXIF'e göre döndür | |
| Remove metadata (incl. location) | Meta verileri kaldır (konum dahil) | |
| Background (#rrggbb) | Arka plan | |
| Replace originals / Subfolder "converted" | Özgün dosyaların yerine yaz / "converted" alt klasörü | Klasör adı İngilizce kalır mı? **[TARTIŞMALI]** ad koddan geliyorsa kalır. |
| Images to PDF… | Resimlerden PDF… | |
| Merge / Merge order | Birleştir / Birleştirme sırası | |
| Page / Pages / Page {} of {} | Sayfa / Sayfalar / Sayfa {0} / {1} | |
| Go to page… / Next page / Previous page | Sayfaya git… / Sonraki sayfa / Önceki sayfa | |
| Ranges / Each page / e.g. 1-3, 5, 8- | Aralıklar / Her sayfa / örn. 1-3, 5, 8- | |
| Margin / Fit / Letter | Kenar boşluğu / Sığdır / Letter | Kâğıt boyutu adları (A4, Letter) çevrilmez. |
| Add text | Metin ekle | |
| Download ffmpeg… / pdfium… | ffmpeg'i indir… / pdfium'u indir… | |

## 9. Medya

| İngilizce | Türkçe | Not |
|---|---|---|
| Play / Pause / Play/Pause | Oynat / Duraklat / Oynat/Duraklat | Win Medya Oynatıcı aynı. |
| Mute / Unmute / Volume | Sesi kapat / Sesi aç / Ses düzeyi | |
| Forward 10 seconds / Back 10 seconds | 10 saniye ileri / 10 saniye geri | |
| Audio / Video / Image | Ses / Video / Resim | Win "Resim"; mac "Görüntü". "Resim" seçildi (kenar çubuğundaki "Resimler" ile aynı kök). |
| Max fps | En fazla kare/sn | |
| Picture size | Resim boyutu | |

## 10. Ayarlar

| İngilizce | Türkçe | Not |
|---|---|---|
| Settings / Settings… | Ayarlar / Ayarlar… | Win ve mac (Ventura'dan beri) "Ayarlar". |
| General / Appearance / Advanced | Genel / Görünüm / Gelişmiş | Appearance Win "Görünüm", mac "Görünüm". View ile aynı sözcük; bağlam ayırır. |
| Theme / Themes / Get theme… | Tema / Temalar / Tema edin… | |
| Light / Dark / Auto | Açık / Koyu / Otomatik | Win "Açık/Koyu". |
| Language / System language | Dil / Sistem dili | |
| Machine translated / Not reviewed / {} % reviewed | Makine çevirisi / Gözden geçirilmedi / %{} gözden geçirildi | |
| Pseudo (for translators) | Sahte dil (çevirmenler için) | |
| Keyboard / Keyboard shortcuts… | Klavye / Klavye kısayolları… | |
| Shortcut / Set key… / Press keys… | Kısayol / Tuş ata… / Tuşlara basın… | |
| Global shortcut / Hotkey | Genel kısayol | **[TARTIŞMALI]** "Sistem geneli kısayol" daha açık ama uzun. |
| Command / Action | Komut / Eylem | |
| Portable mode | Taşınabilir mod | Notlarda böyle. |
| Keep settings in a folder / Sync folder | Ayarları bir klasörde tut / Eşitleme klasörü | Bulut klasörüyle ayarları paylaşma. |
| Use the settings in this folder / Stop keeping settings there | Bu klasördeki ayarları kullan / Ayarları orada tutmayı bırak | |
| Import / Export (settings) | İçe aktar / Dışa aktar | Microsoft terimi. |
| Backup / Restore backup | Yedek / Yedeği geri yükle | |
| Reset to default | Varsayılana sıfırla | |
| Edit in settings.toml | settings.toml'da düzenle | Dosya adı çevrilmez. |
| Open settings folder / config folder / data folder | Ayarlar klasörünü aç / Yapılandırma klasörünü aç / Veri klasörünü aç | |
| System integration… | Sistem tümleştirmesi… | Microsoft "integration" = "tümleştirme". |
| Make Gezik the default file manager | Gezik'i varsayılan dosya yöneticisi yap | |
| Restore the system file manager | Sistemin dosya yöneticisini geri getir | |
| Start Gezik at login | Oturum açılınca Gezik'i başlat | Win "Oturum açıldığında başlat"; mac "Giriş Öğeleri". |
| Tray / tray icon | Bildirim alanı simgesi | **[TARTIŞMALI]** Win "bildirim alanı"; yaygın "sistem tepsisi"; mac'te menü çubuğu. Tek metinse "Tepsi simgesi" kısa çözüm. |
| Single instance | Tek örnek | |
| Add gezik to PATH | gezik'i PATH'e ekle | Komut adı küçük harf kalır. |
| Check for updates | Güncellemeleri denetle | **[TARTIŞMALI]** Win "Güncelleştirmeleri denetle"; günlük "güncelleme". "Güncelleme" seçildi. |
| Install / Installed / Not installed / Download | Yükle / Yüklü / Yüklü değil / İndir | **[TARTIŞMALI]** Win "yükle", günlük "kur". Win seçildi. |
| Offer to download 7-Zip, ffmpeg and pdfium when needed | Gerektiğinde 7-Zip, ffmpeg ve pdfium'u indirmeyi öner | |
| Free up space / Always keep on this device | Yer aç / Her zaman bu cihazda tut | Win OneDrive terimleri. |
| Cloud / Connect to server… / Disconnect | Bulut / Sunucuya bağlan… / Bağlantıyı kes | |
| About Gezik / Quit Gezik / Hide Gezik / Hide others / Show all | Gezik hakkında / Gezik'ten çık / Gezik'i gizle / Diğerlerini gizle / Tümünü göster | mac uygulama menüsü kalıpları. |
| Services | Servisler | mac "Servisler". |
| Open source page | Kaynak kodu sayfasını aç | |
| Search settings | Ayarlarda ara | |

## 11. Tuş adları

Windows ve Linux'ta tuşlar Türkçe klavyedeki etiketle yazılır; çoğu klavyede etiket İngilizcedir, bu yüzden kısaltmalar çevrilmez. Kısayol gösterimi `Ctrl+Shift+N` biçiminde, artı işaretiyle.

| İngilizce | Türkçe | Not |
|---|---|---|
| Ctrl / Alt / Shift / AltGr | Ctrl / Alt / Shift / AltGr | Microsoft Türkçe de olduğu gibi bırakır. |
| Enter | Enter | Cümle içinde "Enter tuşu", ek: "Enter'a basın". |
| Esc / Escape | Esc | Kaynakta "Escape" geçen yerde de "Esc". |
| Tab | Sekme tuşu (Tab) | Kısayol gösteriminde "Tab"; cümle içinde "Sekme tuşu". |
| Space | Ara çubuğu | Microsoft "Ara Çubuğu". Kısayol gösteriminde "Boşluk" daha kısa; **[TARTIŞMALI]**. |
| Backspace | Geri tuşu (Backspace) | Gösterimde "Backspace". |
| Delete / Del | Delete / Del | |
| Home / End / Page Up / Page Down / Insert | Aynı | |
| Arrow keys / Up / Down / Left / Right | Ok tuşları / Yukarı / Aşağı / Sol / Sağ | |
| F1–F12 | F1–F12 | |
| Cmd (⌘) | Komut (⌘) | Apple Türkçe "Komut". Gösterimde yalnız simge: "⌘⇧N". |
| Option (⌥) | Seçenek (⌥) | Apple Türkçe "Seçenek". |
| Control (⌃) | Kontrol (⌃) | Apple Türkçe. |
| Shift (⇧) | Shift (⇧) | Apple de "Shift". |
| Return | Return | mac klavyesindeki ad. |
| Fn | Fn | |

## 12. Tartışmalı seçimler (özet)

1. Büyük harf: macOS menülerinde Başlık Düzeni yerine her yerde cümle düzeni.
2. Trash: Linux'ta "Çöp Sepeti" (macOS) mi "Çöp Kutusu" mu.
3. Çakışma: "İkisini de tut" mu "İkisini de sakla" mı.
4. Grid: "Izgara" mı "Simgeler" mi.
5. Pane: "bölme" (Microsoft) mı "panel" (Total Commander dünyası) mı.
6. Tab set: "sekme seti" / "sekme kümesi".
7. Home: "Giriş klasörü" mü "Ana klasör" mü.
8. Density compact: "Sıkışık" mı "Sık" mı.
9. Rating: "Puan" mı "Derecelendirme" mi.
10. Comment: "Yorum" mu "Açıklama" mı.
11. Filter: "Filtre/Filtrele" (öneri) mi "Süzgeç/Süz" mü.
12. Regex: "Normal ifade" mi "Düzenli ifade" mi.
13. Hash: "özet" / "karma" / "hash".
14. Sync: "Eşitle" mi "Senkronize et" mi.
15. Drop Stack: "Bırakma yığını" mı "Raf" mı.
16. Extract: "Ayıkla" (Windows) mı "Çıkar/Çıkart" (7-Zip) mı.
17. "converted" alt klasör adı çevrilsin mi.
18. Global shortcut: "Genel kısayol" mu "Sistem geneli kısayol" mu.
19. Tray: "Bildirim alanı" / "Sistem tepsisi" / "Tepsi".
20. Update: "güncelleme" mi "güncelleştirme" mi.
21. Install: "Yükle" mi "Kur" mu.
22. Space tuşu: "Ara çubuğu" mu "Boşluk" mu.
