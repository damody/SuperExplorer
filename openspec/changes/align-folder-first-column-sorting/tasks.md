## 1. Comparator contract

- [x] 1.1 Add a shared classification comparator in `file_view.rs` that keeps containers before non-containers independently of sort direction.
- [x] 1.2 Route built-in and runtime extension-byte presentation sorting through the shared classification comparator while preserving existing value, missing-value, and tie-break behavior.

## 2. Regression coverage

- [x] 2.1 Add mixed folder/file tests for ascending and descending name and metadata-column sorting, including independent ordering within both groups.
- [x] 2.2 Add runtime extension-byte tests covering both directions and present/missing values without crossing the classification boundary.
- [x] 2.3 Align or remove duplicated test-only comparator behavior so tests exercise the production presentation ordering contract.

## 3. Verification

- [x] 3.1 Run Rust formatting checks and focused `explorer-ui` file-view tests.
- [x] 3.2 Run the relevant `explorer-ui` package test suite and record failures outside the touched sorting paths (415 passed, 8 unrelated failures, 2 ignored; all file-view sorting tests passed).
- [x] 3.3 Review the final diff against every `file-surface-column-sorting` requirement and verify no visible grouping or persisted-state change was introduced.

## 4. Browsable archive classification correction

- [x] 4.1 Replace `is_container`-only sorting classification with local filesystem directory evidence plus provider fallback.
- [x] 4.2 Add ZIP-like navigable-file tests for ascending and descending built-in column sorting.
- [x] 4.3 Add ZIP-like navigable-file coverage to runtime extension-byte sorting and prove navigation classification remains unchanged.
- [x] 4.4 Run focused and package-level tests, strict OpenSpec validation, and verify an installer-facing release build contains the correction (release SHA-256 `D62B05A94EE07C3B07FDA2F5AC168454383D85C74BDAC9F04BF8328801BD4921`; installer SHA-256 `3B9B38B79FC260585179CBADFAA27E5A182875239F0314F7BE7DF92D73580BF8`).
