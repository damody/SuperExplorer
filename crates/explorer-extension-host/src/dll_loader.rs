//! Per-DLL, pre-callback loading for sealed Rust extension packages.
//!
//! This module deliberately does not own package startup, registrar dispatch,
//! draining, or unloading. `abi_stable` root values contain pointers into their
//! DLLs, so each mapped library is retained for the process lifetime before any
//! root reference can escape this module.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
    sync::{Mutex, OnceLock},
};

use abi_stable::{
    library::{AbiHeaderRef, LibraryError, ROOT_MODULE_LOADER_NAME_WITH_NUL, RootModule},
    std_types::ROption,
};
use explorer_extension_api::{
    AbiErrorCodeV1, AbiErrorV1, ExtensionRootModuleV1_Ref, PluginMetadataV1, RegistrationOutcomeV1,
    RegistrationStatusV1, SDK_MAJOR_VERSION_V1, UiAbiFingerprintV1, registrar_request_v1,
};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    ExtensionHost, HostRegistrationErrorV1, PackageManifestErrorV1, PackageManifestV1,
    PackageValidationErrorV1, ResolvedPackageV1, SealedPackageActivationGuardV1,
    package_validation::sealed_manifest_canonical_digest, plugin_call_guard::PluginCallGuardV1,
};

const MANIFEST_ABI_SCHEMA_V1: u32 = 1;
const HOST_UI_ABI_FINGERPRINT_ARTIFACT: &str = include_str!("../../../sdk/ui-abi-fingerprint.json");
static HOST_UI_ABI_FINGERPRINT: OnceLock<Result<HostUiAbiFingerprintV1, ()>> = OnceLock::new();
static RESIDENT_LOAD_STATE: OnceLock<Mutex<ResidentLoadStateV1>> = OnceLock::new();

/// Loads every declared Rust DLL for one resolved package without invoking a
/// plugin registrar.
///
/// The host UI fingerprint is build-time authority from the approved SDK
/// artifact, never caller-supplied package data. Only the private native
/// lifecycle consumes the returned roots; no raw registrar bypass is exposed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExtensionDllLoaderV1;

