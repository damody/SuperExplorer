## Context

`execute_with_terminals` currently tries Everything, then LocalIndex, then filesystem walk. Folder Options General has language, browse, and restore-session controls but no search-engine choice. MFT SQLite at `%ProgramData%\SuperExplorer\MftIndex\{letter}.mft.sqlite3` already stores names and parent references for folder size.

## Goals / Non-Goals

- Goals: one persisted radio; disabled + hint for unsupported engines; exclusive execution; MFT filename search from reconstructed paths.
- Non-Goals: multi-select fallback order; Windows Index as a user option; changing LocalIndex into a Folder Options engine.

## Decisions

- Preference type is `SearchEnginePreference::{Everything, Mft, FileEnumeration}`; default Everything.
- File enumeration is a bounded filesystem walk (`SearchBackend::FileSystemFallback`), not `LocalIndex`.
- Availability: local filesystem path required for all three; Everything also needs SDK+IPC; MFT also needs `{letter}.mft.sqlite3`.
- Remote locations disable all three. Apply of other settings still works when none are available. If the stored engine is unsupported and another engine works, Apply is rejected until the user picks a supported engine.
- `StartSearch` carries `engine`. The STA worker re-probes and fails closed with no fallback.
- `explorer-shell-win` may depend on `explorer-mft`; it must not depend on `explorer-app`.

## Risks / Trade-offs

- Users without Everything must pick file enumeration before Apply on a local folder, and search with the default Everything engine fails until they do. This is the approved no-silent-fallback contract.
- Loading a volume MFT index for search can be large; cancellation is checked during the walk.
