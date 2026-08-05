## Why

The Extensions popup currently lets long contribution labels escape its bounds and
renders the EXIF rename and bulk-folder commands as inert rows that close without
collecting parameters or performing their advertised operation. These official
examples need the host-rendered form, preview, approval, and operation-plan behavior
already promised by the extension platform contract.

## What Changes

- Bound and ellipsize extension popup labels while preserving full accessible names.
- Replace both command rows with anchored host-rendered interaction panels.
- Preview and validate EXIF rename and bulk-folder plans before confirmation.
- Execute approved plans through host-owned file operations; cancellation and invalid
  input make no filesystem changes.
- Add two-stage Escape/outside-click behavior, unit contracts, and headful UITEST with
  real filesystem assertions.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `extension-commands-forms-and-operation-plans`: Make the existing typed-form,
  preview, approval, EXIF rename, and bulk-directory requirements observable from the
  production Extensions popup.

## Impact

Affected areas are `explorer-ui` extension-menu state/rendering/actions,
`explorer-app` host command composition and operation submission, the two official
extension fixtures, and UITEST scripts/manifest. No persisted setting or public ABI
break is required.
