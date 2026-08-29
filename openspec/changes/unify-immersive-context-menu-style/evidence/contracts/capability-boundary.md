# G3 typed capability boundary

Recorded: 2026-08-30T00:19:25.3177801+08:00

- `immersive_menu.rs` owns closed resolver, capability, unsupported, fallback, phase, theme, and diagnostic types.
- The three private function pointers are constructed as one `ImmersiveMenuEntryPoints` value only by a verified resolver or controlled tests.
- Disabled probes return before calling the resolver and do not populate the process cache.
- Available and unsupported results are cached once.
- The diagnostic shape contains only closed enums and numeric build/DPI fields, so it cannot retain target or extension payloads.
- Production runtime resolver intentionally remains unsupported until G4 passes.

Validation: `cargo test -p explorer-shell-win immersive_menu --lib` — PASS, 4 passed.
Source SHA-256: `D1D376B92A858B7B24BF23EBD7FF971F4AA0758C433ABA8A4A638D1EF2950717`.
