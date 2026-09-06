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

op-copy-route = Copiar { $count } itens | { $source } → { $destination }

op-move-route = Mover { $count } itens | { $source } → { $destination }

op-recycle-route = Reciclar { $count } itens | { $source }

op-permanent-delete-route = Excluir permanentemente { $count } itens | { $source }

op-gdrive-trash-route = Lixeira do Google Drive { $count } itens | { $source }

op-shortcut-route = Criar atalho { $count } itens | { $source }

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

op-no-destination = Destino não fornecido

op-no-source = Origem não fornecida

apk-installing = Instalando: { $name } → { $target }

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
