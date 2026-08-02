//! Fail-closed package-directory and Ed25519 signature validation.
//!
//! Validation is completed before a package can reach a loader or registrar. It
//! rejects unsafe lexical paths before filesystem access, checks every traversed
//! component for symlink/reparse-point indirection, hashes bytes from the opened
//! file handle, and rechecks the final path after hashing. Callers must provide
//! an immutable, source-owned package root; this double-checking narrows, but no
//! path-based API can eliminate, a concurrent replacement race by a writer that
//! controls that root.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::signature::{ED25519, UnparsedPublicKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    PackageManifestErrorV1, PackageManifestV1, PayloadKindV1, PayloadV1, SignatureV1,
    VerifiedPublisherIdentityV1,
};

const ED25519_PUBLIC_KEY_BYTES: usize = 32;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const MAX_PAYLOAD_COUNT: usize = 128;
const MAX_PAYLOAD_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PAYLOAD_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_MANIFEST_FILE_BYTES: usize = 256 * 1024;
const MAX_PACKAGE_DEPTH: usize = 32;
const MAX_PACKAGE_ENTRY_COUNT: usize = 1024;
const DEFAULT_VALIDATION_TIMEOUT: Duration = Duration::from_secs(30);
const MINIMUM_STAGING_AGE: Duration = Duration::from_mins(15);
const MAX_STAGING_ROOT_ENTRY_SCAN: usize = 256;
const STAGING_SCAVENGE_TIMEOUT: Duration = Duration::from_secs(1);
static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Opaque local-developer authorization issued only by a host package source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalDeveloperAuthorizationV1(());

impl LocalDeveloperAuthorizationV1 {
    #[allow(dead_code)] // Task 2.6 package sources issue this authorization.
    pub(crate) const fn issue() -> Self {
        Self(())
    }
}

/// Bounded time and content budget for one validation operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageValidationBudgetV1 {
    deadline: Option<Instant>,
    cancellation: Option<PackageValidationCancellationV1>,
}

impl Default for PackageValidationBudgetV1 {
    fn default() -> Self {
        Self {
            deadline: Some(Instant::now() + DEFAULT_VALIDATION_TIMEOUT),
            cancellation: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PackageValidationCancellationV1(Arc<AtomicBool>);

impl PartialEq for PackageValidationCancellationV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PackageValidationCancellationV1 {}
impl PackageValidationCancellationV1 {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl PackageValidationBudgetV1 {
    #[must_use]
    pub const fn with_deadline(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
            cancellation: None,
        }
    }

    #[must_use]
    pub fn with_cancellation(mut self, token: PackageValidationCancellationV1) -> Self {
        self.cancellation = Some(token);
        self
    }

    fn check(&self) -> Result<(), PackageValidationErrorV1> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(PackageValidationCancellationV1::cancelled)
        {
            Err(PackageValidationErrorV1::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(PackageValidationErrorV1::DeadlineExceeded)
        } else {
            Ok(())
        }
    }
}

/// Inputs supplied by the host package source for one pre-load validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageValidationRequestV1 {
    source_package_root: PathBuf,
    local_developer_authorization: Option<LocalDeveloperAuthorizationV1>,
    budget: PackageValidationBudgetV1,
}

impl PackageValidationRequestV1 {
    /// Creates a fail-closed request that requires a valid publisher signature.
    #[must_use]
    pub fn new(source_package_root: PathBuf) -> Self {
        Self {
            source_package_root,
            local_developer_authorization: None,
            budget: PackageValidationBudgetV1::default(),
        }
    }

    #[must_use]
    pub fn with_budget(mut self, budget: PackageValidationBudgetV1) -> Self {
        self.budget = budget;
        self
    }

    #[must_use]
    pub fn with_local_developer_authorization(
        mut self,
        authorization: LocalDeveloperAuthorizationV1,
    ) -> Self {
        self.local_developer_authorization = Some(authorization);
        self
    }
}

/// Opaque host-owned content-addressed store for validated package generations.
#[derive(Clone)]
pub struct SealedPackageStoreV1 {
    root: Arc<PathBuf>,
}

impl fmt::Debug for SealedPackageStoreV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SealedPackageStoreV1 { root: <redacted> }")
    }
}

impl SealedPackageStoreV1 {
    /// Creates and safely scavenges the host-owned sealed package store.
    ///
    /// # Errors
    ///
    /// Returns an error if the root or a candidate staging tree is unsafe.
    pub fn new(root: &Path) -> Result<Self, PackageValidationErrorV1> {
        fs::create_dir_all(root).map_err(|source| PackageValidationErrorV1::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let root = verify_safe_root(root)?;
        scavenge_staging_directories(&root)?;
        Ok(Self {
            root: Arc::new(root),
        })
    }
}

/// Immutable host-supplied mapping from a signing key to its publisher identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedPublisherKeyV1 {
    key_id: String,
    publisher_id: String,
    ed25519_public_key: [u8; ED25519_PUBLIC_KEY_BYTES],
}

impl TrustedPublisherKeyV1 {
    /// Creates one trusted key entry from host configuration, never from a manifest.
    ///
    /// # Errors
    ///
    /// Returns a typed error for non-normalized identities or a non-Ed25519 key length.
    pub fn new(
        key_id: String,
        publisher_id: String,
        ed25519_public_key: &[u8],
    ) -> Result<Self, PackageValidationErrorV1> {
        if !is_normalized_id(&key_id) {
            return Err(PackageValidationErrorV1::InvalidTrustedKeyIdentifier { key_id });
        }
        if !is_normalized_id(&publisher_id) {
            return Err(
                PackageValidationErrorV1::InvalidTrustedPublisherIdentifier { publisher_id },
            );
        }
        let ed25519_public_key: [u8; ED25519_PUBLIC_KEY_BYTES] =
            ed25519_public_key.try_into().map_err(|_| {
                PackageValidationErrorV1::InvalidTrustedPublicKeyLength {
                    actual: ed25519_public_key.len(),
                }
            })?;
        Ok(Self {
            key_id,
            publisher_id,
            ed25519_public_key,
        })
    }
}

/// Immutable host trust store used to verify package signatures.
#[derive(Clone, Debug, Default)]
pub struct TrustedPublisherKeyStoreV1 {
    keys: BTreeMap<String, TrustedPublisherKeyV1>,
}

impl TrustedPublisherKeyStoreV1 {
    /// Builds a trust store and rejects duplicate signing key IDs deterministically.
    ///
    /// # Errors
    ///
    /// Returns an error when more than one host-supplied entry uses the same key ID.
    pub fn new(
        keys: impl IntoIterator<Item = TrustedPublisherKeyV1>,
    ) -> Result<Self, PackageValidationErrorV1> {
        let mut store = Self::default();
        for key in keys {
            if store.keys.insert(key.key_id.clone(), key.clone()).is_some() {
                return Err(PackageValidationErrorV1::DuplicateTrustedKeyIdentifier {
                    key_id: key.key_id,
                });
            }
        }
        Ok(store)
    }

    fn resolve(&self, key_id: &str) -> Option<&TrustedPublisherKeyV1> {
        self.keys.get(key_id)
    }
}

/// Pre-load validator using a host-owned immutable trust store.
#[derive(Clone, Debug)]
pub struct PackageValidatorV1 {
    trusted_keys: TrustedPublisherKeyStoreV1,
    sealed_store: SealedPackageStoreV1,
}

impl PackageValidatorV1 {
    /// Creates a validator from the host's trusted publisher-key configuration.
    #[must_use]
    pub fn new(
        trusted_keys: TrustedPublisherKeyStoreV1,
        sealed_store: SealedPackageStoreV1,
    ) -> Self {
        Self {
            trusted_keys,
            sealed_store,
        }
    }

    /// Validates package content, target, and signature before any callback may run.
    ///
    /// # Errors
    ///
    /// Returns a typed failure for unsafe paths, content disagreement, I/O, target,
    /// trust-store, or Ed25519 signature verification failures.
    pub fn validate(
        &self,
        request: &PackageValidationRequestV1,
    ) -> Result<PackageValidationResultV1, PackageValidationErrorV1> {
        request.budget.check()?;
        let (manifest, manifest_bytes) = read_manifest_from_source(request)?;
        let verified_publisher_id = self.authenticate_manifest(&manifest, request)?;
        if manifest.sdk.target != host_target_v1() {
            return Err(PackageValidationErrorV1::TargetMismatch {
                manifest_target: manifest.sdk.target.clone(),
                expected_target: host_target_v1().to_owned(),
            });
        }

        let payloads = validate_payload_inventory(&manifest, &request.budget)?;
        validate_manifest_references(&manifest, &payloads)?;
        validate_tool_targets(&manifest)?;
        verify_payload_files(&request.source_package_root, &payloads, &request.budget)?;
        verify_no_unlisted_content(&request.source_package_root, &payloads, &request.budget)?;

        let canonical_manifest_bytes = manifest
            .canonical_serialized_bytes()
            .map_err(PackageValidationErrorV1::Manifest)?;
        let generation_id = hex_sha256(&Sha256::digest(&canonical_manifest_bytes).into());
        let sealed_package_root = seal_generation(
            &self.sealed_store,
            request,
            &canonical_manifest_bytes,
            &payloads,
            &generation_id,
        )?;
        request.budget.check()?;
        let sealed_generation = SealedPackageGenerationV1::new(
            self.sealed_store.clone(),
            sealed_package_root,
            canonical_manifest_bytes,
            &payloads,
        );
        Ok(PackageValidationResultV1 {
            verified_publisher_id,
            manifest_digest: hex_sha256(&Sha256::digest(&manifest_bytes).into()),
            data_version: manifest.data_version,
            manifest,
            sealed_generation,
        })
    }

    fn authenticate_manifest(
        &self,
        manifest: &PackageManifestV1,
        request: &PackageValidationRequestV1,
    ) -> Result<Option<String>, PackageValidationErrorV1> {
        request.budget.check()?;
        Ok(match &manifest.signature {
            SignatureV1::Unsigned => {
                if request.local_developer_authorization.is_some() {
                    None
                } else {
                    return Err(PackageValidationErrorV1::SignatureRequired);
                }
            }
            SignatureV1::Ed25519 { key_id, signature } => {
                let key = self.trusted_keys.resolve(key_id).ok_or_else(|| {
                    PackageValidationErrorV1::UnknownSigningKey {
                        key_id: key_id.clone(),
                    }
                })?;
                let signature = STANDARD.decode(signature).map_err(|_| {
                    PackageValidationErrorV1::InvalidSignatureEncoding {
                        key_id: key_id.clone(),
                    }
                })?;
                let message = manifest
                    .canonical_ed25519_signing_bytes()
                    .map_err(PackageValidationErrorV1::Manifest)?;
                UnparsedPublicKey::new(&ED25519, key.ed25519_public_key)
                    .verify(&message, &signature)
                    .map_err(|_| PackageValidationErrorV1::InvalidSignature {
                        key_id: key_id.clone(),
                    })?;
                let identity = VerifiedPublisherIdentityV1::new(key.publisher_id.clone())
                    .map_err(PackageValidationErrorV1::Manifest)?;
                manifest
                    .validate_verified_signer_publisher_identity(&identity)
                    .map_err(PackageValidationErrorV1::Manifest)?;
                Some(key.publisher_id.clone())
            }
        })
    }
}

