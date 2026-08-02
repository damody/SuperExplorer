//! Desired extension-feature state and pure effective-state resolution.
//!
//! This module deliberately persists only user intent. Runtime observations are
//! supplied to the resolver by its caller and are never written to disk.

#![allow(clippy::missing_errors_doc, clippy::struct_field_names)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
#[link(name = "Kernel32")]
#[allow(unsafe_code)]
unsafe extern "system" {
    #[link_name = "ReplaceFileW"]
    fn replace_file_w(
        replaced_file_name: *const u16,
        replacement_file_name: *const u16,
        backup_file_name: *const u16,
        replace_flags: u32,
        exclude: *mut core::ffi::c_void,
        reserved: *mut core::ffi::c_void,
    ) -> i32;
}

/// The current on-disk desired-state schema.
pub const FEATURE_STATE_STORE_SCHEMA_VERSION_V1: u32 = 1;
/// Maximum accepted desired-state document size.
pub const MAX_FEATURE_STATE_STORE_BYTES_V1: usize = 4 * 1024 * 1024;
/// Maximum package entries retained, including packages not currently installed.
pub const MAX_PACKAGE_DESIRED_STATES_V1: usize = 1_024;
/// Maximum feature entries retained, including features not currently installed.
pub const MAX_FEATURE_DESIRED_STATES_V1: usize = 16_384;
/// Maximum facts accepted by a single pure resolution call.
pub const MAX_FEATURE_RESOLUTION_FACTS_V1: usize = MAX_FEATURE_DESIRED_STATES_V1;
/// Maximum dependencies accepted on one feature fact.
pub const MAX_FEATURE_DEPENDENCIES_V1: usize = 64;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A persisted user preference for a scope.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredStateV1 {
    /// The scope is requested to run when its other requirements are satisfied.
    #[default]
    Enabled,
    /// The scope is requested to remain disabled.
    Disabled,
}

/// Stable package-and-feature key used by desired state and resolution facts.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FeatureKeyV1 {
    /// Owning package identifier.
    pub package_id: String,
    /// Feature identifier within the package.
    pub feature_id: String,
}

impl FeatureKeyV1 {
    /// Creates and validates a feature key.
    pub fn new(
        package_id: impl Into<String>,
        feature_id: impl Into<String>,
    ) -> Result<Self, FeatureStateStoreErrorV1> {
        let key = Self {
            package_id: package_id.into(),
            feature_id: feature_id.into(),
        };
        key.validate()?;
        Ok(key)
    }

    fn validate(&self) -> Result<(), FeatureStateStoreErrorV1> {
        validate_identifier("package_id", &self.package_id)?;
        validate_identifier("feature_id", &self.feature_id)
    }
}

/// Desired state only. Effective runtime state is deliberately not persisted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureStateStoreV1 {
    global_desired: DesiredStateV1,
    package_desired: BTreeMap<String, DesiredStateV1>,
    feature_desired: BTreeMap<FeatureKeyV1, DesiredStateV1>,
}

