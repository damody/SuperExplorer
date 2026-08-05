//! Standalone public-SDK Size Map consumer.
//!
//! This example deliberately uses one ordinary Rust view-renderer trait.  The
//! host performs all I/O and gives this callback only copied node snapshots;
//! this crate returns data-only, normalized treemap rectangles.

use std::collections::HashMap;

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    AbiErrorCodeV1, AbiErrorV1, ExtensionRegistrarImplementationV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, PluginMetadataV1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1, RegistrarOutputV1, RegistrarRequestV1,
    RegistrationOutcomeV1, StableIdV1, ABI_SCHEMA_V1, EXTENSION_ID_NAMESPACE_V1,
    ROOT_MODULE_CONTRACT_ID_V1, SDK_MAJOR_VERSION_V1,
};
use explorer_extension_ui_api::{
    CellColorV1, SizeMapNodeKindV1, SizeMapNodeStatusV1, SizeMapRectangleV1,
    SizeMapRenderContextV1, SizeMapRenderPlanV1, SizeMapViewImplementationV1, SizeMapViewObjectV1,
};

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 2_001);
const PRIMARY_INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 2_002);

struct FolderSizeMapRegistrar;
struct FolderSizeMapView;

impl SizeMapViewImplementationV1 for FolderSizeMapView {
    fn render_size_map(&self, context: SizeMapRenderContextV1) -> SizeMapRenderPlanV1 {
        let visible = context.nodes.iter().collect::<Vec<_>>();
        if visible.is_empty() {
            return SizeMapRenderPlanV1::empty(context.snapshot, "Folder is empty");
        }
        let exact_total = visible
            .iter()
            .filter(|node| node.parent_id.is_none())
            .filter(|node| node.status == SizeMapNodeStatusV1::COMPLETE)
            .fold(0_u64, |total, node| {
                total.saturating_add(node.exact_bytes.unwrap_or_default())
            });
        let mut children =
            HashMap::<Option<StableIdV1>, Vec<&explorer_extension_ui_api::SizeMapNodeV1>>::new();
        for node in visible {
            children
                .entry(node.parent_id.into_option())
                .or_default()
                .push(node);
        }
        for siblings in children.values_mut() {
            siblings.sort_by(|left, right| {
                right
                    .exact_bytes
                    .unwrap_or_default()
                    .cmp(&left.exact_bytes.unwrap_or_default())
                    .then_with(|| left.name.cmp(&right.name))
            });
        }
        let mut rectangles = Vec::with_capacity(context.nodes.len());
        layout_siblings(
            &children,
            None,
            BoundsV1 {
                x: 0,
                y: 0,
                width: 1_000_000,
                height: 1_000_000,
            },
            0,
            exact_total,
            context.theme.accent,
            context.theme.muted_foreground,
            &mut rectangles,
        );

        let all_exact = context
            .nodes
            .iter()
            .all(|node| node.status == SizeMapNodeStatusV1::COMPLETE);
        SizeMapRenderPlanV1 {
            snapshot: context.snapshot,
            rectangles: RVec::from(rectangles),
            status: RString::from(if all_exact {
                "Exact sizes"
            } else {
                "Calculating sizes"
            }),
        }
    }
}

