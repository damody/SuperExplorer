# Folder Size Measurement Migration Map

| Concern | Current Folder Size column | Current Size Map | Shared destination |
|---|---|---|---|
| Request identity | `FolderSizeRequestV1 { context, item_id, path }` plus `FolderSizeWorkIdentityV1` | `SizeMapRequestV1`/`PendingSizeMapWorkV1` keyed by tab/location/refresh | `SnapshotKey { volume, canonical_root, semantic_policy, refresh_generation }` plus consumer lease |
| Result identity | `FolderSizeResultV1 { context, item_id }` | `SizeMapTreeResultV1` with view/snapshot generation | One `FolderSnapshot` revision projected to aggregate rows or tree nodes |
| Physical work | Extension `measure_folder_size` callback | Application breadth scan or MFT projection | Host backend adapter selected once per compatible key |
| Memory cache | `HostExtensionColumnCacheV1` and UI snapshot maps | Size Map coordinator state | Service-owned bounded LRU pinned by leases |
| Disk cache | Official fixture-owned cache | MFT helper index/temp path | Versioned host snapshot/backend records |
| Cancellation | Folder-size pending epoch and request replacement | Size Map request cancellation/generation | Final-consumer cancellation token and stale-generation rejection |
| Refresh invalidation | File metadata cache key/manual generation | F5/view/location generation | Watcher/manual generation plus optional continuous USN checkpoint |
| Reparse policy | Fixture traversal-specific | Breadth/MFT-specific | Canonical-root containment; represent but never recurse directory reparse points |
| Rendering | GPUI host calls extension render plan | Extension size-map layout plan | Unchanged data-only rendering over host projections |
| Feature disable | Column runtime detached | View fallback/cancel | Release only that consumer lease; keep work for remaining consumers |

## Migration order

1. Introduce normalized types and recursive reference adapter alongside current paths.
2. Add shared coalescing/cache/backend selection.
3. Route Size Map tree demand through the service.
4. Route Folder Size aggregate demand through the same snapshot.
5. Remove official extension measurement and retain a bounded legacy adapter only for local compatibility.
