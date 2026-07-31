# Post-Parity Roadmap Implementation Audit

Audit date: 2026-07-28  
OpenSpec change: `complete-explorer-post-parity-roadmap`  
Baseline commit: `9707637`

This audit maps the five roadmap capabilities onto the existing workspace before production work begins. It records reusable contracts, concrete gaps, intended ownership, validation layers, and boundaries that must not be crossed. It is an implementation baseline, not a claim that any roadmap capability is already complete.

## Capability Matrix

| Capability / phase owner | Reusable production contracts | Concrete gaps | Intended crates/modules | Required evidence |
|---|---|---|---|---|
| Session/settings persistence — phases 2–3 | Serializable `TabId`, `Generation`, `ShellItemId`, `LocationDescriptor`; `NavigationHistory`; per-tab `ViewSettings`; startup/shutdown coordinator; background jobs | No versioned snapshot, store, migration, atomic replacement, restore plan, window-placement contract, debounce coordinator, reset UI, or restart harness | `explorer-model::session` for pure schema/projection; `explorer-app::session_store` and lifecycle wiring; `explorer-jobs` for writes; existing UI actions/Folder Options for controls | Golden/migration/corruption tests, injected I/O failures, two-process restart headful matrix, 10-run clean/crash soak |
| Async thumbnails/cache — phases 4–5 | `ShellIconKey` already keys DPI/theme/association/overlay; owned icon pixel flow and versioned icon disk cache; bounded initial viewport icon requests; view modes and per-tab settings; job crate | No thumbnail-specific request key/source/status, visible/prefetch publication, deduplicating priority scheduler, Shell thumbnail adapter, cloud no-hydration branch, byte-cost memory LRU, thumbnail disk cache, invalidation or zoom flow | `explorer-model::thumbnail` and protocol events; `explorer-jobs::thumbnail_scheduler`; `explorer-shell-win::thumbnail`; `explorer-ui` row/tile texture integration | Fake source matrix, real common-file matrix, 1,000 次 fast-scroll and memory-pressure soak, DPI/theme/Explorer comparisons |
| Shell namespace navigation — phases 6–7 | `LocationDescriptor` already distinguishes filesystem, opaque namespace bytes, parsing names, and Known Folder IDs; Shell navigation can resolve PIDLs; breadcrumbs/history are descriptor-based; Shell icon path supports namespace items | No synthetic Home/Quick Access identity, explicit capability bitset, dynamic property/column model, full root tree, pin/recent service, ZIP/Library/Recycle/Network flows, or path/non-path operation matrix | `explorer-model::namespace` plus navigation/protocol extensions; `explorer-shell-win::navigation` and new property adapter; `explorer-ui::navigation_pane`, chrome and actions | Deterministic fake namespaces, real Known Folder/ZIP/Library/Recycle/Network fixtures, keyboard/UIA/interop matrices, slow-provider soak |
| Cross-process extension broker — phases 8–9 | Request correlation/cancellation; bounded Shell channels; diagnostics/error boundaries; current context-menu OLE worker; Windows Job Object/security feature flags already available; installer/release scripts | Current context-menu worker is a thread in the app process and cannot reclaim a permanently hung COM handler; no protocol crate, authentication, supervisor/worker binaries, restricted token/Job policy, quarantine or packaging | New `explorer-broker-protocol` library, `explorer-broker` supervisor and `explorer-broker-worker` binaries; app client/lifecycle; Shell operation adapters; installer/finalizer | Decoder fuzz/property corpus, controlled hostile workers, security review, install/upgrade/uninstall smoke, installed-extension interop and resource soak |
| Preview Handler host — phases 10–11 | `ViewSettings` already has per-tab preview visibility/width; View actions, pane splitter, placeholder render, focus/accessibility helpers and visual smoke already exist | Placeholder has no selection eligibility, handler lookup, lifecycle, broker message, cross-process HWND, focus/accelerator forwarding, DPI negotiation, fallback/quarantine or real-handler evidence | `explorer-model::preview`; broker protocol/worker handler host; app HWND adapter; existing UI pane upgraded behind typed state | Controlled handlers, lifecycle/fault tests, real handler inventory/matrix, keyboard/UIA/DPI/mixed-monitor evidence and leak soak |

