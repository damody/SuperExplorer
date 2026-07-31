# Shell context menu and locked-delete handoff

Date: 2026-07-29
OpenSpec change: `complete-shell-context-and-locked-delete-parity`

## Delivered behavior

- File, folder, multi-selection, and background menus use the native Windows Shell menu in an isolated broker worker. The app owns one persistent broker supervisor and submits menus through one bounded asynchronous lane; it does not create a broker or an unbounded thread for each click. Ordinary and Shift invocation profiles preserve installed providers, lazy cascades, owner-drawn entries, keyboard navigation, Escape, outside-click cancellation, and focus restoration.
- Deletion failures caused by sharing or lock violations use Restart Manager to discover a bounded owner list. The dialog supports Retry, Close programs and retry, Cancel, keyboard focus trapping, UI Automation, multiple owners, partial results, stale PID protection, and one retry of the exact original recycle/permanent-delete operation.
- Close programs is graceful only. The application never calls `TerminateProcess`, never elevates, never closes system/protected/critical/elevated/self/helper processes, and revalidates PID plus creation time before mutation.

## Evidence

- `complete-shell-context-menu-direct` inventories the real machine Shell providers. The current machine exposes installed 7-Zip, WinRAR, TortoiseGit, editor, Defender, cloud/media, and Send To commands.
- `complete-shell-context-menu-broker` compares every file/folder/multi/background and ordinary/Shift combination. Broker-supervised and directly spawned disposable workers match byte-for-byte for command count, extended state, and recursively collected label fingerprints. An independent test-process query is retained as diagnostic evidence because some third-party providers vary entries by executable host identity.
- Concurrent client clones are released together and must report one supervisor PID, one launch, one handshake, and four correlated terminals. The application-side context-menu lane admits only one active request and uses cancellation/queueing for replacement.
- `locked-delete-recovery-headful` proves real lock-owner UIA, Tab/Enter, Escape, pointer Cancel, two owners, and Close-and-retry against owned fixtures.
- `context-lock-resource-soak` runs ten native popup and ten real Restart Manager lock cycles and requires workers, helpers, sessions, and terminal state to return to baseline after every cycle.
- Fake Restart Manager boundaries cover empty results, buffer growth, denied calls, unstable owner lists, cancellation, PID reuse, refused close, timeout classification, and cleanup without touching user files.

## Limitations

- Shell extensions control their own labels, icons, submenus, latency, and availability. A provider may intentionally expose different commands to different executable hosts; the broker boundary test therefore requires exact results against the same disposable worker without the supervisor Job and records other-host differences diagnostically.
- Restart Manager can request graceful shutdown but cannot guarantee that an application cooperates. Refused and timed-out owners remain visible and the source is not reported deleted.
- System, protected, critical, higher-integrity, stale-identity, and inaccessible owners are shown as ineligible; SuperExplorer does not offer a force-termination fallback.

## Release and installation validation

- Release revision: `c64f33d76b66`.
- The finalized x64 app, broker, and worker passed manifest/PE validation. The NSIS package passed fresh install, installed-path launch and broker handshake, in-place upgrade, clean shutdown, and uninstall cleanup in an isolated per-user directory.
- The same three release binaries were synchronized to `C:\Users\Damody\AppData\Local\Programs\SuperExplorer` and verified byte-for-byte by SHA-256.
- SHA-256: app `75AF59C9A0D7E4F33A99D435781049D7A447F43B062789A22827CE1CD9A34672`; broker `3DA1FAFE0DCD09E41E5E6B8828FE0A2ACF660305081D6C0FB7035C053964C855`; worker `15AF3BD492D9C19EBCE46301D246C2BC27232B9D19268E61201627E9EA03244C`.

## Rollback

- Context menus can fall back to the prior ordinary Shell query profile without changing item identity or filesystem state.
- Lock-owner recovery can be disabled independently. Delete then returns the existing safe error notice and does not inspect or close another process.
- Remove the three co-located release binaries together when rolling back (`SuperExplorer.exe`, broker, worker); protocol version verification rejects a mismatched helper before extension activation.
