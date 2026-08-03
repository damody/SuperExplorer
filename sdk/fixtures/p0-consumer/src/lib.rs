//! Standalone P0 consumer using the public Rust-first extension author API.
//!
//! The fixture is a minimal folder-size visual-column example. It proves that a
//! clean consumer can export the SDK root and implement ordinary Rust measure
//! and renderer traits without declaring FFI callbacks, GPUI types, or root
//! layout.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    AbiErrorCodeV1, AbiErrorV1, ExtensionRegistrarImplementationV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, PluginMetadataV1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1, RegistrarOutputV1, RegistrarRequestV1,
    RegistrationOutcomeV1, StableIdV1, StableSortValueKindV1, ABI_SCHEMA_V1,
    EXTENSION_ID_NAMESPACE_V1, ROOT_MODULE_CONTRACT_ID_V1, SDK_MAJOR_VERSION_V1,
};
use explorer_extension_ui_api::{
    CellColorV1, CellRenderContextV1, CellRenderPlanV1, FolderSizeMeasureRequestV1,
    FolderSizeMeasureResultV1, VisualColumnImplementationV1, VisualColumnObjectV1,
};

const MARKER_ENVIRONMENT_VARIABLE: &str = "P0_CONSUMER_REGISTRAR_MARKER";
const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1_001);
const PRIMARY_INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1_002);

struct P0ConsumerRegistrar;

struct FolderSizeMeasureColumn;

impl VisualColumnImplementationV1 for FolderSizeMeasureColumn {
    fn measure_folder_size(
        &self,
        request: FolderSizeMeasureRequestV1,
    ) -> FolderSizeMeasureResultV1 {
        let (exact_bytes, partial_error) = measure_path_bytes(&request);
        match partial_error {
            Some(error) => FolderSizeMeasureResultV1::partial(exact_bytes, error),
            None => FolderSizeMeasureResultV1::complete(exact_bytes),
        }
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        CellRenderPlanV1::text_only("Folder size is measuring", context.theme.muted_foreground)
    }
}

struct FolderSizeRenderer;

impl VisualColumnImplementationV1 for FolderSizeRenderer {
    fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
        FolderSizeMeasureResultV1::partial(0, "renderer does not measure folders")
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        let CellRenderContextV1 {
            exact_bytes,
            aggregate,
            loading,
            error,
            selected,
            hovered,
            theme,
            settings,
            ..
        } = context;
        if loading {
            return CellRenderPlanV1::text_only("Calculating...", theme.muted_foreground);
        }
        if let ROption::RSome(error) = error {
            return CellRenderPlanV1::text_only(error, theme.muted_foreground);
        }
        let Some(exact_bytes) = exact_bytes.into_option() else {
            return CellRenderPlanV1::text_only("", theme.muted_foreground);
        };
        let mut plan =
            CellRenderPlanV1::text_only(compact_byte_label(exact_bytes), theme.foreground);
        plan.detail = RString::from("folder total");
        plan.bar_color = if selected {
            theme.selection_background
        } else if hovered {
            theme.accent
        } else {
            CellColorV1::rgba(theme.accent.red, theme.accent.green, theme.accent.blue, 128)
        };
        if settings.as_str() != "text-only" {
            if let ROption::RSome(aggregate) = aggregate {
                if let ROption::RSome(largest_bytes) = aggregate.largest_sibling_bytes {
                    if largest_bytes != 0 {
                        let fraction = (u128::from(exact_bytes.min(largest_bytes)) * 1_000_000)
                            / u128::from(largest_bytes);
                        plan.set_proportional_bar_millionths(
                            u32::try_from(fraction).unwrap_or(u32::MAX),
                        );
                    }
                }
            }
        }
        plan
    }
}