## Existing Model Contracts and Path Assumptions

### Reusable invariants

- `TabId` is an opaque UUID and is serializable, hashable, and stable for one application session.
- `Generation` advances with checked arithmetic; tab navigation/search requests use it to reject stale results.
- `ShellItemId` is opaque provider-owned bytes. Debug output exposes only byte count, which is suitable for redacted diagnostics.
- `LocationDescriptor` is explicitly a descriptor rather than identity and currently supports `FileSystem`, `ShellNamespace`, `ParsingName`, and `KnownFolder`.
- `RequestContext` currently lives in `explorer-model` and contains `RequestId`, `TabId`, `Generation`, and `CancellationToken`; validation rejects cancellation and each mismatched correlation dimension.
- `NavigationHistory` commits only successful navigation and keeps Back/Forward independent per tab.
- `ViewSettings` is already tab-local and includes view mode, details/preview pane state and width, visibility toggles, sort descriptor, and Details widths.

### Gaps and constraints

- `LocationDescriptor` has no synthetic Home/Quick Access representation and no total byte/string validation at construction or deserialization.
- `ShellItemId::from_provider_bytes` rejects only empty values; roadmap boundaries require a centralized maximum size.
- `RequestContext` has no deadline and terminal-state contract. Because `TabId` and `Generation` are model types, the implementation must put platform-neutral deadline/terminal primitives in `explorer-common` and compose them into the model context without creating a common→model dependency.
- `ViewSettings` is not currently serializable. Persistence must use an explicit versioned persisted mapping rather than adding incidental serialization of all runtime fields.
- Search history is runtime state and is bounded to 32, but result snapshots and drafts are not durable roadmap state.
- Path accessors correctly return `None` for non-path descriptors; every roadmap command must preserve this capability-aware branch instead of using a path fallback.

## Shell and Native Ownership Audit

| Boundary | Current owner | Must remain apartment/thread local | Roadmap rule |
|---|---|---|---|
| Shell command endpoint | `ShellStaHandle` with bounded command/event channels (512/4096) | STA initialization, message pump, apartment-affine COM interfaces | Extend typed commands/events; do not create feature-specific unbounded channels |
| Location/PIDL resolution | `explorer-shell-win::navigation` | `IShellItem`, enumerators, raw/owned PIDLs and allocator-specific release | Only owned descriptor bytes or copied domain values cross the endpoint |
| Icon retrieval/cache | `icon` and `icon_disk_cache` | `HBITMAP`/GDI handles and Shell image-list interaction until copied | Thumbnail work must copy validated pixels before crossing and reuse cache ownership conventions |
| Context menu | Independent in-process OLE worker plus message/session routing | `IContextMenu/2/3`, `HMENU`, owner-draw/message state | Preserve behavior while phase 9 moves activation into a disposable process; a timeout alone cannot reclaim a hung in-process call |
| Watcher | Windows adapter worker | Native directory handles, overlapped buffers and cancellation handles | Emit owned identity/change events only; namespace providers without watchers use explicit bounded refresh |
| Search | Shell/Search adapter and bounded fallback | Search COM interfaces and provider enumerators | Long/untrusted providers become broker candidates; results retain request/tab/generation correlation |

Raw COM pointers, `PROPVARIANT`/`VARIANT`, `HBITMAP`, `HMENU`, preview handler instances, provider enumerators, and allocator-owned PIDLs must never enter model/UI state. HWND numeric values may be transported only through an explicit owner/lifetime contract; they are not general-purpose domain values.

## UI Reducer and Render Integration Audit

