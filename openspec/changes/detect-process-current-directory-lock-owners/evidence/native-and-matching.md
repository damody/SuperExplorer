# G1/G3 native discovery evidence

Recorded 2026-08-14 on Windows at repository commit `fd66c9e8a759ca903b55b1e9ea156c42d37518e1`.

`cargo test -p explorer-shell-win --lib --locked --offline process_current_directory -- --nocapture --test-threads=1` passed 14 tests. Coverage includes local/UNC/extended paths, drive roots, repeated/trailing separators, case folding, component-prefix rejection, relative/traversal/file bypass, checked address and Unicode contracts, cancellation/deadline checks before and after remote-read boundaries, current-process exclusion, real native plus WOW64 `cmd.exe` exact/parent attribution, access-denied/exit local skips, 4,097-candidate fail-closed behavior, candidate-order independence, and tracked snapshot/process-handle drops on success, typed error, cancellation, deadline, and injected panic. The WOW64 test verifies the process with `IsWow64Process2`.

The implementation creates one Toolhelp snapshot per batch, caps candidates at 4,096, opens processes only with `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ`, reads only native/WOW64 current-directory fields with checked addresses/lengths, discards paths after component matching, and projects PID, creation time, safe executable basename, application type, and non-restartable/protected eligibility. Snapshot and process handles use unique RAII ownership.

Source SHA-256: `crates/explorer-shell-win/src/process_current_directory.rs` `477141DD96B3354B5192FDE37FE81A9D80E430851688071B9DE1D4AF8F1F3773`.

All planned native candidate-policy and RAII cleanup seams are covered.
