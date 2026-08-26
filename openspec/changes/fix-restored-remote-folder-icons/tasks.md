## 1. File Row Visual Selection

- [x] 1.1 Add a pure selector that prefers a specific visual, then permits the generic folder texture only for containers.
- [x] 1.2 Resolve the generic Windows Shell folder texture once per file-view render and apply the selector to each row.
- [x] 1.3 Preserve the existing vector placeholders when the selector returns no Shell texture.

## 2. Regression Coverage

- [x] 2.1 Test specific-first, container-generic, non-container rejection, and no-texture selection cases.
- [x] 2.2 Test that the icon snapshot exposes the generic Windows Shell folder texture needed after restored remote startup.

## 3. Validation

- [x] 3.1 Run Rust formatting and verify the changed Rust file has no formatting diff.
- [x] 3.2 Run focused explorer-ui folder icon and snapshot tests.
- [x] 3.3 Run `cargo check -p explorer-ui` and record the result.
