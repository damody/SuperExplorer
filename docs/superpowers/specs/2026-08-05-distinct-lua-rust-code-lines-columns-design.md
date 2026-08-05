# Distinct Lua and Rust Code Lines Columns

## Goal

Keep the Lua and Rust code-line extensions independently identifiable and usable at the same time. The Lua extension contributes `Code lines`. The Rust extension contributes `Main code lines`, which reports only the source language with the greatest aggregate number of code lines.

## Column identity and coexistence

- The Lua column display name is `Code lines`.
- The Rust column display name is `Main code lines`.
- The two contributions retain distinct package and column identities.
- Enabling both extensions registers and displays both columns concurrently in Details view; enabling, disabling, sorting, or updating one must not replace or hide the other.

## Rust aggregation and display

For a regular source file, the Rust provider reports the detected language and that file's code-line count. For a directory, it scans the bounded directory input, groups supported files by detected language, and adds code, comment, blank, and total counts within each language. It selects the language whose aggregate `code` count is greatest.

If multiple languages have the same aggregate code-line count, the lexicographically smaller language name wins so the result is deterministic. Unsupported, binary, and invalid-text inputs do not contribute to an aggregate. A directory with no supported source files remains unsupported.

The visible cell label uses `Language: N` with locale-independent comma grouping for the integer, for example `Rust: 1,250`. The numeric sort value remains the selected language's unformatted aggregate code-line count. Optional detail text describes only the selected language's aggregate comment, blank, and total counts.

## Lua behavior

Apart from the display name `Code lines`, the Lua provider's existing statistics, label formatting, sorting, and error behavior remain unchanged.

## Integration

Host code must track extension columns by their stable identities rather than treating all code-line columns as one active slot. Refresh, background job results, render plans, caches, and sorting must route to the matching Lua or Rust column. Existing unrelated worktree changes must be preserved.

## Verification

Automated coverage must verify:

- the two descriptors have different stable identities and the exact names `Code lines` and `Main code lines`;
- both columns can be registered and visible concurrently;
- a regular Rust-supported file renders the language and count;
- multiple files of one language are aggregated before comparison with other languages;
- only the language with the highest aggregate code-line count is shown;
- ties resolve by ascending language name;
- `1,250`-style comma grouping is rendered while sorting remains numeric;
- unsupported inputs do not become misleading zero counts.

Headful verification must install or enable both fixture extensions, open a mixed-language fixture in Details view, and capture a screenshot that visibly contains both `Code lines` and `Main code lines` headers at the same time. The `Main code lines` cells must visibly use labels such as `Rust: 1,250`. If the screenshot reveals missing columns, clipped labels, incorrect aggregation, stale results, or ambiguous headers, implementation and validation repeat until the acceptance criteria are met. The final screenshot and relevant test output are retained as verification evidence.
