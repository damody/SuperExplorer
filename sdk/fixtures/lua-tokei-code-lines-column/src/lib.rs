//! Lua tokei example: package-attested ToolHandle, bounded batches, stable per-item mapping.
use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::*;
use std::{
    collections::HashMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const DIRECTORY_MAGIC_V1: &[u8; 8] = b"SECLDIR1";

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 6_101);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 6_102);
pub const MAX_BATCH: usize = 128;
pub const MAX_WINDOWS_ARGUMENT_CHARS: usize = 28_000;
#[cfg(test)]
const CACHE_SCHEMA_VERSION: u32 = 1;
#[cfg(test)]
const CACHE_MAX_RECORD_BYTES: u64 = 8 * 1024;
#[cfg(test)]
const CACHE_MAX_FILES: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CodeRow {
    pub path: String,
    pub code: u64,
    pub comments: u64,
    pub blanks: u64,
    pub total: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[cfg(test)]
struct CacheRecord {
    schema: u32,
    identity: String,
    modified_seconds: u64,
    modified_nanos: u32,
    source_size: u64,
    row: CodeRow,
}

#[cfg(test)]
fn cache_directory() -> Option<PathBuf> {
    env::var_os("LUA_TOKEI_CODE_LINES_CACHE")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("LOCALAPPDATA")
                .or_else(|| env::var_os("APPDATA"))
                .map(|root| {
                    PathBuf::from(root)
                        .join("RustGpuiExplorer")
                        .join("cache")
                        .join("code-lines")
                        .join("lua-tokei-code-lines-column")
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
fn file_facts(path: &str) -> Option<(String, u64, u32, u64)> {
    let canonical = fs::canonicalize(path).ok()?;
    let metadata = fs::metadata(&canonical).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some((
        canonical.to_string_lossy().into_owned(),
        modified.as_secs(),
        modified.subsec_nanos(),
        metadata.len(),
    ))
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
fn read_cache_from(directory: &Path, path: &str) -> Option<CodeRow> {
    // A directory's own mtime and size do not represent recursive descendant
    // contents. Reusing that identity can therefore publish a stale all-language
    // total after files are added or edited below it.
    if fs::symlink_metadata(path).ok()?.is_dir() {
        return None;
    }
    let (identity, modified_seconds, modified_nanos, source_size) = file_facts(path)?;
    let cache_path = cache_path(directory, &identity);
    let metadata = fs::symlink_metadata(&cache_path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > CACHE_MAX_RECORD_BYTES
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(cache_path)
        .ok()?
        .take(CACHE_MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let record: CacheRecord = serde_json::from_slice(&bytes).ok()?;
    if bytes.len() as u64 > CACHE_MAX_RECORD_BYTES
        || record.schema != CACHE_SCHEMA_VERSION
        || record.identity != identity
        || record.modified_seconds != modified_seconds
        || record.modified_nanos != modified_nanos
        || record.source_size != source_size
    {
        return None;
    }
    Some(CodeRow {
        path: path.to_owned(),
        ..record.row
    })
}

#[cfg(test)]
fn read_cache(path: &str) -> Option<CodeRow> {
    read_cache_from(&cache_directory()?, path)
}

#[cfg(test)]
fn store_cache_in(directory: &Path, path: &str, row: &CodeRow) {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
        return;
    }
    let Some((identity, modified_seconds, modified_nanos, source_size)) = file_facts(path) else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    prune_cache(directory);
    let record = CacheRecord {
        schema: CACHE_SCHEMA_VERSION,
        identity: identity.clone(),
        modified_seconds,
        modified_nanos,
        source_size,
        row: row.clone(),
    };
    let Ok(bytes) = serde_json::to_vec(&record) else {
        return;
    };
    if bytes.len() as u64 > CACHE_MAX_RECORD_BYTES {
        return;
    }
    let destination = cache_path(directory, &identity);
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        persistent_hash(&identity),
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

#[cfg(test)]
fn store_cache(path: &str, row: &CodeRow) {
    if let Some(directory) = cache_directory() {
        store_cache_in(&directory, path, row);
    }
}

pub fn parse_tokei_json(input: &str) -> Result<Vec<CodeRow>, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|error| error.to_string())?;
    let rows = value
        .as_array()
        .ok_or_else(|| "tool response is not an array".to_owned())?;
    rows.iter()
        .map(|row| {
            Ok(CodeRow {
                path: row
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "missing path".to_owned())?
                    .to_owned(),
                code: row
                    .get("code")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| "missing code".to_owned())?,
                comments: row
                    .get("comments")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| "missing comments".to_owned())?,
                blanks: row
                    .get("blanks")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| "missing blanks".to_owned())?,
                total: row
                    .get("total")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| "missing total".to_owned())?,
            })
        })
        .collect()
}

