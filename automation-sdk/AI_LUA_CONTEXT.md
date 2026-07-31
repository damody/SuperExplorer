# Explorer Automation Lua Context

Target API: `explorer-automation/v1`, embedded Lua 5.4.

Write one complete `.lua` file. Do not invent APIs. Registration runs once and may only call `script.configure`, `watch`, `on`, `hotkey`, and `schedule.*`. Host effects belong inside callbacks. The runtime has no `io`, `os`, `package`, `debug`, `require`, `dofile`, `loadfile`, native modules, or shell command strings.

Save the script as `super_explorer.lua` directly inside the folder it controls. Explorer loads it when a tab enters that exact filesystem folder and unloads it after the last owning tab leaves. Scripts are not inherited by child folders, parent folders are not searched, and there is no global automation-directory fallback. Multiple tabs in the same folder share one runtime. Editing the active file reloads it atomically; an invalid edit keeps the previous working runtime.

Each callback is `function(event, task)`. `task.cwd` is an immutable snapshot of the folder active when the event was queued. Relative file paths default to that task directory. A spawned child inherits cwd, deadline, and cancellation. Changing Explorer folders later never changes an already-created task.

Activation is `always` (restore on next launch) or `temporary` (enabled only for this session). Dispatch defaults to `queue`; alternatives are bounded `parallel`, `latest`, and `drop`. Use `await(...)` for async host calls and `sleep("500ms")` for cooperative waiting.

Deletion is the only action that always requires user confirmation. Direct processes accept an executable plus a separate argument array; shell hosts are rejected. BAT/CMD/PS1 use `process.run_script` and deletion-capable or indeterminate scripts ask for deletion consent.

DeepSeek summaries use `ai.summarize`, model `deepseek-v4-flash`, and may specify `output = { path = "summary.txt", base = task.cwd, mode = "atomic_replace", encoding = "utf-8" }`. Credentials come from Windows Credential Manager; never put keys in Lua.

Read `API_REFERENCE.md`, `EVENT_CATALOG.json`, `types/explorer-automation.lua`, and the runnable `examples/` before generating a script.
