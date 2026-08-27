# Plugin Directory Autoload and Global Safe Mode

## Outcome

SuperExplorer discovers installed plugin packages from its application-owned `plugins` directory at startup. A newly discovered compatible plugin is enabled by default. Folder Options > Extensions persists global, package, and feature desired states to the existing versioned atomic JSON store. A plugin panic, abnormal callback termination, or stale durable callback marker latches global Safe Mode; the next startup loads no plugin code until the user explicitly chooses **Re-enable all plugins**.

## Architecture and data flow

The application composition root resolves the `plugins` directory relative to the executable, never from the current directory or an environment variable. It inspects at most 1,024 direct entries, ignores symlinks, directories, and non-`.sepack` files, and sorts accepted archives. Each archive enters the existing bounded importer, package validator, sealed store, resolver, desired-state admission, and native lifecycle. Package identity comes only from the validated manifest, never from the archive filename. The installer launches `SuperExplorer.exe` without a fixed Plugin argument list; `--plugin-dll` remains an explicit development/test override and is never an automatic production source.

At startup the extension host loads `feature-state-v1.json`. Missing package or feature entries resolve to `Enabled`, preserving the required first-discovery behavior. The Extensions options draft edits only desired state. Apply/OK validates and atomically replaces the store; Cancel does not mutate it. Runtime effective state continues to combine desired state, dependencies, compatibility, quarantine, and restart requirements.

A separate versioned global Safe Mode latch is stored below the host-owned extension state root. Before entering plugin code, the host writes its existing durable marker. A caught plugin panic records a bounded fault reason and latches Safe Mode before returning control. Abnormal process termination leaves the marker; startup converts any stale marker into the same latch before discovery or loading. While latched, catalog metadata may be inspected without executing plugin code, but all plugin admission and dispatch are denied regardless of desired-state JSON.

Folder Options > Extensions displays the latched state and a **Re-enable all plugins** action. The action clears only the global latch and stale incident markers after an explicit confirmation; it does not rewrite package/feature choices. Rust DLLs are not hot-loaded by this action: the UI reports restart required, and the next clean launch loads only plugins whose individual desired state remains enabled. If latch clearing fails, Safe Mode remains active and the error is shown.

## Failure handling and compatibility

Missing or malformed package candidates are reported in the catalog/diagnostics and do not prevent built-in file management. A corrupt desired-state file is not silently reset; plugins remain blocked with repair diagnostics. A corrupt or unreadable Safe Mode latch fails closed. Diagnostics use package IDs and typed reason codes, not filesystem paths or arbitrary plugin payloads.

Existing installations migrate from installer-supplied DLL payloads and arguments to complete `.sepack` archives in the executable-relative `plugins` directory. Old explicit DLL arguments remain accepted for development compatibility, while the updated installer rewrites shortcuts without Plugin arguments. A validated package identity/generation is admitted at most once even if duplicate archives are present.

## Testing and release gates

Unit tests cover deterministic discovery, new-plugin default enablement, atomic state updates, duplicate suppression, panic latching, stale-marker startup, fail-closed corruption, and explicit latch clearing. Integration tests launch twice to prove that a caught panic and a forced plugin-process termination each yield a plugin-free second launch, then prove that explicit re-enable plus restart restores only individually enabled plugins. UI tests verify Extensions switches, Apply/Cancel semantics, Safe Mode messaging, keyboard/UIA access, and restart messaging. Installer tests verify package layout and shortcuts without fixed plugin arguments.

The release gate requires targeted host/app/UI tests, strict OpenSpec validation, a release build, and an installed-path restart scenario. Rollback is removal of production directory composition while retaining the state schemas; no user choices need deletion.

## Decisions and non-goals

Only direct child `.sepack` archives are auto-discovered; loose DLL discovery, recursive discovery, and symlink traversal are excluded. File-system watching and hot-loading newly copied Rust plugins are excluded: discovery happens on startup. One plugin fault intentionally disables all plugins on the next launch. Per-plugin automatic quarantine may remain as diagnostic detail but cannot bypass the global latch.
