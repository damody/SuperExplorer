# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Copy { $count } item
       *[other] Copy { $count } items
    }
move-items =
    { $count ->
        [one] Move { $count } item
       *[other] Move { $count } items
    }
recycle-items =
    { $count ->
        [one] Recycle { $count } item
       *[other] Recycle { $count } items
    }
permanent-delete-items =
    { $count ->
        [one] Permanently delete { $count } item
       *[other] Permanently delete { $count } items
    }
gdrive-trash-items =
    { $count ->
        [one] Move { $count } item to Google Drive trash
       *[other] Move { $count } items to Google Drive trash
    }
shortcut-items =
    { $count ->
        [one] Create shortcut for { $count } item
       *[other] Create shortcut for { $count } items
    }
extra-items =
    { $count ->
        [one] { $first } ({ $count } more item)
       *[other] { $first } ({ $count } more items)
    }
clipboard-source = Source provided by the system clipboard
op-new-folder = New folder | { $path }
op-new-file = New file | { $path }
op-rename = Rename | { $from } → { $to }
op-chmod = Change permissions | { $path } → { $mode }
op-copy-route = { $action } | { $source } → { $destination }
op-move-route = { $action } | { $source } → { $destination }
op-recycle-route = { $action } | { $source }
op-permanent-delete-route = { $action } | { $source }
op-gdrive-trash-route = { $action } | { $source }
op-shortcut-route = { $action } | { $source }
op-preparing-copy = Preparing to copy
op-copying = Copying
op-copy-complete = Copy complete
op-preparing-move = Preparing to move
op-moving = Moving
op-move-complete = Move complete
op-preparing = Preparing
op-processing = Working
op-complete = Done
op-finalizing = Finalizing
op-progress-items = { $summary } | { $phase } | Progress { $completed }/{ $total } items
op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Progress { $completed }/{ $total } items
op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Progress { $completed }/{ $total } items
op-finished = { $phase } | { $summary }
op-done = { $summary } | Done
op-cancelled = { $summary } | Cancelled
op-failed = { $summary } | Failed: { $error }
op-partial = { $summary } | Partial: { $succeeded }/{ $total } succeeded
op-cancelling = { $summary } | Cancelling
op-success-route = Succeeded | { $route }
op-skipped-route = Skipped | { $route }
op-cancelled-route = Cancelled | { $route }
op-partial-status = Partial
op-failed-status = Failed
op-error-code = | Error code { $code }
op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = Destination not provided
op-no-source = Source not provided
apk-installing = Installing: { $name } → { $target }
apk-installed = Installed: { $name } → { $target }
apk-cancelled = Cancelled installing { $name } to { $target }
apk-timeout = Installing { $name } to { $target } timed out
apk-failed = Installing { $name } to { $target } failed: { $error }
apk-check-device = Check the device connection and APK, then try again
status-no-selection = No items selected
status-details-unavailable = Could not load details
status-item-count =
    { $count ->
        [one] { $count } item
       *[other] { $count } items
    }
