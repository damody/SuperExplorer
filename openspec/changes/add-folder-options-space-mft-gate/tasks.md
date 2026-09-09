## 1. Settings model

- [x] 1.1 Add `mft_enabled` default true on `ViewSettings` and `PersistedViewSettings`; cover default, round-trip, and legacy omit
- [x] 1.2 Add `SearchEngineFacts.mft_feature_enabled` and treat MFT as unavailable when it is false; cover remote and feature-off cases
- [x] 1.3 Export host capability constant `mft`

## 2. Plugin gate

- [x] 2.1 Add `requires_mft` on extension options; official folder-size column and Size Map require it
- [x] 2.2 Turning MFT off in the draft disables those plugins; turning MFT on does not re-enable them
- [x] 2.3 Enabling an MFT plugin while MFT is off opens a confirmation; confirm enables both, cancel leaves both unchanged
- [x] 2.4 Declare `mft` on folder-size and Size Map plugin features

## 3. Folder Options UI

- [x] 3.1 Add `FolderOptionsPage::Space` and Space tab (General, View, Space, Extensions)
- [x] 3.2 Move cache/MFT/TTL/clear-thumbnail controls to Space; View keeps presentation options
- [x] 3.3 Add MFT checkbox and help; MFT budget editors inert while MFT is off
- [x] 3.4 Add MFT-plugin confirmation overlay and i18n in every locale
- [x] 3.5 Disable MFT search radio with Space-off hint when MFT is off

## 4. Runtime unload

- [x] 4.1 Skip MFT folder-size and helper queries when MFT is off and drop in-process MFT maps
- [x] 4.2 Add MFT Service `SET_FEATURE_ENABLED` pipe request and unload query caches
- [x] 4.3 Apply/OK propagate `mft_enabled` to search facts and the visual-column runtime

## 5. Verification

- [x] 5.1 Unit tests for settings, draft gate, chrome contracts, and i18n catalog completeness
- [x] 5.2 Update Folder Options UITEST smoke to use the Space tab for cache/MFT rows
