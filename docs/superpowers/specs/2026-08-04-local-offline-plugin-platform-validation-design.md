# Local Offline Validation for the Extensible Plugin Platform

## Decision

`build-extensible-plugin-platform` SHALL never use CI, GitHub Actions, a remote
artifact service, or a `ci://` locator as an implementation gate, completion
gate, evidence authority, or release prerequisite.

All validation SHALL run from the checked-out repository on the release
integrator's Windows workstation with networking disabled where the gate is
defined as offline. An external automation system may not substitute for,
weaken, or become authoritative over this local process.

## Validation architecture

Validation has three local layers:

1. Rust unit and integration tests run directly with exact `cargo test`
   commands using `--locked --offline`.
2. PowerShell contract tests run directly with `powershell -NoProfile
   -ExecutionPolicy Bypass -File <script>` and must emit deterministic reports.
3. UI and headful behavior runs through the repository's own
   `explorer-uitest` binary and `uitest/manifest.json`:

   ```powershell
   cargo run -p explorer-uitest --bin explorer-uitest --locked --offline -- --case <case-id>
   ```

Each gate matrix entry declares exactly one command or manual review procedure,
its working directory and environment, expected exit status, and required
artifacts. Unit, contract, and UITEST gates remain distinct and cannot stand in
for one another.

## Evidence flow

Raw logs, reports, screenshots, and machine-readable UITEST results are written
under `target/openspec-evidence/build-extensible-plugin-platform/<task-id>/`.
They do not close a task while they remain mutable working files.

The local evidence packager creates a deterministic, store-only,
content-addressed release evidence bundle containing:

- an evidence manifest;
- exact commands or manual procedures;
- task and unique subcheck IDs;
- expected and actual results and exit status or reviewer;
- environment and source revision metadata;
- every contained file's SHA-256;
- bundle/RC identity, retention policy, and creation timestamp.

The bundle is signed in the dedicated evidence-signing namespace. Verification
uses a release-integrator-owned trust policy separate from extension publisher
keys. The validator rejects unsigned bundles, untrusted principals or keys,
hash mismatches, path traversal, duplicate normalized paths, reparse-point
escapes, oversized archives, invalid retention metadata, and bundles that do
not bind the current task/subcheck and source revision.

`release://` is a logical content-addressed locator resolved only against an
explicit local retained-bundle root. It is never translated to a network URL.

## Snapshot and release flow

GPUI snapshot discovery may read the configured upstream only during an
explicit primary-agent update operation. Candidate generation, host/plugin
builds, fixtures, tests, promotion, rollback, release freeze, and evidence
verification are local operations. A candidate is promoted only after every
required local unit, contract, and UITEST gate succeeds.

Release freeze binds the protected source revision, dependency locks, vendor
tree, SDK bundle ID, UI ABI fingerprint, local gate results, and signed evidence
bundle. Existing releases are immutable; corrections require a new RC and
bundle ID.

## Failure handling

- Missing tools, signing authority, test cases, artifacts, or retained bundles
  fail closed.
- A failed, blocked, stale, unexecuted, trait-only, or mock-only result cannot
  complete a leaf.
- Superseded work retains its prior evidence, invalidates transitive
  dependents, and requires explicit stale-to-revalidated lineage.
- No task may be checked merely because an external service reports success.
- Historical task text mentioning CI remains unchanged inside legacy lineage
  records, but is never treated as current policy.

## OpenSpec changes

The existing proposal, design, relevant delta specs, and detailed task plan
will be revised in place. Current CI/workflow requirements will become local
runner, `explorer-uitest`, offline packaging, or signed-release-evidence
requirements. Ownership of shared workflows will be removed from the change;
the release integrator instead owns local orchestration, the UITEST manifest,
the evidence ledger, trust policy, and final signed bundle.

The change remains detailed and multi-agent. Permanent task IDs are preserved
where their semantics can be corrected without ambiguity; any split or
replacement retains append-only lineage. No completed checkbox is inherited
without locally reproducible evidence.

## Non-goals

- No GitHub Actions workflow is added or executed.
- No GitHub token, remote CI artifact, `ci://` locator, or hosted release gate is
  required.
- The local evidence trust root is not shared with plugin publisher signing.
- Existing unrelated repository workflows are not deleted by this design.
