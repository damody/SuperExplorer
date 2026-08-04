## ADDED Requirements

### Requirement: 0→1 single-plugin product validation
Before general platform implementation, the active milestone SHALL support exactly one explicitly selected local Rust plugin, `rust-folder-size-visual-column`. SuperExplorer SHALL load it only when launched with one absolute `--plugin-dll` path, SHALL reuse the real `abi_stable` root and registrar path, and SHALL show an owned read-only plugin/contribution summary in the existing Extensions menu. Absence of the flag SHALL preserve current behavior and SHALL NOT scan for unsigned local plugins. Multi-plugin discovery, package-source and the remaining examples SHALL not block this first canonical vertical slice.

#### Scenario: The single demo plugin is explicitly loaded
- **WHEN** SuperExplorer starts with a compatible absolute `rust_folder_size_visual_column.dll` path
- **THEN** the real host loads and registers it and the Extensions menu visibly identifies its plugin ID, contribution ID and contribution kind

#### Scenario: SuperExplorer starts without the demo flag
- **WHEN** SuperExplorer starts without `--plugin-dll`
- **THEN** no unsigned local plugin is scanned or loaded and no demo entry is shown

#### Scenario: The first slice is validated
- **WHEN** the plugin is visible in the real app and the app remains usable
- **THEN** an artificial contract, integration, evidence, snapshot, mock or fake framework is not required to declare product-validation GO; a manual demo or minimal smoke is sufficient

### Requirement: Installer bundles the single validation plugin
The `build_install.bat` release path SHALL build and validate the single `rust-folder-size-visual-column` release DLL locked and offline and SHALL package it as `$INSTDIR\plugins\rust_folder_size_visual_column.dll`. Installer-created shortcuts and the finish-page launch SHALL pass that absolute installed path through the existing `--plugin-dll` argument. Direct execution without that argument SHALL continue to load no unsigned local plugin.

#### Scenario: Installer build includes the plugin
- **WHEN** the application, broker, worker and `rust-folder-size-visual-column` release builds succeed
- **THEN** NSIS receives the exact validated DLL path and the generated installer contains `plugins\rust_folder_size_visual_column.dll`

#### Scenario: Installed shortcut launches the plugin
- **WHEN** the user launches SuperExplorer from an installer-created shortcut or the finish page
- **THEN** the process receives `--plugin-dll` with `$INSTDIR\plugins\rust_folder_size_visual_column.dll` and loads the bundled plugin through the existing explicit loader

#### Scenario: Plugin build or validation fails
- **WHEN** the fixture build fails or its expected release DLL is missing or invalid
- **THEN** installer creation stops without selecting an older, debug or alternate plugin binary

#### Scenario: Bundled plugin is uninstalled
- **WHEN** the user runs the uninstaller
- **THEN** it deletes the known `rust_folder_size_visual_column.dll` and removes the plugin directory only when empty, without recursively deleting unknown files

### Requirement: Complete independent example projects
The SDK SHALL ship eight installable `.sepack` example projects in an independent consumer workspace. Each SHALL include complete source, manifest, zh-TW/en README, locales, license/NOTICE/provenance, fixtures, unit/integration tests, screenshots and package command, and SHALL explain how an author can modify it.

#### Scenario: Example is built outside the product workspace
- **WHEN** a clean environment follows an example README using only the published SDK bundle
- **THEN** the example builds/tests/packages without access to SuperExplorer private source crates

### Requirement: Public SDK only
Examples SHALL depend only on public SDK crates, their precisely locked private dependencies and declared host capabilities. They SHALL NOT reference `explorer-ui`, `explorer-model`, `explorer-shell-win` or any other private workspace crate, nor bypass the composition root.

#### Scenario: Private crate dependency is introduced
- **WHEN** example metadata or source references a private workspace crate
- **THEN** the local offline dependency-validation gate rejects the example and the platform cannot be marked complete

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
`rust-folder-size-map-view` SHALL register a real view mode and return a complete-recursive incremental data-only treemap plan where area is logical size, nesting is folder hierarchy and default color is file type. Its synchronous callback SHALL run on a bounded host worker with a per-call marker; GPUI SHALL only paint the current host-minted revision and perform no renderer callback or I/O. It SHALL share selection, use formal double-click navigation, be accessible, and reject stale refresh/layout results.

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
- **THEN** package validation or the local release gate fails