impl ExtensionDllLoaderV1 {
    /// Validates and maps all Rust roots declared by `resolved` atomically from
    /// the caller's perspective.
    ///
    /// Manifest compatibility is preflighted for the entire package before any
    /// DLL is mapped. If a later per-DLL layout or semantic check fails, no root
    /// is returned and no registrar callback has been invoked.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a manifest preflight, sealed activation,
    /// Windows loading, `abi_stable` layout, or root semantic failure.
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        clippy::unused_self,
        reason = "keeps the public loader surface ready for future host-owned configuration"
    )]
    pub(crate) fn load_package(
        &self,
        resolved: &ResolvedPackageV1<'_>,
    ) -> Result<LoadedPackageRootsV1, ExtensionDllLoadErrorV1> {
        let manifest = resolved.manifest();
        Self::preflight_manifest(manifest)?;
        let sealed_manifest_digest =
            sealed_manifest_canonical_digest(manifest).map_err(|source| {
                ExtensionDllLoadErrorV1::CanonicalManifestDigest {
                    package_id: manifest.package.id.clone(),
                    source,
                }
            })?;
        if let Some(loaded) = Self::resident_cached_load(&sealed_manifest_digest)? {
            return Ok(loaded);
        }
        if manifest.rust.is_empty() {
            return Ok(LoadedPackageRootsV1::bound_to(
                resolved,
                sealed_manifest_digest,
                Vec::new(),
            ));
        }

        let activation_guard = match resolved.validation_result().activation_guard() {
            Ok(guard) => guard,
            Err(source) => {
                if let Some(loaded) = Self::reject_after_guard_failure(&sealed_manifest_digest)? {
                    return Ok(loaded);
                }
                return Err(ExtensionDllLoadErrorV1::ActivationGuard(source));
            }
        };
        if let Some(loaded) = Self::begin_resident_load(&sealed_manifest_digest)? {
            return Ok(loaded);
        }
        // The guard holds sealed file and directory leases. Keep it process-resident
        // before mapping any DLL so a failed later map cannot reopen a mutation
        // window or close a mapped image's backing file.
        let activation_guard = Box::leak(Box::new(activation_guard));

        let result = Self::map_package_roots(manifest, activation_guard);
        match result {
            Ok(roots) => {
                let loaded =
                    LoadedPackageRootsV1::bound_to(resolved, sealed_manifest_digest.clone(), roots);
                Self::finish_resident_load(&sealed_manifest_digest, loaded.clone())?;
                Ok(loaded)
            }
            Err(error) => {
                Self::reject_resident_load(&sealed_manifest_digest)?;
                Err(error)
            }
        }
    }

    fn map_package_roots(
        manifest: &PackageManifestV1,
        activation_guard: &SealedPackageActivationGuardV1,
    ) -> Result<Vec<LoadedExtensionRootV1>, ExtensionDllLoadErrorV1> {
        let mut roots = Vec::with_capacity(manifest.rust.len());
        let mut entries = manifest.rust.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        for entry in entries {
            let payload = manifest
                .payloads
                .iter()
                .find(|payload| {
                    payload.path.eq_ignore_ascii_case(&entry.entrypoint)
                        && matches!(payload.kind, crate::PayloadKindV1::RustDll)
                })
                .ok_or_else(|| ExtensionDllLoadErrorV1::SealedPayloadUnavailable {
                    entrypoint_id: entry.id.clone(),
                    entrypoint: entry.entrypoint.clone(),
                })?;

            if activation_guard.payload_file(&payload.path).is_none() {
                return Err(ExtensionDllLoadErrorV1::SealedPayloadUnavailable {
                    entrypoint_id: entry.id.clone(),
                    entrypoint: payload.path.clone(),
                });
            }

            let root = load_root_from_sealed_path(
                &activation_guard.package_root().join(&payload.path),
                &entry.id,
            )?;
            let metadata = ExtensionHost::new().validate_root(root).map_err(|error| {
                ExtensionDllLoadErrorV1::RootValidation {
                    entrypoint_id: entry.id.clone(),
                    error,
                }
            })?;
            Self::validate_binary_ui_fingerprint(manifest, entry.id.as_str(), root)?;
            roots.push(LoadedExtensionRootV1 {
                entrypoint_id: entry.id.clone(),
                root_module: entry.root_module.clone(),
                entrypoint_path: payload.path.clone(),
                metadata,
                root,
            });
        }

        Ok(roots)
    }

    fn preflight_manifest(manifest: &PackageManifestV1) -> Result<(), ExtensionDllLoadErrorV1> {
        if manifest.sdk.abi_schema != MANIFEST_ABI_SCHEMA_V1 {
            return Err(ExtensionDllLoadErrorV1::ManifestAbiSchemaMismatch {
                package_id: manifest.package.id.clone(),
                actual: manifest.sdk.abi_schema,
                expected: MANIFEST_ABI_SCHEMA_V1,
            });
        }

        let mut entrypoint_paths = BTreeSet::new();
        let mut root_modules = BTreeSet::new();
        for entry in &manifest.rust {
            if entry.sdk_major != SDK_MAJOR_VERSION_V1 {
                return Err(ExtensionDllLoadErrorV1::EntrypointSdkMajorMismatch {
                    package_id: manifest.package.id.clone(),
                    entrypoint_id: entry.id.clone(),
                    actual: entry.sdk_major,
                    expected: SDK_MAJOR_VERSION_V1,
                });
            }
            if !entrypoint_paths.insert(entry.entrypoint.to_ascii_lowercase()) {
                return Err(ExtensionDllLoadErrorV1::DuplicateRustEntrypointPath {
                    package_id: manifest.package.id.clone(),
                    entrypoint: entry.entrypoint.clone(),
                });
            }
            if !root_modules.insert(entry.root_module.to_ascii_lowercase()) {
                return Err(ExtensionDllLoadErrorV1::DuplicateRustRootModule {
                    package_id: manifest.package.id.clone(),
                    root_module: entry.root_module.clone(),
                });
            }
            if !manifest.payloads.iter().any(|payload| {
                payload.path.eq_ignore_ascii_case(&entry.entrypoint)
                    && matches!(payload.kind, crate::PayloadKindV1::RustDll)
            }) {
                return Err(ExtensionDllLoadErrorV1::SealedPayloadUnavailable {
                    entrypoint_id: entry.id.clone(),
                    entrypoint: entry.entrypoint.clone(),
                });
            }
        }

        if manifest.sdk.gpui {
            let expected = host_ui_abi_fingerprint()?;
            let actual = manifest.sdk.ui_abi_fingerprint.as_deref().ok_or_else(|| {
                ExtensionDllLoadErrorV1::ManifestGpuiFingerprintMissing {
                    package_id: manifest.package.id.clone(),
                }
            })?;
            if actual != expected.fingerprint {
                return Err(ExtensionDllLoadErrorV1::GpuiFingerprintMismatch {
                    host_bundle_id: expected.bundle_id.clone(),
                    host_fingerprint: expected.fingerprint.clone(),
                    plugin_bundle_id: manifest.sdk.bundle_id.clone(),
                    plugin_fingerprint: actual.to_owned(),
                });
            }
        }

        Ok(())
    }

    fn validate_binary_ui_fingerprint(
        manifest: &PackageManifestV1,
        entrypoint_id: &str,
        root: ExtensionRootModuleV1_Ref,
    ) -> Result<(), ExtensionDllLoadErrorV1> {
        let binary_fingerprint = root.registrar().ui_abi_fingerprint_sha256();
        if !manifest.sdk.gpui {
            if matches!(binary_fingerprint, Some(ROption::RSome(_))) {
                return Err(ExtensionDllLoadErrorV1::UnexpectedBinaryUiFingerprint {
                    entrypoint_id: entrypoint_id.to_owned(),
                });
            }
            return Ok(());
        }

        let Some(ROption::RSome(binary_fingerprint)) = binary_fingerprint else {
            return Err(ExtensionDllLoadErrorV1::MissingBinaryUiFingerprint {
                entrypoint_id: entrypoint_id.to_owned(),
            });
        };
        let host = host_ui_abi_fingerprint()?;
        let manifest_fingerprint = manifest.sdk.ui_abi_fingerprint.as_deref().ok_or_else(|| {
            ExtensionDllLoadErrorV1::ManifestGpuiFingerprintMissing {
                package_id: manifest.package.id.clone(),
            }
        })?;
        let manifest_bytes = UiAbiFingerprintV1::from_lower_hex(manifest_fingerprint).ok_or(
            ExtensionDllLoadErrorV1::ManifestGpuiFingerprintMissing {
                package_id: manifest.package.id.clone(),
            },
        )?;
        if binary_fingerprint != manifest_bytes || binary_fingerprint.bytes() != host.bytes {
            return Err(ExtensionDllLoadErrorV1::BinaryUiFingerprintMismatch {
                entrypoint_id: entrypoint_id.to_owned(),
                host_bundle_id: host.bundle_id.clone(),
                host_fingerprint: host.fingerprint.clone(),
                plugin_bundle_id: manifest.sdk.bundle_id.clone(),
                manifest_fingerprint: manifest_fingerprint.to_owned(),
            });
        }
        Ok(())
    }

    fn resident_cached_load(
        digest: &str,
    ) -> Result<Option<LoadedPackageRootsV1>, ExtensionDllLoadErrorV1> {
        let state = resident_load_state()?;
        match state.entries.get(digest) {
            Some(ResidentLoadEntryV1::Loaded(loaded)) => Ok(Some(loaded.clone())),
            Some(ResidentLoadEntryV1::Attempting) => {
                Err(ExtensionDllLoadErrorV1::AlreadyAttempted {
                    sealed_manifest_digest: digest.to_owned(),
                })
            }
            Some(ResidentLoadEntryV1::Rejected) => {
                Err(ExtensionDllLoadErrorV1::PreviouslyRejected {
                    sealed_manifest_digest: digest.to_owned(),
                })
            }
            None => Ok(None),
        }
    }

    fn begin_resident_load(
        digest: &str,
    ) -> Result<Option<LoadedPackageRootsV1>, ExtensionDllLoadErrorV1> {
        let mut state = resident_load_state()?;
        match state.entries.get(digest) {
            Some(ResidentLoadEntryV1::Loaded(loaded)) => return Ok(Some(loaded.clone())),
            Some(ResidentLoadEntryV1::Attempting) => {
                return Err(ExtensionDllLoadErrorV1::AlreadyAttempted {
                    sealed_manifest_digest: digest.to_owned(),
                });
            }
            Some(ResidentLoadEntryV1::Rejected) => {
                return Err(ExtensionDllLoadErrorV1::PreviouslyRejected {
                    sealed_manifest_digest: digest.to_owned(),
                });
            }
            None => {}
        }
        state
            .entries
            .insert(digest.to_owned(), ResidentLoadEntryV1::Attempting);
        Ok(None)
    }

    fn reject_after_guard_failure(
        digest: &str,
    ) -> Result<Option<LoadedPackageRootsV1>, ExtensionDllLoadErrorV1> {
        let mut state = resident_load_state()?;
        match state.entries.get(digest) {
            Some(ResidentLoadEntryV1::Loaded(loaded)) => Ok(Some(loaded.clone())),
            Some(ResidentLoadEntryV1::Attempting) => {
                Err(ExtensionDllLoadErrorV1::AlreadyAttempted {
                    sealed_manifest_digest: digest.to_owned(),
                })
            }
            Some(ResidentLoadEntryV1::Rejected) => {
                Err(ExtensionDllLoadErrorV1::PreviouslyRejected {
                    sealed_manifest_digest: digest.to_owned(),
                })
            }
            None => {
                state
                    .entries
                    .insert(digest.to_owned(), ResidentLoadEntryV1::Rejected);
                Ok(None)
            }
        }
    }

    fn finish_resident_load(
        digest: &str,
        loaded: LoadedPackageRootsV1,
    ) -> Result<(), ExtensionDllLoadErrorV1> {
        let mut state = resident_load_state()?;
        state
            .entries
            .insert(digest.to_owned(), ResidentLoadEntryV1::Loaded(loaded));
        Ok(())
    }

    fn reject_resident_load(digest: &str) -> Result<(), ExtensionDllLoadErrorV1> {
        let mut state = resident_load_state()?;
        state
            .entries
            .insert(digest.to_owned(), ResidentLoadEntryV1::Rejected);
        Ok(())
    }
}

