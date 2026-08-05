# 需求追溯矩陣

| Requirement | Tasks / Gate | Evidence |
|---|---|---|
| 持久化型別化書籤 | 1.1、1.2 / G1-G2 | model bookmark tests、session store 5 tests、session fixture SHA-256 `6e70017f858424745b1bab8326948aa4963ae484ba68908f5bc154ca1ac0a982` |
| 書籤工具列與型別圖示 | 2.1 / G3 | `bookmark-toolbar.png`、overflow partition test |
| 建立與管理書籤 | 2.2 / G4 | `bookmark-context-menu.png`、`bookmark-star-on.png`、`bookmark-star-off.png`、`bookmark-manager.png`、registered `bookmark-toolbar-headful`（加入／取消／再加入） |
| 型別化目標啟用 | 2.1 / G3 | dispatch tests/build、headful toolbar projection |
| 受限的按需 Lua 執行 | 3.1 / G5 | explorer-automation bookmark tests（成功、唯讀、例外、timeout） |
| Lua host 邊界 | 3.1 / G5 | host-free runtime test (`io/os/package/debug == nil`) |
| 不再執行資料夾自動化 | 3.2 / G6 | retired-folder-automation test、production source negative search |

## Headful artifact hashes

- `bookmark-context-menu.png`: `cd20d57195fd75d01771ac377b305939f86e82be87232d53298b49b66dd1576a`
- `bookmark-toolbar.png`: `99d2eb2da0e2598a0b01de82165669f626f0622685bd32cd5397951d1652268e`
- `bookmark-overflow.png`: `67865ffde5121c9f4f0ac070fe544915336942b535554c5301618421741594c6`
- `bookmark-manager.png`: `a49df0f99b90c630fe31a455215f68490351dd512d3497fa3f4931189520fb9b`
- case `report.json`: `955a68e12d5e677c177ad82cff029b55498314edde8087bc4181a40c04818dce`
- UITEST runner `report.json`: `b78994b20c2f2bfd6f74bec7b5099a55091b0f9c7502f00b2d90046a8c7ee2bf`
