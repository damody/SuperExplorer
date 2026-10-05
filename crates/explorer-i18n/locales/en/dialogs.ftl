# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialog-transfer-conflict-title = The destination already exists
dialog-transfer-conflict-body = Overwrite files with the same names in this destination? Cancel stops this transfer. For folders, other destination files are kept.
dialog-transfer-overwrite = Overwrite
dialogs-folder-options = Folder Options
dialog-about = About SuperExplorer
dialog-version = Version
dialog-build-date = Build date
dialog-author = Author
dialog-purpose = Purpose: { $value }
dialog-author-line = Author: { $name } — { $bio } · { $date }
dialog-community = Community: { $url }
dialog-plugin-safe-mode-title = Plugin Safe Mode
dialog-plugin-safe-mode-body = Plugin Safe Mode is on. Select plugins to re-enable, choose Apply or OK, then restart SuperExplorer.
dialog-safe-mode-confirm-title = An extension was temporarily disabled
dialog-safe-mode-confirm-aria = Extension temporarily disabled: { $package }
dialog-suspect-package = Affected feature: { $package }
dialog-interface = Interface: { $value }
dialog-operation = Operation: { $value }
dialog-confirm-reenable = Allow loading on next startup
dialog-bookmark-action = Bookmark action
dialog-delete-bookmark = Delete bookmark
dialog-delete-bookmark-prompt = Delete bookmark “{ $name }”?
dialog-delete-bookmark-note = This removes the bookmark. Files on disk are not deleted.
dialog-delete-bookmark-folder = Delete bookmark folder
dialog-delete-bookmark-folder-prompt = Delete bookmark folder “{ $name }”?
dialog-delete-bookmark-folder-note =
    { $count ->
        [one] This removes the folder and { $count } item inside it. Files on disk are not deleted.
       *[other] This removes the folder and { $count } items inside it. Files on disk are not deleted.
    }
dialog-rename-bookmark-folder = Rename bookmark folder
dialog-new-bookmark-toolbar-folder = New bookmarks toolbar folder
dialog-bookmark-library = Library
dialog-new-shortcut = New shortcut
dialog-new-remote-shortcut = New remote shortcut
dialog-shortcut-name = Shortcut name
dialog-shortcut-target = Target path
dialog-create-remote-shortcut = Create remote shortcut
dialog-cancel-new-shortcut = Cancel new shortcut
dialog-properties = { $name } - Properties
dialog-properties-aria = { $name } properties
dialog-general = General
dialog-file-type = Type: { $value }
dialog-location = Location: { $value }
dialog-size = Size: { $value }
dialog-date-created = Date created: { $value }
dialog-date-modified = Date modified: { $value }
dialog-permissions = Permissions: { $mode }
dialog-file-in-use = File in use
dialog-items-in-use = Some items are in use
dialog-lock-owner-body =
    { $count ->
        [one] Windows cannot delete the { $count } selected item because another application is using it.
       *[other] Windows cannot delete the { $count } selected items because another application is using them.
    }
