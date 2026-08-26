## 1. Action Log Classification

- [x] 1.1 Add a pure action classifier that excludes only `UpdateFileDrag` from INFO-level dispatch logging.
- [x] 1.2 Route `UpdateFileDrag` dispatch records to TRACE and all other action records to INFO without changing returned traces or reducer behavior.

## 2. Regression Coverage

- [x] 2.1 Add focused unit coverage for pointer-update, drag-lifecycle, and ordinary-action logging classification.

## 3. Validation

- [x] 3.1 Run Rust formatting and verify no formatting diff remains in the changed Rust file.
- [x] 3.2 Run focused `explorer-ui` action logging and passive-pointer tests.
- [x] 3.3 Run `cargo check -p explorer-ui` and record the result.
