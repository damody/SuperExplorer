## ADDED Requirements

### Requirement: This PC devices and drives view
Activating This PC SHALL show an Explorer-like grouped devices-and-drives surface instead of an ordinary folder listing.

#### Scenario: Fixed drives are available
- **WHEN** This PC enumeration returns fixed drives
- **THEN** each drive shows its volume label, drive letter, icon, capacity bar, available bytes, and total bytes under Devices and drives

#### Scenario: Drive status is unavailable
- **WHEN** a removable, network, encrypted, or inaccessible drive cannot report capacity
- **THEN** it remains visible with a truthful unavailable or disconnected state and no fabricated capacity

#### Scenario: Space is low
- **WHEN** a drive crosses the configured low-free-space threshold
- **THEN** its capacity bar uses the warning presentation while its numeric values remain exact

### Requirement: This PC interaction parity
Drive tiles SHALL use the same stable selection, keyboard, context-menu, drag/drop, and activation contracts as other file items.

#### Scenario: Drive is activated
- **WHEN** the user double-clicks a drive or presses Enter on its selected tile
- **THEN** the active tab navigates to that drive root

#### Scenario: View refreshes
- **WHEN** the user refreshes This PC or device state changes
- **THEN** the service republishes owned drive metadata and the capacity presentation updates without UI-thread disk queries