impl Default for FeatureStateStoreV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureStateStoreV1 {
    /// Creates an enabled store with no package or feature overrides.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            global_desired: DesiredStateV1::Enabled,
            package_desired: BTreeMap::new(),
            feature_desired: BTreeMap::new(),
        }
    }

    /// Returns the persisted global desired state.
    #[must_use]
    pub const fn global_desired(&self) -> DesiredStateV1 {
        self.global_desired
    }

    /// Returns the package override or the enabled default when no override exists.
    #[must_use]
    pub fn package_desired(&self, package_id: &str) -> DesiredStateV1 {
        self.package_desired
            .get(package_id)
            .copied()
            .unwrap_or(DesiredStateV1::Enabled)
    }

    /// Returns the feature override or the enabled default when no override exists.
    #[must_use]
    pub fn feature_desired(&self, feature: &FeatureKeyV1) -> DesiredStateV1 {
        self.feature_desired
            .get(feature)
            .copied()
            .unwrap_or(DesiredStateV1::Enabled)
    }

    /// Updates the global desired state.
    pub fn set_global_desired(&mut self, desired: DesiredStateV1) {
        self.global_desired = desired;
    }

    /// Saves an explicit package override, retaining unknown package IDs for a later reinstall.
    pub fn set_package_desired(
        &mut self,
        package_id: impl Into<String>,
        desired: DesiredStateV1,
    ) -> Result<(), FeatureStateStoreErrorV1> {
        let package_id = package_id.into();
        validate_identifier("package_id", &package_id)?;
        if !self.package_desired.contains_key(&package_id)
            && self.package_desired.len() == MAX_PACKAGE_DESIRED_STATES_V1
        {
            return Err(FeatureStateStoreErrorV1::LimitExceeded {
                field: "package_desired",
                maximum: MAX_PACKAGE_DESIRED_STATES_V1,
            });
        }
        self.package_desired.insert(package_id, desired);
        Ok(())
    }

    /// Saves an explicit feature override, retaining unknown IDs for a later reinstall.
    pub fn set_feature_desired(
        &mut self,
        feature: FeatureKeyV1,
        desired: DesiredStateV1,
    ) -> Result<(), FeatureStateStoreErrorV1> {
        feature.validate()?;
        if !self.feature_desired.contains_key(&feature)
            && self.feature_desired.len() == MAX_FEATURE_DESIRED_STATES_V1
        {
            return Err(FeatureStateStoreErrorV1::LimitExceeded {
                field: "feature_desired",
                maximum: MAX_FEATURE_DESIRED_STATES_V1,
            });
        }
        self.feature_desired.insert(feature, desired);
        Ok(())
    }

    /// Encodes only desired state into a canonical, versioned JSON document.
    pub fn encode_json(&self) -> Result<Vec<u8>, FeatureStateStoreErrorV1> {
        let document = PersistedFeatureStateStoreV1 {
            schema_version: FEATURE_STATE_STORE_SCHEMA_VERSION_V1,
            global_desired: self.global_desired,
            package_desired: self
                .package_desired
                .iter()
                .map(|(package_id, desired)| PersistedPackageDesiredV1 {
                    package_id: package_id.clone(),
                    desired: *desired,
                })
                .collect(),
            feature_desired: self
                .feature_desired
                .iter()
                .map(|(feature, desired)| PersistedFeatureDesiredV1 {
                    package_id: feature.package_id.clone(),
                    feature_id: feature.feature_id.clone(),
                    desired: *desired,
                })
                .collect(),
        };
        let encoded = serde_json::to_vec(&document).map_err(FeatureStateStoreErrorV1::Encode)?;
        if encoded.len() > MAX_FEATURE_STATE_STORE_BYTES_V1 {
            return Err(FeatureStateStoreErrorV1::DocumentTooLarge {
                maximum: MAX_FEATURE_STATE_STORE_BYTES_V1,
            });
        }
        Ok(encoded)
    }

    /// Decodes and validates a versioned desired-state document without resetting on failure.
    pub fn decode_json(bytes: &[u8]) -> Result<Self, FeatureStateStoreErrorV1> {
        if bytes.len() > MAX_FEATURE_STATE_STORE_BYTES_V1 {
            return Err(FeatureStateStoreErrorV1::DocumentTooLarge {
                maximum: MAX_FEATURE_STATE_STORE_BYTES_V1,
            });
        }
        let document: PersistedFeatureStateStoreV1 =
            serde_json::from_slice(bytes).map_err(FeatureStateStoreErrorV1::Corrupt)?;
        if document.schema_version != FEATURE_STATE_STORE_SCHEMA_VERSION_V1 {
            return Err(FeatureStateStoreErrorV1::UnsupportedSchema {
                found: document.schema_version,
            });
        }
        if document.package_desired.len() > MAX_PACKAGE_DESIRED_STATES_V1 {
            return Err(FeatureStateStoreErrorV1::LimitExceeded {
                field: "package_desired",
                maximum: MAX_PACKAGE_DESIRED_STATES_V1,
            });
        }
        if document.feature_desired.len() > MAX_FEATURE_DESIRED_STATES_V1 {
            return Err(FeatureStateStoreErrorV1::LimitExceeded {
                field: "feature_desired",
                maximum: MAX_FEATURE_DESIRED_STATES_V1,
            });
        }
        let mut package_desired = BTreeMap::new();
        for saved in document.package_desired {
            validate_identifier("package_id", &saved.package_id)?;
            if package_desired
                .insert(saved.package_id.clone(), saved.desired)
                .is_some()
            {
                return Err(FeatureStateStoreErrorV1::DuplicatePackageId {
                    package_id: saved.package_id,
                });
            }
        }

        let mut feature_desired = BTreeMap::new();
        for saved in document.feature_desired {
            let feature = FeatureKeyV1::new(saved.package_id, saved.feature_id)?;
            if feature_desired
                .insert(feature.clone(), saved.desired)
                .is_some()
            {
                return Err(FeatureStateStoreErrorV1::DuplicateFeatureKey { feature });
            }
        }
        Ok(Self {
            global_desired: document.global_desired,
            package_desired,
            feature_desired,
        })
    }

    /// Loads a bounded document; corrupt or unsupported input is an error and never defaults.
    pub fn load(path: &Path) -> Result<Self, FeatureStateStoreErrorV1> {
        let bytes = read_bounded(path)?;
        Self::decode_json(&bytes)
    }

    /// Atomically replaces the persisted document after a complete same-directory temporary write.
    ///
    /// A write or replace failure leaves an existing target untouched. Callers keep their current
    /// in-memory store on error rather than treating it as a successful reset.
    pub fn save_atomic(&self, path: &Path) -> Result<(), FeatureStateStoreErrorV1> {
        self.save_atomic_with(path, replace_file_atomically)
    }

    fn save_atomic_with<F>(&self, path: &Path, replace: F) -> Result<(), FeatureStateStoreErrorV1>
    where
        F: FnOnce(&Path, &Path) -> io::Result<()>,
    {
        let encoded = self.encode_json()?;
        let temporary = temporary_path(path)?;
        let write_result = (|| -> Result<(), FeatureStateStoreErrorV1> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|source| FeatureStateStoreErrorV1::Io {
                    path: temporary.clone(),
                    source,
                })?;
            file.write_all(&encoded)
                .and_then(|()| file.sync_all())
                .map_err(|source| FeatureStateStoreErrorV1::Io {
                    path: temporary.clone(),
                    source,
                })?;
            replace(&temporary, path).map_err(|source| FeatureStateStoreErrorV1::Io {
                path: path.to_path_buf(),
                source,
            })
        })();
        if write_result.is_err() {
            let _ignored = fs::remove_file(&temporary);
        }
        write_result
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedFeatureStateStoreV1 {
    schema_version: u32,
    global_desired: DesiredStateV1,
    package_desired: Vec<PersistedPackageDesiredV1>,
    feature_desired: Vec<PersistedFeatureDesiredV1>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedFeatureDesiredV1 {
    package_id: String,
    feature_id: String,
    desired: DesiredStateV1,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedPackageDesiredV1 {
    package_id: String,
    desired: DesiredStateV1,
}

/// Explicit runtime fact; these states are never inferred from desired state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FeatureRuntimeFactV1 {
    /// No runtime exception applies.
    #[default]
    Ready,
    /// A drain coordinated elsewhere is in progress.
    Disabling,
    /// A restart coordinated elsewhere is required.
    PendingRestart,
    /// The feature has faulted.
    Faulted,
}

/// Compatibility fact supplied by package/host validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FeatureCompatibilityFactV1 {
    /// The feature is compatible with the current host.
    #[default]
    Compatible,
    /// Compatibility validation rejected the feature.
    Incompatible(FeatureCompatibilityIssueV1),
}