- `ExplorerAction` and one dispatcher already unify pointer and keyboard entry points; new commands must extend this reducer rather than mutate view state from render callbacks.
- `ViewSettings.preview_pane`, `preview_pane_width`, Details pane exclusion, pane splitter, placeholder preview surface, View menu entry, and UIA hooks already exist. Phase 10 replaces the placeholder content but preserves those public action/layout contracts.
- File views already compute viewport geometry and limit initial Shell icon requests for 100,000-row fixtures. Thumbnail publication should originate from the same virtualized visible range and expose only stable identities/keys to jobs.
- `focus`, `interaction`, `pointer_capture`, `geometry`, `theme`, and semantic token helpers already cover focus restoration, drag terminal paths, DPI-aware layout, light/dark/high contrast, and reduced motion. New panes/tiles/root nodes must consume these helpers.
- The current UI files `chrome.rs`, `layout.rs`, `lib.rs`, `theme.rs`, and several UITEST/smoke files have unrelated in-progress user changes. Initial roadmap implementation must avoid overwriting them and integrate only after checking current diffs.
- No roadmap persistence, Shell, codec, provider, or IPC operation may run from GPUI input, paint, layout, or accessibility callbacks.

## Packaging, Validation, and Evidence Audit

- The Cargo workspace currently has one application binary and no broker protocol/supervisor/worker members.
- `finalize_windows_artifact.ps1` builds and validates only `SuperExplorer.exe`, embeds the application manifest, checks x64 PE machine, and validates application VERSIONINFO.
- NSIS currently installs only `SuperExplorer.exe` and `Uninstall.exe`; broker binaries must later be explicit input definitions and owned install/uninstall files.
- `explorer-uitest` supports quick/full/interop/visual/soak suites, prerequisite-qualified SKIP, exclusive resources, required artifacts, process census, JSON/JUnit/Markdown and per-case logs.
- The current manifest covers active pre-roadmap OpenSpec capabilities only. The five new capability patterns and concrete cases must be registered in task 1.13 before their implementation can claim coverage.
- Existing reusable headful coverage includes lifecycle, keyboard/focus, breadcrumb UIA, mouse commands, view panes, columns, overlays, OLE interop and capability soak. New restart, thumbnail, namespace, broker and preview scripts must produce `report.json` plus capability-specific artifacts.
- Release evidence must record commit/dirty state, Windows build, provider/handler inventory, DPI/monitor topology, executable/protocol versions and truthful unavailable prerequisites.

## Phase Ownership and Exit Order

```text
shared contracts
      |
      v
session model/store -> restore UX -> session gate
      |
      v
thumbnail contracts/scheduler -> render/cache -> thumbnail gate
      |
      v
namespace model/Shell -> roots/commands -> namespace gate
      |
      v
broker protocol/process -> migration/packaging -> broker gate
      |
      v
preview model/worker/UI -> compatibility/soak -> preview gate
      |
      v
combined parity closure and handoff
```

A later phase may replace an execution backend behind the earlier typed contract, but it may not loosen identity, bounds, cancellation, stale-result, fallback, accessibility, or evidence requirements already accepted by a prior gate.

## Session Capability Gate Evidence (2026-07-28)

- `roadmap-session-settings` passed one bounded 10-cycle headful run: five orderly closes and five forced exits with recovery. It did not use the former single-tab/default-`C:\` launch probe.
- The seed contains three ordered tabs spanning owned fixtures on `C:\` and `D:\` plus `shell:MyComputerFolder`; the second tab is active and filesystem tabs retain independent history and view modes.
- Every launch compares UIA tab order/selection, focus and physical window bounds, plus durable locations, history, per-tab settings, active tab, work area, DPI and normal placement. Each launch also emits a screenshot.
- Address entry switches the target UI thread to English (US) and verifies LANGID `0x0409`, preventing the ambient IME from corrupting paths.
- The gate exposed and fixed cumulative window-position drift by persisting `Window::window_bounds()` restore coordinates instead of live client bounds.
- Formal evidence: `target/uitest-runs/utit-22079-7202/report.json` and `evidence/roadmap-session-settings/headful-report.json`; result PASS, 10/10 restored, no residual processes. Resource samples stayed within 124–125 threads, 1008–1010 handles, and approximately 98–99 MB working set.
- Rollback remains user-controlled through the restore toggle and reset-session/reset-view/reset-all actions; corrupt or unavailable snapshots continue through the validated fallback path.
