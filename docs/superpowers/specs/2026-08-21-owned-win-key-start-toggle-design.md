# SuperDesktop Win Key Start Toggle Design

## Goal

When SuperDesktop owns the Windows shell, pressing and releasing either Windows logo key by itself opens or closes the owned Start menu. The behavior must match Windows without breaking existing Windows-key chords.

## Native contract

Microsoft documents the Windows logo key as **Open or close Start**. SuperDesktop therefore treats a standalone Windows-key gesture as a toggle and routes it through the same callback as the taskbar Start button.

## Input state machine

The low-level keyboard hook owns a small explicit state:

- `Idle`: no standalone Windows-key gesture is pending.
- `Candidate(vk)`: the left or right Windows key was pressed by itself.
- `Cancelled(vk)`: another key was pressed while that Windows key was held.

The first left/right Windows keydown enters `Candidate` and is consumed. Repeated keydown for that same key is consumed without producing another action. Any non-Windows keydown while a candidate is held cancels the standalone gesture while allowing existing supported chord routing to run. A second Windows key also cancels the original standalone gesture to avoid ambiguous dual-key input.

Releasing a matching candidate emits exactly one `ToggleStart` action and consumes the release. Releasing a cancelled Windows key clears the state and emits nothing. Unrelated key releases do not affect the state.

## Runtime routing

`ToggleStart` is queued using the existing bounded hook-to-UI action channel. The GPUI refresh loop resolves the first taskbar's `callbacks.start` callback and invokes it on the UI context. This preserves the existing owned Start lifecycle:

- closed Start opens at the configured taskbar alignment and monitor;
- open Start closes;
- no Explorer, StartMenuExperienceHost, simulated input, or second Start implementation is involved.

The hook remains shell-scoped. Preview mode continues to leave the real Windows shell responsible for the Windows key.

## Failure handling

Hook callbacks stay panic-contained and allocation-free. A missing taskbar callback is reported to the console instead of panicking. Hook shutdown resets both chord and standalone-Windows-key state so no gesture survives a restart.

## Verification

Unit tests cover left/right Windows keys, repeat keydown, open/close toggles, supported and unsupported chords, dual Windows keys, and mismatched releases. Source-contract tests prove the runtime routes `ToggleStart` to the owned Start callback.

A headful UTIT case injects real left-Windows down/up events into an owned-shell session, verifies the owned Start window appears, injects the gesture again, and verifies it closes. The harness records trace evidence and restores Explorer and the prior shell registry state in `finally` cleanup.
