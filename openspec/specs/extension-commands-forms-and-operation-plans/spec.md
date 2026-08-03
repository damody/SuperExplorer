# extension-commands-forms-and-operation-plans Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Feature-scoped command and button registration
Rust and Lua extensions SHALL register commands and extension buttons with stable IDs, feature ID, localized label/icon, placement, selection predicate and optional shortcut. Registration SHALL be rejected when the manifest feature or capability is absent.

#### Scenario: Feature is disabled
- **WHEN** a command's feature becomes effectively disabled
- **THEN** its toolbar, extension-area and context-menu entries disappear and no new callback is dispatched

### Requirement: Host-rendered typed forms
The platform SHALL support versioned declarative forms containing text, integer, boolean, choice, authorized path and template fields with typed values, bounds and localized validation messages. Lua SHALL use host-rendered forms; Rust MAY supply a fingerprint-compatible GPUI settings/form renderer.

#### Scenario: Invalid count is submitted
- **WHEN** a bulk-create form submits a count outside its declared 1–100,000 range
- **THEN** the host rejects submission with a field-specific validation message before a plan is produced

### Requirement: Typed operation plans
Extensions SHALL express filesystem changes as typed plan steps such as create directory, rename, copy, move, delete, extract and archive mutation. Distributed extension callbacks SHALL NOT directly invoke OS mutation APIs for plan-covered operations.

#### Scenario: Lua proposes bulk directories
- **WHEN** a Lua extension requests 1,000 directory creations
- **THEN** it returns typed CreateDirectory steps and no directory is created until host validation and user approval complete

### Requirement: Operation validation and preview
Before execution, the host SHALL normalize paths and basenames, reject absolute/parent traversal, invalid/reserved Windows names and duplicate/case-insensitive targets, evaluate permissions and conflicts, and present changes, warnings, irreversible reasons and estimated work.

#### Scenario: Rename template escapes its folder
- **WHEN** a generated basename contains a separator, drive prefix or `..`
- **THEN** the validator rejects or safely sanitizes it according to declared policy and the preview never targets outside the authorized folder

#### Scenario: Large operation needs confirmation
- **WHEN** a bulk directory plan contains more than 1,000 steps
- **THEN** execution requires a second explicit confirmation showing count and representative names

### Requirement: Host execution, progress and cancellation
Approved plans SHALL execute through the existing file-operation pipeline with bounded batching, progress, cancellation, conflict policy, partial terminal result and diagnostic summary. Extensions SHALL not receive private operation/model references.

#### Scenario: Operation is cancelled partway
- **WHEN** the user cancels after some steps complete
- **THEN** the executor stops scheduling new steps, reports completed/failed/unattempted steps and preserves a safe undo record for completed reversible work

### Requirement: Conservative undo
The host SHALL record undo only where state can be safely restored. Bulk-created directories SHALL be removed on undo only if they were created by that plan and remain empty; archive mutation SHALL use a quota-managed original backup.

#### Scenario: Created directory now has content
- **WHEN** undo runs after a user added a file to one of the newly created directories
- **THEN** that directory is retained and listed as not reverted rather than recursively deleted

### Requirement: EXIF metadata decode and rename template
The Rust EXIF rename flow SHALL use a static Rust parser linked into the same plugin DLL and a capability-authorized `InputStreamV1`. It SHALL support rawname, extension, X/YResolution, PixelX/YDimension and DateTimeOriginal tokens and SHALL distinguish density tags from pixel dimensions.

#### Scenario: Clean machine executes EXIF rename
- **WHEN** the plugin runs with no exiftool, no specialist EXIF DLL, empty PATH and no network
- **THEN** it reads metadata from its own DLL, previews collisions/sanitization and submits a host rename plan

#### Scenario: Metadata token is missing
- **WHEN** a selected image lacks a token referenced by the template
- **THEN** preview reports the missing tag and does not silently generate an unsafe or ambiguous target

### Requirement: Static library and executable provenance
Plugin dependencies SHALL declare whether they are static Rust libraries or bundled executables. Static parsers SHALL be linked into plugin.dll and documented in SBOM/NOTICE; separate executables SHALL follow the bundled-tool capability and SHALL NOT masquerade as static libraries.

#### Scenario: EXIF plugin imports undeclared parser DLL
- **WHEN** PE import validation finds an undeclared non-system EXIF DLL
- **THEN** packaging or installation fails before the feature can run
