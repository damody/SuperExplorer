## Context

Navigation-pane known folders are submitted as Shell parsing names. `resolve_location` binds the
correct Shell object but publishes the input descriptor unchanged in `LocationMetadata`. The model
commits that descriptor to history, and `AddressBarState` renders it verbatim as the editable draft.

Windows can report `SIGDN_FILESYSPATH` for filesystem-backed Shell items, including redirected known
folders. Pure namespaces return no filesystem path and still require their parsing identity.

## Goals / Non-Goals

**Goals:**

- Publish the complete Windows-resolved path for every filesystem-backed Shell folder.
- Make address editing, copying, resubmission, history, and session restore use that path.
- Preserve valid Shell identities for locations without a real filesystem path.

**Non-Goals:**

- Change breadcrumb labels or navigation-pane labels.
- Invent paths for Home, This PC, Recycle Bin, Network, Libraries, or provider namespaces.
- Change the Shell object used by the current enumeration request.

## Decisions

### Canonicalize metadata at the Shell resolution boundary

After binding the requested location, the resolver will ask the resolved PIDL for
`SIGDN_FILESYSPATH`. A non-empty value becomes the descriptor returned by `ResolvedLocation::metadata`.
If the input was already `FileSystem`, or Windows returns no usable path, the original descriptor is
preserved. The bound folder and PIDL continue to service enumeration, so only future committed
navigation identity changes.

This is preferred over adding a second address string to protocol and persistence state, which can
drift from location identity. Deriving the text from breadcrumbs was rejected because ancestry is
asynchronous and can be unavailable when edit mode begins.

### Reuse existing model behavior

`LocationResolved` already commits `LocationMetadata.descriptor`, and `AddressBarState::for_entry`
already renders `FileSystem` paths as complete strings. No new UI-only state is introduced.

## Risks / Trade-offs

- **A Shell container can expose a path with provider-specific semantics** -> Trust only a non-empty
  `SIGDN_FILESYSPATH` returned by Windows; resubmission continues through the existing Shell parser.
- **A redirected known folder can be temporarily unavailable** -> Preserve the original descriptor
  when Windows cannot return a path so navigation still has truthful fallback behavior.
- **Descriptor canonicalization changes persisted history identity** -> The canonical path is already
  a supported descriptor and session schema, so no migration is required.

## Migration Plan

Add canonical descriptor selection and tests, run model and Shell regressions, then run a headful
address-edit check. Reverting the resolver selection restores prior behavior without persistent data
migration.

## Open Questions

None.
