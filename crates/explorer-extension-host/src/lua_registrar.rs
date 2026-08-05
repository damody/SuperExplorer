//! Restricted package Lua registrar. Scripts can declare data; they receive no ambient OS API.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use abi_stable::std_types::ROption;
use explorer_extension_api::{
    IdNamespaceV1, OperationKindV1, OperationObjectHandleV1, OperationPlanV1, OperationStepV1,
    OperationTerminalV1, PluginItemOutcomeV1, PluginValueV1, StableIdV1,
};
use mlua::{Lua, LuaOptions, StdLib, Table};

use crate::runtime_authority::{
    AuthorityAdapterV1, AuthorityClaimsV1, AuthorityEnvelopeV1, RuntimeAuthorityV1,
};

pub const MAX_LUA_CONTRIBUTIONS_V1: usize = 64;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaContributionV1 {
    pub id: String,
    pub feature_id: String,
    pub kind: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LuaRegistrarErrorV1 {
    #[error("Lua registrar rejected: {0}")]
    Lua(String),
    #[error("Lua contribution is malformed")]
    Malformed,
    #[error("Lua registrar exceeds contribution limit")]
    TooMany,
    #[error("Lua contribution feature or capability authority was denied")]
    Unauthorized,
    #[error("Lua callback authority is stale or revoked")]
    Stale,
    #[error("Lua typed value, terminal, or operation-plan mirror is malformed")]
    TypedMirror,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LuaDiagnosticCodeV1 {
    UndeclaredCapability,
    StaleAuthority,
    CallbackDenied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaDiagnosticV1 {
    pub package_id: String,
    pub feature_id: String,
    pub interface_id: String,
    pub capability: String,
    pub code: LuaDiagnosticCodeV1,
}

pub fn decode_lua_plugin_value_v1(source: &str) -> Result<PluginValueV1, LuaRegistrarErrorV1> {
    let lua = restricted_lua()?;
    let table = lua
        .load(source)
        .eval::<Table>()
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
    let kind = table
        .get::<String>("kind")
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
    let result = match kind.as_str() {
        "bool" => PluginValueV1::boolean(
            table
                .get("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        ),
        "integer" => PluginValueV1::integer(
            table
                .get("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        ),
        "float" => PluginValueV1::float(
            table
                .get("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        )
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        "bytes" => PluginValueV1::bytes(lua_bytes(&table, "value")?)
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        "time_unix_nanos" => PluginValueV1::time_unix_nanos(
            table
                .get("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        ),
        "duration_nanos" => PluginValueV1::duration_nanos(
            table
                .get("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        )
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        "text" => PluginValueV1::text(
            table
                .get::<String>("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        )
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        "localized_text" => PluginValueV1::localized_text(
            table
                .get::<String>("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        )
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        "structured" => PluginValueV1::structured_canonical_json(
            table
                .get::<String>("value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
                .into_bytes(),
        )
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
        "opaque" => {
            let authority = table
                .get::<u16>("schema_authority")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
            let revision = table
                .get::<u16>("schema_revision")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
            let value = table
                .get::<u64>("schema_value")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
            let version = table
                .get::<u32>("schema_version")
                .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
            PluginValueV1::opaque(
                StableIdV1::new(IdNamespaceV1::new(authority, revision), value),
                version,
                lua_bytes(&table, "value")?,
            )
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
        }
        _ => return Err(LuaRegistrarErrorV1::TypedMirror),
    };
    Ok(result)
}

fn lua_bytes(table: &Table, key: &str) -> Result<Vec<u8>, LuaRegistrarErrorV1> {
    table
        .get::<Table>(key)
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
        .sequence_values::<u8>()
        .collect::<mlua::Result<Vec<_>>>()
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)
}

pub fn decode_lua_item_outcome_v1(value: &str) -> Result<PluginItemOutcomeV1, LuaRegistrarErrorV1> {
    match value {
        "value" => Ok(PluginItemOutcomeV1::VALUE),
        "unsupported" => Ok(PluginItemOutcomeV1::UNSUPPORTED),
        "unavailable" => Ok(PluginItemOutcomeV1::UNAVAILABLE),
        "cancelled" => Ok(PluginItemOutcomeV1::CANCELLED),
        "plugin_error" => Ok(PluginItemOutcomeV1::PLUGIN_ERROR),
        "incompatible" => Ok(PluginItemOutcomeV1::INCOMPATIBLE),
        _ => Err(LuaRegistrarErrorV1::TypedMirror),
    }
}

pub fn decode_lua_operation_terminal_v1(
    value: &str,
) -> Result<OperationTerminalV1, LuaRegistrarErrorV1> {
    match value {
        "completed" => Ok(OperationTerminalV1::COMPLETED),
        "cancelled" => Ok(OperationTerminalV1::CANCELLED),
        "partial" => Ok(OperationTerminalV1::PARTIAL),
        "conflict" => Ok(OperationTerminalV1::CONFLICT),
        "rejected" => Ok(OperationTerminalV1::REJECTED),
        _ => Err(LuaRegistrarErrorV1::TypedMirror),
    }
}

pub fn decode_lua_operation_plan_v1(
    source: &str,
    handles: &BTreeMap<String, OperationObjectHandleV1>,
) -> Result<OperationPlanV1, LuaRegistrarErrorV1> {
    let lua = restricted_lua()?;
    let table = lua
        .load(source)
        .eval::<Table>()
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
    let resolve = |key: String| {
        handles
            .get(&key)
            .copied()
            .ok_or(LuaRegistrarErrorV1::TypedMirror)
    };
    let root = resolve(
        table
            .get("root")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?,
    )?;
    let step_tables = table
        .get::<Table>("steps")
        .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
    let mut steps = Vec::new();
    for candidate in step_tables.sequence_values::<Table>() {
        let step = candidate.map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
        let kind = match step
            .get::<String>("kind")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
            .as_str()
        {
            "create_directory" => OperationKindV1::CREATE_DIRECTORY,
            "rename" => OperationKindV1::RENAME,
            "copy" => OperationKindV1::COPY,
            "move" => OperationKindV1::MOVE,
            "delete" => OperationKindV1::DELETE,
            "extract" => OperationKindV1::EXTRACT,
            "archive_mutation" => OperationKindV1::ARCHIVE_MUTATION,
            _ => return Err(LuaRegistrarErrorV1::TypedMirror),
        };
        let source_handle = step
            .get::<Option<String>>("source")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
            .map(resolve)
            .transpose()?;
        let parent = step
            .get::<Option<String>>("destination_parent")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
            .map(resolve)
            .transpose()?;
        let name = step
            .get::<Option<String>>("destination_name")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?;
        steps.push(OperationStepV1 {
            kind,
            source: source_handle.into(),
            destination_parent: parent.into(),
            destination_name: name.map(Into::into).into(),
            expected_source: ROption::RNone,
        });
    }
    Ok(OperationPlanV1 {
        title: table
            .get::<String>("title")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
            .into(),
        root,
        steps: steps.into(),
        confirmation_threshold: table
            .get::<Option<u32>>("confirmation_threshold")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
            .unwrap_or(0),
        undo_requested: table
            .get::<Option<bool>>("undo_requested")
            .map_err(|_| LuaRegistrarErrorV1::TypedMirror)?
            .unwrap_or(false),
    })
}

#[derive(Clone)]
pub struct AuthorizedLuaContributionV1 {
    pub descriptor: LuaContributionV1,
    package_id: String,
    runtime: Arc<RuntimeAuthorityV1>,
    grants: BTreeMap<String, AuthorityEnvelopeV1>,
    diagnostics: Arc<std::sync::Mutex<Vec<LuaDiagnosticV1>>>,
}

impl std::fmt::Debug for AuthorizedLuaContributionV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthorizedLuaContributionV1")
            .field("descriptor", &self.descriptor)
            .field("grants", &self.grants.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AuthorizedLuaContributionV1 {
    fn record(&self, capability: &str, code: LuaDiagnosticCodeV1) {
        if let Ok(mut diagnostics) = self.diagnostics.lock() {
            diagnostics.push(LuaDiagnosticV1 {
                package_id: self.package_id.clone(),
                feature_id: self.descriptor.feature_id.clone(),
                interface_id: self.descriptor.id.clone(),
                capability: capability.to_owned(),
                code,
            });
        }
    }

    pub fn revalidate(&self, capability: &str) -> Result<(), LuaRegistrarErrorV1> {
        let Some(grant) = self.grants.get(capability) else {
            self.record(capability, LuaDiagnosticCodeV1::UndeclaredCapability);
            return Err(LuaRegistrarErrorV1::Unauthorized);
        };
        let result = self
            .runtime
            .revalidate(grant, AuthorityAdapterV1::Lua)
            .map(|_| ())
            .map_err(|_| LuaRegistrarErrorV1::Stale);
        if result.is_err() {
            self.record(capability, LuaDiagnosticCodeV1::StaleAuthority);
        }
        result
    }

    /// Runs one synchronous callback in the same ambient-authority-free Lua
    /// environment, revalidating the exact capability before and after it.
    pub fn invoke_no_result(
        &self,
        capability: &str,
        source: &str,
        function_name: &str,
    ) -> Result<(), LuaRegistrarErrorV1> {
        self.revalidate(capability)?;
        let lua = restricted_lua()?;
        if let Err(error) = lua.load(source).set_name("package callback").exec() {
            self.record(capability, LuaDiagnosticCodeV1::CallbackDenied);
            return Err(LuaRegistrarErrorV1::Lua(error.to_string()));
        }
        if let Err(error) = lua
            .globals()
            .get::<mlua::Function>(function_name)
            .and_then(|callback| callback.call::<()>(()))
        {
            self.record(capability, LuaDiagnosticCodeV1::CallbackDenied);
            return Err(LuaRegistrarErrorV1::Lua(error.to_string()));
        }
        self.revalidate(capability)
    }
}

pub struct LuaPackageAuthorityV1 {
    package_id: String,
    incarnation: u64,
    authorized_root_sha256: String,
    features: BTreeMap<String, BTreeSet<String>>,
    runtime: Arc<RuntimeAuthorityV1>,
    diagnostics: Arc<std::sync::Mutex<Vec<LuaDiagnosticV1>>>,
}

impl LuaPackageAuthorityV1 {
    /// Creates the host-owned Lua authority only after the caller has sealed
    /// the package manifest and projected its declared feature capabilities.
    pub fn activate_sealed(
        package_id: String,
        incarnation: u64,
        authorized_root_sha256: String,
        features: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<Self, LuaRegistrarErrorV1> {
        if package_id.is_empty()
            || incarnation == 0
            || authorized_root_sha256.len() != 64
            || !authorized_root_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || features.is_empty()
            || features.iter().any(|(feature, capabilities)| {
                feature.is_empty()
                    || capabilities.is_empty()
                    || capabilities.iter().any(String::is_empty)
            })
        {
            return Err(LuaRegistrarErrorV1::Unauthorized);
        }
        let runtime =
            Arc::new(RuntimeAuthorityV1::new().map_err(|_| LuaRegistrarErrorV1::Unauthorized)?);
        Ok(Self::for_host(
            package_id,
            incarnation,
            authorized_root_sha256,
            features,
            runtime,
        ))
    }

    pub(crate) fn for_host(
        package_id: String,
        incarnation: u64,
        authorized_root_sha256: String,
        features: BTreeMap<String, BTreeSet<String>>,
        runtime: Arc<RuntimeAuthorityV1>,
    ) -> Self {
        Self {
            package_id,
            incarnation,
            authorized_root_sha256,
            features,
            runtime,
            diagnostics: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn drain_diagnostics(&self) -> Vec<LuaDiagnosticV1> {
        self.diagnostics
            .lock()
            .map_or_else(|_| Vec::new(), |mut values| std::mem::take(&mut *values))
    }

    /// Disables one feature and synchronously invalidates all Lua handles
    /// minted for it before returning.
    pub fn disable_feature(&self, feature_id: &str) -> Result<usize, LuaRegistrarErrorV1> {
        self.runtime
            .revoke_feature(&self.package_id, feature_id)
            .map_err(|_| LuaRegistrarErrorV1::Unauthorized)
    }

    /// Advances a sealed package generation. Existing callbacks remain bound
    /// to the old incarnation and are revoked before new grants can be minted.
    pub fn replace_sealed_generation(
        &mut self,
        incarnation: u64,
        authorized_root_sha256: String,
        features: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), LuaRegistrarErrorV1> {
        if incarnation <= self.incarnation
            || authorized_root_sha256.len() != 64
            || !authorized_root_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || features.is_empty()
        {
            return Err(LuaRegistrarErrorV1::Unauthorized);
        }
        for feature in self.features.keys() {
            self.runtime
                .revoke_feature_incarnation(&self.package_id, feature, self.incarnation)
                .map_err(|_| LuaRegistrarErrorV1::Unauthorized)?;
        }
        self.incarnation = incarnation;
        self.authorized_root_sha256 = authorized_root_sha256;
        self.features = features;
        Ok(())
    }

    pub fn authorize(
        &self,
        contributions: Vec<LuaContributionV1>,
    ) -> Result<Vec<AuthorizedLuaContributionV1>, LuaRegistrarErrorV1> {
        contributions
            .into_iter()
            .map(|descriptor| {
                let declared = self
                    .features
                    .get(&descriptor.feature_id)
                    .ok_or(LuaRegistrarErrorV1::Unauthorized)?;
                let required = required_capabilities(&descriptor.kind)
                    .ok_or(LuaRegistrarErrorV1::Malformed)?;
                if !descriptor
                    .capabilities
                    .iter()
                    .any(|value| required.contains(&value.as_str()))
                    || descriptor
                        .capabilities
                        .iter()
                        .any(|value| !declared.contains(value))
                {
                    if let Ok(mut diagnostics) = self.diagnostics.lock() {
                        diagnostics.push(LuaDiagnosticV1 {
                            package_id: self.package_id.clone(),
                            feature_id: descriptor.feature_id.clone(),
                            interface_id: descriptor.id.clone(),
                            capability: descriptor
                                .capabilities
                                .iter()
                                .find(|value| !declared.contains(*value))
                                .cloned()
                                .unwrap_or_else(|| "required-registration-capability".to_owned()),
                            code: LuaDiagnosticCodeV1::UndeclaredCapability,
                        });
                    }
                    return Err(LuaRegistrarErrorV1::Unauthorized);
                }
                let mut grants = BTreeMap::new();
                for capability in &descriptor.capabilities {
                    let envelope = self
                        .runtime
                        .issue(AuthorityClaimsV1 {
                            package_id: self.package_id.clone(),
                            feature_id: descriptor.feature_id.clone(),
                            interface_id: descriptor.id.clone(),
                            incarnation: self.incarnation,
                            capability: capability.clone(),
                            authorized_root_sha256: self.authorized_root_sha256.clone(),
                            location_generation: 1,
                            item_generation: 1,
                            refresh_generation: 1,
                            container_generation: 1,
                            job_generation: 1,
                        })
                        .map_err(|_| LuaRegistrarErrorV1::Unauthorized)?;
                    grants.insert(capability.clone(), envelope);
                }
                Ok(AuthorizedLuaContributionV1 {
                    descriptor,
                    package_id: self.package_id.clone(),
                    runtime: Arc::clone(&self.runtime),
                    grants,
                    diagnostics: Arc::clone(&self.diagnostics),
                })
            })
            .collect()
    }
}

fn required_capabilities(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "column" | "single_column" | "batch_column" => Some(&["column.read", "filesystem.read"]),
        "command" | "button" => Some(&["commands.invoke", "filesystem.write"]),
        "form" => Some(&["forms.submit", "filesystem.write"]),
        "operation_plan" => Some(&["operations.submit", "filesystem.write"]),
        _ => None,
    }
}

fn restricted_lua() -> Result<Lua, LuaRegistrarErrorV1> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .map_err(|error| LuaRegistrarErrorV1::Lua(error.to_string()))?;
    let globals = lua.globals();
    for name in [
        "os",
        "io",
        "package",
        "debug",
        "dofile",
        "loadfile",
        "require",
        "load",
        "process",
        "network",
        "filesystem",
        "model",
    ] {
        globals
            .set(name, mlua::Value::Nil)
            .map_err(|error| LuaRegistrarErrorV1::Lua(error.to_string()))?;
    }
    drop(globals);
    Ok(lua)
}

pub fn run_restricted_lua_registrar_v1(
    source: &str,
) -> Result<Vec<LuaContributionV1>, LuaRegistrarErrorV1> {
    let lua = restricted_lua()?;
    let globals = lua.globals();
    let registered = Arc::new(std::sync::Mutex::new(Vec::<LuaContributionV1>::new()));
    let sink = registered.clone();
    let register = lua
        .create_function(move |_, table: Table| {
            let capabilities = table
                .get::<Table>("capabilities")?
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()?;
            let value = LuaContributionV1 {
                id: table.get("id")?,
                feature_id: table.get("feature_id")?,
                kind: table.get("kind")?,
                capabilities,
            };
            sink.lock()
                .map_err(|_| mlua::Error::runtime("registrar poisoned"))?
                .push(value);
            Ok(())
        })
        .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    let api = lua
        .create_table()
        .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    api.set("register", register)
        .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    globals
        .set("superexplorer", api)
        .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    lua.load(source)
        .set_name("package registrar")
        .exec()
        .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    let values = registered
        .lock()
        .map_err(|_| LuaRegistrarErrorV1::Malformed)?
        .clone();
    if values.len() > MAX_LUA_CONTRIBUTIONS_V1 {
        return Err(LuaRegistrarErrorV1::TooMany);
    }
    if values.iter().any(|v| {
        v.id.is_empty()
            || v.feature_id.is_empty()
            || !matches!(
                v.kind.as_str(),
                "column"
                    | "single_column"
                    | "batch_column"
                    | "command"
                    | "button"
                    | "form"
                    | "operation_plan"
            )
    }) {
        return Err(LuaRegistrarErrorV1::Malformed);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_extension_api::PluginValueKindV1;
    #[test]
    fn registers_owned_data_and_has_no_ambient_process_api() {
        let source = r#"assert(os == nil and io == nil and require == nil)
superexplorer.register { id='lua-tokei:column', feature_id='lua-tokei', kind='column', capabilities={'filesystem.read','tools.execute_bundled'} }"#;
        let values = run_restricted_lua_registrar_v1(source).unwrap();
        assert_eq!(values[0].id, "lua-tokei:column");
    }
    #[test]
    fn direct_process_access_fails() {
        assert!(run_restricted_lua_registrar_v1("os.execute('cmd')").is_err());
    }
    #[test]
    fn canonical_lua_examples_execute_in_the_restricted_host_registrar() {
        let tokei = run_restricted_lua_registrar_v1(include_str!(
            "../../../sdk/fixtures/lua-tokei-code-lines-column/lua/main.lua"
        ))
        .unwrap();
        assert_eq!(tokei.len(), 1);
        assert_eq!(tokei[0].kind, "column");
        let bulk = run_restricted_lua_registrar_v1(include_str!(
            "../../../sdk/fixtures/lua-bulk-folder-generator/lua/main.lua"
        ))
        .unwrap();
        assert_eq!(bulk.len(), 3);
        assert_eq!(
            bulk.iter()
                .map(|value| value.kind.as_str())
                .collect::<Vec<_>>(),
            ["command", "form", "operation_plan"]
        );
    }

    #[test]
    fn every_registration_and_callback_revalidates_feature_capability_and_incarnation() {
        let authority = LuaPackageAuthorityV1::activate_sealed(
            "lua-package".into(),
            7,
            "a".repeat(64),
            BTreeMap::from([(
                "feature".into(),
                BTreeSet::from(["filesystem.read".into(), "tools.execute_bundled".into()]),
            )]),
        )
        .unwrap();
        let descriptor = LuaContributionV1 {
            id: "lua-package:batch".into(),
            feature_id: "feature".into(),
            kind: "batch_column".into(),
            capabilities: vec!["filesystem.read".into(), "tools.execute_bundled".into()],
        };
        let authorized = authority
            .authorize(vec![descriptor.clone()])
            .unwrap()
            .remove(0);
        authorized
            .invoke_no_result(
                "filesystem.read",
                "function callback() assert(os == nil and io == nil and process == nil and network == nil and filesystem == nil and model == nil) end",
                "callback",
            )
            .unwrap();

        let mut undeclared = descriptor;
        undeclared.capabilities.push("filesystem.write".into());
        assert!(matches!(
            authority.authorize(vec![undeclared]),
            Err(LuaRegistrarErrorV1::Unauthorized)
        ));
        assert_eq!(
            authority.drain_diagnostics(),
            [LuaDiagnosticV1 {
                package_id: "lua-package".into(),
                feature_id: "feature".into(),
                interface_id: "lua-package:batch".into(),
                capability: "filesystem.write".into(),
                code: LuaDiagnosticCodeV1::UndeclaredCapability,
            }]
        );
        assert_eq!(authority.disable_feature("feature").unwrap(), 2);
        assert!(matches!(
            authorized.revalidate("filesystem.read"),
            Err(LuaRegistrarErrorV1::Stale)
        ));
        assert!(matches!(
            authorized.invoke_no_result("filesystem.read", "function callback() end", "callback"),
            Err(LuaRegistrarErrorV1::Stale)
        ));
        let diagnostics = authority.drain_diagnostics();
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|entry| {
            entry.package_id == "lua-package"
                && entry.feature_id == "feature"
                && entry.interface_id == "lua-package:batch"
                && entry.capability == "filesystem.read"
                && entry.code == LuaDiagnosticCodeV1::StaleAuthority
        }));
    }

    #[test]
    fn package_update_and_forbidden_callback_fail_before_side_effect() {
        let mut authority = LuaPackageAuthorityV1::activate_sealed(
            "lua-package".into(),
            1,
            "b".repeat(64),
            BTreeMap::from([("feature".into(), BTreeSet::from(["filesystem.read".into()]))]),
        )
        .unwrap();
        let authorized = authority
            .authorize(vec![LuaContributionV1 {
                id: "lua-package:column".into(),
                feature_id: "feature".into(),
                kind: "column".into(),
                capabilities: vec!["filesystem.read".into()],
            }])
            .unwrap()
            .remove(0);

        assert!(
            authorized
                .invoke_no_result(
                    "filesystem.read",
                    "function callback() filesystem.delete('must-not-run') end",
                    "callback",
                )
                .is_err()
        );
        assert_eq!(
            authority.drain_diagnostics()[0].code,
            LuaDiagnosticCodeV1::CallbackDenied
        );

        authority
            .replace_sealed_generation(
                2,
                "c".repeat(64),
                BTreeMap::from([("feature".into(), BTreeSet::from(["filesystem.read".into()]))]),
            )
            .unwrap();
        assert!(matches!(
            authorized.invoke_no_result(
                "filesystem.read",
                "error('old callback must not execute')",
                "callback"
            ),
            Err(LuaRegistrarErrorV1::Stale)
        ));
        assert_eq!(
            authority.drain_diagnostics()[0].code,
            LuaDiagnosticCodeV1::StaleAuthority
        );
    }

    #[test]
    fn lua_typed_mirrors_match_rust_values_terminals_and_opaque_plans() {
        let integer = decode_lua_plugin_value_v1("return { kind='integer', value=42 }").unwrap();
        assert_eq!(integer.kind, PluginValueKindV1::I64);
        assert_eq!(integer.integer, 42);
        let structured =
            decode_lua_plugin_value_v1("return { kind='structured', value='{\"a\":1}' }").unwrap();
        assert_eq!(structured.kind, PluginValueKindV1::STRUCTURED);
        assert_eq!(
            decode_lua_item_outcome_v1("unsupported").unwrap(),
            PluginItemOutcomeV1::UNSUPPORTED
        );
        assert_eq!(
            decode_lua_operation_terminal_v1("partial").unwrap(),
            OperationTerminalV1::PARTIAL
        );

        let root = OperationObjectHandleV1::new([1; 16], 3);
        let handles = BTreeMap::from([("root".into(), root)]);
        let plan = decode_lua_operation_plan_v1(
            "return { title='folders', root='root', confirmation_threshold=1000, undo_requested=true, steps={{ kind='create_directory', destination_parent='root', destination_name='Folder-001' }} }",
            &handles,
        )
        .unwrap();
        assert_eq!(plan.root, root);
        assert_eq!(plan.steps[0].kind, OperationKindV1::CREATE_DIRECTORY);
        assert!(decode_lua_plugin_value_v1("return { kind='future_kind', value=1 }").is_err());
        assert!(decode_lua_operation_terminal_v1("future_terminal").is_err());
        assert!(
            decode_lua_operation_plan_v1(
                "return { title='bad', root='forged', steps={} }",
                &handles
            )
            .is_err()
        );
    }

    #[test]
    fn all_six_registration_kinds_are_owned_and_ambient_authority_is_absent() {
        let source = r#"
for _, kind in ipairs({'single_column','batch_column','command','button','form','operation_plan'}) do
  superexplorer.register { id='feature:' .. kind, feature_id='feature', kind=kind, capabilities={'filesystem.read'} }
end
"#;
        let values = run_restricted_lua_registrar_v1(source).unwrap();
        assert_eq!(values.len(), 6);
        assert!(values.iter().all(|value| value.id.starts_with("feature:")));
        for forbidden in [
            "os.execute('cmd')",
            "io.open('x')",
            "require('socket')",
            "filesystem.delete('x')",
            "network.get('x')",
            "process.spawn('x')",
            "model.items()",
        ] {
            assert!(run_restricted_lua_registrar_v1(forbidden).is_err());
        }
    }
}
