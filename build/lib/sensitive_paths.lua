local M = {}

local sensitive_extensions = {
    keystore = true,
    jks = true,
    p12 = true,
    pfx = true,
    der = true,
    bytes = true,
    zip = true,
    ["7z"] = true,
    rar = true,
    tar = true,
    gz = true,
    tgz = true,
}

local sensitive_directories = {
    magtlicense = true,
    request = true,
    license = true,
    signature = true,
    archive = true,
    archives = true,
    credential = true,
    credentials = true,
    secrets = true,
    secret = true,
    signing = true,
}

local game_markers = {"kurogame", "mingchao", "wutheringwaves"}

function M.is_sensitive(path)
    local normalized = path:gsub("\\", "/"):lower()
    for part in normalized:gmatch("[^/]+") do
        if sensitive_directories[part] then return true end
    end
    local extension = normalized:match("%.([^./]+)$")
    if sensitive_extensions[extension] then return true end
    if normalized:match("%.bytes%.base64$") then return true end
    for _, marker in ipairs(game_markers) do
        if normalized:find(marker, 1, true) then return true end
    end
    return false
end

return M
