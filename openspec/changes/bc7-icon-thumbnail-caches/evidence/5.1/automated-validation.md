# G-AUTOMATED status

Passed on 2026-08-14:

- locked offline Release build for all `explorer-app` binaries;
- focused Shell BC7 codec, disk container, warm icon disk hit, and thumbnail cache-mode benchmark tests;
- the 15-test disk-cache corpus, including malformed/truncated headers, trailing bytes, excessive dimensions and file length, checksum/layout failures, root containment, a no-privilege NTFS junction/reparse fixture, read/write races, and bounded quota cleanup;
- explorer-model cache/session tests and explorer-ui Folder Options tests;
- GPUI compressed-raster contract test and gpui_windows BC7 test-target compilation;
- `cargo fmt --all -- --check`;
- `openspec validate bc7-icon-thumbnail-caches --strict`.

The full `explorer-shell-win` library suite did not complete within a 124-second bounded run and is not claimed as passing. Repository warnings also prevent claiming a clean lint/static gate. Hardware device-recovery, full workspace, and detailed-task-validator coverage remain open.

Release/source hashes are in `../1.1/baseline.md`; codec source `96D2EF8A26B93BDE90B1B770E1E706F2EBDFA291EF063FF2411CC22A44967F79`, container source `BC39084B0E9632EB3DB0BBAA53A0B3F0501BF3785BB95DE3C595D634004B4395`.
