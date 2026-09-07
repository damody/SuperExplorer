## 1. Persistence and policy

- [x] 1.1 Add `SearchEnginePreference` and availability helpers
- [x] 1.2 Persist on `ViewSettings` / `PersistedViewSettings` with serde default Everything
- [x] 1.3 Cover default, round-trip, legacy decode, remote disable, and Apply rejection

## 2. Folder Options UI

- [x] 2.1 Add General-page radios with disabled hint
- [x] 2.2 Ignore clicks on unsupported radios
- [x] 2.3 Add i18n keys to every locale `settings.ftl`

## 3. Search pipeline

- [x] 3.1 Pass `engine` on `StartSearch`
- [x] 3.2 Execute only the selected engine with no silent fallback
- [x] 3.3 Reconstruct MFT paths and match filenames
- [x] 3.4 Probe availability when Folder Options opens and before search
