## ADDED Requirements

### Requirement: Command bar exposes labeled Other and Extensions controls
The command bar SHALL render the existing More command as the visible label `其它` and SHALL render an `擴充功能` dropdown immediately to its right without changing the existing More menu commands.

#### Scenario: Full command bar layout
- **WHEN** the non-compact command bar is rendered
- **THEN** `其它` appears after `檢視` and `擴充功能` appears immediately after `其它`
- **AND** the existing More popup remains anchored to the `其它` control

#### Scenario: Accessible identities
- **WHEN** UI Automation inspects the command bar
- **THEN** both controls expose stable button identities and Traditional Chinese accessible labels

### Requirement: Extensions menu reflects TortoiseGit availability
The system SHALL discover TortoiseGit through a side-effect-free Windows adapter and SHALL expose `更新 TortoiseGit 狀態` only when a valid installation executable exists.

#### Scenario: TortoiseGit is installed
- **WHEN** a candidate Program Files path contains `TortoiseGit\bin\TortoiseGitProc.exe`
- **THEN** the Extensions menu contains an enabled `更新 TortoiseGit 狀態` command

#### Scenario: TortoiseGit is unavailable
- **WHEN** no valid candidate executable exists
- **THEN** the Extensions menu remains present and contains a disabled `沒有可用的擴充功能` item
- **AND** application startup succeeds

### Requirement: Command popups are mutually exclusive and keyboard operable
The Extensions popup MUST follow the existing command-bar popup close, focus, anchoring, and top-layer contracts.

#### Scenario: Opening Extensions closes another popup
- **WHEN** Sort, View, or Other is open and the user opens Extensions
- **THEN** the prior popup closes exactly once and only Extensions remains open

#### Scenario: Keyboard execution
- **WHEN** Extensions is open with TortoiseGit available and the user presses Enter or Space
- **THEN** the refresh command executes and the popup closes

#### Scenario: Keyboard cancellation
- **WHEN** Extensions is open and the user presses Escape
- **THEN** the popup closes without changing icon generations

### Requirement: TortoiseGit refresh re-queries active overlay icons
Executing `更新 TortoiseGit 狀態` SHALL invalidate overlay-dependent icon state for the active folder and SHALL request new Shell icon pixels without changing folder content or navigation state.

#### Scenario: Refresh with cached visible icons
- **WHEN** the active folder has cached visible icons and the refresh command executes
- **THEN** the overlay epoch becomes newer than every prior global or per-item overlay epoch
- **AND** overlay-dependent positive, negative, pending, and thumbnail presentation state is invalidated
- **AND** current active-folder icon requests are re-submitted through the Shell service

#### Scenario: Refresh preserves unrelated state
- **WHEN** the refresh command executes
- **THEN** association generation, shared base icons, directory history, selection, view mode, and scroll offset remain unchanged

#### Scenario: Stale icon completion
- **WHEN** an icon request from before the refresh completes afterward
- **THEN** its stale overlay generation MUST NOT replace the current presentation

### Requirement: Installed-environment UITEST proves the interaction
The UITEST suite SHALL cover the Extensions popup and SHALL use an explicit prerequisite or disabled-state oracle when TortoiseGit is unavailable.

#### Scenario: Installed TortoiseGit headful run
- **WHEN** TortoiseGit is installed and the headful interop case opens a real Git fixture
- **THEN** UI Automation can open `擴充功能`, invoke `更新 TortoiseGit 狀態`, and observe a fresh overlay-icon request cycle

#### Scenario: Unavailable TortoiseGit headful run
- **WHEN** TortoiseGit is not installed
- **THEN** the case records a truthful prerequisite skip or verifies the disabled placeholder instead of reporting a false failure
