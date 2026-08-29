# ExplorerPatcher behavioral-reference record

Recorded: 2026-08-30T00:16:34.0876900+08:00

- Repository: `https://github.com/valinet/ExplorerPatcher`
- Studied commit: `0a88a6e0ef6b1752fea36e581cffff1097e862b0`
- License: GPLv2, SHA-256 `189B1AF95D661151E054CEA10C91B3D754E4DE4D3FECFB074C1FB29476F7167B`
- Behavioral reference files: `ExplorerPatcher/dllmain.c`, `ExplorerPatcher/TwinUIPatches.cpp`, `ExplorerPatcher/symbols.h`.
- Behavioral facts retained: a legacy HMENU may receive immersive owner-draw before tracking; measure/draw messages require an owner procedure; styling is removed after tracking; capability can be disabled.

## Forbidden implementation inputs

SuperExplorer must not copy ExplorerPatcher source blocks, byte signatures, pattern tables, symbol-offset tables, binaries, resources, or assets. The implementation must use independently written Rust/Win32 contracts and repository-approved discovery seams. No ExplorerPatcher file is added as a build or runtime dependency.