/// Successful pre-load validation result.
#[derive(Clone)]
pub struct PackageValidationResultV1 {
    /// Publisher identity established by real Ed25519 verification, if signed.
    pub verified_publisher_id: Option<String>,
    /// SHA-256 of the exact validated source `manifest.json` bytes.
    pub manifest_digest: String,
    /// Package data generation declared by the sealed manifest.
    pub data_version: u64,
    /// Manifest accepted with the sealed generation.  It remains crate-private
    /// so only host stages receiving this validation result can resolve it.
    manifest: PackageManifestV1,
    sealed_generation: SealedPackageGenerationV1,
}

impl fmt::Debug for PackageValidationResultV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PackageValidationResultV1")
            .field("verified_publisher_id", &self.verified_publisher_id)
            .field("manifest_digest", &self.manifest_digest)
            .field("data_version", &self.data_version)
            .field("manifest", &"<sealed>")
            .field("sealed_generation", &"<redacted>")
            .finish()
    }
}

impl PackageValidationResultV1 {
    /// Returns the manifest bound to this sealed validation result.
    #[must_use]
    pub(crate) const fn manifest(&self) -> &PackageManifestV1 {
        &self.manifest
    }

    /// Opens an immutable activation guard after revalidating the sealed generation.
    ///
    /// # Errors
    ///
    /// Returns a typed validation or I/O error if sealed bytes no longer match.
    pub fn activation_guard(
        &self,
    ) -> Result<SealedPackageActivationGuardV1, PackageValidationErrorV1> {
        self.sealed_generation
            .open_guard(&PackageValidationBudgetV1::default())
    }

    /// Opens an activation guard with an explicit cancellation/deadline budget.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, cancellation, deadline, or I/O error.
    pub fn activation_guard_with_budget(
        &self,
        budget: &PackageValidationBudgetV1,
    ) -> Result<SealedPackageActivationGuardV1, PackageValidationErrorV1> {
        self.sealed_generation.open_guard(budget)
    }

    #[cfg(test)]
    pub(crate) fn for_resolver_test(manifest: PackageManifestV1) -> Self {
        let canonical_manifest_bytes = manifest
            .canonical_serialized_bytes()
            .expect("test manifest serializes");
        Self {
            verified_publisher_id: None,
            manifest_digest: String::new(),
            data_version: manifest.data_version,
            manifest,
            sealed_generation: SealedPackageGenerationV1 {
                store: SealedPackageStoreV1 {
                    root: Arc::new(PathBuf::new()),
                },
                root: PathBuf::new(),
                canonical_manifest_bytes: Arc::new(canonical_manifest_bytes),
                payloads: Arc::new(Vec::new()),
            },
        }
    }
}

#[derive(Clone)]
struct SealedPackageGenerationV1 {
    store: SealedPackageStoreV1,
    root: PathBuf,
    canonical_manifest_bytes: Arc<Vec<u8>>,
    payloads: Arc<Vec<SealedPayloadV1>>,
}

impl fmt::Debug for SealedPackageGenerationV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SealedPackageGenerationV1 { root: <redacted> }")
    }
}

#[derive(Clone, Debug)]
struct SealedPayloadV1 {
    normalized: String,
    size: u64,
    sha256: String,
}

impl SealedPackageGenerationV1 {
    fn new(
        store: SealedPackageStoreV1,
        root: PathBuf,
        canonical_manifest_bytes: Vec<u8>,
        payloads: &BTreeMap<String, (&PayloadV1, String)>,
    ) -> Self {
        Self {
            store,
            root,
            canonical_manifest_bytes: Arc::new(canonical_manifest_bytes),
            payloads: Arc::new(
                payloads
                    .values()
                    .map(|(payload, normalized)| SealedPayloadV1 {
                        normalized: normalized.clone(),
                        size: payload.size,
                        sha256: payload.sha256.clone(),
                    })
                    .collect(),
            ),
        }
    }

    fn open_guard(
        &self,
        budget: &PackageValidationBudgetV1,
    ) -> Result<SealedPackageActivationGuardV1, PackageValidationErrorV1> {
        budget.check()?;
        let root = verify_safe_root(&self.root)?;
        if root.parent() != Some(self.store.root.as_path()) {
            return Err(PackageValidationErrorV1::SealedGenerationMismatch {
                path: self.root.clone(),
            });
        }
        let declared = self
            .payloads
            .iter()
            .map(|payload| payload.normalized.to_ascii_lowercase())
            .collect();
        let directory_leases = acquire_directory_leases(&root, budget)?;
        // A directory can be populated after its first inventory scan but before
        // activation. Holding a read-only lease for every existing directory and
        // then scanning closes that window: subsequent late injection, rename,
        // or deletion is rejected by Windows sharing semantics until guard drop.
        scan_package_directory(&root, &declared, budget)?;
        let manifest = open_exclusive_safe_file(&root, "manifest.json")?;
        let manifest_bytes = read_limited_file(
            &manifest,
            &root.join("manifest.json"),
            MAX_MANIFEST_FILE_BYTES,
            budget,
        )?;
        if manifest_bytes != *self.canonical_manifest_bytes {
            return Err(PackageValidationErrorV1::SealedGenerationMismatch {
                path: self.root.clone(),
            });
        }
        let mut payload_files = BTreeMap::new();
        for payload in self.payloads.iter() {
            budget.check()?;
            let mut file = open_exclusive_safe_file(&root, &payload.normalized)?;
            let digest = sha256_file(
                &mut file,
                &root.join(&payload.normalized),
                payload.size,
                budget,
            )?;
            if hex_sha256(&digest) != payload.sha256 {
                return Err(PackageValidationErrorV1::PayloadHashMismatch {
                    path: payload.normalized.clone(),
                });
            }
            file.seek(SeekFrom::Start(0))
                .map_err(|source| PackageValidationErrorV1::Io {
                    path: root.join(&payload.normalized),
                    source,
                })?;
            verify_existing_relative_path(&root, &payload.normalized, true)?;
            payload_files.insert(payload.normalized.clone(), file);
        }
        budget.check()?;
        Ok(SealedPackageActivationGuardV1 {
            root,
            _directory_leases: directory_leases,
            _manifest: manifest,
            payload_files,
        })
    }
}

/// Opaque immutable lease that a future host loader consumes for activation.
pub struct SealedPackageActivationGuardV1 {
    #[allow(dead_code)] // Consumed by the host's future DLL loader.
    root: PathBuf,
    _directory_leases: Vec<DirectoryLease>,
    _manifest: File,
    payload_files: BTreeMap<String, File>,
}

impl fmt::Debug for SealedPackageActivationGuardV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedPackageActivationGuardV1")
            .field("root", &"<redacted>")
            .field("payload_file_count", &self.payload_files.len())
            .finish_non_exhaustive()
    }
}

impl SealedPackageActivationGuardV1 {
    #[must_use]
    #[allow(dead_code)] // Consumed by the host's future DLL loader.
    pub(crate) fn payload_file(&self, normalized_path: &str) -> Option<&File> {
        self.payload_files.get(normalized_path)
    }

    #[must_use]
    #[allow(dead_code)] // Consumed by the host's future DLL loader.
    pub(crate) fn package_root(&self) -> &Path {
        &self.root
    }
}

/// A Windows namespace lease for an existing sealed-generation directory.
///
/// The handle intentionally permits other readers while withholding write and
/// delete sharing. A loader can therefore read the declared files, but another
/// process cannot inject an extra DLL/file or replace a directory entry while
/// the activation guard is alive.
struct DirectoryLease {
    #[cfg(windows)]
    handle: isize,
}

impl fmt::Debug for DirectoryLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DirectoryLease(<redacted>)")
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
impl Drop for DirectoryLease {
    fn drop(&mut self) {
        // SAFETY: `handle` originates from a successful CreateFileW call and
        // is owned exclusively by this lease.
        unsafe {
            let _ = close_handle(self.handle);
        }
    }
}

#[cfg(not(windows))]
impl Drop for DirectoryLease {
    fn drop(&mut self) {}
}

