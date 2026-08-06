## Context

Durable session restoration reconstructs tab identities, history, locations, view settings, ordering, and the active tab while intentionally discarding transient directory snapshots and request scopes. `ExplorerRoot` immediately calls the existing active-location load for the restored active tab. A restored background tab remains `DirectoryState::Idle`; `AppViewState::activate_tab` and keyboard tab cycling only change active identity and focus, so no directory command is submitted when that tab later becomes visible.

The implementation must preserve interaction-first startup behavior, use the existing `ExplorerService` boundary, and avoid duplicate enumeration or a second request lifecycle. It must also coexist with unrelated in-progress work in the dirty worktree.

## Goals / Non-Goals

**Goals:**

- Load the active restored tab during root construction.
- Submit one normal navigation command when an idle restored background tab first becomes active.
- Apply the same rule to pointer activation, next/previous cycling, and the newly active tab after close.
- Preserve generation correlation, cancellation, stale-result rejection, and explicit refresh recovery.
- Prove the behavior with unit tests and a real two-process UTIT restart.

**Non-Goals:**

- Eagerly enumerate every restored tab at startup.
- Persist directory rows, icons, thumbnails, or extension values in the session envelope.
- Change the session schema, directory service API, or plugin ABI.
- Automatically retry genuine terminal directory errors.

## Decisions

### Use one state-owned idle-tab load predicate

`AppViewState` will expose a narrowly scoped operation that starts the active location load only when the active directory state is `Idle`. This centralizes the semantic distinction between an unloaded restored tab and tabs in `Loading`, `Ready`, or `Error`. The operation returns the existing `ExplorerCommand::Navigate`; it does not submit work itself.

Checking only for an absent visible snapshot was rejected because a valid loading request can have no snapshot yet and would be duplicated. Treating `Error` as eligible was rejected because changing tabs would become an implicit retry loop.

### Submit after every action that can change the active tab

The root action dispatcher will run one shared post-action helper after `ActivateTab`, `NextTab`, `PreviousTab`, `CloseActiveTab`, and `CloseTab`. The helper asks state for an idle active-tab command and submits it through `ExplorerRoot::submit_command`. `NewTab` retains its existing pending command path so the change does not create double submission.

Placing submission inside `AppViewState::activate_tab` was rejected because presentation state must not own the service endpoint. Adding frame-pump polling was rejected because activation is the exact lifecycle edge and polling complicates duplicate suppression.

### Keep startup immediate and background restoration lazy

The restored root constructor continues to submit the active location before first render. Background tabs load only on first activation. This matches File Explorer's visible behavior while bounding startup directory, icon, thumbnail, and extension-column work.

### Preserve the existing correlated failure path

If service admission fails, `submit_command` converts the failure into the existing correlated directory error. The tab does not remain `Idle`, and activation does not spin. F5 remains the deliberate retry action. No new retry timer or connection state is introduced.

### Validate through state, root seams, and headful restart

Unit tests will prove state eligibility and duplicate suppression, while root-level tests will prove all active-tab-changing actions submit the pending load. The UTIT will use an isolated profile, create a two-tab durable session, restart the application, and activate the restored background tab with real UI Automation. The report and screenshots are blocking evidence.

## Data Flow

1. Session restore resolves each durable location and constructs transient-free `TabState` values in `Idle`.
2. Root construction starts the active tab's normal navigation request and submits it to `ExplorerService`.
3. A tab-changing action commits a new active identity.
4. The post-action helper asks state to begin a load only if the new active tab is still `Idle`.
5. The existing service pump applies correlated location, directory-batch, finished, or failed events.
6. Subsequent activations observe `Loading`, `Ready`, or `Error` and submit nothing.

## Risks / Trade-offs

- **[Risk] A broad post-action hook could duplicate new-tab loading.** → Exclude `NewTab` and retain its established pending-command path; add exact submission-count tests.
- **[Risk] Closing a background tab could unnecessarily inspect the unchanged active tab.** → The idle predicate makes this a no-op unless the active tab genuinely has never loaded.
- **[Risk] A service admission failure could leave a permanent disconnected placeholder.** → Route through the existing correlated failure synthesis and assert the tab transitions out of `Idle`.
- **[Risk] Headful session tests can pass from stale profile state.** → Use an isolated owned profile, verify two distinct fixture paths, and preserve before/after UIA evidence.
- **[Trade-off] Background tabs are not prewarmed.** → Accepted to avoid startup I/O bursts; first activation pays normal directory latency while immediately showing Loading rather than Disconnected.

## Observability and Security

No paths or directory contents are added to diagnostics beyond existing session and UTIT artifacts. Production diagnostics may record command submission failure through the existing privacy-safe service endpoint message. The change adds no process access, network access, credentials, or external writes.

## Migration Plan

No data migration is required. Deploy as a UI/state behavior change. Rollback consists of reverting the idle-tab helper, post-action submission, and associated tests; persisted sessions remain compatible in both directions.

## Evidence and Adjustment Policy

Evidence is indexed under `openspec/changes/autoload-restored-tab-directories/evidence/`. An A-level refinement may split tasks or adjust commands without changing requirements or gates. A B-level correction within the approved scope must update design, spec, tasks, and stale dependent evidence before work resumes. A C-level change to public behavior, session schema, loading strategy, required UTIT evidence, permissions, or blocking thresholds requires user approval. No blocking gate may be weakened silently.

## Open Questions

None. The approved source design fixes lazy background loading, explicit error retry, and two-process UTIT as binding decisions.
