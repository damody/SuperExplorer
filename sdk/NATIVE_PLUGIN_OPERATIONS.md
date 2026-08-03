# Native Rust plugin operations and safety guide / 原生 Rust 外掛操作與安全指南

## English

### Scope and safety model

Rust extensions run **in the SuperExplorer process**, address space, integrity
level, and current-user authority. The manifest, signature,
ABI validation, feature gates, and Safe Mode controls decide whether and when a
known extension may be called; they do not sandbox code that is already running.
A native extension can read or modify any data available to the application
process, consume CPU or memory, deadlock application threads, crash the process,
create threads, or call operating-system, filesystem, registry, and network APIs
directly. Manifest capabilities constrain host-provided APIs; they cannot police
those direct native calls. Run only reviewed publishers and treat a
native extension as trusted application code. Safe Mode is an availability and
recovery mechanism, **not a security boundary**.

The SDK validates the ABI root, SDK compatibility, and (where applicable) GPUI
fingerprint before it invokes a registrar. This prevents known compatibility
failures before a callback, but hashes, signatures, publisher identity, review,
and validation cannot prove that arbitrary DLL code is safe or endorse it.
Authors must not rely on the host to contain `unsafe`, an infinite wait, native
process termination, or data access performed by their DLL.

V1 provides no process isolation or sandbox and no hot load, hot update, hot
replace, or hot unload. Its native target is Windows x64 MSVC. The guarded
operation currently attributable by a durable marker is the registrar; failures
in plugin-created threads or outside guarded calls may have no Safe Mode incident.

### Load, disable, and restart lifecycle

Native Rust DLLs load during startup only. A successfully validated DLL remains
resident until the process exits: SuperExplorer does not hot-unload it and will
never force-unload it to resolve a stuck callback. Installing, replacing,
updating, removing, or enabling an unloaded native DLL therefore requires an
application restart. A mapped DLL can remain resident even when a later root,
layout, fingerprint, or callback check rejects it; retaining the mapping avoids
dangling ABI references, allocators, threads, and GPUI state.

When an operator disables a loaded feature, the host immediately closes its
dispatch gate, stops admitting new contribution calls, removes/gates its
contributions, and requests ordered job/callback draining. If draining completes
within the configured bound, the feature becomes `DisabledResident` while its
validated DLL remains resident; restore completes before its gate reopens and
advances the epoch. If it does not, the feature becomes `PendingRestart` with
the `DrainTimedOut` reason. Do not repeatedly toggle it or attempt to unload the
DLL: exit SuperExplorer normally and restart it. A pending-restart feature stays
non-dispatchable and cannot be re-enabled in that process. A late callback or
`NativeDispatchLeaseV1` drop does not clear this sticky restart state.

| Operation | Ordered result |
| --- | --- |
| Startup | validate package/hash/signature/path/target; map sealed DLL; validate ABI/fingerprint before callbacks; write marker; call registrar; clear marker and record timing; seal startup gates |
| Disable loaded feature | close gate; detach contributions; cancel host-managed work; wait for leases/resources with one bounded deadline |
| Drain success | `DisabledResident`; DLL stays resident and may be restored without remapping |
| Drain timeout | `PendingRestart` + `DrainTimedOut`; do not kill the callback or unload the DLL |
| Enable unloaded / install / update / replace | record the corresponding restart requirement; load the new generation next process |
| Remove loaded | close and drain, then `PendingRestart(Remove)`; retain `DrainTimedOut` too if applicable |
| Shutdown | close all gates and perform bounded drain; the OS reclaims resident DLLs only when the process exits |

### Restart-reason matrix

`NativeRestartReasonV1` is intentionally explicit. `UnloadedEnable` means an
enable request targeted a DLL that was not admitted during startup. `Install`,
`Update`, `Replace`, and `Remove` record package changes that cannot alter a
resident DLL in place. `DrainTimedOut` records a bounded drain that did not
finish. `StartupAborted` records a startup session that did not complete. Every
reason requires process restart; none authorizes a hot unload or a manual DLL
replacement while SuperExplorer is running.

### Panic, typed failure, and raw termination

Before a guarded native registrar call, the host writes and durably flushes a
call marker. On a normal return, a typed plugin error, or a recoverable Rust
panic, the host attempts a durable marker clear before recording completion. A
call is complete only when delete and directory sync succeed. Clear failure
records `MarkerFailure`, faults activation, preserves fail-closed evidence, and
enters global denial rather than hiding an uncertain call. A
recoverable unwind panic crossing the supported registrar boundary is translated
to a plugin error/`Panicked` diagnostic; it is
not permission to continue using corrupted author state.

