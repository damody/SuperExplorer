# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Copier { $count } élément
       *[other] Copier { $count } éléments
    }

move-items =
    { $count ->
        [one] Déplacer { $count } élément
       *[other] Déplacer { $count } éléments
    }

recycle-items =
    { $count ->
        [one] Mettre { $count } élément à la Corbeille
       *[other] Mettre { $count } éléments à la Corbeille
    }

permanent-delete-items =
    { $count ->
        [one] Supprimer définitivement { $count } élément
       *[other] Supprimer définitivement { $count } éléments
    }

gdrive-trash-items =
    { $count ->
        [one] Déplacer { $count } élément vers la corbeille Google Drive
       *[other] Déplacer { $count } éléments vers la corbeille Google Drive
    }

shortcut-items =
    { $count ->
        [one] Créer un raccourci pour { $count } élément
       *[other] Créer un raccourci pour { $count } éléments
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } autre élément)
       *[other] { $first } ({ $count } autres éléments)
    }

clipboard-source = Source fournie par le Presse-papiers système

op-new-folder = Nouveau dossier | { $path }

op-new-file = Nouveau fichier | { $path }

op-rename = Renommer | { $from } → { $to }

op-chmod = Modifier les autorisations | { $path } → { $mode }

op-copy-route = { $action } | { $source } → { $destination }

op-move-route = { $action } | { $source } → { $destination }

op-recycle-route = { $action } | { $source }

op-permanent-delete-route = { $action } | { $source }

op-gdrive-trash-route = { $action } | { $source }

op-shortcut-route = { $action } | { $source }

op-preparing-copy = Préparation de la copie

op-copying = Copie

op-copy-complete = Copie terminée

op-preparing-move = Préparation du déplacement

op-moving = Déplacement

op-move-complete = Déplacement terminé

op-preparing = Préparation

op-processing = Traitement

op-complete = Terminé

op-finalizing = Finalisation

op-progress-items = { $summary } | { $phase } | Progression { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Progression { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Progression { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Terminé

op-cancelled = { $summary } | Annulé

op-failed = { $summary } | Échec : { $error }

op-partial = { $summary } | Partiel : { $succeeded }/{ $total } réussis

op-cancelling = { $summary } | Annulation

op-success-route = Réussi | { $route }

op-skipped-route = Ignoré | { $route }

op-cancelled-route = Annulé | { $route }

op-partial-status = Partiel

op-failed-status = Échec

op-error-code =  | Code d’erreur { $code }

op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = Destination non fournie

op-no-source = Source non fournie

apk-installing = Installation : { $name } → { $target }

apk-installed = Installé : { $name } → { $target }

apk-cancelled = Installation de { $name } sur { $target } annulée

apk-timeout = Délai d’installation de { $name } sur { $target } dépassé

apk-failed = Échec de l’installation de { $name } sur { $target } : { $error }

apk-check-device = Vérifiez la connexion de l’appareil et l’APK, puis réessayez
status-no-selection = Aucun élément sélectionné

status-details-unavailable = Impossible de charger les détails

status-item-count =
    { $count ->
        [one] { $count } élément
       *[other] { $count } éléments
    }

status-preview-select-one = Sélectionnez un élément à prévisualiser

status-preview-failed = Impossible de générer un aperçu de ce fichier

status-preview-loading = Chargement de l’aperçu…

status-preview-item-failed = Impossible de charger l’élément d’aperçu

status-preview-select-single = Sélectionnez un seul élément à prévisualiser

status-no-transfers = Aucune opération de fichier pendant cette session

status-thumbnail-cleared = Cache des miniatures vidé

status-thumbnail-clear-partial = Impossible de vider entièrement le cache des miniatures ; vous pouvez réessayer et la navigation fonctionne toujours

status-invalid-folder-name = Nom de dossier non valide. Corrigez-le et réessayez.

status-name-conflict = Un élément portant ce nom existe déjà.

status-context-menu-unresponsive = Le menu contextuel n’a pas répondu. Vous pouvez continuer à travailler.

status-cannot-go-forward = Impossible d’avancer

status-partial-folders = Certains dossiers n’ont pas pu être énumérés.

status-cannot-list-folder = Impossible d’énumérer le dossier.

status-cannot-cancel-disconnected = Impossible d’annuler : le service de fichiers n’est pas connecté.

status-cannot-cancel-operation = Impossible d’annuler l’opération de fichier : { $error }

status-cannot-load-folder = Impossible de charger le dossier. Réessayez.

status-drive-free-of = { $free } libres sur { $total }

status-no-media = Aucun média

status-disconnected = Déconnecté

status-access-denied = Accès refusé

status-capacity-unavailable = Capacité indisponible

status-waiting-file-count = En attente de File Count…

status-file-count-limit = Dépend de File Count, donc n’a pas démarré

status-file-count-over-limit = File Count a dépassé la limite, donc n’a pas démarré

status-file-count-pending-label = Limit

transfer-upload = Chargement vers la destination

transfer-download = Téléchargement depuis la source

transfer-conflict-inspection = Vérification des conflits de destination
transfer-local-copy = Copie locale
transfer-source-delete = Supprimer la source après le déplacement
transfer-provider-panic = Erreur du fournisseur de transfert
transfer-no-diagnostic = Aucune erreur sous-jacente fournie
transfer-cancelling = Annulation

transfer-cancel = Annuler

status-details-view = Affichage Détails
status-icon-view = Affichage Icônes
status-error = Erreur · 
status-loading = Chargement · 
status-items-selected = { $count } éléments — { $selected } sélectionné(s)
status-operation-progress = Opération { $completed }/{ $total } · { $name }
status-search-cancelled = Recherche annulée · 
status-search-error = Erreur de recherche · 
status-search-fallback = Recherche (secours système de fichiers ; index indisponible) · 
status-search-indexed = Recherche (index + secours) · 
status-search-partial = Résultats de recherche partiels · 
status-search-results = Résultats de recherche · 
status-lock-close-cancelled = La fermeture des applications a été annulée.
status-lock-discovery-timeout = La détection du propriétaire du verrou a atteint son délai.
status-lock-finding = Recherche des applications utilisant l’élément sélectionné…
status-lock-owners-found = { $count } application(s) utilisent l’élément sélectionné.
status-lock-partial-close = Certaines applications ne se sont pas fermées. Aucun processus n’a été arrêté de force.
status-lock-retry-limit = La limite de nouvelles tentatives a été atteinte. L’élément n’a pas été supprimé.
status-lock-retrying = Nouvel essai de la suppression…
status-lock-unidentified = Windows n’a pas pu identifier l’application utilisant cet élément.
transfer-cancel-operation = Annuler l’opération sur les fichiers
transfer-open-location = Ouvrir l’emplacement du transfert

status-generic-item = élément

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
