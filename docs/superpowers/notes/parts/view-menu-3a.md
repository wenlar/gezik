# view-menu-3a: Görünüm menüsü bölündü (tasarım 3a, "Split toolbar") — notlar

Tasarım: Claude Design projesi "Gezik View Menu", 3a. Dal: `feat/view-menu-3a`.

## Ne yapıldı

- **Araç çubuğu** (`pane.slint`): tek Görünüm düğmesi yerine adres çubuğunun sağında
  `BarButton`'lar (`widgets/view-popups.slint`): **Düzen** (geçerli düzenin simgesi + ▾),
  **Sıralama** (sıralama simgesi + geçerli ölçütün adı + ▾; açıkken vurgulu kenar ve seçim
  rengi), 1 px ayraç, **Önizleme bölmesi** ve **İki bölme** açma/kapama düğmeleri (açıkken
  vurgulu; sağ tık: Sağda/Altta, Yan yana/Üst üste), **⋯**.
- **Açılır pencereler** Gezik'in kendi çizdikleri (`ViewPopup`; tema renkleri, koyu temaya uyar),
  Windows ve macOS'ta da (eskiden orada sistem menüsüydü). popup.rs yerleştirir; Yukarı/Aşağı/
  Enter/Boşluk/Esc çalışır (`menu-step` ile), dışarıya basınca kapanır.
  - **Düzen**: Liste | Izgara | Sütunlar, Simge boyutu Küçük | Orta | Büyük (yalnız ızgarada
    seçilebilir), Düz görünüm anahtarı, altta gerçek kısayollar ("Ctrl+Shift+1 / Ctrl+Shift+2 /
    Ctrl+Shift+3 switch layout"). Seçimden sonra açık kalır, Rust yeniden doldurur.
  - **Sıralama**: Ad, Değiştirme tarihi, Oluşturma tarihi, Tür, Boyut, Etiketler, Puan, Özellik…
    (alana göre sıralıysa alanın adı), ✓ geçerlide; Artan | Azalan; Gruplandır ▸ (geçerli değer)
    Gezik'in kendi menüsünü yanında açar (gruplu görünümde Tümünü daralt/genişlet de orada);
    Önce klasörler.
  - **⋯**: Gizli öğeleri göster (kısayolla), Sistem öğelerini göster (yalnız Windows), Dosya
    uzantılarını göster (= Uzantıları gizle'nin tersi), Klasör boyutlarını hesapla (yalnız
    klasörde); Bu görünümü her yerde kullan (= Tüm klasörlere uygula), Bu klasörü sıfırla /
    görünüm kuralı satırları; iki bölme açıkken Gezinmeyi eşitle, Bölmeleri değiştir, Diğer
    bölmede aynı klasör; Kenar çubuğu ağacında göster, Ayarlar… (kısayolla), Sistem tümleştirmesi….
- **Bölmeler arası**: yan yana iki bölmede adres çubukları arasındaki zincir simgesi artık her
  zaman görünür (kapalıyken soluk) ve tıklanınca eşitlemeyi açar/kapatır.
- **Durum çubuğu**: Geçmiş'in yanına **Bırakma yığını** düğmesi (her zaman; açıkken vurgulu).
- Satırlar `view_popups.rs`'de kurulur ve eski View menüsünün kimliklerini taşır; seçim
  `Menus::run(id, Subject::View)` ile eskisi gibi çalışır. Eski `view_items`, `rule_items`,
  `format_subs`, `Menus::view_menu`, `Stack::is_open` ve `TextButton.lead-icon` silindi.
- Sıralama düğmesinin adı `PaneData.sort-label` (`sync_header` yazar).
- Çeviriler: tr elle (sözlüğe göre: Düzen, Simge boyutu, Sıralama ölçütü); de/es/fr/zh-Hans
  `fuzzy` makine çevirisi. `docs/features.md` ve `docs/configuration.md` yeni yerleri söylüyor
  (README Görünüm menüsünü anlatmıyordu).

## Eski Görünüm öğeleri nereye gitti

| Öğe | Şimdi |
| --- | --- |
| Liste/Izgara/Sütunlar, simge boyutları, Düz görünüm | Düzen |
| Sıralama ölçütleri, Artan/Azalan, Gruplandır ▸, Önce klasörler | Sıralama |
| Önizleme bölmesi / Önizleme altta | Önizleme düğmesi (sağ tık: Sağda/Altta) |
| İki bölme / Bölmeleri üst üste | İki bölme düğmesi (sağ tık) |
| Gezinmeyi eşitle | bölmeler arasındaki zincir + ⋯ (iki bölmede) |
| Bölmeleri değiştir, Diğer bölmede aynı klasör | ⋯ (iki bölmede) + kısayollar |
| Önizlemeyi onaltılık göster, Oynat/Duraklat | önizleme çubuğu, sağ tık menüsü, Boşluk, palet |
| Bırakma yığını | durum çubuğu düğmesi + palet |
| İşlem geçmişi | durum çubuğundaki Geçmiş (bir iş bitince görünür) + palet |
| Uzantılar, gizli, sistem öğeleri, klasör boyutları | ⋯ |
| Tüm klasörlere uygula, Bu klasörü sıfırla, kural satırları | ⋯ |
| Tarih biçimi ▸, Boyut biçimi ▸, Tek tıkla aç, Etiket noktaları | yalnız Ayarlar (hepsi orada: `view.date-format`, `view.size-format`, `view.single-click-open`, `tags.dots`) |
| Kenar çubuğu ağacında göster | ⋯ (bir ayar değil, eylem; palette de var) |
| Ayarlar…, Sistem tümleştirmesi… | ⋯ |

Kısayollar ve komut paleti değişmedi.

## Seçimler

1. **Takas ve Aynı klasör bölücüde değil, ⋯ menüsünde.** Bölmeler arası 5 px; araç çubuğu
   dolgusu (10 px) yalnız bir 20 px düğmeye yer veriyor. Oraya eşitleme zinciri kondu (zaten
   oradaydı); üç düğme listenin üstüne biner. Üst üste bölmelerde zincir yok, ⋯'de var.
2. **macOS menü çubuğundaki Görünüm menüsü olduğu gibi kaldı.** macOS'ta küresel menü
   beklenir; ana arayüz artık araç çubuğu. Menü çubuğu öğeleri kısayol çalıyor, dokunmadım.
3. **Simge boyutu yalnız ızgarada seçilebilir** (eski menü de yalnız ızgarada gösteriyordu);
   listede soluk.
4. **Düzen açılır penceresi seçimden sonra açık kalır**, Sıralama ve ⋯ kapanır (menü gibi).
5. **Panel düğmelerinin sağ tık menüsü ve Gruplandır ▸** Gezik'in kendi menüsü
   (`show-own-menu`), Windows/macOS'ta da: açılır pencerelerle aynı görünsün diye.
6. Tasarımdaki 30 px yükseklik yerine `Theme.control-height` (28 px): adres çubuğuyla aynı.
7. "Show file extensions" ✓ = uzantılar görünüyor; altta yine `view.hide-extensions` değişir.

## Kalanlar / bilinen sınırlar

- Tek bölmede araç çubuğu eskisinden ~130 px geniş; pencerenin en dar hali o kadar büyüdü.
  Dar iki bölmede (400 px altı) sıralama adı ve ayraç gizlenir; sıralama adı yer daralınca
  kısalır (elide).
- Dil değişince sıralama düğmesinin adı bir sonraki gezinme/sıralamaya kadar eski dilde
  kalabilir (`sync_header` o zaman yazar).
- Ekran testi yapılmadı (kullanıcı başındaydı): görünüm, açık/koyu tema, dar pencere ve
  klavye gezinmesi kontrol listelerinde.
