# Running-app upgrade reliability design

## Problem

The installer stops the MFT service before replacing files but does not quiesce
running `SuperExplorer.exe` processes. A silent upgrade can therefore update
shortcuts while leaving the old application binary in place, yet still return
exit code zero. This makes a rebuilt repeated-launch fix appear installed when
the installed executable is stale.

## Decision

Embed an installer-owned PowerShell quiescence script. It receives the resolved
installation directory, enumerates only processes whose executable path is the
exact `SuperExplorer.exe` below that directory, requests graceful main-window
closure, waits for a bounded interval, force-stops only the remaining exact-path
matches, and verifies that none remain. Query, termination, timeout, or final
verification failure returns nonzero.

NSIS extracts and invokes the script before service shutdown and before any
application file replacement. A nonzero result shows a controlled error and
aborts the section, so installation cannot report success after a known stale-
binary condition. Fresh installs with no matching process pass immediately.

## Alternatives

- `taskkill /IM SuperExplorer.exe` is smaller but can terminate development or
  portable copies outside the selected installation directory.
- A new Rust process-closer executable offers a native implementation but adds
  build, packaging, signing, and lifecycle surface for a bounded installer job.
- Relying on Windows delayed replacement would require reboot semantics and
  would not meet immediate post-install double-launch behavior.

## Safety and recovery

Path comparison is case-insensitive after full-path normalization. The script
does not enumerate by command text, process name alone, parent process, or user-
supplied glob. It never deletes files. Graceful close precedes force termination.
The existing installer abort path preserves the previous files when quiescence
cannot be proven.

## Verification

- Script tests cover no-process success, exact-path termination, similarly named
  processes outside the target directory, and final absence verification.
- A source contract test proves NSIS invokes the script before `File
  "${APP_EXE}"` and aborts on nonzero exit.
- Release and test-installer builds must pass.
- Installed validation begins with two running windows, performs a silent
  upgrade, verifies both old PIDs exit and installed/release hashes match, then
  launches the shortcut twice and observes a restored first window plus `C:\`
  for the repeated launch.
