## MODIFIED Requirements

### Requirement: Dynamic column registry
The system SHALL represent built-in and extension columns with stable IDs and descriptors
containing value type, width, alignment, applicability, sort semantics and cost. Single, batch and
aggregate providers and feature-gated GPUI renderers SHALL be registrable. In Details view,
registry availability and ordered-layout presentation SHALL remain separate. Header and row SHALL
use the same visible ordered-layout projection. Name SHALL always be visible and first; every other
visible column SHALL be horizontally draggable and its order SHALL persist across restart and
extension disable/re-enable.

#### Scenario: Non-Name column is dragged left
- **WHEN** a user drags a visible non-Name header across another header midpoint
- **THEN** header and row cells move to the same insertion position and a restart restores it

#### Scenario: Name is dragged
- **WHEN** a user attempts to drag Name or drop another column before Name
- **THEN** Name remains first and no data/header identity is displaced

#### Scenario: Extension is disabled and re-enabled
- **WHEN** a reordered extension column is disabled and later re-enabled with the same stable ID
- **THEN** its retained order, width and visibility return without moving neighboring columns

### Requirement: Distinct code-line aggregation columns
Code lines SHALL display the sum of code lines across every recognized language. Main code lines
SHALL aggregate code per language across the item subtree, select only the greatest aggregate with
ascending language-name tie resolution, and display `Language: N`. Their cache identities SHALL be
semantically distinct and sorting SHALL use their respective raw numeric results.

#### Scenario: Mixed-language directory is measured
- **WHEN** a directory contains 1,250 Rust code lines and 75 recognized non-Rust code lines
- **THEN** Code lines displays `1325` and Main code lines displays `Rust: 1,250`

#### Scenario: Single-language directory is measured
- **WHEN** all recognized code belongs to one language
- **THEN** the two numeric counts may be equal while Main code lines still includes the language

#### Scenario: Main-language tie occurs
- **WHEN** two languages have equal aggregate code counts
- **THEN** Main code lines selects the language whose name sorts first ascending
