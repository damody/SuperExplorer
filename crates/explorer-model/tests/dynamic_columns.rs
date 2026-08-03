use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId, ColumnIdError,
    ColumnRegistry, ColumnRegistryError, ColumnSortSemantics, ColumnValueType, OrderedColumnLayout,
    PersistedColumn, PersistedColumnWidths, PersistedViewSettings,
};

fn extension_descriptor(id: ColumnId) -> ColumnDescriptor {
    ColumnDescriptor {
        id,
        display_name: "Folder size".to_owned(),
        value_type: ColumnValueType::Bytes,
        default_width: 144,
        minimum_width: 48,
        maximum_width: 600,
        alignment: ColumnAlignment::End,
        applicability: ColumnApplicability::Containers,
        sort_semantics: ColumnSortSemantics::Bytes,
        cost: ColumnCost::BackgroundAggregate,
    }
}

#[test]
fn stable_ids_and_descriptor_registry_reject_collisions_and_bad_ownership() {
    let mut registry = ColumnRegistry::built_ins();
    assert_eq!(registry.iter().len(), ColumnId::BUILT_INS.len());

    let id = ColumnId::extension("org.example.folder-size", "bytes").unwrap();
    registry
        .replace_package(
            "org.example.folder-size",
            [extension_descriptor(id.clone())],
        )
        .unwrap();
    assert!(registry.contains(&id));
    assert!(matches!(
        registry.replace_package(
            "org.example.folder-size",
            [
                extension_descriptor(id.clone()),
                extension_descriptor(id.clone())
            ],
        ),
        Err(ColumnRegistryError::DuplicateId(_)),
    ));

    let foreign = ColumnId::extension("org.example.other", "bytes").unwrap();
    assert!(matches!(
        registry.replace_package("org.example.folder-size", [extension_descriptor(foreign)]),
        Err(ColumnRegistryError::OwnershipMismatch { .. })
    ));
    assert!(matches!(
        ColumnId::extension("Org.Example", "bytes"),
        Err(ColumnIdError::InvalidFirstCharacter('O'))
    ));
    assert!(matches!(
        ColumnId::parse("org.example.folder-size"),
        Err(ColumnIdError::MissingNamespace)
    ));
    assert!(matches!(
        ColumnId::Extension {
            package_id: "builtin".to_owned(),
            column_id: "evil".to_owned(),
        }
        .validate(),
        Err(ColumnIdError::ReservedNamespace)
    ));
}

#[test]
fn ordered_layout_preserves_width_visibility_and_deterministic_order() {
    let mut registry = ColumnRegistry::built_ins();
    let id = ColumnId::extension("org.example.folder-size", "bytes").unwrap();
    let descriptor = extension_descriptor(id.clone());
    registry
        .replace_package("org.example.folder-size", [descriptor.clone()])
        .unwrap();

    let mut layout = OrderedColumnLayout::default();
    let builtin_order: Vec<_> = layout
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    assert_eq!(builtin_order, ColumnId::BUILT_INS);
    layout.ensure_descriptor(&descriptor, false);
    assert_eq!(layout.width(&id), Some(144));
    assert!(!layout.visible(&id));

    assert!(layout.set_width(&id, 1));
    assert_eq!(layout.width(&id), Some(48));
    assert!(layout.set_visible(&id, true));
    assert!(layout.visible(&id));
    assert!(layout.move_before(&id, Some(&ColumnId::Size)));
    let ext_index = layout
        .entries()
        .iter()
        .position(|entry| entry.id == id)
        .unwrap();
    let size_index = layout
        .entries()
        .iter()
        .position(|entry| entry.id == ColumnId::Size)
        .unwrap();
    assert!(ext_index < size_index);
    assert_eq!(layout.visible_registered(&registry).count(), 5);
}

#[test]
fn descriptor_validation_rejects_invalid_width_range() {
    let id = ColumnId::extension("org.example.folder-size", "invalid").unwrap();
    let mut descriptor = extension_descriptor(id);
    descriptor.minimum_width = 200;
    descriptor.default_width = 100;
    assert!(matches!(
        descriptor.validate(),
        Err(ColumnRegistryError::InvalidWidthRange { .. })
    ));
}

