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

op-copy-route = Sao chép { $count } mục | { $source } → { $destination }

op-move-route = Di chuyển { $count } mục | { $source } → { $destination }

op-recycle-route = Thùng rác { $count } mục | { $source }

op-permanent-delete-route = Xóa vĩnh viễn { $count } mục | { $source }

op-gdrive-trash-route = Thùng rác Google Drive { $count } mục | { $source }

op-shortcut-route = Tạo lối tắt { $count } mục | { $source }

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

op-no-destination = Chưa có đích

op-no-source = Chưa có nguồn

apk-installing = Đang cài: { $name } → { $target }

apk-installed = Đã cài: { $name } → { $target }

apk-cancelled = Đã hủy cài { $name } vào { $target }

apk-timeout = Hết thời gian cài { $name } vào { $target }

apk-failed = Cài { $name } vào { $target } thất bại: { $error }

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

transfer-cancelling = Đang hủy

transfer-cancel = Hủy