struct ResidentLoadStateV1 {
    entries: BTreeMap<String, ResidentLoadEntryV1>,
}

enum ResidentLoadEntryV1 {
    Attempting,
    Loaded(LoadedPackageRootsV1),
    Rejected,
}

fn resident_load_state()
-> Result<std::sync::MutexGuard<'static, ResidentLoadStateV1>, ExtensionDllLoadErrorV1> {
    RESIDENT_LOAD_STATE
        .get_or_init(|| {
            Mutex::new(ResidentLoadStateV1 {
                entries: BTreeMap::new(),
            })
        })
        .lock()
        .map_err(|_| ExtensionDllLoadErrorV1::ResidentStatePoisoned)
}

/// One ABI- and manifest-validated root. It exposes no registrar dispatch.
#[derive(Clone)]
pub(crate) struct LoadedExtensionRootV1 {
    entrypoint_id: String,
    root_module: String,
    entrypoint_path: String,
    metadata: PluginMetadataV1,
    #[allow(
        dead_code,
        reason = "task 3.4 owns registrar dispatch from this validated handle"
    )]
    root: ExtensionRootModuleV1_Ref,
}

impl fmt::Debug for LoadedExtensionRootV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedExtensionRootV1")
            .field("entrypoint_id", &self.entrypoint_id)
            .field("root_module", &self.root_module)
            .field("entrypoint_path", &self.entrypoint_path)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

