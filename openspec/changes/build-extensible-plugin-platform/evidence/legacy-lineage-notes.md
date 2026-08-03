# Legacy lineage audit

The old task file was inspected at commit `65d780e767ac56a4a7fcf3e2e70eddc9f3b198e2`. Git history shows the first checkbox transitions for old tasks 1.1–4.2; old 4.3–4.8 remain unchecked in that history. The JSON map records those boundaries and maps legacy labels to the current machine-readable L3 IDs.

This is traceability only. A checked historical box and a source diff do not establish a passing test, immutable artifact, reviewer sign-off, or release approval. Entries with no commit are explicitly `unverified` and must be re-executed.

The legacy 5.1 FINAL GO backfill is a candidate package, not a GO decision. It enumerates the records required to cover the expanded dynamic-column contract and calls out 5.2/5.4 reopen risks.

## Reproduction commands

```powershell
git show 65d780e767ac56a4a7fcf3e2e70eddc9f3b198e2:openspec/changes/build-extensible-plugin-platform/tasks.md
git log --all --format='%H %ad %s' --date=iso -- openspec/changes/build-extensible-plugin-platform/tasks.md
Get-Content openspec/changes/build-extensible-plugin-platform/evidence/legacy-lineage-map.json | ConvertFrom-Json
Get-Content openspec/changes/build-extensible-plugin-platform/evidence/legacy-5.1-final-go-backfill.json | ConvertFrom-Json
```

