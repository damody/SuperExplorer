//! Host-side package discovery and entitlement seams.
//!
//! A package source identifies direct child directories that contain a regular
//! `manifest.json`. It deliberately does not parse manifests or validate package
//! payloads: [`PackageValidatorV1`] owns those security-sensitive checks. The
//! source only supplies the opaque validation provenance which distinguishes
//! built-in packages from explicitly local-developer packages.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::package_validation::LocalDeveloperAuthorizationV1;
use crate::{
    PackageValidationErrorV1, PackageValidationRequestV1, PackageValidationResultV1,
    PackageValidatorV1,
};

const MAX_DIRECT_CHILDREN_V1: usize = 1_024;

/// The host-controlled origin policy for an extension package.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PackageSourceKindV1 {
    /// Product-shipped packages that must carry a trusted publisher signature.
    BuiltIn,
    /// Developer-controlled packages that may be unsigned during local development.
    LocalDeveloper,
}

/// A host-side package discovery boundary.
///
/// Implementations must return direct-child package roots in deterministic order.
/// They must never make a package eligible for loading without handing it to
/// [`PackageValidatorV1`]. This trait is host-only and is intentionally not part
/// of the extension SDK ABI.
///
/// Local-developer unsigned provenance is intentionally unavailable to external
/// callers. It is issued only by a future host composition-root policy factory.
///
/// ```compile_fail
/// use explorer_extension_host::LocalDeveloperPackageSourceV1;
///
/// let _ = LocalDeveloperPackageSourceV1::new(std::path::PathBuf::new());
/// ```
pub trait PackageSourceV1: Send + Sync {
    /// Returns the provenance policy applied to packages from this source.
    fn kind(&self) -> PackageSourceKindV1;

    /// Discovers immediate package-directory candidates.
    ///
    /// # Errors
    ///
    /// Returns a typed source error when the source root or a candidate cannot
    /// be inspected safely. A reparse point is rejected rather than followed.
    fn discover(&self) -> Result<Vec<DiscoveredPackageV1>, PackageSourceErrorV1>;
}

/// Built-in package source whose packages always require a trusted signature.
#[derive(Clone)]
pub struct BuiltInPackageSourceV1 {
    root: PathBuf,
}

impl BuiltInPackageSourceV1 {
    /// Creates a built-in source rooted at a host-owned package directory.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl fmt::Debug for BuiltInPackageSourceV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BuiltInPackageSourceV1 { root: <redacted> }")
    }
}

impl PackageSourceV1 for BuiltInPackageSourceV1 {
    fn kind(&self) -> PackageSourceKindV1 {
        PackageSourceKindV1::BuiltIn
    }

    fn discover(&self) -> Result<Vec<DiscoveredPackageV1>, PackageSourceErrorV1> {
        discover_direct_children(self.kind(), &self.root, None)
    }
}

/// Host-policy-created local-developer source that grants unsigned-development provenance.
///
/// A future composition-root factory must decide whether developer mode and an
/// approved local root are active before it constructs this source. Keeping the
/// constructor inside the host crate prevents an arbitrary host-crate consumer
/// from minting unsigned-package provenance for an arbitrary path.
#[derive(Clone)]
pub struct LocalDeveloperPackageSourceV1 {
    root: PathBuf,
}

impl LocalDeveloperPackageSourceV1 {
    /// Creates a local-developer source after a host-controlled policy decision.
    #[must_use]
    #[allow(dead_code)] // A later composition-root policy factory is its only non-test caller.
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl fmt::Debug for LocalDeveloperPackageSourceV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LocalDeveloperPackageSourceV1 { root: <redacted> }")
    }
}

impl PackageSourceV1 for LocalDeveloperPackageSourceV1 {
    fn kind(&self) -> PackageSourceKindV1 {
        PackageSourceKindV1::LocalDeveloper
    }

    fn discover(&self) -> Result<Vec<DiscoveredPackageV1>, PackageSourceErrorV1> {
        let authorization = LocalDeveloperAuthorizationV1::issue();
        discover_direct_children(self.kind(), &self.root, Some(&authorization))
    }
}

