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

op-copy-route = คัดลอก { $count } รายการ | { $source } → { $destination }

op-move-route = ย้าย { $count } รายการ | { $source } → { $destination }

op-recycle-route = ถังรีไซเคิล { $count } รายการ | { $source }

op-permanent-delete-route = ลบถาวร { $count } รายการ | { $source }

op-gdrive-trash-route = ถังขยะ Google Drive { $count } รายการ | { $source }

op-shortcut-route = ทางลัด { $count } รายการ | { $source }

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

op-no-destination = ไม่ได้ระบุปลายทาง

op-no-source = ไม่ได้ระบุแหล่งที่มา

apk-installing = กำลังติดตั้ง: { $name } → { $target }

apk-installed = ติดตั้งแล้ว: { $name } → { $target }

apk-cancelled = ยกเลิกการติดตั้ง { $name } ไปยัง { $target }

apk-timeout = หมดเวลาติดตั้ง { $name } ไปยัง { $target }

apk-failed = ติดตั้ง { $name } ไปยัง { $target } ไม่สำเร็จ: { $error }

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

transfer-cancelling = กำลังยกเลิก

transfer-cancel = ยกเลิก
