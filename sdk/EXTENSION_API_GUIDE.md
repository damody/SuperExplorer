# Extension jobs, values, streams and diagnostics

本文件是外掛作者的雙語公開契約；不宣稱 roadmap/task 5 已完成。

## Two API surfaces / 兩個 API 表面

The author-facing `explorer_extension_api` uses `abi_stable` (`sabi_trait`,
`RArc`/`RVec`, `StableAbi`). It exposes `JobProviderImplementationV1`,
`JobContextV1`, `IncrementalResultSinkV1`, `PluginValueV1`, and typed outcomes.
Plugins do not submit scheduler class/priority jobs; the host composes these
calls into internal scheduler lanes and scopes. Host-internal scheduler, cache,
lifecycle, and UI records are diagnostic implementation facts.

作者 API 使用 `abi_stable` 型別與 trait；外掛不直接提交 scheduler
class/priority job，host 會組合到內部 lane/scope。host-internal 記錄不可由
外掛建立或持久化。

The root's internal `RegistrarFactoryV1(extern "C" fn)` and registrar
trampoline are SDK-owned frozen plumbing. Authors do not implement raw ABI
callbacks: implement ordinary Rust `ExtensionRegistrarImplementationV1` and
`JobProviderImplementationV1`, then construct the root with
`ExtensionRootModuleV1::new`.

Root 內部的 `RegistrarFactoryV1(extern "C" fn)` 與 registrar trampoline
是由 SDK 擁有並凍結的 ABI 管線。外掛作者不需實作 raw ABI callback；作者只需實作一般的
Rust `ExtensionRegistrarImplementationV1` 與 `JobProviderImplementationV1`，再以
`ExtensionRootModuleV1::new` 建立 root module。

## Limits and backpressure / Jobs, values and limits / 工作、值與上限

Incremental batches are bounded to 1,024 items and 1 MiB. A `PluginValueV1`
payload is at most 64 KiB. `SinkSubmitStatusV1` includes `ACCEPTED`,
`WOULD_BLOCK`, `STALE`, `CLOSED`, `WRONG_THREAD`, and `INVALID`; a
`rejected_batch` is returned unchanged. `WOULD_BLOCK` is non-terminal and
consumes no credit. A synchronous provider must not spin or retry blindly; if
it elects to stop after that response, it returns `JobTerminalV1::BACKPRESSURED`.
`remaining_batch_credits`, `remaining_item_credits`, and
`remaining_byte_credits` are instantaneous post-attempt snapshots, not
reservations. Only `ACCEPTED` transfers the submitted batch and therefore has
no `rejected_batch`; every non-`ACCEPTED` outcome returns the original batch
unchanged so the provider retains ownership.

`PluginValueKindV1` validates BOOL, I64, F64, BYTES, TEXT, LOCALIZED_TEXT,
STRUCTURED, OPAQUE, TIME_UNIX_NANOS and DURATION_NANOS. Stable sort kinds are
BOOL, I64, U64, F64, TIME_UNIX_NANOS, DURATION_NANOS, TEXT and BYTES. `BYTES`
is an owned blob, not a numeric file-size value; exact byte-size sorting uses
`StableSortValueV1::unsigned(exact_bytes)`. STRUCTURED payloads are canonical,
whitespace-free JSON. STRUCTURED/OPAQUE values have no generic intrinsic
ordering, but a contribution may attach a separately declared, supported
`StableSortValueV1`. OPAQUE routing also requires the matching sealed binding.

`BYTES` 是持有所有權的 blob，不是數值型檔案大小；需要以精確位元組數排序時，請使用
`StableSortValueV1::unsigned(exact_bytes)`。STRUCTURED 與 OPAQUE 沒有通用的
內在排序，但 contribution 可以另外附上已宣告且受支援的 `StableSortValueV1`；
OPAQUE routing 仍必須符合 sealed binding。

`remaining_batch_credits`、`remaining_item_credits` 與
`remaining_byte_credits` 是本次送出後的即時快照，不是預留額度。只有
`ACCEPTED` 會移轉 batch 所有權且不含 `rejected_batch`；所有非 `ACCEPTED`
結果都會原樣退回 batch，所有權仍屬 provider。

