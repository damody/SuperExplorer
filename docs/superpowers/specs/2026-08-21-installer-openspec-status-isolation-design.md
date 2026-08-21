# Installer OpenSpec status isolation

## Goal

Allow the production `build_install.bat` entry point to build the combined
installer when the `SuperDesktop` submodule contains untracked OpenSpec work,
without weakening the source-integrity checks for anything outside OpenSpec.

## Design

`build_install.bat` continues to invoke the component-all installer flow.  The
Lua installer admission check will filter only untracked porcelain entries
whose path is inside `SuperDesktop/openspec/`.  It will run that filtering
before it evaluates submodule cleanliness.

The filter applies exclusively to untracked (`??`) entries.  Modified,
deleted, renamed, conflicted, or malformed porcelain records under OpenSpec
remain failures, as do every status entry outside OpenSpec.  The existing
checks for an initialized submodule, its HEAD, parent gitlink, and configured
origin URLs remain unchanged.

## Alternatives

1. Ignore only OpenSpec evidence logs.  This is the current behaviour and
   fails when test runs leave profiles, screenshots, JSON reports, or other
   OpenSpec files.
2. Bypass all SuperDesktop dirty checks.  This would allow source changes to
   enter a release build and is rejected.
3. Ignore untracked OpenSpec entries only.  This isolates development and test
   artifacts while preserving source and repository identity protections. This
   is the selected approach.

## Failure handling and verification

The installer must still fail with a diagnostic when a non-OpenSpec
`SuperDesktop` file is untracked or when any tracked file is modified. Tests
will cover accepted OpenSpec untracked paths, rejected source paths, rejected
tracked OpenSpec modifications, and the public batch entry-point argument.
The existing check-only mode will be used to validate the end-to-end handoff
without generating or launching an installer.

## Non-goals

This change does not alter OpenSpec validation, remove files, change global
Git ignore rules, or relax the main repository's source checks.