/// A source candidate whose validation provenance cannot be forged by callers.
#[derive(Clone)]
pub struct DiscoveredPackageV1 {
    source_kind: PackageSourceKindV1,
    request: PackageValidationRequestV1,
}

impl fmt::Debug for DiscoveredPackageV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscoveredPackageV1")
            .field("source_kind", &self.source_kind)
            .field("request", &"<opaque>")
            .finish()
    }
}

impl DiscoveredPackageV1 {
    /// Returns the policy-bearing source that discovered this candidate.
    #[must_use]
    pub const fn source_kind(&self) -> PackageSourceKindV1 {
        self.source_kind
    }

    /// Validates this candidate and seals its accepted generation for activation.
    ///
    /// # Errors
    ///
    /// Returns a pre-load package validation failure. Discovery provenance stays
    /// private so callers cannot authorize arbitrary unsigned package paths.
    pub fn validate(
        &self,
        validator: &PackageValidatorV1,
    ) -> Result<PackageValidationResultV1, PackageValidationErrorV1> {
        validator.validate(&self.request)
    }
}

/// Typed failure while safely enumerating a package source.
#[derive(Debug, Error)]
pub enum PackageSourceErrorV1 {
    /// The configured source root does not exist.
    #[error("package source {kind:?} root does not exist: {path}")]
    MissingRoot {
        /// The source policy being enumerated.
        kind: PackageSourceKindV1,
        /// The unavailable configured root.
        path: PathBuf,
    },
    /// The configured source root is not a directory.
    #[error("package source {kind:?} root is not a directory: {path}")]
    RootNotDirectory {
        /// The source policy being enumerated.
        kind: PackageSourceKindV1,
        /// The invalid configured root.
        path: PathBuf,
    },
    /// A source root, candidate, or candidate manifest is a reparse point.
    #[error("package source {kind:?} contains a reparse point at {path}")]
    ReparsePoint {
        /// The source policy being enumerated.
        kind: PackageSourceKindV1,
        /// The unsafe filesystem path.
        path: PathBuf,
    },
    /// Filesystem inspection failed while discovering candidates.
    #[error("could not inspect package source {kind:?} path {path}: {source}")]
    Io {
        /// The source policy being enumerated.
        kind: PackageSourceKindV1,
        /// The path that could not be inspected.
        path: PathBuf,
        /// The operating-system error.
        #[source]
        source: io::Error,
    },
    /// The source root exceeds its bounded direct-child discovery limit.
    #[error("package source {kind:?} exceeds the {maximum}-entry discovery limit")]
    DirectChildLimitExceeded {
        /// The source policy being enumerated.
        kind: PackageSourceKindV1,
        /// The fixed maximum number of direct entries inspected per source root.
        maximum: usize,
    },
}

/// Owned request supplied to a replaceable entitlement provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntitlementRequestV1 {
    /// Stable package ID supplied by a future resolver after package validation.
    pub package_id: String,
    /// Stable package release version supplied by a future resolver.
    pub package_version: String,
    /// The source policy that supplied the resolved package.
    pub source_kind: PackageSourceKindV1,
}

impl EntitlementRequestV1 {
    /// Creates an owned entitlement request without binding this host to a store.
    #[must_use]
    pub fn new(
        package_id: String,
        package_version: String,
        source_kind: PackageSourceKindV1,
    ) -> Self {
        Self {
            package_id,
            package_version,
            source_kind,
        }
    }
}

/// Owned decision returned by an entitlement provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntitlementDecisionV1 {
    /// The caller may use the requested package.
    Granted,
    /// The caller is not entitled to use the package.
    Denied {
        /// Provider-supplied, user-displayable denial context.
        reason: String,
    },
}

/// Owned provider failure that is distinct from an entitlement denial.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EntitlementErrorV1 {
    /// The provider could not reach or evaluate its backing service.
    #[error("entitlement provider is unavailable: {message}")]
    Unavailable {
        /// Provider-supplied diagnostic text.
        message: String,
    },
    /// The provider rejected a malformed or unsupported request.
    #[error("entitlement provider rejected the request: {message}")]
    Rejected {
        /// Provider-supplied diagnostic text.
        message: String,
    },
}

