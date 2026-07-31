## Context

The application already has typed request/correlation IDs, per-tab generations, cancellation, Shell STA ownership, stable item descriptors, view settings, diagnostics, unified UITEST suites, and recoverable UI boundaries. It is nevertheless filesystem-first: session state is not durable, rows mostly use icons rather than real thumbnails, many non-path Shell locations are incomplete, potentially hostile COM extensions remain in-process, and Preview Handlers are not hosted.

This umbrella change deliberately crosses application, model, Shell, job, UI, test, persistence, IPC, and packaging boundaries. Its primary stakeholders are Windows users expecting familiar File Explorer behavior and maintainers who need truthful phase-by-phase evidence rather than a long-lived partially wired rewrite.

## Goals / Non-Goals

**Goals:**

- Deliver the five capabilities in dependency order while keeping every phase runnable and independently verifiable.
- Match public Windows File Explorer interaction patterns for commands, mouse, keyboard, focus, accessibility, navigation, thumbnails, and preview behavior where public APIs permit it.
- Preserve stable identity, stale-result isolation, cancellation, crash recovery, and bounded resource use across persistence and process boundaries.
- Extend existing typed contracts instead of adding filesystem-path shortcuts or parallel UI-only state.
- Produce deterministic, fault-injection, headful, real-Shell, performance, and soak evidence for every capability.

**Non-Goals:**

- Pixel-copying private Explorer assets or depending on undocumented Explorer internals.
- Deep OneDrive Files On-Demand integration, enterprise network credential/offline management, FTP/SFTP/ADB providers, or complete Security/Sharing property pages.
- Persisting clipboard ownership, selections, transient inline editors, in-flight file operations, live COM objects, preview instances, or search result snapshots.
- Supporting multiple independent top-level Explorer windows in the first persistence schema; the schema may evolve without breaking stored data.

## Decisions

### 1. One umbrella change with five strict phase gates

Implementation order is session persistence, thumbnails, namespace navigation, broker isolation, then preview hosting. Each phase has its own capability spec, tests, evidence, and quality gate. Later phases may replace an execution backend behind an earlier typed contract, but may not invalidate earlier public behavior. A single monolithic implementation was rejected because it would make failures and parity claims impossible to attribute; five unrelated changes were rejected because the shared identity, cache, cancellation, and broker contracts would drift.

### 2. Persist reconstructible descriptors, never runtime objects

State is stored in versioned envelopes under `%LOCALAPPDATA%\RustGpuiExplorer\state\v1`. The store contains reconstructible location descriptors, bounded history, window placement, active-tab identity, and per-tab `ViewSettings`. Writes use a same-directory temporary file, flush, atomic replacement, and a last-known-good backup. Startup validates schema and invariants before replacing the default model. JSON is selected for inspectability and migration simplicity; an embedded database was rejected because the data is small, replaced as a coherent snapshot, and has no query workload.

### 3. Shell item identity remains the common spine

Filesystem paths, Known Folder IDs, absolute PIDL bytes, parsing names, and synthetic Home/Quick Access identifiers are descriptors for a typed `ShellItemId`; none is universally assumed to be a path. All enumeration, breadcrumbs, history, thumbnails, commands, persistence, and previews carry item identity plus tab/generation/request context. Synthetic roots expose explicit capabilities and resolve their children to real Shell items.

### 4. Thumbnail work is viewport-driven and cost-bounded

The UI publishes visible and small near-visible ranges; a scheduler deduplicates `(identity, physical size, scale, mode, source generation)` requests, prioritizes the active viewport, and cancels work with no consumers. Shell retrieval produces owned pixel payloads before crossing apartment/thread boundaries. Memory cache eviction uses decoded byte cost; disk cache is lazy, checksummed, versioned, and enabled only after measured benefit. Automatic extraction must not hydrate cloud placeholders. Until broker migration, risky or blocking providers fall back to icons after a deadline; after the broker phase, codec/provider extraction executes in disposable workers.

### 5. Namespace support is capability-driven

Navigation exposes Home, Quick Access, Desktop/Known Folders, This PC, drives, Libraries, ZIP folders, Recycle Bin, Network root, and compatible third-party namespaces. Commands query item/container capabilities before enabling open, pin, rename, delete, restore, paste, drop, search, or property actions. Home is a project-owned aggregation of pinned and recent reconstructible items, not a fake filesystem directory. Unsupported operations remain disabled with an accessible explanation.

### 6. Untrusted COM work runs in disposable broker workers

A supervised broker executable uses authenticated local IPC with a versioned handshake, bounded frames, explicit request types, correlation/generation, deadline, cancellation, progress, and exactly one terminal event. The supervisor launches disposable restricted-token workers inside Windows Job Objects for extension activation. A hung or crashed worker can be terminated and replaced without ending the app or poisoning the Shell STA. CLSID/type failure history drives bounded quarantine with a user-visible retry path. A permanent in-process worker thread was rejected because Windows provides no safe way to terminate a hung COM call.

### 7. Broker access is least-authority and auditable

