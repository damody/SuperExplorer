//! Stateless, atomic validation of registrar contributions against resolved packages.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::{ResolvedPackageV1, package_validation::sealed_manifest_canonical_digest};
use abi_stable::std_types::ROption;
use explorer_extension_api::{StableIdV1, StableSortValueKindV1};

pub const MAX_CONTRIBUTIONS_PER_BATCH_V1: usize = 1_024;
pub const MAX_CAPABILITIES_PER_CONTRIBUTION_V1: usize = 64;

/// Declarative contribution kind. Only GPUI renderer has a V1 host-mandated capability.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContributionKindV1 {
    Column,
    GpuiRenderer,
    Command,
    Form,
    OperationPlan,
    ViewMode,
    Resource,
}

/// Pending contribution bound to a manifest feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionRegistrationV1 {
    pub feature_id: String,
    /// Package-scoped ID, unique across all contribution kinds.
    pub contribution_id: String,
    pub kind: ContributionKindV1,
    pub required_capabilities: Vec<String>,
    /// Inclusive Host-enforced limits for directory jobs. Missing means the
    /// contribution retains its existing unlimited behavior.
    pub folder_admission: Option<explorer_extension_api::FolderAdmissionPolicyV1>,
    /// Optional sealed job contract. A contribution without one cannot mint a
    /// job authority.
    pub job_contract: Option<ContributionJobContractV1>,
}

/// Declarative job contract, validated as part of the complete contribution
/// batch before it can become host authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionJobContractV1 {
    pub interface_id: StableIdV1,
    pub expected_sort: ROption<StableSortValueKindV1>,
    /// Opaque data is opt-in. The schema, version, and renderer binding are
    /// either all present for a source, or all absent.
    pub opaque_schema: Option<(StableIdV1, u32)>,
    pub renderer_contribution_id: Option<String>,
}

/// Immutable canonical successful validation output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedContributionSetV1 {
    package_id: String,
    package_version: String,
    sealed_manifest_digest: String,
    data_version: u64,
    contributions: Vec<ContributionRegistrationV1>,
}

impl ValidatedContributionSetV1 {
    /// Returns the resolved package ID which authorized this set.
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the resolved package version which authorized this set.
    #[must_use]
    pub fn package_version(&self) -> &str {
        &self.package_version
    }

    /// Returns the SHA-256 of canonical bytes from the sealed resolved manifest.
    #[must_use]
    pub fn sealed_manifest_digest(&self) -> &str {
        &self.sealed_manifest_digest
    }

    /// Returns the plugin data generation from the sealed manifest that
    /// authorized this contribution set.
    #[must_use]
    pub const fn data_version(&self) -> u64 {
        self.data_version
    }

    #[must_use]
    pub fn contributions(&self) -> &[ContributionRegistrationV1] {
        &self.contributions
    }

