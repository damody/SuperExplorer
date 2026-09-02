# 安裝版驗收

最終無參數重建產物：`dist\SuperExplorer-Test-Setup-1.2026.9.2-x64.exe`，10,533,958 bytes，SHA-256 `443AB53D2C219281A5C132BDD33A890EA00789E82400DF2A6FCEBB49B78A91E7`。

實際安裝目錄由NSIS registry解析為`C:\Program Files\SuperExplorer`。同步installer terminal exit為0，batch顯示已建置、安裝、驗證及啟動。

- `SuperExplorer.exe`：release／installed皆為`CBF60D0DFEB8AB6FC8E7C66AEC8E608113B4E737740CF1D17D75FAA39786C0FA`。
- `explorer-extension-broker.exe`：release／installed皆為`1485F6E9683B5BA2DE88B67D6BE42D270E8AC3566F860B6E9F668C256F4C2BE4`。
- `explorer-extension-worker.exe`：release／installed皆為`580F6E89135937CB52FDAAA451B9955186AF14CB8BACED176F06F41B078EF62A`。

啟動目標為`C:\Program Files\SuperExplorer\SuperExplorer.exe`，不是工作樹binary或舊LocalAppData副本。
