# Repeated Launch Opens a New Explorer Window

## Goal

When SuperExplorer is already running, executing SuperExplorer again opens a new
top-level explorer window in the existing application and navigates that window
to `C:\`. The first launch retains the current startup behavior, including
session restoration.

## User-visible behavior

- The first process owns the application services and all explorer windows.
- A later invocation sends an `OpenWindow` request to the resident process and
  exits after the request is accepted.
- Each accepted request creates one independent top-level explorer window whose
  initial and active location is `C:\`.
- Closing one explorer window does not close the others. The application exits
  after its final window closes.
- Explicit test and diagnostic startup modes remain independently launchable so
  automated fixtures are not redirected into an unrelated resident process.
- If the resident endpoint cannot be reached, the new invocation continues as
  a normal first process instead of preventing startup.

## Architecture

Add a small Windows-only launch-coordination component owned by
`explorer-app`. It uses a per-user named synchronization object to select the
resident process and a per-user named pipe for bounded launch requests. The
wire contract is versioned and currently supports only `OpenWindow` with the
fixed filesystem location `C:\`.

The listener performs no GPUI work. It validates the bounded request and sends
it through an in-process channel. The GPUI application periodically drains that
channel on its foreground executor. Window creation stays on the GPUI thread.

Main-window construction is extracted into a reusable application helper so the
startup window and later windows share services, shell integration, theme,
bookmarks, extensions, and window behavior. Session restoration and persisted
placement apply only to the initial window. Later windows start with a fresh
single tab at `C:\` and use normal initial window placement.

## Failure handling and security

- Endpoint names are scoped to the current interactive user.
- Requests have a strict size limit and reject unknown protocol versions or
  commands.
- A secondary invocation waits only for a bounded acknowledgment.
- Connection, validation, or acknowledgment failure falls back to ordinary
  startup and is recorded in diagnostics.
- Listener shutdown is explicit and joins its worker without blocking the UI.

## Testing

- Unit-test request encoding, decoding, size limits, and invalid messages.
- Unit-test launch-role selection and the fallback decision.
- Test that a relaunch request maps to exactly one fresh `C:\` window request.
- Preserve existing startup/session tests to prove the first window still
  restores normally.
- Add a Windows smoke test that launches the executable twice, observes two
  top-level windows owned by one resident process, and verifies the new window's
  address is `C:\`.

## Scope

This change does not add arbitrary command-line path launching, tab transfer,
or restoration of multiple persisted windows. Those can extend the versioned
launch protocol later without changing this contract.
