local M = {}

function M.is_sdk_source(path)
    local normalized = path:gsub("\\", "/")
    if normalized:match("^magt_sdk/ue_stub/") then return false end
    local in_sdk = normalized:match("^magt_sdk/") or normalized:match("^magt_queued_buffer_sdk/")
    local extension = normalized:lower():match("%.([^.]+)$")
    local allowed = extension == "rs" or extension == "java" or extension == "h" or extension == "cpp"
    return not not (in_sdk and allowed)
end

function M.filter(paths)
    local result = {}
    for _, path in ipairs(paths) do
        if M.is_sdk_source(path) then result[#result + 1] = path end
    end
    return result
end

return M
