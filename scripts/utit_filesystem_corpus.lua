local lfs = require("lfs")

local root = assert(arg[1], "fixture root is required")
local profile = arg[2] or "small"
local bulk_count = tonumber(arg[3]) or (profile == "soak" and 20000 or 2000)

local separator = package.config:sub(1, 1)

local function join(left, right)
    return left .. (left:sub(-1) == separator and "" or separator) .. right:gsub("[/\\]", separator)
end

local function mkdir_p(path)
    local normalized = path:gsub("/", separator)
    local prefix = normalized:match("^%a:[\\]") and normalized:sub(1, 3) or ""
    local cursor = prefix
    local remainder = prefix ~= "" and normalized:sub(4) or normalized
    for component in remainder:gmatch("[^\\/]+") do
        cursor = cursor == "" and component or join(cursor, component)
        local attributes = lfs.attributes(cursor)
        if not attributes then
            assert(lfs.mkdir(cursor), "cannot create directory: " .. cursor)
        elseif attributes.mode ~= "directory" then
            error("path component is not a directory: " .. cursor)
        end
    end
end

local function parent(path)
    return path:match("^(.*)[\\/][^\\/]+$")
end

local function write_file(relative, contents)
    local path = join(root, relative)
    mkdir_p(assert(parent(path)))
    local file = assert(io.open(path, "wb"), "cannot create file: " .. path)
    assert(file:write(contents))
    file:close()
end

local linked_file_index = 0
local function write_linked_file(relative, contents)
    linked_file_index = linked_file_index + 1
    local staging_relative = string.format(".lfs-unicode-stage-%03d.tmp", linked_file_index)
    local staging_path = join(root, staging_relative)
    write_file(staging_relative, contents)
    local destination = join(root, relative)
    mkdir_p(assert(parent(destination)))
    local ok, message = lfs.link(staging_path, destination, false)
    assert(ok, message or ("cannot create linked file: " .. destination))
    assert(os.remove(staging_path), "cannot remove linked-file staging source")
end

local function repeated(seed, length)
    if length == 0 then return "" end
    local copies = math.ceil(length / #seed)
    return string.rep(seed, copies):sub(1, length)
end

mkdir_p(root)
mkdir_p(join(root, "00-empty-folder"))
mkdir_p(join(root, "01-nested-empty/level-a/level-b/level-c"))
mkdir_p(join(root, "02-unicode"))
local traditional = utf8.char(0x7E41, 0x9AD4, 0x4E2D, 0x6587)
local japanese = utf8.char(0x65E5, 0x672C, 0x8A9E)
local korean = utf8.char(0xD55C, 0xAE00)
local search = utf8.char(0x641C, 0x5C0B)
local emoji = utf8.char(0x1F600)
mkdir_p(join(root, "02-unicode/" .. traditional .. "-" .. japanese .. "-" .. korean .. "-emoji-" .. emoji))
write_linked_file("02-unicode/" .. traditional .. ".txt", "traditional chinese fixture\n")
write_linked_file("02-unicode/" .. japanese .. ".dat", "japanese fixture\n")
write_linked_file("02-unicode/" .. korean .. ".bin", "korean fixture\n")
write_linked_file("02-unicode/emoji-" .. emoji .. ".txt", "emoji fixture\n")
write_linked_file("02-unicode/combining-e" .. utf8.char(0x301) .. ".txt", "combining mark fixture\n")
write_file("02-unicode/spaces (round) #hash %percent.txt", "punctuation fixture\n")

write_file("03-content/empty.bin", "")
write_file("03-content/one-byte.bin", "x")
write_file("03-content/duplicate-a.bin", repeated("duplicate-payload-", 8192))
write_file("03-content/duplicate-b.bin", repeated("duplicate-payload-", 8192))
write_file("03-content/same-size-different-a.bin", repeated("A1", 4096))
write_file("03-content/same-size-different-b.bin", repeated("B2", 4096))
write_file("03-content/small.txt", repeated("s", 17))
write_file("03-content/medium.log", repeated("m", 1024))
write_file("03-content/large.data", repeated("LARGE", 65536))

write_file("04-search/Needle-Alpha.txt", "search alpha\n")
write_file("04-search/needle-beta.TXT", "search beta\n")
write_linked_file("04-search/" .. search .. "-Needle-" .. traditional .. ".txt", "search unicode\n")
write_file("04-search/no-match.dat", "control\n")

write_file("05-mutation/rename-source.txt", "rename-source\n")
write_file("05-mutation/copy-source.txt", "copy-source\n")
write_file("05-mutation/move-source.txt", "move-source\n")
write_file("05-mutation/delete-source.txt", "delete-source\n")
write_file("05-mutation/readonly-source.txt", "readonly-source\n")
mkdir_p(join(root, "05-mutation/destination"))
mkdir_p(join(root, "05-mutation/empty-delete"))

local deep = "06-deep"
for index = 1, 18 do
    deep = deep .. string.format("/segment-%02d-abcdefghijklmnop", index)
end
mkdir_p(join(root, deep))
write_linked_file(deep .. "/deep-leaf.txt", "deep path fixture\n")

if profile == "full" or profile == "soak" then
    local extensions = { "txt", "bin", "log", "dat", "tmp" }
    for index = 1, bulk_count do
        local extension = extensions[((index - 1) % #extensions) + 1]
        local relative = string.format("07-bulk/item-%05d.%s", index, extension)
        write_file(relative, repeated(string.format("%05d", index), (index % 127) + 1))
    end
end

local marker = assert(io.open(join(root, "corpus-generator.txt"), "wb"))
marker:write(string.format("schema=1\nprofile=%s\nbulk_count=%d\n", profile, bulk_count))
marker:close()

print(string.format("filesystem corpus generated: root=%s profile=%s bulk=%d", root, profile, bulk_count))
