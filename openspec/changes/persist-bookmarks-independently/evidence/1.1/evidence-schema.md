# Evidence index schema

`evidence/index.jsonl` is append-only. Each line is one JSON object with:

- `task_id`: permanent L3 task ID;
- `artifact`: repository-relative evidence path or immutable command label;
- `command`: exact command or manual procedure;
- `expected` and `actual`: acceptance result;
- `exit_status`: numeric process status or `manual-review`;
- `sha256`: lowercase content hash for file evidence, or the git revision for an immutable source baseline;
- `gate`: owning gate ID;
- `timestamp`: ISO-8601 timestamp with timezone;
- `disposition`: `passed`, `not-applicable`, or `superseded`.

Failed, blocked, stale, or unexecuted work is never recorded as complete.
