## ADDED Requirements

### Requirement: Validated folder admission metadata
Any extension package's folder-applicable data-column contribution SHALL be allowed to declare optional inclusive `max_file_count` and `max_folder_count` admission limits as JSON integers in `0..=u64::MAX`. Missing fields SHALL mean unlimited, both present SHALL use AND semantics, and zero SHALL be valid. The package validator SHALL reject malformed values and policies attached to non-column or file-only contributions.

#### Scenario: Folder column declares both limits
- **WHEN** a valid folder-applicable data-column contribution declares both admission fields
- **THEN** the validated registration retains both exact inclusive limits for Host-side dispatch admission

#### Scenario: Existing manifest omits the policy
- **WHEN** an existing valid data-column manifest has neither admission field
- **THEN** validation and registration preserve its current unlimited behavior

#### Scenario: Policy is attached to an inapplicable contribution
- **WHEN** a command, view, renderer-only, or file-only column declares a folder admission limit
- **THEN** package validation rejects the contribution with a typed diagnostic before any callback executes
