local M = {}
local sensitive_paths = require("sensitive_paths")

local rejected_names = {
    [".git"] = true, [".vs"] = true, [".claude"] = true, [".gradle"] = true,
    binaries = true, intermediate = true, deriveddatacache = true, saved = true,
    binary_cache = true, windowsnoeditor = true, license = true, request = true,
}

function M.include(relative_path, mode)
    local normalized = tostring(relative_path):gsub("\\", "/")
    if sensitive_paths.is_sensitive(normalized) then return false end
    for component in normalized:gmatch("[^/]+") do
        local lower = component:lower()
        if rejected_names[lower] or lower:match("^android_astc") then return false end
    end
    return true
end

return M
