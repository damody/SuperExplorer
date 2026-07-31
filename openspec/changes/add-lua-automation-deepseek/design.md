## Context

The Rust GPUI Explorer already separates model protocols, background jobs, Windows Shell adapters, UI state, and application composition. The new automation system must preserve those boundaries while adding a long-lived embedded language runtime, high-rate Windows event sources, controlled external effects, and a network AI provider. The approved detailed design is `docs/superpowers/specs/2026-07-27-lua-automation-deepseek-design.md`.

The user treats local Lua scripts as trusted and requires confirmation only for file removal. Hooks must observe rather than suppress original events. Automation exists only for the Explorer process lifetime.

## Goals / Non-Goals

**Goals:**

- Embed standard Lua 5.4 with asynchronous host functions and one isolated VM per script.
- Deliver broad, typed Explorer and Windows event coverage through bounded owned messages.
- Capture the active Explorer directory for every task and use it for relative file/process operations.
- Support controlled CLI/script execution, scheduling, DeepSeek summarization, and atomic text output.
- Provide script management, diagnostics, and versioned AI-oriented documentation/examples.

**Non-Goals:**

- AutoIt syntax compatibility, a built-in source editor, or a built-in AI script generator.
- A tray process, background service, event suppression, native Lua modules, or unrestricted shells.
- Proving that arbitrary trusted executables cannot delete data internally.

## Decisions

### Embedded Lua 5.4 with per-script VMs

Use `mlua` with vendored Lua 5.4 and async support. Each script owns a VM, bounded handler queues, resource accounting, timers, and cancellation scope. Expose only base/coroutine/table/string/math/utf8 libraries. Remove io/os/package/debug and native loading.

Luau was rejected because language compatibility matters. A separate host process was deferred because IPC and deployment cost would delay the first complete vertical slice. Owned protocols keep that migration possible.

### Owned event protocol and non-blocking sources

Normalize every source into versioned owned envelopes. Hook and watcher callbacks perform fixed-cost capture plus non-blocking enqueue only. High-rate move/progress sources coalesce before routing. A bounded router exposes overload instead of blocking or unbounded allocation.

### Immutable task contexts

Every trigger creates a task before dispatch queuing. The task captures event data, identifiers, cancellation/deadline state, and active-tab cwd. Queue delay and later navigation cannot change that cwd. Child tasks inherit it unless explicitly overridden.

### Typed host effects

Lua accesses files, processes, clipboard, UI, scheduling, and AI only through typed Rust adapters. Direct executable launch accepts an executable and separate arguments and rejects shell hosts. BAT/CMD/PowerShell use a dedicated scanned entry point and Windows Job Objects.

Built-in removal and definite/possible/indeterminate script deletion require a UI confirmation that Lua cannot accept. Ordinary file writes and trusted executables do not prompt. The UI documents that arbitrary executables remain a trust boundary.

### Provider-neutral AI client

Add an `explorer-ai` boundary with a fake client and DeepSeek implementation. The initial provider uses the OpenAI-compatible DeepSeek endpoint and model `deepseek-v4-flash`. Credentials live in Windows Credential Manager. AI results can return to Lua/UI or pass to the atomic file writer.

### Script manager and generated contracts

The GPUI manager owns activation, reload, overrides, task history, diagnostics, external-editor launch, and summary presentation. API reference, event JSON, EmmyLua types, and examples are generated from or checked against runtime definitions to prevent drift and AI hallucination.

## Risks / Trade-offs

- [Global input and clipboard events can expose sensitive data] → Do not persist raw payloads; warn when scripts subscribe; retain the explicit trusted-script model.
- [Lua loops or memory growth can degrade the in-process app] → Per-VM limits, watchdog interrupts, bounded queues, and atomic reload isolate failures.
- [Windows callbacks can stall input] → No Lua or blocking work in callbacks; release benchmark requires p99 callback latency no greater than 1 ms.
- [Static script scanning can be bypassed by dynamic behavior] → Treat indeterminate BAT/CMD/PowerShell as deletion-capable and disclose that arbitrary EXEs cannot be proven safe.
- [Network/model behavior changes] → Provider abstraction, explicit model setting, typed errors, bounded retry, and opt-in live tests.
- [Concurrent user edits overlap integration files] → Build new crates and adapters first, then make narrow composition changes while preserving unrelated diffs.

## Migration Plan

1. Add protocol/core crates and fakes without enabling production automation.
2. Add Lua runtime, task scheduler, filesystem output, and contract tests behind composition that is inert when no scripts exist.
3. Add Windows sources and process adapters incrementally.
4. Add DeepSeek/UI features and documentation package.
5. Enable startup discovery after shutdown, stress, and privacy gates pass.

Rollback removes automation composition and workspace members; existing Explorer behavior and persisted script files remain untouched.

## Open Questions

None. Resource defaults, event catalog, lifecycle, confirmation policy, and AI model are fixed by the approved design.
