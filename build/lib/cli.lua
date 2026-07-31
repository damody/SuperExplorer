local M = {}

function M.format(failure)
    if type(failure) ~= "table" then return "[ERROR] " .. tostring(failure) end
    local lines = {
        "[ERROR] Child process failed",
        "Stage: " .. tostring(failure.stage or "unknown"),
        "Command: " .. tostring(failure.command or "unknown"),
        "Working directory: " .. tostring(failure.cwd or "unknown"),
        "Exit code: " .. tostring(failure.exit_code or "unknown"),
        "Log: " .. tostring(failure.log_path or "unknown"),
    }
    if failure.tail and failure.tail ~= "" then
        lines[#lines + 1] = "Output tail:"
        lines[#lines + 1] = failure.tail
    end
    return table.concat(lines, "\n")
end

function M.run(callback, stderr)
    stderr = stderr or io.stderr
    local ok, result = pcall(callback)
    if ok then return type(result) == "number" and result or 0 end
    stderr:write(M.format(result) .. "\n")
    return type(result) == "table" and tonumber(result.exit_code) or 1
end

return M
