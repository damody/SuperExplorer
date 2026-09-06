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
        [one] Eliminar definitivamente { $count } item
       *[other] Eliminar definitivamente { $count } itens
    }

gdrive-trash-items =
    { $count ->
        [one] Mover { $count } item para a reciclagem do Google Drive
       *[other] Mover { $count } itens para a reciclagem do Google Drive
    }

shortcut-items =
    { $count ->
        [one] Criar atalho para { $count } item
       *[other] Criar atalho para { $count } itens
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } item extra)
       *[other] { $first } ({ $count } itens extra)
    }

clipboard-source = Origem fornecida pela área de transferência do sistema

op-new-folder = Nova pasta | { $path }

op-new-file = Novo ficheiro | { $path }

op-rename = Mudar o nome | { $from } → { $to }

op-chmod = Alterar permissões | { $path } → { $mode }

op-copy-route = Copiar { $count } itens | { $source } → { $destination }

op-move-route = Mover { $count } itens | { $source } → { $destination }

op-recycle-route = Reciclar { $count } itens | { $source }

op-permanent-delete-route = Eliminar definitivamente { $count } itens | { $source }

op-gdrive-trash-route = Reciclagem do Google Drive { $count } itens | { $source }

op-shortcut-route = Criar atalho { $count } itens | { $source }

op-preparing-copy = A preparar a cópia

op-copying = A copiar

op-copy-complete = Cópia concluída

op-preparing-move = A preparar a deslocação

op-moving = A mover

op-move-complete = Deslocação concluída

op-preparing = A preparar

op-processing = A processar

op-complete = Concluído

op-finalizing = A finalizar

op-progress-items = { $summary } | { $phase } | Progresso { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Progresso { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Progresso { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Concluído

op-cancelled = { $summary } | Cancelado

op-failed = { $summary } | Falha: { $error }

op-partial = { $summary } | Parcial: { $succeeded }/{ $total } com êxito

op-cancelling = { $summary } | A cancelar

op-success-route = Êxito | { $route }

op-skipped-route = Ignorado | { $route }

op-cancelled-route = Cancelado | { $route }

op-partial-status = Parcial

op-failed-status = Falha

op-error-code =  | Código de erro { $code }

op-no-destination = Destino não fornecido

op-no-source = Origem não fornecida

apk-installing = A instalar: { $name } → { $target }

apk-installed = Instalado: { $name } → { $target }

apk-cancelled = Instalação de { $name } em { $target } cancelada

apk-timeout = A instalação de { $name } em { $target } expirou

apk-failed = Falha ao instalar { $name } em { $target }: { $error }

status-no-selection = Nenhum item selecionado

status-details-unavailable = Não foi possível carregar os detalhes

status-item-count =
    { $count ->
        [one] { $count } item
       *[other] { $count } itens
    }

status-preview-select-one = Selecione um item para pré-visualizar

status-preview-failed = Não foi possível gerar uma pré-visualização deste ficheiro

status-preview-loading = A carregar pré-visualização…

status-preview-item-failed = Não foi possível carregar o item de pré-visualização

status-preview-select-single = Selecione um único item para pré-visualizar

status-no-transfers = Nenhuma operação de ficheiro nesta sessão

status-thumbnail-cleared = Cache de miniaturas limpa

status-thumbnail-clear-partial = Não foi possível limpar totalmente a cache de miniaturas; pode tentar novamente e a navegação continua a funcionar

status-invalid-folder-name = Nome de pasta inválido. Corrija e tente novamente.

status-name-conflict = Já existe um item com esse nome.

status-context-menu-unresponsive = O menu de contexto não respondeu. Pode continuar a trabalhar.

status-cannot-go-forward = Não é possível avançar

status-partial-folders = Não foi possível listar algumas pastas.

status-cannot-list-folder = Não foi possível listar a pasta.

status-cannot-cancel-disconnected = Não é possível cancelar: o serviço de ficheiros não está ligado.

status-cannot-cancel-operation = Não foi possível cancelar a operação de ficheiro: { $error }

status-cannot-load-folder = Não foi possível carregar a pasta. Tente novamente.

status-drive-free-of = { $free } livres de { $total }

status-no-media = Sem suporte

status-disconnected = Desligado

status-access-denied = Acesso recusado

status-capacity-unavailable = Capacidade indisponível

status-waiting-file-count = A aguardar File Count…

status-file-count-limit = Depende do File Count, por isso não foi iniciado

status-file-count-over-limit = File Count excedeu o limite, por isso não foi iniciado

status-file-count-pending-label = Limit

transfer-upload = Carregamento no destino

transfer-download = Transferência da origem

transfer-cancelling = A cancelar

transfer-cancel = Cancelar

status-details-view = Vista de detalhes
status-icon-view = Vista de ícones
status-error = Erro · 
status-loading = A carregar · 
status-items-selected = { $count } itens — { $selected } selecionados
status-operation-progress = Operação { $completed }/{ $total } · { $name }
status-search-cancelled = Pesquisa cancelada · 
status-search-error = Erro de pesquisa · 
status-search-fallback = A pesquisar (sistema de ficheiros; índice indisponível) · 
status-search-indexed = A pesquisar (índice + recurso) · 
status-search-partial = Resultados parciais da pesquisa · 
status-search-results = Resultados da pesquisa · 
status-lock-close-cancelled = O encerramento das aplicações foi cancelado.
status-lock-discovery-timeout = A descoberta do proprietário do bloqueio atingiu o prazo.
status-lock-finding = A localizar aplicações que estão a usar o item selecionado…
status-lock-owners-found = { $count } aplicação/ões está a usar o item selecionado.
status-lock-partial-close = Algumas aplicações não fecharam. Nenhum processo foi forçado a terminar.
status-lock-retry-limit = Foi atingido o limite de novas tentativas. O item não foi eliminado.
status-lock-retrying = A repetir a operação de eliminação…
status-lock-unidentified = O Windows não conseguiu identificar a aplicação que está a usar este item.
transfer-cancel-operation = Cancelar operação de ficheiro
transfer-open-location = Abrir localização da transferência

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
