# Lua Automation and DeepSeek Integration Design

Date: 2026-07-27
Status: Approved design
Target: Rust GPUI Windows Explorer

## Summary

Add an embedded Lua 5.4 automation system to the Explorer application. Users can activate scripts with hotkeys, subscribe to Explorer and Windows events, watch configured folders, spawn asynchronous tasks, wait without blocking the UI, run controlled CLI tools, schedule work, call DeepSeek V4 Flash, and write results to text or other files.

Each script runs in an independent Lua VM. Every trigger creates a task with an immutable execution context, including the active Explorer directory at task creation. Scripts are locally trusted and do not require per-capability approval. File removal is the sole confirmation-gated operation.

The application only runs automation while the Explorer process is running. Closing the final window stops scripts, hooks, watchers, schedules, AI requests, and child processes. Scripts marked `always` load automatically on the next application start; `temporary` scripts remain disabled.

## Goals

- Provide standard Lua 5.4 syntax with a small event-driven API inspired by AutoIt.
- Support many typed Explorer and Windows events without blocking their source callbacks.
- Support global hotkeys and global input/window observation without cancelling or modifying original events.
- Run Lua handlers, waits, CLI tools, AI requests, and schedules asynchronously without freezing GPUI.
- Let every script declare or receive UI-configured folder watch roots, recursion, and glob filters.
- Let Lua call `deepseek-v4-flash`, receive the result, show it in the UI, or atomically save it to a selected TXT path.
- Provide a script manager for enable/disable, reload, diagnostics, task history, settings, and external-editor launch.
- Ship versioned documentation, type stubs, schemas, and runnable examples that an external AI can use to write valid scripts.
- Preserve the repository's owned-message and Windows-adapter boundaries.

## Non-goals

- No built-in Lua source editor.
- No built-in AI script generator.
- No background tray process or Windows service after the final Explorer window closes.
- No cancellation, replacement, or suppression of Windows or Explorer events.
- No unrestricted `os.execute`, `io.popen`, native Lua modules, or direct shell-host invocation.
- No guarantee that an arbitrary trusted third-party executable will not delete files internally.
- No compatibility layer for AutoIt syntax; the similarity is in the compact automation workflow.

## Chosen Approach

Use `mlua` with embedded Lua 5.4 and one VM per script. `mlua`'s async integration maps host futures to Lua coroutines, which supplies the required non-blocking `await` behavior. Lua hooks or interrupts and host-side resource accounting stop runaway code.

Two alternatives were rejected for the first version:

- Luau offers strong sandboxing and interruption but is not fully standard Lua 5.4.
- A separate `automation-host.exe` offers stronger process isolation but adds IPC, packaging, restart, and debugging complexity.

All automation commands and events use owned, versioned messages so the runtime can move to a separate process later without changing the Lua-facing API.

## Architecture

### Crate and layer boundaries

- `explorer-model` owns platform-neutral automation command/event identifiers shared with application state where necessary.
- A new `explorer-automation` crate owns the Lua runtime, script registry, event router, task contexts, scheduling abstractions, settings model, API bindings, and fake adapters.
- A new `explorer-ai` crate owns the provider-neutral summary/chat interface, DeepSeek implementation, streaming parser, retry rules, and fake client.
- `explorer-shell-win` owns Windows global hooks, WinEvent subscriptions, clipboard listener integration, folder watcher adapters, controlled process launch, Job Objects, and Windows Credential Manager access.
- `explorer-jobs` supplies or is extended with bounded non-blocking job primitives where those primitives remain generally useful.
- `explorer-ui` owns the script manager, delete confirmation, summary panel/popup, and task/diagnostic presentation. It does not call Win32 or perform filesystem/network I/O.
- `explorer-app` composes services, starts automation after the window/application foundation is ready, and performs ordered shutdown.

No Windows handle, COM interface, Lua registry value, or borrowed UI reference crosses a service boundary. Hook callbacks copy only the minimal owned payload and enqueue it non-blockingly.

### Main data flow

