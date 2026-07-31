## Why

The current Explorer implementation is production-usable for filesystem-first navigation, file operations, search, clipboard/OLE, context menus, and visual parity, but it still lacks several user-visible capabilities expected from Windows File Explorer: restoring a working session, real thumbnails, first-class non-filesystem Shell locations, safe isolation of untrusted extensions, and Preview Handler integration. These capabilities should be delivered as one dependency-ordered roadmap so shared identity, cancellation, persistence, security, and evidence contracts do not diverge across separate efforts.

## What Changes

- Persist and restore windows, tabs, navigation locations, history, active tab, and per-tab view settings using a versioned, crash-safe store with recoverable fallback behavior.
- Load Windows Shell thumbnails asynchronously for visible and near-visible items, with request deduplication, cancellation, bounded memory/disk caches, invalidation, fallback icons, and resource telemetry.
- Extend navigation from filesystem-first locations to Explorer-like Home, Quick Access, Known Folders, This PC, drives, Libraries, ZIP folders, Recycle Bin, Network root, and compatible third-party Shell Namespace Extensions.
- Add a restartable, low-privilege cross-process broker for untrusted or potentially blocking Shell extensions, codecs, and providers, using versioned typed messages, deadlines, resource budgets, cancellation, crash quarantine, and exact terminal-event semantics.
- Host Windows Preview Handlers through the broker with Explorer-like preview-pane commands, focus and accelerator forwarding, resize, unload, timeout, crash recovery, and safe fallback UI.
- Add Explorer-parity interaction, accessibility, performance, fault-injection, real-Windows interoperability, and truthful evidence requirements to every phase.
- Keep OneDrive Files On-Demand deep integration, enterprise network authentication/offline management, full Security/Sharing property UI, FTP/SFTP/ADB providers, and multiple-window workspace layouts outside this change.

## Capabilities

### New Capabilities

- `session-settings-persistence`: Versioned, crash-safe persistence and restoration of Explorer windows, tabs, navigation history, locations, and view settings.
- `async-thumbnail-cache`: Viewport-prioritized Windows Shell thumbnail retrieval with bounded caching, cancellation, invalidation, and icon fallback.
- `shell-namespace-navigation`: Explorer-like navigation and capability-aware interaction for path and non-path Windows Shell locations.
- `cross-process-extension-broker`: Restartable low-privilege isolation for untrusted or blocking Shell extensions, codecs, and providers.
- `preview-handler-host`: Brokered Windows Preview Handler lifecycle, rendering, input forwarding, recovery, and preview-pane interaction.

### Modified Capabilities

None. There are no archived base specifications under `openspec/specs`; this umbrella change defines new capability contracts while preserving the active changes' existing requirements.

## Impact

- Affects `explorer-app`, `explorer-common`, `explorer-model`, `explorer-shell-win`, `explorer-jobs`, `explorer-ui`, `explorer-test-support`, and `explorer-uitest`.
- Adds persistent settings/session storage under the application-owned local data directory and versioned migration/recovery rules.
- Extends Shell item descriptors, capabilities, property metadata, enumeration, navigation, and command routing without making filesystem paths the universal identity.
- Introduces at least one separately built Windows broker executable plus an authenticated, bounded local IPC protocol and lifecycle supervision.
- Adds thumbnail and preview cache/resource budgets, telemetry, crash recovery, and new Windows-only integration fixtures.
- Expands headful validation for Explorer-compatible mouse, keyboard, context-command, accessibility, DPI/theme, namespace, thumbnail, preview, broker-failure, and restart flows.
