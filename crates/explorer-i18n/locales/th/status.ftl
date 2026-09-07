# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] คัดลอก { $count } รายการ
       *[other] คัดลอก { $count } รายการ
    }

move-items =
    { $count ->
        [one] ย้าย { $count } รายการ
       *[other] ย้าย { $count } รายการ
    }

recycle-items =
    { $count ->
        [one] ย้าย { $count } รายการไปถังรีไซเคิล
       *[other] ย้าย { $count } รายการไปถังรีไซเคิล
    }

permanent-delete-items =
    { $count ->
        [one] ลบถาวร { $count } รายการ
       *[other] ลบถาวร { $count } รายการ
    }

gdrive-trash-items =
    { $count ->
        [one] ย้าย { $count } รายการไปยังถังขยะ Google Drive
       *[other] ย้าย { $count } รายการไปยังถังขยะ Google Drive
    }

shortcut-items =
    { $count ->
        [one] สร้างทางลัดสำหรับ { $count } รายการ
       *[other] สร้างทางลัดสำหรับ { $count } รายการ
    }

extra-items =
    { $count ->
        [one] { $first } (อีก { $count } รายการ)
       *[other] { $first } (อีก { $count } รายการ)
    }

clipboard-source = แหล่งที่มาจากคลิปบอร์ดของระบบ

op-new-folder = โฟลเดอร์ใหม่ | { $path }

op-new-file = ไฟล์ใหม่ | { $path }

op-rename = เปลี่ยนชื่อ | { $from } → { $to }

op-chmod = เปลี่ยนสิทธิ์ | { $path } → { $mode }

op-copy-route = { $action } | { $source } → { $destination }

op-move-route = { $action } | { $source } → { $destination }

op-recycle-route = { $action } | { $source }

op-permanent-delete-route = { $action } | { $source }

op-gdrive-trash-route = { $action } | { $source }

op-shortcut-route = { $action } | { $source }

op-preparing-copy = กำลังเตรียมคัดลอก

op-copying = กำลังคัดลอก

op-copy-complete = คัดลอกเสร็จแล้ว

op-preparing-move = กำลังเตรียมย้าย

op-moving = กำลังย้าย

op-move-complete = ย้ายเสร็จแล้ว

op-preparing = กำลังเตรียม

op-processing = กำลังดำเนินการ

op-complete = เสร็จแล้ว

op-finalizing = กำลังทำให้เสร็จ

