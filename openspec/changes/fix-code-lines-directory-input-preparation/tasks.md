## 1. Reproduce and freeze the contract

- [x] 1.1 Add a focused regression proving a binary-heavy directory with supported source produces a dispatchable source-only snapshot.
- [x] 1.2 Add boundary coverage proving directory snapshots never exceed the single Host input stream limit and empty/oversized source sets are unsupported.
- [x] 1.3 Add batch-preparation coverage proving one invalid row cannot fail other valid rows.

## 2. Repair directory snapshot preparation

- [x] 2.1 Filter recursive directory entries with the locked tokei path classifier before reading their contents.
- [x] 2.2 Apply `MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1` to the complete framed directory snapshot.
- [x] 2.3 Refactor batch input preparation to publish per-row errors while dispatching all successfully prepared rows.
- [x] 2.4 Replace the Lua provider's partial extension and line classifier with the workspace-locked tokei parser.

## 3. Verification

- [x] 3.1 Run formatting and focused `explorer-app`, Rust provider, Lua provider, and Host runtime tests with locked/offline dependencies.
- [x] 3.2 Run strict OpenSpec validation and scan the change artifacts for placeholders or contradictions.
- [x] 3.3 Diagnose every direct child of `D:\code\file_explorer` with the production snapshot builder and record that no accepted snapshot can fail the Host stream size contract.
- [x] 3.4 Build through the standard application path and verify the repaired Code Lines results no longer show `Code lines input could not be prepared` for the target folder.
- [x] 3.5 Load both installed providers in one clean-profile run and verify paired Code lines and Main code lines results for representative `D:\code\file_explorer` folders.
