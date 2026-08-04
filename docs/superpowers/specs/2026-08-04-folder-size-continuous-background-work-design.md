# Folder-size continuous background work

## Problem

The folder-size runtime removes a request from its pending queue when the worker starts it. While that request is running, the UI continues submitting every unresolved visible folder on each render. Because the runtime remembers queued work but not active work, the active folder can be queued repeatedly and scanned again from its root.

## Decision

The application-owned runtime will own one deduplicated work set per request context. A work identity is the context, item ID, and filesystem path. The runtime tracks both queued and in-flight identities and accepts an identity only when it is in neither set.

Taking work from the queue moves its identity to in-flight. A terminal result removes it from in-flight. Repeated UI submissions during the scan are ignored. A foreground timing hint never cancels or restarts the scan.

When navigation or refresh changes the request context, the runtime advances its publication epoch and rejects results for the old UI generation. An already-running scan may finish and populate the plugin-owned exact cache, but its stale result is not published into the current UI. New-context work is queued independently.

## Alternatives considered

- UI-side submitted-item tracking was rejected because recreating UI state could re-submit work and scheduling ownership belongs to the runtime port.
- Resumable traversal state inside the plugin was rejected because the existing synchronous ABI can finish correctly on a background worker; expanding the ABI is unnecessary for this defect.

## Failure handling

Partial or error outcomes are terminal for that work identity and remain typed, never entering the exact cache. Worker shutdown stops accepting work. Queue state is bounded by the visible request set and contains no duplicate identity.

## Verification

- Repeatedly submit the same request while it is active and prove that measurement is invoked once.
- Change generation while a long measurement is active and prove that it reaches terminal completion/cache while its old-generation UI result is rejected.
- Run the application and extension-host tests, rebuild/package `rust-folder-size-visual-column` locked and offline, and rerun its headful UITEST.