#[allow(
    dead_code,
    reason = "task 3.5 consumes the remaining validated root metadata"
)]
impl LoadedExtensionRootV1 {
    /// Returns the manifest Rust entrypoint identifier bound to this root.
    #[must_use]
    pub(crate) fn entrypoint_id(&self) -> &str {
        &self.entrypoint_id
    }

    /// Returns the manifest root-module identifier bound to this DLL.
    #[must_use]
    pub(crate) fn root_module(&self) -> &str {
        &self.root_module
    }

    /// Returns the sealed manifest path bound to this DLL.
    #[must_use]
    pub(crate) fn entrypoint_path(&self) -> &str {
        &self.entrypoint_path
    }

    /// Returns the validated plugin and primary-interface identities.
    #[must_use]
    pub(crate) const fn metadata(&self) -> PluginMetadataV1 {
        self.metadata
    }
}

/// Invokes a root registrar only while the caller holds a live durable marker.
///
/// This is the sole non-test raw ABI dispatch seam. The raw root reference is
/// never exposed outside this module.
pub(crate) fn invoke_guarded_registrar(
    root: &LoadedExtensionRootV1,
    _guard: &PluginCallGuardV1,
) -> Result<RegistrationOutcomeV1, HostRegistrationErrorV1> {
    let registrar = root.root.registrar();
    let _optional_contract_query = registrar.describe_contract();
    registrar
        .register()
        .invoke(registrar_request_v1())
        .into_result()
        .map_err(|error| {
            if error.code == AbiErrorCodeV1::CALLBACK_PANICKED {
                HostRegistrationErrorV1::Panicked(error)
            } else {
                HostRegistrationErrorV1::Plugin(error)
            }
        })
        .and_then(|outcome| {
            let raw_status = outcome.status.into_raw();
            if raw_status == RegistrationStatusV1::ACCEPTED.into_raw() {
                Ok(outcome)
            } else {
                let code = if raw_status == RegistrationStatusV1::REJECTED.into_raw() {
                    AbiErrorCodeV1::REGISTRATION_OUTCOME_REJECTED
                } else if raw_status == 0 {
                    AbiErrorCodeV1::MALFORMED_REGISTRATION_OUTCOME
                } else {
                    AbiErrorCodeV1::UNKNOWN_REGISTRATION_OUTCOME
                };
                Err(HostRegistrationErrorV1::Plugin(AbiErrorV1::new(
                    code,
                    explorer_extension_api::ROOT_MODULE_CONTRACT_ID_V1,
                    raw_status,
                )))
            }
        })
}

/// All validated Rust roots for one package.
#[derive(Clone, Debug)]
pub(crate) struct LoadedPackageRootsV1 {
    package_id: String,
    #[allow(
        dead_code,
        reason = "lifecycle diagnostics retain the selected package version"
    )]
    package_version: String,
    sealed_manifest_digest: String,
    roots: Vec<LoadedExtensionRootV1>,
}

#[allow(
    dead_code,
    reason = "task 3.5 consumes remaining package-root metadata"
)]
impl LoadedPackageRootsV1 {
    fn bound_to(
        resolved: &ResolvedPackageV1<'_>,
        sealed_manifest_digest: String,
        roots: Vec<LoadedExtensionRootV1>,
    ) -> Self {
        Self {
            package_id: resolved.manifest().package.id.clone(),
            package_version: resolved.manifest().package.version.clone(),
            sealed_manifest_digest,
            roots,
        }
    }

    /// Returns the resolved package identifier bound to this set.
    #[must_use]
    pub(crate) fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the resolved package version bound to this set.
    #[must_use]
    pub(crate) fn package_version(&self) -> &str {
        &self.package_version
    }

