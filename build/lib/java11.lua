local lfs = require("lfs")

local M = {}

local function join(left, right)
    return left:gsub("[\\/]+$", "") .. "\\" .. right
end

local function default_is_file(path)
    local file = io.open(path, "rb")
    if not file then return false end
    file:close()
    return true
end

local function default_read_file(path)
    local file = io.open(path, "rb")
    if not file then return nil end
    local content = file:read("*a")
    file:close()
    return content
end

local function default_list_directories(parent)
    local result = {}
    if lfs.attributes(parent, "mode") ~= "directory" then return result end
    for name in lfs.dir(parent) do
        if name ~= "." and name ~= ".." then
            local candidate = join(parent, name)
            if lfs.attributes(candidate, "mode") == "directory" then result[#result + 1] = candidate end
        end
    end
    return result
end

local function major_version(home, dependencies)
    if not dependencies.is_file(join(home, "bin\\java.exe")) then return nil end
    local release = dependencies.read_file(join(home, "release"))
    local version = release and release:match('JAVA_VERSION="([^"]+)"')
    if not version then return nil end
    local legacy, current = version:match("^(%d+)%.(%d+)")
    return tonumber(legacy) == 1 and tonumber(current) or tonumber(legacy)
end

function M.resolve(dependencies)
    dependencies = dependencies or {}
    dependencies.getenv = dependencies.getenv or os.getenv
    dependencies.is_file = dependencies.is_file or default_is_file
    dependencies.read_file = dependencies.read_file or default_read_file
    dependencies.list_directories = dependencies.list_directories or default_list_directories

    local override = dependencies.getenv("MAGT_JAVA_HOME")
    if override then
        if major_version(override, dependencies) == 11 then return override end
        error("MAGT_JAVA_HOME must point to a Java 11 JDK: " .. override, 0)
    end

    local inherited = dependencies.getenv("JAVA_HOME")
    if inherited and major_version(inherited, dependencies) == 11 then return inherited end

    local program_files = dependencies.getenv("ProgramFiles") or [[C:\Program Files]]
    local candidates = dependencies.list_directories(join(program_files, "Eclipse Adoptium"))
    table.sort(candidates, function(left, right) return left:lower() > right:lower() end)
    for _, candidate in ipairs(candidates) do
        local name = candidate:match("[^\\/]+$") or ""
        if name:lower():match("^jdk%-11") and major_version(candidate, dependencies) == 11 then
            return candidate
        end
    end

    error("Java 11 JDK not found. Install Eclipse Temurin 11 or set MAGT_JAVA_HOME to a Java 11 JDK.", 0)
end

return M
