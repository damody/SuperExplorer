# Final traceability

| Normative outcome / scenario | Gate | Tasks | Current evidence |
|---|---|---|---|
| Fourteen stable budgets, legacy defaults, clamping, and overflow-safe bounds | G-CONTRACT, G-MIGRATION | 1.1.1-1.2.3 | `1.1-cache-budget-contract.log`, `final-model.log` |
| Low stops including 24 MB, arbitrary integer input, keyboard operation, wrapping, and scrolling | G-EDITOR | 2.1.1-2.1.5 | `cache-budget-editors-final30/report.json` |
| Atomic Apply/OK, invalid-editor restoration, Cancel discard, and no stale 512 draft | G-COMMIT | 2.2.1-2.2.3 | `final-ui.log`, `cache-budget-editors-installed-final/report.json` |
| Immediate UI, Host, renderer, and disk-owner propagation plus restart initialization | G-RUNTIME, G-DISK | 3.1.1-3.2.3 | `5.1.2-memory-owner.log`, `5.1.2-disk-owner.log`, `cache-budget-editors-installed-representative/report.json` |
| Only approved rows are editable; headings, availability, and counters remain read-only | G-EDITOR | 2.1.4-2.1.5 | `cache-budget-editors-final30/report.json` (14 editors) |
| Versioned five-budget MFT configuration, safe older endpoints, unavailable state, and reconnect retry | G-MFT-IPC | 4.1.1-4.1.4 | `5.1.2-mft-reconnect.log`, `final-mft.log` |
| Independent MFT accounting, trimming, oversized records, 16384 MB bounds, atomic persisted pruning | G-MFT-TRIM | 4.2.1-4.2.5 | `final-mft.log` |
| Partial lineage remains partial through service, sorting, Details model, and Size Map model | G-PARTIAL | 4.3.1-4.3.3 | `final-mft.log`, `final-ui.log` |
| Installed Apply changes MFT LRU to 2048 without navigation; OK/restart retains 4096 | G-INSTALL | 5.2.3 | `cache-budget-editors-installed-final/report.json`, current installed-run `explorer.log` commit records |
| Installed representative UI/Host/GPU/disk/MFT budgets persist across restart | G-INSTALL | 5.2.4 | `cache-budget-editors-installed-representative/report.json` and screenshots |
| Installed bounded fixture visibly presents partial state in Details and Size Map | G-INSTALL | 5.2.5 | Size Map passed in `target/openspec-evidence/independent-cache-max-editors/installed-partial-final-2/report.json`; installed Details partial remains unresolved |

Every completed task has a passed or explicitly superseded record in `index.jsonl`. Task 5.2.5 remains open because its installed Details half has not passed.
