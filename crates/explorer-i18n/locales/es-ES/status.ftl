# SuperExplorer status, transfer, preview, and operation messages.

copy-items =
    { $count ->
        [one] Copiar { $count } elemento
       *[other] Copiar { $count } elementos
    }

move-items =
    { $count ->
        [one] Mover { $count } elemento
       *[other] Mover { $count } elementos
    }

recycle-items =
    { $count ->
        [one] Enviar { $count } elemento a la Papelera
       *[other] Enviar { $count } elementos a la Papelera
    }

permanent-delete-items =
    { $count ->
        [one] Eliminar definitivamente { $count } elemento
       *[other] Eliminar definitivamente { $count } elementos
    }

gdrive-trash-items =
    { $count ->
        [one] Mover { $count } elemento a la papelera de Google Drive
       *[other] Mover { $count } elementos a la papelera de Google Drive
    }

shortcut-items =
    { $count ->
        [one] Crear acceso directo para { $count } elemento
       *[other] Crear acceso directo para { $count } elementos
    }

extra-items =
    { $count ->
        [one] { $first } ({ $count } elemento más)
       *[other] { $first } ({ $count } elementos más)
    }

clipboard-source = Origen proporcionado por el Portapapeles del sistema

op-new-folder = Nueva carpeta | { $path }

op-new-file = Nuevo archivo | { $path }

op-rename = Cambiar nombre | { $from } → { $to }

op-chmod = Cambiar permisos | { $path } → { $mode }

op-copy-route = Copiar { $count } elementos | { $source } → { $destination }

op-move-route = Mover { $count } elementos | { $source } → { $destination }

op-recycle-route = Papelera { $count } elementos | { $source }

op-permanent-delete-route = Eliminar definitivamente { $count } elementos | { $source }

op-gdrive-trash-route = Papelera de Google Drive { $count } elementos | { $source }

op-shortcut-route = Crear acceso directo { $count } elementos | { $source }

op-preparing-copy = Preparando la copia

op-copying = Copiando

op-copy-complete = Copia completada

op-preparing-move = Preparando el traslado

op-moving = Moviendo

op-move-complete = Traslado completado

op-preparing = Preparando

op-processing = En curso

op-complete = Listo

op-finalizing = Finalizando

op-progress-items = { $summary } | { $phase } | Progreso { $completed }/{ $total }

op-progress-bytes = { $summary } | { $phase } { $percent }% ({ $bytes } / { $total-bytes }){ $speed } | Progreso { $completed }/{ $total }

op-progress-unknown = { $summary } | { $phase } { $bytes }{ $speed } | Progreso { $completed }/{ $total }

op-finished = { $phase } | { $summary }

op-done = { $summary } | Listo

op-cancelled = { $summary } | Cancelado

op-failed = { $summary } | Error: { $error }

op-partial = { $summary } | Parcial: { $succeeded }/{ $total } correctos

op-cancelling = { $summary } | Cancelando

op-success-route = Correcto | { $route }

op-skipped-route = Omitido | { $route }

op-cancelled-route = Cancelado | { $route }

op-partial-status = Parcial

op-failed-status = Error

op-error-code =  | Código de error { $code }

op-no-destination = Destino no proporcionado

op-no-source = Origen no proporcionado

apk-installing = Instalando: { $name } → { $target }

apk-installed = Instalado: { $name } → { $target }

apk-cancelled = Se canceló la instalación de { $name } en { $target }

apk-timeout = Se agotó el tiempo de instalación de { $name } en { $target }

apk-failed = Error al instalar { $name } en { $target }: { $error }

status-no-selection = No hay elementos seleccionados

status-details-unavailable = No se pudieron cargar los detalles

status-item-count =
    { $count ->
        [one] { $count } elemento
       *[other] { $count } elementos
    }

status-preview-select-one = Seleccione un elemento para obtener una vista previa

status-preview-failed = No se pudo generar una vista previa de este archivo

status-preview-loading = Cargando vista previa…

status-preview-item-failed = No se pudo cargar el elemento de vista previa

status-preview-select-single = Seleccione un único elemento para obtener una vista previa

status-no-transfers = No hay operaciones de archivo en esta sesión

status-thumbnail-cleared = Caché de miniaturas borrada

status-thumbnail-clear-partial = No se pudo borrar por completo la caché de miniaturas; puede reintentar y la exploración sigue funcionando

status-invalid-folder-name = Nombre de carpeta no válido. Corríjalo e inténtelo de nuevo.

status-name-conflict = Ya existe un elemento con ese nombre.

status-context-menu-unresponsive = El menú contextual no respondió. Puede seguir trabajando.

status-cannot-go-forward = No se puede avanzar

status-partial-folders = No se pudieron enumerar algunas carpetas.

status-cannot-list-folder = No se pudo enumerar la carpeta.

status-cannot-cancel-disconnected = No se puede cancelar: el servicio de archivos no está conectado.

status-cannot-cancel-operation = No se pudo cancelar la operación de archivo: { $error }

status-cannot-load-folder = No se pudo cargar la carpeta. Inténtelo de nuevo.

status-drive-free-of = { $free } libres de { $total }

status-no-media = No hay ningún medio

status-disconnected = Desconectado

status-access-denied = Acceso denegado

status-capacity-unavailable = Capacidad no disponible

status-waiting-file-count = Esperando File Count…

status-file-count-limit = Depende de File Count, por lo que no se inició

status-file-count-over-limit = File Count superó el límite, por lo que no se inició

status-file-count-pending-label = Limit

transfer-upload = Carga en destino

transfer-download = Descarga de origen

transfer-cancelling = Cancelando

transfer-cancel = Cancelar

status-details-view = Vista Detalles
status-icon-view = Vista Iconos
status-error = Error · 
status-loading = Cargando · 
status-items-selected = { $count } elementos — { $selected } seleccionados
status-operation-progress = Operación { $completed }/{ $total } · { $name }
status-search-cancelled = Búsqueda cancelada · 
status-search-error = Error de búsqueda · 
status-search-fallback = Buscando (sistema de archivos; índice no disponible) · 
status-search-indexed = Buscando (índice + reserva) · 
status-search-partial = Resultados de búsqueda parciales · 
status-search-results = Resultados de búsqueda · 
status-lock-close-cancelled = Se canceló el cierre de las aplicaciones.
status-lock-discovery-timeout = La detección del propietario del bloqueo alcanzó su límite de tiempo.
status-lock-finding = Buscando aplicaciones que usan el elemento seleccionado…
status-lock-owners-found = { $count } aplicación(es) están usando el elemento seleccionado.
status-lock-partial-close = Algunas aplicaciones no se cerraron. No se forzó la finalización de ningún proceso.
status-lock-retry-limit = Se alcanzó el límite de reintentos. El elemento no se eliminó.
status-lock-retrying = Reintentando la operación de eliminación…
status-lock-unidentified = Windows no pudo identificar la aplicación que usa este elemento.
transfer-cancel-operation = Cancelar operación de archivo
transfer-open-location = Abrir ubicación de la transferencia

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