/// Replaceable host entitlement boundary.
///
/// The v1 host intentionally supplies no Steamworks, store, or Pro implementation.
/// A future integration can implement this trait without altering package discovery.
pub trait EntitlementProviderV1: Send + Sync {
    /// Evaluates an owned request and returns an owned decision or provider failure.
    ///
    /// # Errors
    ///
    /// Returns a typed provider failure when entitlement cannot be evaluated.
    fn evaluate(
        &self,
        request: EntitlementRequestV1,
    ) -> Result<EntitlementDecisionV1, EntitlementErrorV1>;
}

fn discover_direct_children(
    kind: PackageSourceKindV1,
    root: &Path,
    local_developer_authorization: Option<&LocalDeveloperAuthorizationV1>,
) -> Result<Vec<DiscoveredPackageV1>, PackageSourceErrorV1> {
    validate_root(kind, root)?;
    let entries = fs::read_dir(root).map_err(|source| source_error(kind, root, source))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| source_error(kind, root, source))?;
        if paths.len() == MAX_DIRECT_CHILDREN_V1 {
            return Err(PackageSourceErrorV1::DirectChildLimitExceeded {
                kind,
                maximum: MAX_DIRECT_CHILDREN_V1,
            });
        }
        paths.push(entry.path());
    }
    paths.sort();

    let mut packages = Vec::new();
    for candidate in paths {
        let candidate_metadata = symlink_metadata(kind, &candidate)?;
        if is_reparse_point(&candidate_metadata) {
            return Err(PackageSourceErrorV1::ReparsePoint {
                kind,
                path: candidate,
            });
        }
        if !candidate_metadata.is_dir() {
            continue;
        }

        let manifest = candidate.join("manifest.json");
        let manifest_metadata = match fs::symlink_metadata(&manifest) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(source_error(kind, &manifest, source)),
        };
        if is_reparse_point(&manifest_metadata) {
            return Err(PackageSourceErrorV1::ReparsePoint {
                kind,
                path: manifest,
            });
        }
        if !manifest_metadata.is_file() {
            continue;
        }

        // Check the directory again after inspecting its manifest so a replacement
        // race cannot turn a discovered candidate into a reparse-point path.
        if is_reparse_point(&symlink_metadata(kind, &candidate)?) {
            return Err(PackageSourceErrorV1::ReparsePoint {
                kind,
                path: candidate,
            });
        }

        let mut request = PackageValidationRequestV1::new(candidate);
        if let Some(authorization) = local_developer_authorization {
            request = request.with_local_developer_authorization(authorization.clone());
        }
        packages.push(DiscoveredPackageV1 {
            source_kind: kind,
            request,
        });
    }
    Ok(packages)
}

fn validate_root(kind: PackageSourceKindV1, root: &Path) -> Result<(), PackageSourceErrorV1> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(PackageSourceErrorV1::MissingRoot {
                kind,
                path: root.to_path_buf(),
            });
        }
        Err(source) => return Err(source_error(kind, root, source)),
    };
    if is_reparse_point(&metadata) {
        return Err(PackageSourceErrorV1::ReparsePoint {
            kind,
            path: root.to_path_buf(),
        });
    }
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(PackageSourceErrorV1::RootNotDirectory {
            kind,
            path: root.to_path_buf(),
        })
    }
}

fn symlink_metadata(
    kind: PackageSourceKindV1,
    path: &Path,
) -> Result<fs::Metadata, PackageSourceErrorV1> {
    fs::symlink_metadata(path).map_err(|source| source_error(kind, path, source))
}

