assert(os == nil and io == nil and require == nil)
superexplorer.register {
  id = "lua-tokei:column", feature_id = "lua-tokei", kind = "column",
  capabilities = { "filesystem.read", "tools.execute_bundled" }
}