dialog-apps-using-file = Applications using the file
dialog-close-results = Results of closing applications
dialog-new-bookmark = New bookmark
dialog-edit-bookmark = Edit bookmark
dialog-name-accelerator = Name (N)
dialog-location-accelerator = Location (L)
dialog-url-accelerator = URL (U)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks
dialog-show-editor-on-save = Show editor when saving (S)
dialog-lua-source = Lua source (read-only current_folder only)
dialog-folder-path-editable = Folder path (editable)
dialog-file-path-editable = File path (editable)
dialog-root = Root
dialog-folder-options-scrollbar = Folder Options vertical scroll bar
dialog-new-bookmark-default = New bookmark
dialog-new-folder-default = New folder
dialog-new-shortcut-default = New shortcut
dialog-shortcut-name-invalid = Shortcut name must be a valid name in the current folder.
dialog-shortcut-target-invalid = Enter a valid target path.
dialog-shortcut-window-closed = The main window closed, so the shortcut could not be created.
dialog-folder-changed = The current folder changed. Reopen the New shortcut window.
dialog-remote-unavailable = The remote service is currently unavailable.
dialog-shortcut-in-progress = Another shortcut is already being created.
dialog-allowed = Allowed
dialog-not-allowed = Not allowed
dialog-read = Read
dialog-write = Write
dialog-execute = Execute
dialog-owner = Owner
dialog-group = Group
dialog-others = Others
dialog-remote-folder = Remote folder
dialog-remote-file = Remote file
dialog-unavailable = Unavailable
dialog-bytes = { $value } bytes
dialog-extension-author-bio = SuperExplorer and official sample extension author
dialog-extension-purpose-folder-size = Shows file sizes and recursively totals folder size in the background.
dialog-extension-purpose-size-map = Shows how much space each item in the current folder occupies as an area map.
dialog-extension-purpose-tokei = Counts lines of code in files or folders with Rust and tokei.
dialog-extension-purpose-lua-tokei = Sample Lua extension that counts lines of code.
dialog-extension-purpose-lock-owner = Shows the program or service that currently locks a file.
dialog-extension-purpose-exif = Suggests batch renames from photo EXIF capture data.
dialog-extension-purpose-7z = Presents 7-Zip archives as browsable virtual folders.
dialog-extension-purpose-bulk-folder = Creates many folders at once from a user-specified template.
dialog-extension-folder-size = Folder size column
dialog-extension-size-map = Size Map
dialog-extension-main-code-lines = Main code lines
dialog-extension-code-lines = Code lines
dialog-extension-lock-owner = Lock owner
dialog-extension-exif = Rename from EXIF
dialog-extension-7z = 7-Zip virtual folder
dialog-extension-bulk-folder = Bulk folder generator
dialog-git-hash = Git hash
dialog-release-date-line = Release date: { $value }
dialog-reset = Reset
dialog-reset-prompt = Reset { $label }? This removes only persisted state; current files are not changed.
dialog-reset-session-label = saved windows and tabs
dialog-reset-view-label = saved view settings
dialog-reset-quick-access-label = Quick Access pins
dialog-reset-all-label = all saved Explorer state
dialog-permanent-delete-prompt =
    { $count ->
        [one] Permanently delete { $count } item? This action cannot be undone.
       *[other] Permanently delete { $count } items? This action cannot be undone.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Move { $count } item to Google Drive trash? Recover them at drive.google.com for 30 days. This is not the Windows Recycle Bin.
       *[other] Move { $count } items to Google Drive trash? Recover them at drive.google.com for 30 days. This is not the Windows Recycle Bin.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Permanently delete { $count } item
       *[other] Permanently delete { $count } items
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Move { $count } item to Google Drive trash
       *[other] Move { $count } items to Google Drive trash
    }

ftp-sign-in = Sign in to { $host }
ftp-sign-in-unencrypted = Sign in to { $host } — This connection is not encrypted

dialog-safe-mode-unfinished-body = The previous attempt to load or use this extension left an unfinished record. The app may have closed unexpectedly, or the record may not have been cleared. This does not prove the extension is faulty.

dialog-safe-mode-unverified-body = The extension loading records could not be verified, so extension loading has been paused.

dialog-safe-mode-reenable-help = Keep it disabled to continue using other features. Allowing it again clears this protection record and lets the app try loading it on the next startup.

dialog-safe-mode-show-details = Show technical details

dialog-safe-mode-hide-details = Hide technical details

dialog-safe-mode-package-id = Package ID:

dialog-safe-mode-keep-disabled = Continue

dialog-safe-mode-extension-lock-owners = File locking processes column

dialog-safe-mode-extension-code-lines = Code line count column

dialog-safe-mode-extension-folder-size = Folder size column

dialog-safe-mode-extension-size-map = Storage usage map

dialog-safe-mode-extension-unknown = Unidentified extension
