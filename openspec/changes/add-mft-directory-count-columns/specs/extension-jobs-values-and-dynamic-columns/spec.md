## ADDED Requirements

### Requirement: Host-enforced folder fact admission
Before creating or dispatching a folder-item job for a data-column contribution with count limits, the Host SHALL require every corresponding built-in count column to be visible, then obtain exact current-generation directory facts and enforce every declared limit. A hidden required column, pending, partial, unavailable, cancelled, stale, or over-limit fact SHALL NOT invoke the extension callback. File-item jobs and contributions without a policy SHALL retain their existing behavior.

#### Scenario: Required count column is hidden
- **WHEN** a limited folder contribution is enabled but a built-in column corresponding to one of its declared limits is hidden
- **THEN** the Host publishes the dependency-not-enabled state, submits no directory-facts request on behalf of that contribution, ignores cached hidden facts for admission, and does not invoke the extension callback

#### Scenario: Exact facts satisfy every limit
- **WHEN** current exact File Count and Folder Count are each less than or equal to the contribution's declared maximum
- **THEN** the Host admits the folder job to the existing bounded extension scheduler

#### Scenario: One of two limits is exceeded
- **WHEN** a contribution declares both limits and either exact count exceeds its maximum
- **THEN** the Host rejects the folder job without invoking the extension callback

#### Scenario: Required facts are unavailable
- **WHEN** exact current-generation facts cannot be obtained for a limited folder contribution
- **THEN** the Host publishes a dependency-unavailable presentation state and does not invoke the extension callback

#### Scenario: Contribution has no admission policy
- **WHEN** an existing contribution omits both count limits
- **THEN** its file and folder jobs follow the pre-change dispatch path without an added directory-facts dependency

### Requirement: Host-owned admission presentation states
The dynamic-column projection SHALL distinguish dependency pending, dependency unavailable, and count-limit exceeded from plugin errors. These Host-owned states SHALL remain generation-safe and SHALL NOT be cached as plugin-produced values.

#### Scenario: Facts are pending
- **WHEN** a visible folder cell is waiting for exact facts required by its contribution
- **THEN** the cell displays its Host-defined dependency-pending text and no plugin work starts

#### Scenario: Old admission state completes late
- **WHEN** a pending or terminal admission state belongs to an obsolete request generation
- **THEN** it cannot replace the current cell state or dispatch current work
