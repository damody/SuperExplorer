# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Tùy chọn thư mục

dialog-about = Giới thiệu SuperExplorer

dialog-version = Phiên bản

dialog-build-date = Ngày biên dịch

dialog-author = Tác giả

dialog-purpose = Mục đích: { $value }

dialog-author-line = Tác giả: { $name } — { $bio } · { $date }

dialog-community = Cộng đồng: { $url }

dialog-plugin-safe-mode-title = Chế độ an toàn plugin

dialog-plugin-safe-mode-body = Chế độ an toàn plugin đang bật. Chọn plugin để bật lại, chọn Áp dụng hoặc OK, rồi khởi động lại SuperExplorer.

dialog-safe-mode-confirm-title = Chế độ an toàn cần xác nhận

dialog-safe-mode-confirm-aria = Cần xác nhận chế độ an toàn; Gói đáng ngờ: { $package }

dialog-suspect-package = Gói đáng ngờ: { $package }

dialog-interface = Giao diện: { $value }

dialog-operation = Thao tác: { $value }

dialog-confirm-reenable = Xác nhận và bật lại

dialog-bookmark-action = Thao tác dấu trang

dialog-delete-bookmark = Xóa dấu trang

dialog-delete-bookmark-prompt = Xóa dấu trang “{ $name }”?

dialog-delete-bookmark-note = Thao tác này gỡ dấu trang. Tệp trên đĩa không bị xóa.

dialog-delete-bookmark-folder = Xóa thư mục dấu trang

dialog-delete-bookmark-folder-prompt = Xóa thư mục dấu trang “{ $name }”?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Thư mục và { $count } mục bên trong sẽ bị gỡ. Tệp trên đĩa không bị xóa.
       *[other] Thư mục và { $count } mục bên trong sẽ bị gỡ. Tệp trên đĩa không bị xóa.
    }

dialog-rename-bookmark-folder = Đổi tên thư mục dấu trang

dialog-bookmark-library = Thư viện

dialog-new-shortcut = Lối tắt mới

dialog-new-remote-shortcut = Lối tắt từ xa mới

dialog-shortcut-name = Tên lối tắt

dialog-shortcut-target = Đường dẫn đích

dialog-create-remote-shortcut = Tạo lối tắt từ xa

dialog-cancel-new-shortcut = Hủy lối tắt mới

dialog-properties = { $name } - Thuộc tính

dialog-properties-aria = Thuộc tính { $name }

dialog-general = Chung

dialog-file-type = Loại: { $value }

dialog-location = Vị trí: { $value }

dialog-size = Kích thước: { $value }

dialog-date-created = Ngày tạo: { $value }

dialog-date-modified = Ngày sửa đổi: { $value }

dialog-permissions = Quyền: { $mode }

dialog-file-in-use = Tệp đang được dùng

dialog-items-in-use = Một số mục đang được dùng

dialog-lock-owner-body =
    { $count ->
        [one] Windows không xóa được { $count } mục đã chọn vì ứng dụng khác đang dùng.
       *[other] Windows không xóa được { $count } mục đã chọn vì ứng dụng khác đang dùng.
    }

dialog-apps-using-file = Ứng dụng đang dùng tệp

dialog-close-results = Kết quả đóng ứng dụng

dialog-new-bookmark = Dấu trang mới

dialog-edit-bookmark = Sửa dấu trang

dialog-name-accelerator = Tên (N)

dialog-location-accelerator = Vị trí (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Hiện trình sửa khi lưu (S)

dialog-lua-source = Mã nguồn Lua (chỉ current_folder chỉ đọc)

dialog-folder-path-editable = Đường dẫn thư mục (sửa được)

dialog-file-path-editable = Đường dẫn tệp (sửa được)

dialog-root = Gốc

dialog-folder-options-scrollbar = Thanh cuộn dọc Tùy chọn thư mục

dialog-new-bookmark-default = Dấu trang mới

dialog-new-folder-default = Thư mục mới

dialog-new-shortcut-default = Lối tắt mới

dialog-shortcut-name-invalid = Tên lối tắt phải là tên hợp lệ trong thư mục hiện tại.

dialog-shortcut-target-invalid = Nhập đường dẫn đích hợp lệ.

dialog-shortcut-window-closed = Cửa sổ chính đã đóng nên không tạo được lối tắt.

dialog-folder-changed = Thư mục hiện tại đã đổi. Mở lại cửa sổ Lối tắt mới.

dialog-remote-unavailable = Dịch vụ từ xa hiện không khả dụng.

dialog-shortcut-in-progress = Đang tạo lối tắt khác.

dialog-allowed = Được phép

dialog-not-allowed = Không được phép

dialog-read = Đọc

dialog-write = Ghi

dialog-execute = Thực thi

dialog-owner = Chủ sở hữu

dialog-group = Nhóm

dialog-others = Khác

dialog-remote-folder = Thư mục từ xa

dialog-remote-file = Tệp từ xa

dialog-unavailable = Không khả dụng

dialog-bytes = { $value } byte

dialog-extension-author-bio = Tác giả SuperExplorer và tiện ích mẫu chính thức

dialog-extension-purpose-folder-size = Hiện kích thước tệp và cộng đệ quy kích thước thư mục ở nền.

dialog-extension-purpose-size-map = Hiện dạng bản đồ diện tích dung lượng mỗi mục trong thư mục hiện tại.

dialog-extension-purpose-tokei = Đếm dòng mã trong tệp hoặc thư mục bằng Rust và tokei.

dialog-extension-purpose-lua-tokei = Tiện ích Lua mẫu đếm dòng mã.

dialog-extension-purpose-lock-owner = Hiện chương trình hoặc dịch vụ đang khóa tệp.

dialog-extension-purpose-exif = Gợi ý đổi tên hàng loạt từ dữ liệu EXIF ảnh.

dialog-extension-purpose-7z = Hiện kho 7-Zip thành thư mục ảo duyệt được.

dialog-extension-purpose-bulk-folder = Tạo nhiều thư mục cùng lúc từ mẫu người dùng.

dialog-extension-folder-size = Cột kích thước thư mục

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Dòng mã chính

dialog-extension-code-lines = Dòng mã

dialog-extension-lock-owner = Chủ khóa

dialog-extension-exif = Đổi tên từ EXIF

dialog-extension-7z = Thư mục ảo 7-Zip

dialog-extension-bulk-folder = Trình tạo thư mục hàng loạt

dialog-git-hash = Mã băm Git
dialog-release-date-line = Ngày phát hành: { $value }
dialog-reset = Đặt lại
dialog-reset-prompt = Đặt lại { $label }? Chỉ trạng thái đã lưu bị xóa; tệp hiện tại không đổi.
dialog-reset-session-label = cửa sổ và tab đã lưu
dialog-reset-view-label = cài đặt dạng xem đã lưu
dialog-reset-quick-access-label = ghim Truy cập nhanh
dialog-reset-all-label = toàn bộ trạng thái Explorer đã lưu
dialog-permanent-delete-prompt = Xóa vĩnh viễn { $count } mục? Không thể hoàn tác thao tác này.
dialog-gdrive-trash-prompt = Chuyển { $count } mục vào thùng rác Google Drive? Có thể khôi phục trên drive.google.com trong 30 ngày. Đây không phải Thùng rác Windows.
dialog-permanent-delete-aria = Xóa vĩnh viễn { $count } mục
dialog-gdrive-trash-aria = Chuyển { $count } mục vào thùng rác Google Drive
