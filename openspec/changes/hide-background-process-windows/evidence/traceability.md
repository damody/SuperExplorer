# Traceability

| Requirement / scenario | Implementation | Gate / evidence task IDs |
|---|---|---|
| Debug and release parent diagnostics console | `crates/explorer-app/src/main.rs`; PE subsystem verifier | `PARENT-1`, `WINDOW-DEBUG-1`, `WINDOW-RELEASE-1`; 2.2.1–2.2.2, 4.2.1–4.2.4 |
| Hidden ADB startup and operations | common configurator; `explorer-remote/src/adb.rs` | `POLICY-1`, `ADB-1`; 2.1.1–2.1.4, 3.1.1–3.1.4 |
| Hidden automation with output, failures, timeout, cancellation, cleanup | both automation process hosts | `AUTO-1`, `AUTO-LIFECYCLE-1`; 3.2.1–3.2.5, 4.2.5 |
| Hidden extension broker, worker, probes, and descendants | broker library/main/worker paths | `EXT-1`, `EXT-LIFECYCLE-1`; 3.3.1–3.3.4 |
| Explicit visible Open Command Prompt | `explorer-shell-win::launch_command_prompt` | `VISIBLE-1`; 2.2.3 |
| Unknown production launcher rejection and test/build exclusions | inventory JSON and Python validator/self-tests | `INV-1`, `INV-2`; 1.1.1–1.1.3, 4.1.1–4.1.3 |
| Debug/release and hanging-child verification | common runtime test, broker window test, PE verifier, focused lifecycle tests | `WINDOW-DEBUG-1`, `WINDOW-RELEASE-1`; 4.2.1–4.2.5 |
| Full integration and audit | formatting, focused suites, architecture check, strict OpenSpec | `BUILD-1`, `TEST-1`, `ARCH-1`, `TRACE-1`, `FINAL-1`; 5.1.1–5.2.4 |
