# Extension jobs, values, streams, and cache

This document records the v1 host behavior that extension authors can rely on.
All commands are local and offline; they do not invoke CI.

## Scheduler and callback model

The host owns CPU and I/O queues independently. Queue and running limits are
validated before allocation, both globally and per package, with a hard maximum
of 65,536. Visible-viewport work is admitted ahead of prefetch work, while a
bounded visible burst rotates lower priority lanes so background work cannot
starve. Cancellation and deadlines are cooperative: the host closes admission
and result sinks, but never interrupts a Rust callback or unloads its DLL.

Providers are synchronous `abi_stable` callbacks invoked on a host worker.
Futures, executor handles, file paths, OS handles, GPUI objects, and render
contexts do not cross the ABI.

## Values and sorting

`PluginValueV1` transports the v1 bool, integer, float, bytes, time, duration,
text, localized, structured, and opaque domains. Malformed or unknown values
become a typed incompatible terminal; unsupported and unavailable never become
the numeric value zero. `StableSortValueV1` is independent of display text and
uses a deterministic total order with absent values at the tail in either sort
direction. Opaque payloads are accepted only by the same package, interface,
schema, digest, and live lifecycle generation that produced them.

## Input streams and cache

An `InputStreamV1` is issued only from a sealed `filesystem.read` contribution.
Reads, seeks, retained bytes, deadlines, cancellation, and source generations
are bounded by the host. A source identity change invalidates the handle before
data is published.

Cache keys include package and contribution identity, sealed manifest digest,
data version, file identity and metadata, options, and the applicable runtime,
location, item, source, watcher, TTL, and manual invalidation generations.
Navigation, tab changes, F5, watcher invalidation, feature disable, and package
updates therefore reject stale batches at drain and again at apply.

## Deterministic 1,000-item fixture

The in-tree `thousand_item_scheduler_runtime_ui_pipeline` fixture generates
items `0..999`, lifecycle generation 1, 32 visible items, then 968 prefetch
items. It uses queue limits 1,000/1,000, running limits 1/1, a 32-item visible
burst, a 16-batch UI drain, and a 16 ms non-sliding invalidation window.
The expected first 32 starts are `0..31`; all 1,000 values are eventually
applied; live runtime jobs stay at or below 4; queued batches/items stay at or
below 1,024; queued bytes stay at or below 65,536; cancellation is delivered in
one callback turn; redraw notifications are between 1 and 50, never 1,000.
The baseline list is asserted ready before the first scheduler start and before
any extension result is applied.

The companion `rapid_one_thousand_batches_coalesce_without_per_item_redraw`
fixture covers overload coalescing. Diagnostics expose only package,
contribution/interface, timing, and typed terminal state—never stream contents
or private paths.

Future headful mapping is intentionally declarative: selector
`extension-column-cell[data-state]` maps to the value/terminal projection;
artifact `extension-jobs-1000-items` maps to list readiness, visible-first
ordering, cancellation, and bounded redraw. It is not executed before its
owning example/task reaches its UITEST gate.

## Local reproduction

```powershell
cargo test -p explorer-jobs --lib extension_scheduler --locked --offline
cargo test -p explorer-extension-api --lib --locked --offline
cargo test -p explorer-extension-host --lib extension_job_runtime --locked --offline
cargo test -p explorer-extension-host --lib extension_result_cache --locked --offline
cargo test -p explorer-extension-host --lib extension_value_router --locked --offline
cargo test -p explorer-extension-host --lib ui_invalidation_batcher --locked --offline
```

The expected result is zero failures. The 1,000-item fixture reports exactly
1,000 scheduler completions and accepted values, bounded drain turns, at most 50
redraw notifications, and no value or invalidation after cancellation.
