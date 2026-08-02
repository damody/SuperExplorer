-- 建置 SuperExplorer 發行版執行檔，並使用 NSIS 封裝安裝程式。
local script_dir = assert(
    arg[0]:match("^(.*)[\\/]build_install%.lua$"),
    "build_install.lua 必須從 build 目錄執行"
)
package.path = script_dir .. "/lib/?.lua;" .. package.path
package.cpath = script_dir .. "/tools/lua/?.dll;" .. package.cpath

local fs = require("fs")
local lfs = require("lfs")
local path_util = require("path")
local process = require("process")
local sdk_version = require("sdk_version")

if script_dir == "build" then
    script_dir = path_util.join(lfs.currentdir(), "build")
end

local root = assert(
    script_dir:match("^(.*)[\\/]build$"),
    "build_install.lua 必須位於版本庫根目錄下的 build 目錄"
)

local function path(...)
    return path_util.join(...)
end

local function require_file(file_path, description)
    if lfs.attributes(file_path, "mode") ~= "file" then
        error((description or "必要檔案") .. "不存在：" .. file_path, 0)
    end
    return file_path
end

local function read_file(file_path)
    local file = assert(io.open(file_path, "rb"))
    local contents = assert(file:read("*a"))
    assert(file:close())
    return contents
end