Each `IncrementalResultBatchV1` repeats the host-minted job and sink capability,
job/location/source generations, and a per-job sequence starting at 0 and
increasing by exactly one. Every entry echoes its current item and source
generations. Out-of-order sequence or generation mismatch returns `STALE`
without consuming credits. Structurally malformed transport returns `INVALID`
and quarantines/closes that producer generation; it is not retryable.
Use `PluginItemResultV1::value` for a value, or `absent` with `UNSUPPORTED`,
`UNAVAILABLE`, `CANCELLED`, `PLUGIN_ERROR`, or `INCOMPATIBLE`; an absent result
cannot carry a value or stable sort key.

`JobContextV1::poll_control` returns `ACTIVE`, `CANCELLED`,
`DEADLINE_ELAPSED`, or `CLOSED`. Provider terminals are `COMPLETED`,
`UNSUPPORTED`, `UNAVAILABLE`, `CANCELLED`, `DEADLINE_ELAPSED`,
`BACKPRESSURED`, `PLUGIN_ERROR`, `INCOMPATIBLE`, and `PANICKED`. Host-observed
cancellation/deadline wins over a late provider terminal. Progress uses
`try_submit_progress` with its own sequence starting at 0 and increasing by one.

The provider callback is synchronous and thread-bound. A cloned context, sink,
or `InputStreamV1` does not create an async/Future/runtime handle. Off-owner
`poll_control` reports `CLOSED`; off-owner sink/progress/stream operations
report `WRONG_THREAD`. Use after callback close or generation change reports
`CLOSED` or `STALE` as defined by that operation.

增量 batch 上限為 1,024 items／1 MiB，單一 `PluginValueV1` payload 上限為
64 KiB。sink 可能回傳 `ACCEPTED`、`WOULD_BLOCK`、`STALE`、`CLOSED`、
`WRONG_THREAD` 或 `INVALID`；拒絕時會原樣返還 `rejected_batch`，且
`WOULD_BLOCK` 本身不會終止 job 或消耗 credit。result 與 progress sequence
都必須從 0 開始逐一遞增。callback 是同步且綁定原執行緒；clone 不會變成
Future/runtime handle，跨執行緒或 callback/generation 結束後使用會被拒絕。
`remaining_batch_credits`、`remaining_item_credits`、`remaining_byte_credits`
只是當下快照，不是 reservation；只有非 `ACCEPTED` outcome 才附原樣
`rejected_batch`。sequence/generation 不符回 `STALE`，結構不合法才回
`INVALID` 並 quarantine 該 producer generation。
值可使用 BOOL/I64/F64/BYTES/TIME/DURATION/TEXT/LOCALIZED/STRUCTURED/OPAQUE；
sort key 僅支援 BOOL/I64/U64/F64/TIME/DURATION/TEXT/BYTES。無值結果使用
`UNSUPPORTED`、`UNAVAILABLE`、`CANCELLED`、`PLUGIN_ERROR` 或
`INCOMPATIBLE`；provider terminal 另包含 `COMPLETED`、deadline、
`BACKPRESSURED` 與 `PANICKED`，host 觀察到的取消/deadline 具有優先權。

## InputStream / 輸入串流

`InputStreamV1` is an optional host-attested service with `read`, `seek`, and
`length`; one read is at most 64 KiB. Statuses are `OK`, `EOF`, `CANCELLED`,
`DEADLINE_ELAPSED`, `STALE`, `CLOSED`, `WRONG_THREAD`, `UNSUPPORTED`, and
`INVALID`. It is not a finite-credit API and does not expose a cancellation
token; control is observed with `JobContextV1::poll_control`.
No stream credit counter or cancellation token crosses this ABI.

The request's `maximum_bytes` is the host allocation upper bound, not a caller
buffer. Reserved fields must be zero. The service exposes neither a path nor a
native handle, and callers must not assume the optional stream exists.
The host supplies it only for a sealed contribution authorized for
`filesystem.read`. Capability checks broker host services; they do not sandbox
native DLL code. See [Native plugin operations](NATIVE_PLUGIN_OPERATIONS.md).
The current host source-snapshot ceiling is 8 MiB and may be reduced by stricter
runtime quotas. `seek` or `length` may return `UNSUPPORTED`; every outcome is
generation-bound and must be discarded when it reports `STALE`.

