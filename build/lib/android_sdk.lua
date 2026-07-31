local lfs = require("lfs")

local M = {}

local function join(left, right)
    return left:gsub("[\\/]+$", "") .. "\\" .. right
end

local function default_is_directory(path)
    return lfs.attributes(path, "mode") == "directory"
end

local function valid(root, is_directory)
    return root
        and is_directory(root)
        and is_directory(join(root, "platform-tools"))
        and is_directory(join(root, "platforms"))
end

function M.resolve(dependencies)
    dependencies = dependencies or {}
    local getenv = dependencies.getenv or os.getenv
    local is_directory = dependencies.is_directory or default_is_directory

    for _, name in ipairs({"ANDROID_SDK_ROOT", "ANDROID_HOME"}) do
        local candidate = getenv(name)
        if candidate then
            if valid(candidate, is_directory) then return candidate end
            error(name .. " does not point to a valid Android SDK: " .. candidate, 0)
        end
    end

    local local_app_data = getenv("LOCALAPPDATA")
    local fallback = local_app_data and join(local_app_data, "Android\\Sdk")
    if valid(fallback, is_directory) then return fallback end

    error("Android SDK not found. Set ANDROID_SDK_ROOT or install it under %LOCALAPPDATA%\\Android\\Sdk.", 0)
end

return M
