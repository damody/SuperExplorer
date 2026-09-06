# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Opzioni cartella

dialog-about = Informazioni su SuperExplorer

dialog-version = Versione

dialog-build-date = Data di compilazione

dialog-author = Autore

dialog-purpose = Scopo: { $value }

dialog-author-line = Autore: { $name } — { $bio } · { $date }

dialog-community = Community: { $url }

dialog-plugin-safe-mode-title = Modalità provvisoria dei plug-in

dialog-plugin-safe-mode-body = La modalità provvisoria dei plug-in è attiva. Seleziona i plug-in da riabilitare, scegli Applica o OK, quindi riavvia SuperExplorer.

dialog-safe-mode-confirm-title = La modalità provvisoria richiede conferma

dialog-safe-mode-confirm-aria = Conferma modalità provvisoria richiesta; Pacchetto sospetto: { $package }

dialog-suspect-package = Pacchetto sospetto: { $package }

dialog-interface = Interfaccia: { $value }

dialog-operation = Operazione: { $value }

dialog-confirm-reenable = Conferma e riabilita

dialog-bookmark-action = Azione segnalibro

dialog-delete-bookmark = Elimina segnalibro

dialog-delete-bookmark-prompt = Eliminare il segnalibro “{ $name }”?

dialog-delete-bookmark-note = Questo rimuove il segnalibro. I file sul disco non vengono eliminati.

dialog-delete-bookmark-folder = Elimina cartella segnalibri

dialog-delete-bookmark-folder-prompt = Eliminare la cartella segnalibri “{ $name }”?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] La cartella e { $count } elemento al suo interno vengono rimossi. I file sul disco non vengono eliminati.
       *[other] La cartella e { $count } elementi al suo interno vengono rimossi. I file sul disco non vengono eliminati.
    }

dialog-rename-bookmark-folder = Rinomina cartella segnalibri

dialog-bookmark-library = Libreria

dialog-new-shortcut = Nuovo collegamento

dialog-new-remote-shortcut = Nuovo collegamento remoto

dialog-shortcut-name = Nome del collegamento

dialog-shortcut-target = Percorso di destinazione

dialog-create-remote-shortcut = Crea collegamento remoto

dialog-cancel-new-shortcut = Annulla nuovo collegamento

dialog-properties = { $name } - Proprietà

dialog-properties-aria = Proprietà di { $name }

dialog-general = Generale

dialog-file-type = Tipo: { $value }

dialog-location = Percorso: { $value }

dialog-size = Dimensione: { $value }

dialog-date-created = Data creazione: { $value }

dialog-date-modified = Data modifica: { $value }

dialog-permissions = Autorizzazioni: { $mode }

dialog-file-in-use = File in uso

dialog-items-in-use = Alcuni elementi sono in uso

dialog-lock-owner-body =
    { $count ->
        [one] Windows non può eliminare il { $count } elemento selezionato perché un’altra applicazione lo sta usando.
       *[other] Windows non può eliminare i { $count } elementi selezionati perché un’altra applicazione li sta usando.
    }

dialog-apps-using-file = Applicazioni che usano il file

dialog-close-results = Risultati della chiusura delle applicazioni

dialog-new-bookmark = Nuovo segnalibro

dialog-edit-bookmark = Modifica segnalibro

dialog-name-accelerator = Nome (N)

dialog-location-accelerator = Percorso (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Mostra editor durante il salvataggio (S)

dialog-lua-source = Origine Lua (solo current_folder di sola lettura)

dialog-folder-path-editable = Percorso cartella (modificabile)

dialog-file-path-editable = Percorso file (modificabile)

dialog-root = Radice

dialog-folder-options-scrollbar = Barra di scorrimento verticale di Opzioni cartella

dialog-new-bookmark-default = Nuovo segnalibro

dialog-new-folder-default = Nuova cartella

dialog-new-shortcut-default = Nuovo collegamento

dialog-shortcut-name-invalid = Il nome del collegamento deve essere un nome valido nella cartella corrente.

dialog-shortcut-target-invalid = Immettere un percorso di destinazione valido.

dialog-shortcut-window-closed = La finestra principale è stata chiusa, quindi il collegamento non è stato creato.

dialog-folder-changed = La cartella corrente è cambiata. Riapri la finestra Nuovo collegamento.

dialog-remote-unavailable = Il servizio remoto al momento non è disponibile.

dialog-shortcut-in-progress = È già in corso la creazione di un altro collegamento.

dialog-allowed = Consentito

dialog-not-allowed = Non consentito

dialog-read = Lettura

dialog-write = Scrittura

dialog-execute = Esecuzione

dialog-owner = Proprietario

dialog-group = Gruppo

dialog-others = Altri

dialog-remote-folder = Cartella remota

dialog-remote-file = File remoto

dialog-unavailable = Non disponibile

dialog-bytes = { $value } byte

dialog-extension-author-bio = Autore di SuperExplorer e delle estensioni di esempio ufficiali

dialog-extension-purpose-folder-size = Mostra le dimensioni dei file e totalizza ricorsivamente la dimensione delle cartelle in background.

dialog-extension-purpose-size-map = Mostra come mappa di aree quanto spazio occupa ogni elemento della cartella corrente.

dialog-extension-purpose-tokei = Conta le righe di codice in file o cartelle con Rust e tokei.

dialog-extension-purpose-lua-tokei = Estensione Lua di esempio che conta le righe di codice.

dialog-extension-purpose-lock-owner = Mostra il programma o il servizio che attualmente blocca un file.

dialog-extension-purpose-exif = Suggerisce rinomine in blocco dai dati EXIF di scatto delle foto.

dialog-extension-purpose-7z = Presenta gli archivi 7-Zip come cartelle virtuali esplorabili.

dialog-extension-purpose-bulk-folder = Crea molte cartelle in una volta da un modello specificato dall’utente.

dialog-extension-folder-size = Colonna dimensione cartella

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Righe di codice principali

dialog-extension-code-lines = Righe di codice

dialog-extension-lock-owner = Proprietario del blocco

dialog-extension-exif = Rinomina da EXIF

dialog-extension-7z = Cartella virtuale 7-Zip

dialog-extension-bulk-folder = Generatore di cartelle in blocco

dialog-git-hash = Hash Git
dialog-release-date-line = Data di rilascio: { $value }
dialog-reset = Reimposta
dialog-reset-prompt = Reimpostare { $label }? Viene rimosso solo lo stato persistente; i file attuali non cambiano.
dialog-reset-session-label = finestre e schede salvate
dialog-reset-view-label = impostazioni di visualizzazione salvate
dialog-reset-quick-access-label = elementi aggiunti di Accesso rapido
dialog-reset-all-label = tutto lo stato salvato di Explorer
dialog-permanent-delete-prompt =
    { $count ->
        [one] Eliminare definitivamente { $count } elemento? Questa azione non può essere annullata.
       *[other] Eliminare definitivamente { $count } elementi? Questa azione non può essere annullata.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Spostare { $count } elemento nel Cestino di Google Drive? Recupero su drive.google.com per 30 giorni. Non è il Cestino di Windows.
       *[other] Spostare { $count } elementi nel Cestino di Google Drive? Recupero su drive.google.com per 30 giorni. Non è il Cestino di Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Elimina definitivamente { $count } elemento
       *[other] Elimina definitivamente { $count } elementi
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Sposta { $count } elemento nel Cestino di Google Drive
       *[other] Sposta { $count } elementi nel Cestino di Google Drive
    }
