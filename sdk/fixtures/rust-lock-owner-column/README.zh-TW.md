# Rust Lock Owner 欄位

這是獨立 public SDK example，會在 Details 顯示目前鎖住檔案的程序。Plugin
只能透過 `LockOwnerQueryServiceV1` 取得 opaque item handle 與 owned 顯示資料；
路徑、native handle、關閉、終止程序等權限不會跨越 ABI。

Host 在背景做有界查詢，拒絕舊的 F5／導覽 generation；空結果會清除欄位。
按 F5 可手動重新查詢。建置、驗證與封裝指令請依英文 README 執行。
