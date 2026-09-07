# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Copia { $count } elemento
       *[other] Copia { $count } elementi
    }

move-items =
    { $count ->
        [one] Sposta { $count } elemento
       *[other] Sposta { $count } elementi
    }

recycle-items =
    { $count ->
        [one] Sposta { $count } elemento nel Cestino
       *[other] Sposta { $count } elementi nel Cestino
    }

permanent-delete-items =
    { $count ->
        [one] Elimina definitivamente { $count } elemento
       *[other] Elimina definitivamente { $count } elementi
    }

gdrive-trash-items =
    { $count ->
        [one] Sposta { $count } elemento nel Cestino di Google Drive
       *[other] Sposta { $count } elementi nel Cestino di Google Drive
    }

shortcut-items =
    { $count ->
        [one] Crea collegamento per { $count } elemento
       *[other] Crea collegamento per { $count } elementi
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } altro elemento)
       *[other] { $first } ({ $count } altri elementi)
    }

clipboard-source = Origine fornita dagli Appunti di sistema

op-new-folder = Nuova cartella | { $path }

op-new-file = Nuovo file | { $path }

op-rename = Rinomina | { $from } → { $to }

op-chmod = Cambia autorizzazioni | { $path } → { $mode }

op-copy-route = { $action } | { $source } → { $destination }

op-move-route = { $action } | { $source } → { $destination }

op-recycle-route = { $action } | { $source }

op-permanent-delete-route = { $action } | { $source }

op-gdrive-trash-route = { $action } | { $source }

op-shortcut-route = { $action } | { $source }

op-preparing-copy = Preparazione della copia

op-copying = Copia in corso

op-copy-complete = Copia completata

op-preparing-move = Preparazione dello spostamento

op-moving = Spostamento in corso

op-move-complete = Spostamento completato

op-preparing = Preparazione

op-processing = Elaborazione

op-complete = Completato

op-finalizing = Finalizzazione

op-progress-items = { $summary } | { $phase } | Avanzamento { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Avanzamento { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Avanzamento { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Completato

op-cancelled = { $summary } | Annullato

op-failed = { $summary } | Non riuscito: { $error }

op-partial = { $summary } | Parziale: { $succeeded }/{ $total } riusciti

op-cancelling = { $summary } | Annullamento

op-success-route = Riuscito | { $route }

op-skipped-route = Ignorato | { $route }

op-cancelled-route = Annullato | { $route }

op-partial-status = Parziale

op-failed-status = Non riuscito

op-error-code =  | Codice errore { $code }

op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = Destinazione non fornita

op-no-source = Origine non fornita

apk-installing = Installazione: { $name } → { $target }

apk-installed = Installato: { $name } → { $target }

apk-cancelled = Installazione di { $name } su { $target } annullata

apk-timeout = Timeout dell’installazione di { $name } su { $target }

apk-failed = Installazione di { $name } su { $target } non riuscita: { $error }

apk-check-device = Controlla la connessione del dispositivo e l’APK, quindi riprova
status-no-selection = Nessun elemento selezionato

status-details-unavailable = Impossibile caricare i dettagli

status-item-count =
    { $count ->
        [one] { $count } elemento
       *[other] { $count } elementi
    }

status-preview-select-one = Seleziona un elemento da visualizzare in anteprima

status-preview-failed = Impossibile generare un’anteprima di questo file

status-preview-loading = Caricamento anteprima…

status-preview-item-failed = Impossibile caricare l’elemento di anteprima

status-preview-select-single = Seleziona un solo elemento da visualizzare in anteprima

status-no-transfers = Nessuna operazione sui file in questa sessione

status-thumbnail-cleared = Cache anteprime svuotata

status-thumbnail-clear-partial = Impossibile svuotare completamente la cache delle anteprime; puoi riprovare e l’esplorazione continua a funzionare

status-invalid-folder-name = Nome cartella non valido. Correggilo e riprova.

status-name-conflict = Esiste già un elemento con quel nome.

status-context-menu-unresponsive = Il menu di scelta rapida non ha risposto. Puoi continuare a lavorare.

status-cannot-go-forward = Impossibile andare avanti

status-partial-folders = Impossibile elencare alcune cartelle.

status-cannot-list-folder = Impossibile elencare la cartella.

status-cannot-cancel-disconnected = Impossibile annullare: il servizio file non è connesso.

status-cannot-cancel-operation = Impossibile annullare l’operazione sui file: { $error }

status-cannot-load-folder = Impossibile caricare la cartella. Riprova.

status-drive-free-of = { $free } liberi di { $total }

status-no-media = Nessun supporto

status-disconnected = Disconnesso

status-access-denied = Accesso negato

status-capacity-unavailable = Capacità non disponibile

status-waiting-file-count = In attesa di File Count…

status-file-count-limit = Dipende da File Count, quindi non è stato avviato

status-file-count-over-limit = File Count ha superato il limite, quindi non è stato avviato

status-file-count-pending-label = Limit

transfer-upload = Caricamento destinazione

transfer-download = Download origine

transfer-conflict-inspection = Controllo conflitti di destinazione
transfer-local-copy = Copia locale
transfer-source-delete = Elimina origine dopo lo spostamento
transfer-provider-panic = Errore del provider di trasferimento
transfer-no-diagnostic = Nessun errore sottostante fornito
transfer-cancelling = Annullamento

transfer-cancel = Annulla

status-details-view = Visualizzazione Dettagli
status-icon-view = Visualizzazione Icone
status-error = Errore · 
status-loading = Caricamento · 
status-items-selected = { $count } elementi — { $selected } selezionati
status-operation-progress = Operazione { $completed }/{ $total } · { $name }
status-search-cancelled = Ricerca annullata · 
status-search-error = Errore di ricerca · 
status-search-fallback = Ricerca (fallback file system; indice non disponibile) · 
status-search-indexed = Ricerca (indice + fallback) · 
status-search-partial = Risultati di ricerca parziali · 
status-search-results = Risultati della ricerca · 
status-lock-close-cancelled = La chiusura delle applicazioni è stata annullata.
status-lock-discovery-timeout = Il rilevamento del proprietario del blocco ha raggiunto il termine.
status-lock-finding = Ricerca delle applicazioni che usano l’elemento selezionato…
status-lock-owners-found = { $count } applicazione/i sta usando l’elemento selezionato.
status-lock-partial-close = Alcune applicazioni non si sono chiuse. Nessun processo è stato terminato forzatamente.
status-lock-retry-limit = È stato raggiunto il limite di nuovi tentativi. L’elemento non è stato eliminato.
status-lock-retrying = Nuovo tentativo dell’eliminazione…
status-lock-unidentified = Windows non è riuscito a identificare l’applicazione che usa questo elemento.
transfer-cancel-operation = Annulla operazione sui file
transfer-open-location = Apri percorso del trasferimento

status-generic-item = elemento

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
