//! Standalone public-SDK Size Map consumer.
//!
//! This example deliberately uses one ordinary Rust view-renderer trait.  The
//! host performs all I/O and gives this callback only copied node snapshots;
//! this crate returns data-only, normalized treemap rectangles.

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
            return SizeMapRenderPlanV1::empty(
                context.generation,
                context.render_revision,
                "Folder is empty",
            );
        }
        let exact_total = visible.iter().fold(0_u64, |total, node| {
            if node.status == SizeMapNodeStatusV1::COMPLETE {
                total.saturating_add(node.exact_bytes.unwrap_or_default())
            } else {
                total
            }
        });
        let layout_total = visible.iter().fold(0_u64, |total, node| {
            let weight = if node.status == SizeMapNodeStatusV1::COMPLETE {
                node.exact_bytes.unwrap_or_default().max(1)
            } else {
                1
            };
            total.saturating_add(weight)
        });

        let mut used_width = 0_u32;
        let visible_len = visible.len();
        let rectangles = visible
            .into_iter()
            .enumerate()
            .map(|(index, node)| {
                let bytes = node.exact_bytes.unwrap_or_default();
                let weight = if node.status == SizeMapNodeStatusV1::COMPLETE {
                    bytes.max(1)
                } else {
                    1
                };
                let width = if index + 1 == visible_len {
                    1_000_000_u32.saturating_sub(used_width)
                } else {
                    let proportional = (u128::from(weight) * 1_000_000) / u128::from(layout_total);
                    u32::try_from(proportional).unwrap_or(u32::MAX)
                };
                let color = color_for(
                    node.kind,
                    node.status,
                    context.theme.accent,
                    context.theme.muted_foreground,
                );
                let detail = if node.status == SizeMapNodeStatusV1::COMPLETE {
                    let percentage = if exact_total == 0 {
                        0.0
                    } else {
                        (bytes as f64 * 100.0) / exact_total as f64
                    };
                    format!(
                        "{} · {percentage:.1}% · {} · complete",
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
                let rectangle = SizeMapRectangleV1 {
                    node_id: node.node_id,
                    x_millionths: used_width,
                    y_millionths: 0,
                    width_millionths: width,
                    height_millionths: 1_000_000,
                    color,
                    label: node.name.clone(),
                    detail: RString::from(detail),
                };
                used_width = used_width.saturating_add(width);
                rectangle
            })
            .collect::<Vec<_>>();

        SizeMapRenderPlanV1 {
            generation: context.generation,
            render_revision: context.render_revision,
            rectangles: RVec::from(rectangles),
            status: RString::from("Exact sizes"),
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
    use explorer_extension_ui_api::{CellThemeV1, SizeMapNodeV1, SizeMapViewportV1};

    use super::*;

    fn color() -> CellColorV1 {
        CellColorV1::rgba(1, 2, 3, 255)
    }

    #[test]
    fn public_trait_returns_proportional_data_only_treemap() {
        let color = color();
        let plan = FolderSizeMapView.render_size_map(SizeMapRenderContextV1 {
            generation: 7,
            render_revision: 71,
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
        assert_eq!(plan.generation, 7);
        assert_eq!(plan.rectangles.len(), 2);
        assert_eq!(plan.rectangles[0].width_millionths, 750_000);
        assert_eq!(plan.rectangles[1].width_millionths, 250_000);
    }

    #[test]
    fn unavailable_nodes_remain_visible_with_status() {
        let color = color();
        let plan = FolderSizeMapView.render_size_map(SizeMapRenderContextV1 {
            generation: 8,
            render_revision: 81,
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
