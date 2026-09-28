## Why

The current eight file views show one directory at a time. The requested Finder-style column view lets a user inspect successive folder levels and preview a file without repeatedly leaving the parent folder, making path exploration and file identification faster.

## What Changes

- Add a built-in **Columns / 分欄** view choice for local Windows filesystem directories. Selecting a folder exposes its children in the next column; selecting one file opens an integrated preview at the right.
- Keep the parent chain visible with horizontal scrolling for deep paths. Provide independent vertical scrolling and adjustable directory-column and preview widths.
- Make selection, address/breadcrumb, history, keyboard navigation, refresh, tab switching, and file operations coherent with the active column and existing Explorer commands.
- Reuse the existing asynchronous image and brokered Windows Preview Handler pipeline, with truthful loading/error/fallback states and stale-result cancellation.
- Persist the per-tab view choice and widths through the existing versioned session settings. On unsupported locations, show the existing Details view while preserving the local Columns preference.
- Add localized labels, accessibility semantics, and focused model, UI, performance, and Windows interaction verification.

## Capabilities

### New Capabilities

- `local-column-view`: Local filesystem column navigation, integrated preview, layout, interaction, fallback, and persistence behavior.

### Modified Capabilities

None. The existing Preview Pane and extension-view contracts remain in force; Columns reuses their host services without changing their externally specified behavior.

## Impact

- `crates/explorer-model/src/navigation.rs`, `protocol.rs`, and `session.rs`: built-in mode, transient column path state, typed column-listing messages, persisted mode/width migration.
- `crates/explorer-app` and the existing local directory enumeration boundary: route and cancel auxiliary listings without treating them as tab navigation.
- `crates/explorer-ui/src/state.rs`, `actions.rs`, `chrome.rs`, `file_view.rs`, and `lib.rs`: view command, column navigation/rendering, focus/selection, async listings, integrated preview, layout and resize.
- Existing directory enumeration, snapshot/cache, thumbnail, and preview-host services should be reused; any new per-column request must retain request-generation and cancellation boundaries.
- `crates/explorer-i18n/locales/*`, focused tests, and Windows UI tests/documentation.
- First release covers local drive paths. UNC, WSL, Shell namespaces, archives, cloud virtual folders, ADB/SFTP/FTP/Google Drive, and extension views stay on their supported existing views. No plugin ABI or new external dependency is planned.
