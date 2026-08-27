# Scenario Traceability Matrix

Scenario heading 是本變更的穩定 scenario key；改名屬 B 級修正，必須同步更新本表、tasks 與既有 evidence lineage。每個 implementation／validation task 均以同名 `task_id` 作為唯一 evidence subcheck 寫入 `evidence/index.jsonl`；表內 Gate 與該 evidence record 的 `gate` 共同構成 requirement → implementation → validation → evidence 映射。

| Stable scenario key | Implementation task(s) | Validation task(s) | Gate |
|---|---|---|---|
| Writable remote background | 1.1.1, 3.1.1 | 4.1.20 | REMOTE-MUTATION |
| Remote selected items | 1.1.1, 3.1.1 | 4.1.20 | REMOTE-MUTATION |
| Unsupported location fails closed | 1.1.1, 1.1.5 | 4.1.20 | REMOTE-MUTATION |
| Create folder in ADB or SFTP | 1.2.1, 1.2.3, 3.1.2 | 4.1.1, 4.1.4 | REMOTE-MUTATION |
| Invalid remote child name | 1.2.1, 1.2.3 | 4.1.1, 4.1.4 | REMOTE-MUTATION |
| Confirmed remote permanent delete | 1.2.2, 1.2.4, 3.1.3 | 4.1.2, 4.1.5 | REMOTE-MUTATION |
| Remote root and identity are never deletable | 1.3.1 | 4.1.2, 4.1.5 | REMOTE-MUTATION |
| Confirmation is immutable and stale-safe | 1.3.2 | 4.1.7, 4.1.8 | REMOTE-MUTATION |
| SFTP symlink deletion does not follow target | 1.2.4 | 4.1.5 | REMOTE-MUTATION |
| Local delete remains recyclable | 3.1.3 | 4.1.20 | FINAL-FOCUSED |
| Cancelled remote mutation | 1.2.5, 1.3.3 | 4.1.3, 4.1.6 | REMOTE-MUTATION |
| Cancellation between permanent-delete items | 1.3.3 | 4.1.3, 4.1.6 | REMOTE-MUTATION |
| Copy and cut remote entries | 2.3.1, 2.3.3 | 4.1.20 | CLIPBOARD-ISOLATION |
| Native remote clipboard token is authentic | 2.3.1 | 4.1.22 | CLIPBOARD-ISOLATION |
| Forged or replayed remote clipboard token | 2.3.2 | 4.1.22 | CLIPBOARD-ISOLATION |
| Paste typed mixed-provider sources | 2.3.3 | 4.1.20 | CLIPBOARD-ISOLATION |
| Local copy supersedes stale remote clipboard ownership | 5.1, 5.2 | 5.3 | CLIPBOARD-ISOLATION |
| Editable text owns keyboard clipboard | 2.3.4 | 4.1.20 | CLIPBOARD-ISOLATION |
| Text or image clipboard is ignored by file paste | 2.3.4 | 4.1.21 | CLIPBOARD-ISOLATION |
| Local and remote transfer matrix | 2.1.1, 2.1.6, 2.2.2 | 4.1.11 | TRANSFER-MATRIX |
| Recursive directory copy | 2.1.2 | 4.1.12 | TRANSFER-MATRIX |
| Fixed traversal and staging limits | 1.1.3, 1.1.4, 2.1.5 | 4.1.15, 4.1.16 | TRANSFER-MATRIX |
| Destination conflict | 2.1.4 | 4.1.14 | TRANSFER-MATRIX |
| Skipped descendant prevents move deletion | 1.1.2, 2.1.4, 2.2.3 | 4.1.14, 4.1.18 | TRANSFER-MATRIX |
| Partial destination is not destructively rolled back | 2.1.4, 2.2.3 | 4.1.18 | TRANSFER-MATRIX |
| Traversal or cancellation bound | 1.1.3, 2.1.2, 2.1.5 | 4.1.15, 4.1.18 | TRANSFER-MATRIX |
| Successful staged transfer | 2.2.1, 2.2.2 | 4.1.11, 4.1.17 | TRANSFER-MATRIX |
| Failed or cancelled staged transfer | 2.2.4, 2.2.5 | 4.1.17 | TRANSFER-MATRIX |
| Malicious Windows child name | 1.1.5, 2.1.3 | 4.1.13 | TRANSFER-MATRIX |
| Non-secret staging diagnostics | 2.2.7 | 4.1.17 | TRANSFER-MATRIX |
| Successful move | 2.2.3 | 4.1.18 | TRANSFER-MATRIX |
| Copy fails before deletion | 2.2.3, 2.2.5 | 4.1.18 | TRANSFER-MATRIX |
| Source deletion fails after copy | 2.2.3 | 4.1.18 | TRANSFER-MATRIX |
| Stale view after successful move | 2.3.5 | 4.1.19 | CLIPBOARD-ISOLATION |
| Partial move retains only incomplete intent | 2.3.6 | 4.1.19 | CLIPBOARD-ISOLATION |
| Internal cross-provider drag | 3.2.1, 3.2.2, 3.2.3 | 4.1.23 | DRAG-INTEROP |
| Native Local files dragged into remote | 3.3.1 | 4.1.24, 4.1.28 | DRAG-INTEROP |
| Remote items dragged to Windows Explorer | 3.3.2, 3.3.3, 3.3.4 | 4.1.27, 4.1.29 | HEADFUL-OLE |
| COM staging lease remains valid | 3.3.4, 3.3.5, 3.3.6 | 4.1.25, 4.1.26, 4.1.27 | DRAG-INTEROP |
| Drag-out materialization fails | 3.3.7 | 4.1.27, 4.1.29 | DRAG-INTEROP |
| Mixed item outcomes | 1.1.2, 3.1.4 | 4.1.18 | TRANSFER-MATRIX |
| Navigation during transfer | 3.1.5, 2.3.5 | 4.1.19 | FINAL-FOCUSED |
| Current affected locations refresh | 1.2.6, 3.1.4 | 4.1.18, 4.1.19 | FINAL-FOCUSED |
| Deadline before or during transfer | 1.2.5, 2.2.5 | 4.1.3, 4.1.6, 4.1.18 | TRANSFER-MATRIX |
| Owned remote destructive fixture | 1.3.1 | 4.1.9, 4.1.10 | DESTRUCTIVE-FIXTURE |
| Real Explorer drag evidence | 3.3.1, 3.3.2, 3.3.4 | 4.1.28, 4.1.29 | HEADFUL-OLE |
