## Context

`vendor/gpui-ce` is a detached submodule at `0cd06bd8cc`, consisting of upstream base `6c799b8e99` plus four Explorer-specific commits. The current upstream and `damody/gpui-ce-explorer` main branches both point to `33ed975bf2` at integration start. SuperExplorer consumes the modular `gpui`, `gpui_windows`, and `gpui_elements` crates from this tree and requires APIs that crates.io `gpui-ce 0.3.3` does not contain.

## Goals / Non-Goals

**Goals:**

- Rebase the four isolated Explorer commits onto the recorded latest upstream commit.
- Keep a linear, auditable fork history that can be refreshed again later.
- Preserve all APIs and behaviors required by the current SuperExplorer source.
- Prove compatibility using fork tests plus the parent application's locked build and focused UI tests.
- Publish with a normal fast-forward update to the owned fork.

**Non-Goals:**

- Replacing the modular fork with crates.io's monolithic `gpui-ce 0.3.3` package.
- Force-pushing, squashing upstream history, or rewriting an already-published fork commit.
- Adding unrelated GPUI features or refactoring SuperExplorer UI code.

## Decisions

### Rebase Explorer commits onto a recorded upstream commit

Fetch upstream immediately before integration and record its `main` object ID. Create the integration branch from that object and cherry-pick the four commits in original order. This keeps the upstream boundary obvious and avoids a merge commit containing thousands of unrelated historical paths. A whole-tree copy was rejected because it loses provenance; merging the old detached head was rejected because it obscures which changes belong to Explorer.

### Preserve the modular path dependency

After integration, SuperExplorer SHALL consume `gpui`, `gpui_windows`, and `gpui_elements` from the fork submodule. The failed crates.io probe demonstrated that the monolithic 0.3.3 package lacks required accessibility, editable-text, and external-drop APIs. The fork is therefore the production source until those extensions are accepted upstream or a later migration replaces them.

### Validate before publication

The integration branch is tested locally before any push. Validation includes formatting/diff checks, relevant GPUI crate tests or checks, SuperExplorer dependency resolution, `explorer-ui` tests, and `explorer-app` build. Only a passing commit may be pushed to the fork `main` branch. Push is allowed only when it is a fast-forward from the remote tip observed at the start or after a final fetch.

### Keep parent and fork commits separate

Explorer modifications live in the fork commit history. The parent repository records only the submodule object ID, dependency declarations, lockfile, OpenSpec artifacts, and validation evidence. This prevents duplicated source patches and makes rollback a single submodule-pointer change.

## Risks / Trade-offs

- [Upstream conflicts with editable-text or Windows platform code] → Resolve per public behavior, then run both fork and host tests before continuing.
- [Remote fork advances during integration] → Fetch immediately before push and refuse non-fast-forward publication; replay onto the new tip if necessary.
- [Fork tests pass but host APIs regress] → Treat the SuperExplorer build and explorer-ui tests as mandatory publication gates.
- [Future upstream refresh repeats manual work] → Retain one commit per Explorer concern and document the upstream base commit in the integration commit message.

## Migration Plan

1. Fetch upstream and fork remotes and record both main tips.
2. Create an integration branch from the latest upstream main.
3. Cherry-pick the four Explorer commits in chronological order and resolve conflicts.
4. Validate fork crates.
5. Point the parent submodule and path dependencies at the candidate commit; update the lockfile.
6. Build and test SuperExplorer with Rust 1.97.1.
7. Re-fetch the fork, verify fast-forward ancestry, and push the candidate to fork main.
8. Record the pushed commit in the parent submodule pointer.

Rollback changes the parent submodule pointer back to `0cd06bd8cc` and restores the prior lockfile. No remote history rewrite is required.

## Open Questions

None. The upstream and target branch are discoverable, the required custom commit set is isolated, and the user authorized publication to the named repository.
