-- Configure the repository-bundled Lua modules and native libraries.
-- 設定由版本庫隨附的 Lua 模組與原生函式庫。
local script_dir = assert(arg[0]:match("^(.*)[\\/]build_sdk%.lua$"), "build_sdk.lua must be invoked from build")
package.path = script_dir .. "/lib/?.lua;" .. package.path
package.cpath = script_dir .. "/tools/lua/?.dll;" .. package.cpath

local guard = require("source_guard")
local cli = require("cli")
local path_util = require("path")
local sensitive_paths = require("sensitive_paths")
local fs = require("fs")
local sdk_version = require("sdk_version")
local process = require("process")
local java11 = require("java11")
local android_sdk = require("android_sdk")
local lfs = require("lfs")
if script_dir == "build" then script_dir = path_util.join(lfs.currentdir(), "build") end
local root = assert(script_dir:match("^(.*)[\\/]build$"), "build_sdk.lua must be located below the repository root")
-- Centralize path handling, command execution, and byte-preserving file copies.
-- 集中處理路徑、命令執行，以及保持位元內容不變的檔案複製。
local function path(...)
    return path_util.join(...)
end

local logs = path(script_dir, "logs")
fs.mkdir_p(logs)
local log_counter = 0
local function run(stage, exe, args, cwd, env)
    log_counter = log_counter + 1
    local log_path = path(logs, string.format("sdk-%02d-%s.log", log_counter, stage:gsub("[^%w%-]", "-")))
    process.run({stage=stage, exe=exe, args=args or {}, cwd=cwd or root, log_path=log_path, env=env})
    return log_path
end

local function capture(stage, exe, args, cwd)
    local log_path = run(stage, exe, args, cwd)
    local file = assert(io.open(log_path, "rb"))
    local data = file:read("*a")
    file:close()
    return data
end

local function read_file(p)
    local file = assert(io.open(p, "rb"))
    local data = file:read("*a")
    file:close()
    return data
end

local function write_if_changed(p, data)
    local existing = read_file(p)
    if existing == data then
        print("[SAME] " .. p)
        return false
    end
    local file = assert(io.open(p, "wb"))
    assert(file:write(data))
    file:close()
    print("[UPDATE] " .. p)
    return true
end

local function exists(p) return fs.exists(p) end

local function require_file(p)
    if not exists(p) then error("required file missing: " .. p, 0) end
end

