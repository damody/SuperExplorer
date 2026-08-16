# G4 status

Status: BLOCKED by the current isolated headful fixture, not accepted as passing.

The deterministic fixture, exact assertions (`Main code lines: Rust: 1,250` and `Code lines: 1325`), non-Name drag, Name rejection, and clean-restart persistence automation are implemented and registered in `uitest/manifest.json` as `dual-code-lines-reorder-headful`.

Runs and supersession lineage:

1. `evidence/headful/`: center drop did not reorder.
2. `evidence/headful-rerun-001/`: explicit left midpoint still crossed several columns and did not reorder.
3. `evidence/headful-rerun-002/`: adjacent dynamic-column midpoint reached the scenario, but both folder cells exposed `Limit`.
4. `evidence/headful-rerun-003/` through `headful-rerun-010/`: enabling the required File Count dependency failed because the isolated app's Details chooser could not be opened/materialized by the existing automation path.
5. Runner diagnostics on 2026-08-14 fixed the detached/175%-DPI chooser gesture by dispatching the header right-click to the target HWND. Run `target/uitest-runs/1786699407-2cca032aaab04fc482eb347a33bceee9/` reached the column drag instead of failing at chooser discovery.
6. Run `target/uitest-runs/1786699691-2f71cd83c488404c9efaff6660cfcdc5/` used physical cursor coordinates for the drag. It still failed because the detached runner did not establish GPUI pointer capture; the isolated debug app also lacked an exact MFT service result, so both folder cells remained `Limit`. No raw passing report or before/after/restart screenshot set exists.

Reviewed failure screenshot hashes:

- `evidence/headful/column-drag-failure.png`: `f81eefaf1d500f13e9376f62c139af841b10504d8765329b9aa16f5e1301df09`
- `evidence/headful-rerun-001/column-drag-failure.png`: `27a39eefae93e8746f622f5d624750b26cbd2100249ecb9634def2c512d12122`
- `evidence/headful-rerun-002/column-drag-failure.png`: `085f7d8e7e8f46371766ddec43ceaba74ca2fee5c3d74cb4a11d85eaa0e654a3`
- `target/uitest-runs/1786699691-2f71cd83c488404c9efaff6660cfcdc5/evidence/dual-code-lines-reorder-headful/column-drag-failure.png`: `d3af49e8d6fbfe415bc78ac4dc9c5e82941e2fd03da186248202d80057a023bf`

The last screenshot visibly shows Name first and both dynamic columns installed, but the folder cells are `Limit`; it is retained as failure evidence only.
