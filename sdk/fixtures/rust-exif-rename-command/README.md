# Rust EXIF rename command

This public-SDK example statically links its in-process TIFF/EXIF reader into `plugin.dll`, reads bytes through the host stream contract, distinguishes pixel dimensions from density, expands documented tokens, sanitizes Windows basenames, rejects missing tags and case-insensitive collisions, then submits an identity-checked undoable rename plan. It uses no exiftool, external DLL, PATH, network, or private crate.

Use the standard offline test/validate/build/package commands with this directory as `PluginRoot`. Extend `parse_tiff` and `render_pattern` to add documented tags; preview must remain side-effect free.

Dependency changes require exact Cargo versions, a regenerated `Cargo.lock`,
and refreshed provenance/SBOM/license inventory. After this complete example
gate, run `rust-exif-rename-headful` and
`extension-command-interaction-headful` locally; CI is not an acceptance path.
