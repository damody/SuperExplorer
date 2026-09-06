# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = ตัวเลือกโฟลเดอร์

dialog-about = เกี่ยวกับ SuperExplorer

dialog-version = เวอร์ชัน

dialog-build-date = วันที่คอมไพล์

dialog-author = ผู้เขียน

dialog-purpose = วัตถุประสงค์: { $value }

dialog-author-line = ผู้เขียน: { $name } — { $bio } · { $date }

dialog-community = ชุมชน: { $url }

dialog-plugin-safe-mode-title = เซฟโหมดปลั๊กอิน

dialog-plugin-safe-mode-body = เซฟโหมดปลั๊กอินเปิดอยู่ เลือกปลั๊กอินที่จะเปิดใช้อีกครั้ง เลือกนำไปใช้หรือตกลง แล้วรีสตาร์ต SuperExplorer

dialog-safe-mode-confirm-title = เซฟโหมดต้องมีการยืนยัน

dialog-safe-mode-confirm-aria = ต้องยืนยันเซฟโหมด; แพ็กเกจที่น่าสงสัย: { $package }

dialog-suspect-package = แพ็กเกจที่น่าสงสัย: { $package }

dialog-interface = อินเทอร์เฟซ: { $value }

dialog-operation = การดำเนินการ: { $value }

dialog-confirm-reenable = ยืนยันและเปิดใช้อีกครั้ง

dialog-bookmark-action = การดำเนินการบุ๊กมาร์ก

dialog-delete-bookmark = ลบบุ๊กมาร์ก

dialog-delete-bookmark-prompt = ลบบุ๊กมาร์ก “{ $name }” หรือไม่

dialog-delete-bookmark-note = การดำเนินการนี้จะลบบุ๊กมาร์ก ไฟล์บนดิสก์จะไม่ถูกลบ

dialog-delete-bookmark-folder = ลบโฟลเดอร์บุ๊กมาร์ก

dialog-delete-bookmark-folder-prompt = ลบโฟลเดอร์บุ๊กมาร์ก “{ $name }” หรือไม่

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] โฟลเดอร์และ { $count } รายการภายในจะถูกลบ ไฟล์บนดิสก์จะไม่ถูกลบ
       *[other] โฟลเดอร์และ { $count } รายการภายในจะถูกลบ ไฟล์บนดิสก์จะไม่ถูกลบ
    }

dialog-rename-bookmark-folder = เปลี่ยนชื่อโฟลเดอร์บุ๊กมาร์ก

dialog-bookmark-library = ไลบรารี

dialog-new-shortcut = ทางลัดใหม่

dialog-new-remote-shortcut = ทางลัดระยะไกลใหม่

dialog-shortcut-name = ชื่อทางลัด

dialog-shortcut-target = เส้นทางเป้าหมาย

dialog-create-remote-shortcut = สร้างทางลัดระยะไกล

dialog-cancel-new-shortcut = ยกเลิกทางลัดใหม่

dialog-properties = { $name } - คุณสมบัติ

dialog-properties-aria = คุณสมบัติของ { $name }

dialog-general = ทั่วไป

dialog-file-type = ชนิด: { $value }

dialog-location = ตำแหน่ง: { $value }

dialog-size = ขนาด: { $value }

dialog-date-created = วันที่สร้าง: { $value }

dialog-date-modified = วันที่แก้ไข: { $value }

dialog-permissions = สิทธิ์: { $mode }

dialog-file-in-use = ไฟล์กำลังใช้งาน

dialog-items-in-use = บางรายการกำลังใช้งาน

dialog-lock-owner-body =
    { $count ->
        [one] Windows ไม่สามารถลบ { $count } รายการที่เลือกได้ เพราะแอปพลิเคชันอื่นกำลังใช้อยู่
       *[other] Windows ไม่สามารถลบ { $count } รายการที่เลือกได้ เพราะแอปพลิเคชันอื่นกำลังใช้อยู่
    }

dialog-apps-using-file = แอปพลิเคชันที่ใช้ไฟล์

dialog-close-results = ผลลัพธ์ของการปิดแอปพลิเคชัน

dialog-new-bookmark = บุ๊กมาร์กใหม่