/// Typed compatibility failure categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeatureCompatibilityIssueV1 {
    /// The host or SDK version is unsupported.
    HostVersion,
    /// The target platform is unsupported.
    Target,
    /// Required capabilities are unavailable.
    Capability,
}

/// Typed diagnostics supplied by validation or runtime supervision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeatureDiagnosticFactV1 {
    /// Manifest or package validation failed.
    PackageValidation,
    /// A required bundled tool is unavailable.
    RequiredToolUnavailable,
    /// Host policy prohibits use.
    HostPolicy,
}

/// All pure facts associated with one known feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureResolutionFactV1 {
    /// Feature to resolve.
    pub feature: FeatureKeyV1,
    /// Other known features required by this feature.
    pub dependencies: Vec<FeatureKeyV1>,
    /// Compatibility observation.
    pub compatibility: FeatureCompatibilityFactV1,
    /// Optional diagnostic which takes precedence over compatibility and dependencies.
    pub diagnostic: Option<FeatureDiagnosticFactV1>,
    /// Explicit runtime observation.
    pub runtime: FeatureRuntimeFactV1,
}

/// The current effective state exposed to callers and the options UI.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EffectiveFeatureStateV1 {
    /// The feature may run.
    #[default]
    Enabled,
    /// Desired state disables the feature or one of its parents.
    Disabled,
    /// A runtime drain is in progress.
    Disabling,
    /// A restart is required before the requested state can take effect.
    PendingRestart,
    /// A dependency, compatibility, or diagnostic requirement prevents use.
    Blocked,
    /// A runtime failure prevents use.
    Faulted,
}

/// Typed explanation for an effective state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectiveFeatureReasonV1 {
    /// Global desired state disabled the feature.
    GlobalDesiredDisabled,
    /// Package desired state disabled the feature.
    PackageDesiredDisabled,
    /// Feature desired state disabled the feature.
    FeatureDesiredDisabled,
    /// Explicit runtime observation.
    RuntimeDisabling,
    /// Explicit runtime observation.
    RuntimePendingRestart,
    /// Explicit runtime observation.
    RuntimeFaulted,
    /// A higher-precedence diagnostic blocks the feature.
    Diagnostic(FeatureDiagnosticFactV1),
    /// Compatibility blocks the feature.
    Compatibility(FeatureCompatibilityIssueV1),
    /// A declared dependency is not among the facts.
    MissingDependency { dependency: FeatureKeyV1 },
    /// A dependency is present but not effectively enabled.
    DependencyUnavailable {
        /// The unavailable dependency.
        dependency: FeatureKeyV1,
        /// Its resulting state.
        state: EffectiveFeatureStateV1,
    },
    /// A deterministic sorted strongly-connected dependency component blocks the feature.
    DependencyCycle { members: Vec<FeatureKeyV1> },
}

/// One deterministic resolver result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveFeatureV1 {
    /// Feature key.
    pub feature: FeatureKeyV1,
    /// Effective state; derived, not persisted.
    pub state: EffectiveFeatureStateV1,
    /// Explanation for the state.
    pub reason: Option<EffectiveFeatureReasonV1>,
}

/// Pure desired-plus-facts resolver. It does not load, drain, or call extensions.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectiveFeatureResolverV1;

