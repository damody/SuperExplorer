## Why

目前正常 workspace 編譯會發出 417 個 `dead_code` 診斷，但正規化後的 322 個位置中，有 251 個來自同一份 MFT 原始碼被 library、helper、service 重複編譯所造成的 target-local 假陽性；其餘位置則混合了已被新 OpenSpec 架構取代的程式、只供測試的 seam，以及仍受既有規格要求但尚未接線的型別。若直接大量刪除或加入廣域 lint suppression，會誤刪跨 target 活程式、掩蓋未完成規格或破壞 migration、SQLite atomicity 與 Size Map fallback 測試。

## What Changes

- 建立 immutable、target-aware 的 `dead_code` baseline，將 emitted diagnostics、canonical warning sites、每個 primary item 與所有 emitting targets分開記錄。
- 逐項追溯 Git 歷史及既有 OpenSpec，將每段程式判定為：跨 target 活程式、規格要求但未接線、測試專用、已 supersede、或真正無引用。
- 移除已被 Host-prepared Code Lines、直接 MFT batch query、SQLite persistence、registry-aware UI 與新 bookmark/runtime-authority 流程取代的程式及過時測試。
- 將仍有規格價值的 failure injection、migration seam 與 reference fixture 移入 `#[cfg(test)]`，不得以 production `allow(dead_code)` 代替正確的測試邊界。
- 對仍受規格要求但屬其他active OpenSpec的 recovery/migration diagnostics 與 remove delta保留原change ownership；本change只建立窄範圍、具到期條件的item-level disposition，不新增其production行為。
- 整理 MFT 編譯拓樸，優先讓共用程式只在一個 internal crate/module authority 下編譯；只有無法安全拆分的窄入口才可使用帶具體 target 與保留原因的 item-level suppression。
- 以每批編譯、測試、warning delta、source hash 與 OpenSpec traceability 證明未改變 ABI、IPC、persistence、migration、filesystem semantics 或既有使用者行為。

## Capabilities

### New Capabilities

- `dead-code-governance`: 定義 target-aware dead-code inventory、逐項保留／移除決策、測試邊界、允許的 suppression 範圍，以及零 `dead_code` 診斷的驗證契約。

### Modified Capabilities

無。這項變更只整理實作與 lint ownership；若調查發現既有產品 requirement 必須改變，須依 B/C 級調整流程另行更新對應 capability。

## Impact

- 主要影響 `crates/explorer-app` 的 application、Folder Size、MFT query/journal/migration/persistence/runtime/size-map/SQLite/service 模組，以及 `explorer-ui`、`explorer-extension-host` 的少量 wrapper。
- 可能新增一個 workspace-internal MFT core crate或等價的單一編譯 authority，但不得改變公開 extension ABI、named-pipe frame、SQLite schema、cache location、service identity 或 installer 行為；新增path package必須以offline lockfile更新與diff/hash驗證納管。
- 預期移除 App-owned Code Lines directory cache 後，可在確認無其他引用時移除 `explorer-app` 對 tokei 的直接依賴；任何 dependency 變更必須由 locked/offline 驗證證明。
- 工作樹已有大量其他變更；apply 必須以 preimage hash、scoped diff 與 compare-before-write 保留使用者及其他 OpenSpec 的修改。
