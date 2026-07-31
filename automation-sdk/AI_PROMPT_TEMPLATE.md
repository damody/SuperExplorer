# Prompt template for an AI script author

Create one complete Lua 5.4 script for `explorer-automation/v1`.

Requirements:

1. Use only APIs documented in the attached `AI_LUA_CONTEXT.md` and `API_REFERENCE.md`.
2. Begin with one `script.configure` call and explain which events/hotkeys/schedules trigger it.
3. Default to `dispatch = "queue"` unless the requirement explicitly benefits from another bounded policy.
4. Use the callback's immutable `task.cwd` for current-folder output.
5. Use `await` for async file, process, clipboard, UI, timing, and AI operations.
6. Never use Lua `io/os/package/debug`, command strings, embedded secrets, input suppression, or unconfirmed deletion.
7. Return only the complete `.lua` source followed by a short trigger/effect explanation.

User goal:

`<describe the automation here>`
