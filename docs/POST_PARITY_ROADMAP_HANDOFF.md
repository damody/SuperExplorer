# Post-parity roadmap handoff

Updated: 2026-07-29 (Asia/Taipei)  
OpenSpec change: `complete-explorer-post-parity-roadmap`

## Delivered capabilities

- Versioned, checksummed, crash-safe session/view persistence with bounded history, per-tab settings, restore/reset controls, atomic replacement and backup recovery.
- Bounded asynchronous thumbnails with generation cancellation, memory/disk caches, Windows Shell/provider fallback, invalidation and 1,000-transition deterministic soak coverage.
- Explorer namespace roots and stable non-path Shell identities for Home, Quick Access, Known Folders, This PC, drives, Libraries, ZIP, Recycle Bin, Network and compatible third-party providers.
- Persistent cross-process extension broker plus disposable workers for context menu, thumbnail, namespace and Preview Handler work. Helpers launch without console windows and are contained by authenticated IPC, restricted tokens, Job Objects, deadlines, quarantine and forced cleanup.
- Preview Pane integration with trusted raster preview, broker-hosted Windows Preview Handlers, app-owned HWND boundary, generation-bound resize/DPI, Tab focus, accelerator forwarding, fallback chrome and exact unload on selection/tab/pane/window changes.

## Installed binaries

- `SuperExplorer.exe`: GPUI application and trusted model/service composition root.
- `explorer-extension-broker.exe`: persistent authenticated supervisor.
- `explorer-extension-worker.exe`: disposable provider/handler host.
- `explorer-uitest.exe`: development/CI validation runner; not required by the installed app.

The NSIS installer installs and upgrades the three runtime binaries together. The fresh-install test launches from an isolated installed path, verifies broker readiness and clean shutdown, performs an in-place upgrade, then confirms silent uninstall leaves no program process or file.

## State, cache and recovery

- Session state: `%LOCALAPPDATA%\RustGpuiExplorer\state\v1`
- Thumbnail cache: `%LOCALAPPDATA%\RustGpuiExplorer\thumbnail-cache\v1`
- Shell icon cache: `%LOCALAPPDATA%\RustGpuiExplorer\icon-cache\v1`
- Search fallback index: application data, scoped to visited/requested folders; cancellation stops further indexing.
- Folder Options exposes session restore, view preferences, cache clearing and scoped reset actions. Invalid state falls back to a validated backup or safe defaults. Broker failure falls back to built-in UI and can be retried without restarting the app.

Folder Options/help semantics are shared with the typed settings actions: session-only reset preserves caches, view reset preserves tabs and pins, cache clearing does not remove user files, and full roadmap-state reset is confined to the application-owned data root.

## Validation evidence

- Combined UITEST: `target/uitest-runs/roadmap-combined-final/report.json` — five roadmap capability cases PASS.
- Combined 10-run resource soak: `target/roadmap-combined-soak10-v3/report.json` — session/namespace/thumbnail/broker-preview workloads, no owned descendant leaks.
- Preview visual/DPI/high-contrast: `target/roadmap-preview-evidence-visual-current/report.json`.
- Broker/7-Zip/installer: `target/roadmap-broker-evidence-current5/report.json` and the final rebuilt installer smoke at `target/roadmap-installer-evidence-final/report.json`.
- Thumbnail provider/cache: `target/roadmap-thumbnail-evidence-current/report.json`.
- Namespace real-Shell roots/ZIP: `target/roadmap-namespace-evidence-current2/report.json`.

## Truthful platform limitations

- Preview Handler rendering and theme behavior are provider-owned Windows public API behavior. Missing, incompatible or quarantined handlers show icon/properties/error fallback chrome.
- This machine had one 175% (168 DPI) interactive monitor. Logical 100/125/150/200 matrices passed, while requested raster sessions that remained at 175% are recorded as mismatches, not falsely accepted baselines. Mixed-monitor raster validation remains hardware-dependent.
- Network discovery, authentication, cloud availability, Windows Search and third-party namespace/handler behavior depend on the installed Windows environment. Credentials and file contents are never persisted as roadmap state or evidence.

## Rollback

Runtime rollback is the previous signed installer or removal of the new runtime binaries as one set. A missing or incompatible broker fails closed and leaves filesystem navigation and safe fallback UI usable. User state/cache directories are not removed by uninstall; use the scoped reset controls when intentional cleanup is required.

## Post-roadmap work

- Re-run the raster and cross-process HWND matrix on physical 100/125/150/200% and mixed-monitor hardware when available.
- Expand provider certification as additional third-party namespace, thumbnail and Preview Handlers are installed; absence is a prerequisite skip, never a synthetic pass.
- Continue closing Explorer parity gaps behind the same typed command, bounded background-work and broker isolation contracts.
