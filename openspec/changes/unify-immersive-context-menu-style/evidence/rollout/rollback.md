# Rollback

Set `ViewSettings.immersive_native_context_menus` to `false` for the active tab/profile. New Local context-menu requests then return `Disabled` before the capability provider is called and use the unchanged Shell `TrackPopupMenuEx` path. No bookmarks, paths, menu contents, extension state, or session schema migration is required. A stored `false` value round-trips; legacy profiles without the field default to enabled because the completed feature is user-requested and has passed its implementation gates.
