assert(os == nil and io == nil and require == nil)
for _, contribution in ipairs({
  { id="lua-bulk-folder:button", kind="command" },
  { id="lua-bulk-folder:form", kind="form" },
  { id="lua-bulk-folder:plan", kind="operation_plan" }
}) do
  contribution.feature_id="lua-bulk-folder"
  contribution.capabilities={"filesystem.write"}
  superexplorer.register(contribution)
end
