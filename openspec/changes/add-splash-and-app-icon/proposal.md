## Why

SuperExplorer currently launches without branded startup feedback and its executable does not embed the supplied product icon. Adding a lightweight splash and consistent Windows icon gives the fast startup path a recognizable, polished product identity.

## What Changes

- Add a transparent, centered SuperExplorer splash that overlays the concurrently loading main window for one second and then fades out.
- Add deterministic splash and multi-size Windows icon assets derived from the supplied artwork.
- Embed the application icon in `SuperExplorer.exe` for Windows shell and window surfaces.
- Keep automated and visual-fixture startup deterministic by omitting the splash in those modes.
- Preserve normal application startup when splash creation or display fails.

## Capabilities

### New Capabilities

- `startup-branding`: Defines the splash timing, transparency, concurrent main-window loading, application icon coverage, and automation behavior.

### Modified Capabilities

None.

## Impact

- Affects the `explorer-app` composition root, GPUI window startup, Windows resources, packaged assets, diagnostics, and focused startup tests.
- Adds PNG and ICO artifacts under `crates/explorer-app/assets/` without introducing a runtime dependency or public API change.
- Existing unrelated UI and navigation behavior remains unchanged.
