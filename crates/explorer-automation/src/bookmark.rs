//! Minimal host-free Lua execution for manually invoked bookmarks.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use mlua::{HookTriggers, Lua, LuaOptions, StdLib, VmState};

pub const BOOKMARK_LUA_TIMEOUT_MS: u64 = 5_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaBookmarkRequest {
    pub source: String,
    pub current_folder: PathBuf,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LuaBookmarkResult {
    Completed,
    Failed(String),
    TimedOut,
}

pub fn execute_lua_bookmark(request: &LuaBookmarkRequest) -> LuaBookmarkResult {
    execute_lua_bookmark_with(request, Lua::new_with)
}

fn execute_lua_bookmark_with(
    request: &LuaBookmarkRequest,
    create_runtime: impl FnOnce(StdLib, LuaOptions) -> mlua::Result<Lua>,
) -> LuaBookmarkResult {
    let libraries = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8;
    let Ok(lua) = create_runtime(libraries, LuaOptions::new()) else {
        return LuaBookmarkResult::Failed("Unable to start the Lua runtime".into());
    };
    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms.max(1));
    if lua
        .set_hook(
            HookTriggers::new().every_nth_instruction(1_000),
            move |_lua, _debug| {
                if Instant::now() >= deadline {
                    Err(mlua::Error::RuntimeError(
                        "bookmark command timed out".into(),
                    ))
                } else {
                    Ok(VmState::Continue)
                }
            },
        )
        .is_err()
    {
        return LuaBookmarkResult::Failed("Unable to constrain the Lua runtime".into());
    }
    let folder = request.current_folder.to_string_lossy();
    let Ok(folder_literal) = serde_json::to_string(folder.as_ref()) else {
        return LuaBookmarkResult::Failed("Unable to encode the current folder".into());
    };
    let chunk = format!(
        "local current_folder <const> = {folder_literal}\n{}",
        request.source
    );
    match lua.load(&chunk).set_name("bookmark").exec() {
        Ok(()) => LuaBookmarkResult::Completed,
        Err(error) if error.to_string().contains("bookmark command timed out") => {
            LuaBookmarkResult::TimedOut
        }
        Err(error) => LuaBookmarkResult::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_read_only_current_folder_without_host_api() {
        let request = LuaBookmarkRequest {
            source: "assert(current_folder == [[C:\\\\fixture]]); current_folder = 'other'".into(),
            current_folder: PathBuf::from(r"C:\fixture"),
            timeout_ms: BOOKMARK_LUA_TIMEOUT_MS,
        };
        assert!(matches!(
            execute_lua_bookmark(&request),
            LuaBookmarkResult::Failed(_)
        ));
    }

    #[test]
    fn executes_with_the_physical_current_folder() {
        let request = LuaBookmarkRequest {
            source: r#"assert(current_folder == "C:\\fixture")
                assert(io == nil)
                assert(os == nil)
                assert(package == nil)
                assert(debug == nil)"#
                .into(),
            current_folder: PathBuf::from(r"C:\fixture"),
            timeout_ms: BOOKMARK_LUA_TIMEOUT_MS,
        };
        assert_eq!(execute_lua_bookmark(&request), LuaBookmarkResult::Completed);
    }

    #[test]
    fn reports_runtime_startup_failure() {
        let request = LuaBookmarkRequest {
            source: "return".into(),
            current_folder: PathBuf::from(r"C:\fixture"),
            timeout_ms: BOOKMARK_LUA_TIMEOUT_MS,
        };
        let result = execute_lua_bookmark_with(&request, |_, _| {
            Err(mlua::Error::RuntimeError("fixture startup failure".into()))
        });
        assert_eq!(
            result,
            LuaBookmarkResult::Failed("Unable to start the Lua runtime".into())
        );
    }

    #[test]
    fn reports_script_exceptions() {
        let request = LuaBookmarkRequest {
            source: "error('expected failure')".into(),
            current_folder: PathBuf::from(r"C:\fixture"),
            timeout_ms: BOOKMARK_LUA_TIMEOUT_MS,
        };
        let LuaBookmarkResult::Failed(error) = execute_lua_bookmark(&request) else {
            panic!("script exception must fail");
        };
        assert!(error.contains("expected failure"));
    }

    #[test]
    fn times_out_infinite_commands() {
        let request = LuaBookmarkRequest {
            source: "while true do end".into(),
            current_folder: PathBuf::from(r"C:\fixture"),
            timeout_ms: 1,
        };
        assert_eq!(execute_lua_bookmark(&request), LuaBookmarkResult::TimedOut);
    }
}
