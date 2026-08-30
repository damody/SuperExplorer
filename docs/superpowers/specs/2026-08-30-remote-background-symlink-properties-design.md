# ADB/SFTP Background Symlink and Folder Properties Design

## Goal

Extend the ADB and SFTP file-view background context menus so both providers expose three
real commands: Create Folder, Create Shortcut, and Properties. Create Shortcut creates a Linux
symbolic link through a dedicated editable window. Properties describes the currently displayed
remote directory rather than a selected child item.

## User experience

- The remote background menu keeps the accepted Windows-style visual renderer and presents
  `新增資料夾`, `新增捷徑`, and `內容` in that order, with separators only where required by the
  shared menu grammar.
- `新增捷徑` opens a separate, owned GPUI window. It contains editable `捷徑名稱` and `目標路徑`
  fields plus Cancel and Create actions.
- The target may be relative, absolute, or currently nonexistent so dangling Linux symbolic links
  remain supported.
- The link name must be a single safe child name. Empty names, `.`, `..`, slash, backslash, NUL,
  and provider-invalid components are rejected before dispatch.
- Create runs through the existing remote worker boundary. The UI remains responsive. Success
  closes the editor, refreshes the directory, and selects the created entry when it appears.
  Failure preserves both inputs and shows an inline error for correction.
- `內容` opens the existing remote Properties window with a snapshot for the current directory.
  It shows the display name, canonical public ADB/SFTP path, directory type, permissions, modified
  time, and size information when the provider can return it. Unsupported fields say they are
  unavailable rather than inventing values.

## Architecture

### Provider contract

Add narrow provider-neutral operations to `RemoteProvider`:

- create a symbolic link at a destination descriptor with an opaque target string;
- obtain metadata for one remote descriptor, including the descriptor itself when it is the
  current directory.

ADB implements link creation with an argument-safe shell script invocation of `ln -s --` and
metadata with the same bounded/cancellable stat machinery used for directory entries. SFTP uses
the SFTP protocol's symlink and metadata operations. Neither implementation passes unescaped UI
text through a general command shell.

### UI and application boundary

`ExplorerRoot` captures the immutable tab, generation, parent location, link name, and target
before submitting work. A dedicated observer opens or reuses the owned shortcut-editor window,
following the existing remote Properties and bookmark-editor window lifecycle. Provider work is
submitted through the existing remote job/coordinator boundary; the window only owns editable
state, validation, progress, and error presentation.

The background Properties action requests metadata for the current location, then converts it to
the existing `RemotePropertiesWindowSnapshotV1`. It never fabricates a selected row and ignores a
late result after navigation or tab-generation replacement.

## Error and lifecycle rules

- Validation errors do not start provider work.
- Duplicate names, permission failures, disconnected devices, unavailable profiles, and protocol
  errors are surfaced inside the shortcut window without losing input.
- Closing the window cancels or detaches only that editor session; it does not block the main UI.
- Completion is applied only to the captured tab/generation/location.
- Menu dismissal, keyboard focus, accessibility roles, edge clamping, and right-click replacement
  remain unchanged.

## Testing

- Provider tests cover valid relative/absolute/dangling targets, escaping-sensitive names, invalid
  child names, duplicate links, cancellation, and backend error propagation.
- UI/state tests cover background command membership/order, dedicated-window snapshot data,
  validation, single dispatch, stale completion rejection, failure retention, successful refresh,
  and current-directory Properties routing.
- Headful ADB and SFTP tests open the background menu, create and remove disposable links, verify
  their targets, open Properties for the current directory, and confirm dismissal/accessibility.
- Existing remote context-menu visual and interaction tests must continue to pass.

## Scope exclusions

This change does not add Windows `.lnk` or freedesktop `.desktop` files, symlink editing after
creation, recursive target validation, automatic target browsing, or Local filesystem commands.
