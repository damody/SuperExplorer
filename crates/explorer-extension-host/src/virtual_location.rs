//! Host-owned, generation-bound virtual-location read adapter.

use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use explorer_extension_api::StableIdV1;

use crate::runtime_authority::{AuthorityAdapterV1, AuthorityEnvelopeV1, RuntimeAuthorityV1};

pub const MAX_VIRTUAL_LOCATION_READ_BYTES_V1: usize = 64 * 1024;
static MATERIALIZATION_NONCE_V1: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualProviderRegistrationV1 {
    pub provider_id: String,
    pub capability: String,
    pub maximum_read_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualEntryMetadataV1 {
    pub entry: StableIdV1,
    pub normalized_components: Vec<String>,
    pub uncompressed_size: u64,
    pub crc32: u32,
    pub is_container: bool,
}

/// Opaque use-time grant minted from a sealed `virtual_folder.read`
/// contribution. The grant contains no archive path or entry bytes.
#[derive(Clone)]
pub struct VirtualLocationAuthorityV1 {
    runtime: Arc<RuntimeAuthorityV1>,
    envelope: AuthorityEnvelopeV1,
}

impl std::fmt::Debug for VirtualLocationAuthorityV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VirtualLocationAuthorityV1")
            .finish_non_exhaustive()
    }
}

impl VirtualLocationAuthorityV1 {
    pub(crate) fn from_host(
        runtime: Arc<RuntimeAuthorityV1>,
        envelope: AuthorityEnvelopeV1,
    ) -> Self {
        Self { runtime, envelope }
    }

