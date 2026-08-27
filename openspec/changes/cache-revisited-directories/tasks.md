## 1. 快取模型與容量政策

- [x] 1.1 實作 Local、ADB、SFTP canonical DirectoryCacheKey，排除遠端暫態 identity，並加入等價與隔離測試。
- [x] 1.2 實作 64 個資料夾／100,000 rows 雙上限的 DirectorySnapshotCache、LRU 提升與淘汰。
- [x] 1.3 加入容量邊界、超大型單筆拒絕且不破壞既有 cache、LRU 命中排序測試。
- [x] 1.4 擴充 DirectoryState／TabState，使 navigation 可由指定 target snapshot 開始 Loading，並清除 selection。

## 2. 導覽與背景收斂整合

- [x] 2.1 將 cache 加入 AppViewState，讓直接 Navigate、Back、Forward、多步 history 與 Up 共用 target lookup。
- [x] 2.2 在 accepted DirectoryFinished 後把 Ready snapshot 寫回正確 location，拒絕失敗、取消及 stale event 寫入。
- [x] 2.3 加入 cache hit 首幀可見、cache miss 空 loading、batch 收斂、完成移除舊 rows、失敗保留 rows 的狀態測試。
- [x] 2.4 加入 Back／Forward／Backspace Up 與一般導覽入口共用 cache 的聚焦測試。

## 3. 指定路徑與限定範圍驗證

- [x] 3.1 建立指定 SFTP parent/test 來回切換的真實視窗或可重現服務測試，驗證快取先顯示且背景完成更新。
- [x] 3.2 執行 explorer-model 與 explorer-ui 相關聚焦測試。
- [x] 3.3 執行受影響 crate compile check，修正本變更造成的錯誤。
- [x] 3.4 執行格式、diff check 與嚴格 OpenSpec validation，確認未執行完整迴歸測試。