pub fn bounded_batches(paths: &[String]) -> Result<Vec<Vec<String>>, String> {
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut chars = 0;
    for path in paths {
        if path.contains('\0') {
            return Err("NUL path".into());
        }
        let cost = path.encode_utf16().count() + 3;
        if cost > MAX_WINDOWS_ARGUMENT_CHARS {
            return Err("single path exceeds command bound".into());
        }
        if batch.len() == MAX_BATCH || chars + cost > MAX_WINDOWS_ARGUMENT_CHARS {
            batches.push(std::mem::take(&mut batch));
            chars = 0;
        }
        chars += cost;
        batch.push(path.clone());
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    Ok(batches)
}

pub fn analyze_with_tool(handle: &ToolHandleV1, paths: &[String]) -> Result<Vec<CodeRow>, String> {
    // Persistent lookup/admission belongs to the host. This plugin receives
    // only cache misses and computes each requested path.
    let mut rows_by_path = HashMap::new();
    let misses = paths.to_vec();
    for batch in bounded_batches(&misses)? {
        let result = handle.execute(ToolExecuteRequestV1 {
            arguments: batch
                .iter()
                .cloned()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
            timeout_millis: 30_000,
            max_output_bytes: 8 * 1024 * 1024,
        });
        if result.status != ToolExecuteStatusV1::COMPLETED || result.exit_code != 0 {
            return Err(format!(
                "tokei tool failed with status {}",
                result.status.into_raw()
            ));
        }
        let text = std::str::from_utf8(&result.stdout).map_err(|error| error.to_string())?;
        let rows = parse_tokei_json(text.trim())?;
        if rows.len() != batch.len()
            || rows
                .iter()
                .zip(&batch)
                .any(|(row, expected)| &row.path != expected)
        {
            return Err("tool item mapping mismatch".into());
        }
        for row in rows {
            rows_by_path.insert(row.path.clone(), row);
        }
    }
    paths
        .iter()
        .map(|path| {
            rows_by_path
                .get(path)
                .cloned()
                .ok_or_else(|| "missing tool/cache row".to_owned())
        })
        .collect()
}

#[derive(Clone, Copy)]
struct LuaTokeiColumn;

fn read_host_input(input: &InputStreamV1) -> Option<Vec<u8>> {
    let length = input.length();
    if length.status != InputStreamStatusV1::OK || length.length > 8 * 1024 * 1024 {
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
    }
    Some(bytes)
}

fn count_source(file_name: &str, bytes: &[u8]) -> Option<CodeRow> {
    if bytes.contains(&0) {
        return None;
    }
    let language = match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "Rust",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "py" => "Python",
        "lua" => "Lua",
        "js" | "mjs" | "cjs" => "JavaScript",
        _ => return None,
    };
    let text = std::str::from_utf8(bytes).ok()?;
    let mut row = CodeRow {
        path: file_name.to_owned(),
        code: 0,
        comments: 0,
        blanks: 0,
        total: 0,
    };
    for line in text.lines() {
        row.total += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            row.blanks += 1;
        } else if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--")
        {
            row.comments += 1;
        } else {
            row.code += 1;
        }
    }
    let _ = language;
    Some(row)
}