    fn revalidate(&self) -> bool {
        self.runtime
            .revalidate(&self.envelope, AuthorityAdapterV1::VirtualLocation)
            .is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtualLocationReadStatusV1 {
    Ready,
    Unauthorized,
    Stale,
    UnknownEntry,
    InvalidRange,
    ResourceLimited,
    Cancelled,
    IntegrityFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualLocationReadOutcomeV1 {
    pub status: VirtualLocationReadStatusV1,
    pub container_generation: u64,
    pub bytes: Vec<u8>,
}

/// One immutable host-attested virtual-location snapshot. Entry IDs are
/// opaque; paths, native handles, and provider objects never cross this API.
pub struct HostVirtualLocationAdapterV1 {
    authority: VirtualLocationAuthorityV1,
    container_generation: u64,
    entries: HashMap<StableIdV1, Arc<[u8]>>,
}

pub struct VirtualLocationStreamV1 {
    authority: VirtualLocationAuthorityV1,
    container_generation: u64,
    expected_generation: u64,
    bytes: Arc<[u8]>,
    expected_crc32: u32,
    position: u64,
    cancelled: Arc<AtomicBool>,
}

impl VirtualLocationStreamV1 {
    #[must_use]
    pub fn length(&self) -> u64 {
        self.bytes.len() as u64
    }

    pub fn seek(&mut self, position: u64) -> VirtualLocationReadStatusV1 {
        if !self.is_current() {
            return VirtualLocationReadStatusV1::Stale;
        }
        if position > self.length() {
            return VirtualLocationReadStatusV1::InvalidRange;
        }
        self.position = position;
        VirtualLocationReadStatusV1::Ready
    }

    pub fn read(&mut self, maximum_bytes: usize) -> VirtualLocationReadOutcomeV1 {
        let outcome = |status, bytes| VirtualLocationReadOutcomeV1 {
            status,
            container_generation: self.expected_generation,
            bytes,
        };
        if self.cancelled.load(Ordering::Acquire) {
            return outcome(VirtualLocationReadStatusV1::Cancelled, Vec::new());
        }
        if !self.authority.revalidate() {
            return outcome(VirtualLocationReadStatusV1::Unauthorized, Vec::new());
        }
        if !self.is_current() {
            return outcome(VirtualLocationReadStatusV1::Stale, Vec::new());
        }
        if maximum_bytes == 0 || maximum_bytes > MAX_VIRTUAL_LOCATION_READ_BYTES_V1 {
            return outcome(VirtualLocationReadStatusV1::ResourceLimited, Vec::new());
        }
        if crc32_v1(&self.bytes) != self.expected_crc32 {
            return outcome(VirtualLocationReadStatusV1::IntegrityFailed, Vec::new());
        }
        let start = self.position as usize;
        let end = start.saturating_add(maximum_bytes).min(self.bytes.len());
        let bytes = self.bytes[start..end].to_vec();
        self.position = end as u64;
        outcome(VirtualLocationReadStatusV1::Ready, bytes)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_current(&self) -> bool {
        self.expected_generation != 0 && self.expected_generation == self.container_generation
    }
}

pub struct MaterializedVirtualFileV1 {
    path: PathBuf,
}

impl MaterializedVirtualFileV1 {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for MaterializedVirtualFileV1 {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl HostVirtualLocationAdapterV1 {
    #[must_use]
    pub fn new(
        authority: VirtualLocationAuthorityV1,
        container_generation: u64,
        entries: impl IntoIterator<Item = (StableIdV1, Vec<u8>)>,
    ) -> Self {
        Self {
            authority,
            container_generation,
            entries: entries
                .into_iter()
                .map(|(id, bytes)| (id, Arc::<[u8]>::from(bytes)))
                .collect(),
        }
    }

    #[must_use]
    pub fn read(
        &self,
        entry: StableIdV1,
        container_generation: u64,
        offset: u64,
        maximum_bytes: usize,
    ) -> VirtualLocationReadOutcomeV1 {
        let outcome = |status, bytes| VirtualLocationReadOutcomeV1 {
            status,
            container_generation,
            bytes,
        };
        if !self.authority.revalidate() {
            return outcome(VirtualLocationReadStatusV1::Unauthorized, Vec::new());
        }
        if container_generation == 0 || container_generation != self.container_generation {
            return outcome(VirtualLocationReadStatusV1::Stale, Vec::new());
        }
        if maximum_bytes == 0 || maximum_bytes > MAX_VIRTUAL_LOCATION_READ_BYTES_V1 {
            return outcome(VirtualLocationReadStatusV1::ResourceLimited, Vec::new());
        }
        let Some(bytes) = self.entries.get(&entry) else {
            return outcome(VirtualLocationReadStatusV1::UnknownEntry, Vec::new());
        };
        let Ok(start) = usize::try_from(offset) else {
            return outcome(VirtualLocationReadStatusV1::InvalidRange, Vec::new());
        };
        if start > bytes.len() {
            return outcome(VirtualLocationReadStatusV1::InvalidRange, Vec::new());
        }
        let end = start.saturating_add(maximum_bytes).min(bytes.len());
        let copied = bytes[start..end].to_vec();
        if !self.authority.revalidate() {
            return outcome(VirtualLocationReadStatusV1::Unauthorized, Vec::new());
        }
        outcome(VirtualLocationReadStatusV1::Ready, copied)
    }

    pub fn open_stream(
        &self,
        entry: StableIdV1,
        container_generation: u64,
        expected_crc32: u32,
    ) -> Result<VirtualLocationStreamV1, VirtualLocationReadStatusV1> {
        if !self.authority.revalidate() {
            return Err(VirtualLocationReadStatusV1::Unauthorized);
        }
        if container_generation == 0 || container_generation != self.container_generation {
            return Err(VirtualLocationReadStatusV1::Stale);
        }
        let bytes = Arc::clone(
            self.entries
                .get(&entry)
                .ok_or(VirtualLocationReadStatusV1::UnknownEntry)?,
        );
        Ok(VirtualLocationStreamV1 {
            authority: self.authority.clone(),
            container_generation: self.container_generation,
            expected_generation: container_generation,
            bytes,
            expected_crc32,
            position: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn materialize(
        &self,
        entry: StableIdV1,
        container_generation: u64,
        root: &Path,
        quota_bytes: u64,
    ) -> Result<MaterializedVirtualFileV1, VirtualLocationReadStatusV1> {
        self.materialize_with_cancel(
            entry,
            container_generation,
            root,
            quota_bytes,
            &AtomicBool::new(false),
        )
    }

    pub fn materialize_with_cancel(
        &self,
        entry: StableIdV1,
        container_generation: u64,
        root: &Path,
        quota_bytes: u64,
        cancelled: &AtomicBool,
    ) -> Result<MaterializedVirtualFileV1, VirtualLocationReadStatusV1> {
        if cancelled.load(Ordering::Acquire) {
            return Err(VirtualLocationReadStatusV1::Cancelled);
        }
        if !self.authority.revalidate() {
            return Err(VirtualLocationReadStatusV1::Unauthorized);
        }
        if container_generation == 0 || container_generation != self.container_generation {
            return Err(VirtualLocationReadStatusV1::Stale);
        }
        let bytes = self
            .entries
            .get(&entry)
            .ok_or(VirtualLocationReadStatusV1::UnknownEntry)?;
        if bytes.len() as u64 > quota_bytes {
            return Err(VirtualLocationReadStatusV1::ResourceLimited);
        }
        std::fs::create_dir_all(root).map_err(|_| VirtualLocationReadStatusV1::ResourceLimited)?;
        let nonce = MATERIALIZATION_NONCE_V1.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("virtual-{nonce:016x}.tmp"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| VirtualLocationReadStatusV1::ResourceLimited)?;
        if file.write_all(bytes).is_err()
            || file.sync_all().is_err()
            || cancelled.load(Ordering::Acquire)
            || !self.authority.revalidate()
        {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(if cancelled.load(Ordering::Acquire) {
                VirtualLocationReadStatusV1::Cancelled
            } else {
                VirtualLocationReadStatusV1::Unauthorized
            });
        }
        Ok(MaterializedVirtualFileV1 { path })
    }
}

fn crc32_v1(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_authority::AuthorityClaimsV1;

    fn authority() -> VirtualLocationAuthorityV1 {
        let runtime = Arc::new(RuntimeAuthorityV1::new().unwrap());
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: "virtual-test".into(),
                feature_id: "archive".into(),
                interface_id: "browse".into(),
                incarnation: 1,
                capability: "virtual_folder.read".into(),
                authorized_root_sha256: "a".repeat(64),
                location_generation: 1,
                item_generation: 1,
                refresh_generation: 1,
                container_generation: 7,
                job_generation: 1,
            })
            .unwrap();
        VirtualLocationAuthorityV1::from_host(runtime, envelope)
    }

    #[test]
    fn bounded_read_rejects_unknown_stale_and_revoked_without_bytes() {
        let entry = StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 1);
        let authority = authority();
        let runtime = Arc::clone(&authority.runtime);
        let adapter =
            HostVirtualLocationAdapterV1::new(authority, 7, [(entry, b"abcdef".to_vec())]);
        assert_eq!(adapter.read(entry, 7, 2, 3).bytes, b"cde");
        assert_eq!(
            adapter.read(entry, 6, 0, 3).status,
            VirtualLocationReadStatusV1::Stale
        );
        assert_eq!(
            adapter
                .read(
                    StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 2),
                    7,
                    0,
                    3,
                )
                .status,
            VirtualLocationReadStatusV1::UnknownEntry
        );
        assert_eq!(runtime.revoke_feature("virtual-test", "archive"), Ok(1));
        let revoked = adapter.read(entry, 7, 0, 3);
        assert_eq!(revoked.status, VirtualLocationReadStatusV1::Unauthorized);
        assert!(revoked.bytes.is_empty());
    }

    #[test]
    fn read_bound_is_fail_closed() {
        let entry = StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 1);
        let adapter = HostVirtualLocationAdapterV1::new(authority(), 7, [(entry, vec![1])]);
        assert_eq!(
            adapter
                .read(entry, 7, 0, MAX_VIRTUAL_LOCATION_READ_BYTES_V1 + 1)
                .status,
            VirtualLocationReadStatusV1::ResourceLimited
        );
    }

    #[test]
    fn stream_is_bounded_seekable_crc_checked_cancelled_and_generation_bound() {
        let entry = StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 1);
        let bytes = b"abcdef".to_vec();
        let adapter = HostVirtualLocationAdapterV1::new(authority(), 7, [(entry, bytes.clone())]);
        let mut stream = adapter.open_stream(entry, 7, crc32_v1(&bytes)).unwrap();
        assert_eq!(stream.length(), 6);
        assert_eq!(stream.seek(2), VirtualLocationReadStatusV1::Ready);
        assert_eq!(stream.read(3).bytes, b"cde");
        stream.cancel();
        assert_eq!(
            stream.read(1).status,
            VirtualLocationReadStatusV1::Cancelled
        );
        assert!(matches!(
            adapter.open_stream(entry, 6, crc32_v1(&bytes)),
            Err(VirtualLocationReadStatusV1::Stale)
        ));
        let mut corrupt = adapter.open_stream(entry, 7, 0).unwrap();
        assert_eq!(
            corrupt.read(1).status,
            VirtualLocationReadStatusV1::IntegrityFailed
        );
    }

    #[test]
    fn materialization_obeys_quota_and_drop_cleans_the_file() {
        let entry = StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 1);
        let adapter = HostVirtualLocationAdapterV1::new(authority(), 7, [(entry, b"abc".to_vec())]);
        let root = std::env::temp_dir().join(format!(
            "superexplorer-materialize-{}-{}",
            std::process::id(),
            MATERIALIZATION_NONCE_V1.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(matches!(
            adapter.materialize(entry, 7, &root, 2),
            Err(VirtualLocationReadStatusV1::ResourceLimited)
        ));
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            adapter.materialize_with_cancel(entry, 7, &root, 3, &cancelled),
            Err(VirtualLocationReadStatusV1::Cancelled)
        ));
        assert!(!root.exists());
        let materialized = adapter.materialize(entry, 7, &root, 3).unwrap();
        let path = materialized.path().to_path_buf();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        drop(materialized);
        assert!(!path.exists());
        let _ = std::fs::remove_dir(&root);
    }
}
