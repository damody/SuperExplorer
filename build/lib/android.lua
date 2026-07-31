local lfs = require("lfs")
local M = {}
local sep = package.config:sub(1, 1)
local function join(...) return table.concat({...}, sep) end
local function file_exists(path) return lfs.attributes(path, "mode") == "file" end
local function read(path)
    local file = assert(io.open(path, "rb"), "unable to read " .. path)
    local content = file:read("*a"); file:close(); return content
end

function M.read_project_requirements(default_engine_ini)
    local target = "/Script/AndroidRuntimeSettings.AndroidRuntimeSettings"
    local section, api
    for line in (read(default_engine_ini) .. "\n"):gmatch("([^\r\n]*)[\r\n]+") do
        local trimmed = line:match("^%s*(.-)%s*$")
        if trimmed:sub(1, 1) ~= ";" and trimmed:sub(1, 1) ~= "#" then
            local heading = trimmed:match("^%[([^%]]+)%]")
            if heading then
                section = heading
            elseif section == target then
                local value = trimmed:match("^SDKAPILevelOverride%s*=%s*(android%-%d+)")
                if value then api = value end
            end
        end
    end
    assert(api, "unable to determine SDKAPILevelOverride from " .. default_engine_ini)
    return { sdk_api = api }
end

function M.read_engine_baseline(setup_android_bat)
    local content = read(setup_android_bat)
    local baseline = {
        platform = content:match('platforms;(android%-%d+)'),
        build_tools = content:match('build%-tools;([%d%.]+)'),
        cmake = content:match('cmake;([%d%.]+)'),
        ndk = content:match('ndk;([%d%.]+)') or content:match('\\ndk\\([%d%.]+)'),
    }
    if not (baseline.platform and baseline.build_tools and baseline.cmake and baseline.ndk) then
        error("unable to determine Android package versions from " .. setup_android_bat, 0)
    end
    return baseline
end

local function package_plan(spec)
    local sdk, baseline = spec.sdk_root, spec.baseline
    assert(sdk and baseline and spec.project_platform and spec.java and spec.sdkmanager, "incomplete Android prerequisite specification")
    local candidates = {
        { "platform-tools", join(sdk, "platform-tools", "adb.exe") },
        { "platforms;" .. baseline.platform, join(sdk, "platforms", baseline.platform, "android.jar") },
        { "platforms;" .. spec.project_platform, join(sdk, "platforms", spec.project_platform, "android.jar") },
        { "build-tools;" .. baseline.build_tools, join(sdk, "build-tools", baseline.build_tools, "aapt2.exe") },
        { "cmake;" .. baseline.cmake, join(sdk, "cmake", baseline.cmake, "bin", "cmake.exe") },
        { "ndk;" .. baseline.ndk, join(sdk, "ndk", baseline.ndk, "source.properties") },
    }
    local missing, seen = {}, {}
    for _, candidate in ipairs(candidates) do
        if not seen[candidate[1]] and not file_exists(candidate[2]) then missing[#missing + 1] = candidate[1] end
        seen[candidate[1]] = true
    end
    return missing
end

function M.check(spec)
    local missing = {}
    assert(file_exists(spec.java), "Java executable not found: " .. tostring(spec.java))
    assert(file_exists(spec.sdkmanager), "sdkmanager not found: " .. tostring(spec.sdkmanager))
    if not file_exists(join(spec.sdk_root, "licenses", "android-sdk-license")) then missing[#missing + 1] = "licenses" end
    for _, package in ipairs(package_plan(spec)) do missing[#missing + 1] = package end
    return #missing == 0, missing
end

function M.ensure(spec, process_runner)
    assert(type(process_runner) == "function", "process_runner is required")
    assert(file_exists(spec.java), "Java executable not found: " .. tostring(spec.java))
    assert(file_exists(spec.sdkmanager), "sdkmanager not found: " .. tostring(spec.sdkmanager))
    local licenses = file_exists(join(spec.sdk_root, "licenses", "android-sdk-license"))
    local packages = package_plan(spec)
    local java_home = assert(spec.java:match("^(.*)[\\/]bin[\\/]java%.exe$"), "java must end in bin/java.exe")
    if not licenses then
        process_runner({ stage="accept Android SDK licenses", exe=spec.sdkmanager, cwd=spec.sdk_root,
            log_path=join(spec.sdk_root, "sdkmanager-licenses.log"), env={JAVA_HOME=java_home}, args={"--licenses"} })
    end
    if #packages > 0 then
        process_runner({ stage="install Android SDK packages", exe=spec.sdkmanager, cwd=spec.sdk_root,
            log_path=join(spec.sdk_root, "sdkmanager-install.log"), env={JAVA_HOME=java_home}, args=packages })
    end
    return true
end

return M
