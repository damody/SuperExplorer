//! Host-owned authority for dynamic column descriptors.
//!
//! `explorer-model::ColumnRegistry` remains a copied projection/layout helper.
//! This module owns the package/feature lifecycle facts that decide whether a
//! descriptor is visible and whether its provider or renderer may dispatch.

use std::collections::BTreeMap;

use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId, ColumnRegistry,
    ColumnRegistryError, ColumnSortSemantics, ColumnValueType,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnFeatureRuntimeStateV1 {
    Enabled,
    Disabled,
    Blocked,
    Faulted,
    SafeModeSuppressed,
    Draining,
}

impl ColumnFeatureRuntimeStateV1 {
    const fn permits_dispatch(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedColumnRegistrationV1 {
    pub package_id: String,
    pub feature_id: String,
    pub interface_id: String,
    pub incarnation: u64,
    pub generation: u64,
    pub state: ColumnFeatureRuntimeStateV1,
    pub descriptors: Vec<ColumnDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageColumnsV1 {
    feature_id: String,
    interface_id: String,
    incarnation: u64,
    generation: u64,
    state: ColumnFeatureRuntimeStateV1,
    descriptors: BTreeMap<ColumnId, ColumnDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnCatalogSnapshotV1 {
    generation: u64,
    descriptors: Vec<ColumnDescriptor>,
}

impl ColumnCatalogSnapshotV1 {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn descriptors(&self) -> &[ColumnDescriptor] {
        &self.descriptors
    }

    /// Creates the model-side copied projection. No authority or callback is
    /// transferred to the model.
    pub fn model_projection(&self) -> Result<ColumnRegistry, ColumnRegistryError> {
        let mut projection = ColumnRegistry::built_ins();
        let mut packages = BTreeMap::<&str, Vec<ColumnDescriptor>>::new();
        for descriptor in &self.descriptors {
            if let Some((package, _)) = descriptor.id.extension_parts() {
                packages
                    .entry(package)
                    .or_default()
                    .push(descriptor.clone());
            }
        }
        for (package, descriptors) in packages {
            projection.replace_package(package, descriptors)?;
        }
        Ok(projection)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColumnAuthorityRegistryErrorV1 {
    InvalidIdentity,
    InvalidDescriptor(ColumnRegistryError),
    UnknownPackage,
    UnknownColumn,
    Inactive(ColumnFeatureRuntimeStateV1),
    Stale,
}

pub struct HostColumnAuthorityRegistryV1 {
    generation: u64,
    packages: BTreeMap<String, PackageColumnsV1>,
    snapshot: ColumnCatalogSnapshotV1,
}

impl Default for HostColumnAuthorityRegistryV1 {
    fn default() -> Self {
        Self {
            generation: 1,
            packages: BTreeMap::new(),
            snapshot: ColumnCatalogSnapshotV1 {
                generation: 1,
                descriptors: Vec::new(),
            },
        }
    }
}

impl HostColumnAuthorityRegistryV1 {
    #[must_use]
    pub fn snapshot(&self) -> ColumnCatalogSnapshotV1 {
        self.snapshot.clone()
    }

    /// Validates the complete replacement before changing the live catalog.
    pub fn replace_package(
        &mut self,
        registration: SealedColumnRegistrationV1,
    ) -> Result<ColumnCatalogSnapshotV1, ColumnAuthorityRegistryErrorV1> {
        if registration.package_id.is_empty()
            || registration.feature_id.is_empty()
            || registration.interface_id.is_empty()
            || registration.incarnation == 0
            || registration.generation == 0
        {
            return Err(ColumnAuthorityRegistryErrorV1::InvalidIdentity);
        }
        let mut validator = ColumnRegistry::built_ins();
        validator
            .replace_package(&registration.package_id, registration.descriptors.clone())
            .map_err(ColumnAuthorityRegistryErrorV1::InvalidDescriptor)?;
        let descriptors = registration
            .descriptors
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect();
        self.packages.insert(
            registration.package_id,
            PackageColumnsV1 {
                feature_id: registration.feature_id,
                interface_id: registration.interface_id,
                incarnation: registration.incarnation,
                generation: registration.generation,
                state: registration.state,
                descriptors,
            },
        );
        self.reconcile();
        Ok(self.snapshot())
    }

    /// Consumes the frozen public ABI descriptor into host-owned model data.
    /// ABI allocations and semantic newtypes never escape this conversion.
    #[allow(clippy::too_many_arguments)]
    pub fn replace_public_package(
        &mut self,
        package_id: &str,
        feature_id: &str,
        interface_id: &str,
        incarnation: u64,
        generation: u64,
        state: ColumnFeatureRuntimeStateV1,
        descriptors: &[explorer_extension_api::ColumnDescriptorV1],
    ) -> Result<ColumnCatalogSnapshotV1, ColumnAuthorityRegistryErrorV1> {
        let descriptors = descriptors
            .iter()
            .map(|descriptor| public_descriptor(package_id, descriptor))
            .collect::<Result<Vec<_>, _>>()?;
        self.replace_package(SealedColumnRegistrationV1 {
            package_id: package_id.to_owned(),
            feature_id: feature_id.to_owned(),
            interface_id: interface_id.to_owned(),
            incarnation,
            generation,
            state,
            descriptors,
        })
    }

    pub fn unregister_package(&mut self, package_id: &str) -> bool {
        let removed = self.packages.remove(package_id).is_some();
        if removed {
            self.reconcile();
        }
        removed
    }

    pub fn set_package_state(
        &mut self,
        package_id: &str,
        state: ColumnFeatureRuntimeStateV1,
    ) -> Result<ColumnCatalogSnapshotV1, ColumnAuthorityRegistryErrorV1> {
        self.packages
            .get_mut(package_id)
            .ok_or(ColumnAuthorityRegistryErrorV1::UnknownPackage)?
            .state = state;
        self.reconcile();
        Ok(self.snapshot())
    }

    /// Revalidates lifecycle identity immediately before provider/renderer use.
    pub fn authorize_dispatch(
        &self,
        package_id: &str,
        feature_id: &str,
        interface_id: &str,
        incarnation: u64,
        generation: u64,
        column_id: &ColumnId,
    ) -> Result<&ColumnDescriptor, ColumnAuthorityRegistryErrorV1> {
        let package = self
            .packages
            .get(package_id)
            .ok_or(ColumnAuthorityRegistryErrorV1::UnknownPackage)?;
        if !package.state.permits_dispatch() {
            return Err(ColumnAuthorityRegistryErrorV1::Inactive(package.state));
        }
        if package.feature_id != feature_id
            || package.interface_id != interface_id
            || package.incarnation != incarnation
            || package.generation != generation
        {
            return Err(ColumnAuthorityRegistryErrorV1::Stale);
        }
        package
            .descriptors
            .get(column_id)
            .ok_or(ColumnAuthorityRegistryErrorV1::UnknownColumn)
    }

    fn reconcile(&mut self) {
        self.generation = self.generation.saturating_add(1);
        let descriptors = self
            .packages
            .values()
            .filter(|package| package.state.permits_dispatch())
            .flat_map(|package| package.descriptors.values().cloned())
            .collect();
        self.snapshot = ColumnCatalogSnapshotV1 {
            generation: self.generation,
            descriptors,
        };
    }
}

fn public_descriptor(
    package_id: &str,
    descriptor: &explorer_extension_api::ColumnDescriptorV1,
) -> Result<ColumnDescriptor, ColumnAuthorityRegistryErrorV1> {
    descriptor
        .validate()
        .map_err(|_| ColumnAuthorityRegistryErrorV1::InvalidIdentity)?;
    let value_type = match descriptor.value_kind.into_raw() {
        1 => ColumnValueType::Boolean,
        2 => ColumnValueType::Integer,
        3 => ColumnValueType::Float,
        4 => ColumnValueType::Bytes,
        5 => ColumnValueType::Time,
        6 => ColumnValueType::Duration,
        7 => ColumnValueType::Text,
        8 => ColumnValueType::LocalizedText,
        9 => ColumnValueType::Structured,
        10 => ColumnValueType::Opaque,
        _ => return Err(ColumnAuthorityRegistryErrorV1::InvalidIdentity),
    };
    let sort_semantics = match descriptor.stable_sort_kind {
        abi_stable::std_types::ROption::RNone => ColumnSortSemantics::Unsupported,
        abi_stable::std_types::ROption::RSome(kind) if kind.into_raw() == 1 => {
            ColumnSortSemantics::Boolean
        }
        abi_stable::std_types::ROption::RSome(kind) if matches!(kind.into_raw(), 2 | 3) => {
            ColumnSortSemantics::Integer
        }
        abi_stable::std_types::ROption::RSome(kind) if kind.into_raw() == 4 => {
            ColumnSortSemantics::Float
        }
        abi_stable::std_types::ROption::RSome(kind) if kind.into_raw() == 5 => {
            ColumnSortSemantics::Time
        }
        abi_stable::std_types::ROption::RSome(kind) if kind.into_raw() == 6 => {
            ColumnSortSemantics::Duration
        }
        abi_stable::std_types::ROption::RSome(kind) if kind.into_raw() == 7 => {
            ColumnSortSemantics::Text
        }
        abi_stable::std_types::ROption::RSome(kind) if kind.into_raw() == 8 => {
            ColumnSortSemantics::Bytes
        }
        _ => return Err(ColumnAuthorityRegistryErrorV1::InvalidIdentity),
    };
    Ok(ColumnDescriptor {
        id: ColumnId::extension(package_id, descriptor.id.0.as_str())
            .map_err(|_| ColumnAuthorityRegistryErrorV1::InvalidIdentity)?,
        display_name: descriptor.display_name.to_string(),
        value_type,
        default_width: descriptor.default_width,
        minimum_width: descriptor.minimum_width,
        maximum_width: descriptor.maximum_width,
        alignment: match descriptor.alignment.into_raw() {
            1 => ColumnAlignment::Start,
            2 => ColumnAlignment::Center,
            3 => ColumnAlignment::End,
            _ => return Err(ColumnAuthorityRegistryErrorV1::InvalidIdentity),
        },
        applicability: match descriptor.applicability.into_raw() {
            1 => ColumnApplicability::AllEntries,
            2 => ColumnApplicability::Files,
            3 => ColumnApplicability::Containers,
            _ => return Err(ColumnAuthorityRegistryErrorV1::InvalidIdentity),
        },
        sort_semantics,
        cost: match descriptor.cost.into_raw() {
            1 => ColumnCost::Immediate,
            2 => ColumnCost::BackgroundSingle,
            3 => ColumnCost::BackgroundBatch,
            4 => ColumnCost::BackgroundAggregate,
            _ => return Err(ColumnAuthorityRegistryErrorV1::InvalidIdentity),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_model::{
        ColumnAlignment, ColumnApplicability, ColumnCost, ColumnSortSemantics, ColumnValueType,
    };

    fn descriptor(package: &str, local: &str) -> ColumnDescriptor {
        ColumnDescriptor {
            id: ColumnId::extension(package, local).unwrap(),
            display_name: local.to_owned(),
            value_type: ColumnValueType::Bytes,
            default_width: 144,
            minimum_width: 48,
            maximum_width: 600,
            alignment: ColumnAlignment::End,
            applicability: ColumnApplicability::AllEntries,
            sort_semantics: ColumnSortSemantics::Bytes,
            cost: ColumnCost::BackgroundBatch,
        }
    }

    fn registration(package: &str, local: &str) -> SealedColumnRegistrationV1 {
        SealedColumnRegistrationV1 {
            package_id: package.to_owned(),
            feature_id: "feature".to_owned(),
            interface_id: "column.v1".to_owned(),
            incarnation: 1,
            generation: 1,
            state: ColumnFeatureRuntimeStateV1::Enabled,
            descriptors: vec![descriptor(package, local)],
        }
    }

    #[test]
    fn replacement_is_atomic_and_catalog_order_ignores_callback_arrival() {
        let mut forward = HostColumnAuthorityRegistryV1::default();
        forward
            .replace_package(registration("org.example.a", "same"))
            .unwrap();
        forward
            .replace_package(registration("org.example.b", "same"))
            .unwrap();
        let mut reverse = HostColumnAuthorityRegistryV1::default();
        reverse
            .replace_package(registration("org.example.b", "same"))
            .unwrap();
        reverse
            .replace_package(registration("org.example.a", "same"))
            .unwrap();
        assert_eq!(
            forward.snapshot().descriptors(),
            reverse.snapshot().descriptors()
        );

        let before = forward.snapshot();
        let mut invalid = registration("org.example.a", "replacement");
        invalid.descriptors[0].minimum_width = 500;
        assert!(forward.replace_package(invalid).is_err());
        assert_eq!(forward.snapshot(), before);
    }

    #[test]
    fn every_non_enabled_state_revokes_visibility_and_dispatch() {
        for state in [
            ColumnFeatureRuntimeStateV1::Disabled,
            ColumnFeatureRuntimeStateV1::Blocked,
            ColumnFeatureRuntimeStateV1::Faulted,
            ColumnFeatureRuntimeStateV1::SafeModeSuppressed,
            ColumnFeatureRuntimeStateV1::Draining,
        ] {
            let mut registry = HostColumnAuthorityRegistryV1::default();
            let id = descriptor("org.example.a", "value").id;
            registry
                .replace_package(registration("org.example.a", "value"))
                .unwrap();
            assert!(
                registry
                    .authorize_dispatch("org.example.a", "feature", "column.v1", 1, 1, &id)
                    .is_ok()
            );
            registry.set_package_state("org.example.a", state).unwrap();
            assert!(registry.snapshot().descriptors().is_empty());
            assert_eq!(
                registry.authorize_dispatch("org.example.a", "feature", "column.v1", 1, 1, &id),
                Err(ColumnAuthorityRegistryErrorV1::Inactive(state))
            );
        }
    }

    #[test]
    fn update_generation_and_unregister_revoke_old_dispatch_without_model_authority() {
        let mut registry = HostColumnAuthorityRegistryV1::default();
        let id = descriptor("org.example.a", "value").id;
        registry
            .replace_package(registration("org.example.a", "value"))
            .unwrap();
        let projection = registry.snapshot().model_projection().unwrap();
        assert!(projection.contains(&id));
        let mut updated = registration("org.example.a", "value");
        updated.incarnation = 2;
        updated.generation = 2;
        registry.replace_package(updated).unwrap();
        assert_eq!(
            registry.authorize_dispatch("org.example.a", "feature", "column.v1", 1, 1, &id),
            Err(ColumnAuthorityRegistryErrorV1::Stale)
        );
        assert!(registry.unregister_package("org.example.a"));
        assert!(registry.snapshot().descriptors().is_empty());
    }
}