`InputStreamV1` 是 host attested optional service，提供 read/seek/length，單次
read 最大 64 KiB。它不是 finite-credit API，也不暴露 cancellation token；
控制透過 `JobContextV1::poll_control`。只有 sealed contribution 具
`filesystem.read` 授權時 host 才會提供 stream；它不含 path/native handle，
capability 只約束 host service，不能把同一行程內的 native DLL 變成 sandbox。
目前 host source snapshot 的最高上限是 8 MiB，仍可能受更嚴格 quota 限制；
seek/length 可回 `UNSUPPORTED`，任何 `STALE` outcome 都必須丟棄。

## Rust-first provider sketch / Rust-first 範例

```rust,ignore
use abi_stable::std_types::ROption;
use explorer_extension_api::{IncrementalResultBatchV1, JobContextV1,
    JobControlStateV1, JobProviderImplementationV1, JobTerminalV1,
    SinkSubmitStatusV1};
struct Provider;
fn make_batch(context: &JobContextV1) -> IncrementalResultBatchV1 {
    // Fill one bounded batch from context's host-minted handles/generations.
    todo!()
}
impl JobProviderImplementationV1 for Provider {
    fn run(&self, context: JobContextV1) -> JobTerminalV1 {
        match context.poll_control() {
            state if state == JobControlStateV1::ACTIVE => {}
            state if state == JobControlStateV1::DEADLINE_ELAPSED =>
                return JobTerminalV1::DEADLINE_ELAPSED,
            state if state == JobControlStateV1::CANCELLED ||
                state == JobControlStateV1::CLOSED =>
                return JobTerminalV1::CANCELLED,
            _ => return JobTerminalV1::INCOMPATIBLE,
        }
        let outcome = context.try_submit(make_batch(&context));
        if outcome.status == SinkSubmitStatusV1::ACCEPTED {
            return if matches!(outcome.rejected_batch, ROption::RNone) {
                JobTerminalV1::COMPLETED
            } else {
                JobTerminalV1::PLUGIN_ERROR
            };
        }
        if matches!(outcome.rejected_batch, ROption::RNone) {
            return JobTerminalV1::PLUGIN_ERROR;
        }
        if outcome.status == SinkSubmitStatusV1::WOULD_BLOCK {
            JobTerminalV1::BACKPRESSURED
        } else if outcome.status == SinkSubmitStatusV1::STALE ||
            outcome.status == SinkSubmitStatusV1::CLOSED {
            JobTerminalV1::CANCELLED
        } else {
            JobTerminalV1::PLUGIN_ERROR
        }
    }
}
```

This is an abbreviated, non-compiling sketch because `make_batch` is omitted.
The [ABI transport fixture](fixtures/job-context-v1-contract/new-plugin/src/lib.rs)
shows exact record construction and returned-batch ownership, but deliberately
injects non-production sequences/statuses for negative compatibility tests; it
is not an author lifecycle/backpressure example.
Use the [offline production-semantics author fixture](fixtures/extension-author-jobs-v1/src/main.rs)
for sequence-0 construction, registrar/root wiring, credits, and status mapping.
The provider and registrar are copyable plugin-author examples. The fixture's
`MockHost`/`AbiJobHostServicesV1`/`JobHostServicesV1::from_host` block is a
test-only host harness that exercises the provider; those doc-hidden host
internals are not author API and must not be copied into a plugin.
The example does not construct the production host runtime.
`ACCEPTED` may complete; `WOULD_BLOCK` is non-terminal and a provider choosing
to stop returns `BACKPRESSURED`; other rejection statuses map to a conservative
non-backpressure terminal. The offline author fixture is the canonical runnable
example for batch construction and status mapping.

## Generation and cancellation / Generation and cache / 世代與快取

