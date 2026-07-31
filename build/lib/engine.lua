local lfs = require("lfs")

local M = {}
local sep = package.config:sub(1, 1)
local function join(...)
    return table.concat({...}, sep)
end
local function exists(path)
    return lfs.attributes(path, "mode") == "file"
end
local function read(path)
    local file = assert(io.open(path, "rb"), "unable to read " .. path)
    local content = file:read("*a"); file:close(); return content
end

function M.find_unity(project_root, env)
    env = env or {}
    local version_file = join(project_root, "ProjectSettings", "ProjectVersion.txt")
    local version = read(version_file):match("m_EditorVersion:%s*([^%s]+)")
    assert(version, "unable to determine Unity editor version from " .. version_file)
    local candidates = {}
    if env.UNITY_ROOT then
        candidates[#candidates + 1] = join(env.UNITY_ROOT, "Editor", "Unity.exe")
        candidates[#candidates + 1] = join(env.UNITY_ROOT, "Unity.exe")
    end
    local hub = env.UNITY_HUB_ROOT or join(env.PROGRAM_FILES or os.getenv("ProgramFiles") or "C:\\Program Files", "Unity", "Hub", "Editor")
    candidates[#candidates + 1] = join(hub, version, "Editor", "Unity.exe")
    for _, candidate in ipairs(candidates) do if exists(candidate) then return candidate end end
    error("Unity " .. version .. " was not found; set UNITY_ROOT or UNITY_HUB_ROOT", 0)
end

local function registry_root(association)
    local key = 'HKCU\\Software\\Epic Games\\Unreal Engine\\Builds'
    local command = 'reg.exe query "' .. key .. '" /v "' .. association .. '" 2>NUL'
    local pipe = io.popen(command, "r")
    if not pipe then return nil end
    local output = pipe:read("*a"); pipe:close()
    return output:match("REG_SZ%s+([^\r\n]+)")
end

function M.find_unreal(uproject, major, env)
    env = env or {}
    assert(major == 4 or major == 5, "Unreal major version must be 4 or 5")
    local variable = "UE" .. major .. "_ROOT"
    local association = read(uproject):match('"EngineAssociation"%s*:%s*"([^"]+)"')
    assert(association, "unable to read EngineAssociation from " .. uproject)
    local root = env[variable]
    if not root then root = env.registry and env.registry[association] or registry_root(association) end
    if not root and association:match("^" .. major .. "[%.%d]*$") then
        root = join(env.PROGRAM_FILES or os.getenv("ProgramFiles") or "C:\\Program Files", "Epic Games", "UE_" .. association)
    end
    local editor = root and join(root, "Engine", "Binaries", "Win64", major == 4 and "UE4Editor.exe" or "UnrealEditor.exe")
    if editor and exists(editor) then return root end
    error("Unreal Engine " .. major .. " was not found for " .. uproject .. "; set " .. variable .. " to the engine root", 0)
end

return M
