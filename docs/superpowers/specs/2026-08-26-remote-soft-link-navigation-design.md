# Remote Soft-Link Navigation Design

## Context

ADB directory enumeration currently infers directories from a trailing slash in `ls -1Ap`, while
SFTP checks only the directory entry's own file type. A symbolic link whose target is a directory is
therefore exposed as a remote file. The file view cannot enter it and the navigation pane does not
offer it as an expandable child.

## Goals

- Classify symbolic links consistently for ADB and SFTP.
- Let users select and enter links whose resolved targets are directories from both the file view
  and the navigation pane.
- Preserve the link path while navigating instead of replacing it with the resolved target path.
- Keep broken and circular links visible but non-navigable, with distinct type labels.
- Bound link resolution so malformed remote trees cannot hang directory enumeration.

## Non-Goals

- Editing or creating symbolic links.
- Displaying the complete link target in the initial UI.
- Following broken or circular links during file operations.
- Canonicalizing the address bar to the resolved target path.

## Design

### Shared item classification

`explorer-remote` will replace the remote entry's directory boolean with a typed item kind covering
regular files, regular directories, links to files, links to directories, broken links, and circular
links. The kind owns the shared decisions for whether an item is a container and which remote type
label the UI displays. Both navigation surfaces will consume those decisions through the existing
`FileEntry.is_container` contract, avoiding separate link heuristics in the UI.

Directory links remain addressed by their link-side `VirtualLocationDescriptor`. Entering one lists
that descriptor through the provider, so breadcrumbs and the address bar continue to describe the
path selected by the user.

### ADB classification

ADB enumeration will use a machine-readable shell probe that reports each direct child's name,
link status, resolved target type, and failure status. Because `adb shell` joins host arguments into
one remote command, the validated parent path is
base64-encoded into a safe data-only assignment prefix and decoded by the fixed probe. Raw path
bytes never enter remote shell syntax.

Resolution tracks visited link paths and enforces a finite hop limit. A missing or inaccessible
target becomes a broken link. Re-visiting a link or exhausting the hop limit becomes a circular
link. Names are transported without relying on a trailing slash and are validated before being
added to a virtual descriptor.

### SFTP classification

SFTP first uses directory-entry metadata. For a symbolic link it resolves the target with SFTP
metadata operations, normalizes relative targets against the link's parent, and tracks visited
remote paths under the same finite hop limit. Missing or inaccessible targets become broken links;
repeated paths or hop-limit exhaustion become circular links.

### UI behavior

The adapter maps regular directories and directory links to `FileEntry.is_container = true`.
Consequently, double-click, Enter, open-in-new-tab, navigation-pane expansion, and breadcrumb child
menus all use the existing directory navigation path. File links remain selectable and openable as
files. Broken and circular links remain selectable but are not containers and are not navigated.

The Type column uses these labels:

- `Remote folder`
- `Remote file`
- `Remote folder link`
- `Remote file link`
- `Broken remote link`
- `Circular remote link`

## Error Handling and Safety

Link-resolution failures are classified only when they mean the target cannot be resolved. A
failure of the directory-listing command or SFTP session still fails the directory request normally
rather than converting every entry into a broken link. Cancellation is checked during per-entry and
per-hop work. Link resolution is bounded and never recursively enumerates target directories.

## Testing

- Model tests cover container and label decisions for every remote item kind.
- Fake ADB tests cover ordinary files/directories, relative and absolute directory links, file
  links, broken links, circular links, cancellation, and hostile names.
- SFTP-focused tests cover target normalization and the same terminal classifications without
  requiring credentials.
- Application-adapter tests verify directory links become navigable containers and broken/circular
  links remain distinct non-containers.
- Validation runs formatting, affected crate tests, workspace checking, and strict OpenSpec
  validation.
