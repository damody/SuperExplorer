# Context-menu diagnostic review

The scoped tracing review covers `context_menu.rs` and `immersive_popup.rs`.

- Per-open presentation-path, popup-open, popup-visible, selected canonical-verb, and low-level
  gesture observations are debug-only.
- Warning records are exceptional terminal/fallback or Properties-centering conditions, not
  per-frame or per-row output.
- Records contain only request/tab/generation identifiers, DPI, booleans, command offsets,
  canonical verbs, numeric HWND/coordinates, typed fallback reasons, and typed gesture actions.
- No filesystem path, menu label, username, PIDL bytes, clipboard content, or raw extension
  payload is emitted.
- The ten-cycle headful replacement report observed one broker and a maximum responsiveness probe
  of 11 ms; the 1,000-cycle popup test retained bounded handles/private bytes.

Result: diagnostic volume and payload privacy pass for the implemented Local popup path.
