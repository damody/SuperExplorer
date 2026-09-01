# Final traceability and diff review

Recorded at: 2026-08-30T22:30:29.3212509+08:00
Reviewer: Primary agent
Gate: G-INTEGRATION

## Requirement traceability

| Requirement / scenario group | Implementation | Tests |
| --- | --- | --- |
| Deterministic conventional, compound, compressed, and extensionless Type labels | `crates/explorer-model/src/file_presentation.rs`; `crates/explorer-app/src/remote_service.rs` | model ordered-grammar tests; app dual-constructor matrix |
| Linux dotfile `Setting File` labels | shared classifier and remote metadata formatter | `.bashrc`, `.bash_logout`, `.profile`, `.gitignore`, separators, Unicode, and malformed-dotfile cases |
| Provider-authoritative directory/symlink semantics | `remote_type_display` matches `RemoteEntryKind`; classifier is called only for files/file symlinks | all six kinds plus both row constructors |
| Stable built-in categories including pdf/txt/jpg/tar.gz/bin.gz/tgz | `RemoteFileIconKind`, centralized extension map, `remote_file_icon_spec`, scalable renderer | every family, required examples, metadata uniqueness, 16–512 geometry |
| Remote-only behavior and local preservation | `remote_file_fallback_icon_kind` gates ADB/SFTP and rejects containers; chrome retains folder and local document branches | ADB, SFTP, extension-provider, local-filesystem, folder, and unknown cases |

## Scope audit

- There is exactly one production definition of `classify_remote_file_name`; app and UI import it instead of duplicating extension tables.
- The classifier contains no filesystem, network, MIME, Windows Shell, or registry access.
- Filename classification never sets `is_container` or changes identity, capabilities, commands, transfers, sorting, or navigation.
- The Windows Shell/thumbnail submission exclusion for ADB/SFTP remains unchanged; built-in icons are selected only after a texture miss.
- The complete approved initial extension families are present, including every requested `pdf`, `txt`, `jpg`, `tar.gz`, `bin.gz`, and `tgz` example.
- Existing uncommitted bookmark, clipboard, cache, layout, and other user-owned edits in shared files were retained. This change touched only narrow remote metadata and fallback-rendering hunks inside those files.
- `git diff --check` passes. No new dependency, asset license, unsafe code, I/O, persistence, ABI, or destructive operation was introduced.

## Final source hashes

- `crates/explorer-model/src/file_presentation.rs`: `CA037DADFEB44FC382F9A3AA0AD3EF4E0CDFD7674BFF3F568F3F61E5F372F4F8`
- `crates/explorer-model/src/lib.rs`: `0A2B0221D9979BD6BAFA0237ED8AE700B1074BB4735465C07216F563D63AF814`
- `crates/explorer-app/src/remote_service.rs`: `C19A20BE44DD83A4D8079DCCF1A0C0521D78DA9E4459F8CE5783A475938E2BC1`
- `crates/explorer-ui/src/icons.rs`: `E95F76442990D3A61886770EC5D88FD6B03A3E73D8C5834C1B43B5BC974E3012`
- `crates/explorer-ui/src/lib.rs`: `EB60F6597C17F4EF1BEBD778894A7525DCC9F845184A29A8B746F5E8FBA8525D`
- `crates/explorer-ui/src/chrome.rs`: `871BF9335F11A543ABD778832962BCAB189481EC0353A06F52743A21B6F03E79`

## Gate conclusion

G-CLASSIFIER, G-REMOTE-METADATA, G-REMOTE-ICON, and the change-scoped G-INTEGRATION checks pass. The separate unfiltered UI audit exposed eight unrelated failures already present in other dirty-worktree contracts; their details are preserved in `evidence/artifacts/4.1/integration-gates.txt` and no user-owned edits were overwritten to mask them.
