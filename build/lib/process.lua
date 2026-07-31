local M = {}

local function powershell_literal(value)
    return "'" .. tostring(value):gsub("'", "''") .. "'"
end

local function windows_argument(value)
    value = tostring(value)
    if value ~= "" and not value:find('[%s"]') then return value end
    local result, slashes = '"', 0
    for character in value:gmatch(".") do
        if character == "\\" then
            slashes = slashes + 1
        elseif character == '"' then
            result = result .. string.rep("\\", slashes * 2 + 1) .. '"'
            slashes = 0
        else
            result = result .. string.rep("\\", slashes) .. character
            slashes = 0
        end
    end
    return result .. string.rep("\\", slashes * 2) .. '"'
end

local base64_alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
local function base64(data)
    local result = {}
    for index = 1, #data, 3 do
        local a, b, c = data:byte(index, index + 2)
        local value = a * 65536 + (b or 0) * 256 + (c or 0)
        result[#result + 1] = base64_alphabet:sub((value >> 18) + 1, (value >> 18) + 1)
        result[#result + 1] = base64_alphabet:sub(((value >> 12) & 63) + 1, ((value >> 12) & 63) + 1)
        result[#result + 1] = b and base64_alphabet:sub(((value >> 6) & 63) + 1, ((value >> 6) & 63) + 1) or "="
        result[#result + 1] = c and base64_alphabet:sub((value & 63) + 1, (value & 63) + 1) or "="
    end
    return table.concat(result)
end

local function utf16le(value)
    local bytes = {}
    for _, codepoint in utf8.codes(value) do
        if codepoint <= 0xffff then
            bytes[#bytes + 1] = string.char(codepoint & 255, codepoint >> 8)
        else
            codepoint = codepoint - 0x10000
            local high, low = 0xd800 + (codepoint >> 10), 0xdc00 + (codepoint & 0x3ff)
            bytes[#bytes + 1] = string.char(high & 255, high >> 8, low & 255, low >> 8)
        end
    end
    return table.concat(bytes)
end

local function encoded_powershell_command(lines)
    local encoded = base64(utf16le(table.concat(lines, "\r\n")))
    return "powershell.exe -NoLogo -NoProfile -NonInteractive -OutputFormat Text -EncodedCommand " .. encoded
end

local function encoded_arguments(values)
    local result = {}
    for _, value in ipairs(values or {}) do result[#result + 1] = windows_argument(value) end
    return result
end

function M.run(spec)
    assert(type(spec) == "table" and spec.stage and spec.exe and spec.cwd and spec.log_path)
    local script = {
        "$ErrorActionPreference = 'Stop'",
        "$ProgressPreference = 'SilentlyContinue'",
        "Add-Type -TypeDefinition @'",
        "using System;",
        "using System.Diagnostics;",
        "public static class CodexProcessRunner {",
        "  public static int Run(ProcessStartInfo info) {",
        "    using (var process = new Process()) {",
        "      process.StartInfo = info;",
        "      process.OutputDataReceived += (sender, e) => { if (e.Data != null) { Console.Out.WriteLine(e.Data); Console.Out.Flush(); } };",
        "      process.ErrorDataReceived += (sender, e) => { if (e.Data != null) { Console.Out.WriteLine(e.Data); Console.Out.Flush(); } };",
        "      process.Start(); process.BeginOutputReadLine(); process.BeginErrorReadLine(); process.WaitForExit(); process.WaitForExit();",
        "      return process.ExitCode;",
        "    }",
        "  }",
        "}",
        "'@",
        "$psi = New-Object System.Diagnostics.ProcessStartInfo",
        "$psi.UseShellExecute = $false",
        "$psi.RedirectStandardOutput = $true",
        "$psi.RedirectStandardError = $true",
        "$psi.CreateNoWindow = $true",
        "$psi.FileName = " .. powershell_literal(spec.exe),
        "$psi.WorkingDirectory = " .. powershell_literal(spec.cwd),
    }
    local arguments = encoded_arguments(spec.args)
    local display_command = windows_argument(spec.exe) .. (#arguments > 0 and " " .. table.concat(arguments, " ") or "")
    script[#script + 1] = "$psi.Arguments = " .. powershell_literal(table.concat(arguments, " "))
    if spec.env then
        for key, value in pairs(spec.env) do
            assert(tostring(key):match("^[%a_][%w_]*$"), "invalid environment variable name")
            script[#script + 1] = "$psi.EnvironmentVariables[" .. powershell_literal(key) .. "] = " .. powershell_literal(value)
        end
    end
    script[#script + 1] = "exit [CodexProcessRunner]::Run($psi)"
    local command_text = encoded_powershell_command(script)
    local log = assert(io.open(spec.log_path, "wb"))
    local pipe = assert(io.popen(command_text .. " 2>&1", "r"))
    local tail_lines = {}
    while true do
        local chunk = pipe:read("*L")
        if not chunk then break end
        local line = chunk:gsub("[\r\n]+$", "")
        tail_lines[#tail_lines + 1] = line
        if #tail_lines > 40 then table.remove(tail_lines, 1) end
        assert(log:write(chunk)); assert(log:flush())
        io.stdout:write(chunk); io.stdout:flush()
    end
    local ok, _, exit_code = pipe:close()
    assert(log:close())
    if ok then exit_code = 0 end
    if exit_code == 0 then return true end
    error({
        stage = spec.stage, command = display_command, cwd = spec.cwd,
        exit_code = exit_code, log_path = spec.log_path, tail = table.concat(tail_lines, "\n"),
    }, 0)
end

function M.start(spec)
    assert(type(spec) == "table" and spec.stage and spec.exe and spec.cwd)
    local arguments = encoded_arguments(spec.args)
    local display_command = windows_argument(spec.exe)
        .. (#arguments > 0 and " " .. table.concat(arguments, " ") or "")
    local script = {
        "$ErrorActionPreference = 'Stop'",
        "$ProgressPreference = 'SilentlyContinue'",
        "$psi = New-Object System.Diagnostics.ProcessStartInfo",
        "$psi.UseShellExecute = $true",
        "$psi.FileName = " .. powershell_literal(spec.exe),
        "$psi.WorkingDirectory = " .. powershell_literal(spec.cwd),
        "$psi.Arguments = " .. powershell_literal(table.concat(arguments, " ")),
        "$process = [System.Diagnostics.Process]::Start($psi)",
        "if ($null -eq $process) { throw 'Windows returned no process handle' }",
        "$process.Dispose()",
    }
    local ok, _, exit_code = os.execute(encoded_powershell_command(script) .. " >nul 2>&1")
    if ok then return true end
    error({
        stage = spec.stage,
        command = display_command,
        cwd = spec.cwd,
        exit_code = tonumber(exit_code) or 1,
        tail = "Windows rejected the interactive process launch",
    }, 0)
end

return M
