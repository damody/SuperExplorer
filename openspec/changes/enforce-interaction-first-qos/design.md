## Context

SuperExplorer already has bounded job primitives, file-view callback measurements, virtualized views, cancellable requests, and background file-operation STA workers. These protections are currently local to individual features. Work from different domains can still compete for shared execution or arrive at the UI in an unbounded burst, so being asynchronous does not by itself guarantee foreground responsiveness.

The design must work with GPUI's single UI thread and Windows Shell apartment rules. It must preserve typed command/result correlation and avoid recording paths or item identities in diagnostics.

## Goals / Non-Goals

**Goals:**

- Make interaction and navigation independent from background workload completion.
- Centralize bounded priorities, cancellation, backpressure, degradation, and observations.
- Bound the amount of asynchronous result integration performed in one frame.
- Isolate blocking work domains and reject stale results at the presentation boundary.
- Provide deterministic UT/IT gates for foreground responsiveness under contention.

**Non-Goals:**

- Guarantee fixed storage or network completion latency.
- Hide file-operation progress or errors.
- Immediately move every Shell integration into a separate process.
- Replace existing typed service APIs or file-view virtualization.

## Decisions

### Central policy with domain-specific executors

`explorer-jobs` will own reusable QoS policy and bounded priority/result primitives. Domain executors remain in their owning crates because Shell STA, thumbnails, previews, and search have different threading rules. This is preferred over one global thread pool, which could let a stalled domain exhaust all workers, and over independent feature queues, which cannot enforce a consistent overload policy.

Priority order is visible interaction, current directory, prefetch, then maintenance. Submission is always non-blocking and returns the rejected work on overload.

### Generation-safe delivery

Every queued result is associated with its owning tab/request/navigation generation. Result integration validates that identity even when cancellation was requested. This makes correctness independent from how quickly external work observes cancellation.

### Frame-budgeted result integration

The UI drains result queues until either a configured item limit or the 16 ms frame-integration budget is reached. Remaining results stay queued for a later frame. This targets 60 FPS and is preferred over draining on every notification, which can turn a background completion burst into a UI stall.

### Observable degradation state

Queue saturation and repeated frame-budget exhaustion advance a deterministic degradation state. Recovery uses lower thresholds than entry so behavior does not oscillate. Degradation first removes maintenance, prefetch, and off-screen enrichment; direct interaction and navigation are never shed.

### Isolated blocking domains

Navigation Shell work, file operations, thumbnail/preview work, and search/index work have independent bounded capacity. The existing background file-operation STA path is retained and covered by the common contract. Permanently hung third-party providers remain candidates for later process isolation.

### Privacy-safe measurements

Counters include latency distributions, current/high-water queue depth, overload, cancellation, stale-result rejection, worker saturation, and degradation transitions. They contain correlation IDs only in trace events and never include paths or item names.

## Risks / Trade-offs

- [Central policy adds coordination complexity] → Keep the policy deterministic, side-effect free where possible, and cover every transition with unit tests.
- [Strict shedding can delay thumbnails or background tabs] → Preserve visible placeholders and automatically recover when pressure falls.
- [Wall-clock integration tests can be flaky] → Use explicit worker start/release gates and measure foreground independence rather than storage speed.
- [A third-party in-process provider can remain permanently hung] → Bound its concurrency now and retain an executor interface that can move behind process isolation later.
- [Incremental migration leaves temporary mixed scheduling] → Route one domain at a time and keep existing behavior as the fallback until its tests pass.

## Migration Plan

1. Add QoS policy, bounded result draining, observations, and deterministic unit tests without changing call sites.
2. Adopt frame-budgeted delivery in the file view and expose diagnostics.
3. Register domain capacity and generation checks for navigation, operations, thumbnails/previews, and search.
4. Enable degradation and recovery after observations validate thresholds.
5. Add contention IT coverage and make the interaction-first gates part of UTIT.

Each phase is independently reversible by returning the affected call site to its previous queue while leaving the compatible policy types in place.

## Open Questions

No blocking design questions remain. Initial limits are configuration constants and can be tuned from collected privacy-safe distributions without changing the contract.
