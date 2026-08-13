# Real-folder verification

Verified on 2026-08-14 against `D:\code\file_explorer`.

## Production snapshot contract

`SUPEREXPLORER_CODE_LINES_REAL_FOLDER=D:\code\file_explorer cargo test -p explorer-app code_lines_real_folder_snapshots_respect_stream_contract --lib --locked --offline -- --nocapture` passed.

- Dispatchable: `.claude`, `.vs`, `FluentExplorer`, `FluentExplorer.UITests`, `appmover`, `docs`, `explorer-core`.
- Unsupported without preparation failure: `.git`, `installer`.
- Every dispatchable snapshot was accepted by the single Host input stream contract.

## UI results

The Rust Main code lines and Lua Code lines headful runs completed with clean shutdown and no `Code lines input could not be prepared` cell.

The corrected all-language Code lines values included:

- `.claude`: 69
- `appmover`: 1,989 (dominant Python: 1,925)
- `docs`: 51
- `explorer-core`: 7,080 (dominant Rust: 7,032)
- `FluentExplorer`: 82,469 (dominant XML: 55,556)
- `FluentExplorer.UITests`: 7,585 (dominant C#: 4,963)

The Lua provider now accepts the same tokei-recognized formats as the Host, including JSON, Markdown, C#, and XML. Headful evidence is stored in the ignored build output at `target\code-lines-file-explorer-correct-lua-directories`.

## Build and installation

`build_test_install.bat --no-launch` produced `dist\SuperExplorer-Setup-1.2026.8.14-x64.exe`. Silent installation completed with exit code 0. The SHA-256 of the installed `C:\Program Files\SuperExplorer\plugins\lua_tokei_code_lines_column.dll` matched the release artifact (`93A55780302C391301A1DC41E04816A0B4702EBF3E3A4E39FF04F91759D774D1`).
