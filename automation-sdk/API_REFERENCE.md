# explorer-automation/v1 API

Directory activation contract:

- The filename is exactly `super_explorer.lua`.
- Put it directly in the filesystem folder it controls.
- Activation is exact-directory only; child folders do not inherit it.
- The first tab entering loads it and the last tab leaving unloads it.
- Active file changes are detected and atomically reloaded.

Registration-only APIs:

- `script.configure { name?, activation = "always"|"temporary", default_dispatch?, task_timeout? }`
- `on(event_filter, options?, callback)`; filters are exact names, `prefix.*`, or `*`.
- `hotkey(chord, callback)`; observation-only and never suppresses the foreground application.
- `watch { root, recursive?, include?, exclude? }`
- `schedule.once(delay, callback)`, `schedule.every(interval, callback)`, `schedule.cron(expression, timezone, callback)`

Runtime APIs return awaitable operations unless noted:

- `await(operation)`, `spawn(callback, parent_task?)`, `sleep(duration)`
- `fs.read_text(path, options?)`, `fs.read_bytes(path, options?)`
- `fs.write_text(path, text, options?)`, `fs.append_text(path, text, options?)`
- `fs.write_json(path, value, options?)`, `fs.write_bytes(path, bytes, options?)`
- `fs.remove(path, options?)` — always asks the user; denial is `DeletionDenied`.
- `process.run(executable, argv, { cwd?, timeout? })` — no shell hosts or command string.
- `process.run_script(path, argv, { cwd?, timeout? })` — `.bat`, `.cmd`, `.ps1` only.
- `clipboard.read_text()`, `ui.notify(title, body?)`, `ui.show_summary(text, { popup? })`
- `ai.summarize { text, model?, system_prompt?, output? }`; default model is `deepseek-v4-flash`.

File options may contain `base = task.cwd` and `mode = "create_new"|"atomic_replace"|"append"`. Text is UTF-8. Async failures are structured and privacy-safe; prompts, replies, clipboard content, credentials, and process output are not copied into diagnostic messages.
