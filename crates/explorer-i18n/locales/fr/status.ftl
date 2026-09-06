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

op-copy-route = Copier { $count } éléments | { $source } → { $destination }

op-move-route = Déplacer { $count } éléments | { $source } → { $destination }

op-recycle-route = Corbeille { $count } éléments | { $source }

op-permanent-delete-route = Supprimer définitivement { $count } éléments | { $source }

op-gdrive-trash-route = Corbeille Google Drive { $count } éléments | { $source }

op-shortcut-route = Créer un raccourci { $count } éléments | { $source }

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

op-no-destination = Destination non fournie

op-no-source = Source non fournie

apk-installing = Installation : { $name } → { $target }

apk-installed = Installé : { $name } → { $target }

apk-cancelled = Installation de { $name } sur { $target } annulée

apk-timeout = Délai d’installation de { $name } sur { $target } dépassé

apk-failed = Échec de l’installation de { $name } sur { $target } : { $error }

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

transfer-cancelling = Annulation

transfer-cancel = Annuler
