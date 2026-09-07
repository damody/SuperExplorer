# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Sao chép { $count } mục
       *[other] Sao chép { $count } mục
    }

move-items =
    { $count ->
        [one] Di chuyển { $count } mục
       *[other] Di chuyển { $count } mục
    }

recycle-items =
    { $count ->
        [one] Chuyển { $count } mục vào Thùng rác
       *[other] Chuyển { $count } mục vào Thùng rác
    }

permanent-delete-items =
    { $count ->
        [one] Xóa vĩnh viễn { $count } mục
       *[other] Xóa vĩnh viễn { $count } mục
    }

gdrive-trash-items =
    { $count ->
        [one] Chuyển { $count } mục vào thùng rác Google Drive
       *[other] Chuyển { $count } mục vào thùng rác Google Drive
    }

shortcut-items =
    { $count ->
        [one] Tạo lối tắt cho { $count } mục
       *[other] Tạo lối tắt cho { $count } mục
    }

extra-items =
    { $count ->
        [one] { $first } (thêm { $count } mục)
       *[other] { $first } (thêm { $count } mục)
    }

clipboard-source = Nguồn do khay nhớ tạm hệ thống

op-new-folder = Thư mục mới | { $path }

op-new-file = Tệp mới | { $path }

op-rename = Đổi tên | { $from } → { $to }

op-chmod = Đổi quyền | { $path } → { $mode }

op-copy-route = { $action } | { $source } → { $destination }

op-move-route = { $action } | { $source } → { $destination }

op-recycle-route = { $action } | { $source }

op-permanent-delete-route = { $action } | { $source }

op-gdrive-trash-route = { $action } | { $source }

op-shortcut-route = { $action } | { $source }

op-preparing-copy = Đang chuẩn bị sao chép

op-copying = Đang sao chép

op-copy-complete = Đã sao chép xong

op-preparing-move = Đang chuẩn bị di chuyển

op-moving = Đang di chuyển

op-move-complete = Đã di chuyển xong

op-preparing = Đang chuẩn bị

op-processing = Đang xử lý

op-complete = Xong

op-finalizing = Đang hoàn tất

