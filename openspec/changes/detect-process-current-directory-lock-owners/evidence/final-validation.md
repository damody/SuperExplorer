# G5 validation report

Passed:

- repository `cargo fmt --all -- --check`;
- focused locked/offline Shell, App, Extension Host, Extension API, and example-plugin tests;
- locked/offline workspace metadata resolution;
- `openspec validate detect-process-current-directory-lock-owners --strict`;
- debug application and lock-owner plugin prerequisites existed for headful execution.
- `cargo build -p explorer-app --locked --offline` completed successfully on 2026-08-14;
- `cargo test -p explorer-ui --lib --locked --offline locked_delete -- --nocapture --test-threads=1` passed all 5 affected UI lifecycle tests;
- the `rust-lock-owner-headful` manifest entry requires native nested/parent, WOW64 nested/parent, current-directory-cleared, and file-lock-cleared artifacts.

Warnings from unrelated dirty-tree work remain and are not relabelled as a clean lint gate. The SDK inventory mismatch, headful address-navigation failure, injected native seam gaps, and independent review remain explicit blockers. The change is implemented and strict-valid but not archive-ready.
