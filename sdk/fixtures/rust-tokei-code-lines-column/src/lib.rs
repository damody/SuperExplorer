//! Public-SDK Rust tokei Code lines column consumer.
//!
//! The provider receives only host-attested basenames and bounded input
//! streams. It never opens a path or starts a child process.

use std::path::Path;

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    AbiErrorCodeV1, AbiErrorV1, BatchColumnContextV1, BatchColumnProviderImplementationV1,
    BatchColumnProviderObjectV1, ExtensionRegistrarImplementationV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, IncrementalResultBatchV1, IncrementalResultEntryV1,
    InputStreamReadRequestV1, InputStreamStatusV1, JobTerminalV1, PluginItemOutcomeV1,
    PluginItemResultV1, PluginMetadataV1, PluginValueV1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1, RegistrarOutputV1, RegistrationOutcomeV1,
    StableIdV1, StableSortValueV1, ABI_SCHEMA_V1, EXTENSION_ID_NAMESPACE_V1,
    MAX_INPUT_STREAM_READ_BYTES_V1, ROOT_MODULE_CONTRACT_ID_V1, SDK_MAJOR_VERSION_V1,
};
use explorer_extension_ui_api::{
    CellColorV1, CellRenderContextV1, CellRenderPlanV1, FolderSizeMeasureRequestV1,
    FolderSizeMeasureResultV1, VisualColumnImplementationV1,
};

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 3_001);
const PRIMARY_INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 3_002);
const FEATURE_ID: &str = "rust-tokei";
const CONTRIBUTION_ID: &str = "rust-tokei:code-lines";
const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;

struct TokeiRegistrar;
struct TokeiCodeLinesProvider;

#[derive(serde::Deserialize)]
struct CodeLinesPayload {
    blanks: u64,
    code: u64,
    comments: u64,
    language: String,
    total: u64,
}

fn read_stream(input: &explorer_extension_api::InputStreamV1) -> Option<Vec<u8>> {
    let length = input.length();
    if length.status != InputStreamStatusV1::OK || length.length as usize > MAX_FILE_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(length.length as usize);
    loop {
        let chunk = input.read(InputStreamReadRequestV1 {
            maximum_bytes: MAX_INPUT_STREAM_READ_BYTES_V1,
            reserved: 0,
        });
        if chunk.status == InputStreamStatusV1::EOF {
            break;
        }
        if chunk.status != InputStreamStatusV1::OK || chunk.data.is_empty() {
            return None;
        }
        bytes.extend_from_slice(&chunk.data);
        if bytes.len() > MAX_FILE_BYTES {
            return None;
        }
    }
    Some(bytes)
}

fn classify(file_name: &str, bytes: &[u8]) -> Option<(String, tokei::CodeStats)> {
    // UTF-8 alone does not make a stream source code: an arbitrary binary can
    // contain a NUL while still being valid UTF-8. Keep the public outcome
    // truthful by returning Unsupported rather than a misleading zero count.
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return None;
    }
    let config = tokei::Config::default();
    let language = tokei::LanguageType::from_path(Path::new(file_name), &config)?;
    let stats = language.parse_from_slice(bytes, &config).summarise();
    Some((language.name().to_owned(), stats))
}

fn json_value(language: &str, stats: &tokei::CodeStats) -> Option<PluginValueV1> {
    // Canonical object order is required by the public transport validator.
    let json = format!(
        "{{\"blanks\":{},\"code\":{},\"comments\":{},\"language\":\"{}\",\"total\":{}}}",
        stats.blanks,
        stats.code,
        stats.comments,
        language,
        stats.lines()
    );
    PluginValueV1::structured_canonical_json(RVec::from(json.into_bytes())).ok()
}

fn payload(value: &PluginValueV1) -> Option<CodeLinesPayload> {
    (value.kind == explorer_extension_api::PluginValueKindV1::STRUCTURED)
        .then(|| serde_json::from_slice(value.payload.as_slice()).ok())?
}

fn code_from_value(value: &PluginValueV1) -> Option<u64> {
    payload(value).map(|value| value.code)
}

