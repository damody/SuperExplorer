# MFT-only folder-size design

## Goal

Make recursive folder size a Host-owned capability with one fast data path. Every consumer—including the built-in Size column, the Folder size extension column, and Size Map—reads the same Host cache. A cache miss may be populated only by SuperExplorer MFT Windows Service.

## Required behavior

- Files keep using their ordinary file length in the built-in Size column.
- Folders use the recursive byte count returned by the shared Host folder-size service.
- The built-in Size column displays the folder count when Host cache or MFT Service supplies it, even if the Folder size extension is disabled.
- Folder size, built-in Size, and Size Map share the same cache identity and invalidation policy.
- A cached value remains valid while the folder modification timestamp and cache schema match.
- On a cache miss, the Host requests MFT Service data and admits a successful result into the Host cache.
- Everything and recursive directory walking are not permitted as folder-size fallbacks.
- When MFT Service is unavailable, the volume is unsupported, or no complete result is available, folder-size cells remain blank. The status bar reports the unavailable state rather than starting a slow calculation.
- ZIP files and other Shell namespace containers are not treated as file-system folders.

## Architecture and data flow

`FolderSizeServiceV1` is the sole folder-size provider. It first checks the persistent Host cache using canonical path identity, modification timestamp, and schema. On a miss it queries the per-volume aggregate produced by `SuperExplorerMft`. A successful aggregate is cached and published to all active consumers.

The UI does not perform file-system measurement. It requests folder-size values independently of extension visibility when the built-in Size column needs folder values. Both Size and Folder size render from the same context-scoped result map. Size Map asks the same Host service for its snapshot instead of invoking its own scanner.

## Sorting and presentation

- File rows sort by ordinary file length.
- Folder rows with a known recursive size sort by that value.
- Rows without a known value use the existing missing-value ordering.
- Folder-size values use the existing Explorer byte formatting.
- The status bar distinguishes `Host cache`, `MFT service`, and `MFT unavailable`.

## Failure handling

Service errors, stale or malformed indexes, non-NTFS volumes, and access failures produce an unavailable result. They must not trigger Everything or recursive traversal. A later refresh or service index update may retry the MFT path.

## Verification

Unit and UTIT coverage must prove:

- the built-in Size column shows MFT/Host-cache folder values while retaining file sizes;
- disabling the Folder size extension does not disable the built-in Size behavior;
- multiple consumers reuse one Host cache result;
- timestamp changes invalidate cached folder size;
- ZIP/Shell containers remain blank;
- MFT failure never invokes Everything or recursive scanning;
- sorting uses recursive folder bytes when present;
- installed-build UI evidence shows populated folder values and the active backend status.