fn acquire_directory_leases(
    root: &Path,
    budget: &PackageValidationBudgetV1,
) -> Result<Vec<DirectoryLease>, PackageValidationErrorV1> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut leases = Vec::new();
    let mut entry_count = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        budget.check()?;
        leases.push(open_directory_lease(&directory)?);
        for entry in fs::read_dir(&directory).map_err(|source| PackageValidationErrorV1::Io {
            path: directory.clone(),
            source,
        })? {
            budget.check()?;
            entry_count = entry_count.checked_add(1).ok_or(
                PackageValidationErrorV1::PackageEntryCountExceeded {
                    maximum: MAX_PACKAGE_ENTRY_COUNT,
                },
            )?;
            if entry_count > MAX_PACKAGE_ENTRY_COUNT {
                return Err(PackageValidationErrorV1::PackageEntryCountExceeded {
                    maximum: MAX_PACKAGE_ENTRY_COUNT,
                });
            }
            let entry = entry.map_err(|source| PackageValidationErrorV1::Io {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| PackageValidationErrorV1::Io {
                    path: path.clone(),
                    source,
                })?;
            if metadata_is_reparse_point(&metadata) {
                return Err(PackageValidationErrorV1::ReparsePointPath { path });
            }
            if metadata.is_dir() {
                let child_depth = depth + 1;
                if child_depth > MAX_PACKAGE_DEPTH {
                    return Err(PackageValidationErrorV1::PackageDepthExceeded {
                        path,
                        maximum: MAX_PACKAGE_DEPTH,
                    });
                }
                pending.push((path, child_depth));
            }
        }
    }
    Ok(leases)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_directory_lease(path: &Path) -> Result<DirectoryLease, PackageValidationErrorV1> {
    let handle = open_directory_handle(path)?;
    if let Err(error) = harden_sealed_directory_namespace(handle, path) {
        unsafe {
            let _ = close_handle(handle);
        }
        return Err(error);
    }
    Ok(DirectoryLease { handle })
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_directory_handle(path: &Path) -> Result<isize, PackageValidationErrorV1> {
    use std::{ffi::c_void, iter, os::windows::ffi::OsStrExt as _};

    const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
    const READ_CONTROL: u32 = 0x0002_0000;
    const WRITE_DAC: u32 = 0x0004_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const INVALID_HANDLE_VALUE: isize = -1;
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    // SAFETY: the NUL-terminated path points to valid memory for this call; no
    // security attributes, template handle, or overlapped I/O is supplied.
    let handle = unsafe {
        create_file_w(
            wide_path.as_ptr(),
            FILE_LIST_DIRECTORY | READ_CONTROL | WRITE_DAC,
            FILE_SHARE_READ,
            std::ptr::null_mut::<c_void>(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(PackageValidationErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::last_os_error(),
        });
    }
    Ok(handle)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn directory_handle_identity(
    handle: isize,
    path: &Path,
) -> Result<(u32, u64), PackageValidationErrorV1> {
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;

    #[repr(C)]
    struct FileTime {
        low_date_time: u32,
        high_date_time: u32,
    }
    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    let mut info = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    // SAFETY: `info` provides writable storage for the Win32 output structure.
    if unsafe { get_file_information_by_handle(handle, info.as_mut_ptr().cast()) } == 0 {
        return Err(PackageValidationErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::last_os_error(),
        });
    }
    // SAFETY: GetFileInformationByHandle reported success and initialized info.
    let info = unsafe { info.assume_init() };
    if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || info.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(PackageValidationErrorV1::UnsafePackageRoot {
            path: path.to_path_buf(),
        });
    }
    Ok((
        info.volume_serial_number,
        u64::from(info.file_index_high) << 32 | u64::from(info.file_index_low),
    ))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn harden_sealed_directory_namespace(
    handle: isize,
    path: &Path,
) -> Result<(), PackageValidationErrorV1> {
    // Do not cache this by path. A generation directory can be deleted and
    // recreated at the same spelling after an earlier guard drops; its DACL is
    // then a new security identity and must be hardened again. Re-applying the
    // DENY ACE is safe (Windows merges equivalent deny entries) and fail-closed.
    let _ = directory_handle_identity(handle, path)?;
    deny_directory_mutation_for_everyone(handle, path)?;
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn deny_directory_mutation_for_everyone(
    handle: isize,
    path: &Path,
) -> Result<(), PackageValidationErrorV1> {
    use std::ffi::c_void;

    const SE_FILE_OBJECT: u32 = 1;
    const DACL_SECURITY_INFORMATION: u32 = 0x0000_0004;
    const DENY_ACCESS: u32 = 3;
    const NO_INHERITANCE: u32 = 0;
    const TRUSTEE_IS_SID: u32 = 0;
    const TRUSTEE_IS_WELL_KNOWN_GROUP: u32 = 5;
    const WIN_WORLD_SID: u32 = 1;
    const FILE_ADD_FILE: u32 = 0x0000_0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0000_0004;

    #[repr(C)]
    struct TrusteeW {
        multiple_trustee: *mut c_void,
        multiple_trustee_operation: u32,
        trustee_form: u32,
        trustee_type: u32,
        name: *mut u16,
    }

    #[repr(C)]
    struct ExplicitAccessW {
        access_permissions: u32,
        access_mode: u32,
        inheritance: u32,
        trustee: TrusteeW,
    }

    let mut existing_dacl = std::ptr::null_mut::<c_void>();
    let mut security_descriptor = std::ptr::null_mut::<c_void>();
    // SAFETY: output pointers are initialized writable storage and `handle` is
    // the exact directory object opened with READ_CONTROL/WRITE_DAC.
    let status = unsafe {
        get_security_info(
            handle,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut existing_dacl,
            std::ptr::null_mut(),
            &raw mut security_descriptor,
        )
    };
    if status != 0 {
        return Err(PackageValidationErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::from_raw_os_error(status.cast_signed()),
        });
    }

    // `SECURITY_MAX_SID_SIZE` is 68 bytes on Windows.
    let mut everyone_sid = [0_u8; 68];
    let mut sid_size = 68_u32;
    // SAFETY: `everyone_sid` is writable and `sid_size` describes its capacity.
    if unsafe {
        create_well_known_sid(
            WIN_WORLD_SID,
            std::ptr::null_mut(),
            everyone_sid.as_mut_ptr().cast(),
            &raw mut sid_size,
        )
    } == 0
    {
        unsafe {
            let _ = local_free(security_descriptor);
        }
        return Err(PackageValidationErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::last_os_error(),
        });
    }

    let mut access = ExplicitAccessW {
        access_permissions: FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY,
        access_mode: DENY_ACCESS,
        inheritance: NO_INHERITANCE,
        trustee: TrusteeW {
            multiple_trustee: std::ptr::null_mut(),
            multiple_trustee_operation: 0,
            trustee_form: TRUSTEE_IS_SID,
            trustee_type: TRUSTEE_IS_WELL_KNOWN_GROUP,
            name: everyone_sid.as_mut_ptr().cast(),
        },
    };
    let mut hardened_dacl = std::ptr::null_mut::<c_void>();
    // SAFETY: `access` and the old DACL are valid for this call; Windows
    // allocates `hardened_dacl`, which is released below with LocalFree.
    let status = unsafe {
        set_entries_in_acl_w(
            1,
            std::ptr::addr_of_mut!(access).cast(),
            existing_dacl,
            &raw mut hardened_dacl,
        )
    };
    if status != 0 {
        unsafe {
            let _ = local_free(security_descriptor);
        }
        return Err(PackageValidationErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::from_raw_os_error(status.cast_signed()),
        });
    }
    // SAFETY: this applies the newly allocated DACL to the exact directory
    // handle, not a path that may have been swapped. It deliberately persists:
    // content-addressed generations are immutable once activated.
    let status = unsafe {
        set_security_info(
            handle,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hardened_dacl,
            std::ptr::null_mut(),
        )
    };
    unsafe {
        let _ = local_free(hardened_dacl);
        let _ = local_free(security_descriptor);
    }
    if status != 0 {
        return Err(PackageValidationErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::from_raw_os_error(status.cast_signed()),
        });
    }
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "CreateFileW"]
    fn create_file_w(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut std::ffi::c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: isize,
    ) -> isize;
    #[link_name = "CloseHandle"]
    fn close_handle(handle: isize) -> i32;
    #[link_name = "LocalFree"]
    fn local_free(memory: *mut std::ffi::c_void) -> isize;
    #[link_name = "GetFileInformationByHandle"]
    fn get_file_information_by_handle(handle: isize, info: *mut std::ffi::c_void) -> i32;
}

#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "advapi32")]
unsafe extern "system" {
    #[link_name = "GetSecurityInfo"]
    fn get_security_info(
        handle: isize,
        object_type: u32,
        security_info: u32,
        owner: *mut *mut std::ffi::c_void,
        group: *mut *mut std::ffi::c_void,
        dacl: *mut *mut std::ffi::c_void,
        sacl: *mut *mut std::ffi::c_void,
        security_descriptor: *mut *mut std::ffi::c_void,
    ) -> u32;
    #[link_name = "SetEntriesInAclW"]
    fn set_entries_in_acl_w(
        count: u32,
        entries: *mut std::ffi::c_void,
        old_acl: *mut std::ffi::c_void,
        new_acl: *mut *mut std::ffi::c_void,
    ) -> u32;
    #[link_name = "SetSecurityInfo"]
    fn set_security_info(
        handle: isize,
        object_type: u32,
        security_info: u32,
        owner: *mut std::ffi::c_void,
        group: *mut std::ffi::c_void,
        dacl: *mut std::ffi::c_void,
        sacl: *mut std::ffi::c_void,
    ) -> u32;
    #[link_name = "CreateWellKnownSid"]
    fn create_well_known_sid(
        well_known_sid_type: u32,
        domain_sid: *mut std::ffi::c_void,
        sid: *mut std::ffi::c_void,
        sid_size: *mut u32,
    ) -> i32;
}

#[cfg(not(windows))]
fn open_directory_lease(path: &Path) -> Result<DirectoryLease, PackageValidationErrorV1> {
    Err(PackageValidationErrorV1::Io {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::Unsupported,
            "sealed activation requires Windows directory leases",
        ),
    })
}

/// Typed package pre-load validation failure.
#[derive(Error)]
pub enum PackageValidationErrorV1 {
    /// The decoded manifest failed a required invariant while preparing validation.
    #[error(transparent)]
    Manifest(#[from] PackageManifestErrorV1),
    #[error("manifest target {manifest_target:?} does not match host target {expected_target:?}")]
    TargetMismatch {
        manifest_target: String,
        expected_target: String,
    },
    #[error(
        "tool {tool_id:?} target {manifest_target:?} does not match host tool target {expected_target:?}"
    )]
    ToolTargetMismatch {
        tool_id: String,
        manifest_target: String,
        expected_target: String,
    },
    #[error("tool {tool_id:?} path {path:?} must be beneath {required_prefix:?}")]
    ToolPathLayoutMismatch {
        tool_id: String,
        path: String,
        required_prefix: String,
    },
    #[error("manifest declares {actual} payloads, exceeding the {maximum}-payload limit")]
    PayloadCountExceeded { actual: usize, maximum: usize },
    #[error("payload {path:?} is {actual} bytes, exceeding the {maximum}-byte per-file limit")]
    PayloadFileTooLarge {
        path: String,
        actual: u64,
        maximum: u64,
    },
    #[error("declared payload total is {actual} bytes, exceeding the {maximum}-byte limit")]
    PayloadTotalBytesExceeded { actual: u64, maximum: u64 },
    #[error("package validation deadline elapsed")]
    DeadlineExceeded,
    #[error("package validation was cancelled")]
    Cancelled,
    #[error("manifest.json is missing, invalid, or exceeds its fixed byte limit")]
    InvalidManifestFile,
    #[error("package path at {field} is unsafe: {path:?}")]
    UnsafePackagePath { field: &'static str, path: String },
    #[error("duplicate or case-colliding package path: {path:?}")]
    DuplicateOrCaseCollidingPath { path: String },
    #[error("manifest reference {path:?} is missing from payload inventory")]
    MissingPayloadReference { path: String },
    #[error("manifest reference {path:?} has payload kind {actual:?}, expected {expected:?}")]
    PayloadKindMismatch {
        path: String,
        actual: PayloadKindV1,
        expected: PayloadKindV1,
    },
    #[error("tool {tool_id:?} size does not match its payload declaration")]
    ToolPayloadSizeMismatch { tool_id: String },
    #[error("tool {tool_id:?} hash does not match its payload declaration")]
    ToolPayloadHashMismatch { tool_id: String },
    #[error("locale {locale:?} hash does not match its payload declaration")]
    LocalePayloadHashMismatch { locale: String },
    #[error("package root {path} is not a safe directory")]
    UnsafePackageRoot { path: PathBuf },
    #[error("package path {path} includes a symlink, junction, or reparse point")]
    ReparsePointPath { path: PathBuf },
    #[error("package path {path} escapes the package root")]
    PackagePathEscapesRoot { path: PathBuf },
    #[error("package payload {path} is not a regular file")]
    NotRegularFile { path: PathBuf },
    #[error("package contains an undeclared file: {path}")]
    UnlistedPackageContent { path: PathBuf },
    #[error("package contains a directory that is not a prefix of a declared payload: {path}")]
    UndeclaredPackageDirectory { path: PathBuf },
    #[error("package directory depth exceeds the {maximum}-level limit: {path}")]
    PackageDepthExceeded { path: PathBuf, maximum: usize },
    #[error("package contains more than the {maximum}-entry traversal limit")]
    PackageEntryCountExceeded { maximum: usize },
    #[error("existing sealed package generation does not match the validated generation: {path}")]
    SealedGenerationMismatch { path: PathBuf },
    #[error("could not clean package-sealing staging directory {path}: {source}")]
    StagingCleanupFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not access package path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("payload {path:?} size is {actual}, expected {expected}")]
    PayloadSizeMismatch {
        path: String,
        actual: u64,
        expected: u64,
    },
    #[error("payload {path:?} SHA-256 digest does not match its manifest declaration")]
    PayloadHashMismatch { path: String },
    #[error("host trust-store key ID is invalid: {key_id:?}")]
    InvalidTrustedKeyIdentifier { key_id: String },
    #[error("host trust-store publisher ID is invalid: {publisher_id:?}")]
    InvalidTrustedPublisherIdentifier { publisher_id: String },
    #[error("host trust-store Ed25519 public key is {actual} bytes, expected 32")]
    InvalidTrustedPublicKeyLength { actual: usize },
    #[error("host trust store contains duplicate key ID: {key_id:?}")]
    DuplicateTrustedKeyIdentifier { key_id: String },
    #[error("package signature is required by this source policy")]
    SignatureRequired,
    #[error("package signing key is not trusted: {key_id:?}")]
    UnknownSigningKey { key_id: String },
    #[error("package Ed25519 signature is not canonical base64: {key_id:?}")]
    InvalidSignatureEncoding { key_id: String },
    #[error("package Ed25519 signature verification failed: {key_id:?}")]
    InvalidSignature { key_id: String },
}

