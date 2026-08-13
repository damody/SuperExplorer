//! Public-SDK Rust tokei Code lines column consumer.
//!
//! The provider receives only host-attested basenames and bounded input
//! streams. It never opens a path or starts a child process.

use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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
const MAX_DIRECTORY_PACK_BYTES: usize = 64 * 1024 * 1024;
const DIRECTORY_MAGIC_V1: &[u8; 8] = b"SECLDIR1";
#[cfg(test)]
const CACHE_SCHEMA_VERSION: u32 = 2;
#[cfg(test)]
const CACHE_MAX_RECORD_BYTES: u64 = 8 * 1024;
#[cfg(test)]
const CACHE_MAX_FILES: usize = 256;

struct TokeiRegistrar;
struct TokeiCodeLinesProvider;

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CodeLinesPayload {
    blanks: u64,
    code: u64,
    comments: u64,
    language: String,
    total: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[cfg(test)]
struct CacheRecord {
    schema: u32,
    identity: String,
    modified_seconds: u64,
    modified_nanos: u32,
    source_size: u64,
    value: CodeLinesPayload,
}

#[cfg(test)]
impl CacheRecord {
    fn matches(
        &self,
        identity: &str,
        modified_seconds: u64,
        modified_nanos: u32,
        source_size: u64,
    ) -> bool {
        self.schema == CACHE_SCHEMA_VERSION
            && self.identity == identity
            && self.modified_seconds == modified_seconds
            && self.modified_nanos == modified_nanos
            && self.source_size == source_size
    }
}

#[cfg(test)]
fn cache_directory() -> Option<PathBuf> {
    env::var_os("RUST_TOKEI_CODE_LINES_CACHE")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("LOCALAPPDATA")
                .or_else(|| env::var_os("APPDATA"))
                .map(|root| {
                    PathBuf::from(root)
                        .join("RustGpuiExplorer")
                        .join("cache")
                        .join("code-lines")
                        .join("rust-tokei-code-lines-column")
                        .join("v1")
                })
        })
}

