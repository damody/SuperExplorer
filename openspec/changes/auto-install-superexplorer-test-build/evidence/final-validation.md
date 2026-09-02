# 最終集中檢查

最終檢查包含：Lua component installer fixtures、check/no-launch/default install分支、installer PE及SHA-256、三個release／installed binary hash、ADB／SFTP真實拖放、來源保留、受控cleanup、`git diff --check`、新增行credential literal掃描、tasks validator、52筆唯一evidence index與`openspec validate --strict`。

完整檢查只涵蓋本次build／installer與指定拖放案例，未執行完整迴歸。
