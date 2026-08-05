## 1. Popup and interaction state

- [x] 1.1 Add bounded single-line ellipsis geometry and full accessible labels to Extensions rows
- [x] 1.2 Add typed EXIF and bulk-folder panel states, Cancel, outside-action dismissal, and two-stage Escape
- [x] 1.3 Render both anchored panels with explicit, preview-labelled operation choices

## 2. Preview and host execution

- [x] 2.1 Implement bounded bulk-folder name generation, Windows-name validation, representative preview, and typed request creation
- [x] 2.2 Connect EXIF selection parsing, naming-choice preview, missing-metadata/collision validation, and typed request creation
- [x] 2.3 Revalidate confirmed requests and execute accepted create-directory or rename steps through serialized host-owned operations
- [x] 2.4 Preserve active-location/selection revalidation, refresh, partial result, and conservative undo behavior

## 3. Verification

- [x] 3.1 Add state, validation, and action-routing unit tests
- [x] 3.2 Add headful UITEST for overflow, both panels, Escape no-op, and confirmed filesystem results
- [x] 3.3 Register the case in the UITEST manifest and produce required screenshots/report
- [x] 3.4 Run targeted checks/tests, explorer-app build, and the registered UITEST case
