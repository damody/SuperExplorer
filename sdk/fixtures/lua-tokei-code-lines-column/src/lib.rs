//! Lua tokei example: package-attested ToolHandle, bounded batches, stable per-item mapping.
use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult},
};
use explorer_extension_api::*;

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 6_101);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 6_102);
pub const MAX_BATCH: usize = 128;
pub const MAX_WINDOWS_ARGUMENT_CHARS: usize = 28_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeRow {
    pub path: String,
    pub code: u64,
    pub comments: u64,
    pub blanks: u64,
    pub total: u64,
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
    let mut output = Vec::new();
    for batch in bounded_batches(paths)? {
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
        output.extend(rows);
    }
    Ok(output)
}

struct Registrar;
impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(1),
            contributions: vec![RegisteredContributionV1 {
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
                renderer_contribution_id: ROption::RNone,
                provider: ROption::RNone,
                visual_column: ROption::RNone,
                size_map_view: ROption::RNone,
                batch_column_provider: ROption::RNone,
            }]
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
}