impl EffectiveFeatureResolverV1 {
    /// Resolves facts in sorted feature-key order.
    ///
    /// Reason precedence is explicit runtime, desired hierarchy, diagnostic,
    /// compatibility, then dependency. Child desired overrides are never mutated
    /// when a parent is disabled.
    pub fn resolve(
        store: &FeatureStateStoreV1,
        facts: &[FeatureResolutionFactV1],
    ) -> Result<Vec<EffectiveFeatureV1>, EffectiveFeatureResolverErrorV1> {
        if facts.len() > MAX_FEATURE_RESOLUTION_FACTS_V1 {
            return Err(EffectiveFeatureResolverErrorV1::LimitExceeded {
                field: "facts",
                maximum: MAX_FEATURE_RESOLUTION_FACTS_V1,
            });
        }
        let mut indexed = BTreeMap::new();
        for fact in facts {
            validate_fact(fact)?;
            if indexed.insert(fact.feature.clone(), fact).is_some() {
                return Err(EffectiveFeatureResolverErrorV1::DuplicateFeature {
                    feature: fact.feature.clone(),
                });
            }
        }

        let mut results = BTreeMap::new();
        for (key, fact) in &indexed {
            results.insert(key.clone(), resolve_base(store, fact));
        }
        apply_cycles(&indexed, &mut results);
        apply_dependencies(&indexed, &mut results);
        Ok(results.into_values().collect())
    }

    /// Resolves only global desired state for callers that have not yet supplied feature facts.
    #[must_use]
    pub const fn resolve_global(store: &FeatureStateStoreV1) -> EffectiveFeatureStateV1 {
        match store.global_desired() {
            DesiredStateV1::Enabled => EffectiveFeatureStateV1::Enabled,
            DesiredStateV1::Disabled => EffectiveFeatureStateV1::Disabled,
        }
    }
}

