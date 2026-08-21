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
local publish = require("publish")
local sdk_version = require("sdk_version")
local installer_components = require("installer_components")

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

local function write_file(file_path, contents)
    local file = assert(io.open(file_path, "wb"))
    assert(file:write(contents))
    assert(file:close())
end

local function powershell_literal(value)
    return "'" .. tostring(value):gsub("'", "''") .. "'"
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
            "*.rs",
            ":(exclude)sdk/**",
            ":(exclude)openspec/**/evidence/**",
            ":(exclude)**/utit-results/**",
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
    return installer_components.parse_options(arg)
end

local function run_capture(stage, cwd, log_path, args)
    process.run({ stage = stage, exe = "git.exe", args = args, cwd = cwd, log_path = log_path })
    return read_file(log_path):match("^%s*(.-)%s*$")
end

local function admit_superdesktop(superdesktop_root, logs, formal, ignore_openspec_untracked)
    require_file(path(superdesktop_root, "Cargo.toml"), "SuperDesktop Cargo manifest")
    if not lfs.attributes(path(superdesktop_root, ".git")) then
        error("SuperDesktop submodule 尚未初始化", 0)
    end
    local head = run_capture(
        "讀取 SuperDesktop HEAD", superdesktop_root,
        path(logs, "installer-superdesktop-head.log"), { "rev-parse", "HEAD" }
    )
    if not head:match("^[0-9a-fA-F]+$") then error("SuperDesktop HEAD 無效", 0) end
    if not formal then return head end

    local declared_url = run_capture(
        "讀取 SuperDesktop .gitmodules URL", root,
        path(logs, "installer-superdesktop-declared-url.log"),
        { "config", "-f", ".gitmodules", "--get", "submodule.SuperDesktop.url" }
    )
    local configured_url = run_capture(
        "讀取 SuperDesktop submodule URL", root,
        path(logs, "installer-superdesktop-configured-url.log"),
        { "config", "--get", "submodule.SuperDesktop.url" }
    )
    local origin_url = run_capture(
        "讀取 SuperDesktop origin", superdesktop_root,
        path(logs, "installer-superdesktop-origin.log"), { "remote", "get-url", "origin" }
    )
    local tree_entry = run_capture(
        "讀取 SuperDesktop parent gitlink", root,
        path(logs, "installer-superdesktop-gitlink.log"), { "ls-files", "--stage", "--", "SuperDesktop" }
    )
    local mode, gitlink = tree_entry:match("^(%d+)%s+([0-9a-fA-F]+)%s+0%s+SuperDesktop$")
    local status = run_capture(
        "檢查 SuperDesktop 原始碼是否已提交", superdesktop_root,
        path(logs, "installer-superdesktop-status.log"),
        {
            "status", "--porcelain=v1", "--untracked-files=all", "--", ".",
            ":(exclude)utit-results/**",
        }
    )
    status = installer_components.filter_superdesktop_status(status, ignore_openspec_untracked)
    installer_components.validate_submodule_identity({
        initialized = true,
        head = head,
        declared_url = declared_url,
        configured_url = configured_url,
        origin_url = origin_url,
        mode = mode,
        gitlink = gitlink,
        status = status,
    })
    return head
end

local function validate_executable(file_path, description)
    return installer_components.validate_pe(file_path, description)
end

