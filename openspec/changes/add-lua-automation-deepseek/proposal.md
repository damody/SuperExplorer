## Why

The Explorer currently has typed application actions and asynchronous service boundaries but no user-extensible automation layer. Embedding Lua lets users connect hotkeys, Explorer and Windows events, CLI tools, scheduling, file output, and DeepSeek summarization without rebuilding the application.

## What Changes

- Add an embedded Lua 5.4 runtime with one isolated VM per script and non-blocking coroutine tasks.
- Add typed subscriptions for Explorer, configured-folder, global input, window, clipboard, system, process, schedule, and AI events.
- Add script-scoped task working directories, queue policies, waits, timeouts, debounce, throttle, interval, and cron scheduling.
- Add controlled file-output and direct-process APIs, plus fixed BAT/PowerShell script entry points and deletion confirmation.
- Add a provider-neutral AI client with DeepSeek V4 Flash summarization and atomic TXT output.
- Add a GPUI script manager and configurable summary panel/popup presentation.
- Ship versioned API documentation, event schemas, EmmyLua types, and tested examples suitable for external AI script authoring.

## Capabilities

### New Capabilities

- `lua-automation-runtime`: Script discovery, VM isolation, lifecycle, task contexts, dispatch policies, waits, and scheduling.
- `automation-event-hooks`: Versioned Explorer, filesystem, and observation-only Windows event delivery.
- `automation-host-actions`: Controlled filesystem, CLI, BAT/PowerShell, clipboard, notification, and deletion-confirmation APIs.
- `deepseek-summarization`: Provider-neutral AI requests, DeepSeek V4 Flash, summary UI, cancellation, and atomic TXT output.
- `automation-script-management`: Script manager behavior, persisted overrides, diagnostics, credentials, and AI-oriented documentation/examples.

### Modified Capabilities

None.

## Impact

- Adds new `explorer-automation` and `explorer-ai` workspace crates.
- Extends `explorer-model` with owned automation protocol identifiers and `explorer-shell-win` with hooks, controlled processes, watchers, and credential adapters.
- Extends `explorer-ui` with script-management and summary presentation surfaces and `explorer-app` with ordered service composition and shutdown.
- Adds Lua, HTTP/TLS, cron/time-zone, glob, and credential-related dependencies.
- Introduces privacy-sensitive global event observation and external-process execution; payload persistence is disabled by default and deletion remains confirmation-gated.
