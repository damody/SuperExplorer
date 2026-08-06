## ADDED Requirements

### Requirement: Shared aggregate and tree snapshots
The host SHALL provide aggregate and tree projections from one normalized folder snapshot and SHALL coalesce compatible concurrent consumers so Folder Size and Size Map do not start duplicate physical scans.

#### Scenario: Two consumers open together
- **WHEN** Folder Size and Size Map request the same root and generation concurrently
- **THEN** both receive their required projections and diagnostics record one physical scan

#### Scenario: One consumer disables
- **WHEN** Folder Size is disabled while Size Map still holds the snapshot lease
- **THEN** Size Map remains current and the shared snapshot is not prematurely cancelled or evicted

### Requirement: Explorer-safe filesystem semantics
Every backend SHALL enforce canonical-root containment, SHALL represent but not recursively follow directory reparse points, and SHALL preserve logical per-directory-entry hard-link sizing unless an explicitly different future policy is selected.

#### Scenario: Junction points outside the root
- **WHEN** a directory junction below the requested root targets another directory or volume
- **THEN** the junction has no descendant contribution and the service neither escapes the root nor double-counts the target

### Requirement: Lazy least-privilege backend selection
The service SHALL use a valid snapshot first, then attempt an elevated MFT helper for an uncached local-NTFS demand, then an eligible Everything adapter, then bounded recursive traversal. The main process SHALL remain non-elevated, and decline/failure SHALL fall through without disabling consumers.

#### Scenario: User declines UAC
- **WHEN** the first local-NTFS MFT helper elevation is declined
- **THEN** the service records the decline without repeated simultaneous prompts and completes through Everything or recursive fallback

#### Scenario: Everything is unavailable
- **WHEN** MFT is unavailable and Everything IPC cannot provide an eligible result
- **THEN** bounded recursive traversal produces the snapshot or an explicit partial terminal state

### Requirement: Generation-safe cache and invalidation
Memory and disk snapshots SHALL be schema-versioned, bounded, keyed by volume/root identity and semantic policy, pinned by active leases, invalidated by watcher/manual generation, and rejected when stale. MFT incremental reuse SHALL require a continuous validated journal checkpoint.

#### Scenario: Old scan finishes after refresh
- **WHEN** F5 advances the refresh generation before an older backend completes
- **THEN** the older result is rejected and cannot replace the current snapshot

### Requirement: Typed progress and failure states
Snapshots SHALL distinguish complete, partial, cancelled, unavailable, resource-limited, and failed states; inaccessible subtrees SHALL not become false exact zeros or fail unrelated subtrees.

#### Scenario: One subtree is inaccessible
- **WHEN** traversal cannot read one descendant directory
- **THEN** accessible nodes remain available and affected ancestors are marked partial with a bounded diagnostic

### Requirement: Backend equivalence and observability
MFT and Everything adapters SHALL pass deterministic equality fixtures against the recursive reference before eligibility. The host SHALL expose privacy-safe counters for attempts, selected method, fallback reason, physical scans, subscribers, cache hits, nodes, elapsed time, partial state, and stale rejection.

#### Scenario: Fast index violates reparse policy
- **WHEN** an accelerated result includes descendants reachable only through a directory reparse point
- **THEN** the result is ineligible, the fallback continues, and no inconsistent snapshot is published

### Requirement: Exact zero requires completeness proof
The host SHALL publish and cache an exact zero only when the selected backend proves that the complete requested subtree was observed under the active semantic policy. A shallow, truncated, stale, or otherwise unproven accelerated result SHALL be rejected and SHALL fall back rather than become `0 B`.

#### Scenario: Shallow index returns only child directories
- **WHEN** an accelerated index omits descendants and therefore reports zero file bytes
- **THEN** the adapter rejects the candidate and recursive fallback returns the reference value

### Requirement: Installed MFT service is operational
The installer SHALL create or configure `SuperExplorerMft` as an automatic LocalSystem Windows service, start it, and verify SCM reports `RUNNING` before installation succeeds. The application SHALL consume only fresh, bounded, completed service records and SHALL otherwise use a correctness-preserving fallback.

#### Scenario: Service cannot start
- **WHEN** service creation, configuration, startup, or RUNNING verification fails
- **THEN** installation aborts with a diagnostic instead of installing a build that silently publishes false folder sizes

### Requirement: Everything eligibility requires complete subtree evidence
The Everything adapter SHALL remain ineligible for folder snapshots unless its query and validation prove complete subtree coverage and equality with recursive reference semantics.

#### Scenario: Completeness cannot be established
- **WHEN** Everything IPC succeeds but completeness cannot be established
- **THEN** the service records an ineligible-backend fallback and performs recursive traversal

### Requirement: Folder modified-date cache identity
The shared service SHALL cache every complete folder-size snapshot for all consumers and SHALL reuse it across refresh generations while the canonical folder modified date is unchanged. A changed modified date SHALL invalidate that folder's cached snapshot and trigger recalculation.

#### Scenario: Refresh without folder modification
- **WHEN** Folder Size or Size Map requests a newer generation and the folder modified date is unchanged
- **THEN** the shared cached snapshot is reused without another physical scan

#### Scenario: Folder modified date changes
- **WHEN** the folder modified date differs from the cached identity
- **THEN** the old snapshot is not reused and the shared service recalculates it

### Requirement: MFT aggregate projection is precomputed and bounded-parallel
The Host SHALL precompute complete recursive folder aggregates once per loaded MFT volume index using at most eight worker threads. Folder Size aggregate lookups SHALL reuse those totals without materializing a normalized tree or issuing per-descendant filesystem metadata calls. Consumers requiring Size Map nodes SHALL use the separate bounded tree projection.

#### Scenario: Several sibling folders request Folder Size
- **WHEN** the MFT service index for their volume is valid and loaded
- **THEN** one bounded-parallel aggregate build supplies constant-time totals for every sibling request

### Requirement: Folder Size backend is visible
The status bar SHALL display the Host-observed Folder Size source as Host cache, MFT service, or Recursive scan and SHALL indicate active work. When active requests use mixed sources, Recursive scan SHALL take precedence over accelerated sources.

#### Scenario: MFT is unavailable and traversal falls back
- **WHEN** Folder Size is actively using recursive traversal
- **THEN** the bottom-right status identifies `Folder size: Recursive scan...`
