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

pub(crate) fn encode_snapshot_bounded(snapshot: &FolderSnapshotV1) -> Result<Vec<u8>, String> {
    if snapshot.schema != SNAPSHOT_SCHEMA_V2 {
        return Err("unsupported folder snapshot schema".to_owned());
    }
    let bytes = serde_json::to_vec(snapshot).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES_V1 {
        return Err("folder snapshot exceeds the record limit".to_owned());
    }
    Ok(bytes)
}

pub(crate) fn decode_snapshot_bounded(mut reader: impl Read) -> Result<FolderSnapshotV1, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(MAX_SNAPSHOT_BYTES_V1 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES_V1 {
        return Err("folder snapshot exceeds the record limit".to_owned());
    }
    let snapshot: FolderSnapshotV1 =
        serde_json::from_slice(&bytes).map_err(|_| "corrupt folder snapshot".to_owned())?;
    if snapshot.schema != SNAPSHOT_SCHEMA_V2 || snapshot.nodes.len() > DEFAULT_MAX_NODES_V1 {
        return Err("unsupported or oversized folder snapshot".to_owned());
    }
    if !snapshot
        .nodes
        .iter()
        .any(|node| node.id == snapshot.root_id)
    {
        return Err("folder snapshot root is missing".to_owned());
    }
    Ok(snapshot)
}

pub(crate) fn scan_recursive_reference(
    root: &Path,
    refresh_generation: u64,
    policy: RecursiveSnapshotPolicyV1,
    cancelled: impl Fn() -> bool,
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
            return Ok(finish_snapshot(
                root_id,
                refresh_generation,
                aggregate.status,
                Some("folder snapshot cancelled".to_owned()),
                aggregate,
                nodes,
            ));
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                mark_partial(&mut nodes, &indices, directory_id);
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
                return Ok(finish_snapshot(
                    root_id,
                    refresh_generation,
                    aggregate.status,
                    diagnostic,
                    aggregate,
                    nodes,
                ));
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
        }
    }
    aggregate.recursive_bytes = nodes[0].recursive_bytes;
    aggregate.status = nodes[0].status;
    Ok(finish_snapshot(
        root_id,
        refresh_generation,
        aggregate.status,
        diagnostic,
        aggregate,
        nodes,
    ))
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FolderSizeServiceCountersV1 {
    pub physical_scans: u64,
    pub subscribers: u64,
    pub cache_hits: u64,
    pub stale_rejections: u64,
    pub fallback_count: u64,
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
    mft_cache_memory_mb: u16,
    #[cfg(windows)]
    mft_indexes: HashMap<String, Arc<crate::mft_size_map::MftIndexV1>>,
    #[cfg(windows)]
    mft_aggregates: HashMap<String, Arc<crate::mft_size_map::MftAggregateIndexV1>>,
    #[cfg(windows)]
    mft_checkpoints: HashMap<String, crate::mft_journal::MftCheckpointV2>,
}

