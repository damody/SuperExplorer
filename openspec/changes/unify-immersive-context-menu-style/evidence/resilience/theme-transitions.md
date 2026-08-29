# Theme and accessibility transitions

The same test process opened and cancelled three real application-owned popup sessions in
light, dark, then light order. Every call rematerialized rows and received the current `dark`
projection; no process or owner HWND restart occurred. The separate policy test exercised
enabled/disabled and high-contrast true/false transitions and proved high contrast bypasses
the custom presenter in favor of the native path.

Commands:

- `cargo test -p explorer-shell-win consecutive_light_dark_light_sessions_use_fresh_theme_without_restart -- --test-threads=1 --nocapture`
- `cargo test -p explorer-shell-win owned_popup_policy_falls_back_for_disabled_or_high_contrast_sessions -- --test-threads=1 --nocapture`

Both passed.
