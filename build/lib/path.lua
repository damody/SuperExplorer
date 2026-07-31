local M = {}

function M.join(...)
    local joined = table.concat({...}, "\\"):gsub("\\+", "\\")
    return joined
end

return M
