# Detailed folder-query error transport

Timestamp: `2026-08-25T00:29:32+08:00`

The fixed 48-byte folder response header now has a typed detailed-error status. For that status only, bytes 8–11 carry a validated UTF-8 payload length and the service appends at most 3 KiB. Successful aggregate offsets are unchanged. The current client still accepts legacy generic error statuses without waiting for a payload.

The LocalSystem service wraps its complete query failure with path, volume, reference, elapsed milliseconds, cache limit, stage, and underlying error before returning it over the named pipe. The SuperExplorer client keeps the cell terminal text as `Unavailable`, writes the received reason to stderr, and records the same request context through the process `error.log` sink.

Focused command:

`cargo test -p explorer-app --lib mft_query::tests -- --test-threads=1`

Outcome: PASS, 12 passed, 0 failed. Coverage includes bounded UTF-8, zero/overflow rejection, legacy generic decoding, partial typing, and an actual local named-pipe sequence that returns a detailed error followed by an exact aggregate.
