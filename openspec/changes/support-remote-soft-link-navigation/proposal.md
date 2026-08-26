## Why

ADB and SFTP symbolic links to directories are currently classified as remote files, so neither the
file view nor the navigation pane can enter them. Invalid links are also indistinguishable, which
can mislead users and makes link cycles unsafe to probe.

## What Changes

- Add a provider-neutral remote entry classification for ordinary entries and symbolic-link target
  states.
- Resolve ADB and SFTP symbolic links with bounded cycle detection during directory listing.
- Make directory links selectable and navigable from the file view, navigation pane, and breadcrumb
  child menus while preserving the user-visible link path.
- Display file links, folder links, broken links, and circular links with distinct Type labels.
- Keep broken and circular links selectable but non-navigable.
- Add provider, adapter, and navigation regression tests. Creating or editing links remains outside
  this change.

## Capabilities

### New Capabilities

- `remote-soft-link-navigation`: Remote symbolic-link classification, safe resolution, presentation,
  and navigation behavior for ADB and SFTP.

### Modified Capabilities

None. The active ADB/SFTP change defines general remote entry points but has no symbolic-link
contract; this change introduces a focused additive capability.

## Impact

- `explorer-remote` provider entry contract and ADB/SFTP listing implementations.
- `explorer-app` remote-to-Explorer event adapter.
- Existing `FileEntry.is_container` consumers in `explorer-ui`; no new UI navigation path is needed.
- Fake-provider and adapter test fixtures that construct remote entries.
- No persisted-format, credential, extension ABI, or local Windows Shell behavior changes.