op-progress-items = { $summary } | { $phase } | ความคืบหน้า { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | ความคืบหน้า { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | ความคืบหน้า { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | เสร็จแล้ว

op-cancelled = { $summary } | ยกเลิกแล้ว

op-failed = { $summary } | ล้มเหลว: { $error }

op-partial = { $summary } | บางส่วน: สำเร็จ { $succeeded }/{ $total }

op-cancelling = { $summary } | กำลังยกเลิก

op-success-route = สำเร็จ | { $route }

op-skipped-route = ข้ามแล้ว | { $route }

op-cancelled-route = ยกเลิกแล้ว | { $route }

op-partial-status = บางส่วน

op-failed-status = ล้มเหลว

op-error-code =  | รหัสข้อผิดพลาด { $code }

op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = ไม่ได้ระบุปลายทาง

op-no-source = ไม่ได้ระบุแหล่งที่มา

apk-installing = กำลังติดตั้ง: { $name } → { $target }

apk-installed = ติดตั้งแล้ว: { $name } → { $target }

apk-cancelled = ยกเลิกการติดตั้ง { $name } ไปยัง { $target }

apk-timeout = หมดเวลาติดตั้ง { $name } ไปยัง { $target }

apk-failed = ติดตั้ง { $name } ไปยัง { $target } ไม่สำเร็จ: { $error }

apk-check-device = ตรวจสอบการเชื่อมต่ออุปกรณ์และ APK แล้วลองอีกครั้ง
status-no-selection = ไม่ได้เลือกรายการ

status-details-unavailable = ไม่สามารถโหลดรายละเอียด

status-item-count =
    { $count ->
        [one] { $count } รายการ
       *[other] { $count } รายการ
    }

status-preview-select-one = เลือกรายการเพื่อแสดงตัวอย่าง

status-preview-failed = ไม่สามารถสร้างตัวอย่างของไฟล์นี้

status-preview-loading = กำลังโหลดตัวอย่าง…

status-preview-item-failed = ไม่สามารถโหลดรายการแสดงตัวอย่าง

status-preview-select-single = เลือกเพียงรายการเดียวเพื่อแสดงตัวอย่าง

status-no-transfers = ไม่มีไฟล์ปฏิบัติการในเซสชันนี้

status-thumbnail-cleared = ล้างแคชภาพขนาดย่อแล้ว

status-thumbnail-clear-partial = ไม่สามารถล้างแคชภาพขนาดย่อได้ทั้งหมด คุณสามารถลองอีกครั้ง และการเรียกดูยังใช้ได้

status-invalid-folder-name = ชื่อโฟลเดอร์ไม่ถูกต้อง แก้ไขแล้วลองอีกครั้ง

status-name-conflict = มีรายการชื่อนี้อยู่แล้ว

status-context-menu-unresponsive = เมนูบริบทไม่ตอบสนอง คุณยังทำงานต่อได้

status-cannot-go-forward = ไปข้างหน้าไม่ได้

status-partial-folders = บางโฟลเดอร์ไม่สามารถแสดงได้

status-cannot-list-folder = ไม่สามารถแสดงโฟลเดอร์ได้

status-cannot-cancel-disconnected = ยกเลิกไม่ได้: บริการไฟล์ยังไม่ได้เชื่อมต่อ

status-cannot-cancel-operation = ไม่สามารถยกเลิกการดำเนินการไฟล์: { $error }

status-cannot-load-folder = ไม่สามารถโหลดโฟลเดอร์ ลองอีกครั้ง

status-drive-free-of = ว่าง { $free } จาก { $total }

status-no-media = ไม่มีสื่อ

status-disconnected = ตัดการเชื่อมต่อแล้ว

status-access-denied = การเข้าถึงถูกปฏิเสธ

status-capacity-unavailable = ไม่สามารถใช้ความจุได้

status-waiting-file-count = กำลังรอ File Count…

status-file-count-limit = ขึ้นกับ File Count จึงไม่ได้เริ่ม

status-file-count-over-limit = File Count เกินขีดจำกัด จึงไม่ได้เริ่ม

status-file-count-pending-label = Limit

transfer-upload = อัปโหลดปลายทาง

transfer-download = ดาวน์โหลดแหล่งที่มา

transfer-conflict-inspection = ตรวจสอบความขัดแย้งของปลายทาง
transfer-local-copy = คัดลอกในเครื่อง
transfer-source-delete = ลบต้นทางหลังย้าย
transfer-provider-panic = ข้อผิดพลาดของผู้ให้บริการถ่ายโอน
transfer-no-diagnostic = ไม่ได้ระบุข้อผิดพลาดพื้นฐาน
transfer-cancelling = กำลังยกเลิก

transfer-cancel = ยกเลิก

status-details-view = มุมมองรายละเอียด
status-icon-view = มุมมองไอคอน
status-error = ข้อผิดพลาด · 
status-loading = กำลังโหลด · 
status-items-selected = { $count } รายการ — เลือก { $selected } รายการ
status-operation-progress = การดำเนินการ { $completed }/{ $total } · { $name }
status-search-cancelled = ยกเลิกการค้นหา · 
status-search-error = ข้อผิดพลาดในการค้นหา · 
status-search-fallback = กำลังค้นหา (สำรองระบบไฟล์ ไม่มีดัชนี) · 
status-search-indexed = กำลังค้นหา (ดัชนี + สำรอง) · 
status-search-partial = ผลลัพธ์การค้นหาบางส่วน · 
status-search-results = ผลการค้นหา · 
status-lock-close-cancelled = การปิดแอปพลิเคชันถูกยกเลิก
status-lock-discovery-timeout = การค้นหาเจ้าของล็อกถึงกำหนดเวลาแล้ว
status-lock-finding = กำลังค้นหาแอปพลิเคชันที่ใช้รายการที่เลือก…
status-lock-owners-found = มี { $count } แอปพลิเคชันกำลังใช้รายการที่เลือก
status-lock-partial-close = แอปพลิเคชันบางตัวไม่ปิด ไม่มีการบังคับสิ้นสุดโพรเซส
status-lock-retry-limit = ถึงขีดจำกัดการลองใหม่แล้ว ไม่ได้ลบรายการ
status-lock-retrying = กำลังลองลบอีกครั้ง…
status-lock-unidentified = Windows ไม่สามารถระบุแอปพลิเคชันที่ใช้รายการนี้ได้
transfer-cancel-operation = ยกเลิกการดำเนินการไฟล์
transfer-open-location = เปิดตำแหน่งการถ่ายโอน

status-generic-item = รายการ

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
