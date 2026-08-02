## 1. QoS policy and observations

- [x] 1.1 Add deterministic interaction-first priority, bounded result-drain, and overload/degradation policy types to `explorer-jobs`.
- [x] 1.2 Add bounded privacy-safe QoS counters for queue depth, overload, cancellation, stale results, saturation, and degradation transitions.
- [x] 1.3 Add unit tests for priority, non-blocking capacity, dual drain budgets, degradation hysteresis/recovery, and bounded observations.

## 2. UI frame-budget integration

- [x] 2.1 Route asynchronous file-view result integration through a reusable item/time frame budget without changing item identity or ordering.
- [x] 2.2 Reject superseded tab/request/navigation-generation results at the presentation boundary and record the outcome.
- [x] 2.3 Add deterministic UI tests proving completion bursts are split across frames while input and current-generation results remain available.

## 3. Blocking-domain isolation

- [x] 3.1 Audit navigation, file-operation, thumbnail/preview, and search/index execution paths and give each independently bounded capacity.
- [x] 3.2 Preserve the background file-operation STA path and add explicit correlation/diagnostics for domain saturation and stale completion.
- [x] 3.3 Add gated tests proving a stalled file operation or enrichment provider cannot delay a later navigation request.

## 4. Overload degradation

- [x] 4.1 Connect coordinator pressure to ordered shedding of maintenance, prefetch, off-screen enrichment, visual refinement, and optional animation.
- [x] 4.2 Add recovery scheduling after hysteresis thresholds are satisfied without reviving superseded work.
- [x] 4.3 Add unit and UI tests for degradation order, foreground preservation, and recovery.

## 5. UTIT stress verification

- [ ] 5.1 Add UTIT coverage for copy during navigation, full queues, rapid navigation replacement, competing search/enrichment work, tab closure, and shutdown with active work.
- [x] 5.2 Ensure contention tests use explicit start/release synchronization and emit QoS diagnostics on failure.
- [x] 5.3 Run formatting, targeted crate tests, UTIT, and the application build; document environment-only failures separately from regressions.

## 6. Visible enrichment regression repair

- [x] 6.1 Add regression tests for visible icon/thumbnail admission during degradation and batch-before-terminal navigation-child delivery.
- [x] 6.2 Preserve per-request FIFO for breadcrumb/search batches and their reliable terminals.
- [x] 6.3 Keep realized viewport icon/thumbnail work admitted and retry transient overload without reviving stale generations.
- [x] 6.4 Run targeted UI/Shell tests, formatting, OpenSpec validation, and the application build.
