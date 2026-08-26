# 遠端右鍵選單雙座標設計

## 問題

ADB／SFTP 使用 GPUI 自訂右鍵選單，但目前共用的右鍵事件只保留轉換後的 Windows 螢幕座標。自訂 overlay 將該座標當作視窗 client 座標，因此視窗不在螢幕原點時選單會向右下偏移。

## 設計

右鍵動作與 pending context hit 同時攜帶兩套明確命名的座標：

- `screen_x`／`screen_y`：提供 Windows Shell context menu。
- `client_x`／`client_y`：提供 GPUI 內的 ADB／SFTP 自訂 context menu。

滑鼠觸發時直接從事件位置保存 client 座標，再以既有轉換產生 screen 座標。鍵盤 Menu／Shift+F10 以聚焦列的 client anchor 產生兩套座標。State 在判定 provider 後選擇正確座標，不在 render 階段猜測或反向換算。

## 邊界行為

自訂選單仍以 client 座標為 anchor，右側或底部空間不足時限制在視窗範圍內。座標不得為負值；視窗跨螢幕或位於負螢幕座標時，client 座標不受影響。

## 相容性

Local 路徑的原生 Windows Shell 選單繼續使用 screen 座標，行為不變。ADB／SFTP 以外的 unsupported virtual provider 繼續 fail closed。選單命令、選取狀態與永久刪除確認流程不變。

## 驗證

- State 測試驗證 remote menu 保存 client 座標，而 Shell request 保存 screen 座標。
- UI 定位測試覆蓋一般位置、負值 clamp、右下邊界避讓。
- 僅執行相關 `explorer-ui` focused tests、`cargo fmt --check` 與 `git diff --check`。

