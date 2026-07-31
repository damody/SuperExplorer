local lfs = require("lfs")
local android = require("android")
local engine = require("engine")
local fs = require("fs")
local process = require("process")
local publish = require("publish")
local sdk_version = require("sdk_version")

local M = {}
local sep = package.config:sub(1, 1)
local function join(...) return table.concat({...}, sep) end
local function file(path) return lfs.attributes(path, "mode") == "file" end
local function first_java_home(parent, prefix)
    if lfs.attributes(parent, "mode") ~= "directory" then return nil end
    local names = {}
    for name in lfs.dir(parent) do
        if name ~= "." and name ~= ".." and name:lower():sub(1, #prefix) == prefix then names[#names + 1] = name end
    end
    table.sort(names)
    for _, name in ipairs(names) do
        local home = join(parent, name)
        if file(join(home, "bin", "java.exe")) then return home end
    end
end

local function nonce()
    local temporary = os.tmpname(); os.remove(temporary)
    local name = temporary:match("[^\\/]+$") or temporary
    return os.date("%Y%m%d-%H%M%S") .. "-" .. name:gsub("[^%w_.-]", "-")
end

function M.reserve_invocation(root, platform, nonce_fn)
    fs.mkdir_p(root)
    local id = (nonce_fn or nonce)()
    assert(id ~= "" and not id:find("[\\/]"), "invalid invocation nonce")
    for counter = 0, 9999 do
        local candidate = join(root, platform .. "-" .. id .. "-" .. counter)
        if lfs.mkdir(candidate) then return candidate end
    end
    error("could not reserve a unique Unreal invocation directory", 0)
end

function M.build_paths(invocation, logs, platform, basename, version)
    local invocation_id = assert(invocation:match("[^\\/]+$"))
    return {
        archive = join(invocation, "archive"),
        log_path = join(logs, "unreal4-" .. platform .. "-" .. basename .. "-" .. invocation_id .. ".log"),
        final_name = basename .. "_" .. version .. (platform == "android" and ".apk" or "_Windows"),
    }
end

function M.command(engine_root, project, platform, archive, log_path)
    assert(platform == "android" or platform == "windows", "platform must be android or windows")
    local uat = join(engine_root, "Engine", "Build", "BatchFiles", "RunUAT.bat")
    local target = platform == "android" and "Android" or "Win64"
    local client_config = platform == "android" and "Development" or "Shipping"
    local args = { "/d", "/s", "/c", uat, "BuildCookRun", "-project=" .. project,
        "-platform=" .. target, "-targetplatform=" .. target, "-clientconfig=" .. client_config,
        "-build", "-cook", "-stage", "-pak", "-package", "-archive" }
    if platform == "android" then
        args[#args + 1] = "-cookflavor=ASTC"
        args[#args + 1] = "-architectures=arm64"
    end
    args[#args + 1] = "-archivedirectory=" .. archive
    return { exe="cmd.exe", uat=uat, args=args, log_path=log_path }
end

local function find_recursive(root, predicate)
    local function visit(path)
        if lfs.attributes(path, "mode") ~= "directory" then return nil end
        for name in lfs.dir(path) do
            if name ~= "." and name ~= ".." then
                local candidate = join(path, name)
                local mode = lfs.attributes(candidate, "mode")
                if mode == "file" and predicate(name) then return candidate end
                if mode == "directory" then local found = visit(candidate); if found then return found end end
            end
        end
    end
    return visit(root)
end

function M.locate_apk(archive)
    return assert(find_recursive(archive, function(name) return name:lower():match("%.apk$") end),
        "Unreal Android archive contains no APK: " .. archive)
end

function M.locate_windows(archive, project_name)
    assert(type(project_name) == "string" and project_name ~= "", "project name is required")
    local package = join(archive, "WindowsNoEditor")
    local exe_name = project_name .. ".exe"
    local exe = join(package, exe_name)
    assert(lfs.attributes(package, "mode") == "directory",
        "Unreal Windows package root is missing: " .. package)
    assert(file(exe), "Unreal Windows package executable is missing: " .. exe)
    return package, exe_name
end

local function android_spec(root, engine_root, env)
    env = env or {}
    local sdk = env.ANDROID_SDK_ROOT or env.ANDROID_HOME or os.getenv("ANDROID_SDK_ROOT") or os.getenv("ANDROID_HOME")
        or join(env.LOCALAPPDATA or os.getenv("LOCALAPPDATA") or "", "Android", "Sdk")
    local java_home = env.JAVA_HOME or os.getenv("JAVA_HOME")
    if not java_home then
        local programs = env.PROGRAM_FILES or os.getenv("ProgramFiles") or "C:\\Program Files"
        local studio = join(programs, "Android", "Android Studio")
        java_home = file(join(studio, "jre", "bin", "java.exe")) and join(studio, "jre")
            or first_java_home(join(programs, "Eclipse Adoptium"), "jdk-8")
            or first_java_home(join(programs, "Java"), "jdk1.8")
            or join(studio, "jbr")
    end
    local sdkmanager = join(sdk, "cmdline-tools", "latest", "bin", "sdkmanager.bat")
    if not file(sdkmanager) then sdkmanager = join(sdk, "tools", "bin", "sdkmanager.bat") end
    return {
        sdk_root=sdk, java=join(java_home, "bin", "java.exe"), sdkmanager=sdkmanager,
        baseline=android.read_engine_baseline(join(engine_root, "Engine", "Extras", "Android", "SetupAndroid.bat")),
        project_platform=android.read_project_requirements(join(root, "Unreal4ARPG", "Config", "DefaultEngine.ini")).sdk_api,
    }
end
M.android_spec = android_spec

function M.android_environment(spec, inherited_path)
    local java_home = assert(spec.java:match("^(.*)[\\/]bin[\\/]java%.exe$"),
        "validated Java path must end in bin/java.exe")
    local ndk_version = assert(spec.baseline.ndk, "validated NDK version is required")
    local ndk_root = join(spec.sdk_root, "ndk", ndk_version)
    assert(file(join(ndk_root, "source.properties")), "validated NDK root is missing: " .. ndk_root)
    local additions = {
        join(java_home, "bin"), join(spec.sdk_root, "platform-tools"),
        join(spec.sdk_root, "cmdline-tools", "latest", "bin"), ndk_root,
    }
    local windows = os.getenv("SystemRoot") or "C:\\Windows"
    local system_path = inherited_path or table.concat({
        join(windows, "System32"), windows, join(windows, "System32", "Wbem"),
        join(windows, "System32", "WindowsPowerShell", "v1.0"),
    }, ";")
    additions[#additions + 1] = system_path
    return {
        JAVA_HOME=java_home,
        ANDROID_HOME=spec.sdk_root,
        ANDROID_SDK_ROOT=spec.sdk_root,
        NDKROOT=ndk_root,
        NDK_ROOT=ndk_root,
        PATH=table.concat(additions, ";"),
    }
end

function M.run(config, dependencies)
    dependencies = dependencies or {}
    assert(config.platform == "android" or config.platform == "windows",
        "usage: build_unreal4.lua android|windows [--check-only]")
    local root = dependencies.root or assert(lfs.currentdir())
    local project = join(root, config.project)
    assert(file(project), "Unreal project not found: " .. project)
    local engine_root = dependencies.engine_root or engine.find_unreal(project, config.engine_major)
    local version = dependencies.version or sdk_version.resolve(root).version
    local logs = join(root, "build", "logs"); fs.mkdir_p(logs)
    local reserve = dependencies.reserve_invocation or M.reserve_invocation
    local invocation = reserve(join(root, "build", "temp", "unreal4"), config.platform)
    local paths = M.build_paths(invocation, logs, config.platform, config.app_basename, version)
    local command = M.command(engine_root, project, config.platform, paths.archive, paths.log_path)
    assert(file(command.uat), "RunUAT.bat not found: " .. command.uat)
    print("Unreal Engine root: " .. engine_root)
    print("Project: " .. project)
    print("Platform: " .. config.platform)
    print("Configuration: " .. (config.platform == "android" and "Development" or "Shipping"))
    print("Output: " .. join(root, paths.final_name))
    print("Log: " .. paths.log_path)

    local uat_env
    if config.platform == "android" then
        local spec = (dependencies.android_spec or android_spec)(root, engine_root)
        uat_env = (dependencies.android_environment or M.android_environment)(spec)
        if config.check_only then
            local ok, missing = (dependencies.android_check or android.check)(spec)
            print("Android packages: " .. (#missing == 0 and "ready" or table.concat(missing, ", ")))
            assert(ok, "Android prerequisites are missing: " .. table.concat(missing, ", "))
            print("Android NDK root: " .. uat_env.NDKROOT)
        else
            local ensure = dependencies.android_ensure or android.ensure
            ensure(spec, dependencies.process_run or process.run)
        end
    end
    if config.check_only then fs.remove_tree(invocation); return true end

    fs.mkdir_p(paths.archive)
    local run_process = dependencies.process_run or process.run
    run_process({ stage="Unreal4 " .. config.platform, exe=command.exe,
        args=command.args, cwd=root, log_path=command.log_path, env=uat_env })
    if config.platform == "android" then
        local apk = (dependencies.locate_apk or M.locate_apk)(paths.archive)
        local publish_apk = dependencies.publish_apk or publish.apk
        publish_apk(apk, join(root, paths.final_name))
    else
        local package, exe_name = M.locate_windows(paths.archive,
            assert(config.package_executable, "Windows package executable is required"))
        publish.windows(package, join(root, paths.final_name), exe_name)
    end
    fs.remove_tree(invocation)
    return true
end

return M
