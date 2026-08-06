## Context

`format_file_size` currently uses one decimal only below 10 units and rounds larger values to integers. Folder size already formats scaled values with one decimal, causing visibly inconsistent labels for the same bytes.

## Goals / Non-Goals

**Goals:**

- Display exactly one decimal for every nonzero KB/MB/GB/TB built-in Size value.
- Keep zero and minimum-KB semantics explicit and tested.
- Verify the installed application visually.

**Non-Goals:**

- Changing byte values, binary-unit thresholds, sorting, cache policy, MFT behavior, or Folder size plugin formatting.

## Decisions

Replace the conditional precision branch in the shared built-in formatter with `format!("{value:.1}")` for every nonzero value. Keep the zero fast path as `0 KB` and retain the clamp that maps a nonzero sub-kilobyte file to `1.0 KB`.

Alternative: preserve integer output for exact units. Rejected because the approved requirement calls for exactly one decimal and consistent column precision.

## Risks / Trade-offs

- [Labels become slightly wider] → Existing Size column resizing and clipping behavior handles the additional two characters.
- [Other built-in surfaces reuse the formatter] → This is intentional shared presentation consistency; unit tests lock the contract.

## Migration Plan

Update the formatter/tests, rebuild and install, then capture D:\ Details evidence. Rollback is the single formatting-function change.

## Open Questions

None.