1. Explorer actions, Windows hooks, configured folder watchers, timers, process events, or AI work produce a typed source event.
2. The automation service normalizes it into a versioned event envelope.
3. The router selects subscribed handlers and applies source-side coalescing where required.
4. Each trigger creates an immutable `TaskContext`, including the active Explorer tab directory at that instant.
5. The handler's dispatch strategy places the task in `queue`, `parallel`, `latest`, or `drop` execution.
6. Lua executes until completion or until it awaits a host operation.
7. Host effects pass through typed file, process, UI, clipboard, scheduler, or AI adapters.
8. Completion, failure, cancellation, timeout, and overload are emitted as structured automation events.

## Script Discovery, Identity, and Lifecycle

Scripts live under:

```text
%LOCALAPPDATA%\Rust GPUI Windows Explorer\automation\scripts
```

The script manager scans `.lua` files recursively. A script receives a stable UUID on first discovery; the settings store maps that UUID to the canonical script path and content fingerprint. If a file moves, the manager can relink it without discarding settings.

Each script calls `script.configure` during a restricted registration phase. Registration may declare metadata, handlers, hotkeys, watches, and schedules, but may not launch a process, access AI, modify files, or display UI. Runtime effects are allowed only after successful registration.

The embedded standard-library surface contains `base`, `coroutine`, `table`, `string`, `math`, and `utf8`. It removes `io`, `os`, `package`, and `debug`; scripts cannot load DLLs or native Lua modules. All external effects therefore pass through typed host APIs.

Activation modes are:

- `always`: automatically enabled on every application start.
- `temporary`: enabled only when the user explicitly enables it during the current process lifetime.

Disabling a script cancels its tasks, timers, watchers, AI requests, and child processes, then destroys its VM. Closing the final application window disables every script. The application does not remain in the system tray.

At startup, the automation service registers all `always` scripts before emitting `app.started`, so those scripts can observe it. `temporary` scripts enabled later receive their own registration/load state but do not receive a replayed application-start event.

Hot reload is atomic. The manager parses and registers the new file in a fresh VM. It swaps VMs only after registration succeeds. A failed reload reports diagnostics while the previous valid version continues running.

## Task Model and Working Directory

Every event or schedule trigger spawns a distinct task. Its immutable context contains:

- task, script, handler, correlation, window, and tab identifiers;
- the event snapshot and its sequence/timestamp;
- `cwd`, captured from the active Explorer tab when the task is created;
- cancellation and timeout state;
- bounded stdout/stderr capture;
- Lua coroutine state.

If a task is created while the active directory is `D:\A`, it continues using `D:\A` after an `await`, even if the user navigates elsewhere. A later trigger created in `D:\B` uses `D:\B`. Relative paths passed to file and process APIs resolve against the task's `cwd` unless the call explicitly supplies another `cwd`.

This rule applies when a task waits in a handler queue: the directory is captured when the trigger creates the task, not when execution reaches the front of the queue.

## Dispatch and Timing

Handler dispatch defaults to `queue`. A handler may select:

- `queue`: retain triggers in order and run one at a time.
- `parallel`: run multiple task instances concurrently, within configured limits.
- `latest`: keep the running task and only the newest pending trigger.
- `drop`: ignore triggers while a task is already running.

All queues are bounded. Overload produces an explicit diagnostic/event instead of blocking the source or growing memory without limit.

Timing APIs include:

- non-blocking `sleep`;
- per-call and per-task timeout;
- event `delay`, `debounce`, and `throttle`;
- one-shot scheduling;
- fixed interval scheduling;
- cron scheduling with an explicit Windows/IANA time-zone setting and defined daylight-saving behavior.

Schedules exist only while the owning script is enabled. `always` script schedules are re-created from their declarations on application start. Each schedule selects one missed-run policy: `skip` or `run_once`; it never replays every missed occurrence.

High-frequency sources such as mouse movement, window-location changes, and progress updates coalesce the newest snapshot before handler dispatch. The handler still uses its declared dispatch mode.

## Event Contract

All events use the following logical envelope:

```lua
{
  name = "fs.created",
  version = 1,
  sequence = 1842,
  timestamp = "2026-07-27T10:00:00+08:00",
  source = "filesystem",
  script_id = "...",
  window_id = "...",
  tab_id = "...",
  context = { cwd = "D:\\work" },
  data = { path = "D:\\work\\note.txt", watch_root = "D:\\work" }
}
```

