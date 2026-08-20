local lfs = require("lfs")
local M = {}

M.approved_superdesktop_url = "https://github.com/damody/SuperDesktop.git"

function M.parse_options(values)
    local options = {
        check = false,
        skip_build = false,
        no_launch = false,
        component = nil,
        allow_superexplorer_dirty = false,
        allow_superdesktop_dirty = false,
        ignore_superdesktop_evidence_logs = false,
    }
    local index = 1
    while index <= #values do
        local value = values[index]
        if value == "--check" then
            options.check = true
        elseif value == "--skip-build" then
            options.skip_build = true
        elseif value == "--no-launch" then
            options.no_launch = true
        elseif value == "--allow-superexplorer-dirty" then
            options.allow_superexplorer_dirty = true
        elseif value == "--allow-superdesktop-dirty" then
            options.allow_superdesktop_dirty = true
        elseif value == "--ignore-superdesktop-evidence-logs" then
            options.ignore_superdesktop_evidence_logs = true
        elseif value == "--component" then
            index = index + 1
            if index > #values then error("--component 缺少模式", 0) end
            if options.component then error("只能指定一個 --component 模式", 0) end
            options.component = values[index]
        elseif value:match("^%-%-component=") then
            if options.component then error("只能指定一個 --component 模式", 0) end
            options.component = value:match("^%-%-component=(.*)$")
        else
            error("未知參數：" .. tostring(value), 0)
        end
        index = index + 1
    end
    if options.component ~= "all" and options.component ~= "superexplorer" and options.component ~= "superdesktop" then
        error("必須指定唯一 --component all|superexplorer|superdesktop", 0)
    end
    if options.allow_superexplorer_dirty and options.component ~= "superexplorer" then
        error("--allow-superexplorer-dirty 只能用於 superexplorer 模式", 0)
    end
    if options.allow_superdesktop_dirty and options.component ~= "superdesktop" then
        error("--allow-superdesktop-dirty 只能用於 superdesktop 模式", 0)
    end
    if options.ignore_superdesktop_evidence_logs and options.component ~= "all" then
        error("--ignore-superdesktop-evidence-logs can only be used with --component all", 0)
    end
    options.include_superexplorer = options.component == "all" or options.component == "superexplorer"
    options.include_superdesktop = options.component == "all" or options.component == "superdesktop"
    return options
end

function M.filter_superdesktop_status(status, ignore_evidence_logs)
    status = tostring(status or "")
    if status == "" then return status end

    local remaining = {}
    for line in status:gmatch("[^\r\n]+") do
        local status_path = line:sub(4):gsub("\\", "/"):gsub('^"', ""):gsub('"$', "")
        local generated_test_result = not status_path:find(" -> ", 1, true)
            and (status_path == "utit-results" or status_path:match("^utit%-results/") ~= nil)
        local untracked_path = line:match("^%?%? (.+)$")
        if untracked_path then untracked_path = untracked_path:gsub("\\", "/") end
        local generated_evidence_log = ignore_evidence_logs and untracked_path
            and untracked_path:match("^openspec/.+/evidence/.+%.log$") ~= nil
        if not generated_test_result and not generated_evidence_log then
            remaining[#remaining + 1] = line
        end
    end
    return table.concat(remaining, "\n")
end

function M.validate_submodule_identity(identity)
    if not identity.initialized then error("SuperDesktop submodule 尚未初始化", 0) end
    if not tostring(identity.head or ""):match("^[0-9a-fA-F]+$") then error("SuperDesktop HEAD 無效", 0) end
    if identity.declared_url ~= M.approved_superdesktop_url
        or identity.configured_url ~= M.approved_superdesktop_url
        or identity.origin_url ~= M.approved_superdesktop_url then
        error("SuperDesktop submodule origin 與核准 URL 不符", 0)
    end
    if identity.mode ~= "160000" or not tostring(identity.gitlink or ""):match("^[0-9a-fA-F]+$") then
        error("SuperDesktop parent gitlink 缺失或模式無效", 0)
    end
    if identity.head:lower() ~= identity.gitlink:lower() then
        error("SuperDesktop HEAD 與 parent gitlink 不符", 0)
    end
    if identity.status and identity.status ~= "" then
        error("SuperDesktop 含有未提交的 product/build source，禁止正式封裝：\n  "
            .. identity.status:gsub("[\r\n]+", "\n  "), 0)
    end
    return true
end

function M.validate_pe(file_path, description)
    if lfs.attributes(file_path, "mode") ~= "file" then
        error((description or "必要執行檔") .. "不存在：" .. file_path, 0)
    end
    local file = assert(io.open(file_path, "rb"))
    local signature = file:read(2)
    local size = assert(file:seek("end"))
    assert(file:close())
    if signature ~= "MZ" or size < 1024 then
        error((description or "必要執行檔") .. "不是有效的 Windows 執行檔：" .. file_path, 0)
    end
    return size
end

return M
