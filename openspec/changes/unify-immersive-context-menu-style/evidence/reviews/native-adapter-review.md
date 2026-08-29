# Native popup host review

Review scope: B-003 implementation in `immersive_popup.rs`, its `context_menu.rs` integration, broker protocol propagation, and retained fallback. Reviewer: primary integrator, 2026-08-30.

## Provenance and architecture

- No ExplorerPatcher source block, binary, asset, signature table, private ABI declaration, `twinui.pcshell.dll` load, injection, IAT hook, or process-global patch exists in the scoped diff.
- The implementation uses documented HMENU, Win32 window/message, GDI, DPI, monitor, high-contrast, and Shell COM APIs.
- HMENU and IContextMenu remain authoritative. The custom host reads `MENUITEMINFOW` presentation fields and never requests or writes `dwItemData`; it returns the original command ID.

## Pointer and resource audit

- The `PopupState` pointer stored in `GWLP_USERDATA` points to a pinned heap allocation that outlives `DestroyWindow`; no window message can observe it after the enclosing call drops the box.
- The caller owns HMENU and owner HWND for the modal call. Child HMENUs are non-owning handles read from the parent.
- The host owns its created font, popup HWND, mouse capture, and shadow HWNDs. Creation failures unwind through `Drop`; normal exit releases capture and destroys the popup before invocation; `Drop` destroys shadows and owned font once.
- Bitmap handles remain Shell-owned. A compatible source DC selects/restores the handle and is deleted; the bitmap itself is never deleted by the host.
- All popup work remains on the existing disposable Shell STA worker.

## Fallback and privacy audit

- Disabled setting, high contrast, invalid/empty menu, unsupported owner-draw, enumeration failure, class failure, and window creation failure retain `TrackPopupMenuEx` fallback.
- There is no process cache or circuit state capable of suppressing all later menus.
- Diagnostics contain DPI, point, row count, geometry, presentation path, and typed reason only. They do not contain paths, labels, user names, PIDL bytes, canonical verbs, or raw extension data.

## Findings

- P0: none.
- P1: none.
- P2-01: pixel values for dark mode still require the full indexed visual matrix. Disposition: dark mode is functional and token-tested; visual acceptance remains gated by G11/G13 and is not claimed by this review.
- P2-02: the custom Local host exposes complete keyboard/mnemonic behavior and high-contrast native fallback, but native per-row screen-reader semantics are not claimed beyond the current requirements. Disposition: retain documented keyboard/high-contrast contract; remote GPUI rows keep explicit roles and labels.

G10 result: passed for architecture, provenance, resource ownership, fallback, and diagnostic privacy. Visual acceptance remains independently gated.