#[derive(Clone, Copy)]
struct BoundsV1 {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[allow(clippy::too_many_arguments)]
fn layout_siblings(
    children: &HashMap<Option<StableIdV1>, Vec<&explorer_extension_ui_api::SizeMapNodeV1>>,
    parent: Option<StableIdV1>,
    bounds: BoundsV1,
    depth: usize,
    exact_total: u64,
    accent: CellColorV1,
    muted: CellColorV1,
    output: &mut Vec<SizeMapRectangleV1>,
) {
    let Some(siblings) = children.get(&parent) else {
        return;
    };
    let total = siblings.iter().fold(0_u64, |total, node| {
        total.saturating_add(node.exact_bytes.unwrap_or_default().max(1))
    });
    let horizontal = depth.is_multiple_of(2);
    let extent = if horizontal {
        bounds.width
    } else {
        bounds.height
    };
    let mut used = 0_u32;
    for (index, node) in siblings.iter().enumerate() {
        let weight = node.exact_bytes.unwrap_or_default().max(1);
        let length = if index + 1 == siblings.len() {
            extent.saturating_sub(used)
        } else {
            u32::try_from((u128::from(weight) * u128::from(extent)) / u128::from(total))
                .unwrap_or(extent)
        };
        let node_bounds = if horizontal {
            BoundsV1 {
                x: bounds.x.saturating_add(used),
                width: length,
                ..bounds
            }
        } else {
            BoundsV1 {
                y: bounds.y.saturating_add(used),
                height: length,
                ..bounds
            }
        };
        used = used.saturating_add(length);
        let bytes = node.exact_bytes.unwrap_or_default();
        let detail = if node.status == SizeMapNodeStatusV1::COMPLETE {
            let percentage = if exact_total == 0 {
                0.0
            } else {
                (bytes as f64 * 100.0) / exact_total as f64
            };
            format!(
                "{bytes} bytes · {} · {percentage:.1}% · {} · complete",
                compact_bytes(bytes),
                kind_label(node.kind)
            )
        } else {
            format!(
                "size unavailable · {} · {}",
                kind_label(node.kind),
                status_label(node.status)
            )
        };
        output.push(SizeMapRectangleV1 {
            node_id: node.node_id,
            x_millionths: node_bounds.x,
            y_millionths: node_bounds.y,
            width_millionths: node_bounds.width,
            height_millionths: node_bounds.height,
            color: color_for(node.kind, node.status, accent, muted),
            label: node.name.clone(),
            detail: RString::from(detail),
        });
        if children.contains_key(&Some(node.node_id)) {
            let inset = node_bounds.width.min(node_bounds.height).min(8_000);
            if node_bounds.width > inset.saturating_mul(2)
                && node_bounds.height > inset.saturating_mul(2)
            {
                layout_siblings(
                    children,
                    Some(node.node_id),
                    BoundsV1 {
                        x: node_bounds.x.saturating_add(inset),
                        y: node_bounds.y.saturating_add(inset),
                        width: node_bounds.width.saturating_sub(inset.saturating_mul(2)),
                        height: node_bounds.height.saturating_sub(inset.saturating_mul(2)),
                    },
                    depth.saturating_add(1),
                    exact_total,
                    accent,
                    muted,
                    output,
                );
            }
        }
    }
}

fn color_for(
    kind: SizeMapNodeKindV1,
    status: SizeMapNodeStatusV1,
    accent: CellColorV1,
    muted: CellColorV1,
) -> CellColorV1 {
    if status == SizeMapNodeStatusV1::FAILED {
        return CellColorV1::rgba(196, 43, 28, 224);
    }
    if status != SizeMapNodeStatusV1::COMPLETE {
        return CellColorV1::rgba(muted.red, muted.green, muted.blue, 176);
    }
    if kind == SizeMapNodeKindV1::DIRECTORY {
        return accent;
    }
    if kind == SizeMapNodeKindV1::FILE {
        return CellColorV1::rgba(accent.blue, accent.red, accent.green, 208);
    }
    muted
}

fn status_label(status: SizeMapNodeStatusV1) -> &'static str {
    if status == SizeMapNodeStatusV1::PARTIAL {
        "partial"
    } else if status == SizeMapNodeStatusV1::FAILED {
        "failed"
    } else if status == SizeMapNodeStatusV1::COMPLETE {
        "complete"
    } else if status == SizeMapNodeStatusV1::CANCELLED {
        "cancelled"
    } else if status == SizeMapNodeStatusV1::RESOURCE_LIMITED {
        "resource limited"
    } else {
        "unavailable"
    }
}

fn kind_label(kind: SizeMapNodeKindV1) -> &'static str {
    if kind == SizeMapNodeKindV1::DIRECTORY {
        "folder"
    } else if kind == SizeMapNodeKindV1::FILE {
        "file"
    } else {
        "other"
    }
}

fn compact_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

impl ExtensionRegistrarImplementationV1 for FolderSizeMapRegistrar {
    fn create() -> Self {
        Self
    }

    fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        if request.abi_schema != ABI_SCHEMA_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SCHEMA_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                request.abi_schema.into_raw(),
            ));
        }
        if request.root_contract_id != ROOT_MODULE_CONTRACT_ID_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::UNSUPPORTED_ID,
                request.root_contract_id,
                0,
            ));
        }
        if request.sdk_major != SDK_MAJOR_VERSION_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                u32::from(request.sdk_major),
            ));
        }

        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(1),
            contributions: RVec::from(vec![RegisteredContributionV1 {
                feature_id: RString::from("size-map"),
                contribution_id: RString::from("size-map"),
                kind: RegisteredContributionKindV1::VIEW_MODE,
                required_capabilities: RVec::from(vec![RString::from("abi")]),
                interface_id: PRIMARY_INTERFACE_ID,
                expected_sort: ROption::RNone,
                opaque_contract: ROption::RNone,
                renderer_contribution_id: ROption::RNone,
                provider: ROption::RNone,
                visual_column: ROption::RNone,
                size_map_view: ROption::RSome(SizeMapViewObjectV1::new(FolderSizeMapView)),
                virtual_folder_provider: ROption::RNone,
                batch_column_provider: ROption::RNone,
            }]),
        })
    }
}

/// The sole fixed SDK root module; plugin authors do not define callbacks.
#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<FolderSizeMapRegistrar>(
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: PRIMARY_INTERFACE_ID,
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use explorer_extension_api::registrar_request_v1;
    use explorer_extension_ui_api::{
        CellThemeV1, SizeMapNodeV1, SizeMapViewportV1, ViewSnapshotIdentityV1,
    };

    use super::*;

    fn color() -> CellColorV1 {
        CellColorV1::rgba(1, 2, 3, 255)
    }

    #[test]
    fn public_trait_returns_proportional_data_only_treemap() {
        let color = color();
        let plan = FolderSizeMapView.render_size_map(SizeMapRenderContextV1 {
            snapshot: ViewSnapshotIdentityV1 {
                location_generation: 7,
                refresh_generation: 7,
                render_revision: 71,
            },
            nodes: RVec::from(vec![
                SizeMapNodeV1 {
                    node_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 31),
                    parent_id: ROption::RNone,
                    name: RString::from("large"),
                    kind: SizeMapNodeKindV1::DIRECTORY,
                    exact_bytes: ROption::RSome(30),
                    status: SizeMapNodeStatusV1::COMPLETE,
                },
                SizeMapNodeV1 {
                    node_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 32),
                    parent_id: ROption::RNone,
                    name: RString::from("small"),
                    kind: SizeMapNodeKindV1::FILE,
                    exact_bytes: ROption::RSome(10),
                    status: SizeMapNodeStatusV1::COMPLETE,
                },
            ]),
            viewport: SizeMapViewportV1 {
                width_milli: 1_000,
                height_milli: 1_000,
                dpi_milli: 1_000,
            },
            theme: CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            selected_node_ids: RVec::new(),
            settings: RString::new(),
        });
        assert_eq!(plan.snapshot.location_generation, 7);
        assert_eq!(plan.rectangles.len(), 2);
        assert_eq!(plan.rectangles[0].width_millionths, 750_000);
        assert_eq!(plan.rectangles[1].width_millionths, 250_000);
        assert!(plan.rectangles[0].detail.contains("30 bytes"));
        assert_eq!(plan.status.as_str(), "Exact sizes");
    }

    #[test]
    fn unavailable_nodes_remain_visible_with_status() {
        let color = color();
        let plan = FolderSizeMapView.render_size_map(SizeMapRenderContextV1 {
            snapshot: ViewSnapshotIdentityV1 {
                location_generation: 8,
                refresh_generation: 8,
                render_revision: 81,
            },
            nodes: RVec::from(vec![SizeMapNodeV1 {
                node_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 41),
                parent_id: ROption::RNone,
                name: RString::from("denied"),
                kind: SizeMapNodeKindV1::DIRECTORY,
                exact_bytes: ROption::RNone,
                status: SizeMapNodeStatusV1::FAILED,
            }]),
            viewport: SizeMapViewportV1 {
                width_milli: 1_000,
                height_milli: 1_000,
                dpi_milli: 1_000,
            },
            theme: CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            selected_node_ids: RVec::new(),
            settings: RString::new(),
        });
        assert_eq!(plan.rectangles.len(), 1);
        assert!(plan.rectangles[0].detail.contains("failed"));
        assert!(plan.rectangles[0].detail.contains("size unavailable"));
        assert_eq!(plan.status.as_str(), "Calculating sizes");
    }

    #[test]
    fn aggregated_other_remains_a_named_exact_accessibility_record() {
        let color = color();
        let plan = FolderSizeMapView.render_size_map(SizeMapRenderContextV1 {
            snapshot: ViewSnapshotIdentityV1 {
                location_generation: 9,
                refresh_generation: 9,
                render_revision: 91,
            },
            nodes: RVec::from(vec![SizeMapNodeV1 {
                node_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 51),
                parent_id: ROption::RNone,
                name: RString::from("Other (300 items)"),
                kind: SizeMapNodeKindV1::OTHER,
                exact_bytes: ROption::RSome(4096),
                status: SizeMapNodeStatusV1::COMPLETE,
            }]),
            viewport: SizeMapViewportV1 {
                width_milli: 1_000,
                height_milli: 1_000,
                dpi_milli: 1_000,
            },
            theme: CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            selected_node_ids: RVec::new(),
            settings: RString::new(),
        });
        assert_eq!(plan.rectangles.len(), 1);
        assert_eq!(plan.rectangles[0].label.as_str(), "Other (300 items)");
        assert!(plan.rectangles[0].detail.contains("4096 bytes"));
        assert!(plan.rectangles[0].detail.contains("other"));
        assert_eq!(plan.status.as_str(), "Exact sizes");
    }

    #[test]
    fn child_rectangles_are_nested_inside_their_parent() {
        let color = color();
        let parent = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 61);
        let child = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 62);
        let plan = FolderSizeMapView.render_size_map(SizeMapRenderContextV1 {
            snapshot: ViewSnapshotIdentityV1 {
                location_generation: 10,
                refresh_generation: 10,
                render_revision: 101,
            },
            nodes: RVec::from(vec![
                SizeMapNodeV1 {
                    node_id: parent,
                    parent_id: ROption::RNone,
                    name: "parent".into(),
                    kind: SizeMapNodeKindV1::DIRECTORY,
                    exact_bytes: ROption::RSome(20),
                    status: SizeMapNodeStatusV1::COMPLETE,
                },
                SizeMapNodeV1 {
                    node_id: child,
                    parent_id: ROption::RSome(parent),
                    name: "child.rs".into(),
                    kind: SizeMapNodeKindV1::FILE,
                    exact_bytes: ROption::RSome(20),
                    status: SizeMapNodeStatusV1::COMPLETE,
                },
            ]),
            viewport: SizeMapViewportV1 {
                width_milli: 1_000,
                height_milli: 1_000,
                dpi_milli: 1_000,
            },
            theme: CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            selected_node_ids: RVec::new(),
            settings: RString::new(),
        });
        let parent_rect = plan
            .rectangles
            .iter()
            .find(|rectangle| rectangle.node_id == parent)
            .expect("parent rectangle");
        let child_rect = plan
            .rectangles
            .iter()
            .find(|rectangle| rectangle.node_id == child)
            .expect("child rectangle");
        assert!(child_rect.x_millionths > parent_rect.x_millionths);
        assert!(child_rect.y_millionths > parent_rect.y_millionths);
        assert!(child_rect.width_millionths < parent_rect.width_millionths);
        assert!(child_rect.height_millionths < parent_rect.height_millionths);
    }

    #[test]
    fn root_registers_one_public_view_renderer() {
        let registrar = plugin_root()
            .create_registrar()
            .create()
            .into_result()
            .unwrap();
        let output = registrar
            .register(registrar_request_v1())
            .into_result()
            .unwrap();
        assert_eq!(output.outcome, RegistrationOutcomeV1::accepted(1));
        assert_eq!(
            output.contributions[0].kind,
            RegisteredContributionKindV1::VIEW_MODE
        );
        assert!(matches!(
            output.contributions[0].size_map_view,
            ROption::RSome(_)
        ));
    }
}
