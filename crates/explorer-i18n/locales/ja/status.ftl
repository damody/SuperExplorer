# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] { $count } 個の項目をコピー
       *[other] { $count } 個の項目をコピー
    }

move-items =
    { $count ->
        [one] { $count } 個の項目を移動
       *[other] { $count } 個の項目を移動
    }

recycle-items =
    { $count ->
        [one] { $count } 個の項目をごみ箱に移動
       *[other] { $count } 個の項目をごみ箱に移動
    }

permanent-delete-items =
    { $count ->
        [one] { $count } 個の項目を完全に削除
       *[other] { $count } 個の項目を完全に削除
    }

gdrive-trash-items =
    { $count ->
        [one] { $count } 個の項目を Google ドライブのゴミ箱に移動
       *[other] { $count } 個の項目を Google ドライブのゴミ箱に移動
    }

shortcut-items =
    { $count ->
        [one] { $count } 個の項目のショートカットを作成
       *[other] { $count } 個の項目のショートカットを作成
    }

extra-items =
    { $count ->
        [one] { $first } (他 { $count } 件)
       *[other] { $first } (他 { $count } 件)
    }

clipboard-source = ソースはシステムのクリップボードから提供されます

op-new-folder = 新しいフォルダー | { $path }

op-new-file = 新しいファイル | { $path }

op-rename = 名前の変更 | { $from } → { $to }

op-chmod = アクセス許可の変更 | { $path } → { $mode }

op-copy-route = { $count } 個の項目をコピー | { $source } → { $destination }

op-move-route = { $count } 個の項目を移動 | { $source } → { $destination }

op-recycle-route = ごみ箱 { $count } 個 | { $source }

op-permanent-delete-route = { $count } 個を完全に削除 | { $source }

op-gdrive-trash-route = Google ドライブのゴミ箱 { $count } 個 | { $source }

op-shortcut-route = ショートカット { $count } 個 | { $source }

op-preparing-copy = コピーの準備中

op-copying = コピー中

op-copy-complete = コピー完了

op-preparing-move = 移動の準備中

op-moving = 移動中

op-move-complete = 移動完了

op-preparing = 準備中

op-processing = 処理中

op-complete = 完了

op-finalizing = 完了処理中

op-progress-items = { $summary } | { $phase } | 進行状況 { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | 進行状況 { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | 進行状況 { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | 完了

op-cancelled = { $summary } | キャンセル済み

op-failed = { $summary } | 失敗: { $error }

op-partial = { $summary } | 一部完了: { $succeeded }/{ $total } 成功

op-cancelling = { $summary } | キャンセル中

op-success-route = 成功 | { $route }

op-skipped-route = スキップ | { $route }

op-cancelled-route = キャンセル済み | { $route }

op-partial-status = 一部完了

op-failed-status = 失敗

op-error-code =  | エラー コード { $code }

op-no-destination = 宛先が指定されていません

op-no-source = ソースが指定されていません

apk-installing = インストール中: { $name } → { $target }

apk-installed = インストール完了: { $name } → { $target }

apk-cancelled = { $name } の { $target } へのインストールをキャンセルしました

apk-timeout = { $name } の { $target } へのインストールがタイムアウトしました

apk-failed = { $name } の { $target } へのインストールに失敗しました: { $error }

status-no-selection = 項目が選択されていません

status-details-unavailable = 詳細を読み込めませんでした

status-item-count =
    { $count ->
        [one] { $count } 個の項目
       *[other] { $count } 個の項目
    }

status-preview-select-one = プレビューする項目を選択してください

status-preview-failed = このファイルのプレビューを生成できませんでした

status-preview-loading = プレビューを読み込んでいます…

status-preview-item-failed = プレビュー項目を読み込めませんでした

status-preview-select-single = プレビューするには 1 つの項目を選択してください

status-no-transfers = このセッション中のファイル操作はありません

status-thumbnail-cleared = 縮小版キャッシュをクリアしました

status-thumbnail-clear-partial = 縮小版キャッシュを完全にクリアできませんでした。再試行できます。参照は引き続き利用できます

status-invalid-folder-name = フォルダー名が無効です。修正してもう一度お試しください。

status-name-conflict = 同じ名前の項目が既に存在します。

status-context-menu-unresponsive = コンテキスト メニューが応答しませんでした。作業を続行できます。

status-cannot-go-forward = 進むことができません

status-partial-folders = 一部のフォルダーを列挙できませんでした。

status-cannot-list-folder = フォルダーを列挙できませんでした。

status-cannot-cancel-disconnected = キャンセルできません: ファイル サービスが接続されていません。

status-cannot-cancel-operation = ファイル操作をキャンセルできませんでした: { $error }

status-cannot-load-folder = フォルダーを読み込めませんでした。もう一度お試しください。

status-drive-free-of = { $total } 中 { $free } 空き

status-no-media = メディアがありません

status-disconnected = 切断されました

status-access-denied = アクセスが拒否されました

status-capacity-unavailable = 容量を取得できません

status-waiting-file-count = File Count を待機しています…

status-file-count-limit = File Count に依存するため開始されませんでした

status-file-count-over-limit = File Count が制限を超えたため開始されませんでした

status-file-count-pending-label = Limit

transfer-upload = 宛先へのアップロード

transfer-download = ソースからのダウンロード

transfer-cancelling = キャンセル中

transfer-cancel = キャンセル

status-details-view = 詳細ビュー
status-icon-view = アイコン ビュー
status-error = エラー · 
status-loading = 読み込み中 · 
status-items-selected = { $count } 個の項目 — { $selected } 個選択
status-operation-progress = 操作 { $completed }/{ $total } · { $name }
status-search-cancelled = 検索がキャンセルされました · 
status-search-error = 検索エラー · 
status-search-fallback = 検索中 (ファイルシステム フォールバック、インデックスなし) · 
status-search-indexed = 検索中 (インデックス + フォールバック) · 
status-search-partial = 一部の検索結果 · 
status-search-results = 検索結果 · 
status-lock-close-cancelled = アプリケーションの終了はキャンセルされました。
status-lock-discovery-timeout = ロック所有者の検出が期限に達しました。
status-lock-finding = 選択した項目を使用しているアプリケーションを検索しています…
status-lock-owners-found = { $count } 個のアプリケーションが選択した項目を使用しています。
status-lock-partial-close = 一部のアプリケーションは終了しませんでした。プロセスは強制終了されていません。
status-lock-retry-limit = 再試行の上限に達しました。項目は削除されませんでした。
status-lock-retrying = 削除操作を再試行しています…
status-lock-unidentified = Windows はこの項目を使用しているアプリケーションを識別できませんでした。
transfer-cancel-operation = ファイル操作をキャンセル
transfer-open-location = 転送場所を開く
