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
