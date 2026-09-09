## Purpose

Gives Folder Options a dedicated Space tab for cache budgets and the MFT master switch so View stays about presentation.

## ADDED Requirements

### Requirement: Space tab owns cache and MFT budgets
Folder Options SHALL show tabs in this order: General, View, Space, Extensions. The Space page SHALL contain the MFT enable checkbox, cache usage and limits, MFT Service resource budgets, folder-size cache TTL, and Clear thumbnail cache. The View page SHALL keep folder-view apply/reset and the advanced presentation checkboxes, and SHALL NOT render those cache or MFT budget controls.

#### Scenario: Opening Space shows cache rows that used to live on View
- **WHEN** the user opens Folder Options and selects Space
- **THEN** cache usage/limit editors, MFT resource rows, TTL, and Clear thumbnail cache are on Space and are absent from View

#### Scenario: Tab labels follow the UI locale
- **WHEN** the UI locale is zh-TW
- **THEN** the Space tab label is 空間設定

### Requirement: MFT master switch defaults on and persists
View settings SHALL persist `mft_enabled` with serde default true so sessions written before this field still decode as enabled. Reset view settings SHALL restore MFT enabled. Apply and OK SHALL commit the draft switch with the rest of Folder Options.

#### Scenario: Legacy session has no mft_enabled field
- **WHEN** a stored session omits `mft_enabled`
- **THEN** runtime treats MFT as enabled

#### Scenario: User turns MFT off and confirms
- **WHEN** the user unchecks Enable MFT on Space and chooses Apply or OK
- **THEN** the committed settings have MFT disabled and the next session restore keeps it disabled