    /// Returns the sealed source-manifest digest bound to this set.
    #[must_use]
    pub(crate) fn sealed_manifest_digest(&self) -> &str {
        &self.sealed_manifest_digest
    }
    /// Returns roots in canonical Rust entrypoint-ID order.
    #[must_use]
    pub(crate) fn roots(&self) -> &[LoadedExtensionRootV1] {
        &self.roots
    }
}

/// Typed pre-callback DLL loading failure.
#[derive(Debug, Error)]
#[allow(
    dead_code,
    reason = "platform-specific loader errors remain part of the private loader"
)]
pub(crate) enum ExtensionDllLoadErrorV1 {
    /// The process-global resident-load state was poisoned by a prior panic.
    #[error("resident extension DLL load state is poisoned")]
    ResidentStatePoisoned,
    /// The sealed package generation is currently loading or has already loaded.
    #[error("sealed package generation {sealed_manifest_digest} was already attempted")]
    AlreadyAttempted { sealed_manifest_digest: String },
    /// A prior guard or DLL mapping failure permanently rejected this generation.
    #[error("sealed package generation {sealed_manifest_digest} was previously rejected")]
    PreviouslyRejected { sealed_manifest_digest: String },
    /// The sealed package declares another manifest ABI revision.
    #[error("package {package_id:?} declares manifest ABI schema {actual}, expected {expected}")]
    ManifestAbiSchemaMismatch {
        package_id: String,
        actual: u32,
        expected: u32,
    },
    /// A Rust entrypoint targets a different SDK major.
    #[error(
        "package {package_id:?} Rust entrypoint {entrypoint_id:?} declares SDK major {actual}, expected {expected}"
    )]
    EntrypointSdkMajorMismatch {
        package_id: String,
        entrypoint_id: String,
        actual: u16,
        expected: u16,
    },
    /// More than one Rust entrypoint resolves to the same sealed DLL path.
    #[error("package {package_id:?} declares duplicate Rust DLL path {entrypoint:?}")]
    DuplicateRustEntrypointPath {
        package_id: String,
        entrypoint: String,
    },
    /// More than one Rust entrypoint declares the same root-module identifier.
    #[error("package {package_id:?} declares duplicate Rust root module {root_module:?}")]
    DuplicateRustRootModule {
        package_id: String,
        root_module: String,
    },
    /// A GPUI package omitted the fingerprint required by its own declaration.
    #[error("GPUI package {package_id:?} omitted sdk.ui_abi_fingerprint")]
    ManifestGpuiFingerprintMissing { package_id: String },
    /// A GPUI package's fingerprint is not exactly the host SDK fingerprint.
    #[error(
        "GPUI fingerprint mismatch: host bundle {host_bundle_id:?} fingerprint {host_fingerprint}, plugin bundle {plugin_bundle_id:?} fingerprint {plugin_fingerprint}"
    )]
    GpuiFingerprintMismatch {
        host_bundle_id: String,
        host_fingerprint: String,
        plugin_bundle_id: String,
        plugin_fingerprint: String,
    },
    /// A data-only package exposed a binary UI fingerprint.
    #[error("data-only root {entrypoint_id:?} unexpectedly reports a UI ABI fingerprint")]
    UnexpectedBinaryUiFingerprint { entrypoint_id: String },
    /// A GPUI package's root omitted the required binary UI fingerprint tail.
    #[error("GPUI root {entrypoint_id:?} omitted its binary UI ABI fingerprint")]
    MissingBinaryUiFingerprint { entrypoint_id: String },
    /// The GPUI DLL fingerprint disagreed with its sealed manifest or host SDK.
    #[error(
        "binary UI fingerprint mismatch for entrypoint {entrypoint_id:?}: host bundle {host_bundle_id:?} fingerprint {host_fingerprint}, plugin bundle {plugin_bundle_id:?} manifest fingerprint {manifest_fingerprint}"
    )]
    BinaryUiFingerprintMismatch {
        entrypoint_id: String,
        host_bundle_id: String,
        host_fingerprint: String,
        plugin_bundle_id: String,
        manifest_fingerprint: String,
    },
    /// The checked sealed activation did not expose the declared DLL payload.
    #[error(
        "sealed Rust DLL payload {entrypoint:?} for entrypoint {entrypoint_id:?} is unavailable"
    )]
    SealedPayloadUnavailable {
        entrypoint_id: String,
        entrypoint: String,
    },
    /// The sealed manifest could not be canonicalized for package binding.
    #[error("could not canonicalize sealed manifest for package {package_id:?}: {source}")]
    CanonicalManifestDigest {
        package_id: String,
        #[source]
        source: PackageManifestErrorV1,
    },
    /// The sealed generation could not be revalidated and leased for loading.
    #[error("could not open a sealed package activation guard: {0}")]
    ActivationGuard(#[source] PackageValidationErrorV1),
    /// The current host is not Windows, which is the only supported native-DLL host.
    #[error("native extension DLL loading is supported only on Windows")]
    UnsupportedPlatform,
    /// Windows rejected loading the sealed DLL with restricted dependency search.
    #[error("could not load Rust DLL for entrypoint {entrypoint_id:?}: {source}")]
    DynamicLibraryLoad {
        entrypoint_id: String,
        #[source]
        source: libloading::Error,
    },
    /// The DLL did not export a compatible `abi_stable` root header or layout.
    #[error("invalid abi_stable root for entrypoint {entrypoint_id:?}: {source}")]
    AbiStable {
        entrypoint_id: String,
        #[source]
        source: LibraryError,
    },
    /// Required root data was incompatible before any registrar callback.
    #[error("invalid root data for entrypoint {entrypoint_id:?}: {error:?}")]
    RootValidation {
        entrypoint_id: String,
        error: HostRegistrationErrorV1,
    },
    /// The build-time host fingerprint artifact is invalid.
    #[error("the build-time host UI ABI fingerprint artifact is invalid")]
    InvalidHostUiFingerprintArtifact,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UiAbiFingerprintArtifactV1 {
    bundle_id: String,
    fingerprint: String,
}

#[derive(Debug)]
struct HostUiAbiFingerprintV1 {
    bundle_id: String,
    fingerprint: String,
    bytes: [u8; 32],
}

fn host_ui_abi_fingerprint() -> Result<&'static HostUiAbiFingerprintV1, ExtensionDllLoadErrorV1> {
    match HOST_UI_ABI_FINGERPRINT.get_or_init(parse_host_ui_abi_fingerprint) {
        Ok(fingerprint) => Ok(fingerprint),
        Err(()) => Err(ExtensionDllLoadErrorV1::InvalidHostUiFingerprintArtifact),
    }
}

