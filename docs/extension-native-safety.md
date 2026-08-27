# Native extension safety and recovery / 原生擴充安全與復原

## English

Rust extension DLLs run in the SuperExplorer process. They are not a sandbox and must never be treated as a security boundary. A mapped DLL remains resident until process exit; SuperExplorer does not hot-unload or forcibly interrupt native code.

Before `LoadLibrary` and every native callback, the host writes a durable marker. A normal return or typed error clears it; a translated plugin panic deliberately retains it. A panic or abnormal termination therefore makes the next startup enter global Plugin Safe Mode and execute no plugin code. Folder Options > Extensions Apply/OK is the explicit recovery action: it clears recovered incidents, preserves individual package choices, and takes effect after restart. A failed recovery keeps Safe Mode active.

Disabling closes new dispatch, requests cooperative cancellation, detaches contributions, and performs a bounded drain. Timeout or fault becomes `pending-restart`; restart is the recovery mechanism. Diagnostics contain bounded identifiers, timing classes, and hashes, never filesystem paths, secrets, or arbitrary plugin payloads.

## 繁體中文

Rust 擴充 DLL 與 SuperExplorer 在同一個行程內執行，不是 sandbox，也不能當成安全邊界。DLL 一旦映射就保留到行程結束；SuperExplorer 不會執行熱卸載，也不會強制中斷原生程式碼。

在 `LoadLibrary` 與每次原生 callback 之前，host 會寫入 durable marker。正常返回或 typed error 會清除 marker；已轉換的 Plugin panic 會刻意保留。panic 或異常終止後，下一次啟動會進入全域 Plugin Safe Mode，完全不執行 Plugin 程式碼。使用者必須在「Folder Options > Extensions」按 Apply/OK 明確恢復；此動作會清除 incident、保留各 Plugin 的個別選擇，並於重新啟動後生效。恢復失敗時 Safe Mode 仍保持啟用。

Production startup scans direct `.sepack` children of the executable-adjacent `plugins` directory in deterministic order. Every archive is imported, validated, sealed, resolved, and admitted under its validated manifest identity; loose DLLs are never auto-discovered. New packages default enabled, while `feature-state-v1.json` stores explicit package choices. Installed shortcuts need no Plugin arguments, and `--plugin-dll` remains available only for development and tests.

停用時會先關閉新 dispatch，再請求協作式取消、移除 contribution，並進行有界 drain。逾時或故障會轉為 `pending-restart`；重新啟動才是復原方式。診斷只包含有界 ID、timing class 與 hash，不包含檔案路徑、secret 或任意 Plugin payload。
