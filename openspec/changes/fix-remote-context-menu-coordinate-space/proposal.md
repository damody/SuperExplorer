## Why

ADB／SFTP 的 GPUI 自訂右鍵選單錯把 Windows 螢幕座標當作視窗 client 座標，導致視窗離開螢幕原點時選單偏到右下方。必須明確區隔兩種座標空間，讓遠端與原生選單各自使用正確座標。

## What Changes

- 右鍵動作同時攜帶 client 與 screen anchor。
- ADB／SFTP 自訂選單使用 client anchor，Local Windows Shell 選單維持 screen anchor。
- 鍵盤 Menu／Shift+F10 與滑鼠右鍵共用雙座標契約。
- 保留自訂選單的視窗邊界避讓。

## Capabilities

### New Capabilities

- `context-menu-coordinate-routing`: 定義原生與自訂右鍵選單的雙座標路由、鍵盤 anchor 與邊界行為。

### Modified Capabilities

無。

## Impact

影響 `explorer-ui` 的 action payload、右鍵事件建立、state routing、自訂遠端選單定位與 focused tests。沒有外部 API、依賴或資料格式變更。