status-preview-select-one = Select an item to preview
status-preview-failed = Could not generate a preview for this file
status-preview-loading = Loading preview…
status-preview-item-failed = Could not load the preview item
status-preview-select-single = Select a single item to preview
status-no-transfers = No file operations during this session
status-thumbnail-cleared = Thumbnail cache cleared
status-thumbnail-clear-partial = Could not fully clear the thumbnail cache; you can retry, and browsing still works
status-invalid-folder-name = Invalid folder name. Correct it and try again.
status-name-conflict = An item with that name already exists here.
status-context-menu-unresponsive = The context menu did not respond. You can keep working.
status-cannot-go-forward = Cannot go forward
status-partial-folders = Some folders could not be listed.
status-cannot-list-folder = Could not list the folder.
status-cannot-cancel-disconnected = Cannot cancel: the file service is not connected.
status-cannot-cancel-operation = Could not cancel the file operation: { $error }
status-cannot-load-folder = Could not load the folder. Try again.
status-drive-free-of = { $free } free of { $total }
status-no-media = No media
status-disconnected = Disconnected
status-access-denied = Access denied
status-capacity-unavailable = Capacity unavailable
status-waiting-file-count = Waiting for File Count…
status-file-count-limit = Depends on File Count, so it did not start
status-file-count-over-limit = File Count exceeded the limit, so it did not start
status-file-count-pending-label = Limit
transfer-upload = Destination upload
transfer-download = Source download
transfer-conflict-inspection = Destination conflict check
transfer-local-copy = Local copy
transfer-source-delete = Delete source after move
transfer-provider-panic = Transfer provider error
transfer-no-diagnostic = No underlying error provided
transfer-cancelling = Cancelling
transfer-cancel = Cancel
transfer-cancel-operation = Cancel file operation
transfer-open-location = Open transfer location
status-search-fallback = Searching (filesystem fallback; index unavailable) · 
status-search-indexed = Searching (indexed + fallback) · 
status-search-partial = Partial search results · 
status-search-error = Search error · 
status-search-cancelled = Search cancelled · 
status-search-results = Search results · 
status-loading = Loading · 
status-error = Error · 
status-items-selected = { $count } items — { $selected } selected
status-operation-progress = Operation { $completed }/{ $total } · { $name }
status-details-view = Details view
status-icon-view = Icon view
status-lock-discovery-timeout = Lock owner discovery reached its deadline.
status-lock-retrying = Retrying the delete operation…
status-lock-partial-close = Some applications did not close. No process was force-terminated.
status-lock-close-cancelled = Closing applications was cancelled.
status-lock-finding = Finding applications that are using the selected item…
status-lock-retry-limit = The retry limit was reached. The item was not deleted.
status-lock-owners-found = { $count } application(s) are using the selected item.
status-lock-unidentified = Windows could not identify the application using this item.

status-generic-item = item

status-operation-queue-failed = The operation could not be queued, but Explorer can continue.
status-folder-options-save-failed = Unable to save Folder Options. Check the session storage and try again.
status-bookmark-unavailable = Bookmark is no longer available.
status-bookmark-delete-window-failed = Unable to open the bookmark delete confirmation window.
status-bookmark-saved = Bookmark saved.
status-bookmark-save-failed = Unable to save the bookmark.
status-bookmark-name-required = Bookmark name and target are required.
status-bookmark-renamed = Bookmark renamed.
status-bookmark-rename-failed = Unable to rename the bookmark.
status-bookmark-editor-window-failed = Unable to open the bookmark editor window.
status-bookmark-manager-window-failed = Unable to open the bookmark manager window.
status-bookmark-folder-editor-window-failed = Unable to open the bookmark folder editor window.
status-bookmark-folder-created = Bookmark folder created.
status-bookmark-folder-save-failed = Unable to save the bookmark folder.
status-bookmark-folder-renamed = Bookmark folder renamed.
status-bookmark-folder-rename-failed = Unable to rename the bookmark folder.
status-bookmark-folder-name-required = Bookmark folder name is required.
status-bookmark-folder-removed = Bookmark folder removed.
status-bookmark-folder-remove-failed = Unable to remove the bookmark folder.
status-bookmark-folder-delete-window-failed = Unable to open the bookmark folder delete confirmation window.
status-bookmark-removed = Bookmark removed.
status-bookmark-remove-failed = Unable to remove the bookmark.
status-bookmark-removal-save-failed = Unable to save the bookmark removal.
status-bookmark-order-updated = Bookmark order updated.
status-bookmark-order-save-failed = Unable to save the bookmark order.
status-bookmark-moved = Bookmark moved.
status-bookmark-move-save-failed = Unable to save the bookmark move.
status-bookmark-backup-copied = Bookmark backup copied to the clipboard.
status-bookmark-backup-failed = Unable to back up bookmarks: { $error }
status-bookmark-imported = Bookmarks imported from the clipboard.
status-bookmark-import-persist-failed = Unable to persist imported bookmarks.
status-bookmark-import-invalid = Clipboard does not contain a valid bookmark backup.
status-bookmark-folder-missing = Unable to open bookmark: the folder no longer exists.
status-bookmark-path-unavailable = Unable to open bookmark: the folder path is unavailable or invalid.
status-bookmark-opened = Bookmark opened.
status-bookmark-open-failed = Unable to open bookmark: { $error }
status-bookmark-file-launcher-unavailable = File launcher is unavailable
status-lua-bookmark-need-folder = Lua bookmarks require a filesystem folder.
status-lua-bookmark-completed = Lua bookmark completed.
status-lua-bookmark-timed-out = Lua bookmark timed out.
status-lua-bookmark-failed = Lua bookmark failed: { $error }
