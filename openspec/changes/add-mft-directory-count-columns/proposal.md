## Why

SuperExplorer's MFT pipeline already produces recursive file and directory counts, but Details cannot display them and extensions cannot use them to avoid expensive folder calculations. A shared Host-owned admission path is needed so Code Lines and future data columns start only when exact MFT facts satisfy declared limits.

## What Changes

- Add default-hidden, toggleable built-in `File Count` and `Folder Count` Details columns with stable persistence, exact integer sorting, and MFT-only values.
- Introduce a deduplicated Host directory-facts projection shared by both columns and dependent extension contributions.
- Add optional inclusive `max_file_count` and `max_folder_count` admission limits to folder-applicable data-column contributions and enforce them before callback dispatch.
- Configure the Rust and Lua Code Lines contributions with `max_file_count = 999`, preserving file-item analysis while gating folder analysis.
- Show pending Code Lines state normally; show both over-limit and unavailable-dependency states as a compact red `Limit` label whose hover tooltip and accessible name retain the complete reason, without adding localization resources.
- Preserve existing extension behavior when no admission policy is declared and preserve legacy sessions with both new built-in columns hidden.
- Make count acquisition strictly visibility-driven: showing either built-in count column immediately starts the shared MFT query, while hiding both columns prevents count-only queries even when an enabled extension declares count limits.
- Require the corresponding built-in count column to be visible before a limited extension may consume that fact; Code Lines remains dependency-disabled while File Count is hidden.

## Capabilities

### New Capabilities

- `mft-directory-facts-columns`: Defines recursive MFT-only file/folder count semantics, shared request/cache behavior, Details presentation, sorting, invalidation, and unavailable handling.

### Modified Capabilities

- `extension-package-and-feature-lifecycle`: Adds validated optional folder admission metadata for folder-applicable data-column contributions.
- `extension-jobs-values-and-dynamic-columns`: Requires Host-side fact admission before dispatch and defines pending, rejected, stale, and compatibility behavior.
- `source-example-plugin-suite`: Requires both Code Lines examples to declare and demonstrate the fewer-than-1000-files folder gate.

## Impact

- Affects `explorer-model` column identity, descriptors, layout/session migration, sorting, and request/result types.
- Affects MFT query/service projection and the application-owned folder aggregate runtime, cache, invalidation, and scheduling paths.
- Affects extension manifest validation, public SDK metadata, Host job admission, both Code Lines fixtures, bundle manifests, and package tooling checks.
- Affects GPUI Details rendering, column chooser behavior, Code Lines state display, and focused/headful tests.
- Adds no dependency, privilege, installer-service, localization, or filesystem-fallback behavior.