    /// Returns the only job contract admitted for this sealed contribution.
    /// Callers cannot supply interface/schema/sort identity themselves.
    #[must_use]
    pub(crate) fn job_descriptor(&self, contribution_id: &str) -> Option<ValidatedJobDescriptorV1> {
        let contribution = self
            .contributions
            .iter()
            .find(|entry| entry.contribution_id == contribution_id)?;
        let contract = contribution.job_contract.as_ref()?;
        let (opaque_schema, opaque_schema_version) = contract
            .opaque_schema
            .map_or((None, None), |(schema, version)| {
                (Some(schema), Some(version))
            });
        Some(ValidatedJobDescriptorV1 {
            contribution_id: contribution.contribution_id.clone(),
            feature_id: contribution.feature_id.clone(),
            kind: contribution.kind,
            interface_id: contract.interface_id,
            expected_sort: contract.expected_sort,
            opaque_schema,
            opaque_schema_version,
            renderer_contribution_id: contract.renderer_contribution_id.clone(),
            filesystem_read_authorized: contribution
                .required_capabilities
                .iter()
                .any(|capability| capability == "filesystem.read"),
            lock_owner_query_authorized: contribution
                .required_capabilities
                .iter()
                .any(|capability| capability == "lock_owner.query"),
            folder_admission: contribution.folder_admission,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedJobDescriptorV1 {
    pub(crate) contribution_id: String,
    pub(crate) feature_id: String,
    pub(crate) kind: ContributionKindV1,
    pub(crate) interface_id: StableIdV1,
    pub(crate) expected_sort: ROption<StableSortValueKindV1>,
    pub(crate) opaque_schema: Option<StableIdV1>,
    pub(crate) opaque_schema_version: Option<u32>,
    pub(crate) renderer_contribution_id: Option<String>,
    /// Fixed host-attested bit derived from the canonical validated
    /// contribution capability set. Stream open never accepts a plugin string.
    pub(crate) filesystem_read_authorized: bool,
    pub(crate) lock_owner_query_authorized: bool,
    pub(crate) folder_admission: Option<explorer_extension_api::FolderAdmissionPolicyV1>,
}

#[cfg(all(test, feature = "integration-test-support"))]
#[doc(hidden)]
#[allow(dead_code, clippy::wildcard_imports)]
pub mod integration_test_support {
    use super::*;
    use crate::{PackageManifestV1, PackageResolverV1, PackageValidationResultV1};
    use serde_json::json;

    /// Builds a sealed source/renderer fixture for cross-module lifecycle tests.
    /// This test-only helper mirrors the production gate's canonical output.
    #[must_use]
    pub fn validated_job_fixture(package_id: &str) -> ValidatedContributionSetV1 {
        let manifest = PackageManifestV1::parse_json(
            &json!({
                "manifest_version": 1,
                "package": { "id": package_id, "version": "1.0.0" },
                "publisher": { "id": "example.publisher", "display_name": "Example Publisher", "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }] },
                "sdk": { "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc", "abi_schema": 1, "gpui": true, "ui_abi_fingerprint": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" },
                "rust": [], "lua": [], "skins": [], "locales": [], "tools": [],
                "features": [{ "id": "feature", "capabilities": ["gpui.render"], "dependencies": [] }],
                "dependencies": [], "payloads": [], "signature": { "kind": "unsigned" }, "data_version": 1
            })
            .to_string(),
        )
        .expect("integration fixture manifest is valid");
        let candidates = [PackageValidationResultV1::for_resolver_test(manifest)];
        let resolution = PackageResolverV1::resolve(&candidates);
        ContributionGateV1::validate(
            &resolution.resolved_packages()[0],
            &[
                ContributionRegistrationV1 {
                    feature_id: "feature".to_owned(),
                    contribution_id: "column".to_owned(),
                    kind: ContributionKindV1::Column,
                    required_capabilities: Vec::new(),
                    folder_admission: None,
                    job_contract: Some(ContributionJobContractV1 {
                        interface_id: StableIdV1::new(
                            explorer_extension_api::IdNamespaceV1::new(7, 1),
                            1,
                        ),
                        expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                        opaque_schema: Some((
                            StableIdV1::new(explorer_extension_api::IdNamespaceV1::new(7, 2), 1),
                            1,
                        )),
                        renderer_contribution_id: Some("renderer".to_owned()),
                    }),
                },
                ContributionRegistrationV1 {
                    feature_id: "feature".to_owned(),
                    contribution_id: "renderer".to_owned(),
                    kind: ContributionKindV1::GpuiRenderer,
                    required_capabilities: vec!["gpui.render".to_owned()],
                    folder_admission: None,
                    job_contract: Some(ContributionJobContractV1 {
                        interface_id: StableIdV1::new(
                            explorer_extension_api::IdNamespaceV1::new(7, 1),
                            2,
                        ),
                        expected_sort: ROption::RNone,
                        opaque_schema: Some((
                            StableIdV1::new(explorer_extension_api::IdNamespaceV1::new(7, 2), 1),
                            1,
                        )),
                        renderer_contribution_id: None,
                    }),
                },
            ],
        )
        .expect("integration fixture job contracts are valid")
    }
}

/// Stateless authority for one complete resolved-package contribution batch.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContributionGateV1;

impl ContributionGateV1 {
    /// Validates a complete batch against the sealed manifest of a resolved package.
    ///
    /// No registry state is changed; a later registry transaction owns global ID
    /// collisions and generation-scoped commit.
    ///
    /// # Errors
    ///
    /// Returns deterministic package rejection with no partial validated output.
    pub fn validate(
        resolved: &ResolvedPackageV1<'_>,
        registrations: &[ContributionRegistrationV1],
    ) -> Result<ValidatedContributionSetV1, ContributionGateErrorV1> {
        preflight_batch(registrations)?;
        let mut canonical = registrations.to_vec();
        for registration in &mut canonical {
            registration.required_capabilities.sort();
        }
        canonical.sort_by(|left, right| {
            (
                &left.contribution_id,
                left.kind,
                &left.feature_id,
                &left.required_capabilities,
            )
                .cmp(&(
                    &right.contribution_id,
                    right.kind,
                    &right.feature_id,
                    &right.required_capabilities,
                ))
        });

        let mut previous_id = None::<&str>;
        for registration in &canonical {
            if previous_id == Some(registration.contribution_id.as_str()) {
                return Err(ContributionGateErrorV1::DuplicateContribution {
                    contribution_id: registration.contribution_id.clone(),
                });
            }
            previous_id = Some(&registration.contribution_id);
            if registration.folder_admission.is_some()
                && (registration.kind != ContributionKindV1::Column
                    || registration.job_contract.is_none())
            {
                return Err(ContributionGateErrorV1::InvalidFolderAdmissionPolicy {
                    contribution_id: registration.contribution_id.clone(),
                });
            }
            let Some(feature) = resolved
                .manifest()
                .features
                .iter()
                .find(|feature| feature.id == registration.feature_id)
            else {
                return Err(ContributionGateErrorV1::UndeclaredFeature {
                    feature_id: registration.feature_id.clone(),
                });
            };
            if requires_gpui_sdk(registration.kind) && !resolved.manifest().sdk.gpui {
                return Err(ContributionGateErrorV1::ContributionRequiresGpuiSdk {
                    contribution_id: registration.contribution_id.clone(),
                });
            }
            let declared = feature
                .capabilities
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let mut required = BTreeSet::new();
            for capability in registration
                .required_capabilities
                .iter()
                .map(String::as_str)
            {
                if !required.insert(capability) {
                    return Err(ContributionGateErrorV1::DuplicateRequiredCapability {
                        contribution_id: registration.contribution_id.clone(),
                        capability: capability.to_owned(),
                    });
                }
                if !declared.contains(capability) {
                    return Err(ContributionGateErrorV1::CapabilityExceeded {
                        feature_id: registration.feature_id.clone(),
                        contribution_id: registration.contribution_id.clone(),
                        capability: capability.to_owned(),
                    });
                }
            }
            if let Some(capability) = mandatory_capability(registration.kind)
                && !declared.contains(capability)
            {
                return Err(ContributionGateErrorV1::GpuiNotDeclared {
                    feature_id: registration.feature_id.clone(),
                    contribution_id: registration.contribution_id.clone(),
                });
            }
        }
        for registration in &mut canonical {
            if let Some(capability) = mandatory_capability(registration.kind) {
                registration
                    .required_capabilities
                    .push(capability.to_owned());
            }
            registration.required_capabilities.sort();
            registration.required_capabilities.dedup();
        }
        validate_job_contracts(&canonical)?;
        Ok(ValidatedContributionSetV1 {
            package_id: resolved.manifest().package.id.clone(),
            package_version: resolved.manifest().package.version.clone(),
            sealed_manifest_digest: sealed_manifest_digest(resolved)?,
            data_version: resolved.validation_result().data_version,
            contributions: canonical,
        })
    }
}

fn validate_job_contracts(
    registrations: &[ContributionRegistrationV1],
) -> Result<(), ContributionGateErrorV1> {
    for source in registrations {
        let Some(contract) = source.job_contract.as_ref() else {
            continue;
        };
        if !contract.interface_id.is_valid()
            || matches!(contract.expected_sort, ROption::RSome(sort) if !sort.is_known())
        {
            return Err(ContributionGateErrorV1::InvalidJobContract {
                contribution_id: source.contribution_id.clone(),
            });
        }
        let has_schema = contract.opaque_schema.is_some();
        let has_renderer = contract.renderer_contribution_id.is_some();
        let source_binding_is_invalid = if source.kind == ContributionKindV1::GpuiRenderer {
            has_renderer
        } else {
            has_schema != has_renderer
        };
        if source_binding_is_invalid
            || contract
                .opaque_schema
                .is_some_and(|(schema, version)| !schema.is_valid() || version == 0)
        {
            return Err(ContributionGateErrorV1::InvalidJobContract {
                contribution_id: source.contribution_id.clone(),
            });
        }
        let Some((schema, version)) = contract.opaque_schema else {
            continue;
        };
        let Some(renderer_id) = contract.renderer_contribution_id.as_deref() else {
            // A renderer declares a schema but never binds to itself.
            if source.kind == ContributionKindV1::GpuiRenderer {
                continue;
            }
            return Err(ContributionGateErrorV1::InvalidJobContract {
                contribution_id: source.contribution_id.clone(),
            });
        };
        if renderer_id.len() > 64 || !identifier_is_valid(renderer_id) {
            return Err(ContributionGateErrorV1::InvalidJobContract {
                contribution_id: source.contribution_id.clone(),
            });
        }
        let Some(renderer) = registrations.iter().find(|entry| {
            entry.contribution_id == renderer_id
                && entry.kind == ContributionKindV1::GpuiRenderer
                && entry.feature_id == source.feature_id
        }) else {
            return Err(ContributionGateErrorV1::OpaqueRendererContract {
                contribution_id: source.contribution_id.clone(),
            });
        };
        let matches_schema = renderer
            .job_contract
            .as_ref()
            .and_then(|renderer_contract| renderer_contract.opaque_schema)
            .is_some_and(|renderer_schema| renderer_schema == (schema, version));
        if !matches_schema {
            return Err(ContributionGateErrorV1::OpaqueRendererContract {
                contribution_id: source.contribution_id.clone(),
            });
        }
    }
    Ok(())
}

fn mandatory_capability(kind: ContributionKindV1) -> Option<&'static str> {
    match kind {
        ContributionKindV1::GpuiRenderer | ContributionKindV1::ViewMode => Some("gpui.render"),
        ContributionKindV1::Column
        | ContributionKindV1::Command
        | ContributionKindV1::Form
        | ContributionKindV1::OperationPlan
        | ContributionKindV1::Resource => None,
    }
}

const fn requires_gpui_sdk(kind: ContributionKindV1) -> bool {
    matches!(
        kind,
        ContributionKindV1::GpuiRenderer | ContributionKindV1::ViewMode
    )
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ContributionGateErrorV1 {
    #[error("contribution references undeclared feature {feature_id}")]
    UndeclaredFeature { feature_id: String },
    #[error("duplicate contribution {contribution_id}")]
    DuplicateContribution { contribution_id: String },
    #[error(
        "contribution {contribution_id} exceeds capability {capability} for feature {feature_id}"
    )]
    CapabilityExceeded {
        feature_id: String,
        contribution_id: String,
        capability: String,
    },
    #[error("GPUI contribution {contribution_id} requires the feature to declare gpui.render")]
    GpuiNotDeclared {
        feature_id: String,
        contribution_id: String,
    },
    #[error("GPUI contribution {contribution_id} requires a GPUI SDK package")]
    ContributionRequiresGpuiSdk { contribution_id: String },
    #[error("could not serialize the sealed manifest for contribution authority")]
    SealedManifestDigestUnavailable,
    #[error("contribution {contribution_id} repeats required capability {capability}")]
    DuplicateRequiredCapability {
        contribution_id: String,
        capability: String,
    },
    #[error("contribution {contribution_id} has an invalid sealed job contract")]
    InvalidJobContract { contribution_id: String },
    #[error("contribution {contribution_id} cannot declare a folder admission policy")]
    InvalidFolderAdmissionPolicy { contribution_id: String },
    #[error("contribution {contribution_id} has no matching GPUI renderer contract")]
    OpaqueRendererContract { contribution_id: String },
    #[error("invalid {field} identifier: {value}")]
    InvalidIdentifier { field: &'static str, value: String },
    #[error("{field} identifier exceeds maximum {maximum} bytes")]
    IdentifierTooLong { field: &'static str, maximum: usize },
    #[error("{field} exceeds maximum {maximum}")]
    LimitExceeded { field: &'static str, maximum: usize },
}

fn sealed_manifest_digest(
    resolved: &ResolvedPackageV1<'_>,
) -> Result<String, ContributionGateErrorV1> {
    sealed_manifest_canonical_digest(resolved.manifest())
        .map_err(|_| ContributionGateErrorV1::SealedManifestDigestUnavailable)
}

fn preflight_batch(
    registrations: &[ContributionRegistrationV1],
) -> Result<(), ContributionGateErrorV1> {
    if registrations.len() > MAX_CONTRIBUTIONS_PER_BATCH_V1 {
        return Err(ContributionGateErrorV1::LimitExceeded {
            field: "registrations",
            maximum: MAX_CONTRIBUTIONS_PER_BATCH_V1,
        });
    }
    if registrations.iter().any(|registration| {
        registration.required_capabilities.len() > MAX_CAPABILITIES_PER_CONTRIBUTION_V1
    }) {
        return Err(ContributionGateErrorV1::LimitExceeded {
            field: "required_capabilities",
            maximum: MAX_CAPABILITIES_PER_CONTRIBUTION_V1,
        });
    }
    let mut long_identifier_field = None::<&'static str>;
    let mut grammar_violation = None::<(&'static str, &str)>;
    for registration in registrations {
        for (field, value) in std::iter::once(("feature_id", registration.feature_id.as_str()))
            .chain(std::iter::once((
                "contribution_id",
                registration.contribution_id.as_str(),
            )))
            .chain(
                registration
                    .required_capabilities
                    .iter()
                    .map(|capability| ("capability", capability.as_str())),
            )
        {
            if value.len() > 64 {
                if long_identifier_field.is_none_or(|best| field_rank(field) < field_rank(best)) {
                    long_identifier_field = Some(field);
                }
                continue;
            }
            if !identifier_is_valid(value)
                && grammar_violation.is_none_or(|(best_field, best_value)| {
                    (field, value) < (best_field, best_value)
                })
            {
                grammar_violation = Some((field, value));
            }
        }
    }
    if let Some(field) = long_identifier_field {
        return Err(ContributionGateErrorV1::IdentifierTooLong { field, maximum: 64 });
    }
    grammar_violation.map_or(Ok(()), |(field, value)| {
        Err(ContributionGateErrorV1::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    })
}

fn identifier_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn field_rank(field: &str) -> u8 {
    match field {
        "feature_id" => 0,
        "contribution_id" => 1,
        "capability" => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        PackageManifestV1, PackageResolverV1, PackageValidationResultV1, ResolvedPackageV1,
    };
    use explorer_extension_api::IdNamespaceV1;

    fn registration(
        feature_id: &str,
        contribution_id: &str,
        kind: ContributionKindV1,
        capabilities: &[&str],
    ) -> ContributionRegistrationV1 {
        ContributionRegistrationV1 {
            feature_id: feature_id.to_owned(),
            contribution_id: contribution_id.to_owned(),
            kind,
            required_capabilities: capabilities
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            folder_admission: None,
            job_contract: None,
        }
    }

    fn feature(id: &str, capabilities: &[&str]) -> Value {
        json!({ "id": id, "capabilities": capabilities, "dependencies": [] })
    }

    fn with_resolved<R>(features: &[Value], action: impl FnOnce(&ResolvedPackageV1<'_>) -> R) -> R {
        with_resolved_package_sdk("example.package", false, features, action)
    }

    fn with_resolved_gpui<R>(
        features: &[Value],
        action: impl FnOnce(&ResolvedPackageV1<'_>) -> R,
    ) -> R {
        with_resolved_package_sdk("example.package", true, features, action)
    }

    fn with_resolved_package<R>(
        package_id: &str,
        features: &[Value],
        action: impl FnOnce(&ResolvedPackageV1<'_>) -> R,
    ) -> R {
        with_resolved_package_sdk(package_id, false, features, action)
    }

    fn with_resolved_package_sdk<R>(
        package_id: &str,
        gpui: bool,
        features: &[Value],
        action: impl FnOnce(&ResolvedPackageV1<'_>) -> R,
    ) -> R {
        let manifest = PackageManifestV1::parse_json(&json!({
            "manifest_version": 1, "package": { "id": package_id, "version": "1.0.0" },
            "publisher": { "id": "example.publisher", "display_name": "Example Publisher", "contacts": [{ "kind": "email", "value": "support@example.invalid", "purposes": ["support"] }] },
            "sdk": { "bundle_id": "dev.20260802", "target": "x86_64-pc-windows-msvc", "abi_schema": 1, "gpui": gpui, "ui_abi_fingerprint": if gpui { Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef") } else { None } },
            "rust": [], "lua": [], "skins": [], "locales": [], "tools": [], "features": features, "dependencies": [], "payloads": [], "signature": { "kind": "unsigned" }, "data_version": 1
        }).to_string()).expect("valid test manifest");
        let candidate = PackageValidationResultV1::for_resolver_test(manifest);
        let candidates = [candidate];
        let resolution = PackageResolverV1::resolve(&candidates);
        action(&resolution.resolved_packages()[0])
    }

    #[test]
    fn filesystem_read_is_authorized_only_when_the_validated_contribution_requires_it() {
        let contract = || ContributionJobContractV1 {
            interface_id: StableIdV1::new(IdNamespaceV1::new(1, 1), 9),
            expected_sort: ROption::RNone,
            opaque_schema: None,
            renderer_contribution_id: None,
        };
        with_resolved(&[feature("decode", &["filesystem.read"])], |resolved| {
            let mut metadata_only =
                registration("decode", "metadata", ContributionKindV1::Column, &[]);
            metadata_only.job_contract = Some(contract());
            let accepted = ContributionGateV1::validate(resolved, &[metadata_only]).unwrap();
            assert!(
                !accepted
                    .job_descriptor("metadata")
                    .unwrap()
                    .filesystem_read_authorized
            );

            let mut decoder = registration(
                "decode",
                "decoder",
                ContributionKindV1::Column,
                &["filesystem.read"],
            );
            decoder.job_contract = Some(contract());
            let accepted = ContributionGateV1::validate(resolved, &[decoder]).unwrap();
            assert!(
                accepted
                    .job_descriptor("decoder")
                    .unwrap()
                    .filesystem_read_authorized
            );
        });
    }

    #[test]
    fn unknown_feature_and_requested_excess_reject_without_output() {
        with_resolved_gpui(&[feature("columns", &["column.read"])], |resolved| {
            assert!(matches!(
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "missing",
                        "size",
                        ContributionKindV1::Column,
                        &[]
                    )]
                ),
                Err(ContributionGateErrorV1::UndeclaredFeature { .. })
            ));
            assert!(matches!(
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "columns",
                        "size",
                        ContributionKindV1::Column,
                        &["column.write"]
                    )]
                ),
                Err(ContributionGateErrorV1::CapabilityExceeded { .. })
            ));
        });
    }

    #[test]
    fn gpui_renderer_requires_host_capability_even_when_omitted() {
        with_resolved_gpui(&[feature("columns", &["column.read"])], |resolved| {
            assert!(matches!(
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "columns",
                        "renderer",
                        ContributionKindV1::GpuiRenderer,
                        &[]
                    )]
                ),
                Err(ContributionGateErrorV1::GpuiNotDeclared { .. })
            ));
        });
    }

    #[test]
    fn gpui_renderer_can_explicitly_request_its_host_mandated_capability() {
        with_resolved_gpui(&[feature("columns", &["gpui.render"])], |resolved| {
            let accepted = ContributionGateV1::validate(
                resolved,
                &[registration(
                    "columns",
                    "renderer",
                    ContributionKindV1::GpuiRenderer,
                    &["gpui.render"],
                )],
            )
            .expect("idempotent mandatory capability");
            assert_eq!(accepted.contributions().len(), 1);
            assert_eq!(
                accepted.contributions()[0].required_capabilities,
                vec!["gpui.render"]
            );
        });
    }

    #[test]
    fn empty_gpui_request_materializes_the_mandatory_capability_and_view_modes_require_it() {
        with_resolved_gpui(&[feature("ui", &["gpui.render"])], |resolved| {
            let accepted = ContributionGateV1::validate(
                resolved,
                &[registration(
                    "ui",
                    "renderer",
                    ContributionKindV1::GpuiRenderer,
                    &[],
                )],
            )
            .expect("authorized renderer");
            assert_eq!(
                accepted.contributions()[0].required_capabilities,
                vec!["gpui.render"]
            );
            let denied = ContributionGateV1::validate(
                resolved,
                &[registration(
                    "ui",
                    "view",
                    ContributionKindV1::ViewMode,
                    &[],
                )],
            );
            assert!(denied.is_ok());
        });
        with_resolved_gpui(&[feature("ui", &[])], |resolved| {
            assert!(matches!(
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "ui",
                        "view",
                        ContributionKindV1::ViewMode,
                        &[]
                    )],
                ),
                Err(ContributionGateErrorV1::GpuiNotDeclared { .. })
            ));
        });
        with_resolved(&[feature("ui", &["gpui.render"])], |resolved| {
            assert!(matches!(
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "ui",
                        "renderer",
                        ContributionKindV1::GpuiRenderer,
                        &[]
                    )],
                ),
                Err(ContributionGateErrorV1::ContributionRequiresGpuiSdk { .. })
            ));
        });
    }

