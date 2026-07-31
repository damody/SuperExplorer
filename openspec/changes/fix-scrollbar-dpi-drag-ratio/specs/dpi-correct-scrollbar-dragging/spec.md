## ADDED Requirements

### Requirement: DPI-correct native scrollbar dragging
The system SHALL convert native captured pointer coordinates from physical client pixels to GPUI
logical pixels exactly once before computing a scrollbar target offset.

#### Scenario: Vertical drag at scaled DPI
- **WHEN** the file-view or navigation vertical scrollbar receives a native captured Y coordinate at
  any supported scale from 100% through 200%
- **THEN** its thumb movement matches the equivalent logical pointer movement without a DPI multiplier

#### Scenario: Horizontal drag at scaled DPI
- **WHEN** the Details horizontal scrollbar receives a native captured X coordinate at any supported
  scale from 100% through 200%
- **THEN** its thumb movement matches the equivalent logical pointer movement without a DPI multiplier

#### Scenario: Native capture is unavailable
- **WHEN** no valid native captured coordinate or scale factor is available during a drag
- **THEN** the system uses the already-logical GPUI event coordinate without applying DPI scaling

### Requirement: Existing scrollbar interaction contracts remain intact
The system MUST preserve grab offset, endpoint clamping, track paging, native capture outside the
window, terminal capture release, and scrollbar visibility behavior.

#### Scenario: Pointer leaves the window during thumb drag
- **WHEN** a captured scrollbar drag moves outside the application HWND
- **THEN** scrolling continues with the corrected logical coordinate until the pointer is released

### Requirement: Quantitative multi-file scrollbar regression
The UITEST suite SHALL exercise the custom scrollbars on a deterministic folder containing at least
240 files and SHALL compare observed movement with geometry-derived expected movement.

#### Scenario: Multi-file vertical scrollbar drag
- **WHEN** UITEST drags the file-view vertical thumb by a known physical cursor distance
- **THEN** the observed RangeValue change matches the DPI-normalized expected change within documented
  pixel-rounding tolerance

#### Scenario: Every custom scrollbar kind is covered
- **WHEN** the scrollbar headful matrix runs
- **THEN** it quantitatively validates file-view vertical, navigation vertical, and Details horizontal
  drag ratios while retaining capture and release assertions
