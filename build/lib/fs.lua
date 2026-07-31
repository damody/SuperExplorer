local lfs = require("lfs")

local M = {}
local separator = package.config:sub(1, 1)

local function join(left, right)
    if left:sub(-1) == separator then return left .. right end
    return left .. separator .. right
end

function M.exists(path)
    return lfs.attributes(path) ~= nil
end

function M.mkdir_p(path)
    if M.exists(path) then return true end
    local normalized = path:gsub("[\\/]", separator)
    local current = ""
    if normalized:match("^%a:" .. separator) then
        current = normalized:sub(1, 3)
        normalized = normalized:sub(4)
    elseif normalized:sub(1, 1) == separator then
        current = separator
        normalized = normalized:sub(2)
    end
    for part in normalized:gmatch("[^" .. separator .. "]+") do
        current = current == "" and part or join(current, part)
        if not M.exists(current) then assert(lfs.mkdir(current)) end
    end
    return true
end

function M.remove_tree(path)
    local attributes = lfs.attributes(path)
    if not attributes then return false end
    if attributes.mode ~= "directory" then
        assert(os.remove(path))
        return true
    end
    for name in lfs.dir(path) do
        if name ~= "." and name ~= ".." then M.remove_tree(join(path, name)) end
    end
    assert(lfs.rmdir(path))
    return true
end

function M.same_file(left, right)
    local left_attr, right_attr = lfs.attributes(left), lfs.attributes(right)
    if not left_attr or not right_attr or left_attr.mode ~= "file" or right_attr.mode ~= "file" then return false end
    if left_attr.size ~= right_attr.size then return false end
    local a, b = assert(io.open(left, "rb")), assert(io.open(right, "rb"))
    while true do
        local ac, bc = a:read(65536), b:read(65536)
        if ac ~= bc then a:close(); b:close(); return false end
        if not ac then break end
    end
    a:close(); b:close()
    return true
end

function M.copy_if_different(source, destination)
    assert(lfs.attributes(source, "mode") == "file", "copy source is not a file: " .. source)
    if M.same_file(source, destination) then return false end
    local parent = destination:match("^(.*)[\\/][^\\/]+$")
    if parent then M.mkdir_p(parent) end
    local input, output = assert(io.open(source, "rb")), assert(io.open(destination, "wb"))
    while true do
        local chunk = input:read(65536)
        if not chunk then break end
        assert(output:write(chunk))
    end
    input:close(); output:close()
    assert(M.same_file(source, destination), "copy verification failed: " .. destination)
    return true
end

function M.copy_tree(source, destination, include)
    assert(lfs.attributes(source, "mode") == "directory", "tree source is not a directory: " .. source)
    local copied = 0
    local function visit(directory, relative)
        for name in lfs.dir(directory) do
            if name ~= "." and name ~= ".." then
                local source_path = join(directory, name)
                local relative_path = relative == "" and name or (relative .. "/" .. name)
                local mode = assert(lfs.attributes(source_path, "mode"))
                if not include or include(relative_path, mode) then
                    local destination_path = join(destination, relative_path:gsub("/", separator))
                    if mode == "directory" then
                        M.mkdir_p(destination_path)
                        visit(source_path, relative_path)
                    elseif mode == "file" then
                        if M.copy_if_different(source_path, destination_path) then copied = copied + 1 end
                    end
                end
            end
        end
    end
    M.mkdir_p(destination)
    visit(source, "")
    return copied
end

return M
