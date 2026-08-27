//! Host-owned folder aggregate/tree snapshots shared by every consumer.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

pub(crate) const SNAPSHOT_SCHEMA_V2: u32 = 2;
pub(crate) const MAX_SNAPSHOT_BYTES_V1: u64 = 64 * 1024 * 1024;
pub(crate) const DEFAULT_MAX_NODES_V1: usize = 1_000_000;
pub(crate) const MAX_DIAGNOSTIC_BYTES_V1: usize = 512;
const PERSISTENT_RECORD_SCHEMA_V1: u32 = 1;
const SEMANTIC_POLICY_VERSION_V1: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum SnapshotMethodV1 {
    Recursive,
    Mft,
    Everything,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum SnapshotStatusV1 {
    Complete,
    Partial,
    Cancelled,
    Unavailable,
    ResourceLimited,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum SnapshotNodeKindV1 {
    Directory,
    File,
    ReparsePoint,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct SnapshotNodeIdV1(pub u64);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct FolderSnapshotNodeV1 {
    pub id: SnapshotNodeIdV1,
    pub parent: Option<SnapshotNodeIdV1>,
    pub name: String,
    pub kind: SnapshotNodeKindV1,
    pub direct_bytes: u64,
    pub recursive_bytes: u64,
    pub status: SnapshotStatusV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct FolderAggregateSnapshotV1 {
    pub recursive_bytes: u64,
    pub direct_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
    pub status: SnapshotStatusV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct FolderSnapshotV1 {
    pub schema: u32,
    pub root_id: SnapshotNodeIdV1,
    pub refresh_generation: u64,
    #[serde(default)]
    pub mft_generation: Option<u64>,
    pub method: SnapshotMethodV1,
    pub status: SnapshotStatusV1,
    pub diagnostic: Option<String>,
    pub aggregate: FolderAggregateSnapshotV1,
    pub nodes: Vec<FolderSnapshotNodeV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PersistentSnapshotKeyV1 {
    root_identity: u64,
    modified_stamp: u128,
    semantic_policy_version: u32,
    backend_data_version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PersistentSnapshotRecordV1 {
    schema: u32,
    key: PersistentSnapshotKeyV1,
    snapshot: FolderSnapshotV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FolderSnapshotDeltaV1 {
    Add(FolderSnapshotNodeV1),
    Update(FolderSnapshotNodeV1),
    #[expect(
        dead_code,
        reason = "contract owned by openspec centralize-shared-folder-size-service; remove after bounded remove-delta emitter and consumer wiring exists"
    )]
    Remove(SnapshotNodeIdV1),
    SubtreeComplete(SnapshotNodeIdV1),
    ScanComplete(SnapshotStatusV1),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SnapshotLeaseKeyV1 {
    pub canonical_root: PathBuf,
    pub refresh_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecursiveSnapshotPolicyV1 {
    pub max_nodes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedSnapshotEntryV1 {
    pub path: PathBuf,
    pub bytes: u64,
    pub is_directory: bool,
}

impl Default for RecursiveSnapshotPolicyV1 {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_MAX_NODES_V1,
        }
    }
}
pub(crate) fn scan_recursive_reference(
    root: &Path,
    refresh_generation: u64,
    policy: RecursiveSnapshotPolicyV1,
    cancelled: impl Fn() -> bool,
) -> Result<FolderSnapshotV1, String> {
    scan_recursive_reference_with_deltas(root, refresh_generation, policy, cancelled, |_| {})
}

pub(crate) fn scan_recursive_reference_with_deltas(
    root: &Path,
    refresh_generation: u64,
    policy: RecursiveSnapshotPolicyV1,
    cancelled: impl Fn() -> bool,
    mut emit: impl FnMut(FolderSnapshotDeltaV1),
) -> Result<FolderSnapshotV1, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "folder snapshot root is unavailable".to_owned())?;
    let metadata = fs::symlink_metadata(&canonical_root)
        .map_err(|_| "folder snapshot root is unavailable".to_owned())?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err("folder snapshot root is not a traversable directory".to_owned());
    }

    let root_id = stable_node_id(Path::new(""));
    let mut nodes = vec![FolderSnapshotNodeV1 {
        id: root_id,
        parent: None,
        name: canonical_root.file_name().map_or_else(
            || canonical_root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        kind: SnapshotNodeKindV1::Directory,
        direct_bytes: 0,
        recursive_bytes: 0,
        status: SnapshotStatusV1::Complete,
    }];
    emit(FolderSnapshotDeltaV1::Add(nodes[0].clone()));
    let mut indices = HashMap::from([(root_id, 0_usize)]);
    let mut queue = VecDeque::from([(canonical_root.clone(), root_id)]);
    let mut aggregate = FolderAggregateSnapshotV1 {
        recursive_bytes: 0,
        direct_bytes: 0,
        file_count: 0,
        directory_count: 1,
        status: SnapshotStatusV1::Complete,
    };
    let mut diagnostic = None;

    while let Some((directory, directory_id)) = queue.pop_front() {
        if cancelled() {
            aggregate.status = SnapshotStatusV1::Cancelled;
            let snapshot = finish_snapshot(
                root_id,
                refresh_generation,
                aggregate.status,
                Some("folder snapshot cancelled".to_owned()),
                aggregate,
                nodes,
            );
            emit(FolderSnapshotDeltaV1::ScanComplete(snapshot.status));
            return Ok(snapshot);
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                mark_partial(&mut nodes, &indices, directory_id);
                if let Some(index) = indices.get(&directory_id).copied() {
                    emit(FolderSnapshotDeltaV1::Update(nodes[index].clone()));
                }
                aggregate.status = SnapshotStatusV1::Partial;
                diagnostic
                    .get_or_insert_with(|| "one or more subtrees were inaccessible".to_owned());
                continue;
            }
        };
        for entry in entries.flatten() {
            if nodes.len() >= policy.max_nodes {
                aggregate.status = SnapshotStatusV1::ResourceLimited;
                diagnostic = Some("folder snapshot node limit reached".to_owned());
                let snapshot = finish_snapshot(
                    root_id,
                    refresh_generation,
                    aggregate.status,
                    diagnostic,
                    aggregate,
                    nodes,
                );
                emit(FolderSnapshotDeltaV1::ScanComplete(snapshot.status));
                return Ok(snapshot);
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                mark_partial(&mut nodes, &indices, directory_id);
                aggregate.status = SnapshotStatusV1::Partial;
                continue;
            };
            let relative = path.strip_prefix(&canonical_root).unwrap_or(&path);
            let id = stable_node_id(relative);
            let reparse = is_reparse_point(&metadata);
            let (kind, bytes) = if reparse {
                (SnapshotNodeKindV1::ReparsePoint, 0)
            } else if metadata.is_dir() {
                (SnapshotNodeKindV1::Directory, 0)
            } else if metadata.is_file() {
                (SnapshotNodeKindV1::File, metadata.len())
            } else {
                (SnapshotNodeKindV1::Other, 0)
            };
            let index = nodes.len();
            nodes.push(FolderSnapshotNodeV1 {
                id,
                parent: Some(directory_id),
                name: entry.file_name().to_string_lossy().into_owned(),
                kind,
                direct_bytes: bytes,
                recursive_bytes: bytes,
                status: SnapshotStatusV1::Complete,
            });
            emit(FolderSnapshotDeltaV1::Add(nodes[index].clone()));
            indices.insert(id, index);
            if kind == SnapshotNodeKindV1::Directory {
                aggregate.directory_count = aggregate.directory_count.saturating_add(1);
                queue.push_back((path, id));
            } else if kind == SnapshotNodeKindV1::File {
                aggregate.file_count = aggregate.file_count.saturating_add(1);
                aggregate.recursive_bytes = aggregate.recursive_bytes.saturating_add(bytes);
                if directory_id == root_id {
                    aggregate.direct_bytes = aggregate.direct_bytes.saturating_add(bytes);
                }
            }
        }
    }

    for index in (0..nodes.len()).rev() {
        let Some(parent) = nodes[index].parent else {
            continue;
        };
        let bytes = nodes[index].recursive_bytes;
        if let Some(parent_index) = indices.get(&parent).copied() {
            nodes[parent_index].recursive_bytes =
                nodes[parent_index].recursive_bytes.saturating_add(bytes);
            if nodes[index].status != SnapshotStatusV1::Complete {
                nodes[parent_index].status = SnapshotStatusV1::Partial;
            }
            emit(FolderSnapshotDeltaV1::Update(nodes[parent_index].clone()));
        }
        if nodes[index].kind == SnapshotNodeKindV1::Directory {
            emit(FolderSnapshotDeltaV1::SubtreeComplete(nodes[index].id));
        }
    }
    aggregate.recursive_bytes = nodes[0].recursive_bytes;
    aggregate.status = nodes[0].status;
    emit(FolderSnapshotDeltaV1::SubtreeComplete(root_id));
    let snapshot = finish_snapshot(
        root_id,
        refresh_generation,
        aggregate.status,
        diagnostic,
        aggregate,
        nodes,
    );
    emit(FolderSnapshotDeltaV1::ScanComplete(snapshot.status));
    Ok(snapshot)
}

pub(crate) fn snapshot_from_indexed_entries(
    root: &Path,
    refresh_generation: u64,
    method: SnapshotMethodV1,
    mut entries: Vec<IndexedSnapshotEntryV1>,
) -> Result<FolderSnapshotV1, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "indexed snapshot root is unavailable".to_owned())?;
    let lexical_root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| "indexed snapshot root is unavailable".to_owned())?
            .join(root)
    };
    entries.sort_by_key(|entry| entry.path.components().count());
    if entries.len().saturating_add(1) > DEFAULT_MAX_NODES_V1 {
        return Err("indexed snapshot exceeds node limit".to_owned());
    }
    let root_id = stable_node_id(Path::new(""));
    let mut nodes = vec![FolderSnapshotNodeV1 {
        id: root_id,
        parent: None,
        name: canonical_root.file_name().map_or_else(
            || canonical_root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        kind: SnapshotNodeKindV1::Directory,
        direct_bytes: 0,
        recursive_bytes: 0,
        status: SnapshotStatusV1::Complete,
    }];
    let mut path_ids = HashMap::from([(canonical_root.clone(), root_id)]);
    let mut indices = HashMap::from([(root_id, 0_usize)]);
    let mut aggregate = FolderAggregateSnapshotV1 {
        recursive_bytes: 0,
        direct_bytes: 0,
        file_count: 0,
        directory_count: 1,
        status: SnapshotStatusV1::Complete,
    };
    for entry in entries {
        let indexed_path = if entry.path.starts_with(&canonical_root) {
            entry.path
        } else if let Ok(relative) = entry.path.strip_prefix(&lexical_root) {
            canonical_root.join(relative)
        } else {
            return Err("indexed snapshot path escaped the root".to_owned());
        };
        if !indexed_path.starts_with(&canonical_root) || indexed_path == canonical_root {
            return Err("indexed snapshot path escaped the root".to_owned());
        }
        let metadata = fs::symlink_metadata(&indexed_path)
            .map_err(|_| "indexed snapshot contains a stale path".to_owned())?;
        if is_reparse_point(&metadata) {
            return Err("indexed snapshot contains a reparse point".to_owned());
        }
        if metadata.is_dir() != entry.is_directory
            || (!entry.is_directory && metadata.len() != entry.bytes)
        {
            return Err("indexed snapshot metadata mismatch".to_owned());
        }
        let parent_path = indexed_path
            .parent()
            .ok_or_else(|| "indexed snapshot parent is missing".to_owned())?;
        let parent = path_ids
            .get(parent_path)
            .copied()
            .ok_or_else(|| "indexed snapshot parent is missing".to_owned())?;
        let relative = indexed_path
            .strip_prefix(&canonical_root)
            .map_err(|_| "indexed snapshot path escaped the root".to_owned())?;
        let id = stable_node_id(relative);
        let bytes = if entry.is_directory { 0 } else { entry.bytes };
        indices.insert(id, nodes.len());
        path_ids.insert(indexed_path.clone(), id);
        nodes.push(FolderSnapshotNodeV1 {
            id,
            parent: Some(parent),
            name: indexed_path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            kind: if entry.is_directory {
                aggregate.directory_count = aggregate.directory_count.saturating_add(1);
                SnapshotNodeKindV1::Directory
            } else {
                aggregate.file_count = aggregate.file_count.saturating_add(1);
                SnapshotNodeKindV1::File
            },
            direct_bytes: bytes,
            recursive_bytes: bytes,
            status: SnapshotStatusV1::Complete,
        });
        if !entry.is_directory && parent == root_id {
            aggregate.direct_bytes = aggregate.direct_bytes.saturating_add(bytes);
        }
    }
    for index in (0..nodes.len()).rev() {
        let Some(parent) = nodes[index].parent else {
            continue;
        };
        let bytes = nodes[index].recursive_bytes;
        let parent_index = indices[&parent];
        nodes[parent_index].recursive_bytes =
            nodes[parent_index].recursive_bytes.saturating_add(bytes);
    }
    aggregate.recursive_bytes = nodes[0].recursive_bytes;
    Ok(FolderSnapshotV1 {
        schema: SNAPSHOT_SCHEMA_V2,
        root_id,
        refresh_generation,
        mft_generation: None,
        method,
        status: SnapshotStatusV1::Complete,
        diagnostic: None,
        aggregate,
        nodes,
    })
}

fn finish_snapshot(
    root_id: SnapshotNodeIdV1,
    refresh_generation: u64,
    status: SnapshotStatusV1,
    diagnostic: Option<String>,
    aggregate: FolderAggregateSnapshotV1,
    nodes: Vec<FolderSnapshotNodeV1>,
) -> FolderSnapshotV1 {
    FolderSnapshotV1 {
        schema: SNAPSHOT_SCHEMA_V2,
        root_id,
        refresh_generation,
        mft_generation: None,
        method: SnapshotMethodV1::Recursive,
        status,
        diagnostic: diagnostic.map(|value| truncate_diagnostic(&value)),
        aggregate,
        nodes,
    }
}

fn stable_node_id(relative: &Path) -> SnapshotNodeIdV1 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    relative
        .to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    SnapshotNodeIdV1(hasher.finish().max(1))
}