op-progress-items = { $summary } | { $phase } | Tiến độ { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Tiến độ { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Tiến độ { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Xong

op-cancelled = { $summary } | Đã hủy

op-failed = { $summary } | Thất bại: { $error }

op-partial = { $summary } | Một phần: { $succeeded }/{ $total } thành công

op-cancelling = { $summary } | Đang hủy

op-success-route = Thành công | { $route }

op-skipped-route = Đã bỏ qua | { $route }

op-cancelled-route = Đã hủy | { $route }

op-partial-status = Một phần

op-failed-status = Thất bại

op-error-code =  | Mã lỗi { $code }

op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = Chưa có đích

op-no-source = Chưa có nguồn

apk-installing = Đang cài: { $name } → { $target }

apk-installed = Đã cài: { $name } → { $target }

apk-cancelled = Đã hủy cài { $name } vào { $target }

apk-timeout = Hết thời gian cài { $name } vào { $target }

apk-failed = Cài { $name } vào { $target } thất bại: { $error }

apk-check-device = Hãy kiểm tra kết nối thiết bị và APK rồi thử lại
status-no-selection = Chưa chọn mục nào

status-details-unavailable = Không tải được chi tiết

status-item-count =
    { $count ->
        [one] { $count } mục
       *[other] { $count } mục
    }

status-preview-select-one = Chọn một mục để xem trước

status-preview-failed = Không tạo được xem trước tệp này

status-preview-loading = Đang tải xem trước…

status-preview-item-failed = Không tải được mục xem trước

status-preview-select-single = Chọn một mục duy nhất để xem trước

status-no-transfers = Không có thao tác tệp trong phiên này

status-thumbnail-cleared = Đã xóa bộ đệm hình thu nhỏ

status-thumbnail-clear-partial = Không xóa hết bộ đệm hình thu nhỏ; có thể thử lại, duyệt tệp vẫn hoạt động

status-invalid-folder-name = Tên thư mục không hợp lệ. Sửa rồi thử lại.

status-name-conflict = Đã có mục cùng tên.

status-context-menu-unresponsive = Menu ngữ cảnh không phản hồi. Bạn vẫn có thể làm việc.

status-cannot-go-forward = Không thể tiến

status-partial-folders = Một số thư mục không liệt kê được.

status-cannot-list-folder = Không liệt kê được thư mục.

status-cannot-cancel-disconnected = Không hủy được: dịch vụ tệp chưa kết nối.

status-cannot-cancel-operation = Không hủy được thao tác tệp: { $error }

status-cannot-load-folder = Không tải được thư mục. Thử lại.

status-drive-free-of = còn trống { $free } / { $total }

status-no-media = Không có phương tiện

status-disconnected = Đã ngắt kết nối

status-access-denied = Từ chối truy cập

status-capacity-unavailable = Không lấy được dung lượng

status-waiting-file-count = Đang chờ File Count…

status-file-count-limit = Phụ thuộc File Count nên không chạy

status-file-count-over-limit = File Count vượt giới hạn nên không chạy

status-file-count-pending-label = Limit

transfer-upload = Tải lên đích

transfer-download = Tải xuống từ nguồn

transfer-conflict-inspection = Kiểm tra xung đột đích
transfer-local-copy = Sao chép cục bộ
transfer-source-delete = Xóa nguồn sau khi di chuyển
transfer-provider-panic = Lỗi nhà cung cấp truyền
transfer-no-diagnostic = Không có lỗi cơ sở được cung cấp
transfer-cancelling = Đang hủy

transfer-cancel = Hủy

status-details-view = Chế độ xem chi tiết
status-icon-view = Chế độ xem biểu tượng
status-error = Lỗi · 
status-loading = Đang tải · 
status-items-selected = { $count } mục — đã chọn { $selected }
status-operation-progress = Thao tác { $completed }/{ $total } · { $name }
status-search-cancelled = Đã hủy tìm kiếm · 
status-search-error = Lỗi tìm kiếm · 
status-search-fallback = Đang tìm (dự phòng hệ thống tệp; không có chỉ mục) · 
status-search-indexed = Đang tìm (chỉ mục + dự phòng) · 
status-search-partial = Kết quả tìm kiếm một phần · 
status-search-results = Kết quả tìm kiếm · 
status-lock-close-cancelled = Đã hủy đóng ứng dụng.
status-lock-discovery-timeout = Đã hết hạn tìm ứng dụng đang khóa mục.
status-lock-finding = Đang tìm ứng dụng đang dùng mục đã chọn…
status-lock-owners-found = { $count } ứng dụng đang dùng mục đã chọn.
status-lock-partial-close = Một số ứng dụng không đóng. Không tiến trình nào bị buộc thoát.
status-lock-retry-limit = Đã đạt giới hạn thử lại. Mục không bị xóa.
status-lock-retrying = Đang thử lại thao tác xóa…
status-lock-unidentified = Windows không xác định được ứng dụng đang dùng mục này.
transfer-cancel-operation = Hủy thao tác tệp
transfer-open-location = Mở vị trí truyền

status-generic-item = mục

status-operation-queue-failed = The operation could not be queued, but Explorer can continue.
status-folder-options-save-failed = Unable to save Folder Options. Check the session storage and try again.
status-bookmark-unavailable = Bookmark is no longer available.
status-bookmark-delete-window-failed = Unable to open the bookmark delete confirmation window.
status-bookmark-saved = Bookmark saved.
status-bookmark-save-failed = Unable to save the bookmark.
status-bookmark-name-required = Bookmark name and target are required.
status-bookmark-renamed = Bookmark renamed.
status-bookmark-rename-failed = Unable to rename the bookmark.
status-bookmark-editor-window-failed = Unable to open the bookmark editor window.
status-bookmark-manager-window-failed = Unable to open the bookmark manager window.
status-bookmark-folder-editor-window-failed = Unable to open the bookmark folder editor window.
status-bookmark-folder-created = Bookmark folder created.
status-bookmark-folder-save-failed = Unable to save the bookmark folder.
status-bookmark-folder-renamed = Bookmark folder renamed.
status-bookmark-folder-rename-failed = Unable to rename the bookmark folder.
status-bookmark-folder-name-required = Bookmark folder name is required.
status-bookmark-folder-removed = Bookmark folder removed.
status-bookmark-folder-remove-failed = Unable to remove the bookmark folder.
status-bookmark-folder-delete-window-failed = Unable to open the bookmark folder delete confirmation window.
status-bookmark-removed = Bookmark removed.
status-bookmark-remove-failed = Unable to remove the bookmark.
status-bookmark-removal-save-failed = Unable to save the bookmark removal.
status-bookmark-order-updated = Bookmark order updated.
status-bookmark-order-save-failed = Unable to save the bookmark order.
status-bookmark-moved = Bookmark moved.
status-bookmark-move-save-failed = Unable to save the bookmark move.
status-bookmark-backup-copied = Bookmark backup copied to the clipboard.
status-bookmark-backup-failed = Unable to back up bookmarks: { $error }
status-bookmark-imported = Bookmarks imported from the clipboard.
status-bookmark-import-persist-failed = Unable to persist imported bookmarks.
status-bookmark-import-invalid = Clipboard does not contain a valid bookmark backup.
status-bookmark-folder-missing = Unable to open bookmark: the folder no longer exists.
status-bookmark-path-unavailable = Unable to open bookmark: the folder path is unavailable or invalid.
status-bookmark-opened = Bookmark opened.
status-bookmark-open-failed = Unable to open bookmark: { $error }
status-bookmark-file-launcher-unavailable = File launcher is unavailable
status-lua-bookmark-need-folder = Lua bookmarks require a filesystem folder.
status-lua-bookmark-completed = Lua bookmark completed.
status-lua-bookmark-timed-out = Lua bookmark timed out.
status-lua-bookmark-failed = Lua bookmark failed: { $error }