#[test]
fn descriptor_display_type_and_host_stable_sort_key_are_independent() {
    let id = ColumnId::extension("org.example.folder-size", "localized-size").unwrap();
    let mut localized_size = extension_descriptor(id);
    localized_size.value_type = ColumnValueType::LocalizedText;
    localized_size.sort_semantics = ColumnSortSemantics::Bytes;
    assert_eq!(localized_size.validate(), Ok(()));

    // The displayed text can remain user-facing while a copied integer key gives the host a
    // deterministic, locale-independent ordering.
    localized_size.value_type = ColumnValueType::Text;
    localized_size.sort_semantics = ColumnSortSemantics::Integer;
    assert_eq!(localized_size.validate(), Ok(()));

    // A descriptor cannot request a plugin comparator: V1 accepts only host-comparable stable
    // sort-key domains, or an explicitly unsupported sort affordance.
    localized_size.sort_semantics = ColumnSortSemantics::ProviderDefined;
    assert!(matches!(
        localized_size.validate(),
        Err(ColumnRegistryError::IncompatibleSortSemantics)
    ));

    let malformed = ColumnId::Extension {
        package_id: "builtin".to_owned(),
        column_id: "forbidden".to_owned(),
    };
    localized_size.id = malformed;
    localized_size.sort_semantics = ColumnSortSemantics::Bytes;
    assert!(matches!(
        localized_size.validate(),
        Err(ColumnRegistryError::InvalidExtensionId(
            ColumnIdError::ReservedNamespace
        ))
    ));
}

#[test]
fn package_revoke_hides_but_retains_layout_until_same_id_returns() {
    let mut registry = ColumnRegistry::built_ins();
    let id = ColumnId::extension("org.example.folder-size", "bytes").unwrap();
    let descriptor = extension_descriptor(id.clone());
    registry
        .replace_package("org.example.folder-size", [descriptor.clone()])
        .unwrap();
    let mut layout = OrderedColumnLayout::default();
    layout.ensure_descriptor(&descriptor, false);
    assert!(layout.set_width(&id, 333));
    assert!(layout.set_visible(&id, true));
    assert!(layout.move_before(&id, Some(&ColumnId::Size)));
    let retained = layout.entry(&id).cloned().unwrap();

    // Package removal revokes only the descriptor; the layout preference is retained.
    let generation = registry.generation();
    assert_eq!(registry.unregister_package("org.example.folder-size"), 1);
    assert!(registry.generation() > generation);
    assert_eq!(layout.entry(&id).cloned(), Some(retained.clone()));
    assert_eq!(layout.visible_registered(&registry).count(), 4);

    // Re-registering the exact stable ID makes the retained preference effective again.
    registry
        .replace_package("org.example.folder-size", [descriptor])
        .unwrap();
    assert_eq!(layout.entry(&id).cloned(), Some(retained));
    assert_eq!(layout.visible_registered(&registry).count(), 5);
}

#[test]
fn replace_package_is_atomic_and_namespaces_same_local_column_ids() {
    let mut registry = ColumnRegistry::built_ins();
    let first = ColumnId::extension("org.example.one", "bytes").unwrap();
    let second = ColumnId::extension("org.example.two", "bytes").unwrap();
    registry
        .replace_package("org.example.one", [extension_descriptor(first.clone())])
        .unwrap();
    let generation = registry.generation();
    let invalid = ColumnId::Extension {
        package_id: "org.example.one".to_owned(),
        column_id: "bad:name".to_owned(),
    };
    assert!(
        registry
            .replace_package(
                "org.example.one",
                [
                    extension_descriptor(first.clone()),
                    extension_descriptor(invalid)
                ]
            )
            .is_err()
    );
    assert_eq!(registry.generation(), generation);
    assert!(registry.contains(&first));
    registry
        .replace_package("org.example.two", [extension_descriptor(second.clone())])
        .unwrap();
    assert!(registry.contains(&first));
    assert!(registry.contains(&second));
}

#[test]
fn legacy_runtime_migration_preserves_custom_prefix_and_appends_built_ins() {
    let persisted = PersistedViewSettings {
        details_column_order: vec![
            PersistedColumn::Size,
            PersistedColumn::Name,
            PersistedColumn::Title,
            PersistedColumn::Type,
        ],
        details_columns: PersistedColumnWidths {
            name: 311,
            date_modified: 177,
            item_type: 166,
            size: 233,
            date_created: 144,
            authors: 155,
            tags: 166,
            title: 277,
        },
        details_column_visibility: 8 | 128, // Name is forced visible by migration.
        ..PersistedViewSettings::default()
    };
    let runtime = persisted.to_runtime();
    let ids: Vec<_> = runtime
        .details_layout
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    assert_eq!(
        &ids[..4],
        &[
            ColumnId::Size,
            ColumnId::Name,
            ColumnId::Title,
            ColumnId::Type
        ]
    );
    assert_eq!(
        &ids[4..],
        &[
            ColumnId::DateModified,
            ColumnId::DateCreated,
            ColumnId::Authors,
            ColumnId::Tags
        ]
    );
    assert_eq!(runtime.details_layout.width(&ColumnId::Size), Some(233));
    assert_eq!(runtime.details_layout.width(&ColumnId::Title), Some(277));
    assert!(runtime.details_layout.visible(&ColumnId::Name));
    assert!(runtime.details_layout.visible(&ColumnId::Size));
    assert!(runtime.details_layout.visible(&ColumnId::Title));
    assert!(!runtime.details_layout.visible(&ColumnId::Type));
}
