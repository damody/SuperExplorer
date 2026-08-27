local M = {}

local function windows_argument(value)
    value = tostring(value)
    if value ~= "" and not value:find('[%s"&|<>%^]') then return value end
    local result, slashes = '"', 0
    for character in value:gmatch(".") do
        if character == "\\" then slashes = slashes + 1
        elseif character == '"' then
            result = result .. string.rep("\\", slashes * 2 + 1) .. '"'; slashes = 0
        else result = result .. string.rep("\\", slashes) .. character; slashes = 0 end
    end
    return result .. string.rep("\\", slashes * 2) .. '"'
end

local function command_line(spec)
    local parts = { "cd", "/d", windows_argument(spec.cwd), "&&" }
    if spec.env then
        local names = {}
        for name in pairs(spec.env) do names[#names + 1] = name end
        table.sort(names)
        for _, name in ipairs(names) do
            assert(name:match("^[%a_][%w_]*$"), "invalid environment variable name")
            parts[#parts + 1] = "set"; parts[#parts + 1] = windows_argument(name .. "=" .. tostring(spec.env[name])); parts[#parts + 1] = "&&"
        end
    end
    parts[#parts + 1] = windows_argument(spec.exe)
    for _, argument in ipairs(spec.args or {}) do parts[#parts + 1] = windows_argument(argument) end
    return table.concat(parts, " ")
end

function M.run(spec)
    assert(type(spec) == "table" and spec.stage and spec.exe and spec.cwd and spec.log_path)
    local echo_output = spec.echo_output ~= false
    local display_command = windows_argument(spec.exe)
    for _, argument in ipairs(spec.args or {}) do display_command = display_command .. " " .. windows_argument(argument) end
    local log = assert(io.open(spec.log_path, "wb"))
    local pipe = assert(io.popen(command_line(spec) .. " 2>&1", "r"))
    local tail_lines, output = {}, {}
    while true do
        local chunk = pipe:read("*L")
        if not chunk then break end
        output[#output + 1] = chunk
        local line = chunk:gsub("[\r\n]+$", "")
        tail_lines[#tail_lines + 1] = line
        if #tail_lines > 40 then table.remove(tail_lines, 1) end
        assert(log:write(chunk)); assert(log:flush())
        if echo_output then io.stdout:write(chunk); io.stdout:flush() end
    end
    local ok, _, exit_code = pipe:close()
    assert(log:close())
    if ok then return true, table.concat(output) end
    error({ stage = spec.stage, command = display_command, cwd = spec.cwd,
        exit_code = tonumber(exit_code) or 1, log_path = spec.log_path,
        tail = table.concat(tail_lines, "\n") }, 0)
end

function M.start(spec)
    assert(type(spec) == "table" and spec.stage and spec.exe and spec.cwd)
    local arguments = {}
    for _, argument in ipairs(spec.args or {}) do arguments[#arguments + 1] = windows_argument(argument) end
    local command = "cd /d " .. windows_argument(spec.cwd) .. " && start \"\" " .. windows_argument(spec.exe)
        .. (#arguments > 0 and (" " .. table.concat(arguments, " ")) or "")
    -- `os.execute` uses cmd.exe on Windows, so its built-in `start` provides a
    -- detached launch while keeping the orchestration in Lua.
    local ok, _, exit_code = os.execute("(" .. command .. ") >nul 2>&1")
    if ok then return true end
    error({ stage = spec.stage, command = command, cwd = spec.cwd,
        exit_code = tonumber(exit_code) or 1, tail = "Windows rejected the interactive process launch" }, 0)
end

return M
