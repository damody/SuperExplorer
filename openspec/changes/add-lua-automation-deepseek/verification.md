# Verification Evidence

Date: 2026-07-28 (Asia/Taipei)

## Static gates

- `cargo fmt --all -- --check`: passed.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_architecture.ps1`: passed. The check reports that UI is Shell-free, automation is platform-neutral, and test-support is absent from production dependencies.
- `cargo check --workspace --locked`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `git diff --check`: passed; Git reported only line-ending conversion notices for pre-existing/concurrent modified files.

## Automated tests

- The automation core suite passed all 42 tests, including documentation contracts, router policy, virtual timing, Lua registration/runtime limits, task-relative files, atomic output, deletion confirmation, process policy, lifecycle rollback, and the 100,000-event benchmark.
- The Windows automation adapter suite passed all 8 tests, covering Job Object descendant cleanup, watcher behavior, input/WinEvent/system source unload/restart, clipboard, credentials, and message translation.
- The GPUI automation render test passed.
- The application automation startup/final-window shutdown test passed.
- The complete workspace test run reached an existing real OLE clipboard test while another process temporarily owned the Windows clipboard. That first environmental failure poisoned the suite's shared STA mutex and caused six dependent failures. The exact clipboard test then passed in isolation, and `cargo test -p explorer-shell-win --lib --locked -- --test-threads=1` passed with 63 passed and 6 explicitly environment-gated tests ignored. All automation, AI, application, and documentation tests had already passed in the workspace run.

## Runtime and stress evidence

- The 100,000-event source callback test completed in about 0.09 seconds and asserted p99 callback latency below 1 ms.
- One hundred enable/reload/disable cycles released every Lua VM.
- Windows Job Object timeout/cancellation tests verified descendant process-tree cleanup.
- A real application auto-close smoke run opened, rendered, performed ordered automation shutdown, and exited successfully.

## DeepSeek live smoke

- The ignored, opt-in `deepseek-v4-flash` live smoke test was run once with the credential supplied only through the test process environment.
- It returned a non-empty short summary successfully in about one second.
- Neither the credential nor response content was printed or persisted, and a repository secret scan found no matching credential.

