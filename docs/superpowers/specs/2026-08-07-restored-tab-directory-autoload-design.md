# Restored Tab Directory Auto-load Design

## Problem

Session restore reconstructs every saved tab with a valid location but without transient directory contents. The active restored tab is submitted to the directory service during root construction. Restored background tabs remain in `DirectoryState::Idle`, and activating one only changes the active tab identity. Because activation does not submit that tab's location, the file view displays `Directory service is not connected` indefinitely even though the application-owned directory service is healthy.

## Decision

Use demand-driven restoration, matching Windows File Explorer: load the active tab immediately at startup and load a restored background tab the first time it becomes active. Do not eagerly enumerate every restored tab because simultaneous directory, icon, thumbnail, and extension-column work would create an avoidable startup burst.

The tab activation path will distinguish an unloaded restored tab from a tab that is already loading or has a visible snapshot. After a successful activation, it will create and submit the normal correlated navigation command only when the new active tab is idle. The command uses the existing request generation, cancellation, stale-result rejection, and directory service boundary; no second loading mechanism is introduced.

Keyboard tab cycling, pointer tab activation, and activation caused by closing the current tab must share the same post-activation helper. A new tab continues to use its existing pending load command. Re-selecting an already active, loading, ready, or failed tab must not create duplicate work. F5 remains the explicit retry for a terminal directory error.

## State and Error Handling

- `Idle` means the restored tab has not yet submitted its location and is eligible for one automatic load.
- `Loading`, `Ready`, and `Error` are not automatically resubmitted by activation.
- A command-admission failure is converted through the existing correlated failure path, so the view shows a truthful retryable error rather than a permanent disconnected placeholder.
- Switching away cancels no valid background enumeration solely because of this change; the existing tab-scoped request and stale-generation rules remain authoritative.
- Invalid or missing restored locations continue to use the existing session resolver and fallback location policy before the UI is created.

## Testing

### Unit coverage

- Restore at least two tabs and prove the active tab is loaded during root construction.
- Activate an idle restored background tab and prove exactly one normal navigation command is submitted.
- Prove re-activation while loading and after completion does not submit a duplicate command.
- Prove pointer activation, next/previous cycling, and active-tab close all use the same idle-tab load policy.

### UTIT coverage

Extend session restore headful coverage with a real two-process restart:

1. Open at least two filesystem tabs and persist the session.
2. Close and restart SuperExplorer.
3. Verify the restored active tab automatically displays its directory contents.
4. Activate each restored background tab through UI Automation and verify its contents load without user refresh.
5. Assert `Directory service is not connected` never remains visible after activation.
6. Capture the restored active tab, background-tab activation, and final report as evidence.

## Rejected Alternatives

- Eagerly load all restored tabs: it hides the symptom but creates startup I/O and extension-work spikes proportional to the saved tab count.
- Change the disconnected text to `Loading`: presentation-only and leaves the tab permanently empty.
- Poll idle tabs from the frame pump: adds repeated checks and risks duplicate submissions; activation is the precise lifecycle boundary.

## Out of Scope

- Persisting directory snapshots or thumbnails across sessions.
- Changing the session file schema.
- Automatically retrying genuine access, media, network, or provider errors.
- Preloading background tabs that the user never activates.
