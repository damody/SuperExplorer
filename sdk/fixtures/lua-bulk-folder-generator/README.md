# Lua bulk-folder generator

This restricted-Lua example declares a button, host form, and typed create-directory plan. It generates 1–100,000 names from parent/prefix/start/padding/suffix, requires a second confirmation above 1,000, reports cancellation as partial when needed, and undo removes only still-empty directories created by the plan.

Run the same four `cargo test --locked --offline`, `validate-plugin.ps1`, `build-plugin.ps1`, and `package-plugin.ps1` commands shown by the other examples with this directory as `PluginRoot`. Modify `generate_names` and `lua/main.lua`; keep filesystem mutation in the host operation-plan executor.