local function main()
    local options = parse_options()
    local version = commit_version()
    local logs = path(script_dir, "logs")
    local superdesktop_root = path(root, "SuperDesktop")

    print("SuperExplorer / SuperDesktop 安裝程式建置")
    print("版本庫：" .. root)
    print("版本：" .. version)
    print("元件模式：" .. options.component)
    print("Lua 執行環境：" .. tostring(arg[-1]))

    fs.mkdir_p(logs)
    if options.component == "all" then
        reject_uncommitted_rust(logs)
    end

    local makensis = find_makensis()
    local nsis_script = require_file(path(root, "installer", options.component == "superdesktop" and "SuperDesktop.nsi" or "SuperExplorer.nsi"), "NSIS 腳本")
    if options.include_superdesktop then
        require_file(path(root, "installer", "SuperDesktopFiles.nsh"), "SuperDesktop NSIS 共用檔")
        admit_superdesktop(
            superdesktop_root,
            logs,
            options.component == "all",
            options.ignore_superdesktop_openspec_untracked
        )
    end

    local finalizer
    local plugin_specs = {}
    local superexplorer_inputs = {}
    if options.include_superexplorer then
        finalizer = require_file(path(root, "scripts", "finalize_windows_artifact.ps1"), "發行版最終處理腳本")
        plugin_specs = {
            { define = "PLUGIN_FOLDER_SIZE", root = "rust-folder-size-visual-column", dll = "rust_folder_size_visual_column.dll" },
            { define = "PLUGIN_SIZE_MAP", root = "rust-folder-size-map-view", dll = "rust_folder_size_map_view.dll" },
            { define = "PLUGIN_RUST_TOKEI", root = "rust-tokei-code-lines-column", dll = "rust_tokei_code_lines_column.dll" },
            { define = "PLUGIN_LUA_TOKEI", root = "lua-tokei-code-lines-column", dll = "lua_tokei_code_lines_column.dll" },
            { define = "PLUGIN_LOCK_OWNER", root = "rust-lock-owner-column", dll = "rust_lock_owner_column.dll" },
            { define = "PLUGIN_EXIF_RENAME", root = "rust-exif-rename-command", dll = "rust_exif_rename_command.dll" },
            { define = "PLUGIN_7Z", root = "rust-7z-virtual-folder", dll = "rust_7z_virtual_folder.dll" },
            { define = "PLUGIN_BULK_FOLDER", root = "lua-bulk-folder-generator", dll = "lua_bulk_folder_generator.dll" },
        }
        for _, plugin in ipairs(plugin_specs) do
            plugin.manifest = require_file(path(root, "sdk", "fixtures", plugin.root, "Cargo.toml"), plugin.root .. " manifest")
            plugin.path = path(root, "sdk", "fixtures", plugin.root, "target", "x86_64-pc-windows-msvc", "release", plugin.dll)
        end
        superexplorer_inputs = {
            APP_EXE = path(root, "target", "release", "SuperExplorer.exe"),
            BROKER_EXE = path(root, "target", "release", "explorer-extension-broker.exe"),
            MFT_HELPER_EXE = path(root, "target", "release", "superexplorer-mft-helper.exe"),
            MFT_SERVICE_EXE = path(root, "target", "release", "superexplorer-mft-service.exe"),
            WORKER_EXE = path(root, "target", "release", "explorer-extension-worker.exe"),
            EVERYTHING_DLL = path(root, "target", "release", "Everything64.dll"),
        }
    end

    local superdesktop_inputs = {}
    local superdesktop_identity_inputs = {}
    if options.include_superdesktop then
        local release = path(superdesktop_root, "target", "release")
        superdesktop_inputs = {
            SD_APP_EXE = path(release, "superdesktop-app.exe"),
            SD_GUARDIAN_EXE = path(release, "superdesktop-guardian.exe"),
            SD_INSTALLER_EXE = path(release, "shell-installer.exe"),
            SD_PROVIDER_EXE = path(release, "shell-provider-host.exe"),
            SD_NOTIFICATION_EXE = path(release, "notification-area-host.exe"),
            SD_STATUS_EXE = path(release, "system-status-host.exe"),
            SD_TASKBAR_STATE_EXE = path(release, "taskbar-state-host.exe"),
        }
        local identity = path(superdesktop_root, "build", "windows-identity")
        superdesktop_identity_inputs = {
            SD_IDENTITY_MSIX = path(identity, "SuperDesktop.WindowsShell.msix"),
            SD_IDENTITY_CER = path(identity, "SuperDesktop.WindowsShell.cer"),
            SD_IDENTITY_REGISTER_PS1 = path(superdesktop_root, "scripts", "register-windows-identity-package.ps1"),
        }
    end

    local dist = path(root, "dist")
    local output_stem = options.component == "all" and "SuperExplorer-Setup-"
        or options.component == "superexplorer" and "SuperExplorer-Test-Setup-"
        or "SuperDesktop-Test-Setup-"
    local output = path(dist, output_stem .. version .. "-x64.exe")
    local temporary_name = assert(os.tmpname():match("[^\\/]+$"))
    local temporary_output = path(dist, "." .. temporary_name .. "-" .. output_stem .. "temporary.exe")

    print("NSIS 執行環境：" .. makensis)

    if options.check then
        print("[完成] " .. options.component .. " 安裝程式工具、layout 與 admission 皆可使用")
        return 0
    end

    fs.mkdir_p(dist)

    if not options.skip_build then
        if options.include_superexplorer then
            process.run({
                stage = "建置並驗證 SuperExplorer 發行版執行檔",
                exe = "powershell.exe",
                args = { "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", finalizer, "-Profile", "release" },
                cwd = root,
                log_path = path(logs, "installer-superexplorer-release.log"),
            })
            for _, plugin in ipairs(plugin_specs) do
                process.run({
                    stage = "建置內附 " .. plugin.root .. " Plugin",
                    exe = "powershell.exe",
                    args = {
                        "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command",
                        "$ErrorActionPreference='Stop'; & cargo.exe build --manifest-path " .. powershell_literal(plugin.manifest)
                            .. " --release --target x86_64-pc-windows-msvc --locked --offline; exit $LASTEXITCODE",
                    },
                    cwd = root,
                    log_path = path(logs, "installer-plugin-" .. plugin.root .. "-release.log"),
                })
            end
        end
        if options.include_superdesktop then
            process.run({
                stage = "建置 SuperDesktop locked offline release workspace",
                exe = "cargo.exe",
                args = { "build", "--workspace", "--all-targets", "--release", "--locked", "--offline" },
                cwd = superdesktop_root,
                log_path = path(logs, "installer-superdesktop-release.log"),
            })
        end
    end
    if options.include_superdesktop then
        process.run({
            stage = "建置 SuperDesktop Windows notification identity package",
            exe = "powershell.exe",
            args = {
                "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
                path(superdesktop_root, "scripts", "prepare-windows-identity-package.ps1"),
                "-OutputDirectory", path(superdesktop_root, "build", "windows-identity"),
            },
            cwd = superdesktop_root,
            log_path = path(logs, "installer-superdesktop-identity.log"),
        })
    end

    local selected_size = 0
    for define, file_path in pairs(superexplorer_inputs) do
        selected_size = selected_size + validate_executable(file_path, "SuperExplorer " .. define)
    end
    for _, plugin in ipairs(plugin_specs) do
        selected_size = selected_size + validate_executable(plugin.path, plugin.root .. " Plugin DLL")
    end
    for define, file_path in pairs(superdesktop_inputs) do
        selected_size = selected_size + validate_executable(file_path, "SuperDesktop " .. define)
    end
    for define, file_path in pairs(superdesktop_identity_inputs) do
        require_file(file_path, "SuperDesktop " .. define)
        selected_size = selected_size + assert(lfs.attributes(file_path, "size"))
    end

    os.remove(temporary_output)
    local define_lines = {}
    local function add_define(name, value)
        value = tostring(value)
        if value:find('["\r\n]') then error("NSIS define 含有不允許的字元：" .. name, 0) end
        define_lines[#define_lines + 1] = string.format('!define %s "%s"', name, value)
    end
    add_define("APP_VERSION", version)
    add_define("OUTPUT_FILE", temporary_output)
    if options.component == "all" then define_lines[#define_lines + 1] = "!define INCLUDE_SUPERDESKTOP 1" end
    for define, file_path in pairs(superexplorer_inputs) do add_define(define, file_path) end
    for _, plugin in ipairs(plugin_specs) do add_define(plugin.define, plugin.path) end
    for define, file_path in pairs(superdesktop_inputs) do add_define(define, file_path) end
    for define, file_path in pairs(superdesktop_identity_inputs) do add_define(define, file_path) end
    local defines_path = path(logs, "installer-defines-" .. options.component .. ".nsh")
    write_file(defines_path, table.concat(define_lines, "\r\n") .. "\r\n")
    local nsis_args = {
        "/V4",
        "/WX",
        "/INPUTCHARSET",
        "UTF8",
        "/OUTPUTCHARSET",
        "UTF8",
        "/DGENERATED_DEFINES=" .. defines_path,
        nsis_script,
    }
    process.run({
        stage = "編譯 NSIS 安裝程式",
        exe = makensis,
        args = nsis_args,
        cwd = root,
        log_path = path(logs, "installer-nsis-" .. options.component .. ".log"),
    })
    validate_executable(temporary_output, "暫存安裝程式")
    publish.apk(temporary_output, output)
    local installer_size = validate_executable(output, "安裝程式")

    if not options.no_launch then
        process.start({
            stage = "啟動安裝程式",
            exe = output,
            cwd = dist,
        })
    end

    print(string.format("[完成] 模式：%s，選取輸入合計 %d 位元組", options.component, selected_size))
    print(string.format("[完成] 安裝程式：%s（%d 位元組）", output, installer_size))
    if options.no_launch then
        print("[完成] 已略過啟動安裝程式")
    else
        print("[完成] 已啟動本次建置的 " .. options.component .. " 安裝程式")
    end
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
