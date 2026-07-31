package.path = "build/lib/?.lua;" .. package.path
package.cpath = "build/tools/lua/?.dll;" .. package.cpath

local unreal_build = require("unreal_build")
local cli = require("cli")
local sep = package.config:sub(1, 1)
local M = {
    project = "Unreal4ARPG" .. sep .. "ARPG4.uproject",
    engine_major = 4,
    app_basename = "MagtUnreal4Demo",
    package_executable = "ActionRPG",
}

function M.run(platform, check_only)
    return unreal_build.run({
        project = M.project,
        engine_major = M.engine_major,
        app_basename = M.app_basename,
        package_executable = M.package_executable,
        platform = platform,
        check_only = check_only,
    })
end

if not rawget(_G, "MAGT_BUILD_UNREAL4_TEST") then
    local platform, option = arg[1], arg[2]
    os.exit(cli.run(function()
        assert(option == nil or option == "--check-only",
            "usage: build_unreal4.lua android|windows [--check-only]")
        M.run(platform, option == "--check-only")
    end))
end

return M
