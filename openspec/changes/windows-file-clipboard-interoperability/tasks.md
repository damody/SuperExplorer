## 1. Windows OLE 剪貼簿契約

- [x] 1.1 盤點並修正 Shell STA 對外部 `CF_HDROP`、Preferred DropEffect、clipboard sequence 與非檔案格式的同步邏輯
- [x] 1.2 修正 SuperExplorer local Copy/Cut 發布標準 Shell `IDataObject` 的流程，使原生檔案總管可直接貼上
- [x] 1.3 補齊剪貼簿忙碌、陳舊 sequence、無效來源及所有權切換的安全錯誤處理

## 2. 應用程式與目的地路由

- [x] 2.1 將外部檔案剪貼簿狀態可靠地送至 UI，並讓貼上命令在提交時重新驗證真實 OLE 資料
- [x] 2.2 接通外部本機來源貼至 SuperExplorer local 的 Shell file operation 路徑
- [x] 2.3 接通外部本機來源貼至 ADB 與 SFTP 的既有暫存及 remote transfer service 路徑
- [x] 2.4 確保 Copy/Cut 完成、部分失敗、來源移除與暫存清理符合各目的地語意

## 3. 快捷鍵與剪貼簿隔離

- [x] 3.1 修正 local 檔案檢視 `Ctrl+C`／`Ctrl+X` 的選取項目發布與命令分派
- [x] 3.2 維持位址列、搜尋、重新命名及其他文字編輯器的文字 Copy/Cut/Paste 語意
- [x] 3.3 確保純文字、圖片與 HTML 不啟用檔案貼上，也不被檔案命令清除或改寫

## 4. 最後集中驗證

- [x] 4.1 新增或更新 OLE clipboard、外部狀態、sequence、drop effect 與非檔案隔離的聚焦測試
- [x] 4.2 新增或更新 local／ADB／SFTP 貼上路由、部分失敗與詳細錯誤的聚焦測試
- [x] 4.3 新增或更新檔案檢視與文字輸入焦點快捷鍵的聚焦測試
- [x] 4.4 Headful 驗證檔案總管複製至 SuperExplorer local、ADB、SFTP，以及 SuperExplorer local 複製至檔案總管
- [x] 4.5 執行格式化、相關 crate 測試與編譯檢查，審閱最終 diff 並以 strict 模式驗證 OpenSpec

## 5. 完整拖放矩陣收尾

- [x] 5.1 盤點 local／ADB／SFTP、跨 SuperExplorer 與原生檔案總管的 OLE drag source、drop target、effect 與 staging 路徑
- [x] 5.2 將 remote drop 改為 fail-closed：拒絕空白、非本機與 None／Link 請求，不允許靜默降級或空批次成功
- [x] 5.3 拒絕 remote drag 的空白或混合來源，確保所有來源完整 materialize 後才啟動 native drag
- [x] 5.4 補齊 remote drop 輸入/effect、拖放 terminal/staging 與 3×3 路由的聚焦測試
- [x] 5.5 執行格式化、相關 crate 全測試、workspace check、headful drag smoke、strict OpenSpec 與最終 diff 複查
- [x] 5.6 修正已填滿的 Details 檔案列攔截背景 drop，並以真實 Windows Explorer → ADB Copy／Move 與 local 回歸驗證
