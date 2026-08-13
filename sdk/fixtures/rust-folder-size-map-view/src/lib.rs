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
    ABI_SCHEMA_V1, AbiErrorCodeV1, AbiErrorV1, EXTENSION_ID_NAMESPACE_V1,
    ExtensionRegistrarImplementationV1, ExtensionRootModuleV1, ExtensionRootModuleV1_Ref,
    PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1, RegistrarOutputV1, RegistrarRequestV1,
    RegistrationOutcomeV1, SDK_MAJOR_VERSION_V1, StableIdV1,
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
        let title_height = normalized_pixels(28_000, context.viewport.height_milli, 20_000, 80_000);
        let gutter_x = normalized_pixels(4_000, context.viewport.width_milli, 3_000, 20_000);
        let gutter_y = normalized_pixels(4_000, context.viewport.height_milli, 3_000, 20_000);
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
            exact_total,
            context.theme.accent,
            context.theme.muted_foreground,
            None,
            title_height,
            gutter_x,
            gutter_y,
            &mut rectangles,
        );

        let all_exact = context
            .nodes
            .iter()
            .all(|node| node.status == SizeMapNodeStatusV1::COMPLETE);
        let calculating_method = context.settings.as_str();
        SizeMapRenderPlanV1 {
            snapshot: context.snapshot,
            rectangles: RVec::from(rectangles),
            status: RString::from(if all_exact {
                String::new()
            } else {
                format!("Calculating sizes · {calculating_method}")
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
    available_total: u64,
    accent: CellColorV1,
    muted: CellColorV1,
    palette_group: Option<usize>,
    title_height: u32,
    gutter_x: u32,
    gutter_y: u32,
    output: &mut Vec<SizeMapRectangleV1>,
) {
    let Some(siblings) = children.get(&parent) else {
        return;
    };
    let known_sum = siblings
        .iter()
        .filter_map(|node| node.exact_bytes.into_option())
        .fold(0_u64, u64::saturating_add);
    let unknown_count = siblings
        .iter()
        .filter(|node| node.exact_bytes.into_option().is_none())
        .count() as u64;
    let minimum_weight = siblings
        .iter()
        .filter_map(|node| node.exact_bytes.into_option())
        .max()
        .unwrap_or_default()
        / 500;
    let remaining = available_total.saturating_sub(known_sum);
    let unfinished_pool = if unknown_count == 0 {
        0
    } else if remaining > 0 {
        remaining
    } else {
        known_sum.max(1) / 3
    };
    let unknown_weight = unfinished_pool
        .checked_div(unknown_count)
        .unwrap_or(0)
        .max(1);
    let mut placements = Vec::with_capacity(siblings.len());
    layout_balanced(
        siblings,
        bounds,
        minimum_weight.max(1),
        unknown_weight.max(1),
        &mut placements,
    );
    for (index, (node, node_bounds)) in placements.into_iter().enumerate() {
        let palette_group = palette_group.unwrap_or(index);
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
            color: color_for(node.kind, node.status, accent, muted, palette_group, depth),
            label: node.name.clone(),
            detail: RString::from(detail),
        });
        if children.contains_key(&Some(node.node_id)) {
            let reserved_title = title_height.min(node_bounds.height / 3);
            if node_bounds.width > gutter_x.saturating_mul(2)
                && node_bounds.height > reserved_title.saturating_add(gutter_y)
            {
                layout_siblings(
                    children,
                    Some(node.node_id),
                    BoundsV1 {
                        x: node_bounds.x.saturating_add(gutter_x),
                        y: node_bounds.y.saturating_add(reserved_title),
                        width: node_bounds.width.saturating_sub(gutter_x.saturating_mul(2)),
                        height: node_bounds
                            .height
                            .saturating_sub(reserved_title)
                            .saturating_sub(gutter_y),
                    },
                    depth.saturating_add(1),
                    exact_total,
                    bytes,
                    accent,
                    muted,
                    Some(palette_group),
                    title_height,
                    gutter_x,
                    gutter_y,
                    output,
                );
            }
        }
    }
}

fn visual_weight(
    node: &explorer_extension_ui_api::SizeMapNodeV1,
    minimum_weight: u64,
    unknown_weight: u64,
) -> u64 {
    node.exact_bytes
        .into_option()
        .unwrap_or(unknown_weight)
        .max(minimum_weight)
}

