//! Lua-authored bulk-folder example; Rust shim exposes the same typed plan semantics for ABI tests.
use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult},
};
use explorer_extension_api::*;
use std::collections::BTreeSet;

const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 6_201);
const INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 6_202);

pub fn command_form() -> CommandFormV1 {
    CommandFormV1 {
        title: "Generate folders".into(),
        fields: vec![
            FormFieldV1 {
                id: "prefix".into(),
                label: "Prefix".into(),
                value: "Folder-".into(),
                required: false,
                kind: FormFieldKindV1::TEXT,
                choices: Vec::new().into(),
                minimum: ROption::RNone,
                maximum: ROption::RNone,
            },
            FormFieldV1 {
                id: "start".into(),
                label: "Start".into(),
                value: "1".into(),
                required: true,
                kind: FormFieldKindV1::INTEGER,
                choices: Vec::new().into(),
                minimum: ROption::RSome(0),
                maximum: ROption::RSome(i64::from(u32::MAX)),
            },
            FormFieldV1 {
                id: "count".into(),
                label: "Count (1-100000)".into(),
                value: "10".into(),
                required: true,
                kind: FormFieldKindV1::INTEGER,
                choices: Vec::new().into(),
                minimum: ROption::RSome(1),
                maximum: ROption::RSome(100_000),
            },
            FormFieldV1 {
                id: "padding".into(),
                label: "Zero padding (0-16)".into(),
                value: "3".into(),
                required: true,
                kind: FormFieldKindV1::INTEGER,
                choices: Vec::new().into(),
                minimum: ROption::RSome(0),
                maximum: ROption::RSome(16),
            },
            FormFieldV1 {
                id: "suffix".into(),
                label: "Suffix".into(),
                value: "".into(),
                required: false,
                kind: FormFieldKindV1::TEXT,
                choices: Vec::new().into(),
                minimum: ROption::RNone,
                maximum: ROption::RNone,
            },
        ]
        .into(),
    }
}

pub fn generate_names(
    prefix: &str,
    start: u32,
    count: u32,
    padding: usize,
    suffix: &str,
) -> Result<Vec<String>, String> {
    if !(1..=100_000).contains(&count) || padding > 16 {
        return Err("count or padding out of range".into());
    }
    let end = start
        .checked_add(count)
        .ok_or_else(|| "numeric range overflow".to_owned())?;
    let mut folded = BTreeSet::new();
    let mut names = Vec::with_capacity(count as usize);
    for number in start..end {
        let name = format!("{prefix}{number:0padding$}{suffix}");
        if name.is_empty()
            || name.ends_with(['.', ' '])
            || name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
        {
            return Err(format!("unsafe folder name: {name}"));
        }
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        ) {
            return Err(format!("reserved folder name: {name}"));
        }
        if !folded.insert(name.to_lowercase()) {
            return Err(format!("duplicate folder name: {name}"));
        }
        names.push(name);
    }
    Ok(names)
}

pub fn build_plan(
    parent: OperationObjectHandleV1,
    prefix: &str,
    start: u32,
    count: u32,
    padding: usize,
    suffix: &str,
) -> Result<OperationPlanV1, String> {
    let steps = generate_names(prefix, start, count, padding, suffix)?
        .into_iter()
        .map(|name| OperationStepV1 {
            kind: OperationKindV1::CREATE_DIRECTORY,
            source: ROption::RNone,
            destination_parent: ROption::RSome(parent),
            destination_name: ROption::RSome(name.into()),
            expected_source: ROption::RNone,
        })
        .collect::<Vec<_>>();
    Ok(OperationPlanV1 {
        title: format!("Create {count} folders").into(),
        root: parent,
        steps: steps.into(),
        confirmation_threshold: 1_000,
        undo_requested: true,
    })
}

struct Registrar;
impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        let entries = [
            (
                "lua-bulk-folder:button",
                RegisteredContributionKindV1::COMMAND,
            ),
            ("lua-bulk-folder:form", RegisteredContributionKindV1::FORM),
            (
                "lua-bulk-folder:plan",
                RegisteredContributionKindV1::OPERATION_PLAN,
            ),
        ];
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(3),
            contributions: entries
                .into_iter()
                .map(|(id, kind)| RegisteredContributionV1 {
                    feature_id: "lua-bulk-folder".into(),
                    contribution_id: id.into(),
                    kind,
                    required_capabilities: vec!["filesystem.write".into()].into(),
                    interface_id: INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    folder_admission: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RNone,
                    size_map_view: ROption::RNone,
                    virtual_folder_provider: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                })
                .collect::<Vec<_>>()
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
    fn bounds_padding_and_confirmation() {
        let parent = OperationObjectHandleV1::new([1; 16], 1);
        assert_eq!(
            generate_names("Album-", 8, 2, 3, "").unwrap(),
            ["Album-008", "Album-009"]
        );
        assert!(generate_names("", 0, 100_001, 0, "").is_err());
        assert!(!build_plan(parent, "F", 1, 1000, 0, "")
            .unwrap()
            .steps
            .is_empty());
        assert!(build_plan(parent, "F", 1, 1001, 0, "").unwrap().steps.len() > 1000);
    }
    #[test]
    fn unsafe_names_are_blocked_before_plan() {
        let parent = OperationObjectHandleV1::new([1; 16], 1);
        assert!(build_plan(parent, "../", 1, 1, 0, "").is_err());
        assert!(build_plan(parent, "CON", 0, 1, 0, "").is_ok());
        assert!(build_plan(parent, "", 0, 1, 0, ".").is_err());
    }
}
