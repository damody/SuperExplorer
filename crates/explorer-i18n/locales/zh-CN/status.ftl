# SuperExplorer status, transfer, preview, and operation messages.

copy-items = 复制 { $count } 个项目

move-items = 移动 { $count } 个项目

recycle-items = 移至回收站 { $count } 个项目

permanent-delete-items = 永久删除 { $count } 个项目

gdrive-trash-items = 移到 Google Drive 回收站 { $count } 个项目

shortcut-items = 创建快捷方式 { $count } 个项目

extra-items = { $first }（另有 { $count } 个项目）

clipboard-source = 源由系统剪贴板提供

op-new-folder = 新建文件夹｜{ $path }

op-new-file = 新建文件｜{ $path }

op-rename = 重命名｜{ $from } → { $to }

op-chmod = 更改权限｜{ $path } → { $mode }

op-copy-route = 复制 { $count } 个项目｜{ $source } → { $destination }

op-move-route = 移动 { $count } 个项目｜{ $source } → { $destination }

op-recycle-route = 移至回收站 { $count } 个项目｜{ $source }

op-permanent-delete-route = 永久删除 { $count } 个项目｜{ $source }

op-gdrive-trash-route = 移到 Google Drive 回收站 { $count } 个项目｜{ $source }

op-shortcut-route = 创建快捷方式 { $count } 个项目｜{ $source }

op-preparing-copy = 准备复制

op-copying = 正在复制

op-copy-complete = 复制完成

op-preparing-move = 准备移动

op-moving = 正在移动

op-move-complete = 移动完成

op-preparing = 准备中

op-processing = 处理中

op-complete = 完成

op-finalizing = 完成处理中

op-progress-items = { $summary }｜{ $phase }｜进度 { $completed }/{ $total } 项目

op-progress-bytes = { $summary }｜{ $phase } { $percent }%（{ $bytes } / { $total-bytes }）{ $speed }｜进度 { $completed }/{ $total } 项目

op-progress-unknown = { $summary }｜{ $phase } { $bytes }{ $speed }｜进度 { $completed }/{ $total } 项目

op-finished = { $phase }｜{ $summary }

op-done = { $summary }｜完成

op-cancelled = { $summary }｜已取消

op-failed = { $summary }｜失败：{ $error }

op-partial = { $summary }｜部分完成：{ $succeeded }/{ $total } 成功

op-cancelling = { $summary }｜正在取消

op-success-route = 成功｜{ $route }

op-skipped-route = 跳过｜{ $route }

op-cancelled-route = 已取消｜{ $route }

op-partial-status = 部分完成

op-failed-status = 失败

op-error-code = ｜错误代码 { $code }

op-no-destination = 未提供目标位置

op-no-source = 未提供源

apk-installing = 正在安装：{ $name } → { $target }

apk-installed = 安装完成：{ $name } → { $target }

apk-cancelled = 已取消安装 { $name } 到 { $target }

apk-timeout = 安装 { $name } 到 { $target } 超时

apk-failed = 安装 { $name } 到 { $target } 失败：{ $error }

status-no-selection = 未选择任何项目

status-details-unavailable = 無法加载详细信息

status-item-count = { $count } 个项目

status-preview-select-one = 选择一个项目以预览

status-preview-failed = 无法生成此文件的预览

status-preview-loading = 正在加载预览…

status-preview-item-failed = 无法加载预览项目

status-preview-select-single = 选择单个项目以预览

status-no-transfers = 本次运行期间尚无文件操作

status-thumbnail-cleared = 缩略图缓存已清除

status-thumbnail-clear-partial = 无法完全清除缩略图缓存；可重试，文件浏览仍可使用

status-invalid-folder-name = 文件夹名称无效，请修正后再试一次。

status-name-conflict = 此位置已有同名项目。

status-context-menu-unresponsive = 上下文菜单未响应，仍可继续操作。

status-cannot-go-forward = 无法前进

status-partial-folders = 部分文件夹无法列出。

status-cannot-list-folder = 无法列出文件夹。

status-cannot-cancel-disconnected = 无法取消：文件服务尚未连接。

status-cannot-cancel-operation = 无法取消文件操作：{ $error }

status-cannot-load-folder = 无法加载文件夹，请再试一次。

status-drive-free-of = 剩余 { $free }，共 { $total }

status-no-media = 没有媒体

status-disconnected = 已断开连接

status-access-denied = 拒绝访问

status-capacity-unavailable = 无法获取容量

status-waiting-file-count = 等待 File Count…

status-file-count-limit = 依赖 File Count，因此未启动

status-file-count-over-limit = File Count 超过限制，因此未启动

status-file-count-pending-label = Limit

transfer-upload = 目标上传

transfer-download = 源下载

transfer-cancelling = 正在取消

transfer-cancel = Cancel

status-details-view = 详细信息视图
status-icon-view = 图标视图
status-error = 错误 · 
status-loading = 正在加载 · 
status-items-selected = { $count } 个项目 — 已选择 { $selected } 个
status-operation-progress = 操作 { $completed }/{ $total } · { $name }
status-search-cancelled = 搜索已取消 · 
status-search-error = 搜索出错 · 
status-search-fallback = 正在搜索（文件系统回退；索引不可用） · 
status-search-indexed = 正在搜索（索引 + 回退） · 
status-search-partial = 部分搜索结果 · 
status-search-results = 搜索结果 · 
status-lock-close-cancelled = 已取消关闭应用程序。
status-lock-discovery-timeout = 查找锁定占用程序已超时。
status-lock-finding = 正在查找使用所选项目的应用程序…
status-lock-owners-found =
    { $count ->
        [one] { $count } 个应用程序正在使用所选项目。
        [few] { $count } 个应用程序正在使用所选项目。
        [many] { $count } 个应用程序正在使用所选项目。
       *[other] { $count } 个应用程序正在使用所选项目。
    }
status-lock-partial-close = 部分应用程序未能关闭。未强制终止任何进程。
status-lock-retry-limit = 已达到重试上限。未删除该项目。
status-lock-retrying = 正在重试删除操作…
status-lock-unidentified = Windows 无法识别正在使用此项目的应用程序。
transfer-cancel-operation = 取消文件操作
transfer-open-location = 打开传输位置

status-generic-item = 项目
