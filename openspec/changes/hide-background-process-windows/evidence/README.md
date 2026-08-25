# Evidence Contract

`index.json` is the append-only evidence index for this change. Each record names the command or reviewed artifact, expected and actual result, exit status, content hashes, related gates, UTC timestamp, and task subchecks. A task is complete only when its subcheck is `passed`, evidence-backed `not-applicable`, or `superseded` with replacement lineage. Failed, blocked, stale, or unexecuted work remains incomplete.

The debug and release executables are build artifacts and are referenced by SHA-256 rather than committed. `summary.md` records the reproducible commands. `traceability.md` maps normative scenarios to implementation and evidence.