fn mark_partial(
    nodes: &mut [FolderSnapshotNodeV1],
    indices: &HashMap<SnapshotNodeIdV1, usize>,
    id: SnapshotNodeIdV1,
) {
    if let Some(index) = indices.get(&id).copied() {
        nodes[index].status = SnapshotStatusV1::Partial;
    }
}

fn truncate_diagnostic(value: &str) -> String {
    value.chars().take(MAX_DIAGNOSTIC_BYTES_V1).collect()
}

fn try_everything_snapshot(
    _root: &Path,
    _refresh_generation: u64,
    _cancelled: &impl Fn() -> bool,
) -> Result<FolderSnapshotV1, String> {
    // Everything 1.4 IPC does not provide a transaction/checkpoint proving
    // that a query is a complete, current subtree.  A successful shallow
    // result must therefore never be promoted to an exact folder snapshot.
    Err("Everything folder snapshot lacks complete-subtree proof".to_owned())
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "production-only Size Map acceleration selector; tests deliberately force the recursive reference backend"
    )
)]
fn force_recursive_backend_for_validation() -> bool {
    std::env::var_os("SUPEREXPLORER_FOLDER_SNAPSHOT_FORCE_RECURSIVE")
        .is_some_and(|value| value == "1")
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct FolderSizeServiceCountersV1 {
    pub physical_scans: u64,
    pub subscribers: u64,
    pub cache_hits: u64,
    pub stale_rejections: u64,
    pub fallback_count: u64,
}

#[cfg(windows)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum MftHelperPromptStateV1 {
    InFlight,
    Succeeded,
    Failed(String),
}

