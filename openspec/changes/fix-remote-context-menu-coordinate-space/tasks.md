## 1. 雙座標資料流

- [x] 1.1 擴充右鍵 action 與 pending hit，使滑鼠及鍵盤路徑都保存 client 與 screen anchor。
- [x] 1.2 更新 context-menu state routing，使 ADB／SFTP 使用 client anchor、Local Shell request 使用 screen anchor。

## 2. 選單定位

- [x] 2.1 抽出自訂遠端選單定位 helper，對負值及右／下邊界執行 client-space clamp。
- [x] 2.2 更新遠端選單 render 使用 helper，保持命令、樣式與 overlay 關閉行為不變。

## 3. 聚焦驗證

- [x] 3.1 新增雙座標 routing、滑鼠與鍵盤 anchor、一般位置及邊界 clamp 測試。
- [x] 3.2 執行相關 explorer-ui focused tests 與 cargo check，不執行完整 workspace 回歸。
- [x] 3.3 執行 cargo fmt --check、git diff --check 與 OpenSpec strict validation，審查最終差異。

## 4. 遠端選單生命週期修正

- [x] 4.1 新增獨立的遠端選單明確關閉 action，移除任意非右鍵 action 自動關閉規則。
- [x] 4.2 讓 overlay 點擊、Esc、再次開啟與遠端選單命令關閉選單，pointer／hover action 保持開啟。
- [x] 4.3 新增生命週期 focused tests，並執行 explorer-ui check、相關測試、格式與 strict validation。

## 5. 遠端空白區背景選單

- [x] 5.1 將背景右鍵命中從 scroll-content bounds 改為完整 file-view viewport 與 file-origin 判定。
- [x] 5.2 保持 row handler 優先及 chrome／Details header／viewport 外 fail-closed。
- [x] 5.3 新增短清單空白與邊界 focused tests，執行 explorer-ui check、相關測試、格式與 strict validation。

## 6. Full-height viewport 事件 owner

- [x] 6.1 將 Background secondary-button handler 從 scroll content 移到 full-height file-view viewport owner。
- [x] 6.2 保持 row handler stop-propagation 與 header／viewport 邊界拒絕行為。
- [x] 6.3 新增 handler ownership 結構測試並執行 explorer-ui focused check、格式、diff 與 strict validation。