#[cfg(test)]
fn persistent_hash(input: &str) -> u64 {
    input
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[cfg(test)]
fn cache_path(directory: &Path, identity: &str) -> PathBuf {
    directory.join(format!(
        "{:016x}.code-lines-cache",
        persistent_hash(identity)
    ))
}

#[cfg(test)]
fn prune_cache(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".code-lines-cache")
        })
        .map(|entry| {
            (
                entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .unwrap_or(UNIX_EPOCH),
                entry.path(),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let excess = entries
        .len()
        .saturating_sub(CACHE_MAX_FILES.saturating_sub(1));
    for (_, path) in entries.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
fn cache_facts(item: &explorer_extension_api::BatchColumnItemV1) -> Option<(&str, u64, u32, u64)> {
    let modified = item.modified_unix_seconds.into_option()?;
    let size = item.source_size.into_option()?;
    (!item.cache_identity.is_empty()).then_some((
        item.cache_identity.as_str(),
        modified,
        item.modified_subsec_nanos,
        size,
    ))
}

#[cfg(test)]
fn read_cache(item: &explorer_extension_api::BatchColumnItemV1) -> Option<CodeLinesPayload> {
    let (identity, modified_seconds, modified_nanos, source_size) = cache_facts(item)?;
    let path = cache_path(&cache_directory()?, identity);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > CACHE_MAX_RECORD_BYTES
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .ok()?
        .take(CACHE_MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > CACHE_MAX_RECORD_BYTES {
        return None;
    }
    let record: CacheRecord = serde_json::from_slice(&bytes).ok()?;
    record
        .matches(identity, modified_seconds, modified_nanos, source_size)
        .then_some(record.value)
}

#[cfg(test)]
fn store_cache(item: &explorer_extension_api::BatchColumnItemV1, value: &CodeLinesPayload) {
    let Some((identity, modified_seconds, modified_nanos, source_size)) = cache_facts(item) else {
        return;
    };
    let Some(directory) = cache_directory() else {
        return;
    };
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    prune_cache(&directory);
    let record = CacheRecord {
        schema: CACHE_SCHEMA_VERSION,
        identity: identity.to_owned(),
        modified_seconds,
        modified_nanos,
        source_size,
        value: value.clone(),
    };
    let Ok(bytes) = serde_json::to_vec(&record) else {
        return;
    };
    if bytes.len() as u64 > CACHE_MAX_RECORD_BYTES {
        return;
    }
    let destination = cache_path(&directory, identity);
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        persistent_hash(identity),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    if fs::write(&temporary, bytes).is_ok() {
        #[cfg(windows)]
        {
            let _ = fs::remove_file(&destination);
        }
        if fs::rename(&temporary, &destination).is_err() {
            let _ = fs::remove_file(temporary);
        }
    }
}

fn plugin_value(payload: &CodeLinesPayload) -> Option<PluginValueV1> {
    let json = format!(
        "{{\"blanks\":{},\"code\":{},\"comments\":{},\"language\":{},\"total\":{}}}",
        payload.blanks,
        payload.code,
        payload.comments,
        serde_json::to_string(&payload.language).ok()?,
        payload.total
    );
    PluginValueV1::structured_canonical_json(RVec::from(json.into_bytes())).ok()
}

fn read_stream(input: &explorer_extension_api::InputStreamV1) -> Option<Vec<u8>> {
    let length = input.length();
    if length.status != InputStreamStatusV1::OK || length.length as usize > MAX_DIRECTORY_PACK_BYTES
    {
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
        if bytes.len() > MAX_DIRECTORY_PACK_BYTES {
            return None;
        }
    }
    Some(bytes)
}

fn classify(file_name: &str, bytes: &[u8]) -> Option<(String, tokei::CodeStats)> {
    if bytes.starts_with(DIRECTORY_MAGIC_V1) {
        return classify_directory_pack(bytes);
    }
    if bytes.len() > MAX_FILE_BYTES {
        return None;
    }
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

fn classify_directory_pack(bytes: &[u8]) -> Option<(String, tokei::CodeStats)> {
    let mut cursor = DIRECTORY_MAGIC_V1.len();
    let mut by_language = BTreeMap::<String, tokei::CodeStats>::new();
    while cursor < bytes.len() {
        let name_end = cursor.checked_add(4)?;
        let name_len = usize::try_from(u32::from_le_bytes(
            bytes.get(cursor..name_end)?.try_into().ok()?,
        ))
        .ok()?;
        cursor = name_end;
        let length_end = cursor.checked_add(8)?;
        let data_len = usize::try_from(u64::from_le_bytes(
            bytes.get(cursor..length_end)?.try_into().ok()?,
        ))
        .ok()?;
        cursor = length_end;
        let name_end = cursor.checked_add(name_len)?;
        let name = std::str::from_utf8(bytes.get(cursor..name_end)?).ok()?;
        cursor = name_end;
        let data_end = cursor.checked_add(data_len)?;
        let data = bytes.get(cursor..data_end)?;
        cursor = data_end;
        if let Some((language, stats)) = classify(name, data) {
            *by_language
                .entry(language)
                .or_insert_with(tokei::CodeStats::new) += stats;
        }
    }
    let mut main_language: Option<(String, tokei::CodeStats)> = None;
    for (language, stats) in by_language {
        let replace = main_language
            .as_ref()
            .is_none_or(|(current_language, current)| {
                stats.code > current.code
                    || (stats.code == current.code && language < *current_language)
            });
        if replace {
            main_language = Some((language, stats));
        }
    }
    main_language
}

fn format_grouped_decimal(value: u64) -> String {
    let digits = value.to_string();
    let first_group = digits.len() % 3;
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    let mut cursor = 0;
    if first_group != 0 {
        formatted.push_str(&digits[..first_group]);
        cursor = first_group;
    }
    while cursor < digits.len() {
        if !formatted.is_empty() {
            formatted.push(',');
        }
        formatted.push_str(&digits[cursor..cursor + 3]);
        cursor += 3;
    }
    formatted
}

fn payload(value: &PluginValueV1) -> Option<CodeLinesPayload> {
    (value.kind == explorer_extension_api::PluginValueKindV1::STRUCTURED)
        .then(|| serde_json::from_slice(value.payload.as_slice()).ok())?
}

impl VisualColumnImplementationV1 for TokeiCodeLinesProvider {
    fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
        FolderSizeMeasureResultV1::partial(0, "code-lines is a file column")
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        let muted = context.theme.muted_foreground;
        if let Some(error) = context.error.clone().into_option() {
            return CellRenderPlanV1::text_only(error, muted);
        }
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
            label: RString::from(format!(
                "{}: {}",
                value.language,
                format_grouped_decimal(value.code)
            )),
            detail,
            proportional_bar_millionths: 0,
            text_color: if context.selected {
                context.theme.selection_background
            } else {
                context.theme.foreground
            },
            bar_color: CellColorV1::rgba(0, 0, 0, 0),
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
            // The host performs persistent lookup/admission before dispatch;
            // plugins always compute misses and never choose cache policy.
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
                    let payload = CodeLinesPayload {
                        blanks: stats.blanks as u64,
                        code: stats.code as u64,
                        comments: stats.comments as u64,
                        language,
                        total: stats.lines() as u64,
                    };
                    plugin_value(&payload).map(|value| {
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
                    folder_admission: ROption::RSome(
                        explorer_extension_api::FolderAdmissionPolicyV1 {
                            max_file_count: ROption::RSome(999),
                            max_folder_count: ROption::RNone,
                        },
                    ),
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: ROption::RNone,
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
                    folder_admission: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(
                        explorer_extension_api::VisualColumnObjectV1::new(TokeiCodeLinesProvider),
                    ),
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: ROption::RNone,
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
        let policy = contribution
            .folder_admission
            .expect("Code Lines declares its Host admission limit");
        assert_eq!(policy.max_file_count, ROption::RSome(999));
        assert_eq!(policy.max_folder_count, ROption::RNone);
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
    fn directory_pack_aggregates_supported_source_files() {
        let mut packed = DIRECTORY_MAGIC_V1.to_vec();
        for (name, source) in [
            ("src/main.rs", b"fn main() {}\n// comment\n".as_slice()),
            ("src/lib.rs", b"pub fn helper() {}\n\n".as_slice()),
            ("script.py", b"print('ok')\n\n".as_slice()),
        ] {
            packed.extend_from_slice(&(name.len() as u32).to_le_bytes());
            packed.extend_from_slice(&(source.len() as u64).to_le_bytes());
            packed.extend_from_slice(name.as_bytes());
            packed.extend_from_slice(source);
        }
        let (language, stats) = classify("folder", &packed).expect("folder aggregate");
        assert_eq!(language, "Rust");
        assert_eq!(stats.code, 2);
        assert_eq!(stats.comments, 1);
        assert_eq!(stats.blanks, 1);
    }

    #[test]
    fn directory_pack_selects_language_sum_not_largest_file() {
        let mut packed = DIRECTORY_MAGIC_V1.to_vec();
        for (name, source) in [
            ("a.rs", b"fn a() {}\n".as_slice()),
            ("b.rs", b"fn b() {}\n".as_slice()),
            ("c.rs", b"fn c() {}\n".as_slice()),
            ("main.py", b"one = 1\ntwo = 2\n".as_slice()),
        ] {
            packed.extend_from_slice(&(name.len() as u32).to_le_bytes());
            packed.extend_from_slice(&(source.len() as u64).to_le_bytes());
            packed.extend_from_slice(name.as_bytes());
            packed.extend_from_slice(source);
        }
        let (language, stats) = classify("folder", &packed).expect("folder aggregate");
        assert_eq!(language, "Rust");
        assert_eq!(stats.code, 3);
    }

    #[test]
    fn directory_pack_breaks_equal_code_ties_by_language_name() {
        let mut packed = DIRECTORY_MAGIC_V1.to_vec();
        for (name, source) in [
            ("main.rs", b"fn main() {}\n".as_slice()),
            ("main.py", b"print('ok')\n".as_slice()),
        ] {
            packed.extend_from_slice(&(name.len() as u32).to_le_bytes());
            packed.extend_from_slice(&(source.len() as u64).to_le_bytes());
            packed.extend_from_slice(name.as_bytes());
            packed.extend_from_slice(source);
        }
        assert_eq!(classify("folder", &packed).unwrap().0, "Python");
        assert!(classify("folder", DIRECTORY_MAGIC_V1).is_none());
        assert!(classify("folder", b"SECLDIR1\x01").is_none());
    }

    #[test]
    fn renderer_draws_exact_label_without_bar_and_optional_detail() {
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
        assert_eq!(plan.label, "Rust: 25");
        assert_eq!(plan.proportional_bar_millionths, 0);
        assert_eq!(plan.bar_color.alpha, 0);
        assert!(plan.detail.contains("Rust"));
    }

    #[test]
    fn grouped_decimal_formatting_covers_boundaries() {
        for (value, expected) in [
            (0, "0"),
            (999, "999"),
            (1_000, "1,000"),
            (1_250, "1,250"),
            (1_000_000, "1,000,000"),
        ] {
            assert_eq!(format_grouped_decimal(value), expected);
        }
    }

    #[test]
    fn nul_bearing_known_extension_is_unsupported_not_zero() {
        assert!(classify("binary.rs", b"fn main() {}\0").is_none());
    }

    #[test]
    fn persistent_cache_requires_unchanged_identity_mtime_and_size() {
        let record = CacheRecord {
            schema: CACHE_SCHEMA_VERSION,
            identity: "opaque-1".into(),
            modified_seconds: 10,
            modified_nanos: 20,
            source_size: 30,
            value: CodeLinesPayload {
                blanks: 1,
                code: 2,
                comments: 3,
                language: "Rust".into(),
                total: 6,
            },
        };
        assert!(record.matches("opaque-1", 10, 20, 30));
        assert!(!record.matches("opaque-1", 11, 20, 30));
        assert!(!record.matches("opaque-1", 10, 20, 31));
        assert!(!record.matches("opaque-2", 10, 20, 30));
    }
}
