# Forced popup failures

Command:

`cargo test -p explorer-shell-win forced_resolver_apply_message_and_cleanup_failures_are_local_to_one_session -- --test-threads=1 --nocapture`

Result: passed.

The controlled test covers an unsupported owner-draw resolver outcome plus injected apply,
message-loop, and cleanup failures. Apply failures return `WindowCreationFailed` before a
custom HWND is created. Message-loop and cleanup failures destroy the visible popup before
returning their structured failure. Each injected failure is thread-local and automatically
reset; an immediately following real popup opened and cancelled successfully. Production
integration maps every returned error to the unchanged `TrackPopupMenuEx` fallback, and there
is no process-global circuit that can suppress later menus.
