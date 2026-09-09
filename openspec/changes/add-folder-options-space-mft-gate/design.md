## Context

Folder Options has General / View / Extensions. View currently renders `cache_budget_controls` (icon/thumbnail/GPU/disk plus the MFT resource group and TTL) above the advanced presentation checkboxes. MFT Service is a LocalSystem named-pipe process (`SuperExplorerMft`); Folder Size and Size Map consume it through `folder.aggregate` / `folder.tree`, not a host feature flag. Plugin features already have a free-form `capabilities` list.

See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Isolate cache/MFT UI on Space without changing budget math.
- Persist `mft_enabled` default true; missing field stays on.
- Gate plugins on host capability `mft`; official folder-size packages declare it.
- Apply-off: stop MFT queries, drop in-process and service query caches, keep SQLite.
- Confirm before enabling an MFT plugin while MFT is off.

**Non-Goals:**
- Stopping or deleting the Windows SCM service (needs admin; re-enable would be slow).
- Auto-restoring plugins when MFT is turned back on.
- Changing cache budget ranges or telemetry sampling.
- New manifest schema field; reuse `capabilities: ["mft"]`.

## Decisions

- **Tab order General, View, Space, Extensions.** Space is next to View because the controls moved from View. English label `Space`, zh-TW `空間設定`.
- **`mft_enabled` on `ViewSettings` / `PersistedViewSettings`** with `serde(default = true)`. Same persistence path as search engine.
- **Capability `mft` rather than a new `host_dependencies` field.** Avoids a deny-unknown-fields manifest bump. Host treats `mft` as gated, not as an always-on ABI.
- **Official plugins that require MFT:** `rust-folder-size-visual-column` and `rust-folder-size-map-view`. Other bundled plugins do not.
- **Draft disable is eager.** Unchecking MFT immediately clears those plugins' draft enabled flags so Apply persists the disable. Re-enable MFT does not restore them.
- **Plugin enable while MFT off uses a Folder Options modal** (same overlay pattern as session-reset confirm). Confirm sets both flags in the draft; user still needs Apply/OK.
- **MFT search:** `SearchEngineFacts.mft_feature_enabled`. When false, MFT support is Unavailable and the radio hint is the Space-off string. Existing `can_apply` rejection stays.
- **Service unload via pipe `REQUEST_KIND_SET_FEATURE_ENABLED` (9).** Explorer skips `try_mft_snapshot` / helper when off and drops `FolderSizeServiceV1` MFT maps. Service unloads query LRU/aggregates. Disk indexes stay. SCM is not stopped.
- **Space MFT budget rows stay visible but inert** while MFT is off so users still see what turning it on would cost.

## Risks / Trade-offs

- [Journal volume indexes may remain in the service process] → Query LRU/aggregates unload immediately; live USN indexes stay so re-enable does not rebuild the journal. Documented as query-cache unload, not a full process stop.
- [UITEST opens View because cache used to live there] → Smoke script clicks Space; `SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS` can still land on View for other tests.
- [Desired plugin state is overwritten on MFT-off] → Matches “auto-disable”; restoring desired state would surprise users who turned plugins off on purpose.

## Migration Plan

Sessions without `mft_enabled` decode as enabled. No schema version bump. Rollback is reverting the change; stored `mft_enabled: false` is ignored by older builds because of `deny_unknown_fields` on `PersistedViewSettings` — **BREAKING for older binaries reading a new session file**. Mitigation: field is additive with default; shipping this build as the reader/writer pair is required, same as other recent view-settings fields (`search_engine`).