`std::process::abort`, access violations, stack overflow, power loss, and other
abnormal process termination may bypass Rust unwinding and marker cleanup. The
boundary also cannot contain undefined behavior, memory corruption, deadlock,
infinite loops, arbitrary plugin threads, or damaged allocator/GPUI state. On
the next startup, an uncleared marker is evidence of an interrupted call, not
proof of a root cause. Authors should return typed errors, keep callbacks short,
avoid blocking on host-owned threads, and test their own panic/FFI boundaries.
They must not use raw termination as a control-flow or recovery mechanism.

### Safe Mode incidents and confirmation

Startup recovery exposes path-free `NativeSafeModeIncidentV1` records. A normal
`RegistrarInProgress` recovered registrar incident identifies the package ID, sealed manifest digest,
entrypoint ID, root-module ID, interface namespace/value, and operation. The
host blocks the matching callback until the user explicitly confirms that one
incident through `confirm_safe_mode_incident`. Confirmation is scoped: it does
not re-enable unrelated packages, interfaces, or incidents.

Confirmation acknowledges a recovery decision; it does not verify that a DLL,
its data, or its publisher is safe. It removes denial evidence and can execute
the same native code again, immediately reproducing a crash, hang, corruption,
or data loss. Show the incident identity to the user, retain the package
version/digest for support, check the publisher's security/support contact,
back up important data, and prefer an updated package when the cause is unknown.
Never present Safe Mode confirmation as an approval or trust prompt, and do not
repeatedly confirm a reproducible failure.

An incident is a heuristic record of the last unfinished guarded call, not proof
that the named extension caused the process failure. Host failure, power loss,
or marker I/O failure can leave the same evidence, while a failure outside the
guard can leave none. An unknown or expired incident ID is rejected; confirmation
does not reload a DLL, clear restart facts, elevate capabilities, or override
publisher/signature validation.

Readable but malformed, overflowed, reparse-backed, or otherwise unsafe marker
residue becomes the path-free `UnsafeMarkerState` incident. This is a global
unsafe quarantine: `safe_mode_denies_all` denies native calls rather than
guessing which DLL is safe. Its confirmation re-checks/quarantines the captured
marker evidence and rescans storage; it must still fail closed if the state is
unsafe. Do not delete marker files by hand to bypass this state. Preserve the
evidence, investigate disk/ACL/endpoint-security changes, and use the product's
confirmation path only after the storage problem has been fixed.

If the marker root itself cannot be validated, opened, leased, or enumerated,
startup instead returns `MarkerStateUnavailable` before lifecycle acquisition.
Native startup remains fail closed and there may be no confirmable incident.
Repair the storage/ACL problem and restart; do not treat this error as an
`UnsafeMarkerState` confirmation opportunity.

### Diagnostics and privacy

`native_call_timings` retains a process-memory, bounded, path-free history (up to
128 V1 records; the oldest is evicted). It is neither persistent telemetry nor a
performance SLA. Each record contains package ID, primary interface namespace/value,
operation, elapsed time, terminal class, and the slow flag. The complete V1
terminal set is `Accepted`, `PluginError`, `Incompatible`, `Panicked`,
`MarkerFailure`, and `SafeModeDenied`.

| May be shown or logged / allow | Must be redacted / deny |
| --- | --- |
| package ID; sealed manifest digest; entrypoint/root-module ID; interface namespace/value; operation; sanitized loader code/restart reason; elapsed time; terminal; slow flag | application-state, sealed-store, source-package, plugin DLL, or user absolute path; marker path/content; environment variable/value; callback argument/result; raw OS error; panic payload/backtrace; native handle/function pointer/address; file content; credential, token, password, or secret |

Use allowed identity fields to correlate support reports; do not request raw
marker contents from end users. Package IDs and digests are identifiers, not
anonymous data; verify that the user may disclose them before sharing a report.

### Operator runbook

1. When Safe Mode appears, record the incident kind and its path-free identity
   fields, then leave the suspected contribution disabled.
2. For a scoped registrar incident, update or remove the suspected package if a
   fixed build is available. Confirm only the displayed incident after an
   informed user decision; verify that only that callback is re-admitted.
3. For `UnsafeMarkerState`/global quarantine, do not confirm repeatedly and do
   not manually delete evidence. Fix the marker-storage issue, then use the
   confirmation flow, which rescans and can remain denied.
