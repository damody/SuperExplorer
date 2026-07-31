local lfs = require("lfs")

local root = assert(arg[1], "a disposable absolute test directory is required")
local sep = package.config:sub(1, 1)
local function join(a, b)
  return a .. (a:sub(-1) == sep and "" or sep) .. b
end
local function expect(value, message)
  if not value then error(message, 2) end
  return value
end

local traditional = utf8.char(0x7E41, 0x9AD4, 0x4E2D, 0x6587)
local japanese = utf8.char(0x65E5, 0x672C, 0x8A9E)
local korean = utf8.char(0xD55C, 0xAE00)
local emoji = utf8.char(0x1F600)
local combining = "e" .. utf8.char(0x301)
local directory_name = traditional .. "-" .. japanese .. "-" .. korean .. "-" .. emoji .. "-" .. combining
local directory = join(root, directory_name)

expect(lfs.mkdir(root) or lfs.attributes(root, "mode") == "directory", "cannot create test root")
expect(lfs.mkdir(directory), "cannot create Unicode directory")
expect(lfs.attributes(directory, "mode") == "directory", "cannot stat Unicode directory")

local removable = join(directory, traditional .. "-empty")
expect(lfs.mkdir(removable), "cannot create removable Unicode directory")
expect(lfs.rmdir(removable), "cannot remove Unicode directory")

local original = expect(lfs.currentdir(), "cannot read current directory")
expect(lfs.chdir(directory), "cannot chdir into Unicode directory")
expect(lfs.currentdir():sub(-#directory_name) == directory_name, "currentdir did not return UTF-8")
expect(lfs.chdir(original), "cannot restore current directory")

local source = join(root, "ascii-source.txt")
local handle = assert(io.open(source, "wb"))
handle:write("unicode hard-link payload")
handle:close()
local unicode_file_name = traditional .. "-" .. japanese .. "-" .. emoji .. ".txt"
local unicode_file = join(root, unicode_file_name)
expect(lfs.link(source, unicode_file, false), "cannot create Unicode hard link")
expect(lfs.attributes(unicode_file, "size") == 25, "cannot stat Unicode file")
expect(lfs.touch(unicode_file, os.time() - 5, os.time() - 5), "cannot touch Unicode file")

local found_directory, found_file = false, false
for name in lfs.dir(root) do
  if name == directory_name then found_directory = true end
  if name == unicode_file_name then found_file = true end
end
expect(found_directory, "Unicode directory name was not returned as UTF-8")
expect(found_file, "Unicode file name was not returned as UTF-8")

local lock = expect(lfs.lock_dir(directory), "cannot lock Unicode directory")
lock:free()

local deep = directory
for index = 1, 14 do
  deep = join(deep, traditional .. string.format("-%02d-abcdefghijklmnop", index))
  expect(lfs.mkdir(deep), "cannot create long Unicode directory at segment " .. index)
end
expect(#deep > 260, "long-path fixture did not exceed MAX_PATH")
expect(lfs.attributes(deep, "mode") == "directory", "cannot stat long Unicode directory")

local invalid_utf8 = root .. sep .. string.char(0xC3, 0x28)
expect(lfs.attributes(invalid_utf8) == nil, "invalid UTF-8 path was accepted")

print(string.format("LuaFileSystem Windows UTF-8 test passed: path_bytes=%d", #deep))
