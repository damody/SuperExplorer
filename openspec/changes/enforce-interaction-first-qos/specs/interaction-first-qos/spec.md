## ADDED Requirements

### Requirement: Foreground interaction never waits for background capacity
The system SHALL submit UI-originated work without blocking on queue capacity, Shell calls, storage, networking, or background result delivery. Overload MUST delay or reject lower-priority work instead of waiting on the UI thread.

#### Scenario: Background queue is full
- **WHEN** a background queue is at capacity and the user navigates, scrolls, selects an item, or switches tabs
- **THEN** the foreground handler returns without waiting for queue capacity and the interaction remains available

### Requirement: Work follows interaction-first priority
The system SHALL schedule visible interaction before current-directory completion, current-directory completion before background-tab prefetch, and prefetch before maintenance work.

#### Scenario: Visible and background work compete
- **WHEN** visible-viewport work is submitted while prefetch and maintenance jobs are queued
- **THEN** the visible-viewport work is selected first without discarding required foreground work

### Requirement: UI result integration is frame bounded
The system SHALL bound asynchronous result integration by both an item limit and a 16 ms per-frame time budget targeting 60 FPS. Remaining results MUST be retained or safely superseded for a later frame.

#### Scenario: A completion burst reaches the UI
- **WHEN** more results arrive than can be integrated within one frame budget
- **THEN** the UI integrates only the bounded batch and remains available to input before continuing the remainder

### Requirement: Superseded work cannot mutate current presentation
The system SHALL correlate asynchronous results with their tab, request, and navigation generation and MUST reject results whose owner is closed, cancelled, or superseded.

#### Scenario: Rapid navigation completes out of order
- **WHEN** an older navigation result completes after a newer navigation has become current
- **THEN** the older result is counted as stale and does not alter the current location or file view

### Requirement: Blocking domains have isolated capacity
Navigation Shell work, file operations, thumbnail/preview work, and search/index work SHALL use independently bounded execution capacity so stalled work in one domain cannot consume the capacity required by another.

#### Scenario: File copy is deliberately stalled
- **WHEN** a file-operation worker is held after copy has started and the user navigates to another folder
- **THEN** navigation completes before the copy worker is released

### Requirement: Overload degrades optional work and recovers
The system SHALL expose a deterministic degradation level that sheds maintenance, background prefetch, off-screen enrichment, and optional visual refinement in that order. Direct interaction, navigation, cancellation, and file-operation progress MUST remain available, and optional work SHALL resume after sustained pressure recovery.

#### Scenario: Sustained queue pressure subsides
- **WHEN** pressure advances degradation and later falls below the recovery threshold
- **THEN** the system preserves foreground work throughout and returns toward the normal degradation level without oscillating each sample

### Requirement: QoS observations are bounded and privacy safe
The system SHALL expose bounded latency distributions, queue depth/high-water marks, overload, cancellation, stale-result, saturation, and degradation-transition counts without recording paths, item names, or item identities.

#### Scenario: A responsiveness assertion fails
- **WHEN** a UT or IT responsiveness gate fails
- **THEN** diagnostics report the QoS measurements needed to identify the saturated domain without exposing filesystem content

### Requirement: UTIT verifies real contention without timing luck
The UTIT suite SHALL cover full queues, bounded draining, cancellation, stale results, degradation/recovery, and isolated stalled workers with explicit start/release synchronization. Integration coverage SHALL include copy during navigation and competing large-directory, search, and enrichment workloads.

#### Scenario: Copy competes with navigation
- **WHEN** UTIT starts a controlled real or faithful file copy, waits until the operation worker is occupied, and then requests navigation
- **THEN** navigation is observed before the test permits the copy to finish and failures include QoS diagnostics
