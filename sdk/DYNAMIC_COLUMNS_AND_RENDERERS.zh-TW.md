# 動態欄位與 renderer

SuperExplorer V1 Rust SDK 讓作者使用一般 Rust trait；DLL 邊界由 SDK 自己的
`abi_stable` adapter 處理。最小可工作的 clean consumer 位於
`sdk/fixtures/rust-folder-size-visual-column`。

## 欄位 contract

註冊 `ColumnDescriptorV1` 時提供 package-local ID、typed value、欄寬範圍、
對齊、適用項目、成本、穩定排序型別與 provider contribution。Host 加上 package
namespace，持久化 ID 為 `extension:<package-id>:<local-id>`，不同套件不會碰撞。

aggregate descriptor 必須宣告相依欄位與最大輸出筆數。Host 只接受有界、完整且
generation 相符的結果；partial、stale 或超額結果都會拒絕。renderer 接受的 value
kind 必須與欄位一致。

## Renderer contract

實作 `VisualColumnImplementationV1`。render 只會收到 immutable
`CellRenderContextV1`：typed value、exact bytes、aggregate、loading/error、
selection/hover、DPI、theme facade、settings、host-attested item ID、render
revision 與 request generation。它只回傳 data-only `CellRenderPlanV1`，真正的文字與
比例條由 host-owned GPUI 元件繪製。

render callback 在有界 host worker 執行，而且必須純粹且快速。不可列舉檔案、解析
內容、存取網路、阻塞或保留 host state；I/O 應放在 provider/job callback，再回傳
owned value。SDK adapter 會捕捉 panic 並讓該 renderer fault，不讓 unwind 穿過 host。
若完整 snapshot revision 已過期，host 會忽略舊 plan。

## 建置與檢查範例

在 repository root 執行：

```powershell
cargo test --manifest-path sdk/fixtures/rust-folder-size-visual-column/Cargo.toml --locked --offline
powershell -NoProfile -File scripts/build-plugin.ps1 -PluginRoot sdk/fixtures/rust-folder-size-visual-column
```

範例的 `Cargo.toml` 對第一方 SDK 使用 exact version 加相對路徑，第三方 crate 使用
registry version；它刻意不是 root workspace member。

