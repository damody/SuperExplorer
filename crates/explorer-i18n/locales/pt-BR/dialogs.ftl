# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Opções de pasta

dialog-about = Sobre o SuperExplorer

dialog-version = Versão

dialog-build-date = Data da compilação

dialog-author = Autor

dialog-purpose = Finalidade: { $value }

dialog-author-line = Autor: { $name } — { $bio } · { $date }

dialog-community = Comunidade: { $url }

dialog-plugin-safe-mode-title = Modo de segurança de plug-ins

dialog-plugin-safe-mode-body = O modo de segurança de plug-ins está ativado. Selecione os plug-ins para reativar, escolha Aplicar ou OK e reinicie o SuperExplorer.

dialog-safe-mode-confirm-title = O modo de segurança exige confirmação

dialog-safe-mode-confirm-aria = Confirmação do modo de segurança necessária; Pacote suspeito: { $package }

dialog-suspect-package = Pacote suspeito: { $package }

dialog-interface = Interface: { $value }

dialog-operation = Operação: { $value }

dialog-confirm-reenable = Confirmar e reativar

dialog-bookmark-action = Ação de favorito

dialog-delete-bookmark = Excluir favorito

dialog-delete-bookmark-prompt = Excluir o favorito “{ $name }”?

dialog-delete-bookmark-note = Isso remove o favorito. Os arquivos no disco não são excluídos.

dialog-delete-bookmark-folder = Excluir pasta de favoritos

dialog-delete-bookmark-folder-prompt = Excluir a pasta de favoritos “{ $name }”?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] A pasta e { $count } item dentro dela serão removidos. Os arquivos no disco não são excluídos.
       *[other] A pasta e { $count } itens dentro dela serão removidos. Os arquivos no disco não são excluídos.
    }

dialog-rename-bookmark-folder = Renomear pasta de favoritos

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

dialog-location = Local: { $value }

dialog-size = Tamanho: { $value }

dialog-date-created = Data de criação: { $value }

dialog-date-modified = Data de modificação: { $value }

dialog-permissions = Permissões: { $mode }

dialog-file-in-use = Arquivo em uso

dialog-items-in-use = Alguns itens estão em uso

dialog-lock-owner-body =
    { $count ->
        [one] O Windows não pode excluir o { $count } item selecionado porque outro aplicativo o está usando.
       *[other] O Windows não pode excluir os { $count } itens selecionados porque outro aplicativo os está usando.
    }

dialog-apps-using-file = Aplicativos usando o arquivo

dialog-close-results = Resultados do fechamento de aplicativos

dialog-new-bookmark = Novo favorito

dialog-edit-bookmark = Editar favorito

dialog-name-accelerator = Nome (N)

dialog-location-accelerator = Local (L)

dialog-url-accelerator = URL (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = Mostrar editor ao salvar (S)

dialog-lua-source = Código-fonte Lua (somente current_folder somente leitura)

dialog-folder-path-editable = Caminho da pasta (editável)

dialog-file-path-editable = Caminho do arquivo (editável)

dialog-root = Raiz

dialog-folder-options-scrollbar = Barra de rolagem vertical de Opções de pasta

dialog-new-bookmark-default = Novo favorito

dialog-new-folder-default = Nova pasta

dialog-new-shortcut-default = Novo atalho

dialog-shortcut-name-invalid = O nome do atalho deve ser um nome válido na pasta atual.

dialog-shortcut-target-invalid = Insira um caminho de destino válido.

dialog-shortcut-window-closed = A janela principal foi fechada, então o atalho não pôde ser criado.

dialog-folder-changed = A pasta atual mudou. Reabra a janela Novo atalho.

dialog-remote-unavailable = O serviço remoto está indisponível no momento.

dialog-shortcut-in-progress = Outro atalho já está sendo criado.

dialog-allowed = Permitido

dialog-not-allowed = Não permitido

dialog-read = Leitura

dialog-write = Gravação

dialog-execute = Execução

dialog-owner = Proprietário

dialog-group = Grupo

dialog-others = Outros

dialog-remote-folder = Pasta remota

dialog-remote-file = Arquivo remoto

dialog-unavailable = Indisponível

dialog-bytes = { $value } bytes

dialog-extension-author-bio = Autor do SuperExplorer e das extensões de exemplo oficiais

dialog-extension-purpose-folder-size = Mostra tamanhos de arquivo e totaliza recursivamente o tamanho da pasta em segundo plano.

dialog-extension-purpose-size-map = Mostra quanto espaço cada item da pasta atual ocupa como um mapa de áreas.

dialog-extension-purpose-tokei = Conta linhas de código em arquivos ou pastas com Rust e tokei.

dialog-extension-purpose-lua-tokei = Extensão Lua de exemplo que conta linhas de código.

dialog-extension-purpose-lock-owner = Mostra o programa ou serviço que atualmente bloqueia um arquivo.

dialog-extension-purpose-exif = Sugere renomeações em lote a partir de dados EXIF de captura de fotos.

dialog-extension-purpose-7z = Apresenta arquivos 7-Zip como pastas virtuais navegáveis.

dialog-extension-purpose-bulk-folder = Cria muitas pastas de uma vez a partir de um modelo especificado pelo usuário.

dialog-extension-folder-size = Coluna de tamanho da pasta

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Linhas de código principais

dialog-extension-code-lines = Linhas de código

dialog-extension-lock-owner = Proprietário do bloqueio

dialog-extension-exif = Renomear a partir de EXIF

dialog-extension-7z = Pasta virtual 7-Zip

dialog-extension-bulk-folder = Gerador de pastas em lote

dialog-git-hash = Hash Git
dialog-release-date-line = Data de lançamento: { $value }
dialog-reset = Redefinir
dialog-reset-prompt = Redefinir { $label }? Isso remove apenas o estado persistido; os arquivos atuais não mudam.
dialog-reset-session-label = janelas e guias salvas
dialog-reset-view-label = configurações de exibição salvas
dialog-reset-quick-access-label = fixações do Acesso rápido
dialog-reset-all-label = todo o estado salvo do Explorer
dialog-permanent-delete-prompt =
    { $count ->
        [one] Excluir permanentemente { $count } item? Esta ação não pode ser desfeita.
       *[other] Excluir permanentemente { $count } itens? Esta ação não pode ser desfeita.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Mover { $count } item para a lixeira do Google Drive? É possível recuperá-los em drive.google.com por 30 dias. Isto não é a Lixeira do Windows.
       *[other] Mover { $count } itens para a lixeira do Google Drive? É possível recuperá-los em drive.google.com por 30 dias. Isto não é a Lixeira do Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Excluir permanentemente { $count } item
       *[other] Excluir permanentemente { $count } itens
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Mover { $count } item para a lixeira do Google Drive
       *[other] Mover { $count } itens para a lixeira do Google Drive
    }

ftp-sign-in = Entrar em { $host }
ftp-sign-in-unencrypted = Entrar em { $host } — Esta conexão não está criptografada