impl fmt::Debug for PackageValidationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Validation failures can retain source or sealed-store paths. Keep the
        // debug surface safe for telemetry and logs; callers that need a typed
        // diagnostic can still match the public enum variants.
        formatter.write_str("PackageValidationErrorV1(<redacted>)")
    }
}

fn validate_payload_inventory<'a>(
    manifest: &'a PackageManifestV1,
    budget: &PackageValidationBudgetV1,
) -> Result<BTreeMap<String, (&'a PayloadV1, String)>, PackageValidationErrorV1> {
    if manifest.payloads.len() > MAX_PAYLOAD_COUNT {
        return Err(PackageValidationErrorV1::PayloadCountExceeded {
            actual: manifest.payloads.len(),
            maximum: MAX_PAYLOAD_COUNT,
        });
    }
    let mut payloads = BTreeMap::new();
    let mut total_bytes = 0_u64;
    for payload in &manifest.payloads {
        budget.check()?;
        if payload.size > MAX_PAYLOAD_FILE_BYTES {
            return Err(PackageValidationErrorV1::PayloadFileTooLarge {
                path: payload.path.clone(),
                actual: payload.size,
                maximum: MAX_PAYLOAD_FILE_BYTES,
            });
        }
        total_bytes = total_bytes.checked_add(payload.size).ok_or(
            PackageValidationErrorV1::PayloadTotalBytesExceeded {
                actual: u64::MAX,
                maximum: MAX_PAYLOAD_TOTAL_BYTES,
            },
        )?;
        if total_bytes > MAX_PAYLOAD_TOTAL_BYTES {
            return Err(PackageValidationErrorV1::PayloadTotalBytesExceeded {
                actual: total_bytes,
                maximum: MAX_PAYLOAD_TOTAL_BYTES,
            });
        }
        let normalized = normalize_package_path("payloads[].path", &payload.path)?;
        let case_folded = normalized.to_ascii_lowercase();
        if payloads
            .insert(case_folded, (payload, normalized.clone()))
            .is_some()
        {
            return Err(PackageValidationErrorV1::DuplicateOrCaseCollidingPath {
                path: normalized,
            });
        }
    }
    Ok(payloads)
}

fn validate_tool_targets(manifest: &PackageManifestV1) -> Result<(), PackageValidationErrorV1> {
    for tool in &manifest.tools {
        if tool.target != host_tool_target_v1() {
            return Err(PackageValidationErrorV1::ToolTargetMismatch {
                tool_id: tool.id.clone(),
                manifest_target: tool.target.clone(),
                expected_target: host_tool_target_v1().to_owned(),
            });
        }
        let normalized = normalize_package_path("tools[].path", &tool.path)?;
        let required_prefix = format!("tools/{}/{}/", tool.target, tool.id);
        if !normalized.starts_with(&required_prefix) || normalized.len() == required_prefix.len() {
            return Err(PackageValidationErrorV1::ToolPathLayoutMismatch {
                tool_id: tool.id.clone(),
                path: normalized,
                required_prefix,
            });
        }
    }
    Ok(())
}

fn validate_manifest_references(
    manifest: &PackageManifestV1,
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
) -> Result<(), PackageValidationErrorV1> {
    for entry in &manifest.rust {
        require_payload(
            payloads,
            "rust[].entrypoint",
            &entry.entrypoint,
            PayloadKindV1::RustDll,
        )?;
    }
    for entry in &manifest.lua {
        require_payload(
            payloads,
            "lua[].entrypoint",
            &entry.entrypoint,
            PayloadKindV1::LuaScript,
        )?;
    }
    for entry in &manifest.skins {
        require_payload(
            payloads,
            "skins[].entrypoint",
            &entry.entrypoint,
            PayloadKindV1::SkinAsset,
        )?;
    }
    for locale in &manifest.locales {
        let payload = require_payload(
            payloads,
            "locales[].path",
            &locale.path,
            PayloadKindV1::Locale,
        )?;
        if locale.sha256 != payload.sha256 {
            return Err(PackageValidationErrorV1::LocalePayloadHashMismatch {
                locale: locale.locale.clone(),
            });
        }
    }
    for tool in &manifest.tools {
        let payload = require_payload(payloads, "tools[].path", &tool.path, PayloadKindV1::Tool)?;
        if tool.size != payload.size {
            return Err(PackageValidationErrorV1::ToolPayloadSizeMismatch {
                tool_id: tool.id.clone(),
            });
        }
        if tool.sha256 != payload.sha256 {
            return Err(PackageValidationErrorV1::ToolPayloadHashMismatch {
                tool_id: tool.id.clone(),
            });
        }
        for path in &tool.license_paths {
            require_payload(
                payloads,
                "tools[].license_paths",
                path,
                PayloadKindV1::License,
            )?;
        }
    }
    Ok(())
}

fn require_payload<'a>(
    payloads: &'a BTreeMap<String, (&'a PayloadV1, String)>,
    field: &'static str,
    path: &str,
    expected: PayloadKindV1,
) -> Result<&'a PayloadV1, PackageValidationErrorV1> {
    let normalized = normalize_package_path(field, path)?;
    let Some((payload, _)) = payloads.get(&normalized.to_ascii_lowercase()) else {
        return Err(PackageValidationErrorV1::MissingPayloadReference { path: normalized });
    };
    if payload.kind != expected {
        return Err(PackageValidationErrorV1::PayloadKindMismatch {
            path: normalized,
            actual: payload.kind,
            expected,
        });
    }
    Ok(payload)
}

fn verify_payload_files(
    package_root: &Path,
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
    budget: &PackageValidationBudgetV1,
) -> Result<(), PackageValidationErrorV1> {
    let root = verify_safe_root(package_root)?;
    for (payload, normalized) in payloads.values() {
        budget.check()?;
        let mut file = open_safe_payload_file(&root, normalized)?;
        let metadata = file
            .metadata()
            .map_err(|source| PackageValidationErrorV1::Io {
                path: root.join(normalized),
                source,
            })?;
        if !metadata.is_file() || metadata_is_reparse_point(&metadata) {
            return Err(PackageValidationErrorV1::NotRegularFile {
                path: root.join(normalized),
            });
        }
        if metadata.len() != payload.size {
            return Err(PackageValidationErrorV1::PayloadSizeMismatch {
                path: normalized.clone(),
                actual: metadata.len(),
                expected: payload.size,
            });
        }
        let digest = sha256_file(&mut file, &root.join(normalized), payload.size, budget)?;
        if hex_sha256(&digest) != payload.sha256 {
            return Err(PackageValidationErrorV1::PayloadHashMismatch {
                path: normalized.clone(),
            });
        }
        // Recheck the complete path after reading to detect a concurrent reparse-point swap.
        verify_existing_relative_path(&root, normalized, true)?;
    }
    Ok(())
}

fn verify_no_unlisted_content(
    package_root: &Path,
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
    budget: &PackageValidationBudgetV1,
) -> Result<(), PackageValidationErrorV1> {
    let root = verify_safe_root(package_root)?;
    scan_package_directory(&root, &declared_payload_paths(payloads), budget)
}

fn scan_package_directory(
    root: &Path,
    declared: &BTreeSet<String>,
    budget: &PackageValidationBudgetV1,
) -> Result<(), PackageValidationErrorV1> {
    let mut pending = vec![(root.to_path_buf(), String::new(), 0_usize)];
    let mut entry_count = 0_usize;
    while let Some((directory, relative_prefix, depth)) = pending.pop() {
        budget.check()?;
        let entries = fs::read_dir(&directory).map_err(|source| PackageValidationErrorV1::Io {
            path: directory.clone(),
            source,
        })?;
        for entry in entries {
            budget.check()?;
            entry_count = entry_count.checked_add(1).ok_or(
                PackageValidationErrorV1::PackageEntryCountExceeded {
                    maximum: MAX_PACKAGE_ENTRY_COUNT,
                },
            )?;
            if entry_count > MAX_PACKAGE_ENTRY_COUNT {
                return Err(PackageValidationErrorV1::PackageEntryCountExceeded {
                    maximum: MAX_PACKAGE_ENTRY_COUNT,
                });
            }
            let entry = entry.map_err(|source| PackageValidationErrorV1::Io {
                path: directory.clone(),
                source,
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(PackageValidationErrorV1::UnsafePackagePath {
                    field: "package content",
                    path: entry.path().display().to_string(),
                });
            };
            let relative = if relative_prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{relative_prefix}/{name}")
            };
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| PackageValidationErrorV1::Io {
                    path: path.clone(),
                    source,
                })?;
            if metadata_is_reparse_point(&metadata) {
                return Err(PackageValidationErrorV1::ReparsePointPath { path });
            }
            if metadata.is_dir() {
                let child_depth = depth + 1;
                if child_depth > MAX_PACKAGE_DEPTH {
                    return Err(PackageValidationErrorV1::PackageDepthExceeded {
                        path,
                        maximum: MAX_PACKAGE_DEPTH,
                    });
                }
                let prefix = format!("{}/", relative.to_ascii_lowercase());
                if !declared.iter().any(|payload| payload.starts_with(&prefix)) {
                    return Err(PackageValidationErrorV1::UndeclaredPackageDirectory { path });
                }
                pending.push((path, relative, child_depth));
            } else if metadata.is_file() {
                if relative != "manifest.json" && !declared.contains(&relative.to_ascii_lowercase())
                {
                    return Err(PackageValidationErrorV1::UnlistedPackageContent { path });
                }
            } else {
                return Err(PackageValidationErrorV1::NotRegularFile { path });
            }
        }
    }
    Ok(())
}

fn declared_payload_paths(payloads: &BTreeMap<String, (&PayloadV1, String)>) -> BTreeSet<String> {
    payloads.keys().cloned().collect()
}

fn verify_safe_root(package_root: &Path) -> Result<PathBuf, PackageValidationErrorV1> {
    let metadata =
        fs::symlink_metadata(package_root).map_err(|source| PackageValidationErrorV1::Io {
            path: package_root.to_path_buf(),
            source,
        })?;
    if !metadata.is_dir() || metadata_is_reparse_point(&metadata) {
        return Err(PackageValidationErrorV1::UnsafePackageRoot {
            path: package_root.to_path_buf(),
        });
    }
    fs::canonicalize(package_root).map_err(|source| PackageValidationErrorV1::Io {
        path: package_root.to_path_buf(),
        source,
    })
}

fn scavenge_staging_directories(root: &Path) -> Result<(), PackageValidationErrorV1> {
    let deadline = Instant::now() + STAGING_SCAVENGE_TIMEOUT;
    scavenge_staging_directories_with_limits(root, MAX_STAGING_ROOT_ENTRY_SCAN, deadline)
        .map(|_| ())
}

