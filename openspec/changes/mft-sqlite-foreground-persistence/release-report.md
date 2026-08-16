# MFT SQLite foreground persistence release report

## Candidate identity

- Installer `SuperExplorer-Setup-1.2026.8.14-x64.exe`: SHA-256 `B24FB234DEEBB0D6A6C7BBF669A74212B1F816DFC8F6C199739135594AD184C6`.
- Installed service: `C:\Program Files\SuperExplorer\superexplorer-mft-service.exe`, SHA-256 `A4916F7ADFCC6D7B237F45AECDC5D0329565C651BC17F7E02E8BDAAF1E4DA2E1`.
- Installed application: SHA-256 `9A7536CC424AC5EF619AB247629E9DA42060A105F0A6ED46A545343B17B65919`.
- Service is running as `LocalSystem`, start mode `Auto`, from the expected installed path.
- SQLite is bundled into the service; no external SQLite runtime was added.

## Installed measurements

The clean unfocused trace ran for 1,206.490 seconds with 603 representative mutations. The cache stayed at nine fixed files, emitted zero cache events, and the service read/write byte deltas were both zero.

The focused trace ran for 1,252.204 seconds with 625 mutations. It observed four WAL events: D at 599,094 ms, C at 600,104 ms, then C and D at 1,201,414 ms. Grouped by volume and 30-second burst windows, each volume made two attempts and successive burst starts were at least ten minutes apart.

Defender counters are reported only as environmental CPU/I/O telemetry. In the clean unfocused window, Defender read delta was 142,562,505 bytes and write delta was 3,086,584 bytes. No conclusion is drawn from working-set values, and concurrent host activity prevents attributing all Defender I/O to SuperExplorer.

## Installer lifecycle and rollback

Repair stopped and restarted the service (PID 268260 to 276588) before replacing files. Silent uninstall exited zero, removed the service, and retained all nine SQLite main/WAL/SHM members. Fresh reinstall restored the current candidate. Rollback installer 1.2026.8.13 (SHA-256 `C1E280BF722B845370EB2EA10CFD8920AB09904CB2EF6BF780FBB15AE71005D6`) installed service hash `1C9CC1BE477007FBDDC601E637E785889B3ADCB0F2FF73BD96C79C20A11B4B4C` without changing cache names or lengths. Reinstalling 1.2026.8.14 restored the exact current service/application hashes.

Older builds ignore SQLite rather than reinterpret it as legacy sidecars. The cache remains preserved for rollback and rebuild. An actual pre-SQLite installer was unavailable, so the installed legacy-upgrade procedure remains open even though the deterministic migration fault matrix passes.

## Verification and residual risks

- Formatter, locked package checks, MFT service/application tests, extension host/broker tests, and UI test library pass.
- Current warnings-denied Clippy is not countable because active cache-budget work contains unrelated truncation lints.
- A real Windows reboot with pending work is not captured because it would terminate this execution session.
- Installed crash/disconnect/session-switch coverage is incomplete.
- The current independent review task is open because the previous review became stale and this run is intentionally single-agent.

## Approval disposition

The implementation and all safely actionable installed measurements pass, but this change is not presented as archive-ready while tasks 4.2.4, 5.1.4, 5.2.5, 5.2.6 and 6.1.3 remain open. No blocked or stale evidence is counted as passed.
