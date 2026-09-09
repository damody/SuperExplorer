## Purpose

Lets plugins declare a dependency on the host MFT feature, auto-disables them when MFT is off, and unloads MFT memory while keeping disk indexes.

## ADDED Requirements

### Requirement: Plugins declare host capability mft
A package feature MAY include the host capability `mft` in its capabilities list. The official folder-size visual column and Size Map view SHALL declare `mft`. Features that do not declare `mft` SHALL keep working when MFT is off.

#### Scenario: Folder size column declares mft
- **WHEN** the host loads `rust-folder-size-visual-column`
- **THEN** at least one of its features lists capability `mft`

#### Scenario: Code-lines plugin does not require MFT
- **WHEN** MFT is disabled and `rust-tokei-code-lines-column` is enabled
- **THEN** that plugin remains enabled

### Requirement: Disabling MFT auto-disables dependent plugins
When the Space draft turns MFT off, every listed extension whose package requires `mft` SHALL have its desired enabled flag set to false in that draft. Apply SHALL persist those disabled states. Re-enabling MFT SHALL NOT by itself re-enable those plugins.

#### Scenario: User turns MFT off while folder size is on
- **WHEN** folder size and Size Map are enabled and the user unchecks Enable MFT
- **THEN** both plugins appear disabled in the draft before Apply

#### Scenario: User turns MFT back on
- **WHEN** MFT is applied off and the user later enables MFT and applies
- **THEN** previously auto-disabled MFT plugins stay disabled until the user enables them

### Requirement: Enabling an MFT plugin while MFT is off requires confirmation
If the user tries to enable an MFT-dependent plugin while MFT is off, the host SHALL NOT enable it immediately. It SHALL show a confirmation that the plugin needs MFT and that enabling MFT increases memory use. Confirm SHALL enable MFT and the plugin in the draft. Cancel SHALL leave both unchanged.

#### Scenario: User confirms the MFT memory prompt
- **WHEN** MFT is off, the user checks an MFT-dependent plugin, and confirms
- **THEN** the draft has MFT on and that plugin enabled

#### Scenario: User cancels the MFT memory prompt
- **WHEN** MFT is off, the user checks an MFT-dependent plugin, and cancels
- **THEN** MFT stays off and the plugin stays disabled

### Requirement: Disabled MFT unloads memory and keeps disk indexes
After Apply with MFT off, the host SHALL stop issuing MFT folder-size and helper queries, SHALL drop in-process MFT indexes and aggregates, and SHALL ask MFT Service to unload its memory caches. Persisted `.mft.sqlite3` indexes SHALL remain. MFT budget rows on Space MAY stay visible but MUST be inert while MFT is off. After Apply with MFT on, queries and caches MAY resume.

#### Scenario: Apply with MFT off
- **WHEN** committed settings have MFT disabled
- **THEN** folder-size consumers do not query MFT Service and MFT memory usage reporters become unavailable or zero rather than keeping the previous working set

### Requirement: MFT search is unavailable while MFT is off
Search-engine availability SHALL treat the MFT engine as unsupported when MFT is disabled, even if a local index file exists. The MFT radio SHALL show a hint that MFT is turned off in Space settings. Apply SHALL keep the existing pick-a-supported-engine rule.

#### Scenario: MFT search selected then MFT is turned off
- **WHEN** the stored engine is MFT and the user disables MFT while another engine is available
- **THEN** Apply is rejected until the user picks a supported engine
