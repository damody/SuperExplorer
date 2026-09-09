# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Copiar { $count } item
       *[other] Copiar { $count } itens
    }

move-items =
    { $count ->
        [one] Mover { $count } item
       *[other] Mover { $count } itens
    }

recycle-items =
    { $count ->
        [one] Reciclar { $count } item
       *[other] Reciclar { $count } itens
    }

permanent-delete-items =
    { $count ->
        [one] Excluir permanentemente { $count } item
       *[other] Excluir permanentemente { $count } itens
    }

gdrive-trash-items =
    { $count ->
        [one] Mover { $count } item para a lixeira do Google Drive
       *[other] Mover { $count } itens para a lixeira do Google Drive
    }

shortcut-items =
    { $count ->
        [one] Criar atalho para { $count } item
       *[other] Criar atalho para { $count } itens
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } item a mais)
       *[other] { $first } ({ $count } itens a mais)
    }

clipboard-source = Origem fornecida pela área de transferência do sistema

op-new-folder = Nova pasta | { $path }

op-new-file = Novo arquivo | { $path }

op-rename = Renomear | { $from } → { $to }

op-chmod = Alterar permissões | { $path } → { $mode }

op-copy-route = { $action } | { $source } → { $destination }

op-move-route = { $action } | { $source } → { $destination }

op-recycle-route = { $action } | { $source }

op-permanent-delete-route = { $action } | { $source }

op-gdrive-trash-route = { $action } | { $source }

op-shortcut-route = { $action } | { $source }

op-preparing-copy = Preparando a cópia

op-copying = Copiando

op-copy-complete = Cópia concluída

op-preparing-move = Preparando a movimentação

op-moving = Movendo

op-move-complete = Movimentação concluída

op-preparing = Preparando

op-processing = Em andamento

op-complete = Concluído

op-finalizing = Finalizando

op-progress-items = { $summary } | { $phase } | Progresso { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Progresso { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Progresso { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Concluído

op-cancelled = { $summary } | Cancelado

op-failed = { $summary } | Falha: { $error }

op-partial = { $summary } | Parcial: { $succeeded }/{ $total } êxito

op-cancelling = { $summary } | Cancelando

op-success-route = Êxito | { $route }

op-skipped-route = Ignorado | { $route }

op-cancelled-route = Cancelado | { $route }

op-partial-status = Parcial

op-failed-status = Falha

op-error-code =  | Código de erro { $code }

op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = Destino não fornecido

op-no-source = Origem não fornecida

apk-installing = Instalando: { $name } → { $target }

apk-installed = Instalado: { $name } → { $target }

apk-cancelled = Instalação de { $name } em { $target } cancelada

apk-timeout = A instalação de { $name } em { $target } expirou

apk-failed = Falha ao instalar { $name } em { $target }: { $error }

apk-check-device = Verifique a conexão do dispositivo e o APK e tente novamente
status-no-selection = Nenhum item selecionado

status-details-unavailable = Não foi possível carregar os detalhes

status-item-count =
    { $count ->
        [one] { $count } item
       *[other] { $count } itens
    }

status-preview-select-one = Selecione um item para visualizar

status-preview-failed = Não foi possível gerar uma visualização deste arquivo

status-preview-loading = Carregando visualização…

status-preview-item-failed = Não foi possível carregar o item de visualização

status-preview-select-single = Selecione um único item para visualizar

status-no-transfers = Nenhuma operação de arquivo nesta sessão

status-thumbnail-cleared = Cache de miniaturas limpo

status-thumbnail-clear-partial = Não foi possível limpar totalmente o cache de miniaturas; você pode tentar de novo e a navegação ainda funciona

status-invalid-folder-name = Nome de pasta inválido. Corrija e tente novamente.

status-name-conflict = Já existe um item com esse nome.

status-context-menu-unresponsive = O menu de contexto não respondeu. Você pode continuar trabalhando.

status-cannot-go-forward = Não é possível avançar

status-partial-folders = Algumas pastas não puderam ser listadas.

status-cannot-list-folder = Não foi possível listar a pasta.

status-cannot-cancel-disconnected = Não é possível cancelar: o serviço de arquivos não está conectado.

status-cannot-cancel-operation = Não foi possível cancelar a operação de arquivo: { $error }

status-cannot-load-folder = Não foi possível carregar a pasta. Tente novamente.

status-drive-free-of = { $free } livres de { $total }

status-no-media = Nenhuma mídia

status-disconnected = Desconectado

status-access-denied = Acesso negado

status-capacity-unavailable = Capacidade indisponível

status-waiting-file-count = Aguardando File Count…

status-file-count-limit = Depende de File Count, então não foi iniciado

status-file-count-over-limit = File Count excedeu o limite, então não foi iniciado

status-file-count-pending-label = Limit

transfer-upload = Upload no destino

transfer-download = Download da origem

transfer-conflict-inspection = Verificação de conflito de destino
transfer-local-copy = Cópia local
transfer-source-delete = Excluir origem após mover
transfer-provider-panic = Erro do provedor de transferência
transfer-no-diagnostic = Nenhum erro subjacente foi fornecido
transfer-cancelling = Cancelando

transfer-cancel = Cancelar

status-details-view = Exibição de detalhes
status-icon-view = Exibição de ícones
status-error = Erro · 
status-loading = Carregando · 
status-items-selected = { $count } itens — { $selected } selecionados
status-operation-progress = Operação { $completed }/{ $total } · { $name }
status-search-cancelled = Pesquisa cancelada · 
status-search-error = Erro de pesquisa · 
status-search-fallback = Pesquisando (sistema de arquivos; índice indisponível) · 
status-search-indexed = Pesquisando (índice + fallback) · 
status-search-partial = Resultados parciais da pesquisa · 
status-search-results = Resultados da pesquisa · 
status-lock-close-cancelled = O fechamento dos aplicativos foi cancelado.
status-lock-discovery-timeout = A descoberta do proprietário do bloqueio atingiu o prazo.
status-lock-finding = Localizando aplicativos que estão usando o item selecionado…
status-lock-owners-found = { $count } aplicativo(s) estão usando o item selecionado.
status-lock-partial-close = Alguns aplicativos não fecharam. Nenhum processo foi forçado a encerrar.
status-lock-retry-limit = O limite de novas tentativas foi atingido. O item não foi excluído.
status-lock-retrying = Tentando a exclusão novamente…
status-lock-unidentified = O Windows não conseguiu identificar o aplicativo que está usando este item.
transfer-cancel-operation = Cancelar operação de arquivo
transfer-open-location = Abrir local da transferência

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
status-run-log-missing = O arquivo não existe mais.
status-run-log-opened = Aberto.
status-run-log-open-failed = Não foi possível abrir: { $error }
status-lua-bookmark-need-folder = Lua bookmarks require a filesystem folder.
status-lua-bookmark-completed = Lua bookmark completed.
status-lua-bookmark-timed-out = Lua bookmark timed out.
status-lua-bookmark-failed = Lua bookmark failed: { $error }
status-bookmark-html-exported = Bookmarks exported as HTML.
status-bookmark-html-imported = Bookmarks imported from HTML.
status-bookmark-backup-saved = Bookmark backup saved.
status-bookmark-backup-restored = Bookmark backup restored.
status-bookmark-browser-imported = Bookmarks imported from another browser.
status-bookmark-separator-added = Separator added.
status-bookmark-undone = Bookmark change undone.
status-bookmark-redone = Bookmark change redone.
status-bookmark-nothing-to-undo = Nothing to undo.
status-bookmark-nothing-to-redo = Nothing to redo.