Sequence and generation are host-attested; stale publication is rejected and
terminal outcomes are exactly once. The result cache is host-only and does not
retain UI rows. `from_host` key inputs are package ID, sealed manifest digest,
contribution ID and data version, interface, feature ID and epoch, file identity
and metadata generation, option hash, watcher scope and generation, plus
recursive mode. Outcomes are `Hit`, `Miss`, or `RejectedStale`; current
host-policy defaults are 1,024 entries, 32 MiB, and 30 seconds, not a
compatibility SLA. Plugins cannot configure these defaults. The current
per-package and per-interface
limits are 128 entries and 4 MiB each.
Insertion returns `Inserted`, `RejectedStale`, or `RejectedCapacity`. Cache is
best-effort: no caller is guaranteed a hit, and TTL is non-sliding. View/job
generations are publication gates and a hit is rebound to a fresh generation;
they are not reusable payload-key identity.

Host result-runtime buffer defaults are policy, not scheduler queue limits or
SDK configuration: 256 active jobs
globally/32 per package; 1,024 batches globally/128 per package/32 per job;
32,768 items globally/4,096 per package/1,024 per job; and 64 MiB globally/
8 MiB per package/1 MiB per job. Generation advance, watcher rollover,
manual refresh, data-version change, lifecycle revoke, and TTL expiry reject or
remove stale facts; a cache hit is rebound to a current host generation.

cache 完全由 host 管理，外掛不能建立 key、查詢 cache 或調整容量。key 綁定
package ID、sealed manifest digest、contribution/data version、interface、
feature ID/epoch、檔案 identity/metadata generation、options 與 watcher
scope/generation。現行 policy 為全域 1,024 entries／32 MiB／30 秒 TTL，且
每 package、每 interface 各 128 entries／4 MiB；這些不是相容性 SLA。
insert 可能是 `Inserted`、`RejectedStale` 或 `RejectedCapacity`；cache 是
best-effort，沒有 hit 保證，TTL 不會因 lookup 滑動延長。view/job generation
是 publish gate，cache hit 會重新綁定新 generation，而不是 payload key。
上列 256 jobs／1,024 batches 等數字是 result-runtime buffer 上限，不是
CPU/I/O scheduler queue 或外掛可設定的 concurrency。

## Diagnostics / 診斷

`NativeCallTimingV1` records `package_id`, `callback_id`,
`primary_interface_namespace`, `primary_interface_value`, `operation`,
`elapsed`, `terminal`, and `slow`. The in-memory ring retains 128 records and
evicts the oldest. The configurable default slow-callback threshold is 250 ms.
For `JobProvider`, elapsed covers only the synchronous provider callback. For
`Registrar`, it covers the guarded activation envelope, including host
descriptor preflight/projection and foreign-object drops; it must not be read
as plugin-only execution time. Neither operation includes publish/UI delay.
`slow` is a diagnostic, not a timeout, cancellation, quarantine decision, or
performance SLA. `NativeCallTerminalV1::Accepted` means a known non-fault callback terminal,
not necessarily `JobTerminalV1::COMPLETED`. Diagnostics must not contain
absolute paths, secrets, environment values, file contents, or callback payloads.

`JobProvider` 的 elapsed 只涵蓋同步 provider callback。`Registrar` 的 elapsed
則涵蓋 guarded activation envelope，包括 host descriptor preflight/projection
與 foreign-object drops，因此不可解讀為 plugin-only 執行時間；兩者都不包含
publish/UI 延遲。`slow` 僅供診斷，不代表 timeout、取消、隔離判定或效能 SLA。

UI invalidation uses a non-sliding 16–50 ms window opened by the first accepted,
post-commit fact. Event-loop servicing is not a wall-clock SLA. Overflow becomes
one broad current-state refresh, and the UI thread rechecks generation before
emission. A worker never redraws; 1,000 item results must not cause 1,000
synchronous redraws.

`NativeCallTimingV1` 只記錄穩定 package/callback/interface、operation、elapsed、
terminal 與 `slow`；128 筆 ring 會淘汰最舊記錄。250 ms 是可設定的預設
slow-callback threshold，不是 SLA、timeout、取消或 quarantine。診斷不得包含
絕對路徑、secret、環境值、檔案內容或 callback payload。UI invalidation 從第一個
post-commit fact 開啟 non-sliding 16–50 ms window；overflow 只要求一次廣域
current-state refresh，送出前會重查 generation。worker 不可直接 redraw，
1,000 個結果也不得造成 1,000 次同步 redraw。