Fields that do not apply are absent rather than set to misleading defaults. Event names and payloads are versioned independently from the Lua API version.

### Explorer application and window events

- `app.started`, `app.stopping`
- `window.opened`, `window.closed`, `window.activated`, `window.deactivated`
- `window.moved`, `window.resized`, `window.minimized`, `window.maximized`
- `theme.changed`

### Navigation, tab, selection, and search events

- `navigation.started`, `navigation.completed`, `navigation.failed`
- `directory.entered`, `directory.refreshed`
- `tab.opened`, `tab.closed`, `tab.activated`, `tab.reordered`
- `selection.changed`, `item.opened`
- `search.started`, `search.completed`, `search.cancelled`, `search.failed`

### Explorer file-operation and clipboard events

- `file_operation.started`, `file_operation.progress`
- `file_operation.completed`, `file_operation.cancelled`, `file_operation.failed`
- `file.created`, `file.renamed`, `file.copied`, `file.moved`
- `file.recycled`, `file.deleted`
- `clipboard.copy`, `clipboard.cut`, `clipboard.paste`

### Configured folder-watch events

- `fs.created`, `fs.modified`, `fs.removed`, `fs.renamed`
- `fs.attributes_changed`, `fs.security_changed`
- `watch.started`, `watch.stopped`, `watch.overflow`, `watch.error`

Each script can declare multiple roots, recursion, include globs, and exclude globs. The UI can override these settings without rewriting the Lua file. Watch overflows are reported explicitly; the service never pretends the event stream remained complete.

### Global keyboard, mouse, and hotkey events

- `input.key_down`, `input.key_up`
- `input.mouse_down`, `input.mouse_up`, `input.mouse_move`
- `input.mouse_wheel`, `input.mouse_hwheel`
- `hotkey.triggered`

Keyboard payloads include virtual key, scan code, modifier state, repeat state, and injection metadata. The host does not translate raw keys into typed text. Hooks are observation-only: Lua cannot suppress or change the original input. Global hotkeys are chord matchers over the observation stream, not suppressing `RegisterHotKey` registrations, so the underlying key combination continues to the foreground application.

### Global window, clipboard, and system events

- `system.foreground_changed`
- `system.window_created`, `system.window_destroyed`
- `system.window_shown`, `system.window_hidden`
- `system.window_location_changed`, `system.window_title_changed`
- `clipboard.changed`, `clipboard.text_available`, `clipboard.files_available`
- `system.session_locked`, `system.session_unlocked`
- `system.suspend`, `system.resume`
- `system.display_changed`, `system.dpi_changed`
- `system.device_arrived`, `system.device_removed`
- `system.network_changed`

### Automation, process, schedule, and AI events

- `task.started`, `task.completed`, `task.cancelled`, `task.failed`
- `process.started`, `process.stdout`, `process.stderr`, `process.exited`, `process.timed_out`
- `schedule.fired`, `schedule.missed`
- `ai.started`, `ai.streaming_delta`, `ai.completed`, `ai.cancelled`, `ai.failed`
- `ai.output_written`

## Lua API

The public API version is `explorer-automation/v1`.

### Registration

```lua
script.configure {
  name = "Summarize notes",
  activation = "always",
  default_dispatch = "queue",
  task_timeout = "90s"
}

watch {
  root = "D:\\Notes",
  recursive = true,
  include = { "**/*.txt", "**/*.md" },
  exclude = { "**/summary/**", "**/~*" }
}

on("fs.created", { debounce = "500ms" }, function(event, task)
  -- handler
end)

hotkey("Ctrl+Alt+S", function(event, task)
  -- handler
end)
```

### Tasks and timing

- `spawn(function, options?)`
- `await(future)`
- `sleep(duration)`
- `schedule.once(time_or_delay, function, options?)`
- `schedule.every(interval, function, options?)`
- `schedule.cron(expression, function, options?)`

Every handler invocation is already a task. Handlers receive the event and immutable task object, including `task.cwd`, identifiers, deadline, and cancellation state. `spawn` creates an optional child task that inherits the parent cwd and cancellation scope unless options explicitly override them; it is not required for ordinary event handlers.

### File API

