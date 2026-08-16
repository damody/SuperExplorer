# 2026-08-14 remaining acceptance blockers

The five unchecked tasks remain intentionally incomplete; none can be truthfully replaced by a simulated or single-DPI result.

- 8.14, 9.52, 9.55, and 11.13 require physical 100/125/150/175/200% and mixed-DPI monitor sessions. The current interactive desktop exposes only `\\.\DISPLAY1` (`2194x1234`, work area `2194x1153`) at the existing 175% session. Existing typed scaling tests and mismatch captures do not satisfy these physical matrices.
- 10.8 was retried against the rebuilt production binary with:
  `powershell.exe -Sta -NoProfile -ExecutionPolicy Bypass -File scripts/smoke_explorer_drag_interop.ps1 -Direction both -ExplorerScenario all -OutputDirectory target/explorer-interop-evidence/20260814-match-parity-retry -SkipBuild`.
  The run reached the real OLE source terminal with `performed_effect=0` and then failed the disk oracle because `fixture/explorer-target/app-left-move.txt` was not created. This reproduces the documented synthetic-input limitation instead of proving a physical Explorer drop.
- Retry artifacts include `failure.txt`, `window-layout.json`, `input-state.log`, `desktop-before-first-drop.png`, and the isolated fixture under `target/explorer-interop-evidence/20260814-match-parity-retry`.

No task checkbox was changed. Completion requires either additional physical DPI/mixed-monitor sessions or an input environment capable of completing real cross-process Explorer OLE drops.