fn source_error(kind: PackageSourceKindV1, path: &Path, source: io::Error) -> PackageSourceErrorV1 {
    PackageSourceErrorV1::Io {
        kind,
        path: path.to_path_buf(),
        source,
    }
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Write as _,
        fs, io,
        path::Path,
        process::{Command, Stdio},
    };

    use serde_json::json;
    use sha2::{Digest as _, Sha256};
    use tempfile::TempDir;

    use super::{
        BuiltInPackageSourceV1, EntitlementDecisionV1, EntitlementErrorV1, EntitlementProviderV1,
        EntitlementRequestV1, LocalDeveloperPackageSourceV1, PackageSourceErrorV1,
        PackageSourceKindV1, PackageSourceV1,
    };
    use crate::{
        PackageValidationErrorV1, PackageValidatorV1, SealedPackageStoreV1,
        TrustedPublisherKeyStoreV1,
    };

    fn unsigned_manifest(payload: &[u8]) -> String {
        let payload_hash = sha256_hex(payload);
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
            "features": [], "dependencies": [],
            "payloads": [{ "path": "data/payload.bin", "size": payload.len(), "sha256": payload_hash, "kind": "data" }],
            "signature": { "kind": "unsigned" },
            "data_version": 1
        })
        .to_string()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(64);
        for byte in Sha256::digest(bytes) {
            write!(&mut hex, "{byte:02x}").expect("write fixed-size SHA-256 hex");
        }
        hex
    }

    fn write_package(root: &Path, name: &str, payload: &[u8]) {
        let package = root.join(name);
        fs::create_dir_all(package.join("data")).expect("create package data directory");
        fs::write(package.join("data/payload.bin"), payload).expect("write package payload");
        fs::write(package.join("manifest.json"), unsigned_manifest(payload))
            .expect("write package manifest");
    }

    fn validator(temp: &TempDir) -> PackageValidatorV1 {
        let sealed_store =
            SealedPackageStoreV1::new(&temp.path().join("sealed")).expect("create sealed store");
        PackageValidatorV1::new(TrustedPublisherKeyStoreV1::default(), sealed_store)
    }

    #[test]
    fn discovery_is_direct_child_manifest_only_and_deterministic() {
        let temp = TempDir::new().expect("temporary source root");
        let root = temp.path().join("packages");
        fs::create_dir(&root).expect("create source root");
        write_package(&root, "zeta", b"zeta");
        write_package(&root, "alpha", b"alpha");
        fs::create_dir(root.join("not-a-package")).expect("create ignored directory");
        fs::write(root.join("not-a-package/other.json"), b"{}").expect("write ignored file");
        write_package(&root.join("not-a-package"), "nested", b"nested");
        fs::write(root.join("ordinary-file"), b"not a package").expect("write ignored file");

        let source = LocalDeveloperPackageSourceV1::new(root.clone());
        let discovered = source.discover().expect("discover direct packages");
        assert_eq!(discovered.len(), 2);
        assert!(
            discovered
                .iter()
                .all(|package| package.source_kind() == PackageSourceKindV1::LocalDeveloper)
        );
        let validator = validator(&temp);
        let manifests = discovered
            .iter()
            .map(|package| {
                package
                    .validate(&validator)
                    .expect("validate discovered local package")
                    .manifest_digest
            })
            .collect::<Vec<_>>();
        let expected = ["alpha", "zeta"]
            .into_iter()
            .map(|name| {
                sha256_hex(
                    &fs::read(root.join(name).join("manifest.json"))
                        .expect("read known source manifest"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(manifests, expected, "candidates must be name-sorted");
    }

    #[test]
    fn discovery_reports_missing_not_directory_and_io_roots() {
        let temp = TempDir::new().expect("temporary source root");
        let missing = BuiltInPackageSourceV1::new(temp.path().join("missing"));
        assert!(matches!(
            missing.discover(),
            Err(PackageSourceErrorV1::MissingRoot { .. })
        ));

        let file = temp.path().join("source-file");
        fs::write(&file, b"not a directory").expect("write source file");
        let not_directory = BuiltInPackageSourceV1::new(file);
        assert!(matches!(
            not_directory.discover(),
            Err(PackageSourceErrorV1::RootNotDirectory { .. })
        ));

        let io_error = super::source_error(
            PackageSourceKindV1::BuiltIn,
            temp.path(),
            io::Error::other("synthetic source I/O failure"),
        );
        assert!(matches!(io_error, PackageSourceErrorV1::Io { .. }));
    }

    #[test]
    fn discovery_bounds_the_direct_child_scan() {
        let temp = TempDir::new().expect("temporary source root");
        let root = temp.path().join("packages");
        fs::create_dir(&root).expect("create source root");
        for index in 0..=super::MAX_DIRECT_CHILDREN_V1 {
            fs::write(root.join(format!("ignored-{index}")), b"ignored")
                .expect("write bounded-scan fixture");
        }

        assert!(matches!(
            BuiltInPackageSourceV1::new(root).discover(),
            Err(PackageSourceErrorV1::DirectChildLimitExceeded {
                maximum: super::MAX_DIRECT_CHILDREN_V1,
                ..
            })
        ));
    }

    #[cfg(windows)]
    #[test]
    fn discovery_rejects_reparse_roots_and_candidates() {
        let temp = TempDir::new().expect("temporary source root");
        let target = temp.path().join("target");
        fs::create_dir(&target).expect("create junction target");
        let reparse_root = temp.path().join("reparse-root");
        create_junction(&reparse_root, &target);
        assert!(matches!(
            BuiltInPackageSourceV1::new(reparse_root).discover(),
            Err(PackageSourceErrorV1::ReparsePoint { .. })
        ));

        let root = temp.path().join("packages");
        fs::create_dir(&root).expect("create package root");
        let reparse_candidate = root.join("candidate");
        create_junction(&reparse_candidate, &target);
        assert!(matches!(
            BuiltInPackageSourceV1::new(root).discover(),
            Err(PackageSourceErrorV1::ReparsePoint { .. })
        ));
    }

    #[cfg(windows)]
    fn create_junction(link: &Path, target: &Path) {
        let status = Command::new("cmd")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run mklink");
        assert!(status.success(), "create test junction");
    }

    #[test]
    fn built_in_unsigned_is_rejected_and_local_unsigned_is_sealed() {
        let temp = TempDir::new().expect("temporary source root");
        let root = temp.path().join("packages");
        fs::create_dir(&root).expect("create source root");
        write_package(&root, "example", b"local developer payload");
        let validator = validator(&temp);

        let built_in = BuiltInPackageSourceV1::new(root.clone())
            .discover()
            .expect("discover built-in package")
            .pop()
            .expect("one built-in package");
        assert!(matches!(
            built_in.validate(&validator),
            Err(PackageValidationErrorV1::SignatureRequired)
        ));

        let local = LocalDeveloperPackageSourceV1::new(root)
            .discover()
            .expect("discover local package")
            .pop()
            .expect("one local package");
        let result = local
            .validate(&validator)
            .expect("validate and seal local package");
        let guard = result
            .activation_guard()
            .expect("open sealed local package");
        assert!(guard.package_root().join("data/payload.bin").is_file());
    }

    struct FakeSource;

    impl PackageSourceV1 for FakeSource {
        fn kind(&self) -> PackageSourceKindV1 {
            PackageSourceKindV1::BuiltIn
        }

        fn discover(&self) -> Result<Vec<super::DiscoveredPackageV1>, PackageSourceErrorV1> {
            Ok(Vec::new())
        }
    }

    struct FakeProvider;

    impl EntitlementProviderV1 for FakeProvider {
        fn evaluate(
            &self,
            request: EntitlementRequestV1,
        ) -> Result<EntitlementDecisionV1, EntitlementErrorV1> {
            if request.package_id == "example.package" {
                Ok(EntitlementDecisionV1::Granted)
            } else {
                Ok(EntitlementDecisionV1::Denied {
                    reason: "unknown package".to_owned(),
                })
            }
        }
    }

    #[test]
    fn source_and_entitlement_boundaries_are_replaceable() {
        let source: &dyn PackageSourceV1 = &FakeSource;
        assert_eq!(source.kind(), PackageSourceKindV1::BuiltIn);
        assert!(source.discover().expect("fake discovery").is_empty());

        let provider: &dyn EntitlementProviderV1 = &FakeProvider;
        let decision = provider
            .evaluate(EntitlementRequestV1::new(
                "example.package".to_owned(),
                "1.0.0".to_owned(),
                PackageSourceKindV1::BuiltIn,
            ))
            .expect("fake entitlement evaluation");
        assert_eq!(decision, EntitlementDecisionV1::Granted);
    }
}
