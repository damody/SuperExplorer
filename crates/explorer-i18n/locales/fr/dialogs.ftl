# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Options des dossiers

dialog-about = À propos de SuperExplorer

dialog-version = Version

dialog-build-date = Date de compilation

dialog-author = Auteur

dialog-purpose = Objectif : { $value }

dialog-author-line = Auteur : { $name } — { $bio } · { $date }

dialog-community = Communauté : { $url }

dialog-plugin-safe-mode-title = Mode sans échec des plug-ins

dialog-plugin-safe-mode-body = Le mode sans échec des plug-ins est activé. Sélectionnez les plug-ins à réactiver, choisissez Appliquer ou OK, puis redémarrez SuperExplorer.

dialog-safe-mode-confirm-title = Le mode sans échec nécessite une confirmation

dialog-safe-mode-confirm-aria = Confirmation du mode sans échec requise ; Package suspect : { $package }

dialog-suspect-package = Package suspect : { $package }

dialog-interface = Interface : { $value }

dialog-operation = Opération : { $value }

dialog-confirm-reenable = Confirmer et réactiver

dialog-bookmark-action = Action du signet

dialog-delete-bookmark = Supprimer le signet

dialog-delete-bookmark-prompt = Supprimer le signet « { $name } » ?

dialog-delete-bookmark-note = Cela supprime le signet. Les fichiers sur le disque ne sont pas supprimés.

dialog-delete-bookmark-folder = Supprimer le dossier de signets

dialog-delete-bookmark-folder-prompt = Supprimer le dossier de signets « { $name } » ?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Le dossier et { $count } élément à l’intérieur sont supprimés. Les fichiers sur le disque ne sont pas supprimés.
       *[other] Le dossier et { $count } éléments à l’intérieur sont supprimés. Les fichiers sur le disque ne sont pas supprimés.
    }

dialog-rename-bookmark-folder = Renommer le dossier de signets

dialog-bookmark-library = Bibliothèque

dialog-new-shortcut = Nouveau raccourci

dialog-new-remote-shortcut = Nouveau raccourci distant

dialog-shortcut-name = Nom du raccourci

dialog-shortcut-target = Chemin cible

dialog-create-remote-shortcut = Créer un raccourci distant

dialog-cancel-new-shortcut = Annuler le nouveau raccourci

dialog-properties = { $name } - Propriétés

dialog-properties-aria = Propriétés de { $name }

dialog-general = Général

dialog-file-type = Type : { $value }

dialog-location = Emplacement : { $value }

dialog-size = Taille : { $value }

dialog-date-created = Date de création : { $value }

dialog-date-modified = Date de modification : { $value }

dialog-permissions = Autorisations : { $mode }

dialog-file-in-use = Fichier en cours d’utilisation

dialog-items-in-use = Certains éléments sont en cours d’utilisation

dialog-lock-owner-body =
    { $count ->
        [one] Windows ne peut pas supprimer le { $count } élément sélectionné, car une autre application l’utilise.
       *[other] Windows ne peut pas supprimer les { $count } éléments sélectionnés, car une autre application les utilise.
    }

dialog-apps-using-file = Applications utilisant le fichier

dialog-close-results = Résultats de la fermeture des applications

dialog-new-bookmark = Nouveau signet

dialog-edit-bookmark = Modifier le signet

dialog-name-accelerator = Nom (N)

dialog-location-accelerator = Emplacement (L)

dialog-url-accelerator = URL (L)

dialog-show-editor-on-save = Afficher l’éditeur lors de l’enregistrement (S)

dialog-lua-source = Source Lua (current_folder en lecture seule uniquement)

dialog-folder-path-editable = Chemin de dossier (modifiable)

dialog-file-path-editable = Chemin de fichier (modifiable)

dialog-root = Racine

dialog-folder-options-scrollbar = Barre de défilement verticale des Options des dossiers

dialog-new-bookmark-default = Nouveau signet

