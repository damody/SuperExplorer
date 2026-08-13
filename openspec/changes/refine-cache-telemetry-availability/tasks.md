## 1. Model and classify telemetry availability

- [x] 1.1 Add an explicit pending, available, and unavailable state to cache telemetry snapshots without changing cache budgets or IPC framing.
- [x] 1.2 Classify MFT startup, slow acknowledgement, and reconnect retry as pending; classify only authoritative terminal owner failures as unavailable.

## 2. Retain and present samples

- [x] 2.1 Retain one last successful usage value per cache owner while a later sample is pending, and replace it when a new sample arrives.
- [x] 2.2 Render first-sample pending as `— / limit`, retained pending and available as `used / limit`, and confirmed failure as `Unavailable / limit` while leaving limit editors enabled.

## 3. Regression coverage and verification

- [x] 3.1 Add unit tests covering first pending, stale-value retention, confirmed unavailable, recovery, and limit changes during pending telemetry.
- [x] 3.2 Run formatting and targeted Rust tests, then record passing commands and results in the change evidence.
- [x] 3.3 Build or use the current installable test package, verify Folder Options against a running MFT service, and capture a screenshot proving working telemetry does not display `Unavailable`.
