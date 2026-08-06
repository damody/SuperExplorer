//! Host-owned folder aggregate/tree snapshots shared by every consumer.

use std::{
    collections::{HashMap, VecDeque},
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
    leases: HashMap<SnapshotLeaseKeyV1, usize>,
    lru: VecDeque<SnapshotLeaseKeyV1>,
    capacity: usize,
    counters: FolderSizeServiceCountersV1,
    #[cfg(windows)]
    mft_indexes: HashMap<String, Arc<crate::mft_size_map::MftIndexV1>>,
    #[cfg(windows)]
    mft_aggregates: HashMap<String, Arc<crate::mft_size_map::MftAggregateIndexV1>>,
}

impl FolderSizeServiceV1 {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            ..Self::default()
        }
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
        cancelled: impl Fn() -> bool,
        method_changed: impl Fn(SnapshotMethodV1),
    ) -> Result<(Arc<FolderSnapshotV1>, bool), String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "folder snapshot root is unavailable".to_owned())?;
        let key = SnapshotLeaseKeyV1 {
            canonical_root: canonical_root.clone(),
            refresh_generation,
        };
        let modified_stamp = folder_modified_stamp(&canonical_root)?;
        self.counters.subscribers = self.counters.subscribers.saturating_add(1);
        if let Some(snapshot) = self.snapshots.get(&key).cloned() {
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            return Ok((snapshot, true));
        }
        if let Some((cached_stamp, cached)) = self.modified_snapshots.get(&canonical_root)
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
        if let Some(mut reused) = read_persistent_snapshot(&canonical_root, modified_stamp) {
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
        if let Some(snapshot) = self.snapshots.get(&key).cloned() {
            self.counters.cache_hits = self.counters.cache_hits.saturating_add(1);
            return Ok(snapshot);
        }
        if let Some((cached_stamp, cached)) = self.modified_snapshots.get(&canonical_root)
            && *cached_stamp == modified_stamp
            && cached.status == SnapshotStatusV1::Complete
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
        if let Some(mut reused) = read_persistent_snapshot(&canonical_root, modified_stamp) {
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
    fn try_mft_snapshot(
        &mut self,
        root: &Path,
        refresh_generation: u64,
        cancelled: &impl Fn() -> bool,
    ) -> Result<FolderSnapshotV1, String> {
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
        if !self.mft_indexes.contains_key(&volume) {
            let service_index = fresh_service_mft_index(&canonical_root);
            match service_index {
                Ok(index) => {
                    self.mft_indexes.insert(volume.clone(), Arc::new(index));
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }
        let index = self.mft_indexes[&volume].clone();
        let root_reference = crate::mft_size_map::file_reference_number(&canonical_root)?;
        let projected = index.project_subtree(root_reference, DEFAULT_MAX_NODES_V1, cancelled)?;
        let mut paths = HashMap::from([(root_reference, canonical_root.clone())]);
        let mut entries = Vec::with_capacity(projected.len().saturating_sub(1));
        for node in projected.into_iter().skip(1) {
            let parent = node
                .parent_reference
                .and_then(|reference| paths.get(&reference))
                .ok_or_else(|| "MFT projection parent is missing".to_owned())?;
            let path = parent.join(&node.name);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| "MFT projection contains a stale path".to_owned())?;
            if is_reparse_point(&metadata)
                || (!node.is_directory && crate::mft_size_map::file_link_count(&path)? > 1)
            {
                return Err(
                    "MFT projection requires recursive hard-link/reparse fallback".to_owned(),
                );
            }
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
        cancelled: &impl Fn() -> bool,
    ) -> Result<FolderSnapshotV1, String> {
        use std::path::Component;
        let volume = root
            .components()
            .find_map(|component| match component {
                Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().to_string()),
                _ => None,
            })
            .ok_or_else(|| "MFT requires a local volume".to_owned())?;
        if !self.mft_indexes.contains_key(&volume) {
            let index = fresh_service_mft_index(root);
            match index {
                Ok(index) => {
                    self.mft_indexes.insert(volume.clone(), Arc::new(index));
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }
        if !self.mft_aggregates.contains_key(&volume) {
            let aggregate =
                crate::mft_size_map::MftAggregateIndexV1::build(&self.mft_indexes[&volume], 8)?;
            self.mft_aggregates
                .insert(volume.clone(), Arc::new(aggregate));
        }
        let root_reference = crate::mft_size_map::file_reference_number(root)?;
        let aggregate = self.mft_aggregates[&volume]
            .get(root_reference)
            .ok_or_else(|| "MFT aggregate root is unavailable".to_owned())?;
        let root_id = stable_node_id(Path::new(""));
        Ok(FolderSnapshotV1 {
            schema: SNAPSHOT_SCHEMA_V2,
            root_id,
            refresh_generation,
            method: SnapshotMethodV1::Mft,
            status: SnapshotStatusV1::Complete,
            diagnostic: None,
            aggregate: FolderAggregateSnapshotV1 {
                recursive_bytes: aggregate.logical_bytes,
                direct_bytes: 0,
                file_count: aggregate.file_count,
                directory_count: aggregate.directory_count,
                status: SnapshotStatusV1::Complete,
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
                status: SnapshotStatusV1::Complete,
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
        }
    }
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
fn fresh_service_mft_index(root: &Path) -> Result<crate::mft_size_map::MftIndexV1, String> {
    use std::path::{Component, Prefix};
    use std::time::{Duration, SystemTime};

    let letter = root
        .components()
        .find_map(|component| match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    Some(char::from(letter).to_ascii_uppercase())
                }
                _ => None,
            },
            _ => None,
        })
        .ok_or_else(|| "MFT service cache requires a drive letter".to_owned())?;
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let path = program_data
        .join("SuperExplorer")
        .join("MftIndex")
        .join(format!("{letter}.semftidx"));
    let modified = fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .map_err(|_| "MFT service cache is unavailable".to_owned())?;
    if SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::MAX)
        > Duration::from_secs(120)
    {
        return Err("MFT service cache is stale".to_owned());
    }
    let mut last_error = "MFT service cache is unavailable".to_owned();
    for attempt in 0..5 {
        match crate::mft_size_map::read_index(&path) {
            Ok(index) => return Ok(index),
            Err(error) => last_error = error,
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    Err(last_error)
}

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
            service
                .try_mft_aggregate(&entry.path().canonicalize().unwrap(), 1, &|| false)
                .unwrap_or_else(|error| panic!("{}: {error}", entry.path().display()));
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
}