#[derive(Default)]
pub(crate) struct FolderSizeServiceV1 {
    snapshots: HashMap<SnapshotLeaseKeyV1, Arc<FolderSnapshotV1>>,
    modified_snapshots: HashMap<PathBuf, (u128, Arc<FolderSnapshotV1>)>,
    aggregate_snapshot_roots: HashSet<PathBuf>,
    leases: HashMap<SnapshotLeaseKeyV1, usize>,
    lru: VecDeque<SnapshotLeaseKeyV1>,
    capacity: usize,
    counters: FolderSizeServiceCountersV1,
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "production-only LocalSystem MFT query budget; test builds deliberately disable accelerated queries"
        )
    )]
    mft_cache_memory_mb: u16,
    #[cfg(windows)]
    mft_indexes: HashMap<String, Arc<crate::mft_size_map::MftIndexV1>>,
    #[cfg(windows)]
    mft_aggregates: HashMap<String, Arc<crate::mft_size_map::MftAggregateIndexV1>>,
    #[cfg(windows)]
    mft_checkpoints: HashMap<String, crate::mft_journal::MftCheckpointV2>,
    #[cfg(windows)]
    mft_helper_prompts: HashMap<String, MftHelperPromptStateV1>,
}

impl FolderSizeServiceV1 {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            mft_cache_memory_mb: explorer_model::DEFAULT_MFT_FOLDER_CACHE_MEMORY_MB,
            ..Self::default()
        }
    }
    /// Drops generation-bound publications for a manual refresh while keeping
    /// a complete modified-date record eligible for the correction contract:
    /// an unchanged folder may be rebound to the new generation without I/O.
    /// A watcher change invalidates every cached requested root that is an
    /// ancestor of the changed path. Descendant cache roots are also removed
    /// for directory rename/removal events where their old identity vanished.
    /// Aggregate-only path used by the Details Folder Size column. A valid MFT
    /// service index supplies a constant-time total and never materializes the
    /// Size Map tree or probes every descendant with filesystem metadata APIs.
    /// Returns the generation-compatible snapshot or performs the one physical
    /// reference scan while the caller's shared service lock serializes peers.
    pub(crate) fn snapshot_or_scan(
        &mut self,
        root: &Path,
        refresh_generation: u64,
        cancelled: impl Fn() -> bool,
    ) -> Result<Arc<FolderSnapshotV1>, String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "folder snapshot root is unavailable".to_owned())?;
        let key = SnapshotLeaseKeyV1 {
            canonical_root: canonical_root.clone(),
            refresh_generation,
        };
        let modified_stamp = folder_modified_stamp(&canonical_root)?;
        self.invalidate_modified_mismatch(&canonical_root, modified_stamp);
        self.counters.subscribers = self.counters.subscribers.saturating_add(1);
        if let Some(snapshot) = self
            .snapshots
            .get(&key)
            .filter(|snapshot| snapshot_has_complete_tree(snapshot))
            .cloned()
        {
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            self.emit_validation_counters();
            return Ok(snapshot);
        }
        if let Some((cached_stamp, cached)) = self.modified_snapshots.get(&canonical_root)
            && *cached_stamp == modified_stamp
            && cached.status == SnapshotStatusV1::Complete
            && snapshot_has_complete_tree(cached)
        {
            let mut reused = cached.as_ref().clone();
            reused.refresh_generation = refresh_generation;
            let reused = Arc::new(reused);
            self.snapshots.insert(key.clone(), Arc::clone(&reused));
            self.lru.retain(|candidate| candidate != &key);
            self.lru.push_back(key);
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            self.evict();
            self.emit_validation_counters();
            return Ok(reused);
        }
        if let Some(mut reused) = read_persistent_snapshot(&canonical_root, modified_stamp)
            && snapshot_has_complete_tree(&reused)
            && self.cached_snapshot_backend_is_current(&canonical_root, &reused)
        {
            reused.refresh_generation = refresh_generation;
            let reused = Arc::new(reused);
            self.modified_snapshots.insert(
                canonical_root.clone(),
                (modified_stamp, Arc::clone(&reused)),
            );
            self.snapshots.insert(key.clone(), Arc::clone(&reused));
            self.lru.push_back(key);
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            self.evict();
            self.emit_validation_counters();
            return Ok(reused);
        }
        // Installed Windows builds keep MFT ownership in the LocalSystem
        // service. The interactive process consumes only the service-computed
        // aggregate and projects it into the host-owned UI snapshot.
        #[cfg(all(windows, not(test)))]
        let accelerated = if force_recursive_backend_for_validation() {
            Err("MFT disabled by deterministic validation override".to_owned())
        } else {
            self.try_mft_snapshot(root, refresh_generation, &cancelled)
                .or_else(|service_error| {
                    self.try_helper_mft_snapshot(root, refresh_generation, &cancelled)
                        .map_err(|helper_error| {
                            format!("service: {service_error}; helper: {helper_error}")
                        })
                })
        };
        #[cfg(any(not(windows), test))]
        let accelerated: Result<FolderSnapshotV1, String> =
            Err("MFT backend disabled in this build".to_owned());
        let snapshot = match accelerated {
            Ok(snapshot) => snapshot,
            Err(mft_error) => {
                self.counters.fallback_count = self.counters.fallback_count.saturating_add(1);
                match try_everything_snapshot(root, refresh_generation, &cancelled) {
                    Ok(snapshot) => snapshot,
                    Err(everything_error) => {
                        self.counters.fallback_count =
                            self.counters.fallback_count.saturating_add(1);
                        tracing::debug!(
                            %mft_error,
                            %everything_error,
                            path = %root.display(),
                            "accelerated folder tree unavailable; using recursive reference"
                        );
                        scan_recursive_reference(
                            root,
                            refresh_generation,
                            RecursiveSnapshotPolicyV1::default(),
                            cancelled,
                        )?
                    }
                }
            }
        };
        let _ = self.publish_with_modified_stamp(key.clone(), modified_stamp, snapshot);
        self.snapshots
            .get(&key)
            .cloned()
            .ok_or_else(|| "folder snapshot publication failed".to_owned())
    }
    fn publish_with_modified_stamp(
        &mut self,
        key: SnapshotLeaseKeyV1,
        modified_stamp: u128,
        snapshot: FolderSnapshotV1,
    ) -> bool {
        if snapshot.refresh_generation != key.refresh_generation {
            self.counters.stale_rejections = self.counters.stale_rejections.saturating_add(1);
            self.emit_validation_counters();
            return false;
        }
        self.counters.physical_scans = self.counters.physical_scans.saturating_add(1);
        let snapshot = Arc::new(snapshot);
        if snapshot.status == SnapshotStatusV1::Complete {
            self.modified_snapshots.insert(
                key.canonical_root.clone(),
                (modified_stamp, Arc::clone(&snapshot)),
            );
            write_persistent_snapshot(&key.canonical_root, modified_stamp, &snapshot);
        }
        self.snapshots.insert(key.clone(), snapshot);
        self.lru.retain(|candidate| candidate != &key);
        self.lru.push_back(key);
        self.evict();
        self.emit_validation_counters();
        true
    }

    /// Publishes privacy-safe counters only when a headful validation process
    /// explicitly supplies an output path. Production sessions do no extra I/O.
    fn emit_validation_counters(&self) {
        let Some(path) = std::env::var_os("SUPEREXPLORER_FOLDER_SNAPSHOT_COUNTERS") else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec_pretty(&self.counters) else {
            return;
        };
        let _ = fs::write(PathBuf::from(path), bytes);
    }

    #[cfg(windows)]
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "production Size Map service path; tests validate equivalent recursive and indexed providers without contacting LocalSystem"
        )
    )]
    fn try_mft_snapshot(
        &mut self,
        root: &Path,
        refresh_generation: u64,
        cancelled: &impl Fn() -> bool,
    ) -> Result<FolderSnapshotV1, String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "MFT root is unavailable".to_owned())?;
        let root_reference = crate::mft_size_map::file_reference_number(&canonical_root)?;
        let projected =
            crate::mft_query::query_hierarchy(&canonical_root, self.mft_cache_memory_mb)?;
        if projected.len() > DEFAULT_MAX_NODES_V1
            || projected.first().map(|node| node.reference) != Some(root_reference)
        {
            return Err("MFT hierarchy root or node bound is invalid".to_owned());
        }
        let mut paths = HashMap::from([(root_reference, canonical_root.clone())]);
        let mut entries = Vec::with_capacity(projected.len().saturating_sub(1));
        for node in projected.into_iter().skip(1) {
            if cancelled() {
                return Err("MFT hierarchy projection was cancelled".to_owned());
            }
            let parent = node
                .parent_reference
                .and_then(|reference| paths.get(&reference))
                .ok_or_else(|| "MFT projection parent is missing".to_owned())?;
            let path = parent.join(&node.name);
            paths.insert(node.reference, path.clone());
            entries.push(IndexedSnapshotEntryV1 {
                path,
                bytes: if node.is_directory {
                    0
                } else {
                    node.logical_bytes
                },
                is_directory: node.is_directory,
            });
        }
        let aggregate = crate::mft_query::query_folder(&canonical_root, self.mft_cache_memory_mb)?;
        if aggregate.partial {
            return Err("MFT hierarchy aggregate is partial".to_owned());
        }
        let mut snapshot = snapshot_from_indexed_entries(
            &canonical_root,
            refresh_generation,
            SnapshotMethodV1::Mft,
            entries,
        )?;
        if snapshot.aggregate.recursive_bytes != aggregate.logical_bytes
            || snapshot.aggregate.file_count != aggregate.file_count
            || snapshot.aggregate.directory_count != aggregate.directory_count
        {
            return Err("MFT hierarchy completeness proof mismatch".to_owned());
        }
        snapshot.mft_generation = Some(aggregate.generation);
        Ok(snapshot)
    }

    #[cfg(windows)]
    #[cfg(windows)]
    fn helper_mft_index(
        &mut self,
        root: &Path,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(Arc<crate::mft_size_map::MftIndexV1>, u64), String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "MFT helper root is unavailable".to_owned())?;
        let volume = service_volume_letter(&canonical_root)?.to_string();
        let root_reference = crate::mft_size_map::file_reference_number(&canonical_root)?;
        if let Some(index) = self.mft_indexes.get(&volume).cloned() {
            index.project_subtree(root_reference, DEFAULT_MAX_NODES_V1, || cancelled())?;
            return Ok((index, root_reference));
        }
        match self.mft_helper_prompts.get(&volume) {
            Some(MftHelperPromptStateV1::InFlight) => {
                return Err("MFT helper prompt is already in flight for this volume".to_owned());
            }
            Some(MftHelperPromptStateV1::Failed(error)) => {
                return Err(format!(
                    "MFT helper was already declined or failed for this volume: {error}"
                ));
            }
            Some(MftHelperPromptStateV1::Succeeded) | None => {}
        }
        self.mft_helper_prompts
            .insert(volume.clone(), MftHelperPromptStateV1::InFlight);
        let result =
            crate::mft_size_map::read_volume_index_with_helper(&canonical_root, || cancelled())
                .and_then(|index| {
                    index.validate_topology()?;
                    index.project_subtree(root_reference, DEFAULT_MAX_NODES_V1, || cancelled())?;
                    Ok(Arc::new(index))
                });
        match result {
            Ok(index) => {
                self.mft_indexes.insert(volume.clone(), Arc::clone(&index));
                self.mft_helper_prompts
                    .insert(volume, MftHelperPromptStateV1::Succeeded);
                Ok((index, root_reference))
            }
            Err(error) => {
                self.mft_helper_prompts
                    .insert(volume, MftHelperPromptStateV1::Failed(error.clone()));
                Err(error)
            }
        }
    }

    #[cfg(windows)]
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "production Size Map helper fallback; tests exercise helper state separately without elevation"
        )
    )]
    fn try_helper_mft_snapshot(
        &mut self,
        root: &Path,
        refresh_generation: u64,
        cancelled: &impl Fn() -> bool,
    ) -> Result<FolderSnapshotV1, String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "MFT helper root is unavailable".to_owned())?;
        let (index, root_reference) = self.helper_mft_index(&canonical_root, cancelled)?;
        let projected =
            index.project_subtree(root_reference, DEFAULT_MAX_NODES_V1, || cancelled())?;
        let mut paths = HashMap::from([(root_reference, canonical_root.clone())]);
        let mut entries = Vec::with_capacity(projected.len().saturating_sub(1));
        for node in projected.into_iter().skip(1) {
            let parent = node
                .parent_reference
                .and_then(|reference| paths.get(&reference))
                .ok_or_else(|| "MFT helper projection parent is missing".to_owned())?;
            let path = parent.join(&node.name);
            paths.insert(node.reference, path.clone());
            entries.push(IndexedSnapshotEntryV1 {
                path,
                bytes: if node.is_directory {
                    0
                } else {
                    node.logical_bytes
                },
                is_directory: node.is_directory,
            });
        }
        let snapshot = snapshot_from_indexed_entries(
            &canonical_root,
            refresh_generation,
            SnapshotMethodV1::Mft,
            entries,
        )?;
        let expected = crate::mft_size_map::MftAggregateIndexV1::build(&index, 8)?
            .get(root_reference)
            .ok_or_else(|| "MFT helper aggregate root is missing".to_owned())?;
        if snapshot.aggregate.recursive_bytes != expected.logical_bytes
            || snapshot.aggregate.file_count != expected.file_count
            || snapshot.aggregate.directory_count != expected.directory_count
        {
            return Err("MFT helper completeness proof mismatch".to_owned());
        }
        Ok(snapshot)
    }

    #[cfg(windows)]
    /// Retain only terminal folder results for the active Explorer window.
    /// Aggregate-only snapshots are kept across sibling window navigations so
    /// folder-size results can be reused between tabs on the same volume.
    pub(crate) fn retain_cache_window(&mut self, root: &Path, max_depth: usize) {
        let Ok(root) = root.canonicalize() else {
            return;
        };
        let aggregate_snapshot_roots = &self.aggregate_snapshot_roots;
        self.snapshots.retain(|key, _| {
            self.leases.contains_key(key)
                || path_is_within_depth(&key.canonical_root, &root, max_depth)
        });
        self.modified_snapshots.retain(|path, _| {
            path_is_within_depth(path, &root, max_depth) || aggregate_snapshot_roots.contains(path)
        });
        for (key, snapshot) in &mut self.snapshots {
            if !self.aggregate_snapshot_roots.contains(&key.canonical_root) {
                *snapshot = Arc::new(compact_aggregate_snapshot(snapshot));
            }
        }
        for (path, (stamp, snapshot)) in &mut self.modified_snapshots {
            // Folder Size roots must retain their bounded tree projection so a
            // later Size Map subscriber can reuse the same physical walk.
            if !self.aggregate_snapshot_roots.contains(path) {
                *snapshot = Arc::new(compact_aggregate_snapshot(snapshot));
            }
            write_persistent_snapshot(path, *stamp, snapshot);
        }
        self.lru.retain(|key| self.snapshots.contains_key(key));
        #[cfg(windows)]
        {
            self.mft_indexes.clear();
            self.mft_aggregates.clear();
            self.mft_checkpoints.clear();
        }
    }
    fn cached_snapshot_backend_is_current(&self, root: &Path, snapshot: &FolderSnapshotV1) -> bool {
        if snapshot.method != SnapshotMethodV1::Mft {
            return true;
        }
        #[cfg(all(windows, not(test)))]
        {
            crate::mft_query::query_folder(root, self.mft_cache_memory_mb).is_ok_and(|current| {
                !current.partial && snapshot.mft_generation == Some(current.generation)
            })
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = root;
            false
        }
    }

    fn invalidate_modified_mismatch(&mut self, root: &Path, current_stamp: u128) {
        let Some((stale_stamp, _)) = self.modified_snapshots.get(root) else {
            return;
        };
        if *stale_stamp == current_stamp {
            return;
        }
        let stale_stamp = *stale_stamp;
        if let Some(path) = persistent_snapshot_path(root, stale_stamp) {
            let _ = fs::remove_file(path);
        }
        self.modified_snapshots.remove(root);
        self.aggregate_snapshot_roots.remove(root);
        self.snapshots
            .retain(|key, _| key.canonical_root != root || self.leases.contains_key(key));
        self.lru.retain(|key| self.snapshots.contains_key(key));
    }

    fn evict(&mut self) {
        while self.snapshots.len() > self.capacity {
            let Some(candidate) = self.lru.pop_front() else {
                break;
            };
            if self.leases.contains_key(&candidate) {
                self.lru.push_back(candidate);
                if self.lru.iter().all(|key| self.leases.contains_key(key)) {
                    break;
                }
            } else {
                self.snapshots.remove(&candidate);
            }
        }
        while self.modified_snapshots.len() > self.capacity {
            let Some(oldest) = self.modified_snapshots.keys().next().cloned() else {
                break;
            };
            self.modified_snapshots.remove(&oldest);
            self.aggregate_snapshot_roots.remove(&oldest);
        }
    }
}
fn path_is_within_depth(path: &Path, root: &Path, max_depth: usize) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|relative| relative.components().count() <= max_depth)
}

