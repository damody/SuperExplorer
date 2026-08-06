## Context

The file view currently schedules Shell icon work and content-thumbnail work from the same remaining-capacity value. Pending entries are retained primarily by visible item identity, so requests made for an obsolete icon size or display context can consume the active view's budget. Thumbnail completion also retains a presentation key captured at submission time, which makes rapid mode, DPI, or folder changes vulnerable to stale visual replacement.

The render path is virtualized: only realized rows are submitted. The solution must remain bounded, preserve a useful Shell fallback while slower thumbnail extraction runs, and avoid clearing compatible completed cache entries when the user revisits a folder or view size.

## Goals / Non-Goals

**Goals:**

- Make the five requested views follow Windows Explorer's visual hierarchy.
- Prevent obsolete pending work and late results from starving or overwriting the active view.
- Keep Shell icon and thumbnail work independently bounded.
- Reuse compatible completed cache entries.
- Verify behavior in both deterministic Rust tests and a real Windows UTIT session.

**Non-Goals:**

- Rewriting the Shell STA worker or thumbnail decoder.
- Changing navigation-pane icons, extension ABI, or the meaning of the global always-show-icons preference.
- Adding new image/video codecs.

## Decisions

### Use a current visual-demand signature

Each visible request is compared against a signature containing tab identity, folder generation, DPI/theme context, view mode and actual icon size, thumbnail mode and size, association generation, and overlay generation. Pending requests that cannot satisfy the current signature do not count against its budget. Results are admitted to presentation state only when they still match current demand.

This is preferred over clearing all pending and completed caches on every transition because cache clearing creates visible churn and prevents fast return navigation.

### Separate Shell icon and thumbnail scheduling capacity

Shell icon requests and thumbnail requests receive independent bounded visible-item budgets. A slow or obsolete Shell request therefore cannot prevent an eligible image/video item from entering the thumbnail pipeline, and thumbnail load cannot suppress the immediate Shell fallback.

This is preferred over simply raising the shared cap because a larger shared cap still allows one work class to starve the other and increases burst load.

### Preserve fallback until a matching thumbnail is ready

For extra-large, large, and medium modes, the view first presents the correct current-size Shell icon. A successful matching image/video thumbnail replaces it. Failure, timeout, or stale completion leaves the Shell icon in place. Small-icon and tile modes request Shell icons only.

The policy targets are 256 px for extra-large, 96 px for large, and 64 px for medium, but the requested raster is raised to the actual presentation size when zoom or DPI requires more source pixels. Tile mode uses its 40 logical-pixel Shell icon; small-icon mode uses its configured Shell size.

### Bound prefetch by the configured texture budget

The presentation cache defaults to 128 MiB and Folder Options exposes 64/128/256/512 MiB and 1 GiB presets. Pre-layout and view-switch priming may consume at most half of that budget and never expands a known realized viewport to a fixed 16-item range. This prevents maximum-size icons from evicting one another every frame.

Thumbnail admission concurrency is two, matching the Windows Shell thumbnail worker capacity. Excess work remains queued in the UI scheduler instead of becoming a terminal availability failure.

### Keep completed caches keyed by compatibility

Completed Shell and thumbnail cache entries remain reusable when their source identity, size, display context, association, and overlay inputs remain compatible. Folder/view transitions invalidate presentation admission, not unrelated cache contents.

### Prefer the largest compatible Shell folder texture over the generic fallback

Folder presentation resolves in this order: exact current-size real-item Shell icon, largest compatible same-item Shell icon, largest compatible shared Shell folder icon, then the fixed generic yellow fallback while no Shell pixels exist. Compatible means the same item or folder base class plus current DPI, theme, association generation, and overlay generation. A lower-resolution Shell texture is enlarged with the existing aspect-preserving image host.

An exact-size shared-base failure is a missing-size result rather than permanent failure of the folder class. The visible real folder remains eligible for one bounded item request, allowing a valid smaller Shell result, custom icon, or overlay to recover presentation without duplicate pending work.

### Verify policy and integration separately

Rust tests exercise request classification, capacity isolation, stale-result rejection, fallback preservation, and cache reuse. Headful UTIT covers actual Windows Shell integration, rapid view switching, and newly realized items after scrolling.

## Risks / Trade-offs

- [Windows may not expose a 512 px folder bitmap] - Select and enlarge the largest compatible Shell bitmap; never treat the fixed yellow glyph as a successful folder icon.

- [More request identity fields increase bookkeeping complexity] → Centralize signature construction and equality checks instead of duplicating predicates.
- [Independent budgets can increase concurrent work] → Keep both caps bounded and submit realized items only.
- [Some codecs cannot produce a thumbnail] → Preserve the correct Shell icon and treat thumbnail failure as a stable fallback, not a blank result.
- [Headful visual checks can be timing-sensitive] → Wait on explicit readiness conditions and retain screenshots/logs as evidence.

## Migration Plan

The persisted view-settings payload gains a serde-defaulted cache-budget field, so older sessions restore to 128 MiB without a schema migration. Land the model policy, UI scheduling/admission changes, and tests together. Rollback is a source revert; existing caches are process-local and require no cleanup.

## Open Questions

None. The requested behavior follows Windows Explorer semantics defined above.
