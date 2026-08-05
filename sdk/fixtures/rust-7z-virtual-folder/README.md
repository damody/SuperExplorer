# Rust 7z virtual folder

This pure-Rust example registers a virtual resource and mutation plan. Its safety core rejects absolute/traversal/NUL paths, case-insensitive normalized collisions, excessive depth/count/output/ratio, and stale or changed containers. Reads are bounded. Mutation writes same-volume staging, reopens/verifies, rechecks original identity, atomically replaces, and preserves a whole-container undo backup. Secrets are short-lived handles and never serialized or logged.

From the repository root, run the complete local package gate:

```powershell
cargo test --manifest-path sdk/fixtures/rust-7z-virtual-folder/Cargo.toml --locked --offline
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/validate-plugin.ps1 -PluginRoot sdk/fixtures/rust-7z-virtual-folder
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/build-plugin.ps1 -PluginRoot sdk/fixtures/rust-7z-virtual-folder
powershell -NoProfile -ExecutionPolicy Bypass -File sdk/scripts/package-plugin.ps1 -PluginRoot sdk/fixtures/rust-7z-virtual-folder
```

The production route supports nested navigation, preview/bounded reads, copy/drag-out, add-file, new-folder, delete, rename, same-container move, encrypted archives, whole-container undo, and Explorer-style password prompting. Passwords are never persisted. Mutation writes same-volume quota-bounded staging, reopens and decodes every entry, rechecks the original container, and only then atomically replaces it.

Extend the backend through `VirtualEntry`, `validate_entries`, `bounded_read`, and `transactional_replace`; never extract by joining an unvalidated archive path.

Dependency changes require exact Cargo versions, a regenerated `Cargo.lock`,
and refreshed provenance/SBOM/license inventory. Only after this complete example
gate, run `rust-7z-virtual-folder-headful` locally through the repository UITEST runner.