fn snapshot_has_complete_tree(snapshot: &FolderSnapshotV1) -> bool {
    snapshot.nodes.len() as u64
        >= snapshot
            .aggregate
            .file_count
            .saturating_add(snapshot.aggregate.directory_count)
}

fn compact_aggregate_snapshot(snapshot: &FolderSnapshotV1) -> FolderSnapshotV1 {
    let mut compact = snapshot.clone();
    compact.nodes.truncate(1);
    compact
}

fn folder_modified_stamp(path: &Path) -> Result<u128, String> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)
        })
        .map(|duration| duration.as_nanos())
        .map_err(|_| "folder modified date is unavailable".to_owned())
}

fn persistent_snapshot_path(root: &Path, modified_stamp: u128) -> Option<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    modified_stamp.hash(&mut hasher);
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|base| {
            base.join("SuperExplorer")
                .join("folder-snapshot-cache")
                .join("v2")
                .join(format!("{:016x}.json", hasher.finish()))
        })
}

const OBSOLETE_SNAPSHOT_CLEANUP_LIMIT_V1: usize = 256;

/// Retires the former Details snapshot cache in bounded, path-safe batches.
/// Size Map data uses different namespaces and is never traversed here.
pub(crate) fn retire_obsolete_details_snapshots_v1() {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let base = PathBuf::from(local_app_data)
        .join("SuperExplorer")
        .join("folder-snapshot-cache");
    retire_obsolete_details_snapshots_at_v1(&base);
}