- `fs.read_text(path, options?)`
- `fs.write_text(path, text, options?)`
- `fs.append_text(path, text, options?)`
- `fs.write_json(path, value, options?)`
- `fs.write_bytes(path, bytes, options?)`
- `fs.remove(path, options?)`

Write modes are `create_new`, `atomic_replace`, and `append`. Text defaults to UTF-8. Relative paths resolve against the task's `cwd`. Calls may specify an explicit base/cwd or absolute path.

`atomic_replace` writes a temporary file in the target directory, flushes it as required by the implementation contract, and replaces the destination so a cancelled or failed operation does not leave a partial result.

### Process API

```lua
local result = await(cli.run("indexer.exe", {
  "--path", event.data.path
}, {
  cwd = task.cwd,
  timeout = "30s"
}))

local result = await(process.run_script("tools\\summarize.ps1", {
  "-Input", event.data.path
}, {
  cwd = task.cwd,
  timeout = "2m"
}))
```

`cli.run` accepts a direct executable plus a separate argument array. It rejects shell hosts including `cmd.exe`, Windows PowerShell, `pwsh.exe`, `wscript.exe`, and `cscript.exe`.

`process.run_script` is the only API for `.bat`, `.cmd`, and `.ps1`. The host selects the fixed interpreter and passes the script path and arguments separately. Child processes run in a Windows Job Object so cancellation, timeout, disable, reload, and application shutdown can terminate the process tree.

Stdout and stderr capture are bounded and returned in the structured result. Streaming output is available through process events.

### Clipboard, UI, and logging

- `clipboard.text()`, `clipboard.files()`
- `notify(title, body?)`
- `ui.show_summary(text, { mode = "panel" | "popup" })`
- `log.debug`, `log.info`, `log.warn`, `log.error`

The user's summary presentation preference defaults to a dockable side panel and can be changed to a small popup. Lua AI calls return data without forcing UI presentation.

### DeepSeek API

The provider-neutral AI boundary supports `ai.summarize` and `ai.chat`. The first provider is DeepSeek using:

- base URL: `https://api.deepseek.com`
- model: `deepseek-v4-flash`
- OpenAI-compatible Chat Completions format

Example with direct TXT output:

```lua
local result = await(ai.summarize(source, {
  provider = "deepseek",
  model = "deepseek-v4-flash",
  timeout = "60s",
  output = {
    path = "summary.txt",
    base = task.cwd,
    mode = "atomic_replace",
    encoding = "utf-8"
  }
}))

notify("Summary complete", result.output_path)
```

The example body runs inside an event handler and therefore uses that handler's `task` parameter.

If `output` is absent, the call only returns the generated text. When output is present, the result contains both text and the resolved `output_path`. A write failure returns a structured error and does not report AI work as fully completed.

The API key is stored in Windows Credential Manager and never stored in Lua files, JSON settings, panic reports, or diagnostics. Requests honor task cancellation and timeout. Retryable 429 and 5xx responses use jittered backoff with at most two retries; authentication, validation, and other permanent errors return immediately.

## Script Manager UI

The script manager provides:

- script name, path, API version, activation mode, and current state;
- enable, disable, reload, and open-in-configured-external-editor actions;
- editable UI overrides for watch roots, recursion, globs, dispatch mode, queue size, task timeout, schedules, and summary presentation;
- current and recent tasks with status, duration, bounded output, and structured error;
- reload and registration diagnostics with source location;
- DeepSeek credential setup and connection test;
- a visible warning explaining the third-party executable deletion boundary.

UI overrides live in a separate settings file and never rewrite user Lua source.

## AI Script Documentation Package

Ship an `automation-sdk` directory that can be copied into an AI conversation or coding workspace. It contains:

- `AI_LUA_CONTEXT.md`: one self-contained, AI-oriented document containing API version, functions, event payloads, task/cwd rules, forbidden APIs, error behavior, and compact examples.
- `AI_PROMPT_TEMPLATE.md`: a recommended prompt requiring `explorer-automation/v1`, complete Lua output, no invented host APIs, and an explanation of triggers and effects.
- `API_REFERENCE.md`: human-readable signatures, arguments, return types, asynchronous behavior, errors, and examples.
- `EVENT_CATALOG.json`: machine-readable event names, versions, and payload schemas.
- `types/explorer-automation.lua`: EmmyLua annotations for Lua Language Server and AI tools that consume type stubs.
- `examples/`: runnable, tested scripts.