fn measure_path_bytes(request: &FolderSizeMeasureRequestV1) -> (u64, Option<RString>) {
    let started = std::time::Instant::now();
    let deadline = std::time::Duration::from_millis(u64::from(request.deadline_millis.max(1)));
    let max_entries = request.max_entries.max(1);
    let mut visited = 0_u32;
    let mut total = 0_u64;
    let mut partial_error = None;
    let mut pending = vec![(
        Path::new(request.filesystem_path.as_str()).to_path_buf(),
        0_u16,
    )];

    while let Some((path, depth)) = pending.pop() {
        if visited >= max_entries {
            partial_error = Some(RString::from("folder measurement entry limit reached"));
            break;
        }
        if started.elapsed() >= deadline {
            partial_error = Some(RString::from("folder measurement time limit reached"));
            break;
        }
        visited = visited.saturating_add(1);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                partial_error.get_or_insert_with(|| RString::from(error.to_string()));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        if depth >= request.max_depth {
            partial_error
                .get_or_insert_with(|| RString::from("folder measurement depth limit reached"));
            continue;
        }
        match fs::read_dir(&path) {
            Ok(entries) => {
                for entry in entries {
                    let queued = u32::try_from(pending.len()).unwrap_or(u32::MAX);
                    if visited.saturating_add(queued) >= max_entries {
                        partial_error =
                            Some(RString::from("folder measurement entry limit reached"));
                        break;
                    }
                    if started.elapsed() >= deadline {
                        partial_error =
                            Some(RString::from("folder measurement time limit reached"));
                        break;
                    }
                    match entry {
                        Ok(entry) => pending.push((entry.path(), depth.saturating_add(1))),
                        Err(error) => {
                            partial_error.get_or_insert_with(|| RString::from(error.to_string()));
                        }
                    }
                }
            }
            Err(error) => {
                partial_error.get_or_insert_with(|| RString::from(error.to_string()));
            }
        }
    }
    (total, partial_error)
}

fn compact_byte_label(bytes: u64) -> RString {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        RString::from(format!("{bytes} B"))
    } else {
        RString::from(format!("{value:.1} {}", UNITS[unit]))
    }
}

impl ExtensionRegistrarImplementationV1 for P0ConsumerRegistrar {
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

        // Exercise the exact direct registry dependency patched to the private
        // vendor tree. This proves the source snapshot keeps its private,
        // provenance-bound dependency available to Cargo offline.
        let _ = exif_lite::parser_name();
        if let Err(error) = mark_callback_invocation() {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::REGISTRATION_REJECTED,
                ROOT_MODULE_CONTRACT_ID_V1,
                error.len() as u32,
            ));
        }

        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(2),
            // NativeExtensionLifecycleV1 admits only non-empty output whose
            // accepted count matches the contribution batch exactly. This is
            // deliberately bound to the fixture manifest's `main` feature
            // and its declared `abi` capability.
            contributions: RVec::from(vec![
                RegisteredContributionV1 {
                    feature_id: RString::from("main"),
                    contribution_id: RString::from("folder-size"),
                    kind: RegisteredContributionKindV1::COLUMN,
                    required_capabilities: RVec::from(vec![RString::from("abi")]),
                    interface_id: PRIMARY_INTERFACE_ID,
                    expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RSome(RString::from("folder-size-renderer")),
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(VisualColumnObjectV1::new(
                        FolderSizeMeasureColumn,
                    )),
                    size_map_view: ROption::RNone,
                },
                RegisteredContributionV1 {
                    feature_id: RString::from("main"),
                    contribution_id: RString::from("folder-size-renderer"),
                    kind: RegisteredContributionKindV1::GPUI_RENDERER,
                    required_capabilities: RVec::from(vec![RString::from("abi")]),
                    interface_id: PRIMARY_INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(VisualColumnObjectV1::new(FolderSizeRenderer)),
                    size_map_view: ROption::RNone,
                },
            ]),
        })
    }
}

fn marker_path() -> Option<PathBuf> {
    env::var_os(MARKER_ENVIRONMENT_VARIABLE).map(PathBuf::from)
}

fn mark_callback_invocation() -> Result<(), RString> {
    let Some(path) = marker_path() else {
        return Ok(());
    };
    fs::write(&path, b"p0 consumer registrar invoked").map_err(|error| {
        RString::from(format!(
            "could not write P0 consumer registrar marker {}: {error}",
            path.display()
        ))
    })
}