fn scavenge_staging_directories_with_limits(
    root: &Path,
    root_entry_limit: usize,
    deadline: Instant,
) -> Result<usize, PackageValidationErrorV1> {
    let mut inspected = 0_usize;
    for entry in fs::read_dir(root).map_err(|source| PackageValidationErrorV1::Io {
        path: root.to_path_buf(),
        source,
    })? {
        if inspected >= root_entry_limit || Instant::now() >= deadline {
            // Store startup must not become an attacker-controlled unbounded
            // root enumeration. Skipping excess candidates is safe because
            // staging trees are never loadable generations.
            return Ok(inspected);
        }
        inspected += 1;
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(owner_pid) = staging_owner_pid(name) else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            // A concurrent validator can finish or remove a staging tree while
            // startup scans. Staging is never loadable, so skip rather than
            // bricking the host-owned store on this best-effort cleanup path.
            continue;
        };
        if metadata_is_reparse_point(&metadata)
            || !metadata.is_dir()
            || !staging_is_old_enough(&metadata)
            || staging_owner_is_active(owner_pid)
        {
            continue;
        }
        // Do not traverse an active/fresh candidate. Only a stale, dead-owner
        // candidate is inspected, and any concurrent I/O change leaves it for
        // a future bounded scan instead of rejecting store construction.
        if !matches!(staging_tree_is_safe(&path, deadline), Ok(true)) {
            continue;
        }
        let _ = fs::remove_dir_all(&path);
    }
    Ok(inspected)
}

fn staging_owner_pid(name: &str) -> Option<u32> {
    let suffix = name.strip_prefix(".staging-")?;
    let (generation_and_pid, nonce) = suffix.rsplit_once('-')?;
    if nonce.is_empty() || !nonce.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (generation_id, pid) = generation_and_pid.rsplit_once('-')?;
    if generation_id.len() != 64
        || !generation_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return None;
    }
    pid.parse().ok()
}

fn staging_is_old_enough(metadata: &Metadata) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= MINIMUM_STAGING_AGE)
}

fn staging_owner_is_active(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(windows)]
    {
        process_is_running(pid)
    }
    #[cfg(not(windows))]
    {
        // On a non-Windows build this validator cannot create the namespace
        // leases required for activation, so conservative age protection is
        // the only applicable scavenger policy.
        let _ = pid;
        false
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn process_is_running(pid: u32) -> bool {
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 258;
    let handle = unsafe { open_process(SYNCHRONIZE, 0, pid) };
    if handle == 0 {
        // ERROR_INVALID_PARAMETER is Windows' documented result for a PID
        // that does not identify an existing process. Access denial and every
        // other query failure remain conservative: do not delete its staging.
        return io::Error::last_os_error().raw_os_error() != Some(87);
    }
    // SAFETY: `handle` was returned by OpenProcess and is closed exactly once.
    let running = unsafe { wait_for_single_object(handle, 0) == WAIT_TIMEOUT };
    unsafe {
        let _ = close_handle(handle);
    }
    running
}

#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "OpenProcess"]
    fn open_process(desired_access: u32, inherit_handle: i32, process_id: u32) -> isize;
    #[link_name = "WaitForSingleObject"]
    fn wait_for_single_object(handle: isize, milliseconds: u32) -> u32;
}

fn staging_tree_is_safe(root: &Path, deadline: Instant) -> Result<bool, PackageValidationErrorV1> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut entry_count = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        if Instant::now() >= deadline {
            return Ok(false);
        }
        for entry in fs::read_dir(&directory).map_err(|source| PackageValidationErrorV1::Io {
            path: directory.clone(),
            source,
        })? {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            entry_count = match entry_count.checked_add(1) {
                Some(count) if count <= MAX_PACKAGE_ENTRY_COUNT => count,
                _ => return Ok(false),
            };
            let entry = entry.map_err(|source| PackageValidationErrorV1::Io {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| PackageValidationErrorV1::Io {
                    path: path.clone(),
                    source,
                })?;
            if metadata_is_reparse_point(&metadata) {
                return Ok(false);
            }
            if metadata.is_dir() {
                let Some(child_depth) = depth.checked_add(1) else {
                    return Ok(false);
                };
                if child_depth > MAX_PACKAGE_DEPTH {
                    return Ok(false);
                }
                pending.push((path, child_depth));
            } else if !metadata.is_file() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn read_manifest_from_source(
    request: &PackageValidationRequestV1,
) -> Result<(PackageManifestV1, Vec<u8>), PackageValidationErrorV1> {
    let root = verify_safe_root(&request.source_package_root)?;
    let mut file = open_safe_payload_file(&root, "manifest.json")
        .map_err(|_| PackageValidationErrorV1::InvalidManifestFile)?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        request.budget.check()?;
        let read = file
            .read(&mut buffer)
            .map_err(|_| PackageValidationErrorV1::InvalidManifestFile)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_MANIFEST_FILE_BYTES {
            return Err(PackageValidationErrorV1::InvalidManifestFile);
        }
    }
    request.budget.check()?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| PackageValidationErrorV1::InvalidManifestFile)?;
    let manifest =
        PackageManifestV1::parse_json(text).map_err(PackageValidationErrorV1::Manifest)?;
    Ok((manifest, bytes))
}

fn seal_generation(
    sealed_store: &SealedPackageStoreV1,
    request: &PackageValidationRequestV1,
    manifest_bytes: &[u8],
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
    generation_id: &str,
) -> Result<PathBuf, PackageValidationErrorV1> {
    seal_generation_after_sealing(
        sealed_store,
        request,
        manifest_bytes,
        payloads,
        generation_id,
        || {},
    )
}

fn seal_generation_after_sealing(
    sealed_store: &SealedPackageStoreV1,
    request: &PackageValidationRequestV1,
    manifest_bytes: &[u8],
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
    generation_id: &str,
    after_sealing: impl FnOnce(),
) -> Result<PathBuf, PackageValidationErrorV1> {
    request.budget.check()?;
    let store = verify_safe_root(sealed_store.root.as_path())?;
    let final_path = store.join(generation_id);
    if final_path.exists() {
        return verify_existing_sealed_generation(
            &final_path,
            manifest_bytes,
            payloads,
            &request.budget,
        );
    }
    let mut staging = StagingDirectoryGuard::new(create_staging_directory(&store, generation_id)?);
    let sealing_result = seal_into_staging(request, manifest_bytes, payloads, staging.path());
    match sealing_result {
        Ok(()) => {
            after_sealing();
            // The budget must be checked at the commit boundary. A cancelled
            // validator may leave staging for a later safe scavenger, but it
            // must never publish a generation after cancellation/deadline.
            if let Err(error) = request.budget.check() {
                staging.cleanup()?;
                return Err(error);
            }
            match fs::rename(staging.path(), &final_path) {
                Ok(()) => {
                    staging.disarm();
                    request.budget.check()?;
                    Ok(final_path)
                }
                Err(_) if final_path.exists() => {
                    let reuse_result = verify_existing_sealed_generation(
                        &final_path,
                        manifest_bytes,
                        payloads,
                        &request.budget,
                    );
                    staging.cleanup()?;
                    reuse_result
                }
                Err(source) => {
                    staging.cleanup()?;
                    Err(PackageValidationErrorV1::Io {
                        path: final_path,
                        source,
                    })
                }
            }
        }
        Err(error) => {
            staging.cleanup()?;
            Err(error)
        }
    }
}

struct StagingDirectoryGuard {
    path: PathBuf,
    armed: bool,
}

impl StagingDirectoryGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn cleanup(&mut self) -> Result<(), PackageValidationErrorV1> {
        if !self.armed {
            return Ok(());
        }
        fs::remove_dir_all(&self.path).map_err(|source| {
            PackageValidationErrorV1::StagingCleanupFailed {
                path: self.path.clone(),
                source,
            }
        })?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for StagingDirectoryGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn create_staging_directory(
    store: &Path,
    generation_id: &str,
) -> Result<PathBuf, PackageValidationErrorV1> {
    for _ in 0..32 {
        let nonce = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging = store.join(format!(
            ".staging-{generation_id}-{}-{nonce}",
            std::process::id()
        ));
        match fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(PackageValidationErrorV1::Io {
                    path: staging,
                    source,
                });
            }
        }
    }
    Err(PackageValidationErrorV1::Io {
        path: store.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique sealed-package staging directory",
        ),
    })
}

fn seal_into_staging(
    request: &PackageValidationRequestV1,
    manifest_bytes: &[u8],
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
    staging: &Path,
) -> Result<(), PackageValidationErrorV1> {
    request.budget.check()?;
    fs::write(staging.join("manifest.json"), manifest_bytes).map_err(|source| {
        PackageValidationErrorV1::Io {
            path: staging.join("manifest.json"),
            source,
        }
    })?;
    let source_root = verify_safe_root(&request.source_package_root)?;
    for (payload, normalized) in payloads.values() {
        request.budget.check()?;
        let destination = staging.join(normalized.replace('/', "\\"));
        let parent = destination
            .parent()
            .ok_or_else(|| PackageValidationErrorV1::Io {
                path: destination.clone(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "payload path has no parent"),
            })?;
        fs::create_dir_all(parent).map_err(|source| PackageValidationErrorV1::Io {
            path: destination.clone(),
            source,
        })?;
        let mut input = open_safe_payload_file(&source_root, normalized)?;
        let mut output =
            File::create(&destination).map_err(|source| PackageValidationErrorV1::Io {
                path: destination.clone(),
                source,
            })?;
        let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
        let mut total = 0_u64;
        let mut hasher = Sha256::new();
        loop {
            request.budget.check()?;
            let read = input
                .read(&mut buffer)
                .map_err(|source| PackageValidationErrorV1::Io {
                    path: source_root.join(normalized),
                    source,
                })?;
            if read == 0 {
                break;
            }
            total = total.checked_add(read as u64).ok_or(
                PackageValidationErrorV1::PayloadSizeMismatch {
                    path: normalized.clone(),
                    actual: u64::MAX,
                    expected: payload.size,
                },
            )?;
            if total > payload.size {
                return Err(PackageValidationErrorV1::PayloadSizeMismatch {
                    path: normalized.clone(),
                    actual: total,
                    expected: payload.size,
                });
            }
            output
                .write_all(&buffer[..read])
                .map_err(|source| PackageValidationErrorV1::Io {
                    path: destination.clone(),
                    source,
                })?;
            hasher.update(&buffer[..read]);
        }
        if total != payload.size || hex_sha256(&hasher.finalize().into()) != payload.sha256 {
            return Err(PackageValidationErrorV1::PayloadHashMismatch {
                path: normalized.clone(),
            });
        }
    }
    Ok(())
}

fn verify_existing_sealed_generation(
    final_path: &Path,
    expected_manifest_bytes: &[u8],
    payloads: &BTreeMap<String, (&PayloadV1, String)>,
    budget: &PackageValidationBudgetV1,
) -> Result<PathBuf, PackageValidationErrorV1> {
    let root = verify_safe_root(final_path)?;
    let mut manifest = open_safe_payload_file(&root, "manifest.json")?;
    let mut manifest_bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        budget.check()?;
        let read = manifest
            .read(&mut buffer)
            .map_err(|source| PackageValidationErrorV1::Io {
                path: root.join("manifest.json"),
                source,
            })?;
        if read == 0 {
            break;
        }
        manifest_bytes.extend_from_slice(&buffer[..read]);
        if manifest_bytes.len() > MAX_MANIFEST_FILE_BYTES {
            return Err(PackageValidationErrorV1::SealedGenerationMismatch {
                path: final_path.to_path_buf(),
            });
        }
    }
    if manifest_bytes != expected_manifest_bytes {
        return Err(PackageValidationErrorV1::SealedGenerationMismatch {
            path: final_path.to_path_buf(),
        });
    }
    verify_payload_files(&root, payloads, budget)?;
    verify_no_unlisted_content(&root, payloads, budget)?;
    Ok(root)
}