impl VisualColumnImplementationV1 for TokeiCodeLinesProvider {
    fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
        FolderSizeMeasureResultV1::partial(0, "code-lines is a file column")
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        let muted = context.theme.muted_foreground;
        let Some(value) = context
            .value
            .into_option()
            .and_then(|value| payload(&value))
        else {
            return CellRenderPlanV1::text_only(
                if context.loading { "Loading…" } else { "—" },
                muted,
            );
        };
        let largest = context
            .aggregate
            .into_option()
            .and_then(|aggregate| aggregate.largest_sibling_value.into_option())
            .and_then(|value| code_from_value(&value))
            .unwrap_or(value.code);
        let proportional = if largest == 0 {
            0
        } else {
            u32::try_from((u128::from(value.code) * 1_000_000) / u128::from(largest))
                .unwrap_or(1_000_000)
                .min(1_000_000)
        };
        // The host owns the setting token. It deliberately carries no private
        // UI state across the ABI boundary.
        let detail = if context.settings.contains("with-detail") {
            RString::from(format!(
                "{} · {} comments · {} blanks · {} total",
                value.language, value.comments, value.blanks, value.total
            ))
        } else {
            RString::new()
        };
        CellRenderPlanV1 {
            label: RString::from(value.code.to_string()),
            detail,
            proportional_bar_millionths: proportional,
            text_color: if context.selected {
                context.theme.selection_background
            } else {
                context.theme.foreground
            },
            bar_color: CellColorV1::rgba(
                context.theme.accent.red,
                context.theme.accent.green,
                context.theme.accent.blue,
                128,
            ),
        }
    }
}

impl BatchColumnProviderImplementationV1 for TokeiCodeLinesProvider {
    fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
        if !context.is_well_formed() {
            return JobTerminalV1::INCOMPATIBLE;
        }
        let mut entries = Vec::with_capacity(context.items.len());
        for item in &context.items {
            if context.poll_control().into_raw() != 1 {
                return JobTerminalV1::CANCELLED;
            }
            let Some(bytes) = read_stream(&item.input) else {
                entries.push(IncrementalResultEntryV1 {
                    item: item.item,
                    item_generation: item.item_generation,
                    source_generation: context.source_generation,
                    result: PluginItemResultV1::absent(PluginItemOutcomeV1::UNSUPPORTED),
                });
                continue;
            };
            let result = classify(item.file_name.as_str(), &bytes)
                .and_then(|(language, stats)| {
                    json_value(&language, &stats).map(|value| {
                        PluginItemResultV1::value(
                            value,
                            ROption::RSome(StableSortValueV1::unsigned(stats.code as u64)),
                        )
                    })
                })
                .unwrap_or_else(|| PluginItemResultV1::absent(PluginItemOutcomeV1::UNSUPPORTED));
            entries.push(IncrementalResultEntryV1 {
                item: item.item,
                item_generation: item.item_generation,
                source_generation: context.source_generation,
                result,
            });
        }
        let outcome = context.try_submit(IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            entries: RVec::from(entries),
        });
        match outcome.status.into_raw() {
            1 => JobTerminalV1::COMPLETED,
            2 => JobTerminalV1::BACKPRESSURED,
            3 | 4 => JobTerminalV1::CANCELLED,
            _ => JobTerminalV1::PLUGIN_ERROR,
        }
    }
}

impl ExtensionRegistrarImplementationV1 for TokeiRegistrar {
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
            outcome: RegistrationOutcomeV1::accepted(2),
            contributions: RVec::from(vec![
                RegisteredContributionV1 {
                    feature_id: RString::from(FEATURE_ID),
                    contribution_id: RString::from(CONTRIBUTION_ID),
                    kind: RegisteredContributionKindV1::COLUMN,
                    required_capabilities: RVec::from(vec![
                        RString::from("abi"),
                        RString::from("filesystem.read"),
                    ]),
                    interface_id: PRIMARY_INTERFACE_ID,
                    expected_sort: ROption::RSome(
                        explorer_extension_api::StableSortValueKindV1::U64,
                    ),
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RSome(RString::from(
                        "rust-tokei:code-lines-renderer",
                    )),
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    batch_column_provider: ROption::RSome(BatchColumnProviderObjectV1::new(
                        TokeiCodeLinesProvider,
                    )),
                },
                RegisteredContributionV1 {
                    feature_id: RString::from(FEATURE_ID),
                    contribution_id: RString::from("rust-tokei:code-lines-renderer"),
                    kind: RegisteredContributionKindV1::GPUI_RENDERER,
                    required_capabilities: RVec::from(vec![RString::from("abi")]),
                    interface_id: PRIMARY_INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(
                        explorer_extension_api::VisualColumnObjectV1::new(TokeiCodeLinesProvider),
                    ),
                    size_map_view: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                },
            ]),
        })
    }
}

