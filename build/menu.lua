local initial_script_dir = arg and arg[0] and arg[0]:match("^(.*)[\\/]menu%.lua$")
if initial_script_dir then package.path = initial_script_dir .. "/lib/?.lua;" .. package.path end

local M = {}
local cli = require("cli")

M.labels = {
    "1 Build SDK",
    "2 Build Unity Android",
    "3 Build Unreal 4 Android",
    "4 Build All Android (SDK+Unity Android+UE4 Android)",
    "5 Build All Windows (SDK+Unity Windows+UE4 Windows)",
    "6 Build Everything (SDK once, Unity+UE4 Android/Windows)",
    "7 Exit",
}

local sdk = { script = "build_sdk.lua", args = {} }
local unity_android = { script = "build_unity.lua", args = { "android" } }
local unity_windows = { script = "build_unity.lua", args = { "windows" } }
local unreal4_android = { script = "build_unreal4.lua", args = { "android" } }
local unreal4_windows = { script = "build_unreal4.lua", args = { "windows" } }

local choices = {
    ["1"] = { sdk },
    ["2"] = { unity_android },
    ["3"] = { unreal4_android },
    ["4"] = { sdk, unity_android, unreal4_android },
    ["5"] = { sdk, unity_windows, unreal4_windows },
    ["6"] = { sdk, unity_android, unreal4_android, unity_windows, unreal4_windows },
}

function M.jobs_for(choice)
    if type(choice) == "string" then choice = choice:match("^%s*(.-)%s*$") end
    if choice == "7" then return nil end
    return choices[choice] or false
end

function M.execute_jobs(jobs, execute)
    for _, job in ipairs(jobs) do
        local code = execute(job)
        if code ~= 0 then return code end
    end
    return 0
end

local function quote(value)
    return '"' .. tostring(value):gsub('"', '\\"') .. '"'
end

local function process_exit_code(ok, kind, code)
    if ok == true then return 0 end
    if kind == "exit" and type(code) == "number" then return code end
    return type(ok) == "number" and ok or 1
end

function M.command_for(job)
    local script_path = M.script_dir .. package.config:sub(1, 1) .. job.script
    local command = quote(M.runtime) .. " " .. quote(script_path)
    for _, value in ipairs(job.args) do command = command .. " " .. quote(value) end
    return '"' .. command .. '"'
end

function M.play_completion_sound(success, execute_sound)
    execute_sound = execute_sound or os.execute
    local notes = success
        and "[Console]::Beep(523,120);[Console]::Beep(659,120);[Console]::Beep(784,180)"
        or "[Console]::Beep(392,160);[Console]::Beep(294,160);[Console]::Beep(196,240)"
    execute_sound("powershell -NoProfile -NonInteractive -Command " .. notes)
end

local function default_execute(job)
    io.write("\n[RUN] " .. job.script)
    if #job.args > 0 then io.write(" " .. table.concat(job.args, " ")) end
    io.write("\n")
    local ok, kind, code = os.execute(M.command_for(job))
    local exit_code = process_exit_code(ok, kind, code)
    if exit_code ~= 0 then
        io.stderr:write(string.format("[ERROR] %s failed with exit code %d\n", job.script, exit_code))
    end
    return exit_code
end

function M.run(options)
    options = options or {}
    local read = options.read or io.read
    local write = options.write or io.write
    local execute = options.execute or default_execute
    local play_sound = options.play_sound or M.play_completion_sound
    while true do
        write("\nMAGT Sample Build Menu\n")
        for _, label in ipairs(M.labels) do write(label .. "\n") end
        write("Select an option: ")
        local choice = read()
        if choice == nil then
            write("Input closed; no build was selected.\n")
            return 1
        end
        local jobs = M.jobs_for(choice)
        if jobs == nil then return 0 end
        if jobs == false then
            write("Invalid option. Enter a number from 1 to 7.\n")
        else
            local exit_code = M.execute_jobs(jobs, execute)
            pcall(play_sound, exit_code == 0)
            return exit_code
        end
    end
end

local invoked = arg and arg[0] and arg[0]:match("menu%.lua$")
if invoked then
    os.exit(cli.run(function()
        M.script_dir = assert(arg[0]:match("^(.*)[\\/]menu%.lua$"), "menu.lua must be invoked from its build directory")
        M.runtime = assert(arg[-1], "Lua runtime path is unavailable")
        return M.run()
    end))
end

return M
