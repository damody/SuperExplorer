## 1. Batch Entry Point

- [x] 1.1 Add root-level `commit.bat` that resolves its own directory and launches `codex exec` with the approved model, reasoning, sandbox, and approval settings
- [x] 1.2 Embed the Chinese instruction covering content preservation, artifact exclusion, functional commit grouping, detailed Chinese logs, submodules, and push ordering
- [x] 1.3 Preserve the Codex exit code and print a concise success or failure message

## 2. Verification

- [x] 2.1 Statically inspect batch quoting, working-directory handling, CLI arguments, and embedded prompt content
- [x] 2.2 Confirm only intended files changed without executing the commit-and-push workflow
