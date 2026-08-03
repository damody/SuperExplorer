# lock-owner-host-service Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Restricted lock-owner query service
The host SHALL expose the existing Windows Restart Manager adapter through a read-only `LockOwnerQueryServiceV1` accepting a bounded list of capability-authorized item handles. Results SHALL be owned records containing PID, safe process/service display name, application type and safe status.

#### Scenario: File is locked by multiple processes
- **WHEN** a provider queries a file held by two helper processes
- **THEN** the service returns two bounded owned records without exposing native handles

### Requirement: No process-control operations
The public service SHALL NOT provide shutdown, terminate, close-handle or Restart Manager application-closing operations.

#### Scenario: Plugin attempts to terminate owner
- **WHEN** a plugin inspects the public host-service interface
- **THEN** no callable operation exists to terminate the process or close its handle

### Requirement: Deadline, cancellation and cleanup
Every query SHALL enforce deadline, cancellation, maximum input/results and guaranteed Restart Manager session cleanup across success, error and process-exit races.

#### Scenario: Query is cancelled
- **WHEN** cancellation occurs after a Restart Manager session starts
- **THEN** the session and temporary resources are released and the result is Cancelled rather than a plugin fault

### Requirement: Typed empty and unavailable results
No current owner SHALL produce a valid empty value. Access denial or protected-process limitations SHALL produce Unavailable; recoverable adapter failures SHALL produce PluginError.

#### Scenario: Lock was released before query completes
- **WHEN** the owner process exits during the query
- **THEN** the provider returns an empty/current result or an allowed race status without leaking the session

### Requirement: Short cache and shared refresh path
Lock-owner values SHALL use a short TTL. F5 and the extension's manual refresh command SHALL use the same host cache-invalidation/reschedule path and increment the current location refresh generation.

#### Scenario: Lock state changes around F5
- **WHEN** a helper acquires a lock, F5 is pressed, then releases the lock and F5 is pressed again
- **THEN** the column first displays the owner name and then clears it after the second refresh

### Requirement: Stale lock results are rejected
Query request and result SHALL carry location/item refresh generation. Switching folder/tab, disabling the feature or pressing F5 again SHALL cancel or ignore older work.

#### Scenario: F5 is pressed rapidly
- **WHEN** an older query finishes after the newest refresh query
- **THEN** its generation mismatch prevents it from overwriting the current cell