Required examples are:

- hotkey notification;
- events and queue behavior;
- temporary versus always activation;
- task working-directory capture;
- configured folder watch;
- writing and appending TXT;
- direct executable invocation;
- BAT and PowerShell script invocation;
- delete confirmation handling;
- DeepSeek summary;
- DeepSeek summary directly to TXT;
- clipboard summary;
- delay, debounce, and throttle;
- interval and cron scheduling.

Every document and example declares `explorer-automation/v1`. CI parses and registers every example against the real API. Documentation signatures and the machine-readable catalog are generated from or checked against the same typed definitions used by the runtime.

## Deletion Confirmation and Process Safety

Local scripts are trusted. Reading, creating, overwriting, appending, hooks, clipboard access, ordinary process execution, networking through the AI client, and UI presentation do not require confirmation.

Deletion rules are:

1. Every `fs.remove` or built-in recycle/permanent-delete request displays a confirmation containing the script identity, resolved targets, and deletion mode.
2. Lua cannot programmatically accept, close, or bypass the confirmation.
3. Rejection returns `DeletionDenied`; it fails only that task and does not stop the handler queue.
4. Before running BAT, CMD, or PowerShell files, the process broker scans the exact file contents for deletion commands, aliases, common destructive parameters, and statically resolvable nested local scripts.
5. Definite, possible, or dynamically indeterminate deletion behavior triggers confirmation before the script process starts.

Static inspection cannot prove that an arbitrary third-party executable is non-destructive. A trusted executable can delete data internally, and commands such as synchronization or version-control tools may remove files without a shell command that the host can recognize. The application explicitly documents this limitation and warns on first CLI use, but does not add a general executable confirmation because the approved policy only confirms recognized or possible deletion.

The same trust boundary applies to sensitive observation. An enabled script can receive global key metadata or clipboard contents and can send data through an allowed executable or DeepSeek request. The manager displays this warning when global input/clipboard subscriptions or CLI/AI APIs are first detected, but the approved policy does not require a permission prompt.

## Error Handling and Resource Governance

- One task failure does not stop another task, handler, or script.
- Structured errors carry a stable kind, user-safe message, operation/correlation IDs, and optional source location. Sensitive event data and AI content are excluded.
- Script parse or registration failure leaves the previous valid hot-reloaded VM active.
- Default task wall timeout is 90 seconds; a script/UI override may raise it to a hard maximum of 24 hours.
- A watchdog interrupts Lua after 2 seconds of continuous execution without yielding; the UI may raise this to a hard maximum of 10 seconds.
- Each VM defaults to 128 MiB and has a hard maximum of 512 MiB; allocation failure is contained to that VM/task.
- Each handler queue defaults to 1,024 pending triggers and has a hard maximum of 10,000.
- Parallel handlers default to four simultaneous tasks per handler and have a hard maximum of 32.
- Captured stdout and stderr are limited to 8 MiB each per task, preserving an explicit truncation flag. Per-script task history retains 10,000 metadata records; persistent logs rotate at 10 MiB with five files.
- Source disconnection, watcher overflow, overload, timeout, cancellation, and process exit remain distinguishable.
- Application shutdown first stops new event intake, then cancels tasks and requests, terminates Job Objects, removes hooks/watchers/listeners, destroys Lua VMs, and finally completes diagnostics flushing.

Raw global keyboard data, clipboard contents, AI prompts/responses, selected file names, and complete process output are not written to persistent diagnostics by default. Logs retain counts, sizes, durations, event names, result kinds, and correlation IDs.

## Testing Strategy

### Unit tests

- Event envelope/version serialization and validation.
- Router matching and `queue`, `parallel`, `latest`, and `drop` behavior.
- Bounded overload behavior.
- Virtual-clock tests for sleep, timeout, delay, debounce, throttle, intervals, cron, time zones, daylight-saving transitions, and missed-run policy.
- Path resolution and task-cwd capture.
- Atomic text/JSON/byte output and cancellation cleanup.
- Shell-host rejection and BAT/CMD/PowerShell deletion classification.
- DeepSeek request/stream parsing, retry classification, timeout, cancellation, and output composition.