/// Bounded input validation failure for the pure resolver.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EffectiveFeatureResolverErrorV1 {
    /// A fact set exceeded its fixed input bound.
    #[error("{field} exceeds maximum {maximum}")]
    LimitExceeded { field: &'static str, maximum: usize },
    /// The same feature appeared in more than one fact.
    #[error("duplicate feature fact for {feature:?}")]
    DuplicateFeature { feature: FeatureKeyV1 },
    /// A feature identifier was malformed.
    #[error("invalid {field} identifier: {value}")]
    InvalidIdentifier { field: &'static str, value: String },
    /// A fact repeated a dependency, making its declaration ambiguous.
    #[error("duplicate dependency {dependency:?} for {feature:?}")]
    DuplicateDependency {
        feature: FeatureKeyV1,
        dependency: FeatureKeyV1,
    },
}

/// Failure while validating or persisting desired state.
#[derive(Debug, Error)]
pub enum FeatureStateStoreErrorV1 {
    /// The data is not valid JSON for the exact V1 schema.
    #[error("corrupt feature-state document: {0}")]
    Corrupt(serde_json::Error),
    /// Serialization unexpectedly failed.
    #[error("could not encode feature-state document: {0}")]
    Encode(serde_json::Error),
    /// The document is larger than the fixed bound.
    #[error("feature-state document exceeds maximum {maximum} bytes")]
    DocumentTooLarge { maximum: usize },
    /// The on-disk schema is unknown and is not silently reset.
    #[error("unsupported feature-state schema {found}")]
    UnsupportedSchema { found: u32 },
    /// An input collection exceeds a fixed bound.
    #[error("{field} exceeds maximum {maximum}")]
    LimitExceeded { field: &'static str, maximum: usize },
    /// An identifier is malformed.
    #[error("invalid {field} identifier: {value}")]
    InvalidIdentifier { field: &'static str, value: String },
    /// Duplicate feature entries would make persisted desired state ambiguous.
    #[error("duplicate persisted feature key {feature:?}")]
    DuplicateFeatureKey { feature: FeatureKeyV1 },
    /// Duplicate package entries would make persisted desired state ambiguous.
    #[error("duplicate persisted package ID {package_id}")]
    DuplicatePackageId { package_id: String },
    /// A filesystem operation failed without replacing the previous good file.
    #[error("feature-state I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

fn resolve_base(store: &FeatureStateStoreV1, fact: &FeatureResolutionFactV1) -> EffectiveFeatureV1 {
    let (state, reason) = match fact.runtime {
        FeatureRuntimeFactV1::Disabling => (
            EffectiveFeatureStateV1::Disabling,
            Some(EffectiveFeatureReasonV1::RuntimeDisabling),
        ),
        FeatureRuntimeFactV1::PendingRestart => (
            EffectiveFeatureStateV1::PendingRestart,
            Some(EffectiveFeatureReasonV1::RuntimePendingRestart),
        ),
        FeatureRuntimeFactV1::Faulted => (
            EffectiveFeatureStateV1::Faulted,
            Some(EffectiveFeatureReasonV1::RuntimeFaulted),
        ),
        FeatureRuntimeFactV1::Ready => resolve_ready_base(store, fact),
    };
    EffectiveFeatureV1 {
        feature: fact.feature.clone(),
        state,
        reason,
    }
}

fn resolve_ready_base(
    store: &FeatureStateStoreV1,
    fact: &FeatureResolutionFactV1,
) -> (EffectiveFeatureStateV1, Option<EffectiveFeatureReasonV1>) {
    if store.global_desired() == DesiredStateV1::Disabled {
        (
            EffectiveFeatureStateV1::Disabled,
            Some(EffectiveFeatureReasonV1::GlobalDesiredDisabled),
        )
    } else if store.package_desired(&fact.feature.package_id) == DesiredStateV1::Disabled {
        (
            EffectiveFeatureStateV1::Disabled,
            Some(EffectiveFeatureReasonV1::PackageDesiredDisabled),
        )
    } else if store.feature_desired(&fact.feature) == DesiredStateV1::Disabled {
        (
            EffectiveFeatureStateV1::Disabled,
            Some(EffectiveFeatureReasonV1::FeatureDesiredDisabled),
        )
    } else if let Some(diagnostic) = fact.diagnostic {
        (
            EffectiveFeatureStateV1::Blocked,
            Some(EffectiveFeatureReasonV1::Diagnostic(diagnostic)),
        )
    } else {
        match fact.compatibility {
            FeatureCompatibilityFactV1::Compatible => (EffectiveFeatureStateV1::Enabled, None),
            FeatureCompatibilityFactV1::Incompatible(issue) => (
                EffectiveFeatureStateV1::Blocked,
                Some(EffectiveFeatureReasonV1::Compatibility(issue)),
            ),
        }
    }
}

fn apply_cycles(
    indexed: &BTreeMap<FeatureKeyV1, &FeatureResolutionFactV1>,
    results: &mut BTreeMap<FeatureKeyV1, EffectiveFeatureV1>,
) {
    let enabled = indexed
        .keys()
        .filter(|key| {
            results
                .get(*key)
                .is_some_and(|result| result.state == EffectiveFeatureStateV1::Enabled)
        })
        .cloned()
        .collect::<BTreeSet<_>>();

    let adjacency = enabled
        .iter()
        .map(|feature| {
            let dependencies = indexed.get(feature).map_or_else(Vec::new, |fact| {
                sorted_dependencies(fact)
                    .into_iter()
                    .filter(|dependency| enabled.contains(dependency))
                    .collect()
            });
            (feature.clone(), dependencies)
        })
        .collect::<BTreeMap<_, Vec<_>>>();
    let mut reverse = enabled
        .iter()
        .cloned()
        .map(|feature| (feature, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for (feature, dependencies) in &adjacency {
        for dependency in dependencies {
            if let Some(dependents) = reverse.get_mut(dependency) {
                dependents.insert(feature.clone());
            }
        }
    }

    let finishing_order = finishing_order(&enabled, &adjacency);
    let mut assigned = BTreeSet::new();
    for root in finishing_order.into_iter().rev() {
        if !assigned.insert(root.clone()) {
            continue;
        }
        let mut component = vec![root];
        let mut pending = component.clone();
        while let Some(feature) = pending.pop() {
            if let Some(dependents) = reverse.get(&feature) {
                for dependent in dependents.iter().rev() {
                    if assigned.insert(dependent.clone()) {
                        pending.push(dependent.clone());
                        component.push(dependent.clone());
                    }
                }
            }
        }
        component.sort();
        let self_cycle = component.len() == 1
            && adjacency
                .get(&component[0])
                .is_some_and(|dependencies| dependencies.binary_search(&component[0]).is_ok());
        if component.len() > 1 || self_cycle {
            for feature in &component {
                if let Some(result) = results.get_mut(feature) {
                    result.state = EffectiveFeatureStateV1::Blocked;
                    result.reason = Some(EffectiveFeatureReasonV1::DependencyCycle {
                        members: component.clone(),
                    });
                }
            }
        }
    }
}

fn finishing_order(
    enabled: &BTreeSet<FeatureKeyV1>,
    adjacency: &BTreeMap<FeatureKeyV1, Vec<FeatureKeyV1>>,
) -> Vec<FeatureKeyV1> {
    let mut visited = BTreeSet::new();
    let mut finished = Vec::new();
    for root in enabled {
        if !visited.insert(root.clone()) {
            continue;
        }
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((feature, next_index)) = stack.last_mut() {
            let next_dependency = {
                let dependency = adjacency
                    .get(feature)
                    .and_then(|dependencies| dependencies.get(*next_index))
                    .cloned();
                if dependency.is_some() {
                    *next_index += 1;
                }
                dependency
            };
            if let Some(dependency) = next_dependency {
                if visited.insert(dependency.clone()) {
                    stack.push((dependency, 0));
                }
            } else if let Some((feature, _)) = stack.pop() {
                finished.push(feature);
            }
        }
    }
    finished
}

fn apply_dependencies(
    indexed: &BTreeMap<FeatureKeyV1, &FeatureResolutionFactV1>,
    results: &mut BTreeMap<FeatureKeyV1, EffectiveFeatureV1>,
) {
    let mut reverse = BTreeMap::<FeatureKeyV1, BTreeSet<FeatureKeyV1>>::new();
    for (feature, fact) in indexed {
        for dependency in sorted_dependencies(fact) {
            if results.contains_key(&dependency) {
                reverse
                    .entry(dependency)
                    .or_default()
                    .insert(feature.clone());
            }
        }
    }

    let mut pending = BTreeSet::new();
    for (feature, fact) in indexed {
        if results
            .get(feature)
            .is_none_or(|result| result.state != EffectiveFeatureStateV1::Enabled)
        {
            continue;
        }
        let reason = sorted_dependencies(fact)
            .into_iter()
            .find_map(|dependency| match results.get(&dependency) {
                None => Some(EffectiveFeatureReasonV1::MissingDependency { dependency }),
                Some(result) if result.state != EffectiveFeatureStateV1::Enabled => {
                    Some(EffectiveFeatureReasonV1::DependencyUnavailable {
                        dependency,
                        state: result.state,
                    })
                }
                Some(_) => None,
            });
        if let Some(reason) = reason
            && let Some(result) = results.get_mut(feature)
        {
            result.state = EffectiveFeatureStateV1::Blocked;
            result.reason = Some(reason);
            pending.insert(feature.clone());
        }
    }

    while let Some(dependency) = pending.pop_first() {
        let state = results
            .get(&dependency)
            .map_or(EffectiveFeatureStateV1::Blocked, |result| result.state);
        if let Some(dependents) = reverse.get(&dependency) {
            for dependent in dependents {
                if results
                    .get(dependent)
                    .is_some_and(|result| result.state == EffectiveFeatureStateV1::Enabled)
                    && let Some(result) = results.get_mut(dependent)
                {
                    result.state = EffectiveFeatureStateV1::Blocked;
                    result.reason = Some(EffectiveFeatureReasonV1::DependencyUnavailable {
                        dependency: dependency.clone(),
                        state,
                    });
                    pending.insert(dependent.clone());
                }
            }
        }
    }
}

fn sorted_dependencies(fact: &FeatureResolutionFactV1) -> Vec<FeatureKeyV1> {
    let mut dependencies = fact.dependencies.clone();
    dependencies.sort();
    dependencies
}

fn validate_fact(fact: &FeatureResolutionFactV1) -> Result<(), EffectiveFeatureResolverErrorV1> {
    validate_resolver_key(&fact.feature)?;
    if fact.dependencies.len() > MAX_FEATURE_DEPENDENCIES_V1 {
        return Err(EffectiveFeatureResolverErrorV1::LimitExceeded {
            field: "dependencies",
            maximum: MAX_FEATURE_DEPENDENCIES_V1,
        });
    }
    let mut dependencies = BTreeSet::new();
    for dependency in &fact.dependencies {
        validate_resolver_key(dependency)?;
        if !dependencies.insert(dependency.clone()) {
            return Err(EffectiveFeatureResolverErrorV1::DuplicateDependency {
                feature: fact.feature.clone(),
                dependency: dependency.clone(),
            });
        }
    }
    Ok(())
}

fn validate_resolver_key(key: &FeatureKeyV1) -> Result<(), EffectiveFeatureResolverErrorV1> {
    validate_resolver_identifier("package_id", &key.package_id)?;
    validate_resolver_identifier("feature_id", &key.feature_id)
}

fn validate_resolver_identifier(
    field: &'static str,
    value: &str,
) -> Result<(), EffectiveFeatureResolverErrorV1> {
    if identifier_is_valid(value) {
        Ok(())
    } else {
        Err(EffectiveFeatureResolverErrorV1::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), FeatureStateStoreErrorV1> {
    if identifier_is_valid(value) {
        Ok(())
    } else {
        Err(FeatureStateStoreErrorV1::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

fn identifier_is_valid(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, FeatureStateStoreErrorV1> {
    let file = File::open(path).map_err(|source| FeatureStateStoreErrorV1::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.take((MAX_FEATURE_STATE_STORE_BYTES_V1 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| FeatureStateStoreErrorV1::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_FEATURE_STATE_STORE_BYTES_V1 {
        return Err(FeatureStateStoreErrorV1::DocumentTooLarge {
            maximum: MAX_FEATURE_STATE_STORE_BYTES_V1,
        });
    }
    Ok(bytes)
}

fn temporary_path(path: &Path) -> Result<PathBuf, FeatureStateStoreErrorV1> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| FeatureStateStoreErrorV1::Io {
            path: path.to_path_buf(),
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "state path needs a Unicode file name",
            ),
        })?;
    for _attempt in 0..32 {
        let nonce = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.feature-state-{}-{nonce}.tmp",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(FeatureStateStoreErrorV1::Io {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate state temporary path",
        ),
    })
}

#[cfg(not(windows))]
fn replace_file_atomically(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn replace_file_atomically(temporary: &Path, path: &Path) -> io::Result<()> {
    if !path.exists() {
        return fs::rename(temporary, path);
    }
    let target = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let replacement = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // The pointers refer to the vectors above for the duration of this single Win32 call.
    let replaced = unsafe {
        replace_file_w(
            target.as_ptr(),
            replacement.as_ptr(),
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn key(package: &str, feature: &str) -> FeatureKeyV1 {
        FeatureKeyV1::new(package, feature).expect("valid test key")
    }

    fn fact(package: &str, feature: &str) -> FeatureResolutionFactV1 {
        FeatureResolutionFactV1 {
            feature: key(package, feature),
            dependencies: Vec::new(),
            compatibility: FeatureCompatibilityFactV1::Compatible,
            diagnostic: None,
            runtime: FeatureRuntimeFactV1::Ready,
        }
    }

    #[test]
    fn desired_store_round_trips_unknown_ids_without_effective_state() {
        let mut store = FeatureStateStoreV1::new();
        store
            .set_package_desired("removed.package", DesiredStateV1::Disabled)
            .expect("package");
        store
            .set_feature_desired(
                key("removed.package", "old-feature"),
                DesiredStateV1::Disabled,
            )
            .expect("feature");
        let restored = FeatureStateStoreV1::decode_json(&store.encode_json().expect("encode"))
            .expect("decode");
        assert_eq!(restored, store);
        assert!(
            !String::from_utf8(restored.encode_json().expect("encode"))
                .expect("utf8")
                .contains("faulted")
        );
    }

    #[test]
    fn corrupt_unknown_schema_and_unknown_enum_fail_closed() {
        assert!(FeatureStateStoreV1::decode_json(br#"{"schema_version":2,"global_desired":"enabled","package_desired":{},"feature_desired":[]}"#).is_err());
        assert!(FeatureStateStoreV1::decode_json(br#"{"schema_version":1,"global_desired":"later","package_desired":{},"feature_desired":[]}"#).is_err());
        assert!(FeatureStateStoreV1::decode_json(b"not json").is_err());
    }

    #[test]
    fn duplicate_package_entries_fail_closed_instead_of_last_wins() {
        let duplicate = br#"{
            "schema_version":1,
            "global_desired":"enabled",
            "package_desired":[
                {"package_id":"pkg","desired":"enabled"},
                {"package_id":"pkg","desired":"disabled"}
            ],
            "feature_desired":[]
        }"#;
        assert!(matches!(
            FeatureStateStoreV1::decode_json(duplicate),
            Err(FeatureStateStoreErrorV1::DuplicatePackageId { .. })
        ));
    }

    #[test]
    fn resolver_accepts_the_full_manifest_catalog_bound() {
        let store = FeatureStateStoreV1::new();
        let facts = (0..MAX_FEATURE_RESOLUTION_FACTS_V1)
            .map(|index| fact("pkg", &format!("f{index}")))
            .collect::<Vec<_>>();
        let resolved = EffectiveFeatureResolverV1::resolve(&store, &facts).expect("full catalog");
        assert_eq!(resolved.len(), MAX_FEATURE_RESOLUTION_FACTS_V1);
    }

    #[test]
    fn atomic_save_keeps_last_good_document_when_replace_fails() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("feature-state.json");
        let mut store = FeatureStateStoreV1::new();
        store.set_global_desired(DesiredStateV1::Disabled);
        store.save_atomic(&path).expect("initial save");
        let previous = fs::read(&path).expect("saved bytes");
        store.set_global_desired(DesiredStateV1::Enabled);
        let result = store.save_atomic_with(&path, |_temporary, _target| {
            Err(io::Error::other("injected replace failure"))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).expect("still saved"), previous);
        assert_eq!(
            FeatureStateStoreV1::load(&path)
                .expect("load")
                .global_desired(),
            DesiredStateV1::Disabled
        );
    }

    #[test]
    fn parent_off_preserves_child_desired_and_has_deterministic_precedence() {
        let mut store = FeatureStateStoreV1::new();
        let child = key("pkg", "child");
        store
            .set_feature_desired(child.clone(), DesiredStateV1::Enabled)
            .expect("child");
        store.set_global_desired(DesiredStateV1::Disabled);
        let mut input = fact("pkg", "child");
        input.runtime = FeatureRuntimeFactV1::Faulted;
        input.diagnostic = Some(FeatureDiagnosticFactV1::HostPolicy);
        input.compatibility =
            FeatureCompatibilityFactV1::Incompatible(FeatureCompatibilityIssueV1::Target);
        let result = EffectiveFeatureResolverV1::resolve(&store, &[input]).expect("resolve");
        assert_eq!(result[0].state, EffectiveFeatureStateV1::Faulted);
        assert_eq!(
            result[0].reason,
            Some(EffectiveFeatureReasonV1::RuntimeFaulted)
        );
        assert_eq!(store.feature_desired(&child), DesiredStateV1::Enabled);
    }

    #[test]
    fn explicit_runtime_transitions_remain_visible_after_desired_disable() {
        let mut store = FeatureStateStoreV1::new();
        store.set_global_desired(DesiredStateV1::Disabled);
        for (runtime, expected) in [
            (
                FeatureRuntimeFactV1::Disabling,
                EffectiveFeatureStateV1::Disabling,
            ),
            (
                FeatureRuntimeFactV1::PendingRestart,
                EffectiveFeatureStateV1::PendingRestart,
            ),
            (
                FeatureRuntimeFactV1::Faulted,
                EffectiveFeatureStateV1::Faulted,
            ),
        ] {
            let mut input = fact("pkg", "feature");
            input.runtime = runtime;
            assert_eq!(
                EffectiveFeatureResolverV1::resolve(&store, &[input]).expect("resolve")[0].state,
                expected
            );
        }
    }

    #[test]
    fn diagnostic_then_compatibility_then_dependency_precedence_is_pure() {
        let store = FeatureStateStoreV1::new();
        let mut input = fact("pkg", "feature");
        input.dependencies.push(key("pkg", "missing"));
        input.compatibility =
            FeatureCompatibilityFactV1::Incompatible(FeatureCompatibilityIssueV1::Target);
        input.diagnostic = Some(FeatureDiagnosticFactV1::PackageValidation);
        let result = EffectiveFeatureResolverV1::resolve(&store, &[input]).expect("resolve");
        assert_eq!(
            result[0].reason,
            Some(EffectiveFeatureReasonV1::Diagnostic(
                FeatureDiagnosticFactV1::PackageValidation
            ))
        );
    }

    #[test]
    fn missing_dependency_and_cycle_propagate_deterministically() {
        let store = FeatureStateStoreV1::new();
        let mut a = fact("pkg", "a");
        a.dependencies.push(key("pkg", "b"));
        let mut b = fact("pkg", "b");
        b.dependencies.push(key("pkg", "a"));
        let mut c = fact("pkg", "c");
        c.dependencies.push(key("pkg", "a"));
        let result = EffectiveFeatureResolverV1::resolve(&store, &[c, b, a]).expect("resolve");
        assert_eq!(
            result
                .iter()
                .map(|item| item.feature.feature_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        assert_eq!(result[0].state, EffectiveFeatureStateV1::Blocked);
        assert_eq!(result[1].state, EffectiveFeatureStateV1::Blocked);
        assert!(matches!(
            result[2].reason,
            Some(EffectiveFeatureReasonV1::DependencyUnavailable { .. })
        ));
    }

    #[test]
    fn branching_scc_members_are_identical_and_sorted() {
        let store = FeatureStateStoreV1::new();
        let mut a = fact("pkg", "a");
        a.dependencies = vec![key("pkg", "c"), key("pkg", "b")];
        let mut b = fact("pkg", "b");
        b.dependencies.push(key("pkg", "a"));
        let mut c = fact("pkg", "c");
        c.dependencies.push(key("pkg", "a"));
        let resolved = EffectiveFeatureResolverV1::resolve(&store, &[a, b, c]).expect("resolve");
        let expected = vec![key("pkg", "a"), key("pkg", "b"), key("pkg", "c")];
        for result in resolved {
            assert_eq!(result.state, EffectiveFeatureStateV1::Blocked);
            assert_eq!(
                result.reason,
                Some(EffectiveFeatureReasonV1::DependencyCycle {
                    members: expected.clone()
                })
            );
        }
    }

    #[test]
    fn self_cycles_and_multiple_sccs_have_exact_members() {
        let store = FeatureStateStoreV1::new();
        let mut a = fact("pkg", "a");
        a.dependencies.push(key("pkg", "b"));
        let mut b = fact("pkg", "b");
        b.dependencies.push(key("pkg", "a"));
        let mut c = fact("pkg", "c");
        c.dependencies.push(key("pkg", "d"));
        let mut d = fact("pkg", "d");
        d.dependencies.push(key("pkg", "c"));
        let mut self_cycle = fact("pkg", "self");
        self_cycle.dependencies.push(key("pkg", "self"));
        let resolved = EffectiveFeatureResolverV1::resolve(&store, &[d, self_cycle, b, c, a])
            .expect("resolve");
        assert_eq!(
            resolved[0].reason,
            Some(EffectiveFeatureReasonV1::DependencyCycle {
                members: vec![key("pkg", "a"), key("pkg", "b")]
            })
        );
        assert_eq!(
            resolved[2].reason,
            Some(EffectiveFeatureReasonV1::DependencyCycle {
                members: vec![key("pkg", "c"), key("pkg", "d")]
            })
        );
        assert_eq!(
            resolved[4].reason,
            Some(EffectiveFeatureReasonV1::DependencyCycle {
                members: vec![key("pkg", "self")]
            })
        );
    }

    #[test]
    fn scc_results_are_independent_of_input_permutation() {
        let store = FeatureStateStoreV1::new();
        let mut a = fact("pkg", "a");
        a.dependencies = vec![key("pkg", "b"), key("pkg", "c")];
        let mut b = fact("pkg", "b");
        b.dependencies.push(key("pkg", "a"));
        let mut c = fact("pkg", "c");
        c.dependencies.push(key("pkg", "a"));
        let first = EffectiveFeatureResolverV1::resolve(&store, &[a.clone(), b.clone(), c.clone()])
            .expect("first");
        let second = EffectiveFeatureResolverV1::resolve(&store, &[c, a, b]).expect("second");
        assert_eq!(first, second);
    }

    #[test]
    fn reverse_worklist_handles_reversed_long_chains_deterministically() {
        const CHAIN_LENGTH: usize = 2_048;
        let store = FeatureStateStoreV1::new();
        let facts = (0..CHAIN_LENGTH)
            .map(|index| {
                let mut entry = fact("pkg", &format!("f{index:05}"));
                let dependency = if index + 1 == CHAIN_LENGTH {
                    key("pkg", "missing")
                } else {
                    key("pkg", &format!("f{:05}", index + 1))
                };
                entry.dependencies.push(dependency);
                entry
            })
            .collect::<Vec<_>>();
        let forward = EffectiveFeatureResolverV1::resolve(&store, &facts).expect("forward");
        let reverse = EffectiveFeatureResolverV1::resolve(
            &store,
            &facts.iter().cloned().rev().collect::<Vec<_>>(),
        )
        .expect("reverse");
        assert_eq!(forward, reverse);
        assert!(
            forward
                .iter()
                .all(|result| result.state == EffectiveFeatureStateV1::Blocked)
        );
        assert!(matches!(
            forward.last().and_then(|result| result.reason.as_ref()),
            Some(EffectiveFeatureReasonV1::MissingDependency { .. })
        ));
    }
}
