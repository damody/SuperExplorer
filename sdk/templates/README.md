# Independent plugin templates

Both templates create a standalone Cargo workspace. A generated project keeps
production code in `src/`, localized resources in `locales/`, deterministic
fixtures in `fixtures/`, screenshots in `screenshots/`, and includes English
and Traditional Chinese READMEs, `LICENSE`, `NOTICE`, `SBOM.json`,
`provenance.json`, `plugin-project.json`, and a locked dependency graph.

Rust and Lua-backed plugins share the same public Rust composition root. Lua
business logic belongs in `lua/`; it is registered by `src/lib.rs` through the
public SDK and never by a private product crate.
