local workspace, run_directory, raw_log, runner_exit = ...

local function fail(message)
    io.stderr:write("[UTIT] " .. message .. "\n")
    os.exit(2)
end

if not workspace or not run_directory or not raw_log then
    fail("usage: utit_report.lua <workspace> <run-directory> <raw-log> <runner-exit>")
end

local function join(left, right)
    local separator = left:sub(-1) == "\\" and "" or "\\"
    return left .. separator .. right:gsub("/", "\\")
end

local function read_all(path)
    local file = io.open(path, "rb")
    if not file then return nil end
    local value = file:read("*a")
    file:close()
    return value
end

local function write_line(file, value)
    file:write(value or "", "\r\n")
end

-- Small dependency-free JSON decoder. The runner owns report.json, so rejecting
-- malformed input is preferable to silently producing an incomplete UTIT log.
local function decode_json(text)
    local position = 1

    local function skip_space()
        local _, finish = text:find("^[ \t\r\n]*", position)
        position = (finish or position - 1) + 1
    end

    local parse_value

    local function parse_string()
        if text:sub(position, position) ~= '"' then error("expected string") end
        position = position + 1
        local pieces = {}
        while position <= #text do
            local character = text:sub(position, position)
            if character == '"' then
                position = position + 1
                return table.concat(pieces)
            elseif character == "\\" then
                local escape = text:sub(position + 1, position + 1)
                local replacements = {
                    ['"'] = '"', ['\\'] = '\\', ['/'] = '/',
                    b = '\b', f = '\f', n = '\n', r = '\r', t = '\t'
                }
                if escape == "u" then
                    local hex = text:sub(position + 2, position + 5)
                    local codepoint = tonumber(hex, 16)
                    if not codepoint then error("invalid unicode escape") end
                    pieces[#pieces + 1] = utf8.char(codepoint)
                    position = position + 6
                elseif replacements[escape] then
                    pieces[#pieces + 1] = replacements[escape]
                    position = position + 2
                else
                    error("invalid string escape")
                end
            else
                pieces[#pieces + 1] = character
                position = position + 1
            end
        end
        error("unterminated string")
    end

    local function parse_array()
        position = position + 1
        local result = {}
        skip_space()
        if text:sub(position, position) == "]" then
            position = position + 1
            return result
        end
        while true do
            result[#result + 1] = parse_value()
            skip_space()
            local delimiter = text:sub(position, position)
            if delimiter == "]" then
                position = position + 1
                return result
            end
            if delimiter ~= "," then error("expected array delimiter") end
            position = position + 1
        end
    end

    local function parse_object()
        position = position + 1
        local result = {}
        skip_space()
        if text:sub(position, position) == "}" then
            position = position + 1
            return result
        end
        while true do
            skip_space()
            local key = parse_string()
            skip_space()
            if text:sub(position, position) ~= ":" then error("expected object colon") end
            position = position + 1
            result[key] = parse_value()
            skip_space()
            local delimiter = text:sub(position, position)
            if delimiter == "}" then
                position = position + 1
                return result
            end
            if delimiter ~= "," then error("expected object delimiter") end
            position = position + 1
        end
    end

    function parse_value()
        skip_space()
        local character = text:sub(position, position)
        if character == '"' then return parse_string() end
        if character == "{" then return parse_object() end
        if character == "[" then return parse_array() end
        if text:sub(position, position + 3) == "true" then position = position + 4; return true end
        if text:sub(position, position + 4) == "false" then position = position + 5; return false end
        if text:sub(position, position + 3) == "null" then position = position + 4; return nil end
        local number = text:match("^-?%d+%.?%d*[eE]?[+-]?%d*", position)
        if number and number ~= "" then
            position = position + #number
            return tonumber(number)
        end
        error("unexpected JSON value at byte " .. position)
    end

    local value = parse_value()
    skip_space()
    if position <= #text then error("trailing JSON input") end
    return value
end

local now = os.date("*t")
local log_name = string.format("UTIT-%d-%d-%d.log", now.year, now.month, now.day)
local log_path = join(workspace, log_name)
local output, open_error = io.open(log_path, "wb")
if not output then fail("cannot create " .. log_path .. ": " .. tostring(open_error)) end

write_line(output, "UTIT Windows Explorer regression report")
write_line(output, string.format("Date: %d-%d-%d", now.year, now.month, now.day))
write_line(output, "Run directory: " .. run_directory)
write_line(output, "Runner exit code: " .. tostring(runner_exit or "unknown"))
write_line(output, string.rep("=", 78))

local report_path = join(join(workspace, run_directory), "report.json")
local report_text = read_all(report_path)
if not report_text then
    write_line(output, "[ERROR] UITEST runner")
    write_line(output, "Reason: report.json was not produced")
    write_line(output, "")
    write_line(output, "Runner console output:")
    write_line(output, read_all(join(workspace, raw_log)) or "<runner console log unavailable>")
    write_line(output, "")
    write_line(output, "FINAL STATISTICS")
    write_line(output, "PASS=0 FAIL=0 SKIP=0 TIMEOUT=0 ERROR=1 TOTAL=1")
    output:close()
    print("UTIT log: " .. log_path)
    os.exit(2)
end

local ok, report = pcall(decode_json, report_text)
if not ok then
    write_line(output, "[ERROR] UITEST report parser")
    write_line(output, "Reason: " .. tostring(report))
    write_line(output, "Report: " .. report_path)
    write_line(output, "")
    write_line(output, "FINAL STATISTICS")
    write_line(output, "PASS=0 FAIL=0 SKIP=0 TIMEOUT=0 ERROR=1 TOTAL=1")
    output:close()
    print("UTIT log: " .. log_path)
    os.exit(2)
end

local counts = { PASS = 0, FAIL = 0, SKIP = 0, TIMEOUT = 0, ERROR = 0 }
local failures = 0

local function append_file(label, relative_path)
    write_line(output, label .. ": " .. tostring(relative_path or "<none>"))
    if not relative_path then return end
    local contents = read_all(join(join(workspace, run_directory), relative_path))
    if contents and contents ~= "" then
        write_line(output, "----- " .. label .. " BEGIN -----")
        output:write(contents)
        if contents:sub(-1) ~= "\n" then output:write("\r\n") end
        write_line(output, "----- " .. label .. " END -----")
    end
end

for _, result in ipairs(report.results or {}) do
    local status = result.status or "ERROR"
    if counts[status] == nil then status = "ERROR" end
    counts[status] = counts[status] + 1
    -- Case identifiers are stable ASCII titles. Some legacy manifest descriptions were
    -- authored through a mismatched Windows code page; using the identifier keeps the report
    -- readable and deterministic without attempting lossy encoding repair.
    local title = string.format("[%s] %s", status, result.id or "unknown")
    write_line(output, title)

    if status ~= "PASS" then
        failures = failures + 1
        write_line(output, "Status: " .. status)
        write_line(output, "Reason: " .. tostring(result.terminal_reason or "No terminal reason was recorded"))
        write_line(output, "Command: " .. tostring(result.command or "<none>"))
        write_line(output, "Exit code: " .. tostring(result.exit_code or "<none>"))
        write_line(output, "Duration: " .. tostring(result.duration_ms or 0) .. " ms")
        write_line(output, "Rerun: " .. tostring(result.rerun_command or "<none>"))
        write_line(output, "Evidence: " .. tostring(result.evidence_directory or "<none>"))
        if result.process then
            write_line(output, "Launched PID: " .. tostring(result.process.launched_pid or "<none>"))
            write_line(output, "Cleanup attempted: " .. tostring(result.process.cleanup_attempted or false))
            write_line(output, "Residual PID count: " .. tostring(#(result.process.residual_pids or {})))
        end
        if result.artifacts and #result.artifacts > 0 then
            write_line(output, "Artifacts: " .. table.concat(result.artifacts, ", "))
        end
        append_file("STDOUT", result.stdout)
        append_file("STDERR", result.stderr)
        write_line(output, string.rep("-", 78))
    end
end

local total = counts.PASS + counts.FAIL + counts.SKIP + counts.TIMEOUT + counts.ERROR
write_line(output, "")
write_line(output, "FINAL STATISTICS")
write_line(output, string.format(
    "PASS=%d FAIL=%d SKIP=%d TIMEOUT=%d ERROR=%d TOTAL=%d",
    counts.PASS, counts.FAIL, counts.SKIP, counts.TIMEOUT, counts.ERROR, total
))
write_line(output, string.format("SUCCESS=%d NON_SUCCESS=%d", counts.PASS, failures))
output:close()

print("UTIT log: " .. log_path)
print(string.format(
    "UTIT statistics: PASS=%d FAIL=%d SKIP=%d TIMEOUT=%d ERROR=%d TOTAL=%d",
    counts.PASS, counts.FAIL, counts.SKIP, counts.TIMEOUT, counts.ERROR, total
))
