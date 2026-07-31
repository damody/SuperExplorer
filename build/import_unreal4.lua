package.path = "build/lib/?.lua;" .. package.path

local unreal4_import = require("unreal4_import")

local source, destination = arg[1], arg[2]
assert(source and destination, "usage: import_unreal4.lua <source> <destination>")

local expected_destination = "D:\\unity_samples5\\Unreal4ARPG"
local copied = unreal4_import.import(source, destination, expected_destination)
print(string.format("[PASS] imported %d files", copied))