4. For `DrainTimedOut`, stop work that depends on the feature, save user work,
   exit normally, and restart. Never use DLL unloading, thread termination, or
   process killing as a recovery action.
5. For a slow-call report, identify the package/interface/operation from the
   path-free timing record, collect the package version and sealed digest, and
   ask the publisher for a bounded callback fix. Slow timing is a diagnosis, not
   a safe reason to interrupt arbitrary native code.

### Author guidance and reproducible verification

Declare only the capabilities that the feature needs, do blocking I/O and long
work outside registrar callbacks, honour cancellation/drain requests, and keep
cross-boundary data FFI-safe. Avoid holding locks across host callbacks,
re-entering host lifecycles, or retaining private host state. Test normal typed
errors, recoverable panic translation, a simulated abnormal termination, slow
callbacks, and disable/drain behavior before publishing.

The SDK's reproducible contract builds a real fixture DLL and verifies raw
abort residue, Safe Mode block/confirmation, slow timing identity, and resident
DLL drain timeout/restart behavior:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/native-call-guard-contract.ps1
```

This command is a test fixture, not an operational repair tool.

## 繁體中文（zh-TW）

### 範圍與安全模型

原生 Rust 外掛與 SuperExplorer **同一個行程內執行**，共用位址空間、完整性層級與
目前使用者權限。manifest、簽章、ABI
驗證、feature gate 與 Safe Mode 只能控制已知外掛是否及何時被呼叫，不能把已
執行的 DLL 隔離成 sandbox。外掛可存取該行程可存取的資料、耗盡 CPU/記憶體、
死結執行緒、使行程崩潰、建立執行緒，或直接呼叫作業系統、檔案系統、Registry 與
網路 API。capability 只限制 host 提供的 API，不能攔截 DLL 的直接 native 呼叫；
hash、簽章、publisher identity 與 ABI 驗證也不是安全背書。只安裝並信任
已審查的 publisher；Safe Mode 是可用性與復原機制，**不是安全邊界**。

V1 沒有行程隔離、sandbox、hot load、hot update、hot replace 或 hot unload，native
目標為 Windows x64 MSVC。目前 durable marker 只保護 registrar；外掛自行建立的
thread 或 guard 以外的故障可能完全沒有 Safe Mode 事件。

### 載入、停用與重新啟動

Rust DLL 僅在啟動期載入，通過驗證後會常駐到行程結束；系統不支援 hot unload，
也不會為了解除卡住 callback 而強制卸載 DLL。安裝、替換、移除，或啟用尚未載入
的 Rust DLL 都必須重新啟動。停用已載入 feature 時，host 先關閉新 dispatch、
移除／封鎖 contribution，再要求有界的 jobs/callback drain。成功後 DLL 仍常駐；
逾時則為 `PendingRestart`，原因為 `DrainTimedOut`。此時請正常結束並重新啟動，
不要反覆切換、強殺 thread 或嘗試卸載 DLL。成功 drain 的狀態是
`DisabledResident`；逾時後即使 callback 稍後結束或 `NativeDispatchLeaseV1` 被 drop，
也不會自動清除 sticky restart。shutdown 只會關 gate 並有界 drain，DLL 由 OS 在行程
結束時回收。

DLL 一旦 map，即使後續 root/layout/fingerprint/callback rejection，也可能繼續 resident，
以避免懸空 ABI reference、allocator、thread 或 GPUI state。startup 的固定順序是：驗證
package/hash/signature/path/target → map sealed DLL → callback 前驗證 ABI/fingerprint →
寫入 marker → registrar → durable clear/timing → seal startup gates。停用順序必須是：
close gate → detach contribution → cancel host-managed work → 以同一 deadline 等待 lease／
resource drain。成功為 `DisabledResident`；loaded remove 最終是 `PendingRestart(Remove)`，
若同時逾時還會附加保留 `DrainTimedOut`。shutdown 同樣關 gate、detach/cancel 並以單一
deadline drain，但不會 hot unload。

所有 `NativeRestartReasonV1` 都要求重啟：`UnloadedEnable`、`Install`、`Update`、
`Replace`、`Remove`、`DrainTimedOut` 與 `StartupAborted` 分別記錄未在啟動期載入的
啟用、安裝／更新／替換／移除、drain 逾時與未完成啟動；它們都不允許 hot unload 或在
行程內手動替換 DLL。

### panic、typed error 與異常終止

每個受 guard 保護的 registrar callback 前都會寫入並同步 call marker。正常返回、
typed plugin error 與可復原 Rust panic 後，host 會嘗試 durable clear；只有 marker
delete 與目錄 sync 都成功才算完成。clear 失敗會記錄 `MarkerFailure`、使 activation
fault、保留 fail-closed 證據並進入全域拒絕，而不會隱藏不確定狀態。可復原 panic 會轉為
`Panicked` 類型診斷。`std::process::abort`、access violation、stack overflow、斷電
等可能略過 cleanup，下一次啟動才會以未清除 marker 回報「呼叫被中斷」，而非宣告
根因。作者應回傳 typed error、避免長時間 callback／host-thread wait，且不得把 raw
abort 當作控制流程或復原方法。
此 boundary 也無法 containment undefined behavior、memory corruption、deadlock、
infinite loop、任意 plugin thread，或已受損的 allocator／GPUI state。

### Safe Mode、事件與診斷

`NativeSafeModeIncidentV1` 的 `RegistrarInProgress` registrar 事件只提供無路徑識別：package、sealed
manifest digest、entrypoint、root module、interface namespace/value 與 operation。
使用者以 `confirm_safe_mode_incident` 確認的範圍只限該事件，不會重新啟用其他外掛
或介面。若可讀的 marker residue 格式錯誤、overflow、含 reparse 或無法安全歸因，會顯示
`UnsafeMarkerState` 並啟動全域 quarantine：
`safe_mode_denies_all` 會拒絕所有 native call。不可手動刪 marker 來繞過它；先修正
磁碟、ACL 或安全軟體問題，保留證據，然後走正式 confirmation/re-scan 流程。
confirmation 是復原決定，不是 DLL、資料或 publisher 已安全的認證。
若 marker root 本身無法驗證、開啟、取得 lease 或列舉，startup 會在 lifecycle acquire
前回傳 `MarkerStateUnavailable` 並 fail closed，此時可能沒有可確認事件。應先修復 storage／
ACL 再重新啟動，不能把它當成 `UnsafeMarkerState` confirmation。
事件只表示最後一個未完成的 guarded call，不是根因證明；host crash、斷電或 marker
I/O 失敗也可能留下同樣證據。confirmation 會允許同一段 native code 再執行，可能立刻
再次 crash、hang、破壞記憶體或資料。確認前應備份重要資料、查閱 publisher 的 security／
support 聯絡方式並優先更新；若可重現，不要反覆確認。未知或過期事件 ID 會被拒絕，
確認也不會重載 DLL、清除 restart facts、提高 capability 或略過簽章／publisher 驗證。

`native_call_timings` 最多保留 128 筆 V1、無路徑的 timing 記錄，包含 package、
interface、operation、耗時、terminal 與 slow flag，不含 DLL／使用者路徑、環境變數、
檔案內容或 secret，也不得輸出 app-state／sealed-store／source-package 絕對路徑、raw OS
error、panic payload/backtrace、callback 內容、native handle／位址或密碼。package ID 與
digest 仍是可識別資料，分享前應確認可揭露。慢 callback 是診斷訊號，不能成為安全強制
中斷 native code 的理由；timing 只是最多 128 筆的行程記憶體 ring buffer，不是持久
telemetry 或效能 SLA。
terminal 完整集合為 `Accepted`、`PluginError`、`Incompatible`、`Panicked`、
`MarkerFailure` 與 `SafeModeDenied`。

允許的 runtime diagnostics 僅限 package ID、sealed digest、entrypoint/root/interface stable
ID、operation、sanitized `NativeLoaderDiagnosticCodeV1`、effective state、
`NativeRestartReasonV1`、elapsed、slow flag 與 terminal。除此之外依前述 denylist redaction。

### 營運步驟與作者檢查

遇到 Safe Mode 時，記錄事件種類與無路徑 identity、維持疑似 contribution 停用，
再依修正版與使用者知情決定 scoped confirmation。遇到 global quarantine 則不要重複
確認或刪除證據；修復 marker storage 後讓系統重新掃描。遇到 drain timeout，保存
工作後正常重啟。作者應只宣告需要的 capability、把 I/O／長工作移出 registrar、
遵守 cancellation/drain、避免跨 host callback 持鎖與保留 private host state，並在發布
前測試 typed error、panic、異常終止、slow callback 與 disable/drain。

可重現驗證：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/native-call-guard-contract.ps1
```
