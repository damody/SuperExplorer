# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] { $count } öğeyi kopyala
       *[other] { $count } öğeyi kopyala
    }

move-items =
    { $count ->
        [one] { $count } öğeyi taşı
       *[other] { $count } öğeyi taşı
    }

recycle-items =
    { $count ->
        [one] { $count } öğeyi Geri Dönüşüm Kutusu’na taşı
       *[other] { $count } öğeyi Geri Dönüşüm Kutusu’na taşı
    }

permanent-delete-items =
    { $count ->
        [one] { $count } öğeyi kalıcı olarak sil
       *[other] { $count } öğeyi kalıcı olarak sil
    }

gdrive-trash-items =
    { $count ->
        [one] { $count } öğeyi Google Drive çöp kutusuna taşı
       *[other] { $count } öğeyi Google Drive çöp kutusuna taşı
    }

shortcut-items =
    { $count ->
        [one] { $count } öğe için kısayol oluştur
       *[other] { $count } öğe için kısayol oluştur
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } öğe daha)
       *[other] { $first } ({ $count } öğe daha)
    }

clipboard-source = Kaynak sistem panosu tarafından sağlandı

op-new-folder = Yeni klasör | { $path }

op-new-file = Yeni dosya | { $path }

op-rename = Yeniden adlandır | { $from } → { $to }

op-chmod = İzinleri değiştir | { $path } → { $mode }

op-copy-route = { $count } öğeyi kopyala | { $source } → { $destination }

op-move-route = { $count } öğeyi taşı | { $source } → { $destination }

op-recycle-route = Geri Dönüşüm Kutusu { $count } öğe | { $source }

op-permanent-delete-route = Kalıcı sil { $count } öğe | { $source }

op-gdrive-trash-route = Google Drive çöp kutusu { $count } öğe | { $source }

op-shortcut-route = Kısayol { $count } öğe | { $source }

op-preparing-copy = Kopyalama hazırlanıyor

op-copying = Kopyalanıyor

op-copy-complete = Kopyalama tamamlandı

op-preparing-move = Taşıma hazırlanıyor

op-moving = Taşınıyor

op-move-complete = Taşıma tamamlandı

op-preparing = Hazırlanıyor

op-processing = İşleniyor

op-complete = Bitti

op-finalizing = Sonlandırılıyor

op-progress-items = { $summary } | { $phase } | İlerleme { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | İlerleme { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | İlerleme { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Bitti

op-cancelled = { $summary } | İptal edildi

op-failed = { $summary } | Başarısız: { $error }

op-partial = { $summary } | Kısmi: { $succeeded }/{ $total } başarılı

op-cancelling = { $summary } | İptal ediliyor

op-success-route = Başarılı | { $route }

op-skipped-route = Atlandı | { $route }

op-cancelled-route = İptal edildi | { $route }

op-partial-status = Kısmi

op-failed-status = Başarısız

op-error-code =  | Hata kodu { $code }

op-no-destination = Hedef sağlanmadı

op-no-source = Kaynak sağlanmadı

apk-installing = Yükleniyor: { $name } → { $target }

apk-installed = Yüklendi: { $name } → { $target }

apk-cancelled = { $name } öğesinin { $target } konumuna yüklenmesi iptal edildi

apk-timeout = { $name } öğesinin { $target } konumuna yüklenmesi zaman aşımına uğradı

apk-failed = { $name } öğesinin { $target } konumuna yüklenmesi başarısız: { $error }

status-no-selection = Hiçbir öğe seçilmedi

status-details-unavailable = Ayrıntılar yüklenemedi

status-item-count =
    { $count ->
        [one] { $count } öğe
       *[other] { $count } öğe
    }

status-preview-select-one = Önizlemek için bir öğe seçin

status-preview-failed = Bu dosya için önizleme oluşturulamadı

status-preview-loading = Önizleme yükleniyor…

status-preview-item-failed = Önizleme öğesi yüklenemedi

status-preview-select-single = Önizlemek için tek bir öğe seçin

status-no-transfers = Bu oturumda dosya işlemi yok

status-thumbnail-cleared = Küçük resim önbelleği temizlendi

status-thumbnail-clear-partial = Küçük resim önbelleği tam olarak temizlenemedi; yeniden deneyebilirsiniz, göz atma çalışmaya devam eder

status-invalid-folder-name = Geçersiz klasör adı. Düzeltip yeniden deneyin.

status-name-conflict = Bu ada sahip bir öğe zaten var.

status-context-menu-unresponsive = Bağlam menüsü yanıt vermedi. Çalışmaya devam edebilirsiniz.

status-cannot-go-forward = İleri gidilemiyor

status-partial-folders = Bazı klasörler listelenemedi.

status-cannot-list-folder = Klasör listelenemedi.

status-cannot-cancel-disconnected = İptal edilemiyor: dosya hizmeti bağlı değil.

status-cannot-cancel-operation = Dosya işlemi iptal edilemedi: { $error }

status-cannot-load-folder = Klasör yüklenemedi. Yeniden deneyin.

status-drive-free-of = { $total } içinden { $free } boş

status-no-media = Ortam yok

status-disconnected = Bağlantı kesildi

status-access-denied = Erişim reddedildi

status-capacity-unavailable = Kapasite kullanılamıyor

status-waiting-file-count = File Count bekleniyor…

status-file-count-limit = File Count’a bağlı olduğu için başlatılmadı

status-file-count-over-limit = File Count sınırı aştığı için başlatılmadı

status-file-count-pending-label = Limit

transfer-upload = Hedefe karşıya yükleme

transfer-download = Kaynaktan indirme

transfer-cancelling = İptal ediliyor

transfer-cancel = İptal

status-details-view = Ayrıntılar görünümü
status-icon-view = Simge görünümü
status-error = Hata · 
status-loading = Yükleniyor · 
status-items-selected = { $count } öğe — { $selected } seçili
status-operation-progress = İşlem { $completed }/{ $total } · { $name }
status-search-cancelled = Arama iptal edildi · 
status-search-error = Arama hatası · 
status-search-fallback = Aranıyor (dosya sistemi yedek yolu; dizin yok) · 
status-search-indexed = Aranıyor (dizin + yedek yol) · 
status-search-partial = Kısmi arama sonuçları · 
status-search-results = Arama sonuçları · 
status-lock-close-cancelled = Uygulamaların kapatılması iptal edildi.
status-lock-discovery-timeout = Kilit sahibi keşfi zaman aşımına ulaştı.
status-lock-finding = Seçili öğeyi kullanan uygulamalar aranıyor…
status-lock-owners-found = { $count } uygulama seçili öğeyi kullanıyor.
status-lock-partial-close = Bazı uygulamalar kapanmadı. Hiçbir işlem zorla sonlandırılmadı.
status-lock-retry-limit = Yeniden deneme sınırına ulaşıldı. Öğe silinmedi.
status-lock-retrying = Silme işlemi yeniden deneniyor…
status-lock-unidentified = Windows bu öğeyi kullanan uygulamayı tanımlayamadı.
transfer-cancel-operation = Dosya işlemini iptal et
transfer-open-location = Aktarım konumunu aç

status-generic-item = öğe
