## 1. Workspace and Protocol Foundation

- [x] 1.1 Add `explorer-automation` and `explorer-ai` workspace crates with focused public boundaries
- [x] 1.2 Define versioned automation identifiers, event envelopes, task contexts, and structured error/result types
- [x] 1.3 Add deterministic fake event, clock, file, process, UI, credential, and AI adapters
- [x] 1.4 Add architecture checks preventing UI-to-Win32 and automation-to-GPUI coupling

## 2. Router, Tasks, and Timing

- [x] 2.1 Implement bounded subscription matching and FIFO queue dispatch with explicit overload
- [x] 2.2 Implement bounded parallel, latest, and drop dispatch policies
- [x] 2.3 Implement immutable task-cwd capture, child inheritance, cancellation, and deadlines
- [x] 2.4 Implement async sleep, timeout, delay, debounce, and throttle with a deterministic clock
- [x] 2.5 Implement one-shot, interval, and cron scheduling with timezone and missed-run policies

## 3. Lua Runtime

- [x] 3.1 Embed vendored Lua 5.4 through `mlua` with the restricted standard-library surface
- [x] 3.2 Implement registration-phase `script.configure`, `on`, `hotkey`, and `watch`
- [x] 3.3 Bind handler tasks, await, spawn, sleep, scheduling, cancellation, and structured errors
- [x] 3.4 Add per-VM memory accounting, non-yield watchdog, queue/concurrency limits, and isolated failures
- [x] 3.5 Implement discovery, stable identity, always/temporary activation, atomic reload, disable, and ordered shutdown

## 4. File and Process Host Actions

- [x] 4.1 Implement task-relative read/write/append/JSON/byte APIs and atomic replace
- [x] 4.2 Implement built-in remove/recycle confirmation protocol and `DeletionDenied`
- [x] 4.3 Implement direct executable launch with separate arguments, shell-host rejection, bounded output, and typed exit results
- [x] 4.4 Implement BAT/CMD/PowerShell scanning and fixed-interpreter launch
- [x] 4.5 Contain child process trees with Windows Job Objects and verify timeout/cancellation cleanup
- [x] 4.6 Bind clipboard reads, notifications, summary presentation, and privacy-safe structured logging

## 5. Event Sources

- [x] 5.1 Bridge application, window, navigation, tab, selection, search, file-operation, task, process, schedule, and AI events
- [x] 5.2 Implement per-script configured folder watches with recursive glob filters and overflow/error events
- [x] 5.3 Implement observation-only global keyboard, mouse, and chord-matched hotkey events
- [x] 5.4 Implement WinEvent foreground/window lifecycle and location/title events
- [x] 5.5 Implement clipboard, session, power, display/DPI, device, and network-change event sources
- [x] 5.6 Add source-side coalescing, bounded enqueue metrics, unload tests, and p99 callback benchmark

## 6. DeepSeek and Atomic Summary Output

- [x] 6.1 Implement provider-neutral async summary/chat trait, request/result types, streaming, cancellation, and fake client
- [x] 6.2 Implement DeepSeek OpenAI-compatible client for `deepseek-v4-flash`
- [x] 6.3 Implement Credential Manager storage and privacy-safe AI diagnostics
- [x] 6.4 Implement bounded 429/5xx retry, timeout, permanent-error mapping, and cancellation
- [x] 6.5 Compose DeepSeek result with atomic task-relative or explicit UTF-8 TXT output
- [x] 6.6 Add opt-in live DeepSeek V4 Flash smoke test without persisting credentials or content

## 7. GPUI Script Manager and Summary UI

- [x] 7.1 Implement script list/state, enable/disable/reload, activation mode, and external-editor actions
- [x] 7.2 Implement non-destructive UI overrides for watches, dispatch, limits, schedules, and summary mode
- [x] 7.3 Implement bounded task history, source diagnostics, error/timeout/overload/cancellation presentation, and trust warnings
- [x] 7.4 Implement non-blocking dockable summary panel and popup with loading, copy, retry, cancellation, and error states
- [x] 7.5 Compose automation services into application startup/final-window shutdown without changing no-script behavior

## 8. AI Documentation Package

- [x] 8.1 Generate `AI_LUA_CONTEXT.md`, `AI_PROMPT_TEMPLATE.md`, `API_REFERENCE.md`, and `EVENT_CATALOG.json`
- [x] 8.2 Generate EmmyLua `explorer-automation/v1` type stubs
- [x] 8.3 Add runnable hotkey, event queue, activation, task-cwd, watch, file, CLI/script, deletion, DeepSeek/TXT, clipboard, and timing examples
- [x] 8.4 Add gates that parse/register all examples and verify documentation/event/type signatures against runtime definitions

## 9. Verification and Release Evidence

- [x] 9.1 Add unit and contract tests for router, virtual timing, paths, atomic output, process policy, Lua limits, and reload rollback
- [x] 9.2 Add Windows integration fixtures for hooks, watchers, clipboard/system events, script processes, and resource cleanup
- [x] 9.3 Add GPUI behavior/headful/visual tests for manager, deletion confirmation, and summary panel/popup
- [x] 9.4 Run 100,000-event stress, repeated enable/disable/reload, process-tree timeout, and final-window shutdown tests
- [x] 9.5 Run format, architecture, check, clippy, workspace tests, opt-in live DeepSeek smoke, and record final evidence