fn layout_balanced<'a>(
    nodes: &[&'a explorer_extension_ui_api::SizeMapNodeV1],
    bounds: BoundsV1,
    minimum_weight: u64,
    unknown_weight: u64,
    output: &mut Vec<(&'a explorer_extension_ui_api::SizeMapNodeV1, BoundsV1)>,
) {
    if nodes.is_empty() || bounds.width == 0 || bounds.height == 0 {
        return;
    }
    if nodes.len() == 1 {
        output.push((nodes[0], bounds));
        return;
    }
    let total = nodes.iter().fold(0_u64, |sum, node| {
        sum.saturating_add(visual_weight(node, minimum_weight, unknown_weight))
    });
    let mut first_total = 0_u64;
    let mut split = 1_usize;
    let mut best_delta = u64::MAX;
    for index in 1..nodes.len() {
        first_total = first_total.saturating_add(visual_weight(
            nodes[index - 1],
            minimum_weight,
            unknown_weight,
        ));
        let delta = total.abs_diff(first_total.saturating_mul(2));
        if delta <= best_delta {
            best_delta = delta;
            split = index;
        } else {
            break;
        }
    }
    let first_total = nodes[..split].iter().fold(0_u64, |sum, node| {
        sum.saturating_add(visual_weight(node, minimum_weight, unknown_weight))
    });
    if bounds.width >= bounds.height {
        let first_width =
            u32::try_from((u128::from(bounds.width) * u128::from(first_total)) / u128::from(total))
                .unwrap_or(bounds.width)
                .clamp(1, bounds.width.saturating_sub(1).max(1));
        layout_balanced(
            &nodes[..split],
            BoundsV1 {
                width: first_width,
                ..bounds
            },
            minimum_weight,
            unknown_weight,
            output,
        );
        layout_balanced(
            &nodes[split..],
            BoundsV1 {
                x: bounds.x.saturating_add(first_width),
                width: bounds.width.saturating_sub(first_width),
                ..bounds
            },
            minimum_weight,
            unknown_weight,
            output,
        );
    } else {
        let first_height = u32::try_from(
            (u128::from(bounds.height) * u128::from(first_total)) / u128::from(total),
        )
        .unwrap_or(bounds.height)
        .clamp(1, bounds.height.saturating_sub(1).max(1));
        layout_balanced(
            &nodes[..split],
            BoundsV1 {
                height: first_height,
                ..bounds
            },
            minimum_weight,
            unknown_weight,
            output,
        );
        layout_balanced(
            &nodes[split..],
            BoundsV1 {
                y: bounds.y.saturating_add(first_height),
                height: bounds.height.saturating_sub(first_height),
                ..bounds
            },
            minimum_weight,
            unknown_weight,
            output,
        );
    }
}

fn normalized_pixels(
    logical_pixels_milli: u32,
    viewport_extent_milli: u32,
    minimum: u32,
    maximum: u32,
) -> u32 {
    if viewport_extent_milli == 0 {
        return maximum;
    }
    u32::try_from(
        (u128::from(logical_pixels_milli) * 1_000_000_u128) / u128::from(viewport_extent_milli),
    )
    .unwrap_or(maximum)
    .clamp(minimum, maximum)
}

fn color_for(
    kind: SizeMapNodeKindV1,
    status: SizeMapNodeStatusV1,
    accent: CellColorV1,
    muted: CellColorV1,
    palette_group: usize,
    depth: usize,
) -> CellColorV1 {
    if status == SizeMapNodeStatusV1::FAILED {
        return CellColorV1::rgba(196, 43, 28, 224);
    }
    if status != SizeMapNodeStatusV1::COMPLETE {
        return CellColorV1::rgba(muted.red, muted.green, muted.blue, 176);
    }
    const PALETTE: [(u8, u8, u8); 8] = [
        (55, 145, 222),
        (62, 153, 224),
        (48, 158, 213),
        (68, 164, 218),
        (57, 150, 205),
        (76, 169, 221),
        (47, 164, 200),
        (70, 157, 211),
    ];
    let (mut red, mut green, mut blue) = PALETTE[palette_group % PALETTE.len()];
    let depth_shift = (depth.min(7) as u8) * 12;
    red = red.saturating_add(depth_shift);
    green = green.saturating_add(depth_shift);
    blue = blue.saturating_add(depth_shift / 2);
    if kind == SizeMapNodeKindV1::FILE {
        red = red.saturating_add(12);
        green = green.saturating_add(12);
        blue = blue.saturating_add(12);
    }
    let _ = (accent, muted);
    CellColorV1::rgba(red, green, blue, 242)
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
                folder_admission: ROption::RNone,
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
    fn public_trait_tracks_exact_ratio_with_only_a_tiny_visibility_floor() {
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
                width_milli: 1_000_000,
                height_milli: 1_000_000,
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
        assert_eq!(plan.rectangles[0].height_millionths, 1_000_000);
        assert_eq!(plan.rectangles[1].height_millionths, 1_000_000);
        assert!(plan.rectangles[0].detail.contains("30 bytes"));
        assert!(plan.status.is_empty());
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
            settings: RString::from("Breadth-first fallback"),
        });
        assert_eq!(plan.rectangles.len(), 1);
        assert!(plan.rectangles[0].detail.contains("failed"));
        assert!(plan.rectangles[0].detail.contains("size unavailable"));
        assert_eq!(
            plan.status.as_str(),
            "Calculating sizes · Breadth-first fallback"
        );
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
        assert!(plan.status.is_empty());
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
                width_milli: 1_000_000,
                height_milli: 1_000_000,
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
        assert!(
            child_rect
                .y_millionths
                .saturating_sub(parent_rect.y_millionths)
                >= 28_000,
            "a nested map must reserve a 28 logical-pixel parent title band"
        );
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
