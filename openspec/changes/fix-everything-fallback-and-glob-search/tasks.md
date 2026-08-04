## 1. Shared Glob Contract

- [x] 1.1 Extend parsed unqualified text to preserve escaped wildcard intent without changing plain substring or typed-filter behavior
- [x] 1.2 Implement the bounded case-insensitive Unicode filename glob matcher in `explorer-search`
- [x] 1.3 Route LocalIndex and filesystem traversal through the shared matcher and add parser/matcher regression tests

## 2. Everything Integration

- [x] 2.1 Render escaped Everything filename glob candidates while preserving mandatory canonical folder scope
- [x] 2.2 Apply the shared matcher to Everything candidates and add cross-provider/escaping contract tests
- [x] 2.3 Treat successful zero-result Everything queries as finished, and verify unavailable/error failover plus cancellation-without-fallback
- [x] 2.4 Correct startup/backend diagnostics so they do not claim the legacy WindowsIndex source
- [x] 2.5 Require and package `Everything64.dll` beside `SuperExplorer.exe`, remove it on uninstall, and enforce it in installer smoke validation
- [x] 2.6 Use Everything-compatible `path:` scoping, angle-bracket grouping, and unquoted escaped filename glob candidates

## 3. Verification

- [x] 3.1 Run formatting and focused `explorer-search` and `explorer-shell-win` tests
- [x] 3.2 Run relevant Clippy, workspace all-target tests, and release build without regressing existing search contracts
- [x] 3.3 Validate the OpenSpec change strictly, record completed tasks, and review the final diff for unrelated changes
- [x] 3.4 Compile the NSIS installer with the Everything SDK input and inspect the packaged file table
- [x] 3.5 Verify the exact production `path:"D:\SuperExplorer" <*.rs>` query against the live SDK and confirm scoped `.rs` results
