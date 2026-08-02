//! Versioned, data-only `.sepack` `manifest.json` contract.
//!
//! `PackageManifestV1` is deliberately limited to decoding and invariant checks
//! that can be completed without opening a package.  Publisher contact policy,
//! signature verification, path containment, payload hashing, target checks, and
//! dependency resolution belong to their dedicated host stages.  Keeping those
//! stages separate means decoding untrusted JSON cannot accidentally imply that a
//! package has been trusted or loaded.
//!
//! # V1 wire contract
//!
//! The JSON document has `manifest_version: 1` and no unknown fields at any
//! modelled level.  Its required top-level fields are `package`, `publisher`,
//! `sdk`, `rust`, `lua`, `skins`, `locales`, `tools`, `features`, `dependencies`,
//! `payloads`, `signature`, and `data_version`.  Empty arrays are explicit; no
//! omitted field has a capability- or security-affecting default. The document
//! is bounded to 256 KiB before decoding; V1 collection and string fields have
//! their own fixed bounds. IDs are normalized lowercase ASCII
//! (`[a-z0-9][a-z0-9._-]{0,63}`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The only package-manifest revision understood by this host model.
pub const PACKAGE_MANIFEST_VERSION_V1: u32 = 1;

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_TOP_LEVEL_ENTRIES: usize = 128;
const MAX_CONTACTS: usize = 32;
const MAX_CONTACT_PURPOSES: usize = 8;
const MAX_TOOL_LICENSE_PATHS: usize = 32;
const MAX_FEATURE_CAPABILITIES: usize = 64;
const MAX_FEATURE_DEPENDENCIES: usize = 64;
const MAX_DISPLAY_NAME_BYTES: usize = 256;
const MAX_CONTACT_VALUE_BYTES: usize = 2_048;
const MAX_VERSION_BYTES: usize = 128;
const MAX_TARGET_BYTES: usize = 128;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_LOCALE_BYTES: usize = 64;
const MAX_SOURCE_BYTES: usize = 2_048;
const MAX_SIGNATURE_BYTES: usize = 1_024;

/// Fully decoded `manifest.json` for one `.sepack` package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifestV1 {
    /// Explicit schema revision for this document.
    pub manifest_version: u32,
    /// Package identity and release version.
    pub package: PackageIdentityV1,
    /// Public publisher metadata, including the V1 contact-policy requirements.
    pub publisher: PublisherV1,
    /// SDK and target compatibility declaration.
    pub sdk: SdkCompatibilityV1,
    /// Native Rust entry points contained by the package.
    pub rust: Vec<RustEntrypointV1>,
    /// Lua entry points contained by the package.
    pub lua: Vec<LuaEntrypointV1>,
    /// Declarative Skin entry points contained by the package.
    pub skins: Vec<SkinEntrypointV1>,
    /// Localized resource declarations.
    pub locales: Vec<LocaleResourceV1>,
    /// Bundled executable metadata.
    pub tools: Vec<BundledToolV1>,
    /// Independently controllable package features.
    pub features: Vec<PackageFeatureV1>,
    /// Package-level version requirements.
    pub dependencies: Vec<PackageDependencyV1>,
    /// Inventory of package payload metadata and declared hashes.
    pub payloads: Vec<PayloadV1>,
    /// Explicit unsigned or signed package declaration.
    pub signature: SignatureV1,
    /// Plugin-owned data/cache compatibility generation.
    pub data_version: u64,
}

impl PackageManifestV1 {
    /// Decodes and performs V1 structural validation of a `manifest.json` value.
    ///
    /// This function does not open payload files, inspect reparse points, verify
    /// digest bytes, verify signatures, or resolve dependencies. Those operations
    /// require package context and intentionally remain outside the parser contract.
    ///
    /// # Errors
    ///
    /// Returns a typed syntax, version, identifier, duplicate, or hash-format
    /// error when the document is not a valid `PackageManifestV1`.
    pub fn parse_json(source: &str) -> Result<Self, PackageManifestErrorV1> {
        if source.len() > MAX_MANIFEST_BYTES {
            return Err(PackageManifestErrorV1::ManifestTooLarge {
                actual: source.len(),
                maximum: MAX_MANIFEST_BYTES,
            });
        }
        let manifest: Self = serde_json::from_str(source).map_err(PackageManifestErrorV1::Json)?;
        manifest.validate_structure()?;
        Ok(manifest)
    }

    /// Validates the V1 public publisher-contact policy before package acceptance.
    ///
    /// [`Self::parse_json`] invokes this policy as part of structural validation.
    /// It is also public so callers constructing this Rust type directly can apply
    /// the same policy before accepting a package.
    ///
    /// # Errors
    ///
    /// Returns a typed error when no public contact exists, a contact is blank or
    /// has no purpose, purposes or contact declarations are duplicated, one value
    /// is assigned incompatible contact kinds, or neither support nor security is
    /// declared.
    pub fn validate_publisher_contact_policy(&self) -> Result<(), PackageManifestErrorV1> {
        if self.publisher.contacts.is_empty() {
            return Err(PackageManifestErrorV1::MissingPublisherContact);
        }

        let mut contacts = BTreeSet::new();
        let mut kinds_by_canonical_value = BTreeMap::new();
        let mut has_support_or_security = false;

        for contact in &self.publisher.contacts {
            let canonical_value = canonical_contact_value(contact.kind, &contact.value)?;

            if contact.purposes.is_empty() {
                return Err(PackageManifestErrorV1::MissingPublisherContactPurpose {
                    kind: contact.kind,
                    value: contact.value.clone(),
                });
            }

            if !contacts.insert((contact.kind, canonical_value.clone())) {
                return Err(PackageManifestErrorV1::DuplicatePublisherContact {
                    kind: contact.kind,
                    value: contact.value.clone(),
                });
            }

            if let Some(previous_kind) =
                kinds_by_canonical_value.insert(canonical_value, contact.kind)
                && previous_kind != contact.kind
            {
                return Err(PackageManifestErrorV1::ConflictingPublisherContactKinds {
                    value: contact.value.clone(),
                    first_kind: previous_kind,
                    conflicting_kind: contact.kind,
                });
            }

            let mut purposes = BTreeSet::new();
            for purpose in &contact.purposes {
                if !purposes.insert(*purpose) {
                    return Err(PackageManifestErrorV1::DuplicatePublisherContactPurpose {
                        kind: contact.kind,
                        value: contact.value.clone(),
                        purpose: *purpose,
                    });
                }
                has_support_or_security |= matches!(
                    purpose,
                    ContactPurposeV1::Support | ContactPurposeV1::Security
                );
            }
        }

        if has_support_or_security {
            Ok(())
        } else {
            Err(PackageManifestErrorV1::MissingSupportOrSecurityContact)
        }
    }

