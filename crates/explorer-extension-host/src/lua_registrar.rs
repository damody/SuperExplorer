//! Restricted package Lua registrar. Scripts can declare data; they receive no ambient OS API.
use mlua::{Lua, LuaOptions, StdLib, Table};

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
}

pub fn run_restricted_lua_registrar_v1(
    source: &str,
) -> Result<Vec<LuaContributionV1>, LuaRegistrarErrorV1> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    let globals = lua.globals();
    for name in [
        "os", "io", "package", "debug", "dofile", "loadfile", "require", "load",
    ] {
        globals
            .set(name, mlua::Value::Nil)
            .map_err(|e| LuaRegistrarErrorV1::Lua(e.to_string()))?;
    }
    let registered = std::sync::Arc::new(std::sync::Mutex::new(Vec::<LuaContributionV1>::new()));
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
                "column" | "renderer" | "command" | "form" | "operation_plan"
            )
    }) {
        return Err(LuaRegistrarErrorV1::Malformed);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
