# Large Directory Performance Baseline

Date: 2026-07-28
Build baseline: `f70a034` plus the pre-existing uncommitted post-parity worktree

## Privacy

This evidence intentionally records no directory path, filename, or item identity. The real input is referred to only as the reference network directory.

## Reference Network Directory

- Direct children: 15,667
- Files: 15,665
- Directories: 2
- Provider enumeration observed by a read-only PowerShell measurement: 5,922 ms

## Pre-Change Render Structure

Static inspection established these baseline invariants before implementation:

- A normal wheel event requested an application-wide refresh.
- Each file-view render cloned and filtered the complete snapshot.
- Each render sorted the complete visible collection with lowercase allocation inside comparisons.
- Each render realized all 15,667 rows/cells rather than a viewport range.
- The application deep-cloned the directory-bearing view state into multiple child components.
- The existing 100,000-entry test verified only the first 64 icon requests and performed no row realization or scrolling.

Because file-view timing instrumentation did not exist in the pre-change binary, no fabricated p50/p95 frame values are reported. The structural baseline and provider enumeration measurement are the comparison point. The new Release benchmark records p50, p95, and maximum virtual-scroll and input latency after instrumentation is introduced.

## Required Post-Change Evidence

- No more than 250 realized rows/cells in a standard viewport with 100,000 entries.
- Zero complete-snapshot clones and zero presentation rebuilds during steady-state scroll.
- Local scroll-frame p95 at or below 16.7 ms.
- Network/provider scroll-frame p95 at or below 33 ms, with no render stall above 100 ms.
- Pointer and keyboard latency p95 at or below 50 ms after the first batch is visible.

## Provider Batching Invariant

Shell enumeration continues to cap each provider batch at 64 items and 256 KiB. These caps are
cancellation and apartment-ownership safety boundaries, not render boundaries. Correlated batches
drained in one UI transaction are merged before one model revision/projection update, and the first
accepted transaction is immediately realizable without waiting for terminal enumeration.

## Post-Change Release Geometry Evidence

The deterministic 100,000-entry Release benchmark completed 10,000 virtual-scroll samples on the
development machine with p50 0.000 ms, p95 0.0001 ms, maximum 0.0061 ms, and input p95 0.0001 ms.
These numbers isolate range calculation and input bookkeeping; headful GPUI paint/provider timing
remains a separate rollout gate and must not be inferred from this microbenchmark.

## Resource and Invariant Evidence

- Actual Release GPUI rendering at 1,120×720 realized 51 Details rows with 9.2923 ms p95 and
  10.306 ms maximum scroll frames; Medium Icons realized 147 cells with 8.2028 ms p95 and
  8.7881 ms maximum across 40 deterministic offsets.
- While the first 64-item batch remained in loading state, 100 alternating pointer/keyboard
  selection actions measured 0.0309 ms p95 and 0.0725 ms maximum callback latency.
- Standard 100,000-entry test viewports realize at most 250 rows/cells in every view family.
- Steady-state scroll reuses the revision/sort/hidden-items projection and records zero complete
  directory-snapshot clones.
- Shared bases are capped at 256 entries and 32 MiB; composed visible-item results are capped at
  512 entries and 64 MiB. Both caches share renderer textures through `Arc<RenderImage>`.
- Thumbnail memory is capped at 2,048 entries and 128 MiB. Scheduling is capped at 512 queued
  requests, four concurrent decodes, and 64 MiB decoded in flight; leaving the virtual range
  removes thumbnail consumers.
- Enumeration speed remains provider-dependent. The reference network provider took 5,922 ms to
  enumerate 15,667 children, but provider latency no longer determines realized row count or sort
  work during scrolling.
- Real TortoiseGit Shell validation passed on 2026-07-28: clean, modified/added, and unversioned
  provider states produced distinct composed icon hashes where expected. The normal per-item loader
  remains the visible override path; shared-base loading uses `SHGFI_USEFILEATTRIBUTES` without
  overlays, so provider badges cannot contaminate extension-wide bases.
- Real `desktop.ini` validation passed with a temporary read-only folder whose Explorer executable
  icon differed from the generic shared-folder base after a Shell update notification.
- The configured OneDrive root completed the same real composed-result path. Its current Shell
  state classified as a negative override (the returned bitmap matched the generic base), which is
  cached only for the current overlay epoch; a later provider badge invalidates and replaces only
  that item's shared base.
- Real Shell identity validation loaded owned pixels for the current executable and a generated
  `.lnk`, and two association epochs forced two independent live association-base loads. The real
  folder/drive/archive/namespace matrix and thumbnail provider matrix also passed; unsupported
  document/media providers remained typed fallbacks rather than fabricated thumbnails.
- Realized file-view nodes publish the complete collection size and their one-based presentation
  position. UI Automation focus and invocation first route through stable-ordinal selection, which
  scrolls overscanned offscreen targets into view before the requested interaction.
