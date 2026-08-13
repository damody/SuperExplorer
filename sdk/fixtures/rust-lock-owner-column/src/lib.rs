//! Discover-only Lock Owner column using the public SuperExplorer SDK.

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString},
};
use explorer_extension_api::{
    AbiErrorCodeV1, AbiErrorV1, BatchColumnContextV1, BatchColumnProviderImplementationV1,
    BatchColumnProviderObjectV1, ExtensionRegistrarImplementationV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, IncrementalResultBatchV1, IncrementalResultEntryV1, JobTerminalV1,
    LockOwnerQueryRequestV1, LockOwnerQueryStatusV1, PluginItemOutcomeV1, PluginItemResultV1,
    PluginMetadataV1, PluginValueV1, RegisteredContributionKindV1, RegisteredContributionV1,
    RegistrarOutputResultV1, RegistrarOutputV1, RegistrationOutcomeV1, StableIdV1,
    StableSortValueV1, ABI_SCHEMA_V1, EXTENSION_ID_NAMESPACE_V1, ROOT_MODULE_CONTRACT_ID_V1,
};
use explorer_extension_ui_api::{
    CellColorV1, CellRenderContextV1, CellRenderPlanV1, FolderSizeMeasureRequestV1,
    FolderSizeMeasureResultV1, VisualColumnImplementationV1,
};

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 4_001);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 4_002);
const FEATURE_ID: &str = "rust-lock-owner";
const COLUMN_ID: &str = "rust-lock-owner:owners";
const RENDERER_ID: &str = "rust-lock-owner:owners-renderer";

struct Registrar;
struct Provider;

