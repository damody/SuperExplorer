## ADDED Requirements

### Requirement: Script discovery and activation lifecycle
The system SHALL discover Lua files from the application automation scripts directory and SHALL support `always` and `temporary` activation modes.

#### Scenario: Always script restores
- **WHEN** the application starts with a valid script configured as `always`
- **THEN** the system registers and enables that script before emitting `app.started`

#### Scenario: Temporary script does not restore
- **WHEN** the application restarts after a temporary script was enabled
- **THEN** the script remains disabled until the user enables it again

### Requirement: Isolated Lua 5.4 runtime
The system SHALL execute each enabled script in an independent Lua 5.4 VM with bounded memory, watchdog interruption, and no io/os/package/debug/native-module surface.

#### Scenario: Runaway script is isolated
- **WHEN** one script exceeds its continuous execution or memory limit
- **THEN** its task fails without stopping another script or the GPUI event loop

### Requirement: Immutable task working directory
The system SHALL create a distinct task for every trigger and SHALL capture the active Explorer tab directory before dispatch queuing.

#### Scenario: Navigation after trigger does not retarget output
- **WHEN** a task is triggered in `D:\A`, the user navigates to `D:\B`, and the first task resumes after an await
- **THEN** the first task retains `D:\A` while a later task triggered in `D:\B` receives `D:\B`

### Requirement: Configurable dispatch policies
The system SHALL default every handler to bounded FIFO `queue` dispatch and SHALL support bounded `parallel`, `latest`, and `drop` overrides.

#### Scenario: Queue preserves trigger order
- **WHEN** several events reach a queue handler while its first task is running
- **THEN** the remaining tasks run one at a time in trigger order using their originally captured contexts

#### Scenario: Overload is explicit
- **WHEN** a handler reaches its configured queue or concurrency bound
- **THEN** the source remains non-blocking and the system emits an overload result or diagnostic

### Requirement: Non-blocking timing and scheduling
The system SHALL provide coroutine await, sleep, per-call/task timeouts, delay, debounce, throttle, one-shot, interval, and cron schedules without blocking GPUI.

#### Scenario: Sleep yields execution
- **WHEN** a Lua task awaits `sleep`
- **THEN** other scripts, handlers, and UI work continue until the task becomes runnable

#### Scenario: Missed schedule policy
- **WHEN** an always script restarts after a scheduled occurrence
- **THEN** the schedule applies its declared `skip` or `run_once` policy and never replays every missed occurrence

### Requirement: Atomic hot reload and shutdown
The system SHALL validate a changed script in a fresh VM, swap only on success, and cancel owned resources on disable, reload, or final-window shutdown.

#### Scenario: Invalid reload preserves working version
- **WHEN** a changed script fails parsing or registration
- **THEN** the previous valid VM continues and the manager reports source diagnostics

#### Scenario: Final window closes
- **WHEN** the final Explorer window closes
- **THEN** all tasks, timers, watchers, requests, child processes, hooks, and Lua VMs stop and no tray/background runtime remains