fn host_target_v1() -> &'static str {
    "x86_64-pc-windows-msvc"
}
fn host_tool_target_v1() -> &'static str {
    "windows-x64"
}

fn open_safe_payload_file(root: &Path, normalized: &str) -> Result<File, PackageValidationErrorV1> {
    let path = verify_existing_relative_path(root, normalized, true)?;
    let file = File::open(&path).map_err(|source| PackageValidationErrorV1::Io {
        path: path.clone(),
        source,
    })?;
    let canonical = fs::canonicalize(&path).map_err(|source| PackageValidationErrorV1::Io {
        path: path.clone(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(PackageValidationErrorV1::PackagePathEscapesRoot { path: canonical });
    }
    Ok(file)
}

fn open_exclusive_safe_file(
    root: &Path,
    normalized: &str,
) -> Result<File, PackageValidationErrorV1> {
    let path = verify_existing_relative_path(root, normalized, true)?;
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt as _;

        // FILE_SHARE_READ allows compatible loader/read opens, while omitting
        // FILE_SHARE_WRITE and FILE_SHARE_DELETE prevents replacement bytes.
        OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001)
            .open(&path)
    };
    #[cfg(not(windows))]
    let file = File::open(&path);
    let file = file.map_err(|source| PackageValidationErrorV1::Io {
        path: path.clone(),
        source,
    })?;
    let canonical = fs::canonicalize(&path).map_err(|source| PackageValidationErrorV1::Io {
        path: path.clone(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(PackageValidationErrorV1::PackagePathEscapesRoot { path: canonical });
    }
    Ok(file)
}

fn read_limited_file(
    file: &File,
    path: &Path,
    maximum: usize,
    budget: &PackageValidationBudgetV1,
) -> Result<Vec<u8>, PackageValidationErrorV1> {
    let mut reader = file;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        budget.check()?;
        let read = reader
            .read(&mut buffer)
            .map_err(|source| PackageValidationErrorV1::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > maximum {
            return Err(PackageValidationErrorV1::SealedGenerationMismatch {
                path: path.to_path_buf(),
            });
        }
    }
}

fn verify_existing_relative_path(
    root: &Path,
    normalized: &str,
    expect_file: bool,
) -> Result<PathBuf, PackageValidationErrorV1> {
    let mut current = root.to_path_buf();
    let segments: Vec<_> = normalized.split('/').collect();
    for (index, segment) in segments.iter().enumerate() {
        current.push(segment);
        let metadata =
            fs::symlink_metadata(&current).map_err(|source| PackageValidationErrorV1::Io {
                path: current.clone(),
                source,
            })?;
        if metadata_is_reparse_point(&metadata) {
            return Err(PackageValidationErrorV1::ReparsePointPath {
                path: current.clone(),
            });
        }
        let final_component = index + 1 == segments.len();
        if final_component && expect_file && !metadata.is_file() {
            return Err(PackageValidationErrorV1::NotRegularFile {
                path: current.clone(),
            });
        }
        if !final_component && !metadata.is_dir() {
            return Err(PackageValidationErrorV1::NotRegularFile {
                path: current.clone(),
            });
        }
    }
    Ok(current)
}

fn sha256_file(
    file: &mut File,
    path: &Path,
    expected_size: u64,
    budget: &PackageValidationBudgetV1,
) -> Result<[u8; 32], PackageValidationErrorV1> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut total = 0_u64;
    loop {
        budget.check()?;
        let read = file
            .read(&mut buffer)
            .map_err(|source| PackageValidationErrorV1::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        total = total.checked_add(read as u64).ok_or(
            PackageValidationErrorV1::PayloadSizeMismatch {
                path: path.display().to_string(),
                actual: u64::MAX,
                expected: expected_size,
            },
        )?;
        if total > expected_size {
            return Err(PackageValidationErrorV1::PayloadSizeMismatch {
                path: path.display().to_string(),
                actual: total,
                expected: expected_size,
            });
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_size {
        return Err(PackageValidationErrorV1::PayloadSizeMismatch {
            path: path.display().to_string(),
            actual: total,
            expected: expected_size,
        });
    }
    Ok(hasher.finalize().into())
}

fn normalize_package_path(
    field: &'static str,
    path: &str,
) -> Result<String, PackageValidationErrorV1> {
    let bytes = path.as_bytes();
    let unsafe_path = path.is_empty()
        || !path.is_ascii()
        || path.starts_with('/')
        || path.starts_with('\\')
        || bytes.get(1) == Some(&b':')
        || path.contains('\\')
        || path.bytes().any(|byte| byte.is_ascii_control());
    if unsafe_path {
        return Err(PackageValidationErrorV1::UnsafePackagePath {
            field,
            path: path.to_owned(),
        });
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.iter().any(|part| {
        part.is_empty()
            || matches!(*part, "." | "..")
            || part.ends_with(['.', ' '])
            || part.contains(':')
    }) {
        return Err(PackageValidationErrorV1::UnsafePackagePath {
            field,
            path: path.to_owned(),
        });
    }
    Ok(parts.join("/"))
}

fn metadata_is_reparse_point(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

pub(crate) fn sealed_manifest_canonical_digest(
    manifest: &PackageManifestV1,
) -> Result<String, PackageManifestErrorV1> {
    let canonical = manifest.canonical_serialized_bytes()?;
    Ok(hex_sha256(&Sha256::digest(canonical).into()))
}

fn hex_sha256(digest: &[u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn is_normalized_id(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Read as _,
        path::PathBuf,
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::{
        LocalDeveloperAuthorizationV1, PackageValidationBudgetV1, PackageValidationCancellationV1,
        PackageValidationErrorV1, PackageValidationRequestV1, PackageValidatorV1,
        SealedPackageStoreV1, TrustedPublisherKeyStoreV1, TrustedPublisherKeyV1,
    };
    use crate::PackageManifestV1;

    struct TestPackage {
        _temp: TempDir,
        source: PathBuf,
        sealed_store: PathBuf,
    }

    impl TestPackage {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("test package root");
            let source = temp.path().join("source");
            let sealed_store = temp.path().join("sealed");
            fs::create_dir(&source).expect("source root");
            fs::create_dir(&sealed_store).expect("sealed store");
            Self {
                _temp: temp,
                source,
                sealed_store,
            }
        }

        fn write_file(&self, relative: &str, bytes: &[u8]) {
            let path = self.source.join(relative);
            fs::create_dir_all(path.parent().expect("test file parent"))
                .expect("test file directory");
            fs::write(path, bytes).expect("test file bytes");
        }

        fn write_manifest(&self, value: &Value) {
            fs::write(self.source.join("manifest.json"), value.to_string())
                .expect("manifest bytes");
        }

        fn request(&self) -> PackageValidationRequestV1 {
            PackageValidationRequestV1::new(self.source.clone())
        }
    }

    fn sha256(bytes: &[u8]) -> String {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        super::hex_sha256(&digest)
    }

    fn payload(path: &str, bytes: &[u8], kind: &str) -> Value {
        json!({ "path": path, "size": bytes.len(), "sha256": sha256(bytes), "kind": kind })
    }

    fn manifest_value(payloads: impl IntoIterator<Item = Value>) -> Value {
        let payloads: Vec<_> = payloads.into_iter().collect();
        json!({
            "manifest_version": 1,
            "package": { "id": "example.package", "version": "1.0.0" },
            "publisher": {
                "id": "example.publisher",
                "display_name": "Example Publisher",
                "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }]
            },
            "sdk": {
                "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc",
                "abi_schema": 1, "gpui": false, "ui_abi_fingerprint": null
            },
            "rust": [], "lua": [], "skins": [], "locales": [], "tools": [],
            "features": [], "dependencies": [], "payloads": payloads,
            "signature": { "kind": "ed25519", "key_id": "example.signing", "signature": "" },
            "data_version": 1
        })
    }

    fn signed_manifest(mut value: Value, key_pair: &Ed25519KeyPair) -> Value {
        let manifest =
            PackageManifestV1::parse_json(&value.to_string()).expect("unsigned manifest shape");
        let signature = STANDARD.encode(
            key_pair
                .sign(
                    &manifest
                        .canonical_ed25519_signing_bytes()
                        .expect("canonical signing bytes"),
                )
                .as_ref(),
        );
        *value
            .pointer_mut("/signature/signature")
            .expect("signature field") = json!(signature);
        value
    }

    fn key_pair() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).expect("fixed test Ed25519 seed")
    }

    fn validator(key_pair: &Ed25519KeyPair, package: &TestPackage) -> PackageValidatorV1 {
        let trusted_key = TrustedPublisherKeyV1::new(
            "example.signing".to_owned(),
            "example.publisher".to_owned(),
            key_pair.public_key().as_ref(),
        )
        .expect("trusted key");
        PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::new([trusted_key]).expect("trust store"),
            SealedPackageStoreV1::new(&package.sealed_store).expect("sealed store"),
        )
    }

    fn signed_data_package(
        package: &TestPackage,
        key_pair: &Ed25519KeyPair,
        bytes: &[u8],
    ) -> Value {
        package.write_file("data/payload.bin", bytes);
        let manifest = signed_manifest(
            manifest_value(vec![payload("data/payload.bin", bytes, "data")]),
            key_pair,
        );
        package.write_manifest(&manifest);
        manifest
    }

    #[test]
    fn validates_source_manifest_with_real_ed25519_and_seals_immutable_generation() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        let _manifest = signed_data_package(&package, &key_pair, b"verified payload");

        let first = validator(&key_pair, &package)
            .validate(&package.request())
            .expect("validated package");
        let second = validator(&key_pair, &package)
            .validate(&package.request())
            .expect("safe sealed reuse");
        assert_eq!(first.manifest_digest, second.manifest_digest);
        assert_eq!(
            first.verified_publisher_id.as_deref(),
            Some("example.publisher")
        );
        package.write_file("data/payload.bin", b"source changed after sealing");
        let guard = first.activation_guard().expect("activation guard");
        let mut sealed_payload = guard
            .payload_file("data/payload.bin")
            .expect("guarded payload")
            .try_clone()
            .expect("payload handle clone");
        let mut sealed_bytes = Vec::new();
        sealed_payload
            .read_to_end(&mut sealed_bytes)
            .expect("sealed payload bytes");
        assert_eq!(sealed_bytes, b"verified payload");
        assert!(matches!(
            validator(&key_pair, &package).validate(&package.request()),
            Err(PackageValidationErrorV1::PayloadSizeMismatch { .. }
                | PackageValidationErrorV1::PayloadHashMismatch { .. })
        ));
    }

    #[test]
    fn rejects_absent_or_differently_bound_source_manifest_and_signature_tampering() {
        let absent = TestPackage::new();
        assert!(matches!(
            validator(&key_pair(), &absent).validate(&absent.request()),
            Err(PackageValidationErrorV1::InvalidManifestFile)
        ));

        let package = TestPackage::new();
        let key_pair = key_pair();
        let mut manifest = signed_data_package(&package, &key_pair, b"payload");
        *manifest.pointer_mut("/data_version").expect("data version") = json!(2);
        package.write_manifest(&manifest);
        assert!(matches!(
            validator(&key_pair, &package).validate(&package.request()),
            Err(PackageValidationErrorV1::InvalidSignature { .. })
        ));
    }

    #[test]
    fn unsigned_packages_require_opaque_local_developer_provenance() {
        let package = TestPackage::new();
        package.write_file("data/payload.bin", b"unsigned payload");
        let mut manifest = manifest_value(vec![payload(
            "data/payload.bin",
            b"unsigned payload",
            "data",
        )]);
        *manifest.pointer_mut("/signature").expect("signature") = json!({ "kind": "unsigned" });
        package.write_manifest(&manifest);

        let validator = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::default(),
            SealedPackageStoreV1::new(&package.sealed_store).expect("sealed store"),
        );
        assert!(matches!(
            validator.validate(&package.request()),
            Err(PackageValidationErrorV1::SignatureRequired)
        ));
        let request = package
            .request()
            .with_local_developer_authorization(LocalDeveloperAuthorizationV1::issue());
        assert_eq!(
            validator
                .validate(&request)
                .expect("host-authorized local package")
                .verified_publisher_id,
            None
        );
    }

    #[test]
    fn rejects_target_and_tool_target_mismatches() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        let mut target = signed_data_package(&package, &key_pair, b"payload");
        *target.pointer_mut("/sdk/target").expect("sdk target") = json!("aarch64-pc-windows-msvc");
        package.write_manifest(&signed_manifest(target, &key_pair));
        assert!(matches!(
            validator(&key_pair, &package).validate(&package.request()),
            Err(PackageValidationErrorV1::TargetMismatch { .. })
        ));

        let tool_package = TestPackage::new();
        let tool_bytes = b"tool";
        tool_package.write_file("tools/windows-x64/example/tool.exe", tool_bytes);
        let mut tool = manifest_value(vec![payload(
            "tools/windows-x64/example/tool.exe",
            tool_bytes,
            "tool",
        )]);
        *tool.pointer_mut("/tools").expect("tools") = json!([{
            "id": "example", "target": "linux-x64", "path": "tools/windows-x64/example/tool.exe",
            "version": "1.0.0", "size": tool_bytes.len(), "sha256": sha256(tool_bytes),
            "output_protocol": "json", "source": "https://example.invalid/tool", "license_paths": []
        }]);
        tool_package.write_manifest(&signed_manifest(tool, &key_pair));
        assert!(matches!(
            validator(&key_pair, &tool_package).validate(&tool_package.request()),
            Err(PackageValidationErrorV1::ToolTargetMismatch { .. })
        ));

        let layout_package = TestPackage::new();
        layout_package.write_file("tools/windows-x64/other/tool.exe", tool_bytes);
        let mut layout = manifest_value(vec![payload(
            "tools/windows-x64/other/tool.exe",
            tool_bytes,
            "tool",
        )]);
        *layout.pointer_mut("/tools").expect("tools") = json!([{
            "id": "example", "target": "windows-x64", "path": "tools/windows-x64/other/tool.exe",
            "version": "1.0.0", "size": tool_bytes.len(), "sha256": sha256(tool_bytes),
            "output_protocol": "json", "source": "https://example.invalid/tool", "license_paths": []
        }]);
        layout_package.write_manifest(&signed_manifest(layout, &key_pair));
        assert!(matches!(
            validator(&key_pair, &layout_package).validate(&layout_package.request()),
            Err(PackageValidationErrorV1::ToolPathLayoutMismatch { .. })
        ));
    }

    #[test]
    fn enforces_payload_count_per_file_and_total_bounds() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        let many: Vec<Value> = (0..129)
            .map(|index| payload(&format!("data/{index}.bin"), b"x", "data"))
            .collect();
        package.write_manifest(&manifest_value(many));
        assert!(matches!(
            validator(&key_pair, &package).validate(&package.request()),
            Err(PackageValidationErrorV1::Manifest(
                crate::PackageManifestErrorV1::CollectionTooLong {
                    field: "payloads",
                    ..
                }
            ))
        ));

        let large = TestPackage::new();
        let mut too_large = payload("data/large.bin", b"x", "data");
        *too_large.pointer_mut("/size").expect("size") = json!(super::MAX_PAYLOAD_FILE_BYTES + 1);
        large.write_manifest(&signed_manifest(manifest_value(vec![too_large]), &key_pair));
        assert!(matches!(
            validator(&key_pair, &large).validate(&large.request()),
            Err(PackageValidationErrorV1::PayloadFileTooLarge { .. })
        ));

        let total = TestPackage::new();
        let total_payloads: Vec<Value> = (0..5)
            .map(|index| {
                let mut item = payload(&format!("data/total-{index}.bin"), b"x", "data");
                *item.pointer_mut("/size").expect("payload size") =
                    json!(super::MAX_PAYLOAD_FILE_BYTES);
                item
            })
            .collect();
        total.write_manifest(&signed_manifest(manifest_value(total_payloads), &key_pair));
        assert!(matches!(
            validator(&key_pair, &total).validate(&total.request()),
            Err(PackageValidationErrorV1::PayloadTotalBytesExceeded { .. })
        ));
    }

    #[test]
    fn rejects_unsafe_case_colliding_reparse_and_unlisted_content() {
        let key_pair = key_pair();
        for path in [
            "/absolute.bin",
            "C:/drive.bin",
            "../escape.bin",
            "data/../escape.bin",
            "data\\separator.bin",
        ] {
            let package = TestPackage::new();
            package.write_manifest(&signed_manifest(
                manifest_value(vec![payload(path, b"x", "data")]),
                &key_pair,
            ));
            assert!(matches!(
                validator(&key_pair, &package).validate(&package.request()),
                Err(PackageValidationErrorV1::UnsafePackagePath { .. })
            ));
        }

        let collision = TestPackage::new();
        collision.write_manifest(&signed_manifest(
            manifest_value(vec![
                payload("Data/Payload.bin", b"x", "data"),
                payload("data/payload.BIN", b"x", "data"),
            ]),
            &key_pair,
        ));
        assert!(matches!(
            validator(&key_pair, &collision).validate(&collision.request()),
            Err(PackageValidationErrorV1::DuplicateOrCaseCollidingPath { .. })
        ));

        let unlisted = TestPackage::new();
        signed_data_package(&unlisted, &key_pair, b"payload");
        unlisted.write_file("unlisted.txt", b"not declared");
        assert!(matches!(
            validator(&key_pair, &unlisted).validate(&unlisted.request()),
            Err(PackageValidationErrorV1::UnlistedPackageContent { .. })
        ));

        let reparse = TestPackage::new();
        let outside = reparse.source.parent().expect("test root").join("outside");
        fs::create_dir(&outside).expect("outside directory");
        fs::write(outside.join("payload.bin"), b"payload").expect("outside payload");
        let reparse_directory = reparse.source.join("data");
        let junction_status = Command::new("cmd")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&reparse_directory)
            .arg(&outside)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("create test junction");
        assert!(junction_status.success(), "test junction must be created");
        reparse.write_manifest(&signed_manifest(
            manifest_value(vec![payload("data/payload.bin", b"payload", "data")]),
            &key_pair,
        ));
        assert!(matches!(
            validator(&key_pair, &reparse).validate(&reparse.request()),
            Err(PackageValidationErrorV1::ReparsePointPath { .. })
        ));
    }

    #[test]
    fn iterative_walker_rejects_undeclared_directories_and_excessive_depth() {
        let key_pair = key_pair();
        let extra_directory = TestPackage::new();
        signed_data_package(&extra_directory, &key_pair, b"payload");
        fs::create_dir(extra_directory.source.join("not-declared")).expect("extra directory");
        assert!(matches!(
            validator(&key_pair, &extra_directory).validate(&extra_directory.request()),
            Err(PackageValidationErrorV1::UndeclaredPackageDirectory { .. })
        ));

        let deep = TestPackage::new();
        let nested = (0..=super::MAX_PACKAGE_DEPTH)
            .map(|index| format!("d{index}"))
            .collect::<Vec<_>>()
            .join("/");
        let path = format!("{nested}/payload.bin");
        deep.write_file(&path, b"payload");
        deep.write_manifest(&signed_manifest(
            manifest_value(vec![payload(&path, b"payload", "data")]),
            &key_pair,
        ));
        assert!(matches!(
            validator(&key_pair, &deep).validate(&deep.request()),
            Err(PackageValidationErrorV1::PackageDepthExceeded { .. })
        ));
    }

    #[test]
    fn cancellation_and_deadline_are_checked_during_traversal_hashing_and_sealing() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        let manifest = signed_data_package(&package, &key_pair, b"payload");
        let parsed = PackageManifestV1::parse_json(&manifest.to_string()).expect("manifest");
        let cancellation = PackageValidationCancellationV1::new();
        cancellation.cancel();
        let cancelled = PackageValidationBudgetV1::default().with_cancellation(cancellation);
        let declared = std::collections::BTreeSet::from(["data/payload.bin".to_owned()]);
        assert!(matches!(
            super::scan_package_directory(&package.source, &declared, &cancelled),
            Err(PackageValidationErrorV1::Cancelled)
        ));

        let mut file =
            fs::File::open(package.source.join("data/payload.bin")).expect("payload file");
        assert!(matches!(
            super::sha256_file(
                &mut file,
                &package.source.join("data/payload.bin"),
                7,
                &cancelled
            ),
            Err(PackageValidationErrorV1::Cancelled)
        ));
        let active = PackageValidationBudgetV1::default();
        let payloads = super::validate_payload_inventory(&parsed, &active).expect("inventory");
        let staging = package.sealed_store.join("cancelled-staging");
        fs::create_dir(&staging).expect("staging directory");
        assert!(matches!(
            super::seal_into_staging(
                &package.request().with_budget(cancelled.clone()),
                manifest.to_string().as_bytes(),
                &payloads,
                &staging,
            ),
            Err(PackageValidationErrorV1::Cancelled)
        ));

        let expired = PackageValidationBudgetV1::with_deadline(
            Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("past deadline"),
        );
        assert!(matches!(
            validator(&key_pair, &package).validate(&package.request().with_budget(expired)),
            Err(PackageValidationErrorV1::DeadlineExceeded)
        ));
    }

    #[test]
    fn activation_guard_rejects_prior_mutation_and_prevents_replacement_while_held() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        signed_data_package(&package, &key_pair, b"payload");
        let result = validator(&key_pair, &package)
            .validate(&package.request())
            .expect("sealed package");
        let sealed_payload_path = result.sealed_generation.root.join("data/payload.bin");
        fs::write(&sealed_payload_path, b"altered").expect("alter before activation");
        assert!(matches!(
            result.activation_guard(),
            Err(PackageValidationErrorV1::PayloadSizeMismatch { .. }
                | PackageValidationErrorV1::PayloadHashMismatch { .. })
        ));

        fs::write(&sealed_payload_path, b"payload").expect("restore sealed payload");
        let guard = result.activation_guard().expect("activation guard");
        assert!(guard.payload_file("data/payload.bin").is_some());
        assert!(
            fs::File::open(&sealed_payload_path).is_ok(),
            "shared read remains available"
        );
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .open(&sealed_payload_path)
                .is_err()
        );
        assert!(
            fs::rename(
                &sealed_payload_path,
                sealed_payload_path.with_extension("replaced")
            )
            .is_err()
        );
    }

    #[cfg(windows)]
    #[test]
    #[allow(unsafe_code)]
    fn activation_guard_blocks_late_dll_injection_during_a_real_dll_load() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        let system_dll = PathBuf::from(
            std::env::var_os("SystemRoot").expect("Windows SystemRoot is available for DLL test"),
        )
        .join("System32")
        .join("version.dll");
        let dll_bytes = fs::read(&system_dll).expect("read known Windows DLL");
        package.write_file("plugins/validated.dll", &dll_bytes);
        package.write_manifest(&signed_manifest(
            manifest_value(vec![payload("plugins/validated.dll", &dll_bytes, "data")]),
            &key_pair,
        ));

        let result = validator(&key_pair, &package)
            .validate(&package.request())
            .expect("seal known DLL");
        let guard = result.activation_guard().expect("guard known DLL");
        let loaded_dll = guard.package_root().join("plugins/validated.dll");
        // SAFETY: this test loads a known Windows system DLL copied byte-for-byte
        // into the sealed generation and immediately releases the module handle.
        let module = unsafe { load_library_wide(&loaded_dll) };
        assert_ne!(module, 0, "a guarded loader read must remain possible");
        // SAFETY: `module` is non-null and was returned by LoadLibraryW above.
        unsafe {
            assert_ne!(free_library(module), 0, "release test DLL module");
        }

        assert!(
            fs::write(
                guard.package_root().join("plugins/late-injection.dll"),
                b"must not be introduced after validation",
            )
            .is_err(),
            "directory lease must reject DLL injection while activation is live"
        );
    }

    #[cfg(windows)]
    #[test]
    fn activation_guard_rehardens_a_recreated_generation_before_concurrent_injection() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        signed_data_package(&package, &key_pair, b"payload");
        let result = validator(&key_pair, &package)
            .validate(&package.request())
            .expect("sealed package");
        let first_guard = result.activation_guard().expect("first activation guard");
        let generation_root = first_guard.package_root().to_path_buf();
        let manifest_bytes =
            fs::read(generation_root.join("manifest.json")).expect("original canonical manifest");
        let payload_bytes =
            fs::read(generation_root.join("data/payload.bin")).expect("original sealed payload");
        drop(first_guard);

        // Recreate the exact same path with matching declared bytes. This
        // simulates a cache janitor/delete-recreate race after a prior guard.
        fs::remove_dir_all(&generation_root).expect("remove prior generation");
        fs::create_dir(&generation_root).expect("recreate generation root");
        fs::create_dir(generation_root.join("data")).expect("recreate payload directory");
        fs::write(generation_root.join("manifest.json"), manifest_bytes)
            .expect("restore canonical manifest");
        fs::write(generation_root.join("data/payload.bin"), payload_bytes)
            .expect("restore sealed payload");

        let guard = result
            .activation_guard()
            .expect("reharden recreated generation");
        let injection_path = guard.package_root().join("data/late-injection.dll");
        let injection = thread::spawn(move || fs::write(injection_path, b"late injection"));
        assert!(
            injection.join().expect("injection thread").is_err(),
            "a recreated same-path generation must receive a fresh namespace hardening"
        );
    }

    #[test]
    fn sealing_checks_the_budget_immediately_before_rename_commit() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        let manifest_value = signed_data_package(&package, &key_pair, b"payload");
        let manifest =
            PackageManifestV1::parse_json(&manifest_value.to_string()).expect("parse manifest");
        let payloads =
            super::validate_payload_inventory(&manifest, &PackageValidationBudgetV1::default())
                .expect("payload inventory");
        let canonical_manifest_bytes = manifest
            .canonical_serialized_bytes()
            .expect("canonical manifest bytes");
        let generation_id = super::hex_sha256(&Sha256::digest(&canonical_manifest_bytes).into());
        let cancellation = PackageValidationCancellationV1::new();
        let request = package.request().with_budget(
            PackageValidationBudgetV1::default().with_cancellation(cancellation.clone()),
        );
        let sealed_store = SealedPackageStoreV1::new(&package.sealed_store).expect("sealed store");

        assert!(matches!(
            super::seal_generation_after_sealing(
                &sealed_store,
                &request,
                &canonical_manifest_bytes,
                &payloads,
                &generation_id,
                || cancellation.cancel(),
            ),
            Err(PackageValidationErrorV1::Cancelled)
        ));
        assert!(
            !package.sealed_store.join(&generation_id).exists(),
            "a cancellation at the commit boundary must not publish a generation"
        );
        assert!(
            fs::read_dir(&package.sealed_store)
                .expect("sealed store entries")
                .all(|entry| !entry
                    .expect("store entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".staging-")),
            "a cancelled commit must clean its staging directory"
        );
    }

    #[test]
    fn staging_scavenger_preserves_active_owner_and_bounds_unsafe_walks() {
        let package = TestPackage::new();
        let active_name = format!(".staging-{}-{}-0", "a".repeat(64), std::process::id());
        let active_staging = package.sealed_store.join(active_name);
        fs::create_dir(&active_staging).expect("active staging directory");
        super::scavenge_staging_directories(&package.sealed_store).expect("safe scavenging");
        assert!(
            active_staging.exists(),
            "a sibling validator's current-process staging tree must not be removed"
        );

        let oversized = package.sealed_store.join("oversized-staging-tree");
        fs::create_dir(&oversized).expect("oversized staging directory");
        for index in 0..=super::MAX_PACKAGE_ENTRY_COUNT {
            fs::write(oversized.join(format!("{index}.tmp")), b"x").expect("staging entry");
        }
        assert!(
            !super::staging_tree_is_safe(&oversized, Instant::now() + Duration::from_secs(1))
                .expect("bounded staging inspection"),
            "scavenging must not traverse or delete an unbounded staging tree"
        );
        assert!(
            !super::staging_tree_is_safe(&oversized, Instant::now())
                .expect("expired staging inspection"),
            "staging traversal must stop safely once its deadline elapses"
        );
    }

    #[cfg(windows)]
    #[test]
    fn store_scavenger_skips_active_mutating_and_oversized_staging_candidates() {
        let package = TestPackage::new();
        let active_name = format!(".staging-{}-{}-1", "b".repeat(64), std::process::id());
        let active_staging = package.sealed_store.join(active_name);
        fs::create_dir(&active_staging).expect("active staging directory");
        let writing = Arc::new(AtomicBool::new(true));
        let writer_flag = Arc::clone(&writing);
        let writer_path = active_staging.join("writer.tmp");
        fs::write(&writer_path, b"initial active staging content")
            .expect("non-empty active staging");
        let writer = thread::spawn(move || {
            let mut counter = 0_u64;
            while writer_flag.load(Ordering::Acquire) {
                let _ = fs::write(&writer_path, counter.to_le_bytes());
                counter = counter.wrapping_add(1);
            }
        });

        let oversized_name = format!(".staging-{}-0-2", "c".repeat(64));
        let oversized_staging = package.sealed_store.join(oversized_name);
        fs::create_dir(&oversized_staging).expect("oversized staging directory");
        for index in 0..=super::MAX_PACKAGE_ENTRY_COUNT {
            fs::write(oversized_staging.join(format!("{index}.tmp")), b"x")
                .expect("oversized staging entry");
        }
        mark_staging_directory_old(&oversized_staging);

        let stale_empty_name = format!(".staging-{}-0-3", "d".repeat(64));
        let stale_empty_staging = package.sealed_store.join(stale_empty_name);
        fs::create_dir(&stale_empty_staging).expect("stale empty staging directory");
        mark_staging_directory_old(&stale_empty_staging);

        SealedPackageStoreV1::new(&package.sealed_store)
            .expect("active or oversized staging must not brick store startup");
        writing.store(false, Ordering::Release);
        writer.join().expect("active staging writer");
        assert!(
            active_staging.exists(),
            "active staging must be left untouched"
        );
        assert!(
            oversized_staging.exists(),
            "oversized stale staging must be skipped after bounded inspection"
        );
        assert!(
            !stale_empty_staging.exists(),
            "a stale, dead-owner empty staging candidate must be scavenged"
        );
    }

    #[test]
    fn staging_root_scan_respects_candidate_and_time_bounds() {
        let package = TestPackage::new();
        for index in 0..4 {
            fs::create_dir(package.sealed_store.join(format!("candidate-{index}")))
                .expect("root candidate");
        }
        let inspected = super::scavenge_staging_directories_with_limits(
            &package.sealed_store,
            2,
            Instant::now() + Duration::from_secs(1),
        )
        .expect("bounded root scan");
        assert_eq!(inspected, 2, "root scan must stop at its candidate cap");
        let inspected = super::scavenge_staging_directories_with_limits(
            &package.sealed_store,
            4,
            Instant::now(),
        )
        .expect("expired root scan");
        assert_eq!(
            inspected, 0,
            "expired scavenger budget must not inspect entries"
        );
    }

    #[test]
    fn debug_output_redacts_sealed_store_and_generation_paths() {
        let package = TestPackage::new();
        let key_pair = key_pair();
        signed_data_package(&package, &key_pair, b"payload");
        let sealed_store = SealedPackageStoreV1::new(&package.sealed_store).expect("sealed store");
        let trusted_key = TrustedPublisherKeyV1::new(
            "example.signing".to_owned(),
            "example.publisher".to_owned(),
            key_pair.public_key().as_ref(),
        )
        .expect("trusted key");
        let result = PackageValidatorV1::new(
            TrustedPublisherKeyStoreV1::new([trusted_key]).expect("trust store"),
            sealed_store.clone(),
        )
        .validate(&package.request())
        .expect("validated package");
        let guard = result.activation_guard().expect("activation guard");
        for diagnostic in [
            format!("{sealed_store:?}"),
            format!("{result:?}"),
            format!("{guard:?}"),
            format!(
                "{:?}",
                PackageValidationErrorV1::Io {
                    path: package.sealed_store.clone(),
                    source: std::io::Error::other("test diagnostic"),
                }
            ),
        ] {
            assert!(!diagnostic.contains(&package.sealed_store.display().to_string()));
        }
    }

    #[cfg(windows)]
    #[allow(unsafe_code)]
    unsafe fn load_library_wide(path: &std::path::Path) -> isize {
        use std::{iter, os::windows::ffi::OsStrExt as _};

        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        // SAFETY: `wide_path` is NUL-terminated and valid for this FFI call.
        unsafe { load_library_w(wide_path.as_ptr()) }
    }

    #[cfg(windows)]
    fn mark_staging_directory_old(path: &std::path::Path) {
        use std::os::windows::fs::OpenOptionsExt as _;

        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        let directory = fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .expect("open staging directory for timestamp update");
        let old = std::time::SystemTime::now()
            .checked_sub(super::MINIMUM_STAGING_AGE + Duration::from_secs(1))
            .expect("old staging timestamp");
        directory
            .set_times(fs::FileTimes::new().set_modified(old))
            .expect("age staging directory");
    }

    #[cfg(windows)]
    #[allow(unsafe_code)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "LoadLibraryW"]
        fn load_library_w(file_name: *const u16) -> isize;
        #[link_name = "FreeLibrary"]
        fn free_library(module: isize) -> i32;
    }
}