    #[test]
    fn identical_contribution_ids_remain_bound_to_their_resolved_packages() {
        let first = with_resolved_package(
            "example.one",
            &[feature("columns", &["column.read"])],
            |resolved| {
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "columns",
                        "size",
                        ContributionKindV1::Column,
                        &["column.read"],
                    )],
                )
                .expect("first")
            },
        );
        let second = with_resolved_package(
            "example.two",
            &[feature("columns", &["column.read"])],
            |resolved| {
                ContributionGateV1::validate(
                    resolved,
                    &[registration(
                        "columns",
                        "size",
                        ContributionKindV1::Column,
                        &["column.read"],
                    )],
                )
                .expect("second")
            },
        );
        assert_eq!(first.package_id(), "example.one");
        assert_eq!(second.package_id(), "example.two");
        assert_eq!(first.package_version(), "1.0.0");
        assert!(!first.sealed_manifest_digest().is_empty());
        assert!(!second.sealed_manifest_digest().is_empty());
        assert_ne!(
            first.sealed_manifest_digest(),
            second.sealed_manifest_digest()
        );
    }

    #[test]
    fn across_kind_duplicate_and_invalid_at_end_produce_no_set() {
        with_resolved(
            &[feature("columns", &["column.read", "gpui.render"])],
            |resolved| {
                assert!(matches!(
                    ContributionGateV1::validate(
                        resolved,
                        &[
                            registration(
                                "columns",
                                "size",
                                ContributionKindV1::Column,
                                &["column.read"]
                            ),
                            registration("columns", "size", ContributionKindV1::GpuiRenderer, &[]),
                        ]
                    ),
                    Err(ContributionGateErrorV1::DuplicateContribution { .. })
                ));
                assert!(matches!(
                    ContributionGateV1::validate(
                        resolved,
                        &[
                            registration(
                                "columns",
                                "a",
                                ContributionKindV1::Column,
                                &["column.read"]
                            ),
                            registration("columns", "z", ContributionKindV1::Column, &["BAD"]),
                        ]
                    ),
                    Err(ContributionGateErrorV1::InvalidIdentifier { .. })
                ));
            },
        );
    }

    #[test]
    fn permutations_have_canonical_output_and_error() {
        with_resolved_gpui(
            &[feature("columns", &["column.read", "gpui.render"])],
            |resolved| {
                let first = vec![
                    registration("columns", "z", ContributionKindV1::Column, &["column.read"]),
                    registration("columns", "a", ContributionKindV1::GpuiRenderer, &[]),
                ];
                let second = vec![first[1].clone(), first[0].clone()];
                let accepted_first = ContributionGateV1::validate(resolved, &first).expect("first");
                let accepted_second =
                    ContributionGateV1::validate(resolved, &second).expect("second");
                assert_eq!(accepted_first, accepted_second);
                assert_eq!(
                    accepted_first
                        .contributions()
                        .iter()
                        .map(|contribution| contribution.contribution_id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["a", "z"]
                );
                assert_eq!(
                    accepted_first.contributions()[0].required_capabilities,
                    vec!["gpui.render"]
                );
            },
        );
        with_resolved(
            &[feature("columns", &["column.read", "gpui.render"])],
            |resolved| {
                let error_one = ContributionGateV1::validate(
                    resolved,
                    &[
                        registration("missing", "z", ContributionKindV1::Column, &[]),
                        registration(
                            "columns",
                            "a",
                            ContributionKindV1::Column,
                            &["column.write"],
                        ),
                    ],
                );
                let error_two = ContributionGateV1::validate(
                    resolved,
                    &[
                        registration(
                            "columns",
                            "a",
                            ContributionKindV1::Column,
                            &["column.write"],
                        ),
                        registration("missing", "z", ContributionKindV1::Column, &[]),
                    ],
                );
                assert_eq!(error_one, error_two);
            },
        );
    }

    #[test]
    fn complete_batch_and_capability_bounds_are_rejected() {
        with_resolved(&[feature("columns", &["column.read"])], |resolved| {
            let too_many = (0..=MAX_CONTRIBUTIONS_PER_BATCH_V1)
                .map(|index| {
                    registration(
                        "columns",
                        &format!("c{index}"),
                        ContributionKindV1::Column,
                        &[],
                    )
                })
                .collect::<Vec<_>>();
            assert!(matches!(
                ContributionGateV1::validate(resolved, &too_many),
                Err(ContributionGateErrorV1::LimitExceeded {
                    field: "registrations",
                    ..
                })
            ));
            let capabilities = (0..=MAX_CAPABILITIES_PER_CONTRIBUTION_V1)
                .map(|index| format!("cap{index}"))
                .collect::<Vec<_>>();
            let over = ContributionRegistrationV1 {
                feature_id: "columns".to_owned(),
                contribution_id: "size".to_owned(),
                kind: ContributionKindV1::Column,
                required_capabilities: capabilities,
                folder_admission: None,
                job_contract: None,
            };
            assert!(matches!(
                ContributionGateV1::validate(resolved, &[over]),
                Err(ContributionGateErrorV1::LimitExceeded {
                    field: "required_capabilities",
                    ..
                })
            ));
        });
    }

    #[test]
    fn borrowed_preflight_rejects_huge_and_permuted_malformed_input_before_canonicalization() {
        let huge = "x".repeat(65);
        let oversized = ContributionRegistrationV1 {
            feature_id: huge,
            contribution_id: "size".to_owned(),
            kind: ContributionKindV1::Column,
            required_capabilities: Vec::new(),
            folder_admission: None,
            job_contract: None,
        };
        assert!(matches!(
            preflight_batch(&[oversized]),
            Err(ContributionGateErrorV1::IdentifierTooLong {
                field: "feature_id",
                maximum: 64
            })
        ));
        let first = vec![
            registration("columns", "z", ContributionKindV1::Column, &["BAD"]),
            registration("_bad", "a", ContributionKindV1::Column, &[]),
        ];
        let second = vec![first[1].clone(), first[0].clone()];
        assert_eq!(preflight_batch(&first), preflight_batch(&second));
        let different_long_fields = vec![
            ContributionRegistrationV1 {
                feature_id: "x".repeat(65),
                contribution_id: "size".to_owned(),
                kind: ContributionKindV1::Column,
                required_capabilities: Vec::new(),
                folder_admission: None,
                job_contract: None,
            },
            ContributionRegistrationV1 {
                feature_id: "columns".to_owned(),
                contribution_id: "y".repeat(65),
                kind: ContributionKindV1::Column,
                required_capabilities: Vec::new(),
                folder_admission: None,
                job_contract: None,
            },
        ];
        let reversed_long_fields = different_long_fields
            .iter()
            .cloned()
            .rev()
            .collect::<Vec<_>>();
        assert_eq!(
            preflight_batch(&different_long_fields),
            preflight_batch(&reversed_long_fields)
        );
    }

    #[test]
    fn sealed_job_contract_admits_sortable_columns_and_bound_opaque_renderers() {
        with_resolved_gpui(&[feature("feature", &["gpui.render"])], |resolved| {
            let schema = StableIdV1::new(IdNamespaceV1::new(9, 2), 7);
            let mut column = registration("feature", "column", ContributionKindV1::Column, &[]);
            column.job_contract = Some(ContributionJobContractV1 {
                interface_id: StableIdV1::new(IdNamespaceV1::new(9, 1), 1),
                expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                opaque_schema: Some((schema, 3)),
                renderer_contribution_id: Some("renderer".to_owned()),
            });
            let mut renderer = registration(
                "feature",
                "renderer",
                ContributionKindV1::GpuiRenderer,
                &["gpui.render"],
            );
            renderer.job_contract = Some(ContributionJobContractV1 {
                interface_id: StableIdV1::new(IdNamespaceV1::new(9, 1), 2),
                expected_sort: ROption::RNone,
                opaque_schema: Some((schema, 3)),
                renderer_contribution_id: None,
            });
            let validated = ContributionGateV1::validate(resolved, &[renderer, column]).unwrap();
            let descriptor = validated.job_descriptor("column").unwrap();
            assert_eq!(
                descriptor.expected_sort,
                ROption::RSome(StableSortValueKindV1::U64)
            );
            assert_eq!(descriptor.opaque_schema, Some(schema));
            assert_eq!(descriptor.opaque_schema_version, Some(3));
            assert_eq!(
                descriptor.renderer_contribution_id.as_deref(),
                Some("renderer")
            );

            let mut missing_renderer =
                registration("feature", "broken", ContributionKindV1::Column, &[]);
            missing_renderer.job_contract = Some(ContributionJobContractV1 {
                interface_id: StableIdV1::new(IdNamespaceV1::new(9, 1), 3),
                expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                opaque_schema: Some((schema, 3)),
                renderer_contribution_id: Some("missing".to_owned()),
            });
            assert!(matches!(
                ContributionGateV1::validate(resolved, &[missing_renderer]),
                Err(ContributionGateErrorV1::OpaqueRendererContract { .. })
            ));
        });
    }

    #[test]
    fn folder_admission_accepts_zero_and_is_rejected_outside_data_columns() {
        with_resolved(&[feature("feature", &[])], |resolved| {
            let policy = explorer_extension_api::FolderAdmissionPolicyV1 {
                max_file_count: ROption::RSome(0),
                max_folder_count: ROption::RSome(u64::MAX),
            };
            let mut column = registration("feature", "column", ContributionKindV1::Column, &[]);
            column.folder_admission = Some(policy);
            column.job_contract = Some(ContributionJobContractV1 {
                interface_id: StableIdV1::new(IdNamespaceV1::new(10, 1), 1),
                expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                opaque_schema: None,
                renderer_contribution_id: None,
            });
            let validated = ContributionGateV1::validate(resolved, &[column]).unwrap();
            assert_eq!(
                validated.job_descriptor("column").unwrap().folder_admission,
                Some(policy)
            );

            let mut command = registration("feature", "command", ContributionKindV1::Command, &[]);
            command.folder_admission = Some(policy);
            assert!(matches!(
                ContributionGateV1::validate(resolved, &[command]),
                Err(ContributionGateErrorV1::InvalidFolderAdmissionPolicy { .. })
            ));
        });
    }
}
