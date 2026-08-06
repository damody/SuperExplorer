## Why

The two code-line columns need distinct, testable aggregation semantics, and Details columns need
File Explorer-style user ordering instead of a fixed registry order. Without this, mixed-language
results are ambiguous and extension-heavy layouts cannot be compared efficiently.

## What Changes

- Define Code lines as all-language code total and Main code lines as the largest single-language aggregate.
- Separate caches and deterministic mixed-language verification for the two contracts.
- Make every Details column except Name horizontally draggable with insertion feedback.
- Keep Name permanently leftmost and persist order across restart and extension lifecycle changes.
- Add unit, integration, and UITEST headful coverage including screenshots.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `extension-jobs-values-and-dynamic-columns`: Specify distinct aggregation semantics and user-controlled ordered Details layout with a fixed Name column.

## Impact

Affected areas include code-line providers/cache, `OrderedColumnLayout`, Details header/row
projection, pointer actions/state, session persistence tests, UITEST manifest and headful scripts.
No public extension ABI change is intended.
