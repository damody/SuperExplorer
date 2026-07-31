local lfs = require("lfs")
local fs = require("fs")
local M = {}

local function backup_name(path)
    local candidate, number = path .. ".backup", 0
    while lfs.attributes(candidate) do number = number + 1; candidate = path .. ".backup." .. number end
    return candidate
end

local function replace(temp_path, final_path)
    local backup
    if lfs.attributes(final_path) then
        backup = backup_name(final_path)
        assert(os.rename(final_path, backup), "could not create publication backup")
    end
    local ok, message = os.rename(temp_path, final_path)
    if not ok then
        if backup then assert(os.rename(backup, final_path), "publication failed and backup restoration failed") end
        error("could not publish output: " .. tostring(message), 0)
    end
    if backup then fs.remove_tree(backup) end
    return true
end

function M.apk(temp_file, final_file)
    local attributes = lfs.attributes(temp_file)
    assert(attributes and attributes.mode == "file" and attributes.size > 0, "APK is missing or empty")
    return replace(temp_file, final_file)
end

function M.windows(temp_dir, final_dir, exe_name)
    local attributes = lfs.attributes(temp_dir)
    assert(attributes and attributes.mode == "directory", "Windows package is missing")
    local exe = temp_dir .. package.config:sub(1, 1) .. exe_name
    local exe_attributes = lfs.attributes(exe)
    assert(exe_attributes and exe_attributes.mode == "file", "Windows package executable is missing: " .. exe_name)
    return replace(temp_dir, final_dir)
end

return M
