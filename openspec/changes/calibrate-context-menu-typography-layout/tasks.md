## 1. Typography contract

- [x] 1.1 Correct the shared context-menu fallback font size to 12 logical pixels and update its model tests.
- [x] 1.2 Project menu font size, line height, and weight from `TypographyTokens::menu` into the remote visual tokens.

## 2. Remote rendering

- [x] 2.1 Apply explicit family, size, line height, and weight to every remote command row while preserving existing geometry and behavior.
- [x] 2.2 Add focused tests for typography values, row fit, theme independence, fallback order, and unchanged layout constants.

## 3. Final validation

- [x] 3.1 Run formatting plus focused explorer-model, explorer-ui, and explorer-shell-win tests/build checks.
- [x] 3.2 Attempt representative ADB and SFTP visual validation and record successful evidence or the exact automation prerequisite that prevented it.
- [x] 3.3 Review the final diff and validate the OpenSpec change strictly with every requirement traced to code or evidence.
