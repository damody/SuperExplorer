## Why

ADB upload/download 目前為了取得進度而週期性執行 remote `stat` 或遞迴掃描本機目的樹，會與傳輸爭用 ADB server、增加磁碟 I/O，且大型資料夾成本會隨節點數放大。ADB 本身已輸出即時 progress messages，應直接使用該權威來源並在格式不可靠時安全降級。

## What Changes

- 增量 drain ADB stdout/stderr 並解析 `\r`／`\n` progress frames。
- 將可靠百分比或 byte pair 轉為單調 delivered-byte delta，接入既有 operation reporter。
- 成功時才補齊可靠已知大小；失敗、取消或 timeout 保留最後實值。
- 移除 ADB upload remote `stat` 輪詢與 download 本機 tree scan 輪詢。
- 保留 bounded diagnostics、取消、timeout、panic isolation 與未知格式 indeterminate fallback。
- 不修改公開 extension ABI、不自行實作 ADB sync protocol、不新增 ETA／speed UI。

## Capabilities

### New Capabilities

- `adb-native-transfer-progress`: 規範 ADB 原生進度串流解析、byte 映射、降級與生命週期行為。

### Modified Capabilities

無。

## Impact

- `crates/explorer-remote/src/adb.rs`：runner streaming、parser、provider callback。
- `crates/explorer-remote/src/bin/remote_owned_fixture_probe.rs`：實機中間進度驗證。
- `crates/explorer-app/src/remote_service.rs`：沿用既有 reporter，不改公開事件形狀。
- 測試會使用 `emulator-5554` 與既有受控 fixture；不保存或輸出 SFTP credential。
