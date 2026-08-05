//! Host-owned dynamic view catalog and dispatch authority.

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use crate::{
    ColumnFeatureRuntimeStateV1,
    runtime_authority::{AuthorityAdapterV1, AuthorityEnvelopeV1, RuntimeAuthorityV1},
};

/// Opaque use-time grant for one extension-view navigation bridge.
#[derive(Clone)]
pub struct NavigationAuthorityV1 {
    runtime: Arc<RuntimeAuthorityV1>,
    envelope: AuthorityEnvelopeV1,
}

impl std::fmt::Debug for NavigationAuthorityV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NavigationAuthorityV1")
            .finish_non_exhaustive()
    }
}

impl NavigationAuthorityV1 {
    pub(crate) fn from_host(
        runtime: Arc<RuntimeAuthorityV1>,
        envelope: AuthorityEnvelopeV1,
    ) -> Self {
        Self { runtime, envelope }
    }

    fn revalidate(&self) -> bool {
        self.runtime
            .revalidate(&self.envelope, AuthorityAdapterV1::Navigation)
            .is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewNavigationRouteErrorV1 {
    Unauthorized,
    StaleOrUnknownNode,
}

/// Host-owned adapter which resolves only opaque node IDs from the immutable
/// snapshot that minted them. The caller's dispatch closure is the existing
/// tab/open-policy action and runs only after the envelope is revalidated.
pub struct HostViewNavigationAdapterV1 {
    authority: NavigationAuthorityV1,
    snapshot: explorer_extension_api::ViewSnapshotIdentityV1,
    known_node_ids: HashSet<explorer_extension_api::StableIdV1>,
}

impl HostViewNavigationAdapterV1 {
    #[must_use]
    pub fn new(
        authority: NavigationAuthorityV1,
        snapshot: explorer_extension_api::ViewSnapshotIdentityV1,
        known_node_ids: impl IntoIterator<Item = explorer_extension_api::StableIdV1>,
    ) -> Self {
        Self {
            authority,
            snapshot,
            known_node_ids: known_node_ids.into_iter().collect(),
        }
    }

    pub fn authorize_and_dispatch<T>(
        &self,
        request: &explorer_extension_api::NavigationRequestV1,
        dispatch: impl FnOnce(
            explorer_extension_api::ViewNavigationOperationV1,
            explorer_extension_api::StableIdV1,
        ) -> T,
    ) -> Result<T, ViewNavigationRouteErrorV1> {
        if !self.authority.revalidate() {
            return Err(ViewNavigationRouteErrorV1::Unauthorized);
        }
        if !request.is_authorized_for(self.snapshot, &self.known_node_ids) {
            return Err(ViewNavigationRouteErrorV1::StaleOrUnknownNode);
        }
        if !self.authority.revalidate() {
            return Err(ViewNavigationRouteErrorV1::Unauthorized);
        }
        Ok(dispatch(request.operation, request.node_id))
    }
}

#[derive(Clone, Debug)]
pub struct SealedViewPackageV1 {
    pub package_id: String,
    pub feature_id: String,
    pub incarnation: u64,
    pub generation: u64,
    pub state: ColumnFeatureRuntimeStateV1,
    pub capabilities: Vec<String>,
    pub expected_ui_fingerprint: [u8; 32],
    pub actual_ui_fingerprint: [u8; 32],
    pub views: Vec<explorer_extension_api::ViewModeRegistrationV1>,
}

#[derive(Clone, Debug)]
struct PackageViewsV1 {
    feature_id: String,
    incarnation: u64,
    generation: u64,
    state: ColumnFeatureRuntimeStateV1,
    views: BTreeMap<String, explorer_extension_api::ViewModeRegistrationV1>,
}

#[derive(Clone, Debug)]
pub struct ViewCatalogSnapshotV1 {
    pub generation: u64,
    pub views: Vec<(String, explorer_extension_api::ViewModeRegistrationV1)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewRegistryErrorV1 {
    InvalidIdentity,
    InvalidDescriptor,
    MissingCapability,
    FingerprintMismatch,
    DuplicateView,
    UnknownPackage,
    UnknownView,
    Inactive,
    Stale,
}

#[derive(Default)]
pub struct HostViewRegistryV1 {
    generation: u64,
    packages: BTreeMap<String, PackageViewsV1>,
}

impl HostViewRegistryV1 {
    pub fn replace_package(
        &mut self,
        package: SealedViewPackageV1,
    ) -> Result<ViewCatalogSnapshotV1, ViewRegistryErrorV1> {
        if package.package_id.is_empty()
            || package.feature_id.is_empty()
            || package.incarnation == 0
            || package.generation == 0
        {
            return Err(ViewRegistryErrorV1::InvalidIdentity);
        }
        if !package
            .capabilities
            .iter()
            .any(|capability| capability == "gpui.render")
        {
            return Err(ViewRegistryErrorV1::MissingCapability);
        }
        if package.expected_ui_fingerprint != package.actual_ui_fingerprint {
            return Err(ViewRegistryErrorV1::FingerprintMismatch);
        }
        let mut views = BTreeMap::new();
        for view in package.views {
            view.validate()
                .map_err(|_| ViewRegistryErrorV1::InvalidDescriptor)?;
            let id = format!("extension:{}:{}", package.package_id, view.id);
            if views.insert(id, view).is_some() {
                return Err(ViewRegistryErrorV1::DuplicateView);
            }
        }
        self.packages.insert(
            package.package_id,
            PackageViewsV1 {
                feature_id: package.feature_id,
                incarnation: package.incarnation,
                generation: package.generation,
                state: package.state,
                views,
            },
        );
        self.generation = self.generation.saturating_add(1).max(1);
        Ok(self.snapshot())
    }

    pub fn unregister_package(&mut self, package_id: &str) -> bool {
        let removed = self.packages.remove(package_id).is_some();
        if removed {
            self.generation = self.generation.saturating_add(1);
        }
        removed
    }

    pub fn set_package_state(
        &mut self,
        package_id: &str,
        state: ColumnFeatureRuntimeStateV1,
    ) -> Result<(), ViewRegistryErrorV1> {
        self.packages
            .get_mut(package_id)
            .ok_or(ViewRegistryErrorV1::UnknownPackage)?
            .state = state;
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }

    pub fn authorize(
        &self,
        package_id: &str,
        feature_id: &str,
        incarnation: u64,
        generation: u64,
        view_id: &str,
    ) -> Result<&explorer_extension_api::ViewModeRegistrationV1, ViewRegistryErrorV1> {
        let package = self
            .packages
            .get(package_id)
            .ok_or(ViewRegistryErrorV1::UnknownPackage)?;
        if package.state != ColumnFeatureRuntimeStateV1::Enabled {
            return Err(ViewRegistryErrorV1::Inactive);
        }
        if package.feature_id != feature_id
            || package.incarnation != incarnation
            || package.generation != generation
        {
            return Err(ViewRegistryErrorV1::Stale);
        }
        package
            .views
            .get(view_id)
            .ok_or(ViewRegistryErrorV1::UnknownView)
    }

    pub fn snapshot(&self) -> ViewCatalogSnapshotV1 {
        ViewCatalogSnapshotV1 {
            generation: self.generation,
            views: self
                .packages
                .values()
                .filter(|package| package.state == ColumnFeatureRuntimeStateV1::Enabled)
                .flat_map(|package| {
                    package
                        .views
                        .iter()
                        .map(|(id, view)| (id.clone(), view.clone()))
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_authority::AuthorityClaimsV1;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn navigation_authority() -> NavigationAuthorityV1 {
        let runtime = Arc::new(RuntimeAuthorityV1::new().unwrap());
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: "navigation-test".into(),
                feature_id: "view".into(),
                interface_id: "size-map".into(),
                incarnation: 1,
                capability: "navigation.request".into(),
                authorized_root_sha256: "a".repeat(64),
                location_generation: 3,
                item_generation: 1,
                refresh_generation: 5,
                container_generation: 1,
                job_generation: 8,
            })
            .unwrap();
        NavigationAuthorityV1::from_host(runtime, envelope)
    }

    fn package(id: &str) -> SealedViewPackageV1 {
        SealedViewPackageV1 {
            package_id: id.into(),
            feature_id: "view".into(),
            incarnation: 1,
            generation: 1,
            state: ColumnFeatureRuntimeStateV1::Enabled,
            capabilities: vec!["gpui.render".into()],
            expected_ui_fingerprint: [7; 32],
            actual_ui_fingerprint: [7; 32],
            views: vec![explorer_extension_api::ViewModeRegistrationV1 {
                id: "size-map".into(),
                display_name: "Size Map".into(),
                icon: explorer_extension_api::ViewIconV1::TREE_MAP,
                locations: explorer_extension_api::ViewLocationKindsV1::FILESYSTEM,
                priority: 10,
                selection: explorer_extension_api::ViewSelectionCapabilityV1::MULTIPLE,
                factory_interface_id: explorer_extension_api::StableIdV1::new(
                    explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                    9,
                ),
                factory_contribution_id: "size-map".into(),
            }],
        }
    }

    #[test]
    fn catalog_is_deterministic_and_lifecycle_revalidates_dispatch() {
        let mut forward = HostViewRegistryV1::default();
        forward.replace_package(package("org.example.b")).unwrap();
        forward.replace_package(package("org.example.a")).unwrap();
        let mut reverse = HostViewRegistryV1::default();
        reverse.replace_package(package("org.example.a")).unwrap();
        reverse.replace_package(package("org.example.b")).unwrap();
        let ids = |registry: &HostViewRegistryV1| {
            registry
                .snapshot()
                .views
                .into_iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&forward), ids(&reverse));
        let id = "extension:org.example.a:size-map";
        assert!(forward.authorize("org.example.a", "view", 1, 1, id).is_ok());
        forward
            .set_package_state("org.example.a", ColumnFeatureRuntimeStateV1::Disabled)
            .unwrap();
        assert!(matches!(
            forward.authorize("org.example.a", "view", 1, 1, id),
            Err(ViewRegistryErrorV1::Inactive)
        ));
        forward.replace_package(package("org.example.a")).unwrap();
        assert!(forward.authorize("org.example.a", "view", 1, 1, id).is_ok());
        assert!(forward.unregister_package("org.example.a"));
        assert!(matches!(
            forward.authorize("org.example.a", "view", 1, 1, id),
            Err(ViewRegistryErrorV1::UnknownPackage)
        ));
    }

    #[test]
    fn capability_fingerprint_and_duplicates_fail_closed() {
        let mut registry = HostViewRegistryV1::default();
        let mut missing = package("org.example.a");
        missing.capabilities.clear();
        assert!(matches!(
            registry.replace_package(missing),
            Err(ViewRegistryErrorV1::MissingCapability)
        ));
        let mut mismatch = package("org.example.a");
        mismatch.actual_ui_fingerprint = [8; 32];
        assert!(matches!(
            registry.replace_package(mismatch),
            Err(ViewRegistryErrorV1::FingerprintMismatch)
        ));
        let mut duplicate = package("org.example.a");
        duplicate.views.push(duplicate.views[0].clone());
        assert!(matches!(
            registry.replace_package(duplicate),
            Err(ViewRegistryErrorV1::DuplicateView)
        ));
    }

    #[test]
    fn navigation_adapter_rejects_stale_unknown_and_revoked_before_dispatch() {
        let snapshot = explorer_extension_api::ViewSnapshotIdentityV1 {
            location_generation: 3,
            refresh_generation: 5,
            render_revision: 8,
        };
        let known = explorer_extension_api::StableIdV1::new(
            explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
            1,
        );
        let authority = navigation_authority();
        let runtime = Arc::clone(&authority.runtime);
        let adapter = HostViewNavigationAdapterV1::new(authority, snapshot, [known]);
        let calls = AtomicUsize::new(0);
        let request = explorer_extension_api::NavigationRequestV1 {
            snapshot,
            operation: explorer_extension_api::ViewNavigationOperationV1::ENTER,
            node_id: known,
        };
        assert_eq!(
            adapter.authorize_and_dispatch(&request, |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
            }),
            Ok(())
        );
        let stale = explorer_extension_api::NavigationRequestV1 {
            snapshot: explorer_extension_api::ViewSnapshotIdentityV1 {
                render_revision: 7,
                ..snapshot
            },
            ..request.clone()
        };
        assert_eq!(
            adapter.authorize_and_dispatch(&stale, |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
            }),
            Err(ViewNavigationRouteErrorV1::StaleOrUnknownNode)
        );
        assert_eq!(runtime.revoke_feature("navigation-test", "view"), Ok(1));
        assert_eq!(
            adapter.authorize_and_dispatch(&request, |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
            }),
            Err(ViewNavigationRouteErrorV1::Unauthorized)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