    /// Compares an opaque signer identity already produced by a signature verifier
    /// with this manifest's publisher ID.
    ///
    /// This method performs neither signature verification nor a comparison with
    /// [`SignatureV1::Ed25519`] `key_id`; key IDs and publisher IDs are separate
    /// identities. [`VerifiedPublisherIdentityV1`] can only be constructed inside
    /// this crate, by the future signature-verification stage.
    ///
    /// # Errors
    ///
    /// Returns [`PackageManifestErrorV1::SignedPublisherMismatch`] when the two
    /// normalized publisher IDs differ.
    pub fn validate_verified_signer_publisher_identity(
        &self,
        verified_signer: &VerifiedPublisherIdentityV1,
    ) -> Result<(), PackageManifestErrorV1> {
        if matches!(self.signature, SignatureV1::Unsigned) {
            return Err(PackageManifestErrorV1::VerifiedSignerIdentityRequiresSignature);
        }
        if self.publisher.id == verified_signer.publisher_id {
            Ok(())
        } else {
            Err(PackageManifestErrorV1::SignedPublisherMismatch {
                manifest_publisher_id: self.publisher.id.clone(),
                verified_signer_publisher_id: verified_signer.publisher_id.clone(),
            })
        }
    }

    /// Produces the deterministic, domain-separated message for an Ed25519
    /// package signature.
    ///
    /// The message is canonical JSON for this validated V1 struct with the
    /// signature bytes replaced by an empty string while retaining the Ed25519
    /// key ID, prefixed by a fixed domain separator. It therefore commits to all
    /// package metadata without a self-reference to the signature value itself.
    pub(crate) fn canonical_ed25519_signing_bytes(
        &self,
    ) -> Result<Vec<u8>, PackageManifestErrorV1> {
        let SignatureV1::Ed25519 { key_id, .. } = &self.signature else {
            return Err(PackageManifestErrorV1::VerifiedSignerIdentityRequiresSignature);
        };
        let mut unsigned_copy = self.clone();
        unsigned_copy.signature = SignatureV1::Ed25519 {
            key_id: key_id.clone(),
            signature: String::new(),
        };
        let canonical_json = serde_json::to_vec(&unsigned_copy)
            .map_err(PackageManifestErrorV1::SignaturePayloadSerialization)?;
        let mut message = b"SuperExplorer.sepack.manifest.v1\0".to_vec();
        message.extend_from_slice(&canonical_json);
        Ok(message)
    }

