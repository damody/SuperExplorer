## ADDED Requirements

### Requirement: Unqualified wildcard text uses filename glob semantics
Unqualified search text containing an unescaped `*` or `?` SHALL match the complete filename case-insensitively, where `*` represents zero or more Unicode scalar values and `?` represents exactly one Unicode scalar value.

#### Scenario: Extension glob
- **WHEN** the user searches for `*.rs`
- **THEN** every visible result within the active scope has a filename ending in `.rs` without regard to case

#### Scenario: Prefix, infix, and single-character globs
- **WHEN** the user searches with `foo*.rs`, `*test*`, or `file?.rs`
- **THEN** the visible results satisfy the corresponding complete-filename pattern

#### Scenario: Escaped wildcard
- **WHEN** the user escapes `*`, `?`, or backslash in an unqualified query
- **THEN** the escaped character is matched literally and cannot broaden the query

### Requirement: Plain text and typed extension filters remain compatible
Unqualified text without an unescaped wildcard SHALL retain case-insensitive substring semantics, and `type:`/`ext:` SHALL retain exact extension semantics.

#### Scenario: Plain substring query
- **WHEN** the user searches for `report`
- **THEN** filenames containing `report` remain eligible and wildcard matching is not applied

#### Scenario: Typed extension query
- **WHEN** the user searches for `type:rs` or `ext:rs`
- **THEN** entries whose extension equals `rs` without regard to case are eligible

### Requirement: Glob results are provider-independent and bounded
Everything, LocalIndex, and filesystem fallback SHALL apply the same final glob predicate while preserving folder scope, result limits, cancellation, and injection-safe provider rendering.

#### Scenario: Same fixture through every provider
- **WHEN** the same glob query and filesystem fixture are evaluated through each available backend
- **THEN** each backend produces the same set of visible paths before configured truncation

#### Scenario: Provider syntax characters in glob and scope
- **WHEN** a glob or canonical folder path contains quotes, backslashes, operators, or other Everything syntax characters
- **THEN** the query remains confined to the active folder and the characters cannot inject an additional Everything expression
