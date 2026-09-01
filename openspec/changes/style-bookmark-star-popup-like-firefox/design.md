# Design

The existing dedicated bookmark editor window remains the lifecycle boundary, but its size and content become a compact browser-style card. Folder/file bookmarks hide the editable target payload and show only the quick workflow; Lua bookmarks retain their source field. The name input selects its full document and receives focus after window creation. Root key capture maps Enter to Save and Escape to Cancel. Existing mutations and rollback-on-persistence-failure remain unchanged.
