# Public owner-draw capability

The implementation uses only documented Win32 APIs projected by the `windows` crate: HMENU item enumeration/mutation, UxTheme menu parts, GDI bitmap drawing, per-window DPI, theme activity, and high-contrast system parameters. It does not load ExplorerPatcher, private Windows immersive-menu exports, signatures, binaries, or assets.

The capability is process-cached. Unsupported architecture, inactive theme service, high contrast, invalid menu/theme handles, incompatible pre-existing owner-draw rows, enumeration failure, mutation failure, or an opened cleanup circuit return typed unsupported/fallback values. Original HMENU identity is retained in session-owned memory and restored before invocation or cancellation replay.
