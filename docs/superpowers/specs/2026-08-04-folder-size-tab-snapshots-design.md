# Folder-size tab snapshots

## Problem

Folder-size presentation is globally owned but stores only one active context and one value map. Switching tabs therefore clears completed values and the submitted-work set. Returning to an unchanged tab schedules every visible folder again.

## Decision

Keep one global folder-size presentation owner, backed by bounded snapshots keyed by `(tab_id, generation)`. Each snapshot owns that generation's values. Switching tabs selects an existing snapshot without clearing values or submitted-work identities. F5, navigation, or another content refresh advances the tab generation and therefore selects a new empty snapshot that is measured independently.

Results are admitted only to their matching tab and generation. A stale result may populate its matching retained snapshot but never the active snapshot for another generation. Closing a tab removes its snapshots and submitted identities; a fixed capacity evicts least-recently-used inactive snapshots as a defensive bound.

## Verification

- Populate two tab snapshots, switch between them, and prove both retain exact values.
- Prove switching tabs does not clear submitted identities or enqueue duplicate work.
- Advance one tab generation and prove only that tab receives an empty snapshot and new work.
- Run folder-size unit tests and the headful UITEST.
