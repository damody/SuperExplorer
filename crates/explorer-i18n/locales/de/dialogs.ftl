# SuperExplorer dialogs: Folder Options, About, bookmarks, properties, lock owner, plugins.

dialogs-folder-options = Ordneroptionen

dialog-about = Info zu SuperExplorer

dialog-version = Version

dialog-build-date = Builddatum

dialog-author = Autor

dialog-purpose = Zweck: { $value }

dialog-author-line = Autor: { $name } — { $bio } · { $date }

dialog-community = Community: { $url }

dialog-plugin-safe-mode-title = Plugin-abgesicherter Modus

dialog-plugin-safe-mode-body = Der Plugin-abgesicherte Modus ist aktiv. Wählen Sie Plugins zum erneuten Aktivieren, klicken Sie auf Übernehmen oder OK und starten Sie SuperExplorer neu.

dialog-safe-mode-confirm-title = Der abgesicherte Modus erfordert eine Bestätigung

dialog-safe-mode-confirm-aria = Bestätigung des abgesicherten Modus erforderlich; Verdächtiges Paket: { $package }

dialog-suspect-package = Verdächtiges Paket: { $package }

dialog-interface = Schnittstelle: { $value }

dialog-operation = Vorgang: { $value }

dialog-confirm-reenable = Bestätigen und wieder aktivieren

dialog-bookmark-action = Lesezeichenaktion

dialog-delete-bookmark = Lesezeichen löschen

dialog-delete-bookmark-prompt = Lesezeichen „{ $name }“ löschen?

dialog-delete-bookmark-note = Dadurch wird das Lesezeichen entfernt. Dateien auf dem Datenträger werden nicht gelöscht.

dialog-delete-bookmark-folder = Lesezeichenordner löschen

dialog-delete-bookmark-folder-prompt = Lesezeichenordner „{ $name }“ löschen?

dialog-delete-bookmark-folder-note =
    { $count ->
        [one] Der Ordner und { $count } Element darin werden entfernt. Dateien auf dem Datenträger werden nicht gelöscht.
       *[other] Der Ordner und { $count } Elemente darin werden entfernt. Dateien auf dem Datenträger werden nicht gelöscht.
    }

dialog-rename-bookmark-folder = Lesezeichenordner umbenennen

dialog-bookmark-library = Bibliothek

dialog-new-shortcut = Neue Verknüpfung

dialog-new-remote-shortcut = Neue Remote-Verknüpfung

dialog-shortcut-name = Name der Verknüpfung

dialog-shortcut-target = Zielpfad

dialog-create-remote-shortcut = Remote-Verknüpfung erstellen

dialog-cancel-new-shortcut = Neue Verknüpfung abbrechen

dialog-properties = { $name } - Eigenschaften

dialog-properties-aria = Eigenschaften von { $name }

dialog-general = Allgemein

dialog-file-type = Typ: { $value }

dialog-location = Speicherort: { $value }

dialog-size = Größe: { $value }

dialog-date-created = Erstelldatum: { $value }

dialog-date-modified = Änderungsdatum: { $value }

dialog-permissions = Berechtigungen: { $mode }

dialog-file-in-use = Datei in Verwendung

dialog-items-in-use = Einige Elemente werden verwendet

dialog-lock-owner-body =
    { $count ->
        [one] Windows kann das { $count } ausgewählte Element nicht löschen, weil eine andere Anwendung es verwendet.
       *[other] Windows kann die { $count } ausgewählten Elemente nicht löschen, weil eine andere Anwendung sie verwendet.
    }

dialog-apps-using-file = Anwendungen, die die Datei verwenden

dialog-close-results = Ergebnisse beim Schließen von Anwendungen

dialog-new-bookmark = Neues Lesezeichen

dialog-edit-bookmark = Lesezeichen bearbeiten

dialog-name-accelerator = Name (N)

dialog-location-accelerator = Speicherort (L)

dialog-url-accelerator = URL (L)
dialog-path-accelerator = Path (U)
dialog-tags-accelerator = Tags (T)
dialog-tags-placeholder = Comma-separated tags
dialog-tags-hint = Use tags to search and organize bookmarks

dialog-show-editor-on-save = Editor beim Speichern anzeigen (S)

dialog-lua-source = Lua-Quellcode (nur schreibgeschütztes current_folder)

dialog-folder-path-editable = Ordnerpfad (bearbeitbar)

dialog-file-path-editable = Dateipfad (bearbeitbar)

dialog-root = Stamm

dialog-folder-options-scrollbar = Vertikale Bildlaufleiste der Ordneroptionen

dialog-new-bookmark-default = Neues Lesezeichen

dialog-new-folder-default = Neuer Ordner

dialog-new-shortcut-default = Neue Verknüpfung

