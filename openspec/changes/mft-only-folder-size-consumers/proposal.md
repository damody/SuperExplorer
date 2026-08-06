## Why

Folder-size consumers currently can take separate slow paths and the built-in Size column does not reuse the fast folder totals already available from the privileged MFT service. SuperExplorer needs one predictable, fast, Host-owned source so extension switches cannot change performance or correctness.

## What Changes

- Make the persistent Host folder-size cache the only read surface for built-in Size, Folder size, and Size Map.
- Populate cache misses only from SuperExplorer MFT Windows Service.
- Remove Everything and recursive directory traversal as folder-size fallback behavior.
- Show recursive folder bytes in the built-in Size column without requiring the Folder size extension to be enabled.
- Preserve ordinary file length for files and leave ZIP/Shell namespace containers blank.
- Sort folders by known recursive bytes and preserve existing missing-value ordering when unavailable.
- Expose `Host cache`, `MFT service`, or `MFT unavailable` in the status bar.

## Capabilities

### New Capabilities

- `host-mft-folder-size`: Defines the MFT-only Host cache contract shared by built-in and extension consumers, including invalidation, presentation, sorting, and unavailable behavior.

### Modified Capabilities

None.

## Impact

- Affects `FolderSizeServiceV1`, application folder-size scheduling/cache state, Details Size rendering and sorting, Folder size visual-column integration, and Size Map input.
- Affects Windows installer/service runtime expectations because complete folder totals require `SuperExplorerMft` on supported NTFS volumes.
- Removes implicit slow fallback behavior; unsupported or unavailable results become explicitly blank/unavailable.
- Adds unit, UTIT, installed-build, and screenshot evidence requirements.
