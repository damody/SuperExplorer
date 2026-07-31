## Why

Filesystem-backed Shell folders such as Documents currently expose parsing names like
`shell:Personal` when the address bar is clicked. Users need the actual redirected drive or UNC path
so it can be copied directly into Windows Explorer and other applications.

## What Changes

- Canonicalize successfully resolved filesystem-backed Shell locations to complete filesystem paths.
- Apply the behavior consistently to Documents, Downloads, Desktop, Pictures, Music, Videos, and
  other Shell folders that report a filesystem path.
- Preserve parsing-name descriptors for pure namespaces without a real path.
- Add model, real Shell, and headful address-bar regression coverage.

## Capabilities

### New Capabilities

- `canonical-address-paths`: Defines canonical editable address text for filesystem-backed Shell
  locations and fallback behavior for pure namespaces.

### Modified Capabilities

None. This repository has no baseline capability specs to modify.

## Impact

- Affects Windows Shell location metadata publication and committed navigation history.
- Reuses the existing address draft, history, session, and navigation submission paths.
- Does not change breadcrumb labels, enumeration ownership, or external command APIs.
