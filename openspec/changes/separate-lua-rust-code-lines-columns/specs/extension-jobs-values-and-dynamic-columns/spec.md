## MODIFIED Requirements

### Requirement: Dynamic column registry
The system SHALL represent built-in and extension columns with stable IDs and descriptors containing value type, width, alignment, applicability, sort semantics and cost. Single, batch and aggregate providers and feature-gated GPUI renderers SHALL be registrable. Concurrently enabled extension columns SHALL retain independent visibility, provider/runtime ownership, values, render plans, cache entries, refresh generations, and sort routing by complete stable column identity, including when their purposes or value types are similar.

#### Scenario: Plugin column is enabled
- **WHEN** a valid extension registers a column and provider
- **THEN** the column appears in the column chooser, can be displayed, resized, reordered and sorted like a built-in column

#### Scenario: Two code-line providers are enabled
- **WHEN** distinct Lua and Rust packages register columns with separate stable IDs
- **THEN** both columns can be visible and populated concurrently and a result, refresh, renderer, cache entry, or sort action for one does not replace or mutate the other

#### Scenario: One of two providers is disabled and re-enabled
- **WHEN** one code-line provider is disabled while the other remains enabled and is later re-enabled
- **THEN** the remaining column continues to function independently and the re-enabled stable ID can recover its retained layout without adopting the sibling column's state