### Lua contract tests

- API registration and forbidden standard-library functions.
- Coroutine await, cancellation, error propagation, watchdog, and memory limits.
- Per-script VM and queue isolation.
- Atomic hot reload and failed-reload rollback.
- Every `automation-sdk/examples` file parses, registers, and produces expected fake-host effects.

### Windows integration tests

- Test fixtures generate keyboard, mouse, hotkey, foreground-window, clipboard, display/device, and folder-watch events.
- Events preserve sequence, version, payload, and observation-only behavior.
- Watch overflow is visible and recoverable.
- BAT/PowerShell execution uses the controlled entry point.
- Job Object cancellation terminates descendants on timeout, disable, reload, and application shutdown.
- Hooks, watchers, listeners, handles, threads, and child processes are released after repeated enable/disable cycles.

### UI and end-to-end tests

- Script manager activation, settings override, external-editor action, reload error, task history, and diagnostics.
- Delete confirmation cannot be accepted through Lua and rejection returns `DeletionDenied`.
- Summary side panel and popup both render success, loading, cancellation, and error states.
- Fake DeepSeek streaming, retry, timeout, and cancellation flow into an atomic UTF-8 TXT output.
- A live `deepseek-v4-flash` smoke test runs only with an explicit opt-in environment flag and Credential Manager entry; ordinary CI never consumes API credit.

### Reliability and performance tests

- Feed 100,000 mixed synthetic events and verify bounded memory and queue behavior.
- Stress high-rate mouse/window/progress events and verify source-side coalescing.
- Repeatedly enable, disable, and reload scripts while tasks, watchers, and child processes are active.
- Release-fixture hook callbacks have p99 latency no greater than 1 ms; overload drops/coalesces rather than blocking user input.
- After stress completes, memory returns to a stable band and no hook, watcher, timer, process, or VM remains orphaned.

## Acceptance Criteria

The change is complete when all of the following are demonstrated:

1. A temporary Lua script can be enabled, receive a global hotkey without suppressing the original key input, spawn a non-blocking task, wait, run a direct CLI executable, and report completion.
2. An always-enabled script restores on the next application start and monitors only its configured roots and globs until the application closes.
3. A task created in `D:\A` writes relative output to `D:\A` after awaiting; a later task created in `D:\B` writes to `D:\B`.
4. Lua can send text to `deepseek-v4-flash`, receive the summary, show it in either configured UI mode, and atomically write it to a selected UTF-8 TXT path.
5. Default handler dispatch is `queue`, with verified `parallel`, `latest`, and `drop` overrides.
6. Sleep, timeout, delay, debounce, throttle, one-shot, interval, and cron scheduling do not block GPUI.
7. Explorer, folder, input, window, clipboard, system, task, process, schedule, and AI event catalogs are available with versioned payload documentation.
8. `fs.remove` and possible deletion in BAT/CMD/PowerShell always require a non-scriptable confirmation.
9. Disabling/reloading a script or closing the final window cancels work and leaves no orphaned process, hook, watcher, timer, request, or VM.
10. The AI documentation package is version-consistent, and every supplied Lua example passes automated contract tests.

## Implementation Sequencing Constraint

Implementation should proceed in vertical slices while preserving a usable closed loop:

1. owned automation protocol, task context, fake host, and bounded router;
2. Lua VM lifecycle, registration, handler queue, await/sleep, and manager basics;
3. Explorer events and task-cwd file output;
4. Windows hooks and configured folder watches;
5. controlled process execution and deletion confirmation;
6. scheduler features;
7. DeepSeek client, summary UI, and direct TXT pipeline;
8. AI documentation package, full catalog, stress tests, and release evidence.

The detailed implementation plan is intentionally deferred until this written specification is reviewed and approved.

## External References

- Lua 5.4 reference manual: <https://www.lua.org/manual/5.4/>
- `mlua` async and Lua 5.4 integration: <https://docs.rs/mlua/latest/mlua/>
- DeepSeek model and pricing reference: <https://api-docs.deepseek.com/quick_start/pricing/>
- DeepSeek API change log: <https://api-docs.deepseek.com/updates/>
