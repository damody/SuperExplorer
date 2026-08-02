//! Stateless, atomic validation of registrar contributions against resolved packages.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::ResolvedPackageV1;

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
}

/// Immutable canonical successful validation output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedContributionSetV1 {
    package_id: String,
    package_version: String,
    sealed_manifest_digest: String,
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

    #[must_use]
    pub fn contributions(&self) -> &[ContributionRegistrationV1] {
        &self.contributions
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
        Ok(ValidatedContributionSetV1 {
            package_id: resolved.manifest().package.id.clone(),
            package_version: resolved.manifest().package.version.clone(),
            sealed_manifest_digest: sealed_manifest_digest(resolved)?,
            contributions: canonical,
        })
    }
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
    let bytes = resolved
        .manifest()
        .canonical_serialized_bytes()
        .map_err(|_| ContributionGateErrorV1::SealedManifestDigestUnavailable)?;
    Ok(hex_digest(&Sha256::digest(bytes)))
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(*byte >> 4)]));
        output.push(char::from(HEX[usize::from(*byte & 0x0f)]));
    }
    output
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
            },
            ContributionRegistrationV1 {
                feature_id: "columns".to_owned(),
                contribution_id: "y".repeat(65),
                kind: ContributionKindV1::Column,
                required_capabilities: Vec::new(),
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
}
