## 1. Preserve visual provenance

- [x] 1.1 Add a file-view presentation value that carries the render texture and trusted thumbnail-versus-Shell provenance.
- [x] 1.2 Mark successful thumbnail admissions as thumbnails and all Shell, compatible-size, base, failed-thumbnail, and uncertain paths as Shell visuals.
- [x] 1.3 Add focused tests proving thumbnail provenance survives the render snapshot and failed or uncertain visuals remain Shell-classified.

## 2. Fit thumbnails to the stacked visual region

- [x] 2.1 Add bounded geometry helpers that derive thumbnail width from the current spatial cell minus horizontal padding while retaining the existing icon-region height.
- [x] 2.2 Render real thumbnails with aspect-fit inside the full stacked visual region and retain the square icon host for Shell/fallback visuals and non-stacked modes.
- [x] 2.3 Add focused landscape, portrait, square, DPI-adjusted-width, Shell-icon, and invalid-source geometry tests.

## 3. Verify Explorer-compatible behavior

- [x] 3.1 Extend the maximum-icon UTIT assertion to require a real landscape thumbnail to approach the horizontal cell edges while a folder remains centered and bounded.
- [x] 3.2 Run focused tests, `cargo fmt --all -- --check`, `cargo check -p explorer-ui`, `cargo build -p explorer-app`, UTIT manifest/script parsing, strict OpenSpec validation, and focused diff review; record evidence.

## 4. Remove the residual horizontal gutter

- [x] 4.1 Change stacked real-thumbnail geometry to use the complete realized cell width without subtracting horizontal padding, while preserving the square Shell-icon host.
- [x] 4.2 Update focused geometry tests and maximum-icon UTIT thresholds to reject a padding-sized horizontal gutter.
- [x] 4.3 Run the registered UTIT regression, formatting, focused compile/tests, app build, manifest/script parsing, strict OpenSpec validation, and record refinement evidence.
