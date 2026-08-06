## MODIFIED Requirements

### Requirement: Feature and capability binding
Every registrar contribution SHALL reference one manifest feature ID, and each feature SHALL declare all capabilities and host-data requirements used by its callbacks. Runtime authority handles SHALL bind package, feature, interface, package incarnation, capability, authorized resource root and relevant generations. Host snapshot requirements such as `folder.aggregate` and `folder.tree` SHALL authorize only host-projected data and SHALL NOT expose arbitrary filesystem access or create dependencies on another plugin's effective state.

#### Scenario: Renderer requests shared folder data
- **WHEN** a valid Size Map feature declares `folder.tree`
- **THEN** the host may supply the authorized current-generation tree while Folder Size remains disabled

#### Scenario: Undeclared folder data is requested
- **WHEN** a contribution requests a folder snapshot not declared by its feature
- **THEN** validation or dispatch rejects the request without starting filesystem work