    fn validate_structure(&self) -> Result<(), PackageManifestErrorV1> {
        if self.manifest_version != PACKAGE_MANIFEST_VERSION_V1 {
            return Err(PackageManifestErrorV1::UnsupportedManifestVersion {
                actual: self.manifest_version,
            });
        }

        validate_id("package.id", &self.package.id)?;
        validate_string_length("package.version", &self.package.version, MAX_VERSION_BYTES)?;
        validate_id("publisher.id", &self.publisher.id)?;
        validate_string_length(
            "publisher.display_name",
            &self.publisher.display_name,
            MAX_DISPLAY_NAME_BYTES,
        )?;
        if self.publisher.display_name.trim().is_empty() {
            return Err(PackageManifestErrorV1::EmptyPublisherDisplayName);
        }
        validate_id("sdk.bundle_id", &self.sdk.bundle_id)?;
        validate_string_length("sdk.target", &self.sdk.target, MAX_TARGET_BYTES)?;
        match (self.sdk.gpui, self.sdk.ui_abi_fingerprint.as_deref()) {
            (true, Some(fingerprint)) => {
                validate_sha256("sdk.ui_abi_fingerprint", fingerprint)?;
            }
            (true, None) => return Err(PackageManifestErrorV1::MissingGpuiFingerprint),
            (false, Some(_)) => return Err(PackageManifestErrorV1::UnexpectedGpuiFingerprint),
            (false, None) => {}
        }

        validate_collection_length(
            "publisher.contacts",
            self.publisher.contacts.len(),
            MAX_CONTACTS,
        )?;
        for contact in &self.publisher.contacts {
            validate_string_length(
                "publisher.contacts[].value",
                &contact.value,
                MAX_CONTACT_VALUE_BYTES,
            )?;
            validate_collection_length(
                "publisher.contacts[].purposes",
                contact.purposes.len(),
                MAX_CONTACT_PURPOSES,
            )?;
        }
        self.validate_publisher_contact_policy()?;

        validate_collection_length("rust", self.rust.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids("rust", self.rust.iter().map(|entry| entry.id.as_str()))?;
        for entry in &self.rust {
            validate_id("rust[].id", &entry.id)?;
            validate_id("rust[].root_module", &entry.root_module)?;
            validate_string_length("rust[].entrypoint", &entry.entrypoint, MAX_PATH_BYTES)?;
        }
        validate_collection_length("lua", self.lua.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids("lua", self.lua.iter().map(|entry| entry.id.as_str()))?;
        for entry in &self.lua {
            validate_id("lua[].id", &entry.id)?;
            validate_string_length("lua[].entrypoint", &entry.entrypoint, MAX_PATH_BYTES)?;
        }
        validate_collection_length("skins", self.skins.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids("skins", self.skins.iter().map(|entry| entry.id.as_str()))?;
        for entry in &self.skins {
            validate_id("skins[].id", &entry.id)?;
            validate_string_length("skins[].entrypoint", &entry.entrypoint, MAX_PATH_BYTES)?;
        }
        validate_collection_length("tools", self.tools.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids("tools", self.tools.iter().map(|tool| tool.id.as_str()))?;
        for tool in &self.tools {
            validate_id("tools[].id", &tool.id)?;
            validate_string_length("tools[].target", &tool.target, MAX_TARGET_BYTES)?;
            validate_string_length("tools[].path", &tool.path, MAX_PATH_BYTES)?;
            validate_string_length("tools[].version", &tool.version, MAX_VERSION_BYTES)?;
            validate_sha256("tools[].sha256", &tool.sha256)?;
            validate_string_length("tools[].source", &tool.source, MAX_SOURCE_BYTES)?;
            validate_collection_length(
                "tools[].license_paths",
                tool.license_paths.len(),
                MAX_TOOL_LICENSE_PATHS,
            )?;
            for license_path in &tool.license_paths {
                validate_string_length("tools[].license_paths", license_path, MAX_PATH_BYTES)?;
            }
        }
        validate_collection_length("features", self.features.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids(
            "features",
            self.features.iter().map(|feature| feature.id.as_str()),
        )?;
        for feature in &self.features {
            validate_id("features[].id", &feature.id)?;
            validate_collection_length(
                "features[].capabilities",
                feature.capabilities.len(),
                MAX_FEATURE_CAPABILITIES,
            )?;
            validate_unique_ids(
                "features[].capabilities",
                feature.capabilities.iter().map(String::as_str),
            )?;
            for capability in &feature.capabilities {
                validate_id("features[].capabilities", capability)?;
            }
            validate_collection_length(
                "features[].dependencies",
                feature.dependencies.len(),
                MAX_FEATURE_DEPENDENCIES,
            )?;
            validate_unique_ids(
                "features[].dependencies",
                feature.dependencies.iter().map(String::as_str),
            )?;
            for dependency in &feature.dependencies {
                validate_id("features[].dependencies", dependency)?;
            }
        }
        validate_collection_length(
            "dependencies",
            self.dependencies.len(),
            MAX_TOP_LEVEL_ENTRIES,
        )?;
        validate_unique_ids(
            "dependencies",
            self.dependencies
                .iter()
                .map(|dependency| dependency.package_id.as_str()),
        )?;
        for dependency in &self.dependencies {
            validate_id("dependencies[].package_id", &dependency.package_id)?;
            validate_string_length(
                "dependencies[].version_requirement",
                &dependency.version_requirement,
                MAX_VERSION_BYTES,
            )?;
        }
        validate_collection_length("payloads", self.payloads.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids(
            "payloads",
            self.payloads.iter().map(|payload| payload.path.as_str()),
        )?;
        for payload in &self.payloads {
            validate_string_length("payloads[].path", &payload.path, MAX_PATH_BYTES)?;
            validate_sha256("payloads[].sha256", &payload.sha256)?;
        }
        validate_collection_length("locales", self.locales.len(), MAX_TOP_LEVEL_ENTRIES)?;
        validate_unique_ids(
            "locales",
            self.locales.iter().map(|locale| locale.locale.as_str()),
        )?;
        for locale in &self.locales {
            validate_string_length("locales[].locale", &locale.locale, MAX_LOCALE_BYTES)?;
            validate_string_length("locales[].path", &locale.path, MAX_PATH_BYTES)?;
            validate_sha256("locales[].sha256", &locale.sha256)?;
        }

        if let SignatureV1::Ed25519 { key_id, signature } = &self.signature {
            validate_id("signature.key_id", key_id)?;
            validate_string_length("signature.signature", signature, MAX_SIGNATURE_BYTES)?;
        }
        Ok(())
    }
}

/// Typed manifest decoding or structural-validation failure.
#[derive(Debug, Error)]
pub enum PackageManifestErrorV1 {
    /// JSON was malformed, used an unknown field, or had the wrong value type.
    #[error("invalid package manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The host could not serialize the fixed V1 signature payload.
    #[error("could not serialize the canonical V1 signature payload: {0}")]
    SignaturePayloadSerialization(serde_json::Error),
    /// The document declares a schema revision this host does not implement.
    #[error(
        "unsupported package manifest version {actual}; expected {PACKAGE_MANIFEST_VERSION_V1}"
    )]
    UnsupportedManifestVersion { actual: u32 },
    /// The raw `manifest.json` document exceeded the parser's pre-deserialization bound.
    #[error("package manifest is {actual} bytes, exceeding the {maximum}-byte limit")]
    ManifestTooLarge { actual: usize, maximum: usize },
    /// A decoded manifest string exceeded its bounded V1 field size.
    #[error("string at {field} is {actual} bytes, exceeding the {maximum}-byte limit")]
    StringTooLong {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    /// A decoded manifest collection exceeded its bounded V1 cardinality.
    #[error("collection at {field} has {actual} items, exceeding the {maximum}-item limit")]
    CollectionTooLong {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    /// A publisher display name was empty or whitespace-only after trimming.
    #[error("publisher.display_name must not be empty")]
    EmptyPublisherDisplayName,
    /// A package did not provide any public publisher contact.
    #[error("publisher.contacts must contain at least one public contact")]
    MissingPublisherContact,
    /// A public contact value was empty or whitespace-only.
    #[error("publisher contact value for {kind:?} must not be empty")]
    EmptyPublisherContactValue { kind: PublisherContactKindV1 },
    /// A public contact value had whitespace, controls, or did not match its kind.
    #[error("invalid publisher contact value for {kind:?}: {value:?}")]
    InvalidPublisherContactValue {
        kind: PublisherContactKindV1,
        value: String,
    },
    /// A public contact did not declare a purpose.
    #[error("publisher contact {kind:?} {value:?} must declare at least one purpose")]
    MissingPublisherContactPurpose {
        kind: PublisherContactKindV1,
        value: String,
    },
    /// A public contact was declared more than once with the same kind and value.
    #[error("duplicate publisher contact {kind:?} {value:?}")]
    DuplicatePublisherContact {
        kind: PublisherContactKindV1,
        value: String,
    },
    /// One public contact value was assigned incompatible contact kinds.
    #[error(
        "conflicting publisher contact kinds for {value:?}: {first_kind:?} and {conflicting_kind:?}"
    )]
    ConflictingPublisherContactKinds {
        value: String,
        first_kind: PublisherContactKindV1,
        conflicting_kind: PublisherContactKindV1,
    },
    /// A public contact declared a purpose more than once.
    #[error("duplicate publisher contact purpose {purpose:?} for {kind:?} {value:?}")]
    DuplicatePublisherContactPurpose {
        kind: PublisherContactKindV1,
        value: String,
        purpose: ContactPurposeV1,
    },
    /// No public contact provided a support or security channel.
    #[error("publisher.contacts must declare at least one support or security purpose")]
    MissingSupportOrSecurityContact,
    /// An externally verified signer identity may only be compared for a signed package.
    #[error("an externally verified signer identity requires an ed25519 package signature")]
    VerifiedSignerIdentityRequiresSignature,
    /// A verified signer identity differs from the publisher declared by the manifest.
    #[error(
        "verified signer publisher ID {verified_signer_publisher_id:?} does not match manifest publisher ID {manifest_publisher_id:?}"
    )]
    SignedPublisherMismatch {
        manifest_publisher_id: String,
        verified_signer_publisher_id: String,
    },
    /// A GPUI package must declare its exact canonical UI ABI fingerprint.
    #[error("sdk.gpui is true but sdk.ui_abi_fingerprint is missing")]
    MissingGpuiFingerprint,
    /// A non-GPUI package must explicitly use `null` for its UI ABI fingerprint.
    #[error("sdk.gpui is false but sdk.ui_abi_fingerprint is non-null")]
    UnexpectedGpuiFingerprint,
    /// A stable identifier did not use the canonical V1 spelling.
    #[error("invalid normalized identifier at {field}: {value:?}")]
    InvalidIdentifier { field: &'static str, value: String },
    /// A collection contains the same identity more than once.
    #[error("duplicate identifier in {field}: {value:?}")]
    DuplicateIdentifier { field: &'static str, value: String },
    /// A declared SHA-256 digest was not 64 lowercase hexadecimal characters.
    #[error("invalid SHA-256 format at {field}: {value:?}")]
    InvalidSha256 { field: &'static str, value: String },
}

/// Stable package identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentityV1 {
    pub id: String,
    pub version: String,
}

/// Public publisher declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherV1 {
    pub id: String,
    pub display_name: String,
    pub contacts: Vec<PublisherContactV1>,
}

/// Opaque publisher identity emitted only by the package signature verifier.
///
/// The value is deliberately private: manifest parsing and callers outside this
/// crate cannot forge a verified identity. The signature verifier creates this
/// value only after cryptographic verification succeeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPublisherIdentityV1 {
    publisher_id: String,
}

impl VerifiedPublisherIdentityV1 {
    /// Creates an identity after the crate's signature verifier has established it.
    pub(crate) fn new(publisher_id: String) -> Result<Self, PackageManifestErrorV1> {
        validate_id("verified_signer.publisher_id", &publisher_id)?;
        Ok(Self { publisher_id })
    }
}

/// A public publisher contact preserved for later contact-policy validation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherContactV1 {
    pub kind: PublisherContactKindV1,
    pub value: String,
    pub purposes: Vec<ContactPurposeV1>,
}

/// Supported contact kinds. `other` permits a future documented public channel.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherContactKindV1 {
    Email,
    Website,
    SupportForum,
    GithubIssues,
    DiscordServer,
    DiscordUser,
    QqGroup,
    Other,
}

