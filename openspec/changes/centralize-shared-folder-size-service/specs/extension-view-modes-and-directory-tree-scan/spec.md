## MODIFIED Requirements

### Requirement: Host-owned recursive tree scan
The host-owned shared Folder Size Snapshot Service SHALL accept an authorized location, recursive/reparse/hard-link/metadata policy, deadline, quotas, cancellation and refresh generation. It SHALL perform filesystem/index I/O off the GPUI thread, coalesce compatible aggregate and tree consumers, and return bounded add/update/remove/partial/subtree-complete/scan-complete deltas.

#### Scenario: Deep tree scans incrementally
- **WHEN** Size Map requests complete recursion while Folder Size requests aggregates for the same root
- **THEN** direct children and known sizes appear first, deeper deltas aggregate later, both consumers share one physical scan, and the view remains interactive

### Requirement: Owned tree nodes and terminal states
Shared tree nodes SHALL contain opaque node/item and parent IDs, name, kind, direct and recursive logical bytes, optional allocated bytes, scan state and generation without arbitrary unauthorized paths or native handles. Terminal states SHALL include complete, partial, cancelled, unavailable, resource-limited and failed.

#### Scenario: Subtree is inaccessible
- **WHEN** the shared service encounters a permission-denied directory
- **THEN** the scan/view retains other results, marks that subtree and affected aggregate partial, and does not publish an exact zero

### Requirement: Refresh generation and stale layout rejection
F5 SHALL increment current location refresh generation, invalidate prior shared aggregation and start or join a new compatible service request. Requests, deltas and layout commits SHALL carry generation; switching view/folder/tab or disabling the final consumer SHALL cancel or ignore older work.

#### Scenario: Two scans finish out of order
- **WHEN** an older shared scan completes after a newer scan
- **THEN** its deltas, aggregate values and Size Map layout are rejected
