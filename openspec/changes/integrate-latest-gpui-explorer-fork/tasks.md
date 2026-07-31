## 1. Record integration inputs

- [x] 1.1 Fetch upstream and target remotes and record their current main commits
- [x] 1.2 Verify the vendored tree is clean and identify the ordered Explorer-only commit range

## 2. Integrate the fork

- [x] 2.1 Create an integration branch from the latest upstream main commit
- [x] 2.2 Replay external-drop and editable-text commits in their original order
- [x] 2.3 Resolve conflicts while retaining the public APIs used by SuperExplorer

## 3. Validate compatibility

- [x] 3.1 Run formatting, diff, and relevant fork crate checks/tests
- [x] 3.2 Update SuperExplorer to the candidate submodule and matching path dependencies
- [x] 3.3 Regenerate the lockfile and verify only the fork GPUI graph is used
- [x] 3.4 Run explorer-ui tests, Clippy, and explorer-app build with Rust 1.97.1

## 4. Publish and record

- [x] 4.1 Fetch target main again and verify the candidate is a fast-forward
- [x] 4.2 Commit the integrated fork history and push to damody/gpui-ce-explorer main without force
- [x] 4.3 Record the published fork commit in the parent submodule pointer and validate OpenSpec strictly