dialog-shortcut-name-invalid = Der Name der Verknüpfung muss ein gültiger Name im aktuellen Ordner sein.

dialog-shortcut-target-invalid = Geben Sie einen gültigen Zielpfad ein.

dialog-shortcut-window-closed = Das Hauptfenster wurde geschlossen, daher konnte die Verknüpfung nicht erstellt werden.

dialog-folder-changed = Der aktuelle Ordner hat sich geändert. Öffnen Sie das Fenster „Neue Verknüpfung“ erneut.

dialog-remote-unavailable = Der Remotedienst ist derzeit nicht verfügbar.

dialog-shortcut-in-progress = Es wird bereits eine andere Verknüpfung erstellt.

dialog-allowed = Zulässig

dialog-not-allowed = Nicht zulässig

dialog-read = Lesen

dialog-write = Schreiben

dialog-execute = Ausführen

dialog-owner = Besitzer

dialog-group = Gruppe

dialog-others = Andere

dialog-remote-folder = Remoteordner

dialog-remote-file = Remotedatei

dialog-unavailable = Nicht verfügbar

dialog-bytes = { $value } Bytes

dialog-extension-author-bio = Autor von SuperExplorer und der offiziellen Beispielerweiterungen

dialog-extension-purpose-folder-size = Zeigt Dateigrößen an und summiert Ordnergrößen im Hintergrund rekursiv.

dialog-extension-purpose-size-map = Zeigt als Flächenkarte, wie viel Speicher jeder Eintrag im aktuellen Ordner belegt.

dialog-extension-purpose-tokei = Zählt Codezeilen in Dateien oder Ordnern mit Rust und tokei.

dialog-extension-purpose-lua-tokei = Beispiel-Lua-Erweiterung, die Codezeilen zählt.

dialog-extension-purpose-lock-owner = Zeigt das Programm oder den Dienst, der derzeit eine Datei sperrt.

dialog-extension-purpose-exif = Schlägt Stapelumbenennungen anhand von EXIF-Aufnahmedaten vor.

dialog-extension-purpose-7z = Stellt 7-Zip-Archive als durchsuchbare virtuelle Ordner dar.

dialog-extension-purpose-bulk-folder = Erstellt viele Ordner auf einmal anhand einer vom Benutzer angegebenen Vorlage.

dialog-extension-folder-size = Ordnergrößenspalte

dialog-extension-size-map = Size Map

dialog-extension-main-code-lines = Hauptcodezeilen

dialog-extension-code-lines = Codezeilen

dialog-extension-lock-owner = Sperrenbesitzer

dialog-extension-exif = Umbenennen anhand von EXIF

dialog-extension-7z = Virtueller 7-Zip-Ordner

dialog-extension-bulk-folder = Massen-Ordnergenerator

dialog-git-hash = Git-Hash
dialog-release-date-line = Veröffentlichungsdatum: { $value }
dialog-reset = Zurücksetzen
dialog-reset-prompt = { $label } zurücksetzen? Es wird nur der gespeicherte Zustand entfernt; aktuelle Dateien bleiben unverändert.
dialog-reset-session-label = gespeicherte Fenster und Registerkarten
dialog-reset-view-label = gespeicherte Ansichtseinstellungen
dialog-reset-quick-access-label = Schnellzugriff-Anheftungen
dialog-reset-all-label = gesamter gespeicherter Explorer-Zustand
dialog-permanent-delete-prompt =
    { $count ->
        [one] { $count } Element dauerhaft löschen? Diese Aktion kann nicht rückgängig gemacht werden.
       *[other] { $count } Elemente dauerhaft löschen? Diese Aktion kann nicht rückgängig gemacht werden.
    }
dialog-gdrive-trash-prompt =
    { $count ->
        [one] { $count } Element in den Google-Drive-Papierkorb verschieben? Wiederherstellung 30 Tage lang unter drive.google.com. Dies ist nicht der Windows-Papierkorb.
       *[other] { $count } Elemente in den Google-Drive-Papierkorb verschieben? Wiederherstellung 30 Tage lang unter drive.google.com. Dies ist nicht der Windows-Papierkorb.
    }
dialog-permanent-delete-aria =
    { $count ->
        [one] { $count } Element dauerhaft löschen
       *[other] { $count } Elemente dauerhaft löschen
    }
dialog-gdrive-trash-aria =
    { $count ->
        [one] { $count } Element in den Google-Drive-Papierkorb verschieben
       *[other] { $count } Elemente in den Google-Drive-Papierkorb verschieben
    }

ftp-sign-in = Bei { $host } anmelden
ftp-sign-in-unencrypted = Bei { $host } anmelden — Diese Verbindung ist nicht verschlüsselt
