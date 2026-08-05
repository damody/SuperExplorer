//! Host-owned command/form catalog and validation for extension UI surfaces.

use std::collections::{BTreeMap, BTreeSet};

use explorer_extension_api::{
    CommandDescriptorV1, CommandFormV1, CommandPlacementV1, FormFieldKindV1, MAX_FORM_FIELDS_V1,
    SelectionRequirementV1,
};

const MAX_COMMANDS_V1: usize = 256;
const MAX_FIELD_BYTES_V1: usize = 4_096;

#[derive(Clone, Debug)]
pub struct CommandRegistrationV1 {
    pub package_id: String,
    pub feature_id: String,
    pub feature_enabled: bool,
    pub capabilities: BTreeSet<String>,
    pub descriptor: CommandDescriptorV1,
    pub form: Option<CommandFormV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandUiSnapshotV1 {
    pub package_id: String,
    pub feature_id: String,
    pub command_id: String,
    pub label: String,
    pub shortcut: Option<String>,
    pub shortcut_active: bool,
    pub focus_order: Vec<String>,
    pub accessible_labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CommandRegistryErrorV1 {
    #[error("command catalog exceeds its host bound")]
    Capacity,
    #[error("command identifier, label, placement, or selection requirement is invalid")]
    InvalidDescriptor,
    #[error("duplicate command identifier")]
    DuplicateCommand,
    #[error("command contribution lacks commands.invoke authority")]
    MissingCapability,
    #[error("form schema is invalid")]
    InvalidForm,
    #[error("form submission is invalid for field {0}")]
    InvalidSubmission(String),
}

#[derive(Default)]
pub struct HostCommandRegistryV1 {
    live: BTreeMap<String, CommandRegistrationV1>,
}

impl HostCommandRegistryV1 {
    pub fn replace(
        &mut self,
        registrations: Vec<CommandRegistrationV1>,
    ) -> Result<(), CommandRegistryErrorV1> {
        if registrations.len() > MAX_COMMANDS_V1 {
            return Err(CommandRegistryErrorV1::Capacity);
        }
        let mut next = BTreeMap::new();
        for registration in registrations {
            validate_registration(&registration)?;
            let key = registration.descriptor.id.as_str().to_owned();
            if next.insert(key, registration).is_some() {
                return Err(CommandRegistryErrorV1::DuplicateCommand);
            }
        }
        self.live = next;
        Ok(())
    }

    #[must_use]
    pub fn ui_snapshot(&self) -> Vec<CommandUiSnapshotV1> {
        let mut claimed_shortcuts = BTreeSet::new();
        self.live
            .values()
            .filter(|registration| registration.feature_enabled)
            .map(|registration| {
                let shortcut = registration
                    .descriptor
                    .shortcut
                    .as_ref()
                    .into_option()
                    .map(|value| normalize_shortcut(value.as_str()));
                let shortcut_active = shortcut
                    .as_ref()
                    .is_some_and(|value| claimed_shortcuts.insert(value.clone()));
                let (focus_order, accessible_labels) = registration.form.as_ref().map_or_else(
                    || (Vec::new(), Vec::new()),
                    |form| {
                        (
                            form.fields
                                .iter()
                                .map(|field| field.id.to_string())
                                .collect(),
                            form.fields
                                .iter()
                                .map(|field| field.label.to_string())
                                .collect(),
                        )
                    },
                );
                CommandUiSnapshotV1 {
                    package_id: registration.package_id.clone(),
                    feature_id: registration.feature_id.clone(),
                    command_id: registration.descriptor.id.to_string(),
                    label: registration.descriptor.label.to_string(),
                    shortcut,
                    shortcut_active,
                    focus_order,
                    accessible_labels,
                }
            })
            .collect()
    }

    pub fn validate_submission(
        &self,
        command_id: &str,
        values: &BTreeMap<String, String>,
        authorized_locations: &BTreeSet<String>,
    ) -> Result<(), CommandRegistryErrorV1> {
        let registration = self
            .live
            .get(command_id)
            .filter(|registration| registration.feature_enabled)
            .ok_or(CommandRegistryErrorV1::InvalidDescriptor)?;
        let Some(form) = &registration.form else {
            return if values.is_empty() {
                Ok(())
            } else {
                Err(CommandRegistryErrorV1::InvalidSubmission(command_id.into()))
            };
        };
        if values.len() != form.fields.len() {
            return Err(CommandRegistryErrorV1::InvalidSubmission(command_id.into()));
        }
        for field in &form.fields {
            let value = values
                .get(field.id.as_str())
                .ok_or_else(|| CommandRegistryErrorV1::InvalidSubmission(field.id.to_string()))?;
            if value.len() > MAX_FIELD_BYTES_V1 || (field.required && value.trim().is_empty()) {
                return Err(CommandRegistryErrorV1::InvalidSubmission(
                    field.id.to_string(),
                ));
            }
            if field.kind == FormFieldKindV1::INTEGER {
                let number = value
                    .parse::<i64>()
                    .map_err(|_| CommandRegistryErrorV1::InvalidSubmission(field.id.to_string()))?;
                if field
                    .minimum
                    .as_ref()
                    .into_option()
                    .is_some_and(|min| number < *min)
                    || field
                        .maximum
                        .as_ref()
                        .into_option()
                        .is_some_and(|max| number > *max)
                {
                    return Err(CommandRegistryErrorV1::InvalidSubmission(
                        field.id.to_string(),
                    ));
                }
            } else if field.kind == FormFieldKindV1::CHOICE
                && !field.choices.iter().any(|choice| choice.as_str() == value)
            {
                return Err(CommandRegistryErrorV1::InvalidSubmission(
                    field.id.to_string(),
                ));
            } else if field.kind == FormFieldKindV1::AUTHORIZED_LOCATION
                && !authorized_locations.contains(value)
            {
                return Err(CommandRegistryErrorV1::InvalidSubmission(
                    field.id.to_string(),
                ));
            } else if field.kind == FormFieldKindV1::TEMPLATE
                && (value.contains("..") || value.contains(['/', '\\', '\0']))
            {
                return Err(CommandRegistryErrorV1::InvalidSubmission(
                    field.id.to_string(),
                ));
            } else if field.kind == FormFieldKindV1::BOOLEAN
                && !matches!(value.as_str(), "true" | "false")
            {
                return Err(CommandRegistryErrorV1::InvalidSubmission(
                    field.id.to_string(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_registration(
    registration: &CommandRegistrationV1,
) -> Result<(), CommandRegistryErrorV1> {
    let descriptor = &registration.descriptor;
    if registration.package_id.is_empty()
        || registration.feature_id.is_empty()
        || descriptor.id.is_empty()
        || descriptor.label.trim().is_empty()
        || !matches!(
            descriptor.placement,
            CommandPlacementV1::TOOLBAR
                | CommandPlacementV1::CONTEXT_MENU
                | CommandPlacementV1::EXTENSIONS_MENU
        )
        || !matches!(
            descriptor.selection,
            SelectionRequirementV1::NONE
                | SelectionRequirementV1::ONE
                | SelectionRequirementV1::ONE_OR_MORE
        )
    {
        return Err(CommandRegistryErrorV1::InvalidDescriptor);
    }
    if !registration.capabilities.contains("commands.invoke") {
        return Err(CommandRegistryErrorV1::MissingCapability);
    }
    if let Some(form) = &registration.form {
        if form.fields.is_empty() || form.fields.len() > MAX_FORM_FIELDS_V1 {
            return Err(CommandRegistryErrorV1::InvalidForm);
        }
        let mut ids = BTreeSet::new();
        for field in &form.fields {
            if field.id.is_empty()
                || field.label.trim().is_empty()
                || !ids.insert(field.id.to_string())
                || !matches!(
                    field.kind,
                    FormFieldKindV1::TEXT
                        | FormFieldKindV1::INTEGER
                        | FormFieldKindV1::CHOICE
                        | FormFieldKindV1::AUTHORIZED_LOCATION
                        | FormFieldKindV1::TEMPLATE
                        | FormFieldKindV1::BOOLEAN
                )
                || (field.kind == FormFieldKindV1::CHOICE && field.choices.is_empty())
                || (field.kind != FormFieldKindV1::CHOICE && !field.choices.is_empty())
                || matches!(
                    (field.minimum.as_ref().into_option(), field.maximum.as_ref().into_option()),
                    (Some(min), Some(max)) if min > max
                )
            {
                return Err(CommandRegistryErrorV1::InvalidForm);
            }
        }
    }
    Ok(())
}

fn normalize_shortcut(value: &str) -> String {
    value
        .split('+')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("+")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_stable::std_types::{ROption, RString};
    use explorer_extension_api::{CommandPlacementV1, FormFieldV1};

    fn registration(id: &str, enabled: bool, shortcut: &str) -> CommandRegistrationV1 {
        CommandRegistrationV1 {
            package_id: "package".into(),
            feature_id: id.into(),
            feature_enabled: enabled,
            capabilities: BTreeSet::from(["commands.invoke".into()]),
            descriptor: CommandDescriptorV1 {
                id: id.into(),
                label: id.into(),
                placement: CommandPlacementV1::EXTENSIONS_MENU,
                selection: SelectionRequirementV1::NONE,
                shortcut: ROption::RSome(shortcut.into()),
            },
            form: Some(CommandFormV1 {
                title: "Create".into(),
                fields: vec![FormFieldV1 {
                    id: "count".into(),
                    label: "Count".into(),
                    value: "1".into(),
                    required: true,
                    kind: FormFieldKindV1::INTEGER,
                    choices: Vec::<RString>::new().into(),
                    minimum: ROption::RSome(1),
                    maximum: ROption::RSome(100_000),
                }]
                .into(),
            }),
        }
    }

    #[test]
    fn registry_is_atomic_hides_disabled_and_resolves_shortcuts_deterministically() {
        let mut registry = HostCommandRegistryV1::default();
        registry
            .replace(vec![
                registration("b", true, "Ctrl + Shift + R"),
                registration("a", true, "Ctrl+Shift+R"),
                registration("hidden", false, "Ctrl+H"),
            ])
            .unwrap();
        let snapshot = registry.ui_snapshot();
        assert_eq!(
            snapshot
                .iter()
                .map(|row| row.command_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(snapshot[0].shortcut_active);
        assert!(!snapshot[1].shortcut_active);
        assert_eq!(snapshot[0].focus_order, ["count"]);
        assert_eq!(snapshot[0].accessible_labels, ["Count"]);

        let mut invalid = registration("a", true, "Ctrl+A");
        invalid.capabilities.clear();
        assert_eq!(
            registry.replace(vec![invalid]),
            Err(CommandRegistryErrorV1::MissingCapability)
        );
        assert_eq!(
            registry.ui_snapshot().len(),
            2,
            "failed replacement is atomic"
        );
    }

    #[test]
    fn typed_form_rejects_out_of_range_before_plan() {
        let mut registry = HostCommandRegistryV1::default();
        registry
            .replace(vec![registration("bulk", true, "Ctrl+B")])
            .unwrap();
        let authorized = BTreeSet::new();
        for value in ["0", "100001", "not-a-number"] {
            assert!(
                registry
                    .validate_submission(
                        "bulk",
                        &BTreeMap::from([("count".into(), value.into())]),
                        &authorized
                    )
                    .is_err()
            );
        }
        assert!(
            registry
                .validate_submission(
                    "bulk",
                    &BTreeMap::from([("count".into(), "100000".into())]),
                    &authorized
                )
                .is_ok()
        );
    }

    #[test]
    fn choice_location_and_template_are_validated_before_dispatch() {
        let mut command = registration("advanced", true, "Ctrl+L");
        command.form.as_mut().unwrap().fields = vec![
            FormFieldV1 {
                id: "mode".into(),
                label: "Mode".into(),
                value: "skip".into(),
                required: true,
                kind: FormFieldKindV1::CHOICE,
                choices: vec![RString::from("skip"), RString::from("replace")].into(),
                minimum: ROption::RNone,
                maximum: ROption::RNone,
            },
            FormFieldV1 {
                id: "parent".into(),
                label: "Parent".into(),
                value: "location:1".into(),
                required: true,
                kind: FormFieldKindV1::AUTHORIZED_LOCATION,
                choices: Vec::<RString>::new().into(),
                minimum: ROption::RNone,
                maximum: ROption::RNone,
            },
            FormFieldV1 {
                id: "template".into(),
                label: "Template".into(),
                value: "Folder-{n}".into(),
                required: true,
                kind: FormFieldKindV1::TEMPLATE,
                choices: Vec::<RString>::new().into(),
                minimum: ROption::RNone,
                maximum: ROption::RNone,
            },
        ]
        .into();
        let mut registry = HostCommandRegistryV1::default();
        registry.replace(vec![command]).unwrap();
        let authorized = BTreeSet::from(["location:1".to_owned()]);
        let valid = BTreeMap::from([
            ("mode".into(), "skip".into()),
            ("parent".into(), "location:1".into()),
            ("template".into(), "Folder-{n}".into()),
        ]);
        assert!(
            registry
                .validate_submission("advanced", &valid, &authorized)
                .is_ok()
        );
        for (field, bad) in [
            ("mode", "unknown"),
            ("parent", "D:\\raw"),
            ("template", "..\\escape"),
        ] {
            let mut values = valid.clone();
            values.insert(field.into(), bad.into());
            assert!(
                registry
                    .validate_submission("advanced", &values, &authorized)
                    .is_err()
            );
        }
    }
}