fn retire_obsolete_details_snapshots_at_v1(base: &Path) {
    let directory = base.join("v2");
    let Ok(root_metadata) = fs::symlink_metadata(&directory) else {
        return;
    };
    if !root_metadata.is_dir()
        || root_metadata.file_type().is_symlink()
        || is_reparse_point(&root_metadata)
    {
        return;
    }
    let Ok(entries) = fs::read_dir(&directory) else {
        return;
    };
    let mut eligible = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.parent() != Some(directory.as_path())
            || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || is_reparse_point(&metadata)
            || metadata.len() > MAX_SNAPSHOT_BYTES_V1
        {
            continue;
        }
        let valid = fs::File::open(&path)
            .ok()
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(MAX_SNAPSHOT_BYTES_V1 + 1)
                    .read_to_end(&mut bytes)
                    .ok()
                    .filter(|_| bytes.len() as u64 <= MAX_SNAPSHOT_BYTES_V1)
                    .map(|_| bytes)
            })
            .and_then(|bytes| serde_json::from_slice::<PersistentSnapshotRecordV1>(&bytes).ok())
            .is_some_and(|record| record.schema == PERSISTENT_RECORD_SCHEMA_V1);
        if valid {
            eligible.push((metadata.modified().unwrap_or(std::time::UNIX_EPOCH), path));
        }
    }
    eligible.sort_by_key(|(modified, path)| (*modified, path.clone()));
    let eligible_count = eligible.len();
    for (_, path) in eligible
        .into_iter()
        .take(OBSOLETE_SNAPSHOT_CLEANUP_LIMIT_V1)
    {
        let _ = fs::remove_file(path);
    }
    if eligible_count <= OBSOLETE_SNAPSHOT_CLEANUP_LIMIT_V1 {
        let _ = fs::write(base.join("details-results-owned-by-mft-service.v1"), b"1\n");
    }
}

