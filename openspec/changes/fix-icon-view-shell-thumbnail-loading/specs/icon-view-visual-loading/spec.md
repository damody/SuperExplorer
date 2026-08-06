## ADDED Requirements

### Requirement: View modes select Explorer-compatible visual sources
The system SHALL prefer content thumbnails for eligible image and video files in extra-large, large, and medium icon views, and SHALL use Windows Shell icons without content-thumbnail requests in small-icon and tile views.

#### Scenario: Thumbnail-capable icon view
- **WHEN** an eligible image or video is realized in extra-large, large, or medium icon view
- **THEN** the system presents a correct current-size Shell icon while requesting a content thumbnail at least as large as the current presentation size

#### Scenario: Shell-only view
- **WHEN** an item is realized in small-icon or tile view
- **THEN** the system requests and presents the correct Windows Shell icon and does not request a content thumbnail for that view

### Requirement: Thumbnail replacement preserves a valid fallback
The system MUST replace a Shell fallback only with a successful thumbnail that matches the active visual demand.

#### Scenario: Matching thumbnail succeeds
- **WHEN** a matching content thumbnail completes successfully for the active item and view
- **THEN** the thumbnail replaces the Shell icon presentation

#### Scenario: Thumbnail fails or becomes obsolete
- **WHEN** thumbnail extraction fails or its request no longer matches the active folder, size, DPI, theme, association, or overlay context
- **THEN** the correct Shell icon remains visible and the obsolete result does not alter the active presentation

### Requirement: Work classes have independent bounded scheduling
The system SHALL bound visible Shell icon work and visible content-thumbnail work independently.

#### Scenario: Obsolete Shell work exists
- **WHEN** pending Shell icon work from a previous size or display context exists and the user changes to a thumbnail-capable view
- **THEN** that obsolete work does not consume the current thumbnail request budget or prevent realized eligible items from being scheduled

#### Scenario: Thumbnail work is saturated
- **WHEN** the content-thumbnail budget is fully occupied
- **THEN** realized items can still receive bounded Shell icon requests and a valid fallback presentation

### Requirement: Visual results are admitted only for current demand
The system MUST reject late visual results that cannot satisfy the current tab, folder generation, view size, DPI/theme, association generation, or overlay generation.

#### Scenario: Rapid view switch
- **WHEN** the user switches view size before an earlier icon or thumbnail request completes
- **THEN** the earlier result cannot overwrite the presentation for the new view size

#### Scenario: Folder generation changes
- **WHEN** navigation changes the active folder generation before a visual request completes
- **THEN** the old result is retained only as a compatible cache entry and is not admitted into the new folder presentation

### Requirement: Compatible completed visuals are reusable
The system SHALL reuse completed Shell icon and thumbnail cache entries when their identity and display context remain compatible.

#### Scenario: Return to a previous compatible view
- **WHEN** the user returns to a previously visited folder and view size without changing relevant source or display generations
- **THEN** compatible completed visuals are presented from cache without unconditional recomputation

### Requirement: Icon-view behavior is covered by automated tests
The system SHALL provide deterministic Rust coverage and a headful UTIT scenario for the five requested view modes.

#### Scenario: Unit regression coverage
- **WHEN** the focused Rust test suite runs
- **THEN** it verifies mode policy, independent budgets, stale-result rejection, fallback preservation, and compatible cache reuse

#### Scenario: Headful Windows verification
- **WHEN** the icon-view UTIT scenario runs against a folder containing a directory, a plain file, a bitmap image, and a video
- **THEN** it switches through extra-large, large, medium, small, and tile views, scrolls to newly realized items, and records evidence that correct icons or thumbnails appear without generic fallback blocks

### Requirement: Maximum icon zoom remains stable within a configurable cache budget
The system SHALL default the icon/thumbnail presentation cache to 128 MiB, SHALL offer 64, 128, 256, 512 MiB and 1 GiB choices in Folder Options, and MUST prevent maximum-size prefetch from evicting the same visible working set that requested it.

#### Scenario: Maximum icon zoom
- **WHEN** extra-large icons are zoomed to the maximum size at high DPI
- **THEN** prefetch is limited by both the realized range and half of the configured cache budget, leaving the visible icons stable instead of repeatedly unloading and reloading

#### Scenario: User changes cache budget
- **WHEN** the user selects a cache preset in Folder Options and applies it
- **THEN** the bounded normalized value is persisted and used for subsequent icon admission, with values above 1 GiB clamped to 1 GiB

### Requirement: Maximum folder zoom prefers compatible Shell pixels
When the exact current-size folder Shell icon is unavailable, the system MUST present the largest compatible same-folder or shared-folder Shell texture enlarged into the current icon box before using the generic yellow fallback.

#### Scenario: Exact maximum-size folder icon is unavailable
- **WHEN** a visible folder at maximum zoom has no exact-size Shell texture but a smaller texture exists for the current DPI, theme, association generation, and overlay generation
- **THEN** the largest compatible texture is enlarged with preserved aspect ratio and the generic yellow fallback is not displayed

#### Scenario: Shared base request fails
- **WHEN** the exact-size shared folder base request fails for a visible real folder
- **THEN** the folder class is not permanently disabled, a bounded real-item request remains eligible, and any compatible Shell result can replace the temporary generic fallback

#### Scenario: Display context is incompatible
- **WHEN** only cached folder textures from another DPI, theme, association generation, or overlay generation exist
- **THEN** those textures are not presented and the current-context request remains eligible
