## Why

Details view currently exposes Windows-only and local-content extension columns while browsing ADB and SFTP, producing unusable headers and unnecessary background work. Remote Linux entries also omit the Unix mode information needed to understand file type and access permissions.

## What Changes

- Add a fail-closed `file_systems` array to every extension data-column manifest contribution; extension authors may declare `local`, `adb`, and/or `sftp`, and users cannot override that scope.
- Project the persisted Details layout through the active filesystem identity so inapplicable columns disappear consistently from headers, rows, menus, sorting, drag/size surfaces, and extension requests without erasing saved layout state.
- Mark bundled Folder size, Main code lines, Code lines, and Lock owner contributions as `local` only.
- Add optional Unix mode metadata to remote entries and map it from both ADB and SFTP directory listings.
- Add a built-in Permissions column for ADB and SFTP that renders symbolic plus four-digit octal mode text and sorts numerically.
- Keep failures bounded: invalid filesystem names reject the affected contribution once, while missing remote mode data renders an em dash without per-entry errors.

## Capabilities

### New Capabilities

- `filesystem-scoped-details-columns`: Active-location filesystem identity, fail-closed column applicability, effective layout projection, temporary sorting fallback, and request suppression.
- `unix-permissions-details-column`: ADB/SFTP Unix mode acquisition, metadata transport, symbolic-plus-octal presentation, missing-data behavior, and numeric sorting.

### Modified Capabilities

- `extension-package-and-feature-lifecycle`: Extend validated data-column manifest contributions with an author-owned, fail-closed filesystem scope.
- `extension-jobs-values-and-dynamic-columns`: Require runtime column admission and dispatch to honor the validated filesystem scope before extension work begins.

## Impact

The change affects extension manifest/schema validation, extension protocol descriptors and fixtures, bundled Rust/Lua manifests, column registry and persisted-layout projection, Details header/row/menu/sort/request coordination, shared file-entry metadata, and the ADB/SFTP providers. Legacy data-column manifests without `file_systems` remain loadable but their column contributions are inactive until authors declare an allowed filesystem. No new dependency, credential flow, destructive operation, or full regression run is introduced.
