# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = フォルダー オプション

dialog-about = SuperExplorer について

dialog-version = バージョン

dialog-build-date = ビルド日

dialog-author = 作成者

dialog-purpose = 用途: { $value }

dialog-author-line = 作成者: { $name } — { $bio } · { $date }

dialog-community = コミュニティ: { $url }

dialog-plugin-safe-mode-title = プラグイン セーフ モード

dialog-plugin-safe-mode-body = プラグイン セーフ モードがオンです。再有効化するプラグインを選択し、適用または OK を選んでから SuperExplorer を再起動してください。

dialog-safe-mode-confirm-title = セーフ モードには確認が必要です

dialog-safe-mode-confirm-aria = セーフ モードの確認が必要です。疑わしいパッケージ: { $package }

dialog-suspect-package = 疑わしいパッケージ: { $package }

dialog-interface = インターフェイス: { $value }

dialog-operation = 操作: { $value }

dialog-confirm-reenable = 確認して再有効化

dialog-bookmark-action = ブックマーク操作

dialog-delete-bookmark = ブックマークの削除

dialog-delete-bookmark-prompt = ブックマーク「{ $name }」を削除しますか?

dialog-delete-bookmark-note = ブックマークは削除されます。ディスク上のファイルは削除されません。

dialog-delete-bookmark-folder = ブックマーク フォルダーの削除

dialog-delete-bookmark-folder-prompt = ブックマーク フォルダー「{ $name }」を削除しますか?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] フォルダーとその中の { $count } 個の項目が削除されます。ディスク上のファイルは削除されません。
       *[other] フォルダーとその中の { $count } 個の項目が削除されます。ディスク上のファイルは削除されません。
    }

dialog-rename-bookmark-folder = ブックマーク フォルダー名の変更

dialog-bookmark-library = ライブラリ

dialog-new-shortcut = 新しいショートカット

dialog-new-remote-shortcut = 新しいリモート ショートカット

dialog-shortcut-name = ショートカット名

dialog-shortcut-target = ターゲット パス

dialog-create-remote-shortcut = リモート ショートカットの作成

dialog-cancel-new-shortcut = 新しいショートカットをキャンセル

dialog-properties = { $name } - プロパティ

dialog-properties-aria = { $name } のプロパティ

dialog-general = 全般

dialog-file-type = 種類: { $value }

dialog-location = 場所: { $value }

dialog-size = サイズ: { $value }

dialog-date-created = 作成日時: { $value }

dialog-date-modified = 更新日時: { $value }

dialog-permissions = アクセス許可: { $mode }

dialog-file-in-use = ファイルは使用中です

dialog-items-in-use = 一部の項目は使用中です

dialog-lock-owner-body =
    { $count ->
        [one] 別のアプリケーションが使用しているため、Windows は選択した { $count } 個の項目を削除できません。
       *[other] 別のアプリケーションが使用しているため、Windows は選択した { $count } 個の項目を削除できません。
    }

dialog-apps-using-file = ファイルを使用しているアプリケーション

dialog-close-results = アプリケーションを閉じた結果

dialog-new-bookmark = 新しいブックマーク

dialog-edit-bookmark = ブックマークの編集

dialog-name-accelerator = 名前 (N)

dialog-location-accelerator = 場所 (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = 保存時にエディターを表示 (S)

dialog-lua-source = Lua ソース (読み取り専用 current_folder のみ)

dialog-folder-path-editable = フォルダー パス (編集可能)

dialog-file-path-editable = ファイル パス (編集可能)

dialog-root = ルート

dialog-folder-options-scrollbar = フォルダー オプションの垂直スクロール バー

dialog-new-bookmark-default = 新しいブックマーク

dialog-new-folder-default = 新しいフォルダー

dialog-new-shortcut-default = 新しいショートカット

dialog-shortcut-name-invalid = ショートカット名は現在のフォルダー内の有効な名前である必要があります。

dialog-shortcut-target-invalid = 有効なターゲット パスを入力してください。

dialog-shortcut-window-closed = メイン ウィンドウが閉じられたため、ショートカットを作成できませんでした。

dialog-folder-changed = 現在のフォルダーが変更されました。新しいショートカット ウィンドウを再度開いてください。

dialog-remote-unavailable = リモート サービスは現在利用できません。

dialog-shortcut-in-progress = 別のショートカットが既に作成されています。

dialog-allowed = 許可

dialog-not-allowed = 許可されていません

dialog-read = 読み取り

dialog-write = 書き込み

dialog-execute = 実行

dialog-owner = 所有者

dialog-group = グループ

dialog-others = その他

dialog-remote-folder = リモート フォルダー

dialog-remote-file = リモート ファイル

dialog-unavailable = 取得できません

dialog-bytes = { $value } バイト

dialog-extension-author-bio = SuperExplorer および公式サンプル拡張機能の作成者

dialog-extension-purpose-folder-size = ファイル サイズを表示し、バックグラウンドでフォルダー サイズを再帰的に集計します。

dialog-extension-purpose-size-map = 現在のフォルダー内の各項目が占める領域を面積マップとして表示します。

dialog-extension-purpose-tokei = Rust と tokei を使用してファイルまたはフォルダーのコード行数をカウントします。

dialog-extension-purpose-lua-tokei = コード行数をカウントする Lua 拡張機能のサンプルです。

dialog-extension-purpose-lock-owner = 現在ファイルをロックしているプログラムまたはサービスを表示します。

dialog-extension-purpose-exif = 写真の EXIF 撮影データから一括名前変更を提案します。

dialog-extension-purpose-7z = 7-Zip アーカイブを参照可能な仮想フォルダーとして表示します。

dialog-extension-purpose-bulk-folder = ユーザー指定のテンプレートから一度に多数のフォルダーを作成します。

dialog-extension-folder-size = フォルダー サイズ列

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = メイン コード行

dialog-extension-code-lines = コード行

dialog-extension-lock-owner = ロック所有者

dialog-extension-exif = EXIF から名前を変更

dialog-extension-7z = 7-Zip 仮想フォルダー

dialog-extension-bulk-folder = 一括フォルダー作成

dialog-git-hash = Git ハッシュ
dialog-release-date-line = リリース日: { $value }
dialog-reset = リセット
dialog-reset-prompt = { $label } をリセットしますか? 保存された状態のみ削除され、現在のファイルは変更されません。
dialog-reset-session-label = 保存されたウィンドウとタブ
dialog-reset-view-label = 保存されたビュー設定
dialog-reset-quick-access-label = クイック アクセスのピン留め
dialog-reset-all-label = 保存されたすべての Explorer の状態
dialog-permanent-delete-prompt =
    { $count ->
        [one] { $count } 個の項目を完全に削除しますか? この操作は元に戻せません。
       *[other] { $count } 個の項目を完全に削除しますか? この操作は元に戻せません。
    }
dialog-gdrive-trash-prompt = Google ドライブのゴミ箱に { $count } 個の項目を移動しますか? drive.google.com で 30 日間復元できます。これは Windows のごみ箱ではありません。
dialog-permanent-delete-aria = { $count } 個の項目を完全に削除
dialog-gdrive-trash-aria = Google ドライブのゴミ箱に { $count } 個の項目を移動

ftp-sign-in = { $host } にサインイン
ftp-sign-in-unencrypted = { $host } にサインイン — この接続は暗号化されていません
