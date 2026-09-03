# Final review

The APK action publishes `Started` before spawning its detached resolver/install worker. If that event cannot be delivered, no ADB work is spawned. The context-menu request then completes immediately, so the native menu and later right-clicks are not held by a long install.

Each install is correlated by request ID. The UI accepts one matching `Started`, accepts only the first terminal, rejects duplicates and unmatched events, preserves active records under capacity pressure, and evicts an older terminal first. Success remains for 5 seconds; failure, cancellation, and timeout remain for 12 seconds.

The notice says `安裝中` while running and uses an indeterminate bar without percent or byte claims. Terminal wording is distinct. Only the APK base name, friendly device label, exact serial, and a bounded generic failure summary are rendered; full paths and raw ADB output are not shown.

The system-first resolver and managed official Platform-Tools fallback remain unchanged. Existing exact-serial and argument-safe `install -r` tests pass. `qq9.3.55.apk` was verified as a Local single-file APK candidate without installing it on a real device.

Focused product tests, application library tests, format, application check, strict OpenSpec validation, and a clean headful launch/close passed. A broad repository UI run also exposed eight pre-existing failures in unrelated details-menu, icon-cache, thumbnail, layout, and placeholder assertions; none intersects the APK status code or its focused tests.

Open findings for the APK install-status change: **0**.

## 2026-09-03 terminal-admission regression repair

A real user run exposed that the generic presentation boundary rejected the APK terminal after the native context-menu request token had been retired/cancelled. The APK-specific gate now admits a terminal from the matching active notice independently of popup cancellation, while still rejecting unmatched and duplicate terminals. The exact cancelled-popup regression test passes.

The user-supplied `C:\Users\Damody\Downloads\endfield-hg-1-1.5.3.apk` (1,611,802,878 bytes) was installed with the system ADB at `C:\Users\Damody\AppData\Local\Android\Sdk\platform-tools\adb.exe` to `ASUSAI2501B (emulator-5554)`. ADB returned `Performing Streamed Install` followed by `Success` with exit code 0.