dialog-edit-bookmark = แก้ไขบุ๊กมาร์ก

dialog-name-accelerator = ชื่อ (N)

dialog-location-accelerator = ตำแหน่ง (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = แสดงตัวแก้ไขเมื่อบันทึก (S)

dialog-lua-source = ซอร์ส Lua (ใช้ได้เฉพาะ current_folder แบบอ่านอย่างเดียว)

dialog-folder-path-editable = เส้นทางโฟลเดอร์ (แก้ไขได้)

dialog-file-path-editable = เส้นทางไฟล์ (แก้ไขได้)

dialog-root = ราก

dialog-folder-options-scrollbar = แถบเลื่อนแนวตั้งของตัวเลือกโฟลเดอร์

dialog-new-bookmark-default = บุ๊กมาร์กใหม่

dialog-new-folder-default = โฟลเดอร์ใหม่

dialog-new-shortcut-default = ทางลัดใหม่

dialog-shortcut-name-invalid = ชื่อทางลัดต้องเป็นชื่อที่ถูกต้องในโฟลเดอร์ปัจจุบัน

dialog-shortcut-target-invalid = ป้อนเส้นทางเป้าหมายที่ถูกต้อง

dialog-shortcut-window-closed = หน้าต่างหลักปิดแล้ว จึงสร้างทางลัดไม่ได้

dialog-folder-changed = โฟลเดอร์ปัจจุบันเปลี่ยนแล้ว เปิดหน้าต่างทางลัดใหม่อีกครั้ง

dialog-remote-unavailable = บริการระยะไกลไม่พร้อมใช้งานในขณะนี้

dialog-shortcut-in-progress = กำลังสร้างทางลัดอื่นอยู่แล้ว

dialog-allowed = อนุญาตแล้ว

dialog-not-allowed = ไม่อนุญาต

dialog-read = อ่าน

dialog-write = เขียน

dialog-execute = เรียกใช้

dialog-owner = เจ้าของ

dialog-group = กลุ่ม

dialog-others = อื่นๆ

dialog-remote-folder = โฟลเดอร์ระยะไกล

dialog-remote-file = ไฟล์ระยะไกล

dialog-unavailable = ไม่พร้อมใช้งาน

dialog-bytes = { $value } ไบต์

dialog-extension-author-bio = ผู้เขียน SuperExplorer และส่วนขยายตัวอย่างอย่างเป็นทางการ

dialog-extension-purpose-folder-size = แสดงขนาดไฟล์และรวมขนาดโฟลเดอร์แบบเรียกซ้ำในพื้นหลัง

dialog-extension-purpose-size-map = แสดงว่าแต่ละรายการในโฟลเดอร์ปัจจุบันใช้พื้นที่เท่าใดในรูปแบบแผนที่พื้นที่

dialog-extension-purpose-tokei = นับบรรทัดโค้ดในไฟล์หรือโฟลเดอร์ด้วย Rust และ tokei

dialog-extension-purpose-lua-tokei = ส่วนขยาย Lua ตัวอย่างที่นับบรรทัดโค้ด

dialog-extension-purpose-lock-owner = แสดงโปรแกรมหรือบริการที่กำลังล็อกไฟล์

dialog-extension-purpose-exif = แนะนำการเปลี่ยนชื่อเป็นชุดจากข้อมูล EXIF ของภาพถ่าย

dialog-extension-purpose-7z = แสดงไฟล์เก็บถาวร 7-Zip เป็นโฟลเดอร์เสมือนที่เรียกดูได้

dialog-extension-purpose-bulk-folder = สร้างหลายโฟลเดอร์พร้อมกันจากเทมเพลตที่ผู้ใช้กำหนด

dialog-extension-folder-size = คอลัมน์ขนาดโฟลเดอร์

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = บรรทัดโค้ดหลัก

dialog-extension-code-lines = บรรทัดโค้ด

dialog-extension-lock-owner = เจ้าของการล็อก

dialog-extension-exif = เปลี่ยนชื่อจาก EXIF

dialog-extension-7z = โฟลเดอร์เสมือน 7-Zip

dialog-extension-bulk-folder = ตัวสร้างโฟลเดอร์จำนวนมาก