dialog-new-folder-default = Nouveau dossier

dialog-new-shortcut-default = Nouveau raccourci

dialog-shortcut-name-invalid = Le nom du raccourci doit être un nom valide dans le dossier actuel.

dialog-shortcut-target-invalid = Entrez un chemin cible valide.

dialog-shortcut-window-closed = La fenêtre principale s’est fermée, le raccourci n’a donc pas pu être créé.

dialog-folder-changed = Le dossier actuel a changé. Rouvrez la fenêtre Nouveau raccourci.

dialog-remote-unavailable = Le service distant est actuellement indisponible.

dialog-shortcut-in-progress = Un autre raccourci est déjà en cours de création.

dialog-allowed = Autorisé

dialog-not-allowed = Non autorisé

dialog-read = Lecture

dialog-write = Écriture

dialog-execute = Exécution

dialog-owner = Propriétaire

dialog-group = Groupe

dialog-others = Autres

dialog-remote-folder = Dossier distant

dialog-remote-file = Fichier distant

dialog-unavailable = Indisponible

dialog-bytes = { $value } octets

dialog-extension-author-bio = Auteur de SuperExplorer et des extensions d’exemple officielles

dialog-extension-purpose-folder-size = Affiche les tailles de fichiers et totalise récursivement la taille des dossiers en arrière-plan.

dialog-extension-purpose-size-map = Affiche sous forme de carte de surfaces l’espace occupé par chaque élément du dossier actuel.

dialog-extension-purpose-tokei = Compte les lignes de code dans des fichiers ou des dossiers avec Rust et tokei.

dialog-extension-purpose-lua-tokei = Exemple d’extension Lua qui compte les lignes de code.

dialog-extension-purpose-lock-owner = Affiche le programme ou le service qui verrouille actuellement un fichier.

dialog-extension-purpose-exif = Propose des renommages par lots d’après les données EXIF de prise de vue.

dialog-extension-purpose-7z = Présente les archives 7-Zip comme des dossiers virtuels parcourables.

dialog-extension-purpose-bulk-folder = Crée de nombreux dossiers à la fois à partir d’un modèle spécifié par l’utilisateur.

dialog-extension-folder-size = Colonne de taille de dossier

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Lignes de code principales

dialog-extension-code-lines = Lignes de code

dialog-extension-lock-owner = Propriétaire du verrou

dialog-extension-exif = Renommer d’après EXIF

dialog-extension-7z = Dossier virtuel 7-Zip

dialog-extension-bulk-folder = Générateur de dossiers en bloc

dialog-git-hash = Hash Git
dialog-release-date-line = Date de publication : { $value }
dialog-reset = Réinitialiser
dialog-reset-prompt = Réinitialiser { $label } ? Seul l’état enregistré est supprimé ; les fichiers actuels ne changent pas.
dialog-reset-session-label = fenêtres et onglets enregistrés
dialog-reset-view-label = paramètres d’affichage enregistrés
dialog-reset-quick-access-label = épingles de l’Accès rapide
dialog-reset-all-label = tout l’état Explorer enregistré
dialog-permanent-delete-prompt =
    { $count ->
        [one] Supprimer définitivement { $count } élément ? Cette action est irréversible.
       *[other] Supprimer définitivement { $count } éléments ? Cette action est irréversible.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] Déplacer { $count } élément vers la corbeille Google Drive ? Récupération possible sur drive.google.com pendant 30 jours. Ce n’est pas la Corbeille Windows.
       *[other] Déplacer { $count } éléments vers la corbeille Google Drive ? Récupération possible sur drive.google.com pendant 30 jours. Ce n’est pas la Corbeille Windows.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] Supprimer définitivement { $count } élément
       *[other] Supprimer définitivement { $count } éléments
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] Déplacer { $count } élément vers la corbeille Google Drive
       *[other] Déplacer { $count } éléments vers la corbeille Google Drive
    }
