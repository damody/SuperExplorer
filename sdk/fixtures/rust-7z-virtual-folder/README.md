# Rust 7z virtual folder

This pure-Rust example registers a virtual resource and mutation plan. Its safety core rejects absolute/traversal/NUL paths, case-insensitive normalized collisions, excessive depth/count/output/ratio, and stale or changed containers. Reads are bounded. Mutation writes same-volume staging, reopens/verifies, rechecks original identity, atomically replaces, and preserves a whole-container undo backup. Secrets are short-lived handles and never serialized or logged.

Use the standard offline test/validate/build/package commands with this directory as `PluginRoot`. Extend the backend through `VirtualEntry`, `validate_entries`, `bounded_read`, and `transactional_replace`; never extract by joining an unvalidated archive path.