### Requirement: Example-driven interface completion
Every public interface claimed by this change SHALL be exercised by at least one official example from the independent workspace. An interface implemented only as a trait, empty crate, mock or unwired code SHALL NOT satisfy completion.

#### Scenario: Host trait is never registered in production
- **WHEN** an interface compiles but no composition-root path and official example use it
- **THEN** the associated task remains incomplete and stable SDK publication is blocked

### Requirement: SDK and example release gate
The release integrator SHALL build/package all eight examples locally with the same approved bundle ID, networking disabled, and a pre-populated local Cargo registry cache. Third-party sources SHALL NOT be committed or vendor-tracked. Rust unit/integration gates SHALL run through exact `cargo test --locked --offline` commands; a missing cache entry SHALL be an explicit bootstrap prerequisite and SHALL NOT trigger network access. PowerShell contract gates SHALL run through exact `powershell -NoProfile -ExecutionPolicy Bypass -File <script>` commands; and every UI or headful gate SHALL run through the checked-out repository's `explorer-uitest` binary and `uitest/manifest.json` using `cargo run -p explorer-uitest --bin explorer-uitest --locked --offline -- --case <case-id>`. Each gate matrix entry SHALL declare exactly one command or manual review procedure, working directory, environment, expected exit status and required artifacts. Stable SDK publication SHALL require every local unit, integration, contract, UITEST, manifest-capability, security, performance and documentation-reproduction check to pass.

#### Scenario: One example fails clean reproduction
- **WHEN** any README build, `.sepack` validation, UITEST or capability mapping fails
- **THEN** no stable SDK/release bundle is published even if the other seven examples pass

### Requirement: UITEST sequencing and non-substitution
Phases 1–5 and every Task 6 activity before its final gate SHALL execute zero UITEST cases. The first UITEST SHALL be Task 6's final gate and SHALL run only after every other Task 6 leaf is complete end-to-end, including its framework, consumer contract, implementation, production wiring, SDK surface, fixtures, documentation and deterministic package. A later official example's UITEST SHALL run only after that example has separately completed the same production-wiring, SDK, fixture, documentation and deterministic-package prerequisites. UITEST SHALL NOT substitute for local unit, integration, ABI or PowerShell contract gates and SHALL NOT close incomplete, mock-only or source-shape-only work.

#### Scenario: A pre-final phase 6 gate requests a UI test
- **WHEN** a phase 1–5 gate or any phase 6 gate before the final example gate attempts to execute an `explorer-uitest` case
- **THEN** the plan is invalid because no UITEST executes before phase 6 is complete

#### Scenario: Phase 6 reaches its final example gate
- **WHEN** every Task 6 leaf before the final gate has completed its framework, consumer contract, production wiring, SDK surface, fixtures, documentation and deterministic package obligations
- **THEN** Task 6's final gate is the first UITEST and runs through `explorer-uitest` and `uitest/manifest.json`

#### Scenario: A later official example is only partially complete
- **WHEN** a later example lacks any production wiring, SDK, fixture, documentation or deterministic-package prerequisite
- **THEN** its UITEST does not run and the incomplete work remains open to its required non-UITEST gates

### Requirement: Local validation and release-evidence authority
Completion, stable SDK publication and release readiness SHALL be established only by successful local gate records and a signed, retained local release evidence bundle. The bundle SHALL be deterministic, store-only and content-addressed, and SHALL contain the evidence manifest, exact commands or manual procedures, task and unique subcheck IDs, expected and actual results, source/environment metadata, SHA-256 inventory, RC identity and retention metadata. CI, GitHub Actions, remote artifacts and `ci://` locators SHALL NOT be required for any gate or evidence lookup and SHALL NOT confer completion.

#### Scenario: An external automation service reports success
- **WHEN** CI, GitHub Actions or a remote artifact service reports a successful run without the required locally executed gate records and verified signed local release evidence bundle
- **THEN** the associated task and release gate remain incomplete

### Requirement: Rust-only AI prompt fixture
The SDK SHALL ship a Rust-only AI development prompt that reads machine-readable SDK/manifest data, uses the current bundle rev and official scripts, avoids private crates and generates manifest/tests/locales. A maintained fixture SHALL build, validate and package a minimal provider plus GPUI renderer.

#### Scenario: Prompt attempts to follow GPUI main
- **WHEN** generated code replaces the snapshot rev with the current branch HEAD or another locked dependency
- **THEN** validator rejects it and directs the author to the approved bundle ID
