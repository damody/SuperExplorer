# Installed repeated-launch reliability design

## Problem

An installed SuperExplorer shortcut cannot reliably open a second window. The
second process reaches extension-host startup, then exits because both processes
open the same private `.sepack-staging` root with Windows directory handles that
allow only read sharing. Test installers also persist `--diagnostics-console` in
the Start Menu shortcut, causing ordinary shortcut launches to bypass repeated-
launch classification.

## Decision

Keep the existing independent-process window model. Make the shared staging-root
directory handle explicitly share read and write access while continuing to
deny delete sharing. Imported
packages already use cryptographically random, create-new child directories, so
the root handle can be shared without allowing two importers to own the same
candidate. Existing identity, reparse-point, bounded scavenging, and active-owner
checks remain unchanged.

Installer shortcuts never receive diagnostic arguments. A test installer may
still pass `--diagnostics-console` only to the optional finish-page launch; later
Start Menu and desktop launches are ordinary launches.

## Alternatives

- A resident process plus IPC-created GPUI windows best mirrors Explorer's
  process model, but requires a broad window-composition and lifecycle refactor.
- Disabling extensions in later processes avoids the lock but produces windows
  with inconsistent capabilities.
- Per-process staging roots also avoid the lock, but duplicate scavenging roots
  and add cleanup complexity that the existing unique-child design does not need.

## Behavior and failure handling

The first ordinary launch keeps session restoration. Each later ordinary launch
opens one independent window at `C:\`. Concurrent extension imports use separate
children below the same verified staging root. Genuine unsafe-root or I/O errors
continue through the controlled startup diagnostics path.

## Verification

- A Windows extension-host test opens two importers against one source root at
  the same time and verifies both handles remain usable.
- Launch-classification tests retain special-mode isolation semantics.
- Installer preprocessing confirms shortcuts have empty arguments in both test
  and production builds.
- An installed smoke test starts the Start Menu shortcut twice, observes two
  live processes/windows, and confirms the later window starts at `C:\` without
  a staging sharing-violation error.