local function ps(stage, script)
    run(stage, "powershell.exe", {"-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "$ErrorActionPreference='Stop'; " .. script}, root)
end

local function clean_dir(p)
    fs.remove_tree(p)
    fs.mkdir_p(p)
end
local function copy_file(src, dst)
    require_file(src)
    local changed = fs.copy_if_different(src, dst)
    print(string.format("[%s] %s -> %s", changed and "COPY" or "SAME", src, dst))
end

-- Synchronize the Cargo manifest and release naming with the HEAD committer date.
-- Cargo.lock is ignored build output and must not be an input to this script.
-- 使用 HEAD 提交日期同步 Cargo manifest 與 Release 命名；Cargo.lock 是被忽略的建置中繼檔，不得成為腳本輸入。
local function main()
local original_cargo_toml = read_file(path(root, "Cargo.toml"))
local function workflow()
local build_version = sdk_version.resolve(root)
build_version.sdk_version = build_version.version
local cargo_toml = path(root, "Cargo.toml")
write_if_changed(cargo_toml, sdk_version.replace_workspace_version(read_file(cargo_toml), build_version.sdk_version))
print("=== MAGT SDK version ===")
print("Git commit: " .. build_version.commit)
print("Git commit date: " .. build_version.iso_date)
print("SDK version: " .. build_version.sdk_version)

if os.getenv("MAGT_BUILD_SDK_TEST_FAIL_AFTER_SYNC") == "1" then
    process.run({
        stage = "SDK version restoration test",
        exe = "cmd.exe", args = { "/d", "/c", "echo injected-sdk-failure & exit /b 23" },
        cwd = root, log_path = path(logs, "sdk-version-restoration-test.log"),
    })
end

if arg[1] == "--version-only" then
    print("[OK] SDK version metadata synchronized")
    return 0
end

local java_home = java11.resolve()
local android_sdk_root = android_sdk.resolve()
local gradle_env = {
    JAVA_HOME=java_home,
    ANDROID_SDK_ROOT=android_sdk_root,
    ANDROID_HOME=android_sdk_root,
}
print("Java 11 home: " .. java_home)
print("Android SDK root: " .. android_sdk_root)

local function has_component(relative, names)
    for part in relative:gmatch("[^/]+") do
        if names[part:lower()] then return true end
    end
    return false
end

-- Exclude generated Unreal data and sensitive game credentials from packages.
-- 從封裝中排除 Unreal 產生資料與遊戲敏感憑證。
local plugin_excluded = {binaries=true, intermediate=true, content=true, [".vs"]=true}
local function plugin_include(relative, mode)
    if has_component(relative, plugin_excluded) then return false end
    if mode == "directory" then return true end
    local lower = relative:lower()
    if lower:match("%.sln$") or lower:match("%.suo$") or lower:match("%.user$") then return false end
    if lower == "magtmodule/source/thirdparty/magtmodulelibrary/windows/x64/magt_sdk.lib" then return false end
    return not sensitive_paths.is_sensitive(relative)
end

local function release_dist_include(relative, mode)
    if has_component(relative, {debug=true}) then return false end
    return mode == "directory" or not sensitive_paths.is_sensitive(relative)
end

local function unity_include(relative, mode)
    if mode == "directory" then return not sensitive_paths.is_sensitive(relative) end
    return not relative:lower():match("%.meta$") and not sensitive_paths.is_sensitive(relative)
end

local function unity_plugin_include(relative, mode)
    return not sensitive_paths.is_sensitive(relative)
end

-- Build only from committed SDK source so generated artifacts remain reproducible.
-- 僅使用已提交的 SDK 原始碼建置，以確保產物可重現。
local function changed_sdk_sources()
    local names = {}
    for line in capture("git-diff", "git.exe", {"diff", "--name-only", "HEAD", "--", "magt_sdk", "magt_queued_buffer_sdk"}, root):gmatch("[^\r\n]+") do names[#names + 1] = line end
    for line in capture("git-untracked", "git.exe", {"ls-files", "--others", "--exclude-standard", "--", "magt_sdk", "magt_queued_buffer_sdk"}, root):gmatch("[^\r\n]+") do names[#names + 1] = line end
    return guard.filter(names)
end

local dirty = changed_sdk_sources()
if #dirty > 0 then
    io.stderr:write("[ERROR] Uncommitted SDK source files detected:\n")
    for _, item in ipairs(dirty) do io.stderr:write("  " .. item .. "\n") end
    error("uncommitted SDK source files detected", 0)
end

-- Resolve all canonical source, output, Unity, and Unreal locations.
-- 解析所有標準來源、輸出、Unity 與 Unreal 路徑。
local magt = path(root, "magt_sdk")
local queued = path(root, "magt_queued_buffer_sdk")
local target = path(root, "target")
local dist = path(magt, "dist")
local magt_dist = path(dist, "sdk")
local queued_dist = path(dist, "queued-buffer-sdk")
local unity_android = path(root, "AndroidMAGTSample", "Assets", "Plugins", "Android")
local unreal_targets = {}
do
    local project_plugins = path(root, "Unreal4ARPG", "Plugins")
    require_file(path(project_plugins, "MagtModule", "MagtModule.uplugin"))
    require_file(path(project_plugins, "MTKCompensatedTimeStep", "MTKCompensatedTimeStep.uplugin"))
    require_file(path(project_plugins, "MagtReference", "MagtReference.uplugin"))
    unreal_targets[#unreal_targets + 1] = project_plugins
end
local plugins = unreal_targets[1]
local unreal_lib = path(plugins, "MagtModule", "Source", "ThirdParty", "MagtModuleLibrary")
local aar = path(magt, "android", "magt-sdk", "build", "outputs", "aar", "magt-sdk-release.aar")
local defs = {"MAGTModuleDef.h", "MAGTModuleDef_V1.h", "MAGTModuleDef_V2.h", "MAGTModuleDef_V3.h", "MAGTModuleDef_V4.h", "MAGTModuleDef_V5.h"}

-- Fail early when a required SDK header or Unreal plugin is missing.
-- 必要的 SDK 標頭或 Unreal 外掛缺少時立即停止。
for _, p in ipairs({
    path(magt, "MAGTModuleAPI.h"), path(queued, "MAGTQueuedBufferAPI.h"),
}) do require_file(p) end

-- Build and collect the MAGT Android release artifacts.
-- 建置並收集 MAGT Android Release 產物。
print("=== Build MAGT SDK (Android release) ===")
-- cargo ndk -t arm64-v8a build --release
run("magt-cargo-ndk", "cargo.exe", {"ndk", "-t", "arm64-v8a", "build", "--release"}, magt)
-- -PMAGT_LOAD_QUEUED_BUFFER=false :magt-sdk:assembleRelease
run("magt-gradle", "cmd.exe", {"/d", "/s", "/c", path(magt, "android", "gradlew.bat"), "-PMAGT_LOAD_QUEUED_BUFFER=false", ":magt-sdk:assembleRelease"}, path(magt, "android"), gradle_env)

print("=== Collect MAGT SDK ===")
clean_dir(magt_dist)
copy_file(path(target, "aarch64-linux-android", "release", "libmagt_sdk.so"), path(magt_dist, "android-arm64", "dynamic", "release", "libmagt_sdk.so"))
copy_file(path(target, "aarch64-linux-android", "release", "libmagt_sdk.a"), path(magt_dist, "android-arm64", "static", "release", "libmagt_sdk.a"))
copy_file(path(magt, "MAGTModuleAPI.h"), path(magt_dist, "include", "MAGTModuleAPI.h"))
copy_file(path(magt, "MAGTModuleAPI.h"), path(magt_dist, "include", "Android", "MAGTModuleAPI_Android.h"))
copy_file(path(unreal_lib, "Public", "Stub", "MAGTModuleAPI_Stub.h"), path(magt_dist, "include", "Stub", "MAGTModuleAPI_Stub.h"))
for _, name in ipairs(defs) do copy_file(path(magt, name), path(magt_dist, "include", name)) end
copy_file(aar, path(magt_dist, "magt-sdk.aar"))
copy_file(path(root, "MAGT_SDK_使用指南.md"), path(magt_dist, "MAGT_SDK_使用指南.md"))

-- Build and collect queued-buffer artifacts for Windows and Android.
-- 建置並收集 Windows 與 Android 的 queued-buffer 產物。
print("=== Build Queued Buffer SDK (Windows/Android release) ===")
-- cargo build --target x86_64-pc-windows-msvc -p magt_queued_buffer_sdk --release
run("queued-windows", "cargo.exe", {"build", "--target", "x86_64-pc-windows-msvc", "-p", "magt_queued_buffer_sdk", "--release"}, root)
-- cargo ndk -t arm64-v8a build -p magt_queued_buffer_sdk --release
run("queued-android", "cargo.exe", {"ndk", "-t", "arm64-v8a", "build", "-p", "magt_queued_buffer_sdk", "--release"}, root)
-- -PMAGT_LOAD_QUEUED_BUFFER=true :magt-sdk:assembleRelease
run("queued-gradle", "cmd.exe", {"/d", "/s", "/c", path(magt, "android", "gradlew.bat"), "-PMAGT_LOAD_QUEUED_BUFFER=true", ":magt-sdk:assembleRelease"}, path(magt, "android"), gradle_env)

print("=== Collect Queued Buffer SDK ===")
clean_dir(queued_dist)
local queued_files = {
    {path(target,"x86_64-pc-windows-msvc","release","magt_queued_buffer_sdk.dll"), path(queued_dist,"windows-x64","dynamic","release","magt_queued_buffer_sdk.dll")},
    {path(target,"x86_64-pc-windows-msvc","release","magt_queued_buffer_sdk.dll.lib"), path(queued_dist,"windows-x64","dynamic","release","magt_queued_buffer_sdk.dll.lib")},
    {path(target,"x86_64-pc-windows-msvc","release","magt_queued_buffer_sdk.lib"), path(queued_dist,"windows-x64","static","release","magt_queued_buffer_sdk.lib")},
    {path(target,"aarch64-linux-android","release","libmagt_queued_buffer_sdk.so"), path(queued_dist,"android-arm64","dynamic","release","libmagt_queued_buffer_sdk.so")},
    {path(target,"aarch64-linux-android","release","libmagt_queued_buffer_sdk.a"), path(queued_dist,"android-arm64","static","release","libmagt_queued_buffer_sdk.a")},
    {path(queued,"MAGTQueuedBufferAPI.h"), path(queued_dist,"include","MAGTQueuedBufferAPI.h")},
    {path(queued,"README.zh-CN.md"), path(queued_dist,"readme.md")},
    {aar, path(queued_dist,"magt-sdk.aar")},
}
for _, pair in ipairs(queued_files) do copy_file(pair[1], pair[2]) end

-- Report compiled SDK sizes without enforcing a deployment limit.
-- 顯示已編譯 SDK 的大小，不設定部署上限。
local size_artifacts = {
    path(magt_dist,"android-arm64","dynamic","release","libmagt_sdk.so"),
    path(magt_dist,"android-arm64","static","release","libmagt_sdk.a"),
    path(magt_dist,"magt-sdk.aar"),
    path(queued_dist,"android-arm64","dynamic","release","libmagt_queued_buffer_sdk.so"),
    path(queued_dist,"android-arm64","static","release","libmagt_queued_buffer_sdk.a"),
    path(queued_dist,"magt-sdk.aar"),
}
for _, artifact in ipairs(size_artifacts) do
    local file = assert(io.open(artifact, "rb")); local size = assert(file:seek("end")); file:close()
    print(string.format("[SIZE] %s = %d bytes (%.2f KB, %.2f MB)", artifact, size, size / 1024, size / 1048576))
end

-- Deploy compiled SDK artifacts before independent package validation.
-- 在獨立的封裝驗證前部署已編譯的 SDK 產物。
print("=== Deploy compiled SDK artifacts ===")
local deploy_pairs = {
    {path(magt_dist,"android-arm64","dynamic","release","libmagt_sdk.so"), path(unity_android,"libmagt_sdk.so")},
    {path(magt_dist,"magt-sdk.aar"), path(unity_android,"magt-sdk.aar")},
    {path(magt,"MAGTModuleAPI.h"), path(unity_android,"MAGTModuleAPI.h")},
    {path(magt,"MAGTModuleAPI.h"), path(unity_android,"Android","MAGTModuleAPI_Android.h")},
    {path(unreal_lib,"Public","Stub","MAGTModuleAPI_Stub.h"), path(unity_android,"Stub","MAGTModuleAPI_Stub.h")},
    {path(queued,"MAGTQueuedBufferAPI.h"), path(unity_android,"MAGTQueuedBufferAPI.h")},
    {path(magt_dist,"android-arm64","dynamic","release","libmagt_sdk.so"), path(unreal_lib,"Android","ARM64","libmagt_sdk.so")},
    {path(magt_dist,"android-arm64","static","release","libmagt_sdk.a"), path(unreal_lib,"Android","ARM64","libmagtsdk.a")},
    {path(magt_dist,"magt-sdk.aar"), path(unreal_lib,"Android","magt-sdk.aar")},
    {path(magt,"MAGTModuleAPI.h"), path(unreal_lib,"Public","MAGTModuleAPI.h")},
    {path(magt,"MAGTModuleAPI.h"), path(unreal_lib,"Public","Android","MAGTModuleAPI_Android.h")},
    {path(queued,"MAGTQueuedBufferAPI.h"), path(unreal_lib,"Public","MAGTQueuedBufferAPI.h")},
}
for _, name in ipairs(defs) do
    deploy_pairs[#deploy_pairs+1] = {path(magt,name), path(unity_android,name)}
    deploy_pairs[#deploy_pairs+1] = {path(magt,name), path(unreal_lib,"Public",name)}
end
for index = 2, #unreal_targets do
    local target_lib = path(unreal_targets[index], "MagtModule", "Source", "ThirdParty", "MagtModuleLibrary")
    for _, pair in ipairs({
        {path(magt_dist,"android-arm64","dynamic","release","libmagt_sdk.so"), path(target_lib,"Android","ARM64","libmagt_sdk.so")},
        {path(magt_dist,"android-arm64","static","release","libmagt_sdk.a"), path(target_lib,"Android","ARM64","libmagtsdk.a")},
        {path(magt_dist,"magt-sdk.aar"), path(target_lib,"Android","magt-sdk.aar")},
        {path(magt,"MAGTModuleAPI.h"), path(target_lib,"Public","MAGTModuleAPI.h")},
        {path(magt,"MAGTModuleAPI.h"), path(target_lib,"Public","Android","MAGTModuleAPI_Android.h")},
        {path(queued,"MAGTQueuedBufferAPI.h"), path(target_lib,"Public","MAGTQueuedBufferAPI.h")},
    }) do deploy_pairs[#deploy_pairs+1] = pair end
    for _, name in ipairs(defs) do
        deploy_pairs[#deploy_pairs+1] = {path(magt,name), path(target_lib,"Public",name)}
    end
end
for _, pair in ipairs(deploy_pairs) do require_file(pair[1]) end
for _, pair in ipairs(deploy_pairs) do copy_file(pair[1], pair[2]) end

-- Prepare deterministic staging locations and date-based archive names.
-- 準備固定的暫存位置與依日期命名的封裝檔名。
local date = build_version.iso_date
local stage_root = path(os.getenv("TEMP"), "magt-sdk-lua-stage")
clean_dir(stage_root)
local full_zip = path(root, "magt-sdk-" .. date .. ".zip")
local release_zip = path(root, "magt-sdk-" .. date .. "-release.zip")
local queued_zip = path(root, "magt-queued-buffer-sdk-" .. date .. "-release.zip")
local unity_plugin_zip = path(root, "UnityMagtPlugin.zip")
local unreal_plugin_zip = path(root, "UnrealMagtPlugins.zip")

os.remove(path(root,"MagtPlugins.zip"))
for _, project_plugins in ipairs(unreal_targets) do os.remove(path(project_plugins,"MagtPlugins.zip")) end

local function compress(source, output)
    ps("compress", "if(Test-Path -LiteralPath '"..output.."'){Remove-Item -LiteralPath '"..output.."' -Force}; Compress-Archive -Path '"..source.."\\*' -DestinationPath '"..output.."' -CompressionLevel Optimal -Force")
end

-- Reject archives containing keystores, licenses, signatures, or game data.
-- 拒絕包含 keystore、license、signature 或遊戲資料的封裝檔。
local function validate_archive_has_no_sensitive_files(archive)
    local archive_ps = archive:gsub("'", "''")
    local entries = capture("archive-scan", "powershell.exe", {"-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
        "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; " ..
        "$zip=[IO.Compression.ZipFile]::OpenRead('" .. archive_ps .. "'); try { " ..
        "$zip.Entries | Where-Object { -not $_.FullName.EndsWith('/') } | ForEach-Object { $_.FullName } " ..
        "} finally { $zip.Dispose() }"}, root)
    for entry in entries:gmatch("[^\r\n]+") do
        if sensitive_paths.is_sensitive(entry) then
            error("sensitive game credential/license path in archive " .. archive .. ": " .. entry, 0)
        end
    end
    print("[SECURITY] no sensitive game credential/license paths: " .. archive)
end
compress(magt_dist, full_zip)
compress(queued_dist, queued_zip)

-- Assemble the studio release from validated SDK, Unity, and Unreal inputs.
-- 從已驗證的 SDK、Unity 與 Unreal 來源組合工作室 Release 封裝。
local release_stage = path(stage_root, "release")
clean_dir(release_stage)
fs.copy_tree(magt_dist, release_stage, release_dist_include)
fs.copy_tree(unity_android, path(release_stage,"unity","Plugins","Android"), unity_include)
for _, name in ipairs({"MagtModule","MTKCompensatedTimeStep","MagtReference"}) do
    fs.copy_tree(path(plugins,name), path(release_stage,"unreal",name), plugin_include)
end
for _, name in ipairs(defs) do
    copy_file(path(magt,name), path(release_stage,"unity","Plugins","Android",name))
    copy_file(path(magt,name), path(release_stage,"unreal","MagtModule","Source","ThirdParty","MagtModuleLibrary","Public",name))
end
copy_file(path(magt_dist,"android-arm64","dynamic","release","libmagt_sdk.so"), path(release_stage,"unity","Plugins","Android","libmagt_sdk.so"))
copy_file(path(magt_dist,"magt-sdk.aar"), path(release_stage,"unity","Plugins","Android","magt-sdk.aar"))
copy_file(path(queued,"MAGTQueuedBufferAPI.h"), path(release_stage,"unity","Plugins","Android","MAGTQueuedBufferAPI.h"))
copy_file(path(magt_dist,"android-arm64","dynamic","release","libmagt_sdk.so"), path(release_stage,"unreal","MagtModule","Source","ThirdParty","MagtModuleLibrary","Android","ARM64","libmagt_sdk.so"))
copy_file(path(magt_dist,"magt-sdk.aar"), path(release_stage,"unreal","MagtModule","Source","ThirdParty","MagtModuleLibrary","Android","magt-sdk.aar"))
copy_file(path(queued,"MAGTQueuedBufferAPI.h"), path(release_stage,"unreal","MagtModule","Source","ThirdParty","MagtModuleLibrary","Public","MAGTQueuedBufferAPI.h"))
compress(release_stage, release_zip)

-- Package the core Unity C# bridge and Android SDK files with Assets paths.
-- 依 Assets 路徑封裝 Unity 核心 C# 橋接與 Android SDK 檔案。
local unity_plugin_stage = path(stage_root, "unity-plugin")
clean_dir(unity_plugin_stage)
for _, name in ipairs({"Entry.meta", "Plugins.meta"}) do
    copy_file(path(root,"AndroidMAGTSample","Assets",name), path(unity_plugin_stage,"Assets",name))
end
copy_file(path(root,"AndroidMAGTSample","Assets","Plugins","Android.meta"), path(unity_plugin_stage,"Assets","Plugins","Android.meta"))
local unity_entry_files = {
    "MAGTEntry.cs", "MAGTEnum.cs", "MAGTHooks.cs", "MAGTModule.cs",
    "MAGTModuleAPI.cs", "MAGTModuleNativeAPI.cs", "MAGTService.cs",
    "MAGTTraceEvent.cs", "MAGTVersion.cs",
}
for _, name in ipairs(unity_entry_files) do
    copy_file(path(root,"AndroidMAGTSample","Assets","Entry",name), path(unity_plugin_stage,"Assets","Entry",name))
    copy_file(path(root,"AndroidMAGTSample","Assets","Entry",name..".meta"), path(unity_plugin_stage,"Assets","Entry",name..".meta"))
end
fs.copy_tree(unity_android, path(unity_plugin_stage,"Assets","Plugins","Android"), unity_plugin_include)
compress(unity_plugin_stage, unity_plugin_zip)
copy_file(unity_plugin_zip, path(root,"AndroidMAGTSample","UnityMagtPlugin.zip"))

-- Package Unreal plugins and overlay canonical headers before compression.
-- 封裝 Unreal 外掛，並在壓縮前覆蓋標準標頭。
local plugin_stage = path(stage_root, "plugins")
clean_dir(plugin_stage)
for _, name in ipairs({"MagtModule","MTKCompensatedTimeStep","MagtReference"}) do
    fs.copy_tree(path(plugins,name), path(plugin_stage,name), plugin_include)
end
for _, name in ipairs(defs) do
    copy_file(path(magt,name), path(plugin_stage,"MagtModule","Source","ThirdParty","MagtModuleLibrary","Public",name))
end
compress(plugin_stage, unreal_plugin_zip)
for _, project_plugins in ipairs(unreal_targets) do
    copy_file(unreal_plugin_zip, path(project_plugins,"UnrealMagtPlugins.zip"))
end

for _, archive in ipairs({full_zip, release_zip, queued_zip, unity_plugin_zip, unreal_plugin_zip}) do
    validate_archive_has_no_sensitive_files(archive)
end
fs.remove_tree(stage_root)

-- Report all successfully generated deliverables.
-- 列出所有成功產生的交付檔案。
print("[OK] Lua MAGT SDK build and deployment completed")
print("[OK] " .. full_zip)
print("[OK] " .. release_zip)
print("[OK] " .. queued_zip)
print("[OK] " .. unity_plugin_zip)
print("[OK] " .. unreal_plugin_zip)
return 0
end
local ok, result = pcall(workflow)
if not ok then
    write_if_changed(path(root, "Cargo.toml"), original_cargo_toml)
    error(result, 0)
end
return result
end

os.exit(cli.run(main))
