# Folder-scoped `super_explorer.lua` design

## Goal

Automatically activate a Lua automation script when an Explorer tab enters a directory that directly contains `super_explorer.lua`. The script applies only to that exact directory. Parent-directory scripts are never inherited by child directories.

## Discovery and scope

- On a successful directory transition, resolve the tab's current filesystem directory and check exactly `<current-directory>/super_explorer.lua`.
- Do not walk parents, search descendants, or fall back to a global automation directory.
- Non-filesystem Shell locations do not activate a directory script.
- The canonical directory identity and script path form the stable runtime identity.
- Events are delivered only when they belong to a tab or filesystem path within that exact directory scope. A script does not receive events from sibling, parent, or child directories merely because their paths are related.
- Every task captures the triggering directory as its immutable `task.cwd`.

## Lifetime and multiple tabs

- Maintain one runtime entry per canonical directory and a set of tab owners.
- The first tab entering the directory loads and registers the script VM.
- Additional tabs entering the same directory attach to the existing runtime entry.
- Leaving or closing a tab removes that tab's ownership. The last owner leaving stops new dispatch, cancels outstanding script tasks, detaches watches and hooks, then unloads the VM.
- Navigation replacement is ordered: acquire the destination directory runtime, switch the tab association, then release the previous directory runtime. A destination load failure leaves the destination without automation and does not revive the old directory association.

## Loading and reload

- Registration uses the existing restricted Lua 5.4 environment and host APIs.
- The runtime watches the active `super_explorer.lua` file. A successful change performs an atomic VM swap after the replacement script fully parses and registers.
- If reload fails, retain the previous working VM, report a structured error, and keep watching for a later valid edit.
- Removing or renaming the script stops new dispatch, cancels tasks, and unloads the runtime even while tabs remain in the directory. Recreating it loads a new runtime for the still-present tab owners.

## Event and safety behavior

- Folder-local scripts use the existing bounded router; `queue` remains the default dispatch policy.
- Hotkeys and system events are registered only while the directory runtime has at least one tab owner. Their callbacks still run with the owning tab's captured directory context.
- File events are filtered to the exact directory scope according to the script's explicit watch configuration; scope does not imply child inheritance.
- Deleting or recycling files remains the only action that always requires confirmation. Existing process, BAT/CMD/PowerShell, file output, clipboard, UI, timing, and DeepSeek restrictions remain unchanged.
- A missing, unreadable, or invalid script never prevents directory navigation.

## Components

- Add a folder-script coordinator above `ScriptRegistry`. It owns canonical-directory entries, tab ownership, navigation transitions, and ordered teardown.
- Reuse `ScriptRegistry` for VM construction, validation, atomic reload, and shutdown.
- Feed directory-entered, tab-closed, and file-change notifications into the coordinator through platform-neutral commands.
- Keep filesystem observation in the Windows adapter and GPUI/application lifecycle wiring in the application crate, preserving the existing architecture boundaries.

## Diagnostics and UI

- Expose directory path, script path, load state, owner-tab count, last reload result, and bounded task history to the automation manager.
- Report invalid or unreadable scripts as non-blocking diagnostics and notifications.
- Clearly label the script as directory-local and show that child directories do not inherit it.

## Verification

- Exact-directory activation and no activation when the file is absent.
- No parent-to-child inheritance.
- Independent sibling-directory runtimes.
- Two tabs share one runtime; only the final tab departure unloads it.
- Navigation and tab-close teardown cancel tasks and release hooks/watchers.
- Valid edits atomically replace the VM; invalid edits preserve the previous VM.
- Delete/rename stops the runtime and recreate restarts it for remaining owners.
- Non-filesystem locations remain inert.
- Existing Lua, process, deletion-confirmation, DeepSeek, architecture, and no-script startup tests continue to pass.

## Out of scope

- Recursive script discovery.
- Parent-script inheritance or layered configuration.
- Automatic trust based on directory location.
- Multiple directory script filenames or project manifest formats.
