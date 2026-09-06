# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Opções de pasta

dialog-about = Acerca do SuperExplorer

dialog-version = Versão

dialog-build-date = Data de compilação

dialog-author = Autor

dialog-purpose = Finalidade: { $value }

dialog-author-line = Autor: { $name } — { $bio } · { $date }

dialog-community = Comunidade: { $url }

dialog-plugin-safe-mode-title = Modo de segurança de plugins

dialog-plugin-safe-mode-body = O modo de segurança de plugins está ativado. Selecione os plugins a reativar, escolha Aplicar ou OK e reinicie o SuperExplorer.

dialog-safe-mode-confirm-title = O modo de segurança requer confirmação

dialog-safe-mode-confirm-aria = Confirmação do modo de segurança necessária; Pacote suspeito: { $package }

dialog-suspect-package = Pacote suspeito: { $package }

dialog-interface = Interface: { $value }

dialog-operation = Operação: { $value }

dialog-confirm-reenable = Confirmar e reativar

dialog-bookmark-action = Ação de marcador

dialog-delete-bookmark = Eliminar marcador

dialog-delete-bookmark-prompt = Eliminar o marcador “{ $name }”?

dialog-delete-bookmark-note = Isto remove o marcador. Os ficheiros no disco não são eliminados.

dialog-delete-bookmark-folder = Eliminar pasta de marcadores

dialog-delete-bookmark-folder-prompt = Eliminar a pasta de marcadores “{ $name }”?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] A pasta e { $count } item no interior são removidos. Os ficheiros no disco não são eliminados.
       *[other] A pasta e { $count } itens no interior são removidos. Os ficheiros no disco não são eliminados.
    }

dialog-rename-bookmark-folder = Mudar o nome da pasta de marcadores

dialog-bookmark-library = Biblioteca

dialog-new-shortcut = Novo atalho

dialog-new-remote-shortcut = Novo atalho remoto

dialog-shortcut-name = Nome do atalho

dialog-shortcut-target = Caminho de destino

dialog-create-remote-shortcut = Criar atalho remoto

dialog-cancel-new-shortcut = Cancelar novo atalho

dialog-properties = { $name } - Propriedades

dialog-properties-aria = Propriedades de { $name }

dialog-general = Geral

dialog-file-type = Tipo: { $value }

dialog-location = Localização: { $value }

dialog-size = Tamanho: { $value }

dialog-date-created = Data de criação: { $value }

dialog-date-modified = Data de modificação: { $value }

dialog-permissions = Permissões: { $mode }

dialog-file-in-use = Ficheiro em utilização

dialog-items-in-use = Alguns itens estão em utilização

dialog-lock-owner-body =
    { $count ->
        [one] O Windows não pode eliminar o { $count } item selecionado porque outra aplicação o está a utilizar.
       *[other] O Windows não pode eliminar os { $count } itens selecionados porque outra aplicação os está a utilizar.
    }

dialog-apps-using-file = Aplicações a utilizar o ficheiro

dialog-close-results = Resultados do fecho de aplicações

dialog-new-bookmark = Novo marcador

dialog-edit-bookmark = Editar marcador

dialog-name-accelerator = Nome (N)

dialog-location-accelerator = Localização (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Mostrar editor ao guardar (S)

dialog-lua-source = Código-fonte Lua (apenas current_folder só de leitura)

dialog-folder-path-editable = Caminho da pasta (editável)

dialog-file-path-editable = Caminho do ficheiro (editável)

dialog-root = Raiz

dialog-folder-options-scrollbar = Barra de deslocamento vertical de Opções de pasta

dialog-new-bookmark-default = Novo marcador

dialog-new-folder-default = Nova pasta

dialog-new-shortcut-default = Novo atalho

dialog-shortcut-name-invalid = O nome do atalho deve ser um nome válido na pasta atual.

dialog-shortcut-target-invalid = Introduza um caminho de destino válido.

dialog-shortcut-window-closed = A janela principal foi fechada, pelo que o atalho não pôde ser criado.

dialog-folder-changed = A pasta atual mudou. Volte a abrir a janela Novo atalho.

dialog-remote-unavailable = O serviço remoto está indisponível de momento.

dialog-shortcut-in-progress = Já está a ser criado outro atalho.

dialog-allowed = Permitido

dialog-not-allowed = Não permitido

dialog-read = Leitura

dialog-write = Escrita

dialog-execute = Execução

dialog-owner = Proprietário

dialog-group = Grupo

dialog-others = Outros

dialog-remote-folder = Pasta remota

dialog-remote-file = Ficheiro remoto

dialog-unavailable = Indisponível

dialog-bytes = { $value } bytes

dialog-extension-author-bio = Autor do SuperExplorer e das extensões de exemplo oficiais

dialog-extension-purpose-folder-size = Mostra tamanhos de ficheiro e totaliza recursivamente o tamanho da pasta em segundo plano.

dialog-extension-purpose-size-map = Mostra quanto espaço cada item da pasta atual ocupa como um mapa de áreas.

dialog-extension-purpose-tokei = Conta linhas de código em ficheiros ou pastas com Rust e tokei.

dialog-extension-purpose-lua-tokei = Extensão Lua de exemplo que conta linhas de código.

dialog-extension-purpose-lock-owner = Mostra o programa ou serviço que atualmente bloqueia um ficheiro.

dialog-extension-purpose-exif = Sugere alterações de nome em lote a partir de dados EXIF de captura de fotografias.

dialog-extension-purpose-7z = Apresenta arquivos 7-Zip como pastas virtuais navegáveis.

dialog-extension-purpose-bulk-folder = Cria muitas pastas de uma vez a partir de um modelo especificado pelo utilizador.

dialog-extension-folder-size = Coluna de tamanho da pasta

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Linhas de código principais

dialog-extension-code-lines = Linhas de código

dialog-extension-lock-owner = Proprietário do bloqueio

dialog-extension-exif = Mudar o nome a partir de EXIF

dialog-extension-7z = Pasta virtual 7-Zip

dialog-extension-bulk-folder = Gerador de pastas em lote
