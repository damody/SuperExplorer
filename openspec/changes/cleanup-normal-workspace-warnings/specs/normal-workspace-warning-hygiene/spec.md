## ADDED Requirements

### Requirement: Normal workspace build is warning-free
The repository SHALL complete `cargo check --workspace --locked --offline` with zero compiler warnings and zero errors.

#### Scenario: Normal locked offline build
- **WHEN** the normal workspace check is run on the supported Windows development environment
- **THEN** the command exits successfully and emits no warning diagnostics

### Requirement: All-target workspace build is warning-free
The repository SHALL complete `cargo check --workspace --all-targets --locked --offline` with zero rustc warnings and zero errors.

#### Scenario: Locked offline all-target build
- **WHEN** all workspace libraries, binaries, and test targets are checked
- **THEN** the command exits successfully and emits no rustc warning diagnostics

### Requirement: Cleanup preserves runtime semantics
Warning cleanup SHALL preserve public interfaces, control flow, selected match branches, resource ownership, and destructor timing.

#### Scenario: Unread RAII guard
- **WHEN** a binding exists solely to keep a Win32 event handle alive until scope exit
- **THEN** the binding is retained in the same scope and only renamed to express intentional non-reading

#### Scenario: Lexical lint cleanup
- **WHEN** a redundant qualification, import, mutable marker, or pattern binding is removed
- **THEN** the resolved symbol, branch behavior, and resulting value remain unchanged

### Requirement: Cleanup is manual and narrowly scoped
The implementation MUST NOT use `cargo fix`, add lint suppression, or modify diagnostics outside the normal workspace build merely to satisfy this change.

#### Scenario: Implementation execution
- **WHEN** compiler-reported sites are corrected
- **THEN** each correction is made and reviewed manually without broad automated rewriting or new `allow`/`expect` attributes
