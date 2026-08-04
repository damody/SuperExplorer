# Native extension safety and recovery / 原生擴充安全與復原

## English

Rust extension DLLs run in the SuperExplorer process. They are not a sandbox and must never be treated as a security boundary. A mapped DLL remains resident until process exit; SuperExplorer does not hot-unload or forcibly interrupt native code.

Before `LoadLibrary` and every native callback, the host writes a durable, package- and interface-scoped marker. A normal return, typed error, or translated unwind clears the matching callback marker. Abnormal termination leaves evidence for the next startup, which suppresses only the matching package/interface and presents a Safe Mode incident. Re-enabling requires explicit confirmation. A failed confirmation keeps the incident and does not dispatch plugin code.

Disabling closes new dispatch, requests cooperative cancellation, detaches contributions, and performs a bounded drain. Timeout or fault becomes `pending-restart`; restart is the recovery mechanism. Diagnostics contain bounded identifiers, timing classes, and hashes, never filesystem paths, secrets, or arbitrary plugin payloads.

## 繁體中文

Rust 擴充 DLL 與 SuperExplorer 在同一個行程內執行，不是 sandbox，也不能當成安全邊界。DLL 一旦映射就保留到行程結束；SuperExplorer 不會執行熱卸載，也不會強制中斷原生程式碼。

在 `LoadLibrary` 與每次原生 callback 之前，host 會寫入綁定 package/interface 的 durable marker。正常返回、typed error 或已轉換的 unwind 只會清除對應 marker。異常終止會保留證據；下次啟動只壓制對應 package/interface，並顯示 Safe Mode incident。重新啟用必須明確確認；確認失敗時保留 incident，且不會 dispatch Plugin 程式碼。

停用時會先關閉新 dispatch，再請求協作式取消、移除 contribution，並進行有界 drain。逾時或故障會轉為 `pending-restart`；重新啟動才是復原方式。診斷只包含有界 ID、timing class 與 hash，不包含檔案路徑、secret 或任意 Plugin payload。