/// Declared use of a public contact channel.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactPurposeV1 {
    Support,
    Security,
    Community,
}

/// SDK compatibility inputs declared by a package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdkCompatibilityV1 {
    pub bundle_id: String,
    pub target: String,
    pub abi_schema: u32,
    pub gpui: bool,
    pub ui_abi_fingerprint: Option<String>,
}

/// A Rust native entry point.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustEntrypointV1 {
    pub id: String,
    pub entrypoint: String,
    pub root_module: String,
    pub sdk_major: u16,
}

/// A Lua script entry point.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LuaEntrypointV1 {
    pub id: String,
    pub entrypoint: String,
}

/// A declarative Skin entry point.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinEntrypointV1 {
    pub id: String,
    pub entrypoint: String,
}

/// A localized resource and its declared digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleResourceV1 {
    pub locale: String,
    pub path: String,
    pub sha256: String,
}

/// Metadata for an executable bundled inside the package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundledToolV1 {
    pub id: String,
    pub target: String,
    pub path: String,
    pub version: String,
    pub size: u64,
    pub sha256: String,
    pub output_protocol: ToolOutputProtocolV1,
    pub source: String,
    pub license_paths: Vec<String>,
}

/// Expected output protocol for a bundled tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutputProtocolV1 {
    Json,
    Text,
    LineDelimitedJson,
}

/// One independently configurable feature.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFeatureV1 {
    pub id: String,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
}

/// A versioned dependency on another package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageDependencyV1 {
    pub package_id: String,
    pub version_requirement: String,
    pub optional: bool,
}

/// One content item whose bytes will be validated during package installation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadV1 {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub kind: PayloadKindV1,
}

/// The declarative category of a payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKindV1 {
    RustDll,
    LuaScript,
    SkinAsset,
    Locale,
    Tool,
    License,
    Notice,
    Data,
}

/// Explicit package signature declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SignatureV1 {
    /// The package is intentionally unsigned (such as a local developer build).
    Unsigned,
    /// An Ed25519 signature verified by the host before the package is sealed.
    Ed25519 { key_id: String, signature: String },
}

