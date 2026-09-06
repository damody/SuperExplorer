# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Klasör seçenekleri

dialog-about = SuperExplorer hakkında

dialog-version = Sürüm

dialog-build-date = Derleme tarihi

dialog-author = Yazar

dialog-purpose = Amaç: { $value }

dialog-author-line = Yazar: { $name } — { $bio } · { $date }

dialog-community = Topluluk: { $url }

dialog-plugin-safe-mode-title = Eklenti güvenli modu

dialog-plugin-safe-mode-body = Eklenti güvenli modu açık. Yeniden etkinleştirilecek eklentileri seçin, Uygula veya Tamam’ı seçin, ardından SuperExplorer’ı yeniden başlatın.

dialog-safe-mode-confirm-title = Güvenli mod onay gerektirir

dialog-safe-mode-confirm-aria = Güvenli mod onayı gerekli; Şüpheli paket: { $package }

dialog-suspect-package = Şüpheli paket: { $package }

dialog-interface = Arabirim: { $value }

dialog-operation = İşlem: { $value }

dialog-confirm-reenable = Onayla ve yeniden etkinleştir

dialog-bookmark-action = Yer işareti eylemi

dialog-delete-bookmark = Yer işaretini sil

dialog-delete-bookmark-prompt = “{ $name }” yer işareti silinsin mi?

dialog-delete-bookmark-note = Bu, yer işaretini kaldırır. Diskteki dosyalar silinmez.

dialog-delete-bookmark-folder = Yer işareti klasörünü sil

dialog-delete-bookmark-folder-prompt = “{ $name }” yer işareti klasörü silinsin mi?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Klasör ve içindeki { $count } öğe kaldırılır. Diskteki dosyalar silinmez.
       *[other] Klasör ve içindeki { $count } öğe kaldırılır. Diskteki dosyalar silinmez.
    }

dialog-rename-bookmark-folder = Yer işareti klasörünü yeniden adlandır

dialog-bookmark-library = Kitaplık

dialog-new-shortcut = Yeni kısayol

dialog-new-remote-shortcut = Yeni uzak kısayol

dialog-shortcut-name = Kısayol adı

dialog-shortcut-target = Hedef yol

dialog-create-remote-shortcut = Uzak kısayol oluştur

dialog-cancel-new-shortcut = Yeni kısayolu iptal et

dialog-properties = { $name } - Özellikler

dialog-properties-aria = { $name } özellikleri

dialog-general = Genel

dialog-file-type = Tür: { $value }

dialog-location = Konum: { $value }

dialog-size = Boyut: { $value }

dialog-date-created = Oluşturma tarihi: { $value }

dialog-date-modified = Değiştirilme tarihi: { $value }

dialog-permissions = İzinler: { $mode }

dialog-file-in-use = Dosya kullanımda

dialog-items-in-use = Bazı öğeler kullanımda

dialog-lock-owner-body =
    { $count ->
        [one] Başka bir uygulama kullandığı için Windows, seçili { $count } öğeyi silemiyor.
       *[other] Başka bir uygulama kullandığı için Windows, seçili { $count } öğeyi silemiyor.
    }

dialog-apps-using-file = Dosyayı kullanan uygulamalar

dialog-close-results = Uygulamaları kapatma sonuçları

dialog-new-bookmark = Yeni yer işareti

dialog-edit-bookmark = Yer işaretini düzenle

dialog-name-accelerator = Ad (N)

dialog-location-accelerator = Konum (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Kaydetmede düzenleyiciyi göster (S)

dialog-lua-source = Lua kaynağı (yalnızca salt okunur current_folder)

dialog-folder-path-editable = Klasör yolu (düzenlenebilir)

dialog-file-path-editable = Dosya yolu (düzenlenebilir)

dialog-root = Kök

dialog-folder-options-scrollbar = Klasör Seçenekleri dikey kaydırma çubuğu

dialog-new-bookmark-default = Yeni yer işareti

dialog-new-folder-default = Yeni klasör

dialog-new-shortcut-default = Yeni kısayol

dialog-shortcut-name-invalid = Kısayol adı geçerli klasörde geçerli bir ad olmalıdır.

dialog-shortcut-target-invalid = Geçerli bir hedef yol girin.

dialog-shortcut-window-closed = Ana pencere kapandığı için kısayol oluşturulamadı.

dialog-folder-changed = Geçerli klasör değişti. Yeni kısayol penceresini yeniden açın.

dialog-remote-unavailable = Uzak hizmet şu anda kullanılamıyor.

dialog-shortcut-in-progress = Başka bir kısayol zaten oluşturuluyor.

dialog-allowed = İzin verildi

dialog-not-allowed = İzin verilmedi

dialog-read = Okuma

dialog-write = Yazma

dialog-execute = Yürütme

dialog-owner = Sahip

dialog-group = Grup

dialog-others = Diğerleri

dialog-remote-folder = Uzak klasör

dialog-remote-file = Uzak dosya

dialog-unavailable = Kullanılamıyor

dialog-bytes = { $value } bayt

dialog-extension-author-bio = SuperExplorer ve resmi örnek uzantıların yazarı

dialog-extension-purpose-folder-size = Dosya boyutlarını gösterir ve klasör boyutunu arka planda özyinelemeli olarak toplar.

dialog-extension-purpose-size-map = Geçerli klasördeki her öğenin ne kadar yer kapladığını alan haritası olarak gösterir.

dialog-extension-purpose-tokei = Rust ve tokei ile dosya veya klasörlerdeki kod satırlarını sayar.

dialog-extension-purpose-lua-tokei = Kod satırlarını sayan örnek Lua uzantısı.

dialog-extension-purpose-lock-owner = Şu anda bir dosyayı kilitleyen programı veya hizmeti gösterir.

dialog-extension-purpose-exif = Fotoğraf EXIF çekim verilerinden toplu yeniden adlandırma önerir.

dialog-extension-purpose-7z = 7-Zip arşivlerini gezilebilir sanal klasörler olarak sunar.

dialog-extension-purpose-bulk-folder = Kullanıcının belirttiği şablondan bir kerede birçok klasör oluşturur.

dialog-extension-folder-size = Klasör boyutu sütunu

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Ana kod satırları

dialog-extension-code-lines = Kod satırları

dialog-extension-lock-owner = Kilit sahibi

dialog-extension-exif = EXIF’ten yeniden adlandır

dialog-extension-7z = 7-Zip sanal klasörü

dialog-extension-bulk-folder = Toplu klasör oluşturucu

dialog-git-hash = Git karması
dialog-release-date-line = Yayın tarihi: { $value }
dialog-reset = Sıfırla
dialog-reset-prompt = { $label } sıfırlansın mı? Yalnızca kaydedilmiş durum kaldırılır; geçerli dosyalar değişmez.
dialog-reset-session-label = kaydedilmiş pencereler ve sekmeler
dialog-reset-view-label = kaydedilmiş görünüm ayarları
dialog-reset-quick-access-label = Hızlı erişim sabitlemeleri
dialog-reset-all-label = kayıtlı tüm Explorer durumu
dialog-permanent-delete-prompt = { $count } öğe kalıcı olarak silinsin mi? Bu işlem geri alınamaz.
dialog-gdrive-trash-prompt = { $count } öğe Google Drive çöp kutusuna taşınsın mı? drive.google.com üzerinde 30 gün kurtarılabilir. Bu, Windows Geri Dönüşüm Kutusu değildir.
dialog-permanent-delete-aria = { $count } öğeyi kalıcı olarak sil
dialog-gdrive-trash-aria = { $count } öğeyi Google Drive çöp kutusuna taşı
