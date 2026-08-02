# Interaction-First QoS Design

## Goal

SuperExplorer must keep input, navigation, scrolling, selection, and tab switching responsive while enumeration, copy/move, search, thumbnail, preview, indexing, or Shell work is active. Under overload the application may delay or simplify content, but it must not freeze interaction.

The guarantee is scoped to work controlled by SuperExplorer. Faulting storage, exhausted system memory, and permanently hung third-party components cannot be made latency-free; they must be isolated so the UI degrades locally and remains usable.

## Responsiveness contract

- UI callbacks have a 4 ms p95 budget.
- UI result integration consumes no more than 16 ms per frame to target 60 FPS.
- Reliable terminals never overtake earlier batches from the same request.
- Icons and thumbnails for realized rows remain current-directory work during degradation; only off-screen refinement may be shed.
- A navigation action displays its new location or a loading state within 50 ms.
- UI dispatch never waits for worker capacity, Shell calls, storage, networking, or result delivery.
- Every asynchronous result carries tab, request, and navigation-generation identity; stale results are rejected.
- Copy/move and Clipboard paste cannot occupy the Shell execution resource required by navigation.

## Architecture

The UI mutates immediate interaction state and submits typed work to a central QoS coordinator. The coordinator owns bounded priority lanes, cancellation, overload policy, and observable queue statistics.

Priority order is:

1. visible viewport and direct interaction;
2. current-directory work;
3. background-tab prefetch;
4. thumbnails, previews, indexing, and maintenance.

Execution resources are isolated by blocking domain: navigation Shell STA, file-operation STA workers, thumbnail/preview workers, and search/index workers. No domain may exhaust another domain's capacity. Results enter bounded queues and are integrated by the UI in bounded batches within the frame budget.

## Cancellation and generation safety

Navigation increments the owning tab's generation and cancels superseded work. Workers are cooperatively cancellable, but correctness does not rely on prompt cancellation: the result boundary validates tab, request, and generation before mutating presentation state. Closing a tab invalidates all of its outstanding consumers.

## Backpressure and degradation

When queues or frame budgets are saturated, the coordinator sheds work in this order:

1. pause maintenance and background indexing;
2. cancel background-tab prefetch;
3. cancel off-screen thumbnails and previews;
4. render generic icons for visible items and refine later;
5. reduce the number of directory results integrated per frame;
6. suspend nonessential animation.

Input, navigation, scrolling, cancellation, existing-content selection, and file-operation progress remain available at every degradation level. The coordinator automatically recovers when pressure falls below bounded hysteresis thresholds.

## Errors and lifecycle

Shell, network, metadata, thumbnail, and preview failures produce local retryable errors. A stalled worker cannot consume all capacity in its domain. Background panics terminate the affected task and are reported through existing diagnostics. Shutdown cancels work, stops accepting results, and bounds worker cleanup without making the main window wait indefinitely.

## Observability

Privacy-safe counters record callback and result-integration distributions, queue depth/high-water marks, shed/cancelled/stale result counts, active degradation level, worker saturation, and first-visible-result latency. Traces contain correlation identifiers but no paths or item names.

## Verification

Deterministic unit tests use explicit start/release gates rather than timing luck to verify priority order, non-blocking overload, bounded frame draining, stale-result rejection, cancellation, worker-domain isolation, degradation, and recovery.

Integration tests cover large real copies while navigating, 100,000-item enumeration, simultaneous copy/search/thumbnail activity, rapid path replacement, slow or disconnected storage, stalled Shell metadata/thumbnail providers, full queues, tab closure, and shutdown with active work. Timing assertions measure foreground independence from a deliberately blocked background worker and emit QoS diagnostics on failure.

## Delivery sequence

1. Extend common QoS types and privacy-safe performance counters.
2. Add the bounded coordinator and deterministic policy tests.
3. Route UI result integration through a per-frame budget.
4. isolate blocking domains and enforce generation validation.
5. Add overload degradation/recovery and full UT/IT stress coverage.