#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<TokeiRegistrar>(
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
    use super::*;
    use explorer_extension_api::registrar_request_v1;

    #[test]
    fn mixed_language_stats_are_exact_and_unsupported_is_not_zero() {
        let cases = [
            (
                "main.rs",
                b"fn main() {}\n// comment\n\n".as_slice(),
                "Rust",
                (1, 1, 1, 3),
            ),
            (
                "native.cpp",
                b"int main() {}\n// comment\n\n".as_slice(),
                "C++",
                (1, 1, 1, 3),
            ),
            (
                "script.py",
                b"def main():\n    pass\n# comment\n\n".as_slice(),
                "Python",
                (2, 1, 1, 4),
            ),
            (
                "script.lua",
                b"local value = 1\n-- comment\n\n".as_slice(),
                "Lua",
                (1, 1, 1, 3),
            ),
            (
                "script.js",
                b"function main() {}\n// comment\n\n".as_slice(),
                "JavaScript",
                (1, 1, 1, 3),
            ),
        ];
        for (name, source, expected_language, expected) in cases {
            let (language, stats) = classify(name, source).expect("supported source");
            assert_eq!(language, expected_language, "{name}");
            assert_eq!(
                (stats.code, stats.comments, stats.blanks, stats.lines()),
                expected,
                "{name}"
            );
        }
        for (name, source) in [
            ("unknown.data", b"plain text\n".as_slice()),
            ("binary.rs", b"fn main() {}\0".as_slice()),
        ] {
            assert!(classify(name, source).is_none(), "{name}");
        }
    }

    #[test]
    fn registrar_exposes_one_u64_sorted_batch_column() {
        let output = plugin_root()
            .create_registrar()
            .create()
            .into_result()
            .unwrap()
            .register(registrar_request_v1())
            .into_result()
            .unwrap();
        assert_eq!(output.outcome, RegistrationOutcomeV1::accepted(2));
        let contribution = &output.contributions[0];
        assert_eq!(contribution.kind, RegisteredContributionKindV1::COLUMN);
        assert_eq!(
            contribution.expected_sort.unwrap(),
            explorer_extension_api::StableSortValueKindV1::U64
        );
        assert!(matches!(
            contribution.batch_column_provider,
            ROption::RSome(_)
        ));
        assert!(matches!(contribution.visual_column, ROption::RNone));
        assert_eq!(
            output.contributions[1].kind,
            RegisteredContributionKindV1::GPUI_RENDERER
        );
        assert!(matches!(
            output.contributions[1].visual_column,
            ROption::RSome(_)
        ));
    }

    #[test]
    fn renderer_draws_exact_label_bar_and_optional_detail() {
        let value = PluginValueV1::structured_canonical_json(RVec::from(
            br#"{"blanks":1,"code":25,"comments":2,"language":"Rust","total":28}"#.to_vec(),
        ))
        .unwrap();
        let aggregate = PluginValueV1::structured_canonical_json(RVec::from(
            br#"{"blanks":1,"code":100,"comments":2,"language":"Rust","total":103}"#.to_vec(),
        ))
        .unwrap();
        let plan = TokeiCodeLinesProvider.render(CellRenderContextV1 {
            value: ROption::RSome(value),
            exact_bytes: ROption::RNone,
            aggregate: ROption::RSome(explorer_extension_api::CellAggregateV1 {
                largest_sibling_value: ROption::RSome(aggregate),
                largest_sibling_bytes: ROption::RNone,
            }),
            loading: false,
            error: ROption::RNone,
            selected: false,
            hovered: false,
            dpi_milli: 1_000,
            theme: explorer_extension_api::CellThemeV1 {
                foreground: CellColorV1::rgba(1, 2, 3, 255),
                muted_foreground: CellColorV1::rgba(4, 5, 6, 255),
                background: CellColorV1::rgba(7, 8, 9, 255),
                selection_background: CellColorV1::rgba(10, 11, 12, 255),
                accent: CellColorV1::rgba(13, 14, 15, 255),
            },
            settings: RString::from("with-detail"),
            item_id: explorer_extension_api::StableIdV1::new(
                explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                1,
            ),
            render_generation: 1,
            request_generation: 1,
        });
        assert_eq!(plan.label, "25");
        assert_eq!(plan.proportional_bar_millionths, 250_000);
        assert!(plan.detail.contains("Rust"));
    }

    #[test]
    fn nul_bearing_known_extension_is_unsupported_not_zero() {
        assert!(classify("binary.rs", b"fn main() {}\0").is_none());
    }
}
