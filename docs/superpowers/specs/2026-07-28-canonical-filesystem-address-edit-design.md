# Canonical Filesystem Address Edit Design

## Problem

Navigation-pane entries such as Documents use Shell parsing names including `shell:Personal`.
Successful Shell resolution currently publishes that original descriptor unchanged, so clicking the
address bar exposes the parsing name instead of the actual redirected filesystem path. The text is
less useful for copying into Windows Explorer or another application.

## Scope

Every Shell-backed folder that resolves to a real filesystem path will expose that complete path
when the address bar enters edit mode. This includes Documents, Downloads, Desktop, Pictures, Music,
Videos, and other filesystem-backed known folders, including redirected and UNC locations.

Pure namespace locations without a filesystem path, such as Home, This PC, Recycle Bin, Network,
and Libraries, retain their valid Shell parsing names. Breadcrumb display labels remain friendly names.

## Design

The Windows Shell resolver will distinguish the descriptor used to perform the bind from the
canonical descriptor published in `LocationMetadata`. After resolving the Shell item/PIDL, it will
request `SIGDN_FILESYSPATH`. A non-empty result becomes `LocationDescriptor::FileSystem`; failure or
an empty result preserves the original `ParsingName`, `KnownFolder`, or `ShellNamespace` descriptor.

The already-bound Shell folder remains the enumeration source for the active request. Only the
committed navigation descriptor changes. The normal `LocationResolved` flow then commits the
canonical descriptor to history, and the existing `AddressBarState::for_entry` automatically uses
the complete filesystem string as its editable draft. Back/Forward, session persistence, copied
address text, and submitting that path therefore share one canonical representation.

Filesystem input descriptors remain unchanged instead of being reparsed or rewritten. Non-path
namespaces never receive an invented path. If Windows cannot provide a path, resolution still
succeeds with the original descriptor.

## Alternatives

- Store a separate address-edit string in history and session state. This preserves Shell identity
  but expands protocol and persistence state and can drift from the committed location.
- Derive edit text from the last breadcrumb segment. This is smaller but races asynchronous ancestry
  resolution and fails before the ancestry result arrives.

Canonicalizing at the Shell resolution boundary is preferred because that is where Windows knows
whether the item is truly filesystem-backed.

## Testing

- Model tests verify that a committed `FileSystem` descriptor produces a complete editable draft and
  that non-path parsing names remain available.
- Real Windows Shell tests resolve Documents, Downloads, Desktop, Pictures, Music, and Videos and
  compare published descriptors with their non-empty `SIGDN_FILESYSPATH` results.
- A headful/UIA regression opens a filesystem-backed known folder, clicks or focuses the address bar,
  and verifies that the selected editable value is an absolute drive or UNC path with no `shell:`
  prefix. The value must be accepted when submitted again.
- Existing namespace navigation tests continue to prove Home, This PC, Recycle Bin, Network, and
  Libraries remain navigable without fabricated filesystem paths.

## Acceptance Criteria

- Clicking the address bar in every filesystem-backed Shell folder shows its complete actual path.
- Redirected known folders show the redirected drive or UNC path reported by Windows.
- The selected text can be copied directly into Windows Explorer and navigated successfully.
- Non-filesystem namespace folders retain their Shell parsing names and behavior.
- No breadcrumb label, enumeration, history correlation, or session restore regression is introduced.
