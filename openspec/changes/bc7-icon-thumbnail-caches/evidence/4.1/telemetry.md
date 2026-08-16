# G-TELEMETRY evidence

Status: passed on 2026-08-14.

The bounded Host snapshot retains stable cache entries for independent icon/thumbnail disk and GPU ownership and the encoder pipeline. It now also carries `Bc7PipelineTelemetryV1`, a fixed, identity-free detail record for queue/concurrency/staging limits and usage; submitted/completed/duplicate/overload/oversized/cancelled/stale/persistence/fallback counters; direct icon/thumbnail GPU upload and eviction counters; and adapter capability. The snapshot has no path or free-form identity field and remains subject to the 32-entry bound and duplicate-ID rejection.

Folder Options merges Host-owned telemetry with its local icon, base-icon, and thumbnail memory LRU measurements. It renders independent memory/disk/GPU used/limit rows, rollout and capability states, encoder state, queue state, bounded failure categories, and per-kind GPU upload/eviction counters. Independent budget application and the existing one-second window-scoped sampling, latest-completed retention, pending/unavailable states, close cancellation, and stale-sample handling are preserved.

Verification:

- `cargo test -p explorer-model --lib --locked --offline cache_telemetry::tests -- --nocapture --test-threads=1`: 3 passed, including bounds, duplicate IDs, subtotal partial state, path redaction, and BC7 detail mapping.
- `cargo test -p explorer-ui --lib --locked --offline folder_options_window::tests -- --nocapture --test-threads=1`: 8 passed, including Host mapping, independent cache budget normalization, sampler single-flight reuse, pending/unavailable lifecycle, resize, and close/capture termination.
- `cargo check -p explorer-ui -p explorer-app --locked --offline`: passed; unrelated existing warnings remain.

Source SHA-256:

- `cache_telemetry.rs`: `91CB2EE38BFAE0CE8F47F910AC14E37937083EBC78B2602279474DC6D9818992`
- `brokered_service.rs`: `4EB8BB38F4501422F1304FA3A3D9348121A8289A6B91AA54AFC1CA03585088D1`
- `folder_options_window.rs`: `EE26B8E16BCB8210D89FBDAA0D61E98EF4EBC153FFEA2FA438CDBD8BA21466DC`
- `chrome.rs`: `8BEEE344E62E7AFA131EB01941CD2358C12F983707F2DB87105F835CB06F4D09`
