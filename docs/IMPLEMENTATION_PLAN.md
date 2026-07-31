# Rust + GPUI Windows Explorer 實作計畫

## 2026-07-29 roadmap closure

五階段 umbrella roadmap 已依 session → thumbnails → namespace → broker → preview 的相依順序整合完成。共同 typed reducer、generation/cancellation、bounded cache/IPC、fallback、UIA/focus 與 installer contracts 已進入 final validation；後續新增 Explorer parity 功能應沿用同一 service/event boundary，不可在 GPUI callback 直接做 filesystem、COM 或 IPC。

## 回歸策略（2026-07-28）

所有新功能先加入 deterministic Rust contract test，再視 Windows 行為加入 headful UITEST。標準入口為 `cargo run -p explorer-uitest -- --suite quick`；視窗、Shell、視覺與長時間案例分別由 full、interop、visual、soak suite 執行。新增 OpenSpec requirement 若沒有 manifest coverage，`--validate-only` 必須失敗。

## 2026-07-27 視覺／網址列 change 執行結果

`match-explorer-visual-address-parity` 的 production implementation 已完成 Explorer profile tokens、繁中 address/breadcrumb、deferred topmost chevron menu、Windows Shell icon memory/disk cache、typed sort、per-tab Details widths、八種 view、panes、雙 scrollbar pointer capture、caption 單一 hit rectangle及相關 headful harness。最終 quality gate 與證據路徑整理在 `docs/FINAL_HANDOFF.md`。

後續不是已知 production 功能缺口，而是需要額外驗證設備：在真正的 100/125/150/200% Windows sessions 重跑 raster/caption/menu matrix；在至少兩個不同 DPI 的實體顯示器間 move/maximize/restore；用硬體滑鼠或合格 input driver 重跑 Explorer↔app physical Drop。這些項目在設備到位前維持未勾選。

本文件是 OpenSpec change `build-rust-gpui-windows-explorer` 的執行入口；細部追蹤以 `openspec/changes/build-rust-gpui-windows-explorer/tasks.md` 為準。

## 範圍

本 change 包含：

1. M0/M1 foundation：Windows-only Cargo workspace、固定 GPUI 依賴、diagnostics、Shell STA、GPUI window、Windows 11 shell UI、theme/layout/actions/focus 與 parity evidence。
2. 多分頁與真實資料夾：per-tab history/state、stable identity、增量列舉、generation/cancellation、watcher、真實小型與 100,000 項目資料夾測試。
3. 原生檔案操作：create、rename、copy、move、recycle/permanent delete、progress、cancel、conflict、partial failure 與安全 undo/redo。
4. Shell 資料交換與選單：Explorer 雙向 Clipboard、OLE left/right drag-and-drop、drop effects、auto-scroll、background/single/multi `IContextMenu3`。
5. Search：typed AST、地址列/搜尋列分離、per-tab cancellation、Windows Search、bounded fallback、incremental/dedupe/stale-result isolation。

暫不包含完整 thumbnails/icon views、Home/Gallery/ZIP/Libraries/第三方 namespace parity、Preview Handler、session restore 與第三方 extension 的完整跨 process broker hardening。

## OpenSpec artifacts

- `openspec/changes/build-rust-gpui-windows-explorer/proposal.md`
- `openspec/changes/build-rust-gpui-windows-explorer/design.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/windows-app-foundation/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/explorer-shell-ui/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/tabbed-folder-navigation/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/native-file-operations/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/shell-data-transfer-and-menus/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/file-search/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/specs/parity-verification/spec.md`
- `openspec/changes/build-rust-gpui-windows-explorer/tasks.md`

## 執行順序與 checkpoint

| 順序 | Tasks | 可執行 checkpoint | 主要驗證 |
|---|---|---|---|
| 1 | 1–5 | M0 可啟停視窗與 STA | Cargo gates、panic、啟停、handle snapshot |
| 2 | 6–15 | M1 Windows 11 shell UI | behavior、DPI、visual/manual matrix |
| 3 | 16–19 | 多分頁真實資料夾 | fake/real contract、watcher、100k dataset |
| 4 | 20 | 原生檔案操作 | destructive fixture、progress/cancel/conflict/undo |
| 5 | 21–23 | Explorer 資料交換與選單 | Clipboard/OLE/context menu interoperability |
| 6 | 24 | 搜尋 | parser、Windows Search/fallback、100k/cancel |
| 7 | 25 | 全範圍交付 | E2E、soak、parity closure、handoff |

每個 checkpoint 都必須保持可編譯、可執行、可測試；沒有 evidence 的 parity 項目不得標記完成。

## 標準品質閘門

首次 clone 必須先取得固定 GPUI-CE submodule：

```powershell
git submodule update --init --recursive
```

```powershell
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

Windows-only 或 Explorer 互通案例若無法自動化，必須在 `docs/MANUAL_TESTS.md` 記錄 actual result、環境與證據；不得填寫假成功結果。

## 2026-07-26 checkpoint 實況

| Checkpoint | 實際狀態 | 偏差與下一個不可跳過條件 |
|---|---|---|
| M0 | 完成 | release lifecycle、panic、process resource snapshot、locked Cargo gates與OpenSpec strict validation均有實證；GPUI-CE close tracing error列為已知差異 |
| M1 | 部分完成 | production chrome與七種 deterministic fixtures完成；目前desktop是175%，100/125/150/200%、high contrast、keyboard/Narrator/IME、Snap與Explorer baseline仍不可宣稱完成 |
| 多分頁／真實資料夾 | 完成 | real Shell E2E、history、watcher、reparse與100k explicit evidence完成 |
| 原生檔案操作 | 完成 | owned destructive fixtures涵蓋create/rename/copy/move/delete/cancel/conflict/journal |
| Clipboard/OLE/menu | 部分完成 | native與controlled tests完成；進入最終closure前仍需實際雙向drag gesture與installed第三方extension |
| Search | 完成 | parser、Windows Index probe、bounded fallback、partial/cancel/stale與real-folder E2E完成 |
| 全範圍交付 | 進行中 | 先完成指定實機／人工矩陣、長時間全能力soak、正式visual diff，再建立最終handoff；不得用fake-only evidence替代 |

執行數字、artifact路徑、hardware與安全稽核詳見 `docs/CHECKPOINT_EVIDENCE.md`。進入 thumbnails、namespace、preview或cross-process broker hardening前，以上未驗證的Windows interoperability與accessibility case必須維持可見狀態。