/// The sole ABI root module. `abi_stable` exports its fixed loader symbol;
/// semantic identity is data in [`ExtensionRootModuleV1`], never an
/// author-configurable manifest string.
#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<P0ConsumerRegistrar>(
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
    use explorer_extension_api::{registrar_request_v1, AbiSchemaIdV1, IdNamespaceV1};

    use super::*;

    #[test]
    fn root_is_the_fixed_public_v1_contract() {
        let root = plugin_root();

        assert_eq!(root.abi_schema(), ABI_SCHEMA_V1);
        assert_eq!(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(root.sdk_major(), SDK_MAJOR_VERSION_V1);
        assert_eq!(root.metadata().plugin_id, PLUGIN_ID);
        assert_eq!(root.ui_abi_fingerprint_sha256(), ROption::RNone);
    }

    #[test]
    fn mismatched_root_contract_is_rejected_before_marker_write() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let request = RegistrarRequestV1 {
            root_contract_id: StableIdV1::new(IdNamespaceV1::new(0x1234, 1), 1),
            ..registrar_request_v1()
        };

        assert!(matches!(
            registrar.register(request).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::UNSUPPORTED_ID,
                ..
            })
        ));
    }

    #[test]
    fn matching_public_contract_calls_the_registrar() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let result = registrar
            .register(registrar_request_v1())
            .into_result()
            .unwrap();

        assert_eq!(result.outcome, RegistrationOutcomeV1::accepted(2));
        assert_eq!(result.contributions.len(), 2);
        let contribution = &result.contributions[0];
        assert_eq!(contribution.feature_id, "main");
        assert_eq!(contribution.contribution_id, "folder-size");
        assert_eq!(contribution.kind, RegisteredContributionKindV1::COLUMN);
        assert_eq!(contribution.required_capabilities.as_slice(), ["abi"]);
        assert!(matches!(contribution.visual_column, ROption::RSome(_)));
        assert_eq!(
            result.contributions[1].contribution_id,
            "folder-size-renderer"
        );
    }

    #[test]
    fn folder_measurement_is_typed_and_renderer_plans_a_proportional_bar() {
        let measure = FolderSizeMeasureColumn.measure_folder_size(FolderSizeMeasureRequestV1 {
            filesystem_path: RString::from("Z:\\superexplorer-p0-missing-folder"),
            max_entries: 10_000,
            max_depth: 64,
            deadline_millis: 1_000,
        });
        assert!(measure.partial);

        let color = CellColorV1::rgba(1, 2, 3, 255);
        let context = CellRenderContextV1 {
            value: ROption::RSome(explorer_extension_api::PluginValueV1::integer(10)),
            exact_bytes: ROption::RSome(10),
            aggregate: ROption::RSome(explorer_extension_ui_api::CellAggregateV1 {
                largest_sibling_value: ROption::RSome(
                    explorer_extension_api::PluginValueV1::integer(20),
                ),
                largest_sibling_bytes: ROption::RSome(20),
            }),
            loading: false,
            error: ROption::RNone,
            selected: false,
            hovered: false,
            dpi_milli: 1_000,
            theme: explorer_extension_ui_api::CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            settings: RString::from("bar-and-text"),
        };
        let plan = FolderSizeRenderer.render(context.clone());
        assert_eq!(plan.proportional_bar_millionths, 500_000);
        let text_only = FolderSizeRenderer.render(CellRenderContextV1 {
            settings: RString::from("text-only"),
            ..context
        });
        assert_eq!(text_only.proportional_bar_millionths, 0);
    }

    #[test]
    fn schema_mismatch_is_typed() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let request = RegistrarRequestV1 {
            abi_schema: AbiSchemaIdV1::new(0x5345, 2),
            ..registrar_request_v1()
        };

        assert!(matches!(
            registrar.register(request).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::SCHEMA_MISMATCH,
                ..
            })
        ));
    }
}
