# One-step rollback

Set `immersive_native_context_menus` to `false` in the persisted view settings, then open the
next Local context menu. The request
bypasses `immersive_popup::present` and uses the unchanged `TrackPopupMenuEx` path.

No bookmark, path, remote profile, Shell command, HMENU, or file-content migration is involved.
ADB/SFTP visual tokens are independent and remain functional. New and legacy profiles with the
field absent default to the application-owned Local popup; an explicitly persisted `false`
continues to select the native fallback and is not rewritten by deserialization.