fn persistent_root_identity(root: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    hasher.finish()
}

const fn backend_data_version(method: SnapshotMethodV1) -> u32 {
    match method {
        SnapshotMethodV1::Recursive => 1,
        SnapshotMethodV1::Mft => 2,
        SnapshotMethodV1::Everything => 1,
    }
}

fn read_persistent_snapshot(root: &Path, modified_stamp: u128) -> Option<FolderSnapshotV1> {
    let path = persistent_snapshot_path(root, modified_stamp)?;
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_SNAPSHOT_BYTES_V1
    {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_SNAPSHOT_BYTES_V1 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES_V1 {
        return None;
    }
    let record: PersistentSnapshotRecordV1 = serde_json::from_slice(&bytes).ok()?;
    let expected = PersistentSnapshotKeyV1 {
        root_identity: persistent_root_identity(root),
        modified_stamp,
        semantic_policy_version: SEMANTIC_POLICY_VERSION_V1,
        backend_data_version: backend_data_version(record.snapshot.method),
    };
    (record.schema == PERSISTENT_RECORD_SCHEMA_V1
        && record.key == expected
        && record.snapshot.status == SnapshotStatusV1::Complete)
        .then_some(record.snapshot)
}

fn write_persistent_snapshot(root: &Path, modified_stamp: u128, snapshot: &FolderSnapshotV1) {
    let Some(destination) = persistent_snapshot_path(root, modified_stamp) else {
        return;
    };
    let Some(directory) = destination.parent() else {
        return;
    };
    let record = PersistentSnapshotRecordV1 {
        schema: PERSISTENT_RECORD_SCHEMA_V1,
        key: PersistentSnapshotKeyV1 {
            root_identity: persistent_root_identity(root),
            modified_stamp,
            semantic_policy_version: SEMANTIC_POLICY_VERSION_V1,
            backend_data_version: backend_data_version(snapshot.method),
        },
        snapshot: snapshot.clone(),
    };
    let Ok(bytes) = serde_json::to_vec(&record) else {
        return;
    };
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES_V1 {
        return;
    }
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let temporary = destination.with_extension(format!("{}.tmp", std::process::id()));
    if fs::write(&temporary, bytes).is_ok() {
        let _ = fs::remove_file(&destination);
        if fs::rename(&temporary, &destination).is_err() {
            let _ = fs::remove_file(temporary);
        }
    }
}

#[cfg(windows)]
fn service_volume_letter(root: &Path) -> Result<char, String> {
    use std::path::{Component, Prefix};
    root.components()
        .find_map(|component| match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    Some(char::from(letter).to_ascii_uppercase())
                }
                _ => None,
            },
            _ => None,
        })
        .ok_or_else(|| "MFT service cache requires a drive letter".to_owned())
}