fn parse_host_ui_abi_fingerprint() -> Result<HostUiAbiFingerprintV1, ()> {
    let artifact: UiAbiFingerprintArtifactV1 =
        serde_json::from_str(HOST_UI_ABI_FINGERPRINT_ARTIFACT).map_err(|_| ())?;
    let bytes = UiAbiFingerprintV1::from_lower_hex(&artifact.fingerprint)
        .ok_or(())?
        .bytes();
    if artifact.bundle_id.is_empty() {
        return Err(());
    }
    Ok(HostUiAbiFingerprintV1 {
        bundle_id: artifact.bundle_id,
        fingerprint: artifact.fingerprint,
        bytes,
    })
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the sealed absolute DLL path and abi_stable exported header are validated before their references escape"
)]
fn load_root_from_sealed_path(
    path: &Path,
    entrypoint_id: &str,
) -> Result<ExtensionRootModuleV1_Ref, ExtensionDllLoadErrorV1> {
    use libloading::os::windows::{
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, Library,
    };

    debug_assert!(path.is_absolute(), "sealed activation roots are canonical");
    let library = unsafe {
        Library::load_with_flags(
            path,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map_err(|source| ExtensionDllLoadErrorV1::DynamicLibraryLoad {
        entrypoint_id: entrypoint_id.to_owned(),
        source,
    })?;

    // `AbiHeaderRef` and every root field can contain references into the DLL.
    // Keep the map process-resident even after a compatibility failure, matching
    // abi_stable's own no-unload safety rule and avoiding a DLL detach callback.
    let library: &'static Library = Box::leak(Box::new(library));
    let header = unsafe {
        *library
            .get::<AbiHeaderRef>(ROOT_MODULE_LOADER_NAME_WITH_NUL.as_bytes())
            .map_err(|source| ExtensionDllLoadErrorV1::DynamicLibraryLoad {
                entrypoint_id: entrypoint_id.to_owned(),
                source,
            })?
    }
    .upgrade()
    .map_err(|source| ExtensionDllLoadErrorV1::AbiStable {
        entrypoint_id: entrypoint_id.to_owned(),
        source,
    })?;
    header
        .ensure_layout::<ExtensionRootModuleV1_Ref>()
        .map_err(|source| ExtensionDllLoadErrorV1::AbiStable {
            entrypoint_id: entrypoint_id.to_owned(),
            source,
        })?;
    unsafe { header.init_root_module_with_unchecked_layout::<ExtensionRootModuleV1_Ref>() }
        .map_err(|source| ExtensionDllLoadErrorV1::AbiStable {
            entrypoint_id: entrypoint_id.to_owned(),
            source,
        })?
        .initialization()
        .map_err(|source| ExtensionDllLoadErrorV1::AbiStable {
            entrypoint_id: entrypoint_id.to_owned(),
            source,
        })
}

#[cfg(not(windows))]
fn load_root_from_sealed_path(
    _: &Path,
    _: &str,
) -> Result<ExtensionRootModuleV1_Ref, ExtensionDllLoadErrorV1> {
    Err(ExtensionDllLoadErrorV1::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Barrier,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    use abi_stable::{
        prefix_type::PrefixTypeTrait,
        std_types::{ROption, RResult},
    };
    use explorer_extension_api::{
        AbiErrorV1, ExtensionRegistrarV1, ExtensionRootModuleV1, RegistrarCallbackV1,
        RegistrarImplementationV1, RegistrarRequestV1, RegistrarResultV1, RegistrationOutcomeV1,
        StableIdV1, UiAbiFingerprintV1,
    };
    use serde_json::json;

    use super::*;

    const PLUGIN_ID: StableIdV1 = StableIdV1::new(crate::extension_id_namespace_v1(), 100);
    const INTERFACE_ID: StableIdV1 = StableIdV1::new(crate::extension_id_namespace_v1(), 101);
    fn host_fingerprint() -> &'static str {
        host_ui_abi_fingerprint()
            .expect("build-time artifact is valid")
            .fingerprint
            .as_str()
    }

    fn manifest(gpui: bool, fingerprint: Option<&str>, entry_sdk_major: u16) -> PackageManifestV1 {
        PackageManifestV1::parse_json(
            &json!({
                "manifest_version": 1,
                "package": { "id": "example.loader", "version": "1.0.0" },
                "publisher": { "id": "example.publisher", "display_name": "Example Publisher", "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }] },
                "sdk": { "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc", "abi_schema": 1, "gpui": gpui, "ui_abi_fingerprint": fingerprint },
                "rust": [{ "id": "native", "entrypoint": "native/plugin.dll", "root_module": "example.root", "sdk_major": entry_sdk_major }],
                "lua": [], "skins": [], "locales": [], "tools": [], "features": [], "dependencies": [],
                "payloads": [{ "path": "native/plugin.dll", "size": 1, "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "kind": "rust_dll" }],
                "signature": { "kind": "unsigned" }, "data_version": 1
            })
            .to_string(),
        )
        .expect("valid test manifest")
    }

    static CALLBACK_CALLED: AtomicBool = AtomicBool::new(false);

    struct MarksCallback;

    impl RegistrarImplementationV1 for MarksCallback {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            CALLBACK_CALLED.store(true, Ordering::SeqCst);
            RResult::ROk(RegistrationOutcomeV1::accepted(0))
        }
    }

    extern "C" fn describe_contract() -> StableIdV1 {
        explorer_extension_api::ROOT_MODULE_CONTRACT_ID_V1
    }

    fn root(
        abi_schema: explorer_extension_api::AbiSchemaIdV1,
        ui_abi_fingerprint_sha256: ROption<UiAbiFingerprintV1>,
    ) -> ExtensionRootModuleV1_Ref {
        ExtensionRootModuleV1 {
            abi_schema,
            root_contract_id: explorer_extension_api::ROOT_MODULE_CONTRACT_ID_V1,
            sdk_major: SDK_MAJOR_VERSION_V1,
            reserved: 0,
            metadata: PluginMetadataV1 {
                plugin_id: PLUGIN_ID,
                primary_interface_id: INTERFACE_ID,
            },
            registrar: ExtensionRegistrarV1 {
                register: RegistrarCallbackV1::new::<MarksCallback>(),
                describe_contract,
                ui_abi_fingerprint_sha256,
            }
            .leak_into_prefix(),
        }
        .leak_into_prefix()
    }

    #[test]
    fn gpui_fingerprint_mismatch_is_rejected_before_any_callback() {
        CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let error = ExtensionDllLoaderV1::preflight_manifest(&manifest(
            true,
            Some(&"0".repeat(64)),
            SDK_MAJOR_VERSION_V1,
        ))
        .expect_err("mismatched UI fingerprint must reject");
        assert!(matches!(
            error,
            ExtensionDllLoadErrorV1::GpuiFingerprintMismatch { .. }
        ));
        assert!(!CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn entrypoint_sdk_major_preflight_rejects_the_whole_package() {
        let error = ExtensionDllLoaderV1::preflight_manifest(&manifest(
            false,
            None,
            SDK_MAJOR_VERSION_V1 + 1,
        ))
        .expect_err("manifest SDK major mismatch must reject");
        assert!(matches!(
            error,
            ExtensionDllLoadErrorV1::EntrypointSdkMajorMismatch { .. }
        ));
    }

    #[test]
    fn root_schema_failure_never_invokes_the_registrar() {
        CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let result = ExtensionHost::new().validate_root(root(
            explorer_extension_api::AbiSchemaIdV1::new(0x5345, 2),
            ROption::RNone,
        ));
        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 { .. }))
        ));
        assert!(!CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn build_time_host_fingerprint_is_the_approved_artifact() {
        assert_eq!(
            host_ui_abi_fingerprint()
                .expect("artifact is valid")
                .fingerprint
                .len(),
            64
        );
        ExtensionDllLoaderV1::preflight_manifest(&manifest(
            true,
            Some(host_fingerprint()),
            SDK_MAJOR_VERSION_V1,
        ))
        .expect("exact fingerprint must pass preflight");
    }

    #[test]
    fn gpui_binary_tail_must_match_manifest_and_host_before_any_callback() {
        CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let gpui_manifest = manifest(true, Some(host_fingerprint()), SDK_MAJOR_VERSION_V1);

        let missing = ExtensionDllLoaderV1::validate_binary_ui_fingerprint(
            &gpui_manifest,
            "native",
            root(explorer_extension_api::ABI_SCHEMA_V1, ROption::RNone),
        );
        assert!(matches!(
            missing,
            Err(ExtensionDllLoadErrorV1::MissingBinaryUiFingerprint { .. })
        ));

        let mismatched = ExtensionDllLoaderV1::validate_binary_ui_fingerprint(
            &gpui_manifest,
            "native",
            root(
                explorer_extension_api::ABI_SCHEMA_V1,
                ROption::RSome(UiAbiFingerprintV1::new([0; 32])),
            ),
        );
        assert!(matches!(
            mismatched,
            Err(ExtensionDllLoadErrorV1::BinaryUiFingerprintMismatch { .. })
        ));

        let exact = ExtensionDllLoaderV1::validate_binary_ui_fingerprint(
            &gpui_manifest,
            "native",
            root(
                explorer_extension_api::ABI_SCHEMA_V1,
                ROption::RSome(UiAbiFingerprintV1::new(
                    UiAbiFingerprintV1::from_lower_hex(host_fingerprint())
                        .expect("test fingerprint")
                        .bytes(),
                )),
            ),
        );
        assert!(exact.is_ok());
        assert!(!CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn data_only_roots_reject_a_binary_ui_tail() {
        let error = ExtensionDllLoaderV1::validate_binary_ui_fingerprint(
            &manifest(false, None, SDK_MAJOR_VERSION_V1),
            "native",
            root(
                explorer_extension_api::ABI_SCHEMA_V1,
                ROption::RSome(UiAbiFingerprintV1::new([7; 32])),
            ),
        )
        .expect_err("data-only root must not report UI ABI bytes");
        assert!(matches!(
            error,
            ExtensionDllLoadErrorV1::UnexpectedBinaryUiFingerprint { .. }
        ));
    }

    #[test]
    fn preflight_rejects_case_insensitive_duplicate_dll_paths() {
        let mut duplicate = manifest(false, None, SDK_MAJOR_VERSION_V1);
        let mut second = duplicate.rust[0].clone();
        second.id = "native-second".to_owned();
        second.entrypoint = "NATIVE/PLUGIN.DLL".to_owned();
        duplicate.rust.push(second);

        let error = ExtensionDllLoaderV1::preflight_manifest(&duplicate)
            .expect_err("case-insensitive duplicate DLL path must reject");
        assert!(matches!(
            error,
            ExtensionDllLoadErrorV1::DuplicateRustEntrypointPath { .. }
        ));
    }

    #[test]
    fn resident_load_state_returns_cached_roots_and_preserves_rejections() {
        let loaded_digest = "loader-resident-cache-test";
        assert!(
            ExtensionDllLoaderV1::begin_resident_load(loaded_digest)
                .expect("fresh digest")
                .is_none()
        );
        let loaded = LoadedPackageRootsV1 {
            package_id: "example.loader".to_owned(),
            package_version: "1.0.0".to_owned(),
            sealed_manifest_digest: loaded_digest.to_owned(),
            roots: Vec::new(),
        };
        ExtensionDllLoaderV1::finish_resident_load(loaded_digest, loaded)
            .expect("cache loaded package");
        let cached = ExtensionDllLoaderV1::resident_cached_load(loaded_digest)
            .expect("cached lookup")
            .expect("loaded package cached");
        assert_eq!(cached.sealed_manifest_digest(), loaded_digest);

        let rejected_digest = "loader-resident-rejected-test";
        assert!(
            ExtensionDllLoaderV1::reject_after_guard_failure(rejected_digest)
                .expect("record rejected generation")
                .is_none()
        );
        assert!(matches!(
            ExtensionDllLoaderV1::resident_cached_load(rejected_digest),
            Err(ExtensionDllLoadErrorV1::PreviouslyRejected { .. })
        ));
    }

    #[test]
    fn concurrent_same_generation_has_one_owner_and_other_callers_fail_closed() {
        let digest = "loader-resident-concurrent-attempt-test";
        let start = Arc::new(Barrier::new(2));
        let first_start = Arc::clone(&start);
        let first = thread::spawn(move || {
            first_start.wait();
            ExtensionDllLoaderV1::begin_resident_load(digest)
        });
        let second_start = Arc::clone(&start);
        let second = thread::spawn(move || {
            second_start.wait();
            ExtensionDllLoaderV1::begin_resident_load(digest)
        });

        let first = first.join().expect("first loader thread joins");
        let second = second.join().expect("second loader thread joins");
        let results = [first, second];
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Ok(None)))
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(ExtensionDllLoadErrorV1::AlreadyAttempted { .. })
                ))
                .count(),
            1
        );
    }
}
