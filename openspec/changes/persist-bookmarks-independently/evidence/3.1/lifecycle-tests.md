# Lifecycle evidence

Commands:

- `cargo test -p explorer-app session_lifecycle --lib`: exit 0; 6 passed, 0 failed.
- `cargo test -p explorer-app session_store --lib`: exit 0; focused session-store tests passed.

The worker writes the independent bookmark snapshot before the downgrade-compatible session envelope. A bookmark failure retains the latest pending snapshot and exercises existing health counters/retry. Session and AllRoadmapState reset requests call only `SessionStore::reset`; both mock call-count tests and real sibling-file byte assertions prove bookmark storage is unchanged.