#[cfg(windows)]
#[cfg(windows)]
#[cfg(windows)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "superexplorer-folder-snapshot-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn obsolete_details_snapshot_cleanup_is_bounded_and_non_recursive() {
        let base = fixture_root("obsolete-cleanup");
        let directory = base.join("v2");
        fs::create_dir_all(directory.join("nested")).unwrap();
        let record = PersistentSnapshotRecordV1 {
            schema: PERSISTENT_RECORD_SCHEMA_V1,
            key: PersistentSnapshotKeyV1 {
                root_identity: 1,
                modified_stamp: 1,
                semantic_policy_version: SEMANTIC_POLICY_VERSION_V1,
                backend_data_version: 2,
            },
            snapshot: FolderSnapshotV1 {
                schema: 1,
                root_id: SnapshotNodeIdV1(1),
                refresh_generation: 1,
                mft_generation: Some(1),
                method: SnapshotMethodV1::Mft,
                status: SnapshotStatusV1::Complete,
                diagnostic: None,
                aggregate: FolderAggregateSnapshotV1 {
                    recursive_bytes: 0,
                    direct_bytes: 0,
                    file_count: 0,
                    directory_count: 1,
                    status: SnapshotStatusV1::Complete,
                },
                nodes: Vec::new(),
            },
        };
        let bytes = serde_json::to_vec(&record).unwrap();
        for index in 0..257 {
            fs::write(directory.join(format!("{index:03}.json")), &bytes).unwrap();
        }
        fs::write(directory.join("invalid.json"), b"not-json").unwrap();
        fs::write(directory.join("nested").join("keep.json"), &bytes).unwrap();

        retire_obsolete_details_snapshots_at_v1(&base);

        let valid_remaining = fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry.file_name().to_string_lossy() != "invalid.json"
                    && entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .count();
        assert_eq!(valid_remaining, 1);
        assert!(directory.join("invalid.json").is_file());
        assert!(directory.join("nested").join("keep.json").is_file());
        assert!(
            !base
                .join("details-results-owned-by-mft-service.v1")
                .exists()
        );

        retire_obsolete_details_snapshots_at_v1(&base);
        assert!(
            base.join("details-results-owned-by-mft-service.v1")
                .is_file()
        );
        assert!(directory.join("invalid.json").is_file());
        assert!(directory.join("nested").join("keep.json").is_file());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn recursive_snapshot_projects_exact_aggregate_and_tree() {
        let root = fixture_root("exact");
        fs::create_dir_all(root.join("src/deep")).unwrap();
        fs::write(root.join("root.bin"), vec![0_u8; 3]).unwrap();
        fs::write(root.join("src/lib.rs"), vec![0_u8; 5]).unwrap();
        fs::write(root.join("src/deep/data.bin"), vec![0_u8; 7]).unwrap();
        let snapshot =
            scan_recursive_reference(&root, 9, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        assert_eq!(snapshot.aggregate.recursive_bytes, 15);
        assert_eq!(snapshot.aggregate.direct_bytes, 3);
        assert_eq!(snapshot.aggregate.file_count, 3);
        assert_eq!(snapshot.aggregate.directory_count, 3);
        assert_eq!(snapshot.nodes[0].recursive_bytes, 15);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recursive_reference_emits_progressive_nodes_and_terminal_delta() {
        let root = fixture_root("progressive-deltas");
        fs::create_dir_all(root.join("child")).unwrap();
        fs::write(root.join("child/payload.bin"), vec![0_u8; 9]).unwrap();
        let mut deltas = Vec::new();
        let snapshot = scan_recursive_reference_with_deltas(
            &root,
            7,
            RecursiveSnapshotPolicyV1::default(),
            || false,
            |delta| deltas.push(delta),
        )
        .unwrap();

        assert_eq!(snapshot.aggregate.recursive_bytes, 9);
        assert!(matches!(
            deltas.first(),
            Some(FolderSnapshotDeltaV1::Add(_))
        ));
        assert!(
            deltas
                .iter()
                .any(|delta| matches!(delta, FolderSnapshotDeltaV1::SubtreeComplete(_)))
        );
        assert_eq!(
            deltas.last(),
            Some(&FolderSnapshotDeltaV1::ScanComplete(
                SnapshotStatusV1::Complete
            ))
        );
        assert_eq!(
            deltas
                .iter()
                .filter(|delta| matches!(delta, FolderSnapshotDeltaV1::Add(_)))
                .count(),
            snapshot.nodes.len()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn indexed_provider_normalization_equals_recursive_reference() {
        let root = fixture_root("indexed-equality");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("root.bin"), vec![0_u8; 3]).unwrap();
        fs::write(root.join("src/lib.rs"), vec![0_u8; 5]).unwrap();
        let reference =
            scan_recursive_reference(&root, 4, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        let indexed = snapshot_from_indexed_entries(
            &root,
            4,
            SnapshotMethodV1::Everything,
            vec![
                IndexedSnapshotEntryV1 {
                    path: root.join("src"),
                    bytes: 0,
                    is_directory: true,
                },
                IndexedSnapshotEntryV1 {
                    path: root.join("root.bin"),
                    bytes: 3,
                    is_directory: false,
                },
                IndexedSnapshotEntryV1 {
                    path: root.join("src/lib.rs"),
                    bytes: 5,
                    is_directory: false,
                },
            ],
        )
        .unwrap();
        assert_eq!(indexed.aggregate, reference.aggregate);
        let mut indexed_nodes = indexed.nodes;
        let mut reference_nodes = reference.nodes;
        indexed_nodes.sort_by_key(|node| node.id.0);
        reference_nodes.sort_by_key(|node| node.id.0);
        assert_eq!(indexed_nodes, reference_nodes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn everything_without_complete_subtree_proof_is_ineligible() {
        let root = fixture_root("everything-incomplete");
        fs::create_dir_all(root.join("child")).unwrap();
        fs::write(root.join("child/payload.bin"), vec![0_u8; 32]).unwrap();

        let error = try_everything_snapshot(&root, 1, &|| false).unwrap_err();
        assert!(error.contains("complete-subtree proof"));

        let mut service = FolderSizeServiceV1::with_capacity(1);
        let snapshot = service.snapshot_or_scan(&root, 1, || false).unwrap();
        assert_eq!(snapshot.method, SnapshotMethodV1::Recursive);
        assert_eq!(snapshot.aggregate.recursive_bytes, 32);
        assert_ne!(snapshot.aggregate.recursive_bytes, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resource_limit_and_cancellation_are_typed_not_exact_zero() {
        let root = fixture_root("terminal");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("one"), vec![0_u8; 4]).unwrap();
        fs::write(root.join("two"), vec![0_u8; 8]).unwrap();
        let limited =
            scan_recursive_reference(&root, 1, RecursiveSnapshotPolicyV1 { max_nodes: 2 }, || {
                false
            })
            .unwrap();
        assert_eq!(limited.status, SnapshotStatusV1::ResourceLimited);
        let cancelled =
            scan_recursive_reference(&root, 2, RecursiveSnapshotPolicyV1::default(), || true)
                .unwrap();
        assert_eq!(cancelled.status, SnapshotStatusV1::Cancelled);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn modified_date_controls_cross_generation_folder_cache() {
        let root = fixture_root("modified-cache");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("payload.bin"), vec![0_u8; 17]).unwrap();
        let mut service = FolderSizeServiceV1::with_capacity(8);

        let first = service.snapshot_or_scan(&root, 1, || false).unwrap();
        let second = service.snapshot_or_scan(&root, 2, || false).unwrap();
        assert_eq!(first.aggregate.recursive_bytes, 17);
        assert_eq!(second.aggregate.recursive_bytes, 17);
        assert_eq!(second.refresh_generation, 2);
        assert_eq!(service.counters.physical_scans, 1);

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(root.join("new.bin"), vec![0_u8; 5]).unwrap();
        let third = service.snapshot_or_scan(&root, 3, || false).unwrap();
        assert_eq!(third.aggregate.recursive_bytes, 22);
        assert_eq!(service.counters.physical_scans, 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persistent_record_rejects_policy_backend_identity_and_corruption() {
        let root = fixture_root("persistent-record-key");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("payload.bin"), vec![0_u8; 13]).unwrap();
        let canonical = root.canonicalize().unwrap();
        let stamp = folder_modified_stamp(&canonical).unwrap();
        let snapshot =
            scan_recursive_reference(&canonical, 1, RecursiveSnapshotPolicyV1::default(), || {
                false
            })
            .unwrap();
        write_persistent_snapshot(&canonical, stamp, &snapshot);
        assert_eq!(
            read_persistent_snapshot(&canonical, stamp)
                .unwrap()
                .aggregate
                .recursive_bytes,
            13
        );

        let path = persistent_snapshot_path(&canonical, stamp).unwrap();
        let mut record: PersistentSnapshotRecordV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record.key.semantic_policy_version = record.key.semantic_policy_version.saturating_add(1);
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(read_persistent_snapshot(&canonical, stamp).is_none());

        write_persistent_snapshot(&canonical, stamp, &snapshot);
        let mut record: PersistentSnapshotRecordV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record.key.backend_data_version = record.key.backend_data_version.saturating_add(1);
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(read_persistent_snapshot(&canonical, stamp).is_none());

        fs::write(&path, b"not-json").unwrap();
        assert!(read_persistent_snapshot(&canonical, stamp).is_none());
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn completed_folder_results_are_trimmed_to_active_three_level_window() {
        let root = fixture_root("three-level-window");
        let a = root.join("a");
        let b = a.join("b");
        let c = b.join("c");
        let d = c.join("d");
        let outside = root.with_extension("outside");
        fs::create_dir_all(&d).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let mut service = FolderSizeServiceV1::with_capacity(16);
        for (generation, directory) in [&a, &b, &c, &d, &outside].into_iter().enumerate() {
            service
                .snapshot_or_scan(directory, generation as u64 + 1, || false)
                .unwrap();
        }

        service.retain_cache_window(&root, 3);
        assert!(
            service
                .modified_snapshots
                .contains_key(&a.canonicalize().unwrap())
        );
        assert!(
            service
                .modified_snapshots
                .contains_key(&b.canonicalize().unwrap())
        );
        assert!(
            service
                .modified_snapshots
                .contains_key(&c.canonicalize().unwrap())
        );
        assert!(
            !service
                .modified_snapshots
                .contains_key(&d.canonicalize().unwrap())
        );
        assert!(
            !service
                .modified_snapshots
                .contains_key(&outside.canonicalize().unwrap())
        );
        let compact = &service.modified_snapshots[&a.canonicalize().unwrap()].1;
        assert_eq!(
            compact.nodes.len(),
            1,
            "Host retains only the terminal aggregate"
        );
        let size_map_tree = service.snapshot_or_scan(&a, 99, || false).unwrap();
        assert!(snapshot_has_complete_tree(&size_map_tree));
        assert!(size_map_tree.nodes.len() > 1);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn hard_links_are_counted_per_directory_entry_like_explorer() {
        let root = fixture_root("hard-links");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("original.bin"), vec![0_u8; 11]).unwrap();
        fs::hard_link(root.join("original.bin"), root.join("alias.bin")).unwrap();

        let snapshot =
            scan_recursive_reference(&root, 1, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        assert_eq!(snapshot.aggregate.file_count, 2);
        assert_eq!(snapshot.aggregate.recursive_bytes, 22);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mutations_require_a_new_generation_and_deep_trees_remain_exact() {
        let root = fixture_root("mutation-deep");
        let mut leaf = root.clone();
        for depth in 0..48 {
            leaf.push(format!("d{depth}"));
        }
        fs::create_dir_all(&leaf).unwrap();
        fs::write(leaf.join("payload.bin"), vec![0_u8; 13]).unwrap();
        let before =
            scan_recursive_reference(&root, 7, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        assert_eq!(before.aggregate.recursive_bytes, 13);
        assert_eq!(before.aggregate.directory_count, 49);

        fs::write(leaf.join("payload.bin"), vec![0_u8; 21]).unwrap();
        let after =
            scan_recursive_reference(&root, 8, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        assert_eq!(after.aggregate.recursive_bytes, 21);
        assert_eq!(after.refresh_generation, 8);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn subtree_that_becomes_unavailable_produces_a_partial_snapshot() {
        let root = fixture_root("unavailable-subtree");
        let subtree = root.join("locked");
        fs::create_dir_all(&subtree).unwrap();
        fs::write(subtree.join("payload.bin"), vec![0_u8; 9]).unwrap();
        let polls = AtomicUsize::new(0);
        let snapshot =
            scan_recursive_reference(&root, 1, RecursiveSnapshotPolicyV1::default(), || {
                if polls.fetch_add(1, Ordering::SeqCst) == 1 {
                    fs::remove_dir_all(&subtree).unwrap();
                }
                false
            })
            .unwrap();
        assert_eq!(snapshot.status, SnapshotStatusV1::Partial);
        assert!(snapshot.diagnostic.is_some());
        assert_eq!(snapshot.aggregate.recursive_bytes, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn directory_reparse_points_are_reported_but_never_traversed() {
        use std::os::windows::fs::symlink_dir;

        let root = fixture_root("reparse");
        let outside = fixture_root("reparse-outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("must-not-count.bin"), vec![0_u8; 31]).unwrap();
        if symlink_dir(&outside, root.join("link")).is_err() {
            // Windows can disallow symlink creation when Developer Mode is off.
            let _ = fs::remove_dir_all(root);
            let _ = fs::remove_dir_all(outside);
            return;
        }
        let snapshot =
            scan_recursive_reference(&root, 1, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        assert_eq!(snapshot.aggregate.recursive_bytes, 0);
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == SnapshotNodeKindV1::ReparsePoint)
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[cfg(windows)]
    #[test]
    fn helper_prompt_state_coalesces_and_does_not_repeat_terminal_failure() {
        let root = fixture_root("helper-prompt-state");
        fs::create_dir_all(&root).unwrap();
        let canonical = root.canonicalize().unwrap();
        let volume = service_volume_letter(&canonical).unwrap().to_string();
        let mut service = FolderSizeServiceV1::with_capacity(8);

        service
            .mft_helper_prompts
            .insert(volume.clone(), MftHelperPromptStateV1::InFlight);
        let in_flight = service.helper_mft_index(&canonical, &|| false).unwrap_err();
        assert!(in_flight.contains("already in flight"));

        service.mft_helper_prompts.insert(
            volume,
            MftHelperPromptStateV1::Failed("user declined UAC".to_owned()),
        );
        let declined = service.helper_mft_index(&canonical, &|| false).unwrap_err();
        assert!(declined.contains("already declined or failed"));
        assert!(declined.contains("user declined UAC"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires SUPEREXPLORER_PROFILE_FOLDER_ROOT"]
    fn opt_in_profile_cold_warm_folder_backends() {
        let root = PathBuf::from(
            std::env::var_os("SUPEREXPLORER_PROFILE_FOLDER_ROOT")
                .expect("SUPEREXPLORER_PROFILE_FOLDER_ROOT is required"),
        );
        let started = std::time::Instant::now();
        let cold =
            scan_recursive_reference(&root, 1, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        let cold_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let warm_reference =
            scan_recursive_reference(&root, 2, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        let warm_reference_ms = started.elapsed().as_millis();
        assert_eq!(cold.aggregate, warm_reference.aggregate);

        let mut service = FolderSizeServiceV1::with_capacity(8);
        let started = std::time::Instant::now();
        let first = service.snapshot_or_scan(&root, 3, || false).unwrap();
        let service_first_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let warm = service.snapshot_or_scan(&root, 4, || false).unwrap();
        let service_warm_ms = started.elapsed().as_millis();
        assert_eq!(cold.aggregate, first.aggregate);
        assert_eq!(first.aggregate, warm.aggregate);

        let started = std::time::Instant::now();
        let everything =
            explorer_shell_win::query_folder_index(&root, DEFAULT_MAX_NODES_V1, || false);
        let everything_ms = started.elapsed().as_millis();
        println!(
            "PROFILE_JSON={{\"root\":{:?},\"recursive_cold_ms\":{},\"recursive_warm_ms\":{},\"service_first_ms\":{},\"service_warm_ms\":{},\"bytes\":{},\"files\":{},\"directories\":{},\"everything_ms\":{},\"everything_eligible\":false,\"everything_result\":{:?}}}",
            root.display().to_string(),
            cold_ms,
            warm_reference_ms,
            service_first_ms,
            service_warm_ms,
            cold.aggregate.recursive_bytes,
            cold.aggregate.file_count,
            cold.aggregate.directory_count,
            everything_ms,
            everything.as_ref().map(Vec::len).map_err(String::as_str),
        );
    }
}