impl FolderSizeServiceV1 {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            mft_cache_memory_mb: explorer_model::DEFAULT_MFT_FOLDER_CACHE_MEMORY_MB,
            ..Self::default()
        }
    }

    pub(crate) fn set_mft_cache_memory_mb(&mut self, value: u16) {
        self.mft_cache_memory_mb = explorer_model::normalized_mft_folder_cache_memory_mb(value);
    }

    pub(crate) fn subscribe(&mut self, key: SnapshotLeaseKeyV1) -> Option<Arc<FolderSnapshotV1>> {
        *self.leases.entry(key.clone()).or_insert(0) += 1;
        self.counters.subscribers = self.counters.subscribers.saturating_add(1);
        let snapshot = self.snapshots.get(&key).cloned();
        if snapshot.is_some() {
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
        }
        snapshot
    }

    /// Aggregate-only path used by the Details Folder Size column. A valid MFT
    /// service index supplies a constant-time total and never materializes the
    /// Size Map tree or probes every descendant with filesystem metadata APIs.
    pub(crate) fn aggregate_or_scan(
        &mut self,
        root: &Path,
        refresh_generation: u64,
        require_current_mft: bool,
        cancelled: impl Fn() -> bool,
        method_changed: impl Fn(SnapshotMethodV1),
    ) -> Result<(Arc<FolderSnapshotV1>, bool), String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "folder snapshot root is unavailable".to_owned())?;
        self.aggregate_snapshot_roots.insert(canonical_root.clone());
        let key = SnapshotLeaseKeyV1 {
            canonical_root: canonical_root.clone(),
            refresh_generation,
        };
        let modified_stamp = folder_modified_stamp(&canonical_root)?;
        self.counters.subscribers = self.counters.subscribers.saturating_add(1);
        if !require_current_mft && let Some(snapshot) = self.snapshots.get(&key).cloned() {
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            return Ok((snapshot, true));
        }
        if !require_current_mft
            && let Some((cached_stamp, cached)) = self.modified_snapshots.get(&canonical_root)
            && *cached_stamp == modified_stamp
            && cached.status == SnapshotStatusV1::Complete
        {
            let mut reused = cached.as_ref().clone();
            reused.refresh_generation = refresh_generation;
            let reused = Arc::new(reused);
            self.snapshots.insert(key.clone(), Arc::clone(&reused));
            self.lru.push_back(key);
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            self.evict();
            return Ok((reused, true));
        }
        if !require_current_mft
            && let Some(mut reused) = read_persistent_snapshot(&canonical_root, modified_stamp)
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
            return Ok((reused, true));
        }
        #[cfg(all(windows, not(test)))]
        let accelerated = {
            method_changed(SnapshotMethodV1::Mft);
            self.try_mft_aggregate(&canonical_root, refresh_generation, &cancelled)
        };
        #[cfg(any(not(windows), test))]
        let accelerated: Result<FolderSnapshotV1, String> =
            Err("MFT backend disabled in this build".to_owned());
        let snapshot = match accelerated {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if require_current_mft {
                    return Err(format!("MFT unavailable: {error}"));
                }
                #[cfg(test)]
                {
                    method_changed(SnapshotMethodV1::Recursive);
                    scan_recursive_reference(
                        root,
                        refresh_generation,
                        RecursiveSnapshotPolicyV1::default(),
                        cancelled,
                    )?
                }
                #[cfg(not(test))]
                {
                    tracing::debug!(%error, path = %root.display(), "MFT aggregate unavailable");
                    return Err(format!("MFT unavailable: {error}"));
                }
            }
        };
        // A partial MFT aggregate is a moment-in-time lower bound while the
        // service is rebuilding or enforcing a budget. Do not make it a
        // terminal per-generation snapshot: the Details column must retry and
        // replace it with the later exact service result.
        if snapshot.status != SnapshotStatusV1::Complete {
            return Ok((Arc::new(snapshot), false));
        }
        let _ = self.publish_with_modified_stamp(key.clone(), modified_stamp, snapshot);
        self.snapshots
            .get(&key)
            .cloned()
            .map(|snapshot| (snapshot, false))
            .ok_or_else(|| "folder snapshot publication failed".to_owned())
    }

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
        self.counters.subscribers = self.counters.subscribers.saturating_add(1);
        if let Some(snapshot) = self
            .snapshots
            .get(&key)
            .filter(|snapshot| snapshot_has_complete_tree(snapshot))
            .cloned()
        {
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
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
            return Ok(reused);
        }
        if let Some(mut reused) = read_persistent_snapshot(&canonical_root, modified_stamp)
            && snapshot_has_complete_tree(&reused)
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
            return Ok(reused);
        }
        // Installed Windows builds keep MFT ownership in the LocalSystem
        // service. The interactive process consumes only the service-computed
        // aggregate and projects it into the host-owned UI snapshot.
        #[cfg(all(windows, not(test)))]
        let accelerated = self.try_mft_snapshot(root, refresh_generation, &cancelled);
        #[cfg(any(not(windows), test))]
        let accelerated: Result<FolderSnapshotV1, String> =
            Err("MFT backend disabled in this build".to_owned());
        let snapshot = match accelerated {
            Ok(snapshot) => snapshot,
            Err(error) => {
                #[cfg(test)]
                {
                    scan_recursive_reference(
                        root,
                        refresh_generation,
                        RecursiveSnapshotPolicyV1::default(),
                        cancelled,
                    )?
                }
                #[cfg(not(test))]
                {
                    tracing::debug!(%error, path = %root.display(), "MFT folder snapshot unavailable");
                    return Err(format!("MFT unavailable: {error}"));
                }
            }
        };
        let _ = self.publish_with_modified_stamp(key.clone(), modified_stamp, snapshot);
        self.snapshots
            .get(&key)
            .cloned()
            .ok_or_else(|| "folder snapshot publication failed".to_owned())
    }

    pub(crate) fn publish(&mut self, key: SnapshotLeaseKeyV1, snapshot: FolderSnapshotV1) -> bool {
        let modified_stamp = folder_modified_stamp(&key.canonical_root).unwrap_or_default();
        self.publish_with_modified_stamp(key, modified_stamp, snapshot)
    }

    fn publish_with_modified_stamp(
        &mut self,
        key: SnapshotLeaseKeyV1,
        modified_stamp: u128,
        snapshot: FolderSnapshotV1,
    ) -> bool {
        if snapshot.refresh_generation != key.refresh_generation {
            self.counters.stale_rejections = self.counters.stale_rejections.saturating_add(1);
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
        true
    }

    #[cfg(windows)]
    fn sync_service_mft_index(&mut self, root: &Path) -> Result<(), String> {
        use std::path::Component;

        let canonical_root = root
            .canonicalize()
            .map_err(|_| "MFT root is unavailable".to_owned())?;
        let volume = canonical_root
            .components()
            .find_map(|component| match component {
                Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().to_string()),
                _ => None,
            })
            .ok_or_else(|| "MFT requires a local volume".to_owned())?;
        let letter = service_volume_letter(&canonical_root)?;
        let cache = service_mft_cache_root();
        let latest = crate::mft_journal::latest_checkpoint(&cache, letter)?
            .ok_or_else(|| "MFT service checkpoint is unavailable".to_owned())?;
        if self.mft_checkpoints.get(&volume) == Some(&latest) {
            return Ok(());
        }

        let mut invalidated_paths = std::collections::HashSet::new();
        let (mut index, mut cursor) = if let (Some(index), Some(checkpoint)) = (
            self.mft_indexes.get(&volume),
            self.mft_checkpoints.get(&volume).copied(),
        ) {
            if checkpoint.volume != latest.volume || checkpoint.journal_id != latest.journal_id {
                fresh_service_mft_index(&canonical_root)?
            } else {
                (index.as_ref().clone(), checkpoint)
            }
        } else {
            fresh_service_mft_index(&canonical_root)?
        };

        if cursor.generation < latest.generation {
            for delta in crate::mft_journal::deltas_after(
                &cache,
                letter,
                cursor.generation,
                latest.generation,
            )? {
                if delta.volume != cursor.volume
                    || delta.journal_id != cursor.journal_id
                    || delta.generation != cursor.generation.saturating_add(1)
                    || delta.start_usn != cursor.next_usn
                    || delta.next_usn < delta.start_usn
                {
                    return Err("MFT service delta chain is not contiguous".to_owned());
                }
                for change in &delta.changes {
                    for reference in index.ancestor_references(change.reference) {
                        if let Some(path) =
                            index.path_for_reference(&volume_root(&canonical_root), reference)
                        {
                            invalidated_paths.insert(path);
                        }
                    }
                    let affected = index.apply_change(change)?;
                    for reference in affected {
                        if let Some(path) =
                            index.path_for_reference(&volume_root(&canonical_root), reference)
                        {
                            invalidated_paths.insert(path);
                        }
                    }
                }
                cursor = crate::mft_journal::MftCheckpointV2::new(
                    cursor.volume,
                    cursor.journal_id,
                    delta.next_usn,
                    delta.generation,
                );
            }
        }
        if cursor != latest {
            return Err("MFT service checkpoint does not match applied deltas".to_owned());
        }

        self.mft_indexes.insert(volume.clone(), Arc::new(index));
        self.mft_checkpoints.insert(volume.clone(), latest);
        self.mft_aggregates.remove(&volume);
        if !invalidated_paths.is_empty() {
            for path in &invalidated_paths {
                if let Ok(stamp) = folder_modified_stamp(path)
                    && let Some(snapshot_path) = persistent_snapshot_path(path, stamp)
                {
                    let _ = fs::remove_file(snapshot_path);
                }
            }
            self.modified_snapshots
                .retain(|path, _| !invalidated_paths.contains(path));
            self.aggregate_snapshot_roots
                .retain(|path| !invalidated_paths.contains(path));
            self.snapshots
                .retain(|key, _| !invalidated_paths.contains(&key.canonical_root));
            self.lru
                .retain(|key| !invalidated_paths.contains(&key.canonical_root));
        }
        Ok(())
    }

    #[cfg(windows)]
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
        snapshot_from_indexed_entries(
            &canonical_root,
            refresh_generation,
            SnapshotMethodV1::Mft,
            entries,
        )
    }

    #[cfg(windows)]
    fn try_mft_aggregate(
        &mut self,
        root: &Path,
        refresh_generation: u64,
        _cancelled: &impl Fn() -> bool,
    ) -> Result<FolderSnapshotV1, String> {
        let aggregate = crate::mft_query::query_folder(root, self.mft_cache_memory_mb)?;
        let root_id = stable_node_id(Path::new(""));
        let snapshot_status = if aggregate.partial {
            SnapshotStatusV1::Partial
        } else {
            SnapshotStatusV1::Complete
        };
        Ok(FolderSnapshotV1 {
            schema: SNAPSHOT_SCHEMA_V2,
            root_id,
            refresh_generation,
            mft_generation: Some(aggregate.generation),
            method: SnapshotMethodV1::Mft,
            status: snapshot_status,
            // Partial is a typed successful state, not an error. Keeping the
            // diagnostic empty lets Details render the known lower bound.
            diagnostic: None,
            aggregate: FolderAggregateSnapshotV1 {
                recursive_bytes: aggregate.logical_bytes,
                direct_bytes: 0,
                file_count: aggregate.file_count,
                directory_count: aggregate.directory_count,
                status: snapshot_status,
            },
            nodes: vec![FolderSnapshotNodeV1 {
                id: root_id,
                parent: None,
                name: root.file_name().map_or_else(
                    || root.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                kind: SnapshotNodeKindV1::Directory,
                direct_bytes: 0,
                recursive_bytes: aggregate.logical_bytes,
                status: snapshot_status,
            }],
        })
    }

    pub(crate) fn release(&mut self, key: &SnapshotLeaseKeyV1) {
        let Some(count) = self.leases.get_mut(key) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.leases.remove(key);
        }
        self.evict();
    }

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
        for snapshot in self.snapshots.values_mut() {
            *snapshot = Arc::new(compact_aggregate_snapshot(snapshot));
        }
        for (path, (stamp, snapshot)) in &mut self.modified_snapshots {
            *snapshot = Arc::new(compact_aggregate_snapshot(snapshot));
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

    pub(crate) fn counters(&self) -> FolderSizeServiceCountersV1 {
        self.counters.clone()
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

fn read_persistent_snapshot(root: &Path, modified_stamp: u128) -> Option<FolderSnapshotV1> {
    let path = persistent_snapshot_path(root, modified_stamp)?;
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_SNAPSHOT_BYTES_V1
    {
        return None;
    }
    let snapshot = decode_snapshot_bounded(fs::File::open(path).ok()?).ok()?;
    (snapshot.status == SnapshotStatusV1::Complete).then_some(snapshot)
}

fn write_persistent_snapshot(root: &Path, modified_stamp: u128, snapshot: &FolderSnapshotV1) {
    let Some(destination) = persistent_snapshot_path(root, modified_stamp) else {
        return;
    };
    let Some(directory) = destination.parent() else {
        return;
    };
    let Ok(bytes) = encode_snapshot_bounded(snapshot) else {
        return;
    };
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
fn service_mft_cache_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("SuperExplorer")
        .join("MftIndex")
}

#[cfg(windows)]
fn volume_root(path: &Path) -> PathBuf {
    service_volume_letter(path)
        .map(|letter| PathBuf::from(format!("{letter}:\\")))
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(windows)]
fn fresh_service_mft_index(
    root: &Path,
) -> Result<
    (
        crate::mft_size_map::MftIndexV1,
        crate::mft_journal::MftCheckpointV2,
    ),
    String,
> {
    let letter = service_volume_letter(root)?;
    let cache = service_mft_cache_root();
    let path = cache.join(format!("{letter}.semftidx"));
    let latest = crate::mft_journal::latest_checkpoint(&cache, letter)?
        .ok_or_else(|| "MFT service checkpoint is unavailable".to_owned())?;
    let mut last_error = "MFT service cache is unavailable".to_owned();
    for attempt in 0..5 {
        match crate::mft_size_map::read_index(&path) {
            Ok(mut index) => {
                let deltas =
                    crate::mft_journal::deltas_after(&cache, letter, 0, latest.generation)?;
                let mut expected_generation = 0_u64;
                let mut expected_usn = deltas
                    .first()
                    .map_or(latest.next_usn, |delta| delta.start_usn);
                for delta in deltas {
                    if delta.volume != latest.volume
                        || delta.journal_id != latest.journal_id
                        || delta.generation != expected_generation.saturating_add(1)
                        || delta.start_usn != expected_usn
                    {
                        return Err("MFT service delta chain is not contiguous".to_owned());
                    }
                    for change in &delta.changes {
                        let _ = index.apply_change(change)?;
                    }
                    expected_generation = delta.generation;
                    expected_usn = delta.next_usn;
                }
                if expected_generation != latest.generation || expected_usn != latest.next_usn {
                    return Err("MFT service checkpoint does not match base/deltas".to_owned());
                }
                return Ok((index, latest));
            }
            Err(error) => last_error = error,
        }
        if attempt < 4 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    #[cfg(windows)]
    fn opt_in_mft_service_aggregates_visible_child_directories() {
        let Ok(root) = std::env::var("SUPEREXPLORER_MFT_TEST_PARENT") else {
            return;
        };
        use std::os::windows::fs::MetadataExt as _;
        let mut service = FolderSizeServiceV1::with_capacity(64);
        for entry in fs::read_dir(root).unwrap().flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_dir() || metadata.file_attributes() & (0x2 | 0x4) != 0 {
                continue;
            }
            let canonical = entry.path().canonicalize().unwrap();
            let expected = match entry.file_name().to_string_lossy().as_ref() {
                "files-999" => Some((999, 1)),
                "files-1000" => Some((1_000, 1)),
                "nested-counts" => Some((3, 3)),
                _ => None,
            };
            let mut snapshot = service
                .try_mft_aggregate(&canonical, 1, &|| false)
                .unwrap_or_else(|error| panic!("{}: {error}", entry.path().display()));
            if let Some((expected_files, expected_directories)) = expected {
                for _ in 0..150 {
                    if snapshot.status == SnapshotStatusV1::Complete
                        && snapshot.aggregate.file_count == expected_files
                        && snapshot.aggregate.directory_count == expected_directories
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    snapshot = service
                        .try_mft_aggregate(&canonical, 1, &|| false)
                        .unwrap_or_else(|error| panic!("{}: {error}", entry.path().display()));
                }
                assert_eq!(snapshot.status, SnapshotStatusV1::Complete);
                assert_eq!(snapshot.aggregate.file_count, expected_files);
                assert_eq!(snapshot.aggregate.directory_count, expected_directories);
            }
        }
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
    fn snapshot_codec_rejects_corruption_schema_and_oversize() {
        let root = fixture_root("codec");
        fs::create_dir_all(&root).unwrap();
        let snapshot =
            scan_recursive_reference(&root, 1, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        let bytes = encode_snapshot_bounded(&snapshot).unwrap();
        assert_eq!(decode_snapshot_bounded(bytes.as_slice()).unwrap(), snapshot);
        assert!(decode_snapshot_bounded(b"not-json".as_slice()).is_err());
        let mut wrong = snapshot;
        wrong.schema += 1;
        assert!(encode_snapshot_bounded(&wrong).is_err());
        let oversized = std::io::repeat(0).take(MAX_SNAPSHOT_BYTES_V1 + 1);
        assert!(decode_snapshot_bounded(oversized).is_err());
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
    fn leases_pin_lru_and_stale_publication_is_rejected() {
        let root = fixture_root("leases");
        fs::create_dir_all(&root).unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let key = SnapshotLeaseKeyV1 {
            canonical_root,
            refresh_generation: 4,
        };
        let snapshot =
            scan_recursive_reference(&root, 4, RecursiveSnapshotPolicyV1::default(), || false)
                .unwrap();
        let mut service = FolderSizeServiceV1::with_capacity(1);
        assert!(service.subscribe(key.clone()).is_none());
        assert!(service.publish(key.clone(), snapshot.clone()));
        assert!(service.subscribe(key.clone()).is_some());
        let mut stale = snapshot;
        stale.refresh_generation = 3;
        assert!(!service.publish(key.clone(), stale));
        assert_eq!(service.counters().stale_rejections, 1);
        service.release(&key);
        service.release(&key);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn aggregate_and_tree_consumers_share_one_physical_scan() {
        let root = fixture_root("coalesced");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("payload.bin"), vec![0_u8; 17]).unwrap();
        let mut service = FolderSizeServiceV1::with_capacity(8);
        let aggregate_consumer = service.snapshot_or_scan(&root, 12, || false).unwrap();
        let tree_consumer = service.snapshot_or_scan(&root, 12, || false).unwrap();
        assert!(Arc::ptr_eq(&aggregate_consumer, &tree_consumer));
        assert_eq!(service.counters().physical_scans, 1);
        assert_eq!(service.counters().subscribers, 2);
        assert_eq!(service.counters().cache_hits, 1);
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
        assert_eq!(service.counters().physical_scans, 1);

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(root.join("new.bin"), vec![0_u8; 5]).unwrap();
        let third = service.snapshot_or_scan(&root, 3, || false).unwrap();
        assert_eq!(third.aggregate.recursive_bytes, 22);
        assert_eq!(service.counters().physical_scans, 2);
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
    fn aggregate_results_are_not_pruned_by_depth_window() {
        let root = fixture_root("aggregate-window-share");
        let keep = root.join("keep");
        let evict = root.join("evict");
        let tree = root.join("tree");
        let child = tree.join("child");
        fs::create_dir_all(&keep).unwrap();
        fs::create_dir_all(&evict).unwrap();
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("payload.bin"), vec![0_u8; 3]).unwrap();

        let mut service = FolderSizeServiceV1::with_capacity(8);

        let keep = keep.canonicalize().unwrap();
        let evict = evict.canonicalize().unwrap();
        let tree = tree.canonicalize().unwrap();

        service
            .aggregate_or_scan(&keep, 1, false, || false, |_| {})
            .unwrap();
        service
            .aggregate_or_scan(&evict, 1, false, || false, |_| {})
            .unwrap();
        service.snapshot_or_scan(&tree, 1, || false).unwrap();

        assert!(service.modified_snapshots.contains_key(&keep));
        assert!(service.modified_snapshots.contains_key(&evict));
        assert!(service.modified_snapshots.contains_key(&tree));

        service.retain_cache_window(&keep, 1);

        assert!(service.modified_snapshots.contains_key(&keep));
        assert!(service.modified_snapshots.contains_key(&evict));
        assert!(!service.modified_snapshots.contains_key(&tree));

        let _ = fs::remove_dir_all(&root);
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

    #[test]
    fn exact_directory_facts_never_fall_back_to_recursive_scanning() {
        let root = fixture_root("mft-only-facts");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("would-be-counted.txt"), b"content").unwrap();
        let mut service = FolderSizeServiceV1::with_capacity(8);
        let method_calls = std::cell::Cell::new(0_u32);

        let error = service
            .aggregate_or_scan(
                &root,
                1,
                true,
                || false,
                |_| {
                    method_calls.set(method_calls.get() + 1);
                },
            )
            .unwrap_err();

        assert!(error.starts_with("MFT unavailable:"));
        assert_eq!(method_calls.get(), 0);
        assert_eq!(service.counters().fallback_count, 0);
        fs::remove_dir_all(root).unwrap();
    }
}
