# Circuit-breaker lineage

Status: `not-applicable`.

The implemented application-owned popup has no process-global circuit breaker. Each unsupported
or failed presentation returns a typed per-session fallback to the unchanged `HMENU` and
`TrackPopupMenuEx`; it cannot suppress subsequent menus. The full context-menu suite passed slow,
hung, error, owner-draw, invalid/empty, and failure-recovery cases, followed by successful later
queries. The final headful built-in and replacement sessions also retained one healthy broker and
continued opening menus after every cancellation and command.

No retained run opened a circuit, so there is no failed circuit lineage to rerun. This conditional
task is closed as evidence-backed `not-applicable`, not treated as a synthetic pass.
