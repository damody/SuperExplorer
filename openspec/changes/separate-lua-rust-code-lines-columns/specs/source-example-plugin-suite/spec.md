## MODIFIED Requirements

### Requirement: Rust tokei column example
`rust-tokei-code-lines-column` SHALL use a locked Rust tokei library in its DLL to return language, code, comment, blank and total counts in bounded batches, with a numeric selected sort metric and no OS process per file. Its column display name SHALL be `Main code lines`. For a bounded directory input it SHALL aggregate statistics by detected language, select the language with the greatest aggregate code count, break equal-code ties by ascending language name, and expose only that language's aggregate. Its visible label SHALL use `Language: N` with comma-grouped code lines while its stable sort value remains the unformatted selected code count.

#### Scenario: Mixed language fixture is analyzed
- **WHEN** Rust, C/C++, Python, Lua, JavaScript, empty, invalid-text and unknown files are processed
- **THEN** supported files receive typed counts, unsupported files are not reported as zero and the test observes no per-file process creation

#### Scenario: Directory has multiple files per language
- **WHEN** a bounded directory contains several supported files whose per-language sums differ from the largest individual file
- **THEN** the column selects the language with the largest aggregate code count and returns that language's aggregate code, comment, blank and total counts

#### Scenario: Main-language counts are tied
- **WHEN** two supported languages have the same aggregate code count
- **THEN** the lexicographically smaller language name is selected deterministically

#### Scenario: Main-language value is rendered and sorted
- **WHEN** the selected Rust aggregate has 1,250 code lines
- **THEN** the visible label is `Rust: 1,250` and sorting uses numeric value `1250`

### Requirement: Lua tokei column example
`lua-tokei-code-lines-column` SHALL package its exact `windows-x64` `tokei.exe`, license and hash and invoke it only through `tools.execute_bundled`/ToolHandle with shell-free bounded batches and JSON mapping. Its column display name SHALL be `Code lines`, and its existing statistics, rendering, sorting, and failure semantics SHALL remain unchanged.

#### Scenario: Tool payload is tampered
- **WHEN** the packaged tokei hash differs or the executable is removed while another tokei exists on PATH
- **THEN** the feature is blocked before callback and no fallback executable is used

#### Scenario: Lua and Rust examples are enabled together
- **WHEN** both official tokei example packages are enabled in Details view
- **THEN** `Code lines` and `Main code lines` are simultaneously visible as separate populated columns and retained screenshot evidence shows both exact headers