fn count_input(file_name: &str, bytes: &[u8]) -> Option<CodeRow> {
    if !bytes.starts_with(DIRECTORY_MAGIC_V1) {
        return count_source(file_name, bytes);
    }
    let mut cursor = DIRECTORY_MAGIC_V1.len();
    let mut aggregate = CodeRow {
        path: file_name.to_owned(),
        code: 0,
        comments: 0,
        blanks: 0,
        total: 0,
    };
    let mut recognized = false;
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
        if let Some(row) = count_source(name, data) {
            recognized = true;
            aggregate.code = aggregate.code.saturating_add(row.code);
            aggregate.comments = aggregate.comments.saturating_add(row.comments);
            aggregate.blanks = aggregate.blanks.saturating_add(row.blanks);
            aggregate.total = aggregate.total.saturating_add(row.total);
        }
    }
    recognized.then_some(aggregate)
}

fn row_value(row: &CodeRow) -> Option<PluginValueV1> {
    let json = format!(
        "{{\"blanks\":{},\"code\":{},\"comments\":{},\"language\":\"Lua/tokei\",\"total\":{}}}",
        row.blanks, row.code, row.comments, row.total
    );
    PluginValueV1::structured_canonical_json(RVec::from(json.into_bytes())).ok()
}

impl BatchColumnProviderImplementationV1 for LuaTokeiColumn {
    fn run(&self, context: BatchColumnContextV1) -> JobTerminalV1 {
        if !context.is_well_formed() {
            return JobTerminalV1::INCOMPATIBLE;
        }
        let mut entries = Vec::with_capacity(context.items.len());
        for item in &context.items {
            let result = read_host_input(&item.input)
                .and_then(|bytes| count_input(item.file_name.as_str(), &bytes))
                .and_then(|row| {
                    let code = row.code;
                    row_value(&row).map(|value| {
                        PluginItemResultV1::value(
                            value,
                            ROption::RSome(StableSortValueV1::unsigned(code)),
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
        let submitted = context.try_submit(IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            entries: RVec::from(entries),
        });
        if submitted.status == SinkSubmitStatusV1::ACCEPTED {
            JobTerminalV1::COMPLETED
        } else {
            JobTerminalV1::CANCELLED
        }
    }
}

impl VisualColumnImplementationV1 for LuaTokeiColumn {
    fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
        FolderSizeMeasureResultV1::partial(0, "Code lines is a file column")
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        if let Some(error) = context.error.clone().into_option() {
            return CellRenderPlanV1::text_only(error, context.theme.muted_foreground);
        }
        let Some(value) = context.value.into_option() else {
            return CellRenderPlanV1::text_only(
                if context.loading { "Loading" } else { "—" },
                context.theme.muted_foreground,
            );
        };
        let parsed: serde_json::Value = match serde_json::from_slice(&value.payload) {
            Ok(value) => value,
            Err(_) => return CellRenderPlanV1::text_only("—", context.theme.muted_foreground),
        };
        let code = parsed
            .get("code")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let detail = if context.settings.contains("with-detail") {
            format!(
                "Lua/tokei · {} comments · {} blanks · {} total",
                parsed
                    .get("comments")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                parsed
                    .get("blanks")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                parsed
                    .get("total")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
            )
        } else {
            String::new()
        };
        CellRenderPlanV1 {
            label: RString::from(code.to_string()),
            detail: RString::from(detail),
            proportional_bar_millionths: 0,
            text_color: context.theme.foreground,
            bar_color: CellColorV1::rgba(0, 0, 0, 0),
        }
    }
}

struct Registrar;
impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(2),
            contributions: vec![
                RegisteredContributionV1 {
                    feature_id: "lua-tokei".into(),
                    contribution_id: "lua-tokei:column".into(),
                    kind: RegisteredContributionKindV1::COLUMN,
                    required_capabilities: vec![
                        "filesystem.read".into(),
                        "tools.execute_bundled".into(),
                    ]
                    .into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RSome("lua-tokei:renderer".into()),
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
                        LuaTokeiColumn,
                    )),
                },
                RegisteredContributionV1 {
                    feature_id: "lua-tokei".into(),
                    contribution_id: "lua-tokei:renderer".into(),
                    kind: RegisteredContributionKindV1::GPUI_RENDERER,
                    required_capabilities: vec!["abi".into()].into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    folder_admission: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(VisualColumnObjectV1::new(LuaTokeiColumn)),
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
    use super::*;
    use explorer_extension_api::registrar_request_v1;

    #[test]
    fn registrar_declares_the_999_file_folder_limit() {
        let output = plugin_root()
            .create_registrar()
            .create()
            .into_result()
            .unwrap()
            .register(registrar_request_v1())
            .into_result()
            .unwrap();
        let policy = output.contributions[0]
            .folder_admission
            .expect("Lua Code Lines declares its Host admission limit");
        assert_eq!(policy.max_file_count, ROption::RSome(999));
        assert_eq!(policy.max_folder_count, ROption::RNone);
    }

    #[test]
    fn maps_stable_rows() {
        let rows =
            parse_tokei_json(r#"[{"path":"a.rs","code":4,"comments":1,"blanks":2,"total":7}]"#)
                .unwrap();
        assert_eq!(
            rows[0],
            CodeRow {
                path: "a.rs".into(),
                code: 4,
                comments: 1,
                blanks: 2,
                total: 7
            }
        );
    }
    #[test]
    fn one_thousand_items_are_batched_not_spawned_per_item() {
        let paths = (0..1000)
            .map(|n| format!("C:/fixture/{n}.rs"))
            .collect::<Vec<_>>();
        let batches = bounded_batches(&paths).unwrap();
        assert_eq!(batches.len(), 8);
        assert!(batches.iter().all(|batch| batch.len() <= 128));
    }

    #[test]
    fn directory_pack_sums_every_recognized_language() {
        let mut packed = DIRECTORY_MAGIC_V1.to_vec();
        for (name, source) in [
            ("main.rs", "fn main() {}\n".repeat(1_250)),
            ("script.js", "const value = 1;\n".repeat(75)),
        ] {
            packed.extend_from_slice(&(name.len() as u32).to_le_bytes());
            packed.extend_from_slice(&(source.len() as u64).to_le_bytes());
            packed.extend_from_slice(name.as_bytes());
            packed.extend_from_slice(source.as_bytes());
        }
        let row = count_input("mixed-project", &packed).expect("directory aggregate");
        assert_eq!(row.code, 1_325);
        assert_eq!(row.total, 1_325);
    }

    #[test]
    fn persistent_cache_hits_until_file_metadata_changes() {
        let root = std::env::temp_dir().join(format!(
            "lua-tokei-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("sample.rs");
        let cache = root.join("cache");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, "fn main() {}\n").unwrap();
        let source_text = source.to_string_lossy().into_owned();
        let row = CodeRow {
            path: source_text.clone(),
            code: 1,
            comments: 0,
            blanks: 0,
            total: 1,
        };
        store_cache_in(&cache, &source_text, &row);
        assert_eq!(read_cache_from(&cache, &source_text), Some(row));
        fs::write(&source, "fn main() { println!(\"changed\"); }\n").unwrap();
        assert!(read_cache_from(&cache, &source_text).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn directory_totals_are_never_reused_from_non_recursive_metadata() {
        let root = std::env::temp_dir().join(format!(
            "lua-tokei-directory-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("mixed-project");
        let cache = root.join("cache");
        fs::create_dir_all(&source).unwrap();
        let source_text = source.to_string_lossy().into_owned();
        let row = CodeRow {
            path: source_text.clone(),
            code: 1_325,
            comments: 0,
            blanks: 0,
            total: 1_325,
        };
        store_cache_in(&cache, &source_text, &row);
        assert_eq!(read_cache_from(&cache, &source_text), None);
        assert!(!cache.exists());
        let _ = fs::remove_dir_all(root);
    }
}
