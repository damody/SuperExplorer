## ADDED Requirements

### Requirement: Real thumbnails use the complete stacked visual region
In stacked icon views, the system MUST fit a successful real thumbnail into the complete realized item width, including the horizontal-padding span, and existing icon-region height while preserving source aspect ratio and displaying the complete source without cropping or distortion.

#### Scenario: Landscape thumbnail at maximum zoom
- **WHEN** a successful landscape thumbnail is presented in maximum extra-large-icon zoom
- **THEN** it grows until it reaches the item border's inner horizontal bounds without a padding-sized gutter whenever its aspect ratio permits, while retaining the complete image

#### Scenario: Portrait thumbnail at maximum zoom
- **WHEN** a successful portrait thumbnail is presented in maximum extra-large-icon zoom
- **THEN** it grows until it reaches the visual region's vertical bounds whenever its aspect ratio permits, while retaining the complete image

#### Scenario: Square thumbnail
- **WHEN** a successful square thumbnail is presented in a stacked icon view
- **THEN** it uses the largest aspect-preserving size inside the visual region and remains fully visible

#### Scenario: DPI-adjusted cell width
- **WHEN** DPI scaling or grid adjustment changes the realized item width
- **THEN** the thumbnail host equals the current realized cell width, does not subtract horizontal padding, and does not cross the selection border

### Requirement: Shell and fallback icons retain bounded icon geometry
The system MUST present folders, file-type Shell icons, failed-thumbnail fallbacks, and generic fallback icons inside the existing centered square icon host rather than stretching them across the item cell.

#### Scenario: Folder beside a real thumbnail
- **WHEN** a folder and a thumbnail-capable file are presented in the same stacked icon view
- **THEN** the file's successful thumbnail uses edge-fit geometry while the folder icon remains centered and bounded by the square icon host

#### Scenario: Thumbnail extraction fails
- **WHEN** thumbnail extraction fails and the current Shell icon remains visible
- **THEN** that Shell icon uses square icon geometry and is not treated as a real thumbnail

#### Scenario: Provenance is unavailable
- **WHEN** a visual texture has no trusted thumbnail provenance
- **THEN** the renderer conservatively uses square Shell-icon geometry

### Requirement: Thumbnail edge fit has automated regression coverage
The system SHALL provide deterministic geometry/provenance tests and a headful maximum-icon UTIT assertion for edge-fitted thumbnails.

#### Scenario: Focused unit coverage
- **WHEN** the focused `explorer-ui` regression tests run
- **THEN** landscape, portrait, square, DPI-adjusted width, Shell-icon, and failed-thumbnail geometry cases are verified

#### Scenario: Headful maximum-icon verification
- **WHEN** the registered icon-view UTIT scenario presents a real landscape thumbnail and a folder at maximum zoom
- **THEN** evidence confirms that the thumbnail reaches the horizontal item edges without a padding-sized gutter while the folder remains centered and bounded
