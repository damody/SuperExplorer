# Normal Workspace Warning Cleanup Design

## Goal

Eliminate every warning emitted by `cargo check --workspace --locked --offline` without changing runtime behavior. All-target, example-only, and Clippy-only diagnostics are outside this change.

## Scope

The current normal workspace build emits 17 diagnostics: unnecessary qualifications, one unnecessary `mut`, one unused import, and three unused bindings. The cleanup will modify only the reported sites and any directly resulting normal-build warning.

## Rules

- Replace redundant qualified paths with names already imported in the same module.
- Remove `mut` only when the binding is never reassigned or mutably borrowed.
- Remove imports only when no normal-target use remains.
- Preserve RAII lifetime behavior: unused resource guards are renamed with a leading underscore, not deleted or shortened.
- Replace an unused pattern binding with `_` only when the value has no logging, matching, or cleanup responsibility.
- Do not add lint suppression, change public interfaces, refactor surrounding code, or run broad `cargo fix` rewrites.

## Verification

1. Run `cargo fmt --all --check`.
2. Run `cargo check --workspace --locked --offline` and require zero warnings and zero errors.
3. Confirm `dead_code` and `unsafe_code` remain at zero.
4. Run focused tests only if a changed site has behavior not fully covered by compilation; RAII guard edits receive the relevant MFT focus tests.