local function commit_version()
    local metadata = sdk_version.resolve(root)
    local year, month, day = metadata.iso_date:match("^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
    if not year then error("無法從 HEAD commit 日期產生安裝程式版本", 0) end
    return string.format("1.%d.%d.%d", tonumber(year), tonumber(month), tonumber(day))
end

local function reject_uncommitted_rust(logs)
    local status_log = path(logs, "installer-rust-status.log")
    process.run({
        stage = "檢查 Rust 原始碼是否已提交",
        exe = "git.exe",
        args = {
            "status", "--porcelain=v1", "--untracked-files=all", "--",
            "*.rs", ":(exclude)sdk/**",
        },
        cwd = root,
        log_path = status_log,
    })

    local dirty = {}
    for line in read_file(status_log):gmatch("[^\r\n]+") do
        dirty[#dirty + 1] = line
    end
    if #dirty > 0 then
        error(
            "下列 Rust 原始碼尚未提交，禁止編譯安裝程式：\n  "
                .. table.concat(dirty, "\n  "),
            0
        )
    end
end

local function strip_quotes(value)
    return value:match('^%s*"(.-)"%s*$') or value:match("^%s*(.-)%s*$")
end

local function find_on_path(executable)
    for directory in (os.getenv("PATH") or ""):gmatch("[^;]+") do
        local candidate = path(strip_quotes(directory), executable)
        if lfs.attributes(candidate, "mode") == "file" then return candidate end
    end
end

local function find_makensis()
    local candidates = { find_on_path("makensis.exe") }
    for _, environment_name in ipairs({ "ProgramFiles(x86)", "ProgramFiles" }) do
        local base = os.getenv(environment_name)
        if base and base ~= "" then
            candidates[#candidates + 1] = path(base, "NSIS", "makensis.exe")
        end
    end
    for _, candidate in ipairs(candidates) do
        if candidate and lfs.attributes(candidate, "mode") == "file" then return candidate end
    end
    error("在 PATH 與標準 NSIS 安裝目錄中都找不到 makensis.exe", 0)
end

local function parse_options()
    local options = { check = false, skip_build = false }
    for index = 1, #arg do
        if arg[index] == "--check" then
            options.check = true
        elseif arg[index] == "--skip-build" then
            options.skip_build = true
        else
            error("未知參數：" .. tostring(arg[index]), 0)
        end
    end
    return options
end

local function validate_executable(file_path, description)
    require_file(file_path, description)
    local file = assert(io.open(file_path, "rb"))
    local signature = file:read(2)
    local size = assert(file:seek("end"))
    assert(file:close())
    if signature ~= "MZ" or size < 1024 then
        error(description .. "不是有效的 Windows 執行檔：" .. file_path, 0)
    end
    return size
end

local function main()
    local options = parse_options()
    local version = commit_version()
    local logs = path(script_dir, "logs")

    print("SuperExplorer 安裝程式建置")
    print("版本庫：" .. root)
    print("版本：" .. version)
    print("Lua 執行環境：" .. tostring(arg[-1]))

    fs.mkdir_p(logs)
    reject_uncommitted_rust(logs)

    local makensis = find_makensis()
    local finalizer = require_file(
        path(root, "scripts", "finalize_windows_artifact.ps1"),
        "發行版最終處理腳本"
    )
    local nsis_script = require_file(
        path(root, "installer", "SuperExplorer.nsi"),
        "NSIS 腳本"
    )
    local release_executable = path(root, "target", "release", "SuperExplorer.exe")
    local broker_executable = path(root, "target", "release", "explorer-extension-broker.exe")
    local worker_executable = path(root, "target", "release", "explorer-extension-worker.exe")
    local dist = path(root, "dist")
    local output = path(dist, "SuperExplorer-Setup-" .. version .. "-x64.exe")

    print("NSIS 執行環境：" .. makensis)

    if options.check then
        print("[完成] 安裝程式建置輸入與工具皆可使用")
        return 0
    end

    fs.mkdir_p(dist)

    if not options.skip_build then
        process.run({
            stage = "建置並驗證發行版執行檔",
            exe = "powershell.exe",
            args = {
                "-NoLogo", "-NoProfile", "-NonInteractive",
                "-ExecutionPolicy", "Bypass",
                "-File", finalizer,
                "-Profile", "release",
            },
            cwd = root,
            log_path = path(logs, "installer-release.log"),
        })
    end

    local application_size = validate_executable(release_executable, "發行版執行檔")
    validate_executable(broker_executable, "extension broker")
    validate_executable(worker_executable, "extension worker")
    os.remove(output)
    process.run({
        stage = "編譯 NSIS 安裝程式",
        exe = makensis,
        args = {
            "/V4", "/WX",
            "/DAPP_VERSION=" .. version,
            "/DAPP_EXE=" .. release_executable,
            "/DBROKER_EXE=" .. broker_executable,
            "/DWORKER_EXE=" .. worker_executable,
            "/DOUTPUT_FILE=" .. output,
            nsis_script,
        },
        cwd = root,
        log_path = path(logs, "installer-nsis.log"),
    })
    local installer_size = validate_executable(output, "安裝程式")

    process.start({
        stage = "啟動安裝程式",
        exe = output,
        cwd = dist,
    })

    print(string.format("[完成] 應用程式：%s（%d 位元組）", release_executable, application_size))
    print(string.format("[完成] 安裝程式：%s（%d 位元組）", output, installer_size))
    print("[完成] 已啟動本次建置的 SuperExplorer 安裝程式")
    return 0
end

local function format_failure(failure)
    if type(failure) ~= "table" then return "[錯誤] " .. tostring(failure) end
    local lines = {
        "[錯誤] 子程序執行失敗",
        "階段：" .. tostring(failure.stage or "未知"),
        "命令：" .. tostring(failure.command or "未知"),
        "工作目錄：" .. tostring(failure.cwd or "未知"),
        "結束代碼：" .. tostring(failure.exit_code or "未知"),
        "記錄檔：" .. tostring(failure.log_path or "未知"),
    }
    if failure.tail and failure.tail ~= "" then
        lines[#lines + 1] = "輸出末尾："
        lines[#lines + 1] = failure.tail
    end
    return table.concat(lines, "\n")
end

local function run_main()
    local ok, result = pcall(main)
    if ok then return type(result) == "number" and result or 0 end
    io.stderr:write(format_failure(result) .. "\n")
    return type(result) == "table" and tonumber(result.exit_code) or 1
end

os.exit(run_main())
