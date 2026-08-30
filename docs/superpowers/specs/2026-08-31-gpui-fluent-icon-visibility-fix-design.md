# GPUI Fluent Remote Icon Visibility Fix

## Problem

The screenshot proves that official Fluent Color SVGs render transparent in GPUI while official filled SVGs such as PDF and Archive remain visible. Classification is working; the failure is the renderer's unsupported gradient/paint-server handling.

## Decision

Use official Fluent 20px filled SVGs for all remote file categories. Convert their default SVG fill to `currentColor` at asset load and apply the existing stable category tint. Preserve distinct official silhouettes, filename classification, Type labels, remote-only selection, local Shell behavior, and the 24-category taxonomy.

This is preferred over rasterizing Color SVGs because a single vector asset remains crisp at every supported view size. It is preferred over rewriting gradients because that would be fragile and still depend on unsupported SVG paint features.

## Assets and rendering

- Pin all replacements to `@fluentui/svg-icons@1.1.339`.
- Replace every Color payload with its corresponding official 20px filled payload.
- Keep the existing local asset names and upstream notice, updating upstream paths and SHA-256 hashes.
- Treat every remote file asset as monochrome and tint it through GPUI `currentColor`.
- Reject gradients, paint-server URLs, external references, scripts, images, filters, masks, and `foreignObject` in this compatibility subset.

## Verification

- Assert all 24 mapped payloads are distinct, contain visible path geometry, resolve offline, and become `currentColor` after loading.
- Assert no mapped payload contains gradients or `url(#...)` paint.
- Reconcile all 24 hashes against the notice.
- Re-run remote-file model/app/UI tests, changed-crate compilation, formatting, strict OpenSpec validation, and evidence reconciliation.
- The screenshot failure is closed only when every non-folder row category has a non-empty compatible SVG payload; PDF and archive behavior must remain unchanged.

## Scope

No classification, Type wording, remote provider, transfer, thumbnail, local icon, row geometry, or dependency behavior changes.
