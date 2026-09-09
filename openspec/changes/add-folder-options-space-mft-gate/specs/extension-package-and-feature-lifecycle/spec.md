## ADDED Requirements

### Requirement: Host-gated MFT capability
A feature MAY declare the host capability `mft`. The host SHALL accept `mft` as a known host-gated capability. When the committed MFT feature is off, the host SHALL treat features that declared `mft` as not dispatchable even if their persisted desired state is enabled. Declaring `mft` SHALL NOT by itself reject a package.

#### Scenario: Feature declares mft and MFT is on
- **WHEN** a valid package feature lists `mft` and committed settings have MFT enabled
- **THEN** the host does not reject the package for that capability and the feature may dispatch

#### Scenario: Feature declares mft and MFT is off
- **WHEN** a loaded feature declared `mft` and committed settings have MFT disabled
- **THEN** the host does not dispatch that feature's contributions until MFT is enabled again
