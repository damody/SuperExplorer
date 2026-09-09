# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = 文件夹选项

dialog-about = 关于 SuperExplorer

dialog-version = 版本

dialog-build-date = 编译日期

dialog-author = 作者

dialog-purpose = 用途：{ $value }

dialog-author-line = 作者：{ $name } — { $bio } · { $date }

dialog-community = 社区：{ $url }

dialog-plugin-safe-mode-title = Plugin Safe Mode

dialog-plugin-safe-mode-body = Plugin Safe Mode 已启用。勾选要重新启用的 Plugin，按 Apply 或 OK，然后重新启动 SuperExplorer。

dialog-safe-mode-confirm-title = Safe Mode requires confirmation

dialog-safe-mode-confirm-aria = Safe Mode confirmation required; Suspect package: { $package }

dialog-suspect-package = Suspect package: { $package }

dialog-interface = Interface: { $value }

dialog-operation = Operation: { $value }

dialog-confirm-reenable = Confirm and re-enable

dialog-bookmark-action = 书签操作

dialog-delete-bookmark = 删除书签

dialog-delete-bookmark-prompt = 删除书签「{ $name }」？

dialog-delete-bookmark-note = 这会移除书签，不会删除磁盘上的文件。

dialog-delete-bookmark-folder = 删除书签文件夹

dialog-delete-bookmark-folder-prompt = 删除书签文件夹「{ $name }」？

dialog-delete-bookmark-folder-note = 这会移除文件夹以及其中 { $count } 个项目，不会删除磁盘上的文件。

dialog-rename-bookmark-folder = 重命名书签文件夹

dialog-bookmark-library = 收藏库

dialog-new-shortcut = 新建快捷方式

dialog-new-remote-shortcut = 新建远程快捷方式

dialog-shortcut-name = 快捷方式名称

dialog-shortcut-target = 目标路径

dialog-create-remote-shortcut = 创建远程快捷方式

dialog-cancel-new-shortcut = 取消新建快捷方式

dialog-properties = { $name } - 属性

dialog-properties-aria = { $name } 属性

dialog-general = 常规

dialog-file-type = 文件类型：{ $value }

dialog-location = 位置：{ $value }

dialog-size = 大小：{ $value }

dialog-date-created = 创建日期：{ $value }

dialog-date-modified = 修改日期：{ $value }

dialog-permissions = 权限：{ $mode }

dialog-file-in-use = 文件正在使用中

dialog-items-in-use = 部分项目正在使用中

dialog-lock-owner-body = Windows 无法删除选定的 { $count } 个项目，因为其他应用程序正在使用它。

dialog-apps-using-file = 正在使用文件的应用进程

dialog-close-results = 关闭应用进程的结果

dialog-new-bookmark = 新建书签

dialog-edit-bookmark = 编辑书签

dialog-name-accelerator = 名称 (N)

dialog-location-accelerator = 位置 (L)

dialog-url-accelerator = 网址 (L)
dialog-path-accelerator = 路径 (U)
dialog-tags-accelerator = 标签 (T)
dialog-tags-placeholder = 用逗号分隔每个标签
dialog-tags-hint = 使用标签搜索和整理书签

dialog-show-editor-on-save = 保存时显示编辑器 (S)

dialog-lua-source = Lua 源代码（仅可使用只读 current_folder）

dialog-folder-path-editable = 文件夹路径（可编辑）

dialog-file-path-editable = 文件路径（可编辑）

dialog-root = 根目录

dialog-folder-options-scrollbar = 文件夹选项垂直滚动条

dialog-new-bookmark-default = 新书签

dialog-new-folder-default = 新文件夹

dialog-new-shortcut-default = 新快捷方式

dialog-shortcut-name-invalid = 快捷方式名称必须是当前文件夹内的一个有效名称。

dialog-shortcut-target-invalid = 请输入有效的目标路径。

dialog-shortcut-window-closed = 主窗口已关闭，无法创建快捷方式。

dialog-folder-changed = 当前文件夹已更改，请重新打开新建快捷方式窗口。

dialog-remote-unavailable = 远程服务目前不可用。

dialog-shortcut-in-progress = 另一个快捷方式正在创建中。

dialog-allowed = 已允许

dialog-not-allowed = 未允许

dialog-read = 读取

dialog-write = 写入

dialog-execute = 执行

dialog-owner = 所有者

dialog-group = 组

dialog-others = 其他

dialog-remote-folder = 远程文件夹

dialog-remote-file = 远程文件

dialog-unavailable = 无法获取

dialog-bytes = { $value } 字节

dialog-extension-author-bio = SuperExplorer 与官方范例扩展作者

dialog-extension-purpose-folder-size = 显示文件大小并在后台递归计算文件夹总大小。

dialog-extension-purpose-size-map = 以面积图呈现当前文件夹内各项目的空间占用。

dialog-extension-purpose-tokei = 使用 Rust 与 tokei 统计文件或文件夹中的代码行数。

dialog-extension-purpose-lua-tokei = 示范以 Lua 扩展统计代码行数。

dialog-extension-purpose-lock-owner = 显示当前锁定文件的进程或服务所有者。

dialog-extension-purpose-exif = 按照片 EXIF 拍摄信息批量生成重命名建议。

dialog-extension-purpose-7z = 将 7-Zip 压缩文件以可浏览的虚拟文件夹呈现。

dialog-extension-purpose-bulk-folder = 按用户指定的模板一次创建多个文件夹。

dialog-extension-folder-size = Folder size column

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Main code lines

dialog-extension-code-lines = Code lines

dialog-extension-lock-owner = Lock owner

dialog-extension-exif = Rename from EXIF

dialog-extension-7z = 7-Zip virtual folder

dialog-extension-bulk-folder = Bulk folder generator

dialog-git-hash = Git 哈希
dialog-release-date-line = 发布日期：{ $value }
dialog-reset = 重置
dialog-reset-prompt = 重置 { $label }？这只会删除已保存的状态，不会更改当前文件。
dialog-reset-session-label = 已保存的窗口和标签页
dialog-reset-view-label = 已保存的视图设置
dialog-reset-quick-access-label = 快速访问固定项
dialog-reset-all-label = 所有已保存的 Explorer 状态
dialog-permanent-delete-prompt = 要永久删除 { $count } 个项目吗？此操作无法撤销。
dialog-gdrive-trash-prompt = 要将 { $count } 个项目移到 Google Drive 回收站吗？可在 drive.google.com 恢复 30 天。这不是 Windows 回收站。
dialog-permanent-delete-aria = 永久删除 { $count } 个项目
dialog-gdrive-trash-aria = 将 { $count } 个项目移到 Google Drive 回收站

ftp-sign-in = 登录 { $host }
ftp-sign-in-unencrypted = 登录 { $host } — 此连接未加密