fn canonical_contact_value(
    kind: PublisherContactKindV1,
    value: &str,
) -> Result<String, PackageManifestErrorV1> {
    if value.trim().is_empty() {
        return Err(PackageManifestErrorV1::EmptyPublisherContactValue { kind });
    }
    if value.trim() != value
        || value.chars().any(char::is_control)
        || value.contains(char::is_whitespace)
    {
        return Err(PackageManifestErrorV1::InvalidPublisherContactValue {
            kind,
            value: value.to_owned(),
        });
    }

    let canonical = match kind {
        PublisherContactKindV1::Email => canonical_email(value),
        PublisherContactKindV1::Website | PublisherContactKindV1::SupportForum => {
            canonical_http_url(value)
        }
        PublisherContactKindV1::GithubIssues => canonical_github_issues_url(value),
        PublisherContactKindV1::DiscordServer => canonical_discord_server_url(value),
        PublisherContactKindV1::DiscordUser => canonical_discord_user(value),
        PublisherContactKindV1::QqGroup => canonical_qq_group(value),
        // `other` is a bounded, nonblank, whitespace-free public contact token.
        // Its service-specific syntax is intentionally opaque to V1.
        PublisherContactKindV1::Other => Some(value.to_owned()),
    };

    canonical.ok_or_else(|| PackageManifestErrorV1::InvalidPublisherContactValue {
        kind,
        value: value.to_owned(),
    })
}

fn canonical_email(value: &str) -> Option<String> {
    if !value.is_ascii() || value.matches('@').count() != 1 {
        return None;
    }
    let (local, domain) = value.split_once('@')?;
    if !(1..=64).contains(&local.len())
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'!'
                        | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'/'
                        | b'='
                        | b'?'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'{'
                        | b'|'
                        | b'}'
                        | b'~'
                )
        })
    {
        return None;
    }
    canonical_dns_name(domain)?;
    Some(value.to_ascii_lowercase())
}

fn canonical_http_url(value: &str) -> Option<String> {
    let (scheme, remainder) = value.split_once("://")?;
    let scheme = if scheme.eq_ignore_ascii_case("http") {
        "http"
    } else if scheme.eq_ignore_ascii_case("https") {
        "https"
    } else {
        return None;
    };
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    if authority.is_empty() || authority.contains(['@', ':']) {
        return None;
    }
    let host = canonical_dns_name(authority)?;
    let suffix = &remainder[authority_end..];
    let suffix = if suffix == "/" { "" } else { suffix };
    Some(format!("{scheme}://{host}{suffix}"))
}

fn canonical_github_issues_url(value: &str) -> Option<String> {
    let canonical = canonical_http_url(value)?;
    let remainder = canonical.strip_prefix("https://github.com/")?;
    if remainder.contains(['?', '#']) {
        return None;
    }
    let remainder = remainder.strip_suffix('/').unwrap_or(remainder);
    if remainder.ends_with('/') {
        return None;
    }
    let segments: Vec<_> = remainder.split('/').collect();
    match segments.as_slice() {
        [owner, repository, "issues"]
            if is_github_name(owner, 39) && is_github_name(repository, 100) =>
        {
            Some(format!(
                "https://github.com/{}/{}/issues",
                owner.to_ascii_lowercase(),
                repository.to_ascii_lowercase(),
            ))
        }
        [owner, repository, "issues", issue]
            if is_github_name(owner, 39)
                && is_github_name(repository, 100)
                && !issue.is_empty()
                && issue.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Some(format!(
                "https://github.com/{}/{}/issues/{issue}",
                owner.to_ascii_lowercase(),
                repository.to_ascii_lowercase(),
            ))
        }
        _ => None,
    }
}

fn canonical_discord_server_url(value: &str) -> Option<String> {
    let canonical = canonical_http_url(value)?;
    if canonical.contains(['?', '#']) {
        return None;
    }
    let invite = canonical
        .strip_prefix("https://discord.gg/")
        .or_else(|| canonical.strip_prefix("https://discord.com/invite/"))?;
    if !(2..=64).contains(&invite.len())
        || !invite
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    Some(format!("https://discord.com/invite/{invite}"))
}

fn canonical_discord_user(value: &str) -> Option<String> {
    if value.len() >= 17 && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(value.to_owned());
    }
    let handle = value.strip_prefix('@')?;
    if !(2..=32).contains(&handle.len())
        || handle.starts_with('.')
        || handle.ends_with('.')
        || handle.contains("..")
        || !handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
    {
        return None;
    }
    Some(format!("@{}", handle.to_ascii_lowercase()))
}

fn canonical_qq_group(value: &str) -> Option<String> {
    if (5..=12).contains(&value.len())
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        Some(value.to_owned())
    } else {
        None
    }
}

fn canonical_dns_name(value: &str) -> Option<String> {
    if !value.is_ascii() || value.len() > 253 || !value.contains('.') {
        return None;
    }
    if value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        Some(value.to_ascii_lowercase())
    } else {
        None
    }
}

