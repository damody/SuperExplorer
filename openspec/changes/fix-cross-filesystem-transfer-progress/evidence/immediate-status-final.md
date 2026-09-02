# Immediate transfer status final evidence

Date: 2026-09-02

## Root cause

External Windows Explorer drops queued `DataTransferRequest::DropExternal` without first inserting
the request-correlated `OperationRecord`. The UI intentionally rejects progress and terminal events
that have no matching live record, so remote transfer events were invisible and only the later
directory refresh made the copied file appear. Remote metadata estimation also ran before the first
Preparing event, extending the silent interval on SFTP.

## Fix

- Insert a started Preparing record synchronously when left-drop or resolved right-drop is accepted.
- Preserve Copy versus Move in drag and owned-clipboard paste placeholders.
- Emit Preparing before remote metadata preflight, update the total after estimation, and switch to
  Transferring only after the first delivered-byte delta.
- Render explicit `準備複製` / `正在複製` / `複製完成` and Move equivalents.
- Keep the existing correlated submission-failure terminal path, which now also closes external-drop
  records because those records exist before submission.
- Keep the real drag runner's target inside the file-view body instead of the status strip.

## Focused verification

- UI left external drop record and request routing: passed.
- UI right external drop choice and request routing: passed.
- Owned Cut paste creates a Move record: passed.
- Operation center progress/terminal correlation: passed.
- Chinese Copy lifecycle messages: passed.
- Remote reporter Preparing, monotonic byte progress, unknown-total degradation, terminal barrier:
  passed.
- `cargo check -p explorer-app`: passed.
- Installed build hashes for SuperExplorer, broker, and worker: matched release artifacts.
- Real Windows Explorer `an.txt` drop to `adb://emulator-5554/sdcard/Download`: passed.
- Real Windows Explorer `an.txt` drop to `sftp://45.32.49.125/home/linuxuser`: passed.
- ADB 512 MiB native upload/download intermediate progress probe: passed.

The SFTP drag acceptance and completion path was verified against the configured live profile. The
standalone long-running SFTP destructive fixture was stopped after it exceeded the focused test time
budget; no credential was emitted or persisted in this evidence.
