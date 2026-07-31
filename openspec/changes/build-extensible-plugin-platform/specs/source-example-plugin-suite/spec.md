## ADDED Requirements

### Requirement: Complete independent example projects
The SDK SHALL ship eight installable `.sepack` example projects in an independent consumer workspace. Each SHALL include complete source, manifest, zh-TW/en README, locales, license/NOTICE/provenance, fixtures, unit/integration tests, screenshots and package command, and SHALL explain how an author can modify it.

#### Scenario: Example is built outside the product workspace
- **WHEN** a clean environment follows an example README using only the published SDK bundle
- **THEN** the example builds/tests/packages without access to SuperExplorer private source crates

### Requirement: Public SDK only
Examples SHALL depend only on public SDK crates, their precisely locked private dependencies and declared host capabilities. They SHALL NOT reference `explorer-ui`, `explorer-model`, `explorer-shell-win` or any other private workspace crate, nor bypass the composition root.

#### Scenario: Private crate dependency is introduced
- **WHEN** example metadata or source references a private workspace crate
- **THEN** CI rejects the example and the platform cannot be marked complete

### Requirement: Folder-size visual column example
`rust-folder-size-visual-column` SHALL recursively calculate folder bytes in background, expose exact numeric sorting and largest-sibling aggregation, render a customizable GPUI cell, support cancellation/partial errors/cache invalidation and register separate column/recalculate/settings features.

#### Scenario: One thousand sibling items are displayed
- **WHEN** the example scans a 1,000-item fixture containing inaccessible paths and a symlink cycle
- **THEN** the list remains interactive, valid rows sort by exact bytes, errors remain partial/typed and the GPUI renderer draws only from background results

### Requirement: Rust tokei column example
`rust-tokei-code-lines-column` SHALL use a locked Rust tokei library in its DLL to return language, code, comment, blank and total counts in bounded batches, with a numeric selected sort metric and no OS process per file.

#### Scenario: Mixed language fixture is analyzed
- **WHEN** Rust, C/C++, Python, Lua, JavaScript, empty, invalid-text and unknown files are processed
- **THEN** supported files receive typed counts, unsupported files are not reported as zero and the test observes no per-file process creation

### Requirement: Lua tokei column example
`lua-tokei-code-lines-column` SHALL package its exact `windows-x64` `tokei.exe`, license and hash and invoke it only through `tools.execute_bundled`/ToolHandle with shell-free bounded batches and JSON mapping.

#### Scenario: Tool payload is tampered
- **WHEN** the packaged tokei hash differs or the executable is removed while another tokei exists on PATH
- **THEN** the feature is blocked before callback and no fallback executable is used

### Requirement: Lua bulk-folder example
`lua-bulk-folder-generator` SHALL register an extension button and host form for parent, prefix, start, count, zero padding, suffix and conflict policy. It SHALL submit a create-directory plan for 1–100,000 items, request second confirmation above 1,000, support progress/cancel and conservatively undo only empty directories it created.

#### Scenario: User cancels after partial creation
- **WHEN** some directories have been created and one later gains user content
- **THEN** cancellation reports partial state and undo removes only still-empty plan-created directories

### Requirement: Self-contained Rust EXIF rename example
`rust-exif-rename-command` SHALL statically link its Rust EXIF parser into plugin.dll, read via `InputStreamV1`, support documented metadata tokens and preview basename sanitization/missing tags/case-insensitive collisions before an undoable host rename plan. It SHALL NOT require exiftool, a specialist external DLL, PATH or network.

#### Scenario: EXIF example runs on a clean machine
- **WHEN** tests run offline with empty PATH and no EXIF executable/DLL
- **THEN** plugin.dll reads valid metadata, distinguishes density from pixel dimensions and safely previews/applies/undoes the rename

### Requirement: Lock-owner column example
`rust-lock-owner-column` SHALL use `LockOwnerQueryServiceV1` in a background batch provider, display one/multiple process names and details, provide manual refresh, use short TTL and update/clear through F5 generation without process-control capability.

#### Scenario: Lock appears and disappears
- **WHEN** a helper holds a file across one F5 and releases it before the next
- **THEN** the owner name appears then clears, and a late old-generation result cannot restore it

### Requirement: Editable 7z virtual-folder example
`rust-7z-virtual-folder` SHALL use a locked pure-Rust backend to navigate, preview/stream, extract/copy, add, create folder, delete, rename and move entries with safe normalization, encrypted secret handling, resource limits, staging verification, atomic replacement and whole-container undo.

#### Scenario: Mutation fails before commit
- **WHEN** low disk, cancellation, CRC/header verification or original-file race aborts a mutation
- **THEN** the original archive remains bit-for-bit unchanged and temporary staging is cleaned

### Requirement: Folder Size Map view example
`rust-folder-size-map-view` SHALL register a real view mode and render a complete-recursive incremental GPUI treemap where area is logical size, nesting is folder hierarchy and default color is file type. It SHALL share selection, use formal double-click navigation, be accessible, refresh by generation and perform no renderer I/O.

#### Scenario: User explores and refreshes Size Map
- **WHEN** the user selects a rectangle, double-clicks a folder and presses F5 while an old scan remains active
- **THEN** selection is shared, formal tab history changes, the new location is scanned and stale deltas/layout never overwrite it

### Requirement: Example feature controls
All examples SHALL appear in Folder Options/Extensions and expose their declared independent features. Disabling a provider SHALL also block/remove dependent renderers; disabling a column/view SHALL preserve recoverable user layout state and safely fallback active tabs.

#### Scenario: Size Map feature is disabled while active
- **WHEN** the user applies its feature off state
- **THEN** affected tabs fall back to a built-in view and the saved extension view ID can be recovered after re-enable

### Requirement: Third-party dependency classification
Every example SHALL classify dependencies as static Rust libraries or bundled executables. Static libraries SHALL be linked and listed in SBOM/NOTICE; executables SHALL be package content validated by target/hash/license and never downloaded at runtime.

#### Scenario: Dependency classification is incomplete
- **WHEN** an example needs a parser or executable but omits its payload/provenance/license classification
- **THEN** package validation or CI release gate fails

### Requirement: Example-driven interface completion
Every public interface claimed by this change SHALL be exercised by at least one official example from the independent workspace. An interface implemented only as a trait, empty crate, mock or unwired code SHALL NOT satisfy completion.

#### Scenario: Host trait is never registered in production
- **WHEN** an interface compiles but no composition-root path and official example use it
- **THEN** the associated task remains incomplete and stable SDK publication is blocked

### Requirement: SDK and example release gate
CI SHALL build/package all eight examples with the same approved bundle ID in an isolated empty Cargo home, offline, and SHALL run unit, integration, UITEST, manifest-capability, security, performance and documentation-reproduction checks. Stable SDK publication SHALL require all checks to pass.

#### Scenario: One example fails clean reproduction
- **WHEN** any README build, `.sepack` validation, UITEST or capability mapping fails
- **THEN** no stable SDK/release bundle is published even if the other seven examples pass

### Requirement: Rust-only AI prompt fixture
The SDK SHALL ship a Rust-only AI development prompt that reads machine-readable SDK/manifest data, uses the current bundle rev and official scripts, avoids private crates and generates manifest/tests/locales. A maintained fixture SHALL build, validate and package a minimal provider plus GPUI renderer.

#### Scenario: Prompt attempts to follow GPUI main
- **WHEN** generated code replaces the snapshot rev with the current branch HEAD or another locked dependency
- **THEN** validator rejects it and directs the author to the approved bundle ID