The app sends only the operation, reconstructible item descriptors or duplicated handles required for that operation, size/theme metadata, and a per-session authentication secret inherited through owned handles. IPC rejects unknown versions/types, oversized payloads, invalid state transitions, and late replies. Workers run without unnecessary privileges, are memory/CPU/process-count constrained, and cannot send arbitrary UI commands. Logs retain correlation and handler identity but redact unnecessary full paths and file contents.

### 8. Preview is a broker-owned lifecycle

Selection changes are debounced and create a generation-scoped preview request. The broker resolves the registered handler and negotiates initialization by item, file, or stream. A disposable worker owns the handler and its native preview surface; the app owns only a preview host boundary and forwards bounds, focus, accelerators, theme/DPI notifications, and unload. Cross-process child-window attachment is attempted only with compatible DPI awareness; otherwise the pane shows properties/fallback UI. Switching selection, tab, pane visibility, or window shutdown unloads exactly once and suppresses late events.

### 9. Explorer parity is behavioral and evidence-based

Every visible capability must include mouse, keyboard, focus restoration, UIA role/name/state/action, high-contrast, light/dark, 100/125/150/175/200% DPI contracts, compact-window behavior, error/fallback states, and real Explorer comparison where feasible. Exact private visuals are not required, but geometry, command availability, state transitions, Shell results, and public interaction semantics must be measured. Hardware-dependent cases remain truthful skips rather than simulated passes.

### 10. Existing UITEST is the release gate

Each requirement maps to manifest cases in quick/full/interop/visual/soak suites. Deterministic fakes cover protocol and failure matrices; real Windows fixtures cover Known Folders, ZIP, Libraries, Recycle Bin, installed safe extensions, common thumbnail types, and preview handlers. Soaks monitor process/thread/GDI/User handles, working set, cache bytes, outstanding requests, worker restarts, and terminal-event balance.

### 11. Session restart evidence compares visible multi-tab state

The session capability gate uses a real two-process UIA harness, not a default-location launch probe. The first process establishes at least three ordered tabs spanning distinct filesystem fixtures and a reconstructible Shell namespace, independent histories/view settings, a non-first active tab, known focus, and non-default reachable bounds. The harness saves UIA, screenshots, and the durable envelope before shutdown; every recovery process must expose equivalent visible and durable state. A restore log marker alone is insufficient. The restart soak is ten cycles total, split evenly between orderly and forced exits.

## Risks / Trade-offs

- [Umbrella scope becomes unreviewable] → Enforce five phase gates, per-capability specs, bounded commits, and no work on a later phase before the prior gate passes.
- [Stored PIDLs or paths become stale] → Validate every descriptor during reconstruction, preserve partial sessions, and fall back to the nearest valid location without blocking startup.
- [Thumbnail fan-out harms scrolling] → Request only visible/near-visible items, cap queues and decoded bytes, cancel abandoned work, and retain icon-first rendering.
- [Cloud placeholders hydrate unexpectedly] → Prefer cache-only thumbnail flags and icon fallback; never fetch file content solely because it became visible.
- [Shell namespace behavior varies by Windows build/provider] → Query capabilities dynamically, record build/provider evidence, and expose unsupported behavior rather than guessing.
- [Restricted broker cannot access required content] → Use the least-capable worker profile that still supports the handler and pass explicit handles/descriptors; fail closed to a fallback pane.
- [Cross-process preview HWND has DPI/focus incompatibility] → Negotiate awareness, isolate native hosting behind one adapter, and use property/icon fallback when attachment is unsafe.
- [Crashing handlers create restart loops] → Apply per-handler backoff/quarantine, bounded retries, explicit user retry, and durable diagnostics.
- [Disk caches leak sensitive metadata] → Store opaque hashed keys and owned pixels only, bound retention, provide clear-cache controls, and never persist preview content.

## Migration Plan

1. Introduce persistence contracts and a disabled-by-version loader; write new state only after validation tests pass. Rollback ignores the versioned directory and starts with defaults.
2. Add thumbnail scheduling and memory cache behind icon fallback, then enable disk caching after telemetry confirms bounded behavior. Rollback disables thumbnail mode without changing item models.
3. Add namespace descriptors and roots incrementally, starting with Known Folders/This PC before synthetic Home/Quick Access and slower providers. Rollback hides unsupported roots while filesystem tabs continue working.
4. Ship the broker beside the main executable, validate protocol/packaging, then migrate context menu and thumbnail/provider work one class at a time. Rollback disables the affected broker route and shows a safe unavailable state; it does not restore unsafe in-process activation.
5. Enable Preview Pane only after broker crash/hang recovery, packaging, DPI, focus, and resource soaks pass. Rollback hides/disables preview while other view settings remain valid.
6. Archive the umbrella change only when all five capability gates and the full regression runner pass, with hardware/environment limitations recorded.

## Open Questions

None block implementation. Cache budgets, history limits, debounce intervals, worker Job Object limits, and quarantine durations must be derived from measured fixtures and centralized as versioned configuration rather than embedded across modules.
