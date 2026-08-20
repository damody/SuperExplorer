local M = {}

local function trim(value)
    return tostring(value):match("^%s*(.-)%s*$")
end

function M.parse_commit_timestamp(value)
    local timestamp = trim(value)
    local year, month_text, day_text = timestamp:match("^(%d%d%d%d)%-(%d%d)%-(%d%d)T")
    if not year then error("invalid Git ISO commit timestamp: " .. timestamp, 0) end

    local month, day = tonumber(month_text), tonumber(day_text)
    local numeric_year = tonumber(year)
    local leap = numeric_year % 4 == 0 and (numeric_year % 100 ~= 0 or numeric_year % 400 == 0)
    local month_days = {31, leap and 29 or 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31}
    if month < 1 or month > 12 or day < 1 or day > month_days[month] then
        error("invalid Git commit date: " .. timestamp, 0)
    end

    return {
        timestamp = timestamp,
        iso_date = string.format("%s-%s-%s", year, month_text, day_text),
        month = month,
        day = day,
        sdk_version = string.format("5.%d.%d", month, day),
    }
end

function M.parse_timestamp(value)
    local parsed = M.parse_commit_timestamp(value)
    if not parsed.timestamp:match("^%d%d%d%d%-%d%d%-%d%dT%d%d:%d%d:%d%d[+-]%d%d:%d%d$") then
        error("invalid Git ISO commit timestamp: " .. parsed.timestamp, 0)
    end
    parsed.version = parsed.sdk_version
    return parsed
end

local function quote_windows(value)
    return '"' .. tostring(value):gsub('"', '""') .. '"'
end

function M.resolve(repo_root)
    local command = "git -C " .. quote_windows(repo_root) .. " show -s --format=%H%n%cI HEAD 2>&1"
    local pipe = assert(io.popen(command, "r"))
    local output = pipe:read("*a")
    local ok = pipe:close()
    if not ok then
        local detail = trim(output)
        if detail == "" then detail = "Git 未提供錯誤輸出" end
        error("無法讀取 Git 提交資訊：" .. tostring(repo_root) .. "\n" .. detail, 0)
    end
    local commit, timestamp, extra = output:match("^([^\r\n]+)\r?\n([^\r\n]+)\r?\n?(.*)$")
    if not commit or extra ~= "" or not commit:match("^%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x$") then
        error("Git 提交資訊格式無效：\n" .. trim(output), 0)
    end
    local parsed = M.parse_timestamp(timestamp)
    return { commit = commit, iso_date = parsed.iso_date, version = parsed.version }
end

function M.replace_workspace_version(text, sdk_version)
    local updated, count = text:gsub(
        "(%[workspace%.package%][^\r\n]*\r?\n%s*version%s*=%s*)\"[^\"]+\"",
        "%1\"" .. sdk_version .. "\"",
        1
    )
    if count ~= 1 then error("expected exactly one [workspace.package] version", 0) end
    return updated
end

function M.replace_lock_versions(text, package_names, sdk_version)
    local replaced = {}
    local updated = text:gsub(
        "(%[%[package%]%]%s*[\r\n]+name%s*=%s*\"([^\"]+)\"%s*[\r\n]+version%s*=%s*)\"([^\"]+)\"",
        function(prefix, name, existing_version)
            if not package_names[name] then return prefix .. '"' .. existing_version .. '"' end
            replaced[name] = (replaced[name] or 0) + 1
            return prefix .. '"' .. sdk_version .. '"'
        end
    )

    for name in pairs(package_names) do
        if replaced[name] ~= 1 then
            error("expected exactly one Cargo.lock package entry for " .. name, 0)
        end
    end
    return updated
end

function M.resolve_from_git(capture_fn)
    local commit = trim(capture_fn("git rev-parse HEAD"))
    if not commit:match("^%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x%x$") then
        error("invalid Git HEAD commit hash: " .. commit, 0)
    end

    local parsed = M.parse_commit_timestamp(capture_fn("git show -s --format=%cI HEAD"))
    parsed.commit = commit
    return parsed
end

return M
