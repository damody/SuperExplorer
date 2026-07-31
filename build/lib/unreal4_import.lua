local lfs = require("lfs")
local fs = require("fs")
local import_filter = require("import_filter")

local M = {}
local separator = package.config:sub(1, 1)

local function join(left, right)
    if left:sub(-1) == separator then return left .. right end
    return left .. separator .. right
end

local function lexical_absolute(path)
    local normalized = tostring(path):gsub("/", "\\")
    if not normalized:match("^%a:\\") then
        normalized = lfs.currentdir() .. "\\" .. normalized
    end
    local drive, rest = normalized:match("^(%a:)[\\](.*)$")
    assert(drive, "path must resolve to an absolute Windows path: " .. tostring(path))
    local parts = {}
    for part in rest:gmatch("[^\\]+") do
        if part == ".." then
            assert(#parts > 0, "path escapes drive root: " .. tostring(path))
            table.remove(parts)
        elseif part ~= "." and part ~= "" then
            parts[#parts + 1] = part
        end
    end
    return drive:upper() .. "\\" .. table.concat(parts, "\\")
end

function M.canonicalize(path)
    local absolute = lexical_absolute(path)
    if lfs.attributes(absolute, "mode") == "directory" then
        local previous = assert(lfs.currentdir())
        assert(lfs.chdir(absolute))
        absolute = assert(lfs.currentdir())
        assert(lfs.chdir(previous))
    end
    return absolute:gsub("\\+$", "")
end

local function same(left, right)
    return left:lower() == right:lower()
end

local function within(path, directory)
    local lower_path, lower_directory = path:lower(), directory:lower()
    return lower_path:sub(1, #lower_directory + 1) == lower_directory .. "\\"
end

local function unlink(path, mode)
    if mode == "directory" then
        assert(lfs.rmdir(path))
    elseif not os.remove(path) then
        assert(lfs.rmdir(path))
    end
end

local function safe_remove_tree(path)
    local mode = lfs.symlinkattributes(path, "mode")
    if not mode then return false end
    if mode == "link" then
        unlink(path, mode)
        return true
    end
    if mode ~= "directory" then
        assert(os.remove(path))
        return true
    end
    for name in lfs.dir(path) do
        if name ~= "." and name ~= ".." then safe_remove_tree(join(path, name)) end
    end
    assert(lfs.rmdir(path))
    return true
end

local function safe_copy_tree(source, destination)
    local copied = 0
    fs.mkdir_p(destination)
    local function visit(directory, relative)
        for name in lfs.dir(directory) do
            if name ~= "." and name ~= ".." then
                local source_path = join(directory, name)
                local relative_path = relative == "" and name or (relative .. "/" .. name)
                local mode = assert(lfs.symlinkattributes(source_path, "mode"))
                assert(mode ~= "link", "source tree contains a link or reparse point: " .. source_path)
                if import_filter.include(relative_path, mode) then
                    local destination_path = join(destination, relative_path:gsub("/", separator))
                    if mode == "directory" then
                        fs.mkdir_p(destination_path)
                        visit(source_path, relative_path)
                    elseif mode == "file" and fs.copy_if_different(source_path, destination_path) then
                        copied = copied + 1
                    end
                end
            end
        end
    end
    visit(source, "")
    return copied
end

function M.import(source, destination, expected_destination)
    local expected_literal = lexical_absolute(expected_destination):gsub("\\+$", "")
    source = M.canonicalize(source)
    destination = M.canonicalize(destination)
    expected_destination = M.canonicalize(expected_destination)

    assert(same(expected_destination, expected_literal),
        "expected destination resolves outside its workspace path")
    assert(same(destination, expected_destination),
        "destination must be the workspace Unreal4ARPG directory: " .. expected_destination)
    assert(not same(source, destination), "source and destination must differ")
    assert(not within(destination, source), "destination must not be inside source")
    assert(not within(source, destination), "source must not be inside destination")
    assert(lfs.attributes(source, "mode") == "directory", "source is not a directory: " .. source)

    if lfs.attributes(destination) then
        assert(same(destination, expected_destination), "refusing to clean an unvalidated destination")
        safe_remove_tree(destination)
    end
    return safe_copy_tree(source, destination)
end

return M
