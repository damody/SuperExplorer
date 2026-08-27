## 1. 確認警告

- [x] 1.1 執行一般 workspace check，記錄目前每一個警告。
- [x] 1.2 查看每個警告附近的程式碼，確認修改不會改變功能。

## 2. 手動修正

- [x] 2.1 手動移除多餘的完整路徑，不使用 `cargo fix`。
- [x] 2.2 手動移除沒有使用的 import 與 `mut`。
- [x] 2.3 將 RAII event guard 改成底線名稱，保留原本生命週期。
- [x] 2.4 移除沒有使用的 error 綁定，保留原本比對分支。

## 3. 驗證

- [x] 3.1 執行 MFT focus 相關測試，確認 event guard 修改正常。
- [x] 3.2 執行 `cargo fmt --all --check`。
- [x] 3.3 執行 `cargo check --workspace --locked --offline`，確認零 warning、零 error。
- [x] 3.4 確認 `dead_code` 與 `unsafe_code` 仍然是零。
- [x] 3.5 執行 OpenSpec strict validation 並完成清單。

## 4. All-targets 清理

- [x] 4.1 執行 all-targets workspace check，記錄 28 個警告。
- [x] 4.2 手動移除測試 target 的多餘 import 與完整路徑，不使用 `cargo fix`。
- [x] 4.3 執行 all-targets workspace check，確認零 warning、零 error。
- [x] 4.4 盤點 Clippy 警告並記錄為需獨立治理的非 rustc 技術債。

## 5. 最終回歸

- [x] 5.1 執行格式檢查。
- [x] 5.2 執行 workspace all-targets 測試；記錄既有 native Shell 環境測試失敗。
- [x] 5.3 再次執行一般與 all-targets workspace check。
- [x] 5.4 再次執行 OpenSpec strict validation。

## 6. SDK fixture release 警告

- [x] 6.1 確認兩個 tokei fixture 的 import 只在測試使用。
- [x] 6.2 將測試專用 import 移入 `cfg(test)`，不使用 `cargo fix`。
- [x] 6.3 分別執行兩個 fixture 的 release build，確認零 warning、零 error。
- [x] 6.4 執行兩個 fixture 的測試與格式檢查。
- [x] 6.5 再次執行 OpenSpec strict validation。
