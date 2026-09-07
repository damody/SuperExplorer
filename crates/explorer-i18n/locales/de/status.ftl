# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] { $count } Element kopieren
       *[other] { $count } Elemente kopieren
    }

move-items =
    { $count ->
        [one] { $count } Element verschieben
       *[other] { $count } Elemente verschieben
    }

recycle-items =
    { $count ->
        [one] { $count } Element in den Papierkorb legen
       *[other] { $count } Elemente in den Papierkorb legen
    }

permanent-delete-items =
    { $count ->
        [one] { $count } Element endgültig löschen
       *[other] { $count } Elemente endgültig löschen
    }

gdrive-trash-items =
    { $count ->
        [one] { $count } Element in den Google-Drive-Papierkorb verschieben
       *[other] { $count } Elemente in den Google-Drive-Papierkorb verschieben
    }

shortcut-items =
    { $count ->
        [one] Verknüpfung für { $count } Element erstellen
       *[other] Verknüpfung für { $count } Elemente erstellen
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } weiteres Element)
       *[other] { $first } ({ $count } weitere Elemente)
    }

clipboard-source = Quelle von der Systemzwischenablage bereitgestellt

op-new-folder = Neuer Ordner | { $path }

op-new-file = Neue Datei | { $path }

op-rename = Umbenennen | { $from } → { $to }

op-chmod = Berechtigungen ändern | { $path } → { $mode }

op-copy-route = { $action } | { $source } → { $destination }

op-move-route = { $action } | { $source } → { $destination }

op-recycle-route = { $action } | { $source }

op-permanent-delete-route = { $action } | { $source }

op-gdrive-trash-route = { $action } | { $source }

op-shortcut-route = { $action } | { $source }

op-preparing-copy = Kopieren wird vorbereitet

op-copying = Kopieren

op-copy-complete = Kopieren abgeschlossen

op-preparing-move = Verschieben wird vorbereitet

op-moving = Verschieben

op-move-complete = Verschieben abgeschlossen

op-preparing = Vorbereiten

op-processing = In Bearbeitung

op-complete = Fertig

op-finalizing = Wird abgeschlossen

op-progress-items = { $summary } | { $phase } | Fortschritt { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Fortschritt { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Fortschritt { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Fertig

op-cancelled = { $summary } | Abgebrochen

op-failed = { $summary } | Fehler: { $error }

op-partial = { $summary } | Teilweise: { $succeeded }/{ $total } erfolgreich

op-cancelling = { $summary } | Wird abgebrochen

op-success-route = Erfolgreich | { $route }

op-skipped-route = Übersprungen | { $route }

op-cancelled-route = Abgebrochen | { $route }

op-partial-status = Teilweise

op-failed-status = Fehler

op-error-code =  | Fehlercode { $code }

op-speed-prefix =  | { $speed }
op-failure-detail = { $status } | { $route } | { $operation } | { $detail }{ $native }
op-no-destination = Ziel nicht angegeben

op-no-source = Quelle nicht angegeben

apk-installing = Installation: { $name } → { $target }

apk-installed = Installiert: { $name } → { $target }

apk-cancelled = Installation von { $name } auf { $target } abgebrochen

apk-timeout = Zeitüberschreitung bei der Installation von { $name } auf { $target }

apk-failed = Installation von { $name } auf { $target } fehlgeschlagen: { $error }

apk-check-device = Geräteverbindung und APK prüfen und erneut versuchen
status-no-selection = Keine Elemente ausgewählt

status-details-unavailable = Details konnten nicht geladen werden

status-item-count =
    { $count ->
        [one] { $count } Element
       *[other] { $count } Elemente
    }

status-preview-select-one = Wählen Sie ein Element für die Vorschau aus

status-preview-failed = Für diese Datei konnte keine Vorschau erstellt werden

status-preview-loading = Vorschau wird geladen…

status-preview-item-failed = Vorschauelement konnte nicht geladen werden

status-preview-select-single = Wählen Sie ein einzelnes Element für die Vorschau aus

status-no-transfers = Keine Dateivorgänge in dieser Sitzung

status-thumbnail-cleared = Miniaturansichtencache geleert

status-thumbnail-clear-partial = Miniaturansichtencache konnte nicht vollständig geleert werden; Wiederholen möglich, Durchsuchen funktioniert weiterhin

status-invalid-folder-name = Ungültiger Ordnername. Korrigieren und erneut versuchen.

status-name-conflict = Ein Element mit diesem Namen ist bereits vorhanden.

status-context-menu-unresponsive = Das Kontextmenü hat nicht reagiert. Sie können weiterarbeiten.

status-cannot-go-forward = Kein Vorwärtsgehen möglich

status-partial-folders = Einige Ordner konnten nicht aufgelistet werden.

status-cannot-list-folder = Der Ordner konnte nicht aufgelistet werden.

status-cannot-cancel-disconnected = Abbrechen nicht möglich: Der Dateidienst ist nicht verbunden.

status-cannot-cancel-operation = Dateivorgang konnte nicht abgebrochen werden: { $error }

status-cannot-load-folder = Der Ordner konnte nicht geladen werden. Versuchen Sie es erneut.

status-drive-free-of = { $free } frei von { $total }

status-no-media = Kein Medium

status-disconnected = Getrennt

status-access-denied = Zugriff verweigert

status-capacity-unavailable = Kapazität nicht verfügbar

status-waiting-file-count = Warten auf File Count…

status-file-count-limit = Hängt von File Count ab und wurde daher nicht gestartet

status-file-count-over-limit = File Count hat das Limit überschritten und wurde daher nicht gestartet

status-file-count-pending-label = Limit

transfer-upload = Zielupload

transfer-download = Quellen-Download

transfer-conflict-inspection = Zielkonfliktprüfung
transfer-local-copy = Lokales Kopieren
transfer-source-delete = Quelle nach dem Verschieben löschen
transfer-provider-panic = Übertragungsanbieterfehler
transfer-no-diagnostic = Kein zugrunde liegender Fehler angegeben
transfer-cancelling = Wird abgebrochen

transfer-cancel = Abbrechen

status-details-view = Detailansicht
status-icon-view = Symbolansicht
status-error = Fehler · 
status-loading = Wird geladen · 
status-items-selected = { $count } Elemente — { $selected } ausgewählt
status-operation-progress = Vorgang { $completed }/{ $total } · { $name }
status-search-cancelled = Suche abgebrochen · 
status-search-error = Suchfehler · 
status-search-fallback = Suche (Dateisystem-Fallback; Index nicht verfügbar) · 
status-search-indexed = Suche (Index + Fallback) · 
status-search-partial = Teilweise Suchergebnisse · 
status-search-results = Suchergebnisse · 
status-lock-close-cancelled = Das Schließen von Anwendungen wurde abgebrochen.
status-lock-discovery-timeout = Die Ermittlung der Sperrbesitzer hat das Zeitlimit erreicht.
status-lock-finding = Anwendungen werden gesucht, die das ausgewählte Element verwenden…
status-lock-owners-found = { $count } Anwendung(en) verwenden das ausgewählte Element.
status-lock-partial-close = Einige Anwendungen wurden nicht geschlossen. Es wurde kein Prozess zwangsbeendet.
status-lock-retry-limit = Das Wiederholungslimit wurde erreicht. Das Element wurde nicht gelöscht.
status-lock-retrying = Der Löschvorgang wird wiederholt…
status-lock-unidentified = Windows konnte die Anwendung, die dieses Element verwendet, nicht identifizieren.
transfer-cancel-operation = Dateioperation abbrechen
transfer-open-location = Übertragungsort öffnen

status-generic-item = Element

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