#[cfg(debug_assertions)]
fn delay_for_stale_generation_smoke() {
    let milliseconds = std::env::var("EXPLORER_LOCK_OWNER_TEST_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
        .min(5_000);
    if milliseconds != 0 {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}

#[cfg(not(debug_assertions))]
fn delay_for_stale_generation_smoke() {}

fn escaped(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn empty_owner_outcome(status: LockOwnerQueryStatusV1) -> Option<PluginItemOutcomeV1> {
    if status == LockOwnerQueryStatusV1::EMPTY || status == LockOwnerQueryStatusV1::READY {
        None
    } else if status == LockOwnerQueryStatusV1::UNAVAILABLE {
        Some(PluginItemOutcomeV1::UNAVAILABLE)
    } else if status == LockOwnerQueryStatusV1::HOST_ERROR {
        Some(PluginItemOutcomeV1::PLUGIN_ERROR)
    } else {
        Some(PluginItemOutcomeV1::CANCELLED)
    }
}

impl BatchColumnProviderImplementationV1 for Provider {
    fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
        // Debug-only deterministic scheduling hook used by the final headful
        // stale-generation smoke. Release packages never consult this value.
        delay_for_stale_generation_smoke();
        let Some(service) = context.lock_owner_query.clone().into_option() else {
            return JobTerminalV1::INCOMPATIBLE;
        };
        let outcome = service.query(LockOwnerQueryRequestV1 {
            items: context
                .items
                .iter()
                .map(|item| item.item)
                .collect::<Vec<_>>()
                .into(),
            item_generation: context.item_generation,
            location_generation: context.location_generation,
            deadline_millis: 5_000,
            reserved: 0,
        });
        if outcome.item_generation != context.item_generation
            || outcome.location_generation != context.location_generation
        {
            return JobTerminalV1::CANCELLED;
        }
        if outcome.status == LockOwnerQueryStatusV1::CANCELLED
            || outcome.status == LockOwnerQueryStatusV1::DEADLINE_ELAPSED
        {
            return JobTerminalV1::CANCELLED;
        }
        let mut entries = Vec::with_capacity(context.items.len());
        for item in &context.items {
            let owners = outcome
                .owners
                .iter()
                .filter(|owner| owner.item == item.item)
                .collect::<Vec<_>>();
            let result = if owners.is_empty() {
                if let Some(absent) = empty_owner_outcome(outcome.status) {
                    PluginItemResultV1::absent(absent)
                } else {
                    PluginItemResultV1::value(
                        PluginValueV1::structured_canonical_json(
                            b"{\"count\":0,\"details\":\"\",\"names\":\"\"}".to_vec(),
                        )
                        .unwrap(),
                        ROption::RSome(StableSortValueV1::unsigned(0)),
                    )
                }
            } else {
                let names = owners
                    .iter()
                    .map(|owner| owner.display_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let details = owners
                    .iter()
                    .map(|owner| format!("{} (PID {})", owner.display_name, owner.process_id))
                    .collect::<Vec<_>>()
                    .join("; ");
                let json = format!(
                    "{{\"count\":{},\"details\":\"{}\",\"names\":\"{}\"}}",
                    owners.len(),
                    escaped(&details),
                    escaped(&names)
                );
                match PluginValueV1::structured_canonical_json(json.into_bytes()) {
                    Ok(value) => PluginItemResultV1::value(
                        value,
                        ROption::RSome(StableSortValueV1::unsigned(owners.len() as u64)),
                    ),
                    Err(_) => PluginItemResultV1::absent(PluginItemOutcomeV1::PLUGIN_ERROR),
                }
            };
            entries.push(IncrementalResultEntryV1 {
                item: item.item,
                item_generation: item.item_generation,
                source_generation: context.source_generation,
                result,
            });
        }
        let submitted = context.try_submit(IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            entries: entries.into(),
        });
        if submitted.status.into_raw() == 1 {
            JobTerminalV1::COMPLETED
        } else {
            JobTerminalV1::PLUGIN_ERROR
        }
    }
}

impl VisualColumnImplementationV1 for Provider {
    fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
        FolderSizeMeasureResultV1::partial(0, "lock-owner is a file column")
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        let Some(value) = context.value.into_option() else {
            return CellRenderPlanV1::text_only("", context.theme.muted_foreground);
        };
        let Ok(text) = std::str::from_utf8(&value.payload) else {
            return CellRenderPlanV1::text_only("Unavailable", context.theme.muted_foreground);
        };
        let field = |name: &str| {
            text.split(&format!("\"{name}\":\""))
                .nth(1)
                .and_then(|tail| tail.split('"').next())
                .unwrap_or("")
        };
        let names = if field("names").is_empty() {
            field("language")
        } else {
            field("names")
        };
        CellRenderPlanV1 {
            label: RString::from(names),
            detail: RString::from(field("details")),
            proportional_bar_millionths: 0,
            text_color: context.theme.foreground,
            bar_color: CellColorV1::rgba(0, 0, 0, 0),
        }
    }
}

impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }

    fn register(
        &self,
        request: explorer_extension_api::RegistrarRequestV1,
    ) -> RegistrarOutputResultV1 {
        if request.abi_schema != ABI_SCHEMA_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SCHEMA_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                request.abi_schema.into_raw(),
            ));
        }
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(2),
            contributions: vec![
                RegisteredContributionV1 {
                    feature_id: FEATURE_ID.into(),
                    contribution_id: COLUMN_ID.into(),
                    kind: RegisteredContributionKindV1::COLUMN,
                    required_capabilities: vec![
                        "abi".into(),
                        "filesystem.read".into(),
                        "lock-owner.query".into(),
                    ]
                    .into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RSome(
                        explorer_extension_api::StableSortValueKindV1::U64,
                    ),
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RSome(RENDERER_ID.into()),
                    folder_admission: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: ROption::RNone,
                    batch_column_provider: ROption::RSome(BatchColumnProviderObjectV1::new(
                        Provider,
                    )),
                },
                RegisteredContributionV1 {
                    feature_id: FEATURE_ID.into(),
                    contribution_id: RENDERER_ID.into(),
                    kind: RegisteredContributionKindV1::GPUI_RENDERER,
                    required_capabilities: vec!["abi".into()].into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    folder_admission: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(
                        explorer_extension_ui_api::VisualColumnObjectV1::new(Provider),
                    ),
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                },
            ]
            .into(),
        })
    }
}

#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<Registrar>(
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: INTERFACE_ID,
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use super::{empty_owner_outcome, LockOwnerQueryRequestV1};
    #[test]
    fn request_is_bounded_and_generation_scoped() {
        assert_eq!(explorer_extension_api::MAX_LOCK_OWNER_QUERY_ITEMS_V1, 128);
        let request = LockOwnerQueryRequestV1 {
            items: Vec::new().into(),
            item_generation: 7,
            location_generation: 9,
            deadline_millis: 250,
            reserved: 0,
        };
        assert_eq!(
            (request.item_generation, request.location_generation),
            (7, 9)
        );
    }

    #[test]
    fn empty_denied_and_adapter_fault_remain_distinct() {
        assert_eq!(
            empty_owner_outcome(explorer_extension_api::LockOwnerQueryStatusV1::EMPTY),
            None
        );
        assert_eq!(
            empty_owner_outcome(explorer_extension_api::LockOwnerQueryStatusV1::UNAVAILABLE),
            Some(explorer_extension_api::PluginItemOutcomeV1::UNAVAILABLE)
        );
        assert_eq!(
            empty_owner_outcome(explorer_extension_api::LockOwnerQueryStatusV1::HOST_ERROR),
            Some(explorer_extension_api::PluginItemOutcomeV1::PLUGIN_ERROR)
        );
    }
}