fn is_github_name(value: &str, maximum: usize) -> bool {
    (1..=maximum).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validate_id(field: &'static str, value: &str) -> Result<(), PackageManifestErrorV1> {
    let valid = (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(PackageManifestErrorV1::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

fn validate_string_length(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), PackageManifestErrorV1> {
    if value.len() <= maximum {
        Ok(())
    } else {
        Err(PackageManifestErrorV1::StringTooLong {
            field,
            actual: value.len(),
            maximum,
        })
    }
}

fn validate_collection_length(
    field: &'static str,
    actual: usize,
    maximum: usize,
) -> Result<(), PackageManifestErrorV1> {
    if actual <= maximum {
        Ok(())
    } else {
        Err(PackageManifestErrorV1::CollectionTooLong {
            field,
            actual,
            maximum,
        })
    }
}

fn validate_unique_ids<'a>(
    field: &'static str,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), PackageManifestErrorV1> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(PackageManifestErrorV1::DuplicateIdentifier {
                field,
                value: value.to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), PackageManifestErrorV1> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(PackageManifestErrorV1::InvalidSha256 {
            field,
            value: value.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        ContactPurposeV1, MAX_CONTACT_PURPOSES, MAX_CONTACT_VALUE_BYTES, MAX_FEATURE_CAPABILITIES,
        MAX_FEATURE_DEPENDENCIES, MAX_MANIFEST_BYTES, MAX_TOOL_LICENSE_PATHS,
        PackageManifestErrorV1, PackageManifestV1, PublisherContactKindV1, SignatureV1,
        VerifiedPublisherIdentityV1,
    };

    const SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn valid_manifest() -> String {
        format!(
            r#"{{
                "manifest_version": 1,
                "package": {{ "id": "example.multi-content", "version": "1.2.3" }},
                "publisher": {{
                    "id": "example.publisher",
                    "display_name": "Example Publisher",
                    "contacts": [{{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }}]
                }},
                "sdk": {{
                    "bundle_id": "dev.20260802",
                    "target": "x86_64-pc-windows-msvc",
                    "abi_schema": 1,
                    "gpui": true,
                    "ui_abi_fingerprint": "{SHA256}"
                }},
                "rust": [{{ "id": "native", "entrypoint": "native/plugin.dll", "root_module": "example.root", "sdk_major": 1 }}],
                "lua": [{{ "id": "automation", "entrypoint": "lua/commands.lua" }}],
                "skins": [{{ "id": "appearance", "entrypoint": "skin/skin.json" }}],
                "locales": [{{ "locale": "en-US", "path": "locales/en-US.json", "sha256": "{SHA256}" }}],
                "tools": [{{
                    "id": "tokei", "target": "windows-x64", "path": "tools/windows-x64/tokei/tokei.exe",
                    "version": "12.1.0", "size": 42, "sha256": "{SHA256}", "output_protocol": "json",
                    "source": "https://example.invalid/tokei", "license_paths": ["tools/windows-x64/tokei/LICENSE.txt"]
                }}],
                "features": [{{ "id": "analysis", "capabilities": ["filesystem.read", "tools.execute_bundled"], "dependencies": [] }}],
                "dependencies": [{{ "package_id": "example.base", "version_requirement": "^1.0.0", "optional": false }}],
                "payloads": [{{ "path": "native/plugin.dll", "size": 42, "sha256": "{SHA256}", "kind": "rust_dll" }}],
                "signature": {{ "kind": "ed25519", "key_id": "example.signing", "signature": "base64-not-verified-here" }},
                "data_version": 7
            }}"#
        )
    }

    fn valid_manifest_value() -> Value {
        serde_json::from_str(&valid_manifest()).expect("valid test manifest JSON")
    }

    #[test]
    fn parses_a_strict_multi_content_v1_manifest() {
        let manifest = PackageManifestV1::parse_json(&valid_manifest()).expect("valid manifest");

        assert_eq!(manifest.package.id, "example.multi-content");
        assert_eq!(manifest.rust.len(), 1);
        assert_eq!(manifest.lua.len(), 1);
        assert_eq!(manifest.skins.len(), 1);
        assert_eq!(manifest.locales.len(), 1);
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(
            manifest.features[0].capabilities[1],
            "tools.execute_bundled"
        );
        assert_eq!(manifest.data_version, 7);
        assert!(matches!(manifest.signature, SignatureV1::Ed25519 { .. }));
    }

    #[test]
    fn rejects_unknown_fields_and_missing_security_relevant_fields() {
        let unknown = valid_manifest().replace(
            "\"data_version\": 7",
            "\"data_version\": 7, \"surprise\": true",
        );
        let missing_signature = valid_manifest().replace(
            "\"signature\": { \"kind\": \"ed25519\", \"key_id\": \"example.signing\", \"signature\": \"base64-not-verified-here\" },\n                ",
            "",
        );
        let unknown_signature_field = valid_manifest().replace(
            "\"signature\": { \"kind\": \"ed25519\", \"key_id\": \"example.signing\", \"signature\": \"base64-not-verified-here\" }",
            "\"signature\": { \"kind\": \"ed25519\", \"key_id\": \"example.signing\", \"signature\": \"base64-not-verified-here\", \"trusted\": true }",
        );

        assert!(matches!(
            PackageManifestV1::parse_json(&unknown),
            Err(PackageManifestErrorV1::Json(_))
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&missing_signature),
            Err(PackageManifestErrorV1::Json(_))
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&unknown_signature_field),
            Err(PackageManifestErrorV1::Json(_))
        ));
    }

    #[test]
    fn rejects_version_identifier_duplicate_and_hash_failures() {
        let version =
            valid_manifest().replace("\"manifest_version\": 1", "\"manifest_version\": 2");
        let invalid_id = valid_manifest().replace("\"id\": \"analysis\"", "\"id\": \"Analysis\"");
        let duplicate_feature = valid_manifest().replace(
            "\"features\": [{ \"id\": \"analysis\", \"capabilities\": [\"filesystem.read\", \"tools.execute_bundled\"], \"dependencies\": [] }]",
            "\"features\": [{ \"id\": \"analysis\", \"capabilities\": [], \"dependencies\": [] }, { \"id\": \"analysis\", \"capabilities\": [], \"dependencies\": [] }]",
        );
        let invalid_hash = valid_manifest().replace(SHA256, "ABC");

        assert!(matches!(
            PackageManifestV1::parse_json(&version),
            Err(PackageManifestErrorV1::UnsupportedManifestVersion { actual: 2 })
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&invalid_id),
            Err(PackageManifestErrorV1::InvalidIdentifier { .. })
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&duplicate_feature),
            Err(PackageManifestErrorV1::DuplicateIdentifier {
                field: "features",
                ..
            })
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&invalid_hash),
            Err(PackageManifestErrorV1::InvalidSha256 { .. })
        ));
    }

    #[test]
    fn rejects_oversized_input_before_json_deserialization() {
        let oversized = " ".repeat(MAX_MANIFEST_BYTES + 1);

        assert!(matches!(
            PackageManifestV1::parse_json(&oversized),
            Err(PackageManifestErrorV1::ManifestTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_bounded_strings_and_nested_collection_overflow() {
        let display_name = "x".repeat(257);
        let oversized_string = valid_manifest().replace("Example Publisher", &display_name);
        assert!(matches!(
            PackageManifestV1::parse_json(&oversized_string),
            Err(PackageManifestErrorV1::StringTooLong {
                field: "publisher.display_name",
                ..
            })
        ));

        let mut blank_display_name = valid_manifest_value();
        *blank_display_name
            .pointer_mut("/publisher/display_name")
            .expect("display name") = json!(" \t ");
        assert!(matches!(
            PackageManifestV1::parse_json(&blank_display_name.to_string()),
            Err(PackageManifestErrorV1::EmptyPublisherDisplayName)
        ));

        let contact_value = "x".repeat(MAX_CONTACT_VALUE_BYTES + 1);
        let oversized_contact = valid_manifest().replace("support@example.invalid", &contact_value);
        assert!(matches!(
            PackageManifestV1::parse_json(&oversized_contact),
            Err(PackageManifestErrorV1::StringTooLong {
                field: "publisher.contacts[].value",
                ..
            })
        ));

        let mut purposes = valid_manifest_value();
        let contact_purposes = purposes
            .pointer_mut("/publisher/contacts/0/purposes")
            .and_then(Value::as_array_mut)
            .expect("purposes array");
        contact_purposes.clear();
        contact_purposes.extend((0..=MAX_CONTACT_PURPOSES).map(|_| json!("support")));
        assert!(matches!(
            PackageManifestV1::parse_json(&purposes.to_string()),
            Err(PackageManifestErrorV1::CollectionTooLong {
                field: "publisher.contacts[].purposes",
                ..
            })
        ));

        let mut license_paths = valid_manifest_value();
        let tool_license_paths = license_paths
            .pointer_mut("/tools/0/license_paths")
            .and_then(Value::as_array_mut)
            .expect("license path array");
        tool_license_paths.clear();
        tool_license_paths.extend((0..=MAX_TOOL_LICENSE_PATHS).map(|_| json!("LICENSE.txt")));
        assert!(matches!(
            PackageManifestV1::parse_json(&license_paths.to_string()),
            Err(PackageManifestErrorV1::CollectionTooLong {
                field: "tools[].license_paths",
                ..
            })
        ));

        let mut capabilities = valid_manifest_value();
        let feature_capabilities = capabilities
            .pointer_mut("/features/0/capabilities")
            .and_then(Value::as_array_mut)
            .expect("capabilities array");
        feature_capabilities.clear();
        feature_capabilities.extend(
            (0..=MAX_FEATURE_CAPABILITIES).map(|index| json!(format!("capability.{index}"))),
        );
        assert!(matches!(
            PackageManifestV1::parse_json(&capabilities.to_string()),
            Err(PackageManifestErrorV1::CollectionTooLong {
                field: "features[].capabilities",
                ..
            })
        ));

        let mut dependencies = valid_manifest_value();
        let feature_dependencies = dependencies
            .pointer_mut("/features/0/dependencies")
            .and_then(Value::as_array_mut)
            .expect("feature dependencies array");
        feature_dependencies
            .extend((0..=MAX_FEATURE_DEPENDENCIES).map(|index| json!(format!("feature.{index}"))));
        assert!(matches!(
            PackageManifestV1::parse_json(&dependencies.to_string()),
            Err(PackageManifestErrorV1::CollectionTooLong {
                field: "features[].dependencies",
                ..
            })
        ));
    }

    #[test]
    fn requires_the_exact_gpui_fingerprint_contract() {
        let missing = valid_manifest().replace(
            &format!("\"ui_abi_fingerprint\": \"{SHA256}\""),
            "\"ui_abi_fingerprint\": null",
        );
        let invalid = valid_manifest().replace(
            &format!("\"ui_abi_fingerprint\": \"{SHA256}\""),
            "\"ui_abi_fingerprint\": \"ABC\"",
        );
        let unexpected = valid_manifest().replace("\"gpui\": true", "\"gpui\": false");

        assert!(matches!(
            PackageManifestV1::parse_json(&missing),
            Err(PackageManifestErrorV1::MissingGpuiFingerprint)
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&invalid),
            Err(PackageManifestErrorV1::InvalidSha256 {
                field: "sdk.ui_abi_fingerprint",
                ..
            })
        ));
        assert!(matches!(
            PackageManifestV1::parse_json(&unexpected),
            Err(PackageManifestErrorV1::UnexpectedGpuiFingerprint)
        ));
    }

    #[test]
    fn accepts_each_supported_public_contact_kind() {
        let mut value = valid_manifest_value();
        let contacts = value
            .pointer_mut("/publisher/contacts")
            .and_then(Value::as_array_mut)
            .expect("contacts array");
        contacts.clear();
        contacts.extend([
            json!({ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }),
            json!({ "kind": "website", "value": "https://example.invalid", "purposes": ["community"] }),
            json!({ "kind": "support_forum", "value": "https://forum.example.invalid", "purposes": ["support"] }),
            json!({ "kind": "github_issues", "value": "https://github.com/example/repository/issues", "purposes": ["support"] }),
            json!({ "kind": "discord_server", "value": "https://discord.gg/example", "purposes": ["community"] }),
            json!({ "kind": "discord_user", "value": "@example", "purposes": ["security"] }),
            json!({ "kind": "qq_group", "value": "123456", "purposes": ["community"] }),
            json!({ "kind": "other", "value": "https://contact.example.invalid", "purposes": ["security"] }),
        ]);

        assert!(PackageManifestV1::parse_json(&value.to_string()).is_ok());
    }

    #[test]
    fn rejects_missing_unsupported_and_invalid_publisher_contacts() {
        let mut missing = valid_manifest_value();
        missing
            .pointer_mut("/publisher/contacts")
            .and_then(Value::as_array_mut)
            .expect("contacts array")
            .clear();
        assert!(matches!(
            PackageManifestV1::parse_json(&missing.to_string()),
            Err(PackageManifestErrorV1::MissingPublisherContact)
        ));

        let unsupported =
            valid_manifest().replace("\"kind\": \"email\"", "\"kind\": \"matrix_room\"");
        assert!(matches!(
            PackageManifestV1::parse_json(&unsupported),
            Err(PackageManifestErrorV1::Json(_))
        ));

        let mut blank_value = valid_manifest_value();
        *blank_value
            .pointer_mut("/publisher/contacts/0/value")
            .expect("contact value") = json!("   ");
        assert!(matches!(
            PackageManifestV1::parse_json(&blank_value.to_string()),
            Err(PackageManifestErrorV1::EmptyPublisherContactValue {
                kind: PublisherContactKindV1::Email,
            })
        ));

        let mut empty_purposes = valid_manifest_value();
        empty_purposes
            .pointer_mut("/publisher/contacts/0/purposes")
            .and_then(Value::as_array_mut)
            .expect("purposes array")
            .clear();
        assert!(matches!(
            PackageManifestV1::parse_json(&empty_purposes.to_string()),
            Err(PackageManifestErrorV1::MissingPublisherContactPurpose {
                kind: PublisherContactKindV1::Email,
                ..
            })
        ));

        let community_only = valid_manifest().replace("[\"support\"]", "[\"community\"]");
        assert!(matches!(
            PackageManifestV1::parse_json(&community_only),
            Err(PackageManifestErrorV1::MissingSupportOrSecurityContact)
        ));
    }

    #[test]
    fn rejects_duplicate_and_conflicting_publisher_contact_declarations() {
        let mut duplicate_purpose = valid_manifest_value();
        *duplicate_purpose
            .pointer_mut("/publisher/contacts/0/purposes")
            .expect("purposes") = json!(["support", "support"]);
        assert!(matches!(
            PackageManifestV1::parse_json(&duplicate_purpose.to_string()),
            Err(PackageManifestErrorV1::DuplicatePublisherContactPurpose {
                purpose: ContactPurposeV1::Support,
                ..
            })
        ));

        let mut duplicate_contact = valid_manifest_value();
        duplicate_contact
            .pointer_mut("/publisher/contacts")
            .and_then(Value::as_array_mut)
            .expect("contacts array")
            .push(json!({ "kind": "email", "value": "support@example.invalid", "purposes": ["security"] }));
        assert!(matches!(
            PackageManifestV1::parse_json(&duplicate_contact.to_string()),
            Err(PackageManifestErrorV1::DuplicatePublisherContact {
                kind: PublisherContactKindV1::Email,
                ..
            })
        ));

        let mut conflicting_contact = valid_manifest_value();
        *conflicting_contact
            .pointer_mut("/publisher/contacts/0")
            .expect("contact") = json!({ "kind": "website", "value": "https://example.invalid", "purposes": ["support"] });
        conflicting_contact
            .pointer_mut("/publisher/contacts")
            .and_then(Value::as_array_mut)
            .expect("contacts array")
            .push(json!({ "kind": "support_forum", "value": "https://example.invalid", "purposes": ["security"] }));
        assert!(matches!(
            PackageManifestV1::parse_json(&conflicting_contact.to_string()),
            Err(PackageManifestErrorV1::ConflictingPublisherContactKinds {
                first_kind: PublisherContactKindV1::Website,
                conflicting_kind: PublisherContactKindV1::SupportForum,
                ..
            })
        ));

        let mut canonical_duplicate = valid_manifest_value();
        canonical_duplicate
            .pointer_mut("/publisher/contacts")
            .and_then(Value::as_array_mut)
            .expect("contacts array")
            .push(json!({ "kind": "email", "value": "SUPPORT@EXAMPLE.INVALID", "purposes": ["security"] }));
        assert!(matches!(
            PackageManifestV1::parse_json(&canonical_duplicate.to_string()),
            Err(PackageManifestErrorV1::DuplicatePublisherContact {
                kind: PublisherContactKindV1::Email,
                ..
            })
        ));

        let mut canonical_conflict = valid_manifest_value();
        *canonical_conflict
            .pointer_mut("/publisher/contacts/0")
            .expect("contact") = json!({ "kind": "website", "value": "HTTPS://Example.INVALID", "purposes": ["support"] });
        canonical_conflict
            .pointer_mut("/publisher/contacts")
            .and_then(Value::as_array_mut)
            .expect("contacts array")
            .push(json!({ "kind": "support_forum", "value": "https://example.invalid/", "purposes": ["security"] }));
        assert!(matches!(
            PackageManifestV1::parse_json(&canonical_conflict.to_string()),
            Err(PackageManifestErrorV1::ConflictingPublisherContactKinds {
                first_kind: PublisherContactKindV1::Website,
                conflicting_kind: PublisherContactKindV1::SupportForum,
                ..
            })
        ));
    }

    #[test]
    fn rejects_malformed_values_for_every_contact_kind() {
        let malformed = [
            ("email", "not-an-email"),
            ("website", "ftp://example.invalid"),
            ("support_forum", "mailto:support@example.invalid"),
            (
                "github_issues",
                "https://github.com/example/repository/pulls",
            ),
            ("discord_server", "https://discord.com/channels/example"),
            ("discord_user", "@a"),
            ("qq_group", "1234"),
            ("other", "other contact"),
        ];

        for (kind, contact_value) in malformed {
            let mut manifest = valid_manifest_value();
            *manifest
                .pointer_mut("/publisher/contacts/0")
                .expect("contact") = json!({
                "kind": kind,
                "value": contact_value,
                "purposes": ["support"],
            });
            assert!(matches!(
                PackageManifestV1::parse_json(&manifest.to_string()),
                Err(PackageManifestErrorV1::InvalidPublisherContactValue { .. })
            ));
        }
    }

    #[test]
    fn compares_only_an_opaque_verified_signer_publisher_identity() {
        let manifest = PackageManifestV1::parse_json(&valid_manifest()).expect("valid manifest");
        let matching = VerifiedPublisherIdentityV1::new("example.publisher".to_owned())
            .expect("normalized verified publisher identity");
        let mismatched = VerifiedPublisherIdentityV1::new("example.signing".to_owned())
            .expect("normalized verified publisher identity");

        assert!(
            manifest
                .validate_verified_signer_publisher_identity(&matching)
                .is_ok()
        );
        assert!(matches!(
            manifest.validate_verified_signer_publisher_identity(&mismatched),
            Err(PackageManifestErrorV1::SignedPublisherMismatch {
                manifest_publisher_id,
                verified_signer_publisher_id,
            }) if manifest_publisher_id == "example.publisher"
                && verified_signer_publisher_id == "example.signing"
        ));

        let unsigned = PackageManifestV1::parse_json(
            &valid_manifest().replace(
                "{ \"kind\": \"ed25519\", \"key_id\": \"example.signing\", \"signature\": \"base64-not-verified-here\" }",
                "{ \"kind\": \"unsigned\" }",
            ),
        )
        .expect("structurally valid unsigned manifest");
        assert!(matches!(
            unsigned.validate_verified_signer_publisher_identity(&matching),
            Err(PackageManifestErrorV1::VerifiedSignerIdentityRequiresSignature)
        ));
    }
}
